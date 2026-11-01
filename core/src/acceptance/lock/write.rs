//! Writing the digests, all or nothing (`acceptance-lock.md` §9.4, D-15).

use std::fmt;
use std::fs;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};

use crate::format::lock::{self, LockError as LineError};
use crate::fs::ProjectRoot;

use super::find::{TEMP_SUFFIX, TestFile};

/// One file locked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Written {
    pub path: String,
    pub digest: String,
}

/// A new file ready beside its original. Dropping it removes what is left of the
/// temporary file, so no path out of here leaves one behind.
#[derive(Debug)]
pub struct Prepared {
    temp: PathBuf,
    target: PathBuf,
    written: Written,
}

impl Drop for Prepared {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.temp);
    }
}

/// Lock every one of `files`, which are what the run was given.
pub fn write(
    root: &ProjectRoot,
    ticket: &str,
    files: &[TestFile],
) -> Result<Vec<Written>, WriteError> {
    let prepared = prepare(root, ticket, files)?;
    install(prepared)
}

/// Steps 1 and 2 of §9.4. Nothing here touches a test file.
pub fn prepare(
    root: &ProjectRoot,
    ticket: &str,
    files: &[TestFile],
) -> Result<Vec<Prepared>, WriteError> {
    let changed: Vec<String> = files
        .iter()
        .filter(|file| fs::read(root.path().join(&file.path)).ok().as_ref() != Some(&file.bytes))
        .map(|file| file.path.clone())
        .collect();
    if !changed.is_empty() {
        return Err(WriteError::Changed { files: changed });
    }

    let mut prepared = Vec::with_capacity(files.len());
    for file in files {
        let line = |source| WriteError::Line {
            path: file.path.clone(),
            source,
        };
        let digest = lock::read(&file.bytes)
            .map_err(line)?
            .ok_or_else(|| line(LineError::NoLine))?
            .digest;
        let bytes = lock::with_digest(&file.bytes, ticket).map_err(line)?;

        let target = root.path().join(&file.path);
        let entry = Prepared {
            temp: temp_path(&target),
            target,
            written: Written {
                path: file.path.clone(),
                digest,
            },
        };
        write_temp(&entry.temp, &entry.target, &bytes).map_err(|source| WriteError::Prepare {
            path: file.path.clone(),
            source,
        })?;
        prepared.push(entry);
    }
    Ok(prepared)
}

/// Step 3 of §9.4. A failure here is the only partial case (D-15).
pub fn install(prepared: Vec<Prepared>) -> Result<Vec<Written>, WriteError> {
    let mut written = Vec::with_capacity(prepared.len());
    let mut rest = prepared.into_iter();
    while let Some(entry) = rest.next() {
        if let Err(source) = fs::rename(&entry.temp, &entry.target) {
            let mut not_written = vec![entry.written.path.clone()];
            not_written.extend(rest.map(|left| left.written.path.clone()));
            return Err(WriteError::Partial {
                failed: entry.written.path.clone(),
                source,
                written: written
                    .iter()
                    .map(|one: &Written| one.path.clone())
                    .collect(),
                not_written,
            });
        }
        written.push(entry.written.clone());
    }
    Ok(written)
}

/// The name a leftover is recognised by (§9.1), beside the original so that the
/// rename stays on one file system.
fn temp_path(target: &Path) -> PathBuf {
    let name = target
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    target.with_file_name(format!(".{name}{TEMP_SUFFIX}"))
}

/// `bytes` in a temporary file, with the permissions of the original.
fn write_temp(temp: &Path, target: &Path, bytes: &[u8]) -> io::Result<()> {
    let permissions = fs::metadata(target)?.permissions();
    // A leftover of an earlier interrupted run is ours to remove.
    let _ = fs::remove_file(temp);
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(temp)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    fs::set_permissions(temp, permissions)
}

/// Why the digests were not written, or not all of them.
#[derive(Debug)]
pub enum WriteError {
    /// Different from the bytes that ran. Nothing was written.
    Changed { files: Vec<String> },
    /// The line cannot take a digest. Nothing was written.
    Line { path: String, source: LineError },
    /// A temporary file could not be written. Nothing was written.
    Prepare { path: String, source: io::Error },
    /// A rename failed, with the files before it already written.
    Partial {
        failed: String,
        source: io::Error,
        written: Vec<String>,
        not_written: Vec<String>,
    },
}

impl fmt::Display for WriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Changed { files } => write!(
                f,
                "changed while the tests ran, so what ran is not what would be locked, and \
                 nothing was locked: {}. Run `invar acceptance lock` again",
                files.join(", ")
            ),
            Self::Line { path, source } => {
                write!(f, "{path}: {source}; nothing was locked")
            }
            Self::Prepare { path, source } => {
                write!(
                    f,
                    "cannot write the new {path}: {source}; nothing was locked"
                )
            }
            Self::Partial {
                failed,
                source,
                written,
                not_written,
            } => write!(
                f,
                "cannot replace {failed}: {source}. Locked: {}. Not locked: {}. Run `invar \
                 acceptance lock` again: what was locked now counts as already locked",
                if written.is_empty() {
                    "none".to_owned()
                } else {
                    written.join(", ")
                },
                not_written.join(", ")
            ),
        }
    }
}

impl std::error::Error for WriteError {}
