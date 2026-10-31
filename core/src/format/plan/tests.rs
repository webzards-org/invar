//! Tests for Plan parsing and writing (M1-6), against
//! `contracts/formats/plan-header.md` v1.7.0.
//!
//! The valid Plan used throughout is the contract's own example 9.1, read out
//! of the Markdown, so the contract and the parser cannot drift apart.

use std::fs;
use std::path::Path;

use super::{Acceptance, Approval, Mode, Plan, PlanError, Status};
use crate::diagnostics::Diagnostics;

// --- fixtures ---------------------------------------------------------------

fn contract() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../contracts/formats/plan-header.md");
    fs::read_to_string(&path).unwrap_or_else(|err| panic!("reading {}: {err}", path.display()))
}

/// The first ```markdown block under the contract heading `### <number> `.
fn contract_example(number: &str) -> String {
    let text = contract();
    let heading = format!("### {number} ");
    let after_heading = text
        .split_once(&heading)
        .unwrap_or_else(|| panic!("the contract has no example {number}"))
        .1;
    let body = after_heading
        .split_once("```markdown\n")
        .unwrap_or_else(|| panic!("example {number} has no markdown block"))
        .1;
    body.split_once("\n```\n")
        .unwrap_or_else(|| panic!("example {number} has no closing fence"))
        .0
        .to_owned()
        + "\n"
}

/// Contract example 9.1: a valid light-mode draft.
fn valid() -> String {
    contract_example("9.1")
}

fn parse(text: &str) -> Plan {
    Plan::parse(text, Mode::Light)
        .unwrap_or_else(|problems| panic!("expected a valid Plan:\n{problems}"))
}

fn problems(text: &str, mode: Mode) -> Diagnostics {
    match Plan::parse(text, mode) {
        Ok(_) => panic!("expected an invalid Plan"),
        Err(problems) => problems,
    }
}

/// Every message, `line N: ` prefixed where there is a line.
fn messages(text: &str, mode: Mode) -> Vec<String> {
    problems(text, mode)
        .to_string()
        .lines()
        .map(str::to_owned)
        .collect()
}

fn approval() -> Approval {
    Approval {
        by: "rade".to_owned(),
        at: "2026-10-05".to_owned(),
        checksum: format!("sha256:{}", "ab".repeat(32)),
    }
}

fn replace(text: &str, from: &str, to: &str) -> String {
    assert!(text.contains(from), "{from:?} not in the fixture");
    text.replacen(from, to, 1)
}

// --- the contract's examples -----------------------------------------------

#[test]
fn contract_9_1_parses_into_the_typed_header() {
    let plan = parse(&valid());
    let header = plan.header();
    assert_eq!(header.ticket, "SHOP-214");
    assert_eq!(header.module, "orders");
    assert_eq!(header.status, Status::Draft);
    assert_eq!(header.approved_by, None);
    assert_eq!(header.approved_at, None);
    assert_eq!(header.approved_checksum, None);
    assert_eq!(header.lockfile_checksum, None);
}

#[test]
fn contract_9_1_sections_are_found_in_order() {
    let plan = parse(&valid());
    let headings: Vec<&str> = plan.sections().iter().map(|s| s.heading.as_str()).collect();
    assert_eq!(
        headings,
        [
            "Ticket",
            "Intent",
            "Scope",
            "Open questions / answers",
            "Rejected alternatives",
            "Acceptance criteria",
            "Implementation context",
        ]
    );
    let criteria = plan.acceptance_criteria().unwrap();
    assert!(
        criteria.starts_with("- AC1: Given a pending, unpaid order of mine"),
        "{criteria}"
    );
    assert!(
        criteria.trim_end().ends_with("with status `cancelled`."),
        "{criteria}"
    );
}

#[test]
fn contract_9_4_reports_both_problems_in_scenarios_mode() {
    let found = messages(&contract_example("9.4"), Mode::Scenarios);
    assert!(
        found
            .iter()
            .any(|m| m.contains("missing required section `## Implementation context`")),
        "{found:#?}"
    );
    assert!(
        found
            .iter()
            .any(|m| m.contains("`## Scenarios to delete` is empty") && m.contains("_none_")),
        "{found:#?}"
    );
    assert_eq!(found.len(), 2, "{found:#?}");
}

#[test]
fn contract_9_4_in_light_mode_lacks_criteria_and_context_but_not_scenarios() {
    // The scenario sections are ignored in light mode (D-9): the empty
    // `## Scenarios to delete` is not reported there.
    let found = messages(&contract_example("9.4"), Mode::Light);
    assert!(
        found.iter().any(|m| m.contains("`## Acceptance criteria`")),
        "{found:#?}"
    );
    assert!(
        found
            .iter()
            .any(|m| m.contains("`## Implementation context`")),
        "{found:#?}"
    );
    assert!(!found.iter().any(|m| m.contains("Scenarios")), "{found:#?}");
    assert_eq!(found.len(), 2, "{found:#?}");
}

#[test]
fn contract_9_5_empty_acceptance_criteria_is_reported() {
    // 9.5 is 9.1 with the criteria removed and the heading kept.
    let criteria = parse(&valid()).acceptance_criteria().unwrap().to_owned();
    let text = replace(&valid(), &criteria, "\n");
    let found = messages(&text, Mode::Light);
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(
        found[0].contains("`## Acceptance criteria` is empty"),
        "{found:#?}"
    );
}

// --- round trip -------------------------------------------------------------

#[test]
fn round_trip_is_byte_identical() {
    let lf = valid();
    let variants = [
        lf.clone(),
        lf.replace('\n', "\r\n"),
        lf.replace('\n', "\r"),
        format!("\u{FEFF}{lf}"),
        lf.trim_end_matches('\n').to_owned(),
        // Mixed endings: CRLF in the header, LF in the body.
        lf.replacen('\n', "\r\n", 7),
        // Trailing whitespace and blank lines everywhere a hand edit puts them.
        replace(&lf, "---\n\n## Ticket", "---\n\n  \n\t\n## Ticket"),
        replace(
            &lf,
            "Module: orders\n",
            "Module: orders   \n# a comment\n\nX-Reviewer: ana # inline\n",
        ),
    ];
    for (index, text) in variants.iter().enumerate() {
        // A variant that is not a valid Plan is a broken fixture, not a pass.
        let plan = Plan::parse(text, Mode::Light)
            .unwrap_or_else(|p| panic!("variant {index} does not parse:\n{p}"));
        assert_eq!(plan.to_text(), *text, "variant {index} did not round-trip");
    }
}

#[test]
fn unknown_keys_and_comments_keep_their_positions_through_an_approval() {
    let original = replace(
        &valid(),
        "Status: draft\n",
        "# reviewed by the team lead\nStatus: draft\nX-Estimate: 3d\n",
    );
    let mut plan = parse(&original);
    plan.approve(approval()).unwrap();

    let expected = original
        .replace("Status: draft\n", "Status: approved\n")
        .replace("Approved-by:\n", "Approved-by: rade\n")
        .replace("Approved-at:\n", "Approved-at: 2026-10-05\n")
        .replace(
            "Approved-checksum:\n",
            &format!("Approved-checksum: sha256:{}\n", "ab".repeat(32)),
        );
    assert_eq!(plan.to_text(), expected);
}

