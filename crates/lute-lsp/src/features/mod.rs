//! Editor feature functions (Task 6.3) — the architecture's "LSP feature map".
//!
//! Four PURE functions, each keyed on a cursor **byte offset** into the parsed
//! document, resolve the innermost AST node/attr/CEL-slot under the cursor and
//! look it up in the capability surface:
//!
//! - [`hover::hover_at`] — directive/attr docs, `@ref` def, state type/default.
//! - [`completion::complete_at`] — directive names, attr keys, enum values, def
//!   names, state paths, `<match>` choice ids.
//! - [`nav::definition_at`] — `@ref` -> def, state path -> `state:` decl,
//!   `scene.choices.<id>` -> the `<branch>` node.
//! - [`nav::references_at`] — every use of an `@ref` / state path across the doc.
//!
//! ## Why byte offsets (not LSP `Position`)
//! The feature fns take a plain `byte_off: usize`, so their unit tests are a
//! parse + a `str::find` away. The LSP `Position` (0-based, UTF-16) <-> byte
//! conversion lives in the backend ([`crate::backend`]), where the document text
//! is in hand and a single [`lute_core_span::TextIndex`] owns the UTF-16 math —
//! the same index [`crate::convert`] uses, so no hand-rolled UTF-16 drift.
//!
//! ## Positions are byte-only, backed by `TextIndex`
//! [`nav`] returns [`Span`]s whose `byte_start`/`byte_end` are authoritative;
//! `line`/`column`/`utf16_range` are left zeroed for spans we synthesize from the
//! frontmatter YAML (a `Document` carries no `TextIndex`). The backend re-derives
//! every reported `Range` from the byte offsets through its `TextIndex` — exactly
//! the convention `lute_check`'s `cel_resolve::map_span` follows. [`hover`] and
//! [`completion`] carry no positions at all (hover's `range` is `None`; a
//! `CompletionItem` is label/kind/detail only), so they are pure LSP values.
//!
//! ## State/defs threading (incl. `uses:` imports, dsl §9.2)
//! State-path and `@ref` lookups need the typed frontmatter, so the meta-driven
//! feature fns ([`hover::hover_at`], [`completion::complete_at`]) call
//! [`lute_check::parse_meta`] on `doc.meta` internally (meta diagnostics are
//! dropped — the feature is best-effort and the diagnostic surface is `check()`'s
//! job), then merge the caller-resolved [`lute_check::SchemaImports`] via
//! [`merge_imports`] so the SAME imported state/defs `check()` validates are
//! visible to hover/completion (imported state wins on collision — mirroring
//! `check()`'s `E-STATE-REDECLARE` authority — while defs stay inline-wins).
//! `TypedMeta.state.decls` backs state hover/completion; `TypedMeta.defs`
//! (author-declared inline defs,
//! now unioned with imported defs) plus `snapshot.defs` (plugin-exported
//! [`DefDecl`]) back `@ref` hover/completion. [`nav`] resolves declaration SITES
//! from the document text, so an imported symbol (declared in another file, with
//! no in-document site) surfaces through its in-document *uses* (references),
//! while go-to-definition degrades gracefully to `None`.

pub mod completion;
pub mod folding;
pub mod hover;
pub mod nav;
pub mod semtok;
pub mod symbols;

use lute_cel::scan_refs;
use lute_core_span::Span;
use lute_manifest::schema::{AssetKindDecl, DefDecl};
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_manifest::types::{Literal, Type};
use lute_syntax::ast::{
    Arm, Attr, AttrValue, CelSlot, ClipNode, Directive, Document, Line, Node, Objective, On, Quest,
    Set,
};

/// Merge imported schema (dsl §9.2) into a document's typed frontmatter so the
/// editor features see the same state/defs as [`lute_check::check`]. Precedence
/// mirrors `check()`:
/// - **state** — the IMPORTED decl wins on key collision (an inline path with the
///   same key is overwritten). `check()` treats imported state as authoritative
///   and flags an inline redeclaration as `E-STATE-REDECLARE`, so the imported
///   type/default is what validates; the feature layer must surface that same
///   type. Inline-only paths (no imported collision) are untouched.
/// - **defs** — the INLINE decl wins on key collision (`or_insert_with` only adds
///   imported names not already declared inline). This is the def precedence
///   `check()` uses: plugin < imported < inline.
///
/// Called by each meta-driven feature fn immediately after `parse_meta`, so every
/// existing sub-helper (`state_hover`, `state_path_items`, `def_info`,
/// `def_items`) transparently resolves imported symbols with no further change.
pub(crate) fn merge_imports(meta: &mut lute_check::TypedMeta, imports: &lute_check::SchemaImports) {
    for (k, v) in &imports.state.decls {
        meta.state.decls.insert(k.clone(), v.clone());
    }
    for (k, v) in &imports.defs {
        meta.defs.entry(k.clone()).or_insert_with(|| v.clone());
    }
}

