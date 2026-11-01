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
//!   `Status: approved`. A draft Plan is edited freely; that is Phase 1 working;
//! - the path is under `acceptance.dir` and the file has a locked line, of any
//!   ticket, whether or not its digest still matches. A pending file is the
//!   test-writer working;
//! - the path is under `acceptance.dir` and the call would write a digest into a lock
//!   line. Only `invar acceptance lock` writes one, after it has run the tests (A-4).
//!
//! Only files under `acceptance.dir` are judged for a lock line, because anywhere else
//! `invar-lock:` is just text, such as in invar's own contract.

use std::io;
use std::path::{Component, Path, PathBuf};

use serde_json::Value;

use crate::format::config;
use crate::format::lock::{self, MARKER};
use crate::format::plan::{Mode, Plan, Status};
use crate::fs::plans::SPEC_FILE;
use crate::fs::{ProjectRoot, RootError};

#[cfg(test)]
mod tests;

/// What the guard tells the hook.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Nothing the guard holds: the tool call takes its normal course, including
    /// any permission prompt. The guard never answers "allow" outright.
    Pass,
    /// An approved Plan, a locked test, or a digest written by hand: the tool call is
    /// denied, and `reason` tells the agent why and what to do instead.
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
    match plan_dir_name(&relative) {
        Some(name) => judge_plan(path, &relative, &name),
        // The tools whose payloads are known (`plugin/NOTES.md` §11). Any other keeps
        // what it had: the Plan is all it is judged for.
        None if matches!(tool, "Write" | "Edit") => judge_test(path, &root, &relative, payload),
        None => Verdict::Pass,
    }
}

fn judge_plan(path: &Path, relative: &Path, name: &str) -> Verdict {
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
        .unwrap_or_else(|| name.to_owned());
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
                "this would write a digest into the `{MARKER}` line of {}. Only `invar acceptance \
                 lock` writes a digest, after it has run the tests, and writing one by hand skips \
                 the check that they fail before the code exists. Write the pending line, \
                 `{MARKER} <TICKET>`, and run `invar acceptance lock <TICKET>`",
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
    let before = existing.and_then(|bytes| std::str::from_utf8(bytes).ok());
    let (Some(before), Some(old)) = (before, text_of("old_string").filter(|old| !old.is_empty()))
    else {
        return false;
    };
    let replace_all = input
        .and_then(|i| i.get("replace_all"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let after = if replace_all {
        before.replace(old, new)
    } else {
        before.replacen(old, new, 1)
    };
    lock::holds_digest(&after) && !lock::holds_digest(before)
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
];

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
    match human_only(command) {
        Some((entry, target)) => Verdict::Deny {
            reason: (entry.reason)(target.as_deref().unwrap_or("<ticket>")),
        },
        None => Verdict::Pass,
    }
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
