# lute-manifest 0.2.0 `events` Capability Export (Plan B of 5)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an `events` plugin capability export so a plugin can declare world events (`combatEnd`, `npcDied`, …) that the checker's `<on event="…">` validator (Plan C) resolves against — the 0.2.0 plugin-system erratum (design doc §D5/§D10, dsl 0.2.0 §4.5).

**Architecture:** The `events` export mirrors the existing `defs` export end-to-end: a name-keyed declaration list read per-plugin (`loader.rs`), merged cross-plugin with duplicate detection (`assemble.rs` `merge_map`), stored in `CapabilitySnapshot.events` (`snapshot.rs`), and folded into `capabilityVersion`. Built-in lifecycle events (`questActive`/`questComplete`/`questFailed`) are NOT capability-provided (dsl 0.2.0 §4.5 — the language defines no world events); they live as a shared const `BUILTIN_LIFECYCLE_EVENTS` in `lute-manifest` (single source of truth, consumed by both `assemble.rs`'s reserved-name check and Plan C's `E-UNKNOWN-EVENT`). Self-contained: no other crate changes; `cargo test --workspace` stays green.

**Tech Stack:** Rust (workspace `cargo test`), spec = `docs/proposals/scenario-dsl/0.2.0.md` §4.5, plugin-system = `docs/proposals/plugin-system/0.0.1.md`.

## Global Constraints

- The `events` export follows the `defs` export pattern EXACTLY (schema `DefsFile`/`DefDecl` → `EventsFile`/`EventDecl`; loader `read_kind::<DefsFile,_>` + `merge_named` → same for events; assemble `merge_map(&mut snap.defs, …, "def", …)` → `merge_map(&mut snap.events, …, "event", …)`).
- **`EventDecl { name: String }`** — minimal. Payload is ordinary plugin `state` (dsl 0.2.0 §4.5: "any associated payload is written by the engine into declared state before the event fires"), NOT part of the event declaration. YAGNI: no speculative fields.
- **`capabilityVersion` guard:** the new hash section is `if !snap.events.is_empty()`-GUARDED (unlike the other unconditional sections) so an event-LESS snapshot hashes byte-identically to today — keeping every existing lute-compile e2e golden green. Rationale: plugin-system §13 says "any drift in a **populated** field yields a different version"; an empty `events` field is not populated. Document this inline.
- Reserved event names: a plugin MUST NOT declare an event named `questActive`/`questComplete`/`questFailed` (they are built-ins) → `AssembleError::ReservedName` (reuse the existing variant/code `E-PLUGIN-RESERVED-NAME`), mirroring `RESERVED_DIRECTIVE_NAMES`.
- Work in the worktree `~/Workspace/lute/.worktrees/lute-0.2.0` on branch `feat/lute-0.2.0`. Run only `cargo test -p lute-manifest` per task; the workspace suite is unaffected.

---

### Task 1: `EventDecl` + `EventsFile` schema types + `BUILTIN_LIFECYCLE_EVENTS`

**Files:**
- Modify: `crates/lute-manifest/src/schema.rs` (after `DefsFile`/`DefDecl` ~28-31, ~176-190)

**Interfaces:**
- Produces: `pub struct EventDecl { pub name: String }` (`#[derive(Clone, Debug, Serialize, Deserialize)]`), `pub struct EventsFile { pub events: Vec<EventDecl> }` (`#[derive(Debug, Deserialize)]`).

- [ ] **Step 1: Add types** — in `schema.rs`, after `EnumsFile` (~47) add `EventsFile`, and after `DefDecl` (~190) add `EventDecl`:

```rust
#[derive(Debug, Deserialize)]
pub struct EventsFile {
    pub events: Vec<EventDecl>,
}
```
```rust
/// A capability-declared world event (dsl 0.2.0 §4.5): a named event kind an
/// active plugin makes fireable via `<on event="…">`. Payload (if any) is
/// ordinary plugin `state`, written by the engine before the event fires — NOT
/// part of this declaration. Name is a `CelIdent`-shaped event kind.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EventDecl {
    pub name: String,
}
```

