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

// --- locked acceptance tests ------------------------------------------------------

const ACCEPTANCE: &str = "acceptance:\n  dir: tests/Acceptance\n  \
    run: [\"pest\", \"{report}\", \"{files}\"]\n";
const TEST: &str = "tests/Acceptance/sub/cancel_test.php";
/// Example 8.2's digest, of what [`LOCKED`] covers.
const DIGEST: &str = "sha256:5d5da6ccd3116bee608b2a6c15d4c9f0d20f32b17b220cc13a643169ab874e07";
const LOCKED: &str = "# invar-lock: SHOP-214 sha256:5d5da6ccd3116bee608b2a6c15d4c9f0d20f32b17b220cc13a643169ab874e07\ndef test_ac1():\n    assert False\n";
const PENDING: &str = "# invar-lock: SHOP-214\ndef test_ac1():\n    assert False\n";

/// A project with an `acceptance:` section, and `file` holding `text`.
fn with_test(file: &str, text: &str) -> Project {
    let project = Project::new(APPROVED);
    fs::write(project.root().join("invar.yaml"), ACCEPTANCE).unwrap();
    let path = project.root().join(file);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
    project
}

fn passes_silently(stdin: String) {
    guard(stdin).code(0).stdout("").stderr("");
}

/// Let through, with the problem reported, as an unreadable Plan is.
fn passes_with_problem(stdin: String, problem: &str) {
    guard(stdin).code(0).stderr(contains(problem));
}

fn denied_as_locked(stdin: String) {
    guard(stdin)
        .code(2)
        .stderr(contains(format!(
            "{TEST} is a locked acceptance test of SHOP-214"
        )))
        .stderr(contains("BLOCKED-TEST <file> <AC> <reason>"))
        .stderr(contains("`## Acceptance tests to change`"))
        .stderr(contains("invar acceptance lock"));
}

#[test]
fn an_edit_and_a_write_to_a_locked_test_are_denied() {
    let project = with_test(TEST, LOCKED);
    denied_as_locked(edit(&project.root(), &project.path(TEST)));
    denied_as_locked(write(&project.root(), &project.path(TEST)));
    // A relative path, as the payload may carry one.
    denied_as_locked(edit(&project.root(), TEST));
}

#[test]
fn a_locked_test_whose_digest_no_longer_matches_is_still_denied() {
    let project = with_test(TEST, &LOCKED.replace("assert False", "assert True"));
    denied_as_locked(edit(&project.root(), &project.path(TEST)));
}

#[test]
fn a_test_locked_by_another_ticket_is_denied_naming_that_ticket() {
    let project = with_test(TEST, &LOCKED.replace("SHOP-214", "SHOP-1"));
    guard(edit(&project.root(), &project.path(TEST)))
        .code(2)
        .stderr(contains("a locked acceptance test of SHOP-1."));
}

#[test]
fn a_pending_test_a_file_with_no_line_and_a_new_file_pass() {
    let project = with_test(TEST, PENDING);
    passes_silently(edit(&project.root(), &project.path(TEST)));
    passes_silently(write(&project.root(), &project.path(TEST)));

    let project = with_test(TEST, "def helper():\n    pass\n");
    passes_silently(edit(&project.root(), &project.path(TEST)));

    let project = with_test(TEST, PENDING);
    passes_silently(write(
        &project.root(),
        &project.path("tests/Acceptance/new/test.php"),
    ));
}

#[test]
fn a_binary_file_with_no_marker_passes() {
    let project = with_test(TEST, PENDING);
    fs::write(project.root().join(TEST), [0xff, 0xfe, 0x00]).unwrap();
    passes_silently(edit(&project.root(), &project.path(TEST)));
}

#[test]
fn a_locked_line_outside_the_directory_passes() {
    for file in [
        "contracts/formats/acceptance-lock.md",
        "tests/AcceptanceOther/test.php",
        "tests/test.php",
    ] {
        let project = with_test(file, LOCKED);
        passes_silently(edit(&project.root(), &project.path(file)));
    }
}

#[test]
fn with_no_section_a_locked_line_passes() {
    let project = with_test(TEST, LOCKED);
    fs::write(project.root().join("invar.yaml"), "").unwrap();
    passes_silently(edit(&project.root(), &project.path(TEST)));
}

#[test]
fn a_directory_written_with_a_dot_is_the_same_directory() {
    let project = with_test(TEST, LOCKED);
    fs::write(
        project.root().join("invar.yaml"),
        ACCEPTANCE.replace("dir: tests/Acceptance", "dir: ./tests//Acceptance"),
    )
    .unwrap();
    denied_as_locked(edit(&project.root(), &project.path(TEST)));
}

