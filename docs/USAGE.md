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

![A full cycle on a real ticket: questions, the approved Plan, the coder, and a review of every acceptance criterion and business rule.](media/full-cycle.gif)

---

## Before the first ticket

Fill in the two context files at the project root. Invar never writes their
content.

- **`AI_CONTEXT.md`**: how the project is built. Layers and boundaries,
  naming, module boundaries, and **how tests are written** (framework, where
  they live, the database they use, what may be faked). The coder and the
  test-writer read it.
- **`BUSINESS_CONTEXT.md`**: the words the business uses (*Terms*), the rules
  the application must never break (*Invariants*), and the values that differ
  per region, tenant or customer (*Regional / parameterized facts*). The
  planner reads it, and turns what a ticket touches into the Plan. The coder
  never reads it directly.

Under *Terms*, write the business's own words, one line each, such as
`- Available balance: what a customer can spend now, after holds.` Use them in
the invariants, and the planner uses them in every Plan. Leave the heading
empty if you don't need it.

Under *Invariants*, give each rule an id, so the gate can show which ones a Plan
leaves out:

```markdown
## Invariants (always true, no exceptions)

- INV-1: a paid order is never deleted; it is cancelled through the refund flow.
- INV-3: every query on team data is scoped to the current team by the
  `BelongsToTeam` global scope.
```

An id is `INV-` and a number, then a colon, at the start of each item. Numbers
are names, like ADR numbers: never reuse one for another rule, and leave the
number of a removed invariant unused, since a Plan or a PR may cite it. Gaps and
any order are fine. Number all of them or none: `invar plan validate` refuses a
list where some have ids and some don't, as well as a duplicate or a malformed id
(`INV-0`, `inv-1`). Without ids everything works as before, and `plan approve`
warns once that it cannot show which invariants a Plan leaves out. The full rule
is in [`contracts/formats/business-context.md`](../contracts/formats/business-context.md).

Keep both short. They are loaded into every session, and anything over the
limit (`context.binding_limit`, 9000 characters by default) is refused.

## Is a ticket a job for the cycle?

Use it when the change touches business logic, permissions, money, state
transitions, stored data, or anything a user can observe, and when "done" has
to be agreed before the work starts. Skip it for a typo, a copy change, a
dependency bump or a one-line fix with an obvious outcome. The main session
decides this and says so in one line ("Skipping the invar cycle: …"). If you
disagree, say so.

**When your project requires a cycle for some paths.** If `invar.yaml` lists
`cycle.required_paths`, CI runs `invar cycle check`, and a pull request that
changes one of those paths passes only when the change was made while a
Plan was approved, and that cycle was closed with `/invar:done`, or with a
skip you wrote. A change made after `done`, or before the approval, is not
covered by the cycle. After a skipped change, the main
session runs `invar cycle required` on the files it changed and tells you when
one of them is required. If the change really needs no Plan, say why in a
trailer on any commit of the branch:

```bash
git commit --amend --trailer "Invar-Skip: a comment typo in Order, no behavior"
```

The reason is printed in CI and read in review. The main session never writes
it: the decision is yours. **With signed approvals** (`approval.signed: true` on
the base), a bare trailer does not count: sign the skip instead, once the change
is committed, with **`/invar:skip <reason>`**. It signs the required changes as
they are, so a later change to one of them needs a new skip.

**When you weaken a check on purpose.** If CI runs `invar config check`, a
branch that removes the `acceptance:` section, changes `acceptance.dir` or
`acceptance.run`, turns off `acceptance.required`, `approval.signed` or
`review.offer`, or removes a pattern from `cycle.required_paths` fails until a
commit says why:

```bash
git commit --amend --trailer "Invar-Config: the acceptance tests moved under tests/Feature"
```

Write it yourself. The agent cannot make those edits through its edit tools,
and it does not write the trailer either. **With signed approvals**, sign it
instead with **`/invar:config-accept <reason>`**; a later edit of `invar.yaml`
needs a new one.

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
  gives one an id. A last `Strategy:` line says how the tasks are cut, by layer
  or in vertical slices (each one delivering a behavior a user can see), and
  why. When the other cut fits the ticket too, it is sketched there and you
  choose:

  ```
  Strategy: by layer (API first, then the screen): the API is large and can be
  reviewed alone. A vertical split is possible too: (1) add and list gift cards
  end to end, (2) redeem one at checkout end to end. Which do you prefer?
  ```

  When only one cut makes sense (a backfill the feature needs first), the line
  says so and asks nothing. If your team always cuts one way, say so in
  `AI_CONTEXT.md`, and the planner follows it.
