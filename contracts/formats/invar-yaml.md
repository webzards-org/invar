# Contract: `invar.yaml`

**Version:** 2.2.1 — `acceptance.run` is now executed; see §11
**Status:** stable
**Implements:** §6 (Config file), §13c (the review's base branch), §14.3; light
mode L-2 and L-7 (`docs/TASKS-M1.md`); A-2, A-6 and A-7 (`docs/TASKS-AT.md`)
**Read by:** the core (`format/config`; `invar done` for `done.*`; `plan approve`
for `acceptance.required`; `invar acceptance lock` for `acceptance.dir` and `run`), the protocol skill (`review.offer`, M1-18), any later
reimplementation

> Changing this file is a **public-contract change** (§14.3). It is made
> deliberately, in its own PR, with the reason recorded, the version bumped,
> and every reader updated in the same PR. New optional keys may be added at
> any time under one rule: **an absent key means the previous behavior.**

---

## 1. Scope

One file, `invar.yaml`, at the project root. Its presence is what makes a
directory the project root: commands find the root by walking upward from the
current directory until they find this file (M1-4).

`invar init` creates it (M1-15); after that it belongs to the team. **The core
reads it and never writes it back.** That is what allows this file, unlike the
Plan header, to be read with a YAML library (D-1): nothing has to survive a
round trip.

§6 of the design document describes the full key set the finished system will
read. Version 1.0.0 of this contract covers **light mode** (M1), which reads
four keys (unchanged in 2.0.0); 2.1.0 adds the two keys of the `done` section,
and 2.2.0 the `acceptance` section.
The rest of §6 is listed in §5 below so that a config written for a
later milestone is read by this one without failing and without noise.

## 2. Encoding and document shape

- UTF-8. A leading BOM is permitted and ignored. Invalid UTF-8 is an error.
- `\n`, `\r\n` and `\r` line endings are all accepted.
- **Exactly one YAML 1.2 document.** A second document (`---` followed by more
  content) is an error.
- The document is a **mapping**, or empty. An empty file, or one containing
  only comments, is valid and means every key takes its default (§3).
- Comments (`# …`) are permitted anywhere YAML permits them.
- A key appearing twice in the same mapping is an error.

## 3. Keys read in light mode

| Key | Type | Default | Meaning in M1 |
|---|---|---|---|
| `scenarios` | bool | `false` | Plan shape (L-2). `false` is light mode. `true` is reserved for the milestone that introduces `.feature` files; see §3.1 |
| `review.offer` | bool | `true` (since 2.0.0; `false` before) | Whether the protocol skill offers `/invar:review` before `invar done` (L-7, D-8) |
| `git.base_branch` | string | detected, see §3.3 | Base branch for the review diff |
| `context.binding_limit` | integer | `9000` | Character ceiling for `invar context show --binding` (M1-14) |
| `done.commit` | bool | `false` | Whether `invar done` commits the deletion of the Plan without being given `--commit`; see §3.5 |
| `done.commit_message` | string | `Remove plan {ticket}` | The message of that commit; `{ticket}` is replaced by the Plan's ticket id |
| `acceptance.required` | bool | `true` when the section is present | The default of `plan approve` for whether a ticket needs acceptance tests; see §3.6 |
| `acceptance.style` | string | `integration` | The kind of acceptance tests; `gherkin` is reserved |
| `acceptance.dir` | string | none: **required** in the section | Where the acceptance tests live, relative to the project root |
| `acceptance.run` | list of strings | none: **required** in the section | The command that runs them, with no shell |
| `acceptance.fakes_allowed` | list of strings | empty | What the tests may fake; not interpreted by the core |

A dotted key is a key inside a section: `review.offer` is written as

```yaml
review:
  offer: true
```

The sections `review`, `git`, `context`, `done` and `acceptance` must be mappings
when present. An empty section (`review:` with nothing under it) is the same as an
absent one.

### 3.1 `scenarios`

`false` — light mode. The Plan carries `## Acceptance criteria`, and no
`.feature` files exist (L-1, L-2).

`true` — the `.feature` workflow. **The reader accepts it; every command
refuses to run** with a message saying that `.feature` files arrive in a later
milestone. The value is syntactically valid and will be meaningful once
scenarios exist, so rejecting it while reading would misreport a capability
gap as a broken file (D-4).

Absent means `false`. That is the behavior of every config written before
scenarios existed — the compatibility rule of §14.3 applied forward.

### 3.2 `review.offer`

Read by the protocol skill, not by a CLI command. `true` — the default — makes
the skill offer `/invar:review` once, after the coder answers `DONE` and before
`invar done`; `false` makes it stay silent about the review. The review itself
is advisory and never blocks `invar done` (L-7), so this key changes what the
developer is offered, never what is enforced. Why the default is `true` is D-8.

### 3.3 `git.base_branch`

A branch name as `git` resolves it, such as `main`, `develop` or
`origin/main`. It is used for the diff `/invar:review` reads (M1-20).

When the key is absent, the base branch is detected, in this order:

1. `git symbolic-ref refs/remotes/origin/HEAD`
2. `main`

The key exists for repositories where
that detection is wrong — a team that merges into `develop`, or a clone without
an `origin/HEAD` ref.

Valid values: a non-empty string containing no whitespace. The reader does not
check that the branch exists; the commands that use it do, at the point they
call `git`, and fail loudly if it does not.

### 3.4 `context.binding_limit`

The largest number of characters `invar context show --binding` may print.
Above it the command fails, naming both context files and their sizes (M1-14).

Valid values: an integer from `1` to `10000` inclusive.

The upper bound is not arbitrary. Claude Code moves hook output above 10,000
characters into a file and gives the agent its path, without asking the agent
to read it (`plugin/NOTES.md` §5). Binding context is delivered by a
`SessionStart` hook, so a limit above 10,000 would let the invariants silently
stop reaching the agent — the exact failure the limit exists to prevent. The
default of 9,000 leaves room for the origin labels the command adds (D-5).

### 3.5 `done.commit` and `done.commit_message`

Read by `invar done`. The Plan is deleted at the very end of a cycle, when the
code is already committed and reviewed, so the deletion is a commit of its own
that nobody has to think about. `done.commit: true` makes `invar done` commit
it, alone, as `--commit <MESSAGE>` would — under `done.commit_message`, with
`{ticket}` replaced by the Plan's ticket id (`Remove plan SHOP-214`).

What the command line says wins:

1. `--commit <MESSAGE>` commits under that message, whatever the config says;
2. `--no-commit` only stages the deletion, whatever the config says;
3. otherwise `done.commit` decides. Absent, it is `false`: the deletion is
   staged and the commit is the developer's — the behavior before the key
   existed.

The commit is the same one `--commit` makes: only the Plan directory goes into
it, anything else that is staged stays staged, the project's hooks, signing and
identity apply and are never bypassed, and nothing is pushed.

`done.commit_message` valid values: one line, non-empty once surrounding
whitespace is removed (it is removed). `{ticket}` may appear any number of
times, or not at all. A message that starts with a YAML indicator or contains
`: ` must be quoted: `commit_message: "chore: remove the Plan of {ticket}"`.
The key applies only when a commit is made; set alone, it changes nothing.

### 3.6 The `acceptance` section

```yaml
acceptance:
  required: true                  # default when the section is present
  style: integration              # integration | gherkin (reserved)
  dir: tests/Acceptance           # required
  run: ["vendor/bin/pest", "--log-junit", "{report}", "{files}"]   # required
  fakes_allowed: [http, mail, queue, notification]                 # default []
```

**Absent means the previous behavior:** the project has no acceptance tests, and
`plan approve` defaults to `skipped`. A section in which every key is empty is the
same as an absent one (D-3).

**`required`** is the default of `plan approve` for this project, and `true` when
the section is present (D-10). It is only a default: the developer still decides
per ticket at approval, and a flag or a recorded value comes first (`plan-header.md`
§5.3).

**`style`** is `integration`. `gherkin` is reserved: an error that names it and says
it arrives later. Any other value is an error listing the allowed ones.

**`dir`** is a path relative to the project root: not empty, not absolute, with no
`..` segment and no trailing separator. A backslash counts as a separator, and a
leading drive letter (`C:`) as absolute, so the rule does not depend on the platform
reading the file. Whether the directory exists is not checked here.

**`run`** is a list of strings with at least one element. The core runs it without a
shell (D-11): element 0 is the program, and the rest are its arguments, with no
splitting and no quoting rules. Element 0 is not empty and is not a placeholder. A
string is an error with its own message saying to write a list; it is never split.

| Placeholder | Must appear | Becomes |
|---|---|---|
| `{report}` | Exactly once, as a whole element | One path the core chooses |
| `{files}` | Exactly once, as a whole element | Zero or more arguments, one per test file |

`{files}` is required because without it, locking the tests of one ticket would run
the whole suite. A **placeholder** is `{`, then one or more letters, digits, `_` or
`-`, then `}`, anywhere in an element. Any other placeholder is an error, so a typo
such as `{reports}` is caught. Any other brace is a literal one: `{}`, `{a,b}` and
`{"k": 1}` are passed on as they are.

Reading the command runs nothing. `invar acceptance lock` is what executes it
(`acceptance-lock.md` §9.2).

**`fakes_allowed`** is a list of non-empty strings, empty by default. The core does
not interpret it and passes it on to the test-writer, so unknown values are accepted
as they are.

`invar init` writes this section commented out (D-12).

## 4. Value rules

These apply to every key in §3.

- **Booleans are `true` or `false`.** YAML 1.2 also accepts `True`, `TRUE`,
  `False` and `FALSE`. `yes`, `no`, `on`, `off`, `y`, `n`, `1` and `0` are
  **errors**, never booleans (D-6).
- **Integers** follow YAML 1.2. Write them in decimal (`9000`).
- **Strings that YAML would read as something else must be quoted.** A branch
  named `1.10` is written `base_branch: "1.10"`; unquoted, YAML reads it as the
  number `1.1`. A plain value that resolves to a number, a boolean or null
  where a string is expected is an error (D-7).
- **A key with an empty value is the same as an absent key.** `scenarios:` with
  nothing after it, and `scenarios: ~`, both mean `false` (D-3).
- **Quoted booleans and integers are not part of this contract.** Write
  `true`, not `"true"`; `9000`, not `"9000"`. A reader may accept the quoted
  form or reject it. The core currently accepts it, because its YAML library
  does; another implementation is free not to, and a config relying on it is
  not portable.

## 5. §6 keys this version ignores

These keys are described in §6 of the design document and belong to later
milestones. Version 1.0.0 **ignores them silently**: no warning, and no
validation of their values, which may be any YAML at all. A config written for
a later milestone therefore reads cleanly here (D-2).

| Key | Arrives with |
|---|---|
| `language` | Language adapters (§14.2) |
| `framework` | Framework context (§5b) |
| `infrastructure` (and everything under it) | Framework context (§5b) |
| `pipeline` (and everything under it) | `.feature` files and `invar generate` (§3, §4b) |
| `context.sync_on_plan` | `invar context sync` (§5b) |
| `context.business_context_ref` | Business context by reference (§5, post-MVP) |
| `lint` (and everything under it) | Lint (§6, L-3) |
| `exclude` | Lint and scenario discovery (§6) |

Note that `context` is a section M1 reads: `context.binding_limit` is read,
and the two keys above are ignored beside it.

## 6. Unknown keys

A key that is neither read (§3) nor ignored (§5) is **unknown**. An unknown key
produces a **warning and never a failure**: a config written by a newer
version of `invar` must not break an older one.

- The warning names the key by its full dotted path (`review.ofer`,
  `senarios`), so a typo is easy to find.
- Unknown keys are reported at the top level and inside the five sections
  this version reads. Keys inside an ignored subtree (`lint.rules.…`) are never reported;
  that subtree belongs to a later milestone as a whole.
- The value of an unknown key is not validated. It may be any YAML.

The warning is what makes a typo visible. Without it, `senarios: true` would be
silently ignored and the team would believe it had switched modes.

## 7. Errors

Each of these is a distinct failure, so a caller can tell them apart:

| Failure | When | The message says |
|---|---|---|
| **Missing** | No `invar.yaml` in the project root | The path it looked for, and that `invar init` creates it |
| **Unreadable** | The file exists but cannot be read (permissions, a directory of that name) | The path and the operating system's reason |
| **Invalid** | Not UTF-8, not valid YAML, not a mapping, a duplicated key, a value outside §3 and §4, an `acceptance` section that breaks §3.6 | The path, the line and column where YAML reports one, and what is wrong |
| **Scenarios not supported** | `scenarios: true`, when a command runs | That `.feature` files arrive in a later milestone, and to set `scenarios: false` |

An invalid file reports every problem with a known key's value in one message
where it can. A YAML syntax error stops reading at the first one, because what
follows it cannot be trusted.

## 8. Decisions and their reasons

**D-1 — A real YAML parser, not a hand-written flat reader.** M1 reads only
four keys, which makes a line-oriented reader look sufficient. It is not,
because M1 must also *skip* everything else a valid config may contain: the
nested `lint.rules` maps of §6, lists, flow mappings (`{enabled: true}`),
anchors, and block scalars. A block scalar is the dangerous case:

```yaml
notes: |
  scenarios: true
```

A flat reader sees `scenarios: true` and switches modes; a YAML parser sees a
string value of `notes`. Skipping arbitrary YAML correctly *is* parsing YAML.
The Plan header is the opposite case (M1-6): written by the tool, flat by
specification, and required to round-trip byte for byte — which a YAML library
cannot do, and a line-oriented parser can.

*Implementation note, not part of the contract:* the core uses `serde-saphyr`
(pure Rust, no `unsafe`, YAML 1.2), with strict booleans and duplicate keys as
errors. `serde_yaml` is deprecated, and its best-known fork `serde_yml` is
flagged unsound and unmaintained (RUSTSEC-2025-0068). Any reader satisfying
§2–§7 is conforming.

**D-2 — Keys from later milestones are ignored silently; unknown keys warn.**
Two different situations that look alike. A §6 key such as `lint` is known to
be meaningful later, and warning about it on every command — including the
`SessionStart` hook, which runs in every session — would teach the team to
ignore warnings. A key nobody has defined is most likely a typo, and the
warning is the only thing that makes it visible. Neither fails.

**D-3 — An empty value means absent.** In YAML, `key:` with nothing after it is
null. Treating null as its own state would add a third meaning to every key —
set, unset, and explicitly empty — that no key here needs. For the Plan header
the opposite rule applies, because there the empty value is meaningful
(`plan-header.md` D-3); here it is not.

**D-4 — `scenarios: true` is read, then refused by commands.** Rejecting it in
the reader would report a valid file as broken, and would also block anything
that only reads the config without acting on the mode. The refusal belongs
where the capability is missing: in the commands, with a message that names
the milestone gap rather than a syntax problem.

**D-5 — `context.binding_limit` is capped at 10,000.** The cap is Claude Code's
hook output limit (§3.4), not a preference. If Claude Code changes that limit,
this bound changes with it — in a versioned change to this contract.

**D-6 — Strict booleans.** YAML 1.1 reads `no`, `off` and `n` as `false`; YAML
1.2 reads them as strings. The same file would mean different things to
different readers, and to a team member who knows only one of the two
specifications. Rejecting the ambiguous forms is the only rule every reader
can implement identically.

**D-7 — Strings that resolve to another type must be quoted.** Some YAML
libraries let the expected Rust type decide, so an unquoted `1.10` stays
`"1.10"`; others, including Python's, produce the number `1.1`. Requiring
quotes is what makes the value mean the same thing to every reader.

**D-8 — `review.offer` defaults to `true`** (2.0.0). Since DESIGN v3.1 the coder
reads `AI_CONTEXT.md` and nothing of `BUSINESS_CONTEXT.md`: business rules reach
it only as the approved Plan states them (§5c). An invariant the planner did
not carry into the Plan is then invisible to the coder, and the review — which
checks the branch against the business rules — is the one place that can catch
it before the PR (§13c). A safety net that has to be switched on is one most
teams never switch on. The review stays advisory: the default changes what is
*offered*, never what *blocks* (L-7), and a team that does not want the offer
writes `offer: false`.

This is the one place the contract's own compatibility rule — *an absent key
means the previous behavior* — is broken on purpose, which is why the version is
a major one (§11).

**D-9 — `done.commit` is a project setting, and defaults to `false`** (2.1.0).
By the time `invar done` runs, the code is committed and reviewed, and the
Plan's deletion should be the last commit of the PR, alone. Typing
`--commit "…"` at the end of every cycle is a step with no decision in it, so a
project can make it once, here. The default stays `false` because invar makes
no commit unless asked (§12 of the design document), and an absent key must
mean the previous behavior. The message is configurable because many projects
run a commit-message hook (Conventional Commits and the like) that would refuse
the default; `invar done` never bypasses it, so the project chooses a message
its own hook accepts.

**D-10 — `acceptance.required` defaults to `true` when the section is present**
(2.2.0). A team that writes down a directory and a test command has asked for
acceptance tests, so it gets them by default, and a ticket that needs none is a
decision someone makes at approval with `--no-acceptance`. Skipping is the weaker
guarantee, so skipping is what has to be asked for (A-6). This does not break the
rule that an absent key means the previous behavior: an absent *section* still does,
and a config with the section did not exist before this version.

**D-11 — `acceptance.run` is a list, and runs without a shell** (2.2.0). A string form
would make the core define shell splitting and quoting, which is a language of its
own that every reimplementation would have to match. Handing the string to a shell
would make the command depend on the platform, and on which shell it finds. A list
needs neither: each element is one argument, exactly as written. A project that does
need a shell says so: `["sh", "-c", …]`, or a script of its own.

Each placeholder is a whole element for the same reason. `{files}` becomes any number
of arguments, which has no meaning inside a longer one, and keeping `{report}` to the
same rule means the core never rewrites text inside an argument.

**D-12 — `invar init` writes the `acceptance` section commented out** (2.2.0). `run`
and `dir` depend on the project, and the core must not guess them (A-2). An active
section with a wrong command would stop the first cycle at the test-writer, on a
project that has not chosen acceptance tests at all. Commented out, the section is one
edit away, and a project that does nothing keeps the behavior of an absent section.

## 9. Open questions

**OQ-1 — JSON Schema.** `contracts/README.md` lists a JSON Schema for
`invar.yaml` under `schemas/`. It would give editors completion and validation
for free. Not needed by any M1 reader; write it when the key set grows beyond
four, or when an editor integration asks for it.

## 10. Examples

The examples are executable: the core's tests read each block below by its
marker comment and check the documented result.

### 10.1 Valid — a light-mode project

<!-- example: valid-light-mode -->
```yaml
# invar.yaml — written by `invar init`, then adjusted by the team

scenarios: false

review:
  offer: true

git:
  base_branch: develop

context:
  binding_limit: 8000

done:
  commit: true
  commit_message: "chore: remove the Plan of {ticket}"
```

Reads as: light mode, the review is offered, `develop` is the base branch,
binding context may be at most 8,000 characters, and `invar done` commits the
Plan's deletion as `chore: remove the Plan of SHOP-214`. There are no acceptance
tests. No warnings.

### 10.2 Valid — empty

<!-- example: valid-empty -->
```yaml
# Nothing set yet: every key takes its default.
```

Reads as: `scenarios: false`, `review.offer: true` (D-8), base branch
detected, `binding_limit: 9000`, `done.commit: false`, no `acceptance` section.

### 10.3 Valid — written for a later milestone

<!-- example: valid-later-milestone -->
```yaml
language: python
framework: fastapi

infrastructure:
  database: postgresql
  cache: redis

pipeline:
  require_confirmation: true
  test_framework: pytest-bdd
  tests_dir: tests

scenarios: false

context:
  sync_on_plan: true
  binding_limit: 9000

lint:
  enabled: true
  rules:
    no-bare-except: {enabled: true, severity: error}

exclude:
  - "specs/**/drafts/**"
```

Reads as the defaults plus `binding_limit: 9000`. Every other key is a §5 key:
ignored, and **no warnings**.

### 10.4 Valid, with a warning — a typo

<!-- example: valid-typo-warns -->
```yaml
senarios: true
review:
  ofer: true
```

Reads as the defaults — light mode, the review offered — with **two warnings**,
for `senarios` and `review.ofer`. This is why unknown keys warn: the file reads
without error, and without the warnings the team would believe it had switched
modes.

### 10.5 Invalid — a YAML 1.1 boolean

<!-- example: invalid-yaml11-boolean -->
```yaml
scenarios: no
```

**Why:** `no` is `false` under YAML 1.1 and the string `"no"` under YAML 1.2
(D-6). Reported as an invalid boolean at line 1, column 12, with the hint that
only `true` and `false` are accepted.

### 10.6 Invalid — a binding limit above the hook cap

<!-- example: invalid-binding-limit -->
```yaml
context:
  binding_limit: 12000
```

**Why:** above 10,000 characters, Claude Code moves `SessionStart` output out
of the agent's context (§3.4, D-5). A team that raised the limit to fit a
growing `BUSINESS_CONTEXT.md` would stop delivering its invariants and not
notice. The fix is to trim or split the context files, not to raise the limit.

### 10.7 Valid — a full `acceptance` section

<!-- example: valid-acceptance-full -->
```yaml
acceptance:
  required: true
  style: integration
  dir: tests/Acceptance
  run: ["vendor/bin/pest", "--log-junit", "{report}", "{files}"]
  fakes_allowed: [http, mail, queue, notification]
```

Reads as written. No warnings.

### 10.8 Valid — a minimal `acceptance` section

<!-- example: valid-acceptance-minimal -->
```yaml
acceptance:
  dir: tests/Acceptance
  run: ["vendor/bin/pest", "--log-junit", "{report}", "{files}"]
```

Reads as `required: true` (D-10), `style: integration` and no `fakes_allowed`.

### 10.9 Invalid — `acceptance` without `run`

<!-- example: invalid-acceptance-no-run -->
```yaml
acceptance:
  dir: tests/Acceptance
```

**Why:** a section with no command cannot run the tests it asks for.

### 10.10 Invalid — `run` without `{report}`

<!-- example: invalid-acceptance-no-report -->
```yaml
acceptance:
  dir: tests/Acceptance
  run: ["vendor/bin/pest", "{files}"]
```

**Why:** the report is where the core reads the result, so the command has to be told
where to write it.

### 10.11 Invalid — `style: gherkin`

<!-- example: invalid-acceptance-gherkin -->
```yaml
acceptance:
  style: gherkin
  dir: tests/Acceptance
  run: ["vendor/bin/pest", "--log-junit", "{report}", "{files}"]
```

**Why:** the value is reserved (A-7). The message names it and says it arrives later,
so it is not mistaken for a typo.

### 10.12 Invalid — a `..` in `dir`

<!-- example: invalid-acceptance-dir -->
```yaml
acceptance:
  dir: ../shared/tests
  run: ["vendor/bin/pest", "--log-junit", "{report}", "{files}"]
```

**Why:** the tests the core runs and locks stay inside the project.

## 11. Version history

**2.2.1 — `acceptance.run` is executed (AT-8).** §3.6 no longer says that the core does
not run it. No key, value or rule changes, so every valid file stays valid.

**2.2.0 — the `acceptance` section (AT-4).**

- §3, new §3.6: `acceptance.required`, `style`, `dir`, `run` and `fakes_allowed`. New
  D-10, D-11 and D-12; new examples 10.7 to 10.12.
- §6: unknown keys are reported inside `acceptance` too.
- §7: **Invalid** covers a section that breaks §3.6.
- `plan approve` reads `acceptance.required` (`plan-header.md` §5.3), and refuses
  `--acceptance` when the section is absent. Nothing runs the command yet.

**Why:** acceptance tests need a command and a directory that only the project knows,
and keeping them in configuration keeps the core ignorant of any test framework (A-2).
The default for a ticket belongs here too, and the decision stays a person's (A-6).

**Compatibility:** new optional section. A config without it behaves exactly as
before, so no file that was valid becomes invalid. A config written for 2.2.0 read by
an older `invar` warns about the unknown key `acceptance` and keeps working.


**2.1.0 — `done.commit` and `done.commit_message`.**

- §3, new §3.5: `invar done` commits the Plan's deletion when
  `done.commit: true`, under `done.commit_message` (default
  `Remove plan {ticket}`). `--commit <MESSAGE>` and the new `--no-commit`
  override it. New D-9; example 10.1 sets both keys.
- §6: unknown keys are reported inside `done` too.

**Compatibility:** new optional keys only. Absent, `done.commit` is `false`,
which is the previous behavior, so no file that was valid becomes invalid and
no command's behavior changes. A config that sets `done` is read by an older
`invar` with a warning for the unknown key `done`, and nothing is committed.

**2.0.1 — `invar clean-spec` is gone.**

- §3, §3.3: `git.base_branch` no longer feeds `clean-spec --merged`, which was
  removed with the command; it serves the review diff alone.

**Compatibility:** the key, its default and its values are unchanged.

**2.0.0 — `review.offer` defaults to `true` (M1-25).**

- §3: the default of `review.offer` is `true`; §3.2 and examples 10.2 and 10.4
  say so. New D-8.

**Why:** the coder no longer reads `BUSINESS_CONTEXT.md` (DESIGN §5c, M1-24), so
the review is the safety net for an invariant the planner missed (§13c), and a
net that is off by default is not one.

**Compatibility:** no file that was valid becomes invalid, and no command's
behavior changes. A project that never set `review.offer` is now offered the
review before `invar done`, where it was not before; one that wants the old
behavior writes `offer: false`. That change in what an absent key means is why
this is a major version.

**1.0.0** — initial version (M1-3).
