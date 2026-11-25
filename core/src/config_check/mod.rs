//! `invar config check`: whether a branch weakens what invar checks (#157,
//! `contracts/formats/invar-yaml.md` §3.9).
//!
//! `invar.yaml` decides whether the Plans, the locked tests and the required paths are
//! checked at all. A branch that removes the `acceptance:` section makes `acceptance
//! verify` warn and exit 0 (`acceptance-lock.md` D-21), so an agent could switch a check off
//! with one edit. `plan guard` denies that edit in the edit tools; this is the boundary,
//! in CI: `invar.yaml` at `HEAD` is compared with `invar.yaml` at the base, by the
//! contract's closed list, and a weakening passes only with an `Invar-Config: <reason>`
//! trailer a person wrote, signed by an approver when the base has `approval.signed`
//! (`approval-signature.md` §6.5). It reads git and writes nothing; `invar config
//! accept`, the developer's, signs a configuration change.

use std::fmt::{self, Write as _};
use std::path::Path;

use crate::approval_verify::APPROVERS;
use crate::attestation::{self, Judged, Kind};
use crate::cycle::{self, CycleError, Skip, short};
use crate::format::config::{self, ConfigError, FILE_NAME, Weakening};
use crate::fs::ProjectRoot;
use crate::git;

/// The trailer a person writes to say that a branch weakens the checks on purpose (§3.9).
pub const CONFIG_TRAILER: &str = "Invar-Config";

/// What `invar config check` found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub base: String,
    /// A `git.base_branch` that was not used, and one the branch changed: said, never used.
    pub notes: Vec<String>,
    /// Why the check is off, when it is.
    pub off: Option<String>,
    pub weakenings: Vec<Weakening>,
    /// Each `Invar-Config:` trailer with a reason.
    pub trailers: Vec<Skip>,
    /// The commits with an `Invar-Config:` trailer and no reason.
    pub empty_trailers: Vec<String>,
    /// Whether `approval.signed` is on at the base, so that a trailer counts only signed.
    pub signed: bool,
    /// The trailers that do not count, with why, when `signed`.
    pub not_counted: Vec<Judged>,
}

impl Report {
    pub fn passes(&self) -> bool {
        self.off.is_some() || self.weakenings.is_empty() || !self.trailers.is_empty()
    }

    /// Everything decided on, ending with `config check: off: …`, `config check: passed:
    /// …` or `config check: failed`.
    pub fn render(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "base: {}", self.base);
        for note in &self.notes {
            let _ = writeln!(out, "note: {note}");
        }
        if let Some(off) = &self.off {
            let _ = writeln!(out, "config check: off: {off}");
            return out;
        }
        let _ = writeln!(out, "weakenings (base -> this branch):");
        if self.weakenings.is_empty() {
            let _ = writeln!(out, "  none");
        }
        for weakening in &self.weakenings {
            let _ = writeln!(out, "  {weakening}");
        }
        let _ = writeln!(out, "trailers:");
        if self.trailers.is_empty() && self.not_counted.is_empty() {
            let _ = writeln!(out, "  none");
        }
        for trailer in &self.trailers {
            let signed = trailer.signed_by.as_ref().map_or_else(String::new, |by| {
                format!(" (signed by {}, {})", by.principal, by.fingerprint)
            });
            let _ = writeln!(
                out,
                "  {}: {CONFIG_TRAILER}: {}{signed}",
                short(&trailer.commit),
                trailer.reason
            );
        }
        for judged in &self.not_counted {
            if let Judged::NotCounted { commit, .. } = judged {
                let _ = writeln!(out, "  {}: {CONFIG_TRAILER}: {judged}", short(commit));
            }
        }
        for commit in &self.empty_trailers {
            let _ = writeln!(
                out,
                "  {}: `{CONFIG_TRAILER}:` with no reason, which does not count",
                short(commit)
            );
        }
        if self.weakenings.is_empty() {
            let _ = writeln!(out, "config check: passed: no check is weakened");
        } else if self.passes() {
            let signed = if self.signed { "signed " } else { "" };
            let _ = writeln!(
                out,
                "config check: passed: weakened with `{CONFIG_TRAILER}:` ({} {signed}trailer(s))",
                self.trailers.len()
            );
        } else {
            let _ = writeln!(out, "config check: failed");
        }
        out
    }
}

