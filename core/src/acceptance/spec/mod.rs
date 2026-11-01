//! `invar acceptance spec` (AT-7): everything the test-writer needs to write a
//! ticket's acceptance tests, assembled the same way every time.
//!
//! The output is, in this order:
//!
//! 1. the **ticket and style**;
//! 2. **where and how**: `acceptance.dir`, the `run` command as configured, and
//!    `fakes_allowed`;
//! 3. the **conventions** of the style, one fixed text per style. A new style is a
//!    new text and one more arm in [`conventions`], with no branches inside a text;
//! 4. the approved Plan's **criteria**, each with its id and full text;
//! 5. its **Scope** and **Implementation context**;
//! 6. its **Acceptance tests to change**, when it has any, and what to do with them;
//! 7. the **unlocked** files, when there are any, and what to do with them
//!    (`acceptance-lock.md` §11.4);
//! 8. the **next step**.
//!
//! What it never contains: `BUSINESS_CONTEXT.md` beyond what the Plan quotes, the
//! decision log, the pasted ticket, the questions or the rejected alternatives. The
//! test-writer writes from what was approved, not from how it came about. So nothing
//! here reads any of them.
//!
//! **Nothing is cut for size.** The command runs through Bash, so the cap on a hook's
//! output does not apply, and cutting Implementation context to fit would hide
//! approved text from the agent that must test it.

use std::fmt::{self, Write as _};
use std::fs;
use std::path::Path;

use crate::format::config::{ACCEPTANCE_SECTION_EXAMPLE, AcceptanceConfig, AcceptanceStyle};
use crate::format::lock;
use crate::format::plan::{Acceptance, Mode, Plan, PlanError};
use crate::format::unlock::{Entry, MAX_ROUNDS};
use crate::fs::ProjectRoot;
use crate::gate::check::{self, CheckError};
use crate::review_input::section;

use super::unlock::{self, ReadError};

#[cfg(test)]
mod tests;

/// The conventions of `integration`. `<TICKET>` is replaced by the ticket.
const INTEGRATION: &str = "\
- One ticket per file; a ticket may have several files. Every file lives under the \
directory above.
- Each file has the pending lock line `invar-lock: <TICKET>` within its first 10 lines, \
behind the file's own comment prefix. Never write a digest: `invar acceptance lock` writes it.
- Every test's name contains its criterion's id as a whole token (`AC3`), exactly one id per \
test. Each criterion has at least one test.
- The criterion's text is in the test's docblock or comment, word for word.
- Use the real database and the application's own scopes, policies and middleware. Fake only \
what `fakes_allowed` lists.
- A negative criterion (refused, 403, 404, not found, not changed) gets a control probe in the \
same test: the same request that must succeed for an allowed case, so that the test fails for \
the right reason before the code exists.
- Every test must fail on an assertion before the code exists. A test that passes, or that \
fails on an error (a missing class), is refused by `invar acceptance lock`.
- Write only test files, and helpers under the directory above. Never application code, the \
Plan, or a file with a locked `invar-lock:` line.
";

/// The fixed text of a style.
fn conventions(style: AcceptanceStyle) -> &'static str {
    match style {
        AcceptanceStyle::Integration => INTEGRATION,
    }
}

/// The output for the Plan at `spec`, or why there is none. `config` is the
/// `acceptance` section of `invar.yaml`, if it has one.
pub fn build(
    root: &ProjectRoot,
    spec: &Path,
    mode: Mode,
    config: Option<&AcceptanceConfig>,
) -> Result<String, SpecError> {
    let (plan, config) = usable(root, spec, mode, config)?;
    let plan_dir = spec.parent().unwrap_or(root.path());
    let record = unlock::read_record(plan_dir).map_err(SpecError::Record)?;
    // An open entry whose file is not pending is `lock`'s to warn about (§11.3). There is
    // nothing here for the test-writer to fix.
    let unlocked: Vec<Entry> = record
        .open
        .into_iter()
        .filter(|entry| is_pending(&root.path().join(&entry.path)))
        .collect();
    Ok(render(&plan, config, &unlocked))
}

fn is_pending(path: &Path) -> bool {
    fs::read(path)
        .ok()
        .and_then(|bytes| lock::read(&bytes).ok().flatten())
        .is_some_and(|line| line.recorded.is_none())
}

/// The Plan and the section, if tests may be written or locked for this ticket: the
/// Plan is approved and unchanged since, `required`, the section exists, and the
/// criteria have ids. In that order, and the first that fails is the answer.
pub fn usable<'a>(
    root: &ProjectRoot,
    spec: &Path,
    mode: Mode,
    config: Option<&'a AcceptanceConfig>,
) -> Result<(Plan, &'a AcceptanceConfig), SpecError> {
    let approved = check::check(root, spec, mode).map_err(SpecError::NotApproved)?;
    if approved.acceptance != Acceptance::Required {
        return Err(SpecError::Skipped {
            ticket: approved.ticket,
        });
    }
    let config = config.ok_or(SpecError::NoSection)?;
    let plan = Plan::read(spec, mode).map_err(SpecError::Read)?;
    if plan.criteria().is_empty() {
        return Err(SpecError::NoIds {
            ticket: approved.ticket,
        });
    }
    Ok((plan, config))
}

