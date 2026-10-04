//! Tests for Plan lookup (M1-7): zero, one, two and malformed matches, and
//! the header/directory ticket check.

use std::fs;

use tempfile::TempDir;

use super::{LookupError, PlanDir, SPEC_FILE, find, open, parse_dir_name};
use crate::format::plan::{Mode, PlanError};
use crate::fs::ProjectRoot;

/// A valid light-mode Plan for `ticket`: every required section, in order,
/// none empty (plan-header.md §6).
fn plan_text(ticket: &str) -> String {
    format!(
        "---\nTicket: {ticket}\nModule: orders\nStatus: draft\nApproved-by:\n\
         Approved-at:\nApproved-checksum:\n---\n\n\
         ## Ticket\n{ticket} — Cancel an unpaid order.\n\n\
         ## Intent\nCustomers cancel their own unpaid orders.\n\n\
         ## Scope\n`orders` only.\n\n\
         ## Open questions / answers\nNone were needed.\n\n\
         ## Rejected alternatives\nNone considered.\n\n\
         ## Acceptance criteria\n- Given an unpaid order, when I cancel it, then it is cancelled.\n\n\
         ## Implementation context\n- AI_CONTEXT: controllers stay thin\n"
    )
}

/// A project with `invar.yaml` and the given Plan directories, each holding a
/// valid `spec.md` for the ticket its name starts with.
fn project(plan_dirs: &[&str]) -> (TempDir, ProjectRoot) {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("invar.yaml"), "").unwrap();
    let root = ProjectRoot::for_test(dir.path());
    fs::create_dir_all(root.plans_dir()).unwrap();
    for name in plan_dirs {
        let path = root.plans_dir().join(name);
        fs::create_dir_all(&path).unwrap();
        let ticket = parse_dir_name(name).map_or("PROJ-1", |(ticket, _)| ticket);
        fs::write(path.join(SPEC_FILE), plan_text(ticket)).unwrap();
    }
    (dir, root)
}

fn found(root: &ProjectRoot, query: &str) -> PlanDir {
    find(root, query).unwrap_or_else(|err| panic!("expected one match for {query:?}: {err}"))
}

fn error(root: &ProjectRoot, query: &str) -> LookupError {
    match find(root, query) {
        Ok(dir) => panic!("expected an error for {query:?}, found {}", dir.name),
        Err(err) => err,
    }
}

// --- one match --------------------------------------------------------------

#[test]
fn exactly_one_match_returns_it() {
    let (_dir, root) = project(&["PROJ-412-reserve-stock", "PROJ-500-refunds"]);
    let plan = found(&root, "PROJ-412");
    assert_eq!(plan.name, "PROJ-412-reserve-stock");
    assert_eq!(plan.ticket, "PROJ-412");
    assert_eq!(plan.slug, "reserve-stock");
    assert_eq!(plan.path, root.plans_dir().join("PROJ-412-reserve-stock"));
    assert_eq!(
        plan.spec_path(),
        root.plans_dir().join("PROJ-412-reserve-stock/spec.md")
    );
}

#[test]
fn any_whole_segment_of_the_name_finds_it() {
    let (_dir, root) = project(&["PROJ-412-reserve-stock", "PROJ-500-refunds"]);
    for query in [
        "PROJ-412",
        "412",
        "reserve-stock",
        "stock",
        "PROJ-412-reserve-stock",
    ] {
        assert_eq!(
            found(&root, query).name,
            "PROJ-412-reserve-stock",
            "{query}"
        );
    }
}

#[test]
fn surrounding_whitespace_in_the_query_is_ignored() {
    let (_dir, root) = project(&["PROJ-412-reserve-stock"]);
    assert_eq!(found(&root, "  PROJ-412\n").ticket, "PROJ-412");
}

// --- the prefix trap (D-10) -------------------------------------------------

#[test]
fn a_ticket_never_matches_a_longer_ticket_that_starts_with_it() {
    // With plain substring matching, `PROJ-41` would find `PROJ-412-…` — one
    // confident, wrong match, which no ambiguity check could catch.
    let (_dir, root) = project(&["PROJ-412-reserve-stock"]);
    assert!(
        matches!(error(&root, "PROJ-41"), LookupError::NotFound { .. }),
        "PROJ-41 must not resolve to PROJ-412"
    );
    assert!(matches!(
        error(&root, "ROJ-412"),
        LookupError::NotFound { .. }
    ));
    assert!(matches!(
        error(&root, "reserve-st"),
        LookupError::NotFound { .. }
    ));
}

#[test]
fn a_ticket_finds_its_own_plan_next_to_a_longer_one() {
    let (_dir, root) = project(&["PROJ-41-cancel-order", "PROJ-412-reserve-stock"]);
    assert_eq!(found(&root, "PROJ-41").name, "PROJ-41-cancel-order");
    assert_eq!(found(&root, "PROJ-412").name, "PROJ-412-reserve-stock");
}

