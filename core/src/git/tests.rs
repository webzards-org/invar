//! Tests for the date arithmetic. Running `git` itself is covered by the CLI
//! tests, which control its configuration through the environment.

use super::{civil_from_days, date_from_ident};

fn date(ident: &str) -> Option<String> {
    date_from_ident(ident)
}

#[test]
fn the_date_is_local_to_the_committer_offset() {
    // 2026-10-04T22:30:00Z: still the 4th in UTC, already the 5th in
    // Belgrade, and the 4th in New York.
    let utc_seconds = 1_791_153_000;
    assert_eq!(
        date(&format!("rade <r@x> {utc_seconds} +0000")).as_deref(),
        Some("2026-10-04")
    );
    assert_eq!(
        date(&format!("rade <r@x> {utc_seconds} +0200")).as_deref(),
        Some("2026-10-05")
    );
    assert_eq!(
        date(&format!("rade <r@x> {utc_seconds} -0400")).as_deref(),
        Some("2026-10-04")
    );
    // Minutes count: 22:30Z plus 1h30 is exactly midnight, plus 1h29 is not.
    assert_eq!(
        date(&format!("a <a@x> {utc_seconds} +0130")).as_deref(),
        Some("2026-10-05")
    );
    assert_eq!(
        date(&format!("a <a@x> {utc_seconds} +0129")).as_deref(),
        Some("2026-10-04")
    );
}

#[test]
fn names_with_spaces_and_angle_brackets_do_not_matter() {
    assert_eq!(
        date("Rade  Brković <rade@example.com> 0 +0000").as_deref(),
        Some("1970-01-01")
    );
}

#[test]
fn known_instants() {
    for (seconds, expected) in [
        (0_i64, "1970-01-01"),
        (-1, "1969-12-31"),
        (951_782_400, "2000-02-29"),
        (951_868_800, "2000-03-01"),
        (1_709_164_800, "2024-02-29"),
        (4_102_444_799, "2099-12-31"),
        (4_107_542_400, "2100-03-01"),
    ] {
        assert_eq!(
            date(&format!("x <x> {seconds} +0000")).as_deref(),
            Some(expected),
            "{seconds}"
        );
    }
}

#[test]
fn malformed_idents_are_none() {
    for ident in [
        "",
        "x <x>",
        "x <x> 1791153000",
        "x <x> soon +0200",
        "x <x> 1791153000 0200",
        "x <x> 1791153000 +200",
        "x <x> 1791153000 +02:00",
        "x <x> 1791153000 +0260",
        "x <x> 9223372036854775807 +0100",
    ] {
        assert_eq!(date(ident), None, "{ident:?}");
    }
}

#[test]
fn civil_from_days_agrees_with_counting_day_by_day() {
    fn is_leap(year: i64) -> bool {
        (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
    }
    let mut expected = (1900_i64, 1_u32, 1_u32);
    // 1900-01-01 is day -25567.
    for days in -25_567..80_000 {
        assert_eq!(civil_from_days(days), expected, "day {days}");
        let (year, month, day) = expected;
        let length = match month {
            2 if is_leap(year) => 29,
            2 => 28,
            4 | 6 | 9 | 11 => 30,
            _ => 31,
        };
        expected = if day < length {
            (year, month, day + 1)
        } else if month < 12 {
            (year, month + 1, 1)
        } else {
            (year + 1, 1, 1)
        };
    }
}
