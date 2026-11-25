//! `findings.md`: what the plan-auditor writes (`contracts/formats/plan-audit.md` §3, §4;
//! section numbers below point into it).
//!
//! **Line by line, never Markdown.** Each line is blank, a `#` comment, an `edit:` or a
//! `finding:`, with fields separated by ` | `; the last field is the rest of the line.
//! Every line that is none of these is an error naming its line, and every error is
//! reported in one run.
//!
//! **The verdict is computed here, from the severities alone** (§4, D-1): `NOT READY`
//! when any finding is `blocking`, `READY` otherwise. `blocking` is accepted only on a
//! kind of the closed list, so the agent cannot invent a gate of its own, nor take one
//! away by renaming a kind.

// `invar plan audit report` is its caller, in the next change. `expect` rather than
// `allow`, so the compiler asks for this to go once it is used.
#![cfg_attr(
    not(test),
    expect(dead_code, reason = "plan audit report is not built yet")
)]

use std::fmt;

use crate::diagnostics::Diagnostics;

#[cfg(test)]
mod tests;

/// The sections an `edit` may name (§5.2).
pub const EDITABLE: [&str; 2] = ["Implementation context", "Scope"];

/// The sections a `finding` may name: the required sections of a light-mode Plan
/// (`plan-header.md` §6).
pub const SECTIONS: [&str; 7] = [
    "Ticket",
    "Intent",
    "Scope",
    "Open questions / answers",
    "Rejected alternatives",
    "Acceptance criteria",
    "Implementation context",
];

/// The closed list of blocking kinds, with what each means (§4).
pub const BLOCKING_KINDS: [(&str, &str); 6] = [
    (
        "criterion-cannot-pass",
        "a criterion cannot pass as written, against the code",
    ),
    ("criteria-contradict", "two criteria cannot both pass"),
    (
        "ticket-sentence-uncovered",
        "a sentence of the ticket that no criterion covers",
    ),
    (
        "criterion-without-ground",
        "a criterion with no ground in the ticket",
    ),
    (
        "invariant-not-named",
        "an invariant the ticket plainly touches, not named in Implementation context; its \
         INV-<n> in the text when BUSINESS_CONTEXT.md has ids",
    ),
    (
        "fact-contradicted",
        "a fact in Implementation context the code contradicts, which the auditor could not \
         correct",
    ),
];

/// `blocking` or `note`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Blocking,
    Note,
}

/// Where a finding or an edit was read from: a line of a file, or the ticket.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Evidence {
    Line {
        path: String,
        from: usize,
        to: Option<usize>,
    },
    Ticket,
}

impl fmt::Display for Evidence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Line {
                path,
                from,
                to: None,
            } => write!(f, "{path}:{from}"),
            Self::Line {
                path,
                from,
                to: Some(to),
            } => write!(f, "{path}:{from}-{to}"),
            Self::Ticket => write!(f, "ticket"),
        }
    }
}

/// The reason for one change the auditor made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    pub section: String,
    pub evidence: Evidence,
    pub reason: String,
    /// 1-based line in `findings.md`.
    pub line: usize,
}

/// Something the auditor reports instead of changing it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub kind: String,
    pub severity: Severity,
    pub section: String,
    pub evidence: Evidence,
    pub text: String,
    pub line: usize,
}

/// A valid `findings.md`, in file order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Findings {
    pub edits: Vec<Edit>,
    pub findings: Vec<Finding>,
}

impl Findings {
    /// How many findings are blocking: the verdict is `NOT READY` when this is not zero.
    pub fn blocking(&self) -> usize {
        self.findings
            .iter()
            .filter(|finding| finding.severity == Severity::Blocking)
            .count()
    }
}

/// The last line of a report (§7.1): `plan audit <TICKET>: READY`, or `… NOT READY — <n>
/// blocking finding(s)`.
pub fn verdict_line(ticket: &str, blocking: usize) -> String {
    if blocking == 0 {
        format!("plan audit {ticket}: READY")
    } else {
        format!("plan audit {ticket}: NOT READY — {blocking} blocking finding(s)")
    }
}

/// The line after it, always (D-1).
pub const NOT_A_CHECK: &str =
    "this is an agent's assessment of the Plan, not a check; approving is yours.";

/// The separator between fields.
const SEPARATOR: &str = " | ";

/// Parse `findings.md`. Every malformed line is a problem at its line.
pub fn parse(text: &str) -> Result<Findings, Diagnostics> {
    let mut found = Diagnostics::new();
    let mut findings = Findings::default();
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(text);
    for (index, raw) in text.lines().enumerate() {
        let line = index + 1;
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("edit:") {
            if let Some(edit) = edit(rest, line, &mut found) {
                findings.edits.push(edit);
            }
        } else if let Some(rest) = trimmed.strip_prefix("finding:") {
            if let Some(finding) = finding(rest, line, &mut found) {
                findings.findings.push(finding);
            }
        } else {
            found.at(
                line,
                "a line is `edit: …`, `finding: …`, a `#` comment or blank \
                 (plan-audit.md §3)",
            );
        }
    }
    found.finish(findings)
}

