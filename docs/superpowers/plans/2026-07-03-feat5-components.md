# FEAT-5: Reusable Content Components (`::use`) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development.
> **Design of record:** DSL spec §13 (already written in `docs/proposals/scenario-dsl/0.0.1.md`). Option C — directive-form, file-based, NO grammar change.
> **DOCS-IN-SYNC (user requirement):** every task that changes behavior updates the relevant doc IN THE SAME COMMIT (spec §13 wording, the components proposal doc, showcase + its README). No stale docs.

**Goal:** Reusable content components: a component file (`component:` + `params:` frontmatter, presentational body using `@param`) imported via `components: [..]` and expanded inline with `::use{ component="name" arg=val }`. Checker validates the invocation + the component file; expansion is engine-side.

**Architecture:** Reuse the existing frontmatter + directive + import-DAG surface. `MetaKind::Component` parse mode; a `component_import` resolver (DAG, mirrors `schema_import`) producing a `ComponentSet { table: name -> {params, body-doc, src}, diags }`; `check()` validates `::use` (reserved directive) against the table (declared, named-args match params in count+type, acyclic) and validates each component file in component mode. NO tree-sitter/parser change; capabilityVersion unchanged.

**Tech Stack:** Rust; `lute-check` (`meta.rs`, new `component_import.rs`, `check.rs`, `directives.rs`), `lute-cli`/`lute-lsp` (pass `components` import), spec + fixtures.

## Global Constraints
- `export PATH="$HOME/.cargo/bin:$PATH"` every shell. Worktree `/Users/journey/Workspace/lute/.worktrees/lute-lsp-rust` (branch `feat/lute-lsp-rust`); ABSOLUTE worktree paths; cargo/git cwd = worktree.
- Per-task hygiene BEFORE commit: `cargo fmt` + `cargo clippy --all-targets -- -D warnings` on touched crates.
- CROSS-CRATE: `CheckInput` gains a `components` field + resolver called by CLI+LSP (both `analyze`+`imports_for`) → grep call sites + `cargo build -p lute-check -p lute-lsp -p lute-cli`; keep `divergence.rs` green (add a components divergence golden).
- never-panic (resolver total: I/O/parse/cycle → diagnostic), determinism (BTreeMap/sorted). capabilityVersion / tree-sitter UNCHANGED (no grammar/snapshot field). `use` is a reserved directive (§10) recognized before the unknown-directive check.
- New codes: `E-COMPONENT-{UNDECLARED, ARG, CYCLE, PARSE, DUP}`.

## Design (v0.0.1 scope — spec §13.4)
- One component per file. Component file: frontmatter `component: <name>` + `params: { p: <type> }`; body = presentational (lines + staging directives + `@param` refs in attr/ref value positions).
- **Presentational only (v1):** a component body does NOT read/write scene/run state and contains no `<branch>/<match>/<timeline>` (validated in component mode with params as the only ref namespace + the assembled directive vocab; a state path or logic block in a body is a v1 error). Logic/stateful components = documented future work.
- `::use{ component="name" <arg>=<value> }`: named args bind to params by NAME; every required param supplied, no unknown arg, each value type-compatible with its param type (`type_accepts`). Missing/extra/mistyped → `E-COMPONENT-ARG`.
- Import DAG: `components: [path]` canonicalized; cross-file duplicate component `name` → `E-COMPONENT-DUP`; a `::use` expansion cycle (component A body `::use`s B … back to A) → `E-COMPONENT-CYCLE`; missing/parse → `E-COMPONENT-{UNDECLARED for the name}` / `E-COMPONENT-PARSE`.

---

