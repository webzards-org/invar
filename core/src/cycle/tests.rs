//! Tests for what `invar cycle check` decides on a report, and for `cycle required`.
//! `tests/cycle_check.rs` runs both on real repositories.

use super::{Closed, NotCovered, Report, Skip, required};

fn report() -> Report {
    Report {
        base: "main".to_owned(),
        patterns: vec!["app/Models/**".to_owned()],
        off: None,
        list_change: None,
        fork: Some("0123456789abcdef".to_owned()),
        required: vec![(
            "app/Models/Order.php".to_owned(),
            "app/Models/**".to_owned(),
        )],
        closed: Vec::new(),
        open: Vec::new(),
        skips: Vec::new(),
        empty_skips: Vec::new(),
    }
}

#[test]
fn a_required_change_needs_a_closed_cycle_or_a_skip() {
    let mut failing = report();
    assert!(!failing.passes());
    assert!(failing.render().ends_with("cycle check: failed\n"));

    failing.closed.push(Closed {
        ticket: "SHOP-214".to_owned(),
        spec: ".invar/plans/SHOP-214-x/spec.md".to_owned(),
        deleted_in: "fedcba9876543210".to_owned(),
    });
    assert!(failing.passes());
    let out = failing.render();
    assert!(
        out.contains("  SHOP-214: .invar/plans/SHOP-214-x/spec.md deleted in fedcba987654\n"),
        "{out}"
    );

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
    let message = NotCovered {
        required: report().required,
    }
    .to_string();
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
