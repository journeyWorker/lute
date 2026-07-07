//! `textDocument/definition` + `textDocument/references` (Task 6.3).
//!
//! Pure functions over a parsed [`Document`] + [`CapabilitySnapshot`] + byte
//! offset. [`definition_at`] jumps:
//! - an `@ref` -> its `defs:` decl site (or `None` for a snapshot-only def, which
//!   has no in-document site);
//! - a state path -> its `state:` decl;
//! - `scene.choices.<id>` -> the declaring `<branch id=…>` node.
//!
//! [`references_at`] returns every use site:
//! - an `@ref` -> all `@name` uses across the document's CEL slots;
//! - a state / choice path -> every `::set` target + CEL occurrence.
//!
//! ## Returned spans are byte-only
//! Both return [`Span`]s whose byte offsets are authoritative; `line`/`column`/
//! `utf16_range` may be zero for spans synthesized from the frontmatter YAML. The
//! backend re-derives the `Range` from the bytes through its `TextIndex`, so the
//! reported positions match the headless surface to the code unit.

use lute_check::SchemaImports;
use lute_core_span::Span;
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_syntax::ast::{Document, InterpKind};

use super::{
    branch_span, choice_id, def_decl_span, interp_ref_name, is_state_path, path_at, path_uses,
    ref_at, ref_uses, state_decl_span, Cursor,
};

/// The declaration site of the symbol at byte offset `off`, or `None` when the
/// cursor is not on a navigable symbol (or the symbol has no in-document site — a
/// snapshot-only def, or a `uses:`-imported symbol declared in another file).
///
/// `imports` is threaded for API uniformity with the meta-driven features (the
/// backend feeds all four handlers from one `imports_for` resolver), but nav
/// resolves declaration sites from the document text alone: an imported symbol
/// (dsl §9.2) has no in-document site and no recorded span, so it degrades
/// gracefully to `None` — never a panic or a phantom span (best-effort,
/// local-only). Its in-document *uses* still surface through [`references_at`].
pub fn definition_at(
    doc: &Document,
    snapshot: &CapabilitySnapshot,
    _imports: &SchemaImports,
    off: usize,
) -> Option<Span> {
    let cursor = super::resolve(doc, off)?;
    match cursor {
        Cursor::SetPath { path, .. } => path_definition(doc, path),
        Cursor::Cel { slot, .. } => {
            if let Some(r) = ref_at(slot, off) {
                if r.is_dollar {
                    None
                } else {
                    def_decl_span(doc, &r.name)
                }
            } else if let Some((tok, _)) = path_at(slot, off) {
                path_definition(doc, &tok)
            } else {
                None
            }
        }
        Cursor::Interp(i) => match i.kind {
            // A state path jumps to its `state:`/`<branch>` decl, as a CEL path does.
            InterpKind::Path => path_definition(doc, &i.raw),
            // An `@ref` jumps to its def decl site (`@fond` → the `fond` key).
            InterpKind::Ref => interp_ref_name(&i.raw).and_then(|name| def_decl_span(doc, &name)),
            // A reserved token has no decl site.
            InterpKind::Reserved => None,
        },
        _ => {
            let _ = snapshot;
            None
        }
    }
}

/// A state/choice path's decl site: `scene.choices.<id>` -> the `<branch>` node;
/// any other declared state path -> its `state:` key.
fn path_definition(doc: &Document, path: &str) -> Option<Span> {
    if let Some(id) = choice_id(path) {
        if let Some(sp) = branch_span(doc, id) {
            return Some(sp);
        }
    }
    if is_state_path(path) {
        return state_decl_span(doc, path);
    }
    None
}

