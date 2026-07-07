# lute-compile 0.1.0 Cutover Implementation Plan (Plan C of 6)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Make the compiled JSON artifact a self-contained, engine-executable 0.1.0 target: lower `<hub>` (replacing the transitional compile error), emit an executable `expr` AST for every CEL slot (no runtime CEL parser), lower `<when is>` patterns, carry interpolation placeholders, materialize `wait`, harden the envelope (irVersion/capabilityVersion/episodeId), coerce typed attrs to JSON scalars, carry plugin effect bindings, and ship expr-eval conformance fixtures.

**Architecture:** All work in `crates/lute-compile` (pipeline: `normalize` → `expand` → `lower`/`cfg` → `stage` (+ `schedule`) → `address` → serialize via `ir.rs`). Consumes the shipped cel-parser AST. Implements the IR addendum A1–A13 from `docs/superpowers/specs/2026-07-04-lute-compile-json-ir-design.md`. lute-check (Plan B) now accepts hubs/when-is/interps; this plan makes them compile.

**Tech Stack:** Rust; spec = the compile-IR design doc + `docs/proposals/scenario-dsl/0.1.0.md`.

## Global Constraints

- Byte-stability: `ir.rs` field DECLARATION ORDER = serialized order; never reorder existing fields (append new fields at the documented position). Golden/insta snapshots change ONLY where a task intends.
- Determinism: `compile()` is pure; same input → byte-identical output.
- Diagnostic spellings verbatim. New: `E-L10N-PLACEHOLDER` is DEFERRED (needs translation infra — see Deferred). `E-HUB-LOWERING-UNSUPPORTED` (Plan B transitional) is REMOVED when C3 lands.
- Run only lute-compile per task; `cargo test --workspace` gates the final task.
- Reuse: `match_check::parse_expr`/`analyze_expr` (cel AST walk), the folded `StateSchema`, the capability snapshot, the manifest `effects.writes` decls.

## Deferred (out of Plan C — documented, not dropped)

- **A1 `sprite.costume` + costume stage-join** — requires an active character/cast plugin; no example exercises it yet. Add the IR field (schema-only, always `None` until cast ships) but do NOT build costume resolution. (C7 adds the optional field; resolution is a cast-plugin follow-up.)
- **A6 `E-L10N-PLACEHOLDER`** — requires a translation sidecar format + loader that does not exist. The `placeholders` list (C4) is the prerequisite; the equality check ships with the localization pipeline (future plan).

---

### Task C1: Expr AST (`expr`) for CEL slots (A7)

**Files:** Create `crates/lute-compile/src/expr.rs`; Modify `ir.rs` (MatchArm, ChoiceOption, SetCmd gain `expr`), `stage.rs`/`lower.rs` (build expr when constructing those records); Test: expr.rs unit + a compile golden.

**Interfaces:** Produces `pub enum ExprNode` (serde `Serialize`, tagged) with kinds: `lit`(f64|bool|string), `path`, unary `!`/`-`, binary (`&& || == != < <= > >= + - * / in`), `cond`(ternary), `list`, `isSet`(path), `has`(path). `pub fn lower_expr(raw: &str) -> Option<ExprNode>` (parses via cel-parser like `match_check::parse_expr`, walks the AST to ExprNode). All numeric literals → f64 (double).

- [ ] **Step 1:** failing test — `lower_expr("user.level >= (1)")` → `{op:">=", l:{path:"user.level"}, r:{lit:1.0}}`; `lower_expr("$ == 'gold'")` (subject pre-substituted to `_`) → `{op:"==", l:{path:"_"}, r:{lit:"gold"}}`; `lower_expr("has(scene.x)")` → `{has:"scene.x"}`; `lower_expr("$ in ['a','b']")` → `{op:"in", l:{path:"_"}, r:{list:[{lit:"a"},{lit:"b"}]}}`.
- [ ] **Step 2:** run → FAIL.
- [ ] **Step 3:** implement expr.rs (walk cel-parser `Expr`: Ident/Select→path, Call for operators→binary/unary, `has`/`isSet` Call→typed, list literal→list, ternary→cond, Literal→lit). Add `expr: ExprNode` to MatchArm, `expr: Option<ExprNode>` to ChoiceOption (when present), SetCmd (rhs). Build it at record construction in stage.rs (arms/choices) + lower.rs (set). Keep the existing string field (debug).
- [ ] **Step 4:** run → GREEN; `cargo test -p lute-compile` (golden snapshots gain `expr` — regenerate + eyeball that the tree matches the string form).
- [ ] **Step 5:** commit `feat(compile): expr AST for CEL slots — no runtime CEL parser (IR A7)`.

