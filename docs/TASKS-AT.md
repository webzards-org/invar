# Invar — Acceptance tests (test-writer)

Sixteen issues. Goal of the feature: **the coder no longer writes the tests that
judge its own work.** Acceptance criteria become locked, red-before-green
acceptance tests, written by a separate sub-agent.

> `docs/DESIGN.md` (v3.1) does not describe the test-writer yet. Until revision
> v4 lands, the decisions recorded under **Acceptance decisions** below are
> authoritative where they differ from the design document and from L-3 in
> [`TASKS-M1.md`](TASKS-M1.md).

**Branch naming.** `<type>/at-<n>-<slug>`, e.g. `docs/at-1-acceptance-decisions`.

**Labels:** `milestone:AT`, `area:core`, `area:contracts`, `area:plugin`,
`area:ci`, `type:learning`, `type:decision`.

**Every task under `core/`** must leave `cargo fmt --check`,
`cargo clippy -- -D warnings` and `cargo test` passing. Not repeated in each
acceptance list.

Each task below has two lines M1 does not: `**Decisions:**`, the A-n ids it
implements, and `**Issue:**`, its GitHub issue.

---

## Acceptance decisions

Recorded here until `DESIGN.md` v4. Each one is final for this feature unless a
task below explicitly reopens it.

**A-1 — A test-writer sub-agent runs between the gate and the coder.** It turns
each numbered acceptance criterion into executable acceptance tests (Laravel:
Pest/PHPUnit feature tests over HTTP, real database). The coder may not change
them. Unit tests stay the coder's own and are not locked. Acceptance tests can
be derived from the criteria alone. Unit tests cannot: writing them first would
mean inventing the internal design before the code exists, and if another agent
owned them the coder would lose the refactor step.

**A-2 — L-3 is reopened, narrowly.** The core runs one configured command
(`acceptance.run`) and reads one JUnit XML report. No language adapter, no lint,
no wrapping of the team's own tooling. Running the full suite green stays the
team's CI (L-3 otherwise unchanged), and `done` does not run tests.
Red-before-green needs a test result, and JUnit XML is the one format every
relevant runner already writes, so no adapter is needed to get one. This does
not contradict `CONTRIBUTING.md` ("running pytest belongs in an adapter"): that
rule keeps language-specific work out of the core, and the core here runs a
command it does not understand and reads a language-neutral report. It
never parses PHP or knows Pest.

**A-3 — Locks live in the test file, not in a manifest.** One line,
`invar-lock: <TICKET> sha256:<hex>`, near the top of the file, with any comment
prefix. No per-ticket lock files are added to the repository. A manifest per
ticket would duplicate what the file and git already hold, and would pile up
after every PR. Because a deleted test takes its lock with it, deletions have to
be detected from git.

**A-4 — Red before green.** `invar acceptance lock` locks only tests that fail
on an assertion. An error (missing class, missing route) is not red. A test that
passes before any code exists is refused, by name. Negative criteria carry a
control probe in the same test (see AT-11). A test that passes or crashes before
the code exists proves nothing, and the control probe exists because "returns
404" is already true while the route is missing. One side effect is worth
stating: a weakened test cannot be relocked once code exists, because it no
longer fails.

**A-5 — Traceability by criterion id.** Criteria are numbered (`AC1`…), every
test names its criterion, and the criterion's text sits in the test's docblock,
covered by the file digest, so it outlives the Plan. The Plan is deleted in the
PR (§12, L-6), so the text has to survive somewhere the lock protects. The id is
the join key for coverage, review and the PR, and the core never matches prose
to prose.

**A-6 — Whether a ticket needs acceptance tests is a human decision, recorded in
the Plan.** The default comes from `invar.yaml` and can be overridden per
approval. `plan approve` writes it into the Plan header, and the approval covers
it. Skipping is a weaker guarantee, so it belongs to a person, and only the
developer runs `plan approve` (L-8). The Plan is also what every later reader
(coder, reviewer, PR) already reads, and being under the approval means the
field cannot change silently.

**A-7 — The style is configuration, not prompt logic.** `acceptance.style:
integration` now, `gherkin` reserved. The test-writer learns the style from
`invar acceptance spec`, never from branches in its prompt (§13b). Branches in
the prompt would be logic in the plugin. This way the later upgrade to Gherkin
is a change to one command's output, not to the agent.

