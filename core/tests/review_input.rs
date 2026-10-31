//! End-to-end tests for `invar review-input` (M1-20): the real binary, a real
//! git repository with a base branch and a feature branch.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as Process;

use assert_cmd::Command;
use predicates::str::contains;
use tempfile::TempDir;

const SPEC: &str = ".invar/plans/SHOP-214-cancel-unpaid-order/spec.md";
const ACTION: &str = "app/Actions/Orders/CancelOrder.php";
const WHY: &str =
    "a review checks the branch against an approved specification, and this Plan is not one";

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

/// Example 9.1's body under example 9.2's header: approved, and current.
fn approved_plan() -> String {
    contract_example("9.2") + draft_plan().split_once("\n---\n").unwrap().1
}

/// A repository with `main` holding the first version of the action, and a
/// branch `SHOP-214-cancel-unpaid-order` checked out on top of it.
struct Repo {
    dir: TempDir,
}

impl Repo {
    fn new(plan: &str) -> Self {
        let repo = Self {
            dir: tempfile::tempdir().unwrap(),
        };
        repo.git(&["init", "--quiet", "--initial-branch=main"]);
        repo.write("invar.yaml", "");
        repo.write(ACTION, "<?php\n\nclass CancelOrder\n{\n}\n");
        repo.commit("initial");
        repo.git(&["checkout", "--quiet", "-b", "SHOP-214-cancel-unpaid-order"]);
        repo.write(SPEC, plan);
        repo
    }

    fn root(&self) -> PathBuf {
        self.dir.path().canonicalize().unwrap()
    }

    fn write(&self, relative: &str, text: &str) {
        let path = self.root().join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    /// git with a fixed identity and none of this machine's own configuration.
    fn git(&self, args: &[&str]) {
        let status = Process::new("git")
            .args(args)
            .current_dir(self.root())
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@example.com")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "test@example.com")
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?}");
    }

    fn commit(&self, message: &str) {
        self.git(&["add", "--all", "--", ".", ":(exclude).invar"]);
        self.git(&["commit", "--quiet", "--allow-empty", "-m", message]);
    }

    /// The branch's change: the action gets its body.
    fn implement(&self) {
        self.write(
            ACTION,
            "<?php\n\nclass CancelOrder\n{\n    public function handle(Order $order): void\n    {\n        $order->cancel();\n    }\n}\n",
        );
        self.commit("SHOP-214: cancel an unpaid order");
    }

    fn review_input(&self) -> assert_cmd::assert::Assert {
        Command::cargo_bin("invar")
            .unwrap()
            .current_dir(self.root())
            .args(["review-input", "SHOP-214"])
            .assert()
    }
}

fn stdout_of(assert: &assert_cmd::assert::Assert) -> String {
    String::from_utf8(assert.get_output().stdout.clone()).unwrap()
}

// --- what it prints -------------------------------------------------------------------

#[test]
fn it_prints_the_plan_the_binding_context_and_the_branch_diff() {
    let repo = Repo::new(&approved_plan());
    repo.write(
        "BUSINESS_CONTEXT.md",
        "<!-- invar-context: v1 -->\nEvery query is scoped to the current team.\n",
    );
    repo.commit("context");
    repo.implement();

    let out = stdout_of(&repo.review_input().success().stderr(""));

    let order = [
        "=== invar review input: SHOP-214 ===",
        "=== Plan: Intent ===",
        "=== Plan: Scope ===",
        "=== Plan: Acceptance criteria ===",
        "Given a pending, unpaid order of mine",
        "=== Plan: Implementation context ===",
        "Every query is scoped to the current team.",
        "=== diff against main",
        "diff --git a/app/Actions/Orders/CancelOrder.php b/app/Actions/Orders/CancelOrder.php",
        "+        $order->cancel();",
        "=== end of diff ===",
    ];
    let positions: Vec<usize> = order
        .iter()
        .map(|needle| {
            out.find(needle)
                .unwrap_or_else(|| panic!("no {needle:?} in:\n{out}"))
        })
        .collect();
    assert!(positions.windows(2).all(|w| w[0] < w[1]), "{out}");
    // The ticket text and the planner's reasoning are not review input.
    assert!(!out.contains("As a customer I want to cancel"), "{out}");
    assert!(!out.contains("Rejected alternatives"), "{out}");
}

#[test]
fn the_same_plan_and_commits_print_the_same_bytes() {
    let repo = Repo::new(&approved_plan());
    repo.implement();

    let a = repo.review_input().success().get_output().stdout.clone();
    let b = repo.review_input().success().get_output().stdout.clone();

    assert_eq!(a, b);
}

