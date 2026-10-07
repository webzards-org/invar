//! Tests for project-root discovery and path building (M1-4).
//!
//! Every tree is built in a temp directory. Expected paths are canonicalized
//! too: on macOS the temp directory itself sits behind a symlink
//! (`/var` → `/private/var`), and the module returns canonical paths.

use std::fs;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use super::{Boundary, ProjectRoot, RootError};

/// A temp tree. Each entry is created as an empty file, with its parent
/// directories; an entry ending in `/` is created as a directory.
fn tree(entries: &[&str]) -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    for entry in entries {
        let path = dir.path().join(entry);
        if entry.ends_with('/') {
            fs::create_dir_all(&path).unwrap();
        } else {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, "").unwrap();
        }
    }
    dir
}

fn canonical(path: &Path) -> PathBuf {
    path.canonicalize().unwrap()
}

fn discover(start: &Path) -> Result<PathBuf, RootError> {
    ProjectRoot::discover_from(start).map(|root| root.path().to_owned())
}

// --- discovery --------------------------------------------------------------

#[test]
fn the_same_root_from_the_root_and_from_two_levels_down() {
    let dir = tree(&["invar.yaml", "app/Http/Controllers/"]);
    let root = canonical(dir.path());

    let from_root = discover(dir.path()).unwrap();
    let from_two_down = discover(&dir.path().join("app/Http")).unwrap();
    let from_three_down = discover(&dir.path().join("app/Http/Controllers")).unwrap();

    assert_eq!(from_root, root);
    assert_eq!(from_two_down, root);
    assert_eq!(from_three_down, root);
}

#[test]
fn a_start_through_dot_and_dotdot_resolves_to_the_same_root() {
    let dir = tree(&["invar.yaml", "app/Http/", "resources/js/"]);
    let root = canonical(dir.path());

    let roundabout = dir.path().join("app/Http/../../resources/./js");
    assert_eq!(discover(&roundabout).unwrap(), root);
}

#[cfg(unix)]
#[test]
fn a_start_reached_through_a_symlink_resolves_to_the_real_root() {
    let dir = tree(&["project/invar.yaml", "project/app/Http/"]);
    let link = dir.path().join("shortcut");
    std::os::unix::fs::symlink(dir.path().join("project/app"), &link).unwrap();

    // Walking up from `shortcut/Http` lexically would reach `shortcut`, then
    // the temp dir, and never see `project/invar.yaml`. Canonical form walks
    // the real parents.
    assert_eq!(
        discover(&link.join("Http")).unwrap(),
        canonical(&dir.path().join("project"))
    );
}

#[test]
fn a_file_as_start_begins_in_its_directory() {
    let dir = tree(&["invar.yaml", "app/Http/Kernel.php"]);
    assert_eq!(
        discover(&dir.path().join("app/Http/Kernel.php")).unwrap(),
        canonical(dir.path())
    );
}

#[test]
fn the_nearest_invar_yaml_wins() {
    // A monorepo with a project inside a project: the inner one is meant.
    let dir = tree(&["invar.yaml", "backend/invar.yaml", "backend/app/"]);
    assert_eq!(
        discover(&dir.path().join("backend/app")).unwrap(),
        canonical(&dir.path().join("backend"))
    );
}

#[test]
fn invar_yaml_at_the_repository_root_is_found_not_a_boundary() {
    let dir = tree(&["invar.yaml", ".git/", "app/Http/"]);
    assert_eq!(
        discover(&dir.path().join("app/Http")).unwrap(),
        canonical(dir.path())
    );
}

#[test]
fn invar_yaml_below_the_repository_root_is_found() {
    // A monorepo where only `backend/` uses invar.
    let dir = tree(&[".git/", "backend/invar.yaml", "backend/app/Http/"]);
    assert_eq!(
        discover(&dir.path().join("backend/app/Http")).unwrap(),
        canonical(&dir.path().join("backend"))
    );
}

#[test]
fn any_entry_named_invar_yaml_marks_the_root() {
    // Walking past it would silently pick a different project; reading the
    // config reports it as unreadable instead.
    let dir = tree(&["invar.yaml/", "app/"]);
    assert_eq!(
        discover(&dir.path().join("app")).unwrap(),
        canonical(dir.path())
    );
}

// --- boundaries -------------------------------------------------------------