**A-8 — A locked test the coder believes wrong is `BLOCKED-TEST`, and only the
developer can unlock it.** The unlock is `/invar:relock`, mirroring
`/invar:approve`. The coder must not be the one who decides that a test is
wrong. A corrected test would pass once code exists, so A-4 alone could never
relock it; unlocking therefore has to be explicit and human.

---

## Order

| Task | Issue | Depends on |
|---|---|---|
| AT-1 | #79 | — |
| AT-2 | #80 | AT-1 |
| AT-3 | #81 | AT-2, AT-4 |
| AT-4 | #82 | AT-1 |
| AT-5 | #83 | AT-1 |
| AT-6 | #84 | AT-4, AT-5 |
| AT-7 | #85 | AT-2, AT-3, AT-4 |
| AT-8 | #86 | AT-5, AT-6, AT-7 |
| AT-9 | #87 | AT-2, AT-3, AT-8 |
| AT-10 | #88 | AT-5 |
| AT-11 | #89 | AT-7, AT-8 |
| AT-12 | #90 | AT-3, AT-11 |
| AT-13 | #91 | AT-8, AT-10, AT-12 |
| AT-14 | #92 | AT-9, AT-11 |
| AT-15 | #93 | AT-3, AT-9 |
| AT-16 | #94 | AT-12, AT-13, AT-14, AT-15 |

---

## Open questions

Each one is closed by the task named. A recommendation is not a decision.

- **OQ-1 — Is there a developer override for a green-before-code test?** Closed
  by AT-8: not in this command (`invar acceptance lock`); the test-writer answers
  `BLOCKED` and the developer decides (`acceptance-lock.md` D-12).
- **OQ-2 — Are factories and shared helpers ever locked?** Closed by AT-11 and
  AT-14.
  *Recommendation (not a decision):* the test-writer may add factory states, and
  the review reports coder changes to them (AT-14).
- **OQ-3 — The exact form of the `Acceptance` field inside the approval digest.**
  Closed by AT-3.
  *Recommendation (not a decision):* `Approved-checksum` covers the body plus the
  canonical line `Acceptance: <value>`, and a Plan approved before this contract
  (no field) keeps today's digest.

---

## AT-1 — Decision: acceptance tests and the test-writer

**Branch:** `docs/at-1-acceptance-decisions`
**Labels:** `area:docs`, `type:decision`
**Design:** §3, §4, §4c, §11, §13c, §14.2, §14.3; L-1, L-3
**Decisions:** A-1 … A-8
**Depends on:** —
**Issue:** #79

### Description

Record the decisions behind the test-writer before any code is written, in a new
`docs/TASKS-AT.md` (same shape as `TASKS-M1.md`: decisions first, then this task
list), and link it from `docs/README.md`. Until DESIGN v4 lands, these decisions
are authoritative where they differ from `DESIGN.md`.

The decisions to record are A-1 … A-8, under **Acceptance decisions** above. The
open questions to state, each with the task that closes it, are under **Open
questions** above.

### Purpose

Today the coder writes both the implementation and the tests that judge it, so a
criterion can be quietly weakened in a test without touching the frozen Plan.
Writing the decisions down first keeps the next fifteen tasks consistent, and
makes the L-3 exception a deliberate, bounded one instead of a drift.

### Acceptance criteria

- [ ] `docs/TASKS-AT.md` exists with decisions A-1 … A-8, each with its reason
- [ ] It lists AT-1 … AT-16 with dependencies, and the open questions with the task
      that closes each
- [ ] `docs/README.md` links it; `TASKS-M1.md` L-3 points to A-2 for the exception
- [ ] Every later AT task cites the decisions it implements

---

## AT-2 — Plan contract: numbered criteria and `## Acceptance tests to change`

**Branch:** `feat/at-2-numbered-criteria`
**Labels:** `area:contracts`, `area:core`
**Design:** §4a, §4c, §14.3; L-1
**Decisions:** A-5
**Depends on:** AT-1
**Issue:** #80

### Description

Plan contract 1.6.0 (`contracts/formats/plan-header.md`):

1. **Numbered criteria.** Each item of `## Acceptance criteria` starts with its id:
   `- AC1: Given …, when …, then ….` Ids are `AC` + a positive integer, unique,
   contiguous from `AC1`, in order. A wrapped item keeps one id.
2. **`## Acceptance tests to change`** — optional section. Each item: a path of a
   locked test file (A-3), `change` or `delete`, and a one-line reason. It is how a
   later ticket is allowed to modify or remove another ticket's locked tests
   (§4c). Absent means none.

`plan validate` enforces both. `plan approve` prints criteria with their ids. The
planner prompt (`agents/planner.md`) and its embedded example are updated; the
example still passes `plan validate` in `plugin_prompts.rs`.

