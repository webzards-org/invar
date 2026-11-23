//! `--results <REPORT>... [--report-root <DIR>]`, shared by `invar acceptance verify` and
//! `invar done` (`acceptance-lock.md` §10.5).

use std::path::{Path, PathBuf};

use anyhow::{Result, bail};

use crate::acceptance::results::{self, Reports};

#[derive(Debug, Clone, Default, clap::Args)]
pub struct ResultsArgs {
    /// JUnit XML reports from your own test run, such as `junit.xml` from
    /// `vendor/bin/pest --log-junit junit.xml`: every locked acceptance file must have a
    /// test case in them, and every one of them must have passed.
    ///
    /// One or more files; a ticket, if any, comes before this flag. A `skipped` test has
    /// not passed, and neither has one that passed in one report and not in another.
    /// Without it, no results are read.
    #[arg(long = "results", value_name = "REPORT", num_args = 1..)]
    pub results: Vec<PathBuf>,

    /// The absolute path at which the tests saw the project root, when it is not this
    /// one: `/var/www/html` under Laravel Sail. A test case's `file` under it is read as
    /// under the project root.
    #[arg(long, value_name = "DIR", requires = "results")]
    pub report_root: Option<PathBuf>,
}

impl ResultsArgs {
    /// The reports to read, relative to `cwd`, or `None` without `--results`.
    pub fn reports(&self, cwd: &Path) -> Result<Option<Reports>> {
        if self.results.is_empty() {
            return Ok(None);
        }
        if let Some(dir) = &self.report_root
            && !dir.is_absolute()
        {
            bail!(
                "`--report-root {}` is not an absolute path; give the project root as the \
                 tests saw it, such as `/var/www/html`",
                dir.display()
            );
        }
        Ok(Some(Reports {
            paths: self
                .results
                .iter()
                .map(|path| results::resolve(cwd, path))
                .collect(),
            report_root: self.report_root.clone(),
        }))
    }
}
