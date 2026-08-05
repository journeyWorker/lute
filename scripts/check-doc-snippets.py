#!/usr/bin/env python3
r"""Compile-check the ```lute snippets in the documentation surface.

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


DECLARATION BLOCKS (```yaml WITH A TOP-LEVEL `state:`)
------------------------------------------------------

Backlog #10 row c shipped a non-parsing `state:` declaration on the reference
page FOR state declarations — `app.rating: { type: enum, values: [...] }`,
which is not the language's shape — and nothing here caught it, because the
extractor above only ever read ```lute fences. A declaration block became
compile-checkable the moment `lute check` learned to read a `.yaml` target as a
schema instead of parsing it as a scene (#21/T3.9), so it is gated now: the
body is written to a scratch `*.schema.yaml` and `lute check` is run over it.

Admission is deliberately narrow, because a ```yaml fence in these docs is one
of four different documents. Only a SCHEMA is checkable as one, so a fence
qualifies only when every top-level key it declares is in `SCHEMA_DOC_KEYS`
(the mirror of `UNIVERSAL_KEYS` in `crates/lute-check/src/meta.rs`). That
excludes, measured against the current tree:

  * `--mock` playthroughs (`tracing.md`, `build-an-investigation.mdx`), which
    carry `choose:`/`events:`/`accepts:` beside `state:`;
  * `*.test.yaml` scenario tests (`cli.md`, `investigation/README.md`), which
    carry `file:`/`expect:`;
  * document frontmatter shown as YAML (`frontmatter-and-profiles.md`), which
    carries `kind:`/`character:`/`season:`.

All three check as schemas only by accident, and admitting them would have
reported five failures on a correct tree — the false-failure trap that made
checkability a DECLARED property everywhere else in this script. The same
`expect="<CODE>"` / `unverified="<reason>"` fence meta applies here as it does
to a ```lute fence, so a page that deliberately shows a broken declaration can
still say so.


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

QUOTED DIAGNOSTIC TEXT
----------------------

The docs quote diagnostic *messages* verbatim, and 0.9.0's messages are
deliberately instructive — `E-DOMAIN-UNKNOWN` enumerates all three declaration
routes, `E-DOMAIN-DUP` names the two ways out. Those strings ARE the feature's
user experience, and nothing above reads them: the markers up there assert exit
codes and code SETS (`DIAG_RE` captures the bracketed code and throws the
message away). That gap was measured, not assumed — rewording `E-DOMAIN-DUP` in
`crates/lute-check/src/schema_import.rs` left this script at exit 0 while
`language/vocabulary.md` quoted a sentence the binary no longer emits.

So a fenced block holding real CLI diagnostic output is DECLARED, the same way
a checkable snippet is, with a marker on the line above the fence:

    <!-- lute-diagnostics -->                     (.md — an HTML comment)
    {/* lute-diagnostics */}                      (.mdx — MDX has no HTML comments)

    <!-- lute-diagnostics unverified="<reason>" -->
        The reviewed opt-out, spelled exactly like the fence-meta one: for a
        quote that is deliberately abridged or illustrative. An empty reason is
        an error, and the reason is printed on every run.

Every diagnostic-shaped line in a marked fence is parsed, unwrapped, and pinned
to the Rust source. An UNMARKED fence containing diagnostic-shaped lines is
reported and capped per root at `max_unmarked_diagnostics` (0), so adding a
quote without a marker fails; the pinned-record count is floored by
`MIN_PINNED_DIAGNOSTICS`, so deleting a marker fails. Same floor-and-cap shape
as the snippet counters above, for the same reason.

The two `packages/website/public/llms*.txt` bundles are covered too, under a
STRICTER rule. They are excluded from the ```lute scan above for a measured
reason (see the comment on SNIPPET_ROOTS) that does not apply here: a quoted
message is the binary's or it is not, no project layout required. They are
flattened copies of pages whose every quote is now declared, so there is no
independent authoring decision to declare in them — an unmarked quote there
must simply PIN. The marker still works if a mirrored quote ever needs the
`unverified=` escape hatch; it is only not required. llms-full.txt:847 is a
byte-identical copy of the `E-DOMAIN-UNKNOWN` sentence on
`language/vocabulary.md`, i.e. exactly the copy a reword would strand.


WHERE THE EXPECTED TEXT COMES FROM: the Rust sources, not a fixture run
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

Everything else in this script verifies against a real `lute` run, so running
the binary was the first candidate here too. It was rejected on measurement.
Of the 26 quoting fences, 8 quote plugin-resolution failures
(`E-PLUGIN-OPTION-TYPE`, `E-PLUGIN-RESERVED-STAMP-ATTR`, `E-LOWER-RECORD-*`,
`E-FRONTMATTER-SCHEMA`) that only a deliberately-malformed plugin manifest can
produce. A run-based pin needs one such fixture per quote: ~8 new broken
mini-projects that must live somewhere `check-project docs/examples` and the
capability-hash pin above do not see, each of which is itself a thing that
rots, and none of which makes the comparison any sounder than an exact string
match. The fixture is a means of obtaining the string; the string is the
contract.

The stated risk of scraping — that a source literal is not what the binary
prints — is not waved away, it is MEASURED on every run. `check_message_fidelity`
takes the real diagnostic output the `expect=`-marked blocks above already
produce, feeds it through the same parser and the same matcher, and floors the
number of real messages that resolve to a scraped literal at
`MIN_FIDELITY_SAMPLES`. If message composition ever stops being "one `format!`
literal per diagnostic", that floor breaks before a doc quote can silently
diverge. It costs no extra process: those runs happen anyway.


WHY THE MATCHER CANNOT PASS BY BEING VAGUE
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

A matcher whose own regex is too loose reports success while the defect ships,
which is worse than no matcher. Five independent constraints, every one of them
enforced per quote per run:

  1. FULL ANCHOR. The doc's normalised message must match a literal end to end
     (`\Z`). Substring matching is never used.
  2. ONLY REAL PLACEHOLDERS BECOME WILDCARDS. `PLACEHOLDER_RE` accepts Rust's
     format grammar and nothing else — `{name}`, `{}`, `{0}`, `{name:spec}`.
     A brace run like `{ type: bool, cel: "true" }` is NOT a placeholder and
     stays literal. (It is a real literal in lute-lsp, and under a sloppier
     placeholder rule it compiled to bare `(.+?)` — a pattern matching every
     string in the corpus. That is the exact failure this rule exists to stop.)
  3. ADMISSION FLOOR. A literal joins the corpus only with >= 16 characters of
     literal text AND one unbroken literal run of >= 8. Near-all-wildcard
     format strings (`{prefix}.{speaker}_{code}`) never become patterns.
  4. BOUNDED WILDCARDS. A wildcard matches at least one character, never a
     newline (messages are normalised to one line), and never an elision (`…`
     or `...`) — an elided quote is abridged and must say so with
     `unverified=`. A wildcard span longer than `MAX_FREE_INTERPOLATION` must
     ITSELF resolve to a corpus literal, which is what pins the inner half of a
     composed message such as `E-LOWER-RECORD-FIELD`.
  5. MUTATION SELF-TEST. For every quote, every pinned literal run of >= 3
     characters is overwritten in the DOC text with a canary and the match is
     re-attempted. If any mutant still matches, the pattern is not actually
     pinning that run and the check fails with `matcher too permissive`. This
     is a proof per quote, not a threshold: it fails loudly rather than
     silently passing.

Two further constraints make the pin name the right diagnostic: the matching
literal must be UNIQUE in the corpus, and it must live in a source file that
also declares the quoted code as a string literal.


NORMALISATION — EXACTLY THESE RULES, AND NO MORE
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

Docs wrap long messages; the binary prints one line. Unwrapping is where a
comparison turns into a loophole, so the rules are deliberately minimal and are
asserted on every run by `self_test()`:

  D1. A record STARTS at a line matching one of the three shapes the CLI emits:
      `<path>:L:C: <sev> [CODE] …`, `<sev> [CODE] …` (the position elided, as
      the reference pages write it), or `lute: CODE: …`. The header — position,
      severity, brackets, and the `[denied]` promotion marker — is STRUCTURE,
      parsed and removed; only the message is compared.
  D2. A record ENDS at a blank line, another header, a `$ ` command echo, or an
      `ok:` / `failed:` / `--deny` / `project-wide diagnostics:` summary line.
  D3. Continuation lines are `strip()`ped and joined with EXACTLY ONE space.
      A documented wrap point therefore ASSERTS that a single space exists at
      that point in the emitted message. Wrapping mid-word does not become a
      match: `docu-\nment` normalises to `docu- ment`, which is not `document`.
  D4. Nothing else is touched. Runs of spaces inside a line, case, punctuation,
      Unicode dashes and the `…` inside `@speaker{…}` all survive byte-for-byte
      into the comparison. There is no whitespace collapsing, no case folding,
      no punctuation smoothing — each of those would let two genuinely
      different messages compare equal, which is the failure mode of reporting
      success while the defect ships.

The one thing D3 cannot see is whether the emitted message had two spaces where
the doc wrapped. That is the entire residual, it is stated rather than hidden,
and the CLI emits single-spaced prose.


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
    #: Cap on fences quoting diagnostic output with no `lute-diagnostics`
    #: marker. May only be lowered; marking a fence is always allowed.
    max_unmarked_diagnostics: int = 0
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

# A top-level key at column 0 of a fence body.
TOP_KEY_RE = re.compile(r"^([A-Za-z_][\w.-]*):")

# The keys a `*.schema.yaml` declaration map may carry — a mirror of
# `UNIVERSAL_KEYS` in `crates/lute-check/src/meta.rs`, which is what
# `lute check <file>.yaml` validates a bare YAML target against. A ```yaml
# fence declaring `state:` is admitted as a schema only when EVERY top-level
# key it declares is in here; see the module docstring for the three document
# kinds this excludes and why admitting them reports failures on a correct
# tree. `kind:` is deliberately absent (meta.rs handles it separately, and it
# is exactly what marks a fence as document frontmatter rather than a schema).
SCHEMA_DOC_KEYS = frozenset(
    (
        "mode",
        "title",
        "luteVersion",
        "contentLang",
        "profile",
        "plugins",
        "uses",
        "extends",
        "state",
        "defs",
        "enums",
        "entities",
        "relations",
        "facts",
        "rules",
        "components",
    )
)

# Gated declaration blocks may only grow, for the same reason as the line
# above: deleting the `state:` from a fence to dodge a failure fails instead.
MIN_STATE_BLOCKS = 14

# ---------------------------------------------------------------------------
# Quoted diagnostic text. See the module docstring for the full rationale.
# ---------------------------------------------------------------------------

# The marker that declares "the fence below quotes real CLI diagnostic output".
# Two spellings for one concept, because the file formats differ: `.md` takes an
# HTML comment, `.mdx` has none (MDX 3 rejects `<!--`) and takes a JSX comment.
# Both are invisible in rendered output.
DIAG_MARKER_RE = re.compile(
    r"^[ \t]*(?:<!--|\{/\*)[ \t]*lute-diagnostics\b(?P<meta>.*?)[ \t]*(?:-->|\*/\})[ \t]*$"
)
DIAG_MARKERS = ("unverified",)

# D1 — the three header shapes the CLI emits, plus the `[denied]` promotion
# marker `lute-cli` appends to the bracketed code. Everything these capture is
# STRUCTURE and is stripped before comparison; only `rest` is the message.
_SEV = r"(?P<sev>error|warning) \[(?P<code>[A-Z][A-Z0-9-]*)\](?P<denied> \[denied\])?"
QUOTE_HEAD_RES = (
    # `path:L:C: error [CODE] …`, and the position-only form the repo README
    # writes under an indented path heading (`  26:3: warning [CODE] …`).
    re.compile(r"^[ \t]*(?:(?P<path>[^\s:]+):)?(?P<line>\d+):(?P<col>\d+): " + _SEV + r" (?P<rest>.*)$"),
    # The position elided entirely, as the reference pages write it.
    re.compile(r"^[ \t]*" + _SEV + r" (?P<rest>.*)$"),
    # A project-level diagnostic: no span in any one file.
    re.compile(r"^[ \t]*lute: (?P<code>[A-Z][A-Z0-9-]+): (?P<rest>.*)$"),
)

# D2 — a record ends here. (A new header is checked before this.)
QUOTE_STOP_RE = re.compile(r"^\s*$|^\s*\$ |^(?:ok|failed): |^--deny |^project-wide diagnostics:")

# Rust's format-argument grammar and nothing else: `{}`, `{0}`, `{name}`,
# `{name:spec}`, and the `{{` / `}}` brace escapes. A brace run that is not one
# of these is ordinary text and stays literal — see docstring constraint 2.
PLACEHOLDER_RE = re.compile(r"\{\{|\}\}|\{(?:[A-Za-z_][A-Za-z0-9_]*|[0-9]+)?(?::[^{}]*)?\}")

# Crates whose `format!` literals can reach a user-visible diagnostic. `/src/`
# only: `tests/` fixtures quote codes and messages for their own assertions and
# would let a pin survive on a test's copy of a string the binary stopped using.
MESSAGE_SOURCE_GLOB = "crates/*/src/**/*.rs"

# Corpus admission (docstring constraint 3).
MIN_LITERAL_CHARS = 16
MIN_LITERAL_RUN = 8

# An interpolated value longer than this must itself resolve to a corpus
# literal, which is what pins the inner half of a composed message. Measured
# headroom: the longest genuine value in the docs today is the 55-character
# staging-kind list in `E-LOWER-RECORD-UNKNOWN`.
MAX_FREE_INTERPOLATION = 64

# An elision marker inside an interpolated span means the quote is abridged.
ELISION_RE = re.compile(r"…|\.\.\.")

# Overwrites a pinned literal run in the mutation self-test. Cannot occur in
# source text, so a surviving match can only come from a permissive pattern.
CANARY = "\x01"

# Pinned diagnostic records may only grow: deleting a marker fails here even
# before the per-root unmarked cap catches the fence.
MIN_PINNED_DIAGNOSTICS = 48

# Real messages, harvested from the `expect=` runs above, that must resolve
# against the scraped corpus. This is the standing proof that a `format!`
# literal is still what the binary prints.
MIN_FIDELITY_SAMPLES = 12

ERRORS: list[str] = []

#: Every `lute` invocation's cleaned output, in call order. Sampled by
#: `check_message_fidelity`.
REAL_OUTPUT: list[str] = []


def fail(msg: str) -> "None":
    print(f"check-doc-snippets: FATAL: {msg}", file=sys.stderr)
    sys.exit(2)


def parse_meta(
    meta: str, allowed: tuple[str, ...] = MARKERS
) -> tuple[dict[str, str | None], list[str]]:
    """Split a marker meta string into recognised markers and everything else."""
    known: dict[str, str | None] = {}
    other: list[str] = []
    for m in META_RE.finditer(meta):
        if m.group(1) is None:
            other.append(m.group(4))
        elif m.group(1) in allowed:
            known[m.group(1)] = m.group(2) if m.group(2) is not None else m.group(3)
        else:
            other.append(m.group(0))
    return known, other


def extract_fences(path: pathlib.Path) -> list[dict]:
    """Every fenced block in `path`.

    Each entry carries the 1-based opening line, the info string, the fence
    meta, the body lines, and `diag_meta` — the meta of a `lute-diagnostics`
    marker owning this fence, or None when there is none. A marker owns the
    fence below it across blank lines only; anything else in between and the
    fence is unmarked, which is the safe direction.
    """
    lines = path.read_text(encoding="utf-8").splitlines()
    fences: list[dict] = []
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
                j = opener[3] - 1
                while j >= 0 and not lines[j].strip():
                    j -= 1
                marker = DIAG_MARKER_RE.match(lines[j]) if j >= 0 else None
                fences.append(
                    {
                        "line": opener[3] + 1,
                        "info": opener[2],
                        "meta": opener[4],
                        "body": body,
                        "diag_meta": marker.group("meta") if marker else None,
                        "marker_line": j + 1 if marker else None,
                    }
                )
                opener = None
                continue
        if opener is not None:
            body.append(line)
    if opener is not None:
        ERRORS.append(f"{path.relative_to(ROOT)}:{opener[3] + 1}: unterminated code fence")
    return fences


def extract_blocks(fences: list[dict]) -> list[dict]:
    """The ```lute subset, with its body joined back into one document."""
    return [
        {"line": f["line"], "meta": f["meta"], "body": "\n".join(f["body"])}
        for f in fences
        if f["info"] == "lute"
    ]


def extract_state_blocks(fences: list[dict]) -> list[dict]:
    """The ```yaml subset that is a state-schema declaration map.

    Backlog #10 row c shipped a non-parsing `state:` declaration on the
    reference page for state declarations, and nothing caught it because the
    gate only read ```lute fences. A declaration block is compile-checkable the
    moment `lute check` recognises a `.yaml` schema (#21/T3.9), so it is gated
    here.

    Admitted only when the fence declares a top-level `state:` AND every other
    top-level key is a schema-document key. A mock playthrough, a
    `*.test.yaml`, and a document's frontmatter all declare `state:` too and
    are not schemas; see the module docstring.
    """
    out = []
    for f in fences:
        if f["info"] not in ("yaml", "yml"):
            continue
        body = "\n".join(f["body"])
        keys = {m.group(1) for m in (TOP_KEY_RE.match(l) for l in body.splitlines()) if m}
        if "state" not in keys or not keys <= SCHEMA_DOC_KEYS:
            continue
        out.append({"line": f["line"], "meta": f["meta"], "body": body})
    return out


def run_state_block(lute: str, body: str) -> tuple[int, str]:
    """`lute check` over `body` written as a scratch `*.schema.yaml`."""
    with tempfile.TemporaryDirectory() as tmp:
        target = pathlib.Path(tmp) / "doc.schema.yaml"
        target.write_text(body.rstrip("\n") + "\n", encoding="utf-8")
        rc, out = run_lute(lute, ["check", str(target)])
        out = out.replace(str(target), "<block>")
    REAL_OUTPUT.append(out)
    return rc, out


def check_state_block(
    lute: str, body: str, where: str, expect: set[str] | None = None
) -> None:
    """Gate one declaration block, clean by default or against `expect`."""
    rc, out = run_state_block(lute, body)
    if expect is None:
        if rc != 0:
            ERRORS.append(
                f"{where}: ```yaml declaration block does not check clean as a "
                f"schema (exit {rc}):\n{indent(out)}"
            )
        return
    got = error_codes(out)
    if rc == 0:
        ERRORS.append(
            f"{where}: declaration block expected {','.join(sorted(expect))} "
            f"but checks clean as a schema"
        )
    elif got != expect:
        ERRORS.append(
            f"{where}: declaration block expected {','.join(sorted(expect))}, "
            f"got {','.join(sorted(got)) or '(none)'}:\n{indent(out)}"
        )


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
    # Kept for `check_message_fidelity`: this is real, current binary output,
    # and it costs nothing to reuse as the standing proof that the scraped
    # `format!` literals are still what the binary prints.
    REAL_OUTPUT.append(out)
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


