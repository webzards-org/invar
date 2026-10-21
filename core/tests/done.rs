//! End-to-end tests for `invar done`: the real binary, a real project, and —
//! where staging is the point — a real git repository.

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use predicates::str::contains;
use tempfile::TempDir;

const SPEC: &str = ".invar/plans/SHOP-214-cancel-unpaid-order/spec.md";
const PLAN_DIR: &str = ".invar/plans/SHOP-214-cancel-unpaid-order";
const DRAFT: &str = ".invar/plans/SHOP-214-cancel-unpaid-order/decision.md";

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
            "[user]\n\tname = Test\n\temail = test@example.com\n[commit]\n\tgpgsign = false\n",
        )
        .unwrap();
        let spec = project.root().join(SPEC);
        fs::create_dir_all(spec.parent().unwrap()).unwrap();
        fs::write(spec, plan).unwrap();
        project
    }

    /// The project as a repository: everything committed on `branch`.
    fn in_git(plan: &str, branch: &str) -> Self {
        let project = Self::new(plan);
        project.git(&["init", "-q", "-b", branch]);
        project.git(&["add", "-A"]);
        project.git(&["commit", "-q", "-m", "initial"]);
        project
    }

    fn root(&self) -> PathBuf {
        self.dir.path().canonicalize().unwrap()
    }

    fn read(&self, relative: &str) -> String {
        fs::read_to_string(self.root().join(relative)).unwrap()
    }

    fn exists(&self, relative: &str) -> bool {
        self.root().join(relative).exists()
    }

    fn git(&self, args: &[&str]) -> String {
        let output = std::process::Command::new("git")
            .args(args)
            .current_dir(self.root())
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", self.root().join(".gitconfig-test"))
            .env("HOME", self.root())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    }

    fn done(&self, args: &[&str]) -> assert_cmd::assert::Assert {
        let mut all = vec!["done"];
        all.extend_from_slice(args);
        Command::cargo_bin("invar")
            .unwrap()
            .current_dir(self.root())
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", self.root().join(".gitconfig-test"))
            .env("HOME", self.root())
            .args(&all)
            .assert()
    }
}

// --- success ------------------------------------------------------------------

#[test]
fn deletes_the_plan_and_stages_the_deletion() {
    let project = Project::in_git(&approved_plan(), "feat/SHOP-214-cancel");

    project
        .done(&["SHOP-214"])
        .success()
        .stdout(contains(format!("removed: {PLAN_DIR} (staged")))
        .stderr("");

    assert!(!project.exists(PLAN_DIR));
    // Staged as a deletion, nothing else touched.
    let staged = project.git(&["diff", "--cached", "--name-status"]);
    assert_eq!(staged, format!("D\t{SPEC}\n"));
    // No entry, no PR description.
    assert!(!project.exists(".invar/decisions"));
    assert!(!project.exists(".invar/out"));
}

#[test]
fn without_a_ticket_the_branch_names_the_plan() {
    let project = Project::in_git(&approved_plan(), "feat/SHOP-214-cancel");

    project
        .done(&[])
        .success()
        .stdout(contains(format!("removed: {PLAN_DIR}")));

    assert!(!project.exists(PLAN_DIR));
}

#[test]
fn a_branch_that_names_no_plan_refuses_and_says_how_to_name_one() {
    let project = Project::in_git(&approved_plan(), "main");

    project
        .done(&[])
        .failure()
        .stderr(contains(
            "no Plan's ticket appears in the branch name `main`",
        ))
        .stderr(contains("invar done <TICKET>"));

    assert!(project.exists(SPEC));
}

#[test]
fn a_detached_head_needs_the_ticket() {
    // A CI checkout of a pull request is detached.
    let project = Project::in_git(&approved_plan(), "feat/SHOP-214-cancel");
    project.git(&["checkout", "-q", "--detach"]);

    project
        .done(&[])
        .failure()
        .stderr(contains("HEAD is not on a branch"));
    assert!(project.exists(SPEC));

    // Named, it closes.
    project.done(&["SHOP-214"]).success();
    assert!(!project.exists(PLAN_DIR));
}

