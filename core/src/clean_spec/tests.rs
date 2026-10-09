//! Tests for `clean-spec` (M1-13): which Plans are eligible and why, and that
//! deleting reaches nothing but the Plan directories it was asked for.
//!
//! No git here: the branches merged into the base are a plain list, as the CLI
//! hands them over. The branch queries are tested against a real repository in
//! `git/tests.rs`, and the whole command in `tests/clean_spec.rs`.

use std::fs;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use super::{Assessment, CleanError, Item, Keep, Scope, Verdict, assess, delete};
use crate::format::decision::{Entry, FileName, Type};
use crate::format::plan::{Mode, Status};
use crate::fs::ProjectRoot;
use crate::fs::plans::LookupError;

// --- fixtures ----------------------------------------------------------------

fn contract_example(number: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../contracts/formats/plan-header.md");
    let text = fs::read_to_string(path).unwrap();
    let after = text.split_once(&format!("### {number} ")).unwrap().1;
    let body = after.split_once("```markdown\n").unwrap().1;
    body.split_once("\n```\n").unwrap().0.to_owned() + "\n"
}

/// Example 9.1: a draft Plan for SHOP-214.
fn draft() -> String {
    contract_example("9.1")
}

/// Example 9.1's body under example 9.2's header: the approved Plan.
fn approved() -> String {
    let body = draft().split_once("\n---\n").unwrap().1.to_owned();
    contract_example("9.2") + &body
}

/// `text`, written for another ticket.
fn for_ticket(text: &str, ticket: &str) -> String {
    text.replace("SHOP-214", ticket)
}

const CANCEL: &str = "SHOP-214-cancel-unpaid-order";

fn project() -> (TempDir, ProjectRoot) {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("invar.yaml"), "").unwrap();
    let root = ProjectRoot::for_test(dir.path());
    (dir, root)
}

/// A Plan directory `name` holding `text` as its `spec.md`.
fn add_plan(root: &ProjectRoot, name: &str, text: &str) -> PathBuf {
    let dir = root.plans_dir().join(name);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("spec.md"), text).unwrap();
    dir
}

/// A decision entry for `ticket`, written the way the core writes one, at
/// 16:37:`second` UTC. Returns its id.
fn add_entry(root: &ProjectRoot, ticket: &str, kind: Type, second: u32) -> String {
    let id = format!("d-{:08x}", 0xc0de_0000 + second);
    let entry = Entry {
        id: id.clone(),
        date: format!("2026-10-06T18:37:{second:02}+02:00"),
        ticket: ticket.to_owned(),
        module: "orders".to_owned(),
        kind,
        scenario: None,
        body: "What happened.".to_owned(),
    };
    let name = FileName {
        stamp: format!("2026-10-06-1637{second:02}"),
        module: "orders".to_owned(),
        slug: ticket.to_lowercase(),
    };
    fs::create_dir_all(root.decisions_dir()).unwrap();
    fs::write(root.decisions_dir().join(name.render(1)), entry.render()).unwrap();
    id
}

fn everything(root: &ProjectRoot) -> Assessment {
    assess(
        root,
        Scope {
            ticket: None,
            merged: None,
        },
        Mode::Light,
    )
    .unwrap()
}

fn item<'a>(assessment: &'a Assessment, name: &str) -> &'a Item {
    assessment
        .items
        .iter()
        .find(|item| item.name == name)
        .unwrap_or_else(|| panic!("no item {name}: {assessment:#?}"))
}

fn is_kept(assessment: &Assessment, name: &str, why: Keep) -> bool {
    item(assessment, name).verdict == Verdict::Keep(why)
}

fn is_deleted(assessment: &Assessment, name: &str) -> bool {
    matches!(item(assessment, name).verdict, Verdict::Delete { .. })
}

// --- the decision entry -------------------------------------------------------

