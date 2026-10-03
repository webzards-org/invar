//! The Plan header: `Key: value` lines between the two `---` delimiters
//! (contract §4.2, §5).
//!
//! Line-oriented on purpose, not YAML. Unknown keys and comments must survive
//! a write in their original positions (D-2), which a serde round trip cannot
//! do. Every line is therefore kept verbatim; a known key's value is the only
//! part a command ever changes, and only that part is rewritten.

use crate::diagnostics::Diagnostics;

use super::Line;

pub(super) const TICKET: &str = "Ticket";
pub(super) const MODULE: &str = "Module";
pub(super) const STATUS: &str = "Status";
pub(super) const APPROVED_BY: &str = "Approved-by";
pub(super) const APPROVED_AT: &str = "Approved-at";
pub(super) const APPROVED_CHECKSUM: &str = "Approved-checksum";
pub(super) const LOCKFILE_CHECKSUM: &str = "Lockfile-checksum";

/// Keys that must be present, even with an empty value (§5, D-3).
const REQUIRED_KEYS: [&str; 6] = [
    TICKET,
    MODULE,
    STATUS,
    APPROVED_BY,
    APPROVED_AT,
    APPROVED_CHECKSUM,
];

/// Every key the contract defines. Anything else is an unknown key:
/// permitted, preserved, and otherwise ignored (D-2).
const KNOWN_KEYS: [&str; 7] = [
    TICKET,
    MODULE,
    STATUS,
    APPROVED_BY,
    APPROVED_AT,
    APPROVED_CHECKSUM,
    LOCKFILE_CHECKSUM,
];

// --- the typed view ---------------------------------------------------------

/// `Status` (§5): exactly `draft` or `approved`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Draft,
    Approved,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Approved => "approved",
        }
    }
}

/// The header's known fields. An empty value is `None`: the key is present —
/// the parser requires that — but nothing has been written into it yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    pub ticket: String,
    pub module: String,
    pub status: Status,
    pub approved_by: Option<String>,
    /// `YYYY-MM-DD` (D-6).
    pub approved_at: Option<String>,
    /// `sha256:` + 64 lowercase hex, over the body.
    pub approved_checksum: Option<String>,
    /// Absent and empty both mean no observation (§5.2, D-7).
    pub lockfile_checksum: Option<String>,
}

// --- one header line, kept verbatim ------------------------------------------

/// A header line. `Field` keeps the exact spelling of everything around the
/// value, so writing it back reproduces the original bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum HeaderLine {
    /// `<key>:<sep><value><trailing><eol>` — `trailing` holds trailing
    /// whitespace and an inline comment, if any.
    Field {
        key: String,
        sep: String,
        value: String,
        trailing: String,
        eol: String,
    },
    /// A blank line, a comment, or a line reported as invalid. Never touched.
    Verbatim(Line),
}

impl HeaderLine {
    pub(super) fn write_to(&self, out: &mut String) {
        match self {
            Self::Field {
                key,
                sep,
                value,
                trailing,
                eol,
            } => {
                out.push_str(key);
                out.push(':');
                out.push_str(sep);
                out.push_str(value);
                out.push_str(trailing);
                out.push_str(eol);
            }
            Self::Verbatim(line) => line.write_to(out),
        }
    }

    pub(super) fn key(&self) -> Option<&str> {
        match self {
            Self::Field { key, .. } => Some(key),
            Self::Verbatim(_) => None,
        }
    }

    /// Replace the value, keeping the key, the inline comment and the line
    /// ending as they were.
    pub(super) fn set_value(&mut self, new_value: &str) {
        let Self::Field {
            sep,
            value,
            trailing,
            ..
        } = self
        else {
            unreachable!("only fields have values");
        };
        if new_value.is_empty() {
            // Back to the creation form, `Approved-by:`, unless a comment
            // keeps the separator meaningful.
            if trailing.is_empty() {
                sep.clear();
            }
        } else {
            if sep.is_empty() {
                sep.push(' ');
            }
            // A comment written straight after the old empty value
            // (`Key: # note`) must stay a comment after the new one.
            if trailing.starts_with('#') {
                trailing.insert(0, ' ');
            }
        }
        *value = new_value.to_owned();
    }
}

