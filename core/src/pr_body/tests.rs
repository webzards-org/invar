//! Tests for rendering a Plan as a PR description (M1-16).

use crate::format::plan::{Mode, Plan};

use super::render;

const LIGHT: &str = "\
---
Ticket: SHOP-214
Module: orders
Status: approved
Approved-by: rade
Approved-at: 2026-10-05
Approved-checksum: sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
---

## Ticket
Pasted ticket text.

## Intent
Do the thing.

## Scope
Only orders.

## Open questions / answers
None were needed.

## Rejected alternatives
None considered.

## Acceptance criteria
- AC1: Given x, when y, then z.

## Implementation context
- AI_CONTEXT: do it this way
";

const SCENARIOS: &str = "\
---
Ticket: PROJ-412
Module: orders
Status: approved
Approved-by: rade
Approved-at: 2026-10-05
Approved-checksum: sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
---

## Ticket
PROJ-412 — reserve stock.

## Intent
Reserve at placement.

## Scope
`orders` only.

## Open questions / answers
None were needed.

## Rejected alternatives
None considered.

## Scenarios to write
- specs/orders/reserve-stock.feature

## Scenarios to delete
_none_

## Implementation context
- AI_CONTEXT: do it this way
";

fn plan(text: &str, mode: Mode) -> Plan {
    Plan::parse(text, mode).unwrap()
}

#[test]
fn renders_the_fixed_sections_in_order_without_the_ticket_by_default() {
    let plan = plan(LIGHT, Mode::Light);
    let rendered = render(&plan, "SHOP-214", false);
    assert!(
        rendered.starts_with("# SHOP-214\n\nApproved by rade on 2026-10-05.\n"),
        "{rendered}"
    );
    assert!(!rendered.contains("## Ticket"), "{rendered}");

    let order = [
        "## Intent",
        "## Scope",
        "## Open questions / answers",
        "## Rejected alternatives",
        "## Acceptance criteria",
        "## Implementation context",
    ];
    let positions: Vec<usize> = order.iter().map(|h| rendered.find(h).unwrap()).collect();
    assert!(positions.windows(2).all(|w| w[0] < w[1]), "{rendered}");
    assert!(rendered.contains("Given x, when y, then z."), "{rendered}");
}

#[test]
fn renders_exactly_this_for_the_light_mode_fixture() {
    let plan = plan(LIGHT, Mode::Light);
    let rendered = render(&plan, "SHOP-214", false);
    assert_eq!(
        rendered,
        "\
# SHOP-214

Approved by rade on 2026-10-05.

## Intent

Do the thing.

## Scope

Only orders.

## Open questions / answers

None were needed.

## Rejected alternatives

None considered.

## Acceptance criteria

- AC1: Given x, when y, then z.

## Implementation context

- AI_CONTEXT: do it this way
"
    );
}

#[test]
fn with_ticket_puts_the_pasted_text_first() {
    let plan = plan(LIGHT, Mode::Light);
    let rendered = render(&plan, "SHOP-214", true);
    let ticket_pos = rendered.find("## Ticket").unwrap();
    let intent_pos = rendered.find("## Intent").unwrap();
    assert!(ticket_pos < intent_pos, "{rendered}");
    assert!(rendered.contains("Pasted ticket text."), "{rendered}");
}

#[test]
fn rendering_the_same_plan_twice_is_byte_identical() {
    let plan = plan(LIGHT, Mode::Light);
    assert_eq!(
        render(&plan, "SHOP-214", false),
        render(&plan, "SHOP-214", false)
    );
}

#[test]
fn a_section_the_plan_does_not_have_renders_as_none() {
    // `scenarios: true` has no `## Acceptance criteria` (OQ-2).
    let plan = plan(SCENARIOS, Mode::Scenarios);
    let rendered = render(&plan, "PROJ-412", false);
    assert!(
        rendered.contains("## Acceptance criteria\n\nnone\n"),
        "{rendered}"
    );
}

