//! Tests for decision entries (M1-11), against
//! `contracts/formats/decision-entry.md` v1.4.0. The examples are read out of
//! the contract itself, so the two cannot drift apart.

use std::fs;
use std::path::Path;

use super::{Entry, FileName, Type, check, is_id, normalize_body, parse, utc_stamp_of};
use crate::format::plan::Acceptance;

fn contract() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../contracts/formats/decision-entry.md");
    fs::read_to_string(&path).unwrap_or_else(|err| panic!("reading {}: {err}", path.display()))
}

/// Example `### <number> `: its file name and its text.
fn example(number: &str) -> (String, String) {
    let text = contract();
    let after = text
        .split_once(&format!("### {number} "))
        .unwrap_or_else(|| panic!("the contract has no example {number}"))
        .1;
    let name = after
        .split_once("File: `.invar/decisions/")
        .unwrap()
        .1
        .split_once('`')
        .unwrap()
        .0
        .to_owned();
    let body = after.split_once("```markdown\n").unwrap().1;
    let block = body.split_once("\n```\n").unwrap().0.to_owned() + "\n";
    (name, block)
}

fn problems(name: &str, text: &str) -> String {
    match check(name, text) {
        Ok(entry) => panic!("expected problems, got {entry:?}"),
        Err(problems) => problems.to_string(),
    }
}

fn sample() -> Entry {
    Entry {
        id: "d-7c41e09a".into(),
        date: "2026-10-06T18:37:12+02:00".into(),
        ticket: "SHOP-214".into(),
        module: "orders".into(),
        kind: Type::Failed,
        scenario: None,
        acceptance: None,
        body: "Line one.\n\nLine three.".into(),
    }
}

// --- the contract's examples -------------------------------------------------------

#[test]
fn example_12_1_is_valid() {
    let (name, text) = example("12.1");
    let entry = check(&name, &text).unwrap_or_else(|p| panic!("{p}"));
    assert_eq!(entry.id, "d-7c41e09a");
    assert_eq!(entry.ticket, "SHOP-214");
    assert_eq!(entry.module, "orders");
    assert_eq!(entry.kind, Type::Failed);
    assert_eq!(entry.scenario, None);
    assert!(entry.body.starts_with("The approved Plan requires"));
    assert!(entry.body.ends_with("drop the criterion."));
}

#[test]
fn example_12_1_is_exactly_what_the_core_writes() {
    let (_, text) = example("12.1");
    assert_eq!(parse(&text).unwrap().render(), text);
}

#[test]
fn example_12_2_reports_the_missing_id_and_the_unknown_type() {
    let (name, text) = example("12.2");
    let found = problems(&name, &text);
    assert!(found.contains("has no `id`"), "{found}");
    assert!(found.contains("`type` must be one of"), "{found}");
    assert!(found.contains("found `fixed`"), "{found}");
    assert_eq!(found.lines().count(), 2, "{found}");
}

#[test]
fn example_12_3_reports_the_module_mismatch_and_the_empty_body() {
    let (name, text) = example("12.3");
    let found = problems(&name, &text);
    assert!(found.contains("the body is empty"), "{found}");
    // The body problem stops the entry from parsing, so the filename check
    // runs on what is there: with a body, the mismatch alone is reported.
    let with_body = text.clone() + "\nSomething.\n";
    let found = problems(&name, &with_body);
    assert!(
        found.contains("the filename says module `payments`, the front matter says `orders`"),
        "{found}"
    );
}

// --- writing ----------------------------------------------------------------------

#[test]
fn render_and_parse_round_trip() {
    let entry = sample();
    assert_eq!(parse(&entry.render()).unwrap(), entry);
    let with_scenario = Entry {
        scenario: Some("reserve-stock".into()),
        ..sample()
    };
    let text = with_scenario.render();
    assert!(
        text.contains("type: failed\nscenario: reserve-stock\n---\n"),
        "{text}"
    );
    assert_eq!(parse(&text).unwrap(), with_scenario);
}

#[test]
fn the_rendered_file_ends_with_exactly_one_newline() {
    let text = sample().render();
    assert!(
        text.ends_with("Line three.\n") && !text.ends_with("\n\n"),
        "{text:?}"
    );
    assert!(text.starts_with("---\nid: d-7c41e09a\ndate: "), "{text}");
}

