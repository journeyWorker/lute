# Lute LSP (Rust) — Design

- **Status:** Approved design; precursor to the implementation plan.
- **Date:** 2026-07-01
- **Sources of truth:** [`docs/proposals/scenario-dsl/0.0.1.md`](../../proposals/scenario-dsl/0.0.1.md),
  [`docs/proposals/plugin-system/0.0.1.md`](../../proposals/plugin-system/0.0.1.md),
  [`docs/architecture.md`](../../architecture.md).
- **Scope of this design:** a Rust implementation of the Lute static-analysis core and its three
  surfaces (headless CLI, editor LSP, CI), plus a tree-sitter grammar for editor-side incremental
  parsing.

---

## 1. Goal

One Rust `check()` core that parses a `.lute` document, validates it against the resolved
capability snapshot + state schema + provider snapshots, and returns a structured `CheckResult`
(byte-span diagnostics + fix-its + a resolved/injection view). Three thin surfaces wrap the same
core: a headless CLI (AI agents / CI) and an editor LSP server. A tree-sitter grammar handles
editor-side highlight/fold/bracket-matching.

## 2. Why Rust (and the tension with `architecture.md`)

`architecture.md` (§"Implementation language", §"First build the daemon, not Rust") recommends a
**TypeScript core + warm daemon first**, reaching for Rust only when measured. That recommendation
assumes an *existing* TS `lute-core`/`harp` to reuse. **No such TS code exists in this repo** — the
project is pre-implementation. Given a greenfield start and that the LSP + tree-sitter + CEL
(`cel-rust`) ecosystem the doc itself names is Rust-native, we build the core in Rust from the
start. This is a deliberate, recorded reversal of the doc's "TS-first" default, justified by the
absence of any TS asset to reuse.

**Non-negotiable carry-overs from the doc (unchanged by the language choice):**

- `check(input) → CheckResult` is the contract; surfaces are thin adapters.
- Two-tier AST (`ParseAst` generic → `CheckedIr` per-tag typed).
- `CelSlot`: every CEL-bearing field is a ranged child node.
- The capability snapshot is the data SoT that parser/checker/LSP all consume; `capabilityVersion`
  stamps every generated artifact.
- The engine never sees the DSL. Final `idola_script_commands` emit is **out of scope** (that is
  the engine-format codegen, engine-owned); our "lowering" stops at the LSP-facing **resolved
  view** (timeline table + injection provenance).

## 3. Architecture

### 3.1 Crate layout (Cargo workspace)

| Crate | Responsibility | Spec anchor |
|---|---|---|
| `lute-core-span` | shared `Span`, `Diagnostic`, `Severity`, `Layer`, `Fixit`, stable-id types | — |
| `lute-manifest` | Type system (§7), manifest schemas (§5–6), resolution (§11), capability snapshot + `capabilityVersion` (§13), provider-snapshot loader (§10), the built-in `lute.core` manifest | plugin §5–14 |
| `lute-syntax` | `ParseAst` types, line-oriented parser (§4.3 precedence), `---` frontmatter peel, `/* */` trivia (§4.2), `CelSlot` skeleton, error recovery | dsl §4–7 |
| `lute-cel` | wraps `cel-parser`; detects `@ref`/`@fn(args)`/`$`; fills `CelSlot.ast`; maps CEL spans into document coordinates | dsl §8 |
| `lute-check` | `check() → CheckResult`; schema binding; definite-assignment (§9.4); `<match>` exhaustiveness (§11.2); `::set` op/type matrix; `@ref`/state-path/choice-id/asset/character resolution; timeline resolver (§11.4); `StageState` injection reducer + provenance; assembles `Resolved` | dsl §9–11, arch "stateful resolution" |
| `lute-cli` | `lute check` (JSON `CheckResult`), `lute catalog refresh`; golden-test harness | arch roadmap #12, plugin §12 |
| `lute-lsp` | `tower-lsp-server` adapter: diagnostics/hover/completion/definition/references/folding/semantic-tokens/document-symbols; `DocumentSnapshot`; byte-for-byte golden vs headless | arch "LSP feature map" |
| `tree-sitter-lute` | `grammar.js` → C; highlight/fold queries; stamped with `capabilityVersion` | arch "Two parsers, one grammar" |

Dependency edges: `manifest`, `syntax` → have no inter-dep; `cel` → `syntax`; `check` →
`manifest`+`syntax`+`cel`; `cli`+`lsp` → `check`; `tree-sitter-lute` is standalone, consumed by
`lsp` at the editor surface. All crates depend on `lute-core-span`.

### 3.2 The `check()` contract

