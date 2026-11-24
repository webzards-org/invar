//! `invar config check [--base <ref>]` (#157): whether a branch weakens what invar checks.
//! For CI; it writes nothing.

use anyhow::{Context as _, Result};
use clap::Subcommand;

use crate::config_check::{self, Weakened};
use crate::fs;

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Check that a branch does not weaken what invar checks, in invar.yaml. For CI.
    ///
    /// Compares invar.yaml at HEAD with invar.yaml at the base, by a closed list: the
    /// `acceptance:` section removed, or its `dir` or `run` changed, or its `required`
    /// turned from true to false; `approval.signed` turned from true to false; a pattern
    /// removed from `cycle.required_paths`; `review.offer` turned from true to false.
    /// Adding or strengthening a key is never a weakening, and every other key is free to
    /// change.
    ///
    /// No weakening passes. A weakening passes with an `Invar-Config: <reason>` trailer on
    /// any commit of the branch (`<base>..HEAD`), which the developer writes; one with no
    /// reason does not count. Otherwise it fails, non-zero, naming each weakening with its
    /// value at the base and here. With no invar.yaml at the base the check is off and
    /// exits 0; an invar.yaml here that is not valid is an error.
    ///
    /// Needs the history from the merge base to HEAD: in CI, check out with
    /// `fetch-depth: 0`. The last line is `config check: off: …`, `config check: passed:
    /// …` or `config check: failed`.
    ///
    /// `plan guard` denies an agent's edit that weakens invar.yaml, and its `git commit`
    /// that writes the trailer. Both are conveniences an agent with a shell can get past;
    /// this check, on CI's own binary, is the boundary. Tying the trailer to the
    /// developer's key comes with the verification of signed approvals (#154).
    Check {
        /// The branch or commit to compare with: in CI, the pull request's target, such as
        /// `origin/main`. Without it, the branch `origin/HEAD` points to, else `main`; never
        /// `git.base_branch`, which the branch being checked could set to itself.
        #[arg(long, value_name = "REF")]
        base: Option<String>,
    },
}

pub fn run(command: Command) -> Result<()> {
    match command {
        Command::Check { base } => {
            // A branch that deleted invar.yaml has no project root: the current directory
            // is the one CI checked out, and the deletion is what to report. Nothing is read
            // from the working tree's invar.yaml: the branch does not choose its base (D-20).
            let dir = match fs::project_root() {
                Ok(root) => root.path().to_owned(),
                Err(_) => std::env::current_dir().context("cannot read the current directory")?,
            };
            let report = config_check::check(&dir, base.as_deref())?;
            print!("{}", report.render());
            if report.passes() {
                Ok(())
            } else {
                Err(Weakened {
                    weakenings: report.weakenings,
                }
                .into())
            }
        }
    }
}
