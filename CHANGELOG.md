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

### Changed

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
