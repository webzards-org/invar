use super::{Invariant, Invariants, parse};
use crate::context::MARKER;
use crate::init::business_context;

fn file(invariants: &str) -> String {
    format!(
        "{MARKER}\n# BUSINESS_CONTEXT.md\n\n## Terms\n\n- Account: who pays.\n\n\
         ## Invariants (always true, no exceptions)\n\n{invariants}\n\
         ## Regional / parameterized facts\n\n- VAT: 19% in DE.\n"
    )
}

/// The line the first invariant is on in [`file`].
const FIRST: usize = 10;

fn problems(text: &str) -> Vec<(Option<usize>, String)> {
    match parse(text) {
        Invariants::Invalid(found) => found
            .iter()
            .map(|problem| (problem.line, problem.message.clone()))
            .collect(),
        other => panic!("expected problems, got {other:?}"),
    }
}

#[test]
fn ids_are_read_in_file_order_with_gaps_and_any_order() {
    let text = file(
        "- INV-3: every query is scoped to the account.\n- INV-1: a paid order is never deleted.\n",
    );
    assert_eq!(
        parse(&text),
        Invariants::Ids(vec![
            Invariant {
                number: 3,
                text: "every query is scoped to the account.".to_owned(),
                line: FIRST,
            },
            Invariant {
                number: 1,
                text: "a paid order is never deleted.".to_owned(),
                line: FIRST + 1,
            },
        ])
    );
}

#[test]
fn a_wrapped_item_and_its_sub_items_belong_to_it() {
    let text = file(
        "- INV-1: every query on account data is filtered\n  by `account_id`.\n  - including exports\n\
         - INV-2: an operator action notifies the customer.\n",
    );
    let Invariants::Ids(list) = parse(&text) else {
        panic!()
    };
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].text, "every query on account data is filtered");
    assert_eq!(list[1].line, FIRST + 3);
}

#[test]
fn only_the_invariants_section_is_read() {
    // Terms and regional facts get no ids in #158, and their items are not invariants.
    let text = file("- INV-1: scoped.\n");
    assert_eq!(parse(&text).ids().unwrap().len(), 1);
}

#[test]
fn a_heading_named_invariants_alone_counts_too() {
    let text = format!("{MARKER}\n## Invariants\n- INV-1: scoped.\n");
    assert_eq!(parse(&text).ids().unwrap()[0].line, 3);
}

#[test]
fn a_file_without_ids_is_without_ids() {
    let text = file("- every query is scoped.\n- a paid order is never deleted.\n");
    assert_eq!(parse(&text), Invariants::WithoutIds { count: 2 });
}

#[test]
fn the_scaffold_and_its_comment_are_nothing() {
    // `invar init` writes the form in a comment: the file stays empty (§5).
    assert!(business_context().contains("<!-- - INV-1: ... -->"));
    assert_eq!(parse(&business_context()), Invariants::None);
    assert_eq!(
        crate::context::classify(&business_context()),
        crate::context::State::Empty
    );
}

#[test]
fn no_marker_or_no_file_text_is_nothing() {
    assert_eq!(parse("## Invariants\n- INV-1: scoped.\n"), Invariants::None);
    assert_eq!(parse(""), Invariants::None);
    assert_eq!(parse(&file("")), Invariants::None);
}

#[test]
fn a_duplicate_is_named_with_its_line_and_the_first() {
    let found = problems(&file(
        "- INV-1: scoped.\n- INV-2: notified.\n- INV-1: again.\n",
    ));
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].0, Some(FIRST + 2));
    assert!(
        found[0]
            .1
            .contains("`INV-1` appears again (first on line 10)"),
        "{}",
        found[0].1
    );
    assert!(found[0].1.contains("`INV-3`"), "{}", found[0].1);
}

#[test]
fn malformed_ids_are_named() {
    for token in [
        "INV-0:",
        "INV-x:",
        "inv-1:",
        "INV-01:",
        "INV1:",
        "INV-1",
        "**INV-1:**",
    ] {
        let found = problems(&file(&format!("- {token} scoped.\n")));
        assert_eq!(found.len(), 1, "{token}");
        assert_eq!(found[0].0, Some(FIRST), "{token}");
        assert!(
            found[0]
                .1
                .contains(&format!("`{token}` is not an invariant id")),
            "{token}: {}",
            found[0].1
        );
    }
}

#[test]
fn some_with_ids_and_some_without_is_a_problem_at_each_without() {
    let found = problems(&file(
        "- INV-1: scoped.\n- a paid order is never deleted.\n- INV-4: notified.\n",
    ));
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].0, Some(FIRST + 1));
    assert!(
        found[0].1.contains("this invariant has no id"),
        "{}",
        found[0].1
    );
    assert!(found[0].1.contains("`- INV-5: …`"), "{}", found[0].1);
}

#[test]
fn a_word_that_starts_like_an_id_is_text() {
    let text = file("- Invoices are never edited once sent.\n");
    assert_eq!(parse(&text), Invariants::WithoutIds { count: 1 });
}

#[test]
fn crlf_and_a_bom_keep_the_line_numbers() {
    let text = format!(
        "\u{FEFF}{}",
        file("- INV-2: scoped.\n").replace('\n', "\r\n")
    );
    assert_eq!(parse(&text).ids().unwrap()[0].line, FIRST);
}

#[test]
fn an_example_in_a_fence_or_a_comment_is_not_an_invariant() {
    let text = file("```\n- INV-9: example.\n```\n<!--\n- INV-8: old.\n-->\n- INV-1: scoped.\n");
    let list = parse(&text);
    assert_eq!(list.ids().unwrap().len(), 1, "{list:?}");
}

// --- the contract's examples ------------------------------------------------------

/// The first ```markdown block after the contract heading `heading`.
fn block_after(heading: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../contracts/formats/business-context.md");
    let text = std::fs::read_to_string(&path).unwrap();
    let after = text.split_once(heading).unwrap().1;
    let body = after.split_once("```markdown\n").unwrap().1;
    body.split_once("\n```\n").unwrap().0.to_owned() + "\n"
}

fn contract_example(number: &str) -> String {
    block_after(&format!("### {number} "))
}

#[test]
fn contract_7_1_has_three_ids_with_a_gap_in_file_order() {
    let numbers: Vec<u64> = parse(&contract_example("7.1"))
        .ids()
        .unwrap()
        .iter()
        .map(|invariant| invariant.number)
        .collect();
    assert_eq!(numbers, vec![3, 1, 4]);
}

#[test]
fn contract_7_2_has_no_ids() {
    assert_eq!(
        parse(&contract_example("7.2")),
        Invariants::WithoutIds { count: 2 }
    );
}

#[test]
fn contract_7_3_names_its_three_problems_on_the_lines_it_says() {
    let lines: Vec<Option<usize>> = problems(&contract_example("7.3"))
        .into_iter()
        .map(|(line, _)| line)
        .collect();
    assert_eq!(lines, vec![Some(5), Some(6), Some(7)]);
}

#[test]
fn contract_4_is_what_init_writes() {
    assert_eq!(block_after("## 4. "), business_context());
}
