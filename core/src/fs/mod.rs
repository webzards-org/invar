//! Path resolution and project-tree discovery.
//!
//! Everything the core reads or writes is addressed relative to the **project
//! root**: `invar.yaml`, `.invar/plans/`, `.invar/decisions/`, `.invar/out/`,
//! and later `specs/` and the feature paths recorded in test files (§4b). One
//! module owns that resolution so no command invents its own.
//!
//! - **The project root is the directory containing `invar.yaml`**, found by
//!   walking upward from the current directory ([`project_root`]). A hook
//!   fires with whatever `cwd` Claude Code gives it; a command run from
//!   `app/Http/` must behave exactly as one run from the root, or the same
//!   check produces different verdicts depending on who ran it.
//! - **The walk stops at a `.git` boundary**, and at the filesystem root. A
//!   repository without its own `invar.yaml` must never borrow one from a
//!   directory above it — an unrelated project's config, or a stray file in
//!   the home directory, would silently decide this project's verdicts.
//! - **Paths are compared in canonical form**: symlinks resolved, `.` and `..`
//!   removed. A Plan reached through a symlink, or through `app/../`, is the
//!   same Plan ([`ProjectRoot::relative_path`]).
//! - **[`ProjectRoot`] is the only way to a path inside the project.** It can
//!   be obtained only by discovery, so a command cannot build paths from an
//!   unchecked directory.
//! - **Ambiguity is a hard error.** Plan lookup (§4a) and, later, test-file
//!   lookup (§4b) fail loudly on zero or on several matches. `invar` never
//!   guesses which artifact was meant.
//!
//! Arrived with M1-4. Plan lookup (M1-7) and `init` (M1-15) build on it.

// The plan and decision commands (M1-8 to M1-11) use it; the output
// directory and `join` are for later commands. `expect` rather than
// `allow`, so the compiler asks for this line to go once they are used.
#![cfg_attr(
    not(test),
    expect(dead_code, reason = "out_dir and join are used by later commands")
)]

use std::ffi::OsString;
use std::fmt;
use std::io;
use std::path::{Component, Path, PathBuf};

use crate::format::config;

pub mod plans;

#[cfg(test)]
mod tests;

/// The directory that marks a git repository's root. A file of this name
/// counts too: worktrees and submodules have a `.git` file, not a directory.
const GIT_MARKER: &str = ".git";

/// `invar`'s own state directory, under the project root.
const INVAR_DIR: &str = ".invar";

// --- discovery --------------------------------------------------------------

/// The project root, found from the current directory.
pub fn project_root() -> Result<ProjectRoot, RootError> {
    let cwd = std::env::current_dir().map_err(RootError::CurrentDir)?;
    ProjectRoot::discover_from(&cwd)
}

/// The directory containing `invar.yaml`, in canonical form.
///
/// Every path inside the project is built through this type, and the type
/// can only be obtained by discovery. That is what keeps a command from
/// resolving `.invar/plans/` against whatever directory it happened to start
/// in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectRoot {
    /// Canonical: absolute, symlinks resolved, no `.` or `..`.
    path: PathBuf,
}

impl ProjectRoot {
    /// Walk upward from `start` to the nearest directory containing
    /// `invar.yaml`.
    ///
    /// `start` may be relative (to the current directory) or a file, in which
    /// case the walk begins in the file's directory. The walk stops — and
    /// fails — at the first directory that holds `.git` without
    /// `invar.yaml`, or at the filesystem root.
    pub fn discover_from(start: &Path) -> Result<Self, RootError> {
        let canonical = start.canonicalize().map_err(|source| RootError::Start {
            path: start.to_owned(),
            source,
        })?;
        let mut dir = if canonical.is_dir() {
            canonical
        } else {
            match canonical.parent() {
                Some(parent) => parent.to_owned(),
                None => canonical,
            }
        };

        let mut searched = Vec::new();
        loop {
            // `invar.yaml` before `.git`: a repository root that holds the
            // config is the common case, and it is a match, not a boundary.
            if entry_exists(&dir, config::FILE_NAME)? {
                return Ok(Self { path: dir });
            }
            searched.push(dir.clone());

            if entry_exists(&dir, GIT_MARKER)? {
                return Err(RootError::NotFound {
                    searched,
                    boundary: Boundary::GitRoot(dir),
                });
            }

            match dir.parent() {
                Some(parent) => dir = parent.to_owned(),
                None => {
                    return Err(RootError::NotFound {
                        searched,
                        boundary: Boundary::FilesystemRoot(dir),
                    });
                }
            }
        }
    }

