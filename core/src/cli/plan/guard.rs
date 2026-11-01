//! `invar plan guard`, for a `PreToolUse` hook on `Write|Edit|Bash`.
//!
//! A `Write` or `Edit` of an approved Plan or a locked acceptance test is denied, as
//! is one that writes a lock digest by hand, and so is a `Bash` call that runs
//! `invar plan approve`: that is the developer's decision, run from their own
//! terminal. All of them answer in the same way, below.
//!
//! The output follows what Claude Code does with it, verified against
//! version 2.1.291 (`plugin/NOTES.md` §11):
//!
//! - **Deny:** `hookSpecificOutput.permissionDecision: "deny"` with the reason
//!   on stdout, the reason again on stderr, exit 2. Either signal alone blocks
//!   the call; the agent is shown `permissionDecisionReason`. Exit 2 also
//!   makes the result plain to a script or CI.
//! - **Pass:** nothing at all, exit 0. Never `"allow"`, which would skip the
//!   developer's own permission prompt.
//! - **Unsure:** `systemMessage` on stdout, which the developer sees as
//!   `PreToolUse:Write says: …`, the same text on stderr, exit 0. The edit
//!   goes ahead.

use std::io::Read as _;
use std::process::ExitCode;

use anyhow::Result;
use serde_json::json;

use crate::gate::guard::{self, Verdict};

#[derive(Debug, clap::Args)]
pub struct Args {}

pub fn run(_args: Args) -> Result<ExitCode> {
    let mut input = Vec::new();
    let verdict = match std::io::stdin().read_to_end(&mut input) {
        Ok(_) => {
            let cwd = std::env::current_dir().unwrap_or_default();
            guard::decide(&input, &cwd)
        }
        Err(err) => Verdict::Unsure {
            problem: format!("cannot read the hook input from stdin: {err}"),
        },
    };

    Ok(match verdict {
        Verdict::Pass => ExitCode::SUCCESS,
        Verdict::Deny { reason } => {
            let output = json!({
                "hookSpecificOutput": {
                    "hookEventName": "PreToolUse",
                    "permissionDecision": "deny",
                    "permissionDecisionReason": reason,
                }
            });
            println!("{output}");
            eprintln!("{reason}");
            ExitCode::from(2)
        }
        Verdict::Unsure { problem } => {
            let message = format!("invar plan guard: {problem}; the tool call is allowed");
            println!("{}", json!({ "systemMessage": message }));
            eprintln!("{message}");
            ExitCode::SUCCESS
        }
    })
}
