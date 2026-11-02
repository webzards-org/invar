# Contract: Decision entry

**Version:** 1.4.0 — the `acceptance` key: how the ticket was closed; see §4.2 and
the version history
**Status:** stable
**Implements:** §7 (Decision log), §4c (a `failed` entry resets the cycle),
§12 (the entry survives the Plan); light mode L-1, L-4 (`docs/TASKS-M1.md`)
**Read by:** the core (`format/decision`), `invar decision log`,
`invar decision validate` and `invar decision approve` (the draft),
`invar context show` (M1-14), the `/invar:log` and
`/invar:log-approve` skills, any later
reimplementation and the post-MVP decision graph

> Changing this file is a **public-contract change** (§14.3). It is made
> deliberately, in its own PR, with the reason recorded, the version bumped,
> and every reader updated in the same PR. New optional keys may be added at
> any time under one rule: **an absent key means the previous behavior.**

---

## 1. Scope

One file per entry, under `.invar/decisions/`. An entry records what was
actually built and why — or why it could not be — after the Plan that
produced it is gone (§12). The log is **advisory** context for the planner
(§7): no command may fail or block on what an entry *says*.

This contract fixes the file's name, its front matter and its types. The body
is free text and is not parsed.

## 2. Path and filename

```
.invar/decisions/<YYYY-MM-DD>-<HHMMSS>-<module>-<slug>.md
```

- `<YYYY-MM-DD>-<HHMMSS>` is the moment the entry was written, **in UTC**
  (D-2). Filenames therefore sort chronologically as plain strings, whoever
  wrote them and wherever.
- `<module>` equals the `module` field (§4): `^[A-Za-z][A-Za-z0-9_]*$`, or
  `global` for an entry not tied to one module. Case is kept as written
  (`Billing`), the same as in the Plan header (plan-header.md D-15). It contains no `-`, so the first `-`
  after it starts the slug.
- `<slug>` matches `^[a-z0-9]+(-[a-z0-9]+)*$`. The tool uses the slug of the
  ticket's Plan directory (`PROJ-412-reserve-stock` → `reserve-stock`), or
  the ticket id in lowercase (`proj-412`) when there is no Plan.
- **Two entries never share a name.** When the name is taken — a second entry
  in the same second, for the same module and slug — the tool appends `-2`,
  `-3`, … to the slug (D-3). The suffix is part of the slug; nothing reads it
  separately.

## 3. Encoding and line endings

UTF-8, no BOM. The tool writes LF line endings and ends the file with exactly
one `\n`. A reader accepts LF, CRLF and CR, as for the Plan (plan-header.md
§3).

## 4. File structure

```markdown
---
id: d-7c41e09a
date: 2026-10-06T18:37:12+02:00
ticket: PROJ-412
module: orders
type: failed
---

The free-text body.
```

- The first line is exactly `---`; the front matter ends at the next line that
  is exactly `---`.
- The front matter is a flat list of `Key: value` lines, the same shape as the
  Plan header (plan-header.md §4.2): one key per line, a space after the
  colon, no nesting, no quoting. Blank lines and `# comment` lines are
  permitted. **It is not YAML** (D-5).
- The body is everything after the closing `---`, with leading blank lines
  ignored. It must not be empty.

### 4.1 Fields

| Field | Required | Allowed values |
|---|---|---|
| `id` | yes | `d-` followed by 4 to 32 lowercase hex digits. The tool writes 8. Unique across `.invar/decisions/` (D-4) |
| `date` | yes | `YYYY-MM-DDTHH:MM:SS` followed by `Z` or `±HH:MM` — the local time of writing, with its offset (D-2) |
| `ticket` | yes | `^[A-Z][A-Z0-9]*-[0-9]+$`, the ticket of the cycle the entry belongs to (D-7) |
| `module` | yes | `^[A-Za-z][A-Za-z0-9_]*$`, or `global`. Equal to `<module>` in the filename, letter for letter |
| `type` | yes | `confirmed`, `correction`, `failed` or `note` (§5) |
| `scenario` | **no** | The `.feature` scenario the entry is about, when `scenarios: true`. **Absent in light mode** (L-1) |
| `acceptance` | **no** | `required` or `skipped`, lowercase, exactly: what the ticket's approved Plan decided about acceptance tests (§4.2). **Absent means unknown** |

Each key appears at most once. A key not listed here is permitted and ignored,
so a newer writer cannot break an older reader.