#[test]
fn the_developers_git_configuration_does_not_change_the_bytes() {
    let repo = Repo::new(&approved_plan());
    repo.implement();
    let plain = repo.review_input().success().get_output().stdout.clone();

    // Every setting the review diff pins, set the other way in the repository.
    fs::write(repo.root().join("order.txt"), "zzz\n").unwrap();
    for (key, value) in [
        ("color.ui", "always"),
        ("color.diff", "always"),
        ("diff.noprefix", "true"),
        ("diff.mnemonicPrefix", "true"),
        ("diff.relative", "true"),
        ("diff.renames", "false"),
        ("diff.algorithm", "histogram"),
        ("diff.context", "10"),
        ("diff.orderFile", "order.txt"),
        ("core.quotepath", "true"),
    ] {
        repo.git(&["config", key, value]);
    }
    let configured = repo.review_input().success().get_output().stdout.clone();

    assert_eq!(
        String::from_utf8(plain).unwrap(),
        String::from_utf8(configured).unwrap()
    );
}

#[test]
fn the_decision_log_never_reaches_the_reviewer() {
    let repo = Repo::new(&approved_plan());
    repo.write(
        ".invar/decisions/2026-10-06-163712-orders-earlier.md",
        "---\nid: d-1234abcd\ndate: 2026-10-06T18:37:12+02:00\nticket: SHOP-1\nmodule: orders\n\
         type: note\n---\n\nLOG-ENTRY-TEXT\n",
    );
    repo.git(&["add", "--force", ".invar/decisions"]);
    repo.implement();

    let out = stdout_of(&repo.review_input().success());

    assert!(!out.contains("LOG-ENTRY-TEXT"), "{out}");
    assert!(!out.contains("diff --git a/.invar"), "{out}");
}

#[test]
fn uncommitted_changes_are_named_on_stderr_and_left_out() {
    let repo = Repo::new(&approved_plan());
    repo.implement();
    let committed = repo.review_input().success().get_output().stdout.clone();

    repo.write("app/Uncommitted.php", "<?php // NOT-COMMITTED\n");

    let assert = repo
        .review_input()
        .success()
        .stderr(contains("1 uncommitted change is not in the review input"));
    assert_eq!(assert.get_output().stdout, committed);
}

#[test]
fn a_branch_with_nothing_committed_says_so() {
    let repo = Repo::new(&approved_plan());

    repo.review_input().success().stdout(contains(
        "Nothing is committed on this branch since it left `main`",
    ));
}

#[test]
fn a_diff_over_the_budget_lists_the_files_instead() {
    let repo = Repo::new(&approved_plan());
    let big: String = (0..2_000)
        .map(|n| format!("line {n:05} of a large file\n"))
        .collect();
    repo.write("database/seeders/BigSeeder.php", &big);
    repo.implement();

    let out = stdout_of(&repo.review_input().success());

    assert!(out.contains("over the budget of 40000"), "{out}");
    assert!(
        out.contains("\n+2000 -0  database/seeders/BigSeeder.php\n"),
        "{out}"
    );
    assert!(
        out.contains("\n+4 -0  app/Actions/Orders/CancelOrder.php\n"),
        "{out}"
    );
    assert!(!out.contains("line 01999"), "the patch was included");
    assert!(!out.contains("diff --git"), "{out}");
}

#[test]
fn the_base_branch_comes_from_the_config() {
    let repo = Repo::new(&approved_plan());
    repo.write("invar.yaml", "git:\n  base_branch: develop\n");

    repo.review_input()
        .failure()
        .code(1)
        .stdout("")
        .stderr(contains(
            "the base branch `develop` is not a branch or commit",
        ))
        .stderr(contains("`git.base_branch`"));
}

#[test]
fn only_what_the_branch_added_since_it_left_the_base_is_compared() {
    // Work that landed on `main` after the branch was cut is not the branch's.
    let repo = Repo::new(&approved_plan());
    repo.implement();
    repo.git(&["checkout", "--quiet", "main"]);
    repo.write("app/OnMainLater.php", "<?php // MAIN-ONLY\n");
    repo.commit("later on main");
    repo.git(&["checkout", "--quiet", "SHOP-214-cancel-unpaid-order"]);

    let out = stdout_of(&repo.review_input().success());

    assert!(!out.contains("MAIN-ONLY"), "{out}");
    assert!(out.contains("$order->cancel();"), "{out}");
}

// --- only an approved, unchanged Plan is reviewed against ------------------------------------

#[test]
fn a_draft_is_refused() {
    let repo = Repo::new(&draft_plan());
    repo.implement();

    repo.review_input()
        .failure()
        .code(1)
        .stdout("")
        .stderr(contains(WHY))
        .stderr(contains("is still a draft"));
}

