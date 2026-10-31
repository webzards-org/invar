//! Tests for writing and reading the decision log (M1-11).

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use super::{
    RecordError, Request, draft, existing_ids, log, new_id, new_id_from, order_key, record,
};
use crate::format::decision::{self, Type};
use crate::format::plan::{Mode, Plan};
use crate::fs::ProjectRoot;
use crate::git::Instant;

/// 2026-10-06T18:37:12+02:00, i.e. 16:37:12 UTC.
const NOW: Instant = Instant {
    seconds: 1_791_304_632,
    offset_minutes: 120,
};

fn contract_example(number: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../contracts/formats/plan-header.md");
    let text = fs::read_to_string(path).unwrap();
    let after = text.split_once(&format!("### {number} ")).unwrap().1;
    let body = after.split_once("```markdown\n").unwrap().1;
    body.split_once("\n```\n").unwrap().0.to_owned() + "\n"
}

/// Example 9.2's approved header on example 9.1's body.
fn approved_plan() -> String {
    let draft = contract_example("9.1");
    contract_example("9.2") + draft.split_once("\n---\n").unwrap().1
}

/// A project, with `plan` as the Plan of SHOP-214 when given.
fn project(plan: Option<&str>) -> (TempDir, ProjectRoot) {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("invar.yaml"), "").unwrap();
    let root = ProjectRoot::for_test(dir.path());
    if let Some(plan) = plan {
        let spec = spec(&root);
        fs::create_dir_all(spec.parent().unwrap()).unwrap();
        fs::write(spec, plan).unwrap();
    }
    (dir, root)
}

fn spec(root: &ProjectRoot) -> PathBuf {
    root.plans_dir()
        .join("SHOP-214-cancel-unpaid-order/spec.md")
}

fn request(ticket: &str, kind: Type, module: Option<&str>) -> Request {
    Request {
        ticket: ticket.into(),
        kind,
        module: module.map(Into::into),
        body: "What was decided, and why.".into(),
    }
}

