//! Tests for `invar done` (M1-12): all-or-nothing closing of a cycle.

use std::fs;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use super::{DoneError, Request, close};
use crate::format::decision::Type;
use crate::format::plan::Mode;
use crate::fs::ProjectRoot;
use crate::git::Instant;

fn contract_example(number: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../contracts/formats/plan-header.md");
    let text = fs::read_to_string(path).unwrap();
    let after = text.split_once(&format!("### {number} ")).unwrap().1;
    let body = after.split_once("```markdown\n").unwrap().1;
    body.split_once("\n```\n").unwrap().0.to_owned() + "\n"
}

/// Example 9.1's body under example 9.2's header: the contract's one
/// approved Plan.
fn approved_plan() -> String {
    let body = contract_example("9.1")
        .split_once("\n---\n")
        .unwrap()
        .1
        .to_owned();
    contract_example("9.2") + &body
}

fn project(text: &str) -> (TempDir, ProjectRoot, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("invar.yaml"), "").unwrap();
    let root = ProjectRoot::for_test(dir.path());
    let plan_dir = root.plans_dir().join("SHOP-214-cancel-unpaid-order");
    fs::create_dir_all(&plan_dir).unwrap();
    let spec = plan_dir.join("spec.md");
    fs::write(&spec, text).unwrap();
    (dir, root, spec)
}

fn now() -> Instant {
    Instant::from_ident("t <t@x> 1791304632 +0200").unwrap()
}

fn request(spec: &Path, message: &str) -> Request {
    Request {
        spec: spec.to_owned(),
        body: message.to_owned(),
        with_ticket: false,
    }
}

#[test]
fn closes_the_cycle_all_the_way() {
    let (_dir, root, spec) = project(&approved_plan());
    let plan_dir = spec.parent().unwrap().to_owned();

    let closed = close(
        &root,
        request(&spec, "Shipped as planned."),
        Mode::Light,
        now(),
    )
    .unwrap();

    assert_eq!(closed.pr_description, root.out_dir().join("SHOP-214-pr.md"));
    let rendered = fs::read_to_string(&closed.pr_description).unwrap();
    assert!(rendered.starts_with("# SHOP-214"), "{rendered}");
    assert!(
        rendered.contains("Given a pending, unpaid order of mine"),
        "{rendered}"
    );

    assert!(closed.entry.path.is_file());
    assert_eq!(closed.entry.entry.ticket, "SHOP-214");
    assert_eq!(closed.entry.entry.module, "orders");
    assert_eq!(closed.entry.entry.kind, Type::Confirmed);
    assert_eq!(closed.entry.entry.body, "Shipped as planned.");

    assert_eq!(closed.plan_dir, plan_dir);
    assert!(!plan_dir.exists());
}

#[test]
fn a_draft_plan_refuses_and_writes_nothing() {
    let (_dir, root, spec) = project(&contract_example("9.1"));

    let err = close(&root, request(&spec, "x"), Mode::Light, now()).unwrap_err();

    assert!(matches!(err, DoneError::Check(_)), "{err}");
    assert!(err.to_string().contains("is still a draft"), "{err}");
    assert!(!root.out_dir().exists());
    assert!(!root.decisions_dir().exists());
    assert!(spec.exists());
}

#[test]
fn a_changed_plan_refuses_and_writes_nothing() {
    let (_dir, root, spec) = project(&approved_plan());
    let edited = fs::read_to_string(&spec)
        .unwrap()
        .replace("refused with 403", "refused with 409");
    fs::write(&spec, edited).unwrap();

    let err = close(&root, request(&spec, "x"), Mode::Light, now()).unwrap_err();

    assert!(
        err.to_string().contains("changed after it was approved"),
        "{err}"
    );
    assert!(!root.out_dir().exists());
    assert!(!root.decisions_dir().exists());
}

#[test]
fn a_blocked_pr_description_leaves_no_entry_and_the_plan_in_place() {
    let (_dir, root, spec) = project(&approved_plan());
    let plan_dir = spec.parent().unwrap().to_owned();
    // A file sits where `.invar/out/` needs to be a directory.
    fs::create_dir_all(root.path().join(".invar")).unwrap();
    fs::write(root.out_dir(), "").unwrap();

    let err = close(&root, request(&spec, "x"), Mode::Light, now()).unwrap_err();

    assert!(matches!(err, DoneError::Io { .. }), "{err}");
    assert!(!root.decisions_dir().exists());
    assert!(plan_dir.exists());
}

#[test]
fn a_blocked_decision_log_leaves_the_pr_description_but_not_the_plan_removed() {
    let (_dir, root, spec) = project(&approved_plan());
    let plan_dir = spec.parent().unwrap().to_owned();
    fs::create_dir_all(root.path().join(".invar")).unwrap();
    fs::write(root.decisions_dir(), "").unwrap();

    let err = close(&root, request(&spec, "x"), Mode::Light, now()).unwrap_err();

    assert!(matches!(err, DoneError::Record(_)), "{err}");
    assert!(root.out_dir().join("SHOP-214-pr.md").is_file());
    assert!(plan_dir.exists());
}

#[cfg(unix)]
#[test]
fn a_plan_that_cannot_be_removed_still_has_its_entry_on_record() {
    use std::os::unix::fs::PermissionsExt;

    if is_root() {
        eprintln!("skipped: running as root, permissions are not enforced");
        return;
    }

    let (_dir, root, spec) = project(&approved_plan());
    let plan_dir = spec.parent().unwrap().to_owned();
    let plans_dir = root.plans_dir();
    // Removing a directory needs write permission on its *parent*.
    fs::set_permissions(&plans_dir, fs::Permissions::from_mode(0o500)).unwrap();

    let result = close(&root, request(&spec, "x"), Mode::Light, now());

    fs::set_permissions(&plans_dir, fs::Permissions::from_mode(0o700)).unwrap();

    match result {
        Err(DoneError::RemovePlan {
            entry,
            plan_dir: reported,
            ..
        }) => {
            assert!(entry.is_file(), "{entry:?}");
            assert_eq!(reported, plan_dir);
        }
        other => panic!("expected RemovePlan, got {other:?}"),
    }
    assert!(plan_dir.exists());
}

#[cfg(unix)]
fn is_root() -> bool {
    use std::os::unix::fs::MetadataExt;
    let probe = tempfile::NamedTempFile::new().unwrap();
    probe.as_file().metadata().unwrap().uid() == 0
}
