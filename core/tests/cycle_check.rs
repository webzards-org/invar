//! End-to-end tests for `invar cycle check` and `invar cycle required` (#155,
//! `invar-yaml.md` §3.8): the real binary, in real git repositories, with no network.

use std::fs;
use std::path::PathBuf;

use assert_cmd::Command;
use assert_cmd::assert::Assert;
use predicates::str::contains;
use tempfile::TempDir;

const SPEC: &str = ".invar/plans/SHOP-214-cancel-unpaid-order/spec.md";

const REQUIRED: &str = "cycle:\n  required_paths: [\"app/Models/**\", \"database/migrations/\"]\n";

const BODY: &str = "## Ticket\nSHOP-214 — Cancel an unpaid order.\n\n\
    ## Intent\nA customer can cancel an order they have not paid for.\n\n\
    ## Scope\n`Orders` only.\n\n\
    ## Open questions / answers\nNone.\n\n\
    ## Rejected alternatives\nNone.\n\n\
    ## Acceptance criteria\n\n\
    - AC1: Given an unpaid order, when its owner cancels it, then it is cancelled.\n\n\
    ## Implementation context\n- AI_CONTEXT: controllers stay thin\n";

struct Project {
    dir: TempDir,
}

impl Project {
    /// `main` holding `config` as invar.yaml, a model and a route, then a branch
    /// `feat/SHOP-214-cancel` checked out from it.
    fn new(config: &str) -> Self {
        let project = Self {
            dir: tempfile::tempdir().unwrap(),
        };
        project.write(
            ".gitconfig-test",
            "[user]\n\tname = Test\n\temail = test@example.com\n[commit]\n\tgpgsign = false\n",
        );
        project.git(&["init", "-q", "-b", "main"]);
        project.write(".gitignore", ".gitconfig-test\n");
        project.write("invar.yaml", config);
        project.write("app/Models/Order.php", "<?php\nclass Order {}\n");
        project.write("routes/web.php", "<?php\n");
        project.commit("the base");
        project.git(&["checkout", "-q", "-b", "feat/SHOP-214-cancel"]);
        project
    }

    fn root(&self) -> PathBuf {
        self.dir.path().canonicalize().unwrap()
    }

    fn write(&self, path: &str, text: &str) {
        let path = self.root().join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
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
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    }

    fn commit(&self, message: &str) {
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "--allow-empty", "-m", message]);
    }

    /// A commit with a trailer, as the developer writes it.
    fn commit_with_trailer(&self, message: &str, trailer: &str) {
        self.git(&["add", "-A"]);
        self.git(&[
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            message,
            "--trailer",
            trailer,
        ]);
    }

    /// A change to a required path.
    fn change_a_model(&self) {
        self.write(
            "app/Models/Order.php",
            "<?php\nclass Order { public $paid; }\n",
        );
        self.commit("change the model");
    }

    fn invar(&self, args: &[&str]) -> Assert {
        Command::cargo_bin("invar")
            .unwrap()
            .current_dir(self.root())
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", self.root().join(".gitconfig-test"))
            .env("HOME", self.root())
            .args(args)
            .assert()
    }

    fn check(&self) -> Assert {
        self.invar(&["cycle", "check", "--base", "main"])
    }

    /// The Plan, approved as `plan approve` records it.
    fn approved_plan(&self) -> String {
        let file = self.dir.path().join("to-hash.txt");
        fs::write(&file, format!("Acceptance: skipped\n{BODY}")).unwrap();
        let output = Command::cargo_bin("invar")
            .unwrap()
            .arg("hash")
            .arg(&file)
            .output()
            .unwrap();
        fs::remove_file(&file).unwrap();
        let checksum = String::from_utf8(output.stdout).unwrap();
        format!(
            "---\nTicket: SHOP-214\nModule: orders\nStatus: approved\nApproved-by: rade\n\
             Approved-at: 2026-10-05\nApproved-checksum: {}\nAcceptance: skipped\n---\n\n{BODY}",
            checksum.trim()
        )
    }

    fn draft_plan() -> String {
        format!(
            "---\nTicket: SHOP-214\nModule: orders\nStatus: draft\nApproved-by:\nApproved-at:\n\
             Approved-checksum:\n---\n\n{BODY}"
        )
    }

    /// A cycle: the Plan approved in one commit, deleted by `done` in a later one.
    fn closed_cycle(&self) {
        self.write(SPEC, &Self::draft_plan());
        self.commit("plan SHOP-214");
        self.write(SPEC, &self.approved_plan());
        self.commit("approve SHOP-214");
        self.change_a_model();
        self.git(&[
            "rm",
            "-q",
            "-r",
            ".invar/plans/SHOP-214-cancel-unpaid-order",
        ]);
        self.commit("Remove plan SHOP-214");
    }

    fn head(&self) -> String {
        self.git(&["rev-parse", "HEAD"]).trim().to_owned()
    }
}

