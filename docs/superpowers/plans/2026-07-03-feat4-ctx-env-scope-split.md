# FEAT-4: `Ctx` Env/Scope Split — maintainability refactor (audit #5)

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development.
> **Scope note:** the FEAT-4 phase headline was "reusable abstractions (macros/components)". That construct is UNSPECIFIED (no grammar/semantics in the DSL spec) — building it blind is out of scope for an autonomous pass; it is captured as a proposed design in `docs/superpowers/specs/2026-07-03-components-macros-proposal.md` for user review. This plan delivers the in-label, well-defined, safe part: the `Ctx` env/scope split the audit flagged as the foundation for that future work AND a standalone maintainability + perf win.

**Goal:** Split the checker's `Ctx` God-object into an immutable analysis **`Env`** (schema + def tables + mode) borrowed by reference, and a small lexical **scope** (`in_match`, `match_subject`) that is cheap to derive — eliminating the two full-`Ctx` deep clones per `<match>` (check.rs:467, 486) that currently copy `state`/`defs`/`def_types`/`def_params`. Behavior-preserving.

**Architecture:** `Ctx<'a> { env: &'a Env, in_match: bool, match_subject: Option<String> }` where `Env { state, defs, def_types, def_params, mode }`. `ctx.clone()` becomes cheap (copies a reference + two small fields). All `ctx.<envfield>` reads route to `ctx.env.<field>`. The two match clones become `Ctx { env: ctx.env, in_match, match_subject }`.

**Tech Stack:** Rust; `lute-check` (`ctx.rs`, `check.rs`, `cel_resolve.rs`, `set_op.rs`, `match_check.rs`, `defassign.rs`, `timeline.rs` — every `Ctx` consumer).

## Global Constraints
- `export PATH="$HOME/.cargo/bin:$PATH"` every shell. Worktree `/Users/journey/Workspace/lute/.worktrees/lute-lsp-rust` (branch `feat/lute-lsp-rust`); ABSOLUTE worktree paths; cargo/git cwd = worktree.
- Per-task hygiene BEFORE commit: `cargo fmt -p lute-check` + `cargo clippy -p lute-check --all-targets -- -D warnings`.
- **Behavior-preserving:** every existing lute-check test + the workspace suite MUST stay green byte-for-byte in diagnostics. This is a pure refactor — NO diagnostic behavior changes. capabilityVersion / tree-sitter untouched. Keep `divergence.rs` green.
- CROSS-CRATE: `Ctx`/`Env` are `lute-check`-internal (grep for external `Ctx` construction — the LSP builds a snapshot/imports, not a checker `Ctx`; `resolve_timeline(tl, ctx, snapshot)` takes `&Ctx` and is pub — confirm callers). `cargo build -p lute-check -p lute-lsp -p lute-cli`.

## Grounding
- `Ctx` today (`crates/lute-check/src/ctx.rs`): `{ in_match: bool, match_subject: Option<String>, mode: Mode, state: StateSchema, defs: BTreeSet<String>, def_types: BTreeMap<String,Type>, def_params: BTreeMap<String,Vec<(String,Type)>> }` + `#[derive(Default, Clone)]`.
- Built once at `check.rs:313` (`base_ctx`). Cloned wholesale at `check.rs:467` (`subject_ctx`, toggles off match) and `check.rs:486` (`arm_ctx`, toggles on match + subject).
- Consumers reading env fields: `cel_resolve::check_cel_slot` (`ctx.defs`, `ctx.def_types`, `ctx.def_params`, `ctx.state`, `ctx.in_match`), `set_op` (`ctx` param, `resolve_type(path, &ctx.state)` via caller), `match_check`, `defassign`, `timeline::resolve_timeline(_ctx)`, `check_choice_persist(ctx)` (`ctx.state`).

---

## Task 1: Introduce `Env`, make `Ctx` borrow it, simplify the match clones

**Files:** `crates/lute-check/src/ctx.rs` (define `Env`, `Ctx<'a>`), and every consumer: `check.rs`, `cel_resolve.rs`, `set_op.rs`, `match_check.rs`, `defassign.rs`, `timeline.rs` (+ their tests).

