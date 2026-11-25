//! `invar pr-body <TICKET | PATH> [--with-ticket] [--out <PATH>]` (M1-16), and
//! `--from-approval [--base <ref>]` (#168): the Plan as it was at its last signed
//! approval on the branch, read from git, with who signed it.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};

use crate::approval_verify::{self, APPROVERS};
use crate::cycle::short;
use crate::format::plan::{Mode, Plan};
use crate::format::values::is_ticket;
use crate::fs::plans::parse_dir_name;
use crate::git;
use crate::pr_body;

use super::plan::{require_approved, target};

#[derive(Debug, clap::Args)]
pub struct Args {
    /// A ticket id (`PROJ-412`), or the path of a Plan: its `spec.md` or its
    /// directory.
    ///
    /// Anything containing `/`, ending in `.md`, or `.`/`..` is a path;
    /// everything else is looked up as a ticket under `.invar/plans/`.
    pub target: String,

    /// Include the pasted `## Ticket` text. Left out by default — the
    /// tracker already holds it.
    #[arg(long)]
    pub with_ticket: bool,

    /// Write the description to this file instead of stdout, creating its
    /// directory if there is none. Prints the path it wrote.
    #[arg(long)]
    pub out: Option<PathBuf>,

    /// Render the Plan as it was at its last approval on this branch that is signed by
    /// an approver of the base, read from git, with a line naming who signed it
    /// (principal and key fingerprint). Takes a ticket id. For a project whose base has
    /// `approval.signed: true`; refused, naming why, when no approval of the ticket on
    /// the branch passes `invar approval verify`'s check of it. Works after `invar done`
    /// has deleted the Plan.
    #[arg(long)]
    pub from_approval: bool,

    /// With `--from-approval`: the branch the pull request targets, as `approval
    /// verify` takes it. Without it, the branch `origin/HEAD` points to, else `main`.
    #[arg(long, value_name = "REF", requires = "from_approval")]
    pub base: Option<String>,
}

pub fn run(args: Args) -> Result<()> {
    let cwd = std::env::current_dir().context("cannot read the current directory")?;
    if args.from_approval {
        return from_approval(&args, &cwd);
    }
    let (root, spec) = target::resolve(&args.target, &cwd)?;
    let config = super::config(&root)?;
    let mode = Mode::of(&config);

    // A PR description describes an approved specification: one that has been
    // approved, and has not changed since. That is the question `plan check`
    // asks, and this asks it too, in the same words. A lighter
    // rule — refusing only a draft — would let a Plan edited after approval be
    // described under an "Approved by …" line it no longer has any right to.
    require_approved::require(&root, &spec, mode, &cwd).context(
        "a PR description describes an approved specification, and this Plan is not one",
    )?;

    let plan = Plan::read(&spec, mode)?;
    let ticket = plan.header().ticket.clone();

    let plan_dir = spec.parent().unwrap_or(&spec);
    let (tests, warnings) = pr_body::acceptance::gather(&root, &config, &plan, plan_dir)?;
    for warning in &warnings {
        eprintln!("warning: {warning}");
    }

    let invariants = crate::context::invariants::load(&root)?;
    let rendered = pr_body::render(&plan, &ticket, args.with_ticket, &tests, &invariants, None);
    write_out(&rendered, args.out.as_deref(), &cwd)
}

/// `--from-approval`: the approvals `approval verify` finds on the branch, the last one
/// of the ticket, and the Plan as that commit has it.
fn from_approval(args: &Args, cwd: &Path) -> Result<()> {
    let ticket = args.target.as_str();
    if !is_ticket(ticket) {
        bail!(
            "`--from-approval` takes a ticket id such as `SHOP-214`, and reads its Plan from \
             git; `{ticket}` is not one"
        );
    }
    let root = crate::fs::project_root()?;
    let config = super::config(&root)?;
    let mode = Mode::of(&config);
    let report = approval_verify::check(root.path(), args.base.as_deref())?;
    if let Some(off) = &report.off {
        bail!("`--from-approval` renders the Plan a person signed, and the check is off: {off}");
    }
    let of_ticket = |spec: &str| {
        Path::new(spec)
            .parent()
            .and_then(Path::file_name)
            .is_some_and(|name| {
                parse_dir_name(&name.to_string_lossy()).is_some_and(|(found, _)| found == ticket)
            })
    };
    let Some(signed) = report
        .approvals
        .iter()
        .rev()
        .find(|signed| signed.ticket == ticket)
    else {
        let problems: Vec<String> = report
            .problems
            .iter()
            .filter(|problem| of_ticket(&problem.spec))
            .map(|problem| {
                format!(
                    "\n  {} ({}): {}",
                    short(&problem.commit),
                    problem.subject,
                    problem.what
                )
            })
            .collect();
        if problems.is_empty() {
            bail!(
                "this branch has no approval of {ticket} signed by a key in {}:{APPROVERS}, so \
                 there is no signed Plan to render",
                report.base
            );
        }
        bail!(
            "no approval of {ticket} on this branch is signed by a key in {}:{APPROVERS}, so \
             there is no signed Plan to render:{}",
            report.base,
            problems.concat()
        );
    };
    let bytes = git::file_at(root.path(), &signed.commit, &signed.spec)?
        .with_context(|| format!("{} is not at {}", signed.spec, short(&signed.commit)))?;
    let text = String::from_utf8(bytes)
        .with_context(|| format!("{} at {} is not UTF-8", signed.spec, short(&signed.commit)))?;
    let plan = Plan::parse(&text, mode).map_err(|problems| {
        anyhow::anyhow!(
            "{} at {} is not a valid Plan:\n{}",
            signed.spec,
            short(&signed.commit),
            problems.render(Path::new(&signed.spec)).trim_end()
        )
    })?;

    let plan_dir = root.path().join(&signed.spec);
    let plan_dir = plan_dir.parent().unwrap_or(root.path());
    let (tests, warnings) = pr_body::acceptance::gather(&root, &config, &plan, plan_dir)?;
    for warning in &warnings {
        eprintln!("warning: {warning}");
    }
    let invariants = crate::context::invariants::load(&root)?;
    let line = format!(
        "Signed by {} ({}), approved at {} ({}), verified against {}:{APPROVERS}.",
        signed.verified.principal,
        signed.verified.fingerprint,
        short(&signed.commit),
        signed.subject,
        report.base
    );
    let rendered = pr_body::render(
        &plan,
        ticket,
        args.with_ticket,
        &tests,
        &invariants,
        Some(&line),
    );
    write_out(&rendered, args.out.as_deref(), cwd)
}

/// To `out`, creating its directory, and say where; else to stdout.
fn write_out(rendered: &str, out: Option<&Path>, cwd: &Path) -> Result<()> {
    match out {
        Some(path) => {
            // `.invar/out/` is the usual place, and `init` does not create
            // it: a description asked for there must not fail on that.
            if let Some(parent) = path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
            {
                fs::create_dir_all(parent)
                    .with_context(|| format!("cannot create {}", parent.display()))?;
            }
            fs::write(path, rendered)
                .with_context(|| format!("cannot write {}", path.display()))?;
            println!("{}", target::shown(path, cwd));
        }
        None => print!("{rendered}"),
    }
    Ok(())
}
