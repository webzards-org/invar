# Contract: test results (JUnit XML)

**Version:** 1.0.0 — initial version; see §9
**Status:** stable
**Implements:** §14.2, §14.3; A-2, A-4, A-5 (`docs/TASKS-AT.md`)
**Read by:** the core (`format/junit`), any later reimplementation

> Changing this file is a **public-contract change** (§14.3). It is made
> deliberately, in its own PR, with the reason recorded, the version bumped,
> and every reader updated in the same PR.

---

## 1. Scope

The subset of JUnit XML the core relies on: for each test case, what its outcome was,
which acceptance criterion it covers, and which file it is in. The report is the result
of a run. **The exit code of the command that wrote it is never read** (A-2): a run with
a failing test exits non-zero by design, and the outcome of each test is in the report.

This contract defines how a report is read. Running `acceptance.run`, choosing the report
path (`invar-yaml.md` §3.6), reading lock lines and refusing anything belong to the
command that uses it (AT-8).

Fixtures, including real reports from Pest and PHPUnit: `contracts/fixtures/junit/`.

## 2. What is read

The report is UTF-8 and well-formed XML. A leading BOM is permitted and ignored. The root
element is `<testsuites>` or `<testsuite>`.

A **test case** is a `<testcase>` element whose every ancestor is a `<testsuites>` or a
`<testsuite>`, at any depth, because runners nest suites by configuration, class and data
set. Test cases are returned in document order.

Everything else is ignored: `<properties>`, `<system-out>`, timings, the counts on
`<testsuite>`, and any other element or attribute, along with everything inside an ignored
element. A `<testcase>` inside one is not a test case.

A report with no test case is valid and reads as an empty list. Runners write
`<testsuites/>` when they found no tests, and exit 0.

### 2.1 One test case

| From | Required | Meaning |
|---|---|---|
| `name` | Yes, not empty | The test's name, with XML's entities decoded |
| `file` | Yes, not empty | The file it is in, normalised by §3 |
| Direct children | No | The outcome, below |

A test case without `file` is an error naming it: without a file there is no lock line to
take the ticket from (§5).

**The outcome** comes from the test case's **direct** children:

| Children | Outcome | Means |
|---|---|---|
| None of these three | `passed` | |
| `<failure>` | `failure` | An assertion failed |
| `<error>` | `error` | An exception: a missing class, a missing route |
| `<skipped>` | `skipped` | Skipped, or marked incomplete or todo by the runner |
| More than one kind | The first of `error`, `failure`, `skipped` | D-5 |

Their attributes and text are not read. So any other child reads as `passed`, such as a
`<warning>`.

## 3. The path in `file`

Runners differ in what they write, and these are the only differences in the real reports
(`contracts/fixtures/junit/real/`):

| Runner | `file` holds |
|---|---|
| PHPUnit | `/project/tests/Acceptance/CancelOrderTest.php` |
| Pest | `tests/Acceptance/CancelOrderTest.php::it AC1 reserves stock` |

The reader takes the **project root** as an argument, and does no other I/O. In this order:

1. **Remove the suffix.** If the value ends with `::` followed by the test case's own
   `name`, that suffix is removed. Nothing else is removed, so a `::` that is not followed
   by exactly the name stays in the path (D-6).
