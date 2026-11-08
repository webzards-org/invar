# plugin/

The Claude Code plugin — the delivery form of the system, and what the
developer installs (§13). One install, no manual wiring: the CLI binary, the
hooks and — from M1-18 — the planning sub-agent and the protocol skill arrive
together.

- [`NOTES.md`](NOTES.md) — what the M0-6 spike established about plugin
  mechanics (file layout, local installation, reload rules, debugging, what a
  hook receives and returns, how a `PreToolUse` hook denies), and in section 13
  the M1-17 decisions: the three hooks, how the binary ships, and what has not
  yet been run against Claude Code. Read it before changing anything here.

## What is here

```
plugin/
├── .claude-plugin/plugin.json   the manifest
├── .claude-plugin/icon.png      the plugin directory's listing icon (NOTES §13)
├── hooks/hooks.json             the hooks, each one `invar` call
├── agents/planner.md            Phase 1: a ticket becomes a draft Plan
├── skills/invar/SKILL.md        the cycle, as the main session lives it
├── skills/approve/SKILL.md      /invar:approve — the developer's gate
├── skills/relock/SKILL.md       /invar:relock — the developer unlocks a wrong test
├── skills/review/SKILL.md       /invar:review — the advisory review
├── skills/log/SKILL.md          /invar:log — drafts the decision entry
├── skills/log-approve/SKILL.md  /invar:log-approve — the developer records it
├── skills/done/SKILL.md         /invar:done — the developer closes the cycle
├── bin/invar                    the launcher: picks invar-<os>-<arch> and runs it
├── marketplace.json             what the `dist` branch lists (release.yml)
├── NOTES.md
├── README.dist.md               the user's README: `invar/README.md` on `dist`
└── README.md                    this file, for contributors
```

`bin/invar-<os>-<arch>` is not here: CI builds the binaries and publishes the
plugin with them on the `dist` branch (`.github/workflows/release.yml`). For
development, build the core and copy it beside the launcher (NOTES section 13).

## Belongs here

- `.claude-plugin/plugin.json` — the manifest. It lives inside
  `.claude-plugin/`, while every other directory sits at the plugin root.
- `hooks/` — hook definitions. Each one is a single call of `bin/invar`.
- `agents/` — the planning sub-agent that isolates Phase 1 (§13a).
- `skills/` — the protocol skill: longer explanations, loaded on demand (§5c).
  Both are prompts, and whether they work is only found out by running them on
  real tickets: NOTES section 14 is the protocol for that and the log of what
  changed.
- `README.dist.md` — the README users see: the release copies it to
  `invar/README.md` on `dist`, and the plugin directory shows it as the
  listing's description. It says what the plugin runs and changes in a
  project, so a change to a hook, an agent, a skill, `invar init` or the git
  commands the CLI runs updates it in the same PR.

## Must not

- **No logic.** The plugin fires CLI commands and passes output back. Nothing
  decides anything here (§13b). Behavior that lives in the plugin is behavior CI
  does not have — and CI is where the full loop has to run. The one script,
  `bin/invar`, only finds the binary; `core/tests/plugin.rs` fails if another
  script appears or if the launcher grows.
- **No gate.** `plan approve` and `done` are never hooked, and a test says so. A
  hook that approves a Plan has abolished the gate.
- **No binaries in git.** They are built by CI and published on `dist`. The one
  file that is neither text nor the launcher is the listing icon,
  `.claude-plugin/icon.png`, and a test allows that path only.
- **No `CLAUDE.md` at the plugin root.** Claude Code does not load one; plugins
  contribute context through skills, agents and hooks. What must always be in
  context is pushed by the `SessionStart` hook instead (§5c).
