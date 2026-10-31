//! The Plan: `.invar/plans/<TICKET>-<slug>/spec.md` (§4a).
//!
//! Implements `contracts/formats/plan-header.md` v1.6.0; section numbers and
//! decision ids below (`§4.1`, `D-2`) point into it.
//!
//! **Reading** splits the file into four parts — opening delimiter, header,
//! closing delimiter, the rest — and checks every rule the contract puts on
//! them: the header's keys and values (§4.2, §5) and the required sections for
//! the configured mode (§6). Every problem is collected; one run reports them
//! all ([`Diagnostics`]).
//!
//! **Writing** reproduces the file byte for byte. Nothing is parsed into a
//! structure and serialized back: every line is kept as it was read, with its
//! own line ending, and a command that changes a field (`plan approve`,
//! `decision record --type failed`) rewrites only that field's value. Unknown
//! keys, comments, blank lines, CRLF, a BOM and the whole body survive
//! untouched (D-2). That is also why this is not a YAML parser: a serde round
//! trip drops comments and unknown keys.
//!
//! **The body** is everything from the first non-blank line after the closing
//! delimiter (§4.3, D-1). It is what `Approved-checksum` covers, so its
//! boundary is the delicate part: blank lines added or removed between the
//! header and the body must not change it.

// The plan and decision commands (M1-8 to M1-11) use most of it. `sections`
// has no caller yet; `read` is called only by `fs::plans::open`, which has
// none either. `expect` rather than `allow`, so the compiler asks for this
// line to go once they are used.
#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "sections and read have no caller in the binary yet"
    )
)]

mod criteria;
mod header;
mod sections;

#[cfg(test)]
mod tests;

use std::fmt;
use std::fs::{self, File};
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};

pub use header::{Header, Status};
pub use sections::{ACCEPTANCE_TESTS_TO_CHANGE, Section};

use crate::diagnostics::Diagnostics;
use crate::format::config::Config;

use header::HeaderLine;

/// Exactly this, and nothing else, opens and closes the header (§4.1).
const DELIMITER: &str = "---";

/// Which sections a Plan must have, selected by `scenarios` in `invar.yaml`
/// (§6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// `scenarios: false`, and the default: M1.
    Light,
    /// `scenarios: true`: the required headings of §6 only. The list rules of
    /// §6.2 arrive with the milestone that introduces `.feature` files.
    Scenarios,
}

impl Mode {
    pub fn of(config: &Config) -> Self {
        if config.scenarios {
            Self::Scenarios
        } else {
            Self::Light
        }
    }
}

/// A parsed Plan that satisfies the contract, ready to be read, changed in
/// its command-written fields, and written back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    header: Header,
    bom: bool,
    opening: Line,
    header_lines: Vec<HeaderLine>,
    closing: Line,
    /// Everything after the closing delimiter's line, verbatim.
    rest: String,
    /// Where the body starts within `rest` (D-1).
    body_start: usize,
    sections: Vec<Section>,
    mode: Mode,
}

/// When criteria with no ids at all are valid (D-16).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Unnumbered {
    /// The rule of the contract: when the Plan says it is approved.
    IfApproved,
    /// A new approval is about to be issued.
    Never,
    /// A Plan just returned to draft, whose body must not change.
    Always,
}

impl Plan {
    /// Parse a Plan's text, checking every rule of the contract for `mode`.
    pub fn parse(text: &str, mode: Mode) -> Result<Self, Diagnostics> {
        Self::parse_with(text, mode, Unnumbered::IfApproved)
    }

    /// Parse a Plan that `reset_to_draft` just wrote. It may be an approval from
    /// before ids existed: its body stays as it was, so the draft it is now has
    /// no ids until a person revises it, and reading it back is no reason to
    /// call that a failure (D-16).
    pub fn parse_reset(text: &str, mode: Mode) -> Result<Self, Diagnostics> {
        Self::parse_with(text, mode, Unnumbered::Always)
    }

    fn parse_with(text: &str, mode: Mode, unnumbered: Unnumbered) -> Result<Self, Diagnostics> {
        let analysis = analyze(text, mode, unnumbered);
        match analysis.plan {
            Some(plan) if analysis.checked.problems.is_empty() => Ok(plan),
            _ => Err(analysis.checked.problems),
        }
    }