---

### Task C2: `<when is>` → arm expr; `match.subject` debug-only (A13)

**Files:** Modify `crates/lute-compile/src/stage.rs` (match-arm lowering), `expr.rs` (is-pattern → ExprNode); Test: compile golden + unit.

**Interfaces:** Consumes `Arm::When.is: Option<IsPattern>` + `test`. Produces the arm `expr` synthesized from `is` and/or `test` inlined against the subject.

- [ ] **Step 1:** failing tests — an arm `is="gold"` on subject `scene.serve.debut.rank` → `expr` = `{op:"==", l:{path:"scene.serve.debut.rank"}, r:{lit:"gold"}}`; `is="silver|bronze"` → OR-chain `((S)=='silver' || (S)=='bronze')`; `is="unset"` on a bare path → `{op:"!", …isSet(path)}` (or null-check); `is="gold" test="$ != 'x'"` → `AND(is-expr, test-expr)`. `match.subject` stays a string (debug), NOT executed.
- [ ] **Step 2:** run → FAIL.
- [ ] **Step 3:** implement per IR A13: subject inlined into each arm expr; `is` alternation → parenthesized OR; `is="unset"` → `!isSet(path)` for bare-path subject (compound subject → null-check, error if unlowerable); `is`+`test` → `&&`. `otherwise` arm carries no expr. Exhaustiveness is already check-proven (Plan B) — codegen just emits.
- [ ] **Step 4:** run → GREEN; `cargo test -p lute-compile`.
- [ ] **Step 5:** commit `feat(compile): lower <when is> to arm expr; match.subject debug-only (IR A13)`.

---

### Task C3: `<hub>` lowering (A2) — replace the transitional compile error

**Files:** Modify `crates/lute-compile/src/{cfg,stage,ir,lower}.rs`; Test: replace the `valid_hub_doc_fails_compile` test with real hub-lowering tests + compile the `hub-demo.lute` example.

**Interfaces:** New `HubCmd`/`hub` record kind (ir.rs, `kind: "hub"`): `{ addr, id, recordKey: "scene.choices.<id>", options: [{ id, label, lineId, once, exit, when?, whenExpr?, target }], converge }`. Removes the stage.rs `E-HUB-LOWERING-UNSUPPORTED` arm.

- [ ] **Step 1:** failing tests — compiling `docs/examples/showcase/hub-demo.lute` (via a compile test with the showcase project) → Ok(artifact) containing a `hub` record with the options (once/exit flags, targets, when-expr where guarded), `recordKey`, and a `converge` addr; each arm's body lowered; no panic, no E-HUB-LOWERING-UNSUPPORTED.
- [ ] **Step 2:** run → FAIL (currently Err E-HUB-LOWERING-UNSUPPORTED).
- [ ] **Step 3:** implement hub lowering mirroring `<branch>`/`<choice>` in cfg.rs/stage.rs: each hub choice arm gets a label/target like a choice; the hub record carries options with once/exit/when(+whenExpr via C1); recordKey = `scene.choices.<hubId>`; a `converge` label after the hub (auto-exit + post-exit fall-through target). Selection semantics are a runtime property (re-present loop) — the record just carries the option table + converge, exactly as §7 of the IR doc's flat-VM contract describes (choice-like with re-present). Remove the E-HUB-LOWERING-UNSUPPORTED arm + its diags threading if now unused (keep the channel if other arms use it).
- [ ] **Step 4:** run → GREEN; `cargo test -p lute-compile`; compile hub-demo → Ok.
- [ ] **Step 5:** commit `feat(compile): lower <hub> to hub record — re-present option table + converge (IR A2)`.

---

### Task C4: `line.placeholders` (A3)

**Files:** Modify `ir.rs` (LineCmd gains `placeholders`), `lower.rs` (populate from `Line.interps`); Test: compile golden.

