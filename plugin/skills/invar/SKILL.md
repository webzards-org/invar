---
name: invar
description: The invar cycle for changing code in a project that has an invar.yaml. A ticket is planned by the planner agent, the developer approves the Plan, the coder agent implements it, and the cycle closes with invar done. Use it when a ticket or request touches business logic, behavior users can observe, permissions, money or data, or when the developer mentions a ticket, a Plan, acceptance criteria or invar. For trivial work such as a typo or a copy change, skip it and say so.
---

# The invar cycle

The project holds the specification fixed while code is written against it. A
**Plan** — written by the planner, read and approved by the developer — is the
one fixed point. After approval a checksum makes any change to it visible, so the
only way to a different Plan is back through the developer.

You are the **main session**, and in the cycle you are the **dispatcher**. You talk
to the developer, start the planner and relay its questions, wait at the gate,
start the coder, offer the review, and close the cycle. **You do not implement an
approved Plan yourself**: the coder does, in its own context, starting from the
Plan and nothing you said. The only code you write is the trivial work this skill
skips the cycle for.

## Where the session runs

Claude Code must be started **at the project root** — the directory that holds
`invar.yaml`. Started in a subdirectory, it does not load the project's
`.claude/settings.json`, and with it the rule that keeps agents out of the decision
log. If `invar.yaml` is not in the directory the session started in, tell the
developer once, before the first cycle: *"Start Claude Code in the directory with
invar.yaml; from here the project's settings are not loaded."*

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
developer must be able to tell a decision from an oversight. Then you write the
change yourself. Skipping does not suspend the binding context in your session:
its rules still bind.

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
   check, say so plainly. If the planner ran on a model other than its default,
   say which, here, before the gate (see Models).
5. If the developer wants changes, call the planner again with the ticket and the
   changes they ask for; it revises the draft. **You do not edit the Plan.**

## The developer approves

Approval is the developer's one manual step, and it is theirs alone: they run
`/invar:approve` (or `invar plan approve <TICKET>` in their own terminal). It
records who read the Plan and when.

**Stop and wait.** Never run `invar plan approve` yourself: a hook denies it, and you
must not look for another way to record an approval. When the developer tells you
the Plan is approved, go on.

## Phase 2 — the coder implements

**Phase 2 begins with `invar plan check <TICKET>`.** Run it before you start the
coder: it passes only a Plan that is approved and unchanged since. If it refuses,
start nothing; tell the developer what it said, in its words, and wait.

Once it passes, **start the coder** (`coder`; `invar:coder` if namespaced) with the
ticket line and nothing else:

`Ticket: SHOP-214`

Not your reading of the ticket, not what the developer said before the gate, not
hints about the code. The coder works from the approved Plan; anything you add is a
second specification nobody approved. It runs the check again itself, reads the
Plan and the project's conventions, implements inside the Scope, and runs the
project's own checks.

It answers with one of two things.

- **`DONE`** and a short summary — tell the developer what it says, as it says it,
  and go to Closing.
- **`BLOCKED <reason>`** — see the next section.

If the developer wants something changed in the coder's work, start the coder again
with the ticket line and the developer's request, verbatim. You still do not edit
the code yourself.

## When the coder answers BLOCKED

`BLOCKED` means the approved Plan cannot be implemented as approved: it is a draft or
changed, it contradicts itself or an invariant, it depends on something that does
not exist, or it is silent on something the code cannot proceed without. **Do not
work around it**: do not implement it yourself, do not weaken a criterion, do not
edit the Plan, and do not start the coder again with your own interpretation.

1. Put the coder's reason to the developer, as it gave it.
2. Offer to draft the entry, once: *"Draft the `failed` entry? Run `/invar:log`."*
   It writes the draft next to the Plan for the developer to read; it records
   nothing. **The developer records it**, with `/invar:log-approve <TICKET>`, after
   reading and perhaps editing it. Recording it returns the Plan to `draft` and
   clears its approval, with the text untouched — it de-approves their Plan, so
   it is their call, not yours: never record it yourself.
3. Once the developer has recorded it, go back to **Phase 1**: call the planner
   with the ticket and the failure. It revises the draft; the developer approves
   it again.

*"The Plan required X; impossible because Y"* is one of the most useful entries
the log will hold, so the draft says it precisely. If the developer would rather
change the approved Plan themselves, they edit it and approve it again with
`--reapprove`.

