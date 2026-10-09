//! `invar clean-spec [TICKET] [--merged] [--force]` (M1-13).
//!
//! A dry run unless `--force` is given: one line per Plan, saying what would
//! happen to it and why, on stdout. A Plan the run was *named* for and that is
//! refused — it has no decision entry — is an error, with a non-zero exit: the
//! caller asked for that Plan, and a quiet success would say it was dealt
//! with. A Plan refused in a run over everything is information, not failure.

use std::path::Path;

use anyhow::{Context as _, Result, bail};

use crate::clean_spec::{self, Assessment, Item, Keep, Scope, Verdict};
use crate::format::plan::{Mode, Status};
use crate::fs;
use crate::git;

use super::shown;

#[derive(Debug, clap::Args)]
pub struct Args {
    /// Only this ticket's Plan: a ticket id such as `PROJ-412`, or any whole
    /// segment of its directory name.
    pub ticket: Option<String>,

    /// Show what would be deleted, and why, and delete nothing. This is the
    /// default; the flag only says so.
    #[arg(long, conflicts_with = "force")]
    pub dry_run: bool,

    /// Delete the Plans listed as eligible. Without it nothing is deleted.
    #[arg(long)]
    pub force: bool,

    /// Only Plans whose ticket names a branch already merged into the base
    /// branch.
    ///
    /// The base branch is `git.base_branch` in `invar.yaml`, else the branch
    /// `origin/HEAD` points to, else `main`. A Plan whose ticket names no
    /// merged branch is skipped; one that names several makes the whole run
    /// fail, listing them. A merged branch deleted everywhere (locally and on
    /// the remote) cannot be found.
    #[arg(long)]
    pub merged: bool,
}

pub fn run(args: Args) -> Result<()> {
    let cwd = std::env::current_dir().context("cannot read the current directory")?;
    let root = fs::project_root()?;
    let config = super::config(&root)?;
    let mode = Mode::of(&config);

    // `--merged` asks git once: which branches are already in the base.
    let merged = if args.merged {
        let base = git::base_branch(root.path(), config.git.base_branch.as_deref())?;
        let branches = git::merged_branches(root.path(), &base).with_context(|| {
            format!(
                "cannot list the branches merged into `{base}`; if that is not your base \
                 branch, set `git.base_branch` in invar.yaml"
            )
        })?;
        Some((base, branches))
    } else {
        None
    };
    let scope = Scope {
        ticket: args.ticket.as_deref(),
        merged: merged.as_ref().map(|(_, branches)| branches.as_slice()),
    };

    let assessment = clean_spec::assess(&root, scope, mode)?;
    if args.force {
        clean_spec::delete(&root, &assessment)?;
    }

    let base = merged.as_ref().map(|(base, _)| base.as_str());
    report(&assessment, base, args.force, &cwd);

    // Named, and refused: that is an error, whatever else was listed.
    if args.ticket.is_some()
        && let Some(item) = assessment.items.first()
        && let Verdict::Keep(keep) = &item.verdict
        && keep.is_refusal()
    {
        bail!(
            "nothing was deleted: {} is kept because {}",
            item.name,
            keep.summary()
        );
    }
    Ok(())
}

/// What happened, or what would: one line per Plan, then a summary.
fn report(assessment: &Assessment, base: Option<&str>, forced: bool, cwd: &Path) {
    let plans_dir = shown(&assessment.plans_dir, cwd);
    if !assessment.plans_dir_exists {
        println!("no Plans: {plans_dir} does not exist");
        return;
    }
    if assessment.items.is_empty() {
        println!("no Plans in {plans_dir}");
        return;
    }

    for item in &assessment.items {
        let action = match (&item.verdict, forced) {
            (Verdict::Delete { .. }, true) => "deleted",
            (Verdict::Delete { .. }, false) => "would delete",
            (Verdict::Keep(_), _) => "kept",
        };
        println!(
            "{action}: {} ({}) — {}",
            shown(&item.path, cwd),
            status(item),
            reason(item, base)
        );
    }

    if assessment.unreadable_entries > 0 {
        let n = assessment.unreadable_entries;
        eprintln!(
            "warning: {n} decision {} could not be read. A Plan whose entry is among \
             them is reported as having none; `invar decision log` lists the ones that \
             cannot be read",
            if n == 1 { "entry" } else { "entries" }
        );
    }

    let eligible = assessment.eligible();
    let plans = |n: usize| format!("{n} {}", if n == 1 { "Plan" } else { "Plans" });
    match (forced, eligible) {
        (_, 0) => println!("nothing to delete"),
        (true, n) => println!("deleted {}", plans(n)),
        (false, n) => println!(
            "dry run: nothing was deleted. Run again with `--force` to delete the {}",
            plans(n)
        ),
    }
}

fn status(item: &Item) -> &'static str {
    match &item.status {
        Some(Status::Draft) => "draft",
        Some(Status::Approved) => "approved",
        None => "unreadable",
    }
}

/// Why a Plan is, or is not, going: worded so that whoever reads it — a
/// person or an agent — knows what to do next.
fn reason(item: &Item, base: Option<&str>) -> String {
    let ticket = item.ticket.as_deref().unwrap_or("its ticket");
    match &item.verdict {
        Verdict::Delete { entry, branch } => {
            let day = entry.date.get(..10).unwrap_or(&entry.date);
            let mut text = format!("decision entry {} ({}, {day})", entry.id, entry.kind);
            if let (Some(branch), Some(base)) = (branch, base) {
                text.push_str(&format!("; branch `{branch}` is merged into `{base}`"));
            }
            text
        }
        Verdict::Keep(Keep::UnknownTicket) => "its directory name is not `<TICKET>-<slug>`, so \
             its ticket, and with it its decision entry, cannot be told; rename the \
             directory, or remove it by hand"
            .to_owned(),
        Verdict::Keep(Keep::NoDecisionEntry) => format!(
            "no decision entry for {ticket}. A Plan with none was never closed properly, and \
             that is a signal, not garbage: record one first — `invar decision record \
             --ticket {ticket} --type failed` (or `--type note`) — then clean"
        ),
        Verdict::Keep(Keep::NotMerged) => format!(
            "no branch merged into `{}` names {ticket}",
            base.unwrap_or("the base branch")
        ),
        Verdict::Keep(Keep::ApprovedUnscoped) => format!(
            "approved, so its cycle may still be running; name it \
             (`invar clean-spec {ticket} --force`) or use `--merged` to delete it"
        ),
    }
}
