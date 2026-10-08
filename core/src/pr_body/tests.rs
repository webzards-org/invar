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
- Given x, when y, then z.

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

- Given x, when y, then z.

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
