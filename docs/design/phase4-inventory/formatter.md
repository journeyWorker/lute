---
status: Implemented
---

# Phase 4 inventory — formatter

## summary

Lute cannot currently support a true comment-preserving, canonical, idempotent `lute fmt` end-to-end without a lossless source representation. The handwritten parser preserves semantic node spans and the raw frontmatter substring, but strips body comments into a length-preserving scan buffer and drops blank lines/comments from the AST; tree-sitter likewise treats whitespace and comments as extras outside the structural tree. Existing `lute fix`/`lute tag` are safe because they perform targeted source-text edits, not AST printing. Body formatting is feasible only with a new lossless trivia/CST layer or a source-preserving token stream; frontmatter/schema/plugin YAML need a YAML concrete syntax representation if comments, key order, quote/style, and blank-line placement must survive. LSP currently advertises diagnostics, code actions, completion, symbols, folding, navigation, and semantic tokens, but no formatting capability.

## files

```json
[
  {
    "path": "docs/design/architecture-direction.md:87-95",
    "description": "D2 makes `.lute` plus project declarations authoritative source; semantic edits must apply to source text so comments/prose/layout/Git diffs survive."
  },
  {
    "path": "docs/design/architecture-direction.md:187-204",
    "description": "D8 defines existing identity sources and says AI patch targeting uses the documented composite rather than a new nodeId."
  },
  {
    "path": "docs/design/architecture-direction.md:225-240",
    "description": "D10 requires task-scoped context and patches with base revision, target nodes, preserve declarations, atomic stale/ambiguous-target rejection, and semantic-diff enforcement."
  },
  {
    "path": "docs/design/architecture-direction.md:243-255",
    "description": "D11 explicitly requires one canonical, comment-preserving, idempotent `lute fmt` plus formatter idempotence property tests."
  },
  {
    "path": "docs/design/architecture-direction.md:359-362",
    "description": "Phase 4 exit criterion is the 12-task Appendix B suite over the dogfood corpus, recording validity, unintended semantic diff, and preserved-ID changes."
  },
  {
    "path": "reports/lute_report_03_bundle/lute_report_03.md:893-908",
    "description": "Appendix B task list: choice addition, delayed disclosure, optional quest, event duration, component insertion, host-result branching, prose-only rewrite, regional merge, NPC death condition, job restriction, document/scene move, and schedule-consuming action."
  },
  {
    "path": "crates/lute-syntax/src/parser.rs:1-22",
    "description": "Parser pipeline: peel frontmatter, strip body comments once, preserve byte offsets/newlines, then classify non-blank lines and assemble blocks."
  },
  {
    "path": "crates/lute-syntax/src/parser.rs:195-232",
    "description": "`parse` stores only `raw_yaml`, then parses a comment-stripped body slice; AST spans are mapped back to original offsets through the length-preserving blanking."
  },
  {
    "path": "crates/lute-syntax/src/lex.rs:25-55",
    "description": "Frontmatter is peeled as a raw inner YAML string and one envelope span; no YAML CST is produced."
  },
  {
    "path": "crates/lute-syntax/src/lex.rs:136-158",
    "description": "Body comments are pre-parse trivia; scanner rules distinguish comments from strings and opaque content text."
  },
  {
    "path": "crates/lute-syntax/src/lex.rs:240-258",
    "description": "Comment stripping replaces comment bytes with spaces while retaining newlines and offsets; this preserves spans but not comment text in the parser input."
  },
  {
    "path": "crates/lute-syntax/src/ast.rs:1-21",
    "description": "Document AST has Meta.raw_yaml, title, shots, quests, entries, beats, and document span; no trivia/comment or blank-line fields."
  },
  {
    "path": "crates/lute-syntax/src/ast.rs:1071-1118",
    "description": "Attributes retain decoded semantic value, value span, and whole-attribute span; AttrValue is Str, Ref(CelSlot), or BoolTrue, and CelSlot retains raw CEL plus semantic AST handle."
  },
  {
    "path": "crates/lute-syntax/src/parser/attrs.rs:13-25",
    "description": "Attribute scanner is whitespace-tokenized with quoted-value opacity; values are classified as string, `@ref`, or bare boolean."
  },
  {
    "path": "crates/lute-syntax/src/parser/attrs.rs:54-121",
    "description": "Double-quoted values are decoded and retain only inner value/value-span plus whole attribute span; source quote style and exact escape spelling are not represented separately."
  },
  {
    "path": "crates/lute-syntax/src/parser/attrs.rs:123-170",
    "description": "`@ref` attributes retain raw ref text and span as CelSlot; this is semantic/raw-value retention, not a token-level representation."
  },
  {
    "path": "crates/lute-syntax/src/parser/attrs.rs:172-203",
    "description": "Single-quoted attributes are accepted for recovery, converted to Str, and diagnosed as noncanonical; original quote delimiters are not retained in the AST."
  },
  {
    "path": "tree-sitter-lute/grammar.js:24-31",
    "description": "Tree-sitter is editor-side only, not authoritative; whitespace and comments are extras, and frontmatter is an external opaque token."
  },
  {
    "path": "tree-sitter-lute/src/scanner.c:1-12",
    "description": "The external scanner recognizes the whole frontmatter envelope as one opaque token and only locates its boundaries."
  },
  {
    "path": "crates/lute-cli/src/rewrite.rs:125-135",
    "description": "`lute tag` documents idempotent behavior: read source, add only missing codes, write only when changed; force mode intentionally renumbers."
  },
  {
    "path": "crates/lute-cli/src/rewrite.rs:210-211",
    "description": "`lute fix` is documented as span-targeted mechanical migration over `.lute` and YAML, using shared CEL traversal/rewriting."
  },
  {
    "path": "crates/lute-cli/src/rewrite.rs:199-205",
    "description": "Tag writes the transformed source only when additions exist; already-tagged input remains byte-identical."
  },
  {
    "path": "crates/lute-cli/src/rewrite.rs:282-319",
    "description": "CEL migration validates complete slots, finds candidate call spans lexically, selects non-overlapping replacements, and returns edited text; it is not a serializer."
  },
  {
    "path": "crates/lute-cli/src/rewrite.rs:900-917",
    "description": "YAML scalar rewrite decodes a scalar, rewrites only its semantic CEL payload, then re-encodes according to detected quote style; surrounding comments remain because the source string is edited in place."
  },
  {
    "path": "crates/lute-cli/src/rewrite.rs:1014-1027",
    "description": "Existing tests explicitly assert YAML trailing comments survive CEL rewriting and a second rewrite is unchanged."
  },
  {
    "path": "crates/lute-manifest/src/yaml_text.rs:1-8",
    "description": "YAML helper docs state serde_yaml has no per-node positions; helpers recover key/value spans and plain diagnostics from raw text."
  },
  {
    "path": "crates/lute-manifest/src/yaml_text.rs:21-47",
    "description": "`yaml_span` locates semantic YAML nodes through a visitor/error location, but returns spans only; it does not retain comments, key order, style, or trivia."
  },
  {
    "path": "crates/lute-manifest/src/project.rs:177-212",
    "description": "Project manifest is deserialized into typed raw shapes with serde_yaml; plugin options retain semantic mappings, not source formatting."
  },
  {
    "path": "crates/lute-manifest/src/provider.rs:69-97",
    "description": "Provider snapshots load by serde_yaml and discard malformed files; this path is semantic YAML loading, not lossless parsing."
  },
  {
    "path": "crates/lute-manifest/src/provider.rs:196-203",
    "description": "Provider refresh serializes typed snapshots with serde_yaml, demonstrating canonical semantic serialization that necessarily cannot preserve original comments/layout."
  },
  {
    "path": "crates/lute-lsp/src/backend.rs:693-733",
    "description": "LSP capabilities advertise full sync, hover, completion, definition/references, folding, symbols, semantic tokens, and code actions; no document formatting provider is set."
  },
  {
    "path": "crates/lute-model/src/project.rs:59-88",
    "description": "ProjectModel stores parsed AST, folded environment, checks, optional artifact/source map, and reconciled project outputs; it does not store original source text/trivia in ModelDocument."
  },
  {
    "path": "crates/lute-model/src/graph.rs:79-105",
    "description": "SemanticGraph nodes/edges carry semantic keys, files, spans, speakers, reasons, and evidence; graph provenance is semantic dependency provenance, not formatting trivia."
  },
  {
    "path": "crates/lute-model/src/impact.rs:102-133",
    "description": "Impact reports expose semantic links, source files/lines/spans, evidence, and reasons, suitable for task-scoped semantic context but not source reconstruction."
  },
  {
    "path": "docs/examples/marina-s01ep02.lute:1-40",
    "description": "Real corpus sample uses frontmatter comments, inline flow-map syntax, quoted scalar, blank lines, and a multiline body block comment."
  },
  {
    "path": "docs/examples/marina-s01ep02.lute:41-123",
    "description": "Real body sample mixes unindented headings/directives/content, indented nested blocks, inline trailing comments, multiline comments, and differing attribute sets."
  },
  {
    "path": "docs/examples/carry-ep.lute:1-28",
    "description": "Real corpus variation uses zero indentation for nested `<match>/<when>/<otherwise>` and content lines."
  },
  {
    "path": "docs/examples/games/summer-station/scenes/sol/first.lute:1-43",
    "description": "Real game sample uses frontmatter CEL with spaced operators, unindented top-level body, inline one-line attrs, two-space nested blocks, and self-closing directives."
  },
  {
    "path": "docs/examples/games/monster-league/schema/world.schema.yaml:1-103",
    "description": "Real schema sample has extensive comments, aligned inline mappings, flow lists/maps, block lists, quoted labels, and long single-line maps; a formatter must choose whether to normalize all of these."
  },
  {
    "path": "docs/examples/arcia-project/plugins/arcia.minigame/plugin.yaml:1-14",
    "description": "Plugin manifest uses compact inline maps/lists and spaced flow syntax, distinct from the block-heavy schema corpus."
  },
  {
    "path": "docs/examples/arcia-project/plugins/arcia.minigame/directives/minigame.yaml:1-22",
    "description": "Plugin export mixes comments, block mappings, inline maps, arrays, and long nested semantic effect paths."
  }
]
```

