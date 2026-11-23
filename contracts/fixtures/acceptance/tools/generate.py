#!/usr/bin/env python3
"""Materialize and verify the lock fixtures of contracts/fixtures/acceptance.

The fixtures contain bytes that ordinary tooling destroys: CRLF, lone CR, a
BOM, trailing whitespace, invalid UTF-8. They cannot be maintained by hand, so
this script writes them from an explicit byte-level table, as the digest set
does (fixtures/digest/README.md, D-3).

Modes:

    python3 generate.py --check             verify files and locks.json (default)
    python3 generate.py --write             (re)write the fixture FILES
    python3 generate.py --update-expected   recompute locks.json  DELIBERATE

The table states each result by hand: `none`, `pending`, `locked` or `error`,
with its line. Only the digests are computed, by the rule of
formats/acceptance-lock.md section 4, over the line the table names. There is
no lock-line parser here, so the expectations do not come from one.

Standard library only, so it runs anywhere without a virtualenv.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
MANIFEST = ROOT / "locks.json"

SCHEMA = "invar.lock-fixtures/v1"
FIXTURES_VERSION = "1.1.0"

# Replaced in a fixture's content by the digest of that same content, over the
# fixture's `criteria` too when it has them.
THE_DIGEST = "{DIGEST}"
# Replaced by the digest of the content without the list: what the line would
# record if `ac=` had been added by hand afterwards.
DIGEST_WITHOUT_LIST = "{DIGEST_WITHOUT_LIST}"
WRONG_DIGEST = "sha256:" + "0" * 64
SHORT_DIGEST = "sha256:b017737b"
UPPER_DIGEST = "sha256:" + "AB" * 32

# Unicode White_Space, as fixtures/digest/tools/generate.py spells it out.
UNICODE_WHITE_SPACE = (
    "\u0009\u000a\u000b\u000c\u000d \u0085  "
    "           "
    "    　"
)


def canonicalize(text: str) -> str:
    """DESIGN.md section 4d, steps 1-4, on text already decoded."""
    if text.startswith("﻿"):
        text = text[1:]
    text = text.replace("\r\n", "\n").replace("\r", "\n")
    lines = [line.rstrip(UNICODE_WHITE_SPACE) for line in text.split("\n")]
    while lines and lines[-1] == "":
        lines.pop()
    return "\n".join(lines) + "\n" if lines else ""


def lock_digest(text: str, line: int, criteria: list[str] | None = None) -> str:
    """The digest of a locked file (acceptance-lock.md section 4): over the list
    too, as one line `ac=<list>` at the end, when the line has one."""
    lines = canonicalize(text).split("\n")
    del lines[line - 1]
    remaining = "\n".join(lines).rstrip("\n")
    covered = remaining + "\n" if remaining else ""
    if criteria is not None:
        covered += "ac=" + ",".join(criteria) + "\n"
    return "sha256:" + hashlib.sha256(covered.encode("utf-8")).hexdigest()


# --------------------------------------------------------------------------
# Bodies. Written with explicit escapes; never paste a raw CR in here.
# --------------------------------------------------------------------------


def php(marker: str, eol: str = "\n", start: str = "", end: str = "") -> str:
    lines = [
        "<?php",
        "",
        marker,
        "",
        "/**",
        " * AC1: Given 3 in stock, when I order 2, then 1 remains available.",
        " */",
        "it('reserves stock (AC1)', function () {",
        "    expect(true)->toBeFalse();",
        "});",
    ]
    return start + "".join(line + end + eol for line in lines)


def python(marker: str) -> str:
    return f"{marker}\ndef test_ac1():\n    assert False\n"


def js(marker: str) -> str:
    return f"{marker}\ntest('AC1', () => {{\n  expect(1).toBe(2);\n}});\n"


def padded(marker: str, before: int, after: int = 1) -> str:
    return "".join(
        [f"# line {n}\n" for n in range(1, before + 1)]
        + [marker + "\n"]
        + [f"# after {n}\n" for n in range(1, after + 1)]
    )


LONG_PHP = php("// invar-lock: SHOP-214") + "".join(f"// {n}\n" for n in range(11, 40))

none = {"result": "none"}


def pending(line: int) -> dict:
    return {"result": "pending", "ticket": "SHOP-214", "line": line}


def locked(line: int, matches: bool = True, criteria: list[str] | None = None) -> dict:
    entry = {"result": "locked", "ticket": "SHOP-214", "line": line, "matches": matches}
    if criteria is not None:
        entry["criteria"] = criteria
    return entry


def error(kind: str, *lines: int) -> dict:
    return {"result": "error", "kind": kind, "lines": list(lines)}


# (path, content, note, expected). `bytes` content is written as it is.
FIXTURES: list[tuple[str, str | bytes, str, dict]] = [
    ("none/no-marker.php", php("// no lock here"), "no line contains the marker", none),
    ("none/case.py", python("# INVAR-LOCK: SHOP-214"), "the marker is lowercase", none),
    ("none/no-colon.py", python("# invar-lock SHOP-214"), "the colon is part of the marker", none),
    ("none/empty.txt", "", "an empty file has no lock line", none),

    ("pending/php.php", php("// invar-lock: SHOP-214"), "`//` prefix", pending(3)),
    ("pending/python.py", python("# invar-lock: SHOP-214"), "`#` prefix, line 1", pending(1)),
    ("pending/sql.sql", "-- invar-lock: SHOP-214\nSELECT 1;\n", "`--` prefix", pending(1)),
    ("pending/js-block.js", js("/* invar-lock: SHOP-214 */"), "a block comment closed on the line", pending(1)),
    ("pending/html.html", "<!-- invar-lock: SHOP-214 -->\n<p>x</p>\n", "an HTML comment closed on the line", pending(1)),
    ("pending/docblock.php", "<?php\n/**\n * invar-lock: SHOP-214\n * AC1: text\n */\n", "` * ` inside a docblock", pending(3)),
    ("pending/no-prefix.txt", "invar-lock: SHOP-214\ntext\n", "nothing before the marker", pending(1)),
    ("pending/trailing-text.py", python("# invar-lock: SHOP-214 written by the test-writer"), "trailing text is ignored", pending(1)),
    ("pending/many-spaces.py", python("# invar-lock:    SHOP-214"), "more than one space", pending(1)),
    ("pending/crlf.php", php("// invar-lock: SHOP-214", eol="\r\n"), "CRLF terminators", pending(3)),
    ("pending/line-10.py", padded("# invar-lock: SHOP-214", 9), "the last line of the window", pending(10)),

    ("locked/php-lf.php", php(f"// invar-lock: SHOP-214 {THE_DIGEST}"), "`//` prefix, LF", locked(3)),
    ("locked/php-crlf.php", php(f"// invar-lock: SHOP-214 {THE_DIGEST}", eol="\r\n"), "the same, CRLF", locked(3)),
    ("locked/php-cr.php", php(f"// invar-lock: SHOP-214 {THE_DIGEST}", eol="\r"), "the same, lone CR", locked(3)),
    ("locked/php-bom-crlf-ws.php", php(f"// invar-lock: SHOP-214 {THE_DIGEST}", eol="\r\n", start="﻿", end="  "),
     "the same, BOM, CRLF and trailing spaces on every line", locked(3)),
    ("locked/python.py", python(f"# invar-lock: SHOP-214 {THE_DIGEST}"), "`#` prefix", locked(1)),
    ("locked/js-block.js", js(f"/* invar-lock: SHOP-214 {THE_DIGEST} */"), "a block comment closed after the digest", locked(1)),
    ("locked/html.html", f"<!-- invar-lock: SHOP-214 {THE_DIGEST} -->\n<p>x</p>\n", "an HTML comment closed after the digest", locked(1)),
    ("locked/line-10.py", padded(f"# invar-lock: SHOP-214 {THE_DIGEST}", 9), "the last line of the window", locked(10)),
    ("locked/last-line.py", f"alpha\n\n\n# invar-lock: SHOP-214 {THE_DIGEST}\n",
     "removing the line leaves trailing blank lines, which are stripped", locked(4)),
    ("locked/only-the-line.py", f"# invar-lock: SHOP-214 {THE_DIGEST}\n", "nothing else: the digest of zero bytes", locked(1)),
    ("locked/bom-next-line.py", f"# invar-lock: SHOP-214 {THE_DIGEST}\n﻿alpha\n",
     "a U+FEFF that starts the next line is content: the file is not canonicalized twice", locked(1)),
    ("locked/mismatch.php", php(f"// invar-lock: SHOP-214 {WRONG_DIGEST}"), "the recorded digest is not the file's", locked(3, matches=False)),
    ("locked/criteria.py", python(f"# invar-lock: SHOP-214 {THE_DIGEST} ac=AC1,AC2"),
     "the criteria the file covers, under the digest", locked(1, criteria=["AC1", "AC2"])),
    ("locked/criteria-one.php", php(f"// invar-lock: SHOP-214 {THE_DIGEST} ac=AC1"), "one criterion", locked(3, criteria=["AC1"])),
    ("locked/criteria-js-block.js", js(f"/* invar-lock: SHOP-214 {THE_DIGEST} ac=AC1,AC10 */"),
     "a block comment closed after the list, and AC10 sorted after AC1 by number", locked(1, criteria=["AC1", "AC10"])),
    ("locked/criteria-text.py", python(f"# invar-lock: SHOP-214 {THE_DIGEST} ac=AC2 written by the test-writer"),
     "trailing text after the list is ignored", locked(1, criteria=["AC2"])),
    ("locked/criteria-by-hand.py", python(f"# invar-lock: SHOP-214 {DIGEST_WITHOUT_LIST} ac=AC1"),
     "a list added by hand to a digest that does not cover it: the digest no longer matches",
     locked(1, matches=False, criteria=["AC1"])),

    ("error/two-lines.php", php("// invar-lock: SHOP-214") + "// invar-lock: SHOP-215\n", "two lock lines", error("more-than-one", 3, 11)),
    ("error/two-lines-far-apart.php", LONG_PHP + "// invar-lock: SHOP-215\n", "the second is outside the window", error("more-than-one", 3, 40)),
    ("error/line-11.py", padded("# invar-lock: SHOP-214", 10), "one line past the window", error("after-window", 11)),
    ("error/line-11-locked.py", padded(f"# invar-lock: SHOP-214 {WRONG_DIGEST}", 10), "moving the line down does not unlock the file", error("after-window", 11)),
    ("error/no-ticket.py", python("# invar-lock:"), "nothing after the marker", error("ticket", 1)),
    ("error/no-space.py", python("# invar-lock:SHOP-214"), "no space after the marker", error("ticket", 1)),
    ("error/tab.py", python("# invar-lock:\tSHOP-214"), "a tab is not a space", error("ticket", 1)),
    ("error/lowercase-ticket.py", python("# invar-lock: shop-214"), "not the ticket rule", error("ticket", 1)),
    ("error/ticket-no-number.py", python("# invar-lock: SHOP"), "not the ticket rule", error("ticket", 1)),
    ("error/ticket-glued-close.js", js("/* invar-lock: SHOP-214*/"), "trailing text needs whitespace", error("ticket", 1)),
    ("error/digest-first.py", python(f"# invar-lock: {WRONG_DIGEST} SHOP-214"), "the digest where the ticket goes", error("ticket", 1)),
    ("error/short-digest.py", python(f"# invar-lock: SHOP-214 {SHORT_DIGEST}"), "a truncated digest is not a pending line", error("digest", 1)),
    ("error/upper-digest.py", python(f"# invar-lock: SHOP-214 {UPPER_DIGEST}"), "the hex is lowercase", error("digest", 1)),
    ("error/empty-digest.py", python("# invar-lock: SHOP-214 sha256:"), "nothing after `sha256:`", error("digest", 1)),
    ("error/digest-glued-close.js", js(f"/* invar-lock: SHOP-214 {WRONG_DIGEST}*/"), "trailing text needs whitespace", error("digest", 1)),
    ("error/digest-after-text.py", python(f"# invar-lock: SHOP-214 by-hand {WRONG_DIGEST}"), "a digest out of place is not trailing text", error("digest", 1)),
    ("error/digest-after-tab.py", python(f"# invar-lock: SHOP-214\t{WRONG_DIGEST}"), "fields are separated by spaces", error("digest", 1)),
    ("error/two-digests.py", python(f"# invar-lock: SHOP-214 {WRONG_DIGEST} {WRONG_DIGEST}"), "one digest only", error("digest", 1)),
    ("error/criteria-pending.py", python("# invar-lock: SHOP-214 ac=AC1"), "a pending line has no list", error("criteria", 1)),
    ("error/criteria-empty.py", python(f"# invar-lock: SHOP-214 {WRONG_DIGEST} ac="), "an empty list", error("criteria", 1)),
    ("error/criteria-lowercase.py", python(f"# invar-lock: SHOP-214 {WRONG_DIGEST} ac=ac1"), "ids are written `AC<n>`", error("criteria", 1)),
    ("error/criteria-leading-zero.py", python(f"# invar-lock: SHOP-214 {WRONG_DIGEST} ac=AC01"), "no leading zero", error("criteria", 1)),
    ("error/criteria-zero.py", python(f"# invar-lock: SHOP-214 {WRONG_DIGEST} ac=AC0"), "a positive number", error("criteria", 1)),
    ("error/criteria-unsorted.py", python(f"# invar-lock: SHOP-214 {WRONG_DIGEST} ac=AC2,AC1"), "sorted by number", error("criteria", 1)),
    ("error/criteria-text-sorted.py", python(f"# invar-lock: SHOP-214 {WRONG_DIGEST} ac=AC10,AC2"), "by number, not as text", error("criteria", 1)),
    ("error/criteria-repeat.py", python(f"# invar-lock: SHOP-214 {WRONG_DIGEST} ac=AC1,AC1"), "no repeat", error("criteria", 1)),
    ("error/criteria-space.py", python(f"# invar-lock: SHOP-214 {WRONG_DIGEST} ac=AC1, AC2"), "no spaces in the list", error("criteria", 1)),
    ("error/criteria-glued-close.js", js(f"/* invar-lock: SHOP-214 {WRONG_DIGEST} ac=AC1*/"), "trailing text needs whitespace", error("criteria", 1)),
    ("error/criteria-after-tab.py", python(f"# invar-lock: SHOP-214 {WRONG_DIGEST}\tac=AC1"), "fields are separated by spaces", error("criteria", 1)),
    ("error/criteria-after-text.py", python(f"# invar-lock: SHOP-214 {WRONG_DIGEST} by-hand ac=AC1"), "a list out of place is not trailing text", error("criteria", 1)),
    ("error/criteria-twice.py", python(f"# invar-lock: SHOP-214 {WRONG_DIGEST} ac=AC1 ac=AC2"), "one list only", error("criteria", 1)),
    ("error/criteria-before-digest.py", python(f"# invar-lock: SHOP-214 ac=AC1 {WRONG_DIGEST}"), "the digest comes first, so this is a digest out of place", error("digest", 1)),
    ("error/invalid-utf8.py", b"# invar-lock: SHOP-214\n\xe9\n", "canonicalization's own error, with its offset", {"result": "error", "kind": "invalid-utf8", "offset": 23}),
]

# Each of these lists files that must produce one digest.
SAME_DIGEST = [
    # Line endings, a BOM and trailing whitespace change nothing, and neither
    # does the state of the line: locking does not change what is covered.
    ["locked/php-lf.php", "locked/php-crlf.php", "locked/php-cr.php", "locked/php-bom-crlf-ws.php",
     "locked/mismatch.php", "pending/php.php", "pending/crlf.php"],
    # The prefix and the trailing text are part of the line, and so not covered.
    ["pending/python.py", "pending/trailing-text.py", "pending/many-spaces.py", "locked/python.py"],
    ["pending/js-block.js", "locked/js-block.js"],
]


def content_bytes(content: str | bytes, expected: dict) -> bytes:
    """The fixture's bytes, with the placeholder replaced by its own digest."""
    if isinstance(content, bytes):
        return content
    if THE_DIGEST in content:
        digest = lock_digest(content, expected["line"], expected.get("criteria"))
        content = content.replace(THE_DIGEST, digest)
    if DIGEST_WITHOUT_LIST in content:
        content = content.replace(DIGEST_WITHOUT_LIST, lock_digest(content, expected["line"]))
    return content.encode("utf-8")


