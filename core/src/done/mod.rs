//! `invar done`: closes a cycle by deleting its Plan (§3, §12).
//!
//! The developer's command, run from their terminal or from CI — never by an
//! agent (`plan guard` denies it). In this order:
//!
//! 1. `plan check` — refuse on a draft or changed Plan; nothing is changed.
//! 2. refuse while a draft decision entry (`decision.md`) sits in the Plan
//!    directory: deleting the Plan would delete the text the developer
//!    reviewed, and that is the one way this flow could lose it silently.
//! 3. delete the Plan directory.
//! 4. stage the deletion (`git rm -r --cached`), so it goes into the same
//!    commit — and the same PR — as the code.
//!
//! **No decision entry is read or written** (decision-entry.md 1.3.0, D-12).
//! Entries are the developer's, approved from drafts with
//! `invar decision approve`; whether a cycle left one is not this command's
//! business. No PR description is rendered either: that is `invar pr-body`,
//! run while the Plan still exists.
//!
//! Which Plan: the one a ticket or path names, or — with neither — the one
//! whose ticket the current branch names (`feat/SHOP-214-cancel` →
//! `SHOP-214`), matched as `clean-spec --merged` matches: a whole segment of
//! the branch name, never a guess between two.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::format::decision::DRAFT_FILE;
use crate::format::plan::Mode;
use crate::fs::ProjectRoot;
use crate::fs::plans::{self, LookupError, SPEC_FILE, matches_segment, parse_dir_name};
use crate::gate::check::{self, CheckError};
use crate::git::{self, GitError, WorkTree};

#[cfg(test)]
mod tests;

/// What closing the cycle did.
#[derive(Debug)]
pub struct Closed {
    /// The Plan directory, now gone.
    pub plan_dir: PathBuf,
    pub staged: Staged,
}

/// Whether the deletion was staged.
#[derive(Debug, PartialEq, Eq)]
pub enum Staged {
    Yes,
    /// The project is not in a git work tree; what git said.
    NotARepository(String),
}

/// Close the cycle of the Plan whose `spec.md` is `spec`.
pub fn close(root: &ProjectRoot, spec: &Path, mode: Mode) -> Result<Closed, DoneError> {
    let plan_dir = spec
        .parent()
        .expect("a Plan's spec.md always has a parent directory")
        .to_owned();

    // 1. The Plan is approved and unchanged; changes nothing.
    check::check(root, spec, mode).map_err(DoneError::Check)?;

    // 2. A draft entry nobody approved: the developer decides its fate.
    let draft = plan_dir.join(DRAFT_FILE);
    if draft.symlink_metadata().is_ok() {
        return Err(DoneError::DraftPending { draft });
    }

    // 3. Delete. `remove_dir_all` does not follow a symlinked directory: it
    //    unlinks it, and leaves what it pointed at alone.
    fs::remove_dir_all(&plan_dir).map_err(|source| DoneError::Remove {
        plan_dir: plan_dir.clone(),
        source,
    })?;

    // 4. Stage the deletion.
    let staged = stage(root, &plan_dir).map_err(|source| DoneError::Stage {
        plan_dir: plan_dir.clone(),
        source,
    })?;

    Ok(Closed { plan_dir, staged })
}

fn stage(root: &ProjectRoot, plan_dir: &Path) -> Result<Staged, GitError> {
    if let WorkTree::Outside(said) = git::work_tree(root.path())? {
        return Ok(Staged::NotARepository(said));
    }
    let relative = plan_dir.strip_prefix(root.path()).unwrap_or(plan_dir);
    git::stage_removal(root.path(), relative)?;
    Ok(Staged::Yes)
}

