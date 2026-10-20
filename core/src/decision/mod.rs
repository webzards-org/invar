//! The decision log, `.invar/decisions/` (§7): writing one entry, and reading
//! them all.
//!
//! Implements `contracts/formats/decision-entry.md` v1.2.0; the draft of
//! §8.1 is checked in [`draft`].
//!
//! **Insert-only** (§6). An entry is created with an exclusive create, so an
//! existing file can never be replaced; a name that is taken gets the next
//! suffix (D-3). Nothing in `.invar/decisions/` is ever modified or deleted.
//!
//! **Written on a person's decision** (§8, D-6). This module writes when it is
//! called; who may call it is the protocol's business, and the answer is: the
//! developer, or the agent when the developer asks — never a hook.
//!
//! **`failed` resets the cycle** (§7, §4c). The ticket's Plan returns to
//! `draft` with its approval values cleared, and its body untouched. The Plan
//! is written first and restored if the entry cannot be (D-8).

use std::collections::HashSet;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::hash::{BuildHasher as _, Hasher as _};
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::diagnostics::Diagnostics;
use crate::format::decision::{self, Entry, FileName, Type};
use crate::format::plan::{Mode, Plan, Status};
use crate::format::values::{is_module, is_ticket};
use crate::fs::ProjectRoot;
use crate::fs::plans::{self, LookupError};
use crate::git::Instant;

pub mod draft;

#[cfg(test)]
mod tests;

/// What to record.
#[derive(Debug, Clone)]
pub struct Request {
    pub ticket: String,
    pub kind: Type,
    /// Required when the ticket has no Plan; must agree with the Plan's
    /// `Module` when it has one.
    pub module: Option<String>,
    /// As given; normalized before writing.
    pub body: String,
}

/// What was written.
#[derive(Debug)]
pub struct Recorded {
    pub path: PathBuf,
    pub entry: Entry,
    /// The Plan returned to `draft`, for a `failed` entry.
    pub reset: Option<PathBuf>,
}

/// Write one entry for `request`, at `now` (git's clock, D-2).
pub fn record(
    root: &ProjectRoot,
    request: Request,
    now: Instant,
    mode: Mode,
) -> Result<Recorded, RecordError> {
    let Request {
        ticket,
        kind,
        module,
        body,
    } = request;
    if !is_ticket(&ticket) {
        return Err(RecordError::InvalidTicket { ticket });
    }
    if let Some(module) = &module
        && !is_module(module)
    {
        return Err(RecordError::InvalidModule {
            module: module.clone(),
        });
    }
    let body = decision::normalize_body(&body);
    if body.is_empty() {
        return Err(RecordError::EmptyBody);
    }

    // The ticket's Plan, if it has one: it supplies the module and the slug,
    // and it is what `failed` resets.
    let plan = match plans::find(root, &ticket) {
        Ok(dir) => {
            let spec = dir.spec_path();
            let text = fs::read_to_string(&spec).map_err(|source| RecordError::Io {
                path: spec.clone(),
                source,
            })?;
            Some((dir, spec, text))
        }
        Err(LookupError::NotFound { .. }) => None,
        Err(err) => return Err(RecordError::Lookup(err)),
    };

    let module = match (&plan, module) {
        (None, Some(module)) => module,
        (None, None) => return Err(RecordError::ModuleNeeded { ticket }),
        (Some((_, spec, text)), given) => match (Plan::check(text, mode).module, given) {
            (Some(from_plan), Some(given)) if from_plan != given => {
                return Err(RecordError::ModuleMismatch {
                    ticket,
                    spec: spec.clone(),
                    from_plan,
                    given,
                });
            }
            (Some(from_plan), _) => from_plan,
            (None, Some(given)) => given,
            (None, None) => {
                return Err(RecordError::ModuleUnreadable {
                    ticket,
                    spec: spec.clone(),
                });
            }
        },
    };
    let slug = plan
        .as_ref()
        .map_or_else(|| ticket.to_lowercase(), |(dir, _, _)| dir.slug.clone());

    // `failed`: return the Plan to `draft` before writing anything else.
    let reset = if kind == Type::Failed {
        let Some((_, spec, text)) = &plan else {
            return Err(RecordError::NoPlanToReset {
                ticket,
                plans_dir: root.plans_dir(),
            });
        };
        reset_plan(spec, text, mode)?;
        Some(spec.clone())
    } else {
        None
    };

    let dir = root.decisions_dir();
    let written = fs::create_dir_all(&dir)
        .map_err(|source| RecordError::Io {
            path: dir.clone(),
            source,
        })
        .and_then(|()| {
            let entry = Entry {
                id: new_id(&existing_ids(&dir)),
                date: now.local_rfc3339(),
                ticket: ticket.clone(),
                module: module.clone(),
                kind,
                scenario: None,
                body,
            };
            let name = FileName {
                stamp: now.utc_stamp(),
                module,
                slug,
            };
            let path = create_new(&dir, &name, &entry.render())?;
            Ok((path, entry))
        });

    match written {
        Ok((path, entry)) => Ok(Recorded { path, entry, reset }),
        Err(err) => {
            // D-8: no reset Plan without its entry.
            if let (Some(spec), Some((_, _, original))) = (&reset, &plan)
                && let Err(source) = restore_plan(spec, original, mode)
            {
                return Err(RecordError::RestoreFailed {
                    spec: spec.clone(),
                    entry_error: err.to_string(),
                    source,
                });
            }
            Err(err)
        }
    }
}

