# FEAT-3: `<choice>` Persist Sugar — richer branch control-flow (dsl §11.1.1)

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development.
> **Design of record (autonomous decision — user reviews at showcase).**

**Goal:** Implement the `<choice … persist="run" as="run.<path>" [value="<lit>"]>` authoring sugar (dsl §11.1.1): a choice can record a NAMED, declared `run.*` fact when selected, without hand-writing `::set`. The sugar is **exactly** `::set{run.<path> = <value>}` appended to that choice's arm — no extra power. The static checker VALIDATES the sugar (materializing the `::set` is engine-side, out of scope).

**Architecture:** A validation pass over `<branch>` choices (with the resolved state schema in scope) checks each choice carrying `persist`. Reuses `set_op::resolve_type` for the `as` path's declared type and the existing type/literal machinery. No AST injection — the checker only validates well-formedness (per §3.2 the checker validates; the engine reduces).

**Tech Stack:** Rust; `lute-check` (`check.rs` branch walk, `set_op`, `meta`); spec `docs/proposals/scenario-dsl/0.0.1.md`.

## Global Constraints
- `export PATH="$HOME/.cargo/bin:$PATH"` every shell. Worktree `/Users/journey/Workspace/lute/.worktrees/lute-lsp-rust` (branch `feat/lute-lsp-rust`); ABSOLUTE worktree paths; cargo/git cwd = worktree.
- Per-task hygiene BEFORE commit: `cargo fmt -p lute-check` + `cargo clippy -p lute-check --all-targets -- -D warnings`.
- Determinism, never-panic. capabilityVersion / tree-sitter untouched. Keep `divergence.rs` green (this is a checker diagnostic; CLI+LSP share `check()` so no divergence work needed).
- New codes: `E-PERSIST-TARGET`, `E-PERSIST-MISSING-AS`, `E-PERSIST-VALUE`, `E-PERSIST-CONFLICT` (reuse `E-UNDECLARED` for an undeclared `as` path).

## Design (validation rules — decision of record)
For each `<choice>` whose `attrs` contain `persist`:
1. **`persist` value MUST be `"run"`** (§11.1.1 shows `persist="run"`; cross-episode facts live in `run.*`). Else `E-PERSIST-TARGET`.
2. **`as` is REQUIRED** and MUST be a `run.*` path (matches `persist="run"`). Missing → `E-PERSIST-MISSING-AS`; a non-`run.*` `as` → `E-PERSIST-TARGET`.
3. **`as` MUST resolve to an already-declared path** in the merged schema (§9.2). Undeclared → `E-UNDECLARED` (state-by-typo must fail; reuse the existing code/message shape). Resolve its type via `set_op::resolve_type(as_path, &ctx.state)`.
4. **`value`:**
   - `as` type = `bool`: `value` OPTIONAL (defaults to `true`); if present it MUST be a bool literal (`true`/`false`) → else `E-PERSIST-VALUE`.
   - `as` type = `number`/`enum`: `value` REQUIRED (§11.1.1) → missing = `E-PERSIST-VALUE`; if present it MUST be type-compatible (a numeric literal for `number`; a member of the enum for `enum`) → else `E-PERSIST-VALUE`.
5. **No arm write-conflict:** if the choice's `body` already contains a `::set` whose target path equals `as`, the persist write duplicates it → `E-PERSIST-CONFLICT`.
The persist attrs are consumed (recognized), so they are NOT reported as unknown/extra attrs.

## File Structure
- `crates/lute-check/src/check.rs` — a `check_choice_persist(choice, ctx, diags)` helper called from the branch/choice walk (schema in `ctx.state`); recurses via the existing branch walk. New codes.
- `docs/examples/choice-persist.lute` — fixture (a branch whose choices persist run facts; a later `<match on="run.*">` reacts).
- `docs/proposals/scenario-dsl/0.0.1.md` — note the sugar is implemented + the E-PERSIST-* codes (§11.1.1 already specifies it).

---

## Task 1: `persist` sugar validation + fixture + spec

**Files:** Modify `crates/lute-check/src/check.rs` (+ unit/integration tests); create `docs/examples/choice-persist.lute`; edit the spec.

**Interfaces:**
- Consumes: `Choice.attrs` (`Vec<Attr>`; each `Attr { key, value: AttrValue, .. }` — `AttrValue::Str`/`Bare`/`Ref` — read the enum in `crates/lute-syntax/src/ast.rs`), `ctx.state: StateSchema`, `set_op::resolve_type(path, &schema) -> Option<&Type>`, `Type` variants.
- Produces: `E-PERSIST-*` diagnostics at the choice span.

