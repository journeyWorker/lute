#!/usr/bin/env python3
"""Compile-check the ```lute snippets in the documentation surface.

The `website` CI job builds the Astro site, so it catches a broken page or a
broken link. It has never once looked at what is *inside* a fenced block, and
`docs.yml` lists `docs/**` in its trigger paths while its only content job runs
`check-project` over `docs/examples` alone — a trigger that fires on a file
nothing reads. That combination is how three bug classes shipped:

  * ten pages writing `emotion="…"` after dsl 0.9.0 moved content-vocabulary
    members out of the compiler (`E-DOMAIN-UNKNOWN`);
  * six pages shipping the single-line `<when>…</when>` form that dsl 0.5.0
    §122-123 says was never legal (`E-TAG-INLINE-BODY`);
  * a repo-side tutorial that sat a whole release on `"irVersion": "0.7.0"`.

All three are invisible to a page build and obvious to `lute check`.

This script extracts every ```lute block from the roots in `SNIPPET_ROOTS` and
runs the real checker over the ones an author has declared checkable.


THE FRAGMENT-VS-COMPLETE RULE
-----------------------------

A block's checkability is NOT a property of the block. Three measured reasons,
each of which alone defeats a per-block heuristic:

  1. It depends on the project layout the surrounding prose establishes.
     `getting-started/first-scene.md`'s second whole document has frontmatter
     and a `kind:`, yet checking it standalone reports `E-MAYBE-UNSET` on
     `run.metMira` — the `after:` route that makes the path resolvable is
     introduced later on the page in a block tagged ```yaml. The page is right;
     a heuristic reading it block-by-block is wrong.
  2. Some blocks are SUPPOSED to fail. Both `first-scene` tutorials are
     deliberately incremental and walk the reader through `E-KIND-MISSING`,
     `E-CONTENT-OUTSIDE-SHOT`, `E-DOMAIN-UNKNOWN` and
     `E-LEGACY-CONTENT-SIGIL`. A checker demanding every block pass reports
     errors on a correct page.
  3. `docs/proposals/**` is frozen normative history, pinned to its own
     `luteVersion:`. Those blocks are supposed to show pre-0.9.0 syntax.

So the unit of verification is a block PAIRED WITH ITS EXPECTED OUTCOME, and
both halves are DECLARED in the fence meta rather than guessed:

    ```lute check
        Self-contained whole document, expected CLEAN. `lute check` on it alone,
        with no project around it, must report no errors.

    ```lute check="<repo-relative path>"
        This block is the content of that file. The file must exist, and the
        block is checked *as* that file: the enclosing project root (the
        nearest ancestor holding a `lute.project.yaml`) is copied to a scratch
        directory, the block is written over that path, and
        `lute check <path> --project <root>` must report no errors. This is the
        marker for every excerpt whose prose already says "(From
        `docs/examples/…`)" — it pins the excerpt to a real, laid-out project
        AND fails loudly if the page's cited source is renamed or deleted.

    ```lute expect="E-KIND-MISSING[,E-OTHER…]"
        Expected to FAIL, with exactly that set of error codes. This is what
        makes a tutorial's intermediate states verifiable: a block with no
        frontmatter that is supposed to produce `E-KIND-MISSING` carries a
        contract just as precise as a clean one. Warnings do not gate and are
        ignored. Combine with `check="<path>"` to expect a failure inside a
        project layout; on its own it checks standalone.

    ```lute unverified="<reason>"
        An explicit, reviewed opt-out. Reported by name with its reason on
        every run. An empty reason is an error.

An unmarked block is never silently dropped. It is classified and REPORTED:

  * fragment — no frontmatter fence, or frontmatter with no `kind:`/`component:`
    key. A bare `@speaker:` line or a lone `<hub>` block has no declared
    outcome and nothing to opt out of.
  * whole document, UNMARKED — has the frontmatter of a real document but no
    marker. Printed individually, and capped per root, so ADDING one fails this
    check. A cap only ever needs LOWERING; marking a block is always allowed.

The verified count is likewise floored by `MIN_VERIFIED_BLOCKS`, so quietly
deleting a marker to dodge a failure fails instead. Between the two counters,
the check cannot be disabled by omission — which is the failure mode that
produced the bugs this script exists to catch.

Coverage is printed on every run, pass or fail: N checked, M explicitly
unverified with reasons, K unmarked, F fragments. A check that quietly skips is
the thing being fixed here.


PINNED CAPABILITY HASH
----------------------

`capabilityVersion` is a 64-hex digest of the resolved authoring surface, and
the docs quote it as a literal. It was correct for this build and guaranteed to
rot silently the next time the capability surface moves — the same shape of bug
as the version strings, and it had already rotted once (`78a2f619…`, a release
behind). Every 64-hex literal in the documentation surface must therefore equal
the `capabilityVersion` of one of the reference projects, computed here by
running `lute context --json`. Nothing is hardcoded.

That pin cannot live in scripts/check-docs-consistency.py: that script is
deliberately dependency-free stdlib python3 and runs in a CI job with no Rust
toolchain. This one already needs the binary, so binary-derived facts belong
here.


USAGE
-----

    python3 scripts/check-doc-snippets.py [--lute ./target/debug/lute]

Exit: 0 clean, 1 on a snippet/hash violation, 2 on a missing input or an
unusable `lute` binary.
"""

