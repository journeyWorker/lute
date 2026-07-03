# Plugin Def-Export Lock-in + `@name(args)` Parameterized Def Calls — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Lock the *already-working* plugin `defs`-export path behind an on-disk fixture + tests (Phase 1), then implement static checking of the parameterized def-call form `@name(args)` from DSL §8.1 (Phase 2): arity, per-argument type, and whole-slot produced-type checks.

**Architecture:** The disk plugin loader already reads a `defs` export into `LoadedPlugin.defs`, `assemble_snapshot` already merges it into `snap.defs`, and `check()` already unions plugin def names into `ctx.defs` and their types into `ctx.def_types` — so a plugin-exported `@ref` already type-checks end-to-end (verified: a bool def in a bool guard is clean; a number def in a bool guard fires `E-REF-TYPE`). Phase 1 converts that proven-but-untested-on-disk behavior into guarded regression coverage and deletes the stale comments that claim it doesn't work. Phase 2 extends the DSL-level `@ref` scanner (`lute_cel::scan_refs`) to also capture the call form `@name(args)`, then adds three conservative static checks in the CEL slot resolver, reusing the existing `compatible`/`resolve_type` machinery.

**Tech Stack:** Rust (rustup stable 1.96.1), 6-crate cargo workspace (`lute-core-span` ← `lute-manifest` ← `lute-syntax` ← `lute-cel` ← `lute-check` ← `lute-cli`/`lute-lsp`), `serde`/`serde_yaml` 0.9.34, tree-sitter (Node v22 + `npx --yes tree-sitter-cli@latest`).

## Global Constraints

- **Toolchain:** `export PATH="$HOME/.cargo/bin:$PATH"` in EVERY fresh shell. NEVER `brew install rust`. rustup stable 1.96.1 at `~/.cargo/bin`.
- **Worktree:** `/Users/journey/Workspace/lute/.worktrees/lute-lsp-rust`, branch `feat/lute-lsp-rust`. HARNESS QUIRK: `write`/`edit` resolve RELATIVE paths against the MAIN workspace (`~/Workspace/lute`), NOT the worktree — always use ABSOLUTE worktree paths. After committing, verify `git status` clean in BOTH trees (`?? .worktrees/` in the main tree is expected).
- **Shared target-dir:** both trees share `/Users/journey/Workspace/lute/target` via `~/Workspace/lute/.cargo/config.toml`; the worktree's `target` is a symlink, so `./target/debug/lute` resolves from the worktree.
- **Per-task hygiene (BEFORE committing):** `cargo fmt -p <crate>` every touched crate AND `cargo clippy -p <crate> --all-targets -- -D warnings`. Own-crate `cargo test -p <crate>` during the task.
- **CROSS-CRATE rule:** any public `lute-manifest`/`lute-check`/`lute-cel` type or signature change → grep call sites + `cargo build -p lute-check -p lute-lsp -p lute-cli`. Keep `lute-lsp/tests/divergence.rs` green.
- **tree-sitter capabilityVersion** = `45f46d46fb9b72ceebd735c0fcc5ceca6312e07ad1ce5ef2dd08f4b6d18cf50c` (guarded by `crates/lute-manifest/tests/tree_sitter_stamp.rs`). `lute.core` ships ZERO `defs`, so nothing in this plan changes `snap.defs` for the CORE snapshot → `capabilityVersion` is UNCHANGED and NO re-stamp is needed. The stamp guard MUST still pass at every gate; if it ever fails, STOP — that means an unexpected core-surface drift.
- **Invariants (never break):** snapshot-is-SoT · no-divergence (CLI+LSP both go through `resolve_document_snapshot` + `check()`) · determinism (BTreeMap / sorted output, `span.byte_start` then `code`) · never-panic (scanners/resolvers degrade to diagnostics).
- **Diagnostic-code convention:** stable `E-*` on `Diagnostic.code`. New codes this plan: `E-REF-ARITY`, `E-REF-ARG-TYPE` (both `dsl §8.1`).

---

## File Structure

- `crates/lute-cel/src/lib.rs` — `RefUse` gains `call: Option<Call>`; `scan_refs` captures the `@name(args)` call form (balanced-paren + string-aware top-level-comma split). New `Call`/arg-span types. (Phase 2, P2.1.)
- `crates/lute-check/src/cel_resolve.rs` — extend `check_cel_slot` `@ref` pass: whole-slot `E-REF-TYPE` for the call form (P2.2); `E-REF-ARITY` (P2.3); `E-REF-ARG-TYPE` (P2.4). Extend `is_whole_slot`; add `resolve_arg_type`.
- `crates/lute-check/src/ctx.rs` — add `Ctx::def_params: BTreeMap<String, Vec<(String, Type)>>` (ordered params per def). (P2.3.)
- `crates/lute-check/src/check.rs` — populate `ctx.def_params` from the three def sources (plugin / inline / imported), same precedence as `def_types`. (P2.3.)
- `crates/lute-manifest/src/schema.rs` — `DefDecl.params` changes from `BTreeMap<String, Type>` to order-preserving `Vec<DefParam>` (with a `deserialize_with` that reads the §8.1 `params:` YAML mapping in source order). (P2.3.)
- `crates/lute-lsp/src/features/mod.rs:369` — the one `DefDecl.params` consumer; update the `.iter()` shape (hover then shows params in declaration order). (P2.3.)
- `crates/lute-check/tests/plugin_defs_disk.rs` — NEW: disk-path integration test (temp project with a `defs` export → resolve → check), positive + negative. (P1.1.)
- `crates/lute-check/tests/ref_type.rs` — delete the stale "disk loader does not populate snapshot.defs today" comment (P1.2); add `@name(args)` unit cases (P2.2–P2.4).
- `crates/lute-cel/src/lib.rs` (tests) — `scan_refs` call-form unit tests. (P2.1.)
- `crates/lute-lsp/tests/divergence.rs` — add `divergence_holds_under_plugin_defs`. (P1.2.)
- `docs/examples/plugindef-project/` (NEW: `lute.project.yaml`, `plugins/demo.defs/plugin.yaml`, `plugins/demo.defs/defs/defs.yaml`) + `docs/examples/plugin-def.lute` — committed positive fixture. (P1.1.)
- `docs/examples/param-def.lute` — committed positive fixture calling a parameterized def. (P2.4.)
- `crates/lute-check/src/ctx.rs` doc comments — correct any claim that the loader does not populate `snapshot.defs`. (P1.2.)

---

## Phase 1 — Lock in the plugin def-export path

