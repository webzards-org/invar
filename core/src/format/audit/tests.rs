//! Tests for `findings.md` (`contracts/formats/plan-audit.md` §3, §4), including the
//! contract's own example.

use std::fs;
use std::path::Path;

use super::{Evidence, Severity, parse, verdict_line};

/// The ```` ``` ```` block under `### 3.1 Example`.
fn contract_example() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../contracts/formats/plan-audit.md");
    let text = fs::read_to_string(path).unwrap();
    let after = text.split_once("### 3.1 Example").unwrap().1;
    let block = after.split_once("```\n").unwrap().1;
    block.split_once("\n```").unwrap().0.to_owned() + "\n"
}

/// Every message of a refused file, `line N: …`.
fn problems(text: &str) -> String {
    parse(text).unwrap_err().render(Path::new("findings.md"))
}

#[test]
fn the_contracts_example_is_valid_and_not_ready() {
    let findings = parse(&contract_example()).unwrap();
    assert_eq!(findings.edits.len(), 2);
    assert_eq!(findings.edits[0].section, "Implementation context");
    assert_eq!(
        findings.edits[0].evidence,
        Evidence::Line {
            path: "app/Models/Order.php".into(),
            from: 41,
            to: None
        }
    );
    assert_eq!(findings.findings.len(), 3);
    assert_eq!(findings.findings[2].evidence, Evidence::Ticket);
    assert_eq!(findings.findings[2].severity, Severity::Note);
    assert_eq!(findings.blocking(), 2);
    assert_eq!(
        verdict_line("SHOP-214", findings.blocking()),
        "plan audit SHOP-214: NOT READY — 2 blocking finding(s)"
    );
}

#[test]
fn an_empty_file_is_valid_and_ready() {
    let findings = parse("# nothing to say\n\n").unwrap();
    assert!(findings.edits.is_empty() && findings.findings.is_empty());
    assert_eq!(verdict_line("SHOP-214", 0), "plan audit SHOP-214: READY");
}

#[test]
fn the_last_field_keeps_a_bar_and_a_range_is_evidence() {
    let findings = parse(
        "edit: Scope | routes/web.php:3-9 | `a | b` was the route\r\n\
         finding: wording | note | Intent | ticket | one | two\n",
    )
    .unwrap();
    assert_eq!(findings.edits[0].reason, "`a | b` was the route");
    assert_eq!(
        findings.edits[0].evidence,
        Evidence::Line {
            path: "routes/web.php".into(),
            from: 3,
            to: Some(9)
        }
    );
    assert_eq!(findings.findings[0].text, "one | two");
}

#[test]
fn every_malformed_line_is_reported_with_its_line() {
    let text = "\
edit: Scope | app/x.php:1 | fine
nonsense here
edit: Acceptance criteria | app/x.php:1 | not allowed
edit: Scope | app/x.php | no line
edit: Scope | app/x.php:0 | line zero
edit: Scope | app/x.php:9-3 | backwards
edit: Scope | app/x.php:1 |
edit: Scope | app/x.php:1
finding: Wrong_Kind | note | Intent | ticket | text
finding: wording | urgent | Intent | ticket | text
finding: wording | blocking | Intent | ticket | invented gate
finding: wording | note | Summary | ticket | text
finding: wording | note | Intent | my file.php:3 | text
finding: wording | note | Intent | ticket |
";
    let problems = problems(text);
    for expected in [
        "findings.md:2: a line is `edit: …`",
        "findings.md:3: an edit is of `Implementation context` or `Scope` only",
        "findings.md:4: `app/x.php` is not evidence",
        "findings.md:5: `app/x.php:0` is not evidence",
        "findings.md:6: `app/x.php:9-3` is not evidence",
        "findings.md:7: an edit gives its reason",
        "findings.md:8: an edit is `edit: <section> | <evidence> | <reason>`",
        "findings.md:9: `Wrong_Kind` is not a kind",
        "findings.md:10: the severity is `blocking` or `note`",
        "findings.md:11: `wording` is not a blocking kind",
        "findings.md:12: `Summary` is not a section",
        "findings.md:13: `my file.php:3` is not evidence",
        "findings.md:14: a finding says what it found",
    ] {
        assert!(problems.contains(expected), "{expected}\n---\n{problems}");
    }
    assert!(!problems.contains("findings.md:1:"), "{problems}");
}

#[test]
fn each_blocking_kind_is_accepted_as_blocking_and_any_kind_as_a_note() {
    for (kind, _) in super::BLOCKING_KINDS {
        let text = format!("finding: {kind} | blocking | Acceptance criteria | ticket | x\n");
        assert_eq!(parse(&text).unwrap().blocking(), 1, "{kind}");
    }
    let note = parse("finding: criterion-cannot-pass | note | Intent | ticket | x\n").unwrap();
    assert_eq!(note.blocking(), 0);
}

#[test]
fn the_sentence_after_the_verdict_is_the_contracts() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../contracts/formats/plan-audit.md");
    let contract = fs::read_to_string(path).unwrap();
    assert!(
        contract.contains(&format!("`{}`", super::NOT_A_CHECK)),
        "{}",
        super::NOT_A_CHECK
    );
}
