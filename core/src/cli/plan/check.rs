//! `invar plan check <TICKET | PATH>`.
//!
//! `ok: <path> — …` and `acceptance: required|skipped` on stdout when the
//! approval and the acceptance locks hold (`acceptance-lock.md` §10); otherwise
//! an error on stderr that says what happened and what to do, and a non-zero exit.

use anyhow::{Context as _, Result};

use crate::acceptance::verify::{self, Stage};
use crate::format::plan::Mode;
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
    let config = crate::cli::config(&root)?;
    let mode = Mode::of(&config);

    match check::check(&root, &spec, mode) {
        Ok(approved) => {
            // Before the lines below, so that they are only ever there when it holds.
            let report = verify::for_ticket(&root, &config, &approved, Stage::Working);
            if !report.holds() {
                return Err(report.into());
            }
            for warning in &report.warnings {
                eprintln!("warning: {warning}");
            }
            println!(
                "ok: {} — approved by {} on {}, unchanged since ({})",
                target::shown(&approved.spec, &cwd),
                approved.by,
                approved.at,
                approved.checksum
            );
            // Fixed and parseable: the dispatcher reads the decision from here
            // (plan-header.md §5.3).
            println!("acceptance: {}", approved.acceptance.as_str());
            Ok(())
        }
        Err(CheckError::Invalid(report)) => {
            print_report(&report, &cwd);
            Err(CheckError::Invalid(report).into())
        }
        Err(err) => Err(err.into()),
    }
}
