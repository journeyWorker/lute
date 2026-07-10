# Lute — Data-Catalog Foundation: Closed Vocabularies as Data (approved design)

- **Date:** 2026-07-10
- **Status:** approved design; spec-first (documents/decisions before implementation)
- **Version:** **0.2.2** — a **breaking** foundation minor (pre-1.0 allowance, dsl 0.1.0 §2). Ships
  the closed-vocabulary catalog **before/under** the relational-facts `0.3.0`, which layers relations
  on it. (Decision 2026-07-10: `0.2.2` keeps `0.3.0` = relational facts; the existing
  `feat/lute-0.3.0` branch is NOT renumbered.)
- **Drives:** a new `scenario-dsl` extension proposal (attribute value domains), errata to
  `plugin-system/0.0.1.md` (`{ domain: … }` Type + declaration-file format), `crates/lute-manifest`
  (`Type` + merged vocabulary), `crates/lute-check` (attr-domain + content-line validation, declared-
  YAML claim), `crates/lute-syntax` + `tree-sitter-lute/grammar.js` (`@` speaker), editors, a `lute fix`
  codemod, shipped JSON Schemas, `crates/lute-lsp` (claim declaration YAML).
- **Provenance:** this session's diagnosis + design dialogue (reported problems 6–10 + naming/format
  interjections). **Depends on 0.2.1** (`2026-07-10-lute-0.2.1-editor-hygiene-design.md`) landing first
  — it establishes the content-line attribute schema/validation path and role-based example naming
  this foundation extends.

## 0. Scope, sequencing & relationship to 0.3.0 (normative)

- **The unifying insight:** Lute already has a "data catalog" primitive — *a declared, closed
  vocabulary the checker validates against*. It exists in three scattered forms: plugin
  `enums/*.yaml` + `providers/*.yaml` (capability catalog), the 0.3.0 draft's `entities:`/`enums:`
  (state/fact catalog), and inline scalar `type: enum`. This foundation **unifies them into one
  primitive with one merged namespace**, and lets **attribute value domains** (`emotion`, `action`,
  …) reference it — closing the "free-text attribute" problems (6–10).
- **Relationship to 0.3.0 (why (c)).** 0.3.0's `entities:` (closed member lists) and `enums:`
  (named value lists) ARE this same closed-vocabulary primitive; 0.3.0 then adds `relations:`/
  `rules:`/facts on top. So the catalog is the **foundation 0.3.0 depends on**: build it first, and
  0.3.0 becomes "relations + Datalog derivation over the catalog," not a parallel catalog. This spec
  does **not** include relations/rules/facts/temporal — those stay 0.3.0.
- **Breaking (pre-1.0 allowance, 0.1.0 §2):** the `@` speaker sigil (D5) and the `.schema.lute` →
  `.yaml` declaration-file format (D4). Both are codemod-migrated.

## D1. One closed-vocabulary primitive (normative)

A **domain** is a named, declared, closed vocabulary the checker validates against. Exactly two
shapes, distinguished by cardinality/source (NOT by owner):

- **enum-style** — a finite, author-enumerable literal member list (`emotion`, `action`,
  `trustLevel`). Statically closed at load. (= 0.3.0 `entities { members: […] }` / `enums: […]`.)
