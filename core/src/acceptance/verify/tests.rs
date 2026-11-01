//! Tests for each part of the check, on its own. `tests/acceptance_verify.rs` covers
//! the commands, with real git repositories.

use std::fs;

use tempfile::TempDir;

use super::whole_directory;
use crate::format::config::{AcceptanceConfig, AcceptanceStyle};
use crate::format::lock::with_digest;
use crate::fs::ProjectRoot;

const DIR: &str = "tests/Acceptance";

fn project() -> (TempDir, ProjectRoot) {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("invar.yaml"), "").unwrap();
    let root = ProjectRoot::discover_from(dir.path()).unwrap();
    (dir, root)
}

fn config() -> AcceptanceConfig {
    AcceptanceConfig {
        required: true,
        style: AcceptanceStyle::Integration,
        dir: DIR.to_owned(),
        run: vec!["x".to_owned(), "{report}".to_owned(), "{files}".to_owned()],
        fakes_allowed: Vec::new(),
    }
}

fn write(root: &ProjectRoot, path: &str, bytes: &[u8]) {
    let path = root.path().join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

fn locked(ticket: &str, rest: &str) -> Vec<u8> {
    with_digest(format!("# invar-lock: {ticket}\n{rest}").as_bytes(), ticket).unwrap()
}

#[test]
fn locked_files_of_any_ticket_are_counted_and_other_files_are_not() {
    let (_dir, root) = project();
    write(
        &root,
        "tests/Acceptance/a.py",
        &locked("SHOP-1", "assert a\n"),
    );
    write(
        &root,
        "tests/Acceptance/sub/b.py",
        &locked("SHOP-2", "assert b\n"),
    );
    write(&root, "tests/Acceptance/helper.py", b"def helper(): pass\n");
    let report = whole_directory(&root, &config());
    assert!(report.holds(), "{report}");
    assert_eq!(report.locked, 2);
}

#[test]
fn a_changed_locked_file_is_named_with_its_ticket_and_both_digests() {
    let (_dir, root) = project();
    let good = String::from_utf8(locked("SHOP-1", "assert a\n")).unwrap();
    let recorded = good
        .split_whitespace()
        .find(|f| f.starts_with("sha256:"))
        .unwrap();
    let bad = good.replace("assert a", "assert b");
    write(&root, "tests/Acceptance/a.py", bad.as_bytes());

    let report = whole_directory(&root, &config());

    assert_eq!(report.problems.len(), 1, "{report}");
    let problem = &report.problems[0];
    assert!(
        problem.starts_with("tests/Acceptance/a.py: locked for SHOP-1"),
        "{problem}"
    );
    assert!(problem.contains(recorded), "{problem}");
    let now = crate::format::lock::read(bad.as_bytes())
        .unwrap()
        .unwrap()
        .digest;
    assert!(problem.contains(&now), "{problem}");
    assert_eq!(report.locked, 0);
}

#[test]
fn every_problem_is_reported_in_one_run() {
    let (_dir, root) = project();
    let tampered = String::from_utf8(locked("SHOP-1", "assert a\n"))
        .unwrap()
        .replace("assert a", "assert b");
    write(&root, "tests/Acceptance/a.py", tampered.as_bytes());
    write(&root, "tests/Acceptance/b.py", b"# invar-lock: shop-2\n");
    write(&root, "tests/Acceptance/c.py", b"# invar-lock: SHOP-3\n");

    let report = whole_directory(&root, &config());

    assert_eq!(report.problems.len(), 3, "{report}");
    let message = report.to_string();
    assert!(message.contains("(3 problems)"), "{message}");
    assert!(message.contains("a.py: locked for SHOP-1"), "{message}");
    assert!(message.contains("b.py: line 1"), "{message}");
    assert!(
        message.contains("c.py: a pending line for SHOP-3"),
        "{message}"
    );
}

#[test]
fn a_missing_directory_holds_no_files() {
    let (_dir, root) = project();
    let report = whole_directory(&root, &config());
    assert!(report.holds(), "{report}");
    assert_eq!(report.locked, 0);
}

#[test]
fn a_file_at_the_directorys_path_is_a_problem() {
    let (_dir, root) = project();
    write(&root, DIR, b"not a directory\n");
    let report = whole_directory(&root, &config());
    assert!(report.problems[0].contains("not a directory"), "{report}");
}

#[cfg(unix)]
#[test]
fn a_symlink_at_the_directorys_path_is_a_problem() {
    let (_dir, root) = project();
    write(&root, "elsewhere/a.py", b"# invar-lock: SHOP-1\n");
    fs::create_dir_all(root.path().join("tests")).unwrap();
    std::os::unix::fs::symlink(root.path().join("elsewhere"), root.path().join(DIR)).unwrap();
    let report = whole_directory(&root, &config());
    assert!(report.problems[0].contains("symlink"), "{report}");
}

// --- the branch rule (§10.2) -----------------------------------------------------

use super::Report;
use super::branch;
use crate::format::plan::{TestAction, TestToChange};

const TICKET: &str = "SHOP-214";
const OTHERS: &str = "tests/Acceptance/others.py";
const MINE: &str = "tests/Acceptance/mine.py";

/// `main` with a test locked for `SHOP-1` and one for this ticket, and `feat` checked
/// out from it.
struct Branch {
    _dir: TempDir,
    root: ProjectRoot,
}

impl Branch {
    fn new() -> Self {
        let (dir, root) = project();
        let branch = Self { _dir: dir, root };
        branch.git(&["init", "--quiet", "-b", "main"]);
        branch.write(OTHERS, &locked("SHOP-1", "assert a\n"));
        branch.write(MINE, &locked(TICKET, "assert mine\n"));
        branch.commit_all("tests");
        branch.git(&["checkout", "--quiet", "-b", "feat"]);
        branch
    }

    fn write(&self, path: &str, bytes: &[u8]) {
        write(&self.root, path, bytes);
    }

    fn git(&self, args: &[&str]) {
        git_in(self.root.path(), args);
    }

    fn commit_all(&self, message: &str) {
        self.git(&["add", "-A"]);
        self.git(&["commit", "--quiet", "--allow-empty", "-m", message]);
    }

    fn check(&self, declared: &[(&str, TestAction)]) -> Report {
        check_in(&self.root, declared, None)
    }
}

fn git_in(dir: &std::path::Path, args: &[&str]) {
    let output = std::process::Command::new("git")
        .args(["-c", "user.name=test", "-c", "user.email=test@example.com"])
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", dir.join(".no-global-config"))
        .env("HOME", dir)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn check_in(root: &ProjectRoot, declared: &[(&str, TestAction)], base: Option<&str>) -> Report {
    let declared: Vec<TestToChange> = declared
        .iter()
        .map(|(path, action)| TestToChange {
            path: (*path).to_owned(),
            action: *action,
        })
        .collect();
    let mut report = Report::default();
    branch::check(root, DIR, TICKET, &declared, base, &mut report);
    report
}

/// A new version of a file locked for `ticket`, whose digest holds, as someone writing
/// the digest by hand would leave it (D-20).
fn relocked(ticket: &str) -> Vec<u8> {
    locked(ticket, "assert weaker\n")
}

fn refused_naming(report: &Report, parts: &[&str]) {
    assert_eq!(report.problems.len(), 1, "{report:?}");
    for part in parts {
        assert!(report.problems[0].contains(part), "{part}: {report:?}");
    }
}

fn holds_with_no_warning(report: &Report) {
    assert!(report.problems.is_empty(), "{report:?}");
    assert!(report.warnings.is_empty(), "{report:?}");
}

#[test]
fn nothing_changed_holds() {
    holds_with_no_warning(&Branch::new().check(&[]));
}

#[test]
fn another_tickets_test_changed_is_refused_and_says_what_to_do() {
    let branch = Branch::new();
    branch.write(OTHERS, &relocked("SHOP-1"));
    branch.commit_all("weaken");
    refused_naming(
        &branch.check(&[]),
        &[
            OTHERS,
            "locked for SHOP-1",
            "changed on this branch",
            "`## Acceptance tests to change`",
            "— change — <reason>",
            "git checkout",
        ],
    );
}

#[test]
fn declared_with_change_it_holds() {
    let branch = Branch::new();
    branch.write(OTHERS, &relocked("SHOP-1"));
    branch.commit_all("change");
    holds_with_no_warning(&branch.check(&[(OTHERS, TestAction::Change)]));
}

#[test]
fn another_tickets_test_deleted_is_refused_and_holds_when_declared() {
    let branch = Branch::new();
    branch.git(&["rm", "--quiet", OTHERS]);
    branch.commit_all("delete");
    refused_naming(
        &branch.check(&[]),
        &[
            OTHERS,
            "locked for SHOP-1",
            "deleted on this branch",
            "— delete — <reason>",
        ],
    );
    holds_with_no_warning(&branch.check(&[(OTHERS, TestAction::Delete)]));
}

#[test]
fn the_action_has_to_match() {
    let branch = Branch::new();
    branch.git(&["rm", "--quiet", OTHERS]);
    branch.commit_all("delete");
    refused_naming(
        &branch.check(&[(OTHERS, TestAction::Change)]),
        &["deleted on this branch", "only with `change`"],
    );

    let branch = Branch::new();
    branch.write(OTHERS, &relocked("SHOP-1"));
    refused_naming(
        &branch.check(&[(OTHERS, TestAction::Delete)]),
        &["changed on this branch", "only with `delete`"],
    );
}

#[test]
fn a_rename_is_a_delete_of_the_old_path() {
    let branch = Branch::new();
    branch.git(&["mv", OTHERS, "tests/Acceptance/renamed.py"]);
    branch.commit_all("rename");
    refused_naming(&branch.check(&[]), &[OTHERS, "deleted on this branch"]);
    let declared = branch.check(&[(OTHERS, TestAction::Delete)]);
    holds_with_no_warning(&declared);
}

#[test]
fn uncommitted_changes_count_staged_or_not() {
    let branch = Branch::new();
    branch.write(OTHERS, &relocked("SHOP-1"));
    refused_naming(&branch.check(&[]), &[OTHERS, "changed"]);
    branch.git(&["add", OTHERS]);
    refused_naming(&branch.check(&[]), &[OTHERS, "changed"]);
}

#[test]
fn an_uncommitted_deletion_counts() {
    let branch = Branch::new();
    fs::remove_file(branch.root.path().join(OTHERS)).unwrap();
    refused_naming(&branch.check(&[]), &[OTHERS, "deleted"]);
}

#[test]
fn a_change_put_back_on_disk_is_still_a_change() {
    // Staged, with the file on disk as it was: the next commit would hold the change.
    let branch = Branch::new();
    branch.write(OTHERS, &relocked("SHOP-1"));
    branch.git(&["add", OTHERS]);
    branch.write(OTHERS, &locked("SHOP-1", "assert a\n"));
    refused_naming(&branch.check(&[]), &[OTHERS, "changed"]);

    // Committed, and put back without committing that.
    let branch = Branch::new();
    branch.write(OTHERS, &relocked("SHOP-1"));
    branch.commit_all("weaken");
    branch.write(OTHERS, &locked("SHOP-1", "assert a\n"));
    refused_naming(&branch.check(&[]), &[OTHERS, "changed"]);
}

#[test]
fn taking_the_lock_off_is_a_change_though_the_covered_text_is_the_same() {
    let branch = Branch::new();
    // The digest off: pending, with the same text.
    branch.write(OTHERS, b"# invar-lock: SHOP-1\nassert a\n");
    refused_naming(&branch.check(&[]), &[OTHERS, "changed"]);
    // The whole line off.
    branch.write(OTHERS, b"assert a\n");
    refused_naming(&branch.check(&[]), &[OTHERS, "changed"]);
}

#[test]
fn naming_another_ticket_on_the_line_is_a_change_though_the_digest_holds() {
    let branch = Branch::new();
    let text = String::from_utf8(locked("SHOP-1", "assert a\n")).unwrap();
    branch.write(OTHERS, text.replace("SHOP-1", TICKET).as_bytes());
    refused_naming(
        &branch.check(&[]),
        &[OTHERS, "locked for SHOP-1", "changed"],
    );
}

#[cfg(unix)]
#[test]
fn a_symlink_in_the_files_place_is_a_change() {
    let branch = Branch::new();
    branch.write("elsewhere.py", &locked("SHOP-1", "assert a\n"));
    let path = branch.root.path().join(OTHERS);
    fs::remove_file(&path).unwrap();
    std::os::unix::fs::symlink(branch.root.path().join("elsewhere.py"), &path).unwrap();
    refused_naming(&branch.check(&[]), &[OTHERS, "changed"]);
}

#[test]
fn line_endings_alone_are_not_a_change() {
    let branch = Branch::new();
    let text = String::from_utf8(locked("SHOP-1", "assert a\n")).unwrap();
    branch.write(OTHERS, text.replace('\n', "\r\n").as_bytes());
    holds_with_no_warning(&branch.check(&[]));
}

#[test]
fn the_tickets_own_file_needs_no_declaration() {
    let branch = Branch::new();
    branch.write(MINE, &relocked(TICKET));
    branch.commit_all("second cycle");
    holds_with_no_warning(&branch.check(&[]));
    fs::remove_file(branch.root.path().join(MINE)).unwrap();
    holds_with_no_warning(&branch.check(&[]));
}

#[test]
fn a_new_file_and_a_file_not_locked_at_the_merge_base_need_no_declaration() {
    let branch = Branch::new();
    branch.write("tests/Acceptance/pending.py", b"# invar-lock: SHOP-1\n");
    branch.commit_all("pending");
    branch.git(&["branch", "--quiet", "--force", "main"]);
    branch.write("tests/Acceptance/new.py", &locked("SHOP-2", "assert new\n"));
    branch.write(
        "tests/Acceptance/pending.py",
        b"# invar-lock: SHOP-1\nmore\n",
    );
    holds_with_no_warning(&branch.check(&[]));
}

#[test]
fn what_landed_on_the_base_after_the_branch_left_is_not_this_branchs_change() {
    let branch = Branch::new();
    branch.git(&["checkout", "--quiet", "main"]);
    branch.write(OTHERS, &relocked("SHOP-1"));
    branch.commit_all("a change on main");
    branch.git(&["checkout", "--quiet", "feat"]);
    holds_with_no_warning(&branch.check(&[]));
}

#[test]
fn a_declared_path_that_is_not_a_locked_test_is_a_warning_and_the_real_change_is_refused() {
    let branch = Branch::new();
    branch.write(OTHERS, &relocked("SHOP-1"));
    let typo = "tests/Acceptance/othres.py";
    let report = branch.check(&[(typo, TestAction::Change)]);
    refused_naming(&report, &[OTHERS, "changed"]);
    assert_eq!(report.warnings.len(), 1, "{report:?}");
    assert!(report.warnings[0].contains(typo), "{report:?}");
    assert!(
        report.warnings[0].contains("not a locked test"),
        "{report:?}"
    );
}

#[test]
fn a_declared_path_that_did_not_change_is_a_warning() {
    let report = Branch::new().check(&[(OTHERS, TestAction::Change)]);
    assert!(report.problems.is_empty(), "{report:?}");
    assert!(report.warnings[0].contains("did not change"), "{report:?}");
}

#[test]
fn a_declared_path_that_breaks_the_rules_is_refused_and_cannot_excuse_anything() {
    let branch = Branch::new();
    let report = branch.check(&[(
        "tests/Acceptance/../Acceptance/others.py",
        TestAction::Change,
    )]);
    refused_naming(&report, &["`..`", "plan-header.md §6.5"]);
    branch.write(OTHERS, &relocked("SHOP-1"));
    let report = branch.check(&[(
        "tests/Acceptance/../Acceptance/others.py",
        TestAction::Change,
    )]);
    assert_eq!(report.problems.len(), 2, "{report:?}");
}

#[test]
fn every_problem_is_reported_in_one_run_in_the_branch_rule_too() {
    let branch = Branch::new();
    branch.write(
        "tests/Acceptance/other2.py",
        &locked("SHOP-2", "assert b\n"),
    );
    branch.commit_all("another locked test");
    branch.git(&["branch", "--quiet", "--force", "main"]);
    branch.write(OTHERS, &relocked("SHOP-1"));
    fs::remove_file(branch.root.path().join("tests/Acceptance/other2.py")).unwrap();
    let report = branch.check(&[]);
    assert_eq!(report.problems.len(), 2, "{report:?}");
}

#[test]
fn outside_a_repository_it_warns_and_still_checks_the_paths() {
    let (_dir, root) = project();
    let report = check_in(&root, &[], None);
    assert!(report.problems.is_empty(), "{report:?}");
    assert!(
        report.warnings[0].contains("not in a git repository"),
        "{report:?}"
    );

    let report = check_in(&root, &[("../a.py", TestAction::Change)], None);
    assert_eq!(report.problems.len(), 1, "{report:?}");
}

#[test]
fn a_base_branch_that_is_not_there_is_a_problem_and_not_a_way_past_the_rule() {
    let branch = Branch::new();
    let report = check_in(&branch.root, &[], Some("develop"));
    refused_naming(
        &report,
        &["cannot be compared", "develop", "git.base_branch"],
    );
}

#[test]
fn a_project_below_the_top_level_of_its_repository_works() {
    let dir = tempfile::tempdir().unwrap();
    let top = dir.path().canonicalize().unwrap();
    let project = top.join("apps/shop");
    fs::create_dir_all(project.join(DIR)).unwrap();
    fs::write(project.join("invar.yaml"), "").unwrap();
    fs::write(project.join(OTHERS), locked("SHOP-1", "assert a\n")).unwrap();
    git_in(&top, &["init", "--quiet", "-b", "main"]);
    git_in(&top, &["add", "-A"]);
    git_in(&top, &["commit", "--quiet", "-m", "tests"]);
    git_in(&top, &["checkout", "--quiet", "-b", "feat"]);
    let root = ProjectRoot::discover_from(&project).unwrap();
    holds_with_no_warning(&check_in(&root, &[], None));

    fs::write(project.join(OTHERS), relocked("SHOP-1")).unwrap();
    refused_naming(&check_in(&root, &[], None), &[OTHERS, "changed"]);
    holds_with_no_warning(&check_in(&root, &[(OTHERS, TestAction::Change)], None));
}