    /// Check a Plan's text without needing it to be valid: every problem, and
    /// what could be read of `Ticket` and `Status` despite them. This is
    /// `plan validate` (M1-8), which reports a header that disagrees with its
    /// directory in the same run as a missing section.
    pub fn check(text: &str, mode: Mode) -> Checked {
        analyze(text, mode, Unnumbered::IfApproved).checked
    }

    /// Every problem the text has as a draft, whatever its `Status` says: what
    /// a new approval requires of it (D-16).
    pub fn problems_as_draft(text: &str, mode: Mode) -> Diagnostics {
        analyze(text, mode, Unnumbered::Never).checked.problems
    }

    /// Read and parse a Plan file.
    pub fn read(path: &Path, mode: Mode) -> Result<Self, PlanError> {
        let bytes = fs::read(path).map_err(|source| PlanError::Unreadable {
            path: path.to_owned(),
            source,
        })?;
        let text = std::str::from_utf8(&bytes).map_err(|err| {
            let mut found = Diagnostics::new();
            found.general(format!(
                "not valid UTF-8 (first invalid byte at offset {}); save the Plan as UTF-8",
                err.valid_up_to()
            ));
            PlanError::Invalid {
                path: path.to_owned(),
                problems: found,
            }
        })?;
        Self::parse(text, mode).map_err(|problems| PlanError::Invalid {
            path: path.to_owned(),
            problems,
        })
    }

    /// The Plan as text, byte-identical to what was read except for fields a
    /// command has changed.
    pub fn to_text(&self) -> String {
        let mut out = String::with_capacity(self.rest.len() + 512);
        if self.bom {
            out.push('\u{FEFF}');
        }
        self.opening.write_to(&mut out);
        for line in &self.header_lines {
            line.write_to(&mut out);
        }
        self.closing.write_to(&mut out);
        out.push_str(&self.rest);
        out
    }

