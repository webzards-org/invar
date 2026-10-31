//! End-to-end tests for `invar plan check` (M1-10): the real binary, a real
//! project, and the exit code `done` and the agent rely on.

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use predicates::prelude::*;
use predicates::str::contains;
use tempfile::TempDir;

const SPEC: &str = ".invar/plans/SHOP-214-cancel-unpaid-order/spec.md";
const CHECKSUM: &str = "sha256:b9caa4c64bc83723bb4e930d7fedeae733950c5210ee0e30f4ece84eb2c2cf7b";

fn contract_example(number: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../contracts/formats/plan-header.md");
    let text = fs::read_to_string(path).unwrap();
    let after = text.split_once(&format!("### {number} ")).unwrap().1;
    let body = after.split_once("```markdown\n").unwrap().1;
    body.split_once("\n```\n").unwrap().0.to_owned() + "\n"
}

/// The approved Plan of the contract: example 9.2's header on 9.1's body.
fn approved() -> String {
    let draft = contract_example("9.1");
    contract_example("9.2") + draft.split_once("\n---\n").unwrap().1
}

struct Project {
    dir: TempDir,
}

impl Project {
    fn new(text: &str) -> Self {
        let project = Self {
            dir: tempfile::tempdir().unwrap(),
        };
        fs::write(project.root().join("invar.yaml"), "").unwrap();
        fs::create_dir_all(project.spec().parent().unwrap()).unwrap();
        fs::write(project.spec(), text).unwrap();
        project
    }

    fn root(&self) -> PathBuf {
        self.dir.path().canonicalize().unwrap()
    }

    fn spec(&self) -> PathBuf {
        self.root().join(SPEC)
    }

    fn check_in(&self, cwd: &Path, target: &str) -> assert_cmd::assert::Assert {
        Command::cargo_bin("invar")
            .unwrap()
            .current_dir(cwd)
            .args(["plan", "check", target])
            .assert()
    }

    fn check(&self) -> assert_cmd::assert::Assert {
        self.check_in(&self.root(), "SHOP-214")
    }
}

// --- passes ---------------------------------------------------------------------

#[test]
fn an_approved_unchanged_plan_passes() {
    Project::new(&approved())
        .check()
        .success()
        .stdout(format!(
            "ok: {SPEC} — approved by rade on 2026-10-05, unchanged since ({CHECKSUM})\n"
        ))
        .stderr("");
}

#[test]
fn it_works_from_any_subdirectory_and_by_path() {
    let project = Project::new(&approved());
    let deep = project.root().join("app/Http/Controllers");
    fs::create_dir_all(&deep).unwrap();
    project.check_in(&deep, "SHOP-214").success();
    project
        .check_in(&deep, "../../../.invar/plans/SHOP-214-cancel-unpaid-order")
        .success();
}

#[test]
fn an_approved_plan_checked_out_with_crlf_still_passes() {
    Project::new(&approved().replace('\n', "\r\n"))
        .check()
        .success()
        .stdout(contains(CHECKSUM));
}

// --- refusals ---------------------------------------------------------------------

#[test]
fn a_draft_is_refused() {
    Project::new(&contract_example("9.1"))
        .check()
        .failure()
        .code(1)
        .stdout("")
        .stderr(contains(
            "the Plan for SHOP-214 is still a draft: nobody has approved it",
        ))
        .stderr(contains("Do not start implementing"))
        .stderr(contains("`invar plan approve SHOP-214`"));
}

#[test]
fn a_plan_changed_after_approval_is_refused() {
    let edited = approved().replace(
        "then the request is\n  refused with 403",
        "then the request is\n  refused with 409",
    );
    assert_ne!(edited, approved(), "the edit must hit the body");
    Project::new(&edited)
        .check()
        .failure()
        .code(1)
        .stderr(contains(
            "the Plan for SHOP-214 changed after it was approved",
        ))
        .stderr(contains("rade approved it on 2026-10-05"))
        .stderr(contains("--reapprove"));
}

#[test]
fn an_approval_without_a_checksum_is_refused() {
    // Example 9.3: `Status: approved` written by hand.
    let text = approved().replace(
        &format!("Approved-checksum: {CHECKSUM}"),
        "Approved-checksum:",
    );
    Project::new(&text)
        .check()
        .failure()
        .code(1)
        .stderr(contains(
            "says `Status: approved`, but `Approved-checksum` is empty",
        ))
        .stderr(contains("no `invar plan approve` recorded this approval"));
}

#[test]
fn a_plan_that_is_not_valid_is_refused_with_its_problems() {
    let text = approved().replace("## Intent\n", "## intent\n");
    Project::new(&text)
        .check()
        .failure()
        .code(1)
        .stdout(contains(format!("{SPEC}:")).and(contains("`## intent` should be `## Intent`")))
        .stderr(contains(
            "the Plan is not valid (1 problem), so whether it is approved cannot be checked",
        ));
}

#[test]
fn a_missing_plan_is_an_error() {
    let project = Project::new(&approved());
    project
        .check_in(&project.root(), "SHOP-999")
        .failure()
        .stderr(contains("no Plan for `SHOP-999`"));
}

// --- a Plan approved before criteria had ids (plan-header.md D-16) ----------------

const CHECKSUM_1_5_0: &str =
    "sha256:f3689f4ddfed1a683df95a45a12d36a7eaab8d49f6a077abfc5b5ecae13bff44";

fn approved_under_1_5_0() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../contracts/fixtures/plan/approved-1.5.0-no-ids.md");
    fs::read_to_string(path).unwrap()
}

/// The same Plan with an id in front of every criterion.
fn numbered_after_the_fact() -> String {
    let mut number = 0;
    approved_under_1_5_0()
        .lines()
        .map(|line| match line.strip_prefix("- Given") {
            Some(rest) => {
                number += 1;
                format!("- AC{number}: Given{rest}\n")
            }
            None => format!("{line}\n"),
        })
        .collect()
}

#[test]
fn a_plan_approved_under_1_5_0_with_no_ids_still_passes() {
    Project::new(&approved_under_1_5_0())
        .check()
        .success()
        .stdout(contains(CHECKSUM_1_5_0))
        .stderr("");
}

#[test]
fn adding_ids_to_a_plan_approved_under_1_5_0_stales_its_approval() {
    Project::new(&numbered_after_the_fact())
        .check()
        .failure()
        .stderr(contains("changed after it was approved"));
}

#[test]
fn an_approved_plan_with_some_ids_and_a_broken_rule_is_refused() {
    let text = approved_under_1_5_0().replacen(
        "- Given a pending, unpaid order",
        "- AC1: Given a pending, unpaid order",
        1,
    );
    Project::new(&text)
        .check()
        .failure()
        .stdout(contains("this criterion has no id"));
}

#[test]
fn claiming_approval_to_escape_the_ids_gets_no_further_than_validate() {
    let text = approved_under_1_5_0().replace(CHECKSUM_1_5_0, "");
    Project::new(&text)
        .check()
        .failure()
        .stderr(contains("recorded this approval"));
}