/// `Status: draft`, the approval values cleared, the body byte-identical —
/// checked by reading the file back.
fn reset_plan(spec: &Path, original: &str, mode: Mode) -> Result<(), RecordError> {
    let mut plan = Plan::parse(original, mode).map_err(|problems| RecordError::PlanInvalid {
        spec: spec.to_owned(),
        problems,
    })?;
    let body = plan.body().to_owned();
    plan.reset_to_draft();
    let text = plan.to_text();
    if text == original {
        return Ok(());
    }
    plan.write(spec).map_err(|source| RecordError::Io {
        path: spec.to_owned(),
        source,
    })?;

    let preserved = fs::read_to_string(spec).is_ok_and(|written| {
        written == text
            && Plan::parse(&written, mode).is_ok_and(|reread| {
                reread.body() == body
                    && reread.header().status == Status::Draft
                    && reread.header().approved_checksum.is_none()
            })
    });
    if preserved {
        Ok(())
    } else {
        // Put the original back rather than leave a Plan nobody can trust.
        let _ = restore_plan(spec, original, mode);
        Err(RecordError::PlanNotPreserved {
            spec: spec.to_owned(),
        })
    }
}

/// Put the Plan back exactly as it was, by the same atomic write that changed
/// it.
///
/// A plain `fs::write` truncates the file before it writes: a restore
/// interrupted halfway would leave the very half-written Plan that D-8 exists
/// to prevent. `original` parsed when the Plan was reset, and a Plan
/// reproduces its own text byte for byte, so parsing it again gives the file
/// back; `Plan::write` then replaces it atomically.
fn restore_plan(spec: &Path, original: &str, mode: Mode) -> io::Result<()> {
    match Plan::parse(original, mode) {
        Ok(plan) if plan.to_text() == original => plan.write(spec),
        // Not expected: it parsed a moment ago, and a Plan round-trips. A
        // plain write is still better than leaving the Plan reset.
        _ => fs::write(spec, original),
    }
}

/// Create `name` in `dir` exclusively, trying `-2`, `-3`, … when it is taken
/// (D-3). Never replaces a file.
fn create_new(dir: &Path, name: &FileName, text: &str) -> Result<PathBuf, RecordError> {
    for suffix in 1..=1000 {
        let path = dir.join(name.render(suffix));
        let file = OpenOptions::new().write(true).create_new(true).open(&path);
        let mut file = match file {
            Ok(file) => file,
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(source) => return Err(RecordError::Io { path, source }),
        };
        let written = file
            .write_all(text.as_bytes())
            .and_then(|()| file.sync_all());
        return match written {
            Ok(()) => Ok(path),
            Err(source) => {
                // Our own half-written file from this very call, never an
                // existing entry.
                drop(file);
                let _ = fs::remove_file(&path);
                Err(RecordError::Io { path, source })
            }
        };
    }
    Err(RecordError::NoFreeName {
        dir: dir.to_owned(),
        name: name.render(1),
    })
}