#[test]
fn a_plan_with_a_decision_entry_is_eligible_and_names_it() {
    let (_dir, root) = project();
    add_plan(&root, CANCEL, &draft());
    let id = add_entry(&root, "SHOP-214", Type::Note, 12);

    let found = everything(&root);

    match &item(&found, CANCEL).verdict {
        Verdict::Delete { entry, branch } => {
            assert_eq!(entry.id, id);
            assert_eq!(entry.kind, Type::Note);
            assert_eq!(entry.date, "2026-10-06T18:37:12+02:00");
            assert_eq!(*branch, None);
        }
        other => panic!("expected Delete, got {other:?}"),
    }
    assert_eq!(item(&found, CANCEL).ticket.as_deref(), Some("SHOP-214"));
    assert_eq!(item(&found, CANCEL).status, Some(Status::Draft));
    assert_eq!(found.eligible(), 1);
}

#[test]
fn a_plan_without_a_decision_entry_is_kept_and_reported() {
    // §12: a missing entry means the cycle did not finish properly. That is a
    // signal, not garbage.
    let (_dir, root) = project();
    add_plan(&root, CANCEL, &draft());

    let found = everything(&root);

    assert!(is_kept(&found, CANCEL, Keep::NoDecisionEntry));
    assert_eq!(found.eligible(), 0);
    assert!(Keep::NoDecisionEntry.is_refusal());
}

#[test]
fn an_entry_of_any_type_closes_a_cycle_on_paper() {
    let (_dir, root) = project();
    for (n, kind) in Type::ALL.into_iter().enumerate() {
        let ticket = format!("SHOP-{}", 10 + n);
        add_plan(
            &root,
            &format!("{ticket}-x"),
            &for_ticket(&draft(), &ticket),
        );
        add_entry(&root, &ticket, kind, 10 + n as u32);
    }

    let found = everything(&root);

    assert_eq!(found.eligible(), 4, "{found:#?}");
}

#[test]
fn an_entry_for_another_ticket_does_not_count() {
    // The ticket is compared whole: an entry for SHOP-21 is not one for
    // SHOP-214, whatever the names share.
    let (_dir, root) = project();
    add_plan(&root, "SHOP-21-short", &for_ticket(&draft(), "SHOP-21"));
    add_plan(&root, CANCEL, &draft());
    add_entry(&root, "SHOP-21", Type::Failed, 1);

    let found = everything(&root);

    assert!(is_deleted(&found, "SHOP-21-short"));
    assert!(is_kept(&found, CANCEL, Keep::NoDecisionEntry));
}

#[test]
fn the_newest_entry_is_the_one_named() {
    let (_dir, root) = project();
    add_plan(&root, CANCEL, &draft());
    add_entry(&root, "SHOP-214", Type::Note, 10);
    let newest = add_entry(&root, "SHOP-214", Type::Failed, 40);
    add_entry(&root, "SHOP-214", Type::Correction, 20);

    match &item(&everything(&root), CANCEL).verdict {
        Verdict::Delete { entry, .. } => {
            assert_eq!(entry.id, newest);
            assert_eq!(entry.kind, Type::Failed);
        }
        other => panic!("expected Delete, got {other:?}"),
    }
}

#[test]
fn an_entry_that_cannot_be_read_is_counted_and_proves_nothing() {
    // Better to keep a Plan than to delete one on the strength of a file
    // nobody can read: the safe mistake is the one that keeps it.
    let (_dir, root) = project();
    add_plan(&root, CANCEL, &draft());
    fs::create_dir_all(root.decisions_dir()).unwrap();
    fs::write(
        root.decisions_dir()
            .join("2026-10-06-163799-orders-shop-214.md"),
        "ticket: SHOP-214, but not a decision entry\n",
    )
    .unwrap();

    let found = everything(&root);

    assert_eq!(found.unreadable_entries, 1);
    assert!(is_kept(&found, CANCEL, Keep::NoDecisionEntry));
}

#[test]
fn no_decision_log_at_all_keeps_every_plan() {
    let (_dir, root) = project();
    add_plan(&root, CANCEL, &draft());
    assert!(!root.decisions_dir().exists());

    let found = everything(&root);

    assert!(is_kept(&found, CANCEL, Keep::NoDecisionEntry));
    assert_eq!(found.unreadable_entries, 0);
}

