//! `invar plan audit begin | input | report` (#167, `contracts/formats/plan-audit.md`).
//!
//! `begin` is the developer's, through `/invar:plan-audit`; `input` is what the
//! plan-auditor reads; `report` is what the developer reads. None decides anything about
//! the Plan: the report computes its line from the files, and only advises.

use anyhow::{Context as _, Result};
use clap::Subcommand;

use crate::format::plan::Mode;
use crate::plan_audit::{self, AuditError};

use super::target;
use super::validate::print_report;

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Begin an audit of a draft Plan: record it as the baseline. The developer's.
    ///
    /// Refused unless the Plan is a draft and passes `plan validate`. Writes
    /// `.invar/out/<TICKET>-audit/baseline.md`, the Plan as it is, and removes the
    /// findings and report of an earlier audit. Refused when a baseline exists, unless
    /// `--restart` is given: a baseline taken again after an edit would hide that edit
    /// from the report. Prints `plan audit begun: <TICKET>`.
    Begin {
        /// A ticket id (`SHOP-214`), or the path of a Plan.
        target: String,

        /// Take a new baseline, discarding an earlier audit's.
        #[arg(long)]
        restart: bool,

        /// Report as `/invar:plan-audit` needs it: a refusal is printed on stdout as
        /// `error: …`, and the exit code is 0.
        #[arg(long)]
        skill: bool,
    },
    /// Print what the plan-auditor reads: the Plan, the binding context, the invariant
    /// ids, what it may change, the form of its findings and the blocking kinds.
    ///
    /// Nothing from the decision log. Refused before `begin`.
    Input {
        /// A ticket id (`SHOP-214`), or the path of a Plan.
        target: String,
    },
    /// Report the audit: the Plan's changes since `begin`, with the auditor's reasons,
    /// `plan validate`, the findings, and READY or NOT READY computed from them.
    ///
    /// Refused, naming each, when anything but the text under `## Implementation
    /// context` and `## Scope` changed, or the Plan is no longer a draft; nothing is
    /// restored. Writes `.invar/out/<TICKET>-audit/report.txt`, which `plan approve`
    /// reads its line from. Exits 0 for READY and NOT READY: it is an agent's
    /// assessment, not a check.
    Report {
        /// A ticket id (`SHOP-214`), or the path of a Plan.
        target: String,
    },
}

pub fn run(command: Command) -> Result<()> {
    let cwd = std::env::current_dir().context("cannot read the current directory")?;
    match command {
        Command::Begin {
            target,
            restart,
            skill,
        } => crate::cli::for_skill(
            skill,
            (|| {
                let (root, spec) = target::resolve(&target, &cwd)?;
                let mode = Mode::of(&crate::cli::config(&root)?);
                match plan_audit::begin(&root, &spec, mode, restart) {
                    Ok((plan, files)) => {
                        println!("plan audit begun: {}", plan.ticket());
                        println!("  baseline: {}", target::shown(&files.baseline, &cwd));
                        Ok(())
                    }
                    Err(AuditError::Invalid(report)) => {
                        print_report(&report, &cwd);
                        Err(AuditError::Invalid(report).into())
                    }
                    Err(err) => Err(err.into()),
                }
            })(),
        ),
        Command::Input { target } => {
            let (root, spec) = target::resolve(&target, &cwd)?;
            let config = crate::cli::config(&root)?;
            let text = plan_audit::input(
                &root,
                &spec,
                Mode::of(&config),
                config.context.binding_limit,
            )?;
            print!("{text}");
            Ok(())
        }
        Command::Report { target } => {
            let (root, spec) = target::resolve(&target, &cwd)?;
            let mode = Mode::of(&crate::cli::config(&root)?);
            print!("{}", plan_audit::report(&root, &spec, mode)?.text);
            Ok(())
        }
    }
}