/// Every `id` already in the log, read loosely: a hand-edited entry that no
/// longer parses still reserves its id.
fn existing_ids(dir: &Path) -> HashSet<String> {
    let Ok(entries) = fs::read_dir(dir) else {
        return HashSet::new();
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|entry| fs::read_to_string(entry.path()).ok())
        .filter_map(|text| {
            text.lines().take(40).find_map(|line| {
                line.trim()
                    .strip_prefix("id:")
                    .map(|id| id.trim().to_owned())
            })
        })
        .collect()
}

/// `d-` and eight random hex digits, not in `taken` (D-4).
fn new_id(taken: &HashSet<String>) -> String {
    new_id_from(taken, random_u32)
}

/// [`new_id`] with its source of numbers given, so the retry is testable.
fn new_id_from(taken: &HashSet<String>, mut next: impl FnMut() -> u32) -> String {
    loop {
        let id = format!("d-{:08x}", next());
        if !taken.contains(&id) {
            return id;
        }
    }
}

/// Not cryptographic, and need not be: the id must be unpredictable enough
/// not to collide, and `new_id` excludes the ones that would. The standard
/// library's hasher is seeded randomly per process; the time, the process id
/// and a counter make every call different.
fn random_u32() -> u32 {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    hasher.write_u128(nanos);
    hasher.write_u32(std::process::id());
    hasher.write_u64(COUNTER.fetch_add(1, Ordering::Relaxed));
    let hash = hasher.finish();
    (hash ^ (hash >> 32)) as u32
}

// --- reading ------------------------------------------------------------------------

/// One file of the log, parsed or not.
#[derive(Debug)]
pub struct Listed {
    pub path: PathBuf,
    pub entry: Result<Entry, Diagnostics>,
}

/// Every entry, newest first (§9); `None` when `.invar/decisions/` does not
/// exist. Files that are not `.md`, and hidden ones (`.gitkeep`), are not
/// entries.
pub fn log(root: &ProjectRoot) -> Result<Option<Vec<Listed>>, RecordError> {
    let dir = root.decisions_dir();
    let unreadable = |source| RecordError::Io {
        path: dir.clone(),
        source,
    };
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(unreadable(err)),
    };
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry.map_err(unreadable)?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || !name.ends_with(".md") || !entry.path().is_file() {
            continue;
        }
        names.push(name);
    }
    // The filename orders the log (§9): newest first.
    names.sort_unstable_by_key(|name| std::cmp::Reverse(order_key(name)));
    Ok(Some(
        names
            .into_iter()
            .map(|name| {
                let path = dir.join(&name);
                let entry = match fs::read_to_string(&path) {
                    Ok(text) => decision::check(&name, &text),
                    Err(err) => {
                        let mut problems = Diagnostics::new();
                        problems.general(format!("cannot read the file: {err}"));
                        Err(problems)
                    }
                };
                Listed { path, entry }
            })
            .collect(),
    ))
}

/// The order of §9: the name without `.md`, with a trailing run of digits
/// compared as a number — so `x-2` follows `x`, and `x-10` follows `x-9`.
pub fn order_key(name: &str) -> (String, u64) {
    let stem = name.strip_suffix(".md").unwrap_or(name);
    let prefix = stem.trim_end_matches(|c: char| c.is_ascii_digit());
    let number = stem[prefix.len()..].parse().unwrap_or(0);
    (prefix.to_owned(), number)
}

// --- errors -------------------------------------------------------------------------

