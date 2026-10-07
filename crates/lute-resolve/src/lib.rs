
//! Protocol-neutral cursor and symbol resolution shared by LSP and CLI.
//!
//! This crate deliberately has no editor or command-line dependencies. Byte
//! offsets are authoritative; consumers convert their wire coordinates at the
//! boundary with `TextIndex`.

pub mod cursor;
use std::collections::BTreeMap;
use std::path::PathBuf;

use lute_check::schema_import::SchemaImports;
use lute_core_span::{Span, TextIndex};
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_manifest::types::Type;
use lute_syntax::ast::{Attr, AttrValue, CelKind, CelSlot, Document, Node};
use lute_syntax::walk::for_each_cel_slot;
use lute_semantic::{NodeKey, NodeKind, SourceLocation};

#[derive(Clone, Copy, Debug)]
pub struct PositionQuery<'a> {
    pub document: &'a Document,
    pub source: &'a str,
    pub byte_offset: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cursor {
    pub kind: String,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Symbol {
    pub name: String,
    pub kind: String,
    pub ty: Option<Type>,
    pub declaration: Option<SourceLocation>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Resolution {
    pub cursor: Option<Cursor>,
    pub expected_type: Option<Type>,
    pub visible_symbols: Vec<Symbol>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResolveError {
    OutOfBounds,
    Unresolved(String),
    Invalid(String),
}

/// Resolve a byte cursor using the same typed frontmatter/import precedence as
/// the checker. The result is intentionally free of LSP types.
pub fn resolve_position(
    query: PositionQuery<'_>,
    imports: &SchemaImports,
    snapshot: &CapabilitySnapshot,
) -> Result<Resolution, ResolveError> {
    if query.byte_offset > query.source.len()
        || !query.source.is_char_boundary(query.byte_offset)
    {
        return Err(ResolveError::OutOfBounds);
    }
    let (mut meta, _) = lute_check::parse_meta(&query.document.meta, snapshot);
    // Imported state is authoritative. Inline defs are authoritative over
    // imported defs. This is deliberately the checker's precedence, not a CLI
    // convenience rule.
    for (path, decl) in &imports.state.decls {
        meta.state.decls.insert(path.clone(), decl.clone());
    }
    for (name, value) in &imports.defs {
        meta.defs.entry(name.clone()).or_insert_with(|| value.clone());
    }

    let slot = find_slot(query.document, query.byte_offset);
    let mut cursor = slot.map(|s| Cursor {
        kind: match s.kind {
            CelKind::Condition => "condition",
            CelKind::AttrValue => "attribute-value",
            CelKind::SetExpr => "set-expression",
            CelKind::MatchSubject => "match-subject",
        }
        .to_string(),
        span: s.span,
    });
    let mut expected = slot.and_then(|s| expected_for_slot(query.document, s, &meta, snapshot));

    // `::set` targets are outside the RHS CelSlot. Resolve their authored span
    // before the generic structural fallback.
    if let Some((span, path)) = find_set_path(query.document, query.byte_offset) {
        cursor = Some(Cursor { kind: "state-path".into(), span });
        expected = resolve_state_type(&meta.state.decls, &path);
    }
    if cursor.is_none() {
        cursor = find_attribute_or_construct(query.document, query.byte_offset);
    }
    let visible_symbols = symbols_for_cursor(
        query.source,
        query.byte_offset,
        slot,
        &meta,
        imports,
        snapshot,
    );
    Ok(Resolution {
        cursor,
        expected_type: expected,
        visible_symbols,
    })
}

fn find_slot<'a>(doc: &'a Document, off: usize) -> Option<&'a CelSlot> {
    let mut found = None;
    for_each_cel_slot(doc, &mut |slot| {
        if slot.span.byte_start <= off && off <= slot.span.byte_end {
            let replace = found
                .map(|old: &CelSlot| {
                    slot.span.byte_end.saturating_sub(slot.span.byte_start)
                        < old.span.byte_end.saturating_sub(old.span.byte_start)
                })
                .unwrap_or(true);
            if replace {
                found = Some(slot);
            }
        }
    });
    found
}

fn resolve_state_type(
    decls: &BTreeMap<String, lute_check::meta::StateDecl>,
    path: &str,
) -> Option<Type> {
    if let Some(decl) = decls.get(path) {
        return Some(decl.ty.clone());
    }
    // Nested record/map paths use the nearest declared ancestor, matching the
    // checker's set-op resolution for the common record/map case.
    let mut parts = path.split('.').collect::<Vec<_>>();
    while parts.len() > 1 {
        parts.pop();
        let parent = parts.join(".");
        let Some(decl) = decls.get(&parent) else { continue };
        let mut ty = decl.ty.clone();
        for segment in path.split('.').skip(parts.len()) {
            ty = match ty {
                Type::Record(fields) => fields
                    .into_iter()
                    .find(|field| field.name == segment)
                    .map(|field| field.ty)?,
                Type::Map { value, .. } => *value,
                _ => return None,
            };
        }
        return Some(ty);
    }
    None
}

fn expected_for_slot(
    doc: &Document,
    slot: &CelSlot,
    meta: &lute_check::TypedMeta,
    snapshot: &CapabilitySnapshot,
) -> Option<Type> {
    match slot.kind {
        CelKind::Condition => Some(Type::Bool),
        CelKind::SetExpr => find_set_for_slot(doc, slot)
            .and_then(|(_, path)| resolve_state_type(&meta.state.decls, &path)),
        CelKind::MatchSubject => path_type(&slot.raw, &meta.state.decls),
        CelKind::AttrValue => find_attr_for_slot(doc, slot).and_then(|(tag, key)| {
            snapshot
                .directive(&tag)
                .and_then(|decl| decl.attrs.iter().find(|attr| attr.name == key))
                .map(|attr| attr.ty.clone())
        }),
    }
}

fn path_type(raw: &str, decls: &BTreeMap<String, lute_check::meta::StateDecl>) -> Option<Type> {
    let path = raw.trim().trim_matches('"');
    if path.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b'.') {
        resolve_state_type(decls, path)
    } else {
        None
    }
}

