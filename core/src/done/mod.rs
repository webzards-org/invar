//! `invar done` (M1-12): closes a cycle (§3, §7, §12).
//!
//! All-or-nothing, in this order:
//!
//! 1. `plan check` (M1-10) — refuse on a draft or changed Plan; nothing is
//!    written.
//! 2. render the PR description (M1-16) to `.invar/out/<TICKET>-pr.md`.
//! 3. write a `confirmed` decision entry (M1-11), its body recording what was
//!    actually built — including deviations.
//! 4. delete the Plan directory.
//!
//! Each step runs only if the one before it succeeded, so there is never a
//! decision entry without a rendered PR description, and never a deleted
//! Plan without an entry — the second of those is exactly the state
//! `clean-spec` treats as a broken cycle (§12). A failure at step 4, after
//! the entry already exists, cannot produce that state; it is reported as its
//! own case, since the cycle is recorded but not yet fully closed.

use std::fmt;
use std::fs;
use std::io;
use std::path::PathBuf;

use crate::decision::{self, RecordError, Recorded};
use crate::format::decision::Type;
use crate::format::plan::{Mode, Plan, PlanError};
use crate::fs::ProjectRoot;
use crate::gate::check::{self, CheckError};
use crate::git::Instant;
use crate::pr_body;

#[cfg(test)]
mod tests;

/// What to close.
#[derive(Debug)]
pub struct Request {
    /// The Plan's `spec.md`, already resolved from a ticket or a path.
    pub spec: PathBuf,
    /// The `confirmed` entry's text.
    pub body: String,
    /// Include the pasted `## Ticket` text in the PR description.
    pub with_ticket: bool,
}

/// What closing the cycle produced.
#[derive(Debug)]
pub struct Closed {
    pub pr_description: PathBuf,
    pub entry: Recorded,
    pub plan_dir: PathBuf,
}

/// Close the cycle `request` names, at `now` (git's clock, decision-entry.md
/// D-2).
pub fn close(
    root: &ProjectRoot,
    request: Request,
    mode: Mode,
    now: Instant,
) -> Result<Closed, DoneError> {
    let Request {
        spec,
        body,
        with_ticket,
    } = request;
    let plan_dir = spec
        .parent()
        .expect("a Plan's spec.md always has a parent directory")
        .to_owned();

    // 1. `plan check` — refuse on a draft or changed Plan; writes nothing.
    check::check(root, &spec, mode).map_err(DoneError::Check)?;

    // `check` just proved the file parses as a valid, approved Plan. Reading
    // it again for its content is not expected to fail, but the error is
    // still named rather than unwrapped: a file can always change underneath
    // a running process.
    let plan = Plan::read(&spec, mode).map_err(DoneError::ReadPlan)?;
    let ticket = plan.header().ticket.clone();

    // 2. render the PR description (M1-16) to `.invar/out/<TICKET>-pr.md`.
    let rendered = pr_body::render(&plan, &ticket, with_ticket);
    let out_dir = root.out_dir();
    fs::create_dir_all(&out_dir).map_err(|source| DoneError::Io {
        path: out_dir.clone(),
        source,
    })?;
    let pr_description = out_dir.join(format!("{ticket}-pr.md"));
    fs::write(&pr_description, &rendered).map_err(|source| DoneError::Io {
        path: pr_description.clone(),
        source,
    })?;

    // 3. write a `confirmed` decision entry. The module comes from the Plan,
    //    the same way `decision record` reads it when none is given.
    let entry_request = decision::Request {
        ticket: ticket.clone(),
        kind: Type::Confirmed,
        module: None,
        body,
    };
    let entry = decision::record(root, entry_request, now, mode).map_err(DoneError::Record)?;

    // 4. delete the Plan directory. The entry above already exists, so a
    //    failure here cannot produce the state `clean-spec` treats as broken
    //    (§12) — a Plan with no entry — but the cycle is not fully closed
    //    until this succeeds.
    fs::remove_dir_all(&plan_dir).map_err(|source| DoneError::RemovePlan {
        plan_dir: plan_dir.clone(),
        entry: entry.path.clone(),
        source,
    })?;

    Ok(Closed {
        pr_description,
        entry,
        plan_dir,
    })
}

// --- errors -------------------------------------------------------------------

/// Why the cycle did not close. Each variant names the step that failed, so a
/// caller — and a test forcing a failure at each step — can tell them apart.
#[derive(Debug)]
pub enum DoneError {
    /// Step 1: the Plan is a draft, changed since approval, or not valid.
    /// Nothing is written.
    Check(CheckError),
    /// The Plan could not be re-read right after `plan check` confirmed it
    /// parses. Not expected.
    ReadPlan(PlanError),
    /// Step 2: the PR description could not be written.
    Io { path: PathBuf, source: io::Error },
    /// Step 3: the decision entry could not be written. The PR description
    /// file may already exist — harmless, since `.invar/out/` is a working
    /// directory, not a repository artifact.
    Record(RecordError),
    /// Step 4: the entry was recorded, but the Plan directory could not be
    /// removed.
    RemovePlan {
        plan_dir: PathBuf,
        entry: PathBuf,
        source: io::Error,
    },
}

impl fmt::Display for DoneError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Check(err) => write!(f, "{err}"),
            Self::ReadPlan(err) => write!(
                f,
                "the Plan passed `plan check` a moment ago but cannot be read now: {err}"
            ),
            Self::Io { path, source } => write!(f, "cannot write {}: {source}", path.display()),
            Self::Record(err) => write!(f, "{err}"),
            Self::RemovePlan {
                plan_dir,
                entry,
                source,
            } => write!(
                f,
                "the decision entry {} was recorded, but {} could not be removed: {source}. \
                 The cycle is recorded but not fully closed: remove the Plan directory by hand, \
                 or run `invar done` again once the problem is fixed",
                entry.display(),
                plan_dir.display()
            ),
        }
    }
}

// No `source()`: every variant's message already carries its cause, and
// `main` prints the whole chain, so exposing it would print each refusal twice
// ("the Plan ... is still a draft: the Plan ... is still a draft").
impl std::error::Error for DoneError {}
