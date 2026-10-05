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
/// the committer clock, in the committer's local time zone (§5.3).
///
/// Asked of git rather than computed here because the standard library has
/// no local time zone, and because it honours `GIT_COMMITTER_DATE` — the date
/// is the one the commit carrying the approval will show. The identity is
/// overridden on the command line so a missing `user.email` cannot fail it:
/// only the timestamp is read.
pub fn today(dir: &Path) -> Result<String, GitError> {
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
    date_from_ident(ident).ok_or_else(|| GitError::UnexpectedOutput {
        command: COMMAND,
        output: ident.trim().to_owned(),
    })
}

/// `Name <email> 1791224788 +0200` → the local date of that instant,
/// `2026-10-05`. `None` when the last two fields are not a Unix time and a
/// `±hhmm` offset.
pub fn date_from_ident(ident: &str) -> Option<String> {
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
    let hours: i64 = digits[..2].parse().ok()?;
    let minutes: i64 = digits[2..].parse().ok()?;
    if minutes >= 60 {
        return None;
    }
    let local = seconds.checked_add(sign * (hours * 3600 + minutes * 60))?;

    let (year, month, day) = civil_from_days(local.div_euclid(86_400));
    (0..=9999)
        .contains(&year)
        .then(|| format!("{year:04}-{month:02}-{day:02}"))
}

/// Days since 1970-01-01 → (year, month, day) in the proleptic Gregorian
/// calendar. Howard Hinnant's `civil_from_days`, exact for every `i64` input
/// that fits.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097); // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11], March-based
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

fn run(dir: &Path, args: &[&str]) -> Result<std::process::Output, GitError> {
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
}

impl fmt::Display for GitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotInstalled => write!(
                f,
                "`git` was not found on PATH; invar runs git to read who is approving \
                 and today's date — install git, or add it to PATH"
            ),
            Self::CannotRun { source } => write!(f, "cannot run `git`: {source}"),
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
