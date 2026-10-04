//! Tests for `plan validate` (M1-8): every category in one run, form not
//! approval, and the three ways to name a Plan.

use std::fs;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use super::{Report, drafts, file, ticket};
use crate::format::plan::{Mode, Status};
use crate::fs::ProjectRoot;
use crate::fs::plans::LookupError;

/// A valid light-mode draft for `ticket`.
fn plan_text(ticket: &str) -> String {
    format!(
        "---\nTicket: {ticket}\nModule: orders\nStatus: draft\nApproved-by:\n\
         Approved-at:\nApproved-checksum:\n---\n\n\
         ## Ticket\n{ticket} — Cancel an unpaid order.\n\n\
         ## Intent\nCustomers cancel their own unpaid orders.\n\n\
         ## Scope\n`orders` only.\n\n\
         ## Open questions / answers\nNone were needed.\n\n\
         ## Rejected alternatives\nNone considered.\n\n\
         ## Acceptance criteria\n- Given an unpaid order, when I cancel it, then it is cancelled.\n\n\
         ## Implementation context\n- AI_CONTEXT: controllers stay thin\n"
    )
}

fn project() -> (TempDir, ProjectRoot) {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("invar.yaml"), "").unwrap();
    let root = ProjectRoot::for_test(dir.path());
    fs::create_dir_all(root.plans_dir()).unwrap();
    (dir, root)
}

/// Write `text` as `.invar/plans/<name>/spec.md`; returns its path.
fn write_plan(root: &ProjectRoot, name: &str, text: &str) -> PathBuf {
    let dir = root.plans_dir().join(name);
    fs::create_dir_all(&dir).unwrap();
    let spec = dir.join("spec.md");
    fs::write(&spec, text).unwrap();
    spec
}

/// Each problem as `line N: message` or `message`.
fn messages(report: &Report) -> Vec<String> {
    report
        .problems
        .to_string()
        .lines()
        .map(str::to_owned)
        .collect()
}

fn assert_mentions(report: &Report, needles: &[&str]) {
    let all = report.problems.to_string();
    for needle in needles {
        assert!(all.contains(needle), "no {needle:?} in:\n{all}");
    }
}

fn validate(root: &ProjectRoot, spec: &Path) -> Report {
    file(root, spec, Mode::Light)
}

// --- valid ------------------------------------------------------------------

#[test]
fn a_valid_draft_with_empty_approval_fields_passes() {
    let (_dir, root) = project();
    let spec = write_plan(&root, "PROJ-412-reserve-stock", &plan_text("PROJ-412"));
    let report = validate(&root, &spec);
    assert!(report.is_valid(), "{}", report.problems);
    assert_eq!(report.status, Some(Status::Draft));
    assert_eq!(report.spec, spec);
}

#[test]
fn approval_fields_are_not_checked() {
    // Form, not approval (D-11): `approved` with an empty checksum is
    // `plan check`'s business, and so is a draft carrying approval values.
    let (_dir, root) = project();
    let approved = plan_text("PROJ-1").replace("Status: draft", "Status: approved");
    let spec = write_plan(&root, "PROJ-1-approved-empty", &approved);
    assert!(validate(&root, &spec).is_valid());

    let draft_with_values = plan_text("PROJ-2")
        .replace("Approved-by:", "Approved-by: rade")
        .replace("Approved-at:", "Approved-at: 2026-10-05");
    let spec = write_plan(&root, "PROJ-2-draft-values", &draft_with_values);
    assert!(validate(&root, &spec).is_valid());
}

// --- every category in one run -----------------------------------------------

#[test]
fn every_category_is_reported_in_one_run() {
    let (_dir, root) = project();
    // Directory name: lowercase ticket. Header: bad Module. Sections: one
    // missing, one misspelled, one empty, two out of order.
    let text = plan_text("PROJ-412")
        .replace("Module: orders", "Module: Orders")
        .replace("## Intent\n", "## intent\n")
        .replace("## Rejected alternatives\nNone considered.\n", "")
        .replace(
            "- Given an unpaid order, when I cancel it, then it is cancelled.\n",
            "_none_\n",
        )
        .replace(
            "## Scope\n`orders` only.\n\n## Open questions / answers\nNone were needed.\n",
            "## Open questions / answers\nNone were needed.\n\n## Scope\n`orders` only.\n",
        );
    let spec = write_plan(&root, "proj-412-reserve-stock", &text);
    let report = validate(&root, &spec);
    assert_mentions(
        &report,
        &[
            "directory name `proj-412-reserve-stock`",
            "`Module` must be",
            "`## intent` should be `## Intent`",
            "missing required section `## Rejected alternatives`",
            "`## Acceptance criteria` is empty",
            "out of order",
        ],
    );
    assert_eq!(report.problems.len(), 6, "{}", report.problems);
}

