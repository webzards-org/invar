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
  writes it: `--acceptance` or `--no-acceptance` if given, otherwise `skipped`
  (`invar.yaml` joins that order with its `acceptance:` section). The value is
  printed with the criteria, and `--reapprove` keeps the recorded value unless a
  flag names another. On a Plan already approved and unchanged, a flag changes
  nothing unless `--reapprove` is given too. The planner does not write the field,
  and a `failed` entry clears it. Nothing acts on the value yet.

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
  <TICKET>" && git push`.

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