#[test]
fn a_plan_never_committed_is_deleted_with_nothing_to_stage() {
    let project = Project::new(&approved_plan());
    project.git(&["init", "-q", "-b", "feat/SHOP-214-cancel"]);

    project.done(&["SHOP-214"]).success();

    assert!(!project.exists(PLAN_DIR));
    assert_eq!(project.git(&["diff", "--cached", "--name-status"]), "");
}

#[test]
fn outside_a_repository_it_deletes_and_warns_that_nothing_was_staged() {
    let project = Project::new(&approved_plan());

    project
        .done(&["SHOP-214"])
        .success()
        .stdout(contains(format!("removed: {PLAN_DIR}")))
        .stderr(contains("warning: not staged"));

    assert!(!project.exists(PLAN_DIR));
}

#[test]
fn it_works_by_path_too() {
    let project = Project::new(&approved_plan());
    project.done(&[PLAN_DIR]).success();
    assert!(!project.exists(PLAN_DIR));
}

// --- refusals -------------------------------------------------------------------

#[test]
fn refuses_while_an_unapproved_draft_entry_is_in_the_plan_directory() {
    let project = Project::in_git(&approved_plan(), "feat/SHOP-214-cancel");
    let draft = "---\nticket: SHOP-214\nmodule: orders\ntype: confirmed\n---\n\nReviewed.\n";
    fs::write(project.root().join(DRAFT), draft).unwrap();

    project
        .done(&["SHOP-214"])
        .failure()
        .stderr(contains("Approve it with /invar:log-approve, or delete it"));

    assert_eq!(project.read(DRAFT), draft);
    assert!(project.exists(SPEC));
    assert_eq!(project.git(&["diff", "--cached", "--name-status"]), "");

    // Once it is gone, the Plan goes.
    fs::remove_file(project.root().join(DRAFT)).unwrap();
    project.done(&["SHOP-214"]).success();
    assert!(!project.exists(PLAN_DIR));
}

#[test]
fn refuses_on_a_draft_plan_and_changes_nothing() {
    let project = Project::new(&draft_plan());

    project
        .done(&["SHOP-214"])
        .failure()
        .stderr(contains("is still a draft"));

    assert!(project.exists(SPEC));
}

#[test]
fn a_refusal_is_printed_once() {
    // An error whose cause repeats its message would print it twice.
    let project = Project::new(&draft_plan());

    let assert = project.done(&["SHOP-214"]).failure();

    let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
    assert_eq!(stderr.matches("is still a draft").count(), 1, "{stderr}");
}

#[test]
fn a_plan_that_is_not_valid_lists_its_problems_and_changes_nothing() {
    // The refusal says "fix every problem listed above": the list has to be
    // there, on stdout, as `plan validate` prints it.
    let project = Project::new(
        &approved_plan().replace("## Acceptance criteria\n", "## Acceptance criterion\n"),
    );

    project
        .done(&["SHOP-214"])
        .failure()
        .stdout(contains("spec.md:"))
        .stdout(contains("Acceptance criteria"))
        .stderr(contains("the Plan is not valid"))
        .stderr(contains("listed above"));

    assert!(project.exists(SPEC));
}

#[test]
fn refuses_on_a_plan_changed_after_approval_and_changes_nothing() {
    let project = Project::new(&approved_plan());
    let edited = project
        .read(SPEC)
        .replace("refused with 403", "refused with 409");
    fs::write(project.root().join(SPEC), edited).unwrap();

    project
        .done(&["SHOP-214"])
        .failure()
        .stderr(contains("changed after it was approved"));

    assert!(project.exists(SPEC));
}

#[test]
fn the_message_flag_is_gone() {
    // `done` writes no entry, so it takes no entry text.
    let project = Project::new(&approved_plan());
    project
        .done(&["SHOP-214", "-m", "x"])
        .failure()
        .stderr(contains("Usage"));
    assert!(project.exists(SPEC));
}

// --- --commit <MESSAGE> ---------------------------------------------------------

