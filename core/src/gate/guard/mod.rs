//! `invar plan guard` (M1-10): keep the agent's `Write` and `Edit` off an
//! approved Plan.
//!
//! Run by a `PreToolUse` hook (M1-17), it reads the hook's JSON payload —
//! here, in the CLI, so the plugin stays a thin shell (§13b) and the guard can
//! be tested by piping JSON into it — and decides on `tool_input.file_path`.
//!
//! **Convenience, not the gate.** A `Write|Edit` matcher never sees a file
//! changed through `Bash` (`plugin/NOTES.md` §8), so the guard can only save
//! the agent a wasted edit. The boundary is `Approved-checksum`, which
//! `plan check` and `done` verify whatever tool changed the file.
//!
//! **A broken guard must not halt a session.** Anything it cannot make sense
//! of — input that is not JSON, a payload without a path, an unreadable
//! file — allows the edit and says why. Only one thing denies: the path is an
//! existing `.invar/plans/<name>/spec.md` whose header says
//! `Status: approved`. A draft Plan is edited freely; that is Phase 1 working.

use std::io;
use std::path::{Component, Path, PathBuf};

use serde_json::Value;

use crate::format::plan::{Mode, Plan, Status};
use crate::fs::plans::SPEC_FILE;
use crate::fs::{ProjectRoot, RootError};

#[cfg(test)]
mod tests;

/// What the guard tells the hook.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Not an approved Plan: the tool call takes its normal course, including
    /// any permission prompt. The guard never answers "allow" outright.
    Pass,
    /// An approved Plan: the tool call is denied, and `reason` tells the
    /// agent why and what to do instead.
    Deny { reason: String },
    /// The guard could not decide. The tool call takes its normal course and
    /// `problem` is reported to the developer.
    Unsure { problem: String },
}

/// Decide on a `PreToolUse` payload. `fallback_cwd` resolves a relative
/// `file_path` when the payload carries no `cwd`.
pub fn decide(input: &[u8], fallback_cwd: &Path) -> Verdict {
    match target(input, fallback_cwd) {
        Ok(path) => judge(&path),
        Err(problem) => Verdict::Unsure { problem },
    }
}

/// The absolute path the tool call would write.
fn target(input: &[u8], fallback_cwd: &Path) -> Result<PathBuf, String> {
    if input.iter().all(u8::is_ascii_whitespace) {
        return Err("no hook input on stdin; expected the PreToolUse JSON payload".into());
    }
    let payload: Value = serde_json::from_slice(input)
        .map_err(|err| format!("the hook input is not valid JSON ({err})"))?;
    let tool = payload
        .get("tool_name")
        .and_then(Value::as_str)
        .unwrap_or("an unnamed tool");
    let file_path = payload
        .get("tool_input")
        .and_then(|input| input.get("file_path"))
        .and_then(Value::as_str)
        .filter(|path| !path.is_empty())
        .ok_or_else(|| {
            format!(
                "the hook input for {tool} has no `tool_input.file_path`; the guard \
                 understands `Write` and `Edit` — check the hook's matcher"
            )
        })?;

    let path = Path::new(file_path);
    if path.is_absolute() {
        return Ok(path.to_owned());
    }
    let cwd = payload
        .get("cwd")
        .and_then(Value::as_str)
        .map_or_else(|| fallback_cwd.to_owned(), PathBuf::from);
    Ok(cwd.join(path))
}

fn judge(path: &Path) -> Verdict {
    // The file may not exist yet (`Write` creates it); its project is found
    // from the nearest directory that does.
    let Some(start) = path.ancestors().find(|dir| dir.exists()) else {
        return Verdict::Pass;
    };
    let root = match ProjectRoot::discover_from(start) {
        Ok(root) => root,
        // Not in an invar project: not a Plan.
        Err(RootError::NotFound { .. }) => return Verdict::Pass,
        Err(err) => {
            return Verdict::Unsure {
                problem: format!("cannot tell whether {} is a Plan: {err}", path.display()),
            };
        }
    };
    let relative = match root.relative_path(path) {
        Ok(Some(relative)) => relative,
        Ok(None) => return Verdict::Pass,
        Err(err) => {
            return Verdict::Unsure {
                problem: format!("cannot resolve {}: {err}", path.display()),
            };
        }
    };
    let Some(name) = plan_dir_name(&relative) else {
        return Verdict::Pass;
    };

    let text = match std::fs::read(path) {
        Ok(bytes) => match String::from_utf8(bytes) {
            Ok(text) => text,
            Err(_) => {
                return Verdict::Unsure {
                    problem: format!(
                        "{} is not valid UTF-8, so whether it is approved cannot be read",
                        relative.display()
                    ),
                };
            }
        },
        // A Plan being written for the first time.
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Verdict::Pass,
        Err(err) => {
            return Verdict::Unsure {
                problem: format!("cannot read {}: {err}", relative.display()),
            };
        }
    };

    // `Status` reads the same in either mode; the sections, which differ by
    // mode, play no part here.
    let checked = Plan::check(&text, Mode::Light);
    if checked.status != Some(Status::Approved) {
        return Verdict::Pass;
    }
    let ticket = checked
        .ticket
        .map(|(ticket, _)| ticket)
        .unwrap_or_else(|| name.clone());
    Verdict::Deny {
        reason: format!(
            "{} is the approved Plan for {ticket}. Its text is what the developer \
             approved, and `Approved-checksum` holds it fixed, so it is not edited \
             during implementation — through any tool. If the Plan is wrong or \
             incomplete, stop and tell the developer what should change; they edit it \
             and approve it again with `invar plan approve {ticket} --reapprove`",
            relative.display()
        ),
    }
}

/// `<name>` when `relative` is `.invar/plans/<name>/spec.md`.
fn plan_dir_name(relative: &Path) -> Option<String> {
    let parts: Vec<&str> = relative
        .components()
        .map(|part| match part {
            Component::Normal(name) => name.to_str(),
            _ => None,
        })
        .collect::<Option<_>>()?;
    match parts.as_slice() {
        [".invar", "plans", name, SPEC_FILE] => Some((*name).to_owned()),
        _ => None,
    }
}
