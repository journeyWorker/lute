# lute editor support 0.2.0 — quest constructs (Plan E of 5)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete editor/tooling support for the 0.2.0 quest constructs to parity with the 0.1.0 scene support — the tree-sitter grammar + queries + corpus for `<on>`/`<quest>`/`<objective>` (incl. self-closing `<objective/>`), the lute-lsp features (resolve/hover/completion/nav/semtok/symbols/folding) made quest-aware, and `lute tag` fixed to tag lines inside quest bodies.

**Architecture:** Two orthogonal gaps (per the seam survey): (1) LOCAL — every lute-lsp feature has a Plan-A transitional no-op `Node::On`/`Node::Objective` arm to replace; (2) STRUCTURAL — every lute-lsp walker iterates `doc.shots` ONLY, never the sibling `doc.quests` (and `<quest>` is a top-level non-`Node`, needing its own symbol/fold entry). Both must be closed. `lute tag`'s `collect_lines` (lute-check) is likewise `doc.shots`-only → a silent no-op on quest docs. The tree-sitter grammar has no `<on>`/`<quest>`/`<objective>` rule and no self-closing support anywhere. `lute check`/`compile` are already fully DocKind-transparent — no change needed there.

**Tech Stack:** Rust (lute-lsp, lute-check tag) + JavaScript (tree-sitter-lute grammar) + tree-sitter CLI corpus. Spec `docs/proposals/scenario-dsl/0.2.0.md`. Depends on Plans A–D committed (the AST, kind system, and — for the divergence/corpus quest fixture — `docs/examples/quest-grove.lute` from Plan D).

## Global Constraints

- The AST is committed (Plan A): `Document.quests: Vec<Quest>`; `Node::{On(On), Objective(Objective)}`; `Quest{id,id_span,title,start,fail,attrs,body,span}` (top-level, NOT a Node); `Objective{id,id_span,done,when,title,optional,attrs,body,span}`; `On{event,event_span,when,attrs,body,span}`.
- Event vocabulary for completion: `lute_manifest::snapshot::BUILTIN_LIFECYCLE_EVENTS` ∪ `snapshot.events.keys()` (Plan B/C).
- `lute check`/`compile`/`backend.rs`/`convert.rs` need NO change (DocKind resolved internally). All seams are inside the feature fns + tag.rs + grammar.
- tree-sitter grammar: `<objective/>` needs the grammar's FIRST self-closing alternative `choice(seq(open,"/>"), seq(open,">",body,close))`; `<on>`/`<quest>` have no self-close (per AST); `<quest>` is a `source_file`-level slot (like `shot`), not a `_node`; add `done`/`start`/`fail` to the `cel_key` choice so CEL attrs route through `cel_string`/`ref` (free highlight/tags coverage). NOTE `<hub>` is ALSO missing from the grammar (pre-existing gap) — add it too while here (same block shape) so the grammar covers the full 0.2.0 closed set; if out of appetite, at minimum do on/quest/objective and note hub.
- Keep the `lute_syntax::walk::for_each_cel_slot`-based passes (`all_slots`) as-is — they already cross into `doc.quests`; only the STRUCTURAL shots-only walkers need a `doc.quests` counterpart.
- Work in the worktree `~/Workspace/lute/.worktrees/lute-0.2.0` on branch `feat/lute-0.2.0`. Run the crate's own tests per task; `cargo test --workspace` + `tree-sitter test` gate the final task. Note the pre-existing lute-compile e2e parallelism flake.

---

### Task 1: `lute tag` quest-awareness (lute-check tag.rs)

**Files:**
- Modify: `crates/lute-check/src/tag.rs` (`tag_document` ~24, `collect_lines` ~145-164)
- Test: `crates/lute-check/tests/` (a tag test) or `crates/lute-cli/tests/tag.rs`

**Interfaces:**
- Produces: `tag_document`/`collect_lines` walk `doc.quests` (recursing `Node::On`/`Node::Objective` bodies) in addition to `doc.shots`, keying the per-speaker code back-fill PER `<quest>` (identity scope = the quest, dsl 0.2.0 §7 — mirrors `check_line_codes`).