#[test]
fn a_plan_changed_after_approval_is_refused() {
    let repo = Repo::new(&approved_plan().replace("refused with 403", "refused with 409"));
    repo.implement();

    repo.review_input()
        .failure()
        .code(1)
        .stdout("")
        .stderr(contains(WHY))
        .stderr(contains("changed after it was approved"));
}

// --- advisory: nothing depends on it ------------------------------------------------------

#[test]
fn done_does_not_depend_on_the_review() {
    // L-7: the review never blocks `done`. The plainest way that stays true is
    // that `done` does not call it, and the command module is not reachable
    // from it.
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    for file in ["done/mod.rs", "cli/done.rs"] {
        let text = fs::read_to_string(src.join(file)).unwrap();
        assert!(
            !text.contains("review_input") && !text.contains("review-input"),
            "{file} uses the review"
        );
    }
}

#[test]
fn done_closes_a_cycle_that_was_never_reviewed() {
    let repo = Repo::new(&approved_plan());
    repo.implement();

    Command::cargo_bin("invar")
        .unwrap()
        .current_dir(repo.root())
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_COMMITTER_NAME", "Test")
        .args(["done", "SHOP-214"])
        .assert()
        .success();
}

#[test]
fn the_help_says_it_is_advisory_and_never_shows_the_log() {
    Command::cargo_bin("invar")
        .unwrap()
        .args(["review-input", "--help"])
        .assert()
        .success()
        // Single words: clap may wrap the help between any two.
        .stdout(contains("Advisory"))
        .stdout(contains("draft"))
        .stdout(contains("40,000"));
}

// --- `## Acceptance tests to change` (plan-header.md §6.5) -----------------------

/// Example 9.1 with `content` as its `## Acceptance tests to change`, under
/// example 9.2's header with the checksum of this body: approved, and current.
fn approved_with_tests_to_change(content: &str) -> String {
    let context = "## Implementation context\n";
    let draft = draft_plan().replacen(
        context,
        &format!("## Acceptance tests to change\n{content}\n{context}"),
        1,
    );
    let body = draft.split_once("\n---\n").unwrap().1;

    let scratch = tempfile::tempdir().unwrap();
    let file = scratch.path().join("body.md");
    fs::write(&file, body.trim_start_matches('\n')).unwrap();
    let output = Command::cargo_bin("invar")
        .unwrap()
        .arg("hash")
        .arg(&file)
        .output()
        .unwrap();
    let checksum = String::from_utf8(output.stdout).unwrap();

    let header = contract_example("9.12")
        .lines()
        .map(|line| match line.strip_prefix("Approved-checksum:") {
            Some(_) => format!("Approved-checksum: {}\n", checksum.trim()),
            None => format!("{line}\n"),
        })
        .collect::<String>();
    header + body
}

const TESTS_TO_CHANGE: &str = "- tests/Acceptance/Shop190PaidTest.php — change — its 403 no longer holds within 24 hours\n\
     - tests/Acceptance/Shop177RefundTest.php — delete — refunds moved to SHOP-301\n";

/// Compare `actual` with `tests/snapshots/<name>`, as `tests/pr_body.rs` does:
/// `INVAR_BLESS_SNAPSHOTS=1` rewrites it, and the diff is what gets read.
fn assert_snapshot(name: &str, actual: &str) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/snapshots")
        .join(name);
    if std::env::var_os("INVAR_BLESS_SNAPSHOTS").is_some() {
        fs::write(&path, actual).unwrap();
        return;
    }
    let expected = fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("cannot read the snapshot {} ({err})", path.display()))
        .replace("\r\n", "\n");
    assert_eq!(
        actual,
        expected,
        "the output differs from {}",
        path.display()
    );
}

#[test]
fn a_plan_with_tests_to_change_prints_exactly_the_stored_input() {
    // Nothing committed yet, so the snapshot holds no commit or blob hash that
    // this machine's git configuration could change.
    let repo = Repo::new(&approved_with_tests_to_change(TESTS_TO_CHANGE));
    let out = stdout_of(&repo.review_input().success());
    assert_snapshot("review-input-shop-214-tests-to-change.txt", &out);
}

#[test]
fn a_section_that_says_none_prints_as_a_plan_without_it() {
    let without = stdout_of(&Repo::new(&approved_plan()).review_input().success());
    assert!(!without.contains("Acceptance tests to change"), "{without}");

    let none = Repo::new(&approved_with_tests_to_change("_none_\n"));
    assert_eq!(stdout_of(&none.review_input().success()), without);
}