#[test]
fn an_approval_keeps_line_endings_and_inline_comments() {
    let original = replace(
        &valid().replace('\n', "\r\n"),
        "Approved-by:\r\n",
        "Approved-by: # filled by plan approve\r\n",
    );
    let mut plan = Plan::parse(&original, Mode::Light).unwrap();
    plan.approve(approval()).unwrap();
    let text = plan.to_text();

    assert!(
        text.contains("Approved-by: rade # filled by plan approve\r\n"),
        "{text}"
    );
    assert!(text.contains("Status: approved\r\n"), "{text}");
    assert!(
        !text.replace("\r\n", "").contains('\n'),
        "an LF crept into a CRLF file"
    );
    // And it reads back as what was written.
    let reread = Plan::parse(&text, Mode::Light).unwrap();
    assert_eq!(reread.header().approved_by.as_deref(), Some("rade"));
}

#[test]
fn an_approval_leaves_the_body_byte_identical() {
    let mut plan = parse(&valid());
    let body = plan.body().to_owned();
    plan.approve(approval()).unwrap();
    assert_eq!(plan.body(), body);
    let reread = parse(&plan.to_text());
    assert_eq!(reread.body(), body);
    assert_eq!(reread.header().status, Status::Approved);
}

#[test]
fn reset_to_draft_restores_the_creation_form() {
    let original = valid();
    let mut plan = parse(&original);
    plan.approve(approval()).unwrap();
    plan.reset_to_draft();
    // Back to exactly what the agent wrote.
    assert_eq!(plan.to_text(), original);
    assert_eq!(plan.header().status, Status::Draft);
    assert_eq!(plan.header().approved_checksum, None);
}

#[test]
fn reset_to_draft_clears_a_lockfile_observation_but_keeps_its_key() {
    let original = replace(
        &valid(),
        "Approved-checksum:\n",
        "Approved-checksum:\nLockfile-checksum:\n",
    );
    let approved = original
        .replace("Status: draft", "Status: approved")
        .replace("Approved-by:", "Approved-by: rade")
        .replace("Approved-at:", "Approved-at: 2026-10-05")
        .replace(
            "Approved-checksum:",
            &format!("Approved-checksum: sha256:{}", "cd".repeat(32)),
        )
        .replace(
            "Lockfile-checksum:",
            &format!("Lockfile-checksum: sha256:{}", "ef".repeat(32)),
        );
    let mut plan = parse(&approved);
    assert!(plan.header().lockfile_checksum.is_some());
    plan.reset_to_draft();
    assert_eq!(plan.to_text(), original);
}

// --- `Acceptance` (§5, §5.1) --------------------------------------------------

#[test]
fn acceptance_may_be_absent_or_empty_and_both_mean_not_decided() {
    assert_eq!(parse(&valid()).header().acceptance, None);
    let empty = replace(
        &valid(),
        "Approved-checksum:\n",
        "Approved-checksum:\nAcceptance:\n",
    );
    assert_eq!(parse(&empty).header().acceptance, None);
    assert_eq!(
        parse(&empty).header().recorded_acceptance(),
        Acceptance::Skipped
    );
}

#[test]
fn acceptance_reads_both_values() {
    for (value, expected) in [
        ("required", Acceptance::Required),
        ("skipped", Acceptance::Skipped),
    ] {
        let text = replace(
            &valid(),
            "Approved-checksum:\n",
            &format!("Approved-checksum:\nAcceptance: {value}\n"),
        );
        assert_eq!(parse(&text).header().acceptance, Some(expected));
        assert_eq!(parse(&text).header().recorded_acceptance(), expected);
    }
}

#[test]
fn any_other_acceptance_value_is_an_error_naming_the_line() {
    for value in ["optional", "Required", "SKIPPED", "yes"] {
        let text = replace(
            &valid(),
            "Approved-checksum:\n",
            &format!("Approved-checksum:\nAcceptance: {value}\n"),
        );
        let problems = Plan::parse(&text, Mode::Light).unwrap_err();
        let line = text
            .lines()
            .position(|l| l.starts_with("Acceptance:"))
            .unwrap()
            + 1;
        let rendered = problems.render(Path::new("spec.md"));
        assert!(
            rendered.contains(&format!("spec.md:{line}: `Acceptance` must be `required`"))
                && rendered.contains(&format!("found `{value}`")),
            "{rendered}"
        );
    }
}

#[test]
fn reset_to_draft_clears_acceptance_but_keeps_its_key() {
    let original = replace(
        &valid(),
        "Approved-checksum:\n",
        "Approved-checksum:\nAcceptance:\n",
    );
    let approved = original
        .replace("Status: draft", "Status: approved")
        .replace("Approved-by:", "Approved-by: rade")
        .replace("Approved-at:", "Approved-at: 2026-10-05")
        .replace(
            "Approved-checksum:",
            &format!("Approved-checksum: sha256:{}", "cd".repeat(32)),
        )
        .replace("Acceptance:", "Acceptance: required");
    let mut plan = parse(&approved);
    plan.reset_to_draft();
    assert_eq!(plan.to_text(), original);
    assert_eq!(plan.header().acceptance, None);
}

#[test]
fn reset_to_draft_adds_no_acceptance_key() {
    let approved = valid()
        .replace("Status: draft", "Status: approved")
        .replace("Approved-by:", "Approved-by: rade")
        .replace("Approved-at:", "Approved-at: 2026-10-05")
        .replace(
            "Approved-checksum:",
            &format!("Approved-checksum: sha256:{}", "cd".repeat(32)),
        );
    let mut plan = parse(&approved);
    plan.reset_to_draft();
    assert_eq!(plan.to_text(), valid());
}

#[test]
fn an_approval_that_cannot_be_written_back_is_refused() {
    let mut plan = parse(&valid());
    for bad in [
        Approval {
            by: String::new(),
            ..approval()
        },
        Approval {
            by: "rade # x".to_owned(),
            ..approval()
        },
        Approval {
            by: "rade\n".to_owned(),
            ..approval()
        },
        Approval {
            at: "2026-02-30".to_owned(),
            ..approval()
        },
        Approval {
            checksum: "sha256:ABC".to_owned(),
            ..approval()
        },
    ] {
        assert!(plan.approve(bad.clone()).is_err(), "{bad:?}");
    }
    assert_eq!(
        plan.to_text(),
        valid(),
        "a refused approval changed the file"
    );
}

// --- delimiters -------------------------------------------------------------

#[test]
fn the_first_line_must_be_exactly_the_delimiter() {
    let text = valid();
    for (broken, hint) in [
        (format!("\n{text}"), "remove the blank lines"),
        (format!("--- \n{}", &text[4..]), "remove the spaces"),
        (format!("# Plan\n{text}"), "starts with its header"),
    ] {
        let found = messages(&broken, Mode::Light);
        assert_eq!(found.len(), 1, "{found:#?}");
        assert!(
            found[0].starts_with("line 1: the first line must be exactly `---`"),
            "{found:#?}"
        );
        assert!(found[0].contains(hint), "{found:#?}");
    }
}

#[test]
fn a_bom_before_the_delimiter_is_allowed() {
    let plan = parse(&format!("\u{FEFF}{}", valid()));
    assert_eq!(plan.header().ticket, "SHOP-214");
}

#[test]
fn an_empty_file_and_an_unclosed_header_are_errors() {
    assert!(messages("", Mode::Light)[0].contains("empty"));
    let unclosed = "---\nTicket: SHOP-1\nModule: orders\n";
    let found = messages(unclosed, Mode::Light);
    assert!(found[0].contains("never closed"), "{found:#?}");
}

