# Scene `id:` — authored scene identity design

Date: 2026-09-01
Status: approved design, pre-implementation

## Problem

A scene's canonical key is the derived join `{character}.{episodeId}` (with
`episodeId` defaulting to `s{season:02}ep{episode:02}`), and `character`,
`season`, `episode` are **required** frontmatter (`E-META-MISSING`). That
ontology assumes a character-centric episodic show. Real projects have main
stories, location stories, and team-specific naming rules — and the language
has already drifted to admit this: `docs/schedule-and-play.md` §"Canonical
node identity" instructs authors to write `character: <event>-<variant>` and
treat `season:`/`episode:` as "opaque, frozen identity numbers". The flagship
example writes `character: anseo` — a work title, not a person. Three
required keys have degenerated into an opaque id you must author in three
pieces, two of them forced to be integers.

Quest documents already carry their identity as one authored `<quest id>`
(dsl 0.2.0 §7: "prefix = the enclosing stable declaration id"). Scenes are
the one root kind still bound to the derived join.

## Decision summary

| Question | Decision |
| --- | --- |
| Identity source | New optional scene frontmatter `id:`. When present, it IS the canonical scene key everywhere. |
| Fallback | `id:` absent → derived `{character}.{episodeId}` exactly as today, byte-identical. No migration, no lineId churn. |
| Required keys | `id:` present → `character`/`season`/`episode` no longer required. `id:` absent → required as today. |
| Legacy keys | Deprecated when coexisting with authored `id:` — per-key warning `W-META-LEGACY` ("move under `extra:`"), fix-it, `--deny`-promotable. Legacy-only documents stay silent. |
| Descriptive metadata | New `extra:` frontmatter block: open mapping, scalar / scalar-list values, never read by language semantics, carried verbatim into the artifact for search tooling. Legal on Scene and Quest roots. |
| Duplicate detection | `E-CONN-EPISODE-ID-DUP` code kept (tooling stability); message generalized to "canonical scene id"; authored-id collisions anchor at `id:`. Derived and authored keys share one namespace — a collision between them is the same diagnostic. |
| `defaults:` | `id` NOT defaultable (must be per-document unique; stays outside `DEFAULTABLE_KEYS` → `E-DEFAULTS-KEY`). `extra` IS defaultable (whole-value-per-key rule unchanged). |
| IR | `SceneMeta` gains `id` (always present, the resolved canonical key); `character`/`season`/`episode`/`episodeId` become optional (emitted when authored/derivable); both meta kinds gain optional `extra` (the descriptive block, omitted when empty). IR minor bump. |
| Version | Language + IR → **0.15.0**. Grammar untouched (frontmatter is one opaque token) → no tree-sitter `capabilityVersion` restamp. |

## 1. `id:` — the authored canonical key

Scene-only frontmatter key (joins `SCENE_KEYS` legality). Value: non-empty
string, no whitespace, `[A-Za-z0-9_.-]+` (superset of every derived key the
fallback can produce; `.` admits namespacing like `anseo.s01ep01`). A value
outside the charset is `E-META-ID` (error, anchored at `id:`).

When present it is the single canonical scene key consumed by:

- the lineId prefix (`identity:` templates' `{prefix}`) and the structural
  choice/hub option ids `{prefix}.{branchOrHubId}.{optionId}`
  (`lute-compile/src/address.rs`, `lute-cli/src/loc.rs::scene_prefix`);
- `visited('…')` resolution and suggestion (`connectivity.rs::check_formula_atoms`);
- connectivity nodes / duplicate detection (`connectivity.rs::scene_key_set`,
  which anchors an authored-id occurrence at `id:` instead of `character:`);
- `prereqEdges[].node` lowering and `project.index.json`'s `document_key`;
- `lute play`'s runtime visited-set insertion (`play.rs::scene_canonical_key`
  reads the new `meta.id` artifact field directly; the legacy triad
  reconstruction remains only as the fallback for pre-0.15 artifacts).

`schedule.yaml` is unaffected: it addresses documents by project-relative
path, never by canonical key (survey §8).

When absent, every one of those sites resolves the derived
`{character}.{episodeId}` exactly as 0.14.0 — the fallback is the current
code path unchanged, so untouched projects compile byte-identically (gate:
anseo corpus).

An authored `id:` alongside an authored `episodeId:` leaves the latter inert
for identity; it draws `W-META-LEGACY` like the rest of the triad.

## 2. `extra:` — free descriptive block

