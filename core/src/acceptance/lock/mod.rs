//! `invar acceptance lock` (AT-8): turn the test-writer's pending files into locked
//! ones, and only when every test checks something.
//!
//! Implements `contracts/formats/acceptance-lock.md` §9. The contract is the
//! authority; section numbers and `D-n` below point into it.
//!
//! [`find`] says which files belong to the ticket, [`run`] runs them, and [`check`]
//! decides on the report.

use std::fmt::{self, Write as _};
use std::path::Path;

use crate::format::config::AcceptanceConfig;
use crate::format::junit::{self, JunitError};
use crate::format::plan::Mode;
use crate::fs::ProjectRoot;

use super::spec::{self, SpecError};
use check::{Coverage, Problems};
use find::{FindError, Found};
use run::{Output, RunError, TIME_LIMIT};

pub mod check;
pub mod find;
pub mod run;

#[cfg(test)]
mod tests;

/// What a run that passed every check established.
#[derive(Debug)]
pub struct Checked {
    pub ticket: String,
    pub coverage: Coverage,
    pub files: Vec<String>,
}

impl Checked {
    /// The output on success (§9.5).
    pub fn render(&self) -> String {
        let mut out = String::new();
        for (id, tests) in &self.coverage {
            let _ = writeln!(out, "{id}:");
            for test in tests {
                let _ = writeln!(out, "  {}: {}", test.file, test.name);
            }
        }
        let _ = writeln!(out);
        let _ = writeln!(out, "checked {} {} file(s)", self.ticket, self.files.len());
        out
    }
}

/// Run the checks of §9 on the ticket of the Plan at `spec`.
pub fn lock(
    root: &ProjectRoot,
    spec: &Path,
    mode: Mode,
    config: Option<&AcceptanceConfig>,
) -> Result<Checked, LockError> {
    let (plan, config) = spec::usable(root, spec, mode, config).map_err(LockError::Plan)?;
    let ticket = plan.header().ticket.clone();
    let found: Found = find::find(root, &config.dir, &ticket).map_err(LockError::Find)?;

    let finished =
        run::run(root, &config.run, &found.all_paths(), TIME_LIMIT).map_err(LockError::Run)?;
    let Some(report) = finished.report else {
        return Err(LockError::DidNotRun {
            why: "no report was written".to_owned(),
            status: finished.status,
            output: finished.output,
        });
    };
    let cases = match junit::read(&report, root.path()) {
        Ok(cases) => cases,
        Err(err @ JunitError::Empty) => {
            return Err(LockError::DidNotRun {
                why: err.to_string(),
                status: finished.status,
                output: finished.output,
            });
        }
        Err(source) => {
            return Err(LockError::Report {
                source,
                status: finished.status,
            });
        }
    };

    let coverage =
        check::check(&ticket, &plan.criteria(), &found, &cases).map_err(LockError::Checks)?;
    Ok(Checked {
        ticket,
        coverage,
        files: found.to_lock.into_iter().map(|file| file.path).collect(),
    })
}

/// Why nothing was locked.
#[derive(Debug)]
pub enum LockError {
    /// What `invar acceptance spec` refuses, in its words.
    Plan(SpecError),
    Find(FindError),
    Run(RunError),
    /// No report, or an empty one: nothing is known about any test (§9.2).
    DidNotRun {
        why: String,
        status: String,
        output: Output,
    },
    Report {
        source: JunitError,
        status: String,
    },
    Checks(Problems),
}

impl fmt::Display for LockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Plan(err) => err.fmt(f),
            Self::Find(err) => err.fmt(f),
            Self::Run(err) => err.fmt(f),
            Self::DidNotRun {
                why,
                status,
                output,
            } => write!(
                f,
                "the tests did not run: {why}. The command ended with {status}, and nothing was                  locked.{output}"
            ),
            Self::Report { source, status } => write!(
                f,
                "cannot read the report, and nothing was locked. The command ended with                  {status}: {source}"
            ),
            Self::Checks(problems) => problems.fmt(f),
        }
    }
}

impl std::error::Error for LockError {}
