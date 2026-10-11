//! `invar plan validate --all-drafts --hook` (M1-17): what a `SubagentStop`
//! hook says when the planner is about to stop.
//!
//! The planner writes a Plan as a draft and runs `plan validate` itself. A
//! `SubagentStop` hook is the net under that: if it stops with a draft that is
//! still not valid, it is told, once, what to fix.
//!
//! # Why not simply run `plan validate`
//!
//! A hook's output reaches the agent only on one condition: it has to **block**
//! (`plugin/NOTES.md` §7 — plain stdout is read only by the debug log, and any
//! exit code but 2 is a non-blocking error nobody is shown). `plan validate`
//! prints its problems and exits 1, which a hook would deliver to nobody. So
//! this is the same check with the answer shaped for a stop hook: **block, with
//! the problems as the reason.**
//!
//! # Blocking once, never twice
//!
//! Blocking a stop makes the subagent carry on, and when it tries to stop
//! again the hook runs again with `stop_hook_active: true`. Blocking then as
//! well would trap a planner that cannot fix the problem — one that needs an
//! answer from the developer, say — in a loop until it runs out of turns. So:
//!
//! - first time, with invalid drafts: **[`Verdict::Block`]**;
//! - already blocked once and still invalid: **[`Verdict::GaveUp`]** — let it
//!   stop, and tell the developer what is left.
//!
//! # Every doubt lets it stop
//!
//! Like `plan guard`, a broken hook must not hold a session: input it cannot
//! read, a configuration it cannot load, a directory it cannot list — all are
//! [`Verdict::Unsure`], which lets the planner stop and reports why. A project
//! that is not an invar project has no Plans to speak of: [`Verdict::Pass`],
//! silently, since the plugin is installed for every project.
//!
//! Only draft Plans are looked at, as `plan validate --all-drafts` does: an
//! approved Plan is the gate's business (`plan check`), not the planner's.

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::format::config;
use crate::format::plan::Mode;
use crate::fs::{ProjectRoot, RootError};

use super::validate::{self, Report};

#[cfg(test)]
mod tests;

/// What the hook answers.
#[derive(Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Nothing to say: no invalid draft, no Plans, no project.
    Pass,
    /// Do not stop yet: `reason` is what the planner is shown.
    Block { reason: String },
    /// Invalid drafts, but the planner was already asked once. It may stop;
    /// `message` is for the developer.
    GaveUp { message: String },
    /// Could not tell. It may stop; `problem` says why.
    Unsure { problem: String },
}

/// Decide on a `SubagentStop` payload. `fallback_cwd` is used when the
/// payload carries no `cwd`.
pub fn decide(input: &[u8], fallback_cwd: &Path) -> Verdict {
    if input.iter().all(u8::is_ascii_whitespace) {
        return unsure("no hook input on stdin; expected the SubagentStop JSON payload");
    }
    let payload: Value = match serde_json::from_slice(input) {
        Ok(payload) => payload,
        Err(err) => return unsure(format!("the hook input is not valid JSON ({err})")),
    };
    // Looked up, never required: the payload belongs to Claude Code and grows
    // fields between versions.
    let already_asked = payload
        .get("stop_hook_active")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let cwd = payload
        .get("cwd")
        .and_then(Value::as_str)
        .map_or_else(|| fallback_cwd.to_owned(), PathBuf::from);

    let root = match ProjectRoot::discover_from(&cwd) {
        Ok(root) => root,
        // The plugin is installed for every project; most are not invar's.
        Err(RootError::NotFound { .. }) => return Verdict::Pass,
        Err(err) => return unsure(err.to_string()),
    };
    let mode = match config::load_for_command(&root) {
        Ok(loaded) => Mode::of(&loaded.config),
        Err(err) => return unsure(err.to_string()),
    };
    let reports = match validate::drafts(&root, mode) {
        Ok(Some(reports)) => reports,
        Ok(None) => return Verdict::Pass,
        Err(err) => return unsure(err.to_string()),
    };

    let invalid: Vec<&Report> = reports.iter().filter(|report| !report.is_valid()).collect();
    if invalid.is_empty() {
        return Verdict::Pass;
    }
    let listing = listing(&root, &invalid);
    let count = invalid.len();

    if already_asked {
        Verdict::GaveUp {
            message: format!(
                "invar: the planner stopped with {count} draft {} still not valid, after \
                 being asked once to fix {}. `invar plan validate --all-drafts` shows what \
                 is left:\n{listing}",
                if count == 1 { "Plan" } else { "Plans" },
                if count == 1 { "it" } else { "them" },
            ),
        }
    } else {
        Verdict::Block {
            reason: format!(
                "{count} draft {} not valid yet, so do not stop: fix every problem listed \
                 below, check with `invar plan validate --all-drafts`, and only then finish. \
                 A problem that is the developer's to settle — an answer you are missing — \
                 belongs in your reply, not in a Plan that stays invalid.\n{listing}",
                if count == 1 { "Plan is" } else { "Plans are" },
            ),
        }
    }
}

fn unsure(problem: impl Into<String>) -> Verdict {
    Verdict::Unsure {
        problem: problem.into(),
    }
}

/// Every problem, as `plan validate` prints it: `<path>:<line>: <what to
/// change>`, with the path relative to the project root.
fn listing(root: &ProjectRoot, invalid: &[&Report]) -> String {
    invalid
        .iter()
        .map(|report| {
            let shown = root
                .relative_path(&report.spec)
                .ok()
                .flatten()
                .unwrap_or_else(|| report.spec.clone());
            report.problems.render(&shown)
        })
        .collect::<Vec<_>>()
        .join("")
        .trim_end()
        .to_owned()
}