#[test]
fn the_walk_stops_at_a_git_boundary_and_never_borrows_a_parent_config() {
    // The `invar.yaml` above `repo/` belongs to some other project. The
    // repository has none of its own, and must not use that one.
    let dir = tree(&["invar.yaml", "repo/.git/", "repo/app/Http/"]);
    let repo = canonical(&dir.path().join("repo"));

    let err = discover(&dir.path().join("repo/app/Http")).unwrap_err();
    let RootError::NotFound { searched, boundary } = &err else {
        panic!("expected NotFound, got {err:?}");
    };
    assert_eq!(boundary, &Boundary::GitRoot(repo.clone()));
    assert_eq!(
        searched,
        &[repo.join("app/Http"), repo.join("app"), repo.clone()]
    );
}

#[test]
fn a_git_file_is_a_boundary_too() {
    // Worktrees and submodules have a `.git` file, not a directory.
    let dir = tree(&["invar.yaml", "worktree/.git", "worktree/src/"]);
    let err = discover(&dir.path().join("worktree/src")).unwrap_err();
    assert!(
        matches!(
            &err,
            RootError::NotFound { boundary: Boundary::GitRoot(repo), .. }
                if *repo == canonical(&dir.path().join("worktree"))
        ),
        "{err:?}"
    );
}

#[test]
fn without_git_the_walk_stops_at_the_filesystem_root() {
    // Assumes no `invar.yaml` and no `.git` above the temp directory, which
    // holds on any sane machine and in CI.
    let dir = tree(&["a/b/"]);
    let err = discover(&dir.path().join("a/b")).unwrap_err();
    let RootError::NotFound { searched, boundary } = &err else {
        panic!("expected NotFound, got {err:?}");
    };
    let Boundary::FilesystemRoot(top) = boundary else {
        panic!("expected the filesystem root, got {boundary:?}");
    };
    assert_eq!(top.parent(), None, "{top:?} is not a filesystem root");
    assert_eq!(searched.first(), Some(&canonical(&dir.path().join("a/b"))));
    assert_eq!(searched.last(), Some(top));
}

// --- error messages ---------------------------------------------------------

#[test]
fn not_found_names_every_directory_searched_and_suggests_invar_init() {
    let dir = tree(&["repo/.git/", "repo/app/Http/"]);
    let repo = canonical(&dir.path().join("repo"));
    let message = discover(&dir.path().join("repo/app/Http"))
        .unwrap_err()
        .to_string();

    assert!(message.contains("no invar.yaml found"), "{message}");
    assert!(message.contains("git repository root"), "{message}");
    for searched in [repo.join("app/Http"), repo.join("app"), repo.clone()] {
        assert!(
            message.contains(&format!("  {}\n", searched.display())),
            "{} missing from:\n{message}",
            searched.display()
        );
    }
    assert!(
        message.contains(&format!("run `invar init` in {}", repo.display())),
        "{message}"
    );
}

#[test]
fn not_found_at_the_filesystem_root_says_so() {
    let dir = tree(&["a/"]);
    let message = discover(&dir.path().join("a")).unwrap_err().to_string();
    assert!(message.contains("filesystem root"), "{message}");
    assert!(message.contains("invar init"), "{message}");
}

#[test]
fn a_start_that_does_not_exist_is_its_own_error() {
    let dir = tree(&[]);
    let err = discover(&dir.path().join("no/such/dir")).unwrap_err();
    assert!(matches!(err, RootError::Start { .. }), "{err:?}");
    assert!(err.to_string().contains("no/such/dir"), "{err}");
}

// --- paths inside the project ----------------------------------------------

#[test]
fn every_project_path_is_built_from_the_root() {
    let dir = tree(&["invar.yaml"]);
    let root = ProjectRoot::discover_from(dir.path()).unwrap();
    let base = canonical(dir.path());

    assert_eq!(root.path(), base);
    assert_eq!(root.config_file(), base.join("invar.yaml"));
    assert_eq!(root.plans_dir(), base.join(".invar/plans"));
    assert_eq!(root.decisions_dir(), base.join(".invar/decisions"));
    assert_eq!(root.out_dir(), base.join(".invar/out"));
}

#[test]
fn join_accepts_relative_paths_and_refuses_escapes() {
    let dir = tree(&["invar.yaml"]);
    let root = ProjectRoot::discover_from(dir.path()).unwrap();
    let base = canonical(dir.path());

    assert_eq!(
        root.join(Path::new("specs/orders/reserve.feature")),
        Some(base.join("specs/orders/reserve.feature"))
    );
    assert_eq!(
        root.join(Path::new("./AI_CONTEXT.md")),
        Some(base.join("AI_CONTEXT.md"))
    );
    assert_eq!(root.join(Path::new("../elsewhere")), None);
    assert_eq!(root.join(Path::new("specs/../../elsewhere")), None);
    assert_eq!(root.join(Path::new("/etc/passwd")), None);
}