### Task P1.1: On-disk `defs`-export fixture + end-to-end acceptance + disk-path integration test

**Files:**
- Create: `docs/examples/plugindef-project/lute.project.yaml`
- Create: `docs/examples/plugindef-project/plugins/demo.defs/plugin.yaml`
- Create: `docs/examples/plugindef-project/plugins/demo.defs/defs/defs.yaml`
- Create: `docs/examples/plugin-def.lute`
- Create (test): `crates/lute-check/tests/plugin_defs_disk.rs`

**Interfaces:**
- Consumes: `lute_manifest::project::{load_project, resolve_document_snapshot}` — `resolve_document_snapshot(project: Option<&ProjectConfig>, profile: Option<&str>, plugins: &BTreeMap<String, ...>) -> (CapabilitySnapshot, Vec<ResolveDiag>)`; `lute_manifest::project::project_providers`; `lute_check::{check, CheckInput, Mode, SchemaImports}`; `lute_manifest::provider::ProviderSet`. Study the existing wiring in `crates/lute-cli/src/main.rs:116-160` and mirror it exactly (parse the scene's meta for `profile`/`plugins`, resolve from the scene's directory, feed the assembled snapshot + providers into `CheckInput`). Study `crates/lute-check/tests/uses_import.rs` for the temp-dir test pattern (`unique_dir`, `write_lute`).
- Produces: a committed positive fixture + gate line; `plugin_defs_disk.rs` proving the disk loader populates `snapshot.defs` and that plugin `@ref`s type-check.

- [ ] **Step 1: Write the committed fixture files**

`docs/examples/plugindef-project/lute.project.yaml`:
```yaml
pluginsDir: plugins/
defaultProfile: demo
profiles:
  demo:
    plugins: { demo.defs: true }
```

`docs/examples/plugindef-project/plugins/demo.defs/plugin.yaml`:
```yaml
id: demo.defs
version: 0.1.0
kind: capability
depends: [ { id: lute.core, range: "^0.0.1" } ]
exports:
  defs: defs/
```

`docs/examples/plugindef-project/plugins/demo.defs/defs/defs.yaml`:
```yaml
defs:
  - name: warm
    type: bool
    cel: "true"
```

`docs/examples/plugin-def.lute`:
```
---
character: demo
season: 1
episode: 1
state:
  scene.flag: { type: bool, default: false }
---

## Shot 1.

<match on="scene.flag">
  <when test="@warm">:line[narrator]: warm path
</when>
  <otherwise>:line[narrator]: cold path
</otherwise>
</match>
```

- [ ] **Step 2: Verify the fixture passes end-to-end (RED → GREEN by construction)**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cd /Users/journey/Workspace/lute/.worktrees/lute-lsp-rust && cargo build -p lute-cli && ./target/debug/lute check docs/examples/plugin-def.lute --project docs/examples/plugindef-project; echo "exit=$?"`
Expected: `ok: …/plugin-def.lute (0 warning(s))`, `exit=0`. (If it is NOT exit 0, STOP and investigate — the whole premise is that this path works.)

- [ ] **Step 3: Write the disk-path integration test (positive + negative)**

`crates/lute-check/tests/plugin_defs_disk.rs`. It writes a temp project (mirroring the fixture) with a `defs` export that declares BOTH a bool def and a number def, plus two scenes: one using the bool def whole-slot in a bool guard, one using the number def whole-slot in a bool guard. It resolves the snapshot from disk exactly as the CLI does and runs `check()`.

```rust
//! Disk-path coverage: a plugin `defs` export flows load_plugins_dir ->
//! assemble_snapshot -> snapshot.defs -> check(), so a plugin-exported `@ref`
//! is a declared def AND type-checks (dsl §8). Guards the end-to-end path that
//! ref_type.rs only exercises via a synthetic in-memory snapshot.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use lute_check::{check, CheckInput, Mode, SchemaImports};
use lute_manifest::project::{load_project, project_providers, resolve_document_snapshot};

static N: AtomicU32 = AtomicU32::new(0);

fn unique_dir() -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "lute-plugindefs-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn write(path: &Path, body: &str) {
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p).unwrap();
    }
    std::fs::write(path, body).unwrap();
}

/// Build a temp project whose one plugin exports a bool def `warm` and a number
/// def `tally`, then resolve+check `scene` (its text) sitting in the project root.
fn codes_for_scene(scene: &str) -> Vec<String> {
    let root = unique_dir();
    write(
        &root.join("lute.project.yaml"),
        "pluginsDir: plugins/\ndefaultProfile: demo\nprofiles:\n  demo:\n    plugins: { demo.defs: true }\n",
    );
    write(
        &root.join("plugins/demo.defs/plugin.yaml"),
        "id: demo.defs\nversion: 0.1.0\nkind: capability\ndepends: [ { id: lute.core, range: \"^0.0.1\" } ]\nexports:\n  defs: defs/\n",
    );
    write(
        &root.join("plugins/demo.defs/defs/defs.yaml"),
        "defs:\n  - { name: warm, type: bool, cel: \"true\" }\n  - { name: tally, type: number, cel: \"1\" }\n",
    );
    let scene_path = root.join("scene.lute");
    write(&scene_path, scene);

    // Mirror crates/lute-cli/src/main.rs:116-160.
    let project = load_project(&root).unwrap();
    // The scene declares no `profile:`/`plugins:` inline -> default profile.
    let (snapshot, _rdiags) = resolve_document_snapshot(Some(&project), None, &BTreeMap::new());
    let providers = project_providers(Some(&project));
    let input = CheckInput {
        text: scene.to_string(),
        uri: scene_path.display().to_string(),
        snapshot,
        providers,
        mode: Mode::Author,
        imports: SchemaImports::default(),
    };
    check(&input).diagnostics.into_iter().map(|d| d.code).collect()
}

const HDR: &str = "---\ncharacter: demo\nseason: 1\nepisode: 1\nstate:\n  scene.flag: { type: bool, default: false }\n---\n## Shot 1.\n";

#[test]
fn plugin_bool_def_from_disk_is_declared_and_clean() {
    let scene = format!(
        "{HDR}<match on=\"scene.flag\">\n<when test=\"@warm\">:line[narrator]: a\n</when>\n<otherwise>:line[narrator]: b\n</otherwise>\n</match>\n"
    );
    let codes = codes_for_scene(&scene);
    assert!(!codes.contains(&"E-UNDECLARED-REF".to_string()), "plugin def must be declared from disk; got {codes:?}");
    assert!(!codes.contains(&"E-REF-TYPE".to_string()), "bool def in bool guard is compatible; got {codes:?}");
}