    /// Write the Plan to `path`, replacing the file atomically: the new
    /// content goes to a temporary file in the same directory, which is then
    /// renamed over the old one. A crash halfway leaves the old Plan intact,
    /// never half an approval.
    pub fn write(&self, path: &Path) -> io::Result<()> {
        let temporary = temporary_path(path);
        let result = (|| {
            let mut file = File::create(&temporary)?;
            file.write_all(self.to_text().as_bytes())?;
            file.sync_all()?;
            fs::rename(&temporary, path)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }

    // --- reading -------------------------------------------------------------

    pub fn header(&self) -> &Header {
        &self.header
    }

    /// The body: what `Approved-checksum` is computed over, after
    /// canonicalization (§4d). Starts at the first non-blank line after the
    /// closing delimiter (D-1).
    pub fn body(&self) -> &str {
        &self.rest[self.body_start..]
    }

    /// Every level-2 section, in file order — required and additional alike.
    pub fn sections(&self) -> &[Section] {
        &self.sections
    }

    /// The text under `## <heading>`, if the Plan has that section.
    pub fn section(&self, heading: &str) -> Option<&str> {
        self.sections
            .iter()
            .find(|section| section.heading == heading)
            .map(|section| &self.rest[section.content.clone()])
    }

    /// `## Acceptance criteria`, which `plan approve` prints before approving
    /// (M1-9).
    pub fn acceptance_criteria(&self) -> Option<&str> {
        self.section(sections::ACCEPTANCE_CRITERIA)
    }

    /// `## Acceptance tests to change`, when this Plan declares any (§6.5):
    /// `None` when the section is absent or says `_none_`, and with
    /// `scenarios: true`, where it is an ordinary additional section (D-17).
    /// What a reader shows only when there is something to show.
    pub fn acceptance_tests_to_change(&self) -> Option<&str> {
        if self.mode != Mode::Light {
            return None;
        }
        let content = self.section(ACCEPTANCE_TESTS_TO_CHANGE)?;
        let mut in_comment = false;
        let says_none = content.lines().any(|line| {
            crate::format::values::without_comments(line, &mut in_comment).trim() == "_none_"
        });
        (!says_none).then_some(content)
    }

    // --- the command-written fields -----------------------------------------

    /// Mark the Plan approved (M1-9): `Status: approved` and the three
    /// approval fields. Only those four values change; every other byte of
    /// the file stays as it was.
    pub fn approve(&mut self, approval: Approval) -> Result<(), String> {
        approval.check()?;
        self.set(header::STATUS, Status::Approved.as_str());
        self.set(header::APPROVED_BY, &approval.by);
        self.set(header::APPROVED_AT, &approval.at);
        self.set(header::APPROVED_CHECKSUM, &approval.checksum);
        self.header.status = Status::Approved;
        self.header.approved_by = Some(approval.by);
        self.header.approved_at = Some(approval.at);
        self.header.approved_checksum = Some(approval.checksum);
        Ok(())
    }

    /// Return the Plan to `draft` and clear every approval value
    /// (`decision record --type failed`, M1-11). The keys stay, empty (D-3);
    /// the body is untouched.
    pub fn reset_to_draft(&mut self) {
        self.set(header::STATUS, Status::Draft.as_str());
        for key in [
            header::APPROVED_BY,
            header::APPROVED_AT,
            header::APPROVED_CHECKSUM,
            header::LOCKFILE_CHECKSUM,
        ] {
            self.set(key, "");
        }
        self.header.status = Status::Draft;
        self.header.approved_by = None;
        self.header.approved_at = None;
        self.header.approved_checksum = None;
        self.header.lockfile_checksum = None;
    }

    /// Change a field's value in place. Absent keys stay absent: the parser
    /// guarantees the required ones exist, and `Lockfile-checksum` is only
    /// ever cleared, never added.
    fn set(&mut self, key: &str, value: &str) {
        if let Some(line) = self
            .header_lines
            .iter_mut()
            .find(|line| line.key() == Some(key))
        {
            line.set_value(value);
        }
    }
}

/// What [`Plan::check`] found.
#[derive(Debug)]
pub struct Checked {
    /// Every problem; empty for a valid Plan.
    pub problems: Diagnostics,
    /// The header's `Ticket` and its 1-based line, when the value has the
    /// shape of a ticket id — even if other fields are wrong.
    pub ticket: Option<(String, usize)>,
    /// The header's `Status`, when it is `draft` or `approved` — even if
    /// other fields are wrong.
    pub status: Option<Status>,
    /// The header's `Module`, when it has the shape of a module name — even
    /// if other fields are wrong.
    pub module: Option<String>,
}

struct Analysis {
    /// Built only when the header is complete; valid only when there are no
    /// problems.
    plan: Option<Plan>,
    checked: Checked,
}

/// Split the file into its four parts and check every rule (§4–§6).
fn analyze(text: &str, mode: Mode, unnumbered: Unnumbered) -> Analysis {
    let mut found = Diagnostics::new();
    let stop = |found| Analysis {
        plan: None,
        checked: Checked {
            problems: found,
            ticket: None,
            status: None,
            module: None,
        },
    };

    let (bom, text) = match text.strip_prefix('\u{FEFF}') {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    let lines = split_lines(text);

    // §4.1: the very first line is the opening delimiter.
    match lines.first() {
        Some((first, _)) if *first == DELIMITER => {}
        Some((first, _)) => {
            let hint = if first.trim().is_empty() {
                "remove the blank lines above it"
            } else if first.trim() == DELIMITER {
                "remove the spaces around it"
            } else {
                "a Plan starts with its header, between two `---` lines"
            };
            found.at(1, format!("the first line must be exactly `---`; {hint}"));
            return stop(found);
        }
        None => {
            found.general("the file is empty; a Plan starts with a `---` line");
            return stop(found);
        }
    }

    // §4.1: the header ends at the next line that is exactly `---`. Any
    // later `---` is body content.
    let Some(closing_index) = lines
        .iter()
        .skip(1)
        .position(|(text, _)| *text == DELIMITER)
        .map(|position| position + 1)
    else {
        found.at(
            1,
            "the header opened here is never closed; add a line that is exactly `---` \
             after the last header key",
        );
        return stop(found);
    };

    let to_line = |(text, eol): &(&str, &str)| Line {
        text: (*text).to_owned(),
        eol: (*eol).to_owned(),
    };
    let header_raw: Vec<Line> = lines[1..closing_index].iter().map(to_line).collect();
    let parsed = header::parse(&header_raw, 2, &mut found);

    // Everything after the closing line, as one verbatim string. Its byte
    // offset is the sum of the lines before it.
    let consumed: usize = lines[..=closing_index]
        .iter()
        .map(|(text, eol)| text.len() + eol.len())
        .sum();
    let rest = text[consumed..].to_owned();
    let rest_first_line = closing_index + 2;

    // §4.3, D-1: the body starts at the first non-blank line.
    let mut body_start = rest.len();
    let mut offset = 0;
    for (line, eol) in split_lines(&rest) {
        if !line.trim().is_empty() {
            body_start = offset;
            break;
        }
        offset += line.len() + eol.len();
    }

    let unnumbered_allowed = match unnumbered {
        Unnumbered::IfApproved => parsed.status == Some(Status::Approved),
        Unnumbered::Never => false,
        Unnumbered::Always => true,
    };
    let sections = sections::scan(&rest, rest_first_line, mode, unnumbered_allowed, &mut found);

    let plan = parsed.header.map(|header| Plan {
        header,
        bom,
        opening: to_line(&lines[0]),
        header_lines: parsed.lines,
        closing: to_line(&lines[closing_index]),
        rest,
        body_start,
        sections,
        mode,
    });
    Analysis {
        plan,
        checked: Checked {
            problems: found,
            ticket: parsed.ticket,
            status: parsed.status,
            module: parsed.module,
        },
    }
}

/// What `plan approve` records.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Approval {
    /// `git config user.name`.
    pub by: String,
    /// `YYYY-MM-DD`.
    pub at: String,
    /// The §4d digest of the body.
    pub checksum: String,
}

impl Approval {
    /// Values a header line can hold and read back unchanged. `Plan::approve`
    /// asks it before it writes, and `plan approve` asks it first of all, before
    /// a person is shown anything to approve.
    pub fn check(&self) -> Result<(), String> {
        let by = self.by.as_str();
        if by.trim().is_empty() || by != by.trim() {
            return Err(format!(
                "the approver's name must be non-empty, without surrounding spaces: {by:?}"
            ));
        }
        if by.contains(['\n', '\r']) || by.contains(" #") || by.contains("\t#") {
            return Err(format!(
                "the approver's name cannot be written into the header as it is: {by:?}"
            ));
        }
        if !header::is_date(&self.at) {
            return Err(format!(
                "the approval date must be `YYYY-MM-DD`: {:?}",
                self.at
            ));
        }
        if !header::is_digest(&self.checksum) {
            return Err(format!(
                "the checksum must be `sha256:` and 64 lowercase hex digits: {:?}",
                self.checksum
            ));
        }
        Ok(())
    }
}

// --- errors -----------------------------------------------------------------

/// Why a Plan file could not be used.
#[derive(Debug)]
pub enum PlanError {
    Unreadable {
        path: PathBuf,
        source: io::Error,
    },
    /// Read, but not a valid Plan: every problem found, in one list.
    Invalid {
        path: PathBuf,
        problems: Diagnostics,
    },
}

impl fmt::Display for PlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unreadable { path, source } => {
                write!(f, "cannot read {}: {source}", path.display())
            }
            Self::Invalid { path, problems } => {
                let count = problems.len();
                let noun = if count == 1 { "problem" } else { "problems" };
                write!(
                    f,
                    "{} is not a valid Plan ({count} {noun}):\n{}",
                    path.display(),
                    problems.render(path).trim_end()
                )
            }
        }
    }
}

