# Contributing

Section references (§4d, §13a, …) point into the previous design document,
v3.1, kept in history at
[`docs/DESIGN.md` at c29122d](https://github.com/webzards-org/invar/blob/c29122d/docs/DESIGN.md).
[`docs/DESIGN.md`](docs/DESIGN.md) explains how the system works today.

## Issues first

Work is tracked in [GitHub issues](https://github.com/webzards-org/invar/issues).
Open one before a non-trivial PR, so the change is agreed before it is written,
and link the PR to it (`Closes #123`). A typo fix needs no issue.

Security problems are the exception: report them privately, as
[`SECURITY.md`](SECURITY.md) says, never in an issue.

Everyone taking part follows the [code of conduct](CODE_OF_CONDUCT.md).

## Licensing and sign-off

Invar is licensed under either of MIT or Apache-2.0, at the user's option
([`LICENSE-MIT`](LICENSE-MIT), [`LICENSE-APACHE`](LICENSE-APACHE)). Unless you
explicitly state otherwise, any contribution you intentionally submit for
inclusion is licensed the same way, without any additional terms or conditions.

Every commit carries a sign-off, certifying the
[Developer Certificate of Origin](https://developercertificate.org/): that you
wrote the change or otherwise have the right to submit it under these licenses.

```bash
git commit -s -m "fix(core): …"
```

`-s` adds `Signed-off-by: Your Name <you@example.com>` from your git config. A PR
with an unsigned commit is not merged; `git rebase --signoff main` signs a branch
after the fact.

## Branch naming

```
<type>/<task-id>-<slug>
```

Examples: `chore/m0-1-monorepo-skeleton`, `feat/m1-2-digest-and-hash-command`,
`spike/m0-6-toy-plugin`.

Types: `feat`, `fix`, `chore`, `docs`, `test`, `ci`, `spike`.

**The task id must appear in the branch name, as a whole segment.** That is how
`invar done` without a ticket finds the Plan to close (§12): the ticket must be
a whole segment of the current branch's name. Following the convention here
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
- Every commit is signed off (`git commit -s`)
- Acceptance criteria of the linked issue are all ticked
- Anything that changed a documented behavior is reflected in
  `docs/USAGE.md` (and `docs/DESIGN.md` where it changes how the system
  works), in the same PR

## CI

`.github/workflows/ci.yml` runs on every push and pull request: format, build,
clippy and tests, against the compiler pinned in `core/rust-toolchain.toml`.
All four must pass.

Two conventions worth knowing:

- **`--locked`.** Build and test refuse to update `Cargo.lock`. A dependency
  bump is then a deliberate commit, never something CI does silently.
- **Warnings are errors.** `RUSTFLAGS: -D warnings` applies to every step, not
  only to clippy. A warning that CI tolerates is a warning nobody reads.

CI has no agent and no plugin, which is the point (§13b). From M3 the e2e suite
runs here too, and a change that only works inside Claude Code will fail here —
that is the mechanism working, not the pipeline being wrong.
