//! `invar cycle`: whether a branch that changes declared paths went through a cycle
//! (#155, `contracts/formats/invar-yaml.md` §3.8).
//!
//! The main session decides whether a change needs a Plan (L-9), and it may skip the
//! cycle for trivial work. Nothing in the session can be the check of that decision,
//! because the session is what the check would constrain. So the check is in CI, on
//! the history git keeps: a branch that changes a path the base branch's
//! `cycle.required_paths` names passes only when every commit of it that touched the
//! path was made while the Plan of a cycle closed on the branch was approved (D-17), or
//! with an `Invar-Skip: <reason>` trailer a person wrote. With `approval.signed` on at the
//! base, the trailer counts only signed by an approver (`approval-signature.md` §6.5). It
//! reads git and writes nothing; `invar cycle skip`, the developer's, signs a skip.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{self, Write as _};
use std::path::Path;

use crate::approval_verify::APPROVERS;
use crate::attestation::{self, Change, Judged, Kind};
use crate::format::config::{self, ConfigError, FILE_NAME};
use crate::format::glob;
use crate::format::plan::Mode;
use crate::fs::ProjectRoot;
use crate::fs::plans::{SPEC_FILE, parse_dir_name};
use crate::gate::check;
use crate::git::{self, GitError};
use crate::signature::{Approvers, Verified};

#[cfg(test)]
mod tests;

/// The trailer a person writes to say that a branch skips the cycle, and why (§3.8).
pub const SKIP_TRAILER: &str = "Invar-Skip";

/// Where the Plans are, relative to the project root.
const PLANS: &str = ".invar/plans";

/// Left out of the changes: the Plans and the decision log are the cycle's own files.
const INVAR_DIR: &str = ".invar";

/// What `invar cycle check` found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// The base, as given or found.
    pub base: String,
    /// What was said about the base: a `git.base_branch` the check did not use.
    pub notes: Vec<String>,
    /// The base's `cycle.required_paths`. Empty: the check is off.
    pub patterns: Vec<String>,
    /// Why the check is off, when it is.
    pub off: Option<String>,
    /// How the branch changes `cycle.required_paths`, when it does: said, never used.
    pub list_change: Option<ListChange>,
    /// The merge base, when the check ran.
    pub fork: Option<String>,
    /// Each changed path a pattern matches, and whether the cycles closed on the branch
    /// cover it.
    pub required: Vec<Required>,
    pub closed: Vec<Closed>,
    pub open: Vec<Open>,
    /// Each `Invar-Skip:` trailer that counts: with a reason, and signed when `signed`.
    pub skips: Vec<Skip>,
    /// The commits with an `Invar-Skip:` trailer and no reason: they count for nothing.
    pub empty_skips: Vec<String>,
    /// Whether `approval.signed` is on at the base, so that a skip counts only signed.
    pub signed: bool,
    /// The trailers that do not count, with why, when `signed`.
    pub not_counted: Vec<Judged>,
}

/// A path in the branch's net change that a pattern of the base's list matches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Required {
    pub path: String,
    /// The first pattern that matches it.
    pub pattern: String,
    /// The tickets whose approved Plan was at the parent of a commit that touched it,
    /// sorted. Meaningful only when `uncovered` is empty.
    pub covered_by: Vec<String>,
    /// The commits of the branch that touched it with no closed cycle's approval holding
    /// at their parent, newest first: each its full name and its subject.
    pub uncovered: Vec<(String, String)>,
}

impl Required {
    /// Every commit of the branch that touched it was covered (D-17).
    pub fn is_covered(&self) -> bool {
        self.uncovered.is_empty() && !self.covered_by.is_empty()
    }
}

/// A cycle closed on the branch: its Plan was approved in some commit of it, and is gone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Closed {
    pub ticket: String,
    /// The Plan's `spec.md`, relative to the project root.
    pub spec: String,
    /// The commit that deleted it.
    pub deleted_in: String,
}

