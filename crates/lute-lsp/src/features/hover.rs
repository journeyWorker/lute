//! `textDocument/hover` (Task 6.3): explain the construct under the cursor.
//!
//! A pure function over a parsed [`Document`] + [`CapabilitySnapshot`] + byte
//! offset. Resolves the cursor ([`super::resolve`]) and renders Markdown for:
//! - a directive `::name` -> its [`DirectiveDecl`] (layer, attrs w/ type+required,
//!   semantics);
//! - an attribute key -> its [`AttrDecl`] (type, required, default);
//! - an `@ref` -> the def's CEL text + type (author `defs:` first, then snapshot);
//! - a state path -> its `state:` decl type + default;
//! - an enum-typed attr value -> the enum domain.
//! - an `assetKind`-typed `assetId` value -> the segment under the cursor (its
//!   name, declared type, and authored value).
//!
//! A plain-string `assetId` (an attr NOT typed `assetKind`) or any other value
//! with no capability match yields `None`. `Hover.range` is `None`: highlighting the hovered
//! span is optional and would require the document text this pure fn does not hold.

use lute_check::parse_meta;
use lute_manifest::asset;
use lute_manifest::schema::AssetKindDecl;
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_syntax::ast::{AttrValue, Document};
use tower_lsp_server::ls_types::{Hover, HoverContents, MarkupContent, MarkupKind};

use super::{
    attr_enum_values, choice_id, def_info, is_state_path, literal_label, path_at, ref_at,
    type_label, Cursor,
};

/// Hover documentation for the construct at byte offset `off`, or `None` when the
/// cursor rests on something with no capability-backed explanation.
pub fn hover_at(doc: &Document, snapshot: &CapabilitySnapshot, off: usize) -> Option<Hover> {
    let (meta, _) = parse_meta(&doc.meta, snapshot);
    let cursor = super::resolve(doc, off)?;
    let md = match cursor {
        Cursor::DirectiveName(tag) => directive_hover(snapshot, tag),
        Cursor::AttrValue {
            directive: Some(dir),
            key,
        } => {
            // An `assetId` value documents the segment under the cursor; else an
            // enum attr documents its domain; else the attr's own declaration.
            if let Some(kind) = super::asset_kind_for(snapshot, dir, key) {
                asset_segment_hover(kind, doc, off)
            } else if let Some(vals) = attr_enum_values(snapshot, dir, key) {
                Some(format!("**enum** `{key}`\n\ndomain: {}", vals.join(", ")))
            } else {
                attr_hover(snapshot, dir, key)
            }
        }
        Cursor::AttrKey {
            directive: Some(dir),
            key,
        } => attr_hover(snapshot, dir, key),
        Cursor::SetPath { path, .. } => state_hover(&meta, path),
        Cursor::Cel { slot, .. } => {
            if let Some(r) = ref_at(slot, off) {
                if r.is_dollar {
                    Some("`$` — the `<match>` subject".to_string())
                } else {
                    ref_hover(&meta, snapshot, &r.name)
                }
            } else if let Some((tok, _)) = path_at(slot, off) {
                if is_state_path(&tok) {
                    state_hover(&meta, &tok).or_else(|| choice_hover(&tok))
                } else {
                    None
                }
            } else {
                None
            }
        }
        Cursor::DirectiveAttrArea { .. }
        | Cursor::AttrKey {
            directive: None, ..
        }
        | Cursor::AttrValue {
            directive: None, ..
        } => None,
    }?;
    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: md,
        }),
        range: None,
    })
}

/// Render a directive's declaration: name, layer, each attribute (type +
/// `required`), and its `semantics` vocabulary.
fn directive_hover(snapshot: &CapabilitySnapshot, tag: &str) -> Option<String> {
    let decl = snapshot.directive(tag)?;
    let mut s = format!("**::{}**", decl.name);
    if let Some(layer) = &decl.layer {
        s.push_str(&format!(" — layer `{layer}`"));
    }
    if !decl.attrs.is_empty() {
        s.push_str("\n\n**attributes:**");
        for a in &decl.attrs {
            let req = if a.required { " (required)" } else { "" };
            s.push_str(&format!("\n- `{}`: {}{req}", a.name, type_label(&a.ty)));
            if let Some(def) = &a.default {
                s.push_str(&format!(" = {}", literal_label(def)));
            }
        }
    }
    if !decl.semantics.is_empty() {
        s.push_str(&format!("\n\n**semantics:** {}", decl.semantics.join(", ")));
    }
    Some(s)
}

