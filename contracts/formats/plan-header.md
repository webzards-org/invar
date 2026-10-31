# Contract: Plan header and required sections

**Version:** 1.7.0 — the `Acceptance` field, and what the approval digest covers; see §10 for what changed and why
**Status:** stable
**Implements:** §4a (Plan document structure), §4d (Checksums), §5c (Context
delivery), §12 (Plan lifecycle); light mode L-1, L-2, L-5
(`docs/TASKS-M1.md`); A-3, A-5, A-6 (`docs/TASKS-AT.md`)
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
and the ticket. What this contract fixes is the **header**, the **minimum
required sections**, and the **ids of the acceptance criteria**, since those are
what the core and the coder depend on.

Which sections are required depends on `scenarios` in `invar.yaml`
(`invar-yaml.md` §3.1). **Light mode** — `scenarios: false`, and the default —
is what M1 implements: acceptance criteria live in the Plan, as numbered prose,
and no `.feature` files exist (L-1). The rules for `scenarios: true` stay in this
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

The body's first line may not read `Acceptance: required` or `Acceptance: skipped`
once canonicalized (§4d), which ignores a BOM and trailing whitespace (D-19).

## 5. Header fields

| Field | Required | Allowed values | Written by |
|---|---|---|---|
| `Ticket` | yes | `^[A-Z][A-Z0-9]*-[0-9]+$`; must match the directory name | Agent |
| `Module` | yes | `^[A-Za-z][A-Za-z0-9_]*$`, or `global`; case is kept as written (D-15) | Agent |
| `Status` | yes | `draft` or `approved`, lowercase, exactly | `plan approve` sets `approved`; `decision record --type failed` sets `draft`; the agent writes `draft` on creation |
| `Approved-by` | key present always; value only when `Status: approved` | Non-empty; a git `user.name` | **Command only** (`plan approve`) |
| `Approved-at` | key present always; value only when `Status: approved` | `YYYY-MM-DD` | **Command only** (`plan approve`) |
| `Approved-checksum` | key present always; value only when `Status: approved` | `sha256:` + 64 lowercase hex, over the **body**, and over the `Acceptance` value when there is one (§5.3, D-19) | **Command only** (`plan approve`) |
| `Lockfile-checksum` | **optional** since 1.1.0 — absent and empty both mean no observation (§5.2) | `sha256:` + 64 lowercase hex, or empty | **Command only** (`plan approve`, from the milestone that introduces package context; never in light mode) |
| `Acceptance` | **optional** since 1.7.0 — absent and empty both mean not decided yet | `required` or `skipped`, lowercase, exactly, or empty | **Command only** (`plan approve`, which adds the key if it is absent; a `failed` entry clears the value and keeps the key) |

**The three approval keys — `Approved-by`, `Approved-at` and
`Approved-checksum` — must be present as keys from the moment the agent creates
the file**, with empty values (D-3). A missing key and an empty value would
otherwise be indistinguishable, and `plan validate` could not tell a Plan that
follows the convention from one that forgot half of it. `Lockfile-checksum` is
exempt: it may be left out entirely (D-7). So is `Acceptance`: the agent does
not write it, and a Plan created before 1.7.0 does not have it.

**"Command only" is a protocol rule, not an enforcement.** A misbehaving agent
can write `Status: approved` itself. What makes the ordinary failure mode
detectable is `Approved-checksum`: if the body or the `Acceptance` value changes
after approval, the approval is stale, and `plan check` refuses — and with it `invar done`
(M1-10, M1-12). With `scenarios: true`, `scenario confirm` refuses as well
(§4a, honesty note).

### 5.1 Consistency rules

- `Status: draft` — the three approval values are expected to be empty, and so
  are `Lockfile-checksum` and `Acceptance` when their keys are present. Nothing
  treats a draft that
  holds some as an error: `plan check` refuses every draft for being one, and
  `plan approve` overwrites whatever the approval keys hold (§5.3). While
  `Status` says `draft` the values mean nothing.
- `Status: approved` — `Approved-by`, `Approved-at` and `Approved-checksum`
  must be non-empty. `Lockfile-checksum` may be absent, empty, or a digest.
  `Acceptance` is `required` or `skipped`, or has no value. No value means the
  Plan was approved before 1.7.0, and it is treated as `skipped`, because no
  test-writer existed then.
