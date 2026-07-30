#!/usr/bin/env python3
"""Docs / website / schema consistency checker.

Single-source-of-truth guard for the documentation surface. The Rust crates
own the version strings; the website's `llms.txt`/`llms-full.txt` and the IR
schema files merely restate them. This script fails the moment those restated
claims drift from the crate consts, or a stale canonical domain leaks into the
docs.

Checks (all stdlib python3, no third-party deps — same constraint as
scripts/check-release-workflow-safety.py):

1. Single-source version parity:
   - `LUTE_LANG_VERSION` is read from crates/lute-check/src/lib.rs and
     `LUTE_IR_VERSION` from crates/lute-compile/src/lib.rs (the canonical
     definitions — see those files' doc comments).
   - Every CURRENT-version claim in packages/website/public/llms.txt,
     llms-full.txt, and every page under packages/website/src/content/docs/
     must equal `LUTE_LANG_VERSION`. Only text that literally asserts the
     CURRENT language version is matched (precise phrasings pinned below), so
     the two hazards the docs tree adds stay out: `tooling/**` says `0.8.0`
     about the IR schema and about release history, and `spec/**` maps every
     feature to the version that introduced or last changed it. Neither
     carries a pinned phrasing. The llms files must each carry at least one
     claim (they are generated summaries and losing the claim would be
     drift); an ordinary docs page needs none.
   - The IR schema `schemas/lute-ir-<major.minor>.schema.json` for the current
     IR version's major.minor must exist.

2. Canonical-domain hygiene: the stale `lute-website.vercel.app` host must not
   appear anywhere under packages/website/ or docs/ (canonical is
   lute-lang.vercel.app).

3. Example-check manifest: prints the one example root CI runs
   `lute check-project` against (docs/examples). `conformance/` is NOT a root:
   each fixture is an independent single-document contract test replayed on its
   own (see conformance/README.md), and several deliberately reuse the same
   scene identity, so unioning them into one project is `E-CONN-EPISODE-ID-DUP`
   by construction. The actual `lute` invocation lives in the workflow.

Exit: 0 clean, 1 on a consistency violation, 2 on a missing/unreadable input.
"""

from __future__ import annotations

import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent

LANG_CONST_FILE = ROOT / "crates/lute-check/src/lib.rs"
IR_CONST_FILE = ROOT / "crates/lute-compile/src/lib.rs"
SCHEMA_DIR = ROOT / "schemas"
LLMS = ROOT / "packages/website/public/llms.txt"
LLMS_FULL = ROOT / "packages/website/public/llms-full.txt"
DOCS_CONTENT = ROOT / "packages/website/src/content/docs"
DOCS_CONTENT_SUFFIXES = (".md", ".mdx")

DOMAIN_ROOTS = (ROOT / "packages/website", ROOT / "docs")
STALE_DOMAIN = "lute-website.vercel.app"

# `conformance/` is deliberately absent — see the module docstring: its fixtures
# are replayed one at a time and several share a scene identity on purpose.
EXAMPLE_ROOT_CANDIDATES = ("docs/examples",)

# Precise CURRENT-language-version claim phrasings. Applied to the FULL file
# text (not line-by-line) so a claim that wraps across a newline — e.g.
# "targets language version\n**0.6.1**." — is still captured.
#
# Every pattern below is anchored on wording that can ONLY be a claim about the
# language version as it stands right now. That precision is what lets the same
# set run over packages/website/src/content/docs/ without flagging the two
# legitimate `0.8.0`s living there:
#
#   * IR-version claims. `tooling/runtime-contract.md`'s "What IR 0.8.0
#     changed", `tooling/cli.md`'s `"irVersion": "0.8.0"`, and
#     `getting-started/installation.md`'s `IR schema     0.8.0` name a
#     different axis. None of them says "language".
#   * Historical attribution. `spec/**` maps each feature to the version that
#     introduced or last changed it, `tooling/ai-harness.md` says "Since 0.8.0
#     they set the exit code", and `examples/showcase.md` quotes
#     `luteVersion: "0.8.0"` under a "What 0.8.0 changed here" heading. Those
#     are past-tense attributions, not current-version assertions — which is
#     also why `luteVersion:` is deliberately NOT pinned here.
#
# The last three patterns pin REPLAYED OUTPUT rather than prose, because a
# transcript rots exactly as silently as a sentence: the `language` axis of
# `lute version` and `lute version --json`, and the `"lute"` stamp a compiled
# artifact carries (its sibling `"irVersion"` is a different axis and is not
# matched).
VERSION_CLAIM_PATTERNS = (
    re.compile(r"current language version is\s+\*\*(\d+\.\d+\.\d+)\*\*"),
    re.compile(r"targets language version\s+\*\*(\d+\.\d+\.\d+)\*\*"),
    re.compile(r"Language version\s+(\d+\.\d+\.\d+)\."),
    # Korean locale phrasings (ko/**): "현재 언어 버전은 **0.9.0**입니다" and
    # "언어 버전 **0.9.0**을 대상으로 합니다".
    re.compile(r"현재 언어 버전은\s*\*\*(\d+\.\d+\.\d+)\*\*"),
    re.compile(r"언어\s*버전\s+\*\*(\d+\.\d+\.\d+)\*\*"),
    # `lute version` human output — the middle of three labelled axes.
    re.compile(r"^language\s+(\d+\.\d+\.\d+)\s*$", re.MULTILINE),
    # `lute version --json` and a compiled artifact's language stamp.
    re.compile(r'"language"\s*:\s*"(\d+\.\d+\.\d+)"'),
    re.compile(r'"lute"\s*:\s*"(\d+\.\d+\.\d+)"'),
)

