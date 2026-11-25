//! A signed skip or configuration change: the trailers `Invar-Skip` and
//! `Invar-Config` with a signature beside them (`contracts/formats/approval-signature.md`
//! §6.5; section numbers below point into it).
//!
//! **Bound to what it covers** (D-9). A skip's statement lists each required change of
//! the branch with its blob at the merge base and at `HEAD`; a configuration change's,
//! `invar.yaml` at the base and at `HEAD`. Both survive `git rebase --signoff`, which
//! changes the merge base and HEAD's tree but not those blobs, and neither survives a
//! later change to what they cover.
//!
//! **A trailer, on one line** (D-10). The signature is `ssh-keygen`'s armor without its
//! first and last lines and without line breaks; [`signature::armored`] puts it back.
//! The checks build the statement from the branch as it is, so a signature counts only
//! for the change it was made for.

use std::fmt;
use std::io;
use std::path::Path;

use crate::git::{self, GitError};
use crate::signature::{self, Approvers, Refused, Verified};

/// Which decision a trailer records.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// `Invar-Skip`: required paths changed without a cycle (`invar-yaml.md` §3.8).
    Skip,
    /// `Invar-Config`: a change that weakens a check (`invar-yaml.md` §3.9).
    Config,
}

impl Kind {
    /// The SSH signature namespace (D-9): never valid as another kind, or as an
    /// approval.
    pub fn namespace(self) -> &'static str {
        match self {
            Self::Skip => "invar-skip",
            Self::Config => "invar-config",
        }
    }

    /// The trailer that holds the reason.
    pub fn reason_key(self) -> &'static str {
        match self {
            Self::Skip => "Invar-Skip",
            Self::Config => "Invar-Config",
        }
    }

    /// The trailer that holds the signature, beside it.
    pub fn signature_key(self) -> &'static str {
        match self {
            Self::Skip => "Invar-Skip-Signature",
            Self::Config => "Invar-Config-Signature",
        }
    }

    fn first_line(self) -> &'static str {
        match self {
            Self::Skip => "invar-skip 1",
            Self::Config => "invar-config 1",
        }
    }
}

/// One required change: a path and its blob at the merge base and at `HEAD`, `None`
/// where it is absent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub before: Option<String>,
    pub after: Option<String>,
    pub path: String,
}

/// `-` for an absent file (§6.5).
fn blob(id: &Option<String>) -> &str {
    id.as_deref().unwrap_or("-")
}

/// What a skip signs (§6.5): its reason and every required change, sorted by path.
pub fn skip_statement(reason: &str, changes: &[Change]) -> String {
    let mut sorted: Vec<&Change> = changes.iter().collect();
    sorted.sort_by(|a, b| a.path.as_bytes().cmp(b.path.as_bytes()));
    let mut out = format!("{}\nreason: {reason}\n", Kind::Skip.first_line());
    for change in sorted {
        out.push_str(&format!(
            "change: {} {} {}\n",
            blob(&change.before),
            blob(&change.after),
            change.path
        ));
    }
    out
}

/// What a configuration change signs (§6.5): its reason, and `invar.yaml` at the base
/// and at `HEAD`.
pub fn config_statement(reason: &str, base: &Option<String>, head: &Option<String>) -> String {
    format!(
        "{}\nreason: {reason}\nconfig: {} {}\n",
        Kind::Config.first_line(),
        blob(base),
        blob(head)
    )
}

/// Each of `paths` with its blob at `fork` and at `HEAD`.
pub fn changes(dir: &Path, fork: &str, paths: &[String]) -> Result<Vec<Change>, GitError> {
    paths
        .iter()
        .map(|path| {
            Ok(Change {
                before: git::blob_id_at(dir, fork, path)?,
                after: git::blob_id_at(dir, "HEAD", path)?,
                path: path.clone(),
            })
        })
        .collect()
}

/// A reason as a trailer keeps it: trimmed, one line, not empty.
pub fn reason(given: &str) -> Result<String, String> {
    let reason = given.trim();
    if reason.is_empty() {
        return Err("the reason is empty: say why, in one line".to_owned());
    }
    if reason.contains(['\n', '\r']) {
        return Err("the reason is more than one line: a trailer holds one".to_owned());
    }
    Ok(reason.to_owned())
}

