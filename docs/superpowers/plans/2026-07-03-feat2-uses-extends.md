# FEAT-2: `extends:` Schema Composition + Named Composition Policy

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development.
> **Design of record (autonomous decision — user reviews at showcase):** this plan defines `extends:` semantics that the spec left deferred. See "Design" below.

**Goal:** Add `extends:` to schema/scene composition (dsl §9.2, spec Appendix B "additive extends") so a schema can build on a base schema and REFINE it (override a def or a state default), unlike `uses:` peer imports which forbid duplicates. Formalize the previously-scattered merge rules into ONE named `CompositionPolicy` (audit rec #3).

**Architecture:** Extend `resolve_imports` (lute-check/src/schema_import.rs) to resolve two edge kinds per document — `uses:` (peer, dup = error, current behavior) and `extends:` (base, override-allowed). Precedence, low → high: `extends` bases (recursively; a more-derived base overrides a less-derived one) < the document's own imported decls. `uses` peers remain same-precedence (dup = error). A state override that changes the declared **type** is `E-EXTENDS-STATE-TYPE` (protects persisted state); a state default override or any def override is allowed.

**Tech Stack:** Rust; `lute-check` (`schema_import.rs`, `meta.rs`), `lute-syntax` parse; spec `docs/proposals/scenario-dsl/0.0.1.md`.

## Global Constraints
- `export PATH="$HOME/.cargo/bin:$PATH"` every shell. Worktree `/Users/journey/Workspace/lute/.worktrees/lute-lsp-rust` (branch `feat/lute-lsp-rust`); ABSOLUTE worktree paths; cargo/git cwd = worktree.
- Per-task hygiene BEFORE commit: `cargo fmt` + `cargo clippy --all-targets -- -D warnings` on touched crates.
- CROSS-CRATE: `resolve_imports` signature changes (adds `extends`) → grep call sites (`crates/lute-cli/src/main.rs`, `crates/lute-lsp/src/backend.rs` both call it) + `cargo build -p lute-check -p lute-lsp -p lute-cli`. Keep `divergence.rs` green.
- never-panic (all I/O/parse/cycle → diagnostic), determinism (BTreeMap/sorted). capabilityVersion untouched.
- New codes: `E-EXTENDS-STATE-TYPE`. Reuse `E-USES-{CYCLE,NOT-FOUND,PARSE}` for extends edges (cycle/missing/parse are identical concerns); keep `E-USES-DUP-{STATE,DEF}` for PEER (uses) collisions only.

## Design (the composition policy — decision of record)
Two composition edges in a document's frontmatter (schema or scene):
- **`uses: [P...]`** — PEER imports. The union of all peers' decls; a name declared by two different peers (or a peer and a same-level sibling) is an error (`E-USES-DUP-STATE`/`E-USES-DUP-DEF`). Unchanged from today.
- **`extends: [B...]`** — BASE schemas this document refines. Bases are a LOWER-precedence layer: if the extending document (or its `uses:` peers) declares a name also declared by a base, the extending declaration OVERRIDES the base's — no error. Among multiple bases, a base declared closer to the extending doc overrides one further away; two unrelated bases declaring the same name is a cross-base conflict = `E-USES-DUP-*` (same as peers).

Precedence within a single resolved document D, low → high:
1. D's `extends` bases, recursively (a base's own extends are even lower).
2. D's `uses` peers (peer-dup errors among themselves).
3. D's own inline `state:`/`defs:` (merged by `check.rs`, already highest — inline/imported/plugin precedence unchanged there).

Override rules on a name collision where the LOWER layer is an `extends` base:
- **def:** higher layer replaces the base def entirely (defs are pure CEL macros). No diagnostic.
- **state:** higher layer overrides the base decl. If the override's `type` differs from the base's `type` → `E-EXTENDS-STATE-TYPE` (persisted state must keep a stable type); a `default`-only refinement (same type) is allowed silently. The higher decl wins in the merged schema either way (so downstream checks use the refined decl).

Cycles across `extends` (or mixed uses/extends) → `E-USES-CYCLE` (chain printed). Missing/parse errors on a base → `E-USES-NOT-FOUND`/`E-USES-PARSE`.

## File Structure
- `crates/lute-check/src/meta.rs` — parse `extends:` into `TypedMeta.extends: Vec<String>` (mirror `uses`).
- `crates/lute-check/src/schema_import.rs` — resolve `extends` edges with override; the `CompositionPolicy` logic (layered merge + override rules) lives here; `resolve_imports` gains an `extends` param.
- `crates/lute-cli/src/main.rs`, `crates/lute-lsp/src/backend.rs` — pass `meta0.extends` into `resolve_imports`.
- `docs/proposals/scenario-dsl/0.0.1.md` — document `extends:` (§9.2) + remove the Appendix B "additive extends ... deferred" clause.
- Fixture: `docs/examples/extends-*.lute` (base schema + extending schema + scene).

---

## Task 1: Parse `extends:` frontmatter

**Files:** Modify `crates/lute-check/src/meta.rs` (+ tests).
**Interfaces:** `TypedMeta.extends: Vec<String>` populated exactly like `TypedMeta.uses` (accept a scalar string or a list). Add `"extends"` to the core meta keys so it isn't "unknown."

