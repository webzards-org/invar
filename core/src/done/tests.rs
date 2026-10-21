//! Tests for `invar done`: delete the Plan, refuse what would lose something.
//! Staging, which needs a real repository, is tested end to end
//! (`tests/done.rs`).

use std::fs;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use super::{Closed, DoneError, Staged, plan_for_branch};
use crate::format::plan::Mode;
use crate::fs::ProjectRoot;

/// `done` without `--commit`.
fn close(root: &ProjectRoot, spec: &Path, mode: Mode) -> Result<Closed, DoneError> {
    super::close(root, spec, mode, None)
}

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

#[test]
fn deletes_the_plan_and_writes_nothing_else() {
    let (_dir, root, spec) = project(&approved_plan());
    let plan_dir = spec.parent().unwrap().to_owned();

    let closed = close(&root, &spec, Mode::Light).unwrap();

    assert_eq!(closed.plan_dir, plan_dir);
    assert!(!plan_dir.exists());
    // Not a git repository here: nothing to stage, and said so.
    assert!(
        matches!(closed.staged, Staged::NotARepository(_)),
        "{:?}",
        closed.staged
    );
    // No entry, no PR description.
    assert!(!root.decisions_dir().exists());
    assert!(!root.out_dir().exists());
}

#[test]
fn a_draft_plan_refuses_and_changes_nothing() {
    let (_dir, root, spec) = project(&contract_example("9.1"));

    let err = close(&root, &spec, Mode::Light).unwrap_err();

    assert!(matches!(err, DoneError::Check(_)), "{err}");
    assert!(err.to_string().contains("is still a draft"), "{err}");
    assert!(spec.exists());
}

#[test]
fn a_changed_plan_refuses_and_changes_nothing() {
    let (_dir, root, spec) = project(&approved_plan());
    let edited = fs::read_to_string(&spec)
        .unwrap()
        .replace("refused with 403", "refused with 409");
    fs::write(&spec, edited).unwrap();

    let err = close(&root, &spec, Mode::Light).unwrap_err();

    assert!(
        err.to_string().contains("changed after it was approved"),
        "{err}"
    );
    assert!(spec.exists());
}

#[test]
fn an_unapproved_draft_entry_refuses_and_keeps_both() {
    // The reviewed text would go with the Plan: the one way to lose it
    // silently.
    let (_dir, root, spec) = project(&approved_plan());
    let draft = spec.with_file_name("decision.md");
    fs::write(&draft, "---\nticket: SHOP-214\n---\n\nKept.\n").unwrap();

    let err = close(&root, &spec, Mode::Light).unwrap_err();

    assert!(matches!(err, DoneError::DraftPending { .. }), "{err}");
    let message = err.to_string();
    assert!(
        message.contains("Approve it with /invar:log-approve, or delete it"),
        "{message}"
    );
    assert!(spec.exists());
    assert!(draft.exists());
}

#[test]
fn a_decision_entry_is_not_required() {
    // Before decision-entry.md 1.3.0, `done` wrote one; now it neither writes
    // nor looks for one.
    let (_dir, root, spec) = project(&approved_plan());
    assert!(!root.decisions_dir().exists());
    close(&root, &spec, Mode::Light).unwrap();
    assert!(!root.decisions_dir().exists());
}

#[cfg(unix)]
#[test]
fn a_plan_that_cannot_be_removed_is_reported() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    let probe = tempfile::NamedTempFile::new().unwrap();
    if probe.as_file().metadata().unwrap().uid() == 0 {
        eprintln!("skipped: running as root, permissions are not enforced");
        return;
    }

    let (_dir, root, spec) = project(&approved_plan());
    let plan_dir = spec.parent().unwrap().to_owned();
    let plans_dir = root.plans_dir();
    // Removing a directory needs write permission on its *parent*.
    fs::set_permissions(&plans_dir, fs::Permissions::from_mode(0o500)).unwrap();

    let result = close(&root, &spec, Mode::Light);

    fs::set_permissions(&plans_dir, fs::Permissions::from_mode(0o700)).unwrap();

    match result {
        Err(DoneError::Remove {
            plan_dir: reported, ..
        }) => assert_eq!(reported, plan_dir),
        other => panic!("expected Remove, got {other:?}"),
    }
    assert!(plan_dir.exists());
}

