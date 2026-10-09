//! The Plan body's sections (contract §6).
//!
//! The core reads the body only as far as it needs: the level-2 headings, the
//! text under each, and whether the required ones are there, spelled exactly,
//! in order, and not empty. The prose inside a section is never parsed.

use std::ops::Range;

use crate::diagnostics::Diagnostics;

use super::Mode;

pub(super) const ACCEPTANCE_CRITERIA: &str = "Acceptance criteria";

/// Light mode, `scenarios: false` (§6).
const LIGHT: [&str; 7] = [
    "Ticket",
    "Intent",
    "Scope",
    "Open questions / answers",
    "Rejected alternatives",
    ACCEPTANCE_CRITERIA,
    "Implementation context",
];

/// `scenarios: true` — the 1.0.0 list, unchanged (§6, OQ-2).
const SCENARIOS: [&str; 8] = [
    "Ticket",
    "Intent",
    "Scope",
    "Open questions / answers",
    "Rejected alternatives",
    "Scenarios to write",
    "Scenarios to delete",
    "Implementation context",
];

pub(super) fn required(mode: Mode) -> &'static [&'static str] {
    match mode {
        Mode::Light => &LIGHT,
        Mode::Scenarios => &SCENARIOS,
    }
}

/// A level-2 section of the body: `## <heading>` and the text under it, up to
/// the next level-1 or level-2 heading.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    /// The heading text after `## `, trailing whitespace trimmed.
    pub heading: String,
    /// 1-based line of the heading in the file.
    pub line: usize,
    /// Byte range of the content within the Plan's text after the closing
    /// delimiter.
    pub(super) content: Range<usize>,
}

/// Any line that starts like a heading, for spotting misspellings.
struct HeadingLike {
    text: String,
    line: usize,
}

/// Find every section in `rest` — the text after the closing delimiter, whose
/// first line is line `first_line` of the file — and check the required ones.
pub(super) fn scan(
    rest: &str,
    first_line: usize,
    mode: Mode,
    found: &mut Diagnostics,
) -> Vec<Section> {
    let mut sections: Vec<Section> = Vec::new();
    let mut heading_like: Vec<HeadingLike> = Vec::new();
    let mut fence: Option<(char, usize)> = None;
    let mut offset = 0;

    for (index, (text, eol)) in super::split_lines(rest).into_iter().enumerate() {
        let line = first_line + index;
        let line_start = offset;
        offset += text.len() + eol.len();

        // Code fences: a `## ` inside a fenced block is an example, not a
        // heading. A fence opens with three or more backticks or tildes and
        // closes with at least as many of the same character and nothing
        // after them: a line such as ```` ```php ```` inside an open fence is
        // content, as in CommonMark.
        let unindented = strip_up_to_three_spaces(text);
        if let Some((marker, count, bare)) = fence_marker(unindented) {
            match fence {
                None => fence = Some((marker, count)),
                Some((open, open_count)) if open == marker && count >= open_count && bare => {
                    fence = None;
                }
                Some(_) => {}
            }
            continue;
        }
        if fence.is_some() {
            continue;
        }

        let level_one = text.starts_with("# ") || text == "#";
        let level_two = text.starts_with("## ") || text == "##";
        // A level-1 or level-2 heading ends the section before it.
        if (level_one || level_two)
            && let Some(open) = sections.last_mut()
            && open.content.end == usize::MAX
        {
            open.content.end = line_start;
        }
        if level_two {
            // Exactly `## ` then the name; only trailing whitespace is
            // forgiven (§6). `##  Scope` is reported as a misspelling.
            sections.push(Section {
                heading: text.get(3..).unwrap_or("").trim_end().to_owned(),
                line,
                content: offset..usize::MAX,
            });
        } else if unindented.starts_with('#') {
            heading_like.push(HeadingLike {
                text: text.trim_end().to_owned(),
                line,
            });
        }
    }
    if let Some(open) = sections.last_mut()
        && open.content.end == usize::MAX
    {
        open.content.end = rest.len();
    }
    // A section whose heading is the last line with no line ending still has
    // a valid (empty) range.
    for section in &mut sections {
        section.content.start = section.content.start.min(section.content.end);
    }

    check_required(rest, &sections, &heading_like, mode, found);
    sections
}