/// A Plan the branch added or changed that is still there at `HEAD`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Open {
    pub ticket: String,
    pub spec: String,
}

/// A trailer with a reason, and the commit it is on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skip {
    pub commit: String,
    pub reason: String,
    /// Who signed it, when the base asks for a signature.
    pub signed_by: Option<Verified>,
}

/// The patterns a branch adds to or removes from the base's list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListChange {
    pub added: Vec<String>,
    pub removed: Vec<String>,
}

impl Report {
    /// The check holds: off, nothing required changed, every required change covered by
    /// a closed cycle, or a skip.
    pub fn passes(&self) -> bool {
        self.off.is_some()
            || self.required.iter().all(Required::is_covered)
            || !self.skips.is_empty()
    }

    /// Why it holds, for the last line.
    fn why(&self) -> String {
        if let Some(off) = &self.off {
            return off.clone();
        }
        if self.required.is_empty() {
            return "no required path changed".to_owned();
        }
        if self.required.iter().all(Required::is_covered) {
            return "every required change is covered by a closed cycle".to_owned();
        }
        let signed = if self.signed { "signed " } else { "" };
        format!(
            "skipped by `{SKIP_TRAILER}:` ({} {signed}trailer(s))",
            self.skips.len()
        )
    }

    /// Everything the check decided on, ending with one line: `cycle check: off: …`,
    /// `cycle check: passed: …` or `cycle check: failed`.
    pub fn render(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "base: {}", self.base);
        for note in &self.notes {
            let _ = writeln!(out, "note: {note}");
        }
        if let Some(change) = &self.list_change {
            let _ = writeln!(
                out,
                "note: this branch changes `cycle.required_paths`, and the check uses the base's \
                 list; the change applies once it is on the base"
            );
            for pattern in &change.added {
                let _ = writeln!(out, "  added on the branch: {pattern}");
            }
            for pattern in &change.removed {
                let _ = writeln!(out, "  removed on the branch: {pattern}");
            }
        }
        if self.off.is_some() {
            let _ = writeln!(out, "cycle check: off: {}", self.why());
            return out;
        }
        if self.signed && !self.required.is_empty() {
            let _ = writeln!(
                out,
                "note: `approval.signed` is on at {}, so `{SKIP_TRAILER}:` counts only signed by \
                 an approver: `invar cycle skip --reason \"<reason>\"`",
                self.base
            );
        }
        if let Some(fork) = &self.fork {
            let _ = writeln!(out, "merge base: {}", short(fork));
        }
        let _ = writeln!(out, "required paths: {}", self.patterns.join(", "));

        let _ = writeln!(out, "required changes:");
        if self.required.is_empty() {
            let _ = writeln!(out, "  none");
        }
        for required in &self.required {
            let (path, pattern) = (&required.path, &required.pattern);
            if required.is_covered() {
                let _ = writeln!(
                    out,
                    "  {path} ({pattern}): covered by {}",
                    required.covered_by.join(", ")
                );
            } else {
                let _ = writeln!(
                    out,
                    "  {path} ({pattern}): not covered: {}",
                    commits(required)
                );
            }
        }
        let _ = writeln!(out, "closed cycles:");
        if self.closed.is_empty() {
            let _ = writeln!(out, "  none");
        }
        for closed in &self.closed {
            let _ = writeln!(
                out,
                "  {}: {} deleted in {}",
                closed.ticket,
                closed.spec,
                short(&closed.deleted_in)
            );
        }
        for open in &self.open {
            let _ = writeln!(
                out,
                "open cycle: the cycle of {} is not closed, {} is still there: run /invar:done \
                 {}",
                open.ticket, open.spec, open.ticket
            );
        }
        let _ = writeln!(out, "skips:");
        if self.skips.is_empty() && self.not_counted.is_empty() {
            let _ = writeln!(out, "  none");
        }
        for skip in &self.skips {
            let signed = skip.signed_by.as_ref().map_or_else(String::new, |by| {
                format!(" (signed by {}, {})", by.principal, by.fingerprint)
            });
            let _ = writeln!(
                out,
                "  {}: {SKIP_TRAILER}: {}{signed}",
                short(&skip.commit),
                skip.reason
            );
        }
        for judged in &self.not_counted {
            if let Judged::NotCounted { commit, .. } = judged {
                let _ = writeln!(out, "  {}: {SKIP_TRAILER}: {judged}", short(commit));
            }
        }
        for commit in &self.empty_skips {
            let _ = writeln!(
                out,
                "  {}: `{SKIP_TRAILER}:` with no reason, which does not count",
                short(commit)
            );
        }
        if self.passes() {
            let _ = writeln!(out, "cycle check: passed: {}", self.why());
        } else {
            let _ = writeln!(out, "cycle check: failed");
        }
        out
    }
}

