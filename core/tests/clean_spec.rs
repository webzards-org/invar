//! End-to-end tests for `invar clean-spec` (M1-13): the real binary, a real
//! project, and — for `--merged` — a real git repository.
//!
//! Each test reads as a sentence about the command: *a dry run deletes
//! nothing*, *a Plan with no decision entry is never deleted*, *nothing outside
//! `.invar/plans/` is touched*. Decision entries are written the way a person
//! writes them, with `invar decision record`, so the whole honest route —
//! refused, record an entry, clean — is what is tested.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as Process;

use assert_cmd::Command;
use predicates::prelude::*;
use predicates::str::contains;
use tempfile::TempDir;

/// 16:37:12 UTC.
const NOW: &str = "2026-10-06T18:37:12+02:00";
const CANCEL: &str = "SHOP-214-cancel-unpaid-order";

fn contract_example(number: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../contracts/formats/plan-header.md");
    let text = fs::read_to_string(path).unwrap();
    let after = text.split_once(&format!("### {number} ")).unwrap().1;
    let body = after.split_once("```markdown\n").unwrap().1;
    body.split_once("\n```\n").unwrap().0.to_owned() + "\n"
}

/// Example 9.1: a draft Plan for SHOP-214.
fn draft() -> String {
    contract_example("9.1")
}

/// Example 9.1's body under example 9.2's header: the approved Plan.
fn approved() -> String {
    let body = draft().split_once("\n---\n").unwrap().1.to_owned();
    contract_example("9.2") + &body
}

fn for_ticket(text: &str, ticket: &str) -> String {
    text.replace("SHOP-214", ticket)
}

struct Project {
    dir: TempDir,
}

impl Project {
    fn new(config: &str) -> Self {
        let project = Self {
            dir: tempfile::tempdir().unwrap(),
        };
        fs::write(project.root().join("invar.yaml"), config).unwrap();
        fs::write(project.root().join(".gitconfig-test"), "").unwrap();
        project
    }

    fn root(&self) -> PathBuf {
        self.dir.path().canonicalize().unwrap()
    }

