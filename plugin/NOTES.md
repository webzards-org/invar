# Plugin mechanics — notes from the M0-6 spike

What the spike in `spike-m0-6/` established about Claude Code's plugin
mechanisms, so M5 does not rediscover it. The spike's code is disposable; this
file is not.

Section references (§13, §5c, …) point into [`../docs/DESIGN.md`](../docs/DESIGN.md).

---

## 1. Which mechanism carries which M5 component

§13 says the real plugin bundles four things. This is how each one arrives:

| M5 component (§13) | Mechanism | Location in the plugin | Exercised by the spike? |
|---|---|---|---|
| **CLI binary** | A bundled executable, invoked by path or via `bin/` | `bin/invar` (§6, §13) | No |
| **Hooks** | `hooks/hooks.json` | `hooks/hooks.json` | **Yes** — `PostToolUse` |
| **Planning sub-agent** (§13a) | An agent definition | `agents/<name>.md` | No |
| **Coder and review sub-agents** (§13c, since DESIGN v3.1) | Agent definitions | `agents/coder.md`, `agents/reviewer.md` | No — see §17 |
| **Protocol skill** (§5c) | A skill | `skills/<name>/SKILL.md` | No |

Plus the delivery channel §5c depends on but §13 does not list as a component:
binding context reaches every session through a **`SessionStart` hook**, which
is the same `hooks/hooks.json` mechanism the spike already exercises.

## 2. File layout

The manifest goes in `.claude-plugin/`. **Everything else goes at the plugin
root, not inside `.claude-plugin/`.** This is the one rule that trips people up.

```
spike-m0-6/
├── .claude-plugin/
│   └── plugin.json          # the manifest, and the ONLY thing in here
├── hooks/
│   └── hooks.json           # hook configuration
└── scripts/
    └── log-edit.py          # what the hook runs
```

Default locations for the components M5 needs:

| Component | Default location |
|---|---|
| Manifest | `.claude-plugin/plugin.json` |
| Skills | `skills/<name>/SKILL.md` |
| Agents | `agents/<name>.md` |
| Hooks | `hooks/hooks.json` |
| MCP servers | `.mcp.json` |
| Executables | `bin/` |

Two findings worth recording:

- **The manifest is optional.** Without it, Claude Code loads whatever it finds
  in the standard layout, and the plugin name comes from the directory name.
  The real plugin wants one anyway, for `version` and metadata.
- **A `CLAUDE.md` at the plugin root is not loaded as context**, and
  `claude plugin validate` warns when it finds one. This independently confirms
  the §5c decision: the protocol goes into a skill and a `SessionStart` hook,
  not into a `CLAUDE.md` that `init` writes.

Component paths declared in the manifest must start with `./`, must resolve
inside the plugin root, and must use forward slashes. A path that escapes the
root is rejected with `path escapes plugin directory` and the plugin loads
*without that component* — which fails quietly, so watch for it.

## 3. Installing locally

`--plugin-dir` is the development loop. The plugin loads **in place**, is never
copied into the cache, and lasts for that session only:

```bash
cd ~/Projects/webzards/invar
claude --plugin-dir plugin/spike-m0-6
```

Validate before launching — it catches a bad manifest without starting a
session:

```bash
claude plugin validate ./plugin/spike-m0-6
claude plugin validate ./plugin/spike-m0-6 --strict   # warnings become failures; for CI
```

Notes on `--plugin-dir`:

- The plugin gets the id `<manifest name>@inline`, so this one is
  `invar-spike@inline`.
- It is enabled for the session unless the manifest sets
  `defaultEnabled: false` or a settings file sets `"invar-spike@inline": false`.
- A `--plugin-dir` plugin **replaces a same-named installed plugin**, silently.
  Useful later: developing the real `invar` plugin while a released copy is
  installed just works, with the local copy winning.
- `CLAUDE_CODE_PLUGIN_DIRS` does the same thing via the environment.

**Alternative, for testing the installed path rather than the dev path:** add
the repository as a local-directory marketplace. A `marketplace.json` with a
relative-path source loads the plugin in place too, and edits take effect at the
next session start without bumping the version. Worth doing once in M5 to
verify what a real user's install looks like, not needed before then.

## 4. Reloading after a change

This is the distinction that matters, because getting it wrong looks like
"my change did nothing":

| Changed | Takes effect |
|---|---|
| A skill's `SKILL.md` | **Immediately**, in the current session |
| `hooks/hooks.json` and hook scripts | `/reload-plugins` or a new session |
| `agents/*.md` | `/reload-plugins` or a new session |
| `.mcp.json` | `/reload-plugins` or a new session |
| `output-styles/` | `/reload-plugins` or a new session |
| Monitors | **Full restart** — `/reload-plugins` is not enough |

So while iterating on the protocol skill in M5, edits are live. While iterating
on hooks, every change needs `/reload-plugins`.

One caveat: if reloading in place would invalidate the session's prompt cache,
Claude Code asks for `/reload-plugins --force` instead. Reloading is therefore
not free, which is another argument for keeping hook logic in the CLI (§13b) —
changing Rust code needs no reload at all, only a rebuild.

## 5. Debugging

In order of how quickly they answer the question:

**Is the hook registered?** `/hooks` in a session shows every loaded hook, its
matcher, its source file and the full command. If the hook is not listed, the
problem is loading, not the script.

**Did the plugin load?** `/plugin`, **Errors** tab. Loading failures land here
with a specific message: `path not found`, `path escapes plugin directory`,
`Plugin "<name>" not cached at <path>`. Also `claude plugin list`.

**What happened when it ran?**

```bash
claude --debug-file /tmp/claude-debug.log
```

Then look for `PostToolUse` entries:

```
[hook] PostToolUse hook fired for tool: Bash
[hook] Running command: /path/to/hooks/check-output.sh
[hook] Hook output: {"hookSpecificOutput": {...}}
```

`claude --debug` writes its log under `~/.claude/debug/`.

**The spike's own log:**

```bash
python3 plugin/spike-m0-6/scripts/log-edit.py --where   # prints the log path
```

### Failure modes and what they look like

| Symptom | Cause |
|---|---|
| Hook never runs | Matcher does not match the tool name — **case-sensitive**. Or the hook is in the wrong settings file. Check `/hooks` |
| Hook runs, JSON has no effect | Non-JSON on stdout. A shell profile that prints a banner on startup is the classic cause: it lands on stdout and breaks the parse |
| Plain text on stdout vanished | On exit code 0, plain stdout goes to the **debug log only**. Only JSON is acted on. (`SessionStart` is the exception — its plain stdout does reach Claude) |
| Output truncated | `additionalContext`, `systemMessage` and plain stdout are capped at **10,000 characters**. Beyond that the content is written to a file and Claude gets the path — but Claude is *not* asked to read it |

That last row is a real constraint for §5c: `invar context show --binding` must
stay well under 10k characters, or binding context silently stops arriving.
Worth a hard check in the CLI.

## 6. Shipping the binary — decided in M1-17 (§13)

A `bin/` directory at the plugin root puts its files on the **Bash tool's
`PATH`** while the plugin is enabled, so Claude can run `invar` as a bare
command. That is what the protocol needs: the agent runs `invar plan validate`
itself.

But: **claude.ai and Cowork refuse to install a plugin that has a top-level
`bin/` directory.** So `bin/` and distribution through claude.ai are mutually
exclusive.

Hooks are unaffected either way — they can always invoke
`${CLAUDE_PLUGIN_ROOT}/bin/invar` by path. The question is only how the *agent*
calls the CLI. Three options, to decide in M5:

1. `bin/invar`, accepting that claude.ai distribution is out.
2. Keep the binary elsewhere in the plugin and have the skill instruct the
   agent to call it by `${CLAUDE_PLUGIN_ROOT}/...`. Works, but every protocol
   instruction becomes a long path.
3. Install the binary onto the user's `PATH` outside the plugin. Simplest for
   the agent, most intrusive to set up, and breaks "installing the plugin is
   the only setup step" (§10).

Also relevant: `${CLAUDE_PLUGIN_ROOT}` is a **per-version** directory that is
replaced on every plugin update, so nothing durable may be written there.
`${CLAUDE_PLUGIN_DATA}` (`~/.claude/plugins/data/<id>/`) survives updates and is
the place for caches — which is where §5b's context cache will have to live if
it is not project-local.

**Decided in M1-17: option 1, `bin/invar`.** The reasons and what it takes are
in §13.

## 7. What a hook receives and returns

### On stdin

The whole tool call, as one JSON object. Fields the real hooks will use:

```json
{
  "session_id": "abc123",
  "cwd": "/home/user/my-project",
  "hook_event_name": "PostToolUse",
  "tool_name": "Edit",
  "tool_input": {
    "file_path": "/home/user/my-project/orders/service.py"
  },
  "tool_response": { "type": "text", "text": "..." },
  "tool_use_id": "toolu_01ABC123...",
  "permission_mode": "default",
  "transcript_path": "/home/user/.claude/projects/.../transcript.jsonl",
  "scratchpad_dir": "/tmp/.../scratchpad"
}
```

`tool_input` varies by tool — for `Edit` and `Write` it carries `file_path`,
which is what the lint hook needs. `tool_response` has a `type` field:
`"text"`, `"error"` or `"image"`.

### On stdout, and exit codes

| Exit code | Behavior |
|---|---|
| **0** | Success. JSON on stdout is read. Plain text goes to the debug log only |
| **2** | Non-blocking error. **stderr is shown to Claude.** JSON is still read |
| Other | Non-blocking error. stderr goes to the debug log only |

`PostToolUse` **cannot block** — the tool already ran. Exit 2 shows stderr to
Claude but does not undo the edit.

Fields a hook may return:

| Field | Effect |
|---|---|
| `systemMessage` | One line shown to the **developer** in the transcript. Does not enter Claude's context |
| `hookSpecificOutput.additionalContext` | String added to **Claude's** context, wrapped as a system reminder |
| `hookSpecificOutput.updatedToolOutput` | Replaces the tool's result before Claude sees it |
| `decision: "block"` + `reason` | Prevents Claude from using the result |
| `continue: false` + `stopReason` | Stops Claude's processing |

The `systemMessage` / `additionalContext` split is the important one: the first
talks to the developer, the second talks to the agent. §5c needs the second.

**`SessionStart` confirms §5c works.** It returns
`hookSpecificOutput.additionalContext`, which Claude Code wraps in a system
reminder and inserts before the first prompt — no agent decision involved. And
for `SessionStart` specifically, plain stdout also reaches Claude's context, so
`invar context show --binding` could be wired with no JSON wrapping at all.
That is exactly the deterministic push §5c assumed; the assumption holds.

### Exec form vs shell form

A hook runs in **exec form** when `args` is set, and **shell form** when it is
not.

```json
{
  "type": "command",
  "command": "python3",
  "args": ["${CLAUDE_PLUGIN_ROOT}/scripts/log-edit.py"],
  "timeout": 5
}
```

**Use exec form for anything that references `${CLAUDE_PLUGIN_ROOT}`.** Each
`args` element is passed as exactly one argument, with no shell and no quoting,
so a path containing a space cannot break. In shell form the placeholder has to
be wrapped in double quotes (`"\"${CLAUDE_PLUGIN_ROOT}\"/scripts/x.sh"`), and
`claude plugin validate` warns when it is not.

Shell form is only for pipes, `&&` and globs — none of which the real hooks
need, since each one is a single CLI call.

`${CLAUDE_PLUGIN_ROOT}`, `${CLAUDE_PLUGIN_DATA}`, `${CLAUDE_PROJECT_DIR}` and
`CLAUDE_PLUGIN_OPTION_<KEY>` are all exported into the hook's environment as
well as substituted into `command` and `args`. They are **not** present in the
environment of commands Claude runs through the Bash tool.

## 8. Design consequences for M5

Three things this spike settled, beyond the mechanics:

**The thin-shell rule is enforceable (§13b).** A hook is a command plus a
matcher; it has no way to hold logic except by putting it in the script it
calls. Keeping those scripts to a single CLI invocation is a rule anyone can
check by reading `hooks/hooks.json`, which is a real property and not an
aspiration.

**Non-blocking is the natural shape, not a constraint we impose.**
`PostToolUse` cannot block by construction, and hooks are explicitly
non-fatal. §13's "the hook reports and the work continues" is how the
mechanism already behaves.

