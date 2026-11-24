//! Tests for what `invar cycle check` decides on a report, and for `cycle required`.
//! `tests/cycle_check.rs` runs both on real repositories.

use super::{Closed, NotCovered, Report, Required, Skip, required};

fn order(covered_by: &[&str], uncovered: &[(&str, &str)]) -> Required {
    Required {
        path: "app/Models/Order.php".to_owned(),
        pattern: "app/Models/**".to_owned(),
        covered_by: covered_by.iter().map(|t| (*t).to_owned()).collect(),
        uncovered: uncovered
            .iter()
            .map(|(c, s)| ((*c).to_owned(), (*s).to_owned()))
            .collect(),
    }
}

fn report() -> Report {
    Report {
        base: "main".to_owned(),
        notes: Vec::new(),
        patterns: vec!["app/Models/**".to_owned()],
        off: None,
        list_change: None,
        fork: Some("0123456789abcdef".to_owned()),
        required: vec![order(&[], &[("0011223344556677", "change the model")])],
        closed: Vec::new(),
        open: Vec::new(),
        skips: Vec::new(),
        empty_skips: Vec::new(),
    }
}

#[test]
fn a_required_change_needs_every_commit_covered_or_a_skip() {
    let failing = report();
    assert!(!failing.passes());
    let out = failing.render();
    assert!(
        out.contains(
            "  app/Models/Order.php (app/Models/**): not covered: 001122334455 change the model\n"
        ),
        "{out}"
    );
    assert!(out.ends_with("cycle check: failed\n"));

    // A closed cycle that does not cover the change is not enough.
    let mut unrelated = report();
    unrelated.closed.push(Closed {
        ticket: "SHOP-214".to_owned(),
        spec: ".invar/plans/SHOP-214-x/spec.md".to_owned(),
        deleted_in: "fedcba9876543210".to_owned(),
    });
    assert!(!unrelated.passes());

    let mut covered = unrelated.clone();
    covered.required = vec![order(&["SHOP-214", "SHOP-215"], &[])];
    assert!(covered.passes());
    let out = covered.render();
    assert!(
        out.contains("(app/Models/**): covered by SHOP-214, SHOP-215\n"),
        "{out}"
    );
    assert!(
        out.contains("  SHOP-214: .invar/plans/SHOP-214-x/spec.md deleted in fedcba987654\n"),
        "{out}"
    );
    assert!(
        out.ends_with("cycle check: passed: every required change is covered by a closed cycle\n"),
        "{out}"
    );

    // Covered by one commit and not by another is not covered.
    let mut partly = report();
    partly.required = vec![order(&["SHOP-214"], &[("99aa", "after done")])];
    assert!(!partly.passes());

    let mut skipped = report();
    skipped.skips.push(Skip {
        commit: "abc".to_owned(),
        reason: "a typo".to_owned(),
    });
    assert!(skipped.passes());

    let mut nothing = report();
    nothing.required.clear();
    assert!(nothing.passes());
}

#[test]
fn an_off_report_passes_and_says_why_on_its_last_line() {
    let mut off = report();
    off.off = Some("main has no invar.yaml".to_owned());
    assert!(off.passes());
    assert_eq!(
        off.render(),
        "base: main\ncycle check: off: main has no invar.yaml\n"
    );
}

#[test]
fn the_refusal_lists_every_path_and_both_ways_out() {
    let mut both = report();
    both.required.push(Required {
        path: "app/Models/User.php".to_owned(),
        ..order(&["SHOP-214"], &[])
    });
    let not_covered = NotCovered::of(&both);
    assert_eq!(not_covered.required.len(), 1, "only the uncovered one");
    let message = not_covered.to_string();
    assert!(
        message.contains("touched outside a cycle by: 001122334455 change the model"),
        "{message}"
    );
    assert!(!message.contains("User.php"), "{message}");
    assert!(
        message.contains("  app/Models/Order.php (matches app/Models/**)"),
        "{message}"
    );
    assert!(message.contains("/invar:done"), "{message}");
    assert!(
        message.contains("--trailer \"Invar-Skip: <reason>\""),
        "{message}"
    );
}

#[test]
fn required_normalises_what_a_session_writes() {
    let patterns = vec!["app/Models/**".to_owned()];
    let paths = [
        "./app/Models/A.php".to_owned(),
        "app\\Models\\B.php".to_owned(),
        "x.php".to_owned(),
    ];
    let found = required(&patterns, &paths);
    assert_eq!(
        found[0],
        ("app/Models/A.php".to_owned(), Some("app/Models/**"))
    );
    assert_eq!(
        found[1],
        ("app/Models/B.php".to_owned(), Some("app/Models/**"))
    );
    assert_eq!(found[2], ("x.php".to_owned(), None));
}