fn find_set_for_slot<'a>(doc: &'a Document, slot: &CelSlot) -> Option<(Span, String)> {
    let mut answer = None;
    visit_nodes(doc, &mut |node| {
        if let Node::Set(set) = node {
            if std::ptr::eq(&set.expr, slot) {
                answer = Some((set.span, set.path.clone()));
            }
        }
    });
    answer
}

fn find_attr_for_slot(doc: &Document, slot: &CelSlot) -> Option<(String, String)> {
    let mut answer = None;
    visit_nodes(doc, &mut |node| {
        let attrs: Option<(&str, &[Attr])> = match node {
            Node::Directive(d) => Some((&d.tag, &d.attrs)),
            Node::Line(l) => Some(("", &l.attrs)),
            Node::Branch(b) => Some(("branch", &b.attrs)),
            Node::Hub(h) => Some(("hub", &h.attrs)),
            Node::Match(m) => Some(("match", &m.attrs)),
            Node::Objective(o) => Some(("objective", &o.attrs)),
            Node::On(o) => Some(("on", &o.attrs)),
            _ => None,
        };
        if let Some((tag, attrs)) = attrs {
            for attr in attrs {
                if let AttrValue::Ref(ref value) = attr.value {
                    if std::ptr::eq(value, slot) {
                        answer = Some((tag.to_string(), attr.key.clone()));
                    }
                }
            }
        }
    });
    answer
}

fn visit_nodes(doc: &Document, f: &mut impl FnMut(&Node)) {
    fn body(nodes: &[Node], f: &mut impl FnMut(&Node)) {
        for n in nodes {
            f(n);
            match n {
                Node::Branch(b) => b.choices.iter().for_each(|c| body(&c.body, f)),
                Node::Hub(h) => {
                    h.choices.iter().for_each(|c| body(&c.body, f));
                    if let Some(r) = &h.on_return { body(&r.body, f); }
                }
                Node::Match(m) => m.arms.iter().for_each(|arm| match arm {
                    lute_syntax::ast::Arm::When { body: arm_body, .. } => body(arm_body, f),
                    lute_syntax::ast::Arm::Otherwise { body: arm_body, .. } => body(arm_body, f),
                }),
                Node::Timeline(t) => t.tracks.iter().for_each(|tr| tr.clips.iter().for_each(|c| {
                    if let lute_syntax::ast::ClipNode::Directive(d) = &c.node { f(&Node::Directive(d.clone())); }
                    if let lute_syntax::ast::ClipNode::Set(s) = &c.node { f(&Node::Set(s.clone())); }
                })),
                Node::Objective(o) => body(&o.body, f),
                Node::On(o) => body(&o.body, f),
                _ => {}
            }
        }
    }
    for shot in &doc.sections { body(&shot.body, f); }
    for q in &doc.quests { body(&q.body, f); }
    for e in &doc.entries { body(&e.body, f); }
    for b in &doc.beats { body(&b.body, f); }
}

fn find_set_path(doc: &Document, off: usize) -> Option<(Span, String)> {
    let mut out = None;
    visit_nodes(doc, &mut |n| if let Node::Set(s) = n {
        if s.path_span.byte_start <= off && off <= s.path_span.byte_end {
            out = Some((s.path_span, s.path.clone()));
        }
    });
    out
}