# ---------------------------------------------------------------------------
# Quoted diagnostic text: scrape, normalise, pin.
# ---------------------------------------------------------------------------


def rust_string_literals(text: str) -> list[str]:
    r"""Every string literal in one Rust source, comments excluded.

    Comments are skipped because doc comments in this codebase discuss codes
    and quote message fragments; a pin that could satisfy itself from a comment
    would survive the very edit it exists to catch. Escapes are decoded the way
    rustc decodes them — in particular a trailing `\\` swallows the newline AND
    the next line's leading whitespace, which is how every long message in
    `crates/**` is written, so the reconstruction is exact rather than guessed.
    """
    out: list[str] = []
    i, n = 0, len(text)
    while i < n:
        c = text[i]
        if c == "/" and text.startswith("//", i):
            j = text.find("\n", i)
            i = n if j < 0 else j + 1
            continue
        if c == "/" and text.startswith("/*", i):
            depth, i = 1, i + 2
            while i < n and depth:
                if text.startswith("/*", i):
                    depth, i = depth + 1, i + 2
                elif text.startswith("*/", i):
                    depth, i = depth - 1, i + 2
                else:
                    i += 1
            continue
        if c == "r" and i + 1 < n and text[i + 1] in '"#':
            k = i + 1
            while k < n and text[k] == "#":
                k += 1
            if k < n and text[k] == '"':
                close = '"' + "#" * (k - i - 1)
                j = text.find(close, k + 1)
                if j < 0:
                    break
                out.append(text[k + 1 : j])
                i = j + len(close)
                continue
        if c == '"':
            j, buf = i + 1, []
            while j < n:
                ch = text[j]
                if ch == "\\":
                    nxt = text[j + 1] if j + 1 < n else ""
                    if nxt == "\n":
                        j += 2
                        while j < n and text[j] in " \t":
                            j += 1
                        continue
                    simple = {"n": "\n", "t": "\t", "r": "\r", "\\": "\\", '"': '"', "'": "'", "0": "\0"}
                    if nxt in simple:
                        buf.append(simple[nxt])
                        j += 2
                        continue
                    uni = re.match(r"\\u\{([0-9a-fA-F_]+)\}", text[j:])
                    if nxt == "u" and uni:
                        buf.append(chr(int(uni.group(1).replace("_", ""), 16)))
                        j += len(uni.group(0))
                        continue
                    buf.append(nxt)
                    j += 2
                    continue
                if ch == '"':
                    break
                buf.append(ch)
                j += 1
            out.append("".join(buf))
            i = j + 1
            continue
        if c == "'":
            chlit = re.match(r"'(\\.|[^\\'])'", text[i:])
            i += len(chlit.group(0)) if chlit else 1
            continue
        i += 1
    return out


