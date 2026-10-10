//! `invar clean-spec` (M1-13): removing the Plans that abandoned cycles left
//! behind (§12).
//!
//! A Plan is a consumable instruction, deleted by `invar done` once its cycle
//! closes. A cycle that never closes leaves its Plan behind, and this is the
//! command for those. It is deliberately hard to use by accident:
//!
//! - **Assessing and deleting are separate.** [`assess`] reads and decides,
//!   and changes nothing; [`delete`] removes what `assess` marked. A dry run
//!   is therefore not a second code path that might disagree with the real
//!   one: it is the first half of the same one.
//! - **Only `.invar/plans/<name>/` is ever removed** — never
//!   `.invar/decisions/`, never `specs/`, never project code. `delete` takes
//!   a directory *name*, checks that it is one path component, and joins it
//!   to the Plans directory; there is no other path in this module that
//!   reaches the disk with a deletion.
//! - **A Plan with no decision entry for its ticket is never deleted** (§12).
//!   A missing entry means the cycle did not finish properly, which is a
//!   signal rather than garbage. The honest way to close an abandoned cycle
//!   is to record a `failed` or `note` entry first (`invar decision record`),
//!   and only then clean. An entry of any type counts, but only an entry that
//!   *can be read* does: a hand-edited file that no longer parses is counted
//!   and reported, never taken as proof, because the safe mistake here is to
//!   keep a Plan, not to delete one.
//! - **`--merged` never guesses.** A Plan is eligible when exactly one branch
//!   already merged into the base branch names its ticket. None skips the
//!   Plan; several fail the whole run, listing them, and nothing is deleted —
//!   one ticket on several merged branches is more than one cycle, and which
//!   of them is over is not for this command to decide.
//! - **An approved Plan is kept unless it was asked for.** A bare run cannot
//!   tell an abandoned approved Plan from one whose cycle is running right
//!   now, and the second is worth far more than the first. It is deleted when
//!   it is named, or when its branch is merged — that is the evidence.

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use crate::decision::{self, RecordError};
use crate::format::decision::Type;
use crate::format::plan::{Mode, Plan, Status};
use crate::fs::ProjectRoot;
use crate::fs::plans::{self, LookupError, SPEC_FILE, matches_segment, parse_dir_name};

#[cfg(test)]
mod tests;

// --- what to look at -----------------------------------------------------------

/// How far a run reaches.
#[derive(Debug, Clone, Copy)]
pub struct Scope<'a> {
    /// One Plan, named the way every command names it (`PROJ-412`).
    pub ticket: Option<&'a str>,
    /// The branches already merged into the base branch, when `--merged` was
    /// given. A Plan is then eligible only if one of them names its ticket.
    pub merged: Option<&'a [String]>,
}

/// What a run found, before anything is deleted.
#[derive(Debug)]
pub struct Assessment {
    pub plans_dir: PathBuf,
    /// Whether `.invar/plans/` exists at all.
    pub plans_dir_exists: bool,
    /// Every Plan directory considered, in name order.
    pub items: Vec<Item>,
    /// Decision entries that could not be read. A Plan whose entry is among
    /// them is reported as having none.
    pub unreadable_entries: usize,
}

impl Assessment {
    /// How many Plans would be, or were, deleted.
    pub fn eligible(&self) -> usize {
        self.items
            .iter()
            .filter(|item| matches!(item.verdict, Verdict::Delete { .. }))
            .count()
    }
}

/// One Plan directory and what is to be done with it.
#[derive(Debug)]
pub struct Item {
    /// The directory name: `PROJ-412-reserve-stock`.
    pub name: String,
    /// `.invar/plans/<name>`, absolute.
    pub path: PathBuf,
    /// The ticket the directory name carries; `None` when the name is not
    /// `<TICKET>-<slug>`.
    pub ticket: Option<String>,
    /// The Plan's `Status`, when its `spec.md` can be read well enough.
    pub status: Option<Status>,
    pub verdict: Verdict,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Deleted by [`delete`]: the decision entry that closes the cycle on
    /// paper, and under `--merged` the branch that closes it in git.
    Delete {
        entry: EntryRef,
        branch: Option<String>,
    },
    /// Left alone, and why.
    Keep(Keep),
}

/// The newest decision entry for a ticket.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryRef {
    pub id: String,
    pub kind: Type,
    /// As written in the entry: `2026-10-06T18:37:12+02:00`.
    pub date: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Keep {
    /// The directory's name is not `<TICKET>-<slug>`, so its ticket cannot be
    /// told, and with it neither can its decision entry.
    UnknownTicket,
    /// No decision entry for the ticket (§12): a signal, not garbage.
    NoDecisionEntry,
    /// `--merged`, and no merged branch names the ticket.
    NotMerged,
    /// `Status: approved`, and the run named neither a ticket nor `--merged`.
    ApprovedUnscoped,
}

