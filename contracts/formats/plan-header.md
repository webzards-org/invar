# Contract: Plan header and required sections

**Version:** 1.2.1 — which command checks which rule; see §10 for what changed and why
**Status:** stable
**Implements:** §4a (Plan document structure), §4d (Checksums), §5c (Context
delivery), §12 (Plan lifecycle); light mode L-1, L-2, L-5
(`docs/TASKS-M1.md`)
**Read by:** the core (`format/`), the planner and coder agents (as prose),
`/invar:review` (M1-20), any later reimplementation

> Changing this file is a **public-contract change** (§14.3). It is made
> deliberately, in its own PR, with the reason recorded, the version bumped,
> and every reader updated in the same PR. New optional keys and new optional
> sections may be added at any time under one rule: **an absent key means the
> previous behavior.**

---

## 1. Scope

One file: `.invar/plans/<TICKET>-<slug>/spec.md`.

The Plan is **written by the agent**, not by `invar`. There is no `plan new`
command: the core validates and approves, it does not author. Consequently the
body is deliberately loosely specified — how a Plan reads varies with the agent
and the ticket. What this contract fixes is the **header** and the **minimum
required sections**, since those are what the core and the coder depend on.

Which sections are required depends on `scenarios` in `invar.yaml`
(`invar-yaml.md` §3.1). **Light mode** — `scenarios: false`, and the default —
is what M1 implements: acceptance criteria live in the Plan, as prose, and no
`.feature` files exist (L-1). The rules for `scenarios: true` stay in this
contract, marked as such, for the milestone that introduces `.feature` files.

A Plan is a consumable instruction, not documentation. It is deleted inside the
same PR once its cycle passes (§12), which is why absence is a normal state and
not an integrity failure (§4d).

## 2. Path and directory name

```
.invar/plans/<TICKET>-<slug>/spec.md
```

- `<TICKET>` matches `^[A-Z][A-Z0-9]*-[0-9]+$` — the same pattern as the
  `.feature` `Ticket` field.
- `<slug>` matches `^[a-z0-9]+(-[a-z0-9]+)*$`, non-empty.
- The file is always named `spec.md`. Other files may sit beside it; the core
  reads only `spec.md`.
- The directory name is the join key across the Plan, the branch and the
  decision entry, matched by plain substring (§4a, §12). `plan validate` fails
  loudly when the shape does not match.
- Lookup by ticket is a substring match over directory names, on **whole
  segments**: the query must not have a letter or digit directly before or
  after it in the name (D-10). `PROJ-412`, `412` and `reserve-stock` all find
  `PROJ-412-reserve-stock`; `PROJ-41` does not. Matching is case-sensitive.
- Zero matches, more than one match, a single match whose name does not have
  the shape above, and a single match without a `spec.md` are distinct, loud
  errors — never a guess. A malformed name that matches counts as a match:
  next to a well-formed one, the lookup is ambiguous, not resolved by
  preferring either.

## 3. Encoding and line endings

As for `.feature` files: UTF-8, optional BOM ignored, all three line
terminators accepted when parsing, digests computed over canonicalized text
(§4d). `init` writes a `.gitattributes` entry marking
`.invar/plans/**/*.md` as `text eol=lf`.

## 4. File structure

```
[1] opening delimiter   required   --- as the first line
[2] YAML header         required   Key: value lines
[3] closing delimiter   required   ---
[4] body                required   the required sections
```

### 4.1 Delimiters

- The **first line of the file** — after stripping a BOM — must be exactly
  `---`. Leading blank lines are an error: permitting them would mean two
  implementations could disagree about where the header starts.
- The header ends at the **first subsequent line** that is exactly `---`.
- Any later `---` in the file is ordinary body content (a markdown horizontal
  rule), not a delimiter. Only the first two count.

### 4.2 Header

YAML mapping, one key per line, scalar values only. No nested structures,
lists or multi-line values are used; a parser may reject them.

- Keys are matched **case-sensitively** against §5.
- Unknown keys are **permitted and preserved on write.** This is the opposite
  of the `.feature` rule (D-2 there) and deliberately so: the header is
  delimited, so an unknown key cannot move the header/body boundary. Nothing is
  at risk, and tolerance here costs nothing.
