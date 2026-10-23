---
name: log-approve
description: The developer's approval of a draft decision entry, from inside Claude Code. Records .invar/plans/<TICKET>-<slug>/decision.md in the decision log as it is now and removes the draft. Only the developer can run it.
argument-hint: [ticket]
disable-model-invocation: true
allowed-tools: Bash(invar decision approve *)
---

The developer ran `/invar:log-approve $ARGUMENTS`. The command below was run for
them, by Claude Code, before you saw this message. It records their decision, not
yours.

```!
invar decision approve --skill $ARGUMENTS
```

Show the output above to the developer exactly as it is, and stop.

- A line starting `error:` is a refusal, and it says why; nothing was recorded.
- If there is no output at all — the command above did not run, or you were
  asked to run it yourself — do not run it: say *"The entry was not recorded:
  the command of `/invar:log-approve` did not run. Run `/invar:log-approve`
  again."* and stop.
- Do not run `invar decision approve` yourself, and do not retry it. If it
  refused, the output says why; fixing the draft is the developer's call, or
  `/invar:log` again if they ask for it.
- Do not record the entry another way: never run `invar decision record`, and
  never write into `.invar/decisions/`.
- If the output has a `reset:` line, the entry was `failed` and the Plan is a
  draft again. Back to Phase 1 when the developer asks for it: the planner
  revises the Plan, and the developer approves it again.