**Consistency:** `module` equals the filename's `<module>`, and `date`,
converted to UTC, equals the filename's timestamp. The tool always writes them
that way; a reader that checks an entry reports a mismatch.

### 4.2 `acceptance`

Since 1.4.0. It records whether the ticket's cycle was approved with acceptance
tests (`required`) or without them (`skipped`), the `Acceptance` decision of the
Plan (plan-header.md §5). The Plan is deleted in the PR (§12), so after merge this
key is where that decision survives, in a form that can be counted (D-13).

- **Absent means unknown, not `skipped`** (D-14). An entry written before 1.4.0
  has no key, and neither has one recorded by `invar decision record` without
  `--acceptance`. Nothing can be said about those tickets; a count that read them
  as `skipped` would report skipping that never happened.
- **The value is the Plan's** (D-15). The agent copies it into the draft from the
  `acceptance:` line of `invar plan check` (§8.1), and `invar decision approve`
  checks it against the Plan before recording: the entry says what the approval
  said, not what anyone remembered of it.
- What the Plan says is read from its header: for `Status: approved`, the
  `Acceptance` value, and `skipped` when it has none — a Plan approved before
  plan-header.md 1.7.0, when no test-writer existed, which is what `plan check`
  prints (plan-header.md §5.1). A Plan with `Status: draft` has decided nothing,
  and says nothing.
- Any other value is a problem, reported like any other malformed key.

## 5. Types

The four types of §7:

| Type | Meaning | Written by |
|---|---|---|
| `confirmed` | The cycle completed: what was built, including any deviation from the Plan | `invar decision approve`, from a draft (§8.1). Not written mechanically (D-12) |
| `correction` | The developer corrected generated code, or changed something by hand outside the cycle | `invar decision record`, by the developer |
| `failed` | The cycle could not be completed, or the approved Plan turned out to be impossible (§4c) | `invar decision approve`, from a draft, or `invar decision record`, by the developer — and it resets the cycle (§7) |
| `note` | Anything else the developer wants the planner to know next time | `invar decision approve`, from a draft, or `invar decision record`, by the developer |

## 6. Insert-only

**The tool never modifies or deletes an existing entry.** A new entry is
created with an exclusive create: if the name exists, the write does not
replace it, it picks the next suffix (§2). The developer may edit or delete
entries by hand; the tool neither detects nor cares (§7).

## 7. `failed` resets the cycle

Writing a `failed` entry for a ticket also returns that ticket's Plan to
`draft` (§4c, light-mode form):

1. `Status` becomes `draft`, and `Approved-by`, `Approved-at` and
   `Approved-checksum` become empty. `Lockfile-checksum` and `Acceptance`,
   when present, become empty too. The keys stay (plan-header.md D-3).
2. The body is not touched — nothing is reverted, only de-approved.
3. There are no `.feature` files to reset in light mode.

Without a Plan there is nothing to reset, and the command **refuses**: a
`failed` entry promises that the approval it disproves is gone. A failure
without a Plan is recorded as a `note`.

The two writes happen Plan first, entry second. If the entry cannot be
written, the Plan is restored to the exact bytes it had (D-8).

The entry's `acceptance` (§4.2) is what the Plan said **before** the reset: the
reset clears `Acceptance`, and the entry records the approval it disproves.

## 8. Who writes, and when

**An entry is written because a person decided it is worth keeping** (D-6),
and **the person writes it** (D-12):

- `invar decision approve <ticket>` records the ticket's draft (§8.1), as it
  is at that moment, and deletes the draft. It is the developer's, run through
  `/invar:log-approve`.
- `invar decision record` writes an entry the developer types themselves: a
  `correction`, or a `note`. Its `--acceptance required|skipped` gives the
  `acceptance` key; without it the key is absent, and the command invents no
  value (§4.2). A value that disagrees with the ticket's approved Plan is refused,
  naming both.
- **The agent writes no entry**, not even when asked: it drafts one (§8.1),
  and `plan guard` denies it both commands.
- **No hook ever writes an entry**, and no other command writes one as a side
  effect. `invar done` only deletes the Plan (D-12).

### 8.1 The draft

Before an entry is recorded, the agent may **draft** it, so the developer
decides on a text instead of writing one (D-9). A draft is not an entry: it
lives with the Plan, nothing reads it as part of the log, and it becomes an
entry only when it is recorded.