#[test]
fn only_the_first_two_delimiters_count() {
    // A `---` in the body is a horizontal rule, not a third delimiter.
    let text = replace(&valid(), "## Intent\n", "---\n\n## Intent\n");
    let plan = parse(&text);
    assert!(plan.section("Ticket").unwrap().contains("---"));
    assert_eq!(plan.to_text(), text);
}

// --- header -----------------------------------------------------------------

#[test]
fn status_accepts_only_draft_and_approved() {
    for bad in ["Draft", "APPROVED", "pending", ""] {
        let text = replace(&valid(), "Status: draft\n", &format!("Status: {bad}\n"));
        let found = messages(&text, Mode::Light);
        assert!(
            found
                .iter()
                .any(|m| m.contains("`Status` must be `draft` or `approved`")),
            "{bad:?}: {found:#?}"
        );
    }
}

#[test]
fn the_three_approval_keys_must_be_present_even_when_empty() {
    let mut text = valid();
    for key in ["Approved-by:\n", "Approved-at:\n", "Approved-checksum:\n"] {
        text = replace(&text, key, "");
    }
    let found = messages(&text, Mode::Light);
    for key in ["Approved-by", "Approved-at", "Approved-checksum"] {
        assert!(
            found
                .iter()
                .any(|m| m.contains(&format!("no `{key}` key")) && m.contains("empty value")),
            "{key}: {found:#?}"
        );
    }
    assert_eq!(found.len(), 3, "{found:#?}");
}

#[test]
fn lockfile_checksum_may_be_absent_empty_or_a_digest() {
    // Absent: example 9.1 itself.
    parse(&valid());
    let empty = replace(
        &valid(),
        "Approved-checksum:\n",
        "Approved-checksum:\nLockfile-checksum:\n",
    );
    assert_eq!(parse(&empty).header().lockfile_checksum, None);
    let malformed = replace(
        &valid(),
        "Approved-checksum:\n",
        "Approved-checksum:\nLockfile-checksum: md5:abc\n",
    );
    assert!(messages(&malformed, Mode::Light)[0].contains("`Lockfile-checksum` must be `sha256:`"));
}

#[test]
fn values_are_checked_against_the_contract() {
    for (from, to, expected) in [
        (
            "Ticket: SHOP-214",
            "Ticket: shop-214",
            "`Ticket` must look like `PROJ-412`",
        ),
        (
            "Ticket: SHOP-214",
            "Ticket: SHOP214",
            "`Ticket` must look like",
        ),
        (
            "Module: orders",
            "Module: or-ders",
            "`Module` must be a module name",
        ),
        (
            "Approved-at:",
            "Approved-at: 05.10.2026",
            "`Approved-at` must be a date",
        ),
        (
            "Approved-at:",
            "Approved-at: 2026-02-29",
            "`Approved-at` must be a date",
        ),
        (
            "Approved-checksum:",
            "Approved-checksum: sha256:ABC",
            "64 lowercase hex",
        ),
    ] {
        let found = messages(&replace(&valid(), from, to), Mode::Light);
        assert!(
            found.iter().any(|m| m.contains(expected)),
            "{to}: {found:#?}"
        );
    }
    // A leap day that exists is fine.
    parse(&replace(
        &valid(),
        "Approved-at:",
        "Approved-at: 2028-02-29",
    ));
}

#[test]
fn unknown_keys_are_preserved_and_a_case_slip_is_pointed_out() {
    // `ticket` is an unknown key — permitted (D-2) — so the error is the
    // missing `Ticket`, with a hint at the likely cause.
    let found = messages(
        &replace(&valid(), "Ticket: SHOP-214", "ticket: SHOP-214"),
        Mode::Light,
    );
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(found[0].contains("no `Ticket` key"), "{found:#?}");
    assert!(
        found[0].contains("Line 2 has `ticket`: keys are case-sensitive"),
        "{found:#?}"
    );

    // Next to the real key, it is just an unknown key.
    let both = replace(
        &valid(),
        "Module: orders\n",
        "Module: orders\nticket: old-value\n",
    );
    assert_eq!(parse(&both).to_text(), both);
}

#[test]
fn a_duplicated_key_is_an_error() {
    let text = replace(
        &valid(),
        "Module: orders\n",
        "Module: orders\nModule: billing\n",
    );
    let found = messages(&text, Mode::Light);
    assert_eq!(
        found,
        ["line 4: `Module` appears twice (first on line 3); keep one of them"]
    );
}

#[test]
fn header_lines_must_be_flat_key_value_lines() {
    for (line, expected) in [
        ("  Indented: x", "must not be indented"),
        ("- a list item", "not a `Key: value` line"),
        ("Just words", "not a `Key: value` line"),
        ("Key:value", "put a space after the colon"),
        ("Notes: |", "plain one-line values only"),
        ("Tags: [a, b]", "plain one-line values only"),
    ] {
        let text = replace(
            &valid(),
            "Status: draft\n",
            &format!("Status: draft\n{line}\n"),
        );
        let found = messages(&text, Mode::Light);
        assert!(
            found
                .iter()
                .any(|m| m.starts_with("line 5: ") && m.contains(expected)),
            "{line}: {found:#?}"
        );
    }
}

#[test]
fn an_inline_comment_is_not_part_of_the_value() {
    let text = replace(
        &valid(),
        "Ticket: SHOP-214",
        "Ticket: SHOP-214   # from Jira",
    );
    assert_eq!(parse(&text).header().ticket, "SHOP-214");
    // A `#` inside a value without a space before it is the value.
    let text = replace(&valid(), "Approved-by:", "Approved-by: team#3");
    assert_eq!(parse(&text).header().approved_by.as_deref(), Some("team#3"));
}

// --- body boundary ----------------------------------------------------------

#[test]
fn the_body_starts_at_the_first_non_blank_line() {
    let plan = parse(&valid());
    assert!(
        plan.body().starts_with("## Ticket\n"),
        "{:?}",
        &plan.body()[..20]
    );
    assert!(plan.body().ends_with("`withoutGlobalScopes()`\n"));
}

#[test]
fn blank_lines_between_header_and_body_do_not_change_the_body() {
    // D-1: a cosmetic edit must not invalidate an approval.
    let reference = parse(&valid()).body().to_owned();
    for gap in [
        "---\n## Ticket",
        "---\n\n\n\n## Ticket",
        "---\n  \n\t\n## Ticket",
    ] {
        let text = replace(&valid(), "---\n\n## Ticket", gap);
        assert_eq!(parse(&text).body(), reference, "{gap:?}");
    }
}

// --- required sections ------------------------------------------------------

#[test]
fn every_missing_section_is_reported_in_one_run() {
    let mut text = valid();
    for heading in ["## Intent\n", "## Scope\n"] {
        text = replace(&text, heading, "");
    }
    text = text
        .split("## Implementation context\n")
        .next()
        .unwrap()
        .to_owned();
    // With their headings gone, the text joins the previous section; the last
    // section goes with its text, since list items that joined the criteria
    // would be criteria without ids. The problems left are exactly the three
    // missing headings.
    let found = messages(&text, Mode::Light);
    assert_eq!(
        found,
        [
            "missing required section `## Intent`; add it after `## Ticket`",
            "missing required section `## Scope`; add it after `## Intent`",
            "missing required section `## Implementation context`; add it after `## Acceptance criteria`",
        ]
    );
}

