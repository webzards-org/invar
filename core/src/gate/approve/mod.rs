//! `invar plan approve` (M1-9): the human gate.
//!
//! The one point that records that a person read the Plan and accepted it.
//! Every later refusal — `plan check`, `plan guard`, `done` — means something
//! only because of it, so it is strict where it can be:
//!
//! - **Only a valid Plan is approved.** `plan validate` runs first; a Plan it
//!   rejects is refused with the same list of problems.
//! - **What is approved is what was shown.** The caller prints the acceptance
//!   criteria between [`prepare`] and [`Pending::commit`], and the commit
//!   refuses if the file changed in between.
//! - **Only four values change** (plan-header.md §5.3): `Status`,
//!   `Approved-by`, `Approved-at`, `Approved-checksum`. The write is checked
//!   against the file read back: the body must be byte-identical.
//! - **A changed approval is never silently renewed** (D-12, resolving OQ-1).
//!   An `approved` Plan whose body no longer matches its checksum — or whose
//!   approval values are missing — is refused unless `--reapprove` is given.
//! - **A new approval needs numbered criteria** (D-16). `plan validate` lets an
//!   approval from before ids existed stand; it does not let one be issued.

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use crate::diagnostics::Diagnostics;
use crate::format::plan::{Approval, Mode, Plan, Status};
use crate::fs::ProjectRoot;

use super::approval::{self, Prior, classify};
use super::validate::{self, Report};

#[cfg(test)]
mod tests;

/// What `plan approve` will do.
#[derive(Debug)]
pub enum Decision {
    /// Approve: print the acceptance criteria, then [`Pending::commit`].
    Approve(Box<Pending>),
    /// Already approved, and the body is unchanged: nothing to write.
    AlreadyApproved {
        spec: PathBuf,
        by: String,
        at: String,
    },
}

/// An approval that has been checked and not yet written.
#[derive(Debug)]
pub struct Pending {
    spec: PathBuf,
    mode: Mode,
    /// The file exactly as it was read; the commit refuses if it changed.
    original: String,
    plan: Plan,
    checksum: String,
    /// The earlier approval being replaced, under `--reapprove`.
    replaces: Option<Prior>,
}

/// Check `spec` for approval.
///
/// `reapprove` allows replacing an approval whose body changed or whose
/// values are missing (D-12). It is not needed for a draft, and an approval
/// that is still current is left alone with or without it.
pub fn prepare(
    root: &ProjectRoot,
    spec: &Path,
    mode: Mode,
    reapprove: bool,
) -> Result<Decision, ApproveError> {
    let report = validate::file(root, spec, mode);
    if !report.is_valid() {
        return Err(ApproveError::Invalid(report));
    }
    let original = read(spec)?;
    // Validation just passed; this fails only if the file changed since.
    let plan = Plan::parse(&original, mode).map_err(|problems| {
        ApproveError::Invalid(Report {
            spec: spec.to_owned(),
            status: None,
            problems,
        })
    })?;
    let checksum = approval::body_digest(&plan);

    let replaces = match classify(plan.header(), &checksum) {
        Prior::Draft => None,
        Prior::Current { by, at } => {
            return Ok(Decision::AlreadyApproved {
                spec: spec.to_owned(),
                by,
                at,
            });
        }
        prior @ (Prior::Changed { .. } | Prior::Unrecorded { .. }) => {
            if !reapprove {
                return Err(ApproveError::NeedsReapprove {
                    spec: spec.to_owned(),
                    prior,
                    current: checksum,
                });
            }
            let problems = Plan::problems_as_draft(&original, mode);
            if !problems.is_empty() {
                return Err(ApproveError::Unnumbered(Report {
                    spec: spec.to_owned(),
                    status: Some(Status::Approved),
                    problems,
                }));
            }
            Some(prior)
        }
    };

    Ok(Decision::Approve(Box::new(Pending {
        spec: spec.to_owned(),
        mode,
        original,
        plan,
        checksum,
        replaces,
    })))
}

impl Pending {
    pub fn spec(&self) -> &Path {
        &self.spec
    }

    pub fn ticket(&self) -> &str {
        &self.plan.header().ticket
    }

    /// The approval being replaced, under `--reapprove`.
    pub fn replaces(&self) -> Option<&Prior> {
        self.replaces.as_ref()
    }

    /// `## Acceptance criteria` without the blank lines around it — what the
    /// person approving must see before the approval is written.
    pub fn acceptance_criteria(&self) -> &str {
        self.plan
            .acceptance_criteria()
            .unwrap_or("")
            .trim_matches(['\n', '\r'])
    }

    /// Check that `by` and `at` can be written into the header, before the
    /// criteria are shown. [`commit`](Self::commit) makes the same check, but
    /// by then a person has read the criteria of an approval that was never
    /// going to be recorded.
    pub fn check_values(&self, by: &str, at: &str) -> Result<(), ApproveError> {
        Approval {
            by: by.to_owned(),
            at: at.to_owned(),
            checksum: self.checksum.clone(),
        }
        .check()
        .map_err(ApproveError::Unwritable)
    }