- [ ] **Step 1: Write failing tests** in `crates/lute-check/tests/` (a new `choice_persist.rs`, mirroring `ref_type.rs`'s `check()`-over-inline-schema harness) OR check.rs unit tests. Cover:
  - `persist_bool_default_true_ok`: `run.helped: bool` declared (inline `state:` or via a small schema); `<choice persist="run" as="run.helped">` (no value) → clean.
  - `persist_number_requires_value`: `run.score: number`; `<choice persist="run" as="run.score">` (no value) → `E-PERSIST-VALUE`.
  - `persist_number_value_ok`: same with `value="3"` → clean.
  - `persist_undeclared_as_errors`: `as="run.ghost"` (not declared) → `E-UNDECLARED`.
  - `persist_non_run_target_errors`: `persist="run" as="scene.x"` → `E-PERSIST-TARGET`.
  - `persist_missing_as_errors`: `persist="run"` with no `as` → `E-PERSIST-MISSING-AS`.
  - `persist_arm_conflict_errors`: choice body already has `::set{run.helped = false}` and `persist as="run.helped"` → `E-PERSIST-CONFLICT`.
  - `persist_wrong_value_type_errors`: `run.helped: bool`, `value="7"` → `E-PERSIST-VALUE`.
  Build the schema for tests the way `ref_type.rs` does (inline `state:` frontmatter). Confirm they FAIL.

- [ ] **Step 2: Run** `cargo test -p lute-check --test choice_persist` — confirm fail.

- [ ] **Step 3: Implement `check_choice_persist`** in `check.rs`, called wherever the walker processes a `<branch>`'s choices (schema available via `ctx`). Read `persist`/`as`/`value` from `choice.attrs` (helper to fetch a string attr value). Apply the Design rules in order; short-circuit sensibly (e.g. a missing `as` reports `E-PERSIST-MISSING-AS` and skips value checks). For the enum-membership check, resolve the `as` path type; for `Type::Enum(members)`/`EnumFromOption` verify the value literal is a member (reuse how existing set/attr checks validate enum literals — grep for enum literal validation). For arm-conflict, scan `choice.body` for a `Node::Set` with `path == as`. Emit at `choice.span`.

- [ ] **Step 4: Run tests** `cargo test -p lute-check` — new tests pass; all prior green (persist attrs no longer flagged as unknown if there was such a check — verify branches/choices don't already reject unknown attrs; if they do, ensure persist/as/value are allowed).

- [ ] **Step 5: Fixture** `docs/examples/choice-persist.lute`: a scene with an inline `run.*` schema (or `uses:` a schema), a `<branch>` whose choices carry `persist="run" as="run.<fact>" [value=…]`, and a later `<match on="run.<fact>">` reacting — checks exit 0. `cargo build -p lute-cli && ./target/debug/lute check docs/examples/choice-persist.lute; echo exit=$?` → 0. Iterate until genuinely clean.

- [ ] **Step 6: Spec** — in §11.1.1 note the sugar is implemented and list the `E-PERSIST-*` validation codes (keep terse; the semantics are already specified there).

- [ ] **Step 7: fmt + clippy + commit**
```bash
cd /Users/journey/Workspace/lute/.worktrees/lute-lsp-rust
cargo fmt -p lute-check && cargo clippy -p lute-check --all-targets -- -D warnings
git add crates/lute-check/src/check.rs crates/lute-check/tests/choice_persist.rs docs/examples/choice-persist.lute docs/proposals/scenario-dsl/0.0.1.md
git commit -m "feat(check,examples): <choice persist=…> run-fact sugar validation (dsl §11.1.1)"
```

## Verification (controller, after review)
```
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
cargo test -p lute-manifest --test tree_sitter_stamp
./target/debug/lute check docs/examples/choice-persist.lute   # exit 0
```

## Self-Review
- All §11.1.1 rules enforced: persist=run, as declared+run+writable, value default-true-for-bool / required-typed-for-number-enum, arm-conflict.
- persist/as/value recognized (not "unknown attr"); validation only (engine materializes the ::set).
- Fixture exit 0 + demonstrates persist→later match reaction.
- Deferred (documented, minor): the `<choice value>` for a plain (non-persist) choice; multi-write conflict across sugar + multiple explicit sets (single-set conflict covered).
