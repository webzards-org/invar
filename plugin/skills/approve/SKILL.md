---
name: approve
description: The developer's approval of a Plan, from inside Claude Code. Runs invar plan approve, shows the Acceptance criteria and the result, and starts Phase 2 at once when the approval is recorded, unless --no-start is given. Only the developer can run it.
argument-hint: [ticket] [--no-start]
disable-model-invocation: true
allowed-tools: Bash(invar plan approve *)
---

The developer ran `/invar:approve $ARGUMENTS`. The command below was run for
them, by Claude Code, before you saw this message. It is their approval and not
yours.

```!
invar plan approve --skill $ARGUMENTS
```

Show the output above to the developer as it is. Then do one of two things,
decided by the output alone — never by your own reading of whether the Plan
looks approved.

**If the output contains the line `approved <TICKET> checksum sha256:…`** — the
line `invar plan approve` prints only when it has just recorded an approval —
**and no line `Phase 2: not started (--no-start)`**: the approval is the
developer's decision to implement. **Start Phase 2 for that ticket now**, as the
protocol skill (`invar`) describes it: run `invar plan check <TICKET>` first,
then start the coder (one per work unit, when the Plan declares them) with the
ticket line and nothing else. Do not ask the developer to confirm; say in one
line that Phase 2 has started.

**Otherwise, stop.** Write no code and start no coder. A line starting `error:`
is a refusal, and it says why; that is for the developer. `already approved`
recorded nothing new; `Phase 2: not started` means the developer will implement
later, or elsewhere. Either way, Phase 2 starts when the developer asks for it.

**If there is no output at all** — the command above did not run, or you were
asked to run it yourself — do not run it: say *"The approval was not recorded:
Claude Code did not run `invar plan approve`. Run `/invar:approve` again, or
`! invar plan approve <TICKET>` in this prompt."* and stop.

- Do not run `invar plan approve` yourself, with or without `--reapprove`, and do
  not retry it. If it refused, the output says why; that is for the developer.
- Starting Phase 2 decides nothing: `invar plan check`, which the coder runs
  again, still refuses a Plan that is not approved or has changed since.
