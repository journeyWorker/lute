# Lute Plugin System (from-disk) + Checker Hardening — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Load capability plugins from disk (`plugins/<id>/`), resolve a profile into ONE multi-plugin capability snapshot the checker/LSP/CLI all consume, so `docs/examples/date-minigame.lute` — with the `idola.minigame` plugin present — validates **clean** (`ok:true`); and first fix four real checker `defassign`×`exhaustiveness` bugs (Group D) that the plugin work would otherwise build on top of.

**Architecture:** The `lute.core` capability already ships embedded (`load_core_snapshot`, `include_str!`). This plan adds: (1) a disk **loader** that reads a plugin package into an in-memory `LoadedPlugin`; (2) an **installed-plugin registry** the resolver walks for dependency closure (plugin §11.1 step 6) and typed-option deep-merge (§11.2); (3) a **snapshot assembler** that merges every *active* plugin's package into one `CapabilitySnapshot` stamped with `capabilityVersion`; (4) a **checker integration** that expands each active directive's `state.declares[]` (§8) into the `StateSchema` so plugin-declared slots (`scene.minigame.<key>.*`) resolve; (5) **load-time validation** (dup-id / depends-cycle / unresolved-depends / inactive-plugin fix-it); (6) **surface wiring** so the CLI (`--project`) and LSP (project discovery) build the same snapshot from one shared resolution helper. All of it preserves the two inviolable invariants: **snapshot is SoT** (no hardcoded vocab) and **no divergence** (CLI == LSP through one `check()`).

**Tech Stack:** Rust (workspace, rustc 1.96.1), `serde` + `serde_yaml` 0.9, `sha2` (capability hash), `cel-parser` 0.10.1, `insta` (goldens), `clap` 4 (CLI), `tower-lsp-server` 0.23.0 (LSP). tree-sitter is **out of scope** for this plan (grammar is data-not-grammar; `capabilityVersion` re-stamp of tree-sitter artifacts is deferred — see Deferred scope).

## Global Constraints

