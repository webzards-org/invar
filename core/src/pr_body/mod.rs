//! Render a Plan as a PR description (M1-16; §12, L-6).
//!
//! `invar done` (M1-12) writes the result to `.invar/out/<TICKET>-pr.md`
//! before it deletes the Plan: a Plan added and deleted inside the same PR
//! never appears in the diff, so this file — together with the decision
//! entry (§7) — is what a reviewer or CodeRabbit actually reads.
//!
//! Deterministic by construction: the output is built only from the Plan's
//! own parsed fields and what [`acceptance::gather`] read of its tests, in the
//! one fixed order below, so the same files always render the same description.
//!
//! **How acceptance was decided** comes right after the approval line, where a
//! reader cannot miss it (AT-15): one line when the ticket has no acceptance
//! tests, so nobody has to infer it from a missing section; when it has, the
//! files locked for it, the criteria each names, and what the developer
//! unlocked. `invar done` deletes the unlock record with the Plan, so this is
//! where it survives.

use std::fmt::Write as _;

use crate::acceptance::verify::no_section_warning;
use crate::digest;
use crate::format::plan::{ACCEPTANCE_TESTS_TO_CHANGE, Plan};

pub mod acceptance;

pub use acceptance::Tests;

#[cfg(test)]
mod tests;

/// The fixed section order (M1-16). `## Ticket` is not part of it: the pasted
/// ticket text is included only when `with_ticket` is set, and when it is, it
/// comes first — the context a reviewer would want before everything the
/// Plan derived from it. `## Acceptance tests to change` is not part of it
/// either: it comes right after the criteria, and only when the Plan declares
/// any, because it is what a reviewer must see of a change to another ticket's
/// locked tests (plan-header.md §6.5).
const SECTIONS: [&str; 6] = [
    "Intent",
    "Scope",
    "Open questions / answers",
    "Rejected alternatives",
    "Acceptance criteria",
    "Implementation context",
];

/// Render `plan` as a PR description for `ticket`, with what `tests` found of
/// its acceptance tests.
///
/// Does not look at the approval: a caller decides whether a Plan may be
/// rendered. Both callers — `invar done` and the standalone `pr-body` command
/// — run `plan check` (M1-10) first, so only a Plan that is approved and
/// unchanged since reaches this function, and the "Approved by …" line is
/// true of the text rendered under it.
pub fn render(plan: &Plan, ticket: &str, with_ticket: bool, tests: &Tests) -> String {
    let header = plan.header();
    let mut out = String::new();
    let _ = writeln!(out, "# {ticket}");
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "Approved by {} on {}.",
        header.approved_by.as_deref().unwrap_or("(not approved)"),
        header.approved_at.as_deref().unwrap_or("(not approved)"),
    );
    write_acceptance(&mut out, plan, ticket, tests);

    if with_ticket {
        write_section(&mut out, "Ticket", plan.section("Ticket"));
    }
    for heading in SECTIONS {
        write_section(&mut out, heading, plan.section(heading));
        if heading == "Acceptance criteria"
            && let Some(content) = plan.acceptance_tests_to_change()
        {
            write_section(&mut out, ACCEPTANCE_TESTS_TO_CHANGE, Some(content));
        }
    }
    digest::canonicalize(&out)
}

/// How acceptance was decided, in the first lines (AT-15).
///
/// The criteria shown per file are the ones its text names, by the token rule
/// of test-results.md §4 — what a reviewer can check by opening the file, and
/// not a claim of coverage. That claim was `invar acceptance lock`'s, which saw
/// the tests fail before the code existed.
fn write_acceptance(out: &mut String, plan: &Plan, ticket: &str, tests: &Tests) {
    let _ = writeln!(out);
    let locked = match tests {
        Tests::Skipped => {
            let _ = writeln!(
                out,
                "**No acceptance tests:** this ticket was approved without them."
            );
            return;
        }
        Tests::NotRecorded => {
            let _ = writeln!(
                out,
                "**No acceptance tests:** this ticket was approved before invar recorded that \
                 decision, which counts as without them."
            );
            return;
        }
        Tests::RequiredWithoutConfig => {
            let _ = writeln!(out, "**Acceptance tests:** required");
            let _ = writeln!(out);
            let _ = writeln!(out, "warning: {}", no_section_warning(ticket));
            return;
        }
        Tests::Required(locked) => locked,
    };

    let _ = writeln!(out, "**Acceptance tests:** required");
    let _ = writeln!(out);
    if locked.files.is_empty() {
        let _ = writeln!(
            out,
            "No file under `{}` is locked for {ticket}.",
            locked.dir
        );
    } else {
        let _ = writeln!(
            out,
            "Locked for {ticket} under `{}`, with the criteria each file names:",
            locked.dir
        );
        let _ = writeln!(out);
        for file in &locked.files {
            match file.criteria.as_slice() {
                [] => {
                    let _ = writeln!(out, "- `{}`: names no criterion", file.path);
                }
                ids => {
                    let _ = writeln!(out, "- `{}`: {}", file.path, ids.join(", "));
                }
            }
        }
        let _ = writeln!(out);
        let _ = writeln!(
            out,
            "These are the ids each file's text names, not a proof of coverage: that was \
             `invar acceptance lock`'s, which saw the tests fail before the code existed, \
             and `invar done` checks that every file is still what was locked."
        );
    }

    let named: Vec<&str> = locked
        .files
        .iter()
        .flat_map(|file| file.criteria.iter().map(String::as_str))
        .collect();
    let unnamed: Vec<String> = plan
        .criteria()
        .into_iter()
        .map(|criterion| criterion.id)
        .filter(|id| !named.contains(&id.as_str()))
        .collect();
    if !unnamed.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(out, "Named by no locked file: {}", unnamed.join(", "));
    }

    if !locked.unlocked.is_empty() {
        let _ = writeln!(out);
        for line in &locked.unlocked {
            let _ = writeln!(out, "- {line}");
        }
    }
}

/// `## <heading>`, then its content — or `none` when the Plan has no such
/// section, or the section is blank. A light-mode Plan never leaves one of
/// the fixed headings empty or absent (plan-header.md §6), but a
/// `scenarios: true` Plan has no `## Acceptance criteria` at all (OQ-2), and
/// this is where that case is handled, once, for whichever heading it is.
fn write_section(out: &mut String, heading: &str, content: Option<&str>) {
    let _ = writeln!(out);
    let _ = writeln!(out, "## {heading}");
    let _ = writeln!(out);
    // One line ending before anything is looked at: a Plan may be written
    // with `\r\n` or a lone `\r` (plan-header.md §3), and the blank lines at
    // the edges of a section must go whichever it is.
    let normalized = content.map(|text| text.replace("\r\n", "\n").replace('\r', "\n"));
    let text = normalized
        .as_deref()
        .map(strip_blank_edges)
        .filter(|text| !text.is_empty());
    out.push_str(text.unwrap_or("none"));
    out.push('\n');
}

/// `text` without the blank lines before it and the whitespace after it.
///
/// Not `str::trim`: that would also take the indentation off the first line,
/// and a section that opens with an indented code block, or a nested list,
/// would stop being one in the description a reviewer reads.
///
/// Shared with `review-input` (M1-20), which shows the same sections and must
/// not trim them differently.
pub(crate) fn strip_blank_edges(text: &str) -> &str {
    let mut rest = text;
    while let Some((line, after)) = rest.split_once('\n') {
        if !line.trim().is_empty() {
            break;
        }
        rest = after;
    }
    rest.trim_end()
}