## architecture

The current architecture is semantic-AST plus source spans, not lossless CST. The handwritten `lute-syntax` parser is authoritative for checking; tree-sitter-lute is an editor grammar and deliberately floats comments/whitespace as extras. `ProjectModel` adds resolved semantic documents, graph, impact, checks, and optional source maps, but does not add a source-preserving layer. Existing source mutation follows the correct minimal-edit pattern: parse/validate semantic targets, compute byte ranges, splice replacements into original text, and write only if changed. A formatter would need a separate lossless source layer (token/trivia stream or CST) that maps structural nodes and every comment/blank-line region to original spans, plus canonical printers for body, frontmatter YAML, schema YAML, and plugin YAML. Semantic diff/preserve should consume ProjectModel before/after results and existing stable keys/spans; it cannot be derived from a regenerated AST alone.

## report

## Findings by layer

### 1. Body `.lute`: parser and trivia

- The handwritten parser is the semantic authority. Its documented pipeline peels frontmatter, blank-preserves body comments, then classifies lines and recursively assembles blocks (`crates/lute-syntax/src/parser.rs:1-22`).
- `peel_frontmatter` returns the raw YAML interior and a single envelope span (`crates/lute-syntax/src/lex.rs:25-55`). `parse` passes `raw_yaml` into `Meta` and feeds only a stripped body into parsing (`crates/lute-syntax/src/parser.rs:195-232`).
- Body comments are `/* ... */` and line-leading `//`; the scanner intentionally blanks them byte-for-byte except newlines, preserving downstream offsets but eliminating their text from the parsed body (`crates/lute-syntax/src/lex.rs:240-258`). The AST has no comment/trivia/blank-line arrays (`crates/lute-syntax/src/ast.rs:1-21`). Therefore semantic node spans exist, but comment spans are not retained as AST data. The original caller still has the source string, which is why targeted rewrites can preserve comments; a printer receiving only `Document` cannot.
- Blank lines are likewise not represented as nodes. The parser operates on non-blank lines after preprocessing (`crates/lute-syntax/src/parser.rs:1-22`), and the AST only stores structural spans. Exact blank-line count, placement around nested blocks, and whitespace indentation are therefore unavailable to a pure AST printer.
- Tree-sitter does not solve this today: it is explicitly editor-side only, comments and whitespace are `extras`, and comments float outside the structural tree (`tree-sitter-lute/grammar.js:24-31`). It has a concrete parse tree for editor queries, but no Lute formatter currently consumes it, and extras are not modeled as attached trivia. The frontmatter is one opaque external token (`tree-sitter-lute/src/scanner.c:1-12`), not a YAML CST.

