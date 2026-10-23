//! `invar decision approve [TICKET | PATH] [--skill]`: record the draft entry
//! the developer has read (decision-entry.md §8.1), and remove the draft.
//!
//! Without a ticket, the one draft under `.invar/plans/`; with several, it
//! names them and records none. `--skill` is for `/invar:log-approve`, whose
//! shell block is this one command.
//!
//! The developer's gate on the decision log. It is run by `/invar:log-approve`,
//! a skill the model cannot invoke, and `plan guard` denies it to the agent.
//!
//! Prints `recorded: <path> (<id>, <type>, <module>)`, then `reset: …` for a
//! `failed` entry, as `decision record` does. A draft that is not valid is
//! reported like `decision validate` reports it — `<path>:<line>: <problem>` on
//! stdout — and nothing is written.

use std::path::Path;

use anyhow::{Context as _, Result, bail};

use crate::cli::shown;
use crate::decision::draft::{self, ApproveError};
use crate::format::decision::DRAFT_FILE;
use crate::fs;
use crate::git;

#[derive(Debug, clap::Args)]
pub struct Args {
    /// A ticket id (`PROJ-412`), or the path of a draft: its `decision.md`, or
    /// the Plan directory that holds it. Without it, the one draft under
    /// `.invar/plans/`; when there are several, they are listed and none is
    /// recorded.
    pub target: Option<String>,

    /// Report as `/invar:log-approve` needs it: a refusal is printed on stdout
    /// as `error: …`, and the exit code is 0.
    ///
    /// A skill's shell block that exits non-zero is dropped by Claude Code
    /// with its output, so the refusal would never reach the session.
    #[arg(long)]
    pub skill: bool,
}

pub fn run(args: Args) -> Result<()> {
    let skill = args.skill;
    crate::cli::for_skill(skill, approve(args))
}

fn approve(args: Args) -> Result<()> {
    let cwd = std::env::current_dir().context("cannot read the current directory")?;
    let query = match args.target {
        Some(target) => target,
        None => crate::cli::plan::target::only(
            &fs::project_root()?,
            DRAFT_FILE,
            "there is no draft entry in .invar/plans/ to record; draft one with `/invar:log`",
            "drafts are waiting; name the one to record: `invar decision approve <ticket>` \
             (in Claude Code, `/invar:log-approve <ticket>`)",
        )?,
    };
    let (root, path) = super::validate::resolve(&query, &cwd)?;
    let mode = crate::cli::mode(&root)?;
    let now = git::now(root.path())?;

    let approved = match draft::approve(&root, &path, now, mode) {
        Ok(approved) => approved,
        Err(ApproveError::Invalid(report)) => {
            let shown_path = shown(&report.path, &cwd);
            print!("{}", report.problems.render(Path::new(&shown_path)));
            let count = report.problems.len();
            bail!(
                "the draft is not valid ({count} {}), so nothing was recorded; fix every \
                 problem listed above — by hand, or with `/invar:log` — then approve it again",
                if count == 1 { "problem" } else { "problems" },
            );
        }
        Err(err) => return Err(err.into()),
    };

    let recorded = &approved.recorded;
    println!(
        "recorded: {} ({}, {}, {})",
        shown(&recorded.path, &cwd),
        recorded.entry.id,
        recorded.entry.kind,
        recorded.entry.module
    );
    if let Some(spec) = &recorded.reset {
        println!(
            "reset: {} is `draft` again — `Status`, `Approved-by`, `Approved-at` and \
             `Approved-checksum` cleared, the body unchanged. Back to Phase 1: revise the Plan, \
             then the developer approves it again",
            shown(spec, &cwd)
        );
    }
    let draft_shown = shown(&approved.draft, &cwd);
    match &approved.left_behind {
        None => {
            println!("removed: {draft_shown}");
            Ok(())
        }
        Some(err) => bail!(
            "the entry is recorded, but the draft {draft_shown} could not be removed ({err}); \
             delete it by hand, and do not approve it again — that would record it twice"
        ),
    }
}
