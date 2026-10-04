//! Finding a Plan by ticket: `.invar/plans/<TICKET>-<slug>/` (§4a).
//!
//! Implements the lookup rule of `contracts/formats/plan-header.md` §2.
//!
//! **A wrong guess here means approving one specification and implementing
//! another**, so the lookup never guesses. It has exactly one success — one
//! directory matches — and every other outcome is its own loud error: no
//! match, several matches, a match whose name is malformed, a match without a
//! `spec.md`, and a Plan whose header names a different ticket than its
//! directory.
//!
//! **Matching is substring matching on whole segments** (§2, D-10). The query
//! must appear in the directory name with no letter or digit directly before
//! or after it. `PROJ-412`, `412` and `reserve-stock` all find
//! `PROJ-412-reserve-stock`; `PROJ-41` does not. Plain substring matching
//! would let `PROJ-41` silently resolve to `PROJ-412-…` whenever `PROJ-41`
//! has no Plan of its own — a single, confident, wrong match that no
//! ambiguity check can catch.

// `plan validate` (M1-8) uses `find`; `open`, which needs a valid Plan, is
// for `plan approve` (M1-9). `expect` rather than `allow`, so the compiler
// asks for this line to go once it is used.
#![cfg_attr(
    not(test),
    expect(dead_code, reason = "`open` is used by `plan approve` from M1-9")
)]

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use crate::format::plan::{Mode, Plan, PlanError};

use super::ProjectRoot;

#[cfg(test)]
mod tests;

/// The file inside a Plan directory that holds the Plan (§2).
pub const SPEC_FILE: &str = "spec.md";

/// A Plan directory whose name has the contract's shape and that holds a
/// `spec.md`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanDir {
    /// Absolute, under `.invar/plans/` (through a symlink, if the directory is
    /// one).
    pub path: PathBuf,
    /// The directory name: `PROJ-412-reserve-stock`.
    pub name: String,
    /// `PROJ-412`.
    pub ticket: String,
    /// `reserve-stock`.
    pub slug: String,
}

impl PlanDir {
    pub fn spec_path(&self) -> PathBuf {
        self.path.join(SPEC_FILE)
    }
}

/// Find the one Plan directory `query` names.
///
/// `query` is normally a ticket id (`PROJ-412`), but any whole segment of the
/// directory name works: `412`, `reserve-stock`. Zero matches and several
/// matches are distinct errors; so is a single match that is malformed or has
/// no `spec.md`.
pub fn find(root: &ProjectRoot, query: &str) -> Result<PlanDir, LookupError> {
    let query = query.trim();
    if query.is_empty()
        || query.contains(['/', '\\'])
        || !query.chars().any(|c| c.is_ascii_alphanumeric())
    {
        return Err(LookupError::InvalidQuery {
            query: query.to_owned(),
        });
    }

    let plans_dir = root.plans_dir();
    let Some(names) = directory_names(&plans_dir)? else {
        return Err(LookupError::NotFound {
            query: query.to_owned(),
            plans_dir,
            exists: false,
            plans: Vec::new(),
            case_hint: None,
        });
    };

    let matching: Vec<&String> = names
        .iter()
        .filter(|name| matches_segment(name, query))
        .collect();

    match matching.as_slice() {
        [] => {
            let case_hint = names
                .iter()
                .find(|name| matches_segment(&name.to_lowercase(), &query.to_lowercase()))
                .cloned();
            Err(LookupError::NotFound {
                query: query.to_owned(),
                plans_dir,
                exists: true,
                plans: names,
                case_hint,
            })
        }
        [name] => {
            let path = plans_dir.join(name);
            let Some((ticket, slug)) = parse_dir_name(name) else {
                return Err(LookupError::MalformedName {
                    query: query.to_owned(),
                    path,
                });
            };
            if !path.join(SPEC_FILE).is_file() {
                return Err(LookupError::MissingSpec { path });
            }
            Ok(PlanDir {
                ticket: ticket.to_owned(),
                slug: slug.to_owned(),
                name: (*name).clone(),
                path,
            })
        }
        several => Err(LookupError::Ambiguous {
            query: query.to_owned(),
            matches: several.iter().map(|name| plans_dir.join(name)).collect(),
        }),
    }
}