/// The output itself. A pure function of its arguments: nothing here reads the clock,
/// the environment or the file system.
///
/// The caller decides that the Plan may be used: approved, unchanged since, `required`
/// and with ids, as [`build`] does. `unlocked` is the files of the unlock record that
/// are waiting to be fixed.
pub fn render(plan: &Plan, config: &AcceptanceConfig, unlocked: &[Entry]) -> String {
    let ticket = &plan.header().ticket;
    let mut out = String::new();

    let _ = writeln!(out, "=== invar acceptance spec: {ticket} ===");
    let _ = writeln!(out);
    let _ = writeln!(out, "Ticket: {ticket}");
    let _ = writeln!(out, "Acceptance: required");
    let _ = writeln!(out, "style: {}", config.style.as_str());

    heading(&mut out, "Where and how");
    let _ = writeln!(out, "dir: {}", config.dir);
    let _ = writeln!(out, "run: {}", run_as_configured(&config.run));
    let _ = writeln!(
        out,
        "  You do not run this: `invar acceptance lock` does, and fills in `{{report}}` and \
         `{{files}}`."
    );
    if config.fakes_allowed.is_empty() {
        let _ = writeln!(out, "fakes_allowed: none: nothing may be faked");
    } else {
        let _ = writeln!(out, "fakes_allowed: {}", config.fakes_allowed.join(", "));
    }

    heading(&mut out, "Conventions");
    out.push_str(&conventions(config.style).replace("<TICKET>", ticket));

    heading(&mut out, "Criteria");
    for criterion in plan.criteria() {
        let _ = writeln!(out, "{}: {}", criterion.id, criterion.text);
    }

    for name in ["Scope", "Implementation context"] {
        heading(&mut out, name);
        out.push_str(&section(plan, name));
        out.push('\n');
    }

    if !plan.tests_to_change().is_empty() {
        heading(&mut out, "Acceptance tests to change");
        out.push_str(&section(plan, "Acceptance tests to change"));
        out.push('\n');
        let _ = writeln!(
            out,
            "\nFor a `delete` item, remove the file, and test what this ticket's criteria need in \
             a file of its own. For a `change` item, stop and answer `BLOCKED`, naming the file: \
             a file locked for another ticket cannot be made this ticket's yet."
        );
    }

    if !unlocked.is_empty() {
        heading(&mut out, "Unlocked tests");
        let _ = writeln!(
            out,
            "The developer unlocked these files, because a locked test was believed wrong:\n"
        );
        for entry in unlocked {
            let _ = writeln!(
                out,
                "- {} (round {} of {MAX_ROUNDS})",
                entry.path, entry.round
            );
        }
        let _ = writeln!(
            out,
            "\nFix exactly these files to match their criteria, word for word, and change no \
             other file. Do not weaken them to pass the code: they are checked against the \
             criteria, not against the code. The code may or may not exist yet, so a test in \
             these files may pass or fail on an assertion; an error or a skipped test is still \
             refused."
        );
    }

    heading(&mut out, "Next step");
    let others = if unlocked.is_empty() {
        "every test fails"
    } else {
        "every test outside the unlocked files fails"
    };
    let _ = writeln!(
        out,
        "Run `invar acceptance lock {ticket}`. It checks coverage, that every criterion has a \
         test, and red before green, that {others} on an assertion."
    );
    out
}

fn heading(out: &mut String, name: &str) {
    let _ = writeln!(out);
    let _ = writeln!(out, "=== {name} ===");
    let _ = writeln!(out);
}

/// `["a", "b"]`, quoted as YAML and JSON both read it, because each element is one
/// argument and that has to stay visible.
fn run_as_configured(run: &[String]) -> String {
    let elements: Vec<String> = run
        .iter()
        .map(|element| serde_json::Value::from(element.as_str()).to_string())
        .collect();
    format!("[{}]", elements.join(", "))
}

/// Why there is nothing to print.
#[derive(Debug)]
pub enum SpecError {
    /// What `plan check` refuses, in its words.
    NotApproved(CheckError),
    /// `Acceptance: skipped`, or no value.
    Skipped {
        ticket: String,
    },
    /// `invar.yaml` has no `acceptance:` section.
    NoSection,
    /// An approval from before criteria had ids (plan-header.md D-16).
    NoIds {
        ticket: String,
    },
    Read(PlanError),
    Record(ReadError),
}

impl fmt::Display for SpecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotApproved(err) => err.fmt(f),
            Self::Skipped { ticket } => write!(
                f,
                "{ticket}: this ticket was approved without acceptance tests; there is nothing \
                 for a test-writer to do. To add them, the developer approves it again with \
                 `--reapprove --acceptance`"
            ),
            Self::NoSection => write!(
                f,
                "invar.yaml has no `acceptance:` section, so there is no directory or command for \
                 acceptance tests. Add one, with your own directory and test command:\n\n\
                 {ACCEPTANCE_SECTION_EXAMPLE}"
            ),
            Self::NoIds { ticket } => write!(
                f,
                "{ticket}: the acceptance criteria have no ids, because this approval is from \
                 before ids existed, so no test can name its criterion (plan-header.md D-16). \
                 Give every criterion its id, as in `- AC1: Given …`, and the developer approves \
                 it again with `--reapprove`"
            ),
            Self::Read(err) => err.fmt(f),
            Self::Record(err) => err.fmt(f),
        }
    }
}

impl std::error::Error for SpecError {}