#[test]
fn relative_path_compares_in_canonical_form() {
    let dir = tree(&["invar.yaml", ".invar/plans/PROJ-1-x/spec.md", "app/"]);
    let root = ProjectRoot::discover_from(dir.path()).unwrap();
    let spec = Path::new(".invar/plans/PROJ-1-x/spec.md");

    // Plain, through `..`, and through `.`: one file, one answer.
    for path in [
        dir.path().join(spec),
        dir.path().join("app/../.invar/plans/PROJ-1-x/spec.md"),
        dir.path().join("./.invar/./plans/PROJ-1-x/spec.md"),
    ] {
        assert_eq!(
            root.relative_path(&path).unwrap().as_deref(),
            Some(spec),
            "{}",
            path.display()
        );
    }
}

#[cfg(unix)]
#[test]
fn relative_path_sees_through_a_symlink_into_the_project() {
    let dir = tree(&[
        "project/invar.yaml",
        "project/.invar/plans/PROJ-1-x/spec.md",
    ]);
    let root = ProjectRoot::discover_from(&dir.path().join("project")).unwrap();
    let link = dir.path().join("plans-link");
    std::os::unix::fs::symlink(dir.path().join("project/.invar/plans"), &link).unwrap();

    assert_eq!(
        root.relative_path(&link.join("PROJ-1-x/spec.md"))
            .unwrap()
            .as_deref(),
        Some(Path::new(".invar/plans/PROJ-1-x/spec.md"))
    );
}

#[test]
fn relative_path_handles_a_file_that_does_not_exist_yet() {
    // The `Write` tool reports the path of a file it is about to create.
    let dir = tree(&["invar.yaml", ".invar/plans/"]);
    let root = ProjectRoot::discover_from(dir.path()).unwrap();

    assert_eq!(
        root.relative_path(&dir.path().join(".invar/plans/PROJ-2-new/spec.md"))
            .unwrap()
            .as_deref(),
        Some(Path::new(".invar/plans/PROJ-2-new/spec.md"))
    );
    // `..` inside the missing part is resolved too.
    assert_eq!(
        root.relative_path(&dir.path().join("new/dir/../file.md"))
            .unwrap()
            .as_deref(),
        Some(Path::new("new/file.md"))
    );
}

#[test]
fn relative_path_is_none_outside_the_project() {
    let dir = tree(&["project/invar.yaml", "elsewhere/notes.md"]);
    let root = ProjectRoot::discover_from(&dir.path().join("project")).unwrap();

    assert_eq!(
        root.relative_path(&dir.path().join("elsewhere/notes.md"))
            .unwrap(),
        None
    );
    // Climbing out with `..` from inside the project is outside too.
    assert_eq!(
        root.relative_path(&dir.path().join("project/../elsewhere/notes.md"))
            .unwrap(),
        None
    );
    // A sibling whose name merely starts with the root's name is outside.
    fs::create_dir(dir.path().join("project-old")).unwrap();
    assert_eq!(
        root.relative_path(&dir.path().join("project-old/x"))
            .unwrap(),
        None
    );
}

// --- project_root() ---------------------------------------------------------

#[test]
fn project_root_walks_up_from_the_current_directory() {
    // Changing the working directory is process-wide and would race with the
    // other tests, so this checks the one thing `project_root` adds: that it
    // starts where the process is. Whatever the test's own directory resolves
    // to — the repository root, or an error naming it — both must agree.
    let cwd = std::env::current_dir().unwrap();
    let summary = |result: Result<ProjectRoot, RootError>| match result {
        Ok(root) => Ok(root.path().to_owned()),
        Err(RootError::NotFound { searched, boundary }) => Err((searched, boundary)),
        Err(other) => panic!("unexpected error: {other}"),
    };
    assert_eq!(
        summary(super::project_root()),
        summary(ProjectRoot::discover_from(&cwd))
    );
}

#[cfg(unix)]
#[test]
fn cannot_tell_is_an_error_not_a_miss() {
    use std::os::unix::fs::PermissionsExt;

    // A directory that may be listed but not searched: whether `invar.yaml`
    // is in it cannot be determined. Treating that as "not here" would walk
    // on and might find a different project's config.
    let dir = tree(&["locked/"]);
    let locked = dir.path().join("locked");
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o600)).unwrap();

    let result = super::entry_exists(&locked, "invar.yaml");
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o700)).unwrap();

    // Root ignores permission bits, so the check cannot be provoked there.
    if result.as_ref().is_ok_and(|found| !found) && is_root() {
        eprintln!("skipped: running as root, permissions are not enforced");
        return;
    }
    let err = result.expect_err("an unsearchable directory must be an error");
    assert!(matches!(err, RootError::Inaccessible { .. }), "{err:?}");
    assert!(err.to_string().contains("cannot check"), "{err}");
}