#[test]
fn missing_or_empty_acceptance_criteria_and_implementation_context_fail() {
    let (_dir, root) = project();
    let missing = plan_text("PROJ-1")
        .replace(
            "## Acceptance criteria\n- Given an unpaid order, when I cancel it, then it is cancelled.\n\n",
            "",
        )
        .replace("## Implementation context\n- AI_CONTEXT: controllers stay thin\n", "");
    let report = validate(&root, &write_plan(&root, "PROJ-1-missing", &missing));
    assert_mentions(
        &report,
        &[
            "missing required section `## Acceptance criteria`",
            "missing required section `## Implementation context`",
        ],
    );

    let empty = plan_text("PROJ-2")
        .replace(
            "- Given an unpaid order, when I cancel it, then it is cancelled.\n",
            "",
        )
        .replace("- AI_CONTEXT: controllers stay thin\n", "\n");
    let report = validate(&root, &write_plan(&root, "PROJ-2-empty", &empty));
    assert_mentions(
        &report,
        &[
            "`## Acceptance criteria` is empty",
            "`## Implementation context` is empty",
        ],
    );
}

#[test]
fn a_header_ticket_that_disagrees_with_the_directory_fails_at_its_line() {
    let (_dir, root) = project();
    let spec = write_plan(&root, "PROJ-412-reserve-stock", &plan_text("PROJ-413"));
    let report = validate(&root, &spec);
    assert_eq!(report.problems.len(), 1, "{}", report.problems);
    let message = &messages(&report)[0];
    assert!(message.starts_with("line 2: "), "{message}");
    assert!(message.contains("`Ticket: PROJ-413`"), "{message}");
    assert!(message.contains("is for `PROJ-412`"), "{message}");
}

#[test]
fn the_ticket_mismatch_is_reported_alongside_other_header_problems() {
    let (_dir, root) = project();
    let text = plan_text("PROJ-413").replace("Module: orders", "Module: Orders");
    let report = validate(&root, &write_plan(&root, "PROJ-412-reserve-stock", &text));
    assert_mentions(&report, &["`Module` must be", "`Ticket: PROJ-413`"]);
    assert_eq!(report.problems.len(), 2, "{}", report.problems);
}

// --- location ----------------------------------------------------------------

#[test]
fn a_plan_outside_the_plans_directory_is_told_where_to_move() {
    let (_dir, root) = project();
    let spec = root.path().join("notes/spec.md");
    fs::create_dir_all(spec.parent().unwrap()).unwrap();
    fs::write(&spec, plan_text("PROJ-1")).unwrap();
    let report = validate(&root, &spec);
    assert_eq!(report.problems.len(), 1, "{}", report.problems);
    assert_mentions(
        &report,
        &[
            ".invar/plans/<TICKET>-<slug>/spec.md",
            "not `notes/spec.md`",
            "move it",
        ],
    );
}

#[test]
fn a_plan_file_not_named_spec_md_is_told_to_rename() {
    let (_dir, root) = project();
    let dir = root.plans_dir().join("PROJ-1-cancel");
    fs::create_dir_all(&dir).unwrap();
    let plan = dir.join("plan.md");
    fs::write(&plan, plan_text("PROJ-1")).unwrap();
    let report = validate(&root, &plan);
    assert_eq!(report.problems.len(), 1, "{}", report.problems);
    assert_mentions(&report, &["rename", ".invar/plans/PROJ-1-cancel/spec.md"]);
}

#[test]
fn a_plan_outside_the_project_is_reported() {
    let (_dir, root) = project();
    let elsewhere = tempfile::tempdir().unwrap();
    let spec = elsewhere.path().join("spec.md");
    fs::write(&spec, plan_text("PROJ-1")).unwrap();
    let report = validate(&root, &spec);
    assert_mentions(&report, &["outside the project"]);
}

