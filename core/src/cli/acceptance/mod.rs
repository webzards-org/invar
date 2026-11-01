//! `invar acceptance …`: the acceptance tests of a ticket (A-1 to A-8).

use anyhow::Result;
use clap::Subcommand;

mod lock;
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
    /// Run a ticket's pending acceptance tests, and lock them if every test checks
    /// something.
    ///
    /// Run by the test-writer agent, once it has written its tests. Finds the files
    /// under `acceptance.dir` with the ticket's `invar-lock:` line, and runs them
    /// and its already locked files through `acceptance.run`. It reads the JUnit
    /// report, never the exit code.
    ///
    /// Every criterion needs a test, every test exactly one `AC<n>` token of this
    /// Plan, and every new test has to fail on an assertion: one that passes, errors
    /// or is skipped is refused, and there is no override. All the problems are
    /// listed at once, and a refusal writes nothing.
    ///
    /// Refuses first what `invar acceptance spec` refuses. On success the last line
    /// is `locked <TICKET> <N> file(s)`.
    Lock(lock::Args),
}

pub fn run(command: Command) -> Result<()> {
    match command {
        Command::Spec(args) => spec::run(args),
        Command::Lock(args) => lock::run(args),
    }
}