- **registry-style** — a snapshot-backed id set, possibly large or **engine-minted/open**
  (`character`, `castId`), validated against a pinned snapshot with the *stale ≠ unknown-id*
  discipline (plugin §10). (= 0.3.0 `entities { open: engine }` / today's `providers` + `catalog`.)

The one line that is a real language distinction — **fixed-core vs data vocabulary**:

- **Fixed core enums** — the compiler branches on the specific member, so the members ARE language
  semantics: `delivery` (`spoken|thought|voiceover` → role), staging `show|hide`, `musicAction`.
  These live in `lute.core`, are closed, and are **NOT author-extensible**.
- **Data vocabularies** — the checker only membership-checks; the value flows through as data
  (`emotion`, `action`, `character`, `costume`, `mood`, `vfxType`). These are **author/plugin
  definable**.

(Litmus: does the compiler/engine change behavior by *which member* it is? Yes → fixed core; no →
data vocabulary.)

## D2. Two authoring homes, one merged namespace (normative)

The same domain primitive is authored in either home; **ownership is a packaging choice, not a
semantic one**:

- **Plugin** (`plugins/<id>/enums/*.yaml`, `providers/*.yaml`) — engine/vendor-shipped, versioned,
  profile-activated. (Exists today; `lute.core` ships the fixed-core + baseline domains.)
- **Project declaration** (a `.yaml` declaration file, formerly `.schema.lute`) — author-shipped
  `enums:`/`entities:` (the 0.3.0 surface), composed via `uses:`/`extends:`.

**Merge (reuse 0.3.0 §4.1 / plugin §6.10 rules):** plugin ∪ project domains union into ONE
namespace. A domain name declared by two unrelated peers is an error (`E-USES-DUP-*`); a plugin/
project name clash is an error, never a silent shadow. `extends` may only **ADD** members to an
inherited domain, never remove/retype. This is the closure that keeps the merged vocabulary finite
and statically checkable.

## D3. Attribute value types reference a domain by name (normative)

Extend the manifest `Type` (`crates/lute-manifest/src/types.rs`) and the content-line attribute
schema with a **named-domain reference** — **proposed syntax `{ domain: <name> }`** (confirm at
review) — resolved against the merged vocabulary (D2): an enum-style name → membership check; a
registry-style name → snapshot check.

- This subsumes/dedupes today's redundancy: `lute.core/enums.yaml` declares `musicAction`, yet
  `staging.yaml` re-inlines `{ enum: [start,change,stop,resume,fade-out] }` for `::music action`.
  With `{ domain: musicAction }` the attr references the one declaration.
- Inline `{ enum: [...] }` remains as **sugar** for a one-off local domain; `{ providerRef: … }` /
  `{ assetKind: … }` are named registry/asset references and remain (a `{ domain: … }` naming a
  registry resolves identically — `providerRef` becomes a specialization).
- **Apply to content-line attributes** (the point-6/7/8 fix): `emotion → { domain: emotion }`
  (per-character/costume in the cast catalog, or a project/plugin `emotion` domain), `action →
  { domain: action }` (a project/plugin-authored action catalog). `delivery` stays a **fixed core
  enum** (enforced already in 0.2.1). Result: `emotion`/`action`/cast values become **validated +
  completed**, authored as data, no free text — via the content-line attr-schema path 0.2.1 wired.
- **Authoring, not execution:** a domain constrains *what an author may write*; how a value renders
  stays the engine's job (the Lute contract). `{ domain: action }` says "these are the legal
  actions," not "here is how fade-in-up animates."

## D4. Declaration files become `.yaml` + two-layer lint (normative)

Pure **declaration** files (schema/catalog/plugin-vocab — body-less, all-frontmatter) become plain
**`.yaml`**. `.lute` is reserved for documents with a **body** (scene, component). This collapses
the `.schema.lute`-vs-plugin-`.yaml` inconsistency into one data-declaration format.

- **Structural lint (any editor):** ship a **JSON Schema** per declaration kind (state/defs/enums/
  entities/relations, plugin manifest/exports); wire `redhat.vscode-yaml` (`yaml.schemas` +
  `$schema` marker) so any editor gets shape autocomplete + structural errors — including for plugin
  YAML, which today gets zero editor help.
- **Semantic lint (Lute-aware, on the file itself):** the Lute checker/LSP **claims** declaration
  `.yaml` (by project `schema/`/`catalog/` registration + `uses:`/`extends:` reachability; the LSP
  already discovers `lute.project.yaml`, `backend.rs:513`) and runs the Lute pipeline on it — CEL
  type-check in `defs`, path/domain/relation checks, cross-file merge — reporting diagnostics **on
  that YAML file**, not deferred onto a referencing scene.
- **Why generic YAML lint is insufficient:** declaration values embed Lute — `defs: { x: cel … }`,
  0.3.0 `rules: ["… :- …"]`. JSON Schema checks structure ("`args` is a string list"); only the Lute
  checker checks meaning ("this arg names a declared kind / this CEL type-checks / this rule is
  stratified").
- **Prior art (sound, cited):** Helm `values.yaml` + `values.schema.json`; Kubernetes CRD OpenAPI +
  `x-kubernetes-validations` (**CEL**) — the identical "YAML data + JSON Schema (structure) + CEL
  (semantics)" split. Lute-CEL is the same CEL lineage; the only divergence is enforcement point
  (local checker/LSP + pinned snapshot, not a central admission server) — a reproducible-build plus.
- `uses:`/`extends:` now import `.yaml` targets (parsed as body-less frontmatter). Migration codemod:
  rename `*.schema.lute` → `*.yaml` (already role-renamed in 0.2.1 D6b), drop the `---` envelope,
  rewrite import targets.

## D5. Speaker sigil `:` → `@` (normative, breaking)

`:speaker:` / `:speaker{attrs}:` → **`@speaker:`** / `@speaker{attrs}:`.

- **Why:** the colon pair `:word:` is the ubiquitous emoji-shortcode convention (editors/terminals
  substitute it); the bare `:narrator:`/`:bianca:` forms collide. Moving off `:` fixes it and
  **reserves `:emoji:` for possible future in-dialogue emoji shortcodes**.
- **Sigil allocation (locked):** speaker `@`, def `@name` (CEL-internal, unchanged), match subject
  `$` (unchanged). The `@` speaker/def overload is **context-disjoint** — speaker is a body
  line-start statement, def is inside a CEL expression — so it is unambiguous to parser and reader.
  `def` keeps `@` because `@` is the one sigil clean inside CEL (CEL has no `@`; `%`/`&`/`\` collide
  with modulo/`&&`/string-escape).
- **Touches:** `crates/lute-syntax` + `tree-sitter-lute/grammar.js` `line` rule, `tree-sitter-lute/
  queries/*.scm`, VS Code TextMate `#line` (the modern rule added in 0.2.1 D1b retargeted `:`→`@`),
  nvim queries, LSP `Cursor::Speaker`, all `docs/examples/**`, docs. **Migration:** a `lute fix`
  codemod rule (`:x{…}: ` → `@x{…}: `), one pass (Lute already ships `lute fix`).

## D6. Diagnostics (foundation delta)

Attribute-domain validation reuses existing codes where possible (`E-BAD-ENUM`, `E-UNKNOWN-ID`
[registry/stale], `E-UNKNOWN-ATTR`, `E-ATTR-TYPE`). New:

| Code | Stage | Meaning |
|---|---|---|
| `E-DOMAIN-UNKNOWN` | check/assembly | a `{ domain: <name> }` attribute type naming a domain not in the merged vocabulary (D2). |
| `E-DOMAIN-DUP` | assembly | a domain name declared by two unrelated plugin/project peers, or a plugin↔project clash (D2). |
| `E-DELIVERY-CONFLICT` | check | more than one delivery flag on a content line — `mono`/`os`/`vo` are mutually exclusive (D7). |

Composition/format reuse: `E-USES-DUP-*`, `E-EXTENDS-*` (domain member additivity), catalog-stale
(plugin §10).

## D7. Delivery modes → terse bare flags (normative, breaking surface)

The content-line delivery mode moves from the verbose `delivery="…"` enum-valued attribute to
**terse bare flags** (Lute's existing `{ident}⇒true` boolean-attr form, §4.5 — like `::cut{full}`
and JSX boolean props). A content line carries **at most one** delivery flag; **absent = `spoken`**
(on-stage dialogue, the default):

| Flag | Meaning | Compiler |
|---|---|---|
| *(none)* | on-stage spoken dialogue | resolve sprite; role = dialogue |
| `mono` | inner monologue (the speaker's own inner voice) | role = monologue |
| `os` | **off-screen** — in scene, sprite NOT shown this line (behind a door / before entering) | **skip sprite resolution**; role = offscreen |
| `vo` | voiceover — non-diegetic narration-over | skip sprite resolution; role = voiceover |

- **Naming (proposed, confirm at review):** `mono` / `os` / `vo` (a terse family). `vo` is the
  marginal member once `os` exists — droppable if a project never needs non-diegetic narration.
- **`os` is a new capability, not just sugar.** Today a dialogue line implies a sprite (char-cast
  §7.1: the current costume applies to dialogue sprites, not only `::auto`); a sprite-bearing
  character cannot speak off-screen for one line without changing its kind or using a one-off `as=`
  label. `os` decouples speech from staging per line — **errata to character-cast §7.1**: sprite
  resolution is skipped when the line carries `os` (or `vo`).
- **Mixes with valued attrs, JSX-style, space-separated:** `@bianca{mono emotion="happy"}:` — a bare
  flag is boolean-true; valued attrs stay `key="value"`; order-free; **no commas** (§4.5 attrs are
  whitespace-separated).
- **Diagnostics transition:** an unknown flag becomes `E-UNKNOWN-ATTR` (retiring 0.2.1's
  `E-DELIVERY-VALUE`, which only existed to guard the enum-string form); a delivery flag on
  `narrator` remains `E-DELIVERY-NARRATOR`; two flags is the new `E-DELIVERY-CONFLICT` (D6) — the
  bare-flag form re-adds, as a checker rule, the exclusivity a single enum attribute gave for free.
- **Migration:** `lute fix` rewrites `delivery="thought"` → `{mono}`, `delivery="voiceover"` →
  `{vo}`, `delivery="spoken"` → (removed), in the same pass as the `:`→`@` codemod (D5).
- **Related (no change):** the equality guard shorthand the user asked about already exists —
  `<when is="abc">` = `<when test="$ == 'abc'">`, `<when is="a|b">` = `$ ∈ {a,b}` (§7.3.1), scoped to
  `<match on>` arms; `test=` stays for general CEL. This spec adds no condition-side sugar.

## Testing (per area)

- **Manifest/check:** `{ domain: emotion }` on a content-line `emotion` validates against the merged
  domain (member ok / non-member → `E-BAD-ENUM`); an unknown `{ domain: nope }` → `E-DOMAIN-UNKNOWN`;
  a project `enums:` domain merges with plugin domains; a dup name → `E-DOMAIN-DUP`; `action`
  constrained to a project-authored action catalog (member/non-member).
- **Format/lint:** a declaration `.yaml` with a structural error flags via JSON Schema (fixture); a
  semantically-bad `.yaml` (bad CEL in `defs`, undeclared path) flags **on that file** via the LSP
  claim; `uses: foo.yaml` resolves and merges.
- **Sigil:** `lute fix` migrates `:x{…}: ` → `@x{…}: ` idempotently; grammar/tree-sitter/TextMate/
  LSP recognize `@speaker:`; `@def`/`$` unaffected; a stray `:word:` in dialogue **text** is literal.
- **Examples:** every migrated example (`.yaml` declarations + `@` speakers) checks clean under its
  project.

## Non-goals (this pass)

Relational **`relations:`/`rules:`/facts/derivation/temporal validity** (that is 0.3.0, layered on
this foundation); the runtime engine/renderer; per-game action *animation* semantics (Lute declares
the action *vocabulary*, not its realization). No new control-flow or expression power (Lute-CEL
profile unchanged; `count`/`holds` fact queries remain 0.3.0).
