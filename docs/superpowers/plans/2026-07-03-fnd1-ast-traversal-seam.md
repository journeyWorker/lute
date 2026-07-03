# FND-1: Unified AST Traversal / CEL-Slot Seam — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development.

**Goal:** Replace the duplicated CEL-slot enumeration walkers (`lute-cel/fill.rs` and `lute-lsp/features/mod.rs::collect_slots`, plus the fill test's parallel collector) with ONE shared traversal API in `lute-syntax`, so adding a slot-bearing AST node becomes a single-site change instead of "hunt every recursive match." Behavior-preserving.

**Architecture:** New `crates/lute-syntax/src/walk.rs` exposing closure-based free functions `for_each_cel_slot(&Document, &mut impl FnMut(&CelSlot))` and `for_each_cel_slot_mut(&mut Document, &mut impl FnMut(&mut CelSlot))`, enumerating every `CelSlot` in the EXACT pre-order `fill.rs` currently uses (so `StableId` assignment — and thus determinism — is byte-identical). `fill.rs` and the LSP `all_slots` become thin closures over the shared walk.

**Tech Stack:** Rust, cargo workspace. `lute-syntax` owns the AST and is depended on by `lute-cel` + `lute-check` + `lute-lsp`, so the seam lives there.

## Global Constraints
- `export PATH="$HOME/.cargo/bin:$PATH"` every shell. Worktree `/Users/journey/Workspace/lute/.worktrees/lute-lsp-rust` (branch `feat/lute-lsp-rust`); ABSOLUTE worktree paths for edits; cargo/git cwd = worktree.
- Per-task hygiene BEFORE commit: `cargo fmt -p <crate>` + `cargo clippy -p <crate> --all-targets -- -D warnings` on every touched crate.
- CROSS-CRATE: `lute-syntax` public API addition → `cargo build -p lute-cel -p lute-check -p lute-lsp`. Keep `lute-lsp/tests/divergence.rs` green.
- **Behavior-preserving:** the slot pre-order MUST be identical to today's `fill.rs` order (StableId 1,2,3… unchanged). All workspace tests + goldens + examples MUST stay green. capabilityVersion/tree-sitter untouched.
- never-panic, determinism (output already sorted downstream).

## Canonical slot pre-order (from `lute-cel/src/fill.rs`, MUST be preserved exactly)
Per shot body, per node in source order:
- `Line` → each `AttrValue::Ref` slot in `attrs` order
- `Directive` → each `AttrValue::Ref` slot in `attrs` order
- `Set` → `expr`
- `Branch` → `attrs` refs; then per `choice`: `choice.when` (if any), `choice.attrs` refs, then recurse `choice.body`
- `Match` → `subject`; then per `arm`: `When{test, body}` → `test` then recurse `body`; `Otherwise{body}` → recurse `body`
- `Timeline` → `duration` (if any); then per `track`, per `clip`: `ClipNode::Directive` → attr refs; `ClipNode::Set` → `expr`

(Attr refs: only `AttrValue::Ref(slot)`; bare/other attr values are not slots.)

---

## Task 1: Add the shared traversal seam + migrate both consumers

**Files:**
- Create: `crates/lute-syntax/src/walk.rs`
- Modify: `crates/lute-syntax/src/lib.rs` (add `pub mod walk;`)
- Modify: `crates/lute-cel/src/fill.rs` (use `for_each_cel_slot_mut`; delete the private `Walk` struct + its methods)
- Modify: `crates/lute-lsp/src/features/mod.rs` (rewrite `all_slots`; delete `collect_slots`/`attr_slots`)
- Test: unit tests in `walk.rs`; keep `fill.rs` + lsp tests green.

**Interfaces:**
- Produces:
  ```rust
  // crates/lute-syntax/src/walk.rs
  use crate::ast::{Arm, AttrValue, ClipNode, Document, Node, CelSlot, ...};
  pub fn for_each_cel_slot(doc: &Document, f: &mut impl FnMut(&CelSlot)) { ... }
  pub fn for_each_cel_slot_mut(doc: &mut Document, f: &mut impl FnMut(&mut CelSlot)) { ... }
  ```
  Both walk the canonical pre-order above. Implement the shared recursion once for each mutability (a small internal `node`/`body`/`branch`/`match`/`timeline`/`attrs` helper set per mutability, mirroring `fill.rs`'s current `Walk`). No `unsafe`, no macro trickery required — duplicating the two mutability variants is acceptable and clearest.

- [ ] **Step 1: Write `walk.rs` with both functions + unit tests**

Implement `for_each_cel_slot` / `for_each_cel_slot_mut` following the canonical pre-order (copy the traversal structure verbatim from `fill.rs`'s `Walk` methods — `body`/`node`/`branch`/`match_node`/`timeline`/`attrs` — but calling `f(slot)` instead of fill logic). Add unit tests over a hand-built `Document` with nested Branch(choice.when+attrs+body), Match(subject + When test/body + Otherwise body), Timeline(duration + track clips Directive attrs + Set), and Line/Directive attr refs:
```rust
#[test]
fn for_each_cel_slot_visits_canonical_preorder() {
    let doc = /* build a rich doc; give each slot a distinct raw like "s0".."sN" */;
    let mut seen = Vec::new();
    for_each_cel_slot(&doc, &mut |s| seen.push(s.raw.clone()));
    assert_eq!(seen, vec!["s0","s1", /* ...in the exact expected pre-order... */]);
}
#[test]
fn for_each_cel_slot_mut_visits_same_order_and_can_mutate() {
    let mut doc = /* same */;
    let mut n = 0u64;
    for_each_cel_slot_mut(&mut doc, &mut |s| { s.id = lute_core_span::StableId({n+=1; n}); });
    // assert ids assigned 1..=count in the same order for_each_cel_slot yields
}
```
Build the test `Document` directly via the AST structs (see `crates/lute-syntax/src/ast.rs` for field shapes; `CelSlot` has a constructor/fields — mirror how `fill.rs` tests build slots).

- [ ] **Step 2: Run walk tests**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cd /Users/journey/Workspace/lute/.worktrees/lute-lsp-rust && cargo test -p lute-syntax walk`
Expected: pass.

- [ ] **Step 3: Migrate `lute-cel/fill.rs`**

Rewrite `fill_document` to drive the shared walk; delete the private `Walk` struct and all its methods (`slot`/`attrs`/`body`/`node`/`branch`/`match_node`/`timeline`). The per-slot action (assign StableId, skip empty, parse) moves into the closure:
```rust
pub fn fill_document(arena: &mut CelArena, doc: &mut Document) -> Vec<CelParseError> {
    let mut next_id: u64 = 1;
    let mut errors = Vec::new();
    lute_syntax::walk::for_each_cel_slot_mut(doc, &mut |slot| {
        slot.id = StableId(next_id);
        next_id += 1;
        if slot.raw.trim().is_empty() { return; }
        match parse_slot(arena, &slot.raw, slot.span.byte_start) {
            Ok(handle) => slot.ast = Some(handle),
            Err(e) => errors.push(e),
        }
    });
    errors
}
```
Keep the module doc comment (it documents the enumeration) but note the enumeration now lives in `lute_syntax::walk`. Keep `fill.rs`'s existing tests (they assert ids/parse results) — they must still pass unchanged, proving pre-order preservation. If the fill test has a private parallel slot-collector, replace its body with `lute_syntax::walk::for_each_cel_slot` (it now cross-checks the shared walk rather than a hand copy).

- [ ] **Step 4: Migrate `lute-lsp/features/mod.rs`**

Rewrite `all_slots` and delete `collect_slots` + `attr_slots`:
```rust
pub(crate) fn all_slots(doc: &Document) -> Vec<&CelSlot> {
    let mut out = Vec::new();
    lute_syntax::walk::for_each_cel_slot(doc, &mut |s| out.push(s));
    out
}
```
Fix imports (drop now-unused `Arm`/`ClipNode`/etc. if they were only used by the deleted collectors; keep whatever the rest of the file needs). Leave `byte_span` and all other helpers untouched.

- [ ] **Step 5: Full build + tests + hygiene**

Run:
```
cargo build -p lute-cel -p lute-check -p lute-lsp
cargo test -p lute-syntax -p lute-cel -p lute-lsp
cargo test -p lute-lsp --test divergence
cargo fmt -p lute-syntax -p lute-cel -p lute-lsp
cargo clippy -p lute-syntax -p lute-cel -p lute-lsp --all-targets -- -D warnings
```
Expected: all green, 0 warnings. (The controller runs the full-workspace gate + examples/goldens.)

- [ ] **Step 6: Commit**

```bash
cd /Users/journey/Workspace/lute/.worktrees/lute-lsp-rust
git add crates/lute-syntax/src/walk.rs crates/lute-syntax/src/lib.rs crates/lute-cel/src/fill.rs crates/lute-lsp/src/features/mod.rs
git commit -m "refactor(syntax): shared for_each_cel_slot traversal seam; fill+lsp use it (FND-1)"
```

## Verification (controller, after review)
```
cargo test --workspace   # all green; StableId pre-order unchanged (fill/golden/examples prove it)
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
cargo test -p lute-manifest --test tree_sitter_stamp   # unaffected
```

## Self-Review
- Pre-order identical to old `fill.rs` (goldens/examples/fill tests are the proof).
- No duplicated slot collector remains (fill `Walk`, lsp `collect_slots`/`attr_slots` deleted).
- API is closure-based free functions (audit rec: no OO visitor); lives in `lute-syntax`.