- `Ticket` must equal the `<TICKET>` part of the directory name.
- *When `scenarios: true`:* every `.feature` file confirmed against this Plan
  carries the same `Ticket`.

**Form and approval are checked by different commands** (D-11).
`plan validate` checks **form**: the path and directory name (§2), encoding
and structure (§3, §4), the presence of every required key and the format of
every non-empty value (§5), `Ticket` against the directory name (above), and
the sections (§6). It does not look at whether the Plan's approval holds, and
reads `Status` for one thing only: whether criteria with no ids may stand
(D-16). The second rule above — an `approved` Plan has its three values — is
checked by `plan check` (M1-10), which `invar done` runs, and by
`plan approve` (M1-9), which refuses such a Plan without `--reapprove`
(D-12). A draft is valid whatever its approval values hold, and so is the
Plan of example 9.3 as far as `plan validate` is concerned.

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
approved specification. `Acceptance` sits in the header too, and is covered,
because it is a decision and not an observation (D-19).

Drift is a **warning, never a failure** (§4a). A dependency upgraded mid-cycle
or a rebase onto a branch that upgraded one makes the Implementation context
describe a version that is no longer installed; the tests judge whether the code
works against what is actually installed, and a blocking check here would fire
on every routine rebase.

### 5.3 What `plan approve` writes

`invar plan approve <TICKET | PATH>` is the human gate (§4a).

**The value of `Acceptance` is decided first**, before anything is printed. It is
`--acceptance` (`required`) or `--no-acceptance` (`skipped`) if one is given; they
exclude each other. Otherwise it is `acceptance.required` from `invar.yaml`, once
that key exists (AT-4), and otherwise `skipped`. `--reapprove` takes the value the
same way, and does not inherit the old one.

Then, in order:

1. **Validate.** A Plan that `plan validate` rejects is refused, with the same
   list of problems. A new approval must have numbered criteria (§6.4), so
   `--reapprove` also refuses, listing each one, criteria that `plan validate`
   accepts only because an earlier approval stands (D-16).
2. **Compare with any earlier approval** (D-12):
   - `Status: draft` — approve. Any values already in the approval keys are
     overwritten.
   - `Status: approved`, all three approval values present, and
     `Approved-checksum` equal to the approval digest (step 5) of the Plan as it
     is — already approved; nothing is written, and the command succeeds. A flag
     that names another value than the one recorded changes nothing, and the
     command says so and names `--reapprove`. With `--reapprove`, and only with
     such a flag, approve as for a draft, replacing the earlier values (D-20).
   - `Status: approved` and the approval digest differs from `Approved-checksum`,
     or any of the three approval values is empty — **refused**, unless
     `--reapprove` is given; with it, approve as for a draft, replacing the
     earlier values.
3. **Read the approver and the date from git**, before anything is shown:
   - `Approved-by` is `git config user.name`, run in the project root, with
     surrounding whitespace trimmed. Unset or empty is an error, never an
     empty field.
   - `Approved-at` is today's date, `YYYY-MM-DD`, in the approver's local time
     zone, as git dates a commit made now (D-13).
   - Both must be writable into a header line as they are — a name with a
     newline or a ` #` in it is not — and a pair that is not is refused here,
     before anything is shown.
4. **Print `## Acceptance criteria`** — what the code will be judged against —
   and one line with the value of `Acceptance` and where it came from:
   `Acceptance: required (from --acceptance)`,
   `Acceptance: skipped (from --no-acceptance)`,
   `Acceptance: required (from invar.yaml)` or `Acceptance: skipped (default)`.
5. **Write.** `Status` becomes `approved`; `Approved-by`, `Approved-at`,
   `Approved-checksum` and `Acceptance` get their values. If the header has no
   `Acceptance` key, it is added right after `Approved-checksum`, with that line's
   line ending. Nothing else changes — not the other header lines, not their
   comments, not the line endings, not one byte of the body. `Lockfile-checksum`
   is left as it is (§5.2). The file is replaced atomically, and refused if it
   changed after the criteria were printed.

   `Approved-checksum` is the **approval digest**. When the header has an
   `Acceptance` value, that is the §4d digest of the line `Acceptance: <value>`, a
   newline (`\n`, whatever the file uses), then the canonical body (§4.3). When it
   has none, it is the §4d digest of the body alone, as before 1.7.0 (D-19). A new
   approval always has a value, so only a Plan approved before 1.7.0 has the
   second form.