**Path:** `.invar/plans/<TICKET>-<slug>/decision.md`, next to the ticket's
`spec.md`. One draft per Plan; writing it again replaces it. Recording it
removes it. `invar done` refuses while it is there — deleting the Plan would
delete the reviewed text with it — so it is recorded or deleted first.

**Structure:** the same as an entry (§3, §4) — `---`, a flat `Key: value` front
matter, `---`, a non-empty body — with these fields:

| Field | Required | Allowed values |
|---|---|---|
| `ticket` | yes | As in §4.1, and equal to the `<TICKET>` of the Plan directory it is in |
| `module` | yes | As in §4.1 |
| `type` | yes | `confirmed`, `failed` or `note` (D-11) |
| `scenario` | **no** | As in §4.1 |
| `acceptance` | **no** | As in §4.1, and equal to what the Plan says (§4.2); checked when the draft is recorded |
| `id`, `date` | **must be absent** | Assigned when the draft is recorded (D-10) |

A key not listed is permitted and ignored, as for an entry.

**The type follows the cycle:** `confirmed` for a cycle that finished,
`failed` after the coder answered `BLOCKED` or the approved Plan turned out to
be impossible, `note` otherwise.

**`acceptance`** is copied from the `acceptance:` line that
`invar plan check <TICKET>` prints, never from the conversation (D-15). When
`plan check` refuses, there is no line to copy, and the key is left out.

**The body** says what was decided and why, in a few sentences. It is not a
summary of the diff: the diff is in git, and the planner reads the log for the
reasons it cannot find there.

**Who writes it:** the main session, through `/invar:log`, when the developer
asks or accepts the protocol's offer. It is checked with
`invar decision validate`. Drafting writes no entry and resets nothing.

**Recording it:** `invar decision approve <ticket>`, run by the developer
(`/invar:log-approve`). It reads the draft as it is then — with any edits the
developer made while reading it — and validates it; checks its `acceptance`
against the Plan (below); writes the entry with a new
`id` (D-4) and `date` (D-2), its `module` and `scenario` taken from the draft
and its slug from the Plan directory (§2); and deletes the draft.

The `acceptance` check, against the Plan as it is before any reset (§7):

| The draft | The Plan says (§4.2) | `decision approve` |
|---|---|---|
| a value | the same value | records it |
| a value | the other value | **refuses**, naming both |
| a value | nothing (`Status: draft`, or a Plan it cannot read) | **refuses**: the value cannot be checked |
| no key | a value | records the Plan's value, and says so in its output |
| no key | nothing | records no key: unknown |
| another value | — | **refuses**, as for any malformed key |
 A `failed`
draft resets the cycle as any `failed` entry does (§7, D-8). It refuses, and
writes nothing, when there is no draft or the draft is not valid. A draft that
cannot be deleted after its entry is written is reported; approving it again
would record it twice.

## 9. Reading

- Order is the filename order. Compare the names **without `.md`**, and
  compare a run of digits at the end of the name **as a number**; the greater
  name is the newer entry. That makes `…-reserve-stock-2` newer than
  `…-reserve-stock` and `…-10` newer than `…-9` within one second (D-3), and
  is otherwise a plain comparison that puts the UTC timestamp first.
  `invar decision log` prints newest first.
- `invar context show` (M1-14) reads **every** entry, whatever its module,
  newest first, within a budget (§7). The module is a label in the entry's
  header line, not a filter.
- A reader that meets an entry it cannot parse reports it and goes on. The log
  is advisory; one hand-edited file must not hide the others.

## 10. Decisions and their reasons

**D-1 — One file per entry** (§7). Two branches that each add an entry merge
without conflict. A single file with prepended entries would conflict on
every parallel write.

**D-2 — UTC in the filename, local time in `date`.** The filename is what
orders the log, and a team spans time zones: an entry written at 09:00 in
Belgrade is older than one written at 08:30 in London. Local timestamps in
filenames would sort them the wrong way round. `date` keeps the writer's local
time *and* its offset, which is what a person reading the entry recognises,
and from which the UTC time follows exactly. Both come from git's clock — the
same source as `Approved-at` (plan-header.md D-13), honouring
`GIT_COMMITTER_DATE`.

**D-3 — A taken name gets a suffix; a file is never replaced.** The timestamp
has one-second resolution, and two entries in the same second are realistic
(a script, two quick `note`s). An exclusive create makes overwriting
impossible rather than unlikely, and the suffix keeps the name inside the
filename shape of §2, so every reader keeps working. Ordering within the
second follows the suffix (§9); entries for *different* modules or slugs in
the same second have no order between them that anything relies on.

