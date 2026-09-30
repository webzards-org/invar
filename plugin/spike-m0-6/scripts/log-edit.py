#!/usr/bin/env python3
"""M0-6 spike: log the path of every file Claude edits.

A rehearsal for the M5 hook that runs `invar lint check` after a code edit
(§13). The code is disposable; what it demonstrates is not:

  - a PostToolUse hook receives the whole tool call as JSON on stdin
  - the edited path is `tool_input.file_path`
  - stdout is parsed as JSON when the hook exits 0, so nothing else may be
    printed there
  - `systemMessage` surfaces one line to the developer in the transcript
  - durable state belongs in ${CLAUDE_PLUGIN_DATA}, never in
    ${CLAUDE_PLUGIN_ROOT}, which changes with every plugin version

**It never blocks and never fails the session.** Every path through this
script exits 0, including every error path (§13, "no blocking in flight").
A hook that halts an editing session gets switched off within a week, and
then none of it runs. The real lint hook inherits exactly this property: it
reports, and `invar generate` at the end of the cycle is what actually
renders a verdict.

Usage outside a session, to find the log:

    python3 log-edit.py --where
"""

from __future__ import annotations

import datetime
import json
import os
import sys
from pathlib import Path

LOG_NAME = "edits.log"


def log_path() -> Path:
    """Where the log lives.

    ${CLAUDE_PLUGIN_DATA} is the documented home for a plugin's durable
    files: it survives plugin updates, while ${CLAUDE_PLUGIN_ROOT} points at
    a per-version directory that is replaced on every update. Writing state
    into the plugin root means losing it silently at the next version.
    """
    base = os.environ.get("CLAUDE_PLUGIN_DATA")
    if base:
        return Path(base) / LOG_NAME
    # Not in a hook process (e.g. `--where` from a shell), or the variable
    # was not exported. Fall back rather than fail.
    return Path.home() / ".claude" / "invar-spike" / LOG_NAME


def main() -> int:
    if "--where" in sys.argv:
        print(log_path())
        return 0

    try:
        event = json.load(sys.stdin)
    except Exception:
        # Malformed input is not the session's problem. Say nothing, exit 0.
        return 0

    tool = event.get("tool_name", "<unknown>")
    tool_input = event.get("tool_input") or {}
    edited = tool_input.get("file_path", "<no file_path>")
    session = event.get("session_id", "<no session>")
    stamp = datetime.datetime.now().isoformat(timespec="seconds")

    try:
        target = log_path()
        target.parent.mkdir(parents=True, exist_ok=True)
        with target.open("a", encoding="utf-8") as handle:
            handle.write(f"{stamp}\t{session}\t{tool}\t{edited}\n")
    except Exception as err:
        # Even a failure to log is reported, never raised.
        json.dump({"systemMessage": f"[invar-spike] could not write log: {err}"}, sys.stdout)
        return 0

    # Exit 0 with JSON on stdout is the standard pattern. `systemMessage` is
    # shown to the developer in the transcript; it does not enter Claude's
    # context. To put something in front of Claude instead, a PostToolUse
    # hook returns hookSpecificOutput.additionalContext -- which is the
    # mechanism §5c relies on for SessionStart, and the reason this spike
    # bothers to return JSON at all rather than just exiting.
    json.dump({"systemMessage": f"[invar-spike] {tool} -> {edited}"}, sys.stdout)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
