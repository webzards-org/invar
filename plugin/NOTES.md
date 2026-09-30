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
| **CLI binary** | A bundled executable, invoked by path or via `bin/` | `bin/invar` (see §6 below — unresolved) | No |
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

## 6. Shipping the binary — unresolved for M5

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

## 10. Verification checklist

> **Do not test on the digest fixtures.** They are contract data with
> deliberately unusual bytes (`contracts/fixtures/digest/`), and an edit
> corrupts them — which `generate.py --check` will then report as a failure.
> Use a throwaway file: `echo hi > /tmp/hook-test.txt` and edit that.

The spike is done when all of these hold:

- [ ] `claude plugin validate ./plugin/spike-m0-6` passes
- [ ] `claude --plugin-dir plugin/spike-m0-6` starts, and `/hooks` lists the
      `PostToolUse` hook with matcher `Write|Edit`
- [ ] Asking Claude to edit a throwaway file produces an
      `[invar-spike] Edit -> …` line in the transcript. **Check that the
      transcript actually shows the `Edit` tool succeeding** — if `Edit` failed
      and Claude fell back to `Bash`, the hook correctly did not fire, and the
      spike has not been demonstrated (see §8)
- [ ] The log file under `~/.claude/plugins/data/<id>/edits.log` contains one
      line per edit. `log-edit.py --where` run from a shell prints the *fallback*
      path instead, because `CLAUDE_PLUGIN_DATA` exists only inside the hook
      process; find the real one with
      `find ~/.claude/plugins/data -name edits.log`
- [ ] Breaking the script on purpose (`raise` at the top of `main`) does **not**
      break the session — the edit still succeeds

That last one is the point of the whole exercise: whatever the hook does, the
developer's work continues.
