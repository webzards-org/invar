//! End-to-end tests for `invar config check` (#157, `invar-yaml.md` §3.9): the real
//! binary, in real git repositories, with no network.

use std::fs;
use std::path::PathBuf;

use assert_cmd::Command;
use assert_cmd::assert::Assert;
use predicates::str::contains;
use tempfile::TempDir;

/// Every key the list covers, at its strongest.
const CONFIG: &str = "review:\n  offer: true\napproval:\n  signed: true\n\
    acceptance:\n  required: true\n  dir: tests/Acceptance\n  \
    run: [\"pest\", \"{report}\", \"{files}\"]\n\
    cycle:\n  required_paths: [\"app/Models/**\", \"database/migrations/\"]\n";

/// [`CONFIG`] without `approval.signed`: where a bare `Invar-Config:` trailer counts
/// (`invar-yaml.md` §3.9). With it on, the trailer has to be signed, which
/// `tests/signed_config.rs` covers.
fn unsigned() -> String {
    CONFIG.replace("approval:\n  signed: true\n", "")
}

struct Project {
    dir: TempDir,
}

impl Project {
    /// `main` holding `config` as invar.yaml, then a branch checked out from it.
    fn new(config: Option<&str>) -> Self {
        let project = Self {
            dir: tempfile::tempdir().unwrap(),
        };
        project.write(
            ".gitconfig-test",
            "[user]\n\tname = Test\n\temail = test@example.com\n[commit]\n\tgpgsign = false\n",
        );
        project.git(&["init", "-q", "-b", "main"]);
        project.write(".gitignore", ".gitconfig-test\n");
        if let Some(config) = config {
            project.write("invar.yaml", config);
        }
        project.write("README.md", "base\n");
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

    /// invar.yaml on the branch as `text`, committed.
    fn change_config(&self, text: &str) {
        self.write("invar.yaml", text);
        self.commit("change invar.yaml");
    }

    fn check(&self) -> Assert {
        Command::cargo_bin("invar")
            .unwrap()
            .current_dir(self.root())
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", self.root().join(".gitconfig-test"))
            .env("HOME", self.root())
            .args(["config", "check", "--base", "main"])
            .assert()
    }

    fn head(&self) -> String {
        self.git(&["rev-parse", "HEAD"])
            .trim()
            .chars()
            .take(12)
            .collect()
    }
}

fn stdout_of(assert: &Assert) -> String {
    String::from_utf8(assert.get_output().stdout.clone()).unwrap()
}

fn stderr_of(assert: &Assert) -> String {
    String::from_utf8(assert.get_output().stderr.clone()).unwrap()
}

/// Failed: non-zero, the report ending `config check: failed`, every part on stderr.
fn failed(assert: Assert, parts: &[&str]) -> (String, String) {
    let assert = assert.failure();
    let (out, err) = (stdout_of(&assert), stderr_of(&assert));
    assert!(out.ends_with("config check: failed\n"), "{out}");
    for part in parts {
        assert!(err.contains(part), "{part}:\n{err}");
    }
    (out, err)
}

#[test]
fn with_no_invar_yaml_at_the_base_the_check_is_off() {
    let project = Project::new(None);
    project.change_config("review:\n  offer: false\n");
    project.check().success().stdout(contains(
        "config check: off: main has no invar.yaml, so there is nothing to weaken",
    ));
}

#[test]
fn no_change_passes() {
    let project = Project::new(Some(CONFIG));
    project.write("README.md", "changed\n");
    project.commit("docs");
    let assert = project.check().success();
    let out = stdout_of(&assert);
    assert!(
        out.contains("weakenings (base -> this branch):\n  none\n"),
        "{out}"
    );
    assert!(
        out.ends_with("config check: passed: no check is weakened\n"),
        "{out}"
    );
}

#[test]
fn each_weakening_fails_naming_its_base_and_head_values() {
    let cases = [
        (
            CONFIG.replace(
                "acceptance:\n  required: true\n  dir: tests/Acceptance\n  run: [\"pest\", \"{report}\", \"{files}\"]\n",
                "",
            ),
            "acceptance: present -> absent",
        ),
        (
            CONFIG.replace("dir: tests/Acceptance", "dir: tests/Elsewhere"),
            "acceptance.dir: tests/Acceptance -> tests/Elsewhere",
        ),
        (
            CONFIG.replace("run: [\"pest\",", "run: [\"true\","),
            "acceptance.run: [\"pest\", \"{report}\", \"{files}\"] -> [\"true\", \"{report}\", \"{files}\"]",
        ),
        (
            CONFIG.replace("required: true", "required: false"),
            "acceptance.required: true -> false",
        ),
        (
            CONFIG.replace("signed: true", "signed: false"),
            "approval.signed: true -> false",
        ),
        (
            CONFIG.replace("\"app/Models/**\", ", ""),
            "cycle.required_paths: app/Models/** -> removed",
        ),
        (
            CONFIG.replace("offer: true", "offer: false"),
            "review.offer: true -> false",
        ),
    ];
    for (head, says) in cases {
        let project = Project::new(Some(CONFIG));
        project.change_config(&head);
        let (out, _) = failed(
            project.check(),
            &[
                "weakens 1 check(s) in invar.yaml against the base, with no `Invar-Config:` trailer",
                &format!("  {says}"),
                // The base has `approval.signed`, so the way out is a signed one.
                "invar config accept --reason \"<reason>\"",
            ],
        );
        assert!(out.contains(&format!("  {says}\n")), "{out}");
    }
}

#[test]
fn removing_invar_yaml_altogether_is_every_weakening_at_once() {
    let project = Project::new(Some(CONFIG));
    project.git(&["rm", "-q", "invar.yaml"]);
    project.commit("no config");
    failed(
        project.check(),
        &[
            // `review.offer` defaults to true, so no file is not a weakening of it.
            "weakens 4 check(s)",
            "acceptance: present -> absent",
            "approval.signed: true -> false",
            "cycle.required_paths: database/migrations/ -> removed",
        ],
    );
}

#[test]
fn a_weakening_with_the_trailer_passes_and_prints_the_reason() {
    let project = Project::new(Some(&unsigned()));
    project.write(
        "invar.yaml",
        &unsigned().replace("dir: tests/Acceptance", "dir: tests/Feature/Acceptance"),
    );
    project.commit_with_trailer(
        "move the acceptance tests",
        "Invar-Config: the suite moved under Feature, agreed with the team",
    );
    let commit = project.head();
    let assert = project.check().success();
    let out = stdout_of(&assert);
    assert!(
        out.contains(&format!(
            "  {commit}: Invar-Config: the suite moved under Feature, agreed with the team\n"
        )),
        "{out}"
    );
    assert!(
        out.contains("  acceptance.dir: tests/Acceptance -> tests/Feature/Acceptance\n"),
        "{out}"
    );
    assert!(
        out.ends_with("config check: passed: weakened with `Invar-Config:` (1 trailer(s))\n"),
        "{out}"
    );
}

#[test]
fn an_empty_reason_does_not_count() {
    let project = Project::new(Some(CONFIG));
    project.write("invar.yaml", &CONFIG.replace("offer: true", "offer: false"));
    project.commit_with_trailer("no review", "Invar-Config:");
    let (out, _) = failed(project.check(), &["review.offer: true -> false"]);
    assert!(out.contains("trailers:\n  none\n"), "{out}");
}

#[test]
fn strengthening_and_other_keys_pass() {
    let base = "review:\n  offer: false\ncycle:\n  required_paths: [\"app/Models/**\"]\n";
    let project = Project::new(Some(base));
    project.change_config(
        "review:\n  offer: true\napproval:\n  signed: true\n\
         cycle:\n  required_paths: [\"app/Models/**\", \"app/Policies/**\"]\n\
         git:\n  base_branch: main\ndone:\n  commit: true\n",
    );
    project
        .check()
        .success()
        .stdout(contains("config check: passed: no check is weakened\n"));
}

#[test]
fn an_invalid_invar_yaml_on_the_branch_is_an_error() {
    let project = Project::new(Some(CONFIG));
    project.change_config("review: [unclosed\n");
    let assert = project.check().failure().stdout("");
    assert!(
        stderr_of(&assert).contains("cannot compare the configuration: HEAD:invar.yaml"),
        "{}",
        stderr_of(&assert)
    );
}

#[test]
fn the_trailer_survives_rebase_signoff() {
    let project = Project::new(Some(&unsigned()));
    project.write(
        "invar.yaml",
        &unsigned().replace("offer: true", "offer: false"),
    );
    project.commit_with_trailer("unsign", "Invar-Config: no security keys until January");
    project.git(&["checkout", "-q", "main"]);
    project.write("CHANGELOG.md", "later\n");
    project.commit("later on main");
    project.git(&["checkout", "-q", "feat/SHOP-214-cancel"]);
    project.git(&["rebase", "-q", "--signoff", "main"]);
    let message = project.git(&["log", "-1", "--format=%B"]);
    assert!(
        message.contains("Signed-off-by: Test <test@example.com>"),
        "{message}"
    );
    project
        .check()
        .success()
        .stdout(contains("Invar-Config: no security keys until January"));
}

#[test]
fn a_shallow_clone_missing_the_merge_base_says_to_fetch_the_history() {
    let project = Project::new(Some(CONFIG));
    project.change_config(&CONFIG.replace("offer: true", "offer: false"));
    project.write("README.md", "again\n");
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
        .args(["config", "check", "--base", "origin/main"])
        .assert()
        .failure()
        .stdout("");
    let err = stderr_of(&assert);
    for part in ["shallow clone", "`origin/main`", "fetch-depth: 0"] {
        assert!(err.contains(part), "{part}:\n{err}");
    }
}

// --- the branch does not choose its base (D-20) -----------------------------------------

/// The review's reproduction: `main` signs approvals and requires a cycle for models; one
/// commit on `feat/x` stops signing, names itself as the base, and changes a model.
fn chooses_its_own_base() -> Project {
    let main = "approval:\n  signed: true\ncycle:\n  required_paths: [\"app/Models/**\"]\n";
    let project = Project::new(Some(main));
    project.git(&["checkout", "-q", "main"]);
    project.write("app/Models/O.php", "<?php\n");
    project.commit("a model");
    project.git(&["checkout", "-q", "-b", "feat/x"]);
    project.write(
        "invar.yaml",
        "git:\n  base_branch: feat/x\ncycle:\n  required_paths: [\"app/Models/**\"]\n",
    );
    project.write("app/Models/O.php", "<?php\n// changed\n");
    project.commit("stop signing, choose the base, change a model");
    project
}

fn invar(project: &Project, args: &[&str]) -> Assert {
    Command::cargo_bin("invar")
        .unwrap()
        .current_dir(project.root())
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", project.root().join(".gitconfig-test"))
        .env("HOME", project.root())
        .args(args)
        .assert()
}

#[test]
fn without_base_a_branch_cannot_choose_what_config_and_cycle_check_compare_with() {
    let project = chooses_its_own_base();
    let note = "sets `git.base_branch: feat/x`, and the base used is `main`";

    let (out, _) = failed(
        invar(&project, &["config", "check"]),
        &["approval.signed: true -> false"],
    );
    assert!(out.starts_with("base: main\n"), "{out}");
    assert!(out.contains(note), "{out}");

    let assert = invar(&project, &["cycle", "check"]).failure();
    let out = stdout_of(&assert);
    assert!(out.starts_with("base: main\n"), "{out}");
    assert!(out.contains(note), "{out}");
    assert!(out.ends_with("cycle check: failed\n"), "{out}");
    assert!(
        stderr_of(&assert).contains("app/Models/O.php"),
        "{}",
        stderr_of(&assert)
    );
}

#[test]
fn with_base_both_fail_as_before_and_say_nothing_of_the_key() {
    let project = chooses_its_own_base();
    let (out, _) = failed(
        invar(&project, &["config", "check", "--base", "main"]),
        &["approval.signed: true -> false"],
    );
    assert!(!out.contains("and the base used is"), "{out}");
    let assert = invar(&project, &["cycle", "check", "--base", "main"]).failure();
    assert!(stdout_of(&assert).ends_with("cycle check: failed\n"));
}

#[test]
fn config_check_notes_a_changed_git_base_branch_without_counting_it() {
    let project = Project::new(Some(CONFIG));
    project.change_config(&format!("{CONFIG}git:\n  base_branch: develop\n"));
    let assert = project.check().success();
    let out = stdout_of(&assert);
    assert!(
        out.contains("note: this branch changes `git.base_branch`: absent -> develop"),
        "{out}"
    );
    assert!(
        out.ends_with("config check: passed: no check is weakened\n"),
        "{out}"
    );
}
