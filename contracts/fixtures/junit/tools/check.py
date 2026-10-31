#!/usr/bin/env python3
"""Verify the JUnit fixtures of contracts/fixtures/junit.

The reports in real/ were written by the real runners and cannot be regenerated
here, so unlike the digest set this script materializes nothing. It checks them.

Modes:

    python3 check.py --check         verify the reports and results.json (default)
    python3 check.py --update-sums   recompute sums.json  DELIBERATE

Standard library only, so it runs anywhere without a virtualenv.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
import xml.etree.ElementTree as ElementTree
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
RESULTS = ROOT / "results.json"
SUMS = ROOT / "sums.json"

SCHEMA = "invar.junit-fixtures/v1"
FIXTURES_VERSION = "1.0.0"

MACHINE_PATH = re.compile(rb"/home/|/Users/|/work/|/tmp/|[A-Za-z]:\\Users")

# The worst one decides (test-results.md D-5).
PRECEDENCE = [("error", "error"), ("failure", "failure"), ("skipped", "skipped")]


def reports() -> list[str]:
    found = []
    for directory in ("real", "synthetic"):
        found += [
            path.relative_to(ROOT).as_posix()
            for path in (ROOT / directory).rglob("*")
            if path.is_file()
        ]
    return sorted(found)


def sha256(report: str) -> str:
    return hashlib.sha256((ROOT / report).read_bytes()).hexdigest()


def cmd_update_sums() -> int:
    sums = {report: sha256(report) for report in reports()}
    SUMS.write_text(json.dumps(sums, indent=2) + "\n", encoding="utf-8")
    print(f"wrote {SUMS.relative_to(ROOT)}")
    print("A report changed: say why. A real report is replaced, never edited.")
    return 0


def outcome_of(case: ElementTree.Element) -> str:
    children = {child.tag for child in case}
    for tag, outcome in PRECEDENCE:
        if tag in children:
            return outcome
    return "passed"


def test_cases(element: ElementTree.Element) -> list[ElementTree.Element]:
    """Test cases whose every ancestor is a suite (test-results.md section 2)."""
    found = []
    for child in element:
        if child.tag == "testcase":
            found.append(child)
        elif child.tag in ("testsuite", "testsuites"):
            found += test_cases(child)
    return found


def check_cases(report: str, expected: list[dict], problems: list[str]) -> None:
    try:
        root = ElementTree.fromstring((ROOT / report).read_bytes())
    except ElementTree.ParseError as err:
        problems.append(f"{report}: does not parse, and results.json expects cases: {err}")
        return
    found = [(case.get("name"), outcome_of(case)) for case in test_cases(root)]
    wanted = [(case["name"], case["outcome"]) for case in expected]
    if found != wanted:
        problems.append(f"{report}: the XML holds {found}, results.json says {wanted}")


def cmd_check() -> int:
    problems: list[str] = []
    results = json.loads(RESULTS.read_text(encoding="utf-8"))
    if results.get("schema") != SCHEMA:
        problems.append(f"schema is {results.get('schema')!r}, expected {SCHEMA!r}")
    if results.get("version") != FIXTURES_VERSION:
        problems.append(f"version is {results.get('version')!r}, expected {FIXTURES_VERSION!r}")

    entries = {entry["report"]: entry for entry in results["reports"]}
    sums = json.loads(SUMS.read_text(encoding="utf-8")) if SUMS.exists() else {}
    on_disk = reports()

    for name, listing in (("results.json", set(entries)), ("sums.json", set(sums))):
        for report in sorted(set(on_disk) - listing):
            problems.append(f"{report}: on disk, and not in {name}")
        for report in sorted(listing - set(on_disk)):
            problems.append(f"{report}: in {name}, and not on disk")

    for report in on_disk:
        if report in sums and sha256(report) != sums[report]:
            problems.append(
                f"{report}: not the recorded bytes -- something rewrote this report "
                "(check .gitattributes and your editor)"
            )
        if MACHINE_PATH.search((ROOT / report).read_bytes()):
            problems.append(f"{report}: holds a machine path")

    for report, entry in entries.items():
        if report not in on_disk:
            continue
        if "same_cases_as" in entry:
            check_cases(report, entries[entry["same_cases_as"]]["cases"], problems)
        elif "cases" in entry:
            check_cases(report, entry["cases"], problems)
        elif "error" not in entry:
            problems.append(f"{report}: expects neither cases nor an error")

    if not any(report.startswith("real/pest-") for report in on_disk):
        problems.append("no real Pest report")
    if not any(report.startswith("real/phpunit-") for report in on_disk):
        problems.append("no real PHPUnit report")

    if problems:
        print("JUnit fixtures FAILED:", file=sys.stderr)
        for problem in problems:
            print(f"  - {problem}", file=sys.stderr)
        return 1

    print(f"JUnit fixtures OK: {len(on_disk)} reports")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--check", action="store_true", help="verify the reports (default)")
    mode.add_argument(
        "--update-sums",
        action="store_true",
        help="recompute sums.json -- a deliberate change",
    )
    args = parser.parse_args()
    return cmd_update_sums() if args.update_sums else cmd_check()


if __name__ == "__main__":
    raise SystemExit(main())
