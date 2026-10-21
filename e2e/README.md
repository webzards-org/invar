# e2e/

The full loop on a sample FastAPI project, **with no agent and no plugin
present**.

This is not a convenience suite. §10 makes it an acceptance criterion and §13b
makes it the standing proof that no logic leaked out of the CLI: CI has no
agent at all, so anything that only works with the plugin loaded is behavior CI
does not have.

## Belongs here

- A sample FastAPI project with `specs/`, `invar.yaml`, a marked
  `AI_CONTEXT.md` and `BUSINESS_CONTEXT.md`, and pytest-bdd tests
- Scripted runs of the real commands in sequence: `init`, `plan validate`,
  `plan approve`, `scenario confirm`, `generate`, `done`
- The failure paths, which matter as much as the happy one: a stale checksum, an
  unapproved Plan, a `failed` entry resetting the cycle, `done` refusing while an
  unapproved draft entry is beside the Plan

## Must not

- **No agent, no plugin, no LLM.** The moment a test needs one, it is not this
  suite.
- **No mocking of the CLI.** These tests run the real binary against a real
  project tree. Unit-level behavior belongs in `core/`.
- **No network dependency.** `context sync` is non-blocking in every failure
  mode, and this suite is where that gets proven.

## What "green" means

A green run means a developer could have produced the same result by hand, in a
terminal, with no agent in the room — and that the gate refused everything it
promises to refuse.
