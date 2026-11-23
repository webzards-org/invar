//! The `invar-lock:` line of an acceptance test file.
//!
//! Implements `contracts/formats/acceptance-lock.md` v1.5.0. The contract is the
//! authority; section numbers below (`§2`, `D-3`) point into it.
//!
//! Reading the line and choosing what the digest covers are format knowledge, so
//! they live here. Canonicalizing and hashing stay in [`crate::digest`], which
//! knows nothing about this line. The core never learns a comment syntax (D-2):
//! everything before the marker is ignored.

use std::fmt;

use crate::digest::{self, InvalidUtf8};
use crate::format::values::is_ticket;

#[cfg(test)]
mod tests;

/// What makes a line the lock line (§2).
pub const MARKER: &str = "invar-lock:";

/// The lock line must be within this many lines of the top (§3, D-3).
pub const WINDOW: usize = 10;

/// What a recorded digest starts with (§2).
const DIGEST_PREFIX: &str = "sha256:";

/// What the list of criteria starts with (§2, D-38).
pub const CRITERIA_PREFIX: &str = "ac=";

/// The lock line of a file, and the digest of what it covers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockLine {
    pub ticket: String,
    /// 1-based.
    pub line: usize,
    /// The digest on the line. `None` means the line is pending.
    pub recorded: Option<String>,
    /// The criteria the line records the file as covering, sorted, from `ac=`. `None`
    /// when the line has no list: pending, or locked before lists were recorded (D-38).
    pub criteria: Option<Vec<String>>,
    /// The file's digest as it is now, over the list too when there is one (§4).
    pub digest: String,
}

impl LockLine {
    /// Whether the line records a digest, and it is the file's.
    pub fn matches(&self) -> bool {
        self.recorded.as_deref() == Some(&self.digest)
    }
}

/// Read the lock line of a file. `None` means the file has no lock (§3).
pub fn read(bytes: &[u8]) -> Result<Option<LockLine>, LockError> {
    let canonical = digest::canonicalize_bytes(bytes).map_err(LockError::InvalidUtf8)?;
    parse(&canonical)
}

/// `bytes` with the digest of [`read`] written into the pending line of `ticket`,
/// right after the ticket (§4), and no list of criteria: a line as locked before 1.5.0,
/// which the tests still need to write.
#[cfg(test)]
pub fn with_digest(bytes: &[u8], ticket: &str) -> Result<Vec<u8>, LockError> {
    with_digest_covering(bytes, ticket, &[])
}

/// `bytes` with a digest and `ac=<criteria>` written into the pending line of `ticket`,
/// right after the ticket, the digest covering the list (§4). An empty `criteria`
/// writes no list. The ids are sorted by number and deduplicated here.
///
/// Everything before the marker, any trailing text and the line's own ending stay,
/// and no other byte changes: this works on the file as it is, not on its canonical
/// form, so its line endings, BOM and trailing whitespace survive. Refuses a file
/// with no line, a line for another ticket, a line already locked, and an id that is
/// not `AC<n>`.
pub fn with_digest_covering(
    bytes: &[u8],
    ticket: &str,
    criteria: &[String],
) -> Result<Vec<u8>, LockError> {
    let lock = read(bytes)?.ok_or(LockError::NoLine)?;
    if lock.ticket != ticket {
        return Err(LockError::OtherTicket {
            line: lock.line,
            found: lock.ticket,
            wanted: ticket.to_owned(),
        });
    }
    if lock.recorded.is_some() {
        return Err(LockError::AlreadyLocked {
            line: lock.line,
            ticket: lock.ticket,
        });
    }

    // Valid UTF-8, and one line holds the marker, or `read` would have refused; so
    // the first marker in the raw text is the one it read, whatever the line endings.
    let text = String::from_utf8_lossy(bytes);
    let marker = text.find(MARKER).unwrap_or_default() + MARKER.len();
    let spaces = text[marker..].len() - text[marker..].trim_start_matches(' ').len();
    let at = marker + spaces + lock.ticket.len();

    let fields = if criteria.is_empty() {
        format!(" {}", lock.digest)
    } else {
        let (digest, list) = digest_covering(bytes, criteria)?;
        format!(" {digest} {CRITERIA_PREFIX}{}", list.join(","))
    };

    let mut written = Vec::with_capacity(bytes.len() + fields.len());
    written.extend_from_slice(&bytes[..at]);
    written.extend_from_slice(fields.as_bytes());
    written.extend_from_slice(&bytes[at..]);
    Ok(written)
}

