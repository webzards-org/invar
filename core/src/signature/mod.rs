//! A signed approval: the statement `plan approve` signs, and the signing.
//!
//! Implements `contracts/formats/approval-signature.md` v1.0.0; section numbers
//! below point into it.
//!
//! **Invar never reads a key** (§5). The key is git's `user.signingkey` and the
//! program is git's `gpg.ssh.program`, else `ssh-keygen`, run as git runs it for a
//! signed commit, with `-n invar-approval` instead of `-n git`. Whatever asks the
//! developer for the key — an agent, a passphrase prompt, an askpass, a touch —
//! asks as it does for a commit.
//!
//! **Within 90 seconds** (D-4). Claude Code moves a skill's shell block to the
//! background after 120, and `/invar:approve` would never see the outcome. So a
//! signing program still running at the deadline is killed, and nothing is
//! written.
//!
//! **Everything it started is stopped too, where that is safe.** A signing
//! program may start others — ssh-keygen an askpass dialog, a password manager's
//! signer a helper — and they hold its output pipes open. Without a controlling
//! terminal, which is how `/invar:approve` runs it, the program gets a process
//! group of its own, and the deadline kills the whole group. With one, it stays
//! in ours: a process group of its own would be a background job of that
//! terminal, and the kernel stops a background job that reads its passphrase
//! from `/dev/tty` (SIGTTIN), so it could never be answered. Either way the
//! deadline returns at once, and never waits for the pipes to close.

use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::git::{self, GitError};

mod verify;

#[cfg(test)]
mod tests;

pub use verify::{Approvers, Refused, Verified};

/// The SSH signature namespace of an approval (§4). A signature made for a git
/// commit (`-n git`) is not one.
pub const NAMESPACE: &str = "invar-approval";

/// How long the signing program may take (§5, D-4).
pub const DEADLINE: Duration = Duration::from_secs(90);

/// The first line of what the signing program writes (§4).
const ARMOR: &str = "-----BEGIN SSH SIGNATURE-----";

/// Its last line.
const ARMOR_END: &str = "-----END SSH SIGNATURE-----";

/// An armored signature as one line of base64, for a trailer (§6.5, D-10): without
/// its first and last lines and without line breaks.
pub fn one_line(armored: &str) -> String {
    armored
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && *line != ARMOR && *line != ARMOR_END)
        .collect()
}

/// [`one_line`] undone: the armor back, 70 characters a line, as `ssh-keygen` writes
/// it.
pub fn armored(one_line: &str) -> String {
    let mut out = format!("{ARMOR}\n");
    let chars: Vec<char> = one_line.chars().collect();
    for line in chars.chunks(70) {
        out.extend(line);
        out.push('\n');
    }
    out.push_str(ARMOR_END);
    out.push('\n');
    out
}

/// What is signed (§3): four lines, each ending with `\n`.
pub fn statement(ticket: &str, approved_checksum: &str, approval_id: &str) -> String {
    format!(
        "invar-approval 1\nticket: {ticket}\napproved-checksum: {approved_checksum}\n\
         approval-id: {approval_id}\n"
    )
}

/// A new `Approval-id` (plan-header.md §5): 128 random bits from the operating
/// system, as 32 lowercase hex digits. A new one for every signed approval, so
/// that no two approvals sign the same statement (D-7).
pub fn new_approval_id() -> io::Result<String> {
    let mut bytes = [0u8; 16];
    File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

/// The key and the program git signs with, as `git config` reads them in a
/// project (§5, steps 1 and 2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signer {
    /// `gpg.ssh.program`, else `ssh-keygen`.
    pub program: String,
    pub key: Key,
}

/// `user.signingkey`, as git accepts it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Key {
    /// A path to a public or private key file, with `~/` expanded.
    File(PathBuf),
    /// A public key written in place (`key::ssh-ed25519 AAAA…` or
    /// `ssh-ed25519 AAAA…`), without the `key::`.
    Literal(String),
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::File(path) => write!(f, "{}", path.display()),
            Self::Literal(key) => {
                // The type and the start of the key: enough to tell keys apart.
                let mut words = key.split_whitespace();
                let kind = words.next().unwrap_or("");
                let start: String = words.next().unwrap_or("").chars().take(16).collect();
                write!(f, "{kind} {start}…")
            }
        }
    }
}

impl Signer {
    /// Read git's signing setup in `dir`, the project root.
    pub fn from_git(dir: &Path) -> Result<Self, SignError> {
        let format = git::config_value(dir, "gpg.format")?;
        if format.as_deref() != Some("ssh") {
            return Err(SignError::NotSsh { format });
        }
        let key = git::config_value(dir, "user.signingkey")?.ok_or(SignError::NoKey)?;
        let program =
            git::config_value(dir, "gpg.ssh.program")?.unwrap_or_else(|| "ssh-keygen".to_owned());
        Ok(Self {
            program,
            key: parse_key(&key, std::env::var_os("HOME").map(PathBuf::from)),
        })
    }

