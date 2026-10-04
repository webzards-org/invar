//! Tests for Plan parsing and writing (M1-6), against
//! `contracts/formats/plan-header.md` v1.2.0.
//!
//! The valid Plan used throughout is the contract's own example 9.1, read out
//! of the Markdown, so the contract and the parser cannot drift apart.

use std::fs;
use std::path::Path;

use super::{Approval, Mode, Plan, PlanError, Status};
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
        criteria.starts_with("- Given a pending, unpaid order of mine"),
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
            "Module: Orders",
            "`Module` must be a lowercase module name",
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
    for heading in ["## Intent\n", "## Scope\n", "## Implementation context\n"] {
        text = replace(&text, heading, "");
    }
    // With their headings gone, the text joins the previous section: the
    // problems left are exactly the three missing headings.
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