/// What the cursor is resting on, once the innermost containing construct is
/// resolved. Every feature dispatches on this; the lifetime borrows the parsed
/// [`Document`] so no AST is cloned.
#[derive(Debug)]
pub(crate) enum Cursor<'a> {
    /// The `::name` head of a directive (or a bare `::` with an empty tag). Drives
    /// directive-name completion and directive hover.
    DirectiveName(&'a str),
    /// Inside a directive's `{ … }` but not on any single attribute (whitespace /
    /// empty braces). Drives attr-key completion; no hover.
    DirectiveAttrArea { directive: &'a str },
    /// On an attribute KEY. `directive` is the owning directive tag (`None` for a
    /// `:speaker{…}` line attribute, which has no capability schema).
    AttrKey {
        directive: Option<&'a str>,
        key: &'a str,
    },
    /// On an attribute VALUE (a plain string, not an `@ref`). Drives enum-value
    /// completion/hover when the owning directive's attr is enum-typed.
    AttrValue {
        directive: Option<&'a str>,
        key: &'a str,
    },
    /// Inside a CEL slot. `in_match_subject` is set for a `<match on=…>` subject
    /// (so completion offers `scene.choices.<id>` ids).
    Cel {
        slot: &'a CelSlot,
        in_match_subject: bool,
    },
    /// On a `::set` target path (a state-path position).
    SetPath { path: &'a str },
    /// On an `<on event="…">` EVENT value (dsl 0.2.0 §4) — a plain lifecycle
    /// or capability-declared world event name, NOT CEL. Drives event-name
    /// completion/hover.
    OnEventValue(&'a str),
    /// Inside a `<quest>`/`<on>`/`<objective>` open tag but not on any
    /// specific attr (dsl 0.2.0 §6.3/§4/§6.4) — whitespace, the bare
    /// keyword, or about to type a new attribute. Unlike `::directive`,
    /// these three constructs have a small FIXED attr set, not a
    /// capability-schema one, so completion/hover key off `construct`
    /// instead of a directive name.
    ConstructAttrArea { construct: QuestConstruct },
}

/// Which 0.2.0 quest-kind construct (dsl 0.2.0 §4/§6.3/§6.4) a
/// [`Cursor::ConstructAttrArea`] belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum QuestConstruct {
    Quest,
    On,
    Objective,
}

/// Resolve the innermost construct containing `off` (a byte offset into the
/// original document text). Walks shots -> bodies -> nested `<branch>`/`<match>`/
/// `<timeline>` bodies, AND `<quest>`s -> bodies -> nested `<on>`/`<objective>`
/// bodies (dsl 0.2.0 §6.3), descending into attributes and CEL slots. `None`
/// when the offset lands on structural trivia (headings, whitespace, the
/// frontmatter).
pub(crate) fn resolve(doc: &Document, off: usize) -> Option<Cursor<'_>> {
    for shot in &doc.shots {
        if span_contains(shot.span, off) {
            if let Some(c) = resolve_nodes(&shot.body, off) {
                return Some(c);
            }
        }
    }
    for quest in &doc.quests {
        if span_contains(quest.span, off) {
            if let Some(c) = resolve_quest(quest, off) {
                return Some(c);
            }
        }
    }
    None
}

/// Resolve a cursor inside a `<quest>` (dsl 0.2.0 §6.3): its header CEL guards
/// (`start`/`fail`), then its body, then its own residual attrs — falling
/// back to a construct attr-area cursor (mirrors `resolve_node`'s
/// `Node::On`/`Node::Objective` handling below; `Quest` is not a [`Node`], so
/// it needs its own entry point). Always `Some` — the caller only descends
/// here when `off` is within the quest's span.
fn resolve_quest(q: &Quest, off: usize) -> Option<Cursor<'_>> {
    if let Some(s) = &q.start {
        if span_contains(s.span, off) {
            return Some(Cursor::Cel {
                slot: s,
                in_match_subject: false,
            });
        }
    }
    if let Some(fl) = &q.fail {
        if span_contains(fl.span, off) {
            return Some(Cursor::Cel {
                slot: fl,
                in_match_subject: false,
            });
        }
    }
    if let Some(c) = resolve_nodes(&q.body, off) {
        return Some(c);
    }
    if let Some(c) = resolve_attrs(&q.attrs, None, off) {
        return Some(c);
    }
    Some(Cursor::ConstructAttrArea {
        construct: QuestConstruct::Quest,
    })
}

/// First node in `nodes` whose span contains `off`, resolved to its finest hit.
fn resolve_nodes(nodes: &[Node], off: usize) -> Option<Cursor<'_>> {
    for node in nodes {
        if span_contains(node_span(node), off) {
            return resolve_node(node, off);
        }
    }
    None
}

