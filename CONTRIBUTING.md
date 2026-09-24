# Contributing

Section references (§4d, §13a, …) point into [`docs/DESIGN.md`](docs/DESIGN.md).

## Branch naming

```
<type>/<task-id>-<slug>
```

Examples: `chore/m0-1-monorepo-skeleton`, `feat/m1-2-digest-and-hash-command`,
`spike/m0-6-toy-plugin`.

Types: `feat`, `fix`, `chore`, `docs`, `test`, `ci`, `spike`.

**The task id must appear in the branch name as a substring.** That is how
`clean-spec --merged` maps a ticket to a merged branch (§12), by plain
substring matching against `git branch --merged`. Following the convention here
keeps this repository usable as the tool's own first test subject.

One branch per task. A task that turns out to contain two changes is two tasks.

## Where code goes

The layout is in the root [`README.md`](README.md) and in §15. Two rules decide
almost every placement question:

- **Components never import each other.** The core starts adapters as separate
  processes and exchanges JSON over stdio; the plugin and the MCP server call
  the CLI as a subprocess. If a change requires one component to link another,
  the design is wrong, not the rule.
- **No logic in the plugin** (§13b). The plugin fires commands and passes output
  back; it decides nothing. Behavior that lives in the plugin is behavior CI
  does not have, and the CI run is the standing proof that nothing leaked.

Language-specific work (parsing Python, reading `uv.lock`, running pytest)
belongs in an adapter, never in `core/`.

## Contract changes

A PR that touches `contracts/` is a **public-contract change** (§14.3). The
on-disk formats and the adapter protocol are read by the core, the adapters,
the plugin, the MCP server and any later reimplementation, so they change
deliberately or not at all.

Such a PR must:

1. Say in its description what changed and why.
2. Bump the version recorded in the affected contract file.
3. Respect the compatibility rule: **an absent key means the previous
   behavior.** New optional keys may be added at any time; removing or
   repurposing an existing one may not.
4. Update every component that reads the contract, in the same PR.

Regenerating a fixture's expected value is a contract change too. An expectation
that silently follows a code change tests nothing.

## Commits

Conventional-commit style, with the component as scope:

```
feat(core): canonicalize text before hashing (§4d)
docs(contracts): specify the .feature header/body boundary
```

## Before opening a PR

- `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test` pass for
  changes under `core/`
- Acceptance criteria of the linked issue are all ticked
- Anything that changed a documented behavior is reflected in
  `docs/DESIGN.md`, in the same PR