```rust
pub struct CheckInput {
    pub text: String,
    pub uri: String,
    pub snapshot: CapabilitySnapshot,
    pub providers: ProviderSet,
    pub mode: Mode,            // Author | Ci
}

pub struct CheckResult {
    pub ok: bool,
    pub diagnostics: Vec<Diagnostic>,
    pub resolved: Option<Resolved>,   // commands preview, timeline table, injections
}

pub fn check(input: &CheckInput) -> CheckResult;
```

`Diagnostic` carries `code`, `severity`, `message`, `span { byte_start, byte_end, line, column,
utf16_range }`, `layer` (content|staging|logic|cel), `fixits`, optional `provenance`.

**Divergence invariant:** LSP-published diagnostics MUST equal headless CLI diagnostics
byte-for-byte after normalization. Enforced by a golden test comparing both.

### 3.3 Two-tier AST + CelSlot

`ParseAst` mirrors architecture §"AST": `Document { meta, shots }`, `Node = Line | Directive | Set
| Branch | Match | Timeline`, each carrying a `span`. `Directive` stays *generic* (`tag`, `attrs`) so
a new staging verb is schema work, not grammar churn. `CheckedIr` is produced by `lute-check` with
per-tag typed commands.

`CelSlot { kind, raw, ast: Option<CelAst>, span, id }`. `kind ∈ {condition, attr-value, set-expr,
match-subject}`. Invalid CEL leaves `ast: None` + a `layer: cel` diagnostic, isolated from the DSL
tree (error recovery). `cel-parser` 0.10.1 supplies `ast::Ast` with `IdedExpr` + `SourceInfo`/
`OffsetRange`, so CEL sub-nodes get document-relative spans (the CelSlot precondition).

### 3.4 Two parsers, one grammar

The **authoritative** AST comes from the hand-written line-oriented parser in `lute-syntax`
(follows §4.3 classification precedence exactly). `tree-sitter-lute` is **editor-side only**:
highlight, folding, bracket matching on every keystroke — never the authoritative AST. This is the
architecture doc's "two parsers, one grammar" split.

## 4. Tech stack (verified)

- `cel-parser` 0.10.1 (`cel-rust`, ANTLR-based) — parse-only; `ast` module exposes `Ast`,
  `IdedExpr`, `SourceInfo`, `OffsetRange`. Runtime CEL evaluation stays engine-side (`cel-dart`).
- `tower-lsp-server` 0.23.0 (MSRV 1.85; used by Biome/Oxc) — the maintained fork of the dead
  `tower-lsp`. LSP transport over stdio, on `tokio`.
- `serde` + `serde_yaml` — manifest + `---` frontmatter parsing.
- `clap` — CLI.
- `insta` — golden snapshot tests.
- `tree-sitter` — editor-side grammar.
- Rust 1.85+, edition 2021.

## 5. Testing strategy

- **TDD** throughout; **golden tests are mandatory** (plugin §12): every directive gets a golden
  test (`DSL → CheckResult`), or it is behavior, not data.
- **Golden per injection rule** and **per timeline-resolver fixture**.
- **byte-for-byte** headless-vs-LSP diagnostic equivalence golden.
- The two repo examples (`bianca-s01ep02.lute`, `date-minigame.lute`) serve as integration
  fixtures.

## 6. Scope boundary

**In:** parser, CEL integration, checker (all static validation), resolved view (timeline
resolution + injection provenance), headless CLI, LSP server, tree-sitter grammar.

**Out:** final `idola_script_commands` flat-record codegen (engine-format, engine-owned); runtime
CEL evaluation (`cel-dart`); the warm daemon (a later optimization behind the same `check()`).

## 7. Phase map (→ plan skeleton)

| # | Phase | Deliverable |
|---|---|---|
| 0 | scaffold | workspace + `lute-core-span` shared types |
| 1 | manifest | Type §7, schemas §5–6, resolution §11, snapshot + `capabilityVersion` §13, provider loader §10, built-in `lute.core` |
| 2 | syntax | `ParseAst`, line parser §4.3, frontmatter peel, `/* */` trivia, `CelSlot` skeleton, error recovery |
| 3 | cel | `cel-parser` wrap, `@ref`/`$` detection, `CelSlot.ast` fill, span mapping |
| 4 | check | schema binding, def-assignment §9.4, exhaustiveness §11.2, `::set` matrix, ref/asset/char resolution, timeline resolver §11.4, injection reducer + provenance, `CheckResult` assembly |
| 5 | cli | `lute check` (JSON), `lute catalog refresh`, per-directive golden harness |
| 6 | lsp | `tower-lsp-server` adapter, full feature map, `DocumentSnapshot`, headless-vs-LSP golden |
| 7 | tree-sitter | `grammar.js`, highlight/fold queries, `capabilityVersion` stamp |

Phases 1 and 2 are independent (parallelizable); 3 follows 2; 4 follows 1+2+3; 5 and 6 follow 4; 7
is standalone (consumed by 6).
