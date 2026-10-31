//! End-to-end tests: run the real binary, assert on what a user would see.
//!
//! Unit tests live next to the code they test, in `#[cfg(test)] mod tests`
//! blocks. This file is for the CLI surface itself — arguments, output, exit
//! codes — because that surface is what CI, the hooks and a developer in a
//! terminal all depend on (§13b).

use std::fs;
use std::path::PathBuf;

use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt as _;
use predicates::str::{contains, diff};

fn invar() -> Command {
    Command::cargo_bin("invar").expect("the `invar` binary should be built")
}

fn fixture(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../contracts/fixtures/digest")
        .join(path)
}

#[test]
fn version_flag_prints_the_crate_version() {
    invar()
        .arg("--version")
        .assert()
        .success()
        .stdout(contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn no_arguments_is_a_usage_error() {
    invar().assert().failure().stderr(contains("Usage"));
}

// --- invar hash -------------------------------------------------------------

/// `invar hash --lock` on a file holding `text`.
fn hash_lock(text: &str) -> assert_cmd::assert::Assert {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("test.py");
    std::fs::write(&file, text).unwrap();
    Command::cargo_bin("invar")
        .unwrap()
        .args(["hash", "--lock"])
        .arg(&file)
        .assert()
}

/// The digest of the contract's worked example (acceptance-lock.md §8.2).
const LOCK_DIGEST: &str = "sha256:5d5da6ccd3116bee608b2a6c15d4c9f0d20f32b17b220cc13a643169ab874e07";

#[test]
fn hash_lock_prints_the_digest_alone_and_says_what_the_line_records() {
    let body = "def test_ac1():\n    assert False\n";
    hash_lock(&format!("# invar-lock: SHOP-214\n{body}"))
        .success()
        .stdout(format!("{LOCK_DIGEST}\n"))
        .stderr(predicates::str::contains(":1: SHOP-214: pending"));
    hash_lock(&format!("# invar-lock: SHOP-214 {LOCK_DIGEST}\r\n{body}"))
        .success()
        .stdout(format!("{LOCK_DIGEST}\n"))
        .stderr(predicates::str::contains("the digest matches"));
    hash_lock(&format!("# invar-lock: SHOP-214 {LOCK_DIGEST}\n{body}x\n"))
        .success()
        .stderr(predicates::str::contains("no longer matches"))
        .stdout(predicates::str::contains(LOCK_DIGEST).not());
}

#[test]
fn hash_lock_refuses_a_file_with_no_line_or_a_malformed_one() {
    hash_lock("x\n")
        .failure()
        .stdout("")
        .stderr(predicates::str::contains("no `invar-lock:` line"));
    hash_lock("# invar-lock: SHOP-214 sha256:abc\n")
        .failure()
        .stdout("")
        .stderr(predicates::str::contains("line 1"));
}

#[test]
fn hash_prints_the_digest_of_a_file() {
    // Expected value copied from `digests.json`, group `simple`. CRLF on
    // purpose: the command must canonicalize, not hash raw bytes.
    invar()
        .arg("hash")
        .arg(fixture("simple/crlf.txt"))
        .assert()
        .success()
        .stdout(diff(
            "sha256:e49c81e2d2f84e259d40e2fb8192f3bcd198b355184845d76d8f58807d0d78ee\n",
        ));
}

#[test]
fn hash_of_an_empty_file_is_the_sha256_of_zero_bytes() {
    invar()
        .arg("hash")
        .arg(fixture("empty/bom-and-blanks.txt"))
        .assert()
        .success()
        .stdout(diff(
            "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855\n",
        ));
}

#[test]
fn hash_of_a_missing_file_fails_loudly() {
    // Silence that looks like success is the one failure mode the whole tool
    // exists to prevent: non-zero exit, the path named, nothing on stdout.
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("no-such-file.md");

    invar()
        .arg("hash")
        .arg(&missing)
        .assert()
        .failure()
        .stdout("")
        .stderr(contains("error: reading"))
        .stderr(contains("no-such-file.md"));
}

#[test]
fn hash_of_invalid_utf8_fails_with_the_offset() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("latin1.txt");
    fs::write(&file, b"Beograd \xE8\n").unwrap();

    invar()
        .arg("hash")
        .arg(&file)
        .assert()
        .failure()
        .stdout("")
        .stderr(contains("not valid UTF-8"))
        .stderr(contains("offset 8"));
}

#[test]
fn hash_help_says_the_whole_file_is_hashed() {
    invar()
        .args(["hash", "--help"])
        .assert()
        .success()
        .stdout(contains("whole file is hashed"));
}

#[test]
fn hash_appears_in_the_top_level_help() {
    invar()
        .arg("--help")
        .assert()
        .success()
        .stdout(contains("hash"))
        .stdout(contains("whole file"));
}

// --- exit codes -------------------------------------------------------------

#[test]
fn a_usage_error_never_exits_with_code_2() {
    // clap's own default for a usage error is 2. Here 2 means one thing:
    // `plan guard` denying a tool call, which Claude Code blocks on
    // (plugin/NOTES.md §11). A hook started with arguments this binary does not
    // know would otherwise block every Write, Edit and Bash call in a session.
    for args in [
        &["bogus"][..],
        &["plan", "bogus"],
        &["plan", "guard", "--bogus"],
        &["hash"],
        &["decision", "record"],
    ] {
        invar()
            .args(args)
            .assert()
            .failure()
            .code(1)
            .stdout("")
            .stderr(contains("Usage"));
    }
}

#[test]
fn help_and_version_still_exit_zero() {
    for args in [
        &["--help"][..],
        &["--version"],
        &["plan", "--help"],
        &["plan", "guard", "--help"],
        &["decision", "record", "--help"],
    ] {
        invar().args(args).assert().success().stderr("");
    }
}