fn stdout_of(assert: &Assert) -> String {
    String::from_utf8(assert.get_output().stdout.clone()).unwrap()
}

fn stderr_of(assert: &Assert) -> String {
    String::from_utf8(assert.get_output().stderr.clone()).unwrap()
}

/// Failed: non-zero, the report ending `cycle check: failed`, and every part on stderr.
fn failed(assert: Assert, parts: &[&str]) -> (String, String) {
    let assert = assert.failure();
    let (out, err) = (stdout_of(&assert), stderr_of(&assert));
    assert!(out.ends_with("cycle check: failed\n"), "{out}");
    for part in parts {
        assert!(err.contains(part), "{part}:\n{err}");
    }
    (out, err)
}

// --- off --------------------------------------------------------------------------------

#[test]
fn with_no_key_at_the_base_the_check_is_off() {
    let project = Project::new("scenarios: false\n");
    project.change_a_model();
    project.check().success().stdout(contains(
        "cycle check: off: main:invar.yaml has no `cycle.required_paths`",
    ));
}

#[test]
fn with_no_invar_yaml_at_the_base_the_check_is_off() {
    let project = Project::new("scenarios: false\n");
    project.git(&["checkout", "-q", "main"]);
    project.git(&["rm", "-q", "invar.yaml"]);
    project.commit("no invar.yaml on main");
    project.git(&["checkout", "-q", "feat/SHOP-214-cancel"]);
    project
        .check()
        .success()
        .stdout(contains("cycle check: off: main has no invar.yaml"));
}

#[test]
fn a_key_added_only_on_the_branch_leaves_the_check_off_with_a_note() {
    let project = Project::new("scenarios: false\n");
    project.write("invar.yaml", REQUIRED);
    project.change_a_model();
    let assert = project.check().success();
    let out = stdout_of(&assert);
    assert!(
        out.contains("note: this branch changes `cycle.required_paths`"),
        "{out}"
    );
    assert!(out.contains("added on the branch: app/Models/**"), "{out}");
    assert!(out.ends_with("cycle check: off: main:invar.yaml has no `cycle.required_paths`, so no path needs a cycle\n"), "{out}");
}

// --- passing ----------------------------------------------------------------------------

#[test]
fn a_branch_touching_no_required_path_passes() {
    let project = Project::new(REQUIRED);
    project.write("routes/web.php", "<?php\n// a route\n");
    project.commit("a route");
    let assert = project.check().success();
    let out = stdout_of(&assert);
    assert!(out.contains("required changes:\n  none\n"), "{out}");
    assert!(
        out.ends_with("cycle check: passed: no required path changed\n"),
        "{out}"
    );
}

#[test]
fn a_closed_cycle_passes_and_is_named_with_the_commit_that_deleted_its_plan() {
    let project = Project::new(REQUIRED);
    project.closed_cycle();
    let deleted_in: String = project.head().chars().take(12).collect();
    let assert = project.check().success();
    let out = stdout_of(&assert);
    assert!(
        out.contains("  app/Models/Order.php (app/Models/**)\n"),
        "{out}"
    );
    assert!(
        out.contains(&format!("  SHOP-214: {SPEC} deleted in {deleted_in}\n")),
        "{out}"
    );
    assert!(
        out.ends_with("cycle check: passed: closed cycle(s): SHOP-214\n"),
        "{out}"
    );
}

#[test]
fn an_invar_skip_trailer_passes_and_its_reason_is_printed() {
    let project = Project::new(REQUIRED);
    project.write(
        "app/Models/Order.php",
        "<?php\nclass Order {} // typo fixed\n",
    );
    project.commit_with_trailer("fix a typo", "Invar-Skip: a comment typo, no behavior");
    let commit: String = project.head().chars().take(12).collect();
    let assert = project.check().success();
    let out = stdout_of(&assert);
    assert!(
        out.contains(&format!(
            "  {commit}: Invar-Skip: a comment typo, no behavior\n"
        )),
        "{out}"
    );
    assert!(
        out.ends_with("cycle check: passed: skipped by `Invar-Skip:` (1 trailer(s))\n"),
        "{out}"
    );
}

