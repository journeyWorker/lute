#!/usr/bin/env python3
"""Reject `pub(super)` and `pub(in …)` in the workspace's Rust sources.

Usage:
    scripts/check-visibility.py [ROOT]
    scripts/check-visibility.py --self-test

An item is private, `pub(crate)` or `pub` (docs/design/rust-coding-rules.md
§3). No clippy lint forbids the finer levels, so this script is the gate: it
scans ``crates/*/src/**/*.rs`` under ROOT (default: this checkout), ignores
comments, string and character literals, prints ``file:line: <visibility>``
for each `pub(super)` or `pub(in …)` it finds, and exits 1 if there is any.
Exit 0 means none; exit 2 is a usage error. ``--self-test`` checks the scanner
on synthetic sources and exits 0 when every case passes.
"""

from __future__ import annotations

import argparse
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
SOURCES = "crates/*/src/**/*.rs"

# `pub`, then `(`, then `super` or `in` as a whole word; whitespace (comments
# are already blanked) may sit between the tokens, also across lines.
FORBIDDEN = re.compile(r"\bpub\s*\(\s*(super\b|in\b[^)]*?)\s*\)")
IDENT_CHAR = re.compile(r"[A-Za-z0-9_]")
# A string-literal opener: optional `b`/`c` prefix, optional raw `r` with its
# hashes, then the quote.
STRING_OPEN = re.compile(r'(?:b|c)?(r(#*))?"')
# A character or byte literal (`'x'`, `'\n'`, `'\u{1F600}'`, `b'\''`). A `'`
# that does not start one opens a lifetime or a loop label.
CHAR_LITERAL = re.compile(r"'(?:\\(?:x[0-9A-Fa-f]{2}|u\{[0-9A-Fa-f_]+\}|.)|[^\\'\n])'")


def code_only(text: str) -> str:
    """Return ``text`` with comments and literals replaced by spaces.

    Newlines are kept, so offsets and line numbers match the input.
    """
    out = list(text)
    i, n = 0, len(text)

    def blank(start: int, end: int) -> None:
        for k in range(start, end):
            if out[k] != "\n":
                out[k] = " "

    while i < n:
        ch = text[i]
        if text.startswith("//", i):
            end = text.find("\n", i)
            end = n if end < 0 else end
            blank(i, end)
            i = end
        elif text.startswith("/*", i):
            depth, j = 1, i + 2
            while j < n and depth:
                if text.startswith("/*", j):
                    depth, j = depth + 1, j + 2
                elif text.startswith("*/", j):
                    depth, j = depth - 1, j + 2
                else:
                    j += 1
            blank(i, j)
            i = j
        elif ch in 'bcr"' and (i == 0 or not IDENT_CHAR.match(text[i - 1])) and (opener := STRING_OPEN.match(text, i)):
            j = opener.end()
            if opener.group(1) is not None:
                close = '"' + opener.group(2)
                end = text.find(close, j)
                j = n if end < 0 else end + len(close)
            else:
                while j < n and text[j] != '"':
                    j += 2 if text[j] == "\\" else 1
                j = min(j + 1, n)
            blank(i, j)
            i = j
        elif ch == "'":
            literal = CHAR_LITERAL.match(text, i)
            j = literal.end() if literal else i + 1
            blank(i, j)
            i = j
        else:
            i += 1
    return "".join(out)


def violations(text: str) -> list[tuple[int, str]]:
    """Return ``(line, visibility)`` for every forbidden visibility in ``text``."""
    masked = code_only(text)
    return [
        (masked.count("\n", 0, found.start()) + 1, f"pub({' '.join(found.group(1).split())})")
        for found in FORBIDDEN.finditer(masked)
    ]


def scan(root: pathlib.Path) -> int:
    files = sorted(root.glob(SOURCES))
    if not files:
        print(f"check-visibility: no files match {root / SOURCES}", file=sys.stderr)
        return 2
    count = 0
    for path in files:
        for line, visibility in violations(path.read_text(encoding="utf-8")):
            print(f"{path.relative_to(root)}:{line}: {visibility}")
            count += 1
    if count:
        print(
            f"check-visibility: {count} `pub(super)`/`pub(in …)` in {SOURCES}; "
            "use private, `pub(crate)` or `pub` (rust-coding-rules §3)",
            file=sys.stderr,
        )
        return 1
    print(f"check-visibility: OK ({len(files)} files, no `pub(super)`/`pub(in …)`)")
    return 0


# (name, source, expected (line, visibility) list)
SELF_TEST_CASES: list[tuple[str, str, list[tuple[int, str]]]] = [
    ("pub(super) fn", "fn a() {}\npub(super) fn b() {}\n", [(2, "pub(super)")]),
    ("pub(in path)", "    pub(in crate::x) struct S;\n", [(1, "pub(in crate::x)")]),
    ("spaced", "pub ( super ) const C: u8 = 0;\n", [(1, "pub(super)")]),
    ("across lines", "pub(\n    super\n) use a::b;\n", [(1, "pub(super)")]),
    ("struct field", "struct S {\n    pub(super) x: u8,\n}\n", [(2, "pub(super)")]),
    ("macro body", "macro_rules! m { () => { pub(super) fn f() {} }; }\n", [(1, "pub(super)")]),
    ("two on one line", "pub(super) fn f() {} pub(in super) fn g() {}\n", [(1, "pub(super)"), (1, "pub(in super)")]),
    ("line comment", "// pub(super) fn f() {}\n", []),
    ("doc comment", "/// Was `pub(super)`.\n//! pub(in crate)\nfn f() {}\n", []),
    ("nested block comment", "/* outer /* pub(super) */ pub(super) */ fn f() {}\n", []),
    ("string", 'const S: &str = "pub(super) fn";\n', []),
    ("escaped quote", 'const S: &str = "\\" pub(super)";\n', []),
    ("raw string", 'const S: &str = r#"say "pub(super)" "#;\n', []),
    ("byte string", "const S: &[u8] = b\"pub(super)\";\n", []),
    ("char quote keeps sync", "const Q: char = '\"';\npub(super) fn f() {}\n", [(2, "pub(super)")]),
    ("lifetime is not a char", "fn f<'a>(x: &'a str) {}\npub(super) fn g() {}\n", [(2, "pub(super)")]),
    ("line after multi-line string", 'const S: &str = "a\nb\nc";\npub(in crate) fn f() {}\n', [(4, "pub(in crate)")]),
    ("allowed levels", "pub(crate) fn a() {}\npub fn b() {}\nfn c() {}\npub(self) fn d() {}\n", []),
    ("not a visibility", "fn my_pub(super_x: u8) {}\nlet pub_ = 1;\nfn x() { pub(superb); }\n", []),
]


def self_test() -> int:
    failures = []
    for name, source, expected in SELF_TEST_CASES:
        got = violations(source)
        if got != expected:
            failures.append(f"{name}: expected {expected}, got {got}")
    for failure in failures:
        print(f"check-visibility self-test: FAIL {failure}", file=sys.stderr)
    if failures:
        return 1
    print(f"check-visibility self-test: OK ({len(SELF_TEST_CASES)} cases)")
    return 0


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("root", nargs="?", type=pathlib.Path, default=ROOT, help="checkout to scan (default: this one)")
    parser.add_argument("--self-test", action="store_true", help="check the scanner on synthetic sources, then exit")
    args = parser.parse_args(argv)
    if args.self_test:
        return self_test()
    return scan(args.root.resolve())


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
