# Contract: `BUSINESS_CONTEXT.md`

**Version:** 1.0.0 — first version: what the core already relies on, and invariant ids; see §8
**Status:** stable
**Implements:** §5 (Business context), §5c (Context delivery), M1-14 (`context show`),
#158 (invariant ids)
**Read by:** the core (`core/src/context/`: `context show`; `context/invariants/`:
`plan validate`, `plan approve`, `review-input`, `pr-body`), the planner and reviewer
agents (as prose), any later reimplementation

> Changing this file is a **public-contract change** (§14.3). It is made
> deliberately, in its own PR, with the reason recorded, the version bumped,
> and every reader updated in the same PR. New optional forms may be added at
> any time under one rule: **an absent form means the previous behavior.**

---

## 1. Scope

`BUSINESS_CONTEXT.md`, at the project root, holds the rules of the business that the
application must obey: the words the business uses, the invariants, and the facts that
differ by region or parameter (§5). **The team writes it. Invar never writes its content**:
`invar init` creates the file with the marker, the headings and one comment (§4), and no
command adds, changes or removes a line of it afterwards.

The file is read by the core in two ways:

- **whole**, as binding context: `invar context show --binding` prints it for every
  session, and `--planning` for the planner (§2, §3). Never `--conventions`, which the
  coder reads: the coder meets business rules only as the approved Plan states them (§5c);
- **by invariant id**, under `## Invariants` (§5), so the gate and the review can say which
  invariants a Plan names and which it leaves out.

`AI_CONTEXT.md` follows §2 and §3 too (the same reader), and has no ids.

## 2. The marker

The first non-blank line is exactly:

```
<!-- invar-context: v1 -->
```

A UTF-8 BOM before it, CRLF line endings and spaces around it are allowed. A marker
anywhere else does not count: a document that merely quotes it would otherwise be loaded
as binding context.

| State | Meaning | `context show` | Reported |
|---|---|---|---|
| absent | no such file | nothing | no |
| unrecognized | the first non-blank line is not the marker | nothing: it is not loaded | **yes**, on stderr |
| empty | the marker, then only blank lines, headings and HTML comments | nothing | no |
| loaded | the marker, then anything else | the text after the marker | no |

A file that exists and cannot be read, or is not UTF-8, is an error, never "absent".

*Code:* `core/src/context/mod.rs` (`MARKER`, `classify`, `has_content`).

## 3. The empty-file rule

**An empty file behaves exactly as an absent one**, on stdout and on stderr. `invar init`
scaffolds the file, and writing it must not switch anything on that the team never asked
for. What counts as content is decided line by line: a blank line, an ATX heading (one to
six `#`, then a space or the end of the line) and an HTML comment (`<!-- … -->`, on one
line or across lines) are not content; anything else is. `#hashtag` is content.

The scaffold of §4 is empty by this rule, comment included.

## 4. What `invar init` writes

```markdown
<!-- invar-context: v1 -->
# BUSINESS_CONTEXT.md

## Terms

## Invariants (always true, no exceptions)

<!-- - INV-1: ... -->

## Regional / parameterized facts
```

Only when the file does not exist; an existing file is never touched. The three headings
are the ones the planner is told to read:

- `## Terms` — the business's own words, which the invariants use in that sense (#142);
- `## Invariants …` — rules that hold always, with no exception (§5);
- `## Regional / parameterized facts` — facts that differ by region or setting, which
  oblige the planner to ask which applies.

The comment under `## Invariants` shows the form of an id (§5) and states no rule.
*Code:* `core/src/init/mod.rs` (`business_context`, `INVARIANT_FORM`).

## 5. Invariant ids

### 5.1 The rule

**Each top-level list item under `## Invariants` starts with its id, `INV-<n>:`.**

- `## Invariants` is a level-2 heading whose text is `Invariants`, alone or followed by
  whitespace and more words, as `invar init` writes it. The section runs to the next
  level-1 or level-2 heading; level-3 headings inside it group invariants and are allowed.
  Several such sections are read as one list.
- An id is `INV-`, a positive integer with no leading zero, then `:` and a space (or the end
  of the line). It is the first token of the item, after the list marker (`-`, `*`, `+`,
  `1.` or `1)`).
- **Ids are unique in the file.** Gaps are allowed, and order is free.
- A top-level item is one not indented by two spaces or a tab. **A wrapped line, or a
  sub-item, belongs to the item above it**, which keeps its one id.
- A line of text that is not a list item is not an invariant, and is not read. Items in a
  fenced code block or an HTML comment are not invariants.
- `## Terms` and `## Regional / parameterized facts` have no ids in this version.

**Ids are stable names, like ADR numbers.** A Plan, a decision entry or a pull request may
cite `INV-2`, and the citation must keep meaning the same rule after the file has changed.
So an id is never reused for another rule, and a removed invariant leaves its number
unused. That is why gaps and any order are allowed: renumbering to close a gap would
silently change what every earlier citation means (D-1).