from __future__ import annotations

import argparse
import json
import pathlib
import re
import shutil
import subprocess
import sys
import tempfile
from dataclasses import dataclass, field

ROOT = pathlib.Path(__file__).resolve().parent.parent


@dataclass(frozen=True)
class Root:
    """One documentation tree this script is responsible for."""

    path: str
    suffixes: tuple[str, ...]
    #: Cap on whole-document blocks carrying no marker. May only be lowered.
    max_unmarked_whole: int
    #: Sub-trees deliberately outside the scan, each with its reason.
    exclude: tuple[tuple[str, str], ...] = field(default=())

    def pages(self) -> list[pathlib.Path]:
        base = ROOT / self.path
        if not base.is_dir():
            return []
        skip = tuple(ROOT / e for e, _ in self.exclude)
        return sorted(
            p
            for p in base.rglob("*")
            if p.is_file()
            and p.suffix in self.suffixes
            and not any(s in p.parents for s in skip)
        )


SNIPPET_ROOTS = (
    # The rendered website.
    Root(
        path="packages/website/src/content/docs",
        suffixes=(".md", ".mdx"),
        # Zero: every whole document on the website now carries a marker. Both
        # `first-scene` tutorials' second whole document — the booth scene,
        # meaningful only inside the two-file project the prose builds — is
        # pinned with `check-project="docs/examples/episodes/booth.lute"`, so
        # the layout that makes it check is a real project in this repo rather
        # than a promise in a comment.
        max_unmarked_whole=0,
    ),
    # The repo-side docs. A first-class root, not an afterthought: `docs.yml`
    # already triggers on `docs/**`, and until now nothing in the workflow read
    # a single line of it.
    Root(
        path="docs",
        suffixes=(".md",),
        exclude=(
            (
                "docs/proposals",
                "frozen normative history — each proposal is pinned to its own "
                "`luteVersion:` and deliberately shows the syntax of its own "
                "release, so 'checks clean today' is the wrong contract",
            ),
            (
                "docs/superpowers",
                "agent plan/spec artifacts, not documentation of the language",
            ),
        ),
        # Zero: `docs/adoption/oshiz-assessment.md:257` illustrates a mapping
        # onto a customer catalog that does not exist in this repo, and now
        # says so with `unverified="<reason>"` rather than by silence.
        max_unmarked_whole=0,
    ),
)

# `packages/website/public/llms*.txt` are deliberately NOT snippet roots, and
# this is an ACKNOWLEDGED GAP rather than an oversight. llms.txt carries no
# ```lute block at all; llms-full.txt carries 46, and measured against the
# binary, 9 of its 12 whole documents fail standalone BY CONSTRUCTION — they are
# flattened excerpts of pages whose project layout the surrounding prose
# establishes, and 6 of the 12 have no byte-identical counterpart on any page,
# so a mirror-inheritance rule would cover only a quarter of them. Treating a
# whole document there as implicitly `check`-marked would therefore report ~9
# errors on a correct file: the same false-failure trap that made checkability a
# DECLARED property everywhere else in this script. Marking them would mean
# putting fence meta into a machine-oriented plain-text bundle, which is a call
# for that file's owner, not this script's.
#
# What DOES guard them today: the capability-hash pin below (it caught a
# release-stale `78a2f619…` there), and every current-language-version claim,
# via scripts/check-docs-consistency.py — which is what caught llms-full.txt's
# three stale `lute version` transcript values. Their ```lute blocks themselves
# are hand-verified only.