#: A diagnostic code as it appears in the Rust sources: a standalone literal.
CODE_LITERAL_RE = re.compile(r"^[EW]-[A-Z0-9]+(?:-[A-Z0-9]+)*\Z")


@dataclass(frozen=True)
class MessagePattern:
    """One admitted `format!` literal, compiled for full-anchored matching."""

    literal: str
    regex: re.Pattern[str]
    sources: tuple[str, ...]
    #: Every diagnostic code declared as a literal in any of `sources`. A quote
    #: may only pin to a literal whose own file declares the quoted code.
    codes: frozenset[str]


def compile_message_pattern(literal: str) -> re.Pattern[str] | None:
    """Compile `literal` if it clears the admission floor, else None."""
    if "\n" in literal:
        return None
    parts: list[str] = []
    runs: list[str] = []
    i = 0
    for m in PLACEHOLDER_RE.finditer(literal):
        seg = literal[i : m.start()]
        parts.append(re.escape(seg))
        runs.append(seg)
        tok = m.group(0)
        if tok in ("{{", "}}"):
            parts.append(re.escape(tok[0]))
            runs.append(tok[0])
        else:
            # At least one character, never a newline: a message is one line
            # after normalisation, and an empty interpolation is not a value.
            parts.append(r"(.+?)")
        i = m.end()
    seg = literal[i:]
    parts.append(re.escape(seg))
    runs.append(seg)
    if sum(len(r) for r in runs) < MIN_LITERAL_CHARS:
        return None
    if max((len(r.strip()) for r in runs), default=0) < MIN_LITERAL_RUN:
        return None
    try:
        return re.compile("".join(parts) + r"\Z")
    except re.error:
        return None


