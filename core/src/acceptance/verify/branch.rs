//! The branch rule (`acceptance-lock.md` §10.2): another ticket's locked test may
//! change on this branch only if the current Plan declares it.
//!
//! Read from git, because there is no manifest (A-3). The version at the merge base is
//! what a person locked, and it is out of reach of anything done on this branch.

use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::Path;

use crate::format::lock::{self, LockLine};
use crate::format::plan::{TestAction, TestToChange};
use crate::fs::ProjectRoot;
use crate::git::{self, GitError, WorkTree};

use super::Report;

/// Check the branch of `root` against the declarations of the Plan of `ticket`.
/// `configured` is `git.base_branch`.
pub fn check(
    root: &ProjectRoot,
    dir: &str,
    ticket: &str,
    declared: &[TestToChange],
    configured: Option<&str>,
    report: &mut Report,
) {
    // Needs nothing from git, so it holds outside a repository too.
    let declared: Vec<Declaration> = declared
        .iter()
        .filter_map(|item| match normalised(&item.path, dir) {
            Ok(path) => Some(Declaration {
                path,
                action: item.action,
            }),
            Err(why) => {
                report.problems.push(format!(
                    "{}: in `## Acceptance tests to change`, {why}; a declared path is relative to \
                     the project root, with `/` separators, no `..`, and under `{dir}` \
                     (plan-header.md §6.5)",
                    item.path
                ));
                None
            }
        })
        .collect();

    match repository(root.path()) {
        Repository::Inside => {}
        Repository::None => {
            report.warnings.push(
                "the project is not in a git repository, so changes to other tickets' locked \
                 tests cannot be checked"
                    .to_owned(),
            );
            return;
        }
        Repository::CannotAsk(said) => {
            report.problems.push(cannot_compare(&said));
            return;
        }
    }

    let fork = match merge_base(root.path(), configured) {
        Ok(fork) => fork,
        Err(problem) => {
            report.problems.push(problem);
            return;
        }
    };
    if let Err(err) = compare(root.path(), dir, ticket, &declared, &fork, report) {
        report.problems.push(cannot_compare(&err.to_string()));
    }
}

/// The merge base, or the problem to report. A shallow clone gets a message of its own,
/// because CI checks out that way by default and the fix is not a guess.
fn merge_base(root: &Path, configured: Option<&str>) -> Result<String, String> {
    let base =
        git::base_branch(root, configured).map_err(|err| cannot_compare(&err.to_string()))?;
    git::merge_base(root, &base).map_err(|err| {
        if matches!(git::is_shallow(root), Ok(true)) {
            shallow_clone(&base, &err.to_string())
        } else {
            cannot_compare(&err.to_string())
        }
    })
}

/// The commands are the ones that work. `git fetch origin <base>` alone only sets
/// `FETCH_HEAD` in a shallow clone, whose fetch refspec names one branch, and a checkout
/// with `fetch-depth: 0` makes `origin/<base>` but no `<base>`: neither gives git the name
/// that `base` is, so each has to create it. `--deepen` is not offered: it counts from
/// the existing shallow boundary, a ref fetched for the first time has none, and the
/// result differs between versions of git.
fn shallow_clone(base: &str, said: &str) -> String {
    let (refspec, full_history) = match base.strip_prefix("origin/") {
        Some(branch) => (
            format!("+refs/heads/{branch}:refs/remotes/origin/{branch}"),
            "check out with `fetch-depth: 0`".to_owned(),
        ),
        None => (
            format!("{base}:{base}"),
            format!("check out with `fetch-depth: 0` and run `git branch {base} origin/{base}`"),
        ),
    };
    format!(
        "this is a shallow clone, and it does not have the base branch `{base}` or its merge base \
         with this branch, so changes to other tickets' locked tests cannot be checked: {said}. \
         Fetch the base branch with its history, with `git fetch --unshallow origin {refspec}`, \
         or in CI {full_history}. If `{base}` is not the base branch, set `git.base_branch` in \
         invar.yaml"
    )
}