- [ ] **Step 1: Failing test** — `lute tag` (or `tag_document`) on a `kind: quest` doc with `:speaker:` lines inside an `<on>` arm assigns `code`s; before the fix it tags zero lines.
- [ ] **Step 2: Run** → FAIL (zero lines tagged).
- [ ] **Step 3: Implement.** In `collect_lines`, replace the blanket `Node::Objective(_) | Node::On(_) => {}` no-op with recursion into their `.body` (mirror the `Hub` arm already handled). In `tag_document`, after the `doc.shots` loop, add a `for quest in &doc.quests` loop that collects+tags that quest's lines with the quest's identity scope (per-quest code counter, prefix `{questId}` — mirror `check_line_codes`'s per-quest scoping from Plan C). Scene behavior unchanged.
- [ ] **Step 4: Run** `cargo test -p lute-check -p lute-cli` (the tag tests) → PASS.
- [ ] **Step 5: Commit** — `git commit -am "feat(check): lute tag walks quest bodies + per-quest code scope (dsl 0.2.0 §7)"`

---

### Task 2: lute-lsp structural quest-awareness (walkers reach `doc.quests`)

**Files:**
- Modify: `crates/lute-lsp/src/features/mod.rs` (`resolve` ~118-120 shots loop, `resolve_node` ~146-239, `attr_at`/`scan` ~522-571, `branch_span`, `collect_set_paths`), `crates/lute-lsp/src/features/{folding.rs ~27-31,44-78, symbols.rs ~28-29,56-113, semtok.rs ~118-126,141-251}`
- Test: `crates/lute-lsp/tests/`

**Interfaces:**
- Produces: every structural walker iterates `doc.quests` after `doc.shots`; `<quest>` gets a top-level symbol + fold; `Node::On`/`Node::Objective` get real folds/symbols/semtok keyword tokens + resolve/attr arms. Quest header (`id`/`title`/`start`/`fail`) is walked where relevant (Quest is not a Node).

- [ ] **Step 1: Failing tests** (one per feature, in lute-lsp tests):
  - `folding_ranges` on a quest doc returns folds for `<quest>` + `<on>`/`<objective>` (multi-line) bodies.
  - `document_symbols` on a quest doc returns a top-level symbol per `<quest>` with child symbols per `<objective>`/`<on>`.
  - `semantic_tokens` on a quest doc emits keyword tokens for `<quest>`/`<on>`/`<objective>` opening tags + CEL sub-tokens in `done=`/`start=` (the CEL sub-tokens already flow via `all_slots`; the keyword tokens are new).
  - `resolve`/`hover`: a cursor on an `<on event="…">` or `<objective done="…">` resolves a Cursor (not None).
- [ ] **Step 2: Run** → FAIL.
- [ ] **Step 3: Implement.**
  - `folding.rs`: add `for quest in &doc.quests { push_fold(quest.span); fold_nodes(&quest.body, …) }`; replace the `Node::On|Objective` no-op arm with fold+recurse (mirror `Node::Branch` at folding.rs:46-51).
  - `symbols.rs`: add a `quest_symbol(q, idx)` (mirror `shot_symbol` ~33-48; SymbolKind::MODULE/NAMESPACE for the quest, named by id) called alongside the shots loop; replace the `collect_children` no-op with `Node::On`→child symbol (SymbolKind::EVENT, named by event), `Node::Objective`→child (SymbolKind::FIELD/PROPERTY, named by id).
  - `semtok.rs`: extend `semantic_tokens` (118-126) to also `walk_nodes(&quest.body, …)` per quest + emit quest-header keyword/attr tokens; replace the `Node::On|Objective` no-op (semtok.rs:251) with real keyword tokens (mirror `Node::Branch`/`Match` at 157-207).
  - `mod.rs`: `resolve` (118) add a `doc.quests` walk (each quest's header slots start/fail + body via `resolve_nodes`); `resolve_node` (239) replace the `None` stub with real arms (an `<on event>` value position → a NEW `Cursor::EventName`-style variant or reuse an attr-value cursor; `done`/`when`/`start`/`fail` CEL → the existing `Cursor::Cel`); `attr_at`/`scan` (571) replace the `return None` with attr scanning of on/objective/quest attrs; make `branch_span`/`collect_set_paths` also traverse `doc.quests` so quest-local `<branch>`/`::set` targets resolve.
- [ ] **Step 4: Run** `cargo test -p lute-lsp` → PASS.
- [ ] **Step 5: Commit** — `git commit -am "feat(lsp): quest-aware folding/symbols/semtok/resolve over doc.quests (dsl 0.2.0)"`

---

### Task 3: lute-lsp completion + hover for quest constructs

**Files:**
- Modify: `crates/lute-lsp/src/features/completion.rs` (~29-79, `attr_key_items` ~104, `choice_path_items` ~218), `crates/lute-lsp/src/features/hover.rs` (~34-118), `crates/lute-lsp/src/features/mod.rs` (`Cursor` enum ~85-116 — new variants as needed)
- Test: `crates/lute-lsp/tests/`

**Interfaces:**
- Produces: attr-key completion for `<quest>` (`id`/`title`/`start`/`fail`), `<objective>` (`id`/`done`/`when`/`title`/`optional`), `<on>` (`event`/`when`) via a NEW hardcoded per-construct attr table (these attrs are NOT snapshot-driven, unlike `::directive` attrs); `<on event="…">` value completion from `BUILTIN_LIFECYCLE_EVENTS ∪ snapshot.events`; frontmatter `kind:` value completion (`scene`/`quest`); hover for on/objective/quest keywords + `event`/`done`/`start`/`fail`.

- [ ] **Step 1: Failing tests**: completion at `<on event="|">` lists `questComplete` + any plugin events; completion at `<objective |>` lists `done`/`when`/`optional`/etc.; hover on `<on>`/`<objective>` renders a doc.
- [ ] **Step 2: Run** → FAIL.
- [ ] **Step 3: Implement.** Add a hardcoded attr-key table for quest/on/objective (a small `const`/match keyed on the construct, analogous to how the grammar's `cel_key` enumerates CEL keys); wire it into `complete_at` when the cursor is an attr-key position inside those constructs (new Cursor arms from Task 2). Event-name value completion: when the cursor is the `event=` value of an `<on>`, emit `BUILTIN_LIFECYCLE_EVENTS` + `snapshot.events.keys()` items. `kind:` value completion: detect a frontmatter `kind:` value cursor (a small frontmatter-scan since `resolve()` is body-only — a minimal `kind:`-line detector on `doc.meta.raw_yaml` at the cursor offset) → `scene`/`quest`. Hover: render on/objective/quest keyword docs (mirror `directive_hover`/`state_hover`).
- [ ] **Step 4: Run** `cargo test -p lute-lsp` → PASS.
- [ ] **Step 5: Commit** — `git commit -am "feat(lsp): completion + hover for quest/on/objective + kind:/event values (dsl 0.2.0)"`

---

### Task 4: tree-sitter grammar — `<on>`/`<quest>`/`<objective>` + self-closing

**Files:**
- Modify: `tree-sitter-lute/grammar.js` (`source_file` ~39-40, `_node` choice ~55-63, block rules ~93-160, `cel_key` ~181)
- Test: regenerate + `tree-sitter test` (Task 5 adds corpus).

**Interfaces:**
- Produces: grammar rules `quest` (source_file-level, like `shot`), `on` (a `_node`), `objective` (a `_node`, self-closing alternative), with `cel_key` extended by `done`/`start`/`fail`. (Also add the missing `hub` rule for completeness.)

- [ ] **Step 1: Implement grammar.** In `source_file` (grammar.js:39-40) add `$.quest` to the repeated top-level choice (`repeat(choice($.shot, $.quest))`); add `$.on`, `$.objective` (and `$.hub`) to `_node` (grammar.js:55-63). Add rules (mirror `branch` at 93-101):
  ```js
  quest: $ => seq("<quest", repeat($._tag_attr), ">", repeat($._node), "</quest>"),
  on: $ => seq("<on", repeat($._tag_attr), ">", repeat($._node), "</on>"),
  objective: $ => choice(
    seq("<objective", repeat($._tag_attr), "/>"),
    seq("<objective", repeat($._tag_attr), ">", repeat($._node), "</objective>"),
  ),
  ```
  Extend `cel_key` (grammar.js:181) choice with `"done"`, `"start"`, `"fail"` (so those attr values parse as `cel_string`/`ref`). `event` stays a plain `attr`.
- [ ] **Step 2: Regenerate** — `cd tree-sitter-lute && tree-sitter generate` (or `npx tree-sitter generate`) → parser regenerates without conflicts. Resolve any grammar conflicts per tree-sitter's report (the self-closing `choice` may need care — the `/>` vs `>` lookahead is LR(1)-clean since `/` is not a valid attr start).
- [ ] **Step 3: Smoke** — `tree-sitter parse` a small quest doc → a well-formed tree with `quest`/`objective`/`on` nodes (no ERROR nodes).
- [ ] **Step 4: Commit** — `git commit -am "feat(tree-sitter): <quest>/<on>/<objective> rules + self-closing objective (dsl 0.2.0)"`

---

### Task 5: tree-sitter queries + corpus + capability-version stamp

**Files:**
- Modify: `tree-sitter-lute/queries/{highlights.scm, folds.scm, tags.scm}`, `tree-sitter-lute/tree-sitter.json` + `package.json` (capabilityVersion stamp IF it changed), `crates/lute-manifest/tests/tree_sitter_stamp.rs` context
- Create: `tree-sitter-lute/test/corpus/quest.txt`
- Test: `tree-sitter test` + `cargo test -p lute-manifest --test tree_sitter_stamp`

**Interfaces:**
- Produces: highlight captures for the new block keywords; fold captures `(quest)`/`(on)`/`(objective)`; tags `(quest id) @definition.class` + `(objective id) @definition.function`; a corpus file exercising the new constructs; the tree-sitter capabilityVersion stamp kept in sync IF `load_core_snapshot().version` changed this branch (it did NOT — no core.rs schema change in Plans A–E — but VERIFY: `cargo test -p lute-manifest --test tree_sitter_stamp` must stay green; if it fails, update `tree-sitter.json`/`package.json` `metadata.capabilityVersion` to the current `load_core_snapshot().version`).

- [ ] **Step 1: Queries.** Add to `highlights.scm`: `(quest ["<quest" "</quest>"] @keyword.control)`, `(on ["<on" "</on>"] @keyword.control)`, `(objective ["<objective" "</objective>" "/>"] @keyword.control)` (CEL values already covered by the generic `(cel_string (path) @property)`/`(ref) @variable.parameter` patterns via the `cel_key` extension in Task 4). Add to `folds.scm`: `(quest) @fold`, `(on) @fold`, `(objective) @fold`. Add to `tags.scm`: `(quest (attr (key) @_key (string) @name) (#eq? @_key "id")) @definition.class` and the objective analogue `@definition.function` (mirror the branch/choice id-attr patterns).
- [ ] **Step 2: Corpus.** Create `tree-sitter-lute/test/corpus/quest.txt` with cases (corpus format = `===\n<name>\n===\n<src>\n---\n<S-expr>\n`): a quest with an objective + an `<on>` (nesting parity with branch/match), a self-closing `<objective/>`, and a CEL-valued `done=`/`start=`/`when=` case (produces `(cel_attr (cel_key) (cel_string …))`).
- [ ] **Step 3: Run** `cd tree-sitter-lute && tree-sitter test` → all corpus cases PASS (incl. existing). Adjust expected S-exprs to the actual parse output on first run, then lock.
- [ ] **Step 4: Stamp check** — `cargo test -p lute-manifest --test tree_sitter_stamp` → PASS (update the JSON stamps only if it fails).
- [ ] **Step 5: Commit** — `git commit -am "feat(tree-sitter): highlight/fold/tag queries + corpus for quest constructs (dsl 0.2.0)"`

---

### Task 6: LSP divergence golden (quest) + full green

**Files:**
- Modify: `crates/lute-lsp/tests/divergence.rs` (add a quest fixture case)
- Test: full `cargo test --workspace` + `tree-sitter test`.

**Interfaces:**
- Produces: a quest-doc case in the headless-vs-LSP diagnostic-parity golden (using `docs/examples/quest-grove.lute` from Plan D, or an inline quest source), proving the LSP surfaces the SAME quest diagnostics as headless `check()` byte-for-byte.

- [ ] **Step 1: Add** a quest fixture to `divergence.rs` (mirror the existing scene divergence case) — assert headless `check()` diagnostics == LSP-published diagnostics for a quest doc (clean + an intentionally-erroring one, e.g. E-OBJECTIVE-MISSING-DONE).
- [ ] **Step 2: Run** `cargo test -p lute-lsp --test divergence` → PASS.
- [ ] **Step 3: Full green** — `cargo test --workspace` → GREEN (all crates; confirm the lute-compile e2e 7/7 in isolation) and `cd tree-sitter-lute && tree-sitter test` → PASS.
- [ ] **Step 4: Commit** — `git commit -am "test(lsp): quest-doc divergence golden (headless == LSP diagnostics)"`

---

## Self-Review checklist (run before executing)

1. **Spec coverage:** tree-sitter grammar/queries/corpus for on/quest/objective + self-closing → T4,T5; LSP structural quest-awareness + real feature arms → T2,T3; `lute tag` quest bodies → T1; LSP↔headless parity → T6.
2. **Placeholder scan:** LSP feature tests + corpus S-exprs are finalized against actual output on first run (flagged); grammar rules + query captures are concrete.
3. **Type consistency:** the new `Cursor` variant(s) from T2 are consumed by T3's completion/hover; construct→attr tables stable.
4. **No backend/convert/check/compile changes** (they are DocKind-transparent) — all seams in feature fns + tag.rs + grammar/queries.
5. **Stamp:** tree_sitter_stamp stays green (no core capability_version change in Plans A–E; verify, don't assume).
