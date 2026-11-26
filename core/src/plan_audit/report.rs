//! `invar plan audit report` (§6, §7) and the lines `plan approve` computes (§8).
//!
//! **What changed is computed, not told** (D-2). The Plan now is compared with the
//! baseline: a change anywhere but the text under `## Implementation context` and
//! `## Scope` refuses the report, and a change there is printed as a line diff. The agent
//! gives only the reason of each change, paired with the hunks in order.
//!
//! **The verdict comes from the findings' severities alone** (§4, D-1). It is a report,
//! not a gate: `READY` and `NOT READY` both exit 0.
//!
//! **The gate computes, and never reads back** (§8, D-4). `report.txt` is the developer's
//! copy of the text, and any tool can write a file: the lines `plan approve` shows come
//! from the same reading, comparison and pairing as the report, run again on the
//! baseline, the findings and the Plan as it is about to be approved.

use std::fs;
use std::io;
use std::path::Path;

use crate::format::audit::{self, EDITABLE, Edit, Finding, Findings, NOT_A_CHECK, Severity};
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
    let Audit {
        files,
        baseline,
        findings,
    } = load(root, &ticket, mode)?;

    let now = Plan::read_anyway(&text, mode);
    let refusals = match &now {
        Some(now) => compare(&baseline, now).refusals(),
        None => vec![
            "the header changed: it can no longer be read as a Plan header, between two `---` \
             lines"
                .to_owned(),
        ],
    };
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

/// The files of one audit, read: what the report and the gate both start from.
struct Audit {
    files: Files,
    baseline: Plan,
    findings: Findings,
}