def build_manifest() -> dict:
    entries = []
    digests = {}
    for path, content, note, expected in FIXTURES:
        raw = content_bytes(content, expected)
        entry = {"path": path, "bytes": len(raw), "note": note, **expected}
        if expected["result"] in ("pending", "locked"):
            text = raw.decode("utf-8")
            digest = lock_digest(text, expected["line"], expected.get("criteria"))
            entry["digest"] = digest
            digests[path] = digest
            if expected["result"] == "locked":
                if expected["matches"]:
                    recorded = digest
                elif DIGEST_WITHOUT_LIST in content:
                    recorded = lock_digest(text, expected["line"])
                else:
                    recorded = WRONG_DIGEST
                entry["recorded"] = recorded
        entries.append(entry)

    for paths in SAME_DIGEST:
        if len({digests[path] for path in paths}) != 1:
            raise SystemExit(f"these do not share a digest: {paths}")

    return {
        "schema": SCHEMA,
        "version": FIXTURES_VERSION,
        "algorithm": "sha256",
        "digest_format": "sha256:<64 lowercase hex>",
        "contract": (
            "contracts/formats/acceptance-lock.md. Every file must give the result "
            "listed for it, and each list in same_digest must give one digest. "
            "Expected values are regenerated deliberately, never as part of a code change."
        ),
        "same_digest": SAME_DIGEST,
        "fixtures": entries,
    }


