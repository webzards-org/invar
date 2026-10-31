//! The `invar-lock:` line of an acceptance test file.
//!
//! Implements `contracts/formats/acceptance-lock.md` v1.0.0. The contract is the
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

/// The lock line of a file, and the digest of what it covers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockLine {
    pub ticket: String,
    /// 1-based.
    pub line: usize,
    /// The digest on the line. `None` means the line is pending.
    pub recorded: Option<String>,
    /// The file's digest as it is now (§4).
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
/// right after the ticket (§4).
///
/// Everything before the marker, any trailing text and the line's own ending stay,
/// and no other byte changes: this works on the file as it is, not on its canonical
/// form, so its line endings, BOM and trailing whitespace survive. Refuses a file
/// with no line, a line for another ticket, and a line already locked.
// `invar acceptance lock` is the caller (AT-8); remove this with its arrival.
#[allow(dead_code)]
pub fn with_digest(bytes: &[u8], ticket: &str) -> Result<Vec<u8>, LockError> {
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

    let mut written = Vec::with_capacity(bytes.len() + lock.digest.len() + 1);
    written.extend_from_slice(&bytes[..at]);
    written.push(b' ');
    written.extend_from_slice(lock.digest.as_bytes());
    written.extend_from_slice(&bytes[at..]);
    Ok(written)
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
    let (ticket, recorded) = fields(after_marker, line)?;
    Ok(Some(LockLine {
        ticket: ticket.to_owned(),
        line,
        recorded: recorded.map(str::to_owned),
        digest: digest::digest(&covered(canonical, line)),
    }))
}

/// The ticket and the recorded digest, from what follows the marker (§2).
fn fields(after_marker: &str, line: usize) -> Result<(&str, Option<&str>), LockError> {
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
    let trailing = match recorded {
        Some(digest) => &next[digest.len()..],
        None => after_ticket,
    };
    if let Some(field) = trailing
        .split_whitespace()
        .find(|field| field.starts_with(DIGEST_PREFIX))
    {
        return Err(LockError::Digest {
            line,
            found: field.to_owned(),
        });
    }
    Ok((ticket, recorded))
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

/// What the digest covers: `canonical` without line `line` (§4, steps 2 and 3).
///
/// Only §4d step 4 runs again, not all of it (D-7): `canonicalize` would strip a
/// `U+FEFF` that removing the first line brought to the start.
fn covered(canonical: &str, line: usize) -> String {
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
    /// [`with_digest`] on a file with no lock line.
    NoLine,
    /// [`with_digest`] for another ticket than the one on the line.
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
        }
    }
}

impl std::error::Error for LockError {}