    /// Sign `statement` in `dir`, within `deadline`: the armored signature of an
    /// approval.
    pub fn sign(
        &self,
        dir: &Path,
        statement: &str,
        deadline: Duration,
    ) -> Result<String, SignError> {
        self.sign_in(dir, NAMESPACE, statement, deadline)
    }

    /// [`sign`](Self::sign) in `namespace`: `invar-skip` or `invar-config` for the
    /// trailers of §6.5.
    pub fn sign_in(
        &self,
        dir: &Path,
        namespace: &str,
        statement: &str,
        deadline: Duration,
    ) -> Result<String, SignError> {
        // A key written in place goes to a file for the program, as git does it.
        let temp;
        let key: &Path = match &self.key {
            Key::File(path) => path,
            Key::Literal(key) => {
                temp = TempFile::write("signing-key", "pub", format!("{key}\n").as_bytes())
                    .map_err(|(path, source)| SignError::TempKey { path, source })?;
                temp.path()
            }
        };
        let mut command = Command::new(&self.program);
        command
            .args(["-Y", "sign", "-n", namespace, "-f"])
            .arg(key)
            .current_dir(dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let group = isolate(&mut command, has_terminal());
        let child = command.spawn().map_err(|source| SignError::CannotStart {
            program: self.program.clone(),
            source,
        })?;
        let finished =
            run_with_deadline(child, group, statement.as_bytes(), deadline).map_err(|source| {
                SignError::CannotStart {
                    program: self.program.clone(),
                    source,
                }
            })?;
        let Some(Finished {
            status,
            stdout,
            stderr,
        }) = finished
        else {
            return Err(SignError::TimedOut {
                program: self.program.clone(),
                seconds: deadline.as_secs(),
            });
        };
        let stderr = String::from_utf8_lossy(&stderr).trim().to_owned();
        if !status.success() {
            return Err(SignError::Failed {
                program: self.program.clone(),
                status: status.code(),
                stderr,
            });
        }
        match String::from_utf8(stdout) {
            Ok(signature) if signature.starts_with(ARMOR) => Ok(signature),
            _ => Err(SignError::NoSignature {
                program: self.program.clone(),
            }),
        }
    }
}

/// `user.signingkey` as git reads it: `key::`, or `ssh-`, in front is a key
/// written in place; anything else is a path, `~/` meaning `home`.
fn parse_key(value: &str, home: Option<PathBuf>) -> Key {
    if let Some(key) = value.strip_prefix("key::") {
        return Key::Literal(key.trim().to_owned());
    }
    // git's own rule: `key::`, or, for compatibility, `ssh-` at the start.
    if value.starts_with("ssh-") {
        return Key::Literal(value.to_owned());
    }
    match (value.strip_prefix("~/"), home) {
        (Some(rest), Some(home)) => Key::File(home.join(rest)),
        _ => Key::File(PathBuf::from(value)),
    }
}

/// A program that finished: its status and what it wrote.
#[derive(Debug)]
struct Finished {
    status: ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

/// Whether the program runs in a process group of its own, which the deadline
/// kills as a whole.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Group {
    Own,
    Ours,
}

/// Whether this process has a controlling terminal: `/dev/tty` opens only then.
#[cfg(unix)]
fn has_terminal() -> bool {
    File::open("/dev/tty").is_ok()
}

#[cfg(not(unix))]
fn has_terminal() -> bool {
    false
}

/// Give `command` a process group of its own, unless there is a terminal it may
/// have to read a passphrase from (see the module comment).
#[cfg(unix)]
fn isolate(command: &mut Command, terminal: bool) -> Group {
    use std::os::unix::process::CommandExt as _;
    if terminal {
        return Group::Ours;
    }
    command.process_group(0);
    Group::Own
}

#[cfg(not(unix))]
fn isolate(_: &mut Command, _: bool) -> Group {
    Group::Ours
}

/// Kill `child`, and with [`Group::Own`] every process in its group, then reap
/// it. The group is killed with the `kill` command: the core has no libc
/// binding of its own.
fn stop(child: &mut Child, group: Group) {
    if group == Group::Own {
        // `process_group(0)` made the child the leader: the group's id is its pid.
        let _ = Command::new("kill")
            .args(["-KILL", "--", &format!("-{}", child.id())])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// Feed `input` to `child`, and wait for it at most `deadline`: its status and
/// output, or `None` when the deadline passed and it was killed.
///
/// A program that exits in time is read to the end, as before. Past the
/// deadline it is stopped, and this returns at once: the threads reading its
/// pipes are left to finish on their own, since something it started that
/// [`stop`] could not reach may still hold them open.
fn run_with_deadline(
    mut child: Child,
    group: Group,
    input: &[u8],
    deadline: Duration,
) -> io::Result<Option<Finished>> {
    let read = |pipe: Option<Box<dyn io::Read + Send>>| {
        thread::spawn(move || {
            let mut bytes = Vec::new();
            if let Some(mut pipe) = pipe {
                let _ = pipe.read_to_end(&mut bytes);
            }
            bytes
        })
    };
    let stdout = read(
        child
            .stdout
            .take()
            .map(|p| Box::new(p) as Box<dyn io::Read + Send>),
    );
    let stderr = read(
        child
            .stderr
            .take()
            .map(|p| Box::new(p) as Box<dyn io::Read + Send>),
    );
    if let Some(mut stdin) = child.stdin.take() {
        // A program that exits without reading its input is judged by its status.
        let _ = stdin.write_all(input);
    }

    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break Some(status);
        }
        if started.elapsed() >= deadline {
            stop(&mut child, group);
            break None;
        }
        thread::sleep(Duration::from_millis(20));
    };
    let Some(status) = status else {
        // Dropping the handles detaches the threads.
        return Ok(None);
    };
    Ok(Some(Finished {
        status,
        stdout: stdout.join().unwrap_or_default(),
        stderr: stderr.join().unwrap_or_default(),
    }))
}

/// A file of its own in the temporary directory, removed when dropped: a public key
/// for the signing program, or the approvers list and a signature for the verifier.
struct TempFile {
    path: PathBuf,
}

impl TempFile {
    /// `.invar-<tag>-<pid>-<nanos>-<n>.<extension>`, created new, never replacing a
    /// file. The path is given back with the error.
    fn write(tag: &str, extension: &str, bytes: &[u8]) -> Result<Self, (PathBuf, io::Error)> {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.subsec_nanos());
        let count = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            ".invar-{tag}-{}-{nanos}-{count}.{extension}",
            std::process::id()
        ));
        let mut file: File = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|source| (path.clone(), source))?;
        let temp = Self { path };
        file.write_all(bytes)
            .map_err(|source| (temp.path.clone(), source))?;
        Ok(temp)
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