#[test]
fn misspelled_headings_are_reported_where_they_are() {
    let text = valid()
        .replacen("## Acceptance criteria", "## Acceptance Criteria", 1)
        .replacen("## Scope", "### Scope", 1)
        .replacen("## Intent", "##Intent", 1)
        .replacen(
            "## Open questions / answers",
            "## Open Questions/Answers",
            1,
        );
    let found = messages(&text, Mode::Light);
    assert_eq!(found.len(), 4, "{found:#?}");
    for (fragment, should_be) in [
        (
            "`## Acceptance Criteria` should be",
            "`## Acceptance criteria`",
        ),
        ("`### Scope` should be", "`## Scope`"),
        ("`##Intent` should be", "`## Intent`"),
        (
            "`## Open Questions/Answers` should be",
            "`## Open questions / answers`",
        ),
    ] {
        assert!(
            found
                .iter()
                .any(|m| m.starts_with("line ") && m.contains(fragment) && m.contains(should_be)),
            "{fragment}: {found:#?}"
        );
    }
}

#[test]
fn sections_out_of_order_are_reported() {
    // Swap Intent and Scope.
    let plan = parse(&valid());
    let intent = plan.section("Intent").unwrap().to_owned();
    let scope = plan.section("Scope").unwrap().to_owned();
    let text = replace(
        &valid(),
        &format!("## Intent\n{intent}## Scope\n{scope}"),
        &format!("## Scope\n{scope}## Intent\n{intent}"),
    );
    let found = messages(&text, Mode::Light);
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(
        found[0].contains("`## Intent` is out of order"),
        "{found:#?}"
    );
    assert!(found[0].contains("before `## Scope`"), "{found:#?}");
}

#[test]
fn a_repeated_required_section_is_reported() {
    let text = format!("{}\n## Intent\nMore intent.\n", valid());
    let found = messages(&text, Mode::Light);
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(found[0].contains("`## Intent` appears again"), "{found:#?}");
}

#[test]
fn additional_sections_are_allowed_anywhere() {
    let text = replace(
        &valid(),
        "## Scope\n",
        "## Notes for the reviewer\nSee the ticket.\n\n## Scope\n",
    );
    let plan = parse(&text);
    assert_eq!(
        plan.section("Notes for the reviewer"),
        Some("See the ticket.\n\n")
    );
}

#[test]
fn scenario_sections_are_ignored_in_light_mode() {
    // D-9: present, even empty, they are ordinary additional sections.
    let text = replace(
        &valid(),
        "## Implementation context\n",
        "## Scenarios to write\n\n## Scenarios to delete\n\n## Implementation context\n",
    );
    parse(&text);
}

#[test]
fn scenarios_mode_requires_its_own_list() {
    let found = messages(&valid(), Mode::Scenarios);
    assert!(
        found.iter().any(|m| m.contains("`## Scenarios to write`")),
        "{found:#?}"
    );
    assert!(
        found.iter().any(|m| m.contains("`## Scenarios to delete`")),
        "{found:#?}"
    );
}

#[test]
fn every_required_section_must_have_content() {
    let plan = parse(&valid());
    for heading in ["Intent", "Rejected alternatives", "Implementation context"] {
        let content = plan.section(heading).unwrap();
        let text = replace(
            &valid(),
            &format!("## {heading}\n{content}"),
            &format!("## {heading}\n\n"),
        );
        let found = messages(&text, Mode::Light);
        assert_eq!(found.len(), 1, "{heading}: {found:#?}");
        assert!(
            found[0].contains(&format!("`## {heading}` is empty")),
            "{found:#?}"
        );
    }
}

#[test]
fn none_does_not_count_as_acceptance_criteria() {
    let criteria = parse(&valid()).acceptance_criteria().unwrap().to_owned();
    let text = replace(&valid(), &criteria, "_none_\n\n");
    let found = messages(&text, Mode::Light);
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(
        found[0].contains("`_none_` is not accepted here"),
        "{found:#?}"
    );
}

// --- what counts as content (plan-header.md D-14) ---------------------------

/// Example 9.1 with its acceptance criteria replaced by `content`.
fn with_criteria(content: &str) -> String {
    let criteria = parse(&valid()).acceptance_criteria().unwrap().to_owned();
    replace(&valid(), &criteria, &format!("{content}\n\n"))
}

/// `content` as the acceptance criteria is rejected, and as nothing else than
/// an empty section.
fn assert_criteria_rejected(content: &str) {
    let found = messages(&with_criteria(content), Mode::Light);
    assert_eq!(found.len(), 1, "{content:?}: {found:#?}");
    assert!(
        found[0].contains("`## Acceptance criteria` is empty"),
        "{content:?}: {found:#?}"
    );
}

#[test]
fn placeholders_do_not_count_as_acceptance_criteria() {
    for content in [
        "_none_",
        "none",
        "None.",
        "N/A",
        "n/a",
        "TBD",
        "**TBD**",
        "- TBD",
        "* todo",
        "1. TODO",
        // Several placeholders are still no criterion.
        "TBD\n\n_none_\n\nn/a",
    ] {
        assert_criteria_rejected(content);
    }
}

#[test]
fn markers_and_comments_do_not_count_as_acceptance_criteria() {
    for content in [
        "-",
        "- ",
        "*",
        "+",
        "1.",
        "2)",
        "- [ ]",
        "- [x]",
        "- - -",
        "-\n-\n-",
        "...",
        "\u{2026}",
        "<!-- fill in -->",
        "- <!-- one criterion per item -->",
        "<!-- fill in\nlater -->",
        // Never closed: everything after it is hidden, as when rendered.
        "<!-- never closed\n- Given a paid order, when I cancel it, then it is refused.",
    ] {
        assert_criteria_rejected(content);
    }
}

#[test]
fn real_criteria_are_accepted_in_every_list_form() {
    for content in [
        "- AC1: Given a paid order, when I cancel it, then it is refused.",
        "* AC1: Given a paid order, when I cancel it, then it is refused.",
        "1. AC1: Given a paid order, when I cancel it, then it is refused.",
        "- [ ] AC1: Given a paid order, when I cancel it, then it is refused.",
        "<!-- one criterion per item -->\n- AC1: Given a paid order, when I cancel it, then it is refused.",
        "<!-- note --> - AC1: Given a paid order, when I cancel it, then it is refused.",
        // A placeholder next to a real criterion does not cancel it.
        "TBD\n- AC1: Given a paid order, when I cancel it, then it is refused.",
        // Not English, and not ASCII.
        "- AC1: Ako je narud\u{17E}bina pla\u{107}ena, zahtev se odbija sa 403.",
        // A number or a `#` inside the text is just text.
        "- AC1: Order #214 is refused, not cancelled.",
        // Starting with a placeholder word is not being one.
        "- AC1: None of the unpaid orders is deleted.",
        "- AC1: 1.5 seconds after the click the button is disabled.",
        // A wrapped item keeps its one id, however it is indented.
        "- AC1: Given a paid order,\n  when I cancel it, then it is refused.",
        "- AC1: Given a paid order,\n\twhen I cancel it, then it is refused.",
        // Blank lines and nested detail stay with the item above.
        "- AC1: Given a paid order,\n\n  when I cancel it, then:\n  - it is refused\n- AC2: Given x, then y.",
        // A wrapped line may start with anything but a list item's id.
        "- AC1: Given a paid order,\n  ac2 is its code, and it is refused.",
        // An item's own margin of one space.
        " - AC1: Given x, then y.\n - AC2: Given y, then z.",
    ] {
        Plan::parse(&with_criteria(content), Mode::Light)
            .unwrap_or_else(|problems| panic!("{content:?} was refused:\n{problems}"));
    }
}