    /// Write the approval: `by` from `git config user.name`, `at` as
    /// `YYYY-MM-DD`. Refuses if the file changed since [`prepare`] read it,
    /// and verifies the result by reading it back.
    pub fn commit(mut self, by: String, at: String) -> Result<Approval, ApproveError> {
        if read(&self.spec)? != self.original {
            return Err(ApproveError::ChangedMeanwhile { spec: self.spec });
        }
        let body = self.plan.body().to_owned();
        let approval = Approval {
            by,
            at,
            checksum: self.checksum.clone(),
        };
        self.plan
            .approve(approval.clone())
            .map_err(ApproveError::Unwritable)?;
        let expected = self.plan.to_text();
        self.plan
            .write(&self.spec)
            .map_err(|source| ApproveError::Write {
                spec: self.spec.clone(),
                source,
            })?;

        // Read back: the file must be what was meant, and its body untouched.
        let written = read(&self.spec)?;
        let preserved = written == expected
            && Plan::parse(&written, self.mode).is_ok_and(|reread| {
                reread.body() == body
                    && reread.header().status == Status::Approved
                    && reread.header().approved_checksum.as_deref() == Some(&self.checksum)
            });
        if !preserved {
            return Err(ApproveError::NotPreserved { spec: self.spec });
        }
        Ok(approval)
    }
}

fn read(spec: &Path) -> Result<String, ApproveError> {
    let bytes = std::fs::read(spec).map_err(|source| ApproveError::Read {
        spec: spec.to_owned(),
        source,
    })?;
    String::from_utf8(bytes).map_err(|err| {
        let mut problems = Diagnostics::new();
        problems.general(format!(
            "not valid UTF-8 (first invalid byte at offset {}); save the Plan as UTF-8",
            err.utf8_error().valid_up_to()
        ));
        ApproveError::Invalid(Report {
            spec: spec.to_owned(),
            status: None,
            problems,
        })
    })
}

// --- errors -----------------------------------------------------------------

#[derive(Debug)]
pub enum ApproveError {
    /// `plan validate` does not pass. The report says why.
    Invalid(Report),
    Read {
        spec: PathBuf,
        source: io::Error,
    },
    /// Already approved, and the approval no longer holds: `--reapprove`
    /// is required (D-12).
    NeedsReapprove {
        spec: PathBuf,
        prior: Prior,
        current: String,
    },
    /// `--reapprove` on a Plan whose criteria have no ids (D-16). `plan validate`
    /// accepts the Plan, because an earlier approval stands; a new one does not
    /// inherit that. The report says what to fix.
    Unnumbered(Report),
    /// The file changed between printing the criteria and writing.
    ChangedMeanwhile {
        spec: PathBuf,
    },
    /// The approval values cannot be written into a header line.
    Unwritable(String),
    Write {
        spec: PathBuf,
        source: io::Error,
    },
    /// The file read back is not what was written. Never expected.
    NotPreserved {
        spec: PathBuf,
    },
}

impl fmt::Display for ApproveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(report) => {
                let count = report.problems.len();
                let noun = if count == 1 { "problem" } else { "problems" };
                write!(
                    f,
                    "the Plan is not valid ({count} {noun}), so it cannot be approved; \
                     fix every problem listed above — `invar plan validate` checks them — \
                     then approve again"
                )
            }
            Self::Unnumbered(report) => write!(
                f,
                "{}: criteria with no ids are valid only under the approval that accepted \
                 them, and this would be a new one (plan-header.md D-16). Give every \
                 criterion its id, as listed above, then approve again with `--reapprove`",
                report.spec.display()
            ),
            Self::Read { spec, source } => write!(f, "cannot read {}: {source}", spec.display()),
            Self::NeedsReapprove {
                spec,
                prior,
                current,
            } => {
                write!(f, "{}: ", spec.display())?;
                match prior {
                    Prior::Changed { by, at, recorded } => write!(
                        f,
                        "the body changed after {by} approved it on {at} (approved \
                         {recorded}, now {current}). An approval covers the text that was \
                         read, so it is not renewed silently: read the Plan again, then \
                         approve it with `--reapprove`"
                    ),
                    Prior::Unrecorded { missing } => write!(
                        f,
                        "the header says `Status: approved`, but {} {} empty, so no \
                         `invar plan approve` recorded this approval (plan-header.md §5.1). \
                         Read the Plan, then approve it with `--reapprove` to record a \
                         real approval",
                        missing
                            .iter()
                            .map(|key| format!("`{key}`"))
                            .collect::<Vec<_>>()
                            .join(" and "),
                        if missing.len() == 1 { "is" } else { "are" }
                    ),
                    // `prepare` never builds these with a draft or a current
                    // approval.
                    Prior::Draft | Prior::Current { .. } => {
                        write!(f, "cannot be approved in its current state")
                    }
                }
            }
            Self::ChangedMeanwhile { spec } => write!(
                f,
                "{} changed while it was being approved, so the criteria shown may not \
                 be the ones in the file; nothing was written — approve again",
                spec.display()
            ),
            Self::Unwritable(reason) => write!(f, "cannot record the approval: {reason}"),
            Self::Write { spec, source } => {
                write!(f, "cannot write {}: {source}", spec.display())
            }
            Self::NotPreserved { spec } => write!(
                f,
                "{} does not read back as written; check the file before using it — \
                 this is a bug in invar, please report it",
                spec.display()
            ),
        }
    }
}

impl std::error::Error for ApproveError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Read { source, .. } | Self::Write { source, .. } => Some(source),
            _ => None,
        }
    }
}
