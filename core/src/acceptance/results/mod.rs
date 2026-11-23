//! Whether the locked acceptance tests passed (`acceptance-lock.md` §10.5).
//!
//! The core never runs the team's suite (A-2). It reads the JUnit reports the team's
//! own CI wrote, or the one `invar acceptance run` had `acceptance.run` write for the
//! ticket's locked files, and decides on them: every locked file has a test case in
//! them, every criterion its line records has a test case of that file naming it, and
//! every one of its test cases passed, in every occurrence.
//!
//! [`locked`] says which files are locked, [`read_reports`] reads the reports, and
//! [`check`] decides. The command that does it for one ticket is [`run`].

use std::collections::BTreeMap;
use std::fmt::{self, Write as _};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::format::junit::{self, Criterion as Token, JunitError, Outcome, TestCase};
use crate::format::plan::Criterion;
use crate::fs::ProjectRoot;

use super::lock::find::{self, FindError, Scan};

pub mod run;

#[cfg(test)]
mod tests;

/// Why `--results` cannot run without an `acceptance:` section (§10.5): unlike D-21, the
/// caller asked for a check.
pub const NO_SECTION: &str = "`--results` asks whether the locked acceptance tests passed, \
    and invar.yaml has no `acceptance:` section, so there is no `acceptance.dir` to find them \
    in. Add the section, or leave out `--results`";

/// What a run of the check is given, from the command line.
#[derive(Debug, Clone, Default)]
pub struct Reports {
    /// The JUnit XML files, as the caller resolved them.
    pub paths: Vec<PathBuf>,
    /// The directory the runner saw as the project root (`test-results.md` §3.1).
    pub report_root: Option<PathBuf>,
}

/// One locked file: its path relative to the project root, its ticket, and the
/// criteria its line records it as covering. `None` for a line locked before lists were
/// recorded (D-40).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockedFile {
    pub path: String,
    pub ticket: String,
    pub criteria: Option<Vec<String>>,
}

/// Every file under `dir` locked with a digest that holds, of `ticket` or of any
/// ticket, sorted. Pending, malformed and changed files are not locked: the integrity
/// check of §10.1 is what reports them, and it runs first.
pub fn locked(
    root: &ProjectRoot,
    dir: &str,
    ticket: Option<&str>,
) -> Result<Vec<LockedFile>, FindError> {
    let Scan::Files(files) = find::scan(root, dir)? else {
        return Ok(Vec::new());
    };
    Ok(files
        .into_iter()
        .filter_map(|file| {
            let line = file.line.ok()?;
            let holds = line.recorded.is_some() && line.matches();
            let wanted = ticket.is_none_or(|ticket| line.ticket == ticket);
            (holds && wanted && file.utf8_path).then_some(LockedFile {
                path: file.path,
                ticket: line.ticket,
                criteria: line.criteria,
            })
        })
        .collect())
}

/// The test cases of every report, in the order given. Every report that cannot be
/// read is named, each with its reason.
pub fn read_reports(root: &ProjectRoot, reports: &Reports) -> Result<Vec<TestCase>, ReportError> {
    let mut cases = Vec::new();
    let mut problems = Vec::new();
    for path in &reports.paths {
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                problems.push((path.clone(), "there is no such file".to_owned()));
                continue;
            }
            Err(err) => {
                problems.push((path.clone(), err.to_string()));
                continue;
            }
        };
        match junit::read_mapped(&bytes, root.path(), reports.report_root.as_deref()) {
            Ok(read) => cases.extend(read),
            Err(err) => problems.push((path.clone(), describe(&err, reports))),
        }
    }
    if problems.is_empty() {
        Ok(cases)
    } else {
        Err(ReportError { problems })
    }
}

/// A reader's error, with what to do about a path outside the root.
fn describe(err: &JunitError, reports: &Reports) -> String {
    match err {
        JunitError::OutsideRoot { .. } if reports.report_root.is_none() => format!(
            "{err}. If the tests ran where the project is at another path, such as \
             /var/www/html under Laravel Sail, give that path with `--report-root`"
        ),
        _ => err.to_string(),
    }
}

