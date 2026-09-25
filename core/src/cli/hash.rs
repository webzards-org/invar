//! `invar hash <file>` — print the canonical digest of a file (§4d).
//!
//! Small on purpose. Its value is that the digest can be inspected by hand:
//! when an integrity check fails, "what does invar actually hash here?" has to
//! be answerable without a debugger.
//!
//! Implemented in M1-2, on top of the canonicalization of M1-1.

use std::path::PathBuf;

use anyhow::{Result, bail};

#[derive(Debug, clap::Args)]
pub struct Args {
    /// File to hash. Its entire content is hashed — the body-only rules of §4d
    /// belong to `.feature` and Plan handling, not here.
    pub path: PathBuf,
}

pub fn run(_args: Args) -> Result<()> {
    bail!("`invar hash` is not implemented yet (M1-2)")
}
