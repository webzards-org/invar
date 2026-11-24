# Contract: the `invar-lock:` line

**Version:** 1.6.0 — removing the `acceptance:` section is refused in CI without a person's trailer; see §12
**Status:** stable
**Implements:** §4d (Checksums); A-2, A-3, A-4, A-5 (`docs/TASKS-AT.md`)
**Read by:** the core (`format/lock`; `invar hash --lock`; `invar acceptance lock` and
`verify` and `unlock` and `run`; `plan check`; `done`; `plan guard`), any later reimplementation

> Changing this file is a **public-contract change** (§14.3). It is made
> deliberately, in its own PR, with the reason recorded, the version bumped,
> and every reader updated in the same PR.

---

## 1. Scope

One line of text inside an acceptance test file, and the digest that goes with it. The
line says which ticket owns the file and, once the file is locked, what the file's
content was at that moment. Nothing else records a lock (D-1).

This contract defines how the line is read, when it is an error, how the digest is
computed, how a line comes to be locked (§9), what the commands that verify locks
refuse (§10), whether locked tests passed (§10.5), and how the developer takes a lock off (§11).

Fixtures: `contracts/fixtures/acceptance/`.

## 2. The line

A line is **the lock line** when it contains `invar-lock:`, in lowercase and with the
colon. Everything before the first `invar-lock:` on that line is ignored, so any comment
prefix works (`//`, `#`, `--`, `/*`, `<!--`, ` * ` inside a docblock), and so does no
prefix (D-2). After it comes one of two forms:

```
invar-lock: <TICKET>                                   pending
invar-lock: <TICKET> sha256:<64 hex>                   locked, before 1.5.0
invar-lock: <TICKET> sha256:<64 hex> ac=<id>[,<id>…]   locked, with its criteria
```

A **pending** line names the ticket whose test-writer wrote the file, which is not
locked yet. A **locked** line also records the digest of §4, and, from 1.5.0, the criteria
the file's tests cover (D-38). A locked line without the list is valid: it is what every
line locked before 1.5.0 holds, and it reads as it did.

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
6. **The list** is the field right after the digest, separated from it by spaces, when it
   starts with `ac=`, lowercase. After `ac=` come one or more ids separated by `,`, with no
   space: each `AC` and a positive number with no leading zero, as a criterion's id in the
   Plan (`plan-header.md` §6.4), strictly increasing by number, so sorted and without a
   repeat. `ac=AC1,AC2,AC10` is a list; `ac=AC2,AC1`, `ac=AC1,AC1`, `ac=AC01`, `ac=ac1`,
   `ac=AC1, AC2` and `ac=` are errors.
7. **Any other field that starts with `ac=` is an error** (D-39): on a pending line, a second
   one, or one in trailing text, wherever it stands. Trailing text may follow the list, as it
   may follow the digest, so a comment can still close after it.

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
| **Criteria** | A field that starts with `ac=` breaks §2 rules 6 and 7 | That line |

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
4. **When the line has a list, add one line**: `ac=` and the list exactly as the line writes
   it, then `\n` (D-38). With no list, nothing is added, so the digest of a line without one
   is exactly what it was before 1.5.0.
5. **SHA-256** of the UTF-8 bytes, written `sha256:<64 lowercase hex>`.

So the digest does not depend on line endings, a BOM or trailing whitespace; nor on the
line's prefix, its ticket, its digest or its trailing text; nor on where within the
window the line stands. It does depend on every other line, including the criterion's
text in a docblock (D-4), and on the list.

Replacing one ticket with another therefore does not change the digest, and neither does
replacing a pending line with a locked one that has no list. Adding, removing or changing
a list does.

The digest, and the list after it with one space between them, are written into the
pending line in place, right after the ticket, and nothing else in the file changes: not
the rest of the line, not its line ending, not one other byte.

**Worked example.** For a file of `# invar-lock: SHOP-214` and two more lines, step 3
leaves `def test_ac1():\n    assert False\n`, and its digest is the one in example 8.2. Locked
with the list `AC1`, step 4 makes it `def test_ac1():\n    assert False\nac=AC1\n`, whose digest
is the one in example 8.9.

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
directory to look in, so `verify` warns that there is nothing to verify, and exits 0. `plan check`
and `done` give one warning, in the same words, for a Plan that says `required`, because a person
decided that this ticket needs tests and nothing will check them. For a Plan that says `skipped`
they say nothing, as before this check existed. Pointing `acceptance.dir`
somewhere else has the same effect on the files left behind. **That stays the rule for
`verify`, `plan check` and `done`**: locally, removing the section is allowed, and warned. Since
1.6.0 it is no longer only review that notices it: `invar config check` in CI compares
`invar.yaml` with the base branch's, and refuses a removed section, a changed `dir` or `run`, or
`required` turned to `false`, unless a commit of the branch has an `Invar-Config: <reason>`
trailer the developer wrote (`invar-yaml.md` §3.9). `verify` itself still reads only the
current config; the comparison is that command's.

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