#[test]
fn the_indentation_of_a_sections_first_line_is_kept() {
    // `trim` would turn this code block into a paragraph.
    let text = LIGHT.replace(
        "## Scope\nOnly orders.\n",
        "## Scope\n\n\n    $order->cancel();\n    $order->save();\n",
    );
    let rendered = render(&plan(&text, Mode::Light), "SHOP-214", false);

    assert!(
        rendered.contains(
            "## Scope\n\n    $order->cancel();\n    $order->save();\n\n## Open questions"
        ),
        "{rendered}"
    );
}

#[test]
fn blank_lines_around_a_section_are_dropped_and_text_inside_it_is_not() {
    let text = LIGHT.replace(
        "## Intent\nDo the thing.\n",
        "## Intent\n\n\nDo the thing.\n\nThen the next thing.\n\n\n",
    );
    let rendered = render(&plan(&text, Mode::Light), "SHOP-214", false);

    assert!(
        rendered.contains("## Intent\n\nDo the thing.\n\nThen the next thing.\n\n## Scope"),
        "{rendered}"
    );
}

#[test]
fn the_same_plan_with_other_line_endings_renders_the_same_bytes() {
    // A Plan checked out with CRLF is the same Plan; its description must be
    // too, or the description would depend on whose machine rendered it.
    //
    // Once as the fixture is, and once with a blank line right after a
    // heading: a lone `\r` has no `\n` to look for, which is where this goes
    // wrong if line endings are not normalized first.
    let spaced = LIGHT.replace("Do the thing.", "\nDo the thing.\n");
    for text in [LIGHT, spaced.as_str()] {
        let lf = render(&plan(text, Mode::Light), "SHOP-214", true);
        let crlf = render(
            &plan(&text.replace('\n', "\r\n"), Mode::Light),
            "SHOP-214",
            true,
        );
        let cr = render(
            &plan(&text.replace('\n', "\r"), Mode::Light),
            "SHOP-214",
            true,
        );

        assert_eq!(lf, crlf);
        assert_eq!(lf, cr);
        assert!(!lf.contains('\r'));
    }
}

#[test]
fn an_unapproved_header_renders_a_placeholder_approval_line() {
    let draft = LIGHT
        .replace("Status: approved", "Status: draft")
        .replace("Approved-by: rade", "Approved-by:")
        .replace("Approved-at: 2026-10-05", "Approved-at:")
        .replace(
            "Approved-checksum: sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "Approved-checksum:",
        );
    let plan = plan(&draft, Mode::Light);
    let rendered = render(&plan, "SHOP-214", false);
    assert!(
        rendered.starts_with("# SHOP-214\n\nApproved by (not approved) on (not approved).\n"),
        "{rendered}"
    );
}

// --- `## Acceptance tests to change` (plan-header.md §6.5) -----------------------

const CONTEXT_HEADING: &str = "## Implementation context\n";

fn with_tests_to_change(content: &str) -> String {
    LIGHT.replacen(
        CONTEXT_HEADING,
        &format!("## Acceptance tests to change\n{content}\n{CONTEXT_HEADING}"),
        1,
    )
}

#[test]
fn the_tests_to_change_come_right_after_the_criteria() {
    let text = with_tests_to_change("- tests/A.php — delete — no longer applies\n");
    let rendered = render(&plan(&text, Mode::Light), "SHOP-214", false);
    assert!(
        rendered.contains(
            "- AC1: Given x, when y, then z.\n\n\
             ## Acceptance tests to change\n\n\
             - tests/A.php — delete — no longer applies\n\n\
             ## Implementation context\n"
        ),
        "{rendered}"
    );
}

#[test]
fn nothing_is_written_when_the_section_is_absent_or_says_none() {
    let without = render(&plan(LIGHT, Mode::Light), "SHOP-214", false);
    assert!(!without.contains("Acceptance tests to change"), "{without}");

    let none = with_tests_to_change("_none_\n");
    assert_eq!(
        render(&plan(&none, Mode::Light), "SHOP-214", false),
        without
    );
}
