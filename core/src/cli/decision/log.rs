//! `invar decision log [--ticket <T>] [--module <M>]`.
//!
//! Entries newest first, each as a header line and its indented text. An
//! entry that cannot be read is reported on stderr and skipped: the log is
//! advisory, and one hand-edited file must not hide the others
//! (decision-entry.md §9).

use anyhow::Result;

use crate::decision;
use crate::fs;

#[derive(Debug, clap::Args)]
pub struct Args {
    /// Only entries for this ticket.
    #[arg(long)]
    pub ticket: Option<String>,

    /// Only entries for this module (`global` for entries not tied to one).
    #[arg(long)]
    pub module: Option<String>,
}

pub fn run(args: Args) -> Result<()> {
    let root = fs::project_root()?;
    let Some(listed) = decision::log(&root)? else {
        println!(
            "no decision entries: {} does not exist",
            root.decisions_dir().display()
        );
        return Ok(());
    };

    let mut shown = 0;
    for item in listed {
        let entry = match item.entry {
            Ok(entry) => entry,
            Err(problems) => {
                let count = problems.len();
                let noun = if count == 1 { "problem" } else { "problems" };
                eprintln!(
                    "warning: {} is not a valid decision entry ({count} {noun}); skipped:\n{}",
                    item.path.display(),
                    problems.render(&item.path).trim_end()
                );
                continue;
            }
        };
        if args.ticket.as_ref().is_some_and(|t| *t != entry.ticket)
            || args.module.as_ref().is_some_and(|m| *m != entry.module)
        {
            continue;
        }
        if shown > 0 {
            println!();
        }
        shown += 1;
        println!(
            "{}  {}  {}  {}  {}",
            entry.date, entry.kind, entry.ticket, entry.module, entry.id
        );
        for line in entry.body.lines() {
            if line.is_empty() {
                println!();
            } else {
                println!("    {line}");
            }
        }
    }
    if shown == 0 {
        println!("no decision entries match");
    }
    Ok(())
}