def message_corpus(sources: dict[str, list[str]] | None = None) -> list[MessagePattern]:
    """Every admitted message literal in `crates/*/src/**`, deduplicated."""
    if sources is None:
        sources = {}
        for f in sorted(ROOT.glob(MESSAGE_SOURCE_GLOB)):
            sources[str(f.relative_to(ROOT))] = rust_string_literals(
                f.read_text(encoding="utf-8")
            )
    owners: dict[str, set[str]] = {}
    codes_in: dict[str, set[str]] = {}
    for rel, lits in sources.items():
        for lit in lits:
            owners.setdefault(lit, set()).add(rel)
            if CODE_LITERAL_RE.match(lit):
                codes_in.setdefault(rel, set()).add(lit)
    corpus: list[MessagePattern] = []
    for lit, rels in sorted(owners.items()):
        rx = compile_message_pattern(lit)
        if rx is not None:
            codes = frozenset().union(*(codes_in.get(r, set()) for r in rels))
            corpus.append(MessagePattern(lit, rx, tuple(sorted(rels)), codes))
    return corpus


def unwrap_records(body: list[str]) -> list[tuple[int, str, str]]:
    """Diagnostic records in a fence body, per normalisation rules D1-D4.

    Returns (0-based offset of the header line within the body, code, message).
    """
    recs: list[tuple[int, str, str]] = []
    cur: list | None = None
    for off, line in enumerate(body):
        head = next((m for r in QUOTE_HEAD_RES if (m := r.match(line))), None)
        if head is not None:
            if cur is not None:
                recs.append((cur[0], cur[1], cur[2]))
            cur = [off, head.group("code"), head.group("rest").rstrip()]
            continue
        if cur is None:
            continue
        if QUOTE_STOP_RE.match(line):
            recs.append((cur[0], cur[1], cur[2]))
            cur = None
            continue
        # D3: exactly one space at a documented wrap point.
        cur[2] = cur[2] + " " + line.strip()
    if cur is not None:
        recs.append((cur[0], cur[1], cur[2]))
    return recs