Plans are consumable (§12), so a required id format is safe for new Plans; the
version history in the contract says so.

### Purpose

A criterion id is the join key for everything that follows: the test that covers
it, the lock, the review, the PR. Without stable ids the coverage check in AT-8
would have to match prose to prose, which is exactly the judgement the core never
makes. The new section keeps locked tests changeable, but only through a Plan a
person approved.

### Acceptance criteria

- [ ] Contract 1.6.0 documents both rules, with valid and invalid examples and a
      version-history entry
- [ ] `plan validate` refuses: a missing id, a duplicate id, a gap (`AC1`, `AC3`),
      ids out of order; each with a message naming the line
- [ ] `plan validate` refuses a `## Acceptance tests to change` item without a
      path, an action, or a reason
- [ ] `plan approve` output shows each criterion with its id
- [ ] The planner prompt asks for numbered criteria and its example validates

---

## AT-3 — `Acceptance` in the Plan header, set at approval

**Branch:** `feat/at-3-acceptance-in-plan`
**Labels:** `area:contracts`, `area:core`
**Design:** §4a, §4d, §14.3; L-8
**Decisions:** A-6
**Depends on:** AT-2, AT-4
**Issue:** #81

### Description

A new Plan header field, `Acceptance: required | skipped`, written **only** by
`invar plan approve`, together with the other approval fields.

- `invar plan approve <TICKET> --acceptance` / `--no-acceptance` sets it; without
  either flag it takes the value of the approval being replaced, then
  `acceptance.required` from `invar.yaml` (AT-4), then `skipped`
  (`plan-header.md` §5.3). `--reapprove` keeps the recorded value only as far as
  the digest confirms it (D-20).
- The field is part of the approval: an agent that changes `required` to
  `skipped` after approval must be detected. Decide and document how (open
  question in AT-1): recommended is that `Approved-checksum` covers the body plus
  the canonical line `Acceptance: <value>`, and that a Plan approved before this
  contract (no field) keeps today's digest.
- `invar plan check` prints one fixed line, `acceptance: required` or
  `acceptance: skipped`, so the dispatcher reads the decision from disk (AT-12).
- `--acceptance` / `--no-acceptance` are passed through by `--skill`.

### Purpose

Skipping acceptance tests is a weaker guarantee, so it has to be a person's
decision, recorded where every later reader looks — coder, reviewer, PR — and
impossible to flip silently. Putting it in the approval makes it the developer's
by construction, since only the developer runs `plan approve` (L-8).

### Acceptance criteria

- [ ] Plan contract documents `Acceptance`: values, who writes it, what the
      approval digest covers, and backward compatibility
- [ ] `plan approve` writes the field from flag, config or default, in that order
- [ ] Changing the field after approval makes `plan check` and `done` refuse, with
      the same "changed after it was approved" message family
- [ ] A Plan approved under the previous contract still checks clean
- [ ] `plan check` prints the `acceptance:` line; `tests/plan_check.rs` covers both
      values

---

## AT-4 — `invar.yaml`: the `acceptance` section

**Branch:** `feat/at-4-acceptance-config`
**Labels:** `area:contracts`, `area:core`
**Design:** §6, §14.3
**Decisions:** A-2, A-6, A-7
**Depends on:** AT-1
**Issue:** #82

### Description

`invar.yaml` contract 2.2.0 adds an `acceptance:` section:

```yaml
acceptance:
  required: true                 # default for `plan approve`; true when the section is present
  style: integration             # integration | gherkin (reserved: an error for now)
  dir: tests/Acceptance          # where acceptance tests live; required
  run: ["vendor/bin/pest", "--log-junit", "{report}", "{files}"]   # required
  fakes_allowed: [http, mail, queue, notification]
```

- **Absent section = previous behavior**: `plan approve` defaults to `skipped`, no
  test-writer (§14.3). `invar init` writes the section **commented out**, because
  `run` and `dir` depend on the project (`invar-yaml.md` D-12).
- `run` is a list run without a shell (D-11): element 0 is the program. `{report}`
  expands to one path the core chooses, `{files}` to the test files as separate
  arguments, and each appears exactly once as a whole element. The exit code is
  ignored; the report is the result (AT-6).
- `plan approve --acceptance` is refused with no section, naming what to add, and a
  recorded `required` kept with no section warns (`plan-header.md` §5.3).
- `fakes_allowed` is not interpreted by the core; it is passed to the test-writer
  through `invar acceptance spec` (AT-7).
