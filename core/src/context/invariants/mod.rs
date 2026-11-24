//! The invariants of `BUSINESS_CONTEXT.md`, by id (`business-context.md` §5, #158).
//!
//! Each top-level list item under `## Invariants` starts with `INV-<n>:`. The id is
//! the join key between the file, a Plan's `## Implementation context`, its criteria,
//! the gate and the review: the core never matches prose to prose, as with criterion
//! ids (plan-header.md D-18). Nothing here reads what an invariant says beyond its
//! first line, which is shown, never interpreted.
//!
//! The file is read by the rules of the rest of this module: no marker, or nothing
//! after it, is [`Invariants::None`], so a scaffolded file switches nothing on. A file
//! whose items carry no ids at all is [`Invariants::WithoutIds`], and every command
//! behaves as before it, with one warning where coverage would be shown. Some items
//! with ids and some without is a problem: coverage cannot be shown honestly for half
//! a list.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;

use crate::diagnostics::Diagnostics;
use crate::format::values::without_comments;
use crate::fs::ProjectRoot;

use super::{ContextError, MARKER};

#[cfg(test)]
mod tests;

/// The file the invariants are read from.
pub const FILE: &str = "BUSINESS_CONTEXT.md";

/// One invariant with an id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invariant {
    /// The `<n>` of `INV-<n>`.
    pub number: u64,
    /// The text of the item's first line, after the id.
    pub text: String,
    /// 1-based line of the item in the file.
    pub line: usize,
}

impl Invariant {
    pub fn id(&self) -> String {
        id(self.number)
    }
}

/// `INV-<n>`.
pub fn id(number: u64) -> String {
    format!("INV-{number}")
}

/// What `BUSINESS_CONTEXT.md` holds under `## Invariants`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Invariants {
    /// No file, no marker, nothing after the marker, or no item under the heading:
    /// nothing to show, as before ids existed.
    None,
    /// Invariants, none with an id: coverage cannot be shown.
    WithoutIds { count: usize },
    /// Every invariant has an id, each once. In file order.
    Ids(Vec<Invariant>),
    /// Duplicate or malformed ids, or some items with ids and some without; each
    /// problem at its line of the file.
    Invalid(Diagnostics),
}

impl Invariants {
    /// The invariants with ids, when the file is valid and has them.
    pub fn ids(&self) -> Option<&[Invariant]> {
        match self {
            Self::Ids(list) => Some(list),
            _ => None,
        }
    }
}

/// Read `BUSINESS_CONTEXT.md` in `root`. A file that is not there is
/// [`Invariants::None`]; one that cannot be read is an error, as for the binding
/// context: "cannot tell" is not "not there".
pub fn load(root: &ProjectRoot) -> Result<Invariants, ContextError> {
    read(&root.path().join(FILE))
}

fn read(path: &Path) -> Result<Invariants, ContextError> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(Invariants::None),
        Err(source) => {
            return Err(ContextError::Read {
                path: path.to_owned(),
                source,
            });
        }
    };
    let text = std::str::from_utf8(&bytes).map_err(|err| ContextError::NotUtf8 {
        path: path.to_owned(),
        valid_up_to: err.valid_up_to(),
    })?;
    Ok(parse(text))
}

/// One item under the heading, as read.
struct Item {
    line: usize,
    id: Id,
}

enum Id {
    Valid { number: u64, text: String },
    Malformed { token: String },
    None,
}