fn check_required(
    rest: &str,
    sections: &[Section],
    heading_like: &[HeadingLike],
    mode: Mode,
    found: &mut Diagnostics,
) {
    let required = required(mode);
    // (position in the required order, the section) for each required
    // heading found, in file order.
    let mut present: Vec<(usize, &Section)> = Vec::new();

    for (position, name) in required.iter().enumerate() {
        let matches: Vec<&Section> = sections.iter().filter(|s| s.heading == *name).collect();

        let Some(first) = matches.first() else {
            report_missing(position, name, required, sections, heading_like, found);
            continue;
        };
        for duplicate in &matches[1..] {
            found.at(
                duplicate.line,
                format!(
                    "`## {name}` appears again (first on line {}); a required section \
                     appears once — merge the two",
                    first.line
                ),
            );
        }
        present.push((position, first));

        let content = &rest[first.content.clone()];
        if !has_text(name, content) {
            found.at(first.line, empty_message(name, mode));
        }
    }

    // Order: walk the required headings in file order; one that belongs
    // before a heading already seen is out of place.
    present.sort_by_key(|(_, section)| section.line);
    let mut latest: Option<(usize, &Section)> = None;
    for &(position, section) in &present {
        match latest {
            Some((latest_position, latest_section)) if position < latest_position => {
                found.at(
                    section.line,
                    format!(
                        "`## {}` is out of order: it must come before `## {}` (line {}). \
                         Required sections follow a fixed order (plan-header.md §6)",
                        section.heading, latest_section.heading, latest_section.line
                    ),
                );
            }
            _ => latest = Some((position, section)),
        }
    }
}

fn report_missing(
    position: usize,
    name: &str,
    required: &[&str],
    sections: &[Section],
    heading_like: &[HeadingLike],
    found: &mut Diagnostics,
) {
    let wanted = normalize(name);

    // A level-2 heading that differs only in case, spacing or punctuation.
    if let Some(near) = sections.iter().find(|s| normalize(&s.heading) == wanted) {
        found.at(
            near.line,
            format!(
                "`## {}` should be `## {name}` — required headings are matched \
                 exactly, including capitalisation",
                near.heading
            ),
        );
        return;
    }
    // The right words at the wrong level, or without the space: `### Scope`,
    // `##Scope`.
    if let Some(near) = heading_like
        .iter()
        .find(|h| normalize(h.text.trim_start_matches([' ', '#'])) == wanted)
    {
        found.at(
            near.line,
            format!(
                "`{}` should be `## {name}` — a required section is a level-2 heading, \
                 `## ` followed by the name",
                near.text
            ),
        );
        return;
    }

    let place = match position.checked_sub(1).map(|p| required[p]) {
        Some(previous) => format!("after `## {previous}`"),
        None => "as the first section".to_owned(),
    };
    found.general(format!(
        "missing required section `## {name}`; add it {place}"
    ));
}

/// Does the section hold written content (§6, D-14)?
///
/// A line counts when letters or digits are left on it once HTML comments, a
/// leading list marker and a leading checkbox are removed. So `-`, `1.`,
/// `- [ ]`, `...` and `<!-- fill in -->` are not content. In
/// `## Acceptance criteria` a placeholder line (`_none_`, `TBD`, ...) does not
/// count either (D-8, D-14).
fn has_text(name: &str, content: &str) -> bool {
    let mut in_comment = false;
    content.lines().any(|line| {
        let visible = crate::format::values::without_comments(line, &mut in_comment);
        let text = strip_checkbox(strip_list_marker(visible.trim()));
        text.chars().any(char::is_alphanumeric)
            && !(name == ACCEPTANCE_CRITERIA && is_placeholder(text))
    })
}

