//! End-to-end tests for `invar plan guard` (M1-10): the real binary, with a
//! `PreToolUse` payload piped into it the way Claude Code does. The payload
//! shapes are the ones Claude Code 2.1.291 sent (`plugin/NOTES.md` §11).

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use predicates::prelude::*;
use predicates::str::contains;
use serde_json::{Value, json};
use tempfile::TempDir;

const SPEC: &str = ".invar/plans/SHOP-214-cancel/spec.md";

const APPROVED: &str = "---\nTicket: SHOP-214\nModule: orders\nStatus: approved\n\
    Approved-by: rade\nApproved-at: 2026-10-05\n\
    Approved-checksum: sha256:b9caa4c64bc83723bb4e930d7fedeae733950c5210ee0e30f4ece84eb2c2cf7b\n\
    ---\n\n## Ticket\nSHOP-214\n";

struct Project {
    dir: TempDir,
}

impl Project {
    fn new(plan: &str) -> Self {
        let project = Self {
            dir: tempfile::tempdir().unwrap(),
        };
        fs::write(project.root().join("invar.yaml"), "").unwrap();
        let spec = project.root().join(SPEC);
        fs::create_dir_all(spec.parent().unwrap()).unwrap();
        fs::write(spec, plan).unwrap();
        project
    }

    fn root(&self) -> PathBuf {
        self.dir.path().canonicalize().unwrap()
    }

    fn path(&self, relative: &str) -> String {
        self.root().join(relative).display().to_string()
    }
}

/// What Claude Code sends for `Edit` (verified payload shape).
fn edit(cwd: &Path, file_path: &str) -> String {
    json!({
        "session_id": "8d4316a8-82ea-5dc0-a7e0-18f9e2621b46",
        "transcript_path": "/root/.claude/projects/x/8d4316a8.jsonl",
        "cwd": cwd,
        "permission_mode": "default",
        "effort": { "level": "medium" },
        "hook_event_name": "PreToolUse",
        "tool_name": "Edit",
        "tool_input": {
            "file_path": file_path,
            "old_string": "refused with 403",
            "new_string": "refused with 409",
            "replace_all": false
        },
        "tool_use_id": "toolu_01Kpa9ZF1iydktghpFB6qdt1"
    })
    .to_string()
}

/// What Claude Code sends for `Write`.
fn write(cwd: &Path, file_path: &str) -> String {
    json!({
        "cwd": cwd,
        "hook_event_name": "PreToolUse",
        "tool_name": "Write",
        "tool_input": { "file_path": file_path, "content": "hi" },
    })
    .to_string()
}

/// What Claude Code sends for `Bash`: a command line, no file path.
fn bash(cwd: &Path, command: &str) -> String {
    json!({
        "cwd": cwd,
        "hook_event_name": "PreToolUse",
        "tool_name": "Bash",
        "tool_input": { "command": command, "description": "run it" },
    })
    .to_string()
}

fn guard(stdin: impl Into<Vec<u8>>) -> assert_cmd::assert::Assert {
    Command::cargo_bin("invar")
        .unwrap()
        .args(["plan", "guard"])
        .write_stdin(stdin)
        .assert()
}

// --- deny -----------------------------------------------------------------------

#[test]
fn an_edit_to_an_approved_plan_is_denied() {
    let project = Project::new(APPROVED);
    let assert = guard(edit(&project.root(), &project.path(SPEC)))
        .code(2)
        .stderr(contains(format!(
            "{SPEC} is the approved Plan for SHOP-214"
        )));

    // stdout is exactly one JSON object, the PreToolUse deny decision.
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert_eq!(stdout.lines().count(), 1, "{stdout}");
    let output: Value = serde_json::from_str(&stdout).unwrap();
    let decision = &output["hookSpecificOutput"];
    assert_eq!(decision["hookEventName"], "PreToolUse");
    assert_eq!(decision["permissionDecision"], "deny");
    let reason = decision["permissionDecisionReason"].as_str().unwrap();
    assert!(reason.contains("approved Plan for SHOP-214"), "{reason}");
    assert!(reason.contains("--reapprove"), "{reason}");
}

#[test]
fn a_write_over_an_approved_plan_is_denied() {
    let project = Project::new(APPROVED);
    guard(write(&project.root(), &project.path(SPEC))).code(2);
}

#[test]
fn a_relative_path_is_read_from_the_payloads_cwd() {
    let project = Project::new(APPROVED);
    guard(edit(&project.root(), SPEC)).code(2);
    guard(edit(
        &project.root().join(".invar"),
        "plans/SHOP-214-cancel/spec.md",
    ))
    .code(2);
}

#[test]
fn an_approved_plan_with_crlf_is_denied_too() {
    let project = Project::new(&APPROVED.replace('\n', "\r\n"));
    guard(edit(&project.root(), &project.path(SPEC))).code(2);
}

// --- commands only the developer runs -------------------------------------------

#[test]
fn running_plan_approve_from_a_shell_is_denied() {
    let project = Project::new(APPROVED);
    let assert = guard(bash(
        &project.root(),
        "invar plan approve SHOP-214 --reapprove",
    ))
    .code(2)
    .stderr(contains("is the developer's decision, not yours"));

    // The same answer as for an approved Plan: one JSON object on stdout.
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
    assert_eq!(stdout.lines().count(), 1, "{stdout}");
    let output: Value = serde_json::from_str(&stdout).unwrap();
    let decision = &output["hookSpecificOutput"];
    assert_eq!(decision["hookEventName"], "PreToolUse");
    assert_eq!(decision["permissionDecision"], "deny");
    let reason = decision["permissionDecisionReason"].as_str().unwrap();
    // The developer is sent to the skill, in the same session — not out of it.
    assert!(reason.contains("`/invar:approve SHOP-214`"), "{reason}");
    assert!(reason.contains("starts Phase 2"), "{reason}");
    assert!(!reason.contains("terminal"), "{reason}");
}

