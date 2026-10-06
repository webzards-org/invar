//! Argument parsing and dispatch.
//!
//! One module per subcommand. A subcommand module owns its arguments and a
//! `run` function; it does not own the logic — that lives in `digest`,
//! `format`, `gate`, `decision` and `git`. Keeping the split means a command
//! can be called from a test, from CI or from a hook and behave identically
//! (§13b).

use std::process::ExitCode;

use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::format::config;
use crate::format::plan::Mode;
use crate::fs::ProjectRoot;

mod decision;
mod hash;
mod plan;

#[derive(Debug, Parser)]
#[command(
    name = "invar",
    version,
    about = "Holds the specification fixed while code is generated against it",
    long_about = None,
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Print the canonical digest of a whole file (§4d).
    ///
    /// The whole file is hashed: its entire content is canonicalized (UTF-8,
    /// BOM, line endings, trailing whitespace) and run through SHA-256. A
    /// `Confirmed-checksum` or `Approved-checksum` covers only the body below a
    /// header, so this command run on a `.feature` or a Plan does not
    /// reproduce that field.
    Hash(hash::Args),
    /// Work with the Plan of a ticket: `.invar/plans/<TICKET>-<slug>/spec.md`.
    #[command(subcommand)]
    Plan(plan::Command),
    /// Record and read the decision log: `.invar/decisions/`.
    ///
    /// What was built and why — or why it could not be — kept after the Plan
    /// is gone, for the planner to read next time.
    #[command(subcommand)]
    Decision(decision::Command),
    // The rest of M1 (docs/TASKS-M1.md):
    //   done                             M1-12
    //   clean-spec                       M1-13
    //   context show                     M1-14
    //   init                             M1-15
    //   pr-body                          M1-16
    // Later milestones: scenario validate | confirm, generate, lint, doctor.
}

impl Cli {
    /// Run the command. Most succeed with exit 0; `plan guard` picks its own
    /// code, because a hook reads meaning into it.
    pub fn run(self) -> Result<ExitCode> {
        match self.command {
            Command::Hash(args) => hash::run(args).map(|()| ExitCode::SUCCESS),
            Command::Plan(command) => plan::run(command),
            Command::Decision(command) => decision::run(command).map(|()| ExitCode::SUCCESS),
        }
    }
}

/// Read `invar.yaml` for a command: warnings to stderr, `scenarios: true`
/// refused (invar-yaml.md §3.1).
fn mode(root: &ProjectRoot) -> Result<Mode> {
    let loaded = config::load_for_command(root)?;
    for warning in &loaded.warnings {
        eprintln!("warning: {}: {warning}", loaded.path.display());
    }
    Ok(Mode::of(&loaded.config))
}