/// The invariants in a file's text.
pub fn parse(text: &str) -> Invariants {
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(text);
    let lines: Vec<&str> = text
        .split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .collect();
    let Some(first) = lines.iter().position(|line| !line.trim().is_empty()) else {
        return Invariants::None;
    };
    if lines[first].trim() != MARKER {
        return Invariants::None;
    }

    let mut items: Vec<Item> = Vec::new();
    let mut in_section = false;
    let mut in_comment = false;
    let mut fence = false;
    for (index, raw) in lines.iter().enumerate().skip(first + 1) {
        let line = index + 1;
        let visible = without_comments(raw, &mut in_comment);
        let trimmed = visible.trim();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            fence = !fence;
            continue;
        }
        if fence || trimmed.is_empty() {
            continue;
        }
        if let Some(level) = heading_level(trimmed) {
            if level <= 2 {
                in_section = level == 2 && is_invariants_heading(&trimmed[level..]);
            }
            continue;
        }
        if !in_section || is_indented(&visible) {
            // A wrapped line or a sub-item belongs to the item above it.
            continue;
        }
        let Some(item) = list_item(trimmed) else {
            // A sentence between the items is not an invariant.
            continue;
        };
        items.push(Item {
            line,
            id: parse_id(item),
        });
    }

    if items.is_empty() {
        return Invariants::None;
    }
    let numbered = items
        .iter()
        .filter(|item| !matches!(item.id, Id::None))
        .count();
    if numbered == 0 {
        return Invariants::WithoutIds { count: items.len() };
    }

    let mut problems = Diagnostics::new();
    let mut seen: BTreeMap<u64, usize> = BTreeMap::new();
    let next = items
        .iter()
        .filter_map(|item| match &item.id {
            Id::Valid { number, .. } => Some(*number),
            _ => None,
        })
        .max()
        .unwrap_or(0)
        + 1;
    let mut list = Vec::new();
    for item in &items {
        match &item.id {
            &Id::Valid { number, ref text } => {
                if let Some(first) = seen.get(&number) {
                    problems.at(
                        item.line,
                        format!(
                            "`INV-{number}` appears again (first on line {first}); an id names \
                             one rule. Give this invariant an unused number, such as \
                             `INV-{next}`, and never reuse one (business-context.md §5)"
                        ),
                    );
                    continue;
                }
                seen.insert(number, item.line);
                list.push(Invariant {
                    number,
                    text: text.clone(),
                    line: item.line,
                });
            }
            Id::Malformed { token } => problems.at(
                item.line,
                format!(
                    "`{token}` is not an invariant id; an id is `INV-`, a positive integer \
                     with no leading zero, then `:` and a space, as in `- INV-{next}: …` \
                     (business-context.md §5)"
                ),
            ),
            Id::None => problems.at(
                item.line,
                format!(
                    "this invariant has no id, and others under `## Invariants` have one, so \
                     which invariants a Plan leaves out cannot be shown. Start it with an \
                     unused id, such as `- INV-{next}: …` (business-context.md §5)"
                ),
            ),
        }
    }
    if problems.is_empty() {
        Invariants::Ids(list)
    } else {
        Invariants::Invalid(problems)
    }
}

/// The level of an ATX heading: one to six `#`, then a space or the end.
fn heading_level(line: &str) -> Option<usize> {
    let hashes = line.bytes().take_while(|byte| *byte == b'#').count();
    ((1..=6).contains(&hashes)
        && line[hashes..]
            .chars()
            .next()
            .is_none_or(char::is_whitespace))
    .then_some(hashes)
}

/// `Invariants`, or `Invariants` followed by more words, as `invar init` writes it:
/// `## Invariants (always true, no exceptions)`.
fn is_invariants_heading(text: &str) -> bool {
    let text = text.trim();
    text == "Invariants"
        || text
            .strip_prefix("Invariants")
            .is_some_and(|rest| rest.starts_with(char::is_whitespace))
}

/// Two spaces or a tab, as for a wrapped criterion (plan-header.md §6.4).
fn is_indented(line: &str) -> bool {
    line.starts_with('\t') || line.starts_with("  ")
}

/// The text of a list item: after `-`, `*`, `+` or `<digits>.`/`<digits>)` and a space.
fn list_item(line: &str) -> Option<&str> {
    let ends_marker = |rest: &str| rest.is_empty() || rest.starts_with(char::is_whitespace);
    if let Some(rest) = line.strip_prefix(['-', '*', '+'])
        && ends_marker(rest)
    {
        return Some(rest.trim_start());
    }
    let digits = line.bytes().take_while(u8::is_ascii_digit).count();
    if digits > 0
        && let Some(rest) = line[digits..].strip_prefix(['.', ')'])
        && ends_marker(rest)
    {
        return Some(rest.trim_start());
    }
    None
}

/// `INV-<n>: <text>`, something meant as an id that is not one, or no id.
fn parse_id(item: &str) -> Id {
    let token_end = item.find(char::is_whitespace).unwrap_or(item.len());
    let token = &item[..token_end];
    if let Some(number) = token.strip_suffix(':').and_then(number_of) {
        return Id::Valid {
            number,
            text: item[token_end..].trim().to_owned(),
        };
    }
    if looks_like_id(token) {
        Id::Malformed {
            token: token.to_owned(),
        }
    } else {
        Id::None
    }
}

/// The `<n>` of an exact `INV-<n>`: a positive integer with no leading zero.
pub fn number_of(token: &str) -> Option<u64> {
    let digits = token.strip_prefix("INV-")?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) || digits.starts_with('0') {
        return None;
    }
    digits.parse().ok()
}

/// What was meant as an id: `inv` in any case, behind any emphasis, followed by
/// anything but a letter. `INV-0:`, `inv-1:`, `INV-x:`, `INV1`, `**INV-1:**`.
/// `Invoices are …` is text.
pub fn looks_like_id(token: &str) -> bool {
    let bare = token.trim_start_matches(['*', '_', '`']);
    bare.get(..3)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("inv"))
        && bare[3..].chars().next().is_none_or(|c| !c.is_alphabetic())
}
