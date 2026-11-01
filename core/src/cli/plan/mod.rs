//! `invar plan …`: the Plan of one ticket (§4a).

use std::process::ExitCode;

use anyhow::Result;
use clap::Subcommand;

use super::mode;

mod approve;
mod check;
mod guard;
mod subagent_stop;
// The same check `plan check` makes, for `pr-body` and `done`, which sit
// outside `plan` and must not stand on lighter ground.
pub(super) mod require_approved;
// `done` and `pr-body` sit directly under `invar`, not under `plan`, but name
// and locate a Plan the same way `plan check`/`plan approve` do — so this is
// shared with them rather than reimplemented per command.
pub(super) mod target;
mod validate;

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Check that a Plan is well-formed: location, directory name, header,
    /// required sections, and header against directory.
    ///
    /// Checks form, not approval: a draft with empty approval fields is
    /// valid. Every problem is reported in one run, each as
    /// `path:line: what to change`. Exits non-zero when any Plan is invalid.
    ///
    /// With `--all-drafts --hook` it answers a `SubagentStop` hook instead:
    /// see the flag.
    Validate(validate::Args),

    /// Approve a Plan: the human gate.
    ///
    /// Validates the Plan, prints its acceptance criteria and whether the
    /// ticket needs acceptance tests, then sets `Status: approved` and records
    /// `Approved-by` (`git config user.name`), `Approved-at` (today),
    /// `Approved-checksum` (the digest of the body and that decision) and
    /// `Acceptance`. Only those five values change; the body stays
    /// byte-identical.
    ///
    /// An approved Plan whose body or `Acceptance` changed since is refused
    /// unless `--reapprove` is given; one that is unchanged is left as it is.
    ///
    /// Without a ticket, approves the one Plan under `.invar/plans/`. The
    /// developer's command — through `/invar:approve` in Claude Code, which
    /// then starts Phase 2, or from a terminal. No agent runs it.
    Approve(approve::Args),

    /// Check that a Plan is approved and unchanged since: the gate before any
    /// code is written.
    ///
    /// Run by the agent at the start of Phase 2, and by `invar done`. Passes
    /// only an approved Plan that still matches `Approved-checksum`;
    /// refuses a draft, a Plan changed after approval, and an approval no
    /// `invar plan approve` recorded. Exits non-zero on refusal.
    Check(check::Args),

    /// Deny `Write`/`Edit` of an approved Plan, and `Bash` runs of `plan approve` — for a `PreToolUse` hook.
    ///
    /// Reads the hook's JSON from stdin and looks at `tool_input.file_path`.
    /// An approved Plan's `spec.md` is denied (exit 2, with the reason as JSON
    /// on stdout and as text on stderr); a draft Plan and every other file
    /// pass silently (exit 0). Input it cannot make sense of passes too, with
    /// the problem reported: a broken guard must not halt a session.
    ///
    /// Under `acceptance.dir`, a locked test is denied the same way, and so is a
    /// call that would write a digest into a lock line, which only `invar
    /// acceptance lock` does. `invar acceptance verify` is the boundary there.
    ///
    /// A `Bash` call is read by its `tool_input.command` instead: one that runs
    /// `invar plan approve` is denied the same way, because approving is the
    /// developer's decision, made from their own terminal. Every other command
    /// passes silently.
    ///
    /// The guard is a convenience; the checksum is the boundary. A hook on
    /// `Write|Edit` never sees a file changed through `Bash` — a heredoc,
    /// `sed -i`, `git checkout`. `invar plan check` and `invar done` catch
    /// every such change through `Approved-checksum`. Passing the guard proves
    /// nothing about a Plan; passing `plan check` does.
    Guard(guard::Args),
}

pub fn run(command: Command) -> Result<ExitCode> {
    let done = |result: Result<()>| result.map(|()| ExitCode::SUCCESS);
    match command {
        // The hook form answers in exit codes, which `validate::run` has no
        // way to do: only a block (exit 2) reaches the planner.
        Command::Validate(args) if args.hook => subagent_stop::run(),
        Command::Validate(args) => done(validate::run(args)),
        Command::Approve(args) => done(approve::run(args)),
        Command::Check(args) => done(check::run(args)),
        Command::Guard(args) => guard::run(args),
    }
}
