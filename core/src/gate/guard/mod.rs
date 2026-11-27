//! `invar plan guard` (M1-10): keep the agent's `Write` and `Edit` off an
//! approved Plan and off a locked acceptance test, and keep the agent from running
//! the commands that record a person's decision.
//!
//! Two kinds of job, one hook, told apart by `tool_name`: a `Write` or `Edit` is judged
//! by the file it targets; a `Bash` call is judged by the command line it runs
//! (`HUMAN_ONLY`, at the end of this file). Both answer with the same
//! [`Verdict`]. The second job exists because `plan approve`, `decision
//! approve` and `decision record` are a person's decisions, and nothing in the
//! command itself can tell a person from an agent that has a shell: the
//! developer runs them (from another terminal, or through a skill the model
//! cannot invoke), and the agent is told, at the moment it tries, that this
//! step is not its to take.
//!
//! Run by a `PreToolUse` hook (M1-17), it reads the hook's JSON payload —
//! here, in the CLI, so the plugin stays a thin shell (§13b) and the guard can
//! be tested by piping JSON into it — and decides on `tool_input.file_path`.
//!
//! **Convenience, not the gate.** A `Write|Edit` matcher never sees a file
//! changed through `Bash` (`plugin/NOTES.md` §8), so the guard can only save
//! the agent a wasted edit. The boundary is `Approved-checksum`, which
//! `plan check` and `done` verify whatever tool changed the file. The guard on
//! locked tests is no different, and its boundary is the digest those two and
//! `acceptance verify` check: `sed -i` gets past it, and is caught there. The command
//! guard is the same kind of thing: a shell can spell a command in ways no
//! scan recognises, and a hook that fails lets the call through, so it makes
//! an honest agent stop, and does not stop one that is determined.
//!
//! **A broken guard must not halt a session.** Anything it cannot make sense
//! of — input that is not JSON, a payload without a path, an unreadable
//! file, an unreadable `invar.yaml`, a malformed lock line — allows the edit and says
//! why. Malformed lines are `acceptance verify`'s to report. A `Write` or `Edit` is
//! denied in these cases only:
//!
//! - the path is an existing `.invar/plans/<name>/spec.md` whose header says
//!   `Status: approved`;
//! - the path is a `.invar/plans/<name>/spec.md`, a draft or not there yet, and the
//!   header the call would leave says `Status: approved`, or holds a value for an
//!   approval key (`Approved-by`, `Approved-at`, `Approved-checksum`,
//!   `Lockfile-checksum`, `Acceptance`) that the file did not already have. Only
//!   `invar plan approve` writes them (plan-header.md §5); a value a draft already
//!   holds is not an error there (§5.1), so an edit that leaves it alone passes. Any
//!   other edit of a draft passes; that is Phase 1 working;
//! - the path is the unlock record of a Plan, whether or not it exists yet. Only
//!   `invar acceptance unlock` and `invar acceptance lock` write it;
//! - the path is under `acceptance.dir` and the file has a locked line, of any
//!   ticket, whether or not its digest still matches. A pending file is the
//!   test-writer working;
//! - the path is under `acceptance.dir` and the call would write a digest or an `ac=` list into a lock
//!   line. Only `invar acceptance lock` writes one, after it has run the tests (A-4).
//!
//! - the path is the project's `invar.yaml`, and the file the call would leave weakens
//!   the checks against the one there now, by the list of `invar-yaml.md` §3.9;
//! - the path is the project's `.claude/settings.json`, and the call would take out the
//!   deny rule `invar init` writes for `.invar/decisions/**`;
//! - the path is the baseline or the report of a Plan audit,
//!   `.invar/out/<TICKET>-audit/baseline.md` or `report.txt`, whether or not it exists
//!   yet. Only `invar plan audit begin` and `report` write them; `findings.md` beside them
//!   is the auditor's, and passes (`plan-audit.md` §7.2).
//!
//! Only files under `acceptance.dir` are judged for a lock line, because anywhere else
//! `invar-lock:` is just text, such as in invar's own contract. Likewise only a Plan's
//! header is judged for approval values: `Status: approved` in its body is just text.
//! Both judge the file the call would leave ([`text_after`]), because an `Edit`'s
//! `new_string` need not name the key or the marker it fills.
//!
//! The hook's matcher is `Write|Edit|Bash`: Claude Code 2.1.332 has no `MultiEdit`
//! (`plugin/NOTES.md` §24). A tool that edits files and is added later goes into the
//! matcher, and its payload into [`text_after`], with a test for each guarded file.

use std::io;
use std::path::{Component, Path, PathBuf};

use serde_json::Value;

