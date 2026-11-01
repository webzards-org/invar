//! Which files under `acceptance.dir` belong to a ticket (`acceptance-lock.md` §9.1).

use std::fmt;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use crate::format::lock::{self, MARKER};
use crate::fs::ProjectRoot;

/// What a temporary file of an interrupted write ends with (§9.4). It holds a lock
/// line of its own, so the walk has to know not to read it as a test.
pub const TEMP_SUFFIX: &str = ".invar-tmp";

/// One file of the ticket.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestFile {
    /// Relative to the project root, with `/` separators.
    pub path: String,
    /// As read here: what the run is given, and what is compared after it (§9.4).
    pub bytes: Vec<u8>,
}

/// The ticket's files, each list sorted by path. `to_lock` is never empty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub to_lock: Vec<TestFile>,
    pub locked: Vec<String>,
}

impl Found {
    /// Every file to run, sorted (§9.2).
    pub fn all_paths(&self) -> Vec<String> {
        let mut all: Vec<String> = self.to_lock.iter().map(|file| file.path.clone()).collect();
        all.extend(self.locked.iter().cloned());
        all.sort();
        all
    }
}

/// The ticket's files under `dir`, or why they cannot be locked.
pub fn find(root: &ProjectRoot, dir: &str, ticket: &str) -> Result<Found, FindError> {
    let nothing = || FindError::Nothing {
        ticket: ticket.to_owned(),
        dir: dir.to_owned(),
    };
    let start = root.path().join(dir);
    // Not `is_dir`, which follows a link (D-16).
    match fs::symlink_metadata(&start) {
        Ok(metadata) if metadata.is_dir() => {}
        _ => return Err(nothing()),
    }

    let mut paths = Vec::new();
    walk(&start, &mut paths)?;

    let mut found = Found {
        to_lock: Vec::new(),
        locked: Vec::new(),
    };
    let mut problems = Vec::new();
    for path in paths {
        let bytes = fs::read(&path).map_err(|source| FindError::Io {
            path: path.clone(),
            source,
        })?;
        if !contains(&bytes, MARKER.as_bytes()) {
            continue;
        }
        let shown = relative(root.path(), &path);
        let read = match lock::read(&bytes) {
            Ok(read) => read,
            Err(err) => {
                problems.push((
                    shown.unwrap_or_else(|| path.display().to_string()),
                    err.to_string(),
                ));
                continue;
            }
        };
        let Some(line) = read else { continue };
        if line.ticket != ticket {
            continue;
        }
        let Some(shown) = shown else {
            problems.push((
                path.display().to_string(),
                "the path is not valid UTF-8, so it cannot be matched against a report".to_owned(),
            ));
            continue;
        };
        match line.recorded {
            None => found.to_lock.push(TestFile { path: shown, bytes }),
            Some(_) if line.matches() => found.locked.push(shown),
            Some(recorded) => problems.push((
                shown,
                format!(
                    "line {}: it changed after it was locked: the line records {recorded}, and \
                     the file's digest is {}",
                    line.line, line.digest
                ),
            )),
        }
    }

    if !problems.is_empty() {
        problems.sort();
        return Err(FindError::Files {
            ticket: ticket.to_owned(),
            problems,
        });
    }
    if found.to_lock.is_empty() {
        return Err(nothing());
    }
    found.to_lock.sort_by(|a, b| a.path.cmp(&b.path));
    found.locked.sort();
    Ok(found)
}

/// Every regular file under `dir`. A symlink is neither a file nor a directory here
/// (D-16), because `file_type` does not follow it.
fn walk(dir: &Path, paths: &mut Vec<PathBuf>) -> Result<(), FindError> {
    let failed = |path: &Path, source: io::Error| FindError::Io {
        path: path.to_owned(),
        source,
    };
    let mut entries = fs::read_dir(dir)
        .and_then(Iterator::collect::<io::Result<Vec<_>>>)
        .map_err(|source| failed(dir, source))?;
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        let kind = entry.file_type().map_err(|source| failed(&path, source))?;
        if kind.is_dir() {
            walk(&path, paths)?;
        } else if kind.is_file() && !entry.file_name().to_string_lossy().ends_with(TEMP_SUFFIX) {
            paths.push(path);
        }
    }
    Ok(())
}

/// `path` relative to `root`, with `/` separators. `None` if it is not UTF-8.
fn relative(root: &Path, path: &Path) -> Option<String> {
    let parts: Option<Vec<&str>> = path
        .strip_prefix(root)
        .ok()?
        .components()
        .map(|part| match part {
            Component::Normal(name) => name.to_str(),
            _ => None,
        })
        .collect();
    Some(parts?.join("/"))
}

fn contains(bytes: &[u8], needle: &[u8]) -> bool {
    bytes.windows(needle.len()).any(|window| window == needle)
}

/// Why the ticket's files cannot be locked.
#[derive(Debug)]
pub enum FindError {
    /// No directory, or no pending file in it.
    Nothing {
        ticket: String,
        dir: String,
    },
    /// Malformed or tampered files, each with its reason.
    Files {
        ticket: String,
        problems: Vec<(String, String)>,
    },
    Io {
        path: PathBuf,
        source: io::Error,
    },
}

impl fmt::Display for FindError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Nothing { ticket, dir } => write!(
                f,
                "nothing to lock for {ticket} under {dir}: the test-writer writes files with the \
                 pending line `invar-lock: {ticket}`"
            ),
            Self::Files { ticket, problems } => {
                write!(f, "{ticket}: nothing was run or locked:")?;
                for (file, problem) in problems {
                    write!(f, "\n{file}: {problem}")?;
                }
                Ok(())
            }
            Self::Io { path, source } => write!(f, "cannot read {}: {source}", path.display()),
        }
    }
}

impl std::error::Error for FindError {}
