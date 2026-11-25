---
name: config-accept
description: The developer's signed acceptance of a change to invar.yaml that weakens what invar checks, from inside Claude Code. Runs invar config accept, which signs the reason with the developer's key and commits it as an Invar-Config trailer. Only the developer can run it.
argument-hint: "<reason>"
disable-model-invocation: true
allowed-tools: Bash(invar config accept *)
---

The developer ran `/invar:config-accept $ARGUMENTS`. The command below was run for
them, by Claude Code, before you saw this message. It is their decision and not
yours.

```!
invar config accept --skill --reason "$ARGUMENTS"
```

Show the output above to the developer as it is, and stop. Write no code and start
no agent.

A line `configuration change signed: …` means it is committed: `invar config check`
in CI counts it for the weakenings listed above it, with invar.yaml as it is now. A
later change to invar.yaml needs a new one. A line starting `error:` is a refusal,
and it says why; that is for the developer.

**If there is no output at all** — the command above did not run, or you were asked
to run it yourself — do not run it: say *"The change was not signed: Claude Code did
not run `invar config accept`. Run `/invar:config-accept <reason>` again, or `! invar
config accept --reason "<reason>"` in this prompt."* and stop.

- Do not run `invar config accept` yourself, and do not retry it.
- Never write an `Invar-Config` trailer yourself, in any form.