/// `<sha> <subject>, …` of the commits that touched a path uncovered.
fn commits(required: &Required) -> String {
    if required.uncovered.is_empty() {
        return "no commit of the branch touches it".to_owned();
    }
    required
        .uncovered
        .iter()
        .map(|(commit, subject)| format!("{} {subject}", short(commit)))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The first 12 characters of a commit's name.
pub fn short(commit: &str) -> String {
    commit.chars().take(12).collect()
}

/// The refusal: every required change no closed cycle covers, and the two ways out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotCovered {
    /// Only the required changes that are not covered.
    pub required: Vec<Required>,
    /// Whether a skip has to be signed.
    pub signed: bool,
}

impl NotCovered {
    pub fn of(report: &Report) -> Self {
        Self {
            signed: report.signed,
            required: report
                .required
                .iter()
                .filter(|required| !required.is_covered())
                .cloned()
                .collect(),
        }
    }
}

impl fmt::Display for NotCovered {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "this branch changes {} required path(s) that no closed cycle covers, and has no \
             `{SKIP_TRAILER}:` trailer:",
            self.required.len()
        )?;
        for required in &self.required {
            write!(
                f,
                "\n  {} (matches {}), touched outside a cycle by: {}",
                required.path,
                required.pattern,
                commits(required)
            )?;
        }
        write!(
            f,
            "\nA commit is covered when, at its parent, the Plan of a cycle closed on this branch \
             is approved. Either make the change while a Plan is approved and close the cycle \
             with /invar:done; or, if it needs no Plan, "
        )?;
        if self.signed {
            write!(
                f,
                "the developer signs a skip, which makes a commit with the trailers: `invar \
                 cycle skip --reason \"<reason>\"`, or /invar:skip <reason>"
            )
        } else {
            write!(
                f,
                "the developer adds a trailer to a commit of the branch: `git commit --amend \
                 --trailer \"{SKIP_TRAILER}: <reason>\"`, or the same on a new commit"
            )
        }
    }
}

impl std::error::Error for NotCovered {}

