# Lute Heavy Checker Precision (E-WRITE-CONFLICT + E-REF-TYPE) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Two independent checker-precision features. **B1** rebuilds `E-WRITE-CONFLICT` on each directive's declared **`writes[]` state-write targets** (resolved per clip) instead of the track-key subject heuristic, so two timeline clips conflict only when their *actual resolved state writes* overlap in time. **B2** adds `E-REF-TYPE`: a `@ref` whose def produces a type incompatible with the CEL slot's statically-known expected type is flagged — **design-first**, behind a design-review gate.

**Architecture:** Both features live in `lute-check`. B1 threads the resolved `CapabilitySnapshot` into `timeline::resolve_timeline`, adds a pure `clip_write_targets` resolver, and replaces the subject-based cross-track conflict pass with a state-write-target-overlap pass. B2 adds an `ExpectedType` model + `Ctx.def_types` (name→produced `Type`), sets an expected type per CEL slot where statically known, and emits `E-REF-TYPE` only when BOTH the produced and expected types are known and incompatible. No new crates; no runtime CEL eval (still parser-only via `cel-parser`).

**Tech Stack:** Rust (workspace, rustc 1.96.1); `lute-manifest` (`Type`, `WriteDecl`, `PathSegment`, `CapabilitySnapshot`, `DirectiveDecl.effects`); `lute-syntax` (`Timeline`, `Track`, `TrackKey`, `Clip`, `ClipNode`, `Set`, `CelSlot`, `CelKind`); `lute-cel` (`cel-parser` AST, `scan_refs`). `insta`/plain asserts for tests.

## Global Constraints

- **rustup stable 1.96.1** via `~/.cargo/bin`. Every fresh shell: `export PATH="$HOME/.cargo/bin:$PATH"`. NEVER `brew install rust`.
- **Worktree authoritative.** Work in `/Users/journey/Workspace/lute/.worktrees/lute-lsp-rust` on `feat/lute-lsp-rust` (== `main` at plan authoring, HEAD `ba48381`). **HARNESS QUIRK:** `write`/`edit` resolve RELATIVE paths against the MAIN workspace (`~/Workspace/lute`), NOT the worktree — always use ABSOLUTE worktree paths; after every commit verify `git status` clean in BOTH trees (`?? .worktrees/` in the main tree is expected).
- **TDD, tester-first.** Failing test first, confirm the exact failure, minimal impl, green. Own-crate `cargo test -p lute-check` during a task; full-workspace gate at plan end.
- **Format touched crates** (`cargo fmt -p lute-check`); keep `cargo fmt --check` clean. **Clippy `-D warnings`** per touched crate before each commit (`cargo clippy -p lute-check --all-targets -- -D warnings`).
- **Cross-crate discipline:** `resolve_timeline`'s signature change (B1) and `Ctx`'s new field (B2) are internal to `lute-check`, but grep + `cargo build -p lute-check -p lute-lsp -p lute-cli` after each to catch call sites (the LSP timeline/hover features and CLI call into `check()`).
- **No divergence** (inviolable): the write-target/type logic runs inside the single `check()` and its helpers; the LSP consumes `check()`'s output — never a second implementation. `lute-lsp/tests/divergence.rs` (incl. `divergence_holds_under_plugin_project`) MUST stay green.
- **Determinism** (§3.2) + **never-panic**: all sets `BTreeSet`/sorted; target resolution and type comparison are total (a malformed/partial slot or unresolved attr yields no panic and a conservative fallback, never a crash).
- **Snapshot-is-SoT:** write targets derive from the assembled `CapabilitySnapshot` (`DirectiveDecl.effects.writes`); def/attr types derive from the snapshot + `parse_meta`. No hardcoded directive/def vocabulary.

## Spec source-of-truth

- Language spec §8 (`@ref`/defs typing), §9.4/§9.6 (state reads), §11.4 (timeline / one-writer-per-track): `docs/proposals/scenario-dsl/0.0.1.md`.
- Plugin spec §7 (`Type`), §8 (directive `effects.writes[]`): `docs/proposals/plugin-system/0.0.1.md`.

