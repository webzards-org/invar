//! `invar plan approve <TICKET | PATH> [--reapprove]`.
//!
//! Order matters: validate, read who is approving and today's date from git,
//! check that those values can be written, print the acceptance criteria, and
//! only then write. Everything that can be known to fail fails before the
//! criteria are shown, so a person does not read the criteria of an approval
//! that was never going to be recorded. What is left is the file changing in
//! the meantime, which the commit refuses, and a disk that fails.

use anyhow::{Context as _, Result};

use crate::gate::approval::Prior;
use crate::gate::approve::{self, ApproveError, Decision};
use crate::git;

use super::target;
use super::validate::print_report;

#[derive(Debug, clap::Args)]
pub struct Args {
    /// A ticket id (`PROJ-412`), or the path of a Plan: its `spec.md` or its
    /// directory.
    ///
    /// Anything containing `/`, ending in `.md`, or `.`/`..` is a path;
    /// everything else is looked up as a ticket under `.invar/plans/`.
    pub target: String,

    /// Approve a Plan that is already approved but whose body changed since
    /// (or whose approval values are missing), replacing that approval.
    ///
    /// Without it such a Plan is refused: a silent re-approval would look
    /// exactly like the body edit the checksum exists to catch
    /// (plan-header.md D-12).
    #[arg(long)]
    pub reapprove: bool,
}

pub fn run(args: Args) -> Result<()> {
    let cwd = std::env::current_dir().context("cannot read the current directory")?;
    let (root, spec) = target::resolve(&args.target, &cwd)?;
    let mode = super::mode(&root)?;

    let pending = match approve::prepare(&root, &spec, mode, args.reapprove) {
        Ok(Decision::Approve(pending)) => pending,
        Ok(Decision::AlreadyApproved { spec, by, at }) => {
            println!(
                "already approved: {} — by {by} on {at}, and the body is unchanged; \
                 nothing to do",
                target::shown(&spec, &cwd)
            );
            return Ok(());
        }
        Err(ApproveError::Invalid(report)) => {
            print_report(&report, &cwd);
            return Err(ApproveError::Invalid(report).into());
        }
        Err(err) => return Err(err.into()),
    };

    let by = git::user_name(root.path())?;
    let at = git::today(root.path())?;
    pending.check_values(&by, &at)?;

    let shown = target::shown(pending.spec(), &cwd);
    match pending.replaces() {
        Some(Prior::Changed { by, at, .. }) => {
            println!("re-approving: the body changed after {by} approved it on {at}\n")
        }
        Some(Prior::Unrecorded { .. }) => println!(
            "re-approving: the earlier approval was not recorded by `invar plan approve`\n"
        ),
        Some(Prior::Draft | Prior::Current { .. }) | None => {}
    }
    println!(
        "Acceptance criteria of {} ({shown}) — the code will be judged against these:\n",
        pending.ticket()
    );
    for line in pending.acceptance_criteria().lines() {
        if line.trim().is_empty() {
            println!();
        } else {
            println!("  {line}");
        }
    }
    println!();

    let ticket = pending.ticket().to_owned();
    let approval = pending.commit(by, at)?;
    println!("approved: {shown}");
    println!("  Approved-by: {}", approval.by);
    println!("  Approved-at: {}", approval.at);
    println!("  Approved-checksum: {}", approval.checksum);
    println!();
    println!("{}", recorded_line(&ticket, &approval.checksum));
    Ok(())
}

/// The last line of a run that has just recorded an approval, and only of such
/// a run: `approved <TICKET> checksum sha256:<hex>`.
///
/// Fixed and parseable, because `/invar:approve` starts Phase 2 on it: the
/// skill keys on this line, not on the model's reading of the output. A Plan
/// that was already approved, and every refusal, prints no such line.
fn recorded_line(ticket: &str, checksum: &impl std::fmt::Display) -> String {
    format!("approved {ticket} checksum {checksum}")
}
