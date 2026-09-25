//! Argument parsing and dispatch.
//!
//! One module per subcommand. A subcommand module owns its arguments and a
//! `run` function; it does not own the logic — that lives in `digest`,
//! `format` and (later) the modules for the gate. Keeping the split means a
//! command can be called from a test, from CI or from a hook and behave
//! identically (§13b).

use anyhow::Result;
use clap::{Parser, Subcommand};

mod hash;

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
    /// Print the canonical digest of a file (§4d).
    Hash(hash::Args),
    // The rest of the surface (§8) arrives with M1:
    //   scenario validate | confirm      M1-6, M1-10, M1-11
    //   plan validate | approve          M1-7, M1-9
    //   init                             M1-13
    //   doctor, context, generate,
    //   lint, decision, clean-spec       M2+
}

impl Cli {
    pub fn run(self) -> Result<()> {
        match self.command {
            Command::Hash(args) => hash::run(args),
        }
    }
}
