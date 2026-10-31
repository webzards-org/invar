# contracts/

The public contract. Every present and future surface — the CLI, the language
adapters, the plugin, the MCP server, the registry, any later reimplementation —
reads these definitions, and nothing bypasses them. This is the only place the
components meet (§14.3, §15).

## Belongs here

- `formats/` — the on-disk formats as written specifications: the `.feature`
  metadata block (§4), the Plan header and required sections (§4a), the
  decision-entry format (§7), `invar.yaml` (§6), the `invar-lock:` line of an
  acceptance test. Precise enough that two
  independent implementations split the same file at the same byte.
- `schemas/` — JSON Schema for the machine-readable contracts: the adapter
  protocol (§14.2), `invar.yaml`, decision entries.
- `fixtures/` — inputs and expected outputs every component tests against; the
  digest fixtures of §4d are the first set.

## Must not

- **No executable code.** Specifications, schemas and data only. A contract
  that is expressed as an implementation is not a contract — it is one
  implementation's behavior. One narrow exception is recorded in
  `fixtures/digest/README.md` (D-3): fixture bytes that no editor can preserve
  are materialized by a script, which produces and verifies data and is never
  a reference the core may defer to.
- **No component-specific content.** If only the Python adapter cares, it
  belongs in `adapters/python/`.
- **No silent change.** A PR touching this directory is a public-contract
  change: deliberate, versioned, with the reason recorded and every reader
  updated in the same PR (see `CONTRIBUTING.md`).

## Compatibility rule

New keys may be added at any time under one rule: **an absent key means the
previous behavior.** That is what has kept every addition so far non-breaking,
and what keeps the next ones non-breaking too.

## Fixtures

Fixtures test canonicalization, so they must be exempt from any repo-wide
formatting or whitespace tooling — a hook that "fixes" trailing whitespace in a
fixture destroys exactly what it verifies. `fixtures/digest/.gitattributes`
sets `* -text -diff` for that reason. Expected digests are regenerated
deliberately, never automatically.

| Set | Covers | Written for |
|---|---|---|
| `fixtures/digest/` | Canonicalization and digests (§4d) | M1-1, M1-2 |
| `fixtures/plan/` | A Plan as an earlier contract version approved it (`plan-header.md` D-16) | AT-2 |
| `fixtures/acceptance/` | The `invar-lock:` line and the digest of a locked file (`acceptance-lock.md`) | AT-5 |
