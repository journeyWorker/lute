# Lute §6.9 assetKinds (checker/LSP) + Heavy Checker Precision — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Make an authored `assetId` a *structured, validated* value (plugin §6.9): a plugin declares an `assetKind`'s segment schema, and the checker **decomposes + validates** an authored id against it (per-segment type check, `providerRef` existence vs the pinned snapshot), while the LSP offers **per-segment** completion/hover/go-to-def — replacing today's opaque-`string` `assetId`. Then two independent checker-precision features (heavy-#6): **E-WRITE-CONFLICT** property-level precision and **E-REF-TYPE** (`@ref` type-context match).

**Architecture:** `assetId` today is a plain `Type::string` attribute the checker accepts opaquely (with the `PLACEHOLDER_*` escape). This plan adds an `AssetKind` capability datum (segment schema, resolve mode, provider binding, persistence) to the snapshot, a pure `asset` module (decompose/validate a serialized id ↔ typed segments), and wires it into the directive checker + LSP. **Scope boundary (normative, from the plan's Global Constraints):** producing the final id — compose-from-attrs, query-from-attrs matching, fallback-hook *resolution*, `canonicalAssetId` redirect — is **compiler/engine codegen** and is OUT of scope; the static tool **validates authored ids** and **assists authoring**. Where §6.9 describes engine behavior, this plan implements only the checker/LSP-observable half and documents the engine half as deferred.

**Tech Stack:** Rust (workspace, rustc 1.96.1), `serde`/`serde_yaml` 0.9, `sha2` (capability hash), `insta` (goldens), `tower-lsp-server` 0.23.0. tree-sitter unaffected (asset ids are attribute *values*, not grammar).

## Global Constraints

- **rustup stable 1.96.1** via `~/.cargo/bin`. Every fresh shell: `export PATH="$HOME/.cargo/bin:$PATH"`. NEVER `brew install rust`.
- **Worktree authoritative.** Work in `/Users/journey/Workspace/lute/.worktrees/lute-lsp-rust` on `feat/lute-lsp-rust` (currently == `main`). **HARNESS QUIRK:** `write`/`edit` resolve RELATIVE paths against the MAIN workspace (`~/Workspace/lute`), NOT the worktree — always use ABSOLUTE worktree paths; after every commit verify `git status` clean in BOTH trees.
- **TDD, tester-first.** Failing test first, confirm the exact failure, minimal impl, green. Own-crate tests during a task; full-workspace gate at phase/plan end.
- **Format touched crates** (`cargo fmt -p <crate>`); keep `cargo fmt --check` clean. **Clippy `-D warnings`** must pass per touched crate (the last wave learned that per-task clippy deferral bites at the gate — run `cargo clippy -p <crate> --all-targets -- -D warnings` before each commit).
- **Cross-crate discipline** (the 2.1 lesson): a change to a public `lute-manifest`/`lute-check` type can break an exhaustive `match` in a downstream crate that own-crate tests miss. For any public enum/type/signature change, grep + `cargo build`/`cargo test` the dependent crates (`lute-check`, `lute-cli`, `lute-lsp`) before committing.
- **Snapshot is SoT** (inviolable): no hardcoded asset vocabulary in the checker/LSP; `AssetKind`s flow from the assembled `CapabilitySnapshot`. Baseline `lute.core` ships NO assetKinds, so core-only behavior is unchanged (authored `assetId` stays an opaque string with the `PLACEHOLDER_*` exemption).
- **No divergence** (inviolable): any asset validation runs inside the single `check()` and reports through the one position path; the LSP asset assist reuses the same decompose/segment enumerators — never a second implementation.
- **Determinism** (§3.2) + **never-panic**: `asset` decompose/validate is total (a malformed id yields diagnostics, never a panic); all maps `BTreeMap`/sorted.
- **`capabilityVersion` completeness** (§13): the new `asset_kinds` snapshot field MUST be folded into `capability_version()` under its own section marker, with a determinism/drift test. The tree-sitter drift-guard test (`tree_sitter_stamp.rs`) will then require a re-stamp — do it (re-stamp both JSON files to the new core version) as part of the task that adds the field, so the guard stays green.
- **Backwards-compatible core**: `assetId` remains a valid attribute type. When a directive's `assetId` attr is typed `{ assetKind: <kind> }` (new) the checker validates structurally; when it's plain `string` (core today) behavior is byte-identical.

## Spec source-of-truth

- Plugin spec §6.9 (assetKinds), §7 (Type), §10 (provider snapshot), §13 (capabilityVersion): `docs/proposals/plugin-system/0.0.1.md`.
- Language spec §7.5 (timing attrs), §11.4 (timeline): `docs/proposals/scenario-dsl/0.0.1.md`.

## Scope boundary — what this plan does and does NOT build

**IN (static checker + LSP):**
- `AssetKind` schema + snapshot field + loader (`assetkinds/*.yaml` export) + hash fold.
- A new `Type::AssetKind(String)` attribute type so a directive's `assetId` attr can be typed to a kind.
- `asset::decompose(kind, id) -> Result<Vec<Segment>, DecomposeError>` + `asset::validate_segments(kind, segments, providers) -> Vec<AssetDiag>` (const match, `providerRef` existence vs snapshot, enum/number/string per segment).
- Checker: an authored `assetId` against an `assetKind`-typed attr → decompose + validate; `PLACEHOLDER_*`/opaque-legacy exemption (warning for `PLACEHOLDER_*`); unresolved segment → `E-ASSET-*` diagnostics. Query-kind authored id → provider-existence only.
- LSP: per-segment completion (enum members / provider ids), hover (segment name+type), go-to-def (segment `providerRef` → its provider decl / none) on an authored `assetId` value.

**OUT (compiler/engine codegen — deferred, documented, NOT implemented):**
- compose-from-attrs (segments → id) and query-from-attrs matching (`mood → bgm`) — the engine produces the final id; the checker only validates an *authored* id.
- fallback-hook *resolution* + offline candidate generation (needs the full asset catalog; a closed core hook registry) — engine-owned.
- `canonicalAssetId` redirect table — engine data.
- These are recorded in a "Deferred (engine)" section; a directive with NO authored `assetId` and a query/compose kind is NOT an error here (the engine resolves it) unless the spec/example requires otherwise.

## File Structure

**Phase A1–A2 (manifest schema + snapshot), `crates/lute-manifest/src/`:**
- `types.rs` — add `Type::AssetKind(String)` variant (attribute-position type naming a kind) + serde (`{ assetKind: <name> }`) + `type_label`/`describe` arms across crates.
- `schema.rs` — `AssetKindsFile`, `AssetKindDecl { kind, sep, resolve, segments, provider, match_, aliases, fallback, persistence }`, `AssetSegment` (`{ name, const?, type? }`), `AssetMatch`, enums for resolve mode.
- `snapshot.rs` — `CapabilitySnapshot.asset_kinds: BTreeMap<String, AssetKindDecl>`; fold into `capability_version` under a `assetKinds` marker.
- `loader.rs` — `assetkinds` export no longer skipped: read `assetkinds/*.yaml` → `AssetKindDecl`s (dup-kind reject).
- `assemble.rs` — merge `asset_kinds` (cross-plugin dup reject) like other kinds.
- `tree-sitter-lute/{tree-sitter,package}.json` — re-stamp capabilityVersion (the fold changes the core version); `tree_sitter_stamp.rs` guard enforces it.

**Phase A3 (asset core), `crates/lute-manifest/src/`:**
- `asset.rs` — **NEW.** `Segment { name, value }`, `DecomposeError`, `decompose(&AssetKindDecl, &str) -> Result<Vec<Segment>, DecomposeError>`, `validate_segments(&AssetKindDecl, &[Segment], &ProviderSet) -> Vec<AssetIssue>`, `is_placeholder(&str) -> bool`. Pure, deterministic, total.

**Phase A4 (checker integration), `crates/lute-check/src/`:**
- `directives.rs` — `check_attr_value`'s `Type::AssetKind(kind)` arm: resolve the kind from the snapshot; `PLACEHOLDER_*` → `W-ASSET-PLACEHOLDER`; opaque-legacy (no `sep`/undecomposable + not matching segment shape) → exempt per §6.9 step-1; else decompose + `validate_segments` → `E-ASSET-SEGMENT`/`E-ASSET-UNKNOWN-ID`/`E-ASSET-ARITY`.
- `check.rs` — thread `providers` into the asset check (already available on `Walker`).

**Phase A5 (LSP asset assist), `crates/lute-lsp/src/features/`:**
- `mod.rs`/`completion.rs`/`hover.rs`/`nav.rs` — cursor resolver recognizes an `assetId` attr value on an `assetKind`-typed attr; decompose up to the cursor segment; complete/hover/go-to-def that segment against its declared type + provider snapshot.

**Phase B (heavy-#6, independent — candidate for its own plan):**
- B1 `crates/lute-check/src/timeline.rs` + `check.rs` — thread directive `writes[]` + snapshot into `resolve_timeline`; property-level E-WRITE-CONFLICT.
- B2 `crates/lute-check/src/{ctx.rs,cel_resolve.rs}` — design an expected-type-per-CEL-slot model; `@ref` type-context match → `E-REF-TYPE`.

---

## Phase A1 — Manifest: `AssetKind` schema + `Type::AssetKind`

### Task A1.1: `Type::AssetKind` attribute type

**Files:** `crates/lute-manifest/src/types.rs` (+ downstream `type_label`/`describe`); Test: `types.rs` tests.

**Interfaces:**
- Produces: `Type::AssetKind(String)` — an attribute-position type; serde wire form `{ assetKind: <name> }` (camelCase, matching the existing `TypeDef` pattern for `providerRef`/`slotId`). `type_accepts(Type::AssetKind(_), Literal::Str(_)) => true` (an authored id is a string at the literal level; structural decompose/validate is the checker's job, like `slotId`).

- [ ] **Step 1: failing test** — `type_accepts_assetkind` (a `Type::AssetKind("CH".into())` accepts a `Literal::Str`, rejects non-Str); `assetkind_wire_roundtrip` (`{ assetKind: CH }` deserializes to `Type::AssetKind("CH")` and re-serializes identically). Run `cargo test -p lute-manifest --lib assetkind` → FAIL (variant missing).
- [ ] **Step 2: implement** — add `AssetKind(String)` to `Type`; mirror the `ProviderRef`/`SlotId` handling in the `TypeDef` shadow enum (serde) and the `From` impls; add `(Type::AssetKind(_), Literal::Str(_)) => true` to `type_accepts`.
- [ ] **Step 3: downstream `type_label`/`describe`** — grep `Type::` matches in `crates/lute-lsp/src/features/mod.rs` (`type_label`) and `crates/lute-check/src/directives.rs` (`describe`); add the `AssetKind(k) => "assetKind(<k>)"`-style arms (exhaustive-match compile guard). Run `cargo build -p lute-check -p lute-lsp`.
- [ ] **Step 4: green + commit** — `cargo test -p lute-manifest` green; `cargo build -p lute-check -p lute-lsp` clean; `cargo fmt -p lute-manifest` ; clippy clean. Commit `feat(manifest): Type::AssetKind attribute type (plugin §6.9/§7)`.

### Task A1.2: `AssetKindDecl` schema + `AssetKindsFile`

**Files:** `crates/lute-manifest/src/schema.rs`; Test: `schema.rs` tests.

**Interfaces:**
- Produces:
  ```rust
  #[derive(Clone, Debug, Serialize, Deserialize)]
  #[serde(rename_all = "camelCase")]
  pub struct AssetKindDecl {
      pub kind: String,
      #[serde(default = "default_sep")] pub sep: String,           // default "."
      #[serde(default)] pub resolve: AssetResolve,                  // compose (default) | query
      #[serde(default)] pub segments: Vec<AssetSegment>,
      #[serde(default, skip_serializing_if = "Option::is_none")] pub provider: Option<String>,  // query mode
      #[serde(default, rename = "match")] pub match_: Vec<AssetMatch>,
      #[serde(default)] pub aliases: BTreeMap<String, String>,
      #[serde(default)] pub fallback: Vec<String>,                 // named hooks (engine-resolved; recorded only)
      #[serde(default, skip_serializing_if = "Option::is_none")] pub persistence: Option<String>,
  }
  #[serde(rename_all="camelCase")] pub enum AssetResolve { #[default] Compose, Query }
  pub struct AssetSegment { pub name: String, pub r#const: Option<String>, #[serde(rename="type")] pub ty: Option<Type> }  // const XOR type
  pub struct AssetMatch { pub attr: String, pub field: String, pub via: Option<String> }
  pub struct AssetKindsFile { pub asset_kinds: Vec<AssetKindDecl> }
  ```
- [ ] **Step 1: failing test** — `asset_kind_decl_parses_ch` (the §6.9 `CH` compose example: prefix const + characterId providerRef + costume string + emotion enum + variant number) and `asset_kind_decl_parses_bg_query` (the `BG` query example: resolve query, provider, match with `via`, aliases, fallback). Assert fields bind. Run → FAIL (types missing).
- [ ] **Step 2: implement** the structs above + `default_sep`.
- [ ] **Step 3: green + commit** — `cargo test -p lute-manifest` green; fmt+clippy. Commit `feat(manifest): AssetKindDecl schema (plugin §6.9)`.

## Phase A2 — Snapshot field + loader + assemble + capabilityVersion

### Task A2.1: `CapabilitySnapshot.asset_kinds` + hash fold + tree-sitter re-stamp

**Files:** `snapshot.rs` (field + `capability_version`), `tree-sitter-lute/*.json` (re-stamp); Test: `snapshot.rs` tests + the existing `tree_sitter_stamp.rs`.

**Interfaces:** `CapabilitySnapshot.asset_kinds: BTreeMap<String, AssetKindDecl>`; folded into `capability_version` under a new `assetKinds` section marker (Debug-fold like the other maps). NOT: `asset_kinds` is resolved capability surface → MUST be hashed.

- [ ] **Step 1: failing test** — `asset_kinds_change_the_version` (a snapshot with an AssetKindDecl hashes differently from empty). Run → FAIL (field missing).
- [ ] **Step 2: implement** — add the field (Default = empty BTreeMap); fold into `capability_version` after the `state_templates` section, distinct marker.
- [ ] **Step 3: re-stamp tree-sitter** — the fold changes the core version; `cargo test -p lute-manifest --test tree_sitter_stamp` now FAILS. Compute the new `load_core_snapshot().version` (a throwaway probe or read the assertion's actual-vs-expected) and re-stamp both `tree-sitter-lute/tree-sitter.json` + `package.json` `metadata.capabilityVersion` to it. Re-run → green (the guard's live-computed expectation now matches).
- [ ] **Step 4: green + commit** — `cargo test -p lute-manifest` green (incl. drift guard); `cargo build -p lute-check -p lute-lsp` (additive field, `..Default::default()` sites unaffected — verify). fmt+clippy. Commit `feat(manifest): snapshot asset_kinds (hashed) + tree-sitter re-stamp (plugin §6.9/§13)`.

### Task A2.2: loader reads `assetkinds/` + assemble merges

**Files:** `loader.rs` (the `"assetkinds"` arm, currently ignored), `assemble.rs` (merge + dup reject); Test: `tests/loader.rs` + `tests/assemble.rs`.

**Interfaces:** loader: `"assetkinds" => read_kind::<AssetKindsFile,_>(... merge by `kind`, dup → LoadError::DuplicateId{kind:"assetKind"})`. assemble: merge `asset_kinds` into the snapshot (cross-plugin dup → `AssembleError::DuplicateAcrossPlugins{kind:"assetKind"}`). The unknown-export guard (fix #3-era `UnknownExport`) already lists `assetkinds` in the closed set — confirm it stays known.

- [ ] **Step 1: failing tests** — `loads_asset_kinds` (a package with `assetkinds/ch.yaml` → the CH kind in `LoadedPlugin.asset_kinds`; dup kind → DuplicateId) and `assemble_merges_asset_kinds` (an active plugin's CH kind lands in `snap.asset_kinds`; cross-plugin dup → error). Run → FAIL.
- [ ] **Step 2: implement** — add `asset_kinds: Vec<AssetKindDecl>` to `LoadedPlugin`; loader `"assetkinds"` arm reads+merges (dup reject); assemble merges into `snap.asset_kinds`. Update `LoadedPlugin` construction sites (grep) + any exhaustive match.
- [ ] **Step 3: green + commit** — `cargo test -p lute-manifest` green. fmt+clippy. Commit `feat(manifest): load + assemble assetKinds (plugin §4/§6.9)`.

## Phase A3 — Asset core: decompose + validate (pure, total)

### Task A3.1: `asset::decompose` + `is_placeholder`

**Files:** `crates/lute-manifest/src/asset.rs` (NEW) + `lib.rs` (`pub mod asset;`); Test: `asset.rs` tests.

**Interfaces:**
- Produces:
  ```rust
  pub struct Segment { pub name: String, pub value: String }
  pub enum DecomposeError { Arity { expected: usize, found: usize }, ConstMismatch { name: String, expected: String, found: String } }
  /// Split `id` by the kind's `sep` and zip with the kind's `segments`; a const
  /// segment must match verbatim. Returns typed Segments (name↔value) or an arity/
  /// const error. Segment-less (pure query) kinds decompose to a single opaque value.
  pub fn decompose(kind: &AssetKindDecl, id: &str) -> Result<Vec<Segment>, DecomposeError>;
  /// True for a `PLACEHOLDER_*` sentinel (§6.9 exemption).
  pub fn is_placeholder(id: &str) -> bool; // id.starts_with("PLACEHOLDER_")
  ```
- [ ] **Step 1: failing tests** — decompose `CH.marina.waitress.delighted.1` against the CH kind → 5 segments with the right names/values; arity mismatch (too few/many parts) → `Arity`; const mismatch (`XX.marina...`) → `ConstMismatch`; `is_placeholder("PLACEHOLDER_x")` true, `is_placeholder("BG.a.b")` false; a query kind with no segments → one opaque segment. Run → FAIL (module missing).
- [ ] **Step 2: implement** — split on `sep`, zip with `segments`, check `const`, build `Segment`s; total + deterministic. Add `pub mod asset;`.
- [ ] **Step 3: green + commit** — `cargo test -p lute-manifest` green. fmt+clippy. Commit `feat(manifest): asset::decompose + is_placeholder (plugin §6.9)`.

### Task A3.2: `asset::validate_segments`

**Files:** `asset.rs`; Test: `asset.rs` tests.

**Interfaces:**
- Produces:
  ```rust
  pub enum AssetIssue {
      BadConst { segment: String, expected: String, found: String },   // (redundant with decompose; validate is the single entry that also re-checks)
      NotEnumMember { segment: String, value: String, members: Vec<String> },
      NotNumber { segment: String, value: String },
      UnknownProviderId { segment: String, provider: String, value: String },  // Absent
      StaleProviderId { segment: String, provider: String, value: String },    // Stale (catalog offline) — Warning-worthy
  }
  /// Validate each decomposed Segment against its declared segment type: const
  /// (verbatim), enum (membership), number (parse), string (any), providerRef
  /// (existence vs the ProviderSet — Fresh ok / Stale→StaleProviderId / Absent→
  /// UnknownProviderId). Deterministic; never panics.
  pub fn validate_segments(kind: &AssetKindDecl, segs: &[Segment], providers: &ProviderSet) -> Vec<AssetIssue>;
  ```
- [ ] **Step 1: failing tests** — a CH id whose `characterId` segment (providerRef `character`) is Absent in the ProviderSet → `UnknownProviderId`; a bad `emotion` enum value → `NotEnumMember`; a non-numeric `variant` → `NotNumber`; a Stale provider → `StaleProviderId`; a fully-valid id → `[]`. Run → FAIL.
- [ ] **Step 2: implement** — per-segment match on the segment's `const`/`type`; providerRef via `ProviderSet::contains` → map IdStatus to issue. Run.
- [ ] **Step 3: green + commit** — `cargo test -p lute-manifest` green. fmt+clippy. Commit `feat(manifest): asset::validate_segments (plugin §6.9/§10)`.

## Phase A4 — Checker integration: validate authored `assetId`

### Task A4.1: `check_attr_value` AssetKind arm

**Files:** `crates/lute-check/src/directives.rs` (the `check_attr_value` match); Test: `crates/lute-check/tests/asset.rs` (NEW).

**Interfaces:**
- Consumes: `snapshot.asset_kinds`, `providers`, `asset::{decompose, validate_segments, is_placeholder}`.
- Behavior (plugin §6.9 precedence step-1, checker half): for an attr typed `Type::AssetKind(kind)` with a plain `Str` value `id`:
  - `is_placeholder(id)` → `W-ASSET-PLACEHOLDER` (Warning; the "fill later" convention).
  - kind unknown in snapshot → skip (defensive; assembly should have provided it).
  - kind is `resolve: query` with NO segments → provider-existence only (decompose to one opaque value; if the kind has a `provider`, check `ProviderSet::contains(provider, id)` → Absent→`E-ASSET-UNKNOWN-ID`, Stale→`W-CATALOG-STALE`).
  - segment-bearing kind → `decompose`: `Err(Arity|ConstMismatch)` → `E-ASSET-DECOMPOSE`; else `validate_segments` → map each `AssetIssue` to `E-ASSET-SEGMENT` (Error) / `W-CATALOG-STALE` (Stale). An **opaque-legacy** id that fails decompose against a kind whose spec marks it escape-hatchable is exempt — but for the FIRST implementation, a decompose failure IS a diagnostic (document that the legacy-exempt nuance is deferred; the examples use either structured ids, `PLACEHOLDER_*`, or plain-`string`-typed core `assetId` which never reaches this arm).
- New diagnostic codes: `E-ASSET-DECOMPOSE`, `E-ASSET-SEGMENT`, `E-ASSET-UNKNOWN-ID`, `W-ASSET-PLACEHOLDER` (+ reuse `W-CATALOG-STALE`).

- [ ] **Step 1: failing tests** (in `tests/asset.rs`, build a snapshot with a `CH` assetKind + a directive whose `assetId` attr is `Type::AssetKind("CH")` + a ProviderSet with `character: [marina]`): a valid `CH.marina.waitress.delighted.1` → no error; a bad characterId `CH.zzz...` → `E-ASSET-SEGMENT` (UnknownProviderId); an arity-wrong id → `E-ASSET-DECOMPOSE`; a `PLACEHOLDER_x` → `W-ASSET-PLACEHOLDER`; a query-kind id absent from its provider → `E-ASSET-UNKNOWN-ID`. Run → FAIL.
- [ ] **Step 2: implement** the `Type::AssetKind(kind)` arm in `check_attr_value` per the behavior above; helper `diag`s reuse the file's builder. Ensure `providers` reaches the arm (it's already a param).
- [ ] **Step 3: regression** — `cargo test -p lute-check` fully green; core-only marina/date-minigame unaffected (core `assetId` is plain `string`, never `AssetKind`). Confirm.
- [ ] **Step 4: commit** — fmt+clippy. Commit `feat(check): validate authored assetId against its assetKind (plugin §6.9)`.

## Phase A5 — LSP per-segment asset assist

### Task A5.1: completion/hover/go-to-def on an authored `assetId`

**Files:** `crates/lute-lsp/src/features/{mod.rs,completion.rs,hover.rs,nav.rs}`; Test: the respective feature test modules.

**Interfaces:**
- Consumes: `snapshot.asset_kinds`, `asset::decompose`, the provider snapshot, the existing cursor resolver.
- Behavior: when the cursor is inside an `assetId` attr value whose attr type is `Type::AssetKind(kind)`:
  - **completion**: split the value at the cursor by `sep`; the segment index → its declared segment type → completions: `const` (the literal), `enum` (members), `providerRef` (ids from the pinned snapshot), `number`/`string` (none). No providers baseline → empty (honest).
  - **hover**: the segment under the cursor → `**<segName>**: <type>` (+ for providerRef, note the provider).
  - **go-to-def**: a `providerRef` segment → the provider's decl site if in-document (else None — provider decls are snapshot data, not scene text; None is honest, mirrors @ref-to-snapshot-def).
- Reuse the SAME `asset::decompose` + segment-typing the checker uses (no divergence).

- [ ] **Step 1: failing tests** — completion inside a `CH.marina.<cursor>` costume/emotion segment lists the enum members / provider ids; hover on the `characterId` segment shows its providerRef type; (go-to-def returns None for a snapshot-only provider — assert None, not a panic). Run → FAIL.
- [ ] **Step 2: implement** the cursor-in-assetId path in the resolver + the three features, reusing `asset::decompose`.
- [ ] **Step 3: green + commit** — `cargo test -p lute-lsp` green (incl. divergence). fmt+clippy. Commit `feat(lsp): per-segment assetId completion/hover/nav (plugin §6.9)`.

## Phase A6 — Gate + fixture

### Task A6.1: an assetKind-bearing fixture + full gate

**Files:** extend `docs/examples/arcia-project/plugins/arcia.minigame/` OR a new small plugin with an `assetkinds/` export + a directive whose `assetId` is `Type::AssetKind`; a `.lute` scene using it; extend `catalog/` with the referenced provider ids. Golden/CLI test that the scene checks clean with the plugin and flags a bad segment without.

- [ ] **Step 1** — author the fixture (a `CH`-style kind + a directive + a scene with a valid and an invalid authored id) and a CLI/golden test asserting the valid id is clean + the invalid one yields `E-ASSET-SEGMENT`.
- [ ] **Step 2: full gate** — `cargo test --workspace`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo fmt --check`; tree-sitter `test` (drift guard already green); the 3 example behaviors; both trees clean.
- [ ] **Step 3: commit** the fixture + tests. Then this branch is ready to merge (ff to main, as before).

## Deferred (engine-owned, NOT in this plan)

- **compose-from-attrs / query-from-attrs**: the engine produces the final id from a directive's attrs; the checker validates an *authored* id only.
- **fallback-hook resolution + offline candidate generation**: a closed core hook registry (emotionGroup/neutral/variant0/dropVariation/areaKind/…), deterministic over the full pinned asset catalog. Large + engine-adjacent. `fallback` names are recorded in the snapshot but not resolved.
- **`canonicalAssetId` redirect table**: engine data.
- **opaque-legacy escape-hatch nuance**: a decompose failure is a diagnostic in v1; the §6.9 "opaque legacy id exempt" refinement is deferred (no example exercises it beyond `PLACEHOLDER_*`, which IS handled).

---

# Part B — Heavy checker precision (independent; candidate for its own plan)

> These two are **independent of assetKinds** and of each other. Per writing-plans Scope Check they could be a separate plan; included here as the user folded them with the assetKinds deferral. Execute after Part A or split off.

## Phase B1 — E-WRITE-CONFLICT property precision

**Problem (from `timeline.rs` scope note):** `resolve_timeline` only gets `Ctx`, not the resolved `CapabilitySnapshot`, so cross-track write-conflict detection is scoped to the *subject* derivable from the `TrackKey` — it over-reports when two clips write the same subject but DIFFERENT properties (no real conflict), and can't use a directive's declared `writes[]`.

**Approach:** thread the `snapshot` (or the per-directive `writes[]` it carries) into `resolve_timeline`; refine conflict detection to property granularity — two clips conflict only when their resolved write-targets (subject + property, from `writes[]`) overlap at overlapping times.

### Task B1.1: thread snapshot into resolve_timeline + property-level conflict
**Files:** `crates/lute-check/src/timeline.rs` (signature + conflict logic), `check.rs` (pass snapshot); Test: `timeline.rs` tests.
- [ ] **Step 1: failing tests** — two clips, same subject, DIFFERENT properties (via each directive's `writes[]` property path) at overlapping times → NO `E-WRITE-CONFLICT` (currently false-positive); two clips same subject+property overlapping → `E-WRITE-CONFLICT` (preserved). Run → the different-property case FAILS today (over-reports).
- [ ] **Step 2: implement** — extend `resolve_timeline(tl, ctx)` → `resolve_timeline(tl, ctx, snapshot)`; for each clip directive, resolve its declared `writes[]` targets (scope+path property) from the snapshot; conflict = overlapping interval AND intersecting write-target set (subject+property). A directive with no `writes[]` falls back to subject-only (today's behavior — conservative). Update the `check.rs` call site + any tests.
- [ ] **Step 3: green + commit** — `cargo test -p lute-check` green (property-distinct no longer conflicts; same-property still does; existing timeline goldens updated if their expectation legitimately changed — reconcile against §11.4). fmt+clippy. Commit `feat(check): property-level E-WRITE-CONFLICT via directive writes[] (dsl §11.4)`.

## Phase B2 — E-REF-TYPE (`@ref` type-context match)

**Problem (from `cel_resolve.rs`/`check.rs` notes):** `E-REF-TYPE` is deferred because it needs (a) per-def type info in `Ctx` (partially present — `defs` names are threaded, but not their declared types) AND (b) an **expected-type per CEL slot** (which does not exist — there is no model of what type a given CEL slot must produce). Implementing it requires DESIGNING that expected-type model first.

**Approach (design + implement):**
1. **Expected-type model**: a CEL slot has an expected type derived from its context — a `<match on>` subject inherits the subject path's type; a `<when test>`/guard expects `bool`; a `::set{p = expr}` RHS expects `p`'s declared type; a directive attr `@ref` expects the attr's declared type. Add an `ExpectedType` to the slot-check context (only where statically known; `None` when not).
2. **Def types**: thread each `defs:`/plugin `DefDecl` declared type into `Ctx` (name → Type), so `@ref`'s produced type is known.
3. **Check**: when both a def's produced type and the slot's expected type are known and incompatible → `E-REF-TYPE`.

> This is the largest single task; it is **underspecified** in the deferral note ("expected-type per CEL slot doesn't exist yet"). Treat Phase B2 as **design-first**: the first task DESIGNS + documents the expected-type model (a short design note + the `ExpectedType` enum + where it's derivable), reviewed before implementation. If the design review finds the model too broad for the value, scope B2 to the highest-confidence context only (`::set` RHS vs target type) and defer the rest.

### Task B2.1: design the expected-type model (design note + types, reviewed before impl)
**Files:** `crates/lute-check/src/ctx.rs` (`ExpectedType` + def-type map), a design note appended to the module doc; Test: none yet (design task — the reviewer gates the design).
- [ ] **Step 1** — write the `ExpectedType` enum + `Ctx.def_types: BTreeMap<String, Type>` + a doc-comment design note enumerating exactly which CEL-slot contexts have a statically-known expected type (match subject, when/guard=bool, set-RHS=target type, attr-ref=attr type) and which don't. Compile only (no behavior yet).
- [ ] **Step 2: design review** — dispatch a reviewer on the design note + types: is the model sound, bounded, and does it avoid false positives (only check when BOTH types known)? Gate before Task B2.2.

### Task B2.2: wire def types + expected types + emit E-REF-TYPE
**Files:** `check.rs` (build `def_types` from parse_meta + snapshot DefDecls; set `ExpectedType` per slot), `cel_resolve.rs` (`@ref` produced-type vs expected-type → `E-REF-TYPE`); Test: `cel_resolve.rs` tests.
- [ ] **Step 1: failing tests** — an `@ref` whose def produces `number` used where a `bool` is expected (e.g. a `<when test="@numDef">`) → `E-REF-TYPE`; a type-compatible use → no error; an unknown-expected-type context → no error (no false positive). Run → FAIL.
- [ ] **Step 2: implement** per the reviewed design; only emit when both types are known + incompatible.
- [ ] **Step 3: green + commit** — `cargo test -p lute-check` green; no regression (existing @ref tests). fmt+clippy. Commit `feat(check): E-REF-TYPE type-context match for @ref (dsl §8)`.

## Final gate (after whichever parts are executed)

- [ ] `cargo test --workspace`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo fmt --check`.
- [ ] tree-sitter `test` (drift guard green — re-stamped in A2.1).
- [ ] 3 example behaviors: marina ok:true; date-minigame core-only ok:false; date-minigame +project ok:true.
- [ ] Both git trees clean; whole-branch review → ready to merge; ff to main.

## Self-Review

- **Scope honesty:** the plan explicitly partitions checker/LSP-observable work (IN) from compiler/engine codegen (OUT: compose/query-from-attrs, fallback resolution, canonicalAssetId) — matching the project's Global Constraints. No task ships a fake/partial engine feature.
- **Independence:** Part A (assetKinds) and Part B1/B2 are independent subsystems; Part B is flagged as a split candidate.
- **Invariants:** `asset_kinds` folded into capabilityVersion (A2.1) with the tree-sitter re-stamp; core-only behavior unchanged (core ships no assetKinds; `assetId` stays plain string); LSP asset assist reuses the checker's `asset::decompose` (no divergence); decompose/validate total + deterministic.
- **B2 risk:** E-REF-TYPE is underspecified → made design-first with a review gate, with an explicit narrower fallback scope if the model proves too broad.
- **Cross-crate:** every public-type/signature change (Type::AssetKind, snapshot field, resolve_timeline signature) names the dependent-crate build/test step.
