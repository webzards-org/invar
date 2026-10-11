//! End-to-end tests for `invar plan validate --all-drafts --hook` (M1-17): the
//! real binary, with the JSON a `SubagentStop` hook receives piped in.
//!
//! What matters here is what reaches the planner, and that it is never held
//! twice: exit 2 and a reason once, then let go.

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use predicates::prelude::*;
use predicates::str::contains;
use serde_json::{Value, json};
use tempfile::TempDir;

const PLAN_DIR: &str = ".invar/plans/SHOP-214-cancel-unpaid-order";

fn contract_example(number: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../contracts/formats/plan-header.md");
    let text = fs::read_to_string(path).unwrap();
    let after = text.split_once(&format!("### {number} ")).unwrap().1;
    let body = after.split_once("```markdown\n").unwrap().1;
    body.split_once("\n```\n").unwrap().0.to_owned() + "\n"
}

fn draft() -> String {
    contract_example("9.1")
}

fn broken_draft() -> String {
    draft().replace("## Acceptance criteria\n", "## Acceptance criterion\n")
}

struct Project {
    dir: TempDir,
}

impl Project {
    fn new(plan: Option<&str>) -> Self {
        let project = Self {
            dir: tempfile::tempdir().unwrap(),
        };
        fs::write(project.root().join("invar.yaml"), "").unwrap();
        if let Some(plan) = plan {
            let dir = project.root().join(PLAN_DIR);
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join("spec.md"), plan).unwrap();
        }
        project
    }

    fn root(&self) -> PathBuf {
        self.dir.path().canonicalize().unwrap()
    }

    /// What Claude Code pipes to a `SubagentStop` hook.
    fn payload(&self, stop_hook_active: bool) -> String {
        json!({
            "session_id": "abc123",
            "hook_event_name": "SubagentStop",
            "agent_type": "planner",
            "stop_hook_active": stop_hook_active,
            "cwd": self.root(),
        })
        .to_string()
    }

    fn hook(&self, stdin: &str) -> assert_cmd::assert::Assert {
        Command::cargo_bin("invar")
            .unwrap()
            .current_dir(self.root())
            .args(["plan", "validate", "--all-drafts", "--hook"])
            .write_stdin(stdin.to_owned())
            .assert()
    }
}

fn json_of(assert: &assert_cmd::assert::Assert) -> Value {
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert_eq!(
        stdout.lines().count(),
        1,
        "one JSON object on one line:\n{stdout}"
    );
    serde_json::from_str(&stdout).unwrap()
}

// --- nothing to say -----------------------------------------------------------------

#[test]
fn a_valid_draft_lets_the_planner_stop_silently() {
    let project = Project::new(Some(&draft()));

    project
        .hook(&project.payload(false))
        .success()
        .stdout("")
        .stderr("");
}

#[test]
fn a_project_with_no_plans_lets_it_stop_silently() {
    let project = Project::new(None);

    project
        .hook(&project.payload(false))
        .success()
        .stdout("")
        .stderr("");
}

#[test]
fn a_directory_that_is_not_an_invar_project_is_none_of_its_business() {
    // The plugin is installed for every project.
    let dir = tempfile::tempdir().unwrap();

    Command::cargo_bin("invar")
        .unwrap()
        .current_dir(dir.path())
        .args(["plan", "validate", "--all-drafts", "--hook"])
        .write_stdin(json!({ "stop_hook_active": false }).to_string())
        .assert()
        .success()
        .stdout("")
        .stderr("");
}

// --- told once --------------------------------------------------------------------------

#[test]
fn an_invalid_draft_blocks_the_stop_with_what_to_fix() {
    let project = Project::new(Some(&broken_draft()));

    let assert = project
        .hook(&project.payload(false))
        .failure()
        .code(2)
        .stderr(contains("1 draft Plan is not valid yet, so do not stop"))
        .stderr(contains(format!("{PLAN_DIR}/spec.md:")));

    // The same reason as JSON, which is what a stop hook's decision looks like.
    let output = json_of(&assert);
    assert_eq!(output["decision"], "block");
    let reason = output["reason"].as_str().unwrap();
    assert!(reason.contains("Acceptance criteria"), "{reason}");
    assert!(
        reason.contains("`invar plan validate --all-drafts`"),
        "{reason}"
    );
}

#[test]
fn asked_again_with_the_problems_still_there_it_lets_go_and_tells_the_developer() {
    // Blocking twice would hold a planner that cannot fix the problem.
    let project = Project::new(Some(&broken_draft()));

    let assert = project
        .hook(&project.payload(true))
        .success()
        .stderr(contains("after being asked once to fix it"));

    let output = json_of(&assert);
    assert!(output.get("decision").is_none(), "{output}");
    assert!(
        output["systemMessage"]
            .as_str()
            .unwrap()
            .contains("still not valid"),
        "{output}"
    );
}

// --- every doubt lets it stop ---------------------------------------------------------------

#[test]
fn input_it_cannot_read_lets_the_planner_stop_and_says_why() {
    let project = Project::new(Some(&broken_draft()));

    for stdin in ["", "not json", "{\"stop_hook_active\":"] {
        let assert = project.hook(stdin).success();
        let output = json_of(&assert);
        assert!(
            output["systemMessage"]
                .as_str()
                .unwrap()
                .contains("the planner may stop"),
            "{stdin:?}: {output}"
        );
    }
}

#[test]
fn a_configuration_it_cannot_load_lets_the_planner_stop() {
    let project = Project::new(Some(&broken_draft()));
    fs::write(project.root().join("invar.yaml"), "scenarios: true\n").unwrap();

    project
        .hook(&project.payload(false))
        .success()
        .stdout(contains("the planner may stop"));
}

// --- arguments --------------------------------------------------------------------------------

#[test]
fn the_hook_flag_needs_all_drafts() {
    // A usage error is exit 1, never 2: a hook with arguments this binary does
    // not understand must not read as "block".
    let project = Project::new(None);

    Command::cargo_bin("invar")
        .unwrap()
        .current_dir(project.root())
        .args(["plan", "validate", "--hook"])
        .assert()
        .failure()
        .code(1)
        .stdout("")
        .stderr(contains("--all-drafts"));
}

#[test]
fn without_the_hook_flag_the_same_problems_are_printed_and_the_exit_is_1() {
    // The reason the flag exists: this is invisible to a hook.
    let project = Project::new(Some(&broken_draft()));

    Command::cargo_bin("invar")
        .unwrap()
        .current_dir(project.root())
        .args(["plan", "validate", "--all-drafts"])
        .assert()
        .failure()
        .code(1)
        .stdout(contains("Acceptance criteria"))
        // Not a hook's answer: no JSON decision.
        .stdout(contains("\"decision\"").not());
}
