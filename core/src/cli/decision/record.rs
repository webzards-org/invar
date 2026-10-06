//! `invar decision record --ticket <T> --type <TYPE> [--module <M>] [--message <TEXT>]`.

use std::io::{IsTerminal as _, Read as _};

use anyhow::{Context as _, Result, bail};

use crate::decision::{self, Request};
use crate::format::decision::Type;
use crate::fs;
use crate::git;

#[derive(Debug, clap::Args)]
pub struct Args {
    /// The ticket the entry belongs to, such as `PROJ-412`.
    #[arg(long)]
    pub ticket: String,

    /// `correction`, `failed`, `note` — or `confirmed`, which `invar done`
    /// normally writes when it closes a cycle.
    #[arg(long = "type", value_name = "TYPE", value_parser = ["confirmed", "correction", "failed", "note"])]
    pub kind: String,

    /// The module the entry is about, or `global`. Read from the ticket's Plan
    /// when it has one; required when it has none.
    #[arg(long)]
    pub module: Option<String>,

    /// The entry's text. Without it, the text is read from stdin.
    #[arg(long, short)]
    pub message: Option<String>,
}

pub fn run(args: Args) -> Result<()> {
    let body = match args.message {
        Some(message) => message,
        None => {
            let stdin = std::io::stdin();
            if stdin.is_terminal() {
                bail!(
                    "no text for the entry: give it with `--message \"…\"`, or pipe it on \
                     stdin"
                );
            }
            let mut body = String::new();
            stdin
                .lock()
                .read_to_string(&mut body)
                .context("cannot read the entry's text from stdin")?;
            body
        }
    };

    let root = fs::project_root()?;
    let mode = crate::cli::mode(&root)?;
    let now = git::now(root.path())?;
    let request = Request {
        ticket: args.ticket,
        kind: Type::parse(&args.kind).expect("clap allows only the four types"),
        module: args.module,
        body,
    };
    let recorded = decision::record(&root, request, now, mode)?;

    let cwd = std::env::current_dir().unwrap_or_default();
    let shown = |path: &std::path::Path| {
        path.strip_prefix(cwd.canonicalize().unwrap_or(cwd.clone()))
            .unwrap_or(path)
            .display()
            .to_string()
    };
    println!(
        "recorded: {} ({}, {}, {})",
        shown(&recorded.path),
        recorded.entry.id,
        recorded.entry.kind,
        recorded.entry.module
    );
    if let Some(spec) = recorded.reset {
        println!(
            "reset: {} is `draft` again — `Status`, `Approved-by`, `Approved-at` and \
             `Approved-checksum` cleared, the body unchanged. Back to Phase 1: revise the Plan, \
             then the developer approves it again",
            shown(&spec)
        );
    }
    Ok(())
}