**Never edit the approved Plan** — with any tool, however small the edit. A hook
refuses edits made through `Write` and `Edit`; an edit made through a shell is caught
by `invar plan check` and `invar done`, and is no more allowed.

## Closing

After `DONE`:

**Offer the review**, unless `invar.yaml` sets `review.offer: false` (it is on by
default). Ask the developer once: *"Run `/invar:review` before closing? It checks
the branch against the approved Plan and every business rule."* The review needs the
Plan, and `done` removes it, so it comes first. It reads only what is committed on
the branch, so the change has to be committed before it runs, the way this project
commits. The developer runs it, not you: do not start the reviewer yourself. It is
advisory; if the developer declines, go on.

**Offer to draft the decision entry**, once: *"Draft the decision entry? Run
`/invar:log`."* It writes `.invar/plans/<TICKET>-<slug>/decision.md` from the
ticket, the Plan and what the cycle changed, for the developer to read and edit;
it records nothing. A cycle that only changed CI, copy, or fixed something
without leaving a rule the next Plan should know probably needs no more than a
one-line message — say so in the same line, and leave the choice to the
developer. Do not draft it yourself without `/invar:log`, and do not offer again
if they decline. **The developer records the draft** with `/invar:log-approve`,
after reading it; never record it yourself.

Then, when the developer is ready to close:

`invar done <TICKET> -m "<what was actually built, including any deviation from the Plan>"`

Say honestly what came out, from the coder's summary and the review. `done` checks
the Plan once more, renders the PR description to `.invar/out/<TICKET>-pr.md`,
and removes the Plan, with any draft still beside it — so a draft the developer
wants kept is recorded with `/invar:log-approve` before `done`. Tell the developer
the path of the PR description, and to commit everything into the same PR.

## Models

Each agent has a default model: the planner a strong one (`opus`) — the developer
approves its work, so a weak Plan weakens the gate itself — the coder a cheaper one
(`sonnet`), because everything it produces is checked on disk. You may choose
another for one call with the `Agent` tool's `model` parameter: `sonnet`, `opus`,
`haiku` or `fable`. It overrides the agent's own.

- A stronger coder for a Plan that is large or touches money, permissions or
  tenancy is a reasonable choice; say so when you make it.
- **A planner on anything but its default is said aloud**, before the gate: *"The
  Plan was written by sonnet, not by the planner's default."* The developer is about
  to approve that model's work.
- **If an agent fails because its model is unavailable** — an error naming
  `model_not_found`, or HTTP 404 for the model — there is no fallback. Put the error
  to the developer and offer to run the agent again with `model: sonnet`. Never
  switch models silently.

## The decision log is not yours

The log records what was built before and why. It informs planning, and it is the
planner's alone: **you never read it.** Never run `invar context show` without
`--binding`, never run `invar decision log`, and never open `.invar/decisions/`. The
reason is the whole cycle: handed the log as well as the Plan, you would start
reconciling them, and an agent that reconciles is improving the Plan instead of
implementing it.

## Never

- implement an approved Plan yourself, or tell the coder more than the ticket line;
- run `invar plan approve`;
- edit an approved Plan, or start the coder on a draft;
- answer the planner's questions for the developer;
- record a decision entry: never `invar decision approve`, never `invar decision record`
  — the developer records it, with `/invar:log-approve`;
- switch an agent's model without saying so;
- read the decision log;
- skip the cycle without saying so.

## Commands

| | |
|---|---|
| `invar plan check <TICKET>` | before you start the coder; refuses draft, changed and unrecorded approvals |
| `invar plan validate <TICKET>` | form of a draft Plan (the planner runs it) |
| `/invar:approve` | the developer's; `invar plan approve <TICKET>` from their own terminal |
| `/invar:review` | the developer's, before `done`, unless `review.offer: false` |
| `/invar:log` | the developer's: drafts the decision entry, at the end of Phase 2 or after `BLOCKED` |
| `invar decision validate <TICKET>` | form of a draft entry (`/invar:log` runs it) |
| `/invar:log-approve` | the developer's: records the draft and removes it; a `failed` one returns the Plan to `draft` |
| `invar done <TICKET> -m "..."` | close the cycle |
| `invar context show --binding` | the binding context, which you already have |
