//! `invar approval verify [--base <ref>]` (#168): whether every approval on a branch
//! is signed by an approver of the base. For CI; it writes nothing.

use anyhow::{Context as _, Result};
use clap::Subcommand;

use crate::approval_verify::{self, Failed};
use crate::fs;

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Check that every approval on a branch is signed by an approver of the base. For CI.
    ///
    /// On only when invar.yaml at the base sets `approval.signed: true`; a branch that
    /// turns it off is checked all the same, with a note. Otherwise it says the check is
    /// off and exits 0.
    ///
    /// Follows every Plan through each commit of the branch, along first parents, from
    /// the merge base with the base to HEAD. An approval must have an `approval.sig` that
    /// `ssh-keygen` verifies over the approval's statement, for a principal in
    /// `.invar/approvers` as the base has it, never as the branch has it, and its
    /// `Approved-checksum` must match the Plan. An `Approval-id` that an earlier approval
    /// of the branch had is a replay, and refused. A body changed while approved needs a
    /// later signed approval, a reset or a deletion. A deletion needs a signed approval
    /// before it, with no unsettled body change in between. A Plan approved at HEAD must
    /// pass as it stands.
    ///
    /// Prints who signed each approval (the principal and the key's fingerprint), and a
    /// note when the header's `Approved-by` says otherwise. Every problem is listed in one
    /// run, naming its commit and Plan, and the run then fails, non-zero.
    ///
    /// Needs the whole history and the branch's own head: in CI, check out with
    /// `fetch-depth: 0` and `ref: ${{ github.event.pull_request.head.sha }}`. A shallow
    /// clone, a HEAD that is CI's merge commit onto the base, and a base with no
    /// `.invar/approvers` are refused. The last line is `approval verify: off: …`,
    /// `approval verify: passed: …` or `approval verify: failed`.
    Verify {
        /// The branch or commit to compare with: in CI, the pull request's target, such as
        /// `origin/main`. Without it, the branch `origin/HEAD` points to, else `main`; never
        /// `git.base_branch`, which the branch being checked could set to itself.
        #[arg(long, value_name = "REF")]
        base: Option<String>,
    },
}

pub fn run(command: Command) -> Result<()> {
    match command {
        Command::Verify { base } => {
            // As for `config check`: a branch that deleted invar.yaml has no project root,
            // and the base still says whether the check is on.
            let dir = match fs::project_root() {
                Ok(root) => root.path().to_owned(),
                Err(_) => std::env::current_dir().context("cannot read the current directory")?,
            };
            let report = approval_verify::check(&dir, base.as_deref())?;
            print!("{}", report.render());
            if report.passes() {
                Ok(())
            } else {
                Err(Failed {
                    problems: report.problems.len(),
                }
                .into())
            }
        }
    }
}
