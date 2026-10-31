# On-disk format specifications (§14.3)

Prose specifications, not code. The core implements them; the Python adapter,
the MCP server and any later reimplementation read them. They are public
contracts: changing one is a deliberate, versioned PR (see `../README.md`).

| File | Version | Specifies | Implemented by |
|---|---|---|---|
| `invar-yaml.md` | 2.1.0 | `invar.yaml`: the keys M1 reads (including `done.commit`), the §6 keys it ignores, value rules (§6) | M1-3; `done.*` read by `invar done` |
| `plan-header.md` | 1.7.0 | The Plan header and required sections, per mode (§4a, §5c), the ids of the acceptance criteria, `## Acceptance tests to change`, the `Acceptance` field, and what `plan approve` writes | M1-6 (parsing), M1-7 (lookup), M1-8 (`plan validate`), M1-9 (`plan approve`), AT-2 (ids, the new section); read by M1-10 |
| `decision-entry.md` | 1.3.1 | A decision entry: filename, front matter, the four types, insert-only, who writes, and the `failed` reset (§7, §4c); and its draft in the Plan directory (§8.1) | M1-11 (`decision record`, `decision log`), `decision validate` (the draft); read by M1-12 to M1-14 |
| `feature-metadata.md` | 1.0.0 | The `.feature` metadata block and header/body boundary (§4) | The feature-tests milestone (deferred from M1, L-1) |

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

- **Acceptance criteria when `scenarios: true`** (`plan-header.md` OQ-2).
  Decide in the milestone that introduces `.feature` files.
- **A JSON Schema for `invar.yaml`** (`invar-yaml.md` OQ-1). Not needed by any
  M1 reader; no deadline.
- **Relations between decision entries** (`decision-entry.md` OQ-1), such as
  `supersedes:`, for the post-MVP decision graph. No deadline.

The former question on date formats is closed: `Approved-at` and
`Confirmed-at` are both `YYYY-MM-DD` (`plan-header.md` D-6,
`feature-metadata.md` D-6).
Re-approving a changed Plan is closed too: refused unless `--reapprove` is
given (`plan-header.md` D-12).

Resolving one means editing the specification, bumping its version and
recording the reason — the same as any other contract change.
