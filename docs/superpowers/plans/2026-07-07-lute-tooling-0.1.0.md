# Lute 0.1.0 Tooling Completeness Implementation Plan (Plan D of 6)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Bring the editor + AI + CLI tooling up to the shipped 0.1.0 language: complete LSP support for the three constructs it still ignores (`{{…}}` interpolation, `<hub>` recursion holes, `<when is>` patterns), add a `lute context` subcommand (project-resolved authoring surface for AI), add a `lute fix` migration codemod (`:line[`→`:speaker`, choice `as`→`into`), refresh the tree-sitter grammar to 0.1.0, and document the hub/when-is/interp examples.

**Architecture:** Four independent surfaces. LSP (`crates/lute-lsp`) is a pure re-projection over `lute-syntax` + `lute-check` + the resolved `CapabilitySnapshot` — feature gaps are filled in `features/{mod,completion,hover,nav,semtok}.rs`, never by touching the grammar. CLI (`crates/lute-cli`) adds two thin subcommands over existing library APIs (`resolve_document_snapshot`, `lute_check::check`, the `tag.rs` span-rewrite pattern). tree-sitter-lute (`tree-sitter-lute/`) is the Neovim-only editor grammar (hand-written `grammar.js` → generated `parser.c`); the LSP already carries authoritative 0.1.0 semantic-token highlighting, so this is highlighting parity, not a correctness dependency. Docs/examples round it out.

**Tech Stack:** Rust (lute-lsp on tower-lsp-server 0.23; lute-cli on clap; lute-check/lute-manifest libraries); tree-sitter-cli (npm) for the grammar. Spec = `docs/proposals/scenario-dsl/0.1.0.md` + `docs/proposals/plugin-system/0.0.1.md` (capabilityVersion drift guard).

## Global Constraints

- **No divergence:** the LSP owns ZERO validation/completion logic of its own — every answer resolves a byte-offset `Cursor` against the SAME `CapabilitySnapshot`/imports/components/providers the CLI resolves via `lute_manifest::project::resolve_document_snapshot`. New LSP features MUST reuse that shared data path (mod.rs helpers), never a parallel one. `tests/divergence.rs` (the check↔LSP diagnostic golden) MUST stay green.
- **UTF-16 discipline:** every LSP span→range crosses the `position_to_byte`/`span_to_range`/`byte_to_position` bridge (`backend.rs` ~L415-470). Never hand-roll offset math.
- **Byte-fidelity for codemods:** `lute fix` mirrors `tag.rs` exactly — parse, bail unchanged on ANY Error diagnostic, collect target spans, splice back-to-front by descending `Span.byte_start` with `String::insert_str`; comment-blanking is length-preserving so original-source offsets are safe. NEVER rewrite a node the parser couldn't cleanly parse.
- **`as` is NOT globally renamed:** `as=` remains valid as a content-line display-label override (`:bianca{as="???"}:`, DSL §7.1). The `as`→`into` codemod + any tooling MUST scope the rename to `<choice>`/`<hub>`-choice tags ONLY (persist target), never content lines.
- **tree-sitter regen discipline:** after any `grammar.js` edit, `npm run generate` regenerates `src/{parser.c,grammar.json,node-types.json}` — commit them together; reviewers judge `grammar.js`/`queries/*.scm`/`test/corpus/*.txt`, not the generated `parser.c`. The `capabilityVersion` drift-guard stamp in `tree-sitter.json`/`package.json` is a CORE-snapshot content hash (enforced by `crates/lute-manifest/tests/tree_sitter_stamp.rs`) — a grammar-shape-only change does NOT change it; do NOT re-stamp unless `cargo test -p lute-manifest --test tree_sitter_stamp` goes red.
- Run only the touched crate's tests per task; `cargo test --workspace` + `npm test` (tree-sitter) gate the final task of their respective surfaces. Skip project-wide fmt/clippy per task.

## Deferred (out of Plan D — documented, not dropped)

