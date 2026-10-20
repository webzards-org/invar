# docs/

Project documentation.

- [`INSTALL.md`](INSTALL.md) — installing the plugin in a project, the optional
  terminal CLI, troubleshooting, and publishing a release.
- [`DESIGN.md`](DESIGN.md) — the design document (currently revision v3.1). All
  section references in this repository (§4d, §13a, …) point into it.
- [`TASKS-M1.md`](TASKS-M1.md) — the M1 milestone broken into issues, with the
  decisions each one must close.

## Belongs here

Documents about the system as a whole: the design document, architecture
decisions, and later the roadmap and task breakdowns.

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