/// Every use site of the symbol at byte offset `off`. Empty when the cursor is not
/// on a referable symbol.
///
/// When `include_declaration` is true, the symbol's declaration site — the same
/// [`Span`] [`definition_at`] returns — is unioned into the result, deduped by
/// [`Span`] equality. When false, only the use-set is returned (byte-identical to
/// the historical behavior).
///
/// `imports` is forwarded to [`definition_at`] for the `include_declaration`
/// union; the in-document use-set itself is collected by scanning the body, so an
/// imported symbol's uses (dsl §9.2) surface without any schema lookup.
pub fn references_at(
    doc: &Document,
    snapshot: &CapabilitySnapshot,
    imports: &SchemaImports,
    off: usize,
    include_declaration: bool,
) -> Vec<Span> {
    let Some(cursor) = super::resolve(doc, off) else {
        return Vec::new();
    };
    let mut uses = match cursor {
        Cursor::SetPath { path, .. } => path_uses(doc, path),
        Cursor::Cel { slot, .. } => {
            if let Some(r) = ref_at(slot, off) {
                if r.is_dollar {
                    Vec::new()
                } else {
                    ref_uses(doc, &r.name)
                }
            } else if let Some((tok, _)) = path_at(slot, off) {
                path_uses(doc, &tok)
            } else {
                Vec::new()
            }
        }
        Cursor::Interp(i) => match i.kind {
            InterpKind::Path => path_uses(doc, &i.raw),
            InterpKind::Ref => interp_ref_name(&i.raw)
                .map(|name| ref_uses(doc, &name))
                .unwrap_or_default(),
            InterpKind::Reserved => Vec::new(),
        },
        _ => Vec::new(),
    };
    if include_declaration {
        if let Some(decl) = definition_at(doc, snapshot, imports, off) {
            if !uses.contains(&decl) {
                uses.push(decl);
            }
        }
    }
    uses
}

#[cfg(test)]
mod tests {
    use super::*;
    use lute_core_span::TextIndex;
    use lute_manifest::core::load_core_snapshot;
    use lute_syntax::parse;

    fn parsed(text: &str) -> Document {
        parse(text).0
    }

    /// 1-based source line of a byte offset (matches `Span.line`).
    fn line_of(text: &str, byte: usize) -> u32 {
        TextIndex::new(text).position(byte).line
    }

    /// A Bianca-shaped fixture: a `<branch id="number">` plus a `<match on=…>` on
    /// its folded choice path, and a `@fond` def used in one arm.
    const BIANCA: &str = "---\ncharacter: bianca\nseason: 1\nepisode: 2\nstate:\n  scene.affect.bianca: { type: number, default: 0 }\ndefs:\n  fond: { type: bool, cel: \"scene.affect.bianca >= 1\" }\n---\n## Shot 1.\n<branch id=\"number\">\n  <choice id=\"blunt\" label=\"Flat\">\n    :fixer: number.\n  </choice>\n  <choice id=\"soft\" label=\"Gentle\">\n    ::set{scene.affect.bianca += 1}\n  </choice>\n</branch>\n<match on=\"scene.choices.number\">\n  <when test=\"@fond\">\n    :fixer: gently.\n  </when>\n  <otherwise>\n    :fixer: bluntly.\n  </otherwise>\n</match>\n";

    #[test]
    fn definition_on_choices_path_jumps_to_branch() {
        let doc = parsed(BIANCA);
        // Cursor inside the `on="scene.choices.number"` subject path.
        let off = BIANCA.find("scene.choices.number").unwrap() + 5;
        let loc =
            definition_at(&doc, &load_core_snapshot(), &SchemaImports::default(), off).unwrap();
        let branch_line = line_of(BIANCA, BIANCA.find("<branch id=").unwrap());
        assert_eq!(line_of(BIANCA, loc.byte_start), branch_line);
    }

    #[test]
    fn definition_on_ref_jumps_to_def_decl() {
        let doc = parsed(BIANCA);
        let off = BIANCA.find("@fond").unwrap() + 1;
        let loc =
            definition_at(&doc, &load_core_snapshot(), &SchemaImports::default(), off).unwrap();
        let def_line = line_of(BIANCA, BIANCA.find("  fond:").unwrap() + 2);
        assert_eq!(line_of(BIANCA, loc.byte_start), def_line);
    }

    #[test]
    fn definition_on_state_path_jumps_to_decl() {
        let doc = parsed(BIANCA);
        // The `::set{scene.affect.bianca += 1}` target path.
        let set_at = BIANCA.find("::set{").unwrap();
        let off = BIANCA[set_at..].find("scene.affect.bianca").unwrap() + set_at + 2;
        let loc =
            definition_at(&doc, &load_core_snapshot(), &SchemaImports::default(), off).unwrap();
        let decl_line = line_of(BIANCA, BIANCA.find("  scene.affect.bianca:").unwrap() + 2);
        assert_eq!(line_of(BIANCA, loc.byte_start), decl_line);
    }

