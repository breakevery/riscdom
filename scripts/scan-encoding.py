#!/usr/bin/env python3
"""One-off diagnostic: look for the 0x3F class of accidents in the sources.

WINDOWS ENCODING ACCIDENTS, AND WHY THIS IS *NOT* IN THE GATE
-------------------------------------------------------------

On Windows a non-ASCII character can be eaten before it ever reaches a file or a
commit message: the console's code page replaces it with `?` (0x3F). CONTRIBUTING.md
records the measured case (a commit subject of Chinese text stored as
`test: ?????????`), and v0.5's walkthrough found the consequence in the UI — a tool
row rendered `?? write_source ?` because the emoji in those JSX lines had been
written as literal `?` characters (fixed in v0.5 batch 11).

This script looks for that class of damage across the tree, so a future session can
answer "did an encoding accident land here?" in one command.

**It is deliberately NOT wired into `scripts/gate.sh`.** Two reasons:

1. **C is not reliably decidable.** "A string literal that is nothing but `?`" cannot
   be told apart from legitimate code: a ternary `cond ? "a" : "b"` produces `" ? "`
   tokens on the same line, and `"?"` is a perfectly good literal for URL parsing and
   for "unknown" fallbacks. Measured on this repository (v0.5 batch 12): 13 hits,
   every one a false positive.
2. **A guard that cries wolf erodes the gate.** The gate is the one list of what
   "green" means (see docs/handoff.md §4). Everything in it must be trustworthy; a
   check that fails on healthy code teaches people to ignore failures. Classes A, B,
   D and E could be guarded some day (A, D and E found nothing here; B only matches
   documentation *about* the accident); C would have to be sharpened first.

So this stays a hand-run diagnostic. It only ever reports — there is no write mode at
all, which is the "dry run" people ask about.

USAGE
-----

    python3 scripts/scan-encoding.py                    # scan the repository, report
    python3 scripts/scan-encoding.py --summary          # counts per class only
    python3 scripts/scan-encoding.py --class A --class E  # only these classes
    python3 scripts/scan-encoding.py --root .           # scan somewhere else
    python3 scripts/scan-encoding.py --ext .rs .ts      # override the extension set

Exit status is 0 whether or not anything was found: this is a diagnostic, not a
check. (Nothing here is allowed to make a build fail — see above.)
"""

from __future__ import annotations

import argparse
import io
import os
import re
import sys

# Where the accident can hide: source and documentation, in the languages this
# repository uses. Data files (locks) are included because a generator writing them
# is the same pipeline that writes everything else.
DEFAULT_EXT = (".rs", ".ts", ".tsx", ".css", ".json", ".md")

# Directories that are generated, vendored, or scratch. `gen/` is Tauri's generated
# schema output; `.cowork-temp/` is the session scratch directory.
SKIP_DIRS = {"target", "node_modules", "dist", ".git", ".cowork-temp", "gen"}

# Class C: a quoted literal whose content is only '?' and blanks (" ?", "?", "??").
LONE_QMARK = re.compile(r"""(["'])(\s*\?+\s*)\1""")
# Class D: an ASCII '?' immediately next to a CJK ideograph (the fullwidth '？' lost
# its width).
CJK_QMARK = re.compile(r"[\u4e00-\u9fff]\s?\?(?![\w/&])|\?(?=\s?[\u4e00-\u9fff])")
# Class E: UTF-8 bytes decoded as GBK/CP936 leave these lead sequences behind.
MOJIBAKE = re.compile(r"(鈥|锛|鐨|璁|鏂|绋|鏄|涓|鍜|鍏|鏈|瀹|鍐|鎴|鍔|鏃|鐢|鐩|姝|閿|鍚|瑕|鍙)")
# Class B: three or more '?' in a row — several characters eaten at once.
TRIPLE = re.compile(r"\?\?\?")

CLASSES = {
    "A": "U+FFFD REPLACEMENT CHARACTER (a decode already failed)",
    "B": "three or more consecutive '?' (several characters eaten at once)",
    "C": "a string literal that is only '?'s (a symbol was replaced)",
    "D": "an ASCII '?' next to CJK text (a fullwidth '？' lost its width)",
    "E": "UTF-8 read as GBK (text double-encoded somewhere)",
}

VERDICT_HINT = {
    "A": "almost always real: U+FFFD is never written on purpose",
    "B": "check the line: documentation about this accident quotes the damage",
    "C": "usually legitimate — ternaries and '?' literals look identical to damage",
    "D": "almost always real: this repository writes '？' in Chinese prose",
    "E": "almost always real: mojibake is never intentional",
}


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
        if "E" in classes and MOJIBAKE.search(line):
            hits.append((path, number, line, "E", "UTF-8-as-GBK mojibake"))


def walk(root: str, ext: tuple[str, ...]):
    for dirpath, dirnames, filenames in os.walk(root):
        dirnames[:] = [d for d in dirnames if d not in SKIP_DIRS]
        for name in filenames:
            if name.endswith(ext):
                yield os.path.join(dirpath, name)


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
        return 0

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
    print("(diagnostic only — nothing was modified, and nothing here is a gate check)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