/// Reports that could not be read, each with its reason.
#[derive(Debug)]
pub struct ReportError {
    problems: Vec<(PathBuf, String)>,
}

impl fmt::Display for ReportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "cannot read the test results, and nothing was checked:")?;
        for (path, problem) in &self.problems {
            write!(f, "\n{}: {problem}", path.display())?;
        }
        Ok(())
    }
}

impl std::error::Error for ReportError {}

/// One test, with every outcome it had across the reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Test {
    pub file: String,
    pub name: String,
    pub criterion: Token,
    /// In the order found. More than one when the test ran more than once.
    pub outcomes: Vec<Outcome>,
    /// The first line of the message of the first occurrence that did not pass.
    pub message: Option<String>,
}

impl Test {
    /// Every occurrence passed (§10.5): a pass on retry is not one.
    pub fn passed(&self) -> bool {
        self.outcomes
            .iter()
            .all(|outcome| *outcome == Outcome::Passed)
    }

    /// What it was, as said: `failure`, or `passed in one run and failure in another`.
    fn outcome_text(&self) -> String {
        let worst = self
            .outcomes
            .iter()
            .max()
            .copied()
            .unwrap_or(Outcome::Passed);
        if self.outcomes.contains(&Outcome::Passed) {
            format!(
                "passed in one run and {} in another; a test that passes only on a retry is \
                 not accepted",
                name(worst)
            )
        } else {
            name(worst).to_owned()
        }
    }

    /// `` `it AC1 cancels` (AC1) ``, with the token rule of `test-results.md` §4.
    fn label(&self) -> String {
        match &self.criterion {
            Token::One(id) => format!("`{}` ({id})", self.name),
            Token::Several(ids) => format!("`{}` ({})", self.name, ids.join(", ")),
            Token::None => format!("`{}` (no criterion id)", self.name),
        }
    }

    /// One line: the test, what it was, and the first line of its message.
    fn describe(&self) -> String {
        let mut line = format!("{}: {}: {}", self.file, self.label(), self.outcome_text());
        if let Some(message) = &self.message {
            let _ = write!(line, ": {message}");
        }
        line
    }
}

fn name(outcome: Outcome) -> &'static str {
    match outcome {
        Outcome::Passed => "passed",
        Outcome::Skipped => "skipped",
        Outcome::Failure => "failure",
        Outcome::Error => "error",
    }
}

/// What the reports say of the locked files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Results {
    /// The tests of the locked files, sorted by file, then in the order first found.
    pub tests: Vec<Test>,
    /// Locked files with no test case in any report.
    pub not_run: Vec<String>,
    /// A criterion the line of a file records, with no test case of that file naming it:
    /// the file ran only in part (D-38).
    pub missing: Vec<(String, String)>,
    /// Locked files whose line records no criteria, so only a run of some test is checked.
    pub unlisted: Vec<String>,
    /// `acceptance.dir`, for what to do about a file that did not run.
    dir: String,
}

