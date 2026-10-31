//! Tests for `plan approve` (M1-9): what is refused, what is written, and that
//! nothing else in the file changes.

use std::fs;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use super::{
    ApproveError, Decision, Earlier, Pending, Prior, choose_acceptance, classify, prepare,
};
use crate::digest::{canonicalize, digest};
use crate::format::plan::{Acceptance, Header, Mode, Plan, Status};
use crate::fs::ProjectRoot;
use crate::gate::approval::{ApprovedValue, approved_value};

// --- fixtures ---------------------------------------------------------------

fn contract() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../contracts/formats/plan-header.md");
    fs::read_to_string(&path).unwrap_or_else(|err| panic!("reading {}: {err}", path.display()))
}

/// The first ```markdown block under `### <number> ` in the contract.
fn contract_example(number: &str) -> String {
    let text = contract();
    let after = text
        .split_once(&format!("### {number} "))
        .unwrap_or_else(|| panic!("no example {number}"))
        .1;
    let body = after.split_once("```markdown\n").unwrap().1;
    body.split_once("\n```\n").unwrap().0.to_owned() + "\n"
}

/// A project holding `text` as the Plan of SHOP-214.
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

fn pending(root: &ProjectRoot, spec: &Path, reapprove: bool) -> Pending {
    match prepare(root, spec, Mode::Light, reapprove, None, None) {
        Ok(Decision::Approve(pending)) => *pending,
        Ok(other) => panic!("expected an approval, got {other:?}"),
        Err(err) => panic!("expected an approval, got: {err}"),
    }
}

fn approve(root: &ProjectRoot, spec: &Path, reapprove: bool) {
    pending(root, spec, reapprove)
        .commit("rade".into(), "2026-10-05".into())
        .unwrap();
}

fn refusal(root: &ProjectRoot, spec: &Path, reapprove: bool) -> ApproveError {
    match prepare(root, spec, Mode::Light, reapprove, None, None) {
        Err(err) => err,
        Ok(decision) => panic!("expected a refusal, got {decision:?}"),
    }
}

/// `text` as `plan approve` finds it once the key is there, so a comparison
/// line by line lines up.
fn with_empty_acceptance(text: &str) -> String {
    let mut out = String::new();
    for line in text.split_inclusive('\n') {
        out.push_str(line);
        if line.starts_with("Approved-checksum:") {
            out.push_str("Acceptance:");
            out.push_str(if line.ends_with("\r\n") { "\r\n" } else { "\n" });
        }
    }
    out
}

/// Lines that differ between two texts of the same length in lines.
fn changed_lines(before: &str, after: &str) -> Vec<(String, String)> {
    let before: Vec<&str> = before.split_inclusive('\n').collect();
    let after: Vec<&str> = after.split_inclusive('\n').collect();
    assert_eq!(before.len(), after.len(), "the number of lines changed");
    before
        .iter()
        .zip(&after)
        .filter(|(b, a)| b != a)
        .map(|(b, a)| ((*b).to_owned(), (*a).to_owned()))
        .collect()
}

fn header(status: Status, by: Option<&str>, at: Option<&str>, checksum: Option<&str>) -> Header {
    Header {
        ticket: "SHOP-214".into(),
        module: "orders".into(),
        status,
        approved_by: by.map(Into::into),
        approved_at: at.map(Into::into),
        approved_checksum: checksum.map(Into::into),
        lockfile_checksum: None,
        acceptance: None,
    }
}

const SUM_A: &str = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const SUM_B: &str = "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

// --- the contract's own examples ---------------------------------------------