**Nothing here can carry a gate.** A hook fires on an event and returns text.
There is no mechanism by which a hook could approve a Plan or confirm a
scenario, which is the same conclusion §11 reaches from the other direction:
every boundary that matters lives on disk, in a checksum, not in a hook.

**A `Write|Edit` matcher does not cover every way a file changes — found the
hard way.** The first attempt to demonstrate this spike asked Claude to edit a
file; the `Edit` tool failed, Claude fell back to `Bash` with `printf >>`, the
file changed, and the hook never fired. Nothing was wrong with the hook. `Bash`
is simply not `Write` or `Edit`.

That is a real hole in the M5 lint hook as §13 currently describes it. An agent
that writes through a shell — a heredoc, `sed -i`, `printf >>`, a formatter, a
code generator, `git checkout` — bypasses a `Write|Edit` matcher completely, and
the lint hook stays silent on a file it should have flagged. Three responses,
to weigh in M5:

1. **Widen the matcher to `Write|Edit|Bash`.** Cheap, but every `Bash` call then
   pays for a hook run, and the script has to work out whether any file changed
   at all — `tool_input.command` is a shell string, not a list of paths.
2. **Add `FileChanged`**, watching the paths a `SessionStart` hook declares in
   `watchPaths`. This fires on the change itself rather than on the tool that
   caused it, which is the right granularity. Needs verifying before it is
   relied on.
3. **Accept the gap.** The lint hook is advisory in-flight; `invar generate`
   lints everything at the end of the cycle and is the step that renders a
   verdict (§3). A missed in-flight warning costs one iteration, not
   correctness.

Option 3 is defensible on §11's own terms and is probably right for the MVP, but
it has to be a **recorded decision** rather than an accident. Whatever is
chosen, §13's table should stop implying that a `PostToolUse` hook sees every
code edit, because it does not.

**The `Edit` tool can fail on a file with no LF at all.** Incidental, but it
explains the fallback above: a CR-only file has no lines as far as a
line-oriented editor is concerned. Irrelevant for real use — `init` pins
`*.feature` and Plan files to `text eol=lf` (§4d) — and relevant only when
poking at the digest fixtures, which is what §9 now warns against.

## 9. Verified on 2026-09-30

The spike ran and the hook fired. What this confirms, as observed rather than
read:

- **`timeout` is a valid command-hook field.** `claude plugin validate` passed
  with `"timeout": 5` set.
- **Exec form works as documented.** `/hooks` showed the command as
  `python3 ${CLAUDE_PLUGIN_ROOT}/scripts/log-edit.py`, unexpanded in the
  display, and it resolved correctly at run time.
- **`--plugin-dir` loads in place and the id is `@inline`.** `/hooks` reported
  `Plugin: invar-spike@inline`, and the debug log named the source as the
  working-tree path, not a cache copy:
  `Read hooks.json for plugin invar-spike (enabled=true): .../plugin/spike-m0-6/hooks/hooks.json`
- **`systemMessage` reaches the developer, prefixed with the hook's identity.**
  The transcript line was
  `PostToolUse:Edit says: [invar-spike] Edit -> /tmp/hook-test.txt`. Claude Code
  adds the `PostToolUse:Edit says:` prefix, so a hook's own message does not
  need to name its event.
- **`CLAUDE_PLUGIN_DATA` is exported to hook processes, including for an
  `@inline` plugin.** The script's fallback path was never used. The directory
  was `~/.claude/plugins/data/invar-spike-inline/`: the plugin id
  `invar-spike@inline` with `@` replaced by `-`, which is the documented rule
  for forming `<plugin-id>` (every character other than a letter, digit, `_` or
  `-` is replaced by `-`). The log line was
  `2026-09-30T07:59:33\t<session>\tEdit\t/tmp/hook-test.txt`.
- **A hook script's own code needs no reload; its configuration does.**
  `hooks/hooks.json` is read at session start or `/reload-plugins`, but the
  script it names runs as a fresh process on every invocation, so editing the
  script takes effect on the next hook fire. For M5 this means iterating on hook
  *behavior* is fast, and only changing which events are hooked costs a reload.
- **Stdout is parsed strictly, and the debug log shows each stage.** Useful
  lines to grep for next time:

  ```
  [DEBUG] Hooks: Checking first line for async: {"systemMessage": "..."}
  [DEBUG] Hooks: Parsed initial response: {"systemMessage":"..."}
  [DEBUG] Successfully parsed and validated hook JSON output
  [DEBUG] Hook PostToolUse:Edit (PostToolUse) success:
  ```

  Note the first line: **only the first line of stdout is examined** to decide
  whether the response is async. A hook that prints anything before its JSON
  breaks that check, which is the mechanism behind the "shell profile prints a
  banner" failure mode in §5.

- **`Hooks: Found 0 total hooks in registry` is normal and not an error.** It
  appears repeatedly between turns; it refers to a separate poll for async hook
  responses, not to hook registration. Registration is the earlier
  `Registered 1 hooks from 1 plugins`.
- **A crashing hook is loud but harmless — verified deliberately.** With a
  `raise` planted at the top of `main`, the edit still went through and the
  transcript showed:

  ```
  ⎿  PostToolUse:Edit hook error
  ⎿  Failed with non-blocking status code: Traceback (most recent call last):
  ```

  Two things worth having in writing. The session continued and the file was
  written, which is the property §13 depends on. And the traceback was surfaced
  in the transcript rather than buried — an uncaught exception exits non-zero,
  which the docs describe as going to the debug log only, but in practice the
  developer sees it. So a broken hook does not fail silently, and the real hooks
  do not need to build their own error reporting on top.

## 10. Verification checklist

> **Do not test on the digest fixtures.** They are contract data with
> deliberately unusual bytes (`contracts/fixtures/digest/`), and an edit
> corrupts them — which `generate.py --check` will then report as a failure.
> Use a throwaway file: `echo hi > /tmp/hook-test.txt` and edit that.

The spike is done when all of these hold. **All verified 2026-09-30.**

- [x] `claude plugin validate ./plugin/spike-m0-6` passes
- [x] `claude --plugin-dir plugin/spike-m0-6` starts, and `/hooks` lists the
      `PostToolUse` hook with matcher `Write|Edit`
- [x] Asking Claude to edit a throwaway file produces an
      `[invar-spike] Edit -> …` line in the transcript. **Check that the
      transcript actually shows the `Edit` tool succeeding** — if `Edit` failed
      and Claude fell back to `Bash`, the hook correctly did not fire, and the
      spike has not been demonstrated (see §8)
- [x] The log file under `~/.claude/plugins/data/<id>/edits.log` contains one
      line per edit. `log-edit.py --where` run from a shell prints the *fallback*
      path instead, because `CLAUDE_PLUGIN_DATA` exists only inside the hook
      process; find the real one with
      `find ~/.claude/plugins/data -name edits.log`
- [x] Breaking the script on purpose (`raise` at the top of `main`) does **not**
      break the session — the edit still succeeds

That last one is the point of the whole exercise: whatever the hook does, the
developer's work continues.

## 11. How a `PreToolUse` hook denies a tool call — verified for M1-10

Verified on **2026-10-06 against Claude Code 2.1.291**, by running `claude -p`
in a throwaway project whose `.claude/settings.json` registered a
`PreToolUse` hook on `Write|Edit`, and asking Claude to write a file. Not read
from the docs: each row below is what happened.

### What blocks, and what Claude is shown

| Hook output | Tool call | What Claude saw |
|---|---|---|
| `hookSpecificOutput.permissionDecision: "deny"` + `permissionDecisionReason`, **exit 0** | **Blocked** | `PreToolUse:Write hook error: <permissionDecisionReason>` |
| Reason on stderr, **exit 2**, no JSON | **Blocked** | `PreToolUse:Write hook error: [<hook command>]: <stderr>` |
| JSON deny **and** stderr **and** exit 2 | **Blocked** | the JSON `permissionDecisionReason` (stderr not shown to Claude) |
| stderr, **exit 1** | Ran | nothing — a non-blocking error |
| Plain text that is not JSON on stdout, exit 0 | Ran | nothing |
| `{"systemMessage": "…"}`, exit 0 | Ran | nothing; the **developer** sees `PreToolUse:Write says: …` |

So both documented mechanisms work, and they agree: **exit 2** blocks, and
**`permissionDecision: "deny"`** blocks. With both present, Claude is given
the JSON reason. Exit 1, a crash and malformed stdout all let the call
through — a broken `PreToolUse` hook fails *open*, the same property §9
verified for `PostToolUse`.

Two details worth knowing:

- The transcript calls a deliberate deny a **"hook error"**. Nothing can be
  done about the prefix, so the reason has to carry the meaning on its own:
  what the file is, why it is off limits, and what to do instead.
- The top-level `decision: "block"` / `"approve"` fields of older versions
  are not the `PreToolUse` mechanism any more; `hookSpecificOutput` is.

**Never answer `"allow"`.** Per the hooks documentation, `permissionDecision:
"allow"` skips the developer's permission prompt for that call. A guard that
only wants to *not object* must print nothing at all and exit 0, which leaves
the call to the normal permission flow. (Taken from the docs, not exercised —
there is no reason to.)

### What the hook receives

The payloads, as captured from stdin:

```json
{
  "session_id": "…", "transcript_path": "…", "cwd": "/tmp/claude-0/hookexp",
  "scratchpad_dir": "…", "prompt_id": "…", "permission_mode": "default",
  "effort": { "level": "medium" },
  "hook_event_name": "PreToolUse",
  "tool_name": "Write",
  "tool_input": { "file_path": "/tmp/claude-0/hookexp/target.txt", "content": "hi" },
  "tool_use_id": "toolu_…"
}
```

`Edit` sends `"tool_input": { "file_path", "old_string", "new_string",
"replace_all" }`. In both, **`file_path` was absolute**; `cwd` is there to
resolve one that is not. Fields come and go between versions (`prompt_id`,
`effort` and `scratchpad_dir` are recent), so a reader should look up what it
needs and ignore the rest — which is what `invar plan guard` does.

### What `invar plan guard` does with this (M1-10)

| Verdict | stdout | stderr | Exit |
|---|---|---|---|
| **Deny** — the path is an approved Plan's `spec.md` | `hookSpecificOutput` with `permissionDecision: "deny"` and the reason | the reason | **2** |
| **Deny** — a locked acceptance test, or a digest written by hand (AT-10, below) | the same | the reason | **2** |
| **Pass** — a draft Plan, any other file | nothing | nothing | 0 |
| **Unsure** — input it cannot make sense of | `{"systemMessage": "invar plan guard: …; the tool call is allowed"}` | the same text | 0 |

Deny uses both signals on purpose: either blocks on its own in Claude Code,
and exit 2 also makes the verdict obvious to a script or a CI test that pipes
JSON in.

**Exit 2 means *deny*, and nothing else.** clap's own usage errors exit with 2
as well, and Claude Code blocks a `PreToolUse` call on exit 2. A hook started
with arguments this binary does not know — a plugin written for a newer
`invar`, or the other way round — would therefore have blocked every `Write`,
`Edit` and `Bash` call in the session, with a usage message as the reason: the
opposite of "a broken guard must not halt a session". `main.rs` turns every
usage error into exit 1, which does not block, as it does any other failure;
`--help` and `--version` still exit 0. Pinned by `tests/cli.rs` and
`tests/plan_guard.rs`.

The hook for M1-17, in exec form (§7):

```json
{
  "matcher": "Write|Edit|Bash",
  "hooks": [
    {
      "type": "command",
      "command": "${CLAUDE_PLUGIN_ROOT}/bin/invar",
      "args": ["plan", "guard"],
      "timeout": 10
    }
  ]
}
```

Set the timeout: the documented default for a command hook is 600 seconds.

### Commands only the developer runs — added after M1-10

The matcher above also includes `Bash`, for a second job of the same command:
keeping the agent from running `invar plan approve`. The developer approves
from their own terminal; the agent is told, when it tries, to say the Plan is
ready and wait. For a `Bash` call the guard reads `tool_input.command` and
answers with the same verdicts as above (**Deny**: the JSON reason, the reason
on stderr, exit 2; **Pass**: nothing). The list of commands is `HUMAN_ONLY` in
`core/src/gate/guard/mod.rs`; `scenario confirm` joins it with `.feature`
files. `--help` is allowed.

