# Contract: the `invar-lock:` line

**Version:** 1.2.0 — adds §10, what verifying a lock means; see §11
**Status:** stable
**Implements:** §4d (Checksums); A-2, A-3, A-4, A-5 (`docs/TASKS-AT.md`)
**Read by:** the core (`format/lock`; `invar hash --lock`; `invar acceptance lock` and
`verify`; `plan check`; `done`), any later reimplementation

> Changing this file is a **public-contract change** (§14.3). It is made
> deliberately, in its own PR, with the reason recorded, the version bumped,
> and every reader updated in the same PR.

---

## 1. Scope

One line of text inside an acceptance test file, and the digest that goes with it. The
line says which ticket owns the file and, once the file is locked, what the file's
content was at that moment. Nothing else records a lock (D-1).

This contract defines how the line is read, when it is an error, how the digest is
computed, how a line comes to be locked (§9), and what the commands that verify locks
refuse (§10).

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
commands that verify locks (§10).

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

**D-8 — The exit code is not read when locking** (A-2). A red run exits non-zero by design,
and a run that could not load exits non-zero too, so the code cannot tell them apart. The
report can. The code is kept only for messages.

**D-9 — The report is outside the repository.** It lives in a fresh directory of its own in
the OS temp directory, which is removed however the command ends, short of the process being
killed. A report inside the repository could be committed, or swept up by a formatter or a
file watcher, and it would change the very tree whose bytes the command promises not to
touch. A temp directory configured inside the project is refused for the same reason.

**D-10 — Test cases of files that were not passed are ignored.** A runner may add its own, such
as a bootstrap check, and they say nothing about this ticket. A passed file with **no** test
case is the opposite case, and is refused: a syntax error or a filter can do that, and a file
with nothing proven must not be locked.

**D-11 — A fixed limit of 10 minutes, and no configuration key.** The agent that runs this must
not hang on a hanging suite. Locking runs one ticket's files, not the whole suite, so a limit
a project would need to raise is not expected, and a key can be added later without breaking
anyone. At the limit the program started is killed and the command refuses. Processes it
started itself are not tracked: killing a process group would also take the runner out of the
terminal's foreground group, so that Ctrl-C no longer reached it.

**D-12 — A test that passes has no override in this command** (OQ-1). A green test proves
nothing about the code to come, however good the reason. The test-writer answers `BLOCKED`
and the developer decides, which may mean rewriting the criterion. A flag here would be a
flag the agent could pass.

**D-13 — The outcomes of already locked files are ignored.** They were proven red when they
were locked. Now that their code may exist they may pass, and they are run only so that their
tests count for coverage. Their token checks still apply, because coverage reads the tokens.

**D-14 — Every problem in one run.** A run can be slow, and the test-writer can fix everything
at once. Refusing one problem at a time would cost one run per problem.

**D-15 — All or nothing, up to the renames.** Every new file is written beside its original
before the first rename, so any failure until then leaves every test file as it was. Renames
are one at a time, and no file system offers a rename of several: a failure between two of them
is the only partial case. The message names what was written. Running `lock` again recovers,
since a written file now reads as already locked.

**D-16 — Symlinks are not followed, and that includes `acceptance.dir` itself.** A link could
take a run, and then a write, outside the project, or reach one file by two names. A directory
that is itself a link reads as no directory.

**D-17 — CI checks integrity only.** `done` deletes the Plan in the PR, so the commit CI sees
may have no Plan to read a declaration from, and a shallow clone may not have the merge base.
So `invar acceptance verify` with no ticket needs neither a Plan nor git. What it gives up is
the branch rule, which `plan check` and `done` make while the Plan exists.

**D-18 — Pending lines are allowed in `plan check`, and refused in `done` and in CI.** The
test-writer runs `plan check` before it has locked anything, so refusing them there would stop
the cycle at its first step. A cycle that closes with one has half a lock, and a branch that
reaches CI with one has a test nobody proved red.

**D-19 — A declared path that is not a locked test is a warning.** A typo cannot let a change
through: the declaration matches nothing, so the real change is still refused. Refusing the typo
as well would add nothing but a second reason. A path that breaks §10.2's form is different. It
can never name a test, so it is a problem.

**D-20 — The honest limit: a digest is not a signature.** Someone who edits a locked file and
writes the new digest by hand passes §10.1, because `invar hash --lock` prints that digest for
anyone. §10.2 catches this for another ticket's file, because the version at the merge base is
out of their reach. For the current ticket's own new files nothing in the core does: review and
the PR diff are what catch it, as for the Plan's `Approved-checksum`. The lock makes a weakened
test visible and deliberate. It does not make one impossible.