#[test]
fn a_comment_is_not_content_in_any_required_section() {
    let plan = parse(&valid());
    for heading in ["Intent", "Scope", "Implementation context"] {
        let content = plan.section(heading).unwrap();
        let text = replace(
            &valid(),
            &format!("## {heading}\n{content}"),
            &format!("## {heading}\n<!-- todo -->\n\n"),
        );
        let found = messages(&text, Mode::Light);
        assert_eq!(found.len(), 1, "{heading}: {found:#?}");
        assert!(
            found[0].contains(&format!("`## {heading}` is empty")),
            "{heading}: {found:#?}"
        );
    }
}

#[test]
fn placeholders_are_a_rule_of_the_acceptance_criteria_only() {
    // "None." is a fine answer to "were there open questions?" (§6.1).
    let plan = parse(&valid());
    let content = plan.section("Open questions / answers").unwrap();
    let text = replace(
        &valid(),
        &format!("## Open questions / answers\n{content}"),
        "## Open questions / answers\nNone.\n\n",
    );
    parse(&text);
}

// --- edges ------------------------------------------------------------------

#[test]
fn a_heading_on_the_last_line_without_a_line_ending_is_an_empty_section() {
    let text = format!("{}\n## Notes", valid());
    let plan = parse(&text);
    assert_eq!(plan.section("Notes"), Some(""));
    assert_eq!(plan.to_text(), text);
}

#[test]
fn an_empty_header_reports_every_missing_key() {
    let found = messages("---\n---\n", Mode::Light);
    let missing = found
        .iter()
        .filter(|m| m.contains("the header has no `"))
        .count();
    assert_eq!(missing, 6, "{found:#?}");
}

#[test]
fn a_fence_is_closed_only_by_a_bare_fence_line() {
    // The inner ```php line has an info string, so it cannot close the outer
    // fence: the heading after it is still part of the example.
    let text = replace(
        &valid(),
        "## Scope\n",
        "Example:\n\n```markdown\n```php\n## Implementation context\n## Scope\n```\n\n## Scope\n",
    );
    let plan = parse(&text);
    assert!(plan.section("Intent").unwrap().contains("```php"));
    assert_eq!(plan.sections().len(), 7);
}

#[test]
fn a_heading_inside_a_code_fence_is_not_a_heading() {
    let text = replace(
        &valid(),
        "## Scope\n",
        "Example:\n\n```markdown\n## Implementation context\n## Scope\n```\n\n## Scope\n",
    );
    let plan = parse(&text);
    assert!(plan.section("Intent").unwrap().contains("```markdown"));
    assert_eq!(plan.sections().len(), 7);
}

#[test]
fn header_and_section_problems_are_reported_together() {
    let text = replace(
        &replace(&valid(), "Status: draft", "Status: done"),
        "## Rejected alternatives",
        "## Rejected Alternatives",
    );
    let found = messages(&text, Mode::Light);
    assert_eq!(found.len(), 2, "{found:#?}");
    assert!(found[0].starts_with("line 4: `Status`"), "{found:#?}");
    assert!(
        found[1].contains("`## Rejected Alternatives` should be"),
        "{found:#?}"
    );
}

// --- files ------------------------------------------------------------------

#[test]
fn read_approve_write_read() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("spec.md");
    fs::write(&path, valid()).unwrap();

    let mut plan = Plan::read(&path, Mode::Light).unwrap();
    plan.approve(approval()).unwrap();
    plan.write(&path).unwrap();

    let reread = Plan::read(&path, Mode::Light).unwrap();
    assert_eq!(reread.header().status, Status::Approved);
    assert_eq!(reread.body(), parse(&valid()).body());
    // The atomic write leaves nothing behind.
    let names: Vec<_> = fs::read_dir(dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(names, ["spec.md"]);
}

#[test]
fn an_invalid_file_names_its_path_and_every_problem() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("spec.md");
    fs::write(&path, replace(&valid(), "Status: draft", "Status: done")).unwrap();

    let err = Plan::read(&path, Mode::Light).unwrap_err();
    assert!(matches!(err, PlanError::Invalid { .. }), "{err:?}");
    let message = err.to_string();
    assert!(
        message.contains("is not a valid Plan (1 problem)"),
        "{message}"
    );
    assert!(
        message.contains(&format!("{}:4: `Status`", path.display())),
        "{message}"
    );
}

#[test]
fn invalid_utf8_and_a_missing_file_are_reported() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("spec.md");
    assert!(matches!(
        Plan::read(&path, Mode::Light),
        Err(PlanError::Unreadable { .. })
    ));
    fs::write(&path, b"---\nTicket: SHOP-1\xE9\n---\n").unwrap();
    let message = Plan::read(&path, Mode::Light).unwrap_err().to_string();
    assert!(message.contains("not valid UTF-8"), "{message}");
}

#[test]
fn the_mode_follows_scenarios_in_invar_yaml() {
    use crate::format::config::Config;
    let light = Config::default();
    assert_eq!(Mode::of(&light), Mode::Light);
    let scenarios = Config {
        scenarios: true,
        ..Config::default()
    };
    assert_eq!(Mode::of(&scenarios), Mode::Scenarios);
}

#[test]
fn the_body_digest_survives_a_crlf_checkout() {
    // What `plan approve` records and `plan check` recomputes (M1-9, M1-10):
    // a Plan approved on LF and checked out with `core.autocrlf=true` must
    // still match.
    use crate::digest::{canonicalize, digest};
    let lf = parse(&valid());
    let crlf = parse(&valid().replace('\n', "\r\n"));
    assert_ne!(lf.body(), crlf.body());
    assert_eq!(
        digest(&canonicalize(lf.body())),
        digest(&canonicalize(crlf.body()))
    );
}

// --- check: everything readable despite problems (M1-8) ---------------------

#[test]
fn check_of_a_valid_plan_has_no_problems_and_reads_ticket_and_status() {
    let checked = Plan::check(&valid(), Mode::Light);
    assert!(checked.problems.is_empty(), "{}", checked.problems);
    assert_eq!(checked.ticket, Some(("SHOP-214".to_owned(), 2)));
    assert_eq!(checked.status, Some(Status::Draft));
}

#[test]
fn check_reads_ticket_and_status_when_other_fields_are_wrong() {
    // A broken `Module` and a missing section: the Plan is invalid, but the
    // ticket and the status are still known, so `plan validate` can compare
    // the ticket with the directory in the same run.
    let text = valid()
        .replace("Module: orders", "Module: or-ders")
        .replace("Status: draft", "Status: approved")
        .replace("## Scope\n", "## Scoped\n");
    let checked = Plan::check(&text, Mode::Light);
    assert_eq!(checked.problems.len(), 2, "{}", checked.problems);
    assert_eq!(checked.ticket, Some(("SHOP-214".to_owned(), 2)));
    assert_eq!(checked.status, Some(Status::Approved));
    // `parse` agrees on the problems.
    assert_eq!(
        problems(&text, Mode::Light).to_string(),
        checked.problems.to_string()
    );
}

