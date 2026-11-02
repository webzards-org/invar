# docs/

Project documentation.

- [`INSTALL.md`](INSTALL.md) — installing the plugin in a project, the optional
  terminal CLI, troubleshooting, and publishing a release.
- [`USAGE.md`](USAGE.md) — the cycle, step by step: every command the developer
  runs, and what each agent does.
- [`DESIGN.md`](DESIGN.md) — the design document (currently revision v3.1). All
  section references in this repository (§4d, §13a, …) point into it.
- [`TASKS-M1.md`](TASKS-M1.md) — the M1 milestone broken into issues, with the
  decisions each one must close.
- [`TASKS-AT.md`](TASKS-AT.md) — the test-writer feature broken into issues.
  Until v4, its decisions override DESIGN v3.1 and L-3 in `TASKS-M1.md`.

Both task files are closed records: they keep the decisions (L-1 … L-9,
A-1 … A-8) that other files cite. New work is tracked in
[GitHub issues](https://github.com/webzards-org/invar/issues), not in a new
task file.

## Belongs here

Documents about the system as a whole: the design document and architecture
decisions. Task breakdowns and the roadmap live in GitHub issues.

## Must not

- **No component documentation.** Each component's `README.md` lives with the
  component, so it is read and updated together with its code.
- **No specifications other components depend on.** Anything the core, an
  adapter, the plugin or the MCP server must agree on is a contract and belongs
  in `contracts/` (§14.3). This directory explains; it does not define.

## Changing the design document

A behavior change lands in `DESIGN.md` in the same PR that changes the
behavior. When a decision is reversed, the revision note at the top records what
changed and why — the document's history is part of its value.