**D-4 — `id` is random, not derived.** It is the reference target of the
post-MVP decision graph, so it must not change when anything else about the
entry does — not the filename, not the module, not the body. A random id has
no inputs to change. Eight hex digits are 32 bits; the writer also checks the
id against every entry already in the log, so a collision is not merely
unlikely but excluded. Readers accept 4 to 32 digits, so short hand-written
ids such as `d-4f2a` (§7's example) stay valid.

**D-5 — Flat `Key: value`, not YAML.** Same reasoning as the Plan header
(plan-header.md D-1): a line-oriented reader is simple to reimplement exactly,
and no value here needs YAML's quoting or nesting.

**D-6 — Entries are written on a person's decision.** §7 calls the log "safe
to write automatically" because no agent may block on it. That is true, and
it is not the point: what makes the log useful is that each entry is worth
reading, and the person who knows whether a change is worth remembering is the
developer. An agent that writes an entry whenever it thinks of one fills the
planner's budget (§7: about 50 entries) with noise and pushes the useful ones
out. So entries are written when a person decides: by closing a cycle with
`done`, or by asking for an entry. In particular the agent does not write
`failed` on its own — that entry also de-approves the Plan, which is the
developer's call.

**D-7 — `ticket` is required.** It is the join key back to the cycle: the
Plan and the branch. An entry that belongs to no ticket is a fact about the project, and
those belong in `AI_CONTEXT.md` or `BUSINESS_CONTEXT.md` (§7, "Boundary
against the other context files").

**D-8 — For `failed`, the Plan is reset first, and restored if the entry
fails.** The two writes cannot be one atomic operation. Of the two
half-done states, "entry written, Plan still approved" is the dangerous one:
the next step would run against an approval the entry says was disproved.
Resetting the Plan first, and putting its exact bytes back if the entry
cannot be written, means the only possible outcomes are both writes or
neither — and `.invar/decisions/` stays strictly insert-only, because nothing
there is ever removed to undo a write.

**D-9 — The agent drafts, the developer decides.** D-6 keeps entries a
person's decision, and that is unchanged. What it cost was the text: at the end
of a cycle the developer had to write an entry from memory, and an entry that is
tedious to write is not written. The agent has the ticket, the Plan and what the
cycle changed in front of it, so it writes the draft and the developer reads,
edits or discards it. The draft lives in the Plan directory, not in
`.invar/decisions/`, so the log stays insert-only and holds only what a person
decided to keep; a draft nobody recorded disappears with the Plan.

**D-10 — No `id` or `date` in a draft.** Both describe the entry, not its
draft. The `date` of an entry is the moment a person decided to keep it, which
is when it is recorded, and the `id` is minted then so it is unique in the log
as it is at that moment (D-4). A draft that carried them would invite copying a
stale date into the log.

**D-11 — A draft is `confirmed`, `failed` or `note`, never `correction`.** A
`correction` records something the developer changed by hand outside the cycle;
the agent did not see it happen and cannot draft it truthfully. The developer
records a correction directly with `invar decision record`.

**D-12 — Every entry is approved by the developer; none is written
mechanically.** Until 1.3.0, `invar done` wrote the `confirmed` entry from its
`-m`, and the agent ran `invar decision record` when the developer agreed. Both
made the developer's decision a moment in a conversation rather than a step
they took: the text recorded was whatever the agent typed after "yes". Now the
agent drafts, the developer reads the draft in the file, and the developer's
own command records exactly that file. `decision record` stays for the entries
only the developer can write (D-11). `invar done` writes no entry: it deletes
the Plan, and refuses while an unapproved draft is beside it.

**D-13 — The acceptance decision is a header key.** How often a team closes a
ticket without acceptance tests is the pilot's key measure, and after merge the
Plan that recorded the decision is gone (§12). The PR description says it too,
but in prose, for a reader. In the front matter it is one `key: value` line that
a script counts with `grep` across `.invar/decisions/`, and that no change to the
body's wording can break.

**D-14 — An absent `acceptance` means unknown.** Every entry before 1.4.0 lacks
the key, and so does one the developer records with `decision record` without
saying. Reading absence as `skipped` would turn the whole history before this
version, and every hand-written entry, into tickets closed without tests: the
measure would rise for no reason but the upgrade. A count reports three numbers
— `required`, `skipped`, unknown — and only the first two are evidence.

**D-15 — The Plan is the source; the draft is checked against it.** The agent
writes the draft, and an agent can misremember, or copy what the conversation
said instead of what the developer approved. The approved Plan's header holds
the decision, covered by its checksum (plan-header.md §5.3), so the draft copies
it from `plan check`, and `decision approve` compares the two and refuses a
disagreement rather than choose. A draft without the key is filled in from the
Plan: it can only be the Plan's value, and leaving it out would turn a known
answer into an unknown one. A `failed` entry reads the Plan before its reset,
which is what clears the value (§7).

## 11. Open questions

**OQ-1 — Relations between entries.** `supersedes: d-…` and similar fields
for the post-MVP decision graph. Not needed until the graph exists; absent
means no relation, so adding them later is compatible.

## 12. Examples

### 12.1 Valid — a `failed` entry in light mode

File: `.invar/decisions/2026-10-06-163712-orders-cancel-unpaid-order.md`

```markdown
---
id: d-7c41e09a
date: 2026-10-06T18:37:12+02:00
ticket: SHOP-214
module: orders
type: failed
---

The approved Plan requires cancelling an unpaid order to release its reserved
stock, but reservations are held by the warehouse service and released only
on its own schedule; `orders` cannot release them. Back to Phase 1: the Plan
must either call the warehouse API or drop the criterion.
```

**Why it is valid:** every required key, no `scenario` (light mode), and the
filename agrees with the front matter — `orders`, and 18:37:12 at +02:00 is
16:37:12 UTC.

### 12.2 Invalid — an unknown type and no `id`

File: `.invar/decisions/2026-10-06-163712-orders-cancel-unpaid-order.md`

```markdown
---
date: 2026-10-06T18:37:12+02:00
ticket: SHOP-214
module: orders
type: fixed
---

Reservation is released on cancel.
```

**Why:** `id` is required, and `fixed` is not one of the four types (§5). A
reader reports both.

### 12.3 Invalid — disagrees with its filename, and has no body

File: `.invar/decisions/2026-10-06-163712-payments-cancel-unpaid-order.md`

```markdown
---
id: d-7c41e09a
date: 2026-10-06T18:37:12+02:00
ticket: SHOP-214
module: orders
type: note
---
```

**Why:** the filename says `payments`, the front matter says `orders`; and
the body is empty — an entry that records nothing.

### 12.4 Valid — a draft after a finished cycle

File: `.invar/plans/SHOP-214-cancel-unpaid-order/decision.md`

```markdown
---
ticket: SHOP-214
module: orders
type: confirmed
---

Cancelling releases reserved stock inside the cancel transaction, through
`StockLedger`, rather than in a queued job: a job would leave the stock neither
reserved nor sellable until it ran. Paid orders are refused with 403 and go
through refunds.
```

**Why it is valid:** the three required keys, no `id` or `date`, and `SHOP-214`
is the ticket of the directory it is in.

### 12.5 Invalid — a draft with a date, a `correction`, and the wrong ticket

File: `.invar/plans/SHOP-214-cancel-unpaid-order/decision.md`

```markdown
---
date: 2026-10-06T18:37:12+02:00
ticket: SHOP-300
module: orders
type: correction
---

Fixed the cancel button.
```

**Why:** `date` is assigned when the draft is recorded (D-10); `correction` is
not a draft type (D-11); and the draft names `SHOP-300` in the Plan directory of
`SHOP-214`.

### 12.6 Valid — a `confirmed` entry with acceptance tests

File: `.invar/decisions/2026-10-07-091503-orders-cancel-unpaid-order.md`

```markdown
---
id: d-3e9a51c0
date: 2026-10-07T11:15:03+02:00
ticket: SHOP-214
module: orders
type: confirmed
acceptance: required
---

Cancelling releases reserved stock inside the cancel transaction, through
`StockLedger`. The acceptance tests were locked red before the code existed.
```

**Why it is valid:** `required` is one of the two values, and it is what the
Plan of SHOP-214 recorded when the draft was approved (§8.1).

### 12.7 Valid — a `confirmed` entry without acceptance tests

File: `.invar/decisions/2026-10-08-140210-Billing-invoice-footer.md`

```markdown
---
id: d-81c7f2d4
date: 2026-10-08T14:02:10Z
ticket: SHOP-230
module: Billing
type: confirmed
acceptance: skipped
---

The invoice footer shows the team's VAT number. A copy change in one template;
the developer approved the Plan without acceptance tests.
```

**Why it is valid:** `skipped` is the other value. Without the line, the entry
would still be valid, and would say nothing about acceptance (D-14).

### 12.8 Invalid — an unknown `acceptance` value

File: `.invar/decisions/2026-10-08-140210-Billing-invoice-footer.md`

```markdown
---
id: d-81c7f2d4
date: 2026-10-08T14:02:10Z
ticket: SHOP-230
module: Billing
type: confirmed
acceptance: none
---

The invoice footer shows the team's VAT number.
```

**Why:** `acceptance` is `required` or `skipped`, or absent (§4.1); `none` is
neither. Leaving the key out says "unknown", which is not what `none` means
either.

### 12.9 Refused on approval — a draft that disagrees with its Plan

File: `.invar/plans/SHOP-214-cancel-unpaid-order/decision.md`, next to a
`spec.md` whose header says `Status: approved` and `Acceptance: required`

```markdown
---
ticket: SHOP-214
module: orders
type: confirmed
acceptance: skipped
---

Cancelling releases reserved stock inside the cancel transaction.
```

**Why:** the draft is valid on its own, and `invar decision validate` accepts
it. `invar decision approve` refuses it and writes nothing: the draft says
`skipped`, the Plan says `required`, and the entry records the Plan's decision
(D-15). The fix is to copy the value from `invar plan check SHOP-214`, or to
remove the line and let the Plan's value be recorded.

## 13. Version history

**1.4.0 — the `acceptance` key (AT-15).**

- §4.1 and §4.2: the optional key `acceptance: required | skipped`, what the
  Plan decided about acceptance tests. Absent means unknown. D-13 and D-14.
- §8.1: a draft copies it from `invar plan check`; `invar decision approve`
  checks it against the Plan, fills it in when the draft has none, and refuses a
  disagreement. D-15.
- §7: a `failed` entry records the value from before the reset.
- §8: `invar decision record --acceptance`.
- Examples 12.6 to 12.9.

**Why:** the Plan is deleted in the PR, and how often a ticket is closed without
acceptance tests is the pilot's measure (D-13).

**Compatibility:** every entry and draft written by a 1.3.1 tool stays valid,
and reads as before: none has the key, and absent means unknown, so no earlier
entry is counted either way. An older reader ignores the key, as it ignores any
key it does not know (§4.1). Only a file someone edited by hand to hold an
`acceptance` key with another value than `required` or `skipped` — which no
tool ever wrote — is now reported.

**1.3.1 — a `failed` entry clears `Acceptance` (AT-3).**

- §7: `Acceptance`, new in `plan-header.md` 1.7.0, is cleared with the other
  approval values, and its key stays.

**Why:** a `failed` entry promises that the approval it disproves is gone, and that
decision was part of the approval.

**Compatibility:** nothing changes for a Plan without the field.

**1.3.0 — the developer approves the draft.**

- §8: every entry is written by the developer — `invar decision approve` for a
  draft, `invar decision record` by hand; the agent writes none. D-12.
- §8.1: recording a draft — read as it is, validated, written with a new `id`
  and `date`, deleted.
- §5: who writes each type.

**Why:** the developer's decision to keep an entry has to be a step they take
on a text they have read, not a "yes" the agent acts on (D-12).

**Compatibility:** the entry and draft formats are unchanged; every valid file
stays valid.

**1.2.0 — the draft.**

- §8.1: a draft entry at `.invar/plans/<TICKET>-<slug>/decision.md`, its
  fields, and who writes it. D-9 to D-11; examples 12.4 and 12.5.

**Why:** an entry written from memory at the end of a cycle is an entry that is
not written. The agent drafts it; the developer still decides (D-6, D-9).

**Compatibility:** nothing about entries changes. A draft is a new file in the
Plan directory, which no earlier reader looks at.

**1.1.0 — capitalized module names.**

- §2 and §4.1: `module` matches `^[A-Za-z][A-Za-z0-9_]*$` (or `global`).
- §9: `context show` reads all entries, not a module's and then `global`.

**Why:** the project's modules are capitalized (`Billing`), and the Plan header
now accepts that (plan-header.md 1.5.0); an entry could not name the module of
its own Plan. Selecting entries by module was dropped for the MVP: tickets
touch several modules, and the planner is better served by all the facts.

**Compatibility:** every entry and filename that was valid stays valid. The
filename's module is compared with the front matter's exactly, so `billing` and
`Billing` are different modules.

**1.0.0** — initial version (M1-11).
