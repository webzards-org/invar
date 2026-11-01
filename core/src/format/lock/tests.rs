//! Tests for the `invar-lock:` line (`contracts/formats/acceptance-lock.md`).
//!
//! The first group runs every file of `contracts/fixtures/acceptance/` against
//! `locks.json`, the expectation shared with any other implementation. The rest
//! pin down single rules, so a failure names the rule that broke.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::{LockError, LockLine, holds_digest, read, with_digest, without_digest};
use crate::digest::{canonicalize, digest};

const DIGEST: &str = "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../contracts/fixtures/acceptance")
}

fn manifest() -> Value {
    let raw = fs::read_to_string(fixtures_dir().join("locks.json")).unwrap();
    serde_json::from_str(&raw).unwrap()
}

fn lock(text: &str) -> LockLine {
    read(text.as_bytes()).unwrap().unwrap()
}

fn lock_error(text: &str) -> LockError {
    read(text.as_bytes()).unwrap_err()
}

// --- fixtures ---------------------------------------------------------------

#[test]
fn every_fixture_reads_as_locks_json_says() {
    let manifest = manifest();
    let fixtures = manifest["fixtures"].as_array().unwrap();
    assert!(!fixtures.is_empty());

    for fixture in fixtures {
        let path = fixture["path"].as_str().unwrap();
        // Bytes, never a line-oriented reader: it would make CRLF and LF the same.
        let bytes = fs::read(fixtures_dir().join(path)).unwrap();
        assert_eq!(
            bytes.len() as u64,
            fixture["bytes"].as_u64().unwrap(),
            "{path}: something rewrote this fixture (check .gitattributes and your editor)"
        );

        let number = |field: &str| fixture[field].as_u64().unwrap() as usize;
        let text = |field: &str| fixture[field].as_str().unwrap();
        match (fixture["result"].as_str().unwrap(), read(&bytes)) {
            ("none", Ok(None)) => {}
            ("pending", Ok(Some(lock))) => {
                assert_eq!(lock.ticket, text("ticket"), "{path}");
                assert_eq!(lock.line, number("line"), "{path}");
                assert_eq!(lock.recorded, None, "{path}");
                assert_eq!(lock.digest, text("digest"), "{path}");
                assert!(!lock.matches(), "{path}");
            }
            ("locked", Ok(Some(lock))) => {
                assert_eq!(lock.ticket, text("ticket"), "{path}");
                assert_eq!(lock.line, number("line"), "{path}");
                assert_eq!(lock.recorded.as_deref(), Some(text("recorded")), "{path}");
                assert_eq!(lock.digest, text("digest"), "{path}");
                assert_eq!(
                    lock.matches(),
                    fixture["matches"].as_bool().unwrap(),
                    "{path}"
                );
            }
            ("error", Err(err)) => {
                let lines = |field: &[usize]| {
                    let expected: Vec<usize> = fixture["lines"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|line| line.as_u64().unwrap() as usize)
                        .collect();
                    assert_eq!(field, expected, "{path}");
                };
                let kind = text("kind");
                match &err {
                    LockError::InvalidUtf8(invalid) => {
                        assert_eq!(kind, "invalid-utf8", "{path}");
                        assert_eq!(invalid.offset, number("offset"), "{path}");
                    }
                    LockError::MoreThanOne { lines: found } => {
                        assert_eq!(kind, "more-than-one", "{path}");
                        lines(found);
                    }
                    LockError::AfterWindow { line } => {
                        assert_eq!(kind, "after-window", "{path}");
                        lines(&[*line]);
                    }
                    LockError::Ticket { line, .. } => {
                        assert_eq!(kind, "ticket", "{path}: {err}");
                        lines(&[*line]);
                    }
                    LockError::Digest { line, .. } => {
                        assert_eq!(kind, "digest", "{path}: {err}");
                        lines(&[*line]);
                    }
                    other => panic!("{path}: `read` never returns {other:?}"),
                }
            }
            (expected, got) => panic!("{path}: expected {expected}, got {got:?}"),
        }
    }
}

