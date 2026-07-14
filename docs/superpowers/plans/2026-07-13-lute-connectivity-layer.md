# Lute Connectivity Layer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a checkable scene↔scene / scene↔quest prerequisite-route layer (`after`) plus a per-node available-state-envelope analysis, an advisory IR emission, and a `lute scenario` command — implementing `docs/superpowers/specs/2026-07-13-lute-connectivity-design.md`.

**Architecture:** A new restricted-CEL `after` prerequisite profile (`visited("key")`/`completed("id")` + `&&`/`||`, no `!`) parses to a `PrereqFormula` AST. A project-wide pass in `lute-check` assembles two derived graphs from those formulas — a flattened topological-precedence DAG (cycles) and a structural formula recursion (reachability + envelope) — surfaced as `E-CONN-*` / `E-STATE-MAYBE-UNAVAILABLE` diagnostics in `check-project`, an advisory graph field in the compiled IR (A-hybrid: engine MAY consult, is not obligated), and a read-only `lute scenario reach`/`envelope` explain command. All analyses are graph-structural — the formula's runtime truth is never evaluated by Lute.

**Tech Stack:** Rust workspace (`crates/lute-check`, `lute-cel`, `lute-cli`, `lute-compile`, `lute-syntax`, `lute-core-span`); `clap` (CLI), `insta` (snapshot goldens), `serde`/`serde_yaml`. Diagnostics = `pub const E_...: &str` constants + `Diagnostic` pushes (no central registry).

## Global Constraints

- **Spec is authoritative:** `docs/superpowers/specs/2026-07-13-lute-connectivity-design.md`. Every task cites its spec section.
- **Enforcement posture = A-hybrid (locked):** the graph is emitted to IR as *advisory* data only (like `relations:`/`rules:`). No normative engine gate. Every §4.2/§4.3 diagnostic message MUST carry the verbatim qualifier **"under your declared routes"** (spec §2.6) — asserted by a lint-level test.
- **Command name = `lute scenario`**; subcommands `reach` / `envelope`. Frontmatter key = `after:` (scene); attribute = `after` (`<quest>` element, sibling to `start`/`fail`).
- **Analyses are graph-structural, never formula evaluation.** No CEL/Datalog evaluation of `after`; `producible()` is a boolean walk over rule *structure*, fully outside the D1 quarantine (spec §2.4, §4.2).
- **Soundness invariant (envelope class only):** `E-STATE-MAYBE-UNAVAILABLE` (§4.3) MUST never newly error a file that single-file `check` reports clean standalone. The §4.1/§4.2 **project-only** diagnostics (`E-CONN-UNKNOWN-NODE`, `E-CONN-CYCLE`, `E-CONN-EPISODE-ID-DUP`, `E-CONN-UNREACHABLE`, relational-objective-liveness) are exempt — they legitimately fire on files clean under single-file `check` (spec §5, §7).
- **Determinism:** all project-wide collections keyed on `BTreeMap`/`BTreeSet` (byte-stable output, IR byte-stability contract). IR `Artifact` fields are append-only in declaration order (spec via `ir.rs:15-50`); a new advisory field is appended **after** `rules`, `#[serde(skip_serializing_if = "Vec::is_empty")]` / `Option`.
- **Per-resolved-project-root scoping:** `E-CONN-EPISODE-ID-DUP` and all project-wide graph assembly group by nearest-ancestor `lute.project.yaml` root (`main.rs:592-597` `by_root`), never a flat pooled walk — else false-positives on the corpus's cross-subproject id reuse.
- **No new escaping rules:** `StringLit` reuses cel-parser's existing string-literal `Expr::Literal` variant; matching is exact string equality on the decoded value.
- **TDD, one commit per task, run only the touched crate's tests.** No formatters/linters, no workspace-wide suite mid-plan (final verification task runs it once). New advisory IR field bumps `LUTE_IR_VERSION` (`lute-compile/src/lib.rs:45`).
- **`insta` snapshots:** review `.snap.new` before `cargo insta accept`; never blind-accept.

---

### Task 1: `after` prerequisite-profile grammar + validator

**Spec:** §2.2 (grammar), §2.5 (no negation). **Files:**
- Create: `crates/lute-check/src/prereq.rs`
- Modify: `crates/lute-check/src/lib.rs` (add `pub mod prereq;` + re-export)
- Test: inline `#[cfg(test)] mod tests` in `prereq.rs`

**Interfaces:**
- Produces:
  - `pub enum PrereqFormula { Visited(String), Completed(String), And(Box<PrereqFormula>, Box<PrereqFormula>), Or(Box<PrereqFormula>, Box<PrereqFormula>) }`
  - `pub const E_CONN_PROFILE: &str = "E-CONN-PROFILE";`
  - `pub fn parse_prereq(raw: &str, span: Span) -> (Option<PrereqFormula>, Vec<Diagnostic>)` — parses the CEL text of an `after` value under the restricted profile; `None` + diagnostics when malformed. Reuses `cel_parser` to produce the base `Expr`, then a narrow admit-walk (NOT `cel_resolve::check_cel_profile`).
  - `pub fn atoms(f: &PrereqFormula) -> Vec<Atom>` where `pub enum Atom { Visited(String), Completed(String) }` — flatten helper for later tasks (edge extraction).

- [ ] **Step 1: Write failing tests** in `crates/lute-check/src/prereq.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use lute_core_span::Span;

    fn parse(s: &str) -> (Option<PrereqFormula>, Vec<String>) {
        let (f, diags) = parse_prereq(s, Span::default());
        (f, diags.into_iter().map(|d| d.code).collect())
    }

    #[test]
    fn and_or_of_visited_completed_ok() {
        let (f, codes) = parse(r#"visited("sofia.ep02") && (completed("q1") || completed("q2"))"#);
        assert!(codes.is_empty(), "unexpected diags: {codes:?}");
        assert!(f.is_some());
    }

    #[test]
    fn negation_rejected() {
        let (_f, codes) = parse(r#"!visited("a")"#);
        assert!(codes.contains(&E_CONN_PROFILE.to_string()));
    }

    #[test]
    fn wrong_arity_rejected() {
        let (_f, codes) = parse(r#"visited("a", "b")"#);
        assert!(codes.contains(&E_CONN_PROFILE.to_string()));
    }

    #[test]
    fn non_string_arg_rejected() {
        let (_f, codes) = parse(r#"visited(42)"#);
        assert!(codes.contains(&E_CONN_PROFILE.to_string()));
    }

    #[test]
    fn bare_string_rejected() {
        let (_f, codes) = parse(r#""x""#);
        assert!(codes.contains(&E_CONN_PROFILE.to_string()));
    }

    #[test]
    fn unknown_call_rejected() {
        let (_f, codes) = parse(r#"holds(a) && visited("x")"#);
        assert!(codes.contains(&E_CONN_PROFILE.to_string()));
    }
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p lute-check prereq::tests`
Expected: FAIL — `prereq` module / `parse_prereq` not found.

