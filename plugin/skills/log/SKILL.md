---
name: log
description: Drafts the decision entry for a ticket at the end of an invar cycle, for the developer to read before it is recorded. Writes .invar/plans/<TICKET>-<slug>/decision.md and checks it; records nothing. Only the developer can run it.
argument-hint: [ticket]
disable-model-invocation: true
---

The developer ran `/invar:log $ARGUMENTS`. Claude Code resolved which Plan it is
for before you saw this message:

```!
t="$ARGUMENTS"
case "$t" in
  *[!A-Za-z0-9._/-]*) echo "invar: \`$t\` is not a ticket or a Plan path"; exit 0 ;;
esac
if [ -z "$t" ]; then
  set -- .invar/plans/*/spec.md
  if [ ! -f "$1" ]; then
    echo "invar: there is no Plan in .invar/plans/ to draft an entry for"; exit 0
  fi
  if [ "$#" -gt 1 ]; then
    echo "invar: $# Plans exist; say which: /invar:log <ticket>"
    for p in "$@"; do d="${p%/spec.md}"; echo "  ${d##*/}"; done
    exit 0
  fi
  d="${1%/spec.md}"; t="${d##*/}"
fi
echo "Ticket: $t"
```

If the line above is not `Ticket: …`, show it to the developer and stop.

Otherwise, **draft the decision entry** for that ticket. A draft is not an entry:
it is a text the developer reads, edits or throws away. Nothing is recorded until
the developer decides.

## What to read

- **The Plan**, `.invar/plans/<TICKET>-<slug>/spec.md`: its `Module`, the Intent,
  the Rejected alternatives and the Acceptance criteria.
- **What the cycle actually did**, from this session: the coder's last answer
  (`DONE` and its summary, or `BLOCKED` and its reason), the review's report if
  one ran, and what the developer decided along the way.
- **What changed**, if you need it: `git status` and `git diff --stat`. The
  numbers are for you, not for the entry.

Never open `.invar/decisions/`: the log is the planner's, not yours.

## The type

- **`confirmed`** — the coder answered `DONE` and the cycle is about to close.
- **`failed`** — the coder answered `BLOCKED`, or the approved Plan turned out to
  be impossible.
- **`note`** — anything else worth keeping: an abandoned cycle, a finding the
  next Plan should know.

Never `correction`: that records what the developer changed by hand, and they
record it themselves.

## The body

**What was decided, and why**, in two to five sentences. The planner reads it
next time, for the reasons it cannot find in the code.

- A deviation from the Plan, and its reason.
- A choice between alternatives that will come up again, and why this one.
- For `failed`: *"The Plan required X; impossible because Y."* — precisely.

**Not a summary of the diff**: no list of files, no "added a test", no
restatement of the acceptance criteria. The diff is in git. If the cycle decided
nothing worth remembering, say so to the developer instead of writing filler.

## Write it

Write `.invar/plans/<TICKET>-<slug>/decision.md`, next to the Plan's `spec.md`. If
a draft is already there, replace it: the newest draft is the one.

```markdown
---
ticket: SHOP-214
module: orders
type: confirmed
---

Cancelling releases reserved stock inside the cancel transaction, through
`StockLedger`, rather than in a queued job: a job would leave the stock neither
reserved nor sellable until it ran.
```

- `ticket` is the Plan's ticket. `module` is the Plan's `Module`, or `global`
  when the decision is not about one module.
- **No `id` and no `date`**: they are assigned when the entry is recorded.
- Never write into `.invar/decisions/`, and never edit `spec.md`.

Then run `invar decision validate <TICKET>`. It prints `ok:` or every problem with
its line; fix them and run it again until it says `ok:`.

## Then stop

Show the developer the draft as you wrote it, and its path. **Do not record it**:
whether the cycle is worth an entry, and what it says, is the developer's call.
Tell them, in one line, that they read it, edit it if they want to, and record it
with `/invar:log-approve <TICKET>` — which records the draft as it is then, and
removes it. For `failed`, add that recording it also returns the Plan to `draft`.

Recording is never yours: do not run `invar decision approve` or
`invar decision record` — a hook denies both — and do not write the entry into
the log any other way.

If the developer asks for another draft, change it and run
`invar decision validate <TICKET>` again. If they edit it themselves, leave it as
they left it.