fn resolve_node(node: &Node, off: usize) -> Option<Cursor<'_>> {
    match node {
        Node::Directive(d) => Some(resolve_directive(d, off)),
        Node::Line(l) => resolve_line(l, off),
        Node::Set(s) => resolve_set(s, off),
        Node::Branch(b) => {
            for choice in &b.choices {
                if span_contains(choice.span, off) {
                    if let Some(when) = &choice.when {
                        if span_contains(when.span, off) {
                            return Some(Cursor::Cel {
                                slot: when,
                                in_match_subject: false,
                            });
                        }
                    }
                    if let Some(c) = resolve_attrs(&choice.attrs, None, off) {
                        return Some(c);
                    }
                    return resolve_nodes(&choice.body, off);
                }
            }
            resolve_attrs(&b.attrs, None, off)
        }
        Node::Match(m) => {
            if span_contains(m.subject.span, off) {
                return Some(Cursor::Cel {
                    slot: &m.subject,
                    in_match_subject: true,
                });
            }
            for arm in &m.arms {
                match arm {
                    Arm::When { is: _, test, body, span } if span_contains(*span, off) => {
                        if span_contains(test.span, off) {
                            return Some(Cursor::Cel {
                                slot: test,
                                in_match_subject: false,
                            });
                        }
                        return resolve_nodes(body, off);
                    }
                    Arm::Otherwise { body, span } if span_contains(*span, off) => {
                        return resolve_nodes(body, off);
                    }
                    _ => {}
                }
            }
            None
        }
        Node::Timeline(t) => {
            if let Some(dur) = &t.duration {
                if span_contains(dur.span, off) {
                    return Some(Cursor::Cel {
                        slot: dur,
                        in_match_subject: false,
                    });
                }
            }
            for track in &t.tracks {
                if span_contains(track.span, off) {
                    for clip in &track.clips {
                        if span_contains(clip.span, off) {
                            return match &clip.node {
                                ClipNode::Directive(d) => Some(resolve_directive(d, off)),
                                ClipNode::Set(s) => resolve_set(s, off),
                            };
                        }
                    }
                }
            }
            None
        }
        Node::Hub(h) => {
            for choice in &h.choices {
                if span_contains(choice.span, off) {
                    if let Some(when) = &choice.when {
                        if span_contains(when.span, off) {
                            return Some(Cursor::Cel {
                                slot: when,
                                in_match_subject: false,
                            });
                        }
                    }
                    if let Some(c) = resolve_attrs(&choice.attrs, None, off) {
                        return Some(c);
                    }
                    return resolve_nodes(&choice.body, off);
                }
            }
            resolve_attrs(&h.attrs, None, off)
        }
        Node::On(o) => Some(resolve_on(o, off)),
        Node::Objective(ob) => Some(resolve_objective(ob, off)),
    }
}

/// An `<on>` resolves to: its `when` guard, its `event` value, a hit inside
/// its body, a residual attr, or — as the final fallback — the construct
/// attr-area (dsl 0.2.0 §4). Always `Some`, mirroring `resolve_directive`.
fn resolve_on(o: &On, off: usize) -> Cursor<'_> {
    if let Some(w) = &o.when {
        if span_contains(w.span, off) {
            return Cursor::Cel {
                slot: w,
                in_match_subject: false,
            };
        }
    }
    if span_contains(o.event_span, off) {
        return Cursor::OnEventValue(&o.event);
    }
    if let Some(c) = resolve_nodes(&o.body, off) {
        return c;
    }
    if let Some(c) = resolve_attrs(&o.attrs, None, off) {
        return c;
    }
    Cursor::ConstructAttrArea {
        construct: QuestConstruct::On,
    }
}

/// An `<objective>` resolves to: its `done` predicate, its `when` guard, a
/// hit inside its body (empty for the self-closing form), a residual attr, or
/// — as the final fallback — the construct attr-area (dsl 0.2.0 §6.4). Always
/// `Some`, mirroring `resolve_directive`.
fn resolve_objective(ob: &Objective, off: usize) -> Cursor<'_> {
    if span_contains(ob.done.span, off) {
        return Cursor::Cel {
            slot: &ob.done,
            in_match_subject: false,
        };
    }
    if let Some(w) = &ob.when {
        if span_contains(w.span, off) {
            return Cursor::Cel {
                slot: w,
                in_match_subject: false,
            };
        }
    }
    if let Some(c) = resolve_nodes(&ob.body, off) {
        return c;
    }
    if let Some(c) = resolve_attrs(&ob.attrs, None, off) {
        return c;
    }
    Cursor::ConstructAttrArea {
        construct: QuestConstruct::Objective,
    }
}

/// A directive resolves to one of: its `::name` head, a specific attribute, or
/// the attr area (inside the braces but between attributes). Always `Some` — the
/// caller only descends here when `off` is within the directive span.
fn resolve_directive(d: &Directive, off: usize) -> Cursor<'_> {
    // `::` + tag: the head runs from span start through the end of the tag ident.
    let tag_end = d.span.byte_start + 2 + d.tag.len();
    if off <= tag_end {
        return Cursor::DirectiveName(&d.tag);
    }
    if let Some(c) = resolve_attrs(&d.attrs, Some(&d.tag), off) {
        return c;
    }
    Cursor::DirectiveAttrArea { directive: &d.tag }
}

fn resolve_line(l: &Line, off: usize) -> Option<Cursor<'_>> {
    resolve_attrs(&l.attrs, None, off)
}

fn resolve_set(s: &Set, off: usize) -> Option<Cursor<'_>> {
    if span_contains(s.path_span, off) {
        return Some(Cursor::SetPath { path: &s.path });
    }
    if span_contains(s.expr.span, off) {
        return Some(Cursor::Cel {
            slot: &s.expr,
            in_match_subject: false,
        });
    }
    None
}

