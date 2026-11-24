# Changelog

## Unreleased

### Added

- **Signed approvals, opt-in.** With `approval.signed: true` in `invar.yaml`,
  `invar plan approve` (and `/invar:approve`) signs the approval with your SSH
  signing key, the one git uses for signed commits, and writes the signature
  next to the Plan as `approval.sig`. It refuses an approval it cannot sign,
  and writes nothing, when git has no SSH signing key, when the key does not
  sign, or when signing takes more than 90 seconds. Who may approve is listed
  in `.invar/approvers`, in git's allowed-signers format, read from the base
  branch. A signature is worth more than the checksum only with a key that
  asks you on each use; `docs/INSTALL.md` says how to set one up, and why
  (#154). New contract `approval-signature.md` 1.0.0; `plan-header.md` 1.8.0;
  `invar-yaml.md` 2.3.0.
- **`plan approve --commit <MESSAGE>`** commits the approval, alone: the Plan,
  and its signature when there is one. `approval.commit: true` does it every
  time, as `Approve plan <TICKET>`; `--no-commit` skips it once. Your hooks
  and commit signing apply, and nothing is pushed (#154).
- **Locked acceptance tests have to pass, not only stay unchanged.**
  `invar acceptance verify --results junit.xml` reads the JUnit report of your
  own test run, for CI: every locked file must have a test case in it, and every
  one must have passed. It fails, naming file, test and criterion, on a locked
  file your suite did not run, on `failure`, `error` and `skipped`, and on a test
  that passed only on a retry. `--report-root /var/www/html` maps the paths of a
  runner in a container, such as PHPUnit under Laravel Sail. `invar done
  --results` makes the same check for the ticket before it deletes anything.
  Without `--results`, nothing changes. Invar still never runs your suite (#156).
- **`invar acceptance run <TICKET>`** runs the ticket's locked tests, and only
  them, and says per criterion whether they pass, ending with `acceptance:
  green <TICKET>` or `acceptance: red <TICKET>`. The coder runs it before it
  answers `DONE`, and the main session after: on red it shows what failed,
  offers the coder again, and offers no commit, review or `/invar:done` (#156).
  Contracts `acceptance-lock.md` 1.4.0 and `test-results.md` 1.2.0.
- **A lock line records the criteria its file covers.** `invar acceptance lock`
  writes `ac=AC1,AC2` after the digest, from the run that locked the file, and
  the digest covers the list. `verify --results`, `done --results` and
  `acceptance run` refuse a file whose tests ran only in part, naming the
  criterion: `<file>: AC2: no test of this criterion from this file is in the
  results`. Files locked before keep their digest and the weaker check, with a
  note on stderr, until they are locked again. `invar hash --lock` shows the
  list, and `--ac AC1,AC2` gives the digest a list would get. `plan guard`
  denies writing `ac=` into a lock line by hand (#156). Contract
  `acceptance-lock.md` 1.5.0.
- **`--results` with no `acceptance:` section is an error**, for `verify` and
  `done`: the check was asked for and cannot run. Without `--results`, a missing
  section still only warns (#156).

- **A CI check that changes to declared paths went through a cycle, opt-in.**
  List them in `invar.yaml` as `cycle.required_paths` (globs: `*`, `**`, a
  trailing `/`). `invar cycle check --base <ref>` passes a branch that changes
  one only when every commit that touched it was made while the Plan of a
  cycle closed on the branch was approved (read from the history: approved
  before, deleted by `/invar:done` after), or with an `Invar-Skip: <reason>`
  commit trailer the developer writes. It fails naming each uncovered path, its
  pattern and the commits that touched it.
  The list is read from the base branch, so a branch cannot switch off its own
  check. It needs the history from the merge base: `fetch-depth: 0` in CI. With
  no list on the base, it is off and exits 0 (#155). Contract `invar-yaml.md`
  2.4.0.
- **`invar cycle required <path>...`** says which paths need a cycle. After a
  change made with the cycle skipped, the main session runs it and tells you
  when the pull request will need a cycle or your `Invar-Skip:` trailer. It
  never writes the trailer: `plan guard` denies an agent's `git commit` that
  does (#155).

- **`invar config check --base <ref>`** fails a branch that weakens what invar
  checks, comparing `invar.yaml` with the base branch's: the `acceptance:`
  section removed, `acceptance.dir` or `acceptance.run` changed,
  `acceptance.required`, `approval.signed` or `review.offer` turned off, or a
  pattern removed from `cycle.required_paths`. It names each one with both
  values, and passes a weakening with an `Invar-Config: <reason>` trailer the
  developer writes. With no `invar.yaml` on the base it is off (#157). Contracts
  `invar-yaml.md` 2.5.0 and `acceptance-lock.md` 1.6.0.
- **`plan guard` protects the configuration.** It denies an agent's `Write` or
  `Edit` that weakens `invar.yaml` by the same list, or that takes the
  `.invar/decisions/**` deny rule out of `.claude/settings.json`, and a
  `git commit` that writes the `Invar-Config` key (#157).
- **The CI example installs the released CLI and checks its attestation**, and
  runs `acceptance verify`, `config check` and `cycle check` (#157).

### Changed

- **The README no longer says the agent cannot move its target.** It says
  what holds: a change after approval is detected, and an approval written by
  an agent with a shell is not caught by the checksum. A signed approval, with
  a key that asks you on each use, shows who approved (#154).

### Fixed

- **An agent can no longer approve a draft Plan by editing it.** `invar plan
  guard` now also judges the header a `Write` or `Edit` would leave in a
  draft or a new `spec.md`, and denies one that sets `Status: approved` or
  fills `Approved-by`, `Approved-at`, `Approved-checksum`,
  `Lockfile-checksum` or `Acceptance` with a value the file did not have.
  Only `invar plan approve` writes them. A shell write still gets past the
  guard, as for locked tests. `docs/USAGE.md` now gives the two routes that
  work for changing an approved Plan: a `failed` entry, a revision by the
  planner and a plain `/invar:approve`, or an edit by the developer and
  `--reapprove` (#153).
  An `Edit` with an empty `old_string`, which creates a file or fills an
  empty one, is judged like a `Write`, and an edit whose result the guard
  cannot work out is judged by the text it writes (#153).

## 0.2.0 — 2026-11-18

The first beta release. Everything here is new; nothing changes how an
existing project behaves.

### Added

- **Issue forms for the beta**: Bug (where in the cycle it happened, with the
  invar, Claude Code and OS versions), Beta feedback (what helped, what got in
  the way, the one change that matters most) and Idea. Blank issues are off;
  security problems go to the Security tab (#140).
- **The README says what the beta means.** Its Status section now says what
  is stable (the versioned formats in `contracts/`), what may still change
  (prompts, commands, flags, output), and the known limitations: Claude Code
  only, Linux x86_64 and macOS, a working test setup for locked tests, and the
  binding limit on the two context files (#146).
- **`docs/USAGE.md` answers whether Invar works with other skill sets**: which
  kinds of skills complement it, which overlap with the cycle and should give
  way to Invar's agents inside one, and which contradict its approval steps
  (#146).
- **`BUSINESS_CONTEXT.md` has a `## Terms` heading.** `invar init` scaffolds
  it before the invariants, empty, for the business's own words, one line
  each. The planner uses those terms, with their meanings, in every Plan, when
  the section has content. A
  file with only headings is still treated as absent. An existing
  `BUSINESS_CONTEXT.md` is left as it is: add the heading by hand if you want
  it (#142).
- **The review reports broken conventions from `AI_CONTEXT.md`.** The
  reviewer's report has a new section, "Conventions in AI_CONTEXT.md the change
  appears to break", after the invariants: each finding quotes the convention as
  the file states it, with `file:line`. Only conventions the file states; style
  it does not state stays out. The reviewer also reads the invariants with the
  meanings `## Terms` gives. Still advisory: nothing blocks `done` (#144).
- **A proposed split says how it is cut.** After the tasks, the planner adds a
  `Strategy:` line: by layer, or vertical slices that each deliver a behavior a
  user can see, and why. When both fit the ticket, it sketches the other in two
  or three lines and asks which you prefer; when only one does, it says so and
  asks nothing. A split preference stated in `AI_CONTEXT.md` is followed
  without offering the other (#143).

## 0.1.4 — 2026-11-08

Tagged and published on `dist` without a version bump: its `plugin.json` and
`invar --version` say 0.1.3, so `claude plugin update` did not offer it. 0.2.0
carries the same change with the right version.

### Added

- **The plugin has a listing icon on `dist`.** `.claude-plugin/icon.png` is
  moved from the repository root into `plugin/`, unchanged, so the release
  copies it to `invar/.claude-plugin/icon.png`, where the plugin directory reads
  it. A test checks that it is a square PNG of 512 to 2048 px under 2 MiB, and
  `plugin/` accepts that one path and no other image or binary (#135).

## 0.1.3 — 2026-11-08

Answers the plugin directory's review. No change to what the CLI decides.

### Changed

- **Hooks name their interpreter.** Each hook now runs `sh` with the launcher
  `${CLAUDE_PLUGIN_ROOT}/bin/invar` as its first argument, instead of running
  the launcher directly, as the plugin directory's review asks. Each hook is
  still one `invar` call with the same arguments, exit codes pass through
  unchanged, and the hooks no longer depend on the launcher's executable bit
  (#133).

### Fixed

- **The `argument-hint` of the six slash commands is quoted.** Unquoted,
  `[ticket] [--acceptance | --no-acceptance] [--no-start]` and
  `[ticket] [--commit <message> | --no-commit]` were not valid YAML, and
  `[ticket]` alone read as a list, not as text. CI now reads the front matter
  of every agent and skill with a real YAML parser (#133).

## 0.1.2 — 2026-11-08

Tagged and published on `dist` without a version bump: its `plugin.json` and
`invar --version` say 0.1.1, so `claude plugin update` did not offer it. 0.1.3
carries the same changes with the right version. The release workflow builds the
macOS binaries on `macos-15`, since GitHub retired the macOS 14 runners (#131).

### Added

- **The planner recommends splitting a ticket too complex for one cycle.**
  Before it asks or writes anything, it analyses what the ticket changes:
  behaviors, modules and layers, invariants, unknowns. A ticket that holds
  several things that could each be built and reviewed on their own gets a new
  answer, `SPLIT`, with the reason, and no Plan. If the developer asks, the
  planner proposes the tasks, in dependency order, each with a title and a
  ticket description to paste into a tracker; if the developer says to plan it
  as one, it does. Invar never creates a ticket or gives one an id. The main
  session relays the answer and leaves the decision to the developer. No CLI or
  contract change. See `docs/USAGE.md`, "A ticket too large for one cycle" (#124).
- **The plugin on `dist` has a README for its users**, `invar/README.md`, for
  Anthropic's plugin directory, which shows it as the listing's description. It
  says what Invar is, how to install it, and plainly what the plugin runs and
  changes in a project: each hook and the command it runs, the agents and
  skills, every file `invar init` and the cycle write (including the Read deny
  rule for `.invar/decisions/` in `.claude/settings.json`), the git commands,
  the project's own test command, and that the CLI makes no network requests,
  calls no model and sends no telemetry; and how the binaries are built and
  verified. Its source is `plugin/README.dist.md`, which the release copies;
  `plugin/README.md` stays the contributor README. The manifest's
  `description` now names the locked tests, the decision log and the
  developer-only commands (#129).

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
