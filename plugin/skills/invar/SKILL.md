---
name: invar
description: The invar cycle for changing code in a project that has an invar.yaml. A ticket is planned by the planner agent, the developer approves the Plan, the code is implemented against the approved Plan, and the cycle closes with invar done. Use it when a ticket or request touches business logic, behavior users can observe, permissions, money or data, or when the developer mentions a ticket, a Plan, acceptance criteria or invar. For trivial work such as a typo or a copy change, skip it and say so.
---

# The invar cycle

The project holds the specification fixed while code is written against it. A
**Plan** — written by the planner, read and approved by the developer — is the
one fixed point: your job in Phase 2 is to implement it, not to improve it. After
approval a checksum makes any change to the Plan visible, so the only way to a
different Plan is back through the developer.

You are the **main session**. The planner does Phase 1 in its own context; you
carry Phase 2.

## Is this a job for the cycle?

**Use it** when the change touches business logic, behavior a user or a tenant can
observe, permissions, money, state transitions or stored data — anything where
"done" has to be agreed before the work starts — or when the developer asks for it,
or brings a ticket id.

**Skip it** for trivial work: a typo, a copy change, a comment, formatting, a
rename with no behavior change, a dependency bump, a configuration value, a
one-line fix whose correct outcome is obvious and touches no business rule. A tool
that demands a Plan for a one-line fix gets switched off.

A test: could you state the acceptance criterion in one line, with no business
rule and no regional or per-tenant fact involved? Then skip. If you cannot tell,
ask in one line — *"Plan this, or just do it?"* — and do what the developer says.

**When you skip, say so, before you start**, in one line:
*"Skipping the invar cycle: this is a copy change with no behavior."* The
developer must be able to tell a decision from an oversight. Skipping does not
suspend the binding context in your session: its rules still bind.

## Phase 1 — plan

1. Get the **ticket id** (`SHOP-214`) and the **ticket text** from the developer. If
   there is no id, ask for one.
2. Delegate to the **planner** agent (`planner`; `invar:planner` if your agent list
   shows it namespaced). Give it the ticket id, the ticket text **exactly as
   written** — the Plan embeds it verbatim — and nothing else. Do not read the code
   for it, and do not summarize the ticket.
3. The planner answers with one of two things.
   - **`QUESTIONS`** — put every question to the developer, as they are, in one
     message. Do not answer them yourself and do not assume. Then call the planner
     again with the ticket, the developer's answers verbatim, and the same ticket
     id. Repeat until it returns a Plan.
   - **`PLAN <path>`** — go on.
4. Tell the developer the Plan's path and the planner's summary, and ask them to
   read the Plan, **the acceptance criteria above all**: they are what the code
   will be held to. If the summary says `NOT VALID YET`, or names an assumption to
   check, say so plainly.
5. If the developer wants changes, call the planner again with the ticket and the
   changes they ask for; it revises the draft. **You do not edit the Plan.**

## The developer approves

Approval is the developer's one manual step, and it is theirs alone: they run
`invar plan approve <TICKET>` in their own terminal (or `/invar:approve`, where the
plugin provides it). It records who read the Plan and when.

**Stop and wait.** Never run `invar plan approve` yourself: a hook denies it, and you
must not look for another way to record an approval. When the developer tells you
the Plan is approved, go on — and verify it, as the next section says.

## Phase 2 — implement

**Phase 2 begins with `invar plan check <TICKET>`.** It passes only a Plan that is
approved and unchanged since. If it refuses — the Plan is still a draft, or changed
after it was approved, or no one recorded the approval — **do not write code.** Tell
the developer what it said, in its words, and wait. Do not work around it.

Once it passes:

1. **Read the Plan** (`.invar/plans/<TICKET>-<slug>/spec.md`): Intent, Scope,
   Acceptance criteria, Implementation context. Implementation context is the
   distillate of the rules that apply to this change; the binding context is already
   in your session.
2. **Implement what the Plan says, inside its Scope.** The acceptance criteria are
   the definition of done: write the tests that check each one, in the way the
   project's context says it writes tests, and run the project's own tooling the way
   the team runs it. `invar` runs none of it.
3. **If a rule in the Plan contradicts a binding invariant, stop** and tell the
   developer. Do not pick one.
4. **If the work needs something outside Scope, stop and ask.** Do not widen the
   change quietly; a different change is a different cycle.

**Never edit the approved Plan** — with any tool, however small the edit. A hook
refuses edits made through `Write` and `Edit`; an edit made through a shell is caught
later by `invar plan check` and `invar done`, and is no more allowed. If you think
the Plan is wrong, that is not yours to fix; see the next section.

## When the Plan cannot be implemented

Distinguish two situations.

- **The code fails** a criterion or a test: that is ordinary work. Fix the code.
- **The Plan itself cannot be satisfied**: it contradicts itself or an invariant, it
  depends on something that does not exist, a premise in it is false. This is not
  yours to repair.

For the second: **stop.** Do not weaken a criterion, drop one, or reinterpret it so
the code passes, and do not edit the Plan. Record what happened and send it back:

`invar decision record --ticket <TICKET> --type failed -m "Plan required X; impossible because Y"`

That entry returns the Plan to `draft` and clears its approval, with the text
untouched. Tell the developer, and go back to **Phase 1**: call the planner with the
ticket and the failure. It revises the draft; the developer approves it again. "Plan
required X; impossible because Y" is one of the most useful entries the log will
hold, so write it precisely.

If the **developer** wants the approved Plan changed, they edit it themselves and
approve it again with `invar plan approve <TICKET> --reapprove`, or ask you to send
it back to Phase 1 as above.

## Closing

When every acceptance criterion is met and the project's own checks pass:

**Offer the review, if the project asks for it.** If `invar.yaml` sets
`review.offer: true`, ask the developer once, before closing: *"Run `/invar:review`
before closing? It checks the branch against the approved Plan."* The review needs
the Plan, and `done` removes it, so it comes first. It reads only what is committed
on the branch: the change has to be committed before it runs, the way this project
commits. The developer runs it, not you: do not start the reviewer yourself. It is
advisory; if the developer declines, go on.

Then:

`invar done <TICKET> -m "<what was actually built, including any deviation from the Plan>"`

Say honestly what came out; the message becomes the permanent decision entry. `done`
checks the Plan once more, renders the PR description to `.invar/out/<TICKET>-pr.md`,
records the entry and removes the Plan. Tell the developer the path of the PR
description, and to commit everything into the same PR.

## The decision log is not yours

The log records what was built before and why. It informs planning, and it is the
planner's alone: **you never read it.** Never run `invar context show` without
`--binding`, never run `invar decision log`, and never open `.invar/decisions/`. The
reason is the whole cycle: handed the log as well as the Plan, you would start
reconciling them, and an agent that reconciles is improving the Plan instead of
implementing it.

## Never

- run `invar plan approve`;
- edit an approved Plan, or work from a draft;
- write code before `invar plan check` passes;
- answer the planner's questions for the developer;
- read the decision log;
- skip the cycle without saying so.

## Commands

| | |
|---|---|
| `invar plan check <TICKET>` | Phase 2 begins here; refuses draft, changed and unrecorded approvals |
| `invar plan validate <TICKET>` | form of a draft Plan (the planner runs it) |
| `invar plan approve <TICKET>` | the developer's, from their own terminal |
| `invar decision record --ticket <TICKET> --type failed -m "..."` | the Plan cannot be implemented |
| `invar done <TICKET> -m "..."` | close the cycle |
| `/invar:review` | the developer's, before `done`, when `review.offer: true` |
| `invar context show --binding` | the binding context, which you already have |
