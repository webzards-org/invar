//! `invar review-input` (M1-20): what the reviewer agent reads, assembled the
//! same way every time.
//!
//! The review is advisory (L-7): it never blocks `done` and never fails a
//! build, and no hook runs it. What it needs from the core is only that its
//! input is **fixed** — the same Plan and the same commits give the same bytes
//! — so that two reviews of one branch disagree because of the reviewer, never
//! because of what it was shown.
//!
//! The input is, in this order:
//!
//! 1. the approved Plan's **Intent, Scope, Acceptance criteria and
//!    Implementation context**, with **Acceptance tests to change** after the
//!    criteria when the Plan declares any — what the branch is judged against.
//!    Not the pasted ticket, the questions or the rejected alternatives: they
//!    explain how the Plan came about, and a reviewer reading them starts to
//!    weigh the planner's reasons instead of the result;
//! 2. the **acceptance tests**, right after them: the Plan's `Acceptance`, and
//!    for `required` the files locked for the ticket that name each criterion,
//!    the criteria none names, and what the developer unlocked — read by
//!    [`pr_body::acceptance::gather`], the same reading the PR description
//!    shows. File paths, not test names: names are known only by running the
//!    tests, and nothing here runs anything. The reviewer reads the tests in
//!    each file and judges whether they assert their criterion (AT-14);
//! 3. the **binding context**, `AI_CONTEXT.md` and `BUSINESS_CONTEXT.md`, as
//!    `context show --binding` prints it — the invariants a change can break,
//!    all of them: the reviewer checks every one, including those the Plan
//!    does not name (M1-26);
//! 4. the **diff** of the branch against the base branch (`git::branch_diff`).
//!    Above [`DIFF_BUDGET`] characters the patch is left out and the changed
//!    files are listed with their line counts instead; the reviewer reads what
//!    it needs from the repository itself.
//!
//! What it never contains: the decision log (`.invar/` is left out of the
//! diff, and the log is not read), and anything of the coder's session. The
//! reviewer's independence is the point of having one.

use std::fmt::Write as _;

use crate::acceptance::verify::no_section_warning;
use crate::context::Binding;
use crate::format::plan::{ACCEPTANCE_TESTS_TO_CHANGE, Plan};
use crate::fs::ProjectRoot;
use crate::git::{self, GitError};
use crate::pr_body::acceptance::Tests;
use crate::pr_body::strip_blank_edges;

#[cfg(test)]
mod tests;

/// The largest patch shown in full, in characters: about 10,000 tokens. A
/// larger one is listed by file instead, because a reviewer handed a patch it
/// cannot hold reads the beginning carefully and the end not at all.
pub const DIFF_BUDGET: usize = 40_000;

/// The Plan sections a review judges against, in the order shown.
const SECTIONS: [&str; 4] = [
    "Intent",
    "Scope",
    "Acceptance criteria",
    "Implementation context",
];

/// The branch's changes, as they will be shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Diff {
    /// Nothing committed on the branch since it left the base.
    Empty,
    /// The whole patch.
    Full(String),
    /// The patch was over the budget: the changed files instead.
    Files {
        /// How long the patch was, in characters.
        chars: usize,
        budget: usize,
        files: Vec<FileStat>,
    },
}

/// One changed file, from `git diff --numstat`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileStat {
    /// Lines added; `None` for a binary file.
    pub added: Option<u64>,
    /// Lines deleted; `None` for a binary file.
    pub deleted: Option<u64>,
    /// As git prints it; a rename reads `old => new`.
    pub path: String,
}

/// The branch's changes in `root` against `base`, within `budget`.
pub fn collect(root: &ProjectRoot, base: &str, budget: usize) -> Result<Diff, GitError> {
    let branch = git::branch_diff(root.path(), base)?;
    Ok(choose(branch.patch, &branch.numstat, budget))
}

/// The whole patch when it fits in `budget` characters, the file list when it
/// does not.
pub fn choose(patch: String, numstat: &str, budget: usize) -> Diff {
    if patch.trim().is_empty() {
        return Diff::Empty;
    }
    let chars = patch.chars().count();
    if chars <= budget {
        return Diff::Full(patch);
    }
    Diff::Files {
        chars,
        budget,
        files: parse_numstat(numstat),
    }
}

/// `<added>\t<deleted>\t<path>` per line; `-` for both counts of a binary
/// file. A line of any other shape is skipped.
pub fn parse_numstat(text: &str) -> Vec<FileStat> {
    text.lines()
        .filter_map(|line| {
            let mut parts = line.splitn(3, '\t');
            let added = parts.next()?;
            let deleted = parts.next()?;
            let path = parts.next()?;
            Some(FileStat {
                added: added.parse().ok(),
                deleted: deleted.parse().ok(),
                path: path.to_owned(),
            })
        })
        .collect()
}

