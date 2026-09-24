# adapters/python/

The Python language adapter: FastAPI projects, pytest-bdd tests, `uv.lock` or
`poetry.lock` or the installed distributions.

Implements the four operations of `adapters/README.md` and the protocol in
`contracts/schemas/`. Arrives in milestone M2.

## Belongs here

- Dependency discovery from the lockfile — never from the manifest. A manifest
  says `>=0.4`; a lockfile says `0.4.7`, and resolution starts from what is
  actually installed (§5b).
- Test discovery per §4b: the module-level `scenarios(...)` call and the
  per-scenario `@scenario(...)` decorator, matched as string literals against
  the feature path relative to the project root. A non-literal path argument is
  reported as such, never resolved.
- Test execution: pytest under `pipeline.tests_dir`, or a narrowed
  `--tests-path` when the core asks for one.
- Lint: the base rule set always; the extended rule set when a recognized
  `AI_CONTEXT.md` is present, including the inter-module import-graph rules
  (§6). Imports inside `if TYPE_CHECKING:` are ignored by default.

## Must not

- **No verdicts.** The adapter reports findings; the core decides what they
  mean. A lint violation is data on the way out of here.
- **No knowledge of the gate.** Plans, approvals, checksums and decision
  entries are the core's concern. This adapter does not know they exist.
- **No pytest-bdd assumptions leaking into the protocol.** The one-feature-to-
  one-test-file rule is a property of how pytest-bdd binds features, not of the
  contract; Behat, cucumber-js and godog answer the same question differently.