// --- errors -----------------------------------------------------------------

#[derive(Debug)]
pub enum SignError {
    /// `gpg.format` is not `ssh`: the value, if any.
    NotSsh {
        format: Option<String>,
    },
    /// `user.signingkey` is not set.
    NoKey,
    Git(GitError),
    /// A key written in place could not be written to a file.
    TempKey {
        path: PathBuf,
        source: io::Error,
    },
    CannotStart {
        program: String,
        source: io::Error,
    },
    Failed {
        program: String,
        status: Option<i32>,
        stderr: String,
    },
    /// The program succeeded and wrote no armored signature.
    NoSignature {
        program: String,
    },
    TimedOut {
        program: String,
        seconds: u64,
    },
}

impl From<GitError> for SignError {
    fn from(err: GitError) -> Self {
        Self::Git(err)
    }
}

/// What to set, the same for both settings that can be missing.
const SETUP: &str = "set `git config gpg.format ssh` and `git config user.signingkey <your SSH \
                     key>`, as for signed commits (docs/INSTALL.md, \"Signed approvals\")";

impl fmt::Display for SignError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotSsh { format } => {
                let now = match format {
                    Some(format) => format!("git signs with `gpg.format = {format}`"),
                    None => "git has no `gpg.format` set, so it would sign with GPG".to_owned(),
                };
                write!(
                    f,
                    "`approval.signed` is on in invar.yaml, and {now}; an approval is signed with \
                     an SSH key only. {SETUP}"
                )
            }
            Self::NoKey => write!(
                f,
                "`approval.signed` is on in invar.yaml, and git has no `user.signingkey`. {SETUP}"
            ),
            Self::Git(err) => write!(f, "cannot read git's signing setup: {err}"),
            Self::TempKey { path, source } => write!(
                f,
                "cannot write the signing key to {} for the signing program: {source}",
                path.display()
            ),
            Self::CannotStart { program, source } => {
                write!(f, "cannot run `{program}` to sign the approval: {source}")
            }
            Self::Failed {
                program,
                status,
                stderr,
            } => {
                let status = status.map_or_else(|| "a signal".to_owned(), |code| code.to_string());
                write!(f, "`{program}` did not sign the approval (exit {status})")?;
                if !stderr.is_empty() {
                    write!(f, ": {}", stderr.lines().collect::<Vec<_>>().join(" / "))?;
                }
                Ok(())
            }
            Self::NoSignature { program } => {
                write!(
                    f,
                    "`{program}` exited successfully and wrote no SSH signature"
                )
            }
            Self::TimedOut { program, seconds } => write!(
                f,
                "`{program}` did not sign the approval within {seconds} seconds, so it was \
                 stopped. Answer the key's prompt (a touch, a passphrase, a confirmation) when it \
                 appears, then approve again"
            ),
        }
    }
}

impl std::error::Error for SignError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Git(err) => Some(err),
            Self::TempKey { source, .. } | Self::CannotStart { source, .. } => Some(source),
            _ => None,
        }
    }
}