/// Parse one header line. Problems go into `found`; the line is kept either
/// way, so a file with problems still round-trips.
fn parse_line(line: &Line, number: usize, found: &mut Diagnostics) -> HeaderLine {
    let text = line.text.as_str();
    let verbatim = || HeaderLine::Verbatim(line.clone());

    if text.trim().is_empty() || text.trim_start().starts_with('#') {
        return verbatim();
    }
    if text.starts_with(char::is_whitespace) {
        found.at(
            number,
            "header lines must not be indented: the Plan header is a flat list of \
             `Key: value` lines, without nested values (plan-header.md §4.2)",
        );
        return verbatim();
    }
    let Some((key, rest)) = text.split_once(':') else {
        found.at(
            number,
            format!(
                "`{text}` is not a `Key: value` line; write it as `Key: value`, or as a `# comment`"
            ),
        );
        return verbatim();
    };
    if !is_key(key) {
        found.at(
            number,
            format!(
                "`{key}` is not a valid header key; keys are letters, digits and `-`, \
                 such as `Approved-by`"
            ),
        );
        return verbatim();
    }
    if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
        found.at(
            number,
            format!("put a space after the colon: `{key}: {rest}`"),
        );
        return verbatim();
    }

    let sep_len = rest.len() - rest.trim_start().len();
    let (sep, after_sep) = rest.split_at(sep_len);
    let value_part = &after_sep[..inline_comment_start(after_sep)];
    let value = value_part.trim_end();
    let trailing = &after_sep[value.len()..];

    // Block scalars (`|`, `>`) and flow collections (`[`, `{`) are YAML
    // structure, which the contract lets a parser reject (§4.2).
    if value.starts_with(['|', '>', '[', '{']) {
        found.at(
            number,
            format!(
                "`{key}` has a YAML structure as its value; the Plan header takes \
                 plain one-line values only (plan-header.md §4.2)"
            ),
        );
    }

    HeaderLine::Field {
        key: key.to_owned(),
        sep: sep.to_owned(),
        value: value.to_owned(),
        trailing: trailing.to_owned(),
        eol: line.eol.clone(),
    }
}

/// Where an inline comment starts: a `#` at the start, or one preceded by
/// whitespace — as in YAML. `sha256:ab#` is a value; `x # note` is a comment.
fn inline_comment_start(text: &str) -> usize {
    let mut previous_is_space = true;
    for (index, ch) in text.char_indices() {
        if ch == '#' && previous_is_space {
            return index;
        }
        previous_is_space = ch.is_whitespace();
    }
    text.len()
}

fn is_key(key: &str) -> bool {
    let mut chars = key.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphanumeric())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

// --- the whole header ---------------------------------------------------------