def mutation_survivors(rx: re.Pattern[str], match: re.Match[str], text: str) -> list[str]:
    """Pinned literal runs of `text` that the pattern does NOT actually pin.

    Overwrites each run with `CANARY` and re-matches. A run whose destruction
    the pattern tolerates was never load-bearing, and a matcher built only from
    such runs is the "passes because the regex is too loose" failure.
    """
    spans: list[tuple[int, int]] = []
    prev = 0
    for g in range(1, rx.groups + 1):
        s, e = match.span(g)
        if s < 0:
            continue
        spans.append((prev, s))
        prev = e
    spans.append((prev, len(text)))
    survivors = []
    for s, e in spans:
        run = text[s:e]
        if len(run.strip()) < 3:
            continue
        if rx.match(text[:s] + CANARY * (e - s) + text[e:]):
            survivors.append(run)
    return survivors


def pin_message(
    corpus: list[MessagePattern], code: str, msg: str, depth: int = 0
) -> tuple[MessagePattern | None, str | None]:
    """Resolve one normalised message against the corpus.

    Returns (pattern, None) on success, (None, reason) on failure. Every
    constraint in the docstring's matcher section is applied here.
    """
    hits = [(mp, m) for mp in corpus if (m := mp.regex.match(msg))]
    if not hits:
        return None, (
            "no `format!` literal in crates/*/src matches this text — the "
            "message was reworded, or the doc drifted from it"
        )
    if depth == 0:
        hits = [(mp, m) for mp, m in hits if code in mp.codes]
        if not hits:
            return None, (
                f"text matches a literal, but no source declaring `{code}` — "
                f"the quote and the code do not belong together"
            )
    if len({mp.literal for mp, _ in hits}) > 1:
        return None, f"ambiguous: {len(hits)} distinct literals match this text"
    mp, m = hits[0]
    for g in m.groups():
        if not g:
            continue
        if ELISION_RE.search(g):
            return None, (
                f"the interpolated span {g[:60]!r} contains an elision — the "
                f"quote is abridged, so declare it "
                f'`unverified="<reason>"` instead of pinning it'
            )
        if len(g) <= MAX_FREE_INTERPOLATION:
            continue
        if depth >= 2:
            return None, f"composed message nested too deep at {g[:60]!r}"
        _, why = pin_message(corpus, code, g, depth + 1)
        if why is not None:
            return None, f"unpinned {len(g)}-character span {g[:60]!r}: {why}"
    if depth == 0:
        loose = mutation_survivors(mp.regex, m, msg)
        if loose:
            return None, (
                "matcher too permissive: destroying the pinned run(s) "
                + ", ".join(repr(r[:40]) for r in loose)
                + " still matches. Refusing to report a pass this pattern did "
                "not earn."
            )
    return mp, None


