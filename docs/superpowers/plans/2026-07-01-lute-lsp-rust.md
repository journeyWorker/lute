# Lute LSP (Rust) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a Rust `check()` core that statically validates `.lute` documents and returns structured `CheckResult`, wrapped by a headless CLI and an editor LSP server, plus a tree-sitter grammar.

**Architecture:** One `check(input) -> CheckResult` core (parse -> CEL-slot fill -> validate -> resolved view). Two-tier AST (`ParseAst` generic -> `CheckedIr` per-tag typed). `CelSlot` makes every CEL field a ranged child node. The capability snapshot is the data SoT all consumers read. Thin surfaces (CLI, LSP) wrap the same core; tree-sitter handles editor-side highlight/fold.

**Tech Stack:** Rust 1.85+ (edition 2021), Cargo workspace. `cel-parser` 0.10.1, `tower-lsp-server` 0.23.0 + `tokio`, `serde` + `serde_yaml`, `clap`, `insta`, `tree-sitter`.

## Global Constraints

- **Two SoT proposals are normative:** `docs/proposals/scenario-dsl/0.0.1.md` (grammar + semantics), `docs/proposals/plugin-system/0.0.1.md` (manifest + resolution). `docs/architecture.md` is the AST/compiler/LSP architecture. Section refs below (`dsl §N`, `plugin §N`, `arch`) point at these.
- **`check()` is the contract, not the LSP protocol.** LSP and CLI are thin adapters over one `check()`. No second validation code path.
- **Divergence invariant:** LSP-published diagnostics MUST equal headless CLI diagnostics byte-for-byte after normalization (arch "No divergence").
- **Data, not grammar/behavior** (plugin §3): a plugin adds names + schemas only. Grammar is fixed. Lowering hooks are a closed core registry.
- **Golden-test gate** (plugin §12): every directive declaration MUST have a golden test (DSL -> CheckResult). No clean golden = it is behavior, not data.
- **Scope out:** final `idola_script_commands` flat-record codegen; runtime CEL evaluation; the warm daemon. Our "lowering" stops at the LSP-facing resolved view.
- **`capabilityVersion`** (plugin §13): a deterministic content hash over resolved plugin ids+versions, option objects, active profile, bound provider-snapshot versions. Every generated artifact is stamped; consumers refuse mismatched stamps.
- **Span carries both encodings:** `{ byte_start, byte_end, line, column, utf16_range }` — bytes for the core, UTF-16 for LSP (dsl uses byte offsets; LSP protocol uses UTF-16 columns).
- **TDD:** every task is failing-test-first, minimal impl, green, commit.

---

## File Structure

```
Cargo.toml                     # workspace
crates/
  lute-core-span/              # Span, Diagnostic, Severity, Layer, Fixit, StableId
  lute-manifest/               # Type, schemas, resolution, CapabilitySnapshot, providers, lute.core
  lute-syntax/                 # ParseAst, line parser, frontmatter, comments, CelSlot skeleton
  lute-cel/                    # cel-parser wrap, @ref/$ detection, span mapping
  lute-check/                  # check() -> CheckResult, all validators, timeline, injection
  lute-cli/                    # `lute check`, `lute catalog refresh`
  lute-lsp/                    # tower-lsp-server adapter
tree-sitter-lute/              # grammar.js, queries/
```

---

# Phase 0 — Scaffold

### Task 0.1: Cargo workspace + shared span/diagnostic crate

**Files:**
- Create: `Cargo.toml` (workspace root)
- Create: `crates/lute-core-span/Cargo.toml`
- Create: `crates/lute-core-span/src/lib.rs`
- Test: `crates/lute-core-span/src/lib.rs` (`#[cfg(test)]` module)

**Interfaces:**
- Produces: `Span`, `Severity`, `Layer`, `Fixit`, `Diagnostic`, `StableId`, `TextIndex`. Every downstream crate imports these.

- [ ] **Step 1: Write the failing test**

