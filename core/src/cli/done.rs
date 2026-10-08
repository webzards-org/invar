//! `invar done <TICKET | PATH> [--message TEXT] [--with-ticket]` (M1-12).

use std::ffi::OsStr;
use std::path::{Component, Path};

use anyhow::{Context as _, Result};

use crate::done::{self, Request};
use crate::fs::ProjectRoot;
use crate::git;

use super::plan::target;

#[derive(Debug, clap::Args)]
pub struct Args {
    /// A ticket id (`PROJ-412`), or the path of a Plan: its `spec.md` or its
    /// directory.
    ///
    /// Anything containing `/`, ending in `.md`, or `.`/`..` is a path;
    /// everything else is looked up as a ticket under `.invar/plans/`.
    pub target: String,

    /// The decision entry's text — what was actually built, including
    /// deviations. Without it, read from stdin.
    #[arg(long, short)]
    pub message: Option<String>,

    /// Include the pasted `## Ticket` text in the PR description.
    #[arg(long)]
    pub with_ticket: bool,
}

pub fn run(args: Args) -> Result<()> {
    let cwd = std::env::current_dir().context("cannot read the current directory")?;
    let (root, spec) = target::resolve(&args.target, &cwd)?;
    let mode = super::mode(&root)?;
    let now = git::now(root.path())?;

    warn_on_uncommitted(&root, &cwd);

    let body = match args.message {
        Some(message) => message,
        None => super::read_stdin()?,
    };

    let request = Request {
        spec,
        body,
        with_ticket: args.with_ticket,
    };
    let closed = done::close(&root, request, mode, now)?;

    println!(
        "pr description: {}",
        target::shown(&closed.pr_description, &cwd)
    );
    println!(
        "recorded: {} ({}, {}, {})",
        target::shown(&closed.entry.path, &cwd),
        closed.entry.entry.id,
        closed.entry.entry.kind,
        closed.entry.entry.module
    );
    println!("removed: {}", target::shown(&closed.plan_dir, &cwd));
    Ok(())
}

/// Uncommitted changes outside `.invar/` are a warning, never a refusal
/// (`docs/TASKS-M1.md` M1-12): the developer may legitimately close a cycle
/// and commit everything into the same PR. Any failure to ask git — no
/// repository, `git` missing — is silence, not an error: the check is purely
/// advisory and must not get in the way of closing a cycle that does not need
/// it.
fn warn_on_uncommitted(root: &ProjectRoot, cwd: &Path) {
    let Ok(paths) = git::uncommitted(root.path()) else {
        return;
    };
    let outside: Vec<String> = paths
        .iter()
        .filter(|path| !under_invar(path))
        .map(|path| target::shown(&root.path().join(path), cwd))
        .collect();
    if outside.is_empty() {
        return;
    }
    eprintln!(
        "warning: {} outside .invar/ {} uncommitted: {}. The decision entry will describe \
         what was built regardless of what is committed yet — commit everything into the same \
         PR before you open it.",
        outside.len(),
        if outside.len() == 1 {
            "change is"
        } else {
            "changes are"
        },
        outside.join(", ")
    );
}

fn under_invar(path: &Path) -> bool {
    path.components().next() == Some(Component::Normal(OsStr::new(".invar")))
}
