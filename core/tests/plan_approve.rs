//! End-to-end tests for `invar plan approve` (M1-9). `git` runs for real; its
//! configuration is isolated per test through the environment, so the
//! developer's own `user.name` never leaks in.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as Process;

use assert_cmd::Command;
use predicates::prelude::*;
use predicates::str::contains;
use tempfile::TempDir;

const SPEC: &str = ".invar/plans/PROJ-412-reserve-stock/spec.md";

/// 00:30 in Belgrade on the 5th is still the 4th in UTC: the approval date
/// must be the local one (plan-header.md D-13).
const COMMITTER_DATE: &str = "2026-10-05T00:30:00+02:00";

fn plan_text(ticket: &str) -> String {
    format!(
        "---\nTicket: {ticket}\nModule: orders\nStatus: draft\nApproved-by:\n\
         Approved-at:\nApproved-checksum:\n---\n\n\
         ## Ticket\n{ticket} — Reserve stock when an order is placed.\n\n\
         ## Intent\nStock is reserved at order time.\n\n\
         ## Scope\n`orders` only.\n\n\
         ## Open questions / answers\nNone were needed.\n\n\
         ## Rejected alternatives\nNone considered.\n\n\
         ## Acceptance criteria\n\n\
         - Given 3 in stock, when I order 2, then 1 remains available.\n\
         - Given 1 in stock, when I order 2, then the order is refused.\n\n\
         ## Implementation context\n- AI_CONTEXT: controllers stay thin\n"
    )
}

/// A project with a Plan, and a git configuration of its own.
struct Project {
    dir: TempDir,
}

impl Project {
    /// `user_name`: what the isolated global git config says, if anything.
    fn new(user_name: Option<&str>) -> Self {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("invar.yaml"), "").unwrap();
        let project = Self { dir };
        fs::write(project.gitconfig(), "").unwrap();
        if let Some(name) = user_name {
            project.git(&["config", "--global", "user.name", name]);
        }
        project.write_spec(&plan_text("PROJ-412"));
        project
    }

    fn root(&self) -> PathBuf {
        self.dir.path().canonicalize().unwrap()
    }

    fn gitconfig(&self) -> PathBuf {
        self.dir.path().join(".test-gitconfig")
    }

    fn spec(&self) -> PathBuf {
        self.root().join(SPEC)
    }

    fn write_spec(&self, text: &str) {
        fs::create_dir_all(self.spec().parent().unwrap()).unwrap();
        fs::write(self.spec(), text).unwrap();
    }

    fn read_spec(&self) -> String {
        fs::read_to_string(self.spec()).unwrap()
    }

    fn git(&self, args: &[&str]) {
        let status = Process::new("git")
            .args(args)
            .current_dir(self.root())
            .envs(self.git_env())
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?}");
    }

    fn git_env(&self) -> Vec<(&'static str, String)> {
        vec![
            ("GIT_CONFIG_GLOBAL", self.gitconfig().display().to_string()),
            ("GIT_CONFIG_NOSYSTEM", "1".into()),
            ("HOME", self.root().display().to_string()),
            ("GIT_COMMITTER_DATE", COMMITTER_DATE.into()),
        ]
    }

    fn approve(&self, args: &[&str]) -> assert_cmd::assert::Assert {
        self.approve_in(&self.root(), args)
    }

    fn approve_in(&self, cwd: &Path, args: &[&str]) -> assert_cmd::assert::Assert {
        Command::cargo_bin("invar")
            .unwrap()
            .current_dir(cwd)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("XDG_CONFIG_HOME")
            .envs(self.git_env())
            .args(["plan", "approve"])
            .args(args)
            .assert()
    }
}

/// The digest `invar hash` gives the body alone, as an independent check of
/// `Approved-checksum`.
fn body_digest(text: &str, scratch: &Path) -> String {
    let body = text
        .split_once("\n---\n")
        .unwrap()
        .1
        .trim_start_matches('\n');
    let file = scratch.join("body.md");
    fs::write(&file, body).unwrap();
    let output = Command::cargo_bin("invar")
        .unwrap()
        .arg("hash")
        .arg(&file)
        .output()
        .unwrap();
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn header_value<'a>(text: &'a str, key: &str) -> &'a str {
    text.lines()
        .find_map(|line| line.strip_prefix(&format!("{key}:")))
        .unwrap_or_else(|| panic!("no {key}"))
        .trim()
}

// --- approving -------------------------------------------------------------------

#[test]
fn approves_a_draft_printing_the_criteria_first() {
    let project = Project::new(Some("Rade Brković"));
    let draft = project.read_spec();
    let assert = project.approve(&["PROJ-412"]).success().stderr("");

    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).into_owned();
    let criteria = stdout
        .find("- Given 3 in stock")
        .expect("the criteria are printed");
    let approved = stdout.find("approved: ").expect("the result is printed");
    assert!(
        criteria < approved,
        "criteria after the approval:\n{stdout}"
    );
    assert!(stdout.contains(&format!("Acceptance criteria of PROJ-412 ({SPEC})")));
    assert!(stdout.contains("- Given 1 in stock, when I order 2, then the order is refused."));

    let written = project.read_spec();
    assert_eq!(header_value(&written, "Status"), "approved");
    assert_eq!(header_value(&written, "Approved-by"), "Rade Brković");
    assert_eq!(header_value(&written, "Approved-at"), "2026-10-05");
    let checksum = header_value(&written, "Approved-checksum");
    assert_eq!(checksum, body_digest(&draft, project.dir.path()));
    assert!(stdout.contains(&format!("Approved-checksum: {checksum}")));

    // The body is byte-identical.
    assert_eq!(
        written.split_once("\n---\n").unwrap().1,
        draft.split_once("\n---\n").unwrap().1
    );
}