fn names(root: &ProjectRoot) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(root.decisions_dir())
        .map(|entries| {
            entries
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

// --- writing ----------------------------------------------------------------------

#[test]
fn an_entry_takes_module_and_slug_from_the_plan() {
    let (_dir, root) = project(Some(&approved_plan()));
    let recorded = record(
        &root,
        request("SHOP-214", Type::Note, None),
        NOW,
        Mode::Light,
    )
    .unwrap();
    assert_eq!(
        recorded.path,
        root.decisions_dir()
            .join("2026-10-06-163712-orders-cancel-unpaid-order.md")
    );
    let entry = &recorded.entry;
    assert_eq!(entry.date, "2026-10-06T18:37:12+02:00");
    assert_eq!(entry.ticket, "SHOP-214");
    assert_eq!(entry.module, "orders");
    assert_eq!(entry.kind, Type::Note);
    assert_eq!(entry.scenario, None);
    assert!(
        decision::is_id(&entry.id) && entry.id.len() == 10,
        "{}",
        entry.id
    );
    assert_eq!(recorded.reset, None);

    // The file is exactly the entry, and valid against its own name.
    let text = fs::read_to_string(&recorded.path).unwrap();
    assert_eq!(text, entry.render());
    let name = recorded.path.file_name().unwrap().to_str().unwrap();
    assert_eq!(&decision::check(name, &text).unwrap(), entry);
}

#[test]
fn without_a_plan_the_module_is_required_and_the_slug_is_the_ticket() {
    let (_dir, root) = project(None);
    match record(&root, request("SHOP-9", Type::Note, None), NOW, Mode::Light) {
        Err(RecordError::ModuleNeeded { ticket }) => assert_eq!(ticket, "SHOP-9"),
        other => panic!("expected ModuleNeeded, got {other:?}"),
    }
    let recorded = record(
        &root,
        request("SHOP-9", Type::Note, Some("global")),
        NOW,
        Mode::Light,
    )
    .unwrap();
    assert_eq!(
        recorded.path.file_name().unwrap(),
        "2026-10-06-163712-global-shop-9.md"
    );
    assert_eq!(recorded.entry.module, "global");
}

#[test]
fn a_module_that_disagrees_with_the_plan_is_refused() {
    let (_dir, root) = project(Some(&approved_plan()));
    match record(
        &root,
        request("SHOP-214", Type::Note, Some("payments")),
        NOW,
        Mode::Light,
    ) {
        Err(RecordError::ModuleMismatch {
            from_plan, given, ..
        }) => assert_eq!((from_plan.as_str(), given.as_str()), ("orders", "payments")),
        other => panic!("expected ModuleMismatch, got {other:?}"),
    }
    // Agreeing is fine.
    assert!(
        record(
            &root,
            request("SHOP-214", Type::Note, Some("orders")),
            NOW,
            Mode::Light
        )
        .is_ok()
    );
    assert!(names(&root).len() == 1);
}

#[test]
fn a_plan_with_an_unreadable_module_needs_the_argument() {
    let (_dir, root) = project(Some(
        &approved_plan().replace("Module: orders", "Module: or-ders"),
    ));
    assert!(matches!(
        record(
            &root,
            request("SHOP-214", Type::Note, None),
            NOW,
            Mode::Light
        ),
        Err(RecordError::ModuleUnreadable { .. })
    ));
    let recorded = record(
        &root,
        request("SHOP-214", Type::Note, Some("orders")),
        NOW,
        Mode::Light,
    )
    .unwrap();
    assert_eq!(recorded.entry.module, "orders");
}

#[test]
fn bad_input_is_refused_before_anything_is_written() {
    let (_dir, root) = project(Some(&approved_plan()));
    assert!(matches!(
        record(
            &root,
            request("shop-214", Type::Note, None),
            NOW,
            Mode::Light
        ),
        Err(RecordError::InvalidTicket { .. })
    ));
    assert!(matches!(
        record(
            &root,
            request("SHOP-214", Type::Note, Some("or-ders")),
            NOW,
            Mode::Light
        ),
        Err(RecordError::InvalidModule { .. })
    ));
    let empty = Request {
        body: " \n\t\n".into(),
        ..request("SHOP-214", Type::Note, None)
    };
    assert!(matches!(
        record(&root, empty, NOW, Mode::Light),
        Err(RecordError::EmptyBody)
    ));
    assert!(names(&root).is_empty());
}

#[test]
fn an_ambiguous_ticket_is_a_lookup_error() {
    let (_dir, root) = project(Some(&approved_plan()));
    let other = root.plans_dir().join("SHOP-214-other/spec.md");
    fs::create_dir_all(other.parent().unwrap()).unwrap();
    fs::write(other, approved_plan()).unwrap();
    assert!(matches!(
        record(
            &root,
            request("SHOP-214", Type::Note, None),
            NOW,
            Mode::Light
        ),
        Err(RecordError::Lookup(_))
    ));
}

// --- the same second, and insert-only ----------------------------------------------

#[test]
fn entries_in_the_same_second_get_suffixes_and_keep_their_order() {
    let (_dir, root) = project(Some(&approved_plan()));
    let mut ids = HashSet::new();
    for n in 1..=11 {
        let req = Request {
            body: format!("Entry {n}."),
            ..request("SHOP-214", Type::Note, None)
        };
        let recorded = record(&root, req, NOW, Mode::Light).unwrap();
        assert!(ids.insert(recorded.entry.id), "duplicate id");
    }
    assert_eq!(names(&root).len(), 11);
    assert!(
        names(&root).contains(&"2026-10-06-163712-orders-cancel-unpaid-order-11.md".to_owned())
    );

    // Newest first: 11, 10, …, 2, then the unsuffixed first entry.
    let listed = log(&root).unwrap().unwrap();
    let bodies: Vec<String> = listed
        .iter()
        .map(|item| item.entry.as_ref().unwrap().body.clone())
        .collect();
    let expected: Vec<String> = (1..=11).rev().map(|n| format!("Entry {n}.")).collect();
    assert_eq!(bodies, expected);
}

#[test]
fn an_existing_file_is_never_replaced() {
    let (_dir, root) = project(Some(&approved_plan()));
    fs::create_dir_all(root.decisions_dir()).unwrap();
    let taken = root
        .decisions_dir()
        .join("2026-10-06-163712-orders-cancel-unpaid-order.md");
    fs::write(&taken, "written by hand, not even an entry\n").unwrap();

    let recorded = record(
        &root,
        request("SHOP-214", Type::Note, None),
        NOW,
        Mode::Light,
    )
    .unwrap();
    assert!(
        recorded
            .path
            .ends_with("2026-10-06-163712-orders-cancel-unpaid-order-2.md")
    );
    assert_eq!(
        fs::read_to_string(&taken).unwrap(),
        "written by hand, not even an entry\n"
    );
}

#[test]
fn ids_already_in_the_log_are_not_reused() {
    let (_dir, root) = project(None);
    fs::create_dir_all(root.decisions_dir()).unwrap();
    fs::write(
        root.decisions_dir().join("2026-01-01-000000-global-x.md"),
        "---\n# by hand\nid: d-0000abcd\n---\n\nbroken but still reserving its id\n",
    )
    .unwrap();
    let ids = existing_ids(&root.decisions_dir());
    assert!(ids.contains("d-0000abcd"), "{ids:?}");
    // A number whose id is taken is skipped, and the next one is used.
    let mut numbers = [0x0000_abcd_u32, 0x0000_abcd, 0x1234_5678].into_iter();
    assert_eq!(new_id_from(&ids, || numbers.next().unwrap()), "d-12345678");
    // The real generator gives a fresh id each time.
    let fresh: HashSet<String> = (0..1000).map(|_| new_id(&ids)).collect();
    assert_eq!(fresh.len(), 1000);
    assert!(fresh.iter().all(|id| decision::is_id(id) && id.len() == 10));
}

#[test]
fn the_order_key_compares_trailing_numbers_as_numbers() {
    let mut names = vec![
        "2026-10-06-163712-orders-x-10.md",
        "2026-10-06-163712-orders-x.md",
        "2026-10-06-163712-orders-x-9.md",
        "2026-10-06-163712-orders-x-2.md",
        "2026-10-06-163713-orders-a.md",
        "2026-10-05-235959-orders-z-99.md",
    ];
    names.sort_by_key(|name| std::cmp::Reverse(order_key(name)));
    assert_eq!(
        names,
        [
            "2026-10-06-163713-orders-a.md",
            "2026-10-06-163712-orders-x-10.md",
            "2026-10-06-163712-orders-x-9.md",
            "2026-10-06-163712-orders-x-2.md",
            "2026-10-06-163712-orders-x.md",
            "2026-10-05-235959-orders-z-99.md",
        ]
    );
}

// --- `failed` resets the cycle ---------------------------------------------------------

#[test]
fn failed_clears_acceptance_and_keeps_its_key() {
    // plan-header.md §5, decision-entry.md §7.
    let original: String = approved_plan()
        .lines()
        .filter(|line| !line.starts_with("Acceptance:"))
        .map(|line| match line.starts_with("Approved-checksum:") {
            true => format!("{line}\nAcceptance: required\n"),
            false => format!("{line}\n"),
        })
        .collect();
    let (_dir, root) = project(Some(&original));
    record(
        &root,
        request("SHOP-214", Type::Failed, None),
        NOW,
        Mode::Light,
    )
    .unwrap();

    let reset = fs::read_to_string(spec(&root)).unwrap();
    assert!(
        reset.contains("\nApproved-checksum:\nAcceptance:\n---\n"),
        "{reset}"
    );
    assert_eq!(
        reset.split_once("\n---\n").unwrap().1,
        original.split_once("\n---\n").unwrap().1,
        "the body changed"
    );
}

#[test]
fn failed_returns_the_plan_to_draft_with_the_body_untouched() {
    let original = approved_plan();
    let (_dir, root) = project(Some(&original));
    let recorded = record(
        &root,
        request("SHOP-214", Type::Failed, None),
        NOW,
        Mode::Light,
    )
    .unwrap();
    assert_eq!(recorded.reset.as_deref(), Some(spec(&root).as_path()));
    assert_eq!(recorded.entry.kind, Type::Failed);

    let reset = fs::read_to_string(spec(&root)).unwrap();
    let changed: Vec<(&str, &str)> = original
        .lines()
        .zip(reset.lines())
        .filter(|(a, b)| a != b)
        .collect();
    assert_eq!(
        changed,
        [
            ("Status: approved", "Status: draft"),
            ("Approved-by: rade", "Approved-by:"),
            ("Approved-at: 2026-10-05", "Approved-at:"),
            (
                "Approved-checksum: sha256:b9caa4c64bc83723bb4e930d7fedeae733950c5210ee0e30f4ece84eb2c2cf7b",
                "Approved-checksum:"
            ),
        ]
    );
    assert_eq!(
        reset.split_once("\n---\n").unwrap().1,
        original.split_once("\n---\n").unwrap().1,
        "the body changed"
    );
}

#[test]
fn failed_resets_a_plan_approved_before_criteria_had_ids() {
    // Its body stays as it was, so it reads back as a draft with no ids. That is
    // what it now is; it is no reason to undo the reset (plan-header.md D-16).
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../contracts/fixtures/plan/approved-1.5.0-no-ids.md");
    let original = fs::read_to_string(path).unwrap();
    let (_dir, root) = project(Some(&original));
    let recorded = record(
        &root,
        request("SHOP-214", Type::Failed, None),
        NOW,
        Mode::Light,
    )
    .unwrap();
    assert_eq!(recorded.reset.as_deref(), Some(spec(&root).as_path()));

    let reset = fs::read_to_string(spec(&root)).unwrap();
    assert!(reset.contains("\nStatus: draft\n"), "{reset}");
    assert_eq!(
        reset.split_once("\n---\n").unwrap().1,
        original.split_once("\n---\n").unwrap().1,
        "the body changed"
    );
    // ...and as a draft it now needs its ids before anyone approves it.
    assert!(!Plan::check(&reset, Mode::Light).problems.is_empty());
}

#[test]
fn failed_clears_a_lockfile_checksum_too_and_keeps_crlf() {
    let original = approved_plan()
        .replace(
            "---\n\n## Ticket",
            &format!(
                "Lockfile-checksum: sha256:{}\n---\n\n## Ticket",
                "a".repeat(64)
            ),
        )
        .replace('\n', "\r\n");
    let (_dir, root) = project(Some(&original));
    record(
        &root,
        request("SHOP-214", Type::Failed, None),
        NOW,
        Mode::Light,
    )
    .unwrap();
    let reset = fs::read_to_string(spec(&root)).unwrap();
    assert!(reset.contains("Status: draft\r\n"));
    assert!(reset.contains("Lockfile-checksum:\r\n"));
    assert_eq!(
        reset.split_once("\r\n---\r\n").unwrap().1,
        original.split_once("\r\n---\r\n").unwrap().1
    );
}

#[test]
fn failed_on_a_draft_leaves_the_plan_file_as_it_is() {
    let draft = contract_example("9.1");
    let (_dir, root) = project(Some(&draft));
    let recorded = record(
        &root,
        request("SHOP-214", Type::Failed, None),
        NOW,
        Mode::Light,
    )
    .unwrap();
    assert!(recorded.reset.is_some());
    assert_eq!(fs::read_to_string(spec(&root)).unwrap(), draft);
}

#[test]
fn failed_without_a_plan_is_refused_and_writes_nothing() {
    let (_dir, root) = project(None);
    match record(
        &root,
        request("SHOP-9", Type::Failed, Some("orders")),
        NOW,
        Mode::Light,
    ) {
        Err(err @ RecordError::NoPlanToReset { .. }) => {
            let message = err.to_string();
            assert!(message.contains("has nothing to reset"), "{message}");
            assert!(message.contains("`--type note`"), "{message}");
        }
        other => panic!("expected NoPlanToReset, got {other:?}"),
    }
    assert!(names(&root).is_empty());
}

#[test]
fn failed_on_an_invalid_plan_is_refused_and_writes_nothing() {
    let broken = approved_plan().replace("## Scope\n", "## Scoop\n");
    let (_dir, root) = project(Some(&broken));
    assert!(matches!(
        record(
            &root,
            request("SHOP-214", Type::Failed, None),
            NOW,
            Mode::Light
        ),
        Err(RecordError::PlanInvalid { .. })
    ));
    assert_eq!(fs::read_to_string(spec(&root)).unwrap(), broken);
    assert!(names(&root).is_empty());
}

#[test]
fn when_the_entry_cannot_be_written_the_plan_is_restored() {
    let original = approved_plan();
    let (_dir, root) = project(Some(&original));
    // A file where the directory should be: the entry cannot be created.
    fs::create_dir_all(root.decisions_dir().parent().unwrap()).unwrap();
    fs::write(root.decisions_dir(), "not a directory").unwrap();

    assert!(matches!(
        record(
            &root,
            request("SHOP-214", Type::Failed, None),
            NOW,
            Mode::Light
        ),
        Err(RecordError::Io { .. })
    ));
    assert_eq!(fs::read_to_string(spec(&root)).unwrap(), original);
    // The Plan was put back by an atomic write, which leaves nothing behind.
    assert_eq!(plan_dir_files(&root), ["spec.md"]);
}

/// The names in the Plan's directory.
fn plan_dir_files(root: &ProjectRoot) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(spec(root).parent().unwrap())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn a_restored_plan_is_byte_identical_whatever_its_form() {
    // A BOM, CRLF line endings, a comment line, an unknown key and an inline
    // comment: everything a Plan keeps byte for byte has to come back the same
    // from a restore, not only the plain case.
    let original = format!(
        "\u{FEFF}{}",
        approved_plan()
            .replace(
                "Status: approved",
                "# set by the planner\nStatus: approved   # keep\nOwner: team-orders"
            )
            .replace('\n', "\r\n")
    );
    let (_dir, root) = project(Some(&original));
    fs::create_dir_all(root.decisions_dir().parent().unwrap()).unwrap();
    fs::write(root.decisions_dir(), "not a directory").unwrap();

    assert!(matches!(
        record(
            &root,
            request("SHOP-214", Type::Failed, None),
            NOW,
            Mode::Light
        ),
        Err(RecordError::Io { .. })
    ));
    assert_eq!(fs::read(spec(&root)).unwrap(), original.as_bytes());
    assert_eq!(plan_dir_files(&root), ["spec.md"]);
}

#[test]
fn other_types_never_touch_the_plan() {
    let original = approved_plan();
    let (_dir, root) = project(Some(&original));
    for kind in [Type::Confirmed, Type::Correction, Type::Note] {
        let recorded = record(&root, request("SHOP-214", kind, None), NOW, Mode::Light).unwrap();
        assert_eq!(recorded.reset, None);
    }
    assert_eq!(fs::read_to_string(spec(&root)).unwrap(), original);
}

// --- approving a draft (`invar decision approve`) ---------------------------------------

fn draft_path(root: &ProjectRoot) -> PathBuf {
    spec(root).with_file_name(decision::DRAFT_FILE)
}

fn write_draft(root: &ProjectRoot, module: &str, kind: &str, body: &str) -> PathBuf {
    let path = draft_path(root);
    fs::write(
        &path,
        format!("---\nticket: SHOP-214\nmodule: {module}\ntype: {kind}\n---\n\n{body}\n"),
    )
    .unwrap();
    path
}

#[test]
fn approving_records_the_draft_as_it_is_now_and_removes_it() {
    let (_dir, root) = project(Some(&approved_plan()));
    let path = write_draft(&root, "orders", "confirmed", "First version.");
    // The developer edits the draft while reading it; the edit is recorded.
    write_draft(
        &root,
        "orders",
        "confirmed",
        "Stock is released in the cancel transaction.\r\n",
    );

    let approved = draft::approve(&root, &path, NOW, Mode::Light).unwrap();
    assert!(approved.left_behind.is_none());
    assert_eq!(approved.draft, path);
    assert!(!path.exists(), "the draft is removed");

    let recorded = approved.recorded;
    assert_eq!(
        recorded.path,
        root.decisions_dir()
            .join("2026-10-06-163712-orders-cancel-unpaid-order.md")
    );
    assert!(decision::is_id(&recorded.entry.id), "{}", recorded.entry.id);
    assert_eq!(recorded.entry.date, "2026-10-06T18:37:12+02:00");
    assert_eq!(recorded.entry.kind, Type::Confirmed);
    assert_eq!(
        recorded.entry.body,
        "Stock is released in the cancel transaction."
    );
    assert_eq!(recorded.reset, None);
    let text = fs::read_to_string(&recorded.path).unwrap();
    assert_eq!(text, recorded.entry.render());
    let name = recorded.path.file_name().unwrap().to_str().unwrap();
    assert!(decision::check(name, &text).is_ok());
    // The Plan is untouched, and only its spec is left beside it.
    assert_eq!(fs::read_to_string(spec(&root)).unwrap(), approved_plan());
    assert_eq!(plan_dir_files(&root), ["spec.md"]);
}

#[test]
fn the_drafts_module_is_taken_as_written_even_when_the_plan_names_another() {
    let (_dir, root) = project(Some(&approved_plan()));
    let path = write_draft(&root, "global", "note", "Applies to every module.");
    let recorded = draft::approve(&root, &path, NOW, Mode::Light)
        .unwrap()
        .recorded;
    assert_eq!(recorded.entry.module, "global");
    assert_eq!(
        recorded.path.file_name().unwrap(),
        "2026-10-06-163712-global-cancel-unpaid-order.md"
    );
}

#[test]
fn approving_a_failed_draft_returns_the_plan_to_draft() {
    let (_dir, root) = project(Some(&approved_plan()));
    let path = write_draft(&root, "orders", "failed", "The gateway has no void call.");
    let recorded = draft::approve(&root, &path, NOW, Mode::Light)
        .unwrap()
        .recorded;
    assert_eq!(recorded.entry.kind, Type::Failed);
    assert_eq!(recorded.reset, Some(spec(&root)));
    let plan = fs::read_to_string(spec(&root)).unwrap();
    assert!(plan.contains("Status: draft"), "{plan}");
    assert!(!path.exists());
}

#[test]
fn without_a_draft_nothing_is_written() {
    let (_dir, root) = project(Some(&approved_plan()));
    match draft::approve(&root, &draft_path(&root), NOW, Mode::Light) {
        Err(err @ draft::ApproveError::NoDraft { .. }) => {
            assert!(err.to_string().contains("/invar:log"), "{err}");
        }
        other => panic!("expected NoDraft, got {other:?}"),
    }
    assert!(!root.decisions_dir().exists());
}

#[test]
fn an_invalid_draft_is_refused_and_kept() {
    let (_dir, root) = project(Some(&approved_plan()));
    let path = write_draft(&root, "orders", "correction", "By hand.");
    let before = fs::read_to_string(&path).unwrap();
    match draft::approve(&root, &path, NOW, Mode::Light) {
        Err(draft::ApproveError::Invalid(report)) => {
            assert!(!report.problems.is_empty());
            assert!(report.problems.to_string().contains("correction"));
        }
        other => panic!("expected Invalid, got {other:?}"),
    }
    assert_eq!(fs::read_to_string(&path).unwrap(), before);
    assert!(!root.decisions_dir().exists());
    // A failed draft that does not validate resets nothing either.
    let path = write_draft(&root, "orders", "failed", "");
    assert!(matches!(
        draft::approve(&root, &path, NOW, Mode::Light),
        Err(draft::ApproveError::Invalid(_))
    ));
    assert_eq!(fs::read_to_string(spec(&root)).unwrap(), approved_plan());
}

#[test]
fn approving_twice_records_once() {
    let (_dir, root) = project(Some(&approved_plan()));
    let path = write_draft(&root, "orders", "note", "Once.");
    draft::approve(&root, &path, NOW, Mode::Light).unwrap();
    assert!(matches!(
        draft::approve(&root, &path, NOW, Mode::Light),
        Err(draft::ApproveError::NoDraft { .. })
    ));
    assert_eq!(names(&root).len(), 1);
}

// --- reading ----------------------------------------------------------------------------

#[test]
fn the_log_is_newest_first_and_skips_what_is_not_an_entry() {
    let (_dir, root) = project(None);
    assert!(log(&root).unwrap().is_none());

    let earlier = Instant {
        seconds: NOW.seconds - 3600,
        ..NOW
    };
    record(
        &root,
        request("SHOP-1", Type::Note, Some("orders")),
        earlier,
        Mode::Light,
    )
    .unwrap();
    record(
        &root,
        request("SHOP-2", Type::Note, Some("global")),
        NOW,
        Mode::Light,
    )
    .unwrap();
    fs::write(root.decisions_dir().join(".gitkeep"), "").unwrap();
    fs::write(root.decisions_dir().join("README.txt"), "not an entry").unwrap();
    fs::write(
        root.decisions_dir()
            .join("2026-10-06-170000-orders-broken.md"),
        "no front matter\n",
    )
    .unwrap();

    let listed = log(&root).unwrap().unwrap();
    let tickets: Vec<String> = listed
        .iter()
        .map(|item| match &item.entry {
            Ok(entry) => entry.ticket.clone(),
            Err(_) => "invalid".into(),
        })
        .collect();
    assert_eq!(tickets, ["invalid", "SHOP-2", "SHOP-1"]);
}