- [ ] **Step 3: Implement `parse_prereq`**

Mirror the exact-call-shape discipline of `cel_resolve::is_profile_fact_query` (`cel_resolve.rs:517-528`) — a NEW sibling walk, never extending `check_cel_profile`. Use `cel_parser` to parse `raw` to an `Expr`; recurse:
- `Expr::Call(c)` with `c.target.is_none()`, name ∈ {`visited`,`completed`}, `c.args.len()==1`, and `c.args[0]` a string-literal `Expr::Literal` → `Visited`/`Completed(decoded_string)`.
- `Expr::Binary`/operator node for `&&` / `||` only → `And`/`Or` recursing both operands. (Verify the crate's actual `&&`/`||` AST shape via `cel_parser::ast`; the profile-operator subset here is `{&&, ||}` ONLY — stricter than `is_profile_operator`.)
- Parenthesization: cel-parser produces no separate paren node — grouping is already in the tree structure; nothing to handle.
- Anything else (negation, arithmetic, comparisons, `holds`/`count`, bare literal, wrong arity, non-string arg, unknown call) → push `Diagnostic { code: E_CONN_PROFILE.into(), severity: Severity::Error, message: "…", span, layer: Layer::Cel, .. }` and stop descending that branch (mirror `E_CEL_PROFILE` stop-and-report, `cel_resolve.rs:426-437`).

```rust
use lute_core_span::{Diagnostic, Layer, Severity, Span};

pub const E_CONN_PROFILE: &str = "E-CONN-PROFILE";

pub enum PrereqFormula {
    Visited(String),
    Completed(String),
    And(Box<PrereqFormula>, Box<PrereqFormula>),
    Or(Box<PrereqFormula>, Box<PrereqFormula>),
}

pub enum Atom { Visited(String), Completed(String) }

pub fn parse_prereq(raw: &str, span: Span) -> (Option<PrereqFormula>, Vec<Diagnostic>) {
    let mut diags = Vec::new();
    // 1. cel_parser::parse(raw) -> Expr (map a parse error to E_CONN_PROFILE at `span`).
    // 2. walk(&expr, span, &mut diags) -> Option<PrereqFormula> per the admit-rules above.
    // Return (formula, diags); formula is None if any diag was pushed.
    todo!("implement admit-walk per Step 3 notes")
}

pub fn atoms(f: &PrereqFormula) -> Vec<Atom> { /* recursive flatten */ todo!() }
```

Add `pub mod prereq;` to `crates/lute-check/src/lib.rs`.

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p lute-check prereq::tests`
Expected: PASS (6 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/lute-check/src/prereq.rs crates/lute-check/src/lib.rs
git commit -m "feat(check): restricted after prerequisite-profile grammar + PrereqFormula (connectivity T1)"
```

---

### Task 2: `after` surface — scene frontmatter key + quest attribute

**Spec:** §2.1 (placement asymmetric by design). **Files:**
- Modify: `crates/lute-check/src/meta.rs` (`TypedMeta`: add `pub after: Option<String>`, populated from raw frontmatter `after:`)
- Modify: quest declaration parse (`<quest>` element attrs) to accept an `after` attribute sibling to `start`/`fail` — locate the quest-decl attribute reader (grep `start` handling near the `<quest>` parse; likely `crates/lute-syntax` quest parsing or `crates/lute-check/src/meta.rs` quest lift). Add `pub after: Option<String>` to the quest decl struct.
- Test: `crates/lute-check/tests/connectivity.rs` (new)

**Interfaces:**
- Consumes: T1 `parse_prereq` (validate the surfaced text in `check`).
- Produces: `TypedMeta.after: Option<String>` (raw CEL text) and `<quest>` decl `.after: Option<String>`.

- [ ] **Step 1: Write failing tests** — `crates/lute-check/tests/connectivity.rs`

```rust
use lute_check::{check, input_for}; // match the existing tests/examples.rs import surface

#[test]
fn scene_after_key_is_parsed_and_validated() {
    let text = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nafter: 'visited(\"y.s01ep01\")'\n---\n## Shot 1.\n@a: hi\n";
    let res = check(&input_for(text));
    // A well-formed after against an (unknown, single-file) node must NOT raise E-CONN-PROFILE.
    assert!(!res.diagnostics.iter().any(|d| d.code == "E-CONN-PROFILE"));
}

#[test]
fn scene_after_malformed_raises_profile_error() {
    let text = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nafter: '!visited(\"y\")'\n---\n## Shot 1.\n@a: hi\n";
    let res = check(&input_for(text));
    assert!(res.diagnostics.iter().any(|d| d.code == "E-CONN-PROFILE"));
}
```

- [ ] **Step 2: Run to verify fail**

Run: `cargo test -p lute-check --test connectivity`
Expected: FAIL — `after` not surfaced / not validated.

- [ ] **Step 3: Surface + validate**

- In `meta.rs`, add `after: Option<String>` to `TypedMeta`; populate from the raw YAML mapping (same ad-hoc `serde_yaml` mapping lookup pattern `artifact_meta` uses for `episodeId`, per anchor §5).
- In the quest-decl parse, read an optional `after` attribute alongside `start`/`fail`.
- In `crates/lute-check/src/check.rs`, in the single-file check flow, when a scene/quest carries `after`, call `prereq::parse_prereq(text, span)` and extend diagnostics with its result (this is the §5 "`check` MAY validate local syntax" behavior — grammar only, no node resolution).

- [ ] **Step 4: Run to verify pass**

Run: `cargo test -p lute-check --test connectivity`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/lute-check/src/meta.rs crates/lute-check/src/check.rs crates/lute-check/tests/connectivity.rs crates/lute-syntax/
git commit -m "feat(check): surface + locally validate scene after: and quest after attribute (connectivity T2)"
```

---

### Task 3: Canonical episode key + project key set + `E-CONN-EPISODE-ID-DUP`

**Spec:** §2.3 (canonical key, exact lookup), §4.1 (§A dup). **Files:**
- Modify: `crates/lute-check/src/meta.rs` — extract `pub fn canonical_episode_key(character: &str, season: i64, episode: i64, episode_id: Option<&str>) -> String` (the `{character}.{episodeId}` derivation; `episodeId` = authored non-empty value else `s{season:02}ep{episode:02}`, per `lute-compile/src/lib.rs:329-332`).
- Modify: `crates/lute-compile/src/lib.rs` — refactor `artifact_meta`'s inline `episode_id` derivation to call the new shared helper (DRY; lute-compile already depends on lute-check).
- Create: `crates/lute-check/src/connectivity.rs` (`pub mod connectivity;` in lib.rs) — project-wide graph module; this task adds key-set assembly + dup check.
- Modify: `crates/lute-cli/src/main.rs` — wire `check_conn_episode_dup` into the `by_root` loop (§4.1) next to `check_project_quest_ids`.
- Test: inline tests in `connectivity.rs` + a `crates/lute-check/src/project_check.rs`-style fixture.

**Interfaces:**
- Produces:
  - `meta::canonical_episode_key(...) -> String`
  - `pub const E_CONN_EPISODE_ID_DUP: &str = "E-CONN-EPISODE-ID-DUP";`
  - `pub fn scene_key_set(docs: &[(PathBuf, Document)]) -> BTreeMap<String, Vec<(PathBuf, Span)>>` — group scene docs by computed canonical key.
  - `pub fn check_conn_episode_dup(docs: &[(PathBuf, Document)]) -> Vec<(PathBuf, Diagnostic)>` — flag any key group with ≥2 members (parallel to `check_project_quest_ids`, `project_check.rs:97`).

- [ ] **Step 1: Write failing tests** — inline in `connectivity.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;
    // reuse project_check.rs-style doc(...) fixture helpers (copy the minimal builder)

    #[test]
    fn identical_pair_in_same_root_is_dup() {
        let docs = /* two scene docs, same character+season+episode, same root */;
        let out = check_conn_episode_dup(&docs);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].1.code, "E-CONN-EPISODE-ID-DUP");
    }

    #[test]
    fn distinct_keys_do_not_collide() {
        let docs = /* two scene docs, different episode */;
        assert!(check_conn_episode_dup(&docs).is_empty());
    }

    #[test]
    fn cross_pair_join_collision_is_caught() {
        // character="a", episodeId="b.c"  vs  character="a.b", episodeId="c"  → same "a.b.c"
        let docs = /* two docs whose canonical keys collide via embedded '.' */;
        assert_eq!(check_conn_episode_dup(&docs).len(), 1);
    }
}
```

- [ ] **Step 2: Run to verify fail**

Run: `cargo test -p lute-check connectivity::tests`
Expected: FAIL — module/functions absent.

- [ ] **Step 3: Implement key helper + dup check**

- `meta::canonical_episode_key` extracted from `lute-compile/src/lib.rs:329-332`; refactor `artifact_meta` to call it (keep the address.rs join semantics identical).
- `scene_key_set`: for each scene `Document`, read `character`/`season`/`episode` (`TypedMeta`) + `episodeId` (raw YAML mapping), compute key, push `(path, span)` into a `BTreeMap<String, Vec<_>>`.
- `check_conn_episode_dup`: for each group with `len() >= 2`, emit `E_CONN_EPISODE_ID_DUP` on the 2nd+ occurrences (mirror `check_project_quest_ids`'s skip-first shape).

- [ ] **Step 4: Wire into `check-project`** (`crates/lute-cli/src/main.rs`, the `by_root` loop ~:606)

Inside `for group in by_root.values() { … }`, add `project_diags.extend(lute_check::connectivity::check_conn_episode_dup(group));`.

- [ ] **Step 5: Run to verify pass**

Run: `cargo test -p lute-check connectivity::tests && cargo test -p lute-compile --lib`
Expected: PASS (compile tests still green after the DRY refactor; 3 new connectivity tests pass).

- [ ] **Step 6: Commit**

```bash
git add crates/lute-check/src/meta.rs crates/lute-check/src/connectivity.rs crates/lute-check/src/lib.rs crates/lute-compile/src/lib.rs crates/lute-cli/src/main.rs
git commit -m "feat(check): canonical episode key + E-CONN-EPISODE-ID-DUP, wired into check-project (connectivity T3)"
```

---

### Task 4: Node resolution — `E-CONN-UNKNOWN-NODE`

**Spec:** §2.3, §4.1 (§A unknown-node). **Files:**
- Modify: `crates/lute-check/src/connectivity.rs`
- Modify: `crates/lute-cli/src/main.rs` (extend the project pass)
- Test: `crates/lute-check/tests/connectivity.rs`

**Interfaces:**
- Consumes: T1 `PrereqFormula`/`atoms`, T3 `scene_key_set`; quest-id set (reuse `project_check::defined_quests` if `pub`, else assemble from `<quest id>`).
- Produces:
  - `pub const E_CONN_UNKNOWN_NODE: &str = "E-CONN-UNKNOWN-NODE";`
  - `pub fn resolve_nodes(docs, key_set, quest_ids) -> Vec<(PathBuf, Diagnostic)>` — for every `after` atom, exact-lookup `visited(K)` against `key_set`, `completed(Q)` against `quest_ids`; unknown → `E-CONN-UNKNOWN-NODE` with nearest-match suggestion (mirror `E-COMPONENT-UNDECLARED`).

- [ ] **Step 1: Write failing test**

```rust
#[test]
fn unknown_visited_key_is_flagged() {
    // scene A declares after: visited("nope.s99ep99"); no such episode exists
    let res = check_project_fixture(/* … */);
    assert!(res.iter().any(|(_p, d)| d.code == "E-CONN-UNKNOWN-NODE"));
}