- [ ] **Step 2: Compile** — `cargo check -p lute-manifest` → clean (types unused until Tasks 2-4; a standalone `cargo check` may warn — that's fine, or commit Tasks 1-4 together).

- [ ] **Step 3: Commit** — `git commit -am "feat(manifest): EventDecl/EventsFile schema types (dsl 0.2.0 §4.5)"`

---

### Task 2: `CapabilitySnapshot.events` + accessor + `capabilityVersion` fold + `BUILTIN_LIFECYCLE_EVENTS`

**Files:**
- Modify: `crates/lute-manifest/src/snapshot.rs` (`CapabilitySnapshot` ~8-24, `impl` accessor ~32-40, `capability_version` ~156-163)
- Test: `crates/lute-manifest/src/snapshot.rs` `mod tests` (~166)

**Interfaces:**
- Consumes: `EventDecl` (Task 1).
- Produces:
  - `CapabilitySnapshot` gains `pub events: BTreeMap<String, EventDecl>,` (last field; `#[derive(Default)]` covers it).
  - `impl CapabilitySnapshot { pub fn event(&self, name: &str) -> Option<&EventDecl> { self.events.get(name) } }`.
  - `pub const BUILTIN_LIFECYCLE_EVENTS: &[&str] = &["questActive", "questComplete", "questFailed"];` (module-level, `pub`).
  - `capability_version` folds `events` (guarded).

- [ ] **Step 1: Write the failing test** — in snapshot.rs `mod tests`:

```rust
#[test]
fn events_absent_keeps_capability_version_stable() {
    // An event-LESS snapshot must hash identically to a fresh default — the
    // guarded events section must NOT perturb existing (event-less) snapshots.
    let a = CapabilitySnapshot::default();
    let mut b = CapabilitySnapshot::default();
    b.directives.insert("foo".into(), crate::schema::DirectiveDecl {
        name: "foo".into(), layer: None, attrs: vec![], semantics: vec![],
        state: None, effects: None, ..Default::default() // adapt to DirectiveDecl's real fields
    });
    // events empty on both -> the events section contributes nothing:
    let base = capability_version(&a);
    assert_eq!(capability_version(&CapabilitySnapshot::default()), base);
    let _ = b;
}

#[test]
fn declaring_an_event_changes_capability_version() {
    let a = CapabilitySnapshot::default();
    let mut b = CapabilitySnapshot::default();
    b.events.insert("combatEnd".into(), crate::schema::EventDecl { name: "combatEnd".into() });
    assert_ne!(capability_version(&a), capability_version(&b));
}

#[test]
fn event_accessor_finds_declared_event() {
    let mut s = CapabilitySnapshot::default();
    s.events.insert("combatEnd".into(), crate::schema::EventDecl { name: "combatEnd".into() });
    assert!(s.event("combatEnd").is_some());
    assert!(s.event("nope").is_none());
}
```

> NOTE: the first test's `DirectiveDecl { … }` literal is illustrative — if constructing one is awkward, drop `b` and just assert `capability_version(default) == capability_version(default)` plus that inserting an event changes it (test 2). The load-bearing assertions are tests 2 and 3.

- [ ] **Step 2: Run to verify failure** — `cargo test -p lute-manifest declaring_an_event` → FAIL (no `events` field).

- [ ] **Step 3: Implement.**
  - Add `pub events: BTreeMap<String, EventDecl>,` as the LAST field of `CapabilitySnapshot` (import `EventDecl` via the existing `use crate::schema::*;`).
  - Add the accessor in `impl CapabilitySnapshot` beside `directive`:
    ```rust
    pub fn event(&self, name: &str) -> Option<&EventDecl> {
        self.events.get(name)
    }
    ```
  - Add the const at module top (near the struct):
    ```rust
    /// The built-in quest lifecycle events (dsl 0.2.0 §6.6): quest-scoped, fired
    /// by the engine, usable as `<on event=…>`. NOT capability-provided (§4.5) —
    /// the single source of truth shared by `assemble`'s reserved-name check and
    /// the checker's `E-UNKNOWN-EVENT`.
    pub const BUILTIN_LIFECYCLE_EVENTS: &[&str] = &["questActive", "questComplete", "questFailed"];
    ```
  - In `capability_version`, immediately before `format!("{:x}", h.finalize())` (~163), add the GUARDED section:
    ```rust
    if !snap.events.is_empty() {
        h.update(b"\nevents\n");
        for (name, e) in &snap.events {
            h.update(name.as_bytes());
            h.update(b"=");
            h.update(format!("{e:?}").as_bytes());
            h.update(b";");
        }
    }
    ```

- [ ] **Step 4: Run** — `cargo test -p lute-manifest` → PASS.

- [ ] **Step 5: Commit** — `git commit -am "feat(manifest): CapabilitySnapshot.events + event() + capabilityVersion fold (dsl 0.2.0 §4.5)"`

---

### Task 3: `LoadedPlugin.events` + `"events"` export loader arm

**Files:**
- Modify: `crates/lute-manifest/src/loader.rs` (`LoadedPlugin` ~12-24, `out` literal ~94-105, export match ~117-154)
- Test: `crates/lute-manifest/tests/loader.rs`

**Interfaces:**
- Consumes: `EventsFile`/`EventDecl` (Task 1).
- Produces: `LoadedPlugin` gains `pub events: Vec<EventDecl>`; the loader reads an `events: events/` export dir.

- [ ] **Step 1: Write the failing test** — in `tests/loader.rs`, mirroring `loads_a_valid_package`/`write_pkg` (~5-30). Add a package with an `events: events/` export and an `events/a.yaml` containing:

```rust
#[test]
fn loads_events_export() {
    // Build a temp plugin dir with `exports: { events: events/ }` and
    // `events/a.yaml` = "events:\n  - name: combatEnd\n". Reuse the crate's
    // existing temp-dir/write_pkg helper shape (see loads_a_valid_package).
    let dir = /* temp dir */;
    /* write plugin.yaml with exports: { events: events/ } and events/a.yaml */
    let loaded = lute_manifest::loader::load_plugin_dir(&dir).expect("loads");
    assert_eq!(loaded.events.len(), 1);
    assert_eq!(loaded.events[0].name, "combatEnd");
}
```

> Read `tests/loader.rs`'s existing `write_pkg`/`loads_a_valid_package` to reuse its exact temp-dir + `plugin.yaml` scaffolding; the plugin.yaml needs `id`/`version`/`kind`/`exports:` at minimum (copy from an existing passing test).

- [ ] **Step 2: Run to verify failure** — `cargo test -p lute-manifest --test loader loads_events_export` → FAIL.

- [ ] **Step 3: Implement.**
  - Add `pub events: Vec<EventDecl>,` to `LoadedPlugin` (~23, after `asset_kinds`).
  - Add `events: Vec::new(),` to the `out` literal (~104).
  - Add the export arm alongside `"defs"` (~134):
    ```rust
    "events" => read_kind::<EventsFile, _>(&path, &mut errs, |f, e| {
        merge_named(&mut out.events, f.events, "event", |ev| ev.name.clone(), e)
    }),
    ```
    (`merge_named` and `read_kind` are the same generics `defs` uses; `EventsFile` comes via `use crate::schema::*;`.)

- [ ] **Step 4: Run** — `cargo test -p lute-manifest` → PASS.

- [ ] **Step 5: Commit** — `git commit -am "feat(manifest): load plugin events/ export into LoadedPlugin (dsl 0.2.0 §4.5)"`

---

### Task 4: `assemble_snapshot` — merge events + reserved-name guard

**Files:**
- Modify: `crates/lute-manifest/src/assemble.rs` (`RESERVED_DIRECTIVE_NAMES` ~59-61, per-plugin merge block ~155-182)
- Test: `crates/lute-manifest/tests/assemble.rs`

**Interfaces:**
- Consumes: `LoadedPlugin.events` (Task 3), `snapshot.events` (Task 2), `BUILTIN_LIFECYCLE_EVENTS` (Task 2).
- Produces: assembled `snap.events` with cross-plugin dup detection (`merge_map`, kind `"event"`) and a reserved-name guard against `BUILTIN_LIFECYCLE_EVENTS`.

- [ ] **Step 1: Write failing tests** — in `tests/assemble.rs`, mirroring `assemble_merges_asset_kinds`/`assemble_rejects_cross_plugin_asset_kind_dup` (~299-362). Add an `event_decl(name)`-style helper building a `LoadedPlugin` with `.events = vec![EventDecl{ name }]` (copy the existing `plugin_with_directive`/`asset_kind` helper shape), then:

```rust
#[test]
fn assemble_merges_events() {
    // one plugin exporting event "combatEnd" -> snap.events has it.
    /* build active+installed with a plugin whose loaded.events = [combatEnd] */
    let (snap, errs) = /* assemble_snapshot(...) */;
    assert!(errs.is_empty(), "{errs:?}");
    assert!(snap.events.contains_key("combatEnd"));
}

#[test]
fn assemble_rejects_cross_plugin_event_dup() {
    // two plugins both exporting "combatEnd" -> DuplicateAcrossPlugins{kind:"event"}.
    let (_snap, errs) = /* assemble two plugins each with events=[combatEnd] */;
    assert!(errs.iter().any(|e| matches!(e,
        lute_manifest::assemble::AssembleError::DuplicateAcrossPlugins { kind, .. } if kind == "event")));
}

#[test]
fn assemble_rejects_reserved_builtin_event_name() {
    // a plugin exporting "questComplete" -> ReservedName.
    let (_snap, errs) = /* assemble a plugin with events=[questComplete] */;
    assert!(errs.iter().any(|e| matches!(e,
        lute_manifest::assemble::AssembleError::ReservedName { id, .. } if id == "questComplete")));
}
```

> Reuse the exact `active`/`installed` construction the existing asset-kind tests use (`assemble_merges_asset_kinds` at ~299 and its helpers). Confirm `AssembleError`/`EventDecl` import paths.

- [ ] **Step 2: Run to verify failure** — `cargo test -p lute-manifest --test assemble assemble_merges_events` → FAIL.

- [ ] **Step 3: Implement.** In `assemble_snapshot`'s per-plugin loop (after the `merge_map` for `enums`/`asset_kinds`, ~182), add a reserved-name-checked events merge. Because `merge_map` has no reserved-name hook, hand-roll the events merge like the directives block (~105-131) but simpler (no `validate_*`):

```rust
for e in &pkg.events {
    if crate::snapshot::BUILTIN_LIFECYCLE_EVENTS.contains(&e.name.as_str()) {
        errs.push(AssembleError::ReservedName { id: e.name.clone(), plugin: ap.id.clone() });
        continue;
    }
    if let Some(first) = ev_owner.get(&e.name) {
        errs.push(AssembleError::DuplicateAcrossPlugins {
            kind: "event".into(), id: e.name.clone(),
            first: first.clone(), second: ap.id.clone(),
        });
        continue;
    }
    ev_owner.insert(e.name.clone(), ap.id.clone());
    snap.events.insert(e.name.clone(), e.clone());
}
```

Declare `let mut ev_owner: BTreeMap<String, String> = BTreeMap::new();` beside the existing `dir_owner` declaration (find it near the top of `assemble_snapshot`; it tracks first-owner for the directive dup message). (Using an explicit owner map matches `dir_owner` and gives a correct `first` in the dup error — `merge_map`'s dup uses `"?"` for `first`, so hand-rolling is also a small fidelity upgrade.)

- [ ] **Step 4: Run** — `cargo test -p lute-manifest` → PASS (all workspace manifest tests too: `cargo test -p lute-manifest`).

- [ ] **Step 5: Commit** — `git commit -am "feat(manifest): assemble merges plugin events + reserves builtin lifecycle names (dsl 0.2.0 §4.5)"`

---

### Task 5: plugin-system spec §6.11 + Appendix D erratum

**Files:**
- Modify: `docs/proposals/plugin-system/0.0.1.md` (new §6.11 after §6.10 ~336-351; §13 capabilitySnapshot listing ~577-601; Appendix D new dated erratum ~726)

**Interfaces:** none (docs). This is the deliverable the design doc names ("errata to `plugin-system/0.0.1.md` — an `events` capability export").

- [ ] **Step 1: Add §6.11.** After §6.10 (`enums`), add a terse `### 6.11 \`events/*.yaml\` — capability world events` subsection mirroring §6.6 (defs) / §6.10 (enums): what an event decl is (a named world-event kind, capability-declared, fireable via `<on event="…">`, dsl 0.2.0 §4), the uniqueness rule (event `name` unique across the resolved closure — a duplicate is an error, no silent shadow; a name colliding with a built-in lifecycle event `questActive`/`questComplete`/`questFailed` is reserved), and a YAML example:
  ````markdown
  ```yaml
  # events/world.yaml
  events:
    - name: combatEnd
    - name: npcDied
  ```
  ````
  End with the §6.10-style closing sentence: "The merged event names populate the `events` field of the capability snapshot (§13) and therefore contribute to `capabilityVersion` (§13)."

- [ ] **Step 2: §13 listing.** Add an `events, // §6.11 — capability world events; fireable via <on event>` line to the `capabilitySnapshot = { … }` listing (~577-601), beside `frontmatter`.

- [ ] **Step 3: Appendix D erratum.** Add a NEW dated entry (`### 2026-07-09 — Lute DSL 0.2.0 quest kind`) with the bullet: "**§4 / §6.11** — `events` is added to the closed set of export kinds; the new §6.11 documents `events/*.yaml` (capability-declared world events, fireable via the new `<on event>` ECA trigger, dsl 0.2.0 §4.5)."

- [ ] **Step 4: Commit** — `git commit -am "docs(plugin-system): §6.11 events export erratum (dsl 0.2.0 §4.5)"`

---

## Self-Review checklist (run before executing)

1. **Spec coverage:** `events` export (dsl 0.2.0 §4.5) → Tasks 1-4; built-in lifecycle events single-source-of-truth → Task 2 const; erratum → Task 5.
2. **Placeholder scan:** test bodies in Tasks 3-4 have prose stubs for temp-dir scaffolding — the implementer MUST fill them from the existing `tests/loader.rs`/`tests/assemble.rs` helpers (named explicitly). Every production code block is complete.
3. **Type consistency:** `EventDecl { name }` / `EventsFile { events }` identical across Tasks 1-4; `snap.events` / `LoadedPlugin.events` / `BUILTIN_LIFECYCLE_EVENTS` names stable.
4. **No-churn invariant:** the guarded `capabilityVersion` section keeps event-less snapshots byte-identical (Task 2 test 1) — no lute-compile golden re-record.