**D-25 — Unlocking is a command of its own, not `lock --relock`.** One command that locks and
another that unlocks are easier to read and to guard than one command with a flag that does the
opposite. `HUMAN_ONLY` names words, so `acceptance unlock` is denied whole, while a flag on `lock`,
which the test-writer has to run, would have been one more word to look for after the ones that
are allowed. The skill keeps the name `/invar:relock` (A-8), because relocking is what the
developer is asking for.

**D-26 — The record is a file in the Plan's directory.** It dies with the Plan at `done`, which is
right for something that only means anything during one cycle. It needs no change to the grammar
of §2, and a mark inside the test file would be one more thing the test-writer could write by hand,
under a digest that covers the file.

**D-27 — Two rounds per ticket.** Two corrections of the same tests mean the criteria are not what
the developer meant, and a third correction would be editing the specification through its tests.
So a third is refused, and the cycle ends as `failed`. A round is one run of the command, however
many files it names, so the files of one correction are named together. The rounds outlive a
re-approval, because the limit is on the ticket.

**D-28 — Only the red check goes, and for the whole file.** A corrected test would pass now that the
code exists, so A-4 could never relock it (A-8). An `error` still means the test is broken, and a
`skipped` test still proves nothing. The tokens and coverage are what they always are. The check is
skipped per file because the lock is per file, so a test added to an unlocked file skips it too;
the reviewer reads what the file says against the criteria (AT-14).

**D-29 — A file leaves the open list when locked, and stays in the record's history.** "Unlocked
once per `unlock`" needs it off the list that grants the skip, and telling the reader at `done` which
files were unlocked needs it to stay somewhere. Every open entry closes on a successful `lock`,
including one that was ignored, so that no old entry can grant a skip later.

**D-30 — The honest limit: the record is not a signature either.** The guard denies `Write` and
`Edit` of the record, and a shell can still write it, as it can write a digest (D-20). An agent that
writes the record and takes the digest off a file by hand gets a skipped red check. What catches
that is that it cannot be quiet: `lock` names the file, `verify` and `done` list it, the record and
the test are in the branch's diff, and a person who never ran `/invar:relock` sees an unlock they did
not make. It makes the skip visible and deliberate. It does not make it impossible.

**D-31 — `verify` says it on stderr, and `done` on stdout.** `verify`'s stdout is one line that the
main session reads (§10.4). `done` is shown through `/invar:done`, whose shell block carries stdout,
and its success line stays the last.

**D-32 — Results are read, and the suite is still never run** (A-2). Running the team's suite would
need its database, its services and its time, and would make the core a CI runner. The team's CI
already runs it, and every relevant runner writes JUnit XML, so the core reads that report. A report
need not have been asked for by the core: any run that wrote it counts. The one run the core starts is
the one `lock` already starts, on locked acceptance files only.

**D-33 — `skipped` is never passed, and there is no setting.** A locked test that is skipped asserts
nothing, and a coder that marks one skipped has weakened it as surely as one that edits it, without
changing a byte the digest sees. A setting would be the way to switch the check off.

**D-34 — A test that passes only on a retry is not passed.** Runners and CI steps retry flaky tests,
and both runs can end up in the reports. A test that failed once and passed once has shown that it can
fail; for a test that is the target, that is reported, not accepted. Merging by file and name is what
makes a retry one test, and not two of which one passed.

**D-35 — Results after integrity.** The results of a file that changed after locking say nothing about
the test that was locked, so `verify` reports the broken lock and reads no report until it holds.

**D-36 — `acceptance run` is feedback, and the boundary is CI.** It runs in the agent's environment,
from the agent's shell, so an agent could fake what it prints. It exists so that the main session does
not offer a review of red tests, and so that the coder sees what failed before it says `DONE`. What a
person can rely on is `verify --results` on the team's CI, where the report comes from the team's own
run.

**D-37 — In `acceptance run`, a criterion with no test that ran is not passed.** Coverage held when the
files were locked (§9.3), so a criterion with no test now means the run filtered it out. `verify` does
not check coverage, because with no ticket it has no Plan.

