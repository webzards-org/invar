---
name: plan-audit
description: An optional check of a draft Plan against the code, before approval. Begins the audit, runs the plan-auditor agent, and shows the report invar computes. Only the developer can run it.
argument-hint: "<ticket> [--restart]"
disable-model-invocation: true
allowed-tools: Bash(invar plan audit begin *)
---

The developer ran `/invar:plan-audit $ARGUMENTS`. The command below was run for
them, by Claude Code, before you saw this message. It records the Plan as it is now,
so that the report can show what the audit changed.

```!
invar plan audit begin --skill $ARGUMENTS
```

**If the output has the line `plan audit begun: <TICKET>`**, delegate to the
**plan-auditor** agent (`plan-auditor`; `invar:plan-auditor` if your agent list shows
it namespaced). **Give it the line `Ticket: <TICKET>` and nothing else**: not what the
Plan is about, not what you expect it to find, not what the planner said. The auditor
is independent of the session that wrote the Plan.

When it answers:

- **`AUDITED`** — run `invar plan audit report <TICKET>` and show its output to the
  developer **exactly as it is**, refusal or report. Then stop.
- **`BLOCKED <reason>`** — show its line to the developer, and stop.

**Otherwise, stop.** Start no agent. A line starting `error:` is a refusal, and it
says why; that is for the developer. An audit begun already is begun again with
`/invar:plan-audit <TICKET> --restart`, by the developer.

**If there is no output at all** — the command above did not run, or you were asked
to run it yourself — do not run it: say *"The audit did not begin: Claude Code did
not run `invar plan audit begin`. Run `/invar:plan-audit` again."* and stop.

- Do not run `invar plan audit begin` yourself, with or without `--restart`: a hook
  denies it, and the baseline is the developer's.
- **Never summarize the report in your own words**, restate its last line, or call
  the Plan ready, valid or approved. The report says what it is: an agent's
  assessment, not a check.
- Do not edit the Plan because of a finding, and do not restore anything the report
  refuses: the developer decides. If they want the draft revised, the planner
  revises it, as in Phase 1.
- It is advisory. `/invar:approve` does not wait for it, and shows its line.