- Reader and errors follow the existing contract style (unknown keys warn, YAML 1.1
  booleans are errors).

### Purpose

Keeps the core ignorant of Pest, PHPUnit or any framework: what to run and where
tests live is the project's configuration. It is also the seam for the later
upgrade to `style: gherkin` without touching the core's checks.

### Acceptance criteria

- [ ] Contract 2.2.0 documents every key, its default, examples and version history
- [ ] The reader accepts the example above and an absent section
- [ ] `run` without `{report}` is an error; `style: gherkin` is an error naming the
      reserved value; an unknown `fakes_allowed` value is accepted as-is
- [ ] `invar init` writes the section commented out; existing configs are not
      rewritten

---

## AT-5 — Contract and parser: the `invar-lock:` line

**Branch:** `feat/at-5-lock-marker`
**Labels:** `area:contracts`, `area:core`
**Design:** §4d, §14.3
**Decisions:** A-3, A-5
**Depends on:** AT-1
**Issue:** #83

### Description

New contract `contracts/formats/acceptance-lock.md`, and its parser and digest in
the core (`format/` and `digest/` kept separate, as today).

- **The line:** `invar-lock: <TICKET>` (pending) or
  `invar-lock: <TICKET> sha256:<64 hex>` (locked), within the first 10 lines;
  anything before `invar-lock:` on that line is ignored, so any comment syntax
  works (`//`, `#`, `--`). More than one such line anywhere is an error, and so is
  one after line 10 or a malformed `sha256:` field.
- **The digest:** the §4d canonicalization over the whole file **with that line
  removed**, recorded as `sha256:<hex>`.
- **One ticket per file.** The ticket on the line is the ticket that last locked it;
  a later approved change (AT-2) replaces it.
- Fixtures under `contracts/fixtures/acceptance/`: PHP, Python and JS comment
  styles, CRLF, a missing digest, two marker lines, a marker on line 11.

### Purpose

This is the whole lock mechanism: self-contained in the file, readable in CI after
the Plan is gone, with no extra files accumulating in the repository. Defining it
as plain text keeps the core language-neutral; the core never learns a comment
syntax.

### Acceptance criteria

- [ ] Contract with the line grammar, search window, digest rule, decisions and
      examples
- [ ] Parser returns pending / locked / none / error for every fixture as the
      fixture's expected file says
- [ ] Digest ignores the marker line and is stable across CRLF/LF
- [ ] `invar hash` (or an equivalent debug flag) can print a file's lock digest

---

## AT-6 — Contract and reader: JUnit XML results

**Branch:** `feat/at-6-junit-results`
**Labels:** `area:contracts`, `area:core`
**Design:** §14.2, §14.3
**Decisions:** A-2, A-4, A-5
**Depends on:** AT-4, AT-5
**Issue:** #84

### Description

New contract `contracts/formats/test-results.md` and a reader for the subset of
JUnit XML the core relies on:

- `testcase` with `name` and `file` (Pest and PHPUnit write both; pytest and Vitest
  can). Outcome per testcase: `passed`, `failure` (an assertion), `error`
  (exception, missing class), `skipped`.
- **Criterion from the name:** the token `AC<n>` as a whole word, case-insensitive,
  `_` or `-` allowed between `AC` and the number (`it('AC3: …')`,
  `test_ac3_…`). **Ticket from the file's lock line** (AT-5), never from the name.
- A testcase with no criterion token, or with two, is reported, not guessed.
- A missing or unparsable report is an error; the command's exit code is not read.
- `file` is normalised: Pest's `::<test name>` suffix is removed, an absolute path is
  made relative to the project root, and one outside the root is an error.

Fixtures: real reports produced by Pest and PHPUnit on a small sample, checked in.

### Purpose

JUnit XML is the one result format every relevant test runner already writes, so
the core can read verdicts without an adapter (A-2). The assertion-versus-error
distinction is what makes red-before-green meaningful: a test that fails because a
class does not exist yet proves nothing.

### Acceptance criteria

- [ ] Contract documents the subset, the criterion token rule and every error
- [ ] Reader classifies every fixture testcase correctly, including `error` vs
      `failure`
- [ ] Pest and PHPUnit sample reports from the pilot stack are in the fixtures
- [ ] A testcase without a `file` attribute is an error naming the testcase

---

## AT-7 — `invar acceptance spec`

**Branch:** `feat/at-7-acceptance-spec`
**Labels:** `area:core`
**Design:** §5c, §13b
**Decisions:** A-7
**Depends on:** AT-2, AT-3, AT-4
**Issue:** #85