// --- zero matches -----------------------------------------------------------

#[test]
fn zero_matches_names_the_ticket_and_the_directory_searched() {
    let (_dir, root) = project(&["PROJ-412-reserve-stock", "PROJ-500-refunds"]);
    let err = error(&root, "PROJ-999");
    assert!(
        matches!(err, LookupError::NotFound { exists: true, .. }),
        "{err:?}"
    );
    let message = err.to_string();
    assert!(message.contains("no Plan for `PROJ-999`"), "{message}");
    assert!(
        message.contains(&root.plans_dir().display().to_string()),
        "{message}"
    );
    assert!(
        message.contains("PROJ-412-reserve-stock, PROJ-500-refunds"),
        "{message}"
    );
}

#[test]
fn zero_matches_in_an_empty_or_missing_plans_directory() {
    let (_dir, root) = project(&[]);
    let message = error(&root, "PROJ-1").to_string();
    assert!(message.contains("there are no Plans there"), "{message}");

    fs::remove_dir_all(root.plans_dir()).unwrap();
    let err = error(&root, "PROJ-1");
    assert!(
        matches!(err, LookupError::NotFound { exists: false, .. }),
        "{err:?}"
    );
    assert!(err.to_string().contains("does not exist"), "{err}");
}

#[test]
fn a_wrongly_capitalised_ticket_is_not_found_but_pointed_out() {
    let (_dir, root) = project(&["PROJ-412-reserve-stock"]);
    let err = error(&root, "proj-412");
    assert!(matches!(err, LookupError::NotFound { .. }), "{err:?}");
    assert!(
        err.to_string()
            .contains("did you mean `PROJ-412-reserve-stock`?"),
        "{err}"
    );
}

#[test]
fn files_and_hidden_entries_are_not_plans() {
    let (_dir, root) = project(&[]);
    fs::write(root.plans_dir().join("PROJ-412-notes.md"), "").unwrap();
    fs::write(root.plans_dir().join(".gitkeep"), "").unwrap();
    fs::create_dir(root.plans_dir().join(".PROJ-412-hidden")).unwrap();
    assert!(matches!(
        error(&root, "PROJ-412"),
        LookupError::NotFound { .. }
    ));
}

// --- several matches --------------------------------------------------------

#[test]
fn two_matches_are_a_different_error_listing_every_match() {
    let (_dir, root) = project(&["PROJ-412-reserve-stock", "PROJ-412-reserve-stock-v2"]);
    let err = error(&root, "PROJ-412");
    let LookupError::Ambiguous { matches, .. } = &err else {
        panic!("expected Ambiguous, got {err:?}");
    };
    assert_eq!(
        matches,
        &[
            root.plans_dir().join("PROJ-412-reserve-stock"),
            root.plans_dir().join("PROJ-412-reserve-stock-v2"),
        ]
    );
    let message = err.to_string();
    assert!(message.contains("matches 2 Plans"), "{message}");
    for path in matches {
        assert!(message.contains(&path.display().to_string()), "{message}");
    }
}

#[test]
fn a_shared_word_matches_several_tickets() {
    let (_dir, root) = project(&[
        "PROJ-412-reserve-stock",
        "PROJ-500-restock-alerts",
        "PROJ-501-stock-report",
    ]);
    // `stock` is a whole segment of two names, and only part of `restock`.
    let err = error(&root, "stock");
    let LookupError::Ambiguous { matches, .. } = &err else {
        panic!("expected Ambiguous, got {err:?}");
    };
    assert_eq!(matches.len(), 2, "{err}");
}

// --- malformed matches ------------------------------------------------------

#[test]
fn a_malformed_directory_name_is_its_own_error() {
    for name in [
        "proj-412-reserve-stock",  // lowercase ticket
        "PROJ-412",                // no slug
        "PROJ-412-Reserve-Stock",  // uppercase slug
        "PROJ-412-reserve--stock", // double dash
        "PROJ-412-reserve-stock-", // trailing dash
        "PROJ-412_reserve_stock",  // underscores
        "PROJ412-reserve-stock",   // no dash in the ticket
    ] {
        let (_dir, root) = project(&[name]);
        let query = if name.starts_with("proj") {
            "proj-412"
        } else if name.starts_with("PROJ412") {
            "PROJ412"
        } else if name.contains('_') {
            "PROJ"
        } else {
            "PROJ-412"
        };
        let err = error(&root, query);
        assert!(
            matches!(&err, LookupError::MalformedName { path, .. } if path.ends_with(name)),
            "{name}: {err:?}"
        );
        assert!(
            err.to_string().contains("is not `<TICKET>-<slug>`"),
            "{err}"
        );
    }
}

