# Contract: `invar.yaml`

**Version:** 2.7.0 — `approval.signed` is read from the base by the verifier of approvals; see §11
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
| `approval.signed` | bool | `false` | Whether `plan approve` signs the approval (`approval-signature.md`); see §3.7 |
| `approval.commit` | bool | `false` | Whether `plan approve` commits the approval without being given `--commit` |
| `approval.commit_message` | string | `Approve plan {ticket}` | The message of that commit; `{ticket}` is replaced by the Plan's ticket id |
| `cycle.required_paths` | list of strings | empty: nothing required | The paths a branch may change only through a cycle, or with a skip a person wrote, as `invar cycle check` reads them **from the base branch**; see §3.8 |

A dotted key is a key inside a section: `review.offer` is written as

```yaml
review:
  offer: true
```

The sections `review`, `git`, `context`, `done`, `acceptance`, `approval` and `cycle` must be mappings
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

#### 3.3.1 Which base a CI command uses

**Nothing a branch changes chooses what that branch is checked against** (D-20). The
commands CI runs — `invar config check`, `invar cycle check`, `invar approval verify`, and
`invar acceptance verify` with a ticket — find their base in this order:

1. `--base <ref>`;
2. `git symbolic-ref refs/remotes/origin/HEAD`, the remote's default branch;
3. `main`.

They never read `git.base_branch`, from the working tree or from `HEAD`. When no `--base`
is given and `invar.yaml` at `HEAD` sets `git.base_branch` to another branch than the one
found, they say so in a note naming both, and recommend `--base`: a team whose pull
requests target `develop` gives `--base origin/develop`. `invar acceptance verify` without
a ticket compares nothing with a base, and is unchanged.

The local commands — `plan check`, `done`, `review-input`, `acceptance lock` — keep the
order above with `git.base_branch` first. They run for the developer, and CI is the
boundary.

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
  run: ["vendor/bin/pest", "--colors=never", "--log-junit", "{report}", "{files}"]   # required
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

The examples pass `--colors=never` because the runner's output is shown as it is when the
tests did not run, and colour codes would make it unreadable there.

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

### 3.7 The `approval` section

Read by `invar plan approve`. Absent, or empty, it changes nothing: no signature
is made, and no commit (D-13).

```yaml
approval:
  signed: true
  commit: true
  commit_message: "chore: approve the Plan of {ticket}"
```

**`signed`**, `false` by default. When `true`, `plan approve` signs every
approval it writes with the developer's SSH signing key, and refuses an approval
it cannot sign (`approval-signature.md` §5). The signature is written next to the
Plan as `approval.sig`. The key only says what `plan approve` writes: whether a
branch's approvals are signed is checked where the agent cannot change the
answer, against the base branch's `.invar/approvers` (`approval-signature.md`
§6). **Verification is required when `invar.yaml` at the base has
`approval.signed: true`.** A branch that turns it off is still checked, with a
note, by the rule of §3.3.1: `invar approval verify` reads the key from the base,
never from the branch. `invar config check` reports the change as a weakening
(§3.9).

**`commit`** and **`commit_message`** work as `done.commit` and
`done.commit_message` do (§3.5), for the approval instead of the deletion:

1. `plan approve --commit <MESSAGE>` commits under that message, whatever the
   config says;
2. `--no-commit` writes the approval and commits nothing, whatever the config
   says;
3. otherwise `approval.commit` decides. Absent, it is `false`.

The commit holds the Plan's `spec.md`, and its `approval.sig` when there is one,
and nothing else: anything else that is staged stays staged. The project's hooks,
signing and identity apply and are never bypassed, and nothing is pushed. The
commit is a convenience: the guarantee is the signature, which does not depend on
any commit (`approval-signature.md` D-1). `commit_message` follows the rules of
`done.commit_message`: one line, non-empty once surrounding whitespace is removed,
`{ticket}` any number of times or not at all.

