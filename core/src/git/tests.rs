//! Tests for the date and time arithmetic, for what can be decided without
//! running `git` (a directory that is not there), and for the branch and index
//! queries — which run against a real repository built here. The identity and date
//! commands run `git` in the CLI tests, which control its configuration
//! through the environment.

use std::path::Path;

use super::{
    GitError, Instant, WorkTree, base_branch, changed_since, current_branch, file_at,
    file_in_index, merge_base, stage_removal, today, user_name, work_tree,
};
use crate::calendar::civil_from_days;

fn date(ident: &str) -> Option<String> {
    Instant::from_ident(ident).map(Instant::local_date)
}

#[test]
fn the_date_is_local_to_the_committer_offset() {
    // 2026-10-04T22:30:00Z: still the 4th in UTC, already the 5th in
    // Belgrade, and the 4th in New York.
    let utc_seconds = 1_791_153_000;
    assert_eq!(
        date(&format!("rade <r@x> {utc_seconds} +0000")).as_deref(),
        Some("2026-10-04")
    );
    assert_eq!(
        date(&format!("rade <r@x> {utc_seconds} +0200")).as_deref(),
        Some("2026-10-05")
    );
    assert_eq!(
        date(&format!("rade <r@x> {utc_seconds} -0400")).as_deref(),
        Some("2026-10-04")
    );
    // Minutes count: 22:30Z plus 1h30 is exactly midnight, plus 1h29 is not.
    assert_eq!(
        date(&format!("a <a@x> {utc_seconds} +0130")).as_deref(),
        Some("2026-10-05")
    );
    assert_eq!(
        date(&format!("a <a@x> {utc_seconds} +0129")).as_deref(),
        Some("2026-10-04")
    );
}

#[test]
fn names_with_spaces_and_angle_brackets_do_not_matter() {
    assert_eq!(
        date("Rade  Brković <rade@example.com> 0 +0000").as_deref(),
        Some("1970-01-01")
    );
}

#[test]
fn known_instants() {
    for (seconds, expected) in [
        (0_i64, "1970-01-01"),
        (-1, "1969-12-31"),
        (951_782_400, "2000-02-29"),
        (951_868_800, "2000-03-01"),
        (1_709_164_800, "2024-02-29"),
        (4_102_444_799, "2099-12-31"),
        (4_107_542_400, "2100-03-01"),
    ] {
        assert_eq!(
            date(&format!("x <x> {seconds} +0000")).as_deref(),
            Some(expected),
            "{seconds}"
        );
    }
}

#[test]
fn malformed_idents_are_none() {
    for ident in [
        "",
        "x <x>",
        "x <x> 1791153000",
        "x <x> soon +0200",
        "x <x> 1791153000 0200",
        "x <x> 1791153000 +200",
        "x <x> 1791153000 +02:00",
        "x <x> 1791153000 +0260",
        // Hours 24 and up are not an RFC 3339 offset (see
        // `offsets_are_limited_to_what_rfc_3339_allows`).
        "x <x> 1791153000 +2400",
        "x <x> 1791153000 -2400",
        "x <x> 1791153000 +9959",
        "x <x> 9223372036854775807 +0100",
    ] {
        assert_eq!(date(ident), None, "{ident:?}");
    }
}

