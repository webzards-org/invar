# Changelog

## 0.1.1 — 2026-11-02

The first public release. No change to how Invar behaves: what is new is how
it is published, licensed and verified. Dependencies updated: `sha2` 0.11,
`serde-saphyr` 2.0, `serde_json` 1.0.154, and the CI actions.

### Changed

- **The repository is prepared for public release.** Invar is dual-licensed
  under MIT or Apache-2.0 (`LICENSE-MIT`, `LICENSE-APACHE`), and both licenses
  ship with the plugin on `dist`. `SECURITY.md` says how to report a
  vulnerability privately; `CONTRIBUTING.md` asks for a DCO sign-off on every
  commit and points new work to GitHub issues.
- **Every released binary can be verified.** The release workflow attaches a
  build provenance attestation to each binary, checks it before publishing, and
  attaches the binaries with `SHA256SUMS` to a GitHub release for the tag.
  `docs/INSTALL.md` shows how to check a downloaded or installed binary, and
  takes the terminal CLI from the release instead of a clone of `dist`.
- **CI's supply chain is pinned.** Every action is pinned to a commit, kept
  current by Dependabot; CI's token is read-only, and the release builds with
  no cache.
- **No private repository access is needed any more**, and `docs/INSTALL.md`
  no longer describes it.
- Notes and test comments no longer name the pilot project or its tickets.
- `CODE_OF_CONDUCT.md`: the Contributor Covenant 2.1.

## 0.1.0 — 2026-11-02

The first release. It was beta-tested on one project, and the entries under
"Changes during the beta" below record what changed for its users on the way.
A new user can start from this summary, [`docs/INSTALL.md`](docs/INSTALL.md)
and [`docs/USAGE.md`](docs/USAGE.md).

### What is in it

- **The Plan gate.** A planner agent writes a Plan with numbered acceptance
  criteria; the developer approves it with `/invar:approve`; a checksum freezes
  it; `invar plan check` refuses an unapproved or changed Plan before any agent
  writes code; a hook keeps `Write` and `Edit` off an approved Plan.
- **Context.** `AI_CONTEXT.md` (conventions) and `BUSINESS_CONTEXT.md`
  (invariants, regional facts), delivered to the right agents by hooks:
  business rules reach the coder only through the approved Plan.
- **Phase 2 in sub-agents.** The main session dispatches; the coder implements
  with the ticket line only, and answers `DONE`, `BLOCKED` or `BLOCKED-TEST`.
- **Locked acceptance tests** (optional, per project and per ticket). A
  test-writer agent turns each criterion into a test; `invar acceptance lock`
  locks it only if every criterion is covered and every test fails on an
  assertion before the code exists; `plan check`, `done` and
  `invar acceptance verify` (for CI) catch any change. `/invar:relock` lets the
  developer unlock a test the coder showed to be wrong, at most twice per
  ticket.
- **Review.** `/invar:review`, advisory: per criterion, per locked test
  (`asserts`, `weaker`, `unrelated`), shared test code, every invariant, Scope.
- **Decision log.** `/invar:log` drafts an entry, `/invar:log-approve` records
  it; the planner reads the log on later tickets. Entries record whether the
  ticket had acceptance tests.
- **Closing.** `invar pr-body` renders the Plan as the PR description;
  `/invar:done` deletes the Plan in the same PR, after checking it and the locks
  once more.
- **`invar init`**, and the plugin with static binaries for Linux x86_64 and
  macOS (Apple silicon and Intel).

### Changes during the beta

#### Removed

- **`invar clean-spec` is gone**, with its flags `--dry-run`, `--force` and
  `--merged`, and the merged-branch detection behind `--merged`.

  **What replaces it:** `invar done` is now the only way a Plan is deleted. It
  closes a cycle by deleting `.invar/plans/<TICKET>-<slug>/` and staging the
  deletion, inside the PR. A Plan that reached the main branch without `done`
  is removed by hand, in a commit of its own:

  ```bash
  git rm -r .invar/plans/<TICKET>-<slug>
  ```

  Scripts or CI jobs that call `invar clean-spec` fail with an unknown-command
  error; drop the call, or replace it with `invar done <TICKET>` on the PR
  branch. `git.base_branch` in `invar.yaml` stays: it sets the base branch of the
  review diff.

