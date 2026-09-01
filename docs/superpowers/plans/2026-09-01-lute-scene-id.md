# Lute 0.15.0 — Authored Scene Identity Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Scene frontmatter gains `id:` (authored canonical key, derived `{character}.{episodeId}` fallback), `character`/`season`/`episode` demote to optional, a free `extra:` descriptive block ships on scene+quest roots, and legacy identity keys coexisting with `id:` draw a per-key deprecation warning.

**Architecture:** One shared resolution point — `lute_check::meta::canonical_scene_key(&TypedMeta)` — feeds every consumer (connectivity key set, compile prefix/`prereqEdges`, `project.index.json`, loc prefix). The artifact carries the resolved key as `SceneMeta.id` so `play`/`index` stop rederiving. Legacy path is the existing code unchanged; the anseo corpus gates byte-stability.

**Tech Stack:** Rust workspace (rustc stable via `~/.cargo/bin`), serde_yaml frontmatter lift, serde_json IR.

**Spec:** `docs/proposals/scenario-dsl/0.15.0.md` + design record `docs/superpowers/specs/2026-09-01-lute-scene-id-design.md`.

## Global Constraints

- `export PATH="$HOME/.cargo/bin:$PATH"` in every fresh shell.
- **Worktree authoritative:** `/Users/journey/Workspace/lute/.worktrees/lute-0.15.0` on `feat/lute-0.15.0-scene-id`. HARNESS QUIRK: `write`/`edit` relative paths resolve against the MAIN workspace — always use ABSOLUTE worktree paths.
- TDD tester-first; own-crate `cargo test -p <crate>` during a task; full gate at the end. `cargo fmt -p` + `cargo clippy -p … --all-targets -- -D warnings` before each commit.
- `id:` charset: `[A-Za-z0-9_.-]+`, non-empty (spec §2).
- `extra:` values: YAML scalars (string/int/float/bool) or flat sequences of scalars; anything else `E-META-VALUE` (spec §3).
- `W-META-LEGACY` consults the AUTHORED frontmatter map (pre-`defaults:` merge), never the merged `Cow` (spec §4, D-C).
- JSON field names are the cross-task contract: `meta.id` (string, always on scenes), `meta.extra` (object, skip-empty, scenes+quests), legacy `character`/`season`/`episode`/`episodeId` optional.
- Grammar/tree-sitter untouched — `capabilityVersion` MUST NOT change (tree-sitter tests stay green unmodified).
- Diagnostics codes exactly: `E-META-ID`, `E-META-VALUE`, `W-META-LEGACY`; `E-META-MISSING` narrowed; `E-CONN-EPISODE-ID-DUP` message generalized to "canonical scene id".

---

### Task 1: Foundation — meta lift, diagnostics, canonical key, defaults (lute-check + lute-manifest)

**Files:**
- Modify: `crates/lute-check/src/meta.rs` (SCENE_KEYS :157, required-key loop :523-533, unknown-key gate :550-556, lifts :589-600, defaults legality :165-223, canonical helpers :348-379, TypedMeta :59-75)
- Modify: `crates/lute-manifest/src/project.rs` (`DEFAULTABLE_KEYS` :126-153, defaults value canonicalization :358-480)
- Tests: `#[cfg(test)]` in both files (house style; existing helpers `parse_meta_str`, `parse_kind_str`).

**Interfaces (Produces — later tasks consume these):**
```rust
// TypedMeta gains:
pub id: Option<String>,                                  // authored `id:` (validated)
pub meta_block: BTreeMap<String, serde_json::Value>,     // `meta:` block, JSON-ready

/// The canonical scene key: authored `id:`, else `{character}.{episodeId}`
/// via canonical_episode_key. None when neither is derivable.
pub fn canonical_scene_key(meta: &TypedMeta) -> Option<String>;
```