#[test]
fn commit_commits_the_deletion_alone_under_the_message() {
    let project = Project::in_git(&approved_plan(), "feat/SHOP-214-cancel");
    // Something else is staged; it must stay out of the commit.
    fs::write(project.root().join("app.php"), "<?php\n").unwrap();
    project.git(&["add", "app.php"]);

    project
        .done(&["SHOP-214", "--commit", "chore: remove the Plan of SHOP-214"])
        .success()
        .stdout(contains(format!(
            "removed: {PLAN_DIR} (committed: chore: remove the Plan of SHOP-214)"
        )))
        .stderr("");

    assert!(!project.exists(PLAN_DIR));
    assert_eq!(
        project.git(&["log", "-1", "--format=%s"]),
        "chore: remove the Plan of SHOP-214\n"
    );
    assert_eq!(
        project.git(&["diff", "--name-status", "HEAD~1", "HEAD"]),
        format!("D\t{SPEC}\n")
    );
    // Still staged, still not committed.
    assert_eq!(
        project.git(&["diff", "--cached", "--name-status"]),
        "A\tapp.php\n"
    );
}

#[test]
fn commit_without_a_ticket_uses_the_branch() {
    let project = Project::in_git(&approved_plan(), "feat/SHOP-214-cancel");
    project
        .done(&["--commit", "chore: close SHOP-214"])
        .success();
    assert_eq!(
        project.git(&["log", "-1", "--format=%s"]),
        "chore: close SHOP-214\n"
    );
}

#[test]
fn commit_of_a_plan_git_never_tracked_makes_no_commit() {
    // The Plan is on disk, and git does not track it.
    let project = Project::in_git(&approved_plan(), "feat/SHOP-214-cancel");
    project.git(&["rm", "-r", "-q", "--cached", PLAN_DIR]);
    project.git(&["commit", "-q", "-m", "untrack"]);
    let before_done = project.git(&["rev-parse", "HEAD"]);

    project
        .done(&["SHOP-214", "--commit", "chore: remove"])
        .success()
        .stdout(contains("nothing to commit"));
    assert_eq!(project.git(&["rev-parse", "HEAD"]), before_done);
    assert!(!project.exists(PLAN_DIR));
}

#[test]
fn an_empty_commit_message_refuses_and_changes_nothing() {
    let project = Project::in_git(&approved_plan(), "feat/SHOP-214-cancel");
    project
        .done(&["SHOP-214", "--commit", "  "])
        .failure()
        .stderr(contains("`--commit` needs the commit message"));
    assert!(project.exists(SPEC));
}

#[test]
fn commit_outside_a_repository_refuses_and_changes_nothing() {
    let project = Project::new(&approved_plan());
    project
        .done(&["SHOP-214", "--commit", "chore: remove"])
        .failure()
        .stderr(contains("`--commit` needs a git repository"));
    assert!(project.exists(SPEC));
}

#[cfg(unix)]
#[test]
fn a_commit_a_hook_refuses_leaves_the_deletion_staged_and_says_how_to_finish() {
    use std::os::unix::fs::PermissionsExt as _;

    let project = Project::in_git(&approved_plan(), "feat/SHOP-214-cancel");
    let hook = project.root().join(".git/hooks/pre-commit");
    fs::write(&hook, "#!/bin/sh\necho 'no commits today' >&2\nexit 1\n").unwrap();
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    let before = project.git(&["rev-parse", "HEAD"]);

    project
        .done(&["SHOP-214", "--commit", "chore: remove"])
        .failure()
        .stderr(contains("the commit was refused"))
        .stderr(contains("no commits today"))
        .stderr(contains("git commit --only"));

    // Never bypassed: no commit, the deletion staged for the developer.
    assert_eq!(project.git(&["rev-parse", "HEAD"]), before);
    assert!(!project.exists(PLAN_DIR));
    assert_eq!(
        project.git(&["diff", "--cached", "--name-status"]),
        format!("D\t{SPEC}\n")
    );
}

#[test]
fn the_help_says_what_done_does_and_does_not_do() {
    Command::cargo_bin("invar")
        .unwrap()
        .args(["done", "--help"])
        .assert()
        .success()
        .stdout(contains("stage the deletion"))
        .stdout(contains("--commit <MESSAGE>"))
        .stdout(contains("/invar:log-approve"))
        .stdout(contains("Writes no"));
}