impl std::error::Error for PlanError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Unreadable { source, .. } => Some(source),
            Self::Invalid { problems, .. } => Some(problems),
        }
    }
}

// --- lines, kept with their own endings --------------------------------------

/// One line and the exact terminator that ended it: `\n`, `\r\n`, `\r`, or
/// nothing on a last line without one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Line {
    pub(super) text: String,
    pub(super) eol: String,
}

impl Line {
    fn write_to(&self, out: &mut String) {
        out.push_str(&self.text);
        out.push_str(&self.eol);
    }
}

/// Split into `(text, terminator)` pairs. All three terminators are accepted
/// (§3), and each line keeps its own: a file with mixed endings writes back
/// with the same mix. Concatenating the pairs gives back the input.
fn split_lines(text: &str) -> Vec<(&str, &str)> {
    let bytes = text.as_bytes();
    let mut lines = Vec::new();
    let mut start = 0;
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'\n' => {
                lines.push((&text[start..index], &text[index..index + 1]));
                index += 1;
                start = index;
            }
            b'\r' => {
                let end = if bytes.get(index + 1) == Some(&b'\n') {
                    index + 2
                } else {
                    index + 1
                };
                lines.push((&text[start..index], &text[index..end]));
                index = end;
                start = index;
            }
            _ => index += 1,
        }
    }
    if start < bytes.len() {
        lines.push((&text[start..], ""));
    }
    lines
}

/// `spec.md` → `.spec.md.invar-tmp`, beside it.
fn temporary_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    path.with_file_name(format!(".{name}.invar-tmp"))
}