#[test]
fn approving_example_9_1_produces_example_9_2() {
    // 9.2 is "the same Plan after `plan approve`", with Approved-by `rade` on
    // 2026-10-05. Its header, including the checksum, must be exactly what
    // the command writes, and its body must be 9.1's.
    let draft = contract_example("9.1");
    let (_dir, root, spec) = project(&draft);
    approve(&root, &spec, false);
    let written = fs::read_to_string(&spec).unwrap();

    let header_of = |text: &str| -> String {
        let rest = text.strip_prefix("---\n").unwrap();
        let end = rest.find("\n---\n").unwrap();
        format!("---\n{}\n---\n", &rest[..end])
    };
    assert_eq!(header_of(&written), contract_example("9.2"));
    assert_eq!(
        written.split_once("\n---\n").unwrap().1,
        draft.split_once("\n---\n").unwrap().1,
        "the body changed"
    );
}

// --- what is written -----------------------------------------------------------

#[test]
fn only_the_five_approval_values_change() {
    let draft = with_empty_acceptance(&contract_example("9.1"));
    let (_dir, root, spec) = project(&draft);
    let pending = pending(&root, &spec, false);
    let approval_digest = digest(&format!(
        "Acceptance: skipped\n{}",
        canonicalize(
            draft
                .split_once("\n---\n")
                .unwrap()
                .1
                .trim_start_matches('\n'),
        )
    ));
    let approval = pending
        .commit("Rade Brković".into(), "2026-10-05".into())
        .unwrap();
    assert_eq!(approval.checksum, approval_digest);

    let written = fs::read_to_string(&spec).unwrap();
    assert_eq!(
        changed_lines(&draft, &written),
        [
            ("Status: draft\n".into(), "Status: approved\n".into()),
            (
                "Approved-by:\n".into(),
                "Approved-by: Rade Brković\n".into()
            ),
            ("Approved-at:\n".into(), "Approved-at: 2026-10-05\n".into()),
            (
                "Approved-checksum:\n".into(),
                format!("Approved-checksum: {approval_digest}\n")
            ),
            ("Acceptance:\n".into(), "Acceptance: skipped\n".into()),
        ]
    );
}

#[test]
fn crlf_a_bom_comments_unknown_keys_and_the_body_survive_byte_for_byte() {
    let draft = format!(
        "\u{FEFF}{}",
        contract_example("9.1")
            .replace(
                "Status: draft\n",
                "# set by the planner\nStatus: draft   # keep\nOwner: team-orders\n"
            )
            .replace("## Scope\n", "## Scope  \n")
            .replace('\n', "\r\n")
    );
    let draft = with_empty_acceptance(&draft);
    let (_dir, root, spec) = project(&draft);
    approve(&root, &spec, false);
    let written = fs::read_to_string(&spec).unwrap();

    let changed = changed_lines(&draft, &written);
    assert_eq!(changed.len(), 5, "{changed:#?}");
    assert_eq!(
        changed[0],
        (
            "Status: draft   # keep\r\n".into(),
            "Status: approved   # keep\r\n".into()
        )
    );
    assert!(written.starts_with('\u{FEFF}'));
    // The checksum is the §4d digest: a CRLF checkout of the same text
    // approves to the same value as the LF original.
    let (_lf_dir, lf_root, lf_spec) = project(&contract_example("9.1"));
    approve(&lf_root, &lf_spec, false);
    let checksum_of = |text: &str| {
        text.lines()
            .find_map(|line| line.strip_prefix("Approved-checksum: "))
            .unwrap()
            .trim()
            .to_owned()
    };
    assert_eq!(
        checksum_of(&written),
        checksum_of(&fs::read_to_string(&lf_spec).unwrap())
    );
    assert!(written.contains("# set by the planner\r\nStatus: approved"));
    assert!(written.contains("Owner: team-orders\r\n"));
    assert_eq!(
        written.split_once("\r\n---\r\n").unwrap().1,
        draft.split_once("\r\n---\r\n").unwrap().1
    );
}

