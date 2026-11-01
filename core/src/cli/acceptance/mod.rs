//! `invar acceptance …`: the acceptance tests of a ticket (A-1 to A-8).

use anyhow::Result;
use clap::Subcommand;

mod lock;
mod spec;
mod unlock;
mod verify;

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Print everything the test-writer needs to write a ticket's acceptance
    /// tests.
    ///
    /// Run by the test-writer agent, right after `invar plan check`. Prints the
    /// style, `acceptance.dir`, the `run` command, `fakes_allowed`, the conventions
    /// of the style, every criterion with its id, and the Plan's Scope and
    /// Implementation context. Never `BUSINESS_CONTEXT.md` beyond what the Plan
    /// quotes, and never the decision log. Nothing is cut for size. When there are any, also
    /// the files the developer unlocked, and the Plan's `## Acceptance tests to change`, each
    /// with what to do.
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
    /// or is skipped is refused, and there is no override. The one difference is a
    /// file the developer unlocked with `invar acceptance unlock`, which may pass too, and
    /// the output names it. All the problems are listed at once, and a refusal writes
    /// nothing.
    ///
    /// Refuses first what `invar acceptance spec` refuses. On success the last line
    /// is `locked <TICKET> <N> file(s)`.
    Lock(lock::Args),
    /// Take the lock off named test files of a ticket, so that the test-writer can fix
    /// them: the developer's decision, through `/invar:relock`.
    ///
    /// For a locked test that is wrong. Every file must be under `acceptance.dir`, locked
    /// for this ticket, with its digest holding, or nothing is unlocked. The line of each
    /// goes back to pending, and the files are written to the unlock record beside the
    /// Plan. A ticket has two rounds, and a third is refused: end the cycle as `failed`.
    ///
    /// Refuses first what `invar acceptance spec` refuses. On success the last line is
    /// `unlocked <TICKET> <N> file(s), round <R> of 2`. An agent may not run it.
    Unlock(unlock::Args),
    /// Check that the acceptance locks hold. Runs no test.
    ///
    /// With no ticket, for CI on every push: every locked file under
    /// `acceptance.dir` has the digest its line records, no line is malformed, and no
    /// line is pending. It needs no Plan and no git, so it works after the Plan is
    /// deleted and in a shallow clone.
    ///
    /// With a ticket, before `invar done`: the check `done` makes, deleting nothing.
    /// That adds the branch rule, under which another ticket's locked test may change
    /// only if the approved Plan declares it, and it asks for a locked test for the
    /// ticket when its Plan says `required`, with no pending line left.
    ///
    /// Prints `verified <N> locked file(s)`, or every problem on stderr and exits
    /// non-zero. With no `acceptance:` section it warns and exits 0. With a ticket, it
    /// says on stderr which files the developer unlocked.
    Verify(verify::Args),
}

pub fn run(command: Command) -> Result<()> {
    match command {
        Command::Spec(args) => spec::run(args),
        Command::Lock(args) => lock::run(args),
        Command::Unlock(args) => unlock::run(args),
        Command::Verify(args) => verify::run(args),
    }
}