#[test]
fn bodies_are_normalized() {
    assert_eq!(
        normalize_body("\n\n  \nhello\r\nworld  \r\n\n"),
        "hello\nworld"
    );
    assert_eq!(normalize_body("a\rb"), "a\nb");
    assert_eq!(normalize_body("  indented\n  kept"), "  indented\n  kept");
    assert_eq!(normalize_body(" \n\t\n"), "");
    assert_eq!(normalize_body("\u{FEFF}x"), "x");
}

// --- reading ----------------------------------------------------------------------

#[test]
fn crlf_comments_blank_lines_and_unknown_keys_are_accepted() {
    let text = "---\r\n# written by hand\r\nid: d-4f2a\r\n\r\ndate: 2026-08-14T09:30:12Z\r\n\
        ticket: PROJ-412\r\nmodule: orders  # the main one\r\ntype: correction\r\n\
        reviewed-by: mila\r\n---\r\n\r\nBody.\r\n";
    let entry = parse(text).unwrap_or_else(|p| panic!("{p}"));
    assert_eq!(entry.id, "d-4f2a");
    assert_eq!(entry.module, "orders");
    assert_eq!(entry.kind, Type::Correction);
    assert_eq!(entry.body, "Body.");
}

#[test]
fn every_problem_is_reported_in_one_run() {
    let text = "---\nid: 7c41\ndate: 2026-10-06 18:37\nticket: shop-214\nmodule: or-ders\n\
        type: failed\ntype: note\nscenario: Reserve Stock\nnot a key\n---\n";
    let found = parse(text).unwrap_err().to_string();
    for needle in [
        "line 2: `id` must be",
        "line 3: `date` must be",
        "line 4: `ticket` must be",
        "line 5: `module` must be",
        "line 7: `type` appears twice",
        "line 8: `scenario` must be a slug",
        "line 9: `not a key` is not a `key: value` line",
        "the body is empty",
    ] {
        assert!(found.contains(needle), "no {needle:?} in:\n{found}");
    }
}

#[test]
fn a_missing_or_unclosed_front_matter_is_reported() {
    assert!(
        parse("no front matter\n")
            .unwrap_err()
            .to_string()
            .contains("the first line must be exactly `---`")
    );
    assert!(
        parse("---\nid: d-1234\n")
            .unwrap_err()
            .to_string()
            .contains("never closed")
    );
}

#[test]
fn the_filename_must_agree_with_the_date() {
    let entry = sample();
    // 18:37:12+02:00 is 16:37:12 UTC.
    assert!(check("2026-10-06-163712-orders-x.md", &entry.render()).is_ok());
    let found = problems("2026-10-06-183712-orders-x.md", &entry.render());
    assert!(
        found.contains("is not `date: 2026-10-06T18:37:12+02:00`"),
        "{found}"
    );
}

#[test]
fn a_malformed_filename_is_reported_with_the_shape() {
    let found = problems("orders-note.md", &sample().render());
    assert!(
        found.contains("is not `<YYYY-MM-DD>-<HHMMSS>-<module>-<slug>.md`"),
        "{found}"
    );
}

// --- the pieces ---------------------------------------------------------------------

#[test]
fn filenames_parse_and_render() {
    let name = FileName::parse("2026-10-06-163712-orders-reserve-stock-2.md").unwrap();
    assert_eq!(name.stamp, "2026-10-06-163712");
    assert_eq!(name.module, "orders");
    assert_eq!(name.slug, "reserve-stock-2");
    let fresh = FileName {
        stamp: "2026-10-06-163712".into(),
        module: "global".into(),
        slug: "proj-412".into(),
    };
    assert_eq!(fresh.render(1), "2026-10-06-163712-global-proj-412.md");
    assert_eq!(fresh.render(3), "2026-10-06-163712-global-proj-412-3.md");
    assert_eq!(
        FileName::parse(&fresh.render(3)).unwrap().slug,
        "proj-412-3"
    );

    // Capitals are kept: Laravel modules are `Billing`, not `billing`.
    let capitalized = FileName::parse("2026-10-06-163712-Orders-x.md").unwrap();
    assert_eq!(capitalized.module, "Orders");
    assert_eq!(capitalized.render(1), "2026-10-06-163712-Orders-x.md");

    for bad in [
        "2026-10-06-163712-orders.md",
        "2026-10-06-163712-orders-.md",
        "2026-10-06-163712-1orders-x.md",
        "2026-10-06-163712-orders-x.txt",
        "2026-10-06-246000-orders-x.md",
        "2026-02-30-120000-orders-x.md",
        "26-10-06-163712-orders-x.md",
        // Non-ASCII where digits belong: rejected, never a panic.
        // (the stamp is 17 bytes either way, so the slicing is reached)
        "2026-10-06-1é371-orders-x.md",
        "2026-10-06-123é1-orders-x.md",
        "2026-10-06-163712-ordérs-x.md",
    ] {
        assert_eq!(FileName::parse(bad), None, "{bad}");
    }
}

