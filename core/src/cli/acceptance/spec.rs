//! `invar acceptance spec <TICKET | PATH>` (AT-7).
//!
//! The output on stdout, and nothing else: a refusal leaves stdout empty, so that
//! what the test-writer reads there is always a whole spec.

use std::path::Path;

use anyhow::{Context as _, Result};

use crate::acceptance::spec::{self, SpecError};
use crate::cli::plan::target;
use crate::format::plan::Mode;
use crate::gate::check::CheckError;

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

    match spec::build(&root, &plan, mode, config.acceptance.as_ref()) {
        Ok(output) => {
            print!("{output}");
            Ok(())
        }
        Err(err) => {
            // `plan check` lists these on stdout, and its error points at the list.
            if let SpecError::NotApproved(CheckError::Invalid(report)) = &err {
                let shown = target::shown(&report.spec, &cwd);
                eprint!("{}", report.problems.render(Path::new(&shown)));
            }
            Err(err.into())
        }
    }
}