// --- what is in scope -----------------------------------------------------------

#[test]
fn without_a_plans_directory_there_is_nothing_to_do() {
    let (_dir, root) = project();

    let found = everything(&root);

    assert!(!found.plans_dir_exists);
    assert!(found.items.is_empty());
}

#[test]
fn a_ticket_limits_the_run_to_that_plan() {
    let (_dir, root) = project();
    add_plan(&root, CANCEL, &draft());
    add_plan(&root, "SHOP-300-other", &for_ticket(&draft(), "SHOP-300"));
    add_entry(&root, "SHOP-214", Type::Note, 1);
    add_entry(&root, "SHOP-300", Type::Note, 2);

    let found = assess(
        &root,
        Scope {
            ticket: Some("SHOP-214"),
            merged: None,
        },
        Mode::Light,
    )
    .unwrap();

    assert_eq!(found.items.len(), 1);
    assert_eq!(found.items[0].name, CANCEL);
    assert!(is_deleted(&found, CANCEL));
}

#[test]
fn a_ticket_that_names_no_plan_or_several_is_a_lookup_error() {
    let (_dir, root) = project();
    add_plan(&root, "SHOP-214-a", &draft());
    add_plan(&root, "SHOP-214-b", &draft());
    let scoped = |ticket| {
        assess(
            &root,
            Scope {
                ticket: Some(ticket),
                merged: None,
            },
            Mode::Light,
        )
    };

    assert!(matches!(
        scoped("SHOP-999"),
        Err(CleanError::Lookup(LookupError::NotFound { .. }))
    ));
    assert!(matches!(
        scoped("SHOP-214"),
        Err(CleanError::Lookup(LookupError::Ambiguous { .. }))
    ));
}

#[test]
fn a_directory_whose_name_is_not_a_plan_name_is_kept_and_never_deleted() {
    let (_dir, root) = project();
    let odd = add_plan(&root, "SHOP-9-Bad_Slug", &draft());
    add_entry(&root, "SHOP-9", Type::Failed, 1);

    // Named by its ticket, the one directory that matches is still judged.
    let named = assess(
        &root,
        Scope {
            ticket: Some("SHOP-9"),
            merged: None,
        },
        Mode::Light,
    )
    .unwrap();
    assert!(is_kept(&named, "SHOP-9-Bad_Slug", Keep::UnknownTicket));
    assert_eq!(named.items[0].ticket, None);

    // And in a run over everything.
    let all = everything(&root);
    assert!(is_kept(&all, "SHOP-9-Bad_Slug", Keep::UnknownTicket));
    assert!(Keep::UnknownTicket.is_refusal());

    delete(&root, &all).unwrap();
    assert!(odd.exists());
}

#[test]
fn a_leftover_directory_without_a_spec_can_be_cleaned_once_recorded() {
    // Half a Plan directory is exactly the kind of leftover this is for.
    let (_dir, root) = project();
    let dir = root.plans_dir().join("SHOP-8-empty");
    fs::create_dir_all(&dir).unwrap();
    add_entry(&root, "SHOP-8", Type::Note, 1);

    let found = everything(&root);

    assert!(is_deleted(&found, "SHOP-8-empty"));
    assert_eq!(item(&found, "SHOP-8-empty").status, None);
}

// --- --merged ---------------------------------------------------------------------

fn merged_scope(merged: &[String]) -> Scope<'_> {
    Scope {
        ticket: None,
        merged: Some(merged),
    }
}

fn branches(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| (*name).to_owned()).collect()
}

#[test]
fn one_merged_branch_that_names_the_ticket_makes_the_plan_eligible() {
    let (_dir, root) = project();
    add_plan(&root, CANCEL, &draft());
    add_entry(&root, "SHOP-214", Type::Failed, 1);
    let merged = branches(&["main", "feat/SHOP-214-cancel", "feat/SHOP-300-other"]);

    let found = assess(&root, merged_scope(&merged), Mode::Light).unwrap();

    match &item(&found, CANCEL).verdict {
        Verdict::Delete { branch, .. } => {
            assert_eq!(branch.as_deref(), Some("feat/SHOP-214-cancel"));
        }
        other => panic!("expected Delete, got {other:?}"),
    }
}

