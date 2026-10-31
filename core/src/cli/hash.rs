//! `invar hash <file>` — print the canonical digest of a file (§4d).
//!
//! Small on purpose. Its value is that the digest can be inspected by hand:
//! when an integrity check fails, "what does invar actually hash here?" has to
//! be answerable without a debugger.
//!
//! It hashes the **whole file**. A `Confirmed-checksum` or an
//! `Approved-checksum` covers the body below a header, not the file, so `invar hash`
//! run on a `.feature` or a Plan does not reproduce that field. Body
//! extraction is format knowledge and stays in `format/`; this command stays
//! a plain view of canonicalization plus SHA-256.

use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::digest;

#[derive(Debug, clap::Args)]
pub struct Args {
    /// File to hash. Its whole content is hashed, not a body below a header.
    pub path: PathBuf,
}

pub fn run(args: Args) -> Result<()> {
    // Bytes, not `read_to_string`: decoding — and refusing invalid UTF-8 with
    // the offset of the first bad byte — is canonicalization's step 1.
    let bytes = fs::read(&args.path).with_context(|| format!("reading {}", args.path.display()))?;
    let canonical = digest::canonicalize_bytes(&bytes)
        .with_context(|| format!("hashing {}", args.path.display()))?;

    // The digest alone on stdout, so the output can be compared or captured
    // by a script without parsing.
    println!("{}", digest::digest(&canonical));
    Ok(())
}
