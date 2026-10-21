//! The draft of a decision entry: `.invar/plans/<TICKET>-<slug>/decision.md`
//! (decision-entry.md §8.1).
//!
//! The agent writes the draft (`/invar:log`); [`validate`] checks it, so a
//! malformed draft is found when it is written and not when the developer
//! records it. A draft is not an entry, and nothing reads a draft as part of
//! the log.
//!
//! [`approve`] is the developer's gate (`invar decision approve`, run through
//! `/invar:log-approve`): it validates the draft as it is now — after any
//! edits the developer made while reading it — records it as an entry, and
//! removes it. It is the only thing here that touches `.invar/decisions/`.
//!
//! The checks are the draft's own (`format::decision::parse_draft`) and the
//! ones that need its place: it sits in a Plan directory, next to a `spec.md`,
//! and names that directory's ticket.

use std::fmt;
use std::io;
use std::path::{Component, Path, PathBuf};

use crate::diagnostics::Diagnostics;
use crate::format::decision::{self, DRAFT_FILE, Draft};
use crate::format::plan::Mode;
use crate::fs::ProjectRoot;
use crate::fs::plans::{self, LookupError, SPEC_FILE};
use crate::git::Instant;

use super::{RecordError, Recorded};

/// The result of validating one draft.
#[derive(Debug)]
pub struct Report {
    /// The file validated, absolute.
    pub path: PathBuf,
    /// The draft, when it is valid.
    pub draft: Option<Draft>,
    /// Every problem found; empty for a valid draft.
    pub problems: Diagnostics,
}

/// The draft's path for the Plan `query` names, whether or not the draft
/// exists yet.
///
/// A match whose directory name is malformed still names a place — that is a
/// problem of the draft's location, reported by [`validate`]. Every other
/// lookup failure is returned: there is no Plan for the draft to belong to.
pub fn path_for(root: &ProjectRoot, query: &str) -> Result<PathBuf, LookupError> {
    match plans::find(root, query) {
        Ok(dir) => Ok(dir.path.join(DRAFT_FILE)),
        Err(LookupError::MalformedName { path, .. } | LookupError::MissingSpec { path }) => {
            Ok(path.join(DRAFT_FILE))
        }
        Err(err) => Err(err),
    }
}

/// Validate the draft at `path`, which is absolute.
pub fn validate(root: &ProjectRoot, path: &Path) -> Report {
    let mut problems = Diagnostics::new();
    let dir_ticket = check_location(root, path, &mut problems);

    let mut draft = None;
    match read_text(path) {
        Err(problem) => problems.general(problem),
        Ok(text) => match decision::parse_draft(&text) {
            Err(found) => problems.extend(found),
            Ok(parsed) => {
                if let Some(dir_ticket) = &dir_ticket
                    && parsed.ticket != *dir_ticket
                {
                    problems.general(format!(
                        "the draft says `ticket: {}`, but it is in the Plan directory of \
                         {dir_ticket}; a draft belongs to the Plan it sits next to — write \
                         `ticket: {dir_ticket}`, or move the draft to the Plan of {}",
                        parsed.ticket, parsed.ticket
                    ));
                }
                if problems.is_empty() {
                    draft = Some(parsed);
                }
            }
        },
    }

    Report {
        path: path.to_owned(),
        draft,
        problems,
    }
}

// --- approving --------------------------------------------------------------------

/// What [`approve`] did.
#[derive(Debug)]
pub struct Approved {
    /// The entry, and the Plan reset for `failed`.
    pub recorded: Recorded,
    /// The draft that was recorded.
    pub draft: PathBuf,
    /// Why the draft could not be removed, when it could not. The entry is
    /// recorded either way; the draft left behind must not be approved again.
    pub left_behind: Option<io::Error>,
}