**D-38 — The line records the criteria its file covers, under the digest.** A file whose tests ran
only in part passed the check of 1.4.0: it asked for one test case per file. An agent cannot change a
locked file, but it can leave a test out of the run, through `phpunit.xml` or a filter in CI, and CI is
where the check is the boundary and has no Plan to count criteria against. So the line says which
criteria the file covers, and `verify --results` asks for each of them. The list is taken from the run
that locked the file, because that run is what proved each test red for its criterion. It must be under
the digest, or an agent could take `AC2` off the line and the digest would still hold; it goes in as one
line after the covered text, for the reason `plan-header.md` D-19 puts `Acceptance` in the approval
digest: a value the command writes, outside the body, and part of what was decided. A line with no list
gets exactly the digest of before, so no file locked before 1.5.0 breaks. The ids are sorted and
written one way only, so that there is one list for one set of criteria.

**D-39 — An `ac=` field out of place is an error, as a `sha256:` one is.** As with D-5: read as trailing
text, a list written after a tab, after other text or a second time would be ignored silently, and a
person reading the line would believe it. A list on a pending line is an error for the same reason: only
`lock` writes one, with the digest. `plan guard` denies a `Write` or `Edit` that writes either.

**D-40 — A line locked before 1.5.0 keeps the check of 1.4.0, and says so.** Refusing it would break
every project that locked before; giving it a list would mean writing into locked files, which only
`lock` does. It is checked as before, and the note says the check is weaker for it, until the file is
locked again, after `acceptance unlock` or under `## Acceptance tests to change`.

**D-41 — `--results` with no `acceptance:` section is an error.** D-21 lets a project switch the lock
check off by removing the section, because then nothing asked for it. A caller that passes `--results`
asked for a check that cannot run; exiting 0 would let a CI step pass that checked nothing.

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

### 8.9 Valid — locked, with its criteria

```
# invar-lock: SHOP-214 sha256:af62a75863c29c1d9e9504ef95758cb7ec59c07d5265d67cabddf46810c1d04b ac=AC1
/* invar-lock: SHOP-214 sha256:af62a75863c29c1d9e9504ef95758cb7ec59c07d5265d67cabddf46810c1d04b ac=AC1 */
```

The file of example 8.2, locked with the list `AC1`: the digest covers the list (§4 step 4), so
it is not example 8.2's.

### 8.10 Invalid — a malformed or misplaced list

```
# invar-lock: SHOP-214 ac=AC1
# invar-lock: SHOP-214 sha256:af62a75863c29c1d9e9504ef95758cb7ec59c07d5265d67cabddf46810c1d04b ac=AC2,AC1
# invar-lock: SHOP-214 sha256:af62a75863c29c1d9e9504ef95758cb7ec59c07d5265d67cabddf46810c1d04b ac=AC1 ac=AC2
```

**Why:** on a pending line, not sorted, and twice (D-39). Each is **Criteria**.

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

**In a file to lock that the unlock record lists** (§11.3), `passed` is accepted too. `error` and
`skipped` are refused as above.

**Coverage:** every criterion id of the Plan has at least one test case with that one token,
counting both kinds of file. Missing ids are refused, each with its text.

Every problem is reported in one run, grouped by file and test (D-14).

### 9.4 Writing

Only when every check holds:

1. Every file to lock is read again and compared with the bytes that ran. If any changed, the
   command refuses, naming them, and writes nothing.
2. The digest and the list go into each file's line (§4). The list is every criterion id that
   a test case of that file named in this run, one token each as §9.3 requires, sorted by
   number (D-38). Each result is written to a temporary file beside the original, with the
   original's permissions.
3. Each temporary file is renamed over its original.

A failure before step 3 leaves every test file as it was. A failure during step 3 is the only
partial case, and the message names the files written and the files not written (D-15).

### 9.5 The output on success

On stdout: per criterion in the Plan's order, the tests that cover it, each with its file and
name; then each file locked, with its digest and its list; then one fixed last line that the
test-writer keys on:

```
locked <TICKET> <N> file(s)
```

`<N>` counts the files written by this run, not the ones already locked. Above that line, each
file that skipped the red check is named (§11.3).

## 10. Verifying

One check, used by `plan check`, `done` and `invar acceptance verify`. None of them runs a test
(A-2). Whether the locked tests pass is read from JUnit reports, by §10.5. Every problem is reported in one run (D-14), each naming its file
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
holds, for any ticket. With a ticket that has an unlock record, it also says on stderr which files
were unlocked (§11.4).

**No `acceptance:` section** (D-21): `verify` warns on stderr that there is nothing to verify, and
exits 0. `plan check` and `done` go on as before, with one warning on stderr when the Plan says
`required`, and nothing added to stderr otherwise.

