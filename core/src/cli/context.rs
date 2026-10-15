//! `invar context show [--binding | --conventions]` (M1-14, M1-23).
//!
//! What the agents read, on stdout — and nothing else there, because stdout is
//! what reaches the agent (a hook delivers it as it is). What a person needs to
//! know and the agent does not — a context file that is not recognized, a
//! decision entry that cannot be read — goes to stderr.

use anyhow::Result;
use clap::Subcommand;

use crate::context::{self, Kind};
use crate::fs;

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Print the context the agents read, each source labelled with its origin.
    ///
    /// Without a flag, this is the planner's context: the binding context, then
    /// the whole decision log (every module). Only the planner receives it; the
    /// coder must not see the log (§7).
    ///
    /// `--binding` prints `AI_CONTEXT.md` and `BUSINESS_CONTEXT.md` and
    /// nothing from the log: what a `SessionStart` hook delivers to the main
    /// session.
    ///
    /// `--conventions` prints `AI_CONTEXT.md` and nothing else: what the coder
    /// reads. Never `BUSINESS_CONTEXT.md` and never the log — the coder meets
    /// business rules only as the approved Plan states them (§5c).
    ///
    /// Both are refused above `context.binding_limit` in `invar.yaml`, because
    /// Claude Code silently moves hook output above 10,000 characters into a
    /// file the agent is not asked to read.
    ///
    /// A context file counts only when its first non-blank line is
    /// `<!-- invar-context: v1 -->`. One without it is reported on stderr and
    /// not loaded. A file holding nothing but the marker, headings and
    /// comments is treated exactly like one that is absent.
    Show(ShowArgs),
}

#[derive(Debug, clap::Args)]
pub struct ShowArgs {
    /// Print only the binding context: both context files, nothing from the
    /// decision log.
    #[arg(long)]
    pub binding: bool,

    /// Print only the conventions: `AI_CONTEXT.md`, nothing from
    /// `BUSINESS_CONTEXT.md` or the decision log. What the coder reads.
    #[arg(long, conflicts_with = "binding")]
    pub conventions: bool,
}

pub fn run(command: Command) -> Result<()> {
    match command {
        Command::Show(args) => show(args),
    }
}

fn show(args: ShowArgs) -> Result<()> {
    let root = fs::project_root()?;
    let config = super::config(&root)?;
    let binding = context::load_binding(&root)?;

    let kind = match (args.binding, args.conventions) {
        (_, true) => Some(Kind::Conventions),
        (true, false) => Some(Kind::Binding),
        (false, false) => None,
    };

    // Only the files this output carries are reported: `--conventions` has
    // nothing to say about `BUSINESS_CONTEXT.md`, not even its name.
    for name in binding.unrecognized_in(kind.unwrap_or(Kind::Binding)) {
        eprintln!(
            "warning: {name} exists but is not recognized, so it is not loaded: its first \
             non-blank line must be `{}`",
            context::MARKER
        );
    }

    if let Some(kind) = kind {
        print!(
            "{}",
            binding.render_as_within(kind, config.context.binding_limit)?
        );
        return Ok(());
    }

    let decisions = context::load_decisions(&root)?;
    for (path, problems) in &decisions.unreadable {
        eprintln!(
            "warning: {} is not a valid decision entry; skipped:\n{}",
            path.display(),
            problems.render(path).trim_end()
        );
    }

    let text = context::planner_output(&binding, &decisions);
    if text.is_empty() {
        println!("nothing to show: no context file is loaded, and the decision log has no entries");
    } else {
        print!("{text}");
    }
    Ok(())
}
