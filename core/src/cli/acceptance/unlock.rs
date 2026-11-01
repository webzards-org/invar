//! `invar acceptance unlock <TICKET | PATH> <FILE>...` (AT-13): the developer's, through
//! `/invar:relock`. `plan guard` denies it to an agent.

use anyhow::{Context as _, Result};

use crate::acceptance::unlock::{self, UnlockError};
use crate::cli::plan::target;
use crate::format::plan::Mode;

use super::spec::show_problems;

#[derive(Debug, clap::Args)]
pub struct Args {
    /// A ticket id (`PROJ-412`), or the path of a Plan: its `spec.md` or its
    /// directory.
    ///
    /// Anything containing `/`, ending in `.md`, or `.`/`..` is a path;
    /// everything else is looked up as a ticket under `.invar/plans/`.
    pub target: String,

    /// The test files to unlock, all or none. Each must be under `acceptance.dir`,
    /// locked for this ticket, with its digest holding.
    pub files: Vec<String>,

    /// Report as `/invar:relock` needs it: a refusal is printed on stdout as
    /// `error: …`, and the exit code is 0.
    ///
    /// A skill's shell block that exits non-zero is dropped by Claude Code
    /// with its output, so the refusal would never reach the session.
    #[arg(long)]
    pub skill: bool,
}

pub fn run(args: Args) -> Result<()> {
    let skill = args.skill;
    crate::cli::for_skill(skill, unlock(args))
}

fn unlock(args: Args) -> Result<()> {
    // Checked here and not by clap, which would refuse it before `--skill` is read.
    if args.files.is_empty() {
        return Err(UnlockError::NoFiles {
            target: args.target,
        }
        .into());
    }
    let cwd = std::env::current_dir().context("cannot read the current directory")?;
    let (root, plan) = target::resolve(&args.target, &cwd)?;
    let config = crate::cli::config(&root)?;
    let mode = Mode::of(&config);

    match unlock::unlock(
        &root,
        &plan,
        mode,
        config.acceptance.as_ref(),
        &args.files,
        &cwd,
    ) {
        Ok(unlocked) => {
            print!("{}", unlocked.render());
            Ok(())
        }
        Err(err) => {
            if let UnlockError::Plan(err) = &err {
                show_problems(err, &cwd);
            }
            Err(err.into())
        }
    }
}
