//! A decision entry: `.invar/decisions/<YYYY-MM-DD>-<HHMMSS>-<module>-<slug>.md`
//! (§7).
//!
//! Implements `contracts/formats/decision-entry.md` v1.4.0, including the
//! draft of §8.1; section numbers and decision ids below point into it.
//!
//! **Writing** produces one canonical text: LF line endings, the five keys in
//! a fixed order, the body after one blank line, exactly one `\n` at the end.
//! The core only ever writes new files (§6), so there is no round trip to
//! preserve, unlike the Plan.
//!
//! **Reading** is tolerant in what it skips and strict in what it checks.
//! Entries may be edited by hand, so comments, blank lines and unknown keys
//! are passed over; a required key that is missing or malformed is a problem,
//! reported with every other one in a single run.

use std::fmt;

use crate::calendar::days_from_civil;
use crate::diagnostics::Diagnostics;
use crate::format::plan::Acceptance;
use crate::format::values::{is_module, is_slug, is_ticket};

#[cfg(test)]
mod tests;

/// The four types of §5.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Type {
    /// The cycle completed: what was built. Drafted by the agent
    /// (`/invar:log`), recorded by the developer (`invar decision approve`).
    /// Nothing writes it mechanically (§8).
    Confirmed,
    /// The developer corrected something by hand.
    Correction,
    /// The cycle could not be completed; resets the Plan (§7).
    Failed,
    /// Anything else the developer wants the planner to know.
    Note,
}

impl Type {
    pub const ALL: [Type; 4] = [Self::Confirmed, Self::Correction, Self::Failed, Self::Note];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Confirmed => "confirmed",
            Self::Correction => "correction",
            Self::Failed => "failed",
            Self::Note => "note",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == value)
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One entry: its front matter and its body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// `d-7c41e09a` (D-4).
    pub id: String,
    /// `2026-10-06T18:37:12+02:00`: local time with its offset (D-2).
    pub date: String,
    pub ticket: String,
    pub module: String,
    pub kind: Type,
    /// The `.feature` scenario; absent in light mode.
    pub scenario: Option<String>,
    /// What the Plan decided about acceptance tests (§4.2). `None` is
    /// unknown, never `skipped` (D-14).
    pub acceptance: Option<Acceptance>,
    /// Free text, without trailing newline.
    pub body: String,
}

impl Entry {
    /// The file's text, exactly as the core writes it (§3, §4).
    pub fn render(&self) -> String {
        let mut out = String::with_capacity(self.body.len() + 160);
        out.push_str("---\n");
        out.push_str(&format!("id: {}\n", self.id));
        out.push_str(&format!("date: {}\n", self.date));
        out.push_str(&format!("ticket: {}\n", self.ticket));
        out.push_str(&format!("module: {}\n", self.module));
        out.push_str(&format!("type: {}\n", self.kind));
        if let Some(scenario) = &self.scenario {
            out.push_str(&format!("scenario: {scenario}\n"));
        }
        if let Some(acceptance) = self.acceptance {
            out.push_str(&format!("acceptance: {}\n", acceptance.as_str()));
        }
        out.push_str("---\n\n");
        out.push_str(&self.body);
        out.push('\n');
        out
    }
}

// --- the filename (§2) ----------------------------------------------------------

/// `<YYYY-MM-DD>-<HHMMSS>-<module>-<slug>.md`, taken apart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileName {
    /// `2026-10-06-163712`, UTC.
    pub stamp: String,
    pub module: String,
    pub slug: String,
}

impl FileName {
    /// `None` unless `name` has the shape of §2.
    pub fn parse(name: &str) -> Option<Self> {
        let stem = name.strip_suffix(".md")?;
        // `YYYY-MM-DD-HHMMSS-` is 18 bytes.
        let (stamp, rest) = (stem.get(..17)?, stem.get(17..)?);
        let rest = rest.strip_prefix('-')?;
        if !is_stamp(stamp) {
            return None;
        }
        // The module has no `-`, so the first one ends it.
        let (module, slug) = rest.split_once('-')?;
        (is_module(module) && is_slug(slug)).then(|| Self {
            stamp: stamp.to_owned(),
            module: module.to_owned(),
            slug: slug.to_owned(),
        })
    }