/// The refusal: every weakening, and the way out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Weakened {
    pub weakenings: Vec<Weakening>,
    /// Whether the trailer has to be signed.
    pub signed: bool,
}

impl fmt::Display for Weakened {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "this branch weakens {} check(s) in {FILE_NAME} against the base, with no \
             `{CONFIG_TRAILER}:` trailer:",
            self.weakenings.len()
        )?;
        for weakening in &self.weakenings {
            write!(f, "\n  {weakening}")?;
        }
        if self.signed {
            write!(
                f,
                "\nPut the keys back, or, if the developer means it, they sign it, which makes a \
                 commit with the trailers: `invar config accept --reason \"<reason>\"`, or \
                 /invar:config-accept <reason>"
            )
        } else {
            write!(
                f,
                "\nPut the keys back, or, if the developer means it, they add a trailer to a \
                 commit of the branch: `git commit --amend --trailer \"{CONFIG_TRAILER}: \
                 <reason>\"`, or the same on a new commit"
            )
        }
    }
}

impl std::error::Error for Weakened {}

/// Run the check in `dir`, the project root, against `base`, or the remote's default
/// branch, or `main` (§3.3.1). A directory and not a [`ProjectRoot`], because a branch that deletes
/// `invar.yaml` leaves a checkout with no project root, and that is a weakening to name.
pub fn check(dir: &Path, base: Option<&str>) -> Result<Report, ConfigCheckError> {
    let given = base;
    let (base, base_commit) = cycle::resolve_base(dir, given)?;
    let mut report = Report {
        base: base.clone(),
        notes: cycle::base_note(dir, given, &base).into_iter().collect(),
        off: None,
        weakenings: Vec::new(),
        trailers: Vec::new(),
        empty_trailers: Vec::new(),
        signed: false,
        not_counted: Vec::new(),
    };

    let at = |rev: &str| git::file_at(dir, rev, FILE_NAME).map_err(CycleError::Git);
    let Some(old) = at(&base_commit)? else {
        report.off = Some(format!(
            "{base} has no {FILE_NAME}, so there is nothing to weaken"
        ));
        return Ok(report);
    };
    // The history the trailers are read from; a shallow clone is refused here.
    cycle::merge_base(dir, &base)?;

    let base_shown = format!("{base}:{FILE_NAME}");
    let old = config::parse_bytes(&old, Path::new(&base_shown))
        .map_err(|source| ConfigCheckError::Config { source })?
        .config;
    // No file at `HEAD` is every key absent.
    let new = at("HEAD")?.unwrap_or_default();
    let new = config::parse_bytes(&new, Path::new(&format!("HEAD:{FILE_NAME}")))
        .map_err(|source| ConfigCheckError::Config { source })?
        .config;

    report.weakenings = config::weakenings(&old, &new);
    // Not a weakening (D-20): the CI commands do not read it. Said, so that a reviewer sees
    // a branch that points the local commands elsewhere.
    if old.git.base_branch != new.git.base_branch {
        let shown = |value: &Option<String>| value.clone().unwrap_or_else(|| "absent".to_owned());
        report.notes.push(format!(
            "this branch changes `git.base_branch`: {} -> {}. The CI checks do not read it; the \
             local commands (`plan check`, `done`, `review-input`) do",
            shown(&old.git.base_branch),
            shown(&new.git.base_branch)
        ));
    }
    let (trailers, empty_trailers) = cycle::trailers(dir, &base_commit, CONFIG_TRAILER)?;
    report.empty_trailers = empty_trailers;
    report.signed = old.approval.signed;
    if report.signed {
        let blobs = (
            git::blob_id_at(dir, &base_commit, FILE_NAME).map_err(CycleError::Git)?,
            git::blob_id_at(dir, "HEAD", FILE_NAME).map_err(CycleError::Git)?,
        );
        let approvers = cycle::base_approvers(dir, &base_commit)?;
        let list_shown = format!("{base}:{APPROVERS}");
        for found in attestation::read(dir, &base_commit, Kind::Config).map_err(CycleError::Git)? {
            let judged = attestation::judge(
                Kind::Config,
                &found,
                approvers.as_ref(),
                &list_shown,
                |reason| attestation::config_statement(reason, &blobs.0, &blobs.1),
            )
            .map_err(CycleError::SshKeygen)?;
            match judged {
                Judged::Counted {
                    commit,
                    reason,
                    verified,
                } => report.trailers.push(Skip {
                    commit,
                    reason,
                    signed_by: Some(verified),
                }),
                not_counted => report.not_counted.push(not_counted),
            }
        }
    } else {
        report.trailers = trailers;
    }
    Ok(report)
}

