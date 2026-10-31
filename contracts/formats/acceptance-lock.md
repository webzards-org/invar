# Contract: the `invar-lock:` line

**Version:** 1.0.0 — initial version; see §9
**Status:** stable
**Implements:** §4d (Checksums); A-3, A-5 (`docs/TASKS-AT.md`)
**Read by:** the core (`format/lock`; `invar hash --lock`), any later
reimplementation

> Changing this file is a **public-contract change** (§14.3). It is made
> deliberately, in its own PR, with the reason recorded, the version bumped,
> and every reader updated in the same PR.

---

## 1. Scope

One line of text inside an acceptance test file, and the digest that goes with it. The
line says which ticket owns the file and, once the file is locked, what the file's
content was at that moment. Nothing else records a lock (D-1).

This contract defines how the line is read, when it is an error, and how the digest is
computed. Which files are read, what writes the line and what a mismatch refuses belong
to the commands that use it (AT-8, AT-9).

Fixtures: `contracts/fixtures/acceptance/`.

## 2. The line

A line is **the lock line** when it contains `invar-lock:`, in lowercase and with the
colon. Everything before the first `invar-lock:` on that line is ignored, so any comment
prefix works (`//`, `#`, `--`, `/*`, `<!--`, ` * ` inside a docblock), and so does no
prefix (D-2). After it comes one of two forms:

```
invar-lock: <TICKET>                       pending
invar-lock: <TICKET> sha256:<64 hex>       locked
```

A **pending** line names the ticket whose test-writer wrote the file, which is not
locked yet. A **locked** line also records the digest of §4.

The rest of the line is read by these rules:

1. **Spaces separate the marker, the ticket and the digest.** One or more `U+0020`. A tab
   or any other whitespace there is not a separator.
2. **`<TICKET>`** is the next run of characters up to whitespace or the end of the line,
   and matches `^[A-Z][A-Z0-9]*-[0-9]+$`, the same rule as `Ticket` in the Plan header
   (`plan-header.md` §5).
3. **The digest** is the field right after the ticket when it is `sha256:` followed by
   exactly 64 lowercase hex characters.
4. **Trailing text** is allowed after the ticket or the digest, separated from it by any
   whitespace (Unicode `White_Space`), and is ignored. A block comment can therefore close
   on the same line: `/* invar-lock: SHOP-214 sha256:… */`.
5. **Any other field that starts with `sha256:` is an error** (D-5). Trailing text never
   holds one, wherever it stands.

The closing of a comment needs its whitespace: `/* invar-lock: SHOP-214*/` has the ticket
`SHOP-214*/`, which is an error.

## 3. Where

The file is canonicalized first (§4d), and a line number is a 1-based line of the
canonical text. Canonicalization adds and removes no line above the last non-blank one, so
this is the same as counting after removing the BOM and normalising line endings: `\r\n`,
`\r` and `\n` each end a line, and nothing else does.

The lock line must be **within the first 10 lines** (D-3).

A file with no line containing `invar-lock:` has no lock. That is not an error here.

### 3.1 Errors

Each is reported with its line number, and exactly one is reported per file, the first
that applies in this order:

| Kind | When | Line reported |
|---|---|---|
| **Invalid UTF-8** | The file does not decode (§4d step 1). Canonicalization's own error | None: the byte offset of the first invalid byte |
| **More than one** | More than one line contains `invar-lock:`, **anywhere in the file** | Every such line |
| **After the window** | The one lock line is after line 10 | That line |
| **Ticket** | The ticket is missing or malformed, or no space follows the marker | That line |
| **Digest** | A field that starts with `sha256:` breaks §2 | That line |

A mention of `invar-lock:` in a test's own text counts as a line, and is reported as one.
A file about the lock line itself cannot be locked; that is the price of a rule with no
judgement in it.

## 4. The digest of a locked file

This is an exact rule, so that another implementation produces the same bytes:

1. **Canonicalize the whole file**: §4d, steps 1 to 4. The result has only `\n` line
   endings, no trailing whitespace on any line, and ends in one `\n`.
2. **Remove the lock line together with its `\n`.** Every other byte stays, in order.
3. **Remove every `\n` at the end of what is left, then add one back**, unless nothing is
   left, in which case nothing is added. This is §4d step 4 again, since removing the last
   non-blank line can leave blank lines at the end. No other step runs twice: in
   particular a `U+FEFF` that now starts the text is content, not a BOM.
4. **SHA-256** of the UTF-8 bytes, written `sha256:<64 lowercase hex>`.

So the digest does not depend on line endings, a BOM or trailing whitespace; nor on the
line's prefix, its ticket, its digest or its trailing text; nor on where within the
window the line stands. It does depend on every other line, including the criterion's
text in a docblock (D-4).

Replacing a pending line with a locked one, or one ticket with another, therefore does
not change the digest.

The digest is written into the pending line in place, right after the ticket, and nothing
else in the file changes: not the rest of the line, not its line ending, not one other byte.

