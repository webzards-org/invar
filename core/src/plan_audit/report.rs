//! `invar plan audit report` (§6, §7) and the line `plan approve` shows from it (§8).
//!
//! **What changed is computed, not told** (D-2). The Plan now is compared with the
//! baseline: a change anywhere but the text under `## Implementation context` and
//! `## Scope` refuses the report, and a change there is printed as a line diff. The agent
//! gives only the reason of each change, paired with the hunks in order.
//!
//! **The verdict comes from the findings' severities alone** (§4, D-1). It is a report,
//! not a gate: `READY` and `NOT READY` both exit 0, and the line `plan approve` reads
//! back from `report.txt` refuses nothing (D-4).

use std::fs;
use std::io;
use std::path::Path;

use crate::digest;
use crate::format::audit::{self, EDITABLE, Finding, Findings, NOT_A_CHECK, Severity};
use crate::format::plan::{Mode, Plan, Status};
use crate::fs::ProjectRoot;
use crate::gate::approval;
use crate::gate::validate;

use super::diff::{self, Hunk};
use super::{AuditError, Files, read, shown};

/// What `report` computed: the text it printed and wrote to `report.txt`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub text: String,
    pub files: Files,
}

/// `invar plan audit report` (§7): compare the Plan with its baseline, run `plan
/// validate`, and render the findings. Writes `report.txt` and returns its text. A
/// refusal writes nothing.
pub fn report(root: &ProjectRoot, spec: &Path, mode: Mode) -> Result<Report, AuditError> {
    let text = read(spec)?;
    let ticket = Plan::check(&text, mode)
        .ticket
        .map(|(ticket, _)| ticket)
        .ok_or_else(|| AuditError::NoTicket {
            spec: spec.to_owned(),
        })?;
    let files = Files::of(root, &ticket);
    if !files.baseline.exists() {
        return Err(AuditError::NoBaseline {
            ticket,
            baseline: files.baseline,
        });
    }
    let baseline_text = read(&files.baseline)?;
    let baseline =
        Plan::read_anyway(&baseline_text, mode).ok_or_else(|| AuditError::BaselineUnreadable {
            ticket: ticket.clone(),
            baseline: files.baseline.clone(),
        })?;
    let findings = match fs::read_to_string(&files.findings) {
        Ok(text) => audit::parse(&text).map_err(|problems| AuditError::FindingsInvalid {
            findings: shown(root, &files.findings),
            problems,
        })?,
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            return Err(AuditError::NoFindings {
                ticket,
                findings: shown(root, &files.findings),
            });
        }
        Err(source) => {
            return Err(AuditError::Io {
                path: files.findings,
                source,
            });
        }
    };

    let now = Plan::read_anyway(&text, mode);
    let refusals = compare(&baseline, now.as_ref());
    let Some(now) = now.filter(|_| refusals.is_empty()) else {
        return Err(AuditError::Changed {
            ticket,
            spec: shown(root, spec),
            baseline: shown(root, &files.baseline),
            refusals,
        });
    };

    let validation = validate::file(root, spec, mode);
    let shown_spec = shown(root, spec);
    let mut out = String::new();
    out.push_str(&format!("plan audit {ticket}: {shown_spec}\n\n"));
    out.push_str("changes:\n");
    out.push_str(&changes(&baseline, &now, &findings));
    if validation.is_valid() {
        out.push_str("plan validate: ok\n");
    } else {
        out.push_str(&format!(
            "plan validate: {} problem(s)\n",
            validation.problems.len()
        ));
        for line in validation.problems.render(Path::new(&shown_spec)).lines() {
            out.push_str(&format!("  {line}\n"));
        }
    }
    let (blocking, notes): (Vec<&Finding>, Vec<&Finding>) = findings
        .findings
        .iter()
        .partition(|finding| finding.severity == Severity::Blocking);
    out.push_str("blocking findings:\n");
    out.push_str(&listed(&blocking));
    out.push_str("notes:\n");
    out.push_str(&listed(&notes));
    out.push_str(&format!(
        "{BODY}{}\n",
        approval::approval_digest(&now, None)
    ));
    out.push_str(&audit::verdict_line(&ticket, findings.blocking()));
    out.push('\n');
    out.push_str(NOT_A_CHECK);
    out.push('\n');

    fs::write(&files.report, &out).map_err(|source| AuditError::Io {
        path: files.report.clone(),
        source,
    })?;
    Ok(Report { text: out, files })
}