#[test]
fn check_does_not_report_a_ticket_or_status_that_is_malformed() {
    let text = valid()
        .replace("Ticket: SHOP-214", "Ticket: shop-214")
        .replace("Status: draft", "Status: Draft");
    let checked = Plan::check(&text, Mode::Light);
    assert_eq!(checked.problems.len(), 2, "{}", checked.problems);
    assert_eq!(checked.ticket, None);
    assert_eq!(checked.status, None);
}

#[test]
fn check_without_a_header_reports_only_that() {
    let checked = Plan::check("## Ticket\nno header\n", Mode::Light);
    assert_eq!(checked.problems.len(), 1);
    assert_eq!(checked.ticket, None);
    assert_eq!(checked.status, None);
}

// --- numbered criteria (plan-header.md §6.4, D-16, D-18) ---------------------

/// The 1-based line of the first line that contains `needle`.
fn line_of(text: &str, needle: &str) -> usize {
    text.lines()
        .position(|line| line.contains(needle))
        .unwrap_or_else(|| panic!("{needle:?} not in the fixture"))
        + 1
}

/// `content` as the criteria yields exactly one problem: on the line holding
/// `at`, and mentioning `what`.
fn assert_criteria_problem(content: &str, at: &str, what: &str) {
    let text = with_criteria(content);
    let found = messages(&text, Mode::Light);
    assert_eq!(found.len(), 1, "{content:?}: {found:#?}");
    let prefix = format!("line {}: ", line_of(&text, at));
    assert!(found[0].starts_with(&prefix), "{prefix}: {found:#?}");
    assert!(found[0].contains(what), "{what:?}: {found:#?}");
}

#[test]
fn a_criterion_without_an_id_is_refused_at_its_line() {
    assert_criteria_problem(
        "- AC1: Given x, then y.\n- Given z, then w.",
        "Given z",
        "this criterion has no id",
    );
}

#[test]
fn a_malformed_id_is_refused_naming_it() {
    for (content, token) in [
        ("- AC01: Given x, then y.", "`AC01:` is not a criterion id"),
        ("- ac1: Given x, then y.", "`ac1:` is not a criterion id"),
        ("- Ac1: Given x, then y.", "`Ac1:` is not a criterion id"),
        ("- AC1 Given x, then y.", "`AC1` is not a criterion id"),
        ("- AC1:Given x, then y.", "`AC1:` is not a criterion id"),
        ("- AC0: Given x, then y.", "`AC0:` is not a criterion id"),
        ("- AC 1: Given x, then y.", "`AC 1:` is not a criterion id"),
        ("- AC-1: Given x, then y.", "`AC-1:` is not a criterion id"),
        (
            "- AC99999999999999999999: Given x.",
            "`AC99999999999999999999:` is not a criterion id",
        ),
    ] {
        assert_criteria_problem(content, "Given x", token);
    }
}

#[test]
fn a_word_that_starts_like_an_id_is_only_a_missing_id() {
    assert_criteria_problem(
        "- Accepted orders are listed.",
        "Accepted",
        "this criterion has no id",
    );
}

#[test]
fn a_duplicate_id_is_refused_at_the_second_one() {
    let text = with_criteria("- AC1: First.\n- AC2: Second.\n- AC2: Third.");
    let found = messages(&text, Mode::Light);
    assert_eq!(
        found,
        [format!(
            "line {}: `AC2` appears again (first on line {}); each criterion has its own id — \
             renumber them (plan-header.md §6.4)",
            line_of(&text, "Third"),
            line_of(&text, "Second"),
        )]
    );
}

#[test]
fn a_gap_is_refused_at_the_id_after_it() {
    assert_criteria_problem(
        "- AC1: First.\n- AC3: Third.",
        "Third",
        "`AC3` follows `AC1`, so `AC2` is missing",
    );
    assert_criteria_problem(
        "- AC1: First.\n- AC5: Fifth.",
        "Fifth",
        "`AC5` follows `AC1`, so `AC2` to `AC4` are missing",
    );
    assert_criteria_problem(
        "- AC2: Second.",
        "Second",
        "`AC2` is the first id, so `AC1` is missing",
    );
}

#[test]
fn ids_out_of_order_are_refused_once_and_not_as_a_gap() {
    assert_criteria_problem(
        "- AC2: Second.\n- AC1: First.",
        "First",
        "`AC1` is out of order: it comes after `AC2`",
    );
}

#[test]
fn a_gap_a_bad_item_may_fill_is_not_reported_on_top_of_it() {
    assert_criteria_problem(
        "- AC1: First.\n- Second has none.\n- AC3: Third.",
        "Second",
        "this criterion has no id",
    );
    // One bad item cannot explain two missing ids.
    let found = messages(
        &with_criteria("- AC1: First.\n- Second has none.\n- AC4: Fourth."),
        Mode::Light,
    );
    assert_eq!(found.len(), 2, "{found:#?}");
}

#[test]
fn a_line_of_text_outside_a_list_item_is_refused() {
    for (content, at) in [
        ("Given x, then y.", "Given x"),
        ("The criteria:\n- AC1: Given x.", "The criteria"),
        // Not indented, so not a continuation.
        ("- AC1: Given x,\nwhen y.", "when y"),
        ("### Refusals\n- AC1: Given x.", "### Refusals"),
        // An id without a list marker, indented or not, is not swallowed.
        (
            "- AC1: Given x.\nAC2: Given y.\n- AC3: Given z.",
            "AC2: Given y.",
        ),
        (
            "- AC1: Given x.\n  AC2: Given y.\n- AC3: Given z.",
            "AC2: Given y.",
        ),
    ] {
        assert_criteria_problem(content, at, "every criterion is a numbered list item");
    }
}

#[test]
fn an_indented_item_with_an_id_is_a_criterion_and_not_a_continuation() {
    // Otherwise `AC3` would vanish into `AC1`, and the gap with it.
    assert_criteria_problem(
        "- AC1: First.\n  - AC3: Third.",
        "Third",
        "`AC2` is missing",
    );
    Plan::parse(
        &with_criteria("  - AC1: First.\n  - AC2: Second."),
        Mode::Light,
    )
    .unwrap();
}

#[test]
fn a_placeholder_after_an_id_is_still_a_placeholder() {
    for content in [
        "- AC1: TBD",
        "- AC1: **N/A.**",
        "- AC1: _none_",
        "- AC1:",
        "- AC1: ...",
        "- AC1: <!-- fill in -->",
        "- [ ] AC1: TODO",
    ] {
        assert_criteria_rejected(content);
    }
    // Beside a real criterion it is its own problem, not a silent gap.
    assert_criteria_problem(
        "- AC1: TBD\n- AC2: Given x, then y.",
        "AC1: TBD",
        "`AC1` has no criterion",
    );
}

#[test]
fn every_id_problem_is_reported_in_one_run() {
    let found = messages(
        &with_criteria("Intro.\n- AC1: A.\n- AC1: B.\n- ac2: C.\n- AC6: D.\n- AC3: E."),
        Mode::Light,
    );
    for needle in [
        "every criterion is a numbered",
        "`AC1` appears again",
        "`ac2:` is not",
        "`AC3` is out of order",
    ] {
        assert!(
            found.iter().any(|m| m.contains(needle)),
            "{needle}: {found:#?}"
        );
    }
}

