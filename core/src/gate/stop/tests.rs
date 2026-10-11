//! Tests for the `SubagentStop` answer (M1-17): who is told, once, what to fix
//! — and every case where the planner is simply let go.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::json;
use tempfile::TempDir;

use super::{Verdict, decide};
use crate::fs::ProjectRoot;

// --- fixtures -------------------------------------------------------------------

fn contract_example(number: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../contracts/formats/plan-header.md");
    let text = fs::read_to_string(path).unwrap();
    let after = text.split_once(&format!("### {number} ")).unwrap().1;
    let body = after.split_once("```markdown\n").unwrap().1;
    body.split_once("\n```\n").unwrap().0.to_owned() + "\n"
}

/// Example 9.1: a valid draft.
fn draft() -> String {
    contract_example("9.1")
}

/// The same Plan with a misspelled heading: a draft that is not valid.
fn broken_draft() -> String {
    draft().replace("## Acceptance criteria\n", "## Acceptance criterion\n")
}

/// Example 9.1's body under example 9.2's header.
fn approved() -> String {
    let body = draft().split_once("\n---\n").unwrap().1.to_owned();
    contract_example("9.2") + &body
}

fn project(config: &str) -> (TempDir, ProjectRoot) {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("invar.yaml"), config).unwrap();
    let root = ProjectRoot::for_test(dir.path());
    (dir, root)
}

fn add_plan(root: &ProjectRoot, name: &str, text: &str) {
    let dir = root.plans_dir().join(name);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("spec.md"), text).unwrap();
}

/// What Claude Code sends a `SubagentStop` hook, give or take the fields that
/// come and go between versions.
fn payload(cwd: &Path, stop_hook_active: bool) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "session_id": "abc123",
        "hook_event_name": "SubagentStop",
        "agent_type": "planner",
        "stop_hook_active": stop_hook_active,
        "cwd": cwd,
        "a_field_from_the_future": { "nested": true },
    }))
    .unwrap()
}

fn decide_in(root: &ProjectRoot, stop_hook_active: bool) -> Verdict {
    decide(&payload(root.path(), stop_hook_active), Path::new("/"))
}

fn unsure_message(verdict: Verdict) -> String {
    match verdict {
        Verdict::Unsure { problem } => problem,
        other => panic!("expected Unsure, got {other:?}"),
    }
}

// --- nothing to say -----------------------------------------------------------------

#[test]
fn a_valid_draft_lets_the_planner_stop() {
    let (_dir, root) = project("");
    add_plan(&root, "SHOP-214-cancel-unpaid-order", &draft());

    assert_eq!(decide_in(&root, false), Verdict::Pass);
}

#[test]
fn a_project_with_no_plans_lets_the_planner_stop() {
    let (_dir, root) = project("");

    assert_eq!(decide_in(&root, false), Verdict::Pass);
}

#[test]
fn only_drafts_are_the_planners_business() {
    // An approved Plan that no longer validates is `plan check`'s to refuse,
    // not something to hold the planner for.
    let (_dir, root) = project("");
    add_plan(
        &root,
        "SHOP-214-cancel-unpaid-order",
        &approved().replace("## Acceptance criteria\n", "## Acceptance criterion\n"),
    );

    assert_eq!(decide_in(&root, false), Verdict::Pass);
}

#[test]
fn a_directory_that_is_not_an_invar_project_has_nothing_to_say() {
    // The plugin is installed for every project, and most are not invar's.
    let dir = tempfile::tempdir().unwrap();

    assert_eq!(
        decide(&payload(dir.path(), false), Path::new("/")),
        Verdict::Pass
    );
}

// --- told once what to fix --------------------------------------------------------------

#[test]
fn an_invalid_draft_blocks_the_stop_and_the_reason_lists_what_to_fix() {
    let (_dir, root) = project("");
    add_plan(&root, "SHOP-214-cancel-unpaid-order", &broken_draft());

    let Verdict::Block { reason } = decide_in(&root, false) else {
        panic!("expected Block");
    };

    assert!(
        reason.starts_with("1 draft Plan is not valid yet, so do not stop"),
        "{reason}"
    );
    // `<path>:<line>: <what to change>`, the path relative to the project.
    assert!(
        reason.contains(".invar/plans/SHOP-214-cancel-unpaid-order/spec.md:"),
        "{reason}"
    );
    assert!(reason.contains("Acceptance criteria"), "{reason}");
    assert!(
        reason.contains("`invar plan validate --all-drafts`"),
        "{reason}"
    );
    assert!(!reason.ends_with('\n'), "{reason:?}");
}