**Interfaces:** LineCmd gains `placeholders: Vec<Placeholder>` (omitted when empty); `Placeholder { kind: "path"|"ref"|"reserved", ref: String }` in source order. `text` stays verbatim (the `{{…}}` markers remain in the string).

- [ ] **Step 1:** failing test — a line `:bianca: Hi {{userName}}, {{run.coins}} left, {{@fond}}.` → LineCmd `placeholders: [{kind:reserved,ref:userName},{kind:path,ref:run.coins},{kind:ref,ref:@fond}]`, `text` unchanged.
- [ ] **Step 2:** run → FAIL.
- [ ] **Step 3:** map `Line.interps` (InterpKind→kind, raw→ref, in order) into LineCmd.placeholders during lowering. Choice/hub option labels with interps carry the same list on the option (extend ChoiceOption/hub option if labels interpolate).
- [ ] **Step 4:** run → GREEN; `cargo test -p lute-compile`.
- [ ] **Step 5:** commit `feat(compile): line.placeholders for {{…}} interpolation (IR A3)`.

---

### Task C5: `wait` fully materialized (A8)

**Files:** Modify `crates/lute-compile/src/{lower,stage}.rs` (+ manifest per-directive wait defaults); Test: compile golden.

**Interfaces:** Every record whose directive family defines `wait` carries the RESOLVED value (manifest default ⊕ author override) explicitly in its `Stamp.wait` — no record omits it where the family defines it.

- [ ] **Step 1:** failing test — a `::music{action=start}` (no author wait) record carries the resolved `wait` (manifest default), not an absent field; `::bg` carries `wait:true`; a `::camera` carries its resolved wait. (Audit had `music` missing `wait`.)
- [ ] **Step 2:** run → FAIL.
- [ ] **Step 3:** resolve each directive's effective `wait` from the capability snapshot's per-directive default ⊕ the author's `wait` attr, and always set `Stamp.wait` for wait-family records. (Determine the default source: the manifest AttrDecl default for `wait`.)
- [ ] **Step 4:** run → GREEN; `cargo test -p lute-compile`.
- [ ] **Step 5:** commit `feat(compile): materialize resolved wait on every wait-family record (IR A8)`.

---

### Task C6: Envelope hardening — irVersion, capabilityVersion, episodeId (A9)

**Files:** Modify `ir.rs` (Artifact/ArtifactMeta), `lib.rs` (`compile`/`artifact_meta`); Test: compile golden + unit.

**Interfaces:** Artifact envelope gains `irVersion` (IR schema version, bump to `"0.1.0"`) distinct from `lute` (language pin); `capabilityVersion` (the plugin-system §13 snapshot stamp, from the capability snapshot); `meta.episodeId` normalized to equal the lineId episode segment byte-for-byte (default lowercase `s{season:02}ep{episode:02}`).

- [ ] **Step 1:** failing tests — envelope has `irVersion: "0.1.0"`; `capabilityVersion` present + equals the snapshot's `capability_version`; `meta.episodeId` is lowercase `s01ep01` and matches the `lineId` middle segment (audit found `S01EP01` vs `s01ep01`).
- [ ] **Step 2:** run → FAIL.
- [ ] **Step 3:** add `irVersion` (const `LUTE_IR_VERSION` → "0.1.0" — note this changes the existing `lute` field's meaning; keep `lute` as language pin and ADD `irVersion`), thread `capabilityVersion` from the snapshot (lute-manifest `capability_version`), and normalize episodeId (lowercase, matching the address pass's lineId derivation) in artifact_meta. Verify lineId derivation + meta.episodeId agree.
- [ ] **Step 4:** run → GREEN; `cargo test -p lute-compile`.
- [ ] **Step 5:** commit `feat(compile): envelope irVersion + capabilityVersion + episodeId normalization (IR A9)`.

---

### Task C7: Attr coercion in records (A10) + `sprite.costume` schema field (A1 schema-only)

**Files:** Modify `ir.rs` (numeric/bool attr fields → typed; SpriteCmd gains optional `costume`), record construction; Test: compile golden.

**Interfaces:** A manifest-declared `number` attr serializes as a JSON number, `bool` as a JSON bool (audit: `camera.zoom:1.2` number vs `camera.shake:"0.4"` string — both should be numbers). SpriteCmd gains `costume: Option<String>` (always `None` until cast ships — schema presence only, A1 deferred resolution).

