//! The two sections the core reads line by line: `## Acceptance criteria` as
//! far as its ids go (contract §6.4) and `## Acceptance tests to change`
//! (§6.5).
//!
//! The id is the join key for everything after the gate, so its form is checked
//! here, and nothing else about a criterion is: the Given/When/Then prose stays
//! the person's to judge at the gate. A path in `## Acceptance tests to change`
//! is only a token here; whether the file exists and is locked is another
//! command's question.

use std::collections::BTreeMap;

use crate::diagnostics::Diagnostics;
use crate::format::values::without_comments;

use super::sections::{is_placeholder, strip_checkbox, strip_list_marker};

/// A list item's text, read for an id.
enum Id<'a> {
    /// `AC<n>: <rest>`. `rest` is empty when nothing follows the colon.
    Valid {
        number: u64,
        rest: &'a str,
    },
    /// Meant as an id, and not one: `AC01`, `ac1`, `AC1` without `:`.
    Malformed {
        token: &'a str,
    },
    None,
}

/// `AC`, a positive integer with no leading zero, then `:` and a space (§6.4).
fn parse_id(text: &str) -> Id<'_> {
    let bytes = text.as_bytes();
    let separated = usize::from(matches!(bytes.get(2), Some(b' ' | b'-' | b'_')));
    let start = 2 + separated;
    let digits = bytes.get(start..).map_or(0, |rest| {
        rest.iter().take_while(|b| b.is_ascii_digit()).count()
    });
    let meant_as_id = text
        .get(..2)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("ac"))
        && digits > 0;
    if !meant_as_id {
        return Id::None;
    }

    let end = start + digits;
    let colon = usize::from(bytes.get(end) == Some(&b':'));
    let malformed = Id::Malformed {
        token: &text[..end + colon],
    };
    if !text.starts_with("AC") || separated == 1 || bytes[start] == b'0' {
        return malformed;
    }
    let Ok(number) = text[start..end].parse() else {
        return malformed;
    };
    match text[end..].strip_prefix(':') {
        Some("") => Id::Valid { number, rest: "" },
        Some(rest) if rest.starts_with(' ') => Id::Valid {
            number,
            rest: rest.trim_start(),
        },
        _ => malformed,
    }
}

/// The criterion's own words: a line's text without its id, so that `AC1: TBD`
/// is the placeholder it looks like (§6.4).
pub(super) fn without_id(text: &str) -> &str {
    match parse_id(text) {
        Id::Valid { rest, .. } => rest,
        _ => text,
    }
}

