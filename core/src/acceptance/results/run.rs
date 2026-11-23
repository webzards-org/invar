//! `invar acceptance run <TICKET>`: run the ticket's locked files through
//! `acceptance.run`, and say whether they pass (`acceptance-lock.md` §10.5).
//!
//! Feedback for the dispatcher, after the coder's `DONE`, and not the boundary: that is
//! `verify --results` in CI. It is the narrow exception A-2 already makes for `lock`:
//! only locked acceptance files are run, never the suite. It writes nothing.

use std::fmt;
use std::path::Path;

use crate::acceptance::lock::find::{self, FindError, Scan};
use crate::acceptance::lock::run::{self, Output, RunError, TIME_LIMIT};
use crate::format::config::{ACCEPTANCE_SECTION_EXAMPLE, AcceptanceConfig};
use crate::format::junit::{self, JunitError};
use crate::format::lock::LockError as LineError;
use crate::format::plan::{Acceptance, Mode, Plan, PlanError};
use crate::fs::ProjectRoot;
use crate::gate::check::{self, CheckError};

use super::LockedFile;

/// What a run established, and what to print.
#[derive(Debug)]
pub struct Ran {
    pub ticket: String,
    /// Every criterion has a test that ran, and every test passed.
    pub green: bool,
    /// [`Results::render`], ending with `acceptance: green|red <TICKET>`.
    pub output: String,
}

/// Run the locked files of the ticket of the Plan at `spec`, or say why not.
pub fn run(
    root: &ProjectRoot,
    spec: &Path,
    mode: Mode,
    config: Option<&AcceptanceConfig>,
) -> Result<Ran, RunLockedError> {
    let approved = check::check(root, spec, mode).map_err(RunLockedError::NotApproved)?;
    let ticket = approved.ticket.clone();
    if approved.acceptance != Acceptance::Required {
        return Err(RunLockedError::Skipped { ticket });
    }
    let config = config.ok_or(RunLockedError::NoSection)?;
    let plan = Plan::read(spec, mode).map_err(RunLockedError::Read)?;
    let files = locked_files(root, &config.dir, &ticket)?;
    let paths: Vec<String> = files.iter().map(|file| file.path.clone()).collect();

    let finished = run::run(root, &config.run, &paths, TIME_LIMIT).map_err(RunLockedError::Run)?;
    let Some(report) = finished.report else {
        return Err(RunLockedError::DidNotRun {
            why: "no report was written".to_owned(),
            status: finished.status,
            output: finished.output,
        });
    };
    let cases = match junit::read(&report, root.path()) {
        Ok(cases) => cases,
        Err(err @ JunitError::Empty) => {
            return Err(RunLockedError::DidNotRun {
                why: err.to_string(),
                status: finished.status,
                output: finished.output,
            });
        }
        Err(source) => {
            return Err(RunLockedError::Report {
                source,
                status: finished.status,
            });
        }
    };

    let results = super::check(&config.dir, &files, &cases);
    let criteria = plan.criteria();
    Ok(Ran {
        green: results.green_for(&criteria),
        output: results.render(&ticket, &criteria),
        ticket,
    })
}

/// The ticket's locked files, sorted. One of the ticket's that changed since it was
/// locked, or any malformed line, is refused: running it would say nothing about the
/// test that was locked.
fn locked_files(
    root: &ProjectRoot,
    dir: &str,
    ticket: &str,
) -> Result<Vec<LockedFile>, RunLockedError> {
    let none = || RunLockedError::NoLocked {
        ticket: ticket.to_owned(),
        dir: dir.to_owned(),
    };
    let Scan::Files(scanned) = find::scan(root, dir).map_err(RunLockedError::Find)? else {
        return Err(none());
    };
    let mut files = Vec::new();
    let mut problems = Vec::new();
    for file in scanned {
        let line = match file.line {
            Ok(line) => line,
            Err(err) => {
                problems.push((file.path, line_problem(&err)));
                continue;
            }
        };
        if line.ticket != ticket {
            continue;
        }
        match &line.recorded {
            None => {}
            Some(_) if !file.utf8_path => problems.push((
                file.path,
                "the path is not valid UTF-8, so it cannot be matched against a report".to_owned(),
            )),
            Some(_) if line.matches() => files.push(LockedFile {
                path: file.path,
                ticket: line.ticket,
            }),
            Some(recorded) => problems.push((
                file.path,
                format!(
                    "line {}: it changed after it was locked: the line records {recorded}, and \
                     the file's digest is {}. Restore it from git",
                    line.line, line.digest
                ),
            )),
        }
    }
    if !problems.is_empty() {
        return Err(RunLockedError::Files {
            ticket: ticket.to_owned(),
            problems,
        });
    }
    if files.is_empty() {
        return Err(none());
    }
    Ok(files)
}

fn line_problem(err: &LineError) -> String {
    format!("{err}; fix the `invar-lock:` line, or restore the file")
}

/// Why nothing was run, or why the run has no result.
#[derive(Debug)]
pub enum RunLockedError {
    /// What `plan check` refuses, in its words.
    NotApproved(CheckError),
    Skipped {
        ticket: String,
    },
    NoSection,
    Read(PlanError),
    Find(FindError),
    /// No file locked for the ticket.
    NoLocked {
        ticket: String,
        dir: String,
    },
    /// Malformed lines, or the ticket's files changed after locking.
    Files {
        ticket: String,
        problems: Vec<(String, String)>,
    },
    Run(RunError),
    DidNotRun {
        why: String,
        status: String,
        output: Output,
    },
    Report {
        source: JunitError,
        status: String,
    },
}

impl fmt::Display for RunLockedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotApproved(err) => err.fmt(f),
            Self::Skipped { ticket } => write!(
                f,
                "{ticket}: this ticket was approved without acceptance tests, so it has no \
                 locked tests to run"
            ),
            Self::NoSection => write!(
                f,
                "invar.yaml has no `acceptance:` section, so there is no directory or command for \
                 acceptance tests. Add one, with your own directory and test command:\n\n\
                 {ACCEPTANCE_SECTION_EXAMPLE}"
            ),
            Self::Read(err) => err.fmt(f),
            Self::Find(err) => err.fmt(f),
            Self::NoLocked { ticket, dir } => write!(
                f,
                "{ticket}: no file under {dir} is locked for it, so there is nothing to run. The \
                 test-writer writes the tests and runs `invar acceptance lock {ticket}`"
            ),
            Self::Files { ticket, problems } => {
                write!(f, "{ticket}: nothing was run:")?;
                for (file, problem) in problems {
                    write!(f, "\n{file}: {problem}")?;
                }
                Ok(())
            }
            Self::Run(err) => match err {
                // Its own words say "nothing was locked".
                RunError::TimedOut { limit, output } => write!(
                    f,
                    "the tests did not finish within {} minutes, so the run was killed; look \
                     for a test that hangs{output}",
                    limit.as_secs() / 60
                ),
                _ => err.fmt(f),
            },
            Self::DidNotRun {
                why,
                status,
                output,
            } => write!(
                f,
                "the tests did not run: {why}. The command ended with {status}.{output}"
            ),
            Self::Report { source, status } => write!(
                f,
                "cannot read the report. The command ended with {status}: {source}"
            ),
        }
    }
}

impl std::error::Error for RunLockedError {}