- A duplicated key is an error.
- Comments (`# …`) are permitted and preserved.
- A key may be present with an empty value; §5 says where that is required.

### 4.3 Body

Everything from the **first non-blank line after the closing delimiter** to the
end of the file. Blank lines between the delimiter and the first heading are not
part of the body (see D-1).

## 5. Header fields

| Field | Required | Allowed values | Written by |
|---|---|---|---|
| `Ticket` | yes | `^[A-Z][A-Z0-9]*-[0-9]+$`; must match the directory name | Agent |
| `Module` | yes | `^[a-z][a-z0-9_]*$`, or `global` | Agent |
| `Status` | yes | `draft` or `approved`, lowercase, exactly | `plan approve` sets `approved`; `decision record --type failed` sets `draft`; the agent writes `draft` on creation |
| `Approved-by` | key present always; value only when `Status: approved` | Non-empty; a git `user.name` | **Command only** (`plan approve`) |
| `Approved-at` | key present always; value only when `Status: approved` | `YYYY-MM-DD` | **Command only** (`plan approve`) |
| `Approved-checksum` | key present always; value only when `Status: approved` | `sha256:` + 64 lowercase hex, over the **body** | **Command only** (`plan approve`) |
| `Lockfile-checksum` | **optional** since 1.1.0 — absent and empty both mean no observation (§5.2) | `sha256:` + 64 lowercase hex, or empty | **Command only** (`plan approve`, from the milestone that introduces package context; never in light mode) |

**The three approval keys — `Approved-by`, `Approved-at` and
`Approved-checksum` — must be present as keys from the moment the agent creates
the file**, with empty values (D-3). A missing key and an empty value would
otherwise be indistinguishable, and `plan validate` could not tell a Plan that
follows the convention from one that forgot half of it. `Lockfile-checksum` is
exempt: it may be left out entirely (D-7).

**"Command only" is a protocol rule, not an enforcement.** A misbehaving agent
can write `Status: approved` itself. What makes the ordinary failure mode
detectable is `Approved-checksum`: if the body changes after approval, the
approval is stale, and `plan check` refuses — and with it `invar done`
(M1-10, M1-12). With `scenarios: true`, `scenario confirm` refuses as well
(§4a, honesty note).

### 5.1 Consistency rules

- `Status: draft` — the three approval values must be empty, and so must
  `Lockfile-checksum` when the key is present.
- `Status: approved` — `Approved-by`, `Approved-at` and `Approved-checksum`
  must be non-empty. `Lockfile-checksum` may be absent, empty, or a digest.
- `Ticket` must equal the `<TICKET>` part of the directory name.
- *When `scenarios: true`:* every `.feature` file confirmed against this Plan
  carries the same `Ticket`.

**Form and approval are checked by different commands** (D-11).
`plan validate` checks **form**: the path and directory name (§2), encoding
and structure (§3, §4), the presence of every required key and the format of
every non-empty value (§5), `Ticket` against the directory name (above), and
the required sections (§6). It does not look at whether the Plan is approved:
the first two rules above — the approval values against `Status` — are
checked by `plan check` (M1-10), which `invar done` runs, and by
`plan approve` before it approves (M1-9). A draft with empty approval values
is therefore valid, and so is the Plan of example 9.3 as far as
`plan validate` is concerned.

### 5.2 `Lockfile-checksum`

**Optional since 1.1.0 (L-5).** Light mode neither writes nor reads it: it
records an observation that only matters once package context exists (§5b),
and no M1 command produces package context. A Plan without the key, and a Plan
with the key and an empty value, both mean the same thing — no lockfile
observation was made.

When written, it is the canonical digest (§4d) of the project's lockfile at
approval time. Which file that is, is an adapter concern (§14.2).

It sits in the **header**, so it is not covered by `Approved-checksum`. That is
intentional: it records an observation made at approval time, not part of the
approved specification.

Drift is a **warning, never a failure** (§4a). A dependency upgraded mid-cycle
or a rebase onto a branch that upgraded one makes the Implementation context
describe a version that is no longer installed; the tests judge whether the code
works against what is actually installed, and a blocking check here would fire
on every routine rebase.