use crate::format::config;
use crate::format::lock::{self, MARKER};
use crate::format::plan::{APPROVAL_KEYS, Checked, Mode, Plan, Status};
use crate::fs::plans::{SPEC_FILE, UNLOCK_FILE};
use crate::fs::{ProjectRoot, RootError};

#[cfg(test)]
mod tests;

/// What the guard tells the hook.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Nothing the guard holds: the tool call takes its normal course, including
    /// any permission prompt. The guard never answers "allow" outright.
    Pass,
    /// An approved Plan, approval values or a digest written by hand, or a locked test:
    /// the tool call is denied, and `reason` tells the agent why and what to do instead.
    Deny { reason: String },
    /// The guard could not decide. The tool call takes its normal course and
    /// `problem` is reported to the developer.
    Unsure { problem: String },
}

/// Decide on a `PreToolUse` payload. `fallback_cwd` resolves a relative
/// `file_path` when the payload carries no `cwd`.
pub fn decide(input: &[u8], fallback_cwd: &Path) -> Verdict {
    let payload = match parse(input) {
        Ok(payload) => payload,
        Err(problem) => return Verdict::Unsure { problem },
    };
    let tool = payload
        .get("tool_name")
        .and_then(Value::as_str)
        .unwrap_or("an unnamed tool");
    if tool == "Bash" {
        return judge_command(&payload);
    }
    match target(&payload, tool, fallback_cwd) {
        Ok(path) => judge(&path, tool, &payload),
        Err(problem) => Verdict::Unsure { problem },
    }
}

fn parse(input: &[u8]) -> Result<Value, String> {
    if input.iter().all(u8::is_ascii_whitespace) {
        return Err("no hook input on stdin; expected the PreToolUse JSON payload".into());
    }
    serde_json::from_slice(input).map_err(|err| format!("the hook input is not valid JSON ({err})"))
}