    #[test]
    fn references_on_ref_used_twice_returns_two() {
        let text = "---\ncharacter: bianca\nseason: 1\nepisode: 2\ndefs:\n  fond: { type: bool, cel: \"scene.x >= 1\" }\n---\n## Shot 1.\n<match on=\"scene.choices.number\">\n  <when test=\"@fond\">\n    :f: a.\n  </when>\n  <when test=\"@fond && true\">\n    :f: b.\n  </when>\n  <otherwise>\n    :f: c.\n  </otherwise>\n</match>\n";
        let doc = parsed(text);
        let off = text.find("@fond").unwrap() + 1; // on the first use
        let refs = references_at(
            &doc,
            &load_core_snapshot(),
            &SchemaImports::default(),
            off,
            false,
        );
        assert_eq!(refs.len(), 2, "two @fond uses: {refs:?}");
        // Both spans land on `@fond` occurrences.
        for r in &refs {
            assert_eq!(&text[r.byte_start..r.byte_end], "@fond");
        }
    }

    #[test]
    fn references_on_state_path_counts_set_and_reads() {
        let text = "---\ncharacter: bianca\nseason: 1\nepisode: 2\nstate:\n  scene.affect.bianca: { type: number, default: 0 }\n---\n## Shot 1.\n::set{scene.affect.bianca += 1}\n<match on=\"scene.affect.bianca\">\n  <otherwise>\n    :f: x.\n  </otherwise>\n</match>\n";
        let doc = parsed(text);
        let set_at = text.find("::set{").unwrap();
        let off = text[set_at..].find("scene.affect.bianca").unwrap() + set_at + 2;
        let refs = references_at(
            &doc,
            &load_core_snapshot(),
            &SchemaImports::default(),
            off,
            false,
        );
        // One `::set` target path + one `<match on=…>` CEL occurrence.
        assert_eq!(refs.len(), 2, "set + read: {refs:?}");
    }

    #[test]
    fn references_include_declaration_unions_decl_span() {
        let text = "---\ncharacter: bianca\nseason: 1\nepisode: 2\nstate:\n  scene.affect.bianca: { type: number, default: 0 }\n---\n## Shot 1.\n::set{scene.affect.bianca += 1}\n<match on=\"scene.affect.bianca\">\n  <otherwise>\n    :f: x.\n  </otherwise>\n</match>\n";
        let doc = parsed(text);
        let set_at = text.find("::set{").unwrap();
        let off = text[set_at..].find("scene.affect.bianca").unwrap() + set_at + 2;
        let snap = load_core_snapshot();
        // Baseline (=false): use-set only, declaration excluded — byte-identical to today.
        let uses = references_at(&doc, &snap, &SchemaImports::default(), off, false);
        assert_eq!(uses.len(), 2, "set + read only: {uses:?}");
        let decl = definition_at(&doc, &snap, &SchemaImports::default(), off)
            .expect("state path has a decl site");
        assert!(
            !uses.contains(&decl),
            "decl must be excluded when include_declaration=false: {uses:?}"
        );
        // =true: the decl span is unioned in (deduped) alongside the use-set.
        let with_decl = references_at(&doc, &snap, &SchemaImports::default(), off, true);
        assert_eq!(with_decl.len(), 3, "set + read + decl: {with_decl:?}");
        assert!(
            with_decl.contains(&decl),
            "decl span must be present when include_declaration=true: {with_decl:?}"
        );
    }

    #[test]
    fn definition_off_symbol_is_none() {
        let doc = parsed("## Shot 1.\n:narrator: just prose.\n");
        let off = 20; // in the prose
        assert!(
            definition_at(&doc, &load_core_snapshot(), &SchemaImports::default(), off).is_none()
        );
    }

