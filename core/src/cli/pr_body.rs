//! `invar pr-body <TICKET | PATH> [--with-ticket] [--out <PATH>]` (M1-16).

use std::fs;
use std::path::PathBuf;

use anyhow::{Context as _, Result};

use crate::format::plan::{Mode, Plan};
use crate::pr_body;

use super::plan::{require_approved, target};

#[derive(Debug, clap::Args)]
pub struct Args {
    /// A ticket id (`PROJ-412`), or the path of a Plan: its `spec.md` or its
    /// directory.
    ///
    /// Anything containing `/`, ending in `.md`, or `.`/`..` is a path;
    /// everything else is looked up as a ticket under `.invar/plans/`.
    pub target: String,

    /// Include the pasted `## Ticket` text. Left out by default — the
    /// tracker already holds it.
    #[arg(long)]
    pub with_ticket: bool,

    /// Write the description to this file instead of stdout, creating its
    /// directory if there is none. Prints the path it wrote.
    #[arg(long)]
    pub out: Option<PathBuf>,
}

pub fn run(args: Args) -> Result<()> {
    let cwd = std::env::current_dir().context("cannot read the current directory")?;
    let (root, spec) = target::resolve(&args.target, &cwd)?;
    let config = super::config(&root)?;
    let mode = Mode::of(&config);

    // A PR description describes an approved specification: one that has been
    // approved, and has not changed since. That is the question `plan check`
    // asks, and this asks it too, in the same words. A lighter
    // rule — refusing only a draft — would let a Plan edited after approval be
    // described under an "Approved by …" line it no longer has any right to.
    require_approved::require(&root, &spec, mode, &cwd).context(
        "a PR description describes an approved specification, and this Plan is not one",
    )?;

    let plan = Plan::read(&spec, mode)?;
    let ticket = plan.header().ticket.clone();

    let plan_dir = spec.parent().unwrap_or(&spec);
    let (tests, warnings) = pr_body::acceptance::gather(&root, &config, &plan, plan_dir)?;
    for warning in &warnings {
        eprintln!("warning: {warning}");
    }

    let rendered = pr_body::render(&plan, &ticket, args.with_ticket, &tests);
    match args.out {
        Some(path) => {
            // `.invar/out/` is the usual place, and `init` does not create
            // it: a description asked for there must not fail on that.
            if let Some(parent) = path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
            {
                fs::create_dir_all(parent)
                    .with_context(|| format!("cannot create {}", parent.display()))?;
            }
            fs::write(&path, &rendered)
                .with_context(|| format!("cannot write {}", path.display()))?;
            println!("{}", target::shown(&path, &cwd));
        }
        None => print!("{rendered}"),
    }
    Ok(())
}
