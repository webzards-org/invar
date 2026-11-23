//! Tests for the results check (`acceptance-lock.md` §10.5), on test cases built by
//! hand. `tests/acceptance_results.rs` runs the commands on real reports.

use crate::format::junit::{Criterion as Token, Outcome, TestCase};
use crate::format::plan::Criterion;

use super::{LockedFile, check};

const DIR: &str = "tests/Acceptance";
const A: &str = "tests/Acceptance/a_test.php";
const B: &str = "tests/Acceptance/b_test.php";

fn locked(paths: &[&str]) -> Vec<LockedFile> {
    paths
        .iter()
        .map(|path| LockedFile {
            path: (*path).to_owned(),
            ticket: "SHOP-214".to_owned(),
        })
        .collect()
}

fn case(file: &str, name: &str, outcome: Outcome) -> TestCase {
    let ids = crate::format::junit::criterion_ids(name);
    TestCase {
        name: name.to_owned(),
        file: file.to_owned(),
        outcome,
        criterion: match ids.as_slice() {
            [] => Token::None,
            [one] => Token::One(one.clone()),
            _ => Token::Several(ids.clone()),
        },
        failure_type: (outcome != Outcome::Passed).then(|| "Type".to_owned()),
        message: (outcome != Outcome::Passed).then(|| "\n  first line\nsecond line".to_owned()),
    }
}

fn criteria(ids: &[&str]) -> Vec<Criterion> {
    ids.iter()
        .map(|id| Criterion {
            id: (*id).to_owned(),
            text: format!("the text of {id}"),
        })
        .collect()
}

#[test]
fn every_locked_test_passed_is_green() {
    let results = check(
        DIR,
        &locked(&[A, B]),
        &[
            case(A, "it AC1 cancels", Outcome::Passed),
            case(B, "it AC2 refuses", Outcome::Passed),
        ],
    );
    assert!(results.green());
    assert!(results.problems().is_empty());
    assert!(results.green_for(&criteria(&["AC1", "AC2"])));
}

#[test]
fn failure_error_and_skipped_are_each_not_passed() {
    for outcome in [Outcome::Failure, Outcome::Error, Outcome::Skipped] {
        let results = check(DIR, &locked(&[A]), &[case(A, "it AC1 cancels", outcome)]);
        assert!(!results.green(), "{outcome:?}");
        let problems = results.problems();
        assert_eq!(problems.len(), 1, "{problems:?}");
        // The file, the test, the criterion, the outcome and the first line of the message.
        assert!(
            problems[0].starts_with(&format!("{A}: `it AC1 cancels` (AC1): ")),
            "{problems:?}"
        );
        assert!(problems[0].ends_with(": first line"), "{problems:?}");
    }
}

#[test]
fn a_locked_file_with_no_test_case_is_not_run() {
    let results = check(DIR, &locked(&[A, B]), &[case(A, "it AC1", Outcome::Passed)]);
    assert!(!results.green());
    assert_eq!(results.not_run, vec![B.to_owned()]);
    let problems = results.problems();
    assert!(problems[0].starts_with(&format!("{B}: not run by your test suite")));
    assert!(problems[0].contains("Add tests/Acceptance to a test suite"));
}

#[test]
fn a_test_passed_once_and_failed_once_is_not_passed_whichever_came_first() {
    for outcomes in [
        [Outcome::Failure, Outcome::Passed],
        [Outcome::Passed, Outcome::Failure],
    ] {
        let cases: Vec<TestCase> = outcomes
            .iter()
            .map(|outcome| case(A, "it AC1", *outcome))
            .collect();
        let results = check(DIR, &locked(&[A]), &cases);
        assert!(!results.green(), "{outcomes:?}");
        assert_eq!(results.tests.len(), 1, "one test, two occurrences");
        let problems = results.problems();
        assert!(
            problems[0].contains("passed in one run and failure in another"),
            "{problems:?}"
        );
        assert!(problems[0].contains("only on a retry is not accepted"));
    }
    // Twice green is green.
    let twice = [
        case(A, "it AC1", Outcome::Passed),
        case(A, "it AC1", Outcome::Passed),
    ];
    assert!(check(DIR, &locked(&[A]), &twice).green());
}

#[test]
fn test_cases_of_files_that_are_not_locked_are_ignored() {
    let results = check(
        DIR,
        &locked(&[A]),
        &[
            case(A, "it AC1", Outcome::Passed),
            case("tests/Feature/x_test.php", "it fails", Outcome::Failure),
            case(B, "it AC2 pending", Outcome::Error),
        ],
    );
    assert!(results.green(), "{:?}", results.problems());
    assert_eq!(results.tests.len(), 1);
}

#[test]
fn every_problem_is_listed_files_first() {
    let results = check(
        DIR,
        &locked(&[A, B]),
        &[
            case(A, "it AC1", Outcome::Failure),
            case(A, "it AC2", Outcome::Skipped),
            case(A, "no token", Outcome::Error),
        ],
    );
    let problems = results.problems();
    assert_eq!(problems.len(), 4, "{problems:?}");
    assert!(problems[0].starts_with(&format!("{B}: not run")));
    assert!(problems[1].contains("(AC1): failure"));
    assert!(problems[2].contains("(AC2): skipped"));
    assert!(problems[3].contains("`no token` (no criterion id): error"));
}

#[test]
fn a_message_with_no_text_shows_its_type() {
    let mut failed = case(A, "it AC1", Outcome::Failure);
    failed.message = None;
    let results = check(DIR, &locked(&[A]), &[failed]);
    assert!(results.problems()[0].ends_with("(AC1): failure: Type"));
}

#[test]
fn render_says_each_criterion_then_the_line_to_key_on() {
    let green = check(
        DIR,
        &locked(&[A]),
        &[
            case(A, "it AC1", Outcome::Passed),
            case(A, "it AC2", Outcome::Passed),
        ],
    );
    assert_eq!(
        green.render("SHOP-214", &criteria(&["AC1", "AC2"])),
        "AC1: passed\nAC2: passed\nacceptance: green SHOP-214\n"
    );

    let red = check(
        DIR,
        &locked(&[A, B]),
        &[
            case(A, "it AC1", Outcome::Passed),
            case(A, "it AC2", Outcome::Failure),
        ],
    );
    let out = red.render("SHOP-214", &criteria(&["AC1", "AC2", "AC3"]));
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines[0], "AC1: passed");
    assert_eq!(lines[1], "AC2: not passed");
    assert_eq!(
        lines[2],
        format!("  {A}: `it AC2` (AC2): failure: first line")
    );
    assert_eq!(lines[3], "AC3: not passed");
    assert_eq!(lines[4], "  no test of AC3 ran");
    assert_eq!(lines[5], "not passed, and of no criterion of the Plan:");
    assert!(lines[6].starts_with(&format!("  {B}: not run by your test suite")));
    assert_eq!(lines.last(), Some(&"acceptance: red SHOP-214"));
}

#[test]
fn a_criterion_no_test_ran_is_red_even_when_every_test_passed() {
    let results = check(DIR, &locked(&[A]), &[case(A, "it AC1", Outcome::Passed)]);
    assert!(results.green());
    assert!(!results.green_for(&criteria(&["AC1", "AC2"])));
    let out = results.render("SHOP-214", &criteria(&["AC1", "AC2"]));
    assert!(out.ends_with("AC2: not passed\n  no test of AC2 ran\nacceptance: red SHOP-214\n"));
}