/// The review input. A pure function of its arguments: nothing here reads the
/// clock, the environment or the file system.
///
/// The caller decides that the Plan may be reviewed against — approved, and
/// unchanged since (`plan check`) — exactly as for `pr-body`.
pub fn render(plan: &Plan, binding: &Binding, base: &str, diff: &Diff, tests: &Tests) -> String {
    let header = plan.header();
    let mut out = String::new();

    let _ = writeln!(out, "=== invar review input: {} ===", header.ticket);
    let _ = writeln!(
        out,
        "The Plan below was approved by {} on {} and has not changed since. Judge the \
         branch against it and the binding context, and nothing else: each acceptance \
         criterion `met`, `not met` or `unclear`, with file:line \
         evidence; every invariant of BUSINESS_CONTEXT.md the \
         change appears to break, whether the Plan names it or not; changes outside Scope.",
        header.approved_by.as_deref().unwrap_or("(not approved)"),
        header.approved_at.as_deref().unwrap_or("(not approved)"),
    );

    for heading in SECTIONS {
        write_section(&mut out, plan, heading);
        if heading == "Acceptance criteria" {
            if plan.acceptance_tests_to_change().is_some() {
                write_section(&mut out, plan, ACCEPTANCE_TESTS_TO_CHANGE);
            }
            write_acceptance(&mut out, plan, tests);
        }
    }

    let binding = binding.render();
    if !binding.is_empty() {
        out.push('\n');
        out.push_str(&binding);
    }

    out.push('\n');
    match diff {
        Diff::Empty => {
            let _ = writeln!(out, "=== diff against {base} ===");
            let _ = writeln!(out);
            let _ = writeln!(
                out,
                "Nothing is committed on this branch since it left `{base}` (`.invar/` left \
                 out). There is nothing to review yet: commit the change first."
            );
            let _ = writeln!(out);
            let _ = writeln!(out, "=== end of diff ===");
        }
        Diff::Full(patch) => {
            let _ = writeln!(
                out,
                "=== diff against {base} — what is committed on this branch since it left \
                 `{base}`; `.invar/` left out ==="
            );
            let _ = writeln!(out);
            out.push_str(patch);
            if !patch.ends_with('\n') {
                out.push('\n');
            }
            let _ = writeln!(out);
            let _ = writeln!(out, "=== end of diff ===");
        }
        Diff::Files {
            chars,
            budget,
            files,
        } => {
            let _ = writeln!(
                out,
                "=== changed files against {base} — the diff is {chars} characters, over the \
                 budget of {budget}, so it is not included: read the files you need from \
                 the repository ==="
            );
            let _ = writeln!(out);
            for file in files {
                let _ = match (file.added, file.deleted) {
                    (Some(added), Some(deleted)) => {
                        writeln!(out, "+{added} -{deleted}  {}", file.path)
                    }
                    _ => writeln!(out, "binary  {}", file.path),
                };
            }
            let _ = writeln!(out);
            let _ = writeln!(out, "=== end of changed files ===");
        }
    }
    out
}

/// `=== Acceptance tests ===`: what the reviewer judges the locked tests by.
fn write_acceptance(out: &mut String, plan: &Plan, tests: &Tests) {
    let ticket = &plan.header().ticket;
    let _ = writeln!(out);
    let _ = writeln!(out, "=== Acceptance tests ===");
    let _ = writeln!(out);
    let locked = match tests {
        Tests::Skipped | Tests::NotRecorded => {
            let _ = writeln!(out, "Acceptance: skipped");
            let _ = writeln!(out);
            let _ = writeln!(
                out,
                "The ticket was approved without acceptance tests, so no test is locked for \
                 it. Judge the coder's own tests, if any, as before, and list the criteria no \
                 test covers, as information, not as a defect."
            );
            return;
        }
        Tests::RequiredWithoutConfig => {
            let _ = writeln!(out, "Acceptance: required");
            let _ = writeln!(out);
            let _ = writeln!(
                out,
                "{}: no locked tests can be shown.",
                no_section_warning(ticket)
            );
            return;
        }
        Tests::Required(locked) => locked,
    };

    let _ = writeln!(out, "Acceptance: required");
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "Per criterion, the files locked for {ticket} under `{}` whose text names it. Test \
         names are not listed, since only a run knows them: read the tests in each file, and \
         judge whether each asserts its criterion — `asserts`, `weaker` or `unrelated`.",
        locked.dir
    );
    let _ = writeln!(out);
    for criterion in plan.criteria() {
        let files: Vec<&str> = locked
            .files
            .iter()
            .filter(|file| file.criteria.contains(&criterion.id))
            .map(|file| file.path.as_str())
            .collect();
        if files.is_empty() {
            let _ = writeln!(out, "- {}: named by no locked file", criterion.id);
        } else {
            let _ = writeln!(out, "- {}: {}", criterion.id, files.join(", "));
        }
    }

    let silent: Vec<&str> = locked
        .files
        .iter()
        .filter(|file| file.criteria.is_empty())
        .map(|file| file.path.as_str())
        .collect();
    if !silent.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(out, "Locked files that name no criterion:");
        for path in silent {
            let _ = writeln!(out, "- {path}");
        }
    }

    if !locked.unlocked.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(
            out,
            "Unlocked by the developer during the cycle, after the coder objected to them: \
             read these with extra care, since a criterion is likeliest to have been softened \
             here."
        );
        let _ = writeln!(out);
        for line in &locked.unlocked {
            let _ = writeln!(out, "- {line}");
        }
    }
}

fn write_section(out: &mut String, plan: &Plan, heading: &str) {
    let _ = writeln!(out);
    let _ = writeln!(out, "=== Plan: {heading} ===");
    let _ = writeln!(out);
    out.push_str(&section(plan, heading));
    out.push('\n');
}

/// A section's text without the blank lines around it, line endings
/// normalized; `none` when the Plan has no such section or it is blank.
pub(crate) fn section(plan: &Plan, heading: &str) -> String {
    let normalized = plan
        .section(heading)
        .map(|text| text.replace("\r\n", "\n").replace('\r', "\n"));
    normalized
        .as_deref()
        .map(strip_blank_edges)
        .filter(|text| !text.is_empty())
        .unwrap_or("none")
        .to_owned()
}