/// Run the check against `base`, or the remote's default branch, or `main` (§3.3.1).
pub fn check(root: &ProjectRoot, base: Option<&str>) -> Result<Report, CycleError> {
    let dir = root.path();
    let given = base;
    let (base, base_commit) = resolve_base(dir, given)?;

    let base_config = base_config(dir, &base, &base_commit)?;
    let signed = base_config
        .as_ref()
        .is_some_and(|config| config.approval.signed);
    let base_list = base_config.map(|config| config.cycle.required_paths);
    let head_list = git::file_at(dir, "HEAD", FILE_NAME)
        .ok()
        .flatten()
        .and_then(|bytes| config::parse_bytes(&bytes, Path::new(FILE_NAME)).ok())
        .map(|loaded| loaded.config.cycle.required_paths)
        .unwrap_or_default();

    let patterns = base_list.clone().unwrap_or_default();
    let list_change = {
        let added: Vec<String> = head_list
            .iter()
            .filter(|p| !patterns.contains(p))
            .cloned()
            .collect();
        let removed: Vec<String> = patterns
            .iter()
            .filter(|p| !head_list.contains(p))
            .cloned()
            .collect();
        (!added.is_empty() || !removed.is_empty()).then_some(ListChange { added, removed })
    };

    let mut report = Report {
        base: base.clone(),
        notes: base_note(dir, given, &base).into_iter().collect(),
        patterns: patterns.clone(),
        off: None,
        list_change,
        fork: None,
        required: Vec::new(),
        closed: Vec::new(),
        open: Vec::new(),
        skips: Vec::new(),
        empty_skips: Vec::new(),
        signed,
        not_counted: Vec::new(),
    };
    match &base_list {
        None => {
            report.off = Some(format!("{base} has no {FILE_NAME}"));
            return Ok(report);
        }
        Some(list) if list.is_empty() => {
            report.off = Some(format!(
                "{base}:{FILE_NAME} has no `cycle.required_paths`, so no path needs a cycle"
            ));
            return Ok(report);
        }
        Some(_) => {}
    }

    let fork = merge_base(dir, &base)?;

    // The branch's commits are `<base>..HEAD`: what it merged in from the base is the
    // base's, not the branch's (D-17). The net change is still taken from the merge base.
    cycles(root, &base_commit, &mut report)?;

    let mut covering = Covering::default();
    for (path, pattern) in required_changes(dir, &fork, &patterns)? {
        let mut required = Required {
            path: path.clone(),
            pattern,
            covered_by: Vec::new(),
            uncovered: Vec::new(),
        };
        let mut by = BTreeSet::new();
        for commit in git::commits_touching(dir, &base_commit, &path).map_err(CycleError::Git)? {
            let tickets = covering.of(dir, &commit, &report.closed)?;
            if tickets.is_empty() {
                let subject = git::subject(dir, &commit).map_err(CycleError::Git)?;
                required.uncovered.push((commit, subject));
            } else {
                by.extend(tickets.iter().cloned());
            }
        }
        required.covered_by = by.into_iter().collect();
        report.required.push(required);
    }

    let (skips, empty_skips) = trailers(dir, &base_commit, SKIP_TRAILER)?;
    report.empty_skips = empty_skips;
    if signed {
        let paths: Vec<String> = report.required.iter().map(|r| r.path.clone()).collect();
        let changes = attestation::changes(dir, &fork, &paths).map_err(CycleError::Git)?;
        let approvers = base_approvers(dir, &base_commit)?;
        let list_shown = format!("{base}:{APPROVERS}");
        for trailers in attestation::read(dir, &base_commit, Kind::Skip).map_err(CycleError::Git)? {
            let judged = attestation::judge(
                Kind::Skip,
                &trailers,
                approvers.as_ref(),
                &list_shown,
                |reason| attestation::skip_statement(reason, &changes),
            )
            .map_err(CycleError::SshKeygen)?;
            match judged {
                Judged::Counted {
                    commit,
                    reason,
                    verified,
                } => report.skips.push(Skip {
                    commit,
                    reason,
                    signed_by: Some(verified),
                }),
                not_counted => report.not_counted.push(not_counted),
            }
        }
    } else {
        report.skips = skips;
    }
    report.fork = Some(fork);
    Ok(report)
}

/// `invar.yaml` at the base, when it has one.
pub fn base_config(
    dir: &Path,
    base: &str,
    base_commit: &str,
) -> Result<Option<config::Config>, CycleError> {
    match git::file_at(dir, base_commit, FILE_NAME).map_err(CycleError::Git)? {
        None => Ok(None),
        Some(bytes) => {
            let shown = format!("{base}:{FILE_NAME}");
            let loaded = config::parse_bytes(&bytes, Path::new(&shown))
                .map_err(|source| CycleError::BaseConfig { source })?;
            Ok(Some(loaded.config))
        }
    }
}