# The capability-hash scan covers the same trees plus the two generated llms
# bundles that mirror the website.
HASH_SCAN_EXTRA = (
    ROOT / "packages/website/public/llms.txt",
    ROOT / "packages/website/public/llms-full.txt",
)

# Reference projects whose capabilityVersion a doc may legitimately quote: the
# core-only surface (no project at all) plus every project root under
# docs/examples. The plugin-carrying ones (showcase/, idola-project/,
# plugindef-project/) resolve a different surface and therefore a different
# hash, which is correct and must not be flagged.
EXAMPLES_ROOT = ROOT / "docs/examples"

HASH_RE = re.compile(r"\b[0-9a-f]{64}\b")

# Opening fence: indent, 3+ backticks or tildes, an info word, then meta.
FENCE_RE = re.compile(r"^([ \t]*)(`{3,}|~{3,})[ \t]*([^\s`~]*)(.*)$")

# One meta token, scanned left to right over the whole meta string so a quoted
# value may contain spaces: `key`, `key=value`, `key="value with spaces"`, or
# (last alternative) any unrecognised run of non-space.
META_RE = re.compile(r'([A-Za-z_][\w-]*)(?:=(?:"([^"]*)"|([^\s"]+)))?|(\S+)')

MARKERS = ("check", "check-project", "expect", "unverified")

# Error-severity codes in `lute check`'s human output: a positioned diagnostic
# (`file:line:col: error [CODE] …`) or a project-level one (`lute: CODE: …`).
DIAG_RE = re.compile(r"^\S*?:\d+:\d+: error \[([A-Z0-9-]+)\]", re.MULTILINE)
PROJECT_DIAG_RE = re.compile(r"^lute: ([A-Z][A-Z0-9-]+): ", re.MULTILINE)

# Verified blocks may only grow. Quietly deleting a marker fails instead.
MIN_VERIFIED_BLOCKS = 18

ERRORS: list[str] = []


def fail(msg: str) -> "None":
    print(f"check-doc-snippets: FATAL: {msg}", file=sys.stderr)
    sys.exit(2)


def parse_meta(meta: str) -> tuple[dict[str, str | None], list[str]]:
    """Split a fence meta string into recognised markers and everything else."""
    known: dict[str, str | None] = {}
    other: list[str] = []
    for m in META_RE.finditer(meta):
        if m.group(1) is None:
            other.append(m.group(4))
        elif m.group(1) in MARKERS:
            known[m.group(1)] = m.group(2) if m.group(2) is not None else m.group(3)
        else:
            other.append(m.group(0))
    return known, other


def extract_blocks(path: pathlib.Path) -> list[dict]:
    """Every ```lute block in `path`, with its 1-based opening line and meta."""
    lines = path.read_text(encoding="utf-8").splitlines()
    blocks: list[dict] = []
    opener: tuple[str, int, str, int, str] | None = None
    body: list[str] = []
    for i, line in enumerate(lines):
        m = FENCE_RE.match(line)
        if m:
            char, run, info, meta = m.group(2)[0], m.group(2), m.group(3), m.group(4)
            if opener is None:
                opener = (char, len(run), info, i, meta.strip())
                body = []
                continue
            # A closer repeats the opener's character at least as many times
            # and carries no info string.
            if info == "" and char == opener[0] and len(run) >= opener[1]:
                if opener[2] == "lute":
                    blocks.append(
                        {"line": opener[3] + 1, "meta": opener[4], "body": "\n".join(body)}
                    )
                opener = None
                continue
        if opener is not None:
            body.append(line)
    if opener is not None:
        ERRORS.append(f"{path.relative_to(ROOT)}:{opener[3] + 1}: unterminated code fence")
    return blocks


def classify(body: str) -> str:
    """`whole` if the block is a document in its own right, else `fragment`."""
    lines = body.splitlines()
    if not lines or lines[0].strip() != "---":
        return "fragment"
    ends = [i for i, l in enumerate(lines[1:], 1) if l.strip() == "---"]
    if not ends:
        return "fragment"
    front = lines[1 : ends[0]]
    # A scene/quest declares `kind:`; a reusable content component declares
    # `component:` and legitimately has no `kind:` at all.
    if any(re.match(r"(kind|component):\s*\S", l) for l in front):
        return "whole"
    return "fragment"