/// Record the draft at `path` (absolute; [`path_for`] finds it from a
/// ticket), and remove it.
///
/// The draft is read and validated now, so what is recorded is what the
/// developer has just read, edits included. It refuses when there is no
/// draft, or when the draft is not valid; nothing is written then. A
/// `failed` draft returns the Plan to `draft` with its entry (decision-entry.md
/// §7, D-8).
pub fn approve(
    root: &ProjectRoot,
    path: &Path,
    now: Instant,
    mode: Mode,
) -> Result<Approved, ApproveError> {
    if matches!(
        std::fs::symlink_metadata(path),
        Err(err) if err.kind() == io::ErrorKind::NotFound
    ) {
        return Err(ApproveError::NoDraft {
            path: path.to_owned(),
        });
    }
    let report = validate(root, path);
    let Some(draft) = report.draft else {
        return Err(ApproveError::Invalid(Box::new(report)));
    };
    let recorded = super::record_draft(root, draft, now, mode).map_err(ApproveError::Record)?;
    let left_behind = std::fs::remove_file(path).err();
    Ok(Approved {
        recorded,
        draft: path.to_owned(),
        left_behind,
    })
}

#[derive(Debug)]
pub enum ApproveError {
    /// The Plan has no `decision.md`.
    NoDraft { path: PathBuf },
    /// The draft has problems; the report lists them all. Boxed: it is far
    /// larger than the other variants (`clippy::result_large_err`).
    Invalid(Box<Report>),
    /// The draft is valid, but the entry could not be written.
    Record(RecordError),
}

impl fmt::Display for ApproveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoDraft { path } => write!(
                f,
                "there is no draft to record: {} does not exist. Have the agent draft the \
                 entry with `/invar:log` first, read it, then approve it; nothing was \
                 written",
                path.display()
            ),
            Self::Invalid(report) => {
                let count = report.problems.len();
                write!(
                    f,
                    "the draft is not valid ({count} {}), so nothing was recorded; fix it \
                     — by hand, or with `/invar:log` — then approve it again:\n{}",
                    if count == 1 { "problem" } else { "problems" },
                    report.problems.render(&report.path).trim_end()
                )
            }
            Self::Record(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for ApproveError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Record(err) => Some(err),
            Self::NoDraft { .. } | Self::Invalid(_) => None,
        }
    }
}