- [ ] **Step 1:** failing test — `::camera{shake="0.4" zoom="1.2"}` → both `shake` and `zoom` serialize as JSON numbers (0.4, 1.2), not strings; a bool attr → JSON bool.
- [ ] **Step 2:** run → FAIL.
- [ ] **Step 3:** at record construction, coerce each attr per its manifest AttrDecl type (number→f64 JSON, bool→bool JSON, else string), per DSL §4.5 coercion grammar. Add `costume: Option<String>` to SpriteCmd (None for now).
- [ ] **Step 4:** run → GREEN; `cargo test -p lute-compile`.
- [ ] **Step 5:** commit `feat(compile): coerce typed attrs to JSON scalars + sprite.costume field (IR A10/A1)`.

---

### Task C8: Plugin effect bindings (A12)

**Files:** Modify `ir.rs` (OtherCmd/plugin record gains `effects`), the plugin-lowering path (lower.rs/stage.rs); Test: compile golden (showcase `::serve`).

**Interfaces:** A `plugin` record whose manifest directive declares `effects.writes` gains a resolved `effects: Vec<Effect>` — `Effect { path: String, from: EffectSource }` where `EffectSource` is `{bridgeResult: String}` or `{op:"increment", by: N}` or a literal, with attr-templates (`fromAttr`) already substituted at compile time.

- [ ] **Step 1:** failing test — the showcase `::serve{resultKey="debut" …}` plugin record carries `effects: [{path:"scene.serve.debut.rank", from:{bridgeResult:"rank"}}, {path:"scene.serve.debut.attempts", from:{op:increment, by:1}}, …]` with `resultKey` substituted into the paths.
- [ ] **Step 2:** run → FAIL.
- [ ] **Step 3:** at plugin-record construction, read the directive's manifest `effects.writes`, resolve `fromAttr` path templates against the record's attrs, and emit the `effects` array. Reuse the manifest AttrDecl/effects structures from lute-manifest.
- [ ] **Step 4:** run → GREEN; `cargo test -p lute-compile` (showcase e2e golden gains `effects` on the serve record — regenerate + verify).
- [ ] **Step 5:** commit `feat(compile): plugin records carry resolved effect bindings (IR A12)`.

---

### Task C9: Expr-eval conformance fixtures + workspace green

**Files:** Create `crates/lute-compile/tests/fixtures/expr_eval/` (JSON fixtures: `{expr, state, expected}`) + a test that asserts a reference tree-walk evaluator matches; ensure `cargo test --workspace` green.

**Interfaces:** A golden fixture set (`expr` A7 tree + a state snapshot → expected value) that any runtime SDK (Plan E lute-dart) must pass. A minimal Rust reference evaluator (in the test) validates the fixtures are self-consistent.

- [ ] **Step 1:** author ~10 fixtures covering the profile: comparisons, `&&`/`||`/`!`, arithmetic, `in`, ternary, `has`/`isSet`, string/bool/enum equality, `<when is>`-derived exprs.
- [ ] **Step 2:** write a small Rust tree-walk evaluator over `ExprNode` + a test asserting each fixture's expr evaluates to `expected` under `state`. (This is the conformance contract; lute-dart mirrors it.)
- [ ] **Step 3:** `cargo test --workspace` → fully green (incl. the hub-demo now compiling, all regenerated goldens verified).
- [ ] **Step 4:** commit `test(compile): expr-eval conformance fixtures + reference evaluator (IR A7 contract)`.

---

## Self-Review (authoring)

1. **Spec coverage:** IR addendum A2 (C3 hub), A3 (C4 placeholders), A7 (C1 expr), A8 (C5 wait), A9 (C6 envelope), A10 (C7 coercion), A12 (C8 effects), A13 (C2 when-is). Deferred with rationale: A1 costume resolution (no cast plugin), A6 E-L10N-PLACEHOLDER (no translation infra) — C4/C7 lay their schema groundwork.
2. **Placeholders:** test contracts given; implementers read files for exact edit sites; golden regenerations must be eyeballed (only intended fields change).
3. **Type consistency:** C1's `ExprNode` feeds C2 (when-is) + C3 (hub option whenExpr). C6 keeps `lute` (language pin) and ADDS `irVersion`. C3 removes E-HUB-LOWERING-UNSUPPORTED (added Plan B B6).
