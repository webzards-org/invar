//! The `invar` CLI.
//!
//! This file does two things and nothing else: parse arguments and dispatch.
//! Every subcommand lives in its own module under `cli/`, so `main.rs` stays
//! readable as the surface grows (§8).
//!
//! Errors are handled at this boundary: anything a subcommand returns as an
//! `Err` is printed with its full context chain and turns into a non-zero exit
//! code. `invar generate` must fail loudly on any violation (§10), which starts
//! with the exit code being right.

mod acceptance;
mod calendar;
mod cli;
mod context;
mod decision;
mod diagnostics;
mod digest;
mod done;
mod format;
mod fs;
mod gate;
mod git;
mod init;
mod pr_body;
mod review_input;

use std::process::ExitCode;

use clap::Parser;

fn main() -> ExitCode {
    let cli = match cli::Cli::try_parse() {
        Ok(cli) => cli,
        Err(err) => {
            let args: Vec<_> = std::env::args_os().skip(1).collect();
            return usage(&err, cli::is_plan_approve_for_skill(&args));
        }
    };

    match cli.run() {
        // Most commands exit 0 on success; `plan guard` chooses its own code
        // (2 denies a tool call to a `PreToolUse` hook).
        Ok(code) => code,
        Err(err) => {
            // `{err:#}` prints the whole anyhow context chain on one line:
            // "reading the Plan: opening spec.md: No such file or directory".
            eprintln!("error: {err:#}");
            ExitCode::FAILURE
        }
    }
}

/// What to do with a command line clap does not accept — or with `--help` and
/// `--version`, which clap reports through the same error type.
///
/// clap's own `parse()` exits with code 2 for a usage error. In this tool 2
/// means exactly one thing: `plan guard` denying a tool call to a `PreToolUse`
/// hook, and Claude Code *blocks* the call on it (`plugin/NOTES.md` §11). A
/// hook started with arguments this binary does not know — a plugin and a CLI
/// of different versions — would otherwise be read as a denial of every
/// `Write`, `Edit` and `Bash` call in the session, with a usage message as the
/// reason. So a usage error exits 1, like every other failure, and 2 stays a
/// verdict someone chose to give.
///
/// `for_skill` is `plan approve --skill`: an error goes to stdout and exits 0,
/// as every refusal of that command does, because a skill's shell block that
/// exits non-zero loses its output. clap's text already starts with `error: `.
fn usage(err: &clap::Error, for_skill: bool) -> ExitCode {
    if for_skill && err.use_stderr() {
        print!("{err}");
        return ExitCode::SUCCESS;
    }
    // Help and version go to stdout, errors to stderr, in clap's own format.
    // If even printing fails there is nowhere left to say so.
    let _ = err.print();
    if err.use_stderr() {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