## 6. Required sections

The required sections depend on `scenarios` in `invar.yaml`. Each is a level-2
heading written exactly as shown, and they appear in the order shown.

**Light mode — `scenarios: false` (and absent).** Seven headings:

```markdown
## Ticket
## Intent
## Scope
## Open questions / answers
## Rejected alternatives
## Acceptance criteria
## Implementation context
```

`## Scenarios to write` and `## Scenarios to delete` are **not required** in
light mode. If a Plan contains them anyway, they are ordinary additional
sections: the core ignores them, and the rules of §6.2 and D-5 do not apply
(D-9).

**`scenarios: true`.** Eight headings, as specified in version 1.0.0:

```markdown
## Ticket
## Intent
## Scope
## Open questions / answers
## Rejected alternatives
## Scenarios to write
## Scenarios to delete
## Implementation context
```

Whether `## Acceptance criteria` is required in this mode as well is open
(OQ-2) and is decided by the milestone that introduces `.feature` files.

**Rules for both modes:**

- Order is fixed so a reviewer reads every Plan the same way and so a diff
  between two Plans is meaningful (D-4).
- Heading text is matched **exactly**, case-sensitively, after trimming
  trailing whitespace. `## Acceptance Criteria` is an error.
- Additional sections beyond the required ones are permitted, anywhere, and
  ignored by the core. Only the relative order of the required ones is checked.
- A required section may be empty of prose only where §6.1 says so.
- `plan validate` reports **all** missing, misspelled or misordered headings in
  one run, not one per invocation. A developer fixing a Plan should need one
  round trip.

### 6.1 What each section must contain

| Section | Content | May be empty |
|---|---|---|
| `## Ticket` | The pasted ticket text, verbatim | No |
| `## Intent` | What the change is meant to achieve, in the developer's terms | No |
| `## Scope` | Modules and layers touched; what is explicitly out of scope | No |
| `## Open questions / answers` | The clarifying questions of Phase 1 and the developer's answers | Yes — with an explicit line saying none were needed |
| `## Rejected alternatives` | Approaches considered and dropped, with the reason | Yes — same |
| `## Acceptance criteria` | What the change will be judged against (§6.4) — light mode | **No** — and `_none_` counts as empty (D-8) |
| `## Scenarios to write` | A machine-readable list (§6.2) — `scenarios: true` only | No |
| `## Scenarios to delete` | A machine-readable list (§6.2) — `scenarios: true` only | Yes — empty means nothing is deleted |
| `## Implementation context` | Ticket-specific guidance for Phase 2 (§6.3) | Yes — with an explicit line saying no context source applies |

`## Open questions / answers` and `## Rejected alternatives` are the sections
that justify the Plan's existence: everything else could be inferred from the
ticket and the diff, and these two cannot.

### 6.2 List sections — machine-readable form (`scenarios: true` only)

**Applies when `scenarios: true`.** In light mode these sections are not
required and, if present, not parsed (§6, D-9).

The core parses `## Scenarios to write` and `## Scenarios to delete`; the bulk
form of `scenario confirm` and the deletion check of §4c depend on them.

One item per line, no nesting, no blank lines inside the list:

```
- <path>
- <path> — <reason>
```

- `<path>` is a `.feature` path **relative to the project root**, starting
  with `specs/`, as the first token after `- `.
- In `## Scenarios to delete` a reason is required, separated by a space, an em
  dash and a space. In `## Scenarios to write` anything after the path is
  ignored.
- An empty list is expressed as a single line: `_none_`. An absent line is an
  error, since "no items" and "the agent forgot the section" must be
  distinguishable.

`scenario confirm --plan <ticket>` confirms every draft scenario this list
names and **refuses on any mismatch, in either direction**, between the list
and the files on disk. That check is what keeps the bulk form honest: it cannot
confirm a scenario the approved Plan never announced.

### 6.3 `## Implementation context`

The ticket-specific distillate of all context sources (§5c). One item per line:

```
- <source> <version-info>: <guidance>
```

