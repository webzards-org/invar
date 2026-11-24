use super::{TEXT_WIDTH, check, cut, gate_text};
use crate::context::invariants::{Invariant, Invariants};
use crate::format::plan::InvariantRefs;

fn invariant(number: u64, text: &str, line: usize) -> Invariant {
    Invariant {
        number,
        text: text.to_owned(),
        line,
    }
}

fn two() -> Invariants {
    Invariants::Ids(vec![
        invariant(1, "every tenant-scoped query is filtered by account_id", 9),
        invariant(2, "an operator action notifies the customer", 10),
    ])
}

fn refs(named: &[(u64, usize)], cited: &[(&str, &[u64], usize)]) -> InvariantRefs {
    InvariantRefs {
        named: named.to_vec(),
        cited: cited
            .iter()
            .map(|(id, numbers, line)| ((*id).to_owned(), numbers.to_vec(), *line))
            .collect(),
    }
}

fn messages(found: &crate::diagnostics::Diagnostics) -> Vec<(Option<usize>, String)> {
    found
        .iter()
        .map(|problem| (problem.line, problem.message.clone()))
        .collect()
}

#[test]
fn a_consistent_plan_has_no_problem() {
    let found = check(&two(), &refs(&[(1, 40)], &[("AC2", &[1], 30)]));
    assert!(found.is_empty(), "{found}");
}

#[test]
fn an_id_the_file_does_not_define_is_refused_where_it_is_written() {
    let found = messages(&check(&two(), &refs(&[(7, 40)], &[("AC2", &[7], 30)])));
    // In line order: the criterion first.
    assert_eq!(found.len(), 2, "{found:?}");
    assert_eq!(found[1].0, Some(40));
    assert!(
        found[1]
            .1
            .contains("`INV-7` is not an invariant of BUSINESS_CONTEXT.md")
    );
    assert!(
        found[1].1.contains("defines INV-1, INV-2"),
        "{}",
        found[1].1
    );
    assert_eq!(found[0].0, Some(30));
}

#[test]
fn an_id_against_a_file_without_ids_is_refused_too() {
    for invariants in [Invariants::None, Invariants::WithoutIds { count: 3 }] {
        let found = messages(&check(&invariants, &refs(&[(1, 40)], &[("AC1", &[1], 30)])));
        assert_eq!(found.len(), 2, "{found:?}");
        assert!(
            found[0].1.contains("gives no invariant an id"),
            "{}",
            found[0].1
        );
    }
}

#[test]
fn a_named_invariant_no_criterion_cites_is_refused_at_its_line() {
    let found = messages(&check(
        &two(),
        &refs(&[(1, 40), (2, 41)], &[("AC2", &[1], 30)]),
    ));
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].0, Some(41));
    assert!(
        found[0]
            .1
            .contains("`INV-2` is named here and no criterion cites it")
    );
}

#[test]
fn a_criterion_citing_an_invariant_the_context_does_not_name_is_refused_at_its_line() {
    let found = messages(&check(&two(), &refs(&[(1, 40)], &[("AC2", &[1, 2], 30)])));
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].0, Some(30));
    assert!(
        found[0]
            .1
            .contains("`AC2` cites `INV-2`, which `## Implementation context` does not name")
    );
}

#[test]
fn the_files_own_problems_are_reported_with_the_files_lines() {
    let mut problems = crate::diagnostics::Diagnostics::new();
    problems.at(12, "`INV-1` appears again");
    let found = messages(&check(
        &Invariants::Invalid(problems),
        &InvariantRefs::default(),
    ));
    assert_eq!(
        found,
        vec![(
            None,
            "BUSINESS_CONTEXT.md:12: `INV-1` appears again".to_owned()
        )]
    );
}

#[test]
fn without_ids_in_the_plan_and_the_file_nothing_is_checked() {
    for invariants in [Invariants::None, Invariants::WithoutIds { count: 2 }, two()] {
        assert!(check(&invariants, &InvariantRefs::default()).is_empty());
    }
}

#[test]
fn the_gate_lists_every_invariant_named_or_not_and_what_approving_means() {
    let text = gate_text(&two(), &refs(&[(1, 40)], &[("AC2", &[1], 30)]));
    assert_eq!(
        text,
        "Invariants in BUSINESS_CONTEXT.md:\n\
         \x20 INV-1  named, covered by AC2   - every tenant-scoped query is filtered by account_id\n\
         \x20 INV-2  NOT NAMED in this Plan  - an operator action notifies the customer\n\
         \n\
         1 invariant is not named: approving says none of them applies to this ticket.\n"
    );
}

#[test]
fn every_invariant_named_has_no_summary_line() {
    let text = gate_text(
        &two(),
        &refs(
            &[(1, 40), (2, 41)],
            &[("AC2", &[1], 30), ("AC3", &[1, 2], 31)],
        ),
    );
    assert!(
        text.contains("INV-1  named, covered by AC2, AC3  - every"),
        "{text}"
    );
    assert!(!text.contains("not named"), "{text}");
}

#[test]
fn several_not_named_are_counted() {
    let text = gate_text(&two(), &InvariantRefs::default());
    assert!(
        text.ends_with(
            "\n2 invariants are not named: approving says none of them applies to this ticket.\n"
        ),
        "{text}"
    );
}

#[test]
fn without_ids_one_warning_and_without_invariants_nothing() {
    let warning = gate_text(
        &Invariants::WithoutIds { count: 3 },
        &InvariantRefs::default(),
    );
    assert!(warning.starts_with("warning: the 3 invariants in BUSINESS_CONTEXT.md have no id"));
    assert!(warning.contains("`- INV-1: …`"));
    assert_eq!(warning.lines().count(), 1);
    assert_eq!(gate_text(&Invariants::None, &InvariantRefs::default()), "");
}

#[test]
fn the_text_is_cut_to_a_fixed_width_and_the_same_files_print_the_same_bytes() {
    let long = "x".repeat(TEXT_WIDTH + 20);
    assert_eq!(cut(&long).chars().count(), TEXT_WIDTH);
    assert!(cut(&long).ends_with('…'));
    assert_eq!(cut("short"), "short");
    let invariants = Invariants::Ids(vec![invariant(1, &long, 9)]);
    assert_eq!(
        gate_text(&invariants, &InvariantRefs::default()),
        gate_text(&invariants, &InvariantRefs::default())
    );
}