#[test]
fn a_lockfile_checksum_is_left_as_it_is() {
    let draft = with_empty_acceptance(&contract_example("9.1")).replace(
        "Approved-checksum:\n",
        &format!("Approved-checksum:\nLockfile-checksum: {SUM_A}\n"),
    );
    let (_dir, root, spec) = project(&draft);
    approve(&root, &spec, false);
    let written = fs::read_to_string(&spec).unwrap();
    assert!(written.contains(&format!("Lockfile-checksum: {SUM_A}\n")));
    assert_eq!(changed_lines(&draft, &written).len(), 5);
}

#[test]
fn the_acceptance_criteria_are_the_section_without_surrounding_blank_lines() {
    let (_dir, root, spec) = project(&contract_example("9.1"));
    let pending = pending(&root, &spec, false);
    let criteria = pending.acceptance_criteria();
    assert!(
        criteria.starts_with("- AC1: Given a pending, unpaid order"),
        "{criteria}"
    );
    assert!(criteria.ends_with("with status `cancelled`."), "{criteria}");
    assert_eq!(pending.ticket(), "SHOP-214");
}

// --- refusals ------------------------------------------------------------------

#[test]
fn an_invalid_plan_is_refused_with_the_validation_problems() {
    let draft = contract_example("9.1").replace("## Scope\n", "## Scop\n");
    let (_dir, root, spec) = project(&draft);
    let ApproveError::Invalid(report) = refusal(&root, &spec, false) else {
        panic!("expected Invalid");
    };
    assert!(
        report
            .problems
            .to_string()
            .contains("missing required section `## Scope`"),
        "{}",
        report.problems
    );
    assert_eq!(
        fs::read_to_string(&spec).unwrap(),
        draft,
        "the file changed"
    );
}

#[test]
fn a_header_that_disagrees_with_its_directory_is_refused() {
    let draft = contract_example("9.1").replace("Ticket: SHOP-214", "Ticket: SHOP-215");
    let (_dir, root, spec) = project(&draft);
    assert!(matches!(
        refusal(&root, &spec, false),
        ApproveError::Invalid(_)
    ));
}

#[test]
fn a_file_changed_after_prepare_is_not_written() {
    let draft = contract_example("9.1");
    let (_dir, root, spec) = project(&draft);
    let pending = pending(&root, &spec, false);
    let edited = draft.replace("then the request is", "then the request must be");
    fs::write(&spec, &edited).unwrap();
    let err = pending
        .commit("rade".into(), "2026-10-05".into())
        .unwrap_err();
    assert!(
        matches!(err, ApproveError::ChangedMeanwhile { .. }),
        "{err}"
    );
    assert_eq!(fs::read_to_string(&spec).unwrap(), edited);
}

#[test]
fn values_that_cannot_be_written_into_the_header_are_refused() {
    let draft = contract_example("9.1");
    let (_dir, root, spec) = project(&draft);
    for (by, at) in [
        ("rade # admin", "2026-10-05"),
        ("rade", "05.10.2026"),
        ("  ", "2026-10-05"),
    ] {
        let err = pending(&root, &spec, false)
            .commit(by.into(), at.into())
            .unwrap_err();
        assert!(matches!(err, ApproveError::Unwritable(_)), "{by:?}: {err}");
        assert_eq!(fs::read_to_string(&spec).unwrap(), draft);
    }
}

// --- re-approval (OQ-1, D-12) ----------------------------------------------------

#[test]
fn an_approved_plan_whose_body_is_unchanged_is_left_alone() {
    let draft = contract_example("9.1");
    let (_dir, root, spec) = project(&draft);
    approve(&root, &spec, false);
    let approved = fs::read_to_string(&spec).unwrap();
    for reapprove in [false, true] {
        match prepare(&root, &spec, Mode::Light, reapprove, None, None).unwrap() {
            Decision::AlreadyApproved { by, at, .. } => {
                assert_eq!((by.as_str(), at.as_str()), ("rade", "2026-10-05"));
            }
            other => panic!("expected AlreadyApproved, got {other:?}"),
        }
    }
    assert_eq!(fs::read_to_string(&spec).unwrap(), approved);
}

