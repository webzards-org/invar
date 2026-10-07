//! Argument parsing and dispatch.
//!
//! One module per subcommand. A subcommand module owns its arguments and a
//! `run` function; it does not own the logic — that lives in `digest`,
//! `format`, `gate`, `decision` and `git`. Keeping the split means a command
//! can be called from a test, from CI or from a hook and behave identically
//! (§13b).

use std::path::Path;
use std::process::ExitCode;

use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::format::config;
use crate::format::plan::Mode;
use crate::fs::ProjectRoot;

mod decision;
mod hash;
mod plan;

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
    /// `Confirmed-checksum` or `Approved-checksum` covers only the body below a
    /// header, so this command run on a `.feature` or a Plan does not
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
    // The rest of M1 (docs/TASKS-M1.md):
    //   done                             M1-12
    //   clean-spec                       M1-13
    //   context show                     M1-14
    //   init                             M1-15
    //   pr-body                          M1-16
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
        }
    }
}

/// Read `invar.yaml` for a command: warnings to stderr, `scenarios: true`
/// refused (invar-yaml.md §3.1).
fn mode(root: &ProjectRoot) -> Result<Mode> {
    let loaded = config::load_for_command(root)?;
    for warning in &loaded.warnings {
        eprintln!("warning: {}: {warning}", loaded.path.display());
    }
    Ok(Mode::of(&loaded.config))
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