#[test]
fn known_visited_key_resolves_clean() {
    // scene A declares after: visited(<B's real canonical key>); B exists
    let res = check_project_fixture(/* … */);
    assert!(!res.iter().any(|(_p, d)| d.code == "E-CONN-UNKNOWN-NODE"));
}
```

- [ ] **Step 2: Run to verify fail** — `cargo test -p lute-check --test connectivity` → FAIL.

- [ ] **Step 3: Implement `resolve_nodes`** — parse each doc's `after` via `prereq::parse_prereq`, flatten via `atoms`, test each atom's string against the exact key/quest-id set; on miss push `E_CONN_UNKNOWN_NODE` (nearest-match suggestion via the existing edit-distance helper used by `E-COMPONENT-UNDECLARED`).

- [ ] **Step 4: Wire into `check-project`** — add `project_diags.extend(connectivity::resolve_nodes(group, &key_set, &quest_ids));` in the `by_root` loop.

- [ ] **Step 5: Run to verify pass** — `cargo test -p lute-check --test connectivity` → PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/lute-check/src/connectivity.rs crates/lute-cli/src/main.rs crates/lute-check/tests/connectivity.rs
git commit -m "feat(check): E-CONN-UNKNOWN-NODE exact-lookup node resolution (connectivity T4)"
```

---

### Task 5: Topological-precedence DAG + `E-CONN-CYCLE`

**Spec:** §2.4 (graph 1), §4.1 (§A cycle). **Files:**
- Modify: `crates/lute-check/src/connectivity.rs`
- Test: inline + `tests/connectivity.rs`