2. **Separators.** Every `\` becomes `/`.
3. **Resolve.** Empty and `.` segments are dropped, and `..` removes the segment before it.
   This looks at the text only, and never at the file system.
4. **Make it relative.** A value starting with `/` or a drive (`C:/`) is absolute: its
   segments must start with those of the root, compared exactly, and those are removed. A
   relative value is already relative to the root.

The result is `/`-separated with no leading `./`. **It is an error** if the value is
absolute and not under the root, if a `..` climbs above the root, or if nothing remains. So
`/projectile/A.php` is not under `/project`.

The core compares text, so the root has to be the path the runner sees. A runner in a
container, or a path through a symlink, gives an absolute path outside the root, which is
reported and not guessed at.

## 4. The criterion, from the name

The id is the token `AC<n>` (A-5), found in `name`:

- `AC` in any case, then optionally one `_` or `-`, then digits with no leading zero: the
  first digit is `1` to `9`.
- A whole token: preceded by the start or by a character that is not a letter or digit,
  and followed by the end or by a character that is not a letter or digit. Letters and
  digits are Unicode's, and underscore counts as a boundary.

So `test_ac3_rejects`, `it AC-3: rejects` and `ac_3` give `AC3`. `ACME`, `vac3`, `AC3x`,
`AC03`, `AC0`, `AC__3` and `AC 3` give nothing.

The id is written `AC<n>` in capitals, whatever was found. The result is one of:

| Found | Result |
|---|---|
| No token | **None** |
| One token, once or more | **One**: that id. `AC3 and ac_3` is one token |
| Two or more different tokens | **Several**: all of them, in the order first found |

**None** and **several** are reported, not guessed. The reader refuses neither (D-4), and
what the caller refuses is the caller's decision (AT-8). The whole `name` is searched,
including what a runner adds to it, such as a data set's name.

## 5. The ticket is never read from the name

The ticket comes from the `invar-lock:` line of the file the test case names
(`acceptance-lock.md`). The reader returns the file, and the caller reads its lock line. A
name can be copied into any test, and a lock line cannot be without changing a locked file
(D-3).

## 6. Errors

Each is a distinct failure, and the first in the document is the one reported.

| Kind | When | The message says |
|---|---|---|
| **Missing** | There is no report. The caller finds this, since the reader takes bytes | The path it expected |
| **Empty** | The report has no bytes, or only whitespace or a BOM | That no report was written. Runners leave a 0-byte file when a test file cannot load |
| **Not XML** | Not UTF-8, or not well-formed: an element left open, a mismatched end tag, a second root, text outside the root, a malformed attribute, an entity other than XML's five and numeric ones in an attribute | What is wrong, and the line |
| **Wrong root** | The root is not `<testsuites>` or `<testsuite>` | The element found |
| **No name** | A test case with no `name`, or an empty one | The line |
| **No file** | A test case with no `file`, or an empty one | The test case's name, and the line |
| **Outside the root** | §3 | The test case's name, and the `file` value |

**No entity is ever resolved** from a DTD. A `<!DOCTYPE>` is skipped, so an external entity
is not fetched, and a reference to it in an attribute is **Not XML**. References in an
element's text are in what is not read, and stay unread.

## 7. Decisions and their reasons

**D-1 — JUnit XML is the only format, and no adapter is needed** (A-2). It is the one result
format that every relevant runner already writes, so a project only has to name a command.
An adapter per framework would put language-specific work back in the core, and would be
one more thing to install. The core runs a command it does not understand and reads a
language-neutral report.

**D-2 — `failure` and `error` stay apart** (A-4). Red before green means that a test fails on
its assertion before the code exists. A test that dies on a missing class or route has
tested nothing: it would stop failing once the class exists, whatever the behavior is. The
two elements are what tell these apart, so the reader never merges them. `skipped` stays
apart too, because a skipped test has neither passed nor failed.

**D-3 — The criterion comes from the name, and the ticket from the lock line** (A-5, A-3). The
criterion is a claim about a test, and the name is where a test can carry one that its
runner reports back. The ticket decides who owns a locked file, which a copied name must not
be able to change, so it comes from the lock line: changing that line in a locked file is
something the checks see (AT-9).

**D-4 — The token rule is exact, and a missing or doubled token is reported.** Test names are
free text, so any looser rule finds `AC` in `vacuum` or `ACME`. Accepting `_` and `-` covers
how languages spell a function name (`test_ac_3`), and the boundary is what makes both
`it('AC3: …')` and `test_ac3_rejects` work. No leading zero, because criteria are numbered
`AC1`, `AC2` (`plan-header.md` §6.4), and `AC03` would otherwise be a second spelling of one
id. Choosing among several tokens, or inventing one, would attach a test to a criterion
nobody said it covers, so the reader says what it found.

**D-5 — When a test case has several outcomes, `error` wins, then `failure`, then `skipped`.**
The worst one decides, so that nothing reads as a cleaner result than it had. In particular,
a test that failed on its assertion and then errored in teardown did not cleanly fail on an
assertion. No real report here has more than one, so this is pinned by a synthetic fixture.

**D-6 — The Pest suffix is removed only when it is exactly the test's own name.** Every Pest
report checked ends `file` with `::` and the decoded `name`, and that name can hold anything,
including `::`, so cutting at a `::` would be a guess. If a runner writes some other suffix,
the path stays as written and does not name a file, which the caller finds at once when it
reads the lock line.

**D-7 — Paths are compared as text, and the reader does no I/O.** A pure reader can be tested
on fixtures with a fixed root. Resolving symlinks would need the file system, and would hide
a runner that sees the project at another path. That is reported instead, and the caller
passes the root as the runner sees it.

## 8. Examples

The real reports in `contracts/fixtures/junit/real/` are the executable form of these,
checked against `results.json`.

### 8.1 Valid — PHPUnit 13

```xml
<?xml version="1.0" encoding="UTF-8"?>
<testsuites>
  <testsuite name="Tests\Acceptance\CancelOrderTest" file="/project/tests/Acceptance/CancelOrderTest.php" tests="2" assertions="2" errors="0" failures="1" skipped="0" time="0.001601">
    <testcase name="test_vacuum_and_acme_are_not_tokens" file="/project/tests/Acceptance/CancelOrderTest.php" line="11" class="Tests\Acceptance\CancelOrderTest" classname="Tests.Acceptance.CancelOrderTest" assertions="1" time="0.000181"/>
    <testcase name="test_ac1_warns_then_fails" file="/project/tests/Acceptance/CancelOrderTest.php" line="16" class="Tests\Acceptance\CancelOrderTest" classname="Tests.Acceptance.CancelOrderTest" assertions="1" time="0.001421">
      <failure type="PHPUnit\Framework\ExpectationFailedException">Failed asserting that false is true.</failure>
    </testcase>
  </testsuite>