/// Find the Plan `query` names, read it, and check that its header agrees
/// with its directory.
pub fn open(root: &ProjectRoot, query: &str, mode: Mode) -> Result<(PlanDir, Plan), LookupError> {
    let dir = find(root, query)?;
    let plan = Plan::read(&dir.spec_path(), mode).map_err(LookupError::Plan)?;
    if let Some(problem) = ticket_mismatch(&dir.name, &dir.ticket, &plan.header().ticket) {
        return Err(LookupError::TicketMismatch {
            spec: dir.spec_path(),
            problem,
        });
    }
    Ok((dir, plan))
}

/// The header's `Ticket` must equal the directory's `<TICKET>` (§5.1). `None`
/// when they agree; otherwise the problem, worded for a person or an agent to
/// act on. `plan validate` (M1-8) reports it alongside the Plan's own
/// problems, so it takes plain values rather than a valid Plan.
pub fn ticket_mismatch(dir_name: &str, dir_ticket: &str, header_ticket: &str) -> Option<String> {
    (header_ticket != dir_ticket).then(|| {
        format!(
            "the header says `Ticket: {header_ticket}`, but the directory `{dir_name}` is \
             for `{dir_ticket}`. One of them is wrong: fix the header, or rename the \
             directory — the directory name is what the branch and the decision entry \
             are matched by"
        )
    })
}

/// Every Plan directory name under `.invar/plans/`, sorted, whatever its
/// shape; `None` when `.invar/plans/` does not exist. Used by
/// `plan validate --all-drafts` (M1-8).
pub fn list(root: &ProjectRoot) -> Result<Option<Vec<String>>, LookupError> {
    directory_names(&root.plans_dir())
}

/// `<TICKET>-<slug>` → `(TICKET, slug)` (§2): an uppercase ticket id
/// `[A-Z][A-Z0-9]*-[0-9]+`, a dash, then a slug of lowercase letters and
/// digits in groups separated by single dashes. `None` for any other shape.
///
/// The split is unambiguous: the ticket's letters stop at its first dash, and
/// its number stops at the first character that is not a digit.
pub fn parse_dir_name(name: &str) -> Option<(&str, &str)> {
    let (prefix, rest) = name.split_once('-')?;
    let mut prefix_chars = prefix.chars();
    let prefix_ok = prefix_chars.next().is_some_and(|c| c.is_ascii_uppercase())
        && prefix_chars.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit());
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    let slug = rest[digits..].strip_prefix('-')?;
    let slug_ok = !slug.is_empty()
        && slug.split('-').all(|group| {
            !group.is_empty()
                && group
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        });
    (prefix_ok && digits > 0 && slug_ok).then(|| (&name[..prefix.len() + 1 + digits], slug))
}

/// Whether `query` occurs in `name` as a whole segment: no letter or digit
/// directly before or after the occurrence (D-10).
fn matches_segment(name: &str, query: &str) -> bool {
    name.match_indices(query).any(|(start, found)| {
        let before = name[..start].chars().next_back();
        let after = name[start + found.len()..].chars().next();
        let joins = |c: Option<char>| c.is_some_and(|c| c.is_ascii_alphanumeric());
        !joins(before) && !joins(after)
    })
}