#[test]
fn a_missing_file_and_invalid_utf8_are_problems_in_the_report() {
    let (_dir, root) = project();
    let dir = root.plans_dir().join("PROJ-1-cancel");
    fs::create_dir_all(&dir).unwrap();
    let report = validate(&root, &dir.join("spec.md"));
    assert_mentions(&report, &["there is no `spec.md`"]);

    let spec = write_plan(&root, "PROJ-2-bytes", "");
    fs::write(&spec, b"---\nTicket: PROJ-2\xE9\n---\n").unwrap();
    assert_mentions(&validate(&root, &spec), &["not valid UTF-8"]);
}

#[test]
fn a_plan_at_the_right_depth_in_the_wrong_directory_is_told_where_to_move() {
    let (_dir, root) = project();
    let spec = root.path().join("docs/plans/PROJ-1-cancel/spec.md");
    fs::create_dir_all(spec.parent().unwrap()).unwrap();
    fs::write(&spec, plan_text("PROJ-1")).unwrap();
    let report = validate(&root, &spec);
    assert_eq!(report.problems.len(), 1, "{}", report.problems);
    assert_mentions(&report, &["not `docs/plans/PROJ-1-cancel/spec.md`"]);
}

// --- by ticket -----------------------------------------------------------------

#[test]
fn by_ticket_validates_the_matching_plan() {
    let (_dir, root) = project();
    let spec = write_plan(&root, "PROJ-412-reserve-stock", &plan_text("PROJ-412"));
    write_plan(&root, "PROJ-500-refunds", "broken");
    let report = ticket(&root, "PROJ-412", Mode::Light).unwrap();
    assert!(report.is_valid(), "{}", report.problems);
    assert_eq!(report.spec, spec);
}

#[test]
fn by_ticket_still_validates_a_malformed_name_or_a_missing_spec() {
    let (_dir, root) = project();
    let text = plan_text("PROJ-7").replace("## Scope\n", "## Scoop\n");
    write_plan(&root, "PROJ-7-Bad_Name", &text);
    let report = ticket(&root, "PROJ-7", Mode::Light).unwrap();
    assert_mentions(
        &report,
        &[
            "directory name `PROJ-7-Bad_Name`",
            "missing required section `## Scope`",
        ],
    );

    fs::create_dir_all(root.plans_dir().join("PROJ-8-empty")).unwrap();
    let report = ticket(&root, "PROJ-8", Mode::Light).unwrap();
    assert_mentions(&report, &["there is no `spec.md`"]);
}

#[test]
fn by_ticket_returns_lookup_errors_that_leave_nothing_to_validate() {
    let (_dir, root) = project();
    write_plan(&root, "PROJ-412-reserve-stock", &plan_text("PROJ-412"));
    write_plan(&root, "PROJ-412-other", &plan_text("PROJ-412"));
    assert!(matches!(
        ticket(&root, "PROJ-412", Mode::Light),
        Err(LookupError::Ambiguous { .. })
    ));
    assert!(matches!(
        ticket(&root, "PROJ-41", Mode::Light),
        Err(LookupError::NotFound { .. })
    ));
}

// --- all drafts ------------------------------------------------------------------

#[test]
fn drafts_skips_approved_plans_and_keeps_unreadable_status() {
    let (_dir, root) = project();
    write_plan(&root, "PROJ-1-draft", &plan_text("PROJ-1"));
    let approved = plan_text("PROJ-2").replace("Status: draft", "Status: approved");
    // Invalid in form, but approved: not a draft, so not validated.
    write_plan(
        &root,
        "PROJ-2-approved",
        &approved.replace("## Scope\n", ""),
    );
    write_plan(
        &root,
        "PROJ-3-no-status",
        &plan_text("PROJ-3").replace("Status: draft\n", ""),
    );
    fs::create_dir_all(root.plans_dir().join("PROJ-4-no-spec")).unwrap();

    let reports = drafts(&root, Mode::Light).unwrap().unwrap();
    let names: Vec<String> = reports
        .iter()
        .map(|r| {
            r.spec
                .parent()
                .unwrap()
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    assert_eq!(
        names,
        ["PROJ-1-draft", "PROJ-3-no-status", "PROJ-4-no-spec"]
    );
    assert!(reports[0].is_valid());
    assert!(!reports[1].is_valid());
    assert!(!reports[2].is_valid());
}

#[test]
fn drafts_without_a_plans_directory_is_none() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("invar.yaml"), "").unwrap();
    let root = ProjectRoot::for_test(dir.path());
    assert!(drafts(&root, Mode::Light).unwrap().is_none());
}
