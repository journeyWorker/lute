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
     must equal `LUTE_LANG_VERSION`. Claims are found two ways — an exact
     phrasing list AND a cue-plus-bold-version rule (both described at
     VERSION_CLAIM_PATTERNS below) — because a list of sentences someone
     thought of only catches the sentences someone thought of. What must stay
     unmatched are the two hazards this tree really contains: `tooling/**`
     says `0.8.0` about the IR line and about release history, and `spec/**`
     maps every feature to the version that introduced or last changed it.
     Both write a historical version in `backticks`, never in **bold**. The
     llms files must each carry at least one claim (they are generated
     summaries and losing the claim would be drift); an ordinary docs page
     needs none.
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

# The repo-side prose surface, scanned for the same claims. The website tree
# alone was one directory short: the root README.md carried "the grammar is at
# **0.8.0**", a stale "current tip 0.8.0" spec row, and a quoted excerpt still
# stamped `luteVersion: "0.1.0"` — eight releases behind — with nothing looking
# at the file at all.
#
# The two excluded subtrees are excluded for the reasons
# scripts/check-doc-snippets.py already documents, and this list is kept
# deliberately identical to that script's:
#   * docs/proposals/**  — frozen normative history. Each proposal describes
#     its own release and legitimately says "this document stays **0.0.1**".
#   * docs/superpowers/** — agent plan/spec artifacts, not documentation.
REPO_DOC_FILES = (ROOT / "README.md", ROOT / "CHANGELOG.md")
REPO_DOC_TREE = ROOT / "docs"
REPO_DOC_EXCLUDED = ("proposals", "superpowers")

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

# Pass 2 — the one that does not depend on anyone predicting a sentence.
#
# Pass 1 is a list of exact phrasings, and a list only catches what someone
# thought of. It has already been escaped once: llms-full.txt mirrored
# spec/current.md and spec/index.md, drifted a whole release behind on
# "what is *current* at language version **0.8.0**" and a "| **0.8.0** |
# Current tip" table row, and neither phrasing was in the list — so a guard
# sitting directly on that file said OK.
#
# So instead of adding those two sentences, pin the CONVENTION the docs
# already follow, which is much harder to slip past:
#
#   A bare version in **bold** asserts a version. A version being talked
#   ABOUT is written in `backticks`.
#
# Every `**X.Y.Z**` in the scanned set is a current-language-version claim, and
# every historical mention — `spec/**`'s introduced/last-changed columns,
# `tooling/**`'s IR line and release history, "Before `0.8.0` the index
# field…" — is backticked.
#
# The bold alone is not quite enough to fire on, because a bolded version can
# legitimately name a different axis ("Plugin system **0.0.3**"). So the
# sentence must ALSO carry a cue, of either kind:
#
#   * a CURRENCY cue — "current", "latest", "as of", "현재" …
#     ("The current language version is **0.9.0**", "| **0.9.0** | Current tip")
#   * an AXIS cue — the sentence names the language/grammar axis itself
#     ("It targets language version **0.9.0**", "The grammar is at **0.9.0**",
#      "언어 버전 **0.9.0**을 대상으로 합니다")
#
# Two cue kinds rather than one because the README's phrasing carries no
# currency word at all — it just asserts what the grammar is — and that is the
# file that went eight releases stale.
#
# A violation here means one of two things, and the message says both: the
# number is stale, or a historical version got bolded and should be
# `backticked` instead.
BOLD_VERSION_RE = re.compile(r"\*\*(\d+\.\d+\.\d+)\*\*")
CLAIM_CUE_RE = re.compile(
    r"current|currently|latest|today|as of|now at|up to date"
    r"|language|grammar"
    r"|현재|최신|지금|언어|문법",
    re.IGNORECASE,
)
# Sentence-ish boundaries. A blank line ends a unit; so does terminal
# punctuation followed by whitespace. A SINGLE newline does not — the docs are
# hard-wrapped at ~100 columns and a claim routinely straddles one
# ("It targets language version\n**0.9.0**.").
SEGMENT_BREAK_RE = re.compile(r"\n[ \t]*\n|(?<=[.!?:;])\s+")


def cue_claims(text: str) -> list[tuple[int, str]]:
    """Bold bare versions sitting in a sentence that asserts one."""
    found: list[tuple[int, str]] = []
    pos = 0
    for brk in SEGMENT_BREAK_RE.finditer(text):
        _scan_segment(text, pos, brk.start(), found)
        pos = brk.end()
    _scan_segment(text, pos, len(text), found)
    return found


def _scan_segment(text: str, start: int, end: int, out: list[tuple[int, str]]) -> None:
    seg = text[start:end]
    if not CLAIM_CUE_RE.search(seg):
        return
    for m in BOLD_VERSION_RE.finditer(seg):
        out.append((text.count("\n", 0, start + m.start()) + 1, m.group(1)))


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
    exact = set(found)
    cued = [c for c in cue_claims(text) if c not in exact]
    found.extend(cued)
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
            f"LUTE_LANG_VERSION {expected!r} — either the number is stale, or a "
            f"HISTORICAL version got written in **bold** inside a sentence that "
            f"asserts currency (write a historical version in `backticks`)",
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


def repo_doc_pages() -> list[pathlib.Path]:
    """README/CHANGELOG plus docs/**.md, minus the two frozen subtrees."""
    pages = [p for p in REPO_DOC_FILES if p.is_file()]
    for p in REPO_DOC_TREE.rglob("*.md"):
        rel = p.relative_to(REPO_DOC_TREE)
        if rel.parts and rel.parts[0] in REPO_DOC_EXCLUDED:
            continue
        pages.append(p)
    return sorted(set(pages))


def check_stale_domain() -> None:
    # The same one-directory-short bug bit here too: DOMAIN_ROOTS never
    # included the repo root, so README.md's links were unscanned.
    targets: list[pathlib.Path] = [p for p in REPO_DOC_FILES if p.is_file()]
    for base in DOMAIN_ROOTS:
        if not base.is_dir():
            continue
        targets.extend(p for p in base.rglob("*") if p.is_file())
    for p in sorted(set(targets)):
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

    # 1a. …and so does every claim on every page of the docs content tree,
    # AND on the repo-side prose surface. Scope is what failed twice here: the
    # first time the website tree was unscanned and spec/index.md shipped
    # stale; the second time the website tree was scanned but README.md was
    # not, and it went eight releases stale in a block quoting a real file.
    pages = docs_content_pages() + repo_doc_pages()
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

    # 2. No stale canonical domain under the docs/website trees or the root.
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
