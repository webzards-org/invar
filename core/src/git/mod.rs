//! The few things the core asks `git`, by running it as an external command.
//!
//! No git library (§14.2): `git` is already on every machine that uses invar,
//! and running it means the core sees exactly the configuration the developer
//! sees — global, per-repository, `includeIf`, environment overrides — without
//! reimplementing any of it.

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::calendar::civil_from_days;

#[cfg(test)]
mod tests;

/// `git config user.name`, run in `dir`: who is approving (plan-header.md
/// §5.3). An unset or empty name is an error, never an empty field.
pub fn user_name(dir: &Path) -> Result<String, GitError> {
    let output = run(dir, &["config", "user.name"])?;
    // `git config` exits 1 when the key is not set; anything else is a real
    // failure (an unreadable config file, say).
    if !output.status.success() && output.status.code() != Some(1) {
        return Err(failed("git config user.name", &output));
    }
    let name = text("git config user.name", &output.stdout)?
        .trim()
        .to_owned();
    if name.is_empty() {
        return Err(GitError::UserNameMissing {
            dir: dir.to_owned(),
        });
    }
    Ok(name)
}

/// Today's date, `YYYY-MM-DD`, as git would date a commit made now in `dir`:
/// the committer clock, in the committer's local time zone
/// (plan-header.md §5.3, D-13).
pub fn today(dir: &Path) -> Result<String, GitError> {
    Ok(now(dir)?.local_date())
}

/// The current moment as git would stamp a commit made now in `dir`: the
/// committer clock, with the committer's local UTC offset.
///
/// Asked of git rather than computed here because the standard library has
/// no local time zone, and because it honours `GIT_COMMITTER_DATE` — the time
/// is the one the commit carrying the change will show. The identity is
/// overridden on the command line so a missing `user.email` cannot fail it:
/// only the timestamp is read.
pub fn now(dir: &Path) -> Result<Instant, GitError> {
    const COMMAND: &str = "git var GIT_COMMITTER_IDENT";
    let output = run(
        dir,
        &[
            "-c",
            "user.name=invar",
            "-c",
            "user.email=invar@localhost",
            "var",
            "GIT_COMMITTER_IDENT",
        ],
    )?;
    if !output.status.success() {
        return Err(failed(COMMAND, &output));
    }
    let ident = text(COMMAND, &output.stdout)?;
    Instant::from_ident(ident).ok_or_else(|| GitError::UnexpectedOutput {
        command: COMMAND,
        output: ident.trim().to_owned(),
    })
}

/// A moment in time, as git reports it: seconds since the Unix epoch, and
/// the UTC offset of the clock that read it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Instant {
    /// Seconds since 1970-01-01T00:00:00Z.
    pub seconds: i64,
    /// The local UTC offset, in minutes: `+0200` is 120.
    pub offset_minutes: i32,
}

impl Instant {
    /// `Name <email> 1791224788 +0200` → that instant. `None` when the last
    /// two fields are not a Unix time and a `±hhmm` offset, or when the
    /// result falls outside the years 0000–9999.
    ///
    /// The offset's hours are 00–23 and its minutes 00–59, which is what
    /// RFC 3339 allows and what the decision entry's `date` has to be. Real
    /// zones stay between −12:00 and +14:00; the wider range only keeps a
    /// `GIT_COMMITTER_DATE` that git itself accepts from turning into a date
    /// no reader of the entry can parse.
    pub fn from_ident(ident: &str) -> Option<Self> {
        let mut fields = ident.split_whitespace().rev();
        let offset = fields.next()?;
        let seconds: i64 = fields.next()?.parse().ok()?;

        let (sign, digits) = match offset.split_at_checked(1)? {
            ("+", digits) => (1, digits),
            ("-", digits) => (-1, digits),
            _ => return None,
        };
        if digits.len() != 4 || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let hours: i32 = digits[..2].parse().ok()?;
        let minutes: i32 = digits[2..].parse().ok()?;
        if hours > 23 || minutes >= 60 {
            return None;
        }
        let instant = Self {
            seconds,
            offset_minutes: sign * (hours * 60 + minutes),
        };
        // Both renderings must stay within four-digit years.
        let in_range = |seconds: i64| (0..=9999).contains(&civil(seconds).0);
        (in_range(instant.local_seconds()?) && in_range(seconds)).then_some(instant)
    }

    /// `YYYY-MM-DD` in local time.
    pub fn local_date(self) -> String {
        let (year, month, day, ..) = civil(self.local_seconds().unwrap_or(self.seconds));
        format!("{year:04}-{month:02}-{day:02}")
    }