fn cannot_compare(said: &str) -> String {
    format!(
        "this branch cannot be compared with its base branch, so changes to other tickets' locked \
         tests cannot be checked: {said}. A shallow clone may not have the merge base: fetch \
         more history, or set `git.base_branch` in invar.yaml"
    )
}

enum Repository {
    Inside,
    None,
    /// A repository is there, and git will not say anything about it (D-23).
    CannotAsk(String),
}

fn repository(dir: &Path) -> Repository {
    let said = match git::work_tree(dir) {
        Ok(WorkTree::Inside(_)) => return Repository::Inside,
        Ok(WorkTree::Outside(said)) => said,
        Err(err) => err.to_string(),
    };
    // A file, in a worktree or a submodule. What git says here depends on the locale
    // and the version, so it is not what tells the two cases apart.
    if dir.ancestors().any(|up| up.join(".git").exists()) {
        Repository::CannotAsk(said)
    } else {
        Repository::None
    }
}

struct Declaration {
    path: String,
    action: TestAction,
}

/// `path` with `.` and empty segments dropped, if it may name a test under `dir`.
pub fn normalised(path: &str, dir: &str) -> Result<String, String> {
    if path.contains('\\') {
        return Err("the path has a `\\`".to_owned());
    }
    if path.starts_with('/') || path.as_bytes().get(1) == Some(&b':') {
        return Err("the path is absolute".to_owned());
    }
    let segments: Vec<&str> = path
        .split('/')
        .filter(|segment| !segment.is_empty() && *segment != ".")
        .collect();
    if segments.contains(&"..") {
        return Err("the path has a `..`".to_owned());
    }
    let under: Vec<&str> = dir
        .split('/')
        .filter(|segment| !segment.is_empty() && *segment != ".")
        .collect();
    if segments.len() <= under.len() || segments[..under.len()] != under[..] {
        return Err(format!("the path is not under `{dir}`"));
    }
    Ok(segments.join("/"))
}

/// How a file stands against the merge base, in the three places it can be (D-22).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Now {
    Same,
    Changed,
    Deleted,
}

fn compare(
    root: &Path,
    dir: &str,
    ticket: &str,
    declared: &[Declaration],
    fork: &str,
    report: &mut Report,
) -> Result<(), GitError> {
    let mut paths: BTreeSet<String> = git::changed_since(root, fork, dir)?.into_iter().collect();
    paths.extend(declared.iter().map(|item| item.path.clone()));

    for path in &paths {
        let allowed: Vec<TestAction> = declared
            .iter()
            .filter(|item| item.path == *path)
            .map(|item| item.action)
            .collect();
        let at_fork = git::file_at(root, fork, path)?;
        let locked = at_fork
            .as_deref()
            .and_then(|bytes| lock::read(bytes).ok().flatten())
            .filter(|line| line.recorded.is_some());
        let Some(locked) = locked else {
            if !allowed.is_empty() {
                report.warnings.push(format!(
                    "{path}: declared in `## Acceptance tests to change`, and it was not a locked \
                     test at the merge base; a typo here lets nothing through"
                ));
            }
            continue;
        };
        let bytes = at_fork.unwrap_or_default();
        let now = now(root, path, &bytes, &locked)?;

        if now == Now::Same {
            if !allowed.is_empty() {
                report.warnings.push(format!(
                    "{path}: declared in `## Acceptance tests to change`, and it did not change"
                ));
            }
            continue;
        }
        // A ticket's second cycle: its own file needs no declaration.
        if locked.ticket == ticket {
            continue;
        }
        let needed = match now {
            Now::Deleted => TestAction::Delete,
            _ => TestAction::Change,
        };
        if !allowed.contains(&needed) {
            report
                .problems
                .push(undeclared(path, &locked, needed, &allowed, fork));
        }
    }
    Ok(())
}

