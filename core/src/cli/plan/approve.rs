//! `invar plan approve [TICKET | PATH] [--acceptance | --no-acceptance]
//! [--reapprove] [--no-start] [--skill]`.
//!
//! Without a ticket, the one Plan under `.invar/plans/`; with several, it
//! names them and approves none. `--no-start` and `--skill` are for
//! `/invar:approve`, whose shell block is this one command.
//!
//! Order matters: validate, compare with any earlier approval and decide
//! `Acceptance` from that, read who is approving and today's date from git,
//! check that those values can be written, print the acceptance criteria and
//! the decision, and only then write. Everything that can be known to fail
//! fails before the criteria are shown, so a person does not read the
//! criteria of an approval that was never going to be recorded. What is left
//! is the file changing in the meantime, which the commit refuses, and a disk
//! that fails.

use anyhow::{Context as _, Result};

use crate::format::plan::{Acceptance, Mode};
use crate::gate::approval::Prior;
use crate::gate::approve::{self, ApproveError, Decision};
use crate::git;

use crate::fs::{self, plans::SPEC_FILE};

use super::target;
use super::validate::print_report;

#[derive(Debug, clap::Args)]
pub struct Args {
    /// A ticket id (`PROJ-412`), or the path of a Plan: its `spec.md` or its
    /// directory. Without it, the one Plan under `.invar/plans/`; when there
    /// are several, they are listed and none is approved.
    ///
    /// Anything containing `/`, ending in `.md`, or `.`/`..` is a path;
    /// everything else is looked up as a ticket under `.invar/plans/`.
    pub target: Option<String>,

    /// This ticket needs acceptance tests: records `Acceptance: required`.
    ///
    /// Refused when `invar.yaml` has no `acceptance:` section, since there is no
    /// command to run them with. Without either flag, an approval being replaced
    /// keeps the value in its header; otherwise the value is `acceptance.required`
    /// from `invar.yaml`, and `skipped` when there is no such section. On a Plan
    /// already approved it changes nothing unless `--reapprove` is given as well.
    #[arg(long, conflicts_with = "no_acceptance")]
    pub acceptance: bool,

    /// This ticket needs no acceptance tests: records `Acceptance: skipped`.
    ///
    /// It overrides `acceptance.required` in `invar.yaml`, and it is how a
    /// recorded `required` is dropped: `--reapprove` alone keeps it.
    /// On a Plan already approved it changes nothing unless `--reapprove` is
    /// given as well.
    #[arg(long)]
    pub no_acceptance: bool,

    /// Approve a Plan that is already approved but whose body or `Acceptance`
    /// changed since (or whose approval values are missing), replacing that
    /// approval. With `--acceptance` or `--no-acceptance`, also change the value
    /// recorded by an approval that still holds.
    ///
    /// Without it such a Plan is refused: a silent re-approval would look
    /// exactly like the edit the checksum exists to catch (plan-header.md
    /// D-12). The value of `Acceptance` in the header is kept unless
    /// `--acceptance` or `--no-acceptance` names another. If the checksum shows
    /// that value is not the approved one, one of those flags is required
    /// (D-20).
    #[arg(long)]
    pub reapprove: bool,

    /// Approve without starting Phase 2: ends the output with `Phase 2: not
    /// started (--no-start)`.
    ///
    /// For `/invar:approve`, which starts the coder once an approval is
    /// recorded; with this the developer implements later, or elsewhere.
    /// What is recorded is the same either way.
    #[arg(long)]
    pub no_start: bool,

    /// Report as `/invar:approve` needs it: a refusal is printed on stdout as
    /// `error: …`, and the exit code is 0.
    ///
    /// A skill's shell block that exits non-zero is dropped by Claude Code
    /// with its output, so the refusal would never reach the session.
    #[arg(long)]
    pub skill: bool,
}

pub fn run(args: Args) -> Result<()> {
    let (skill, no_start) = (args.skill, args.no_start);
    let result = approve(args);
    if no_start && result.is_ok() {
        println!("Phase 2: not started (--no-start)");
    }
    crate::cli::for_skill(skill, result)
}

fn approve(args: Args) -> Result<()> {
    let cwd = std::env::current_dir().context("cannot read the current directory")?;
    let query = match args.target {
        Some(target) => target,
        None => target::only(
            &fs::project_root()?,
            SPEC_FILE,
            "there is no Plan in .invar/plans/ to approve",
            "Plans are waiting; name the one to approve: `invar plan approve <ticket>` \
             (in Claude Code, `/invar:approve <ticket>`)",
        )?,
    };
    let (root, spec) = target::resolve(&query, &cwd)?;
    let config = crate::cli::config(&root)?;
    let mode = Mode::of(&config);
    let configured = config.acceptance.as_ref().map(|section| {
        if section.required {
            Acceptance::Required
        } else {
            Acceptance::Skipped
        }
    });
    let flag = match (args.acceptance, args.no_acceptance) {
        (true, _) => Some(Acceptance::Required),
        (_, true) => Some(Acceptance::Skipped),
        _ => None,
    };

    let pending = match approve::prepare(&root, &spec, mode, args.reapprove, flag, configured) {
        Ok(Decision::Approve(pending)) => pending,
        Ok(Decision::AlreadyApproved {
            spec,
            by,
            at,
            recorded,
            ignored_flag,
        }) => {
            println!(
                "already approved: {} — by {by} on {at}, and the body is unchanged; \
                 nothing to do",
                target::shown(&spec, &cwd)
            );
            if let Some(flag) = ignored_flag {
                println!(
                    "`{flag}` changed nothing: this approval records `Acceptance: {}`. To \
                     change it, approve again with `--reapprove {flag}`",
                    recorded.as_str()
                );
            }
            return Ok(());
        }
        Err(ApproveError::Invalid(report)) => {
            print_report(&report, &cwd);
            return Err(ApproveError::Invalid(report).into());
        }
        Err(ApproveError::Unnumbered(report)) => {
            print_report(&report, &cwd);
            return Err(ApproveError::Unnumbered(report).into());
        }
        Err(err) => return Err(err.into()),
    };

    let by = git::user_name(root.path())?;
    let at = git::today(root.path())?;
    pending.check_values(&by, &at)?;

    let shown = target::shown(pending.spec(), &cwd);
    match pending.replaces() {
        Some(Prior::Changed { by, at, .. }) => {
            println!(
                "re-approving: the body or `Acceptance` changed after {by} approved it on {at}\n"
            )
        }
        Some(Prior::Current { by, at }) => println!(
            "re-approving: {by} approved it on {at} with `Acceptance: {}`, and this replaces \
             that\n",
            pending.recorded_acceptance().as_str()
        ),
        Some(Prior::Unrecorded { .. }) => println!(
            "re-approving: the earlier approval was not recorded by `invar plan approve`\n"
        ),
        Some(Prior::Draft) | None => {}
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
    println!("Acceptance: {}", pending.acceptance());
    if pending.required_without_section() {
        eprintln!(
            "warning: invar.yaml has no `acceptance:` section, so no acceptance tests can be \
             run for this ticket until one is added"
        );
    }
    println!();

    let ticket = pending.ticket().to_owned();
    let approval = pending.commit(by, at)?;
    println!("approved: {shown}");
    println!("  Approved-by: {}", approval.by);
    println!("  Approved-at: {}", approval.at);
    println!("  Approved-checksum: {}", approval.checksum);
    println!("  Acceptance: {}", approval.acceptance.as_str());
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