#[test]
fn a_changed_body_is_refused_without_reapprove_and_approved_with_it() {
    let (_dir, root, spec) = project(&contract_example("9.1"));
    approve(&root, &spec, false);
    let edited = fs::read_to_string(&spec)
        .unwrap()
        .replace("refused with 403", "refused with 409");
    fs::write(&spec, &edited).unwrap();

    match refusal(&root, &spec, false) {
        ApproveError::NeedsReapprove {
            prior: Prior::Changed { by, at, recorded },
            current,
            ..
        } => {
            assert_eq!((by.as_str(), at.as_str()), ("rade", "2026-10-05"));
            assert_ne!(recorded, current);
        }
        other => panic!("expected NeedsReapprove(Changed), got {other}"),
    }
    assert_eq!(fs::read_to_string(&spec).unwrap(), edited, "refusal wrote");

    let pending = pending(&root, &spec, true);
    assert!(matches!(pending.replaces(), Some(Prior::Changed { .. })));
    let approval = pending.commit("mila".into(), "2026-10-06".into()).unwrap();
    let written = fs::read_to_string(&spec).unwrap();
    assert!(written.contains("Approved-by: mila\n"));
    assert!(written.contains("Approved-at: 2026-10-06\n"));
    assert!(written.contains(&format!("Approved-checksum: {}\n", approval.checksum)));
    assert!(written.contains("refused with 409"));
}

#[test]
fn an_approval_no_command_recorded_needs_reapprove() {
    // Example 9.3's situation: an agent wrote `Status: approved` itself.
    let draft = contract_example("9.1")
        .replace("Status: draft", "Status: approved")
        .replace("Approved-by:", "Approved-by: agent");
    let (_dir, root, spec) = project(&draft);
    match refusal(&root, &spec, false) {
        ApproveError::NeedsReapprove {
            prior: Prior::Unrecorded { missing },
            ..
        } => assert_eq!(missing, ["Approved-at", "Approved-checksum"]),
        other => panic!("expected NeedsReapprove(Unrecorded), got {other}"),
    }
    let message = refusal(&root, &spec, false).to_string();
    assert!(
        message.contains("`Approved-at` and `Approved-checksum` are empty"),
        "{message}"
    );
    assert!(message.contains("--reapprove"), "{message}");

    approve(&root, &spec, true);
    assert!(
        fs::read_to_string(&spec)
            .unwrap()
            .contains("Approved-by: rade\n")
    );
}

#[test]
fn reapprove_on_a_draft_is_an_ordinary_approval() {
    let (_dir, root, spec) = project(&contract_example("9.1"));
    let pending = pending(&root, &spec, true);
    assert_eq!(pending.replaces(), None);
}

#[test]
fn a_draft_with_leftover_approval_values_is_approved_and_they_are_replaced() {
    // plan-header.md §5.1 and §5.3: while `Status` says `draft` the approval
    // values mean nothing. `plan approve` overwrites whatever the keys hold,
    // without `--reapprove`, and the body is untouched.
    let draft = with_empty_acceptance(&contract_example("9.1"))
        .replace("Acceptance:\n", "Acceptance: required\n")
        .replace("Approved-by:\n", "Approved-by: someone-else\n")
        .replace("Approved-at:\n", "Approved-at: 2020-01-01\n")
        .replace(
            "Approved-checksum:\n",
            &format!("Approved-checksum: {SUM_A}\n"),
        );
    assert_ne!(
        draft,
        contract_example("9.1"),
        "the leftovers must be in the header"
    );
    let (_dir, root, spec) = project(&draft);

    let pending = pending(&root, &spec, false);
    assert_eq!(pending.replaces(), None);
    let approval = pending.commit("rade".into(), "2026-10-05".into()).unwrap();
    assert_ne!(approval.checksum, SUM_A);

    let written = fs::read_to_string(&spec).unwrap();
    assert_eq!(
        changed_lines(&draft, &written),
        [
            ("Status: draft\n".into(), "Status: approved\n".into()),
            (
                "Approved-by: someone-else\n".into(),
                "Approved-by: rade\n".into()
            ),
            (
                "Approved-at: 2020-01-01\n".into(),
                "Approved-at: 2026-10-05\n".into()
            ),
            (
                format!("Approved-checksum: {SUM_A}\n"),
                format!("Approved-checksum: {}\n", approval.checksum)
            ),
            (
                "Acceptance: required\n".into(),
                "Acceptance: skipped\n".into()
            ),
        ]
    );
    for stale in ["someone-else", "2020-01-01", SUM_A] {
        assert!(!written.contains(stale), "{stale} is still there");
    }
}