#[test]
fn plugin_number_def_from_disk_flags_ref_type() {
    let scene = format!(
        "{HDR}<match on=\"scene.flag\">\n<when test=\"@tally\">:line[narrator]: a\n</when>\n<otherwise>:line[narrator]: b\n</otherwise>\n</match>\n"
    );
    let codes = codes_for_scene(&scene);
    assert!(!codes.contains(&"E-UNDECLARED-REF".to_string()), "got {codes:?}");
    assert!(codes.contains(&"E-REF-TYPE".to_string()), "number def in bool guard must flag E-REF-TYPE from disk; got {codes:?}");
}
```

NOTE: verify the exact `resolve_document_snapshot` / `project_providers` / `load_project` signatures against `crates/lute-cli/src/main.rs` before finalizing (adapt the `Some(&project)` / `Option` shapes if they differ). If `load_project` returns a `Result`/`Option`, unwrap as the CLI does.

- [ ] **Step 4: Run the integration test**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cd /Users/journey/Workspace/lute/.worktrees/lute-lsp-rust && cargo test -p lute-check --test plugin_defs_disk`
Expected: 2 passed.

- [ ] **Step 5: fmt + clippy the touched crate**

Run: `cargo fmt -p lute-check && cargo clippy -p lute-check --all-targets -- -D warnings`
Expected: no output / 0 warnings.

- [ ] **Step 6: Commit**

```bash
cd /Users/journey/Workspace/lute/.worktrees/lute-lsp-rust
git add docs/examples/plugindef-project docs/examples/plugin-def.lute crates/lute-check/tests/plugin_defs_disk.rs
git commit -m "test(check,examples): on-disk plugin defs-export fixture + disk-path coverage (dsl §8)"
```

---

### Task P1.2: Divergence-under-plugin-defs test + delete stale comments

**Files:**
- Modify: `crates/lute-lsp/tests/divergence.rs` (add `divergence_holds_under_plugin_defs`)
- Modify: `crates/lute-check/tests/ref_type.rs:107-111` (delete stale comment)
- Modify: `crates/lute-check/src/ctx.rs` (correct any "loader does not populate" claim)

**Interfaces:**
- Consumes: the existing `divergence_holds_under_plugin_project` test in `crates/lute-lsp/tests/divergence.rs` — copy its structure verbatim, swapping the project/scene for a plugin `defs` export.
- Produces: a golden proving CLI == LSP projection when a scene uses a plugin-exported def.

- [ ] **Step 1: Read the existing divergence golden**

Run: `read crates/lute-lsp/tests/divergence.rs` — locate `divergence_holds_under_plugin_project` and its temp-project + CLI-vs-LSP-projection helpers. The new test reuses the SAME helpers.

- [ ] **Step 2: Add `divergence_holds_under_plugin_defs`**

Add a test mirroring `divergence_holds_under_plugin_project`: write a temp project whose plugin exports a `defs` file (bool def `warm`, `cel: "true"`) and a scene using `@warm` whole-slot in a `<when test>` bool guard, then assert the CLI-side projection equals the LSP-side projection (byte-identical diagnostics list), exactly as the existing test asserts. Follow the existing test's exact helper calls and assertion shape — do not invent new machinery.

