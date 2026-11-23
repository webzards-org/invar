//! `invar plan approve [TICKET | PATH] [--acceptance | --no-acceptance]
//! [--reapprove] [--commit <MESSAGE> | --no-commit] [--no-start] [--skill]`.
//!
//! Without a ticket, the one Plan under `.invar/plans/`; with several, it
//! names them and approves none. `--no-start` and `--skill` are for
//! `/invar:approve`, whose shell block is this one command.
//!
//! Order matters: validate, compare with any earlier approval and decide
//! `Acceptance` from that, read who is approving and today's date from git,
//! check that those values can be written and, with `approval.signed`, that git
//! has an SSH signing key, print the acceptance criteria and the decision, sign,
//! and only then write, and commit when asked. Everything that can be known to
//! fail fails before the criteria are shown, so a person does not read the
//! criteria of an approval that was never going to be recorded. What is left
//! is the key refusing to sign, which writes nothing, the file changing in the
//! meantime, which the write refuses, a disk that fails, and a commit that
//! fails, which leaves the approval written and says so.
//!
//! The signature is made after the criteria are printed, so that a key which
//! asks a person (a touch, a passphrase) asks once they have read them.

use anyhow::{Context as _, Result, bail};

use crate::format::config::Config;
use crate::format::plan::{Acceptance, Mode};
use crate::gate::approval::Prior;
use crate::gate::approve::{self, ApproveError, Decision};
use crate::git;
use crate::signature::{self, Signer};

use crate::fs::{self, ProjectRoot, plans::SPEC_FILE};

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
    /// Refused together with `--no-acceptance`.
    #[arg(long)]
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

    /// Also commit the approval, alone, with this message:
    /// `--commit "chore: approve the Plan of SHOP-214"`.
    ///
    /// Only the Plan's `spec.md`, and its `approval.sig` when the approval is
    /// signed, go into the commit; anything else that is staged stays staged.
    /// The project's hooks and signing apply. Nothing is pushed. Without it,
    /// nothing is committed, unless `invar.yaml` sets `approval.commit: true`,
    /// which commits under `approval.commit_message` (default "Approve plan
    /// {ticket}").
    #[arg(long, value_name = "MESSAGE")]
    pub commit: Option<String>,

    /// Commit nothing, even when `invar.yaml` sets `approval.commit: true`.
    #[arg(long)]
    pub no_commit: bool,

    /// Approve without starting Phase 2: ends the output with `Phase 2: not
    /// started (--no-start)`.
    ///
    /// For `/invar:approve`, which starts Phase 2 once an approval is
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
    // Here and not clap's `conflicts_with`: clap would refuse the pair before
    // `--skill` is read, and `/invar:approve` would lose the reason.
    if args.acceptance && args.no_acceptance {
        bail!(
            "`--acceptance` cannot be used with `--no-acceptance`: name the one this ticket needs"
        );
    }
    if args.commit.is_some() && args.no_commit {
        bail!("`--commit` cannot be used with `--no-commit`: name the one you want");
    }
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
    // Read before the criteria are shown, like the values above: a missing key
    // is known now (approval-signature.md §5).
    let signer = match config.approval.signed {
        true => Some(Signer::from_git(root.path())?),
        false => None,
    };

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
    let spec = pending.spec().to_owned();
    let signature = match &signer {
        Some(signer) => {
            println!("signing with {} — answer its prompt if it asks", signer.key);
            let statement = signature::statement(&ticket, pending.checksum());
            Some(signer.sign(root.path(), &statement, signature::DEADLINE)?)
        }
        None => None,
    };
    let approval = pending.commit_signed(by, at, signature.as_deref())?;
    println!("approved: {shown}");
    println!("  Approved-by: {}", approval.by);
    println!("  Approved-at: {}", approval.at);
    println!("  Approved-checksum: {}", approval.checksum);
    println!("  Acceptance: {}", approval.acceptance.as_str());
    let signature_file = approve::signature_path(&spec);
    if signature.is_some() {
        println!("  Signature: {}", target::shown(&signature_file, &cwd));
    }

    if let Some(message) = commit_message(&args.commit, args.no_commit, &config, &ticket) {
        let mut paths = vec![spec.as_path()];
        if signature.is_some() {
            paths.push(signature_file.as_path());
        }
        commit(&root, &paths, &message).with_context(|| {
            format!(
                "the approval of {ticket} is recorded, and committing it failed; commit it \
                 yourself, or approve with `--no-commit`"
            )
        })?;
        println!("  committed: {message}");
    }
    println!();
    println!("{}", recorded_line(&ticket, &approval.checksum));
    Ok(())
}

/// The message to commit the approval under, or `None` to commit nothing:
/// `--commit <MESSAGE>`, then `--no-commit`, then `approval.commit` with its
/// `approval.commit_message` (invar-yaml.md §3.7).
fn commit_message(
    flag: &Option<String>,
    no_commit: bool,
    config: &Config,
    ticket: &str,
) -> Option<String> {
    if let Some(message) = flag {
        return Some(message.clone());
    }
    (!no_commit && config.approval.commit).then(|| config.approval.message_for(ticket))
}

/// Stage and commit `paths` alone: a Plan written by the planner may be new to
/// git.
fn commit(root: &ProjectRoot, paths: &[&std::path::Path], message: &str) -> Result<()> {
    let relative: Vec<std::path::PathBuf> = paths
        .iter()
        .map(|path| match root.relative_path(path) {
            Ok(Some(relative)) => Ok(relative),
            Ok(None) => bail!("{} is outside the project", path.display()),
            Err(err) => Err(err.into()),
        })
        .collect::<Result<_>>()?;
    let relative: Vec<&std::path::Path> = relative.iter().map(|p| p.as_path()).collect();
    git::stage(root.path(), &relative)?;
    git::commit_paths(root.path(), &relative, message)?;
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
