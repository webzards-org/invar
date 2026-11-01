//! `invar done [TICKET | PATH] [--commit <MESSAGE> | --no-commit] [--skill]`:
//! delete the Plan, stage the deletion, and commit it when asked — by
//! `--commit`, or by `done.commit: true` in `invar.yaml`. `--skill` is for
//! `/invar:done`, whose shell block is this one command.

use anyhow::{Context as _, Result};

use crate::done::{self, Committed, DoneError, Staged};
use crate::format::config::Done as DoneConfig;
use crate::fs;
use crate::fs::plans::parse_dir_name;
use crate::gate::check::CheckError;
use crate::git;

use super::plan::{require_approved, target};

#[derive(Debug, clap::Args)]
pub struct Args {
    /// A ticket id (`PROJ-412`), or the path of a Plan: its `spec.md` or its
    /// directory. Without it, the Plan whose ticket the current branch's name
    /// contains (`feat/PROJ-412-reserve-stock`).
    ///
    /// Anything containing `/`, ending in `.md`, or `.`/`..` is a path;
    /// everything else is looked up as a ticket under `.invar/plans/`.
    pub target: Option<String>,

    /// Also commit the deletion, alone, with this message:
    /// `--commit "chore: remove the Plan of PROJ-412"`.
    ///
    /// Only the Plan directory goes into the commit; anything else that is
    /// staged stays staged. The project's hooks, signing and identity apply.
    /// Nothing is pushed. Without it, the deletion is staged and the commit is
    /// yours — unless `invar.yaml` sets `done.commit: true`, which commits
    /// under `done.commit_message` (default "Remove plan {ticket}").
    #[arg(long, value_name = "MESSAGE", conflicts_with = "no_commit")]
    pub commit: Option<String>,

    /// Only stage the deletion, even when `invar.yaml` sets `done.commit:
    /// true`.
    #[arg(long)]
    pub no_commit: bool,

    /// Report as `/invar:done` needs it: a refusal is printed on stdout as
    /// `error: …`, and the exit code is 0.
    ///
    /// A skill's shell block that exits non-zero is dropped by Claude Code
    /// with its output, so the refusal would never reach the session.
    #[arg(long)]
    pub skill: bool,
}

pub fn run(args: Args) -> Result<()> {
    let skill = args.skill;
    super::for_skill(skill, close(args))
}

fn close(args: Args) -> Result<()> {
    let cwd = std::env::current_dir().context("cannot read the current directory")?;
    let (root, spec) = match &args.target {
        Some(target) => target::resolve(target, &cwd)?,
        None => {
            let root = fs::project_root()?;
            let branch = git::current_branch(root.path())?.ok_or(DoneError::DetachedHead)?;
            let spec = done::plan_for_branch(&root, &branch)?;
            (root, spec)
        }
    };
    let config = super::config(&root)?;
    let commit = commit_message(&args, &config.done, &spec);

    let closed = match done::close(&root, &spec, &config, commit.as_deref()) {
        Ok(closed) => closed,
        // The refusal for a Plan that is not valid in form says "fix every
        // problem listed above"; here is the list it points at.
        Err(DoneError::Check(CheckError::Invalid(report))) => {
            require_approved::print_problems(&report, &cwd);
            return Err(DoneError::Check(CheckError::Invalid(report)).into());
        }
        Err(err) => return Err(err.into()),
    };

    for warning in &closed.warnings {
        eprintln!("warning: {warning}");
    }
    let shown = target::shown(&closed.plan_dir, &cwd);
    match (&closed.staged, &closed.committed) {
        (_, Committed::Yes(message)) => {
            println!("removed: {shown} (committed: {message})");
        }
        (_, Committed::NothingTracked) => {
            println!("removed: {shown} (git never tracked it, so there was nothing to commit)");
        }
        (Staged::Yes, Committed::NotAsked) => {
            println!("removed: {shown} (staged; commit it with the rest of the branch)");
        }
        (Staged::NotARepository(said), Committed::NotAsked) => {
            println!("removed: {shown}");
            eprintln!("warning: not staged — the project is not in a git work tree ({said})");
        }
    }
    Ok(())
}

/// The message to commit the deletion under, or `None` to only stage it.
///
/// `--commit <MESSAGE>` wins, then `--no-commit`, then `done.commit` in
/// `invar.yaml` with its `done.commit_message` (invar-yaml.md §3.5). Absent
/// all three, nothing is committed — the behavior before the key existed.
fn commit_message(args: &Args, config: &DoneConfig, spec: &std::path::Path) -> Option<String> {
    if let Some(message) = &args.commit {
        return Some(message.clone());
    }
    if args.no_commit || !config.commit {
        return None;
    }
    // The ticket from the directory name: `SHOP-214-cancel-unpaid-order`. A
    // Plan always lives in one of that shape; the name itself is the fallback.
    let dir_name = spec
        .parent()
        .and_then(|dir| dir.file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ticket = parse_dir_name(&dir_name)
        .map(|(ticket, _)| ticket.to_owned())
        .unwrap_or(dir_name);
    Some(config.message_for(&ticket))
}