/// A configuration change ready to sign: what `invar config accept` shows, and the
/// statement it signs (`approval-signature.md` §6.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedAccept {
    pub base: String,
    pub reason: String,
    pub weakenings: Vec<Weakening>,
    pub statement: String,
}

/// Prepare a signed acceptance of what this branch weakens in `invar.yaml`, against
/// `base` found by the CI rule (§3.3.1): refused when the base does not ask for a
/// signature, or nothing is weakened.
pub fn prepare_accept(
    root: &ProjectRoot,
    base: Option<&str>,
    reason: &str,
) -> Result<PreparedAccept, ConfigCheckError> {
    let dir = root.path();
    let reason = attestation::reason(reason).map_err(CycleError::CannotSign)?;
    let (base, base_commit) = cycle::resolve_base(dir, base)?;
    let old = cycle::base_config(dir, &base, &base_commit)?;
    let Some(old) = old.filter(|config| config.approval.signed) else {
        return Err(CycleError::CannotSign(format!(
            "{base}:{FILE_NAME} does not set `approval.signed: true`, so a configuration \
             change is not signed: add the trailer to a commit of the branch, `git commit \
             --amend --trailer \"{CONFIG_TRAILER}: <reason>\"`"
        ))
        .into());
    };
    let new = git::file_at(dir, "HEAD", FILE_NAME)
        .map_err(CycleError::Git)?
        .unwrap_or_default();
    let new = config::parse_bytes(&new, Path::new(&format!("HEAD:{FILE_NAME}")))
        .map_err(|source| ConfigCheckError::Config { source })?
        .config;
    let weakenings = config::weakenings(&old, &new);
    if weakenings.is_empty() {
        return Err(CycleError::CannotSign(format!(
            "this branch weakens nothing in {FILE_NAME} against {base}, so there is nothing to \
             accept"
        ))
        .into());
    }
    let statement = attestation::config_statement(
        &reason,
        &git::blob_id_at(dir, &base_commit, FILE_NAME).map_err(CycleError::Git)?,
        &git::blob_id_at(dir, "HEAD", FILE_NAME).map_err(CycleError::Git)?,
    );
    Ok(PreparedAccept {
        base,
        reason,
        weakenings,
        statement,
    })
}

#[derive(Debug)]
pub enum ConfigCheckError {
    Cycle(CycleError),
    /// `invar.yaml` at the base or at `HEAD` is not a valid config.
    Config {
        source: ConfigError,
    },
}

impl From<CycleError> for ConfigCheckError {
    fn from(err: CycleError) -> Self {
        Self::Cycle(err)
    }
}

impl fmt::Display for ConfigCheckError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cycle(err) => err.fmt(f),
            Self::Config { source } => write!(f, "cannot compare the configuration: {source}"),
        }
    }
}

impl std::error::Error for ConfigCheckError {}
