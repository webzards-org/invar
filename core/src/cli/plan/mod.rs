//! `invar plan …`: the Plan of one ticket (§4a).

use std::process::ExitCode;

use anyhow::Result;
use clap::Subcommand;

use crate::format::config;
use crate::format::plan::Mode;
use crate::fs::ProjectRoot;

mod approve;
mod check;
mod guard;
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

    /// Check that a Plan is approved and unchanged since: the gate before any
    /// code is written.
    ///
    /// Run by the agent at the start of Phase 2, and by `invar done`. Passes
    /// only an approved Plan whose body still matches `Approved-checksum`;
    /// refuses a draft, a Plan changed after approval, and an approval no
    /// `invar plan approve` recorded. Exits non-zero on refusal.
    Check(check::Args),

    /// Deny `Write`/`Edit` to an approved Plan — for a `PreToolUse` hook.
    ///
    /// Reads the hook's JSON from stdin and looks at `tool_input.file_path`.
    /// An approved Plan's `spec.md` is denied (exit 2, with the reason as JSON
    /// on stdout and as text on stderr); a draft Plan and every other file
    /// pass silently (exit 0). Input it cannot make sense of passes too, with
    /// the problem reported: a broken guard must not halt a session.
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
        Command::Validate(args) => done(validate::run(args)),
        Command::Approve(args) => done(approve::run(args)),
        Command::Check(args) => done(check::run(args)),
        Command::Guard(args) => guard::run(args),
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
