# Contract: Plan header and required sections

**Version:** 1.0.0
**Status:** stable
**Implements:** §4a (Plan document structure), §4d (Checksums), §5c (Context
delivery), §12 (Plan lifecycle)
**Read by:** the core (`format/`), the Codegen Agent (as prose), any later
reimplementation

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
required sections**, since those are what the core and the Codegen Agent depend
on.

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
- Lookup by ticket is a substring match over directory names. Zero matches and
  more than one match are distinct, loud errors — never a guess.

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

YAML mapping, one key per line, scalar values only. In version 1.0.0 no nested
structures, lists or multi-line values are used; a parser may reject them.

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
| `Approved-at` | key present always; value only when `Status: approved` | RFC 3339 timestamp with offset, e.g. `2026-08-14T09:30:12+02:00` | **Command only** (`plan approve`) |
| `Approved-checksum` | key present always; value only when `Status: approved` | `sha256:` + 64 lowercase hex, over the **body** | **Command only** (`plan approve`) |
| `Lockfile-checksum` | key present always; value optional even when approved | `sha256:` + 64 lowercase hex, or empty | **Command only** (`plan approve`) |

**The four command-written keys must be present as keys from the moment the
agent creates the file**, with empty values. A missing key and an empty value
would otherwise be indistinguishable, and `plan validate` could not tell a
Plan that follows the convention from one that forgot half of it.

**"Command only" is a protocol rule, not an enforcement.** A misbehaving agent
can write `Status: approved` itself. What makes the ordinary failure mode
detectable is `Approved-checksum`: if the body changes after approval, the
approval is stale and `scenario confirm` refuses (§4a, honesty note).

### 5.1 Consistency rules

- `Status: draft` — all four command-written values must be empty.
- `Status: approved` — `Approved-by`, `Approved-at` and `Approved-checksum`
  must be non-empty. `Lockfile-checksum` may be empty: a project with no
  lockfile is a valid state, not an error (M1-8).
- `Ticket` must equal the `<TICKET>` part of the directory name.
- Every `.feature` file confirmed against this Plan carries the same `Ticket`.

### 5.2 `Lockfile-checksum`

The canonical digest (§4d) of the project's lockfile at approval time. Which
file that is, is an adapter concern (§14.2); until M2 the path is declared in
`invar.yaml`.

It sits in the **header**, so it is not covered by `Approved-checksum`. That is
intentional: it records an observation made at approval time, not part of the
approved specification.

Drift is a **warning, never a failure** (§4a). A dependency upgraded mid-cycle
or a rebase onto a branch that upgraded one makes the Implementation context
describe a version that is no longer installed; the tests judge whether the code
works against what is actually installed, and a blocking check here would fire
on every routine rebase.

## 6. Required sections

Eight headings, each written exactly as shown, each at level 2, in this order:

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

Order is fixed so a reviewer reads every Plan the same way and so a diff
between two Plans is meaningful.

- Heading text is matched **exactly**, case-sensitively, after trimming
  trailing whitespace. `## Scenarios To Write` is an error.
- Additional sections beyond these eight are permitted, anywhere, and ignored
  by the core.
- A required section may be empty of prose only where §6.1 says so.
- `plan validate` reports **all** missing or misspelled headings in one run,
  not one per invocation. A developer fixing a Plan should need one round trip.

### 6.1 What each section must contain

| Section | Content | May be empty |
|---|---|---|
| `## Ticket` | The pasted ticket text, verbatim | No |
| `## Intent` | What the change is meant to achieve, in the developer's terms | No |
| `## Scope` | Modules and layers touched; what is explicitly out of scope | No |
| `## Open questions / answers` | The clarifying questions of Phase 1 and the developer's answers | Yes — with an explicit line saying none were needed |
| `## Rejected alternatives` | Approaches considered and dropped, with the reason | Yes — same |
| `## Scenarios to write` | A machine-readable list (§6.2) | No |
| `## Scenarios to delete` | A machine-readable list (§6.2) | Yes — empty means nothing is deleted |
| `## Implementation context` | Ticket-specific guidance for Phase 2 (§6.3) | Yes — with an explicit line saying no context source applies |

`## Open questions / answers` and `## Rejected alternatives` are the sections
that justify the Plan's existence: everything else could be inferred from the
ticket and the diff, and these two cannot.

