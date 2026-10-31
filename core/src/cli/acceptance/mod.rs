//! `invar acceptance …`: the acceptance tests of a ticket (A-1 to A-8).

use anyhow::Result;
use clap::Subcommand;

mod spec;

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Print everything the test-writer needs to write a ticket's acceptance
    /// tests.
    ///
    /// Run by the test-writer agent, right after `invar plan check`. Prints the
    /// style, `acceptance.dir`, the `run` command, `fakes_allowed`, the conventions
    /// of the style, every criterion with its id, and the Plan's Scope and
    /// Implementation context. Never `BUSINESS_CONTEXT.md` beyond what the Plan
    /// quotes, and never the decision log. Nothing is cut for size.
    ///
    /// The same Plan and config print the same bytes. Refuses, with nothing on
    /// stdout, what `plan check` refuses; a ticket approved as `skipped`; an
    /// `invar.yaml` with no `acceptance:` section; and criteria with no ids.
    Spec(spec::Args),
}

pub fn run(command: Command) -> Result<()> {
    match command {
        Command::Spec(args) => spec::run(args),
    }
}
