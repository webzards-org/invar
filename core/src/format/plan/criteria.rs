//! The two sections the core reads line by line: `## Acceptance criteria` as
//! far as its ids go (contract §6.4) and `## Acceptance tests to change`
//! (§6.5).
//!
//! The id is the join key for everything after the gate, so its form is checked
//! here, and nothing else about a criterion is: the Given/When/Then prose stays
//! the person's to judge at the gate. A path in `## Acceptance tests to change`
//! is only a token here; whether the file exists and is locked is another
//! command's question.
//!
//! Reading the criteria ([`parse_criteria`]) and checking their ids
//! ([`check_ids`]) see each line through the same functions, so they cannot
//! disagree about what a criterion is.

use std::collections::BTreeMap;

use crate::diagnostics::Diagnostics;
use crate::format::values::without_comments;

use super::sections::{is_placeholder, strip_checkbox, strip_list_marker};

/// One acceptance criterion: its id and its text, with a wrapped criterion as one
/// line (§6.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Criterion {
    /// `AC<n>`.
    pub id: String,
    pub text: String,
    /// The invariants it cites after its id, `AC3 (INV-2, INV-4):`, by number, in
    /// the order written (§6.4). Empty when it cites none.
    pub invariants: Vec<u64>,
}

/// A list item's text, read for an id.
enum Id<'a> {
    /// `AC<n>: <rest>` or `AC<n> (INV-<m>, …): <rest>`. `rest` is empty when
    /// nothing follows the colon.
    Valid {
        number: u64,
        invariants: Vec<u64>,
        rest: &'a str,
    },
    /// An id followed by a parenthesis that is not a list of invariant ids:
    /// `AC3 (INV-0):`, `AC3 (inv-2):`, `AC3 (INV-2`.
    BadCitation {
        id: u64,
        citation: &'a str,
    },
    /// Meant as an id, and not one: `AC01`, `ac1`, `AC1` without `:`.
    Malformed {
        token: &'a str,
    },
    /// An id with emphasis around it: `**AC1:**`, `_AC1_:`, `` `AC1:` ``.
    Emphasised {
        token: &'a str,
    },
    None,
}

/// Where the digits of what looks like an id start and end in `text`: `AC`, in
/// any case, perhaps a separator, then digits.
fn id_digits(text: &str) -> Option<(usize, usize)> {
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
    meant_as_id.then_some((start, start + digits))
}

fn is_emphasis(byte: &u8) -> bool {
    matches!(byte, b'*' | b'_' | b'`')
}

/// An id inside emphasis: the opening marks and what looks like an id, then any
/// colon and closing marks, in the order they come. The closing marks may be
/// further on, when the emphasis covers the whole criterion.
fn emphasised_id(text: &str, opening: usize) -> Id<'_> {
    let Some((_, digits_end)) = id_digits(&text[opening..]) else {
        return Id::None;
    };
    let bytes = text.as_bytes();
    let mut end = opening + digits_end;
    let mut colon = bytes.get(end) == Some(&b':');
    end += usize::from(colon);
    while bytes.get(end).is_some_and(is_emphasis) {
        end += 1;
    }
    colon = !colon && bytes.get(end) == Some(&b':');
    end += usize::from(colon);
    Id::Emphasised {
        token: &text[..end],
    }
}

/// `AC`, a positive integer with no leading zero, then `:` and a space (§6.4).
fn parse_id(text: &str) -> Id<'_> {
    let bytes = text.as_bytes();
    let opening = bytes.iter().take_while(|b| is_emphasis(b)).count();
    if opening > 0 {
        return emphasised_id(text, opening);
    }
    let Some((start, end)) = id_digits(text) else {
        return Id::None;
    };
    let separated = start - 2;

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
    // `AC3 (INV-2, INV-4): …`: the invariants it covers, between the id and the colon.
    let (invariants, after) = match text[end..].strip_prefix(" (") {
        Some(inside) => {
            let close = inside.find(')');
            let cited = close.and_then(|close| citation(&inside[..close]));
            match (close, cited) {
                (Some(close), Some(cited)) => (cited, &inside[close + 1..]),
                _ => {
                    // From the `(` to the `)`, or to the colon when it is never closed.
                    let length = close.map_or_else(
                        || inside.find(':').unwrap_or(inside.len()),
                        |close| close + 1,
                    );
                    return Id::BadCitation {
                        id: number,
                        citation: text[end + 1..end + 2 + length].trim_end(),
                    };
                }
            }
        }
        None => (Vec::new(), &text[end..]),
    };
    match after.strip_prefix(':') {
        Some("") => Id::Valid {
            number,
            invariants,
            rest: "",
        },
        Some(rest) if rest.starts_with(' ') => Id::Valid {
            number,
            invariants,
            rest: rest.trim_start(),
        },
        _ => malformed,
    }
}

