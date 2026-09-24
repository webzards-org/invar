# conformance/

The suite every language adapter must pass. An adapter is complete when it
passes this suite, and the core needs to know nothing else about it (§14.2).

This is what makes the process boundary between core and adapters real rather
than described: the contract is testable on its own, without either side.

## Belongs here

- Fixture projects — small, realistic trees per language, with known
  dependencies, known test wiring and known lint violations
- Expected protocol exchanges: for a given request on stdin, the response the
  adapter must produce on stdout
- A runner that executes the suite against any adapter executable, chosen by
  path or by the `language:` key

## Must not

- **Nothing language-specific to one adapter.** A test that only the Python
  adapter could pass is a Python test and belongs in `adapters/python/`.
- **No core internals.** The suite exercises the protocol, not the gate. How
  the core uses an adapter's answer is tested in `core/`.
- **No dependency on a running registry or a network.** Conformance must pass
  offline.

## Adding a language

A new adapter starts by running this suite and failing it. When it passes, the
adapter is done — that is the whole acceptance criterion, and the reason a
second language is a packaging job rather than a second implementation.
