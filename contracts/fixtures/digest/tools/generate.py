#!/usr/bin/env python3
"""Materialize and verify the digest fixtures of contracts/fixtures/digest.

The fixtures deliberately contain bytes that ordinary tooling destroys: CRLF
and lone CR terminators, trailing whitespace, a UTF-8 BOM, a missing final
newline. Editors strip them, formatters "fix" them, and git normalizes them
unless `.gitattributes` says otherwise. They therefore cannot be maintained by
hand -- this script writes them from an explicit byte-level table.

Modes:

    python3 generate.py --check             verify files and digests (default)
    python3 generate.py --write             (re)write the fixture FILES
    python3 generate.py --update-digests    recompute digests.json  DELIBERATE

`--check` is the default and is what CI runs. `--update-digests` must be passed
explicitly: an expected value that silently follows a code change tests
nothing (see README.md).

Standard library only, so it runs anywhere without a virtualenv.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
MANIFEST = ROOT / "digests.json"

SCHEMA = "invar.digest-fixtures/v1"
FIXTURES_VERSION = "1.0.0"

# --------------------------------------------------------------------------
# Reference canonicalization -- DESIGN.md section 4d, steps 1-4.
#
# Step 5 (body extraction) is a FORMAT concern and is deliberately absent:
# these fixtures test canonicalization on its own, which is only possible if
# nothing here knows what a .feature or a Plan looks like.
# --------------------------------------------------------------------------


# Unicode White_Space (DESIGN.md 4d step 3, README D-2). Spelled out rather
# than taken from str.rstrip(), which also strips U+001C-U+001F. U+000A and
# U+000D are included for completeness; after step 2 neither can occur inside
# a line.
UNICODE_WHITE_SPACE = (
    "\u0009\u000a\u000b\u000c\u000d\u0020\u0085\u00a0\u1680"
    "\u2000\u2001\u2002\u2003\u2004\u2005\u2006\u2007\u2008\u2009\u200a"
    "\u2028\u2029\u202f\u205f\u3000"
)


def canonicalize(raw: bytes) -> str:
    text = raw.decode("utf-8")               # 1. decode as UTF-8
    if text.startswith("﻿"):            #    strip a BOM if present
        text = text[1:]
    text = text.replace("\r\n", "\n").replace("\r", "\n")   # 2. line endings
    lines = [line.rstrip(UNICODE_WHITE_SPACE) for line in text.split("\n")]  # 3. trailing ws
    while lines and lines[-1] == "":         # 4. strip trailing blank lines
        lines.pop()
    if not lines:                            #    D-1: empty stays empty
        return ""
    return "\n".join(lines) + "\n"           #    exactly one final newline


def digest(canonical_text: str) -> str:
    return "sha256:" + hashlib.sha256(canonical_text.encode("utf-8")).hexdigest()


# --------------------------------------------------------------------------
# The fixture table. Content is written with explicit escapes so the bytes are
# visible in the source; never paste raw CR characters in here.
# --------------------------------------------------------------------------

FEATURE_BODY_LF = (
    '  Scenario: Stock is reserved when an order is placed\n'
    '    Given a Product with sku "ABC-123" and stock 10\n'
    '    When a customer places an order for 3 units of "ABC-123"\n'
    '    Then the Product stock for "ABC-123" is 7\n'
)

GROUPS: list[tuple[str, str, list[tuple[str, str, str]]]] = [
    (
        "simple",
        "Two plain lines. Every file here differs only in line endings or in "
        "trailing whitespace, so all of them must produce one digest. This is "
        "the group that proves a mixed Windows/Unix team gets the same "
        "checksum for identical content.",
        [
            ("lf.txt", "alpha\nbeta\n", "LF terminators; already canonical"),
            ("crlf.txt", "alpha\r\nbeta\r\n", "CRLF terminators (Windows, or core.autocrlf=true)"),
            ("cr.txt", "alpha\rbeta\r", "lone CR terminators (classic Mac)"),
            ("mixed.txt", "alpha\r\nbeta\n", "CRLF and LF mixed in one file"),
            ("no-final-newline.txt", "alpha\nbeta", "no terminator on the last line"),
            ("trailing-spaces.txt", "alpha   \nbeta \n", "trailing spaces on every line"),
            ("trailing-tabs.txt", "alpha\t\nbeta\t\t\n", "trailing tabs"),
            ("trailing-blank-lines.txt", "alpha\nbeta\n\n\n\n", "three blank lines at end of file"),
            ("bom-lf.txt", "﻿alpha\nbeta\n", "UTF-8 BOM, LF"),
            ("bom-crlf-trailing-ws.txt", "﻿alpha  \r\nbeta\t\r\n\r\n",
             "BOM, CRLF, trailing whitespace and a trailing blank line at once"),
        ],
    ),
    (
        "empty",
        "An empty result canonicalizes to the EMPTY STRING, not to a single "
        "newline (README, D-1). Its digest is the well-known sha256 of zero "
        "bytes, which makes this group a self-check on the implementation.",
        [
            ("empty.txt", "", "zero bytes"),
            ("newlines-only.txt", "\n\n\n", "only line terminators"),
            ("whitespace-only.txt", "   \n\t\n  ", "only whitespace, no content"),
            ("bom-only.txt", "﻿", "a BOM and nothing else"),
            ("bom-and-blanks.txt", "﻿\r\n  \r\n", "BOM plus blank lines"),
        ],
    ),
    (
        "interior-blank-lines",
        "Blank lines INSIDE the text are content and are preserved. This "
        "group must not share a digest with 'simple' -- if it does, the "
        "implementation is collapsing blank lines everywhere instead of only "
        "at the end.",
        [
            ("lf.txt", "alpha\n\nbeta\n", "one blank line between the two lines"),
            ("crlf.txt", "alpha\r\n\r\nbeta\r\n", "same content, CRLF"),
            ("crlf-trailing-ws.txt", "alpha\r\n   \r\nbeta   \r\n\r\n",
             "the interior blank line carries spaces; it stays a blank line"),
        ],
    ),
    (
        "leading-blank-lines",
        "Leading blank lines are preserved; only TRAILING ones are stripped. "
        "This asymmetry is the reason both format contracts start a body at "
        "its first non-blank line.",
        [
            ("lf.txt", "\nalpha\nbeta\n", "one blank line at the start"),
            ("crlf-ws.txt", "   \r\nalpha\r\nbeta\r\n\r\n",
             "leading blank line with spaces kept; trailing blank line stripped"),
        ],
    ),
    (
        "indentation",
        "Leading whitespace on a line is content; trailing whitespace is not. "
        "Gherkin is indented, so getting this backwards would change the "
        "digest of every .feature body.",
        [
            ("lf.txt", "alpha\n    beta\n\tgamma\n", "spaces and a tab as indentation"),
            ("crlf-trailing-ws.txt", "alpha   \r\n    beta\t\r\n\tgamma  \r\n",
             "same indentation, trailing whitespace added"),
        ],
    ),
    (
        "non-ascii",
        "Multi-byte UTF-8 must survive canonicalization byte for byte. "
        "Serbian Latin and Cyrillic, plus an emoji, so a byte-oriented "
        "trailing-whitespace strip that splits a code point shows up here.",
        [
            ("lf.txt", "Grad: Beograd\nĆirilica: Београд\nkafa: ☕\n", "LF"),
            ("crlf.txt", "Grad: Beograd\r\nĆirilica: Београд\r\nkafa: ☕\r\n", "CRLF"),
            ("bom-crlf-ws.txt",
             "﻿Grad: Beograd  \r\nĆirilica: Београд\t\r\nkafa: ☕\r\n\r\n",
             "BOM, CRLF, trailing whitespace, trailing blank line"),
        ],
    ),
    (
        "feature-body",
        "A realistic .feature body -- the actual thing a Confirmed-checksum "
        "is computed over (section 4). Proves the equivalence on real content, "
        "not only on toy input.",
        [
            ("lf.feature", FEATURE_BODY_LF, "LF"),
            ("crlf.feature", FEATURE_BODY_LF.replace("\n", "\r\n"), "CRLF"),
            ("crlf-no-final-newline.feature",
             FEATURE_BODY_LF.replace("\n", "\r\n").rstrip("\r\n"),
             "CRLF and no terminator on the last line"),
        ],
    ),
]


def build_manifest() -> dict:
    groups = []
    for name, description, files in GROUPS:
        digests = {digest(canonicalize(content.encode("utf-8"))) for _, content, _ in files}
        if len(digests) != 1:
            raise SystemExit(
                f"group '{name}' does not agree with itself: {sorted(digests)}\n"
                "Every file in a group must canonicalize to the same text."
            )
        expected = digests.pop()
        canonical = canonicalize(files[0][1].encode("utf-8"))
        groups.append(
            {
                "name": name,
                "description": description,
                "expected_digest": expected,
                "canonical_byte_length": len(canonical.encode("utf-8")),
                "files": [
                    {"path": f"{name}/{fname}", "bytes": len(content.encode("utf-8")), "note": note}
                    for fname, content, note in files
                ],
            }
        )

    by_digest: dict[str, str] = {}
    for group in groups:
        other = by_digest.get(group["expected_digest"])
        if other is not None:
            raise SystemExit(
                f"groups '{other}' and '{group['name']}' share a digest; "
                "each group must describe distinct content"
            )
        by_digest[group["expected_digest"]] = group["name"]

    return {
        "schema": SCHEMA,
        "version": FIXTURES_VERSION,
        "algorithm": "sha256",
        "digest_format": "sha256:<64 lowercase hex>",
        "canonicalization": (
            "docs/DESIGN.md section 4d, steps 1-4. Step 5 (body extraction) is a "
            "format concern and is not exercised by these fixtures."
        ),
        "contract": (
            "Every file in a group must produce that group's expected_digest, and "
            "no two groups may share one. Digests are regenerated deliberately, "
            "never as part of a code change."
        ),
        "groups": groups,
    }


def cmd_write() -> int:
    for name, _, files in GROUPS:
        (ROOT / name).mkdir(parents=True, exist_ok=True)
        for fname, content, _ in files:
            path = ROOT / name / fname
            # newline="" is not enough -- write bytes, so nothing between here
            # and the disk can reinterpret a terminator.
            path.write_bytes(content.encode("utf-8"))
            print(f"wrote {path.relative_to(ROOT)} ({len(content.encode('utf-8'))} bytes)")
    return 0


def cmd_update_digests() -> int:
    MANIFEST.write_text(
        json.dumps(build_manifest(), indent=2, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )
    print(f"wrote {MANIFEST.relative_to(ROOT)}")
    print("This is a contract change: bump the version and record the reason.")
    return 0


def cmd_check() -> int:
    if not MANIFEST.exists():
        print(f"missing {MANIFEST}; run --update-digests", file=sys.stderr)
        return 1

    recorded = json.loads(MANIFEST.read_text(encoding="utf-8"))
    problems: list[str] = []

    if recorded.get("schema") != SCHEMA:
        problems.append(f"schema is {recorded.get('schema')!r}, expected {SCHEMA!r}")

    expected_by_group = {g["name"]: g for g in build_manifest()["groups"]}
    recorded_by_group = {g["name"]: g for g in recorded.get("groups", [])}

    for name in sorted(set(expected_by_group) | set(recorded_by_group)):
        want = expected_by_group.get(name)
        have = recorded_by_group.get(name)
        if want is None:
            problems.append(f"{name}: in digests.json but not in the table")
            continue
        if have is None:
            problems.append(f"{name}: in the table but not in digests.json")
            continue
        if want["expected_digest"] != have["expected_digest"]:
            problems.append(
                f"{name}: digests.json says {have['expected_digest']}, "
                f"the table computes {want['expected_digest']}"
            )

    # The files on disk must match the table byte for byte. This is what
    # catches an editor or a git filter having "fixed" a fixture.
    for name, _, files in GROUPS:
        for fname, content, _ in files:
            path = ROOT / name / fname
            want_bytes = content.encode("utf-8")
            if not path.exists():
                problems.append(f"{name}/{fname}: missing; run --write")
                continue
            got = path.read_bytes()
            if got != want_bytes:
                problems.append(
                    f"{name}/{fname}: on disk {len(got)} bytes, table says "
                    f"{len(want_bytes)} -- something rewrote this fixture "
                    "(check .gitattributes and your editor)"
                )

    if problems:
        print("digest fixtures FAILED:", file=sys.stderr)
        for problem in problems:
            print(f"  - {problem}", file=sys.stderr)
        return 1

    total = sum(len(files) for _, _, files in GROUPS)
    print(f"digest fixtures OK: {len(GROUPS)} groups, {total} files")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--check", action="store_true", help="verify files and digests (default)")
    mode.add_argument("--write", action="store_true", help="(re)write the fixture files")
    mode.add_argument(
        "--update-digests",
        action="store_true",
        help="recompute digests.json -- a deliberate contract change",
    )
    args = parser.parse_args()

    if args.write:
        return cmd_write()
    if args.update_digests:
        return cmd_update_digests()
    return cmd_check()


if __name__ == "__main__":
    raise SystemExit(main())