**Interfaces:**
- Consumes: T3 key set, T4 resolution, `atoms`.
- Produces:
  - `pub struct ConnGraph { pub nodes: BTreeMap<String, NodeInfo>, pub edges: BTreeMap<String, BTreeSet<String>>, pub topo_order: Vec<String> }` where `NodeInfo { key: String, path: PathBuf, formula: Option<PrereqFormula>, span: Span }`.
  - `pub const E_CONN_CYCLE: &str = "E-CONN-CYCLE";`
  - `pub fn assemble_graph(docs, key_set, quest_ids) -> (ConnGraph, Vec<(PathBuf, Diagnostic)>)` — flattened edge `p → n` for every atom `p` in `n`'s formula; DFS 3-coloring cycle detection cloned from `schema_import::detect_cycles`/`dfs_cycle` (`schema_import.rs:784-833`), chain-printed; `topo_order` from the acyclic DAG (`topo_order` empty/undefined if a cycle exists — cycle diag already emitted).

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn two_node_cycle_is_flagged() {
    // A after visited(Bkey); B after visited(Akey)
    let (_g, diags) = assemble_graph_fixture(/* … */);
    assert!(diags.iter().any(|(_p, d)| d.code == "E-CONN-CYCLE"));
}
#[test]
fn acyclic_graph_has_no_cycle_and_topo_order() {
    let (g, diags) = assemble_graph_fixture(/* A entry, B after A, C after B */);
    assert!(diags.is_empty());
    assert_eq!(g.topo_order.len(), 3);
}
```

- [ ] **Step 2: Run to verify fail** — FAIL.

- [ ] **Step 3: Implement `assemble_graph`** — build `nodes` from every scene (+ `after`-opted-in quest, T12 reuses this), `edges` = flattened union of atoms per formula (over-approx, ignoring `&&`/`||` position), DFS 3-coloring for `E-CONN-CYCLE` (clone `dfs_cycle`), Kahn/DFS topo sort for `topo_order`.

- [ ] **Step 4: Run to verify pass** — `cargo test -p lute-check connectivity` → PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/lute-check/src/connectivity.rs crates/lute-check/tests/connectivity.rs
git commit -m "feat(check): topological-precedence DAG + E-CONN-CYCLE (connectivity T5)"
```

---

### Task 6: `E-CONN-UNREACHABLE` + `E-CONN-FORMULA-TOO-COMPLEX`

**Spec:** §2.4 (graph 2), §4.1 (§A reachability). **Files:**
- Modify: `crates/lute-check/src/connectivity.rs`
- Test: inline + `tests/connectivity.rs`

