//! `invar plan validate --all-drafts --hook`, for a `SubagentStop` hook.
//!
//! The answer follows what Claude Code does with it (`plugin/NOTES.md` §7, §13):
//!
//! - **Block:** `{"decision": "block", "reason": …}` on stdout, the reason again
//!   on stderr, exit 2. The planner is not allowed to stop yet and is shown the
//!   reason: what to fix. Either signal alone blocks; exit 2 also makes the
//!   verdict plain to a script, as for `plan guard`.
//! - **Pass:** nothing at all, exit 0.
//! - **Gave up / unsure:** `systemMessage` on stdout, which the developer
//!   sees, the same text on stderr, exit 0. The planner may stop.
//!
//! Exit 2 means *block* here and nowhere else (`main.rs` turns every usage
//! error into exit 1), so a hook started with arguments this binary does not
//! know never holds a planner.

use std::io::Read as _;
use std::process::ExitCode;

use anyhow::Result;
use serde_json::json;

use crate::gate::stop::{self, Verdict};

pub fn run() -> Result<ExitCode> {
    let mut input = Vec::new();
    let verdict = match std::io::stdin().read_to_end(&mut input) {
        Ok(_) => {
            let cwd = std::env::current_dir().unwrap_or_default();
            stop::decide(&input, &cwd)
        }
        Err(err) => Verdict::Unsure {
            problem: format!("cannot read the hook input from stdin: {err}"),
        },
    };

    Ok(match verdict {
        Verdict::Pass => ExitCode::SUCCESS,
        Verdict::Block { reason } => {
            println!("{}", json!({ "decision": "block", "reason": reason }));
            eprintln!("{reason}");
            ExitCode::from(2)
        }
        Verdict::GaveUp { message } => {
            println!("{}", json!({ "systemMessage": message }));
            eprintln!("{message}");
            ExitCode::SUCCESS
        }
        Verdict::Unsure { problem } => {
            let message = format!("invar plan validate --hook: {problem}; the planner may stop");
            println!("{}", json!({ "systemMessage": message }));
            eprintln!("{message}");
            ExitCode::SUCCESS
        }
    })
}