/// Locate `off` within an attribute list: an `@ref` value -> [`Cursor::Cel`], a
/// plain value -> [`Cursor::AttrValue`], otherwise the key -> [`Cursor::AttrKey`].
fn resolve_attrs<'a>(
    attrs: &'a [Attr],
    directive: Option<&'a str>,
    off: usize,
) -> Option<Cursor<'a>> {
    for attr in attrs {
        if !span_contains(attr.span, off) {
            continue;
        }
        if let AttrValue::Ref(slot) = &attr.value {
            if span_contains(slot.span, off) {
                return Some(Cursor::Cel {
                    slot,
                    in_match_subject: false,
                });
            }
        }
        if span_contains(attr.value_span, off) {
            return Some(Cursor::AttrValue {
                directive,
                key: &attr.key,
            });
        }
        return Some(Cursor::AttrKey {
            directive,
            key: &attr.key,
        });
    }
    None
}

/// Original-text span of any [`Node`].
fn node_span(node: &Node) -> Span {
    match node {
        Node::Line(l) => l.span,
        Node::Directive(d) => d.span,
        Node::Set(s) => s.span,
        Node::Branch(b) => b.span,
        Node::Match(m) => m.span,
        Node::Timeline(t) => t.span,
        Node::Hub(h) => h.span,
        Node::On(o) => o.span,
        Node::Objective(o) => o.span,
    }
}

/// Half-open by construction but end-inclusive here, so a cursor resting at the
/// very end of a construct (`::` at EOF, an empty `{}`) still resolves inside it.
/// Nodes are newline-separated, so the inclusive end never bleeds into a sibling.
fn span_contains(span: Span, off: usize) -> bool {
    span.byte_start <= off && off <= span.byte_end
}

// -- CEL sub-resolution -------------------------------------------------------

/// The `@ref` / `$` token at `off` within `slot`, if any. Reuses
/// [`lute_cel::scan_refs`] over the RAW slot text (never re-parsing CEL) and maps
/// its slot-relative span into the document by [`CelSlot::span`]'s start byte —
/// the same mapping `lute_check::cel_resolve` performs.
fn ref_at(slot: &CelSlot, off: usize) -> Option<lute_cel::RefUse> {
    let base = slot.span.byte_start;
    scan_refs(&slot.raw)
        .into_iter()
        .find(|r| base + r.span.byte_start <= off && off <= base + r.span.byte_end)
}

/// The maximal dotted path token (`[A-Za-z0-9_.-]+`) surrounding `off` within
/// `slot`, plus its document-relative span. Used for state-path / choice-path
/// resolution when the cursor is not on an `@ref`.
///
/// A cursor inside a CEL string literal (§4.4) resolves to no path — the dotted
/// text there is literal content, not a state path. This reuses the shared
/// [`lute_cel::cel_string_mask`] (the same quote-tracking `scan_refs` uses for
/// @ref/$), so DSL-token and state-path scanning agree on string boundaries.
fn path_at(slot: &CelSlot, off: usize) -> Option<(String, Span)> {
    let base = slot.span.byte_start;
    if off < base {
        return None;
    }
    let local = off - base;
    let b = slot.raw.as_bytes();
    if local > b.len() {
        return None;
    }
    let mask = lute_cel::cel_string_mask(&slot.raw);
    // A path token contains no quotes, so it never straddles a string boundary:
    // if the cursor byte is string content, there is no path here.
    if local < b.len() && mask[local] {
        return None;
    }
    let mut start = local;
    while start > 0 && is_path_byte(b[start - 1]) && !mask[start - 1] {
        start -= 1;
    }
    let mut end = local;
    while end < b.len() && is_path_byte(b[end]) && !mask[end] {
        end += 1;
    }
    if start == end {
        return None;
    }
    Some((
        slot.raw[start..end].to_string(),
        byte_span(base + start, base + end),
    ))
}

/// A byte permitted in a CEL path token: an ident byte or `.`.
fn is_path_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b'.'
}

// -- capability lookups (shared across features) ------------------------------

/// Resolved info for an `@ref` def: its CEL text, a rendered type label, and any
/// parameters. Author-declared inline `defs:` (from [`lute_check::parse_meta`])
/// win over plugin-exported [`DefDecl`]s; either may be absent.
pub(crate) struct DefInfo {
    pub cel: String,
    pub ty: Option<String>,
    pub params: Vec<(String, String)>,
}

/// Look up `@name` in the author's inline `defs:` first, then the snapshot's
/// exported defs. Returns `None` for an unknown ref (graceful — no placeholder).
pub(crate) fn def_info(
    name: &str,
    defs: &std::collections::BTreeMap<String, serde_yaml::Value>,
    snapshot: &CapabilitySnapshot,
) -> Option<DefInfo> {
    if let Some(v) = defs.get(name) {
        let cel = v
            .get("cel")
            .and_then(|c| c.as_str())
            .unwrap_or_default()
            .to_string();
        let ty = v.get("type").and_then(yaml_type_label);
        return Some(DefInfo {
            cel,
            ty,
            params: Vec::new(),
        });
    }
    snapshot.defs.get(name).map(def_decl_info)
}