/// The `spec.md` of the one Plan whose ticket `branch` names.
///
/// A ticket is named when it is a whole segment of the branch name, with its
/// capitalisation (`plans::matches_segment`, as `clean-spec --merged` uses
/// it): `feat/SHOP-214-cancel` names `SHOP-214`, and not `SHOP-21`.
pub fn plan_for_branch(root: &ProjectRoot, branch: &str) -> Result<PathBuf, DoneError> {
    let names = plans::list(root)
        .map_err(DoneError::Lookup)?
        .unwrap_or_default();
    let named: Vec<&String> = names
        .iter()
        .filter(|name| {
            parse_dir_name(name).is_some_and(|(ticket, _)| matches_segment(branch, ticket))
        })
        .collect();
    match named.as_slice() {
        [name] => Ok(root.plans_dir().join(name).join(SPEC_FILE)),
        [] => Err(DoneError::BranchNamesNoPlan {
            branch: branch.to_owned(),
            plans: names.clone(),
        }),
        several => Err(DoneError::BranchNamesSeveral {
            branch: branch.to_owned(),
            plans: several.iter().map(|name| (*name).clone()).collect(),
        }),
    }
}

// --- errors -------------------------------------------------------------------

/// Why the cycle did not close. Each variant names the step that failed.
#[derive(Debug)]
pub enum DoneError {
    /// The Plan is a draft, changed since approval, or not valid. Nothing was
    /// changed.
    Check(CheckError),
    /// A draft decision entry is still in the Plan directory. Nothing was
    /// changed.
    DraftPending { draft: PathBuf },
    /// The Plan directory could not be removed (perhaps in part).
    Remove {
        plan_dir: PathBuf,
        source: io::Error,
    },
    /// The Plan was deleted, but git could not stage the deletion.
    Stage { plan_dir: PathBuf, source: GitError },
    /// No ticket given, and `HEAD` is not on a branch.
    DetachedHead,
    /// The Plans could not be listed.
    Lookup(LookupError),
    /// No ticket given, and no Plan's ticket is in the branch name.
    BranchNamesNoPlan { branch: String, plans: Vec<String> },
    /// No ticket given, and several Plans' tickets are in the branch name.
    BranchNamesSeveral { branch: String, plans: Vec<String> },
}

impl fmt::Display for DoneError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Check(err) => write!(f, "{err}"),
            Self::DraftPending { draft } => write!(
                f,
                "{} is a draft decision entry nobody has approved, and deleting the Plan \
                 would delete it with it. Approve it with /invar:log-approve, or delete it; \
                 then run `invar done` again. Nothing was changed",
                draft.display()
            ),
            Self::Remove { plan_dir, source } => write!(
                f,
                "cannot remove {}: {source}. Part of it may be gone already; remove the rest \
                 by hand, or run `invar done` again once the problem is fixed",
                plan_dir.display()
            ),
            Self::Stage { plan_dir, source } => write!(
                f,
                "{} was removed, but the removal could not be staged: {source}. Stage it \
                 yourself with `git rm -r --cached {}`",
                plan_dir.display(),
                plan_dir.display()
            ),
            Self::DetachedHead => write!(
                f,
                "no ticket given, and HEAD is not on a branch (detached, as in a CI checkout \
                 of a pull request), so the Plan cannot be told from the branch name. Name it: \
                 `invar done <TICKET>`, or check out the branch first"
            ),
            Self::Lookup(err) => write!(f, "{err}"),
            Self::BranchNamesNoPlan { branch, plans } => {
                write!(f, "no Plan's ticket appears in the branch name `{branch}`")?;
                if plans.is_empty() {
                    write!(f, ", and there are no Plans in .invar/plans/")?;
                } else {
                    write!(f, "; the Plans are: {}", plans.join(", "))?;
                }
                write!(
                    f,
                    ". Name the one to close: `invar done <TICKET>`. A ticket is matched in \
                     the branch name with its capitalisation"
                )
            }
            Self::BranchNamesSeveral { branch, plans } => write!(
                f,
                "the branch name `{branch}` names {} Plans: {}. invar does not guess; name \
                 the one to close: `invar done <TICKET>`",
                plans.len(),
                plans.join(", ")
            ),
        }
    }
}

// No `source()`: every variant's message already carries its cause, and
// `main` prints the whole chain, so exposing it would print each refusal twice
// ("the Plan ... is still a draft: the Plan ... is still a draft").
impl std::error::Error for DoneError {}
