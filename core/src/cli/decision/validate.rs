//! `invar decision validate <TICKET | PATH>`: check a draft entry
//! (decision-entry.md §8.1).
//!
//! Prints `ok: <path> (<ticket>, <type>, <module>)` for a valid draft and
//! `<path>:<line>: <problem>` for every problem, on stdout, so an agent reads
//! them as the command's output — the same shape as `plan validate`. A
//! failing run ends with one summary line on stderr and a non-zero exit.

use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};

use crate::cli::shown;
use crate::decision::draft;
use crate::format::decision::DRAFT_FILE;
use crate::fs::{self, ProjectRoot};

#[derive(Debug, clap::Args)]
pub struct Args {
    /// A ticket id (`PROJ-412`), or the path of a draft: its `decision.md`, or
    /// the Plan directory that holds it.
    ///
    /// Anything containing `/`, ending in `.md`, or `.`/`..` is a path;
    /// everything else is looked up as a ticket under `.invar/plans/`.
    pub target: String,
}

pub fn run(args: Args) -> Result<()> {
    let cwd = std::env::current_dir().context("cannot read the current directory")?;
    let (root, path) = resolve(&args.target, &cwd)?;

    let report = draft::validate(&root, &path);
    let path = shown(&report.path, &cwd);
    if let Some(draft) = &report.draft {
        println!(
            "ok: {path} ({}, {}, {})",
            draft.ticket, draft.kind, draft.module
        );
        return Ok(());
    }
    print!("{}", report.problems.render(Path::new(&path)));
    let count = report.problems.len();
    bail!(
        "the draft is not valid ({count} {}); fix every problem listed above, then run \
         `invar decision validate {}` again",
        if count == 1 { "problem" } else { "problems" },
        args.target
    )
}

/// The project, and the draft `target` names. Shared with `decision approve`.
pub(super) fn resolve(target: &str, cwd: &Path) -> Result<(ProjectRoot, PathBuf)> {
    if crate::cli::plan::target::is_path(target) {
        let given = cwd.join(target);
        let path = if given.is_dir() {
            given.join(DRAFT_FILE)
        } else {
            given
        };
        let parent = path.parent().unwrap_or(&path);
        let parent = parent
            .canonicalize()
            .with_context(|| format!("{} does not exist", parent.display()))?;
        let path = match path.file_name() {
            Some(name) => parent.join(name),
            None => parent,
        };
        let root = ProjectRoot::discover_from(path.parent().unwrap_or(&path))?;
        Ok((root, path))
    } else {
        let root = fs::project_root()?;
        let path = draft::path_for(&root, target)?;
        Ok((root, path))
    }
}