- [ ] Step 1: Add a failing test in `meta.rs` tests: a frontmatter `extends: base.lute` (and `extends: [a.lute, b.lute]`) yields `TypedMeta.extends == ["base.lute"]` / `["a.lute","b.lute"]`, no unknown-key diagnostic.
- [ ] Step 2: Run `cargo test -p lute-check --lib meta` — confirm fail.
- [ ] Step 3: Add `pub extends: Vec<String>` to `TypedMeta`; parse it in `parse_meta_kind` exactly as `uses` is parsed (find the `uses` lift and mirror it); add `"extends"` to the CORE_KEYS list so it's recognized.
- [ ] Step 4: `cargo test -p lute-check --lib meta` — pass. fmt+clippy.
- [ ] Step 5: Commit: `git commit -m "feat(check): parse extends: frontmatter (dsl §9.2)"`

## Task 2: Resolve `extends` with override in `schema_import.rs`

**Files:** Modify `crates/lute-check/src/schema_import.rs` (+ tests); update `resolve_imports` call sites in cli/lsp.
**Interfaces:**
- `resolve_imports(base_dir: &Path, uses: &[String], extends: &[String], at: Span) -> SchemaImports` (new `extends` param).
- Internally, track per-name provenance layer so a base entry can be overridden without a dup error, while peer/peer stays a dup error. Suggested: extend `Acc` entries to record whether the current winner came from an `extends` base (overridable) vs a `uses` peer (dup-guarded), plus its declared type for the state type-change check.

- [ ] Step 1: Write failing tests (in schema_import.rs tests or a new tests file) using the existing temp-dir helper pattern (see `crates/lute-check/tests/uses_import.rs` `unique_dir`/`write_lute`):
  - `extends_overrides_base_def`: base declares `def helped: {type: bool, cel: "false"}`; child `extends: base.lute` re-declares `helped: {type: bool, cel: "true"}`; resolving the child yields the CHILD's def (cel "true"), NO E-USES-DUP-DEF.
  - `extends_state_default_override_ok`: base `run.gold: {type: number, default: 0}`; child extends + `run.gold: {type: number, default: 5}` → merged decl has default 5, no error.
  - `extends_state_type_change_errors`: base `run.gold: {type: number}`; child extends + `run.gold: {type: string}` → `E-EXTENDS-STATE-TYPE`.
  - `uses_peer_dup_still_errors`: two `uses:` peers declaring the same def → `E-USES-DUP-DEF` (regression, unchanged).
  - `extends_cycle_errors`: A extends B, B extends A → `E-USES-CYCLE`.
- [ ] Step 2: Run — confirm fail (compile: new `extends` param / behavior).
- [ ] Step 3: Implement. Extend `resolve_into` (or add an extends-aware layered resolve) so that: a doc's `extends` bases are resolved into the base layer FIRST; then the doc's `uses` peers; a name already present from a BASE layer is OVERRIDDEN (replace + record new provenance) rather than dup-flagged; a name present from a PEER (non-base) collides → dup error; on a state override, compare declared types and push `E-EXTENDS-STATE-TYPE` on mismatch (but still take the overriding decl). Recurse each schema doc's OWN `extends` + `uses`. Keep cycle/not-found/parse handling (reuse E-USES-*). Preserve the diamond dedup (one file = one identity).
- [ ] Step 4: Update call sites: `crates/lute-cli/src/main.rs` and `crates/lute-lsp/src/backend.rs` (both `analyze` and `imports_for`) pass `&meta0.extends` into `resolve_imports`. `cargo build -p lute-check -p lute-lsp -p lute-cli`.
- [ ] Step 5: `cargo test -p lute-check`; `cargo test -p lute-lsp --test divergence`. All green. fmt+clippy the 3 crates.
- [ ] Step 6: Commit: `git commit -m "feat(check): extends: schema composition with override (E-EXTENDS-STATE-TYPE) (dsl §9.2)"`

## Task 3: Fixture + spec

**Files:** Create `docs/examples/extends-base.lute` (base schema), `docs/examples/extends-child.lute` (schema that `extends:` the base + refines a default), `docs/examples/extends-scene.lute` (scene that `uses:` the child); modify `docs/proposals/scenario-dsl/0.0.1.md`.

- [ ] Step 1: Author the three fixture files (schema docs are `.lute` frontmatter, schema-mode; see `docs/examples/state.schema.lute` + `carry-ep.lute` for the shape). The scene must check exit 0 and demonstrably use a def/state that came from the base THROUGH the extending child (override applied). `cargo build -p lute-cli && ./target/debug/lute check docs/examples/extends-scene.lute; echo exit=$?` → exit 0.
- [ ] Step 2: Spec: in §9.2 document `extends:` (base composition + override + `E-EXTENDS-STATE-TYPE`) alongside `uses:`; in Appendix B remove/trim the "additive extends ... deferred" clause (project-resolved schema ids stay deferred).
- [ ] Step 3: fmt+clippy; commit: `git commit -m "docs(examples,spec): extends: composition fixture + §9.2 (dsl §9.2)"`

## Verification (controller, after review)
```
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
cargo test -p lute-manifest --test tree_sitter_stamp
./target/debug/lute check docs/examples/extends-scene.lute   # exit 0
./target/debug/lute check docs/examples/carry-ep.lute        # exit 0 (uses: regression)
```

## Self-Review
- `extends` overrides bases (def + state-default); `E-EXTENDS-STATE-TYPE` on type change; `uses` peer-dup unchanged; cycle/not-found/parse reuse E-USES-*.
- Both CLI + LSP pass `extends` → no divergence.
- Composition precedence documented in one place (the Design section + code comments).