`plan check` reads the field back: on success it prints, after its `ok:` line, one
fixed line, `acceptance: required` or `acceptance: skipped`. A Plan with no value
prints `skipped`.

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

One section is **optional** in light mode: `## Acceptance tests to change`, right
after `## Acceptance criteria` (§6.5). Absent means none.

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
(OQ-2) and is decided by the milestone that introduces `.feature` files. In this
mode `## Acceptance criteria` and `## Acceptance tests to change` are ordinary
additional sections: the rules of §6.4 and §6.5 do not apply (D-17).

**Rules for both modes:**

- Order is fixed so a reviewer reads every Plan the same way and so a diff
  between two Plans is meaningful (D-4).
- Heading text is matched **exactly**, case-sensitively, after trimming
  trailing whitespace. `## Acceptance Criteria` is an error.
- Additional sections beyond the required ones are permitted, anywhere, and
  ignored by the core. Only the relative order of the required ones is checked.
- A required section may be empty of prose only where §6.1 says so. What
  counts as **content** is defined once, for every required section (D-14): a
  line that still has at least one letter or digit after HTML comments
  (`<!-- … -->`, also across lines), a leading list marker (`-`, `*`, `+`,
  `1.`, `1)`) and a leading checkbox (`[ ]`, `[x]`) are removed. A comment, an
  empty list item and a lone `...` are not content.
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
| `## Acceptance criteria` | What the change will be judged against, as numbered items (§6.4) — light mode | **No** — and no placeholder counts as content (D-8, D-14) |
| `## Acceptance tests to change` | Another ticket's locked tests this ticket changes or deletes (§6.5) — light mode, **optional** | Absent means none; if present, not blank — `_none_` when there are none |
| `## Scenarios to write` | A machine-readable list (§6.2) — `scenarios: true` only | No |
| `## Scenarios to delete` | A machine-readable list (§6.2) — `scenarios: true` only | Yes — written `_none_`, never left blank (D-5) |
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

**Non-empty, in substance.** The section must contain at least one criterion:
a line of real text (§6, D-14) that is not a placeholder. The placeholders are
`_none_`, `none`, `n/a`, `tbd` and `todo`, in any capitalisation and with any
emphasis or trailing punctuation (`**TBD**`, `N/A.`), when they are the whole
line. A Plan with nothing to accept cannot be reviewed against, cannot be
approved meaningfully — there is nothing for the person at the gate to agree
to — and cannot be turned into tests later. Unlike `## Scenarios to delete`,
"nothing" is never a valid answer here, so `_none_` is not an escape (D-8), and
neither is any other way of writing it (D-14).

**One criterion per list item, numbered, in Given/When/Then prose.** *Numbered
since 1.6.0.*

```markdown
- AC1: Given <the starting state>, when <the action>, then <the observable outcome>.
- AC2: Given <the starting state>,
  when <the action>, then <the observable outcome>.
```

The number is the criterion's **id**, and these rules are checked (D-18):

- An id is `AC` followed by a positive integer with no leading zero, then `:`
  and a space. It is the first token after the list marker, and after a checkbox
  if the item has one. `AC01`, `ac1`, `AC 1:` and `AC1` without the colon are
  not ids. An id is written plain, and emphasis around it (`**AC1:**`) is an
  error; emphasis in the criterion's text is fine.
- Ids are unique, contiguous from `AC1`, and appear in order. `AC1`, `AC3` is a
  gap; `AC2`, `AC1` is out of order.
- A list item may wrap. A line indented by two spaces or a tab, under an item,
  belongs to that item, which keeps its one id. An indented line that has a list
  marker and an id is a criterion of its own.
- What counts as a criterion is what counted before: comments, bare list markers
  and placeholders are not criteria. A placeholder after an id (`AC1: TBD`) is
  still a placeholder.
- Any other line of real text in the section is an error: every criterion is a
  numbered list item.