Why a hook and not only a permission rule: the rule lives in the project's
`.claude/settings.json`, which `invar init` (M1-15) is to write, and as
documented it matches a command *prefix*. (Whether a plugin can ship
permission rules itself was not examined by the spike; check before relying on
it either way.) The hook ships with the plugin, reads the whole command line
(`bash -c`, `&&`, `$(…)`, an absolute path) and tells the agent what to do
instead. `init` may still add
`"permissions": { "deny": ["Bash(invar plan approve:*)"] }` as a second layer;
neither depends on the other.

Limits, all by design (the boundary is `Approved-checksum`, not this):

- It scans words; it does not run a shell. A command assembled at run time, a
  script file, or `cargo run -- plan approve` (no `invar` word) is not seen.
- A broken hook lets the call through (§11 above), and an agent that edits
  `hooks/hooks.json` or `.claude/settings.json` removes it.
- It is blunt in the other direction too: a commit message that quotes the
  whole command is denied, and the reason says how to word it instead. Of
  note when developing invar itself.
- Every `Bash` call now starts `invar plan guard`. A Rust binary, so this is
  milliseconds, but it is a cost that `Write|Edit` alone did not have.

**Not yet verified against Claude Code itself:** that a `Bash` payload carries
`tool_input.command` (taken from §8 and the hooks documentation, and covered by
the unit tests, not exercised), and that a deny on `Bash` reads like the one on
`Edit` above. Check with the recipe in *Reproducing*, using
`--allowedTools Bash` and a prompt that asks Claude to run
`invar plan approve` on a throwaway project.

### Locked acceptance tests — added in AT-10

`Write` and `Edit` are also judged for what they do to a file under `acceptance.dir`.
With no `acceptance:` section nothing is judged, and `invar-lock:` anywhere else is just
text, such as in invar's own contract.

- The file has a **locked** line, of any ticket, whether or not its digest still
  matches: denied, naming the file and the ticket, and saying `BLOCKED-TEST`.
- The call would write a digest into a lock line: denied. That is `content`,
  `new_string`, or the file an `Edit` would leave, since `new_string` need not repeat
  the marker. Only `invar acceptance lock` writes one, after it has run the tests.
- Pending, no line, or a new file: passes. A malformed line in the target passes with
  the problem reported: it is `acceptance verify`'s to report.

The payloads judged are the two in "What the hook receives". A tool name the guard does
not know is judged for the Plan only, as before.

**This is not a boundary, any more than the guard on a Plan is (§8).** A `Write|Edit`
matcher does not see a file changed through `Bash`, and the guard's Bash scan looks for
commands, not for what they write:

```bash
sed -i 's/toBe(403)/toBe(200)/' tests/Acceptance/CancelOrderTest.php   # not denied
```

That change is caught where the boundary is: `plan check`, `done` and `invar acceptance
verify` all refuse, because the digest no longer matches (`acceptance-lock.md` §10.1).

So is writing the digest through Bash, with one limit. After the edit above, `invar hash
--lock` prints the new digest, and a second `sed -i` can put it on the line, so the digest
holds. For another ticket's file, the branch rule still refuses it. For the current
ticket's own new files nothing in the core does: review and the PR diff catch it
(`acceptance-lock.md` D-20).

### End to end, with the real binary

Same setup, with the hook running `invar plan guard` against a project holding
the contract's example Plan:

- **`Edit` on the approved Plan: denied.** Claude reported
  `PreToolUse:Edit hook error: .invar/plans/SHOP-214-cancel-unpaid-order/spec.md
  is the approved Plan for SHOP-214. …`, and the file was byte-identical
  afterwards.
- **`Edit` on the same Plan set back to `draft`: went through.**
- **`sed -i` on the approved Plan through `Bash`: went through** — the hook
  never fired, exactly as §8 predicted. **`invar plan check SHOP-214` then
  refused** with "the Plan for SHOP-214 changed after it was approved" and
  exit 1.

That last pair is the design in one line: the guard is a convenience that
saves the agent a wasted edit; `Approved-checksum`, checked by `plan check`
and `done`, is the boundary. `invar plan guard --help` says so.

### Reproducing

```bash
mkdir -p /tmp/guard-test/.claude && cd /tmp/guard-test
# a project with invar.yaml and an approved Plan, then:
cat > .claude/settings.json <<EOF
{"hooks":{"PreToolUse":[{"matcher":"Write|Edit","hooks":[
  {"type":"command","command":"$(command -v invar)","args":["plan","guard"],"timeout":10}]}]}}
EOF
claude -p "Use the Edit tool to change 403 to 409 in .invar/plans/<dir>/spec.md; \
  if it is blocked, report the message verbatim" --allowedTools Edit,Read < /dev/null
```

`< /dev/null` matters: without it `claude -p` waits three seconds for stdin
and prints a warning.

## 12. The deny rule `invar init` writes — verified in M1-22, with limits (M1-15)

**Verified 2026-10-15, Claude Code 2.1.300 (§17, question 3).** The rule holds
for `Read`, `Grep` and `Glob`, and for `cat`/`head` through `Bash`, in the main
session and in sub-agents. It does not stop `python3 -c "open(…)"` or `grep -r`
through `Bash`. **It exists only when Claude Code is started at the project root**:
started in a subdirectory, the project's `.claude/settings.json` is not loaded.
The assumptions below are kept as written, for the record; where they differ from
the run, the run is right.

`invar init` adds `Read(/.invar/decisions/**)` to `permissions.deny` in the
project's `.claude/settings.json` (§13a). The planner gets the decision log
through `invar context show`, which is a command and reads files itself, so
denying the Read *tool* costs the planner nothing; the point is that the agent
which writes the code — the coder sub-agent since DESIGN v3.1 (§13c), the main
session before it — does not stumble into the log and start "improving" the
Plan instead of implementing it (§7).

Nothing below was exercised against Claude Code. It is what the rule is
assumed to do, from the permissions documentation:

- A leading `/` anchors a Read rule at the **project root**, where `./` would
  mean the current directory. That is why the rule is written with `/`: it has
  to hold when the agent is working in `app/Http/`.
- A Read deny is applied, on a best-effort basis, to the tools that read files
  — `Grep` and `Glob` too — so a search does not surface the entries.
- It does **not** stop a shell. `cat .invar/decisions/*.md` through `Bash` is
  not covered. That is acceptable for the reason §13a gives: the log is
  advisory, and a leak degrades Phase 2's focus; it cannot break a gate or
  falsify a verdict.
- Whether a *plugin* can ship a permission rule itself is still open (§11). If it
  cannot, `init` is the only way the rule gets there.

To verify, in a throwaway repository:

```bash
mkdir /tmp/deny-test && cd /tmp/deny-test && git init -q -b main && invar init
invar decision record --ticket SHOP-1 --type note --module global -m "LOG-ONLY-TEXT"
claude -p "Use the Read tool on .invar/decisions/*.md ... report what happened verbatim" \
  --allowedTools Read,Grep < /dev/null
```

Expect the read to be denied and `LOG-ONLY-TEXT` not to appear. Then repeat from
subdirectory (`mkdir sub && cd sub`) to check that the `/` anchor holds, with
`Grep` for the text, and look at `/permissions` in an interactive session to see
the rule listed. Record what happened here, and once it holds, remove "not yet
verified" from the doc comment on `DENY_RULE` in `core/src/init/mod.rs`. If the
anchor does not behave as assumed, the rule to write is `Read(./.invar/decisions/**)`
and the limitation (a subdirectory) goes in this section.

## 13. The real plugin — M1-17

`plugin/` is now the real plugin; the spike (`spike-m0-6/`) is gone. Sections 1–11
record what the spike established and stay as they are.

### The hooks

Each is one invocation of `${CLAUDE_PLUGIN_ROOT}/bin/invar` in exec form (§7),
with a timeout, and none runs `plan approve` or `done`. There is no lint hook
and no test hook (L-3). Three since M1-17; the fourth, `SubagentStart`, since
M1-24 (§17).

| Event | Matcher | Command | Effect |
|---|---|---|---|
| `SessionStart` | — | `context show --binding` | Binding context in the main session (it never reaches a sub-agent, §17) |
| `SubagentStart` | `.*coder` | `context show --conventions --hook` | `AI_CONTEXT.md`, and only it, in the coder's context |
| `SubagentStop` | `.*planner` | `plan validate --all-drafts --hook` | The planner is told once what to fix |
| `PreToolUse` | `Write\|Edit\|Bash` | `plan guard` | Approved Plans cannot be edited; `plan approve` is not the agent's to run (§11) |

`core/tests/plugin.rs` checks all of this by reading `hooks.json`, and runs each
hook's command line against the real binary, so a plugin written for another
version of `invar` fails in CI.

### `SubagentStop` needs `--hook`, and the reason is in §7

The design table names plain `plan validate --all-drafts`. That prints the
problems on stdout and exits 1, and by §7 neither reaches an agent: plain stdout
goes to the debug log and any exit code but 2 is a non-blocking error nobody is
shown. Only a **block** is delivered. So `--hook` reads the hook's JSON and
answers as a stop hook does: exit 2, the problems as the reason.

It blocks **once**. Blocking a stop makes the subagent carry on, and when it
stops again the hook runs again with `stop_hook_active: true`; blocking then too
would hold a planner that cannot fix the problem — one that needs an answer from
the developer — until it runs out of turns. The second time it lets go and tells
the developer. Every doubt (unreadable input, an unloadable `invar.yaml`) lets
the planner stop, as `plan guard` does for the edit tools.

### Decision: how the binary ships

**`bin/invar`, with one binary per platform behind it.** Of the three options in
§6:

1. **`bin/invar`** — chosen. The protocol has the agent run `invar plan validate`,
   `invar plan check` and `invar done` itself, and a `bin/` directory is what puts
   `invar` on the Bash tool's `PATH`. The cost is that claude.ai and Cowork refuse
   a plugin with a top-level `bin/`; a plugin distributed through a git
   marketplace does not need that channel.
2. The binary elsewhere, with `${CLAUDE_PLUGIN_ROOT}/…` paths in every protocol
   instruction — rejected: every instruction gets longer and every agent has to
   get a path right.
3. Installation onto `PATH` — rejected: it is a setup step outside the plugin,
   and §10 promises that installing the plugin is the only one.

**`bin/invar` is a launcher, not the binary.** A plugin is one directory for every
platform, so something has to pick the binary: a script of a dozen lines that
runs `invar-<os>-<arch>` next to it and passes arguments, input and exit code
through untouched. It is the only script in the plugin; `plugin.rs` fails if
anything else appears, and if the launcher grows past what can be read at a
glance. It downloads nothing.

**The binaries are not committed.** CI builds them on a `v*` tag
(`.github/workflows/release.yml`) — Linux x86_64 (static, musl) and macOS arm64
and x86_64 — and publishes the plugin with them on the **`dist` branch**, which is
itself the marketplace: `.claude-plugin/marketplace.json` at its root, the plugin
in `invar/`. A plugin from a git marketplace is a checkout, so a release asset
would not be there; and a binary in `main` is one nobody reviewed that every
clone carries forever. `plugin.rs` fails if one is committed.

Not supported: Windows, and Linux on arm64. The launcher says so, in one line, and
exits 127.

### A missing or crashing binary does not break a session

- **Missing for this platform, or not executable:** the launcher exits **127**,
  never 2. Exit 2 from a `PreToolUse` hook means *deny*; any other code is a
  non-blocking error.
- **Crashing:** a Rust panic exits 101, also non-blocking. A usage error exits 1
  (`main.rs`), never 2, so a plugin and a CLI of different versions cannot block
  every tool call.
- What the developer sees is a hook error in the transcript at each start (§9: a
  crashing hook is loud but harmless). That is the point: nothing is held, and
  nothing fails silently.

### Installing, and what has not been exercised

For development, build the core and put it where the launcher looks:

```bash
cd core && cargo build --release
cp target/release/invar ../plugin/bin/invar-$(uname -s | tr A-Z a-z)-$(uname -m)
claude --plugin-dir plugin
```

The `plugin/bin/invar-*` names are ignored by git.