### Description

`invar acceptance spec <TICKET>` prints everything the test-writer needs, and is
its first command after `plan check`:

- style, `dir`, the `run` command, `fakes_allowed`
- the conventions: one ticket per file, the pending `invar-lock:` line, the `AC<n>`
  token in each test name, the criterion text in the docblock, a control probe for
  negative criteria
- every criterion with its id, from the approved Plan
- `## Implementation context` and `## Scope` of the Plan

Refuses when the Plan is not approved, changed after approval, or
`Acceptance: skipped`; when `invar.yaml` has no `acceptance:` section; and when the
criteria have no ids.

**No size cap.** The hook's 10,000-character cap was dropped from this task: the
test-writer runs this command through Bash, so no hook delivers its output and the cap
does not apply, and cutting Implementation context to fit would hide approved text from
the agent that must test it. Everything is printed.

### Purpose

The test-writer's prompt stays one fixed text for every style and framework; what
changes with configuration is this command's output (§13b, A-7). Upgrading to
Gherkin later is a change to this command, not to the agent.

### Acceptance criteria

- [ ] Output for the contract's example Plan is pinned by a snapshot test
- [ ] Refusals for draft, changed and `skipped` Plans, each with its message
- [ ] No text of `BUSINESS_CONTEXT.md` beyond what the Plan quotes, and nothing
      from `.invar/decisions/`
- [ ] Nothing is cut for size, and that is tested with a large Plan

---

## AT-8 — `invar acceptance lock`: coverage and red before green

**Branch:** `feat/at-8-acceptance-lock`
**Labels:** `area:core`
**Design:** §4, §4d
**Decisions:** A-2, A-4, A-5
**Depends on:** AT-5, AT-6, AT-7
**Issue:** #86

### Description

`invar acceptance lock <TICKET>`:

1. Finds the files under `acceptance.dir` with a **pending** line for the ticket.
2. Runs `acceptance.run` on them and reads the report (AT-6).
3. Checks **coverage**: every criterion of the Plan has at least one test.
4. Checks **red**: every test fails with `failure`. `error` refuses ("fails for the
   wrong reason"); `passed` refuses ("passes before the code exists"), naming each.
5. Only if all hold, writes the digest into each file's lock line.

Locking is the agent's step (the test-writer runs it). It cannot be used to relock a
weakened test once code exists, because those tests no longer fail (A-4).

This closes the open question from AT-1, whether a developer may lock a green test (e.g.
a criterion describing behavior that already exists): not in this command; the
test-writer answers `BLOCKED` and the developer decides.

**Note from AT-6:** an empty report means the tests did not run. Pest and PHPUnit write
0 bytes when they cannot load the tests, and the reader reports that as **Empty**. It is
refused as "the tests did not run", with the runner's stderr, and nothing about coverage
or red.

The behavior is in `contracts/formats/acceptance-lock.md` §9. A failure between two
renames is the only partial write, and its message names what was written (D-15).

### Purpose

This is the step that turns "tests the test-writer wrote" into "tests nobody can
weaken": complete against the criteria, proven to test something, and frozen.

### Acceptance criteria

- [ ] Locks a pending file whose tests all fail on assertions, and writes the digest
- [ ] Refuses, naming test and criterion: an uncovered criterion, an `error`, a
      `passed` test, a test without an `AC` token, a token for a criterion the Plan
      lacks
- [ ] Writes nothing on any refusal (all or nothing)
- [ ] Refuses on a Plan that is not approved or `Acceptance: skipped`
- [ ] End-to-end test on a tiny fixture project with a fake `run` command that
      writes a canned report
- [ ] An empty report is refused as "the tests did not run", with the runner's
      stderr, and nothing is locked

---

## AT-9 — Verify locks in `plan check`, `done` and CI

**Branch:** `feat/at-9-verify-locks`
**Labels:** `area:core`
**Design:** §4c, §4d, §10
**Decisions:** A-3, A-6
**Depends on:** AT-2, AT-3, AT-8
**Issue:** #87

### Description

One check with three parts (`contracts/formats/acceptance-lock.md` §10), and three
callers:

- **Integrity:** every file under `acceptance.dir`: a locked line has the file's digest,
  and no line is malformed. Needs no git and no Plan.
- **Branch rule:** a test locked by **another** ticket at the merge base, compared with
  the working tree, the index and `HEAD`, with no rename detection. Changed only if the
  current Plan lists it in `## Acceptance tests to change` with `change`; deleted (content
  read from git) only with `delete`. A declared path that is not a locked test, or did not
  change, is a warning; one that breaks the path rules is a problem. A file locked for the
  current ticket is its own.
- **Closing:** with `Acceptance: required`, at least one locked test for the ticket; with
  either value, no pending line left for it.

| Command | Checks | Pending lines |
|---|---|---|
| `plan check <TICKET>` | Integrity, branch rule | Allowed |
| `done <TICKET>` | All three, before it deletes anything | The ticket's refused |
| `invar acceptance verify <TICKET>` | All three, deleting nothing | The ticket's refused |
| `invar acceptance verify` | Integrity, for CI: no Plan, no git | Any refused |

Outside a git repository the branch rule is skipped with a warning; inside one, a base
branch or merge base that cannot be found is a problem. With no `acceptance:` section,
`verify` warns and exits 0, and `plan check` and `done` warn for a `required` Plan. Removing the section disables the check, and review and the decision log are what
notice it (D-21). A digest is not a signature (D-20).

`done` still runs no tests (A-2); green is the team's CI.

### Purpose

The lock is only worth something if every closing step checks it, including in CI
after the Plan is gone. Reading deletions from git is what lets the design go
without a manifest (A-3).

### Acceptance criteria

- [ ] A one-character change in a locked test makes `verify`, `plan check` and
      `done` refuse, naming the file and its ticket
- [ ] A change to another ticket's locked test passes only when the Plan declares
      it; same for a deletion
- [ ] `done` refuses with `required` and no locked test, and with a pending line left
- [ ] `verify` without a ticket checks the whole directory and is CI-safe (no Plan
      needed)

---

## AT-10 — Guard: locked tests and `--relock`

**Branch:** `feat/at-10-guard-locked-tests`
**Labels:** `area:core`, `area:plugin`
**Design:** §11, §13
**Decisions:** A-3, A-8
**Depends on:** AT-5
**Issue:** #88

### Description

Extend `invar plan guard` (the `PreToolUse` hook on `Write|Edit|Bash`):

- **Deny** `Write` and `Edit` on a file whose lock line is **locked**, with a reason
  that says what the file is, which ticket locked it, and what to do instead
  (answer `BLOCKED-TEST`, AT-13). Pending files are not guarded.
- Add `acceptance lock --relock` to `HUMAN_ONLY` (AT-13).
- Same failure rule as today: any doubt lets the call through; exit 2 only for deny.

### Purpose

A convenience that saves the coder a wasted edit, exactly like the guard on an
approved Plan. The boundary remains the digest (AT-9); `sed -i` still gets past the
guard and is still caught at `plan check` and `done`.

### Acceptance criteria

- [ ] `tests/plan_guard.rs`: deny on a locked file, pass on a pending file and on a
      file without a line
- [ ] Reason text names the ticket and `BLOCKED-TEST`
- [ ] `--relock` from the agent's Bash is denied; `--help` is allowed
- [ ] NOTES.md records the guard is not a boundary, with the `sed -i` case

---

## AT-11 — Test-writer sub-agent

**Branch:** `feat/at-11-test-writer-agent`
**Labels:** `area:plugin`
**Design:** §13a, §13c, §13d
**Decisions:** A-1, A-4, A-7
**Depends on:** AT-7, AT-8
**Issue:** #89

### Description

`plugin/agents/test-writer.md`. Started by the main session with the ticket line
only. It:

1. runs `invar plan check <TICKET>`, then `invar acceptance spec <TICKET>`
2. writes acceptance tests only under `acceptance.dir` (and helpers there): one file
   per ticket, pending lock line, `AC<n>` in every test name, criterion text in the
   docblock
3. uses the real database and the app's own scopes and policies; fakes only what
   `fakes_allowed` lists
4. gives every negative criterion (403, 404, refused) a **control probe** in the same
   test, so it fails before the code exists for the right reason
5. runs `invar acceptance lock <TICKET>` and fixes its own tests until it locks

Never edits application code, the Plan, or a locked file. Reads neither
`BUSINESS_CONTEXT.md` nor `.invar/decisions/`.

Answers `LOCKED` + at most eight lines (files, tests per criterion), or
`BLOCKED <reason>` (a criterion it cannot test, a green-before-code test it cannot
make red, missing test infrastructure).

Decide (AT-1 open question) whether factories it creates or changes are in scope;
recommended: it may add factory states, and the review reports coder changes to
them (AT-14).

### Purpose

The agent that writes the tests has no reason to weaken them: it is not fighting
code that won't pass. Separating it from the coder is the point of the feature.

### Acceptance criteria

- [ ] Agent file exists; `claude plugin validate --strict` passes
- [ ] `plugin_prompts.rs`: every command and flag named exists; both answer shapes
      present; no pointer to `.invar/decisions/` or `BUSINESS_CONTEXT.md`; no
      style-specific branches in the prompt
- [ ] Model default chosen per the M1-22 finding and recorded in NOTES.md
- [ ] On walkthrough ticket 1 it answers `LOCKED` with every criterion covered

---

## AT-12 — `/invar:approve` flags and the dispatcher

**Branch:** `feat/at-12-dispatch-test-writer`
**Labels:** `area:plugin`
**Design:** §13, §13c; L-8
**Decisions:** A-6
**Depends on:** AT-3, AT-11
**Issue:** #90

### Description

- `/invar:approve` accepts `--acceptance` / `--no-acceptance` and passes them to
  `invar plan approve`; `argument-hint` updated. The argument check in the skill
  allows exactly these flags.
- The protocol skill (`skills/invar/SKILL.md`): after a successful approval the main
  session runs `invar plan check` and reads the `acceptance:` line. `required` →
  start the test-writer; on `LOCKED`, start the coder. `skipped` → start the coder.
  On the test-writer's `BLOCKED`, report to the developer and stop.
- The decision comes from the line on disk, never from the conversation.

### Purpose

Approval stays the developer's only manual step (L-8), and it now also decides
whether this ticket gets locked tests. The dispatcher never chooses on its own.

### Acceptance criteria

- [ ] `plugin_prompts.rs`: both flags handled; dispatch is conditional on the
      `acceptance:` line; `LOCKED` precedes the coder start
- [ ] Walkthrough: `/invar:approve T` starts the test-writer, then the coder;
      `/invar:approve T --no-acceptance` starts the coder directly
- [ ] A `BLOCKED` from the test-writer starts no coder

**Note from AT-3 (#97).** `invar plan approve --acceptance --no-acceptance` is
refused by clap, which exits 1 before `--skill` is handled. A skill's shell
block that exits non-zero loses its output, so `/invar:approve T --acceptance
--no-acceptance` lands in the skill's "no output at all" path instead of
showing an `error:` line. Either check the conflict in code, so `--skill`
formats it, or have the skill's argument check refuse the pair before the
shell block runs.

- [ ] `/invar:approve T --acceptance --no-acceptance` shows an `error:` line
      naming both flags, not the "no output at all" message

---

## AT-13 — `BLOCKED-TEST` and `/invar:relock`

**Branch:** `feat/at-13-blocked-test-relock`
**Labels:** `area:core`, `area:plugin`
**Design:** §4c, §13c; L-8
**Decisions:** A-8
**Depends on:** AT-8, AT-10, AT-12
**Issue:** #91

### Description

- **Coder:** `agents/coder.md` gains a third answer, `BLOCKED-TEST <file> <AC> <reason>`,
  when it believes a locked test is wrong. It leaves its code in place. With
  `Acceptance: required` it never writes tests with a lock line; with `skipped` it
  writes its own tests without one.
- **Main session:** reports `BLOCKED-TEST` to the developer and stops.
- **`/invar:relock <TICKET> [file]`:** a skill with `disable-model-invocation: true`
  and shell injection, like `/invar:approve`. It runs
  `invar acceptance lock <TICKET> --relock`: clears the digest so the test-writer can
  fix the test, and on the next `lock` the red check is skipped **only** for files
  the developer relocked. `--relock` is human-only (AT-10).
- After the developer relocks, the main session starts the test-writer for the fix,
  then the coder again. At most two such rounds; then the cycle is `failed`.

### Purpose

A test can be wrong, and the coder must not be the one to decide that. Unlocking is
a person's decision, exactly as approval is, so a weakening is never automatic.

### Acceptance criteria

- [ ] `plugin_prompts.rs`: the coder has all three answer shapes; the relock skill
      has `disable-model-invocation: true` spelled exactly
- [ ] `--relock` skips the red check only for the named files, and is recorded in
      the command output
- [ ] Walkthrough ticket with a seeded wrong test ends in `BLOCKED-TEST`, a relock,
      and `DONE`; no criterion text changed

---

## AT-14 — Review: does each locked test assert its criterion

**Branch:** `feat/at-14-review-acceptance`
**Labels:** `area:core`, `area:plugin`
**Design:** §13c; L-7
**Decisions:** A-5
**Depends on:** AT-9, AT-11
**Issue:** #92

### Description

- `invar review-input` adds: the `Acceptance` value, and for each criterion the
  locked tests that cover it (file and test name).
- `agents/reviewer.md`, per criterion: does each locked test actually assert what
  the criterion says? (`asserts`, `weaker`, `unrelated`, with `file:line`.)
- New heading: **Shared test code changed by the coder** — factories, seeders or
  helpers used by a locked test and modified on the branch.
- With `Acceptance: skipped`: a heading listing criteria no test covers, as
  information, not a defect.
- `agents/reviewer.md` learns `## Acceptance tests to change` (Plan contract
  1.6.0, AT-2): `review-input` already shows it after the criteria. A change to
  a locked test that the Plan declares there is not reported as outside Scope;
  a change to another ticket's locked test that it does **not** declare is.
- Seeded branch in `eval/review-seeds.sh`: a locked test whose assertion is weaker
  than its criterion.

### Purpose

Coverage and red-before-green are mechanical; whether a test means what its
criterion says is a judgement. The review is where that judgement lives, advisory
as always (L-7).

### Acceptance criteria

- [ ] `review-input` snapshot shows the new sections; nothing from
      `.invar/decisions/`
- [ ] The weak-test seed is reported as `weaker` with `file:line`
- [ ] A factory change on a locked test's path is reported under its heading
- [ ] `plugin_prompts.rs`: the reviewer prompt names `## Acceptance tests to change`
      and says how declared changes are treated
- [ ] `tests/review_input.rs`: `done` still neither calls nor waits for the review

---

## AT-15 — `acceptance` in the decision entry and the PR description

**Branch:** `feat/at-15-acceptance-in-records`
**Labels:** `area:contracts`, `area:core`, `area:plugin`
**Design:** §7; L-6
**Decisions:** A-6
**Depends on:** AT-3, AT-9
**Issue:** #93

### Description

- Decision entry contract 1.4.0: optional header key `acceptance: required | skipped`.
  `/invar:log` drafts it from the Plan; `invar decision approve` validates it.
  Absent means unknown (entries before this contract).
- PR description (`pr_body`): shows `Acceptance` and, when required, the locked
  tests per criterion; when skipped, says so plainly in the first lines.

### Purpose

The Plan is deleted in the PR, so after merge the decision entry is the only place
that remembers whether a ticket was closed without acceptance tests. As a header
key it can be counted — how often teams skip is the pilot's key metric.

### Acceptance criteria

- [ ] Contract 1.4.0 with the key, its values and compatibility
- [ ] `/invar:log` drafts include the key; `decision approve` rejects other values
- [ ] `pr_body` snapshot tests for `required` and `skipped`

---

## AT-16 — Pilot walkthrough on the Laravel project

**Branch:** `docs/at-16-pilot-walkthrough`
**Labels:** `area:docs`, `area:plugin`
**Design:** §10, §16
**Decisions:** A-1 … A-8
**Depends on:** AT-12, AT-13, AT-14, AT-15
**Issue:** #94

### Description

Run the feature on the Laravel pilot and write it down.

- **Setup, once, by hand:** test database matching production (not SQLite
  in-memory), `RefreshDatabase`, base factories, the test-style convention in
  `AI_CONTEXT.md` (§5a), `acceptance:` in `invar.yaml`.
- **Walkthrough** (new section in `plugin/NOTES.md`): a ticket touching an
  invariant; a ticket with a negative criterion (403/404); an ordinary ticket; a
  seeded wrong test (`BLOCKED-TEST`); a ticket approved with `--no-acceptance`.
- **Record per run:** tests green before code; `BLOCKED` / `BLOCKED-TEST` and
  whether justified; review findings of `weaker` tests; files the test-writer read.
- **Docs:** CHANGELOG entries; DESIGN revision note (or v4) for A-1 … A-8; `INSTALL.md`
  for the pilot setup.

### Purpose

Whether separate, locked acceptance tests actually stop false positives is a
property of agents on real tickets, not of the code. This task produces the
evidence, and the numbers that decide whether Gherkin (`style: gherkin`) or a human
look at tests is needed next.

### Acceptance criteria

- [ ] The five walkthrough tickets are run and recorded, with the metrics above
- [ ] At least one negative criterion locks only thanks to its control probe
- [ ] The `--no-acceptance` ticket's decision entry and PR show `skipped`
- [ ] CHANGELOG, DESIGN revision and INSTALL updated