/// `<base>:.invar/approvers` in a temporary file for `ssh-keygen`, when the base has
/// one (`approval-signature.md` §2).
pub fn base_approvers(dir: &Path, base_commit: &str) -> Result<Option<Approvers>, CycleError> {
    let Some(list) = git::file_at(dir, base_commit, APPROVERS).map_err(CycleError::Git)? else {
        return Ok(None);
    };
    Approvers::write(&list)
        .map(Some)
        .map_err(|(_, source)| CycleError::SshKeygen(source))
}

/// The branch's required changes: every path that differs between `fork` and `HEAD`,
/// committed, `.invar/` left out, that one of `patterns` matches, with the first pattern
/// that does.
pub fn required_changes(
    dir: &Path,
    fork: &str,
    patterns: &[String],
) -> Result<Vec<(String, String)>, CycleError> {
    let mut found = Vec::new();
    for path in git::committed_changes(dir, fork).map_err(CycleError::Git)? {
        if path == INVAR_DIR || path.starts_with(&format!("{INVAR_DIR}/")) {
            continue;
        }
        if let Some(pattern) = glob::first_match(patterns, &path) {
            found.push((path.clone(), pattern.to_owned()));
        }
    }
    Ok(found)
}

/// A skip ready to sign: what `invar cycle skip` shows, and the statement it signs
/// (`approval-signature.md` §6.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedSkip {
    pub base: String,
    pub reason: String,
    /// Each required change, with the pattern that requires it.
    pub changes: Vec<(Change, String)>,
    pub statement: String,
}

/// Prepare a signed skip of the branch's required changes, against `base` found by the
/// CI rule (§3.3.1): refused when the base does not ask for signed skips, when nothing
/// is required, or when a path cannot be written into the statement.
pub fn prepare_skip(
    root: &ProjectRoot,
    base: Option<&str>,
    reason: &str,
) -> Result<PreparedSkip, CycleError> {
    let dir = root.path();
    let reason = attestation::reason(reason).map_err(CycleError::CannotSign)?;
    let (base, base_commit) = resolve_base(dir, base)?;
    let config = base_config(dir, &base, &base_commit)?;
    if !config.as_ref().is_some_and(|config| config.approval.signed) {
        return Err(CycleError::CannotSign(format!(
            "{base}:{FILE_NAME} does not set `approval.signed: true`, so a skip is not signed: \
             add the trailer to a commit of the branch, `git commit --amend --trailer \
             \"{SKIP_TRAILER}: <reason>\"`"
        )));
    }
    let patterns = config
        .map(|config| config.cycle.required_paths)
        .unwrap_or_default();
    let fork = merge_base(dir, &base)?;
    let required = required_changes(dir, &fork, &patterns)?;
    if required.is_empty() {
        return Err(CycleError::CannotSign(format!(
            "this branch changes no path that {base}:{FILE_NAME} requires a cycle for, so there \
             is nothing to skip"
        )));
    }
    if let Some((path, _)) = required
        .iter()
        .find(|(path, _)| path.contains(['\n', '\r']))
    {
        return Err(CycleError::CannotSign(format!(
            "the path {path:?} holds a line break, and cannot be written into the statement"
        )));
    }
    let paths: Vec<String> = required.iter().map(|(path, _)| path.clone()).collect();
    let blobs = attestation::changes(dir, &fork, &paths).map_err(CycleError::Git)?;
    let statement = attestation::skip_statement(&reason, &blobs);
    Ok(PreparedSkip {
        base,
        reason,
        changes: blobs
            .into_iter()
            .zip(required.into_iter().map(|(_, pattern)| pattern))
            .collect(),
        statement,
    })
}

