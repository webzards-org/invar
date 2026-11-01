# Changelog

## Unreleased

### Removed

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

### Added

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
  and a `failed` entry clears it. Nothing acts on the value yet.

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

- **The planner finds every caller before it changes a signature**, and reads
  the source of the framework or library functions the Plan relies on, instead
  of assuming their defaults. What it found goes into Scope.

### Changed

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
