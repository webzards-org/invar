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

```
core/src/
├── main.rs      # argument parsing and dispatch only
├── cli/         # one module per subcommand; arguments and `run`, no logic
├── format/      # .feature and Plan parsing (M1-3, M1-4)
├── digest/      # canonicalization and hashing (M1-1, M1-2)
└── fs/          # path resolution, project root discovery
```

The split between `digest/` and `format/` is deliberate: canonicalization takes
text and returns text, while deciding where a header ends and a body begins is
format knowledge. Fixtures in `contracts/fixtures/` test the first on its own,
which only works if the second never leaks into it (§4d).

## Errors

`anyhow` at the CLI boundary while the surface is still moving: subcommands
return `anyhow::Result<()>`, add context with `.context("…")`, and `main`
prints the full chain and exits non-zero. Failing loudly is a requirement, not
a convenience (§10).

Typed errors (`thiserror`) arrive once the surface stabilises — roughly when
the gate is complete and callers start needing to *distinguish* failures rather
than report them: a hook that treats "scenario not confirmed" differently from
"file not found", or the MCP server translating a failure into a structured
response. Until then a typed error model would be rewritten more often than it
is read.

## Toolchain

`rust-toolchain.toml` pins the compiler, so every machine and every CI run use
the same one. Bump it deliberately, in its own PR.

`Cargo.lock` **is committed**: this crate produces a binary, and the point of
the lockfile is that a build from a given commit resolves to the same
dependency versions everywhere — including in CI, which is the standing proof
that the loop runs without an agent (§13b).

## Working on it

```bash
cargo run -- --version     # build and run in one step
cargo build --release      # the binary the plugin ships
cargo test                 # unit tests plus the CLI tests in tests/
cargo fmt --check
cargo clippy -- -D warnings
```

The last three must pass before a PR.