/// The names of the directories in `plans_dir`, sorted; `None` when
/// `plans_dir` itself does not exist. Files and hidden entries (`.gitkeep`,
/// `.DS_Store`) are not Plans and are skipped.
fn directory_names(plans_dir: &Path) -> Result<Option<Vec<String>>, LookupError> {
    let unreadable = |source| LookupError::PlansDirUnreadable {
        path: plans_dir.to_owned(),
        source,
    };
    let entries = match std::fs::read_dir(plans_dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(unreadable(err)),
    };
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry.map_err(unreadable)?;
        let name = entry.file_name().to_string_lossy().into_owned();
        // `metadata` follows symlinks: a symlinked Plan directory counts.
        if name.starts_with('.') || !entry.path().is_dir() {
            continue;
        }
        names.push(name);
    }
    names.sort();
    Ok(Some(names))
}

// --- errors -----------------------------------------------------------------

/// Why no single, valid Plan was found. Each case is distinct, so a caller —
/// and a test — can tell them apart (M1-7).
#[derive(Debug)]
pub enum LookupError {
    /// Nothing usable to search for: empty, a path, punctuation only.
    InvalidQuery { query: String },
    /// `.invar/plans/` exists but cannot be listed.
    PlansDirUnreadable { path: PathBuf, source: io::Error },
    /// No directory matches.
    NotFound {
        query: String,
        plans_dir: PathBuf,
        /// Whether `.invar/plans/` exists at all.
        exists: bool,
        /// Every Plan directory there, for the message.
        plans: Vec<String>,
        /// A directory that matches when case is ignored.
        case_hint: Option<String>,
    },
    /// More than one directory matches. Never resolved by guessing.
    Ambiguous {
        query: String,
        matches: Vec<PathBuf>,
    },
    /// The one match does not have the shape `<TICKET>-<slug>`.
    MalformedName { query: String, path: PathBuf },
    /// The one match has no `spec.md`.
    MissingSpec { path: PathBuf },
    /// The Plan was found but is not valid.
    Plan(PlanError),
    /// The Plan is valid, but its header names another ticket.
    TicketMismatch { spec: PathBuf, problem: String },
}

impl fmt::Display for LookupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidQuery { query } => write!(
                f,
                "`{query}` cannot be looked up; give a ticket id such as `PROJ-412`"
            ),
            Self::PlansDirUnreadable { path, source } => {
                write!(f, "cannot list {}: {source}", path.display())
            }
            Self::NotFound {
                query,
                plans_dir,
                exists,
                plans,
                case_hint,
            } => {
                write!(f, "no Plan for `{query}` in {}", plans_dir.display())?;
                if !exists {
                    return write!(
                        f,
                        ": the directory does not exist, so no Plan has been written \
                         in this project yet"
                    );
                }
                if let Some(hint) = case_hint {
                    write!(
                        f,
                        "; did you mean `{hint}`? Ticket ids are matched with their exact \
                         capitalisation"
                    )?;
                } else if plans.is_empty() {
                    write!(f, "; there are no Plans there")?;
                } else {
                    write!(f, "; the Plans there are: {}", plans.join(", "))?;
                }
                Ok(())
            }
            Self::Ambiguous { query, matches } => {
                write!(
                    f,
                    "`{query}` matches {} Plans; give the full ticket id to choose one:",
                    matches.len()
                )?;
                for path in matches {
                    write!(f, "\n  {}", path.display())?;
                }
                Ok(())
            }
            Self::MalformedName { query, path } => write!(
                f,
                "`{query}` matches {}, but its name is not `<TICKET>-<slug>`: an \
                 uppercase ticket id, a dash, then a lowercase slug of letters, digits \
                 and single dashes, such as `PROJ-412-reserve-stock`. Rename the \
                 directory",
                path.display()
            ),
            Self::MissingSpec { path } => write!(
                f,
                "{} has no `{SPEC_FILE}`; a Plan directory holds its Plan in `{SPEC_FILE}`",
                path.display()
            ),
            Self::Plan(err) => write!(f, "{err}"),
            Self::TicketMismatch { spec, problem } => {
                write!(f, "{}: {problem}", spec.display())
            }
        }
    }
}

impl std::error::Error for LookupError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::PlansDirUnreadable { source, .. } => Some(source),
            Self::Plan(err) => Some(err),
            _ => None,
        }
    }
}