    /// A Plan directory `name` with `text` as its `spec.md`.
    fn plan(&self, name: &str, text: &str) -> PathBuf {
        let dir = self.root().join(".invar/plans").join(name);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("spec.md"), text).unwrap();
        dir
    }

    /// The Plan directories that exist, sorted.
    fn plans(&self) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(self.root().join(".invar/plans"))
            .map(|entries| {
                entries
                    .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }

    fn invar(&self, args: &[&str]) -> Command {
        let mut command = Command::cargo_bin("invar").unwrap();
        command
            .current_dir(self.root())
            .env("GIT_COMMITTER_DATE", NOW)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", self.root().join(".gitconfig-test"))
            .env("HOME", self.root())
            .args(args);
        command
    }

    fn clean(&self, args: &[&str]) -> assert_cmd::assert::Assert {
        let mut all = vec!["clean-spec"];
        all.extend_from_slice(args);
        self.invar(&all).assert()
    }

    /// A `note` entry for `ticket`, through the real command: the honest way
    /// to close an abandoned cycle.
    fn record(&self, ticket: &str) {
        self.invar(&[
            "decision",
            "record",
            "--ticket",
            ticket,
            "--type",
            "note",
            "--module",
            "orders",
            "-m",
            "The cycle was dropped.",
        ])
        .assert()
        .success();
    }

    /// Run git in the project, apart from the developer's own configuration.
    fn git(&self, args: &[&str]) {
        let output = Process::new("git")
            .args([
                "-c",
                "user.name=test",
                "-c",
                "user.email=test@example.com",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .current_dir(self.root())
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", self.root().join(".gitconfig-test"))
            .env("HOME", self.root())
            .output()
            .expect("git runs");
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn commit(&self, message: &str) {
        self.git(&["commit", "--quiet", "--allow-empty", "-m", message]);
    }

    /// A repository on `main` with one commit.
    fn init_git(&self) {
        self.git(&["init", "--quiet", "-b", "main"]);
        self.commit("initial");
    }
}

/// Every file and directory under `root`, with each file's bytes; `.git` is
/// left out, since git's own bookkeeping is not the command's to account for.
fn snapshot(root: &Path) -> BTreeMap<String, Option<Vec<u8>>> {
    fn walk(dir: &Path, root: &Path, into: &mut BTreeMap<String, Option<Vec<u8>>>) {
        for entry in fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            let relative = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            if relative == ".git" {
                continue;
            }
            if entry.file_type().unwrap().is_dir() {
                into.insert(format!("{relative}/"), None);
                walk(&path, root, into);
            } else {
                into.insert(relative, Some(fs::read(&path).unwrap()));
            }
        }
    }
    let mut into = BTreeMap::new();
    walk(root, root, &mut into);
    into
}

// --- a dry run ------------------------------------------------------------------

#[test]
fn a_dry_run_lists_what_would_be_deleted_and_why_and_deletes_nothing() {
    let project = Project::new("");
    project.plan(CANCEL, &draft());
    project.record("SHOP-214");
    let before = snapshot(&project.root());

    project
        .clean(&[])
        .success()
        .stdout(contains(format!(
            "would delete: .invar/plans/{CANCEL} (draft) — decision entry d-"
        )))
        .stdout(contains("(note, 2026-10-06)"))
        .stdout(contains(
            "dry run: nothing was deleted. Run again with `--force` to delete the 1 Plan",
        ));

    assert_eq!(
        snapshot(&project.root()),
        before,
        "a dry run changed the tree"
    );
}

#[test]
fn saying_dry_run_is_the_same_as_saying_nothing() {
    let project = Project::new("");
    project.plan(CANCEL, &draft());
    project.record("SHOP-214");

    project
        .clean(&["--dry-run"])
        .success()
        .stdout(contains("would delete: .invar/plans/"));

    assert_eq!(project.plans(), [CANCEL]);
}

#[test]
fn dry_run_and_force_contradict_each_other() {
    let project = Project::new("");
    project.plan(CANCEL, &draft());
    project.record("SHOP-214");

    project
        .clean(&["--dry-run", "--force"])
        .failure()
        .code(1)
        .stderr(contains("cannot be used with"));

    assert_eq!(project.plans(), [CANCEL]);
}

// --- --force --------------------------------------------------------------------

#[test]
fn force_deletes_only_the_eligible_plans() {
    let project = Project::new("");
    project.plan(CANCEL, &draft());
    project.plan("SHOP-300-no-entry", &for_ticket(&draft(), "SHOP-300"));
    project.plan("SHOP-400-approved", &for_ticket(&approved(), "SHOP-400"));
    project.record("SHOP-214");
    project.record("SHOP-400");

    project
        .clean(&["--force"])
        .success()
        .stdout(contains(format!("deleted: .invar/plans/{CANCEL} (draft)")))
        .stdout(contains(
            "kept: .invar/plans/SHOP-300-no-entry (draft) — no decision entry for SHOP-300",
        ))
        .stdout(contains(
            "kept: .invar/plans/SHOP-400-approved (approved) — approved, so its cycle may \
             still be running",
        ))
        .stdout(contains("deleted 1 Plan"));

    assert_eq!(project.plans(), ["SHOP-300-no-entry", "SHOP-400-approved"]);
}

#[test]
fn what_is_in_a_plan_directory_goes_with_it() {
    let project = Project::new("");
    let dir = project.plan(CANCEL, &draft());
    fs::write(dir.join("notes.md"), "beside the Plan").unwrap();
    project.record("SHOP-214");

    project.clean(&["--force"]).success();

    assert!(!dir.exists());
}

// --- no decision entry ------------------------------------------------------------

#[test]
fn a_plan_without_a_decision_entry_is_reported_and_never_deleted() {
    let project = Project::new("");
    project.plan("SHOP-300-no-entry", &for_ticket(&draft(), "SHOP-300"));

    // Not even with `--force`: it is a signal, not garbage.
    project
        .clean(&["--force"])
        .success()
        .stdout(contains(
            "kept: .invar/plans/SHOP-300-no-entry (draft) — no decision entry for SHOP-300",
        ))
        .stdout(contains(
            "`invar decision record --ticket SHOP-300 --type failed`",
        ))
        .stdout(contains("nothing to delete"));

    assert_eq!(project.plans(), ["SHOP-300-no-entry"]);
}

#[test]
fn naming_a_plan_that_has_no_entry_is_an_error_not_a_quiet_success() {
    let project = Project::new("");
    project.plan("SHOP-300-no-entry", &for_ticket(&draft(), "SHOP-300"));

    project
        .clean(&["SHOP-300", "--force"])
        .failure()
        .code(1)
        .stdout(contains("kept: .invar/plans/SHOP-300-no-entry"))
        .stderr(contains(
            "nothing was deleted: SHOP-300-no-entry is kept because it has no decision entry",
        ));

    assert_eq!(project.plans(), ["SHOP-300-no-entry"]);
}

#[test]
fn recording_an_entry_first_is_the_honest_way_to_close_an_abandoned_cycle() {
    let project = Project::new("");
    project.plan("SHOP-300-no-entry", &for_ticket(&draft(), "SHOP-300"));
    project.clean(&["SHOP-300", "--force"]).failure();
    assert_eq!(project.plans(), ["SHOP-300-no-entry"]);

    project.record("SHOP-300");
    project.clean(&["SHOP-300", "--force"]).success();

    assert!(project.plans().is_empty());
    // The decision log is what survives the Plan, and the command left it be.
    let entries = fs::read_dir(project.root().join(".invar/decisions"))
        .unwrap()
        .count();
    assert_eq!(entries, 1);
}

#[test]
fn an_entry_that_cannot_be_read_is_reported_and_proves_nothing() {
    let project = Project::new("");
    project.plan(CANCEL, &draft());
    let decisions = project.root().join(".invar/decisions");
    fs::create_dir_all(&decisions).unwrap();
    fs::write(
        decisions.join("2026-10-06-163712-orders-shop-214.md"),
        "---\nticket: SHOP-214\n---\n",
    )
    .unwrap();

    project
        .clean(&["--force"])
        .success()
        .stdout(contains("kept: .invar/plans/").and(contains("no decision entry for SHOP-214")))
        .stderr(contains("1 decision entry could not be read"));

    assert_eq!(project.plans(), [CANCEL]);
}

// --- an approved plan ---------------------------------------------------------------

#[test]
fn an_approved_plan_is_kept_in_a_bare_run_and_deleted_once_it_is_named() {
    let project = Project::new("");
    project.plan(CANCEL, &approved());
    project.record("SHOP-214");

    project
        .clean(&["--force"])
        .success()
        .stdout(contains("approved, so its cycle may still be running"))
        .stdout(contains("`invar clean-spec SHOP-214 --force`"));
    assert_eq!(project.plans(), [CANCEL]);

    project.clean(&["SHOP-214", "--force"]).success();
    assert!(project.plans().is_empty());
}

// --- scope and lookup ------------------------------------------------------------------

#[test]
fn a_ticket_limits_the_run_to_that_plan() {
    let project = Project::new("");
    project.plan(CANCEL, &draft());
    project.plan("SHOP-300-other", &for_ticket(&draft(), "SHOP-300"));
    project.record("SHOP-214");
    project.record("SHOP-300");

    project
        .clean(&["SHOP-214", "--force"])
        .success()
        .stdout(contains("SHOP-300").not());

    assert_eq!(project.plans(), ["SHOP-300-other"]);
}

#[test]
fn a_ticket_that_names_no_plan_or_several_is_an_error() {
    let project = Project::new("");
    project.plan("SHOP-214-a", &draft());
    project.plan("SHOP-214-b", &draft());

    project
        .clean(&["SHOP-999"])
        .failure()
        .code(1)
        .stderr(contains("no Plan for `SHOP-999`"));
    project
        .clean(&["SHOP-214", "--force"])
        .failure()
        .code(1)
        .stderr(contains("matches 2 Plans"));

    assert_eq!(project.plans(), ["SHOP-214-a", "SHOP-214-b"]);
}

#[test]
fn without_a_plans_directory_there_is_nothing_to_do() {
    let project = Project::new("");

    project
        .clean(&["--force"])
        .success()
        .stdout("no Plans: .invar/plans does not exist\n");
}

// --- --merged --------------------------------------------------------------------------

#[test]
fn merged_skips_a_plan_when_no_merged_branch_names_its_ticket() {
    let project = Project::new("");
    project.init_git();
    project.git(&["branch", "feat/SHOP-300-other"]);
    project.plan(CANCEL, &draft());
    project.record("SHOP-214");

    project
        .clean(&["--merged", "--force"])
        .success()
        .stdout(contains("no branch merged into `main` names SHOP-214"))
        .stdout(contains("nothing to delete"));

    assert_eq!(project.plans(), [CANCEL]);
}

#[test]
fn merged_deletes_a_plan_when_exactly_one_merged_branch_names_its_ticket() {
    let project = Project::new("");
    project.init_git();
    project.git(&["branch", "feat/SHOP-214-cancel"]);
    project.plan(CANCEL, &draft());
    project.record("SHOP-214");

    project
        .clean(&["--merged", "--force"])
        .success()
        .stdout(contains(
            "branch `feat/SHOP-214-cancel` is merged into `main`",
        ))
        .stdout(contains("deleted 1 Plan"));

    assert!(project.plans().is_empty());
}

#[test]
fn a_branch_that_is_not_merged_is_not_the_tickets() {
    let project = Project::new("");
    project.init_git();
    project.git(&["checkout", "--quiet", "-b", "feat/SHOP-214-open"]);
    project.commit("work in progress");
    project.git(&["checkout", "--quiet", "main"]);
    project.plan(CANCEL, &draft());
    project.record("SHOP-214");

    project
        .clean(&["--merged", "--force"])
        .success()
        .stdout(contains("no branch merged into `main` names SHOP-214"));

    assert_eq!(project.plans(), [CANCEL]);
}

#[test]
fn merged_with_several_branches_for_one_ticket_fails_listing_them_and_deletes_nothing() {
    let project = Project::new("");
    project.init_git();
    project.git(&["branch", "feat/SHOP-214-a"]);
    project.git(&["branch", "fix/SHOP-214-b"]);
    project.git(&["branch", "feat/SHOP-300-only"]);
    project.plan(CANCEL, &draft());
    project.plan("SHOP-300-fine", &for_ticket(&draft(), "SHOP-300"));
    project.record("SHOP-214");
    project.record("SHOP-300");

    project
        .clean(&["--merged", "--force"])
        .failure()
        .code(1)
        .stderr(contains("nothing was deleted"))
        .stderr(contains(
            "SHOP-214 names 2 merged branches: feat/SHOP-214-a, fix/SHOP-214-b",
        ))
        .stderr(contains("SHOP-300").not());

    // Even the Plan with a single, clear branch stays: the run failed as a
    // whole.
    assert_eq!(project.plans(), [CANCEL, "SHOP-300-fine"]);
}

#[test]
fn the_base_branch_comes_from_the_config() {
    // The branch is merged into `develop`, not into `main`.
    let setup = |project: &Project| {
        project.init_git();
        project.git(&["checkout", "--quiet", "-b", "feat/SHOP-214-cancel"]);
        project.commit("the feature");
        project.git(&["branch", "develop"]);
        project.git(&["checkout", "--quiet", "main"]);
        project.plan(CANCEL, &draft());
        project.record("SHOP-214");
    };

    let configured = Project::new("git:\n  base_branch: develop\n");
    setup(&configured);
    configured
        .clean(&["--merged", "--force"])
        .success()
        .stdout(contains(
            "branch `feat/SHOP-214-cancel` is merged into `develop`",
        ));
    assert!(configured.plans().is_empty());

    let detected = Project::new("");
    setup(&detected);
    detected
        .clean(&["--merged", "--force"])
        .success()
        .stdout(contains("no branch merged into `main` names SHOP-214"));
    assert_eq!(detected.plans(), [CANCEL]);
}

#[test]
fn a_base_branch_that_does_not_exist_is_an_error_naming_the_setting() {
    let project = Project::new("git:\n  base_branch: no-such-branch\n");
    project.init_git();
    project.plan(CANCEL, &draft());
    project.record("SHOP-214");

    project
        .clean(&["--merged", "--force"])
        .failure()
        .code(1)
        .stderr(contains(
            "cannot list the branches merged into `no-such-branch`",
        ))
        .stderr(contains("git.base_branch"));

    assert_eq!(project.plans(), [CANCEL]);
}

#[test]
fn a_ticket_and_merged_together_need_both_to_hold() {
    let project = Project::new("");
    project.init_git();
    project.git(&["branch", "feat/SHOP-214-cancel"]);
    project.plan(CANCEL, &draft());
    project.plan("SHOP-300-other", &for_ticket(&draft(), "SHOP-300"));
    project.record("SHOP-214");
    project.record("SHOP-300");

    project
        .clean(&["SHOP-214", "--merged", "--force"])
        .success();

    // SHOP-300 has an entry, and no merged branch; it was not asked for.
    assert_eq!(project.plans(), ["SHOP-300-other"]);
}

// --- nothing outside .invar/plans/ ---------------------------------------------------------

#[test]
fn nothing_outside_the_plans_directory_is_touched() {
    let project = Project::new("");
    let root = project.root();
    for (path, text) in [
        ("app/Http/Controllers/OrderController.php", "<?php // code"),
        (
            "resources/js/Pages/Orders/Show.vue",
            "<template></template>",
        ),
        ("tests/Feature/CancelOrderTest.php", "<?php // a test"),
        ("README.md", "# the project"),
        (".invar/out/SHOP-1-pr.md", "# an earlier PR description"),
        (".invar/notes.md", "kept beside the plans"),
        ("AI_CONTEXT.md", "<!-- invar-context: v1 -->\n"),
    ] {
        let file = root.join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, text).unwrap();
    }
    let eligible = project.plan(CANCEL, &draft());
    fs::write(eligible.join("sketch.md"), "beside the Plan").unwrap();
    project.plan("SHOP-300-no-entry", &for_ticket(&draft(), "SHOP-300"));
    project.plan("SHOP-400-approved", &for_ticket(&approved(), "SHOP-400"));
    project.record("SHOP-214");
    project.record("SHOP-400");
    let before = snapshot(&root);

    project.clean(&["--force"]).success();
    let after = snapshot(&root);

    // Everything outside `.invar/plans/` is the same, byte for byte — the
    // decision log included.
    let outside = |snapshot: &BTreeMap<String, Option<Vec<u8>>>| {
        snapshot
            .iter()
            .filter(|(path, _)| !path.starts_with(".invar/plans/"))
            .map(|(path, content)| (path.clone(), content.clone()))
            .collect::<BTreeMap<_, _>>()
    };
    assert_eq!(outside(&before), outside(&after));
    assert!(
        after.contains_key(".invar/plans/"),
        "the Plans directory itself stays"
    );

    // Inside it, the eligible Plan's directory is all that went.
    let gone: Vec<&String> = before
        .keys()
        .filter(|path| !after.contains_key(*path))
        .collect();
    assert!(!gone.is_empty());
    assert!(
        gone.iter()
            .all(|path| path.starts_with(&format!(".invar/plans/{CANCEL}"))),
        "{gone:?}"
    );
    assert!(
        after
            .keys()
            .any(|path| path.starts_with(".invar/plans/SHOP-300-no-entry"))
    );
    assert!(
        after
            .keys()
            .any(|path| path.starts_with(".invar/plans/SHOP-400-approved"))
    );
}

#[cfg(unix)]
#[test]
fn a_plan_directory_that_is_a_symlink_is_unlinked_and_its_target_is_left_alone() {
    let project = Project::new("");
    let root = project.root();
    let elsewhere = root.join("elsewhere");
    fs::create_dir_all(&elsewhere).unwrap();
    fs::write(elsewhere.join("spec.md"), draft()).unwrap();
    fs::write(elsewhere.join("precious.txt"), "not a Plan").unwrap();
    fs::create_dir_all(root.join(".invar/plans")).unwrap();
    std::os::unix::fs::symlink(&elsewhere, root.join(".invar/plans").join(CANCEL)).unwrap();
    project.record("SHOP-214");

    project.clean(&["--force"]).success();

    assert!(project.plans().is_empty());
    assert_eq!(
        fs::read_to_string(elsewhere.join("precious.txt")).unwrap(),
        "not a Plan"
    );
    assert!(elsewhere.join("spec.md").is_file());
}

// --- the command's own surface ------------------------------------------------------------------

#[test]
fn the_help_describes_the_flags() {
    Command::cargo_bin("invar")
        .unwrap()
        .args(["clean-spec", "--help"])
        .assert()
        .success()
        .stdout(contains("--dry-run"))
        .stdout(contains("--force"))
        .stdout(contains("--merged"))
        .stdout(contains("TICKET"));
}

#[test]
fn it_appears_in_the_top_level_help() {
    Command::cargo_bin("invar")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(contains("clean-spec"));
}