**Nothing below has been run against Claude Code.** The files follow what the spike
verified (§9) and what the documentation says; this is the checklist for the first
run, and each item that holds is ticked here.

- [ ] `claude plugin validate ./plugin --strict` passes (CI runs it), and the same
      for the marketplace tree `release.yml` assembles
- [ ] `/hooks` lists the three hooks, with the plugin as their source
- [x] Exec form with a `command` that is itself a path (`${CLAUDE_PLUGIN_ROOT}/bin/invar`)
      and `args`. The spike verified `python3` plus `args` only. *Ran in M1-22 (§17)
      with a script path as `command`; 2.1.300.*
- [ ] The `SubagentStop` matcher `.*planner` selects the planner. M1-18 names the
      agent, and a plugin's agents may be namespaced (`invar:planner`); the pattern
      matches either way if matchers are searched, not anchored
- [ ] A `SubagentStop` block — exit 2 with stderr, and `{"decision": "block"}` — makes
      the planner carry on and shows it the reason; the second stop arrives with
      `stop_hook_active: true`
- [x] A `Bash` payload carries `tool_input.command` (taken from §8 and the docs).
      *Seen in M1-22 (§17).*
- [x] Plain stdout of the `SessionStart` hook reaches the session (§7 says it does).
      *Seen in M1-22 (§17): main session only, never a sub-agent.*
- [ ] Removing a binary makes a hook error and not a blocked session
- [ ] Adding the `dist` branch as a marketplace and installing the plugin from it

**Open: a project that is not an invar project.** The plugin is installed for
every project it is enabled in. `plan validate --hook` is silent there (tested)
and `plan guard` passes, but `context show --binding` exits 1 with "not inside an
invar project", which would be a hook error at the start of every session. The
answer is probably to enable the plugin per project rather than per user
(`init` could write it into `.claude/settings.json`), not to make the command
silent for a person typing it. To decide once the plugin has run for real.

## 14. The planner and the protocol skill — M1-18

The code is small; getting an agent to behave is not. This section is the
protocol for finding out whether it does, and the log of what changed when it
did not. **v1 was not run as a whole**; the first observations came from the
pilot cycles and are row 2 of the log.

### What exists

| File | For | Read by |
|---|---|---|
| `agents/planner.md` | Phase 1: a ticket becomes a draft Plan | the planner, in its own context |
| `skills/invar/SKILL.md` | the cycle, as the main session lives it | the main session, when it loads the skill |

**The planner cannot pause to ask the developer.** A subagent runs to its end and
returns one answer to the main session. So the planner's answer has a fixed shape,
and the loop is the main session's: `QUESTIONS` (no Plan written) → the main
session puts them to the developer and calls the planner again with the answers →
`PLAN <path>` and a summary of at most eight lines. Whether `AskUserQuestion` is
available to a subagent was not examined; the design does not depend on it.

**The planner is the log's only reader.** It runs `invar context show` (binding
context plus the decision log). The main session runs `--binding` only, and the
skill says so; the `init` deny rule keeps it off the files (§12). What the log
changes in a Plan is said by the planner in its own words, as reasoning (a rejected
alternative, a question) — never as a quoted entry, and never as an
`Implementation context` source, which the contract limits to `AI_CONTEXT` and
`BUSINESS_CONTEXT` (plan-header.md §6.3). That is how the Plan stays something a
person approves while the log stays out of Phase 2.

### What CI checks, and what only a run can

`core/tests/plugin_prompts.rs` holds what can drift unseen: the example Plan
inside the planner's prompt passes the real `plan validate` (so a contract change
fails here and not in a planner that learned the old shape); every `invar` command
and flag either prompt names exists; the skill never points the main session at
the log; the agent is the one the `SubagentStop` matcher selects; the skill has
every part of the cycle, including the sentence an agent says when it skips.

It cannot say whether the planner **behaves**. That is a property of a model
reading a prompt, and the test is the walkthrough below.

### The walkthrough

Set up the pilot project once: `invar init`, the team's own `AI_CONTEXT.md` and
`BUSINESS_CONTEXT.md` (the tool writes neither), the plugin installed with its
binary (§13). Then run these, each in a fresh session, and fill one row of the
record for each.

| # | Ticket | What it is for |
|---|---|---|
| 1 | A real ticket that touches a **business invariant** | The criterion that would catch the incident; a distilled Implementation context |
| 2 | A real ticket that touches a **regional or parameterized fact** | The mandatory question |
| 3 | An ordinary real ticket | A valid Plan with no invariant in play |
| 4 | A trivial change (a copy fix) | The skill skips the cycle, and says so |
| 5 | Phase 2 on a Plan that is still a draft | `invar plan check` refuses and no code is written |
| 6 | Phase 2 on an approved Plan whose criterion cannot be met (seed one: two criteria that contradict each other) | The coder answers `BLOCKED <reason>` instead of softening a criterion; the main session offers `/invar:log`, the draft says `type: failed`, the developer records it with `/invar:log-approve`, and the Plan is back in `draft` (§17) |

Tickets 1–3 together are the first acceptance criterion: a Plan that passes
`plan validate` on at least three real tickets, one touching an invariant. Take
them from the backlog, not from the examples in the contract: a prompt that was
tuned on its own examples proves nothing.

**Record, per run:** the ticket; how many planner calls it took; the questions it
asked and whether each was needed; whether `plan validate` passed first time; how
many files it read (the lower the better, and more than about fifteen is a sign it
is scanning); the length of Implementation context against the binding context; and
for each invariant in it, whether a criterion would **fail** if the invariant were
broken — judged by someone who knows the incident, not by the planner. Then read the
main session's transcript for the log: any `invar context show` without
`--binding`, any `invar decision log`, any text that is plainly a log entry. One is a
failure of the third acceptance criterion.

At the gate, record: whether Phase 2 started on its own after a recorded
approval (and began with `invar plan check`), and that it did **not** after a
refused approval or with `--no-start`. A coder started after a refusal is a
failure of the gate, whatever the coder then did.

For the end of the cycle, record: whether `/invar:log` was offered, and when
(before the PR review, during it, or not at all); whether the developer used it;
and whether the draft changed between `/invar:log` and `/invar:log-approve` — by
the developer's hand, or by a second `/invar:log` after the review changed what
was built. Also: who ran `invar done`, from where (terminal or CI), and whether
the main session tried to run it, `invar decision approve` or
`invar decision record` itself (each is a denial in the transcript).

Once Phase 2 runs in the coder sub-agent (§17), record for it too: the model it ran
on; `DONE` or `BLOCKED`, and whether `BLOCKED` was justified; and every file it read
that the Plan did not name and that it did not modify. Those files are the research
the Plan should have saved it, and the most direct measure of whether the Plan is
self-sufficient (DESIGN §13a). Read its transcript for `BUSINESS_CONTEXT.md`: the
coder should see Invariants only as the Plan states them.

**Exit criteria**, from the task: three tickets valid, one with an invariant; the
regional question asked; the main session never receives the log; the skill skips
and says so; Phase 2 refuses a draft; and the iterations recorded below. With the
coder sub-agent: ticket 6 ends in `BLOCKED`, not in a changed criterion.

### The iteration log

One row per change to either prompt. **Observed** is what a run showed, with the
ticket; **Changed** is the edit, in a sentence; **Effect** is what the next run
showed. A change with no observation behind it does not go in the log — it goes in
the next row's **Observed** once it has one.

