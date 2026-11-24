//! `invar acceptance verify [<TICKET | PATH>]` (AT-9).
//!
//! With no ticket it is what CI runs: it needs no Plan and never runs git, so it works
//! after the Plan is deleted and in a shallow clone (D-17). With a ticket it makes the
//! check `done` makes, and deletes nothing. `--results` adds whether the locked tests
//! passed in the team's own JUnit reports (§10.5), once the locks hold.

use anyhow::{Context as _, Result};

use crate::acceptance::results;
use crate::acceptance::unlock;
use crate::acceptance::verify::{self, Report, Stage};
use crate::cli::plan::require_approved;
use crate::cli::plan::target;
use crate::cli::results::ResultsArgs;
use crate::cycle;
use crate::format::plan::Mode;
use crate::fs;
use crate::gate::check::{self, CheckError};

#[derive(Debug, clap::Args)]
pub struct Args {
    /// A ticket id (`PROJ-412`), or the path of a Plan: its `spec.md` or its
    /// directory. Without it, the whole of `acceptance.dir`, with no Plan and no git.
    ///
    /// Anything containing `/`, ending in `.md`, or `.`/`..` is a path;
    /// everything else is looked up as a ticket under `.invar/plans/`.
    pub target: Option<String>,

    /// With a ticket: the branch or commit the branch rule compares with, in CI the pull
    /// request's target, such as `origin/main`. Without it, the branch `origin/HEAD` points
    /// to, else `main`; never `git.base_branch`, which the branch being checked could set
    /// to itself. Without a ticket nothing is compared, and it is not read.
    #[arg(long, value_name = "REF")]
    pub base: Option<String>,

    #[command(flatten)]
    pub results: ResultsArgs,
}

pub fn run(args: Args) -> Result<()> {
    let cwd = std::env::current_dir().context("cannot read the current directory")?;
    // A path finds its own project; a ticket's Plan is looked up only once there is a
    // section to check it against, because no section is an answer of its own (D-21).
    let root = match &args.target {
        Some(path) if target::is_path(path) => target::resolve(path, &cwd)?.0,
        _ => fs::project_root()?,
    };
    let config = crate::cli::config(&root)?;
    let reports = args.results.reports(&cwd)?;
    if reports.is_some() && config.acceptance.is_none() {
        anyhow::bail!("{}", results::NO_SECTION);
    }
    let Some(acceptance) = &config.acceptance else {
        eprintln!(
            "warning: invar.yaml has no `acceptance:` section, so there is nothing to verify"
        );
        return Ok(());
    };

    let mut plan_dir = None;
    let mut ticket = None;
    let report: Report = match &args.target {
        None => verify::whole_directory(&root, acceptance),
        Some(target) => {
            let (_, spec) = target::resolve(target, &cwd)?;
            plan_dir = spec.parent().map(ToOwned::to_owned);
            match check::check(&root, &spec, Mode::of(&config)) {
                Ok(approved) => {
                    ticket = Some(approved.ticket.clone());
                    // A CI command: the base is `--base`, or found, never the branch's own
                    // `git.base_branch` (invar-yaml.md §3.3.1).
                    if let Ok(found) = cycle::ci_base(root.path(), args.base.as_deref())
                        && let Some(note) =
                            cycle::base_note(root.path(), args.base.as_deref(), &found)
                    {
                        eprintln!("note: {note}");
                    }
                    verify::for_ticket(
                        &root,
                        &config,
                        &approved,
                        Stage::Closing,
                        args.base.as_deref(),
                    )
                }
                Err(CheckError::Invalid(report)) => {
                    require_approved::print_problems(&report, &cwd);
                    return Err(CheckError::Invalid(report).into());
                }
                Err(err) => return Err(err.into()),
            }
        }
    };

    if !report.holds() {
        return Err(report.into());
    }
    // Only once the locks hold: the results of a changed test say nothing of the locked one.
    let passed = match &reports {
        None => None,
        Some(reports) => Some(results::verify(
            &root,
            &acceptance.dir,
            ticket.as_deref(),
            reports,
        )?),
    };
    for warning in &report.warnings {
        eprintln!("warning: {warning}");
    }
    // On stderr: the main session reads stdout, and it is one line (§10.4, D-31).
    match plan_dir.as_deref().map(unlock::said).transpose() {
        Ok(said) => said
            .unwrap_or_default()
            .iter()
            .for_each(|line| eprintln!("{line}")),
        Err(err) => eprintln!("warning: {err}"),
    }
    match passed {
        None => println!("verified {} locked file(s)", report.locked),
        Some(verified) => {
            for note in &verified.notes {
                eprintln!("note: {note}");
            }
            println!(
                "verified {} locked file(s), and {} locked test(s) passed",
                report.locked, verified.tests
            );
        }
    }
    Ok(())
}