/// The base a CI command compares a branch with, and its commit: `--base`, else the
/// remote's default branch (`origin/HEAD`), else `main` (`invar-yaml.md` §3.3.1, D-20).
/// Never `git.base_branch`, which the branch being checked could set to itself. In a
/// shallow clone that does not have it, the error says how to fetch it.
pub fn resolve_base(dir: &Path, base: Option<&str>) -> Result<(String, String), CycleError> {
    let base = ci_base(dir, base).map_err(CycleError::Git)?;
    let commit = git::commit_of(dir, &base).map_err(|err| shallow_or(dir, err, &base))?;
    Ok((base, commit))
}

/// The base of [`resolve_base`], without its commit: for `acceptance verify`, whose branch
/// rule resolves the commit itself.
pub fn ci_base(dir: &Path, base: Option<&str>) -> Result<String, GitError> {
    match base {
        Some(base) => Ok(base.to_owned()),
        None => git::base_branch(dir, None),
    }
}

/// What to say when no `--base` was given and `invar.yaml` at `HEAD` names another
/// `git.base_branch` than the base that was used: it was not used, and why (D-20).
pub fn base_note(dir: &Path, given: Option<&str>, used: &str) -> Option<String> {
    if given.is_some() {
        return None;
    }
    let bytes = git::file_at(dir, "HEAD", FILE_NAME).ok().flatten()?;
    let named = config::parse_bytes(&bytes, Path::new(FILE_NAME))
        .ok()?
        .config
        .git
        .base_branch?;
    (named != used).then(|| {
        format!(
            "{FILE_NAME} on this branch sets `git.base_branch: {named}`, and the base used is \
             `{used}`, the remote's default branch or `main`: a branch does not choose what it \
             is checked against. If pull requests target `{named}`, give `--base origin/{named}`"
        )
    })
}

/// The merge base of `base` and `HEAD`, with the advice of [`resolve_base`].
pub fn merge_base(dir: &Path, base: &str) -> Result<String, CycleError> {
    git::merge_base(dir, base).map_err(|err| shallow_or(dir, err, base))
}

fn shallow_or(dir: &Path, err: GitError, base: &str) -> CycleError {
    if matches!(git::is_shallow(dir), Ok(true)) {
        CycleError::Shallow {
            base: base.to_owned(),
            said: err.to_string(),
        }
    } else {
        CycleError::Git(err)
    }
}

/// The trailers `key` on the commits `from..HEAD`: each one with a reason, and the commits
/// whose trailer had none, which count for nothing. For `Invar-Skip` and `Invar-Config`.
pub fn trailers(dir: &Path, from: &str, key: &str) -> Result<(Vec<Skip>, Vec<String>), CycleError> {
    let mut with_reason = Vec::new();
    let mut empty = Vec::new();
    for (commit, values) in git::trailers(dir, from, key).map_err(CycleError::Git)? {
        let had_trailer = !values.is_empty();
        let mut counted = false;
        for value in values {
            let reason = value.trim();
            if !reason.is_empty() {
                with_reason.push(Skip {
                    commit: commit.clone(),
                    reason: reason.to_owned(),
                    signed_by: None,
                });
                counted = true;
            }
        }
        if !counted && had_trailer {
            empty.push(commit);
        }
    }
    Ok((with_reason, empty))
}

/// Which closed cycles cover each commit, asked once per commit.
#[derive(Default)]
struct Covering {
    known: BTreeMap<String, Vec<String>>,
}

impl Covering {
    /// The tickets of `closed` whose Plan is at `commit`'s first parent with an approval
    /// that holds there (D-17). A root commit has none.
    fn of(
        &mut self,
        dir: &Path,
        commit: &str,
        closed: &[Closed],
    ) -> Result<&Vec<String>, CycleError> {
        if !self.known.contains_key(commit) {
            let mut tickets = Vec::new();
            if let Some(parent) = git::first_parent(dir, commit).map_err(CycleError::Git)? {
                for cycle in closed {
                    let text = git::file_at(dir, &parent, &cycle.spec).map_err(CycleError::Git)?;
                    let holds = text
                        .as_deref()
                        .and_then(|bytes| std::str::from_utf8(bytes).ok())
                        .and_then(|text| check::holds(text, Mode::Light));
                    if holds.as_deref() == Some(cycle.ticket.as_str()) {
                        tickets.push(cycle.ticket.clone());
                    }
                }
            }
            self.known.insert(commit.to_owned(), tickets);
        }
        Ok(&self.known[commit])
    }
}

