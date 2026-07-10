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
    Arm, Attr, AttrValue, CelSlot, ClipNode, Directive, Document, Hub, Interp, InterpKind, Line,
    Node, Objective, On, Quest, Set,
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
    /// Inside a `{{…}}` content-line interpolation (dsl §7.6). Not a [`CelSlot`],
    /// so it carries the [`Interp`] itself; hover/nav resolve `interp.raw` as the
    /// equivalent state-path (`Path`) / `@ref` (`Ref`) referent, or note the
    /// reserved token (`Reserved`).
    Interp(&'a Interp),
    /// Inside a `<when is="…">` literal pattern value (dsl §7.3.1). Unlike a
    /// `test=` guard (CEL), the `is=` value is a `|`-alternation of literals over
    /// the enclosing `<match>` subject's FINITE domain; `subject_path` is that
    /// subject's raw path so hover/completion can derive the domain
    /// ([`subject_domain`]).
    IsPattern { subject_path: &'a str },
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
    /// On the `:speaker` name itself (dsl §7.1), between the leading `:` and
    /// the attrs `{`/the second `:` — NOT a capability-schema position (a
    /// content line's speaker has no directive), so it carries no data.
    /// Drives speaker-id completion (character/cast catalog ids + the
    /// `narrator` keyword); speaker-id VALIDATION stays out of scope for
    /// 0.2.1 (deferred to the 0.2.2 foundation minor).
    Speaker,
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
                    Arm::When { is, test, body, span } if span_contains(*span, off) => {
                        // Check `is` FIRST: an `is=`-only `<when>` gives `test` an
                        // empty slot spanning the WHOLE open tag (parser
                        // blocks.rs), which would otherwise swallow the `is=`
                        // cursor. The `is=` value is a literal pattern over the
                        // subject's finite domain (dsl §7.3.1), NOT CEL.
                        if let Some(pat) = is {
                            if span_contains(pat.span, off) {
                                return Some(Cursor::IsPattern {
                                    subject_path: m.subject.raw.as_str(),
                                });
                            }
                        }
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
    // The `:speaker{attrs}` head resolves first (attrs never overlap the text).
    if let Some(c) = resolve_attrs(&l.attrs, None, off) {
        return Some(c);
    }
    // The speaker NAME itself (dsl §7.1): `Line` carries no dedicated span for
    // it (only `span`/`text_span`), so it's derived the same way
    // `lute_check::tag::tag_scope` computes `speaker_end` — the ident runs
    // from just past the leading `:` for exactly `speaker.len()` bytes (an
    // ident holds no comments/whitespace, so no raw-text rescan is needed).
    let speaker_start = l.span.byte_start + 1;
    let speaker_end = speaker_start + l.speaker.len();
    if speaker_start <= off && off <= speaker_end {
        return Some(Cursor::Speaker);
    }
    // A `{{…}}` interpolation inside the content text (dsl §7.6).
    l.interps
        .iter()
        .find(|i| span_contains(i.span, off))
        .map(Cursor::Interp)
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
        Type::Domain(name) => format!("domain({name})"),
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

/// The enum domain of a directive attribute, following an inline `enum` type, an
/// `enumFromOption` indirection through `snapshot.enums`, or a `{domain: X}`-typed
/// attr (data-catalog foundation A5) resolved against the FULL merged vocabulary —
/// `snapshot.domains` ∪ `imports.domains`, via [`lute_check::schema_import::merge_domains`],
/// the SAME merge `check()` feeds its `Walker` (mirrors A4's checker resolution, so a
/// project-declared domain completes/hovers exactly like a plugin/core one). CLOSED
/// domains list their members; OPEN (registry-minted, `Domain::open`) domains have no
/// static member list, so they resolve to `None` here (same as a non-enum attr: honest,
/// never fabricated) — same for a project/core domain-name collision (`merge_domains`'s
/// `E-DOMAIN-DUP` diag is dropped; that surface is `check()`'s job, not hover/completion's).
/// `None` for non-enum, non-domain, or open-domain attrs.
pub(crate) fn attr_enum_values(
    snapshot: &CapabilitySnapshot,
    imports: &lute_check::SchemaImports,
    directive: &str,
    key: &str,
) -> Option<Vec<String>> {
    let decl = snapshot.directive(directive)?;
    let attr = decl.attrs.iter().find(|a| a.name == key)?;
    match &attr.ty {
        Type::Enum(members) => Some(members.clone()),
        Type::EnumFromOption(name) => snapshot.enums.get(name).cloned(),
        Type::Domain(name) => {
            let zero_span = Span { byte_start: 0, byte_end: 0, line: 1, column: 1, utf16_range: (0, 0) };
            let (merged, _) = lute_check::schema_import::merge_domains(snapshot, imports, zero_span);
            merged.get(name).filter(|d| !d.open).map(|d| d.members.clone())
        }
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
            Node::Hub(h) => {
                for c in &h.choices {
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

/// The finite value domain a `<when is="…">` literal may draw from (dsl §7.3.1),
/// reproducing the checker's fold ([`lute_check`]'s `infer_domain` over the
/// branch/hub-folded schema) from `doc` + the merged `meta` alone — the LSP
/// handlers carry no full `CheckInput`. Three cases, mirroring the checker:
/// - `scene.choices.<id>` -> the matching `<branch id>`/`<hub id>`'s choice ids ∪
///   `unset` (the implicit recording enum, §11.1/§11.1.3); with NO matching
///   branch/hub it falls through to the schema-decl case below, since the
///   checker's fold (`enum_members` over the FOLDED schema) also folds an author
///   `state: scene.choices.<id>` enum into a finite domain;
/// - `scene.visited.<hubId>.<choiceId>` -> the folded per-choice bool: `true` /
///   `false` / `unset` (§9.6, §11.1.3);
/// - else the merged `meta` schema decl's type: `Enum(members)` -> members ∪
///   `unset`; `Bool` -> `true`/`false`/`unset`; anything else -> `None`.
///
/// `None` for a non-path subject (a compound `on=` expression) or a path with no
/// finite domain — the checker's INFINITE case (requires `<otherwise>`, no `is=`
/// menu). Shared by hover + completion so the two never diverge.
pub(crate) fn subject_domain(
    doc: &Document,
    meta: &lute_check::TypedMeta,
    subject_path: &str,
) -> Option<Vec<String>> {
    // Reconstruct the subject's dotted path EXACTLY as the checker does
    // (`lute_check::match_check::subject_path` = `lute_cel::parse_slot` +
    // `cel_paths::select_path`): only a pure `Ident`/`Select` chain is a
    // finite-domain subject. A compound expression (`isSet(run.x)`, `a == b`) or
    // a hyphenated `on=` (`scene.choices.pick-one`, which cel-parser reads as
    // subtraction) reconstructs to `None` — the checker's INFINITE treatment — so
    // the LSP never offers a domain `check()` does not fold. The raw byte-scan
    // wrongly accepted `-` (a `is_path_byte`) here, diverging from the checker.
    let path = subject_reconstructed_path(subject_path)?;
    let path = path.as_str();
    // Case 1: `scene.choices.<id>` -> the branch/hub's choice ids ∪ `unset` when a
    // `<branch>`/`<hub>` declares `id`. When NONE does, fall through to case 3: the
    // checker's fold (`infer_domain` -> `enum_members` over the FOLDED schema) also
    // folds an author `state: scene.choices.<id>: { enum: … }` decl into a finite
    // domain, so an unmatched `branch_choice_ids` must NOT early-return `None` —
    // that would hide a domain `check()` treats as finite (§11.1, no-divergence).
    if let Some(id) = path.strip_prefix("scene.choices.") {
        if let Some(mut members) = branch_choice_ids(doc, id) {
            members.push("unset".to_string());
            return Some(members);
        }
    }
    // Case 2: `scene.visited.<hubId>.<choiceId>` -> the folded bool ∪ `unset`.
    if is_visited_bool(doc, path) {
        return Some(bool_domain());
    }
    // Case 3: the declared state decl's finite type.
    match &meta.state.decls.get(path)?.ty {
        Type::Enum(members) => {
            let mut vals = members.clone();
            vals.push("unset".to_string());
            Some(vals)
        }
        Type::Bool => Some(bool_domain()),
        _ => None,
    }
}

/// Reconstruct the `<match on=…>` subject's dotted path the SAME way the checker
/// does (`lute_check::match_check::subject_path` = `lute_cel::parse_slot` +
/// `cel_paths::select_path`): a pure `Ident`/`Select` chain becomes `a.b.c`,
/// anything else (a compound expression, or a hyphenated `pick-one` that
/// cel-parser reads as subtraction) yields `None`. The LSP handlers carry no
/// `CheckInput`, so this re-parses `raw` into a throwaway [`lute_cel::CelArena`]
/// exactly as the checker's `parse_expr` does; an empty/malformed subject -> `None`.
fn subject_reconstructed_path(raw: &str) -> Option<String> {
    if raw.trim().is_empty() {
        return None;
    }
    let mut arena = lute_cel::CelArena::default();
    let handle = lute_cel::parse_slot(&mut arena, raw, 0).ok()?;
    let root = arena.get(handle)?;
    select_path(&root.expr)
}

/// Verbatim mirror of `lute_check::cel_paths::select_path`: the dotted path of a
/// pure `Ident`/`Select` chain, or `None` if the chain bottoms out in anything
/// but a bare `Ident`. Kept byte-identical to the checker so the offered
/// `<when is>` domain never diverges from the checker's subject reconstruction.
fn select_path(expr: &cel_parser::ast::Expr) -> Option<String> {
    use cel_parser::ast::Expr;
    match expr {
        Expr::Ident(name) => Some(name.clone()),
        Expr::Select(sel) => {
            let base = select_path(&sel.operand.expr)?;
            Some(format!("{base}.{}", sel.field))
        }
        _ => None,
    }
}

/// `true`/`false`/`unset` — the finite domain of a `bool` (or `scene.visited.*`)
/// `<when is>` subject; `unset` is a valid `is=` literal (§7.3.1).
fn bool_domain() -> Vec<String> {
    vec!["true".to_string(), "false".to_string(), "unset".to_string()]
}

/// The `id` attribute of a `<hub>` (a string literal), if present.
fn hub_decl_id(h: &Hub) -> Option<&str> {
    h.attrs.iter().find(|a| a.key == "id").and_then(|a| match &a.value {
        AttrValue::Str(s) => Some(s.as_str()),
        _ => None,
    })
}

/// The direct choice ids of the `<branch id>` / `<hub id>` named `id`, searched
/// depth-first through nested bodies (a branch/hub may live in a match arm or
/// another choice body — the D2 recursion). Mirrors the checker's implicit
/// `scene.choices.<id>` enum (`check_branch`/`check_hub`: members = the choice
/// ids in document order). On a DUPLICATE id the checker's fold
/// (`fold_branches_nodes` -> `schema.decls.insert`) keeps the LAST declaration
/// (even while it emits `E-DUP-BRANCH`), so this returns the LAST matching
/// branch/hub's choice ids, not the first. `None` when no branch/hub declares
/// `id` (the checker's unfolded -> INFINITE -> no-domain case).
fn branch_choice_ids(doc: &Document, id: &str) -> Option<Vec<String>> {
    let mut latest = None;
    for shot in &doc.shots {
        branch_choice_ids_nodes(&shot.body, id, &mut latest);
    }
    latest
}

/// Overwrite `latest` with each matching `<branch id>`/`<hub id>`'s choice ids in
/// the checker's pre-order fold order (visit the node, THEN recurse its choice /
/// match-arm bodies), so `latest` ends as the LAST matching decl. Mirrors
/// `fold_branches_nodes`' `schema.decls.insert`: a duplicate id folds last-wins
/// into `scene.choices.<id>`, so the offered domain matches the folded schema
/// rather than the first declaration.
fn branch_choice_ids_nodes(nodes: &[Node], id: &str, latest: &mut Option<Vec<String>>) {
    for node in nodes {
        match node {
            Node::Branch(b) => {
                if b.id == id {
                    *latest = Some(b.choices.iter().map(|c| c.id.clone()).collect());
                }
                for c in &b.choices {
                    branch_choice_ids_nodes(&c.body, id, latest);
                }
            }
            Node::Hub(h) => {
                if hub_decl_id(h) == Some(id) {
                    *latest = Some(h.choices.iter().map(|c| c.id.clone()).collect());
                }
                for c in &h.choices {
                    branch_choice_ids_nodes(&c.body, id, latest);
                }
            }
            Node::Match(m) => {
                for arm in &m.arms {
                    let body = match arm {
                        Arm::When { body, .. } | Arm::Otherwise { body, .. } => body,
                    };
                    branch_choice_ids_nodes(body, id, latest);
                }
            }
            _ => {}
        }
    }
}

/// True when `path` is a folded `scene.visited.<hubId>.<choiceId>` — some
/// `<hub id="hubId">` declares a `<choice id="choiceId">` (the per-choice bool of
/// §9.6/§11.1.3). Reproduces `check_hub`'s `scene.visited.<hubId>.<choiceId>`
/// decl WITHOUT the fold: only a real hub+choice yields the bool domain.
fn is_visited_bool(doc: &Document, path: &str) -> bool {
    let Some(rest) = path.strip_prefix("scene.visited.") else {
        return false;
    };
    // A folded visited leaf is exactly `<hubId>.<choiceId>` — both single tokens.
    let Some((hub, choice)) = rest.split_once('.') else {
        return false;
    };
    if choice.contains('.') {
        return false;
    }
    doc.shots
        .iter()
        .any(|s| visited_in_nodes(&s.body, hub, choice))
}

fn visited_in_nodes(nodes: &[Node], hub: &str, choice: &str) -> bool {
    for node in nodes {
        match node {
            Node::Hub(h) => {
                if hub_decl_id(h) == Some(hub) && h.choices.iter().any(|c| c.id == choice) {
                    return true;
                }
                if h.choices.iter().any(|c| visited_in_nodes(&c.body, hub, choice)) {
                    return true;
                }
            }
            Node::Branch(b) => {
                if b.choices.iter().any(|c| visited_in_nodes(&c.body, hub, choice)) {
                    return true;
                }
            }
            Node::Match(m) => {
                for arm in &m.arms {
                    let body = match arm {
                        Arm::When { body, .. } | Arm::Otherwise { body, .. } => body,
                    };
                    if visited_in_nodes(body, hub, choice) {
                        return true;
                    }
                }
            }
            _ => {}
        }
    }
    false
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

/// The def name an `InterpKind::Ref` interior refers to (`@fond` → `fond`,
/// `@fn(a)` → `fn`), via the same [`lute_cel::scan_refs`] tokenizer the CEL-slot
/// ref cursor ([`ref_at`]) uses. `None` when the interior holds no well-formed
/// `@ref` (the checker flags that; the feature degrades to no resolution).
pub(crate) fn interp_ref_name(raw: &str) -> Option<String> {
    scan_refs(raw).into_iter().find(|r| !r.is_dollar).map(|r| r.name)
}

/// The byte span of an interp's referent (`i.raw`) within its `{{…}}`, located in
/// `src`. `{{ run.coins }}` → the span of `run.coins` (interior whitespace and the
/// `{{`/`}}` brackets excluded). Falls back to the whole interp span when the raw
/// is empty or not found (a malformed interp — the checker flags it).
pub(crate) fn interp_referent_span(src: &str, i: &Interp) -> Span {
    let outer = &src[i.span.byte_start..i.span.byte_end];
    match outer.find(&i.raw) {
        Some(rel) if !i.raw.is_empty() => {
            byte_span(i.span.byte_start + rel, i.span.byte_start + rel + i.raw.len())
        }
        _ => i.span,
    }
}

/// Every document span at which `@name` is used (any `@ref` token whose name
/// matches), across all CEL slots plus content-line `{{@ref}}` interps. The
/// declaration site is NOT included — this is the use-set
/// (`textDocument/references` with `includeDeclaration = false`).
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
    for shot in &doc.shots {
        collect_line_interps(
            &shot.body,
            &|i| i.kind == InterpKind::Ref && interp_ref_name(&i.raw).as_deref() == Some(name),
            &mut out,
        );
    }
    out
}

/// Every document span at which the state/choice path `path` is used: `::set`
/// target paths, matching dotted tokens inside a CEL slot, plus content-line
/// `{{path}}` interps.
pub(crate) fn path_uses(doc: &Document, path: &str) -> Vec<Span> {
    let mut out = Vec::new();
    for shot in &doc.shots {
        collect_set_paths(&shot.body, path, &mut out);
        collect_line_interps(
            &shot.body,
            &|i| i.kind == InterpKind::Path && i.raw == path,
            &mut out,
        );
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
            Node::Hub(h) => {
                for c in &h.choices {
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

/// Push the whole-`{{…}}` span of every content-line interp for which `matches`
/// holds, descending into every body that can hold a content [`Line`]
/// (`<branch>`/`<hub>`/`<match>` choice + arm bodies). The interp AST records no
/// interior offset, so the whole interpolation span is the use-site. Timeline
/// tracks hold clips (directives/`::set`), never content lines, so they carry no
/// interps and are skipped.
fn collect_line_interps(nodes: &[Node], matches: &impl Fn(&Interp) -> bool, out: &mut Vec<Span>) {
    for node in nodes {
        match node {
            Node::Line(l) => {
                for i in &l.interps {
                    if matches(i) {
                        out.push(i.span);
                    }
                }
            }
            Node::Branch(b) => {
                for c in &b.choices {
                    collect_line_interps(&c.body, matches, out);
                }
            }
            Node::Hub(h) => {
                for c in &h.choices {
                    collect_line_interps(&c.body, matches, out);
                }
            }
            Node::Match(m) => {
                for arm in &m.arms {
                    let body = match arm {
                        Arm::When { body, .. } | Arm::Otherwise { body, .. } => body,
                    };
                    collect_line_interps(body, matches, out);
                }
            }
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

    /// D3: a cursor inside a `<when is="…">` value resolves to [`Cursor::IsPattern`]
    /// carrying the enclosing `<match>` subject path — NOT the empty `test` slot
    /// (which, for an `is=`-only `<when>`, spans the whole open tag). A cursor on a
    /// `test=` value still resolves to [`Cursor::Cel`] (regression guard).
    #[test]
    fn resolve_when_is_vs_test() {
        let text = "## Shot 1.\n<match on=\"scene.choices.pick\">\n<when is=\"a\">\n:f: x.\n</when>\n<when test=\"true\">\n:f: y.\n</when>\n<otherwise>\n:f: z.\n</otherwise>\n</match>\n";
        let (doc, _) = parse(text);
        let is_off = text.find("is=\"a\"").unwrap() + "is=\"".len();
        match resolve(&doc, is_off) {
            Some(Cursor::IsPattern { subject_path }) => {
                assert_eq!(subject_path, "scene.choices.pick", "carries the match subject");
            }
            other => panic!("expected Cursor::IsPattern, got {other:?}"),
        }
        let test_off = text.find("test=\"true\"").unwrap() + "test=\"".len();
        assert!(
            matches!(resolve(&doc, test_off), Some(Cursor::Cel { .. })),
            "a test= value is still a CEL slot"
        );
    }

    /// Plan D final-review no-divergence fix: an author-declared
    /// `scene.choices.<id>` enum in `state:` with NO matching `<branch>`/`<hub>`
    /// is a FINITE domain to the checker (`infer_domain` -> `enum_members` over
    /// the folded schema, which folds author `state:` decls under
    /// `scene.choices.*`), so an exhaustive `<match>` is accepted. `subject_domain`
    /// must offer that SAME domain (members ∪ `unset`) instead of early-returning
    /// `None` when `branch_choice_ids` finds no branch/hub — else hover/completion
    /// diverge from what `check()` folds.
    #[test]
    fn subject_domain_authored_scene_choices_enum_without_branch() {
        let text = "---\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  scene.choices.manual: { type: { enum: [a, b] } }\n---\n## Shot 1.\n<match on=\"scene.choices.manual\">\n<when is=\"a\">\n:narrator: x\n</when>\n<when is=\"b\">\n:narrator: y\n</when>\n<when is=\"unset\">\n:narrator: z\n</when>\n</match>\n";
        let (doc, _) = parse(text);
        let (meta, _) =
            lute_check::parse_meta(&doc.meta, &lute_manifest::snapshot::CapabilitySnapshot::default());
        // No `<branch>`/`<hub>` declares `manual`, so `branch_choice_ids` is None;
        // the domain must instead come from the author-declared enum (case 3).
        assert_eq!(
            subject_domain(&doc, &meta, "scene.choices.manual"),
            Some(vec!["a".to_string(), "b".to_string(), "unset".to_string()]),
            "authored scene.choices.* enum -> members ∪ unset (matches the checker's finite domain)"
        );
        // Divergence guard: the checker ACCEPTS the exhaustive match (folds the
        // author enum as a finite domain), so the LSP offering that domain is the
        // non-divergent behavior — not a domain `check()` never folds.
        let input = lute_check::CheckInput {
            text: text.to_string(),
            uri: "when_is_domain".into(),
            snapshot: lute_manifest::core::load_core_snapshot(),
            providers: lute_manifest::provider::ProviderSet::default(),
            mode: lute_check::Mode::Author,
            imports: lute_check::SchemaImports::default(),
            components: Default::default(),
        };
        let codes: Vec<String> = lute_check::check(&input)
            .diagnostics
            .into_iter()
            .map(|d| d.code)
            .collect();
        assert!(
            !codes.contains(&"E-NONEXHAUSTIVE".to_string()),
            "checker folds the author enum as finite + accepts the exhaustive match: {codes:?}"
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