    /// The name, with `-<suffix>` appended to the slug when `suffix > 1`
    /// (D-3).
    pub fn render(&self, suffix: u32) -> String {
        if suffix > 1 {
            format!("{}-{}-{}-{suffix}.md", self.stamp, self.module, self.slug)
        } else {
            format!("{}-{}-{}.md", self.stamp, self.module, self.slug)
        }
    }
}

/// `YYYY-MM-DD-HHMMSS`, and a moment that exists.
fn is_stamp(stamp: &str) -> bool {
    let b = stamp.as_bytes();
    if b.len() != 17 || b[4] != b'-' || b[7] != b'-' || b[10] != b'-' {
        return false;
    }
    let number = |range: std::ops::Range<usize>| -> Option<u32> {
        // `get`, not indexing: a hand-made name may hold any character.
        let part = stamp.get(range)?;
        part.bytes()
            .all(|b| b.is_ascii_digit())
            .then(|| part.parse().ok())
            .flatten()
    };
    match (
        number(0..4),
        number(5..7),
        number(8..10),
        number(11..13),
        number(13..15),
        number(15..17),
    ) {
        (Some(y), Some(mo), Some(d), Some(h), Some(mi), Some(s)) => {
            days_from_civil(i64::from(y), mo, d).is_some() && h < 24 && mi < 60 && s < 60
        }
        _ => false,
    }
}

// --- values (§4.1) ---------------------------------------------------------------

