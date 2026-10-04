//! `invar plan validate <TICKET | PATH> | --all-drafts`.
//!
//! Prints `ok: <path>` for each valid Plan and `<path>:<line>: <problem>` for
//! every problem, on stdout, so an agent reads them as the command's output.
//! A failing run ends with one summary line on stderr and a non-zero exit.

use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};
use clap::ArgGroup;

use crate::format::config;
use crate::format::plan::Mode;
use crate::fs::plans::SPEC_FILE;
use crate::fs::{self, ProjectRoot};
use crate::gate::validate::{self, Report};

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
}

pub fn run(args: Args) -> Result<()> {
    let cwd = std::env::current_dir().context("cannot read the current directory")?;

    let (root, target) = match &args.target {
        Some(target) if is_path(target) => {
            let spec = spec_path(&cwd.join(target))?;
            let root = ProjectRoot::discover_from(spec.parent().unwrap_or(&spec))?;
            (root, Target::Path(spec))
        }
        Some(ticket) => (fs::project_root()?, Target::Ticket(ticket.clone())),
        None => (fs::project_root()?, Target::AllDrafts),
    };
    let loaded = config::load_for_command(&root)?;
    for warning in &loaded.warnings {
        eprintln!("warning: {}: {warning}", loaded.path.display());
    }
    let mode = Mode::of(&loaded.config);

    let reports = match &target {
        Target::Path(spec) => vec![validate::file(&root, spec, mode)],
        Target::Ticket(ticket) => vec![validate::ticket(&root, ticket, mode)?],
        Target::AllDrafts => match validate::drafts(&root, mode)? {
            Some(reports) if !reports.is_empty() => reports,
            found => {
                let plans_dir = shown(&root.plans_dir(), &cwd);
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

    let noun = |count: usize, one: &str, many: &str| {
        format!("{count} {}", if count == 1 { one } else { many })
    };
    let which = if reports.len() == 1 {
        format!(
            "the Plan is not valid ({})",
            noun(problems, "problem", "problems")
        )
    } else {
        format!(
            "{invalid} of {} are not valid ({})",
            noun(reports.len(), "Plan", "Plans"),
            noun(problems, "problem", "problems")
        )
    };
    let again = match &args.target {
        Some(target) => target.clone(),
        None => "--all-drafts".to_owned(),
    };
    bail!("{which}; fix every problem listed above, then run `invar plan validate {again}` again")
}

enum Target {
    Path(PathBuf),
    Ticket(String),
    AllDrafts,
}

fn is_path(target: &str) -> bool {
    target.contains(['/', '\\']) || target.ends_with(".md") || target == "." || target == ".."
}

/// A directory means its `spec.md`. The result is absolute and canonical as
/// far as it exists, so it compares with the project root.
fn spec_path(path: &Path) -> Result<PathBuf> {
    let spec = if path.is_dir() {
        path.join(SPEC_FILE)
    } else {
        path.to_owned()
    };
    let parent = spec.parent().unwrap_or(&spec);
    let parent = parent
        .canonicalize()
        .with_context(|| format!("{} does not exist", parent.display()))?;
    Ok(match spec.file_name() {
        Some(name) => parent.join(name),
        None => parent,
    })
}

fn print_report(report: &Report, cwd: &Path) {
    let path = shown(&report.spec, cwd);
    if report.is_valid() {
        println!("ok: {path}");
    } else {
        print!("{}", report.problems.render(Path::new(&path)));
    }
}

/// Relative to the current directory when the file is under it, so the
/// paths an agent sees are the ones it would type.
fn shown(path: &Path, cwd: &Path) -> String {
    let cwd = cwd.canonicalize().unwrap_or_else(|_| cwd.to_owned());
    path.strip_prefix(&cwd)
        .unwrap_or(path)
        .display()
        .to_string()
}