- *"Plan it as one."* The planner writes one Plan, and does not raise the split
  again for this ticket.

![A real run: the planner answers SPLIT with its reasons, proposes two tickets on request, and the main session creates them in GitHub and closes the original once the developer agrees.](media/split.gif)

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
checksum of the Plan, and starts the next phase at once.

When your invariants have ids, it lists every one of them after the criteria:

```
Invariants in BUSINESS_CONTEXT.md:
  INV-1  NOT NAMED in this Plan  - a paid order is never deleted; it is cancelled…
  INV-3  named, covered by AC3   - every query on team data is scoped to the curr…

1 invariant is not named: approving says none of them applies to this ticket.
```

Read the `NOT NAMED` lines. Approving says that none of those invariants applies
to this ticket. If one does, don't approve: tell the planner, which names it in
the Plan's *Implementation context* and gives it a criterion that cites it
(`- AC3 (INV-3): Given …`). `plan validate` already makes sure that every
invariant the Plan names has such a criterion. It also records
whether this ticket gets **locked acceptance tests**:

| You run | Acceptance tests |
|---|---|
| `/invar:approve SHOP-214` | as `invar.yaml` says (`acceptance.required`, `true` by default once the section is there) |
| `/invar:approve SHOP-214 --acceptance` | yes |
| `/invar:approve SHOP-214 --no-acceptance` | no: the coder writes its own tests, unlocked |
| `/invar:approve SHOP-214 --no-start` | records the approval, and starts nothing yet |

Two more choices, both off unless you ask for them:

- **Commit the approval.** `--commit "<message>"` commits the Plan, alone, right
  after it is approved; `approval.commit: true` in `invar.yaml` does it every
  time, as `Approve plan SHOP-214`, and `--no-commit` skips it once. Anything
  else you staged stays staged. Your hooks and commit signing apply, and nothing
  is pushed.