## Spec decisions (READ FIRST — these resolve the underspecification the prior draft flagged)

1. **E-WRITE-CONFLICT is a STATE-write conflict, not a staging-subject conflict.** DSL §11.4 (line 471) is "one writer per track" (duplicate track keys → `E-DUP-TRACK`). The current `E-WRITE-CONFLICT` compares the track-key *subject* (`subject_of`), which only distinguishes cross-track writers when two tracks share a subject via different keys — i.e. `property=` tracks, which §11.4 (line 522) **defers**. Rather than deepen a heuristic gated on a deferred feature, B1 redefines the conflict in terms of what each clip's directive **actually writes** (`effects.writes[]`), resolved to concrete state paths. This **decouples the check from the deferred property-track feature** and aligns with the spec's directive-effects model. The `TrackKey` subject is used ONLY as a conservative fallback when a clip's writes cannot be resolved (unknown directive / unresolved `fromAttr`).
2. **A directive that provably writes nothing does not conflict.** A known directive with empty `effects.writes[]` contributes no write targets → cannot raise `E-WRITE-CONFLICT` (today it can, purely from its track subject — a false positive this plan removes).
3. **B2 is design-first.** The expected-type-per-CEL-slot model does not exist yet. Task B2.1 produces the model + a doc note and stops at a **design-review gate** (reviewer + human) before B2.2 implements. If the model proves too broad, B2.2 scopes to the highest-confidence contexts (`SetExpr` RHS vs target-path type; `Condition` expects `bool`) and defers the rest.

## File Structure

- `crates/lute-check/src/timeline.rs` — B1: `resolve_timeline` gains a `snapshot: &CapabilitySnapshot` param; new pure `clip_write_targets(node, snapshot, track_subject) -> WriteTargets`; the cross-track pass compares resolved targets (path-prefix overlap) instead of `subject_of`. Module doc's "scope/limitation" note replaced with the new model.
- `crates/lute-check/src/check.rs` — B1: pass `self.snapshot` to `resolve_timeline` (call site ~line 350).
- `crates/lute-check/src/ctx.rs` — B2: add `ExpectedType` enum + `Ctx.def_types: BTreeMap<String, Type>` + the design-note doc-comment.
- `crates/lute-check/src/cel_resolve.rs` — B2: emit `E-REF-TYPE` in `check_cel_slot` when a `@ref`'s produced type (from `ctx.def_types`) is incompatible with the slot's expected type; the deferral NOTE (lines 45-53) is removed.
- `crates/lute-check/src/check.rs` — B2: build `ctx.def_types` from `parse_meta` inline defs + `snapshot.defs` (`DefDecl.ty`); set each slot's expected type where statically known (see B2.1 for the exact contexts) before calling `check_cel_slot`.

---

# Phase B1 — E-WRITE-CONFLICT on resolved `writes[]` state targets

## Task B1.1: pure `clip_write_targets` resolver

**Files:**
- Modify: `crates/lute-check/src/timeline.rs` (add the resolver + supporting types; NO behavior change to `resolve_timeline` yet)
- Test: `crates/lute-check/src/timeline.rs` `#[cfg(test)] mod tests`

**Interfaces:**
- Consumes: `lute_manifest::snapshot::CapabilitySnapshot`, `lute_manifest::schema::{WriteDecl, PathSegment}` (via `DirectiveDecl.effects`), `lute_syntax::ast::{ClipNode, Set, Directive, AttrValue}`.
- Produces:
  ```rust
  /// What a clip writes, for cross-track conflict detection.
  #[derive(Clone, Debug, PartialEq)]
  enum WriteTargets {
      /// Fully-resolved concrete state-write paths (e.g. "scene.minigame.service01.score").
      Paths(std::collections::BTreeSet<String>),
      /// Writes are unresolvable to concrete paths (unknown directive, or a
      /// `fromAttr` path segment with no matching clip attr) — fall back to the
      /// coarse track subject as a single conservative target.
      Coarse(String),
      /// The clip provably writes no state (known directive, empty `effects.writes[]`).
      None,
  }

  /// Resolve what `node` writes. `track_subject` is the clip's track subject
  /// (`subject_of(&track.key)`), used only for the `Coarse` fallback; `None`
  /// track subject with unresolvable writes ⇒ `WriteTargets::None` (a Channel
  /// track with an unknown directive cannot be scoped, so it never conflicts).
  fn clip_write_targets(
      node: &ClipNode,
      snapshot: &CapabilitySnapshot,
      track_subject: Option<&str>,
  ) -> WriteTargets;
  ```
