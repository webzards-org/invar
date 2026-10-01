# Invar — M1 Light gate

Twenty-one issues. Goal of the milestone: **a team can run the whole cycle —
plan, approve, implement, review, close — inside Claude Code on a Laravel + Vue
project, and the approved Plan cannot be moved without it being visible.**

M1 is the first phase of the full design, not a fork of it. It ships the parts
that need no language adapter: the Plan and its gate, the business and project
context, the decision log, and the plugin that carries them. Feature files,
lint and test execution arrive in later milestones; nothing built here is
thrown away when they do.

> **Supersedes** the previous M1 ("Core gate", sixteen issues). That milestone
> built the `.feature` gate first and the plugin last (M5). The order is now
> reversed: the Plan gate and the plugin come first, because they are what a
> real team can use and measure today, and because they need no adapter. The
> mapping from old to new issue ids is at the end of this file.
>
> `docs/DESIGN.md` (v3) does not describe light mode yet. Until revision v4
> lands, the decisions recorded under **Light-mode decisions** below are
> authoritative where they differ from the design document.

**Branch naming.** `<type>/<task-id>-<slug>`, e.g. `feat/m1-1-canonicalization`.
The task id must appear as a substring (§12, `clean-spec --merged`).

**Labels:** `milestone:M1`, `area:core`, `area:contracts`, `area:plugin`,
`area:ci`, `good-first-rust`, `type:learning`, `type:decision`.

**Every task under `core/`** must leave `cargo fmt --check`,
`cargo clippy -- -D warnings` and `cargo test` passing. Not repeated in each
acceptance list.

---

## Light-mode decisions

Recorded here until `DESIGN.md` v4. Each one is final for M1 unless a task
below explicitly reopens it.

**L-1 — No `.feature` files in M1.** Acceptance criteria live in the Plan, as
prose, under `## Acceptance criteria`. They are covered by `Approved-checksum`
like the rest of the body, so they are frozen at approval exactly as a
confirmed scenario would be. When feature tests arrive, they are written *from*
this section.

**L-2 — `scenarios:` in `invar.yaml` selects the Plan shape.** `false` (and
absent) means light mode: `## Acceptance criteria` is required and the two
`## Scenarios to …` sections are not. `true` is reserved for the milestone that
introduces `.feature` files. Absent means `false` because that is the behavior
of every config written before scenarios exist — the compatibility rule of
§14.3 applied forward.

**L-3 — No language adapter, no lint, no test execution.** The project's own
tooling (PHPStan/Larastan, Deptrac, PHP_CodeSniffer, ESLint, Pest/PHPUnit,
Vitest) stays where it is and is run the way the team already runs it. `invar`
neither wraps nor replaces it. Consequently the core stays language-neutral and
Laravel + Vue support needs no adapter at all.

**L-4 — `invar done` replaces `invar generate` for M1.** With no tests to run,
closing a cycle means: verify the approved Plan was not changed, write the
decision entry, produce the PR description, remove the Plan. `generate` (or
`verify`, §16 open question) returns when tests do.

**L-5 — No `Lockfile-checksum`.** It records an observation that only matters
once package context exists (§5b, M6). The key becomes optional in the Plan
header; absent means no lockfile observation.

**L-6 — The Plan reaches the PR as its description.** A Plan added and deleted
inside the same PR does not appear in the net diff, so neither a reviewer nor
CodeRabbit would see it at review time. `invar done` therefore renders the Plan
into a PR description before removing the file. The decision entry remains the
permanent record (§7).

**L-7 — `/invar:review` is advisory, never a gate.** A review is an LLM
judgement: non-deterministic, with false positives and misses. It never blocks
`done`, and its result is information for the developer and the PR, not a
verdict. Every verdict is still rendered by the CLI (§11).

**L-8 — The developer's only manual step is approval.** Everything else is run
by the agent or a hook. Approval is the `/invar:approve` command, which the
model is not allowed to invoke on its own; `invar plan approve` remains the
underlying CLI command and works the same from a terminal.

**L-9 — Not every ticket needs a Plan.** The protocol skill tells the agent to
use the cycle when a ticket touches business logic or the developer asks for
it, and to say so — and skip it — for trivial work (typos, copy changes,
dependency bumps). A tool that demands a Plan for a one-line fix gets switched
off.

---

## M1-1 — Canonicalization

**Branch:** `feat/m1-1-canonicalization`
**Labels:** `milestone:M1`, `area:core`, `good-first-rust`
**Design:** §4d (Checksums — what is hashed)
**Contract:** `contracts/fixtures/digest/` (28 files, 7 groups, `digests.json`)
**Depends on:** M0-2, M0-5

### Description

The five-step transformation of §4d: decode UTF-8, strip a BOM, normalise line
endings, strip trailing whitespace per line, strip trailing blank lines and end
with exactly one `\n`.

A pure function over text. **No file I/O, and no knowledge of Plan structure.**
Body extraction lives in `format/`, not here. That separation is what makes the
fixtures testable on their own.

Unchanged from the previous M1: the Plan checksum rests on "same content always
produces the same digest" exactly as the `.feature` checksum would have. A
Windows developer with `core.autocrlf=true` must not invalidate an approval by
checking out a branch.

### Acceptance criteria

- [ ] `canonicalize(&str) -> String` in `digest/`, with no I/O and no `format/`
      imports