/// The digest the lock line of `bytes` would record with the list `criteria`, and the
/// list as it would be written: sorted by number, without repeats (§4). Whatever the
/// line records now is left out. Refuses an id that is not `AC<n>`, and an empty list.
pub fn digest_covering(
    bytes: &[u8],
    criteria: &[String],
) -> Result<(String, Vec<String>), LockError> {
    let canonical = digest::canonicalize_bytes(bytes).map_err(LockError::InvalidUtf8)?;
    let lock = parse(&canonical)?.ok_or(LockError::NoLine)?;
    let list = sorted(criteria)
        .filter(|list| !list.is_empty())
        .ok_or_else(|| LockError::Criteria {
            line: lock.line,
            found: criteria.join(","),
            why: "an id is `AC` and a positive number with no leading zero",
        })?;
    let digest = digest::digest(&covered(&canonical, lock.line, Some(&list)));
    Ok((digest, list))
}

/// `bytes` with the digest taken out of the locked line of `ticket`, and the spaces
/// before it, and the list of criteria with its spaces when there is one: the inverse
/// of [`with_digest_covering`] (§11.2).
///
/// No other byte changes, for the same reason. Refuses a file with no line, a line for
/// another ticket, and a line that is pending.
pub fn without_digest(bytes: &[u8], ticket: &str) -> Result<Vec<u8>, LockError> {
    let lock = read(bytes)?.ok_or(LockError::NoLine)?;
    if lock.ticket != ticket {
        return Err(LockError::OtherTicket {
            line: lock.line,
            found: lock.ticket,
            wanted: ticket.to_owned(),
        });
    }
    let Some(recorded) = &lock.recorded else {
        return Err(LockError::NotLocked {
            line: lock.line,
            ticket: lock.ticket,
        });
    };

    // As in `with_digest`: the first marker in the raw text is the one `read` read, and
    // §2 puts only spaces between the ticket and the digest.
    let text = String::from_utf8_lossy(bytes);
    let marker = text.find(MARKER).unwrap_or_default() + MARKER.len();
    let spaces = text[marker..].len() - text[marker..].trim_start_matches(' ').len();
    let from = marker + spaces + lock.ticket.len();
    let spaces = text[from..].len() - text[from..].trim_start_matches(' ').len();
    let mut to = from + spaces + recorded.len();
    if lock.criteria.is_some() {
        // §2 puts only spaces between the digest and the list, too.
        let spaces = text[to..].len() - text[to..].trim_start_matches(' ').len();
        to += spaces + first_field(&text[to + spaces..]).len();
    }

    let mut written = Vec::with_capacity(bytes.len() - (to - from));
    written.extend_from_slice(&bytes[..from]);
    written.extend_from_slice(&bytes[to..]);
    Ok(written)
}

/// Whether any line of `text` is a lock line with a digest, with a malformed `sha256:`
/// field after its ticket (§2, D-5), or with an `ac=` field anywhere after it, well
/// formed or not (D-39): each is something only `invar acceptance lock` writes.
///
/// For a piece of text that is not a whole file, such as what an edit puts in: the window
/// and the one-line rule of §3 say nothing about a fragment, so [`read`] cannot answer.
/// A line whose ticket is malformed has no digest to speak of, and is §3.1's to report.
pub fn holds_digest(text: &str) -> bool {
    digest::canonicalize(text).split('\n').any(|line| {
        line.split_once(MARKER).is_some_and(|(_, after_marker)| {
            matches!(
                fields(after_marker, 0),
                Ok(Fields {
                    recorded: Some(_),
                    ..
                }) | Err(LockError::Digest { .. } | LockError::Criteria { .. })
            )
        })
    })
}