/// Read the baseline and the findings of `ticket`'s audit (§7): refused without a
/// baseline, with one that is not a Plan, without `findings.md`, and with one that is not
/// valid.
fn load(root: &ProjectRoot, ticket: &str, mode: Mode) -> Result<Audit, AuditError> {
    let files = Files::of(root, ticket);
    if !files.baseline.exists() {
        return Err(AuditError::NoBaseline {
            ticket: ticket.to_owned(),
            baseline: files.baseline,
        });
    }
    let baseline_text = read(&files.baseline)?;
    let baseline =
        Plan::read_anyway(&baseline_text, mode).ok_or_else(|| AuditError::BaselineUnreadable {
            ticket: ticket.to_owned(),
            baseline: files.baseline.clone(),
        })?;
    let findings = match fs::read_to_string(&files.findings) {
        Ok(text) => audit::parse(&text).map_err(|problems| AuditError::FindingsInvalid {
            findings: shown(root, &files.findings),
            problems,
        })?,
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            return Err(AuditError::NoFindings {
                ticket: ticket.to_owned(),
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
    Ok(Audit {
        files,
        baseline,
        findings,
    })
}

/// How the Plan now differs from its baseline (§6).
struct Comparison {
    not_draft: bool,
    header_changed: bool,
    /// A section added, removed, renamed, moved or repeated, each named. When there is
    /// any, the sections are not compared one by one.
    structure: Vec<String>,
    outside_changed: bool,
    /// The headings of the sections other than the two that changed, in Plan order.
    forbidden: Vec<String>,
}

impl Comparison {
    /// Everything that refuses the report, each named. Empty when the Plan changed only
    /// where an audit may change it.
    fn refusals(&self) -> Vec<String> {
        let mut refusals = Vec::new();
        if self.not_draft {
            refusals.push(
                "the Plan is no longer a draft: an audit is of a draft, and an agent never \
                 approves"
                    .to_owned(),
            );
        }
        if self.header_changed {
            refusals.push(
                "the header changed, between the two `---` lines: an audit never changes it"
                    .to_owned(),
            );
        }
        refusals.extend(self.structure.iter().cloned());
        if self.outside_changed {
            refusals.push(
                "text outside every section changed: a heading, or a line before the first \
                 section"
                    .to_owned(),
            );
        }
        for heading in &self.forbidden {
            refusals.push(format!(
                "`## {heading}` changed, and an audit changes only `## Implementation context` \
                 and `## Scope`"
            ));
        }
        refusals
    }

    /// Whether the sections are the same, in the same order, with the same text outside
    /// them: what pairing the sections one by one needs.
    fn same_sections(&self) -> bool {
        self.structure.is_empty() && !self.outside_changed
    }
}

fn compare(baseline: &Plan, now: &Plan) -> Comparison {
    let mut comparison = Comparison {
        not_draft: now.header().status != Status::Draft,
        header_changed: now.header_text() != baseline.header_text(),
        structure: Vec::new(),
        outside_changed: false,
        forbidden: Vec::new(),
    };
    let headings = |plan: &Plan| -> Vec<String> {
        plan.sections()
            .iter()
            .map(|section| section.heading.clone())
            .collect()
    };
    let (before, after) = (headings(baseline), headings(now));
    if before != after {
        let structure = &mut comparison.structure;
        for heading in after.iter().filter(|heading| !before.contains(heading)) {
            structure.push(format!("a section was added: `## {heading}`"));
        }
        for heading in before.iter().filter(|heading| !after.contains(heading)) {
            structure.push(format!("a section was removed or renamed: `## {heading}`"));
        }
        let common = |of: &[String], other: &[String]| -> Vec<String> {
            of.iter().filter(|h| other.contains(h)).cloned().collect()
        };
        // A heading repeated or dropped where it also stays elsewhere is neither added
        // nor removed by name, so the two lists are shown.
        if common(&before, &after) != common(&after, &before) || structure.is_empty() {
            structure.push(format!(
                "the sections were moved or repeated: they were {}, and are {}",
                listing(&before),
                listing(&after)
            ));
        }
        return comparison;
    }
    comparison.outside_changed = now.outside_sections() != baseline.outside_sections();
    for ((heading, then), (_, text)) in baseline.section_texts().iter().zip(now.section_texts()) {
        if !EDITABLE.contains(heading) && !same_lines(then, text) {
            comparison.forbidden.push((*heading).to_owned());
        }
    }
    comparison
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

/// One of the two sections an audit may edit, with its hunks and the edits paired with
/// them in order (§7.1).
struct Paired<'a> {
    heading: &'a str,
    hunks: Vec<(Hunk, Option<&'a Edit>)>,
    /// Edits left over once every hunk has one.
    unpaired: Vec<&'a Edit>,
}

/// Each of the two sections that changed or has an edit, in Plan order. The sections of
/// the two Plans must be the same (`Comparison::same_sections`).
fn pair<'a>(baseline: &'a Plan, now: &'a Plan, findings: &'a Findings) -> Vec<Paired<'a>> {
    let mut paired = Vec::new();
    for ((heading, then), (_, text)) in baseline
        .section_texts()
        .into_iter()
        .zip(now.section_texts())
    {
        if !EDITABLE.contains(&heading) {
            continue;
        }
        let hunks = diff::hunks(then, text);
        let mut edits = findings.edits.iter().filter(|edit| edit.section == heading);
        let hunks: Vec<_> = hunks.into_iter().map(|hunk| (hunk, edits.next())).collect();
        let unpaired: Vec<_> = edits.collect();
        if !hunks.is_empty() || !unpaired.is_empty() {
            paired.push(Paired {
                heading,
                hunks,
                unpaired,
            });
        }
    }
    paired
}

/// The `changes:` part (§7.1).
fn changes(baseline: &Plan, now: &Plan, findings: &Findings) -> String {
    let mut out = String::new();
    for section in pair(baseline, now, findings) {
        out.push_str(&format!("  ## {}\n", section.heading));
        for (Hunk { removed, added }, edit) in &section.hunks {
            for line in removed {
                out.push_str(&format!("    -{line}\n"));
            }
            for line in added {
                out.push_str(&format!("    +{line}\n"));
            }
            match edit {
                Some(edit) => out.push_str(&format!(
                    "      reason: {} ({})\n",
                    edit.reason, edit.evidence
                )),
                None => out.push_str("      edited without a reason\n"),
            }
        }
        for edit in &section.unpaired {
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

/// The lines `plan approve` prints (§8), for `plan`, the Plan about to be approved:
/// computed from the baseline, the findings and `plan`, never read from `report.txt`. One
/// line, then one for each section the auditor may not edit that changed since `begin`.
/// It refuses nothing.
pub fn gate_lines(root: &ProjectRoot, plan: &Plan, mode: Mode) -> String {
    let audit = match load(root, &plan.header().ticket, mode) {
        Ok(audit) => audit,
        Err(AuditError::FindingsInvalid { .. }) => {
            return "plan audit: findings unreadable".to_owned();
        }
        Err(_) => return "plan audit: not run".to_owned(),
    };
    let blocking = audit.findings.blocking();
    let verdict = if blocking == 0 {
        "READY".to_owned()
    } else {
        format!("NOT READY ({blocking} blocking)")
    };
    let comparison = compare(&audit.baseline, plan);
    // The header is not compared: approving writes it.
    let accounted = comparison.same_sections()
        && comparison.forbidden.is_empty()
        && pair(&audit.baseline, plan, &audit.findings)
            .iter()
            .all(|section| section.hunks.iter().all(|(_, edit)| edit.is_some()));
    let mut lines = vec![if accounted {
        format!("plan audit: {verdict}, for this version of the Plan")
    } else {
        format!("plan audit: {verdict}, but the Plan changed after it")
    }];
    if !comparison.same_sections() {
        lines.push("plan audit: the sections changed since the audit began".to_owned());
    }
    for heading in &comparison.forbidden {
        lines.push(format!(
            "plan audit: the {heading} changed since the audit began"
        ));
    }
    lines.join("\n")
}
