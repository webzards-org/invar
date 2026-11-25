//! Checking a signed approval against a list of approvers (`approval-signature.md`
//! §6.3, rule 1, and §6.4).
//!
//! **`ssh-keygen` directly** (D-5), never `git verify-commit` or `%G?`, and never
//! `gpg.ssh.program`: those come from a repository's config, which the agent can
//! change on its own machine. Three calls, each with the statement on standard
//! input:
//!
//! 1. `-Y check-novalidate -n invar-approval` — is this a signature over this
//!    statement, in this namespace, by any key? Its answer names the key's
//!    fingerprint, which a refusal for an unlisted key reports.
//! 2. `-Y find-principals -f <list>` — which principals in the list have that key.
//!    It does not read `namespaces=`, so it is not the verdict.
//! 3. `-Y verify -f <list> -I <principal> -n invar-approval` — the verdict, for one
//!    principal, with every restriction of its line.

use std::fmt;
use std::io::{self, Write as _};
use std::process::{Command, Output, Stdio};

use super::{NAMESPACE, TempFile};

/// The approvers list, `<base>:.invar/approvers`, in a temporary file for
/// `ssh-keygen` (§2). Never the work tree's copy.
pub struct Approvers {
    file: TempFile,
}

/// Who a signature that verified is from (§6.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verified {
    /// The principal in the list whose line the signature verified against.
    pub principal: String,
    /// `SHA256:…`, as `ssh-keygen` prints it.
    pub fingerprint: String,
}

/// Why a signature does not make an approval.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refused {
    /// Not a signature over this statement in the namespace `invar-approval`, or
    /// not a signature at all: `ssh-keygen`'s words.
    DoesNotVerify { said: String },
    /// A good signature by a key that no line of the list has.
    NotListed { fingerprint: String },
    /// The key is in the list, and no line of it accepts the signature, for
    /// instance a `namespaces=` that leaves out `invar-approval`.
    NotAccepted {
        fingerprint: String,
        principals: Vec<String>,
        said: String,
    },
}

impl fmt::Display for Refused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DoesNotVerify { said } => write!(
                f,
                "the signature in approval.sig is not over this approval ({said})"
            ),
            Self::NotListed { fingerprint } => {
                write!(
                    f,
                    "signed by a key that is not in the approvers list ({fingerprint})"
                )
            }
            Self::NotAccepted {
                fingerprint,
                principals,
                said,
            } => write!(
                f,
                "signed by {fingerprint}, which the approvers list has for {}, and its line \
                 does not accept this signature ({said})",
                principals.join(", ")
            ),
        }
    }
}

impl Approvers {
    /// Write `list` to a temporary file. The path is given back with the error.
    pub fn write(list: &[u8]) -> Result<Self, (std::path::PathBuf, io::Error)> {
        Ok(Self {
            file: TempFile::write("approvers", "txt", list)?,
        })
    }

    /// Check `signature`, the bytes of an `approval.sig`, over `statement` (§3).
    /// The outer error is `ssh-keygen` that could not be run at all.
    pub fn verify(
        &self,
        statement: &str,
        signature: &[u8],
    ) -> io::Result<Result<Verified, Refused>> {
        self.verify_in(NAMESPACE, statement, signature)
    }

    /// [`verify`](Self::verify) in `namespace`: `invar-skip` or `invar-config` for
    /// the trailers of §6.5.
    pub fn verify_in(
        &self,
        namespace: &str,
        statement: &str,
        signature: &[u8],
    ) -> io::Result<Result<Verified, Refused>> {
        let signature =
            TempFile::write("approval", "sig", signature).map_err(|(_, source)| source)?;
        let sig = signature.path().as_os_str();

        let checked = ssh_keygen(
            &[
                "-Y".as_ref(),
                "check-novalidate".as_ref(),
                "-n".as_ref(),
                namespace.as_ref(),
                "-s".as_ref(),
                sig,
            ],
            statement,
        )?;
        if !checked.status.success() {
            return Ok(Err(Refused::DoesNotVerify {
                said: said(&checked),
            }));
        }
        let fingerprint = fingerprint(&checked).unwrap_or_else(|| "an unknown key".to_owned());

        let found = ssh_keygen(
            &[
                "-Y".as_ref(),
                "find-principals".as_ref(),
                "-s".as_ref(),
                sig,
                "-f".as_ref(),
                self.file.path().as_os_str(),
            ],
            "",
        )?;
        let principals: Vec<String> = if found.status.success() {
            String::from_utf8_lossy(&found.stdout)
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(str::to_owned)
                .collect()
        } else {
            Vec::new()
        };
        if principals.is_empty() {
            return Ok(Err(Refused::NotListed { fingerprint }));
        }

        let mut words = Vec::new();
        for principal in &principals {
            let verified = ssh_keygen(
                &[
                    "-Y".as_ref(),
                    "verify".as_ref(),
                    "-f".as_ref(),
                    self.file.path().as_os_str(),
                    "-I".as_ref(),
                    principal.as_ref(),
                    "-n".as_ref(),
                    namespace.as_ref(),
                    "-s".as_ref(),
                    sig,
                ],
                statement,
            )?;
            if verified.status.success() {
                return Ok(Ok(Verified {
                    principal: principal.clone(),
                    fingerprint,
                }));
            }
            words.push(said(&verified));
        }
        Ok(Err(Refused::NotAccepted {
            fingerprint,
            principals,
            said: words.join("; "),
        }))
    }
}

/// Run `ssh-keygen` with `args`, and `input` on standard input.
fn ssh_keygen(args: &[&std::ffi::OsStr], input: &str) -> io::Result<Output> {
    let mut child = Command::new("ssh-keygen")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    if let Some(mut stdin) = child.stdin.take() {
        // A program that exits without reading its input is judged by its status.
        let _ = stdin.write_all(input.as_bytes());
    }
    child.wait_with_output()
}

/// What `ssh-keygen` said, on one line, without the closing line every refusal
/// repeats and without the temporary file's name.
fn said(output: &Output) -> String {
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && *line != "Could not verify signature.")
        .map(|line| match line.split_once(".txt:") {
            // `<list file>:<n>: …` names a file nobody else can see.
            Some((_, rest)) => rest.split_once(": ").map_or(rest, |(_, said)| said),
            None => line,
        })
        .collect();
    if lines.is_empty() {
        format!("ssh-keygen exited with {}", output.status)
    } else {
        lines.join(" / ")
    }
}

/// The `SHA256:…` in a `Good "…" signature … with <type> key SHA256:…` line.
fn fingerprint(output: &Output) -> Option<String> {
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    text.split_whitespace()
        .find(|word| word.starts_with("SHA256:"))
        .map(str::to_owned)
}