with `<source>` one of `AI_CONTEXT`, `BUSINESS_CONTEXT`, a package name, or the
framework name, and `<version-info>` — required for external context — giving
the installed version, the published version and the match class exactly as
`context show` labels them (§5b). External context is out of scope for M1, so
in light mode every item names `AI_CONTEXT` or `BUSINESS_CONTEXT` and carries
no version information.

The core does not parse the guidance text; it checks only that the section
exists and is non-empty. The form is fixed anyway so a reviewer can see at a
glance where a rule came from.

**A distillate, never a copy.** Only what this ticket needs. A Plan that pastes
whole context files in defeats both purposes of the section: it spends the token
budget twice, and it becomes too long to be genuinely read at the gate — which
is how a gate turns into a click.

Because the section is part of the body, `Approved-checksum` covers it: the
guidance Phase 2 follows is guidance a human approved, frozen with the rest of
the specification.

### 6.4 `## Acceptance criteria`

New in 1.1.0. **Required in light mode, and never empty.** It states what the
change will be judged against: by the developer approving the Plan, by the
coder in Phase 2, by `/invar:review` before the PR (M1-20), and later by the
`.feature` files written from it (L-1).

**Non-empty.** The section must contain at least one non-blank line other than
`_none_`. A Plan with nothing to accept cannot be reviewed against, cannot be
approved meaningfully — there is nothing for the person at the gate to agree
to — and cannot be turned into tests later. Unlike `## Scenarios to delete`,
"nothing" is never a valid answer here, so `_none_` is not an escape (D-8).

**Recommended form: one criterion per list item, in Given/When/Then prose.**

```markdown
- Given <the starting state>, when <the action>, then <the observable outcome>.
```

A list item may wrap across lines. Each criterion should describe one
behavior, observable from outside the code — a response, a stored state, what
a user sees — so that it can be checked without reading the implementation.

**The core does not parse it.** `plan validate` checks only that the heading is
present, in order, and that the section is non-empty (M1-8). The form is fixed
anyway, for the readers that depend on it:

- **The developer at the gate.** `plan approve` prints the criteria before
  approving (M1-9). They are the part of the Plan the code will be held to, so
  they are the part a person must actually read.
- **`/invar:review`** reports each criterion as `met`, `not met` or `unclear`,
  with evidence (M1-20). One criterion per item is what lets it answer per
  criterion instead of for the section as a whole.
- **The PR description** carries them to the reviewer (M1-16).
- **The later conversion into `.feature` files** (L-1). One Given/When/Then
  criterion becomes one `Scenario:`; a criterion written that way converts
  without reinterpretation.

**Invariants need a criterion that would catch their violation.** Every
`BUSINESS_CONTEXT` invariant named in `## Implementation context` should have
at least one acceptance criterion that would fail if the invariant were broken
— the criterion that would have caught the incident the invariant exists to
prevent. **This is guidance to the planner, not a rule the core checks.**
Matching prose criteria to prose invariants is a judgement; a mechanical check
could only look for shared words, which a planner would learn to satisfy
without covering anything. Where a criterion is missing, `/invar:review` is
where it surfaces: it reports invariants the diff appears to break (M1-20).

Because the section is part of the body, `Approved-checksum` covers it: the
criteria are frozen at approval exactly as a confirmed scenario would be (L-1).

## 7. Decisions and their reasons

**D-1 — The body starts at the first non-blank line after the closing
delimiter.** Same reasoning as in `feature-metadata.md`: canonicalization
strips trailing blank lines, not leading ones (§4d), so counting them would let
a cosmetic edit invalidate an approval.

**D-2 — Unknown header keys are preserved, not rejected.** The header is
delimited, so an unknown key cannot move the boundary. This differs from the
`.feature` rule for that reason alone, and the difference is recorded here so
neither side looks like an oversight.

**D-3 — The three approval keys are present from creation, with empty
values.** `Approved-by`, `Approved-at` and `Approved-checksum`. Otherwise
`plan validate` cannot distinguish convention from omission, and `plan
approve` would have to decide whether to insert or update a key — two code
paths where one will do. *Changed in 1.1.0:* this rule covered four keys;
`Lockfile-checksum` left it when it became optional (D-7).