ERRORS: list[str] = []


def check(cond: bool, msg: str) -> None:
    if not cond:
        ERRORS.append(msg)


def fail(msg: str) -> None:
    print(f"check-docs-consistency: FATAL: {msg}", file=sys.stderr)
    sys.exit(2)


def read(path: pathlib.Path) -> str:
    if not path.is_file():
        fail(f"missing required file: {path.relative_to(ROOT)}")
    return path.read_text(encoding="utf-8")


def extract_const(path: pathlib.Path, name: str) -> str:
    text = read(path)
    m = re.search(rf'pub const {re.escape(name)}\s*:\s*&str\s*=\s*"([^"]+)"', text)
    if not m:
        fail(f"could not find `pub const {name}` in {path.relative_to(ROOT)}")
    return m.group(1)


def check_version_claims(
    path: pathlib.Path, expected: str, *, require_claim: bool = True
) -> int:
    """Pin every CURRENT-language-version claim in `path` to `expected`.

    Returns the number of claims found. `require_claim` is for the two llms
    files, which are generated summaries whose claim going missing is itself
    drift; an ordinary docs page is free to carry none.
    """
    text = read(path)
    rel = path.relative_to(ROOT)
    found: list[tuple[int, str]] = []
    for pat in VERSION_CLAIM_PATTERNS:
        for m in pat.finditer(text):
            found.append((text.count("\n", 0, m.start()) + 1, m.group(1)))
    if require_claim:
        check(
            len(found) > 0,
            f"{rel}: no current-language-version claim found (expected a phrasing "
            f"like 'current language version is **{expected}**'); did the wording "
            f"change? Update VERSION_CLAIM_PATTERNS in this script.",
        )
    for line, v in sorted(found):
        check(
            v == expected,
            f"{rel}:{line}: current-language-version claim {v!r} != crate "
            f"LUTE_LANG_VERSION {expected!r}",
        )
    return len(found)


def docs_content_pages() -> list[pathlib.Path]:
    if not DOCS_CONTENT.is_dir():
        fail(f"missing docs content tree: {DOCS_CONTENT.relative_to(ROOT)}")
    return sorted(
        p
        for p in DOCS_CONTENT.rglob("*")
        if p.is_file() and p.suffix in DOCS_CONTENT_SUFFIXES
    )


def check_stale_domain() -> None:
    for base in DOMAIN_ROOTS:
        if not base.is_dir():
            continue
        for p in sorted(base.rglob("*")):
            if not p.is_file():
                continue
            try:
                text = p.read_text(encoding="utf-8")
            except (UnicodeDecodeError, OSError):
                continue  # binary / unreadable — no textual URL to leak
            if STALE_DOMAIN in text:
                ERRORS.append(
                    f"{p.relative_to(ROOT)}: contains stale domain "
                    f"'{STALE_DOMAIN}' (canonical is lute-lang.vercel.app)"
                )


def example_roots() -> list[str]:
    return [r for r in EXAMPLE_ROOT_CANDIDATES if (ROOT / r).is_dir()]


def main() -> int:
    lang_version = extract_const(LANG_CONST_FILE, "LUTE_LANG_VERSION")
    ir_version = extract_const(IR_CONST_FILE, "LUTE_IR_VERSION")

    # 1. Version claims in the website llms files match the crate const. Both
    # are generated summaries, so a MISSING claim is drift too.
    claims = check_version_claims(LLMS, lang_version)
    claims += check_version_claims(LLMS_FULL, lang_version)

    # 1a. …and so does every claim on every page of the docs content tree.
    # This is the scope that was missing: the same first pattern that guards
    # llms.txt matched, verbatim, the stale string that shipped in
    # spec/index.md — it was simply never pointed at these files.
    pages = docs_content_pages()
    for page in pages:
        claims += check_version_claims(page, lang_version, require_claim=False)

    # 1b. The IR schema for the current IR major.minor exists.
    ir_mm = ".".join(ir_version.split(".")[:2])
    schema = SCHEMA_DIR / f"lute-ir-{ir_mm}.schema.json"
    check(
        schema.is_file(),
        f"missing IR schema for current IR version {ir_version}: expected "
        f"{schema.relative_to(ROOT)}",
    )

    # 2. No stale canonical domain anywhere under the docs/website trees.
    check_stale_domain()

    # 3. Example-check manifest for the workflow.
    roots = example_roots()
    check(len(roots) > 0, "no example roots found (expected docs/examples)")

    if ERRORS:
        print("check-docs-consistency: DRIFT DETECTED\n", file=sys.stderr)
        for e in ERRORS:
            print(f"  - {e}", file=sys.stderr)
        return 1

    print(
        f"check-docs-consistency: OK — language version {lang_version}, "
        f"IR version {ir_version} (schema {schema.relative_to(ROOT)}); "
        f"{claims} current-language-version claim(s) across llms.txt, "
        f"llms-full.txt and {len(pages)} docs page(s) all read "
        f"{lang_version}; canonical domain is coherent."
    )
    print("check-docs-consistency: example roots for CI check-project:")
    for r in roots:
        print(f"  - {r}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
