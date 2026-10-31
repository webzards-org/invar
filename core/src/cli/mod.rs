//! Argument parsing and dispatch.
//!
//! One module per subcommand. A subcommand module owns its arguments and a
//! `run` function; it does not own the logic — that lives in `digest`,
//! `format`, `gate`, `decision` and `git`. Keeping the split means a command
//! can be called from a test, from CI or from a hook and behave identically
//! (§13b).

use std::io::{IsTerminal as _, Read as _};
use std::path::Path;
use std::process::ExitCode;

use anyhow::{Context as _, Result, bail};
use clap::{Parser, Subcommand};

use crate::format::config;
use crate::format::plan::Mode;
use crate::fs::ProjectRoot;

mod acceptance;
mod context;
mod decision;
mod done;
mod hash;
mod init;
mod plan;
mod pr_body;
mod review_input;

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
    /// Print the canonical digest of a whole file (§4d).
    ///
    /// The whole file is hashed: its entire content is canonicalized (UTF-8,
    /// BOM, line endings, trailing whitespace) and run through SHA-256. A
    /// `Confirmed-checksum` or `Approved-checksum` covers the body below a
    /// header, not the whole file, so this command run on a `.feature` or a Plan does not
    /// reproduce that field.
    Hash(hash::Args),
    /// Work with the Plan of a ticket: `.invar/plans/<TICKET>-<slug>/spec.md`.
    #[command(subcommand)]
    Plan(plan::Command),
    /// Record and read the decision log: `.invar/decisions/`.
    ///
    /// What was built and why — or why it could not be — kept after the Plan
    /// is gone, for the planner to read next time.
    #[command(subcommand)]
    Decision(decision::Command),
    /// Work with the acceptance tests of a ticket, for the test-writer agent.
    #[command(subcommand)]
    Acceptance(acceptance::Command),
    /// Render the Plan as a PR description.
    ///
    /// Fixed section order — Intent, Scope, Open questions / answers,
    /// Rejected alternatives, Acceptance criteria, Acceptance tests to change
    /// when the Plan declares any, Implementation context — with the ticket id
    /// and the approval line. The pasted `## Ticket` text
    /// is left out unless `--with-ticket` is given. Refuses on a draft Plan:
    /// a PR description describes an approved specification.
    ///
    /// Run it while the Plan exists: `invar done` deletes it (§12).
    PrBody(pr_body::Args),
    /// Print what the reviewer reads: the approved Plan, the binding context
    /// and the branch's diff.
    ///
    /// The Plan's Intent, Scope, Acceptance criteria, Acceptance tests to change
    /// when it declares any, and Implementation context; `AI_CONTEXT.md` and `BUSINESS_CONTEXT.md`; and the diff of
    /// what is committed on this branch since it left the base branch
    /// (`git.base_branch`, else `origin/HEAD`, else `main`), with `.invar/`
    /// left out. A diff above 40,000 characters is replaced by the list of
    /// changed files with their line counts. Never the decision log.
    ///
    /// The same Plan and the same commits print the same bytes. Refuses on a
    /// draft Plan, or one changed after approval: reviewing against an
    /// unapproved specification proves nothing. Advisory: `done` does not
    /// depend on it, and no hook runs it.
    ReviewInput(review_input::Args),
    /// Close a cycle: delete the Plan directory, and stage the deletion (§12).
    ///
    /// The only command that deletes a Plan. A Plan that reached the main
    /// branch without it is removed with `git rm -r`.
    ///
    /// The developer's command — through `/invar:done` in Claude Code, from a
    /// terminal, or from CI (for example a workflow on pull-request approval).
    /// No agent runs it.
    ///
    /// Without a ticket, closes the Plan whose ticket the current branch's
    /// name contains. Refuses, changing nothing, on a draft or changed Plan,
    /// and while a draft decision entry (`decision.md`) is still in the Plan
    /// directory: approve it with /invar:log-approve, or delete it. Writes no
    /// decision entry and requires none.
    ///
    /// Commits the deletion, alone, with `--commit <MESSAGE>`, or when
    /// `invar.yaml` sets `done.commit: true` (under `done.commit_message`,
    /// default "Remove plan {ticket}"); `--no-commit` only stages it.
    Done(done::Args),
    /// Print the context the agents read: `AI_CONTEXT.md`,
    /// `BUSINESS_CONTEXT.md` and the decision log (§5, §5c, §7).
    #[command(subcommand)]
    Context(context::Command),
    /// Scaffold this directory as an invar project, in light mode.
    ///
    /// Run it in the directory that should hold `invar.yaml` — the project
    /// root — inside a git repository. Creates `invar.yaml` (with `scenarios:
    /// false` stated outright), `.invar/plans/`, `.invar/decisions/`, and
    /// `AI_CONTEXT.md` and `BUSINESS_CONTEXT.md` holding the marker and the
    /// section headings only: never content, which only your team knows.
    ///
    /// Adds to `.gitattributes` (Plan line endings), `.gitignore`
    /// (`.invar/out/`) and `.claude/settings.json` (a rule denying the Read
    /// tool `.invar/decisions/**`; the planner reads the log through `invar
    /// context show`). Those usually exist already: what is in them stays
    /// exactly as it is, and a settings file that is not valid JSON is refused,
    /// not replaced.
    ///
    /// Safe to run again: it says `unchanged` for every file that is already
    /// right, and never touches `invar.yaml` or the context files once they
    /// exist.
    Init,
    // Later milestones: scenario validate | confirm, generate, lint, doctor.
}

