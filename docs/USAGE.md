# Using Invar

This guide walks through one ticket, from the request to the merged PR. It
assumes the plugin is installed and the project is set up
([`INSTALL.md`](INSTALL.md)).

You talk to Claude Code as usual. The main session follows the cycle on its
own: it starts the agents, runs the checks and tells you when it needs you.
**Your steps are the slash commands below.** Every one of them is marked "only
the developer can run it", because each records a decision that is yours.

| Your step | When |
|---|---|
| `/invar:approve <TICKET>` | You have read the Plan and agree with it |
| `/invar:relock <TICKET> <file>…` | The coder says a locked test is wrong, and you agree |
| `/invar:review <TICKET>` | Optional, before the PR |
| `/invar:log <TICKET>`, then `/invar:log-approve <TICKET>` | Optional, if the cycle is worth a decision entry |
| `/invar:done <TICKET>` | The PR review is over |

---

## Before the first ticket

Fill in the two context files at the project root. Invar never writes their
content.

- **`AI_CONTEXT.md`**: how the project is built. Layers and boundaries,
  naming, module boundaries, and **how tests are written** (framework, where
  they live, the database they use, what may be faked). The coder and the
  test-writer read it.
- **`BUSINESS_CONTEXT.md`**: the rules the application must never break
  (*Invariants*), and the values that differ per region, tenant or customer
  (*Regional / parameterized facts*). The planner reads it, and turns what a
  ticket touches into the Plan. The coder never reads it directly.

Keep both short. They are loaded into every session, and anything over the
limit (`context.binding_limit`, 9000 characters by default) is refused.

## Is a ticket a job for the cycle?

Use it when the change touches business logic, permissions, money, state
transitions, stored data, or anything a user can observe, and when "done" has
to be agreed before the work starts. Skip it for a typo, a copy change, a
dependency bump or a one-line fix with an obvious outcome. The main session
decides this and says so in one line ("Skipping the invar cycle: …"). If you
disagree, say so.

## 1. Plan

Give Claude Code the ticket id and its text:

> SHOP-214: Customers can cancel their own unpaid orders from the order page.
> Cancelling releases the reserved stock.

The **planner** agent reads the ticket, the context files, the decision log
and the code, and either asks questions or writes a Plan:

```
.invar/plans/SHOP-214-cancel-unpaid-order/spec.md
```

Questions come to you as they are. Answer them, and the planner revises. A
question about a regional or per-tenant value (a currency, a deadline, a tax
rule) is always asked, because the right answer is not in the code.

**A ticket too large for one cycle.** Before it asks or writes anything, the
planner works out what the ticket changes. If it holds several things that could
each be built and reviewed on their own (a migration with a backfill, a feature
on top of it, and a screen reworked, say), it writes no Plan and tells you why:

```
SPLIT
Why: three independent changes: …
```

Smaller tickets are easier to approve, to implement without guessing, and to
review. Whether to split is your decision, since how tickets are tracked differs
from team to team. Answer one of two ways:

- *"Propose the split."* The planner lists the tasks in the order they depend
  on each other, each with a title and a ticket description you can paste into
  your tracker. Create the tickets there, by hand or through your tracker's MCP,
  and start each one as a ticket of its own. Invar never creates a ticket or
  gives one an id.
- *"Plan it as one."* The planner writes one Plan, and does not raise the split
  again for this ticket.

**Read the Plan, the acceptance criteria above all.** They are what the code
will be held to:

```markdown
## Acceptance criteria
- AC1: Given a pending, unpaid order of mine, when I cancel it, then its status
  is `cancelled` and its reserved stock is available for sale again.
- AC2: Given a paid order of mine, when I try to cancel it, then the request is
  refused with 403 and the order is unchanged.
- AC3: Given an order that belongs to another team, when I send a cancel request
  with its id, then the response is 404 and the order is unchanged.
```

Want something changed? Say so, and the planner revises the draft. Nobody
else edits it.

## 2. Approve

```
/invar:approve SHOP-214
```

This prints the criteria once more, records your name, the date and a
checksum of the Plan, and starts the next phase at once. It also records
whether this ticket gets **locked acceptance tests**:

| You run | Acceptance tests |
|---|---|
| `/invar:approve SHOP-214` | as `invar.yaml` says (`acceptance.required`, `true` by default once the section is there) |
| `/invar:approve SHOP-214 --acceptance` | yes |
| `/invar:approve SHOP-214 --no-acceptance` | no: the coder writes its own tests, unlocked |
| `/invar:approve SHOP-214 --no-start` | records the approval, and starts nothing yet |

From now on the Plan is frozen. If anyone changes it, even by one character,
`invar plan check` refuses and no agent starts. To change an approved Plan,
ask the planner to revise it and approve again with `--reapprove`.

## 3. The acceptance tests are locked

*Only when the approval says `required`.*

The **test-writer** agent turns each criterion into a test that reaches the
behavior from outside, as a user or client would (for Laravel, a Pest or
PHPUnit feature test over HTTP, against the real database). Every test names
its criterion (`AC2`), and the file starts with a lock line:

```php
<?php
// invar-lock: SHOP-214
```

Then `invar acceptance lock SHOP-214` runs them. It locks the files only if:

