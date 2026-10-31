//! `invar init` (M1-15): scaffolding a project for light mode.
//!
//! **The directory `init` runs in is the project root** — the directory that
//! will hold `invar.yaml`, which is how every other command finds it. In a
//! repository with one project that is the repository root; in a monorepo it is
//! the project's own directory. The caller checks that it is inside a git
//! work tree; this module only ever works relative to the directory it is
//! given.
//!
//! What it makes:
//!
//! - `invar.yaml`, with `scenarios: false` written explicitly (L-2);
//! - `.invar/plans/` and `.invar/decisions/`;
//! - `AI_CONTEXT.md` and `BUSINESS_CONTEXT.md`: the marker and the section
//!   headings, **never content** (§5, §5a). The headings are language-neutral:
//!   whether a team splits them into backend and frontend parts is its decision,
//!   not the tool's. Written empty on purpose — an empty marked file behaves
//!   exactly like an absent one (§5), so scaffolding them switches nothing on
//!   that the team did not ask for;
//! - `.gitattributes`: Plan line endings pinned (§4d);
//! - `.gitignore`: `.invar/out/`, where `invar done` renders the PR description;
//! - `.claude/settings.json`: a deny rule for reading `.invar/decisions/**`
//!   (§13a). The planner receives the log through `invar context show`, not by
//!   reading files, so nothing the planner needs is lost by denying the Read
//!   tool.
//!
//! # Three promises
//!
//! **Merging, never clobbering.** `.gitattributes`, `.gitignore` and
//! `.claude/settings.json` usually exist already in a real project. They are
//! added to, and everything in them stays where it was ([`lines`],
//! [`settings`]). Files that are the team's from the first moment —
//! `invar.yaml` and the two context files — are never touched once they exist.
//!
//! **Idempotent.** A second run changes nothing, and says so per file.
//!
//! **Planned before it is written.** Every existing file is read, and every
//! change computed, before the first byte is written. A `settings.json` that
//! cannot be merged into stops `init` with nothing created, rather than leaving
//! a half-scaffolded project behind a refusal. What can still go wrong
//! mid-write — a full disk — is safe to leave: running `init` again finishes
//! what was left undone.

mod lines;
mod settings;

use std::fmt;
use std::fs;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};

use crate::context::{self, MARKER};
use crate::fs::ProjectRoot;
use settings::SettingsError;

#[cfg(test)]
mod tests;

/// The rule written to `.claude/settings.json` (§13a).
///
/// `/` anchors a Read rule at the project root. Verified against Claude Code
/// 2.1.300 (`plugin/NOTES.md` §12, §17): it denies `Read`, `Grep`, `Glob` and
/// `cat`/`head` through `Bash`, in the main session and in sub-agents, and keeps
/// holding after a `cd` into a subdirectory. It does **not** stop an interpreter
/// or `grep -r` through `Bash`, and it exists only when Claude Code is *started*
/// at the project root: started in a subdirectory, this settings file is not
/// loaded. A speed bump for the log, not a wall (NOTES D-17.3).
pub const DENY_RULE: &str = "Read(/.invar/decisions/**)";

const PLANS_DIR: &str = ".invar/plans";
const DECISIONS_DIR: &str = ".invar/decisions";
const GITATTRIBUTES: &str = ".gitattributes";
const GITIGNORE: &str = ".gitignore";
const SETTINGS: &str = ".claude/settings.json";

/// `invar.yaml` as `init` writes it: `scenarios: false` stated outright, the
/// optional keys shown commented out. The core reads this file and never
/// writes it back, so it is the team's from the first run.
pub const INVAR_YAML: &str = "\
# invar.yaml — written by `invar init`, and yours from here on: invar reads it and never
# writes it back. Every key is described in contracts/formats/invar-yaml.md in the invar
# repository.

# The shape of a Plan. `false` is light mode: acceptance criteria live in the Plan, and
# there are no .feature files.
scenarios: false