Open mapping under one reserved key. Values: scalars (string / int / bool /
float) or flat lists of scalars; a nested mapping or non-scalar list entry is
`E-META-VALUE` (error, anchored at the offending key). Keys are free — this
is the sanctioned home for team search metadata (`character`, `season`,
`arc`, `location`, whatever), which is why the top level can stay closed
(`E-META-UNKNOWN-KEY` keeps defending against typos).

Semantics: none. Not readable from CEL, not a state path, not a template
token, never consulted by any checker/compiler/runtime rule. Carried
verbatim into the artifact meta (`meta.extra`, omitted when empty) so external
tooling (jq, TMS, editors) can search it.

Legal on Scene and Quest roots (both artifact meta kinds carry it); Schema
and Component documents reject it via the existing kind gate.

`defaults:` may supply `extra:` (added to `DEFAULTABLE_KEYS`); §6.2's
whole-value-per-key rule applies — a document authoring any `extra:` replaces
the default entirely.

## 3. Deprecation — quiet legacy, loud coexistence

- Document authors `id:` AND any of `character:` / `season:` / `episode:` /
  `episodeId:` **in its own frontmatter** → one `W-META-LEGACY` warning per
  key, anchored at that key (`meta_key_span`), message: identity now comes
  from `id:`; move the value under `extra:` if it should stay searchable.
  Promotable via `--deny W-META-LEGACY`.
- The check consults the **authored** map, never the defaults-merged one: a
  manifest-inherited `character:` on an `id:`-carrying document does not warn
  (the document author wrote nothing to move). This matters because required
  and unknown-key enforcement run over the merged map (survey §2) — the
  deprecation pass must not.
- `id:` absent → no warning anywhere; the legacy path is fully supported and
  documented as deprecated in prose only.
- `pov:` and `after:` carry semantics and are untouched.

## 4. Required-key rule

`MetaKind::Scene` only, after defaults merge (unchanged position):

- merged frontmatter has `id:` → no required keys;
- otherwise → `character`/`season`/`episode` required, one `E-META-MISSING`
  per absent key, exactly today's behavior.

## 5. IR / schema / conformance

- `SceneMeta`: new required `id: string` (the resolved canonical key —
  authored or derived — so `index.rs` and `play.rs` stop rederiving it);
  `character: string?`, `season: i64?`, `episode: i64?`, `episodeId: string?`
  now optional, emitted when the source supplies them (a legacy document's
  artifact keeps all four plus the new `id`, so the only delta on untouched
  corpora is the added `id` field).
- `SceneMeta` + `QuestMeta`: new optional `extra` object (the §2 block),
  omitted when empty.
- `LUTE_LANG_VERSION` / `LUTE_IR_VERSION` → `0.15.0`;
  `schemas/lute-ir-0.15.schema.json` (sceneMeta: `id` required, legacy four
  optional, `extra` free-object; questMeta: `extra`); `conformance/README.md`
  schema link; all seven conformance fixtures restamped, five scene fixtures
  re-accepted for the added `meta.id`.

## 6. Diagnostics delta

| Code | Grade | New/changed |
| --- | --- | --- |
| `E-META-ID` | error | new — malformed `id:` value |
| `E-META-VALUE` | error | new — non-scalar(-list) value under `extra:` |
| `W-META-LEGACY` | warning | new — authored legacy identity key coexisting with authored `id:` |
| `E-META-MISSING` | error | condition narrowed: only fires when `id:` absent |
| `E-CONN-EPISODE-ID-DUP` | error | kept; message says "canonical scene id"; authored-id occurrences anchor at `id:` |
| `E-DEFAULTS-KEY` | error | unchanged; `id` stays outside the closed set, `meta` joins it |

## Out of scope

- Renaming `E-CONN-EPISODE-ID-DUP` (code churn breaks downstream tooling for
  a name).
- Any search/query command over `extra:` (external tooling's job).
- Removing the legacy derived path (a later major).
- LSP frontmatter-key completion/hover (none exists today; diagnostics flow
  through shared `lute-check::meta` automatically).
- Grammar/tree-sitter changes (frontmatter is one opaque external token).

## Consumers audit (why this is safe)

Survey (2026-09-01) found **no production consumer of frontmatter
`character`/`season`/`episode`/`pov` semantics beyond identity derivation and
descriptive serialization** — no cast, lint, POV, routing, or staging rule
reads them. Staging's `character=` attribute, LSP provider catalogs, and the
plugin `cast` surface are distinct concepts. Demoting the triad therefore
changes identity plumbing only, at the sites enumerated in §1/§5.
