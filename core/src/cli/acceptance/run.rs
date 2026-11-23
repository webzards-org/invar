//! `invar acceptance run <TICKET | PATH>` (#156): the ticket's locked files through
//! `acceptance.run`, and whether they pass.
//!
//! The verdict and what failed go to stdout, which ends with the fixed line the protocol
//! skill keys on. A refusal leaves stdout empty, so that the line is only ever there when
//! the tests ran.

use std::path::Path;

use anyhow::{Context as _, Result, anyhow};

use crate::acceptance::results::run::{self, RunLockedError};
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

    match run::run(&root, &plan, mode, config.acceptance.as_ref()) {
        Ok(ran) => {
            print!("{}", ran.output);
            if ran.green {
                Ok(())
            } else {
                Err(anyhow!(
                    "the locked acceptance tests of {} did not all pass; what did not is above",
                    ran.ticket
                ))
            }
        }
        Err(err) => {
            // The list the error points at, on stderr: stdout stays empty on a refusal.
            if let RunLockedError::NotApproved(CheckError::Invalid(report)) = &err {
                let shown = target::shown(&report.spec, &cwd);
                eprint!("{}", report.problems.render(Path::new(&shown)));
            }
            Err(err.into())
        }
    }
}
