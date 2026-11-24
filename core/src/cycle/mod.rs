//! `invar cycle`: whether a branch that changes declared paths went through a cycle
//! (#155, `contracts/formats/invar-yaml.md` §3.8).
//!
//! The main session decides whether a change needs a Plan (L-9), and it may skip the
//! cycle for trivial work. Nothing in the session can be the check of that decision,
//! because the session is what the check would constrain. So the check is in CI, on
//! the history git keeps: a branch that changes a path the base branch's
//! `cycle.required_paths` names passes only with a cycle closed on it (a Plan approved,
//! then deleted by `invar done`), or with an `Invar-Skip: <reason>` trailer a person
//! wrote. It reads git and writes nothing.

use std::fmt::{self, Write as _};
use std::path::Path;

use crate::format::config::{self, ConfigError, FILE_NAME};
use crate::format::glob;
use crate::format::plan::Mode;
use crate::fs::ProjectRoot;
use crate::fs::plans::{SPEC_FILE, parse_dir_name};
use crate::gate::check;
use crate::git::{self, GitError};

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
    /// The base's `cycle.required_paths`. Empty: the check is off.
    pub patterns: Vec<String>,
    /// Why the check is off, when it is.
    pub off: Option<String>,
    /// How the branch changes `cycle.required_paths`, when it does: said, never used.
    pub list_change: Option<ListChange>,
    /// The merge base, when the check ran.
    pub fork: Option<String>,
    /// Each changed path a pattern matches, with the first pattern that does.
    pub required: Vec<(String, String)>,
    pub closed: Vec<Closed>,
    pub open: Vec<Open>,
    /// Each `Invar-Skip:` trailer with a reason.
    pub skips: Vec<Skip>,
    /// The commits with an `Invar-Skip:` trailer and no reason: they count for nothing.
    pub empty_skips: Vec<String>,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skip {
    pub commit: String,
    pub reason: String,
}

/// The patterns a branch adds to or removes from the base's list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListChange {
    pub added: Vec<String>,
    pub removed: Vec<String>,
}

impl Report {
    /// The check holds: off, nothing required changed, a cycle closed, or a skip.
    pub fn passes(&self) -> bool {
        self.off.is_some()
            || self.required.is_empty()
            || !self.closed.is_empty()
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
        if !self.closed.is_empty() {
            let tickets: Vec<&str> = self.closed.iter().map(|c| c.ticket.as_str()).collect();
            return format!("closed cycle(s): {}", tickets.join(", "));
        }
        format!(
            "skipped by `{SKIP_TRAILER}:` ({} trailer(s))",
            self.skips.len()
        )
    }

    /// Everything the check decided on, ending with one line: `cycle check: off: …`,
    /// `cycle check: passed: …` or `cycle check: failed`.
    pub fn render(&self) -> String {
        let mut out = String::new();
        let short = |commit: &str| commit.chars().take(12).collect::<String>();
        let _ = writeln!(out, "base: {}", self.base);
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
        if let Some(fork) = &self.fork {
            let _ = writeln!(out, "merge base: {}", short(fork));
        }
        let _ = writeln!(out, "required paths: {}", self.patterns.join(", "));

        let _ = writeln!(out, "required changes:");
        if self.required.is_empty() {
            let _ = writeln!(out, "  none");
        }
        for (path, pattern) in &self.required {
            let _ = writeln!(out, "  {path} ({pattern})");
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
        if self.skips.is_empty() {
            let _ = writeln!(out, "  none");
        }
        for skip in &self.skips {
            let _ = writeln!(
                out,
                "  {}: {SKIP_TRAILER}: {}",
                short(&skip.commit),
                skip.reason
            );
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

/// The refusal: every required change, and the two ways out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotCovered {
    pub required: Vec<(String, String)>,
}

impl fmt::Display for NotCovered {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "this branch changes {} required path(s), with no cycle closed on it and no \
             `{SKIP_TRAILER}:` trailer:",
            self.required.len()
        )?;
        for (path, pattern) in &self.required {
            write!(f, "\n  {path} (matches {pattern})")?;
        }
        write!(
            f,
            "\nEither run the change through a cycle and close it with /invar:done, so that the \
             Plan's approval and deletion are on the branch; or, if it needs no Plan, the \
             developer adds a trailer to a commit of the branch: \
             `git commit --amend --trailer \"{SKIP_TRAILER}: <reason>\"`, or the same on a new \
             commit"
        )
    }
}

impl std::error::Error for NotCovered {}

/// Run the check against `base`, or `git.base_branch`, or the base `git` finds.
pub fn check(
    root: &ProjectRoot,
    base: Option<&str>,
    configured: Option<&str>,
) -> Result<Report, CycleError> {
    let dir = root.path();
    let base = match base {
        Some(base) => base.to_owned(),
        None => git::base_branch(dir, configured).map_err(CycleError::Git)?,
    };
    let shallow_or = |err: GitError, base: &str| {
        if matches!(git::is_shallow(dir), Ok(true)) {
            CycleError::Shallow {
                base: base.to_owned(),
                said: err.to_string(),
            }
        } else {
            CycleError::Git(err)
        }
    };
    let base_commit = git::commit_of(dir, &base).map_err(|err| shallow_or(err, &base))?;

    let base_list = match git::file_at(dir, &base_commit, FILE_NAME).map_err(CycleError::Git)? {
        None => None,
        Some(bytes) => {
            let shown = format!("{base}:{FILE_NAME}");
            let loaded = config::parse_bytes(&bytes, Path::new(&shown))
                .map_err(|source| CycleError::BaseConfig { source })?;
            Some(loaded.config.cycle.required_paths)
        }
    };
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
        patterns: patterns.clone(),
        off: None,
        list_change,
        fork: None,
        required: Vec::new(),
        closed: Vec::new(),
        open: Vec::new(),
        skips: Vec::new(),
        empty_skips: Vec::new(),
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

    let fork = git::merge_base(dir, &base).map_err(|err| shallow_or(err, &base))?;

    for path in git::committed_changes(dir, &fork).map_err(CycleError::Git)? {
        if path == INVAR_DIR || path.starts_with(&format!("{INVAR_DIR}/")) {
            continue;
        }
        if let Some(pattern) = glob::first_match(&patterns, &path) {
            report.required.push((path, pattern.to_owned()));
        }
    }

    cycles(root, &fork, &mut report)?;

    for (commit, values) in git::trailers(dir, &fork, SKIP_TRAILER).map_err(CycleError::Git)? {
        let had_trailer = !values.is_empty();
        let mut counted = false;
        for value in values {
            let reason = value.trim();
            if !reason.is_empty() {
                report.skips.push(Skip {
                    commit: commit.clone(),
                    reason: reason.to_owned(),
                });
                counted = true;
            }
        }
        if !counted && had_trailer {
            report.empty_skips.push(commit);
        }
    }
    report.fork = Some(fork);
    Ok(report)
}

/// The Plans the branch touched: closed when approved in some commit of the branch and
/// gone at `HEAD`, open when still there.
fn cycles(root: &ProjectRoot, fork: &str, report: &mut Report) -> Result<(), CycleError> {
    let dir = root.path();
    let git = CycleError::Git;
    let at_head = git::files_at(dir, "HEAD", PLANS).map_err(git)?;
    for spec in git::touched_on_branch(dir, fork, PLANS).map_err(git)? {
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
        for commit in git::commits_touching(dir, fork, &spec).map_err(git)? {
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
    Shallow {
        base: String,
        said: String,
    },
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
        }
    }
}

impl std::error::Error for CycleError {}
