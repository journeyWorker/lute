# Lute `uses:` Schema Import Resolver (§9.2) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Make a scene's frontmatter `uses:` import a shared state-schema document (a `.lute` file whose frontmatter carries `state:`/`defs:`/nested `uses:`), so `run`/`user`/`app` reads/writes and shared `@ref` defs resolve against the imported schema instead of being falsely `E-UNDECLARED`/`E-UNDECLARED-REF`. Imports form a DAG with normative rules: cycle rejection, duplicate-`defs` rejection, path canonicalization (two paths to one file = one identity), and no scene override of an imported tier.

**Architecture:** File I/O + DAG resolution live in a NEW pure-ish resolver module `lute-check/src/schema_import.rs` (`resolve_imports`), NOT inside `check()` (which stays a pure fn over text + snapshot + inputs). Both surfaces (CLI `main.rs`, LSP `backend.rs::snapshot_for`) call `resolve_imports` with the scene's base directory and pass the result into `check()` via a new `CheckInput.imports: SchemaImports` field — the SAME pattern as `resolve_document_snapshot`, preserving no-divergence. `check()` merges the imported `state:`/`defs:` into its schema/def tables (flagging `E-STATE-REDECLARE` on a scene override of an imported path) and folds the resolver's `E-USES-*` diagnostics into its output.

**Tech Stack:** Rust (workspace, rustc 1.96.1). Reuses `lute_syntax::parse` (frontmatter peel) + `lute-check/src/meta.rs` (`parse_meta`, `StateSchema`, `StateDecl`, `Namespace`). `std::fs` for reads + `std::fs::canonicalize` for identity. `serde_yaml` (already a dep) for the `defs` `type:` extraction reuse. Plain `#[test]` + `tempfile`-free temp dirs via `std::env::temp_dir()` (or a fixture under `docs/examples/`).

## Global Constraints

- **rustup stable 1.96.1** via `~/.cargo/bin`. Every fresh shell: `export PATH="$HOME/.cargo/bin:$PATH"`. NEVER `brew install rust`.
- **Worktree authoritative.** Work in `/Users/journey/Workspace/lute/.worktrees/lute-lsp-rust` on `feat/lute-lsp-rust` (== `main` at plan authoring, HEAD `0a101d8`). **HARNESS QUIRK:** `write`/`edit` resolve RELATIVE paths against the MAIN workspace (`~/Workspace/lute`), NOT the worktree — always use ABSOLUTE worktree paths; after every commit verify `git status` clean in BOTH trees (`?? .worktrees/` in the main tree is expected).
- **TDD, tester-first.** Failing test first, confirm the exact failure, minimal impl, green. Own-crate `cargo test -p <crate>` during a task; full-workspace gate at plan end.
- **Format touched crates** (`cargo fmt -p <crate>`); keep `cargo fmt --check` clean. **Clippy `-D warnings`** per touched crate before each commit.
- **Cross-crate discipline:** adding `CheckInput.imports` breaks EVERY `CheckInput { .. }` literal (2 prod + ~11 test). U1 updates ALL of them (grep `CheckInput {`); after any signature/type change grep call sites + `cargo build -p lute-check -p lute-lsp -p lute-cli`.
- **No divergence** (inviolable): `resolve_imports` is the single resolver both surfaces call; all import diagnostics flow through `check()`'s one output. `lute-lsp/tests/divergence.rs` (incl. `divergence_holds_under_plugin_project`) MUST stay green; U4 adds a `uses:`-scene divergence case.
- **Determinism** (§3.2) + **never-panic**: `resolve_imports` is total — a missing/unreadable/malformed schema file, a cycle, a dup def, or a canonicalize failure yields a diagnostic, never a panic; all maps `BTreeMap`, all sets `BTreeSet`, import order canonicalized so the merged schema is deterministic.
- **Snapshot-is-SoT unaffected:** the state schema is *game content* (separate axis from the engine capability snapshot). `uses:` touches NO capability field → `capabilityVersion` UNCHANGED → tree-sitter drift guard stays green, NO re-stamp.

## Spec source-of-truth

