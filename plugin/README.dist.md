# Invar

Invar keeps an approved Plan fixed while an AI agent writes the code against it.
A planner agent turns a ticket into a Plan with numbered acceptance criteria; the
developer reads and approves it; a checksum freezes it, and the agent that writes
the code cannot move the target it is measured against. Around that sit locked
acceptance tests the coder cannot change, business context the planner must
respect, a decision log that informs the next Plan, and deterministic checks: every
verdict comes from the `invar` CLI in this plugin, never from a model.

## Install and first steps

Invar needs Claude Code and `git`, on Linux x86_64 or macOS (Apple silicon or
Intel). In the root of your project:

```bash
claude plugin marketplace add webzards-org/invar#dist --scope project
claude plugin install invar@invar --scope project
```

Then start Claude Code in the project and ask it to run `invar init`. It scaffolds
the project (listed below) and changes nothing that is already there. Commit what
it created, fill in `AI_CONTEXT.md` and `BUSINESS_CONTEXT.md` with your team's
conventions and business rules, and bring a ticket, such as `SHOP-214`. Until
`invar init` has run, the `SessionStart` hook reports "not inside an invar
project" at the start of each session; it is harmless.

- Installing, teammates, acceptance tests, updating:
  [docs/INSTALL.md](https://github.com/webzards-org/invar/blob/main/docs/INSTALL.md)
- One ticket from request to merged PR:
  [docs/USAGE.md](https://github.com/webzards-org/invar/blob/main/docs/USAGE.md)

## What this plugin runs and changes

### Hooks

Each hook runs one command of the plugin's own `invar` binary, through the
launcher `bin/invar`, with a 10-second timeout:

| Event | When | Command |
|---|---|---|
| `SessionStart` | every session | `invar context show --binding`: prints `AI_CONTEXT.md` and `BUSINESS_CONTEXT.md` into the session |
| `SubagentStart` | the `coder` or `test-writer` agent starts | `invar context show --conventions --hook`: gives it `AI_CONTEXT.md` |
| `SubagentStop` | the `planner` agent stops | `invar plan validate --all-drafts --hook`: checks the form of draft Plans |
| `PreToolUse` | before `Write`, `Edit` and `Bash` | `invar plan guard`: denies an edit of an approved Plan or a locked test, and the commands that only the developer may run |

No hook approves a Plan, records a decision, unlocks a test or closes a cycle.

### Agents and skills

- **Agents:** `planner` (writes the draft Plan; the only one that reads the
  decision log), `test-writer` (writes and locks the acceptance tests), `coder`
  (implements the approved Plan) and `reviewer` (an advisory report; changes
  nothing).
- **Skill:** `invar`, the cycle the main session follows.
- **Slash commands for the developer:** `/invar:approve`, `/invar:relock`,
  `/invar:log-approve` and `/invar:done` record a person's decision: approving a
  Plan, unlocking a wrong test, recording a decision entry, closing the cycle.
  `/invar:review` and `/invar:log` start the review and draft a decision entry.
  All six have model invocation disabled, so only you can run them, and the
  `PreToolUse` hook denies the model the commands behind the four decisions
  (`invar plan approve`, `invar acceptance unlock`, `invar decision approve`,
  `invar decision record`, `invar done`) when it tries them through `Bash`.
  `/invar:review` and `/invar:log` begin with a short shell snippet that finds
  the ticket's Plan under `.invar/plans/`; it only reads and prints.

### Files it writes or changes in your project

`invar init`, run once, in the project root:

- creates `invar.yaml` (the configuration, yours from then on: Invar never
  writes it again), `.invar/plans/` and `.invar/decisions/`;
- creates `AI_CONTEXT.md` and `BUSINESS_CONTEXT.md` with section headings only,
  if they do not exist. Invar never writes their content;
- adds one line to `.gitattributes`, `.invar/plans/**/*.md text eol=lf`, so a
  Plan's checksum does not depend on line endings;
- adds one line to `.gitignore`, `.invar/out/`;
- adds a permission rule to `.claude/settings.json`: **`Read(/.invar/decisions/**)`
  under `permissions.deny`**. It stops the main session and the agents from
  reading the decision log with the Read tool; the planner receives the log
  through `invar context show` instead.

These three files are merged into, never overwritten: everything already in them
stays. A second run changes nothing.

During a cycle:

- the planner writes the draft Plan, `.invar/plans/<TICKET>-<slug>/spec.md`, and
  `/invar:approve` writes the approval into its header (your `git config
  user.name`, the date, the checksum);
- the test-writer writes test files under `acceptance.dir`, and
  `invar acceptance lock` writes an `invar-lock:` line with a digest into each;
- the coder changes the application code inside the Plan's Scope;
- `/invar:relock` records the unlock in `.invar/plans/<TICKET>-<slug>/unlocked`;
- `/invar:log` drafts `.invar/plans/<TICKET>-<slug>/decision.md`, and
  `/invar:log-approve` moves it into `.invar/decisions/`;
- `invar pr-body --out <path>` writes the PR description to that file (the
  cycle uses `.invar/out/`); without `--out` it only prints;
- `/invar:done` deletes the Plan directory.

### Git commands

Invar runs `git` as an external program. To read: `rev-parse`, `config
user.name`, `var GIT_COMMITTER_IDENT`, `status`, `symbolic-ref`, `ls-files`,
`ls-tree`, `cat-file`, `diff` and `merge-base`. It changes the repository only
in `invar done` (`/invar:done`): it stages the deletion of the Plan directory
with `git rm -r --cached`, and commits that deletion alone, with
`git commit --only`, only when you pass `--commit <message>` or `invar.yaml` sets
`done.commit: true`. It never pushes, fetches or rewrites history, and never
bypasses your git hooks or signing.

### Your test command

If you turn on acceptance tests, `invar acceptance lock` runs the command your
team configures as `acceptance.run` in `invar.yaml`, from the project root,
without a shell, for at most 10 minutes, with the test files and a temporary
JUnit report path. Without that section, Invar runs no command of yours. The
coder agent also runs your project's own checks through `Bash`, as any Claude
Code session would, under your permission settings.

### Network, models, telemetry

The `invar` binary makes no network requests, calls no model and sends no
telemetry. Everything it decides, it decides from the files in your project and
from `git`.

## About the binaries

`bin/` holds static Rust builds of the `invar` CLI, one per platform
(`invar-linux-x86_64`, `invar-darwin-arm64`, `invar-darwin-x86_64`), and
`bin/invar`, a short shell launcher that picks the one for your platform and runs
it with the arguments unchanged. The source is on the `main` branch of
[webzards-org/invar](https://github.com/webzards-org/invar), in `core/`. The
binaries are built by `.github/workflows/release.yml` on GitHub's runners, from
the tagged commit, with no build cache, and each carries a signed build
provenance attestation. Check an installed binary with
`gh attestation verify <file> --repo webzards-org/invar`; see
[Verifying a binary](https://github.com/webzards-org/invar/blob/main/docs/INSTALL.md#verifying-a-binary).

## License

MIT OR Apache-2.0, at your option: `LICENSE-MIT` and `LICENSE-APACHE` in this
folder.