**A Plan approved before 1.6.0 has no ids**, and its body is frozen, so an
approved Plan whose criteria carry **no ids at all** is valid as it was approved
(D-16). It must not be edited to add them. An approved Plan with some ids and a
broken rule is invalid, as a draft is. A new approval always needs ids.

Each criterion should describe one behavior, observable from outside the code —
a response, a stored state, what a user sees — so that it can be checked without
reading the implementation.

**The core parses the section as far as the ids, and no further.** It never
interprets the Given/When/Then prose (D-18). `plan validate` checks that the
heading is present, in order, that the section is non-empty, and the rules above
(M1-8). The form of the prose is fixed anyway, for the readers that depend on it:

- **The developer at the gate.** `plan approve` prints the criteria before
  approving (M1-9). They are the part of the Plan the code will be held to, so
  they are the part a person must actually read.
- **`/invar:review`** reports each criterion by its id as `met`, `not met` or
  `unclear`, with evidence (M1-20). One criterion per item is what lets it answer
  per criterion instead of for the section as a whole.
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

### 6.5 `## Acceptance tests to change`

New in 1.6.0. **Optional, and light mode only** (D-17). It is how a later ticket
is allowed to change or remove another ticket's locked acceptance tests: the
change is declared in the Plan, so it passes the gate and a person sees it (§4c,
A-3). Absent means none.

It comes right after `## Acceptance criteria`, and in any case before
`## Implementation context`. The list form is that of §6.2:

```
- <path> — change — <reason>
- <path> — delete — <reason>
```

- `<path>` is relative to the project root, the first token after `- `. It is
  not checked here that the file exists or is locked.
- The action is exactly `change` or `delete`.
- The reason is required: a placeholder is not a reason.
- The separators are a space, an em dash and a space. Each item is one
  unindented line.
- If the section is present it must not be empty. `_none_` is allowed as its
  single line, as in §6.2.

As with `## Acceptance criteria`, `Approved-checksum` covers it.

The PR description and `invar review-input` show it right after the criteria,
when it lists something: the point of the section is that a person sees the
change. Absent or `_none_`, they show nothing, not even the heading.

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

**D-12 — A changed approval is refused without `--reapprove`** (1.3.0,
resolves OQ-1). `plan approve` on a Plan that is already approved but whose
body no longer matches `Approved-checksum` could re-approve it and issue a new
checksum. It does not: a body edited after approval is exactly the failure
the checksum exists to catch, and a command that renews the approval as a
matter of course turns that check into one more step of the routine. Asking
for `--reapprove` makes renewing a deliberate act of someone who knows the
text changed — and the command prints the criteria again before writing.
The same applies to `Status: approved` with empty approval values (example
9.3): no command recorded that approval, so there is nothing to keep, but
writing a real one over it is still a decision about a Plan someone already
calls approved. An approval that is still current is left alone, with or
without the flag: re-running the command is harmless, and the original
approver and date stay on record.

**D-13 — `Approved-at` is the approver's local date, as git sees it**
(1.3.0). D-6 makes it a date a person can check against their own memory of
the day, and that is the local day: an approval at 00:30 in Belgrade belongs
to that day, not to the previous one in UTC. The core reads it from git
(`git var GIT_COMMITTER_IDENT`, with the identity overridden so a missing
`user.email` does not matter) rather than computing it: the standard library
has no local time zone, git already resolves it on every platform, and the
date then matches the commit that carries the approval — `GIT_COMMITTER_DATE`
included.

**D-14 — Content means text, not just a non-blank line** (1.4.0). The gate
rests on `## Acceptance criteria`, and a check that only asks for a non-blank
line is satisfied by `-`, `TBD` or `<!-- fill in -->` — the placeholders an
agent or a template leaves behind. §6 therefore defines content once, for every
required section: a line with a letter or digit left once HTML comments, a
leading list marker and a leading checkbox are removed. The acceptance criteria
add a short list of placeholder lines (§6.4) that do not count either, because
in that section "nothing" is never an answer (D-8). The list is deliberately
small and is not extended with every spelling of "nothing yet": a planner can
learn to satisfy any mechanical check, so this one only removes what is
unambiguously not a criterion. Whether a criterion is a *good* one remains what
the person at the gate is for.