### 6.2 List sections — machine-readable form

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
`context show` labels them (§5b).

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

## 7. Decisions and their reasons

**D-1 — The body starts at the first non-blank line after the closing
delimiter.** Same reasoning as in `feature-metadata.md`: canonicalization
strips trailing blank lines, not leading ones (§4d), so counting them would let
a cosmetic edit invalidate an approval.

**D-2 — Unknown header keys are preserved, not rejected.** The header is
delimited, so an unknown key cannot move the boundary. This differs from the
`.feature` rule for that reason alone, and the difference is recorded here so
neither side looks like an oversight.

**D-3 — The command-written keys are present from creation, with empty
values.** Otherwise `plan validate` cannot distinguish convention from
omission, and `plan approve` would have to decide whether to insert or update a
key — two code paths where one will do.

**D-4 — Section order is fixed.** The Plan is read by a human at the gate and
by a reviewer in the PR diff. A stable order is what makes that reading fast
enough to actually happen.

**D-5 — `_none_` rather than an absent line.** An empty section and a
forgotten section must not look the same. This matters most for
`## Scenarios to delete`, where "nothing to delete" and "the agent did not
consider deletions" have very different consequences (§4c).

## 8. Open questions

**OQ-1 — `Approved-at` is a timestamp, `Confirmed-at` is a date.** See
`feature-metadata.md`, OQ-1. Decide once, before M1-9 and M1-10.

**OQ-2 — Re-approving an already approved Plan.** `plan approve` on a Plan
whose body changed after approval: refuse and require an explicit flag, or
re-approve and issue a new checksum? The safe default is to refuse, since a
silent re-approval is indistinguishable from the failure mode the checksum
exists to catch. Decide in M1-9.

## 9. Examples

### 9.1 Valid — draft, as the agent creates it

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
PROJ-412 — Reserve stock when an order is placed, so two customers cannot buy
the same unit. Stock must be locked before payment is attempted.

## Intent
Reserve stock at order placement rather than at payment, and reject an order
that cannot be satisfied from available stock.

## Scope
Touches `orders` (service, repository, router). Payment handling is out of
scope; `payments` is not modified.

## Open questions / answers
- Q: Does a rejected order leave any record? A: No — it is rejected before
  persistence.
- Q: Which country does the billing cutoff apply to here? A: Not applicable,
  this ticket does not touch billing.

## Rejected alternatives
- Reserving on `paid`: leaves a window in which two customers can pay for the
  same unit.
- A nightly reconciliation job: hides the race rather than preventing it.

## Scenarios to write
- specs/orders/reserve-stock.feature

## Scenarios to delete
_none_

## Implementation context
- AI_CONTEXT: no SQLAlchemy imports in `router.py`; the service layer owns the
  transaction boundary
- BUSINESS_CONTEXT invariant: every tenant-scoped query filters by
  `account_id`, via `fastkit_core.database.scopes.AgencyScope`
- pydantic 2.7.1 (published 2.7.0, compatible): use `@field_validator`, not
  `@validator`
```

### 9.2 Valid — the same Plan after `plan approve`

Only the header changes; the body is untouched, which is what makes
`Approved-checksum` verifiable later.

```markdown
---
Ticket: PROJ-412
Module: orders
Status: approved
Approved-by: rade
Approved-at: 2026-08-14T09:30:12+02:00
Approved-checksum: sha256:1f4ac0de5b7e4d2f8a9c3b1e6d0f7a25c8b4e93f1a6d2c07b5e8f43a9d1c6b20
Lockfile-checksum: sha256:9b8c7d6e5f4a3b2c1d0e9f8a7b6c5d4e3f2a1b0c9d8e7f6a5b4c3d2e1f0a9b87
---
```

### 9.3 Invalid — approved with no checksum

```markdown
---
Ticket: PROJ-412
Module: orders
Status: approved
Approved-by: rade
Approved-at: 2026-08-14T09:30:12+02:00
Approved-checksum:
Lockfile-checksum:
---
```

**Why:** `Status: approved` requires a non-empty `Approved-checksum` (§5.1).
Without it nothing can detect a body edited after approval, so the Plan asserts
an approval that cannot be checked — which is the exact state §4a's honesty
note exists to prevent. Most likely cause: an agent writing the status itself
instead of running `plan approve`.

### 9.4 Invalid — missing section, and an ambiguous empty list

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