#[derive(Debug)]
pub enum RecordError {
    InvalidTicket {
        ticket: String,
    },
    InvalidModule {
        module: String,
    },
    EmptyBody,
    /// The Plan lookup failed in a way other than "no Plan".
    Lookup(LookupError),
    /// No Plan, and no `--module`.
    ModuleNeeded {
        ticket: String,
    },
    /// The Plan's `Module` cannot be read, and no `--module`.
    ModuleUnreadable {
        ticket: String,
        spec: PathBuf,
    },
    ModuleMismatch {
        ticket: String,
        spec: PathBuf,
        from_plan: String,
        given: String,
    },
    /// `failed` without a Plan to reset.
    NoPlanToReset {
        ticket: String,
        plans_dir: PathBuf,
    },
    /// `failed` on a Plan that is not valid, so it cannot be rewritten.
    PlanInvalid {
        spec: PathBuf,
        problems: Diagnostics,
    },
    PlanNotPreserved {
        spec: PathBuf,
    },
    Io {
        path: PathBuf,
        source: io::Error,
    },
    NoFreeName {
        dir: PathBuf,
        name: String,
    },
    /// The entry could not be written and the Plan could not be restored.
    RestoreFailed {
        spec: PathBuf,
        entry_error: String,
        source: io::Error,
    },
}

impl fmt::Display for RecordError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidTicket { ticket } => write!(
                f,
                "`{ticket}` is not a ticket id; give one such as `PROJ-412` (capital letters, \
                 a dash, digits)"
            ),
            Self::InvalidModule { module } => write!(
                f,
                "`{module}` is not a module name; give a lowercase name such as `orders` \
                 (letters, digits, `_`), or `global`"
            ),
            Self::EmptyBody => write!(
                f,
                "the entry has no text; say what was decided and why, with `--message` or on \
                 stdin"
            ),
            Self::Lookup(err) => write!(f, "{err}"),
            Self::ModuleNeeded { ticket } => write!(
                f,
                "there is no Plan for {ticket}, so the module cannot be read from it; pass \
                 `--module <module>`, or `--module global` for an entry not tied to one \
                 module"
            ),
            Self::ModuleUnreadable { ticket, spec } => write!(
                f,
                "the Plan for {ticket} ({}) has no readable `Module`; pass \
                 `--module <module>`, or fix the Plan's header",
                spec.display()
            ),
            Self::ModuleMismatch {
                ticket,
                spec,
                from_plan,
                given,
            } => write!(
                f,
                "`--module {given}` disagrees with the Plan for {ticket}, which says \
                 `Module: {from_plan}` ({}); leave `--module` out to use the Plan's, or \
                 correct the one that is wrong",
                spec.display()
            ),
            Self::NoPlanToReset { ticket, plans_dir } => write!(
                f,
                "there is no Plan for {ticket} in {}, so a `failed` entry has nothing to \
                 reset: it returns the ticket's Plan to `draft`, and that is part of what it \
                 records. Record the failure with `--type note` instead",
                plans_dir.display()
            ),
            Self::PlanInvalid { spec, problems } => {
                let count = problems.len();
                let noun = if count == 1 { "problem" } else { "problems" };
                write!(
                    f,
                    "{} is not a valid Plan ({count} {noun}), so it cannot be returned to \
                     `draft`; nothing was written. Fix it — `invar plan validate` lists the \
                     problems — then record the entry again:\n{}",
                    spec.display(),
                    problems.render(spec).trim_end()
                )
            }
            Self::PlanNotPreserved { spec } => write!(
                f,
                "{} did not read back as written when it was reset, so it was restored and \
                 nothing was recorded — this is a bug in invar, please report it",
                spec.display()
            ),
            Self::Io { path, source } => write!(f, "{}: {source}", path.display()),
            Self::NoFreeName { dir, name } => write!(
                f,
                "every name from {name} to its 1000th suffix is taken in {}; wait a second \
                 and record again",
                dir.display()
            ),
            Self::RestoreFailed {
                spec,
                entry_error,
                source,
            } => write!(
                f,
                "the entry could not be written ({entry_error}), and {} could not be put \
                 back as it was ({source}): the Plan is now `draft` without a `failed` entry. \
                 Restore it with git, or record the entry again",
                spec.display()
            ),
        }
    }
}

impl std::error::Error for RecordError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Lookup(err) => Some(err),
            Self::Io { source, .. } | Self::RestoreFailed { source, .. } => Some(source),
            _ => None,
        }
    }
}