**D-15 — `Module` may be capitalized** (1.5.0). Laravel projects name their
modules `Billing`, `Invoicing`; a rule that only accepted `billing` would make
the planner write a name that exists nowhere else in the project. The value is
a label: nothing in the core compares modules, so case is kept as written and
no normalization is applied. `-` stays out of the name because the decision
entry's filename (`decision-entry.md` §2) ends the module at the first `-`.

**D-16 — An approved Plan with no ids stands; a new approval needs them**
(1.6.0). `plan check` and `done` run `plan validate`, so requiring ids of every
Plan would refuse each Plan approved under 1.5.0 before its cycle closed — and
its body cannot gain ids, because `Approved-checksum` freezes it. So the rule
follows the state: criteria with **no ids at all** are valid when `Status` says
`approved`. That is not a way around the numbering. A draft that claims
`Status: approved` passes `plan validate` and nothing else: `plan check` refuses
it because no approval of that text was recorded, and `plan approve --reapprove`
refuses it for its missing ids, since the exception covers the approval that accepted those criteria and no
later one. A Plan that has some ids is numbered, and every rule applies, whatever
its `Status`. `decision record --type failed` still returns such a Plan to draft,
with its body untouched (§4c); it is then an ordinary draft, and needs its ids
before it can be approved again.

**D-17 — The new rules apply in light mode only** (1.6.0). With
`scenarios: true`, `## Acceptance criteria` is not required (OQ-2), and locked
acceptance tests are written from numbered criteria, so neither those criteria
nor a list of changes to their tests has a meaning yet. Both are ordinary
additional sections there, as the scenario sections are in light mode (D-9). The
milestone that introduces `.feature` files decides again, with OQ-2.

**D-18 — The core reads the id and nothing after it** (1.6.0). A later command
joins criteria to tests, to the review and to the PR by id (A-5), and that is a
comparison of tokens. Matching prose to prose would be a judgement, which the
core never makes, so the prose stays unparsed (D-14). Emphasis around an id is
refused by name rather than read through, because a token is compared as it is
written. Wrapping is decided by indentation alone, and an indented list item with an id still counts as a
criterion: otherwise a wrongly indented `AC2` would disappear into `AC1`, and no
later check would know that it had existed.

**D-19 — The approval digest covers `Acceptance`** (1.7.0, closes OQ-3 of
`docs/TASKS-AT.md`). Whether a ticket gets acceptance tests is a person's decision
(A-6), and skipping them is the weaker guarantee, so it must not change silently.
The field sits in the header because every reader already looks there, and because
the command writes it, as it writes the approval values; the body is the agent's
text, and no place for a value only `plan approve` may set. It is still covered,
unlike `Lockfile-checksum`, which is a deliberate observation outside the approval
(§5.2): that one records what was installed, and this one records what was decided.
So the digest takes the line `Acceptance: <value>` in front of the canonical body,
and changing the value, removing the line or clearing it each stale the approval. A
Plan with no value keeps the digest of its body alone, which is what keeps an
approval from before 1.7.0 standing, and also why adding the line to such a Plan
stales it. An empty value counts as no value, so a Plan approved before 1.7.0 that
gains an empty key still stands, and still reads as `skipped`. Line and body are
hashed as one text, so moving the line out of the header to the top of the body
would give the same digest with no value in the header. §4.3 refuses a body that
starts that way; it is the one body the digest cannot tell apart.

**D-20 — A current approval changes its `Acceptance` only when a flag asks for it**
(1.7.0). D-12 leaves a current approval alone, with or without `--reapprove`, and
that still holds for the file's text. It does not hold for a person who decides to
add or drop the acceptance tests of a Plan that has not changed: they need a way to
record that, and `--reapprove` is the way to replace an approval. So `--reapprove`
with `--acceptance` or `--no-acceptance`, naming a value other than the recorded one,
replaces the approval. `--reapprove` alone still does nothing to a current approval:
the default or a changed `invar.yaml` must not undo a decision that was recorded,
when nobody named a value and the text did not change. Without `--reapprove`, a flag
never writes, because re-running the command is harmless (D-12).

## 8. Open questions

