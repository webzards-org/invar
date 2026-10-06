//! Tests for `plan guard` (M1-10): deny only an approved Plan, and never fail
//! closed on input the guard does not understand.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::json;
use tempfile::TempDir;

use super::{Verdict, decide};

const APPROVED: &str = "---\nTicket: SHOP-214\nModule: orders\nStatus: approved\n\
    Approved-by: rade\nApproved-at: 2026-10-05\n\
    Approved-checksum: sha256:f3689f4ddfed1a683df95a45a12d36a7eaab8d49f6a077abfc5b5ecae13bff44\n\
    ---\n\n## Ticket\nSHOP-214\n";

fn draft() -> String {
    APPROVED
        .replace("Status: approved", "Status: draft")
        .replace("Approved-by: rade", "Approved-by:")
}

/// A project with one Plan, `text`, at `.invar/plans/SHOP-214-cancel/spec.md`.
fn project(text: &str) -> (TempDir, PathBuf, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    fs::write(root.join("invar.yaml"), "").unwrap();
    let spec = root.join(".invar/plans/SHOP-214-cancel/spec.md");
    fs::create_dir_all(spec.parent().unwrap()).unwrap();
    fs::write(&spec, text).unwrap();
    (dir, root, spec)
}

fn payload(tool: &str, file_path: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "session_id": "abc",
        "hook_event_name": "PreToolUse",
        "tool_name": tool,
        "tool_input": { "file_path": file_path, "old_string": "a", "new_string": "b" },
    }))
    .unwrap()
}

fn on(path: &Path) -> Verdict {
    decide(&payload("Edit", path.to_str().unwrap()), Path::new("/"))
}

fn assert_unsure(verdict: Verdict, needle: &str) {
    match verdict {
        Verdict::Unsure { problem } => assert!(problem.contains(needle), "{problem}"),
        other => panic!("expected Unsure, got {other:?}"),
    }
}

// --- deny -----------------------------------------------------------------------

#[test]
fn an_approved_plan_is_denied_with_what_to_do_instead() {
    let (_dir, _root, spec) = project(APPROVED);
    match on(&spec) {
        Verdict::Deny { reason } => {
            assert!(
                reason.starts_with(
                    ".invar/plans/SHOP-214-cancel/spec.md is the approved Plan for SHOP-214"
                ),
                "{reason}"
            );
            assert!(reason.contains("tell the developer"), "{reason}");
            assert!(
                reason.contains("`invar plan approve SHOP-214 --reapprove`"),
                "{reason}"
            );
        }
        other => panic!("expected Deny, got {other:?}"),
    }
}

#[test]
fn write_is_denied_like_edit() {
    let (_dir, _root, spec) = project(APPROVED);
    let input = payload("Write", spec.to_str().unwrap());
    assert!(matches!(
        decide(&input, Path::new("/")),
        Verdict::Deny { .. }
    ));
}

#[test]
fn an_approved_plan_with_crlf_or_a_broken_body_is_still_denied() {
    // Status is what matters: whether the approval still holds is
    // `plan check`'s question, not the guard's.
    let (_dir, _root, spec) = project(&APPROVED.replace('\n', "\r\n"));
    assert!(matches!(on(&spec), Verdict::Deny { .. }));
    fs::write(&spec, APPROVED.replace("## Ticket\n", "")).unwrap();
    assert!(matches!(on(&spec), Verdict::Deny { .. }));
}

#[test]
fn a_relative_path_is_resolved_against_the_payloads_cwd() {
    let (_dir, root, _spec) = project(APPROVED);
    let input = serde_json::to_vec(&json!({
        "cwd": root,
        "tool_name": "Edit",
        "tool_input": { "file_path": ".invar/plans/SHOP-214-cancel/spec.md" },
    }))
    .unwrap();
    assert!(matches!(
        decide(&input, Path::new("/")),
        Verdict::Deny { .. }
    ));
    // Without a `cwd` in the payload, the guard's own directory is used.
    let input = payload("Edit", "plans/SHOP-214-cancel/spec.md");
    assert!(matches!(
        decide(&input, &root.join(".invar")),
        Verdict::Deny { .. }
    ));
}

#[cfg(unix)]
#[test]
fn a_path_through_a_symlink_is_still_denied() {
    let (_dir, root, _spec) = project(APPROVED);
    let link = root.join("plan-link");
    std::os::unix::fs::symlink(root.join(".invar/plans/SHOP-214-cancel"), &link).unwrap();
    assert!(matches!(on(&link.join("spec.md")), Verdict::Deny { .. }));
}

// --- pass -----------------------------------------------------------------------

