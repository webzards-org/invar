//! End-to-end tests: run the real binary, assert on what a user would see.
//!
//! Unit tests live next to the code they test, in `#[cfg(test)] mod tests`
//! blocks. This file is for the CLI surface itself — arguments, output, exit
//! codes — because that surface is what CI, the hooks and a developer in a
//! terminal all depend on (§13b).

use std::fs;
use std::path::PathBuf;

use assert_cmd::Command;
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