## Task C1: `MetaKind::Component` + parse `component`/`params`/`components` keys
**Files:** `crates/lute-check/src/meta.rs` (+ tests).
- Add `TypedMeta.components: Vec<String>` (scene import list), `TypedMeta.component: Option<String>` (a component file's declared name), and reuse `DefParam`/a param list for a component file's `params` (component-file frontmatter). Add `MetaKind::Component` (skips scene-required keys, lifts `component`+`params`, like `MetaKind::Schema`). Add `component`/`params`/`components` to core meta keys.
- Failing tests: a scene `components: [greet.lute]` → `TypedMeta.components == ["greet.lute"]`; a component-mode parse of `component: greet\nparams: { who: providerRef(cast) }` → `component == Some("greet")` + params parsed; no unknown-key diag.
- Commit: `feat(check): parse component/params/components frontmatter + MetaKind::Component (dsl §13)`

## Task C2: component resolver `component_import.rs`
**Files:** create `crates/lute-check/src/component_import.rs`; wire into `check.rs` + `lib.rs`.
- `pub struct ComponentDef { pub params: Vec<(String, Type)>, pub body: lute_syntax::ast::Document, pub src: PathBuf }` and `pub struct ComponentSet { pub table: BTreeMap<String, ComponentDef>, pub diags: Vec<Diagnostic> }`.
- `pub fn resolve_components(base_dir: &Path, components: &[String], at: Span) -> ComponentSet` — DAG load (canonicalize, cycle→E-COMPONENT-CYCLE for import cycles, diamond dedup); parse each file, `parse_meta_kind(.., MetaKind::Component)`; require `component:` present (else E-COMPONENT-PARSE); cross-file dup name → E-COMPONENT-DUP; record params + body. TOTAL (never panic). Mirror `schema_import.rs` structure/tests (temp-dir helper).
- `CheckInput.components: ComponentSet` (pure input, like `imports`).
- Failing tests: resolve a component file → table has it with params; missing → not-found; two files same `component:` name → E-COMPONENT-DUP; malformed → E-COMPONENT-PARSE.
- Commit: `feat(check): component_import DAG resolver + CheckInput.components (dsl §13)`

## Task C3: `::use` validation + reserved directive + component-body check
**Files:** `crates/lute-check/src/check.rs`, `directives.rs`; CLI/LSP call sites.
- Recognize `use` as a reserved built-in directive BEFORE the unknown-directive check (see how `scene`/`cut` or the snapshot lookup is bypassed). For a `::use` directive: read `component=` attr → look up in `ctx`/input components table; unknown → `E-COMPONENT-UNDECLARED`. Validate the remaining attrs as named args: every required param present, no unknown arg key, each value `type_accepts` its param type → else `E-COMPONENT-ARG` (reuse the persist/`@name(args)` value-coercion-by-type helper).
- **Component-body validation:** validate each imported component's body (component mode) — its `@param` refs must resolve to declared params (params act as the ref namespace); directives valid against the snapshot; a state read/write or logic block in a body is the v1 presentational-scope error (E-COMPONENT-BODY or reuse E-UNDECLARED with a clear message). Detect `::use` expansion cycles across components (build the component→uses graph) → `E-COMPONENT-CYCLE`.
- Thread the resolved components through: `check()` consumes `input.components`; CLI (`main.rs`) + LSP (`backend.rs` analyze + imports_for) resolve `components:` from the scene dir via `resolve_components` and pass it in (mirror `resolve_imports` wiring). Add a `divergence_holds_under_components` golden.
- Failing tests: valid `::use` clean; unknown component → E-COMPONENT-UNDECLARED; missing/extra/mistyped arg → E-COMPONENT-ARG; recursive components → E-COMPONENT-CYCLE; a state read in a body → v1 error.
- Commit: `feat(check,cli,lsp): validate ::use component invocations + bodies (dsl §13)`

## Task C4: fixture + showcase + docs sync
**Files:** create `docs/examples/components/{greet.component.lute, scene.lute}`; update `docs/examples/showcase/` (a component + `::use` in episode01 + README); update `docs/superpowers/specs/2026-07-03-components-macros-proposal.md` (mark Option C implemented, link §13); verify spec §13 matches the built behavior.
- Fixture: a `greet.component.lute` (component `greet`, param `who: providerRef` or a simpler `who: str`, presentational body) + a `scene.lute` (`components: [greet.component.lute]`, `::use{component="greet" who=…}`) → `lute check` exit 0.
- Showcase: add a component to `docs/examples/showcase/` and a `::use` in `episode01.lute`; keep episode exit 0; update showcase README feature map.
- Docs sync: proposal doc marked implemented; §13 accurate; README updated.
- Commit: `docs(examples,spec): components fixture + showcase ::use + proposal marked implemented (dsl §13)`

## Verification (controller, after review)
```
cargo test --workspace ; cargo clippy --workspace --all-targets -- -D warnings ; cargo fmt --check
cargo test -p lute-manifest --test tree_sitter_stamp   # unchanged
./target/debug/lute check docs/examples/components/scene.lute --project <if needed>   # exit 0
./target/debug/lute check docs/examples/showcase/episode01.lute --project docs/examples/showcase   # exit 0
```

## Self-Review
- `::use` validated (declared/args/cycle); component files validated (params/presentational); import DAG (dup/cycle/parse) handled; CLI+LSP no divergence.
- NO grammar/snapshot change; capabilityVersion unchanged.
- Docs synced: §13 + proposal + showcase README all reflect the built behavior.
- v1 limitations (presentational bodies; one-per-file; no text interpolation) documented in §13.4 + the proposal.