- Resolution rules (implement exactly):
  - `ClipNode::Set(s)` ⇒ `Paths({ s.path.clone() })` (a `::set` writes its target path verbatim).
  - `ClipNode::Directive(d)`:
    - `snapshot.directive(&d.tag)` is `None` ⇒ unknown directive ⇒ `track_subject.map(|s| Coarse(s.into())).unwrap_or(None)`.
    - `Some(decl)` with `decl.effects` `None` or `effects.writes` empty ⇒ `WriteTargets::None`.
    - `Some(decl)` with non-empty `writes`: for each `WriteDecl`, build `{scope}` then append each `PathSegment` joined by `.`:
      - `PathSegment::Literal(seg)` ⇒ `seg`.
      - `PathSegment::FromAttr { from_attr }` ⇒ the clip directive's attr value: `d.attrs.iter().find(|a| a.key == from_attr.name)` and, if its `value` is `AttrValue::Str(v)`, use `v`; otherwise the segment is UNRESOLVABLE.
      - If ANY segment of ANY write is unresolvable ⇒ return `track_subject.map(|s| Coarse(s.into())).unwrap_or(WriteTargets::None)` (don't emit partial paths). Otherwise collect all fully-resolved paths into `Paths(set)`.

- [ ] **Step 1: Write the failing tests** (add a `snapshot_with_writer()` helper that inserts into `load_core_snapshot()` a `::writer` directive with `effects.writes = [scene.box.<key>.a, scene.box.<key>.b]` using a `key` `slotId`/`str` attr — mirror `tests/directive_slots.rs::snapshot_with_minigame` for building `DirectiveDecl`/`DirectiveEffects`/`WriteDecl`/`PathSegment::FromAttr`):

```rust
#[test]
fn write_targets_resolve_fromattr() {
    let snap = snapshot_with_writer();
    // a ::writer directive clip with key="k1"
    let node = writer_clip("k1"); // ClipNode::Directive with attr key="k1"
    assert_eq!(
        clip_write_targets(&node, &snap, Some("box")),
        WriteTargets::Paths(
            ["scene.box.k1.a".to_string(), "scene.box.k1.b".to_string()]
                .into_iter().collect()
        )
    );
}

#[test]
fn write_targets_set_clip_is_its_path() {
    let snap = lute_manifest::core::load_core_snapshot();
    let node = set_clip("scene.affect.bianca"); // ClipNode::Set { path: "scene.affect.bianca", .. }
    assert_eq!(
        clip_write_targets(&node, &snap, Some("bianca")),
        WriteTargets::Paths(["scene.affect.bianca".to_string()].into_iter().collect())
    );
}

#[test]
fn write_targets_unknown_directive_is_coarse() {
    let snap = lute_manifest::core::load_core_snapshot();
    let node = directive_clip("nosuchdir"); // unknown tag
    assert_eq!(clip_write_targets(&node, &snap, Some("cam")), WriteTargets::Coarse("cam".into()));
}

#[test]
fn write_targets_effectless_directive_is_none() {
    // core ::vfx has no effects.writes -> None (provably writes nothing)
    let snap = lute_manifest::core::load_core_snapshot();
    let node = directive_clip("vfx");
    assert_eq!(clip_write_targets(&node, &snap, Some("x")), WriteTargets::None);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cargo test -p lute-check --lib write_targets`
Expected: FAIL — `clip_write_targets`/`WriteTargets` do not exist.

- [ ] **Step 3: Implement `WriteTargets` + `clip_write_targets`** per the rules above (pure, total; no panics on missing attrs/effects).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p lute-check --lib write_targets`
Expected: PASS (4/4).

- [ ] **Step 5: fmt + clippy + commit**

Run: `cargo fmt -p lute-check && cargo clippy -p lute-check --all-targets -- -D warnings`
```bash
git add crates/lute-check/src/timeline.rs
git commit -m "feat(check): pure clip_write_targets resolver for timeline write-conflict (dsl §11.4)"
```

## Task B1.2: cross-track conflict on target overlap + thread snapshot

**Files:**
- Modify: `crates/lute-check/src/timeline.rs` (`resolve_timeline` signature + the `Placed` struct + the cross-track pass; module doc note)
- Modify: `crates/lute-check/src/check.rs:~350` (pass `self.snapshot`)
- Test: `crates/lute-check/src/timeline.rs` tests

**Interfaces:**
- Changes: `pub fn resolve_timeline(tl: &Timeline, ctx: &Ctx, snapshot: &CapabilitySnapshot) -> (ResolvedTimeline, Vec<Diagnostic>)` (was `(tl, _ctx)`). `Placed` gains `targets: WriteTargets` (computed via `clip_write_targets(&clip.node, snapshot, subject.as_deref())`), keeps `at/end/key/span`; the `subject` field is removed (subsumed by `targets`).
- Conflict rule (replace the current `subject_of`-based pass, timeline.rs:181-202): for each pair of placed clips `a`, `b` with `b` after `a`, DIFFERENT track (`a.key != b.key`), and overlapping intervals (`a.at < b.end && b.at < a.end`): they conflict iff `targets_overlap(&a.targets, &b.targets)` where:
  ```rust
  fn targets_overlap(a: &WriteTargets, b: &WriteTargets) -> Option<String> {
      // None never conflicts.
      let a_set = targets_as_set(a)?; // Paths -> the set; Coarse(s) -> {s}; None -> None
      let b_set = targets_as_set(b)?;
      // path-prefix overlap: equal, or one is a dotted-boundary prefix of the other.
      for x in &a_set {
          for y in &b_set {
              if x == y
                  || y.strip_prefix(x).is_some_and(|r| r.starts_with('.'))
                  || x.strip_prefix(y).is_some_and(|r| r.starts_with('.'))
              { return Some(x.clone().min(y.clone())); } // deterministic: report the shorter/lower
          }
      }
      None
  }
  ```
  Emit `E-WRITE-CONFLICT` (Severity::Error, Layer::Staging, span `b.span`) with message `format!("cross-track write conflict on `{target}` at overlapping times")`.
- Module doc (timeline.rs:33-42) "scope/limitation" note is replaced by a short description of the writes[]-based model + the `Coarse` fallback.

- [ ] **Step 1: Write the failing tests** (build two-track timelines; reuse `snapshot_with_writer`):

```rust
#[test]
fn no_conflict_when_different_properties() {
    // track A: ::writer key="k" writes scene.box.k.a ; track B: a ::set to scene.box.k.b
    // overlapping times, SAME subject box, DIFFERENT property -> NO E-WRITE-CONFLICT
    let (tl, snap) = two_writers_diff_prop();
    let (_t, diags) = resolve_timeline(&tl, &Ctx::default(), &snap);
    assert!(!diags.iter().any(|d| d.code == "E-WRITE-CONFLICT"),
        "different properties must not conflict; got {:?}", codes(&diags));
}

#[test]
fn conflict_when_same_target() {
    // both clips write scene.box.k.a at overlapping times, different tracks -> E-WRITE-CONFLICT
    let (tl, snap) = two_writers_same_prop();
    let (_t, diags) = resolve_timeline(&tl, &Ctx::default(), &snap);
    assert!(diags.iter().any(|d| d.code == "E-WRITE-CONFLICT"));
}

#[test]
fn conflict_when_subject_prefixes_property() {
    // clip A (unknown directive) Coarse("scene.box.k") vs clip B writes scene.box.k.a -> conflict (prefix)
    // (only if the coarse subject IS the state prefix; construct via a ::set path subject)
    let (tl, snap) = coarse_vs_precise();
    let (_t, diags) = resolve_timeline(&tl, &Ctx::default(), &snap);
    assert!(diags.iter().any(|d| d.code == "E-WRITE-CONFLICT"));
}

#[test]
fn effectless_directives_never_conflict() {
    // two ::vfx clips (no writes) on different tracks, overlapping -> NO conflict
    let (tl, snap) = two_effectless();
    let (_t, diags) = resolve_timeline(&tl, &Ctx::default(), &snap);
    assert!(!diags.iter().any(|d| d.code == "E-WRITE-CONFLICT"));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p lute-check --lib timeline`
Expected: FAIL — `resolve_timeline` arity (needs `snapshot`) / different-property still conflicts under the old subject rule.

- [ ] **Step 3: Implement** — change the signature, compute `targets` per placed clip, replace the conflict pass with `targets_overlap`, update the call site in `check.rs` to `resolve_timeline(tl, ctx, self.snapshot)`, update the existing timeline tests to pass a snapshot (`&lute_manifest::core::load_core_snapshot()` where they don't need writers). Update the module doc note.

- [ ] **Step 4: Run tests + cross-crate build**

Run: `cargo test -p lute-check --lib timeline && cargo build -p lute-check -p lute-lsp -p lute-cli`
Expected: PASS; build clean (grep `resolve_timeline(` first — only `check.rs` should call it).

- [ ] **Step 5: fmt + clippy + commit**

Run: `cargo fmt -p lute-check && cargo clippy -p lute-check --all-targets -- -D warnings`
```bash
git add crates/lute-check/src/timeline.rs crates/lute-check/src/check.rs
git commit -m "feat(check): property-level E-WRITE-CONFLICT via resolved directive writes[] (dsl §11.4)"
```

- [ ] **Step 6: regression** — `cargo test -p lute-check` fully green (reconcile any existing timeline golden whose expectation legitimately changed — a previously-flagged same-subject-different-property case is now correctly clean; document each change in the commit body). Confirm `lute-lsp` divergence tests still pass: `cargo test -p lute-lsp`.

---

# Phase B2 — E-REF-TYPE (`@ref` type-context match) — DESIGN-FIRST

## Task B2.1: design the expected-type model (types + doc note; DESIGN-REVIEW GATE before B2.2)

**Files:**
- Modify: `crates/lute-check/src/ctx.rs` (`ExpectedType` enum + `Ctx.def_types` field + a doc-comment design note)
- Test: none yet (design task — the reviewer + human gate the design). Must COMPILE.

**Interfaces:**
- Produces (compile-only; no behavior wired):
  ```rust
  /// The statically-known expected type of a CEL slot's value, when derivable.
  /// `None`-equivalent contexts are represented by simply not setting it.
  #[derive(Clone, Debug, PartialEq)]
  pub enum ExpectedType {
      /// A boolean guard/condition (`<when test>`, `<match>`-arm test): expects bool.
      Bool,
      /// A concrete manifest type (a `::set` RHS = the target path's declared type;
      /// a directive attr `@ref` = the attr's declared type; a `<match on>` subject
      /// = the subject path's declared type).
      Ty(lute_manifest::types::Type),
  }
  ```
  and on `Ctx`:
  ```rust
  /// def name -> the manifest `Type` the def PRODUCES (author inline `defs:` typed
  /// via parse_meta, plus plugin `DefDecl.ty`). Empty until B2.2 populates it.
  pub def_types: std::collections::BTreeMap<String, lute_manifest::types::Type>,
  ```
- A doc-comment design note in `ctx.rs` MUST enumerate, per `CelKind`, whether an expected type is statically known and how it is derived:
  - `Condition` ⇒ `ExpectedType::Bool` (always).
  - `SetExpr` ⇒ `ExpectedType::Ty(target_path_type)` when the `::set` target path's type is resolvable from the `state:` schema / snapshot; else unknown.
  - `AttrValue` (a `@ref` directive attr) ⇒ `ExpectedType::Ty(attr_declared_type)` when the owning directive+attr is known; else unknown. (NOTE the threading cost: the CEL slot does not currently carry its owning directive/attr — B2.2 must supply the expected type at the call site in `check.rs`, not inside `check_cel_slot`.)
  - `MatchSubject` ⇒ `ExpectedType::Ty(subject_path_type)` when the subject is a single state path with a known type; else unknown (a compound expression has no single expected type).
  - The compatibility relation (used in B2.2): two `Type`s are compatible iff equal, or a documented widening (e.g. any `Enum` ⊆ `Str`? — DECIDE in the note; default: exact match only, `EnumFromOption`/`Enum` treated as `Str`-compatible only if the note says so). Keep it conservative: **emit only on a clear mismatch** (e.g. def produces `Number`, slot expects `Bool`).

- [ ] **Step 1: Add the `ExpectedType` enum + `Ctx.def_types` field + the design-note doc comment.** No behavior. `Ctx` still derives `Default` (BTreeMap defaults empty).

- [ ] **Step 2: Verify it compiles**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cargo build -p lute-check`
Expected: clean (additive field; `Ctx { .. }` literals unaffected — grep `Ctx {` to confirm all use `..Default::default()` or update them).

- [ ] **Step 3: DESIGN-REVIEW GATE.** Do NOT proceed to B2.2. The controller dispatches a `reviewer` on the design note + types (is the model sound, bounded, false-positive-safe — only flag when BOTH types known?) AND surfaces the design to the human for approval/scoping (the human chose "proper direction, minimize drift"). If the review finds the model too broad, scope B2.2 to `SetExpr`-RHS + `Condition` only and defer `AttrValue`/`MatchSubject`.

- [ ] **Step 4: Commit the design**

Run: `cargo fmt -p lute-check && cargo clippy -p lute-check --all-targets -- -D warnings`
```bash
git add crates/lute-check/src/ctx.rs
git commit -m "design(check): ExpectedType model + Ctx.def_types for E-REF-TYPE (dsl §8)"
```

## Task B2.2: wire def types + expected types + emit E-REF-TYPE

> Execute ONLY after the B2.1 design gate approves (with whatever scope it approved).

**Files:**
- Modify: `crates/lute-check/src/check.rs` (build `def_types` from `parse_meta` inline defs + `snapshot.defs` `DefDecl.ty`; set the expected type per slot at each `check_cel_slot` call site per the approved contexts)
- Modify: `crates/lute-check/src/cel_resolve.rs` (accept an `expected: Option<ExpectedType>`; emit `E-REF-TYPE` when a `@ref`'s `ctx.def_types[name]` is known AND incompatible; remove the deferral NOTE at lines 45-53)
- Test: `crates/lute-check/src/cel_resolve.rs` tests

**Interfaces:**
- Consumes: `Ctx.def_types`, `ExpectedType` (B2.1). `check_cel_slot` signature gains the expected type: `pub fn check_cel_slot(slot: &CelSlot, arena: &CelArena, ctx: &Ctx, expected: Option<&ExpectedType>) -> Vec<Diagnostic>` (update all call sites in `check.rs`).
- Behavior: in the `@ref` branch (cel_resolve.rs:45), after the `E-UNDECLARED-REF` check, if `expected` is `Some` and `ctx.def_types.get(&r.name)` is `Some(produced)` and `!compatible(produced, expected)` ⇒ push `E-REF-TYPE` (Severity::Error, Layer::Cel, span = the `@ref` span) with a message naming the def, produced type, and expected type. Only when BOTH are known (no false positives on unknown).
- New code: `E-REF-TYPE`.

- [ ] **Step 1: Write the failing tests** (build a `Ctx` with `def_types = { "num": Type::Number }` and an inline def; a `<when test="@num">` slot expects `Bool`):

```rust
#[test]
fn ref_type_mismatch_flags() {
    let ctx = ctx_with_def("num", Type::Number);
    let slot = cel_slot(CelKind::Condition, "@num"); // expects Bool
    let diags = check_cel_slot(&slot, &arena_for(&slot), &ctx, Some(&ExpectedType::Bool));
    assert!(diags.iter().any(|d| d.code == "E-REF-TYPE"));
}

#[test]
fn ref_type_compatible_is_clean() {
    let ctx = ctx_with_def("flag", Type::Bool);
    let slot = cel_slot(CelKind::Condition, "@flag");
    let diags = check_cel_slot(&slot, &arena_for(&slot), &ctx, Some(&ExpectedType::Bool));
    assert!(!diags.iter().any(|d| d.code == "E-REF-TYPE"));
}

#[test]
fn ref_type_unknown_expected_no_false_positive() {
    let ctx = ctx_with_def("num", Type::Number);
    let slot = cel_slot(CelKind::Condition, "@num");
    let diags = check_cel_slot(&slot, &arena_for(&slot), &ctx, None); // expected unknown
    assert!(!diags.iter().any(|d| d.code == "E-REF-TYPE"));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p lute-check --lib ref_type`
Expected: FAIL — `check_cel_slot` arity / `E-REF-TYPE` not emitted.

- [ ] **Step 3: Implement** — add the `expected` param + the `compatible` helper (per the approved B2.1 note; conservative), emit `E-REF-TYPE`; in `check.rs` build `def_types` and pass the per-slot expected type for the approved contexts (at minimum `Condition ⇒ Bool` and `SetExpr ⇒ target-path type`); update ALL `check_cel_slot` call sites (grep).

- [ ] **Step 4: Run tests + cross-crate build**

Run: `cargo test -p lute-check --lib ref_type && cargo build -p lute-check -p lute-lsp -p lute-cli`
Expected: PASS; build clean.

- [ ] **Step 5: regression + fmt + clippy + commit**

Run: `cargo test -p lute-check && cargo fmt -p lute-check && cargo clippy -p lute-check --all-targets -- -D warnings`
Expected: no regression (existing `@ref`/cel tests green; no new false positives).
```bash
git add crates/lute-check/src/cel_resolve.rs crates/lute-check/src/check.rs
git commit -m "feat(check): E-REF-TYPE type-context match for @ref (dsl §8)"
```

---

# Final gate (after B1 + B2)

- [ ] `export PATH="$HOME/.cargo/bin:$PATH" && cargo test --workspace` — all green.
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` — 0.
- [ ] `cargo fmt --check` — clean.
- [ ] `(cd tree-sitter-lute && npx --yes tree-sitter-cli@latest test)` — 25/25 (B1/B2 don't touch the grammar or the capability hash, so the drift guard stays green).
- [ ] Acceptances unchanged: `./target/debug/lute check docs/examples/bianca-s01ep02.lute` exit 0; `date-minigame.lute` core-only exit 1; `--project docs/examples/idola-project` exit 0; `idola-portrait.lute --project` exit 0.
- [ ] Both git trees clean; whole-branch review (most-capable) → ready to merge; ff-merge to `main`.

## Self-Review

- **Spec coverage:** B1 → §11.4 write-conflict (rebuilt on directive `effects.writes[]`, decoupled from deferred `property=` tracks). B2 → §8 `@ref` typing (design-first, review-gated). Both map to tasks.
- **Drift minimization (the human's explicit ask):** B1 replaces a heuristic that only fired for a spec-deferred feature with a model grounded in the spec's directive-effects — narrowing false positives, not adding a parallel notion. B2 reuses the existing `cel-parser` AST + `scan_refs` + `Ctx`; no runtime eval; emits only on a clear, both-sides-known mismatch (no speculative flags).
- **Placeholder scan:** every code step shows real types/signatures (`WriteTargets`, `clip_write_targets`, `targets_overlap`, `ExpectedType`, `def_types`, `check_cel_slot` new arity) and real test bodies. No TBD.
- **Type consistency:** `resolve_timeline(tl, ctx, snapshot)` and `check_cel_slot(slot, arena, ctx, expected)` signatures are stated once and reused at every call site (check.rs). `WriteTargets`/`ExpectedType` names are stable across tasks.
- **Independence:** B1 and B2 are independent (different files/passes); either could ship alone. B2.2 is gated on B2.1's design approval.
- **Risk:** B1's regression step explicitly reconciles existing timeline goldens (the false-positive removal legitimately changes some expectations). B2's design-first gate de-risks the underspecified model; if too broad, scope to `SetExpr`+`Condition`.
