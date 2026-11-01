//! Tests for the parts of `invar acceptance lock`, on their own.
//! `tests/acceptance_lock.rs` covers the command.

use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

use tempfile::TempDir;

use super::find::{FindError, find};
use super::run::{self, RunError, TIME_LIMIT, argv};
use crate::fs::ProjectRoot;

const DIR: &str = "tests/Acceptance";

fn project() -> (TempDir, ProjectRoot) {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("invar.yaml"), "").unwrap();
    fs::create_dir_all(dir.path().join(DIR)).unwrap();
    let root = ProjectRoot::discover_from(dir.path()).unwrap();
    (dir, root)
}

fn write(root: &ProjectRoot, path: &str, text: &str) {
    let path = root.path().join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

/// A file locked for `ticket`, with the digest `lock` itself computes.
fn locked_text(ticket: &str, rest: &str) -> String {
    let pending = format!("# invar-lock: {ticket}\n{rest}");
    String::from_utf8(crate::format::lock::with_digest(pending.as_bytes(), ticket).unwrap())
        .unwrap()
}

// --- find -----------------------------------------------------------------------

#[test]
fn files_are_sorted_into_pending_and_locked_for_this_ticket_only() {
    let (_dir, root) = project();
    write(&root, "tests/Acceptance/z.py", "# invar-lock: SHOP-214\n");
    write(
        &root,
        "tests/Acceptance/sub/a.py",
        "# invar-lock: SHOP-214\n",
    );
    write(
        &root,
        "tests/Acceptance/locked.py",
        &locked_text("SHOP-214", "def test(): pass\n"),
    );
    write(
        &root,
        "tests/Acceptance/other.py",
        "# invar-lock: SHOP-999\n",
    );
    write(&root, "tests/Acceptance/helper.py", "def helper(): pass\n");

    let found = find(&root, DIR, "SHOP-214").unwrap();

    let to_lock: Vec<&str> = found.to_lock.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(
        to_lock,
        ["tests/Acceptance/sub/a.py", "tests/Acceptance/z.py"]
    );
    assert_eq!(found.locked, ["tests/Acceptance/locked.py"]);
    assert_eq!(found.to_lock[1].bytes, b"# invar-lock: SHOP-214\n");
    assert_eq!(
        found.all_paths(),
        [
            "tests/Acceptance/locked.py",
            "tests/Acceptance/sub/a.py",
            "tests/Acceptance/z.py"
        ]
    );
}

#[test]
fn a_file_without_the_marker_is_skipped_even_when_it_is_not_utf8() {
    let (_dir, root) = project();
    write(&root, "tests/Acceptance/a.py", "# invar-lock: SHOP-214\n");
    fs::write(
        root.path().join("tests/Acceptance/logo.png"),
        [0xff, 0xfe, 0x00],
    )
    .unwrap();
    assert_eq!(find(&root, DIR, "SHOP-214").unwrap().to_lock.len(), 1);
}

#[test]
fn malformed_and_tampered_files_are_all_named() {
    let (_dir, root) = project();
    write(&root, "tests/Acceptance/a.py", "# invar-lock: shop-214\n");
    write(&root, "tests/Acceptance/ok.py", "# invar-lock: SHOP-214\n");
    let tampered = locked_text("SHOP-214", "def test(): assert False\n")
        .replace("assert False", "assert True");
    write(&root, "tests/Acceptance/tampered.py", &tampered);
    // Malformed is refused for any ticket: which one it was for cannot be read.
    let mut bad = b"# invar-lock: SHOP-214\n".to_vec();
    bad.push(0xff);
    fs::write(root.path().join("tests/Acceptance/bytes.py"), bad).unwrap();

    let message = find(&root, DIR, "SHOP-214").unwrap_err().to_string();

    assert!(
        message.contains("tests/Acceptance/a.py: line 1:"),
        "{message}"
    );
    assert!(message.contains("tests/Acceptance/bytes.py:"), "{message}");
    assert!(
        message.contains("tests/Acceptance/tampered.py: line 1: it changed after it was locked"),
        "{message}"
    );
    assert!(!message.contains("ok.py"), "{message}");
}

#[test]
fn nothing_to_lock_says_what_the_test_writer_writes() {
    let (_dir, root) = project();
    write(
        &root,
        "tests/Acceptance/locked.py",
        &locked_text("SHOP-214", "def test(): pass\n"),
    );
    let expected = "nothing to lock for SHOP-214 under tests/Acceptance: the test-writer writes \
                    files with the pending line `invar-lock: SHOP-214`";
    assert_eq!(
        find(&root, DIR, "SHOP-214").unwrap_err().to_string(),
        expected
    );
}

#[test]
fn a_missing_directory_gets_the_same_message_naming_it() {
    let (_dir, root) = project();
    let err = find(&root, "tests/Nowhere", "SHOP-214").unwrap_err();
    assert!(matches!(err, FindError::Nothing { .. }));
    assert!(err.to_string().contains("under tests/Nowhere"), "{err}");
}

#[test]
fn a_dir_written_with_a_dot_gives_the_same_relative_paths() {
    let (_dir, root) = project();
    write(&root, "tests/Acceptance/a.py", "# invar-lock: SHOP-214\n");
    let found = find(&root, "./tests//Acceptance", "SHOP-214").unwrap();
    assert_eq!(found.to_lock[0].path, "tests/Acceptance/a.py");
}

#[test]
fn a_leftover_temporary_file_is_not_a_test() {
    let (_dir, root) = project();
    write(&root, "tests/Acceptance/a.py", "# invar-lock: SHOP-214\n");
    write(
        &root,
        "tests/Acceptance/.a.py.invar-tmp",
        &locked_text("SHOP-214", ""),
    );
    let found = find(&root, DIR, "SHOP-214").unwrap();
    assert_eq!(found.all_paths(), ["tests/Acceptance/a.py"]);
}

#[cfg(unix)]
#[test]
fn symlinks_are_not_followed() {
    use std::os::unix::fs::symlink;

    let (dir, root) = project();
    write(&root, "elsewhere/a.py", "# invar-lock: SHOP-214\n");
    write(
        &root,
        "tests/Acceptance/real.py",
        "# invar-lock: SHOP-214\n",
    );
    let outside = dir.path().join("elsewhere");
    symlink(&outside, root.path().join("tests/Acceptance/linked-dir")).unwrap();
    symlink(
        outside.join("a.py"),
        root.path().join("tests/Acceptance/linked.py"),
    )
    .unwrap();

    let found = find(&root, DIR, "SHOP-214").unwrap();
    assert_eq!(found.all_paths(), ["tests/Acceptance/real.py"]);

    // The directory itself, too (D-16).
    symlink(&outside, root.path().join("linked-root")).unwrap();
    assert!(matches!(
        find(&root, "linked-root", "SHOP-214"),
        Err(FindError::Nothing { .. })
    ));
    assert!(Path::new(&outside).join("a.py").exists());
}

// --- run ------------------------------------------------------------------------

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_owned()).collect()
}

