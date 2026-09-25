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
//! Arrives in M1-1 (`canonicalize`) and M1-2 (`digest`), with signatures
//! roughly:
//!
//! ```text
//! pub fn canonicalize(input: &str) -> String
//! pub fn digest(canonical_text: &str) -> String   // "sha256:<hex>"
//! ```
//!
//! Digests are recorded as `sha256:<hex>`.