#[test]
fn every_invalid_draft_is_listed_and_a_valid_one_is_not() {
    let (_dir, root) = project("");
    add_plan(&root, "SHOP-214-cancel-unpaid-order", &broken_draft());
    add_plan(
        &root,
        "SHOP-300-other",
        &broken_draft().replace("SHOP-214", "SHOP-300"),
    );
    add_plan(
        &root,
        "SHOP-400-fine",
        &draft().replace("SHOP-214", "SHOP-400"),
    );

    let Verdict::Block { reason } = decide_in(&root, false) else {
        panic!("expected Block");
    };

    assert!(
        reason.starts_with("2 draft Plans are not valid yet"),
        "{reason}"
    );
    assert!(
        reason.contains("SHOP-214-cancel-unpaid-order/spec.md:"),
        "{reason}"
    );
    assert!(reason.contains("SHOP-300-other/spec.md:"), "{reason}");
    assert!(!reason.contains("SHOP-400-fine"), "{reason}");
}

// --- never twice ---------------------------------------------------------------------------

#[test]
fn a_planner_already_asked_once_is_let_go_and_the_developer_is_told() {
    // Blocking again would hold a planner that cannot fix the problem until it
    // runs out of turns.
    let (_dir, root) = project("");
    add_plan(&root, "SHOP-214-cancel-unpaid-order", &broken_draft());

    let Verdict::GaveUp { message } = decide_in(&root, true) else {
        panic!("expected GaveUp");
    };

    assert!(
        message.contains("after being asked once to fix it"),
        "{message}"
    );
    assert!(
        message.contains("SHOP-214-cancel-unpaid-order/spec.md:"),
        "{message}"
    );
}

#[test]
fn the_second_stop_with_everything_fixed_is_just_a_stop() {
    let (_dir, root) = project("");
    add_plan(&root, "SHOP-214-cancel-unpaid-order", &draft());

    assert_eq!(decide_in(&root, true), Verdict::Pass);
}

#[test]
fn a_stop_hook_active_that_is_not_a_boolean_counts_as_the_first_time() {
    let (_dir, root) = project("");
    add_plan(&root, "SHOP-214-cancel-unpaid-order", &broken_draft());
    let input = serde_json::to_vec(&json!({
        "stop_hook_active": "yes",
        "cwd": root.path(),
    }))
    .unwrap();

    assert!(matches!(
        decide(&input, Path::new("/")),
        Verdict::Block { .. }
    ));
}

// --- where the project is ------------------------------------------------------------------------

#[test]
fn the_payloads_cwd_is_the_project_and_the_process_directory_is_the_fallback() {
    let (_dir, root) = project("");
    add_plan(&root, "SHOP-214-cancel-unpaid-order", &broken_draft());
    let elsewhere = tempfile::tempdir().unwrap();

    // The payload names the project; the process happens to be elsewhere.
    assert!(matches!(
        decide(&payload(root.path(), false), elsewhere.path()),
        Verdict::Block { .. }
    ));

    // No `cwd` in the payload: the directory the hook runs in is used.
    let without_cwd = serde_json::to_vec(&json!({ "stop_hook_active": false })).unwrap();
    assert!(matches!(
        decide(&without_cwd, root.path()),
        Verdict::Block { .. }
    ));
    assert_eq!(decide(&without_cwd, elsewhere.path()), Verdict::Pass);
}

#[test]
fn it_works_from_a_directory_below_the_project_root() {
    let (_dir, root) = project("");
    add_plan(&root, "SHOP-214-cancel-unpaid-order", &broken_draft());
    let deep: PathBuf = root.path().join("app/Http/Controllers");
    fs::create_dir_all(&deep).unwrap();

    assert!(matches!(
        decide(&payload(&deep, false), Path::new("/")),
        Verdict::Block { .. }
    ));
}

// --- every doubt lets the planner stop ------------------------------------------------------------

#[test]
fn input_it_cannot_read_lets_the_planner_stop_and_says_why() {
    for input in [&b""[..], b"   \n", b"not json", b"{\"a\":", b"["] {
        let message = unsure_message(decide(input, Path::new("/")));
        assert!(!message.is_empty(), "{input:?}");
    }
    assert!(unsure_message(decide(b"", Path::new("/"))).contains("no hook input"));
    assert!(unsure_message(decide(b"nope", Path::new("/"))).contains("not valid JSON"));
}

#[test]
fn a_configuration_it_cannot_load_lets_the_planner_stop_and_says_why() {
    // `scenarios: true` is refused by every command until feature files exist;
    // a hook must not hold the planner because of that.
    let (_dir, root) = project("scenarios: true\n");
    add_plan(&root, "SHOP-214-cancel-unpaid-order", &broken_draft());

    let message = unsure_message(decide_in(&root, false));

    assert!(
        message.contains("feature") || message.contains("scenarios"),
        "{message}"
    );
}

#[test]
fn an_invar_yaml_that_is_not_yaml_lets_the_planner_stop_and_says_why() {
    let (_dir, root) = project("scenarios: [unclosed\n");
    add_plan(&root, "SHOP-214-cancel-unpaid-order", &broken_draft());

    assert!(matches!(decide_in(&root, false), Verdict::Unsure { .. }));
}