/// Check the ids in `## Acceptance criteria`, whose `content` starts at line
/// `first_line` of the file. The caller has established that the section holds
/// a criterion.
///
/// `unnumbered_allowed` is D-16: criteria with no id at all are valid as they
/// were approved under 1.5.0. One id anywhere means the Plan is numbered, and
/// every rule applies.
pub(super) fn check_ids(
    content: &str,
    first_line: usize,
    unnumbered_allowed: bool,
    found: &mut Diagnostics,
) {
    let mut problems = Diagnostics::new();
    let mut ids: Vec<(u64, usize)> = Vec::new();
    // Items that should have held an id. Each may be what fills a gap, so a gap
    // they explain is not reported on top of them.
    let mut without_valid_id: u64 = 0;
    let mut seen_item = false;
    let mut in_comment = false;

    for (index, (raw, _)) in super::split_lines(content).into_iter().enumerate() {
        let line = first_line + index;
        let visible = without_comments(raw, &mut in_comment);
        let trimmed = visible.trim();
        let after_marker = strip_list_marker(trimmed);
        let is_item = after_marker.len() != trimmed.len();
        let text = strip_checkbox(after_marker);
        if !text.chars().any(char::is_alphanumeric) || is_placeholder(text) {
            continue;
        }
        // A line indented under an item belongs to it: a wrapped criterion
        // keeps one id. An id is never swallowed that way, or a criterion
        // would vanish from the list with nothing reported.
        let belongs_to_item = seen_item && is_indented(&visible);

        match parse_id(text) {
            Id::Valid { number, rest } if is_item => {
                ids.push((number, line));
                seen_item = true;
                if !rest.chars().any(char::is_alphanumeric) || is_placeholder(rest) {
                    problems.at(
                        line,
                        format!(
                            "`AC{number}` has no criterion: nothing but a placeholder follows \
                             the id. State the criterion on this line, or remove the item and \
                             renumber the rest (plan-header.md §6.4)"
                        ),
                    );
                }
            }
            Id::Valid { .. } => {
                without_valid_id += 1;
                problems.at(line, not_a_list_item());
            }
            Id::Malformed { token } if is_item => {
                without_valid_id += 1;
                seen_item = true;
                problems.at(
                    line,
                    format!(
                        "`{token}` is not a criterion id; an id is `AC`, a positive integer \
                         with no leading zero, then `:` and a space, as in \
                         `- AC1: Given …, when …, then ….` (plan-header.md §6.4)"
                    ),
                );
            }
            Id::Malformed { .. } | Id::None if belongs_to_item => {}
            Id::Malformed { .. } => problems.at(line, not_a_list_item()),
            Id::None if is_item => {
                without_valid_id += 1;
                seen_item = true;
                problems.at(
                    line,
                    "this criterion has no id; start the list item with its id, in order from \
                     `AC1`: `- AC1: Given …, when …, then ….` (plan-header.md §6.4)",
                );
            }
            Id::None => problems.at(line, not_a_list_item()),
        }
    }

    if unnumbered_allowed && ids.is_empty() {
        return;
    }
    check_sequence(&ids, without_valid_id, &mut problems);
    found.extend(problems);
}

fn not_a_list_item() -> &'static str {
    "every criterion is a numbered list item, and this line is not one; make it \
     `- AC<n>: …`, indent it under the criterion it belongs to, or move it to another \
     section (plan-header.md §6.4)"
}

/// Two spaces or a tab: at least as far as the text of a `- ` item. One space
/// is an item's own margin.
fn is_indented(line: &str) -> bool {
    line.starts_with('\t') || line.starts_with("  ")
}

/// Unique, in order, contiguous from `AC1` (§6.4).
fn check_sequence(ids: &[(u64, usize)], without_valid_id: u64, problems: &mut Diagnostics) {
    let mut first_seen: BTreeMap<u64, usize> = BTreeMap::new();
    let mut highest: Option<(u64, usize)> = None;

    for &(number, line) in ids {
        if let Some(first) = first_seen.get(&number) {
            problems.at(
                line,
                format!(
                    "`AC{number}` appears again (first on line {first}); each criterion has \
                     its own id — renumber them (plan-header.md §6.4)"
                ),
            );
            continue;
        }
        first_seen.insert(number, line);
        match highest {
            Some((above, above_line)) if number < above => problems.at(
                line,
                format!(
                    "`AC{number}` is out of order: it comes after `AC{above}` (line \
                     {above_line}); ids appear in order — renumber them (plan-header.md §6.4)"
                ),
            ),
            _ => highest = Some((number, line)),
        }
    }

    let Some((top, _)) = highest else {
        return;
    };
    let missing = top - first_seen.len() as u64;
    if missing <= without_valid_id {
        return;
    }
    let mut previous = 0;
    for (&number, &line) in &first_seen {
        if number > previous + 1 {
            let lost = if number == previous + 2 {
                format!("`AC{}` is missing", previous + 1)
            } else {
                format!("`AC{}` to `AC{}` are missing", previous + 1, number - 1)
            };
            let place = if previous == 0 {
                "is the first id".to_owned()
            } else {
                format!("follows `AC{previous}`")
            };
            problems.at(
                line,
                format!(
                    "`AC{number}` {place}, so {lost}; ids are contiguous from `AC1` — add the \
                     criterion or renumber (plan-header.md §6.4)"
                ),
            );
        }
        previous = number;
    }
}

