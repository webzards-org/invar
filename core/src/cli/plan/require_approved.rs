//! Refusing a Plan that is not approved and unchanged since — for the commands
//! that sit outside `plan` and describe or close one (`pr-body`, `done`).
//!
//! `plan check` is the read side of the gate (M1-10): it passes exactly one
//! state, an approval that holds over the body as it is now. A command that
//! turns a Plan into something a reviewer reads, or closes its cycle, stands on
//! the same ground — a description of text nobody approved, under an
//! "Approved by …" line, would be a false statement — so it asks the same
//! question, with the same words, instead of a lighter one of its own.
//!
//! One thing `plan check` itself does that a caller outside `plan` would
//! otherwise forget: for a Plan that is not valid in form, its error says "fix
//! every problem listed above". The list is printed here, first, the way
//! `plan validate` prints it, so there is something above to point at.

use std::path::Path;

use anyhow::Result;

use crate::format::plan::Mode;
use crate::fs::ProjectRoot;
use crate::gate::check::{self, CheckError};
use crate::gate::validate::Report;

use super::validate::print_report;

/// `Ok` when the Plan at `spec` is approved and unchanged since; otherwise the
/// refusal `plan check` gives, in its own words — draft, changed after
/// approval, approved without its recorded values, or not valid.
pub fn require(root: &ProjectRoot, spec: &Path, mode: Mode, cwd: &Path) -> Result<()> {
    match check::check(root, spec, mode) {
        Ok(_) => Ok(()),
        Err(CheckError::Invalid(report)) => {
            print_problems(&report, cwd);
            Err(CheckError::Invalid(report).into())
        }
        Err(err) => Err(err.into()),
    }
}

/// The problems of a Plan that is not valid in form, as `plan validate`
/// prints them: `<path>:<line>: <what to change>`, on stdout.
pub fn print_problems(report: &Report, cwd: &Path) {
    print_report(report, cwd);
}
