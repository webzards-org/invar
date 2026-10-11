//! Tests for decision entries (M1-11), against
//! `contracts/formats/decision-entry.md` v1.0.0. The examples are read out of
//! the contract itself, so the two cannot drift apart.

use std::fs;
use std::path::Path;

use super::{Entry, FileName, Type, check, is_id, normalize_body, parse, utc_stamp_of};

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
