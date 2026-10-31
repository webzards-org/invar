# JUnit fixtures

The executable half of `contracts/formats/test-results.md`. **AT-6** is written against
them, and any later reimplementation is checked against the same expectations.

A reader tested only on hand-written XML breaks on the first real report, so `real/` holds
what the runners actually wrote.

## Layout

```
junit/
├── results.json     the contract: the expected result per report, by hand
├── sums.json        SHA-256 of every report, so that nothing rewrites one unnoticed
├── .gitattributes   keeps git from rewriting a single byte
├── tools/check.py   verifies all of it
├── real/            reports written by Pest and PHPUnit, unedited
└── synthetic/       edge cases written by hand
```

## `real/`: how the reports were produced

| Report | Tool |
|---|---|
| `phpunit-13.4.5*.xml` | PHPUnit 13.4.5 |
| `pest-4.7.8*.xml` | Pest 4.7.8, on PHPUnit 12.5.33 |
| `pest-3.8.7*.xml` | Pest 3.8.7, on PHPUnit 11.5.56 |

All ran on PHP 8.5.12 with Composer 2.11.1, in the official image
`composer@sha256:4ed71c38c6d788eb4e4e3747b5332f5b40d5b5a6fcadd5062a1a70986a118e0a`. PHP and
Composer were not installed on the machine, and a container gives the same real tools. Pest 2
did not install on this PHP. The projects were plain PHP with throwaway tests, outside this
repository, and only the reports are checked in.

**The reports are unedited.** Each project was mounted at `/project`, so that is the path the
runners wrote, and no machine path had to be replaced:

```bash
docker run --rm -v "$PWD:/project" -v "$OUT:/out" -w /project composer:2 sh -c '<command>'
```

| Suffix | Command | Exit code |
|---|---|---|
| none | `vendor/bin/phpunit --log-junit /out/phpunit-13.4.5.xml` | 2 |
| none | `vendor/bin/pest --log-junit /out/pest-<version>.xml tests/Acceptance` | 2 |
| `-one-file` | The same, with `--log-junit` to this file and `tests/Acceptance/CancelOrderTest.php` | 1 |
| `-load-failure` | The same, with a test file whose class cannot load | 255 for PHPUnit, 1 for Pest |
| `-no-tests` | The same, with an empty directory | 0 |

What they show, and the contract relies on:

- PHPUnit writes an absolute `file`. Pest writes a relative one with `::<test name>` appended.
- A test file that cannot load leaves a **0-byte report** with all three, so `Empty` is a real
  case and not a theoretical one. No test case is reported for that file.
- A run that finds no tests writes `<testsuites/>` and exits 0.
- A data set nests one more `<testsuite>`, and its name becomes part of each test case's name.
- `markTestIncomplete`, `->todo()` and `->skip()` all write `<skipped/>`. A PHPUnit test with no
  assertions is reported as a plain pass.
- The Pest 3 and Pest 4 reports are the same except for timings.

## The contract

`results.json` was **written by hand**, never from a reader's output. One entry per report:

| Field | Meaning |
|---|---|
| `report` | The path, under this directory |
| `root` | The project root to read it with; always `/project` |
| `cases` | The expected test cases, in order: `name`, `file`, `outcome`, `criterion` |
| `same_cases_as` | In place of `cases`: another report with the same expected cases |
| `error` | In place of `cases`: the expected failure, `kind`, and `name`, `line` where they apply |

`criterion` is `null` for none, a string for one id, and a list for several. Kinds are `empty`,
`not-xml`, `wrong-root`, `no-name`, `no-file` and `outside-root`, as in contract §6. **Missing**
has no fixture: the reader takes bytes, so that one belongs to the caller.

## `tools/check.py`

`--check`, the default, runs in CI, and verifies that:

1. every file under `real/` and `synthetic/` is in `results.json` and `sums.json`, and has its
   recorded digest, which is what catches an editor or a git filter having "fixed" a report;
2. no report holds a machine path;
3. for every report with `cases`, the standard library's XML parser finds the same names, in the
   same order, with the same outcomes.

So the hand-written table is checked twice, here and by the core, by parsers that share no code.
The script has no opinion on tokens or paths. Those are checked by the core's test alone.

`tools/check.py` is executable code inside `contracts/`, under the same narrow exception as the
digest set (`../digest/README.md`, D-3). **The contract is `results.json`.**

## Adding a report

1. For `real/`: produce it with the real tool, and change nothing in it. Add the tool's version
   and the command above. For `synthetic/`: write it by hand, and say in its name what it is for.
2. Add its expected result to `results.json`, by hand.
3. Run `python3 tools/check.py --update-sums`, then `python3 tools/check.py --check`.

`--update-sums` is a separate, explicit mode. **An expectation that silently follows a code
change tests nothing.** If `cargo test` fails against `results.json`, the default assumption is
that the implementation is wrong.

## Exempt from formatting tooling

`.gitattributes` here sets `* -text -diff`. If a pre-commit hook, an editor's format-on-save or a
whitespace linter is ever added to this repository, **it must exclude this directory**.