#### Added

- **`Acceptance: required | skipped`**, a new Plan header field (Plan contract
  1.7.0) that records whether a ticket needs acceptance tests. `plan approve`
  writes it: `--acceptance` or `--no-acceptance` if given, otherwise
  `acceptance.required` from `invar.yaml`, otherwise `skipped`. The value is
  printed with the criteria. `--reapprove` keeps the value in the header unless a
  flag names another, and labels it by what the checksum shows: if the body is
  unchanged and the value is not the approved one, it refuses until a flag chooses,
  and if the body changed too, it prints that the earlier approval cannot confirm
  the value. On a Plan already approved and unchanged, a flag changes
  nothing unless `--reapprove` is given too. The planner does not write the field,
  and a `failed` entry clears it. The main session acts on the value in Phase 2.

- **The `acceptance:` section in `invar.yaml`** (contract 2.2.0): `dir` and `run`
  are required, `required` defaults to `true` and is the default for `plan approve`,
  `style` is `integration` (`gherkin` is reserved), and `fakes_allowed` is passed on
  as it is. `run` is a list run without a shell, with `{report}` and `{files}` each
  exactly once as a whole element. **Configs without the section
  behave as before.** `plan approve --acceptance` needs the section and is refused
  without it, and a recorded `required` kept by `--reapprove` with no section prints a
  warning. `invar init` writes the section commented out, because its values depend
  on the project.

- **The `invar-lock:` line** (new contract `acceptance-lock.md` 1.0.0, with fixtures
  in `contracts/fixtures/acceptance/`): one line near the top of an acceptance test,
  with any comment prefix, naming its ticket and, once locked, the digest of the file
  without that line. `invar hash --lock <file>` prints that digest and says what the
  line records. `invar acceptance lock` writes the digest, and `plan check`, `done` and
  `invar acceptance verify` check it.

- **A reader for JUnit XML test results** (new contract `test-results.md` 1.0.0, with
  real Pest and PHPUnit reports in `contracts/fixtures/junit/`): each test case's
  outcome (`passed`, `failure`, `error`, `skipped`), its file relative to the project
  root, and the `AC<n>` token in its name. The exit code of a run is never read, and
  the ticket comes from the file's lock line, never from a name. A failure's type
  and the start of its message are kept, for showing only (contract 1.1.0).

- **`invar acceptance spec <TICKET>`** prints everything the test-writer needs: the
  style, `acceptance.dir`, the `run` command, `fakes_allowed`, the conventions of the
  style, every criterion with its id, and the Plan's Scope and Implementation context.
  The same Plan and config print the same bytes, and nothing is cut for size. It
  refuses what `plan check` refuses, a ticket approved as `skipped`, an `invar.yaml`
  with no `acceptance:` section, and criteria with no ids. Never the decision log, or
  `BUSINESS_CONTEXT.md` beyond what the Plan quotes. No agent uses it yet.

- **`invar acceptance lock <TICKET>`** turns a ticket's pending acceptance tests into
  locked ones, and is the first command that runs anything (`acceptance-lock.md` 1.1.0
  §9). It runs `acceptance.run` on the ticket's files and reads the JUnit report, never
  the exit code. It writes the digests only if every criterion has a test, every test
  has exactly one `AC<n>` token of this Plan, and every new test fails on an assertion.
  A test that passes, errors or is skipped is refused, and a test that passes has no
  override: the test-writer answers `BLOCKED` and the developer decides. An empty or
  missing report is refused as "the tests did not run", with the runner's own output.
  All the problems are listed at once, and a refusal writes nothing. A run that takes
  over 10 minutes is killed and refused. On success the last line is
  `locked <TICKET> <N> file(s)`. No agent uses it yet.

