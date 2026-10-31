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
//! - **Only five values change** (plan-header.md §5.3): `Status`,
//!   `Approved-by`, `Approved-at`, `Approved-checksum`, `Acceptance`. The write
//!   is checked against the file read back: the body must be byte-identical.
//! - **A changed approval is never silently renewed** (D-12, resolving OQ-1).
//!   An `approved` Plan that no longer matches its checksum — or whose approval
//!   values are missing — is refused unless `--reapprove` is given.
//! - **A new approval needs numbered criteria** (D-16). `plan validate` lets an
//!   approval from before ids existed stand; it does not let one be issued.

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use crate::diagnostics::Diagnostics;
use crate::format::plan::{Acceptance, Approval, Mode, Plan, Status};
use crate::fs::ProjectRoot;

use super::approval::{self, ApprovedValue, Prior, classify};
use super::validate::{self, Report};

#[cfg(test)]
mod tests;

/// The value `Acceptance` will get, and where it came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chosen {
    pub value: Acceptance,
    source: Source,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Source {
    Flag,
    EarlierApproval,
    UnconfirmedHeader,
    /// With the value in the header, if any, that the approval did not cover.
    Default(Option<Acceptance>),
}

/// What the approval being replaced gives `Acceptance` to go on (D-20).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Earlier {
    /// Nothing to keep: a draft, an approval no command recorded, or a header
    /// with no value.
    Nothing,
    /// The digest shows the approval had this value.
    Confirmed(Acceptance),
    /// The body changed too, so the digest cannot say; this is what the header
    /// holds.
    Unconfirmed(Acceptance),
    /// The digest shows the approval had no value, and the header holds this.
    NotApproved(Acceptance),
}

/// The flag that names `value`.
fn flag_for(value: Acceptance) -> &'static str {
    match value {
        Acceptance::Required => "--acceptance",
        Acceptance::Skipped => "--no-acceptance",
    }
}

impl fmt::Display for Chosen {
    /// What the person approving is told (plan-header.md §5.3). A label claims
    /// only what the digest showed (D-20).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = self.value.as_str();
        match self.source {
            Source::Flag => write!(f, "{value} (from {})", flag_for(self.value)),
            Source::EarlierApproval => write!(f, "{value} (kept from the earlier approval)"),
            Source::UnconfirmedHeader => write!(
                f,
                "{value} (from the header; the earlier approval cannot confirm it)"
            ),
            Source::Default(None) => write!(f, "{value} (default)"),
            Source::Default(Some(header)) => write!(
                f,
                "{value} (default; the `{}` in the header was not part of the approval)",
                header.as_str()
            ),
        }
    }
}

/// Decide the value of `Acceptance` (plan-header.md §5.3, D-20): `flag` if one
/// was given, else what `earlier` has to keep, else `acceptance.required` from
/// `invar.yaml`, else `skipped`.
pub fn choose_acceptance(flag: Option<Acceptance>, earlier: Earlier) -> Chosen {
    if let Some(value) = flag {
        return Chosen {
            value,
            source: Source::Flag,
        };
    }
    let in_header = match earlier {
        Earlier::Confirmed(value) => {
            return Chosen {
                value,
                source: Source::EarlierApproval,
            };
        }
        Earlier::Unconfirmed(value) => {
            return Chosen {
                value,
                source: Source::UnconfirmedHeader,
            };
        }
        Earlier::NotApproved(value) => Some(value),
        Earlier::Nothing => None,
    };
    // AT-4: `acceptance.required` from `invar.yaml` plugs in here, ahead of the
    // default, printed as `(from invar.yaml)`. `--acceptance` is then refused
    // above when the config has no `acceptance:` section: there is no `run`
    // command to test with, and the refusal says what to add.
    Chosen {
        value: Acceptance::Skipped,
        source: Source::Default(in_header),
    }
}

