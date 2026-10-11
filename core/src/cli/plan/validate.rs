//! `invar plan validate <TICKET | PATH> | --all-drafts`.
//!
//! Prints `ok: <path>` for each valid Plan and `<path>:<line>: <problem>` for
//! every problem, on stdout, so an agent reads them as the command's output.
//! A failing run ends with one summary line on stderr and a non-zero exit.

use std::path::Path;

use anyhow::{Context as _, Result, bail};
use clap::ArgGroup;

use crate::fs;
use crate::gate::validate::{self, Report};

use super::target;

#[derive(Debug, clap::Args)]
#[command(group(ArgGroup::new("which").required(true).args(["target", "all_drafts"])))]
pub struct Args {
    /// A ticket id (`PROJ-412`), or the path of a Plan: its `spec.md` or its
    /// directory.
    ///
    /// Anything containing `/`, ending in `.md`, or `.`/`..` is a path;
    /// everything else is looked up as a ticket under `.invar/plans/`.
    pub target: Option<String>,

    /// Validate every Plan under `.invar/plans/` whose `Status` is not
    /// `approved`.
    #[arg(long)]
    pub all_drafts: bool,

    /// Answer as a `SubagentStop` hook, for the planner that is about to stop.
    ///
    /// Reads the hook's JSON from stdin. If a draft Plan is still not valid,
    /// the stop is blocked (exit 2) and the problems are returned as the
    /// reason, so the planner sees what to fix. It is blocked once: a planner
    /// that stops again with the problems still there is let go, and the
    /// developer is told. Anything it cannot make sense of lets the planner
    /// stop, with the problem reported.
    ///
    /// Plain `--all-drafts` prints the same problems and exits 1, which a hook
    /// would deliver to nobody: only a block reaches the agent.
    #[arg(long, requires = "all_drafts")]
    pub hook: bool,
}

pub fn run(args: Args) -> Result<()> {
    let cwd = std::env::current_dir().context("cannot read the current directory")?;

    let (root, spec) = match &args.target {
        Some(target) => {
            let (root, spec) = target::resolve(target, &cwd)?;
            (root, Some(spec))
        }
        None => (fs::project_root()?, None),
    };
    let mode = super::mode(&root)?;

    let reports = match &spec {
        Some(spec) => vec![validate::file(&root, spec, mode)],
        None => match validate::drafts(&root, mode)? {
            Some(reports) if !reports.is_empty() => reports,
            found => {
                let plans_dir = target::shown(&root.plans_dir(), &cwd);
                let why = if found.is_none() {
                    " (the directory does not exist)"
                } else {
                    ""
                };
                println!("no draft Plans in {plans_dir}{why}; nothing to validate");
                return Ok(());
            }
        },
    };

    let mut invalid = 0;
    let mut problems = 0;
    for report in &reports {
        print_report(report, &cwd);
        if !report.is_valid() {
            invalid += 1;
            problems += report.problems.len();
        }
    }
    if invalid == 0 {
        return Ok(());
    }

    let which = if reports.len() == 1 {
        format!(
            "the Plan is not valid ({})",
            count(problems, "problem", "problems")
        )
    } else {
        format!(
            "{invalid} of {} are not valid ({})",
            count(reports.len(), "Plan", "Plans"),
            count(problems, "problem", "problems")
        )
    };
    let again = match &args.target {
        Some(target) => target.clone(),
        None => "--all-drafts".to_owned(),
    };
    bail!("{which}; fix every problem listed above, then run `invar plan validate {again}` again")
}

/// `ok: <path>`, or one `<path>:<line>: <problem>` line per problem.
pub(super) fn print_report(report: &Report, cwd: &Path) {
    let path = target::shown(&report.spec, cwd);
    if report.is_valid() {
        println!("ok: {path}");
    } else {
        print!("{}", report.problems.render(Path::new(&path)));
    }
}

fn count(count: usize, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}