#[test]
fn the_command_is_denied_however_the_shell_is_asked_to_run_it() {
    let project = Project::new(APPROVED);
    for command in [
        "bash -c 'invar plan approve SHOP-214'",
        "cd app && ./target/debug/invar plan approve SHOP-214",
        "echo y | invar plan approve SHOP-214",
    ] {
        guard(bash(&project.root(), command)).code(2);
    }
}

#[test]
fn every_other_shell_command_passes_silently() {
    let project = Project::new(APPROVED);
    for command in [
        "ls -la",
        "git status",
        "invar plan validate SHOP-214",
        "invar plan check SHOP-214",
        "invar plan approve --help",
    ] {
        guard(bash(&project.root(), command))
            .code(0)
            .stdout("")
            .stderr("");
    }
}

// --- allow ----------------------------------------------------------------------

#[test]
fn an_edit_to_a_draft_plan_passes_silently() {
    let project = Project::new(&APPROVED.replace("Status: approved", "Status: draft"));
    guard(edit(&project.root(), &project.path(SPEC)))
        .code(0)
        .stdout("")
        .stderr("");
}

#[test]
fn any_other_file_passes_silently() {
    let project = Project::new(APPROVED);
    for path in [
        project.path("app/Models/Order.php"),
        project.path(".invar/plans/SHOP-214-cancel/notes.md"),
        project.path(".invar/plans/SHOP-300-new/spec.md"),
        "/tmp/somewhere/else.txt".to_owned(),
    ] {
        guard(write(&project.root(), &path))
            .code(0)
            .stdout("")
            .stderr("");
    }
}

#[test]
fn the_guard_never_answers_allow() {
    // "allow" would skip the developer's own permission prompt.
    let project = Project::new(APPROVED);
    for stdin in [
        edit(&project.root(), &project.path("app/x.php")),
        bash(&project.root(), "ls"),
        bash(&project.root(), "invar plan approve SHOP-214"),
        "garbage".to_owned(),
    ] {
        guard(stdin).stdout(contains("\"allow\"").not());
    }
}

// --- broken input: allow, and say why --------------------------------------------

#[test]
fn malformed_input_allows_the_edit_and_reports_the_problem() {
    for (stdin, problem) in [
        ("", "no hook input on stdin"),
        ("not json", "the hook input is not valid JSON"),
        (
            "{\"tool_name\": \"Edit\", ",
            "the hook input is not valid JSON",
        ),
        ("{}", "has no `tool_input.file_path`"),
        // `Bash` is judged by its command (above); any other tool without a
        // path is reported, and so is a `Bash` call without a command.
        (
            "{\"tool_name\":\"Grep\",\"tool_input\":{\"pattern\":\"approve\"}}",
            "for Grep has no `tool_input.file_path`",
        ),
        (
            "{\"tool_name\":\"Bash\",\"tool_input\":{}}",
            "for Bash has no `tool_input.command`",
        ),
    ] {
        let assert = guard(stdin)
            .code(0)
            .stderr(contains(problem).and(contains("the tool call is allowed")));
        let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
        let output: Value = serde_json::from_str(&stdout)
            .unwrap_or_else(|err| panic!("{stdin:?}: stdout is not JSON ({err}): {stdout}"));
        let message = output["systemMessage"].as_str().unwrap();
        assert!(message.starts_with("invar plan guard: "), "{message}");
        assert!(message.contains(problem), "{stdin:?}: {message}");
        assert!(output.get("hookSpecificOutput").is_none(), "{stdout}");
    }
}

#[test]
fn an_unreadable_plan_allows_the_edit_and_reports_the_problem() {
    let project = Project::new(APPROVED);
    fs::write(
        project.root().join(SPEC),
        b"---\nStatus: approved\xff\n---\n",
    )
    .unwrap();
    guard(edit(&project.root(), &project.path(SPEC)))
        .code(0)
        .stdout(contains("systemMessage").and(contains("not valid UTF-8")));
}

// --- the help says what the guard is not --------------------------------------------

#[test]
fn the_help_says_the_checksum_is_the_boundary() {
    Command::cargo_bin("invar")
        .unwrap()
        .args(["plan", "guard", "--help"])
        .assert()
        .success()
        .stdout(contains(
            "The guard is a convenience; the checksum is the boundary",
        ))
        .stdout(contains("never sees a file changed through `Bash`"))
        .stdout(contains("invar plan check"));
}

#[test]
fn the_help_says_a_bash_call_is_read_by_its_command() {
    Command::cargo_bin("invar")
        .unwrap()
        .args(["plan", "guard", "--help"])
        .assert()
        .success()
        .stdout(contains("tool_input.command"));
}

#[test]
fn a_hook_started_with_arguments_this_binary_does_not_know_is_not_a_denial() {
    // A plugin written for a newer or an older `invar`. Exit 2 would make
    // Claude Code block every Write, Edit and Bash call; a usage error must
    // never look like a verdict, even when the payload piped in would be one.
    let project = Project::new(APPROVED);
    for args in [
        &["plan", "guard", "--no-such-flag"][..],
        &["plan", "guard-v2"],
    ] {
        Command::cargo_bin("invar")
            .unwrap()
            .args(args)
            .write_stdin(edit(&project.root(), &project.path(SPEC)))
            .assert()
            .code(1)
            .stdout("");
    }
}
