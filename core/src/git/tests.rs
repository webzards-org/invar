//! Tests for the date and time arithmetic, for what can be decided without
//! running `git` (a directory that is not there), and for the branch queries
//! — which run against a real repository built here. The identity and date
//! commands run `git` in the CLI tests, which control its configuration
//! through the environment.

use std::path::Path;

use super::{
    GitError, Instant, WorkTree, base_branch, branch_name, merged_branches, today, user_name,
    work_tree,
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

/// `main` at the initial commit; `feat/PROJ-1-merged` at the same commit, so
/// it is merged; `feat/PROJ-2-open` one commit ahead, so it is not.
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
fn merged_branches_are_the_ones_already_in_the_base() {
    let repo = repo_with_branches();
    assert_eq!(
        merged_branches(repo.path(), "main").unwrap(),
        ["feat/PROJ-1-merged", "main"]
    );
}

#[test]
fn a_branch_and_its_remote_twin_are_one_branch() {
    let repo = repo_with_branches();
    repo.git(&["update-ref", "refs/remotes/origin/main", "HEAD"]);
    repo.git(&[
        "update-ref",
        "refs/remotes/origin/feat/PROJ-1-merged",
        "HEAD",
    ]);
    repo.git(&[
        "symbolic-ref",
        "refs/remotes/origin/HEAD",
        "refs/remotes/origin/main",
    ]);
    // Counted once, so a ticket does not look ambiguous because its branch
    // exists here and on the server; and `origin/HEAD` is not a branch.
    assert_eq!(
        merged_branches(repo.path(), "origin/main").unwrap(),
        ["feat/PROJ-1-merged", "main"]
    );
}

#[test]
fn a_merged_branch_that_exists_only_on_the_remote_is_found() {
    let repo = repo_with_branches();
    repo.git(&[
        "update-ref",
        "refs/remotes/origin/feat/PROJ-3-remote",
        "HEAD",
    ]);
    let merged = merged_branches(repo.path(), "main").unwrap();
    assert!(
        merged.contains(&"feat/PROJ-3-remote".to_owned()),
        "{merged:?}"
    );
}

#[test]
fn a_base_that_does_not_exist_is_git_failing_not_an_empty_list() {
    // An empty list would read as "nothing is merged" and quietly skip every
    // Plan; git's own error says what is wrong.
    let repo = repo_with_branches();
    let err = merged_branches(repo.path(), "no-such-branch").unwrap_err();
    assert!(matches!(err, GitError::Failed { .. }), "{err:?}");
}

#[test]
fn a_base_that_looks_like_an_option_is_not_taken_for_one() {
    // `--merged=--all` names a commit called `--all`; it does not turn the
    // option on and list everything.
    let repo = repo_with_branches();
    let err = merged_branches(repo.path(), "--all").unwrap_err();
    assert!(matches!(err, GitError::Failed { .. }), "{err:?}");
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
fn ref_names_are_reduced_to_branch_names() {
    assert_eq!(
        branch_name("refs/heads/feat/PROJ-1-x").as_deref(),
        Some("feat/PROJ-1-x")
    );
    assert_eq!(
        branch_name("refs/remotes/origin/feat/PROJ-1-x").as_deref(),
        Some("feat/PROJ-1-x")
    );
    assert_eq!(
        branch_name("refs/remotes/upstream/main").as_deref(),
        Some("main")
    );
    for not_a_branch in [
        "refs/remotes/origin/HEAD",
        "refs/remotes/origin",
        "refs/tags/v1",
        "(HEAD detached at 1a2b3c4)",
        "",
    ] {
        assert_eq!(branch_name(not_a_branch), None, "{not_a_branch:?}");
    }
}