def project_root_for(rel: str) -> pathlib.Path | None:
    """Nearest ancestor of `rel` holding a lute.project.yaml, bounded by ROOT."""
    d = (ROOT / rel).parent
    while True:
        if (d / "lute.project.yaml").is_file():
            return d
        if d == ROOT:
            return None
        d = d.parent


def run_lute(lute: str, args: list[str]) -> tuple[int, str]:
    proc = subprocess.run([lute, *args], capture_output=True, text=True)
    return proc.returncode, proc.stdout + proc.stderr


def error_codes(out: str) -> set[str]:
    return set(DIAG_RE.findall(out)) | set(PROJECT_DIAG_RE.findall(out))


def indent(out: str) -> str:
    return "\n".join(f"      {l}" for l in out.strip().splitlines())


def run_block(
    lute: str, body: str, rel: str | None, whole_project: bool = False
) -> tuple[int, str, str]:
    """Check `body`, standalone or materialised as `rel` inside its project.

    `whole_project` swaps single-file `check <path> --project <root>` for
    `check-project <root>`. That is not a stylistic choice: single-file check
    cannot establish cross-scene route guarantees, so a block that is only
    sound as part of a multi-file project needs the project-wide pass. Measured
    on docs/examples/episodes: `check booth.lute --project episodes` reports
    `E-MAYBE-UNSET` on `run.metMira` where `check-project episodes` is clean,
    and docs/examples/connected-outro.lute documents that as a language
    property.

    Returns (exit code, cleaned output, a human description of the scope).
    """
    with tempfile.TemporaryDirectory() as tmp:
        if rel is None:
            target = pathlib.Path(tmp) / "snippet.lute"
            args = ["check", str(target)]
            strip, scope = str(target), "standalone"
        else:
            root = project_root_for(rel)
            if root is None:
                base = pathlib.Path(tmp)
                target = base / pathlib.PurePath(rel).name
                args = ["check", str(target)]
                scope = "no enclosing project"
            else:
                base = pathlib.Path(tmp) / "proj"
                shutil.copytree(root, base)
                target = base / pathlib.PurePath(rel).relative_to(root.relative_to(ROOT))
                if whole_project:
                    args = ["check-project", str(base)]
                    scope = f"check-project {root.relative_to(ROOT)}"
                else:
                    args = ["check", str(target), "--project", str(base)]
                    scope = f"--project {root.relative_to(ROOT)}"
            strip = str(base) + "/"
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(body.rstrip("\n") + "\n", encoding="utf-8")
        rc, out = run_lute(lute, args)
        out = out.replace(strip, "" if rel is not None else "<block>")
    return rc, out, scope


def check_clean(
    lute: str, body: str, rel: str | None, where: str, whole_project: bool = False
) -> None:
    rc, out, scope = run_block(lute, body, rel, whole_project)
    if rc != 0:
        ERRORS.append(
            f"{where}: block marked clean does not check clean ({scope}, "
            f"exit {rc}):\n{indent(out)}"
        )


def check_expected_failure(
    lute: str,
    body: str,
    rel: str | None,
    expected: set[str],
    where: str,
    whole_project: bool = False,
) -> None:
    rc, out, scope = run_block(lute, body, rel, whole_project)
    if rc == 0:
        ERRORS.append(
            f"{where}: block declares expect=\"{','.join(sorted(expected))}\" "
            f"but checks CLEAN ({scope}). Either the page no longer illustrates "
            f"that diagnostic, or the marker is stale — drop the marker or fix "
            f"the block."
        )
        return
    got = error_codes(out)
    if got != expected:
        missing = sorted(expected - got)
        extra = sorted(got - expected)
        detail = []
        if missing:
            detail.append(f"declared but not reported: {', '.join(missing)}")
        if extra:
            detail.append(f"reported but not declared: {', '.join(extra)}")
        ERRORS.append(
            f"{where}: expected error codes do not match ({scope}); "
            f"{'; '.join(detail)}:\n{indent(out)}"
        )