#[test]
fn one_trailer_anywhere_on_the_branch_covers_it() {
    let project = Project::new(REQUIRED);
    project.change_a_model();
    project.commit_with_trailer("say why", "Invar-Skip: a rename the team agreed on");
    project.check().success();
}

// --- failing ----------------------------------------------------------------------------

#[test]
fn a_required_change_with_no_cycle_and_no_trailer_fails_naming_path_and_glob() {
    let project = Project::new(REQUIRED);
    project.change_a_model();
    project.write("database/migrations/2026_create_orders.php", "<?php\n");
    project.write("routes/web.php", "<?php\n// not required\n");
    project.commit("a migration");
    let (out, err) = failed(
        project.check(),
        &[
            "changes 2 required path(s), with no cycle closed on it and no `Invar-Skip:` trailer",
            "  app/Models/Order.php (matches app/Models/**)",
            "  database/migrations/2026_create_orders.php (matches database/migrations/)",
            "/invar:done",
            "git commit --amend --trailer \"Invar-Skip: <reason>\"",
        ],
    );
    assert!(!err.contains("routes/web.php"), "{err}");
    assert!(out.contains("closed cycles:\n  none\n"), "{out}");
}

#[test]
fn an_empty_reason_does_not_count() {
    let project = Project::new(REQUIRED);
    project.change_a_model();
    project.commit_with_trailer("no reason", "Invar-Skip:");
    let (out, _) = failed(project.check(), &["app/Models/Order.php"]);
    assert!(out.contains("skips:\n  none\n"), "{out}");
}

#[test]
fn a_plan_still_there_is_an_open_cycle_and_does_not_count() {
    let project = Project::new(REQUIRED);
    project.write(SPEC, &project.approved_plan());
    project.commit("approve SHOP-214");
    project.change_a_model();
    let (out, _) = failed(project.check(), &["app/Models/Order.php"]);
    assert!(
        out.contains(&format!(
            "open cycle: the cycle of SHOP-214 is not closed, {SPEC} is still there: run \
             /invar:done SHOP-214"
        )),
        "{out}"
    );
}

#[test]
fn a_plan_deleted_without_being_approved_is_not_a_closed_cycle() {
    let project = Project::new(REQUIRED);
    project.write(SPEC, &Project::draft_plan());
    project.commit("plan SHOP-214");
    project.change_a_model();
    project.git(&["rm", "-q", "-r", ".invar/plans"]);
    project.commit("drop the draft");
    failed(project.check(), &["app/Models/Order.php"]);
}

#[test]
fn a_plan_that_only_says_approved_is_not_a_closed_cycle() {
    // `Status: approved` written by hand, with no approval values: no `plan approve` ran.
    let project = Project::new(REQUIRED);
    project.write(
        SPEC,
        &Project::draft_plan().replace("Status: draft", "Status: approved"),
    );
    project.commit("approved by hand");
    project.change_a_model();
    project.git(&["rm", "-q", "-r", ".invar/plans"]);
    project.commit("Remove plan SHOP-214");
    failed(project.check(), &["app/Models/Order.php"]);
}

#[test]
fn renames_into_and_out_of_a_required_path_and_a_deletion_are_required_changes() {
    for (from, to, says) in [
        (
            "routes/web.php",
            "app/Models/Route.php",
            "app/Models/Route.php (matches app/Models/**)",
        ),
        (
            "app/Models/Order.php",
            "app/Order.php",
            "app/Models/Order.php (matches app/Models/**)",
        ),
    ] {
        let project = Project::new(REQUIRED);
        project.git(&["mv", from, to]);
        project.commit("move a file");
        failed(project.check(), &[says]);
    }
    let project = Project::new(REQUIRED);
    project.git(&["rm", "-q", "app/Models/Order.php"]);
    project.commit("delete a model");
    failed(
        project.check(),
        &["app/Models/Order.php (matches app/Models/**)"],
    );
}

#[test]
fn a_branch_that_removes_a_pattern_is_checked_against_the_bases_list() {
    let project = Project::new(REQUIRED);
    project.write(
        "invar.yaml",
        "cycle:\n  required_paths: [\"database/migrations/\"]\n",
    );
    project.change_a_model();
    let (out, _) = failed(
        project.check(),
        &["app/Models/Order.php (matches app/Models/**)"],
    );
    assert!(
        out.contains("removed on the branch: app/Models/**"),
        "{out}"
    );
    // Removing the whole section is the same.
    project.write("invar.yaml", "scenarios: false\n");
    project.commit("no list");
    failed(project.check(), &["app/Models/Order.php"]);
}