/// Decide on the test cases of the reports for the `locked` files. Test cases of any
/// other file are ignored.
pub fn check(dir: &str, locked: &[LockedFile], cases: &[TestCase]) -> Results {
    let mut by_file: BTreeMap<&str, Vec<Test>> = BTreeMap::new();
    for file in locked {
        by_file.entry(file.path.as_str()).or_default();
    }
    for case in cases {
        let Some(tests) = by_file.get_mut(case.file.as_str()) else {
            continue;
        };
        let first_line = || {
            let message = case.message.as_deref().unwrap_or_default();
            let line = message.lines().find(|line| !line.trim().is_empty());
            line.map(|line| line.trim().to_owned())
                .or_else(|| case.failure_type.clone())
        };
        match tests.iter_mut().find(|test| test.name == case.name) {
            Some(test) => {
                test.outcomes.push(case.outcome);
                if test.message.is_none() && case.outcome != Outcome::Passed {
                    test.message = first_line();
                }
            }
            None => tests.push(Test {
                file: case.file.clone(),
                name: case.name.clone(),
                criterion: case.criterion.clone(),
                outcomes: vec![case.outcome],
                message: (case.outcome != Outcome::Passed).then(first_line).flatten(),
            }),
        }
    }
    let not_run: Vec<String> = by_file
        .iter()
        .filter(|(_, tests)| tests.is_empty())
        .map(|(file, _)| (*file).to_owned())
        .collect();
    let mut missing = Vec::new();
    let mut unlisted = Vec::new();
    for file in locked {
        let Some(criteria) = &file.criteria else {
            unlisted.push(file.path.clone());
            continue;
        };
        // A file that did not run at all is said once, as not run.
        if not_run.contains(&file.path) {
            continue;
        }
        let tests = &by_file[file.path.as_str()];
        for id in criteria {
            let named = tests
                .iter()
                .any(|test| matches!(&test.criterion, Token::One(one) if one == id));
            if !named {
                missing.push((file.path.clone(), id.clone()));
            }
        }
    }
    missing.sort();
    unlisted.sort();
    unlisted.dedup();
    Results {
        tests: by_file.into_values().flatten().collect(),
        not_run,
        missing,
        unlisted,
        dir: dir.to_owned(),
    }
}

impl Results {
    pub fn green(&self) -> bool {
        self.not_run.is_empty() && self.missing.is_empty() && self.tests.iter().all(Test::passed)
    }

    /// A note per locked file whose line records no criteria: said, and never refused for.
    pub fn notes(&self) -> Vec<String> {
        self.unlisted
            .iter()
            .map(|file| {
                format!(
                    "{file}: locked before the criteria a file covers were recorded, so only a \
                     full run of it is checked: some test of it ran, and every one that ran \
                     passed. Locking it again records them"
                )
            })
            .collect()
    }

    /// [`Self::green`], and every criterion has a test that ran: coverage held when the
    /// files were locked, so a criterion with none now proves nothing (§10.5).
    pub fn green_for(&self, criteria: &[Criterion]) -> bool {
        self.green() && criteria.iter().all(|criterion| self.ran(&criterion.id) > 0)
    }

    fn of<'a>(&'a self, id: &'a str) -> impl Iterator<Item = &'a Test> + 'a {
        self.tests
            .iter()
            .filter(move |test| matches!(&test.criterion, Token::One(one) if one == id))
    }

    fn ran(&self, id: &str) -> usize {
        self.of(id).count()
    }

    /// Every problem, a line each: files first, then criteria missing from them, then tests.
    pub fn problems(&self) -> Vec<String> {
        let mut problems: Vec<String> = self
            .not_run
            .iter()
            .map(|file| self.not_run_problem(file))
            .collect();
        problems.extend(
            self.missing
                .iter()
                .map(|(file, id)| missing_problem(file, id)),
        );
        problems.extend(
            self.tests
                .iter()
                .filter(|test| !test.passed())
                .map(Test::describe),
        );
        problems
    }

    fn missing_for<'a>(&'a self, id: &'a str) -> impl Iterator<Item = &'a String> + 'a {
        self.missing
            .iter()
            .filter(move |(_, missing)| missing == id)
            .map(|(file, _)| file)
    }

    fn not_run_problem(&self, file: &str) -> String {
        format!(
            "{file}: not run by your test suite: no test case from this file is in the results. \
             Add {} to a test suite, so that the run that writes the report runs it",
            self.dir
        )
    }

    /// The output of `invar acceptance run` (§10.5): per criterion of the Plan, whether it
    /// passed and each test of it that did not; then what belongs to no criterion; then the
    /// line the protocol skill keys on.
    pub fn render(&self, ticket: &str, criteria: &[Criterion]) -> String {
        let mut out = String::new();
        for criterion in criteria {
            let ran = self.ran(&criterion.id);
            let failed: Vec<&Test> = self
                .of(&criterion.id)
                .filter(|test| !test.passed())
                .collect();
            let missing: Vec<&String> = self.missing_for(&criterion.id).collect();
            if ran > 0 && failed.is_empty() && missing.is_empty() {
                let _ = writeln!(out, "{}: passed", criterion.id);
                continue;
            }
            let _ = writeln!(out, "{}: not passed", criterion.id);
            if ran == 0 {
                let _ = writeln!(out, "  no test of {} ran", criterion.id);
            }
            for file in missing {
                let _ = writeln!(out, "  {}", missing_problem(file, &criterion.id));
            }
            for test in failed {
                let _ = writeln!(out, "  {}", test.describe());
            }
        }

        let known = |test: &&Test| {
            criteria
                .iter()
                .any(|criterion| matches!(&test.criterion, Token::One(id) if *id == criterion.id))
        };
        let others: Vec<String> = self
            .not_run
            .iter()
            .map(|file| self.not_run_problem(file))
            .chain(
                self.missing
                    .iter()
                    .filter(|(_, id)| !criteria.iter().any(|criterion| criterion.id == *id))
                    .map(|(file, id)| missing_problem(file, id)),
            )
            .chain(
                self.tests
                    .iter()
                    .filter(|test| !test.passed() && !known(test))
                    .map(Test::describe),
            )
            .collect();
        if !others.is_empty() {
            let _ = writeln!(out, "not passed, and of no criterion of the Plan:");
            for line in others {
                let _ = writeln!(out, "  {line}");
            }
        }
        let verdict = if self.green_for(criteria) {
            "green"
        } else {
            "red"
        };
        let _ = writeln!(out, "acceptance: {verdict} {ticket}");
        out
    }
}