/// The line after a leading list marker: `-`, `*` or `+`, or digits followed
/// by `.` or `)` — and only when the marker is followed by a space or ends the
/// line. `**TBD**` and `1.5 seconds` are not list items.
fn strip_list_marker(line: &str) -> &str {
    let ends_marker = |rest: &str| rest.is_empty() || rest.starts_with(char::is_whitespace);

    if let Some(rest) = line.strip_prefix(['-', '*', '+'])
        && ends_marker(rest)
    {
        return rest.trim_start();
    }
    let digits = line.bytes().take_while(u8::is_ascii_digit).count();
    if digits > 0
        && let Some(rest) = line[digits..].strip_prefix(['.', ')'])
        && ends_marker(rest)
    {
        return rest.trim_start();
    }
    line
}

/// The text after a leading task-list checkbox: `[ ]`, `[x]` or `[X]`.
fn strip_checkbox(text: &str) -> &str {
    ["[ ]", "[x]", "[X]"]
        .iter()
        .find_map(|mark| text.strip_prefix(*mark))
        .map_or(text, str::trim_start)
}

/// A line that says "nothing yet" instead of stating a criterion: the whole
/// line is one of a few words, in any capitalisation, with any emphasis or
/// trailing punctuation around it (`**TBD**`, `N/A.`). Deliberately a short
/// list (plan-header.md D-14).
fn is_placeholder(text: &str) -> bool {
    let word = text
        .trim_matches(|c: char| c.is_whitespace() || matches!(c, '_' | '*' | '.' | ':' | '!'))
        .to_lowercase();
    matches!(word.as_str(), "none" | "n/a" | "tbd" | "todo")
}

fn empty_message(name: &str, mode: Mode) -> String {
    let what_to_write = match name {
        ACCEPTANCE_CRITERIA => {
            "add at least one acceptance criterion, one per list item: \
             `- Given …, when …, then ….` Comments, bare list markers and placeholders \
             (`_none_`, `TBD`, `TODO`, `N/A`) do not count; `_none_` is not accepted here \
             (plan-header.md D-8, D-14)"
        }
        "Open questions / answers" => {
            "list the questions and answers, or write one line saying none were needed"
        }
        "Rejected alternatives" => {
            "list the alternatives and why they were dropped, or write one line saying \
             none were considered"
        }
        "Implementation context" => {
            "add the guidance this ticket needs from AI_CONTEXT and BUSINESS_CONTEXT, \
             or write one line saying no context source applies"
        }
        "Scenarios to write" | "Scenarios to delete" if mode == Mode::Scenarios => {
            "list the `.feature` paths, or write `_none_` (plan-header.md D-5)"
        }
        _ => "write its content; this section may not be empty (plan-header.md §6.1)",
    };
    format!("`## {name}` is empty; {what_to_write}")
}

/// Lowercase letters and digits only: `Open Questions/Answers:` and
/// `open questions / answers` normalize alike.
fn normalize(text: &str) -> String {
    text.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

fn strip_up_to_three_spaces(text: &str) -> &str {
    let spaces = text.bytes().take(3).take_while(|b| *b == b' ').count();
    &text[spaces..]
}

/// `Some((char, count, bare))` when the line can open or close a code fence:
/// three or more backticks or tildes. `bare` says nothing but whitespace
/// follows them, which is what a closing fence needs; an opening fence may
/// carry an info string (```` ```php ````).
fn fence_marker(text: &str) -> Option<(char, usize, bool)> {
    let marker = text.chars().next().filter(|c| matches!(c, '`' | '~'))?;
    let count = text.chars().take_while(|c| *c == marker).count();
    // The markers are ASCII, one byte each, so `count` is also a byte offset.
    let bare = text[count..].trim().is_empty();
    (count >= 3).then_some((marker, count, bare))
}