#[test]
fn dates_convert_to_the_utc_stamp() {
    for (date, stamp) in [
        ("2026-10-06T18:37:12+02:00", "2026-10-06-163712"),
        ("2026-10-06T16:37:12Z", "2026-10-06-163712"),
        ("2026-10-06T12:37:12-04:00", "2026-10-06-163712"),
        ("2026-10-07T01:30:00+02:00", "2026-10-06-233000"),
        ("2026-12-31T22:00:00-03:00", "2027-01-01-010000"),
        ("2026-10-06T22:07:12+05:30", "2026-10-06-163712"),
    ] {
        assert_eq!(utc_stamp_of(date).as_deref(), Some(stamp), "{date}");
    }
    for bad in [
        "2026-10-06",
        "2026-10-06T18:37:12",
        "2026-10-06 18:37:12+02:00",
        "2026-10-06T18:37+02:00",
        "2026-10-06T24:00:00Z",
        "2026-02-30T12:00:00Z",
        "2026-10-06T18:37:12+0200",
        "2026-10-06T18:37:12.5Z",
        "2026-10-06T18:37:1é+02:00",
        "2026-10-06T18:37:12+0é:00",
        "2026-10-06T18:37:12é",
    ] {
        assert_eq!(utc_stamp_of(bad), None, "{bad}");
    }
}

#[test]
fn ids_have_the_contract_shape() {
    for good in ["d-4f2a", "d-7c41e09a", &format!("d-{}", "a".repeat(32))] {
        assert!(is_id(good), "{good}");
    }
    for bad in [
        "d-4f2",
        "d-7C41E09A",
        "7c41e09a",
        "d-",
        "d-xyz1",
        &format!("d-{}", "a".repeat(33)),
    ] {
        assert!(!is_id(bad), "{bad}");
    }
}

#[test]
fn types_parse_by_their_exact_names() {
    for kind in Type::ALL {
        assert_eq!(Type::parse(kind.as_str()), Some(kind));
    }
    assert_eq!(Type::parse("Failed"), None);
    assert_eq!(Type::parse("fixed"), None);
}

// --- the draft (§8.1) -------------------------------------------------------------------

/// Draft example `### <number> `: its text. Drafts live in a Plan directory,
/// so their `File:` line is not under `.invar/decisions/`.
fn draft_example(number: &str) -> String {
    let text = contract();
    let after = text
        .split_once(&format!("### {number} "))
        .unwrap_or_else(|| panic!("the contract has no example {number}"))
        .1;
    assert!(
        after
            .split_once("File: `")
            .unwrap()
            .1
            .starts_with(".invar/plans/"),
        "example {number} is not a draft"
    );
    let body = after.split_once("```markdown\n").unwrap().1;
    body.split_once("\n```\n").unwrap().0.to_owned() + "\n"
}

fn draft_problems(text: &str) -> String {
    match super::parse_draft(text) {
        Ok(draft) => panic!("expected problems, got {draft:?}"),
        Err(problems) => problems.to_string(),
    }
}

#[test]
fn the_contracts_valid_draft_parses() {
    let draft = super::parse_draft(&draft_example("12.4")).unwrap();
    assert_eq!(draft.ticket, "SHOP-214");
    assert_eq!(draft.module, "orders");
    assert_eq!(draft.kind, Type::Confirmed);
    assert_eq!(draft.scenario, None);
    assert!(draft.body.starts_with("Cancelling releases reserved stock"));
    assert!(!draft.body.ends_with('\n'));
}