#[test]
fn the_fixtures_cover_every_kind_of_result() {
    let manifest = manifest();
    let fixtures = manifest["fixtures"].as_array().unwrap();
    for result in ["none", "pending", "locked", "error"] {
        assert!(fixtures.iter().any(|f| f["result"] == result), "{result}");
    }
    for kind in [
        "more-than-one",
        "after-window",
        "ticket",
        "digest",
        "invalid-utf8",
    ] {
        assert!(fixtures.iter().any(|f| f["kind"] == kind), "{kind}");
    }
}

#[test]
fn the_files_listed_together_share_one_digest() {
    let manifest = manifest();
    let groups = manifest["same_digest"].as_array().unwrap();
    assert!(!groups.is_empty());
    for group in groups {
        let digests: Vec<String> = group
            .as_array()
            .unwrap()
            .iter()
            .map(|path| {
                let bytes = fs::read(fixtures_dir().join(path.as_str().unwrap())).unwrap();
                read(&bytes).unwrap().unwrap().digest
            })
            .collect();
        assert!(digests.windows(2).all(|pair| pair[0] == pair[1]), "{group}");
    }
}

// --- the contract's worked example -------------------------------------------

#[test]
fn the_contract_states_the_digest_it_says_it_does() {
    let body = "def test_ac1():\n    assert False\n";
    let expected = digest(body);
    let file = format!("# invar-lock: SHOP-214 {expected}\n{body}");
    let line = lock(&file);
    assert_eq!(line.digest, expected);
    assert!(line.matches());
    assert!(lock(&file.replace('\n', "\r\n")).matches());

    let contract =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../contracts/formats/acceptance-lock.md");
    let contract = fs::read_to_string(contract).unwrap();
    assert!(
        contract.contains(&format!("# invar-lock: SHOP-214 {expected}\n{body}")),
        "example 8.2 does not show {expected}"
    );
}

// --- the line -----------------------------------------------------------------

#[test]
fn anything_before_the_marker_is_ignored() {
    for prefix in [
        "",
        "// ",
        "# ",
        "-- ",
        "/* ",
        "<!-- ",
        " * ",
        "prefix-text",
        "%% %",
    ] {
        let line = lock(&format!("{prefix}invar-lock: SHOP-214\nx\n"));
        assert_eq!(line.ticket, "SHOP-214", "{prefix:?}");
        assert_eq!(line.recorded, None, "{prefix:?}");
    }
}

#[test]
fn the_first_marker_on_a_line_is_the_one_read() {
    let line = lock("invar-lock: SHOP-214 invar-lock: OTHER-1\n");
    assert_eq!(line.ticket, "SHOP-214");
}

#[test]
fn the_ticket_rule_is_the_plan_headers() {
    for ticket in ["A-1", "SHOP-214", "S1-2", "AB9C-0007"] {
        assert_eq!(lock(&format!("# invar-lock: {ticket}\n")).ticket, ticket);
    }
    for ticket in [
        "shop-214", "1A-2", "SHOP", "SHOP-", "-214", "SHOP-2-1", "Shop-1",
    ] {
        assert!(
            matches!(
                lock_error(&format!("# invar-lock: {ticket}\n")),
                LockError::Ticket { line: 1, .. }
            ),
            "{ticket}"
        );
    }
}

#[test]
fn only_spaces_separate_the_fields() {
    for line in [
        "# invar-lock:\tSHOP-214\n",
        "# invar-lock: \tSHOP-214\n",
        "# invar-lock:\u{00A0}SHOP-214\n",
        "# invar-lock:SHOP-214\n",
    ] {
        assert!(
            matches!(lock_error(line), LockError::Ticket { .. }),
            "{line:?}"
        );
    }
    let tabbed = format!("# invar-lock: SHOP-214\t{DIGEST}\n");
    assert!(matches!(lock_error(&tabbed), LockError::Digest { .. }));
    assert_eq!(
        lock(&format!("# invar-lock:   SHOP-214   {DIGEST}\n"))
            .recorded
            .as_deref(),
        Some(DIGEST)
    );
}