fn def_decl_info(d: &DefDecl) -> DefInfo {
    DefInfo {
        cel: d.cel.clone(),
        ty: Some(type_label(&d.ty)),
        params: d
            .params
            .iter()
            .map(|p| (p.name.clone(), type_label(&p.ty)))
            .collect(),
    }
}

/// Best-effort label for a `type:` value expressed as raw YAML (inline `defs:`).
fn yaml_type_label(v: &serde_yaml::Value) -> Option<String> {
    match v {
        serde_yaml::Value::String(s) => Some(s.clone()),
        serde_yaml::Value::Mapping(_) => serde_yaml::from_value::<Type>(v.clone())
            .ok()
            .map(|t| type_label(&t)),
        _ => None,
    }
}

/// A compact, human-readable rendering of a capability [`Type`] for hover/detail.
pub(crate) fn type_label(ty: &Type) -> String {
    match ty {
        Type::Bool => "bool".to_string(),
        Type::Number => "number".to_string(),
        Type::Str => "string".to_string(),
        Type::Enum(members) => format!("enum [{}]", members.join(", ")),
        Type::List(inner) => format!("list<{}>", type_label(inner)),
        Type::Record(_) => "record".to_string(),
        Type::Map { key, value } => format!("map<{}, {}>", type_label(key), type_label(value)),
        Type::EnumFromOption(name) => format!("enum(option:{name})"),
        Type::ProviderRef(name) => format!("providerRef({name})"),
        Type::SlotId { namespace } => format!("slotId({namespace})"),
        Type::AssetKind(k) => format!("assetKind({k})"),
    }
}

/// A compact rendering of a [`Literal`] default for hover/detail.
pub(crate) fn literal_label(lit: &Literal) -> String {
    match lit {
        Literal::Bool(b) => b.to_string(),
        Literal::Num(n) => n.to_string(),
        Literal::Str(s) => format!("\"{s}\""),
        Literal::List(items) => {
            format!(
                "[{}]",
                items
                    .iter()
                    .map(literal_label)
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }
        Literal::Map(m) => {
            format!(
                "{{{}}}",
                m.iter()
                    .map(|(k, v)| format!("{k}: {}", literal_label(v)))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }
    }
}

/// The enum domain of a directive attribute, following an inline `enum` type or an
/// `enumFromOption` indirection through `snapshot.enums`. `None` for non-enum attrs.
pub(crate) fn attr_enum_values(
    snapshot: &CapabilitySnapshot,
    directive: &str,
    key: &str,
) -> Option<Vec<String>> {
    let decl = snapshot.directive(directive)?;
    let attr = decl.attrs.iter().find(|a| a.name == key)?;
    match &attr.ty {
        Type::Enum(members) => Some(members.clone()),
        Type::EnumFromOption(name) => snapshot.enums.get(name).cloned(),
        _ => None,
    }
}

// -- asset-id segment lookups (plugin §6.9) -----------------------------------

/// The `&Attr` whose *value span* contains `off`, walking the same node tree
/// [`resolve`] uses (shots -> bodies -> nested `<branch>`/`<match>`/`<timeline>`).
/// `None` when `off` is not inside any attribute value. Lets the asset-segment
/// features recover the raw id text without changing the shared [`Cursor`].
pub(crate) fn attr_at(doc: &Document, off: usize) -> Option<&Attr> {
    fn in_attrs(attrs: &[Attr], off: usize) -> Option<&Attr> {
        attrs.iter().find(|a| span_contains(a.value_span, off))
    }
    fn scan(nodes: &[Node], off: usize) -> Option<&Attr> {
        for node in nodes {
            if !span_contains(node_span(node), off) {
                continue;
            }
            match node {
                Node::Directive(d) => return in_attrs(&d.attrs, off),
                Node::Line(l) => return in_attrs(&l.attrs, off),
                Node::Set(_) => return None,
                Node::Branch(b) => {
                    for c in &b.choices {
                        if span_contains(c.span, off) {
                            return in_attrs(&c.attrs, off).or_else(|| scan(&c.body, off));
                        }
                    }
                    return in_attrs(&b.attrs, off);
                }
                Node::Match(m) => {
                    for arm in &m.arms {
                        let (body, span) = match arm {
                            Arm::When { body, span, .. } | Arm::Otherwise { body, span } => {
                                (body, *span)
                            }
                        };
                        if span_contains(span, off) {
                            return scan(body, off);
                        }
                    }
                    return None;
                }
                Node::Timeline(t) => {
                    for track in &t.tracks {
                        if span_contains(track.span, off) {
                            for clip in &track.clips {
                                if span_contains(clip.span, off) {
                                    if let ClipNode::Directive(d) = &clip.node {
                                        return in_attrs(&d.attrs, off);
                                    }
                                }
                            }
                        }
                    }
                    return None;
                }
                Node::Hub(h) => {
                    for c in &h.choices {
                        if span_contains(c.span, off) {
                            return in_attrs(&c.attrs, off).or_else(|| scan(&c.body, off));
                        }
                    }
                    return in_attrs(&h.attrs, off);
                }
                Node::On(o) => return in_attrs(&o.attrs, off).or_else(|| scan(&o.body, off)),
                Node::Objective(ob) => {
                    return in_attrs(&ob.attrs, off).or_else(|| scan(&ob.body, off))
                }
            }
        }
        None
    }
    for shot in &doc.shots {
        if span_contains(shot.span, off) {
            if let Some(a) = scan(&shot.body, off) {
                return Some(a);
            }
        }
    }
    for quest in &doc.quests {
        if span_contains(quest.span, off) {
            if let Some(a) = in_attrs(&quest.attrs, off) {
                return Some(a);
            }
            if let Some(a) = scan(&quest.body, off) {
                return Some(a);
            }
        }
    }
    None
}

/// If the attr `key` on directive `directive` is declared `Type::AssetKind(name)`,
/// return the resolved kind decl from the snapshot. `None` for any other type or
/// an unknown directive/attr. The kind model (sep + segments) comes ENTIRELY from
/// this decl — the same datum the checker and `asset::decompose` use.
pub(crate) fn asset_kind_for<'a>(
    snapshot: &'a CapabilitySnapshot,
    directive: &str,
    key: &str,
) -> Option<&'a AssetKindDecl> {
    let decl = snapshot.directive(directive)?;
    let adecl = decl.attrs.iter().find(|a| a.name == key)?;
    if let Type::AssetKind(name) = &adecl.ty {
        snapshot.asset_kinds.get(name)
    } else {
        None
    }
}

