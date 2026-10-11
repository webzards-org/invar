//! End-to-end tests for `invar done` (M1-12): the real binary, a real
//! project, and git's clock fixed through `GIT_COMMITTER_DATE`.

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use predicates::str::contains;
use tempfile::TempDir;

const SPEC: &str = ".invar/plans/SHOP-214-cancel-unpaid-order/spec.md";
const PLAN_DIR: &str = ".invar/plans/SHOP-214-cancel-unpaid-order";
const NOW: &str = "2026-10-06T18:37:12+02:00";

fn contract_example(number: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../contracts/formats/plan-header.md");
    let text = fs::read_to_string(path).unwrap();
    let after = text.split_once(&format!("### {number} ")).unwrap().1;
    let body = after.split_once("```markdown\n").unwrap().1;
    body.split_once("\n```\n").unwrap().0.to_owned() + "\n"
}

fn draft_plan() -> String {
    contract_example("9.1")
}

fn approved_plan() -> String {
    let draft = draft_plan();
    contract_example("9.2") + draft.split_once("\n---\n").unwrap().1
}

struct Project {
    dir: TempDir,
}

impl Project {
    fn new(plan: &str) -> Self {
        let project = Self {
            dir: tempfile::tempdir().unwrap(),
        };
        fs::write(project.root().join("invar.yaml"), "").unwrap();
        fs::write(
            project.root().join(".gitconfig-test"),
            "[user]\n\tname = Test\n\temail = test@example.com\n",
        )
        .unwrap();
        let spec = project.root().join(SPEC);
        fs::create_dir_all(spec.parent().unwrap()).unwrap();
        fs::write(spec, plan).unwrap();
        project
    }

    fn root(&self) -> PathBuf {
        self.dir.path().canonicalize().unwrap()
    }

    fn read(&self, relative: &str) -> String {
        fs::read_to_string(self.root().join(relative)).unwrap()
    }

    fn git(&self, args: &[&str]) {
        let status = std::process::Command::new("git")
            .args(args)
            .current_dir(self.root())
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", self.root().join(".gitconfig-test"))
            .env("HOME", self.root())
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?} failed");
    }

    fn done(&self, args: &[&str]) -> assert_cmd::assert::Assert {
        let mut all = vec!["done"];
        all.extend_from_slice(args);
        Command::cargo_bin("invar")
            .unwrap()
            .current_dir(self.root())
            .env("GIT_COMMITTER_DATE", NOW)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", self.root().join(".gitconfig-test"))
            .env("HOME", self.root())
            .args(&all)
            .assert()
    }

    fn decisions(&self) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(self.root().join(".invar/decisions"))
            .map(|dir| {
                dir.map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }
}

// --- success ------------------------------------------------------------------

#[test]
fn closes_the_cycle_and_prints_every_path() {
    let project = Project::new(&approved_plan());

    project
        .done(&["SHOP-214", "-m", "Shipped as planned."])
        .success()
        .stdout(contains("pr description: .invar/out/SHOP-214-pr.md"))
        .stdout(contains(
            ".invar/decisions/2026-10-06-163712-orders-cancel-unpaid-order.md",
        ))
        .stdout(contains(", confirmed, orders)"))
        .stdout(contains(format!("removed: {PLAN_DIR}")))
        .stderr("");

    let pr = project.read(".invar/out/SHOP-214-pr.md");
    assert!(pr.starts_with("# SHOP-214"), "{pr}");

    assert_eq!(
        project.decisions(),
        ["2026-10-06-163712-orders-cancel-unpaid-order.md"]
    );
    let entry = project.read(".invar/decisions/2026-10-06-163712-orders-cancel-unpaid-order.md");
    assert!(entry.contains("type: confirmed"), "{entry}");
    assert!(entry.contains("Shipped as planned."), "{entry}");

    assert!(!project.root().join(PLAN_DIR).exists());
}

#[test]
fn the_body_can_come_from_stdin() {
    let project = Project::new(&approved_plan());
    Command::cargo_bin("invar")
        .unwrap()
        .current_dir(project.root())
        .env("GIT_COMMITTER_DATE", NOW)
        .args(["done", "SHOP-214"])
        .write_stdin("Built from stdin.\n")
        .assert()
        .success();
    let entry = project.read(".invar/decisions/2026-10-06-163712-orders-cancel-unpaid-order.md");
    assert!(entry.contains("Built from stdin."), "{entry}");
}

// --- refusals -------------------------------------------------------------------

#[test]
fn refuses_on_a_draft_plan_and_writes_nothing() {
    let project = Project::new(&draft_plan());

    project
        .done(&["SHOP-214", "-m", "x"])
        .failure()
        .stderr(contains("is still a draft"));

    assert!(!project.root().join(".invar/out").exists());
    assert!(!project.root().join(".invar/decisions").exists());
    assert!(project.root().join(SPEC).exists());
}