In `crates/lute-core-span/src/lib.rs`, at the bottom:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_index_maps_byte_to_line_col_and_utf16() {
        // "a\nsé" : 'é' is 2 bytes (U+00E9), 1 UTF-16 unit
        let idx = TextIndex::new("a\nsé");
        // byte 0 = line 1 col 1
        let p0 = idx.position(0);
        assert_eq!((p0.line, p0.column), (1, 1));
        // byte 2 = start of line 2 ('s')
        let p2 = idx.position(2);
        assert_eq!((p2.line, p2.column), (2, 1));
        // 'é' begins at byte 3; its UTF-16 column within line 2 is 1 (0-based), byte column 2
        let p3 = idx.position(3);
        assert_eq!(p3.line, 2);
        assert_eq!(p3.utf16_col, 1);
    }

    #[test]
    fn span_from_bytes_fills_both_encodings() {
        let idx = TextIndex::new("hello");
        let s = Span::from_bytes(&idx, 1, 4);
        assert_eq!((s.byte_start, s.byte_end), (1, 4));
        assert_eq!(s.line, 1);
        assert_eq!(s.column, 2); // 1-based byte column
        assert_eq!(s.utf16_range, (1, 4));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p lute-core-span`
Expected: FAIL — `TextIndex`, `Span`, `Position` not defined.

- [ ] **Step 3: Write minimal implementation**

`Cargo.toml` (root):
```toml
[workspace]
resolver = "2"
members = ["crates/*"]

[workspace.package]
edition = "2021"
rust-version = "1.85"

[workspace.dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
serde_yaml = "0.9"
insta = { version = "1", features = ["yaml"] }
```

`crates/lute-core-span/Cargo.toml`:
```toml
[package]
name = "lute-core-span"
version = "0.0.0"
edition.workspace = true
rust-version.workspace = true

[dependencies]
serde = { workspace = true }
```

`crates/lute-core-span/src/lib.rs` (above the test module):
```rust
use serde::{Deserialize, Serialize};

/// Precomputed line-start table for byte <-> (line, col, utf16) mapping.
pub struct TextIndex<'a> {
    text: &'a str,
    line_starts: Vec<usize>, // byte offset of each line start
}

#[derive(Clone, Copy, Debug)]
pub struct Position {
    pub line: u32,     // 1-based
    pub column: u32,   // 1-based byte column within line
    pub utf16_col: u32, // 0-based UTF-16 column within line
}

impl<'a> TextIndex<'a> {
    pub fn new(text: &'a str) -> Self {
        let mut line_starts = vec![0usize];
        for (i, b) in text.bytes().enumerate() {
            if b == b'\n' {
                line_starts.push(i + 1);
            }
        }
        Self { text, line_starts }
    }

    pub fn position(&self, byte: usize) -> Position {
        let line_ix = match self.line_starts.binary_search(&byte) {
            Ok(i) => i,
            Err(i) => i - 1,
        };
        let line_start = self.line_starts[line_ix];
        let slice = &self.text[line_start..byte];
        let byte_col = (byte - line_start) as u32;
        let utf16_col = slice.chars().map(|c| c.len_utf16() as u32).sum();
        Position { line: line_ix as u32 + 1, column: byte_col + 1, utf16_col }
    }

    fn utf16_offset(&self, byte: usize) -> u32 {
        // total UTF-16 units from start of file to byte (for LSP ranges we use per-line cols,
        // but Span keeps a file-relative utf16_range for the divergence golden)
        self.text[..byte].chars().map(|c| c.len_utf16() as u32).sum()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    pub byte_start: usize,
    pub byte_end: usize,
    pub line: u32,       // 1-based, of byte_start
    pub column: u32,     // 1-based byte column of byte_start
    pub utf16_range: (u32, u32), // file-relative UTF-16 offsets
}

impl Span {
    pub fn from_bytes(idx: &TextIndex, start: usize, end: usize) -> Self {
        let p = idx.position(start);
        Span {
            byte_start: start,
            byte_end: end,
            line: p.line,
            column: p.column,
            utf16_range: (idx.utf16_offset(start), idx.utf16_offset(end)),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity { Error, Warning, Info, Hint }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Layer { Content, Staging, Logic, Cel }

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fixit {
    pub title: String,
    pub kind: String,             // e.g. "quickfix"
    pub edit: Vec<TextEdit>,
    pub confidence: u8,           // 0..=100
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextEdit {
    pub span: Span,
    pub new_text: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub code: String,             // stable, e.g. "E-UNDECLARED"
    pub severity: Severity,
    pub message: String,
    pub span: Span,
    pub layer: Layer,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fixits: Vec<Fixit>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<String>,
}

/// Stable node id: assigned once, survives edits (dsl §12 textUnitId principle).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StableId(pub u64);
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p lute-core-span`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml crates/lute-core-span
git commit -m "feat(span): workspace + shared span/diagnostic types"
```

---

# Phase 1 — Manifest & capability snapshot

Order per arch roadmap #9: enums + attr schemas -> manifest -> resolution -> snapshot hash -> providers.

### Task 1.1: The manifest Type system (plugin §7)

**Files:**
- Create: `crates/lute-manifest/Cargo.toml`
- Create: `crates/lute-manifest/src/lib.rs`
- Create: `crates/lute-manifest/src/types.rs`
- Test: `crates/lute-manifest/src/types.rs` (`#[cfg(test)]`)

**Interfaces:**
- Produces: `Type`, `Field`, `PathSegment`, `type_accepts(&Type, &Literal) -> bool`, `Literal`.

- [ ] **Step 1: Write the failing test**

`crates/lute-manifest/src/types.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enum_type_accepts_member_rejects_nonmember() {
        let t = Type::Enum(vec!["gold".into(), "silver".into()]);
        assert!(type_accepts(&t, &Literal::Str("gold".into())));
        assert!(!type_accepts(&t, &Literal::Str("bronze".into())));
    }

    #[test]
    fn list_type_accepts_homogeneous_only() {
        let t = Type::List(Box::new(Type::Number));
        assert!(type_accepts(&t, &Literal::List(vec![Literal::Num(1.0), Literal::Num(2.0)])));
        assert!(!type_accepts(&t, &Literal::List(vec![Literal::Num(1.0), Literal::Bool(true)])));
    }

    #[test]
    fn yaml_roundtrips_provider_ref_type() {
        let y = "providerRef: character";
        let t: Type = serde_yaml::from_str(y).unwrap();
        assert!(matches!(t, Type::ProviderRef(ref n) if n == "character"));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p lute-manifest types`
Expected: FAIL — module/types not defined.

- [ ] **Step 3: Write minimal implementation**

`crates/lute-manifest/Cargo.toml`:
```toml
[package]
name = "lute-manifest"
version = "0.0.0"
edition.workspace = true
rust-version.workspace = true

[dependencies]
lute-core-span = { path = "../lute-core-span" }
serde = { workspace = true }
serde_yaml = { workspace = true }
```

`crates/lute-manifest/src/lib.rs`:
```rust
pub mod types;
pub use types::*;
```

`crates/lute-manifest/src/types.rs` (above the test module):
```rust
use serde::{Deserialize, Serialize};

/// plugin §7 Type. Serde uses YAML's tagged-map forms
/// ({ enum: [...] }, { list: T }, { providerRef: name }, ...).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Type {
    Bool,
    Number,
    #[serde(rename = "string")]
    Str,
    Enum(Vec<String>),
    List(Box<Type>),
    Record(Vec<Field>),
    Map { key: Box<Type>, value: Box<Type> },
    EnumFromOption(String),   // attribute types only
    ProviderRef(String),      // any typed position
    SlotId { namespace: String }, // attribute types only
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Field {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: Type,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<Literal>,
    #[serde(default)]
    pub required: bool,
    /// state-shape fields MAY use `shape: <name>` instead of an inline type; attr types MAY NOT.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shape: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Literal {
    Bool(bool),
    Num(f64),
    Str(String),
    List(Vec<Literal>),
}

/// plugin §7.4 structured path segment.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PathSegment {
    Literal(String),
    FromAttr { #[serde(rename = "fromAttr")] from_attr: FromAttr },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FromAttr {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slot_type: Option<String>,
}

pub fn type_accepts(ty: &Type, lit: &Literal) -> bool {
    match (ty, lit) {
        (Type::Bool, Literal::Bool(_)) => true,
        (Type::Number, Literal::Num(_)) => true,
        (Type::Str, Literal::Str(_)) => true,
        (Type::Enum(members), Literal::Str(s)) => members.iter().any(|m| m == s),
        (Type::List(inner), Literal::List(items)) => items.iter().all(|i| type_accepts(inner, i)),
        _ => false,
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p lute-manifest types`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/lute-manifest
git commit -m "feat(manifest): plugin §7 Type system + literal acceptance"
```

### Task 1.2: Directive / state / provider / bridge / def schemas (plugin §6, §8)

**Files:**
- Create: `crates/lute-manifest/src/schema.rs`
- Modify: `crates/lute-manifest/src/lib.rs:1` (add `pub mod schema;`)
- Test: `crates/lute-manifest/src/schema.rs` (`#[cfg(test)]`)

**Interfaces:**
- Consumes: `Type`, `Field`, `PathSegment` (Task 1.1).
- Produces: `DirectiveDecl`, `AttrDecl`, `StateShape`, `StateTemplate`, `ProviderDecl`, `BridgeCapability`, `DefDecl`, `PluginManifest`.

- [ ] **Step 1: Write the failing test**

`crates/lute-manifest/src/schema.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    const MINIGAME_DIR: &str = r#"
directives:
  - name: minigame
    layer: bridge
    attrs:
      - { name: kind, required: true, type: { enumFromOption: allowedKinds } }
      - { name: id, required: true, type: { providerRef: minigameId } }
      - { name: wait, type: bool, default: true }
    semantics: [ "writes.sceneState", "bridgeCall" ]
    bridge: { service: minigame, operation: play }
    lower: { kind: builtin, name: bridgeMinigame }
"#;

    #[test]
    fn parses_directive_with_attrs_and_lower() {
        let file: DirectivesFile = serde_yaml::from_str(MINIGAME_DIR).unwrap();
        let d = &file.directives[0];
        assert_eq!(d.name, "minigame");
        assert_eq!(d.attrs.len(), 3);
        assert!(d.attrs[0].required);
        assert!(matches!(d.lower, Lowering::Builtin { .. }));
    }

    #[test]
    fn state_shape_field_defaults_are_typed() {
        let y = r#"
stateShapes:
  - name: minigameResult
    fields:
      - { name: rank, type: { enum: [fail, gold] }, default: fail }
"#;
        let f: ShapesFile = serde_yaml::from_str(y).unwrap();
        assert_eq!(f.state_shapes[0].fields[0].name, "rank");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p lute-manifest schema`
Expected: FAIL — schema types not defined.

- [ ] **Step 3: Write minimal implementation**

Add `pub mod schema;` to `lib.rs`. `crates/lute-manifest/src/schema.rs` (above tests):
```rust
use serde::{Deserialize, Serialize};
use crate::types::{Field, PathSegment, Type, Literal};

#[derive(Debug, Deserialize)] pub struct DirectivesFile { pub directives: Vec<DirectiveDecl> }
#[derive(Debug, Deserialize)] pub struct ShapesFile { #[serde(rename = "stateShapes")] pub state_shapes: Vec<StateShape> }
#[derive(Debug, Deserialize)] pub struct TemplatesFile { #[serde(rename = "stateTemplates")] pub state_templates: Vec<StateTemplate> }
#[derive(Debug, Deserialize)] pub struct ProvidersFile { pub providers: Vec<ProviderDecl> }
#[derive(Debug, Deserialize)] pub struct BridgeFile { #[serde(rename = "bridgeCapabilities")] pub bridge: Vec<BridgeCapability> }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DirectiveDecl {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer: Option<String>,
    pub attrs: Vec<AttrDecl>,
    #[serde(default)]
    pub semantics: Vec<String>,      // closed vocabulary; validated in Task 1.5
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<DirectiveState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effects: Option<DirectiveEffects>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bridge: Option<BridgeRef>,
    pub lower: Lowering,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AttrDecl {
    pub name: String,
    #[serde(default)]
    pub required: bool,
    #[serde(rename = "type")]
    pub ty: Type,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<Literal>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DirectiveState { pub declares: Vec<SlotDecl> }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SlotDecl { pub scope: String, pub path: Vec<PathSegment>, pub shape: String }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DirectiveEffects { pub writes: Vec<WriteDecl> }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WriteDecl { pub scope: String, pub path: Vec<PathSegment>, pub value: WriteValue }

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum WriteValue {
    FromBridgeResult { #[serde(rename = "fromBridgeResult")] from_bridge_result: String },
    Op { op: String, by: f64 },
    Literal(Literal),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BridgeRef { pub service: String, pub operation: String }

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Lowering {
    Record { record: String, fields: serde_yaml::Value },
    Builtin { kind: String, name: String },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StateShape { pub name: String, pub fields: Vec<Field> }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StateTemplate { pub name: String, pub scope: String, pub path: Vec<PathSegment>, pub shape: String }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProviderDecl {
    pub name: String,
    #[serde(rename = "idShape", default, skip_serializing_if = "Option::is_none")]
    pub id_shape: Option<String>,
    pub snapshot: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BridgeCapability {
    pub service: String,
    pub operation: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replay: Option<String>,
    #[serde(default)]
    pub result: Vec<Field>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DefDecl {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: Type,
    #[serde(default)]
    pub params: std::collections::BTreeMap<String, Type>,
    pub cel: String,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub max: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub values: Option<Vec<String>>,
}

/// plugin §5 manifest entry.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PluginManifest {
    pub id: String,
    pub version: String,
    pub kind: String,
    #[serde(default)]
    pub depends: Vec<Depends>,
    pub exports: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub options: Vec<OptionDecl>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Depends { pub id: String, pub range: String }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OptionDecl {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: Type,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<Literal>,
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p lute-manifest schema`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/lute-manifest
git commit -m "feat(manifest): directive/state/provider/bridge/def schemas (plugin §6,§8)"
```

### Task 1.3: Profile resolution order (plugin §11.1, §11.2)

**Files:**
- Create: `crates/lute-manifest/src/resolve.rs`
- Modify: `crates/lute-manifest/src/lib.rs` (add `pub mod resolve;`)
- Test: `crates/lute-manifest/src/resolve.rs` (`#[cfg(test)]`)

**Interfaces:**
- Consumes: `PluginManifest`, `OptionDecl`, `Literal` (Task 1.2).
- Produces: `ProfileGraph`, `resolve_activation(&ProfileGraph, selected: &str, scene_local: &ActivationMap) -> Result<Vec<ActivePlugin>, ResolveError>`, `ActivePlugin { id, options }`.

- [ ] **Step 1: Write the failing test**

`crates/lute-manifest/src/resolve.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn graph() -> ProfileGraph {
        // global -> story -> date -> date-minigame, per plugin §11 example
        let mut profiles = BTreeMap::new();
        profiles.insert("global".into(), Profile { extends: None, plugins: map(&[("lute.core", opts(&[]))]) });
        profiles.insert("story".into(), Profile { extends: None, plugins: map(&[("idola.vn", opts(&[]))]) });
        profiles.insert("date".into(), Profile { extends: Some("story".into()), plugins: map(&[("idola.date", opts(&[]))]) });
        profiles.insert("date-minigame".into(), Profile {
            extends: Some("date".into()),
            plugins: map(&[("idola.minigame", opts(&[("resultScope", Literal::Str("scene".into()))]))]),
        });
        ProfileGraph { profiles, default_profile: "story".into() }
    }
    fn opts(kv: &[(&str, Literal)]) -> BTreeMap<String, Literal> {
        kv.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
    }
    fn map(kv: &[(&str, BTreeMap<String, Literal>)]) -> BTreeMap<String, BTreeMap<String, Literal>> {
        kv.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
    }

    #[test]
    fn resolves_extends_chain_parent_first_with_core_and_global() {
        let g = graph();
        let active = resolve_activation(&g, "date-minigame", &BTreeMap::new()).unwrap();
        let ids: Vec<_> = active.iter().map(|a| a.id.as_str()).collect();
        // §11.1 order: lute.core, global's plugins, extends chain parent-first, selected, scene-local
        assert_eq!(ids, vec!["lute.core", "idola.vn", "idola.date", "idola.minigame"]);
    }

    #[test]
    fn scalar_option_later_layer_overrides() {
        let g = graph();
        let scene_local = map(&[("idola.minigame", opts(&[("resultScope", Literal::Str("run".into()))]))]);
        let active = resolve_activation(&g, "date-minigame", &scene_local).unwrap();
        let mg = active.iter().find(|a| a.id == "idola.minigame").unwrap();
        assert_eq!(mg.options.get("resultScope"), Some(&Literal::Str("run".into())));
    }

    #[test]
    fn extends_cycle_is_error() {
        let mut g = graph();
        g.profiles.get_mut("story").unwrap().extends = Some("date".into()); // story<-date<-story
        assert!(matches!(resolve_activation(&g, "date", &std::collections::BTreeMap::new()), Err(ResolveError::ExtendsCycle(_))));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p lute-manifest resolve`
Expected: FAIL.

- [ ] **Step 3: Write minimal implementation**

Add `pub mod resolve;`. `crates/lute-manifest/src/resolve.rs` (above tests):
```rust
use std::collections::BTreeMap;
use crate::types::Literal;

pub type ActivationMap = BTreeMap<String, BTreeMap<String, Literal>>;

#[derive(Clone, Debug)]
pub struct Profile {
    pub extends: Option<String>,
    pub plugins: ActivationMap,
}

#[derive(Clone, Debug)]
pub struct ProfileGraph {
    pub profiles: BTreeMap<String, Profile>,
    pub default_profile: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ActivePlugin {
    pub id: String,
    pub options: BTreeMap<String, Literal>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ResolveError {
    UnknownProfile(String),
    ExtendsCycle(String),
}

impl ProfileGraph {
    fn extends_chain(&self, selected: &str) -> Result<Vec<String>, ResolveError> {
        // returns parent-first chain EXCLUDING global, INCLUDING selected last
        let mut chain = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        let mut cur = Some(selected.to_string());
        while let Some(name) = cur {
            if !self.profiles.contains_key(&name) {
                return Err(ResolveError::UnknownProfile(name));
            }
            if !seen.insert(name.clone()) {
                return Err(ResolveError::ExtendsCycle(name));
            }
            chain.push(name.clone());
            cur = self.profiles[&name].extends.clone();
        }
        chain.reverse(); // parent-first
        Ok(chain)
    }
}

/// plugin §11.1 resolution order + §11.2 merge (scalar override, map deep-merge, list replace).
pub fn resolve_activation(
    graph: &ProfileGraph,
    selected: &str,
    scene_local: &ActivationMap,
) -> Result<Vec<ActivePlugin>, ResolveError> {
    // ordered id list + merged options
    let mut order: Vec<String> = Vec::new();
    let mut merged: BTreeMap<String, BTreeMap<String, Literal>> = BTreeMap::new();

    let mut apply = |acts: &ActivationMap, order: &mut Vec<String>, merged: &mut BTreeMap<String, BTreeMap<String, Literal>>| {
        for (id, opts) in acts {
            if !merged.contains_key(id) { order.push(id.clone()); }
            let entry = merged.entry(id.clone()).or_default();
            for (k, v) in opts { entry.insert(k.clone(), v.clone()); } // scalar override
        }
    };

    // 1. lute.core is always first (language-required)
    if !merged.contains_key("lute.core") {
        order.push("lute.core".into());
        merged.insert("lute.core".into(), BTreeMap::new());
    }
    // 2. profiles.global
    if let Some(g) = graph.profiles.get("global") {
        apply(&g.plugins, &mut order, &mut merged);
    }
    // 3+4. extends chain (parent-first) then selected
    for name in graph.extends_chain(selected)? {
        if name == "global" { continue; }
        apply(&graph.profiles[&name].plugins, &mut order, &mut merged);
    }
    // 5. scene-local
    apply(scene_local, &mut order, &mut merged);

    Ok(order.into_iter().map(|id| ActivePlugin { options: merged.remove(&id).unwrap_or_default(), id }).collect())
}
```

Note: dependency closure (§11.1 step 6) is added in Task 1.4 once manifests are loaded.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p lute-manifest resolve`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/lute-manifest
git commit -m "feat(manifest): profile resolution order + option merge (plugin §11)"
```

### Task 1.4: CapabilitySnapshot assembly + capabilityVersion hash (plugin §13)

**Files:**
- Create: `crates/lute-manifest/src/snapshot.rs`
- Modify: `crates/lute-manifest/src/lib.rs` (add `pub mod snapshot;`)
- Modify: `crates/lute-manifest/Cargo.toml` (add `sha2 = "0.10"`)
- Test: `crates/lute-manifest/src/snapshot.rs` (`#[cfg(test)]`)

**Interfaces:**
- Consumes: `ActivePlugin`, all schema types.
- Produces: `CapabilitySnapshot { version, plugins, enums, directives, providers, state_shapes, bridge_capabilities, defs, frontmatter, ... }`, `capability_version(&CapabilitySnapshot) -> String`, `CapabilitySnapshot::directive(&self, name) -> Option<&DirectiveDecl>`.

- [ ] **Step 1: Write the failing test**

`crates/lute-manifest/src/snapshot.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_deterministic_and_order_independent() {
        let a = CapabilitySnapshot::builder()
            .plugin("lute.core", "0.0.1", &[])
            .plugin("idola.minigame", "0.1.0", &[])
            .build();
        let b = CapabilitySnapshot::builder()
            .plugin("idola.minigame", "0.1.0", &[])   // reversed insert order
            .plugin("lute.core", "0.0.1", &[])
            .build();
        assert_eq!(a.version, b.version);
    }

    #[test]
    fn version_changes_when_plugin_version_changes() {
        let a = CapabilitySnapshot::builder().plugin("lute.core", "0.0.1", &[]).build();
        let b = CapabilitySnapshot::builder().plugin("lute.core", "0.0.2", &[]).build();
        assert_ne!(a.version, b.version);
    }

    #[test]
    fn directive_lookup_finds_registered() {
        let mut snap = CapabilitySnapshot::builder().plugin("lute.core", "0.0.1", &[]).build();
        // (directives are wired via load in Task 1.6; here just assert empty lookup is None)
        assert!(snap.directive("nope").is_none());
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p lute-manifest snapshot`
Expected: FAIL.

- [ ] **Step 3: Write minimal implementation**

Add `sha2 = "0.10"` to deps and `pub mod snapshot;`. `crates/lute-manifest/src/snapshot.rs` (above tests):
```rust
use std::collections::BTreeMap;
use sha2::{Digest, Sha256};
use crate::schema::*;
use crate::types::Literal;

#[derive(Clone, Debug, Default)]
pub struct CapabilitySnapshot {
    pub version: String, // capabilityVersion
    pub plugins: BTreeMap<String, ResolvedPlugin>, // id -> {version, options}
    pub enums: BTreeMap<String, Vec<String>>,
    pub directives: BTreeMap<String, DirectiveDecl>, // by ::name
    pub providers: BTreeMap<String, ProviderDecl>,
    pub state_shapes: BTreeMap<String, StateShape>,
    pub bridge_capabilities: BTreeMap<(String, String), BridgeCapability>,
    pub defs: BTreeMap<String, DefDecl>,
    pub frontmatter: BTreeMap<String, crate::types::Type>,
}

#[derive(Clone, Debug)]
pub struct ResolvedPlugin { pub version: String, pub options: BTreeMap<String, Literal> }

impl CapabilitySnapshot {
    pub fn builder() -> SnapshotBuilder { SnapshotBuilder::default() }
    pub fn directive(&self, name: &str) -> Option<&DirectiveDecl> { self.directives.get(name) }
}

#[derive(Default)]
pub struct SnapshotBuilder { plugins: BTreeMap<String, ResolvedPlugin> }

impl SnapshotBuilder {
    pub fn plugin(mut self, id: &str, version: &str, opts: &[(&str, Literal)]) -> Self {
        self.plugins.insert(id.into(), ResolvedPlugin {
            version: version.into(),
            options: opts.iter().map(|(k, v)| (k.to_string(), v.clone())).collect(),
        });
        self
    }
    pub fn build(self) -> CapabilitySnapshot {
        let mut snap = CapabilitySnapshot { plugins: self.plugins, ..Default::default() };
        snap.version = capability_version(&snap);
        snap
    }
}

/// plugin §13: deterministic hash over resolved plugin ids+versions + option objects (+ providers).
/// BTreeMap iteration is sorted -> order-independent by construction.
pub fn capability_version(snap: &CapabilitySnapshot) -> String {
    let mut h = Sha256::new();
    for (id, p) in &snap.plugins {
        h.update(id.as_bytes());
        h.update(b"@");
        h.update(p.version.as_bytes());
        for (k, v) in &p.options {
            h.update(b"|");
            h.update(k.as_bytes());
            h.update(b"=");
            h.update(format!("{v:?}").as_bytes());
        }
        h.update(b";");
    }
    format!("{:x}", h.finalize())
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p lute-manifest snapshot`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/lute-manifest
git commit -m "feat(manifest): CapabilitySnapshot + deterministic capabilityVersion (plugin §13)"
```

### Task 1.5: Semantics-flag closed vocabulary + plugin validation (plugin §8.1, §15)

**Files:**
- Create: `crates/lute-manifest/src/validate.rs`
- Modify: `crates/lute-manifest/src/lib.rs` (add `pub mod validate;`)
- Test: `crates/lute-manifest/src/validate.rs` (`#[cfg(test)]`)

**Interfaces:**
- Consumes: `DirectiveDecl`, `PluginManifest`.
- Produces: `SEMANTICS_VOCAB: &[&str]`, `validate_plugin(&PluginManifest, &[DirectiveDecl]) -> Vec<ManifestError>`, `ManifestError`.

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{DirectiveDecl, Lowering, AttrDecl};
    use crate::types::Type;

    fn dir(name: &str, semantics: &[&str]) -> DirectiveDecl {
        DirectiveDecl {
            name: name.into(), layer: None,
            attrs: vec![AttrDecl { name: "x".into(), required: false, ty: Type::Bool, default: None }],
            semantics: semantics.iter().map(|s| s.to_string()).collect(),
            state: None, effects: None, bridge: None,
            lower: Lowering::Builtin { kind: "builtin".into(), name: "noop".into() },
        }
    }

    #[test]
    fn unknown_semantics_flag_is_error() {
        let errs = validate_directive(&dir("d", &["writes.sceneState", "totallyMadeUp"]));
        assert!(errs.iter().any(|e| matches!(e, ManifestError::UnknownSemanticsFlag { flag, .. } if flag == "totallyMadeUp")));
    }

    #[test]
    fn known_semantics_flags_pass() {
        let errs = validate_directive(&dir("d", &["writes.sceneState", "bridgeCall"]));
        assert!(errs.is_empty());
    }

    #[test]
    fn duplicate_attr_name_is_error() {
        let mut d = dir("d", &[]);
        d.attrs.push(d.attrs[0].clone());
        let errs = validate_directive(&d);
        assert!(errs.iter().any(|e| matches!(e, ManifestError::DuplicateAttr { .. })));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p lute-manifest validate`
Expected: FAIL.

- [ ] **Step 3: Write minimal implementation**

```rust
use crate::schema::DirectiveDecl;

/// plugin §8.1 closed vocabulary — owned by the core; a plugin MUST NOT invent flags.
pub const SEMANTICS_VOCAB: &[&str] = &[
    "writes.sceneState", "writes.characterState", "reads.onStage",
    "mayExitCharacter", "usesAnchor", "isExit", "isStateful",
    "mutatesScene", "requiresAnchor", "cancelsPrevious", "bridgeCall",
];

#[derive(Clone, Debug, PartialEq)]
pub enum ManifestError {
    UnknownSemanticsFlag { directive: String, flag: String },
    DuplicateAttr { directive: String, attr: String },
}

pub fn validate_directive(d: &DirectiveDecl) -> Vec<ManifestError> {
    let mut errs = Vec::new();
    for flag in &d.semantics {
        if !SEMANTICS_VOCAB.contains(&flag.as_str()) {
            errs.push(ManifestError::UnknownSemanticsFlag { directive: d.name.clone(), flag: flag.clone() });
        }
    }
    let mut seen = std::collections::BTreeSet::new();
    for a in &d.attrs {
        if !seen.insert(a.name.clone()) {
            errs.push(ManifestError::DuplicateAttr { directive: d.name.clone(), attr: a.name.clone() });
        }
    }
    errs
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p lute-manifest validate`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/lute-manifest
git commit -m "feat(manifest): semantics closed vocab + directive validation (plugin §8.1)"
```

### Task 1.6: Built-in `lute.core` manifest + provider snapshot loader (plugin §10, dsl Appendix A)

**Files:**
- Create: `crates/lute-manifest/assets/lute.core/plugin.yaml`
- Create: `crates/lute-manifest/assets/lute.core/directives/staging.yaml` (bg/music/sfx/auto/vfx/cut/video/camera + `:line`/`::set` handled as language-owned)
- Create: `crates/lute-manifest/assets/lute.core/enums.yaml` (emotion/mood/volume/anchor/vfxType/musicAction)
- Create: `crates/lute-manifest/src/core.rs` (embed via `include_str!`)
- Create: `crates/lute-manifest/src/provider.rs` (ProviderSnapshot loader §10)
- Modify: `crates/lute-manifest/src/lib.rs`
- Test: `crates/lute-manifest/src/core.rs`, `crates/lute-manifest/src/provider.rs`

**Interfaces:**
- Consumes: all schema + snapshot types.
- Produces: `load_core_snapshot() -> CapabilitySnapshot` (built-in `lute.core` fully populated with dsl Appendix A directives + enums), `ProviderSnapshot { manifest_version, provider_version, entries }`, `ProviderSet::load(dir) -> ProviderSet`, `ProviderSet::contains(provider, id) -> IdStatus { Fresh, Stale, Absent }`.

- [ ] **Step 1: Write the failing test**

`crates/lute-manifest/src/core.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_snapshot_has_baseline_directives() {
        let snap = load_core_snapshot();
        for name in ["bg", "music", "sfx", "auto", "vfx", "cut", "video", "camera"] {
            assert!(snap.directive(name).is_some(), "missing ::{name}");
        }
    }

    #[test]
    fn camera_has_timing_attrs() {
        let snap = load_core_snapshot();
        let cam = snap.directive("camera").unwrap();
        let names: Vec<_> = cam.attrs.iter().map(|a| a.name.as_str()).collect();
        for k in ["focus", "zoom", "duration", "wait"] {
            assert!(names.contains(&k), "camera missing {k}");
        }
    }

    #[test]
    fn music_action_enum_matches_spec() {
        let snap = load_core_snapshot();
        let e = snap.enums.get("musicAction").unwrap();
        assert!(e.contains(&"fade-out".to_string()));
    }
}
```

`crates/lute-manifest/src/provider.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_id_in_fresh_snapshot_is_absent() {
        let ps = ProviderSnapshot {
            manifest_version: "v1".into(), provider_version: "1".into(),
            entries: [("character".to_string(), vec!["bianca".to_string()])].into(),
            stale: false,
        };
        let set = ProviderSet::from_one(ps);
        assert_eq!(set.contains("character", "bianca"), IdStatus::Fresh);
        assert_eq!(set.contains("character", "ghost"), IdStatus::Absent);
    }

    #[test]
    fn absent_id_in_stale_snapshot_is_stale() {
        let ps = ProviderSnapshot {
            manifest_version: "v1".into(), provider_version: "1".into(),
            entries: [("character".to_string(), vec!["bianca".to_string()])].into(),
            stale: true,
        };
        let set = ProviderSet::from_one(ps);
        assert_eq!(set.contains("character", "ghost"), IdStatus::Stale);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p lute-manifest core provider`
Expected: FAIL.

- [ ] **Step 3: Write minimal implementation**

`crates/lute-manifest/assets/lute.core/enums.yaml`:
```yaml
enums:
  emotion: [neutral, surprised, delighted, shy, content, angry, sad]
  mood: [peaceful, tense, romantic, sad, upbeat]
  volume: [silent, down, normal, up, full]
  anchor: [left, center, right]
  vfxType: [whiteOut, blackOut, rain, snow, leaves, petals, raindrop]
  musicAction: [start, change, stop, resume, fade-out]
```

`crates/lute-manifest/assets/lute.core/directives/staging.yaml` (dsl Appendix A — all 8 baseline directives; baseline enums inlined as `{ enum: [...] }` since `Type` has no named-enum-reference variant. `snapshot.enums` mirrors these separately for LSP completion/hover/semantic-tokens only, not for attr type resolution):
```yaml
directives:
  - name: bg
    attrs:
      - { name: location, type: string }
      - { name: time, type: string }
      - { name: assetId, type: string }
    semantics: [ "mutatesScene" ]
    lower: { record: setBackground, fields: {} }
  - name: music
    attrs:
      - { name: action, type: { enum: [start, change, stop, resume, fade-out] } }
      - { name: mood, type: string }
      - { name: volume, type: { enum: [silent, down, normal, up, full] } }
      - { name: assetId, type: string }
      - { name: track, type: string }
    semantics: [ "mutatesScene" ]
    lower: { record: setMusic, fields: {} }
  - name: sfx
    attrs:
      - { name: sound, type: string }
      - { name: assetId, type: string }
      - { name: name, type: string }
    lower: { record: playSfx, fields: {} }
  - name: auto
    attrs:
      - { name: character, type: string }
      - { name: anchor, type: { enum: [left, center, right] } }
      - { name: action, type: string }
    semantics: [ "reads.onStage", "usesAnchor", "mayExitCharacter", "writes.characterState" ]
    lower: { kind: builtin, name: autoStage }
  - name: vfx
    attrs:
      - { name: type, type: { enum: [whiteOut, blackOut, rain, snow, leaves, petals, raindrop] } }
      - { name: label, type: string }
      - { name: transition, type: string }
    lower: { record: playVfx, fields: {} }
  - name: cut
    attrs:
      - { name: assetId, type: string }
      - { name: action, type: { enum: [show, hide] } }
      - { name: full, type: bool }
    lower: { record: cutIn, fields: {} }
  - name: video
    attrs:
      - { name: assetId, type: string }
      - { name: action, type: { enum: [show, hide] } }
      - { name: wait, type: bool, default: true }
    lower: { record: playVideo, fields: {} }
  - name: camera
    attrs:
      - { name: focus, type: string }
      - { name: zoom, type: string }
      - { name: move-x, type: string }
      - { name: move-y, type: string }
      - { name: shake, type: string }
      - { name: reset, type: bool }
      - { name: duration, type: number }
      - { name: easing, type: string }
      - { name: delay, type: number }
      - { name: wait, type: bool, default: false }
    lower: { kind: builtin, name: cameraTransform }
```

`crates/lute-manifest/src/core.rs`:
```rust
use crate::schema::{DirectivesFile, DirectiveDecl};
use crate::snapshot::{CapabilitySnapshot, ResolvedPlugin, capability_version};
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Deserialize)]
struct EnumsFile { enums: BTreeMap<String, Vec<String>> }

const STAGING: &str = include_str!("../assets/lute.core/directives/staging.yaml");
const ENUMS: &str = include_str!("../assets/lute.core/enums.yaml");

pub fn load_core_snapshot() -> CapabilitySnapshot {
    let staging: DirectivesFile = serde_yaml::from_str(STAGING).expect("core staging.yaml");
    let enums: EnumsFile = serde_yaml::from_str(ENUMS).expect("core enums.yaml");
    let mut directives = BTreeMap::new();
    for d in staging.directives { directives.insert(d.name.clone(), d); }
    let mut plugins = BTreeMap::new();
    plugins.insert("lute.core".to_string(), ResolvedPlugin { version: "0.0.1".into(), options: BTreeMap::new() });
    let mut snap = CapabilitySnapshot { plugins, directives, enums: enums.enums, ..Default::default() };
    snap.version = capability_version(&snap);
    snap
}
```

`crates/lute-manifest/src/provider.rs`:
```rust
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub struct ProviderSnapshot {
    pub manifest_version: String,
    pub provider_version: String,
    pub entries: BTreeMap<String, Vec<String>>, // provider name -> ids
    pub stale: bool,
}

#[derive(Clone, Debug, Default)]
pub struct ProviderSet { snaps: Vec<ProviderSnapshot> }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdStatus { Fresh, Stale, Absent }

impl ProviderSet {
    pub fn from_one(s: ProviderSnapshot) -> Self { Self { snaps: vec![s] } }
    pub fn contains(&self, provider: &str, id: &str) -> IdStatus {
        for s in &self.snaps {
            if let Some(ids) = s.entries.get(provider) {
                if ids.iter().any(|x| x == id) { return IdStatus::Fresh; }
                return if s.stale { IdStatus::Stale } else { IdStatus::Absent };
            }
        }
        IdStatus::Absent
    }
}
```

Wire `pub mod core; pub mod provider;` into `lib.rs`. Fill `staging.yaml` fully with all 8 baseline directives from dsl Appendix A (bg/music/sfx/auto/vfx/cut/video/camera).

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p lute-manifest`
Expected: PASS (all manifest tests).

- [ ] **Step 5: Commit**

```bash
git add crates/lute-manifest
git commit -m "feat(manifest): built-in lute.core snapshot + provider snapshot loader (plugin §10)"
```

---

# Phase 2 — Syntax (ParseAst, line parser, CelSlot skeleton)

### Task 2.1: ParseAst node types + CelSlot skeleton

**Files:**
- Create: `crates/lute-syntax/Cargo.toml`
- Create: `crates/lute-syntax/src/lib.rs`
- Create: `crates/lute-syntax/src/ast.rs`
- Test: `crates/lute-syntax/src/ast.rs`

**Interfaces:**
- Consumes: `Span`, `StableId` (lute-core-span).
- Produces: `Document`, `Meta`, `Shot`, `Node`, `Line`, `Directive`, `Set`, `Branch`, `Choice`, `Match`, `Arm`, `Timeline`, `Track`, `Clip`, `Attr`, `AttrValue`, `CelSlot`, `CelKind`.

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn celslot_defaults_to_unparsed() {
        let s = CelSlot::raw(CelKind::Condition, "$ == 'gold'".into(), test_span());
        assert!(s.ast.is_none());
        assert_eq!(s.raw, "$ == 'gold'");
        assert_eq!(s.kind, CelKind::Condition);
    }
    fn test_span() -> lute_core_span::Span {
        lute_core_span::Span { byte_start: 0, byte_end: 0, line: 1, column: 1, utf16_range: (0, 0) }
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p lute-syntax ast`
Expected: FAIL.

- [ ] **Step 3: Write minimal implementation**

`crates/lute-syntax/Cargo.toml`:
```toml
[package]
name = "lute-syntax"
version = "0.0.0"
edition.workspace = true
rust-version.workspace = true

[dependencies]
lute-core-span = { path = "../lute-core-span" }
serde = { workspace = true }
serde_yaml = { workspace = true }
```

`crates/lute-syntax/src/ast.rs` (above tests):
```rust
use lute_core_span::{Span, StableId};

#[derive(Clone, Debug)]
pub struct Document { pub meta: Meta, pub title: Option<(String, Span)>, pub shots: Vec<Shot>, pub span: Span }

#[derive(Clone, Debug)]
pub struct Meta { pub raw_yaml: String, pub span: Span } // parsed into typed form in check

#[derive(Clone, Debug)]
pub struct Shot { pub heading: String, pub number: Option<i64>, pub body: Vec<Node>, pub span: Span }

#[derive(Clone, Debug)]
pub enum Node {
    Line(Line),
    Directive(Directive),
    Set(Set),
    Branch(Branch),
    Match(Match),
    Timeline(Timeline),
}

#[derive(Clone, Debug)]
pub struct Line { pub speaker: String, pub attrs: Vec<Attr>, pub text: String, pub text_span: Span, pub span: Span }

#[derive(Clone, Debug)]
pub struct Directive { pub tag: String, pub attrs: Vec<Attr>, pub span: Span }

#[derive(Clone, Debug)]
pub struct Set { pub path: String, pub path_span: Span, pub op: String, pub expr: CelSlot, pub span: Span }

#[derive(Clone, Debug)]
pub struct Branch { pub id: String, pub attrs: Vec<Attr>, pub choices: Vec<Choice>, pub span: Span }

#[derive(Clone, Debug)]
pub struct Choice { pub id: String, pub label: String, pub when: Option<CelSlot>, pub attrs: Vec<Attr>, pub body: Vec<Node>, pub span: Span }

#[derive(Clone, Debug)]
pub struct Match { pub subject: CelSlot, pub arms: Vec<Arm>, pub span: Span }

#[derive(Clone, Debug)]
pub enum Arm { When { test: CelSlot, body: Vec<Node>, span: Span }, Otherwise { body: Vec<Node>, span: Span } }

#[derive(Clone, Debug)]
pub struct Timeline { pub duration: Option<CelSlot>, pub tracks: Vec<Track>, pub span: Span }

#[derive(Clone, Debug)]
pub struct Track { pub key: TrackKey, pub clips: Vec<Clip>, pub span: Span }

#[derive(Clone, Debug)]
pub enum TrackKey { Subject(String), Channel(String), Property { subject: String, property: String } }

#[derive(Clone, Debug)]
pub struct Clip { pub node: ClipNode, pub at: Option<f64>, pub span: Span }

#[derive(Clone, Debug)]
pub enum ClipNode { Directive(Directive), Set(Set) }

#[derive(Clone, Debug)]
pub struct Attr { pub key: String, pub value: AttrValue, pub span: Span }

#[derive(Clone, Debug)]
pub enum AttrValue { Str(String), Ref(CelSlot), BoolTrue } // bare ident => true; @ref becomes a CelSlot

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CelKind { Condition, AttrValue, SetExpr, MatchSubject }

#[derive(Clone, Debug)]
pub struct CelSlot {
    pub kind: CelKind,
    pub raw: String,
    pub ast: Option<crate::cel_ast::CelAstHandle>, // filled by lute-cel
    pub span: Span,
    pub id: StableId,
}

impl CelSlot {
    pub fn raw(kind: CelKind, raw: String, span: Span) -> Self {
        Self { kind, raw, ast: None, span, id: StableId(0) }
    }
}
```

Create `crates/lute-syntax/src/cel_ast.rs` with a placeholder handle type so `CelSlot` compiles without depending on `lute-cel` (avoids a cycle: `cel` depends on `syntax`, not vice versa):
```rust
/// Opaque handle; lute-cel owns the real AST and attaches it via an index.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CelAstHandle(pub u32);
```

Wire `pub mod ast; pub mod cel_ast;` into `lib.rs`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p lute-syntax ast`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/lute-syntax
git commit -m "feat(syntax): ParseAst node types + CelSlot skeleton"
```

### Task 2.2: Frontmatter peel + comment stripping (dsl §4.2, §6.1)

**Files:**
- Create: `crates/lute-syntax/src/lex.rs`
- Modify: `crates/lute-syntax/src/lib.rs`
- Test: `crates/lute-syntax/src/lex.rs`

**Interfaces:**
- Produces: `peel_frontmatter(text) -> (Option<(String, Span)>, body_start: usize)`, `strip_comments(line, in_string_state) -> String` respecting §4.4 quoted-string exemption, `CommentError::Unterminated(Span)`.

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peels_yaml_frontmatter() {
        let doc = "---\ncharacter: bianca\n---\n# Title\n";
        let (fm, body_start) = peel_frontmatter(doc).unwrap();
        assert!(fm.unwrap().0.contains("character: bianca"));
        assert_eq!(&doc[body_start..], "# Title\n");
    }

    #[test]
    fn strips_block_comment_but_not_inside_string() {
        assert_eq!(strip_comments(r#"::sfx{sound="a /* b */ c"} /* real */"#).trim_end(),
                   r#"::sfx{sound="a /* b */ c"}"#);
    }

    #[test]
    fn unterminated_comment_errors() {
        assert!(matches!(strip_comments_checked("foo /* bar"), Err(CommentError::Unterminated)));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p lute-syntax lex`
Expected: FAIL.

- [ ] **Step 3: Write minimal implementation**

`crates/lute-syntax/src/lex.rs` (above tests): implement `peel_frontmatter` (find leading `---\n`, then next `\n---` line; return inner slice + byte offset after closing delimiter line), `strip_comments`/`strip_comments_checked` (scan char-by-char, track `in_string` toggled by unescaped `"`, drop `/* ... */` only when `!in_string`, error on EOF inside a comment; comments do not nest per §4.2).

```rust
use lute_core_span::Span;

#[derive(Debug, PartialEq)]
pub enum CommentError { Unterminated }

pub fn peel_frontmatter(text: &str) -> Result<(Option<(String, Span)>, usize), CommentError> {
    if !text.starts_with("---\n") && text != "---" {
        return Ok((None, 0));
    }
    let after_open = 4; // "---\n"
    // find a line that is exactly "---"
    let bytes = text.as_bytes();
    let mut i = after_open;
    let mut line_start = after_open;
    while i <= bytes.len() {
        let at_eol = i == bytes.len() || bytes[i] == b'\n';
        if at_eol {
            let line = &text[line_start..i];
            if line == "---" {
                let inner = text[after_open..line_start].to_string();
                let body_start = if i < bytes.len() { i + 1 } else { i };
                let span = Span { byte_start: 0, byte_end: body_start, line: 1, column: 1, utf16_range: (0, 0) };
                return Ok((Some((inner, span)), body_start));
            }
            line_start = i + 1;
        }
        i += 1;
    }
    Ok((None, 0)) // no closing delimiter: treated as no frontmatter (checker flags)
}

pub fn strip_comments(line: &str) -> String {
    strip_comments_checked(line).unwrap_or_else(|_| line.to_string())
}

pub fn strip_comments_checked(line: &str) -> Result<String, CommentError> {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.char_indices().peekable();
    let mut in_string = false;
    let mut escaped = false;
    while let Some((_, c)) = chars.next() {
        if in_string {
            out.push(c);
            if escaped { escaped = false; }
            else if c == '\\' { escaped = true; }
            else if c == '"' { in_string = false; }
            continue;
        }
        if c == '"' { in_string = true; out.push(c); continue; }
        if c == '/' && matches!(chars.peek(), Some((_, '*'))) {
            chars.next(); // consume '*'
            let mut closed = false;
            while let Some((_, d)) = chars.next() {
                if d == '*' && matches!(chars.peek(), Some((_, '/'))) { chars.next(); closed = true; break; }
            }
            if !closed { return Err(CommentError::Unterminated); }
            continue; // comment dropped
        }
        out.push(c);
    }
    Ok(out)
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p lute-syntax lex`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/lute-syntax
git commit -m "feat(syntax): frontmatter peel + comment stripping (dsl §4.2,§6.1)"
```

### Task 2.3: Attribute + line-classification parser (dsl §4.3, §4.5, §7)

**Files:**
- Create: `crates/lute-syntax/src/parser.rs`
- Modify: `crates/lute-syntax/src/lib.rs`
- Test: `crates/lute-syntax/src/parser.rs` + `crates/lute-syntax/tests/examples.rs`

**Interfaces:**
- Consumes: `ast::*`, `lex::*`.
- Produces: `parse(text) -> (Document, Vec<Diagnostic>)`. Diagnostics use `lute_core_span::Diagnostic` (parse errors, layer set per line kind). Classification follows dsl §4.3 precedence exactly.

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Node;

    #[test]
    fn classifies_set_before_generic_directive() {
        let (doc, diags) = parse("---\ncharacter: x\n---\n## Shot 1.\n::set{scene.a = 1}\n");
        assert!(diags.is_empty(), "{diags:?}");
        let body = &doc.shots[0].body;
        assert!(matches!(body[0], Node::Set(_)), "::set must classify as Set, not Directive");
    }

    #[test]
    fn line_text_is_opaque_to_eol() {
        let (doc, _) = parse("---\ncharacter: x\n---\n## Shot 1.\n:line[narrator]: (a) <b> : c\n");
        if let Node::Line(l) = &doc.shots[0].body[0] {
            assert_eq!(l.text, "(a) <b> : c");
            assert_eq!(l.speaker, "narrator");
        } else { panic!("expected Line"); }
    }

    #[test]
    fn unrecognized_line_is_error() {
        let (_doc, diags) = parse("---\ncharacter: x\n---\n## Shot 1.\ngarbage prose\n");
        assert!(diags.iter().any(|d| d.code == "E-UNCLASSIFIED"));
    }

    #[test]
    fn attr_quote_protects_structural_chars() {
        let (doc, _) = parse("---\ncharacter: x\n---\n## Shot 1.\n::sfx{sound=\"a } b\" name=\"n\"}\n");
        if let Node::Directive(d) = &doc.shots[0].body[0] {
            assert_eq!(d.attrs.len(), 2);
            assert_eq!(d.attrs[0].key, "sound");
        } else { panic!(); }
    }
}
```

`crates/lute-syntax/tests/examples.rs`:
```rust
#[test]
fn parses_bianca_example_without_parse_errors() {
    let text = std::fs::read_to_string("../../docs/examples/bianca-s01ep02.lute").unwrap();
    let (doc, diags) = lute_syntax::parse(&text);
    let parse_errs: Vec<_> = diags.iter().filter(|d| d.severity == lute_core_span::Severity::Error).collect();
    assert!(parse_errs.is_empty(), "unexpected parse errors: {parse_errs:?}");
    assert_eq!(doc.shots.len(), 5); // Shot 1..5
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p lute-syntax parser` and `cargo test -p lute-syntax --test examples`
Expected: FAIL.

- [ ] **Step 3: Write minimal implementation**

Implement `parse`: peel frontmatter (Task 2.2), then a **line cursor** that, per non-blank body line (after `strip_comments`), applies dsl §4.3 precedence:
1. `## ` -> shot heading (`ShotHeading` regex: `Shot|Scene|프롤로그|에필로그`)
2. `# ` -> title
3. `::set{` -> `Set` (parse `Path AssignOp CelExpr`)
4. `::`ident -> `Directive` (parse `Attrs`)
5. `:line[` -> `Line` (parse speaker, optional `Attrs`, `:` WS, opaque `Text`)
6. `<`ident / `</` -> block open/close -> recursive `Branch`/`Match`/`Timeline` assembly via a tag stack
7. else -> `E-UNCLASSIFIED` diagnostic (dsl §4.3 rule 7)

Attribute scanner (`{ ... }`): tokenize respecting `"`-quoted values (§4.4) — structural chars inside quotes are literal. `@ref` values become `AttrValue::Ref(CelSlot::raw(CelKind::AttrValue, ...))`; bare ident -> `BoolTrue`. Block assembly matches open/close tags by name (JSX self-naming close) and nests `Node`s; a mismatched/unclosed tag -> `E-UNCLOSED-TAG`. `<timeline>`/`<track>` bodies restrict to staging leaves + `::set` (dsl §7.4); a `:line`/logic block inside a timeline -> `E-TIMELINE-CONTENT` (emitted here as a parse-structure error). `at` outside a timeline -> deferred to check (it is a schema/positional rule, not pure structure).

(This is the largest task; keep the parser in one focused module. Split into `parser/attrs.rs` + `parser/blocks.rs` if it exceeds ~500 lines.)

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p lute-syntax`
Expected: PASS including the bianca example integration test.

- [ ] **Step 5: Commit**

```bash
git add crates/lute-syntax
git commit -m "feat(syntax): line-classification parser + block assembly (dsl §4.3,§7)"
```

---

# Phase 3 — CEL integration

### Task 3.1: CEL parse wrapper with document-relative spans

**Files:**
- Create: `crates/lute-cel/Cargo.toml`
- Create: `crates/lute-cel/src/lib.rs`
- Test: `crates/lute-cel/src/lib.rs`

**Interfaces:**
- Consumes: `cel-parser` 0.10.1; `CelSlot`, `CelAstHandle` (lute-syntax).
- Produces: `CelArena` (owns parsed CEL asts, hands out `CelAstHandle`), `parse_slot(&mut CelArena, raw, base_byte) -> Result<CelAstHandle, CelParseError>`, `CelParseError { message, span }` with the offset mapped back into the document (base + cel `OffsetRange`).

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_cel_and_records_ast() {
        let mut arena = CelArena::default();
        let h = parse_slot(&mut arena, "scene.affect.bianca >= 1", 0).unwrap();
        assert!(arena.get(h).is_some());
    }

    #[test]
    fn invalid_cel_error_span_is_document_relative() {
        let mut arena = CelArena::default();
        // base_byte = 100 => the error offset must be >= 100
        let err = parse_slot(&mut arena, "1 +", 100).unwrap_err();
        assert!(err.span.byte_start >= 100);
    }

    #[test]
    fn dollar_and_ref_parse_as_identifiers() {
        let mut arena = CelArena::default();
        // `$` and `@fond` are DSL-level; cel-parser sees them as ident-ish tokens or we pre-substitute.
        // Here assert our detector finds them before handing to cel-parser.
        let refs = super::scan_refs("@fond && $ == 'gold'");
        assert!(refs.iter().any(|r| r.name == "fond"));
        assert!(refs.iter().any(|r| r.is_dollar));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p lute-cel`
Expected: FAIL.

- [ ] **Step 3: Write minimal implementation**

`crates/lute-cel/Cargo.toml`:
```toml
[package]
name = "lute-cel"
version = "0.0.0"
edition.workspace = true
rust-version.workspace = true

[dependencies]
lute-core-span = { path = "../lute-core-span" }
lute-syntax = { path = "../lute-syntax" }
cel-parser = "0.10.1"
```

`crates/lute-cel/src/lib.rs` (above tests): `CelArena { asts: Vec<cel_parser::ast::Ast> }` with `get(handle)`. `parse_slot` first runs `scan_refs` to record `@ref`/`$` positions (these are DSL macros, not raw CEL — substitute `@name`->a placeholder ident and `$`->a reserved ident `__subject__` before handing the string to `cel_parser::parse`, preserving byte lengths so `OffsetRange`s stay mappable, or map offsets through a fixup table). On `cel_parser` error, translate its offset to `base_byte + offset` and build a `Span` (line/col recomputed by the caller's `TextIndex`; here store byte offsets and a 0 line, filled at check time). Provide `scan_refs(raw) -> Vec<RefUse { name, args_span, is_dollar, span }>`.

```rust
use lute_core_span::Span;
use lute_syntax::cel_ast::CelAstHandle;

#[derive(Default)]
pub struct CelArena { asts: Vec<cel_parser::ast::Ast> }

impl CelArena {
    pub fn get(&self, h: CelAstHandle) -> Option<&cel_parser::ast::Ast> { self.asts.get(h.0 as usize) }
}

#[derive(Clone, Debug)]
pub struct CelParseError { pub message: String, pub span: Span }

#[derive(Clone, Debug)]
pub struct RefUse { pub name: String, pub is_dollar: bool, pub span: Span }

pub fn scan_refs(raw: &str) -> Vec<RefUse> {
    let mut out = Vec::new();
    let b = raw.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'@' {
            let start = i; i += 1;
            let s = i;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_' || b[i] == b'-') { i += 1; }
            out.push(RefUse { name: raw[s..i].to_string(), is_dollar: false, span: byte_span(start, i) });
        } else if b[i] == b'$' {
            out.push(RefUse { name: "$".into(), is_dollar: true, span: byte_span(i, i + 1) });
            i += 1;
        } else { i += 1; }
    }
    out
}

fn byte_span(s: usize, e: usize) -> Span {
    Span { byte_start: s, byte_end: e, line: 0, column: 0, utf16_range: (0, 0) }
}

pub fn parse_slot(arena: &mut CelArena, raw: &str, base_byte: usize) -> Result<CelAstHandle, CelParseError> {
    // substitute DSL tokens with length-preserving placeholders
    let prepared = substitute_dsl_tokens(raw);
    match cel_parser::parse(&prepared) {
        Ok(ast) => { let h = CelAstHandle(arena.asts.len() as u32); arena.asts.push(ast); Ok(h) }
        Err(e) => Err(CelParseError {
            message: e.to_string(),
            span: Span { byte_start: base_byte, byte_end: base_byte + raw.len(), line: 0, column: 0, utf16_range: (0, 0) },
        }),
    }
}

fn substitute_dsl_tokens(raw: &str) -> String {
    // `@name` -> `name` (drop the @, pad a leading space to keep length), `$` -> a valid ident char run.
    // Length preservation keeps cel OffsetRanges mappable back to the doc.
    let mut s = String::with_capacity(raw.len());
    let b = raw.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'@' { s.push(' '); i += 1; }
        else if b[i] == b'$' { s.push('_'); i += 1; }
        else { s.push(b[i] as char); i += 1; }
    }
    s
}
```

Note: the actual `cel_parser::parse` entry name is confirmed against 0.10.1 in Task 3.2's doc-check step; if the entry differs (`parse` vs `Ast::parse`), adjust the one call site.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p lute-cel`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/lute-cel
git commit -m "feat(cel): cel-parser wrap, @ref/$ scan, doc-relative error spans"
```

### Task 3.2: Verify cel-parser entry API + fill CelSlot.ast during a walk

**Files:**
- Modify: `crates/lute-cel/src/lib.rs`
- Create: `crates/lute-cel/src/fill.rs`
- Test: `crates/lute-cel/src/fill.rs`

**Interfaces:**
- Produces: `fill_document(&mut CelArena, &mut Document) -> Vec<CelParseError>` — walks every `CelSlot` in a `ParseAst`, calls `parse_slot`, sets `slot.ast` (Some on success) and `slot.id`, collecting parse errors.

- [ ] **Step 1: Confirm the cel-parser entry point**

Read `https://docs.rs/cel-parser/0.10.1/cel_parser/` and confirm the free function or method that returns `ast::Ast` from `&str`. Adjust the single call in `parse_slot` if the name is not `cel_parser::parse`. (This is a doc-verification step, not code — record the confirmed signature in a comment above the call.)

- [ ] **Step 2: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use lute_syntax::parse;

    #[test]
    fn fills_valid_cel_slots_and_reports_invalid() {
        let text = "---\ncharacter: x\n---\n## Shot 1.\n<match on=\"scene.a\">\n<when test=\"1 +\">\n:line[narrator]: hi\n</when>\n<otherwise>\n:line[narrator]: bye\n</otherwise>\n</match>\n";
        let (mut doc, _) = parse(text);
        let mut arena = CelArena::default();
        let errs = fill_document(&mut arena, &mut doc);
        assert_eq!(errs.len(), 1); // the "1 +" test slot fails; "scene.a" subject parses
    }
}
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test -p lute-cel fill`
Expected: FAIL.

- [ ] **Step 4: Write minimal implementation**

`crates/lute-cel/src/fill.rs`: a recursive visitor over `Document.shots[].body[]` (and choice/arm bodies, timeline clips) that finds each `CelSlot` (`Match.subject`, `When.test`, `Choice.when`, `Set.expr`, `AttrValue::Ref`, `Timeline.duration`), calls `parse_slot(arena, &slot.raw, slot.span.byte_start)`, assigns `slot.ast` / accumulates errors, and assigns a monotonic `StableId`.

- [ ] **Step 5: Run test, commit**

Run: `cargo test -p lute-cel`
Expected: PASS.
```bash
git add crates/lute-cel
git commit -m "feat(cel): fill CelSlot.ast across ParseAst; verified cel-parser entry"
```

---

# Phase 4 — check() core

### Task 4.1: Typed meta (frontmatter) + state schema (dsl §6.1, §9.3)

**Files:**
- Create: `crates/lute-check/Cargo.toml`
- Create: `crates/lute-check/src/lib.rs`
- Create: `crates/lute-check/src/meta.rs`
- Test: `crates/lute-check/src/meta.rs`

**Interfaces:**
- Consumes: `Document`, `Meta`, `CapabilitySnapshot`, `Type`.
- Produces: `TypedMeta { character, season, episode, pov, profile, plugins, uses, state: StateSchema, defs }`, `StateSchema { decls: BTreeMap<String, StateDecl> }`, `StateDecl { ty: Type, default: Option<Literal>, namespace: Namespace }`, `parse_meta(&Meta, &CapabilitySnapshot) -> (TypedMeta, Vec<Diagnostic>)`.

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_state_decls_with_namespace() {
        let yaml = "character: bianca\nseason: 1\nepisode: 2\npov: fixer\nstate:\n  scene.affect.bianca: { type: number, default: 0 }\n";
        let (meta, diags) = parse_meta_str(yaml);
        assert!(diags.is_empty(), "{diags:?}");
        let d = meta.state.decls.get("scene.affect.bianca").unwrap();
        assert_eq!(d.namespace, Namespace::Scene);
    }

    #[test]
    fn missing_required_meta_key_errors() {
        let (_m, diags) = parse_meta_str("season: 1\nepisode: 2\n"); // no character
        assert!(diags.iter().any(|d| d.code == "E-META-MISSING"));
    }

    #[test]
    fn app_write_is_flagged_readonly_at_schema_level() {
        // scene may declare; app.* declared read-only downstream (checked in Task 4.5)
        let (meta, _d) = parse_meta_str("character: x\nseason: 1\nepisode: 2\nstate:\n  app.lang: { type: string }\n");
        assert_eq!(meta.state.decls.get("app.lang").unwrap().namespace, Namespace::App);
    }
}
```

- [ ] **Step 2–5:** Implement `parse_meta` (serde_yaml over the peeled block), map `scene.`/`run.`/`user.`/`app.` prefixes to `Namespace`, validate required keys (`character`/`season`/`episode`), reject unknown top-level keys not owned by an active plugin's `frontmatter` (dsl §6.1). Test, commit.

```bash
git commit -m "feat(check): typed meta + state schema parse (dsl §6.1,§9.3)"
```

### Task 4.2: Directive/attr/enum validation against the snapshot (dsl §7.2, plugin §8)

**Files:**
- Create: `crates/lute-check/src/directives.rs`
- Test: `crates/lute-check/src/directives.rs`

**Interfaces:**
- Consumes: `Directive`, `CapabilitySnapshot`, `ProviderSet`, `Type`, `type_accepts`.
- Produces: `check_directive(&Directive, &CapabilitySnapshot, &ProviderSet, &Ctx) -> Vec<Diagnostic>` — unknown directive (`E-UNKNOWN-DIRECTIVE` with inactive-plugin fix-it), unknown attr (`E-UNKNOWN-ATTR`), missing required attr (`E-MISSING-ATTR`), bad enum value (`E-BAD-ENUM`), bad `providerRef` id (`E-UNKNOWN-ID` / `W-CATALOG-STALE`), type coercion failure (`E-ATTR-TYPE`).

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use lute_manifest::core::load_core_snapshot;

    #[test]
    fn unknown_directive_errors_with_layer_staging() {
        let d = directive("teleport", &[]);
        let errs = check_directive(&d, &load_core_snapshot(), &empty_providers(), &ctx());
        assert!(errs.iter().any(|e| e.code == "E-UNKNOWN-DIRECTIVE" && e.layer == lute_core_span::Layer::Staging));
    }

    #[test]
    fn bad_enum_value_errors() {
        let d = directive("music", &[("action", "explode")]); // not in musicAction enum
        let errs = check_directive(&d, &load_core_snapshot(), &empty_providers(), &ctx());
        assert!(errs.iter().any(|e| e.code == "E-BAD-ENUM"));
    }

    #[test]
    fn known_directive_valid_attrs_pass() {
        let d = directive("music", &[("action", "start"), ("mood", "peaceful")]);
        let errs = check_directive(&d, &load_core_snapshot(), &empty_providers(), &ctx());
        assert!(errs.is_empty(), "{errs:?}");
    }
}
```

- [ ] **Step 2–5:** Implement lookup + per-attr validation. For `Type::Enum(members)` resolve inline members; for `Type::EnumFromOption(opt)` resolve the domain from the directive's plugin's resolved option value (`snapshot.plugins[plugin_id].options[opt]` — a `list`/`enum` literal, plugin §7); `Type::ProviderRef` -> `ProviderSet::contains` -> `Fresh`/`Stale`/`Absent` -> ok/warn/error. Missing `required` -> error. Unknown directive present in an *installed but inactive* plugin -> fix-it "activate plugin"/"change profile" (plugin §11.2). Test, commit.

```bash
git commit -m "feat(check): directive/attr/enum/providerRef validation (plugin §8)"
```

### Task 4.3: CEL slot type-checking + @ref / $ / state-path resolution (dsl §8, §9.4)

**Files:**
- Create: `crates/lute-check/src/cel_resolve.rs`
- Test: `crates/lute-check/src/cel_resolve.rs`

**Interfaces:**
- Consumes: `CelSlot`, `CelArena`, `StateSchema`, `defs` map, `Match` subject context.
- Produces: `check_cel_slot(&CelSlot, &CelArena, &Ctx) -> Vec<Diagnostic>` — undeclared `@ref` (`E-UNDECLARED-REF`), type-mismatched `@ref` use (`E-REF-TYPE`), `$` outside `<match>` (`E-DOLLAR-OUTSIDE-MATCH`), undeclared state path read (`E-UNDECLARED`), reserved `run.choiceLog.*` read in guard (`E-CHOICELOG-READ`, dsl §9.6).

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dollar_outside_match_errors() {
        let ctx = ctx_no_match();
        let slot = cel_slot_condition("$ == 'x'");
        let errs = check_cel_slot(&slot, &arena_for(&slot), &ctx);
        assert!(errs.iter().any(|e| e.code == "E-DOLLAR-OUTSIDE-MATCH"));
    }

    #[test]
    fn undeclared_ref_errors() {
        let ctx = ctx_with_defs(&["fond"]);
        let slot = cel_slot_condition("@warm");
        let errs = check_cel_slot(&slot, &arena_for(&slot), &ctx);
        assert!(errs.iter().any(|e| e.code == "E-UNDECLARED-REF"));
    }

    #[test]
    fn choicelog_read_in_guard_errors() {
        let ctx = ctx_in_match();
        let slot = cel_slot_condition("run.choiceLog.ep02.couch == 'help'");
        let errs = check_cel_slot(&slot, &arena_for(&slot), &ctx);
        assert!(errs.iter().any(|e| e.code == "E-CHOICELOG-READ"));
    }
}
```

- [ ] **Step 2–5:** Walk the `cel_parser` AST via its visitor; collect identifier/select chains; classify each as state-path (starts `scene`/`run`/`user`/`app`) -> check against `StateSchema` (`E-UNDECLARED`), `@ref` (from `scan_refs`) -> check against `defs` + type-context, `$` -> require enclosing match. Test, commit.

```bash
git commit -m "feat(check): CEL slot resolution — @ref/$/state-path (dsl §8,§9.4,§9.6)"
```

### Task 4.4: Definite-assignment analysis (path-sensitive) (dsl §9.4)

**Files:**
- Create: `crates/lute-check/src/defassign.rs`
- Test: `crates/lute-check/src/defassign.rs`

**Interfaces:**
- Consumes: node stream of a shot, `StateSchema`, guard detection (`has()`/`isSet()`).
- Produces: `check_definite_assignment(&[Node], &StateSchema, &Ctx) -> Vec<Diagnostic>` — `E-UNDECLARED` (path not in schema), `E-MAYBE-UNSET` (non-scene tier, no default, no dominating write, no guard). Compound `::set` reads old value.

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_path_no_default_read_is_maybe_unset() {
        // run.metHelpfully declared without default; read before any write
        let (nodes, schema) = fixture("read run.metHelpfully in a <when> with no prior ::set");
        let errs = check_definite_assignment(&nodes, &schema, &ctx());
        assert!(errs.iter().any(|e| e.code == "E-MAYBE-UNSET"));
    }

    #[test]
    fn dominating_write_proves_path() {
        let (nodes, schema) = fixture("::set{run.x = 1} then <when test=\"run.x > 0\">");
        let errs = check_definite_assignment(&nodes, &schema, &ctx());
        assert!(!errs.iter().any(|e| e.code == "E-MAYBE-UNSET"));
    }

    #[test]
    fn compound_assign_first_reads_old_value() {
        let (nodes, schema) = fixture("::set{run.x += 1} with run.x no default, no prior write");
        let errs = check_definite_assignment(&nodes, &schema, &ctx());
        assert!(errs.iter().any(|e| e.code == "E-MAYBE-UNSET")); // += reads old
    }
}
```

- [ ] **Step 2–5:** Implement a forward walk tracking an "assigned set" per path; `scene.*` uses ordinary flow, non-`scene` tiers seed maybe-unset unless schema-defaulted; `=` writes assign; `+=`/`-=`/`*=` require prior-assigned (else `E-MAYBE-UNSET`); guards (`has()`/`isSet()` in enclosing `<when>`) add to assigned within the arm. Branch/match arms fork the state; merge is intersection at the join. Test, commit.

```bash
git commit -m "feat(check): path-sensitive definite-assignment (dsl §9.4)"
```

### Task 4.5: `::set` op/type matrix + write policy (dsl §7.3.4, §9.5)

**Files:**
- Create: `crates/lute-check/src/set_op.rs`
- Test: `crates/lute-check/src/set_op.rs`

**Interfaces:**
- Produces: `check_set(&Set, &StateSchema, &Ctx) -> Vec<Diagnostic>` — `app.*` write (`E-APP-READONLY`), op/type mismatch (`E-SET-OP-TYPE`: `bool` allows `=` only; `*=` numbers only), path not declared (`E-UNDECLARED`).

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn app_write_errors() {
        let errs = check_set(&set("app.lang", "=", "'en'"), &schema_app_lang(), &ctx());
        assert!(errs.iter().any(|e| e.code == "E-APP-READONLY"));
    }
    #[test]
    fn bool_compound_assign_errors() {
        let errs = check_set(&set("scene.flags.saw", "+=", "1"), &schema_bool_flag(), &ctx());
        assert!(errs.iter().any(|e| e.code == "E-SET-OP-TYPE"));
    }
    #[test]
    fn number_increment_ok() {
        let errs = check_set(&set("scene.affect.bianca", "+=", "1"), &schema_number(), &ctx());
        assert!(errs.is_empty(), "{errs:?}");
    }
}
```

- [ ] **Step 2–5:** Implement the matrix from dsl §7.3.4/§9.5. Test, commit.

```bash
git commit -m "feat(check): ::set op/type matrix + app read-only (dsl §7.3.4,§9.5)"
```

### Task 4.6: `<match>` exhaustiveness + first-match + branch recording (dsl §11.1, §11.2)

**Files:**
- Create: `crates/lute-check/src/match_check.rs`
- Test: `crates/lute-check/src/match_check.rs`

**Interfaces:**
- Produces: `check_match(&Match, &StateSchema, &Ctx) -> Vec<Diagnostic>` — non-exhaustive finite domain without `<otherwise>` (`E-NONEXHAUSTIVE`), missing `unset` coverage for maybe-unset subject (`E-UNSET-UNCOVERED`), age-gate `app.rating` without `teen`/`otherwise` (`E-AGE-GATE`), provably-overlapping arms (`W-OVERLAP-ARMS`). `check_branch` — duplicate `<branch id>` in episode (`E-DUP-BRANCH`), records `scene.choices.<id>` implicit decl.

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn enum_domain_without_otherwise_and_missing_arm_errors() {
        // subject domain {fail,gold}; arms cover only gold; no otherwise
        let m = match_on_enum(&["fail", "gold"], &["gold"], false);
        let errs = check_match(&m, &schema_enum_subject(), &ctx());
        assert!(errs.iter().any(|e| e.code == "E-NONEXHAUSTIVE"));
    }
    #[test]
    fn full_coverage_no_error() {
        let m = match_on_enum(&["fail", "gold"], &["fail", "gold"], false);
        let errs = check_match(&m, &schema_enum_subject(), &ctx());
        assert!(!errs.iter().any(|e| e.code == "E-NONEXHAUSTIVE"));
    }
    #[test]
    fn maybe_unset_subject_needs_unset_or_otherwise() {
        let m = match_on_enum(&["fail", "gold"], &["fail", "gold"], false); // no unset arm/otherwise
        let errs = check_match(&m, &schema_maybe_unset_subject(), &ctx());
        assert!(errs.iter().any(|e| e.code == "E-UNSET-UNCOVERED"));
    }
}
```

- [ ] **Step 2–5:** Implement domain inference (enum/bool/branch-child-ids finite; else require otherwise), `unset` membership for maybe-unset subjects, age-gate special case. Test, commit.

```bash
git commit -m "feat(check): <match> exhaustiveness + branch recording (dsl §11.1,§11.2)"
```

### Task 4.7: Timeline resolver + resolved-table view (dsl §11.4)

**Files:**
- Create: `crates/lute-check/src/timeline.rs`
- Test: `crates/lute-check/src/timeline.rs`

**Interfaces:**
- Produces: `resolve_timeline(&Timeline, &Ctx) -> (ResolvedTimeline, Vec<Diagnostic>)`. Per track: sequential-omission cursor (first omitted `at` = 0.0, else after prev clip end); track-local overlap (`E-CLIP-OVERLAP`); duplicate track key (`E-DUP-TRACK`); cross-track write conflict (`E-WRITE-CONFLICT`); final barrier at `duration` or max end. `ResolvedTimeline { rows: Vec<ResolvedRow { at, subject, summary, duration }>, barrier_at }`. Warns `>8` tracks / `>12` clips per track / `>40` total (arch LSP feature map).

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn omitted_at_follows_previous_clip_end() {
        // track camera: clip A dur 0.4 (at omitted=>0.0), clip B dur 0.4 (at omitted=>0.4)
        let tl = timeline_camera_two_clips();
        let (res, diags) = resolve_timeline(&tl, &ctx());
        assert!(diags.is_empty());
        assert_eq!(res.rows[1].at, 0.4);
    }
    #[test]
    fn duplicate_track_key_errors() {
        let tl = timeline_two_camera_tracks();
        let (_res, diags) = resolve_timeline(&tl, &ctx());
        assert!(diags.iter().any(|d| d.code == "E-DUP-TRACK"));
    }
    #[test]
    fn barrier_is_max_end_when_no_duration() {
        let tl = timeline_camera_two_clips(); // ends at 0.8, no explicit duration
        let (res, _d) = resolve_timeline(&tl, &ctx());
        assert_eq!(res.barrier_at, 0.8);
    }
}
```

- [ ] **Step 2–5:** Implement per dsl §11.4. Test, commit.

```bash
git commit -m "feat(check): timeline resolver + resolved table (dsl §11.4)"
```

### Task 4.8: StageState injection reducer + provenance (arch "stateful resolution")

**Files:**
- Create: `crates/lute-check/src/inject.rs`
- Test: `crates/lute-check/src/inject.rs`

**Interfaces:**
- Produces: `StageState { on_stage: BTreeMap<String, SpriteState>, dirty: BTreeSet<String>, bg, music }`, `lower_node(state, node, lookahead) -> (StageState, Vec<InjectedCommand>)`, named rules `auto_pose_reset`, `auto_anchor_on_show`, `entry_emotion_lookahead`, `stage_bookkeeping`. `InjectedCommand { kind, provenance: { injected: true, by, reason } }`. Conflict (author-written vs would-inject) -> `W-INJECT-CONFLICT`.

- [ ] **Step 1: Write the failing test (one per named rule)**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn show_without_anchor_injects_anchor_with_provenance() {
        let st = StageState::default();
        let (st2, injected) = lower_node(st, &show_bianca_no_anchor(), &[]);
        assert!(injected.iter().any(|c| c.provenance.by == "auto-anchor-on-show"));
        assert!(st2.on_stage.contains_key("bianca"));
    }
    #[test]
    fn dirty_pose_before_nonstateful_line_injects_posreset() {
        let mut st = StageState::default();
        st.dirty.insert("bianca".into());
        st.on_stage.insert("bianca".into(), SpriteState::default());
        let (_st2, injected) = lower_node(st, &line_bianca(), &[]);
        assert!(injected.iter().any(|c| c.provenance.by == "auto-pose-reset"));
    }
}
```

- [ ] **Step 2–5:** Implement each named, ordered, pure rule + provenance. Test (one golden per rule via `insta`), commit.

```bash
git commit -m "feat(check): StageState injection reducer + provenance (arch stateful resolution)"
```

### Task 4.9: `check()` assembly + Resolved view

**Files:**
- Create: `crates/lute-check/src/check.rs`
- Modify: `crates/lute-check/src/lib.rs`
- Test: `crates/lute-check/tests/examples.rs`

**Interfaces:**
- Consumes: all Task 4.x validators, `parse` (syntax), `fill_document` (cel).
- Produces: `CheckInput`, `CheckResult`, `Resolved { commands_preview, timeline_tables, injections }`, `Mode`, `check(&CheckInput) -> CheckResult`.

- [ ] **Step 1: Write the failing integration test**

```rust
#[test]
fn bianca_example_checks_clean() {
    let text = std::fs::read_to_string("../../docs/examples/bianca-s01ep02.lute").unwrap();
    let snap = lute_manifest::core::load_core_snapshot();
    let input = lute_check::CheckInput {
        text, uri: "bianca".into(), snapshot: snap,
        providers: permissive_providers(), mode: lute_check::Mode::Author,
    };
    let res = lute_check::check(&input);
    let errors: Vec<_> = res.diagnostics.iter().filter(|d| d.severity == lute_core_span::Severity::Error).collect();
    assert!(errors.is_empty(), "unexpected errors: {errors:?}");
    assert!(res.resolved.is_some());
}

#[test]
fn undeclared_state_read_is_reported() {
    let text = "---\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n<match on=\"scene.nope\">\n<otherwise>\n:line[narrator]: hi\n</otherwise>\n</match>\n";
    let res = lute_check::check(&input_for(text));
    assert!(res.diagnostics.iter().any(|d| d.code == "E-UNDECLARED"));
}
```

- [ ] **Step 2–5:** Implement `check`: `parse` -> `fill_document` -> `parse_meta` -> walk running all validators -> resolve timelines + run injection reducer -> assemble `CheckResult` (sort diagnostics by `span.byte_start` for determinism -> the divergence golden). Test against both examples, commit.

```bash
git commit -m "feat(check): check() assembly + Resolved view; example integration tests"
```

---

# Phase 5 — Headless CLI

### Task 5.1: `lute check` (JSON CheckResult) + `lute catalog refresh`

**Files:**
- Create: `crates/lute-cli/Cargo.toml`
- Create: `crates/lute-cli/src/main.rs`
- Test: `crates/lute-cli/tests/cli.rs`

**Interfaces:**
- Consumes: `check`, `load_core_snapshot`, `ProviderSet`.
- Produces: binary `lute` with `check <file> [--json] [--providers <dir>]` (exit 0 clean / 1 on error diagnostics) and `catalog refresh <dir>`.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn check_clean_file_exits_zero_json() {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_lute"))
        .args(["check", "../../docs/examples/bianca-s01ep02.lute", "--json"])
        .output().unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], true);
}
```

- [ ] **Step 2–5:** `clap` command tree; `--json` prints serialized `CheckResult`; exit code from `res.ok`. Test, commit.

```bash
git commit -m "feat(cli): lute check (JSON) + catalog refresh"
```

### Task 5.2: Per-directive golden harness (plugin §12 gate)

**Files:**
- Create: `crates/lute-check/tests/golden/` (one `.lute` + committed `insta` snapshot per directive)
- Create: `crates/lute-check/tests/golden.rs`

**Interfaces:**
- Produces: a parametrized test that runs `check()` over each `tests/golden/*.lute` and asserts the `insta` snapshot of its `CheckResult` (diagnostics + resolved).

- [ ] **Step 1: Write one failing golden**

Add `crates/lute-check/tests/golden/camera_ok.lute` and:
```rust
#[test]
fn golden_camera_ok() {
    let text = include_str!("golden/camera_ok.lute");
    let res = lute_check::check(&super::input_for(text));
    insta::assert_yaml_snapshot!(res.diagnostics);
}
```

- [ ] **Step 2:** Run `cargo test -p lute-check golden` — FAIL (no snapshot).
- [ ] **Step 3:** `cargo insta review` (accept) once output is correct.
- [ ] **Step 4:** Add one golden per baseline directive (bg/music/sfx/auto/vfx/cut/video/camera) + `::set` + `<branch>`/`<match>`/`<timeline>`.
- [ ] **Step 5: Commit**

```bash
git commit -m "test(check): per-directive golden suite (plugin §12 gate)"
```

---

# Phase 6 — LSP server

### Task 6.1: DocumentSnapshot + didOpen/didChange -> publishDiagnostics

**Files:**
- Create: `crates/lute-lsp/Cargo.toml`
- Create: `crates/lute-lsp/src/main.rs`
- Create: `crates/lute-lsp/src/backend.rs`
- Create: `crates/lute-lsp/src/convert.rs` (CheckResult -> LSP types)
- Test: `crates/lute-lsp/src/convert.rs`

**Interfaces:**
- Consumes: `check`, `Diagnostic`, `Span`; `tower-lsp-server`.
- Produces: `Backend` impl `LanguageServer`; `to_lsp_diagnostic(&Diagnostic, &TextIndex) -> lsp_types::Diagnostic` mapping byte span -> LSP `Range` via per-line UTF-16; `DocumentSnapshot { text, version }`.

- [ ] **Step 1: Write the failing test (pure conversion, no server spin-up)**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn diagnostic_uses_utf16_range() {
        let d = lute_core_span::Diagnostic {
            code: "E-UNDECLARED".into(), severity: lute_core_span::Severity::Error,
            message: "x".into(),
            span: lute_core_span::Span { byte_start: 5, byte_end: 8, line: 3, column: 2, utf16_range: (5, 8) },
            layer: lute_core_span::Layer::Cel, fixits: vec![], provenance: None,
        };
        let l = to_lsp_diagnostic(&d, &line_index());
        assert_eq!(l.range.start.character, /* utf16 col within line 3 */ 1);
        assert_eq!(l.code.unwrap(), tower_lsp_server::lsp_types::NumberOrString::String("E-UNDECLARED".into()));
    }
}
```

- [ ] **Step 2–5:** Implement `Backend` (holds a `DashMap<Url, DocumentSnapshot>`), `did_open`/`did_change` -> `check()` -> `publish_diagnostics`. Conversion maps byte-span to LSP `Range` via per-line UTF-16. Test the pure conversion; a manual smoke via `initialize` handshake is a follow-up step. Commit.

```bash
git commit -m "feat(lsp): DocumentSnapshot + publishDiagnostics over check() (tower-lsp-server)"
```

### Task 6.2: Divergence golden — headless vs LSP diagnostics byte-for-byte

**Files:**
- Create: `crates/lute-lsp/tests/divergence.rs`

**Interfaces:**
- Consumes: `check` output + `to_lsp_diagnostic` round-trip.
- Produces: a test that serializes headless `CheckResult.diagnostics` and the LSP-converted-then-normalized diagnostics and asserts equality.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn headless_and_lsp_diagnostics_match() {
    let text = std::fs::read_to_string("../../docs/examples/date-minigame.lute").unwrap();
    let res = lute_check::check(&input_for(&text));
    // normalize: LSP path converts to utf16 range then back to a comparable tuple
    let headless: Vec<_> = res.diagnostics.iter().map(normalize_headless).collect();
    let via_lsp: Vec<_> = res.diagnostics.iter().map(|d| normalize_lsp(&lute_lsp::convert::to_lsp_diagnostic(d, &idx(&text)))).collect();
    assert_eq!(headless, via_lsp);
}
```

- [ ] **Step 2–5:** Implement normalization helpers; assert equality. This is the arch "No divergence" golden. Test, commit.

```bash
git commit -m "test(lsp): headless vs LSP diagnostics byte-for-byte golden (arch invariant)"
```

### Task 6.3: Hover + completion + go-to-definition + references (arch LSP feature map)

**Files:**
- Create: `crates/lute-lsp/src/features/hover.rs`
- Create: `crates/lute-lsp/src/features/completion.rs`
- Create: `crates/lute-lsp/src/features/nav.rs`
- Modify: `crates/lute-lsp/src/backend.rs`
- Test: each feature file (`#[cfg(test)]`, pure functions over a parsed doc + snapshot)

**Interfaces:**
- Produces: `hover_at(&Document, &CapabilitySnapshot, pos) -> Option<Hover>` (directive/attr docs, `@ref` -> CEL def + type, state path -> type/default, enum docs, assetId -> catalog); `complete_at(...) -> Vec<CompletionItem>` (directive names, attr keys per schema, enum values, character ids, assetId, `@ref` names, state paths, choice ids in `<match on=>`); `definition_at(...)` (`@ref` -> def, state path -> decl, `scene.choices.<id>` -> `<branch id>`); `references_at(...)` (`@ref` uses, state reads/writes, choice id).

- [ ] **Step 1: Write failing tests (one per feature, pure)**

```rust
#[test]
fn completion_after_double_colon_lists_directives() {
    let items = complete_at(&parsed("## Shot 1.\n::"), &load_core_snapshot(), pos_after("::"));
    assert!(items.iter().any(|i| i.label == "camera"));
}
#[test]
fn hover_on_ref_shows_def_cel() {
    let h = hover_at(&parsed_with_def_fond(), &snap(), pos_on("@fond")).unwrap();
    assert!(h.contents_text().contains("scene.affect.bianca >= 1"));
}
#[test]
fn definition_on_choices_path_jumps_to_branch() {
    let loc = definition_at(&parsed_bianca(), &snap(), pos_on("scene.choices.number")).unwrap();
    assert_eq!(loc.line_of_target(), branch_number_line());
}
```

- [ ] **Step 2–5:** Implement each as a pure function keyed on cursor byte-offset -> AST node under cursor -> snapshot/schema lookup. Wire into `Backend`. Test, commit.

```bash
git commit -m "feat(lsp): hover/completion/definition/references (arch feature map)"
```

### Task 6.4: Folding + semantic tokens + document symbols

**Files:**
- Create: `crates/lute-lsp/src/features/folding.rs`
- Create: `crates/lute-lsp/src/features/semtok.rs`
- Create: `crates/lute-lsp/src/features/symbols.rs`
- Modify: `crates/lute-lsp/src/backend.rs`
- Test: each file

**Interfaces:**
- Produces: `folding_ranges(&Document) -> Vec<FoldingRange>` (`<...>` blocks, shots, per `<track>`); `semantic_tokens(&Document) -> Vec<SemanticToken>` (3 layers distinct + CEL sub-tokens + `@ref`s + state paths); `document_symbols(&Document) -> Vec<DocumentSymbol>` (shots, branches, matches).

- [ ] **Step 1–5:** Test-first per feature (assert fold count on the bianca example = shots + timeline + branch + match; assert a `::camera` token carries the staging layer; assert 5 shot symbols). Implement, wire, commit.

```bash
git commit -m "feat(lsp): folding + semantic tokens + document symbols"
```

---

# Phase 7 — tree-sitter grammar (editor-side)

### Task 7.1: grammar.js for the fixed DSL grammar (dsl §4–7)

**Files:**
- Create: `tree-sitter-lute/grammar.js`
- Create: `tree-sitter-lute/package.json`
- Create: `tree-sitter-lute/test/corpus/basic.txt` (tree-sitter corpus tests)

**Interfaces:**
- Produces: a tree-sitter grammar covering frontmatter fence, shot headings, `:line`, `::`-directives, `::set`, `<branch>/<choice>/<match>/<when>/<otherwise>/<timeline>/<track>` blocks, `/* */` comments, attribute lists. Editor-side only — NOT the authoritative AST.

- [ ] **Step 1: Write the failing corpus test**

`tree-sitter-lute/test/corpus/basic.txt`:
```
==================
directive with attrs
==================
::camera{focus="x" wait="true"}
---
(source_file (directive (ident) (attrs (attr (key) (string)) (attr (key) (string)))))
```

- [ ] **Step 2:** `cd tree-sitter-lute && tree-sitter generate && tree-sitter test` — FAIL (no grammar).
- [ ] **Step 3:** Write `grammar.js` with rules matching dsl §4.3 classification. Content text after `: ` is an opaque token to EOL (external scanner or a `token(prec(...))` regex). `<...>` blocks nest; `::` leaves do not.
- [ ] **Step 4:** `tree-sitter generate && tree-sitter test` — PASS.
- [ ] **Step 5: Commit**

```bash
git add tree-sitter-lute
git commit -m "feat(tree-sitter): grammar.js for fixed Lute grammar (dsl §4-7)"
```

### Task 7.2: highlight + fold queries, capabilityVersion stamp

**Files:**
- Create: `tree-sitter-lute/queries/highlights.scm`
- Create: `tree-sitter-lute/queries/folds.scm`
- Create: `tree-sitter-lute/queries/tags.scm`
- Modify: `tree-sitter-lute/package.json` (embed target `capabilityVersion` stamp field)
- Test: `tree-sitter-lute/test/corpus/highlight.txt`

**Interfaces:**
- Produces: highlight captures coloring the 3 layers distinctly (content/staging/logic) + CEL + `@ref` + state paths; fold captures for blocks/shots/tracks.

- [ ] **Step 1–5:** Corpus/highlight tests, write queries, stamp the grammar artifact with the `capabilityVersion` it targets (plugin §13). Verify `tree-sitter test`. Commit.

```bash
git commit -m "feat(tree-sitter): highlight/fold queries + capabilityVersion stamp"
```

---

## Deferred scope (explicitly out of this plan)

Recorded so they are not mistaken for gaps — each is genuinely out of the baseline `lute.core` LSP:

- **Rich `assetKind` decomposition (plugin §6.9):** segment compose/decompose, `resolve: query` catalogs, `fallback` hooks. Baseline `lute.core` treats `assetId` as an opaque `string` with the `PLACEHOLDER_*` exemption (dsl escape hatch). Segment-level validation/completion is a *plugin-manifest* concern; it lands when a plugin ships an `assetKinds` export. The Type system (T1.1) and provider loader (T1.6) already carry the primitives it needs.
- **Plugin *package loading from disk* (plugin §4):** the loader that reads `plugins/<id>/` dirs, sorts + merges files, and rejects duplicate ids. This plan ships only the built-in `lute.core` (embedded via `include_str!`, T1.6) + the resolution algorithm (T1.3). External plugin discovery is additive on top.
- **Localization / `textUnitId` assignment (dsl §12) and `lute tag`:** id-keyed sidecar tables and the `code` back-fill pass — separate tooling, not the checker.
- **Final `idola_script_commands` codegen** and **runtime CEL evaluation** — engine-owned (see Global Constraints).
- **Warm daemon** — a later optimization behind the same `check()`; not a second code path.

## Final verification (run once, after all phases)

- [ ] `cargo test --workspace` — all unit + integration + golden tests pass.
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` — clean.
- [ ] `cargo fmt --check` — formatted.
- [ ] `cd tree-sitter-lute && tree-sitter test` — grammar corpus passes.
- [ ] `lute check docs/examples/bianca-s01ep02.lute --json` and `... date-minigame.lute --json` — both `ok: true`.
- [ ] Divergence golden green (headless == LSP diagnostics).

---

## Notes for the executing engineer

- **Read the two proposals first** (`docs/proposals/scenario-dsl/0.0.1.md`, `docs/proposals/plugin-system/0.0.1.md`). Every `E-*`/`W-*` code and rule traces to a numbered section cited in the task.
- **The snapshot is the SoT.** Never hardcode a directive/enum in the checker — read it from `CapabilitySnapshot`. The built-in `lute.core` manifest (Task 1.6) is the only place baseline vocabulary lives.
- **CelSlot isolation:** a CEL parse failure MUST NOT abort DSL checking — it sets `ast: None` + one `layer: cel` diagnostic; surrounding DSL validation continues.
- **Determinism:** all maps that feed `capabilityVersion` or diagnostic ordering are `BTreeMap`/sorted. Diagnostics are sorted by `span.byte_start` before return (the divergence golden depends on it).
- **Scope discipline:** stop at the resolved view. Do not emit `idola_script_commands`.
