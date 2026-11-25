//! `invar pr-body <TICKET> --from-approval` (#168): the Plan as it was at its last signed
//! approval on the branch, read from git, with who signed it. Real repositories, real
//! keys, the real `plan approve`.

mod common;

use std::fs;
use std::path::Path;

use common::{Project, fingerprint, stdout};
use predicates::prelude::*;
use predicates::str::contains;

const SPEC: &str = ".invar/plans/SHOP-214-cancel-unpaid-order/spec.md";
const SIGNED: &str = "approval:\n  signed: true\n";

fn draft() -> String {
    "---\nTicket: SHOP-214\nModule: orders\nStatus: draft\nApproved-by:\n\
     Approved-at:\nApproved-checksum:\n---\n\n\
     ## Ticket\nSHOP-214 — Cancel an unpaid order.\n\n\
     ## Intent\nCustomers cancel their own unpaid orders.\n\n\
     ## Scope\n`orders` only.\n\n\
     ## Open questions / answers\nNone were needed.\n\n\
     ## Rejected alternatives\nNone considered.\n\n\
     ## Acceptance criteria\n\n\
     - AC1: Given an unpaid order, when its customer cancels it, then it is cancelled.\n\n\
     ## Implementation context\n- AI_CONTEXT: controllers stay thin\n"
        .to_owned()
}

/// A Plan approved with a signature on the branch, at the commit returned.
fn approved() -> (Project, std::path::PathBuf, String) {
    let (project, key) = Project::signed(SIGNED);
    project.write(SPEC, &draft());
    project.commit_all("docs: plan SHOP-214");
    project
        .invar(&[
            "plan",
            "approve",
            "SHOP-214",
            "--commit",
            "chore: approve SHOP-214",
        ])
        .success();
    let commit = project.head();
    (project, key, commit)
}

fn from_approval(project: &Project) -> assert_cmd::assert::Assert {
    project.invar(&["pr-body", "SHOP-214", "--from-approval", "--base", "main"])
}

/// Compare with `tests/snapshots/<name>`, with the key's fingerprint and the commit
/// replaced: each test makes its own key and commits.
fn assert_snapshot(name: &str, actual: &str) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/snapshots")
        .join(name);
    if std::env::var_os("INVAR_BLESS_SNAPSHOTS").is_some() {
        fs::write(&path, actual).unwrap();
        return;
    }
    let expected = fs::read_to_string(&path).unwrap_or_else(|err| {
        panic!(
            "cannot read the snapshot {} ({err}); run with INVAR_BLESS_SNAPSHOTS=1 to write it",
            path.display()
        )
    });
    assert_eq!(actual, expected, "the description differs from {name}");
}

#[test]
fn the_signed_plan_renders_with_who_signed_it() {
    let (project, key, commit) = approved();
    let out = stdout(&from_approval(&project).success());
    let normalized = out
        .replace(&fingerprint(&key), "SHA256:<fingerprint>")
        .replace(&commit[..12], "<commit>");
    assert_snapshot("pr-body-shop-214-from-approval.md", &normalized);
}

#[test]
fn it_renders_what_was_signed_not_what_was_changed_after() {
    let (project, _, _) = approved();
    let spec = project.read(SPEC).unwrap();
    project.write(SPEC, &spec.replace("Customers cancel", "Anyone cancels"));
    project.commit_all("docs: widen SHOP-214");
    from_approval(&project)
        .success()
        .stdout(contains("Customers cancel their own unpaid orders."))
        .stdout(contains("Anyone cancels").not());
}

#[test]
fn it_works_after_the_plan_is_deleted() {
    let (project, _, _) = approved();
    project
        .invar(&["done", "SHOP-214", "--commit", "chore: close SHOP-214"])
        .success();
    assert_eq!(project.read(SPEC), None);
    from_approval(&project)
        .success()
        .stdout(contains("Signed by dev@example.com ("));
}

#[test]
fn without_a_signed_approval_it_is_refused_naming_why() {
    let (project, _) = Project::signed(SIGNED);
    project.write(SPEC, &draft().replace("Status: draft", "Status: approved"));
    project.commit_all("approved by hand");
    from_approval(&project)
        .failure()
        .stderr(contains(
            "no approval of SHOP-214 on this branch is signed by a key in main:.invar/approvers",
        ))
        .stderr(contains(
            "(approved by hand): is approved, and has no approval.sig",
        ));

    let (project, _) = Project::signed(SIGNED);
    project.write(SPEC, &draft());
    project.commit_all("docs: plan SHOP-214");
    from_approval(&project).failure().stderr(contains(
        "this branch has no approval of SHOP-214 signed by a key",
    ));
}

#[test]
fn with_signing_off_at_the_base_it_is_refused() {
    let project = Project::new("");
    project.write(SPEC, &draft());
    project.commit_all("docs: plan SHOP-214");
    from_approval(&project).failure().stderr(contains(
        "`--from-approval` renders the Plan a person signed, and the check is off",
    ));
}

#[test]
fn without_the_flag_nothing_changes() {
    let (project, _, _) = approved();
    project
        .invar(&["pr-body", "SHOP-214"])
        .success()
        .stdout(contains("Signed by").not());
}