/// Canonical text keeps every line number of the file above its trailing blank
/// lines, and a lock line is never blank, so the numbers here are the file's (§3).
fn parse(canonical: &str) -> Result<Option<LockLine>, LockError> {
    let marked: Vec<usize> = canonical
        .split('\n')
        .enumerate()
        .filter(|(_, line)| line.contains(MARKER))
        .map(|(index, _)| index + 1)
        .collect();
    let line = match marked[..] {
        [] => return Ok(None),
        [line] => line,
        _ => return Err(LockError::MoreThanOne { lines: marked }),
    };
    if line > WINDOW {
        return Err(LockError::AfterWindow { line });
    }

    let text = canonical.split('\n').nth(line - 1).unwrap_or_default();
    let after_marker = text.split_once(MARKER).map_or("", |(_, rest)| rest);
    let fields = fields(after_marker, line)?;
    Ok(Some(LockLine {
        ticket: fields.ticket.to_owned(),
        line,
        recorded: fields.recorded.map(str::to_owned),
        digest: digest::digest(&covered(canonical, line, fields.criteria.as_deref())),
        criteria: fields.criteria,
    }))
}

/// What follows the marker (§2).
struct Fields<'a> {
    ticket: &'a str,
    recorded: Option<&'a str>,
    criteria: Option<Vec<String>>,
}

/// The ticket, the recorded digest and the list of criteria, from what follows the
/// marker (§2).
fn fields(after_marker: &str, line: usize) -> Result<Fields<'_>, LockError> {
    let after_spaces = after_marker.trim_start_matches(' ');
    let ticket = first_field(after_spaces);
    if after_spaces.len() == after_marker.len() || !is_ticket(ticket) {
        return Err(LockError::Ticket {
            line,
            found: after_marker.trim().to_owned(),
        });
    }

    let after_ticket = &after_spaces[ticket.len()..];
    let next = after_ticket.trim_start_matches(' ');
    // Empty unless only spaces came before it: a tab leaves `next` on whitespace.
    let candidate = first_field(next);
    let recorded = is_digest(candidate).then_some(candidate);
    let mut trailing = match recorded {
        Some(digest) => &next[digest.len()..],
        None => after_ticket,
    };

    // The list comes right after the digest, separated by spaces only (D-38).
    let mut criteria = None;
    if recorded.is_some() {
        let next = trailing.trim_start_matches(' ');
        let candidate = first_field(next);
        if next.len() < trailing.len() && candidate.starts_with(CRITERIA_PREFIX) {
            let list = criteria_list(candidate).ok_or_else(|| LockError::Criteria {
                line,
                found: candidate.to_owned(),
                why: "the list is ids such as `AC1`, sorted by number, with no repeat, separated \
                      by commas and no spaces",
            })?;
            criteria = Some(list);
            trailing = &next[candidate.len()..];
        }
    }

    let field_starting = |prefix: &str| {
        trailing
            .split_whitespace()
            .find(|field| field.starts_with(prefix))
            .map(str::to_owned)
    };
    if let Some(found) = field_starting(DIGEST_PREFIX) {
        return Err(LockError::Digest { line, found });
    }
    if let Some(found) = field_starting(CRITERIA_PREFIX) {
        return Err(LockError::Criteria {
            line,
            found,
            why: if recorded.is_none() {
                "a pending line has no list: `invar acceptance lock` writes it, with the digest"
            } else {
                "the list comes once, right after the digest, separated from it by spaces"
            },
        });
    }
    Ok(Fields {
        ticket,
        recorded,
        criteria,
    })
}

/// The ids of an `ac=` field, if it is well formed (§2): `AC<n>` with no leading zero,
/// strictly increasing by number, comma-separated, at least one.
fn criteria_list(field: &str) -> Option<Vec<String>> {
    let ids: Vec<String> = field
        .strip_prefix(CRITERIA_PREFIX)?
        .split(',')
        .map(str::to_owned)
        .collect();
    let numbers: Vec<u64> = ids.iter().map(|id| id_number(id)).collect::<Option<_>>()?;
    numbers
        .windows(2)
        .all(|pair| pair[0] < pair[1])
        .then_some(ids)
}

/// `ids` sorted by number with repeats removed, or `None` if one is not `AC<n>`.
fn sorted(ids: &[String]) -> Option<Vec<String>> {
    let mut numbers: Vec<u64> = ids.iter().map(|id| id_number(id)).collect::<Option<_>>()?;
    numbers.sort_unstable();
    numbers.dedup();
    Some(numbers.iter().map(|n| format!("AC{n}")).collect())
}