#[test]
fn values_that_cannot_be_written_are_refused_before_the_commit() {
    // `plan approve` asks this before it prints the criteria, so a person
    // never reads the criteria of an approval that cannot be recorded.
    let draft = contract_example("9.1");
    let (_dir, root, spec) = project(&draft);
    let pending = pending(&root, &spec, false);

    assert!(pending.check_values("rade", "2026-10-05").is_ok());
    for (by, at) in [
        ("rade # admin", "2026-10-05"),
        ("rade", "05.10.2026"),
        ("  ", "2026-10-05"),
    ] {
        assert!(
            matches!(
                pending.check_values(by, at),
                Err(ApproveError::Unwritable(_))
            ),
            "{by:?} {at:?}"
        );
    }
    assert_eq!(fs::read_to_string(&spec).unwrap(), draft);
}

// --- classify ------------------------------------------------------------------

#[test]
fn classify_covers_every_state() {
    // A draft is a draft, whatever is in its approval fields.
    assert_eq!(
        classify(
            &header(Status::Draft, Some("x"), Some("2026-01-01"), Some(SUM_A)),
            SUM_A
        ),
        Prior::Draft
    );
    assert_eq!(
        classify(
            &header(
                Status::Approved,
                Some("rade"),
                Some("2026-10-05"),
                Some(SUM_A)
            ),
            SUM_A
        ),
        Prior::Current {
            by: "rade".into(),
            at: "2026-10-05".into()
        }
    );
    assert_eq!(
        classify(
            &header(
                Status::Approved,
                Some("rade"),
                Some("2026-10-05"),
                Some(SUM_A)
            ),
            SUM_B
        ),
        Prior::Changed {
            by: "rade".into(),
            at: "2026-10-05".into(),
            recorded: SUM_A.into()
        }
    );
    assert_eq!(
        classify(
            &header(Status::Approved, None, Some("2026-10-05"), None),
            SUM_A
        ),
        Prior::Unrecorded {
            missing: vec!["Approved-by", "Approved-checksum"]
        }
    );
}

// --- where `Acceptance` comes from (plan-header.md §5.3, D-20) -----------------------

#[test]
fn every_label_claims_only_what_the_digest_showed() {
    use Acceptance::{Required, Skipped};
    for (flag, earlier, value, shown) in [
        (
            Some(Skipped),
            Earlier::Confirmed(Required),
            Skipped,
            "skipped (from --no-acceptance)",
        ),
        (
            Some(Required),
            Earlier::Nothing,
            Required,
            "required (from --acceptance)",
        ),
        (
            None,
            Earlier::Confirmed(Required),
            Required,
            "required (kept from the earlier approval)",
        ),
        (
            None,
            Earlier::Unconfirmed(Required),
            Required,
            "required (from the header; the earlier approval cannot confirm it)",
        ),
        (
            None,
            Earlier::NotApproved(Required),
            Skipped,
            "skipped (default; the `required` in the header was not part of the approval)",
        ),
        (None, Earlier::Nothing, Skipped, "skipped (default)"),
    ] {
        let chosen = choose_acceptance(flag, earlier, None);
        assert_eq!(chosen.value, value);
        assert_eq!(chosen.to_string(), shown);
    }
}

