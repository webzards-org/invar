//! Tests for assembling the review input (M1-20). The command as a whole —
//! the refusals, the real git diff, the exit codes — is tested in
//! `tests/review_input.rs`.

use crate::context::{Binding, File, State};
use crate::format::plan::{Mode, Plan};

use super::{Diff, FileStat, choose, parse_numstat, render};

const PLAN: &str = "\
---
Ticket: SHOP-214
Module: orders
Status: approved
Approved-by: rade
Approved-at: 2026-10-05
Approved-checksum: sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
---

## Ticket
TICKET-TEXT-NOT-SHOWN

## Intent
Customers cancel unpaid orders.

## Scope
Only `app/Actions/Orders`.

## Open questions / answers
- Q: QUESTION-NOT-SHOWN A: yes

## Rejected alternatives
- REJECTED-NOT-SHOWN

## Acceptance criteria
- AC1: Given an unpaid order, when I cancel it, then it is cancelled.
- AC2: Given another team's order, when I cancel it, then the response is 404.

## Implementation context
- BUSINESS_CONTEXT invariant: every query is scoped to the team.
";

fn plan(text: &str) -> Plan {
    Plan::parse(text, Mode::Light).unwrap()
}

fn no_binding() -> Binding {
    Binding { files: Vec::new() }
}

fn business(text: &str) -> Binding {
    Binding {
        files: vec![File {
            name: "BUSINESS_CONTEXT.md",
            label: "business rules the application must obey; binding",
            state: State::Loaded(text.to_owned()),
        }],
    }
}

fn in_order(haystack: &str, needles: &[&str]) -> bool {
    let positions: Vec<usize> = needles
        .iter()
        .map(|needle| {
            haystack
                .find(needle)
                .unwrap_or_else(|| panic!("no {needle:?} in:\n{haystack}"))
        })
        .collect();
    positions.windows(2).all(|pair| pair[0] < pair[1])
}

const PATCH: &str = "diff --git a/app/A.php b/app/A.php\n--- a/app/A.php\n+++ b/app/A.php\n@@ -1 +1 @@\n-old\n+new\n";

// --- what is shown, and in what order ------------------------------------------------

#[test]
fn the_four_sections_then_the_binding_context_then_the_diff() {
    let out = render(
        &plan(PLAN),
        &business("Every query is scoped to the team."),
        "main",
        &Diff::Full(PATCH.to_owned()),
    );

    assert!(
        out.starts_with("=== invar review input: SHOP-214 ===\n"),
        "{out}"
    );
    assert!(out.contains("approved by rade on 2026-10-05"), "{out}");
    assert!(
        in_order(
            &out,
            &[
                "=== Plan: Intent ===",
                "Customers cancel unpaid orders.",
                "=== Plan: Scope ===",
                "=== Plan: Acceptance criteria ===",
                "then the response is 404.",
                "=== Plan: Implementation context ===",
                "=== invar: binding context ===",
                "Every query is scoped to the team.",
                "=== diff against main",
                "+new",
                "=== end of diff ===",
            ]
        ),
        "{out}"
    );
}

#[test]
fn what_explains_the_plan_rather_than_states_it_is_left_out() {
    let out = render(&plan(PLAN), &no_binding(), "main", &Diff::Empty);

    for absent in [
        "TICKET-TEXT-NOT-SHOWN",
        "QUESTION-NOT-SHOWN",
        "REJECTED-NOT-SHOWN",
        "Open questions",
        "Rejected alternatives",
        // Nothing of the decision log, whatever is in the project.
        "decision log",
    ] {
        assert!(!out.contains(absent), "{absent:?} in:\n{out}");
    }
}

#[test]
fn the_same_arguments_give_the_same_bytes() {
    let diff = Diff::Full(PATCH.to_owned());
    let binding = business("Rule.");
    let a = render(&plan(PLAN), &binding, "main", &diff);
    let b = render(&plan(PLAN), &binding, "main", &diff);
    assert_eq!(a, b);
}

#[test]
fn a_plan_checked_out_with_other_line_endings_gives_the_same_bytes() {
    let diff = Diff::Full(PATCH.to_owned());
    let lf = render(&plan(PLAN), &no_binding(), "main", &diff);
    let crlf = render(
        &plan(&PLAN.replace('\n', "\r\n")),
        &no_binding(),
        "main",
        &diff,
    );
    assert_eq!(lf, crlf);
}

#[test]
fn without_binding_context_there_is_no_binding_block() {
    let out = render(&plan(PLAN), &no_binding(), "main", &Diff::Empty);
    assert!(!out.contains("binding context ==="), "{out}");
}

