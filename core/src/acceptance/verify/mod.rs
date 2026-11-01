//! Whether the locks under `acceptance.dir` hold (`acceptance-lock.md` §10).
//!
//! One check, used by `plan check`, `invar done` and `invar acceptance verify`. It
//! runs no test (A-2). It reports every problem in one run (D-14), because a caller
//! that stopped at the first would cost a round trip per problem.

use std::fmt;

use crate::format::config::{AcceptanceConfig, Config};
use crate::format::lock::LockLine;
use crate::format::plan::{Acceptance, Mode, Plan};
use crate::fs::ProjectRoot;
use crate::gate::check::Approved;

use super::lock::find::{self, Scan, Scanned};

pub mod branch;

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

/// How far along the cycle is, which decides whether a pending line may be there (D-18).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// `plan check`: the test-writer may not have locked anything yet.
    Working,
    /// `done`, and `verify` with a ticket.
    Closing,
}

/// The check for a ticket whose Plan holds: §10.1, §10.2 and, when closing, §10.3.
pub fn for_ticket(
    root: &ProjectRoot,
    config: &Config,
    approved: &Approved,
    stage: Stage,
) -> Report {
    let mut report = Report::default();
    let Some(acceptance) = &config.acceptance else {
        // No directory to look in (D-21). Said once, when the cycle closes, and not for
        // a ticket that never wanted tests.
        if stage == Stage::Closing && approved.acceptance == Acceptance::Required {
            report.warnings.push(format!(
                "{}: the Plan says `Acceptance: required`, and invar.yaml has no `acceptance:` \
                 section, so there is nothing to verify",
                approved.ticket
            ));
        }
        return report;
    };

    let files = integrity(root, &acceptance.dir, &mut report);

    match Plan::read(&approved.spec, Mode::of(config)) {
        Ok(plan) => branch::check(
            root,
            &acceptance.dir,
            &approved.ticket,
            &plan.tests_to_change(),
            config.git.base_branch.as_deref(),
            &mut report,
        ),
        Err(err) => report.problems.push(err.to_string()),
    }

    if stage == Stage::Closing {
        closing(&files, approved, &acceptance.dir, &mut report);
    }
    report
}

/// §10.3.
fn closing(files: &[Scanned], approved: &Approved, dir: &str, report: &mut Report) {
    let ticket = &approved.ticket;
    let lines = || {
        files.iter().filter_map(|file| {
            let line = file.line.as_ref().ok()?;
            (line.ticket == *ticket).then_some((&file.path, line))
        })
    };
    if approved.acceptance == Acceptance::Required
        && !lines().any(|(_, line)| line.recorded.is_some())
    {
        report.problems.push(format!(
            "{ticket}: the Plan says `Acceptance: required`, and no file under {dir} is locked \
             for it. The test-writer writes the tests and runs `invar acceptance lock {ticket}`"
        ));
    }
    for (path, line) in lines().filter(|(_, line)| line.recorded.is_none()) {
        report.problems.push(pending_problem(path, line));
    }
}

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