- **The test-writer sub-agent** (`plugin/agents/test-writer.md`, `model: opus`) turns a
  ticket's criteria into acceptance tests between the gate and the coder, following
  `invar acceptance spec`, and locks them with `invar acceptance lock`. It answers
  `LOCKED` only after the command's `locked <TICKET> <N> file(s)` line, or `BLOCKED` with
  the reason. It never changes application code, factories, seeders or the Plan, and reads
  neither `BUSINESS_CONTEXT.md` nor the decision log. `AI_CONTEXT.md` reaches it through a
  second `SubagentStart` hook.

- **`/invar:approve` takes `--acceptance` and `--no-acceptance`, and Phase 2 starts the
  test-writer for a required ticket.** After a recorded approval the main session reads the
  `acceptance:` line of `invar plan check`, never the conversation. With `skipped` it starts
  the coder. With `required` it starts the test-writer, and the coder only after `LOCKED`
  and a passing `invar acceptance verify`. A test-writer's `BLOCKED` goes to the developer
  and starts no coder. Approval is still the developer's only manual step.

- **`BLOCKED-TEST` and `/invar:relock`: a locked test can be wrong, and only the developer
  can say so** (`acceptance-lock.md` 1.3.0 §11). The coder answers `BLOCKED-TEST <file> <AC>
  <reason>` and changes no test, and the main session puts it to the developer and stops.
  `/invar:relock <TICKET> <file>...` runs `invar acceptance unlock`, which agents are denied. It
  takes each file back to pending, all or nothing, and notes it in an `unlocked` record beside the
  Plan. The main session then starts the test-writer, runs `invar acceptance verify`, and starts
  the coder again. A ticket has two rounds; a third is refused and says to end the cycle as
  `failed`.

- **`invar acceptance lock` skips the red check for a file the developer unlocked**, and only for
  that file, once per unlock. It may pass or fail on an assertion. An error or a skipped test is
  still refused, tokens and coverage apply as always, and the output names each file that skipped
  the check. `invar acceptance verify <TICKET>` (on stderr) and `invar done` (on stdout) list what
  was unlocked. `plan guard` denies `Write` and `Edit` of the record; a shell can still write it,
  and a skip is never quiet (D-30).

- **`invar acceptance spec` prints more, when there is more.** The unlocked files, and a Plan's
  `## Acceptance tests to change`, each with what to do: a `delete` is removed, and a `change` is
  answered with `BLOCKED` for now (OQ-4). A ticket with neither prints the same bytes as before.

- **The coder knows who writes the criteria's tests.** With `acceptance: required` it makes the
  locked tests pass and never writes an `invar-lock:` line; with `skipped` it writes its own, with
  no such line. Its unit tests are its own either way.

- **`plan approve --skill` reports argument errors as `error: …` with exit 0**, including
  `--acceptance --no-acceptance` and an unknown flag, so `/invar:approve` shows the reason
  instead of "no output at all". Without `--skill` they still exit 1 on stderr, and other
  commands are unchanged, except `acceptance unlock --skill`, which does the same. The text for
  the two flags together changed: it now names them in invar's words, not clap's.

- **`--colors=never` in the Pest examples** of `invar init`, the messages and the contract
  (`invar-yaml.md` 2.2.2), because Pest's colour codes reached the "tests did not run"
  messages. An existing `run` keeps working, and adding the flag to it is your choice.

- **`invar plan guard` keeps `Write` and `Edit` off locked acceptance tests.** Under
  `acceptance.dir` it denies a call on a file with a locked line, naming the ticket and
  what to do instead (`BLOCKED-TEST`, or declaring the change in the Plan), and a call
  that would write a digest into a lock line, since only `invar acceptance lock` writes
  one. Pending files, new files and everything outside `acceptance.dir` pass, as does
  everything with no `acceptance:` section. A malformed line or an unreadable
  `invar.yaml` passes with the problem reported. This is a convenience and not a boundary:
  `sed -i` gets past it, and `plan check`, `done` and `invar acceptance verify` catch that.

