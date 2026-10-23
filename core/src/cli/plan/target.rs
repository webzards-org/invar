//! How the plan commands name a Plan: a ticket id, or a path.

use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};

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

/// The one Plan directory under `.invar/plans/` that holds `file`, by name,
/// for a command run without a ticket.
///
/// Never a guess: with none it fails with `nothing`, with several it fails
/// with `several` (a sentence that follows the count, such as "Plans are
/// waiting; name one: …") and the names, one per line. The name is then looked
/// up like a ticket, so everything after this is what an explicit ticket gets.
pub fn only(root: &ProjectRoot, file: &str, nothing: &str, several: &str) -> Result<String> {
    let names: Vec<String> = crate::fs::plans::list(root)?
        .unwrap_or_default()
        .into_iter()
        .filter(|name| root.plans_dir().join(name).join(file).is_file())
        .collect();
    match names.as_slice() {
        [] => bail!("{nothing}"),
        [one] => Ok(one.clone()),
        many => bail!(
            "{} {several}\n{}",
            many.len(),
            many.iter()
                .map(|name| format!("  {name}"))
                .collect::<Vec<_>>()
                .join("\n")
        ),
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
/// paths an agent or a person sees are the ones they would type. Shared with
/// the other commands, so it lives in `cli`.
pub use crate::cli::shown;
