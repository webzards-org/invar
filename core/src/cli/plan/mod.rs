//! `invar plan …`: the Plan of one ticket (§4a).

use anyhow::Result;
use clap::Subcommand;

use crate::format::config;
use crate::format::plan::Mode;
use crate::fs::ProjectRoot;

mod approve;
mod target;
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

    /// Approve a Plan: the human gate.
    ///
    /// Validates the Plan, prints its acceptance criteria, then sets
    /// `Status: approved` and records `Approved-by` (`git config user.name`),
    /// `Approved-at` (today) and `Approved-checksum` (the digest of the body).
    /// Only those four values change; the body stays byte-identical.
    ///
    /// An approved Plan whose body changed since is refused unless
    /// `--reapprove` is given; one that is unchanged is left as it is.
    Approve(approve::Args),
}

pub fn run(command: Command) -> Result<()> {
    match command {
        Command::Validate(args) => validate::run(args),
        Command::Approve(args) => approve::run(args),
    }
}

/// Read `invar.yaml` for a plan command: warnings to stderr, `scenarios: true`
/// refused (invar-yaml.md §3.1).
fn mode(root: &ProjectRoot) -> Result<Mode> {
    let loaded = config::load_for_command(root)?;
    for warning in &loaded.warnings {
        eprintln!("warning: {}: {warning}", loaded.path.display());
    }
    Ok(Mode::of(&loaded.config))
}