#[cfg(unix)]
#[test]
fn a_path_through_a_symlink_is_judged_where_it_leads() {
    let project = with_test(TEST, LOCKED);
    let link = project.root().join("link.php");
    std::os::unix::fs::symlink(project.root().join(TEST), &link).unwrap();
    denied_as_locked(edit(&project.root(), &link.display().to_string()));
}

#[test]
fn an_unreadable_config_passes_with_the_problem_reported_and_the_plan_stays_guarded() {
    let project = with_test(TEST, LOCKED);
    fs::write(
        project.root().join("invar.yaml"),
        "acceptance: [not, a, mapping\n",
    )
    .unwrap();
    passes_with_problem(
        edit(&project.root(), &project.path(TEST)),
        "cannot tell whether tests/Acceptance/sub/cancel_test.php is a locked acceptance test",
    );
    // The Plan never depended on the config.
    guard(edit(&project.root(), &project.path(SPEC))).code(2);
}

#[test]
fn a_malformed_line_in_the_target_passes_with_the_problem_reported() {
    for text in [
        "# invar-lock: shop-214\n",
        "# invar-lock: SHOP-214 sha256:abc\n",
        "# invar-lock: SHOP-1\n# invar-lock: SHOP-2\n",
    ] {
        let project = with_test(TEST, text);
        passes_with_problem(edit(&project.root(), &project.path(TEST)), TEST);
    }
    // Not UTF-8, with the marker in it: whether it is locked cannot be read.
    let project = with_test(TEST, PENDING);
    let mut bytes = PENDING.as_bytes().to_vec();
    bytes.push(0xff);
    fs::write(project.root().join(TEST), bytes).unwrap();
    passes_with_problem(edit(&project.root(), &project.path(TEST)), TEST);
}

#[test]
fn a_tool_the_guard_does_not_know_is_judged_for_the_plan_only() {
    let project = with_test(TEST, LOCKED);
    let call = |file: &str| {
        json!({
            "cwd": project.root(),
            "tool_name": "NotebookEdit",
            "tool_input": { "file_path": file },
        })
        .to_string()
    };
    passes_silently(call(&project.path(TEST)));
    guard(call(&project.path(SPEC))).code(2);
}

#[test]
fn a_change_through_bash_gets_past_the_guard_and_is_caught_by_the_digest() {
    let project = with_test(TEST, LOCKED);
    passes_silently(bash(
        &project.root(),
        &format!("sed -i s/False/True/ {TEST}"),
    ));
    fs::write(
        project.root().join(TEST),
        LOCKED.replace("assert False", "assert True"),
    )
    .unwrap();
    Command::cargo_bin("invar")
        .unwrap()
        .current_dir(project.root())
        .args(["acceptance", "verify"])
        .assert()
        .failure()
        .stderr(contains("changed after it was locked"))
        .stderr(contains(TEST))
        .stderr(contains(DIGEST));
}

// --- a digest written by hand --------------------------------------------------------

fn write_with(cwd: &Path, file_path: &str, content: &str) -> String {
    json!({
        "cwd": cwd,
        "hook_event_name": "PreToolUse",
        "tool_name": "Write",
        "tool_input": { "file_path": file_path, "content": content },
    })
    .to_string()
}

fn edit_with(cwd: &Path, file_path: &str, old_string: &str, new_string: &str) -> String {
    json!({
        "cwd": cwd,
        "hook_event_name": "PreToolUse",
        "tool_name": "Edit",
        "tool_input": {
            "file_path": file_path,
            "old_string": old_string,
            "new_string": new_string,
            "replace_all": false
        },
    })
    .to_string()
}

fn denied_as_hand_written(stdin: String) {
    guard(stdin)
        .code(2)
        .stderr(contains("this would write a digest"))
        .stderr(contains(TEST))
        .stderr(contains("Only `invar acceptance lock` writes a digest"))
        .stderr(contains("run the tests"));
}

#[test]
fn a_write_holding_a_digest_is_denied_on_a_new_file_and_on_a_pending_one() {
    let project = with_test(TEST, PENDING);
    denied_as_hand_written(write_with(&project.root(), &project.path(TEST), LOCKED));

    let project = with_test("tests/Acceptance/other.php", PENDING);
    denied_as_hand_written(write_with(&project.root(), &project.path(TEST), LOCKED));
}

#[test]
fn an_edit_whose_new_string_holds_a_digest_is_denied() {
    let project = with_test(TEST, PENDING);
    denied_as_hand_written(edit_with(
        &project.root(),
        &project.path(TEST),
        "# invar-lock: SHOP-214",
        &format!("# invar-lock: SHOP-214 {DIGEST}"),
    ));
}

