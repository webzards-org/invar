//! Tests for what `invar acceptance spec` prints from a Plan and a config, on their
//! own. `tests/acceptance_spec.rs` covers the command.

use std::fs;
use std::path::Path;

use super::{INTEGRATION, conventions, render, run_as_configured};
use crate::format::config::{AcceptanceConfig, AcceptanceStyle};
use crate::format::plan::{Mode, Plan};

fn plan() -> Plan {
    let contract =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../contracts/formats/plan-header.md");
    let contract = fs::read_to_string(contract).unwrap();
    let after = contract.split_once("### 9.1 ").unwrap().1;
    let body = after.split_once("```markdown\n").unwrap().1;
    let text = body.split_once("\n```\n").unwrap().0.to_owned() + "\n";
    Plan::parse(&text, Mode::Light).unwrap()
}

fn config() -> AcceptanceConfig {
    AcceptanceConfig {
        required: true,
        style: AcceptanceStyle::Integration,
        dir: "tests/Acceptance".to_owned(),
        run: vec![
            "vendor/bin/pest".to_owned(),
            "{report}".to_owned(),
            "{files}".to_owned(),
        ],
        fakes_allowed: vec!["http".to_owned(), "mail".to_owned()],
    }
}

#[test]
fn the_parts_come_in_their_fixed_order() {
    let out = render(&plan(), &config());
    let mut last = 0;
    for heading in [
        "=== invar acceptance spec: SHOP-214 ===",
        "=== Where and how ===",
        "=== Conventions ===",
        "=== Criteria ===",
        "=== Scope ===",
        "=== Implementation context ===",
        "=== Next step ===",
    ] {
        let at = out
            .find(heading)
            .unwrap_or_else(|| panic!("{heading}\n{out}"));
        assert!(at >= last, "{heading} is out of order\n{out}");
        last = at;
    }
}

#[test]
fn the_ticket_goes_into_the_lock_line() {
    let out = render(&plan(), &config());
    assert!(out.contains("`invar-lock: SHOP-214`"), "{out}");
    assert!(!out.contains("<TICKET>"), "{out}");
    assert!(out.contains("`invar acceptance lock SHOP-214`"), "{out}");
    // The template keeps it, so that the text is the same for every ticket.
    assert!(INTEGRATION.contains("`invar-lock: <TICKET>`"));
}

#[test]
fn the_conventions_are_the_fixed_text_of_the_style() {
    assert_eq!(conventions(AcceptanceStyle::Integration), INTEGRATION);
    for rule in [
        "One ticket per file",
        "within its first 10 lines",
        "Never write a digest",
        "exactly one id per test",
        "at least one test",
        "word for word",
        "real database",
        "Fake only what `fakes_allowed` lists",
        "control probe",
        "fail on an assertion before the code exists",
        "Never application code, the Plan, or a file with a locked",
    ] {
        assert!(INTEGRATION.contains(rule), "{rule}");
    }
}

#[test]
fn the_run_command_is_shown_as_configured() {
    let out = render(&plan(), &config());
    assert!(
        out.contains("run: [\"vendor/bin/pest\", \"{report}\", \"{files}\"]\n"),
        "{out}"
    );
    // One element stays one element, and quoting does not hide that.
    assert_eq!(
        run_as_configured(&["a b".to_owned(), "c\"d".to_owned()]),
        r#"["a b", "c\"d"]"#
    );
}

#[test]
fn fakes_allowed_or_none() {
    assert!(render(&plan(), &config()).contains("fakes_allowed: http, mail\n"));
    let none = AcceptanceConfig {
        fakes_allowed: Vec::new(),
        ..config()
    };
    assert!(render(&plan(), &none).contains("fakes_allowed: none: nothing may be faked\n"));
}

#[test]
fn every_criterion_of_the_plan_is_printed_with_its_id_and_text() {
    let plan = plan();
    let out = render(&plan, &config());
    let criteria = plan.criteria();
    assert!(!criteria.is_empty());
    for criterion in criteria {
        assert!(
            out.contains(&format!("\n{}: {}\n", criterion.id, criterion.text)),
            "{}\n{out}",
            criterion.id
        );
    }
}

#[test]
fn what_explains_how_the_plan_came_about_is_left_out() {
    let plan = plan();
    let out = render(&plan, &config());
    for heading in [
        "Ticket",
        "Open questions / answers",
        "Rejected alternatives",
        "Intent",
    ] {
        let section = plan.section(heading).unwrap().trim();
        assert!(!section.is_empty(), "{heading}");
        assert!(!out.contains(section), "{heading} is in the output\n{out}");
    }
}
