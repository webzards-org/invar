//! `invar acceptance unlock` (AT-13): the developer takes the lock off named files of
//! a ticket, because a locked test is wrong (A-8).
//!
//! Implements `contracts/formats/acceptance-lock.md` §11. The contract is the
//! authority; section numbers and `D-n` below point into it.

use std::fmt;
use std::fs;
use std::io::{self, Write as _};
use std::path::{Component, Path, PathBuf};

use crate::format::config::AcceptanceConfig;
use crate::format::lock::{self, LockError as LineError};
use crate::format::plan::Mode;
use crate::format::unlock::{MAX_ROUNDS, Record, RecordError};
use crate::fs::ProjectRoot;
use crate::fs::plans::UNLOCK_FILE;

use super::lock::find::{self, FindError, Scan, Scanned, TEMP_SUFFIX, TestFile};
use super::lock::write::{self, WriteError};
use super::spec::{self, SpecError};
use super::verify::branch;

/// What a run that unlocked its files established.
#[derive(Debug)]
pub struct Unlocked {
    pub ticket: String,
    pub files: Vec<String>,
    pub round: u32,
}

impl Unlocked {
    /// The output on success (§11.2). The last line is what `/invar:relock` keys on.
    pub fn render(&self) -> String {
        let mut out = String::new();
        for file in &self.files {
            out.push_str(&format!("unlocked: {file}\n"));
        }
        out.push_str(&format!(
            "unlocked {} {} file(s), round {} of {MAX_ROUNDS}\n",
            self.ticket,
            self.files.len(),
            self.round
        ));
        out
    }
}

/// Unlock `files` of the ticket of the Plan at `spec`, or say why not (§11.1). A relative
/// path in `files` is read from `cwd`.
pub fn unlock(
    root: &ProjectRoot,
    spec: &Path,
    mode: Mode,
    config: Option<&AcceptanceConfig>,
    files: &[String],
    cwd: &Path,
) -> Result<Unlocked, UnlockError> {
    let (plan, config) = spec::usable(root, spec, mode, config).map_err(UnlockError::Plan)?;
    let ticket = plan.header().ticket.clone();
    let plan_dir = spec.parent().unwrap_or(root.path());

    let mut record = read_record(plan_dir).map_err(UnlockError::Record)?;
    if record.exhausted() {
        return Err(UnlockError::TooManyRounds { ticket });
    }

    let to_unlock = named_files(root, &config.dir, &ticket, files, cwd)?;
    let paths: Vec<String> = to_unlock.iter().map(|file| file.path.clone()).collect();

    // In this order so that a failure before the renames leaves every test as it was, and
    // no file is ever pending with nothing to say it was unlocked (§11.2).
    let prepared = write::prepare_with(root, &to_unlock, |bytes| {
        Ok((lock::without_digest(bytes, &ticket)?, String::new()))
    })
    .map_err(UnlockError::Write)?;
    record.unlock(&paths);
    write_record(plan_dir, &record).map_err(UnlockError::WriteRecord)?;
    write::install(prepared).map_err(UnlockError::Write)?;

    Ok(Unlocked {
        ticket,
        files: paths,
        round: record.round,
    })
}

/// The files named, each checked, or every problem with them (D-14).
fn named_files(
    root: &ProjectRoot,
    dir: &str,
    ticket: &str,
    files: &[String],
    cwd: &Path,
) -> Result<Vec<TestFile>, UnlockError> {
    let scanned = match find::scan(root, dir).map_err(UnlockError::Find)? {
        Scan::Files(scanned) => scanned,
        Scan::Missing | Scan::NotADirectory => Vec::new(),
    };

    let mut found: Vec<TestFile> = Vec::new();
    let mut problems = Vec::new();
    for named in files {
        let checked = relative_to_root(root, named, cwd)
            .and_then(|path| branch::normalised(&path, dir))
            .and_then(|path| {
                let file = scanned.iter().find(|file| file.path == path);
                checkable(root, &path, file, ticket)
            });
        match checked {
            Ok(file) if found.iter().any(|one| one.path == file.path) => {}
            Ok(file) => found.push(file),
            Err(why) => problems.push((named.clone(), why)),
        }
    }
    if problems.is_empty() {
        Ok(found)
    } else {
        Err(UnlockError::Files {
            ticket: ticket.to_owned(),
            problems,
        })
    }
}

/// `named` relative to the project root, with `/` separators, without looking at the disk:
/// a symlink is not followed (D-16), so a path through one names nothing the walk found.
fn relative_to_root(root: &ProjectRoot, named: &str, cwd: &Path) -> Result<String, String> {
    if named.contains(['\n', '\r']) {
        return Err("the path holds a line break".to_owned());
    }
    let cwd = cwd.canonicalize().unwrap_or_else(|_| cwd.to_owned());
    let mut path = PathBuf::new();
    for part in cwd.join(named).components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                path.pop();
            }
            other => path.push(other),
        }
    }
    let inside = path
        .strip_prefix(root.path())
        .map_err(|_| "the path is outside the project".to_owned())?;
    let parts: Vec<&str> = inside.iter().filter_map(|part| part.to_str()).collect();
    Ok(parts.join("/"))
}

