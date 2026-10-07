# Digest fixtures (§4d)

The executable half of §4d, and the definition of correctness that **M1-1**
(canonicalization) and **M1-2** (digest) are written against.

Canonicalization exists for a concrete reason: without line-ending
normalisation and trailing-whitespace stripping, a mixed Windows/Unix team — or
simply `core.autocrlf=true` — produces different digests for identical content,
and every integrity check fails for a reason the developer cannot see. These
fixtures are what stops that from being discovered in production.

Because they live in `contracts/`, any later reimplementation — another
adapter, a rewritten core — is verified against exactly the same expectations.

## Layout

```
digest/
├── digests.json          the contract: expected digest per group
├── .gitattributes        keeps git from rewriting a single byte
├── tools/generate.py     materializes and verifies the fixtures
├── simple/               one content, nine encodings
├── empty/                everything that canonicalizes to nothing
├── interior-blank-lines/ blank lines that are content
├── leading-blank-lines/  the asymmetry: leading kept, trailing stripped
├── indentation/          leading whitespace kept, trailing stripped
├── non-ascii/            multi-byte UTF-8
└── feature-body/         a realistic .feature body
```

## The contract

`digests.json` (schema `invar.digest-fixtures/v1`) holds one entry per group:

```json
{
  "name": "simple",
  "expected_digest": "sha256:e49c81e2…",
  "canonical_byte_length": 11,
  "files": [{ "path": "simple/crlf.txt", "bytes": 13, "note": "CRLF terminators" }]
}
```

Two invariants, both checked by `tools/generate.py --check`:

1. **Every file in a group produces that group's `expected_digest`.** This is
   the equivalence claim: the files differ in encoding, never in content.
2. **No two groups share a digest.** This is the distinctness claim, and it is
   the half that is easy to forget. An implementation that strips *all* blank
   lines, or that trims leading whitespace too, would satisfy (1) and collapse
   `simple`, `interior-blank-lines` and `leading-blank-lines` into one digest.

`bytes` per file is recorded so a corrupted fixture is reported as "on disk 11
bytes, table says 13" rather than as a mysterious digest mismatch.

## Consuming them

**From Rust** (M1-1, M1-2): read `digests.json`, then for every group read each
file, canonicalize, digest and compare against `expected_digest`. Read the
files as **bytes**, never through a line-oriented reader — one that normalizes
terminators for you makes `crlf.txt` and `lf.txt` indistinguishable and the
test passes without testing anything.

**From Python** (a future adapter, or the conformance suite): same shape.
`json` plus `hashlib` and nothing else.

## Decisions

**D-1 — An empty result is the empty string, not `"\n"`.** §4d step 4 says
"strip trailing blank lines, then append exactly one `\n`", which taken
literally turns an empty file into a one-byte file. The `empty` group fixes the
other reading: when nothing remains, nothing is appended. Consequences: the
expected digest is the well-known SHA-256 of zero bytes
(`e3b0c442…b855`), which makes the group double as a self-check — an
implementation that gets a different value there has a bug in its hashing, not
in its canonicalization. An empty `.feature` body is invalid anyway (§4), so
nothing downstream depends on the other reading. §4d step 4 now states the
rule itself ("unless nothing is left"), so this is no longer a deviation from
the text but its wording.

**D-2 — Trailing whitespace is every code point with the Unicode `White_Space`
property, and nothing else.** §4d now says so explicitly. Rust's
`str::trim_end()` implements exactly that definition. Python's bare
`str.rstrip()` does *not*: it also strips the ASCII control characters
`U+001C`–`U+001F`, so `tools/generate.py` strips an explicit `White_Space` set
instead (`UNICODE_WHITE_SPACE`). Not whitespace, and therefore kept: `U+FEFF`,
`U+200B`–`U+200D`, `U+2060`, `U+180E`, `U+001C`–`U+001F`. Only `\n` ends a line
once step 2 has run, so `U+2028` inside a line is content, and at the end of
one it is stripped like any other whitespace. The boundary is pinned by unit
tests in `core/src/digest/tests.rs`, not by fixture files: a fixture of exotic
whitespace would be destroyed by the same tooling the other fixtures resist.
Any other implementation must use the same definition.

**D-3 — `tools/generate.py` is executable code inside `contracts/`, which
`contracts/README.md` otherwise forbids.** The exception is deliberate and
narrow: these bytes cannot be maintained by hand (every editor destroys them),
so the table in that script is the only honest way to keep them reviewable in a
diff. The script produces and checks data; it is not a reference
implementation the core may defer to. The **contract is `digests.json`**, and
the core must satisfy it on its own.

## Adding a fixture

1. Add the file to the `GROUPS` table in `tools/generate.py`, with content
   written using explicit escapes (`"alpha\r\nbeta\r\n"`). Never paste a raw CR
   into the source — whatever you use to edit it will eat it.
2. If it belongs to an existing group, it must canonicalize to that group's
   content. `--check` fails loudly if it does not.
3. If it is a new group, write a description saying **what would break if this
   group were absent**. A fixture whose absence changes nothing is noise.
4. Run:

   ```bash
   python3 tools/generate.py --write
   python3 tools/generate.py --update-digests   # only if a group is new or changed
   python3 tools/generate.py --check
   ```

5. Commit the fixture, `digests.json` and the script together.

## Regenerating digests

`--update-digests` is a separate, explicit mode, and that separation is the
point. **A regenerated expectation that silently follows a code change tests
nothing.** If `cargo test` fails against these digests, the default assumption
is that the implementation is wrong, not the fixture.

Regenerating is legitimate in exactly two cases: a new group was added, or §4d
itself changed. The second is a **public-contract change** (§14.3): its own PR,
the reason recorded, `version` bumped in both `digests.json` and the
`FIXTURES_VERSION` constant, and every reader updated in the same PR.

## Exempt from formatting tooling

`.gitattributes` in this directory sets `* -text -diff`, so git performs no
end-of-line conversion and treats the fixtures as binary for diffing.

If a pre-commit hook, an editor-format-on-save, or a whitespace linter is ever
added to this repository, **it must exclude this directory**. A hook that
strips trailing whitespace here destroys exactly what these files test, and it
does so silently — `--check` is what would catch it, which is why it runs in
CI.

## Not covered here

**Invalid UTF-8.** §4d step 1 says decode as UTF-8; a file that is not valid
UTF-8 must produce a clear error rather than a panic or a lossy replacement.
That belongs in the core's own unit tests: it is a behavior of one
implementation's error handling, not a shared expectation about a digest, and a
fixture of invalid bytes cannot be committed through ordinary tooling anyway.