### 2. Body attributes, quotes, escapes, and CEL

- The parser recognizes three semantic attribute forms: quoted string, `@ref`/CEL slot, and bare boolean (`crates/lute-syntax/src/parser/attrs.rs:13-25`).
- Strings are decoded into `AttrValue::Str` while retaining a value span and whole-attribute span, but not the original quote kind, whitespace, or escape spelling (`crates/lute-syntax/src/parser/attrs.rs:54-121`; `crates/lute-syntax/src/ast.rs:1071-1118`). Single quotes and curly quotes are recovery cases diagnosed as noncanonical and normalized into semantic strings (`crates/lute-syntax/src/parser/attrs.rs:172-203`). A formatter can choose a canonical double-quote/escape policy, but cannot preserve an author's original quote style from the AST.
- `@ref` values retain raw text and spans as CEL slots (`crates/lute-syntax/src/parser/attrs.rs:123-170`). CEL slots carry raw expressions and later AST handles (`crates/lute-syntax/src/ast.rs:1110-1118`), but there is no general body printer and no preservation of original CEL whitespace/comments. `lute fix` only rewrites selected CEL constructs and preserves all unrelated source text (`crates/lute-cli/src/rewrite.rs:282-319`).
- Canonical choices that are not currently specified include indentation width/indentation of nested blocks, whether closing tags align with opens, attribute order, spacing inside `{...}`, double-quote policy, quote escaping, whether compact flow-style is allowed in tags, spacing around CEL operators, and placement/number of blank lines. Existing examples prove these are materially variable: `docs/examples/carry-ep.lute:9-28` has fully unindented nested match content, `docs/examples/games/summer-station/scenes/sol/first.lute:20-43` uses two-space nested blocks, and `docs/examples/marina-s01ep02.lute:41-123` mixes block comments, inline comments, blank lines, and indented nested constructs.

