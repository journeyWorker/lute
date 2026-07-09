# lute-compile 0.2.0 — kind envelope + quest/on records (Plan D of 5)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Compile a **quest** document to its JSON artifact — the `kind` envelope discriminator, kind-polymorphic envelope `meta`, and the `quest`/`on` command records (objectives inlined in the quest record) — reusing the existing flat-record + CEL-`ExprNode` + addressing machinery. The scene artifact is unchanged except for the new leading `kind` field and the `lute`/`irVersion` bump to `0.2.0`.

**Architecture:** `compile()` gates on the clean check (D6, unchanged), then dispatches on `folded.doc_kind` (from lute-check's Plan C): **scene** = the existing shot loop (byte-identical output aside from `kind:"scene"` + version); **quest** = a parallel loop over `doc.quests` that emits, per quest, a `quest` declaration record (with the objective table inlined + symbolic `body` targets) followed by the objective completion bodies and `<on>` records+bodies, all through the existing `stage`/`cfg`/`address` machinery keyed by quest-declaration index. Identity (`lineId`) becomes per-declaration: scene = one document-wide scope (`{character}.{episodeId}`), quest = one scope per `<quest>` (`{questId}`). Design addendum: `docs/superpowers/specs/2026-07-09-lute-compile-0.2.0-quest-ir.md`.

**Tech Stack:** Rust (workspace `cargo test` + `cargo insta`), spec `docs/proposals/scenario-dsl/0.2.0.md`, IR addendum (above), depends on Plans A–C committed.

## Global Constraints

- IR addendum is normative for record shapes: read `docs/superpowers/specs/2026-07-09-lute-compile-0.2.0-quest-ir.md` FIRST. Envelope field DECLARATION ORDER is the byte-stability contract — `kind` is the FIRST `Artifact` field.
- `Artifact.kind` uses lute-check's `pub enum DocKind { Scene, Quest }` (compile already depends on lute-check; re-export/import it) serialized lowercase — add `#[derive(Serialize)]`-compatible handling (either derive Serialize on DocKind in lute-check with `rename_all="lowercase"`, OR map to a compile-local `DocKind` serde enum mirroring `Role` at ir.rs:83-90). PREFER a compile-local serde enum to avoid adding serde to lute-check's public type (keep the crates' serialization concerns separate); map `check::DocKind -> ir::DocKind` once.
- `ArtifactMeta` becomes `#[serde(untagged)] enum { Scene(SceneMeta), Quest(QuestMeta) }` — `SceneMeta` = the CURRENT `ArtifactMeta` struct fields verbatim (scene bytes UNCHANGED); `QuestMeta { title?, content_lang? }` (both `skip_serializing_if=None`, camelCase `contentLang`).
- New `Command::{Quest(QuestCmd), On(OnCmd)}`; objectives are INLINED as `ObjectiveEntry` structs in `QuestCmd.objectives` (NO standalone objective record). Each `*Cmd`: first field `pub addr: String`. The three `impl Command` match sites gain arms: `addr_mut` + `stamp_mut` (compiler-forced, exhaustive); `for_each_target` (has `_` wildcard) needs EXPLICIT arms for `Quest`/`On` because they carry symbolic `body`/objective-`body` targets that MUST be rewritten to concrete addrs — do NOT let them fall through `_`.
- `LUTE_LANG_VERSION` + `LUTE_IR_VERSION` both bump to `"0.2.0"` (lib.rs:32,36). This re-records ALL scene e2e insta snapshots (kind + version) — expected, done in the final task via `cargo insta`.
- CEL slots (`start`/`fail`/`done`/`when`) lower via the existing `crate::expr::lower_expr` into the `{raw, expr}` dual-field shape (like `HubOption.when`/`expr`). Placeholders in titles via `scan_label_interps` + `placeholder_from_interp` (as hub option labels).
- Reuse the existing `stage::walk_seq` for objective/on BODY lowering (they contain ordinary content/set/directive/match/branch). The `Node::On|Objective` no-op arms in `walk_seq` (Plan A) are REPLACED / made unreachable — quest bodies are driven explicitly from the quest loop, not via a shot `walk_seq`.
- Work in the worktree `~/Workspace/lute/.worktrees/lute-0.2.0` on branch `feat/lute-0.2.0`. Run `cargo test -p lute-compile` per task; `cargo test --workspace` gates the final task (note the pre-existing e2e parallelism flake — confirm 6/6 in isolation).

---

### Task 1: IR types — `kind` envelope, `ArtifactMeta` enum, `Quest`/`On` records + `impl Command` seams + version bump

**Files:**
- Modify: `crates/lute-compile/src/ir.rs` (`Artifact` ~14-26, `ArtifactMeta` ~28-37, `Command` ~101-121, `impl Command` ~474-548, new `*Cmd`/`ObjectiveEntry`/`DocKind`/`SceneMeta`/`QuestMeta`), `crates/lute-compile/src/lib.rs` (version consts ~32/36)
- Test: `crates/lute-compile/tests/ir_golden.rs` (per-record exact-JSON tests)

**Interfaces:**
- Produces:
  - `pub enum DocKind { Scene, Quest }` (`#[serde(rename_all="lowercase")]`, mirrors `Role`).
  - `Artifact { pub kind: DocKind, pub lute, pub ir_version, pub capability_version, pub meta: ArtifactMeta, pub state, pub commands }` (`kind` FIRST).
  - `#[serde(untagged)] pub enum ArtifactMeta { Scene(SceneMeta), Quest(QuestMeta) }`; `SceneMeta` = current fields (`character, season, episode, episode_id, title?`); `QuestMeta { title?: Option<String>, content_lang?: Option<String> }`.
  - `pub struct QuestCmd { pub addr, pub id, pub title?: Option<String>, pub title_line_id?: Option<String>, pub start?: Option<CelPair>, pub fail?: Option<CelPair>, pub objectives: Vec<ObjectiveEntry>, #[serde(flatten)] pub stamp: Stamp }`.
  - `pub struct ObjectiveEntry { pub id, pub title?, pub title_line_id?, pub done: CelPair, pub when?: Option<CelPair>, pub optional: bool, pub body?: Option<String> }` (`body` = target addr into completion records; None when empty).
  - `pub struct OnCmd { pub addr, pub event: String, pub when?: Option<CelPair>, pub body: String, #[serde(flatten)] pub stamp: Stamp }`.
  - `CelPair { raw: String, #[serde(skip_serializing_if="Option::is_none")] expr: Option<ExprNode> }` — the `{raw, expr}` shape (reuse if an equivalent exists on `HubOption`; else add this small struct). Camel-case field names per `#[serde(rename_all="camelCase")]`.

- [ ] **Step 1: Failing ir_golden tests** — in `ir_golden.rs`, one exact-JSON assertion per new shape (mirror the existing `fn j(cmd)` + per-kind tests):
```rust
#[test] fn quest_record_serializes_per_spec() {
    let cmd = Command::Quest(QuestCmd {
        addr: "001-0100".into(), id: "rescueHalsin".into(),
        title: Some("Rescue".into()), title_line_id: Some("rescueHalsin.title".into()),
        start: Some(CelPair { raw: "run.act == 1".into(), expr: None }),
        fail: None,
        objectives: vec![ObjectiveEntry {
            id: "reachGrove".into(), title: Some("Reach".into()),
            title_line_id: Some("rescueHalsin.reachGrove".into()),
            done: CelPair { raw: "run.region == 'grove'".into(), expr: None },
            when: None, optional: false, body: None,
        }],
        stamp: Stamp::default(),
    });
    assert_eq!(j(&cmd), r#"{"kind":"quest","addr":"001-0100","id":"rescueHalsin","title":"Rescue","titleLineId":"rescueHalsin.title","start":{"raw":"run.act == 1"},"objectives":[{"id":"reachGrove","title":"Reach","titleLineId":"rescueHalsin.reachGrove","done":{"raw":"run.region == 'grove'"},"optional":false}]}"#);
}
#[test] fn on_record_serializes_per_spec() {
    let cmd = Command::On(OnCmd {
        addr: "001-0400".into(), event: "questComplete".into(),
        when: None, body: "001-0500".into(), stamp: Stamp::default(),
    });
    assert_eq!(j(&cmd), r#"{"kind":"on","addr":"001-0400","event":"questComplete","body":"001-0500"}"#);
}
```
> Verify the EXACT expected JSON against the actual serde output when you run it (field order = declaration order; None fields omitted; `expr:None` omitted). Adjust the expected string to match reality on first run, then lock it. Add a scene-envelope test if `ir_golden.rs` tests `Artifact` (confirm `kind:"scene"` appears first).

- [ ] **Step 2: Run** → FAIL (types don't exist).
- [ ] **Step 3: Implement** the types + `impl Command` arms (`addr_mut`: return `&mut self.addr` for Quest/On; `stamp_mut`: `&mut self.stamp`; `for_each_target`: Quest → visit each `objectives[].body` (if Some) as a target; On → visit `body`). Bump `LUTE_LANG_VERSION`/`LUTE_IR_VERSION` to `"0.2.0"`. Add `#[serde(rename_all="camelCase")]` to the new `*Cmd`/`ObjectiveEntry`.
- [ ] **Step 4: Run** `cargo test -p lute-compile --test ir_golden` → PASS.
- [ ] **Step 5: Commit** — `git commit -am "feat(compile): IR kind envelope + quest/on records + 0.2.0 version bump (IR addendum §1,§3)"`

---

### Task 2: Per-declaration identity — generalize `assign_identity`/`IdCx`

**Files:**
- Modify: `crates/lute-compile/src/address.rs` (`IdCx` ~24-27, `assign_addresses` ~32-67, `assign_identity` ~78-132, `ShotRecords` ~14-19)
- Test: `crates/lute-compile/tests/address.rs`

**Interfaces:**
- Produces: identity keyed per addressing unit. Design: give `ShotRecords` a `pub prefix: String` (the lineId prefix for this unit's records) and split `assign_identity` to run PER identity SCOPE, where a scope = a set of units sharing one prefix + one code-counter namespace. Scene = ONE scope (all shots, prefix `{character}.{episodeId}`); quest = one scope per quest (`{questId}`). Concretely: `assign_addresses(units: Vec<ShotRecords>, scopes: &[IdScope])` where `IdScope { prefix: String, unit_range: Range<usize> }` (contiguous units in one scope), OR simpler — carry `prefix` on each `ShotRecords` and make `assign_identity`'s code-counter reset when the prefix changes across units in emission order (scene: all same prefix = one counter; quest: prefix changes per quest = counter resets per quest). PICK the simpler `prefix`-per-unit + reset-on-prefix-change; document it.

- [ ] **Step 1: Failing test** in `address.rs`: a two-unit input with DIFFERENT prefixes where the SAME (speaker, no-code) line in each unit gets DISTINCT `{prefix}.{speaker}_{code}` ids and the code counter RESETS per prefix (so unit 2's first backfilled code is not continued from unit 1). Assert the lineIds.
- [ ] **Step 2: Run** → FAIL.
- [ ] **Step 3: Implement.** Add `prefix: String` to `ShotRecords`. In `assign_identity`, replace the single `let prefix = format!("{}.{}", cx.character, cx.episode_id)` with a per-unit prefix (thread the current unit's prefix through Pass 2; keep Pass 1's max-authored-code map PER SCOPE — reset when the prefix changes). Scene callers set every `ShotRecords.prefix` to `{character}.{episodeId}`; the quest caller (Task 3) sets each to `{questId}`. `IdCx` may be retired or kept for the scene default. Keep scene output BYTE-IDENTICAL (same prefix on every shot ⇒ one continuous counter ⇒ identical ids).
- [ ] **Step 4: Run** `cargo test -p lute-compile --test address` (+ the e2e goldens in the final task confirm scene bytes unchanged) → PASS.
- [ ] **Step 5: Commit** — `git commit -am "feat(compile): per-declaration lineId prefix + per-scope code counters (IR addendum §4)"`

---

### Task 3: `compile()` kind dispatch + quest lowering

**Files:**
- Modify: `crates/lute-compile/src/lib.rs` (`compile` ~41-122, `artifact_meta` ~128-153, new `quest_meta`/quest lowering helpers), `crates/lute-compile/src/stage.rs` (real `walk_on` + objective-body lowering; the `Node::On|Objective` arms ~84)
- Test: `crates/lute-compile/tests/compile.rs` (inline quest source → artifact assertions)

**Interfaces:**
- Consumes: `folded.doc_kind` (lute-check Plan C), `doc.quests`, the IR types (Task 1), per-unit prefix (Task 2), `stage::walk_seq`.
- Produces: `compile()` emits a quest artifact for a `kind: quest` document — one `quest` record per `<quest>` (objectives inlined, symbolic `body` targets for non-empty objective bodies + `<on>` arms), followed by the objective completion bodies + `on` records+bodies, addressed per-quest.

- [ ] **Step 1: Failing tests** in `compile.rs` — an inline `kind: quest` source (mirror the DSL Appendix D worked example, trimmed):
```rust
#[test] fn quest_doc_compiles_to_quest_artifact() {
    let art = compile(&input_quest(QUEST_SRC)).expect("compiles");
    let j = serde_json::to_value(&art).unwrap();
    assert_eq!(j["kind"], "quest");
    let cmds = j["commands"].as_array().unwrap();
    let q = cmds.iter().find(|c| c["kind"] == "quest").expect("quest record");
    assert_eq!(q["id"], "rescueHalsin");
    assert_eq!(q["objectives"].as_array().unwrap().len(), 2);
    assert!(cmds.iter().any(|c| c["kind"] == "on" && c["event"] == "questComplete"));
    // an <on> body content line lowered as a line record with a {questId} lineId:
    assert!(cmds.iter().any(|c| c["kind"] == "line" && c["lineId"].as_str().map_or(false, |s| s.starts_with("rescueHalsin."))));
}
```
> `input_quest` mirrors the existing `input()` helper but with a quest source + `load_core_snapshot()`; QUEST_SRC = a `kind: quest` doc with one `<quest id="rescueHalsin">` + 2 objectives + an `<on event="questComplete">` with a `::set` + a `:narrator:` line. Ensure the schema it reads (run.* paths in start/done) is declared inline via `state:` so check passes.
- [ ] **Step 2: Run** → FAIL.
- [ ] **Step 3: Implement.**
  - `compile()`: after the D6 gate + fold, `match folded.doc_kind`: **Scene** = the existing shot loop → `SceneMeta` envelope. **Quest** = for each `(i, quest)` in `doc.quests.enumerate()`: a fresh `cfg::Emitter`; allocate `em.fresh()` labels for each non-empty objective body + each `<on>` arm; push the `Command::Quest(QuestCmd{…})` (objectives inlined, `body` = the objective's body label `.sym()` or None; `start`/`fail` via `lower_expr`); then for each objective with a body, `em.bind(label)` + `stage::walk_seq(&obj.body, …)`; for each `<on>`, push `Command::On(OnCmd{ body: arm_label.sym(), … })` then `em.bind(arm_label)` + `walk_seq(&on.body, …)`. Collect into `ShotRecords { shot: (i as i64)+1, prefix: quest.id.clone(), recs, trailing }`. Then `assign_addresses`.
  - `artifact_meta` → kind-aware: scene builds `SceneMeta` (as today); quest builds `QuestMeta { title: lookup("title"), content_lang: lookup("contentLang") }`.
  - `stage.rs`: replace the transitional `Node::On(_) | Node::Objective(_) => {}` in `walk_seq` — since quest bodies are driven from the quest loop (NOT via a shot walk_seq), these Node variants should be `unreachable!()` inside `walk_seq` (a scene body can never contain them — admission forbids it, D6 gate proved it). Provide the real `<on>`/objective lowering in the quest loop (or small `stage::walk_on`/`emit_quest` helpers mirroring `walk_hub`).
- [ ] **Step 4: Run** `cargo test -p lute-compile --test compile` → PASS.
- [ ] **Step 5: Commit** — `git commit -am "feat(compile): kind dispatch + quest/on lowering + quest artifact meta (IR addendum §5,§6)"`

---

### Task 4: e2e quest golden + `assert_artifact_invariants` quest-awareness

**Files:**
- Create: `docs/examples/quest-grove.lute` (a `kind: quest` fixture — the DSL Appendix D worked example, self-contained with an inline `state:` so it checks clean)
- Modify: `crates/lute-compile/tests/e2e.rs` (`assert_artifact_invariants` ~45-109 for quest addrs; a new `#[test]` golden)
- Test: the new e2e golden (insta snapshot).

**Interfaces:** none new. Adds a full-artifact golden for a quest doc + extends the invariant checker to accept quest records.

- [ ] **Step 1: Fixture.** Write `docs/examples/quest-grove.lute` (`kind: quest`, `luteVersion: "0.2.0"`, an inline `state:` declaring the `run.*` paths its `start`/`done`/`when` reference so `check()` is clean, one+ `<quest>` with objectives + `<on>` handlers with content + `::set`). Confirm `check()` on it is clean (run the CLI or a quick test) before goldening.
- [ ] **Step 2: Invariants.** Extend `assert_artifact_invariants` (e2e.rs ~45-109) so quest records pass: `quest`/`on` records have `addr`; the `objectives[].body` + `on.body` targets resolve to a real record addr OR a unit's one-past-end converge (same `valid` set logic, now over quest-indexed units); CEL fields (`start`/`fail`/`done`/`when`/`on.when`) are `@`/`$`-free & fully expanded. Guard the scene-specific assertions behind `if json["kind"] == "scene"` where they don't apply to quests.
- [ ] **Step 3: Golden test.**
```rust
#[test] fn quest_grove() { golden("quest_grove", "../../docs/examples/quest-grove.lute", None); }
```
Run `cargo test -p lute-compile --test e2e quest_grove` → it writes a new `.snap.new`; review with `cargo insta review` (or inspect the artifact JSON for correctness: `kind:"quest"`, quest record with inlined objectives, on records, per-quest lineIds, resolved addrs) and accept.
- [ ] **Step 4: Run** `cargo test -p lute-compile --test e2e` → PASS (7/7 incl. the new quest golden).
- [ ] **Step 5: Commit** — `git commit -am "test(compile): e2e quest artifact golden + quest-aware invariants (IR addendum)"`

---

### Task 5: Re-record scene goldens (kind + version) + full green

**Files:**
- Modify: `crates/lute-compile/tests/snapshots/*.snap` (the 3 scene e2e goldens: `e2e__bianca_s01ep02.snap`, `e2e__components_scene.snap`, `e2e__showcase_episode01.snap`), any ir_golden envelope test.
- Test: full `cargo test --workspace`.

- [ ] **Step 1: Re-record.** The `kind:"scene"` field + `lute`/`irVersion` → `"0.2.0"` change every scene envelope. Run `cargo test -p lute-compile --test e2e` → 3 `.snap.new`; `cargo insta review` and CONFIRM each diff is EXACTLY: (a) a new leading `"kind": "scene"`, (b) `"lute": "0.1.0"→"0.2.0"`, (c) `"irVersion": "0.1.0"→"0.2.0"`, and NOTHING else (no command/state/lineId drift — the untagged `SceneMeta` keeps scene meta byte-identical, and `kind: scene` in the input doesn't alter records). If ANY other bytes changed, STOP — it means the identity/meta refactor (Tasks 2-3) regressed scene output; fix before accepting.
- [ ] **Step 2: Accept** the 3 scene snapshots (`cargo insta accept` after confirming the diffs).
- [ ] **Step 3: Run** `cargo test --workspace` → GREEN (all crates; note the e2e parallelism flake — confirm `cargo test -p lute-compile --test e2e` 6→7/7 in isolation).
- [ ] **Step 4: Commit** — `git commit -am "test(compile): re-record scene goldens for kind envelope + 0.2.0 version (IR addendum §1)"`

---

## Self-Review checklist (run before executing)

1. **Spec coverage:** kind envelope + QuestMeta (addendum §1) → T1; quest/on records + inlined objectives (§3) → T1,T3; per-quest addressing/identity (§4) → T2,T3; compile flow (§6) → T3; e2e coverage → T4; scene byte-stability + version → T5.
2. **Placeholder scan:** the ir_golden expected-JSON strings + QUEST_SRC/fixture are to be finalized against actual serde output / a clean check on first run (flagged); every production interface is concrete.
3. **Type consistency:** `DocKind`/`ArtifactMeta`/`SceneMeta`/`QuestMeta`/`QuestCmd`/`ObjectiveEntry`/`OnCmd`/`CelPair`/`ShotRecords.prefix` names stable across tasks. `Command::{Quest,On}` (2 variants, objectives inlined — NO objective record).
4. **Byte-stability:** scene output changes ONLY by `kind:"scene"` + version (T5 verifies exactly this); the untagged `ArtifactMeta::Scene` and same-prefix code counters preserve everything else.
5. **0.3.0 seam:** additive envelope + `DocKind` enum accommodate 0.3.0's relational sections; no over-building (no relations/facts here).
