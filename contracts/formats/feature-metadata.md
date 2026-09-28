# Contract: `.feature` metadata block

**Version:** 1.0.0
**Status:** stable
**Implements:** §4 (Scenario document structure), §4d (Checksums)
**Read by:** the core (`format/`), the Python adapter (test discovery), any
later reimplementation

> Changing this file is a **public-contract change** (§14.3). It is made
> deliberately, in its own PR, with the reason recorded, the version bumped,
> and every reader updated in the same PR. New optional keys may be added at
> any time under one rule: **an absent key means the previous behavior.**

---

## 1. Scope

A tool-managed Gherkin file: `specs/**/*.feature`, marked with the `@invar`
tag. Files without that tag are hand-written Gherkin used for other purposes in
the same repository, and are not covered by this contract.

This document specifies the **metadata block** and the **header/body
boundary**. It does not specify Gherkin itself: steps are the adapter's
concern, never the core's (§14.2).

## 2. Encoding and line endings

- UTF-8. A leading BOM is permitted and ignored.
- `\n`, `\r\n` and `\r` are all accepted as line terminators when parsing.
- A digest is never computed over the raw bytes. Text is canonicalized first
  (§4d), which is what makes the same content produce the same digest on a
  mixed Windows/Unix team.
- `init` writes a `.gitattributes` entry marking `*.feature` as
  `text eol=lf`, so the working tree and the index agree.

## 3. File structure

Exactly five regions, in this order:

```
[1] preamble        optional   comment and blank lines
[2] tag line        required   @invar
[3] Feature line    required   Feature: <title>
[4] metadata block  required   Key: value lines
[5] body            required   everything else
```

### 3.1 Preamble

Zero or more lines, each either blank or a comment (first non-whitespace
character is `#`). Conventionally one comment giving the file's own path.

The preamble is **not** part of the body and therefore does not affect the
digest. A developer may add or remove a path comment without invalidating a
confirmation.

### 3.2 Tag line

The first non-blank, non-comment line must be exactly `@invar` after trimming
surrounding whitespace. No other tags on that line in version 1.0.0.

### 3.3 Feature line

The next non-blank line must match `Feature:` after trimming leading
whitespace. Indentation is permitted and ignored — Gherkin allows it, and a
file that renders correctly in an editor must not be rejected here. The title
is the trimmed remainder and must be non-empty.

### 3.4 Metadata block

The **maximal run** of lines immediately following the Feature line in which
every line, after trimming leading and trailing whitespace, matches:

```
<Key>:<optional whitespace><optional value>
```

Rules:

- `<Key>` is matched **case-sensitively** against the table in §4.
- A line whose key is not in that table is an **error**, not the start of the
  body. This is the single most important rule in this document: if an
  unrecognized key ended the block instead, a typo (`Tickett:`) would silently
  move the header/body boundary and change the digest of an already-confirmed
  file. Failing loudly is the only safe behavior.
- A duplicated key is an error.
- Blank lines inside the block are permitted and ignored; they do not end it.
- Comment lines inside the block are **not** permitted (see §6, D-3).
- The value is the trimmed remainder of the line. `#` in a value is an error
  (D-3).
- Key order is free. A parser must not depend on it, and a writer must preserve
  the order it found.

### 3.5 Body

Everything from the **first non-blank line after the metadata block** to the
end of the file.

## 4. Fields

| Field | Required | Allowed values | Written by |
|---|---|---|---|
| `Module` | yes | A module name: `^[a-z][a-z0-9_]*$`, or `global` | Agent |
| `Status` | yes | `draft` or `confirmed`, lowercase, exactly | `scenario confirm` sets `confirmed`; `decision record --type failed` sets `draft`; the agent writes `draft` on creation |
| `Ticket` | yes | `^[A-Z][A-Z0-9]*-[0-9]+$`, e.g. `PROJ-412` | Agent |
| `Confirmed-by` | only when `Status: confirmed` | Non-empty; a git `user.name` | **Command only** (`scenario confirm`) |
| `Confirmed-at` | only when `Status: confirmed` | `YYYY-MM-DD` | **Command only** (`scenario confirm`) |
| `Confirmed-checksum` | only when `Status: confirmed` | `sha256:` + 64 lowercase hex | **Command only** (`scenario confirm`) |

**"Command only" is a protocol rule, not an enforcement.** Nothing inside a
repository can stop an agent from writing `Status: confirmed` and a checksum
itself. What makes the ordinary failure mode detectable is that `scenario
confirm` refuses without an approved Plan, and that the checksum must match the
body at verification time (§4a, honesty note). An agent that fills these in is
out of protocol.

### 4.1 Consistency rules

- `Status: draft` — the three `Confirmed-*` fields must be **absent or
  empty**. A present, non-empty value alongside `draft` is an error: it means a
  previous confirmation was de-approved without being cleared (§4c).