/// 0-based segment index of byte offset `off` within an authored id whose value
/// starts at document byte `value_start`: the count of `sep` occurrences before
/// the cursor. Total (never panics); sep-boundary safe via `match_indices`.
pub(crate) fn asset_segment_index(
    kind: &AssetKindDecl,
    value: &str,
    value_start: usize,
    off: usize,
) -> usize {
    let rel = off.saturating_sub(value_start);
    value
        .match_indices(kind.sep.as_str())
        .filter(|(i, _)| *i < rel)
        .count()
}

// -- state / def / branch decl sites ------------------------------------------

/// `scene.choices.<id>` -> `id`, matching the implicit branch-folded state path.
pub(crate) fn choice_id(path: &str) -> Option<&str> {
    let rest = path.strip_prefix("scene.choices.")?;
    Some(rest.split('.').next().unwrap_or(rest))
}

/// True for a state-tier path (dsl §9.1 namespaces).
pub(crate) fn is_state_path(path: &str) -> bool {
    ["scene.", "run.", "user.", "app."]
        .iter()
        .any(|t| path.starts_with(t))
}

/// Document span of the `<branch id=…>` declaring `id`, searched depth-first
/// through nested bodies (a branch may live in a match arm / another choice).
pub(crate) fn branch_span(doc: &Document, id: &str) -> Option<Span> {
    doc.shots
        .iter()
        .find_map(|s| branch_span_nodes(&s.body, id))
        .or_else(|| doc.quests.iter().find_map(|q| branch_span_nodes(&q.body, id)))
}

fn branch_span_nodes(nodes: &[Node], id: &str) -> Option<Span> {
    for node in nodes {
        match node {
            Node::Branch(b) => {
                if b.id == id {
                    return Some(b.span);
                }
                for c in &b.choices {
                    if let Some(sp) = branch_span_nodes(&c.body, id) {
                        return Some(sp);
                    }
                }
            }
            Node::Match(m) => {
                for arm in &m.arms {
                    let body = match arm {
                        Arm::When { body, .. } | Arm::Otherwise { body, .. } => body,
                    };
                    if let Some(sp) = branch_span_nodes(body, id) {
                        return Some(sp);
                    }
                }
            }
            Node::On(o) => {
                if let Some(sp) = branch_span_nodes(&o.body, id) {
                    return Some(sp);
                }
            }
            Node::Objective(ob) => {
                if let Some(sp) = branch_span_nodes(&ob.body, id) {
                    return Some(sp);
                }
            }
            _ => {}
        }
    }
    None
}

/// Document span of a `state:` decl key inside the frontmatter YAML, or a `defs:`
/// key for an `@ref`. Returns a byte-only span (line/col recomputed by the
/// backend's `TextIndex`, per the module contract).
pub(crate) fn state_decl_span(doc: &Document, path: &str) -> Option<Span> {
    find_yaml_key_span(&doc.meta.raw_yaml, path)
}

pub(crate) fn def_decl_span(doc: &Document, name: &str) -> Option<Span> {
    find_yaml_key_span(&doc.meta.raw_yaml, name)
}

