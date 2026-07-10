# Lute Data-Catalog Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Unify Lute's scattered closed-vocabulary mechanisms into one data-catalog primitive that attribute value domains reference by name; move declaration files to plain `.yaml` with two-layer lint; swap the speaker sigil `:`→`@`; and reshape `delivery` into terse bare flags.

**Architecture:** Four independent-ish subsystems, sequenced. **A) Domain type system** — one `Type::Domain(name)` reference resolved against a merged plugin∪project vocabulary. **B) `.yaml` declaration format** — body-less declarations become `.yaml`, linted structurally by a shipped JSON Schema and semantically by the Lute checker/LSP claiming those files. **C) `@` speaker sigil** — grammar + parser + editors + codemod. **D) `delivery` bare flags** — `{mono|os|vo}`, at-most-one, `os` skips sprite. Each group is its own reviewable sub-effort; a splitter MAY execute A/B/C/D as four separate plans.

**Tech Stack:** Rust workspace crates, tree-sitter (`grammar.js`), TextMate JSON, JSON Schema, Bun (JS tests), YAML.

## Global Constraints

- **BREAKING** (pre-1.0 allowance, dsl 0.1.0 §2). Every syntactic break ships a `lute fix` codemod rule.
- **Depends on 0.2.1** (`docs/superpowers/plans/2026-07-10-lute-0.2.1-editor-hygiene.md`) landing first: this plan EXTENDS 0.2.1's content-line attribute schema (adds domain refs), RESHAPES 0.2.1's `delivery="…"` enforcement into bare flags, and RETARGETS 0.2.1's modern `:speaker:` TextMate rule to `@speaker:`.
- Design of record: `docs/superpowers/specs/2026-07-10-lute-data-catalog-foundation-design.md`.
- **Version: `0.2.2`** (breaking, pre-1.0 allowance; does not affect any task's code). Sequencing (c): this foundation ships before/under the relational-facts `0.3.0`, which builds on it. Decision 2026-07-10 — `0.2.2` keeps `0.3.0` = relational facts; the `feat/lute-0.3.0` branch is NOT renumbered.
- The fixed-core enums (`delivery` role members, staging `show|hide`, `musicAction`) stay in `lute.core` and are NOT author-extensible. Everything else is data vocabulary.
- Run only each task's tests + that crate's suite; no whole-workspace runs or formatters per task.

## File Structure

| File | Responsibility | Group |
|---|---|---|
| `crates/lute-manifest/src/types.rs` | `Type::Domain(String)` variant + `TypeDef` mirror + `type_accepts` arm | A |
| `crates/lute-manifest/src/snapshot.rs` | merged `domains` (enums ∪ entities) field on `CapabilitySnapshot` | A |
| `crates/lute-manifest/src/assemble.rs` | merge plugin+project domains; `E-DOMAIN-DUP` | A |
| `crates/lute-manifest/src/entities.rs` (new) | `entities:`/`enums:` project declaration parse | A |
| `crates/lute-check/src/directives.rs` | validate `{domain:}` attr values against merged domains; `E-DOMAIN-UNKNOWN` | A |
| `crates/lute-check/src/content_line.rs` | content-line `emotion`/`action` → `{domain:}`; delivery bare flags (Group D) | A, D |
| `schemas/*.schema.json` (new) | JSON Schema per declaration kind | B |
| `crates/lute-check/src/schema_import.rs` | import `.yaml` declaration targets | B |
| `crates/lute-lsp/src/backend.rs` | claim project declaration `.yaml`; run semantic pipeline | B |
| `editors/vscode/package.json` | `contributes.configurationDefaults` → `yaml.schemas` | B |
| `tree-sitter-lute/grammar.js` | `line` rule `:`→`@` | C |
| `crates/lute-syntax/src/parser.rs` | line classification `@` | C |
| `crates/lute-lsp/src/features/mod.rs` + TextMate `#line` + nvim | `@` in resolvers/highlighters | C |
| `crates/lute-check/src/fix.rs` | codemod: `:x{…}:`→`@x{…}:`, `delivery="…"`→`{flag}` | C, D |
| `crates/lute-compile/src/lower.rs` | role from delivery flags; `os`/`vo` skip sprite | D |

---

## GROUP A — Domain type system (spec D1/D2/D3)

### Task A1: `Type::Domain(name)` reference type

**Files:**
- Modify: `crates/lute-manifest/src/types.rs` (`Type` enum L11-23, `type_accepts` L98-128, `TypeDef` L156-176)
- Test: `crates/lute-manifest/src/types.rs` `mod tests` (L272+)

**Interfaces:**
- Produces: `Type::Domain(String)` — a named reference into the merged vocabulary; serde form `{ domain: <name> }` (camelCase `TypeDef::Domain`). Structurally accepts a string literal (membership is checked at check-stage with the snapshot, mirroring `ProviderRef`/`AssetKind`).

- [ ] **Step 1: Failing test** — round-trip + structural accept:

```rust
#[test]
fn domain_type_roundtrips_and_accepts_string() {
    let ty: Type = serde_yaml::from_str("{ domain: emotion }").unwrap();
    assert_eq!(ty, Type::Domain("emotion".into()));
    assert_eq!(serde_yaml::to_string(&ty).unwrap().trim(), "domain: emotion");
    assert!(type_accepts(&ty, &Literal::Str("neutral".into())));   // structural: any string
    assert!(!type_accepts(&ty, &Literal::Bool(true)));
}
```

- [ ] **Step 2: Run to verify it fails** — `cargo test -p lute-manifest domain_type_roundtrips` → FAIL (no `Domain` variant).
- [ ] **Step 3: Implement** — add `Domain(String)` to `Type` (L23) and `TypeDef` (L176); add `From` arms in both `impl From<TypeDef> for Type` and `impl From<&Type> for TypeDef`; add to `type_accepts` (L125-style): `(Type::Domain(_), Literal::Str(_)) => true,`.
- [ ] **Step 4: Run to verify it passes** — `cargo test -p lute-manifest domain_type_roundtrips` → PASS.
- [ ] **Step 5: Commit** — `git commit -am "feat(manifest): Type::Domain named-vocabulary reference (foundation A1)"`

### Task A2: Merged `domains` on the capability snapshot + `E-DOMAIN-DUP`

**Files:**
- Modify: `crates/lute-manifest/src/snapshot.rs` (`CapabilitySnapshot` — add `pub domains: BTreeMap<String, Domain>` where `Domain` = enum-style member list or registry ref), `crates/lute-manifest/src/assemble.rs` (merge; dup detection)
- Test: `crates/lute-manifest/src/snapshot.rs`/`assemble.rs` tests

**Interfaces:**
- Produces: `CapabilitySnapshot.domains` — plugin `enums` (existing `.enums` field) folded in as enum-style domains, keyed by name; a duplicate name across unrelated plugin peers → `E-DOMAIN-DUP` (reuse the existing cross-plugin dup machinery in `assemble.rs`, which already reserves names and reports first-owner). Registry-style domains (providers) resolve via the existing `ProviderSet`, referenced by the same name.

- [ ] **Step 1: Failing test** — two plugins declaring the same enum name → `E-DOMAIN-DUP`; distinct names merge:

```rust
// assemble.rs tests (mirror the existing cross-plugin dup test)
let snap = assemble(&[plugin_with_enum("a", "mood", &["calm"]), plugin_with_enum("b", "mood", &["tense"])]);
assert!(snap.errors.iter().any(|e| e.code == "E-DOMAIN-DUP"));
```

- [ ] **Step 2: Run to verify it fails.**
- [ ] **Step 3: Implement** — fold each active plugin's `enums` into `snapshot.domains` in `assemble.rs` (right after the existing enum merge), routing a name collision through the same drop-and-report path `RESERVED_DIRECTIVE_NAMES` uses, with code `E-DOMAIN-DUP`.
- [ ] **Step 4: Run to verify it passes.**
- [ ] **Step 5: Commit** — `"feat(manifest): merged domain vocabulary + E-DOMAIN-DUP (foundation A2)"`

### Task A3: Project-authored `enums:`/`entities:` declarations

**Files:**
- Create: `crates/lute-manifest/src/entities.rs` (parse `enums:`/`entities:` YAML into `Domain`s)
- Modify: `crates/lute-check/src/schema_import.rs` (lift a schema doc's `enums:`/`entities:` into the merged vocabulary alongside `state:`/`defs:`)
- Test: `crates/lute-check/tests/domains.rs` (new)

**Interfaces:**
- Consumes: schema-doc frontmatter `enums: { <name>: [<member>…] }` and `entities: { <kind>: { members: [<id>…] } | { open: engine } }`.
- Produces: those domains merged (union) with plugin domains; same dup rules (`E-DOMAIN-DUP`), `extends` may only ADD members (`E-EXTENDS-*`).

- [ ] **Step 1: Failing test** — a project schema `enums: { action: [wave, bow] }` is visible to the merged vocabulary; a member/non-member of an attr typed `{domain: action}` validates/errors (ties to A5).
- [ ] **Step 2–5:** implement parse + merge; TDD as above; commit `"feat(manifest/check): project enums/entities feed the merged vocabulary (foundation A3)"`.

### Task A4: Checker validates `{domain:}` values against the merged vocabulary

**Files:**
- Modify: `crates/lute-check/src/directives.rs` (`check_attr_value` — add a `Type::Domain(name)` arm)
- Test: `crates/lute-check/tests/domains.rs`

**Interfaces:**
- Consumes: `snapshot.domains` (A2) + provider registries. An attr typed `{domain: X}` where X ∉ merged vocabulary → `E-DOMAIN-UNKNOWN`; a value not in domain X's members → `E-BAD-ENUM` (enum-style) or `E-UNKNOWN-ID`/catalog-stale (registry-style).

- [ ] **Step 1: Failing test:**

```rust
// domains.rs
#[test]
fn unknown_domain_ref_errors() {
    // a directive attr typed { domain: nope } -> E-DOMAIN-UNKNOWN
    assert!(codes_with_plugin_attr("kind", "{ domain: nope }", "x").contains(&"E-DOMAIN-UNKNOWN".into()));
}
#[test]
fn domain_member_ok_nonmember_errors() {
    assert!(!codes_with_plugin_attr("mood", "{ domain: mood }", "calm").iter().any(|c| c == "E-BAD-ENUM"));
    assert!(codes_with_plugin_attr("mood", "{ domain: mood }", "zzz").contains(&"E-BAD-ENUM".into()));
}
```

- [ ] **Step 2: Run to verify it fails.**
- [ ] **Step 3: Implement** the `Type::Domain(name)` arm in `check_attr_value`: look up `name` in `snapshot.domains` (→ enum membership via the existing `check_enum_member`) else in provider registries (→ existing providerRef check) else `E-DOMAIN-UNKNOWN`.
- [ ] **Step 4: Run to verify it passes.**
- [ ] **Step 5: Commit** — `"feat(check): validate {domain:} attr values against merged vocabulary (foundation A4)"`

### Task A5: Content-line `emotion`/`action` become domain-typed; dedupe core enums

**Files:**
- Modify: `crates/lute-check/src/content_line.rs` (from 0.2.1 Task 1 — retype `emotion`/`action` as `{domain:}`), `crates/lute-manifest/assets/lute.core/directives/staging.yaml` (replace inline `{enum:[…]}` with `{domain: musicAction}` etc.)
- Test: `crates/lute-check/tests/content_line.rs`

- [ ] **Step 1: Failing test** — `:x{emotion="neutral"}:` clean; `:x{emotion="zzz"}:` → `E-BAD-ENUM`; a project-authored `action` domain constrains `action="wave"`/errors `action="zzz"`.
- [ ] **Step 2–5:** retype content-line `emotion`→`{domain: emotion}`, `action`→`{domain: action}` (default empty/`open` until a project declares it); swap staging inline enums to `{domain: …}` referencing `lute.core/enums.yaml`; TDD; commit `"feat(check): content-line emotion/action are domain-typed; dedupe core enums (foundation A5)"`.

---

## GROUP B — `.yaml` declaration format + two-layer lint (spec D4)

### Task B1: JSON Schema per declaration kind (structural lint)

**Files:**
- Create: `schemas/lute.schema.json` (state/defs/enums/entities schema doc), `schemas/lute.plugin.json` (plugin manifest + export files)
- Test: `tree-sitter-lute/test/json_schema.test.js` (validate the shipped example `.yaml` declarations against the schema with a JSON-Schema validator via `bun`)

- [ ] **Step 1: Failing test** — validate `docs/examples/showcase/plugins/showcase.pack/enums/*.yaml` (once migrated) against `schemas/lute.plugin.json`; a hand-broken fixture fails.
- [ ] **Step 2–5:** author the JSON Schemas mirroring `crates/lute-manifest/src/schema.rs`/`types.rs`/`entities.rs`; TDD with a `bun` ajv/validator; commit `"feat(schemas): JSON Schema for declaration YAML (foundation B1)"`.

### Task B2: `uses:`/`extends:` import `.yaml` declaration targets

**Files:**
- Modify: `crates/lute-check/src/schema_import.rs` (resolve a `.yaml` import target as a body-less declaration; parse frontmatter-equivalent directly)
- Test: `crates/lute-check/tests/uses_import.rs`

- [ ] **Step 1: Failing test** — a scene `uses: schema/game.yaml` resolves + merges state/defs/domains identically to the old `.schema.lute`.
- [ ] **Step 2–5:** teach the import resolver to load `.yaml` targets as pure declarations (no `---` envelope, no body); TDD; commit `"feat(check): uses/extends import .yaml declaration targets (foundation B2)"`.

### Task B3: LSP claims project declaration `.yaml` for semantic lint

**Files:**
- Modify: `crates/lute-lsp/src/backend.rs` (register `yaml` documents under the project's `schema/`/`catalog/` dirs or reachable via `uses:`/`extends:`; run the Lute semantic pipeline; report diagnostics on the file)
- Test: `crates/lute-lsp/tests/` (a declaration `.yaml` with a bad CEL in `defs` → diagnostic ON that file)

- [ ] **Step 1: Failing test** — open `schema/game.yaml` with `defs: { x: cel run.nope }` → an undeclared-path diagnostic on that file (today: nothing, `.yaml` unclaimed).
- [ ] **Step 2–5:** implement the claim + pipeline dispatch; TDD; commit `"feat(lsp): claim declaration .yaml; semantic diagnostics on the file (foundation B3)"`.

### Task B4: Migrate `.schema.lute` → `.yaml` (+ examples) and VS Code `yaml.schemas`

**Files:**
- Rename: `docs/examples/**/*.schema.lute` → `*.yaml` (and `*.component.lute` stay `.lute` — they have a body); update `uses:`/`extends:` targets
- Modify: `editors/vscode/package.json` (`contributes.configurationDefaults` → `yaml.schemas` mapping the JSON Schemas to `**/*.schema.yaml`/plugin dirs)
- Test: `crates/lute-cli/tests/examples_check.rs` (examples still check clean after migration)

- [ ] **Step 1: Failing test** — the migrated showcase still checks clean (`--project docs/examples/showcase`).
- [ ] **Step 2–5:** codemod the renames + import-target rewrites (extend `lute fix`); wire `yaml.schemas`; TDD; commit `"refactor(examples): declaration files .schema.lute -> .yaml; wire yaml.schemas (foundation B4)"`.

---

## GROUP C — `@` speaker sigil (spec D5)

### Task C1: Grammar + parser `:`→`@`

**Files:**
- Modify: `tree-sitter-lute/grammar.js` (`line` rule L91-98: first `":"` → `"@"`), `crates/lute-syntax/src/parser.rs` (line classification §4.3: `@` ident ⇒ content line), `tree-sitter-lute/test/corpus/*` (update speaker cases)
- Test: `tree-sitter-lute` corpus + `crates/lute-syntax` parser tests

- [ ] **Step 1: Failing test** — corpus: `@bianca{code="0010"}: hi` parses to a `line` with speaker `bianca`; `:bianca:` no longer parses as a line.
- [ ] **Step 2: Run** — `cd tree-sitter-lute && npx tree-sitter generate && npx tree-sitter test` → FAIL until the rule changes.
- [ ] **Step 3: Implement** — change `line` rule first token `":"`→`"@"` (grammar.js L93); regenerate; update `crates/lute-syntax/src/parser.rs` §4.3 classifier (the `:`-content branch → `@`-content); update corpus fixtures.
- [ ] **Step 4: Run to verify it passes** — corpus + `cargo test -p lute-syntax`.
- [ ] **Step 5: Commit** — `"feat(syntax)!: speaker sigil : -> @ (foundation C1)"`

### Task C2: LSP + editors recognize `@` speaker

**Files:**
- Modify: `crates/lute-lsp/src/features/mod.rs` (`resolve_line`/`Cursor::Speaker` span for `@`), `editors/vscode/syntaxes/lute.tmLanguage.json` (`#line` begin `^[ \t]*(:)` → `(@)`), `editors/nvim/queries/lute/*.scm` (via the drift guard)
- Test: `editors/vscode/test/tmgrammar.test.js` (extend), `crates/lute-lsp` completion tests

- [ ] **Step 1: Failing test** — TextMate `#line` matches `@narrator:`; LSP `Cursor::Speaker` resolves under `@`.
- [ ] **Step 2–5:** retarget the 0.2.1 `#line` rule + LSP speaker resolver to `@`; TDD; commit `"feat(lsp,vscode)!: @ speaker in resolvers/highlighter (foundation C2)"`.

### Task C3: `lute fix` codemod `:x{…}:` → `@x{…}:`

**Files:**
- Modify: `crates/lute-check/src/fix.rs` (add a phase producing `(start,end,replacement)` edits that replace a content line's leading `:` with `@`; splice back-to-front, mirroring the existing discipline)
- Test: `crates/lute-check/src/fix.rs` `mod tests`

- [ ] **Step 1: Failing test:**

```rust
#[test]
fn migrates_speaker_colon_to_at() {
    let out = fix_document("## Shot 1.\n:bianca{code=\"0010\"}: hi\n:narrator: x\n");
    assert!(out.text.contains("@bianca{code=\"0010\"}: hi"));
    assert!(out.text.contains("@narrator: x"));
    // idempotent
    assert_eq!(fix_document(&out.text).text, out.text);
}
```

Note: since 0.2.x still PARSES `:speaker:`, phase 2 can walk `Line` nodes and rewrite the leading `:` span; run the codemod on 0.2.x-shaped input BEFORE the C1 grammar break lands, or drive it off the `Line` node's `span` start.
- [ ] **Step 2–5:** implement; TDD; commit `"feat(fix): codemod : -> @ speaker lines (foundation C3)"`.

### Task C4: Migrate all examples/docs to `@`

**Files:** `docs/examples/**/*.lute`, `docs/**/*.md` inline samples
- [ ] Run `lute fix` over every example; verify `cargo test -p lute-cli` (examples check clean); commit `"refactor(examples)!: @ speaker sigil across examples/docs (foundation C4)"`.

---

## GROUP D — `delivery` bare flags (spec D7)

### Task D1: Content-line delivery flags `{mono|os|vo}` (checker)

**Files:**
- Modify: `crates/lute-check/src/content_line.rs` (replace the 0.2.1 `delivery="…"` enum check with bare-flag recognition + at-most-one)
- Test: `crates/lute-check/tests/content_line.rs`

**Interfaces:** delivery flags are bare-Ident attrs (`AttrValue::BoolTrue`): `mono`, `os`, `vo`. At most one (`E-DELIVERY-CONFLICT`); on `narrator` → `E-DELIVERY-NARRATOR`; an unknown bare flag → `E-UNKNOWN-ATTR` (retires 0.2.1's `E-DELIVERY-VALUE`). Add `mono`/`os`/`vo` to `content_line.rs::KNOWN_ATTRS`.

- [ ] **Step 1: Failing test:**

```rust
#[test]
fn two_delivery_flags_conflict() {
    let cs = codes(&format!("{HDR}@x{{mono os}}: hi\n"));
    assert!(cs.contains(&"E-DELIVERY-CONFLICT".to_string()), "{cs:?}");
}
#[test]
fn single_delivery_flag_ok() {
    for f in ["mono", "os", "vo"] {
        assert!(!codes(&format!("{HDR}@x{{{f}}}: hi\n")).iter().any(|c| c.starts_with("E-DELIVERY")));
    }
}
#[test]
fn delivery_flag_on_narrator_errors() {
    assert!(codes(&format!("{HDR}@narrator{{mono}}: hi\n")).contains(&"E-DELIVERY-NARRATOR".to_string()));
}
```

(Uses `@` speaker — this task lands after Group C. Before C, use `:x{mono}:`.)
- [ ] **Step 2: Run to verify it fails.**
- [ ] **Step 3: Implement** — recognize the three bare flags in `content_line.rs`; count set flags → `E-DELIVERY-CONFLICT` if >1; narrator → `E-DELIVERY-NARRATOR`; drop the `delivery="…"` enum arm and the `E-DELIVERY-VALUE` code.
- [ ] **Step 4: Run to verify it passes.**
- [ ] **Step 5: Commit** — `"feat(check)!: delivery bare flags mono/os/vo + at-most-one (foundation D1)"`

### Task D2: Compiler role from flags; `os`/`vo` skip sprite

**Files:**
- Modify: `crates/lute-compile/src/lower.rs` (`lower_line` L16-42 role derivation reads the flags instead of `get("delivery")`; add an `offscreen` role/flag; `os`/`vo` set "skip sprite")
- Test: `crates/lute-compile/tests/`

**Interfaces:** `Role::{Dialogue, Monologue, Offscreen, Voiceover, Narration}`; the emitted `LineCmd` gains a `sprite: bool`/`skip_sprite` signal that `os`/`vo` clear (char-cast §7.1 errata — sprite resolution skipped).

- [ ] **Step 1: Failing test** — `@x{mono}:` → `Role::Monologue`; `@x{os}:` → `Role::Offscreen` + no sprite; `@x{vo}:` → `Role::Voiceover` + no sprite; bare `@x:` → `Role::Dialogue` + sprite.
- [ ] **Step 2–5:** map flags → role in `lower.rs`; add `Offscreen` to `Role`; thread the skip-sprite signal; TDD; commit `"feat(compile)!: role from delivery flags; os/vo skip sprite (foundation D2)"`.

### Task D3: `lute fix` codemod `delivery="…"` → `{flag}`

**Files:**
- Modify: `crates/lute-check/src/fix.rs`
- Test: `crates/lute-check/src/fix.rs` `mod tests`

- [ ] **Step 1: Failing test:**

```rust
#[test]
fn migrates_delivery_attr_to_flag() {
    let out = fix_document("## Shot 1.\n:x{delivery=\"thought\"}: a\n:y{delivery=\"voiceover\"}: b\n");
    assert!(out.text.contains("{mono}") && out.text.contains("{vo}"));
    assert!(!out.text.contains("delivery="));
}
```

- [ ] **Step 2–5:** add the attr→flag rewrite (`thought`→`mono`, `voiceover`→`vo`, `spoken`→removed) to `fix.rs`; run in the same pass as C3; TDD; commit `"feat(fix): codemod delivery=... -> bare flag (foundation D3)"`.

---

## Notes for the executor

- **Sequence:** Group A → B → C → D. A and B are non-syntactic (safe anytime after 0.2.1). C (grammar break) and D (delivery reshape) are the breaking syntax changes; land the `lute fix` rules (C3, D3) BEFORE migrating examples (C4, B4) so the migration is mechanical. D1's tests use `@` speakers — order D after C, or use `:` in D's pre-C tests.
- **`type_accepts` cannot resolve domains** (it has no snapshot) — like `ProviderRef`/`AssetKind` it returns structural-accept (`Str`), and real membership is checked in `crates/lute-check/src/directives.rs` against `snapshot.domains` + `ProviderSet` (Task A4). Do not try to validate membership inside `type_accepts`.
- **`entities { open: engine }`** is the registry-style domain — resolve it like a `providerRef` (snapshot-backed, stale ≠ unknown), not a closed enum.
- **`lute fix` discipline** (`crates/lute-check/src/fix.rs`): collect `(byte_start, byte_end, replacement)` from ORIGINAL-source spans, splice descending by `byte_start`; comment-blanking is length-preserving, so offsets map 1:1. New rules (C3, D3) follow this exactly.
- Confirm `CapabilitySnapshot` field access + the `assemble.rs` dup-report path against `crates/lute-manifest/src/{snapshot,assemble}.rs` before A2; confirm `Role` enum variants in `crates/lute-compile/src/ir.rs` before D2.
- This plan is large (4 subsystems); a splitter MAY execute A/B/C/D as four independent plans, each producing working software (A: domain refs validate; B: `.yaml` declarations lint; C: `@` speakers parse; D: delivery flags check).
