---
name: review
description: The advisory check before a pull request. Runs the reviewer agent on the approved Plan of a ticket and shows its report. Only the developer can run it.
argument-hint: [ticket]
disable-model-invocation: true
---

The developer ran `/invar:review $ARGUMENTS`. Claude Code resolved which Plan it is
for before you saw this message:

```!
t="$ARGUMENTS"
case "$t" in
  *[!A-Za-z0-9._/-]*) echo "invar: \`$t\` is not a ticket or a Plan path"; exit 0 ;;
esac
if [ -z "$t" ]; then
  set -- .invar/plans/*/spec.md
  if [ ! -f "$1" ]; then
    echo "invar: there is no Plan in .invar/plans/ to review against"; exit 0
  fi
  if [ "$#" -gt 1 ]; then
    echo "invar: $# Plans exist; say which: /invar:review <ticket>"
    for p in "$@"; do d="${p%/spec.md}"; echo "  ${d##*/}"; done
    exit 0
  fi
  d="${1%/spec.md}"; t="${d##*/}"
fi
echo "Ticket: $t"
```

If the line above is not `Ticket: …`, show it to the developer and stop.

Otherwise, delegate to the **reviewer** agent (`reviewer`; `invar:reviewer` if your
agent list shows it namespaced). **Give it the ticket line and nothing else**: not
what you built, not why, not which tests pass, not what you expect it to find. The
reviewer is independent of the session that wrote the code, and every sentence you
add is one of your justifications it would inherit.

Show its report to the developer **exactly as it returns it**. Then stop.

- Do not argue with the report, explain it away, or re-check its findings.
- Do not change code because of it unless the developer asks you to.
- It is advisory. `invar done` does not depend on it, and nothing blocks on it.
