//! `invar decision …`: the decision log, `.invar/decisions/` (§7).

use anyhow::Result;
use clap::Subcommand;

mod log;
mod record;
mod validate;

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Write one decision entry — when the developer decides it is worth
    /// keeping.
    ///
    /// The entry goes to `.invar/decisions/<date>-<time>-<module>-<slug>.md`
    /// and is never modified afterwards. Its text comes from `--message`, or
    /// from stdin.
    ///
    /// Run it when the developer asks for an entry. An agent proposes one; it
    /// does not record one on its own, and no hook runs this command
    /// (decision-entry.md §8). `invar done` writes the `confirmed` entry that
    /// closes a cycle.
    ///
    /// `--type failed` also returns the ticket's Plan to `draft`: `Status`
    /// becomes `draft` and the approval values are cleared, the body is
    /// untouched. It refuses when the ticket has no Plan.
    Record(record::Args),

    /// Print the decision log, newest first.
    Log(log::Args),

    /// Check a draft entry: `.invar/plans/<TICKET>-<slug>/decision.md`.
    ///
    /// The draft is what the agent writes for the developer to read before an
    /// entry is recorded (`/invar:log`). It has an entry's front matter —
    /// `ticket`, `module`, `type` (`confirmed`, `failed` or `note`), optionally
    /// `scenario` — but no `id` and no `date`, which are assigned when it is
    /// recorded; and a body that says what was decided and why.
    ///
    /// Checks the front matter, the body, and that the draft sits next to its
    /// Plan and names that Plan's ticket. Every problem is reported in one
    /// run, each as `path:line: what to change`. Writes nothing; records
    /// nothing.
    Validate(validate::Args),
}

pub fn run(command: Command) -> Result<()> {
    match command {
        Command::Record(args) => record::run(args),
        Command::Log(args) => log::run(args),
        Command::Validate(args) => validate::run(args),
    }
}
