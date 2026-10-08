//! Render a Plan as a PR description (M1-16; §12, L-6).
//!
//! `invar done` (M1-12) writes the result to `.invar/out/<TICKET>-pr.md`
//! before it deletes the Plan: a Plan added and deleted inside the same PR
//! never appears in the diff, so this file — together with the decision
//! entry (§7) — is what a reviewer or CodeRabbit actually reads.
//!
//! Deterministic by construction: the output is built only from the Plan's
//! own parsed fields, read in the one fixed order below, so the same Plan
//! always renders the same description.

use std::fmt::Write as _;

use crate::digest;
use crate::format::plan::Plan;

#[cfg(test)]
mod tests;

/// The fixed section order (M1-16). `## Ticket` is not part of it: the pasted
/// ticket text is included only when `with_ticket` is set, and when it is, it
/// comes first — the context a reviewer would want before everything the
/// Plan derived from it.
const SECTIONS: [&str; 6] = [
    "Intent",
    "Scope",
    "Open questions / answers",
    "Rejected alternatives",
    "Acceptance criteria",
    "Implementation context",
];

/// Render `plan` as a PR description for `ticket`.
///
/// Does not look at `Status`: a caller decides whether a draft Plan may be
/// rendered, since `invar done` and the standalone `pr-body` command refuse
/// one for different reasons (§12) — `done` through the stronger `plan check`
/// (M1-10), `pr-body` through its own, lighter "not a draft" rule.
pub fn render(plan: &Plan, ticket: &str, with_ticket: bool) -> String {
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

    if with_ticket {
        write_section(&mut out, "Ticket", plan.section("Ticket"));
    }
    for heading in SECTIONS {
        write_section(&mut out, heading, plan.section(heading));
    }
    digest::canonicalize(&out)
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
    let text = content.map(str::trim).filter(|text| !text.is_empty());
    out.push_str(text.unwrap_or("none"));
    out.push('\n');
}