    /// A snapshot with a `CH` assetKind (plugin §6.9 shape) and a `::portrait`
    /// directive whose `assetId` attr is typed `assetKind("CH")`.
    fn asset_snapshot() -> CapabilitySnapshot {
        use lute_manifest::schema::{
            AssetKindDecl, AssetResolve, AssetSegment, AttrDecl, DirectiveDecl, Lowering,
        };
        use lute_manifest::types::Type;
        let ch = AssetKindDecl {
            kind: "CH".to_string(),
            sep: ".".to_string(),
            resolve: AssetResolve::Compose,
            segments: vec![
                AssetSegment {
                    name: "prefix".to_string(),
                    r#const: Some("CH".to_string()),
                    ty: None,
                },
                AssetSegment {
                    name: "characterId".to_string(),
                    r#const: None,
                    ty: Some(Type::ProviderRef("character".to_string())),
                },
                AssetSegment {
                    name: "costume".to_string(),
                    r#const: None,
                    ty: Some(Type::Str),
                },
                AssetSegment {
                    name: "emotion".to_string(),
                    r#const: None,
                    ty: Some(Type::Enum(vec![
                        "delighted".to_string(),
                        "content".to_string(),
                        "neutral".to_string(),
                    ])),
                },
                AssetSegment {
                    name: "variant".to_string(),
                    r#const: None,
                    ty: Some(Type::Number),
                },
            ],
            provider: None,
            match_: Vec::new(),
            aliases: std::collections::BTreeMap::new(),
            fallback: Vec::new(),
            persistence: None,
        };
        let portrait = DirectiveDecl {
            name: "portrait".to_string(),
            layer: None,
            attrs: vec![AttrDecl {
                name: "assetId".to_string(),
                required: true,
                ty: Type::AssetKind("CH".to_string()),
                default: None,
            }],
            semantics: Vec::new(),
            state: None,
            effects: None,
            bridge: None,
            lower: Lowering::Builtin {
                kind: "builtin".to_string(),
                name: "portrait".to_string(),
            },
        };
        let mut snap = CapabilitySnapshot::default();
        snap.asset_kinds.insert("CH".to_string(), ch);
        snap.directives.insert("portrait".to_string(), portrait);
        snap
    }

    #[test]
    fn nav_asset_segment_none() {
        // Go-to-def on a providerRef asset segment resolves to no scene text —
        // provider decls are snapshot data, not document nodes — so it is `None`.
        let text = "## Shot 1.\n::portrait{assetId=\"CH.bianca.waitress.delighted.3\"}\n";
        let doc = parsed(text);
        let off = text.find("bianca").unwrap() + 1;
        assert!(definition_at(&doc, &asset_snapshot(), &SchemaImports::default(), off).is_none());
    }

    /// A directly-constructed `SchemaImports` (no disk): an imported `run.gold`
    /// state path and an imported `helped` def — exactly the shape `check()` sees
    /// after resolving a scene's `uses:` schema (dsl §9.2).
    fn schema_imports() -> lute_check::SchemaImports {
        use lute_check::{Namespace, SchemaImports, StateDecl};
        use lute_manifest::types::Type;
        let mut imports = SchemaImports::default();
        imports.state.decls.insert(
            "run.gold".to_string(),
            StateDecl {
                ty: Type::Number,
                default: None,
                namespace: Namespace::Run,
            },
        );
        imports.defs.insert(
            "helped".to_string(),
            serde_yaml::from_str("{ type: bool, cel: \"true\" }").unwrap(),
        );
        imports
    }

    #[test]
    fn references_on_imported_ref_returns_uses() {
        // `@helped` is only imported via `uses:` — its in-document uses are still
        // collected (nav scans the body; the declaration lives in another file).
        let text = "---\ncharacter: bianca\nseason: 1\nepisode: 2\n---\n## Shot 1.\n<match on=\"scene.choices.number\">\n  <when test=\"@helped\">\n    :f: a.\n  </when>\n  <when test=\"@helped && true\">\n    :f: b.\n  </when>\n  <otherwise>\n    :f: c.\n  </otherwise>\n</match>\n";
        let doc = parsed(text);
        let off = text.find("@helped").unwrap() + 1;
        let refs = references_at(&doc, &load_core_snapshot(), &schema_imports(), off, false);
        assert_eq!(refs.len(), 2, "two @helped uses surfaced: {refs:?}");
        for r in &refs {
            assert_eq!(&text[r.byte_start..r.byte_end], "@helped");
        }
    }

    #[test]
    fn references_on_imported_state_path_returns_uses() {
        // `run.gold` is only imported via `uses:` — its `::set` target + CEL read
        // are still collected.
        let text = "---\ncharacter: bianca\nseason: 1\nepisode: 2\n---\n## Shot 1.\n::set{run.gold += 1}\n<match on=\"run.gold\">\n  <otherwise>\n    :f: x.\n  </otherwise>\n</match>\n";
        let doc = parsed(text);
        let set_at = text.find("::set{").unwrap();
        let off = text[set_at..].find("run.gold").unwrap() + set_at + 2;
        let refs = references_at(&doc, &load_core_snapshot(), &schema_imports(), off, false);
        assert_eq!(refs.len(), 2, "set + read surfaced: {refs:?}");
    }

