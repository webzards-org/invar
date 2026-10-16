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
    /// business rules only as the approved Plan states them (§5c). With
    /// `--hook` it is printed as the JSON a `SubagentStart` hook returns, which
    /// is how the plugin hands it to the coder.
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

    /// With `--conventions`: print them as a `SubagentStart` hook's
    /// `additionalContext`, the one channel that reaches a sub-agent
    /// (`plugin/NOTES.md` §17). Prints nothing when there are none.
    //
    // Both rules are spelled out: clap does not check `requires` once the
    // argument it requires is excluded by a conflict, so `--binding --hook`
    // passed as plain `--binding` with `requires` alone.
    #[arg(long, requires = "conventions", conflicts_with = "binding")]
    pub hook: bool,
}

/// `text` as a `SubagentStart` hook's output: one JSON line carrying it as
/// `additionalContext`, or nothing at all when there is nothing to deliver —
/// so that absent and empty conventions stay indistinguishable (§5).
///
/// The JSON form is the one the M1-22 spike saw reach a sub-agent; the hook's
/// payload on stdin is not read, because the plugin's matcher already chose
/// the agent.
pub fn subagent_start(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }
    let output = serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "SubagentStart",
            "additionalContext": text,
        }
    });
    format!("{output}\n")
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
        let text = binding.render_as_within(kind, config.context.binding_limit)?;
        if args.hook {
            print!("{}", subagent_start(&text));
        } else {
            print!("{text}");
        }
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