/// `d-` and 4 to 32 lowercase hex digits (D-4).
pub fn is_id(value: &str) -> bool {
    value.strip_prefix("d-").is_some_and(|hex| {
        (4..=32).contains(&hex.len())
            && hex
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

/// `YYYY-MM-DDTHH:MM:SS` + `Z` or `±HH:MM` → the UTC stamp of the filename,
/// `YYYY-MM-DD-HHMMSS`. `None` for anything else, or a moment that does not
/// exist.
pub fn utc_stamp_of(date: &str) -> Option<String> {
    let b = date.as_bytes();
    if b.len() < 20
        || b[4] != b'-'
        || b[7] != b'-'
        || b[10] != b'T'
        || b[13] != b':'
        || b[16] != b':'
    {
        return None;
    }
    let number = |range: std::ops::Range<usize>| -> Option<i64> {
        let part = date.get(range)?;
        part.bytes()
            .all(|b| b.is_ascii_digit())
            .then(|| part.parse().ok())
            .flatten()
    };
    let (year, month, day) = (number(0..4)?, number(5..7)?, number(8..10)?);
    let (hour, minute, second) = (number(11..13)?, number(14..16)?, number(17..19)?);
    if hour >= 24 || minute >= 60 || second >= 60 {
        return None;
    }
    let offset_minutes = match date.get(19..)? {
        "Z" => 0,
        zone if zone.len() == 6 && zone.as_bytes()[3] == b':' => {
            let sign = match zone.as_bytes()[0] {
                b'+' => 1,
                b'-' => -1,
                _ => return None,
            };
            let (h, m) = (number(20..22)?, number(23..25)?);
            if h >= 24 || m >= 60 {
                return None;
            }
            sign * (h * 60 + m)
        }
        _ => return None,
    };
    let days = days_from_civil(year, u32::try_from(month).ok()?, u32::try_from(day).ok()?)?;
    let utc = days * 86_400 + hour * 3600 + minute * 60 + second - offset_minutes * 60;
    let (y, mo, d) = crate::calendar::civil_from_days(utc.div_euclid(86_400));
    let of_day = utc.rem_euclid(86_400);
    (0..=9999).contains(&y).then(|| {
        format!(
            "{y:04}-{mo:02}-{d:02}-{:02}{:02}{:02}",
            of_day / 3600,
            of_day / 60 % 60,
            of_day % 60
        )
    })
}

// --- reading ----------------------------------------------------------------------

/// The keys of §4.1, in the order the core writes them.
const ID: &str = "id";
const DATE: &str = "date";
const TICKET: &str = "ticket";
const MODULE: &str = "module";
const TYPE: &str = "type";
const SCENARIO: &str = "scenario";
const ACCEPTANCE: &str = "acceptance";

/// The front matter's `key: value` pairs, each with its line number, and the
/// body.
struct Raw {
    values: Vec<(String, String, usize)>,
    body: String,
}

impl Raw {
    fn get(&self, key: &str) -> Option<&(String, String, usize)> {
        self.values.iter().find(|(k, _, _)| k == key)
    }

    /// A required key's value, when present and `valid`; a problem otherwise.
    fn required(
        &self,
        found: &mut Diagnostics,
        key: &str,
        valid: fn(&str) -> bool,
        expected: &str,
    ) -> Option<String> {
        match self.get(key) {
            None => {
                found.general(format!(
                    "the front matter has no `{key}`; add `{key}: {expected}`"
                ));
                None
            }
            Some((_, value, line)) if !valid(value) => {
                found.at(
                    *line,
                    format!("`{key}` must be {expected}; found `{value}`"),
                );
                None
            }
            Some((_, value, _)) => Some(value.clone()),
        }
    }

    /// `scenario`, which is optional.
    fn scenario(&self, found: &mut Diagnostics) -> Option<String> {
        match self.get(SCENARIO) {
            Some((_, value, line)) if !is_slug(value) => {
                found.at(
                    *line,
                    format!("`scenario` must be a slug such as `reserve-stock`; found `{value}`"),
                );
                None
            }
            Some((_, value, _)) => Some(value.clone()),
            None => None,
        }
    }

    /// `acceptance`, which is optional (§4.2): absent is unknown.
    fn acceptance(&self, found: &mut Diagnostics) -> Option<Acceptance> {
        let (_, value, line) = self.get(ACCEPTANCE)?;
        let parsed = Acceptance::parse(value);
        if parsed.is_none() {
            found.at(
                *line,
                format!(
                    "`acceptance` must be `required` or `skipped`, exactly; found `{value}`. \
                     Copy it from the `acceptance:` line of `invar plan check`, or remove the \
                     line: absent means unknown"
                ),
            );
        }
        parsed
    }
}

const TICKET_EXPECTED: &str = "a ticket id such as `PROJ-412`";
const MODULE_EXPECTED: &str =
    "a module name such as `Orders` or `orders` (letters, digits, `_`), or `global`";

/// The front matter and the body, checking the structure of §4 that entries
/// and drafts share: the two `---` lines, `key: value` lines, each key once, a
/// body that is not empty. `None` when the front matter cannot be found at
/// all; every other problem is added to `found`.
fn read(text: &str, found: &mut Diagnostics) -> Option<Raw> {
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(text);
    let lines = split_lines(text);

    if lines.first() != Some(&"---") {
        found.at(
            1,
            "the first line must be exactly `---`; a decision entry starts with its \
             front matter, between two `---` lines",
        );
        return None;
    }
    let Some(closing) = lines.iter().skip(1).position(|line| *line == "---") else {
        found.at(
            1,
            "the front matter opened here is never closed; add a line that is exactly \
             `---` after the last key",
        );
        return None;
    };
    let closing = closing + 1;

    let mut values: Vec<(String, String, usize)> = Vec::new();
    for (index, raw) in lines[1..closing].iter().enumerate() {
        let number = index + 2;
        let line = raw.trim_end();
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        let Some((key, rest)) = line.split_once(':') else {
            found.at(
                number,
                format!("`{line}` is not a `key: value` line; write it as `key: value`"),
            );
            continue;
        };
        if key.is_empty()
            || !key
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            found.at(
                number,
                format!("`{key}` is not a valid key; keys are letters, digits, `-` and `_`"),
            );
            continue;
        }
        if !rest.is_empty() && !rest.starts_with(' ') {
            found.at(
                number,
                format!("put a space after the colon: `{key}: {rest}`"),
            );
            continue;
        }
        // An inline comment starts at ` #`, as in the Plan header.
        let value = rest.split(" #").next().unwrap_or("").trim();
        if let Some((_, _, first)) = values.iter().find(|(seen, _, _)| seen == key) {
            found.at(
                number,
                format!("`{key}` appears twice (first on line {first}); keep one of them"),
            );
            continue;
        }
        values.push((key.to_owned(), value.to_owned(), number));
    }

    // The body: everything after the closing line, leading blank lines and
    // trailing whitespace dropped.
    let body: Vec<&str> = lines[closing + 1..]
        .iter()
        .copied()
        .skip_while(|line| line.trim().is_empty())
        .collect();
    let body = body.join("\n").trim_end().to_owned();
    if body.is_empty() {
        found.general(
            "the body is empty; an entry records something — write what was decided and why \
             below the closing `---`",
        );
    }
    Some(Raw { values, body })
}

/// Parse an entry's text, checking every rule of §4 that does not involve
/// the filename.
pub fn parse(text: &str) -> Result<Entry, Diagnostics> {
    let mut found = Diagnostics::new();
    let Some(raw) = read(text, &mut found) else {
        return Err(found);
    };
    let id = raw.required(
        &mut found,
        ID,
        is_id,
        "`d-` followed by 4 to 32 lowercase hex digits, such as `d-7c41e09a`",
    );
    let date = raw.required(
        &mut found,
        DATE,
        |value| utc_stamp_of(value).is_some(),
        "a date and time with its offset, such as `2026-10-06T18:37:12+02:00`",
    );
    let ticket = raw.required(&mut found, TICKET, is_ticket, TICKET_EXPECTED);
    let module = raw.required(&mut found, MODULE, is_module, MODULE_EXPECTED);
    let kind = raw.required(
        &mut found,
        TYPE,
        |value| Type::parse(value).is_some(),
        "one of `confirmed`, `correction`, `failed`, `note`",
    );
    let scenario = raw.scenario(&mut found);
    let acceptance = raw.acceptance(&mut found);

    match (id, date, ticket, module, kind) {
        (Some(id), Some(date), Some(ticket), Some(module), Some(kind)) if found.is_empty() => {
            Ok(Entry {
                id,
                date,
                ticket,
                module,
                kind: Type::parse(&kind).expect("checked above"),
                scenario,
                acceptance,
                body: raw.body,
            })
        }
        _ => Err(found),
    }
}

// --- the draft (§8.1) -------------------------------------------------------------------

/// The file a draft is written to, inside the ticket's Plan directory.
pub const DRAFT_FILE: &str = "decision.md";

/// The types a draft may have (D-11): not `correction`, which records what the
/// developer changed by hand and the agent did not see.
pub const DRAFT_TYPES: [Type; 3] = [Type::Confirmed, Type::Failed, Type::Note];

/// A draft entry: an entry without its `id` and `date`, which are assigned
/// when it is recorded (D-10).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Draft {
    pub ticket: String,
    pub module: String,
    pub kind: Type,
    /// The `.feature` scenario; absent in light mode.
    pub scenario: Option<String>,
    /// As the agent copied it from `plan check`; checked against the Plan when
    /// the draft is recorded (§8.1, D-15).
    pub acceptance: Option<Acceptance>,
    /// Free text, without trailing newline.
    pub body: String,
}

/// Parse a draft's text, checking every rule of §8.1 that does not involve
/// the Plan directory it is in.
pub fn parse_draft(text: &str) -> Result<Draft, Diagnostics> {
    let mut found = Diagnostics::new();
    let Some(raw) = read(text, &mut found) else {
        return Err(found);
    };
    for key in [ID, DATE] {
        if let Some((_, _, line)) = raw.get(key) {
            found.at(
                *line,
                format!(
                    "a draft has no `{key}`: it is assigned when the draft is recorded \
                     (decision-entry.md D-10); remove this line"
                ),
            );
        }
    }
    let ticket = raw.required(&mut found, TICKET, is_ticket, TICKET_EXPECTED);
    let module = raw.required(&mut found, MODULE, is_module, MODULE_EXPECTED);
    let kind = match raw.get(TYPE) {
        Some((_, value, line)) if value == Type::Correction.as_str() => {
            found.at(
                *line,
                "a draft is not a `correction`: that records what the developer changed by \
                 hand, and the developer records it with `invar decision record` \
                 (decision-entry.md D-11). Use `confirmed`, `failed` or `note`",
            );
            None
        }
        _ => raw.required(
            &mut found,
            TYPE,
            |value| DRAFT_TYPES.iter().any(|kind| kind.as_str() == value),
            "one of `confirmed`, `failed`, `note`",
        ),
    };
    let scenario = raw.scenario(&mut found);
    let acceptance = raw.acceptance(&mut found);

    match (ticket, module, kind) {
        (Some(ticket), Some(module), Some(kind)) if found.is_empty() => Ok(Draft {
            ticket,
            module,
            kind: Type::parse(&kind).expect("checked above"),
            scenario,
            acceptance,
            body: raw.body,
        }),
        _ => Err(found),
    }
}

/// Parse an entry and check it against its filename (§2, §4.1
/// "Consistency").
pub fn check(file_name: &str, text: &str) -> Result<Entry, Diagnostics> {
    let mut found = Diagnostics::new();
    let name = FileName::parse(file_name);
    if name.is_none() {
        found.general(format!(
            "`{file_name}` is not `<YYYY-MM-DD>-<HHMMSS>-<module>-<slug>.md`; rename it, \
             such as `2026-10-06-163712-orders-reserve-stock.md`"
        ));
    }
    let entry = match parse(text) {
        Ok(entry) => Some(entry),
        Err(problems) => {
            found.extend(problems);
            None
        }
    };
    if let (Some(name), Some(entry)) = (&name, &entry) {
        if name.module != entry.module {
            found.general(format!(
                "the filename says module `{}`, the front matter says `{}`; they must agree",
                name.module, entry.module
            ));
        }
        if utc_stamp_of(&entry.date).as_deref() != Some(name.stamp.as_str()) {
            found.general(format!(
                "the filename's time {} (UTC) is not `date: {}`; they must be the same moment",
                name.stamp, entry.date
            ));
        }
    }
    match entry {
        Some(entry) if found.is_empty() => Ok(entry),
        _ => Err(found),
    }
}

/// Lines, accepting `\n`, `\r\n` and `\r` (§3).
fn split_lines(text: &str) -> Vec<&str> {
    let mut lines = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        match rest.find(['\n', '\r']) {
            Some(end) => {
                lines.push(&rest[..end]);
                let width = if rest[end..].starts_with("\r\n") {
                    2
                } else {
                    1
                };
                rest = &rest[end + width..];
            }
            None => {
                lines.push(rest);
                rest = "";
            }
        }
    }
    lines
}

/// A body as the core writes it: LF line endings, no leading blank lines, no
/// trailing whitespace at the end. Empty when there is nothing to record.
pub fn normalize_body(raw: &str) -> String {
    let lines = split_lines(raw.strip_prefix('\u{FEFF}').unwrap_or(raw));
    let lines: Vec<&str> = lines
        .into_iter()
        .skip_while(|line| line.trim().is_empty())
        .collect();
    lines.join("\n").trim_end().to_owned()
}