#[test]
fn changes_under_invar_are_not_required_changes() {
    let project = Project::new("cycle:\n  required_paths: [\"**\"]\n");
    project.write(".invar/decisions/2026-11-24-orders-SHOP-214.md", "x\n");
    project.commit("a decision entry");
    project
        .check()
        .success()
        .stdout(contains("cycle check: passed: no required path changed"));
}

#[test]
fn a_shallow_clone_missing_the_merge_base_says_to_fetch_the_history() {
    let project = Project::new(REQUIRED);
    project.change_a_model();
    project.write(
        "app/Models/Order.php",
        "<?php\nclass Order { public $cancelled; }\n",
    );
    project.commit("again");
    project.git(&["checkout", "-q", "main"]);
    project.git(&["commit", "-q", "--allow-empty", "-m", "later on main"]);
    project.git(&["checkout", "-q", "feat/SHOP-214-cancel"]);

    let clone = tempfile::tempdir().unwrap();
    let url = format!("file://{}", project.root().display());
    let git = |args: &[&str]| {
        let output = std::process::Command::new("git")
            .args(args)
            .current_dir(clone.path())
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", project.root().join(".gitconfig-test"))
            .env("HOME", project.root())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    };
    git(&[
        "clone",
        "-q",
        "--depth",
        "1",
        "--branch",
        "feat/SHOP-214-cancel",
        &url,
        ".",
    ]);
    git(&[
        "fetch",
        "-q",
        "--depth",
        "1",
        "origin",
        "+refs/heads/main:refs/remotes/origin/main",
    ]);
    let assert = Command::cargo_bin("invar")
        .unwrap()
        .current_dir(clone.path())
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("HOME", project.root())
        .args(["cycle", "check", "--base", "origin/main"])
        .assert()
        .failure()
        .stdout("");
    let err = stderr_of(&assert);
    for part in ["shallow clone", "`origin/main`", "fetch-depth: 0"] {
        assert!(err.contains(part), "{part}:\n{err}");
    }
}

#[test]
fn the_trailer_survives_rebase_signoff() {
    let project = Project::new(REQUIRED);
    project.write(
        "app/Models/Order.php",
        "<?php\nclass Order {} // typo fixed\n",
    );
    project.commit_with_trailer("fix a typo", "Invar-Skip: a comment typo, no behavior");
    // Something new on main, so that the rebase rewrites the branch's commit.
    project.git(&["checkout", "-q", "main"]);
    project.write("README.md", "later\n");
    project.commit("later on main");
    project.git(&["checkout", "-q", "feat/SHOP-214-cancel"]);
    let before = project.head();
    project.git(&["rebase", "-q", "--signoff", "main"]);
    assert_ne!(project.head(), before, "the rebase rewrote nothing");
    let message = project.git(&["log", "-1", "--format=%B"]);
    assert!(
        message.contains("Signed-off-by: Test <test@example.com>"),
        "{message}"
    );

    let assert = project.check().success();
    assert!(
        stdout_of(&assert).contains("Invar-Skip: a comment typo, no behavior"),
        "{}",
        stdout_of(&assert)
    );
}

#[test]
fn without_base_it_uses_git_base_branch() {
    let project = Project::new(&format!("git:\n  base_branch: main\n{REQUIRED}"));
    project.closed_cycle();
    project
        .invar(&["cycle", "check"])
        .success()
        .stdout(contains("base: main\n"));
}

// --- cycle required -----------------------------------------------------------------------

#[test]
fn required_names_each_path_a_pattern_matches_and_ends_with_one_line() {
    let project = Project::new(REQUIRED);
    project
        .invar(&[
            "cycle",
            "required",
            "app/Models/Order.php",
            "routes/web.php",
        ])
        .success()
        .stdout("app/Models/Order.php: required (app/Models/**)\ncycle: required\n");
    project
        .invar(&["cycle", "required", "routes/web.php"])
        .success()
        .stdout("cycle: not required\n");
}

#[test]
fn required_reads_the_working_tree_and_with_no_list_requires_nothing() {
    let project = Project::new("scenarios: false\n");
    project
        .invar(&["cycle", "required", "app/Models/Order.php"])
        .success()
        .stdout("cycle: not required (invar.yaml has no `cycle.required_paths`)\n");
    project.write("invar.yaml", REQUIRED);
    project
        .invar(&["cycle", "required", "./app/Models/Order.php"])
        .success()
        .stdout("app/Models/Order.php: required (app/Models/**)\ncycle: required\n");
}