    #[test]
    fn definition_on_imported_symbol_is_none() {
        // An imported symbol has NO in-document declaration site (it lives in the
        // imported schema file, for which `SchemaImports` records no span), so
        // go-to-definition degrades gracefully to `None` — never a panic or a
        // phantom span (best-effort, local-only; dsl §9.2).
        let text = "---\ncharacter: bianca\nseason: 1\nepisode: 2\n---\n## Shot 1.\n<match on=\"scene.choices.number\">\n  <when test=\"@helped\">\n    :f: a.\n  </when>\n  <otherwise>\n    :f: b.\n  </otherwise>\n</match>\n";
        let doc = parsed(text);
        let off = text.find("@helped").unwrap() + 1;
        assert!(definition_at(&doc, &load_core_snapshot(), &schema_imports(), off).is_none());
    }

    /// A content line with all three interp kinds plus a `::set` on the same
    /// state path — for interp definition / references.
    const WITH_INTERPS: &str = "---\ncharacter: bianca\nseason: 1\nepisode: 2\nstate:\n  run.coins: { type: number, default: 0 }\ndefs:\n  fond: { type: bool, cel: \"run.coins >= 1\" }\n---\n## Shot 1.\n:bianca: Hi {{userName}}, {{run.coins}} — {{@fond}}.\n::set{run.coins += 1}\n";

    /// D1: go-to-definition inside `{{@fond}}` jumps to the `@fond` def decl.
    #[test]
    fn definition_on_interp_ref_jumps_to_def_decl() {
        let doc = parsed(WITH_INTERPS);
        let off = WITH_INTERPS.find("{{@fond}}").unwrap() + 2;
        let loc =
            definition_at(&doc, &load_core_snapshot(), &SchemaImports::default(), off).unwrap();
        let def_line = line_of(WITH_INTERPS, WITH_INTERPS.find("  fond:").unwrap() + 2);
        assert_eq!(line_of(WITH_INTERPS, loc.byte_start), def_line);
    }

    /// D1: go-to-definition inside `{{run.coins}}` jumps to the `state:` decl.
    #[test]
    fn definition_on_interp_path_jumps_to_state_decl() {
        let doc = parsed(WITH_INTERPS);
        let off = WITH_INTERPS.find("{{run.coins}}").unwrap() + 2;
        let loc =
            definition_at(&doc, &load_core_snapshot(), &SchemaImports::default(), off).unwrap();
        let decl_line = line_of(WITH_INTERPS, WITH_INTERPS.find("  run.coins:").unwrap() + 2);
        assert_eq!(line_of(WITH_INTERPS, loc.byte_start), decl_line);
    }

    /// D1: find-references on a state path counts the `{{run.coins}}` interp
    /// occurrence alongside the `::set` target use.
    #[test]
    fn references_on_state_path_include_interp() {
        let doc = parsed(WITH_INTERPS);
        let set_at = WITH_INTERPS.find("::set{").unwrap();
        let off = WITH_INTERPS[set_at..].find("run.coins").unwrap() + set_at + 2;
        let refs = references_at(&doc, &load_core_snapshot(), &SchemaImports::default(), off, false);
        assert!(
            refs.iter()
                .any(|r| &WITH_INTERPS[r.byte_start..r.byte_end] == "{{run.coins}}"),
            "the interp occurrence is among references: {refs:?}"
        );
        assert!(
            refs.iter()
                .any(|r| &WITH_INTERPS[r.byte_start..r.byte_end] == "run.coins"),
            "the ::set target use is still counted: {refs:?}"
        );
    }

    /// D1: find-references from inside `{{@fond}}` counts the interp among the
    /// `@fond` ref uses.
    #[test]
    fn references_on_interp_ref_include_interp() {
        let doc = parsed(WITH_INTERPS);
        let off = WITH_INTERPS.find("{{@fond}}").unwrap() + 2;
        let refs = references_at(&doc, &load_core_snapshot(), &SchemaImports::default(), off, false);
        assert!(
            refs.iter()
                .any(|r| &WITH_INTERPS[r.byte_start..r.byte_end] == "{{@fond}}"),
            "the @fond interp is among its own references: {refs:?}"
        );
    }
}