### 3. Frontmatter YAML

- Frontmatter is parsed separately from body at the envelope boundary and handed verbatim to semantic YAML checking (`crates/lute-syntax/src/parser.rs:195-232`; `crates/lute-syntax/src/lex.rs:25-55`). YAML `#` comments are therefore not mistaken for body comments, and the raw YAML text is available while parsing that document.
- However, all typed consumers use `serde_yaml` values/mappings. The repository explicitly documents that serde_yaml has no per-node positions; `yaml_text` reconstructs key/value spans using a visitor and raw text (`crates/lute-manifest/src/yaml_text.rs:1-8`, `:21-47`). The AST does not preserve YAML comments or a YAML CST; `Meta.raw_yaml` is just an opaque string (`crates/lute-syntax/src/ast.rs:17-21`).
- YAML semantics preserve mapping content and often order through concrete Rust map choices, but this does not preserve original key order/style/comments. A semantic serializer would select its own order, scalar quoting, flow/block form, indentation, and line wrapping. Existing YAML already varies between aligned flow values and block values (`docs/examples/marina-s01ep02.lute:1-25`, `docs/examples/games/summer-station/scenes/sol/first.lute:1-14`, `docs/examples/games/monster-league/schema/world.schema.yaml:1-103`).
- Feasibility: a frontmatter-only formatter can preserve comments today by scalar/path span splicing, as `lute fix` does for CEL (`crates/lute-cli/src/rewrite.rs:900-917`, tests at `:1014-1027`), but cannot safely canonicalize whole-map order/layout while guaranteeing comments remain attached. Full comment-preserving canonical frontmatter formatting requires a YAML CST/token layer or an explicit preservation/attachment algorithm.

### 4. Schema/plugin YAML

- Schema, manifest, plugin export, and provider YAML are all semantic serde_yaml surfaces. Plugin manifests deserialize into typed raw structures (`crates/lute-manifest/src/project.rs:177-212`); provider snapshots deserialize and are reserialized on refresh (`crates/lute-manifest/src/provider.rs:69-97`, `:196-203`).
- The loader has source-text helpers for diagnostics and spans, not lossless editing (`crates/lute-manifest/src/yaml_text.rs:1-47`). Duplicate keys are especially problematic: semantic `serde_yaml::Value::Mapping` collapses duplicates, while the repository notes raw-text scanning is needed for authoritative occurrence handling (`crates/lute-manifest/src/relations.rs:108-112`).
- Feasibility: semantic validity/checking is strong; comment-preserving canonical formatting of schema/plugin YAML is not available through the current representation. A narrow scalar rewriter is feasible and already demonstrated; a whole-file formatter needs YAML CST support and a policy for comments attached to keys, sequence items, flow collections, duplicate keys, anchors/aliases if admitted, and quoted/plain scalar preservation. Provider refresh is explicitly a serializer and therefore not comment-preserving (`crates/lute-manifest/src/provider.rs:196-203`).