- [ ] Every fixture in `contracts/fixtures/digest/` canonicalizes to its
      group's content
- [ ] Idempotent: canonicalizing already-canonical text changes nothing
- [ ] An empty result is the **empty string**, not `"\n"` (fixtures README, D-1)
- [ ] Interior and leading blank lines are preserved; only trailing ones are
      stripped
- [ ] Leading whitespace on a line is preserved; trailing is stripped
- [ ] Invalid UTF-8 produces a clear error, not a panic and not lossy
      replacement

### Learning notes

- **`&str` in, `String` out.** The input is borrowed, the output is owned.
- **`split('\n')` vs `lines()`.** `lines()` handles `\r\n` for you, which hides
  whether normalisation works. Do step 2 explicitly, then split on `'\n'`.
- **`trim_end()`** does the per-line work (fixtures README D-2).
- **Cloning is fine.** Do not fight the borrow checker for performance nothing
  needs.

---

## M1-2 — Digest and `invar hash`

**Branch:** `feat/m1-2-digest-and-hash`
**Labels:** `milestone:M1`, `area:core`, `good-first-rust`
**Design:** §4d
**Contract:** `contracts/fixtures/digest/digests.json`
**Depends on:** M1-1

### Description

SHA-256 over canonicalized text, rendered as `sha256:<64 lowercase hex>`. Plus
`invar hash <file>`, replacing the M0-2 stub. It hashes the **whole file**, not
a body; say so in `--help`.

Write this against `digests.json`, not against `generate.py` — reading the
Python first would turn an independent check into a translation.

### Acceptance criteria

- [ ] `digest(&str) -> String` returning the `sha256:` form, lowercase hex
- [ ] A test walks all 7 groups and 28 files in `digests.json` and asserts
      each file's digest equals its group's `expected_digest`
- [ ] The same test asserts **no two groups share a digest**
- [ ] Fixtures are read as **bytes**, never through a line-oriented reader
- [ ] The `empty` group yields `sha256:e3b0c442…b855`
- [ ] `invar hash <file>` prints the digest; a missing file exits non-zero with
      a clear message
- [ ] The `unimplemented_command_fails_loudly` test in `tests/cli.rs` is
      replaced by one that runs the real `hash`
- [ ] `--help` states that the whole file is hashed

---

## M1-3 — `invar.yaml`: contract and reader

**Branch:** `feat/m1-3-invar-yaml`
**Labels:** `milestone:M1`, `area:contracts`, `area:core`
**Design:** §6, §14.3; L-2, L-5, L-7
**Contract:** writes `contracts/formats/invar-yaml.md` (new)
**Depends on:** M0-4, M1-1

### Description

`invar.yaml` is a public contract (§14.3) without a specification. Write it,
then read it.

The M1 key set is smaller than §6 and that is deliberate. Document every key
M1 reads, and list the §6 keys that M1 ignores, so a config written for a later
milestone does not break this one:

| Key | Type | Default | Meaning in M1 |
|---|---|---|---|
| `scenarios` | bool | `false` | Plan shape (L-2) |
| `review.offer` | bool | `false` | Whether the protocol skill offers `/invar:review` at the end of Phase 2 (L-7) |
| `git.base_branch` | string | detected (§12) | Base for the review diff and `--merged` |
| `context.binding_limit` | int | `9000` | Character ceiling for `context show --binding` (M1-14) |

Unknown keys warn and never fail — a config written by a newer `invar` must not
break an older one.

### Acceptance criteria

- [ ] `contracts/formats/invar-yaml.md` exists, versioned, with the
      public-contract line, every M1 key with its default, the list of §6 keys
      M1 ignores, one valid example and two invalid ones
- [ ] Config parsed into a struct; unknown keys warn and do not fail
- [ ] A missing `invar.yaml` is a distinct error naming `invar init`
- [ ] Defaults are applied and testable: an empty file produces the documented
      behavior
- [ ] `scenarios: true` is accepted by the reader but rejected by every command
      with a message saying feature files arrive in a later milestone

### Decision to make

**Which YAML parser, if any.** `invar.yaml` is not round-tripped, so a real
YAML crate is acceptable here (unlike the Plan header, M1-6). `serde_yaml` is
unmaintained; check what the ecosystem currently recommends, and consider
whether a flat reader is enough for four keys.

---

## M1-4 — Project root discovery

**Branch:** `feat/m1-4-project-root`
**Labels:** `milestone:M1`, `area:core`
**Design:** §6
**Depends on:** M1-3

### Description

**The root is the directory containing `invar.yaml`**, found by walking upward
from the current directory. A hook fires with whatever `cwd` Claude Code gives
it; a command run from `app/Http/` must behave exactly as one run from the root,
or the same check produces different verdicts depending on who ran it.

Stop at the filesystem root and at a `.git` boundary, and say which.

### Acceptance criteria

- [ ] `project_root() -> Result<PathBuf>` walks upward for `invar.yaml`
- [ ] Not found is a clear error naming the directories searched and suggesting
      `invar init`
- [ ] A test builds a nested temp tree and asserts identical resolution from
      the root and from two levels down
- [ ] All later path building goes through this module
- [ ] Paths are compared in a canonical form

---

## M1-5 — Plan contract v1.1.0: light mode

**Branch:** `feat/m1-5-plan-contract-light`
**Labels:** `milestone:M1`, `area:contracts`, `type:decision`
**Design:** §4a, §14.3; L-1, L-2, L-5
**Contract:** `contracts/formats/plan-header.md` 1.0.0 → 1.1.0
**Depends on:** M1-3

