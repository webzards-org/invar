//! `invar done`: closes a cycle by deleting its Plan (§3, §12).
//!
//! The developer's command, run from their terminal or from CI — never by an
//! agent (`plan guard` denies it). In this order:
//!
//! 1. `plan check` — refuse on a draft or changed Plan; nothing is changed.
//! 2. refuse while a draft decision entry (`decision.md`) sits in the Plan
//!    directory: deleting the Plan would delete the text the developer
//!    reviewed, and that is the one way this flow could lose it silently.
//! 3. refuse unless the acceptance locks hold (`acceptance-lock.md` §10), with
//!    no pending line left. No test is run (A-2).
//! 4. delete the Plan directory.
//! 5. stage the deletion (`git rm -r --cached`), so it goes into the same
//!    commit — and the same PR — as the code.
//! 6. with `--commit <MESSAGE>` — or `done.commit: true` in `invar.yaml`,
//!    under `done.commit_message` — commit the deletion alone, under that
//!    message (`git commit --only`): anything else that is staged stays
//!    staged. invar makes no commit unless asked; the project's hooks, signing
//!    and identity apply, and are never bypassed. The CLI picks the message
//!    (`cli/done.rs`); this module only receives it.
//!
//! **No decision entry is read or written** (decision-entry.md 1.3.0, D-12).
//! Entries are the developer's, approved from drafts with
//! `invar decision approve`; whether a cycle left one is not this command's
//! business. No PR description is rendered either: that is `invar pr-body`,
//! run while the Plan still exists.
//!
//! Which Plan: the one a ticket or path names, or — with neither — the one
//! whose ticket the current branch names (`feat/SHOP-214-cancel` →
//! `SHOP-214`), matched as a Plan lookup matches: a whole segment of the branch
//! name, never a guess between two.
//!
//! **The only way a Plan is deleted.** A Plan that reached the main branch
//! without `done` is removed with `git rm -r`; there is no cleanup command.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::acceptance::verify::{self, Report, Stage};
use crate::format::config::Config;
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
    pub committed: Committed,
    /// What the check of the locks said without refusing.
    pub warnings: Vec<String>,
}

/// Whether the deletion was committed.
#[derive(Debug, PartialEq, Eq)]
pub enum Committed {
    /// No `--commit`.
    NotAsked,
    /// Committed, under this message.
    Yes(String),
    /// `--commit`, but git never tracked the Plan: there was nothing to
    /// commit.
    NothingTracked,
}

/// Whether the deletion was staged.
#[derive(Debug, PartialEq, Eq)]
pub enum Staged {
    Yes,
    /// The project is not in a git work tree; what git said.
    NotARepository(String),
}

