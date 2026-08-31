# Subquest (`<objective quest=>`) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** First-class subquests: a parent quest's `<objective quest="childId"/>` makes the child's completion part of the parent's derived completion, with bidirectional derived failure.

**Architecture:** One new AST attribute; predicate **synthesis in `lute_compile::normalize`** (shared by `compile` and `trace`, so upward semantics need no engine change); doc-level + project-level structural checks; trace-side engine rules for the two things synthesis cannot express (child activation, downward cascade).

**Tech Stack:** Rust workspace (`lute-syntax`, `lute-check`, `lute-compile`, `lute-trace`), insta snapshots, conformance fixtures.

**Spec:** `docs/superpowers/specs/2026-08-31-lute-subquest-design.md`

## Global Constraints

- Synthesized done raw text: `quest.<child>.state == 'complete'` (exact).
- Synthesized fail raw text: `(<authoredFail>) || quest.<c1>.state == 'failed' || …` — required children only, doc order; no authored fail → disjunction only.
- AST: `Objective.quest: Option<String>` + `quest_span: Span` (fallback: open-tag span).
- IR: `ObjectiveEntry.quest: Option<String>`, `#[serde(skip_serializing_if = "Option::is_none")]`, appended AFTER `body` (byte-stability: artifacts without the feature are byte-identical).
- Diag codes (exact): `E-OBJECTIVE-QUEST-DONE`, `E-QUEST-REF-UNKNOWN`, `E-QUEST-MULTI-PARENT`, `E-QUEST-TREE-CYCLE`.
- Tree, not DAG: ≤1 parent per quest; cycles rejected; depth unbounded.
- Checker (`lute-check`) does NOT run synthesis; it validates the authored surface.
- No task runs workspace-wide builds/tests/formatters mid-flight; integration verification happens once at the end.

---

### Task A: AST + parser (foundation)

**Files:** `crates/lute-syntax/src/ast.rs` (Objective struct), `crates/lute-syntax/src/parser/blocks.rs` (`parse_objective`).

- Add `quest: Option<String>`, `quest_span: Span` to `Objective`; extract via `take_str_spanned(&mut attrs, "quest")` in `parse_objective` (before residual `attrs` capture), fallback span = open-tag span. Missing `done` keeps today's empty-slot idiom.
- Fix all struct-literal construction sites (grep `Objective {`).

### Task B: doc-level checks (`lute-check`)

**Files:** `crates/lute-check/src/match_check.rs` (`check_quest`), tests inline + `crates/lute-check/tests/quest.rs`.

- `quest=` + non-empty `done=` → `E-OBJECTIVE-QUEST-DONE` (error, at objective).
- `quest=` present + empty done → suppress `E-OBJECTIVE-MISSING-DONE` and any empty-CEL-slot diagnostics for that done slot.
- Neither → today's missing-done diagnostic unchanged.
- `quest=` equal to the enclosing quest id → `E-QUEST-TREE-CYCLE` (doc-level early catch, length-1).
- Reserved-decl fold (`quest.<id>.objectives.<oid>.done`) unchanged for subquest objectives.
- Unknown `quest=` ids stay silent at doc level (may be cross-file; project pass owns it).

### Task C: project-level tree checks (`lute-check`)

**Files:** `crates/lute-check/src/project_check.rs` (reuse `defined_quests`; new pass `check_project_quest_tree`), CLI wiring where `check_project_quest_ids`/`check_project_quest_refs` are invoked; tests in a NEW file `crates/lute-check/tests/quest_tree.rs` (avoid colliding with Task B).

- Collect edges: for every doc, every quest, every objective with `quest=Some(c)` → edge (parent, c, span, required=!optional).
- `E-QUEST-REF-UNKNOWN`: `c` not in `defined_quests` union.
- `E-QUEST-MULTI-PARENT`: `c` referenced from ≥2 distinct parent quests (anchor at the second-by-path occurrence, mirroring `check_project_quest_ids` style).
- `E-QUEST-TREE-CYCLE`: cycle in parent→child edges (DFS; report once per cycle, anchored at the edge closing it).
- `E-OBJECTIVE-UNSATISFIABLE` propagation: where check-project already aggregates per-file `check()` diags, post-process — child quest carrying `E-QUEST-UNREACHABLE` + referenced by a REQUIRED objective → emit `E-OBJECTIVE-UNSATISFIABLE` at the referencing objective's span, message naming the child and its unreachability.

### Task D: synthesis + IR (`lute-compile`)

**Files:** `crates/lute-compile/src/normalize.rs` (synthesis pass inside `normalize_document`), `crates/lute-compile/src/ir.rs` (`ObjectiveEntry.quest`), `crates/lute-compile/src/stage.rs` (`walk_quest` populates `quest`), e2e snapshot test with a new subquest fixture.

- Synthesis (per Global Constraints) mutates the AST: fill empty `done` slots of `quest=` objectives; extend/synthesize parent `fail`. Runs for both `compile` and `trace` by construction (trace calls `normalize_document`).
- `stage.rs`: `quest: o.quest.clone()` into `ObjectiveEntry`; `done: CelPair::from_raw(&o.done.raw)` now carries the synthesized text.
- Existing snapshots (`e2e__quest_grove.snap` etc.) must remain byte-identical; add one new snapshot exercising: parent with 2 required subquest objectives + 1 optional + 1 plain, authored fail present.

### Task E: trace engine rules + conformance (`lute-trace`)

**Files:** `crates/lute-trace/src/walk.rs` (`walk_quests`/`walk_quest`/`settle_quest`), `crates/lute-trace/tests/quest.rs`, new `conformance/quest-subquest/` mirroring `conformance/quest-complete/` (find and wire its harness).

- Build same-doc child→parent map from `doc.quests` `quest=` attrs.
- Activation: referenced child with no `start` activates when its parent activates (not at walk start; not accept-driven — extend the `E-TRACE-ACCEPT` guard's reasoning/message accordingly); with `start`, the predicate is evaluated only while the parent is active.
- Restructure to a settle fixpoint: hold per-quest lifecycle in a map; repeat passes (doc order) of activation + `settle_quest` for `Active` quests until a full pass makes no transition (monotonic transitions bound it).
- Downward cascade: on a quest's terminal transition (`failed`/`complete`), every still-`active` child transitions to `failed` + fires `questFailed`; recursive.
- Upward failure needs NO trace code — the synthesized `fail` disjunction (Task D) covers it; add a test proving it.

### Task F: docs (`docs/`, website, CHANGELOG)

**Files:** `docs/runtime/quest-lifecycle.md`, `packages/website/src/content/docs/language/quests-and-scenes.md`, new `docs/examples/quest-subquest.lute` + `docs/examples/README.md` entry, `CHANGELOG.md` (this task ONLY touches CHANGELOG).

- quest-lifecycle.md: spec §2.3 (downward cascade) and §2.4 (activation of referenced children) engine rules, `ObjectiveEntry.quest` union note.
- quests-and-scenes.md: a "Subquests" section — syntax, semantics table, the four new diag codes, one worked example.
- Example file: BG3-style parent + two children, compiling mentally against the spec (checked in the final integration pass).

### Final integration (plan owner)

- `cargo build --workspace && cargo test --workspace`; fix fallout; snapshot review (`insta`); run `lute check`/`compile`/`trace` on the new example; commit per logical unit.