### 5. Existing printers/serializers and how they edit

- There is no discovered canonical `.lute` AST printer/serializer in the inspected crates. The source-mutating commands are `lute fix` and `lute tag`, and both operate on original source text. `lute tag` writes only when adding/renumbering codes (`crates/lute-cli/src/rewrite.rs:125-135`, `:199-205`).
- `lute fix` combines `lute_check::fix_document` with selected CEL rewrites and writes only if the computed text differs (`crates/lute-cli/src/rewrite.rs:210-211`, `:282-319`). The YAML path decodes only a target scalar and re-encodes that scalar, leaving surrounding comments and layout in the original string (`crates/lute-cli/src/rewrite.rs:900-917`). This is span/scalar splicing, not full serialization.
- `serde_yaml::to_string` is used where canonical semantic output is intended, e.g. provider refresh (`crates/lute-manifest/src/provider.rs:196-203`) and catalog refresh (`crates/lute-cli/src/cmd_catalog.rs:75-90` from the repository search). Those outputs are not evidence of source-preserving format support.
- Existing idempotence is local to migrations: tests assert a second CEL rewrite is unchanged and comments survive (`crates/lute-cli/src/rewrite.rs:1014-1027`). No equivalent `fmt` command, canonical body printer, or whole-project formatting contract was found in the CLI command inventory (`crates/lute-cli/src/cli.rs:238-258` documents `tag` and `fix`, but no `fmt`).

### 6. Canonical-form questions exposed by the dogfood corpus

Observed variation that a formatter contract must settle, rather than infer:

- Indentation: both zero-indented nested logic (`docs/examples/carry-ep.lute:9-28`) and two-space nested logic (`docs/examples/games/summer-station/scenes/sol/first.lute:20-43`) exist.
- Blank lines: examples place blank lines between frontmatter, headings, directives, prose, and blocks, but there is no AST field to distinguish intentional paragraph rhythm from incidental whitespace (`docs/examples/marina-s01ep02.lute:1-123`).
- Comments: frontmatter YAML comments, standalone/multiline body comments, inline trailing comments, and comments embedded beside aligned YAML values all occur (`docs/examples/marina-s01ep02.lute:1-40`, `:41-123`; `docs/examples/games/monster-league/schema/world.schema.yaml:1-103`).
- Attribute order and quoting: real lines put `code`, `emotion`, `variant`, or staging attrs in varying orders; semantic AST uses `Vec<Attr>` in authored order but a canonical policy has not been specified (`crates/lute-syntax/src/ast.rs:1071-1087`; `docs/examples/marina-s01ep02.lute:41-123`; `docs/examples/games/summer-station/scenes/sol/first.lute:20-43`).
- CEL spacing and quoting: frontmatter conditions use strings such as `"holds('at', ['sol', 'radio'])"`, while body `::set` uses spaced assignment syntax; `lute fix` itself chooses quote styles based on context and target scalar, not a global printer policy (`docs/examples/games/summer-station/scenes/sol/first.lute:1-14`; `crates/lute-cli/src/rewrite.rs:900-917`).
- YAML key order/style: schema uses aligned inline maps, flow sequences, block sequences, long flow maps, and extensive explanatory comments (`docs/examples/games/monster-league/schema/world.schema.yaml:1-103`); plugin files use another mixture of compact flow forms and block forms (`docs/examples/arcia-project/plugins/arcia.minigame/plugin.yaml:3-14`; `docs/examples/arcia-project/plugins/arcia.minigame/directives/minigame.yaml:1-22`). There is no specified canonical key order for frontmatter, schemas, or plugin exports.
- Document structure: prose-only source, top-level directives, and nested blocks vary in indentation and compactness; tree-sitter's grammar recognizes structure but does not retain formatter-owned trivia attachments (`tree-sitter-lute/grammar.js:24-31`).

### 7. LSP formatting status

