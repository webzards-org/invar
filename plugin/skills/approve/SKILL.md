---
name: approve
description: The developer's approval of a Plan, from inside Claude Code. Runs invar plan approve and shows the Acceptance criteria and the result. Only the developer can run it.
argument-hint: [ticket]
disable-model-invocation: true
---

The developer ran `/invar:approve $ARGUMENTS`. The command below was run for
them, by Claude Code, before you saw this message. It is their approval and not
yours.

```!
t="$ARGUMENTS"
case "$t" in
  *[!A-Za-z0-9._/-]*) echo "invar: \`$t\` is not a ticket or a Plan path"; exit 0 ;;
esac
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
```

Show the output above to the developer exactly as it is, and stop.

- Do not run `invar plan approve` yourself, with or without `--reapprove`, and do
  not retry it. If it refused, the output says why; that is for the developer.
- Do not start implementing. Phase 2 begins when the developer asks for it.
