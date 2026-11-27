# Contract: the Plan audit

**Version:** 1.0.0 — initial version; see §10
**Status:** stable
**Implements:** #167 (an agent corrects a draft Plan's facts against the code, and reports
what it cannot correct); builds on `plan-header.md` (sections, criterion and invariant ids)
and `business-context.md` (invariant ids)
**Read by:** the core (`invar plan audit begin | input | report`; `invar plan approve`,
which computes the audit line; `invar plan guard`, which keeps an agent's `Write` and `Edit`
off the files only commands write); the plan-auditor agent (as prose); any later
reimplementation

> Changing this file is a **public-contract change** (§14.3). It is made deliberately, in
> its own PR, with the reason recorded, the version bumped, and every reader updated in the
> same PR. An absent key or file means the previous behavior.

---

## 1. Scope

`plan validate` checks a Plan's form, and the checksum freezes it. Neither says whether its
facts are right: a column that does not exist, a path with a typo, a criterion that cannot
pass. An audit is an agent's reading of a **draft** Plan against the code, before the
developer approves it. The agent corrects facts that have one right answer in the code, and
reports everything else as findings. The core computes the rest: what changed, whether the
changes stayed where they may, and one line from the findings.

**Validity is mechanical; quality is a judgement** (D-1). The audit helps with the second.
It is advisory: no hook runs it, and `plan approve` and `done` neither wait for it nor
refuse because of it.

This contract defines the files (§2), the findings the agent writes (§3, §4), the commands
(§5 to §7), and the line `plan approve` prints (§8).

## 2. Files

Under `.invar/out/<TICKET>-audit/`, which `invar init` already ignores (`.invar/out/`):

| File | Written by | Holds |
|---|---|---|
| `baseline.md` | `plan audit begin` | the Plan's `spec.md` exactly as it was when the audit began, header included (D-3) |
| `findings.md` | the auditor | its edits and findings (§3) |
| `report.txt` | `plan audit report` | what the report printed, byte for byte (§7); the developer's copy, which nothing reads back (§8, D-4) |

`<TICKET>` is the Plan header's `Ticket`.

`findings.md` is the only one of the three the auditor writes. `plan guard` denies an
agent's `Write` and `Edit` of `baseline.md` and `report.txt`, and allows `findings.md`
(§7.2, D-3).

## 3. `findings.md`

UTF-8, one entry per line, LF or CRLF. Read line by line; it is not Markdown.

```
line     = blank | comment | edit | finding
blank    = only whitespace
comment  = "#" anything
edit     = "edit: " section " | " evidence " | " reason
finding  = "finding: " kind " | " severity " | " section " | " evidence " | " text
```

- Fields are separated by ` | ` (space, bar, space) and trimmed. The last field, `reason`
  or `text`, is the rest of the line, and may itself hold ` | `.
- **`section`** is the heading of a Plan section, exactly as after `## `: `Ticket`,
  `Intent`, `Scope`, `Open questions / answers`, `Rejected alternatives`,
  `Acceptance criteria`, `Implementation context`. For an `edit`, only `Implementation
  context` or `Scope` (§5.2).
- **`evidence`** is `<path>:<line>` or `<path>:<line>-<line>`, a file of the repository
  relative to the project root and a 1-based line, or the word `ticket`, for a sentence of
  the Plan's `## Ticket`. A path holds no whitespace.
- **`kind`** is lowercase letters and digits in words joined by single `-`:
  `^[a-z][a-z0-9]*(-[a-z0-9]+)*$`.
- **`severity`** is `blocking` or `note`. `blocking` is allowed only for a kind of §4.
- **`reason`** and **`text`** are not empty.

Each **edit** gives the reason for one change the auditor made, and the line it read the
right answer from. The edits of one section are listed in the order of the changes in that
section, top to bottom: the report pairs them in that order (§7.2).

Every line that is none of the forms above is an error naming its line: an unknown entry,
a wrong number of fields, an unknown section, an `edit` of a section other than the two,
malformed evidence, a malformed kind, an unknown severity, `blocking` on a kind outside §4,
an empty reason or text. Every error is reported in one run. A file with no entry is valid.

### 3.1 Example

```
# SHOP-214, audited against the code at 3f1c2a9
edit: Implementation context | app/Models/Order.php:41 | the column is `cancelled_at`, not `canceled_at`
edit: Scope | app/Http/Controllers/OrderController.php:1 | the controller is OrderController.php, not OrdersController.php
finding: criterion-cannot-pass | blocking | Acceptance criteria | app/Models/Order.php:12 | AC3 asks for a refund e-mail, and orders have no customer e-mail to send it to
finding: invariant-not-named | blocking | Implementation context | BUSINESS_CONTEXT.md:9 | INV-2: cancelling releases reserved stock, and the Plan does not name it
finding: wording | note | Intent | ticket | "own orders" could also mean the team's orders
```

## 4. Blocking kinds

A closed list. Any other kind is a note, and `blocking` on it is an error (§3).