    /// A root at `path`, without discovery. Tests only: production code gets
    /// a root from [`project_root`] or [`ProjectRoot::discover_from`].
    #[cfg(test)]
    pub(crate) fn for_test(path: &Path) -> Self {
        Self {
            path: path.canonicalize().expect("test root exists"),
        }
    }

    // --- paths inside the project ------------------------------------------

    /// The root directory itself, canonical.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// `invar.yaml`.
    pub fn config_file(&self) -> PathBuf {
        self.path.join(config::FILE_NAME)
    }

    /// `.invar/plans/` — one directory per Plan (§4a, §12).
    pub fn plans_dir(&self) -> PathBuf {
        self.path.join(INVAR_DIR).join("plans")
    }

    /// `.invar/decisions/` — the decision log (§7).
    pub fn decisions_dir(&self) -> PathBuf {
        self.path.join(INVAR_DIR).join("decisions")
    }

    /// `.invar/out/` — working files such as the PR description (L-6).
    pub fn out_dir(&self) -> PathBuf {
        self.path.join(INVAR_DIR).join("out")
    }

    /// Join a path that is relative to the project root, as recorded in a
    /// file (§4b). `None` when it is absolute or climbs out with `..`: a path
    /// read from a file must not be able to point outside the project.
    pub fn join(&self, relative: &Path) -> Option<PathBuf> {
        let stays_inside = relative
            .components()
            .all(|part| matches!(part, Component::Normal(_) | Component::CurDir));
        stays_inside.then(|| self.path.join(relative))
    }

    /// Where `path` sits inside the project, relative to the root; `None` when
    /// it is outside.
    ///
    /// Compared in canonical form, so a symlink into the project, a path
    /// through `..`, and the plain path all give the same answer. `path` need
    /// not exist yet — the `Write` tool reports a file it is about to create
    /// (M1-10) — but its nearest existing ancestor is resolved, symlinks
    /// included. A symbolic link that points at something that does not exist
    /// is followed too: a write through it creates the file at its target, so
    /// that is the path that is reported. A relative `path` is taken relative
    /// to the current directory.
    pub fn relative_path(&self, path: &Path) -> io::Result<Option<PathBuf>> {
        let canonical = canonicalize_lenient(path)?;
        Ok(canonical
            .strip_prefix(&self.path)
            .ok()
            .map(Path::to_path_buf))
    }
}

impl AsRef<Path> for ProjectRoot {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}

/// Whether `dir` holds an entry called `name`, of any kind.
///
/// Any entry counts — a directory named `invar.yaml` still marks the root, and
/// reading the config then reports it as unreadable, which is clearer than
/// walking past it to a different project. A permission error is an error:
/// "cannot tell" must not be read as "not here".
fn entry_exists(dir: &Path, name: &str) -> Result<bool, RootError> {
    let path = dir.join(name);
    match path.symlink_metadata() {
        Ok(_) => Ok(true),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(source) => Err(RootError::Inaccessible { path, source }),
    }
}

/// Where `path` points, when `path` is itself a symbolic link: the link's
/// target, resolved against the link's directory when it is relative (an
/// absolute target replaces the directory, as in `Path::join`). `None` when
/// `path` is not a symlink, or does not exist.
fn link_target(path: &Path) -> io::Result<Option<PathBuf>> {
    match path.symlink_metadata() {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            let target = path.read_link()?;
            Ok(Some(match path.parent() {
                Some(parent) => parent.join(target),
                None => target,
            }))
        }
        Ok(_) => Ok(None),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err),
    }
}

/// The most symbolic links followed while resolving one path: the limit the
/// operating system itself applies. A bound, not an expectation — a link that
/// loops makes `canonicalize` fail with an error of its own long before.
const MAX_LINKS: usize = 40;