**Behavior:**
1. `SCENE_KEYS` += `"id"`; `meta` legal on Scene AND Quest (extend the `core_key` disjunction at :550-553 with `(matches!(kind, MetaKind::Scene | MetaKind::Quest) && key == "meta")`; keep `SCENE_KEYS` scene-only).
2. Required-key loop: skip entirely when the MERGED map contains `id`.
3. Lift `typed.id = get_str(map, "id")`; validate non-empty + `[A-Za-z0-9_.-]+` else push `E-META-ID` at `meta_key_span(meta, "id")`.
4. Lift `meta:`: must be a YAML mapping with string keys; each value scalar or seq-of-scalars → convert to `serde_json::Value` (string/i64/f64/bool; seq → array). Violation: `E-META-VALUE` anchored via `meta_key_span(meta, <inner key>)` (the raw-text scan finds inner `key:` lines; fall back to `meta_key_span(meta, "meta")` when it misses), message naming the inner key and the accepted shapes. Non-mapping `meta:` value → one `E-META-VALUE` at `meta:`.
5. `W-META-LEGACY`: BEFORE the `Cow` merge (:500), snapshot `authored_has_id` and which of `character`/`season`/`episode`/`episodeId` the AUTHORED map contains; after lifts, when `authored_has_id`, push one warning per authored legacy key at `meta_key_span(meta, key)`: `` `{key}` no longer carries scene identity (superseded by `id:`); move it under `meta:` to keep it searchable (dsl 0.15.0 §4) ``. Severity `Warning`.
6. `canonical_scene_key`: authored id wins; else requires all of character+season+episode (episodeId optional) → `canonical_episode_key(...)`.
7. `lute-manifest`: `DEFAULTABLE_KEYS` += `"meta"` (keep list sorted); a `defaults.meta` must be a mapping (else `E-DEFAULTS-KEY`), stored as raw `serde_yaml::Value` like other passthrough defaults; `default_key_legal_on` sync in meta.rs: `meta` legal on Scene+Quest kinds. `id` stays OUT (spec D-D — verify a `defaults: { id: x }` fixture draws `E-DEFAULTS-KEY`).