- **rustup stable 1.96.1** via `~/.cargo/bin`. Every fresh shell: `export PATH="$HOME/.cargo/bin:$PATH"`. NEVER `brew install rust`.
- **Worktree is authoritative.** All work happens in `/Users/journey/Workspace/lute/.worktrees/lute-lsp-rust` on branch `feat/lute-lsp-rust`. **HARNESS QUIRK:** `write`/`edit` resolve RELATIVE paths against the MAIN workspace (`~/Workspace/lute`), NOT the worktree. **Always use ABSOLUTE paths under the worktree.** After every commit, verify `git status` clean in BOTH the worktree AND `~/Workspace/lute`.
- **TDD, tester-first.** Every task: write the failing test FIRST, run it to confirm the exact expected failure, implement the minimal fix, run to green. Group D (Phase 1) is strict tester-first because it carries the highest regression risk to the current **204-passing** suite.
- **Format touched files.** Run `cargo fmt -p <crate>` (or `cargo fmt --check` to confirm) on every crate you touch so `cargo fmt --check` stays clean workspace-wide.
- **Own-crate tests during a task**, full-workspace gate only at phase/plan end: `cargo test -p <crate>`; the final gate runs `cargo test --workspace`.
- **Snapshot is SoT (inviolable #1):** no directive/enum/attr/state vocabulary is ever hardcoded in the checker/CLI/LSP. All vocabulary flows from a `CapabilitySnapshot`. New code obeys this too.
- **No divergence (inviolable #2):** CLI and LSP MUST produce byte-identical diagnostics/positions via ONE `check()`, ONE `TextIndex::position`, ONE sort `(byte_start, code)`. Any new snapshot-building logic MUST live in a **shared** `lute-manifest` function both surfaces call — never duplicated per surface. `lute-lsp/tests/divergence.rs` MUST stay green.
- **Determinism (inviolable #5):** every map/set that feeds resolution or the snapshot is a `BTreeMap`/`BTreeSet` or is explicitly sorted. Resolution MUST NOT depend on filesystem iteration order (plugin §3.2): the loader sorts directory entries by byte-wise path order before merging.
- **Never panic in `check()` / loaders.** A corrupt/missing plugin dir, unparseable YAML, or a bad manifest degrades to diagnostics + a best-effort snapshot; it never aborts. (Mirrors `ProviderSet::load`.)
- **`capabilityVersion` completeness (plugin §13):** any new field added to `CapabilitySnapshot` MUST be folded into `capability_version()` under its own section marker, with a determinism test.
- **Do NOT change the core-only tests' expectations.** `date-minigame.lute` under core-only (no plugins passed) MUST remain `ok:false` (2 intentional errors) — the existing golden/CLI/divergence tests assert that. The new `ok:true` behavior is asserted by NEW plugin-loaded tests that pass the project/catalog dirs.

## Spec source-of-truth

- Plugin spec: `docs/proposals/plugin-system/0.0.1.md` (rule SoT). Sections cited inline as "plugin §N".
- Language spec: `docs/proposals/scenario-dsl/0.0.1.md`. Cited as "dsl §N".
- Reference example: `docs/examples/date-minigame.lute` + Appendix A (`idola.minigame`).

## File Structure

**Phase 1 (Group D — checker hardening), all in `crates/lute-check/src/`:**
- `check.rs` — `Node::Match` subject-ctx fix (C5); `check_match` E-UNSET-UNCOVERED gating relies on C1.
- `cel_paths.rs` — dominance-aware guard classification (C4): new `PathRole::WeakGuard`, `dominating` param on `walk`.
- `defassign.rs` — `apply_condition` ignores `WeakGuard` (C4); `walk_match` intersects on `is_exhaustive` not syntactic `<otherwise>` (C2).
- `match_check.rs` — `infer_domain` marks non-default `scene.*` nullable (C1); `check_match` scopes `E-UNSET-UNCOVERED` to non-scene tiers (C1 regression guard).

**Phase 2–6 (plugin system), all in `crates/lute-manifest/src/` unless noted:**
- `types.rs` — `Literal::Map` variant + `type_accepts` Record/Map arms + `Literal::from_yaml` (F1).
- `resolve.rs` — deep-merge for map options (F1); `InstalledPlugins` registry + dependency closure in `resolve_activation` (F2).
- `loader.rs` — **NEW.** `LoadedPlugin`, `load_plugin_dir`, `load_plugins_dir` → `InstalledPlugins`; per-package dup-id reject (§4); load errors as `LoadError`.
- `assemble.rs` — **NEW.** `assemble_snapshot(active: &[ActivePlugin], installed: &InstalledPlugins) -> (CapabilitySnapshot, Vec<AssembleError>)`; multi-plugin merge, cross-plugin dup-id reject, `capabilityVersion` stamp (§13).
- `snapshot.rs` — add `state_templates` + `inactive` (installed-but-inactive id→plugin index for the §11.2 fix-it) fields; fold `state_templates` into `capability_version`.
- `schema.rs` — `AssetKindsFile`/`FrontmatterFile`/`StateTemplate` deserializers as needed by the loader (assetKinds body itself deferred — see below).
- `project.rs` — **NEW.** `lute.project.yaml` (`profiles` graph + `defaultProfile` + `pluginsDir`) loader → `ProfileGraph` + plugins dir path; `resolve_document_snapshot(project, scene_meta) -> (CapabilitySnapshot, Vec<Diagnostic>)` — the ONE shared resolution helper both surfaces call.

**Phase 6 (checker integration), `crates/lute-check/src/`:**
- `check.rs` — new `fold_directive_slots` pre-pass (expands active `state.declares[]` into `StateSchema`); thread `snapshot.inactive` into `check_directive` for the inactive-plugin fix-it.
- `directives.rs` — emit the "activate plugin"/"change profile" fix-it on `E-UNKNOWN-DIRECTIVE` when the tag is owned by an inactive installed plugin (discharges the T4.2 documented gap).

**Phase 7 (surfaces + fixture):**
- `crates/lute-cli/src/main.rs` — `--project <dir>` flag; call `resolve_document_snapshot`.
- `crates/lute-lsp/src/backend.rs` — project discovery (walk up from doc URI to find `lute.project.yaml`); call the SAME `resolve_document_snapshot`.
- `docs/examples/idola-project/` — **NEW.** on-disk reference project: `lute.project.yaml`, `plugins/idola.minigame/` (Appendix A verbatim), `catalog/minigame.yaml` (provider snapshot with `bianca_service_01`).
- `crates/lute-cli/tests/` + `crates/lute-lsp/tests/` — plugin-loaded acceptance (date-minigame `ok:true`) + divergence under plugins.

---

## Phase 1 — Group D: checker `defassign`×`exhaustiveness` fixes (tester-first)

> Order is dependency-driven: **C5** (isolated) → **C4** (isolated) → **C1** (feeds `is_exhaustive`) → **C2** (consumes `is_exhaustive`). Each task: failing test first, minimal fix, own-crate green. After Phase 1, run `cargo test -p lute-check` (expect all green) before Phase 2.

### Task 1.1 (C5): nested `<match on="$">` must error

**Files:**
- Modify: `crates/lute-check/src/check.rs:306-321` (the `Node::Match` arm)
- Test: `crates/lute-check/tests/group_d.rs` (Create)

**Interfaces:**
- Consumes: `check(&CheckInput) -> CheckResult` (existing), `Mode::Author`, `ProviderSet::default()`, `lute_manifest::core::load_core_snapshot()`.
- Produces: nothing new; behavioral fix only.

**Root cause (verified):** for a nested `<match>`, the subject slot is checked with the *incoming* `ctx`. Inside an outer match arm that `ctx` already has `in_match=true`, so `$` in the inner `on=` escapes `E-DOLLAR-OUTSIDE-MATCH`. Per dsl §8.2, `$` is valid ONLY in `<when test>`, never in a subject.

- [ ] **Step 1: Write the failing test**

Create `crates/lute-check/tests/group_d.rs`:

```rust
//! Group D — defassign×exhaustiveness regression tests (plan 2026-07-02, Phase 1).
use lute_check::{check, CheckInput, Mode};
use lute_manifest::provider::ProviderSet;

const HDR: &str = "---\ncharacter: x\nseason: 1\nepisode: 1\n";

fn codes(text: &str) -> Vec<String> {
    let input = CheckInput {
        text: text.to_string(),
        uri: "group_d".into(),
        snapshot: lute_manifest::core::load_core_snapshot(),
        providers: ProviderSet::default(),
        mode: Mode::Author,
    };
    check(&input).diagnostics.into_iter().map(|d| d.code).collect()
}

#[test]
fn c5_nested_match_on_dollar_is_error() {
    let t = format!(
        "{HDR}state:\n  scene.g: {{ type: bool, default: false }}\n---\n## Shot 1.\n\
         <match on=\"scene.g\">\n\
         <when test=\"$ == true\">\n\
           <match on=\"$\">\n\
           <otherwise>:line[narrator]: a\n</otherwise>\n\
           </match>\n\
         </when>\n\
         <otherwise>:line[narrator]: b\n</otherwise>\n\
         </match>\n"
    );
    assert!(
        codes(&t).contains(&"E-DOLLAR-OUTSIDE-MATCH".to_string()),
        "nested `<match on=\"$\">` must report E-DOLLAR-OUTSIDE-MATCH (dsl §8.2); got {:?}",
        codes(&t)
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p lute-check --test group_d c5_nested_match_on_dollar_is_error`
Expected: FAIL — assertion fails, `codes` is `[]` (subject `$` currently accepted).

- [ ] **Step 3: Fix — check the subject with `in_match=false`**

In `crates/lute-check/src/check.rs`, the `Node::Match(m)` arm currently does:

```rust
                Node::Match(m) => {
                    self.diags.extend(check_match(m, &ctx.state, ctx));
                    // Subject is evaluated OUTSIDE match scope (`$` is not itself
                    // a valid subject token), so check it with the base ctx.
                    self.diags
                        .extend(check_cel_slot(&m.subject, self.arena, ctx));
```

Replace the subject-check line so the subject is evaluated with `in_match` forced OFF (a subject is never inside match scope, even when the whole `<match>` is nested in an outer arm):

```rust
                Node::Match(m) => {
                    self.diags.extend(check_match(m, &ctx.state, ctx));
                    // The subject expression is evaluated OUTSIDE match scope: `$`
                    // is only valid in a `<when test>` (dsl §8.2), never in `on=`.
                    // Force `in_match=false` so a nested `<match on="$">` (whose
                    // incoming ctx has in_match=true from the enclosing arm) is
                    // correctly flagged E-DOLLAR-OUTSIDE-MATCH.
                    let subject_ctx = Ctx {
                        in_match: false,
                        match_subject: None,
                        ..ctx.clone()
                    };
                    self.diags
                        .extend(check_cel_slot(&m.subject, self.arena, &subject_ctx));
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p lute-check --test group_d c5_nested_match_on_dollar_is_error`
Expected: PASS.

- [ ] **Step 5: Regression — run the whole crate**

Run: `cargo test -p lute-check`
Expected: all pass (no test relied on nested-subject `$` being accepted).

- [ ] **Step 6: Format + commit**

```bash
cargo fmt -p lute-check
git add crates/lute-check/src/check.rs crates/lute-check/tests/group_d.rs
git commit -m "fix(check): reject \$ in nested <match> subject (dsl §8.2, C5)"
```

### Task 1.2 (C4): disjunctive/negated presence guards must not prove reads

**Files:**
- Modify: `crates/lute-check/src/cel_paths.rs:26-33` (add `PathRole::WeakGuard`), `:63-126` (`walk` gains a `dominating` param)
- Modify: `crates/lute-check/src/defassign.rs:215-232` (`apply_condition` ignores `WeakGuard`)
- Test: `crates/lute-check/tests/group_d.rs` (append)

**Interfaces:**
- Consumes: `collect_path_uses(&Expr) -> Vec<PathUse>` (existing, signature unchanged — top-level call is dominating), `PathUse { path, role }`.
- Produces: `PathRole::WeakGuard` (a presence test in a non-dominating position: proves nothing, but the path is still surfaced so read-site declaration checks in `cel_resolve` are unaffected).

**Root cause (verified):** `walk` classifies `isSet(p)`/`has(p)` as `PathRole::Guard` regardless of boolean context. Operators desugar to `Expr::Call` (`_||_`, `_&&_`, `!_`), handled by the generic `Call` arm with no dominance tracking. So in `isSet(run.x) || run.x > 0`, `isSet(run.x)` emits `Guard`; `apply_condition` inserts it into the arm-assigned set; the sibling `run.x > 0` read is then falsely proven → `E-MAYBE-UNSET` suppressed. A guard only *dominates* (proves) at the top level and recursively through `&&`; under `||` or `!` it proves nothing (dsl §9.4: a read is proven by *an enclosing guard*, i.e. one that gates the body).

- [ ] **Step 1: Write the failing test (append to `group_d.rs`)**

```rust
#[test]
fn c4_disjunctive_guard_does_not_prove_read() {
    // isSet(run.x) is under `||`, so it does NOT prove `run.x`; the `run.x > 0`
    // read of a non-defaulted run tier is E-MAYBE-UNSET (dsl §9.4).
    let t = format!(
        "{HDR}state:\n  run.x: {{ type: number }}\n  scene.y: {{ type: bool, default: false }}\n---\n## Shot 1.\n\
         <match on=\"scene.y\">\n\
         <when test=\"isSet(run.x) || run.x > 0\">:line[narrator]: a\n</when>\n\
         <otherwise>:line[narrator]: b\n</otherwise>\n\
         </match>\n"
    );
    assert!(
        codes(&t).contains(&"E-MAYBE-UNSET".to_string()),
        "disjunctive guard must NOT prove the read (C4); got {:?}",
        codes(&t)
    );
}

#[test]
fn c4_conjunctive_guard_still_proves_read() {
    // Regression guard: a top-level / conjunctive `isSet` MUST still prove.
    let t = format!(
        "{HDR}state:\n  run.x: {{ type: number }}\n  scene.y: {{ type: bool, default: false }}\n---\n## Shot 1.\n\
         <match on=\"scene.y\">\n\
         <when test=\"isSet(run.x) && run.x > 0\">:line[narrator]: a\n</when>\n\
         <otherwise>:line[narrator]: b\n</otherwise>\n\
         </match>\n"
    );
    assert!(
        !codes(&t).contains(&"E-MAYBE-UNSET".to_string()),
        "conjunctive isSet must still prove the read; got {:?}",
        codes(&t)
    );
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p lute-check --test group_d c4_`
Expected: `c4_disjunctive_guard_does_not_prove_read` FAILS (no `E-MAYBE-UNSET` today); `c4_conjunctive_guard_still_proves_read` PASSES already (guards prove today). Both must pass after the fix.

- [ ] **Step 3: Add `WeakGuard` to `PathRole`**

In `crates/lute-check/src/cel_paths.rs`, extend the enum (currently `Read`, `Guard`):

```rust
/// How a state path appears in an expression.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PathRole {
    /// An ordinary value read (subject to definite-assignment, dsl §9.4).
    Read,
    /// A presence test (`has(p)`/`isSet(p)`) in a **dominating** position (top
    /// level or a conjunct of `&&`): it proves the path for the guarded body.
    Guard,
    /// A presence test in a **non-dominating** position (under `||`/`!`): it
    /// proves nothing (dsl §9.4). The path is still surfaced so read-site
    /// declaration checks are unaffected, but definite-assignment ignores it.
    WeakGuard,
}
```

- [ ] **Step 4: Thread `dominating` through `walk`**

In `crates/lute-check/src/cel_paths.rs`, change `collect_path_uses` to start dominating, and give `walk` a `dominating: bool` parameter. The `_&&_` call keeps dominance for its args; `_||_` and `!_` drop it; every other call/operand recursion drops it too (a guard buried in an arbitrary sub-expression does not gate the body). Presence guards emit `Guard` when `dominating`, else `WeakGuard`.

```rust
pub(crate) fn collect_path_uses(expr: &Expr) -> Vec<PathUse> {
    let mut out = Vec::new();
    walk(expr, true, &mut out);
    out
}

fn walk(expr: &Expr, dominating: bool, out: &mut Vec<PathUse>) {
    match expr {
        Expr::Ident(name) => push_path(out, name.clone(), PathRole::Read),
        Expr::Select(sel) => {
            // A test-only Select is the `has(p)` macro (dsl §9.4 guard).
            let role = if sel.test {
                if dominating { PathRole::Guard } else { PathRole::WeakGuard }
            } else {
                PathRole::Read
            };
            if let Some(path) = select_path(expr) {
                push_path(out, path, role);
            } else {
                walk(&sel.operand.expr, false, out);
            }
        }
        Expr::Call(call) => {
            // `isSet(p)` — a DSL presence guard whose single arg is a static path.
            if call.target.is_none()
                && call.func_name.eq_ignore_ascii_case("isSet")
                && call.args.len() == 1
            {
                if let Some(path) = select_path(&call.args[0].expr) {
                    if is_state_path(&path) {
                        let role = if dominating { PathRole::Guard } else { PathRole::WeakGuard };
                        push_path(out, path, role);
                        return;
                    }
                }
            }
            // Boolean structure controls dominance: `&&` preserves it for both
            // args; `||` and `!` (and any other call/operand) drop it.
            let child_dom = dominating && call.target.is_none() && call.func_name == "_&&_";
            if let Some(target) = &call.target {
                walk(&target.expr, false, out);
            }
            for arg in &call.args {
                walk(&arg.expr, child_dom, out);
            }
        }
        Expr::List(list) => {
            for el in &list.elements {
                walk(&el.expr, false, out);
            }
        }
        Expr::Map(map) => {
            for entry in &map.entries {
                walk_entry(&entry.expr, out);
            }
        }
        Expr::Struct(st) => {
            for entry in &st.entries {
                walk_entry(&entry.expr, out);
            }
        }
        Expr::Comprehension(c) => {
            walk(&c.iter_range.expr, false, out);
            walk(&c.accu_init.expr, false, out);
            walk(&c.loop_cond.expr, false, out);
            walk(&c.loop_step.expr, false, out);
            walk(&c.result.expr, false, out);
        }
        Expr::Literal(_) | Expr::Unspecified => {}
    }
}
```

Update `walk_entry` calls (which recurse with default non-dominating context):

```rust
fn walk_entry(entry: &EntryExpr, out: &mut Vec<PathUse>) {
    match entry {
        EntryExpr::MapEntry(m) => {
            walk(&m.key.expr, false, out);
            walk(&m.value.expr, false, out);
        }
        EntryExpr::StructField(f) => walk(&f.value.expr, false, out),
    }
}
```

- [ ] **Step 5: `apply_condition` ignores `WeakGuard`**

In `crates/lute-check/src/defassign.rs`, the `PathRole` match currently handles `Read` and `Guard`. Add the `WeakGuard` arm (proves nothing; the read-site declaration check is `cel_resolve`'s job, not defassign's):

```rust
    for use_ in slot_uses(slot) {
        match use_.role {
            PathRole::Read => check_read(&use_.path, schema, assigned, slot.span, diags),
            PathRole::Guard => {
                // A guard on an undeclared path is a read-site concern (T4.3).
                if is_declared(&use_.path, schema) && !is_choicelog(&use_.path) {
                    assigned.insert(use_.path);
                }
            }
            // A non-dominating presence test (under `||`/`!`) proves nothing.
            PathRole::WeakGuard => {}
        }
    }
```

Also check `crates/lute-check/src/cel_resolve.rs` for any exhaustive `match use_.role` — if `collect_path_uses` results are matched there by role, add a `WeakGuard` arm treated like `Guard`/`Read` for declaration purposes. (Grep `PathRole::` across `lute-check/src` before compiling; the compiler will flag any non-exhaustive match.)

- [ ] **Step 6: Run tests + whole crate**

Run: `cargo test -p lute-check`
Expected: both `c4_` tests PASS; all prior tests still pass.

- [ ] **Step 7: Format + commit**

```bash
cargo fmt -p lute-check
git add crates/lute-check/src/cel_paths.rs crates/lute-check/src/defassign.rs crates/lute-check/tests/group_d.rs
git commit -m "fix(check): non-dominating presence guards don't prove reads (dsl §9.4, C4)"
```

### Task 1.3 (C1): non-default `scene.*` match subjects are nullable

**Files:**
- Modify: `crates/lute-check/src/match_check.rs:263-292` (`infer_domain` `maybe_unset`), `:157-167` (`check_match` `E-UNSET-UNCOVERED` gating)
- Test: `crates/lute-check/tests/group_d.rs` (append)

**Interfaces:**
- Consumes: `StateSchema`, `StateDecl { ty, default, namespace }`, `Namespace`.
- Produces: `infer_domain` now reports `maybe_unset=true` for a non-default `scene.*` (non-`scene.choices`) subject; `is_exhaustive` (already reads `info.maybe_unset`) therefore returns `false` for such a subject unless `unset`/`<otherwise>` is covered — this is what un-suppresses C1 and is consumed by C2 (Task 1.4).

**Root cause (verified):** `infer_domain` sets `maybe_unset` only for `Run|User|App` with no default; a non-default `scene.bool` gets `maybe_unset=false`, so `is_exhaustive` returns `true` when `{true,false}` are covered, and `suppress_exhaustive_subject_reads` then DROPS the legitimate `E-MAYBE-UNSET` that `defassign` emitted for the unset subject read (false-negative). Per dsl §9.4 a `scene.*` read follows ordinary path-sensitivity — an unassigned read IS an error.

**Regression trap:** `check_match` also emits `E-UNSET-UNCOVERED` from `info.maybe_unset && !covers_unset`. `check_match` is NOT path-sensitive (it can't see that a `scene.*` subject was `::set` before the match — the C1b case). If we let `E-UNSET-UNCOVERED` fire for `scene.*`, the *written* case (C1b) becomes a false-positive. Fix: scope `check_match`'s `E-UNSET-UNCOVERED` to the tiers whose maybe-unset-at-entry is *schema-derivable* (`run`/`user`/`app`, and `scene.choices.*`); leave plain `scene.*` maybe-unset to the path-sensitive `defassign` pass (which is now correctly un-suppressed).

- [ ] **Step 1: Write the failing tests (append to `group_d.rs`)**

```rust
#[test]
fn c1_scene_bool_unwritten_subject_is_maybe_unset() {
    // Non-default scene.bool, never written, match covers {true,false}, NO otherwise.
    // The unset subject read must NOT be suppressed (dsl §9.4).
    let t = format!(
        "{HDR}state:\n  scene.flag: {{ type: bool }}\n---\n## Shot 1.\n\
         <match on=\"scene.flag\">\n\
         <when test=\"$ == true\">:line[narrator]: a\n</when>\n\
         <when test=\"$ == false\">:line[narrator]: b\n</when>\n\
         </match>\n"
    );
    assert!(
        codes(&t).contains(&"E-MAYBE-UNSET".to_string()),
        "unwritten non-default scene subject must be E-MAYBE-UNSET (C1); got {:?}",
        codes(&t)
    );
}

#[test]
fn c1b_scene_bool_written_subject_is_clean() {
    // REGRESSION GUARD: the same subject, WRITTEN before the match, is clean —
    // no E-MAYBE-UNSET (proven) and no false-positive E-UNSET-UNCOVERED.
    let t = format!(
        "{HDR}state:\n  scene.flag: {{ type: bool }}\n---\n## Shot 1.\n\
         ::set{{scene.flag = true}}\n\
         <match on=\"scene.flag\">\n\
         <when test=\"$ == true\">:line[narrator]: a\n</when>\n\
         <when test=\"$ == false\">:line[narrator]: b\n</when>\n\
         </match>\n"
    );
    let c = codes(&t);
    assert!(!c.contains(&"E-MAYBE-UNSET".to_string()), "written subject must be proven; got {c:?}");
    assert!(!c.contains(&"E-UNSET-UNCOVERED".to_string()), "written scene subject must not need an unset arm; got {c:?}");
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p lute-check --test group_d c1`
Expected: `c1_scene_bool_unwritten_subject_is_maybe_unset` FAILS (`[]` today); `c1b_...` PASSES today (both errors currently absent). Both must pass after the fix.

- [ ] **Step 3: `infer_domain` — non-default `scene.*` is nullable**

In `crates/lute-check/src/match_check.rs`, the `Some(decl)` arm computes `maybe_unset` from `Run|User|App`. Broaden it so a non-default `scene.*` decl (the leading tier is `Scene`; `scene.choices.*` already handled earlier in the function) is also nullable:

```rust
            let maybe_unset = decl.default.is_none()
                && matches!(
                    decl.namespace,
                    Namespace::Scene | Namespace::Run | Namespace::User | Namespace::App
                );
```

- [ ] **Step 4: `check_match` — scope `E-UNSET-UNCOVERED` off path-sensitive `scene.*`**

In `crates/lute-check/src/match_check.rs`, the `E-UNSET-UNCOVERED` block is `if info.maybe_unset && !covers_unset`. `check_match` cannot prove a plain `scene.*` write, so its unset coverage for that tier would be a false-positive (C1b). Gate it to the schema-derivable-nullable subjects; plain `scene.*` maybe-unset is owned by the path-sensitive `defassign` pass:

```rust
    // A maybe-unset subject's `unset` case must be covered (§11.2/§9.4). This is
    // scoped to subjects whose nullability is derivable from the schema alone —
    // `run`/`user`/`app` (maybe-unset at scene entry) and `scene.choices.*` (a
    // branch may not have run). A plain `scene.*` subject's maybe-unset status is
    // path-sensitive; it is owned by `check_definite_assignment` (E-MAYBE-UNSET),
    // so emitting E-UNSET-UNCOVERED here would false-positive the written case.
    let unset_owned_here = subject
        .as_deref()
        .map(|p| p.starts_with("scene.choices.") || !p.starts_with("scene."))
        .unwrap_or(false);
    if info.maybe_unset && unset_owned_here && !covers_unset {
        diags.push(diag(
            "E-UNSET-UNCOVERED",
            Severity::Error,
            "maybe-unset `<match>` subject: the `unset` case is not covered by an `unset` arm or \
             an `<otherwise>` (dsl §11.2)"
                .to_string(),
            m.span,
        ));
    }
```

> Note: `is_exhaustive` (lines 209-231) already ANDs `(!info.maybe_unset || covers_unset)`; with C1 making a non-default `scene.*` subject `maybe_unset`, `is_exhaustive` now returns `false` for the C1 fixture, so `suppress_exhaustive_subject_reads` no longer drops the `defassign` `E-MAYBE-UNSET`. Do NOT gate `is_exhaustive` — its broader nullability is exactly what C1 needs and C2 consumes.

- [ ] **Step 5: Run tests + whole crate**

Run: `cargo test -p lute-check`
Expected: `c1` + `c1b` PASS; all prior tests pass. If any pre-existing `match_check`/`examples` test regresses, it is exercising the exact C1/C1b behavior — reconcile against the spec (§9.4/§11.2) before proceeding; do NOT loosen the fix to paper over a real spec-required change.

- [ ] **Step 6: Format + commit**

```bash
cargo fmt -p lute-check
git add crates/lute-check/src/match_check.rs crates/lute-check/tests/group_d.rs
git commit -m "fix(check): non-default scene.* match subjects are nullable (dsl §9.4/§11.2, C1)"
```

### Task 1.4 (C2): `walk_match` folds on `is_exhaustive`, not syntactic `<otherwise>`

**Files:**
- Modify: `crates/lute-check/src/defassign.rs:159-188` (`walk_match`)
- Test: `crates/lute-check/tests/group_d.rs` (append)

**Interfaces:**
- Consumes: `crate::match_check::is_exhaustive(&Match, &StateSchema) -> bool` (already `pub`, re-exported via `lute_check::is_exhaustive`; call the module path `crate::match_check::is_exhaustive`).
- Produces: nothing new.

**Root cause (verified):** `walk_match` intersects arm-final assigned-sets into the surviving set ONLY when it sees a syntactic `Arm::Otherwise`. A domain-exhaustive match without `<otherwise>` (e.g. bool `{true,false}` both arms assign `scene.x`) is treated as a possible no-match fall-through, so the post-block set stays the pre-block set → a later read of `scene.x` false-positives `E-MAYBE-UNSET`. Coverage should follow `is_exhaustive` (which, post-C1, correctly accounts for nullable subjects).

- [ ] **Step 1: Write the failing test (append to `group_d.rs`)**

```rust
#[test]
fn c2_exhaustive_match_without_otherwise_folds_assignment() {
    // Domain-exhaustive bool match (default false so not maybe-unset), both arms
    // assign scene.x; the read AFTER the match must be proven (no E-MAYBE-UNSET).
    let t = format!(
        "{HDR}state:\n  scene.g: {{ type: bool, default: false }}\n  scene.x: {{ type: number }}\n---\n## Shot 1.\n\
         <match on=\"scene.g\">\n\
         <when test=\"$ == true\">::set{{scene.x = 1}}\n</when>\n\
         <when test=\"$ == false\">::set{{scene.x = 2}}\n</when>\n\
         </match>\n\
         ::set{{scene.x += 5}}\n"
    );
    assert!(
        !codes(&t).contains(&"E-MAYBE-UNSET".to_string()),
        "exhaustive-without-otherwise both-arms-assign must fold (C2); got {:?}",
        codes(&t)
    );
}

#[test]
fn c2b_nonexhaustive_match_does_not_fold_assignment() {
    // REGRESSION GUARD: a NON-exhaustive match (one arm only, no otherwise) must
    // NOT fold — the read after is genuinely maybe-unset.
    let t = format!(
        "{HDR}state:\n  scene.g: {{ type: bool, default: false }}\n  scene.x: {{ type: number }}\n---\n## Shot 1.\n\
         <match on=\"scene.g\">\n\
         <when test=\"$ == true\">::set{{scene.x = 1}}\n</when>\n\
         </match>\n\
         ::set{{scene.x += 5}}\n"
    );
    assert!(
        codes(&t).contains(&"E-MAYBE-UNSET".to_string()),
        "non-exhaustive match must NOT fold the assignment; got {:?}",
        codes(&t)
    );
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p lute-check --test group_d c2`
Expected: `c2_exhaustive_...` FAILS (has `E-MAYBE-UNSET` today); `c2b_nonexhaustive_...` PASSES today. Both must pass after the fix.

- [ ] **Step 3: Fix — fold on `is_exhaustive`**

In `crates/lute-check/src/defassign.rs`, `walk_match` uses a local `exhaustive` bool set only on `Arm::Otherwise`. Replace it with `is_exhaustive`:

```rust
fn walk_match(
    m: &Match,
    schema: &StateSchema,
    assigned: &mut Assigned,
    diags: &mut Vec<Diagnostic>,
) {
    // Subject is a value-read check only; subject-position guards do NOT prove.
    check_reads(&m.subject, schema, assigned, diags);

    let mut arm_finals: Vec<Assigned> = Vec::new();
    for arm in &m.arms {
        let mut branch = assigned.clone();
        match arm {
            Arm::When { test, body, .. } => {
                apply_condition(test, schema, &mut branch, diags);
                walk_nodes(body, schema, &mut branch, diags);
            }
            Arm::Otherwise { body, .. } => {
                walk_nodes(body, schema, &mut branch, diags);
            }
        }
        arm_finals.push(branch);
    }
    // Fold the arms' assignments into the surviving set iff the match is
    // exhaustive (a covered finite/nullable domain, or an `<otherwise>`): every
    // path then flows through exactly one arm, so the intersection of arm-final
    // sets is provably assigned afterward. A non-exhaustive match may match
    // nothing, so its pre-block set survives unchanged (dsl §9.4/§11.2).
    if !arm_finals.is_empty() && crate::match_check::is_exhaustive(m, schema) {
        *assigned = intersect_all(arm_finals);
    }
}
```

- [ ] **Step 4: Run tests + whole crate**

Run: `cargo test -p lute-check`
Expected: `c2` + `c2b` PASS; all prior tests (incl. Task 1.1-1.3) pass. Re-run `date-minigame` core-only expectation via the existing golden/examples tests — they MUST be unchanged.

- [ ] **Step 5: Format + commit**

```bash
cargo fmt -p lute-check
git add crates/lute-check/src/defassign.rs crates/lute-check/tests/group_d.rs
git commit -m "fix(check): fold match assignments on is_exhaustive not syntactic <otherwise> (dsl §11.2, C2)"
```

### Task 1.5: Phase 1 gate

- [ ] **Step 1: Full workspace suite + clippy + fmt**

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
```
Expected: all green; test count = 204 (prior) + 7 new Group D tests = **211 passed** (adjust if a pre-existing test legitimately changed — document any such change in the commit and the ledger).

- [ ] **Step 2: Confirm both examples still behave**

```bash
./target/debug/lute check docs/examples/bianca-s01ep02.lute --json   # ok:true, exit 0
./target/debug/lute check docs/examples/date-minigame.lute --json    # ok:false, exit 1 (core-only, unchanged)
```

- [ ] **Step 3: Verify clean trees**

```bash
git status --short
(cd /Users/journey/Workspace/lute && git status --short)
```
Expected: both empty.

---

## Phase 2 — Manifest data model: `Literal::Map` (F1)

> plugin §7/§11.2: typed options and shape/record/map field defaults need a map/record literal, and map option values must deep-merge across profile layers. Today `Literal` is `Bool|Num|Str|List` only.

### Task 2.1: `Literal::Map` variant + `type_accepts` Record/Map arms

**Files:**
- Modify: `crates/lute-manifest/src/types.rs:38-45` (`Literal`), `:66-75` (`type_accepts`)
- Test: `crates/lute-manifest/src/types.rs` (`#[cfg(test)] mod tests`, append)

**Interfaces:**
- Consumes: `Type::{Record(Vec<Field>), Map { key, value }}` (existing), `Field { name, ty, default, required, shape }`.
- Produces: `Literal::Map(BTreeMap<String, Literal>)`; `type_accepts` now returns `true` for a `Record`/`Map` type against a `Literal::Map` whose entries satisfy the field/value types.

- [ ] **Step 1: Write the failing test (append to `types.rs` tests)**

```rust
    #[test]
    fn type_accepts_record_literal() {
        use std::collections::BTreeMap;
        let ty = Type::Record(vec![
            Field { name: "costume".into(), ty: Type::Str, default: None, required: true, shape: None },
            Field { name: "sealed".into(), ty: Type::Bool, default: Some(Literal::Bool(false)), required: false, shape: None },
        ]);
        let mut m = BTreeMap::new();
        m.insert("costume".to_string(), Literal::Str("waitress".into()));
        m.insert("sealed".to_string(), Literal::Bool(true));
        assert!(type_accepts(&ty, &Literal::Map(m)));

        // missing required field -> reject
        let mut bad = BTreeMap::new();
        bad.insert("sealed".to_string(), Literal::Bool(true));
        assert!(!type_accepts(&ty, &Literal::Map(bad)));
    }

    #[test]
    fn type_accepts_map_literal() {
        use std::collections::BTreeMap;
        let ty = Type::Map { key: Box::new(Type::Str), value: Box::new(Type::Number) };
        let mut m = BTreeMap::new();
        m.insert("a".to_string(), Literal::Num(1.0));
        m.insert("b".to_string(), Literal::Num(2.0));
        assert!(type_accepts(&ty, &Literal::Map(m)));

        let mut bad = BTreeMap::new();
        bad.insert("a".to_string(), Literal::Str("x".into())); // value type mismatch
        assert!(!type_accepts(&ty, &Literal::Map(bad)));
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p lute-manifest --lib type_accepts_`
Expected: FAIL to compile (`Literal::Map` unknown).

- [ ] **Step 3: Add the variant**

In `crates/lute-manifest/src/types.rs`, `Literal` is `#[serde(untagged)]`. A YAML mapping (both `record` and `map` literals serialize as a mapping) deserializes to a `BTreeMap<String, Literal>`. Add the arm LAST so untagged tries scalars/list first:

```rust
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Literal {
    Bool(bool),
    Num(f64),
    Str(String),
    List(Vec<Literal>),
    /// A record/map literal (plugin §7): a YAML mapping. Deterministic order.
    Map(std::collections::BTreeMap<String, Literal>),
}
```

- [ ] **Step 4: Extend `type_accepts`**

```rust
pub fn type_accepts(ty: &Type, lit: &Literal) -> bool {
    match (ty, lit) {
        (Type::Bool, Literal::Bool(_)) => true,
        (Type::Number, Literal::Num(_)) => true,
        (Type::Str, Literal::Str(_)) => true,
        (Type::Enum(members), Literal::Str(s)) => members.iter().any(|m| m == s),
        (Type::List(inner), Literal::List(items)) => items.iter().all(|i| type_accepts(inner, i)),
        (Type::Record(fields), Literal::Map(m)) => {
            // every required field present + typed; no unknown keys.
            fields.iter().all(|f| match m.get(&f.name) {
                Some(v) => field_type_accepts(f, v),
                None => !f.required,
            }) && m.keys().all(|k| fields.iter().any(|f| &f.name == k))
        }
        (Type::Map { key, value }, Literal::Map(m)) => {
            // keys are strings in YAML; enum-typed keys checked against members.
            matches!(**key, Type::Str | Type::Enum(_))
                && m.iter().all(|(k, v)| {
                    key_accepts(key, k) && type_accepts(value, v)
                })
        }
        _ => false,
    }
}

/// A record field MAY use `shape: <name>` instead of an inline type; a shape
/// reference is validated at snapshot-assembly time (the shape registry is not
/// available here), so a `Map` literal against a shape field is accepted here.
fn field_type_accepts(f: &Field, v: &Literal) -> bool {
    if f.shape.is_some() {
        matches!(v, Literal::Map(_))
    } else {
        type_accepts(&f.ty, v)
    }
}

fn key_accepts(key: &Type, k: &str) -> bool {
    match key {
        Type::Str => true,
        Type::Enum(members) => members.iter().any(|m| m == k),
        _ => false,
    }
}
```

- [ ] **Step 5: Run tests + whole crate**

Run: `cargo test -p lute-manifest`
Expected: new tests PASS; existing `types.rs` roundtrip tests still pass (the untagged `Map` arm is last so scalars/lists are unaffected). If a roundtrip test fails because a `List` of maps now needs the `Map` arm, that's expected coverage — confirm it serializes back identically.

- [ ] **Step 6: Format + commit**

```bash
cargo fmt -p lute-manifest
git add crates/lute-manifest/src/types.rs
git commit -m "feat(manifest): Literal::Map record/map variant + type_accepts (plugin §7, F1)"
```

### Task 2.2: `Literal::from_yaml` — convert `serde_yaml::Value` → `Literal`

**Files:**
- Modify: `crates/lute-manifest/src/types.rs` (impl block near `Literal`)
- Test: `crates/lute-manifest/src/types.rs` tests

**Interfaces:**
- Produces: `pub fn Literal::from_yaml(v: &serde_yaml::Value) -> Option<Literal>` — converts a scene-frontmatter `plugins:` option value (which `parse_meta` retains as `serde_yaml::Value`) into a `Literal` for the `ActivationMap`. Consumed by the surface resolution helper (Task 7.1) so scene-local plugin options merge through `resolve_activation`.

- [ ] **Step 1: Write the failing test**

```rust
    #[test]
    fn literal_from_yaml_scalars_list_map() {
        let y: serde_yaml::Value = serde_yaml::from_str("[rhythm, timing]").unwrap();
        assert_eq!(
            Literal::from_yaml(&y),
            Some(Literal::List(vec![Literal::Str("rhythm".into()), Literal::Str("timing".into())]))
        );
        let y2: serde_yaml::Value = serde_yaml::from_str("scene").unwrap();
        assert_eq!(Literal::from_yaml(&y2), Some(Literal::Str("scene".into())));
        let y3: serde_yaml::Value = serde_yaml::from_str("{ a: 1, b: two }").unwrap();
        match Literal::from_yaml(&y3).unwrap() {
            Literal::Map(m) => {
                assert_eq!(m.get("a"), Some(&Literal::Num(1.0)));
                assert_eq!(m.get("b"), Some(&Literal::Str("two".into())));
            }
            other => panic!("expected Map, got {other:?}"),
        }
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p lute-manifest --lib literal_from_yaml_`
Expected: FAIL to compile (`from_yaml` missing).

- [ ] **Step 3: Implement `from_yaml`**

```rust
impl Literal {
    /// Convert a YAML value (e.g. a scene `plugins:` option) into a `Literal`.
    /// Returns `None` for values with no literal representation (null, tagged,
    /// non-string map keys). Numbers become `Num` (f64); sequences `List`;
    /// mappings `Map` (string keys only).
    pub fn from_yaml(v: &serde_yaml::Value) -> Option<Literal> {
        use serde_yaml::Value;
        match v {
            Value::Bool(b) => Some(Literal::Bool(*b)),
            Value::Number(n) => n.as_f64().map(Literal::Num),
            Value::String(s) => Some(Literal::Str(s.clone())),
            Value::Sequence(items) => items
                .iter()
                .map(Literal::from_yaml)
                .collect::<Option<Vec<_>>>()
                .map(Literal::List),
            Value::Mapping(m) => {
                let mut out = std::collections::BTreeMap::new();
                for (k, val) in m {
                    let key = k.as_str()?.to_string();
                    out.insert(key, Literal::from_yaml(val)?);
                }
                Some(Literal::Map(out))
            }
            Value::Null | Value::Tagged(_) => None,
        }
    }
}
```

- [ ] **Step 4: Run + commit**

Run: `cargo test -p lute-manifest` (PASS), then:

```bash
cargo fmt -p lute-manifest
git add crates/lute-manifest/src/types.rs
git commit -m "feat(manifest): Literal::from_yaml for scene-local option merge (plugin §11.2, F1)"
```

### Task 2.3: deep-merge map options in `resolve_activation` (F1)

**Files:**
- Modify: `crates/lute-manifest/src/resolve.rs:61-73` (the `apply` closure merge)
- Test: `crates/lute-manifest/src/resolve.rs` tests

**Interfaces:**
- Consumes: `Literal::Map`, `ActivationMap = BTreeMap<String, BTreeMap<String, Literal>>`.
- Produces: merge is now: scalar/list **replace**; **map deep-merge** (plugin §11.2). Later layers still win.

- [ ] **Step 1: Write the failing test (append to resolve.rs tests)**

```rust
    #[test]
    fn map_option_values_deep_merge_across_layers() {
        use crate::types::Literal;
        use std::collections::BTreeMap;
        // parent sets cast.bianca={costume:a}; child adds cast.ren={costume:b}.
        let mut parent_opt = BTreeMap::new();
        let mut cast_p = BTreeMap::new();
        cast_p.insert("bianca".to_string(), Literal::Str("a".into()));
        parent_opt.insert("cast".to_string(), Literal::Map(cast_p));
        let mut child_opt = BTreeMap::new();
        let mut cast_c = BTreeMap::new();
        cast_c.insert("ren".to_string(), Literal::Str("b".into()));
        child_opt.insert("cast".to_string(), Literal::Map(cast_c));

        let mut parent = BTreeMap::new();
        parent.insert("p.plug".to_string(), parent_opt);
        let mut child = BTreeMap::new();
        child.insert("p.plug".to_string(), child_opt);

        let graph = ProfileGraph {
            profiles: BTreeMap::from([
                ("parent".to_string(), Profile { extends: None, plugins: parent }),
                ("child".to_string(), Profile { extends: Some("parent".into()), plugins: child }),
            ]),
            default_profile: "child".to_string(),
        };
        let active = resolve_activation(&graph, "child", &BTreeMap::new()).unwrap();
        let plug = active.iter().find(|a| a.id == "p.plug").unwrap();
        match plug.options.get("cast").unwrap() {
            Literal::Map(m) => {
                assert!(m.contains_key("bianca"), "parent entry retained");
                assert!(m.contains_key("ren"), "child entry merged in");
            }
            other => panic!("expected merged Map, got {other:?}"),
        }
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p lute-manifest --lib map_option_values_deep_merge`
Expected: FAIL — child `cast` replaces parent `cast` (only `ren` present).

- [ ] **Step 3: Deep-merge in the `apply` closure**

In `crates/lute-manifest/src/resolve.rs`, the `apply` closure does `entry.insert(k.clone(), v.clone())`. Replace with a recursive merge helper:

```rust
    let apply = |acts: &ActivationMap,
                 order: &mut Vec<String>,
                 merged: &mut BTreeMap<String, BTreeMap<String, Literal>>| {
        for (id, opts) in acts {
            if !merged.contains_key(id) {
                order.push(id.clone());
            }
            let entry = merged.entry(id.clone()).or_default();
            for (k, v) in opts {
                match (entry.get_mut(k), v) {
                    // map deep-merge (plugin §11.2)
                    (Some(Literal::Map(dst)), Literal::Map(src)) => merge_map(dst, src),
                    // scalar/list replace, or type change
                    _ => {
                        entry.insert(k.clone(), v.clone());
                    }
                }
            }
        }
    };
```

Add the free helper below `resolve_activation`:

```rust
/// Recursive map deep-merge (plugin §11.2): src entries override dst; nested maps
/// recurse; scalars/lists replace.
fn merge_map(dst: &mut BTreeMap<String, Literal>, src: &BTreeMap<String, Literal>) {
    for (k, v) in src {
        match (dst.get_mut(k), v) {
            (Some(Literal::Map(d)), Literal::Map(s)) => merge_map(d, s),
            _ => {
                dst.insert(k.clone(), v.clone());
            }
        }
    }
}
```

- [ ] **Step 4: Run + commit**

Run: `cargo test -p lute-manifest` (PASS, existing scalar-override test still green), then:

```bash
cargo fmt -p lute-manifest
git add crates/lute-manifest/src/resolve.rs
git commit -m "feat(manifest): deep-merge map option values across profile layers (plugin §11.2, F1)"
```

---

## Phase 3 — Resolution: installed-plugin registry + dependency closure (F2)

> plugin §11.1 step 6: after profile/scene layers, pull in the `depends` closure of everything activated. `resolve_activation` currently stops at step 5 and has no access to plugin manifests. Introduce an `InstalledPlugins` registry (an in-memory index of every installed plugin's manifest) and thread it in.

### Task 3.1: `InstalledPlugins` registry type

**Files:**
- Modify: `crates/lute-manifest/src/resolve.rs` (new types near the top)
- Modify: `crates/lute-manifest/src/lib.rs` (no change if `resolve` already `pub`)
- Test: `crates/lute-manifest/src/resolve.rs` tests

**Interfaces:**
- Produces:
  - `pub struct InstalledPlugin { pub manifest: crate::schema::PluginManifest }` (the parsed `plugin.yaml`; the full `LoadedPlugin` package arrives in Phase 4 and will wrap/replace this — keep the closure logic keyed on `manifest.id`/`manifest.depends`/`manifest.version`).
  - `pub struct InstalledPlugins { pub by_id: BTreeMap<String, InstalledPlugin> }` with `pub fn get(&self, id: &str) -> Option<&InstalledPlugin>`.
- Consumes: `PluginManifest { id, version, kind, depends: Vec<Depends>, exports, options }`, `Depends { id, range }`.

- [ ] **Step 1: Write the failing test**

```rust
    #[test]
    fn installed_plugins_lookup() {
        use crate::schema::{Depends, PluginManifest};
        use std::collections::BTreeMap;
        let m = PluginManifest {
            id: "idola.minigame".into(),
            version: "0.1.0".into(),
            kind: "capability".into(),
            depends: vec![Depends { id: "lute.core".into(), range: "^0.0.1".into() }],
            exports: BTreeMap::new(),
            options: vec![],
        };
        let reg = InstalledPlugins {
            by_id: BTreeMap::from([("idola.minigame".to_string(), InstalledPlugin { manifest: m })]),
        };
        assert_eq!(reg.get("idola.minigame").unwrap().manifest.version, "0.1.0");
        assert!(reg.get("nope").is_none());
    }
```

- [ ] **Step 2: Run to verify failure** — `cargo test -p lute-manifest --lib installed_plugins_lookup` → FAIL to compile.

- [ ] **Step 3: Define the types** (top of `resolve.rs`, after imports):

```rust
/// One installed plugin's parsed manifest entry (plugin §5). The full loaded
/// package (directives/shapes/providers/…) is carried by `loader::LoadedPlugin`
/// (Phase 4); resolution needs only the manifest's id/version/depends here.
#[derive(Clone, Debug)]
pub struct InstalledPlugin {
    pub manifest: crate::schema::PluginManifest,
}

/// Every plugin discovered on disk, indexed by id (plugin §4). The resolver
/// walks this for the dependency closure (§11.1 step 6) and the inactive-plugin
/// fix-it (§11.2); the assembler merges the *active* subset into the snapshot.
#[derive(Clone, Debug, Default)]
pub struct InstalledPlugins {
    pub by_id: std::collections::BTreeMap<String, InstalledPlugin>,
}

impl InstalledPlugins {
    pub fn get(&self, id: &str) -> Option<&InstalledPlugin> {
        self.by_id.get(id)
    }
}
```

- [ ] **Step 4: Run + commit** — `cargo test -p lute-manifest` (PASS):

```bash
cargo fmt -p lute-manifest
git add crates/lute-manifest/src/resolve.rs
git commit -m "feat(manifest): InstalledPlugins registry (plugin §4, F2 scaffold)"
```

### Task 3.2: dependency closure + version-range check in `resolve_activation`

**Files:**
- Modify: `crates/lute-manifest/src/resolve.rs:24-28` (`ResolveError`), `:52-101` (`resolve_activation` signature + step 6)
- Modify: callers — `crates/lute-manifest/src/resolve.rs` tests (existing tests pass `&InstalledPlugins::default()`)
- Test: `crates/lute-manifest/src/resolve.rs` tests

**Interfaces:**
- Produces (signature change): `pub fn resolve_activation(graph: &ProfileGraph, selected: &str, scene_local: &ActivationMap, installed: &InstalledPlugins) -> Result<Vec<ActivePlugin>, ResolveError>`.
- New `ResolveError` variants: `UnresolvedDepends { plugin: String, dep: String }`, `DependsVersionMismatch { plugin: String, dep: String, need: String, found: String }`, `DependsCycle(String)`.
- Behavior: after step 5, iteratively add each active plugin's `depends[]` (transitively) that is not yet active; a `depends` id absent from `installed` → `UnresolvedDepends`; present but its `version` fails the `range` → `DependsVersionMismatch`; a cycle in the depends graph → `DependsCycle`. Dependency-added plugins get empty options (defaults). Deterministic: iterate `installed`/pending in sorted id order.

> **Version range:** implement a minimal semver-range satisfier supporting the caret form the spec uses (`^0.0.1`) and exact (`0.1.0`). Do NOT add a semver crate — the offline build forbids new deps. A small internal `fn range_satisfies(range: &str, version: &str) -> bool` covering `^X.Y.Z` (compatible-with, pre-1.0 caret pins to the minor: `^0.0.z` ⇒ same `0.0.z`; `^0.y.z` ⇒ `0.y.*` with patch ≥ z) and a bare exact match is sufficient for 0.0.1; document the supported grammar in a doc comment.

- [ ] **Step 1: Write the failing tests**

```rust
    fn manifest(id: &str, version: &str, deps: &[(&str, &str)]) -> crate::schema::PluginManifest {
        crate::schema::PluginManifest {
            id: id.into(),
            version: version.into(),
            kind: "capability".into(),
            depends: deps.iter().map(|(i, r)| crate::schema::Depends { id: i.to_string(), range: r.to_string() }).collect(),
            exports: std::collections::BTreeMap::new(),
            options: vec![],
        }
    }

    fn installed(ms: Vec<crate::schema::PluginManifest>) -> InstalledPlugins {
        InstalledPlugins {
            by_id: ms.into_iter().map(|m| (m.id.clone(), InstalledPlugin { manifest: m })).collect(),
        }
    }

    #[test]
    fn dependency_closure_pulls_transitive_deps() {
        use std::collections::BTreeMap;
        // story activates idola.vn; idola.vn depends idola.base; base depends lute.core.
        let graph = ProfileGraph {
            profiles: BTreeMap::from([(
                "story".to_string(),
                Profile { extends: None, plugins: BTreeMap::from([("idola.vn".to_string(), BTreeMap::new())]) },
            )]),
            default_profile: "story".to_string(),
        };
        let inst = installed(vec![
            manifest("lute.core", "0.0.1", &[]),
            manifest("idola.base", "0.1.0", &[("lute.core", "^0.0.1")]),
            manifest("idola.vn", "0.1.0", &[("idola.base", "^0.1.0")]),
        ]);
        let active = resolve_activation(&graph, "story", &BTreeMap::new(), &inst).unwrap();
        let ids: Vec<_> = active.iter().map(|a| a.id.as_str()).collect();
        assert!(ids.contains(&"idola.base"), "transitive dep must be activated: {ids:?}");
        assert!(ids.contains(&"idola.vn"));
        assert!(ids.contains(&"lute.core"));
    }

    #[test]
    fn unresolved_depends_is_error() {
        use std::collections::BTreeMap;
        let graph = ProfileGraph {
            profiles: BTreeMap::from([(
                "s".to_string(),
                Profile { extends: None, plugins: BTreeMap::from([("a.x".to_string(), BTreeMap::new())]) },
            )]),
            default_profile: "s".to_string(),
        };
        let inst = installed(vec![manifest("a.x", "0.1.0", &[("a.missing", "^0.1.0")])]);
        assert!(matches!(
            resolve_activation(&graph, "s", &BTreeMap::new(), &inst),
            Err(ResolveError::UnresolvedDepends { .. })
        ));
    }

    #[test]
    fn depends_version_mismatch_is_error() {
        use std::collections::BTreeMap;
        let graph = ProfileGraph {
            profiles: BTreeMap::from([(
                "s".to_string(),
                Profile { extends: None, plugins: BTreeMap::from([("a.x".to_string(), BTreeMap::new())]) },
            )]),
            default_profile: "s".to_string(),
        };
        let inst = installed(vec![
            manifest("a.x", "0.1.0", &[("a.dep", "^0.2.0")]),
            manifest("a.dep", "0.1.0", &[]),
        ]);
        assert!(matches!(
            resolve_activation(&graph, "s", &BTreeMap::new(), &inst),
            Err(ResolveError::DependsVersionMismatch { .. })
        ));
    }
```

- [ ] **Step 2: Run to verify failure** — `cargo test -p lute-manifest --lib depend` → FAIL to compile (arity + variants).

- [ ] **Step 3: Extend `ResolveError`**

```rust
#[derive(Clone, Debug, PartialEq)]
pub enum ResolveError {
    UnknownProfile(String),
    ExtendsCycle(String),
    /// A `depends` id (plugin §5) is not installed (plugin §11.1 step 6).
    UnresolvedDepends { plugin: String, dep: String },
    /// A `depends` is installed but its version fails the declared range.
    DependsVersionMismatch { plugin: String, dep: String, need: String, found: String },
    /// The `depends` graph has a cycle.
    DependsCycle(String),
}
```

- [ ] **Step 4: Add the closure to `resolve_activation`**

Change the signature to add `installed: &InstalledPlugins`, and after the step-5 `apply(scene_local, …)` line and before building the `Ok(...)` vec, insert step 6:

```rust
    // 6. Dependency closure (plugin §11.1 step 6): transitively activate every
    //    `depends` of an active plugin, in deterministic (sorted-id) order.
    //    depends-added plugins take default (empty) options.
    let mut queue: Vec<String> = order.clone();
    let mut in_progress: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    while let Some(id) = queue.pop() {
        let Some(inst) = installed.get(&id) else {
            // lute.core is always synthetic-present even if not installed on disk;
            // any other missing active id is the caller's concern (it was named by
            // a profile, not a depends) — skip closure for it.
            continue;
        };
        if !in_progress.insert(id.clone()) {
            return Err(ResolveError::DependsCycle(id));
        }
        let mut deps = inst.manifest.depends.clone();
        deps.sort_by(|a, b| a.id.cmp(&b.id));
        for dep in deps {
            match installed.get(&dep.id) {
                None if dep.id == "lute.core" => { /* synthetic core, always ok */ }
                None => {
                    return Err(ResolveError::UnresolvedDepends {
                        plugin: id.clone(),
                        dep: dep.id.clone(),
                    })
                }
                Some(dep_inst) => {
                    if !range_satisfies(&dep.range, &dep_inst.manifest.version) {
                        return Err(ResolveError::DependsVersionMismatch {
                            plugin: id.clone(),
                            dep: dep.id.clone(),
                            need: dep.range.clone(),
                            found: dep_inst.manifest.version.clone(),
                        });
                    }
                }
            }
            if !merged.contains_key(&dep.id) {
                order.push(dep.id.clone());
                merged.insert(dep.id.clone(), BTreeMap::new());
                queue.push(dep.id.clone());
            }
        }
    }
```

Add the range satisfier as a free fn:

```rust
/// Minimal semver-range check for plugin `depends` (plugin §5). Supports the
/// caret form used in 0.0.1 (`^MAJOR.MINOR.PATCH`) and a bare exact version.
/// Caret semantics: pre-1.0 the caret pins to the leftmost non-zero component —
/// `^0.0.z` requires exactly `0.0.z`; `^0.y.z` requires `0.y.*` with patch ≥ z;
/// `^x.y.z` (x≥1) requires `x.*` with (minor,patch) ≥ (y,z). An unparseable
/// range or version is treated as NOT satisfied (conservative).
fn range_satisfies(range: &str, version: &str) -> bool {
    fn parse(v: &str) -> Option<(u64, u64, u64)> {
        let mut it = v.trim().split('.');
        let a = it.next()?.parse().ok()?;
        let b = it.next()?.parse().ok()?;
        let c = it.next().unwrap_or("0").parse().ok()?;
        Some((a, b, c))
    }
    let Some((vmaj, vmin, vpat)) = parse(version) else { return false };
    if let Some(caret) = range.strip_prefix('^') {
        let Some((rmaj, rmin, rpat)) = parse(caret) else { return false };
        if rmaj == 0 && rmin == 0 {
            return (vmaj, vmin, vpat) == (rmaj, rmin, rpat);
        }
        if rmaj == 0 {
            return vmaj == 0 && vmin == rmin && vpat >= rpat;
        }
        return vmaj == rmaj && (vmin, vpat) >= (rmin, rpat);
    }
    parse(range) == Some((vmaj, vmin, vpat))
}
```

- [ ] **Step 5: Fix existing callers**

All existing `resolve_activation(...)` call sites (tests in `resolve.rs`) now need a 4th arg. Update each to pass `&InstalledPlugins::default()` (they exercise profile-layer behavior, no depends). Grep to be sure: `grep -rn resolve_activation crates/`.

- [ ] **Step 6: Run + commit**

Run: `cargo test -p lute-manifest` (all PASS), then:

```bash
cargo fmt -p lute-manifest
git add crates/lute-manifest/src/resolve.rs
git commit -m "feat(manifest): dependency closure + version-range check in resolve_activation (plugin §11.1, F2)"
```

---

## Phase 4 — Disk loader (plugin §4)

> Read a `plugins/<id>/` package into memory, honoring `exports`, sorting files byte-wise, rejecting per-package duplicate ids. Never panics.

### Task 4.1: `LoadedPlugin` + `load_plugin_dir` (single package)

**Files:**
- Create: `crates/lute-manifest/src/loader.rs`
- Modify: `crates/lute-manifest/src/lib.rs` (add `pub mod loader;`)
- Modify: `crates/lute-manifest/src/schema.rs` — ensure `ShapesFile`, `TemplatesFile`, `ProvidersFile`, `BridgeFile`, `DirectivesFile` are `pub` and add `FrontmatterFile` + `DefsFile` deserializers.
- Test: `crates/lute-manifest/tests/loader.rs` (Create) with a temp-dir fixture.

**Interfaces:**
- Produces:
  ```rust
  pub struct LoadedPlugin {
      pub manifest: PluginManifest,
      pub directives: Vec<DirectiveDecl>,
      pub enums: BTreeMap<String, Vec<String>>,
      pub state_shapes: Vec<StateShape>,
      pub state_templates: Vec<StateTemplate>,
      pub providers: Vec<ProviderDecl>,
      pub bridge: Vec<BridgeCapability>,
      pub defs: Vec<DefDecl>,
      pub frontmatter: BTreeMap<String, Type>,
  }
  pub enum LoadError {
      Manifest { dir: String, msg: String },
      Parse { file: String, msg: String },
      DuplicateId { kind: String, id: String },
      MissingExportDir { export: String, path: String },
  }
  pub fn load_plugin_dir(dir: &Path) -> Result<LoadedPlugin, Vec<LoadError>>;
  ```
- Consumes: all `schema.rs` file types + `types::Type`.

**Loader rules (plugin §4):** read only the dirs named in `manifest.exports`; a listed dir/file that is absent → `MissingExportDir`; within each export, sort entries byte-wise before merging; reject two declarations sharing an id within a kind (directive name, shape name, provider name, def name, bridge `service+operation`, enum name, frontmatter key) → `DuplicateId`. `enums` export may be a single file (`enums.yaml`, as in `lute.core`) or a dir of `*.yaml` — support both by checking whether the export path is a file or dir.

- [ ] **Step 1: Write the failing test**

Create `crates/lute-manifest/tests/loader.rs`:

```rust
use lute_manifest::loader::{load_plugin_dir, LoadError};
use std::fs;

/// Build a minimal on-disk plugin package under a temp dir; return its path.
fn write_pkg(root: &std::path::Path, dup: bool) {
    fs::create_dir_all(root.join("directives")).unwrap();
    fs::write(
        root.join("plugin.yaml"),
        "id: t.plug\nversion: 0.1.0\nkind: capability\nexports:\n  directives: directives/\n",
    )
    .unwrap();
    let d = if dup {
        "directives:\n  - { name: foo, attrs: [], lower: { kind: builtin, name: n } }\n  - { name: foo, attrs: [], lower: { kind: builtin, name: n } }\n"
    } else {
        "directives:\n  - { name: foo, attrs: [ { name: x, type: bool } ], lower: { kind: builtin, name: n } }\n"
    };
    fs::write(root.join("directives/a.yaml"), d).unwrap();
}

#[test]
fn loads_a_valid_package() {
    let tmp = std::env::temp_dir().join(format!("lute_pkg_ok_{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    write_pkg(&tmp, false);
    let p = load_plugin_dir(&tmp).expect("valid package loads");
    assert_eq!(p.manifest.id, "t.plug");
    assert_eq!(p.directives.len(), 1);
    assert_eq!(p.directives[0].name, "foo");
    fs::remove_dir_all(&tmp).ok();
}

#[test]
fn rejects_duplicate_directive_id() {
    let tmp = std::env::temp_dir().join(format!("lute_pkg_dup_{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    write_pkg(&tmp, true);
    let errs = load_plugin_dir(&tmp).unwrap_err();
    assert!(errs.iter().any(|e| matches!(e, LoadError::DuplicateId { kind, id } if kind == "directive" && id == "foo")));
    fs::remove_dir_all(&tmp).ok();
}

#[test]
fn rejects_missing_export_dir() {
    let tmp = std::env::temp_dir().join(format!("lute_pkg_miss_{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).unwrap();
    fs::write(
        tmp.join("plugin.yaml"),
        "id: t.plug\nversion: 0.1.0\nkind: capability\nexports:\n  directives: directives/\n",
    )
    .unwrap();
    let errs = load_plugin_dir(&tmp).unwrap_err();
    assert!(errs.iter().any(|e| matches!(e, LoadError::MissingExportDir { .. })));
    fs::remove_dir_all(&tmp).ok();
}
```

- [ ] **Step 2: Run to verify failure** — `cargo test -p lute-manifest --test loader` → FAIL to compile (`loader` missing).

- [ ] **Step 3: Add schema deserializers**

In `crates/lute-manifest/src/schema.rs`, confirm `pub struct DirectivesFile`, `ShapesFile`, `TemplatesFile`, `ProvidersFile`, `BridgeFile` exist and are `pub`; add:

```rust
#[derive(Debug, Deserialize)]
pub struct DefsFile {
    pub defs: Vec<DefDecl>,
}

#[derive(Debug, Deserialize)]
pub struct FrontmatterFile {
    pub frontmatter: Vec<FrontmatterDecl>,
}

#[derive(Debug, Deserialize)]
pub struct FrontmatterDecl {
    pub key: String,
    pub schema: Type,
}

#[derive(Debug, Deserialize)]
pub struct EnumsFile {
    pub enums: std::collections::BTreeMap<String, Vec<String>>,
}
```

(If `core.rs` already defines a private `EnumsFile`, make it use `schema::EnumsFile` to avoid duplication — update `core.rs`'s `use` and delete its local copy.)

- [ ] **Step 4: Implement `loader.rs`**

```rust
//! Plugin package loader (plugin §4). Reads a `plugins/<id>/` directory into a
//! `LoadedPlugin`, honoring `exports`, sorting files byte-wise, and rejecting
//! per-package duplicate ids within a kind. Never panics: every failure is a
//! `LoadError` in the returned vec.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::schema::*;
use crate::types::Type;

#[derive(Clone, Debug)]
pub struct LoadedPlugin {
    pub manifest: PluginManifest,
    pub directives: Vec<DirectiveDecl>,
    pub enums: BTreeMap<String, Vec<String>>,
    pub state_shapes: Vec<StateShape>,
    pub state_templates: Vec<StateTemplate>,
    pub providers: Vec<ProviderDecl>,
    pub bridge: Vec<BridgeCapability>,
    pub defs: Vec<DefDecl>,
    pub frontmatter: BTreeMap<String, Type>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum LoadError {
    Manifest { dir: String, msg: String },
    Parse { file: String, msg: String },
    DuplicateId { kind: String, id: String },
    MissingExportDir { export: String, path: String },
}

/// Read one plugin package. `dir` MUST contain `plugin.yaml`.
pub fn load_plugin_dir(dir: &Path) -> Result<LoadedPlugin, Vec<LoadError>> {
    let mut errs = Vec::new();

    let manifest_path = dir.join("plugin.yaml");
    let manifest: PluginManifest = match std::fs::read_to_string(&manifest_path) {
        Ok(s) => match serde_yaml::from_str(&s) {
            Ok(m) => m,
            Err(e) => return Err(vec![LoadError::Manifest { dir: dir.display().to_string(), msg: e.to_string() }]),
        },
        Err(e) => return Err(vec![LoadError::Manifest { dir: dir.display().to_string(), msg: e.to_string() }]),
    };

    let mut out = LoadedPlugin {
        manifest: manifest.clone(),
        directives: Vec::new(),
        enums: BTreeMap::new(),
        state_shapes: Vec::new(),
        state_templates: Vec::new(),
        providers: Vec::new(),
        bridge: Vec::new(),
        defs: Vec::new(),
        frontmatter: BTreeMap::new(),
    };

    // Read each declared export. A relative export path resolves under `dir`.
    for (export, rel) in &manifest.exports {
        let path = dir.join(rel);
        if !path.exists() {
            errs.push(LoadError::MissingExportDir { export: export.clone(), path: path.display().to_string() });
            continue;
        }
        match export.as_str() {
            "directives" => read_kind::<DirectivesFile, _>(&path, &mut errs, |f, e| merge_directives(&mut out.directives, f.directives, e)),
            "state" => read_state(&path, &mut out, &mut errs),
            "providers" => read_kind::<ProvidersFile, _>(&path, &mut errs, |f, e| merge_named(&mut out.providers, f.providers, "provider", |p| p.name.clone(), e)),
            "bridge" => read_kind::<BridgeFile, _>(&path, &mut errs, |f, e| merge_bridge(&mut out.bridge, f.bridge_capabilities, e)),
            "defs" => read_kind::<DefsFile, _>(&path, &mut errs, |f, e| merge_named(&mut out.defs, f.defs, "def", |d| d.name.clone(), e)),
            "enums" => read_enums(&path, &mut out.enums, &mut errs),
            "frontmatter" => read_kind::<FrontmatterFile, _>(&path, &mut errs, |f, e| merge_frontmatter(&mut out.frontmatter, f.frontmatter, e)),
            "docs" => { /* non-normative (plugin §6.7); skip */ }
            "assetkinds" => { /* plugin §6.9 deferred to a later plan; ignore for now */ }
            _ => { /* unknown export key: ignore (closed set enforced by validate) */ }
        }
    }

    if errs.is_empty() { Ok(out) } else { Err(errs) }
}

/// Read a single YAML file OR every `*.yaml`/`*.yml` in a dir (sorted byte-wise),
/// deserialize each to `F`, and hand it to `merge`.
fn read_kind<F, M>(path: &Path, errs: &mut Vec<LoadError>, mut merge: M)
where
    F: serde::de::DeserializeOwned,
    M: FnMut(F, &mut Vec<LoadError>),
{
    for file in yaml_files(path) {
        match std::fs::read_to_string(&file).ok().and_then(|s| serde_yaml::from_str::<F>(&s).map_err(|e| errs.push(LoadError::Parse { file: file.display().to_string(), msg: e.to_string() })).ok()) {
            Some(f) => merge(f, errs),
            None => {}
        }
    }
}

/// `state/` holds `shapes.yaml` (stateShapes) and/or `templates.yaml` (stateTemplates).
fn read_state(path: &Path, out: &mut LoadedPlugin, errs: &mut Vec<LoadError>) {
    for file in yaml_files(path) {
        let Ok(s) = std::fs::read_to_string(&file) else { continue };
        if let Ok(f) = serde_yaml::from_str::<ShapesFile>(&s) {
            merge_named(&mut out.state_shapes, f.state_shapes, "shape", |s| s.name.clone(), errs);
        } else if let Ok(f) = serde_yaml::from_str::<TemplatesFile>(&s) {
            merge_named(&mut out.state_templates, f.state_templates, "template", |t| t.name.clone(), errs);
        } else {
            errs.push(LoadError::Parse { file: file.display().to_string(), msg: "not a state shapes/templates file".into() });
        }
    }
}

fn read_enums(path: &Path, dst: &mut BTreeMap<String, Vec<String>>, errs: &mut Vec<LoadError>) {
    for file in yaml_files(path) {
        let Ok(s) = std::fs::read_to_string(&file) else { continue };
        match serde_yaml::from_str::<EnumsFile>(&s) {
            Ok(f) => {
                for (k, v) in f.enums {
                    if dst.insert(k.clone(), v).is_some() {
                        errs.push(LoadError::DuplicateId { kind: "enum".into(), id: k });
                    }
                }
            }
            Err(e) => errs.push(LoadError::Parse { file: file.display().to_string(), msg: e.to_string() }),
        }
    }
}

/// Every `*.yaml`/`*.yml` under `path` (a dir), sorted byte-wise; or `[path]`
/// itself if `path` is a file (plugin §4 sort determinism).
fn yaml_files(path: &Path) -> Vec<std::path::PathBuf> {
    if path.is_file() {
        return vec![path.to_path_buf()];
    }
    let mut v: Vec<_> = std::fs::read_dir(path)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_file() && matches!(p.extension().and_then(|x| x.to_str()), Some("yaml") | Some("yml")))
        .collect();
    v.sort();
    v
}

fn merge_named<T, K: Fn(&T) -> String>(dst: &mut Vec<T>, items: Vec<T>, kind: &str, key: K, errs: &mut Vec<LoadError>) {
    let mut seen: BTreeSet<String> = dst.iter().map(&key).collect();
    for it in items {
        let id = key(&it);
        if !seen.insert(id.clone()) {
            errs.push(LoadError::DuplicateId { kind: kind.into(), id });
        } else {
            dst.push(it);
        }
    }
}

fn merge_directives(dst: &mut Vec<DirectiveDecl>, items: Vec<DirectiveDecl>, errs: &mut Vec<LoadError>) {
    merge_named(dst, items, "directive", |d| d.name.clone(), errs);
}

fn merge_bridge(dst: &mut Vec<BridgeCapability>, items: Vec<BridgeCapability>, errs: &mut Vec<LoadError>) {
    let mut seen: BTreeSet<(String, String)> = dst.iter().map(|b| (b.service.clone(), b.operation.clone())).collect();
    for b in items {
        let k = (b.service.clone(), b.operation.clone());
        if !seen.insert(k) {
            errs.push(LoadError::DuplicateId { kind: "bridge".into(), id: format!("{}.{}", b.service, b.operation) });
        } else {
            dst.push(b);
        }
    }
}

fn merge_frontmatter(dst: &mut BTreeMap<String, Type>, items: Vec<FrontmatterDecl>, errs: &mut Vec<LoadError>) {
    for f in items {
        if dst.insert(f.key.clone(), f.schema).is_some() {
            errs.push(LoadError::DuplicateId { kind: "frontmatter".into(), id: f.key });
        }
    }
}
```

Add `pub mod loader;` to `crates/lute-manifest/src/lib.rs`.

- [ ] **Step 5: Run + commit**

Run: `cargo test -p lute-manifest --test loader` (PASS), then `cargo test -p lute-manifest` (all PASS):

```bash
cargo fmt -p lute-manifest
git add crates/lute-manifest/src/loader.rs crates/lute-manifest/src/lib.rs crates/lute-manifest/src/schema.rs crates/lute-manifest/src/core.rs
git commit -m "feat(manifest): plugin package loader with dup-id reject (plugin §4)"
```

### Task 4.2: `load_plugins_dir` → `InstalledPlugins` (scan + cross-package dup reject)

**Files:**
- Modify: `crates/lute-manifest/src/loader.rs`
- Modify: `crates/lute-manifest/src/resolve.rs` — `InstalledPlugin` gains a `pub loaded: LoadedPlugin` field (so the assembler in Phase 5 reads the package). Update Task 3.1's struct + its test to construct via a `LoadedPlugin`.
- Test: `crates/lute-manifest/tests/loader.rs` (append)

**Interfaces:**
- Produces: `pub fn load_plugins_dir(dir: &Path) -> (InstalledPlugins, Vec<LoadError>)` — scan each immediate subdirectory (sorted), `load_plugin_dir` each, index by `manifest.id`; a duplicate `manifest.id` across packages → `LoadError::DuplicateId { kind: "plugin", id }` (the second is dropped). Missing dir → empty registry.
- Change `InstalledPlugin` to `{ pub manifest: PluginManifest, pub loaded: crate::loader::LoadedPlugin }` (or fold `manifest` access through `loaded.manifest` — pick one; the assembler needs the full `LoadedPlugin`). Recommended: `pub struct InstalledPlugin { pub loaded: LoadedPlugin }` with `impl InstalledPlugin { pub fn manifest(&self) -> &PluginManifest { &self.loaded.manifest } }`, and update `resolve.rs`'s closure to call `.manifest()`.

- [ ] **Step 1: Write the failing test (append to loader.rs test file)**

```rust
use lute_manifest::loader::load_plugins_dir;

#[test]
fn scans_a_plugins_directory() {
    let root = std::env::temp_dir().join(format!("lute_plugins_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    write_pkg(&root.join("t.plug"), false); // reuse the helper; nested dir = plugin id
    let (reg, errs) = load_plugins_dir(&root);
    assert!(errs.is_empty(), "{errs:?}");
    assert!(reg.get("t.plug").is_some());
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn missing_plugins_dir_is_empty() {
    let (reg, errs) = load_plugins_dir(std::path::Path::new("/no/such/dir/xyz"));
    assert!(reg.by_id.is_empty());
    assert!(errs.is_empty());
}
```

(The `write_pkg` helper writes `plugin.yaml` with `id: t.plug`; the containing dir name need not equal the id — indexing is by manifest id.)

- [ ] **Step 2: Run to verify failure** — `cargo test -p lute-manifest --test loader scans_a_plugins` → FAIL to compile.

- [ ] **Step 3: Refactor `InstalledPlugin` to carry the package**

In `crates/lute-manifest/src/resolve.rs`:

```rust
#[derive(Clone, Debug)]
pub struct InstalledPlugin {
    pub loaded: crate::loader::LoadedPlugin,
}

impl InstalledPlugin {
    pub fn manifest(&self) -> &crate::schema::PluginManifest {
        &self.loaded.manifest
    }
}
```

Update the closure code in `resolve_activation` to use `inst.manifest()` instead of `inst.manifest`. Update Task 3.1/3.2 tests' `manifest(...)`/`installed(...)` helpers to wrap each manifest in a minimal `LoadedPlugin` (all export vecs empty). Add a test helper in `resolve.rs` tests:

```rust
    fn loaded(m: crate::schema::PluginManifest) -> crate::loader::LoadedPlugin {
        crate::loader::LoadedPlugin {
            manifest: m,
            directives: vec![], enums: Default::default(), state_shapes: vec![],
            state_templates: vec![], providers: vec![], bridge: vec![], defs: vec![],
            frontmatter: Default::default(),
        }
    }
    // installed(...) now: InstalledPlugin { loaded: loaded(m) }
```

- [ ] **Step 4: Implement `load_plugins_dir`**

```rust
use crate::resolve::{InstalledPlugin, InstalledPlugins};

/// Scan `dir` for plugin packages (each immediate subdirectory containing a
/// `plugin.yaml`), in sorted order, and index by manifest id. A duplicate id
/// across packages is a `LoadError::DuplicateId { kind: "plugin", .. }` (the
/// later package is dropped). A missing `dir` yields an empty registry.
pub fn load_plugins_dir(dir: &Path) -> (InstalledPlugins, Vec<LoadError>) {
    let mut reg = InstalledPlugins::default();
    let mut errs = Vec::new();
    let mut subs: Vec<_> = match std::fs::read_dir(dir) {
        Ok(rd) => rd.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.is_dir()).collect(),
        Err(_) => return (reg, errs),
    };
    subs.sort();
    for sub in subs {
        if !sub.join("plugin.yaml").is_file() {
            continue;
        }
        match load_plugin_dir(&sub) {
            Ok(loaded) => {
                let id = loaded.manifest.id.clone();
                if reg.by_id.contains_key(&id) {
                    errs.push(LoadError::DuplicateId { kind: "plugin".into(), id });
                } else {
                    reg.by_id.insert(id, InstalledPlugin { loaded });
                }
            }
            Err(mut e) => errs.append(&mut e),
        }
    }
    (reg, errs)
}
```

- [ ] **Step 5: Run + commit**

Run: `cargo test -p lute-manifest` (all PASS — loader + resolve), then:

```bash
cargo fmt -p lute-manifest
git add crates/lute-manifest/src/loader.rs crates/lute-manifest/src/resolve.rs
git commit -m "feat(manifest): load_plugins_dir scans + indexes installed plugins (plugin §4)"
```

---

## Phase 5 — Snapshot assembly (multi-plugin merge, plugin §13)

> Merge every *active* plugin's package into one `CapabilitySnapshot`, namespaced by plugin, with cross-plugin dup-id rejection, and stamp `capabilityVersion`. Also add the `state_templates` + `inactive` snapshot fields.

### Task 5.1: `CapabilitySnapshot` gains `state_templates` + `inactive`; fold into hash

**Files:**
- Modify: `crates/lute-manifest/src/snapshot.rs:8-19` (struct), `:78-145` (`capability_version`)
- Test: `crates/lute-manifest/src/snapshot.rs` tests (determinism/drift)

**Interfaces:**
- Produces: `CapabilitySnapshot.state_templates: BTreeMap<String, StateTemplate>` and `.inactive: BTreeMap<String, String>` (directive/tag name → owning inactive plugin id, for the §11.2 fix-it). `capability_version` folds `state_templates` under a new section marker. `inactive` is NOT hashed (it is diagnostic metadata, not resolved capability surface — document this).

- [ ] **Step 1: Write the failing test**

```rust
    #[test]
    fn state_templates_change_the_version() {
        let mut a = CapabilitySnapshot::default();
        a.version = capability_version(&a);
        let mut b = CapabilitySnapshot::default();
        b.state_templates.insert(
            "slot".into(),
            crate::schema::StateTemplate {
                name: "slot".into(),
                scope: "scene".into(),
                path: vec![],
                shape: "s".into(),
            },
        );
        b.version = capability_version(&b);
        assert_ne!(a.version, b.version, "state_templates must affect capabilityVersion");
    }

    #[test]
    fn inactive_does_not_change_the_version() {
        let mut a = CapabilitySnapshot::default();
        let va = capability_version(&a);
        a.inactive.insert("minigame".into(), "idola.minigame".into());
        assert_eq!(va, capability_version(&a), "inactive index is metadata, not hashed");
    }
```

- [ ] **Step 2: Run to verify failure** — `cargo test -p lute-manifest --lib state_templates_change` → FAIL to compile.

- [ ] **Step 3: Add the fields**

```rust
#[derive(Clone, Debug, Default)]
pub struct CapabilitySnapshot {
    pub version: String,
    pub plugins: BTreeMap<String, ResolvedPlugin>,
    pub enums: BTreeMap<String, Vec<String>>,
    pub directives: BTreeMap<String, DirectiveDecl>,
    pub providers: BTreeMap<String, ProviderDecl>,
    pub state_shapes: BTreeMap<String, StateShape>,
    pub state_templates: BTreeMap<String, StateTemplate>,
    pub bridge_capabilities: BTreeMap<(String, String), BridgeCapability>,
    pub defs: BTreeMap<String, DefDecl>,
    pub frontmatter: BTreeMap<String, crate::types::Type>,
    /// Installed-but-inactive tag → owning plugin id (plugin §11.2 fix-it). Not
    /// part of the resolved capability surface, so NOT folded into the version.
    pub inactive: BTreeMap<String, String>,
}
```

- [ ] **Step 4: Fold `state_templates` into `capability_version`**

In `capability_version`, after the `state_shapes` section, add a `state_templates` section marker + fold each `(name, template)` via `Debug` (matching the existing pattern). Do NOT add `inactive`.

- [ ] **Step 5: Run + commit**

Run: `cargo test -p lute-manifest` (PASS; existing drift/determinism tests still pass), then:

```bash
cargo fmt -p lute-manifest
git add crates/lute-manifest/src/snapshot.rs
git commit -m "feat(manifest): snapshot state_templates (hashed) + inactive index (plugin §13/§11.2)"
```

### Task 5.2: `assemble_snapshot` — merge active plugins into one snapshot

**Files:**
- Create: `crates/lute-manifest/src/assemble.rs`
- Modify: `crates/lute-manifest/src/lib.rs` (`pub mod assemble;`)
- Test: `crates/lute-manifest/tests/assemble.rs` (Create)

**Interfaces:**
- Produces:
  ```rust
  pub enum AssembleError {
      DuplicateAcrossPlugins { kind: String, id: String, first: String, second: String },
      ReservedName { id: String, plugin: String },
      MissingActivePlugin { id: String },
  }
  pub fn assemble_snapshot(
      active: &[crate::resolve::ActivePlugin],
      installed: &crate::resolve::InstalledPlugins,
  ) -> (CapabilitySnapshot, Vec<AssembleError>);
  ```
- Behavior: start from `load_core_snapshot()` (the embedded `lute.core` is always the base — it may or may not also appear in `active`; dedupe by id). For each `ActivePlugin` (skipping `lute.core`, already embedded): look it up in `installed`; merge its `LoadedPlugin` exports into the snapshot maps, keying directives by `name`, providers by `name`, shapes by `name`, templates by `name`, defs by `name`, bridge by `(service,operation)`, enums by name, frontmatter by key. A key already present from a DIFFERENT plugin → `DuplicateAcrossPlugins` (drop the second). A directive/tag colliding with a language-reserved term (dsl §10: `cut`, `scene`, timing attrs) → `ReservedName`. Record each active plugin in `snapshot.plugins` as a `ResolvedPlugin { version, options }` (options from `ActivePlugin.options`). Populate `snapshot.inactive` from `installed` minus active. Finally `snapshot.version = capability_version(&snapshot)`.
- Reserved-name set: reuse a small const `RESERVED_DIRECTIVE_NAMES: &[&str] = &["cut", "scene"]` (dsl §10 — note `cut` IS a core directive, so only reject reserved names from NON-core plugins; core owns `cut`). Timing keys `duration/delay/wait/at` are reserved as *attribute* names (dsl §7.5) — enforce at attr level only if trivially checkable; otherwise document as deferred.

- [ ] **Step 1: Write the failing test**

Create `crates/lute-manifest/tests/assemble.rs`:

```rust
use lute_manifest::assemble::assemble_snapshot;
use lute_manifest::loader::LoadedPlugin;
use lute_manifest::resolve::{ActivePlugin, InstalledPlugin, InstalledPlugins};
use lute_manifest::schema::{AttrDecl, DirectiveDecl, Lowering, PluginManifest};
use lute_manifest::types::Type;
use std::collections::BTreeMap;

fn plugin_with_directive(id: &str, dname: &str) -> LoadedPlugin {
    LoadedPlugin {
        manifest: PluginManifest {
            id: id.into(), version: "0.1.0".into(), kind: "capability".into(),
            depends: vec![], exports: BTreeMap::new(), options: vec![],
        },
        directives: vec![DirectiveDecl {
            name: dname.into(), layer: Some("bridge".into()),
            attrs: vec![AttrDecl { name: "x".into(), required: false, ty: Type::Bool, default: None }],
            semantics: vec![], state: None, effects: None, bridge: None,
            lower: Lowering::Builtin { kind: "builtin".into(), name: "n".into() },
        }],
        enums: BTreeMap::new(), state_shapes: vec![], state_templates: vec![],
        providers: vec![], bridge: vec![], defs: vec![], frontmatter: BTreeMap::new(),
    }
}

#[test]
fn active_plugin_directive_lands_in_snapshot() {
    let reg = InstalledPlugins {
        by_id: BTreeMap::from([(
            "idola.minigame".to_string(),
            InstalledPlugin { loaded: plugin_with_directive("idola.minigame", "minigame") },
        )]),
    };
    let active = vec![
        ActivePlugin { id: "lute.core".into(), options: BTreeMap::new() },
        ActivePlugin { id: "idola.minigame".into(), options: BTreeMap::new() },
    ];
    let (snap, errs) = assemble_snapshot(&active, &reg);
    assert!(errs.is_empty(), "{errs:?}");
    assert!(snap.directive("minigame").is_some(), "plugin directive merged");
    assert!(snap.directive("bg").is_some(), "core directive retained");
    assert!(!snap.version.is_empty());
}

#[test]
fn inactive_plugin_is_indexed_not_merged() {
    let reg = InstalledPlugins {
        by_id: BTreeMap::from([(
            "idola.minigame".to_string(),
            InstalledPlugin { loaded: plugin_with_directive("idola.minigame", "minigame") },
        )]),
    };
    // only core active
    let active = vec![ActivePlugin { id: "lute.core".into(), options: BTreeMap::new() }];
    let (snap, errs) = assemble_snapshot(&active, &reg);
    assert!(errs.is_empty(), "{errs:?}");
    assert!(snap.directive("minigame").is_none(), "inactive directive must NOT merge");
    assert_eq!(snap.inactive.get("minigame"), Some(&"idola.minigame".to_string()));
}
```

- [ ] **Step 2: Run to verify failure** — `cargo test -p lute-manifest --test assemble` → FAIL to compile.

- [ ] **Step 3: Implement `assemble.rs`**

```rust
//! Multi-plugin capability-snapshot assembly (plugin §13). Merges every active
//! plugin's loaded package onto the embedded `lute.core` base into one
//! deterministic snapshot, rejecting cross-plugin duplicate ids and reserved
//! names, and stamping `capabilityVersion`.

use crate::core::load_core_snapshot;
use crate::resolve::{ActivePlugin, InstalledPlugins};
use crate::snapshot::{capability_version, CapabilitySnapshot, ResolvedPlugin};

#[derive(Clone, Debug, PartialEq)]
pub enum AssembleError {
    DuplicateAcrossPlugins { kind: String, id: String, first: String, second: String },
    ReservedName { id: String, plugin: String },
    MissingActivePlugin { id: String },
}

/// dsl §10 reserved terms a non-core plugin MUST NOT (re)define as a directive.
/// `cut` is core-owned, so it is only reserved against NON-core plugins.
const RESERVED_DIRECTIVE_NAMES: &[&str] = &["scene", "cut"];

pub fn assemble_snapshot(
    active: &[ActivePlugin],
    installed: &InstalledPlugins,
) -> (CapabilitySnapshot, Vec<AssembleError>) {
    let mut snap = load_core_snapshot();
    let mut errs = Vec::new();
    // Track which plugin owns each merged key for precise dup errors.
    let mut dir_owner: std::collections::BTreeMap<String, String> = snap.directives.keys().map(|k| (k.clone(), "lute.core".to_string())).collect();

    for ap in active {
        if ap.id == "lute.core" {
            // Already embedded; just record resolved options.
            snap.plugins.insert(ap.id.clone(), ResolvedPlugin { version: snap.plugins.get("lute.core").map(|p| p.version.clone()).unwrap_or_default(), options: ap.options.clone() });
            continue;
        }
        let Some(inst) = installed.get(&ap.id) else {
            errs.push(AssembleError::MissingActivePlugin { id: ap.id.clone() });
            continue;
        };
        let pkg = &inst.loaded;

        for d in &pkg.directives {
            if RESERVED_DIRECTIVE_NAMES.contains(&d.name.as_str()) {
                errs.push(AssembleError::ReservedName { id: d.name.clone(), plugin: ap.id.clone() });
                continue;
            }
            if let Some(first) = dir_owner.get(&d.name) {
                errs.push(AssembleError::DuplicateAcrossPlugins { kind: "directive".into(), id: d.name.clone(), first: first.clone(), second: ap.id.clone() });
                continue;
            }
            dir_owner.insert(d.name.clone(), ap.id.clone());
            snap.directives.insert(d.name.clone(), d.clone());
        }
        merge_map(&mut snap.state_shapes, pkg.state_shapes.iter().map(|s| (s.name.clone(), s.clone())), "shape", &ap.id, &mut errs);
        merge_map(&mut snap.state_templates, pkg.state_templates.iter().map(|t| (t.name.clone(), t.clone())), "template", &ap.id, &mut errs);
        merge_map(&mut snap.providers, pkg.providers.iter().map(|p| (p.name.clone(), p.clone())), "provider", &ap.id, &mut errs);
        merge_map(&mut snap.defs, pkg.defs.iter().map(|d| (d.name.clone(), d.clone())), "def", &ap.id, &mut errs);
        merge_map(&mut snap.frontmatter, pkg.frontmatter.iter().map(|(k, v)| (k.clone(), v.clone())), "frontmatter", &ap.id, &mut errs);
        merge_map(&mut snap.enums, pkg.enums.iter().map(|(k, v)| (k.clone(), v.clone())), "enum", &ap.id, &mut errs);
        for b in &pkg.bridge {
            let k = (b.service.clone(), b.operation.clone());
            if snap.bridge_capabilities.contains_key(&k) {
                errs.push(AssembleError::DuplicateAcrossPlugins { kind: "bridge".into(), id: format!("{}.{}", b.service, b.operation), first: "?".into(), second: ap.id.clone() });
            } else {
                snap.bridge_capabilities.insert(k, b.clone());
            }
        }
        snap.plugins.insert(ap.id.clone(), ResolvedPlugin { version: pkg.manifest.version.clone(), options: ap.options.clone() });
    }

    // Inactive index (plugin §11.2 fix-it): every installed directive whose plugin
    // is not active, tag -> owning plugin id.
    let active_ids: std::collections::BTreeSet<&str> = active.iter().map(|a| a.id.as_str()).collect();
    for (id, inst) in &installed.by_id {
        if active_ids.contains(id.as_str()) {
            continue;
        }
        for d in &inst.loaded.directives {
            snap.inactive.entry(d.name.clone()).or_insert_with(|| id.clone());
        }
    }

    snap.version = capability_version(&snap);
    (snap, errs)
}

fn merge_map<V: Clone>(
    dst: &mut std::collections::BTreeMap<String, V>,
    items: impl Iterator<Item = (String, V)>,
    kind: &str,
    plugin: &str,
    errs: &mut Vec<AssembleError>,
) {
    for (k, v) in items {
        if dst.contains_key(&k) {
            errs.push(AssembleError::DuplicateAcrossPlugins { kind: kind.into(), id: k, first: "?".into(), second: plugin.into() });
        } else {
            dst.insert(k, v);
        }
    }
}
```

Add `pub mod assemble;` to `lib.rs`. Ensure `resolve::ActivePlugin`, `resolve::InstalledPlugin`, `resolve::InstalledPlugins`, `loader::LoadedPlugin` are `pub`.

- [ ] **Step 4: Run + commit**

Run: `cargo test -p lute-manifest` (all PASS), then:

```bash
cargo fmt -p lute-manifest
git add crates/lute-manifest/src/assemble.rs crates/lute-manifest/src/lib.rs
git commit -m "feat(manifest): assemble_snapshot merges active plugins (plugin §13)"
```

### Task 5.3: wire `validate_directive` into the loader/assembler

**Files:**
- Modify: `crates/lute-manifest/src/assemble.rs` (call `validate::validate_directive` per merged directive)
- Modify: `crates/lute-manifest/src/validate.rs` (extend `ManifestError` surfacing if needed)
- Test: `crates/lute-manifest/tests/assemble.rs` (append)

**Interfaces:**
- Consumes: `crate::validate::validate_directive(&DirectiveDecl) -> Vec<ManifestError>` (existing; currently orphaned — this discharges the T1.5 documented gap).
- Produces: `AssembleError::InvalidDirective { plugin: String, directive: String, msg: String }` for each `ManifestError` from a merged plugin directive (unknown semantics flag, duplicate attr).

- [ ] **Step 1: Write the failing test (append)**

```rust
#[test]
fn plugin_directive_with_bad_semantics_flag_is_rejected() {
    use lute_manifest::schema::{AttrDecl, DirectiveDecl, Lowering, PluginManifest};
    let mut pkg = plugin_with_directive("a.bad", "boom");
    pkg.directives[0].semantics = vec!["totallyMadeUp".into()];
    let reg = InstalledPlugins {
        by_id: BTreeMap::from([("a.bad".to_string(), InstalledPlugin { loaded: pkg })]),
    };
    let active = vec![
        ActivePlugin { id: "lute.core".into(), options: BTreeMap::new() },
        ActivePlugin { id: "a.bad".into(), options: BTreeMap::new() },
    ];
    let (_snap, errs) = assemble_snapshot(&active, &reg);
    assert!(errs.iter().any(|e| matches!(e, lute_manifest::assemble::AssembleError::InvalidDirective { .. })), "{errs:?}");
}
```

- [ ] **Step 2: Run to verify failure** — `cargo test -p lute-manifest --test assemble plugin_directive_with_bad` → FAIL to compile (variant missing).

- [ ] **Step 3: Add the variant + call `validate_directive`**

Add `InvalidDirective { plugin: String, directive: String, msg: String }` to `AssembleError`. In the directive-merge loop, before inserting, run:

```rust
            for me in crate::validate::validate_directive(d) {
                errs.push(AssembleError::InvalidDirective { plugin: ap.id.clone(), directive: d.name.clone(), msg: format!("{me:?}") });
            }
```

- [ ] **Step 4: Run + commit**

Run: `cargo test -p lute-manifest` (all PASS), then:

```bash
cargo fmt -p lute-manifest
git add crates/lute-manifest/src/assemble.rs crates/lute-manifest/src/validate.rs
git commit -m "feat(manifest): validate plugin directives during assembly (plugin §8.1, T1.5 gap)"
```

---

## Phase 6 — Checker integration: directive-slot expansion + inactive fix-it

> The keystone. Make an active directive's `state.declares[]` open concrete `StateSchema` slots so plugin state resolves, and emit the inactive-plugin fix-it on unknown tags.

### Task 6.1: `fold_directive_slots` — expand active directive `state.declares[]` into `StateSchema`

**Files:**
- Modify: `crates/lute-check/src/check.rs` (new pre-pass, called before the walk + defassign)
- Modify: `crates/lute-check/src/meta.rs` — `StateDecl`/`StateSchema` reused; may need a helper to add a decl.
- Test: `crates/lute-check/tests/group_d.rs` or a new `crates/lute-check/tests/directive_slots.rs` (Create)

**Interfaces:**
- Consumes: `input.snapshot.directives` (each `DirectiveDecl.state: Option<DirectiveState>` with `declares: Vec<SlotDecl { scope, path: Vec<PathSegment>, shape }>`), `input.snapshot.state_shapes` (`StateShape { name, fields: Vec<Field> }`), the parsed `Document` directives (attrs), `types::{PathSegment, FromAttr, Field}`.
- Produces: for each directive USE whose declaration has `state.declares`, resolve each `SlotDecl.path` into a concrete dotted path (literal segments verbatim; `FromAttr { name }` segments replaced by the directive's attr value at that use site), then for each field of the referenced `StateShape`, insert `StateSchema.decls["<scope>.<path>.<field>"] = StateDecl { ty: field.ty (or shape descent), default: field.default, namespace: namespace_of(scope) }`. This runs in `check()` after `fold_branches`, feeding the same `schema` the walk + defassign consume.

**Concrete goal:** `::minigame{resultKey="service01" ...}` (decl path `[minigame, {fromAttr: resultKey}]`, scope `scene`, shape `minigameResult` with fields score/rank/cleared/attempts each carrying a default) opens `scene.minigame.service01.score|rank|cleared|attempts` — so `<match on="scene.minigame.service01.rank">` resolves (declared, enum with default ⇒ finite, not maybe-unset).

- [ ] **Step 1: Write the failing test**

Create `crates/lute-check/tests/directive_slots.rs`:

```rust
//! Directive-slot expansion: an active directive's state.declares[] opens
//! concrete StateSchema slots at each use site (plugin §8/§9).
use lute_check::{check, CheckInput, Mode};
use lute_manifest::provider::ProviderSet;
use lute_manifest::schema::*;
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_manifest::types::{FromAttr, PathSegment, Field, Type, Literal};
use std::collections::BTreeMap;

/// Core snapshot + a synthetic `::minigame` directive declaring
/// scene.minigame.<resultKey>.* via the `minigameResult` shape.
fn snapshot_with_minigame() -> CapabilitySnapshot {
    let mut snap = lute_manifest::core::load_core_snapshot();
    snap.state_shapes.insert(
        "minigameResult".into(),
        StateShape {
            name: "minigameResult".into(),
            fields: vec![
                Field { name: "score".into(), ty: Type::Number, default: Some(Literal::Num(0.0)), required: false, shape: None },
                Field { name: "rank".into(), ty: Type::Enum(vec!["fail".into(),"bronze".into(),"silver".into(),"gold".into()]), default: Some(Literal::Str("fail".into())), required: false, shape: None },
                Field { name: "cleared".into(), ty: Type::Bool, default: Some(Literal::Bool(false)), required: false, shape: None },
                Field { name: "attempts".into(), ty: Type::Number, default: Some(Literal::Num(0.0)), required: false, shape: None },
            ],
        },
    );
    snap.directives.insert(
        "minigame".into(),
        DirectiveDecl {
            name: "minigame".into(), layer: Some("bridge".into()),
            attrs: vec![
                AttrDecl { name: "kind".into(), required: true, ty: Type::Str, default: None },
                AttrDecl { name: "id".into(), required: true, ty: Type::Str, default: None },
                AttrDecl { name: "resultKey".into(), required: true, ty: Type::SlotId { namespace: "scene.minigame".into() }, default: None },
                AttrDecl { name: "wait".into(), required: false, ty: Type::Bool, default: Some(Literal::Bool(true)) },
            ],
            semantics: vec![],
            state: Some(DirectiveState { declares: vec![SlotDecl {
                scope: "scene".into(),
                path: vec![PathSegment::Literal("minigame".into()), PathSegment::FromAttr { from_attr: FromAttr { name: "resultKey".into(), slot_type: Some("localId".into()) } }],
                shape: "minigameResult".into(),
            }]}),
            effects: None, bridge: None,
            lower: Lowering::Builtin { kind: "builtin".into(), name: "bridgeMinigame".into() },
        },
    );
    snap
}

fn check_codes(text: &str, snap: CapabilitySnapshot) -> Vec<String> {
    let input = CheckInput { text: text.into(), uri: "t".into(), snapshot: snap, providers: ProviderSet::default(), mode: Mode::Author };
    check(&input).diagnostics.into_iter().map(|d| d.code).collect()
}

const SCENE: &str = "---\ncharacter: bianca\nseason: 1\nepisode: 5\n---\n## Shot 1.\n\
::minigame{kind=\"rhythm\" id=\"x\" resultKey=\"service01\" wait=\"true\"}\n\
<match on=\"scene.minigame.service01.rank\">\n\
<when test=\"$ == 'gold'\">:line[bianca]: a\n</when>\n\
<otherwise>:line[bianca]: b\n</otherwise>\n\
</match>\n";

#[test]
fn directive_slot_opens_scene_path() {
    let codes = check_codes(SCENE, snapshot_with_minigame());
    assert!(!codes.contains(&"E-UNDECLARED".to_string()), "slot path must be declared; got {codes:?}");
}

#[test]
fn without_directive_the_path_is_undeclared() {
    // core-only: no ::minigame directive => tag unknown AND path undeclared.
    let codes = check_codes(SCENE, lute_manifest::core::load_core_snapshot());
    assert!(codes.contains(&"E-UNDECLARED".to_string()), "core-only must flag undeclared path; got {codes:?}");
}
```

- [ ] **Step 2: Run to verify failure** — `cargo test -p lute-check --test directive_slots` → `directive_slot_opens_scene_path` FAILS (path undeclared even with the directive), `without_directive...` PASSES.

- [ ] **Step 3: Implement `fold_directive_slots`**

In `crates/lute-check/src/check.rs`, add a pre-pass called right after `fold_branches(&doc, ...)` and before building `base_ctx`, mutating the SAME `schema`:

```rust
    // 4b. Expand every active directive's `state.declares[]` into concrete state
    //     slots at each use site (plugin §8/§9): a `::minigame{resultKey="k"}`
    //     opens `scene.minigame.k.<field>` for each field of its shape. This runs
    //     before the walk + defassign so plugin-declared state resolves.
    fold_directive_slots(&doc, &input.snapshot, &mut schema);
```

Add the functions (walks all directive locations, including timeline clips, mirroring the CEL/inject walkers):

```rust
use lute_manifest::schema::{DirectiveDecl, SlotDecl, StateShape};
use lute_manifest::types::{Field, PathSegment};
use lute_syntax::ast::Directive;

fn fold_directive_slots(
    doc: &Document,
    snapshot: &CapabilitySnapshot,
    schema: &mut crate::meta::StateSchema,
) {
    for shot in &doc.shots {
        fold_slots_nodes(&shot.body, snapshot, schema);
    }
}

fn fold_slots_nodes(nodes: &[Node], snapshot: &CapabilitySnapshot, schema: &mut crate::meta::StateSchema) {
    for node in nodes {
        match node {
            Node::Directive(d) => expand_directive_slots(d, snapshot, schema),
            Node::Branch(b) => {
                for c in &b.choices {
                    fold_slots_nodes(&c.body, snapshot, schema);
                }
            }
            Node::Match(m) => {
                for arm in &m.arms {
                    match arm {
                        Arm::When { body, .. } | Arm::Otherwise { body, .. } => fold_slots_nodes(body, snapshot, schema),
                    }
                }
            }
            Node::Timeline(tl) => {
                for track in &tl.tracks {
                    for clip in &track.clips {
                        if let ClipNode::Directive(d) = &clip.node {
                            expand_directive_slots(d, snapshot, schema);
                        }
                    }
                }
            }
            Node::Set(_) => {}
        }
    }
}

fn expand_directive_slots(dir: &Directive, snapshot: &CapabilitySnapshot, schema: &mut crate::meta::StateSchema) {
    let Some(decl) = snapshot.directive(&dir.tag) else { return };
    let Some(state) = &decl.state else { return };
    for slot in &state.declares {
        let Some(base) = resolve_slot_path(slot, dir) else { continue };
        let Some(shape) = snapshot.state_shapes.get(&slot.shape) else { continue };
        let Some(ns) = crate::meta::namespace_of(&base) else { continue };
        insert_shape_fields(schema, &base, ns, shape, snapshot);
    }
}

/// Resolve a SlotDecl's path (scope + segments) into a concrete dotted path at a
/// use site: literal segments verbatim; `fromAttr` segments -> that attr's value.
fn resolve_slot_path(slot: &SlotDecl, dir: &Directive) -> Option<String> {
    let mut parts = vec![slot.scope.clone()];
    for seg in &slot.path {
        match seg {
            PathSegment::Literal(s) => parts.push(s.clone()),
            PathSegment::FromAttr { from_attr } => {
                let val = attr_str(dir, &from_attr.name)?;
                parts.push(val);
            }
        }
    }
    Some(parts.join("."))
}

/// The string value of a directive attribute (a plain string literal only; a
/// CEL/ref-valued key can't seed a static path).
fn attr_str(dir: &Directive, key: &str) -> Option<String> {
    dir.attrs.iter().find(|a| a.key == key).and_then(|a| match &a.value {
        AttrValue::Str(s) => Some(s.clone()),
        _ => None,
    })
}

/// Insert one StateDecl per shape field at `<base>.<field>`; a field that itself
/// references a nested shape recurses.
fn insert_shape_fields(
    schema: &mut crate::meta::StateSchema,
    base: &str,
    ns: crate::meta::Namespace,
    shape: &StateShape,
    snapshot: &CapabilitySnapshot,
) {
    for f in &shape.fields {
        let path = format!("{base}.{}", f.name);
        if let Some(nested_name) = &f.shape {
            if let Some(nested) = snapshot.state_shapes.get(nested_name) {
                insert_shape_fields(schema, &path, ns, nested, snapshot);
                continue;
            }
        }
        schema.decls.insert(
            path,
            crate::meta::StateDecl { ty: f.ty.clone(), default: f.default.clone(), namespace: ns },
        );
    }
}
```

Confirm imports at the top of `check.rs` include `AttrValue` (already used) and add `use lute_syntax::ast::Directive;` if not present. Check `namespace_of`/`Namespace`/`StateDecl` are reachable (they are `pub(crate)`/`pub` in `meta.rs`).

- [ ] **Step 4: Run tests + whole crate**

Run: `cargo test -p lute-check --test directive_slots` (PASS), then `cargo test -p lute-check` (all PASS — Group D tests unaffected; the new pass only adds decls for directives that declare state, which the core directives don't).

- [ ] **Step 5: Format + commit**

```bash
cargo fmt -p lute-check
git add crates/lute-check/src/check.rs crates/lute-check/tests/directive_slots.rs
git commit -m "feat(check): expand active directive state.declares into StateSchema (plugin §8/§9)"
```

### Task 6.2: inactive-plugin fix-it on `E-UNKNOWN-DIRECTIVE`

**Files:**
- Modify: `crates/lute-check/src/directives.rs:44-58` (the unknown-tag branch)
- Test: `crates/lute-check/tests/directive_slots.rs` (append)

**Interfaces:**
- Consumes: `snapshot.inactive: BTreeMap<String, String>` (tag → owning inactive plugin id, from Task 5.2), `lute_core_span::Diagnostic.fixits` (existing field).
- Produces: when an unknown tag is present in `snapshot.inactive`, the `E-UNKNOWN-DIRECTIVE` diagnostic carries a fix-it message naming the plugin to activate (discharges the T4.2 documented gap). Fix-it shape: reuse whatever `Diagnostic.fixits` element type exists (grep `fixits` in `core-span`); if it's `Vec<String>`, push a human string `"activate plugin `<id>` (add it to your profile or scene `plugins:`)"`.

- [ ] **Step 1: Write the failing test (append)**

```rust
#[test]
fn unknown_tag_from_inactive_plugin_gets_fixit() {
    let mut snap = lute_manifest::core::load_core_snapshot();
    snap.inactive.insert("minigame".into(), "idola.minigame".into());
    let text = "---\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n::minigame{kind=\"rhythm\"}\n";
    let input = CheckInput { text: text.into(), uri: "t".into(), snapshot: snap, providers: ProviderSet::default(), mode: Mode::Author };
    let res = check(&input);
    let d = res.diagnostics.iter().find(|d| d.code == "E-UNKNOWN-DIRECTIVE").expect("unknown directive");
    assert!(!d.fixits.is_empty(), "inactive-plugin unknown tag must carry a fix-it");
    assert!(d.fixits.iter().any(|f| f.title.contains("idola.minigame")), "fix-it names the plugin: {:?}", d.fixits);
}
```

- [ ] **Step 2: Run to verify failure** — `cargo test -p lute-check --test directive_slots unknown_tag_from_inactive` → FAIL (fixits empty).

- [ ] **Step 3: Attach the fix-it**

In `crates/lute-check/src/directives.rs`, the `else` for the unknown tag builds a plain `E-UNKNOWN-DIRECTIVE`. Thread the snapshot's `inactive` lookup (the function already takes `snapshot`):

```rust
    let Some(decl) = snapshot.directive(&dir.tag) else {
        let mut fixits = Vec::new();
        if let Some(plugin) = snapshot.inactive.get(&dir.tag) {
            // plugin §11.2: an installed-but-inactive tag is a diagnostic WITH a
            // fix-it, never silently accepted syntax. `Fixit` is a struct
            // (title/kind/edit) — an advisory quickfix with no text edit.
            fixits.push(lute_core_span::Fixit {
                title: format!(
                    "activate plugin `{plugin}` (add it to your profile or the scene `plugins:` block)"
                ),
                kind: "quickfix".to_string(),
                edit: Vec::new(),
            });
        }
        diags.push(Diagnostic {
            code: "E-UNKNOWN-DIRECTIVE".to_string(),
            severity: Severity::Error,
            message: format!("unknown directive `::{}`", dir.tag),
            span: dir.span,
            layer: Layer::Staging,
            fixits,
            provenance: None,
        });
        return diags;
    };
```

(The existing `diag(...)` helper hardcodes `fixits: Vec::new()`, so this branch constructs the `Diagnostic` inline to attach the fix-it. Confirm `lute_core_span::Fixit` is imported — add `use lute_core_span::Fixit;` or use the full path as above. Preserve the current message/span/layer exactly.)

- [ ] **Step 4: Run tests + whole crate**

Run: `cargo test -p lute-check` (all PASS).

- [ ] **Step 5: Format + commit**

```bash
cargo fmt -p lute-check
git add crates/lute-check/src/directives.rs crates/lute-check/tests/directive_slots.rs
git commit -m "feat(check): inactive-plugin fix-it on E-UNKNOWN-DIRECTIVE (plugin §11.2, T4.2 gap)"
```

---

## Phase 7 — Surfaces: project config, shared resolution, CLI + LSP, fixture, acceptance

> One shared `lute-manifest` resolver both surfaces call, so CLI == LSP (no divergence). A real on-disk reference project. date-minigame `ok:true` under plugins.

### Task 7.1: `lute.project.yaml` loader + shared `resolve_document_snapshot`

**Files:**
- Create: `crates/lute-manifest/src/project.rs`
- Modify: `crates/lute-manifest/src/lib.rs` (`pub mod project;`)
- Test: `crates/lute-manifest/tests/project.rs` (Create)

**Interfaces:**
- Produces:
  ```rust
  pub struct ProjectConfig {
      pub graph: crate::resolve::ProfileGraph,
      pub plugins_dir: std::path::PathBuf, // resolved absolute (project_dir.join(pluginsDir))
  }
  pub fn load_project(project_dir: &Path) -> Option<ProjectConfig>; // None if no lute.project.yaml
  /// The ONE resolution both CLI and LSP call. Given a project (or None for
  /// core-only) and the scene's parsed frontmatter (profile + plugins), resolve
  /// activation and assemble the snapshot. Returns the snapshot + any resolution
  /// diagnostics (unresolved depends / cycles / assembly dup ids), which the
  /// caller folds into the check result. Never panics.
  pub fn resolve_document_snapshot(
      project: Option<&ProjectConfig>,
      scene_profile: Option<&str>,
      scene_plugins: &std::collections::BTreeMap<String, serde_yaml::Value>,
  ) -> (crate::snapshot::CapabilitySnapshot, Vec<ResolveDiag>);
  pub struct ResolveDiag { pub message: String }
  ```
- Behavior: no project ⇒ `load_core_snapshot()` + empty diags. With a project: `load_plugins_dir(&plugins_dir)` → registry (+ load errors → `ResolveDiag`); pick profile = `scene_profile` or `graph.default_profile`; convert `scene_plugins` (`serde_yaml::Value` → `Literal` via `Literal::from_yaml`) into an `ActivationMap`; `resolve_activation(&graph, profile, &scene_local, &registry)` (+ `ResolveError` → `ResolveDiag`); `assemble_snapshot(&active, &registry)` (+ `AssembleError` → `ResolveDiag`). Deterministic throughout.

**`lute.project.yaml` format (define here, matching plugin §11 `profiles`):**
```yaml
pluginsDir: plugins/          # OPTIONAL; default "plugins/"
defaultProfile: story
profiles:
  global:
    plugins: { lute.core: true }
  story:
    plugins: { idola.minigame: true }   # etc.
  date-minigame:
    extends: story
    plugins:
      idola.minigame: { resultScope: scene, allowedKinds: [rhythm] }
```
The loader deserializes `profiles` (a map of name → `{ extends?, plugins: map<id, true|options-map> }`) into `ProfileGraph`. `true` normalizes to an empty option map (plugin §11: presence activates). Reuse `Literal::from_yaml` for option values.

- [ ] **Step 1: Write the failing test**

Create `crates/lute-manifest/tests/project.rs`:

```rust
use lute_manifest::project::{load_project, resolve_document_snapshot};
use std::collections::BTreeMap;
use std::fs;

fn write_project(root: &std::path::Path) {
    // plugin package
    let pdir = root.join("plugins/idola.minigame/directives");
    fs::create_dir_all(&pdir).unwrap();
    fs::write(root.join("plugins/idola.minigame/plugin.yaml"),
        "id: idola.minigame\nversion: 0.1.0\nkind: capability\nexports:\n  directives: directives/\n").unwrap();
    fs::write(pdir.join("d.yaml"),
        "directives:\n  - { name: minigame, attrs: [ { name: kind, type: string } ], lower: { kind: builtin, name: n } }\n").unwrap();
    // project config
    fs::write(root.join("lute.project.yaml"),
        "pluginsDir: plugins/\ndefaultProfile: date\nprofiles:\n  global:\n    plugins: { lute.core: true }\n  date:\n    plugins: { idola.minigame: true }\n").unwrap();
}

#[test]
fn resolves_project_snapshot_with_active_plugin() {
    let root = std::env::temp_dir().join(format!("lute_proj_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    write_project(&root);
    let proj = load_project(&root).expect("project loads");
    let (snap, diags) = resolve_document_snapshot(Some(&proj), None, &BTreeMap::new());
    assert!(diags.is_empty(), "{:?}", diags.iter().map(|d| &d.message).collect::<Vec<_>>());
    assert!(snap.directive("minigame").is_some(), "active plugin directive present");
    fs::remove_dir_all(&root).ok();
}

#[test]
fn no_project_is_core_only() {
    let (snap, diags) = resolve_document_snapshot(None, None, &BTreeMap::new());
    assert!(diags.is_empty());
    assert!(snap.directive("bg").is_some());
    assert!(snap.directive("minigame").is_none());
}
```

- [ ] **Step 2: Run to verify failure** — `cargo test -p lute-manifest --test project` → FAIL to compile.

- [ ] **Step 3: Implement `project.rs`** (deserialize config → `ProfileGraph`; implement both fns per the Behavior above). Add `pub mod project;` to `lib.rs`. Key details:
  - Deserialize `profiles` values with a serde helper that accepts either `true` (bool) or a map for each plugin entry — use `serde_yaml::Value` then convert: `true` → empty `BTreeMap`, mapping → `Literal::from_yaml` per value.
  - `plugins_dir = project_dir.join(pluginsDir.unwrap_or("plugins/"))`.
  - Map every `ResolveError`/`AssembleError`/`LoadError` to a `ResolveDiag { message: format!("{e:?}") }` (human enough for now; a structured diag mapping is a later refinement — document it).

- [ ] **Step 4: Run + commit**

Run: `cargo test -p lute-manifest` (all PASS), then:

```bash
cargo fmt -p lute-manifest
git add crates/lute-manifest/src/project.rs crates/lute-manifest/src/lib.rs
git commit -m "feat(manifest): project config loader + shared resolve_document_snapshot (plugin §11)"
```

### Task 7.2: on-disk reference project fixture (`idola.minigame`)

**Files:**
- Create: `docs/examples/idola-project/lute.project.yaml`
- Create: `docs/examples/idola-project/plugins/idola.minigame/plugin.yaml` (Appendix A)
- Create: `docs/examples/idola-project/plugins/idola.minigame/state/shapes.yaml` (Appendix A)
- Create: `docs/examples/idola-project/plugins/idola.minigame/directives/minigame.yaml` (Appendix A)
- Create: `docs/examples/idola-project/plugins/idola.minigame/providers/minigame.yaml`
- Create: `docs/examples/idola-project/plugins/idola.minigame/bridge/minigame.yaml`
- Create: `docs/examples/idola-project/catalog/minigame.yaml` (provider snapshot with `bianca_service_01`)

**Interfaces:** none (data fixture). This is the reference project the acceptance tests point at. `date-minigame.lute` selects `profile: date-minigame` and scene-local `plugins: { idola.minigame: { resultScope: scene, allowedKinds: [rhythm] } }` — the project must define that profile.

- [ ] **Step 1: Write the manifest files verbatim from Appendix A**

`plugins/idola.minigame/plugin.yaml`:
```yaml
id: idola.minigame
version: 0.1.0
kind: capability
depends: [ { id: lute.core, range: "^0.0.1" } ]
exports:
  directives: directives/
  state: state/
  providers: providers/
  bridge: bridge/
options:
  - { name: resultScope, type: { enum: [scene, run] }, default: scene }
  - { name: allowedKinds, type: { list: { enum: [rhythm, puzzle, timing] } }, default: [rhythm, puzzle, timing] }
```

`plugins/idola.minigame/state/shapes.yaml`:
```yaml
stateShapes:
  - name: minigameResult
    fields:
      - { name: score,    type: number, default: 0 }
      - { name: rank,     type: { enum: [fail, bronze, silver, gold] }, default: fail }
      - { name: cleared,  type: bool,   default: false }
      - { name: attempts, type: number, default: 0 }
```

`plugins/idola.minigame/directives/minigame.yaml`:
```yaml
directives:
  - name: minigame
    layer: bridge
    attrs:
      - { name: kind,      required: true, type: { enumFromOption: allowedKinds } }
      - { name: id,        required: true, type: { providerRef: minigameId } }
      - { name: resultKey, required: true, type: { slotId: { namespace: scene.minigame } } }
      - { name: wait,      type: bool, default: true }
    semantics: [ "writes.sceneState", "bridgeCall" ]
    state:
      declares:
        - { scope: scene, path: [minigame, { fromAttr: { name: resultKey, slotType: localId } }], shape: minigameResult }
    effects:
      writes:
        - { scope: scene, path: [minigame, { fromAttr: { name: resultKey } }, score],    value: { fromBridgeResult: score } }
        - { scope: scene, path: [minigame, { fromAttr: { name: resultKey } }, rank],     value: { fromBridgeResult: rank } }
        - { scope: scene, path: [minigame, { fromAttr: { name: resultKey } }, cleared],  value: { fromBridgeResult: cleared } }
        - { scope: scene, path: [minigame, { fromAttr: { name: resultKey } }, attempts], value: { op: increment, by: 1 } }
    bridge: { service: minigame, operation: play }
    lower: { kind: builtin, name: bridgeMinigame }
```

`plugins/idola.minigame/providers/minigame.yaml`:
```yaml
providers:
  - name: minigameId
    idShape: "Ident"
    snapshot: minigame-id
```

`plugins/idola.minigame/bridge/minigame.yaml`:
```yaml
bridgeCapabilities:
  - service: minigame
    operation: play
    replay: recorded
    result:
      - { name: score,   type: number }
      - { name: rank,    type: { enum: [fail, bronze, silver, gold] } }
      - { name: cleared, type: bool }
```

`catalog/minigame.yaml` (provider snapshot, `ProviderSet::load` format — the `entries` key is the provider `name`, i.e. `minigameId`):
```yaml
manifestVersion: "pending"
providerVersion: "1"
stale: false
entries:
  minigameId: [bianca_service_01]
```

`lute.project.yaml`:
```yaml
pluginsDir: plugins/
defaultProfile: date-minigame
profiles:
  global:
    plugins: { lute.core: true }
  story:
    plugins: { idola.minigame: true }
  date:
    extends: story
    plugins: { idola.minigame: true }
  date-minigame:
    extends: date
    plugins:
      idola.minigame: { resultScope: scene, allowedKinds: [rhythm, timing] }
```

- [ ] **Step 2: Sanity-load the project (temporary unit assertion or manual)**

Run a quick check that `load_plugins_dir` reads the package cleanly (add a throwaway assertion in `project.rs` tests pointing at `../../docs/examples/idola-project`, or verify in Task 7.4's acceptance). No standalone commit test needed — this is data.

- [ ] **Step 3: Commit the fixture**

```bash
git add docs/examples/idola-project
git commit -m "test(fixture): on-disk idola.minigame reference project (plugin Appendix A)"
```

### Task 7.3: CLI `--project` wiring

**Files:**
- Modify: `crates/lute-cli/src/main.rs` (add `--project`, call `resolve_document_snapshot`)
- Modify: `crates/lute-cli/src/main.rs` — parse the scene's frontmatter `profile`/`plugins` (via `lute_check::parse_meta` on the parsed doc, or a lightweight YAML peek) to pass to the resolver.
- Test: `crates/lute-cli/tests/` (append a plugin-loaded acceptance test)

**Interfaces:**
- Consumes: `lute_manifest::project::{load_project, resolve_document_snapshot, ProjectConfig}`, `lute_check::parse_meta` (to get `TypedMeta.profile` + `.plugins`), existing `CheckInput`.
- Produces: `lute check <file> --project <dir> [--providers <dir>] [--json]` builds the snapshot from the project + scene meta; folds any `ResolveDiag` into stderr/human output (or a synthetic diagnostic). Without `--project`, behavior is EXACTLY as today (core-only) — do not change existing tests.

- [ ] **Step 1: Write the failing acceptance test**

In a new `crates/lute-cli/tests/plugin_loaded.rs`:

```rust
use std::process::Command;

fn lute_bin() -> &'static str { env!("CARGO_BIN_EXE_lute") }

#[test]
fn date_minigame_is_clean_with_plugin_project() {
    let out = Command::new(lute_bin())
        .args([
            "check",
            "../../docs/examples/date-minigame.lute",
            "--project", "../../docs/examples/idola-project",
            "--providers", "../../docs/examples/idola-project/catalog",
            "--json",
        ])
        .output()
        .expect("run lute");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("\"ok\": true"), "expected ok:true, got: {stdout}\nstderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(out.status.code(), Some(0), "exit 0 on clean");
}

#[test]
fn date_minigame_core_only_still_errors() {
    // REGRESSION GUARD: without --project, the existing core-only contract holds.
    let out = Command::new(lute_bin())
        .args(["check", "../../docs/examples/date-minigame.lute", "--json"])
        .output().expect("run lute");
    assert_eq!(out.status.code(), Some(1), "core-only still exits 1");
}
```

- [ ] **Step 2: Run to verify failure** — `cargo test -p lute-cli --test plugin_loaded` → `date_minigame_is_clean_with_plugin_project` FAILS (`--project` unknown arg / ok:false).

- [ ] **Step 3: Wire `--project`**

In `crates/lute-cli/src/main.rs`:
- Add `#[arg(long, value_name = "DIR")] project: Option<PathBuf>` to the `check` subcommand.
- In the check handler: parse the doc (`lute_syntax::parse`), run `parse_meta` (needs a snapshot — parse twice or restructure: first do a cheap frontmatter read for profile/plugins, OR call `parse_meta` with a default snapshot just to lift `profile`/`plugins`, which are built-in keys not gated by the snapshot). Simpler: `let (meta0, _) = lute_check::parse_meta(&doc.meta, &CapabilitySnapshot::default());` then use `meta0.profile` + `meta0.plugins`.
- `let project = project.as_deref().and_then(load_project);`
- `let (snapshot, rdiags) = resolve_document_snapshot(project.as_ref(), meta0.profile.as_deref(), &meta0.plugins);`
- Build `CheckInput { snapshot, ... }` as before; if `!rdiags.is_empty()`, print each to stderr (human) or include in JSON under a new top-level (keep it simple: stderr lines `lute: resolve: <msg>`; do not fail the build on resolve diags unless they blocked snapshot assembly — but a resolve error that left the plugin unloaded WILL surface as `E-UNKNOWN-DIRECTIVE`, which is the honest outcome).

- [ ] **Step 4: Run tests + whole crate**

Run: `cargo test -p lute-cli` (all PASS — new acceptance + all existing core-only tests unchanged).

- [ ] **Step 5: Format + commit**

```bash
cargo fmt -p lute-cli
git add crates/lute-cli/src/main.rs crates/lute-cli/tests/plugin_loaded.rs
git commit -m "feat(cli): --project loads plugins; date-minigame checks clean (plugin §4/§11)"
```

### Task 7.4: LSP project discovery + divergence under plugins

**Files:**
- Modify: `crates/lute-lsp/src/backend.rs` (project discovery from doc URI; call `resolve_document_snapshot`)
- Test: `crates/lute-lsp/tests/divergence.rs` (append a plugin-loaded divergence case)

**Interfaces:**
- Consumes: same `resolve_document_snapshot` + `load_project`; `parse_meta` for scene profile/plugins; the doc's file path from its `Uri`.
- Produces: `analyze` (and the hover/completion/def/refs handlers) build the snapshot via the shared resolver — discovering the project by walking up from the document's directory to the first `lute.project.yaml`. When none is found, core-only (today's behavior). **Divergence invariant:** because both CLI and LSP call the identical `resolve_document_snapshot`, they build byte-identical snapshots → identical diagnostics. Add a divergence test that loads the fixture project and asserts headless == LSP under plugins.

- [ ] **Step 1: Write the failing divergence test (append to `divergence.rs`)**

```rust
#[test]
fn divergence_holds_under_plugin_project() {
    use lute_manifest::project::{load_project, resolve_document_snapshot};
    let text = std::fs::read_to_string("../../docs/examples/date-minigame.lute").unwrap();
    let proj = load_project(std::path::Path::new("../../docs/examples/idola-project")).unwrap();
    // Lift scene profile/plugins the same way the surfaces do.
    let (doc, _) = lute_syntax::parse(&text);
    let (meta0, _) = lute_check::parse_meta(&doc.meta, &lute_manifest::snapshot::CapabilitySnapshot::default());
    let (snapshot, _rd) = resolve_document_snapshot(Some(&proj), meta0.profile.as_deref(), &meta0.plugins);
    let providers = lute_manifest::provider::ProviderSet::load("../../docs/examples/idola-project/catalog");

    let input = lute_check::CheckInput { text: text.clone(), uri: "date".into(), snapshot, providers, mode: lute_check::Mode::Author };
    let res = lute_check::check(&input);
    // With the plugin loaded, the scene is clean.
    let errs: Vec<_> = res.diagnostics.iter().filter(|d| d.severity == lute_core_span::Severity::Error).collect();
    assert!(errs.is_empty(), "plugin-loaded date-minigame must be clean; got {errs:#?}");

    // headless positions vs converted LSP positions agree (the existing helpers
    // normalize_headless / normalize_lsp — reuse them here).
    let idx = lute_core_span::TextIndex::new(&text);
    for d in &res.diagnostics {
        let lsp = lute_lsp::convert::to_lsp_diagnostic(d, &idx);
        let p = idx.position(d.span.byte_start);
        assert_eq!(lsp.range.start.line, p.line - 1);
    }
}
```

(Adjust to reuse whatever `normalize_headless`/`normalize_lsp` helpers already exist in `divergence.rs`; the point is one snapshot, one position path.)

- [ ] **Step 2: Run to verify failure** — `cargo test -p lute-lsp --test divergence divergence_holds_under_plugin` → may FAIL first if `date-minigame` isn't clean because the `allowedKinds` enum (`enumFromOption`) or providerRef isn't satisfied. Diagnose: the scene uses `kind="rhythm"` (in `allowedKinds`) and `id="bianca_service_01"` (in the catalog) — both must resolve. If `E-BAD-ENUM`/`E-UNKNOWN-ID` appears, verify (a) the resolved `ResolvedPlugin.options` carries `allowedKinds` so `resolve_option_domain` finds it, and (b) `--providers` catalog has `minigameId: [bianca_service_01]`. Fix data/wiring until clean.

- [ ] **Step 3: Implement LSP project discovery**

In `crates/lute-lsp/src/backend.rs`, factor the snapshot build into a helper the analyze + feature handlers all call:

```rust
    /// Build the capability snapshot for `uri`'s document by discovering a
    /// `lute.project.yaml` above the file and resolving via the SHARED
    /// `resolve_document_snapshot` (identical to the CLI — no divergence).
    fn snapshot_for(&self, uri: &Uri, text: &str) -> lute_manifest::snapshot::CapabilitySnapshot {
        let (doc, _) = lute_syntax::parse(text);
        let (meta0, _) = lute_check::parse_meta(&doc.meta, &lute_manifest::snapshot::CapabilitySnapshot::default());
        let project = uri_to_path(uri)
            .and_then(|p| find_project_root(&p))
            .and_then(|root| lute_manifest::project::load_project(&root));
        let (snap, _diags) = lute_manifest::project::resolve_document_snapshot(
            project.as_ref(), meta0.profile.as_deref(), &meta0.plugins,
        );
        snap
    }
```

Add `uri_to_path` (parse the `file:` URI to a `PathBuf`; `Uri` is `tower_lsp_server::ls_types::Uri` — use `.path()` / percent-decoding) and `find_project_root` (walk parents until `lute.project.yaml` exists; return `None` at fs root). Replace every `lute_manifest::core::load_core_snapshot()` call in `backend.rs` (analyze + hover + completion + definition + references handlers) with `self.snapshot_for(&uri, &text)`. Keep `providers` as `ProviderSet::default()` for now unless a project catalog convention is added (document: LSP provider-catalog discovery is a follow-up; core-only providers means a `providerRef` id resolves `Absent` in Author mode → a `catalog-stale`/unknown-id diagnostic, which is honest; the CLI acceptance uses explicit `--providers`).

> **Divergence note:** the CLI passes `--providers` explicitly; the LSP currently defaults to empty. For the divergence *test* above, both sides use the SAME `ProviderSet::load(catalog)` so positions match. In production the LSP's empty providers is a known limitation (documented), not a divergence in the position path (the invariant is about position computation, not catalog contents). If strict parity is required, add project-catalog discovery symmetrically — flag as a follow-up.

- [ ] **Step 4: Run tests + whole crate**

Run: `cargo test -p lute-lsp` (all PASS — existing divergence + convert tests + new plugin case).

- [ ] **Step 5: Format + commit**

```bash
cargo fmt -p lute-lsp
git add crates/lute-lsp/src/backend.rs crates/lute-lsp/tests/divergence.rs
git commit -m "feat(lsp): project discovery via shared resolver; divergence holds under plugins (plugin §11)"
```

### Task 7.5: `catalog refresh` re-stamps the project catalog against the resolved version

**Files:**
- Modify: `crates/lute-cli/src/main.rs` (`run_refresh` uses the resolved project `capabilityVersion` when `--project` given)
- Test: `crates/lute-cli/tests/plugin_loaded.rs` (append)

**Interfaces:**
- Today `catalog refresh <dir>` stamps `manifestVersion` to `load_core_snapshot().version`. Under a project, the correct stamp is the RESOLVED multi-plugin `capabilityVersion`. Add optional `--project <dir>` to `catalog refresh`; when present, resolve the snapshot (no scene ⇒ default profile) and stamp that version. Without it, behavior unchanged (core version).

- [ ] **Step 1: Write the failing test (append)**

```rust
#[test]
fn refresh_stamps_resolved_version_under_project() {
    // Copy the fixture catalog to a temp dir, refresh with --project, assert the
    // manifestVersion changed away from "pending" and matches the resolved snap.
    let tmp = std::env::temp_dir().join(format!("lute_cat_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    std::fs::copy("../../docs/examples/idola-project/catalog/minigame.yaml", tmp.join("minigame.yaml")).unwrap();
    let out = std::process::Command::new(lute_bin())
        .args(["catalog", "refresh", tmp.to_str().unwrap(), "--project", "../../docs/examples/idola-project"])
        .output().expect("refresh");
    assert_eq!(out.status.code(), Some(0));
    let after = std::fs::read_to_string(tmp.join("minigame.yaml")).unwrap();
    assert!(!after.contains("pending"), "manifestVersion must be re-stamped: {after}");
    std::fs::remove_dir_all(&tmp).ok();
}
```

- [ ] **Step 2: Run to verify failure** — FAIL (`--project` unknown on `catalog refresh`).

- [ ] **Step 3: Wire it** — add `--project` to the `Refresh` variant; in `run_refresh`, if `project` is `Some`, `let version = resolve_document_snapshot(load_project(p).as_ref(), None, &BTreeMap::new()).0.version;` else keep `load_core_snapshot().version`.

- [ ] **Step 4: Run + commit**

Run: `cargo test -p lute-cli` (all PASS):

```bash
cargo fmt -p lute-cli
git add crates/lute-cli/src/main.rs crates/lute-cli/tests/plugin_loaded.rs
git commit -m "feat(cli): catalog refresh stamps resolved project capabilityVersion (plugin §10/§13)"
```

---

## Phase 8 — Final verification gate

- [ ] **Step 1: Full workspace suite**

```bash
export PATH="$HOME/.cargo/bin:$PATH" && cd /Users/journey/Workspace/lute/.worktrees/lute-lsp-rust
cargo test --workspace
```
Expected: all pass. Count ≈ 204 (baseline) + Group D (7) + manifest (F1/F2/loader/assemble/project ≈ 20) + check integration (≈ 4) + surface acceptance (≈ 5). Record the exact final count in the ledger.

- [ ] **Step 2: Clippy + fmt**

```bash
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
```
Expected: 0 warnings; clean.

- [ ] **Step 3: Examples — core-only unchanged, plugin-loaded clean**

```bash
./target/debug/lute check docs/examples/bianca-s01ep02.lute --json                      # ok:true, exit 0
./target/debug/lute check docs/examples/date-minigame.lute --json                       # ok:false, exit 1 (core-only, UNCHANGED)
./target/debug/lute check docs/examples/date-minigame.lute --json \
    --project docs/examples/idola-project --providers docs/examples/idola-project/catalog   # ok:true, exit 0
```

- [ ] **Step 4: tree-sitter unchanged** (out of scope this plan, but confirm no regression)

```bash
(cd tree-sitter-lute && npx --yes tree-sitter-cli@latest test)   # 25/25
```

- [ ] **Step 5: Clean trees**

```bash
git status --short
(cd /Users/journey/Workspace/lute && git status --short)
```
Expected: both empty.

---

## Deferred scope (explicitly out of this plan)

- **§6.9 assetKind decomposition** (segment compose/decompose, `resolve: query` provider matching, `fallback` hook registry, `persistence`/preview, `canonicalAssetId` redirect). A self-contained id-composition subsystem no current example exercises; the loader IGNORES an `assetkinds` export (does not error). **Its own plan** (user decision, 2026-07-02).
- **tree-sitter `capabilityVersion` re-stamp** under multi-plugin snapshots. The grammar is data-not-grammar (§3.1), so the parser itself does not change per plugin; only the stamp would. Deferred with §6.9 (assetKinds are the main grammar-adjacent surface).
- **Structured resolve diagnostics.** `ResolveDiag` is currently `{ message }` (`Debug`-formatted errors). Mapping load/resolve/assemble errors to spanned `Diagnostic`s with codes is a refinement.
- **LSP project-catalog discovery.** The LSP defaults to empty providers; symmetric `--providers`-equivalent project catalog discovery is a follow-up (the position-path divergence invariant is unaffected).
- **`E-REF-TYPE`, `commands_preview` depth, `E-WRITE-CONFLICT` property precision, references `includeDeclaration`** — pre-existing documented deferrals, unchanged.
- **Group A beyond F1/F2** — F1 (`Literal::Map`) and F2 (dep closure) are IN this plan; no other Group A items remain.

## Self-Review

- **Spec coverage:** plugin §4 loader (Task 4.1/4.2) ✓; §5 manifest (existing + `FrontmatterFile`) ✓; §6.1-6.8 exports merged in loader/assembler ✓; §6.9 deferred (documented) ✓; §7 type system + `Literal::Map` (F1, Task 2.1) ✓; §8 directive + `state.declares` expansion (Task 6.1) ✓; §8.1 semantics validation wired (Task 5.3) ✓; §10 providers (existing + catalog fixture + refresh, Task 7.5) ✓; §11.1 resolution order + dep closure (F2, Task 3.2) ✓; §11.2 map deep-merge (Task 2.3) + inactive fix-it (Task 6.2) ✓; §13 snapshot + version fold (Task 5.1/5.2) ✓; §14 reserved names (Task 5.2) ✓. Group D C1/C2/C4/C5 (Phase 1) ✓.
- **Type consistency:** `resolve_activation` 4-arg signature is used identically in Tasks 3.2, 7.1, and tests. `InstalledPlugin { loaded: LoadedPlugin }` + `.manifest()` used consistently after Task 4.2. `Literal::Map(BTreeMap<String,Literal>)`, `Literal::from_yaml`, `assemble_snapshot`, `resolve_document_snapshot`, `fold_directive_slots` names are stable across their producing + consuming tasks.
- **Placeholders:** none — every code/test step shows real content.
- **Regression discipline:** Phase 1 carries explicit regression-guard tests (C1b, C2b, C4-conjunctive); Phase 7 carries core-only regression guards (CLI exit 1, existing divergence). Existing core-only tests are never edited.
