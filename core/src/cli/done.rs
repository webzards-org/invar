//! `invar done [TICKET | PATH] [--commit <MESSAGE>]`: delete the Plan, stage
//! the deletion, and commit it when asked.

use anyhow::{Context as _, Result};

use crate::done::{self, Committed, DoneError, Staged};
use crate::fs;
use crate::gate::check::CheckError;
use crate::git;

use super::plan::{require_approved, target};

#[derive(Debug, clap::Args)]
pub struct Args {
    /// A ticket id (`PROJ-412`), or the path of a Plan: its `spec.md` or its
    /// directory. Without it, the Plan whose ticket the current branch's name
    /// contains (`feat/PROJ-412-reserve-stock`).
    ///
    /// Anything containing `/`, ending in `.md`, or `.`/`..` is a path;
    /// everything else is looked up as a ticket under `.invar/plans/`.
    pub target: Option<String>,

    /// Also commit the deletion, alone, with this message:
    /// `--commit "chore: remove the Plan of PROJ-412"`.
    ///
    /// Only the Plan directory goes into the commit; anything else that is
    /// staged stays staged. The project's hooks, signing and identity apply.
    /// Nothing is pushed. Without it, the deletion is staged and the commit is
    /// yours.
    #[arg(long, value_name = "MESSAGE")]
    pub commit: Option<String>,
}

pub fn run(args: Args) -> Result<()> {
    let cwd = std::env::current_dir().context("cannot read the current directory")?;
    let (root, spec) = match &args.target {
        Some(target) => target::resolve(target, &cwd)?,
        None => {
            let root = fs::project_root()?;
            let branch = git::current_branch(root.path())?.ok_or(DoneError::DetachedHead)?;
            let spec = done::plan_for_branch(&root, &branch)?;
            (root, spec)
        }
    };
    let mode = super::mode(&root)?;

    let closed = match done::close(&root, &spec, mode, args.commit.as_deref()) {
        Ok(closed) => closed,
        // The refusal for a Plan that is not valid in form says "fix every
        // problem listed above"; here is the list it points at.
        Err(DoneError::Check(CheckError::Invalid(report))) => {
            require_approved::print_problems(&report, &cwd);
            return Err(DoneError::Check(CheckError::Invalid(report)).into());
        }
        Err(err) => return Err(err.into()),
    };

    let shown = target::shown(&closed.plan_dir, &cwd);
    match (&closed.staged, &closed.committed) {
        (_, Committed::Yes(message)) => {
            println!("removed: {shown} (committed: {message})");
        }
        (_, Committed::NothingTracked) => {
            println!("removed: {shown} (git never tracked it, so there was nothing to commit)");
        }
        (Staged::Yes, Committed::NotAsked) => {
            println!("removed: {shown} (staged; commit it with the rest of the branch)");
        }
        (Staged::NotARepository(said), Committed::NotAsked) => {
            println!("removed: {shown}");
            eprintln!("warning: not staged — the project is not in a git work tree ({said})");
        }
    }
    Ok(())
}
