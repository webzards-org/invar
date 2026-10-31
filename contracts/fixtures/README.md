# Shared fixtures

Inputs and expected outputs that every component tests against. A fixture set
here is the same expectation for the Rust core, for a language adapter, for the
conformance suite and for any later reimplementation — which is what keeps
"they behave the same" a checkable claim rather than an intention.

| Set | Covers | Written for |
|---|---|---|
| `digest/` | Canonicalization and digests (§4d) | M1-1, M1-2 |
| `plan/` | A Plan as an earlier contract version approved it (`plan-header.md` D-16) | AT-2 |
| `acceptance/` | The `invar-lock:` line and the digest of a locked file (`acceptance-lock.md`) | AT-5 |

Each set carries its own README with the rules for adding to it. Two apply
everywhere:

- **Expected values are regenerated deliberately, never automatically.** An
  expectation that follows a code change tests nothing. When a test fails, the
  default assumption is that the implementation is wrong.
- **Fixtures are exempt from repo-wide formatting tooling.** Some of them
  exist precisely because their bytes are unusual; a hook that tidies them up
  destroys what they verify.
