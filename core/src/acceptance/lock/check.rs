//! What a report has to say for its files to be locked (`acceptance-lock.md` §9.3).
//!
//! Pure: a Plan's criteria, the files and the test cases come in, and the coverage or
//! every problem comes out (D-14).

use std::collections::BTreeMap;
use std::fmt;

use crate::format::junit::{Criterion as Token, Outcome, TestCase};
use crate::format::plan::Criterion;

use super::find::Found;

/// How many lines of a failure's message a refusal shows.
const MESSAGE_LINES: usize = 5;

/// One test, as the output names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Covering {
    pub file: String,
    pub name: String,
}

/// Each criterion of the Plan, in its order, with the tests that cover it.
pub type Coverage = Vec<(String, Vec<Covering>)>;

/// Decide on the test cases of a run.
pub fn check(
    ticket: &str,
    criteria: &[Criterion],
    found: &Found,
    unlocked: &[&str],
    cases: &[TestCase],
) -> Result<Coverage, Problems> {
    let mut coverage: Coverage = criteria
        .iter()
        .map(|criterion| (criterion.id.clone(), Vec::new()))
        .collect();
    let mut problems = Problems {
        ticket: ticket.to_owned(),
        files: BTreeMap::new(),
        uncovered: Vec::new(),
        known: criteria
            .iter()
            .map(|criterion| criterion.id.clone())
            .collect(),
    };

    let to_lock = |file: &str| found.to_lock.iter().any(|one| one.path == file);
    let locked = |file: &str| found.locked.iter().any(|one| one == file);

    for path in found.all_paths() {
        if !cases.iter().any(|case| case.file == path) {
            problems.files.entry(path).or_default().no_tests = true;
        }
    }

    for case in cases {
        let new = to_lock(&case.file);
        // A runner may add test cases of its own (D-10).
        if !new && !locked(&case.file) {
            continue;
        }
        let mut wrong = Vec::new();
        match &case.criterion {
            Token::None => wrong.push(Wrong::NoToken),
            Token::Several(ids) => wrong.push(Wrong::Several(ids.clone())),
            Token::One(id) => match coverage.iter_mut().find(|(known, _)| known == id) {
                Some((_, tests)) => tests.push(Covering {
                    file: case.file.clone(),
                    name: case.name.clone(),
                }),
                None => wrong.push(Wrong::Unknown(id.clone())),
            },
        }
        // The outcome of a locked file does not matter (D-13).
        if new {
            match case.outcome {
                Outcome::Failure => {}
                // The code exists now, so a corrected test may pass (D-28).
                Outcome::Passed if unlocked.contains(&case.file.as_str()) => {}
                Outcome::Passed => wrong.push(Wrong::Passed),
                Outcome::Skipped => wrong.push(Wrong::Skipped),
                Outcome::Error => wrong.push(Wrong::Error {
                    failure_type: case.failure_type.clone(),
                    message: case.message.clone(),
                }),
            }
        }
        if !wrong.is_empty() {
            problems
                .files
                .entry(case.file.clone())
                .or_default()
                .tests
                .push(TestProblem {
                    name: case.name.clone(),
                    token: match &case.criterion {
                        Token::One(id) => Some(id.clone()),
                        _ => None,
                    },
                    wrong,
                });
        }
    }

    for criterion in criteria {
        let covered = coverage
            .iter()
            .any(|(id, tests)| *id == criterion.id && !tests.is_empty());
        if !covered {
            problems.uncovered.push(criterion.clone());
        }
    }

    if problems.files.is_empty() && problems.uncovered.is_empty() {
        Ok(coverage)
    } else {
        Err(problems)
    }
}

/// Everything wrong with a run, grouped by file and test.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problems {
    ticket: String,
    files: BTreeMap<String, FileProblems>,
    uncovered: Vec<Criterion>,
    /// The Plan's ids, for the message about one it does not have.
    known: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct FileProblems {
    /// Passed to the run, and no test case of it came back.
    no_tests: bool,
    tests: Vec<TestProblem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TestProblem {
    name: String,
    token: Option<String>,
    wrong: Vec<Wrong>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Wrong {
    NoToken,
    Several(Vec<String>),
    Unknown(String),
    Passed,
    Skipped,
    Error {
        failure_type: Option<String>,
        message: Option<String>,
    },
}

impl Problems {
    fn has_passed(&self) -> bool {
        self.files.values().any(|file| {
            file.tests
                .iter()
                .any(|test| test.wrong.contains(&Wrong::Passed))
        })
    }
}

impl fmt::Display for Problems {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: nothing was locked:", self.ticket)?;
        for (file, problems) in &self.files {
            write!(f, "\n\n{file}")?;
            if problems.no_tests {
                write!(
                    f,
                    "\n  no test from this file ran: a syntax error or a filter can do this"
                )?;
            }
            for test in &problems.tests {
                match &test.token {
                    Some(id) => write!(f, "\n  `{}` ({id})", test.name)?,
                    None => write!(f, "\n  `{}`", test.name)?,
                }
                for wrong in &test.wrong {
                    write!(f, "\n    ")?;
                    wrong.describe(f, &self.known)?;
                }
            }
        }
        if !self.uncovered.is_empty() {
            write!(f, "\n\nno test covers:")?;
            for criterion in &self.uncovered {
                write!(f, "\n  {}: {}", criterion.id, criterion.text)?;
            }
        }
        if self.has_passed() {
            write!(
                f,
                "\n\nA test that passes has no override here: if it cannot fail before the code \
                 exists, answer `BLOCKED` and the developer decides."
            )?;
        }
        write!(
            f,
            "\n\nFix all of these, then run `invar acceptance lock` again."
        )
    }
}

impl Wrong {
    fn describe(&self, f: &mut fmt::Formatter<'_>, known: &[String]) -> fmt::Result {
        match self {
            Self::NoToken => write!(
                f,
                "its name has no `AC<n>` token, so it covers no criterion"
            ),
            Self::Several(ids) => write!(
                f,
                "its name has more than one criterion token ({}), and a test covers exactly one",
                ids.join(", ")
            ),
            Self::Unknown(id) => write!(
                f,
                "it names {id}, which this Plan does not have; it has {}",
                known.join(", ")
            ),
            Self::Passed => write!(
                f,
                "passes before the code exists, so it proves nothing about the code (A-4)"
            ),
            Self::Skipped => write!(f, "a skipped test proves nothing"),
            Self::Error {
                failure_type,
                message,
            } => {
                write!(f, "fails for the wrong reason (an error, not an assertion)")?;
                if let Some(failure_type) = failure_type {
                    write!(f, ": {failure_type}")?;
                }
                let lines = message.as_deref().unwrap_or_default().lines();
                for line in lines.take(MESSAGE_LINES) {
                    write!(f, "\n      {line}")?;
                }
                Ok(())
            }
        }
    }
}
