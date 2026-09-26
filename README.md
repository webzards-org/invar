# Invar

[![CI](https://github.com/webzards-org/invar/actions/workflows/ci.yml/badge.svg)](https://github.com/webzards-org/invar/actions/workflows/ci.yml)

> A Claude Code plugin that brings a spec-driven, two-phase generate-and-verify
> loop to any FastAPI project.

An AI coding agent under pressure deforms the specification before it deforms
anything else: the acceptance criterion gets quietly relaxed until the code
satisfies it, and the result still looks correct. Invar holds the specification
fixed while the code is generated against it. One human-readable scenario is
both the input to generation and the criterion for verification — approved by a
person, then checksummed, so the party doing the implementing cannot move the
target it is being measured against.

**The full design document is [`docs/DESIGN.md`](docs/DESIGN.md).** Section
references throughout this repository (§4d, §13a, …) point into it.

## Status

Pre-implementation. Milestone **M0 — Foundations** is in progress; see the
roadmap in §16 of the design document.

## Layout

```
invar/
├── contracts/        # the public contract — nothing bypasses it
│   ├── schemas/      # JSON Schema: adapter protocol, invar.yaml, decision entry
│   ├── formats/      # .feature metadata block, Plan header and sections (§14.3)
│   └── fixtures/     # shared inputs/outputs every component tests against
├── core/             # Rust — the invar CLI
├── adapters/
│   └── python/       # Python — dependency/test discovery, pytest, lint
├── plugin/           # Claude Code plugin — .claude-plugin/, hooks/, agents/, skills/
├── mcp/              # MCP server — a client of the CLI and the registry (§14.1)
├── conformance/      # the suite every adapter must pass
├── e2e/              # the full loop on a sample FastAPI project, no agent
└── docs/             # the design document
```

Each directory has its own `README.md` stating what belongs in it and what must
not. Three rules hold across all of them (§15):

1. **Components never import each other.** The core starts adapters as
   processes; the plugin and the MCP server call the CLI. They meet only in
   `contracts/`.
2. **No logic in the plugin** (§13b). It fires CLI commands and passes output
   back. The standing proof is that the full loop runs in CI, where there is no
   plugin and no agent at all.
3. **A change to `contracts/` is a change to a public contract** (§14.3): made
   deliberately, with the reason recorded, never as a side effect of an
   implementation edit.

## Contributing

See [`CONTRIBUTING.md`](CONTRIBUTING.md) for branch naming, commit scope and
what a contract change requires.

## License

See [`LICENSE`](LICENSE).