/// The absolute path the tool call would write.
fn target(payload: &Value, tool: &str, fallback_cwd: &Path) -> Result<PathBuf, String> {
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

fn judge(path: &Path, tool: &str, payload: &Value) -> Verdict {
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
    if matches!(tool, "Write" | "Edit") {
        if relative == Path::new(config::FILE_NAME) {
            return judge_config(path, &relative, payload);
        }
        if relative == Path::new(SETTINGS_FILE) {
            return judge_settings(path, &relative, payload);
        }
        if let Some(reason) = audit_file(&relative) {
            return Verdict::Deny { reason };
        }
    }
    if plan_dir_name(&relative, UNLOCK_FILE).is_some() {
        return Verdict::Deny {
            reason: format!(
                "{} is the unlock record. It says which locked tests the developer unlocked, and \
                 only `invar acceptance unlock`, through `/invar:relock`, and `invar acceptance \
                 lock` write it, so it is not edited through any tool. If a locked test is wrong, \
                 stop and answer `BLOCKED-TEST <file> <AC> <reason>`; the developer decides",
                relative.display()
            ),
        };
    }
    match plan_dir_name(&relative, SPEC_FILE) {
        Some(name) => judge_plan(path, &relative, &name, tool, payload),
        // The tools whose payloads are known (`plugin/NOTES.md` §11). Any other keeps
        // what it had: the Plan is all it is judged for.
        None if matches!(tool, "Write" | "Edit") => judge_test(path, &root, &relative, payload),
        None => Verdict::Pass,
    }
}

fn judge_plan(path: &Path, relative: &Path, name: &str, tool: &str, payload: &Value) -> Verdict {
    let text = match std::fs::read(path) {
        Ok(bytes) => match String::from_utf8(bytes) {
            Ok(text) => Some(text),
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
        Err(err) if err.kind() == io::ErrorKind::NotFound => None,
        Err(err) => {
            return Verdict::Unsure {
                problem: format!("cannot read {}: {err}", relative.display()),
            };
        }
    };

    // `Status` and the approval keys read the same in either mode; the
    // sections, which differ by mode, play no part here.
    let before = text.as_deref().map(|text| Plan::check(text, Mode::Light));
    if let Some(checked) = &before
        && checked.status == Some(Status::Approved)
    {
        let ticket = ticket_or(checked, name);
        return Verdict::Deny {
            reason: format!(
                "{} is the approved Plan for {ticket}. Its text is what the developer \
                 approved, and `Approved-checksum` holds it fixed, so it is not edited \
                 during implementation — through any tool. If the Plan is wrong or \
                 incomplete, stop and tell the developer what should change. They decide \
                 how it changes: they record a `failed` entry, which returns the Plan to \
                 draft for the planner to revise and for them to approve again with \
                 `/invar:approve {ticket}`, or they edit it themselves and approve it \
                 again with `invar plan approve {ticket} --reapprove`",
                relative.display()
            ),
        };
    }

    // The tools whose payloads are known (`plugin/NOTES.md` §11).
    if !matches!(tool, "Write" | "Edit") {
        return Verdict::Pass;
    }
    let (written, ticket) = match text_after(payload, text.as_deref()) {
        Some(after) => {
            let after = Plan::check(&after, Mode::Light);
            (
                approval_written(before.as_ref(), &after),
                ticket_or(&after, name),
            )
        }
        None => {
            let ticket = before
                .as_ref()
                .map_or_else(|| name.to_owned(), |b| ticket_or(b, name));
            (approval_in_fragment(payload), ticket)
        }
    };
    if written.is_empty() {
        return Verdict::Pass;
    }
    let noun = if written.len() == 1 { "it" } else { "them" };
    Verdict::Deny {
        reason: format!(
            "this would write {} into the header of {}. Only `invar plan approve` writes \
             {noun}, when the developer approves the Plan, so a draft keeps `Status: draft` \
             and leaves every approval key as it is. If the Plan is ready, stop: tell the \
             developer it is ready and wait. Only the developer approves it, with \
             `/invar:approve {ticket}`",
            written.join(" and "),
            relative.display()
        ),
    }
}

/// `Ticket` from the header, or else the Plan's directory name.
fn ticket_or(checked: &Checked, name: &str) -> String {
    checked
        .ticket
        .as_ref()
        .map_or_else(|| name.to_owned(), |(ticket, _)| ticket.clone())
}

/// The approval values `after` holds that `before` did not, as the reason names them:
/// `Status: approved`, then each key whose value the call writes. A value the Plan
/// already had is not this call's doing: a draft may hold one (plan-header.md §5.1),
/// and an edit elsewhere in it changes nothing about its approval.
fn approval_written(before: Option<&Checked>, after: &Checked) -> Vec<String> {
    let mut written = Vec::new();
    // `judge_plan` has already denied a Plan that was approved before the call.
    if after.status == Some(Status::Approved) {
        written.push("`Status: approved`".to_owned());
    }
    let had = |key: &str, value: &str| {
        before.is_some_and(|before| {
            before
                .approval_values
                .iter()
                .any(|(k, v)| *k == key && v == value)
        })
    };
    for (key, value) in &after.approval_values {
        if !had(key, value) {
            written.push(format!("a value for `{key}`"));
        }
    }
    written
}

/// The approval values the text a call writes holds on its own, when the file it would
/// leave cannot be told ([`text_after`] is `None`): a line that is, once trimmed,
/// `Status: approved` or an approval key with a value.
///
/// **This may over-deny.** A fragment has no header to read: `Approved-by: rade` in a
/// criterion or a code block cannot be told from the header line, so it is denied too.
/// Passing instead would let any call the guard cannot follow write an approval.
fn approval_in_fragment(payload: &Value) -> Vec<String> {
    let input = payload.get("tool_input");
    let text_of = |key: &str| input.and_then(|i| i.get(key)).and_then(Value::as_str);
    let Some(fragment) = text_of("content").or_else(|| text_of("new_string")) else {
        return Vec::new();
    };
    let mut written = Vec::new();
    let lines = || fragment.lines().map(str::trim);
    if lines().any(|line| line == "Status: approved") {
        written.push("`Status: approved`".to_owned());
    }
    for key in APPROVAL_KEYS {
        let filled = lines().any(|line| {
            line.split_once(':')
                .is_some_and(|(k, value)| k == key && !value.trim().is_empty())
        });
        if filled {
            written.push(format!("a value for `{key}`"));
        }
    }
    written
}

/// `<name>` when `relative` is `.invar/plans/<name>/<file>`.
fn plan_dir_name(relative: &Path, file: &str) -> Option<String> {
    let parts: Vec<&str> = relative
        .components()
        .map(|part| match part {
            Component::Normal(name) => name.to_str(),
            _ => None,
        })
        .collect::<Option<_>>()?;
    match parts.as_slice() {
        [".invar", "plans", name, last] if *last == file => Some((*name).to_owned()),
        _ => None,
    }
}

/// The reason a `Write` or `Edit` of `relative` is denied, when it is the baseline or the
/// report of a Plan audit (`plan-audit.md` §2, §7.2).
fn audit_file(relative: &Path) -> Option<String> {
    let parts: Vec<&str> = relative
        .components()
        .map(|part| match part {
            Component::Normal(name) => name.to_str(),
            _ => None,
        })
        .collect::<Option<_>>()?;
    let [".invar", "out", dir, file] = parts.as_slice() else {
        return None;
    };
    dir.strip_suffix("-audit")
        .filter(|ticket| !ticket.is_empty())?;
    let what = match *file {
        "baseline.md" => {
            "the baseline of a Plan audit: the Plan as it was when the developer began the \
             audit, which the report compares the Plan with. Only `invar plan audit begin` \
             writes it"
        }
        "report.txt" => {
            "the report of a Plan audit. Only `invar plan audit report` writes it, from the \
             baseline, the findings and the Plan"
        }
        _ => return None,
    };
    Some(format!(
        "{} is {what}, so it is not written through any tool. Only `findings.md` in that \
         directory is the auditor's: write your edits and findings there, and run `invar plan \
         audit report` to see the result (plan-audit.md §7.2)",
        relative.display()
    ))
}

// --- locked acceptance tests ---------------------------------------------------

fn judge_test(path: &Path, root: &ProjectRoot, relative: &Path, payload: &Value) -> Verdict {
    let loaded = match config::load(root) {
        Ok(loaded) => loaded,
        Err(err) => {
            return Verdict::Unsure {
                problem: format!(
                    "cannot tell whether {} is a locked acceptance test: {err}",
                    relative.display()
                ),
            };
        }
    };
    let Some(acceptance) = loaded.config.acceptance else {
        return Verdict::Pass;
    };
    if !is_under(relative, &acceptance.dir) {
        return Verdict::Pass;
    }

    let (existing, target) = match std::fs::read(path) {
        Ok(bytes) => {
            let verdict = judge_existing(&bytes, relative);
            (Some(bytes), verdict)
        }
        // A test being written for the first time.
        Err(err) if err.kind() == io::ErrorKind::NotFound => (None, Verdict::Pass),
        Err(err) => (
            None,
            Verdict::Unsure {
                problem: format!("cannot read {}: {err}", relative.display()),
            },
        ),
    };
    if matches!(target, Verdict::Deny { .. }) {
        return target;
    }
    // Asked even when the target is in doubt: what the call writes is known.
    if writes_digest(payload, existing.as_deref()) {
        return Verdict::Deny {
            reason: format!(
                "this would write a digest or an `ac=` list into the `{MARKER}` line of {}. Only \
                 `invar acceptance lock` writes a digest, and the criteria list with it, after it \
                 has run the tests, and writing one by hand skips the check that they fail before \
                 the code exists. Write the pending line, `{MARKER} <TICKET>`, and run `invar \
                 acceptance lock <TICKET>`",
                relative.display()
            ),
        };
    }
    target
}

/// The verdict on the file as it stands.
fn judge_existing(bytes: &[u8], relative: &Path) -> Verdict {
    // Not `read` alone, which refuses any file that is not UTF-8: a file with no marker
    // has no lock, as `acceptance.dir` is walked (`acceptance-lock.md` §9.1).
    if !bytes
        .windows(MARKER.len())
        .any(|at| at == MARKER.as_bytes())
    {
        return Verdict::Pass;
    }
    match lock::read(bytes) {
        Ok(Some(line)) if line.recorded.is_some() => Verdict::Deny {
            reason: format!(
                "{} is a locked acceptance test of {ticket}. Neither the coder nor the \
                 test-writer changes it once it is locked, through any tool, and \
                 `invar acceptance verify` checks its digest. If the test is wrong, stop and \
                 answer `BLOCKED-TEST <file> <AC> <reason>`; the developer decides whether it \
                 is unlocked. If the Plan is meant to change it, the developer lists it in \
                 `## Acceptance tests to change` and approves the Plan again; the change then \
                 goes through the test-writer and `invar acceptance lock`",
                relative.display(),
                ticket = line.ticket,
            ),
        },
        Ok(_) => Verdict::Pass,
        // `acceptance verify` reports it. Blocking here would keep the agent from fixing it.
        Err(err) => Verdict::Unsure {
            problem: format!("{}: {err}", relative.display()),
        },
    }
}

/// Whether the `Write` or `Edit` in `payload` puts a digest into a lock line. `existing`
/// is the file as it is, if it is there.
///
/// An `Edit` is also judged by the file it would leave, because `new_string` need not
/// hold the marker: replacing `SHOP-214` with `SHOP-214 sha256:…` writes a digest into
/// the line all the same. A digest the file already had is not this call's doing: a
/// malformed one stays `acceptance verify`'s to report, and may be being fixed.
fn writes_digest(payload: &Value, existing: Option<&[u8]>) -> bool {
    let input = payload.get("tool_input");
    let text_of = |key: &str| input.and_then(|i| i.get(key)).and_then(Value::as_str);
    if let Some(content) = text_of("content") {
        return lock::holds_digest(content);
    }
    let Some(new) = text_of("new_string") else {
        return false;
    };
    if lock::holds_digest(new) {
        return true;
    }
    let Some(before) = existing.and_then(|bytes| std::str::from_utf8(bytes).ok()) else {
        return false;
    };
    let Some(after) = text_after(payload, Some(before)) else {
        return false;
    };
    lock::holds_digest(&after) && !lock::holds_digest(before)
}

/// The file the `Write` or `Edit` in `payload` would leave: `content` for a `Write`;
/// for an `Edit`, `before`, the file as it is, with `old_string` replaced by
/// `new_string` (every occurrence under `replace_all`), or `new_string` alone when
/// `old_string` is empty and the file is not there or empty. `None` when that cannot
/// be told: any other `Edit` of a file that is not there or not text, an empty
/// `old_string` on a file with text in it, or a payload that is neither.
fn text_after(payload: &Value, before: Option<&str>) -> Option<String> {
    let input = payload.get("tool_input");
    let text_of = |key: &str| input.and_then(|i| i.get(key)).and_then(Value::as_str);
    if let Some(content) = text_of("content") {
        return Some(content.to_owned());
    }
    let new = text_of("new_string")?;
    let old = text_of("old_string")?;
    if old.is_empty() {
        // Claude Code creates the file, or fills an empty one, with `new_string`, and
        // refuses the call on any other file (`plugin/NOTES.md` §11, #153).
        return before.is_none_or(str::is_empty).then(|| new.to_owned());
    }
    let before = before?;
    let replace_all = input
        .and_then(|i| i.get("replace_all"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    Some(if replace_all {
        before.replace(old, new)
    } else {
        before.replacen(old, new, 1)
    })
}

/// The project's Claude Code settings, which hold the deny rule `invar init` writes.
const SETTINGS_FILE: &str = ".claude/settings.json";

/// The file as it is, as text. `None` when it is not there, or not text.
fn read_text(path: &Path) -> Option<String> {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok())
}

/// A `Write` or `Edit` of the project's `invar.yaml`: denied when the file it would leave
/// weakens the checks against the file as it is (`invar-yaml.md` §3.9).
///
/// **Allowed whenever it cannot tell**: the result cannot be computed, or either side is
/// not a valid config. The boundary is `invar config check` in CI, which compares the
/// branch with the base and refuses a weakening there whatever tool made it; this only
/// saves the agent an edit the developer has to undo.
fn judge_config(path: &Path, relative: &Path, payload: &Value) -> Verdict {
    let before = read_text(path);
    let Some(after) = text_after(payload, before.as_deref()) else {
        return Verdict::Pass;
    };
    let read = |text: &str| {
        config::parse(text, relative)
            .ok()
            .map(|loaded| loaded.config)
    };
    let (Some(old), Some(new)) = (read(before.as_deref().unwrap_or_default()), read(&after)) else {
        return Verdict::Pass;
    };
    let weakened = config::weakenings(&old, &new);
    if weakened.is_empty() {
        return Verdict::Pass;
    }
    let list: Vec<String> = weakened.iter().map(|w| format!("`{w}`")).collect();
    Verdict::Deny {
        reason: format!(
            "this would weaken what invar checks, in {}: {}. These keys decide whether the \
             Plans, the locked tests and the required paths are checked at all, so changing \
             them that way is the developer's decision, not yours. Leave them as they are, and \
             tell the developer what you would change and why: they edit invar.yaml \
             themselves, and CI asks them for an `{trailer}: <reason>` trailer (`git commit \
             --amend --trailer \"{trailer}: <reason>\"`). Other keys, and adding or \
             strengthening any key, stay yours to edit",
            relative.display(),
            list.join(", "),
            trailer = crate::config_check::CONFIG_TRAILER,
        ),
    }
}

/// A `Write` or `Edit` of the project's `.claude/settings.json`: denied when the file it
/// would leave no longer has the deny rule `invar init` writes for the decision log, and
/// the file as it is has it.
///
/// Allowed whenever it cannot tell, or when either side is not JSON: the rule is a speed
/// bump for the log, not a boundary (`init`, NOTES D-17.3), and a settings file the guard
/// cannot read is not one to block edits to.
fn judge_settings(path: &Path, relative: &Path, payload: &Value) -> Verdict {
    let has_rule = |text: &str| -> Option<bool> {
        let value: Value = serde_json::from_str(text).ok()?;
        Some(
            value
                .pointer("/permissions/deny")
                .and_then(Value::as_array)
                .is_some_and(|rules| {
                    rules
                        .iter()
                        .any(|rule| rule.as_str() == Some(crate::init::DENY_RULE))
                }),
        )
    };
    let before = read_text(path);
    let Some(after) = text_after(payload, before.as_deref()) else {
        return Verdict::Pass;
    };
    if before.as_deref().and_then(has_rule) == Some(true) && has_rule(&after) == Some(false) {
        return Verdict::Deny {
            reason: format!(
                "this would take the rule `{rule}` out of {}. It keeps agents from reading the \
                 decision log, which only the planner reads, through `invar context show`. \
                 Leave it in; if it is in your way, tell the developer, who changes it",
                relative.display(),
                rule = crate::init::DENY_RULE,
            ),
        };
    }
    Verdict::Pass
}

/// Whether `relative` is a path below `dir`, as `acceptance.dir` may write it (`./t`).
fn is_under(relative: &Path, dir: &str) -> bool {
    let dir: Vec<Component> = Path::new(dir)
        .components()
        .filter(|part| matches!(part, Component::Normal(_)))
        .collect();
    let parts: Vec<Component> = relative.components().collect();
    parts.len() > dir.len() && parts[..dir.len()] == dir[..]
}

// --- commands only the developer runs -----------------------------------------

/// A command the agent must not run, because what it records is a person's
/// decision.
struct HumanOnly {
    /// The words that follow `invar`: `["plan", "approve"]`, `["done"]`.
    words: &'static [&'static str],
    /// What the agent is shown, given the ticket (or `<ticket>`). The
    /// transcript prefixes it with "hook error" (plugin/NOTES.md §11), so it
    /// has to say on its own what to do instead.
    reason: fn(&str) -> String,
}

/// `scenario confirm` joins this list with the milestone that adds `.feature`
/// files.
///
/// `decision approve` and `decision record` write the insert-only decision log
/// (decision-entry.md §8): the first records the draft the developer reviewed,
/// the second writes past the draft altogether. Both are the developer's —
/// `decision record` stays for the notes they write by hand.
///
/// `done` deletes the approved Plan and closes the cycle: the developer runs it,
/// with `/invar:done` or from a terminal, or CI does, when the work is finished.
///
/// `acceptance unlock` takes the lock off a test, and the next `lock` of that file
/// skips the red check (`acceptance-lock.md` §11).
const HUMAN_ONLY: &[HumanOnly] = &[
    HumanOnly {
        words: &["plan", "approve"],
        reason: plan_approve_reason,
    },
    HumanOnly {
        words: &["decision", "approve"],
        reason: decision_approve_reason,
    },
    HumanOnly {
        words: &["decision", "record"],
        reason: decision_record_reason,
    },
    HumanOnly {
        words: &["done"],
        reason: done_reason,
    },
    HumanOnly {
        words: &["acceptance", "unlock"],
        reason: acceptance_unlock_reason,
    },
    HumanOnly {
        words: &["cycle", "skip"],
        reason: cycle_skip_reason,
    },
    HumanOnly {
        words: &["config", "accept"],
        reason: config_accept_reason,
    },
    HumanOnly {
        words: &["plan", "audit", "begin"],
        reason: plan_audit_begin_reason,
    },
];

fn plan_audit_begin_reason(target: &str) -> String {
    format!(
        "`invar plan audit begin` is the developer's, not yours: it records the Plan as the \
         baseline an audit is compared with, and a baseline taken again after an edit would \
         hide that edit from the report (plan-audit.md D-3). Do not run it, and do not write \
         the baseline another way. If the Plan should be audited, say so and wait: the \
         developer begins it with `/invar:plan-audit {target}`. {}",
        mention("plan audit begin")
    )
}

fn config_accept_reason(_: &str) -> String {
    format!(
        "`invar config accept` is the developer's decision, not yours: it signs, with their \
         key, that this branch weakens what invar checks, in invar.yaml, on purpose. Do not run \
         it, and do not look for another way to record the same thing. Tell the developer what \
         the branch changes in invar.yaml and why, and wait: they accept it themselves, with \
         `/invar:config-accept <reason>`, or the keys go back. {}",
        mention("config accept")
    )
}

fn cycle_skip_reason(_: &str) -> String {
    format!(
        "`invar cycle skip` is the developer's decision, not yours: it signs, with their key, \
         that this branch changes paths that need a cycle and goes without one. Do not run it, \
         and do not look for another way to record the same thing. Tell the developer which \
         required paths the change touches and wait: they skip it themselves, with \
         `/invar:skip <reason>`, or the change goes through a cycle. {}",
        mention("cycle skip")
    )
}

/// How every reason ends: what to do when the command was only mentioned.
fn mention(words: &str) -> String {
    format!(
        "If you only meant to mention the command, write it without `invar` directly in \
         front of `{words}`."
    )
}

fn plan_approve_reason(target: &str) -> String {
    format!(
        "`invar plan approve` is the developer's decision, not yours: it records that a \
         person read the Plan and accepted it, and every later check relies on that. Do \
         not run it, and do not look for another way to record the same thing. Tell the \
         developer the Plan is ready and wait: they approve it themselves, with \
         `/invar:approve {target}`, which starts Phase 2 once the approval is recorded (a \
         Plan approved before and changed since takes `invar plan approve {target} \
         --reapprove`). {}",
        mention("plan approve")
    )
}

fn decision_approve_reason(target: &str) -> String {
    format!(
        "`invar decision approve` is the developer's decision, not yours: it records the \
         draft entry in the decision log, which is never changed afterwards, and a \
         `failed` entry also returns the Plan to `draft`. Do not run it, and do not look \
         for another way to record the same thing. Tell the developer the draft \
         (`decision.md`, next to the Plan) is ready and wait: they read it, edit it if \
         they want to, and record it themselves with `/invar:log-approve {target}`. {}",
        mention("decision approve")
    )
}

fn decision_record_reason(target: &str) -> String {
    format!(
        "`invar decision record` is the developer's decision, not yours: it writes \
         straight into the decision log, which is never changed afterwards, past the \
         draft the developer reviews. Do not run it, and do not look for another way to \
         record the same thing. If an entry is worth keeping, say so and wait: the \
         developer has it drafted with `/invar:log {target}`, reads the draft, and \
         records it with `/invar:log-approve {target}`. `invar decision record` stays \
         for the notes the developer writes by hand. {}",
        mention("decision record")
    )
}

fn done_reason(target: &str) -> String {
    format!(
        "`invar done` is the developer's decision, not yours: it closes the cycle by \
         deleting the approved Plan. Do not run it, and do not look for another way to do \
         the same thing — do not delete the Plan directory either. Tell the developer the \
         cycle is ready to close and wait: they close it themselves, with `/invar:done \
         {target}`, or CI does. {}",
        mention("done")
    )
}

fn acceptance_unlock_reason(target: &str) -> String {
    format!(
        "`invar acceptance unlock` is the developer's decision, not yours: it takes the lock \
         off a test, and the next `invar acceptance lock` of that file skips the check that it \
         fails first. Do not run it, and do not look for another way to do the same thing. Say \
         which test you believe is wrong and why (the coder answers `BLOCKED-TEST <file> <AC> \
         <reason>`) and wait: the developer unlocks it themselves, with `/invar:relock {target} \
         <file>`. {}",
        mention("acceptance unlock")
    )
}

/// A `Bash` call: deny it when its command line runs a human-only command.
fn judge_command(payload: &Value) -> Verdict {
    let Some(command) = payload
        .get("tool_input")
        .and_then(|input| input.get("command"))
        .and_then(Value::as_str)
    else {
        return Verdict::Unsure {
            problem: "the hook input for Bash has no `tool_input.command`; check the hook's \
                      matcher"
                .into(),
        };
    };
    if writes_trailer(command, crate::cycle::SKIP_TRAILER) {
        return Verdict::Deny {
            reason: skip_trailer_reason(),
        };
    }
    if writes_trailer(command, crate::config_check::CONFIG_TRAILER) {
        return Verdict::Deny {
            reason: config_trailer_reason(),
        };
    }
    match human_only(command) {
        Some((entry, target)) => Verdict::Deny {
            reason: (entry.reason)(target.as_deref().unwrap_or("<ticket>")),
        },
        None => Verdict::Pass,
    }
}

/// Whether a command line writes the trailer `key` into a commit: a `git commit` in one of
/// its simple commands, and the key anywhere in the line, in any case, so that a message
/// in `-m`, in `--trailer` or in a here-document is seen. For `Invar-Skip` and
/// `Invar-Config` (`invar-yaml.md` §3.8, §3.9).
///
/// **Words, not shell**, as [`human_only`]: a message written to a file first and given
/// with `-F`, or a key a shell builds at run time, is not seen. It makes an honest agent
/// stop and ask; the binding of a skip to a person comes with the verification of signed
/// approvals (#154).
fn writes_trailer(command: &str, key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    if !command.to_ascii_lowercase().contains(&key) {
        return false;
    }
    let command = command.replace("\\\n", " ");
    command
        .split([';', '&', '|', '\n', '(', ')', '`'])
        .any(|segment| {
            let words: Vec<&str> = segment
                .split(|c: char| c.is_whitespace() || matches!(c, '"' | '\''))
                .filter(|word| !word.is_empty())
                .collect();
            words.iter().enumerate().any(|(at, word)| {
                let name = word.rsplit(['/', '\\']).next().unwrap_or(word);
                matches!(name, "git" | "git.exe") && words[at + 1..].contains(&"commit")
            })
        })
}

fn config_trailer_reason() -> String {
    format!(
        "an `{key}:` trailer is the developer's decision, not yours: it tells `invar config \
         check` in CI that this branch weakens what invar checks, in invar.yaml, on purpose, \
         and why. Do not write it, in a commit message, a `--trailer` or any other way. Tell \
         the developer what the branch changes in invar.yaml and why; they sign it \
         themselves, with `/invar:config-accept <reason>` when approvals are signed, else \
         `git commit --amend --trailer \"{key}: <reason>\"`. If you only meant to mention the \
         trailer, leave the key out of a command that runs `git commit`.",
        key = crate::config_check::CONFIG_TRAILER
    )
}

fn skip_trailer_reason() -> String {
    format!(
        "an `{key}:` trailer is the developer's decision, not yours: it tells `invar cycle \
         check` in CI that this branch changes a path that needs a cycle and goes without one, \
         and why. Do not write it, in a commit message, a `--trailer` or any other way. Commit \
         without it, tell the developer that the change touches a path `cycle.required_paths` \
         names, and that the pull request needs either a cycle closed with /invar:done or a \
         skip they make themselves: `/invar:skip <reason>` when approvals are signed, else an \
         `{key}: <reason>` trailer, `git commit --amend --trailer \"{key}: <reason>\"`. If you \
         only meant to mention the trailer, leave the key out of a command that runs \
         `git commit`.",
        key = crate::cycle::SKIP_TRAILER
    )
}

/// The human-only command a shell command line runs, and its ticket: the value
/// of `--ticket` when there is one, the first argument after the command
/// otherwise.
///
/// **Words, not shell.** The line is cut at `;`, `&`, `|`, `(`, `)`, backticks
/// and newlines into simple commands, and each is split into words with quotes
/// treated as spaces, so `bash -c 'invar plan approve X'`, `$(…)`, `env …`
/// and `cd app && …` all show the same three words. A command is recognised
/// when a word that is `invar` (or a path ending in it) is followed, in the
/// same simple command, by the two words of an entry. That is deliberately
/// blunt: it also catches a commit message that quotes the whole command (an
/// apostrophe counts as a quote, so even `invar's plan approve`), which the
/// reason says how to avoid, and it misses anything a shell can build at run
/// time, or `cargo run -- plan approve`, which has no `invar` word. Asking for
/// `--help` is not running it.
fn human_only(command: &str) -> Option<(&'static HumanOnly, Option<String>)> {
    let command = command.replace("\\\n", " ");
    for segment in command.split([';', '&', '|', '\n', '(', ')', '`']) {
        let words: Vec<&str> = segment
            .split(|c: char| c.is_whitespace() || matches!(c, '"' | '\''))
            .filter(|word| !word.is_empty())
            .collect();
        let Some(start) = words.iter().position(|word| is_invar(word)) else {
            continue;
        };
        let after = &words[start + 1..];
        if after.iter().any(|word| matches!(*word, "--help" | "-h")) {
            continue;
        }
        for entry in HUMAN_ONLY {
            let length = entry.words.len();
            let found = after
                .windows(length)
                .position(|window| window == entry.words);
            if let Some(at) = found {
                return Some((entry, ticket_of(&after[at + length..])));
            }
        }
    }
    None
}

/// The ticket among a command's arguments: the value of `--ticket`
/// (`decision record`), or else the first word that is not an option (`plan
/// approve SHOP-214`, `decision approve SHOP-214`).
fn ticket_of(words: &[&str]) -> Option<String> {
    let flagged = words.iter().enumerate().find_map(|(at, word)| {
        if *word == "--ticket" {
            words.get(at + 1).copied()
        } else {
            word.strip_prefix("--ticket=")
        }
    });
    flagged
        .or_else(|| words.iter().copied().find(|word| !word.starts_with('-')))
        .map(str::to_owned)
}

/// `invar`, `/usr/local/bin/invar`, `.\target\debug\invar.exe`.
fn is_invar(word: &str) -> bool {
    let name = word.rsplit(['/', '\\']).next().unwrap_or(word);
    matches!(name, "invar" | "invar.exe")
}
