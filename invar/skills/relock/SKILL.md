---
name: relock
description: The developer's decision that a locked acceptance test is wrong, from inside Claude Code. Runs invar acceptance unlock on the named files, and starts the test-writer to fix them when they are unlocked. Only the developer can run it.
argument-hint: <ticket> <file>...
disable-model-invocation: true
allowed-tools: Bash(invar acceptance unlock *)
---

The developer ran `/invar:relock $ARGUMENTS`. The command below was run for them,
by Claude Code, before you saw this message. It is their decision and not yours.

```!
invar acceptance unlock --skill $ARGUMENTS
```

Show the output above to the developer as it is. Then do one of two things,
decided by the output alone — never by your own reading of whether the test looks
wrong.

**If the output contains the line `unlocked <TICKET> <N> file(s), round <R> of 2`**:
start the test-writer for that ticket now, exactly as the section "When the coder
answers BLOCKED-TEST" of the protocol skill (`invar`) describes it; if you have not
read that skill in this session, read it first. Do not ask the developer to confirm;
say in one line that the test-writer has started.

**Otherwise, stop.** Write no code and start no agent. A line starting `error:` is a
refusal, and it says why; that is for the developer. Nothing was unlocked. A third
round is refused this way: the message says to end the cycle as `failed`.

**If there is no output at all** — the command above did not run, or you were
asked to run it yourself — do not run it: say *"The tests were not unlocked: Claude
Code did not run `invar acceptance unlock`. Run `/invar:relock` again."* and stop.

- Do not run `invar acceptance unlock` yourself, and do not retry it. If it refused,
  the output says why; that is for the developer.
- Do not edit a test yourself, and do not take a lock off another way.