### Description

A contract change, made before any code reads the Plan, so the parser is
written once against the shape M1 actually uses.

**Required sections when `scenarios: false`**, in this order:

```markdown
## Ticket
## Intent
## Scope
## Open questions / answers
## Rejected alternatives
## Acceptance criteria
## Implementation context
```

`## Scenarios to write` and `## Scenarios to delete` are not required in light
mode and, if present, are ordinary sections the core ignores. Their rules
(§6.2, D-5) stay in the contract, marked as applying when `scenarios: true`.

**`## Acceptance criteria`** — new section:

- Must be non-empty. A Plan with nothing to accept cannot be reviewed against,
  approved meaningfully, or turned into tests later.
- Recommended form is one criterion per list item, in Given/When/Then prose.
  The core does **not** parse it; the form is fixed for the reader, for
  `/invar:review` (M1-20), and for the later conversion into `.feature` files.
- Every `BUSINESS_CONTEXT` invariant named in `## Implementation context`
  should have a criterion that would fail if the invariant were broken. Stated
  as guidance to the planner, not checked mechanically — say so in the
  contract.

**`Lockfile-checksum` becomes optional** (L-5): absent and empty both mean no
observation. The "four command-written keys present from creation" rule (D-3)
becomes three.

Two stale items in `contracts/formats/README.md` are fixed in the same PR: the
"Date formats" open question was resolved by D-6, and the implementing-task
column still names old issue ids.

### Acceptance criteria

- [ ] `plan-header.md` is version 1.1.0, with the reason recorded
- [ ] Section list for `scenarios: false` specified, in fixed order
- [ ] `## Acceptance criteria` specified: non-empty, recommended form, not
      parsed, its role for review and later tests
- [ ] `Lockfile-checksum` optional; D-3 updated
- [ ] Example 9.1 rewritten as a Laravel ticket in light mode, with a
      `BUSINESS_CONTEXT` invariant in Implementation context and a matching
      acceptance criterion
- [ ] One new invalid example: light-mode Plan with an empty
      `## Acceptance criteria`
- [ ] `contracts/formats/README.md` open questions and task column updated

---

## M1-6 — Plan parsing

**Branch:** `feat/m1-6-plan-parsing`
**Labels:** `milestone:M1`, `area:core`
**Design:** §4a
**Contract:** `contracts/formats/plan-header.md` v1.1.0
**Depends on:** M1-1, M1-5

### Description

Read `.invar/plans/<TICKET>-<slug>/spec.md`: the header between the two `---`
delimiters, the required sections for the configured mode, and the body. Write
it back.

**Do not reach for a YAML crate.** Unknown header keys and comments must be
preserved on write (D-2, §4.2); a serde round-trip drops both. The header is a
flat sequence of `Key: value` lines — a line-oriented parser that keeps every
unrecognized line verbatim is simpler and the only option that satisfies the
contract. Correct the YAML-crate comment in `core/Cargo.toml` in this task.

**Report every missing or misspelled section in one run.** Build the small
diagnostics collector here; M1-8 reuses it.

### Acceptance criteria

- [ ] Header parsed into a struct; **unknown keys and comments preserved on
      write**, in their original positions
- [ ] The first line must be exactly `---` after a BOM strip
- [ ] Only the first two `---` lines are delimiters
- [ ] Required sections checked for the configured mode, in fixed order; every
      missing or misspelled heading reported in one run
- [ ] `Status` accepts only `draft` and `approved`; the three command-written
      keys must be present as keys even when empty
- [ ] Body boundary is the first non-blank line after the closing delimiter
- [ ] **Round-trip is byte-identical**
- [ ] The `core/Cargo.toml` comment is corrected

---

## M1-7 — Plan lookup by ticket

**Branch:** `feat/m1-7-plan-lookup`
**Labels:** `milestone:M1`, `area:core`
**Design:** §4a
**Depends on:** M1-4, M1-6

### Description

Find the Plan directory for a ticket id under `.invar/plans/`, by substring
match. **Zero matches and more than one match are distinct, loud errors.** A
wrong guess here means approving one specification and implementing another.

### Acceptance criteria

- [ ] Exactly one match returns it
- [ ] Zero matches: an error naming the ticket and the directory searched
- [ ] More than one: a **different** error listing every match
- [ ] Directory name validated against `^[A-Z][A-Z0-9]*-[0-9]+-[a-z0-9-]+$`
- [ ] The header's `Ticket` must equal the directory's `<TICKET>`
- [ ] Tested with zero, one, two and malformed matches

---

## M1-8 — `invar plan validate`

**Branch:** `feat/m1-8-plan-validate`
**Labels:** `milestone:M1`, `area:core`
**Design:** §4a, §5c, §13a
**Depends on:** M1-6, M1-7

### Description

Path, directory-name shape, complete header, required sections, internal
consistency — all reported in one run.

**`validate` checks form, not approval.** A draft Plan with empty approval
fields is valid. Otherwise every fresh Plan fails and the planner learns to
ignore the command.

Called by the planner inside Phase 1 and by a `SubagentStop` hook (M1-17), so
its output is read by an agent as often as by a person: every message must say
what to change.

`--all-drafts` validates every `draft` Plan under `.invar/plans/`. The hook
needs it, because a `SubagentStop` event does not say which ticket the planner
worked on.