### 5.2 Problems

A file with ids is checked by `plan validate` (`plan-header.md` §6.3, §6.4), and each
problem is named with its line of `BUSINESS_CONTEXT.md`:

- **a duplicate id**: the same `INV-<n>` on two items;
- **a malformed id**: something meant as an id that is not one — `INV-0:`, `INV-01:`,
  `INV-x:`, `inv-1:`, `INV1:`, `INV-1` without the colon, `**INV-1:**`. What is "meant as
  an id" is a first token that starts with `inv` in any case, behind any emphasis, followed
  by anything but a letter; `Invoices are …` is text;
- **a mixed list**: some items with an id and some without. Each item without one is named.
  Coverage cannot be shown honestly for half a list (D-2).

### 5.3 A file without ids

**A file whose items under `## Invariants` carry no id at all behaves as before this
version** (D-3). `plan validate` checks nothing against it, except that a Plan citing an
`INV-<n>` cites one the file does not define; `plan approve` prints one warning that
coverage cannot be shown, and how to add ids; `review-input` and `pr-body` show nothing.
An absent, unrecognized or empty file, or one with no item under the heading, shows
nothing at all, warning included.

### 5.4 What the core reads of an invariant

Its id, its line, and the text of its first line after the id — to show it, never to
interpret it. The core never matches an invariant's words with a Plan's: the id is the
join key (D-4).

*Code:* `core/src/context/invariants/mod.rs` (`parse`, `Invariants`).

## 6. The binding limit

`invar context show --binding` prints both context files with their labels, and refuses
output longer than `context.binding_limit` characters (`invar-yaml.md` §3.4; default 9,000,
at most 10,000), naming each file and its size. The limit is on exactly what is printed,
labels included, counted in characters, because Claude Code moves hook output above 10,000
characters into a file the agent is not asked to read. Ids add a few characters per
invariant and count like any other text. *Code:* `core/src/context/mod.rs`
(`render_as_within`).

## 7. Examples

### 7.1 Valid — ids with a gap, out of order, one wrapped

```markdown
<!-- invar-context: v1 -->
# BUSINESS_CONTEXT.md

## Terms

- Available balance: the balance minus every pending reservation.

## Invariants (always true, no exceptions)

- INV-3: every query on team data is scoped to the current team by the
  `BelongsToTeam` global scope.
- INV-1: a paid order is never deleted; it is cancelled through the refund flow.
- INV-4: an operator action on an order notifies its customer.

## Regional / parameterized facts

- VAT: 19% in DE, 20% in AT.
```

`INV-2` was removed, and its number stays unused.

### 7.2 Valid — no ids, as before 1.0.0

```markdown
<!-- invar-context: v1 -->
## Invariants (always true, no exceptions)

- Every query on team data is scoped to the current team.
- A paid order is never deleted.
```

`plan approve` warns once that coverage cannot be shown.

### 7.3 Invalid — a duplicate, a malformed id, an item without one

```markdown
<!-- invar-context: v1 -->
## Invariants

- INV-1: every query on team data is scoped to the current team.
- INV-1: a paid order is never deleted.
- inv-2: an operator action notifies the customer.
- Stock is never negative.
```

Line 5 repeats `INV-1`; line 6's `inv-2:` is not an id; line 7 has none while the others
do. `plan validate` names all three.

## 8. Decisions and their reasons

**D-1 — Ids are names, not positions.** Criterion ids (`plan-header.md` D-18) are
contiguous because they live and die with one Plan. Invariant ids outlive every Plan that
cites them, so they follow ADR numbering: unique, never reused, gaps allowed.

**D-2 — Some with ids and some without is refused.** The gate lists every invariant as
named or not named. An item without an id could be neither, so the list would show fewer
invariants than the file has and look complete. A file with no ids at all says plainly that
coverage is not shown; a half-numbered one would hide it.

**D-3 — No ids means the previous behavior.** Ids are the team's to add, when they choose
(§1); a project that has not added them must not start failing.

**D-4 — The core matches ids, never prose.** Whether a Plan covers an invariant written in
other words is a judgement, and a check on shared words would teach a planner to repeat
words without covering anything. With ids, "the Plan names `INV-2`" and "`AC3` cites it" are
facts, and the judgement left — does `AC3` really catch a broken `INV-2`? — stays the
developer's at the gate.

## 9. Open questions

- **Ids for regional / parameterized facts**, for the mandatory clarifying question (§5).
  Later, as a separate step (#158).

## 10. Version history

**1.0.0 — first version (#158).** §2 to §4 and §6 write down what the core already did:
the marker, the empty-file rule, the scaffold (with the new comment under
`## Invariants`), and the binding limit. §5 is new: invariant ids, their problems, and what
a file without them means.

**Compatibility:** a file without ids behaves as before (§5.3). The scaffold gains one
comment, which is not content (§3), so a freshly scaffolded file is still empty.