### 10.5 Whether the locked tests passed

A lock proves a test did not change. It does not prove the test passes, and the coder's job is to
make it pass. **The core still never runs the team's test suite** (A-2, D-32): it reads JUnit reports
(`test-results.md`), and runs `acceptance.run` only on locked acceptance files, as `lock` does.

**The files.** Every file under `acceptance.dir`, found as in §9.1, with a locked line whose digest
is the file's: of every ticket, or of one ticket when the caller names it. Pending files, and files of
no ticket, are not looked at. Test cases of any other file are ignored, as in §9.3 (D-10).

**With no `acceptance:` section**, `--results` is an error naming the missing section, non-zero, for
`verify` and for `done`, which then deletes nothing (D-41). Without `--results`, D-21 stands.

**The rules**, the same for every caller. Each problem names the file, the test's name and its
criterion by the token rule of `test-results.md` §4, and every problem is reported in one run (D-14):

| What the reports hold | Means |
|---|---|
| No test case for a locked file | A problem: not run by the test suite; add `acceptance.dir` to a test suite |
| A criterion the file's line lists, with no test case of that file naming it by the token rule | A problem: `<file>: <id>: no test of this criterion from this file is in the results` (D-38) |
| A locked line with no list | Only the other rules apply. A note on stderr names the file: locked before criteria were recorded, so only a full run of it is checked (D-40) |
| A test case that is `failure` or `error` | A problem, with the outcome and the first line of its message |
| A test case that is `skipped` | A problem, always (D-33) |
| The same test, by file and name, more than once | Passed only if every occurrence passed (D-34). Otherwise a problem, saying it passed in one run and not in another |
| Every test case of every locked file `passed` | Passed |

**The callers:**

| Command | Reports | Files | Runs |
|---|---|---|---|
| `invar acceptance verify --results <REPORT>...` | The team's own, from its CI | Every ticket's; with a ticket, that ticket's | After §10.1 to §10.4 hold, and only then (D-35) |
| `invar done <TICKET> --results <REPORT>...` | The team's own | The ticket's | After the checks `done` makes, before deleting anything |
| `invar acceptance run <TICKET>` | The one its own run writes | The ticket's | On its own, below |

`--report-root <DIR>` maps the paths in the reports (`test-results.md` §3.1), and needs
`--results`. A report that is missing, empty or cannot be read is an error naming it, and nothing is
checked. Without `--results`, `verify` and `done` read no report and print what they printed before.

`verify --results` exits 0 with one line, `verified <N> locked file(s), and <M> locked test(s)
passed`, where `<M>` counts the distinct tests of the files checked. On any problem it exits non-zero,
with nothing on stdout and every problem on stderr.

**`invar acceptance run <TICKET>`** is feedback for the dispatcher, not the boundary (D-36). It is an
agent's command, and writes nothing. It refuses, with nothing on stdout, in this order: what `plan
check` refuses; a Plan that does not say `required`; no `acceptance:` section; then any malformed line
under `acceptance.dir`, and any of the ticket's files whose digest is not the one its line records;
then no locked file for the ticket. It runs `acceptance.run` as §9.2 does, with only the ticket's
locked files as `{files}`, and refuses as §9.2 does when the run has no result. On stdout:

```
AC1: passed
AC2: not passed
  tests/Acceptance/b_test.php: `it AC2 refuses` (AC2): failure: Failed asserting that false is true.