fn find_attribute_or_construct(doc: &Document, off: usize) -> Option<Cursor> {
    let mut out = None;
    visit_nodes(doc, &mut |n| {
        let (kind, span) = match n {
            Node::Directive(d) => ("directive", d.span),
            Node::Line(l) => ("line", l.span),
            Node::Branch(b) => ("branch", b.span),
            Node::Match(m) => ("match", m.span),
            Node::Hub(h) => ("hub", h.span),
            Node::Objective(o) => ("objective", o.span),
            Node::On(o) => ("on", o.span),
            _ => return,
        };
        if span.byte_start <= off && off <= span.byte_end {
            let replace = out.as_ref().map(|c: &Cursor| span.byte_end - span.byte_start < c.span.byte_end - c.span.byte_start).unwrap_or(true);
            if replace { out = Some(Cursor { kind: kind.into(), span }); }
        }
    });
    out
}

fn symbols_for_cursor(
    source: &str,
    off: usize,
    slot: Option<&CelSlot>,
    meta: &lute_check::TypedMeta,
    imports: &SchemaImports,
    snapshot: &CapabilitySnapshot,
) -> Vec<Symbol> {
    let mut symbols = Vec::new();
    let before = source.get(..off).unwrap_or_default();
    let defs_only = before.ends_with('@')
        || before
            .rsplit_once('@')
            .map(|(_, tail)| tail.chars().all(|c| !c.is_whitespace()))
            .unwrap_or(false);
    if !defs_only {
        for (name, decl) in &meta.state.decls {
            symbols.push(Symbol {
                name: name.clone(),
                kind: "state".into(),
                ty: Some(decl.ty.clone()),
                declaration: declaration(source, name),
            });
        }
    }
    if defs_only || slot.is_none() {
        let mut def_values = imports.defs.clone();
        for (name, value) in &meta.defs { def_values.insert(name.clone(), value.clone()); }
        for (name, value) in def_values {
            let ty = value.get("type").cloned().and_then(|v| serde_yaml::from_value(v).ok());
            symbols.push(Symbol { name: name.clone(), kind: "def".into(), ty, declaration: declaration(source, &name) });
        }
        for (name, decl) in &snapshot.defs {
            if !symbols.iter().any(|s| s.name == *name) {
                symbols.push(Symbol { name: name.clone(), kind: "def".into(), ty: Some(decl.ty.clone()), declaration: declaration(source, name) });
            }
        }
    }
    symbols.sort_by(|a, b| {
        a.kind.cmp(&b.kind).then_with(|| a.name.cmp(&b.name)).then_with(|| {
            a.declaration.as_ref().map(|d| d.file.clone()).cmp(&b.declaration.as_ref().map(|d| d.file.clone()))
        })
    });
    symbols
}

fn declaration(source: &str, key: &str) -> Option<SourceLocation> {
    let needle = format!("{key}:");
    let start = source.find(&needle)?;
    let idx = TextIndex::new(source);
    Some(SourceLocation { file: PathBuf::new(), span: Span::from_bytes(&idx, start, start + key.len()) })
}

/// Parse the canonical `kind:key` form without coupling callers to graph
/// construction. Kept here so context and patch consumers share validation.
pub fn parse_node_key(raw: &str) -> Result<NodeKey, ResolveError> {
    let (kind, key) = raw.split_once(':').ok_or_else(|| ResolveError::Invalid("target must be kind:key".into()))?;
    let kind = match kind {
        "project" => NodeKind::Project,
        "document" => NodeKind::Document,
        "scene" => NodeKind::Scene,
        "shot" => NodeKind::Shot,
        "line" => NodeKind::Line,
        "choice" => NodeKind::Choice,
        "quest" => NodeKind::Quest,
        "objective" => NodeKind::Objective,
        "reward" => NodeKind::Reward,
        "entry" => NodeKind::Entry,
        "beat" => NodeKind::Beat,
        "occasion" => NodeKind::Occasion,
        "relation" => NodeKind::Relation,
        "state" => NodeKind::State,
        "def" => NodeKind::Def,
        "component" => NodeKind::Component,
        "expanded" => NodeKind::Expanded,
        "fact" => NodeKind::Fact,
        "clock" => NodeKind::Clock,
        "engine" => NodeKind::Engine,
        _ => return Err(ResolveError::Invalid(format!("unknown node kind `{kind}`"))),
    };
    if key.is_empty() { return Err(ResolveError::Invalid("target key is empty".into())); }
    Ok(NodeKey::new(kind, key))
}