    /// `YYYY-MM-DDTHH:MM:SS±HH:MM` in local time, with its offset (RFC 3339):
    /// a decision entry's `date` (decision-entry.md D-2).
    pub fn local_rfc3339(self) -> String {
        let (year, month, day, hour, minute, second) =
            civil(self.local_seconds().unwrap_or(self.seconds));
        let sign = if self.offset_minutes < 0 { '-' } else { '+' };
        let offset = self.offset_minutes.unsigned_abs();
        format!(
            "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}{sign}{:02}:{:02}",
            offset / 60,
            offset % 60
        )
    }

    /// `YYYY-MM-DD-HHMMSS` in UTC: a decision entry's filename prefix
    /// (decision-entry.md §2).
    pub fn utc_stamp(self) -> String {
        let (year, month, day, hour, minute, second) = civil(self.seconds);
        format!("{year:04}-{month:02}-{day:02}-{hour:02}{minute:02}{second:02}")
    }

    fn local_seconds(self) -> Option<i64> {
        self.seconds
            .checked_add(i64::from(self.offset_minutes) * 60)
    }
}

/// Seconds since the epoch → (year, month, day, hour, minute, second).
fn civil(seconds: i64) -> (i64, u32, u32, u32, u32, u32) {
    let (year, month, day) = civil_from_days(seconds.div_euclid(86_400));
    let of_day = seconds.rem_euclid(86_400) as u32;
    (
        year,
        month,
        day,
        of_day / 3600,
        of_day / 60 % 60,
        of_day % 60,
    )
}

fn run(dir: &Path, args: &[&str]) -> Result<std::process::Output, GitError> {
    // Starting a process in a directory that is not there fails with
    // `NotFound`, the very kind that means `git` is not installed. Without
    // this check the developer would be told to install a git they have.
    if !dir.is_dir() {
        return Err(GitError::NotADirectory {
            dir: dir.to_owned(),
        });
    }
    Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .map_err(|source| match source.kind() {
            io::ErrorKind::NotFound => GitError::NotInstalled,
            _ => GitError::CannotRun { source },
        })
}

fn text<'a>(command: &'static str, bytes: &'a [u8]) -> Result<&'a str, GitError> {
    std::str::from_utf8(bytes).map_err(|_| GitError::UnexpectedOutput {
        command,
        output: String::from_utf8_lossy(bytes).trim().to_owned(),
    })
}

fn failed(command: &'static str, output: &std::process::Output) -> GitError {
    GitError::Failed {
        command,
        status: output.status.code(),
        stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
    }
}

// --- errors -----------------------------------------------------------------

#[derive(Debug)]
pub enum GitError {
    /// No `git` on `PATH`.
    NotInstalled,
    /// `git` exists but could not be started.
    CannotRun { source: io::Error },
    /// `git` ran and failed.
    Failed {
        command: &'static str,
        status: Option<i32>,
        stderr: String,
    },
    /// `git` succeeded but printed something this code does not understand.
    UnexpectedOutput {
        command: &'static str,
        output: String,
    },
    /// `user.name` is unset or empty.
    UserNameMissing { dir: PathBuf },
    /// The directory `git` was to run in does not exist, or is not a directory.
    NotADirectory { dir: PathBuf },
}

impl fmt::Display for GitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotInstalled => write!(
                f,
                "`git` was not found on PATH; invar runs git to read who is approving \
                 and the current date and time — install git, or add it to PATH"
            ),
            Self::CannotRun { source } => write!(f, "cannot run `git`: {source}"),
            Self::NotADirectory { dir } => write!(
                f,
                "cannot run `git` in {}: it does not exist, or it is not a directory",
                dir.display()
            ),
            Self::Failed {
                command,
                status,
                stderr,
            } => {
                let status = status.map_or("a signal".to_owned(), |code| format!("exit {code}"));
                write!(f, "`{command}` failed ({status})")?;
                if !stderr.is_empty() {
                    write!(f, ": {stderr}")?;
                }
                Ok(())
            }
            Self::UnexpectedOutput { command, output } => {
                write!(f, "`{command}` printed something unexpected: {output:?}")
            }
            Self::UserNameMissing { dir } => write!(
                f,
                "`git config user.name` is not set (looked from {}). The approval records \
                 who approved the Plan, so it is never left empty. Set it with \
                 `git config --global user.name \"Your Name\"`, or without `--global` \
                 for this repository only, then approve again",
                dir.display()
            ),
        }
    }
}

impl std::error::Error for GitError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::CannotRun { source } => Some(source),
            _ => None,
        }
    }
}