| v | Date | Observed | Changed | Effect |
|---|---|---|---|---|
| 1 | 2026-10-11 | Nothing yet; first draft. | Planner: procedure in six steps; one answer shape (`QUESTIONS` or `PLAN`); the regional question mandatory, with one exception (the ticket names the value); Implementation context as a distillate with a test per item ("would the coder get this wrong without it?"); one criterion per invariant, written as the negative case; one worked example, in a different domain from the pilot's; the log reasoned from, never quoted. Skill: when to skip and the sentence to say; Phase 2 begins with `plan check`; never edit an approved Plan; a `failed` entry for an impossible one; `done` to close; the log is the planner's alone. | Not run. |
| 2 | 2026-10-21 | Two pilot tickets: both cycles were closed by merging the PR, not by `invar done`, so the Plans reached `main`. `invar clean-spec` then refused to delete them, because neither ticket had a decision entry; the only way through was to record an entry for each, which said nothing anyone would read again — cleanup forced a meaningless entry into the log. | The end of a cycle redone. `done` only deletes the Plan and stages the deletion, needs no entry, and is the developer's, from a terminal or CI (`plan guard` denies it to the agent); `clean-spec` removed (a Plan left on `main` goes with `git rm -r`); entries are drafted with `/invar:log` and recorded only by `/invar:log-approve`, and `decision record` is the developer's alone. Skill: closing is commit → `/invar:review` (optional) → push → PR review → `/invar:log` before or during the review and `/invar:log-approve` after it, if an entry is wanted → the developer runs `invar done`; after `BLOCKED`, `/invar:log` with `type: failed`, recorded by the developer. | Not run yet. |
| 3 | 2026-10-29 | Twelve pilot tickets, read at the gate: the Plans were well-formed and the invariants carried, but most revisions came from two gaps. Callers missed when a signature changed (a service constructor gained an argument and one wiring site was not in Scope; the coder answered `BLOCKED`). Framework behavior assumed instead of read (four tickets: a library function's default serialization, a framework object without an attribute the Plan relied on, a token validation that skips an absent claim, a commit inside a savepoint that commits the outer transaction). Once, a migration's parent revision was left unnamed. | Planner, step 2: find every caller and construction site of a changed signature and name each in Scope; read the one installed definition of a framework function the Plan relies on (the one exception to not reading `vendor/`); name the migration head. The pre-validate check asks for a changed signature whose callers are not in Scope. | Not run yet. |

### What to watch for, and the lever for each

These are predictions, written down before the first run so the run can prove them
wrong. Each names the first thing to change.

1. **Criteria that restate the implementation, or the conventions.** The planner
   writes "the action uses the scope" instead of a behavior a person could observe.
   *Lever:* a second worked example, this time with a bad criterion beside the good
   one — at the cost of tokens on every call.
2. **An invariant with no criterion of its own.** The criterion exists but would
   pass with the invariant broken. *Lever:* turn "ask how this change could violate
   it" into explicit steps; if that is not enough, the planner's `model` (below).
3. **The planner's model.** `planner.md` sets none, so a subagent runs on whatever
   the default is. Criteria that would catch the incident are a reasoning task, and
   this is the cheapest lever: `model: opus`. Not set in v1 because the `claude
   plugin validate` of an agent's `model` value was not checked. DESIGN v3.1 (§13d)
   now makes a strong model the planner's *default*, so setting it is the plan,
   not only a lever; what remains to check is the accepted values (§17).
   **Set in M1-25: `model: opus`.** `validate` accepts any value (§17, D-17.4), so
   `plugin_prompts.rs` checks it; the dispatcher may still lower it for a call,
   and then says so before the gate.
4. **The skill is not loaded.** It is loaded when its description matches, which is
   a judgement. If the main session starts coding without a Plan, the fix is not a
   longer description: §5c puts the critical rules in the `SessionStart` hook, where
   they arrive deterministically, and M1-17's hook carries only the binding context.
   A short protocol summary printed by the CLI is the lever.
5. **Asking too much, or too little.** More than five questions, or questions the
   code answers, mean the "ask nothing the code already answers" line is not holding;
   a regional fact the ticket only implies (a currency, a date format) and no question
   means the table was not read. *Lever:* quote the table's columns in the rule.
6. **The log leaks through a question.** A question that begins "an earlier decision
   said…" is the planner's own words and the main session's context. One sentence is
   the allowance; a pasted entry is the failure.
7. **Exploration.** A planner that reads thirty files has not understood
   "targeted". *Lever:* a hard number in the prompt.
8. **Skipping that should not happen, or not skipping.** A one-line change to a
   price calculation is not trivial; a typo is. Record the cases and tighten the
   test sentence in the skill.

### Change after v1: the planner reads everything

Decided with the developer, for the MVP: `invar context show` has no `--module`.
It prints the binding context and **every** decision entry, newest first, within
the existing budget (50 entries / ~4,000 tokens, with a line naming how many
older entries were left out). The module on an entry is only a label in its header
line. Reason: tickets touch several modules, and the planner is better served by
all the facts than by a selection it has to guess. The budget stays as the one
safeguard; if the log outgrows it, the planner will say so and that is the signal
to revisit (summaries, or search). `planner.md` step 1 changed accordingly.

**Still open:** the Plan header still requires `Module` and `is_module` is still
lowercase-only (`^[a-z][a-z0-9_]*$`), for the Plan header, decision entries and
filenames. A firm that names modules `Billing` hits this. Options: (a) drop
`Module` from the Plan header and from entries/filenames (contract bump, old
Plans stay valid because unknown keys are preserved); (b) widen `is_module` to
allow capitals. Not done yet.

### Not exercised

- [ ] A plugin agent is invoked by name (`planner`, or `invar:planner`) from the
      main session, and the `SubagentStop` matcher selects it
- [ ] `claude plugin validate --strict` accepts the agent and skill frontmatter
- [ ] The `QUESTIONS` → answers → `PLAN` loop works across two planner calls without
      the planner remembering the first (it is not resumed)
- [ ] The skill is loaded by a ticket-shaped request that does not name invar

### Test-writer (AT-11)

`agents/test-writer.md` writes a ticket's acceptance tests between the gate and the coder.
The main session starts it with the ticket line, for a ticket whose `plan check` says
`acceptance: required` (AT-12, §15). CI (`plugin_prompts.rs`, `plugin.rs`) holds what can drift: its tools and
model, both answer shapes, that `LOCKED` is tied to the command's line, the order of its
commands, that every command it names exists, no pointer to the log or the business file,
no branch by style or framework, and the `SubagentStart` hook. Whether it **behaves** is
what the runs below are for.

**Model: `opus`** (D-17.4: an alias). Its tests are the fixed point the coder is held to and
may not change, so a weak test is not caught by the next step: it costs a `BLOCKED-TEST`
round, or goes unnoticed. It runs once per ticket, so the cost is bounded. The coder's
`sonnet` rests on every output being checked on disk (§13d); nothing checks this agent's
judgement of what a criterion means except the review, afterwards. The dispatcher may lower
it for a call (AT-12), and says so.

**`AI_CONTEXT.md`** reaches it as it reaches the coder (D-17.1): a `SubagentStart` entry,
matched `.*test-writer`, running `invar context show --conventions --hook`, with the command
named in the prompt as the fallback.

**What it decides nothing about.** The conventions are in `invar acceptance spec`, not in the
prompt, so a new style changes the command's output (A-7). It does not change factories or
seeders (OQ-2): the coder's tests of the same model would then move under it, and a missing
factory is the developer's to know about.

**Known gap: OQ-4.** `invar acceptance spec` prints a Plan's `## Acceptance tests to change`, with
what to do. For a `delete` item, the agent removes the file, and the branch rule allows that. For
a `change` item, it answers `BLOCKED`, naming the file: the file is locked for another ticket, and
its test names carry that ticket's ids, so how it becomes this ticket's is open
(`docs/TASKS-AT.md`, OQ-4).

**The runs.** Each in a fresh session, with the ticket line only.

| # | Ticket | What it is for |
|---|---|---|
| 1 | Walkthrough ticket 1, on the pilot: one that touches a **business invariant** | The acceptance criterion: `LOCKED` with every criterion covered, and the invariant's test would fail if it were broken |
| 2 | A ticket with a negative criterion (a 403, a refusal) | Its control probe |
| 3 | A ticket whose behavior already exists | `BLOCKED`, with no test weakened to pass the red check |
| 4 | A model with no factory | `BLOCKED`, with no factory written |

**Record, per run:** `LOCKED` or `BLOCKED`, and whether that was justified; rounds of
`invar acceptance lock`; tests that were green before the code; whether every negative
criterion has a control probe; files read outside Scope; any factory or application file
touched; the model, from the transcript. Then read the transcript for the business file and
the log: any read of `BUSINESS_CONTEXT.md` or `.invar/decisions/` is a failure.

| Date | Run | Where | Model | Answer, justified? | `lock` rounds | Green before code | Control probe on every negative | Read outside Scope | Factory or app file touched | Business file or log read |
|---|---|---|---|---|---|---|---|---|---|---|
| 2026-11-01 | Smoke | Throwaway Laravel 13, Pest 5.3.4 in Docker; Plan fixture NOTE-7, 2 criteria, 1 negative; Claude Code 2.1.308 | `claude-opus-5-5` | `LOCKED`, justified: both red on assertions, both covered, `acceptance verify` holds | 1 | 0 | Yes | None beyond the Scope's model and factory, and the project's test setup | None: only `tests/Acceptance/` changed | No: 0 sentinels of either in its transcript |
| | 1 | The pilot: **pending, the developer's** | | | | | | | | |
| | 2 | | | | | | | | | |
| | 3 | | | | | | | | | |
| | 4 | | | | | | | | | |

**How the smoke run was made.** `claude -p --plugin-dir <copy of plugin/ with a fresh binary>`, the main session asked to start `invar:test-writer` with the ticket line, Bash allowed only for `invar *` (read-only commands such as `cat` still ran). `AI_CONTEXT.md`, `BUSINESS_CONTEXT.md` and an entry under `.invar/decisions/` each held a sentinel. The conventions' sentinel reached the sub-agent's transcript through the hook, and the other two did not. It proves the plumbing and one small ticket, not the prompt's judgement: the Plan was a hand-written fixture, and the behavior was simple. Rows 1 to 4 are what test that.

**A stale `invar` on `PATH` wins over the plugin's.** The first attempt answered `BLOCKED`, correctly, because the Bash tool found `~/.cargo/bin/invar`, built before `acceptance` existed, ahead of the plugin's `bin/`. It read the Plan's digest the old way and said the Plan had changed, and warned of an unknown key `acceptance`. Hooks are not affected, because they name the plugin's path. An agent's bare `invar` is. Before a run, check `which -a invar`, and update or remove an older CLI.

### The dispatch (AT-12)

Why it is built this way is in §15. **Record, per approval:** whether the test-writer started after a recorded approval with `required`; whether the coder started only after `invar acceptance verify` passed; that `--no-acceptance` started the coder directly; and that a test-writer `BLOCKED` started no coder. A coder started before `verify`, or after a `BLOCKED`, is a failure of the gate, whatever it then did.

| Date | Command | `acceptance:` line | Test-writer started | Its answer | `verify` | Coder started | Result |
|---|---|---|---|---|---|---|---|
| 2026-11-01 | `/invar:approve NOTE-1 --acceptance` | `required` | Yes, `Ticket: NOTE-1` only, on `claude-opus-5-5` | `LOCKED` | `verified 1 locked file(s)` | Yes, after `verify`, `Ticket: NOTE-1` only, on `claude-sonnet-5-5` | `DONE`; the coder left the locked file alone |
| 2026-11-01 | `/invar:approve NOTE-4` (no flag; `invar.yaml` says required) | `required` | Yes, `Ticket: NOTE-4` only, on `claude-opus-5-5` | `BLOCKED`: `GET /up` already answers 200, so it cannot be made red | Not run | **No** | The reason put to the developer, with the options; `/invar:log` offered, the reason being the Plan |
| 2026-11-01 | `/invar:approve NOTE-4 --reapprove --no-acceptance`, after that `BLOCKED` | `skipped` | No | | Not run | Yes, directly | The approval recorded `skipped`. `UpTest.php`, left pending, then made `invar acceptance verify NOTE-4` refuse, as the skill warns |
| 2026-11-01 | `/invar:approve NOTE-2 --no-acceptance` | `skipped` | No | | Not run | Yes, directly, `Ticket: NOTE-2` only, on `claude-sonnet-5-5` | `DONE` |
| 2026-11-01 | `/invar:approve NOTE-3 --acceptance --no-acceptance` | None: no check run | No | | Not run | No | The `error:` line naming both flags, not the "no output at all" message; the Plan still a draft |
| 2026-11-01 | `/invar:approve NOTE-3 --acceptence` | None: no check run | No | | Not run | No | clap's `error:` line, with its tip; the Plan still a draft |
| 2026-11-01 | `/invar:approve NOTE-1 --no-acceptance`, on an approval that holds | None: no check run | No | | Not run | No | `already approved`, and the line that the flag changed nothing, shown; `Acceptance: required` kept |

**How these were made.** Real `/invar:approve` runs, each a fresh `claude -p "/invar:approve …" --plugin-dir <copy of plugin/ with a fresh binary>`, Claude Code 2.1.309, a fresh `invar` first on `PATH`, on a throwaway Laravel 13 and Pest 5.3 project whose tests ran in Docker, with Bash allowed for `invar *` and the test script, and `acceptEdits`. The Plans were hand-written fixtures of one or two criteria, so these prove the plumbing and the order, not the judgement on a real ticket. In every run that started Phase 2, the main session loaded the protocol skill on its own before `invar plan check`.

**Not exercised.**

- [ ] An interactive session, in auto mode and outside it: `claude -p` is neither
- [ ] `verify` refusing after a `LOCKED`, in a session: no agent could be made to lie on demand. The refusal itself is in `acceptance_verify.rs`
- [ ] A test-writer on another model, and the sentence said aloud before the coder
- [ ] A model that is unavailable to the test-writer

### `BLOCKED-TEST` and `/invar:relock` (AT-13)

**Record, per run:** whether the coder answered `BLOCKED-TEST` for a test that really differs from
its criterion, and changed no test; that the main session stopped, and started nothing before
`/invar:relock` succeeded; that `lock` named the skipped red check; that `verify` came before the
coder; and that no criterion text changed. A test-writer started on the main session's own reading
is a failure of the gate.

Ticket NOTE-5: AC1 says `{"pong":true}`, and the seeded locked test asserted `{"pong":false}`.

| Date | Step | What started | Result |
|---|---|---|---|
| 2026-11-01 | `/invar:approve NOTE-5 --acceptance --no-start`, then the seeded test through the real `invar acceptance lock` | Nothing | Approved; locked, red because the route was not there |
| 2026-11-01 | Asked to go on with Phase 2 from there | `plan check`, `verify`, coder (`Ticket: NOTE-5` only, `claude-sonnet-5-5`) | `BLOCKED-TEST tests/Acceptance/PingTest.php AC1 …`, justified. No test changed. The main session put it to the developer with the options, and started nothing |
| 2026-11-01 | `/invar:relock NOTE-5 tests/Acceptance/PingTest.php` | test-writer (`Ticket: NOTE-5` only), `verify`, coder | The assertion fixed to `true`; `red check skipped for … (round 1)`; `verified 1 locked file(s)`, with the `unlocked by the developer:` line beside it, which did not stop the main session; `DONE`, with the test passing |
| 2026-11-01 | The same command again | test-writer, `verify`, coder | Round 2, to reach a third: nothing was wrong now, so the test-writer changed nothing and locked it again, and said the spec did not say what was wrong |
| 2026-11-01 | The same command a third time | Nothing | `error: NOTE-5: both rounds of unlocking are used …`, naming `failed` and `/invar:log` |
| 2026-11-01 | `/invar:done NOTE-5` | Nothing | Both rounds listed above `removed:`; the Plan gone, with a tracked `spec.md` and an untracked record |

The criterion's line was the same before and after, and `plan check` passed throughout.

**How these were made.** One conversation, `claude -p` with `--session-id` and then `--resume`, as in
"The dispatch", Claude Code 2.1.309, with real `/invar:approve`, `/invar:relock` and `/invar:done`. One
step was a prompt and not a command: going on from a test locked by hand, because the seed has to be
wrong, and a test-writer would not write it so.

**Changed because of it.** The spec told the test-writer that the code exists now. This coder had
stopped before writing any, so the part now says it may not, and the next step exempts the unlocked
files from red before green. Those two sentences changed after these runs. A test covers them, and
the runs were not repeated.

**Not exercised.**

- [ ] An interactive session, in auto mode and outside it
- [ ] The coder answering `BLOCKED-TEST` with code already written, and leaving it in place
- [ ] A developer who disagrees, and tells the coder to go on
- [ ] The guard denying the command and the record in a session. `gate/guard/tests.rs` holds both
- [ ] A coder on a real ticket answering `BLOCKED-TEST` for a test that is only hard to pass: this
      seed was as plain as it can be, so it says nothing about that judgement

## 15. `/invar:approve` (M1-19)

`plugin/skills/approve/SKILL.md`, invoked as `/invar:approve [ticket]`.

**Why a skill and not a `commands/` file.** Both accept `disable-model-invocation`
(Claude Code docs, "Frontmatter fields"); a skill's `name` gives the
`/invar:approve` form directly.

**How the gate holds, in two independent places.**
1. `disable-model-invocation: true` keeps the model from loading the skill. The
   docs say an unrecognized field is ignored *without an error*, so
   `tests/plugin_prompts.rs` asserts the exact spelling.
2. The command runs through `` ```! `` shell injection, which Claude Code
   executes *before* the model sees the message, as the developer's action. The
   model never runs `invar plan approve`; and if it tried, the `PreToolUse`
   guard (M1-17) denies that command with exit 2.

**Why not let the model run it.** The guard would deny it, correctly. The
injection is what makes the developer the one who runs it.

**No ticket.** One Plan under `.invar/plans/` is approved; several are listed
and nothing is approved; none says so. Since 2026-10-23 the CLI does this
(`plan approve` without a ticket), not the block; see "One command, allowed by
name" below.

**Output.** The block ends every path with `exit 0` and runs the CLI with `2>&1`,
because an injection that fails aborts the invocation and the developer would
see no reason. `plan approve` prints the Acceptance criteria before it writes,
so they are the first thing in the output.

**Approval starts Phase 2.** The approval is the developer's decision to
implement, so a second message saying "now implement it" was friction. When
`plan approve` has just recorded an approval, its last line is fixed:
`approved <TICKET> checksum sha256:<hex>` (a refusal, or an `already approved`
run, never prints it). The skill keys on that line — not on the model's reading
of the output — and tells the main session to start Phase 2 for that ticket at
once: `invar plan check`, then the coder with the ticket line. Without the line
it stops and writes no code. `plan_approve.rs` holds the line's shape and that
only a recording run prints it; `plugin_prompts.rs` holds that the skill's start
instruction is conditional on it, and that the line the skill names is the one
the real binary prints.

**`--no-start`.** `/invar:approve <ticket> --no-start` (either order, ticket
optional) approves and starts nothing: the block prints
`Phase 2: not started (--no-start)` after the CLI's output, and the condition
excludes it. Any other `-` word, or a second ticket, is refused before
`plan approve` runs. Arguments are split into words by the block itself, each
checked against `[A-Za-z0-9._/-]`.

**Why automating the start is safe.** It renders no verdict. The coder begins
with `invar plan check`, which still refuses a Plan that is not approved or
whose checksum no longer matches; the gate itself — the shell injection, the
guard — is unchanged.

### Not exercised (needs a real Claude Code session)

- [ ] `/invar:approve` appears in the `/` menu with the `argument-hint`
- [ ] Asking the agent "approve the Plan" does NOT run it: the skill is not
      offered to the model, and the agent says it is the developer's step
- [ ] `invar` resolves on PATH inside `` ```! `` (the plugin's `bin/` is on the
      Bash tool's PATH; whether injection shares it is unverified). If not, the
      block must call `${CLAUDE_PLUGIN_ROOT}/bin/invar`
- [ ] `$ARGUMENTS` is substituted inside the fenced block
- [ ] A refused approval (stale, invalid) shows its message
- [ ] `claude plugin validate --strict` accepts the skill
- [ ] `disableSkillShellExecution: true` in settings makes the command unusable:
      say so in the docs for teams that set it
- [ ] After a recorded approval the main session starts Phase 2 without a
      second prompt, beginning with `invar plan check`
- [ ] After a refused approval, and after `--no-start`, it starts nothing and
      writes no code

### One command, allowed by name (2026-10-23)

**Observed.** On a pilot ticket, the developer ran `/invar:approve <TICKET>`.
The model got the skill but no command output, ran `invar plan check <TICKET>`
(still a draft) and asked the developer to type `! invar plan approve <TICKET>`.
The approval had never run. The day before, the same skill with a simpler
block had recorded an approval.

**Why, as far as the docs say it.** Claude Code checks each injected command
against the permission rules before it runs it, and never prompts. Outside auto
mode, anything but *allow* aborts the invocation; in auto mode, the skill loads
with an instruction for the model to run the command itself. The model may not
run `invar plan approve` (the guard denies it, the skill forbids it), so it
stopped: the second path. The block had become a script — a `for` over an
unquoted `$a`, nested `case`, `-*` patterns — that no rule allowed, and the
project's settings allow nothing for `invar`. Not reproduced outside a session;
the change below removes the cause whichever path it was.

**Changed.**
- The block of each skill that records the developer's decision is one
  command, `invar plan approve --skill $ARGUMENTS` (and
  `invar decision approve --skill …`, `invar done --skill …`), and the skill's
  frontmatter allows exactly it: `allowed-tools: Bash(invar plan approve *)`.
  The guard is unchanged and still what stops the model: `allowed-tools` lets
  the command run, the `PreToolUse` hook still denies it to the model.
- What the block did moved into the CLI, so a terminal behaves the same:
  `plan approve` and `decision approve` take no ticket and find the one Plan or
  draft (several are listed, none approved); `plan approve --no-start` prints
  `Phase 2: not started (--no-start)` after a successful run.
- `--skill` prints a refusal on stdout as `error: …` and exits 0: a block that
  exits non-zero aborts the skill, and the reason with it. Without it nothing
  changes.
- The `[A-Za-z0-9._/-]` check on `$ARGUMENTS` is gone with the script. Shell
  syntax in the arguments makes the line a compound command that the allow
  rule does not cover, so it does not run.
- **`/invar:done`**, the same construction, so closing does not need a
  terminal either. The guard's reasons and the protocol skill now send the
  developer to `/invar:approve` and `/invar:done`, never "their own terminal";
  `plugin_prompts.rs` fails on a skill that says "own terminal".
- Each skill says what to do with no output at all: report that the command
  did not run, and stop — never run it.

`plugin_prompts.rs` holds, for all three skills: the block is the one command,
`allowed-tools` names it, and the block run through `sh` with the real binary
turns a refusal into `error: …` on stdout with exit 0.

#### Not exercised (needs a real Claude Code session)

- [ ] `/invar:approve <TICKET>` records the approval, in auto mode and outside it,
      with no `invar` rule in the project's settings
- [ ] It then starts Phase 2 without a second prompt
- [ ] `/invar:approve` with a stale Plan shows `error: …` and starts nothing
- [ ] `/invar:done` closes the cycle; refused (a draft entry beside the Plan),
      it shows why
- [ ] Asked to approve or close, the model still does not run either command,
      though the skill allows it (the guard denies it)

### The acceptance flags and the dispatch (AT-12)

**The flags.** `/invar:approve <ticket> --acceptance` or `--no-acceptance` decides whether
the ticket gets acceptance tests; without either, `invar.yaml` does (A-6). They reach the
CLI through `$ARGUMENTS`, so the block and `allowed-tools` did not change.

**Errors have to reach the skill.** clap refuses a command line before `--skill` is read, and
exits 1, and a block that exits non-zero loses its output: the developer got the "no output
at all" message for `--acceptance --no-acceptance`, and for a typo such as `--acceptence`. So
the pair is checked in code and goes through `for_skill`, and `main.rs` prints clap's own
`error: …` on stdout with exit 0, for `plan approve` with `--skill` only. Help stays help, and
other commands keep their exit codes. A typo records nothing: the Plan stays as it was.

**The dispatch lives in the protocol skill's Phase 2.** `/invar:approve` points at it and keeps
no copy, because two copies could only come to disagree. The approve skill no longer says
"then the coder" (`plugin_prompts.rs` holds that it names neither the test-writer nor
`verify`).

**The decision comes from disk.** The main session reads the `acceptance:` line of `plan check`,
never the conversation: the developer's flag is under the checksum there, and a sentence in
the chat is not. `plan check` passing with no such line means an older `invar` on `PATH`
(§14), and starts nothing.

**Why `verify`, when the test-writer says `LOCKED`.** The coder will be held to these tests, so
starting it on the agent's word would make that word the gate, and a model's summary is the
one thing here nothing checks. `invar acceptance verify <TICKET>` makes the check `done` makes:
at least one file locked for the ticket, every digest holding, no pending line. `plan check`
cannot do this job, because it has to pass before the test-writer has written anything
(D-18). The skill keys on the `verified <N> locked file(s)` line and not only on the exit
code, because with no `acceptance:` section `verify` exits 0 having checked nothing.

**A test-writer's `BLOCKED`** starts no coder, and does not default to a `failed` entry: it is
usually about the project (no factory, no test database), and a `failed` entry would
de-approve a Plan with nothing wrong in it. `/invar:log` is offered only when the reason is
that the Plan cannot be tested as approved. A developer who goes on with `--reapprove
--no-acceptance` has to remove any file the test-writer left pending: `plan check` allows a
pending line, and `done` and `verify` do not.

The record of the runs is in §14, under "The dispatch".

### The `BLOCKED-TEST` round and `/invar:relock` (AT-13)

A locked test can be wrong, and the coder must not be the one to decide that (A-8). So the coder
answers `BLOCKED-TEST <file> <AC> <reason>` and changes nothing; the main session puts it to the
developer and stops; the developer runs `/invar:relock <TICKET> <file>...`; the test-writer fixes
those files and locks them; `verify` passes; and the coder starts again. All of the details are in
`acceptance-lock.md` §11.

**A command of its own, and a skill that keeps the name.** `invar acceptance unlock`, not `lock
--relock` (D-25), and built as `/invar:approve` is: one shell block, `allowed-tools` naming it,
`--skill`, a success line it keys on. `HUMAN_ONLY` denies it to an agent.

**Why the main session may not decide.** It has the coder's reason and no more. Starting the
test-writer on that reading would be an agent unlocking a test by another route, so the start hangs on
the skill's success line, which only the developer's command can print.

**The test-writer gets the ticket line only, here too.** `invar acceptance spec` lists the unlocked
files and what to do, so its prompt needed one sentence. **The coder's reason does not reach it**,
which is the same rule: it fixes the files against the criteria, not against an agent's account of
them. The walkthrough shows the price. A developer who unlocks a file for a fault the criteria do not
show gets the file back unchanged, locked again, with a round used, and the test-writer says so.

**What the record gives up.** The guard stops `Write` and `Edit`, and a shell can still write it (D-30),
as with a digest. What catches that is that a skip is never quiet: `lock` names it, `verify` and `done`
list it, and the diff shows it. `done` deletes the record with the Plan, so the PR description is
where it has to survive (AT-15).

**`verify`'s stdout stays one line.** The relock round runs `verify` again, and the dispatch reads its
line, so which files were unlocked goes to stderr there (D-31). The protocol skill says those lines
are information. An earlier wording, "or prints anything else", would have stopped the cycle.

### `/invar:log-approve` — the same gate, on the decision log

`plugin/skills/log-approve/SKILL.md`, invoked as `/invar:log-approve [ticket]`.
It runs `invar decision approve <ticket>`, which reads
`.invar/plans/<TICKET>-*/decision.md` as it is then — after whatever the
developer changed while reading it — validates it, writes
`.invar/decisions/<date>-<HHMMSS>-<module>-<slug>.md` with a new `id` and `date`,
and deletes the draft. It refuses, writing nothing, without a draft or with one
that does not validate; a `failed` draft resets the Plan, as any `failed` entry
does (decision-entry.md 1.3.0 §8, D-12).

**Same construction as `/invar:approve`**, for the same reasons:
`disable-model-invocation: true` (exact spelling held by `plugin_prompts.rs`),
the command in a `` ```! `` block so the developer is the one who runs it, every
path ending `exit 0`, `2>&1`. Without a ticket it counts
`.invar/plans/*/decision.md` — drafts, not Plans: one is recorded, several are
listed, none says to run `/invar:log` first.

**The guard, three entries now.** `plan guard` denies the agent
`invar plan approve`, `invar decision approve` and `invar decision record`.
Each has its own reason: the first tells the agent to wait for
`invar plan approve <ticket>`, the other two to tell the developer the draft is
ready for `/invar:log-approve <ticket>`. `decision record` is denied because it
would write an entry past the draft; it stays for the developer's own `note`s
and `correction`s, typed in their terminal. The ticket in a reason is the value
of `--ticket` when the command has one, else its first argument.
`plugin_prompts.rs` also checks that every line of every prompt naming either
command forbids it.

**`invar done` writes no entry.** It deletes the Plan and stages the deletion,
and refuses while an unapproved `decision.md` is beside it — "approve it with
/invar:log-approve, or delete it". It is the developer's too: `plan guard`
denies it, with a reason telling the agent to say the cycle is ready to close.

#### Not exercised (needs a real Claude Code session)

- [ ] `/invar:log-approve` appears in the `/` menu with its `argument-hint`, and
      asking the agent to "record the decision" does not run it
- [ ] Asked to record it anyway, the agent tries `invar decision approve` or
      `invar decision record`, is denied, and says the developer runs
      `/invar:log-approve`
- [ ] The developer edits `decision.md` by hand, runs `/invar:log-approve`, and
      the entry has the edit
- [ ] A refused approval (no draft, invalid draft) shows its message, and the
      agent does not retry
- [ ] After a `failed` draft is approved, the agent goes back to Phase 1 only
      when the developer asks

## 16. The review (M1-20)

Three parts: `invar review-input <TICKET>` (core), `agents/reviewer.md`, and
`/invar:review` (`skills/review/SKILL.md`). Advisory throughout (L-7): no hook runs
any of it (`tests/plugin.rs` forbids `review-input` in a hook), and `done` neither
calls it nor waits for it (`tests/review_input.rs`).

**What the reviewer sees, and what it does not.**
- It is started with the ticket line only. `/invar:review` tells the main session
  — which wrote the code — to add nothing; the reviewer is told to ignore anything
  added anyway. The main session's justifications are the one input that would
  make the review agree with the code for the wrong reason.
- `review-input` leaves `.invar/` out of the diff, so the decision log cannot reach
  it through a committed entry. The reviewer's prompt forbids reading the log, and
  `init`'s deny rule on `Read(/.invar/decisions/**)` (§12, still unverified)
  applies to subagents too, if it holds at all.
- The Plan sections it gets are the four the branch is judged against. Not the
  ticket, the questions or the rejected alternatives: they are the planner's
  reasons, and a reviewer that reads reasons starts weighing them.
- **The locked acceptance tests (AT-14).** After the criteria (and after
  `## Acceptance tests to change`), `=== Acceptance tests ===` gives the Plan's
  `Acceptance` and, for `required`, per criterion the locked files whose text names
  it, the criteria no file names, and the unlock record's files with their rounds. It
  is AT-15's `pr_body::acceptance::gather`, so the PR description and the review read
  the same files the same way. **Files, not test names:** names are known only by
  running the tests, and `review-input` runs nothing; the reviewer reads the tests in
  each file. Whether a test *means* its criterion — `asserts`, `weaker`,
  `unrelated` — is the reviewer's judgement, with `file:line`; that it runs and was
  red is `acceptance lock`'s. Shared test code (factories, seeders, helpers outside
  `acceptance.dir`) is judged on the diff: the core does not know what a factory is.

**Determinism.** `review-input` is a pure render of the Plan, the context files
and two `git diff` runs whose options pin everything a git configuration can change
(`git::REVIEW_DIFF`). The diff is from the merge base to `HEAD`, committed only, so
uncommitted work is named on stderr and left out. Above 40,000 characters the patch
is replaced by `--numstat` per file.

**Why the review is committed-only.** What is reviewed is what the PR will hold.
The protocol skill therefore offers the review after the change is committed and
before `done` — which removes the Plan the review needs.

### Seeded branches

`eval/review-seeds.sh <empty-dir>` (outside `plugin/`, which holds no scripts)
builds a small Laravel-shaped repository with three approved Plans — example 9.1's
`SHOP-214`, `SHOP-215`, whose Implementation context names no invariant, and
`SHOP-216` (AT-14), approved with acceptance tests, which the real
`invar acceptance lock` locks with a stand-in runner — and five branches. In it:
`git checkout <branch>`, start Claude Code, run `/invar:review <ticket>`.

Since M1-26 the report has two invariant headings: **named** ("Invariants the Plan
names that the change appears to break") and **unnamed** ("Invariants the Plan does
not name that the change appears to break").

| Branch | Ticket | The review must report |
|---|---|---|
| `seed-invariant` | `SHOP-214` | criterion 2 (a paid order is refused with 403) `not met`, with the `file:line` of `CancelOrder::handle`; criterion 3 (another team's order is a 404) `not met`; under **named**, the `BelongsToTeam` scope broken by `withoutGlobalScopes()` at `app/Actions/Orders/CancelOrder.php`; **unnamed** `None found.`; nothing outside Scope |
| `seed-scope` | `SHOP-214` | criteria 1–4 `met` or `unclear` (no tests are seeded, so `unclear` is honest for some); both invariant sections `None found.`; under **Changes outside Scope**, `app/Http/Controllers/InvoiceController.php` |
| `seed-unnamed-invariant` (M1-26) | `SHOP-215` | criteria 1–2 `met` or `unclear`; **named** `None found.` — the Plan names no invariant; under **unnamed**, the `BelongsToTeam` scope broken by `withoutGlobalScopes()` at `app/Actions/Orders/ExportCancelledOrders.php`; nothing outside Scope |
| `seed-weak-test` (AT-14) | `SHOP-216` | under **Locked acceptance tests**, AC1 `asserts` and AC2 `weaker` at `tests/Acceptance/Shop216ArchiveOrderTest.php:21` — it asserts the 422 and not that the order is unchanged; **Shared test code** `None found.`; nothing outside Scope |
| `seed-factory` (AT-14) | `SHOP-216` | under **Shared test code changed by the coder**, `database/factories/OrderFactory.php`, used by the AC1 test, whose `cancelled()` state now sets `archived_at`, so that test passes without the archive; AC2 `asserts` |

With `Acceptance: skipped` (the first three), the report's **Locked acceptance tests**
and **Shared test code** say there are no acceptance tests, and **Criteria no test
covers** lists, as information, the criteria no test checks.

A report that adds style remarks, a summary or suggestions fails the shape, however
correct its verdicts.

### Iteration log

| Version | Branch | Result | Changed |
|---|---|---|---|
| v1 | — | Not run | — |
| AT-14 | `seed-weak-test`, `seed-factory` | Both as the table says. Run on 2026-11-02 by starting `invar:reviewer` with the ticket line only, from `claude -p` with `--plugin-dir plugin` (in `-p` mode `/invar:review` runs as a local command and never reaches the model). `seed-weak-test`: AC2 `weaker` at `Shop216ArchiveOrderTest.php:21`, "nothing reloads the order"; Shared test code `None found.` `seed-factory`: `OrderFactory.php` under Shared test code, "both assertions pass even if the archive endpoint does nothing"; also listed under Changes outside Scope, since Scope names no factory; AC1 `asserts` as written, with that caveat beside it | The first run's AC2 test posted a form, and the reviewer rightly called AC2 `unclear` — a stock Laravel app answers a failed validation on a form post with a 302, not 422. The seed now uses `postJson` |

### Not exercised

- [ ] The reviewer, started by `/invar:review`, receives only the ticket line
      (check the subagent's first message in the transcript)
- [ ] The reviewer's Bash call `invar review-input` is not denied by the
      `PreToolUse` guard
- [x] The `Read` deny rule on `.invar/decisions/**` holds inside a subagent —
      for the tools and `cat`, not for `python3` or `grep -r`; only when started at
      the project root (§17, question 3)
- [ ] The first three seeded branches, as in the table above
- [x] `seed-weak-test` and `seed-factory`, as in the table above (AT-14; the reviewer
      started directly, not through `/invar:review` — see the iteration log)
- [ ] With `review.offer: true`, the main session offers `/invar:review` once,
      before `done`, and does not start the reviewer itself

**Changed by DESIGN v3.1.** The coder no longer reads `BUSINESS_CONTEXT.md`, so the
reviewer is the only agent that checks the branch against every Invariant
(DESIGN §13c). Two consequences here: the review is offered **by default**, so
`review.offer` should default to `true`; and the report needs a heading for an
Invariant the Plan did not name at all, separate from one the Plan named and the
code broke. A third seeded branch covers it — see §17.

## 17. The coder sub-agent, models and work units — DESIGN v3.1, **not built**

DESIGN v3.1 moves Phase 2 out of the main session (§13c) and lets the main session
choose each sub-agent's model and run coders in parallel along the Plan's work units
(§13d). Nothing of it exists yet. This section is what M5 has to build and what it
has to find out first, in the order a run would hit it.

### What to build

| Part | What it is |
|---|---|
| `agents/coder.md` | **Built in M1-24.** Phase 2: begins with `invar plan check`, reads the Plan and the conventions, never `BUSINESS_CONTEXT.md` or the log; answers `DONE` + at most eight lines, or `BLOCKED <reason>`. `model: sonnet` |
| `invar context show --conventions` | **Built in M1-23.** Core: `AI_CONTEXT.md` only. Same 10,000-character check as `--binding` (§5). `--hook` (M1-24) prints it as `SubagentStart` JSON |
| `skills/invar/SKILL.md` | **Built in M1-25.** The main session as dispatcher: after the gate it starts the coder with the ticket line only, as `/invar:review` does for the reviewer; on `BLOCKED` it puts the reason to the developer and offers `/invar:log` for a `failed` draft, which the developer records with `/invar:log-approve`; it offers the review before `done` (on by default since `invar-yaml.md` 2.0.0); it chooses models aloud and reports an unavailable one; it tells the developer to start at the project root |
| `agents/planner.md`, `agents/reviewer.md` | `model: opus` — the planner since M1-25; the reviewer in M1-26 |
| `core/tests/plugin_prompts.rs` | The coder's prompt names only commands that exist, never points at the log or `BUSINESS_CONTEXT.md`, and has both answer shapes |

Work units (DESIGN §4a, §13d) are M5b and come after the walkthrough has run with
one coder.

### Found out before building — the M1-22 spike, 2026-10-15

**Setup.** Claude Code **2.1.300**, headless (`claude -p … --output-format json`),
in a cloud Linux container, not on a developer machine. A throwaway git project
with `.claude/settings.json` denying `Read(/.invar/decisions/**)` and one decision
file holding `LOG-SECRET-4412`; a probe plugin loaded with `--plugin-dir`, whose
hooks (exec form: `command` + `args`) log their stdin payload and whose
`SessionStart` hook prints `SPIKE-CONTEXT: … AUBERGINE-7731`; a `PreToolUse` hook
that exits 2 for any call naming `protected.txt`; a `probe` agent with `Read, Write,
Bash, Grep, Glob`. Sub-agents were started by the main session with a fixed message
that mentions no marker. Every answer below is checked against the sub-agent's own
transcript (`~/.claude/projects/<project>/<session>/subagents/agent-*.jsonl`) and
the hook log, not only against what the agent said. None of the spike's files are
in the repository; how to rebuild it is at the end of this section.

**1. `SessionStart` context does not reach a sub-agent.** The main session quoted
`SPIKE-CONTEXT: the secret word for this session is AUBERGINE-7731.`; the `probe`
agent and a built-in `general-purpose` agent both answered `NONE`, and
`AUBERGINE` appears in neither sub-agent transcript nor in its `.prefix.json`. The
`SessionStart` hook ran once per session, never per sub-agent. **But a
`SubagentStart` hook's `additionalContext` does reach it**: printing
`{"hookSpecificOutput":{"hookEventName":"SubagentStart","additionalContext":"SUBSTART-CONTEXT: … ZUCCHINI-55"}}`
put that line into the sub-agent's context (`SubagentStart hook additional context:
SUBSTART-CONTEXT: the agent word is ZUCCHINI-55.`), and only there. The
`SubagentStart` payload names the agent: `"agent_id":"ae7dd4f1748b155c3",
"agent_type":"spike:probe"`.

**2. `PreToolUse` fires for a sub-agent's tool calls, and its exit 2 blocks them.**
The hook log has one `PreToolUse` per `Bash` and `Write` call of both sub-agents,
and the payload says whose call it is — `"agent_id":"ae7dd4f1748b155c3",
"agent_type":"spike:probe"` (absent for the main session's own calls). The
`Write` to `protected.txt` came back to the sub-agent as `PreToolUse:Write hook
error: […/log.sh PreToolUse]: spike: protected.txt is guarded` and the file was not
created. A `Bash` payload carries `tool_input.command`
(`"tool_input":{"command":"cat .invar/decisions/…"`). A call refused by a deny
rule never reaches the hook.

**3. The `Read` deny rule holds inside a sub-agent — for the tools, not for a
shell.** In both sub-agents and in the main session:

| Attempt | Result |
|---|---|
| `Read` on the entry | `File is in a directory that is denied by your permission settings.` |
| `Grep` for `LOG-SECRET` in `.invar` | `No matches found` |
| `Glob` `.invar/decisions/*.md` | `No files found` |
| `Bash` `cat …` / `head -1 …` | `Permission to use Bash with command cat … has been denied.` |
| `Bash` `cd .invar && cat decisions/*.md` | `cat from '…/.invar/decisions' was blocked by a deny rule.` |
| `Bash` `python3 -c "print(open('.invar/decisions/…').read())"` | **printed the entry** |
| `Bash` `grep -r LOG-SECRET .invar` | **printed the entry** |

Claude Code extends a `Read` deny to the common file readers in `Bash` (more than
§12 assumed) but not to an interpreter or `grep -r`. **And the rule only exists
when the session starts at the project root**: started in `app/Http/`, the
project's `.claude/settings.json` is not loaded at all — the `Read` and `Grep` of
the entry succeeded, and a probe deny rule `Bash(echo SETTINGS-PROBE*)` did not
fire either. Started at the root and then `cd`-ing into `app/Http` keeps it.

**4. Models.** `claude plugin validate --strict` accepts **any** `model` value in an
agent's frontmatter: `opus`, `sonnet`, `haiku`, `fable`, `inherit`,
`claude-opus-5-5`, `claude-opus-5-5[1m]`, `opus-5-5`, `Opus` and `banana` all
validated clean, exit 0. At runtime, read from the sub-agent transcripts'
`"model"`:

| Agent `model` | `Agent` call `model` | Ran as |
|---|---|---|
| none (main on sonnet) | none | `claude-sonnet-5-5` |
| `inherit` (main on haiku) | none | `claude-haiku-5-5` |
| `haiku` | none | `claude-haiku-5-5` |
| `sonnet` | none | `claude-sonnet-5-5` |
| `claude-opus-5-5` | none | `claude-opus-5-5` |
| none | `haiku` | `claude-haiku-5-5` |
| `haiku` | `opus` | `claude-opus-5-5` |
| `banana` | none | **failed**: `Agent terminated early due to an API error: There's an issue with the selected model (banana). It may not exist or you may not have access to it. (error type model_not_found, HTTP 404 …)` |

The main session **can** choose a model per call: the `Agent` tool has an optional
`model` parameter, its schema accepts `"sonnet"`, `"opus"`, `"haiku"`, `"fable"`, and
it overrides the agent's own `model`. A model the account cannot use is a hard
failure of that call, not a fallback.

**Also settled on the way** (§13): exec form — `"command":
"${CLAUDE_PLUGIN_ROOT}/…"` plus `"args"` — runs, and `validate --strict` prefers
it (shell form with an unquoted `${CLAUDE_PLUGIN_ROOT}` is a warning, so an error
under `--strict`); plain stdout of a `SessionStart` hook reaches the main session.

### Decisions from the spike

**D-17.1 — The §5c split is real, and stays as designed.** The coder does not
receive the main session's binding context, so it meets `BUSINESS_CONTEXT.md` only
through the Plan. Nothing in DESIGN §5c or M1-23 changes. The `SubagentStart`
channel is a *deterministic* way to give the coder `AI_CONTEXT.md` instead of a
prompt instruction to run `context show --conventions`; M1-24 records which it
uses. Recommended: the hook, for the coder only, because §5c's own rule is that
what must always hold does not travel by an instruction the agent may skip.

**D-17.2 — `plan guard` protects the Plan from the coder as it does from the main
session.** No change to the guard. The payload's `agent_type` would let it tell
the two apart; nothing needs that now.

**D-17.3 — The deny rule is a speed bump, not a wall, and it needs the session to
start at the root.** That was always the standing of the log's protection
(DESIGN §5c, "None of this is a boundary"): the prompts forbid reading it, the
rule catches the ordinary tools, and the check that counts is the coder's
transcript (M1-24's acceptance). Two consequences: `invar init`'s output and the
protocol skill tell the developer to start Claude Code at the project root, and the
doc comment on `DENY_RULE` says what holds and what does not.

**D-17.4 — Models are set as aliases, and a test checks them.** `validate` checks
nothing, so `core/tests/plugin_prompts.rs` asserts that every agent's `model` is
one of the values the `Agent` tool accepts per call — `opus`, `sonnet`, `haiku`,
`fable` — or `inherit`. Aliases, not full ids: they follow the current model, they
are what a per-call override can name, and a full id is the likeliest to become a
404. Defaults per DESIGN §13d: planner `opus`, reviewer `opus`, coder `sonnet`.
§13d holds as written — the per-call choice exists — with one addition: a model
the account cannot use fails the call, so the dispatcher (M1-25) puts that error to
the developer and offers to rerun the agent with `model: sonnet`; it never retries
silently on another model, because a weaker planner is the one change §13d says
must be said aloud.

**Left open: is `SubagentStop` worth hooking for the coder?** Not part of the four
questions; decide after the coder's first run (M1-24).

### How `AI_CONTEXT.md` reaches the coder — decided in M1-24

**A `SubagentStart` hook, matched to the coder, pushes it; the prompt names
`invar context show --conventions` as the fallback.** DESIGN §5c's own rule is that
what must always hold does not travel by an instruction the agent may skip, and
this is the one channel that reaches a sub-agent deterministically (question 1).
The hook runs `invar context show --conventions --hook`, which prints
`{"hookSpecificOutput":{"hookEventName":"SubagentStart","additionalContext":"…"}}`
— the form the spike saw arrive — or nothing when there are no conventions.

**The matcher was run, not assumed.** Second spike run, 2026-10-16, Claude Code
2.1.300: `SubagentStart` with `"matcher": ".*coder"` and a script printing the JSON
above with `CODER-ONLY-CONTEXT: KIWI-31`. The agent `spike:coder` quoted the line;
`spike:probe` answered `NONE`; `KIWI` appears 3 times in the coder's transcript and
0 times in the probe's. So the matcher selects by the namespaced agent type, and the
planner and the reviewer — which read their own context — receive nothing from it.

**Limits.** The 10,000-character check applies to what the coder is pushed,
exactly as for `--binding`. Outside an invar project the hook fails like
`SessionStart` does (§13, open item); it fires only when the coder is started,
which only the invar protocol does.

### Rebuilding the spike

Nothing is committed; rebuild in a temporary directory when Claude Code changes:

1. `git init` a project with `.claude/settings.json`
   `{"permissions":{"deny":["Read(/.invar/decisions/**)"]}}` and one file in
   `.invar/decisions/` holding a marker.
2. A plugin with `hooks/hooks.json` hooking `SessionStart`, `PreToolUse` (matcher
   `Read|Write|Edit|Bash`), `SubagentStart` and `SubagentStop`, each in exec form to
   one script that appends `<event>\t<stdin>` to a log; for `SessionStart` it prints
   a marker line, for `SubagentStart` the `additionalContext` JSON above, for
   `PreToolUse` it exits 2 when the payload names `protected.txt`. Agents
   `probe.md` and copies with `model:` set.
3. From the project root: `claude -p --plugin-dir <plugin> --allowedTools Read Bash
   Write Agent Grep Glob --output-format json "<start agent X with exactly this
   message: …>" < /dev/null`, then read `result`, the hook log, and the sub-agent
   transcripts for the markers and `"model"`.

### Not exercised

**Running the coder.** Since M1-25 the protocol skill starts it after the gate. To
check a run, read its transcript
(`~/.claude/projects/<project>/<session>/subagents/agent-*.jsonl`): the first
message, the `=== invar: conventions ===` block, the files it read, and any
`BUSINESS_CONTEXT.md` or decision-log text.

- [ ] The coder, started after the gate, receives only the ticket line (check
      its first message in the transcript)
- [ ] Its context holds the `=== invar: conventions ===` block from the
      `SubagentStart` hook, and it did not run `invar context show` itself
- [ ] Its first command is `invar plan check`; on a draft it stops with `BLOCKED`
      and writes no code
- [ ] Walkthrough ticket 6 (§14): `BLOCKED`, the Plan back in `draft`, no criterion
      softened
- [ ] The dispatcher cycle end to end (M1-25): ticket in → planner →
      `/invar:approve` → `invar plan check` → coder `DONE` → review offered once →
      `invar done`; recorded in §14
- [ ] `BLOCKED` end to end (M1-25): the reason put to the developer, the `failed`
      entry recorded only after they agree, the Plan back in `draft`, the planner
      called again
- [ ] The main session writes no code in either run (its own transcript: no
      `Write` or `Edit` outside a skipped trivial change)
- [ ] Started in a subdirectory, the main session says to start at the project root
- [ ] A planner started with `model: sonnet` is reported before the gate
- [ ] The coder's transcript has no `BUSINESS_CONTEXT.md` text except what the
      Plan quotes
- [ ] A third seeded review branch, `seed-unnamed-invariant`: the Plan names no
      Invariant, the code breaks one in `BUSINESS_CONTEXT.md`; the review reports
      it under its own heading

### Iteration log

| v | Date | Observed | Changed | Effect |
|---|---|---|---|---|
| — | — | Not built. | — | — |

## 18. `/invar:log` — the agent drafts a decision entry

`plugin/skills/log/SKILL.md`, invoked as `/invar:log [ticket]`; the draft's
format is `contracts/formats/decision-entry.md` §8.1, checked by
`invar decision validate`.

**Why a draft, and why there.** At the end of a cycle the developer had to write
the entry from memory, through `done -m` or `decision record -m`, and an entry
that is tedious to write is not written. The main session has the ticket, the
Plan and what the cycle did in front of it, so it drafts; the developer reads,
edits or discards. The draft sits in the Plan directory, not in
`.invar/decisions/`: the log stays insert-only and holds only what a person
chose to keep, and an unrecorded draft disappears with the Plan.

**Why the model cannot invoke it.** Drafting records nothing, but whether a cycle
deserves an entry at all is the developer's call (decision-entry.md D-6). The
protocol skill offers it — at closing, after the review offer, and after
`BLOCKED`, before the `failed` entry — and the developer runs it.
`plugin_prompts.rs` holds the frontmatter and both offers.

**It does not record.** The developer records the draft with
`/invar:log-approve` (§15), which assigns `id` and `date` then and deletes the
draft. `done` refuses while a draft is unapproved, so none is lost with the
Plan.

**What CI checks.** The example draft inside the skill passes the real
`invar decision validate`, so a contract change that outdates it fails in CI.
The skill's ticket-resolving block behaves like `/invar:review`'s. Every
`invar` command and flag the skill names exists.

### Not exercised (needs a real Claude Code session)

- [ ] `/invar:log` appears in the `/` menu with its `argument-hint`, and asking
      the agent to "log the decision" does not run it on its own
- [ ] The offer at closing comes after the review offer and before `invar done`,
      once; declined, it is not repeated
- [ ] After `BLOCKED`, the offer comes before the `failed` entry, and the draft's
      `type` is `failed`
- [ ] The draft's body says what was decided and why, not which files changed —
      judged on walkthrough tickets 1–3 and 6 (§14)
- [ ] The session runs `invar decision validate` and fixes what it reports
- [ ] A second `/invar:log` replaces the draft
- [ ] For a cycle that only changed CI or copy, the offer says it probably needs no
      more than a one-line message

### Iteration log

| v | Date | Observed | Changed | Effect |
|---|---|---|---|---|
| 1 | 2026-10-20 | Nothing yet; first draft. | Skill: resolve the ticket as `/invar:review` does; read the Plan and the cycle's own record (coder's answer, review); pick `confirmed` / `failed` / `note`; a body of two to five sentences on what was decided and why; write `decision.md`, validate, show, stop. Protocol skill: offer it at closing and after `BLOCKED`. | Not run. |
