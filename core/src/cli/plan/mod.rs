//! `invar plan …`: the Plan of one ticket (§4a).

use anyhow::Result;
use clap::Subcommand;

mod validate;

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Check that a Plan is well-formed: location, directory name, header,
    /// required sections, and header against directory.
    ///
    /// Checks form, not approval: a draft with empty approval fields is
    /// valid. Every problem is reported in one run, each as
    /// `path:line: what to change`. Exits non-zero when any Plan is invalid.
    Validate(validate::Args),
    // `plan approve` arrives with M1-9.
}

pub fn run(command: Command) -> Result<()> {
    match command {
        Command::Validate(args) => validate::run(args),
    }
}