/// Close the cycle of the Plan whose `spec.md` is `spec`; with `commit`,
/// commit the deletion under that message.
pub fn close(
    root: &ProjectRoot,
    spec: &Path,
    config: &Config,
    commit: Option<&str>,
) -> Result<Closed, DoneError> {
    let plan_dir = spec
        .parent()
        .expect("a Plan's spec.md always has a parent directory")
        .to_owned();
    let relative = plan_dir
        .strip_prefix(root.path())
        .unwrap_or(&plan_dir)
        .to_owned();

    // 1. The Plan is approved and unchanged; changes nothing.
    let approved = check::check(root, spec, Mode::of(config)).map_err(DoneError::Check)?;

    // 2. A draft entry nobody approved: the developer decides its fate.
    let draft = plan_dir.join(DRAFT_FILE);
    if draft.symlink_metadata().is_ok() {
        return Err(DoneError::DraftPending { draft });
    }

    // 3. The locks hold, and none is half done.
    let report = verify::for_ticket(root, config, &approved, Stage::Closing);
    if !report.holds() {
        return Err(DoneError::Locks(report));
    }

    // `--commit` is checked before anything is deleted: a message that is
    // empty, or a project outside git, refuses with the Plan still in place.
    let commit = match commit {
        None => None,
        Some(message) => {
            let message = message.trim();
            if message.is_empty() {
                return Err(DoneError::EmptyCommitMessage);
            }
            let git = |source| DoneError::Git { source };
            if let WorkTree::Outside(said) = git::work_tree(root.path()).map_err(git)? {
                return Err(DoneError::CommitOutsideRepository { said });
            }
            let tracked = git::is_tracked(root.path(), &relative).map_err(git)?;
            Some((message.to_owned(), tracked))
        }
    };

    // 4. Delete. `remove_dir_all` does not follow a symlinked directory: it
    //    unlinks it, and leaves what it pointed at alone.
    fs::remove_dir_all(&plan_dir).map_err(|source| DoneError::Remove {
        plan_dir: plan_dir.clone(),
        source,
    })?;

    // 5. Stage the deletion.
    let staged = stage(root, &relative).map_err(|source| DoneError::Stage {
        plan_dir: plan_dir.clone(),
        source,
    })?;

    // 6. Commit it, alone, when asked.
    let committed = match commit {
        None => Committed::NotAsked,
        Some((_, false)) => Committed::NothingTracked,
        Some((message, true)) => {
            git::commit_only(root.path(), &relative, &message).map_err(|source| {
                DoneError::Commit {
                    relative: relative.clone(),
                    message: message.clone(),
                    source,
                }
            })?;
            Committed::Yes(message)
        }
    };

    Ok(Closed {
        plan_dir,
        staged,
        committed,
        warnings: report.warnings,
    })
}

fn stage(root: &ProjectRoot, relative: &Path) -> Result<Staged, GitError> {
    if let WorkTree::Outside(said) = git::work_tree(root.path())? {
        return Ok(Staged::NotARepository(said));
    }
    git::stage_removal(root.path(), relative)?;
    Ok(Staged::Yes)
}

/// The `spec.md` of the one Plan whose ticket `branch` names.
///
/// A ticket is named when it is a whole segment of the branch name, with its
/// capitalisation (`plans::matches_segment`): `feat/SHOP-214-cancel` names
/// `SHOP-214`, and not `SHOP-21`.
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
    /// The acceptance locks do not hold. Nothing was changed.
    Locks(Report),
    /// The Plan directory could not be removed (perhaps in part).
    Remove {
        plan_dir: PathBuf,
        source: io::Error,
    },
    /// The Plan was deleted, but git could not stage the deletion.
    Stage { plan_dir: PathBuf, source: GitError },
    /// `--commit` with an empty message. Nothing was changed.
    EmptyCommitMessage,
    /// `--commit` outside a git work tree. Nothing was changed.
    CommitOutsideRepository { said: String },
    /// git could not be asked what `--commit` needs. Nothing was changed.
    Git { source: GitError },
    /// The deletion is staged, but the commit was refused (a hook, signing,
    /// identity).
    Commit {
        /// The Plan directory, relative to the project root.
        relative: PathBuf,
        message: String,
        source: GitError,
    },
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
            Self::Locks(report) => write!(f, "{report}\nNothing was changed"),
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
            Self::EmptyCommitMessage => write!(
                f,
                "`--commit` needs the commit message, such as `--commit \"chore: remove the \
                 Plan of SHOP-214\"`; nothing was changed"
            ),
            Self::CommitOutsideRepository { said } => write!(
                f,
                "`--commit` needs a git repository, and the project is not in one ({said}); \
                 nothing was changed. Run `invar done` without `--commit` — with \
                 `--no-commit` when `invar.yaml` sets `done.commit: true`"
            ),
            Self::Git { source } => write!(f, "{source}; nothing was changed"),
            Self::Commit {
                relative,
                message,
                source,
            } => write!(
                f,
                "{} was removed and the removal is staged, but the commit was refused: \
                 {source}. Fix what git says, then commit it yourself: `git commit --only \
                 -m {message:?} -- {}`",
                relative.display(),
                relative.display()
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