impl Keep {
    /// A refusal rather than a skip: the Plan is not deleted because deleting
    /// it would be unsafe, not because a condition the caller set was not met.
    /// A Plan the caller *named* and got one of these for is an error, not a
    /// quiet success.
    pub fn is_refusal(self) -> bool {
        matches!(self, Self::UnknownTicket | Self::NoDecisionEntry)
    }

    /// The reason in a few words, for an error message.
    pub fn summary(self) -> &'static str {
        match self {
            Self::UnknownTicket => "its directory name does not tell its ticket",
            Self::NoDecisionEntry => "it has no decision entry",
            Self::NotMerged => "no merged branch names its ticket",
            Self::ApprovedUnscoped => "it is approved, and its cycle may still be running",
        }
    }
}

// --- assessing -------------------------------------------------------------------

/// Decide what to do with each Plan in scope. Changes nothing.
pub fn assess(root: &ProjectRoot, scope: Scope<'_>, mode: Mode) -> Result<Assessment, CleanError> {
    let plans_dir = root.plans_dir();
    let names = match scope.ticket {
        Some(ticket) => vec![named_plan(root, ticket)?],
        None => match plans::list(root).map_err(CleanError::Lookup)? {
            Some(names) => names,
            None => {
                return Ok(Assessment {
                    plans_dir,
                    plans_dir_exists: false,
                    items: Vec::new(),
                    unreadable_entries: 0,
                });
            }
        },
    };
    let (entries, unreadable_entries) = newest_entries(root)?;

    let mut items = Vec::new();
    let mut ambiguous = Vec::new();
    for name in names {
        let path = plans_dir.join(&name);
        let ticket = parse_dir_name(&name).map(|(ticket, _)| ticket.to_owned());
        let status = read_status(&path.join(SPEC_FILE), mode);

        let verdict = match &ticket {
            None => Verdict::Keep(Keep::UnknownTicket),
            Some(ticket) => match entries.get(ticket) {
                None => Verdict::Keep(Keep::NoDecisionEntry),
                Some(entry) => match scope.merged {
                    Some(merged) => {
                        let named: Vec<&String> = merged
                            .iter()
                            .filter(|branch| matches_segment(branch, ticket))
                            .collect();
                        match named.as_slice() {
                            [] => Verdict::Keep(Keep::NotMerged),
                            [branch] => Verdict::Delete {
                                entry: entry.clone(),
                                branch: Some((*branch).clone()),
                            },
                            several => {
                                ambiguous.push(AmbiguousBranches {
                                    ticket: ticket.clone(),
                                    branches: several.iter().map(|b| (*b).clone()).collect(),
                                });
                                continue;
                            }
                        }
                    }
                    None if scope.ticket.is_none() && status == Some(Status::Approved) => {
                        Verdict::Keep(Keep::ApprovedUnscoped)
                    }
                    None => Verdict::Delete {
                        entry: entry.clone(),
                        branch: None,
                    },
                },
            },
        };
        items.push(Item {
            name,
            path,
            ticket,
            status,
            verdict,
        });
    }

    if !ambiguous.is_empty() {
        return Err(CleanError::AmbiguousBranches(ambiguous));
    }
    Ok(Assessment {
        plans_dir,
        plans_dir_exists: true,
        items,
        unreadable_entries,
    })
}

/// The one directory `ticket` names.
///
/// `plans::find` already refuses none and several. A directory whose name is
/// malformed, or that has no `spec.md`, still matched exactly once — and is
/// exactly what this command exists to look at — so those two are accepted
/// here and judged like any other.
fn named_plan(root: &ProjectRoot, ticket: &str) -> Result<String, CleanError> {
    match plans::find(root, ticket) {
        Ok(dir) => Ok(dir.name),
        Err(LookupError::MalformedName { path, .. } | LookupError::MissingSpec { path }) => {
            Ok(path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default())
        }
        Err(err) => Err(CleanError::Lookup(err)),
    }
}

/// The newest readable decision entry of every ticket, and how many entries
/// could not be read. The log comes newest first (decision-entry.md §9), so
/// the first entry seen for a ticket is its newest.
fn newest_entries(root: &ProjectRoot) -> Result<(BTreeMap<String, EntryRef>, usize), CleanError> {
    let mut newest = BTreeMap::new();
    let mut unreadable = 0;
    if let Some(listed) = decision::log(root).map_err(CleanError::Log)? {
        for item in listed {
            match item.entry {
                Ok(entry) => {
                    newest.entry(entry.ticket).or_insert(EntryRef {
                        id: entry.id,
                        kind: entry.kind,
                        date: entry.date,
                    });
                }
                Err(_) => unreadable += 1,
            }
        }
    }
    Ok((newest, unreadable))
}

