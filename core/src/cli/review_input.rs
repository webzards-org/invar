//! `invar review-input <TICKET | PATH>` (M1-20).
//!
//! Prints what the reviewer agent reads: the approved Plan's Intent, Scope,
//! Acceptance criteria, Acceptance tests to change when it declares any, the
//! ticket's locked acceptance tests per criterion, and Implementation context,
//! the binding context, and the branch's diff against
//! the base branch. stdout carries only that; what a
//! person needs to know and the reviewer does not goes to stderr.
//!
//! Advisory, like the review itself (L-7): no hook runs it, and `done` does not
//! depend on it.

use anyhow::{Context as _, Result};

use crate::context;
use crate::format::plan::{Mode, Plan};
use crate::git;
use crate::pr_body::acceptance;
use crate::review_input;

use super::plan::{require_approved, target};

#[derive(Debug, clap::Args)]
pub struct Args {
    /// A ticket id (`PROJ-412`), or the path of a Plan: its `spec.md` or its
    /// directory.
    pub target: String,
}

pub fn run(args: Args) -> Result<()> {
    let cwd = std::env::current_dir().context("cannot read the current directory")?;
    let (root, spec) = target::resolve(&args.target, &cwd)?;
    let config = super::config(&root)?;
    let mode = Mode::of(&config);

    // Reviewing against a draft, or against text changed after it was
    // approved, proves nothing: the question is the one `plan check` asks.
    require_approved::require(&root, &spec, mode, &cwd).context(
        "a review checks the branch against an approved specification, and this Plan is not one",
    )?;
    let plan = Plan::read(&spec, mode)?;

    let plan_dir = spec.parent().unwrap_or(&spec);
    let (tests, warnings) = acceptance::gather(&root, &config, &plan, plan_dir)?;
    for warning in &warnings {
        eprintln!("warning: {warning}");
    }

    let binding = context::load_binding(&root)?;
    for name in binding.unrecognized() {
        eprintln!(
            "warning: {name} exists but is not recognized, so it is not in the review input: \
             its first non-blank line must be `{}`",
            context::MARKER
        );
    }

    let base = git::base_branch(root.path(), config.git.base_branch.as_deref())?;
    let pending = git::uncommitted(root.path())?
        .into_iter()
        .filter(|path| !path.components().any(|part| part.as_os_str() == ".invar"))
        .count();
    if pending > 0 {
        eprintln!(
            "warning: {pending} uncommitted {} not in the review input, which covers what is \
             committed on this branch; commit first to have {} reviewed",
            if pending == 1 {
                "change is"
            } else {
                "changes are"
            },
            if pending == 1 { "it" } else { "them" },
        );
    }

    let diff = review_input::collect(&root, &base, review_input::DIFF_BUDGET)?;
    print!(
        "{}",
        review_input::render(&plan, &binding, &base, &diff, &tests)
    );
    Ok(())
}