def path_marker(marks: dict[str, str | None], where: str) -> tuple[str | None, bool]:
    """Validate a `check`/`check-project` path marker.

    Returns (repo-relative path or None, whether to run the project-wide pass).
    """
    key = "check-project" if "check-project" in marks else "check"
    rel = marks.get(key)
    if rel is None:
        return None, False
    if not (ROOT / rel).is_file():
        ERRORS.append(
            f'{where}: {key}="{rel}" names a file that does not exist. The '
            f"marker pins this block to a real source file; update the path or "
            f"the page's citation."
        )
        return None, False
    if key == "check-project" and project_root_for(rel) is None:
        ERRORS.append(
            f'{where}: check-project="{rel}" has no enclosing '
            f"lute.project.yaml. `check-project` needs a project root; use "
            f'check="{rel}" for a single-file check.'
        )
        return None, False
    return rel, key == "check-project"


def capability_versions(lute: str) -> dict[str, str]:
    """capabilityVersion per reference project, straight from the binary."""
    out: dict[str, str] = {}

    def context(doc: pathlib.Path, project: pathlib.Path | None) -> str | None:
        args = ["context", str(doc), "--json"]
        if project is not None:
            args += ["--project", str(project)]
        rc, text = run_lute(lute, args)
        if rc != 0:
            return None
        try:
            return json.loads(text).get("capabilityVersion")
        except json.JSONDecodeError:
            return None

    probe = (
        "---\nkind: scene\ncharacter: probe\nseason: 1\nepisode: 1\n---\n"
        "\n## Probe\n\n@narrator: probe.\n"
    )
    with tempfile.TemporaryDirectory() as tmp:
        doc = pathlib.Path(tmp) / "probe.lute"
        doc.write_text(probe, encoding="utf-8")
        core = context(doc, None)
    if core is None:
        fail("`lute context --json` failed on a core-only probe document")
    out["core-only (no project)"] = core

    if EXAMPLES_ROOT.is_dir():
        for manifest in sorted(EXAMPLES_ROOT.rglob("lute.project.yaml")):
            root = manifest.parent
            # A document that belongs to THIS root, not to a nested one.
            docs = [
                d
                for d in sorted(root.rglob("*.lute"))
                if project_root_for(str(d.relative_to(ROOT))) == root
            ]
            if not docs:
                continue
            cv = context(docs[0], root)
            if cv:
                out[str(root.relative_to(ROOT))] = cv
    return out