**D-21 — Removing the `acceptance:` section disables the check.** With no section there is no
directory to look in, so `verify` warns that there is nothing to verify, and exits 0. `done` warns
the same way for a Plan that says `required`, since it closes the cycle, and `plan check` says
nothing, as it did before this check existed. Pointing `acceptance.dir`
somewhere else has the same effect on the files left behind. The change to `invar.yaml` is in
the PR, so review and the decision log are what notice it. The core does not read an older
config from git to second-guess the current one.

**D-22 — A change is read through the digest, and in all three places.** Comparing bytes would
call a line-ending conversion by a checkout filter a change, and §4 already says what does not
count. Comparing only the digest would miss a lock being taken off, since removing the line or
the digest leaves the covered text alone, so the ticket and the recorded digest are compared
too. The working tree, the index and `HEAD` are each compared, because the file on disk can be
put back while the change stays staged or committed, and that change is what the PR will hold.

**D-23 — No repository is a warning, and a repository that cannot answer is a problem.** A
project outside git has no merge base and no branch, and `invar done` has always worked there,
so the branch rule is skipped and says so. Inside a repository, a base branch that cannot be
found, a missing merge base or a git that refuses to look would otherwise be a way past the
rule, so each is reported. The fix is to fetch, or to set `git.base_branch`.

**D-24 — A missing `acceptance.dir` holds no files, and one that is not a directory is a
problem.** A project with no acceptance tests yet has nothing wrong with it. A file or a symlink
at that path (D-16) could hide every lock from the walk, and must not read as zero files that
all hold.

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

## 9. Locking

`invar acceptance lock <TICKET>` is the test-writer's command, and the first one that runs
anything (A-2). It writes digests only when every test checks something. On any refusal it
writes nothing and prints nothing on stdout, and the reason is on stderr.

### 9.1 Before anything runs

The refusals of `invar acceptance spec`, in its order and its words: the Plan is not approved
or changed since; `Acceptance` is not `required`; there is no `acceptance:` section; the
criteria have no ids.

Then the ticket's files. `acceptance.dir` is walked recursively, in sorted order and following
no symlink (D-16). A file whose bytes do not contain `invar-lock:` is not a test file of any
ticket, and is skipped, as is a leftover `*.invar-tmp` of an interrupted write. The rest are
read by §2 to §4:

| What the file holds | Means |
|---|---|
| An error of §3.1 | Refused, naming the file and the error |
| A pending line for this ticket | **To lock** |
| A locked line for this ticket, with the file's digest | **Already locked** |
| A locked line for this ticket, with another digest | Refused, naming the file: it changed after locking |
| A line for another ticket | Ignored |

All of these refusals are reported together. With nothing to lock, or with no such directory,
the command refuses and says that the test-writer writes files with the pending line.

### 9.2 The run

The files to lock and the already locked files run together, in one run. The already locked
ones are there only so that their tests count for coverage (D-13).

`acceptance.run` is run without a shell, with the project root as its working directory and
stdin empty. `{report}` becomes one path (D-9), and `{files}` becomes the files as separate
arguments, relative to the root and sorted. stdout and stderr are captured. The exit code is
not read (D-8).

These refuse, and none of them says anything about coverage or red:

| What happened | The message says |
|---|---|
| The program cannot be started | The program, and `acceptance.run` |
| The limit passed (D-11) | The limit, and the last lines of stderr and of stdout |
| No report was written, or it is **Empty** (`test-results.md` §6) | "the tests did not run", the exit status, and the last 40 lines of stderr and of stdout |
| Any other error of `test-results.md` §6 | That error, and the exit status |

### 9.3 The checks

Test cases whose file was not passed are ignored (D-10). A file passed with no test case is
refused ("no test from this file ran").

**In both kinds of file**, each test case has exactly one `AC<n>` token (`test-results.md` §4),
and the Plan has that id. Otherwise the test is refused.

**In the files to lock**, the outcome decides:

| Outcome | Means |
|---|---|
| `failure` | Red for the right reason. The only one accepted (A-4) |
| `passed` | Refused: it passes before the code exists. There is no override here (D-12) |
| `error` | Refused: it fails for the wrong reason, with the failure's type and the first lines of its message |
| `skipped` | Refused: a skipped test proves nothing |

**In already locked files**, the outcome is not looked at (D-13).

**Coverage:** every criterion id of the Plan has at least one test case with that one token,
counting both kinds of file. Missing ids are refused, each with its text.

Every problem is reported in one run, grouped by file and test (D-14).

### 9.4 Writing

Only when every check holds:

1. Every file to lock is read again and compared with the bytes that ran. If any changed, the
   command refuses, naming them, and writes nothing.
