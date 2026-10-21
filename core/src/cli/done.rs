//! `invar done [TICKET | PATH]`: delete the Plan, and stage the deletion.

use anyhow::{Context as _, Result};

use crate::done::{self, DoneError, Staged};
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

    let closed = match done::close(&root, &spec, mode) {
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
    match closed.staged {
        Staged::Yes => {
            println!("removed: {shown} (staged; commit it with the rest of the branch)");
        }
        Staged::NotARepository(said) => {
            println!("removed: {shown}");
            eprintln!("warning: not staged — the project is not in a git work tree ({said})");
        }
    }
    Ok(())
}