#[test]
fn trailing_text_follows_any_whitespace_and_is_ignored() {
    for rest in [" */", "\t*/", " -->", " anything at all", "\u{00A0}x"] {
        assert_eq!(
            lock(&format!("/* invar-lock: SHOP-214{rest}\n")).recorded,
            None
        );
        assert_eq!(
            lock(&format!("/* invar-lock: SHOP-214 {DIGEST}{rest}\n"))
                .recorded
                .as_deref(),
            Some(DIGEST),
            "{rest:?}"
        );
    }
}

#[test]
fn a_field_that_starts_with_sha256_is_never_trailing_text() {
    let upper = format!("sha256:{}", "A".repeat(64));
    let long = format!("{DIGEST}0");
    let short = &DIGEST[..DIGEST.len() - 1];
    for field in [
        "sha256:",
        "sha256:abc",
        short,
        long.as_str(),
        upper.as_str(),
    ] {
        for line in [
            format!("# invar-lock: SHOP-214 {field}\n"),
            format!("# invar-lock: SHOP-214 by-hand {field}\n"),
            format!("# invar-lock: SHOP-214 {DIGEST} {field}\n"),
        ] {
            match lock_error(&line) {
                LockError::Digest { line: 1, found } => assert_eq!(found, field),
                other => panic!("{line:?}: {other:?}"),
            }
        }
    }
    // A digest out of place, even a well-formed one.
    for line in [
        format!("# invar-lock: SHOP-214 by-hand {DIGEST}\n"),
        format!("# invar-lock: SHOP-214 {DIGEST} {DIGEST}\n"),
    ] {
        assert!(
            matches!(lock_error(&line), LockError::Digest { .. }),
            "{line:?}"
        );
    }
    // A word that only contains it is ordinary text.
    assert_eq!(
        lock("# invar-lock: SHOP-214 not-sha256:abc\n").recorded,
        None
    );
}

// --- where ------------------------------------------------------------------

fn at_line(number: usize) -> String {
    format!("{}# invar-lock: SHOP-214\nx\n", "y\n".repeat(number - 1))
}

#[test]
fn the_window_is_ten_lines() {
    assert_eq!(lock(&at_line(1)).line, 1);
    assert_eq!(lock(&at_line(10)).line, 10);
    assert_eq!(
        lock_error(&at_line(11)),
        LockError::AfterWindow { line: 11 }
    );
}

#[test]
fn line_endings_and_a_bom_do_not_move_the_line() {
    let lf = at_line(10);
    for text in [
        lf.replace('\n', "\r\n"),
        lf.replace('\n', "\r"),
        format!("\u{FEFF}{lf}"),
    ] {
        assert_eq!(lock(&text).line, 10, "{text:?}");
    }
    assert_eq!(
        lock_error(&format!("\u{FEFF}{}", at_line(11).replace('\n', "\r\n"))),
        LockError::AfterWindow { line: 11 }
    );
    // Only line endings end a line: U+2028 is content.
    assert_eq!(lock("a\u{2028}b\n# invar-lock: SHOP-214\n").line, 2);
}

#[test]
fn more_than_one_line_is_an_error_anywhere_in_the_file() {
    let far = format!(
        "# invar-lock: SHOP-214\n{}# invar-lock: SHOP-215\n",
        "x\n".repeat(98)
    );
    assert_eq!(
        lock_error(&far),
        LockError::MoreThanOne {
            lines: vec![1, 100]
        }
    );
    // It comes before the checks of a single line, whatever those would say.
    assert_eq!(
        lock_error("invar-lock:\ninvar-lock:\ninvar-lock:\n"),
        LockError::MoreThanOne {
            lines: vec![1, 2, 3]
        }
    );
}