def check_pinned_hashes(known: dict[str, str], pages: list[pathlib.Path]) -> int:
    valid = set(known.values())
    seen = 0
    for p in pages + [q for q in HASH_SCAN_EXTRA if q.is_file()]:
        for n, line in enumerate(p.read_text(encoding="utf-8").splitlines(), 1):
            for m in HASH_RE.finditer(line):
                seen += 1
                if m.group(0) not in valid:
                    ERRORS.append(
                        f"{p.relative_to(ROOT)}:{n}: pinned capability hash "
                        f"{m.group(0)[:16]}… matches no reference project's "
                        f"capabilityVersion. Current values, from "
                        f"`lute context --json`:\n"
                        + "\n".join(f"      {v[:16]}…  {k}" for k, v in known.items())
                    )
    return seen


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument(
        "--lute",
        default=str(ROOT / "target/debug/lute"),
        help="path to the lute binary (default ./target/debug/lute)",
    )
    args = ap.parse_args()

    lute = args.lute
    try:
        rc, _ = run_lute(lute, ["--version"])
    except OSError as exc:
        rc = -1
        reason = f" ({exc.strerror or exc})"
    else:
        reason = ""
    if rc != 0:
        fail(
            f"cannot run `{lute} --version`{reason} — build it first "
            f"(`cargo build -p lute-cli`) or pass --lute"
        )

    verified: list[str] = []
    opted_out: list[tuple[str, str]] = []
    unmarked_whole: list[str] = []
    other_meta: list[tuple[str, str]] = []
    fragments = 0
    all_pages: list[pathlib.Path] = []

    for root in SNIPPET_ROOTS:
        pages = root.pages()
        if not pages:
            fail(f"snippet root has no pages: {root.path}")
        all_pages += pages
        root_unmarked: list[str] = []
        for page in pages:
            rel_page = page.relative_to(ROOT)
            for b in extract_blocks(page):
                where = f"{rel_page}:{b['line']}"
                marks, other = parse_meta(b["meta"])
                if other:
                    other_meta.append((where, " ".join(other)))
                if "unverified" in marks:
                    if len(marks) > 1:
                        ERRORS.append(
                            f"{where}: `unverified` cannot be combined with "
                            f"{', '.join(sorted(set(marks) - {'unverified'}))}"
                        )
                        continue
                    why = (marks["unverified"] or "").strip()
                    if not why:
                        ERRORS.append(
                            f'{where}: `unverified` needs a reason — write '
                            f'unverified="why this block cannot be checked"'
                        )
                        continue
                    opted_out.append((where, why))
                    continue
                if "check" in marks and "check-project" in marks:
                    ERRORS.append(
                        f"{where}: use either `check` or `check-project`, not "
                        f"both — they name the same file and differ only in "
                        f"which pass verifies it"
                    )
                    continue
                has_path = "check" in marks or "check-project" in marks
                if "expect" in marks:
                    codes = {
                        c.strip() for c in (marks["expect"] or "").split(",") if c.strip()
                    }
                    if not codes:
                        ERRORS.append(
                            f'{where}: `expect` needs at least one diagnostic code — '
                            f'write expect="E-KIND-MISSING"'
                        )
                        continue
                    rel, whole = path_marker(marks, where) if has_path else (None, False)
                    check_expected_failure(lute, b["body"], rel, codes, where, whole)
                    how = f"as {rel} via check-project, " if whole else (
                        f"as {rel}, " if rel else ""
                    )
                    verified.append(f"{where}  ({how}expects {','.join(sorted(codes))})")
                    continue
                if has_path:
                    rel, whole = path_marker(marks, where)
                    check_clean(lute, b["body"], rel, where, whole)
                    how = (
                        f"as {rel} via check-project"
                        if whole
                        else (f"as {rel}" if rel else "standalone")
                    )
                    verified.append(f"{where}  ({how})")
                    continue
                if classify(b["body"]) == "whole":
                    root_unmarked.append(where)
                else:
                    fragments += 1
        unmarked_whole += root_unmarked
        if len(root_unmarked) > root.max_unmarked_whole:
            ERRORS.append(
                f"{root.path}: {len(root_unmarked)} unmarked whole-document "
                f"block(s), cap is {root.max_unmarked_whole}. A whole document "
                f"was added with no marker: give it `check`, "
                f'`check="<path>"`, `expect="<CODE>"`, or '
                f'`unverified="<reason>"`.\n'
                + "\n".join(f"      {w}" for w in root_unmarked)
            )

    caps = capability_versions(lute)
    hashes = check_pinned_hashes(caps, all_pages)

    if len(verified) < MIN_VERIFIED_BLOCKS:
        ERRORS.append(
            f"snippet coverage dropped: {len(verified)} verified block(s), "
            f"floor is {MIN_VERIFIED_BLOCKS}. A marker was removed rather than "
            f"a snippet fixed. Restore it, or lower MIN_VERIFIED_BLOCKS in this "
            f"script with a reason."
        )

    # Always report coverage, pass or fail. A check that quietly skips is the
    # exact failure mode this script exists to prevent.
    print(f"check-doc-snippets: roots scanned ({len(all_pages)} page(s)):")
    for root in SNIPPET_ROOTS:
        print(f"  · {root.path}/**{''.join(root.suffixes)}  ({len(root.pages())} page(s))")
        for excluded, why in root.exclude:
            print(f"      excluded: {excluded}/** — {why}")
    print(f"check-doc-snippets: {len(verified)} block(s) verified:")
    for v in verified:
        print(f"  ✓ {v}")
    print(
        f"check-doc-snippets: {len(opted_out)} block(s) explicitly unverified, "
        f"{len(unmarked_whole)} whole document(s) unmarked, "
        f"{fragments} fragment(s) with no declared outcome:"
    )
    for where, why in opted_out:
        print(f"  – {where}  unverified: {why}")
    for where in unmarked_whole:
        print(f"  ? {where}  whole document with no marker")
    for where, meta in other_meta:
        print(f"  · {where}  fence meta not read by this check: {meta}")
    print(
        f"check-doc-snippets: {hashes} pinned capability hash(es) checked "
        f"against {len(caps)} reference project(s):"
    )
    for k, v in caps.items():
        print(f"  · {v[:16]}…  {k}")

    if ERRORS:
        print("\ncheck-doc-snippets: FAILURES\n", file=sys.stderr)
        for e in ERRORS:
            print(f"  - {e}", file=sys.stderr)
        return 1

    print("check-doc-snippets: OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