#[test]
fn crlf_does_not_change_the_lines_or_the_verdict() {
    let text = with_criteria("- AC1: First.\n- AC3: Third.");
    let at = line_of(&text, "Third");
    let found = messages(&text.replace('\n', "\r\n"), Mode::Light);
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(
        found[0].starts_with(&format!("line {at}: `AC3` follows")),
        "{found:#?}"
    );
    parse(&valid().replace('\n', "\r\n"));
}

/// The body of `text` under the header of an approval.
fn as_approved(text: &str) -> String {
    let body = text.split_once("\n---\n").unwrap().1;
    contract_example("9.2") + body
}

const UNNUMBERED: &str = "- Given x, then y.\n- Given z, then w.\nA closing remark.";

#[test]
fn an_approved_plan_with_no_ids_at_all_is_valid_as_it_was_approved() {
    parse(&as_approved(&with_criteria(UNNUMBERED)));
    // `Status` is what decides: the same text as a draft is not valid.
    assert_eq!(messages(&with_criteria(UNNUMBERED), Mode::Light).len(), 3);
    // Malformed ids are no ids: 1.5.0 read them as prose.
    parse(&as_approved(&with_criteria("- ac1: Given x, then y.")));
}

#[test]
fn an_approved_plan_with_some_ids_is_held_to_every_rule() {
    for (content, what) in [
        ("- AC1: A.\n- AC3: B.", "`AC2` is missing"),
        ("- AC1: A.\n- AC1: B.", "`AC1` appears again"),
        ("- AC2: A.\n- AC1: B.", "out of order"),
        ("- AC1: A.\n- B has none.", "this criterion has no id"),
    ] {
        let found = messages(&as_approved(&with_criteria(content)), Mode::Light);
        assert_eq!(found.len(), 1, "{content:?}: {found:#?}");
        assert!(found[0].contains(what), "{what:?}: {found:#?}");
    }
}

#[test]
fn an_approved_plan_with_no_criteria_is_still_refused() {
    let found = messages(&as_approved(&with_criteria("TBD")), Mode::Light);
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(
        found[0].contains("`## Acceptance criteria` is empty"),
        "{found:#?}"
    );
}

#[test]
fn a_new_approval_is_held_to_the_rules_whatever_the_status_says() {
    let approved = as_approved(&with_criteria(UNNUMBERED));
    assert_eq!(Plan::problems_as_draft(&approved, Mode::Light).len(), 3);
    assert!(Plan::problems_as_draft(&valid(), Mode::Light).is_empty());
}

#[test]
fn a_plan_just_reset_to_draft_reads_back_with_its_body_as_it_was() {
    let mut plan = parse(&as_approved(&with_criteria(UNNUMBERED)));
    plan.reset_to_draft();
    let text = plan.to_text();
    assert!(Plan::parse(&text, Mode::Light).is_err());
    Plan::parse_reset(&text, Mode::Light).unwrap();
}

fn fixture_approved_under_1_5_0() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../contracts/fixtures/plan/approved-1.5.0-no-ids.md");
    fs::read_to_string(path).unwrap()
}

#[test]
fn the_plan_approved_under_1_5_0_is_valid_and_has_no_ids() {
    let plan = parse(&fixture_approved_under_1_5_0());
    assert_eq!(plan.header().status, Status::Approved);
    assert!(!plan.acceptance_criteria().unwrap().contains("AC1"));
}

#[test]
fn the_new_rules_do_not_apply_with_scenarios() {
    // Both are ordinary additional sections there (D-17).
    let text = contract_example("9.4").replace(
        "## Scenarios to delete\n",
        "## Scenarios to delete\n_none_\n\n## Acceptance criteria\nnot numbered\n\n\
         ## Acceptance tests to change\n\n## Implementation context\n- AI_CONTEXT: x\n",
    );
    Plan::parse(&text, Mode::Scenarios).unwrap();
}

// --- `## Acceptance tests to change` (plan-header.md §6.5, D-17) -------------

/// Example 9.1 with `content` as the section, right after the criteria.
fn with_tests_to_change(content: &str) -> String {
    replace(
        &valid(),
        "## Implementation context\n",
        &format!("## Acceptance tests to change\n{content}\n## Implementation context\n"),
    )
}

fn assert_tests_to_change_problem(content: &str, at: &str, what: &str) {
    let text = with_tests_to_change(content);
    let found = messages(&text, Mode::Light);
    assert_eq!(found.len(), 1, "{content:?}: {found:#?}");
    let prefix = format!("line {}: ", line_of(&text, at));
    assert!(found[0].starts_with(&prefix), "{prefix}: {found:#?}");
    assert!(found[0].contains(what), "{what:?}: {found:#?}");
}

#[test]
fn the_section_is_optional_and_its_valid_forms_pass() {
    parse(&valid());
    for content in [
        "- tests/A.php — change — the rule moved\n- tests/B.php — delete — no longer applies\n",
        "_none_\n",
        // Blank lines around it are nothing, and a comment is not an item.
        "\n- tests/A.php — change — the rule moved\n\n<!-- one more later -->\n",
        // Only the first two separators are separators.
        "- tests/A.php — change — the rule moved — see SHOP-9\n",
    ] {
        Plan::parse(&with_tests_to_change(content), Mode::Light)
            .unwrap_or_else(|problems| panic!("{content:?} was refused:\n{problems}"));
    }
}

#[test]
fn an_item_without_a_path_an_action_or_a_reason_is_refused_at_its_line() {
    for (content, what) in [
        ("- — change — why\n", "has no path"),
        ("-\n", "has no path"),
        ("- tests/A.php\n", "has no action"),
        ("- tests/A.php —  — why\n", "has no action"),
        ("- tests/A.php — change\n", "has no reason"),
        ("- tests/A.php — change —\n", "has no reason"),
        ("- tests/A.php — delete — TBD\n", "has no reason"),
    ] {
        let text = with_tests_to_change(content);
        let line = line_of(&text, "## Acceptance tests to change") + 1;
        let found = messages(&text, Mode::Light);
        assert_eq!(found.len(), 1, "{content:?}: {found:#?}");
        assert!(
            found[0].starts_with(&format!("line {line}: ")),
            "{found:#?}"
        );
        assert!(found[0].contains(what), "{what:?}: {found:#?}");
    }
}

#[test]
fn an_item_missing_two_parts_reports_both() {
    let found = messages(&with_tests_to_change("- tests/A.php —\n"), Mode::Light);
    assert_eq!(found.len(), 2, "{found:#?}");
    assert!(found[0].contains("has no action"), "{found:#?}");
    assert!(found[1].contains("has no reason"), "{found:#?}");
}

#[test]
fn an_unknown_action_is_refused_naming_it() {
    for action in ["rewrite", "Change", "DELETE", "remove"] {
        assert_tests_to_change_problem(
            &format!("- tests/A.php — {action} — why\n"),
            "tests/A.php",
            &format!("unknown action `{action}`"),
        );
    }
}

#[test]
fn the_wrong_separators_and_wrapped_items_are_refused() {
    assert_tests_to_change_problem(
        "- tests/A.php - change - why\n",
        "tests/A.php",
        "the separators are a space, an em dash and a space",
    );
    assert_tests_to_change_problem(
        "tests/A.php — change — why\n",
        "tests/A.php",
        "one unindented line",
    );
    assert_tests_to_change_problem(
        "- tests/A.php — change — why it\n  has to move\n",
        "has to move",
        "one unindented line",
    );
}