# Uncomment to change a default.
#
# review:
#   offer: false            # default true: offer /invar:review before `invar done`
#
# git:
#   base_branch: develop    # default: the branch origin/HEAD points to, else main
#
# context:
#   binding_limit: 9000     # characters, at most 10000: what a SessionStart hook may deliver

# Acceptance tests written by the test-writer before the coder starts, and locked so the
# coder cannot change them. Uncomment to turn on; every ticket then needs them unless
# approved with --no-acceptance.
#
# acceptance:
#   required: true          # default true: the default for `plan approve`
#   dir: tests/Acceptance
#   run: [\"vendor/bin/pest\", \"--log-junit\", \"{report}\", \"{files}\"]
#   fakes_allowed: [http, mail, queue, notification]
";

/// `AI_CONTEXT.md`: this project's own conventions (§5a). The marker and the
/// headings, nothing under them.
pub fn ai_context() -> String {
    format!(
        "{MARKER}\n# AI_CONTEXT.md\n\n## Layers and boundaries\n\n## Naming\n\n\
         ## Module boundaries\n\n## Tests\n"
    )
}

/// `BUSINESS_CONTEXT.md`: business rules the application must obey (§5). The
/// marker and the two headings of §5, nothing under them.
pub fn business_context() -> String {
    format!(
        "{MARKER}\n# BUSINESS_CONTEXT.md\n\n## Invariants (always true, no exceptions)\n\n\
         ## Regional / parameterized facts\n"
    )
}

// --- the report -------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    Created,
    /// Existed, and something was added to it.
    Updated,
    Unchanged,
}

/// What happened to one file or directory.
#[derive(Debug)]
pub struct Item {
    /// Relative to the project root; directories end in `/`.
    pub path: String,
    pub change: Change,
    /// What was added, for an update.
    pub note: Option<String>,
}

#[derive(Debug)]
pub struct Report {
    pub items: Vec<Item>,
    /// Things worth a person's attention that `init` does not change on its
    /// own: a context file the marker is missing from, a `.gitignore` that
    /// hides `.invar/`.
    pub warnings: Vec<String>,
}

impl Report {
    /// Did this run change anything?
    pub fn changed(&self) -> bool {
        self.items
            .iter()
            .any(|item| item.change != Change::Unchanged)
    }
}

// --- doing it ---------------------------------------------------------------------------

/// Scaffold the project in `dir`.
pub fn apply(dir: &Path) -> Result<Report, InitError> {
    // 1. Read what is there, and work out every change — refusing, with
    //    nothing written, if any of them cannot be made.
    let gitattributes = Existing::read(dir, GITATTRIBUTES)?;
    let gitignore = Existing::read(dir, GITIGNORE)?;
    let settings_file = Existing::read(dir, SETTINGS)?;

    let new_gitattributes = lines::pin_plan_line_endings(gitattributes.text.as_deref());
    let new_gitignore = lines::ignore_out_dir(gitignore.text.as_deref());
    let new_settings =
        settings::add_deny_rule(settings_file.text.as_deref(), DENY_RULE).map_err(|error| {
            InitError::Settings {
                path: SETTINGS,
                error,
            }
        })?;

    let plans = Planned::dir(dir, PLANS_DIR)?;
    let decisions = Planned::dir(dir, DECISIONS_DIR)?;
    let invar_yaml = Planned::file(dir, "invar.yaml")?;
    let ai = Planned::file(dir, "AI_CONTEXT.md")?;
    let business = Planned::file(dir, "BUSINESS_CONTEXT.md")?;

    // 2. Write.
    let mut items = vec![
        plans.make_dir()?,
        decisions.make_dir()?,
        invar_yaml.make_file(INVAR_YAML)?,
        ai.make_file(&ai_context())?,
        business.make_file(&business_context())?,
    ];
    items.push(merged(
        &gitattributes,
        new_gitattributes,
        &format!("added `{}`", lines::GITATTRIBUTES_ENTRY),
    )?);
    items.push(merged(
        &gitignore,
        new_gitignore,
        &format!("added `{}`", lines::GITIGNORE_ENTRY),
    )?);
    items.push(merged(
        &settings_file,
        new_settings,
        &format!("added the deny rule `{DENY_RULE}`"),
    )?);

    Ok(Report {
        items,
        warnings: warnings(dir, gitignore.text.as_deref()),
    })
}