2. The digest goes into each file's line (§4), and each result is written to a temporary file
   beside the original, with the original's permissions.
3. Each temporary file is renamed over its original.

A failure before step 3 leaves every test file as it was. A failure during step 3 is the only
partial case, and the message names the files written and the files not written (D-15).

### 9.5 The output on success

On stdout: per criterion in the Plan's order, the tests that cover it, each with its file and
name; then each file locked, with its digest; then one fixed last line that the test-writer
keys on:

```
locked <TICKET> <N> file(s)
```

`<N>` counts the files written by this run, not the ones already locked.

## 10. Verifying

One check, used by `plan check`, `done` and `invar acceptance verify`. None of them runs a test
(A-2): green is the team's CI. Every problem is reported in one run (D-14), each naming its file
and ticket, and saying what to do.

### 10.1 Integrity

Needs no git and no Plan. Every file under `acceptance.dir`, found as in §9.1:

| What the file holds | Means |
|---|---|
| An error of §3.1 | A problem, naming the file and the error |
| A locked line whose digest is not the file's | A problem, naming the file, the ticket on the line, and both digests |

A missing directory holds no files. A path that exists and is not a directory, or is a symlink,
is a problem (D-24).

### 10.2 The branch rule

Needs git and the current, approved Plan. It compares the merge base of `HEAD` and the base
branch (`git.base_branch`, else `origin/HEAD`, else `main`) with the files as they are now, in the
working tree, the index and `HEAD` (D-22).

**Which files.** Every file under `acceptance.dir` whose version at the merge base has a **locked**
line for **another** ticket. A file locked there for the current ticket is its own (a ticket's
second cycle), and needs no declaration.

**What counts.** There is no rename detection: a rename is a delete plus a new file.

- **Deleted:** missing from the working tree, the index or `HEAD`. Allowed only if the Plan lists
  the path with `delete`. The version at the merge base is read from git, since there is no
  manifest (A-3).
- **Changed:** otherwise, when any of the three differs from the merge base. It differs unless its
  bytes are the same, or its line has the same ticket and the same recorded digest and its digest
  of §4 is the same. So taking the line or the digest off is a change, and so is naming another
  ticket on it. A symlink in the file's place is a change. Allowed only if the Plan lists the path
  with `change`.

The action has to match: a file deleted under a `change` declaration is refused, and the other way
round.

**The declarations** (`plan-header.md` §6.5):

| A path in `## Acceptance tests to change` | Means |
|---|---|
| Absolute, with a `\`, with a `..`, or not under `acceptance.dir` | A problem. `.` and empty segments are ignored |
| Not a locked test at the merge base | A warning (D-19) |
| A locked test that did not change | A warning |

**When it cannot run.** Outside a git repository, the rule is skipped with a warning. Inside one, a
base branch that is not there, no merge base, or git failing is a problem (D-23).

### 10.3 Closing

For `done`, and `verify` with a ticket:

- `Acceptance: required`: at least one **locked** file for the ticket under `acceptance.dir`.
- `required` or `skipped`: **no pending** line for the ticket (D-18).

### 10.4 The callers

| Command | Checks | Pending lines | Refuses before |
|---|---|---|---|
| `plan check <TICKET>` | §10.1, §10.2, after its own checks | Allowed | Printing its `ok:` and `acceptance:` lines, which do not change |
| `done <TICKET>` | §10.1, §10.2, §10.3, after its own checks | The ticket's are refused | Deleting anything |
| `invar acceptance verify <TICKET>` | The same as `done`, and what `plan check` refuses | The ticket's are refused | |
| `invar acceptance verify` | §10.1 | Anywhere under `acceptance.dir`, refused | |

`invar acceptance verify` is an agent's command, and needs no Plan and, with no ticket, no git. It
exits 0 with one line on stdout, `verified <N> locked file(s)`, or non-zero with nothing on stdout
and every problem on stderr. `<N>` counts the locked files under `acceptance.dir` whose digest
holds, for any ticket.

**No `acceptance:` section** (D-21): `verify` warns on stderr that there is nothing to verify, and
exits 0. `plan check` goes on as before, and so does `done`, with the same warning when the Plan
says `required`.

## 11. Version history

**1.2.0** — adds §10 and D-17 to D-24: what verifying a lock means (AT-9). The line, the digest and
locking do not change.

**1.1.0** — adds §9 and D-8 to D-16: what `invar acceptance lock` checks and writes (AT-8). The
line and the digest do not change. It is here, not in a contract of its own, because §9 is how
the line of §2 becomes the line of §2 with a digest, and the two are read together.

**1.0.0** — initial version (AT-5).
