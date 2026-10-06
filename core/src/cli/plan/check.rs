//! `invar plan check <TICKET | PATH>`.
//!
//! `ok: <path> — …` on stdout when the approval holds; otherwise one error on
//! stderr that says what happened and what to do, and a non-zero exit.

use anyhow::{Context as _, Result};

use crate::gate::check::{self, CheckError};

use super::target;
use super::validate::print_report;

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
    let (root, spec) = target::resolve(&args.target, &cwd)?;
    let mode = super::mode(&root)?;

    match check::check(&root, &spec, mode) {
        Ok(approved) => {
            println!(
                "ok: {} — approved by {} on {}, unchanged since ({})",
                target::shown(&approved.spec, &cwd),
                approved.by,
                approved.at,
                approved.checksum
            );
            Ok(())
        }
        Err(CheckError::Invalid(report)) => {
            print_report(&report, &cwd);
            Err(CheckError::Invalid(report).into())
        }
        Err(err) => Err(err.into()),
    }
}