### Acceptance criteria

- [ ] All categories checked; every failure reported in a single run
- [ ] Fails when `## Acceptance criteria` or `## Implementation context` is
      missing or empty
- [ ] Fails when a required section is missing, misspelled or out of order
- [ ] Fails when the header's `Ticket` disagrees with the directory name
- [ ] **Does not** check the approval fields
- [ ] Accepts a ticket id, an explicit path, or `--all-drafts`
- [ ] Exit 0 / non-zero, and output an agent can act on

---

## M1-9 — `invar plan approve`

**Branch:** `feat/m1-9-plan-approve`
**Labels:** `milestone:M1`, `area:core`, `type:decision`
**Design:** §4a, §8; L-8
**Contract:** `plan-header.md` OQ-1
**Depends on:** M1-8

### Description

The human gate. Sets `Status: approved` and writes `Approved-by` and
`Approved-at` from git config, and `Approved-checksum` over the body.

Every refusal later in this milestone — `plan check`, `plan guard`, `done` — is
meaningful only because this one point records that a person read the Plan and
accepted it.

Call `git` as an external command, not through `git2`. The write must leave the
**body byte-identical**.

**Print the Acceptance criteria before approving.** They are what the code will
be judged against, by the developer and by `/invar:review`. A gate that
approves silently becomes a click.

### Resolve OQ-1

What does `plan approve` do on a Plan whose body changed after approval? The
recommendation is to **refuse without an explicit `--reapprove` flag**: a silent
re-approval is indistinguishable from the failure the checksum exists to catch.
Update `plan-header.md`, bump its version, record the reason — in this PR.

### Acceptance criteria

- [ ] Refuses when `plan validate` does not pass, naming the reason
- [ ] Prints the Acceptance criteria, then sets `Status: approved` and fills the
      three command-written fields
- [ ] `Approved-checksum` is the §4d digest of the **body**
- [ ] `Approved-at` is `YYYY-MM-DD`
- [ ] Missing `git config user.name` is a clear error, never an empty field
- [ ] The body is **byte-identical** after the write
- [ ] Re-approval behaves per the resolved OQ-1, with a test
- [ ] `plan-header.md` updated, version bumped, reason recorded

---

## M1-10 — `invar plan check` and `invar plan guard`

**Branch:** `feat/m1-10-plan-check-guard`
**Labels:** `milestone:M1`, `area:core`
**Design:** §4d, §11, §13; L-1
**Depends on:** M1-9

### Description

The read side of the gate, in two forms.

**`invar plan check <ticket>`** — run by the agent at the start of Phase 2, and
internally by `done`. Refuses when the Plan is `draft`, when its body digest
disagrees with `Approved-checksum`, or when it is `approved` without a
checksum (contract example 9.3). This is how "the coder may not work from a
draft Plan" becomes a check on disk rather than an instruction.

**`invar plan guard`** — run by a `PreToolUse` hook on `Write|Edit` (M1-17). It
reads the hook's JSON from stdin itself, extracts `tool_input.file_path`, and
denies the edit when the path is an **approved** Plan's `spec.md`. A draft Plan
may be edited freely; that is Phase 1 working.

Reading the hook payload belongs in the CLI, not in a plugin script: that keeps
the plugin a thin shell (§13b) and makes the guard testable in CI by piping
JSON into it.

**The guard is convenience, the checksum is the boundary.** The guard does not
see writes made through `Bash` (`plugin/NOTES.md` §8). `plan check` and `done`
catch those through the digest. Say so in `--help`, so nobody mistakes the guard
for the gate.

Verify how a `PreToolUse` hook denies a tool call in the current Claude Code
version — exit code 2 with stderr, or JSON with a permission decision — and
record the finding in `plugin/NOTES.md`.

### Acceptance criteria

