//! `invar plan check` (M1-10): may code be written against this Plan?
//!
//! The read side of the gate. Run by the agent at the start of Phase 2 and by
//! `invar done`, it turns "the coder may not work from a draft Plan" into a
//! check on disk. It passes exactly one state: `Status: approved`, every
//! approval value present, and `Approved-checksum` equal to the approval digest
//! of the Plan as it is now (§5.3, D-19). Everything else is refused, each case
//! in its own words (plan-header.md §5.1, D-11):
//!
//! - a draft — nobody has approved it;
//! - an approval whose body or `Acceptance` changed since — the developer
//!   approved something else;
//! - `approved` without its recorded values (example 9.3) — no
//!   `plan approve` wrote it;
//! - a Plan that is not valid in form — its approval cannot be read.
//!
//! The checksum is the boundary. Edits through any tool — `Write`, `Edit`,
//! `Bash`, a formatter, `git checkout` — all end up here as a changed digest,
//! which is why this check, not `plan guard`, is the gate.

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use crate::format::plan::{Mode, Plan};
use crate::fs::ProjectRoot;

use super::approval::{self, Prior, classify};
use super::validate::{self, Report};

#[cfg(test)]
mod tests;

/// An approval that holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Approved {
    pub spec: PathBuf,
    pub ticket: String,
    pub by: String,
    pub at: String,
    pub checksum: String,
}

/// Check that the Plan at `spec` is approved and unchanged since.
pub fn check(root: &ProjectRoot, spec: &Path, mode: Mode) -> Result<Approved, CheckError> {
    let report = validate::file(root, spec, mode);
    if !report.is_valid() {
        return Err(CheckError::Invalid(report));
    }
    let text = std::fs::read_to_string(spec).map_err(|source| CheckError::Read {
        spec: spec.to_owned(),
        source,
    })?;
    // Validation just passed; this fails only if the file changed since.
    let plan = Plan::parse(&text, mode).map_err(|problems| {
        CheckError::Invalid(Report {
            spec: spec.to_owned(),
            status: None,
            problems,
        })
    })?;

    let current = approval::current_digest(&plan);
    let spec = spec.to_owned();
    let ticket = plan.header().ticket.clone();
    match classify(plan.header(), &current) {
        Prior::Current { by, at } => Ok(Approved {
            spec,
            ticket,
            by,
            at,
            checksum: current,
        }),
        Prior::Draft => Err(CheckError::Draft { spec, ticket }),
        Prior::Changed { by, at, recorded } => Err(CheckError::Changed(Box::new(Stale {
            spec,
            ticket,
            by,
            at,
            recorded,
            current,
        }))),
        Prior::Unrecorded { missing } => Err(CheckError::Unrecorded {
            spec,
            ticket,
            missing,
        }),
    }
}

// --- refusals ---------------------------------------------------------------

/// An approval whose body or `Acceptance` changed since.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stale {
    pub spec: PathBuf,
    pub ticket: String,
    /// Who approved, and when.
    pub by: String,
    pub at: String,
    /// `Approved-checksum`, and the approval digest now.
    pub recorded: String,
    pub current: String,
}

/// Why no code may be written against the Plan. Each case is its own
/// variant, so `done` and the tests can tell them apart.
#[derive(Debug)]
pub enum CheckError {
    /// Not valid in form: the approval cannot be read.
    Invalid(Report),
    Read {
        spec: PathBuf,
        source: io::Error,
    },
    /// `Status: draft`.
    Draft {
        spec: PathBuf,
        ticket: String,
    },
    /// Approved, and the body or `Acceptance` changed since.
    Changed(Box<Stale>),
    /// `Status: approved` without the values `plan approve` writes.
    Unrecorded {
        spec: PathBuf,
        ticket: String,
        missing: Vec<&'static str>,
    },
}

impl fmt::Display for CheckError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(report) => {
                let count = report.problems.len();
                let noun = if count == 1 { "problem" } else { "problems" };
                write!(
                    f,
                    "the Plan is not valid ({count} {noun}), so whether it is approved \
                     cannot be checked. Do not start implementing. Fix every problem \
                     listed above — `invar plan validate` checks them — and if the Plan \
                     was approved, the developer re-approves it"
                )
            }
            Self::Read { spec, source } => write!(f, "cannot read {}: {source}", spec.display()),
            Self::Draft { spec, ticket } => write!(
                f,
                "{}: the Plan for {ticket} is still a draft: nobody has approved it, so no \
                 code may be written against it yet. Do not start implementing. The \
                 developer reads the Plan and approves it with \
                 `invar plan approve {ticket}`",
                spec.display()
            ),
            Self::Changed(stale) => {
                let Stale {
                    spec,
                    ticket,
                    by,
                    at,
                    recorded,
                    current,
                } = stale.as_ref();
                write!(
                    f,
                    "{}: the Plan for {ticket} changed after it was approved: {by} \
                     approved it on {at}, and its body or its `Acceptance` value is no \
                     longer what they approved (approved {recorded}, now {current}). Do not \
                     start implementing. Show \
                     the developer what changed; if they accept it, they approve it again \
                     with `invar plan approve {ticket} --reapprove`, otherwise restore the \
                     approved text",
                    spec.display()
                )
            }
            Self::Unrecorded {
                spec,
                ticket,
                missing,
            } => write!(
                f,
                "{}: the Plan for {ticket} says `Status: approved`, but {} {} empty: no \
                 `invar plan approve` recorded this approval, so nothing shows that the \
                 developer accepted this text (plan-header.md example 9.3). Do not start \
                 implementing. The developer reads the Plan and approves it with \
                 `invar plan approve {ticket} --reapprove`",
                spec.display(),
                missing
                    .iter()
                    .map(|key| format!("`{key}`"))
                    .collect::<Vec<_>>()
                    .join(" and "),
                if missing.len() == 1 { "is" } else { "are" }
            ),
        }
    }
}

impl std::error::Error for CheckError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Read { source, .. } => Some(source),
            _ => None,
        }
    }
}