#[test]
fn a_path_works_and_so_does_running_from_a_subdirectory() {
    let project = Project::new(Some("rade"));
    let sub = project.root().join("app");
    fs::create_dir_all(&sub).unwrap();
    project.approve_in(&sub, &["PROJ-412"]).success();
    let project = Project::new(Some("rade"));
    project
        .approve(&[".invar/plans/PROJ-412-reserve-stock"])
        .success()
        .stdout(contains(format!("approved: {SPEC}")));
}

#[test]
fn the_repository_user_name_wins_over_the_global_one() {
    let project = Project::new(Some("global name"));
    project.git(&["init", "-q"]);
    project.git(&["config", "user.name", "repo name"]);
    project.approve(&["PROJ-412"]).success();
    assert_eq!(
        header_value(&project.read_spec(), "Approved-by"),
        "repo name"
    );
}

// --- refusals --------------------------------------------------------------------

#[test]
fn an_invalid_plan_is_refused_naming_the_problems() {
    let project = Project::new(Some("rade"));
    let draft = plan_text("PROJ-412").replace(
        "- Given 3 in stock, when I order 2, then 1 remains available.\n\
         - Given 1 in stock, when I order 2, then the order is refused.\n",
        "_none_\n",
    );
    project.write_spec(&draft);
    project
        .approve(&["PROJ-412"])
        .failure()
        .stdout(contains(format!(
            "{SPEC}:25: `## Acceptance criteria` is empty"
        )))
        .stdout(contains("Acceptance criteria of").not())
        .stderr(contains(
            "error: the Plan is not valid (1 problem), so it cannot be approved",
        ));
    assert_eq!(project.read_spec(), draft);
}

#[test]
fn a_missing_user_name_is_a_clear_error_and_nothing_is_written() {
    for name in [None, Some("   ")] {
        let project = Project::new(name);
        let draft = project.read_spec();
        project
            .approve(&["PROJ-412"])
            .failure()
            .stdout("")
            .stderr(contains("`git config user.name` is not set"))
            .stderr(contains("git config --global user.name"));
        assert_eq!(project.read_spec(), draft, "{name:?}");
    }
}

#[test]
fn lookup_errors_are_reported() {
    let project = Project::new(Some("rade"));
    project
        .approve(&["PROJ-9"])
        .failure()
        .stderr(contains("no Plan for `PROJ-9`"));
}

// --- re-approval (D-12) --------------------------------------------------------------

#[test]
fn approving_again_with_an_unchanged_body_does_nothing() {
    let project = Project::new(Some("rade"));
    project.approve(&["PROJ-412"]).success();
    let approved = project.read_spec();
    project.git(&["config", "--global", "user.name", "someone else"]);
    for args in [&["PROJ-412"][..], &["PROJ-412", "--reapprove"]] {
        project
            .approve(args)
            .success()
            .stdout(contains("already approved").and(contains("by rade on 2026-10-05")))
            .stdout(contains("Acceptance criteria of").not());
    }
    assert_eq!(project.read_spec(), approved);
}

#[test]
fn a_body_changed_after_approval_needs_reapprove() {
    let project = Project::new(Some("rade"));
    project.approve(&["PROJ-412"]).success();
    let edited = project.read_spec().replace(
        "then the order is refused",
        "then the order waits for stock",
    );
    project.write_spec(&edited);

    project
        .approve(&["PROJ-412"])
        .failure()
        .stdout("")
        .stderr(contains(
            "the body changed after rade approved it on 2026-10-05",
        ))
        .stderr(contains("--reapprove"));
    assert_eq!(project.read_spec(), edited, "a refusal must not write");

    project.git(&["config", "--global", "user.name", "mila"]);
    project
        .approve(&["PROJ-412", "--reapprove"])
        .success()
        .stdout(contains(
            "re-approving: the body changed after rade approved it",
        ))
        .stdout(contains("then the order waits for stock"));
    let written = project.read_spec();
    assert_eq!(header_value(&written, "Approved-by"), "mila");
    assert_eq!(
        header_value(&written, "Approved-checksum"),
        body_digest(&edited, project.dir.path())
    );
}

#[test]
fn an_approval_written_by_hand_needs_reapprove() {
    let project = Project::new(Some("rade"));
    project.write_spec(&plan_text("PROJ-412").replace("Status: draft", "Status: approved"));
    project
        .approve(&["PROJ-412"])
        .failure()
        .stderr(contains("no `invar plan approve` recorded this approval"));
    project
        .approve(&["PROJ-412", "--reapprove"])
        .success()
        .stdout(contains(
            "re-approving: the earlier approval was not recorded",
        ));
    assert_eq!(header_value(&project.read_spec(), "Approved-by"), "rade");
}

#[test]
fn a_name_that_cannot_be_recorded_is_refused_before_the_criteria_are_shown() {
    // `rade # admin` cannot be written into a header line as it is. That is
    // known before anything is shown, so the person is not asked to read the
    // criteria of an approval that was never going to be recorded.
    let project = Project::new(Some("rade # admin"));
    let draft = project.read_spec();
    project
        .approve(&["PROJ-412"])
        .failure()
        .stdout("")
        .stderr(contains("cannot record the approval"));
    assert_eq!(project.read_spec(), draft);
}
