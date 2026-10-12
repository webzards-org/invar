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

## 12. The deny rule `invar init` writes — **not yet verified** (M1-15)

`invar init` adds `Read(/.invar/decisions/**)` to `permissions.deny` in the
project's `.claude/settings.json` (§13a). The planner gets the decision log
through `invar context show`, which is a command and reads files itself, so
denying the Read *tool* costs the planner nothing; the point is that the main
session, which writes the code, does not stumble into the log and start
"improving" the Plan instead of implementing it (§7).

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

### The three hooks

Each is one invocation of `${CLAUDE_PLUGIN_ROOT}/bin/invar` in exec form (§7),
with a timeout, and none runs `plan approve` or `done`. There is no lint hook
and no test hook (L-3).

| Event | Matcher | Command | Effect |
|---|---|---|---|
| `SessionStart` | — | `context show --binding` | Binding context in every session |
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
   a plugin with a top-level `bin/`; for a pilot inside one company, distributed
   through a git marketplace, that distribution is irrelevant.
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
- [ ] Exec form with a `command` that is itself a path (`${CLAUDE_PLUGIN_ROOT}/bin/invar`)
      and `args`. The spike verified `python3` plus `args` only
- [ ] The `SubagentStop` matcher `.*planner` selects the planner. M1-18 names the
      agent, and a plugin's agents may be namespaced (`invar:planner`); the pattern
      matches either way if matchers are searched, not anchored
- [ ] A `SubagentStop` block — exit 2 with stderr, and `{"decision": "block"}` — makes
      the planner carry on and shows it the reason; the second stop arrives with
      `stop_hook_active: true`
- [ ] A `Bash` payload carries `tool_input.command` (taken from §8 and the docs)
- [ ] Plain stdout of the `SessionStart` hook reaches the session (§7 says it does)
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
did not. **v1 has not been run.** Everything below the first row of the log is
empty on purpose.

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

**Exit criteria**, from the task: three tickets valid, one with an invariant; the
regional question asked; the main session never receives the log; the skill skips
and says so; Phase 2 refuses a draft; and the iterations recorded below.

### The iteration log

One row per change to either prompt. **Observed** is what a run showed, with the
ticket; **Changed** is the edit, in a sentence; **Effect** is what the next run
showed. A change with no observation behind it does not go in the log — it goes in
the next row's **Observed** once it has one.

| v | Date | Observed | Changed | Effect |
|---|---|---|---|---|
| 1 | 2026-10-11 | Nothing yet; first draft. | Planner: procedure in six steps; one answer shape (`QUESTIONS` or `PLAN`); the regional question mandatory, with one exception (the ticket names the value); Implementation context as a distillate with a test per item ("would the coder get this wrong without it?"); one criterion per invariant, written as the negative case; one worked example, in a different domain from the pilot's; the log reasoned from, never quoted. Skill: when to skip and the sentence to say; Phase 2 begins with `plan check`; never edit an approved Plan; a `failed` entry for an impossible one; `done` to close; the log is the planner's alone. | Not run. |

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
   plugin validate` of an agent's `model` value was not checked.
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

**No ticket.** The block counts `.invar/plans/*/spec.md`: one Plan is approved;
several are listed and nothing is approved; none says so. The ticket is checked
against `[A-Za-z0-9._/-]` before it reaches the shell. (Better placed in the CLI,
so a terminal behaves the same: `plan approve` without a ticket.)

**Output.** The block ends every path with `exit 0` and runs the CLI with `2>&1`,
because an injection that fails aborts the invocation and the developer would
see no reason. `plan approve` prints the Acceptance criteria before it writes,
so they are the first thing in the output.

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
builds a small Laravel-shaped repository with example 9.1's Plan approved and two
branches. In it: `git checkout <branch>`, start Claude Code, run
`/invar:review SHOP-214`.

| Branch | The review must report |
|---|---|
| `seed-invariant` | criterion 2 (a paid order is refused with 403) `not met`, with the `file:line` of `CancelOrder::handle`; criterion 3 (another team's order is a 404) `not met`; under **Invariants**, the `BelongsToTeam` scope broken by `withoutGlobalScopes()` at `app/Actions/Orders/CancelOrder.php`; nothing outside Scope |
| `seed-scope` | criteria 1–4 `met` or `unclear` (no tests are seeded, so `unclear` is honest for some); no invariant; under **Changes outside Scope**, `app/Http/Controllers/InvoiceController.php` |

A report that adds style remarks, a summary or suggestions fails the shape, however
correct its verdicts.

### Iteration log

| Version | Branch | Result | Changed |
|---|---|---|---|
| v1 | — | Not run | — |

### Not exercised

- [ ] The reviewer, started by `/invar:review`, receives only the ticket line
      (check the subagent's first message in the transcript)
- [ ] The reviewer's Bash call `invar review-input` is not denied by the
      `PreToolUse` guard
- [ ] The `Read` deny rule on `.invar/decisions/**` holds inside a subagent
- [ ] Both seeded branches, as in the table above
- [ ] With `review.offer: true`, the main session offers `/invar:review` once,
      before `done`, and does not start the reviewer itself