#[test]
fn a_refusal_is_printed_once() {
    // An error whose cause repeats its message would print it twice.
    let project = Project::new(&draft_plan());

    let assert = project.done(&["SHOP-214", "-m", "x"]).failure();

    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert_eq!(stderr.matches("is still a draft").count(), 1, "{stderr}");
}

#[test]
fn a_plan_that_is_not_valid_lists_its_problems_and_writes_nothing() {
    // The refusal says "fix every problem listed above": the list has to be
    // there, on stdout, as `plan validate` prints it.
    let project = Project::new(
        &approved_plan().replace("## Acceptance criteria\n", "## Acceptance criterion\n"),
    );

    project
        .done(&["SHOP-214", "-m", "x"])
        .failure()
        .stdout(contains("spec.md:"))
        .stdout(contains("Acceptance criteria"))
        .stderr(contains("the Plan is not valid"))
        .stderr(contains("listed above"));

    assert!(!project.root().join(".invar/out").exists());
    assert!(!project.root().join(".invar/decisions").exists());
    assert!(project.root().join(SPEC).exists());
}

#[test]
fn refuses_on_a_plan_changed_after_approval_and_writes_nothing() {
    let project = Project::new(&approved_plan());
    let edited = project
        .read(SPEC)
        .replace("refused with 403", "refused with 409");
    fs::write(project.root().join(SPEC), edited).unwrap();

    project
        .done(&["SHOP-214", "-m", "x"])
        .failure()
        .stderr(contains("changed after it was approved"));

    assert!(!project.root().join(".invar/out").exists());
    assert!(!project.root().join(".invar/decisions").exists());
}

// --- all-or-nothing: forcing a failure at each later step -----------------------

#[test]
fn a_blocked_pr_description_leaves_no_entry_and_the_plan_in_place() {
    let project = Project::new(&approved_plan());
    fs::create_dir_all(project.root().join(".invar")).unwrap();
    fs::write(project.root().join(".invar/out"), "").unwrap();

    project.done(&["SHOP-214", "-m", "x"]).failure();

    assert!(!project.root().join(".invar/decisions").exists());
    assert!(project.root().join(PLAN_DIR).exists());
}

#[test]
fn a_blocked_decision_log_leaves_the_pr_description_but_not_the_plan_removed() {
    let project = Project::new(&approved_plan());
    fs::create_dir_all(project.root().join(".invar")).unwrap();
    fs::write(project.root().join(".invar/decisions"), "").unwrap();

    project.done(&["SHOP-214", "-m", "x"]).failure();

    assert!(project.root().join(".invar/out/SHOP-214-pr.md").is_file());
    assert!(project.root().join(PLAN_DIR).exists());
}

#[cfg(unix)]
#[test]
fn a_plan_that_cannot_be_removed_still_has_its_entry_on_record() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    let is_root = {
        let probe = tempfile::NamedTempFile::new().unwrap();
        probe.as_file().metadata().unwrap().uid() == 0
    };
    if is_root {
        eprintln!("skipped: running as root, permissions are not enforced");
        return;
    }

    let project = Project::new(&approved_plan());
    let plans_dir = project.root().join(".invar/plans");
    fs::set_permissions(&plans_dir, fs::Permissions::from_mode(0o500)).unwrap();

    let result = project.done(&["SHOP-214", "-m", "x"]);

    fs::set_permissions(&plans_dir, fs::Permissions::from_mode(0o700)).unwrap();

    result
        .failure()
        .stderr(contains("could not be removed"))
        .stderr(contains("not fully closed"));
    assert_eq!(
        project.decisions(),
        ["2026-10-06-163712-orders-cancel-unpaid-order.md"]
    );
    assert!(project.root().join(PLAN_DIR).exists());
}

// --- uncommitted changes: warn, never refuse ------------------------------------

#[test]
fn warns_but_does_not_refuse_on_uncommitted_changes_outside_invar() {
    let project = Project::new(&approved_plan());
    project.git(&["init", "-q"]);
    project.git(&["add", "-A"]);
    project.git(&["commit", "-q", "-m", "initial"]);
    fs::write(project.root().join("app.php"), "<?php // uncommitted\n").unwrap();

    project
        .done(&["SHOP-214", "-m", "Shipped."])
        .success()
        .stderr(contains("warning:"))
        .stderr(contains("app.php"));

    assert!(!project.root().join(SPEC).exists());
}

#[test]
fn outside_a_git_repository_the_check_is_silent() {
    // No `git init`: `git status` fails, and the warning is simply skipped.
    let project = Project::new(&approved_plan());
    project
        .done(&["SHOP-214", "-m", "Shipped."])
        .success()
        .stderr("");
}
