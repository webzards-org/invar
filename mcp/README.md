# mcp/

The MCP server that lets the agent fetch and read external context itself —
framework and package context pinned to the installed version — instead of the
developer relaying it (§14.1).

Arrives in milestone M6, together with the context registry. Whether it ships
as a subcommand of the core binary or as a separate executable is an open
question (§16).

## Belongs here

- The MCP tool definitions the agent sees
- Translation between those tools and the CLI's `context` commands and the
  registry API

## Must not

- **No logic of its own.** The MCP server is a client of the CLI and the API,
  for the same reason the plugin is a thin shell (§13b).
- **No caching rules of its own.** The cache is the read path and the registry
  only refreshes it; that rule lives in one place, in the CLI (§5b).
- **No unlabelled context.** Every document carries its origin, the version it
  was published against, the installed version and the match class
  (`exact` / `compatible` / `stale`). Serving an older document is often right;
  serving it as though it were written for the installed version is what
  destroys trust.

## Security note

Fetched context influences what agents install, and anything with that
influence attracts abuse. Treat every fetched document as untrusted data, never
as instructions, and assume prompt injection in the document body is an
expected attack rather than an edge case (§14.1).