const BODY: &str = "body: ";

/// Everything that refuses the report (§6), each named. Empty when the Plan changed only
/// where an audit may change it.
fn compare(baseline: &Plan, now: Option<&Plan>) -> Vec<String> {
    let Some(now) = now else {
        return vec![
            "the header changed: it can no longer be read as a Plan header, between two `---` \
             lines"
                .to_owned(),
        ];
    };
    let mut refusals = Vec::new();
    if now.header().status != Status::Draft {
        refusals.push(
            "the Plan is no longer a draft: an audit is of a draft, and an agent never \
             approves"
                .to_owned(),
        );
    }
    if now.header_text() != baseline.header_text() {
        refusals.push(
            "the header changed, between the two `---` lines: an audit never changes it".to_owned(),
        );
    }
    let headings = |plan: &Plan| -> Vec<String> {
        plan.sections()
            .iter()
            .map(|section| section.heading.clone())
            .collect()
    };
    let (before, after) = (headings(baseline), headings(now));
    if before != after {
        let named = refusals.len();
        for heading in after.iter().filter(|heading| !before.contains(heading)) {
            refusals.push(format!("a section was added: `## {heading}`"));
        }
        for heading in before.iter().filter(|heading| !after.contains(heading)) {
            refusals.push(format!("a section was removed or renamed: `## {heading}`"));
        }
        let common = |of: &[String], other: &[String]| -> Vec<String> {
            of.iter().filter(|h| other.contains(h)).cloned().collect()
        };
        // A heading repeated or dropped where it also stays elsewhere is neither added
        // nor removed by name, so the two lists are shown.
        if common(&before, &after) != common(&after, &before) || refusals.len() == named {
            refusals.push(format!(
                "the sections were moved or repeated: they were {}, and are {}",
                listing(&before),
                listing(&after)
            ));
        }
        return refusals;
    }
    if now.outside_sections() != baseline.outside_sections() {
        refusals.push(
            "text outside every section changed: a heading, or a line before the first \
             section"
                .to_owned(),
        );
    }
    for ((heading, then), (_, text)) in baseline.section_texts().iter().zip(now.section_texts()) {
        if !EDITABLE.contains(heading) && !same_lines(then, text) {
            refusals.push(format!(
                "`## {heading}` changed, and an audit changes only `## Implementation context` \
                 and `## Scope`"
            ));
        }
    }
    refusals
}