**D-4 — Section order is fixed.** The Plan is read by a human at the gate and
by a reviewer in the PR diff. A stable order is what makes that reading fast
enough to actually happen.

**D-5 — `_none_` rather than an absent line** (`scenarios: true`). An empty
section and a forgotten section must not look the same. This matters most for
`## Scenarios to delete`, where "nothing to delete" and "the agent did not
consider deletions" have very different consequences (§4c).

**D-6 — Dates, not timestamps.** `Approved-at` and `Confirmed-at` are both
`YYYY-MM-DD`. These fields are read by a developer checking when an approval
happened, not by a machine ordering events — the decision log's timestamped
filenames already carry ordering (§7). A date is what a human can verify
against their own memory of the day; the extra precision would be there for a
reader that does not exist.

**D-7 — `Lockfile-checksum` is optional, and may be absent** (1.1.0). It
records an observation that only matters once package context exists (L-5). A
key that every Plan must carry and no command ever fills is noise the planner
has to remember and the reviewer has to skip. Absent and empty mean the same
thing, so a 1.0.0 header with the empty key is still valid; the D-3 argument
does not apply, because no command needs to tell "forgotten" from "not yet
written" for a key no command writes.

**D-8 — `## Acceptance criteria` is never empty, and `_none_` does not count**
(1.1.0). D-5 gives list sections a way to say "nothing" because nothing can be
a correct answer there. Here it cannot: a change with no observable outcome to
accept is either not ready for approval or does not need a Plan at all (L-9).
Accepting `_none_` would turn the one section the gate depends on into a
formality.

**D-9 — In light mode, the scenario sections are ignored, not rejected**
(1.1.0). A Plan written from an older planner prompt, or by a team preparing
for `.feature` files, may carry them. Rejecting them would fail a Plan for
containing extra information; parsing them would enforce rules for files that
do not exist in this mode. They are ordinary additional sections, which is
also what makes switching modes later a change of required headings, not of
parsing.

**D-10 — Lookup matches whole segments** (1.2.0). Plain substring matching
lets `PROJ-41` find `PROJ-412-reserve-stock`. When `PROJ-41` has a Plan of its
own the lookup is ambiguous and fails loudly; when it has none, the lookup
returns `PROJ-412`'s Plan with complete confidence — one specification
approved, another implemented, and no ambiguity check can catch it because
there is only one match. Requiring a non-alphanumeric character (or the end of
the name) on both sides of the match closes that hole and keeps everything
substring matching is used for: a full ticket id, its number alone, or a word
of the slug.

**D-11 — `plan validate` checks form, not approval** (1.2.1). The validator
runs while the Plan is being written — by the planner after each draft and by
the hook when the planner stops (M1-8, M1-17). At that point the Plan is a
draft by definition, and the approval values belong to `plan approve`, which
the agent never runs. Checking them there would only ever fire on an agent
that wrote `Status: approved` itself, and the place that has to catch that is
the one that guards the code: `plan check`, run by `invar done`. Keeping the
two apart also means a body edited after approval surfaces as a stale
approval from `plan check`, with its own wording, instead of as a form error.

## 8. Open questions

**OQ-1 — Re-approving an already approved Plan.** `plan approve` on a Plan
whose body changed after approval: refuse and require an explicit flag, or
re-approve and issue a new checksum? The safe default is to refuse, since a
silent re-approval is indistinguishable from the failure mode the checksum
exists to catch. Decide in M1-9.

**OQ-2 — `## Acceptance criteria` when `scenarios: true`.** Light mode
requires it; the 1.0.0 shape for `scenarios: true` does not have it. Once
`.feature` files exist, the criteria could stay as the human-readable source
the scenarios are written from (L-1), or the scenarios could replace them.
Decide in the milestone that introduces `.feature` files; until then the
`scenarios: true` section list is the 1.0.0 one, unchanged.

## 9. Examples

Examples 9.1, 9.2, 9.3 and 9.5 are light mode. Example 9.4 applies when
`scenarios: true`.

### 9.1 Valid — a light-mode draft, as the agent creates it