acceptance: red SHOP-214
```

One line per criterion of the Plan, in its order, each test of it that did not pass below it; a
criterion with no test that ran is not passed (D-37). Then, under `not passed, and of no criterion of
the Plan:`, the locked files that did not run and the tests with no one id of the Plan that did not
pass. The last line is fixed: `acceptance: green <TICKET>`, exit 0, when every criterion passed and
nothing else is listed, and `acceptance: red <TICKET>`, non-zero, otherwise.

## 11. Unlocking

A locked test can be wrong, and the coder must not be the one to decide that (A-8).
`invar acceptance unlock <TICKET> <file>...` is the **developer's** command, through `/invar:relock`:
`plan guard` denies it to an agent, and allows its `--help`.

### 11.1 What it refuses

In this order, and the first that fails is the answer:

1. No file named.
2. What §9.1 refuses first: the Plan is not approved or changed since, `Acceptance` is not `required`,
   there is no `acceptance:` section, the criteria have no ids.
3. An unlock record that cannot be read (§11.2).
4. A third round (D-27). The message says the cycle should end as `failed`, with `/invar:log`.
5. Any named file that is not all of these, each named, and every one reported (D-14): under
   `acceptance.dir`, found as in §9.1, with a locked line, for this ticket, whose digest is the file's.

On any refusal it changes nothing, and with `--skill` the reason is on stdout as `error: …` with
exit 0, as for `plan approve`.

### 11.2 What it writes

**The record**, `.invar/plans/<TICKET>-<slug>/unlocked`, in UTF-8 with `\n` line endings:

```
round: 2
open 2 tests/Acceptance/a_test.php
closed 1 tests/Acceptance/a_test.php
```

| Line | Means |
|---|---|
| `round: <R>` | The first line, and exactly once. `<R>` is `1` or `2`, the rounds used |
| `open <R> <path>` | Unlocked in round `<R>`, waiting for `lock`. This is what grants the skip |
| `closed <R> <path>` | Unlocked in round `<R>`, and `lock` has run since (D-29) |

`<path>` is relative to the project root, with `/` separators, and is the rest of the line. Anything
else is a record that cannot be read, which `unlock`, `lock` and `spec` refuse, and `verify` and `done`
warn about. A path holding a line break is refused by `unlock`. A file listed twice as `open` keeps
the later round.

**The files.** The lock line of each goes back to pending: the digest and the spaces before it are
removed, and the list and the spaces before it when there is one, and no other byte changes. Applied to the output of §4's writing, this gives back the pending
file byte for byte.

In order: every new file is written beside its original; the record is written; each file is renamed
over its original. A failure before the renames leaves the test files as they were. A failure during
them is reported with the files written and not written, and the round is used.

**On success** each file is named, and the last line on stdout is fixed:

```
unlocked <TICKET> <N> file(s), round <R> of 2
```

### 11.3 What `lock` does with it

For a file to lock that is `open` in the record:

- no red check: `passed` and `failure` are both accepted (D-28);
- the tokens and coverage of §9.3 apply as always;
- the output names it, above the last line:
  `red check skipped for <file> (unlocked by the developer, round <R>)`.

Every other file is checked as before. An `open` entry whose file is not pending, being still locked
or gone, is ignored with a warning on stderr.

After a successful `lock`, every `open` entry becomes `closed`, and the round stays. A record that
cannot be updated at that point is a warning, because the files are already locked.

### 11.4 Who says so

`invar acceptance spec` prints the `open` files that are pending, each with its round, under a heading
of its own, with what to do: fix exactly these files to match their criteria, word for word, without
weakening them to pass the code.

`invar acceptance verify <TICKET>`, on stderr, and `invar done`, on stdout above its `removed:` line,
print a line per entry, as information (D-31):

```
unlocked by the developer: <path> (round <R>)
```

### 11.5 The guard

`plan guard` denies `Write` and `Edit` of an `unlocked` file directly in a Plan's directory, whether
or not it exists yet. A shell can still write it (D-30).

## 12. Version history

**1.6.0** — D-21 (#157): removing the `acceptance:` section, or changing `acceptance.dir`, is
still allowed and warned locally, and is now refused in CI by `invar config check` without an
`Invar-Config:` trailer (`invar-yaml.md` 2.5.0, §3.9). No line, digest or command of this
contract changes.

**1.5.0** — §2 rules 6 and 7, the **Criteria** error of §3.1, §4 step 4, §9.4, §10.5, §11.2,
examples 8.9 and 8.10, and D-38 to D-41 (#156, after review): a locked line records `ac=<ids>`, the
criteria its file's tests named when it was locked, and the digest covers the list. `verify
--results`, `done --results` and `acceptance run` ask for a test of each listed criterion from its
file, so a file that ran only in part is refused. A line with no list reads, digests and verifies as
before, with a note. `--results` with no `acceptance:` section is now an error.

**1.4.0** — adds §10.5 and D-32 to D-37: whether the locked tests passed (#156). `verify` and `done`
take `--results` and `--report-root`, and `invar acceptance run` is new. Without `--results`, nothing
changes: no report is read, and every output is as before. The line, the digest, locking and unlocking
do not change.

**1.3.0** — adds §11 and D-25 to D-31: unlocking a test (AT-13). `lock`, `verify`, `spec` and `done`
read the record. The line and the digest do not change, and nothing changes for a ticket with no
record.

**1.2.0** — adds §10 and D-17 to D-24: what verifying a lock means (AT-9). The line, the digest and
locking do not change.

**1.1.0** — adds §9 and D-8 to D-16: what `invar acceptance lock` checks and writes (AT-8). The
line and the digest do not change. It is here, not in a contract of its own, because §9 is how
the line of §2 becomes the line of §2 with a digest, and the two are read together.

**1.0.0** — initial version (AT-5).
