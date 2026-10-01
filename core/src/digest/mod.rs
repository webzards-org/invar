//! Canonicalization and hashing (§4d).
//!
//! Two checksums exist in the system — a `.feature` body against its
//! `Confirmed-checksum`, a Plan body against its `Approved-checksum` — and both
//! rest on one rule: the same content must always produce the same digest.
//! Without that, a mixed Windows/Unix team, or simply `core.autocrlf=true`,
//! makes every integrity check fail for a reason the developer cannot see.
//!
//! Canonicalization, per §4d:
//!
//! 1. decode as UTF-8, strip a BOM if present
//! 2. normalize line endings: `\r\n` and `\r` → `\n`
//! 3. strip trailing whitespace from every line
//! 4. strip trailing blank lines, then append exactly one `\n`
//! 5. (body extraction — *not* done here, see below)
//!
//! **Step 5 lives elsewhere on purpose.** Deciding where a `.feature` header
//! ends and its body begins is format knowledge; it belongs in `format`. This
//! module takes text and returns text, with no idea what kind of document it
//! came from. Keeping the two apart is what lets the fixtures in
//! `contracts/fixtures/` test canonicalization on its own.
//!
//! The shared expectations live in `contracts/fixtures/digest/`; the tests in
//! `tests.rs` run every fixture there through this module.
//!
//! Arrives in M1-1 (`canonicalize`) and M1-2 (`digest`). Digests are recorded
//! as `sha256:<hex>`.

// Nothing in the binary calls into this module until `invar hash` lands in
// M1-2. `expect` rather than `allow`: the moment M1-2 wires it in, the
// expectation goes unfulfilled and the compiler asks for this line to go.
#![cfg_attr(
    not(test),
    expect(dead_code, reason = "called by `invar hash` from M1-2 onward")
)]

use std::fmt;

#[cfg(test)]
mod tests;

/// The UTF-8 byte order mark, as it appears once decoded.
const BOM: char = '\u{FEFF}';

/// Canonicalize text per §4d, steps 1–4.
///
/// Pure: no I/O, and no knowledge of what kind of document the text came
/// from. The input is borrowed, the output is owned.
///
/// The text is already decoded, so step 1 is reduced to stripping a BOM;
/// decoding bytes — and refusing invalid UTF-8 — is [`canonicalize_bytes`].
///
/// An empty result is the empty string, not `"\n"` (fixtures README, D-1):
/// when nothing is left after stripping, nothing is appended.
pub fn canonicalize(input: &str) -> String {
    // 1. Strip a BOM if present. Only at the very start: a U+FEFF anywhere
    //    else is content (a zero-width no-break space), not an encoding mark.
    let text = input.strip_prefix(BOM).unwrap_or(input);

    // 2. Normalize line endings. Order matters: `\r\n` first, or it would
    //    turn into two newlines.
    let text = text.replace("\r\n", "\n").replace('\r', "\n");

    // 3. Strip trailing whitespace from every line. `split('\n')` rather than
    //    `lines()`: `lines()` quietly eats a `\r` before each `\n`, which would
    //    hide whether step 2 actually works. `trim_end()` is Unicode-aware
    //    (fixtures README, D-2) and leaves leading whitespace — indentation —
    //    untouched.
    let lines: Vec<&str> = text.split('\n').map(str::trim_end).collect();

    // 4. Strip trailing blank lines. Only *trailing*: leading and interior
    //    blank lines are content. After step 3 a blank line is an empty one.
    let Some(last) = lines.iter().rposition(|line| !line.is_empty()) else {
        // Nothing but whitespace (or nothing at all): D-1, the empty string.
        return String::new();
    };

    // ...then end with exactly one `\n`.
    let mut canonical = lines[..=last].join("\n");
    canonical.push('\n');
    canonical
}

/// Decode bytes as UTF-8 and canonicalize them — §4d, steps 1–4 in full.
///
/// Invalid UTF-8 is an error, never a panic and never a lossy replacement:
/// replacing bytes with U+FFFD would give two different files the same
/// digest, which is exactly what a checksum must not do.
pub fn canonicalize_bytes(bytes: &[u8]) -> Result<String, InvalidUtf8> {
    let text = std::str::from_utf8(bytes).map_err(|err| InvalidUtf8 {
        offset: err.valid_up_to(),
    })?;
    Ok(canonicalize(text))
}

/// Input that is not valid UTF-8 (§4d, step 1).
///
/// Carries the byte offset of the first invalid sequence, so the message can
/// point at it instead of only saying that something somewhere is wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidUtf8 {
    /// Byte offset of the first byte that is not part of valid UTF-8.
    pub offset: usize,
}

impl fmt::Display for InvalidUtf8 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "not valid UTF-8 (first invalid byte at offset {}); invar only \
             hashes UTF-8 text and does not guess at other encodings",
            self.offset
        )
    }
}

impl std::error::Error for InvalidUtf8 {}