- `Status: confirmed` — all three must be present and non-empty.
- `Ticket` records which cycle last confirmed this file. It is not history; the
  permanent record of why behavior changed is the decision log (§7).

## 5. Body rules

- At least one `Scenario:` line (after trimming leading whitespace).
- Each `Scenario:` block maps to exactly one generated test function; no
  scenario may span multiple unrelated behaviors. Only the count is
  mechanically checkable — the "one behavior" part is a review concern.
- Plain Gherkin only in version 1.0.0: `Scenario:`, `Given`, `When`, `Then`,
  `And`, `But`. `Scenario Outline:`, `Examples:` and `Background:` are **not**
  supported and are an error (D-4).

## 6. Decisions and their reasons

The rules above resolve ambiguities the design document left open. Each is
recorded here because a future reader will otherwise re-litigate it.

**D-1 — The body starts at the first *non-blank* line after the block.**
Blank lines between the metadata block and the body are not part of the body.
Otherwise adding or removing one blank line changes the digest of a confirmed
file, and canonicalization cannot help: §4d strips *trailing* blank lines, not
leading ones. Under this rule, cosmetic whitespace between header and body is
free; whitespace inside the body is not, which is correct, since the body is
the acceptance criterion.

**D-2 — An unknown key is an error.** See §3.4. Loud failure over a silently
moved boundary.

**D-3 — No comments and no `#` inside the metadata block.** §4's own example
in the design document shows an inline annotation
(`Confirmed-checksum: sha256:1f4a…    # written by scenario confirm`); that is
documentation, not a permitted form. Allowing it would require deciding whether
`#` inside a value is a comment or data — and a value that can be truncated by
a comment marker is a value that can be silently wrong. Comments belong in the
preamble.

**D-4 — No `Scenario Outline:` in 1.0.0.** An outline expands to several test
functions from one block, which contradicts the one-scenario-to-one-test rule
(§4b) and makes "which scenario failed" ambiguous. Supporting it is a real
feature with a real design, not a parser tweak, and adding it later is
non-breaking.

**D-5 — `Ticket` is uppercase with a numeric suffix.** The id is the join key
across the Plan directory name, the branch and the decision entry, all matched
by plain substring (§4a, §4b, §12). A permissive pattern makes those matches
ambiguous; JIRA-style ids are the overwhelming case.

## 7. Open questions

**OQ-1 — `Confirmed-at` is a date, `Approved-at` is a timestamp.** §4 shows
`2026-08-03` for a scenario and §7 shows `2026-08-14T09:30:12` for a decision
entry. A date is enough for a human record and a timestamp is enough for
ordering, so neither is wrong, but the asymmetry is arbitrary. Decide before
M1-9 and M1-10 write these fields; changing it afterwards is a contract change.

## 8. Examples

### 8.1 Valid — confirmed

```gherkin
# specs/orders/reserve-stock.feature

@invar
Feature: Reserve stock on order placement
  Module: orders
  Status: confirmed
  Ticket: PROJ-412
  Confirmed-by: rade
  Confirmed-at: 2026-08-03
  Confirmed-checksum: sha256:1f4ac0de5b7e4d2f8a9c3b1e6d0f7a25c8b4e93f1a6d2c07b5e8f43a9d1c6b20

  Scenario: Stock is reserved when an order is placed
    Given a Product with sku "ABC-123" and stock 10
    When a customer places an order for 3 units of "ABC-123"
    Then the Product stock for "ABC-123" is 7
    And an OrderCreated event is emitted
```

The digest is computed over the body, which begins at
`  Scenario: Stock is reserved…` — not at the blank line above it (D-1).

### 8.2 Valid — draft

```gherkin
@invar
Feature: Refund a settled payment
  Module: payments
  Status: draft
  Ticket: PROJ-418

  Scenario: A settled payment is refunded in full
    Given a settled Payment of 100.00 EUR
    When an operator refunds it in full
    Then the Payment status is "refunded"
```

No `Confirmed-*` fields, which is required for a draft (§4.1).

### 8.3 Invalid — misspelled key

```gherkin
@invar
Feature: Reserve stock on order placement
  Module: orders
  Status: confirmed
  Tickett: PROJ-412
  Confirmed-by: rade
```

**Why:** `Tickett` is not a recognized key (D-2), and the required `Ticket`
field is therefore missing. Reported as an unrecognized key naming the line —
never treated as the first line of the body.

### 8.4 Invalid — confirmed without a checksum

```gherkin
@invar
Feature: Reserve stock on order placement
  Module: orders
  Status: confirmed
  Ticket: PROJ-412
  Confirmed-by: rade
  Confirmed-at: 2026-08-03

  Scenario: Stock is reserved when an order is placed
    Given a Product with sku "ABC-123" and stock 10
```

**Why:** `Status: confirmed` requires all three `Confirmed-*` fields (§4.1).
Without a checksum there is nothing for `generate` to verify the body against,
so the file claims an approval that cannot be checked — exactly the state the
gate exists to make impossible.