fn missing_problem(file: &str, id: &str) -> String {
    format!("{file}: {id}: no test of this criterion from this file is in the results")
}

/// The refusal of `verify --results` and `done --results`: every problem, a line each,
/// then the notes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotGreen {
    pub problems: Vec<String>,
    pub notes: Vec<String>,
}

impl fmt::Display for NotGreen {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let count = self.problems.len();
        let noun = if count == 1 { "problem" } else { "problems" };
        write!(
            f,
            "the locked acceptance tests did not all pass ({count} {noun}):"
        )?;
        for problem in &self.problems {
            write!(f, "\n{problem}")?;
        }
        for note in &self.notes {
            write!(f, "\nnote: {note}")?;
        }
        Ok(())
    }
}

impl std::error::Error for NotGreen {}

/// What a check that passed found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verified {
    /// The distinct tests of the files checked.
    pub tests: usize,
    /// [`Results::notes`], for stderr.
    pub notes: Vec<String>,
}

/// Why the results could not be checked, or what they say when they are not green.
#[derive(Debug)]
pub enum CheckError {
    Find(FindError),
    Reports(ReportError),
    NotGreen(NotGreen),
}

impl fmt::Display for CheckError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Find(err) => err.fmt(f),
            Self::Reports(err) => err.fmt(f),
            Self::NotGreen(err) => err.fmt(f),
        }
    }
}

impl std::error::Error for CheckError {}

/// The whole check for `verify --results` and `done --results`: the locked files of
/// `ticket` (or of every ticket), against the reports.
pub fn verify(
    root: &ProjectRoot,
    dir: &str,
    ticket: Option<&str>,
    reports: &Reports,
) -> Result<Verified, CheckError> {
    let files = locked(root, dir, ticket).map_err(CheckError::Find)?;
    let cases = read_reports(root, reports).map_err(CheckError::Reports)?;
    let results = check(dir, &files, &cases);
    if results.green() {
        Ok(Verified {
            tests: results.tests.len(),
            notes: results.notes(),
        })
    } else {
        Err(CheckError::NotGreen(NotGreen {
            problems: results.problems(),
            notes: results.notes(),
        }))
    }
}

/// A report path as given on the command line, relative to `cwd`.
pub fn resolve(cwd: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_owned()
    } else {
        cwd.join(path)
    }
}