// --- `--commit`, refused before anything is deleted -----------------------------------

#[test]
fn an_empty_commit_message_refuses_and_changes_nothing() {
    let (_dir, root, spec) = project(&approved_plan());
    for message in ["", "   ", "\n"] {
        let err = super::close(&root, &spec, Mode::Light, Some(message)).unwrap_err();
        assert!(matches!(err, DoneError::EmptyCommitMessage), "{err}");
        assert!(spec.exists());
    }
}

#[test]
fn commit_outside_a_repository_refuses_and_changes_nothing() {
    // Not a git repository: there is nothing to commit into, so the Plan is
    // not deleted only to fail afterwards.
    let (_dir, root, spec) = project(&approved_plan());
    let err = super::close(&root, &spec, Mode::Light, Some("chore: remove")).unwrap_err();
    assert!(
        matches!(err, DoneError::CommitOutsideRepository { .. }),
        "{err}"
    );
    assert!(err.to_string().contains("nothing was changed"), "{err}");
    assert!(spec.exists());
}

// --- which Plan, from the branch -------------------------------------------------

fn plans(names: &[&str]) -> (TempDir, ProjectRoot) {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("invar.yaml"), "").unwrap();
    let root = ProjectRoot::for_test(dir.path());
    for name in names {
        fs::create_dir_all(root.plans_dir().join(name)).unwrap();
    }
    (dir, root)
}

#[test]
fn the_branch_names_the_plan_by_its_ticket() {
    let (_dir, root) = plans(&["SHOP-214-cancel", "SHOP-300-refunds"]);
    for branch in ["feat/SHOP-214-cancel", "SHOP-214", "fix/SHOP-214_hotfix"] {
        assert_eq!(
            plan_for_branch(&root, branch).unwrap(),
            root.plans_dir().join("SHOP-214-cancel/spec.md"),
            "{branch}"
        );
    }
}

#[test]
fn a_ticket_that_is_only_part_of_a_segment_is_not_named() {
    // `SHOP-21` is not `SHOP-214`.
    let (_dir, root) = plans(&["SHOP-21-old"]);
    assert!(matches!(
        plan_for_branch(&root, "feat/SHOP-214-cancel"),
        Err(DoneError::BranchNamesNoPlan { .. })
    ));
}

#[test]
fn a_branch_that_names_no_plan_lists_the_plans() {
    let (_dir, root) = plans(&["SHOP-214-cancel"]);
    let err = plan_for_branch(&root, "main").unwrap_err();
    let message = err.to_string();
    assert!(message.contains("`main`"), "{message}");
    assert!(message.contains("SHOP-214-cancel"), "{message}");
    assert!(message.contains("invar done <TICKET>"), "{message}");
}

#[test]
fn a_branch_that_names_several_plans_is_never_guessed() {
    let (_dir, root) = plans(&["SHOP-214-cancel", "SHOP-300-refunds"]);
    match plan_for_branch(&root, "feat/SHOP-214-and-SHOP-300") {
        Err(DoneError::BranchNamesSeveral { plans, .. }) => {
            assert_eq!(plans, ["SHOP-214-cancel", "SHOP-300-refunds"]);
        }
        other => panic!("expected BranchNamesSeveral, got {other:?}"),
    }
}

#[test]
fn without_plans_the_branch_names_none() {
    let (_dir, root) = plans(&[]);
    let err = plan_for_branch(&root, "feat/SHOP-1-x").unwrap_err();
    assert!(err.to_string().contains("no Plans"), "{err}");
}