- every criterion has a test
- every test **fails on an assertion**, before the code exists. A test that
  passes already checks nothing, and a test that crashes on a missing class
  is not red for the right reason.

Locking writes a digest into the line:

```php
// invar-lock: SHOP-214 sha256:9c1e4f…
```

From then on nobody changes the file: the coder may not, the test-writer may
not, and `plan check`, `done` and CI notice if it happens.

The test-writer answers `LOCKED`, and the main session confirms it with
`invar acceptance verify SHOP-214` before the coder starts. If it answers
`BLOCKED`, you get the reason and nothing else starts. Typical reasons: no
factory for a model, no test database, or behavior that already exists, so a
test cannot fail. Fix the cause and ask to go on. Or, if this ticket should go
without tests, run `/invar:approve SHOP-214 --reapprove --no-acceptance` and
remove any file it left behind with a pending `invar-lock:` line.

## 4. Code

The **coder** agent gets only the ticket id. It reads the approved Plan and
`AI_CONTEXT.md`, implements inside the Plan's Scope, makes the locked tests
pass, and runs your project's own checks. It answers in one of three ways:

- **`DONE`** with a short summary. Go to step 5.
- **`BLOCKED <reason>`**: the Plan cannot be implemented as approved (two
  criteria contradict each other, something it depends on does not exist).
  It never softens a criterion to get past it. You decide what follows: often
  `/invar:log` with a `failed` entry, which returns the Plan to draft for the
  planner to revise.
- **`BLOCKED-TEST <file> <AC> <reason>`**: a locked test asserts something its
  criterion does not say. The coder changes nothing. **You** decide whether the
  test is wrong:

  ```
  /invar:relock SHOP-214 tests/Acceptance/CancelOrderTest.php
  ```

  This unlocks the file. The test-writer fixes it and locks it again (the red
  check is skipped for that file only, and the output says so), and the coder
  starts again. At most two rounds per ticket. A third is refused: two
  corrections of the same tests mean the criteria are not what you meant, and
  the cycle should end as `failed`.

## 5. Review, PR, decision entry, close

After `DONE`, the main session offers each step once:

1. **Commit** the change.
2. **`/invar:review SHOP-214`** (optional, advisory). A reviewer agent reports,
   per criterion, whether the code meets it. Per locked test, it says whether
   the test `asserts` its criterion or is `weaker` or `unrelated`, with
   `file:line`. It also reports changes to shared test code (factories,
   seeders) that a locked test depends on, invariants the change appears to
   break, and changes outside Scope. It never blocks anything.
3. **The PR description**: `invar pr-body SHOP-214 --out .invar/out/SHOP-214-pr.md`
   renders the Plan, including whether the ticket has acceptance tests and
   which files are locked.
4. **Push and review the PR** as you always do.
5. **`/invar:log SHOP-214`** (optional) drafts a decision entry next to the
   Plan: what was built, a deviation, a lesson for the next Plan. Read it,
   edit it, and record it with **`/invar:log-approve SHOP-214`**. The planner
   reads the log on later tickets.
6. **`/invar:done SHOP-214`** when the PR review is over. It checks the Plan
   and the locks once more, deletes the Plan and stages the deletion, so it
   leaves in the same PR as the code. `--commit "<message>"` commits it as
   well. CI can run it instead (see [`INSTALL.md`](INSTALL.md)).

---

## What stays after the PR

- The **locked acceptance tests**, in your test directory, each with its
  criterion's text and its lock line. They keep proving the behavior on every
  later ticket, and CI (`invar acceptance verify`) notices if one changes.
- The **decision entry**, if you recorded one, in `.invar/decisions/`, with
  `acceptance: required` or `skipped` in its header.
- The **PR description**, with the Plan in it.

The Plan itself is gone, by design: the next ticket gets a fresh one.

## When a later ticket must change a locked test

A later ticket may need another ticket's locked test to change. The planner
says so in the Plan, under `## Acceptance tests to change`, with `change` or
`delete` and a reason, and your approval covers it. `plan check` and `done`
refuse an undeclared change to another ticket's locked test. In 0.1.0, a
`delete` works: the test-writer removes the file and covers the behavior under
the new ticket's criteria. A `change` makes the test-writer answer `BLOCKED`,
naming the file. Plan such a change as a `delete` plus new criteria
(`docs/TASKS-AT.md`, OQ-4).

## Commands you may run in a terminal

You never need them inside Claude Code. They are useful in a shell or in CI
(see [`INSTALL.md`](INSTALL.md) for the CLI):

| Command | What it does |
|---|---|
| `invar plan validate <TICKET>` | Checks the Plan's form |
| `invar plan check <TICKET>` | Checks that the Plan is approved and unchanged, and the locks |
| `invar plan approve <TICKET> [--acceptance\|--no-acceptance] [--reapprove]` | The approval, as `/invar:approve` runs it |
| `invar acceptance verify [<TICKET>]` | Checks the locks. Without a ticket: the whole test directory, for CI |
| `invar acceptance unlock <TICKET> <file>…` | What `/invar:relock` runs |
| `invar pr-body <TICKET>` | The PR description |
| `invar decision log` | The decision log, newest first |
| `invar done <TICKET> [--commit <message>]` | Closes the cycle |
| `invar hash --lock <file>` | Shows what a lock line records, for debugging |

Every command has `--help`.