The LSP initialize result sets full document sync and advertises hover, completion, definitions, references, folding, symbols, semantic tokens, and code actions (`crates/lute-lsp/src/backend.rs:693-733`). It does not set `document_formatting_provider` or `document_range_formatting_provider`; no formatting handler was found in the inspected backend feature set. Therefore editor formatting is currently unavailable as an LSP capability. Code actions can apply diagnostic fixits, but that is not general formatting (`crates/lute-lsp/src/backend.rs:731-733`).

### 8. Per-layer feasibility and missing pieces

| Layer | Current feasibility | Missing for required `fmt` contract |
|---|---|---|
| Body `.lute` | Semantic validation and targeted source edits are feasible now; pure AST reprinting is not comment/blank-line preserving because trivia is dropped (`crates/lute-syntax/src/parser.rs:1-22`; `crates/lute-syntax/src/ast.rs:1-21`). | Lossless token/trivia/CST representation; canonical indentation/blank-line/attribute-order/quoting/CEL policy; printer tests for comments and idempotence; source-to-node mapping for semantic patching. |
| Document frontmatter | Raw YAML is retained per document and scalar/path splicing works for narrow edits (`crates/lute-syntax/src/ast.rs:17-21`; `crates/lute-cli/src/rewrite.rs:900-917`). | YAML CST/token retention for comments, key order, scalar style, flow/block style and blank lines; canonical key-order and quote policy; safe attachment of comments to moved keys. |
| Schema YAML | Typed semantic parse and raw-text diagnostic spans work (`crates/lute-manifest/src/yaml_text.rs:1-47`). | Lossless YAML representation and formatter policy for declarations, comments, duplicate keys, flow/block styles, list layout and ordering. |
| Plugin YAML | Semantic loading/validation and generated serialization work (`crates/lute-manifest/src/project.rs:177-212`; `crates/lute-manifest/src/provider.rs:196-203`). | Same YAML CST/trivia layer plus plugin-specific ordering policy; preservation around nested effect declarations and inline maps/lists. |
| Project semantic model / graph / impact | Strong substrate for semantic diff, task context, affected-node computation, and preserve checks: ProjectModel has folded/check/artifact/source-map data (`crates/lute-model/src/project.rs:59-88`), graph has semantic nodes/edges/provenance (`crates/lute-model/src/graph.rs:79-105`), impact has evidence and source-span explanations (`crates/lute-model/src/impact.rs:102-133`). | A before/after API that compares model snapshots and emits explicit unintended semantic changes/preserved-ID changes; source revision/hash in patch protocol; node identity coverage for all editable node kinds; integration with formatter output and stale/ambiguous target rejection. |
| LSP | Diagnostics and fixit code actions exist; formatting is not advertised or implemented (`crates/lute-lsp/src/backend.rs:693-733`). | Formatting request handler, capability advertisement, range/full-document edit generation, and version-aware application; these depend on a lossless formatter core.

### Gaps / risks

- No lossless CST/trivia attachment exists in either current parser path: body comments/blank lines are discarded by `lute-syntax`, and tree-sitter extras are not formatter-owned (`crates/lute-syntax/src/lex.rs:240-258`; `tree-sitter-lute/grammar.js:24-31`).
- YAML comments/order/style are not retained by serde_yaml; raw text is available for some document frontmatter but not as structured editable YAML (`crates/lute-manifest/src/yaml_text.rs:1-47`).
- Canonical formatting policy is unspecified for indentation, blank lines, attribute order, quote/escape normalization, CEL spacing, and YAML ordering/style; the corpus contains conflicting conventions (`docs/examples/carry-ep.lute:9-28`; `docs/examples/games/summer-station/scenes/sol/first.lute:20-43`; `docs/examples/games/monster-league/schema/world.schema.yaml:1-103`).
- A formatter that regenerates from semantic AST/serde values would necessarily risk comment loss, quote/style churn, YAML comment misattachment, duplicate-key collapse, and accidental semantic changes; existing serializers demonstrate this distinction (`crates/lute-manifest/src/provider.rs:196-203`).
- LSP has no formatting capability today (`crates/lute-lsp/src/backend.rs:693-733`).
- The semantic model has graph spans and source maps, but no explicit source-revision or original-trivia contract in the public structures shown (`crates/lute-model/src/project.rs:59-88`; `crates/lute-model/src/graph.rs:79-105`).