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
//!
//! `--lock` answers the same question for a locked acceptance test, whose digest
//! leaves out its `invar-lock:` line (acceptance-lock.md §4). What the line says
//! goes to stderr, so stdout stays the digest alone. The digest covers the line's
//! `ac=` list when it has one, and `--ac` gives the digest another list would get.

use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};

use crate::digest;
use crate::format::lock;

#[derive(Debug, clap::Args)]
pub struct Args {
    /// File to hash. Its whole content is hashed, not a body below a header.
    pub path: PathBuf,

    /// Print the file's lock digest instead: the whole file without its
    /// `invar-lock:` line. Says on stderr what the line records, and whether it
    /// matches. A file with no such line, or a malformed one, is an error.
    #[arg(long)]
    pub lock: bool,

    /// With `--lock`: the digest the line would record with this list of criteria,
    /// such as `AC1,AC2`, written after it as `ac=AC1,AC2`. Says on stderr the fields
    /// that line would hold after the ticket.
    #[arg(long, value_name = "IDS", requires = "lock", value_delimiter = ',')]
    pub ac: Vec<String>,
}

pub fn run(args: Args) -> Result<()> {
    // Bytes, not `read_to_string`: decoding — and refusing invalid UTF-8 with
    // the offset of the first bad byte — is canonicalization's step 1.
    let bytes = fs::read(&args.path).with_context(|| format!("reading {}", args.path.display()))?;
    if args.lock {
        return print_lock(&args, &bytes);
    }
    let canonical = digest::canonicalize_bytes(&bytes)
        .with_context(|| format!("hashing {}", args.path.display()))?;

    // The digest alone on stdout, so the output can be compared or captured
    // by a script without parsing.
    println!("{}", digest::digest(&canonical));
    Ok(())
}

fn print_lock(args: &Args, bytes: &[u8]) -> Result<()> {
    let path = args.path.display();
    let Some(line) = lock::read(bytes).with_context(|| format!("reading {path}"))? else {
        bail!(
            "{path} has no `{}` line, so it has no lock digest; `invar hash` without \
             `--lock` hashes the whole file",
            lock::MARKER
        );
    };
    let state = match &line.recorded {
        None => "pending; `invar acceptance lock` records the criteria its tests name".to_owned(),
        Some(_) if line.matches() => "locked, and the digest matches".to_owned(),
        Some(recorded) => format!("locked at {recorded}, and the file no longer matches"),
    };
    let list = match &line.criteria {
        Some(ids) if line.recorded.is_some() => format!(
            "; it records {}{}, under the digest",
            lock::CRITERIA_PREFIX,
            ids.join(",")
        ),
        None if line.recorded.is_some() => {
            "; it records no criteria, as a line locked before they were".to_owned()
        }
        _ => String::new(),
    };
    eprintln!("{path}:{}: {}: {state}{list}", line.line, line.ticket);
    if args.ac.is_empty() {
        println!("{}", line.digest);
        return Ok(());
    }
    let (digest, ids) =
        lock::digest_covering(bytes, &args.ac).with_context(|| format!("reading {path}"))?;
    eprintln!(
        "{path}:{}: with {}{}, the line holds: {} {digest} {}{}",
        line.line,
        lock::CRITERIA_PREFIX,
        ids.join(","),
        line.ticket,
        lock::CRITERIA_PREFIX,
        ids.join(",")
    );
    println!("{digest}");
    Ok(())
}