- DSL §9.1 (tiers), §9.2 (`uses:` import — the normative DAG rules), §9.3 (decl form), §9.4 (definite assignment; maybe-unset for non-`scene` tiers), §8.1 (`defs`): `docs/proposals/scenario-dsl/0.0.1.md:325-405, 296-313`.
- Design + worked shape (schema doc = `.lute` frontmatter; `uses: ../state.schema.lute`): `docs/proposals/scenario-dsl/state-model-design.md:53-138`.

## Spec decisions (READ FIRST)

1. **Schema document = a `.lute` file** whose `---` frontmatter carries `state:` (run/user/app tiers + optionally scene.* — but see #4), `defs:`, and optional nested `uses:`. It is parsed with `lute_syntax::parse` (frontmatter peel) then a **schema-mode** `parse_meta` that SKIPS the scene-required-key check (`character`/`season`/`episode`) — a schema doc is not a scene. Its body (if any) is ignored by the resolver (only the frontmatter matters).
2. **`uses:` refs are relative file paths**, resolved against the IMPORTING document's directory (`base_dir.join(ref)`), then `std::fs::canonicalize`d for identity (v0.0.1 "start flat, relative paths"; project-resolved ids + `extends:` are deferred but the resolver is a DAG now). A canonicalize/read failure → `E-USES-NOT-FOUND` (message names the ref + importer).
3. **DAG rules (all → diagnostics, never panic):** cycle (a canonical path already on the import stack) → `E-USES-CYCLE` with the chain printed; duplicate `defs` name across two DISTINCT imported files → `E-USES-DUP-DEF`; duplicate `state` path across two DISTINCT imported files → `E-USES-DUP-STATE` (determinism guard; the spec names dup-defs, we extend the same rule to state paths to avoid nondeterministic last-wins); a file reached by two paths (diamond) is processed ONCE (canonical dedup) and is NOT a dup error; a schema doc that fails to parse → `E-USES-PARSE`.
4. **No scene override of an imported tier (§9.2):** if the scene's inline `state:` declares a path that the imported schema ALSO declares → `E-STATE-REDECLARE` (the imported decl wins; the inline one is ignored). Inline `scene.*` locals remain fully supported. **Deferred (documented):** the stricter "only `scene.*` MAY appear inline" rule is NOT enforced (inline `run/user/app` without a conflicting import stays allowed) — low-drift; a later lint can tighten it.
5. **Def merge precedence:** imported defs first, then inline defs override on a name collision (mirrors the existing `def_types` "inline overrides plugin" precedence in `check.rs`). Dup across imports is the `E-USES-DUP-DEF` error (#3); inline-vs-import collision is NOT an error (inline is scene-local override).
6. **All import diagnostics are `Layer::Content`** at the scene's frontmatter span (`doc.meta.span`), matching the existing `E-META-*`/`E-STATE-*` family.

## File Structure

- `crates/lute-check/src/schema_import.rs` — NEW. `SchemaImports { state, defs, diags }` + `resolve_imports(base_dir, uses) -> SchemaImports` (I/O + DAG). `pub mod schema_import;` + re-export `SchemaImports`, `resolve_imports` in `lib.rs`.
- `crates/lute-check/src/meta.rs` — add a schema-mode parse (`MetaKind::{Scene, Schema}` + `parse_meta_kind`; `parse_meta` delegates as `Scene`) that skips the required-key check for schema docs.
- `crates/lute-check/src/check.rs` — `CheckInput` gains `imports: SchemaImports`; `check()` merges imported state (redeclare check → `E-STATE-REDECLARE`) + imported defs into `schema`/`defs`/`def_types`, and folds `input.imports.diags` into the output.
- `crates/lute-cli/src/main.rs` + `crates/lute-lsp/src/backend.rs` — call `resolve_imports(scene_base_dir, &meta0.uses)` and pass into `CheckInput`.
- `docs/examples/` — a `state.schema.lute` + a scene `uses:`-ing it; `crates/lute-cli/tests/` acceptance + `lute-lsp/tests/divergence.rs` case.

---

# Task U1: `SchemaImports` + `CheckInput.imports` + pure merge in `check()`

**Files:**
- Create: `crates/lute-check/src/schema_import.rs` (the `SchemaImports` struct ONLY this task; `resolve_imports` is U2 — leave a `todo!()`-free stub-free module: define just the struct + `Default`).
- Modify: `crates/lute-check/src/lib.rs` (`pub mod schema_import;` + `pub use schema_import::SchemaImports;`).
- Modify: `crates/lute-check/src/check.rs` (`CheckInput.imports`; merge logic; append `imports.diags`).
- Modify: EVERY `CheckInput { .. }` literal — 2 prod (`crates/lute-cli/src/main.rs`, `crates/lute-lsp/src/backend.rs`) + all `crates/lute-check/tests/*.rs` + `crates/lute-lsp/tests/divergence.rs` (grep `CheckInput {`) — add `imports: SchemaImports::default()`.
- Test: `crates/lute-check/src/check.rs` `#[cfg(test)]` or a new `crates/lute-check/tests/uses_import.rs`.

**Interfaces:**
- Produces:
  ```rust
  // schema_import.rs
  use std::collections::BTreeMap;
  use lute_core_span::Diagnostic;
  use crate::meta::StateSchema;

  /// The resolved result of a scene's `uses:` imports: the merged imported state
  /// schema, the merged imported `defs` (untyped YAML values, like inline defs),
  /// and every `E-USES-*` diagnostic produced while resolving them.
  #[derive(Clone, Debug, Default)]
  pub struct SchemaImports {
      pub state: StateSchema,
      pub defs: BTreeMap<String, serde_yaml::Value>,
      pub diags: Vec<Diagnostic>,
  }
  ```
- Changes `CheckInput`: add `pub imports: SchemaImports,` (after `mode`).
- `check()` merge (in `check.rs`, where `schema` is built ~line 168 and `defs`/`def_types` ~line 179):
  - Build `schema` starting from `input.imports.state.decls` (imported), THEN insert inline `typed.state.decls`; on a key collision push `E-STATE-REDECLARE` (Severity::Error, Layer::Content, span `doc.meta.span`, message naming the path) and KEEP the imported decl (do not overwrite). (Then the existing branch-fold + directive-slot fold run on the merged schema unchanged.)
  - Build the `defs` name set from imported def names ∪ inline def names ∪ plugin `snapshot.defs` keys (extend the existing union). Build `def_types` from: plugin `snapshot.defs` (typed), THEN imported defs (extract `type:` via `serde_yaml::from_value::<Type>`), THEN inline defs (extract `type:`) — later overrides earlier, so inline > imported > plugin.
  - Append `input.imports.diags.clone()` to the diagnostic list (they dedupe/sort with the rest through the existing pipeline).

- [ ] **Step 1: failing tests** in a new `crates/lute-check/tests/uses_import.rs` (build a `CheckInput` manually with a populated `SchemaImports` — no file I/O; use a `check_codes(text, imports)` helper mirroring `directive_slots.rs::check_codes` but taking `imports: SchemaImports` and core snapshot):
  ```rust
  use lute_check::schema_import::SchemaImports;
  use lute_check::meta::{StateDecl, StateSchema, Namespace};
  use lute_manifest::types::Type;
  // helper builds CheckInput { text, uri:"t", snapshot: core, providers: default, mode: Author, imports }

  #[test]
  fn imported_run_path_resolves_no_undeclared() {
      // schema imports run.choseHelp: bool default false; scene <match on="run.choseHelp"> reads it
      let mut st = StateSchema::default();
      st.decls.insert("run.choseHelp".into(),
          StateDecl { ty: Type::Bool, default: Some(lute_manifest::types::Literal::Bool(false)), namespace: Namespace::Run });
      let imports = SchemaImports { state: st, defs: Default::default(), diags: Default::default() };
      let codes = check_codes(SCENE_READS_RUN, imports); // scene has NO inline run.choseHelp decl
      assert!(!codes.contains(&"E-UNDECLARED".to_string()), "imported path must resolve; got {codes:?}");
  }

  #[test]
  fn scene_override_of_imported_tier_flags_redeclare() {
      // imports run.x; scene inline ALSO declares run.x -> E-STATE-REDECLARE
      let mut st = StateSchema::default();
      st.decls.insert("run.x".into(), StateDecl { ty: Type::Bool, default: None, namespace: Namespace::Run });
      let imports = SchemaImports { state: st, defs: Default::default(), diags: Default::default() };
      let codes = check_codes(SCENE_REDECLARES_RUN_X, imports); // frontmatter state: { run.x: {...} }
      assert!(codes.contains(&"E-STATE-REDECLARE".to_string()), "got {codes:?}");
  }

  #[test]
  fn import_diags_are_surfaced() {
      let imports = SchemaImports { state: Default::default(), defs: Default::default(),
          diags: vec![/* a synthetic E-USES-CYCLE Diagnostic at span default */] };
      let codes = check_codes(MINIMAL_SCENE, imports);
      assert!(codes.contains(&"E-USES-CYCLE".to_string()));
  }
  ```
  (Craft `SCENE_READS_RUN` as a minimal valid scene whose only run-tier reference is `<match on="run.choseHelp"><when test="$ == true">…</when><otherwise>…</otherwise></match>`; `SCENE_REDECLARES_RUN_X` adds `state:\n  run.x: { type: bool }` to the frontmatter. Assert only on the target codes.)
- [ ] **Step 2: RED** — `export PATH="$HOME/.cargo/bin:$PATH" && cargo test -p lute-check --test uses_import` → FAIL (`SchemaImports`/`CheckInput.imports` missing).
- [ ] **Step 3: implement** the struct + field + merge + redeclare + diag append; add `SchemaImports::default()` to every `CheckInput` literal (grep to find all).
- [ ] **Step 4: GREEN + cross-crate build** — `cargo test -p lute-check --test uses_import` + `cargo build -p lute-check -p lute-lsp -p lute-cli` (all `CheckInput` sites updated).
- [ ] **Step 5: regression** — `cargo test -p lute-check` fully green (existing tests unaffected: empty `imports` is a no-op).
- [ ] **Step 6: fmt + clippy + commit**
  ```bash
  cargo fmt -p lute-check && cargo clippy -p lute-check --all-targets -- -D warnings
  git add -A && git commit -m "feat(check): SchemaImports + CheckInput.imports merge (dsl §9.2)"
  ```

# Task U2: `resolve_imports` resolver (I/O + DAG + cycle + dup)

**Files:**
- Modify: `crates/lute-check/src/schema_import.rs` (add `resolve_imports` + private DFS).
- Modify: `crates/lute-check/src/meta.rs` (add `MetaKind::{Scene, Schema}` + `pub fn parse_meta_kind(meta, snapshot, kind)`; `parse_meta` = `parse_meta_kind(.., Scene)`; Schema mode skips the `REQUIRED_KEYS` loop).
- Modify: `crates/lute-check/src/lib.rs` (`pub use schema_import::resolve_imports;`).
- Test: `crates/lute-check/tests/uses_import.rs` (temp-dir fixtures via `std::env::temp_dir().join(unique)`).

**Interfaces:**
- Produces:
  ```rust
  /// Resolve a scene's `uses:` imports into a merged schema. `base_dir` is the
  /// importing document's directory; each `uses` entry is a relative path.
  /// Total: any I/O/parse/cycle/dup failure yields a diagnostic in the result,
  /// never a panic. `at` is the scene frontmatter span used for every diagnostic.
  pub fn resolve_imports(base_dir: &std::path::Path, uses: &[String], at: lute_core_span::Span) -> SchemaImports;
  ```
- Algorithm (DFS with a canonical-path visited set + an on-stack set for cycles):
  - For each `ref` in `uses`: `let path = base_dir.join(ref);` `std::fs::canonicalize(&path)` → on `Err` push `E-USES-NOT-FOUND` (name `ref`) and continue.
  - If canonical path is on the current stack → `E-USES-CYCLE` (message prints the chain `a -> b -> a`), continue (do not recurse).
  - If already in `visited` (diamond) → skip (one identity), continue.
  - Insert into `visited` + push on stack. `std::fs::read_to_string` → on `Err` `E-USES-NOT-FOUND`; else `lute_syntax::parse(&text)` → `parse_meta_kind(&doc.meta, &CapabilitySnapshot::default(), MetaKind::Schema)` → on any returned diag, wrap/forward as `E-USES-PARSE` (or forward the meta diag with its code — decide: forward the schema doc's own `E-STATE-*`/`E-META-*` diags re-spanned to `at`, plus a leading `E-USES-PARSE` naming the file if the frontmatter itself failed). Simplest: collect the schema doc's `parse_meta` diags; if non-empty, push ONE `E-USES-PARSE` naming the file (the detailed schema-doc diags are the schema author's concern; keep the scene's view to "this import didn't load cleanly"). 
  - Merge the schema doc's `state.decls` into the accumulator: on a key already present from a DIFFERENT canonical file → `E-USES-DUP-STATE` (name path + both files); else insert. Merge `defs`: on a name already present from a DIFFERENT file → `E-USES-DUP-DEF`; else insert.
  - Recurse into the schema doc's own `uses` with `base_dir = <that file>.parent()`; pop the stack after.
- All diagnostics: `Diagnostic { code, severity: Error, message, span: at, layer: Content, fixits: [], provenance: None }` (mirror `meta.rs`'s `err`).

- [ ] **Step 1: failing tests** (temp dirs; write schema `.lute` files then resolve):
  ```rust
  #[test] fn resolves_single_import() { /* write schema.lute w/ state run.x; resolve_imports(dir, &["schema.lute".into()], span) -> state has run.x, no diags */ }
  #[test] fn cycle_is_e_uses_cycle() { /* a.lute uses b.lute uses a.lute -> E-USES-CYCLE */ }
  #[test] fn dup_def_across_imports_errors() { /* two schema files each defs: foo -> E-USES-DUP-DEF */ }
  #[test] fn missing_file_is_not_found() { /* uses ["nope.lute"] -> E-USES-NOT-FOUND, no panic */ }
  #[test] fn diamond_is_one_identity_no_dup() { /* a uses [b,c]; b uses d; c uses d (d defs: x) -> no dup, x present once */ }
  ```
- [ ] **Step 2: RED** — `cargo test -p lute-check --test uses_import resolve` → FAIL.
- [ ] **Step 3: implement** `resolve_imports` + `MetaKind`/`parse_meta_kind`.
- [ ] **Step 4: GREEN + build** — `cargo test -p lute-check --test uses_import` + `cargo build -p lute-check -p lute-lsp -p lute-cli`.
- [ ] **Step 5: fmt + clippy + commit**
  ```bash
  cargo fmt -p lute-check && cargo clippy -p lute-check --all-targets -- -D warnings
  git add -A && git commit -m "feat(check): resolve_imports DAG resolver + schema-mode parse (dsl §9.2)"
  ```

# Task U3: wire CLI + LSP to call `resolve_imports`

**Files:**
- Modify: `crates/lute-cli/src/main.rs` (~line 138-152, the check command) — after `parse_meta(meta0)`, compute the scene base dir and imports.
- Modify: `crates/lute-lsp/src/backend.rs` (the `analyze`/`snapshot_for` path that builds `CheckInput` ~line 80-90) — base dir from the document uri.
- Test: `crates/lute-lsp/tests/divergence.rs` (a `uses:` scene case), updated `input_for` if needed.

**Interfaces:**
- Consumes: `lute_check::resolve_imports`, `lute_check::schema_import::SchemaImports`.
- CLI: `let base = file.parent().unwrap_or_else(|| std::path::Path::new("."));` `let imports = lute_check::resolve_imports(base, &meta0.uses, doc.meta.span);` → `CheckInput { .., imports }`. (`meta0.uses` is already parsed at main.rs:139-ish.)
- LSP: derive the document path from the uri (`uri_to_path`), take its `.parent()`; if the uri is not a file path (untitled), pass an empty `SchemaImports::default()` (no imports resolvable). `let imports = uri_to_path(uri).and_then(|p| p.parent().map(|d| resolve_imports(d, &meta0.uses, meta0_span))).unwrap_or_default();`

- [ ] **Step 1: failing test** — add to `divergence.rs` a case that reads a fixture scene with `uses:` and asserts CLI-path vs LSP-path diagnostics match (both call `resolve_imports` on the same base dir). Use the U4 fixture (author it first if executing U3 before U4, or fold U4's fixture creation into this step).
- [ ] **Step 2: RED** → `cargo test -p lute-lsp --test divergence` FAIL (arity / behavior) until wired.
- [ ] **Step 3: implement** the CLI + LSP wiring.
- [ ] **Step 4: GREEN + build** — `cargo test -p lute-lsp` + `cargo build -p lute-check -p lute-lsp -p lute-cli`.
- [ ] **Step 5: fmt + clippy + commit**
  ```bash
  cargo fmt -p lute-cli -p lute-lsp && cargo clippy -p lute-cli -p lute-lsp --all-targets -- -D warnings
  git add -A && git commit -m "feat(cli,lsp): resolve scene uses: imports via shared resolver (dsl §9.2)"
  ```

# Task U4: fixture + full gate

**Files:**
- Create: `docs/examples/state.schema.lute` (a schema doc: frontmatter `state:` with `run.*`/`user.*`/`app.*` + shared `defs:`; no scene body needed).
- Create: `docs/examples/carry-ep.lute` (a minimal valid scene `uses: state.schema.lute` that reads an imported `run.*` path via `<match>`).
- Create/modify: `crates/lute-cli/tests/uses_import.rs` — acceptance: `carry-ep.lute` exits 0 (imported path resolves); a bad-import scene (cyclic or redeclare) exits 1 with the right code.
- Test: the full gate.

- [ ] **Step 1** author the fixture pair; verify `./target/debug/lute check docs/examples/carry-ep.lute` exits 0 (build first).
- [ ] **Step 2** add the CLI acceptance test(s) (RED→GREEN).
- [ ] **Step 3: full gate**
  - `cargo test --workspace` green.
  - `cargo clippy --workspace --all-targets -- -D warnings` 0.
  - `cargo fmt --check` clean.
  - `(cd tree-sitter-lute && npx --yes tree-sitter-cli@latest test)` 25/25 (capabilityVersion UNCHANGED — no grammar/hash touch).
  - Prior acceptances unchanged: bianca exit 0; date-minigame core-only exit 1; `--project` exit 0; idola-portrait `--project` exit 0. New: `carry-ep.lute` exit 0.
- [ ] **Step 4: commit**
  ```bash
  git add -A && git commit -m "feat(examples,check): uses: schema-import fixture + gate (dsl §9.2)"
  ```

---

# Final gate (after U1–U4)

- [ ] `cargo test --workspace` all green · `cargo clippy --workspace --all-targets -- -D warnings` 0 · `cargo fmt --check` clean · tree-sitter 25/25.
- [ ] Acceptances: bianca 0 / dm-core 1 / dm-proj 0 / portrait 0 / carry-ep 0.
- [ ] Both trees clean; whole-branch review (most-capable) → Ready to merge; ff-merge to `main`.

## Self-Review

- **Spec coverage:** §9.2 DAG (U2 cycle/dup/canonicalize), schema-before-scene (resolver runs before check), no-override (U1 `E-STATE-REDECLARE`), `E-UNDECLARED` for unknown (unchanged; imported paths now resolve), dup-defs (U2 `E-USES-DUP-DEF`), path canonicalization (U2 `canonicalize` + visited set). Schema doc format = `.lute` frontmatter (U2 schema-mode parse). CLI+LSP wiring + no-divergence (U3). Fixture (U4).
- **Placeholder scan:** every task has concrete signatures + representative test bodies; per-task briefs (written at execution) carry the full test code grounded in a fresh read.
- **Type consistency:** `SchemaImports { state: StateSchema, defs: BTreeMap<String, serde_yaml::Value>, diags: Vec<Diagnostic> }` and `resolve_imports(base_dir, uses, at) -> SchemaImports` are stable across U1–U4; `MetaKind::{Scene, Schema}` + `parse_meta_kind` introduced in U2.
- **Deferrals (documented, low-drift):** "only `scene.*` inline" strictness NOT enforced (only override-of-imported errors); project-resolved schema ids + `extends:` deferred (resolver is a DAG so they land without rework); schema-doc internal diagnostics collapsed to one `E-USES-PARSE` per bad file (the scene sees "import didn't load"; the schema author checks the schema doc directly).
- **Risk:** `std::fs::canonicalize` requires the file to exist (it does I/O) — a missing file correctly falls into `E-USES-NOT-FOUND`. The LSP untitled-buffer case (no file path) yields empty imports (no crash). `check()` stays pure; all I/O is in `resolve_imports`, called identically by both surfaces → no divergence.
