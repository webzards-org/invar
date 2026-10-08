//! `invar pr-body <TICKET | PATH> [--with-ticket] [--out <PATH>]` (M1-16).

use std::fs;
use std::path::PathBuf;

use anyhow::{Context as _, Result, bail};

use crate::format::plan::{Plan, Status};
use crate::pr_body;

use super::plan::target;

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

    /// Write the description to this file instead of stdout.
    #[arg(long)]
    pub out: Option<PathBuf>,
}

pub fn run(args: Args) -> Result<()> {
    let cwd = std::env::current_dir().context("cannot read the current directory")?;
    let (root, spec) = target::resolve(&args.target, &cwd)?;
    let mode = super::mode(&root)?;

    let plan = Plan::read(&spec, mode)?;
    let ticket = plan.header().ticket.clone();
    if plan.header().status == Status::Draft {
        bail!(
            "{}: the Plan for {ticket} is still a draft; a PR description describes an \
             approved specification. Approve it first with `invar plan approve {ticket}`",
            target::shown(&spec, &cwd)
        );
    }

    let rendered = pr_body::render(&plan, &ticket, args.with_ticket);
    match args.out {
        Some(path) => {
            fs::write(&path, &rendered)
                .with_context(|| format!("cannot write {}", path.display()))?;
            println!("{}", target::shown(&path, &cwd));
        }
        None => print!("{rendered}"),
    }
    Ok(())
}