fn listing(headings: &[String]) -> String {
    headings
        .iter()
        .map(|heading| format!("`{heading}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The same text, line endings aside: the diff reads lines, so a section whose only
/// change is CRLF for LF has no hunk to show, and is not a change here either.
fn same_lines(a: &str, b: &str) -> bool {
    a.lines().eq(b.lines())
}

/// The `changes:` part (§7.1): each section of the two that changed or has an edit, in
/// Plan order, its hunks paired with its edits in order.
fn changes(baseline: &Plan, now: &Plan, findings: &Findings) -> String {
    let mut out = String::new();
    for ((heading, then), (_, text)) in baseline.section_texts().iter().zip(now.section_texts()) {
        if !EDITABLE.contains(heading) {
            continue;
        }
        let hunks = diff::hunks(then, text);
        let edits: Vec<_> = findings
            .edits
            .iter()
            .filter(|edit| edit.section == *heading)
            .collect();
        if hunks.is_empty() && edits.is_empty() {
            continue;
        }
        out.push_str(&format!("  ## {heading}\n"));
        let mut edits = edits.into_iter();
        for Hunk { removed, added } in &hunks {
            for line in removed {
                out.push_str(&format!("    -{line}\n"));
            }
            for line in added {
                out.push_str(&format!("    +{line}\n"));
            }
            match edits.next() {
                Some(edit) => out.push_str(&format!(
                    "      reason: {} ({})\n",
                    edit.reason, edit.evidence
                )),
                None => out.push_str("      edited without a reason\n"),
            }
        }
        for edit in edits {
            out.push_str(&format!(
                "    reason with no change: {} ({})\n",
                edit.reason, edit.evidence
            ));
        }
    }
    if out.is_empty() {
        out.push_str("  none\n");
    }
    out
}

/// `  <kind> (<section>, <evidence>): <text>` per finding, or `  none`.
fn listed(findings: &[&Finding]) -> String {
    if findings.is_empty() {
        return "  none\n".to_owned();
    }
    findings
        .iter()
        .map(|finding| {
            format!(
                "  {} ({}, {}): {}\n",
                finding.kind, finding.section, finding.evidence, finding.text
            )
        })
        .collect()
}

/// The line `plan approve` prints (§8), for `plan` as it is about to be approved.
/// `not run` when there is no `report.txt`, or its last lines cannot be read.
pub fn gate_line(root: &ProjectRoot, plan: &Plan) -> String {
    let ticket = &plan.header().ticket;
    let files = Files::of(root, ticket);
    let verdict = fs::read(&files.report)
        .ok()
        .and_then(|bytes| digest::canonicalize_bytes(&bytes).ok())
        .and_then(|text| recorded(&text, ticket));
    let Some((verdict, body)) = verdict else {
        return "plan audit: not run".to_owned();
    };
    if body == approval::approval_digest(plan, None) {
        format!("plan audit: {verdict}, for this version of the Plan")
    } else {
        format!("plan audit: {verdict}, but the Plan changed after it")
    }
}

/// The verdict as the gate words it, `READY` or `NOT READY (<n> blocking)`, and the body
/// digest, from a report's last three lines.
fn recorded(text: &str, ticket: &str) -> Option<(String, String)> {
    let lines: Vec<&str> = text.lines().collect();
    let [.., body, verdict, last] = lines.as_slice() else {
        return None;
    };
    if *last != NOT_A_CHECK {
        return None;
    }
    let body = body.strip_prefix(BODY)?;
    let verdict = verdict.strip_prefix(&format!("plan audit {ticket}: "))?;
    let verdict = if verdict == "READY" {
        "READY".to_owned()
    } else {
        let count: usize = verdict
            .strip_prefix("NOT READY — ")?
            .strip_suffix(" blocking finding(s)")?
            .parse()
            .ok()
            .filter(|count| *count > 0)?;
        format!("NOT READY ({count} blocking)")
    };
    Some((verdict, body.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::recorded;

    const LAST: &str =
        "this is an agent's assessment of the Plan, not a check; approving is yours.";

    #[test]
    fn the_last_lines_of_a_report_are_read_back() {
        let text = format!("…\nbody: sha256:ab\nplan audit SHOP-214: READY\n{LAST}\n");
        assert_eq!(
            recorded(&text, "SHOP-214"),
            Some(("READY".to_owned(), "sha256:ab".to_owned()))
        );
        let text = format!(
            "body: sha256:cd\nplan audit SHOP-214: NOT READY — 2 blocking finding(s)\n{LAST}\n"
        );
        assert_eq!(
            recorded(&text, "SHOP-214"),
            Some(("NOT READY (2 blocking)".to_owned(), "sha256:cd".to_owned()))
        );
    }

    #[test]
    fn last_lines_that_are_not_a_report_s_are_not_read() {
        let ok = format!("body: sha256:ab\nplan audit SHOP-214: READY\n{LAST}\n");
        assert!(recorded(&ok, "SHOP-215").is_none());
        assert!(recorded("plan audit SHOP-214: READY\n", "SHOP-214").is_none());
        assert!(recorded(&ok.replace(LAST, "approved."), "SHOP-214").is_none());
        let zero = format!(
            "body: sha256:ab\nplan audit SHOP-214: NOT READY — 0 blocking finding(s)\n{LAST}\n"
        );
        assert!(recorded(&zero, "SHOP-214").is_none());
        assert!(recorded(&ok.replace("body: ", "digest: "), "SHOP-214").is_none());
    }
}