/// Find the mapping key `key` in the peeled frontmatter YAML and return its
/// document-relative byte span. `raw_yaml` starts at document byte 4 (`peel`
/// consumes the leading `---\n`), so a local offset maps by `+4`. Matches a key
/// only when it is immediately followed by `:` (or whitespace then `:`), so
/// `scene.affect.bianca` does not match `scene.affect.bianca_2`.
fn find_yaml_key_span(raw_yaml: &str, key: &str) -> Option<Span> {
    if raw_yaml.is_empty() {
        return None;
    }
    const FRONTMATTER_BASE: usize = 4; // len("---\n")
    let mut line_start = 0usize;
    for line in raw_yaml.split_inclusive('\n') {
        let trimmed_leading = line.len() - line.trim_start().len();
        let content = line.trim_start();
        if let Some(rest) = content.strip_prefix(key) {
            let rest = rest.trim_start();
            if rest.starts_with(':') {
                let key_start = line_start + trimmed_leading;
                let doc_start = FRONTMATTER_BASE + key_start;
                return Some(byte_span(doc_start, doc_start + key.len()));
            }
        }
        line_start += line.len();
    }
    None
}

// -- reference collection -----------------------------------------------------

/// Every document span at which `@name` is used (any `@ref` token whose name
/// matches), across all CEL slots. The declaration site is NOT included — this is
/// the use-set (`textDocument/references` with `includeDeclaration = false`).
pub(crate) fn ref_uses(doc: &Document, name: &str) -> Vec<Span> {
    let mut out = Vec::new();
    for slot in all_slots(doc) {
        let base = slot.span.byte_start;
        for r in scan_refs(&slot.raw) {
            if !r.is_dollar && r.name == name {
                out.push(byte_span(base + r.span.byte_start, base + r.span.byte_end));
            }
        }
    }
    out
}

/// Every document span at which the state/choice path `path` is used: `::set`
/// target paths plus every matching dotted token inside a CEL slot.
pub(crate) fn path_uses(doc: &Document, path: &str) -> Vec<Span> {
    let mut out = Vec::new();
    for shot in &doc.shots {
        collect_set_paths(&shot.body, path, &mut out);
    }
    for quest in &doc.quests {
        collect_set_paths(&quest.body, path, &mut out);
    }
    for slot in all_slots(doc) {
        let base = slot.span.byte_start;
        for (tok, sp) in path_tokens(&slot.raw) {
            if tok == path {
                out.push(byte_span(base + sp.0, base + sp.1));
            }
        }
    }
    out.sort_by_key(|s| s.byte_start);
    out
}

fn collect_set_paths(nodes: &[Node], path: &str, out: &mut Vec<Span>) {
    for node in nodes {
        match node {
            Node::Set(s) if s.path == path => out.push(s.path_span),
            Node::Branch(b) => {
                for c in &b.choices {
                    collect_set_paths(&c.body, path, out);
                }
            }
            Node::Match(m) => {
                for arm in &m.arms {
                    let body = match arm {
                        Arm::When { body, .. } | Arm::Otherwise { body, .. } => body,
                    };
                    collect_set_paths(body, path, out);
                }
            }
            Node::Timeline(t) => {
                for track in &t.tracks {
                    for clip in &track.clips {
                        if let ClipNode::Set(s) = &clip.node {
                            if s.path == path {
                                out.push(s.path_span);
                            }
                        }
                    }
                }
            }
            Node::On(o) => collect_set_paths(&o.body, path, out),
            Node::Objective(ob) => collect_set_paths(&ob.body, path, out),
            _ => {}
        }
    }
}

/// Maximal dotted path tokens (start-relative byte spans) in a raw CEL fragment.
///
/// A dotted token inside a CEL string literal (§4.4) is literal content, not a
/// state path, so it is skipped via the shared [`lute_cel::cel_string_mask`] (the
/// same quote-tracking `scan_refs`/`slot_tokens` use for @ref/$).
pub(crate) fn path_tokens(raw: &str) -> Vec<(String, (usize, usize))> {
    let b = raw.as_bytes();
    let mask = lute_cel::cel_string_mask(raw);
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if (b[i].is_ascii_alphabetic() || b[i] == b'_') && !mask[i] {
            let start = i;
            while i < b.len() && is_path_byte(b[i]) && !mask[i] {
                i += 1;
            }
            out.push((raw[start..i].to_string(), (start, i)));
        } else {
            i += 1;
        }
    }
    out
}

/// Every CEL slot in the document, in `lute_syntax::walk`'s canonical pre-order
/// (set exprs, `@ref`/CEL attr values, choice `when`, match subject + arm tests,
/// timeline durations, clip nodes).
pub(crate) fn all_slots(doc: &Document) -> Vec<&CelSlot> {
    let mut out = Vec::new();
    lute_syntax::walk::for_each_cel_slot(doc, &mut |s| out.push(s));
    out
}