/// Parse the lines between the delimiters. `first_line` is the 1-based number
/// of the first of them. Returns the lines (always) and the typed header (only
/// when every rule of §5 holds).
pub(super) fn parse(
    lines: &[Line],
    first_line: usize,
    found: &mut Diagnostics,
) -> (Vec<HeaderLine>, Option<Header>) {
    let before = found.len();
    let parsed: Vec<HeaderLine> = lines
        .iter()
        .enumerate()
        .map(|(index, line)| parse_line(line, first_line + index, found))
        .collect();

    // Values of the known keys, by key, with their line numbers.
    let mut values: Vec<(&str, &str, usize)> = Vec::new();
    for (index, line) in parsed.iter().enumerate() {
        let HeaderLine::Field { key, value, .. } = line else {
            continue;
        };
        let number = first_line + index;
        if let Some((_, _, first)) = values.iter().find(|(seen, _, _)| *seen == key.as_str()) {
            found.at(
                number,
                format!("`{key}` appears twice (first on line {first}); keep one of them"),
            );
            continue;
        }
        values.push((key, value, number));
    }
    let value_of = |key: &str| {
        values
            .iter()
            .find(|(k, _, _)| *k == key)
            .map(|(_, v, l)| (*v, *l))
    };

    for required in REQUIRED_KEYS {
        if value_of(required).is_none() {
            let mut message = format!("the header has no `{required}` key; add it");
            if !matches!(required, TICKET | MODULE | STATUS) {
                message.push_str(&format!(
                    " with an empty value (`{required}:`) — `invar plan approve` fills it in"
                ));
            }
            // A case slip is the likeliest cause: point at it.
            if let Some((other, _, line)) = values
                .iter()
                .find(|(k, _, _)| k.eq_ignore_ascii_case(required) && !KNOWN_KEYS.contains(k))
            {
                message.push_str(&format!(
                    ". Line {line} has `{other}`: keys are case-sensitive"
                ));
            }
            found.general(message);
        }
    }

    let ticket = value_of(TICKET);
    if let Some((value, line)) = ticket
        && !is_ticket(value)
    {
        found.at(
            line,
            format!("`Ticket` must look like `PROJ-412` (capital letters, a dash, digits); found `{value}`"),
        );
    }
    let module = value_of(MODULE);
    if let Some((value, line)) = module
        && !is_module(value)
    {
        found.at(
            line,
            format!(
                "`Module` must be a lowercase module name such as `orders` (letters, \
                 digits, `_`), or `global`; found `{value}`"
            ),
        );
    }
    let status = value_of(STATUS).and_then(|(value, line)| match value {
        "draft" => Some(Status::Draft),
        "approved" => Some(Status::Approved),
        other => {
            found.at(
                line,
                format!("`Status` must be `draft` or `approved`, exactly; found `{other}`"),
            );
            None
        }
    });
    if let Some((value, line)) = value_of(APPROVED_AT)
        && !value.is_empty()
        && !is_date(value)
    {
        found.at(
            line,
            format!("`Approved-at` must be a date written `YYYY-MM-DD`; found `{value}`"),
        );
    }
    for key in [APPROVED_CHECKSUM, LOCKFILE_CHECKSUM] {
        if let Some((value, line)) = value_of(key)
            && !value.is_empty()
            && !is_digest(value)
        {
            found.at(
                line,
                format!(
                    "`{key}` must be `sha256:` followed by 64 lowercase hex digits; found `{value}`"
                ),
            );
        }
    }

    let header = if found.len() == before {
        let filled = |key: &str| {
            value_of(key)
                .map(|(value, _)| value)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
        };
        Some(Header {
            ticket: ticket.map(|(v, _)| v.to_owned()).unwrap_or_default(),
            module: module.map(|(v, _)| v.to_owned()).unwrap_or_default(),
            status: status.unwrap_or(Status::Draft),
            approved_by: filled(APPROVED_BY),
            approved_at: filled(APPROVED_AT),
            approved_checksum: filled(APPROVED_CHECKSUM),
            lockfile_checksum: filled(LOCKFILE_CHECKSUM),
        })
    } else {
        None
    };
    (parsed, header)
}

// --- value formats (§5) --------------------------------------------------------

/// `^[A-Z][A-Z0-9]*-[0-9]+$`
pub(super) fn is_ticket(value: &str) -> bool {
    let Some((prefix, number)) = value.split_once('-') else {
        return false;
    };
    let mut prefix_chars = prefix.chars();
    prefix_chars.next().is_some_and(|c| c.is_ascii_uppercase())
        && prefix_chars.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        && !number.is_empty()
        && number.chars().all(|c| c.is_ascii_digit())
}

/// `^[a-z][a-z0-9_]*$` — which `global` satisfies too.
fn is_module(value: &str) -> bool {
    let mut chars = value.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// `YYYY-MM-DD`, and a day that exists.
pub(super) fn is_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return false;
    }
    let number = |range: std::ops::Range<usize>| -> Option<u32> {
        let part = &value[range];
        part.bytes()
            .all(|b| b.is_ascii_digit())
            .then(|| part.parse().ok())
            .flatten()
    };
    let (Some(year), Some(month), Some(day)) = (number(0..4), number(5..7), number(8..10)) else {
        return false;
    };
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    (1..=days).contains(&day)
}

/// `sha256:` + 64 lowercase hex.
pub(super) fn is_digest(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|hex| {
        hex.len() == 64
            && hex
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}