/// What `init` notices and does not fix.
fn warnings(dir: &Path, gitignore: Option<&str>) -> Vec<String> {
    let mut warnings = Vec::new();

    if gitignore.is_some_and(lines::hides_invar_dir) {
        warnings.push(
            ".gitignore ignores the whole of `.invar/`, which would keep the Plans and the \
             decision log out of git. Plans live in git so that a reviewer reads them (§12): \
             ignore `.invar/out/` only"
                .to_owned(),
        );
    }

    // A context file the team already had, without the marker, is not loaded.
    // Say so now, not when an agent turns out never to have seen it.
    match ProjectRoot::discover_from(dir)
        .map_err(|err| err.to_string())
        .and_then(|root| context::load_binding(&root).map_err(|err| err.to_string()))
    {
        Ok(binding) => {
            for name in binding.unrecognized() {
                warnings.push(format!(
                    "{name} already exists without the marker, so `invar context show` does not \
                     load it. Add `{MARKER}` as its first line to have it delivered; `init` does \
                     not edit it"
                ));
            }
        }
        Err(problem) => warnings.push(format!("could not check the context files: {problem}")),
    }
    warnings
}

/// A file that is merged into, as it was found.
struct Existing {
    display: &'static str,
    path: PathBuf,
    /// `None` when there is no such file.
    text: Option<String>,
}

impl Existing {
    fn read(dir: &Path, display: &'static str) -> Result<Self, InitError> {
        let path = dir.join(display);
        let text = match fs::read(&path) {
            Ok(bytes) => {
                Some(String::from_utf8(bytes).map_err(|_| InitError::NotUtf8 { path: display })?)
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => None,
            Err(_) if path.is_dir() => {
                return Err(InitError::NotAFile {
                    path: display.to_owned(),
                });
            }
            Err(source) => return Err(InitError::Io { path, source }),
        };
        Ok(Self {
            display,
            path,
            text,
        })
    }
}

/// Write `new` to the file `existing` describes: create it, replace it, or —
/// when there is nothing to add — leave it alone.
fn merged(existing: &Existing, new: Option<String>, added: &str) -> Result<Item, InitError> {
    let item = |change, note| Item {
        path: existing.display.to_owned(),
        change,
        note,
    };
    let Some(text) = new else {
        return Ok(item(Change::Unchanged, None));
    };
    let io_error = |source| InitError::Io {
        path: existing.path.clone(),
        source,
    };
    if existing.text.is_none() {
        create_new(&existing.path, &text).map_err(io_error)?;
        Ok(item(Change::Created, None))
    } else {
        replace_file(&existing.path, &text).map_err(io_error)?;
        Ok(item(Change::Updated, Some(added.to_owned())))
    }
}

/// Something `init` scaffolds and then leaves to the team.
struct Planned {
    display: String,
    path: PathBuf,
    exists: bool,
}

impl Planned {
    fn dir(dir: &Path, display: &str) -> Result<Self, InitError> {
        let planned = Self::look(dir, display)?;
        if planned.exists && !planned.path.is_dir() {
            return Err(InitError::NotADirectory {
                path: display.to_owned(),
            });
        }
        Ok(planned)
    }

    fn file(dir: &Path, display: &str) -> Result<Self, InitError> {
        let planned = Self::look(dir, display)?;
        if planned.exists && planned.path.is_dir() {
            return Err(InitError::NotAFile {
                path: display.to_owned(),
            });
        }
        Ok(planned)
    }

