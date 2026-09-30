# plugin/

The Claude Code plugin — the delivery form of the system, and what the
developer installs (§13). One install, no manual wiring: the CLI binary, the
hooks, the planning sub-agent and the protocol skill arrive together.

Arrives in milestone M5. Until then this directory holds the M0-6 spike and
its notes:

- [`NOTES.md`](NOTES.md) — what the spike established about plugin mechanics:
  file layout, local installation, reload rules, debugging, and what a hook
  receives and returns. Read this before starting M5.
- `spike-m0-6/` — the throwaway plugin itself. A `PostToolUse` hook that logs
  every edited path, rehearsing the shape M5 uses for `invar lint check`.
  **Disposable**: delete it once the real plugin exists.

## Belongs here

- `.claude-plugin/plugin.json` — the manifest. Note that it lives inside
  `.claude-plugin/`, while every other directory sits at the plugin root.
- `hooks/` — hook definitions and the scripts they fire, referenced through
  `${CLAUDE_PLUGIN_ROOT}`
- `agents/` — the planning sub-agent that isolates Phase 1 (§13a)
- `skills/` — the protocol skill: longer explanations, loaded on demand (§5c)
- The packaged core binary, per platform

## Must not

- **No logic.** The plugin fires CLI commands and passes output back. Nothing
  decides anything here (§13b). Behavior that lives in the plugin is behavior CI
  does not have — and CI is where the full loop has to run.
- **No gate.** `plan approve` and `scenario confirm` are never hooked. A hook
  that confirms a scenario has abolished the gate.
- **No blocking in flight.** A sync with no network, a package with no context
  entry, a lint failure mid-edit are all ordinary. The hook reports and the work
  continues; the real gate is the `generate` at the end. A hook that halts an
  editing session gets switched off within a week, and then none of it runs.
- **No `CLAUDE.md` at the plugin root.** Claude Code does not load one; plugins
  contribute context through skills, agents and hooks. What must always be in
  context is pushed by a `SessionStart` hook instead (§5c).