#[test]
fn civil_from_days_agrees_with_counting_day_by_day() {
    fn is_leap(year: i64) -> bool {
        (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
    }
    let mut expected = (1900_i64, 1_u32, 1_u32);
    // 1900-01-01 is day -25567.
    for days in -25_567..80_000 {
        assert_eq!(civil_from_days(days), expected, "day {days}");
        let (year, month, day) = expected;
        let length = match month {
            2 if is_leap(year) => 29,
            2 => 28,
            4 | 6 | 9 | 11 => 30,
            _ => 31,
        };
        expected = if day < length {
            (year, month, day + 1)
        } else if month < 12 {
            (year, month + 1, 1)
        } else {
            (year + 1, 1, 1)
        };
    }
}

#[test]
fn an_instant_renders_local_with_its_offset_and_utc_for_filenames() {
    // 2026-10-06T16:37:12Z, read in Belgrade.
    let belgrade = Instant::from_ident("rade <r@x> 1791304632 +0200").unwrap();
    assert_eq!(belgrade.local_rfc3339(), "2026-10-06T18:37:12+02:00");
    assert_eq!(belgrade.utc_stamp(), "2026-10-06-163712");
    assert_eq!(belgrade.local_date(), "2026-10-06");

    // The same instant in New York, and in a half-hour zone.
    let new_york = Instant::from_ident("x <x> 1791304632 -0400").unwrap();
    assert_eq!(new_york.local_rfc3339(), "2026-10-06T12:37:12-04:00");
    assert_eq!(new_york.utc_stamp(), "2026-10-06-163712");
    let delhi = Instant::from_ident("x <x> 1791304632 +0530").unwrap();
    assert_eq!(delhi.local_rfc3339(), "2026-10-06T22:07:12+05:30");
    let utc = Instant::from_ident("x <x> 1791304632 +0000").unwrap();
    assert_eq!(utc.local_rfc3339(), "2026-10-06T16:37:12+00:00");

    // Midnight boundaries in both renderings.
    let late = Instant::from_ident("x <x> 1791323999 +0200").unwrap(); // 21:59:59Z
    assert_eq!(late.local_rfc3339(), "2026-10-06T23:59:59+02:00");
    assert_eq!(late.utc_stamp(), "2026-10-06-215959");
    let early = Instant::from_ident("x <x> 1791324000 +0200").unwrap(); // 22:00:00Z
    assert_eq!(early.local_rfc3339(), "2026-10-07T00:00:00+02:00");
    assert_eq!(early.utc_stamp(), "2026-10-06-220000");
}

#[test]
fn offsets_are_limited_to_what_rfc_3339_allows() {
    // Hours are 00–23. An offset outside that would be written into a decision
    // entry's `date` as something no reader can parse.
    for ident in [
        "x <x> 1791304632 +2400",
        "x <x> 1791304632 -2400",
        "x <x> 1791304632 +2500",
        "x <x> 1791304632 +9959",
        "x <x> 1791304632 -9959",
    ] {
        assert_eq!(Instant::from_ident(ident), None, "{ident:?}");
    }

    // The extremes real zones have (−12:00 and +14:00), and the widest offset
    // the format allows, are accepted and rendered as they are.
    // The base instant is 2026-10-06T16:37:12Z.
    for (ident, rendered) in [
        ("x <x> 1791304632 -1200", "2026-10-06T04:37:12-12:00"),
        ("x <x> 1791304632 +1400", "2026-10-07T06:37:12+14:00"),
        ("x <x> 1791304632 +2359", "2026-10-07T16:36:12+23:59"),
    ] {
        let instant = Instant::from_ident(ident).unwrap_or_else(|| panic!("{ident:?} refused"));
        assert_eq!(instant.local_rfc3339(), rendered, "{ident:?}");
    }
}

// --- a directory that is not there ------------------------------------------
//
// Starting a process in a missing directory fails with `NotFound`, the same
// kind of error as a `git` that is not installed. These pin that the two are
// told apart. Neither test needs `git` at all: the check comes first.

#[test]
fn a_missing_directory_is_not_reported_as_git_being_missing() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("no-such-directory");

    for result in [user_name(&missing).map(drop), today(&missing).map(drop)] {
        let err = result.unwrap_err();
        assert!(matches!(err, GitError::NotADirectory { .. }), "{err:?}");
        let message = err.to_string();
        assert!(message.contains("no-such-directory"), "{message}");
        assert!(!message.contains("not found on PATH"), "{message}");
        assert!(!message.contains("install git"), "{message}");
    }
}

#[test]
fn a_file_where_a_directory_was_expected_is_reported_the_same_way() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("a-file");
    std::fs::write(&file, "").unwrap();

    let err = user_name(&file).unwrap_err();
    assert!(matches!(err, GitError::NotADirectory { .. }), "{err:?}");
    assert!(err.to_string().contains("a-file"), "{err}");
}

// --- branches ---------------------------------------------------------------

/// A real repository in a temporary directory. `git` is run here with the
/// developer's own configuration out of the way, so a global `init.defaultBranch`
/// or a signing key cannot change what the tests see.
struct Repo {
    dir: tempfile::TempDir,
}

impl Repo {
    /// `main`, with one commit.
    fn new() -> Self {
        let repo = Self {
            dir: tempfile::tempdir().unwrap(),
        };
        repo.git(&["init", "--quiet", "-b", "main"]);
        repo.commit("initial");
        repo
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }

    fn git(&self, args: &[&str]) -> String {
        let output = std::process::Command::new("git")
            .args([
                "-c",
                "user.name=test",
                "-c",
                "user.email=test@example.com",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .current_dir(self.path())
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", self.path().join(".no-global-config"))
            .env("HOME", self.path())
            .output()
            .expect("git runs");
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    fn commit(&self, message: &str) {
        self.git(&["commit", "--quiet", "--allow-empty", "-m", message]);
    }
}

/// `main` at the initial commit, checked out; `feat/PROJ-1-merged` at the same
/// commit; `feat/PROJ-2-open` one commit ahead.
fn repo_with_branches() -> Repo {
    let repo = Repo::new();
    repo.git(&["branch", "feat/PROJ-1-merged"]);
    repo.git(&["checkout", "--quiet", "-b", "feat/PROJ-2-open"]);
    repo.commit("work in progress");
    repo.git(&["checkout", "--quiet", "main"]);
    repo
}

#[test]
fn a_configured_base_branch_is_used_as_it_is_and_git_is_not_asked() {
    // `run` would refuse this directory, so git is demonstrably not called.
    let nowhere = Path::new("/no/such/directory");
    assert_eq!(base_branch(nowhere, Some("develop")).unwrap(), "develop");
}

#[test]
fn without_origin_head_the_base_branch_is_main() {
    let repo = Repo::new();
    assert_eq!(base_branch(repo.path(), None).unwrap(), "main");
}

#[test]
fn outside_a_repository_the_base_branch_is_still_main() {
    // Not an error: whoever needs the branch asks git about it, and gets
    // git's own answer.
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(base_branch(dir.path(), None).unwrap(), "main");
}

#[test]
fn the_base_branch_is_what_origin_head_points_to() {
    let repo = Repo::new();
    repo.git(&["update-ref", "refs/remotes/origin/develop", "HEAD"]);
    repo.git(&[
        "symbolic-ref",
        "refs/remotes/origin/HEAD",
        "refs/remotes/origin/develop",
    ]);
    assert_eq!(base_branch(repo.path(), None).unwrap(), "origin/develop");
    // What the project says wins over what is detected.
    assert_eq!(
        base_branch(repo.path(), Some("release")).unwrap(),
        "release"
    );
}

#[test]
fn a_directory_inside_a_repository_reports_its_top_level() {
    let repo = Repo::new();
    let nested = repo.path().join("app/Http");
    std::fs::create_dir_all(&nested).unwrap();
    let top = repo.path().canonicalize().unwrap();

    for dir in [repo.path(), nested.as_path()] {
        match work_tree(dir).unwrap() {
            WorkTree::Inside(found) => {
                assert_eq!(found.canonicalize().unwrap(), top, "from {}", dir.display());
            }
            other => panic!("expected Inside, got {other:?}"),
        }
    }
}

#[test]
fn a_directory_outside_any_repository_is_an_answer_carrying_what_git_said() {
    let dir = tempfile::tempdir().unwrap();

    match work_tree(dir.path()).unwrap() {
        WorkTree::Outside(said) => assert!(!said.is_empty(), "git said nothing"),
        other => panic!("expected Outside, got {other:?}"),
    }
}

#[test]
fn inside_the_git_directory_itself_is_not_a_work_tree() {
    let repo = Repo::new();

    assert!(matches!(
        work_tree(&repo.path().join(".git")).unwrap(),
        WorkTree::Outside(_)
    ));
}

#[test]
fn the_current_branch_is_its_short_name_and_a_detached_head_is_none() {
    let repo = repo_with_branches();
    assert_eq!(
        current_branch(repo.path()).unwrap().as_deref(),
        Some("main")
    );
    repo.git(&["checkout", "--quiet", "feat/PROJ-2-open"]);
    assert_eq!(
        current_branch(repo.path()).unwrap().as_deref(),
        Some("feat/PROJ-2-open")
    );
    repo.git(&["checkout", "--quiet", "--detach"]);
    assert_eq!(current_branch(repo.path()).unwrap(), None);
}

#[test]
fn outside_a_repository_the_current_branch_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let err = current_branch(dir.path()).unwrap_err();
    assert!(matches!(err, GitError::Failed { .. }), "{err:?}");
}

#[test]
fn a_removal_is_staged_and_an_untracked_path_is_no_error() {
    let repo = Repo::new();
    let plan = repo.path().join(".invar/plans/PROJ-1-x");
    std::fs::create_dir_all(&plan).unwrap();
    std::fs::write(plan.join("spec.md"), "plan\n").unwrap();
    repo.git(&["add", "-A"]);
    repo.commit("plan");

    std::fs::remove_dir_all(&plan).unwrap();
    stage_removal(repo.path(), Path::new(".invar/plans/PROJ-1-x")).unwrap();
    assert_eq!(
        repo.git(&["diff", "--cached", "--name-status"]),
        "D\t.invar/plans/PROJ-1-x/spec.md\n"
    );

    // Never tracked: nothing to stage, and not a failure.
    stage_removal(repo.path(), Path::new(".invar/plans/PROJ-9-never")).unwrap();
}

// --- the merge base, and what changed since --------------------------------------

/// `main` with `t/a.txt`, `t/b.txt` and `t/c.txt`, and `feat` checked out, one commit
/// on. Returns the merge base too.
fn repo_with_tests() -> (Repo, String) {
    let repo = Repo::new();
    std::fs::create_dir_all(repo.path().join("t")).unwrap();
    for name in ["a", "b", "c"] {
        std::fs::write(repo.path().join(format!("t/{name}.txt")), name).unwrap();
    }
    repo.git(&["add", "-A"]);
    repo.commit("tests");
    let fork = repo.git(&["rev-parse", "HEAD"]).trim().to_owned();
    repo.git(&["checkout", "--quiet", "-b", "feat"]);
    repo.commit("work");
    (repo, fork)
}

#[test]
fn the_merge_base_is_where_the_branch_left_not_where_the_base_is_now() {
    let (repo, fork) = repo_with_tests();
    repo.git(&["checkout", "--quiet", "main"]);
    repo.commit("later on main");
    repo.git(&["checkout", "--quiet", "feat"]);
    assert_eq!(merge_base(repo.path(), "main").unwrap(), fork);
}

#[test]
fn a_base_that_is_not_there_is_its_own_error() {
    let (repo, _) = repo_with_tests();
    let err = merge_base(repo.path(), "develop").unwrap_err();
    assert!(matches!(err, GitError::BaseNotFound { .. }), "{err}");
}

#[test]
fn nothing_changed_is_an_empty_list() {
    let (repo, fork) = repo_with_tests();
    assert!(changed_since(repo.path(), &fork, "t").unwrap().is_empty());
}

#[test]
fn committed_staged_and_unstaged_changes_all_count_and_only_under_the_path() {
    let (repo, fork) = repo_with_tests();
    let write = |path: &str| std::fs::write(repo.path().join(path), "changed").unwrap();
    write("t/a.txt");
    write("outside.txt");
    repo.git(&["add", "-A"]);
    repo.commit("committed");
    write("t/b.txt");
    repo.git(&["add", "t/b.txt"]);
    write("t/c.txt");
    assert_eq!(
        changed_since(repo.path(), &fork, "t").unwrap(),
        ["t/a.txt", "t/b.txt", "t/c.txt"]
    );
}

#[test]
fn a_change_put_back_on_disk_still_counts() {
    let (repo, fork) = repo_with_tests();
    std::fs::write(repo.path().join("t/a.txt"), "changed").unwrap();
    repo.git(&["add", "t/a.txt"]);
    std::fs::write(repo.path().join("t/a.txt"), "a").unwrap();
    assert_eq!(changed_since(repo.path(), &fork, "t").unwrap(), ["t/a.txt"]);
}

#[test]
fn a_rename_is_a_delete_and_an_add() {
    let (repo, fork) = repo_with_tests();
    repo.git(&["mv", "t/a.txt", "t/z.txt"]);
    repo.commit("renamed");
    assert_eq!(
        changed_since(repo.path(), &fork, "t").unwrap(),
        ["t/a.txt", "t/z.txt"]
    );
}

#[test]
fn below_the_top_level_paths_are_relative_to_the_directory_asked_from() {
    let repo = Repo::new();
    let project = repo.path().join("apps/shop");
    std::fs::create_dir_all(project.join("t")).unwrap();
    std::fs::write(project.join("t/a.txt"), "a").unwrap();
    repo.git(&["add", "-A"]);
    repo.commit("tests");
    let fork = repo.git(&["rev-parse", "HEAD"]).trim().to_owned();
    std::fs::write(project.join("t/a.txt"), "changed").unwrap();

    assert_eq!(changed_since(&project, &fork, "t").unwrap(), ["t/a.txt"]);
    assert_eq!(file_at(&project, &fork, "t/a.txt").unwrap().unwrap(), b"a");
    assert_eq!(file_in_index(&project, "t/a.txt").unwrap().unwrap(), b"a");
}

#[test]
fn a_file_is_read_at_a_commit_and_in_the_index_and_its_absence_is_an_answer() {
    let (repo, fork) = repo_with_tests();
    repo.git(&["rm", "--quiet", "t/a.txt"]);
    assert_eq!(
        file_at(repo.path(), &fork, "t/a.txt").unwrap().unwrap(),
        b"a"
    );
    assert_eq!(file_at(repo.path(), &fork, "t/none.txt").unwrap(), None);
    assert_eq!(
        file_at(repo.path(), &fork, "t").unwrap(),
        None,
        "a directory"
    );
    assert_eq!(file_in_index(repo.path(), "t/a.txt").unwrap(), None);
    assert_eq!(
        file_in_index(repo.path(), "t/b.txt").unwrap().unwrap(),
        b"b"
    );
}

#[test]
fn bytes_come_back_as_they_are() {
    let repo = Repo::new();
    std::fs::write(repo.path().join("binary"), [0xff, 0x00, 0x0d, 0x0a]).unwrap();
    repo.git(&["add", "-A"]);
    repo.commit("binary");
    assert_eq!(
        file_at(repo.path(), "HEAD", "binary").unwrap().unwrap(),
        [0xff, 0x00, 0x0d, 0x0a]
    );
}

#[test]
fn a_revision_that_is_not_there_is_an_error_and_not_an_absent_file() {
    let (repo, _) = repo_with_tests();
    assert!(file_at(repo.path(), "no-such-rev", "t/a.txt").is_err());
}