#: Self-contained documents whose only purpose is to make the binary SPEAK.
#: They carry no fixture files and no project layout — every one is a string
#: this script writes to a temp dir — and between them they cover the
#: diagnostics the tutorials quote most: the missing-frontmatter set, the
#: undeclared-domain sentence, the legacy-sigil migration hint, the
#: member-semantics pair and a bad enum value.
FIDELITY_PROBES = (
    # No frontmatter at all: E-KIND-MISSING, E-META-MISSING x3,
    # E-CONTENT-OUTSIDE-SHOT.
    "@narrator: bare.\n",
    # E-STATE-DECL, E-DOMAIN-UNKNOWN, E-LEGACY-CONTENT-SIGIL.
    "---\nkind: scene\ncharacter: probe\nseason: 1\nepisode: 1\n"
    "state:\n  run.inventory: { type: list }\n---\n"
    '\n## Probe\n\n@narrator{emotion="furious"}: hello.\n\n:narrator: legacy.\n',
    # E-ENUM-MISSING-SEMANTICS x2, E-BAD-ENUM.
    "---\nkind: scene\ncharacter: probe\nseason: 1\nepisode: 1\n"
    "enums:\n  emotion: [neutral]\n  action: [wave]\n  anchor: [center]\n---\n"
    '\n## Probe\n\n@narrator{emotion="furious"}: hello.\n',
)


def check_message_fidelity(lute: str, corpus: list[MessagePattern]) -> list[tuple[str, str]]:
    """Resolve REAL binary output against the corpus. See docstring.

    A scraped literal is only ground truth while it is still what the binary
    prints, so that claim is measured rather than asserted: every message this
    binary emits — from the `expect=`-marked blocks above, which ran anyway,
    and from `FIDELITY_PROBES` — goes through the same parser and the same
    matcher as a doc quote, and the number that resolve is floored.
    """
    for probe in FIDELITY_PROBES:
        run_block(lute, probe, None)
    samples: dict[tuple[str, str], bool] = {}
    for out in REAL_OUTPUT:
        for _, code, msg in unwrap_records(out.splitlines()):
            if (code, msg) in samples:
                continue
            samples[(code, msg)] = pin_message(corpus, code, msg)[1] is None
    resolved = [k for k, ok in samples.items() if ok]
    if len(resolved) < MIN_FIDELITY_SAMPLES:
        ERRORS.append(
            f"message fidelity dropped: {len(resolved)} of {len(samples)} real "
            f"diagnostic message(s) emitted by this binary resolve to a "
            f"`format!` literal in crates/*/src, floor is "
            f"{MIN_FIDELITY_SAMPLES}. The scraped literals are the ground "
            f"truth every quoted diagnostic is pinned against, so they must "
            f"keep reproducing real output. Unresolved:\n"
            + "\n".join(
                f"      [{c}] {m[:110]}" for (c, m), ok in sorted(samples.items()) if not ok
            )
        )
    return sorted(resolved)


def check_quoted_diagnostics(
    corpus: list[MessagePattern],
    rel_page: pathlib.PurePath,
    fences: list[dict],
    pinned: list[str],
    opted_out: list[tuple[str, str]],
    unmarked: list[str] | None,
) -> None:
    """Pin, opt out, or report every fence in one file that quotes output.

    `unmarked is None` selects the MIRROR rule used for the llms bundles: they
    are flattened copies of pages whose every quote is already declared, so
    there is no independent authoring decision to declare there and an unmarked
    quote must simply PIN. The marker still works if a future mirrored quote
    needs the `unverified=` escape hatch; it is just not required.
    """
    for f in fences:
        where = f"{rel_page}:{f['line']}"
        recs = unwrap_records(f["body"])
        if f["diag_meta"] is None:
            if not recs:
                continue
            if unmarked is not None:
                unmarked.append(f"{where}  ({len(recs)} record(s), first is [{recs[0][1]}])")
                continue
        else:
            marker_at = f"{rel_page}:{f['marker_line']}"
            marks, other = parse_meta(f["diag_meta"], DIAG_MARKERS)
            if other:
                ERRORS.append(
                    f"{marker_at}: unrecognised `lute-diagnostics` option(s) "
                    f"{' '.join(other)} — the only option is "
                    f'unverified="<reason>"'
                )
                continue
            if not recs:
                ERRORS.append(
                    f"{marker_at}: `lute-diagnostics` marks a fence with no "
                    f"diagnostic output in it. A marker that pins nothing is a "
                    f"claim of coverage this check does not provide — drop it, "
                    f"or move it above the fence that holds the quote."
                )
                continue
            if "unverified" in marks:
                why = (marks["unverified"] or "").strip()
                if not why:
                    ERRORS.append(
                        f'{marker_at}: `unverified` needs a reason — write '
                        f'unverified="why this quote cannot be pinned"'
                    )
                    continue
                opted_out.append((f"{where}  ({len(recs)} record(s))", why))
                continue
        for off, code, msg in recs:
            at = f"{rel_page}:{f['line'] + 1 + off}"
            mp, why = pin_message(corpus, code, msg)
            if why is not None:
                ERRORS.append(
                    f"{at}: quoted `{code}` text is not what the binary "
                    f"emits — {why}.\n"
                    f"      documented: {msg}\n"
                    f"      Rewording a diagnostic silently invalidates every "
                    f"page quoting it; update this page, or restore the "
                    f"message in crates/*/src."
                )
                continue
            pinned.append(f"{at}  [{code}] -> {'/'.join(mp.sources)}")


