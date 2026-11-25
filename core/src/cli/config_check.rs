//! `invar config check [--base <ref>]` (#157): whether a branch weakens what invar checks.
//! For CI; it writes nothing.

use anyhow::{Context as _, Result};
use clap::Subcommand;

use crate::attestation::Kind;
use crate::config_check::{self, Weakened};
use crate::cycle::short;
use crate::fs::{self, ProjectRoot};
use crate::git;
use crate::signature::{self, Signer};

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
    /// Sign that this branch weakens what invar checks on purpose, and commit it. The
    /// developer's, never an agent's.
    ///
    /// For a project whose base has `approval.signed: true`, where `config check` counts
    /// an `Invar-Config:` trailer only with a signature by a key in the base's
    /// `.invar/approvers`. Lists each weakening, signs a statement of invar.yaml at the
    /// base and at HEAD, and the reason, with the SSH key git signs with, within 90
    /// seconds, then makes one empty commit with the trailers `Invar-Config: <reason>` and
    /// `Invar-Config-Signature: <signature>`. What else is staged stays staged; nothing
    /// is pushed. A later change to invar.yaml needs a new signature.
    Accept {
        /// Why the branch weakens the checks, in one line: the value of `Invar-Config:`.
        #[arg(long)]
        reason: String,

        /// The branch the pull request targets, as `config check` takes it.
        #[arg(long, value_name = "REF")]
        base: Option<String>,

        /// Print the two trailers instead of committing them.
        #[arg(long)]
        print: bool,

        /// Report as `/invar:config-accept` needs it: a refusal is printed on stdout as
        /// `error: …`, and the exit code is 0.
        #[arg(long)]
        skill: bool,
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
                    signed: report.signed,
                }
                .into())
            }
        }
        Command::Accept {
            reason,
            base,
            print,
            skill,
        } => super::for_skill(
            skill,
            fs::project_root()
                .map_err(Into::into)
                .and_then(|root| accept(&root, &reason, base.as_deref(), print)),
        ),
    }
}

/// `invar config accept`: show, sign, then commit or print.
fn accept(root: &ProjectRoot, reason: &str, base: Option<&str>, print: bool) -> Result<()> {
    let prepared = config_check::prepare_accept(root, base, reason)?;
    let signer = Signer::from_git(root.path())?;
    println!(
        "What this branch weakens in invar.yaml, against {} (base -> this branch):",
        prepared.base
    );
    for weakening in &prepared.weakenings {
        println!("  {weakening}");
    }
    println!();
    println!("Invar-Config: {}", prepared.reason);
    println!();
    println!("signing with {} — answer its prompt if it asks", signer.key);
    let armored = signer.sign_in(
        root.path(),
        Kind::Config.namespace(),
        &prepared.statement,
        signature::DEADLINE,
    )?;
    let one_line = signature::one_line(&armored);
    let trailers = [
        (Kind::Config.reason_key(), prepared.reason.as_str()),
        (Kind::Config.signature_key(), one_line.as_str()),
    ];
    if print {
        for (key, value) in trailers {
            println!("{key}: {value}");
        }
        return Ok(());
    }
    let message = "chore: accept the configuration change";
    let commit = git::commit_empty(root.path(), message, &trailers)?;
    println!("committed: {} {message}", short(&commit));
    println!(
        "configuration change signed: {} weakening(s)",
        prepared.weakenings.len()
    );
    Ok(())
}
