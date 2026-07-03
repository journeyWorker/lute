# FND-2: LSP Feature Context sees `uses:` Imports — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development.

**Goal:** Fix a real half-divergence: `check()` diagnostics resolve `uses:` schema imports (dsl §9.2), but the editor features (hover, completion, definition, references) do NOT — so imported state paths / imported `@ref` defs are invisible to hover/completion/definition/references even though they validate. Thread the resolved `SchemaImports` into the four feature entrypoints and merge imported state/defs into their lookups, exactly as `check()` does.

**Architecture:** Extract the `uses:` resolution already in `Backend::analyze` into `Backend::imports_for(uri, text) -> SchemaImports`, call it in the 4 feature handlers, pass `&SchemaImports` into `hover_at` / `complete_at` / `definition_at` / `references_at`. Each feature fn merges imported state (`imports.state.decls`) and imported defs (`imports.defs`) into its local `TypedMeta` (inline wins on key collision — matches `check()`'s scene-local precedence), so every existing sub-helper (`state_hover`, `state_path_items`, `def_info`, nav targets) transparently sees imports.

**Tech Stack:** Rust; `lute-lsp` (tower-lsp-server), `lute-check` (`parse_meta`, `resolve_imports`, `SchemaImports`, `TypedMeta`).

## Global Constraints
- `export PATH="$HOME/.cargo/bin:$PATH"` every shell. Worktree `/Users/journey/Workspace/lute/.worktrees/lute-lsp-rust` (branch `feat/lute-lsp-rust`); ABSOLUTE worktree paths; cargo/git cwd = worktree.
- Per-task hygiene BEFORE commit: `cargo fmt -p lute-lsp` + `cargo clippy -p lute-lsp --all-targets -- -D warnings`.
- Keep `lute-lsp/tests/divergence.rs` green (diagnostics unaffected — this only touches features).
- No-divergence spirit: editor features must now agree with diagnostics on imported schema. Do NOT change `check()` or diagnostics. Best-effort: if import resolution fails, features degrade to local-only (never panic).
- capabilityVersion / tree-sitter untouched.

## Background (grounding)
- `Backend::analyze` (backend.rs:84-94) already resolves imports: `uri_to_path(uri).and_then(|p| p.parent().map(|d| lute_check::resolve_imports(d, &meta0.uses, doc.meta.span))).unwrap_or_default()`.
- Feature handlers (backend.rs:214-283) call `snapshot_for` but never resolve imports; they call `hover::hover_at(&doc, &snapshot, off)`, `completion::complete_at(&doc, &snapshot, &providers, off)`, `nav::definition_at(&doc, &snapshot, off)`, `nav::references_at(&doc, &snapshot, off, include_decl)`.
- Feature fns parse meta locally: `let (meta, _) = parse_meta(&doc.meta, snapshot)` (completion.rs:35, hover.rs:33; nav.rs similarly). Lookups: `meta.state.decls` (state hover/completion/nav), `meta.defs.keys()` + `snapshot.defs.keys()` (def names, completion.rs:197-198), `def_info` (mod.rs:342-362).
- `SchemaImports { state: StateSchema, defs: BTreeMap<String, serde_yaml::Value>, diags: Vec<Diagnostic> }`. `TypedMeta { state: StateSchema, defs: BTreeMap<String, serde_yaml::Value>, .. }` (both pub). `StateSchema { decls: BTreeMap<String, StateDecl>, .. }` (pub `decls`).

---

## Task 1: Thread `SchemaImports` into the 4 feature entrypoints + merge

**Files:**
- Modify: `crates/lute-lsp/src/backend.rs` (add `imports_for`; use in analyze + 4 handlers; pass imports to feature fns)
- Modify: `crates/lute-lsp/src/features/hover.rs` (signature + merge)
- Modify: `crates/lute-lsp/src/features/completion.rs` (signature + merge)
- Modify: `crates/lute-lsp/src/features/nav.rs` (both signatures + merge)
- Modify: `crates/lute-lsp/src/features/mod.rs` (add a shared merge helper; update doc comment)
- Test: feature tests for imported state + imported defs in each of hover/completion/nav.

**Interfaces:**
- `Backend::imports_for(&self, uri: &Uri, text: &str) -> lute_check::SchemaImports` — the extracted resolver (analyze's block). `analyze` calls it too (dedup).
- New signatures:
  - `hover::hover_at(doc: &Document, snapshot: &CapabilitySnapshot, imports: &SchemaImports, off: usize) -> Option<Hover>`
  - `completion::complete_at(doc, snapshot, providers, imports: &SchemaImports, off) -> Vec<CompletionItem>`
  - `nav::definition_at(doc, snapshot, imports: &SchemaImports, off) -> Option<Span>`
  - `nav::references_at(doc, snapshot, imports: &SchemaImports, off, include_decl) -> Vec<Span>`
- Shared merge helper in `features/mod.rs`:
  ```rust
  /// Merge imported schema (dsl §9.2) into a document's typed frontmatter so
  /// editor features see the same state/defs as `check()`. Inline entries win on
  /// key collision (scene-local precedence; a real conflict is a diagnostic, not
  /// the feature layer's concern).
  pub(crate) fn merge_imports(meta: &mut lute_check::TypedMeta, imports: &lute_check::SchemaImports) {
      for (k, v) in &imports.state.decls {
          meta.state.decls.entry(k.clone()).or_insert_with(|| v.clone());
      }
      for (k, v) in &imports.defs {
          meta.defs.entry(k.clone()).or_insert_with(|| v.clone());
      }
  }
  ```

- [ ] **Step 1: Write failing feature tests (imported state + defs)**

Add tests to `completion.rs`, `hover.rs`, `nav.rs` test modules. Each builds a doc whose frontmatter has `uses:` (or directly constructs a `SchemaImports` with an imported state path + imported def) and asserts the feature now surfaces the imported item:
- completion: an imported state path appears in state-path completion; an imported def name appears in `@ref` completion.
- hover: hovering an imported state path yields its type; hovering an imported `@ref` yields the def.
- nav: definition/references resolve an imported def / state path.
Construct `SchemaImports` directly in tests (avoid disk): e.g. `let mut imports = SchemaImports::default(); imports.state.decls.insert("run.gold".into(), StateDecl{ ty: Type::Number, default: None, .. }); imports.defs.insert("helped".into(), serde_yaml::from_str("{ type: bool, cel: \"true\" }").unwrap());`. Match the real `StateDecl` field shape (read `lute-check`/`lute-manifest` for it). Pass `&imports` into the feature fn.

- [ ] **Step 2: Run tests to verify they fail**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cd /Users/journey/Workspace/lute/.worktrees/lute-lsp-rust && cargo test -p lute-lsp` — the new tests fail to compile (signature) / assert.

- [ ] **Step 3: Add `merge_imports` to `features/mod.rs`**

Add the helper above; update the module doc comment (mod.rs:31-36) to note features now also merge `SchemaImports` (imported state/defs) so they match `check()`.

- [ ] **Step 4: Update the 4 feature fns**

In each of `hover_at`, `complete_at`, `definition_at`, `references_at`: add the `imports: &SchemaImports` param; change `let (meta, _) = parse_meta(...)` to `let (mut meta, _) = parse_meta(...)` then `super::merge_imports(&mut meta, imports);` immediately after. All existing sub-helpers already read `meta.state.decls` / `meta.defs`, so they now see imports with no further change. For the def-name completion (completion.rs:197-198) and `def_info` (mod.rs), confirm they read `meta.defs` (now merged) — no change needed beyond the merge. Add necessary `use lute_check::SchemaImports;`.

- [ ] **Step 5: Extract `imports_for` in backend + thread through handlers**

In `backend.rs`, add:
```rust
fn imports_for(&self, uri: &Uri, text: &str) -> lute_check::SchemaImports {
    let (doc, _) = lute_syntax::parse(text);
    let (meta0, _) = lute_check::parse_meta(&doc.meta, &lute_manifest::snapshot::CapabilitySnapshot::default());
    uri_to_path(uri)
        .and_then(|p| p.parent().map(|d| lute_check::resolve_imports(d, &meta0.uses, doc.meta.span)))
        .unwrap_or_default()
}
```
Refactor `analyze` to use it (replace lines 84-94). In each feature handler (`hover`, `completion`, `goto_definition`, `references`), compute `let imports = self.imports_for(&uri, &text);` and pass `&imports` into the feature call. (Reuse the already-parsed `text`; a second parse inside `imports_for` is acceptable and mirrors `snapshot_for`.)

- [ ] **Step 6: Build + tests + hygiene**

Run:
```
cargo build -p lute-lsp
cargo test -p lute-lsp
cargo test -p lute-lsp --test divergence
cargo fmt -p lute-lsp && cargo clippy -p lute-lsp --all-targets -- -D warnings
```
Expected: all green, 0 warnings. Existing feature tests updated to pass `&SchemaImports::default()` where they don't exercise imports.

- [ ] **Step 7: Commit**

```bash
cd /Users/journey/Workspace/lute/.worktrees/lute-lsp-rust
git add crates/lute-lsp/src/backend.rs crates/lute-lsp/src/features/
git commit -m "feat(lsp): editor features resolve uses: imports (state+defs), matching check() (FND-2, dsl §9.2)"
```

## Verification (controller, after review)
```
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
```

## Self-Review
- All 4 feature entrypoints thread `&SchemaImports` and merge before lookup.
- `imports_for` is the single resolver (analyze + handlers reuse it); no divergence between diagnostics' import resolution and features'.
- Inline-wins precedence matches `check()`; best-effort (no panic on unresolved imports).
- New tests prove imported state + imported defs surface in hover/completion/nav.