#[cfg(unix)]
fn is_root() -> bool {
    use std::os::unix::fs::MetadataExt;
    // The owner of a file this process just created is its effective user.
    let probe = tempfile::NamedTempFile::new().unwrap();
    probe.as_file().metadata().unwrap().uid() == 0
}

// --- symbolic links that point nowhere --------------------------------------
//
// The operating system reports a dangling symlink as `NotFound`, exactly like
// a name that does not exist. A write through it, though, creates the file at
// its target, so `relative_path` has to report the target (M1-4, and the
// `PreToolUse` guard that relies on it, M1-10).

#[cfg(unix)]
mod dangling_symlinks {
    use std::os::unix::fs::symlink;

    use super::*;

    /// A project with `.invar/decisions/` and `.invar/plans/`, and a directory
    /// beside it. Returns the temp dir, the root, and the project's path
    /// as the test built it (not canonicalized).
    fn setup() -> (TempDir, ProjectRoot, PathBuf) {
        let dir = tree(&[
            "project/invar.yaml",
            "project/.invar/decisions/",
            "project/.invar/plans/",
            "elsewhere/",
        ]);
        let project = dir.path().join("project");
        let root = ProjectRoot::discover_from(&project).unwrap();
        (dir, root, project)
    }

    fn relative(root: &ProjectRoot, path: &Path) -> Option<PathBuf> {
        root.relative_path(path).unwrap()
    }

    #[test]
    fn a_dangling_link_that_points_out_of_the_project_is_outside() {
        // Reported as `out-link`, inside the project, a write through it would
        // be allowed: it creates `elsewhere/new.md`.
        let (_dir, root, project) = setup();
        let link = project.join("out-link");
        symlink("../elsewhere/new.md", &link).unwrap();

        assert_eq!(relative(&root, &link), None);
    }

    #[test]
    fn a_dangling_link_to_a_protected_path_is_that_path() {
        // Reported as `shortcut`, the decision log could be written to under
        // another name.
        let (_dir, root, project) = setup();
        let link = project.join("shortcut");
        symlink(".invar/decisions/new.md", &link).unwrap();

        assert_eq!(
            relative(&root, &link).as_deref(),
            Some(Path::new(".invar/decisions/new.md"))
        );
    }

    #[test]
    fn an_absolute_link_target_is_followed_too() {
        let (_dir, root, project) = setup();
        let link = project.join("absolute");
        symlink(root.path().join(".invar/decisions/abs.md"), &link).unwrap();

        assert_eq!(
            relative(&root, &link).as_deref(),
            Some(Path::new(".invar/decisions/abs.md"))
        );
    }

    #[test]
    fn a_chain_of_dangling_links_is_followed_to_its_end() {
        let (_dir, root, project) = setup();
        symlink(".invar/decisions/chained.md", project.join("second")).unwrap();
        symlink("second", project.join("first")).unwrap();

        assert_eq!(
            relative(&root, &project.join("first")).as_deref(),
            Some(Path::new(".invar/decisions/chained.md"))
        );
    }

    #[test]
    fn a_dangling_link_in_the_middle_of_a_path_is_followed_too() {
        // `plans-link` points at a directory that does not exist yet; a file
        // below it would be created there once the directory is.
        let (_dir, root, project) = setup();
        symlink(".invar/plans/PROJ-9-new", project.join("plans-link")).unwrap();

        assert_eq!(
            relative(&root, &project.join("plans-link/spec.md")).as_deref(),
            Some(Path::new(".invar/plans/PROJ-9-new/spec.md"))
        );
    }

    #[test]
    fn a_link_that_loops_is_an_error_not_a_hang() {
        let (_dir, root, project) = setup();
        symlink("b", project.join("a")).unwrap();
        symlink("a", project.join("b")).unwrap();

        assert!(root.relative_path(&project.join("a")).is_err());
    }

    #[test]
    fn a_missing_name_next_to_a_dangling_link_is_still_just_missing() {
        // Following links must not change the ordinary case.
        let (_dir, root, project) = setup();
        symlink(".invar/decisions/new.md", project.join("shortcut")).unwrap();

        assert_eq!(
            relative(&root, &project.join("specs/orders/new.feature")).as_deref(),
            Some(Path::new("specs/orders/new.feature"))
        );
    }
}
