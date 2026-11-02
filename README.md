# Invar

[![CI](https://github.com/webzards-org/invar/actions/workflows/ci.yml/badge.svg)](https://github.com/webzards-org/invar/actions/workflows/ci.yml)

> A Claude Code plugin that keeps the specification fixed while an AI agent
> writes the code against it.

An AI coding agent under pressure deforms the specification before it deforms
anything else. The acceptance criterion gets quietly relaxed until the code
satisfies it, and the result still looks correct. Invar holds the
specification fixed: a person approves it, a checksum freezes it, and the agent
that writes the code cannot move the target it is measured against.

## How it works

Each ticket goes through one cycle:

1. **Plan.** A planner agent reads the ticket, your project's conventions and
   business rules, and the code. It writes a Plan with numbered acceptance
   criteria (`AC1`, `AC2`, …).
2. **Approve.** You read the Plan and approve it with `/invar:approve`. This
   is your only manual step. The approval records a checksum, so any later
   change to the Plan is visible.
3. **Lock the acceptance tests** (optional, per ticket). A test-writer agent
   turns each criterion into a test that fails before the code exists, and
   locks it. The coder cannot change a locked test.
4. **Code.** A coder agent implements the Plan and makes the tests pass. If
   the Plan cannot be implemented as approved, it says so instead of bending a
   criterion.
5. **Review and close.** An optional reviewer checks the branch against the
   Plan, every business rule and the locked tests. You record a decision
   entry if it is worth keeping, and `/invar:done` removes the Plan in the
   same PR.

All decisions are made by the `invar` CLI, a static binary inside the plugin.
The plugin only runs it. The same checks run in CI with no agent at all.

Invar does not depend on your language. It has been used so far on a
Laravel + Vue application with Pest, and anything that writes JUnit XML can
run the acceptance tests.

## Status

**0.1.0, the first release.** It has been beta-tested on one project. The
Plan gate, the context files, the decision log and the locked acceptance tests
are complete. See [`CHANGELOG.md`](CHANGELOG.md).

Supported platforms: Linux x86_64 and macOS (Apple silicon and Intel).

## Documentation

- [`docs/INSTALL.md`](docs/INSTALL.md): installing the plugin, setting up a
  project, acceptance tests, CI, troubleshooting.
- [`docs/USAGE.md`](docs/USAGE.md): the cycle, step by step, with every
  command you run.
- [`docs/DESIGN.md`](docs/DESIGN.md): the design document. Section references
  in this repository (§4d, §13a, …) point into it. Decisions made after
  revision v3.1 are in [`docs/TASKS-M1.md`](docs/TASKS-M1.md) (the Plan gate,
  light mode) and [`docs/TASKS-AT.md`](docs/TASKS-AT.md) (acceptance tests),
  and they win where they differ.
- [`contracts/`](contracts/): the file formats every component agrees on.

## Layout

```
invar/
├── contracts/        # the public contract: file formats and shared fixtures
├── core/             # Rust: the invar CLI, where every decision is made
├── plugin/           # Claude Code plugin: hooks, agents, skills, the launcher
├── eval/             # seeded branches for checking the reviewer by hand
├── adapters/         # language adapters (later milestones)
├── mcp/              # MCP server (later milestones)
├── conformance/      # the suite every adapter must pass (later milestones)
├── e2e/              # the full loop without an agent (later milestones)
└── docs/             # design, installation, usage, task breakdowns
```

Each directory has its own `README.md` saying what belongs in it. Three rules
hold across all of them (§15):

1. **Components never import each other.** The plugin and the MCP server call
   the CLI. They meet only in `contracts/`.
2. **No logic in the plugin** (§13b). It fires CLI commands and passes output
   back.
3. **A change to `contracts/` is a change to a public contract** (§14.3):
   made deliberately, with the reason recorded and the version bumped.

## Contributing

See [`CONTRIBUTING.md`](CONTRIBUTING.md) for branch naming, commit scope,
sign-off and what a contract change requires. Work is tracked in
[GitHub issues](https://github.com/webzards-org/invar/issues).

## Security

Please report vulnerabilities privately, as described in
[`SECURITY.md`](SECURITY.md), not in a public issue.

## License

Licensed under either of

- Apache License, Version 2.0 ([`LICENSE-APACHE`](LICENSE-APACHE))
- MIT license ([`LICENSE-MIT`](LICENSE-MIT))

at your option.

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in this project by you, as defined in the Apache-2.0
license, shall be dual licensed as above, without any additional terms or
conditions.