#[test]
fn no_merged_branch_skips_the_plan() {
    let (_dir, root) = project();
    add_plan(&root, CANCEL, &draft());
    add_entry(&root, "SHOP-214", Type::Failed, 1);
    let merged = branches(&["main", "feat/SHOP-300-other"]);

    let found = assess(&root, merged_scope(&merged), Mode::Light).unwrap();

    assert!(is_kept(&found, CANCEL, Keep::NotMerged));
    // A skip, not a refusal: nothing is wrong with the Plan.
    assert!(!Keep::NotMerged.is_refusal());
}

#[test]
fn a_branch_is_matched_on_whole_segments_of_its_name() {
    // `PROJ-41` must not find `feat/PROJ-412-x`, for branches as for Plans.
    let (_dir, root) = project();
    add_plan(&root, CANCEL, &draft());
    add_entry(&root, "SHOP-214", Type::Failed, 1);
    let merged = branches(&[
        "feat/SHOP-2140-longer",
        "feat/SHOP-21-shorter",
        "feat/XSHOP-214-prefixed",
        // Tickets are matched with their capitalisation, as for Plans.
        "feat/shop-214-lowercase",
    ]);

    let found = assess(&root, merged_scope(&merged), Mode::Light).unwrap();

    assert!(is_kept(&found, CANCEL, Keep::NotMerged));
}

#[test]
fn several_merged_branches_fail_the_run_and_list_them() {
    let (_dir, root) = project();
    add_plan(&root, CANCEL, &draft());
    add_entry(&root, "SHOP-214", Type::Failed, 1);
    let merged = branches(&["feat/SHOP-214-a", "fix/SHOP-214-b", "main"]);

    let err = assess(&root, merged_scope(&merged), Mode::Light).unwrap_err();

    let message = err.to_string();
    assert!(message.contains("nothing was deleted"), "{message}");
    assert!(
        message.contains("SHOP-214 names 2 merged branches"),
        "{message}"
    );
    assert!(
        message.contains("feat/SHOP-214-a, fix/SHOP-214-b"),
        "{message}"
    );
    assert!(
        message.contains("invar clean-spec <ticket> --force"),
        "{message}"
    );
}

#[test]
fn one_ambiguous_ticket_fails_the_whole_run_even_with_good_plans_beside_it() {
    let (_dir, root) = project();
    add_plan(&root, CANCEL, &draft());
    add_plan(&root, "SHOP-300-fine", &for_ticket(&draft(), "SHOP-300"));
    add_entry(&root, "SHOP-214", Type::Failed, 1);
    add_entry(&root, "SHOP-300", Type::Failed, 2);
    let merged = branches(&["feat/SHOP-214-a", "fix/SHOP-214-b", "feat/SHOP-300-only"]);

    let err = assess(&root, merged_scope(&merged), Mode::Light).unwrap_err();

    assert!(matches!(err, CleanError::AmbiguousBranches(ref found) if found.len() == 1));
    assert!(!err.to_string().contains("SHOP-300"));
}

#[test]
fn merged_does_not_excuse_a_missing_decision_entry() {
    let (_dir, root) = project();
    add_plan(&root, CANCEL, &draft());
    let merged = branches(&["feat/SHOP-214-cancel"]);

    let found = assess(&root, merged_scope(&merged), Mode::Light).unwrap();

    assert!(is_kept(&found, CANCEL, Keep::NoDecisionEntry));
}

// --- an approved plan ----------------------------------------------------------------