#[test]
fn no_marker_is_no_lock_and_not_an_error() {
    for text in [
        "",
        "\n\n",
        "x\n",
        "INVAR-LOCK: SHOP-214\n",
        "invar-lock SHOP-214\n",
    ] {
        assert_eq!(read(text.as_bytes()).unwrap(), None, "{text:?}");
    }
}

#[test]
fn invalid_utf8_is_canonicalizations_own_error() {
    match read(b"# invar-lock: SHOP-214\n\xE9\n").unwrap_err() {
        LockError::InvalidUtf8(err) => assert_eq!(err.offset, 23),
        other => panic!("{other:?}"),
    }
}

// --- the digest ---------------------------------------------------------------

#[test]
fn the_digest_excludes_the_line_and_nothing_else() {
    let base = lock("# invar-lock: SHOP-214\nalpha\n * AC1: text\n").digest;
    assert_eq!(base, digest("alpha\n * AC1: text\n"));
    for same in [
        "// invar-lock: SHOP-214\nalpha\n * AC1: text\n".to_owned(),
        "# invar-lock: OTHER-9\nalpha\n * AC1: text\n".to_owned(),
        format!("# invar-lock: SHOP-214 {DIGEST}\nalpha\n * AC1: text\n"),
        "# invar-lock: SHOP-214 anything\nalpha\n * AC1: text\n".to_owned(),
        "alpha\n# invar-lock: SHOP-214\n * AC1: text\n".to_owned(),
        "alpha\n * AC1: text\n# invar-lock: SHOP-214\n".to_owned(),
        "# invar-lock: SHOP-214\r\nalpha  \r\n * AC1: text\r\n\r\n".to_owned(),
    ] {
        assert_eq!(lock(&same).digest, base, "{same:?}");
    }
    // A character of the criterion's text is covered (D-4).
    for different in [
        "# invar-lock: SHOP-214\nalpha\n * AC1: test\n",
        "# invar-lock: SHOP-214\nalpha\n\n * AC1: text\n",
        "\n# invar-lock: SHOP-214\nalpha\n * AC1: text\n",
    ] {
        assert_ne!(lock(different).digest, base, "{different:?}");
    }
}

#[test]
fn removing_the_line_strips_the_blank_lines_it_leaves_at_the_end() {
    assert_eq!(
        lock("alpha\n\n\n# invar-lock: SHOP-214\n").digest,
        digest("alpha\n")
    );
    assert_eq!(lock("# invar-lock: SHOP-214\n").digest, digest(""));
    assert_eq!(lock("\n\n# invar-lock: SHOP-214\n\n").digest, digest(""));
}

#[test]
fn the_file_is_not_canonicalized_twice() {
    // D-7: a second pass would strip this U+FEFF as a BOM, and give two different
    // files one digest.
    let with = lock("# invar-lock: SHOP-214\n\u{FEFF}alpha\n").digest;
    assert_eq!(with, digest("\u{FEFF}alpha\n"));
    assert_ne!(with, lock("# invar-lock: SHOP-214\nalpha\n").digest);
}

#[test]
fn matches_needs_a_recorded_digest_equal_to_the_files() {
    let body = "alpha\n";
    let right = digest(&canonicalize(body));
    assert!(lock(&format!("# invar-lock: SHOP-214 {right}\n{body}")).matches());
    assert!(!lock(&format!("# invar-lock: SHOP-214 {DIGEST}\n{body}")).matches());
    assert!(!lock(&format!("# invar-lock: SHOP-214\n{body}")).matches());
    let edited = format!("# invar-lock: SHOP-214 {right}\nalphb\n");
    assert!(!lock(&edited).matches());
}

// --- with_digest ----------------------------------------------------------------