**Interfaces:**
- Consumes: `ConnGraph`, `PrereqFormula`; quest reachability signal (`E-QUEST-UNREACHABLE`, 0.4.0 §5.3) — treat a `completed(Q)` for an unreachable quest as `reachable = false` (stub `true` for now if quest-reachability data isn't threaded; note the TODO for T7 integration).
- Produces:
  - `pub const E_CONN_UNREACHABLE: &str = "E-CONN-UNREACHABLE";`
  - `pub const E_CONN_FORMULA_TOO_COMPLEX: &str = "E-CONN-FORMULA-TOO-COMPLEX";`
  - `pub fn check_reachability(g: &ConnGraph) -> (BTreeMap<String, bool>, Vec<(PathBuf, Diagnostic)>)` — memoized structural recursion over each node's formula in `topo_order`: `reachable(visited(Y)) = reach[Y]`, `reachable(completed(Q)) = quest_reachable(Q)`, `And = ∧`, `Or = ∨`; absent/empty `after` ⇒ `true` (entry). A per-formula atom-count cap emits `E-CONN-FORMULA-TOO-COMPLEX`.

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn entry_node_is_reachable() {
    let (reach, _d) = check_reachability(&graph_fixture(/* A: no after */));
    assert_eq!(reach["a.s01ep01"], true);
}
#[test]
fn node_behind_unreachable_prereq_is_unreachable() {
    // B after visited(A); A itself unreachable (A after visited(<nonexistent-but-declared cycle-free dead node>))
    let (_reach, diags) = check_reachability_fixture(/* … */);
    assert!(diags.iter().any(|(_p, d)| d.code == "E-CONN-UNREACHABLE"));
}
#[test]
fn oversized_formula_is_capped() {
    let (_reach, diags) = check_reachability_fixture(/* formula with atom count > cap */);
    assert!(diags.iter().any(|(_p, d)| d.code == "E-CONN-FORMULA-TOO-COMPLEX"));
}
```

- [ ] **Step 2: Run to verify fail** — FAIL.

- [ ] **Step 3: Implement `check_reachability`** — process nodes in `topo_order`, memoize `reach[node]`, structural recursion per Interfaces; message MUST carry "under your declared routes" (Global Constraints). Atom-count cap constant (e.g. 256) → `E_CONN_FORMULA_TOO_COMPLEX`.

- [ ] **Step 4: Wire into `check-project`** — call `assemble_graph` then `check_reachability`, extend `project_diags`. (Envelope/producibility wire in T7/T11.)

- [ ] **Step 5: Run to verify pass** — `cargo test -p lute-check connectivity` → PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/lute-check/src/connectivity.rs crates/lute-cli/src/main.rs crates/lute-check/tests/connectivity.rs
git commit -m "feat(check): E-CONN-UNREACHABLE structural reachability + formula cap (connectivity T6)"
```

---

### Task 7: `producible()` walk + relational-objective liveness

**Spec:** §4.2 (§B). **Files:**
- Create: `crates/lute-check/src/producible.rs` (`pub mod producible;`)
- Modify: `crates/lute-check/src/connectivity.rs` (feed reachable-node set into the assert-site base case) / `crates/lute-cli/src/main.rs`
- Test: `crates/lute-check/tests/connectivity.rs` (+ the `quest-rescue-halsin.lute` corpus regression)

**Interfaces:**
- Consumes: `RelVocab` (`rel_schema.rs:34-44`: `.relations` with `.derive`/`.reserved`, `.facts`, `.rules`), `datalog_check::predicate_edges` shape (`datalog_check.rs:550-582`), T6 reachable-node set.
- Produces:
  - `pub fn producible(vocab: &RelVocab, reachable_assert_nodes: &BTreeSet<String>) -> BTreeMap<String, bool>` — monotone least-fixpoint over the rule DAG: base `R` producible iff `facts:` seed OR `R.reserved` OR an `::assert{R(…)}` in a reachable node; derived `R` producible iff any clause has all positive atoms producible; `BodyLiteral::Neg` + `BodyLiteral::Guard{cel}` treated always-satisfiable (never a false-positive dead claim).
  - relational-objective-liveness: an `<objective done>` gated on `holds`/`count`/`validAt` over a non-producible `R` rides `E-OBJECTIVE-UNSATISFIABLE`/`E-QUEST-UNREACHABLE` as a third named cause.

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn derived_relation_seeded_via_facts_is_producible_no_false_positive() {
    // reproduce docs/examples/quest-rescue-halsin.lute + act1.schema.yaml:
    // done="holds(canReach(player,grove))", canReach derive:true from atLocation/connected (facts-seeded)
    let res = check_project_on_corpus(&["docs/examples/quest-rescue-halsin.lute"]);
    assert!(!res.iter().any(|(_p, d)| d.code == "E-OBJECTIVE-UNSATISFIABLE"));
}
#[test]
fn objective_on_never_producible_relation_is_dead() {
    // relation R: derived, its only rule body needs a base relation with no facts seed / no assert / not reserved
    let res = check_project_fixture(/* … */);
    assert!(res.iter().any(|(_p, d)| d.code == "E-OBJECTIVE-UNSATISFIABLE" || d.code == "E-QUEST-UNREACHABLE"));
}
```

- [ ] **Step 2: Run to verify fail** — FAIL.

- [ ] **Step 3: Implement `producible`** — boolean least-fixpoint iterating to a fixed point over `vocab.rules`/`vocab.relations`/`vocab.facts`; then scan objective `done` guards (fact-query calls) and flag when the gated relation is non-producible, worded "under your declared routes".

- [ ] **Step 4: Wire into `check-project`** — after reachability, compute `producible`, run the liveness scan, extend diags.

- [ ] **Step 5: Run to verify pass**

Run: `cargo test -p lute-check --test connectivity`
Expected: PASS — including the halsin corpus case NOT false-flagged.

- [ ] **Step 6: Commit**

```bash
git add crates/lute-check/src/producible.rs crates/lute-check/src/connectivity.rs crates/lute-check/src/lib.rs crates/lute-cli/src/main.rs crates/lute-check/tests/connectivity.rs
git commit -m "feat(check): producible() rule-dep walk + relational-objective liveness (connectivity T7)"
```

---

### Task 8: Expose `G`/`P` per document from defassign

**Spec:** §4.3 (§C effect summary). **Files:**
- Modify: `crates/lute-check/src/defassign.rs` (`check_definite_assignment` returns the final `Assigned`; add a flat `possible_writes` scan)
- Modify: `crates/lute-check/src/check.rs:706-738` (capture the returned set)
- Create: `crates/lute-check/src/envelope.rs` (`pub mod envelope;`) — the run.*/user.* filter + G/P types
- Test: inline in `defassign.rs` + `envelope.rs`

**Interfaces:**
- Consumes: `defassign::{Assigned, intersect_all, walk_nodes}`, `meta::namespace_of` (tier classification).
- Produces:
  - Changed: `pub fn check_definite_assignment(nodes, schema, ctx) -> (Vec<Diagnostic>, Assigned)` (was `-> Vec<Diagnostic>`).
  - `envelope::G(assigned: &Assigned) -> BTreeSet<String>` filtered to `Namespace::{Run,User}`.
  - `pub fn possible_writes(nodes: &[Node]) -> BTreeSet<String>` — flat path-insensitive scan of every `::set`/persist-sugar target (NOT `::assert`), filtered to run.*/user.*.

- [ ] **Step 1: Write failing tests** (inline `defassign.rs`)

```rust
#[test]
fn definite_assignment_returns_final_assigned_set() {
    let (nodes, schema) = fixture("::set{run.x = 1}"); // existing fixture helper
    let (errs, assigned) = check_definite_assignment(&nodes, &schema, &ctx());
    assert!(errs.is_empty());
    assert!(assigned.contains("run.x"));
}
#[test]
fn possible_writes_collects_all_set_targets_run_user_only() {
    let nodes = fixture_nodes("<branch><match>::set{run.a=1}</match><match>::set{run.b=2}</match></branch>");
    let p = super::envelope::possible_writes(&nodes);
    assert!(p.contains("run.a") && p.contains("run.b"));
}
```

- [ ] **Step 2: Run to verify fail** — FAIL (arity change + missing fn).

- [ ] **Step 3: Implement** — change `check_definite_assignment` to return `(diags, assigned)`; update **every** caller (`lsp` reference: run `cargo build -p lute-check` and fix all callsites — `check.rs:706-738` and any others `lsp references` finds). Add `possible_writes` (walk all `Node::Set`/persist-sugar, collect targets, filter tier).

- [ ] **Step 4: Run to verify pass** — `cargo test -p lute-check defassign::tests envelope::tests` → PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/lute-check/src/defassign.rs crates/lute-check/src/check.rs crates/lute-check/src/envelope.rs crates/lute-check/src/lib.rs
git commit -m "feat(check): return final Assigned + possible_writes scan for envelope (connectivity T8)"
```

---

### Task 9: `writesOnComplete(Q)`

**Spec:** §4.3 (quest completion writes). **Files:**
- Modify: `crates/lute-check/src/envelope.rs`
- Test: inline in `envelope.rs`

**Interfaces:**
- Consumes: quest decl (required vs optional objectives, `questComplete` `<on>` body), `defassign::intersect_all`, T8 per-body Assigned.
- Produces: `pub fn writes_on_complete(q: &QuestDecl, schema: &StateSchema) -> BTreeSet<String>` — union across (each required objective body + `questComplete` `<on>` body) of each body's own `intersect_all` guaranteed set; optional objectives excluded; filtered to run.*/user.*.

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn writes_on_complete_intersects_within_body_unions_across() {
    // objective1 body: <branch> both arms ::set{run.done=1} → run.done guaranteed
    // questComplete <on>: ::set{run.flag=1}
    let q = quest_fixture(/* … */);
    let w = writes_on_complete(&q, &schema());
    assert!(w.contains("run.done") && w.contains("run.flag"));
}
#[test]
fn optional_objective_writes_excluded() {
    let q = quest_fixture(/* optional objective sets run.opt */);
    assert!(!writes_on_complete(&q, &schema()).contains("run.opt"));
}
```

- [ ] **Step 2: Run to verify fail** — FAIL.

- [ ] **Step 3: Implement** — for each required objective body + the `questComplete` `<on>` handler body, run the same forward `walk_nodes`+`intersect_all` pass on that body's node stream alone (a small variant that returns the body's guaranteed set, per anchor §2), then union across bodies; filter tier.

- [ ] **Step 4: Run to verify pass** — `cargo test -p lute-check envelope::tests` → PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/lute-check/src/envelope.rs
git commit -m "feat(check): writesOnComplete(Q) quest-completion guaranteed writes (connectivity T9)"
```

---

### Task 10: Envelope propagation (Guaranteed/Possible per node)

**Spec:** §4.3 (canonical propagation, algebraic identity). **Files:**
- Modify: `crates/lute-check/src/envelope.rs`
- Test: inline + a property-based identity test

**Interfaces:**
- Consumes: `ConnGraph` (topo order), `PrereqFormula`, T8 `G`/`possible_writes` per node, T9 `writes_on_complete`, entry base `D` (= `has_default`-derived schema-default run.*/user.* set, `defassign.rs:~475`).
- Produces:
  - `pub struct Env { pub guaranteed: BTreeSet<String>, pub possible: BTreeSet<String> }`
  - `pub fn propagate(g: &ConnGraph, per_doc: &PerDocEffects, d: &BTreeSet<String>) -> BTreeMap<String, Env>` — memoized structural recursion over each node's formula in topo order: `visited(Y): G=Guar(Y)∪G(Y), P=Poss(Y)∪P(Y)`; `completed(Q): G=P=writesOnComplete(Q)`; `X&&Y: G=∪,P=∪`; `X||Y: G=∩,P=∪`; entry: `G=P=D`.

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn or_intersects_guaranteed_union_possible() {
    // node N after (visited(A) || visited(B)); A guarantees {run.a, run.x}, B guarantees {run.b, run.x}
    let envs = propagate_fixture(/* … */);
    assert!(envs["n"].guaranteed.contains("run.x"));  // in both arms
    assert!(!envs["n"].guaranteed.contains("run.a")); // only one arm
    assert!(envs["n"].possible.contains("run.a") && envs["n"].possible.contains("run.b"));
}
#[test]
fn structural_recursion_equals_bruteforce_per_route() {
    // property-based: generate random && / || formula ASTs over small labeled atom sets,
    // compare propagate() Guaranteed/Possible against brute-force per-route ∩/∪ enumeration.
    for _ in 0..500 {
        let (g, per_doc, d) = random_small_graph();
        let via_struct = propagate(&g, &per_doc, &d);
        let via_routes = bruteforce_per_route(&g, &per_doc, &d);
        assert_eq!(via_struct, via_routes);
    }
}
#[test]
fn default_set_survives_every_node() {
    // D = {run.def}; every node's Guaranteed contains run.def
    let envs = propagate_fixture(/* … */);
    assert!(envs.values().all(|e| e.guaranteed.contains("run.def")));
}
```

- [ ] **Step 2: Run to verify fail** — FAIL.

- [ ] **Step 3: Implement `propagate`** + a test-only `bruteforce_per_route` (enumerate DNF routes on small graphs, `∩` guaranteed within a route's set / `∪` across routes) to validate the identity. Entry base `D` from the schema-default set (reuse `has_default`).

- [ ] **Step 4: Run to verify pass** — `cargo test -p lute-check envelope::tests` → PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/lute-check/src/envelope.rs
git commit -m "feat(check): envelope propagation (Guaranteed/Possible) + per-route identity test (connectivity T10)"
```

---

### Task 11: `E-STATE-MAYBE-UNAVAILABLE` diagnostic

**Spec:** §4.3 (diagnostic use), §2.6 (wording), §7 (soundness invariant — envelope class). **Files:**
- Modify: `crates/lute-check/src/envelope.rs` + `crates/lute-cli/src/main.rs`
- Test: `crates/lute-check/tests/connectivity.rs`

**Interfaces:**
- Consumes: T10 `propagate` result, each node's actual state reads (from the existing read-collection in defassign/check — reuse the read sites `check_definite_assignment` already visits).
- Produces:
  - `pub const E_STATE_MAYBE_UNAVAILABLE: &str = "E-STATE-MAYBE-UNAVAILABLE";`
  - `pub fn check_envelope(g, envs, reads_per_node) -> Vec<(PathBuf, Diagnostic)>` — read `P` at node X: `P ∉ Possible(X)` ⇒ error `E-STATE-MAYBE-UNAVAILABLE` (default); `P ∈ Possible\Guaranteed` ⇒ warning, default-suppressed (surfaced only via `lute scenario envelope`, T14). Message carries "under your declared routes".

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn read_never_set_on_any_route_errors() {
    // node reads run.z; no predecessor route ever sets run.z
    let res = check_project_fixture(/* … */);
    let d = res.iter().find(|(_p, d)| d.code == "E-STATE-MAYBE-UNAVAILABLE").expect("expected error");
    assert!(d.1.message.contains("under your declared routes"));
}
#[test]
fn read_set_on_all_routes_is_clean() {
    let res = check_project_fixture(/* run.z guaranteed */);
    assert!(!res.iter().any(|(_p, d)| d.code == "E-STATE-MAYBE-UNAVAILABLE"));
}
#[test]
fn standalone_clean_file_not_newly_errored() {
    // a scene that check() reports clean standalone must not gain E-STATE-MAYBE-UNAVAILABLE at project scope
    let single = check(&input_for(SCENE_CLEAN));
    assert!(single.diagnostics.iter().all(|d| d.severity != Severity::Error));
    let proj = check_project_fixture(&[SCENE_CLEAN]);
    assert!(!proj.iter().any(|(_p, d)| d.code == "E-STATE-MAYBE-UNAVAILABLE"));
}
```

- [ ] **Step 2: Run to verify fail** — FAIL.

- [ ] **Step 3: Implement `check_envelope`** — for each node's reads, classify against its `Env`; emit error/warning per Interfaces. Gate the warning behind a flag defaulting off in `check-project`.

- [ ] **Step 4: Wire into `check-project`** — after `propagate`, run `check_envelope`, extend `project_diags` (errors only by default).

- [ ] **Step 5: Run to verify pass** — `cargo test -p lute-check --test connectivity` → PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/lute-check/src/envelope.rs crates/lute-cli/src/main.rs crates/lute-check/tests/connectivity.rs
git commit -m "feat(check): E-STATE-MAYBE-UNAVAILABLE envelope diagnostic (connectivity T11)"
```

---

### Task 12: Quest-time availability inventory

**Spec:** §4.4 (§D). **Files:**
- Modify: `crates/lute-check/src/envelope.rs` (quest-node envelope entry)
- Test: inline in `envelope.rs`

**Interfaces:**
- Consumes: T10 `propagate`, entry base `D`; quest `after` (T2) when present.
- Produces: `pub fn quest_envelope(q: &QuestDecl, g: &ConnGraph, envs: &BTreeMap<String, Env>, d: &BTreeSet<String>) -> QuestEnv` where a quest **with** `after` reuses the full scene envelope tables; a quest **without** `after` returns `Env { guaranteed: D.clone(), possible: D.clone() }` + an enrichment note flag (never empty, never error). The diagnostic side (`check_quest_guard_defassign`, `defassign.rs:95`) is unchanged — verify it still runs; add no new quest-guard diagnostic.

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn quest_without_after_gets_defaults_only_non_empty() {
    let q = quest_fixture(/* no after */);
    let qe = quest_envelope(&q, &g, &envs, &d_set(["run.def"]));
    assert_eq!(qe.env.guaranteed, qe.env.possible);
    assert!(qe.env.guaranteed.contains("run.def"));
    assert!(qe.enrichment_note); // "declaring after would enrich"
}
#[test]
fn quest_with_after_gets_full_tables() {
    let q = quest_fixture(/* after: visited(A) that guarantees run.a */);
    let qe = quest_envelope(&q, &g, &envs, &d_set([]));
    assert!(qe.env.guaranteed.contains("run.a"));
    assert!(!qe.enrichment_note);
}
```

- [ ] **Step 2: Run to verify fail** — FAIL.

- [ ] **Step 3: Implement `quest_envelope`** per Interfaces. Register `after`-opted-in quests as nodes in `assemble_graph` (T5) so `propagate` already covers them.

- [ ] **Step 4: Run to verify pass** — `cargo test -p lute-check envelope::tests` → PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/lute-check/src/envelope.rs crates/lute-check/src/connectivity.rs
git commit -m "feat(check): quest-time envelope inventory (defaults-only + opt-in after) (connectivity T12)"
```

---

### Task 13: Advisory IR emission

**Spec:** §2.6 (A-hybrid). **Files:**
- Modify: `crates/lute-compile/src/ir.rs` (new `Artifact` field), `crates/lute-compile/src/lib.rs` (lowering + `LUTE_IR_VERSION` bump)
- Test: inline in `lib.rs` + accept the e2e golden re-snapshot

**Interfaces:**
- Consumes: this document's own declared `after` only. **Boundary (explicit):** `compile` is single-document and has no resolved project root, so each `Artifact` emits ONLY its own nodes' raw declared `after` formulas — never a project-assembled graph. The engine reconstructs the whole graph by unioning `prereqEdges` across every document's artifact, exactly as it already unions per-document `relations`/`rules`. Project-wide resolution, cycle/reachability/envelope, and node-existence checks stay entirely in `check-project`/`lute scenario` (T3–T14); the IR carries no resolved or validated graph, only raw local declarations.
- Produces: `pub prereq_edges: Vec<PrereqEdgeEntry>` on `Artifact`, appended **after `commands`** — the current LAST field (`ir.rs:48`, `lib.rs:154-167`). Field declaration order IS the byte-stability contract (`ir.rs:1-3`), so this is a true append-only change and `commands` MUST NOT move. `#[serde(skip_serializing_if = "Vec::is_empty")]`, serialized name-sorted (byte-stable). `pub struct PrereqEdgeEntry { pub node: String, pub after: String }` — `node` = the contributing node's canonical key (a scene's `{character}.{episodeId}` via `meta::canonical_episode_key`, or a quest's `<quest id>`); `after` = the raw declared formula text (unresolved, unvalidated). A scene doc contributes at most one entry (its `after:`); a quest-pack doc contributes one entry per `<quest>` that declares an `after` attribute (≥0 entries).

- [ ] **Step 1: Write failing test** (inline `lib.rs`, mirror `artifact_emits_relational_schema` at `lib.rs:583`)

```rust
#[test]
fn artifact_emits_prereq_edges_when_after_declared() {
    let input = raw_scene_with("after: 'visited(\"y.s01ep01\")'"); // this scene's key = x.s01ep01
    let art = compile(&input).unwrap();
    let v = serde_json::to_value(&art).unwrap();
    assert_eq!(v["prereqEdges"][0]["node"], serde_json::json!("x.s01ep01"));
    assert_eq!(v["prereqEdges"][0]["after"], serde_json::json!("visited(\"y.s01ep01\")"));
}
#[test]
fn quest_pack_emits_one_edge_per_after_quest() {
    // a quest doc with two <quest> decls, only one carrying an `after` attribute
    let art = compile(&raw_quest_pack_with_one_after()).unwrap();
    let v = serde_json::to_value(&art).unwrap();
    assert_eq!(v["prereqEdges"].as_array().unwrap().len(), 1);
}
#[test]
fn artifact_omits_prereq_edges_when_absent() {
    let art = compile(&raw_scene_no_after()).unwrap();
    let v = serde_json::to_value(&art).unwrap();
    assert!(v.get("prereqEdges").is_none()); // skip_serializing_if = Vec::is_empty
}
```

- [ ] **Step 2: Run to verify fail** — `cargo test -p lute-compile --lib edge` → FAIL.

- [ ] **Step 3: Implement** — add `prereq_edges: Vec<PrereqEdgeEntry>` as the LAST field (after `commands`) in BOTH the `ir.rs` `Artifact` struct declaration and the `Artifact { … }` construction (`lib.rs:154-167`) — append-only, `commands` and all prior fields unmoved. Add a `prereq_edge_entries(doc, folded) -> Vec<PrereqEdgeEntry>` lowering (parallel to `rel_entries`, `lib.rs:178`; collect this doc's scene `after:` and/or each `<quest after>`, `node` via `meta::canonical_episode_key`/`<quest id>`, `after` = raw text, sorted by `node`), then bump `LUTE_IR_VERSION` (`lib.rs:45`) to the next version. Raw declarations only — no resolution/validation in `compile`.

- [ ] **Step 4: Re-accept e2e goldens** (version line + any newly-emitted `prereqEdges` field only)

Run: `cargo test -p lute-compile --test e2e` (fails on snapshot drift) → review `.snap.new` diffs (MUST be only the `irVersion`/`lute` line + a trailing `prereqEdges` field for after-declaring fixtures; `commands` and all prior fields byte-unchanged) → `cargo insta accept`.

- [ ] **Step 5: Run to verify pass** — `cargo test -p lute-compile` → PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/lute-compile/src/ir.rs crates/lute-compile/src/lib.rs crates/lute-compile/tests/snapshots/
git commit -m "feat(compile): advisory connectivity graph in IR + version bump (connectivity T13)"
```

---

### Task 14: `lute scenario` command + `check-project` wiring

**Spec:** §5 (tooling), §6 (diagnostics table). **Files:**
- Modify: `crates/lute-cli/src/main.rs` (`Scenario(ScenarioCommand)` + `run_scenario`)
- Create: `crates/lute-cli/tests/scenario.rs` (integration)

**Interfaces:**
- Consumes: all of `connectivity`/`envelope` (graph, reachability, envelopes, quest envelopes).
- Produces: `#[derive(Subcommand)] enum ScenarioCommand { Reach { node_id: String }, Envelope { node_id: String } }` (mirror `Catalog(CatalogCommand)`, `main.rs:221-233`); `lute scenario` (bare) prints the topological graph; `reach <nodeId>` prints reachability + route(s); `envelope <nodeId>` / `envelope quest:<id>` prints Guaranteed/Possible (full for scene/after-quest, defaults-only + note for bare quest, T12). Warning-grade `Possible\Guaranteed` surfaced here (not in default `check-project`). No CEL/Datalog eval, no mocks (spec §5).

- [ ] **Step 1: Write failing tests** — `crates/lute-cli/tests/scenario.rs` (use `std::process::Command` on the built binary, or `assert_cmd` if already a dev-dep; else invoke the `run_scenario` fn directly if exposed for testing)

```rust
#[test]
fn scenario_envelope_reports_guaranteed_for_scene() {
    // build a temp project dir with A (entry, sets run.a) and B (after visited(Akey))
    let out = run_scenario_envelope(tmp_dir(), "b.s01ep01");
    assert!(out.contains("run.a"));
    assert!(out.contains("under your declared routes"));
}
#[test]
fn scenario_envelope_quest_without_after_shows_defaults_note() {
    let out = run_scenario_envelope(tmp_dir(), "quest:someQuest");
    assert!(out.contains("declaring `after`")); // enrichment note
}
```

- [ ] **Step 2: Run to verify fail** — `cargo test -p lute-cli --test scenario` → FAIL.

- [ ] **Step 3: Implement** — add the `Scenario` arm + `ScenarioCommand` enum + `run_scenario` dispatch (assemble the project graph via the same `by_root` collection used by `check-project`, then format the requested view). Confirm `check-project` already emits the full §6 diagnostics table (T3–T7, T11 wired errors) — add a `crates/lute-cli/tests` assertion that a project with an unknown node exits non-zero with `E-CONN-UNKNOWN-NODE`.

- [ ] **Step 4: Run to verify pass** — `cargo test -p lute-cli` → PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/lute-cli/src/main.rs crates/lute-cli/tests/scenario.rs
git commit -m "feat(cli): lute scenario reach/envelope command + check-project diagnostics wiring (connectivity T14)"
```

---

### Task 15: Corpus grounding + soundness/wording invariant suite

**Spec:** §7 (testing approach). **Files:**
- Create/extend: `crates/lute-check/tests/connectivity.rs` (corpus + invariant tests)
- Test: full touched-crate suites

**Interfaces:** Consumes everything above.

- [ ] **Step 1: Corpus grounding tests**

```rust
#[test]
fn corpus_no_false_positive_episode_dup_across_subprojects() {
    // run check-project over docs/examples/; the demo/bianca cross-subproject reuse must NOT flag E-CONN-EPISODE-ID-DUP
    let res = check_project_dir("docs/examples");
    assert!(!res.iter().any(|(_p, d)| d.code == "E-CONN-EPISODE-ID-DUP"));
}
#[test]
fn corpus_halsin_relational_objective_not_dead() {
    let res = check_project_dir("docs/examples");
    // quest-rescue-halsin's holds(canReach(...)) objective stays live (facts-seeded derived relation)
    assert!(!res.iter().any(|(_p, d)| d.code == "E-OBJECTIVE-UNSATISFIABLE"));
}
```

- [ ] **Step 2: Soundness-invariant + wording-lint tests**

```rust
#[test]
fn envelope_never_newly_errors_a_clean_standalone_scene() {
    for scene in SHIPPED_CLEAN_SCENES {
        let single = check(&input_for(scene));
        if single.diagnostics.iter().all(|d| d.severity != Severity::Error) {
            let proj = check_project_fixture(&[scene]);
            assert!(!proj.iter().any(|(_p, d)| d.code == "E-STATE-MAYBE-UNAVAILABLE"),
                "envelope newly errored a clean-standalone file");
        }
    }
}
#[test]
fn all_route_diagnostics_carry_declared_routes_qualifier() {
    for (_p, d) in produce_all_route_class_diags() {
        // E-CONN-UNREACHABLE, E-STATE-MAYBE-UNAVAILABLE, relational-liveness cause
        assert!(d.message.contains("under your declared routes"),
            "{} missing declared-routes qualifier", d.code);
    }
}
```

- [ ] **Step 3: Run to verify pass**

Run: `cargo test -p lute-check --test connectivity`
Expected: PASS.

- [ ] **Step 4: Full touched-crate verification (no piping — real exit codes)**

Run: `cargo test -p lute-check && cargo test -p lute-compile && cargo test -p lute-cli`
Expected: all PASS (0 failed).

- [ ] **Step 5: Commit**

```bash
git add crates/lute-check/tests/connectivity.rs
git commit -m "test(check): connectivity corpus grounding + soundness/wording invariants (connectivity T15)"
```

---

## Self-Review (run by the plan author, not a subagent)

**1. Spec coverage** — mapped: §2.1→T2; §2.2/§2.5→T1; §2.3→T3/T4; §2.4→T5/T6; §2.6→T13 (+ wording in T6/T7/T11/T15); §3→T3/T4 (frontmatter collection); §4.1 §A→T3/T4/T5/T6; §4.2 §B→T7; §4.3 §C→T8/T9/T10/T11; §4.4 §D→T12; §5 tooling→T14 (+ T2 `check` local validation, `trace` unaffected = no task, correct); §6 diagnostics table→each owning task; §7 testing→T15 (+ per-task tests); §8 future work→intentionally NOT implemented (negation, posture-B, precise per-quest envelope, warning promotion). No gaps.

**2. Placeholder scan** — code steps show concrete test bodies + impl sketches with exact anchor paths/lines; `todo!()` appears only inside Step-3 skeletons that the same step's prose fully specifies (acceptable scaffolding, resolved within the task), never as a delivered deliverable.

**3. Type consistency** — `PrereqFormula`/`Atom` (T1) consumed by T4/T5/T6/T10; `ConnGraph`/`NodeInfo` (T5) consumed by T6/T7/T10/T11/T12/T14; `check_definite_assignment` arity change (T8) — T8 Step 3 mandates fixing ALL callsites via `lsp references`; `Env` (T10) consumed by T11/T12/T14; `canonical_episode_key` (T3) reused by lute-compile. Diagnostic codes match the §6 table verbatim.

## Execution Handoff

**Plan complete and saved to `docs/superpowers/plans/2026-07-13-lute-connectivity-layer.md`. Two execution options:**

**1. Subagent-Driven (recommended)** — a fresh subagent per task, two-stage review (spec + quality) between tasks, on a `feat/lute-connectivity` branch. Mostly sequential (shared `connectivity.rs`/`envelope.rs`/`main.rs`; single worktree).

**2. Inline Execution** — batch tasks in this session with checkpoints.

**Which approach?**
