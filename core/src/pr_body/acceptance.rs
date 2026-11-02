//! What the PR description says about acceptance tests (AT-15), read from the
//! project: the Plan's decision, the files locked for the ticket and the criteria
//! each one names, and what the developer unlocked.
//!
//! Runs no test and needs no git, so it gives the same answer in CI as in the
//! developer's clone. Everything it finds is sorted before it is returned, so the
//! same files always give the same description.

use std::path::Path;

use crate::acceptance::lock::find::{self, FindError, Scan};
use crate::acceptance::unlock;
use crate::format::config::Config;
use crate::format::junit::criterion_ids;
use crate::format::plan::{Acceptance, Plan};
use crate::fs::ProjectRoot;

/// How the ticket was decided, and what shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tests {
    /// `Acceptance: skipped`.
    Skipped,
    /// No `Acceptance` value: approved before plan-header.md 1.7.0, which counts
    /// as `skipped` (§5.1).
    NotRecorded,
    /// `Acceptance: required`, and `invar.yaml` has no `acceptance:` section:
    /// there is no directory to look in.
    RequiredWithoutConfig,
    Required(Locked),
}

/// The ticket's locked files under `acceptance.dir`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Locked {
    /// `acceptance.dir`, as configured.
    pub dir: String,
    /// Sorted by path.
    pub files: Vec<LockedFile>,
    /// `unlocked by the developer: <file> (round <R>)`, one per file, from the
    /// unlock record. Empty when there is none, as in another clone or in CI.
    pub unlocked: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockedFile {
    /// Relative to the project root, with `/` separators.
    pub path: String,
    /// The criterion ids its text names (test-results.md §4), in numeric order.
    pub criteria: Vec<String>,
}

/// What the PR description of `plan` shows about acceptance tests, and any
/// warning to give beside it: an unlock record that cannot be read is said, and
/// left out of the description rather than guessed at.
pub fn gather(
    root: &ProjectRoot,
    config: &Config,
    plan: &Plan,
    plan_dir: &Path,
) -> Result<(Tests, Vec<String>), FindError> {
    let header = plan.header();
    let acceptance = match (header.acceptance, &config.acceptance) {
        (None, _) => return Ok((Tests::NotRecorded, Vec::new())),
        (Some(Acceptance::Skipped), _) => return Ok((Tests::Skipped, Vec::new())),
        (Some(Acceptance::Required), None) => {
            return Ok((Tests::RequiredWithoutConfig, Vec::new()));
        }
        (Some(Acceptance::Required), Some(acceptance)) => acceptance,
    };

    let mut warnings = Vec::new();
    let unlocked = unlock::said(plan_dir).unwrap_or_else(|err| {
        warnings.push(format!(
            "{err}; the description says nothing about unlocking rather than guess"
        ));
        Vec::new()
    });
    let locked = Locked {
        dir: acceptance.dir.clone(),
        files: locked_files(root, &acceptance.dir, &header.ticket)?,
        unlocked,
    };
    Ok((Tests::Required(locked), warnings))
}

/// Every file under `dir` whose lock line names `ticket` and records a digest, with
/// the criteria its text names. A pending line is not a lock.
fn locked_files(root: &ProjectRoot, dir: &str, ticket: &str) -> Result<Vec<LockedFile>, FindError> {
    let Scan::Files(scanned) = find::scan(root, dir)? else {
        return Ok(Vec::new());
    };
    let mut files: Vec<LockedFile> = scanned
        .into_iter()
        .filter(|file| {
            file.line
                .as_ref()
                .is_ok_and(|line| line.ticket == ticket && line.recorded.is_some())
        })
        .map(|file| {
            let mut criteria = criterion_ids(&String::from_utf8_lossy(&file.bytes));
            criteria.sort_by_key(|id| (id.len(), id.clone()));
            LockedFile {
                path: file.path,
                criteria,
            }
        })
        .collect();
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}