/// The trailers of one kind on one commit of the branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trailers {
    pub commit: String,
    /// The reasons, trimmed, empty ones left out.
    pub reasons: Vec<String>,
    pub signatures: Vec<String>,
}

/// Every commit of `from..HEAD` with a reason or a signature trailer of `kind`, newest
/// first.
pub fn read(dir: &Path, from: &str, kind: Kind) -> Result<Vec<Trailers>, GitError> {
    let reasons = git::trailers(dir, from, kind.reason_key())?;
    let signatures = git::trailers(dir, from, kind.signature_key())?;
    let mut found = Vec::new();
    for ((commit, reasons), (_, signatures)) in reasons.into_iter().zip(signatures) {
        let reasons: Vec<String> = reasons
            .into_iter()
            .map(|reason| reason.trim().to_owned())
            .filter(|reason| !reason.is_empty())
            .collect();
        let signatures: Vec<String> = signatures
            .into_iter()
            .map(|signature| signature.trim().to_owned())
            .filter(|signature| !signature.is_empty())
            .collect();
        if !reasons.is_empty() || !signatures.is_empty() {
            found.push(Trailers {
                commit,
                reasons,
                signatures,
            });
        }
    }
    Ok(found)
}

/// What a commit's trailers count for, when `approval.signed` is on at the base.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Judged {
    /// Signed by an approver, over the branch as it is.
    Counted {
        commit: String,
        reason: String,
        verified: Verified,
    },
    /// Named, with why, and not counted.
    NotCounted {
        commit: String,
        reason: Option<String>,
        why: String,
    },
}

/// Why one commit's trailers do not count, or who signed them (§6.5, "Verifying").
/// `approvers` is `None` when the base has no `.invar/approvers`; `statement` builds the
/// statement from a reason, out of the branch as it is. The outer error is `ssh-keygen`
/// that could not be run.
pub fn judge(
    kind: Kind,
    trailers: &Trailers,
    approvers: Option<&Approvers>,
    list_shown: &str,
    statement: impl Fn(&str) -> String,
) -> io::Result<Judged> {
    let not_counted = |reason: Option<&String>, why: String| Judged::NotCounted {
        commit: trailers.commit.clone(),
        reason: reason.cloned(),
        why,
    };
    let (reason_key, signature_key) = (kind.reason_key(), kind.signature_key());
    let ([reason], [signature]) = (trailers.reasons.as_slice(), trailers.signatures.as_slice())
    else {
        let reason = trailers.reasons.first();
        let why = match (trailers.reasons.len(), trailers.signatures.len()) {
            (_, 0) => format!(
                "not signed: `approval.signed` is on at the base, so `{reason_key}:` counts only \
                 with an `{signature_key}:` beside it"
            ),
            (0, _) => format!("`{signature_key}:` with no `{reason_key}:` beside it"),
            _ => format!(
                "a commit holds one `{reason_key}:` and one `{signature_key}:`, and this one has \
                 {} and {}",
                trailers.reasons.len(),
                trailers.signatures.len()
            ),
        };
        return Ok(not_counted(reason, why));
    };
    let Some(approvers) = approvers else {
        return Ok(not_counted(
            Some(reason),
            format!("{list_shown} does not exist, so no signature can be verified"),
        ));
    };
    let armored = signature::armored(signature);
    Ok(
        match approvers.verify_in(kind.namespace(), &statement(reason), armored.as_bytes())? {
            Ok(verified) => Judged::Counted {
                commit: trailers.commit.clone(),
                reason: reason.clone(),
                verified,
            },
            Err(Refused::DoesNotVerify { said }) => not_counted(
                Some(reason),
                format!(
                    "the signature is not over the change as it is now ({said}): it counts only \
                     for the change it was made for, so a later change to what it covers needs \
                     a new one"
                ),
            ),
            Err(refused) => not_counted(Some(reason), format!("{refused} ({list_shown})")),
        },
    )
}

impl fmt::Display for Judged {
    /// One line for the report, after the commit's short name.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Counted {
                reason, verified, ..
            } => write!(
                f,
                "{reason} (signed by {}, {})",
                verified.principal, verified.fingerprint
            ),
            Self::NotCounted { reason, why, .. } => match reason {
                Some(reason) => write!(f, "{reason}: does not count: {why}"),
                None => write!(f, "does not count: {why}"),
            },
        }
    }
}
