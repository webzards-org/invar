# On-disk format specifications (§14.3)

Prose specifications, not code. The core implements them; the Python adapter,
the MCP server and any later reimplementation read them. They are public
contracts: changing one is a deliberate, versioned PR (see `../README.md`).

| File | Specifies | Implemented by |
|---|---|---|
| `feature-metadata.md` | The `.feature` metadata block and header/body boundary (§4) | M1-3 |
| `plan-header.md` | The Plan YAML header and required sections (§4a, §5c) | M1-4 |
| `invar-yaml.md` | `invar.yaml`: the keys M1 reads, the §6 keys it ignores, value rules (§6) | M1-3 |

Still to be written: the decision-entry format (§7, M3).

## Why the boundary rules carry so much weight

Checksums are computed over the body alone (§4d). A parser that splits one byte
differently from another implementation produces a different digest for the
same file, and every integrity check then fails for a reason the developer
cannot see. Both specifications therefore state the boundary as a rule that can
be implemented without judgement, and name the edge cases explicitly: blank
lines around the boundary, comments before the tag, an indented `Feature:`
line, a `---` appearing inside a Plan's body.

## Open questions

Both files carry an `Open questions` section. Two are live and must be settled
before the milestone that writes the fields:

- **Date formats.** `Confirmed-at` is a date, `Approved-at` a timestamp.
  Decide before M1-9 / M1-10.
- **Re-approving a changed Plan.** Refuse, or re-approve with a new checksum?
  Decide in M1-9.

Resolving one means editing the specification, bumping its version and
recording the reason — the same as any other contract change.