def cmd_write() -> int:
    for path, content, _, expected in FIXTURES:
        target = ROOT / path
        target.parent.mkdir(parents=True, exist_ok=True)
        # Bytes, so nothing between here and the disk can reinterpret a terminator.
        target.write_bytes(content_bytes(content, expected))
        print(f"wrote {path}")
    return 0


def cmd_update_expected() -> int:
    MANIFEST.write_text(
        json.dumps(build_manifest(), indent=2, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )
    print(f"wrote {MANIFEST.relative_to(ROOT)}")
    print("This is a contract change: bump the version and record the reason.")
    return 0


def cmd_check() -> int:
    if not MANIFEST.exists():
        print(f"missing {MANIFEST}; run --update-expected", file=sys.stderr)
        return 1

    problems: list[str] = []
    recorded = json.loads(MANIFEST.read_text(encoding="utf-8"))
    if recorded != build_manifest():
        problems.append("locks.json is not what the table computes")

    for path, content, _, expected in FIXTURES:
        target = ROOT / path
        if not target.exists():
            problems.append(f"{path}: missing; run --write")
            continue
        want = content_bytes(content, expected)
        got = target.read_bytes()
        if got != want:
            problems.append(
                f"{path}: on disk {len(got)} bytes, table says {len(want)} -- something "
                "rewrote this fixture (check .gitattributes and your editor)"
            )

    if problems:
        print("lock fixtures FAILED:", file=sys.stderr)
        for problem in problems:
            print(f"  - {problem}", file=sys.stderr)
        return 1

    print(f"lock fixtures OK: {len(FIXTURES)} files")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--check", action="store_true", help="verify files and locks.json (default)")
    mode.add_argument("--write", action="store_true", help="(re)write the fixture files")
    mode.add_argument(
        "--update-expected",
        action="store_true",
        help="recompute locks.json -- a deliberate contract change",
    )
    args = parser.parse_args()

    if args.write:
        return cmd_write()
    if args.update_expected:
        return cmd_update_expected()
    return cmd_check()


if __name__ == "__main__":
    raise SystemExit(main())