### 3.8 `cycle.required_paths`

Read by `invar cycle check`, which CI runs on a pull request, and by `invar cycle required`,
which the main session runs. Absent, empty, or an empty list, and no path needs a cycle:
`cycle check` says the check is off and exits 0 (D-14).

```yaml
cycle:
  required_paths:
    - "app/Models/**"
    - "app/Policies/**"
    - "database/migrations/"
```

**The value** is a list of patterns, each a string. A single string is an error, not a
list of one: it is how a pattern written without brackets looks. Each pattern is relative
to the project root, with `/` between segments. **An error**, each reported with the
others: an empty pattern, an absolute one (a leading `/` or a drive such as `C:`), a `\`,
an empty segment (`app//Models`), and a `.` or `..` segment. A trailing `/` is allowed,
below.

**Matching**, a small and exact part of `.gitignore`'s rules, with none of its others (no
`!`, no `?` or `[…]`, no matching at any depth):

1. A pattern and a path are split at `/` into segments, and the pattern is **anchored at
   the project root**: `*.php` matches `index.php` and not `app/User.php`.
2. **`*`** matches any run of characters within one segment, the empty one included, and
   never a `/`.
3. **`**`** as a whole segment matches any number of segments, none included:
   `app/**/Order.php` matches `app/Order.php` and `app/a/b/Order.php`. Inside a segment,
   `**` is the same as `*`.
4. Every other character matches itself, case included.
5. A pattern matches a path when it matches **the whole path, or a directory the path is
   in**: `app/Models` matches `app/Models/Order.php`.
6. A pattern ending in **`/`** matches only a directory the path is in:
   `database/migrations/` matches every file under it, and not a file of that name.

**Read from the base branch, never from the branch checked** (D-14). `cycle check` reads
`invar.yaml` from the commit the base names (`git show <base>:invar.yaml`), as a verifier of
approvals reads `.invar/approvers` (`approval-signature.md` §6.1, D-3): a branch must not be
able to switch off its own check. A branch that adds, removes or changes a pattern is
checked against the base's list, and the output says how its own list differs. A change to
the list applies to the branches after it is on the base. An invalid `invar.yaml` at the
base is an error. `invar cycle required` is the session's question about its own working
tree, and reads the working tree's file.

**The base** is found by §3.3.1: `--base`, else the remote's default branch, else `main`,
and never `git.base_branch`. In CI give the pull request's target
(`--base origin/${{ github.base_ref }}`).

**What `invar cycle check` decides.** The **commits of the branch** are `<base>..HEAD`: what
the branch merged in from the base is the base's, not the branch's. It needs the history
from the merge base of the base and `HEAD` to `HEAD`: a shallow checkout that does not reach it is an error naming
`fetch-depth: 0`. Then:

- **The required changes**: every file that differs between the merge base and `HEAD`,
  committed only, under the project root, with `.invar/` left out: added, changed, deleted,
  and both names of a rename. Each one a pattern matches is a required change, named with
  the first pattern that matches it.
- **A closed cycle**: a Plan `.invar/plans/<TICKET>-<slug>/spec.md` that is absent at
  `HEAD`, that some commit of the branch deleted, and whose text at some commit of the
  branch is an **approval that holds**: `Status: approved`, every approval value present,
  and `Approved-checksum` the digest of that text (`plan-header.md` §5.3). The ticket is the
  Plan header's (D-16). A Plan the branch added or changed that is still at `HEAD` is an
  **open cycle**: it is named, with `/invar:done`, and does not count.
- **A skip**: a trailer `Invar-Skip: <reason>` on **any commit of the branch**, with a
  reason that is not empty once trimmed. git matches the key without regard to case. A
  trailer with no reason is named, and does not count (D-15).

- **A covered commit**: a commit of the branch is **covered by ticket T** when, at its first
  parent, T's Plan is there and its approval holds there, and T is a closed cycle of the
  branch (D-17). A root commit is covered by nothing.