A Laravel + Vue ticket. `Lockfile-checksum` is absent, which is valid since
1.1.0 (D-7), and there are no scenario sections.

```markdown
---
Ticket: SHOP-214
Module: orders
Status: draft
Approved-by:
Approved-at:
Approved-checksum:
---

## Ticket
SHOP-214 — As a customer I want to cancel an order that has not been paid
yet, so I don't have to contact support. Paid orders go through the refund
flow, not through this button.

## Intent
Let customers cancel their own unpaid orders from the order page. Cancelling
releases the reserved stock and keeps the order in the history as
`cancelled`.

## Scope
Backend: `App\Actions\Orders\CancelOrder`, `OrderPolicy::cancel`, the route
`POST /orders/{order}/cancel` and its controller method. Frontend: a Cancel
button on `resources/js/Pages/Orders/Show.vue`, shown only for cancellable
orders. Out of scope: refunds, cancellation by staff, e-mail notifications.

## Open questions / answers
- Q: Can an order be cancelled once it has been paid? A: No. Only `pending`,
  unpaid orders; paid orders go through the refund flow.
- Q: Is a cancelled order deleted? A: No. It stays, with status `cancelled`,
  for the customer's history and for reporting.

## Rejected alternatives
- Soft-deleting the order: removes it from the customer's history and from
  reporting.
- Releasing the stock in a queued job: leaves a window in which the stock is
  neither reserved nor sellable.

## Acceptance criteria
- Given a pending, unpaid order of mine, when I cancel it, then its status is
  `cancelled` and its reserved stock is available for sale again.
- Given a paid order of mine, when I try to cancel it, then the request is
  refused with 403 and the order is unchanged.
- Given an order that belongs to another team, when I send a cancel request
  with its id, then the response is 404 and the order is unchanged.
- Given an order I cancelled, when I open my order history, then it is listed
  with status `cancelled`.

## Implementation context
- AI_CONTEXT: controllers stay thin — the change lives in
  `App\Actions\Orders\CancelOrder`, and authorization goes through
  `OrderPolicy`, never through inline checks
- AI_CONTEXT: stock changes go through `StockLedger`, inside the action's
  database transaction
- BUSINESS_CONTEXT invariant: every query on team data is scoped to the
  current team by the `BelongsToTeam` global scope; request handling never
  calls `withoutGlobalScopes()`
```

The third criterion is the one the invariant asks for (§6.4): it fails if the
cancel route loads the order without the team scope — exactly the incident the
invariant exists to prevent. The other criteria come from the ticket and the
answers in `## Open questions / answers`.

### 9.2 Valid — the same Plan after `plan approve`

Only the header changes; the body is untouched, which is what makes
`Approved-checksum` verifiable later. Light mode writes no
`Lockfile-checksum`.

```markdown
---
Ticket: SHOP-214
Module: orders
Status: approved
Approved-by: rade
Approved-at: 2026-10-05
Approved-checksum: sha256:1f4ac0de5b7e4d2f8a9c3b1e6d0f7a25c8b4e93f1a6d2c07b5e8f43a9d1c6b20
---
```

### 9.3 Invalid — approved with no checksum

```markdown
---
Ticket: SHOP-214
Module: orders
Status: approved
Approved-by: rade
Approved-at: 2026-10-05
Approved-checksum:
---
```

**Why:** `Status: approved` requires a non-empty `Approved-checksum` (§5.1).
Without it nothing can detect a body edited after approval, so the Plan asserts
an approval that cannot be checked — which is the exact state §4a's honesty
note exists to prevent. Most likely cause: an agent writing the status itself
instead of running `plan approve`. Reported by `plan check`; `plan validate`
checks form only and accepts it (§5.1, D-11).

### 9.4 Invalid (`scenarios: true`) — missing section, and an ambiguous empty list

```markdown
---
Ticket: PROJ-412
Module: orders
Status: draft
Approved-by:
Approved-at:
Approved-checksum:
Lockfile-checksum:
---

## Ticket
PROJ-412 — Reserve stock when an order is placed.

## Intent
Reserve at placement, reject when stock is insufficient.

## Scope
`orders` only.

## Open questions / answers
None were needed.

## Rejected alternatives
None considered.

## Scenarios to write
- specs/orders/reserve-stock.feature

## Scenarios to delete
```

