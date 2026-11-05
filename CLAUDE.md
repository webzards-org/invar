# Working on Invar

Instructions for an agent working **on this repository**. Users of the plugin
never see this file: the plugin ships no `CLAUDE.md` (see `plugin/README.md`).
What Invar is and how it is used is in [`README.md`](README.md) and
[`docs/USAGE.md`](docs/USAGE.md).

## What Invar is

A Claude Code plugin that keeps the specification fixed while an AI agent
writes the code against it. A developer approves a Plan with numbered
acceptance criteria, a checksum freezes it, and the agent that writes the code
cannot move the target it is measured against. Around that sit locked
acceptance tests the coder cannot change, business context the planner must
respect, a decision log, and deterministic checks that render every verdict.

It is a method of work built on established practice (ATDD, separation of
duties, test-first, design by contract, decision records), not a prompt pack.
When a design question is open, ask: which practice does this follow, and does
the structure enforce it, or does it only ask the agent to behave?

## Principles that decide most questions

1. **The CLI is the system; the plugin is a shell.** Every check, verdict and
   refusal is made by the `invar` binary in `core/`. Hooks run one CLI call;
   agent prompts and skills contain no logic. If a feature seems to need logic
   in the plugin, it needs a CLI command instead.
2. **No LLM inside Invar.** Invar never calls a model and never needs the
   network to reach a verdict.
3. **Boundaries live on disk, not in prompts.** A rule checked against a file's
   digest is a boundary; a rule an agent is asked to follow is a convenience.
   Never move a boundary out of an on-disk check into a hook or a prompt.
4. **The on-disk format is the contract.** Everything components agree on is
   specified in `contracts/`. An absent key always means the previous behavior.
5. **Language-neutral core.** `core/` never parses PHP, Python or JS and knows
   no test framework. That belongs to an adapter or to the project's own
   tooling.
6. **Decisions that weaken a guarantee belong to a person.** Approving a Plan,
   skipping tests, unlocking a test, recording a decision and closing a cycle
   are developer-only commands. Never make an agent able to run them.

## Where to look, in order of authority

1. The code and `contracts/formats/` describe exact behavior.
2. `plugin/NOTES.md` records what was verified against Claude Code, and why
   the plugin is built the way it is. Read the relevant section before
   changing anything under `plugin/`.
3. `docs/DESIGN.md` explains how the system works and why, as built today.
   Section references in code and contracts (§4d, §13a, …) point into the
   previous revision, v3.1, at
   https://github.com/webzards-org/invar/blob/c29122d/docs/DESIGN.md. Parts of
   v3.1 describe what is not built (FastAPI, `.feature` files,
   `invar generate`, language adapters, lint); where it disagrees with the
   code or a contract, the code and the contract win.

Older comments and contracts cite decision ids such as `L-3` or `A-2` from
task files that are no longer in the repository. Treat the contract or code
that cites them as authoritative; do not recreate the task files.

## Repository map

```
contracts/   public formats (plan-header, invar-yaml, decision-entry,
             acceptance-lock, test-results, feature-metadata) and fixtures
core/        Rust CLI: every check and verdict
plugin/      hooks, agents, skills, launcher; NOTES.md is the engineering log
eval/        seeded branches for checking the reviewer
docs/        user documentation (INSTALL, USAGE) and DESIGN
adapters/, mcp/, conformance/, e2e/   placeholders for later work
```

Components never import each other; they meet only in `contracts/`.

## Rules for a change

**Before starting**
- Work starts from a GitHub issue. For a non-trivial change, ask for one or
  open one, and link the PR to it (`Closes #123`). Do not create task files
  in the repository.
- Scope, a contract change, anything irreversible, and anything not already
  decided are the developer's call. Ask; do not assume.

**Contracts** (`contracts/`)
- A change is a public contract change: state what changed and why, bump the
  contract's version, record it in its version history, keep "absent means
  previous behavior", and update every reader in the same PR.
- Regenerating a fixture's expected value is a contract change too.

**Core** (`core/`)
- `cargo fmt --check`, `cargo clippy -- -D warnings` and `cargo test` must
  pass. CI builds with `--locked` and treats warnings as errors.
- The toolchain is pinned in `core/rust-toolchain.toml`; do not bump it as a
  side effect.
- Keep it plain: files, markdown sections, hashing, `git` as an external
  command. No async runtime, few dependencies.

**Plugin** (`plugin/`)
- No logic, no binaries in git, no hook that runs a developer-only command.
  `core/tests/plugin.rs` and `core/tests/plugin_prompts.rs` check this and
  the prompts: every command and flag an agent prompt names must exist, and
  agent models are aliases (`opus`, `sonnet`, `haiku`, `fable`, `inherit`).
- Whether a prompt works is only learned by running it on real tickets;
  record what changed and why in `plugin/NOTES.md`.

**Git and PRs**
- Branch from `main` as `<type>/<task-id>-<slug>`, types `feat`, `fix`,
  `chore`, `docs`, `test`, `ci`, `spike`. The task id is a whole segment of
  the name.
- Conventional commits with the component as scope: `feat(core): …`.
- Every commit needs a DCO sign-off, which only a person can give. An agent's
  commit has none: say so in the PR, and the developer signs the branch off
  (`git rebase --signoff origin/main`) before merging.
- User-visible changes get a `CHANGELOG.md` entry under Unreleased.
- A behavior change updates the user documentation in the same PR.
- Releases are the maintainers': only they tag `v*`.

**Writing**
- Repository, code and documentation are in English.
- Examples use the fictional ticket `SHOP-214`. Never name a real project,
  client or ticket.
- Never write content into a project's `AI_CONTEXT.md` or
  `BUSINESS_CONTEXT.md`: Invar scaffolds them, the team writes them.
- Keep internal planning out of the repository. Roadmap, priorities and
  ideas live in GitHub issues and the project board, not in committed files.
