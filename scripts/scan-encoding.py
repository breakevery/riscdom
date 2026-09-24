#!/usr/bin/env python3
"""One-off diagnostic: look for the 0x3F class of accidents in the sources.

WINDOWS ENCODING ACCIDENTS, AND WHY ONLY THE CERTAIN CLASSES FAIL THE GATE
-------------------------------------------------------------------------

On Windows a non-ASCII character can be eaten before it ever reaches a file or a
commit message: the console's code page replaces it with `?` (0x3F). CONTRIBUTING.md
records the measured case (a commit subject of Chinese text stored as
`test: ?????????`), and v0.5's walkthrough found the consequence in the UI — a tool
row rendered `?? write_source ?` because the emoji in those JSX lines had been
written as literal `?` characters (fixed in v0.5 batch 11).

A second accident is silently *worse*, because nothing about it looks wrong in an
editor: **Windows PowerShell 5.1 reads a BOM-less UTF-8 file as GBK and writes the
text back as UTF-8**, which turns `E2 80 xx` (an em dash, an ellipsis) into `U+9225`
plus a lost byte, turns `C2 A7` (`§`) into `U+6402`, and — via
`Set-Content -Encoding utf8` — adds a BOM. It happened twice here (v0.7 in
`sandbox/`, D2b-1 in `ui/src/api/`) and no check saw it, because every damaged
character sat inside a comment.

This script looks for both classes of damage across the tree, so a future session can
answer "did an encoding accident land here?" in one command.

**`--check` runs in `scripts/gate.sh`; the other modes do not.** The gate fails only
on the classes that can be *certain* — E (UTF-8 read as GBK) and F (a BOM) — because
those two are never intentional. The rest stay diagnostic:

1. **C is not reliably decidable.** "A string literal that is nothing but `?`" cannot
   be told apart from legitimate code: a ternary `cond ? "a" : "b"` produces `" ? "`
   tokens on the same line, and `"?"` is a perfectly good literal for URL parsing and
   for "unknown" fallbacks. Measured on this repository (v0.5 batch 12): 13 hits,
   every one a false positive.
2. **A guard that cries wolf erodes the gate.** The gate is the one list of what
   "green" means (see docs/handoff.md §4). Everything in it must be trustworthy; a
   check that fails on healthy code teaches people to ignore failures. So B, C and D
   keep reporting and never block (decision §60), and `--check`'s scope is the two
   classes that cannot be a false positive.

There is no write mode at all: the script only ever reports.

USAGE
-----

    python3 scripts/scan-encoding.py                    # scan the repository, report
    python3 scripts/scan-encoding.py --check            # the gate's mode: fail on E or F
    python3 scripts/scan-encoding.py --summary          # counts per class only
    python3 scripts/scan-encoding.py --class A --class E  # only these classes
    python3 scripts/scan-encoding.py --root .           # scan somewhere else
    python3 scripts/scan-encoding.py --ext .rs .ts      # override the extension set

Exit status: `0` in every diagnostic mode, whatever was found; with `--check`, `1`
when class E or class F has a hit. (Nothing else here is allowed to make a build
fail — see above.)
"""

from __future__ import annotations

import argparse
import io
import os
import re
import sys

# Where the accident can hide: source and documentation, in the languages this
# repository uses. Data files (locks) are included because a generator writing them
# is the same pipeline that writes everything else. `.mjs` / `.js` are not listed yet:
# `ui/scripts/*.mjs` is the one place they matter, and it is probes (which the
# repository writes by hand).
DEFAULT_EXT = (".rs", ".ts", ".tsx", ".css", ".json", ".md", ".py", ".sh", ".ps1")

# Directories that are generated, vendored, or scratch. `gen/` is Tauri's generated
# schema output; `.cowork-temp/` is the session scratch directory.
SKIP_DIRS = {"target", "node_modules", "dist", ".git", ".cowork-temp", "gen"}

# Class C: a quoted literal whose content is only '?' and blanks (" ?", "?", "??").
LONE_QMARK = re.compile(r"""(["'])(\s*\?+\s*)\1""")
# Class D: an ASCII '?' immediately next to a CJK ideograph (the fullwidth '？' lost
# its width).
CJK_QMARK = re.compile(r"[\u4e00-\u9fff]\s?\?(?![\w/&])|\?(?=\s?[\u4e00-\u9fff])")
# Class E: UTF-8 bytes decoded as GBK/CP936 leave these lead sequences behind.
# The list is the v0.5 accident's, and it is left exactly as it was measured: the
# section sign's residue is appended from an escape below rather than typed in here,
# so this file never has to be edited *inside* its own pattern.
MOJIBAKE = re.compile(r"(鈥|锛|鐨|璁|鏂|绋|鏄|涓|鍜|鍏|鏈|瀹|鍐|鎴|鍔|鏃|鐢|鐩|姝|閿|鍚|瑕|鍙)")
# `C2 A7` (`§`) read as GBK is `U+6402`; it is not in the list above because that list
# came from a different accident's samples (v0.9, decision §59).
MOJIBAKE_EXTRA = re.compile(r"[\u6402]")
# What class E actually tests: the measured list, plus the shapes added since.
MOJIBAKE_ALL = re.compile(MOJIBAKE.pattern + "|" + MOJIBAKE_EXTRA.pattern)
# Class B: three or more '?' in a row — several characters eaten at once.
TRIPLE = re.compile(r"\?\?\?")