**Interfaces:**
```rust
// ctx.rs
#[derive(Default, Clone)]
pub struct Env {
    pub mode: Mode,
    pub state: StateSchema,
    pub defs: BTreeSet<String>,
    pub def_types: BTreeMap<String, Type>,
    pub def_params: BTreeMap<String, Vec<(String, Type)>>,
}
#[derive(Clone)]
pub struct Ctx<'a> {
    pub env: &'a Env,
    pub in_match: bool,
    pub match_subject: Option<String>,
}
```
(Keep field names on `Env` identical to today's `Ctx` fields so the rename is `ctx.X` → `ctx.env.X` for env fields; `ctx.in_match`/`ctx.match_subject` unchanged.)

- [ ] **Step 1: Define `Env` + `Ctx<'a>` in ctx.rs**; keep the existing doc comments (move the env-field docs onto `Env`). Provide `impl Default for Ctx<'_>` ONLY if needed by tests — prefer test helpers building an `Env` then `Ctx { env: &env, .. }`. (A `Ctx` holding a reference can't be `Default` without a static Env; adjust test helpers instead — see Step 4.)

- [ ] **Step 2: Update `check.rs`** — build a `let env = Env { mode: input.mode, state: schema, defs, def_types, def_params };` then `let base_ctx = Ctx { env: &env, in_match: false, match_subject: None };`. Replace the two match clones:
  ```rust
  let subject_ctx = Ctx { env: ctx.env, in_match: false, match_subject: None };
  let arm_ctx = Ctx { env: ctx.env, in_match: true, match_subject: Some(m.subject.raw.clone()) };
  ```
  Update all `ctx.state`→`ctx.env.state`, `ctx.defs`→`ctx.env.defs`, `ctx.def_types`→`ctx.env.def_types`, `ctx.def_params`→`ctx.env.def_params`, `ctx.mode`→`ctx.env.mode` (leave `ctx.in_match`/`ctx.match_subject`). Add the `<'a>` lifetime to `Walker`/functions that store or return a `Ctx` as needed.

- [ ] **Step 3: Update consumers** — `cel_resolve.rs`, `set_op.rs`, `match_check.rs`, `defassign.rs`, `timeline.rs`: change `&Ctx` params to `&Ctx<'_>` (or let elision handle it) and route env-field reads through `ctx.env.*`. `resolve_timeline`'s `_ctx: &Ctx` → `_ctx: &Ctx<'_>`.

- [ ] **Step 4: Update tests** — test helpers that build a `Ctx { state: .., .. }` now build `let env = Env { .. }; Ctx { env: &env, in_match, match_subject }`. Where a test returned/stored a `Ctx`, keep the `Env` alive in the test scope (bind it to a local). Update `ctx_with_defs`/`ctx()` style helpers (cel_resolve, timeline, set_op tests) accordingly.

- [ ] **Step 5: Build + test + hygiene**
```
cargo build -p lute-check -p lute-lsp -p lute-cli
cargo test -p lute-check
cargo test -p lute-lsp --test divergence
cargo fmt -p lute-check && cargo clippy -p lute-check --all-targets -- -D warnings
```
Expected: ALL green (behavior-preserving), 0 warnings. If any diagnostic output changed, the refactor is wrong — investigate.

- [ ] **Step 6: Commit**
```bash
cd /Users/journey/Workspace/lute/.worktrees/lute-lsp-rust
git add crates/lute-check/src/
git commit -m "refactor(check): split Ctx into borrowed Env + lexical scope; cheap match clones (FEAT-4, audit #5)"
```

## Verification (controller, after review)
```
cargo test --workspace   # all green, diagnostics unchanged
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
cargo test -p lute-manifest --test tree_sitter_stamp
```

## Self-Review
- `Env` holds the immutable analysis tables; `Ctx<'a>` borrows it + carries only lexical scope; the two match clones no longer deep-copy the maps.
- Zero diagnostic behavior change (all tests green unchanged); pure refactor.
- No external `Ctx` construction broke (LSP/CLI don't build a checker `Ctx`).
