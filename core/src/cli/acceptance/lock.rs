//! `invar acceptance lock <TICKET | PATH>` (AT-8).
//!
//! A refusal leaves stdout empty, so that the fixed last line is only ever there
//! when it is true.

use anyhow::{Context as _, Result};

use crate::acceptance::lock::{self, LockError};
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
}

pub fn run(args: Args) -> Result<()> {
    let cwd = std::env::current_dir().context("cannot read the current directory")?;
    let (root, plan) = target::resolve(&args.target, &cwd)?;
    let config = crate::cli::config(&root)?;
    let mode = Mode::of(&config);

    match lock::lock(&root, &plan, mode, config.acceptance.as_ref()) {
        Ok(locked) => {
            for warning in &locked.warnings {
                eprintln!("warning: {warning}");
            }
            print!("{}", locked.render());
            Ok(())
        }
        Err(err) => {
            if let LockError::Plan(err) = &err {
                show_problems(err, &cwd);
            }
            Err(err.into())
        }
    }
}
