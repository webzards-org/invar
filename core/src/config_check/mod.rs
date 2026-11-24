//! `invar config check`: whether a branch weakens what invar checks (#157,
//! `contracts/formats/invar-yaml.md` §3.9).
//!
//! `invar.yaml` decides whether the Plans, the locked tests and the required paths are
//! checked at all. A branch that removes the `acceptance:` section makes `acceptance
//! verify` warn and exit 0 (`acceptance-lock.md` D-21), so an agent could switch a check off
//! with one edit. `plan guard` denies that edit in the edit tools; this is the boundary,
//! in CI: `invar.yaml` at `HEAD` is compared with `invar.yaml` at the base, by the
//! contract's closed list, and a weakening passes only with an `Invar-Config: <reason>`
//! trailer a person wrote. It reads git and writes nothing.

use std::fmt::{self, Write as _};
use std::path::Path;

use crate::cycle::{self, CycleError, Skip, short};
use crate::format::config::{self, ConfigError, FILE_NAME, Weakening};
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
        if self.trailers.is_empty() {
            let _ = writeln!(out, "  none");
        }
        for trailer in &self.trailers {
            let _ = writeln!(
                out,
                "  {}: {CONFIG_TRAILER}: {}",
                short(&trailer.commit),
                trailer.reason
            );
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
            let _ = writeln!(
                out,
                "config check: passed: weakened with `{CONFIG_TRAILER}:` ({} trailer(s))",
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
        write!(
            f,
            "\nPut the keys back, or, if the developer means it, they add a trailer to a commit \
             of the branch: `git commit --amend --trailer \"{CONFIG_TRAILER}: <reason>\"`, or the \
             same on a new commit"
        )
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
    (report.trailers, report.empty_trailers) = cycle::trailers(dir, &base_commit, CONFIG_TRAILER)?;
    Ok(report)
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