/// Render one attribute's declaration (type, required, default).
fn attr_hover(snapshot: &CapabilitySnapshot, directive: &str, key: &str) -> Option<String> {
    let decl = snapshot.directive(directive)?;
    let attr = decl.attrs.iter().find(|a| a.name == key)?;
    let req = if attr.required {
        "required"
    } else {
        "optional"
    };
    let mut s = format!("**`{key}`** ({req}): {}", type_label(&attr.ty));
    if let Some(def) = &attr.default {
        s.push_str(&format!("\n\ndefault: {}", literal_label(def)));
    }
    s.push_str(&format!("\n\non `::{directive}`"));
    Some(s)
}

/// Render the `assetId` segment under the cursor: its declared name plus type (a
/// const segment names its literal; a providerRef names its provider via
/// [`type_label`]). When the whole id decomposes against the kind, the segment's
/// current authored value is appended — the breakdown derived from the same
/// [`AssetKindDecl`] the checker uses, never a re-hardcoded vocabulary.
fn asset_segment_hover(kind: &AssetKindDecl, doc: &Document, off: usize) -> Option<String> {
    let attr = super::attr_at(doc, off)?;
    let AttrValue::Str(value) = &attr.value else {
        return None;
    };
    let idx = super::asset_segment_index(kind, value, attr.value_span.byte_start, off);
    let seg = kind.segments.get(idx)?;
    let mut s = format!("**{}**", seg.name);
    if let Some(c) = &seg.r#const {
        s.push_str(&format!(" — const `{c}`"));
    } else if let Some(ty) = &seg.ty {
        s.push_str(&format!(" — {}", type_label(ty)));
    }
    if let Ok(segs) = asset::decompose(kind, value) {
        if let Some(cur) = segs.get(idx) {
            s.push_str(&format!("\n\nvalue: `{}`", cur.value));
        }
    }
    Some(s)
}

/// Render a `state:` declaration (type + default).
fn state_hover(meta: &lute_check::TypedMeta, path: &str) -> Option<String> {
    let decl = meta.state.decls.get(path)?;
    let mut s = format!("**state** `{path}`: {}", type_label(&decl.ty));
    if let Some(def) = &decl.default {
        s.push_str(&format!("\n\ndefault: {}", literal_label(def)));
    }
    Some(s)
}

/// Render a `scene.choices.<id>` path when no state decl backs it (the implicit
/// branch-folded path): name the branch it resolves to.
fn choice_hover(path: &str) -> Option<String> {
    let id = choice_id(path)?;
    Some(format!(
        "**choice path** `{path}` — the chosen id of `<branch id=\"{id}\">`"
    ))
}