#[test]
fn an_approved_plan_is_kept_in_a_run_over_everything() {
    // Its cycle may be running right now; a bare run cannot tell.
    let (_dir, root) = project();
    add_plan(&root, CANCEL, &approved());
    add_entry(&root, "SHOP-214", Type::Note, 1);

    let found = everything(&root);

    assert!(is_kept(&found, CANCEL, Keep::ApprovedUnscoped));
    assert_eq!(item(&found, CANCEL).status, Some(Status::Approved));
    assert!(!Keep::ApprovedUnscoped.is_refusal());
}

#[test]
fn an_approved_plan_is_deleted_when_it_is_named_or_its_branch_is_merged() {
    let (_dir, root) = project();
    add_plan(&root, CANCEL, &approved());
    add_entry(&root, "SHOP-214", Type::Note, 1);

    let named = assess(
        &root,
        Scope {
            ticket: Some("SHOP-214"),
            merged: None,
        },
        Mode::Light,
    )
    .unwrap();
    assert!(is_deleted(&named, CANCEL));

    let merged = branches(&["feat/SHOP-214-cancel"]);
    let by_branch = assess(&root, merged_scope(&merged), Mode::Light).unwrap();
    assert!(is_deleted(&by_branch, CANCEL));

    // `--merged` with nothing merged is a skip for what it is, not a claim
    // about the approval.
    let none: Vec<String> = Vec::new();
    let unmerged = assess(&root, merged_scope(&none), Mode::Light).unwrap();
    assert!(is_kept(&unmerged, CANCEL, Keep::NotMerged));
}

// --- deleting ---------------------------------------------------------------------------

#[test]
fn delete_removes_the_eligible_plans_and_leaves_everything_else() {
    let (_dir, root) = project();
    let eligible = add_plan(&root, CANCEL, &draft());
    let no_entry = add_plan(
        &root,
        "SHOP-300-no-entry",
        &for_ticket(&draft(), "SHOP-300"),
    );
    let approved_dir = add_plan(
        &root,
        "SHOP-400-approved",
        &for_ticket(&approved(), "SHOP-400"),
    );
    add_entry(&root, "SHOP-214", Type::Failed, 1);
    add_entry(&root, "SHOP-400", Type::Note, 2);
    let decisions_before = fs::read_dir(root.decisions_dir()).unwrap().count();

    let found = everything(&root);
    let removed = delete(&root, &found).unwrap();

    assert_eq!(removed.as_slice(), std::slice::from_ref(&eligible));
    assert!(!eligible.exists());
    assert!(no_entry.exists(), "no entry, never deleted");
    assert!(approved_dir.exists(), "approved, not asked for");
    assert_eq!(
        fs::read_dir(root.decisions_dir()).unwrap().count(),
        decisions_before,
        "the decision log was touched"
    );
    assert!(root.plans_dir().exists());
}

#[test]
fn delete_does_nothing_for_an_assessment_with_nothing_eligible() {
    let (_dir, root) = project();
    let kept = add_plan(&root, CANCEL, &draft());

    let removed = delete(&root, &everything(&root)).unwrap();

    assert!(removed.is_empty());
    assert!(kept.join("spec.md").is_file());
}

#[test]
fn the_whole_directory_goes_including_what_else_is_in_it() {
    let (_dir, root) = project();
    let dir = add_plan(&root, CANCEL, &draft());
    fs::create_dir_all(dir.join("notes")).unwrap();
    fs::write(dir.join("notes/sketch.md"), "a sketch").unwrap();
    fs::write(dir.join("extra.txt"), "beside the spec").unwrap();
    add_entry(&root, "SHOP-214", Type::Note, 1);

    delete(&root, &everything(&root)).unwrap();

    assert!(!dir.exists());
}

