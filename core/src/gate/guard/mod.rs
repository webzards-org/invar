//! `invar plan guard` (M1-10): keep the agent's `Write` and `Edit` off an
//! approved Plan, and keep the agent from running the commands that record a
//! person's decision.
//!
//! Two jobs, one hook, told apart by `tool_name`: a `Write` or `Edit` is judged
//! by the file it targets; a `Bash` call is judged by the command line it runs
//! (`HUMAN_ONLY`, at the end of this file). Both answer with the same
//! [`Verdict`]. The second job exists because `plan approve` is a person's
//! decision, and nothing in the command itself can tell a person from an agent
//! that has a shell: the developer approves from another terminal, and the
//! agent is told, at the moment it tries, that this step is not its to take.
//!
//! Run by a `PreToolUse` hook (M1-17), it reads the hook's JSON payload —
//! here, in the CLI, so the plugin stays a thin shell (§13b) and the guard can
//! be tested by piping JSON into it — and decides on `tool_input.file_path`.
//!
//! **Convenience, not the gate.** A `Write|Edit` matcher never sees a file
//! changed through `Bash` (`plugin/NOTES.md` §8), so the guard can only save
//! the agent a wasted edit. The boundary is `Approved-checksum`, which
//! `plan check` and `done` verify whatever tool changed the file. The command
//! guard is the same kind of thing: a shell can spell a command in ways no
//! scan recognises, and a hook that fails lets the call through, so it makes
//! an honest agent stop, and does not stop one that is determined.
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
        Ok(path) => judge(&path),
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

// --- commands only the developer runs -----------------------------------------

/// A command the agent must not run, because what it records is a person's
/// decision.
struct HumanOnly {
    /// The two words that follow `invar`.
    words: [&'static str; 2],
    /// Why, finishing "… is the developer's decision, not yours: …".
    why: &'static str,
}

/// `scenario confirm` joins this list with the milestone that adds `.feature`
/// files.
const HUMAN_ONLY: &[HumanOnly] = &[HumanOnly {
    words: ["plan", "approve"],
    why: "it records that a person read the Plan and accepted it, and every later \
          check relies on that",
}];

impl HumanOnly {
    /// What the agent is shown. The transcript prefixes it with "hook error"
    /// (plugin/NOTES.md §11), so it has to say on its own what to do instead.
    fn reason(&self, target: Option<&str>) -> String {
        let [first, second] = self.words;
        let target = target.unwrap_or("<ticket>");
        format!(
            "`invar {first} {second}` is the developer's decision, not yours: {}. Do not \
             run it, and do not look for another way to record the same thing. Tell the \
             developer the Plan is ready and wait: they run `invar {first} {second} \
             {target}` themselves, from their own terminal (with `--reapprove` if the \
             Plan was approved before and has changed), and you carry on once they \
             have. If you only meant to mention the command, write it without `invar` \
             directly in front of `{first} {second}`.",
            self.why
        )
    }
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
            reason: entry.reason(target.as_deref()),
        },
        None => Verdict::Pass,
    }
}

/// The human-only command a shell command line runs, and the first argument
/// after it (the ticket, normally), if there is one.
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
            let [first, second] = entry.words;
            let found = after
                .windows(2)
                .position(|pair| pair[0] == first && pair[1] == second);
            if let Some(at) = found {
                let target = after[at + 2..]
                    .iter()
                    .find(|word| !word.starts_with('-'))
                    .map(|word| (*word).to_owned());
                return Some((entry, target));
            }
        }
    }
    None
}

/// `invar`, `/usr/local/bin/invar`, `.\target\debug\invar.exe`.
fn is_invar(word: &str) -> bool {
    let name = word.rsplit(['/', '\\']).next().unwrap_or(word);
    matches!(name, "invar" | "invar.exe")
}