/// Check that `written` is `original` with only ` <digest>` added, right after
/// the ticket, and that it now reads as locked and matching.
fn assert_only_the_digest_was_added(original: &[u8], written: &[u8], ticket: &str) {
    let before = read(original).unwrap().unwrap();
    let inserted = format!(" {}", before.digest);
    let original_text = std::str::from_utf8(original).unwrap();
    let written_text = std::str::from_utf8(written).unwrap();

    let at = written_text.find(&inserted).unwrap();
    assert_eq!(written.len(), original.len() + inserted.len());
    assert_eq!(&written[..at], &original[..at]);
    assert_eq!(&written[at + inserted.len()..], &original[at..]);
    assert!(original_text[..at].ends_with(ticket), "{original_text:?}");

    let after = read(written).unwrap().unwrap();
    assert!(after.matches(), "{written_text:?}");
    assert_eq!(after.ticket, ticket);
    assert_eq!(after.line, before.line);
    assert_eq!(after.digest, before.digest);
}

#[test]
fn with_digest_round_trips_every_pending_fixture() {
    let manifest = manifest();
    let pending: Vec<&Value> = manifest["fixtures"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|fixture| fixture["result"] == "pending")
        .collect();
    assert!(!pending.is_empty());
    for fixture in pending {
        let bytes = fs::read(fixtures_dir().join(fixture["path"].as_str().unwrap())).unwrap();
        let ticket = fixture["ticket"].as_str().unwrap();
        let written = with_digest(&bytes, ticket).unwrap();
        assert_only_the_digest_was_added(&bytes, &written, ticket);
    }
}

#[test]
fn with_digest_keeps_every_other_byte() {
    for text in [
        // CRLF, on the line and off it.
        "<?php\r\n// invar-lock: SHOP-214\r\n\r\nbody\r\n",
        // A BOM.
        "\u{FEFF}# invar-lock: SHOP-214\nbody\n",
        // A BOM, mixed line endings, lone CR, trailing whitespace and blank lines.
        "\u{FEFF}a  \r\n# invar-lock: SHOP-214\rb\t\nc \r\n\n\n",
        // Trailing whitespace on the line itself, no final newline.
        "# invar-lock: SHOP-214   \nbody",
        "# invar-lock: SHOP-214",
        // Spaces inside the line stay as they were.
        "#   invar-lock:     SHOP-214\nbody\n",
    ] {
        let written = with_digest(text.as_bytes(), "SHOP-214").unwrap();
        assert_only_the_digest_was_added(text.as_bytes(), &written, "SHOP-214");
    }
}

#[test]
fn with_digest_keeps_the_prefix_and_the_trailing_text() {
    let digest = digest("body\n");
    for (before, after) in [
        (
            "/* invar-lock: SHOP-214 */\r\nbody\r\n".to_owned(),
            format!("/* invar-lock: SHOP-214 {digest} */\r\nbody\r\n"),
        ),
        (
            "<!-- invar-lock: SHOP-214 -->\nbody\n".to_owned(),
            format!("<!-- invar-lock: SHOP-214 {digest} -->\nbody\n"),
        ),
        (
            " * invar-lock: SHOP-214\twritten by hand\nbody\n".to_owned(),
            format!(" * invar-lock: SHOP-214 {digest}\twritten by hand\nbody\n"),
        ),
    ] {
        let written = with_digest(before.as_bytes(), "SHOP-214").unwrap();
        assert_eq!(String::from_utf8(written).unwrap(), after);
    }
}

#[test]
fn with_digest_is_not_confused_by_a_ticket_in_the_prefix() {
    let text = "SHOP-214 invar-lock: SHOP-214\nbody\n";
    let written = with_digest(text.as_bytes(), "SHOP-214").unwrap();
    assert_eq!(
        String::from_utf8(written).unwrap(),
        format!("SHOP-214 invar-lock: SHOP-214 {}\nbody\n", digest("body\n"))
    );
}

