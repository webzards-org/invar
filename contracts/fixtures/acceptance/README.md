# Lock fixtures

The executable half of `contracts/formats/acceptance-lock.md`: files with and
without an `invar-lock:` line, and what each must read as. **AT-5** is written
against them, and any later reimplementation is checked against the same
expectations.

## Layout

```
acceptance/
├── locks.json            the contract: the expected result per file
├── .gitattributes        keeps git from rewriting a single byte
├── tools/generate.py     materializes and verifies the fixtures
├── none/                 no lock line
├── pending/              a ticket and no digest
├── locked/               a ticket and a digest
└── error/                every way the line can be wrong
```

## The contract

`locks.json` (schema `invar.lock-fixtures/v1`) holds one entry per file:

| `result` | Other fields |
|---|---|
| `none` | none |
| `pending` | `ticket`, `line`, and `digest`: the file's lock digest |
| `locked` | `ticket`, `line`, `digest`, `recorded`: the digest on the line, and `matches`: whether the two are equal |
| `error` | `kind` (`more-than-one`, `after-window`, `ticket`, `digest`) and `lines`; or `kind: invalid-utf8` and `offset` |

Every entry has `path`, `bytes` and `note`. `bytes` is recorded so that a corrupted
fixture is reported as a size difference, not as a mysterious wrong result.

`same_digest` lists groups of files that must produce one digest. Line endings, a
BOM, trailing whitespace, the comment prefix, trailing text and the state of the line
all leave the digest alone.

## Consuming them

Read `locks.json`, then read each file as **bytes**, never through a line-oriented
reader: one that normalizes terminators makes `php-crlf.php` and `php-lf.php`
indistinguishable, and the test passes without testing anything.

## Decisions

`tools/generate.py` is executable code inside `contracts/`, under the same narrow
exception as the digest set (`../digest/README.md`, D-3). **The contract is
`locks.json`**, and the core must satisfy it on its own.

The script has no lock-line parser on purpose. Its table states each result by hand,
and only the digests are computed, from the line the table names. A parser here would
be a reference implementation, and expectations taken from it would prove only that
two parsers agree.

## Adding a fixture

1. Add the file to the `FIXTURES` table in `tools/generate.py`, with its content in
   explicit escapes (`"alpha\r\nbeta\r\n"`) and its result stated by hand. Never paste
   a raw CR into the source. `{DIGEST}` in the content becomes the file's own digest.
2. Say in its note **what would break if it were absent**. A fixture whose absence
   changes nothing is noise.
3. Run:

   ```bash
   python3 tools/generate.py --write
   python3 tools/generate.py --update-expected
   python3 tools/generate.py --check
   ```

4. Commit the fixture, `locks.json` and the script together.

## Regenerating expectations

`--update-expected` is a separate, explicit mode. **An expectation that silently
follows a code change tests nothing.** If `cargo test` fails against `locks.json`, the
default assumption is that the implementation is wrong. Regenerating is legitimate
when a fixture was added or the contract changed; a change to the contract is a
public-contract change, with `version` bumped in `locks.json` and in
`FIXTURES_VERSION`.

## Exempt from formatting tooling

`.gitattributes` here sets `* -text -diff`. If a pre-commit hook, an editor's
format-on-save or a whitespace linter is ever added to this repository, **it must
exclude this directory**. `--check`, which runs in CI, is what would catch one that
does not.
