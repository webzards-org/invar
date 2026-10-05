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

mod cli;
mod diagnostics;
mod digest;
mod format;
mod fs;
mod gate;
mod git;

use std::process::ExitCode;

use clap::Parser;

fn main() -> ExitCode {
    let cli = cli::Cli::parse();

    match cli.run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            // `{err:#}` prints the whole anyhow context chain on one line:
            // "reading the Plan: opening spec.md: No such file or directory".
            eprintln!("error: {err:#}");
            ExitCode::FAILURE
        }
    }
}