- **Machine-readable diagnostics registry.** Every `E-*`/`W-*` code is an inline string literal across ~14 lute-check modules; the only registry is DSL Appendix E (markdown). A code-level `DiagnosticInfo` catalog is a cross-cutting lute-check refactor out of proportion here. `lute context` (D4) therefore exposes the **authoring surface** (directives/attrs/enums/state/components/capabilities) — what an AI needs to WRITE valid Lute — and DEFERS the diagnostics catalog. (Future plan: a registry that both `lute context` and the LSP can query.)
- **LSP hover/completion for branch/choice/hub-level attrs** (`persist`/`into`/`once`/`exit`/`label`/`id`): a PRE-EXISTING uniform limitation (all resolve to `directive: None`, no capability schema for non-directive constructs) — independent of 0.1.0. D3 adds `is=` value support specifically (its domain IS derivable from the subject); the generic non-directive-attr schema is out of scope.

---

### Task D1: LSP `{{…}}` interpolation support (the flagship gap)

**Files:** Modify `crates/lute-lsp/src/features/mod.rs` (`resolve_line` ~L255, the `Cursor` enum ~L86, ref/path use-collectors), `crates/lute-lsp/src/features/semtok.rs` (`walk_nodes` `Node::Line` arm ~L142-150); Test: unit tests in each + a fixture with interps.

**Interfaces:** Consumes `Line.interps: Vec<Interp>` + `Line.text_span` (lute-syntax ast; `Interp { kind: InterpKind::{Path,Ref,Reserved}, raw, span }`, the interp `span` covers the whole `{{…}}`). Produces: a cursor inside an interp resolves to the same `Cursor` variant a CEL ref/path would (so hover/completion/definition/references reuse the existing handlers). Reuses `lute_syntax::scan_label_interps` is NOT needed here (content-line interps are already on `Line.interps`).

