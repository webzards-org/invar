//! Tests for `plan check` (M1-10): exactly one state passes.

use std::fs;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use super::{CheckError, check};
use crate::digest::{canonicalize, digest};
use crate::format::plan::{Acceptance, Mode};
use crate::fs::ProjectRoot;
use crate::gate::approve::{Decision, prepare};

fn contract_example(number: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../contracts/formats/plan-header.md");
    let text = fs::read_to_string(path).unwrap();
    let after = text.split_once(&format!("### {number} ")).unwrap().1;
    let body = after.split_once("```markdown\n").unwrap().1;
    body.split_once("\n```\n").unwrap().0.to_owned() + "\n"
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

/// Approve the Plan the way `plan approve` does.
fn approve(root: &ProjectRoot, spec: &Path) {
    let Decision::Approve(pending) = prepare(root, spec, Mode::Light, false, None, None).unwrap()
    else {
        panic!("expected an approval");
    };
    pending.commit("rade".into(), "2026-10-05".into()).unwrap();
}

fn refusal(root: &ProjectRoot, spec: &Path) -> CheckError {
    match check(root, spec, Mode::Light) {
        Err(err) => err,
        Ok(approved) => panic!("expected a refusal, got {approved:?}"),
    }
}

/// Example 9.1's body under example 9.2's header.
fn example_9_2_in_full() -> String {
    let body = contract_example("9.1")
        .split_once("\n---\n")
        .unwrap()
        .1
        .to_owned();
    contract_example("9.2") + &body
}

// --- passes ---------------------------------------------------------------------

#[test]
fn the_contracts_approved_example_passes() {
    let (_dir, root, spec) = project(&example_9_2_in_full());
    let approved = check(&root, &spec, Mode::Light).unwrap();
    assert_eq!(approved.ticket, "SHOP-214");
    assert_eq!(approved.by, "rade");
    assert_eq!(approved.at, "2026-10-05");
    assert_eq!(
        approved.checksum,
        "sha256:b017737bb6bfa67784b6eb2979946c4703c7e28f37ee363501ca0b886978aea5"
    );
    assert_eq!(approved.spec, spec);
}

#[test]
fn a_plan_just_approved_passes() {
    let (_dir, root, spec) = project(&contract_example("9.1"));
    approve(&root, &spec);
    assert!(check(&root, &spec, Mode::Light).is_ok());
}

#[test]
fn an_approved_plan_checked_out_with_crlf_still_passes() {
    // Approved on LF, then `core.autocrlf=true` on another machine.
    let (_dir, root, spec) = project(&contract_example("9.1"));
    approve(&root, &spec);
    let crlf = fs::read_to_string(&spec).unwrap().replace('\n', "\r\n");
    fs::write(&spec, crlf).unwrap();
    let approved = check(&root, &spec, Mode::Light).unwrap();
    assert_eq!(
        approved.checksum,
        "sha256:b017737bb6bfa67784b6eb2979946c4703c7e28f37ee363501ca0b886978aea5"
    );
}

#[test]
fn trailing_whitespace_and_blank_lines_before_the_body_do_not_matter() {
    // §4d canonicalization and D-1: neither changes the body's digest.
    let text = example_9_2_in_full()
        .replace("## Scope\n", "## Scope   \n")
        .replace("\n---\n\n## Ticket", "\n---\n\n\n\n## Ticket");
    let (_dir, root, spec) = project(&text);
    assert!(check(&root, &spec, Mode::Light).is_ok());
}

// --- refusals ---------------------------------------------------------------------

#[test]
fn a_draft_is_refused() {
    let (_dir, root, spec) = project(&contract_example("9.1"));
    match refusal(&root, &spec) {
        CheckError::Draft { ticket, .. } => assert_eq!(ticket, "SHOP-214"),
        other => panic!("expected Draft, got {other}"),
    }
    let message = refusal(&root, &spec).to_string();
    assert!(message.contains("is still a draft"), "{message}");
    assert!(message.contains("Do not start implementing"), "{message}");
    assert!(
        message.contains("`invar plan approve SHOP-214`"),
        "{message}"
    );
}

#[test]
fn a_plan_changed_after_approval_is_refused_in_plain_words() {
    let (_dir, root, spec) = project(&example_9_2_in_full());
    let edited = fs::read_to_string(&spec)
        .unwrap()
        .replace("refused with 403", "refused with 409");
    fs::write(&spec, edited).unwrap();
    match refusal(&root, &spec) {
        CheckError::Changed(stale) => {
            assert_eq!(
                (stale.by.as_str(), stale.at.as_str()),
                ("rade", "2026-10-05")
            );
            assert_ne!(stale.recorded, stale.current);
            assert_eq!(stale.ticket, "SHOP-214");
        }
        other => panic!("expected Changed, got {other}"),
    }
    let message = refusal(&root, &spec).to_string();
    assert!(
        message.contains("the Plan for SHOP-214 changed after it was approved"),
        "{message}"
    );
    assert!(message.contains("--reapprove"), "{message}");
}

#[test]
fn an_approval_without_a_checksum_is_refused() {
    // Example 9.3's header, on a body that is valid in form.
    let text = contract_example("9.1")
        .replace("Status: draft", "Status: approved")
        .replace("Approved-by:", "Approved-by: rade")
        .replace("Approved-at:", "Approved-at: 2026-10-05");
    let (_dir, root, spec) = project(&text);
    match refusal(&root, &spec) {
        CheckError::Unrecorded { missing, .. } => assert_eq!(missing, ["Approved-checksum"]),
        other => panic!("expected Unrecorded, got {other}"),
    }
    let message = refusal(&root, &spec).to_string();
    assert!(
        message.contains("`Approved-checksum` is empty: no `invar plan approve` recorded"),
        "{message}"
    );
}

#[test]
fn a_plan_that_is_not_valid_is_refused_with_its_problems() {
    let text = example_9_2_in_full().replace("## Intent\n", "## Purpose\n");
    let (_dir, root, spec) = project(&text);
    match refusal(&root, &spec) {
        CheckError::Invalid(report) => assert!(
            report
                .problems
                .to_string()
                .contains("missing required section `## Intent`"),
            "{}",
            report.problems
        ),
        other => panic!("expected Invalid, got {other}"),
    }
}

#[test]
fn a_header_only_change_that_keeps_the_body_still_passes() {
    // The checksum covers the body, not the header (§4.3): a comment added
    // to the header is not a change to what was approved.
    let text = example_9_2_in_full().replace("Module: orders\n", "Module: orders  # billing too\n");
    let (_dir, root, spec) = project(&text);
    assert!(check(&root, &spec, Mode::Light).is_ok());
}

#[test]
fn a_header_ticket_moved_away_from_its_directory_is_refused() {
    // The checksum covers the body only, so this edit leaves it intact; the
    // form check is what catches it.
    let text = example_9_2_in_full().replace("Ticket: SHOP-214\n", "Ticket: SHOP-215\n");
    let (_dir, root, spec) = project(&text);
    match refusal(&root, &spec) {
        CheckError::Invalid(report) => assert!(
            report.problems.to_string().contains("`Ticket: SHOP-215`"),
            "{}",
            report.problems
        ),
        other => panic!("expected Invalid, got {other}"),
    }
}

// --- the approval digest covers `Acceptance` (plan-header.md §5.3, D-19) ------------

/// The body of example 9.1, as the digest sees it.
fn canonical_body() -> String {
    canonicalize(
        contract_example("9.1")
            .split_once("\n---\n")
            .unwrap()
            .1
            .trim_start_matches('\n'),
    )
}

/// Example `number`'s header on example 9.1's body.
fn in_full(number: &str) -> String {
    let body = contract_example("9.1")
        .split_once("\n---\n")
        .unwrap()
        .1
        .to_owned();
    contract_example(number) + &body
}

#[test]
fn with_a_value_the_digest_takes_its_line_in_front_of_the_body() {
    let (_dir, root, spec) = project(&in_full("9.10"));
    let approved = check(&root, &spec, Mode::Light).unwrap();
    assert_eq!(
        approved.checksum,
        digest(&format!("Acceptance: required\n{}", canonical_body()))
    );
    assert_ne!(approved.checksum, digest(&canonical_body()));
}

#[test]
fn without_a_value_the_digest_is_that_of_the_body() {
    let (_dir, root, spec) = project(&in_full("9.12"));
    let approved = check(&root, &spec, Mode::Light).unwrap();
    assert_eq!(approved.checksum, digest(&canonical_body()));
}

#[test]
fn the_decision_on_record_comes_back_with_the_approval() {
    for (number, expected) in [
        ("9.10", Acceptance::Required),
        ("9.2", Acceptance::Skipped),
        ("9.12", Acceptance::Skipped),
    ] {
        let (_dir, root, spec) = project(&in_full(number));
        let approved = check(&root, &spec, Mode::Light).unwrap();
        assert_eq!(approved.acceptance, expected, "{number}");
    }
}

#[test]
fn an_approval_with_a_value_checked_out_with_crlf_still_passes() {
    // The line in the digest ends in `\n`, whatever the file uses.
    let (_dir, root, spec) = project(&in_full("9.10").replace('\n', "\r\n"));
    assert!(check(&root, &spec, Mode::Light).is_ok());
}

#[test]
fn changing_removing_or_clearing_the_value_stales_the_approval() {
    let approved = in_full("9.10");
    for edited in [
        approved.replace("Acceptance: required", "Acceptance: skipped"),
        approved.replace("Acceptance: required\n", ""),
        approved.replace("Acceptance: required", "Acceptance:"),
    ] {
        assert_ne!(edited, approved);
        let (_dir, root, spec) = project(&edited);
        assert!(
            matches!(refusal(&root, &spec), CheckError::Changed(_)),
            "{edited}"
        );
    }
}

#[test]
fn adding_a_value_to_an_approval_from_before_the_field_stales_it() {
    let line = "\nApproved-checksum: sha256:b9caa4c64bc83723bb4e930d7fedeae733950c5210ee0e30f4ece84eb2c2cf7b\n";
    for value in ["required", "skipped"] {
        let edited = in_full("9.12").replace(line, &format!("{line}Acceptance: {value}\n"));
        let (_dir, root, spec) = project(&edited);
        assert!(
            matches!(refusal(&root, &spec), CheckError::Changed(_)),
            "{edited}"
        );
    }
}

#[test]
fn an_empty_key_added_to_an_approval_from_before_the_field_changes_nothing() {
    let line = "\nApproved-checksum: sha256:b9caa4c64bc83723bb4e930d7fedeae733950c5210ee0e30f4ece84eb2c2cf7b\n";
    let edited = in_full("9.12").replace(line, &format!("{line}Acceptance:\n"));
    let (_dir, root, spec) = project(&edited);
    assert!(check(&root, &spec, Mode::Light).is_ok());
}

#[test]
fn the_moved_line_is_refused_in_every_spelling_the_digest_cannot_tell_apart() {
    for line in [
        "Acceptance: skipped",
        "Acceptance: required   ",
        "\u{FEFF}Acceptance: required",
    ] {
        let moved =
            in_full("9.12").replace("---\n\n## Ticket", &format!("---\n\n{line}\n## Ticket"));
        let (_dir, root, spec) = project(&moved);
        assert!(
            matches!(refusal(&root, &spec), CheckError::Invalid(_)),
            "{line:?}"
        );
    }
}

#[test]
fn moving_the_value_to_the_top_of_the_body_is_refused() {
    // The digest alone cannot see this: it hashes the same text (D-19).
    let moved = in_full("9.10")
        .replace("Acceptance: required\n", "")
        .replace("---\n\n## Ticket", "---\n\nAcceptance: required\n## Ticket");
    assert_eq!(
        digest(&canonicalize(
            moved
                .split_once("\n---\n")
                .unwrap()
                .1
                .trim_start_matches('\n')
        )),
        digest(&format!("Acceptance: required\n{}", canonical_body())),
        "the moved line must give the digest of the approval"
    );
    let (_dir, root, spec) = project(&moved);
    assert!(
        matches!(refusal(&root, &spec), CheckError::Invalid(_)),
        "{moved}"
    );
}
