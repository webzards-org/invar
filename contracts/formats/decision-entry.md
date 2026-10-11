# Contract: Decision entry

**Version:** 1.1.0 — `module` may use capital letters; see §13
**Status:** stable
**Implements:** §7 (Decision log), §4c (a `failed` entry resets the cycle),
§12 (the entry survives the Plan); light mode L-1, L-4 (`docs/TASKS-M1.md`)
**Read by:** the core (`format/decision`), `invar decision log`,
`invar context show` (M1-14), `invar clean-spec` (M1-13), any later
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

Each key appears at most once. A key not listed here is permitted and ignored,
so a newer writer cannot break an older reader.

**Consistency:** `module` equals the filename's `<module>`, and `date`,
converted to UTC, equals the filename's timestamp. The tool always writes them
that way; a reader that checks an entry reports a mismatch.

## 5. Types

The four types of §7:

| Type | Meaning | Written by |
|---|---|---|
| `confirmed` | The cycle completed: what was built, including any deviation from the Plan | `invar done` (M1-12), when it closes a cycle |
| `correction` | The developer corrected generated code, or changed something by hand outside the cycle | `invar decision record`, when the developer asks for it |
| `failed` | The cycle could not be completed, or the approved Plan turned out to be impossible (§4c) | `invar decision record`, when the developer asks for it — and it resets the cycle (§7) |
| `note` | Anything else the developer wants the planner to know next time | `invar decision record`, when the developer asks for it |

## 6. Insert-only

**The tool never modifies or deletes an existing entry.** A new entry is
created with an exclusive create: if the name exists, the write does not
replace it, it picks the next suffix (§2). The developer may edit or delete
entries by hand; the tool neither detects nor cares (§7).

## 7. `failed` resets the cycle

Writing a `failed` entry for a ticket also returns that ticket's Plan to
`draft` (§4c, light-mode form):

1. `Status` becomes `draft`, and `Approved-by`, `Approved-at` and
   `Approved-checksum` become empty. `Lockfile-checksum`, when present,
   becomes empty too. The keys stay (plan-header.md D-3).
2. The body is not touched — nothing is reverted, only de-approved.
3. There are no `.feature` files to reset in light mode.

Without a Plan there is nothing to reset, and the command **refuses**: a
`failed` entry promises that the approval it disproves is gone. A failure
without a Plan is recorded as a `note`.

The two writes happen Plan first, entry second. If the entry cannot be
written, the Plan is restored to the exact bytes it had (D-8).

## 8. Who writes, and when

**An entry is written because a person decided it is worth keeping** (D-6).
In M1:

- `invar done` writes the `confirmed` entry when it closes a cycle (M1-12).
  Closing the cycle is itself the developer's decision; the body says what
  was actually built.
- Every other entry is written by `invar decision record`, run by the
  developer — or by the agent **only when the developer asks it to**. The
  agent may *propose* an entry, including a `failed` one when it finds the
  approved Plan impossible, but it does not write one on its own.
- **No hook ever writes an entry**, and no other command writes one as a side
  effect.

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
Plan, the branch, `clean-spec`'s "every abandoned Plan has an entry" rule
(M1-13). An entry that belongs to no ticket is a fact about the project, and
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

## 13. Version history

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