#[test]
fn a_malformed_directory_that_does_not_match_is_ignored() {
    let (_dir, root) = project(&["PROJ-412-reserve-stock", "Notes For Later"]);
    assert_eq!(found(&root, "PROJ-412").name, "PROJ-412-reserve-stock");
}

#[test]
fn a_malformed_and_a_valid_match_are_still_ambiguous() {
    // Never resolved by preferring the well-formed one: that is a guess too.
    let (_dir, root) = project(&["PROJ-412-reserve-stock", "PROJ-412-Old"]);
    assert!(matches!(
        error(&root, "PROJ-412"),
        LookupError::Ambiguous { .. }
    ));
}

#[test]
fn a_match_without_spec_md_is_its_own_error() {
    let (_dir, root) = project(&["PROJ-412-reserve-stock"]);
    fs::remove_file(root.plans_dir().join("PROJ-412-reserve-stock/spec.md")).unwrap();
    let err = error(&root, "PROJ-412");
    assert!(matches!(err, LookupError::MissingSpec { .. }), "{err:?}");
    assert!(err.to_string().contains("has no `spec.md`"), "{err}");
}

#[test]
fn a_query_that_names_nothing_is_refused() {
    let (_dir, root) = project(&["PROJ-412-reserve-stock"]);
    for query in [
        "",
        "   ",
        "-",
        "--",
        ".invar/plans/PROJ-412-reserve-stock",
        "..\\x",
    ] {
        assert!(
            matches!(error(&root, query), LookupError::InvalidQuery { .. }),
            "{query:?}"
        );
    }
}

// --- the directory name -----------------------------------------------------

#[test]
fn directory_names_follow_the_contract_shape() {
    assert_eq!(
        parse_dir_name("PROJ-412-reserve-stock"),
        Some(("PROJ-412", "reserve-stock"))
    );
    assert_eq!(
        parse_dir_name("SHOP-214-cancel"),
        Some(("SHOP-214", "cancel"))
    );
    // Digits in the prefix and a slug starting with a digit are both fine.
    assert_eq!(
        parse_dir_name("A1B-7-2fa-setup"),
        Some(("A1B-7", "2fa-setup"))
    );
    for bad in [
        "",
        "PROJ",
        "PROJ-",
        "PROJ-412",
        "PROJ-412-",
        "-412-x",
        "1PROJ-412-x",
        "PROJ-x-reserve",
        "PROJ-412x-reserve",
        "PROJ-412-Reserve",
        "PROJ-412-re_serve",
        "PROJ-412--x",
        "PROJ-412-x-",
    ] {
        assert_eq!(parse_dir_name(bad), None, "{bad:?}");
    }
}

// --- the header's ticket ----------------------------------------------------

#[test]
fn open_reads_the_plan_and_checks_its_ticket() {
    let (_dir, root) = project(&["PROJ-412-reserve-stock"]);
    let (dir, plan) = open(&root, "PROJ-412", Mode::Light).unwrap();
    assert_eq!(dir.ticket, "PROJ-412");
    assert_eq!(plan.header().ticket, "PROJ-412");
}

#[test]
fn a_header_ticket_that_disagrees_with_the_directory_is_an_error() {
    let (_dir, root) = project(&["PROJ-412-reserve-stock"]);
    fs::write(
        root.plans_dir().join("PROJ-412-reserve-stock/spec.md"),
        plan_text("PROJ-413"),
    )
    .unwrap();
    let err = open(&root, "PROJ-412", Mode::Light).unwrap_err();
    assert!(matches!(err, LookupError::TicketMismatch { .. }), "{err:?}");
    let message = err.to_string();
    assert!(message.contains("`Ticket: PROJ-413`"), "{message}");
    assert!(message.contains("is for `PROJ-412`"), "{message}");
}

#[test]
fn an_invalid_plan_is_reported_with_its_problems() {
    let (_dir, root) = project(&["PROJ-412-reserve-stock"]);
    fs::write(
        root.plans_dir().join("PROJ-412-reserve-stock/spec.md"),
        plan_text("PROJ-412").replace("Status: draft", "Status: done"),
    )
    .unwrap();
    let err = open(&root, "PROJ-412", Mode::Light).unwrap_err();
    assert!(
        matches!(err, LookupError::Plan(PlanError::Invalid { .. })),
        "{err:?}"
    );
    assert!(err.to_string().contains("`Status` must be"), "{err}");
}

#[cfg(unix)]
#[test]
fn a_symlinked_plan_directory_counts() {
    let (dir, root) = project(&[]);
    let real = dir.path().join("elsewhere/PROJ-412-reserve-stock");
    fs::create_dir_all(&real).unwrap();
    fs::write(real.join(SPEC_FILE), plan_text("PROJ-412")).unwrap();
    std::os::unix::fs::symlink(&real, root.plans_dir().join("PROJ-412-reserve-stock")).unwrap();
    assert_eq!(found(&root, "PROJ-412").ticket, "PROJ-412");
}