const ITEM_FORM: &str = "`- <path> — change — <reason>` or `- <path> — delete — <reason>`";

/// Check `## Acceptance tests to change`, whose heading is on `heading_line`
/// and whose `content` starts on the line after it (§6.5).
pub(super) fn check_tests_to_change(content: &str, heading_line: usize, found: &mut Diagnostics) {
    let mut items = 0;
    let mut none_line = None;
    let mut in_comment = false;

    for (index, (raw, _)) in super::split_lines(content).into_iter().enumerate() {
        let line = heading_line + 1 + index;
        let visible = without_comments(raw, &mut in_comment);
        let text = visible.trim_end();
        if text.trim() == "_none_" {
            none_line = Some(line);
            continue;
        }
        if text == "-" {
            items += 1;
            found.at(line, missing_part("path"));
            continue;
        }
        let Some(item) = text.strip_prefix("- ") else {
            // Comments and punctuation are not content (D-14).
            if text.chars().any(char::is_alphanumeric) {
                items += 1;
                found.at(
                    line,
                    format!(
                        "an item of `## Acceptance tests to change` is one unindented line: \
                         {ITEM_FORM} (plan-header.md §6.5)"
                    ),
                );
            }
            continue;
        };
        items += 1;
        check_item(item, line, found);
    }

    match none_line {
        None if items == 0 => found.at(
            heading_line,
            format!(
                "`## Acceptance tests to change` is empty; list each locked test this ticket \
                 changes or deletes as {ITEM_FORM}, write `_none_`, or remove the section \
                 (plan-header.md §6.5)"
            ),
        ),
        Some(line) if items > 0 => found.at(
            line,
            "`_none_` is the single line of a section with nothing in it, and this section \
             lists items; remove it (plan-header.md §6.5)",
        ),
        _ => {}
    }
}

fn check_item(item: &str, line: usize, found: &mut Diagnostics) {
    let (path, after_path) = next_field(item);
    if path.is_empty() {
        found.at(line, missing_part("path"));
    } else if path.contains(char::is_whitespace) {
        found.at(
            line,
            format!(
                "`{path}` is not one path followed by ` — `; the separators are a space, an \
                 em dash and a space: {ITEM_FORM} (plan-header.md §6.5)"
            ),
        );
        return;
    }
    let Some(after_path) = after_path else {
        if !path.is_empty() {
            found.at(line, missing_part("action"));
        }
        return;
    };

    let (action, reason) = next_field(after_path);
    match action {
        "change" | "delete" => {}
        "" => found.at(line, missing_part("action")),
        other => found.at(
            line,
            format!(
                "unknown action `{other}`; the action is exactly `change` or `delete`: \
                 {ITEM_FORM} (plan-header.md §6.5)"
            ),
        ),
    }
    let reason = reason.unwrap_or("").trim();
    if !reason.chars().any(char::is_alphanumeric) || is_placeholder(reason) {
        found.at(line, missing_part("reason"));
    }
}

/// The field up to the next separator, and what follows it. `None` when there
/// is no separator; a separator that ends the line has an empty field after it.
fn next_field(text: &str) -> (&str, Option<&str>) {
    let text = text.trim();
    if text == "—" {
        return ("", Some(""));
    }
    if let Some(rest) = text.strip_prefix("— ") {
        return ("", Some(rest));
    }
    if let Some((field, rest)) = text.split_once(" — ") {
        return (field.trim(), Some(rest));
    }
    if let Some(field) = text.strip_suffix(" —") {
        return (field.trim(), Some(""));
    }
    (text, None)
}

fn missing_part(part: &str) -> String {
    format!(
        "this item of `## Acceptance tests to change` has no {part}; write it as \
         {ITEM_FORM} (plan-header.md §6.5)"
    )
}