**Worked example.** For a file of `# invar-lock: SHOP-214` and two more lines, step 3
leaves `def test_ac1():\n    assert False\n`, and its digest is the one in example 8.2.

## 5. One ticket per file

The line names the ticket that last locked the file. A ticket may own several files. A
later ticket that changes the file through `## Acceptance tests to change`
(`plan-header.md` §6.5) replaces the ticket on the line, and that is checked by the
commands that verify locks (AT-9), not here.

## 6. Decisions and their reasons

**D-1 — The line is in the file, not in a manifest** (A-3). The file proves itself after
the Plan is deleted (§12), in CI, with nothing else present. A manifest per ticket would
duplicate what the file and git already hold, and would pile up in the repository after
every PR. What this gives up is noticing a deleted file from the lock alone, so deletions
are read from git (AT-9).

**D-2 — The marker is plain text with any prefix.** The core never learns a comment
syntax, so it stays language-neutral (A-2), and a new language needs no change here. A
string that merely contains the marker therefore reads as a lock line, which §3.1 reports
rather than guesses about.

**D-3 — A 10-line window, and a later marker is an error.** A window keeps the line where
a reader sees it, at the top of the file. If a marker below the window simply did not
count, moving the line down one place would turn a locked file into one with no lock.
Reporting it means that moving the line cannot unlock a file. More than one line is an
error anywhere in the file for the same reason: a second line outside the window must not
be able to hide.

**D-4 — The digest excludes only the line itself** (A-5). The Plan is deleted in the PR, so
the criterion's text survives in the test's docblock, and it has to be covered to be worth
anything. Excluding the line is unavoidable, because the line holds the digest. Excluding
nothing else means there is no list of what is safe to change.

**D-5 — A malformed `sha256:` field is an error and not trailing text.** Otherwise a
truncated digest would read as a pending line followed by a remark, and a file whose lock
was damaged would look like one never locked. The same goes for a digest out of place:
any field starting with `sha256:` that is not the digest of §2 is refused.

**D-6 — Spaces only between the fields, any whitespace before trailing text.** A tab in
front of a digest would otherwise make it trailing text (D-5 refuses it instead). Trailing
text is not the core's to describe, so what separates it is left open.

**D-7 — The digest is not canonicalized twice.** Step 3 of §4 re-applies §4d step 4 and
nothing else. Running all of §4d again would strip a `U+FEFF` that the removal happened to
bring to the start of the text, giving two different files one digest. Fixture
`locked/bom-next-line.py` pins it.

## 7. Open questions

None.

## 8. Examples

The fixtures are the executable form of these, and the core's tests check example 8.2's
digest against this file.

### 8.1 Valid — pending, with any prefix

```
// invar-lock: SHOP-214
# invar-lock: SHOP-214
-- invar-lock: SHOP-214
/* invar-lock: SHOP-214 */
<!-- invar-lock: SHOP-214 -->
 * invar-lock: SHOP-214
# invar-lock:    SHOP-214 written by the test-writer
```

Each of these, as the only such line in its file, is pending for `SHOP-214`.

### 8.2 Valid — locked

```python
# invar-lock: SHOP-214 sha256:5d5da6ccd3116bee608b2a6c15d4c9f0d20f32b17b220cc13a643169ab874e07
def test_ac1():
    assert False
```

The digest is that of §4's worked example, and the file is the same with CRLF.

### 8.3 Valid — locked, with a comment closed after the digest

```
/* invar-lock: SHOP-214 sha256:5d5da6ccd3116bee608b2a6c15d4c9f0d20f32b17b220cc13a643169ab874e07 */
<!-- invar-lock: SHOP-214 sha256:5d5da6ccd3116bee608b2a6c15d4c9f0d20f32b17b220cc13a643169ab874e07 -->
```

### 8.4 Valid — no lock

```python
def test_ac1():
    assert False
```

### 8.5 Invalid — a line after the window

A file whose line 11 is `# invar-lock: SHOP-214`. **Why:** moving the line down must not
unlock the file (D-3).

### 8.6 Invalid — more than one line

A file with `// invar-lock: SHOP-214` on line 3 and `// invar-lock: SHOP-215` on line 40.
**Why:** a second line outside the window must not be able to hide (D-3).

### 8.7 Invalid — a missing or malformed ticket

```
# invar-lock:
# invar-lock:SHOP-214
# invar-lock: shop-214
/* invar-lock: SHOP-214*/
```

**Why:** nothing after the marker, no space after it, not the ticket rule, and a comment
closed with no whitespace before it.

### 8.8 Invalid — a malformed digest

```
# invar-lock: SHOP-214 sha256:b017737b
# invar-lock: SHOP-214 sha256:ABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABABAB
# invar-lock: SHOP-214 by-hand sha256:5d5da6ccd3116bee608b2a6c15d4c9f0d20f32b17b220cc13a643169ab874e07
```

**Why:** truncated, uppercase, and out of place. None of them passes as a pending line (D-5).

## 9. Version history

**1.0.0** — initial version (AT-5).
