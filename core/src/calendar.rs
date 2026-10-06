//! Proleptic Gregorian calendar arithmetic, in days since 1970-01-01.
//!
//! The standard library has no calendar, and the core needs only two
//! conversions: an instant git reports into a date and time
//! (`Approved-at`, a decision entry's `date` and filename), and a decision
//! entry's `date` back into an instant, to check it against the filename.
//! Howard Hinnant's algorithms, exact for every date with a four-digit year.

/// Days since 1970-01-01 → (year, month, day).
pub fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097); // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11], March-based
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// (year, month, day) → days since 1970-01-01, or `None` for a day that does
/// not exist (`2026-02-30`, month 13).
pub fn days_from_civil(year: i64, month: u32, day: u32) -> Option<i64> {
    if !(1..=12).contains(&month) || day == 0 {
        return None;
    }
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400); // [0, 399]
    let mp = i64::from(if month > 2 { month - 3 } else { month + 9 }); // [0, 11]
    let doy = (153 * mp + 2) / 5 + i64::from(day) - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    let days = era * 146_097 + doe - 719_468;
    // A day past the end of its month lands in the next one.
    (civil_from_days(days) == (year, month, day)).then_some(days)
}

#[cfg(test)]
mod tests {
    use super::{civil_from_days, days_from_civil};

    #[test]
    fn the_two_conversions_are_inverse() {
        for days in -800_000..800_000 {
            let (year, month, day) = civil_from_days(days);
            assert_eq!(days_from_civil(year, month, day), Some(days), "day {days}");
        }
    }

    #[test]
    fn days_that_do_not_exist_are_none() {
        for (year, month, day) in [
            (2026, 2, 29),
            (2100, 2, 29),
            (2026, 4, 31),
            (2026, 13, 1),
            (2026, 0, 1),
            (2026, 1, 0),
        ] {
            assert_eq!(
                days_from_civil(year, month, day),
                None,
                "{year}-{month}-{day}"
            );
        }
        assert!(days_from_civil(2024, 2, 29).is_some());
        assert!(days_from_civil(2000, 2, 29).is_some());
        assert_eq!(days_from_civil(1970, 1, 1), Some(0));
    }
}