#[test]
fn the_contracts_invalid_draft_reports_the_date_and_the_type() {
    // The wrong ticket is a problem of the draft's place, not of its text:
    // `decision::draft` reports that one.
    let found = draft_problems(&draft_example("12.5"));
    assert!(found.contains("a draft has no `date`"), "{found}");
    assert!(found.contains("a draft is not a `correction`"), "{found}");
}

#[test]
fn examples_12_6_and_12_7_are_valid_with_each_value_and_written_as_they_are() {
    for (number, value) in [
        ("12.6", Acceptance::Required),
        ("12.7", Acceptance::Skipped),
    ] {
        let (name, text) = example(number);
        let entry = check(&name, &text).unwrap_or_else(|p| panic!("{number}: {p}"));
        assert_eq!(entry.acceptance, Some(value), "{number}");
        assert_eq!(entry.render(), text, "{number}");
    }
}

#[test]
fn example_12_8_reports_the_unknown_acceptance_value() {
    let (name, text) = example("12.8");
    let found = problems(&name, &text);
    assert!(
        found.contains(
            "line 7: `acceptance` must be `required` or `skipped`, exactly; found `none`"
        ),
        "{found}"
    );
    assert_eq!(found.lines().count(), 1, "{found}");
}

#[test]
fn example_12_9_is_a_valid_draft_on_its_own() {
    // What refuses it is the Plan beside it, when it is recorded (§8.1).
    let draft = super::parse_draft(&draft_example("12.9")).unwrap();
    assert_eq!(draft.acceptance, Some(Acceptance::Skipped));
}

#[test]
fn an_entry_without_acceptance_is_unknown_and_written_without_the_key() {
    let (_, text) = example("12.1");
    let entry = parse(&text).unwrap();
    assert_eq!(entry.acceptance, None);
    assert!(!entry.render().contains("acceptance"));
    let draft = super::parse_draft(&draft_example("12.4")).unwrap();
    assert_eq!(draft.acceptance, None);
}

#[test]
fn an_id_in_a_draft_is_named_with_its_line() {
    let text = "---\nid: d-7c41e09a\nticket: SHOP-214\nmodule: orders\ntype: note\n---\n\nx\n";
    let found = draft_problems(text);
    assert!(found.contains("line 2: a draft has no `id`"), "{found}");
}

#[test]
fn every_draft_type_is_accepted_and_nothing_else() {
    for kind in super::DRAFT_TYPES {
        let text = format!("---\nticket: SHOP-214\nmodule: global\ntype: {kind}\n---\n\nWhy.\n");
        assert_eq!(super::parse_draft(&text).unwrap().kind, kind);
    }
    for kind in ["fixed", "Confirmed", ""] {
        let text = format!("---\nticket: SHOP-214\nmodule: global\ntype: {kind}\n---\n\nWhy.\n");
        assert!(
            draft_problems(&text).contains("one of `confirmed`, `failed`, `note`"),
            "{kind:?}"
        );
    }
}

#[test]
fn a_draft_needs_its_three_keys_and_a_body() {
    let found = draft_problems("---\n---\n");
    for key in ["ticket", "module", "type"] {
        assert!(
            found.contains(&format!("the front matter has no `{key}`")),
            "{found}"
        );
    }
    assert!(found.contains("the body is empty"), "{found}");
}

#[test]
fn an_entry_is_not_a_valid_draft_and_a_draft_is_not_a_valid_entry() {
    // The two shapes must not be confused: a recorded entry copied into the
    // Plan directory carries an `id` and a `date`, and a draft copied into the
    // log has neither.
    let entry = example("12.1").1;
    assert!(draft_problems(&entry).contains("a draft has no `id`"));
    let draft = draft_example("12.4");
    match parse(&draft) {
        Ok(entry) => panic!("a draft parsed as an entry: {entry:?}"),
        Err(found) => assert!(found.to_string().contains("no `id`")),
    }
}

#[test]
fn a_draft_with_crlf_and_a_bom_parses_like_one_with_lf() {
    let lf = draft_example("12.4");
    let crlf = format!("\u{FEFF}{}", lf.replace('\n', "\r\n"));
    assert_eq!(
        super::parse_draft(&crlf).unwrap(),
        super::parse_draft(&lf).unwrap()
    );
}