- **`invar acceptance verify [<TICKET>]`, and the same check in `plan check` and `invar
  done`** (`acceptance-lock.md` 1.2.0 §10). None of them runs a test.
  - Every locked file under `acceptance.dir` has the digest its line records, and no
    line is malformed.
  - A test that another ticket locked at the merge base may change on this branch, in
    the working tree, the index or `HEAD`, only if the approved Plan lists it in
    `## Acceptance tests to change` with `change`, and be deleted only with `delete`. A
    rename is a delete. A declared path that is not a locked test, or did not change, is
    a warning (Plan contract 1.7.1).
  - `done`, and `verify` with a ticket, also ask for a locked test for a `required`
    ticket, and refuse a pending line left for it.
  - `verify` with no ticket is for CI: it needs no Plan and no git, works in a shallow
    clone, and refuses a pending line of any ticket.

  **Upgrading:** nothing changes without an `acceptance:` section, except that `plan
  check` and `done` print one warning on stderr when the Plan says `required`. With one, `plan check` and `done` refuse inside a
  repository whose base branch or merge base cannot be found: fetch it, or set
  `git.base_branch`. Outside git they warn and skip the branch rule. A digest is not a
  signature: for a ticket's own new files, review and the PR diff catch a digest written
  by hand. See `docs/INSTALL.md` for the CI step.

- **`plan check` prints one more line** on success: `acceptance: required` or
  `acceptance: skipped`. Every existing line stays as it was.

- **`## Acceptance tests to change`**, an optional Plan section right after
  `## Acceptance criteria` (Plan contract 1.6.0): `- <path> — change — <reason>`
  or `- <path> — delete — <reason>`. `plan validate` checks its form. It is how a
  later ticket will be allowed to change another ticket's locked acceptance tests;
  nothing reads the paths yet. The section reaches the PR description and the
  review when present.

- **`/invar:approve` starts Phase 2.** When `invar plan approve` records an
  approval it now ends with one fixed line, `approved <TICKET> checksum
  sha256:<hex>`, and on that line the session begins Phase 2 at once — no second
  prompt. `/invar:approve <ticket> --no-start` approves without starting it. A
  refusal, or a Plan that was already approved, starts nothing.

- **`invar done --commit <MESSAGE>`** commits the Plan's deletion, alone, under
  the given message (`git commit --only`): anything else that is staged stays
  out of it. Hooks, signing and identity apply and are never bypassed; nothing is
  pushed. For CI: `invar done <TICKET> --commit "chore: remove the Plan of
  <TICKET>" && git push`. With an `acceptance:` section this needs history, while
  `invar acceptance verify` does not: a shallow checkout such as the `fetch-depth: 1` of
  GitHub Actions is refused, and the refusal and `docs/INSTALL.md` say how to fetch the
  base branch.

- **`done.commit` in `invar.yaml`** (contract 2.1.0) makes `invar done` and
  `/invar:done` commit the deletion without `--commit`, under
  `done.commit_message` — default `Remove plan {ticket}`, the ticket id put in;
  set it to a message your commit-message hook accepts:

  ```yaml
  done:
    commit: true
    commit_message: "chore: remove the Plan of {ticket}"
  ```

  `--commit <MESSAGE>` still wins, and the new `--no-commit` only stages. Absent,
  nothing is committed unless asked, as before.

- **`acceptance: required | skipped` in decision entries** (decision-entry contract
  1.4.0): how the ticket's approved Plan decided about acceptance tests, kept after
  the Plan is deleted, as a header key that can be counted. `/invar:log` copies it
  into the draft from the `acceptance:` line of `invar plan check`. `invar decision
  approve` checks it against the Plan, read before a `failed` reset: the same value is
  recorded, another value or one a draft-status Plan cannot confirm is refused, and a
  draft without the key gets the Plan's value, which the output says in an
  `acceptance: …` line. A Plan approved before the field counts as `skipped`.
  `invar decision record --acceptance required|skipped` gives it by hand.
  **An entry without the key means unknown, not `skipped`**: every existing entry
  stays valid and is counted neither way.