- **A covered required change**: every commit of the branch that touched its path is covered
  by some ticket. A rename touches both names, and each name that is a required change is
  judged on its own. git's default history simplification applies, so a merge that only
  brought in a change made elsewhere did not touch the path.

The branch **passes** when there is no required change, or every required change is
covered, or there is at least one skip. One pull request may close several tickets, and each
covers the commits made while its Plan was approved. Otherwise it **fails**, non-zero, with
every required change that is not covered, its pattern and the commits that touched it
outside a cycle (each its short name and subject) on stderr, and the two ways out: a change
made while a Plan is approved and closed with `/invar:done`, or a trailer the developer
writes.

The output on stdout always says what was decided on: the base and the merge base, the
patterns, each required change with `covered by <TICKET>, …` or `not covered: <sha>
<subject>, …`, the closed cycles with the commit that deleted each Plan,
the open cycles, the skips with their commits and reasons, and the note on a changed list.
Its last line is one of `cycle check: off: <why>`, `cycle check: passed: <why>` and
`cycle check: failed`. It writes nothing, and it is not a developer-only command.

**Who writes the trailer.** A skip is a person's decision. `plan guard` denies an agent's
Bash command line that runs `git commit` and holds the key `Invar-Skip`, in any case
(`-m`, `--trailer`, a here-document), and tells the agent to ask the developer. That is a
scan of words, like the rest of the guard: a message written to a file and given with `-F`
gets past it. It makes a skip visible in CI and owned in review, not proven to be a
person's: binding a skip to the developer's key is a signed statement, as an approval is,
and comes with the verification of signed approvals (#154).

**`invar cycle required <path>...`** prints `<path>: required (<pattern>)` for each path a
pattern of the working tree's list matches, then `cycle: required` or `cycle: not
required`, and exits 0.

### 3.9 Weakening the checks

Some keys decide whether the Plans, the locked tests and the required paths are checked at
all. Changing them in one direction switches a check off, and an agent with an edit tool
could do that with one edit (#157). **Weakening** is this closed list, and nothing else
(D-18). Values are compared as read, with their defaults, so removing a key whose default is
the stronger value weakens nothing:

| A change from the base to the branch | Why it weakens |
|---|---|
| The `acceptance:` section removed | `acceptance verify` then warns and exits 0, and `plan check` and `done` no longer look at any lock (`acceptance-lock.md` D-21) |
| `acceptance.dir` changed | The locked files at the old path are no longer looked at |
| `acceptance.run` changed | `lock` and `acceptance run` would run something else, which could pass anything |
| `acceptance.required` from `true` to `false` | New tickets are approved without acceptance tests by default |
| `approval.signed` from `true` to `false` | Approvals are no longer signed, so an approval written by an agent looks like a person's |
| A pattern removed from `cycle.required_paths`, the key or the section with it | Changes to those paths no longer need a cycle; `cycle check` reads the base's list, and this keeps the change from landing quietly |
| `review.offer` from `true` to `false` | The main session stops offering the review before a pull request |

Adding a key or a pattern, or changing a value the other way, is never weakening, and every
key not in the list stays free to change. `git.base_branch` is not on the list, since no CI
command reads it (§3.3.1); `config check` notes a change of it all the same. A whole `invar.yaml` removed is every weakening its
keys allow. A change of `dir` or `run` is any change of its text, even one that names the
same thing (`./tests/Acceptance`).

**The boundary is CI.** `invar config check [--base <ref>]` reads `invar.yaml` at the base
and at `HEAD`, with the base found by §3.3.1, as `cycle check` reads it (§3.8), and lists every weakening with its
value at both. It passes with none. It passes a weakening when any commit of the branch
(`<base>..HEAD`) has an **`Invar-Config: <reason>`** trailer with a reason that is not empty
once trimmed, read as `Invar-Skip` is (D-15), and prints each reason and its commit.
Otherwise it fails, non-zero, naming each weakening and the way out: `git commit --amend
--trailer "Invar-Config: <reason>"`. No `invar.yaml` at the base: the check is off, and
exits 0. One at the base or at `HEAD` that is not a valid config: an error. A shallow
checkout that does not reach the merge base: the error of §3.8. Its last line is one of
`config check: off: <why>`, `config check: passed: <why>` and `config check: failed`. It
writes nothing, and it is not a developer-only command.

**The guard is a convenience** (D-19). `plan guard` denies a `Write` or `Edit` of the
project's `invar.yaml` when the file it would leave weakens the one there now, and names
each weakened key; it allows the call whenever it cannot tell, or either side is not a valid
config, because CI refuses what it lets through. An `invar.yaml` that is not the project's
own, such as one in a subdirectory with no project of its own, is not judged. It denies an
agent's `git commit` that writes the `Invar-Config` key, as it does `Invar-Skip`: a scan of
words, which a message given with `-F` gets past. Binding the trailer to the developer's key
comes with the verification of signed approvals (#154), as for a skip.

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
- Unknown keys are reported at the top level and inside the seven sections
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
| **Invalid** | Not UTF-8, not valid YAML, not a mapping, a duplicated key, a value outside §3 and §4, an `acceptance` section that breaks §3.6, a `cycle.required_paths` that breaks §3.8 | The path, the line and column where YAML reports one, and what is wrong |
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

**D-13 — Signing is opt-in** (2.3.0). A signature needs a key set up for SSH
signing, and, to be worth more than the checksum, one that asks a person on each
use (`approval-signature.md` §7). A project that has neither keeps working as
before, and turns it on once it has.

**D-14 — The required paths are opt-in, and read from the base** (2.4.0). Which paths need a
Plan is the team's decision: Invar can recommend models, policies and migrations, and only the
team knows its code. Absent, nothing changes, so every project keeps the behavior it had. The
list is read from the base because the branch is what is checked: a list read from the branch
could be emptied by the branch, and the check would pass a change it exists to catch.

**D-15 — A skip is a commit trailer, and one anywhere on the branch is enough** (2.4.0). The
main session may still skip the cycle for trivial work (L-9); what changes is that a skip on a
required path is visible in CI and has to be explained. A trailer, because it is part of the
commit message, and survives `git rebase --signoff` and every other rewrite that keeps
messages, where a commit signature is dropped by any rebase (`plugin/NOTES.md` §19). One
trailer on any commit, and not on every commit that touches a required path, because the pull
request is what is reviewed: one reason a reviewer reads covers the branch, and asking for it
on every commit would only teach people to paste it. That it is a person's is not proven here:
the guard is a scan, and a signed statement for a skip belongs with #154.

**D-16 — A closed cycle is read from the history, not from the branch name** (2.4.0). A branch
name is the developer's text and proves nothing. The commits do: a Plan whose approval holds in
one of them, deleted in a later one, is what `plan approve` and `invar done` leave behind. The
approval must hold in that text, and not only say `Status: approved`, so that a header typed by
hand is not a cycle. Whose approval it was is #154's to check.

**D-17 — A change is covered by the cycle whose approved Plan was at its parent** (2.4.0).
One closed cycle anywhere on a branch covering every change on it would pass a Plan about
documentation followed, after `done`, by a change to a model with no Plan at all. Which paths
a Plan is about is in its `## Scope`, and Scope is prose: matching it would be a guess. The
history answers instead, without reading it: the work of a cycle is made while its Plan is
approved, so a commit is the cycle's when the Plan is approved at the commit's parent. That
covers the coder's commits and the deletion by `done`, and not a commit made before the
approval or after the deletion. The parent, and not the commit, because the commit that
deletes the Plan is the cycle's and has no Plan of its own, and the commit that approves a
Plan was made before it was approved. A `failed` reset and a later approval need no rule of
their own: a commit is covered only while some approval holds. The rule is deterministic and
reads only what `plan approve` and `invar done` write.

**D-18 — Weakening is a closed list** (2.5.0). Which keys switch a check off is a fact about
the checks, and the same list has to be used by the guard and by CI, so it is stated once,
here. A list that grew by judgement ("anything that looks like a relaxation") could not be
checked by a program and would block ordinary edits; keys outside it, such as
`context.binding_limit` or `done.commit`, change what a command does and not whether
something is checked. Comparing values with their defaults keeps removing a key that only
restated its default from being reported.

**D-19 — CI is the boundary, the guard a convenience, and a trailer the way out** (2.5.0).
The guard sees only the edit tools: a shell, or a branch rewritten elsewhere, never reaches
it. CI compares what will be merged with what is there, with its own binary, which the agent
cannot change. A weakening is sometimes right (a suite moved, a key dropped for a while), so
it is not forbidden: it needs a reason a person wrote, which the pull request shows. The
trailer, and not a signed commit, for the reason of D-15: it survives `git rebase
--signoff`.

**D-20 — A branch does not choose its base** (2.6.0). A check compares the branch with what it
will be merged into. When the base came from `git.base_branch` in the branch's own
`invar.yaml`, one commit could set it to the branch itself: the merge base was `HEAD`, nothing
had changed, and `config check`, `cycle check` and the branch rule of `acceptance verify` all
passed a branch that stopped signing, changed a required path and deleted another ticket's
locked test (#165). The approvers list has the same answer for the same reason: it is read from
the base, never the branch (`approval-signature.md` §6.1, D-3). So the CI commands take the base
from their command line, where the CI configuration, not the branch, sets it, or from the
remote, and only note the key. Putting `git.base_branch` on the weakening list instead would
still let the branch name the base the list is read from. The local commands keep the key:
there the developer runs them, and a wrong base costs them a wrong answer, not a check.

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
  run: ["vendor/bin/pest", "--colors=never", "--log-junit", "{report}", "{files}"]
  fakes_allowed: [http, mail, queue, notification]
```

Reads as written. No warnings.

### 10.8 Valid — a minimal `acceptance` section

<!-- example: valid-acceptance-minimal -->
```yaml
acceptance:
  dir: tests/Acceptance
  run: ["vendor/bin/pest", "--colors=never", "--log-junit", "{report}", "{files}"]
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
  run: ["vendor/bin/pest", "--colors=never", "--log-junit", "{report}", "{files}"]
```

**Why:** the value is reserved (A-7). The message names it and says it arrives later,
so it is not mistaken for a typo.

### 10.12 Invalid — a `..` in `dir`

<!-- example: invalid-acceptance-dir -->
```yaml
acceptance:
  dir: ../shared/tests
  run: ["vendor/bin/pest", "--colors=never", "--log-junit", "{report}", "{files}"]
```

**Why:** the tests the core runs and locks stay inside the project.

### 10.13 Valid — a signed approval, committed

<!-- example: valid-approval -->
```yaml
approval:
  signed: true
  commit: true
```

**Why:** every approval is signed, and committed as `Approve plan <TICKET>`.

### 10.14 Valid — required paths

<!-- example: valid-cycle -->
```yaml
cycle:
  required_paths:
    - "app/Models/**"
    - "app/Policies/**"
    - "database/migrations/"
    - "routes/*.php"
```

**Why:** a branch that changes a model, a policy, a migration or a route file passes `invar
cycle check` only with a closed cycle or an `Invar-Skip:` trailer.

### 10.15 Invalid — a `..` in a pattern

<!-- example: invalid-cycle-dot-dot -->
```yaml
cycle:
  required_paths: ["../shared/**"]
```

**Why:** a pattern names paths under the project root as they are.

## 11. Version history

**2.7.0 — where the verifier reads `approval.signed` (#168).**

- §3.7: verification of signed approvals is required when `invar.yaml` at the base
  has `approval.signed: true`; a branch that turns it off is still checked, with a
  note. §3.3.1: `invar approval verify` is one of the CI commands, and finds its base
  as they do.

**Why:** `approval-signature.md` §6.1 read the approvers list from the base, but not
the switch, so a branch could turn the check off for itself (#168).

**Compatibility:** no key, value or default changes, so every valid file stays valid
and reads as it did. `plan approve` reads the key as before.

**2.6.0 — the CI commands do not take their base from the branch (#165).**

- §3.3, new §3.3.1: `config check`, `cycle check` and `acceptance verify <TICKET>` use
  `--base`, else `origin/HEAD`, else `main`, never `git.base_branch`, and note a key they did
  not use. §3.8 and §3.9 point to it; `config check` notes a change of the key. New D-20.
- `invar acceptance verify` takes `--base <ref>` for the branch rule.

**Why:** a branch that set `git.base_branch` to itself was compared with itself, and passed
every CI check (D-20).

**Compatibility:** no key or value changes. A CI step without `--base`, in a repository whose
`origin/HEAD` is not the branch pull requests target, now compares with the remote's default
branch, and says so; give `--base`. Local commands behave as before.

**2.5.0 — weakening the checks (#157).**

- §3, new §3.9: the closed list of weakenings, `invar config check`, the `Invar-Config:`
  trailer, and the guard on `invar.yaml`. New D-18 and D-19.
- `plan guard` reads the list for `invar.yaml`; `invar config check` reads it in CI.

**Why:** removing the `acceptance:` section, or changing `acceptance.dir`, switched the lock
check off in one edit, and CI stayed green (#157). The keys added since, `approval.signed`
and `cycle.required_paths`, can switch off a check the same way.

**Compatibility:** no key, value or default changes, so every valid file stays valid and
reads as it did. What is new is a command and a guard that compare two files.

**2.4.0 — `cycle.required_paths` (#155).**

- §3, new §3.8: the key, its glob rules, that `invar cycle check` reads it from the base
  branch, what the check decides, and the `Invar-Skip:` trailer. New D-14 to D-17, and
  examples 10.14 and 10.15.
- Revised in review, before 2.4.0 was released (D-17): a required change is covered only when
  every commit of the branch that touched it was made while a closed cycle's Plan was
  approved at the commit's parent, and not by any closed cycle on the branch. The branch's
  commits are `<base>..HEAD`.
- §6: unknown keys are reported inside `cycle` too. §7: **Invalid** covers the key.
- New commands read it: `invar cycle check` and `invar cycle required`. `plan guard` denies
  an agent's `git commit` that writes the trailer.

**Why:** whether a change goes through the cycle was decided only by the main session, the
agent the cycle constrains, and a branch that skipped it looked in CI like one whose Plan was
closed (#155).

**Compatibility:** a new optional section. Absent, nothing is required and `cycle check`
exits 0, so every valid file stays valid and every command behaves as before. A file written
for 2.4.0 and read by an older core gets a warning for the unknown `cycle` key.

**2.3.0 — the `approval` section (#154).**

- §3, new §3.7: `approval.signed`, `approval.commit` and `approval.commit_message`.
  New D-13 and example 10.13.
- §6: unknown keys are reported inside `approval` too.
- `plan approve` reads them: it signs the approval (`approval-signature.md` 1.0.0),
  and commits it when asked.

**Why:** an approval anyone with a shell can write is not a person's decision. A
signature by a key the developer holds is one the agent cannot make, where the key
asks a person on each use (#154).

**Compatibility:** an absent section is today's behavior, so every valid file stays
valid and means what it meant. A file written for 2.3.0 and read by an older core
gets a warning for the unknown `approval` key, and nothing is signed.

**2.2.2 — `--colors=never` in the examples (AT-11).** Pest's colour codes reached the messages
of `invar acceptance lock`. No key, value or rule changes.

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