#[test]
fn a_draft_plan_passes() {
    let (_dir, _root, spec) = project(&draft());
    assert_eq!(on(&spec), Verdict::Pass);
}

#[test]
fn a_status_that_is_not_exactly_approved_passes() {
    // `Status: Approved` is not an approval (§5); `plan validate` reports it.
    let (_dir, _root, spec) = project(&APPROVED.replace("Status: approved", "Status: Approved"));
    assert_eq!(on(&spec), Verdict::Pass);
}

#[test]
fn every_other_file_passes() {
    let (_dir, root, _spec) = project(APPROVED);
    for other in [
        root.join("app/Http/OrderController.php"),
        root.join("invar.yaml"),
        root.join(".invar/plans/SHOP-214-cancel/notes.md"),
        root.join(".invar/plans/SHOP-214-cancel/drafts/spec.md"),
        root.join(".invar/spec.md"),
        root.join("spec.md"),
    ] {
        assert_eq!(on(&other), Verdict::Pass, "{}", other.display());
    }
}

#[test]
fn an_approved_looking_spec_md_elsewhere_in_the_project_passes() {
    // Only `.invar/plans/<name>/spec.md` is a Plan, whatever the file says.
    let (_dir, root, _spec) = project(APPROVED);
    for elsewhere in [
        root.join("docs/examples/spec.md"),
        root.join(".invar/plans/SHOP-214-cancel/old/spec.md"),
        root.join(".invar/archive/SHOP-1-x/spec.md"),
    ] {
        fs::create_dir_all(elsewhere.parent().unwrap()).unwrap();
        fs::write(&elsewhere, APPROVED).unwrap();
        assert_eq!(on(&elsewhere), Verdict::Pass, "{}", elsewhere.display());
    }
}

#[test]
fn a_new_plan_being_written_passes() {
    let (_dir, root, _spec) = project(APPROVED);
    let new = root.join(".invar/plans/SHOP-300-refunds/spec.md");
    assert_eq!(on(&new), Verdict::Pass);
}

#[test]
fn a_file_outside_any_project_passes() {
    let dir = tempfile::tempdir().unwrap();
    let elsewhere = dir.path().join(".invar/plans/SHOP-1-x/spec.md");
    fs::create_dir_all(elsewhere.parent().unwrap()).unwrap();
    fs::write(&elsewhere, APPROVED).unwrap();
    // No `invar.yaml` above it: not a Plan, whatever its path looks like.
    assert_eq!(on(&elsewhere), Verdict::Pass);
}

// --- unsure: allow, and say why -------------------------------------------------

#[test]
fn input_that_is_not_a_payload_is_reported_and_allowed() {
    let cwd = Path::new("/");
    assert_unsure(decide(b"", cwd), "no hook input");
    assert_unsure(decide(b"  \n", cwd), "no hook input");
    assert_unsure(decide(b"not json", cwd), "not valid JSON");
    assert_unsure(decide(b"{\"tool_name\":", cwd), "not valid JSON");
    assert_unsure(decide(b"[1, 2]", cwd), "no `tool_input.file_path`");
    assert_unsure(decide(b"{}", cwd), "no `tool_input.file_path`");
    assert_unsure(
        decide(
            br#"{"tool_name":"Bash","tool_input":{"command":"ls"}}"#,
            cwd,
        ),
        "for Bash has no `tool_input.file_path`",
    );
    assert_unsure(
        decide(
            br#"{"tool_name":"Edit","tool_input":{"file_path":42}}"#,
            cwd,
        ),
        "no `tool_input.file_path`",
    );
    assert_unsure(
        decide(
            br#"{"tool_name":"Edit","tool_input":{"file_path":""}}"#,
            cwd,
        ),
        "no `tool_input.file_path`",
    );
}

#[test]
fn a_plan_that_cannot_be_read_is_reported_and_allowed() {
    let (_dir, _root, spec) = project(APPROVED);
    fs::write(&spec, b"---\nStatus: approved\xff\n---\n").unwrap();
    assert_unsure(on(&spec), "not valid UTF-8");
}

#[test]
fn unknown_payload_fields_are_ignored() {
    let (_dir, _root, spec) = project(APPROVED);
    let input = serde_json::to_vec(&json!({
        "tool_name": "Edit",
        "tool_input": { "file_path": spec, "something_new": [1, 2, 3] },
        "effort": { "level": "medium" },
        "a_field_from_the_future": true,
    }))
    .unwrap();
    assert!(matches!(
        decide(&input, Path::new("/")),
        Verdict::Deny { .. }
    ));
}
