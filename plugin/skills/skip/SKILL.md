---
name: skip
description: The developer's signed skip of the cycle, from inside Claude Code, for a change to paths that need a cycle and do not get one. Runs invar cycle skip, which signs the reason with the developer's key and commits it as an Invar-Skip trailer. Only the developer can run it.
argument-hint: "<reason>"
disable-model-invocation: true
allowed-tools: Bash(invar cycle skip *)
---

The developer ran `/invar:skip $ARGUMENTS`. The command below was run for them, by
Claude Code, before you saw this message. It is their skip and not yours.

```!
invar cycle skip --skill --reason "$ARGUMENTS"
```

Show the output above to the developer as it is, and stop. Write no code and start
no agent.

A line `skip signed: …` means the skip is committed: `invar cycle check` in CI counts
it for the required changes listed above it, as they are now. A later change to one
of them, or to another required path, needs a new skip. A line starting `error:` is a
refusal, and it says why; that is for the developer.

**If there is no output at all** — the command above did not run, or you were asked
to run it yourself — do not run it: say *"The skip was not signed: Claude Code did
not run `invar cycle skip`. Run `/invar:skip <reason>` again, or `! invar cycle skip
--reason "<reason>"` in this prompt."* and stop.

- Do not run `invar cycle skip` yourself, and do not retry it.
- Never write an `Invar-Skip` trailer yourself, in any form.
