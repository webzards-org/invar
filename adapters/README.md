# adapters/

Language adapters. Each one is a **separate executable** implementing the four
language-specific operations behind a JSON protocol over stdin/stdout
(§14.2):

| Operation | Answers |
|---|---|
| Dependency discovery | Which packages are installed, at which versions (feeds `context sync` and `Lockfile-checksum`) |
| Test discovery | Is this scenario wired to an executable test, and how is it run? |
| Test execution | Run the tests; report the result |
| Lint | Base and extended rule sets for this language |

An adapter is written in the language of its ecosystem — Python's `ast` for
Python, `nikic/php-parser` for PHP, `go/ast` for Go — because each analysis is
best done from inside the ecosystem it analyses.

## Belongs here

One directory per language. `python/` is the only one in MVP.

## Must not

- **No linking, in either direction.** The core starts an adapter as a process
  and talks JSON to it. Neither imports the other.
- **No language-neutral logic.** The gate, checksums, the Plan, the decision
  log and `clean-spec` live in `core/`. An adapter that decides whether a
  scenario may be confirmed has taken over a verdict that is not its to render.
- **No protocol definitions.** The message shapes are JSON Schemas in
  `contracts/schemas/`; an adapter implements them, it does not define them.

## Done means conformance

An adapter is complete when it passes the suite in `conformance/`. The core
needs to know nothing else about it — which is what makes a second language a
new executable rather than a change to the core.