/// Build a byte-only [`Span`]; `line`/`column`/`utf16_range` are recomputed by the
/// backend's `TextIndex` at report time (mirrors `lute_cel`/`cel_resolve`).
pub(crate) fn byte_span(start: usize, end: usize) -> Span {
    Span {
        byte_start: start,
        byte_end: end,
        line: 0,
        column: 0,
        utf16_range: (0, 0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lute_syntax::parse;

    /// S3 (dsl §4.4): a dotted path INSIDE a CEL string literal is literal text,
    /// not a state-path use. `path_tokens` must skip it (reusing the same
    /// quote-tracking `lute_cel::cel_string_mask` FE3 uses for @ref/$ scanning).
    #[test]
    fn path_tokens_skips_dotted_text_inside_cel_string() {
        // A real state path outside the string + a look-alike inside a literal.
        let toks = path_tokens("scene.affect.bianca == 'scene.affect.bianca'");
        let count = toks
            .iter()
            .filter(|(t, _)| t == "scene.affect.bianca")
            .count();
        assert_eq!(
            count, 1,
            "only the path outside the CEL string is a token, got {toks:?}"
        );
    }

    /// End-to-end: `references_at` on a match subject path must NOT count a
    /// same-spelled dotted string literal in a sibling arm test as a use.
    #[test]
    fn references_ignore_path_inside_cel_string_literal() {
        let text = "---\nkind: scene\ncharacter: bianca\nseason: 1\nepisode: 2\nstate:\n  scene.affect.bianca: { type: number, default: 0 }\n---\n## Shot 1.\n::set{scene.affect.bianca = 1}\n<match on=\"scene.affect.bianca\">\n<when test=\"'scene.affect.bianca' == 'x'\">\n:f: a.\n</when>\n<otherwise>\n:f: b.\n</otherwise>\n</match>\n";
        let (doc, _) = parse(text);
        // Cursor on the `::set` target path.
        let off = text.find("scene.affect.bianca = 1").unwrap();
        let uses = nav::references_at(
            &doc,
            &lute_manifest::core::load_core_snapshot(),
            &lute_check::SchemaImports::default(),
            off,
            false,
        );
        // Expected real uses: the ::set target + the `<match on=...>` subject. The
        // dotted text inside the `<when test="'scene.affect.bianca' == 'x'">`
        // string literal must NOT be counted.
        assert_eq!(
            uses.len(),
            2,
            "only the ::set target + match subject are uses (string literal excluded), got {uses:?}"
        );
    }

    // ---- dsl 0.2.0 §6.3/§4/§6.4: quest-body cursor resolution ----

    const QUEST_DOC: &str = "---\nkind: quest\n---\n\
        <quest id=\"q\" start=\"run.s\" fail=\"run.f\">\n\
        <objective id=\"o\" done=\"run.d\">\n\
        <branch id=\"b\">\n<choice id=\"c\" label=\"C\">\n::set{run.x = 1}\n</choice>\n</branch>\n\
        </objective>\n\
        <on event=\"questComplete\">\n:narrator: bye\n</on>\n\
        </quest>\n";

    /// ACCEPTANCE: before the fix, `resolve` walked `doc.shots` only (a quest
    /// doc has none), so EVERY cursor position in a quest doc resolved `None`.
    #[test]
    fn resolve_reaches_quest_start_fail_objective_done_and_on_event() {
        let (doc, _) = parse(QUEST_DOC);

        let start_off = QUEST_DOC.find("run.s").unwrap() + 1;
        assert!(
            matches!(resolve(&doc, start_off), Some(Cursor::Cel { .. })),
            "quest start= is a CEL cursor"
        );

        let fail_off = QUEST_DOC.find("run.f").unwrap() + 1;
        assert!(
            matches!(resolve(&doc, fail_off), Some(Cursor::Cel { .. })),
            "quest fail= is a CEL cursor"
        );

        let done_off = QUEST_DOC.find("run.d").unwrap() + 1;
        assert!(
            matches!(resolve(&doc, done_off), Some(Cursor::Cel { .. })),
            "objective done= is a CEL cursor"
        );

        let event_off = QUEST_DOC.find("questComplete").unwrap() + 1;
        assert!(
            matches!(resolve(&doc, event_off), Some(Cursor::OnEventValue(_))),
            "on event= is an OnEventValue cursor"
        );
    }

    /// A cursor resting inside a `<quest>`/`<on>`/`<objective>` open tag, on no
    /// specific attr, resolves to a construct attr-area cursor (drives
    /// attr-key completion + keyword hover) rather than `None`.
    #[test]
    fn resolve_quest_attr_area_is_some() {
        let (doc, _) = parse(QUEST_DOC);
        let off = QUEST_DOC.find("<quest ").unwrap() + 1;
        assert!(
            matches!(
                resolve(&doc, off),
                Some(Cursor::ConstructAttrArea {
                    construct: QuestConstruct::Quest
                })
            ),
            "got {:?}",
            resolve(&doc, off)
        );
    }

    /// `branch_span` finds a `<branch>` nested inside a quest's `<objective>`
    /// body (dsl 0.2.0 §6.7 nesting) — before the fix it walked `doc.shots`
    /// only.
    #[test]
    fn branch_span_reaches_into_quest_objective_body() {
        let (doc, _) = parse(QUEST_DOC);
        assert!(branch_span(&doc, "b").is_some());
    }

    /// `path_uses` (via `collect_set_paths`) finds a `::set` nested inside a
    /// quest's `<objective>`/`<branch>`/`<choice>` body — before the fix it
    /// walked `doc.shots` only.
    #[test]
    fn path_uses_reaches_into_quest_body() {
        let (doc, _) = parse(QUEST_DOC);
        let uses = path_uses(&doc, "run.x");
        assert!(
            uses.iter().any(|s| QUEST_DOC[s.byte_start..s.byte_end] == *"run.x"),
            "got {uses:?}"
        );
    }
}
