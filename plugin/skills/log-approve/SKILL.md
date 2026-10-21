---
name: log-approve
description: The developer's approval of a draft decision entry, from inside Claude Code. Records .invar/plans/<TICKET>-<slug>/decision.md in the decision log as it is now and removes the draft. Only the developer can run it.
argument-hint: [ticket]
disable-model-invocation: true
---

The developer ran `/invar:log-approve $ARGUMENTS`. The command below was run for
them, by Claude Code, before you saw this message. It records their decision, not
yours.

```!
t="$ARGUMENTS"
case "$t" in
  *[!A-Za-z0-9._/-]*) echo "invar: \`$t\` is not a ticket or a Plan path"; exit 0 ;;
esac
if [ -z "$t" ]; then
  set -- .invar/plans/*/decision.md
  if [ ! -f "$1" ]; then
    echo "invar: there is no draft entry in .invar/plans/ to record; draft one with /invar:log"; exit 0
  fi
  if [ "$#" -gt 1 ]; then
    echo "invar: $# drafts are waiting; say which: /invar:log-approve <ticket>"
    for p in "$@"; do d="${p%/decision.md}"; echo "  ${d##*/}"; done
    exit 0
  fi
  d="${1%/decision.md}"; t="${d##*/}"
fi
invar decision approve "$t" 2>&1
```

Show the output above to the developer exactly as it is, and stop.

- Do not run `invar decision approve` yourself, and do not retry it. If it
  refused, the output says why; fixing the draft is the developer's call, or
  `/invar:log` again if they ask for it.
- Do not record the entry another way: never run `invar decision record`, and
  never write into `.invar/decisions/`.
- If the output has a `reset:` line, the entry was `failed` and the Plan is a
  draft again. Back to Phase 1 when the developer asks for it: the planner
  revises the Plan, and the developer approves it again.