**Why, two reasons:** `## Implementation context` is absent (§6), so Phase 2
would have to redo the research Phase 1 already did — the failure this section
exists to prevent. And `## Scenarios to delete` is empty without `_none_`
(D-5), so "nothing to delete" cannot be distinguished from "deletions were
never considered". Both are reported in the same `plan validate` run.

In light mode the second reason would not apply — the scenario sections are
ignored there (D-9) — but the Plan would still be invalid: it lacks both
`## Acceptance criteria` and `## Implementation context`.

### 9.5 Invalid (light mode) — empty acceptance criteria

The header and every other section as in 9.1; only this part differs:

```markdown
## Rejected alternatives
- Soft-deleting the order: removes it from the customer's history and from
  reporting.

## Acceptance criteria

## Implementation context
- BUSINESS_CONTEXT invariant: every query on team data is scoped to the
  current team by the `BelongsToTeam` global scope; request handling never
  calls `withoutGlobalScopes()`
```

**Why:** `## Acceptance criteria` is present and in order, but empty (§6.4).
There is nothing for the developer to approve the change against, nothing for
`/invar:review` to check, and nothing to write tests from later. Writing
`_none_` under the heading would be invalid in the same way (D-8). `plan
validate` reports it as an empty required section, naming the heading.

## 10. Version history

**1.2.1 — which command checks which rule (M1-8).**

- §5.1 states that `plan validate` checks form and `plan check` checks the
  approval values against `Status` (D-11). Example 9.3 says which command
  rejects it.

**Why:** §5.1 listed every consistency rule without saying who enforces it,
and M1-8 fixes `plan validate` to form only. Without the note, example 9.3
reads as a case `plan validate` must fail.

**Compatibility:** no rule changed; only where each is checked is stated.

**1.2.0 — whole-segment lookup (M1-7).**

- Plan lookup by ticket matches whole segments of the directory name, not any
  substring (§2, D-10). `PROJ-41` no longer finds `PROJ-412-…`.
- The distinct lookup errors are listed in §2: none, several, malformed name,
  missing `spec.md`.

**Why:** implementing the lookup exposed that plain substring matching can
resolve one ticket to another ticket's Plan without any ambiguity to report —
the exact failure the lookup exists to prevent.

**Compatibility:** every query that found a single Plan under 1.1.0 and was a
full ticket id still finds the same Plan. A query that relied on a partial
match inside a segment (`PROJ-41` for `PROJ-412`) now reports no match instead
of a wrong one.

**1.1.0 — light mode (M1-5).**

- Required sections now depend on `scenarios` in `invar.yaml`. Light mode
  (`scenarios: false`, the default) requires seven sections, including the new
  `## Acceptance criteria` (§6.4); `## Scenarios to write` and
  `## Scenarios to delete` are not required there and are ignored when present
  (D-9). Their rules (§6.2, D-5) are unchanged and apply when
  `scenarios: true`.
- `Lockfile-checksum` is optional; absent and empty both mean no observation
  (§5.2, D-7). The "present from creation" rule (D-3) covers three keys instead
  of four.
- New decisions D-7, D-8, D-9 and open question OQ-2. Example 9.1 rewritten as
  a light-mode Laravel ticket, examples 9.2 and 9.3 updated to match, 9.4
  marked as `scenarios: true`, 9.5 added.

**Why:** M1 ships the Plan gate without `.feature` files (L-1), so acceptance
criteria live in the Plan as prose and are frozen by `Approved-checksum`.
Changing the contract before any code reads the Plan means the parser (M1-6)
is written once, against the shape M1 actually uses.

**Compatibility:** a 1.0.0 header — including its empty `Lockfile-checksum` —
stays valid. A 1.0.0 body has no `## Acceptance criteria`, so a 1.0.0 Plan is
invalid in light mode. That is safe for the same reason §14.3 gives for
`## Implementation context`: Plans are consumable and do not outlive their
cycle (§12), and no Plan exists yet that any version of the core has read.

**1.0.0** — initial version.
