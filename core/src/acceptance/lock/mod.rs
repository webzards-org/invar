//! `invar acceptance lock` (AT-8): turn the test-writer's pending files into locked
//! ones, and only when every test checks something.
//!
//! Implements `contracts/formats/acceptance-lock.md` §9. The contract is the
//! authority; section numbers and `D-n` below point into it.
//!
//! [`find`] says which files belong to the ticket, [`run`] runs them, [`check`]
//! decides on the report, and [`write`] writes the digests.

use std::fmt::{self, Write as _};
use std::path::Path;

use crate::format::config::AcceptanceConfig;
use crate::format::junit::{self, JunitError};
use crate::format::plan::Mode;
use crate::fs::ProjectRoot;

use super::spec::{self, SpecError};
use super::unlock::{self, ReadError};
use check::{Coverage, Problems};
use find::{FindError, Found};
use run::{Output, RunError, TIME_LIMIT};
use write::{WriteError, Written};

pub mod check;
pub mod find;
pub mod run;
pub mod write;

#[cfg(test)]
mod tests;

/// What a run that locked its files established.
#[derive(Debug)]
pub struct Locked {
    pub ticket: String,
    pub coverage: Coverage,
    pub files: Vec<Written>,
    /// The files that skipped the red check, each with its round (§11.3).
    pub skipped_red: Vec<(String, u32)>,
    /// Said on stderr, and refused for nothing.
    pub warnings: Vec<String>,
}

impl Locked {
    /// The output on success (§9.5). The last line is what the test-writer keys on.
    pub fn render(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "Coverage:");
        for (id, tests) in &self.coverage {
            let _ = writeln!(out, "{id}:");
            for test in tests {
                let _ = writeln!(out, "  {}: {}", test.file, test.name);
            }
        }
        let _ = writeln!(out);
        let _ = writeln!(out, "Locked:");
        for file in &self.files {
            let _ = writeln!(out, "  {} {}", file.path, file.digest);
        }
        let _ = writeln!(out);
        for (file, round) in &self.skipped_red {
            let _ = writeln!(
                out,
                "red check skipped for {file} (unlocked by the developer, round {round})"
            );
        }
        let _ = writeln!(out, "locked {} {} file(s)", self.ticket, self.files.len());
        out
    }
}

/// Lock the ticket of the Plan at `spec`, or say why not (§9).
pub fn lock(
    root: &ProjectRoot,
    spec: &Path,
    mode: Mode,
    config: Option<&AcceptanceConfig>,
) -> Result<Locked, LockError> {
    let (plan, config) = spec::usable(root, spec, mode, config).map_err(LockError::Plan)?;
    let ticket = plan.header().ticket.clone();
    let plan_dir = spec.parent().unwrap_or(root.path());
    let mut record = unlock::read_record(plan_dir).map_err(LockError::Record)?;
    let found: Found = find::find(root, &config.dir, &ticket).map_err(LockError::Find)?;

    let skipped_red: Vec<(String, u32)> = found
        .to_lock
        .iter()
        .filter_map(|file| Some((file.path.clone(), record.open_round(&file.path)?)))
        .collect();
    let mut warnings: Vec<String> = record
        .open
        .iter()
        .filter(|entry| !found.to_lock.iter().any(|file| file.path == entry.path))
        .map(|entry| {
            format!(
                "{}: in the unlock record, and it is not pending, so the record says nothing \
                 about it",
                entry.path
            )
        })
        .collect();
    let unlocked: Vec<&str> = skipped_red.iter().map(|(path, _)| path.as_str()).collect();

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

    let coverage = check::check(&ticket, &plan.criteria(), &found, &unlocked, &cases)
        .map_err(LockError::Checks)?;
    let files = write::write(root, &ticket, &found.to_lock).map_err(LockError::Write)?;

    // The files are locked by now, so a record left behind is said and not refused for: an
    // open entry for a locked file grants nothing (§11.3).
    if !record.open.is_empty() {
        record.close_all();
        if let Err(err) = unlock::write_record(plan_dir, &record) {
            warnings.push(format!("cannot update the unlock record: {err}"));
        }
    }
    Ok(Locked {
        ticket,
        coverage,
        files,
        skipped_red,
        warnings,
    })
}

/// Why nothing was locked.
#[derive(Debug)]
pub enum LockError {
    /// What `invar acceptance spec` refuses, in its words.
    Plan(SpecError),
    Record(ReadError),
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
    Write(WriteError),
}

impl fmt::Display for LockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Plan(err) => err.fmt(f),
            Self::Record(err) => write!(f, "{err}; nothing was run or locked"),
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
            Self::Write(err) => err.fmt(f),
        }
    }
}

impl std::error::Error for LockError {}