**OQ-1 — Re-approving an already approved Plan.** *Resolved in 1.3.0 by
D-12:* refused unless `--reapprove` is given.

**OQ-2 — `## Acceptance criteria` when `scenarios: true`.** Light mode
requires it; the 1.0.0 shape for `scenarios: true` does not have it. Once
`.feature` files exist, the criteria could stay as the human-readable source
the scenarios are written from (L-1), or the scenarios could replace them.
Decide in the milestone that introduces `.feature` files; until then the
`scenarios: true` section list is the 1.0.0 one, unchanged.

## 9. Examples

Examples 9.1, 9.2, 9.3 and 9.5 to 9.12 are light mode. Example 9.4 applies when
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
- AC1: Given a pending, unpaid order of mine, when I cancel it, then its status
  is `cancelled` and its reserved stock is available for sale again.
- AC2: Given a paid order of mine, when I try to cancel it, then the request is
  refused with 403 and the order is unchanged.
- AC3: Given an order that belongs to another team, when I send a cancel request
  with its id, then the response is 404 and the order is unchanged.
- AC4: Given an order I cancelled, when I open my order history, then it is
  listed with status `cancelled`.

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

`AC3` is the criterion the invariant asks for (§6.4): it fails if the cancel
route loads the order without the team scope — exactly the incident the
invariant exists to prevent. The other criteria come from the ticket and the
answers in `## Open questions / answers`.

### 9.2 Valid — the same Plan after `plan approve`

Only the header changes; the body is untouched, which is what makes
`Approved-checksum` verifiable later. Light mode writes no
`Lockfile-checksum`. The checksum is the real §4d digest of example 9.1's
body; the core's tests approve 9.1 as `rade` on 2026-10-05 and compare the
result with this header.

```markdown
---
Ticket: SHOP-214
Module: orders
Status: approved
Approved-by: rade
Approved-at: 2026-10-05
Approved-checksum: sha256:b9caa4c64bc83723bb4e930d7fedeae733950c5210ee0e30f4ece84eb2c2cf7b
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
checks form only and accepts it (§5.1, D-11); `plan approve` refuses it
without `--reapprove` (D-12).

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

### 9.6 Valid — numbered criteria and `## Acceptance tests to change`

```markdown
---
Ticket: SHOP-301
Module: orders
Status: draft
Approved-by:
Approved-at:
Approved-checksum:
---

## Ticket
SHOP-301 — A paid order can be cancelled too, for 24 hours, and it is refunded.

## Intent
Let customers cancel a paid order within 24 hours of paying for it.

## Scope
`App\Actions\Orders\CancelOrder` and `OrderPolicy::cancel`. Out of scope:
partial refunds.

## Open questions / answers
None were needed.

## Rejected alternatives
None considered.

## Acceptance criteria
- AC1: Given an order of mine paid 23 hours ago, when I cancel it, then its status
  is `cancelled` and a full refund is recorded.
- AC2: Given an order of mine paid 25 hours ago, when I try to cancel it, then the
  request is refused with 403 and the order is unchanged.

## Acceptance tests to change
- tests/Acceptance/Shop214CancelTest.php — change — its 403 no longer holds within 24 hours
- tests/Acceptance/Shop190PaidTest.php — delete — it pins "paid is never cancelled"

## Implementation context
- AI_CONTEXT: stock and refund changes stay inside the action's transaction
```

### 9.7 Invalid — a gap in the ids

The header and every other section as in 9.1; only this part differs:

```markdown
## Acceptance criteria
- AC1: Given a pending, unpaid order of mine, when I cancel it, then its status
  is `cancelled`.
- AC3: Given a paid order of mine, when I try to cancel it, then the request is
  refused with 403.
```

**Why:** ids are contiguous from `AC1` (§6.4). `AC2` either was forgotten or was
removed without renumbering, and every later reader joins on the id. `plan
validate` reports it on the line of `AC3`, saying that `AC2` is missing.

### 9.8 Invalid — an unknown action

The header and every other section as in 9.1, with this section added after
`## Acceptance criteria`:

```markdown
## Acceptance tests to change
- tests/Acceptance/Shop214CancelOrderTest.php — rewrite — the 403 no longer holds
```

