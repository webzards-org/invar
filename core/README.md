# core/

The `invar` CLI, written in Rust. Holds all language-neutral logic and is the
only component that renders a verdict (§8, §14.2).

Runnable on its own, which is why the full loop also runs in CI where no agent
and no plugin exist — the standing proof that no logic leaked into the plugin
(§13b).

## Belongs here

- The gate: `plan validate/approve`, `scenario validate/confirm`, and the
  refusals they enforce (§3, §4a)
- Canonicalization and checksums (§4d)
- Parsing of the on-disk formats: `.feature` metadata, Plan header,
  `invar.yaml`, decision entries — against the specifications in `contracts/`
- The decision log, `clean-spec`, `context show` assembly
- Starting language adapters as processes and exchanging JSON over stdio

## Must not

- **No language-specific logic.** Dependency discovery, test discovery, test
  execution and lint are adapter operations (§14.2). If the code knows what a
  `uv.lock` is, or how pytest-bdd binds a feature, it is in the wrong crate.
- **No agent- or plugin-specific behavior.** The core does not know Claude Code
  exists. It is called by the plugin, by the MCP server, by CI and by a
  developer in a terminal, and behaves identically for all four.
- **No LLM calls, ever** (§11). The developer's agent does the thinking; the
  CLI does the judging.

## Shape

Deliberately plain: reads and writes files, parses YAML headers and markdown
sections, hashes, and shells out to `git` rather than linking a git library.
No async runtime. Dependencies stay few and boring.

## Working on it

```bash
cargo fmt --check
cargo clippy -- -D warnings
cargo test
```

All three must pass before a PR.
