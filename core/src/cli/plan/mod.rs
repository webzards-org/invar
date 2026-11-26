//! `invar plan …`: the Plan of one ticket (§4a).

use std::process::ExitCode;

use anyhow::Result;
use clap::Subcommand;

use super::mode;

mod approve;
mod audit;
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
    /// A Plan that is not approved is also held to the invariant ids of
    /// `BUSINESS_CONTEXT.md`: an `INV-<n>` the file does not define, an invariant
    /// named in Implementation context that no criterion cites, a criterion citing
    /// one it does not name, and the file's own duplicate, malformed or partial ids.
    /// A file without ids is not checked.
    ///
    /// With `--all-drafts --hook` it answers a `SubagentStop` hook instead:
    /// see the flag.
    Validate(validate::Args),

    /// Approve a Plan: the human gate.
    ///
    /// Validates the Plan, prints its acceptance criteria, every invariant of
    /// `BUSINESS_CONTEXT.md` marked named or NOT NAMED when they have ids, the line of
    /// a Plan audit, computed again from its baseline and findings (READY or NOT READY,
    /// for this version of the Plan or changed after it, `not run`, or `findings
    /// unreadable`, and a line for each section changed since it began that the auditor
    /// may not edit), which refuses nothing, and whether the ticket needs acceptance tests,
    /// then sets `Status: approved` and records
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
    /// A draft Plan, or a `spec.md` not written yet, is judged by the header
    /// the call would leave: one that says `Status: approved`, or fills an
    /// approval key (`Approved-by`, `Approved-at`, `Approved-checksum`,
    /// `Lockfile-checksum`, `Acceptance`) with a value the file did not have,
    /// is denied. Only `invar plan approve` writes them.
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
    /// nothing about a Plan; passing `plan check` does. A shell write (`sed
    /// -i`, a heredoc, `python3`) of approval values into a draft gets past
    /// the guard too, as it does for a lock line (acceptance-lock.md D-20).
    ///
    /// The project's `invar.yaml` is denied an edit that would weaken what invar checks
    /// (invar-yaml.md §3.9), and `.claude/settings.json` one that takes out the deny rule
    /// for `.invar/decisions/**`. A `git commit` that writes an `Invar-Skip:` or
    /// `Invar-Config:` trailer is denied: those are the developer's. The baseline and the
    /// report of a Plan audit (`.invar/out/<TICKET>-audit/baseline.md`, `report.txt`) are
    /// denied too: only `plan audit begin` and `report` write them, and `findings.md`
    /// beside them is the auditor's. A shell write of either gets past the guard, as for
    /// the other files (plan-audit.md §7.2). `invar config check`
    /// and `invar cycle check` in CI are the boundaries there.
    ///
    /// Known limits, which no guard closes. The plugin's binary, its `hooks.json` and the
    /// project's `.claude/settings.json` can be changed from the agent's shell, and the
    /// guard fails open: a crash, a missing binary or the hook's timeout lets the call
    /// through. The command guard reads words: a copied or renamed binary (`cp … /tmp/x
    /// && /tmp/x plan approve`), a name built at run time, or `cargo run -- plan approve`
    /// is not seen. The boundary is CI, running its own released binary, and for
    /// approvals the signature of #154.
    Guard(guard::Args),

    /// Audit a draft Plan against the code, with an agent: `begin`, the developer's;
    /// `input`, what the plan-auditor reads; `report`, what changed and READY or NOT
    /// READY (`contracts/formats/plan-audit.md`). Advisory: nothing waits for it.
    #[command(subcommand)]
    Audit(audit::Command),
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
        Command::Audit(command) => done(audit::run(command)),
    }
}