**Why:** the action is exactly `change` or `delete` (§6.5). A later check treats
the two differently, and it can only do that for words it knows. `plan validate`
names the line and the word.

### 9.9 Valid — approved under 1.5.0, with no ids

`contracts/fixtures/plan/approved-1.5.0-no-ids.md` is example 9.1 and its approval
as 1.5.0 wrote them: `Status: approved`, the real digest, and these criteria:

```markdown
## Acceptance criteria
- Given a pending, unpaid order of mine, when I cancel it, then its status is
  `cancelled` and its reserved stock is available for sale again.
```

**Why valid:** it passes `plan validate` and `plan check`, and `done` closes it
(D-16). Adding ids would change the body and stale the approval. Under
`Status: draft` the same text is invalid: every item lacks its id.

### 9.10 Valid — example 9.1 approved with `--acceptance`

The checksum is the §4d digest of `Acceptance: required`, a newline, and example
9.1's body (§5.3).

```markdown
---
Ticket: SHOP-214
Module: orders
Status: approved
Approved-by: rade
Approved-at: 2026-10-05
Approved-checksum: sha256:d06d08d4c20d81e4d30b2240ed3307cb7973c07f7b4fb8e3fca30dfcc7e692e2
Acceptance: required
---
```

Changing the value to `skipped`, or removing the line, leaves the file valid in
form, and `plan check` and `done` refuse it: the Plan changed after it was approved
(D-19).

### 9.11 Invalid — an unknown `Acceptance` value

```markdown
---
Ticket: SHOP-214
Module: orders
Status: draft
Approved-by:
Approved-at:
Approved-checksum:
Acceptance: optional
---
```

**Why:** the value is `required`, `skipped` or empty (§5). A later reader acts on
the two words it knows. `plan validate` names the line and the word, whatever
`Status` says.

### 9.12 Valid — example 9.1 approved under 1.6.0, with no `Acceptance`

The checksum is the digest of example 9.1's body alone.

```markdown
---
Ticket: SHOP-214
Module: orders
Status: approved
Approved-by: rade
Approved-at: 2026-10-05
Approved-checksum: sha256:b9caa4c64bc83723bb4e930d7fedeae733950c5210ee0e30f4ece84eb2c2cf7b
---
```

**Why valid:** the approval keeps its digest and stands, and `plan check` prints
`acceptance: skipped` (§5.1, D-19). Adding `Acceptance: required` by hand stales it.

## 10. Version history

**1.7.0 — the `Acceptance` field (AT-3).**

- §5: new optional key `Acceptance: required | skipped`, written only by
  `plan approve`. A `failed` entry clears its value and keeps its key.
- §5.1: what the value means for a draft and for an approved Plan; no value on an
  approved Plan is `skipped`.
- §5.3: where the value comes from, what is printed, what is written, the approval
  digest, and the line `plan check` prints. New D-19 and D-20.
- §4.3: the body's first line may not be an `Acceptance` line (D-19).
- Example 9.2 shows the field and its digest; examples 9.10 to 9.12 are new.

**Why:** skipping acceptance tests is a weaker guarantee, so it has to be a person's
decision, recorded where every later reader looks and impossible to flip silently
(A-6). OQ-3 asked how the digest should cover it, and D-19 answers.

**Compatibility:** every draft that was valid stays valid: the key is optional, and
the planner does not write it. A Plan approved before 1.7.0 keeps its digest and
stays approved, and an absent field reads as `skipped`. Two things that were
accepted are now refused: a value other than `required` or `skipped` under an
`Acceptance` key, which was an unknown key before, and a body whose first line is
an `Acceptance` line. The success line of `plan approve` is unchanged, and
the checksum it shows is the new digest. `plan check` prints one more line.

**1.6.0 — numbered criteria and `## Acceptance tests to change` (AT-2).**

- §6.4: every acceptance criterion is a list item that starts with its id, `AC<n>:`;
  ids are unique, contiguous from `AC1` and in order, and any other line of real
  text in the section is an error. New D-18.
- New §6.5: `## Acceptance tests to change`, optional, in light mode, shown by
  the PR description and `invar review-input` when it lists something. New D-17.
- §5.1, §5.3 and new D-16: criteria with no ids at all stay valid under an
  existing approval; a new approval, `--reapprove` included, needs ids.