    fn look(dir: &Path, display: &str) -> Result<Self, InitError> {
        let path = dir.join(display);
        let exists = match path.symlink_metadata() {
            Ok(_) => true,
            Err(err) if err.kind() == io::ErrorKind::NotFound => false,
            Err(source) => return Err(InitError::Io { path, source }),
        };
        Ok(Self {
            display: display.to_owned(),
            path,
            exists,
        })
    }

    /// The directory, made if it was not there.
    fn make_dir(&self) -> Result<Item, InitError> {
        let change = if self.exists {
            Change::Unchanged
        } else {
            fs::create_dir_all(&self.path).map_err(|source| InitError::Io {
                path: self.path.clone(),
                source,
            })?;
            Change::Created
        };
        Ok(Item {
            path: format!("{}/", self.display),
            change,
            note: None,
        })
    }

    /// The file with `text`, if it was not there. One that exists is the
    /// team's: left alone, whatever it holds.
    fn make_file(&self, text: &str) -> Result<Item, InitError> {
        let change = if self.exists {
            Change::Unchanged
        } else {
            create_new(&self.path, text).map_err(|source| InitError::Io {
                path: self.path.clone(),
                source,
            })?;
            Change::Created
        };
        Ok(Item {
            path: self.display.clone(),
            change,
            note: None,
        })
    }
}

/// A new file, never over an existing one: `create_new` refuses if anything is
/// there, whatever happened since it was looked at.
fn create_new(path: &Path, text: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(text.as_bytes())
}

/// Replace the contents of a file that exists, all at once: written beside it
/// and renamed over it, so an interruption leaves the old file or the new one,
/// never half of either.
///
/// A settings file is often a symlink into a dotfiles repository, so what is
/// replaced is what the link points at, not the link; and the file keeps its
/// permissions.
fn replace_file(path: &Path, text: &str) -> io::Result<()> {
    let target = path.canonicalize()?;
    let permissions = fs::metadata(&target)?.permissions();
    let name = target
        .file_name()
        .ok_or_else(|| io::Error::other("the path has no file name"))?
        .to_string_lossy()
        .into_owned();
    let temp = target.with_file_name(format!(".{name}.invar-tmp"));

    // A leftover of an earlier interrupted run is ours to remove.
    let _ = fs::remove_file(&temp);
    let written = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        fs::set_permissions(&temp, permissions)?;
        fs::rename(&temp, &target)
    })();
    if written.is_err() {
        let _ = fs::remove_file(&temp);
    }
    written
}

// --- errors --------------------------------------------------------------------------------

/// Why `init` stopped. Every variant but [`InitError::Io`] is a refusal made
/// **before anything was written**.
#[derive(Debug)]
pub enum InitError {
    /// A file `init` merges into is not UTF-8.
    NotUtf8 {
        path: &'static str,
    },
    /// A path that should be a file is a directory.
    NotAFile {
        path: String,
    },
    /// A path that should be a directory is not one.
    NotADirectory {
        path: String,
    },
    /// `.claude/settings.json` cannot be merged into.
    Settings {
        path: &'static str,
        error: SettingsError,
    },
    Io {
        path: PathBuf,
        source: io::Error,
    },
}

impl fmt::Display for InitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        const NOTHING: &str = "Nothing was changed.";
        match self {
            Self::NotUtf8 { path } => write!(
                f,
                "{path} is not valid UTF-8, so `init` cannot add to it safely. {NOTHING}"
            ),
            Self::NotAFile { path } => {
                write!(
                    f,
                    "{path} is a directory, where `init` needs a file. {NOTHING}"
                )
            }
            Self::NotADirectory { path } => {
                write!(f, "{path} exists and is not a directory. {NOTHING}")
            }
            Self::Settings { path, error } => write!(f, "{path} {error}. {NOTHING}"),
            Self::Io { path, source } => write!(
                f,
                "cannot read or write {}: {source}. `init` is safe to run again: it finishes \
                 what was left undone and leaves the rest as it is",
                path.display()
            ),
        }
    }
}

// No `source()`: every variant's message already carries its cause, and
// `main` prints the whole chain, so exposing it would print it twice.
impl std::error::Error for InitError {}
