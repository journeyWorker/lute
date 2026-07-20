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
   - Every CURRENT-version claim in packages/website/public/llms.txt and
     llms-full.txt must equal `LUTE_LANG_VERSION`. Only lines that literally
     assert the current version are matched (precise phrasings pinned below);
     historical proposal-table rows (`| **0.6.0** | ... |`) are exempt because
     they carry none of those phrasings.
   - The IR schema `schemas/lute-ir-<major.minor>.schema.json` for the current
     IR version's major.minor must exist.

2. Canonical-domain hygiene: the stale `lute-website.vercel.app` host must not
   appear anywhere under packages/website/ or docs/ (canonical is
   lute-lang.vercel.app).

3. Example-check manifest: prints the list of example roots CI is expected to
   run `lute check-project` against (docs/examples, plus conformance/ when
   present). The actual `lute` invocation lives in the workflow, not here.

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

DOMAIN_ROOTS = (ROOT / "packages/website", ROOT / "docs")
STALE_DOMAIN = "lute-website.vercel.app"

EXAMPLE_ROOT_CANDIDATES = ("docs/examples", "conformance")

# Precise CURRENT-language-version claim phrasings actually present in the two
# llms files. Applied to the FULL file text (not line-by-line) so a claim that
# wraps across a newline — e.g. "targets language version\n**0.6.1**." — is
# still captured. Proposal-table rows ("| **0.6.1** | Current tip ... |") carry
# none of these phrasings and are therefore exempt.
VERSION_CLAIM_PATTERNS = (
    re.compile(r"current language version is\s+\*\*(\d+\.\d+\.\d+)\*\*"),
    re.compile(r"targets language version\s+\*\*(\d+\.\d+\.\d+)\*\*"),
    re.compile(r"Language version\s+(\d+\.\d+\.\d+)\."),
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


def check_version_claims(path: pathlib.Path, expected: str) -> None:
    text = read(path)
    rel = path.relative_to(ROOT)
    found: list[str] = []
    for pat in VERSION_CLAIM_PATTERNS:
        found.extend(pat.findall(text))
    check(
        len(found) > 0,
        f"{rel}: no current-language-version claim found (expected a phrasing "
        f"like 'current language version is **{expected}**'); did the wording "
        f"change? Update VERSION_CLAIM_PATTERNS in this script.",
    )
    for v in found:
        check(
            v == expected,
            f"{rel}: version claim {v!r} != crate LUTE_LANG_VERSION {expected!r}",
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

    # 1. Version claims in the website llms files match the crate const.
    check_version_claims(LLMS, lang_version)
    check_version_claims(LLMS_FULL, lang_version)

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
        f"website version claims and canonical domain are coherent."
    )
    print("check-docs-consistency: example roots for CI check-project:")
    for r in roots:
        print(f"  - {r}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