- [ ] `plan check` refuses draft, stale and checksum-less approved Plans, each
      with a message that says what happened in plain terms ("the Plan changed
      after it was approved"), not only "checksum mismatch"
- [ ] `plan check` passes an approved, unchanged Plan and works from any
      subdirectory
- [ ] `plan guard` denies an edit to an approved Plan and allows an edit to a
      draft Plan or any other file
- [ ] `plan guard` on malformed or unexpected stdin **allows** the edit and
      reports the problem — a broken guard must not halt a session
- [ ] Both tested by running the real binary; `plan guard` with piped JSON
- [ ] CRLF/LF: an approved Plan checked out with CRLF still passes
- [ ] `plugin/NOTES.md` records the PreToolUse deny mechanism, verified

---

## M1-11 — Decision log: contract and `invar decision record`

**Branch:** `feat/m1-11-decision-record`
**Labels:** `milestone:M1`, `area:contracts`, `area:core`
**Design:** §7, §4c
**Contract:** writes `contracts/formats/decision-entry.md` (new)
**Depends on:** M1-4, M1-7

### Description

The contract first, then the command.

**The contract** fixes the filename (`<YYYY-MM-DD>-<HHMMSS>-<module>-<slug>.md`),
the front matter (`id`, `date`, `ticket`, `module`, `type`, and `scenario` as
optional — absent in light mode), the four types of §7 and the body as free
text. Insert-only: existing files are never modified or deleted by the tool.

**The command** writes one entry. `--type failed` also resets the cycle (§4c,
light-mode form): the Plan returns to `draft` and its three approval fields are
cleared; the body is untouched. There are no `.feature` files to reset.

`id` is stable and unique (e.g. `d-` plus a short random hex); it is the target
of the post-MVP decision graph.

### Acceptance criteria

- [ ] `contracts/formats/decision-entry.md` exists, versioned, with one valid
      and two invalid examples
- [ ] `invar decision record --ticket <t> --type <type>` writes a new file;
      body from `--message` or stdin
- [ ] Two entries written in the same second do not collide
- [ ] `module` taken from the Plan when one exists, otherwise required as an
      argument; `global` accepted
- [ ] `--type failed` resets the Plan to `draft` and clears its approval
      fields, body byte-identical; refuses with a clear message if the Plan is
      absent
- [ ] Never modifies an existing entry
- [ ] `invar decision log` prints entries newest-first

---

## M1-12 — `invar done`

**Branch:** `feat/m1-12-done`
**Labels:** `milestone:M1`, `area:core`, `type:decision`
**Design:** §3, §7, §12; L-4, L-6
**Depends on:** M1-10, M1-11, M1-16

### Description

Closes a cycle. In order, all-or-nothing:

1. `plan check` — refuse on draft or changed Plan
2. render the PR description (M1-16) to `.invar/out/<TICKET>-pr.md`
3. write a `confirmed` decision entry; body from `--message` or stdin, so the
   agent can record what was actually built, including deviations
4. delete the Plan directory

If any step fails, nothing after it happens and nothing before it is left
half-done: no entry without a rendered PR description, no deleted Plan without
an entry. A deleted Plan with no entry is exactly the state `clean-spec`
treats as a broken cycle (§12).

`.invar/out/` is gitignored (M1-15). The PR description is a working file for
the developer or the agent (`gh pr create --body-file …`), not a repository
artifact.

### Decision to make

Whether `done` should also refuse while uncommitted changes exist outside
`.invar/`. A clean tree makes the decision entry describe exactly what is in
the branch, but it is one more thing to trip over. Recommendation: warn, do
not refuse — the developer may legitimately close the cycle and commit
everything together.

### Acceptance criteria

- [ ] Refuses on a draft or changed Plan, writing nothing
- [ ] On success: PR description file exists, one new decision entry of type
      `confirmed`, Plan directory removed
- [ ] All-or-nothing, tested by forcing a failure at each step
- [ ] Prints the path of the PR description and the decision entry
- [ ] The resolved decision on uncommitted changes is implemented and tested

---

## M1-13 — `invar clean-spec`

**Branch:** `feat/m1-13-clean-spec`
**Labels:** `milestone:M1`, `area:core`
**Design:** §12
**Depends on:** M1-7, M1-11

### Description

For Plans left behind by abandoned cycles. All §12 guarantees hold:

- Touches only `.invar/plans/`, never `.invar/decisions/` or project code
- `--dry-run` is the default; `--force` is required to delete anything
- `[ticket]` scopes to one Plan; `--merged` limits to Plans whose ticket maps
  to a merged branch (substring match, ambiguity is a hard error)
- **Refuses to delete a Plan with no decision entry** for its ticket — that is
  a signal, not garbage. The developer records a `failed` or `note` entry
  first, which is the honest way to close an abandoned cycle.

Default branch: `git.base_branch` from `invar.yaml`, else
`git symbolic-ref refs/remotes/origin/HEAD`, else `main`.

### Acceptance criteria

- [ ] Dry run lists what would be deleted and why, deletes nothing
- [ ] `--force` deletes only eligible Plans
- [ ] A Plan without a decision entry is reported and never deleted
- [ ] `--merged` with zero matching branches skips the Plan; with more than one,
      fails listing them
- [ ] Never touches anything outside `.invar/plans/`, verified by a test that
      snapshots the rest of the tree

---

## M1-14 — `invar context show`

**Branch:** `feat/m1-14-context-show`
**Labels:** `milestone:M1`, `area:core`
**Design:** §5, §5a, §5c, §7
**Depends on:** M1-3, M1-11

### Description

Two outputs from the same sources.

**`--binding`** — `AI_CONTEXT.md` and `BUSINESS_CONTEXT.md`, each labelled with
its origin, for the `SessionStart` hook (M1-17). Both agents receive it.
Hook output above 10,000 characters is silently moved to a file Claude is not
asked to read (`plugin/NOTES.md` §5), so binding context would quietly stop
arriving. **Exceeding `context.binding_limit` is a hard error** naming both
file sizes — the team must split or trim, not discover months later that the
invariants were never in context.

**Default (planner)** — binding context plus decision entries: module entries
newest-first, then `global`, within a budget of ~50 entries or ~4000 tokens
(§7). Only the planner receives this.

A file without the `<!-- invar-context: v1 -->` marker is treated as absent and
reported as such. An empty-but-marked file behaves exactly like an absent one
(§5).

### Acceptance criteria

- [ ] `--binding` prints both files with origin labels, nothing from the
      decision log
- [ ] Above the limit: non-zero exit, message with sizes and the limit
- [ ] Default output adds decision entries in the documented order and budget
- [ ] `--module <m>` selects module entries; `global` always follows
- [ ] Unmarked files reported as unrecognized, not loaded
- [ ] Empty-but-marked and absent files produce identical output

---

## M1-15 — `invar init`

**Branch:** `feat/m1-15-init`
**Labels:** `milestone:M1`, `area:core`
**Design:** §4d, §5, §5a, §13a; L-2
**Depends on:** M1-3

### Description

Scaffolds a project for light mode:

- `invar.yaml` with `scenarios: false` written explicitly
- `.invar/plans/`, `.invar/decisions/`
- `AI_CONTEXT.md` and `BUSINESS_CONTEXT.md` with the marker and section
  headings only — **never content** (§5, §5a). `AI_CONTEXT.md` headings are
  language-neutral; whether a team splits them into backend and frontend parts
  is the team's decision, not the tool's
- `.gitattributes`: `.invar/plans/**/*.md text eol=lf`
- `.gitignore`: `.invar/out/`
- `.claude/settings.json`: a permission deny rule for reading
  `.invar/decisions/**` (§13a). The planner receives the log through
  `invar context show`, not by reading files

**Merging, never clobbering.** `.gitattributes`, `.gitignore` and
`.claude/settings.json` usually exist already in a real Laravel project. `init`
adds its entries and leaves everything else untouched. Losing a team's
`.claude/settings.json` to `init` would end the pilot on day one.

**Idempotent.** A second run changes nothing and says so per file.

Pulling this task forward is encouraged: it makes hand-testing every other task
easier.

### Acceptance criteria

- [ ] Creates every item above on an empty git repository
- [ ] Context files contain the marker and headings, and no content
- [ ] Existing `.gitattributes`, `.gitignore` and `.claude/settings.json` are
      merged into, never overwritten; tested on populated fixtures, including
      a `settings.json` with existing permissions
- [ ] Idempotent: a second run reports "unchanged" per file
- [ ] Refuses outside a git repository with a clear message

---

## M1-16 — PR description from the Plan

**Branch:** `feat/m1-16-pr-body`
**Labels:** `milestone:M1`, `area:core`
**Design:** §12; L-6
**Depends on:** M1-6, M1-7

### Description

`invar pr-body <ticket>` renders the Plan as a PR description: Intent, Scope,
Open questions / answers, Rejected alternatives, Acceptance criteria and
Implementation context, under a fixed heading order, with the ticket id and
the approval line (who, when). The pasted `## Ticket` text is omitted by
default — the tracker already holds it — and included with `--with-ticket`.

Deterministic: the same Plan always renders the same description. This is what
the reviewer and CodeRabbit read, and the measurement the OKR depends on, so it
must not vary between runs.

`done` (M1-12) calls this; it is also usable on its own while the Plan exists.

### Acceptance criteria

- [ ] Renders to stdout, or to a file with `--out`
- [ ] Fixed section order; empty optional sections rendered as "none"
- [ ] Byte-identical output for the same Plan across runs
- [ ] Refuses on a draft Plan — a PR description must describe an approved
      specification
- [ ] Snapshot test against the contract's example Plan

---

## M1-17 — Plugin: manifest, binary and hooks

**Branch:** `feat/m1-17-plugin-hooks`
**Labels:** `milestone:M1`, `area:plugin`, `type:decision`
**Design:** §13, §13b; `plugin/NOTES.md`
**Depends on:** M1-8, M1-10, M1-14

### Description

The real plugin replaces `plugin/spike-m0-6/`. Three hooks, each a single CLI
call in exec form:

| Event | Command | Effect |
|---|---|---|
| `SessionStart` | `invar context show --binding` | Binding context in every session |
| `SubagentStop` (planner) | `invar plan validate --all-drafts` | Planner sees what to fix |
| `PreToolUse` on `Write\|Edit` | `invar plan guard` | Approved Plans cannot be edited through the edit tools |

No lint hook and no test hook (L-3). No hook ever runs `plan approve` or
`done`.

### Decision to make — shipping the binary

`plugin/NOTES.md` §6 leaves it open: `bin/invar` (agent calls `invar` directly,
claude.ai distribution excluded), the binary elsewhere with long
`${CLAUDE_PLUGIN_ROOT}` paths, or installation onto `PATH`. For a pilot inside
one company, distributed through a git marketplace, `bin/` is the simplest and
claude.ai distribution is irrelevant. Decide, record the reason in `NOTES.md`.

### Acceptance criteria

- [ ] `claude plugin validate --strict` passes; runs in CI
- [ ] The three hooks are registered and visible in `/hooks`
- [ ] Each hook is one CLI invocation; no logic in plugin scripts (§13b)
- [ ] A crashing or missing `invar` binary does not break a session
- [ ] Binary shipping decided and recorded; Linux and macOS binaries built by
      CI
- [ ] `plugin/spike-m0-6/` deleted

---

## M1-18 — Planner sub-agent and protocol skill

**Branch:** `feat/m1-18-planner-protocol`
**Labels:** `milestone:M1`, `area:plugin`
**Design:** §3, §5c, §13a; L-1, L-9
**Depends on:** M1-17

### Description

The largest task by iteration time, and the one that decides whether the pilot
works. The code is small; getting the agent to behave is not.

**The planner** (`agents/planner.md`) runs Phase 1 in its own context: reads the
ticket, `invar context show`, and targeted parts of the code base (the modules
and interfaces the ticket touches, never a full scan). Asks clarifying
questions — **mandatory** when the ticket touches a Regional/parameterized
fact (§5). Writes the Plan as `draft`, runs `invar plan validate` and fixes
what it finds. Returns only the Plan path and a short summary.

Two things it must do well, and that are worth testing against real tickets:

- **Implementation context as a distillate.** Only the invariants and
  conventions this ticket touches — not a copy of the context files.
- **Acceptance criteria that would catch the incident.** Every invariant named
  in Implementation context gets a criterion that fails if it is broken.

**The protocol skill** (`skills/invar/SKILL.md`) states the cycle for the main
session: when to use it and when to skip it (L-9), that Phase 2 begins with
`invar plan check`, that an approved Plan is never edited, that an impossible
Plan goes back to Phase 1 with a `failed` entry rather than being "fixed", and
that the cycle closes with `invar done`.

### Acceptance criteria

- [ ] Planner produces a Plan that passes `plan validate` on at least three
      real tickets from the pilot project, one of them touching a business
      invariant
- [ ] Planner asks a clarifying question when a Regional/parameterized fact is
      involved
- [ ] The main session never receives the decision log
- [ ] The protocol skill tells the agent to skip the cycle for trivial work and
      to say that it is skipping it
- [ ] Phase 2 refuses to start on a draft Plan in a manual walkthrough
- [ ] Prompt iterations and what changed between them recorded in
      `plugin/NOTES.md`

---

## M1-19 — `/invar:approve`

**Branch:** `feat/m1-19-approve-command`
**Labels:** `milestone:M1`, `area:plugin`
**Design:** §3, §13; L-8
**Depends on:** M1-9, M1-17

### Description

The developer's one manual step, without leaving Claude Code. The command runs
`invar plan approve <ticket>` and shows its output — the Acceptance criteria
first, then the result.

**The model must not be able to invoke it.** A command the agent can call on
its own has abolished the gate (§13). Use the frontmatter setting that excludes
a command or skill from model invocation, verify that it holds in the current
Claude Code version, and record the verification in `plugin/NOTES.md`.

With one Plan in `.invar/plans/` the ticket argument is optional; with several,
it is required and the command lists them.

### Acceptance criteria

- [ ] `/invar:approve [ticket]` approves through the CLI and shows the
      Acceptance criteria
- [ ] Model invocation disabled, and verified: asking the agent to approve the
      Plan does not run the command
- [ ] Missing ticket with several Plans lists them instead of guessing
- [ ] `invar plan approve` from a terminal still works identically

---

## M1-20 — `/invar:review` (optional)

**Branch:** `feat/m1-20-review`
**Labels:** `milestone:M1`, `area:core`, `area:plugin`
**Design:** §11, §13a; L-7
**Depends on:** M1-10, M1-14, M1-18

### Description

A check before the PR: does the branch do what the approved Plan says, and does
it break a business invariant? **Advisory — it never blocks `done` and never
fails a build** (L-7).

**`invar review-input <ticket>`** (core) assembles, deterministically: the
approved Plan's Intent, Scope, Acceptance criteria and Implementation context;
binding context; and the diff of the branch against `git.base_branch`. For a
diff above a size budget, it emits the changed-file list with per-file stats
and lets the reviewer read files itself. Refuses on a draft or changed Plan —
reviewing against an unapproved specification proves nothing.

**The reviewer** (`agents/reviewer.md`) runs in its own context. It sees only
`review-input` output and the repository. It does **not** see the coder's
transcript — it would inherit the coder's justifications — and does not see the
decision log. Independence is the point.

**Output, fixed shape:**

1. Each acceptance criterion: `met` / `not met` / `unclear`, with `file:line`
   evidence. `unclear` is a legitimate answer and must stay one; a reviewer
   forced to choose invents confidence.
2. Invariants from Implementation context that the diff appears to break.
3. Changes outside the declared Scope.
4. Nothing else. General code quality is CodeRabbit's job; duplicating it adds
   noise to both.

`review.offer: true` in `invar.yaml` makes the protocol skill offer the review
at the end of Phase 2. The developer runs it with `/invar:review`.

### Acceptance criteria

- [ ] `review-input` output is byte-identical for the same Plan and diff
- [ ] `review-input` refuses on a draft or changed Plan
- [ ] Reviewer output follows the fixed shape, with evidence for every
      `met` / `not met`
- [ ] Tested on a seeded branch that breaks one invariant and misses one
      criterion: both are reported
- [ ] Tested on a branch that touches a file outside Scope: reported
- [ ] Never invoked by a hook; `done` does not depend on it

---

## M1-21 — Gate-refusal suite and Laravel e2e

**Branch:** `test/m1-21-gate-refusals`
**Labels:** `milestone:M1`, `area:core`, `area:ci`
**Design:** §10, §11, §13b
**Depends on:** M1-12, M1-13, M1-15

### Description

Every refusal this milestone promises, as a test running the **real binary**
against real temp trees — no agent, no plugin. This is the evidence that every
boundary that matters lives on disk (§11), and the §13b proof that nothing
leaked into the plugin.

Plus one end-to-end run in `e2e/` on a **minimal Laravel project** (a fixture
tree, not a full application — no PHP needs to run): `init` → hand-written
draft Plan → `validate` → `approve` → code change → `check` → `done` → Plan
gone, entry present, PR description rendered.

Each test reads as a sentence about the system: *an edited approved Plan
cannot be closed*.

### Acceptance criteria

One test for each:

- [ ] `plan approve` refuses an invalid Plan
- [ ] `plan validate` fails without `## Acceptance criteria`
- [ ] Re-approving a changed Plan behaves per OQ-1
- [ ] `plan check` refuses a draft Plan
- [ ] `plan check` refuses a Plan edited after approval, including through a
      plain file write (the Bash path the guard cannot see)
- [ ] `plan guard` denies an edit to an approved Plan
- [ ] `done` refuses a changed Plan and writes nothing
- [ ] `--type failed` returns the Plan to `draft`
- [ ] `clean-spec` refuses a Plan with no decision entry
- [ ] Ambiguous Plan lookup fails, listing the matches
- [ ] `context show --binding` fails above the limit

Plus:

- [ ] CRLF/LF equivalence end-to-end: approve on LF, check out as CRLF, `check`
      passes
- [ ] The Laravel e2e runs in CI
- [ ] `e2e/README.md` updated: sample project is Laravel for M1

---

## Suggested order

ID order is build order. Where work can proceed in parallel:

```
M1-1 → M1-2
M1-3 → M1-4
M1-3 → M1-5 → M1-6 → M1-7 → M1-8 → M1-9 → M1-10
M1-15  any time after M1-3 — pull it forward
M1-7 → M1-11 → M1-13
M1-6 + M1-7 → M1-16
M1-10 + M1-11 + M1-16 → M1-12
M1-11 → M1-14
M1-8 + M1-10 + M1-14 → M1-17 → M1-18 → M1-19
M1-18 → M1-20
M1-12 + M1-13 + M1-15 → M1-21
```

**Pilot checkpoint after M1-10, M1-15, M1-17, M1-18 and M1-19.** That set is
already usable: context in every session, a planner, a Plan the developer
approves, and an approved Plan the coder cannot quietly move. Start the pilot
there and measure; `done`, `clean-spec`, the PR description and review follow
with real feedback.

## Decisions M1 must close

| Decision | Where | By |
|---|---|---|
| Which YAML parser for `invar.yaml`, or none | `invar-yaml.md` | M1-3 |
| Re-approving a Plan whose body changed | `plan-header.md` OQ-1 | M1-9 |
| How `PreToolUse` denies a tool call | `plugin/NOTES.md` | M1-10 |
| `done` with uncommitted changes: warn or refuse | `--help` + this file | M1-12 |
| Shipping the binary in the plugin | `plugin/NOTES.md` §6 | M1-17 |
| Disabling model invocation for `/invar:approve` | `plugin/NOTES.md` | M1-19 |

Contract decisions close by editing the contract, bumping its version and
recording the reason (§14.3) — in the same PR. `DESIGN.md` v4 records the
light-mode decisions (L-1 to L-9) once M1 is done.

## Deliberately out of scope

Named so nobody adds them mid-milestone:

- **`.feature` files** and everything built on them: `scenario validate`,
  `scenario confirm`, `--plan` bulk confirm, `Confirmed-checksum`. The
  contract `feature-metadata.md` stays as written, for the milestone that
  brings feature tests.
- **Language adapters**, conformance suite, dependency and test discovery, test
  execution, lint (L-3).
- **`invar generate` / `verify`** (L-4).
- **`Lockfile-checksum`** (L-5).
- **`context sync`, framework and package context, the registry and MCP.**
- **Ticket templates and the content of `AI_CONTEXT.md` /
  `BUSINESS_CONTEXT.md`** for any specific project. Those belong to the team
  using the tool, never to the tool (§5).

## Mapping from the previous M1

For updating GitHub issues. "Deferred" issues move to a later milestone label,
not closed.

| Previous | Now | Change |
|---|---|---|
| M1-1 Canonicalization | M1-1 | Unchanged |
| M1-2 Digest and `invar hash` | M1-2 | Unchanged |
| M1-3 `invar.yaml` | M1-3 | Light-mode key set; `lockfile:` removed |
| M1-4 Project root | M1-4 | Unchanged |
| M1-5 `.feature` parsing | — | **Deferred** (feature tests milestone) |
| M1-6 Plan parsing | M1-6 | Against contract v1.1.0, mode-dependent sections |
| M1-7 Plan lookup | M1-7 | Unchanged |
| M1-8 `scenario validate` | — | **Deferred** |
| M1-9 `plan validate` | M1-8 | Acceptance criteria required; `--all-drafts` |
| M1-10 Lockfile digest | — | **Deferred** to M6 (L-5) |
| M1-11 `plan approve` | M1-9 | No lockfile; prints Acceptance criteria |
| M1-12 `scenario confirm` | — | **Deferred** |
| M1-13 `scenario confirm --plan` | — | **Deferred** |
| M1-14 Integrity check | M1-10 | Plan side only; adds `plan check` and `plan guard` |
| M1-15 `init` | M1-15 | Adds `.gitignore`, `.claude/settings.json` deny rule; no `specs/` |
| M1-16 Gate-refusal suite | M1-21 | Light-mode refusals; Laravel e2e |
| — | M1-5 | **New:** Plan contract v1.1.0 |
| — | M1-11 | **New** (from M3): decision log |
| — | M1-12 | **New:** `invar done` |
| — | M1-13 | **New** (from M3): `clean-spec` |
| — | M1-14 | **New** (from M3): `context show` |
| — | M1-16 | **New:** PR description |
| — | M1-17 | **New** (from M5): plugin and hooks |
| — | M1-18 | **New** (from M5): planner and protocol skill |
| — | M1-19 | **New:** `/invar:approve` |
| — | M1-20 | **New:** `/invar:review` |