- [ ] **Step 1: failing tests** (meta.rs `#[cfg(test)]`):
```rust
#[test]
fn authored_id_relieves_required_triad() {
    let (m, diags) = parse_meta_str("id: anseo.s01ep01\n");
    assert!(!diags.iter().any(|d| d.code == "E-META-MISSING"), "{diags:?}");
    assert_eq!(m.id.as_deref(), Some("anseo.s01ep01"));
    assert_eq!(canonical_scene_key(&m).as_deref(), Some("anseo.s01ep01"));
}
#[test]
fn no_id_keeps_required_triad() {
    let (_m, diags) = parse_meta_str("season: 1\nepisode: 2\n");
    assert!(diags.iter().any(|d| d.code == "E-META-MISSING"));
}
#[test]
fn malformed_id_is_e_meta_id() {
    let (_m, diags) = parse_meta_str("id: \"has space\"\n");
    assert!(diags.iter().any(|d| d.code == "E-META-ID"), "{diags:?}");
}
#[test]
fn meta_block_lifts_scalars_and_lists() {
    let (m, diags) = parse_meta_str("id: x\nmeta:\n  arc: main\n  season: 1\n  tags: [harbor, night]\n");
    assert!(diags.is_empty(), "{diags:?}");
    assert_eq!(m.meta_block.get("arc"), Some(&serde_json::json!("main")));
    assert_eq!(m.meta_block.get("tags"), Some(&serde_json::json!(["harbor", "night"])));
}
#[test]
fn nested_meta_value_is_e_meta_value() {
    let (_m, diags) = parse_meta_str("id: x\nmeta:\n  nested: { a: 1 }\n");
    assert!(diags.iter().any(|d| d.code == "E-META-VALUE"), "{diags:?}");
}
#[test]
fn authored_legacy_beside_id_warns_per_key() {
    let (_m, diags) = parse_meta_str("id: x\ncharacter: bianca\nseason: 1\nepisode: 1\n");
    let n = diags.iter().filter(|d| d.code == "W-META-LEGACY").count();
    assert_eq!(n, 3, "{diags:?}");
}
#[test]
fn defaults_inherited_legacy_beside_id_is_silent() {
    // parse with a defaults map supplying character/season/episode; authored map has only id
    // (use the existing defaults-aware parse entry the 0.10.0 tests use)
    // assert: no W-META-LEGACY, no E-META-MISSING
}
#[test]
fn derived_fallback_unchanged() {
    let (m, diags) = parse_meta_str("character: x\nseason: 1\nepisode: 2\n");
    assert!(diags.is_empty(), "{diags:?}");
    assert_eq!(canonical_scene_key(&m).as_deref(), Some("x.s01ep02"));
}
```
And in project.rs tests: `defaults: { meta: { arc: main } }` accepted + applied whole-value (document's own `meta:` replaces it entirely); `defaults: { id: x }` → `E-DEFAULTS-KEY`; `defaults: { meta: notAMap }` → `E-DEFAULTS-KEY`.
- [ ] **Step 2: RED** — `cargo test -p lute-check --lib meta` fails on missing symbols/behavior.
- [ ] **Step 3: implement** per Behavior 1–7.
- [ ] **Step 4: GREEN** — `cargo test -p lute-check && cargo test -p lute-manifest`.
- [ ] **Step 5: fmt + clippy + commit** — `feat(check,manifest): scene id:/meta: frontmatter — authored identity, descriptive block, legacy deprecation (dsl 0.15.0 §2-§4)`.

### Task 2: Connectivity — canonical key set on authored ids

**Files:**
- Modify: `crates/lute-check/src/connectivity.rs` (`scene_identity`/`scene_key_set` :41-123, dup messages :69-123, atoms :219-278, graph :480-530, live-asserts :1097-1123)
- Tests: `crates/lute-check/tests/connectivity.rs`.

**Interfaces:** Consumes Task 1's `canonical_scene_key`. Produces: `scene_key_set` keyed by the resolved canonical key; occurrence span anchored at `id:` when authored, else `character:` (existing).

**Behavior:** `scene_identity` resolves via `canonical_scene_key`; dup diagnostic keeps code `E-CONN-EPISODE-ID-DUP`, messages become: same-file `` duplicate canonical scene id `{key}`; a scene's `id:` (or its `{character}.{episodeId}` fallback) must be unique project-wide (dsl 0.15.0 §2) `` and the cross-file variant analogously. `visited('…')` resolution and `E-CONN-UNKNOWN-NODE` suggestions need no code change beyond the key set (verify by test).

- [ ] Failing tests: (a) two docs, one `id: harbor.night`, one `character: harbor\nepisodeId: night\nseason:1\nepisode:1` → ONE `E-CONN-EPISODE-ID-DUP` pair (authored/derived collide in one namespace), authored occurrence anchored at the `id:` line; (b) `after: "visited('harbor.night')"` on a third doc resolves clean against the authored id; (c) unknown key suggestion includes the authored id.
- [ ] RED → implement → GREEN (`cargo test -p lute-check`).
- [ ] fmt + clippy + commit — `feat(check): connectivity keys on authored scene id (dsl 0.15.0 §2)`.

### Task 3: Compile — prefix, IR meta, schema, version bump

**Files:**
- Modify: `crates/lute-compile/src/lib.rs` (prefix :185-215, `artifact_meta` :479-515, `prereq_edge_entries` :582-604, `LUTE_IR_VERSION` :42-119), `crates/lute-compile/src/ir.rs` (:193-227), `crates/lute-compile/src/index.rs` (`document_key` :294-309, hand-built SceneMeta :319-338), `crates/lute-check/src/lib.rs` (`LUTE_LANG_VERSION` :39-44)
- Create: `schemas/lute-ir-0.15.schema.json` (from 0.14: sceneMeta requires `id`, legacy four optional, adds `meta` object `additionalProperties: {scalar or scalar-array}`; questMeta adds `meta`; envelope version pins 0.15). Delete nothing.
- Tests: `crates/lute-compile/tests/compile.rs`, `tests/ir_golden.rs`, unit tests in `lib.rs`.

**Interfaces:** Consumes `canonical_scene_key`, `TypedMeta::{id, meta_block}`. Produces wire fields:
```rust
pub struct SceneMeta {
    pub id: String,                                   // always
    #[serde(skip_serializing_if = "Option::is_none")] pub character: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")] pub season: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")] pub episode: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")] pub episode_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")] pub title: Option<String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")] pub meta: BTreeMap<String, serde_json::Value>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")] pub plugin: BTreeMap<String, serde_json::Value>,
}
// QuestMeta gains the same `meta` field. Both versions -> "0.15.0".
```
Legacy documents emit `id` + all four legacy fields (episodeId stays the RESOLVED value as today); authored-id documents emit `id`, any authored legacy keys, `meta`.

- [ ] Failing tests: (a) authored-id doc compiles with `meta.id == "anseo.s01ep01"`, lineIds prefixed `anseo.s01ep01.`, `prereqEdges[].node` and index `document_key` equal to it; (b) legacy doc artifact JSON differs from a pinned 0.14-shape expectation ONLY by the `"id"` line and version strings (assert via serde_json Value diff, not string compare); (c) `meta:` block lands under `meta.meta` key-sorted; (d) version-alignment test updated to 0.15.0.
- [ ] RED → implement (update goldens/hand-built metas listed in survey §4) → GREEN (`cargo test -p lute-compile`).
- [ ] fmt + clippy + commit — `feat(compile): SceneMeta.id canonical key, optional legacy fields, meta block; IR 0.15.0`.

### Task 4: CLI — loc prefix, play key, conformance restamp

**Files:**
- Modify: `crates/lute-cli/src/loc.rs` (`scene_prefix` :316-370 — lift `id:` from raw YAML first, fallback to today's derivation), `crates/lute-cli/src/play.rs` (`scene_canonical_key` :485-501 — read `meta.id` string; fallback to the legacy triad reconstruction for pre-0.15 artifacts), `conformance/README.md` (schema link → 0.15)
- Regenerate: all 7 `conformance/*/artifact.json` via the rebuilt `lute compile` (5 scene fixtures gain `meta.id`; versions restamp) + 7 `expected.json` irVersion → `0.15`.
- Tests: `crates/lute-cli/tests/` (existing conformance harness; loc/play integration tests).

**Interfaces:** Consumes wire fields from Task 3 (`meta.id`, `meta.meta`). No new public surface.

- [ ] Failing tests: (a) loc export of an authored-id doc rows carry `lineId` prefixed by the authored id; (b) loc export of the legacy corpus byte-identical to pre-change output (pin one fixture); (c) `lute play` marks `visited` with the authored id (a schedule fixture whose `after:` names it proceeds).
- [ ] RED → implement → regenerate conformance (`cargo build -p lute-cli` then re-emit fixtures; verify `cargo test -p lute-cli` incl. conformance suite green).
- [ ] fmt + clippy + commit — `feat(cli): loc/play consume authored scene id; conformance restamp 0.15.0`.

### Task 5: Docs

**Files:**
- Modify: `docs/getting-started-first-scene.md` (scene-key section :417-435 → `id:` primary + derived fallback; artifact meta sample :246-309; worked examples/diagnostic remediation :437-590), `docs/runtime/execution-model.md` (artifact envelope: name `meta.id` as the canonical key engines/tools should join on), `docs/runtime/quest-lifecycle.md` (:203-214 node identity wording), `docs/schedule-and-play.md` (:204-208 — replace the `character: <event>-<variant>` workaround with `id: <event>-<variant>`; note season/episode now simply omitted).

- [ ] Update the four docs; every example stays runnable (`lute check` any embedded full documents by hand-spot where feasible).
- [ ] Commit — `docs: authored scene id across getting-started, runtime contract, schedule guide`.

### Task 6: Final gate (plan owner)

- [ ] `cargo test --workspace` green; `cargo clippy --workspace --all-targets -- -D warnings` 0; `cargo fmt --check` clean.
- [ ] `(cd tree-sitter-lute && npx --yes tree-sitter-cli@latest test)` green, `capabilityVersion` UNCHANGED.
- [ ] Corpus: `lute check-project docs/examples/anseo` exit 0, zero new diagnostics; `compile --all` artifacts differ from pre-branch ONLY by `meta.id` + version strings (scripted JSON diff); `loc export docs/examples/anseo` byte-identical to pre-branch.
- [ ] New-model smoke: author a scratch project (2 scenes with `id:`, one `visited()` edge, one `meta:` block) → `check-project` 0, `compile` prefixes correct, `play` walks.
- [ ] Whole-branch review → merge decision with user.

## Self-Review

- **Spec coverage:** §2 (T1 id + T2 keys + T3 prefix/IR + T4 consumers), §3 (T1 lift + T3 wire + T1 defaults), §4 (T1 warning + authored-map rule), §5 (T3 + T4 restamps), §6 diagnostics (T1/T2), §7 compatibility (T3 byte-diff test, T4 loc pin, T6 corpus gate). No gap found.
- **Placeholder scan:** one deliberate soft spot — `defaults_inherited_legacy_beside_id_is_silent` names the entry point generically because the 0.10.0 defaults-aware test helper's exact name must be read from meta.rs tests; the implementer locates it in-file (it exists — survey §2).
- **Type consistency:** `canonical_scene_key(&TypedMeta) -> Option<String>` (T1) consumed by T2/T3; wire names `id`/`meta` fixed in Global Constraints and repeated in T3/T4.