- **Sign the approval.** With `approval.signed: true` in `invar.yaml`, the
  approval is signed with your SSH signing key, the one git uses for signed
  commits, and the signature is written next to the Plan as `approval.sig`. If
  the key asks for a touch, a passphrase or a confirmation, it asks after the
  criteria are printed; answer it within 90 seconds. If git has no SSH signing
  key, or the key does not sign, nothing is approved and the reason is printed.
  CI checks the signatures with `invar approval verify`. Setting it up, and
  what it proves and does not prove, is in
  [INSTALL.md](INSTALL.md#optional-signed-approvals).

From now on the Plan is frozen. If anyone changes it, even by one character,
`invar plan check` refuses and no agent starts. No agent edits an approved
Plan, and none can approve a draft by writing the approval fields into it: the
hook refuses both. To change an approved Plan, either:

- record a `failed` entry (`/invar:log SHOP-214`, then
  `/invar:log-approve SHOP-214`). The Plan goes back to draft with its
  approval cleared; ask the planner to revise it, then approve it with
  `/invar:approve SHOP-214`, as any draft; or
- edit the Plan yourself, then approve it again with
  `/invar:approve SHOP-214 --reapprove`.

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

Locking writes a digest into the line, and the criteria the file's tests cover:

```php
// invar-lock: SHOP-214 sha256:9c1e4f… ac=AC1,AC2
```

From then on nobody changes the file: the coder may not, the test-writer may
not, and `plan check`, `done` and CI notice if it happens. The digest covers
the `ac=` list too, so taking a criterion off the line is a change like any
other. CI uses the list to check that every one of those criteria ran.

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

- **`DONE`** with a short summary. With acceptance tests, the coder has run
  `invar acceptance run SHOP-214` first, and the main session runs it again
  before anything else. It runs the ticket's locked tests, and only them, and
  says per criterion whether they pass:

  ```
  AC1: passed
  AC2: not passed
    tests/Acceptance/CancelOrderTest.php: `it AC2 refuses a paid order` (AC2): failure: Failed asserting that 200 is identical to 403.
  acceptance: red SHOP-214
  ```

  On `acceptance: green SHOP-214`, go to step 5. On `red`, you see what did not
  pass, and the main session offers to start the coder again with it; it offers
  no commit, review or `/invar:done` until the tests are green. A skipped test
  is not green, and neither is one that passed only on a retry. This run is
  feedback in your session; the check you can rely on is your CI's (see
  [Verify the locks in CI](INSTALL.md#optional-verify-the-locks-in-ci)).
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

After `DONE`, and with acceptance tests only after `acceptance: green`, the
main session offers each step once:

1. **Commit** the change.
2. **`/invar:review SHOP-214`** (optional, advisory). A reviewer agent reports,
   per criterion, whether the code meets it. Per locked test, it says whether
   the test `asserts` its criterion or is `weaker` or `unrelated`, with
   `file:line`. It also reports changes to shared test code (factories,
   seeders) that a locked test depends on, invariants the change appears to
   break, conventions stated in `AI_CONTEXT.md` it appears to break (each
   quoted, with `file:line`), and changes outside Scope. It never blocks
   anything.
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
([OQ-4](https://github.com/webzards-org/invar/blob/f0c9f903303b5d814cf4f4ea9c85ef70071a34b7/docs/TASKS-AT.md#L137)
in the task file as it was before its removal).

## Using Invar with other skill sets

Many teams already give Claude Code a skill set or plugin that teaches it a way
of working. Invar can run alongside one, but some combinations conflict. As a
rule of thumb:

- **Complements Invar.** Skills for domain language and glossaries, for
  exploring a design before there is a ticket, and for writing documentation.
  They work on things the cycle does not cover, and a clearer domain or design
  makes a better Plan.
- **Overlaps with the cycle.** Skills that write tests, implement or review
  code. Inside an Invar cycle, let Invar's test-writer, coder and reviewer do
  that work, so there is one process and its checks hold: two TDD loops or two
  reviews on the same ticket give you two verdicts and no clear gate. Outside a
  cycle, for work the main session skips the cycle for, use whatever you like.
- **Contradicts it.** Any instruction to proceed without asking you, or to
  treat the code as the specification. Invar's approval steps are the point:
  the Plan you approve is the specification, and only you approve it, relock a
  test or close a cycle. If a skill set says otherwise, remove that
  instruction or don't use the two on the same project.

The difference in one line: a skill set teaches an agent good practice; Invar
enforces it with checks on disk and in CI.

## Commands you may run in a terminal

You never need them inside Claude Code. They are useful in a shell or in CI
(see [`INSTALL.md`](INSTALL.md) for the CLI):

| Command | What it does |
|---|---|
| `invar plan validate <TICKET>` | Checks the Plan's form |
| `invar plan check <TICKET>` | Checks that the Plan is approved and unchanged, and the locks |
| `invar plan approve <TICKET> [--acceptance\|--no-acceptance] [--reapprove] [--commit <MESSAGE>\|--no-commit]` | The approval, as `/invar:approve` runs it; signed with `approval.signed` |
| `invar cycle skip --reason <reason>` | What `/invar:skip` runs: a signed skip of the cycle, committed |
| `invar config accept --reason <reason>` | What `/invar:config-accept` runs: a signed weakening of `invar.yaml`, committed |
| `invar pr-body <TICKET> --from-approval [--base <ref>]` | The PR description from the Plan as it was signed, read from git, with who signed it |
| `invar approval verify [--base <ref>]` | Checks, for CI, that every approval on the branch is signed by a key in the base's `.invar/approvers`, when the base has `approval.signed: true` |
| `invar acceptance verify [<TICKET>] [--results <report>… [--report-root <dir>]]` | Checks the locks. Without a ticket: the whole test directory, for CI. With `--results`, also that every locked test passed in your JUnit reports |
| `invar acceptance run <TICKET>` | Runs the ticket's locked tests, and says per criterion whether they pass |
| `invar acceptance unlock <TICKET> <file>…` | What `/invar:relock` runs |
| `invar pr-body <TICKET>` | The PR description |
| `invar decision log` | The decision log, newest first |
| `invar done <TICKET> [--commit <message>] [--results <report>…]` | Closes the cycle; with `--results`, only if the ticket's locked tests passed |
| `invar hash --lock <file>` | Shows what a lock line records, for debugging |

Every command has `--help`.
