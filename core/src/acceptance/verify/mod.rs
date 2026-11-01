//! Whether the locks under `acceptance.dir` hold (`acceptance-lock.md` §10).
//!
//! One check, used by `plan check`, `invar done` and `invar acceptance verify`. It
//! runs no test (A-2). It reports every problem in one run (D-14), because a caller
//! that stopped at the first would cost a round trip per problem.

use std::fmt;

use crate::format::config::AcceptanceConfig;
use crate::format::lock::LockLine;
use crate::fs::ProjectRoot;

use super::lock::find::{self, Scan, Scanned};

#[cfg(test)]
mod tests;

/// What a check found. A refusal when `problems` is not empty.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Report {
    pub problems: Vec<String>,
    /// Said, and never refused for.
    pub warnings: Vec<String>,
    /// Locked files whose digest holds, for any ticket.
    pub locked: usize,
}

impl Report {
    pub fn holds(&self) -> bool {
        self.problems.is_empty()
    }
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let count = self.problems.len();
        let noun = if count == 1 { "problem" } else { "problems" };
        write!(f, "the acceptance locks do not hold ({count} {noun}):")?;
        for problem in &self.problems {
            write!(f, "\n{problem}")?;
        }
        for warning in &self.warnings {
            write!(f, "\nwarning: {warning}")?;
        }
        Ok(())
    }
}

impl std::error::Error for Report {}

/// What `invar acceptance verify` with no ticket checks: §10.1, and a pending line
/// anywhere is a problem (D-17, D-18). Needs no Plan, and never runs git.
pub fn whole_directory(root: &ProjectRoot, config: &AcceptanceConfig) -> Report {
    let mut report = Report::default();
    for file in integrity(root, &config.dir, &mut report) {
        if let Ok(line) = &file.line
            && line.recorded.is_none()
        {
            report.problems.push(pending_problem(&file.path, line));
        }
    }
    report
}

/// §10.1. Returns the files, which the other parts look at again.
fn integrity(root: &ProjectRoot, dir: &str, report: &mut Report) -> Vec<Scanned> {
    let files = match find::scan(root, dir) {
        Ok(Scan::Files(files)) => files,
        Ok(Scan::Missing) => Vec::new(),
        Ok(Scan::NotADirectory) => {
            report.problems.push(format!(
                "{dir}: `acceptance.dir` is not a directory, or is a symlink, so no lock under it \
                 can be checked; make it a directory, or fix `acceptance.dir` in invar.yaml"
            ));
            Vec::new()
        }
        Err(err) => {
            report.problems.push(err.to_string());
            Vec::new()
        }
    };
    for file in &files {
        match &file.line {
            Err(err) => report.problems.push(format!(
                "{}: {err}; fix the `invar-lock:` line, or restore the file",
                file.path
            )),
            Ok(line) => match &line.recorded {
                None => {}
                Some(_) if line.matches() => report.locked += 1,
                Some(recorded) => report.problems.push(format!(
                    "{}: locked for {}, and it changed after it was locked: the line records \
                     {recorded}, and the file's digest is {}. Restore the file from git; a locked \
                     test is changed only when a Plan declares it in `## Acceptance tests to \
                     change`, and then it is locked again",
                    file.path, line.ticket, line.digest
                )),
            },
        }
    }
    files
}

fn pending_problem(path: &str, line: &LockLine) -> String {
    format!(
        "{path}: a pending line for {ticket}, so nobody proved its tests red. Run `invar \
         acceptance lock {ticket}`, or remove the file",
        ticket = line.ticket
    )
}