- [ ] **Step 3: Run the divergence suite**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cd /Users/journey/Workspace/lute/.worktrees/lute-lsp-rust && cargo test -p lute-lsp --test divergence`
Expected: all divergence tests pass (was 4; now 5).

- [ ] **Step 4: Delete the stale comment in `ref_type.rs`**

In `crates/lute-check/tests/ref_type.rs`, the block comment at lines 107-111 ends with "Exercised via a SYNTHETIC snapshot (the disk loader does not populate `snapshot.defs` today)." That parenthetical is false (P1.1 proves the disk loader DOES populate `snapshot.defs`). Rewrite the comment to state the truth: these tests use a synthetic snapshot for convenience/isolation, and `crates/lute-check/tests/plugin_defs_disk.rs` covers the on-disk path. Keep the `check_codes` helper unchanged.

- [ ] **Step 5: Correct the `ctx.rs` doc comment**

In `crates/lute-check/src/ctx.rs`, read the `def_types` / "Def-type sources" / "`ctx.defs` vs `def_types` boundary" doc region (~lines 46-158). The description of the two tables' independence is ACCURATE — keep it. Only fix any sentence that implies the loader cannot / does not populate `snapshot.defs`. If none exists there, make no change and note that in the commit body. (Do NOT gratuitously reword accurate docs.)

- [ ] **Step 6: fmt + clippy**

Run: `cargo fmt -p lute-check -p lute-lsp && cargo clippy -p lute-check -p lute-lsp --all-targets -- -D warnings`
Expected: 0 warnings.

- [ ] **Step 7: Commit**

```bash
cd /Users/journey/Workspace/lute/.worktrees/lute-lsp-rust
git add crates/lute-lsp/tests/divergence.rs crates/lute-check/tests/ref_type.rs crates/lute-check/src/ctx.rs
git commit -m "test(lsp),docs(check): divergence under plugin defs + drop stale snapshot.defs gap comment (dsl §8)"
```

---

## Phase 2 — `@name(args)` parameterized def calls (DSL §8.1)

**Design (owned decisions; the reviewer verifies soundness):**
- DSL §8.1: `@name` / `@name(args)` is a compile-time macro; params bound from call args; a `@ref` MUST appear in a position whose required type matches the def's declared `type`, and `name` MUST be declared. Macro EXPANSION is the ENGINE's job → OUT of scope. The static checker owns four checks, three of them new:
  1. **name declared** — already works (`E-UNDECLARED-REF`).
  2. **arity** (`E-REF-ARITY`, new) — call arg count MUST equal the def's param count. A bare `@name` is 0 args; a def with N>0 params called bare → `E-REF-ARITY`.
  3. **whole-slot produced-type** (`E-REF-TYPE`, extend existing) — when `@name(args)` is the WHOLE CEL value, compare the def's produced `type` to the slot's expected type via the existing `compatible`.
  4. **per-argument type** (`E-REF-ARG-TYPE`, new) — for each positional arg whose type is statically resolvable (number/string/bool literal, bare state path, bare `@ref`), compare to the corresponding param's declared type via `compatible`; skip unresolvable args (conservative — no false positives).
- Positional arg→param binding REQUIRES declaration order. `DefDecl.params` (plugin defs) is a `BTreeMap` today (loses order) → change it to an order-preserving `Vec<DefParam>` (P2.3). Inline/imported defs arrive as a `serde_yaml` mapping, which is insertion-ordered (serde_yaml 0.9.34) → extract in order. `lute.core` ships no defs, so this does NOT shift `capabilityVersion`.
- Conservative throughout: only PROVABLY-wrong cases flag; unknown/unresolvable → silent, exactly like Part B's `E-REF-TYPE`.

### Task P2.1: Scanner captures the `@name(args)` call form (`lute-cel`)

**Files:**
- Modify: `crates/lute-cel/src/lib.rs` (`RefUse`, `scan_refs`, new `Call` type + tests)

**Interfaces:**
- Consumes: existing `cel_string_mask(raw) -> Vec<bool>` (string-literal mask) and `byte_span(s, e) -> Span`.
- Produces:
  ```rust
  pub struct RefUse {
      pub name: String,
      pub is_dollar: bool,
      pub span: Span,        // the `@name` token (unchanged)
      pub call: Option<Call>, // Some when `@name` is immediately followed by `(...)`
  }
  pub struct Call {
      pub span: Span,        // the whole `(...)` group, parens inclusive
      pub args: Vec<Span>,   // one span per top-level, comma-separated argument (trimmed); empty for `@name()`
  }
  ```
  Downstream (`cel_resolve::check_cel_slot`) reads `r.call`; the `$` branch and bare refs set `call: None`.

- [ ] **Step 1: Write failing scanner unit tests**

Add to the `#[cfg(test)]` module in `crates/lute-cel/src/lib.rs`:
```rust
#[test]
fn scan_refs_captures_call_form() {
    let refs = scan_refs("@atLeast(2)");
    let r = refs.iter().find(|r| r.name == "atLeast").expect("ref");
    let call = r.call.as_ref().expect("call captured");
    assert_eq!(call.args.len(), 1);
}

#[test]
fn scan_refs_bare_ref_has_no_call() {
    let refs = scan_refs("@fond");
    assert!(refs.iter().find(|r| r.name == "fond").unwrap().call.is_none());
}

#[test]
fn scan_refs_empty_args() {
    let refs = scan_refs("@now()");
    assert_eq!(refs.iter().find(|r| r.name == "now").unwrap().call.as_ref().unwrap().args.len(), 0);
}

#[test]
fn scan_refs_commas_in_nested_paren_and_string_not_split() {
    // top-level args: `max(a, b)` and `'x,y'` -> exactly 2 args
    let refs = scan_refs("@pick(max(a, b), 'x,y')");
    let call = refs.iter().find(|r| r.name == "pick").unwrap().call.as_ref().unwrap();
    assert_eq!(call.args.len(), 2, "commas inside nested parens/strings must not split");
}

#[test]
fn scan_refs_space_before_paren_is_not_a_call() {
    // `@x (y)` is a bare ref `@x` then a separate parenthesized group.
    let refs = scan_refs("@x (y)");
    assert!(refs.iter().find(|r| r.name == "x").unwrap().call.is_none());
}

#[test]
fn scan_refs_unterminated_paren_degrades_to_no_call() {
    // never panic; an unterminated `(` yields no call (bare ref).
    let refs = scan_refs("@x(a, b");
    assert!(refs.iter().find(|r| r.name == "x").unwrap().call.is_none());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cd /Users/journey/Workspace/lute/.worktrees/lute-lsp-rust && cargo test -p lute-cel scan_refs_`
Expected: compile error (`RefUse` has no field `call`) or assertion failures.

- [ ] **Step 3: Add the `Call` type and extend `RefUse`**

In `crates/lute-cel/src/lib.rs`, add near `RefUse`:
```rust
/// A parenthesized call group following an `@name` (dsl §8.1 `@name(args)`).
#[derive(Clone, Debug)]
pub struct Call {
    /// Byte span of the whole `(...)` group (parens inclusive), within the raw source.
    pub span: Span,
    /// One byte span per top-level, comma-separated argument (whitespace-trimmed).
    /// Empty for `@name()`.
    pub args: Vec<Span>,
}
```
Add `pub call: Option<Call>,` to `RefUse`.

- [ ] **Step 4: Extend `scan_refs`**

After building the `@name` token (`out.push(RefUse { … })`), the current code pushes with no call. Replace the `@`-branch push so that, after computing the name end `i`, it checks whether `b[i] == b'('` (IMMEDIATELY, no whitespace) and `!mask[i]`; if so, scan a balanced paren group honoring `cel_string_mask` (a `(`/`)` inside a string does not nest), tracking depth from 1 at the opening `(`. On reaching depth 0, the group is `[open..=close]`. Split the interior `(open+1 .. close)` on top-level commas (depth 0, `!mask`), producing trimmed arg spans (skip a leading/trailing all-whitespace interior → 0 args). Set `call: Some(Call { span: byte_span(open, close+1), args })`. If the group never closes (EOF before depth 0), degrade: `call: None`, and set `i` back to the byte just after the name (so the `(` is re-scanned as ordinary text). For the bare case set `call: None`. Implement an arg-splitter helper:
```rust
/// Split the interior of a call group (`start..end` byte range of `raw`, exclusive
/// of the parens) into per-argument trimmed spans, honoring string literals and
/// nested parens. An all-whitespace interior yields no args.
fn split_args(raw: &str, mask: &[bool], start: usize, end: usize) -> Vec<Span> {
    let b = raw.as_bytes();
    let mut args = Vec::new();
    let mut depth = 0i32;
    let mut seg_start = start;
    let mut i = start;
    let push_seg = |args: &mut Vec<Span>, s: usize, e: usize| {
        // trim ASCII whitespace at both ends; skip an empty segment
        let mut a = s;
        let mut z = e;
        while a < z && raw.as_bytes()[a].is_ascii_whitespace() { a += 1; }
        while z > a && raw.as_bytes()[z - 1].is_ascii_whitespace() { z -= 1; }
        if a < z { args.push(byte_span(a, z)); }
    };
    while i < end {
        let c = b[i];
        if !mask[i] {
            match c {
                b'(' | b'[' | b'{' => depth += 1,
                b')' | b']' | b'}' => depth -= 1,
                b',' if depth == 0 => {
                    push_seg(&mut args, seg_start, i);
                    seg_start = i + 1;
                }
                _ => {}
            }
        }
        i += 1;
    }
    push_seg(&mut args, seg_start, end);
    args
}
```
Keep the whole scanner panic-free: all indexing stays `< b.len()`.

- [ ] **Step 5: Run the scanner tests**

Run: `cargo test -p lute-cel scan_refs_`
Expected: all pass. Then run the whole crate: `cargo test -p lute-cel` — existing `scan_refs` tests still pass (bare `@fond`, string-embedded `@x` still work; they now also carry `call: None`).

- [ ] **Step 6: Cross-crate — grep RefUse construction + build downstream**

Run: `grep RefUse across crates/` — any struct-literal construction of `RefUse` outside `lib.rs` must add `call: None`. Readers (`crates/lute-check/src/cel_resolve.rs:41`) only pattern-read fields, so they are unaffected. Then:
Run: `cargo build -p lute-check -p lute-lsp -p lute-cli`
Expected: builds clean.

- [ ] **Step 7: fmt + clippy + commit**

```bash
cd /Users/journey/Workspace/lute/.worktrees/lute-lsp-rust
cargo fmt -p lute-cel && cargo clippy -p lute-cel --all-targets -- -D warnings
git add crates/lute-cel/src/lib.rs
git commit -m "feat(cel): scan_refs captures the @name(args) call form (dsl §8.1)"
```

---

### Task P2.2: Whole-slot `E-REF-TYPE` for the call form (`lute-check`)

**Files:**
- Modify: `crates/lute-check/src/cel_resolve.rs` (`is_whole_slot` / the `E-REF-TYPE` branch)
- Test: `crates/lute-check/tests/ref_type.rs`

**Interfaces:**
- Consumes: `RefUse.call` (P2.1); existing `compatible`, `ctx.def_types`, `ExpectedType`.
- Produces: `E-REF-TYPE` now fires for a whole-slot `@name(args)` (previously only bare `@name`).

- [ ] **Step 1: Write a failing test**

Add to `crates/lute-check/tests/ref_type.rs` (using the synthetic-snapshot `check_codes` helper; inject a number def WITH a param). Define a scene whose `<when test>` is exactly `@countAtLeast(2)`:
```rust
#[test]
fn call_form_whole_slot_number_def_in_bool_guard_flags_ref_type() {
    let mut snap = lute_manifest::core::load_core_snapshot();
    snap.defs.insert(
        "countAtLeast".into(),
        DefDecl {
            name: "countAtLeast".into(),
            ty: Type::Number,
            params: vec![lute_manifest::schema::DefParam { name: "n".into(), ty: Type::Number }],
            cel: "1".into(),
            min: None, max: None, values: None,
        },
    );
    let scene = "---\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  scene.flag: { type: bool, default: false }\n---\n## Shot 1.\n<match on=\"scene.flag\">\n<when test=\"@countAtLeast(2)\">:line[narrator]: a\n</when>\n<otherwise>:line[narrator]: b\n</otherwise>\n</match>\n";
    let codes = check_codes(scene, snap);
    assert!(codes.contains(&"E-REF-TYPE".to_string()), "number-producing @call in a bool guard must flag; got {codes:?}");
}
```
NOTE: `DefParam` lands in P2.3. To keep P2.2 self-contained, TEMPORARILY construct the def with `params: Default::default()` (empty) — the whole-slot type check does NOT depend on params, only on `ty` and the call being whole-slot. Adjust the test to use empty params:
```rust
            params: Default::default(),
```
(P2.3 adds param-bearing cases.)

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p lute-check --test ref_type call_form_whole_slot`
Expected: FAIL — `E-REF-TYPE` not present, because `is_whole_slot` currently rejects `@name(...)`.

- [ ] **Step 3: Extend `is_whole_slot` in `cel_resolve.rs`**

The current guard (cel_resolve.rs:64-68) is:
```rust
let is_whole_slot = slot.raw.trim().strip_prefix('@').is_some_and(|rest| rest == r.name);
```
Replace it so the trimmed slot may be `@name` OR `@name(<balanced parens>)` and nothing else. Use the scanned `call`: the slot is whole iff, after trimming, it starts with `@name` and the remainder is either empty (bare) or exactly the call group captured by `r.call` (i.e. `r.call`'s span reaches the end of the trimmed slot). Concretely:
```rust
let trimmed = slot.raw.trim();
let is_whole_slot = match trimmed.strip_prefix('@') {
    Some(rest) if rest == r.name => true, // bare `@name`
    Some(rest) => {
        // `@name(...)` with the call group consuming the rest.
        r.call.as_ref().is_some_and(|c| {
            rest.strip_prefix(r.name.as_str())
                .map(str::trim_end)
                .is_some_and(|after| after.starts_with('(') && after.ends_with(')') && !after.is_empty())
                && c.args.iter().all(|_| true) // call was parsed => balanced
        })
    }
    None => false,
};
```
Keep it simple and robust; the essential change is: a whole-slot `@name(args)` is accepted. (The `r.call.is_some()` + "trimmed remainder after the name is a parenthesized group" is the sound test.)

- [ ] **Step 4: Run tests**

Run: `cargo test -p lute-check --test ref_type`
Expected: the new test passes; ALL existing `ref_type` tests still pass (bare-ref whole-slot and compound `@num > 0` non-whole-slot behavior unchanged — a compound expression still has `is_whole_slot == false`).

- [ ] **Step 5: fmt + clippy + commit**

```bash
cd /Users/journey/Workspace/lute/.worktrees/lute-lsp-rust
cargo fmt -p lute-check && cargo clippy -p lute-check --all-targets -- -D warnings
git add crates/lute-check/src/cel_resolve.rs crates/lute-check/tests/ref_type.rs
git commit -m "feat(check): E-REF-TYPE covers whole-slot @name(args) call form (dsl §8.1)"
```

---

### Task P2.3: Order-preserving def params + `E-REF-ARITY` (`lute-manifest` + `lute-check`)

**Files:**
- Modify: `crates/lute-manifest/src/schema.rs` (`DefDecl.params` → `Vec<DefParam>`; new `DefParam`; ordered `deserialize_with`)
- Modify: `crates/lute-lsp/src/features/mod.rs:369-372` (the one `.params` consumer)
- Modify: `crates/lute-check/src/ctx.rs` (`Ctx::def_params`)
- Modify: `crates/lute-check/src/check.rs` (populate `ctx.def_params`)
- Modify: `crates/lute-check/src/cel_resolve.rs` (emit `E-REF-ARITY`)
- Test: `crates/lute-check/tests/ref_type.rs`

**Interfaces:**
- Consumes: `RefUse.call` (P2.1); `DefDecl.params`; `serde_yaml::Value`.
- Produces:
  ```rust
  // lute-manifest/src/schema.rs
  #[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
  pub struct DefParam { pub name: String, #[serde(rename = "type")] pub ty: Type }
  // DefDecl.params: Vec<DefParam>  (was BTreeMap<String, Type>)

  // lute-check/src/ctx.rs
  pub def_params: BTreeMap<String, Vec<(String, Type)>>, // def name -> ordered (param name, type)
  ```

- [ ] **Step 1: Write failing arity tests**

Add to `crates/lute-check/tests/ref_type.rs`:
```rust
#[test]
fn arity_mismatch_flags() {
    // def `atLeast(n)` (1 param) called with 0 args (bare) -> E-REF-ARITY.
    let mut snap = lute_manifest::core::load_core_snapshot();
    snap.defs.insert("atLeast".into(), DefDecl {
        name: "atLeast".into(), ty: Type::Bool,
        params: vec![lute_manifest::schema::DefParam { name: "n".into(), ty: Type::Number }],
        cel: "true".into(), min: None, max: None, values: None,
    });
    let scene = "---\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  scene.flag: { type: bool, default: false }\n---\n## Shot 1.\n<match on=\"scene.flag\">\n<when test=\"@atLeast\">:line[narrator]: a\n</when>\n<otherwise>:line[narrator]: b\n</otherwise>\n</match>\n";
    assert!(check_codes(scene, snap).contains(&"E-REF-ARITY".to_string()));
}

#[test]
fn arity_match_is_clean() {
    let mut snap = lute_manifest::core::load_core_snapshot();
    snap.defs.insert("atLeast".into(), DefDecl {
        name: "atLeast".into(), ty: Type::Bool,
        params: vec![lute_manifest::schema::DefParam { name: "n".into(), ty: Type::Number }],
        cel: "true".into(), min: None, max: None, values: None,
    });
    let scene = "---\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  scene.flag: { type: bool, default: false }\n---\n## Shot 1.\n<match on=\"scene.flag\">\n<when test=\"@atLeast(2)\">:line[narrator]: a\n</when>\n<otherwise>:line[narrator]: b\n</otherwise>\n</match>\n";
    assert!(!check_codes(scene, snap).contains(&"E-REF-ARITY".to_string()));
}

#[test]
fn paramless_def_called_with_args_flags_arity() {
    let mut snap = lute_manifest::core::load_core_snapshot();
    snap.defs.insert("warm".into(), DefDecl {
        name: "warm".into(), ty: Type::Bool, params: Default::default(),
        cel: "true".into(), min: None, max: None, values: None,
    });
    let scene = "---\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  scene.flag: { type: bool, default: false }\n---\n## Shot 1.\n<match on=\"scene.flag\">\n<when test=\"@warm(1)\">:line[narrator]: a\n</when>\n<otherwise>:line[narrator]: b\n</otherwise>\n</match>\n";
    assert!(check_codes(scene, snap).contains(&"E-REF-ARITY".to_string()));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p lute-check --test ref_type arity` and `paramless_def`
Expected: compile error (`DefParam` missing) then assertion failures.

- [ ] **Step 3: Change `DefDecl.params` to ordered `Vec<DefParam>`**

In `crates/lute-manifest/src/schema.rs`:
```rust
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DefParam {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: Type,
}
```
Change `DefDecl`:
```rust
    #[serde(default, deserialize_with = "de_params")]
    pub params: Vec<DefParam>,
```
Add the ordered deserializer (reads the §8.1 `params:` YAML mapping in SOURCE order via `serde_yaml::Mapping`, which is insertion-ordered in serde_yaml 0.9.34; ALSO accept a YAML sequence of `{name,type}` for the plugin `defs.yaml` list form, so both spellings work):
```rust
fn de_params<'de, D>(d: D) -> Result<Vec<DefParam>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Raw {
        Map(serde_yaml::Mapping),
        Seq(Vec<DefParam>),
    }
    Ok(match Raw::deserialize(d)? {
        Raw::Seq(v) => v,
        Raw::Map(m) => m
            .into_iter()
            .filter_map(|(k, v)| {
                let name = k.as_str()?.to_string();
                let ty: Type = serde_yaml::from_value(v).ok()?;
                Some(DefParam { name, ty })
            })
            .collect(),
    })
}
```
(A `params: { p: number }` mapping and a `params: [{ name: p, type: number }]` sequence both deserialize; a malformed entry is skipped, never a panic.)

- [ ] **Step 4: Fix the one consumer + capability-version hash**

`crates/lute-lsp/src/features/mod.rs:369-372` currently does `d.params.iter().map(|(k, t)| (k.clone(), type_label(t)))`. Change to:
```rust
            params: d.params.iter().map(|p| (p.name.clone(), type_label(&p.ty))).collect(),
```
The capability-version hasher (`crates/lute-manifest/src/snapshot.rs:150-153`) folds each def via `format!("{d:?}")`; `Vec<DefParam>`'s Debug is deterministic (source order), so hashing stays deterministic. `lute.core` ships no defs → the CORE `capabilityVersion` is byte-identical → the tree-sitter stamp guard still passes.

- [ ] **Step 5: Add `Ctx::def_params` and populate it**

In `crates/lute-check/src/ctx.rs`, add to `Ctx`:
```rust
    /// def name -> ordered (param name, type), for `@name(args)` arity/arg-type
    /// checks (dsl §8.1). Same sources & precedence as `def_types`.
    pub def_params: BTreeMap<String, Vec<(String, Type)>>,
```
In `crates/lute-check/src/check.rs`, alongside the existing `def_types` build (check.rs:213-241), build `def_params` from the same three sources with the same precedence (plugin < imported < inline):
- **plugin** (`input.snapshot.defs`): `d.params.iter().map(|p| (p.name.clone(), p.ty.clone())).collect()`.
- **imported** (`input.imports.defs`, a `serde_yaml::Value`): read the `params` sub-mapping in order → `Vec<(String, Type)>` (deserialize each value to `Type`; skip malformed).
- **inline** (`typed.defs`, a `serde_yaml::Value`): same extraction; overrides.
A def with no `params:` → empty Vec. Reuse a small helper `fn params_from_yaml(v: &serde_yaml::Value) -> Vec<(String, Type)>` for the inline/imported cases (mapping order preserved). Insert `def_params` into the constructed `Ctx` (check.rs ~247).

- [ ] **Step 6: Emit `E-REF-ARITY` in `cel_resolve.rs`**

In the `@ref` pass (cel_resolve.rs:52-81), after confirming the name is declared (`ctx.defs.contains(&r.name)`), add an arity check BEFORE/alongside the type check: if `ctx.def_params` has the name, compare the expected param count to the actual arg count (`r.call.as_ref().map_or(0, |c| c.args.len())`):
```rust
if let Some(params) = ctx.def_params.get(&r.name) {
    let got = r.call.as_ref().map_or(0, |c| c.args.len());
    if got != params.len() {
        diags.push(diag(
            "E-REF-ARITY",
            format!("`@{}` expects {} argument(s) but got {} (dsl §8.1)", r.name, params.len(), got),
            span,
        ));
    }
}
```
Place it so it does not suppress the existing `E-REF-TYPE` branch (both may fire). Keep determinism (final sort handles ordering).

- [ ] **Step 7: Run tests + cross-crate build**

Run: `cargo test -p lute-check --test ref_type` (arity tests pass; all prior pass).
Run: `cargo build -p lute-check -p lute-lsp -p lute-cli` (DefDecl change ripples clean).
Run: `cargo test -p lute-manifest` (DefsFile/DefDecl deserialize; add a unit test in `schema.rs`/loader tests if params deserialization is untested — a `params: { a: number, b: bool }` mapping yields `[DefParam{a,Number}, DefParam{b,Bool}]` in order).
Run: `cargo test -p lute-manifest --test tree_sitter_stamp` — MUST still pass (capabilityVersion unchanged).

- [ ] **Step 8: fmt + clippy + commit**

```bash
cd /Users/journey/Workspace/lute/.worktrees/lute-lsp-rust
cargo fmt -p lute-manifest -p lute-check -p lute-lsp && cargo clippy -p lute-manifest -p lute-check -p lute-lsp --all-targets -- -D warnings
git add crates/lute-manifest/src/schema.rs crates/lute-lsp/src/features/mod.rs crates/lute-check/src/ctx.rs crates/lute-check/src/check.rs crates/lute-check/src/cel_resolve.rs crates/lute-check/tests/ref_type.rs
git commit -m "feat(manifest,check): ordered def params + E-REF-ARITY for @name(args) (dsl §8.1)"
```

---

### Task P2.4: Per-argument type check `E-REF-ARG-TYPE` + fixture + gate + docs

**Files:**
- Modify: `crates/lute-check/src/cel_resolve.rs` (`E-REF-ARG-TYPE`, `resolve_arg_type`)
- Test: `crates/lute-check/tests/ref_type.rs`
- Create: `docs/examples/param-def.lute`
- Modify: `docs/proposals/scenario-dsl/0.0.1.md` (§8.1 note — checker-owned static checks)

**Interfaces:**
- Consumes: `ctx.def_params` (P2.3, ordered types); `RefUse.call.args` (P2.1, arg byte spans); existing `compatible`, `set_op::resolve_type`, `ctx.def_types`, `ExpectedType`.
- Produces: `E-REF-ARG-TYPE` for a provably-wrong positional arg type.

- [ ] **Step 1: Write failing arg-type tests**

Add to `crates/lute-check/tests/ref_type.rs`:
```rust
#[test]
fn arg_type_mismatch_flags() {
    // def `atLeast(n: number)` called with a string literal -> E-REF-ARG-TYPE.
    let mut snap = lute_manifest::core::load_core_snapshot();
    snap.defs.insert("atLeast".into(), DefDecl {
        name: "atLeast".into(), ty: Type::Bool,
        params: vec![lute_manifest::schema::DefParam { name: "n".into(), ty: Type::Number }],
        cel: "true".into(), min: None, max: None, values: None,
    });
    let scene = "---\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  scene.flag: { type: bool, default: false }\n---\n## Shot 1.\n<match on=\"scene.flag\">\n<when test=\"@atLeast('hi')\">:line[narrator]: a\n</when>\n<otherwise>:line[narrator]: b\n</otherwise>\n</match>\n";
    assert!(check_codes(scene, snap).contains(&"E-REF-ARG-TYPE".to_string()));
}

#[test]
fn arg_type_match_is_clean() {
    let mut snap = lute_manifest::core::load_core_snapshot();
    snap.defs.insert("atLeast".into(), DefDecl {
        name: "atLeast".into(), ty: Type::Bool,
        params: vec![lute_manifest::schema::DefParam { name: "n".into(), ty: Type::Number }],
        cel: "true".into(), min: None, max: None, values: None,
    });
    let scene = "---\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  scene.flag: { type: bool, default: false }\n---\n## Shot 1.\n<match on=\"scene.flag\">\n<when test=\"@atLeast(2)\">:line[narrator]: a\n</when>\n<otherwise>:line[narrator]: b\n</otherwise>\n</match>\n";
    assert!(!check_codes(scene, snap).contains(&"E-REF-ARG-TYPE".to_string()));
}

#[test]
fn unresolvable_arg_is_not_flagged() {
    // a compound arg expression has no single statically-known type -> skip (no false positive).
    let mut snap = lute_manifest::core::load_core_snapshot();
    snap.defs.insert("atLeast".into(), DefDecl {
        name: "atLeast".into(), ty: Type::Bool,
        params: vec![lute_manifest::schema::DefParam { name: "n".into(), ty: Type::Number }],
        cel: "true".into(), min: None, max: None, values: None,
    });
    let scene = "---\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  scene.n: { type: number, default: 0 }\n  scene.flag: { type: bool, default: false }\n---\n## Shot 1.\n<match on=\"scene.flag\">\n<when test=\"@atLeast(scene.n + 1)\">:line[narrator]: a\n</when>\n<otherwise>:line[narrator]: b\n</otherwise>\n</match>\n";
    assert!(!check_codes(scene, snap).contains(&"E-REF-ARG-TYPE".to_string()));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p lute-check --test ref_type arg_type` and `unresolvable_arg`
Expected: the mismatch test fails (code not emitted yet); the match/unresolvable tests pass trivially (no code) — confirm they stay green.

- [ ] **Step 3: Add `resolve_arg_type` + emit `E-REF-ARG-TYPE`**

In `crates/lute-check/src/cel_resolve.rs`, add a conservative arg-type resolver that reads the arg's raw substring (via its byte span, offset into `slot.raw`):
```rust
/// Best-effort static type of a single call argument's raw source. Returns
/// `None` (skip — never flag) for anything not trivially typeable.
fn resolve_arg_type(arg_raw: &str, ctx: &Ctx) -> Option<Type> {
    let a = arg_raw.trim();
    if a == "true" || a == "false" {
        return Some(Type::Bool);
    }
    if a.parse::<f64>().is_ok() {
        return Some(Type::Number);
    }
    if (a.starts_with('\'') && a.ends_with('\'') && a.len() >= 2)
        || (a.starts_with('"') && a.ends_with('"') && a.len() >= 2)
    {
        return Some(Type::Str);
    }
    if let Some(name) = a.strip_prefix('@') {
        // a nested bare `@ref` (no call) -> its produced type
        if name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-') {
            return ctx.def_types.get(name).cloned();
        }
    }
    // a bare, resolvable state path
    crate::set_op::resolve_type(a, &ctx.state).cloned()
}
```
Then, in the arity/type region of the `@ref` pass, when `r.call` is `Some` AND arg count == param count (only check types when arity is right — a wrong arity already reported), compare each arg to its positional param:
```rust
if let (Some(call), Some(params)) = (r.call.as_ref(), ctx.def_params.get(&r.name)) {
    if call.args.len() == params.len() {
        for (arg_span, (_pname, pty)) in call.args.iter().zip(params.iter()) {
            // map the arg span (relative to slot.raw) to its raw text
            let raw = &slot.raw[arg_span.byte_start..arg_span.byte_end];
            if let Some(at) = resolve_arg_type(raw, ctx) {
                if !compatible(&at, &ExpectedType::Ty(pty.clone())) {
                    diags.push(diag(
                        "E-REF-ARG-TYPE",
                        format!(
                            "argument to `@{}` produces {} but the parameter expects {} (dsl §8.1)",
                            r.name, ty_desc(&at), ty_desc(pty)
                        ),
                        map_span(slot, *arg_span),
                    ));
                }
            }
        }
    }
}
```
NOTE: `arg_span` byte offsets are relative to `slot.raw` (that is what `scan_refs` runs on) — index `slot.raw` directly, and use `map_span(slot, *arg_span)` for the diagnostic (the same mapping the `@ref` span uses). Verify `map_span`'s signature at the call site.

- [ ] **Step 4: Run tests**

Run: `cargo test -p lute-check --test ref_type`
Expected: all pass (mismatch flags; match & unresolvable & nested-`@ref` cases clean).

- [ ] **Step 5: Add a committed positive fixture + gate**

`docs/examples/param-def.lute` — an inline parameterized def CALLED correctly (arity + arg type + position type all valid), must be exit 0:
```
---
character: demo
season: 1
episode: 1
state:
  scene.score: { type: number, default: 0 }
  scene.flag: { type: bool, default: false }
defs:
  atLeast: { type: bool, params: { n: number }, cel: "scene.score >= 1" }
---

## Shot 1.

<match on="scene.flag">
  <when test="@atLeast(3)">:line[narrator]: high enough
</when>
  <otherwise>:line[narrator]: not yet
</otherwise>
</match>
```
Run: `cargo build -p lute-cli && ./target/debug/lute check docs/examples/param-def.lute; echo "exit=$?"`
Expected: `ok: …/param-def.lute`, `exit=0`. (If the def's own `cel` or the `@atLeast(3)` call raises anything, STOP and fix — the fixture MUST be clean. Confirm the def's `cel` string is not itself walked as a slot; if the inline `params:` extraction needs a specific YAML shape, adjust to match `params_from_yaml`.)

- [ ] **Step 6: Document the §8.1 checker surface**

In `docs/proposals/scenario-dsl/0.0.1.md` §8.1 (~lines 300-313), add a short NOTE listing the static checks the checker enforces for `@ref`/`@name(args)`: name declared (`E-UNDECLARED-REF`), arity (`E-REF-ARITY`), per-argument type (`E-REF-ARG-TYPE`), and whole-slot produced-type (`E-REF-TYPE`); and that macro EXPANSION is engine-owned. Keep it to a few lines, consistent with the surrounding prose.

- [ ] **Step 7: fmt + clippy + commit**

```bash
cd /Users/journey/Workspace/lute/.worktrees/lute-lsp-rust
cargo fmt -p lute-check && cargo clippy -p lute-check --all-targets -- -D warnings
git add crates/lute-check/src/cel_resolve.rs crates/lute-check/tests/ref_type.rs docs/examples/param-def.lute docs/proposals/scenario-dsl/0.0.1.md
git commit -m "feat(check,examples): E-REF-ARG-TYPE for @name(args) argument types + fixture (dsl §8.1)"
```

---

## Verification (full-workspace gate — run after all tasks + whole-branch review)

```bash
export PATH="$HOME/.cargo/bin:$PATH" && cd /Users/journey/Workspace/lute/.worktrees/lute-lsp-rust
cargo test --workspace                                   # 0 failed (adds: plugin_defs_disk 2, ref_type new cases, scan_refs new cases, divergence 5)
cargo clippy --workspace --all-targets -- -D warnings    # 0
cargo fmt --check                                        # clean
(cd tree-sitter-lute && npx --yes tree-sitter-cli@latest test)   # 25/25 (capabilityVersion UNCHANGED)
cargo test -p lute-manifest --test tree_sitter_stamp     # pass (no core drift)
./target/debug/lute check docs/examples/bianca-s01ep02.lute                                        # exit 0
./target/debug/lute check docs/examples/date-minigame.lute                                         # exit 1 (core-only)
./target/debug/lute check docs/examples/date-minigame.lute --project docs/examples/idola-project   # exit 0
./target/debug/lute check docs/examples/idola-portrait.lute --project docs/examples/idola-project  # exit 0
./target/debug/lute check docs/examples/carry-ep.lute                                               # exit 0
./target/debug/lute check docs/examples/plugin-def.lute --project docs/examples/plugindef-project   # exit 0 (P1.1)
./target/debug/lute check docs/examples/param-def.lute                                              # exit 0 (P2.4)
```

## Self-Review (checklist)

1. **Spec coverage:** §8.1 `@name`/`@name(args)` — name declared (existing `E-UNDECLARED-REF`), arity (`E-REF-ARITY`, P2.3), position type (`E-REF-TYPE` extended to call form, P2.2), argument types (`E-REF-ARG-TYPE`, P2.4). Plugin def-export path proven + guarded (P1.1/P1.2). Macro expansion explicitly OUT (engine).
2. **Placeholder scan:** none — every step carries concrete code/commands.
3. **Type consistency:** `DefParam { name, ty }` (schema.rs), `RefUse.call: Option<Call>` / `Call { span, args: Vec<Span> }` (lute-cel), `Ctx::def_params: BTreeMap<String, Vec<(String, Type)>>` (ctx.rs), reused `compatible(produced, &ExpectedType::Ty(pty))` / `ty_desc` / `set_op::resolve_type` / `map_span` — names identical across P2.1→P2.4.