| Kind | Meaning |
|---|---|
| `criterion-cannot-pass` | a criterion cannot pass as written, against the code |
| `criteria-contradict` | two criteria cannot both pass |
| `ticket-sentence-uncovered` | a sentence of the ticket that no criterion covers |
| `criterion-without-ground` | a criterion with no ground in the ticket |
| `invariant-not-named` | an invariant the ticket plainly touches, not named in Implementation context; its `INV-<n>` in the text when `BUSINESS_CONTEXT.md` has ids |
| `fact-contradicted` | a fact in Implementation context the code contradicts, which the auditor could not correct |

**The verdict** is `NOT READY` when any finding has severity `blocking`, and `READY`
otherwise. Nothing else the agent writes changes it.

## 5. `invar plan audit begin <TICKET | PATH> [--restart] [--skill]`

The developer's (D-3). `plan guard` denies it to an agent's Bash; `/invar:plan-audit` runs
it in its shell block.

1. Refused, naming why, unless the Plan is a **draft** and passes `plan validate`.
2. Refused when `baseline.md` already exists, unless `--restart` is given: otherwise an
   agent that had edited a section it may not could begin again and move the baseline past
   its own edit.
3. Writes `baseline.md`, and removes any `findings.md` and `report.txt` of the ticket.
4. Prints one line, `plan audit begun: <TICKET>`, and the path of the baseline.

With `--skill`, a refusal is printed on stdout as `error: …` and the exit code is 0, as for
`plan approve --skill`.

### 5.1 What the auditor reads: `invar plan audit input <TICKET | PATH>`

Refused without a baseline. Prints, in this order and nothing else: the Plan as it is now;
the binding context, as `invar context show --binding` prints it; the invariant ids of
`BUSINESS_CONTEXT.md` with their first lines, when it has ids; what the auditor may change
(§5.2); the path and grammar of `findings.md` (§3); and the blocking kinds (§4). Nothing
from `.invar/decisions/`, and nothing from the planner's session. The same files print the
same bytes.

### 5.2 What the auditor may change

Only a draft, only `## Implementation context` and `## Scope`, and in them only facts with
one right answer in the code, each cited `file:line`: a class, method, route, column,
config key or mechanism named differently in the code, an `INV-<n>`, a path that does not
exist or is misspelled. Never `## Ticket`, `## Intent`, `## Open questions / answers`,
`## Rejected alternatives`, `## Acceptance criteria`, what `## Scope` covers (adding or
removing anything from it), any other section, or the header. Everything else is a finding.

## 6. Comparing the Plan with its baseline

The Plan now and `baseline.md` are both parsed as Plans. The report is **refused**, naming
each, and restores nothing (`git diff` shows the change, and the developer decides), when:

- the Plan is no longer a draft;
- the header changed, any byte from the opening `---` to the closing one;
- a section was added, removed, renamed or moved, or text outside every section changed;
- a section other than `## Implementation context` and `## Scope` changed.

A section is the text under its `## ` heading, as `plan-header.md` §6 reads it.

## 7. `invar plan audit report <TICKET | PATH>`

Reads `baseline.md`, the Plan now and `findings.md`. Refused without a baseline, without
`findings.md`, with a `findings.md` that is not valid (every error, each with its line), and
in every case of §6. A refusal exits non-zero and writes no `report.txt`.

### 7.1 Output

In this order, each part always present:

```
plan audit <TICKET>: <spec path>

changes:
  ## <section>
    -<baseline line>
    +<line now>
      reason: <reason> (<evidence>)
plan validate: <ok, or every problem>
blocking findings:
  <kind> (<section>, <evidence>): <text>
notes:
  <kind> (<section>, <evidence>): <text>
body: sha256:<hex>
plan audit <TICKET>: READY
this is an agent's assessment of the Plan, not a check; approving is yours.
```

- **changes:** per changed section, in Plan order, each hunk of the line diff from the
  baseline to now (`-` a line removed, `+` a line added, two-space indentation, no context
  lines), and under each hunk `reason:` with its edit's reason and evidence. `  none` when
  nothing changed.
- **The line diff** is the longest common subsequence of the two sections' lines, removals
  before additions within a hunk; the same input gives the same diff.
- **Pairing (§3):** the edits of a section are paired with its hunks in order. A hunk with
  no edit says `edited without a reason`. An edit with no hunk is listed under the section
  as `reason with no change: …`, and one for a section that did not change as well.
- **plan validate:** `plan validate: ok`, or `plan validate: <n> problem(s)` followed by
  each, as `plan validate` prints them. A Plan the auditor left invalid is reported, not
  refused.
- **Findings:** blocking first, then notes, each in file order; `  none` when there are
  none.
- **`body:`** the §4d digest of the canonical body of the Plan now (`plan-header.md` §4.3):
  the same digest an approval of it with no `Acceptance` value would have (§5.3 there).
- **The last two lines,** exactly: `plan audit <TICKET>: READY`, or
  `plan audit <TICKET>: NOT READY — <n> blocking finding(s)`; then
  `this is an agent's assessment of the Plan, not a check; approving is yours.`