/// `count` fields of `rest`, split at the first `count - 1` separators and trimmed: the
/// last is the rest of the line.
fn fields(rest: &str, count: usize) -> Option<Vec<String>> {
    // A space after the line, so that a last field left empty, `… |`, is still a field.
    let rest = format!("{} ", rest.trim());
    let fields: Vec<String> = rest
        .splitn(count, SEPARATOR)
        .map(|field| field.trim().to_owned())
        .collect();
    (fields.len() == count).then_some(fields)
}

fn edit(rest: &str, line: usize, found: &mut Diagnostics) -> Option<Edit> {
    let Some(parts) = fields(rest, 3) else {
        found.at(
            line,
            "an edit is `edit: <section> | <evidence> | <reason>`, three fields separated by \
             ` | `",
        );
        return None;
    };
    let [section, evidence, reason] = <[String; 3]>::try_from(parts).ok()?;
    let before = found.len();
    if !EDITABLE.contains(&section.as_str()) {
        found.at(
            line,
            format!(
                "an edit is of `Implementation context` or `Scope` only, and this one names \
                 `{section}`: report anything else as a finding (plan-audit.md §5.2)"
            ),
        );
    }
    let evidence = parse_evidence(&evidence, line, found);
    if reason.is_empty() {
        found.at(line, "an edit gives its reason: the last field is empty");
    }
    (found.len() == before).then(|| Edit {
        section,
        evidence: evidence.expect("checked above"),
        reason,
        line,
    })
}

fn finding(rest: &str, line: usize, found: &mut Diagnostics) -> Option<Finding> {
    let Some(parts) = fields(rest, 5) else {
        found.at(
            line,
            "a finding is `finding: <kind> | <severity> | <section> | <evidence> | <text>`, five \
             fields separated by ` | `",
        );
        return None;
    };
    let [kind, severity, section, evidence, text] = <[String; 5]>::try_from(parts).ok()?;
    let before = found.len();
    if !is_kind(&kind) {
        found.at(
            line,
            format!(
                "`{kind}` is not a kind: lowercase letters and digits in words joined by `-`, \
                 such as `criterion-cannot-pass`"
            ),
        );
    }
    let severity = match severity.as_str() {
        "blocking" => Some(Severity::Blocking),
        "note" => Some(Severity::Note),
        other => {
            found.at(
                line,
                format!("the severity is `blocking` or `note`, and this one is `{other}`"),
            );
            None
        }
    };
    if severity == Some(Severity::Blocking) && !BLOCKING_KINDS.iter().any(|(k, _)| *k == kind) {
        let kinds: Vec<&str> = BLOCKING_KINDS.iter().map(|(kind, _)| *kind).collect();
        found.at(
            line,
            format!(
                "`{kind}` is not a blocking kind, so it is a `note`: blocking is one of {} \
                 (plan-audit.md §4)",
                kinds.join(", ")
            ),
        );
    }
    if !SECTIONS.contains(&section.as_str()) {
        found.at(
            line,
            format!(
                "`{section}` is not a section; name one of: {}",
                SECTIONS.join(", ")
            ),
        );
    }
    let evidence = parse_evidence(&evidence, line, found);
    if text.is_empty() {
        found.at(
            line,
            "a finding says what it found: the last field is empty",
        );
    }
    (found.len() == before).then(|| Finding {
        kind,
        severity: severity.expect("checked above"),
        section,
        evidence: evidence.expect("checked above"),
        text,
        line,
    })
}

/// `^[a-z][a-z0-9]*(-[a-z0-9]+)*$`.
fn is_kind(kind: &str) -> bool {
    kind.split('-').enumerate().all(|(index, word)| {
        let mut chars = word.chars();
        match chars.next() {
            Some(first) if index == 0 => {
                first.is_ascii_lowercase()
                    && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
            }
            Some(first) => {
                (first.is_ascii_lowercase() || first.is_ascii_digit())
                    && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
            }
            None => false,
        }
    })
}

/// `<path>:<line>`, `<path>:<line>-<line>`, or `ticket`.
fn parse_evidence(evidence: &str, line: usize, found: &mut Diagnostics) -> Option<Evidence> {
    if evidence == "ticket" {
        return Some(Evidence::Ticket);
    }
    let bad = |found: &mut Diagnostics| {
        found.at(
            line,
            format!(
                "`{evidence}` is not evidence: `<path>:<line>`, `<path>:<line>-<line>` with a \
                 path of the repository, or `ticket`"
            ),
        );
        None
    };
    let Some((path, lines)) = evidence.rsplit_once(':') else {
        return bad(found);
    };
    if path.is_empty() || path.contains(char::is_whitespace) {
        return bad(found);
    }
    let number = |text: &str| text.parse::<usize>().ok().filter(|n| *n >= 1);
    let (from, to) = match lines.split_once('-') {
        Some((from, to)) => match (number(from), number(to)) {
            (Some(from), Some(to)) if to >= from => (from, Some(to)),
            _ => return bad(found),
        },
        None => match number(lines) {
            Some(from) => (from, None),
            None => return bad(found),
        },
    };
    Some(Evidence::Line {
        path: path.to_owned(),
        from,
        to,
    })
}