/// `INV-2` or `INV-1, INV-4`: invariant ids separated by `, `, each a positive
/// integer with no leading zero (§6.4). `None` for anything else.
fn citation(inside: &str) -> Option<Vec<u64>> {
    let mut numbers: Vec<u64> = Vec::new();
    for part in inside.split(", ") {
        let number = crate::context::invariants::number_of(part)?;
        if !numbers.contains(&number) {
            numbers.push(number);
        }
    }
    Some(numbers)
}

/// The criterion's own words: a line's text without its id, so that `AC1: TBD`
/// is the placeholder it looks like (§6.4).
pub(super) fn without_id(text: &str) -> &str {
    match parse_id(text) {
        Id::Valid { rest, .. } => rest,
        _ => text,
    }
}

/// Whether a line is a list item, and its text without the marker and any checkbox.
fn read_line(visible: &str) -> (bool, &str) {
    let trimmed = visible.trim();
    let after_marker = strip_list_marker(trimmed);
    (
        after_marker.len() != trimmed.len(),
        strip_checkbox(after_marker),
    )
}

/// The criteria in `## Acceptance criteria`, in order. Empty when they have no ids
/// (D-16). Meant for a section that passed [`check_ids`].
///
/// An indented line belongs to the criterion above it, as for [`check_ids`], and is
/// joined to it with one space. It is kept as written, list marker and all, and so
/// is one that holds no words: what was approved is not this function's to shorten.
pub(super) fn parse_criteria(content: &str) -> Vec<Criterion> {
    let mut criteria: Vec<Criterion> = Vec::new();
    let mut in_comment = false;
    for (raw, _) in super::split_lines(content) {
        let visible = without_comments(raw, &mut in_comment);
        let (is_item, text) = read_line(&visible);
        match parse_id(text) {
            Id::Valid {
                number,
                invariants,
                rest,
            } if is_item => criteria.push(Criterion {
                id: format!("AC{number}"),
                text: rest.to_owned(),
                invariants,
            }),
            _ => {
                let wrapped = visible.trim();
                if let Some(last) = criteria.last_mut()
                    && is_indented(&visible)
                    && !wrapped.is_empty()
                {
                    last.text.push(' ');
                    last.text.push_str(wrapped);
                }
            }
        }
    }
    criteria
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
        let (is_item, text) = read_line(&visible);
        if !text.chars().any(char::is_alphanumeric) || is_placeholder(text) {
            continue;
        }
        // A line indented under an item belongs to it: a wrapped criterion
        // keeps one id. An id is never swallowed that way, or a criterion
        // would vanish from the list with nothing reported.
        let belongs_to_item = seen_item && is_indented(&visible);

        match parse_id(text) {
            Id::Valid { number, rest, .. } if is_item => {
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
            Id::BadCitation { id, citation } if is_item => {
                // The id is sound: it counts, so no gap is reported on top of this.
                ids.push((id, line));
                seen_item = true;
                problems.at(
                    line,
                    format!(
                        "`AC{id} {citation}` does not cite invariants in the form \
                         `AC{id} (INV-<n>): …` or `AC{id} (INV-<n>, INV-<m>): …`: ids \
                         `INV-` and a positive integer, separated by `, ` \
                         (plan-header.md §6.4)"
                    ),
                );
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
            Id::Emphasised { token } if is_item => {
                without_valid_id += 1;
                seen_item = true;
                problems.at(
                    line,
                    format!(
                        "`{token}` is an id written with emphasis; write it plain, as in \
                         `- AC1: Given …` (plan-header.md §6.4)"
                    ),
                );
            }
            Id::Malformed { .. } | Id::Emphasised { .. } | Id::BadCitation { .. } | Id::None
                if belongs_to_item => {}
            Id::Malformed { .. } | Id::Emphasised { .. } | Id::BadCitation { .. } => {
                problems.at(line, not_a_list_item());
            }
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

/// Where a Plan names and cites the invariants of `BUSINESS_CONTEXT.md`, each with
/// its line in the file (§6.3, §6.4). What the gate and the review compare with the
/// file's own ids.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InvariantRefs {
    /// `- BUSINESS_CONTEXT INV-<n>: …` in `## Implementation context`: the number
    /// and the line, in file order.
    pub named: Vec<(u64, usize)>,
    /// A criterion that cites invariants: its id, the numbers and the line.
    pub cited: Vec<(String, Vec<u64>, usize)>,
}

impl InvariantRefs {
    /// Whether the Plan names `number` in Implementation context.
    pub fn names(&self, number: u64) -> bool {
        self.named.iter().any(|(named, _)| *named == number)
    }

    /// The criteria that cite `number`, in order.
    pub fn citing(&self, number: u64) -> Vec<&str> {
        self.cited
            .iter()
            .filter(|(_, numbers, _)| numbers.contains(&number))
            .map(|(id, _, _)| id.as_str())
            .collect()
    }
}

/// The criteria of `## Acceptance criteria` that cite invariants, with their lines;
/// `content` starts at line `first_line`.
pub(super) fn citations(content: &str, first_line: usize) -> Vec<(String, Vec<u64>, usize)> {
    let mut cited = Vec::new();
    let mut in_comment = false;
    for (index, (raw, _)) in super::split_lines(content).into_iter().enumerate() {
        let visible = without_comments(raw, &mut in_comment);
        let (is_item, text) = read_line(&visible);
        if let Id::Valid {
            number, invariants, ..
        } = parse_id(text)
            && is_item
            && !invariants.is_empty()
        {
            cited.push((format!("AC{number}"), invariants, first_line + index));
        }
    }
    cited
}

/// The source an Implementation context item names an invariant by (§6.3).
const SOURCE: &str = "BUSINESS_CONTEXT ";

/// What an item of `## Implementation context` says of an invariant id.
enum Named<'a> {
    /// `- BUSINESS_CONTEXT INV-<n>: …`.
    Id(u64),
    /// `- BUSINESS_CONTEXT <token>` where the token is meant as an id and is not one.
    Malformed(&'a str),
    None,
}

fn named(visible: &str) -> Named<'_> {
    let (is_item, text) = read_line(visible);
    let Some(rest) = text.strip_prefix(SOURCE).filter(|_| is_item) else {
        return Named::None;
    };
    let token = rest.split_whitespace().next().unwrap_or("");
    if let Some(number) = token
        .strip_suffix(':')
        .and_then(crate::context::invariants::number_of)
    {
        return Named::Id(number);
    }
    if crate::context::invariants::looks_like_id(token) {
        Named::Malformed(token)
    } else {
        Named::None
    }
}

/// The invariants `## Implementation context` names by id; `content` starts at line
/// `first_line`.
pub(super) fn named_invariants(content: &str, first_line: usize) -> Vec<(u64, usize)> {
    let mut found = Vec::new();
    let mut in_comment = false;
    for (index, (raw, _)) in super::split_lines(content).into_iter().enumerate() {
        let visible = without_comments(raw, &mut in_comment);
        if let Named::Id(number) = named(&visible) {
            found.push((number, first_line + index));
        }
    }
    found
}

/// Check the form of the ids `## Implementation context` names invariants by
/// (§6.3). Whether the file defines them is `plan validate`'s question, which knows
/// the file.
pub(super) fn check_named_invariants(content: &str, first_line: usize, found: &mut Diagnostics) {
    let mut in_comment = false;
    for (index, (raw, _)) in super::split_lines(content).into_iter().enumerate() {
        let visible = without_comments(raw, &mut in_comment);
        if let Named::Malformed(token) = named(&visible) {
            found.at(
                first_line + index,
                format!(
                    "`{token}` is not an invariant id; name an invariant as \
                     `- BUSINESS_CONTEXT INV-<n>: <guidance>`, with the id \
                     `BUSINESS_CONTEXT.md` gives it (plan-header.md §6.3)"
                ),
            );
        }
    }
}

/// What `## Acceptance tests to change` allows for one path (§6.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestAction {
    Change,
    Delete,
}

impl TestAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Change => "change",
            Self::Delete => "delete",
        }
    }
}

/// One item of `## Acceptance tests to change`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestToChange {
    /// As written. What it must be is checked where `acceptance.dir` is known.
    pub path: String,
    pub action: TestAction,
}

/// The items of a section that [`check_tests_to_change`] accepted. An item it would
/// refuse is left out: an approved Plan has none.
pub(super) fn parse_tests_to_change(content: &str) -> Vec<TestToChange> {
    let mut in_comment = false;
    let mut items = Vec::new();
    for (raw, _) in super::split_lines(content) {
        let visible = without_comments(raw, &mut in_comment);
        let Some(item) = visible.trim_end().strip_prefix("- ") else {
            continue;
        };
        let (path, Some(rest)) = next_field(item) else {
            continue;
        };
        let action = match next_field(rest).0 {
            "change" => TestAction::Change,
            "delete" => TestAction::Delete,
            _ => continue,
        };
        items.push(TestToChange {
            path: path.to_owned(),
            action,
        });
    }
    items
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