`report.txt` holds the same bytes. The exit code is 0 for `READY` and for `NOT READY`: it
is a report, not a gate. Byte-identical for the same baseline, Plan and findings.

### 7.2 What protects the files, and what does not

`plan guard` denies a `Write` or `Edit` of `.invar/out/<TICKET>-audit/baseline.md` and of
`.invar/out/<TICKET>-audit/report.txt`, naming the file and saying that only `findings.md`
is the auditor's; a `Write` or `Edit` of `findings.md` passes. Like every other file the
guard holds, a write through the shell (`cp`, `sed -i`, a heredoc) gets past it: a hook on
`Write|Edit` never sees one. A baseline rewritten that way makes the report, and the line
of §8, compare the Plan with the rewritten text. Nothing closes that here: the audit is
advisory (D-4), and the developer reads the Plan at the gate.

## 8. The line in `plan approve`

When it is about to record an approval, after the acceptance criteria and the invariants
and before the `Acceptance:` line, `plan approve` prints the audit's line. It **computes**
it, with the comparison of §6 and the pairing of §7.1 that `plan audit report` uses, from
`baseline.md`, `findings.md` and the Plan now. It **never reads `report.txt`** (D-4).

First, exactly one of:

```
plan audit: not run
plan audit: findings unreadable
plan audit: READY, for this version of the Plan
plan audit: NOT READY (<n> blocking), for this version of the Plan
plan audit: <READY | NOT READY (<n> blocking)>, but the Plan changed after it
```

- **`not run`:** no `baseline.md`, a `baseline.md` that is not a Plan, or no `findings.md`.
- **`findings unreadable`:** a `findings.md` that is not valid (§3).
- **The verdict** is the report's: `NOT READY` with the number of blocking findings when
  there are any, `READY` otherwise (§4).
- **"This version"** means every change from the baseline to the Plan now is one the
  audit accounts for: the sections are the same, in the same order, text outside every
  section is the same, no section but `## Implementation context` and `## Scope` changed,
  and every hunk of those two has its edit (§7.1). Otherwise **the Plan changed after
  it**: a hunk without a reason is a change the auditor did not explain, so it is one made
  after the audit, or one the auditor would not own up to. The header is not compared here:
  approving writes it.

Then, on a line of its own, for each section other than the two that differs from the
baseline, in Plan order:

```
plan audit: the <section> changed since the audit began
```

and, once, when a section was added, removed, renamed or moved, or text outside every
section changed: `plan audit: the sections changed since the audit began`.

The same lines appear with `--skill`. They refuse nothing.

## 9. Decisions and their reasons

**D-1 — READY is not a check** (1.0.0). An agent's reading of a Plan can be wrong in both
directions, and a word like "valid" or "passed" would let it stand in for the developer's.
So the verdict says what it is, on the line after it, every time: an agent's assessment,
and approving is the developer's. It is computed from the findings alone, so a summary
written by the agent cannot change it, and it gates nothing.

**D-2 — The CLI lists the edits, not the agent** (1.0.0). An agent's account of what it
changed is the agent's word; the diff from the baseline is a fact. The report prints the
diff, refuses any change outside the two sections, and only asks the agent for the reason
of each change, which it pairs by order. A change with no reason shows as one.

**D-3 — The baseline is the whole file, and begin is the developer's** (1.0.0). The header
must not change, so the baseline holds it. `begin` refuses a second time without
`--restart`, and an agent's Bash cannot run it, because a fresh baseline taken after a
forbidden edit would hide that edit from the report. The baseline sits in `.invar/out/`,
where an agent with a shell can still rewrite it. `plan guard` denies a `Write` or `Edit`
of it, and of `report.txt`, so that copying the Plan over the baseline is not one tool call
away (§7.2); a shell write still gets past it. Like the guard elsewhere, this stops the
ordinary case, and the report is advisory.

**D-4 — Advisory, like the review, and computed at the gate** (1.0.0). The developer
reads the Plan at `/invar:approve`; the audit is one more thing to read there, never a
condition. A gate on an agent's judgement would move the decision to the agent. The line
the gate shows is computed again from the baseline, the findings and the Plan now, never
read back from `report.txt`: a file any tool could write would let a hand-written `READY`
stand in for the audit. `report.txt` stays as the developer's copy of the text. "This
version" is therefore what the files can show, that every change since `begin` is one the
audit accounts for, not the moment `report` ran, which an agent could repeat at will. A
section the auditor may not edit that changed since `begin`, the acceptance criteria above
all, gets a line of its own, so that the gate says so even when the report would refuse.

## 10. Version history

**1.0.0** — initial version (#167), released in 0.3.0. Amended in place before that
release, after review: `plan guard` denies `Write` and `Edit` of `baseline.md` and
`report.txt` (§2, §7.2, D-3), and the line of §8 is computed from the baseline, the findings and the Plan now instead of read from
`report.txt`, with `findings unreadable`, "this version" as every change accounted for,
and a line for each section changed that the auditor may not edit (D-4).

Released in 0.3.0 as written here. Any later change bumps the version.
