#!/usr/bin/env python3
"""Validate CHANGELOG.md structure. Usage: check-changelog.py [PATH]."""

import re
import sys

ORDER = ["Added", "Changed", "Fixed", "Removed", "Security"]
VERSION_RE = re.compile(r"^## \[(\d+\.\d+\.\d+)\] - (\d{4}-\d{2}-\d{2})$")
REF_RE = re.compile(r"(\(#\d+\)|\[#\d+\])")


def fail(msg):
    print(f"error: {msg}", file=sys.stderr)
    sys.exit(1)


def main():
    path = sys.argv[1] if len(sys.argv) > 1 else "CHANGELOG.md"
    with open(path) as fh:
        lines = [line.rstrip("\n") for line in fh]

    text = [line for line in lines if line.strip()]
    if not text or text[0] != "# Changelog":
        fail("file must start with '# Changelog'")
    if "## [Unreleased]" not in text:
        fail("file must contain '## [Unreleased]'")

    section = None
    subsection = None
    seen_order = []
    has_bullet = False
    versioned = False
    in_draft = False

    def close_subsection():
        if subsection and versioned and not has_bullet:
            fail(f"empty section '### {subsection}'")

    for line in lines:
        if line.startswith("## "):
            close_subsection()
            subsection = None
            seen_order = []
            if line == "## [Unreleased]":
                section = "unreleased"
                versioned = False
            elif VERSION_RE.match(line):
                section = "version"
                versioned = True
            else:
                fail(f"bad release heading: {line}")
        elif line.startswith("### "):
            if section is None:
                fail(f"subsection outside release: {line}")
            close_subsection()
            name = line[4:].strip()
            if name not in ORDER:
                fail(f"unknown subsection: {line}")
            if name in seen_order:
                fail(f"duplicate subsection: {line}")
            seen_order = seen_order + [name]
            if seen_order != sorted(seen_order, key=ORDER.index):
                fail(f"subsections out of order at: {line}")
            subsection = name
            has_bullet = False
        elif line.startswith("- "):
            if subsection is None:
                if not (in_draft and section == "unreleased"):
                    fail(f"bullet outside subsection: {line}")
            else:
                has_bullet = True
                if versioned and not REF_RE.search(line):
                    fail(f"bullet lacks (#123) reference: {line}")
        elif line.startswith("<!-- changelog-draft:start -->"):
            in_draft = True
        elif line.startswith("<!-- changelog-draft:end -->"):
            in_draft = False

    close_subsection()
    print("changelog OK")


main()