fn undeclared(
    path: &str,
    locked: &LockLine,
    needed: TestAction,
    allowed: &[TestAction],
    fork: &str,
) -> String {
    let what = match needed {
        TestAction::Change => "changed",
        TestAction::Delete => "deleted",
    };
    let listed = match allowed.first() {
        Some(other) => format!(", and the Plan lists it only with `{}`", other.as_str()),
        None => String::new(),
    };
    format!(
        "{path}: locked for {ticket} at the merge base, and {what} on this branch{listed}. Declare \
         it in the Plan's `## Acceptance tests to change` as `- {path} — {action} — <reason>` and \
         have the developer approve the Plan again, or restore the file with `git checkout {fork} \
         -- {path}`",
        ticket = locked.ticket,
        action = needed.as_str(),
    )
}

/// The work tree, the index and `HEAD`, each against the merge base. Deleted in any of
/// them is deleted; otherwise changed in any of them is changed.
fn now(root: &Path, path: &str, bytes: &[u8], locked: &LockLine) -> Result<Now, GitError> {
    let on_disk = match on_disk(&root.join(path)) {
        Ok(on_disk) => on_disk,
        // Unreadable is not the same file.
        Err(_) => OnDisk::Other,
    };
    let in_index = git::file_in_index(root, path)?;
    let in_head = git::file_at(root, "HEAD", path)?;

    if matches!(on_disk, OnDisk::Missing) || in_index.is_none() || in_head.is_none() {
        return Ok(Now::Deleted);
    }
    let same = |now: &[u8]| now == bytes || same_lock(now, locked);
    let on_disk_same = match &on_disk {
        OnDisk::File(now) => same(now),
        _ => false,
    };
    let all_same = on_disk_same
        && in_index.as_deref().is_some_and(same)
        && in_head.as_deref().is_some_and(same);
    Ok(if all_same { Now::Same } else { Now::Changed })
}

/// Whether `now` has the same line and covers the same text (§4, D-22). The digest does
/// not depend on line endings, so a checkout filter is not a change. The ticket and the
/// recorded digest are compared too, because removing them leaves the digest alone.
fn same_lock(now: &[u8], locked: &LockLine) -> bool {
    matches!(
        lock::read(now),
        Ok(Some(line)) if line.ticket == locked.ticket
            && line.recorded == locked.recorded
            && line.digest == locked.digest
    )
}

enum OnDisk {
    Missing,
    File(Vec<u8>),
    /// A symlink or a directory where the file was, which is never the same file.
    Other,
}

fn on_disk(path: &Path) -> io::Result<OnDisk> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() => fs::read(path).map(OnDisk::File),
        Ok(_) => Ok(OnDisk::Other),
        // A file where a parent directory was reads as `NotADirectory`.
        Err(err)
            if matches!(
                err.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
            ) =>
        {
            Ok(OnDisk::Missing)
        }
        Err(err) => Err(err),
    }
}

#[cfg(test)]
mod tests {
    use super::normalised;

    const DIR: &str = "tests/Acceptance";

    #[test]
    fn a_path_under_the_directory_is_taken_with_dots_and_doubled_slashes_dropped() {
        for path in [
            "tests/Acceptance/a.php",
            "./tests/Acceptance/a.php",
            "tests//Acceptance/./a.php",
        ] {
            assert_eq!(normalised(path, DIR).unwrap(), "tests/Acceptance/a.php");
        }
        assert_eq!(
            normalised("tests/Acceptance/sub/a.php", "./tests/Acceptance").unwrap(),
            "tests/Acceptance/sub/a.php"
        );
    }

    #[test]
    fn a_path_that_cannot_name_a_test_is_refused_with_the_reason() {
        for (path, why) in [
            ("tests/Acceptance/../a.php", "`..`"),
            ("../tests/Acceptance/a.php", "`..`"),
            ("/tests/Acceptance/a.php", "absolute"),
            ("C:/tests/Acceptance/a.php", "absolute"),
            ("tests\\Acceptance\\a.php", "`\\`"),
            ("src/a.php", "not under"),
            ("tests/AcceptanceOther/a.php", "not under"),
            ("tests/Acceptance", "not under"),
        ] {
            let err = normalised(path, DIR).unwrap_err();
            assert!(err.contains(why), "{path}: {err}");
        }
    }
}
