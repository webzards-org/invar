# docs/

Project documentation.

- [`INSTALL.md`](INSTALL.md): installing the plugin in a project, the optional
  terminal CLI, troubleshooting, and publishing a release.
- [`USAGE.md`](USAGE.md): the cycle, step by step: every command the developer
  runs, and what each agent does.
- [`DESIGN.md`](DESIGN.md): how Invar works: its parts, the cycle in detail,
  and why it is built the way it is.

Section references in code, contracts and notes (§4d, §13a, …) point into the
previous design document, revision v3.1, kept in history at
[`docs/DESIGN.md` at c29122d](https://github.com/webzards-org/invar/blob/c29122d/docs/DESIGN.md).
New work is tracked in
[GitHub issues](https://github.com/webzards-org/invar/issues), not in files.

## Belongs here

Documents about the system as a whole, written for people who use or evaluate
Invar.

## Must not

- **No component documentation.** Each component's `README.md` lives with the
  component, so it is read and updated together with its code.
- **No specifications other components depend on.** Anything the core, the
  plugin or a later component must agree on is a contract and belongs in
  `contracts/`. This directory explains; it does not define.
- **No roadmap or internal planning.** That lives in issues and the project
  board.

## Changing these documents

A behavior change updates `USAGE.md`, and `DESIGN.md` where it affects how the
system works, in the same PR that changes the behavior.