- [ ] **Step 1:** failing tests — (a) hover on `{{run.coins}}` inside a content line returns the state-path decl (type+default), same as hovering `run.coins` in a CEL slot; (b) definition on `{{@fond}}` jumps to the `@fond` def; (c) references on a state path counts a `{{path}}` occurrence; (d) semtok emits `statePath`/`ref` sub-tokens for `{{…}}` interior (not one opaque Content token). Assert against a fixture line `:bianca: Hi {{userName}}, {{run.coins}} — {{@fond}}.`.
- [ ] **Step 2:** run → FAIL (interps invisible today).
- [ ] **Step 3:** in `resolve_line`, after `resolve_attrs`, if the cursor byte falls inside an `Interp.span`, build the appropriate `Cursor` from the interp: `InterpKind::Path` → the state-path cursor (as a CEL path read would), `InterpKind::Ref` → the `@ref` cursor, `InterpKind::Reserved` (`userName`) → a no-op/None (reserved token, always renders — no decl). Add the interp `raw`/inner span so hover/nav resolve the referent (mirror how CEL-slot path/ref cursors carry their name+span). In `semtok.rs` `Node::Line`, instead of one Content token over `text_span`, emit the Content token for the non-interp spans and `ref`/`statePath` sub-tokens for each interp interior (mirror `slot_tokens`' CEL sub-classification). `Reserved` interps stay Content (or a distinct token — keep within the 6-type legend; `statePath`/`ref` for Path/Ref, Content for Reserved).
- [ ] **Step 4:** run → GREEN; `cargo test -p lute-lsp`; confirm `tests/divergence.rs` still green (no diagnostic change).
- [ ] **Step 5:** commit `feat(lsp): hover/def/references/semantic-tokens for {{…}} interpolation`.

---

### Task D2: LSP `<hub>` recursion completeness

**Files:** Modify `crates/lute-lsp/src/features/mod.rs` (`branch_span_nodes` ~L635-655, `collect_set_paths` ~L739-765), `crates/lute-lsp/src/features/completion.rs` (`collect_branch_ids` ~L241-265, `present_attr_keys` ~L267-300); Test: unit tests with a `<hub>` containing a nested `<branch>`/`::set`/directive.

**Interfaces:** Consumes `Node::Hub(Hub { choices, .. })` where each `Choice { body: Vec<Node>, .. }`. Produces: the four helper walks descend into hub choice bodies exactly as they descend into `<branch>` choice bodies (the cursor resolver `resolve_node`/`node_span` ALREADY handle Hub — only these decl-site/use-site helpers miss it).

- [ ] **Step 1:** failing tests — (a) go-to-definition on `scene.choices.<id>` where `<branch id>` is nested inside a `<hub>` choice body resolves; (b) find-references on a state path counts a `::set` nested in a hub choice; (c) `<match on=…>` subject completion offers a `scene.choices.<id>` whose branch is nested in a hub; (d) attr-key completion for a directive inside a hub choice narrows (doesn't re-offer filled keys). 
- [ ] **Step 2:** run → FAIL.
- [ ] **Step 3:** add a `Node::Hub(h) => { for c in &h.choices { recurse(&c.body) } }` arm to each of the four helpers (mirror their existing `Node::Branch` arm). No new logic — just the missing recursion.
- [ ] **Step 4:** run → GREEN; `cargo test -p lute-lsp`.
- [ ] **Step 5:** commit `feat(lsp): descend into <hub> choice bodies for nav/completion helpers`.

---

### Task D3: LSP `<when is>` pattern support

**Files:** Modify `crates/lute-lsp/src/features/mod.rs` (`Arm::When { is: _, .. }` ~L178, the `Cursor` enum), `crates/lute-lsp/src/features/{hover,completion}.rs`; Test: unit tests on an `is=` cursor.

**Interfaces:** Consumes `Arm::When { is: Option<IsPattern>, test, .. }` (`IsPattern { raw, span }` — the `is=` value is a `|`-alternation of literals over the match subject, NOT CEL). The match subject domain (enum members / bool) is derivable from the subject path's declared type via the resolved schema (the same infer used by lute-check). Produces: a cursor on an `is=` value → hover showing the subject's domain; completion offering the subject's enum members / `true`/`false`/`unset`.

- [ ] **Step 1:** failing tests — (a) hover on `<when is="gold">` where the subject `scene.serve.debut.rank` is an enum shows the enum domain; (b) completion inside `is="…"` offers the subject's enum members (+ `unset`); a bool subject offers `true`/`false`/`unset`.
- [ ] **Step 2:** run → FAIL (is discarded today).
- [ ] **Step 3:** stop discarding `is` in the `Arm::When` walk: when the cursor is inside `is.span`, build a new `Cursor::IsPattern { subject_path, span }` (resolve the enclosing `<match on=…>` subject path). Add hover (render the subject domain) + completion (the domain members ∪ `unset`; bool → `true`/`false`/`unset`) for that cursor, sourcing the domain from the resolved state schema (reuse the schema lookup hover.rs already uses for state-path type). Keep it read-only over the shared snapshot.
- [ ] **Step 4:** run → GREEN; `cargo test -p lute-lsp`.
- [ ] **Step 5:** commit `feat(lsp): hover + completion for <when is="…"> literal patterns`.

---

### Task D4: `lute context` subcommand (AI authoring surface)

**Files:** Modify `crates/lute-cli/src/main.rs` (clap `Command` enum ~L42, dispatch ~L113, a new `run_context`); Test: `crates/lute-cli/tests/cli.rs`.

**Interfaces:** `lute context <file> [--project <dir>] [--providers <dir>] [--json]` reuses `build_input()` (main.rs:139) to get the resolved `CapabilitySnapshot` + the `CheckResult`. Produces a JSON authoring surface: `{ capabilityVersion, directives: [{name, layer?, attrs:[{name, type, required, default?}], semantics}], enums: {name: [members]}, stateSchema: [{path, type, default?, domain?}], components: [{name, params}], assetKinds, providers }` — everything an AI needs to author valid Lute against THIS project. Diagnostics registry DEFERRED (see Deferred).

- [ ] **Step 1:** failing test — `lute context docs/examples/showcase/episode01.lute --project docs/examples/showcase --json` → exit 0, JSON with a non-empty `directives` array containing `serve` (with its attrs+types) and the core directives, `enums` including the manifest enums, `stateSchema` with the folded paths, and a `capabilityVersion` equal to the snapshot's.
- [ ] **Step 2:** run → FAIL (no subcommand).
- [ ] **Step 3:** add the `Context { file, project, providers, json }` clap variant + dispatch; `run_context` builds the input, serializes the authoring surface from `snapshot` (`directives`/`enums`/`asset_kinds`/`providers`/`state_shapes`) + the folded state schema from `check()`'s resolved output + components. Serialize deterministically (BTreeMap ordering). Exit 0 on success, 2 on I/O. A human (non-`--json`) mode MAY print a compact outline; `--json` is the machine surface. Reuse existing manifest/serde types; do NOT invent a diagnostics catalog.
- [ ] **Step 4:** run → GREEN; `cargo test -p lute-cli`.
- [ ] **Step 5:** commit `feat(cli): lute context — project-resolved authoring surface (JSON)`.

---

### Task D5: `lute fix` migration codemod

**Files:** Create `crates/lute-check/src/fix.rs` (the span-rewrite, mirroring `tag.rs`); Modify `crates/lute-check/src/lib.rs` (export `fix_document`), `crates/lute-cli/src/main.rs` (a `Fix` subcommand + `run_fix`); Test: `crates/lute-check/src/fix.rs` unit tests + a `crates/lute-cli/tests/cli.rs` e2e.

**Interfaces:** `pub fn fix_document(text: &str) -> FixResult { text: String, changed: usize }` — parse via `lute_syntax::parse`; if ANY Error diagnostic, return unchanged (`changed:0`). Two rewrites, span-driven, back-to-front by descending offset: (1) content lines authored in the removed `:line[speaker]{…}: text` form → `:speaker{…}: text`; (2) `<choice>`/`<hub>`-choice tags with an `as="…"` persist attr → `into="…"`. **Scope (2) to choice/hub tags ONLY** — never content-line `as=`. `lute fix <file>` writes back only when `changed>0`, exit 0.

- [ ] **Step 1:** failing tests (fix.rs) — (a) `:line[bianca]{emotion="x"}: hi` → `:bianca{emotion="x"}: hi`; (b) `<choice id="c" label="L" as="run.flag">` → `<choice id="c" label="L" into="run.flag">`; (c) a content-line `:bianca{as="???"}: hi` is UNCHANGED (as-label override, not a persist target); (d) an already-0.1.0 doc → `changed:0`, byte-identical; (e) a doc with an Error diagnostic → unchanged.
- [ ] **Step 2:** run → FAIL.
- [ ] **Step 3:** implement `fix.rs` mirroring `tag.rs`'s structure (parse → bail-on-error → collect targets with spans → build `Vec<(offset, replacement)>` → splice descending). For (1): the 0.1.0 parser REJECTS `:line[` with an Error diagnostic — so a doc needing fix (1) does NOT cleanly parse. Handle this by detecting the `:line[` form via the parser's fix-it diagnostic OR a lexical pre-scan (the fix-it diagnostic at parser.rs:460 carries the span) and rewriting from THAT, before the clean-parse gate — document the two-phase approach (lexical `:line[`→`:speaker` first, then parse-clean for the `as`→`into` AST rewrite). For (2): walk `<choice>`/`<hub>` choice nodes, find an `as` attr, rewrite its key span `as`→`into`. Add the CLI `Fix { file }` variant + `run_fix`.
- [ ] **Step 4:** run → GREEN; `cargo test -p lute-check -p lute-cli`.
- [ ] **Step 5:** commit `feat(cli,check): lute fix — :line[ →:speaker and choice as→into codemod`.

---

### Task D6: editor examples + docs

**Files:** Create `docs/examples/showcase/<a non-hub when-is example>.lute` (or add a shot to an existing example); Modify `docs/examples/showcase/README.md` (add `hub-demo.lute` to the Layout + feature-map tables); Test: a `lute check` CLI assertion if a new example file is added to a compiled/checked set.

**Interfaces:** Adds a `<when is>` example over a NON-hub finite-enum path (a plain `scene.*`/`run.*` enum), complementing hub-demo's hub-recorded namespaces. README documents hub-demo's feature→location map.

- [ ] **Step 1:** author the non-hub `<when is>` example (a declared enum state path + a `<match>` with `is=` literal arms, exhaustive) and verify `lute check --project docs/examples/showcase` exit 0 on it.
- [ ] **Step 2:** add `hub-demo.lute` + the new example to `docs/examples/showcase/README.md` Layout + feature-map tables (mirror episode01's entries).
- [ ] **Step 3:** if the showcase has a "check every example" CLI test, add the new file; run `cargo test -p lute-cli`.
- [ ] **Step 4:** commit `docs(examples): non-hub <when is> example + hub-demo in showcase README`.

---

### Task D7: tree-sitter grammar — 0.1.0 rules + queries + regen

**Files:** Modify `tree-sitter-lute/grammar.js` (line rule, comment, interpolation, hub, when-is, otherwise), `tree-sitter-lute/queries/highlights.scm` (`:line[`→`:speaker`), `editors/nvim/queries/lute/*.scm` (mirror); regenerate `src/{parser.c,grammar.json,node-types.json}`; Test: representative `test/corpus/*.txt` cases per construct.

**Interfaces:** Grammar parses 0.1.0: `line: seq(":", speaker, attrs?, ":", text?)` (was `:line[speaker]{}:`); `//` line comment added to `extras`/comment; `text` gains internal `{{…}}` interpolation structure (Path|Ref|ReservedToken sub-nodes) with `\{{` escape; `hub`/`hub_choice` rules parallel to `branch`/`choice` (with `once`/`exit` bare-bool attrs) + `hub` added to `_node`; `<when is>` gets an explicit pattern node (not generic-attr fallthrough); `<otherwise>` tightened to zero attrs.

- [ ] **Step 1:** add ONE corpus case per new/changed construct to `test/corpus/*.txt` (0.1.0 content line; `//` comment; `{{…}}` interp; `<hub>`; `<when is>`); run `npm test` → FAIL (grammar can't parse them / old `:line[` cases now wrong).
- [ ] **Step 2:** edit `grammar.js`: rewrite `line`; add `//` line-comment token (line-leading, excluded inside Text/strings — mirror the DSL §4.2 precedence; an external-scanner extension MAY be needed given the line-start + Text-opacity rules, mirroring `scanner.c`'s frontmatter approach); give `text` (+ choice `label` string) `{{…}}` sub-structure; add `hub`/`hub_choice` + `hub` in `_node`; add the `when_pattern`/`is` node; tighten `<otherwise>`. Update `queries/highlights.scm` (`:line[`→`:speaker` at ~L18-20) + sync `editors/nvim/queries/lute/*.scm`.
- [ ] **Step 3:** `npm run generate` (regenerate parser.c/grammar.json/node-types.json); `npm test` → the new cases GREEN.
- [ ] **Step 4:** `cargo test -p lute-manifest --test tree_sitter_stamp` → still green (grammar-shape change does NOT touch the capabilityVersion stamp; confirm, do NOT re-stamp).
- [ ] **Step 5:** commit `feat(tree-sitter): 0.1.0 grammar — :speaker lines, //, {{…}}, <hub>, <when is>`.

---

### Task D8: tree-sitter corpus migration + docs + drift-guard

**Files:** Modify all `tree-sitter-lute/test/corpus/{basic,cel,highlight}.txt` (migrate every remaining 0.0.1 case), `docs/editors/README.md` (remove the deferral note ~L53-56), confirm `editors/nvim/queries` synced; Test: `npm test` full green.

**Interfaces:** Every corpus case uses 0.1.0 syntax; new cases cover choice `into`, `<otherwise>` no-attrs, and any construct D7 added a single case for. The docs no longer say the grammar is deferred/stale.

- [ ] **Step 1:** migrate every remaining `:line[…]` corpus case → `:speaker{…}:`; add cases for choice `into` and `<otherwise>` (no attrs); run `npm test`.
- [ ] **Step 2:** fix any grammar/query fallout until `npm test` is fully green (all 25 migrated + new cases).
- [ ] **Step 3:** remove the "tree-sitter grammar for 0.1.0 is deferred" note in `docs/editors/README.md:53-56`; state the grammar is 0.1.0-current.
- [ ] **Step 4:** `cargo test -p lute-manifest --test tree_sitter_stamp` green; commit `test(tree-sitter): migrate corpus to 0.1.0 + drop deferral note`.

---

## Self-Review (authoring)

1. **Surface coverage:** LSP (D1 interp, D2 hub, D3 when-is — the three mapped gaps), CLI (D4 context, D5 fix), examples/docs (D6), tree-sitter (D7 grammar+queries+regen, D8 corpus+docs). `.omp/lsp.json` needs no change (capabilities are server-side in backend.rs; it only points at the binary). Comment highlighting is D7's job (LSP structurally can't color lexer-stripped comments).
2. **Deferred with rationale:** diagnostics registry (no code-level catalog; D4 does the authoring surface), generic non-directive-attr hover/completion (pre-existing uniform limitation; D3 does `is=` specifically).
3. **Type/interface consistency:** D1-D3 reuse the shared `Cursor`/snapshot data path (no divergence); D4 reuses `build_input`/`resolve_document_snapshot`; D5 reuses `tag.rs`'s span-rewrite shape + scopes `as`→`into` to choice/hub only; D7/D8 keep the capabilityVersion drift-guard green (grammar-shape change doesn't touch it).