/// What `plan approve` will do.
#[derive(Debug)]
pub enum Decision {
    /// Approve: print the acceptance criteria, then [`Pending::commit`].
    Approve(Box<Pending>),
    /// Already approved, and nothing changed since: nothing to write.
    AlreadyApproved {
        spec: PathBuf,
        by: String,
        at: String,
        /// The value on record, and the flag that named another one, if any.
        /// That flag changed nothing, and the person who gave it is told so.
        recorded: Acceptance,
        ignored_flag: Option<&'static str>,
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
    acceptance: Chosen,
    /// The earlier approval being replaced, under `--reapprove`.
    replaces: Option<Prior>,
}

/// Check `spec` for approval.
///
/// `reapprove` allows replacing an approval that changed or whose values are
/// missing (D-12). It is not needed for a draft. An approval that is still
/// current is left alone with or without it, unless `flag` names another value
/// of `Acceptance` than the one on record (D-20). `Acceptance` is decided here,
/// once the approval being replaced is known, and so before anything is shown.
pub fn prepare(
    root: &ProjectRoot,
    spec: &Path,
    mode: Mode,
    reapprove: bool,
    flag: Option<Acceptance>,
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
    let current = approval::current_digest(&plan);
    let recorded = plan.header().recorded_acceptance();
    let differing_flag = flag.filter(|value| *value != recorded);

    let replaces = match classify(plan.header(), &current) {
        Prior::Draft => None,
        Prior::Current { by, at } if !reapprove || differing_flag.is_none() => {
            return Ok(Decision::AlreadyApproved {
                spec: spec.to_owned(),
                by,
                at,
                recorded,
                ignored_flag: differing_flag.map(flag_for),
            });
        }
        prior => {
            if !reapprove {
                return Err(ApproveError::NeedsReapprove {
                    spec: spec.to_owned(),
                    prior,
                    current,
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

    // A draft's value means nothing (§5.1), and an approval no command recorded
    // has nothing to keep (example 9.3). A flag settles it without asking.
    let earlier = match (&replaces, flag) {
        (Some(Prior::Changed { by, at, recorded }), None) => {
            let in_header = plan.header().acceptance;
            match approval::approved_value(&plan, recorded) {
                ApprovedValue::Confirmed(value) => Earlier::Confirmed(value),
                ApprovedValue::Changed(approved) => {
                    return Err(ApproveError::AcceptanceChanged {
                        spec: spec.to_owned(),
                        by: by.clone(),
                        at: at.clone(),
                        approved,
                        in_header,
                    });
                }
                ApprovedValue::None => in_header.map_or(Earlier::Nothing, Earlier::NotApproved),
                ApprovedValue::Unknown => in_header.map_or(Earlier::Nothing, Earlier::Unconfirmed),
            }
        }
        _ => Earlier::Nothing,
    };
    let acceptance = choose_acceptance(flag, earlier);
    let checksum = approval::approval_digest(&plan, Some(acceptance.value));

    Ok(Decision::Approve(Box::new(Pending {
        spec: spec.to_owned(),
        mode,
        original,
        plan,
        checksum,
        acceptance,
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

    /// The value being written, and where it came from.
    pub fn acceptance(&self) -> Chosen {
        self.acceptance
    }

    /// The value on record in the approval being replaced.
    pub fn recorded_acceptance(&self) -> Acceptance {
        self.plan.header().recorded_acceptance()
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
            acceptance: self.acceptance.value,
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
            acceptance: self.acceptance.value,
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
                    && reread.header().acceptance == Some(self.acceptance.value)
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
    /// `--reapprove` with no flag, and the digest shows that the body is what
    /// was approved and `Acceptance` is not (D-20): a person chooses the value.
    AcceptanceChanged {
        spec: PathBuf,
        by: String,
        at: String,
        approved: Acceptance,
        in_header: Option<Acceptance>,
    },
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
                        "the body or `Acceptance` changed after {by} approved it on {at} \
                         (approved {recorded}, now {current}). An approval covers what was \
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
                    // `prepare` never refuses with a draft or a current approval.
                    Prior::Draft | Prior::Current { .. } => {
                        write!(f, "cannot be approved in its current state")
                    }
                }
            }
            Self::AcceptanceChanged {
                spec,
                by,
                at,
                approved,
                in_header,
            } => {
                let other = match approved {
                    Acceptance::Required => Acceptance::Skipped,
                    Acceptance::Skipped => Acceptance::Required,
                };
                let now = match in_header {
                    Some(value) => format!("the header now says `{}`", value.as_str()),
                    None => "the header now has no value".to_owned(),
                };
                write!(
                    f,
                    "{}: `Acceptance` was `{}` when {by} approved it on {at}, and {now}. Choose \
                     explicitly: re-approve with `--reapprove {}` to keep `{}`, or \
                     `--reapprove {}` to record `{}`",
                    spec.display(),
                    approved.as_str(),
                    flag_for(*approved),
                    approved.as_str(),
                    flag_for(other),
                    other.as_str()
                )
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