/// The Plan's `Status`, when `spec.md` can be read well enough to say. Not
/// being able to is not an error: a Plan directory with a damaged or missing
/// `spec.md` is exactly the kind of leftover this command is for.
fn read_status(spec: &Path, mode: Mode) -> Option<Status> {
    let bytes = fs::read(spec).ok()?;
    let text = std::str::from_utf8(&bytes).ok()?;
    Plan::check(text, mode).status
}

// --- deleting --------------------------------------------------------------------

/// Delete every Plan directory `assessment` marked, and nothing else. Returns
/// the directories removed, in order.
///
/// Stops at the first failure and says what was already removed: a half-done
/// cleanup that reports itself is safe to run again, because `assess` will
/// not find the Plans that are gone.
pub fn delete(root: &ProjectRoot, assessment: &Assessment) -> Result<Vec<PathBuf>, CleanError> {
    let mut removed = Vec::new();
    for item in &assessment.items {
        if !matches!(item.verdict, Verdict::Delete { .. }) {
            continue;
        }
        let path = plan_directory(root, &item.name)?;
        // `remove_dir_all` does not follow a symbolic link: a Plan directory
        // that is one is unlinked, and what it pointed at is left alone.
        if let Err(source) = fs::remove_dir_all(&path) {
            return Err(CleanError::Remove {
                path,
                source,
                removed,
            });
        }
        removed.push(path);
    }
    Ok(removed)
}

/// `.invar/plans/<name>`, for a `name` that is exactly one path component. The
/// names `assess` produces always are; this is the check that stays true
/// whoever builds an [`Assessment`].
fn plan_directory(root: &ProjectRoot, name: &str) -> Result<PathBuf, CleanError> {
    let mut components = Path::new(name).components();
    match (components.next(), components.next()) {
        (Some(Component::Normal(_)), None) => Ok(root.plans_dir().join(name)),
        _ => Err(CleanError::NotAPlanName {
            name: name.to_owned(),
        }),
    }
}

// --- errors ----------------------------------------------------------------------

/// A ticket that names more than one merged branch.
#[derive(Debug, PartialEq, Eq)]
pub struct AmbiguousBranches {
    pub ticket: String,
    pub branches: Vec<String>,
}

#[derive(Debug)]
pub enum CleanError {
    /// The Plans, or the one a ticket names, could not be found or listed.
    Lookup(LookupError),
    /// The decision log could not be listed.
    Log(RecordError),
    /// `--merged`, and a ticket names several merged branches. Nothing was
    /// deleted.
    AmbiguousBranches(Vec<AmbiguousBranches>),
    /// A directory name that is not one path component: never joined to the
    /// Plans directory.
    NotAPlanName { name: String },
    /// A directory could not be removed.
    Remove {
        path: PathBuf,
        source: io::Error,
        /// What was removed before it.
        removed: Vec<PathBuf>,
    },
}

impl fmt::Display for CleanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Lookup(err) => write!(f, "{err}"),
            Self::Log(err) => write!(
                f,
                "cannot read the decision log, which `clean-spec` needs to know which \
                 cycles were recorded: {err}"
            ),
            Self::AmbiguousBranches(found) => {
                write!(
                    f,
                    "`--merged` cannot tell which branch is a ticket's, so nothing was deleted:"
                )?;
                for one in found {
                    write!(
                        f,
                        "\n  {} names {} merged branches: {}",
                        one.ticket,
                        one.branches.len(),
                        one.branches.join(", ")
                    )?;
                }
                write!(
                    f,
                    "\nA ticket on several merged branches may be several cycles, and \
                     invar does not guess which of them is over. Name the Plan \
                     (`invar clean-spec <ticket> --force`) when you know it is abandoned"
                )
            }
            Self::NotAPlanName { name } => write!(
                f,
                "{name:?} is not the name of a directory in .invar/plans/; nothing was \
                 deleted for it"
            ),
            Self::Remove {
                path,
                source,
                removed,
            } => {
                write!(f, "cannot remove {}: {source}.", path.display())?;
                if removed.is_empty() {
                    write!(f, " Nothing was removed before it.")
                } else {
                    write!(f, " Removed before it:")?;
                    for path in removed {
                        write!(f, "\n  {}", path.display())?;
                    }
                    Ok(())
                }
            }
        }
    }
}

// No `source()`: every variant's message already carries its cause, and
// `main` prints the whole chain, so exposing it would print it twice.
impl std::error::Error for CleanError {}