def self_test() -> None:
    """Assert the normaliser and the matcher on their boundary cases.

    Runs unconditionally, on synthetic literals, before anything real is read.
    A comparator that has not been shown to reject is not evidence of a pass.
    """
    corp = message_corpus(
        {
            "crates/x/src/a.rs": [
                "slot `{slot}` needs a declaration before use (dsl 0.9.0 D-C)",
                "slot `{slot}` needs a declaration before reuse (dsl 0.9.0 D-C)",
                "the quick brown fox jumps over it",
                "E-BOUNDARY",
            ],
            "crates/x/src/b.rs": ["{a}.{b}_{c}", "{ type: bool, cel: \"true\" }"],
        }
    )
    lits = {mp.literal for mp in corp}

    def bad(why: str) -> None:
        fail(f"self-test: {why}")

    # Admission: a near-all-wildcard format string never becomes a pattern,
    # and a brace run that is not a format placeholder stays literal.
    if "{a}.{b}_{c}" in lits:
        bad("`{a}.{b}_{c}` was admitted; it is 2 literal characters of anchor")
    brace = '{ type: bool, cel: "true" }'
    if brace not in lits:
        bad("a non-placeholder brace run must stay a literal, not be dropped")
    brace_rx = next(mp.regex for mp in corp if mp.literal == brace)
    if brace_rx.match("anything at all here"):
        bad("a non-placeholder brace run compiled to a wildcard")

    # D1/D2/D3: header stripped, wrap points joined with exactly one space,
    # summary lines end the record.
    recs = unwrap_records(
        [
            "$ lute check s.lute",
            "s.lute:3:1: error [E-BOUNDARY] the quick brown",
            "fox jumps over it",
            "failed: s.lute (1 error(s), 0 warning(s))",
        ]
    )
    if recs != [(1, "E-BOUNDARY", "the quick brown fox jumps over it")]:
        bad(f"unwrapping is wrong: {recs!r}")
    if unwrap_records(["error [E-BOUNDARY] a", "", "b"])[0][2] != "a":
        bad("a blank line must end a record")
    if unwrap_records(["error [E-BOUNDARY] [denied] a b c d"])[0][2] != "a b c d":
        bad("the `[denied]` promotion marker is header, not message")

    def pin(msg: str) -> str | None:
        return pin_message(corp, "E-BOUNDARY", msg)[1]

    # D3 boundary: a wrap INSIDE a word must not normalise into the word.
    if pin("the quick brown fox jumps over it") is not None:
        bad("the exact message must pin")
    joined = unwrap_records(["error [E-BOUNDARY] the quick bro", "wn fox jumps over it"])[0][2]
    if joined != "the quick bro wn fox jumps over it":
        bad("a mid-word wrap must not be silently healed")
    if pin(joined) is None:
        bad("a mid-word wrap must NOT pin")
    # D4: no whitespace collapsing, no punctuation smoothing.
    if pin("the quick  brown fox jumps over it") is None:
        bad("a doubled internal space must NOT pin")
    if pin("the quick brown fox jumps over it.") is None:
        bad("trailing punctuation must NOT pin")
    if pin("The quick brown fox jumps over it") is None:
        bad("a case change must NOT pin")

    # Wildcards: never empty, never past their following anchor, never an
    # elision, and the whole text must be consumed.
    if pin_message(corp, "E-BOUNDARY", "slot `x` needs a declaration before use (dsl 0.9.0 D-C)")[1]:
        bad("a legitimate interpolation must pin")
    if pin("slot `` needs a declaration before use (dsl 0.9.0 D-C)") is None:
        bad("an empty interpolation must NOT pin")
    if pin("slot `x` needs a declaration before use (dsl 0.9.0 D-C) and more") is None:
        bad("trailing text outside the literal must NOT pin")
    if pin("slot `…` needs a declaration before use (dsl 0.9.0 D-C)") is None:
        bad("an elision inside an interpolated span must NOT pin")
    # Two literals one word apart stay distinguishable, and neither is claimed
    # by the other.
    if pin("slot `x` needs a declaration before reuse (dsl 0.9.0 D-C)") is not None:
        bad("the near-twin literal must pin to itself")
    if pin("slot `x` needs a declaration before misuse (dsl 0.9.0 D-C)") is None:
        bad("a reworded message must NOT pin to either near-twin")
    # A quote may not borrow a literal from a file that never declares its code.
    if pin_message(corp, "E-ELSEWHERE", "the quick brown fox jumps over it")[1] is None:
        bad("a quote must not pin to a literal whose file does not declare the code")

    # The mutation self-test itself must be able to condemn: an anchor that a
    # neighbouring wildcard can simply re-absorb is not pinning anything, and
    # `mutation_survivors` must say so rather than report a clean match.
    loose_rx = re.compile(r"(.+?)abc(.+?)\Z")
    loose_txt = "xabcabcx"
    if mutation_survivors(loose_rx, loose_rx.match(loose_txt), loose_txt) != ["abc"]:
        bad("mutation_survivors failed to condemn a re-absorbable anchor")


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

    self_test()
    corpus = message_corpus()

    verified: list[str] = []
    opted_out: list[tuple[str, str]] = []
    unmarked_whole: list[str] = []
    other_meta: list[tuple[str, str]] = []
    fragments = 0
    all_pages: list[pathlib.Path] = []
    pinned: list[str] = []
    diag_opted_out: list[tuple[str, str]] = []
    unmarked_diags: list[str] = []
    state_blocks: list[str] = []
    state_opted_out: list[tuple[str, str]] = []

    for root in SNIPPET_ROOTS:
        pages = root.pages()
        if not pages:
            fail(f"snippet root has no pages: {root.path}")
        all_pages += pages
        root_unmarked: list[str] = []
        root_unmarked_diags: list[str] = []
        for page in pages:
            rel_page = page.relative_to(ROOT)
            fences = extract_fences(page)
            check_quoted_diagnostics(
                corpus, rel_page, fences, pinned, diag_opted_out, root_unmarked_diags
            )
            for b in extract_blocks(fences):
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
            for b in extract_state_blocks(fences):
                where = f"{rel_page}:{b['line']}"
                marks, other = parse_meta(b["meta"])
                if other:
                    other_meta.append((where, " ".join(other)))
                if "unverified" in marks:
                    why = (marks["unverified"] or "").strip()
                    if not why:
                        ERRORS.append(
                            f'{where}: `unverified` needs a reason — write '
                            f'unverified="why this block cannot be checked"'
                        )
                        continue
                    state_opted_out.append((where, why))
                    continue
                expect: set[str] | None = None
                if "expect" in marks:
                    expect = {
                        c.strip() for c in (marks["expect"] or "").split(",") if c.strip()
                    }
                    if not expect:
                        ERRORS.append(
                            f'{where}: `expect` needs at least one diagnostic code — '
                            f'write expect="E-STATE-DECL"'
                        )
                        continue
                check_state_block(lute, b["body"], where, expect)
                how = f"expects {','.join(sorted(expect))}" if expect else "clean"
                state_blocks.append(f"{where}  (as a schema, {how})")
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
        unmarked_diags += root_unmarked_diags
        if len(root_unmarked_diags) > root.max_unmarked_diagnostics:
            ERRORS.append(
                f"{root.path}: {len(root_unmarked_diags)} fence(s) quoting "
                f"diagnostic output with no `lute-diagnostics` marker, cap is "
                f"{root.max_unmarked_diagnostics}. A quoted diagnostic with no "
                f"marker is a sentence the binary is free to rewrite behind the "
                f"docs' back: put `<!-- lute-diagnostics -->` "
                f"(`{{/* lute-diagnostics */}}` in .mdx) on the line above the "
                f"fence, or `<!-- lute-diagnostics unverified=\"<reason>\" -->` "
                f"if the quote is deliberately abridged.\n"
                + "\n".join(f"      {w}" for w in root_unmarked_diags)
            )

    # The llms bundles are flattened mirrors of pages whose every quote is now
    # declared, and they were already a scanned surface for the capability
    # pin. They get the MIRROR rule (see `check_quoted_diagnostics`): no marker
    # required, but every quote must pin. One of their eleven — the
    # `E-DOMAIN-UNKNOWN` sentence at llms-full.txt:847 — is a byte-identical
    # copy of a page quote, which is exactly the copy a reword would strand.
    for bundle in HASH_SCAN_EXTRA:
        if bundle.is_file():
            check_quoted_diagnostics(
                corpus,
                bundle.relative_to(ROOT),
                extract_fences(bundle),
                pinned,
                diag_opted_out,
                None,
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

    if len(state_blocks) < MIN_STATE_BLOCKS:
        ERRORS.append(
            f"declaration-block coverage dropped: {len(state_blocks)} gated "
            f"```yaml block(s), floor is {MIN_STATE_BLOCKS}. A `state:` fence "
            f"was reshaped out of admission rather than fixed. Restore it, or "
            f"lower MIN_STATE_BLOCKS in this script with a reason."
        )

    if len(pinned) < MIN_PINNED_DIAGNOSTICS:
        ERRORS.append(
            f"quoted-diagnostic coverage dropped: {len(pinned)} pinned "
            f"record(s), floor is {MIN_PINNED_DIAGNOSTICS}. A "
            f"`lute-diagnostics` marker was removed rather than a quote fixed. "
            f"Restore it, or lower MIN_PINNED_DIAGNOSTICS in this script with a "
            f"reason."
        )

    fidelity = check_message_fidelity(lute, corpus)

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
        f"check-doc-snippets: {len(state_blocks)} ```yaml declaration block(s) "
        f"compile-checked as schemas (floor {MIN_STATE_BLOCKS}):"
    )
    for v in state_blocks:
        print(f"  ✓ {v}")
    for where, why in state_opted_out:
        print(f"  – {where}  unverified: {why}")
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
    print(
        f"check-doc-snippets: {len(pinned)} quoted diagnostic record(s) pinned "
        f"to {len(corpus)} `format!` literal(s) scanned from "
        f"{MESSAGE_SOURCE_GLOB}:"
    )
    for p in pinned:
        print(f"  ✓ {p}")
    print(
        f"check-doc-snippets: {len(diag_opted_out)} quoted diagnostic(s) "
        f"explicitly unverified, {len(unmarked_diags)} quoting fence(s) "
        f"unmarked:"
    )
    for where, why in diag_opted_out:
        print(f"  – {where}  unverified: {why}")
    for where in unmarked_diags:
        print(f"  ? {where}  quotes diagnostic output with no marker")
    print(
        f"check-doc-snippets: {len(fidelity)} real message(s) emitted by this "
        f"binary resolve to a scraped literal (floor {MIN_FIDELITY_SAMPLES}):"
    )
    for code, msg in fidelity:
        print(f"  · [{code}] {msg[:96]}{'…' if len(msg) > 96 else ''}")

    if ERRORS:
        print("\ncheck-doc-snippets: FAILURES\n", file=sys.stderr)
        for e in ERRORS:
            print(f"  - {e}", file=sys.stderr)
        return 1

    print("check-doc-snippets: OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