#[test]
fn config_comes_after_the_flag_and_the_earlier_approval_and_before_the_default() {
    use Acceptance::{Required, Skipped};
    for (flag, earlier, configured, value, shown) in [
        (
            None,
            Earlier::Nothing,
            Some(Required),
            Required,
            "required (from invar.yaml)",
        ),
        (
            None,
            Earlier::Nothing,
            Some(Skipped),
            Skipped,
            "skipped (from invar.yaml)",
        ),
        (
            None,
            Earlier::NotApproved(Skipped),
            Some(Required),
            Required,
            "required (from invar.yaml; the `skipped` in the header was not part of the approval)",
        ),
        (
            Some(Skipped),
            Earlier::Nothing,
            Some(Required),
            Skipped,
            "skipped (from --no-acceptance)",
        ),
        (
            Some(Required),
            Earlier::Nothing,
            Some(Skipped),
            Required,
            "required (from --acceptance)",
        ),
        (
            None,
            Earlier::Confirmed(Skipped),
            Some(Required),
            Skipped,
            "skipped (kept from the earlier approval)",
        ),
        (
            None,
            Earlier::Unconfirmed(Required),
            Some(Skipped),
            Required,
            "required (from the header; the earlier approval cannot confirm it)",
        ),
    ] {
        let chosen = choose_acceptance(flag, earlier, configured);
        assert_eq!(chosen.value, value);
        assert_eq!(chosen.to_string(), shown);
    }
}

/// Example `number`'s header on example 9.1's body.
fn parsed(number: &str) -> Plan {
    let draft = contract_example("9.1");
    let text = contract_example(number) + draft.split_once("\n---\n").unwrap().1;
    Plan::parse(&text, Mode::Light).unwrap()
}

#[test]
fn the_digest_says_what_it_can_about_the_approved_value() {
    use Acceptance::{Required, Skipped};
    let required = parsed("9.10");
    let recorded = required.header().approved_checksum.clone().unwrap();
    assert_eq!(
        approved_value(&required, &recorded),
        ApprovedValue::Confirmed(Required)
    );
    // Example 9.2 is the same body under `skipped`.
    let skipped = parsed("9.2");
    assert_eq!(
        approved_value(&skipped, &recorded),
        ApprovedValue::Changed(Required)
    );
    let old = parsed("9.12");
    assert_eq!(
        approved_value(&required, old.header().approved_checksum.as_ref().unwrap()),
        ApprovedValue::None
    );
    assert_eq!(approved_value(&required, SUM_A), ApprovedValue::Unknown);
    // No value in the header: the digest still shows what the approval had.
    assert_eq!(
        approved_value(&old, &recorded),
        ApprovedValue::Changed(Required)
    );
    assert_eq!(
        approved_value(&old, skipped.header().approved_checksum.as_ref().unwrap()),
        ApprovedValue::Changed(Skipped)
    );
}

/// `text` with its `Acceptance` key, if any, holding `line` instead.
fn with_acceptance(text: &str, line: &str) -> String {
    with_empty_acceptance(text).replace("Acceptance:\n", &format!("{line}\n"))
}

#[test]
fn a_value_left_in_a_draft_is_not_kept() {
    let draft = with_acceptance(&contract_example("9.1"), "Acceptance: required");
    let (_dir, root, spec) = project(&draft);
    assert_eq!(
        pending(&root, &spec, false).acceptance().to_string(),
        "skipped (default)"
    );
}

#[test]
fn a_value_on_an_approval_nobody_recorded_is_not_kept() {
    // Example 9.3: no command recorded this approval, so neither its value.
    let unrecorded = with_acceptance(&contract_example("9.1"), "Acceptance: required")
        .replace("Status: draft", "Status: approved");
    let (_dir, root, spec) = project(&unrecorded);
    assert_eq!(
        pending(&root, &spec, true).acceptance().to_string(),
        "skipped (default)"
    );
}
