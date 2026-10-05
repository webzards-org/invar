//! `invar plan validate` (M1-8): is this Plan well-formed?
//!
//! **Form, not approval.** The checks are the Plan's location, its directory
//! name, its header, its required sections, and whether the header agrees
//! with the directory. Whether it is approved — the approval fields, the
//! checksum — is not looked at: a draft with empty approval fields is valid,
//! and so is an `approved` Plan whose checksum no longer matches. Approval is
//! checked by `plan approve` and `plan check` (M1-9, M1-10; plan-header.md
//! D-11).
//!
//! **One run reports everything.** The validator is called by the planner
//! while it writes the Plan, and by a `SubagentStop` hook when it stops
//! (M1-17). Each round trip costs an agent a turn, so every problem goes in
//! one [`Report`], and every message says what to change.

use std::io;
use std::path::{Component, Path, PathBuf};

use crate::diagnostics::Diagnostics;
use crate::format::plan::{Mode, Plan, Status};
use crate::fs::ProjectRoot;
use crate::fs::plans::{self, LookupError, SPEC_FILE};

#[cfg(test)]
mod tests;

/// The result of validating one Plan file.
#[derive(Debug)]
pub struct Report {
    /// The file validated, absolute.
    pub spec: PathBuf,
    /// The header's `Status`, when it could be read.
    pub status: Option<Status>,
    /// Every problem found; empty for a valid Plan.
    pub problems: Diagnostics,
}

impl Report {
    pub fn is_valid(&self) -> bool {
        self.problems.is_empty()
    }
}

/// Validate the Plan file at `spec`, which is absolute.
pub fn file(root: &ProjectRoot, spec: &Path, mode: Mode) -> Report {
    let mut problems = Diagnostics::new();
    let dir_name = check_location(root, spec, &mut problems);
    let dir_ticket = dir_name.as_deref().and_then(|name| {
        let parsed = plans::parse_dir_name(name).map(|(ticket, _)| ticket.to_owned());
        if parsed.is_none() {
            problems.general(format!(
                "the directory name `{name}` is not `<TICKET>-<slug>`: an uppercase \
                 ticket id, a dash, then a lowercase slug of letters, digits and single \
                 dashes, such as `PROJ-412-reserve-stock`. Rename the directory"
            ));
        }
        parsed
    });

    let mut status = None;
    match read_text(spec) {
        Err(problem) => problems.general(problem),
        Ok(text) => {
            let checked = Plan::check(&text, mode);
            problems.extend(checked.problems);
            status = checked.status;
            if let (Some(name), Some(dir_ticket), Some((ticket, line))) =
                (&dir_name, &dir_ticket, &checked.ticket)
                && let Some(problem) = plans::ticket_mismatch(name, dir_ticket, ticket)
            {
                problems.at(*line, problem);
            }
        }
    }

    Report {
        spec: spec.to_owned(),
        status,
        problems,
    }
}

/// The `spec.md` that `query` names, for validating it.
///
/// A match whose directory name is malformed, or that has no `spec.md`, still
/// names a file — those are problems of the Plan, reported with the rest by
/// [`file`]. Every other lookup failure (no match, several) is returned:
/// there is no Plan to report on.
pub fn spec_for(root: &ProjectRoot, query: &str) -> Result<PathBuf, LookupError> {
    match plans::find(root, query) {
        Ok(dir) => Ok(dir.spec_path()),
        Err(LookupError::MalformedName { path, .. } | LookupError::MissingSpec { path }) => {
            Ok(path.join(SPEC_FILE))
        }
        Err(err) => Err(err),
    }
}

/// Validate every Plan under `.invar/plans/` that is not `approved`, in name
/// order. A Plan whose `Status` cannot be read counts as a draft: it is being
/// written. `None` when `.invar/plans/` does not exist.
pub fn drafts(root: &ProjectRoot, mode: Mode) -> Result<Option<Vec<Report>>, LookupError> {
    let Some(names) = plans::list(root)? else {
        return Ok(None);
    };
    let plans_dir = root.plans_dir();
    Ok(Some(
        names
            .iter()
            .map(|name| file(root, &plans_dir.join(name).join(SPEC_FILE), mode))
            .filter(|report| report.status != Some(Status::Approved))
            .collect(),
    ))
}

/// A Plan lives at `.invar/plans/<name>/spec.md` (§2). Reports where it must
/// move otherwise; returns `<name>` when the location is right.
fn check_location(root: &ProjectRoot, spec: &Path, problems: &mut Diagnostics) -> Option<String> {
    const EXPECTED: &str = ".invar/plans/<TICKET>-<slug>/spec.md";
    let relative = match root.relative_path(spec) {
        Ok(Some(relative)) => relative,
        Ok(None) => {
            problems.general(format!(
                "the file is outside the project at {}; a Plan lives at `{EXPECTED}` \
                 in the project — move it there",
                root.path().display()
            ));
            return None;
        }
        Err(err) => {
            problems.general(format!("cannot resolve the file's location: {err}"));
            return None;
        }
    };

    let parts: Vec<String> = relative
        .components()
        .map(|part| match part {
            Component::Normal(name) => name.to_string_lossy().into_owned(),
            other => other.as_os_str().to_string_lossy().into_owned(),
        })
        .collect();
    let shown = parts.join("/");
    match parts.as_slice() {
        [invar, plans, name, file] if invar == ".invar" && plans == "plans" => {
            if file != SPEC_FILE {
                problems.general(format!(
                    "a Plan file is named `{SPEC_FILE}`; rename `{shown}` to \
                     `.invar/plans/{name}/{SPEC_FILE}`"
                ));
            }
            Some(name.clone())
        }
        _ => {
            problems.general(format!(
                "a Plan lives at `{EXPECTED}`, not `{shown}`; move it there, into a \
                 directory named after its ticket"
            ));
            None
        }
    }
}

/// The file as UTF-8 text, or the problem that prevents reading it.
fn read_text(spec: &Path) -> Result<String, String> {
    let bytes = std::fs::read(spec).map_err(|err| match err.kind() {
        io::ErrorKind::NotFound => format!(
            "there is no `{SPEC_FILE}`; a Plan directory holds its Plan in `{SPEC_FILE}` — \
             write it there"
        ),
        io::ErrorKind::IsADirectory => format!("this is a directory, not a `{SPEC_FILE}` file"),
        _ => format!("cannot read the file: {err}"),
    })?;
    String::from_utf8(bytes).map_err(|err| {
        format!(
            "not valid UTF-8 (first invalid byte at offset {}); save the Plan as UTF-8",
            err.utf8_error().valid_up_to()
        )
    })
}