/// A draft lives at `.invar/plans/<TICKET>-<slug>/decision.md`, next to the
/// Plan's `spec.md`. Reports where it must move otherwise; returns the
/// directory's ticket when the location is right.
fn check_location(root: &ProjectRoot, path: &Path, problems: &mut Diagnostics) -> Option<String> {
    const EXPECTED: &str = ".invar/plans/<TICKET>-<slug>/decision.md";
    let relative = match root.relative_path(path) {
        Ok(Some(relative)) => relative,
        Ok(None) => {
            problems.general(format!(
                "the file is outside the project at {}; a draft lives at `{EXPECTED}` in \
                 the project — move it there",
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
    let [invar, plans_dir, name, file] = parts.as_slice() else {
        problems.general(format!(
            "a draft lives at `{EXPECTED}`, not `{shown}`; move it into the Plan directory \
             of its ticket"
        ));
        return None;
    };
    if invar != ".invar" || plans_dir != "plans" {
        problems.general(format!(
            "a draft lives at `{EXPECTED}`, not `{shown}`; move it into the Plan directory \
             of its ticket"
        ));
        return None;
    }
    if file != DRAFT_FILE {
        problems.general(format!(
            "a draft is named `{DRAFT_FILE}`; rename `{shown}` to \
             `.invar/plans/{name}/{DRAFT_FILE}`"
        ));
    }
    let Some((ticket, _)) = plans::parse_dir_name(name) else {
        problems.general(format!(
            "the directory name `{name}` is not `<TICKET>-<slug>`, so the draft belongs to no \
             ticket; a draft sits in the Plan directory of its ticket, such as \
             `.invar/plans/PROJ-412-reserve-stock/{DRAFT_FILE}`"
        ));
        return None;
    };
    let dir = path.parent().unwrap_or(path);
    if !dir.join(SPEC_FILE).is_file() {
        problems.general(format!(
            "there is no `{SPEC_FILE}` next to the draft; a draft belongs to a Plan, and \
             `.invar/plans/{name}/` holds none"
        ));
    }
    Some(ticket.to_owned())
}

/// The file as UTF-8 text, or the problem that prevents reading it.
fn read_text(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|err| match err.kind() {
        io::ErrorKind::NotFound => format!(
            "there is no `{DRAFT_FILE}` yet; draft the entry with `/invar:log`, which writes \
             it here"
        ),
        io::ErrorKind::IsADirectory => format!("this is a directory, not a `{DRAFT_FILE}` file"),
        _ => format!("cannot read the file: {err}"),
    })?;
    String::from_utf8(bytes).map_err(|err| {
        format!(
            "not valid UTF-8 (first invalid byte at offset {}); save the draft as UTF-8",
            err.utf8_error().valid_up_to()
        )
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    const VALID: &str = "---\nticket: SHOP-214\nmodule: orders\ntype: confirmed\n---\n\n\
                         Stock is released inside the cancel transaction.\n";

    /// A project with `invar.yaml` and one Plan directory, `SHOP-214-cancel`.
    fn project() -> (tempfile::TempDir, ProjectRoot, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("invar.yaml"), "scenarios: false\n").unwrap();
        let plan = dir.path().join(".invar/plans/SHOP-214-cancel");
        fs::create_dir_all(&plan).unwrap();
        fs::write(plan.join(SPEC_FILE), "").unwrap();
        let root = ProjectRoot::discover_from(dir.path()).unwrap();
        let plan = root.plans_dir().join("SHOP-214-cancel");
        (dir, root, plan)
    }

    fn messages(report: &Report) -> String {
        report.problems.to_string()
    }

    #[test]
    fn a_valid_draft_next_to_its_plan_passes() {
        let (_dir, root, plan) = project();
        fs::write(plan.join(DRAFT_FILE), VALID).unwrap();
        let report = validate(&root, &plan.join(DRAFT_FILE));
        assert!(report.problems.is_empty(), "{}", messages(&report));
        let draft = report.draft.unwrap();
        assert_eq!(draft.ticket, "SHOP-214");
        assert_eq!(draft.kind.as_str(), "confirmed");
    }

    #[test]
    fn a_draft_naming_another_ticket_is_refused() {
        let (_dir, root, plan) = project();
        fs::write(plan.join(DRAFT_FILE), VALID.replace("SHOP-214", "SHOP-300")).unwrap();
        let report = validate(&root, &plan.join(DRAFT_FILE));
        assert!(!report.problems.is_empty());
        assert!(messages(&report).contains("Plan directory of SHOP-214"));
        assert!(report.draft.is_none());
    }

    #[test]
    fn a_missing_draft_says_how_to_write_one() {
        let (_dir, root, plan) = project();
        let report = validate(&root, &plan.join(DRAFT_FILE));
        assert!(
            messages(&report).contains("/invar:log"),
            "{}",
            messages(&report)
        );
    }

    #[test]
    fn a_draft_outside_a_plan_directory_is_told_where_to_go() {
        let (_dir, root, _plan) = project();
        let stray = root.path().join(".invar/decisions/decision.md");
        fs::create_dir_all(stray.parent().unwrap()).unwrap();
        fs::write(&stray, VALID).unwrap();
        let report = validate(&root, &stray);
        assert!(
            messages(&report).contains(".invar/plans/<TICKET>-<slug>/decision.md"),
            "{}",
            messages(&report)
        );
    }

    #[test]
    fn a_draft_with_no_plan_beside_it_is_refused() {
        let (_dir, root, plan) = project();
        fs::remove_file(plan.join(SPEC_FILE)).unwrap();
        fs::write(plan.join(DRAFT_FILE), VALID).unwrap();
        let report = validate(&root, &plan.join(DRAFT_FILE));
        assert!(messages(&report).contains("no `spec.md` next to the draft"));
    }

    #[test]
    fn the_path_for_a_ticket_is_in_its_plan_directory_even_before_it_exists() {
        let (_dir, root, plan) = project();
        assert_eq!(path_for(&root, "SHOP-214").unwrap(), plan.join(DRAFT_FILE));
        assert!(matches!(
            path_for(&root, "SHOP-999"),
            Err(LookupError::NotFound { .. })
        ));
    }

    #[test]
    fn validating_writes_nothing() {
        let (_dir, root, plan) = project();
        fs::write(plan.join(DRAFT_FILE), VALID).unwrap();
        validate(&root, &plan.join(DRAFT_FILE));
        assert_eq!(fs::read_to_string(plan.join(DRAFT_FILE)).unwrap(), VALID);
        assert!(!root.decisions_dir().exists());
    }
}
