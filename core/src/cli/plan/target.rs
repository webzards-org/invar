//! How the plan commands name a Plan: a ticket id, or a path.

use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};

use crate::fs::plans::SPEC_FILE;
use crate::fs::{self, ProjectRoot};
use crate::gate::validate;

/// The project, and the `spec.md` that `target` names.
///
/// A path finds its own project, so it works from any directory. A ticket is
/// looked up in the project around the current directory.
pub fn resolve(target: &str, cwd: &Path) -> Result<(ProjectRoot, PathBuf)> {
    if is_path(target) {
        let spec = spec_path(&cwd.join(target))?;
        let root = ProjectRoot::discover_from(spec.parent().unwrap_or(&spec))?;
        Ok((root, spec))
    } else {
        let root = fs::project_root()?;
        let spec = validate::spec_for(&root, target)?;
        Ok((root, spec))
    }
}

pub fn is_path(target: &str) -> bool {
    target.contains(['/', '\\']) || target.ends_with(".md") || target == "." || target == ".."
}

/// A directory means its `spec.md`. The result is absolute and canonical as
/// far as it exists, so it compares with the project root.
fn spec_path(path: &Path) -> Result<PathBuf> {
    let spec = if path.is_dir() {
        path.join(SPEC_FILE)
    } else {
        path.to_owned()
    };
    let parent = spec.parent().unwrap_or(&spec);
    let parent = parent
        .canonicalize()
        .with_context(|| format!("{} does not exist", parent.display()))?;
    Ok(match spec.file_name() {
        Some(name) => parent.join(name),
        None => parent,
    })
}

/// Relative to the current directory when the file is under it, so the
/// paths an agent or a person sees are the ones they would type.
pub fn shown(path: &Path, cwd: &Path) -> String {
    let cwd = cwd.canonicalize().unwrap_or_else(|_| cwd.to_owned());
    path.strip_prefix(&cwd)
        .unwrap_or(path)
        .display()
        .to_string()
}