#[test]
fn with_digest_refuses_what_it_cannot_lock() {
    assert_eq!(with_digest(b"body\n", "SHOP-214"), Err(LockError::NoLine));
    assert_eq!(with_digest(b"", "SHOP-214"), Err(LockError::NoLine));
    assert_eq!(
        with_digest(b"x\n# invar-lock: OTHER-1\n", "SHOP-214"),
        Err(LockError::OtherTicket {
            line: 2,
            found: "OTHER-1".to_owned(),
            wanted: "SHOP-214".to_owned(),
        })
    );
    // Already locked, whether or not the digest still holds.
    for recorded in [digest("body\n"), DIGEST.to_owned()] {
        let text = format!("# invar-lock: SHOP-214 {recorded}\nbody\n");
        assert_eq!(
            with_digest(text.as_bytes(), "SHOP-214"),
            Err(LockError::AlreadyLocked {
                line: 1,
                ticket: "SHOP-214".to_owned(),
            })
        );
    }
    // Locked for another ticket: the ticket is what is wrong first.
    let text = format!("# invar-lock: OTHER-1 {DIGEST}\nbody\n");
    assert!(matches!(
        with_digest(text.as_bytes(), "SHOP-214"),
        Err(LockError::OtherTicket { .. })
    ));
    // What `read` refuses, this refuses too.
    assert!(matches!(
        with_digest(&at_line(11).into_bytes(), "SHOP-214"),
        Err(LockError::AfterWindow { line: 11 })
    ));
    assert!(matches!(
        with_digest(b"# invar-lock: SHOP-214\n\xE9\n", "SHOP-214"),
        Err(LockError::InvalidUtf8(_))
    ));
}

#[test]
fn a_file_cannot_be_locked_twice() {
    let once = with_digest(b"# invar-lock: SHOP-214\nbody\n", "SHOP-214").unwrap();
    assert!(matches!(
        with_digest(&once, "SHOP-214"),
        Err(LockError::AlreadyLocked { .. })
    ));
}

// --- messages -----------------------------------------------------------------

#[test]
fn every_error_names_its_line_and_what_to_do() {
    for (error, expected) in [
        (
            lock_error("invar-lock: A-1\ninvar-lock: A-2\n"),
            &["lines 1, 2", "more than one", "remove the others"][..],
        ),
        (
            lock_error(&at_line(11)),
            &["line 11", "first 10 lines", "move it back up"],
        ),
        (
            lock_error("invar-lock: shop\n"),
            &["line 1", "`PROJ-412`", "found `shop`"],
        ),
        (
            lock_error("invar-lock: A-1 sha256:abc\n"),
            &["line 1", "`sha256:abc`", "64 lowercase hex"],
        ),
        (LockError::NoLine, &["no `invar-lock:` line"]),
        (
            LockError::OtherTicket {
                line: 2,
                found: "OTHER-1".to_owned(),
                wanted: "SHOP-214".to_owned(),
            },
            &["line 2", "`OTHER-1`", "`SHOP-214` cannot lock"],
        ),
        (
            LockError::AlreadyLocked {
                line: 3,
                ticket: "SHOP-214".to_owned(),
            },
            &["line 3", "already locked", "`SHOP-214`"],
        ),
    ] {
        let message = error.to_string();
        for part in expected {
            assert!(message.contains(part), "{part}: {message}");
        }
    }
}

// --- holds_digest -----------------------------------------------------------------

#[test]
fn holds_digest_finds_a_digest_on_a_lock_line_wherever_it_stands() {
    let digest = format!("sha256:{}", "ab".repeat(32));
    for text in [
        format!("# invar-lock: SHOP-214 {digest}\n"),
        format!("/* invar-lock: SHOP-214 {digest} */"),
        format!("first\r\n// invar-lock: SHOP-214 {digest}\r\nlast"),
        format!("first\r// invar-lock: SHOP-214 {digest}\rlast"),
        // Past the window, and beside another line: `read` would call these other errors.
        format!("{}# invar-lock: SHOP-214 {digest}\n", "x\n".repeat(20)),
        format!("# invar-lock: SHOP-214\n# invar-lock: SHOP-215 {digest}\n"),
    ] {
        assert!(holds_digest(&text), "{text:?}");
    }
}