/// Canonicalize a path that may not exist yet.
///
/// The longest existing ancestor is canonicalized by the operating system, so
/// symlinks are resolved where they can exist. The remaining components do
/// not exist, so they cannot be symlinks, and are normalized lexically.
///
/// **A symlink whose target does not exist is not "a name that does not
/// exist".** The operating system reports both as `NotFound`, but a write
/// through the link creates the file at its target. Treating the link as a
/// plain missing name would report a path inside the project for a write that
/// lands outside it, or inside a protected directory under another name. So
/// such a link is followed, one step at a time, to where it points.
fn canonicalize_lenient(path: &Path) -> io::Result<PathBuf> {
    enum Part {
        Name(OsString),
        Up,
    }

    let mut existing = std::path::absolute(path)?;
    let mut missing = Vec::new();
    let mut links = 0;
    loop {
        match existing.canonicalize() {
            Ok(mut resolved) => {
                for part in missing.into_iter().rev() {
                    match part {
                        Part::Name(name) => resolved.push(name),
                        Part::Up => {
                            resolved.pop();
                        }
                    }
                }
                return Ok(resolved);
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                // A link that points nowhere fails here like a missing name.
                // Follow it; what was peeled off below it stays pending.
                if let Some(target) = link_target(&existing)? {
                    links += 1;
                    if links > MAX_LINKS {
                        return Err(io::Error::other("too many levels of symbolic links"));
                    }
                    existing = target;
                    continue;
                }
                match existing.components().next_back() {
                    Some(Component::Normal(name)) => missing.push(Part::Name(name.to_owned())),
                    Some(Component::ParentDir) => missing.push(Part::Up),
                    Some(Component::CurDir) => {}
                    // Only the root or a drive prefix is left, and even that
                    // does not exist: nothing to resolve against.
                    _ => return Err(err),
                }
                if !existing.pop() {
                    return Err(err);
                }
            }
            Err(err) => return Err(err),
        }
    }
}

// --- errors -----------------------------------------------------------------

/// Where the upward walk stopped without finding `invar.yaml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Boundary {
    /// A directory holding `.git` and no `invar.yaml`: the repository root.
    GitRoot(PathBuf),
    /// The top of the filesystem.
    FilesystemRoot(PathBuf),
}

/// Why the project root could not be determined.
#[derive(Debug)]
pub enum RootError {
    /// The current directory is unavailable (deleted, or not permitted).
    CurrentDir(io::Error),
    /// The directory to start from does not exist or cannot be resolved.
    Start { path: PathBuf, source: io::Error },
    /// An entry on the way up could not be checked.
    Inaccessible { path: PathBuf, source: io::Error },
    /// No `invar.yaml` between the start and the boundary.
    NotFound {
        /// Every directory checked, nearest first.
        searched: Vec<PathBuf>,
        boundary: Boundary,
    },
}

impl fmt::Display for RootError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CurrentDir(source) => {
                write!(f, "cannot determine the current directory: {source}")
            }
            Self::Start { path, source } => {
                write!(f, "cannot resolve {}: {source}", path.display())
            }
            Self::Inaccessible { path, source } => write!(
                f,
                "cannot check {} while looking for the project root: {source}",
                path.display()
            ),
            Self::NotFound { searched, boundary } => {
                let (stopped, init_hint) = match boundary {
                    Boundary::GitRoot(repo) => (
                        format!("stopped at the git repository root {}", repo.display()),
                        format!(
                            "If this repository should use invar, run `invar init` in {}.",
                            repo.display()
                        ),
                    ),
                    Boundary::FilesystemRoot(top) => (
                        format!("stopped at the filesystem root {}", top.display()),
                        "Run `invar init` in your project's root directory to create one."
                            .to_owned(),
                    ),
                };
                writeln!(
                    f,
                    "not inside an invar project: no {} found; {stopped}. Searched:",
                    config::FILE_NAME
                )?;
                for dir in searched {
                    writeln!(f, "  {}", dir.display())?;
                }
                write!(f, "{init_hint}")
            }
        }
    }
}

impl std::error::Error for RootError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::CurrentDir(source)
            | Self::Start { source, .. }
            | Self::Inaccessible { source, .. } => Some(source),
            Self::NotFound { .. } => None,
        }
    }
}
