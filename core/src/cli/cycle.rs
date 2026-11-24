//! `invar cycle …`: whether a change has to go through a cycle (#155).
//!
//! `check` is for CI and decides on the branch's history; `required` is for the main
//! session, which asks it of the paths it changed when it skipped the cycle (L-9). Both
//! write nothing.

use anyhow::Result;
use clap::Subcommand;

use crate::cycle::{self, NotCovered};
use crate::fs;

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Check that a branch changing a required path went through a cycle. For CI.
    ///
    /// Reads `cycle.required_paths` from `invar.yaml` on the base branch, never from this
    /// branch, so that a branch cannot switch off its own check; a branch that changes the
    /// list gets a note. With no list there, the check is off and exits 0.
    ///
    /// A path changed on the branch (added, changed, deleted, or either name of a rename;
    /// `.invar/` left out) that a pattern matches is a required change. It is covered when
    /// every commit of the branch (`<base>..HEAD`) that touched it was made while the Plan
    /// of a cycle closed on the branch was approved at that commit's parent — a Plan that
    /// a commit of the branch approved and a later one deleted, as `invar done` does. The
    /// branch passes when every required change is covered, or with an
    /// `Invar-Skip: <reason>` trailer on any commit of the branch. A Plan still there is an
    /// open cycle, and covers nothing. An `Invar-Skip:` with no reason does not count.
    ///
    /// Needs the history from the merge base to HEAD: in CI, check out with
    /// `fetch-depth: 0`. Prints what it decided on, and ends with `cycle check: off: …`,
    /// `cycle check: passed: …` or `cycle check: failed`, the last with every required
    /// path on stderr and a non-zero exit.
    ///
    /// The trailer is a person's decision: `plan guard` denies an agent's `git commit`
    /// that writes it. That is a scan of the command line, like the rest of the guard,
    /// and not a proof of who wrote it; tying a skip to the developer's key comes with
    /// the verification of signed approvals (#154).
    Check {
        /// The branch or commit to compare with: in CI, the pull request's target, such as
        /// `origin/main`. Without it, the branch `origin/HEAD` points to, else `main`; never
        /// `git.base_branch`, which the branch being checked could set to itself.
        #[arg(long, value_name = "REF")]
        base: Option<String>,
    },
    /// Say which of the given paths `cycle.required_paths` requires a cycle for.
    ///
    /// For the main session, when it skipped the cycle: it names the files it changed.
    /// Reads `invar.yaml` in the working tree. Prints `<path>: required (<pattern>)` for
    /// each that a pattern matches, then one line: `cycle: required` or
    /// `cycle: not required`. Exits 0 either way.
    Required {
        /// Paths relative to the project root, with `/`.
        #[arg(required = true)]
        paths: Vec<String>,
    },
}

pub fn run(command: Command) -> Result<()> {
    let root = fs::project_root()?;
    let config = super::config(&root)?;
    match command {
        Command::Check { base } => {
            let report = cycle::check(&root, base.as_deref())?;
            print!("{}", report.render());
            if report.passes() {
                Ok(())
            } else {
                Err(NotCovered::of(&report).into())
            }
        }
        Command::Required { paths } => {
            let patterns = &config.cycle.required_paths;
            let found = cycle::required(patterns, &paths);
            let mut any = false;
            for (path, pattern) in &found {
                if let Some(pattern) = pattern {
                    any = true;
                    println!("{path}: required ({pattern})");
                }
            }
            if patterns.is_empty() {
                println!("cycle: not required (invar.yaml has no `cycle.required_paths`)");
            } else if any {
                println!("cycle: required");
            } else {
                println!("cycle: not required");
            }
            Ok(())
        }
    }
}