/// The file, if it is locked for `ticket` and its digest holds (§11.1).
fn checkable(
    root: &ProjectRoot,
    path: &str,
    file: Option<&Scanned>,
    ticket: &str,
) -> Result<TestFile, String> {
    let Some(file) = file else {
        return Err(if root.path().join(path).exists() {
            "it has no `invar-lock:` line, or is reached through a link, which is not followed, \
             so there is nothing to unlock"
                .to_owned()
        } else {
            "there is no such file".to_owned()
        });
    };
    let line = file.line.as_ref().map_err(LineError::to_string)?;
    if line.ticket != ticket {
        return Err(format!(
            "it belongs to {other}, not {ticket}, so this ticket cannot unlock it. If this Plan is \
             meant to change it, it is declared in `## Acceptance tests to change`",
            other = line.ticket
        ));
    }
    match &line.recorded {
        None => Err(
            "it is pending, not locked, so there is nothing to unlock: the test-writer fixes it \
             and runs `invar acceptance lock`"
                .to_owned(),
        ),
        Some(_) if line.matches() => Ok(TestFile {
            path: file.path.clone(),
            bytes: file.bytes.clone(),
        }),
        Some(recorded) => Err(format!(
            "it changed after it was locked: the line records {recorded}, and the file's digest \
             is {}. Restore it from git first, so that what is unlocked is what was locked",
            line.digest
        )),
    }
}

/// The record in `plan_dir`. No file is a ticket with no record.
pub fn read_record(plan_dir: &Path) -> Result<Record, ReadError> {
    let path = plan_dir.join(UNLOCK_FILE);
    let failed = |source: io::Error| ReadError::Io {
        path: path.clone(),
        source,
    };
    // Not `read` alone, which follows a link (D-16).
    match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.is_file() => {}
        Ok(_) => {
            return Err(failed(io::Error::new(
                io::ErrorKind::InvalidData,
                "it is not a regular file",
            )));
        }
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(Record::default()),
        Err(source) => return Err(failed(source)),
    }
    let bytes = fs::read(&path).map_err(failed)?;
    let text = String::from_utf8(bytes).map_err(|_| {
        failed(io::Error::new(
            io::ErrorKind::InvalidData,
            "it is not UTF-8",
        ))
    })?;
    Record::parse(&text).map_err(|source| ReadError::Invalid { path, source })
}

/// Beside the original and then a rename, so that the record is never half written, and a
/// link at its path is replaced and not followed.
pub fn write_record(plan_dir: &Path, record: &Record) -> io::Result<()> {
    let target = plan_dir.join(UNLOCK_FILE);
    let temp = plan_dir.join(format!(".{UNLOCK_FILE}{TEMP_SUFFIX}"));
    let _ = fs::remove_file(&temp);
    let written = fs::File::create(&temp).and_then(|mut file| {
        file.write_all(record.render().as_bytes())?;
        file.sync_all()
    });
    let renamed = written.and_then(|()| fs::rename(&temp, &target));
    if renamed.is_err() {
        let _ = fs::remove_file(&temp);
    }
    renamed
}

/// Why a record cannot be read.
#[derive(Debug)]
pub enum ReadError {
    Io { path: PathBuf, source: io::Error },
    Invalid { path: PathBuf, source: RecordError },
}

impl fmt::Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => {
                write!(
                    f,
                    "cannot read the unlock record {}: {source}",
                    path.display()
                )
            }
            Self::Invalid { path, source } => write!(
                f,
                "cannot read the unlock record {}: {source}. Only `invar acceptance unlock` and \
                 `invar acceptance lock` write it; restore it, or remove it if no file is unlocked",
                path.display()
            ),
        }
    }
}

impl std::error::Error for ReadError {}

/// Why nothing was unlocked, or not all of it.
#[derive(Debug)]
pub enum UnlockError {
    /// No file was named.
    NoFiles {
        target: String,
    },
    /// What `invar acceptance spec` refuses, in its words.
    Plan(SpecError),
    Record(ReadError),
    /// A third round (D-27).
    TooManyRounds {
        ticket: String,
    },
    Find(FindError),
    /// The files named that cannot be unlocked, each with its reason.
    Files {
        ticket: String,
        problems: Vec<(String, String)>,
    },
    Write(WriteError),
    WriteRecord(io::Error),
}

impl fmt::Display for UnlockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoFiles { target } => write!(
                f,
                "name the files to unlock: `/invar:relock {target} <file>...`; nothing was unlocked"
            ),
            Self::Plan(err) => err.fmt(f),
            Self::Record(err) => write!(f, "{err}; nothing was unlocked"),
            Self::TooManyRounds { ticket } => write!(
                f,
                "{ticket}: both rounds of unlocking are used, and nothing was unlocked. Two \
                 corrections of the same tests mean the criteria are not what was meant, so end \
                 this cycle as `failed`: draft the entry with `/invar:log`, and record it with \
                 `/invar:log-approve`"
            ),
            Self::Find(err) => err.fmt(f),
            Self::Files { ticket, problems } => {
                write!(f, "{ticket}: nothing was unlocked:")?;
                for (file, problem) in problems {
                    write!(f, "\n{file}: {problem}")?;
                }
                Ok(())
            }
            Self::Write(WriteError::Changed { files }) => write!(
                f,
                "changed while they were being unlocked, and nothing was unlocked: {}",
                files.join(", ")
            ),
            Self::Write(WriteError::Line { path, source }) => {
                write!(f, "{path}: {source}; nothing was unlocked")
            }
            Self::Write(WriteError::Prepare { path, source }) => {
                write!(
                    f,
                    "cannot write the new {path}: {source}; nothing was unlocked"
                )
            }
            Self::Write(WriteError::Partial {
                failed,
                source,
                written,
                not_written,
            }) => write!(
                f,
                "cannot replace {failed}: {source}, and this round is used. Unlocked: {}. Still \
                 locked: {}",
                if written.is_empty() {
                    "none".to_owned()
                } else {
                    written.join(", ")
                },
                not_written.join(", ")
            ),
            Self::WriteRecord(err) => {
                write!(
                    f,
                    "cannot write the unlock record: {err}; nothing was unlocked"
                )
            }
        }
    }
}

impl std::error::Error for UnlockError {}
