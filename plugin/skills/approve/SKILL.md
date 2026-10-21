---
name: approve
description: The developer's approval of a Plan, from inside Claude Code. Runs invar plan approve, shows the Acceptance criteria and the result, and starts Phase 2 at once when the approval is recorded, unless --no-start is given. Only the developer can run it.
argument-hint: [ticket] [--no-start]
disable-model-invocation: true
---

The developer ran `/invar:approve $ARGUMENTS`. The command below was run for
them, by Claude Code, before you saw this message. It is their approval and not
yours.

```!
a="$ARGUMENTS"
case "$a" in
  *[!A-Za-z0-9._/\ -]*) echo "invar: \`$a\` is not a ticket or a Plan path"; exit 0 ;;
esac
t=""; start=yes
for w in $a; do
  case "$w" in
    --no-start) start=no ;;
    -*) echo "invar: \`$w\` is not an option of /invar:approve; the one option is --no-start"; exit 0 ;;
    *)
      if [ -n "$t" ]; then echo "invar: give one ticket: /invar:approve <ticket> [--no-start]"; exit 0; fi
      t="$w" ;;
  esac
done
if [ -z "$t" ]; then
  set -- .invar/plans/*/spec.md
  if [ ! -f "$1" ]; then
    echo "invar: there is no Plan in .invar/plans/ to approve"; exit 0
  fi
  if [ "$#" -gt 1 ]; then
    echo "invar: $# Plans are waiting; say which: /invar:approve <ticket>"
    for p in "$@"; do d="${p%/spec.md}"; echo "  ${d##*/}"; done
    exit 0
  fi
  d="${1%/spec.md}"; t="${d##*/}"
fi
invar plan approve "$t" 2>&1
if [ "$start" = no ]; then echo "Phase 2: not started (--no-start)"; fi
```

Show the output above to the developer as it is. Then do one of two things,
decided by the output alone — never by your own reading of whether the Plan
looks approved.

**If the output contains the line `approved <TICKET> checksum sha256:…`** — the
line `invar plan approve` prints only when it has just recorded an approval —
**and no line `Phase 2: not started (--no-start)`**: the approval is the
developer's decision to implement. **Start Phase 2 for that ticket now**, as the
protocol skill (`invar`) describes it: run `invar plan check <TICKET>` first,
then start the coder (one per work unit, when the Plan declares them) with the
ticket line and nothing else. Do not ask the developer to confirm; say in one
line that Phase 2 has started.

**Otherwise, stop.** Write no code and start no coder. A refusal says why, and
that is for the developer. `already approved` recorded nothing new; `Phase 2:
not started` means the developer will implement later, or elsewhere. Either way,
Phase 2 starts when the developer asks for it.

- Do not run `invar plan approve` yourself, with or without `--reapprove`, and do
  not retry it. If it refused, the output says why; that is for the developer.
- Starting Phase 2 decides nothing: `invar plan check`, which the coder runs
  again, still refuses a Plan that is not approved or has changed since.