CLASSES = {
    "A": "U+FFFD REPLACEMENT CHARACTER (a decode already failed)",
    "B": "three or more consecutive '?' (several characters eaten at once)",
    "C": "a string literal that is only '?'s (a symbol was replaced)",
    "D": "an ASCII '?' next to CJK text (a fullwidth '？' lost its width)",
    "E": "UTF-8 read as GBK (text double-encoded somewhere)",
    "F": "the file starts with a BOM (a byte-order mark written by a tool)",
}

VERDICT_HINT = {
    "A": "almost always real: U+FFFD is never written on purpose",
    "B": "check the line: documentation about this accident quotes the damage",
    "C": "usually legitimate — ternaries and '?' literals look identical to damage",
    "D": "almost always real: this repository writes '？' in Chinese prose",
    "E": "almost always real: mojibake is never intentional",
    "F": "always real: nothing in this repository wants a BOM",
}

# The classes `--check` fails on, and the only two that cannot be a false positive.
CERTAIN = ("E", "F")


def scan_file(path: str, classes: set[str], hits: list[tuple]) -> None:
    try:
        text = io.open(path, encoding="utf-8").read()
    except UnicodeDecodeError as exc:
        if "A" in classes:
            hits.append((path, 0, "", "A", f"not valid UTF-8: {exc}"))
        return
    for number, line in enumerate(text.split("\n"), 1):
        if "A" in classes and "\ufffd" in line:
            hits.append((path, number, line, "A", "U+FFFD replacement character"))
        if "B" in classes and TRIPLE.search(line):
            hits.append((path, number, line, "B", "3+ consecutive '?'"))
        if "C" in classes:
            for match in LONE_QMARK.finditer(line):
                hits.append(
                    (path, number, line, "C", f"string literal {match.group(0)!r} is only '?'s")
                )
        if "D" in classes and CJK_QMARK.search(line):
            hits.append((path, number, line, "D", "ASCII '?' next to CJK text"))
        if "E" in classes and MOJIBAKE_ALL.search(line):
            hits.append((path, number, line, "E", "UTF-8-as-GBK mojibake"))
    # A BOM is a property of the file, not of a line: it is the first character.
    if "F" in classes and text.startswith("\ufeff"):
        hits.append((path, 1, text.split("\n")[0], "F", "file starts with a BOM (U+FEFF)"))


def walk(root: str, ext: tuple[str, ...]):
    here = os.path.abspath(__file__)
    for dirpath, dirnames, filenames in os.walk(root):
        dirnames[:] = [d for d in dirnames if d not in SKIP_DIRS]
        for name in filenames:
            if name.endswith(ext):
                full = os.path.join(dirpath, name)
                # This script's own mojibake list *quotes* the accident on purpose, so
                # class E would hit it forever. A scanner skips its own pattern table.
                if os.path.abspath(full) == here:
                    continue
                yield full


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Report the 0x3F class of encoding accidents (diagnostic only).",
    )
    parser.add_argument("--root", default=os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
                        help="tree to scan (default: this repository)")
    parser.add_argument("--ext", nargs="+", default=list(DEFAULT_EXT),
                        help=f"extensions to scan (default: {' '.join(DEFAULT_EXT)})")
    parser.add_argument("--class", dest="classes", action="append", choices=sorted(CLASSES),
                        help="only this class (repeatable; default: all)")
    parser.add_argument("--summary", action="store_true", help="counts only, no context")
    parser.add_argument("--check", action="store_true",
                        help="fail (exit 1) when class E or class F has a hit; the gate's mode")
    args = parser.parse_args()

    classes = set(args.classes) if args.classes else set(CLASSES)
    hits: list[tuple] = []
    for path in walk(args.root, tuple(args.ext)):
        scan_file(path, classes, hits)

    root = os.path.abspath(args.root)
    shown = sorted(classes)
    print(f"scan-encoding: {root}")
    print(f"  extensions: {' '.join(args.ext)}")
    print(f"  classes:    {', '.join(shown)}")
    print(f"  skipped:    {' '.join(sorted(SKIP_DIRS))}")

    if args.summary:
        print()
        for cls in shown:
            count = sum(1 for h in hits if h[3] == cls)
            print(f"  {cls}: {count}")
        print(f"  total: {len(hits)}")
        return check_verdict(hits, shown) if args.check else 0

    for cls in shown:
        group = [h for h in hits if h[3] == cls]
        print(f"\n=== class {cls}: {len(group)} hit(s) ===")
        print(f"     {CLASSES[cls]}")
        print(f"     {VERDICT_HINT[cls]}")
        for path, number, line, _cls, why in group:
            shown_path = os.path.relpath(path, root).replace(os.sep, "/")
            print(f"  {shown_path}:{number}: {why}")
            print(f"      {line.strip()[:160]}")
    print(f"\ntotal: {len(hits)} hit(s)")
    print("(diagnostic only — nothing was modified)")
    return check_verdict(hits, shown) if args.check else 0


def check_verdict(hits: list[tuple], shown: list[str]) -> int:
    """`--check`'s answer: the certain classes decide, the rest only report."""
    certain = [cls for cls in CERTAIN if cls in shown]
    failed = [h for h in hits if h[3] in certain]
    label = ", ".join(certain) if certain else "(none of the certain classes were scanned)"
    if failed:
        print(f"check: {len(failed)} hit(s) in a class that cannot be a false positive ({label})")
        return 1
    print(f"check: OK ({label})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