- Example 9.1 is numbered, so example 9.2 carries its new digest; examples 9.6 to
  9.9 are new.

**Why:** a criterion id is the join key for everything after the gate: the test
that covers it, the lock, the review, the PR (A-5). Without one, a coverage check
would have to match prose to prose, which is the judgement the core never makes.
The new section keeps locked tests changeable, but only through a Plan a person
approved (§4c).

**Compatibility:** Plans are consumable and do not outlive their cycle (§12), so a
required id format is safe for new Plans. A Plan already **approved** under 1.5.0
keeps working until its cycle closes: its criteria have no ids, and it passes
`plan validate`, `plan check` and `done` unchanged (D-16). A Plan that is still a
**draft** under 1.5.0 is invalid until its criteria have ids. So is an approved
Plan returned to draft by a `failed` entry, or one being re-approved after an
edit. An approved Plan with some ids and a broken rule is invalid as well; under
1.5.0 such ids were only prose. A section named `## Acceptance tests to change`
was an ignored additional section under 1.5.0 and is now read.

**1.5.0 — capitalized module names.**

- §5: `Module` matches `^[A-Za-z][A-Za-z0-9_]*$` (or `global`) instead of
  `^[a-z][a-z0-9_]*$`. New D-15.

**Why:** the project's own modules are capitalized (`Billing`), and the lowercase
rule made `plan validate` refuse a Plan that named the module correctly.

**Compatibility:** every Plan that was valid stays valid; one that named a
capitalized module was invalid and now passes.

**1.4.1 — a draft's approval values, and when they are checked (M1-9).**

- §5.1 no longer says that `plan check` and `plan approve` check a draft's
  approval values. They never did, and nothing needs them to: `plan check`
  refuses a draft for being one, and `plan approve` overwrites the values
  (§5.3). §5.1 now says so.
- §5.3 step 3: values that cannot be written into a header line are refused
  before the criteria are printed, not after.

**Why:** the text promised a check that no command makes, so a reader
implementing it from the contract would add a refusal the reference
implementation does not have; and the order of §5.3 promised that nothing is
shown for an approval that cannot be recorded, which the first implementation
only kept for what git reports.

**Compatibility:** no Plan that was valid becomes invalid, and none that was
refused is accepted. The contract now describes what the core does.

**1.4.0 — what counts as content (M1-6).**

- §6 defines *content* for every required section (D-14): HTML comments, bare
  list markers, empty checkboxes and lines with no letter or digit do not count.
- `## Acceptance criteria` additionally rejects placeholder lines — `_none_`,
  `none`, `n/a`, `tbd`, `todo` (§6.4, D-14).
- §6.1: `## Scenarios to delete` is empty only as `_none_`, as D-5 and example
  9.4 already said; the table said otherwise.

**Why:** the gate rests on the acceptance criteria, and "at least one non-blank
line" accepted `-`, `TBD` and `<!-- fill in -->`: exactly what a template or a
hurried agent leaves behind, and exactly what the person at the gate is least
likely to notice.

**Compatibility:** a Plan that was valid under 1.3.0 is invalid under 1.4.0 only
if a required section holds nothing but comments, bare list markers or
punctuation, or if its acceptance criteria are only placeholders. Plans are
consumable and do not outlive their cycle (§12), so at most a Plan in flight
needs one line rewritten.

**1.3.0 — `plan approve` (M1-9).**

- New §5.3: what `plan approve` writes, where each value comes from, and in
  what order it checks, prints and writes.
- OQ-1 resolved by D-12: an approved Plan whose body changed — or whose
  approval values are empty — is refused unless `--reapprove` is given; one
  that is still current is left alone.
- D-13: `Approved-at` is the approver's local date, read from git.
- Example 9.2 carries the real digest of example 9.1's body instead of a
  placeholder, and is now checked by a test.

**Why:** the approval is the one point that records a person accepted the
Plan; every later refusal rests on it. Writing down exactly what it records,
and that it is never renewed without being asked, is what lets `plan check`
and `done` rely on it.

**Compatibility:** no Plan that was valid becomes invalid. `--reapprove` is
new; a Plan approved before 1.3.0 is read the same way.

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
