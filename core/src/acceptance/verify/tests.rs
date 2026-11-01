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