- **The PR description says how acceptance was decided**, right after the
  "Approved by" line. For `skipped`: *No acceptance tests: this ticket was approved
  without them.* For `required`: each file locked for the ticket under
  `acceptance.dir` with the criterion ids its text names (read by the criterion token
  rule, not a coverage proof), any criterion no locked file names, and, while the
  unlock record is there, `unlocked by the developer: <file> (round <R>)` — `done`
  deletes the record with the Plan, so the PR is where it survives. With no
  `acceptance:` section, `required` and the warning `done` prints. `pr-body` still runs
  no test and needs no git.

- **The review checks the locked acceptance tests.** `invar review-input` prints
  `=== Acceptance tests ===` after the criteria: the Plan's `Acceptance`, and for
  `required`, per criterion the locked files whose text names it, the criteria none
  names, and the files the developer unlocked, with their rounds. It gives files, not
  test names, which only a run would know. The reviewer reads those tests and reports
  each as `asserts`, `weaker` or `unrelated`, with `file:line` for the last two, reads
  unlocked files with extra care, and reports factories, seeders and helpers a locked
  test uses that the branch changed. With `skipped` it lists the criteria no test
  covers, as information. A change the Plan declares in `## Acceptance tests to
  change` is not reported as outside Scope. The report gains three headings after
  Acceptance criteria; the existing ones are unchanged. Still advisory: `done` does
  not depend on it.

- **The planner finds every caller before it changes a signature**, and reads
  the source of the framework or library functions the Plan relies on, instead
  of assuming their defaults. What it found goes into Scope.

#### Changed

- **Every PR description has one more paragraph** after the "Approved by" line, the
  acceptance decision above. For a Plan approved before the `Acceptance` field it reads
  *No acceptance tests: this ticket was approved before invar recorded that
  decision, which counts as without them.* Nothing else in the description moves.

- **`Approved-checksum` covers the `Acceptance` value too** (Plan contract 1.7.0):
  it is the digest of the line `Acceptance: <value>` and the body, so changing,
  removing or clearing the value after approval makes `plan check` and `done`
  refuse. `plan approve` always writes a value, so a new approval's checksum is no
  longer what `invar hash` gives for the body alone. **Plans already approved keep
  working:** with no value, the checksum stays that of the body, and the field
  reads as `skipped`. Adding a value to one by hand makes it stale. `plan validate`
  now refuses any other value under an `Acceptance` key, and a body whose first
  line is an `Acceptance` line.

- **Acceptance criteria are numbered** (Plan contract 1.6.0). Each criterion is a
  list item that starts with its id, `- AC1: Given …, when …, then ….`; ids are
  unique, contiguous from `AC1` and in order. `plan validate` and `plan approve`
  refuse anything else, naming the line, and `/invar:review` reports by id.

  **What a team has to do:** new Plans need ids, and the planner now writes them.
  A draft written before this version needs ids added by hand, or by the planner,
  before it can be approved. **Plans already approved keep working:** one whose
  criteria have no ids passes `plan check` and `done` unchanged. Do not add ids to
  it, because that changes the approved text. If it has to be approved again, with
  `--reapprove` or after a `failed` entry, it needs ids first.

- **`invar done [TICKET]` only deletes the Plan.** It checks that the Plan is
  approved and unchanged, deletes its directory and stages the deletion. It no
  longer writes a decision entry or renders the PR description, and `-m` and
  `--with-ticket` are gone. Without a ticket it closes the Plan whose ticket the
  current branch's name contains. It refuses while an unapproved draft entry
  (`decision.md`) is in the Plan directory. Render the PR description with
  `invar pr-body <TICKET> --out .invar/out/<TICKET>-pr.md` before running it.
- **Decision entries are approved by the developer.** The agent drafts one with
  `/invar:log`; the developer records it with `/invar:log-approve`
  (`invar decision approve <TICKET>`). `invar decision record` stays for entries
  the developer writes by hand.
- **`plan guard` denies the agent** `invar done`, `invar decision approve` and
  `invar decision record`, as it already denied `invar plan approve`.