/// The Plans the branch touched: closed when approved in some commit of the branch and
/// gone at `HEAD`, open when still there. `from` starts the branch's commits.
fn cycles(root: &ProjectRoot, from: &str, report: &mut Report) -> Result<(), CycleError> {
    let dir = root.path();
    let git = CycleError::Git;
    let at_head = git::files_at(dir, "HEAD", PLANS).map_err(git)?;
    for spec in git::touched_on_branch(dir, from, PLANS).map_err(git)? {
        let Some(name) = spec
            .strip_prefix(&format!("{PLANS}/"))
            .and_then(|rest| rest.strip_suffix(&format!("/{SPEC_FILE}")))
            .filter(|name| !name.contains('/'))
        else {
            continue;
        };
        let dir_ticket = parse_dir_name(name).map_or(name, |(ticket, _)| ticket);
        if at_head.contains(&spec) {
            report.open.push(Open {
                ticket: dir_ticket.to_owned(),
                spec,
            });
            continue;
        }
        let mut deleted_in = None;
        let mut approved = None;
        for commit in git::commits_touching(dir, from, &spec).map_err(git)? {
            match git::file_at(dir, &commit, &spec).map_err(git)? {
                // Newest first: the first deletion found is the last one.
                None => {
                    deleted_in.get_or_insert(commit);
                }
                Some(bytes) => {
                    if approved.is_none()
                        && let Ok(text) = std::str::from_utf8(&bytes)
                    {
                        approved = check::holds(text, Mode::Light);
                    }
                }
            }
        }
        if let (Some(ticket), Some(deleted_in)) = (approved, deleted_in) {
            report.closed.push(Closed {
                ticket,
                spec,
                deleted_in,
            });
        }
    }
    Ok(())
}

/// What `invar cycle required <path>...` says of each path, from the working tree's
/// `invar.yaml`: the pattern it matches, if any.
pub fn required<'a>(patterns: &'a [String], paths: &[String]) -> Vec<(String, Option<&'a str>)> {
    paths
        .iter()
        .map(|path| {
            let path = path.trim_start_matches("./").replace('\\', "/");
            let pattern = glob::first_match(patterns, &path);
            (path, pattern)
        })
        .collect()
}

/// Why the check could not run.
#[derive(Debug)]
pub enum CycleError {
    Git(GitError),
    /// The base's `invar.yaml` is not a valid config.
    BaseConfig {
        source: ConfigError,
    },
    /// A shallow checkout that does not reach the merge base.
    /// The same for `invar config check`.
    Shallow {
        base: String,
        said: String,
    },
    /// `ssh-keygen` could not be run to verify a signed trailer.
    SshKeygen(std::io::Error),
    /// `invar cycle skip` or `invar config accept` refused to sign, and why.
    CannotSign(String),
}

impl fmt::Display for CycleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Git(err) => err.fmt(f),
            Self::BaseConfig { source } => write!(
                f,
                "cannot read `cycle.required_paths` from the base branch: {source}"
            ),
            Self::Shallow { base, said } => write!(
                f,
                "this is a shallow clone, and it does not have the base branch `{base}` or its \
                 merge base with this branch, so the branch's history cannot be read: {said}. {}",
                git::shallow_advice(base)
            ),
            Self::SshKeygen(source) => write!(
                f,
                "cannot run `ssh-keygen` to verify the signed trailers: {source}. Install \
                 OpenSSH's ssh-keygen"
            ),
            Self::CannotSign(why) => write!(f, "{why}"),
        }
    }
}

impl std::error::Error for CycleError {}