/// Render an `@ref` def: its CEL text, type, and any parameters.
fn ref_hover(
    meta: &lute_check::TypedMeta,
    snapshot: &CapabilitySnapshot,
    name: &str,
) -> Option<String> {
    let info = def_info(name, &meta.defs, snapshot)?;
    let mut s = format!("**@{name}**");
    if let Some(ty) = &info.ty {
        s.push_str(&format!(": {ty}"));
    }
    if !info.params.is_empty() {
        let ps = info
            .params
            .iter()
            .map(|(k, t)| format!("{k}: {t}"))
            .collect::<Vec<_>>();
        s.push_str(&format!("\n\nparams: {}", ps.join(", ")));
    }
    if !info.cel.is_empty() {
        s.push_str(&format!("\n\n```cel\n{}\n```", info.cel));
    }
    Some(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lute_manifest::core::load_core_snapshot;
    use lute_syntax::parse;

    /// Text of a `Markup` hover (the only variant `hover_at` emits).
    fn contents_text(h: &Hover) -> &str {
        match &h.contents {
            HoverContents::Markup(m) => &m.value,
            _ => panic!("expected markup hover"),
        }
    }

    fn parsed(text: &str) -> Document {
        parse(text).0
    }

    /// Byte offset just inside `needle` (on its first char) within `text`.
    fn pos_on(text: &str, needle: &str) -> usize {
        text.find(needle).expect("needle present") + 1
    }

    const WITH_DEF_FOND: &str = "---\ncharacter: bianca\nseason: 1\nepisode: 2\nstate:\n  scene.affect.bianca: { type: number, default: 0 }\ndefs:\n  fond: { type: bool, cel: \"scene.affect.bianca >= 1\" }\n---\n## Shot 1.\n<match on=\"scene.affect.bianca\">\n  <when test=\"@fond\">\n    :line[fixer]: gently.\n  </when>\n  <otherwise>\n    :line[fixer]: bluntly.\n  </otherwise>\n</match>\n";

    #[test]
    fn hover_on_ref_shows_def_cel() {
        let doc = parsed(WITH_DEF_FOND);
        let off = pos_on(WITH_DEF_FOND, "@fond");
        let h = hover_at(&doc, &load_core_snapshot(), off).unwrap();
        assert!(contents_text(&h).contains("scene.affect.bianca >= 1"));
    }

    #[test]
    fn hover_on_ref_shows_def_type() {
        let doc = parsed(WITH_DEF_FOND);
        let off = pos_on(WITH_DEF_FOND, "@fond");
        let h = hover_at(&doc, &load_core_snapshot(), off).unwrap();
        assert!(contents_text(&h).contains("bool"));
    }

    #[test]
    fn hover_on_directive_name_shows_attrs() {
        let text = "## Shot 1.\n::camera{focus=\"bianca\"}\n";
        let doc = parsed(text);
        let off = pos_on(text, "camera");
        let h = hover_at(&doc, &load_core_snapshot(), off).unwrap();
        let s = contents_text(&h);
        assert!(s.contains("::camera"), "names the directive: {s}");
        assert!(s.contains("focus"), "lists an attribute: {s}");
    }

    #[test]
    fn hover_on_state_path_shows_type_and_default() {
        let text = "---\ncharacter: bianca\nseason: 1\nepisode: 2\nstate:\n  scene.affect.bianca: { type: number, default: 3 }\n---\n## Shot 1.\n::set{scene.affect.bianca += 1}\n";
        let doc = parsed(text);
        // Cursor on the `::set` target path (first occurrence in the body).
        let body_start = text.find("::set{").unwrap();
        let off = text[body_start..].find("scene.affect.bianca").unwrap() + body_start + 2;
        let h = hover_at(&doc, &load_core_snapshot(), off).unwrap();
        let s = contents_text(&h);
        assert!(s.contains("number"), "shows the type: {s}");
        assert!(s.contains("3"), "shows the default: {s}");
    }

    #[test]
    fn hover_on_enum_attr_value_shows_domain() {
        let text = "## Shot 1.\n::auto{character=\"b\" anchor=\"center\"}\n";
        let doc = parsed(text);
        let off = pos_on(text, "center");
        let h = hover_at(&doc, &load_core_snapshot(), off).unwrap();
        let s = contents_text(&h);
        assert!(
            s.contains("left") && s.contains("right"),
            "shows enum domain: {s}"
        );
    }

    #[test]
    fn hover_on_unknown_ref_is_none() {
        let text = "## Shot 1.\n::set{scene.x = @nope}\n";
        let doc = parsed(text);
        let off = pos_on(text, "@nope");
        assert!(hover_at(&doc, &load_core_snapshot(), off).is_none());
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
    #[allow(non_snake_case)] // brief-specified name mirrors the `characterId` segment
    fn hover_characterId_segment() {
        // Cursor on `bianca` → segment idx 1 (characterId, providerRef character).
        let text = "## Shot 1.\n::portrait{assetId=\"CH.bianca.waitress.delighted.3\"}\n";
        let doc = parsed(text);
        let off = pos_on(text, "bianca");
        let h = hover_at(&doc, &asset_snapshot(), off).unwrap();
        let s = contents_text(&h);
        assert!(s.contains("characterId"), "names the segment: {s}");
        assert!(
            s.contains("providerRef(character)"),
            "names the provider type: {s}"
        );
    }
}