#[test]
fn a_malformed_digest_field_is_denied_too() {
    let project = with_test(TEST, PENDING);
    denied_as_hand_written(edit_with(
        &project.root(),
        &project.path(TEST),
        "# invar-lock: SHOP-214",
        "# invar-lock: SHOP-214 sha256:abc",
    ));
    denied_as_hand_written(write_with(
        &project.root(),
        &project.path(TEST),
        "# invar-lock: SHOP-214 sha256:abc\n",
    ));
}

#[test]
fn an_edit_that_adds_the_digest_without_repeating_the_marker_is_denied() {
    let project = with_test(TEST, PENDING);
    denied_as_hand_written(edit_with(
        &project.root(),
        &project.path(TEST),
        "SHOP-214",
        &format!("SHOP-214 {DIGEST}"),
    ));
}

#[test]
fn a_line_past_the_window_in_new_content_is_still_a_digest() {
    let project = with_test(TEST, PENDING);
    let content = format!("{}# invar-lock: SHOP-214 {DIGEST}\n", "x\n".repeat(20));
    denied_as_hand_written(write_with(&project.root(), &project.path(TEST), &content));
}

#[test]
fn new_content_with_only_a_pending_line_passes() {
    let project = with_test(TEST, PENDING);
    passes_silently(write_with(&project.root(), &project.path(TEST), PENDING));
    passes_silently(edit_with(
        &project.root(),
        &project.path(TEST),
        "assert False",
        "assert 1 == 2",
    ));
    passes_silently(edit_with(
        &project.root(),
        &project.path(TEST),
        "# invar-lock: SHOP-214",
        "// invar-lock: SHOP-214",
    ));
    passes_silently(write_with(
        &project.root(),
        &project.path("tests/Acceptance/new.php"),
        PENDING,
    ));
}

#[test]
fn a_digest_outside_the_directory_or_with_no_section_passes() {
    let project = with_test(TEST, PENDING);
    let outside = project.path("contracts/formats/acceptance-lock.md");
    passes_silently(write_with(&project.root(), &outside, LOCKED));
    passes_silently(edit_with(&project.root(), &outside, "a", LOCKED));

    fs::write(project.root().join("invar.yaml"), "").unwrap();
    passes_silently(write_with(&project.root(), &project.path(TEST), LOCKED));
}

#[test]
fn a_locked_file_gets_the_reason_about_the_lock_whatever_the_content() {
    let project = with_test(TEST, LOCKED);
    denied_as_locked(write_with(&project.root(), &project.path(TEST), LOCKED));
}

#[test]
fn a_target_in_doubt_stays_a_doubt_unless_the_call_itself_writes_a_digest() {
    // A malformed digest already there: an edit elsewhere, or one that fixes it, is let
    // through with the problem reported.
    let project = with_test(TEST, "# invar-lock: SHOP-214 sha256:abc\nassert False\n");
    passes_with_problem(
        edit_with(
            &project.root(),
            &project.path(TEST),
            "assert False",
            "assert 1 == 2",
        ),
        TEST,
    );
    passes_with_problem(
        edit_with(
            &project.root(),
            &project.path(TEST),
            "# invar-lock: SHOP-214 sha256:abc",
            "# invar-lock: SHOP-214",
        ),
        TEST,
    );
    // What this call writes is known, whatever the file holds.
    denied_as_hand_written(edit_with(
        &project.root(),
        &project.path(TEST),
        "# invar-lock: SHOP-214 sha256:abc",
        &format!("# invar-lock: SHOP-214 {DIGEST}"),
    ));
}

#[test]
fn a_tool_the_guard_does_not_know_is_not_judged_for_a_digest() {
    let project = with_test(TEST, PENDING);
    passes_silently(
        json!({
            "cwd": project.root(),
            "tool_name": "NotebookEdit",
            "tool_input": { "file_path": project.path(TEST), "content": LOCKED },
        })
        .to_string(),
    );
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

#[test]
fn the_test_writers_command_passes_silently() {
    // The agent's command, and so not on `HUMAN_ONLY`.
    let project = Project::new(APPROVED);
    guard(bash(&project.root(), "invar acceptance spec SHOP-214"))
        .code(0)
        .stdout("")
        .stderr("");
}

#[test]
fn the_test_writers_lock_passes_silently() {
    // Its own step, and so not on `HUMAN_ONLY`. `--relock` is the developer's (AT-13).
    let project = Project::new(APPROVED);
    guard(bash(&project.root(), "invar acceptance lock SHOP-214"))
        .code(0)
        .stdout("")
        .stderr("");
}

#[test]
fn verifying_the_locks_passes_silently() {
    // An agent's command, with or without a ticket, and so not on `HUMAN_ONLY`.
    let project = Project::new(APPROVED);
    for command in [
        "invar acceptance verify",
        "invar acceptance verify SHOP-214",
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
