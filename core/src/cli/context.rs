//! `invar context show [--binding] [--module <m>]` (M1-14).
//!
//! What the agents read, on stdout — and nothing else there, because stdout is
//! what reaches the agent (a `SessionStart` hook delivers it as it is). What a
//! person needs to know and the agent does not — a context file that is not
//! recognized, a decision entry that cannot be read — goes to stderr.

use anyhow::Result;
use clap::Subcommand;

use crate::context;
use crate::fs;

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Print the context the agents read, each source labelled with its origin.
    ///
    /// Without `--binding`, this is the planner's context: the binding
    /// context, then the decision log. Only the planner receives it; the coder
    /// must not see the log (§7).
    ///
    /// `--binding` prints `AI_CONTEXT.md` and `BUSINESS_CONTEXT.md` and
    /// nothing from the log: what a `SessionStart` hook delivers to every
    /// session. It is an error for that output to exceed
    /// `context.binding_limit` in `invar.yaml`, because Claude Code silently
    /// moves hook output above 10,000 characters into a file the agent is not
    /// asked to read.
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
    #[arg(long, conflicts_with = "module")]
    pub binding: bool,

    /// The module whose decision entries come first, newest first; the
    /// `global` entries always follow.
    ///
    /// Without it only the `global` entries are shown, and the output says
    /// which other modules have entries. The log is read within a budget of 50
    /// entries or about 4000 tokens; older entries are left out.
    #[arg(long, value_name = "MODULE")]
    pub module: Option<String>,
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

    for name in binding.unrecognized() {
        eprintln!(
            "warning: {name} exists but is not recognized, so it is not loaded: its first \
             non-blank line must be `{}`",
            context::MARKER
        );
    }

    if args.binding {
        print!("{}", binding.render_within(config.context.binding_limit)?);
        return Ok(());
    }

    let decisions = context::load_decisions(&root, args.module.as_deref())?;
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
