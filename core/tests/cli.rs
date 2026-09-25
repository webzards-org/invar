//! End-to-end tests: run the real binary, assert on what a user would see.
//!
//! Unit tests live next to the code they test, in `#[cfg(test)] mod tests`
//! blocks. This file is for the CLI surface itself — arguments, output, exit
//! codes — because that surface is what CI, the hooks and a developer in a
//! terminal all depend on (§13b).

use assert_cmd::Command;
use predicates::str::contains;

fn invar() -> Command {
    Command::cargo_bin("invar").expect("the `invar` binary should be built")
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

#[test]
fn unimplemented_command_fails_loudly() {
    // A stub must not exit 0. Silence that looks like success is the one
    // failure mode the whole tool exists to prevent.
    invar()
        .args(["hash", "some-file.md"])
        .assert()
        .failure()
        .stderr(contains("not implemented"));
}