</testsuites>
```

With the root `/project`, this reads as:

| `name` | `file` | Outcome | Criterion |
|---|---|---|---|
| `test_vacuum_and_acme_are_not_tokens` | `tests/Acceptance/CancelOrderTest.php` | `passed` | None |
| `test_ac1_warns_then_fails` | `tests/Acceptance/CancelOrderTest.php` | `failure` | `AC1` |

### 8.2 Valid — Pest 4, with the suffix and an entity

```xml
<testsuites>
  <testsuite name="Tests\Acceptance\CancelOrderTest" file="tests/Acceptance/CancelOrderTest.php">
    <testcase name="it AC1 ünïcödé &amp; ampersand" file="tests/Acceptance/CancelOrderTest.php::it AC1 ünïcödé &amp; ampersand">
      <failure type="PHPUnit\Framework\ExpectationFailedException">Failed asserting that true is false.</failure>
    </testcase>
  </testsuite>
</testsuites>
```

`it AC1 ünïcödé & ampersand`, in `tests/Acceptance/CancelOrderTest.php`, is a `failure` for
`AC1`.

### 8.3 Valid — `failure` is not `error`

```xml
<testsuite>
  <testcase name="it AC-3 errors on a missing class" file="tests/A.php">
    <error type="Error">Error: Class "App\DoesNotExist" not found</error>
  </testcase>
</testsuite>
```

An `error` for `AC3`, which AT-8 will not accept as red (D-2).

### 8.4 Valid — a run that found no tests

```xml
<?xml version="1.0" encoding="UTF-8"?>
<testsuites/>
```

An empty list. What it means for a ticket is the caller's decision.

### 8.5 Invalid — no `file`

```xml
<testsuite>
  <testcase name="AC1 has no file"/>
</testsuite>
```

**No file**, naming `AC1 has no file`, on line 2.

### 8.6 Invalid — the file is outside the root

```xml
<testsuite>
  <testcase name="AC1" file="/other/tests/A.php"/>
</testsuite>
```

**Outside the root**, with the root `/project`.

### 8.7 Invalid — an empty report

A 0-byte file is **Empty**. PHPUnit 13 and Pest 3 and 4 all leave one when a test file
cannot load, and exit non-zero.

### 8.8 Invalid — the wrong root, and not XML

```xml
<html><testcase name="AC1" file="tests/A.php"/></html>
```

**Wrong root**, naming `html`. A file holding `this is not xml` is **Not XML**.

## 9. Version history

**1.0.0** — initial version (AT-6).