#[test]
fn an_empty_section_is_refused_at_its_heading() {
    for content in ["", "<!-- later -->\n", "...\n"] {
        assert_tests_to_change_problem(
            content,
            "## Acceptance tests to change",
            "`## Acceptance tests to change` is empty",
        );
    }
}

#[test]
fn none_is_the_single_line_or_it_is_an_error() {
    assert_tests_to_change_problem(
        "_none_\n- tests/A.php — change — why\n",
        "_none_",
        "`_none_` is the single line",
    );
}

#[test]
fn the_section_appears_once_in_its_place_spelled_exactly() {
    let item = "- tests/A.php — change — why\n";

    let twice = with_tests_to_change(&format!("{item}\n## Acceptance tests to change\n{item}"));
    let found = messages(&twice, Mode::Light);
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(found[0].contains("appears again"), "{found:#?}");

    let misplaced = replace(
        &valid(),
        "## Acceptance criteria\n",
        &format!("## Acceptance tests to change\n{item}\n## Acceptance criteria\n"),
    );
    let found = messages(&misplaced, Mode::Light);
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(found[0].contains("is out of order"), "{found:#?}");

    let misspelled = with_tests_to_change(item).replace(
        "## Acceptance tests to change",
        "## Acceptance Tests To Change",
    );
    let found = messages(&misspelled, Mode::Light);
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(
        found[0].contains("should be `## Acceptance tests to change`"),
        "{found:#?}"
    );
}

#[test]
fn an_additional_section_may_stand_between_it_and_the_criteria() {
    let text = replace(
        &valid(),
        "## Implementation context\n",
        "## Notes\nAnything.\n\n## Acceptance tests to change\n_none_\n\n\
         ## Implementation context\n",
    );
    parse(&text);
}

// --- the contract's examples of 1.6.0 ----------------------------------------

#[test]
fn contract_9_6_is_valid() {
    let plan = parse(&contract_example("9.6"));
    assert!(plan.section("Acceptance tests to change").is_some());
}

#[test]
fn contract_9_7_is_refused_for_its_gap() {
    let excerpt = contract_example("9.7");
    let content = excerpt
        .split_once("## Acceptance criteria\n")
        .unwrap()
        .1
        .trim_end();
    assert_criteria_problem(content, "AC3:", "`AC2` is missing");
}

#[test]
fn contract_9_8_is_refused_for_its_action() {
    let excerpt = contract_example("9.8");
    let content = excerpt
        .split_once("## Acceptance tests to change\n")
        .unwrap()
        .1;
    assert_tests_to_change_problem(content, "rewrite", "unknown action `rewrite`");
}

#[test]
fn contract_9_9_quotes_the_fixture() {
    let excerpt = contract_example("9.9");
    let criterion = excerpt
        .split_once("## Acceptance criteria\n")
        .unwrap()
        .1
        .trim_end();
    assert!(fixture_approved_under_1_5_0().contains(criterion));
}

// --- an id written with emphasis (plan-header.md §6.4, D-18) ------------------

#[test]
fn an_emphasised_id_is_refused_naming_it_at_its_line() {
    for (content, token) in [
        ("- **AC1:** Given x, then y.", "`**AC1:**`"),
        ("- **AC1**: Given x, then y.", "`**AC1**:`"),
        ("- *AC1:* Given x, then y.", "`*AC1:*`"),
        ("- *AC1*: Given x, then y.", "`*AC1*:`"),
        ("- _AC1_: Given x, then y.", "`_AC1_:`"),
        ("- __AC1:__ Given x, then y.", "`__AC1:__`"),
        ("- `AC1:` Given x, then y.", "``AC1:``"),
        ("- `AC1`: Given x, then y.", "``AC1`:`"),
        ("- ***AC1:*** Given x, then y.", "`***AC1:***`"),
        ("- [ ] **AC1:** Given x, then y.", "`**AC1:**`"),
        // No colon, the wrong case, or emphasis over the whole criterion.
        ("- **AC1** Given x, then y.", "`**AC1**`"),
        ("- **ac1:** Given x, then y.", "`**ac1:**`"),
        ("- **AC1: Given x, then y.**", "`**AC1:`"),
    ] {
        assert_criteria_problem(
            content,
            "Given x",
            &format!("{token} is an id written with"),
        );
    }
}

#[test]
fn emphasis_in_the_text_of_a_criterion_stays_fine() {
    for content in [
        "- AC1: Given a **paid** order, when I cancel it, then it is refused.",
        "- AC1: **Given** a paid order, `when` I cancel it, then *it* is refused.",
        "- AC1: _AC2_ is only a name here.",
        "- AC1: Given a paid order,\n  **AC2** is its code, and it is refused.",
    ] {
        Plan::parse(&with_criteria(content), Mode::Light)
            .unwrap_or_else(|problems| panic!("{content:?} was refused:\n{problems}"));
    }
}

#[test]
fn emphasis_with_no_id_inside_it_is_only_a_missing_id() {
    for (content, at) in [
        ("- **Given** x, then y.", "**Given**"),
        ("- `AC` is shown.", "`AC` is shown"),
        ("- _ACME_ is shown.", "_ACME_"),
    ] {
        assert_criteria_problem(content, at, "this criterion has no id");
    }
}

#[test]
fn an_emphasised_id_fills_a_gap() {
    assert_criteria_problem(
        "- AC1: First.\n- **AC2:** Second.\n- AC3: Third.",
        "Second",
        "`**AC2:**` is an id written with emphasis",
    );
}

#[test]
fn an_emphasised_id_counts_as_no_id_in_an_approved_plan() {
    parse(&as_approved(&with_criteria("- **AC1:** Given x, then y.")));
    // Beside a valid id the Plan is numbered, and it is an error like any other.
    let found = messages(
        &as_approved(&with_criteria("- AC1: First.\n- **AC2:** Second.")),
        Mode::Light,
    );
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(found[0].contains("written with emphasis"), "{found:#?}");
}

// --- which Plans show `## Acceptance tests to change` --------------------------

#[test]
fn the_tests_to_change_are_there_only_when_the_plan_lists_some() {
    assert_eq!(parse(&valid()).acceptance_tests_to_change(), None);
    for none in ["_none_\n", "\n_none_\n\n", "<!-- nothing --> _none_\n"] {
        assert_eq!(
            parse(&with_tests_to_change(none)).acceptance_tests_to_change(),
            None,
            "{none:?}"
        );
    }
    let item = "- tests/A.php — change — why\n";
    let text = with_tests_to_change(item);
    assert!(
        parse(&text)
            .acceptance_tests_to_change()
            .is_some_and(|content| content.contains(item))
    );
    let none_crlf = with_tests_to_change("_none_\n").replace('\n', "\r\n");
    assert_eq!(parse(&none_crlf).acceptance_tests_to_change(), None);
}

#[test]
fn the_tests_to_change_are_ignored_with_scenarios() {
    // An ordinary additional section there (D-17), whatever it holds.
    let text = contract_example("9.4").replace(
        "## Scenarios to delete\n",
        "## Scenarios to delete\n_none_\n\n## Acceptance tests to change\nanything\n\n\
         ## Implementation context\n- AI_CONTEXT: x\n",
    );
    let plan = Plan::parse(&text, Mode::Scenarios).unwrap();
    assert_eq!(plan.acceptance_tests_to_change(), None);
}