#[test]
fn holds_digest_counts_a_malformed_sha256_field_after_a_ticket() {
    for text in [
        "# invar-lock: SHOP-214 sha256:abc\n",
        "# invar-lock: SHOP-214 by-hand sha256:abc\n",
        "# invar-lock: SHOP-214\tsha256:abc\n",
    ] {
        assert!(holds_digest(text), "{text:?}");
    }
}

#[test]
fn holds_digest_is_false_for_what_is_not_a_digest_on_a_lock_line() {
    let digest = format!("sha256:{}", "ab".repeat(32));
    for text in [
        "",
        "# invar-lock: SHOP-214\n",
        "# invar-lock: SHOP-214 written by the test-writer\n",
        "# invar-lock:\n",
        "# invar-lock: shop-214\n",
        &format!("the digest is {digest}\n"),
        &format!("# invar-lock: SHOP-214\n{digest}\n"),
        &format!("# invar-lock: shop-214 {digest}\n"),
    ] {
        assert!(!holds_digest(text), "{text:?}");
    }
}

// --- without_digest ---------------------------------------------------------------------------

#[test]
fn without_digest_is_the_inverse_of_with_digest_byte_for_byte() {
    for pending in [
        "# invar-lock: SHOP-214\ndef test():\n    assert False\n",
        "# invar-lock:    SHOP-214 written by hand\nbody\n",
        "/* invar-lock: SHOP-214 */\nbody\n",
        "<!-- invar-lock: SHOP-214\t-->\nbody\n",
        "# invar-lock: SHOP-214\r\nbody   \r\n",
        "\u{feff}# invar-lock: SHOP-214\nbody\n\n\n",
        "# invar-lock: SHOP-214",
    ] {
        let locked = with_digest(pending.as_bytes(), "SHOP-214").unwrap();
        assert_ne!(locked, pending.as_bytes());
        assert_eq!(
            without_digest(&locked, "SHOP-214").unwrap(),
            pending.as_bytes(),
            "{pending:?}"
        );
    }
}

#[test]
fn without_digest_leaves_a_pending_line_with_the_digest_the_file_had() {
    let pending = b"# invar-lock: SHOP-214\nbody\n";
    let locked = with_digest(pending, "SHOP-214").unwrap();
    let line = read(&without_digest(&locked, "SHOP-214").unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(line.recorded, None);
    assert_eq!(line.digest, read(&locked).unwrap().unwrap().digest);
}

#[test]
fn without_digest_takes_the_spaces_a_hand_written_line_had_before_it() {
    let digest = format!("sha256:{}", "a".repeat(64));
    let bytes = format!("# invar-lock: SHOP-214   {digest} trailing\nbody\n");
    assert_eq!(
        without_digest(bytes.as_bytes(), "SHOP-214").unwrap(),
        b"# invar-lock: SHOP-214 trailing\nbody\n"
    );
}

#[test]
fn without_digest_refuses_no_line_another_ticket_and_a_pending_line() {
    assert_eq!(
        without_digest(b"body\n", "SHOP-214").unwrap_err(),
        LockError::NoLine
    );
    let digest = format!("sha256:{}", "a".repeat(64));
    assert!(matches!(
        without_digest(
            format!("# invar-lock: SHOP-9 {digest}\n").as_bytes(),
            "SHOP-214"
        ),
        Err(LockError::OtherTicket { .. })
    ));
    assert!(matches!(
        without_digest(b"# invar-lock: SHOP-214\n", "SHOP-214"),
        Err(LockError::NotLocked { .. })
    ));
}