#[cfg(unix)]
#[test]
fn a_plan_directory_that_is_a_symlink_is_unlinked_and_its_target_left_alone() {
    // `.invar/plans/<name>` may be a link to somewhere else. Deleting the
    // Plan must never mean deleting what it points at.
    let (dir, root) = project();
    let elsewhere = dir.path().join("elsewhere");
    fs::create_dir_all(&elsewhere).unwrap();
    fs::write(elsewhere.join("spec.md"), draft()).unwrap();
    fs::write(elsewhere.join("precious.txt"), "not a Plan").unwrap();
    fs::create_dir_all(root.plans_dir()).unwrap();
    std::os::unix::fs::symlink(&elsewhere, root.plans_dir().join(CANCEL)).unwrap();
    add_entry(&root, "SHOP-214", Type::Note, 1);

    let found = everything(&root);
    assert!(is_deleted(&found, CANCEL));
    delete(&root, &found).unwrap();

    assert!(!root.plans_dir().join(CANCEL).exists());
    assert!(root.plans_dir().join(CANCEL).symlink_metadata().is_err());
    assert_eq!(
        fs::read_to_string(elsewhere.join("precious.txt")).unwrap(),
        "not a Plan"
    );
    assert!(elsewhere.join("spec.md").is_file());
}

#[test]
fn a_name_that_is_not_one_path_component_is_never_joined_to_the_plans_directory() {
    // `assess` cannot produce these names. The check is for whoever builds an
    // `Assessment` by hand, and it is what keeps `delete` inside
    // `.invar/plans/` whatever it is given.
    let (_dir, root) = project();
    add_plan(&root, CANCEL, &draft());
    let sibling = root.plans_dir().parent().unwrap().join("escape");
    fs::create_dir_all(&sibling).unwrap();
    fs::write(sibling.join("keep.txt"), "outside the Plans directory").unwrap();

    for name in ["../escape", "a/b", "..", "", "/etc"] {
        let assessment = Assessment {
            plans_dir: root.plans_dir(),
            plans_dir_exists: true,
            items: vec![Item {
                name: name.to_owned(),
                path: root.plans_dir().join(CANCEL),
                ticket: None,
                status: None,
                verdict: Verdict::Delete {
                    entry: super::EntryRef {
                        id: "d-0000".to_owned(),
                        kind: Type::Note,
                        date: "2026-10-06T18:37:12+02:00".to_owned(),
                    },
                    branch: None,
                },
            }],
            unreadable_entries: 0,
        };

        let err = delete(&root, &assessment).unwrap_err();

        assert!(
            matches!(err, CleanError::NotAPlanName { .. }),
            "{name:?}: {err}"
        );
    }
    assert!(sibling.join("keep.txt").is_file());
    assert!(root.plans_dir().join(CANCEL).is_dir());
}

#[cfg(unix)]
#[test]
fn a_failed_removal_says_what_was_removed_before_it() {
    use std::os::unix::fs::PermissionsExt as _;

    let (_dir, root) = project();
    let first = add_plan(&root, "SHOP-1-first", &for_ticket(&draft(), "SHOP-1"));
    let second = add_plan(&root, "SHOP-2-second", &for_ticket(&draft(), "SHOP-2"));
    add_entry(&root, "SHOP-1", Type::Note, 1);
    add_entry(&root, "SHOP-2", Type::Note, 2);
    let found = everything(&root);

    // The second directory cannot be emptied: its parent of a nested
    // directory is not writable.
    let nested = second.join("nested");
    fs::create_dir_all(&nested).unwrap();
    fs::write(nested.join("file"), "x").unwrap();
    fs::set_permissions(&nested, fs::Permissions::from_mode(0o555)).unwrap();
    let probe = tempfile::NamedTempFile::new().unwrap();
    let is_root = {
        use std::os::unix::fs::MetadataExt as _;
        probe.as_file().metadata().unwrap().uid() == 0
    };

    let result = delete(&root, &found);

    fs::set_permissions(&nested, fs::Permissions::from_mode(0o755)).unwrap();
    if is_root {
        // Permissions do not stop root; nothing to show.
        return;
    }
    match result {
        Err(CleanError::Remove { path, removed, .. }) => {
            assert_eq!(path, second);
            assert_eq!(removed.as_slice(), std::slice::from_ref(&first));
        }
        other => panic!("expected Remove, got {other:?}"),
    }
    assert!(!first.exists());
    assert!(second.exists());
}