/// The number of `AC<n>`, as the Plan writes an id (`plan-header.md` §6.4).
fn id_number(id: &str) -> Option<u64> {
    let digits = id.strip_prefix("AC")?;
    let first = digits.bytes().next()?;
    if !(b'1'..=b'9').contains(&first) || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

/// `text` up to the first whitespace; empty if it starts with some.
fn first_field(text: &str) -> &str {
    text.split(char::is_whitespace).next().unwrap_or_default()
}

/// `sha256:` and exactly 64 lowercase hex characters.
fn is_digest(field: &str) -> bool {
    field.strip_prefix(DIGEST_PREFIX).is_some_and(|hex| {
        hex.len() == 64
            && hex
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

/// What the digest covers: `canonical` without line `line` (§4, steps 2 and 3), then,
/// when the line has a list, the line `ac=<list>` (step 4, D-38).
///
/// Only §4d step 4 runs again, not all of it (D-7): `canonicalize` would strip a
/// `U+FEFF` that removing the first line brought to the start.
fn covered(canonical: &str, line: usize, criteria: Option<&[String]>) -> String {
    let lines: Vec<&str> = canonical
        .split('\n')
        .enumerate()
        .filter(|(index, _)| index + 1 != line)
        .map(|(_, text)| text)
        .collect();
    let joined = lines.join("\n");
    let mut covered = joined.trim_end_matches('\n').to_owned();
    if !covered.is_empty() {
        covered.push('\n');
    }
    if let Some(criteria) = criteria {
        covered.push_str(&format!("{CRITERIA_PREFIX}{}\n", criteria.join(",")));
    }
    covered
}

/// Why a file's lock line cannot be read (§3.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LockError {
    InvalidUtf8(InvalidUtf8),
    MoreThanOne {
        lines: Vec<usize>,
    },
    AfterWindow {
        line: usize,
    },
    Ticket {
        line: usize,
        found: String,
    },
    Digest {
        line: usize,
        found: String,
    },
    /// An `ac=` field that breaks §2: malformed, on a pending line, or out of place.
    Criteria {
        line: usize,
        found: String,
        why: &'static str,
    },
    /// [`with_digest`] or [`without_digest`] on a file with no lock line.
    NoLine,
    /// [`with_digest`] or [`without_digest`] for another ticket than the one on the line.
    OtherTicket {
        line: usize,
        found: String,
        wanted: String,
    },
    /// [`with_digest`] on a line that already records a digest.
    AlreadyLocked {
        line: usize,
        ticket: String,
    },
    /// [`without_digest`] on a line that records no digest.
    NotLocked {
        line: usize,
        ticket: String,
    },
}

impl fmt::Display for LockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidUtf8(err) => err.fmt(f),
            Self::MoreThanOne { lines } => {
                let lines = lines
                    .iter()
                    .map(usize::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(
                    f,
                    "lines {lines}: more than one line contains `{MARKER}`; a file has one lock \
                     line, so remove the others"
                )
            }
            Self::AfterWindow { line } => write!(
                f,
                "line {line}: the `{MARKER}` line must be within the first {WINDOW} lines; \
                 moving it down does not unlock the file, so move it back up"
            ),
            Self::Ticket { line, found } => write!(
                f,
                "line {line}: `{MARKER}` must be followed by spaces and a ticket such as \
                 `PROJ-412`; found `{found}`"
            ),
            Self::Digest { line, found } => write!(
                f,
                "line {line}: `{found}` is not a recorded digest, which is `{DIGEST_PREFIX}` and \
                 64 lowercase hex characters, right after the ticket and separated by spaces"
            ),
            Self::Criteria { line, found, why } => {
                write!(f, "line {line}: `{found}` is not a list of criteria: {why}")
            }
            Self::NoLine => write!(
                f,
                "no `{MARKER}` line to write a digest into; a test is locked from a pending line"
            ),
            Self::OtherTicket {
                line,
                found,
                wanted,
            } => write!(
                f,
                "line {line}: the `{MARKER}` line names `{found}`, not `{wanted}`, so `{wanted}` \
                 cannot lock this file"
            ),
            Self::AlreadyLocked { line, ticket } => write!(
                f,
                "line {line}: the `{MARKER}` line is already locked for `{ticket}`, and a digest \
                 is written only into a pending line"
            ),
            Self::NotLocked { line, ticket } => write!(
                f,
                "line {line}: the `{MARKER}` line for `{ticket}` is pending, so there is no \
                 digest to take out"
            ),
        }
    }
}

impl std::error::Error for LockError {}
