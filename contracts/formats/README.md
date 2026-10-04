# On-disk format specifications (§14.3)

Prose specifications, not code. The core implements them; the Python adapter,
the MCP server and any later reimplementation read them. They are public
contracts: changing one is a deliberate, versioned PR (see `../README.md`).

| File | Version | Specifies | Implemented by |
|---|---|---|---|
| `invar-yaml.md` | 1.0.0 | `invar.yaml`: the keys M1 reads, the §6 keys it ignores, value rules (§6) | M1-3 |
| `plan-header.md` | 1.2.0 | The Plan header and required sections, per mode (§4a, §5c) | M1-6 (parsing), M1-7 (lookup); read by M1-8 to M1-10 |
| `feature-metadata.md` | 1.0.0 | The `.feature` metadata block and header/body boundary (§4) | The feature-tests milestone (deferred from M1, L-1) |

Still to be written: the decision-entry format (§7, M1-11).

## Why the boundary rules carry so much weight

Checksums are computed over the body alone (§4d). A parser that splits one byte
differently from another implementation produces a different digest for the
same file, and every integrity check then fails for a reason the developer
cannot see. The `.feature` and Plan specifications therefore state the boundary
as a rule that can be implemented without judgement, and name the edge cases
explicitly: blank lines around the boundary, comments before the tag, an
indented `Feature:` line, a `---` appearing inside a Plan's body.

## Open questions

Each file carries its own `Open questions` section. Live ones:

- **Re-approving a changed Plan** (`plan-header.md` OQ-1). Refuse, or
  re-approve with a new checksum? Decide in M1-9.
- **Acceptance criteria when `scenarios: true`** (`plan-header.md` OQ-2).
  Decide in the milestone that introduces `.feature` files.
- **A JSON Schema for `invar.yaml`** (`invar-yaml.md` OQ-1). Not needed by any
  M1 reader; no deadline.

The former question on date formats is closed: `Approved-at` and
`Confirmed-at` are both `YYYY-MM-DD` (`plan-header.md` D-6,
`feature-metadata.md` D-6).

Resolving one means editing the specification, bumping its version and
recording the reason — the same as any other contract change.
