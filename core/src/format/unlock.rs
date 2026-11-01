//! The unlock record of a Plan: `.invar/plans/<TICKET>-<slug>/unlocked`.
//!
//! Implements `contracts/formats/acceptance-lock.md` §11.2. Reading and writing the
//! text lives here; the file itself is `acceptance::unlock`'s.

use std::fmt::{self, Write as _};

/// The rounds a ticket may use (D-27).
pub const MAX_ROUNDS: u32 = 2;

/// One file, and the round that unlocked it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub round: u32,
    /// Relative to the project root, with `/` separators.
    pub path: String,
}

/// What a ticket has unlocked. The default is a ticket with no record.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Record {
    /// The rounds used.
    pub round: u32,
    /// Waiting for `lock`: what grants the skip of the red check (§11.3).
    pub open: Vec<Entry>,
    /// `lock` has run since (D-29).
    pub closed: Vec<Entry>,
}

impl Record {
    pub fn parse(text: &str) -> Result<Self, RecordError> {
        let mut lines = text.strip_suffix('\n').unwrap_or(text).split('\n');
        let first = lines.next().unwrap_or_default();
        let round = first
            .strip_prefix("round: ")
            .and_then(number)
            .filter(|round| (1..=MAX_ROUNDS).contains(round))
            .ok_or_else(|| RecordError {
                line: 1,
                found: first.to_owned(),
            })?;

        let mut record = Self {
            round,
            ..Self::default()
        };
        for (index, line) in lines.enumerate() {
            let invalid = || RecordError {
                line: index + 2,
                found: line.to_owned(),
            };
            let (state, rest) = line.split_once(' ').ok_or_else(invalid)?;
            let (entry_round, path) = rest.split_once(' ').ok_or_else(invalid)?;
            let entry = Entry {
                round: number(entry_round)
                    .filter(|entry_round| (1..=round).contains(entry_round))
                    .ok_or_else(invalid)?,
                path: path.to_owned(),
            };
            if entry.path.is_empty() || entry.path.contains('\r') {
                return Err(invalid());
            }
            match state {
                "open" => record.open.push(entry),
                "closed" => record.closed.push(entry),
                _ => return Err(invalid()),
            }
        }
        Ok(record)
    }

    pub fn render(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "round: {}", self.round);
        for entry in &self.open {
            let _ = writeln!(out, "open {} {}", entry.round, entry.path);
        }
        for entry in &self.closed {
            let _ = writeln!(out, "closed {} {}", entry.round, entry.path);
        }
        out
    }

    /// Every file unlocked so far, with its round, for saying so (§11.4).
    pub fn entries(&self) -> impl Iterator<Item = &Entry> {
        self.closed.iter().chain(&self.open)
    }

    /// The round of a file waiting for `lock`, which is what grants the skip.
    pub fn open_round(&self, path: &str) -> Option<u32> {
        self.open
            .iter()
            .find(|entry| entry.path == path)
            .map(|entry| entry.round)
    }

    /// `lock` succeeded: nothing is open any more, and no old entry can grant a skip
    /// later. The round stays (D-29).
    pub fn close_all(&mut self) {
        self.closed.append(&mut self.open);
    }

    /// Whether every round is used.
    pub fn exhausted(&self) -> bool {
        self.round >= MAX_ROUNDS
    }

    /// One more round, with `paths` open in it. A path already open keeps the later round.
    pub fn unlock(&mut self, paths: &[String]) {
        self.round += 1;
        self.open.retain(|entry| !paths.contains(&entry.path));
        self.open.extend(paths.iter().map(|path| Entry {
            round: self.round,
            path: path.clone(),
        }));
    }
}

fn number(text: &str) -> Option<u32> {
    // Not `parse` alone, which takes `+1` and `01`.
    (text.len() == 1 && text.bytes().all(|b| b.is_ascii_digit()))
        .then(|| text.parse().ok())
        .flatten()
}

/// A line of the record that is not what §11.2 says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordError {
    /// 1-based.
    pub line: usize,
    pub found: String,
}

impl fmt::Display for RecordError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "line {}: `{}` is not a line of an unlock record",
            self.line, self.found
        )
    }
}

impl std::error::Error for RecordError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(round: u32, path: &str) -> Entry {
        Entry {
            round,
            path: path.to_owned(),
        }
    }

    #[test]
    fn a_record_reads_back_what_it_wrote() {
        let record = Record {
            round: 2,
            open: vec![entry(2, "tests/a b.php")],
            closed: vec![entry(1, "tests/a b.php"), entry(1, "tests/c.php")],
        };
        let text = record.render();
        assert_eq!(
            text,
            "round: 2\nopen 2 tests/a b.php\nclosed 1 tests/a b.php\nclosed 1 tests/c.php\n"
        );
        assert_eq!(Record::parse(&text).unwrap(), record);
    }

    #[test]
    fn closing_keeps_the_history_and_the_round_and_grants_nothing_more() {
        let mut record = Record::default();
        record.unlock(&["a".to_owned()]);
        assert_eq!(record.open_round("a"), Some(1));
        assert_eq!(record.open_round("b"), None);
        record.close_all();
        assert_eq!(record.open_round("a"), None);
        assert_eq!(record.round, 1);
        record.unlock(&["a".to_owned()]);
        let rounds: Vec<u32> = record.entries().map(|entry| entry.round).collect();
        assert_eq!(rounds, [1, 2]);
    }

    #[test]
    fn anything_else_is_refused_with_its_line() {
        for (text, line) in [
            ("", 1),
            ("open 1 a\n", 1),
            ("round: 0\n", 1),
            ("round: 3\n", 1),
            ("round: 01\n", 1),
            ("round: +1\n", 1),
            ("round: 1\r\n", 1),
            ("round: 1\nopen 1\n", 2),
            ("round: 1\nopen 2 a\n", 2),
            ("round: 1\nopen 0 a\n", 2),
            ("round: 1\nopen x a\n", 2),
            ("round: 1\nopen 1 \n", 2),
            ("round: 1\nopen 1 a\r\n", 2),
            ("round: 1\nlocked 1 a\n", 2),
            ("round: 1\n\n", 2),
            ("round: 1\nopen 1 a\nround: 2\n", 3),
        ] {
            assert_eq!(Record::parse(text).unwrap_err().line, line, "{text:?}");
        }
    }

    #[test]
    fn a_round_is_one_run_and_a_path_named_again_keeps_the_later_round() {
        let mut record = Record::default();
        assert!(!record.exhausted());
        record.unlock(&["a".to_owned(), "b".to_owned()]);
        assert_eq!(record.round, 1);
        record.unlock(&["b".to_owned()]);
        assert_eq!(record.round, 2);
        assert_eq!(record.open, [entry(1, "a"), entry(2, "b")]);
        assert!(record.exhausted());
    }
}