#[test]
fn an_empty_diff_says_there_is_nothing_to_review() {
    let out = render(&plan(PLAN), &no_binding(), "develop", &Diff::Empty);
    assert!(out.contains("=== diff against develop ==="), "{out}");
    assert!(out.contains("Nothing is committed on this branch"), "{out}");
    assert!(out.ends_with("=== end of diff ===\n"), "{out}");
}

#[test]
fn a_patch_without_a_final_newline_still_closes_on_its_own_line() {
    let out = render(
        &plan(PLAN),
        &no_binding(),
        "main",
        &Diff::Full("+new".to_owned()),
    );
    assert!(out.ends_with("+new\n\n=== end of diff ===\n"), "{out}");
}

// --- the budget -----------------------------------------------------------------------

#[test]
fn a_patch_within_the_budget_is_shown_whole_and_one_character_over_is_not() {
    let size = PATCH.chars().count();
    assert_eq!(
        choose(PATCH.to_owned(), "", size),
        Diff::Full(PATCH.to_owned())
    );
    match choose(PATCH.to_owned(), "1\t1\tapp/A.php\n", size - 1) {
        Diff::Files {
            chars,
            budget,
            files,
        } => {
            assert_eq!((chars, budget), (size, size - 1));
            assert_eq!(files.len(), 1);
        }
        other => panic!("expected Files, got {other:?}"),
    }
}

#[test]
fn the_budget_counts_characters_not_bytes() {
    let patch = "+ć\n".repeat(10);
    assert_eq!(patch.chars().count(), 30);
    assert!(matches!(choose(patch, "", 30), Diff::Full(_)));
}

#[test]
fn a_blank_patch_is_empty() {
    assert_eq!(choose(String::new(), "", 10), Diff::Empty);
    assert_eq!(choose("\n".to_owned(), "", 10), Diff::Empty);
}

#[test]
fn over_the_budget_the_files_are_listed_and_the_patch_is_not() {
    let diff = Diff::Files {
        chars: 50_123,
        budget: 40_000,
        files: vec![
            FileStat {
                added: Some(12),
                deleted: Some(3),
                path: "app/Actions/Orders/CancelOrder.php".to_owned(),
            },
            FileStat {
                added: None,
                deleted: None,
                path: "public/logo.png".to_owned(),
            },
        ],
    };
    let out = render(&plan(PLAN), &no_binding(), "main", &diff);

    assert!(
        out.contains("the diff is 50123 characters, over the budget of 40000"),
        "{out}"
    );
    assert!(out.contains("read the files you need"), "{out}");
    assert!(
        out.contains("\n+12 -3  app/Actions/Orders/CancelOrder.php\n"),
        "{out}"
    );
    assert!(out.contains("\nbinary  public/logo.png\n"), "{out}");
    assert!(out.ends_with("=== end of changed files ===\n"), "{out}");
}

#[test]
fn numstat_lines_are_read_with_binaries_and_renames() {
    let files = parse_numstat(
        "12\t3\tapp/A.php\n-\t-\tpublic/logo.png\n0\t0\tapp/{Old => New}/B.php\nnot a line\n",
    );
    assert_eq!(
        files,
        [
            FileStat {
                added: Some(12),
                deleted: Some(3),
                path: "app/A.php".to_owned(),
            },
            FileStat {
                added: None,
                deleted: None,
                path: "public/logo.png".to_owned(),
            },
            FileStat {
                added: Some(0),
                deleted: Some(0),
                path: "app/{Old => New}/B.php".to_owned(),
            },
        ]
    );
}

// --- `## Acceptance tests to change` (plan-header.md §6.5) -----------------------

fn with_tests_to_change(content: &str) -> String {
    PLAN.replacen(
        "## Implementation context\n",
        &format!("## Acceptance tests to change\n{content}\n## Implementation context\n"),
        1,
    )
}

#[test]
fn the_tests_to_change_come_right_after_the_criteria() {
    let text = with_tests_to_change("- tests/A.php — delete — no longer applies\n");
    let out = render(&plan(&text), &no_binding(), "main", &Diff::Empty);
    assert!(
        out.contains(
            "is 404.\n\n\
             === Plan: Acceptance tests to change ===\n\n\
             - tests/A.php — delete — no longer applies\n\n\
             === Plan: Implementation context ===\n"
        ),
        "{out}"
    );
}

#[test]
fn nothing_is_written_when_the_section_is_absent_or_says_none() {
    let without = render(&plan(PLAN), &no_binding(), "main", &Diff::Empty);
    assert!(!without.contains("Acceptance tests to change"), "{without}");

    let none = with_tests_to_change("_none_\n");
    assert_eq!(
        render(&plan(&none), &no_binding(), "main", &Diff::Empty),
        without
    );
}
