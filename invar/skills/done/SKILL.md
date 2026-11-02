---
name: done
description: The developer's close of an invar cycle, from inside Claude Code. Runs invar done, which checks the approved Plan once more, deletes it and stages the deletion, or commits it when asked. Only the developer can run it.
argument-hint: [ticket] [--commit <message> | --no-commit]
disable-model-invocation: true
allowed-tools: Bash(invar done *)
---

The developer ran `/invar:done $ARGUMENTS`. The command below was run for them,
by Claude Code, before you saw this message. It closes the cycle on their
decision, not yours.

```!
invar done --skill $ARGUMENTS
```

Show the output above to the developer exactly as it is, and stop.

- A line starting `removed:` means the cycle is closed: the Plan directory is
  deleted, and the deletion is staged — or committed, with `--commit` or when
  the project's `invar.yaml` sets `done.commit: true`. Remind the developer, in
  one line, that it belongs in the same PR as the code.
- A line starting `error:` is a refusal, and it says why; nothing was deleted. A
  draft entry still beside the Plan is recorded with `/invar:log-approve`, or
  deleted, first.
- If there is no output at all — the command above did not run, or you were
  asked to run it yourself — do not run it: say *"The cycle was not closed:
  Claude Code did not run `invar done`. Run `/invar:done` again."* and stop.
- Do not run `invar done` yourself, do not retry it, and never delete the Plan
  another way.