impl Cli {
    /// Run the command. Most succeed with exit 0; `plan guard` picks its own
    /// code, because a hook reads meaning into it.
    pub fn run(self) -> Result<ExitCode> {
        match self.command {
            Command::Hash(args) => hash::run(args).map(|()| ExitCode::SUCCESS),
            Command::Plan(command) => plan::run(command),
            Command::Decision(command) => decision::run(command).map(|()| ExitCode::SUCCESS),
            Command::Acceptance(command) => acceptance::run(command).map(|()| ExitCode::SUCCESS),
            Command::PrBody(args) => pr_body::run(args).map(|()| ExitCode::SUCCESS),
            Command::ReviewInput(args) => review_input::run(args).map(|()| ExitCode::SUCCESS),
            Command::Done(args) => done::run(args).map(|()| ExitCode::SUCCESS),
            Command::Context(command) => context::run(command).map(|()| ExitCode::SUCCESS),
            Command::Init => init::run().map(|()| ExitCode::SUCCESS),
        }
    }
}

/// A command's free-text body from stdin (`decision record`). A
/// terminal is refused: nobody is going to type it there, and waiting would
/// only look like a hang.
fn read_stdin() -> Result<String> {
    let stdin = std::io::stdin();
    if stdin.is_terminal() {
        bail!("no text for the entry: give it with `--message \"…\"`, or pipe it on stdin");
    }
    let mut body = String::new();
    stdin
        .lock()
        .read_to_string(&mut body)
        .context("cannot read the entry's text from stdin")?;
    Ok(body)
}

/// Read `invar.yaml` for a command: warnings to stderr, `scenarios: true`
/// refused (invar-yaml.md §3.1).
fn config(root: &ProjectRoot) -> Result<config::Config> {
    let loaded = config::load_for_command(root)?;
    for warning in &loaded.warnings {
        eprintln!("warning: {}: {warning}", loaded.path.display());
    }
    Ok(loaded.config)
}

/// The Plan shape the config selects, for the commands that need nothing else
/// from it.
fn mode(root: &ProjectRoot) -> Result<Mode> {
    Ok(Mode::of(&config(root)?))
}

/// `result`, as a command run with `--skill` reports it: a failure is printed
/// on stdout as `error: …` and the command still exits 0.
///
/// A skill's shell block (`/invar:approve`, `/invar:log-approve`,
/// `/invar:done`) is run by Claude Code before the model sees the skill, and a
/// block that exits non-zero aborts the whole invocation: the developer gets a
/// bare failure and the model never sees the refusal it is meant to show. So a
/// refusal has to come back as output, with the reason in it, on the stream
/// Claude Code injects. Without `--skill`, nothing changes: the error goes to
/// stderr and the exit code is non-zero, for a terminal and for CI.
fn for_skill(skill: bool, result: Result<()>) -> Result<()> {
    match result {
        Err(err) if skill => {
            println!("error: {err:#}");
            Ok(())
        }
        other => other,
    }
}

/// A path relative to the current directory when it is under it, so the paths
/// an agent or a person sees are the ones they would type; otherwise as it is.
///
/// `cwd` is canonicalized first, so a symlinked or roundabout current
/// directory still matches the canonical paths the project root gives.
pub fn shown(path: &Path, cwd: &Path) -> String {
    let cwd = cwd.canonicalize().unwrap_or_else(|_| cwd.to_owned());
    path.strip_prefix(&cwd)
        .unwrap_or(path)
        .display()
        .to_string()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::shown;

    #[test]
    fn a_path_under_the_current_directory_is_shown_relative_to_it() {
        let dir = tempfile::tempdir().unwrap();
        let cwd = dir.path().canonicalize().unwrap();
        let inside = cwd.join(".invar").join("plans").join("spec.md");
        let expected = Path::new(".invar").join("plans").join("spec.md");
        assert_eq!(shown(&inside, &cwd), expected.display().to_string());
    }

    #[test]
    fn a_path_elsewhere_is_shown_in_full() {
        let here = tempfile::tempdir().unwrap();
        let there = tempfile::tempdir().unwrap();
        let path = there.path().canonicalize().unwrap().join("spec.md");
        assert_eq!(shown(&path, here.path()), path.display().to_string());
    }

    #[test]
    fn a_current_directory_that_is_not_canonical_still_matches() {
        // Spelled with a `..` in it, as a symlinked or relative one can be.
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().canonicalize().unwrap();
        std::fs::create_dir(real.join("sub")).unwrap();
        let roundabout = real.join("sub").join("..");
        assert_eq!(shown(&real.join("a.md"), &roundabout), "a.md");
    }
}