fn sh(script: &str) -> Vec<String> {
    strings(&["sh", "-c", script, "runner", "{report}", "{files}"])
}

#[test]
fn the_placeholders_become_one_path_and_separate_files() {
    let run = strings(&[
        "pest",
        "--log-junit",
        "{report}",
        "--x={y",
        "{files}",
        "--end",
    ]);
    let files = strings(&["a b.php", "c.php"]);
    assert_eq!(
        argv(&run, Path::new("/tmp/r.xml"), &files),
        [
            "pest",
            "--log-junit",
            "/tmp/r.xml",
            "--x={y",
            "a b.php",
            "c.php",
            "--end"
        ]
    );
}

#[test]
fn the_report_and_the_output_come_back_and_the_exit_code_is_only_described() {
    let (_dir, root) = project();
    let script = "echo out; echo err >&2; pwd > where; printf '<testsuites/>' > \"$1\"; exit 3";
    let done = run::run(&root, &sh(script), &[], TIME_LIMIT).unwrap();

    assert_eq!(done.report.as_deref(), Some(&b"<testsuites/>"[..]));
    assert_eq!(done.status, "exit status: 3");
    assert_eq!(done.output.stdout, "out");
    assert_eq!(done.output.stderr, "err");
    // The working directory is the project root.
    let cwd = fs::read_to_string(root.path().join("where")).unwrap();
    assert_eq!(Path::new(cwd.trim()), root.path());
}

#[test]
fn no_report_reads_as_none() {
    let (_dir, root) = project();
    let done = run::run(&root, &sh("exit 0"), &[], TIME_LIMIT).unwrap();
    assert_eq!(done.report, None);
}

#[test]
fn only_the_last_lines_of_a_stream_are_kept() {
    let (_dir, root) = project();
    let script = "i=0; while [ $i -lt 100 ]; do echo line$i; i=$((i+1)); done";
    let done = run::run(&root, &sh(script), &[], TIME_LIMIT).unwrap();
    let lines: Vec<&str> = done.output.stdout.lines().collect();
    assert_eq!(lines.len(), 40);
    assert_eq!(lines[0], "line60");
    assert_eq!(lines[39], "line99");
}

#[test]
fn a_program_that_cannot_start_is_named() {
    let (_dir, root) = project();
    let command = strings(&["./no-such-runner", "{report}", "{files}"]);
    let err = run::run(&root, &command, &[], TIME_LIMIT).unwrap_err();
    assert!(matches!(err, RunError::CannotStart { .. }));
    let message = err.to_string();
    assert!(message.contains("`./no-such-runner`"), "{message}");
    assert!(message.contains("acceptance.run"), "{message}");
}

#[test]
fn a_run_past_the_limit_is_killed_and_says_so() {
    let (_dir, root) = project();
    // `exec`, so the process killed is the one sleeping and nothing is left behind.
    let script = "echo started; exec sleep 30";
    let started = Instant::now();
    let err = run::run(&root, &sh(script), &[], Duration::from_millis(300)).unwrap_err();

    assert!(started.elapsed() < Duration::from_secs(20), "it waited");
    let RunError::TimedOut { output, .. } = &err else {
        panic!("{err}");
    };
    assert_eq!(output.stdout, "started");
}

#[test]
fn the_time_limit_is_named_in_minutes() {
    let err = RunError::TimedOut {
        limit: TIME_LIMIT,
        output: run::Output::default(),
    };
    assert!(err.to_string().contains("within 10 minutes"), "{err}");
}
