//! `textDocument/completion` (Task 6.3): candidates for the cursor position.
//!
//! A pure function over a parsed [`Document`] (plus its source text) +
//! [`CapabilitySnapshot`] + byte offset. Resolves the cursor
//! ([`lute_resolve::cursor::resolve`]) and returns:
//! - after `::` (a directive head) -> directive names;
//! - inside a directive's `{ … }` at a key position -> that directive's attr keys
//!   (per its schema plus the universal `duration`/`delay`/`wait` timing keys,
//!   minus keys already present);
//! - at an enum- or domain-typed attr value -> the members (`framing`, `move`,
//!   `transition`, `playback`, `layout`, `name` of `::sequence`, …);
//! - a `::jump{to=…}` value -> the document's `::label{name=…}` names; a
//!   `::label{name=…}` value -> jump targets no label declares yet;
//! - inside a content line's `@speaker{…}` -> the content-line attr keys
//!   (delivery flags `mono`/`os`/`vo` select the line role), and the
//!   `emotion`/`action` domain members at their values;
//! - inside a `<branch>`/`<choice>`/`<match>` open tag -> that tag's keys
//!   (`text` on a choice, `subject` on a match);
//! - after `:` in content-line text -> inline modifier names (`pause`,
//!   `speed`, and the project's `textStyle` members);
//! - `@` in a CEL slot -> author `defs:` + snapshot def names;
//! - a `<match subject=…>` subject -> `scene.choices.<id>` ids from every `<branch>`;
//! - any other state-path position in CEL -> declared state paths.
//!
//! Empty result (`vec![]`) when nothing is offerable — never a placeholder item.

use std::collections::BTreeSet;

use lute_check::content_line::{CONTENT_LINE_DOMAIN_SLOTS, KNOWN_ATTRS};
use lute_check::directives::UNIVERSAL_TIMING_ATTRS;
use lute_check::{parse_meta, SchemaImports};
use lute_core_span::Span;
use lute_manifest::core::{CLEAR_DIRECTIVE, JUMP_DIRECTIVE, LABEL_DIRECTIVE, LABEL_NAME_ATTR};
use lute_manifest::provider::ProviderSet;
use lute_manifest::schema::AssetKindDecl;
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_manifest::types::Type;
use lute_syntax::ast::{Arm, AttrValue, Document, Line, Node, ACCEPT_DIRECTIVE};
use tower_lsp_server::ls_types::{CompletionItem, CompletionItemKind};

use lute_resolve::cursor::{
    attr_enum_values, asset_kind_for, asset_segment_index, attr_at, span_contains, subject_domain,
    type_label, Cursor, QuestConstruct,
};

/// Completion candidates at byte offset `off` of `src` (the text `doc` was
/// parsed from). Empty when the cursor is somewhere with nothing to offer.
pub fn complete_at(
    doc: &Document,
    src: &str,
    snapshot: &CapabilitySnapshot,
    providers: &ProviderSet,
    imports: &SchemaImports,
    off: usize,
) -> Vec<CompletionItem> {
    // `kind:` frontmatter value completion (dsl 0.2.0 §3.1) — `resolve()` is
    // BODY-only (it walks `doc.sections`/`doc.quests`, never the frontmatter
    // YAML), so this is a small dedicated detector, checked first.
    if let Some(items) = kind_value_items(doc, snapshot, off) {
        return items;
    }
    let (mut meta, _) = parse_meta(&doc.meta, snapshot);
    lute_resolve::cursor::merge_imports(&mut meta, imports);
    let cursor = lute_resolve::cursor::resolve(doc, off);
    // A `<branch>`/`<choice>`/`<match>` open tag has no cursor of its own (or
    // only a residual-attr key one): its fixed key set comes from the tag.
    // A bare attr (`@x{mo|}`, `::camera{fr|}`, `<choice … on|>`) is a key being
    // typed: its value span is its key span, so the resolver reports a value.
    let bare = attr_at(doc, off).is_some_and(|a| matches!(a.value, AttrValue::BoolTrue));
    let tag_key_position = match cursor {
        None | Some(Cursor::AttrKey { directive: None, .. }) => true,
        Some(Cursor::AttrValue { directive: None, .. }) => bare,
        // A `<match>` without `subject=` gets an empty subject slot spanning
        // its whole open tag.
        Some(Cursor::Cel {
            slot,
            in_match_subject: true,
        }) => {
            slot.raw.is_empty()
                && src
                    .get(slot.span.byte_start..)
                    .is_some_and(|s| s.starts_with("<match"))
        }
        _ => false,
    };
    if tag_key_position {
        if let Some((tag, inner)) = open_tag_at(doc, src, off) {
            return open_tag_items(tag, inner);
        }
    }
    let Some(cursor) = cursor else {
        return modifier_items(doc, snapshot, imports, &meta, off);
    };
    match cursor {
        Cursor::DirectiveName(_) => directive_items(snapshot),
        Cursor::DirectiveAttrArea { directive } => attr_key_items(snapshot, directive, doc, off),
        Cursor::AttrKey {
            directive: Some(dir),
            ..
        } => attr_key_items(snapshot, dir, doc, off),
        Cursor::AttrValue {
            directive: Some(dir),
            ..
        } if bare => attr_key_items(snapshot, dir, doc, off),
        Cursor::AttrValue {
            directive: Some(dir),
            key,
        } => {
            let permitted = snapshot.permissions.allows_directive(dir)
                && snapshot.directive(dir).is_some_and(|decl| {
                    decl.bridge.as_ref().is_none_or(|bridge| {
                        snapshot
                            .permissions
                            .allows_bridge(&bridge.service, &bridge.operation)
                    })
                });
            if !permitted {
                Vec::new()
            } else if dir == JUMP_DIRECTIVE && key == "to" {
                label_items(doc)
            } else if dir == LABEL_DIRECTIVE && key == LABEL_NAME_ATTR {
                unlabeled_target_items(doc)
            } else if let Some(kind) = asset_kind_for(snapshot, dir, key) {
                asset_segment_items(kind, doc, providers, off)
            } else {
                enum_value_items(snapshot, imports, &meta, dir, key)
            }
        }
        Cursor::Cel {
            slot,
            in_match_subject,
        } => {
            let base = slot.span.byte_start;
            let local = off.saturating_sub(base);
            if at_ref(&slot.raw, local) {
                def_items(&meta, snapshot)
            } else if in_match_subject {
                choice_path_items(doc)
            } else if let Some(items) = host_items(&slot.raw, local, doc) {
                items
            } else {
                state_path_items(&meta)
            }
        }
        // A content-line attr key/value (dsl §7.1's `@speaker{…}:` — no owning
        // directive/capability schema, unlike a `::directive`'s attrs).
        Cursor::AttrKey {
            directive: None, ..
        } => super::line_at(doc, off)
            .map(content_line_attr_key_items)
            .unwrap_or_default(),
        Cursor::AttrValue {
            directive: None,
            key,
        } => match super::line_at(doc, off) {
            Some(line) if bare => content_line_attr_key_items(line),
            Some(_) => content_line_attr_value_items(snapshot, imports, &meta, key),
            None => Vec::new(),
        },
        Cursor::SetPath { .. } => state_path_items(&meta),
        // Interp interiors (dsl §7.6) get hover/def/references (Task D1) but no
        // completion — a `{{…}}` referent is authored inline, matching the prior
        // behavior (interps resolved to no cursor before D1).
        Cursor::Interp(_) => Vec::new(),
        Cursor::IsPattern { subject_path } => is_pattern_items(doc, &meta, subject_path),
        Cursor::OnEventValue(_) => event_name_items(snapshot),
        Cursor::ConstructAttrArea { construct }
            if !snapshot.permissions.allows_quests()
                && matches!(construct, QuestConstruct::Quest | QuestConstruct::Objective) =>
        {
            Vec::new()
        }
        Cursor::ConstructAttrArea { construct } => construct_attr_key_items(construct),
        Cursor::Speaker => speaker_items(
            providers,
            &lute_check::declared_cast(snapshot, imports, &meta.cast),
        ),
    }
}

/// A tag whose open-tag keys are a small fixed set the checker closes over
/// (`lute_check::logic_attrs`: `BRANCH_ATTRS`, `BRANCH_CHOICE_ATTRS`,
/// `HUB_CHOICE_ATTRS`, `MATCH_ATTRS`) but which the cursor resolver gives no
/// attr-area cursor of its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OpenTag {
    Branch,
    BranchChoice,
    HubChoice,
    Match,
}

impl OpenTag {
    fn keys(self) -> &'static [(&'static str, &'static str)] {
        const CHOICE: &[(&str, &str)] = &[
            ("id", "string"),
            ("text", "string — the displayed choice text"),
            ("when", "cel<bool>"),
            ("into", "state path — records the pick"),
            ("value", "the value `into` records"),
        ];
        const HUB_CHOICE: &[(&str, &str)] = &[
            ("id", "string"),
            ("text", "string — the displayed choice text"),
            ("when", "cel<bool>"),
            ("into", "state path — records the pick"),
            ("value", "the value `into` records"),
            ("once", "flag — offered until picked once"),
            ("exit", "flag — leaves the hub"),
        ];
        match self {
            OpenTag::Branch => &[
                ("id", "string"),
                ("prompt", "string"),
                ("timeout", "positive integer seconds"),
            ],
            OpenTag::BranchChoice => CHOICE,
            OpenTag::HubChoice => HUB_CHOICE,
            OpenTag::Match => &[("subject", "cel — the value the arms match")],
        }
    }
}

/// The open tag (`<branch …>`, `<choice …>`, `<match …>`) whose interior
/// holds `off`, with that interior's source text (between the keyword and the
/// closing `>`). `None` when `off` is not inside such an open tag.
fn open_tag_at<'s>(doc: &Document, src: &'s str, off: usize) -> Option<(OpenTag, &'s str)> {
    fn interior<'s>(src: &'s str, start: usize, keyword: &str, off: usize) -> Option<&'s str> {
        if !src.get(start..)?.starts_with(keyword) {
            return None;
        }
        let inner = start + keyword.len();
        let mut quoted = false;
        let mut close = src.len();
        for (i, b) in src.bytes().enumerate().skip(inner) {
            match b {
                b'"' => quoted = !quoted,
                b'>' if !quoted => {
                    close = i;
                    break;
                }
                _ => {}
            }
        }
        (inner < off && off <= close).then(|| &src[inner..close])
    }
    fn scan<'s>(nodes: &[Node], src: &'s str, off: usize) -> Option<(OpenTag, &'s str)> {
        let node = nodes.iter().find(|n| span_contains(node_span(n), off))?;
        match node {
            Node::Branch(b) => {
                if let Some(inner) = interior(src, b.span.byte_start, "<branch", off) {
                    return Some((OpenTag::Branch, inner));
                }
                let c = b.choices.iter().find(|c| span_contains(c.span, off))?;
                match interior(src, c.span.byte_start, "<choice", off) {
                    Some(inner) => Some((OpenTag::BranchChoice, inner)),
                    None => scan(&c.body, src, off),
                }
            }
            Node::Hub(h) => {
                if let Some(c) = h.choices.iter().find(|c| span_contains(c.span, off)) {
                    return match interior(src, c.span.byte_start, "<choice", off) {
                        Some(inner) => Some((OpenTag::HubChoice, inner)),
                        None => scan(&c.body, src, off),
                    };
                }
                scan(&h.on_return.as_ref()?.body, src, off)
            }
            Node::Match(m) => {
                if let Some(inner) = interior(src, m.span.byte_start, "<match", off) {
                    return Some((OpenTag::Match, inner));
                }
                m.arms.iter().find_map(|arm| {
                    let (Arm::When { body, .. } | Arm::Otherwise { body, .. }) = arm;
                    scan(body, src, off)
                })
            }
            Node::On(o) => scan(&o.body, src, off),
            Node::Objective(ob) => scan(&ob.body, src, off),
            _ => None,
        }
    }
    doc.sections
        .iter()
        .map(|s| &s.body)
        .chain(doc.quests.iter().map(|q| &q.body))
        .chain(doc.entries.iter().map(|e| &e.body))
        .chain(doc.beats.iter().map(|b| &b.body))
        .find_map(|body| scan(body, src, off))
}

/// Span of any [`Node`], for the open-tag descent.
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
        Node::Assert(a) => a.span,
        Node::Retract(r) => r.span,
    }
}

/// The keys of `tag` not yet written in its open-tag `interior`, kind `FIELD`.
fn open_tag_items(tag: OpenTag, interior: &str) -> Vec<CompletionItem> {
    // Every identifier outside a quoted value is (at worst) an attr key.
    let mut present = BTreeSet::new();
    let mut quoted = false;
    let mut word = String::new();
    for c in interior.chars().chain(std::iter::once(' ')) {
        if c == '"' {
            quoted = !quoted;
        }
        if !quoted && (c.is_ascii_alphanumeric() || c == '_' || c == '-') {
            word.push(c);
        } else if !word.is_empty() {
            present.insert(std::mem::take(&mut word));
        }
    }
    tag.keys()
        .iter()
        .filter(|(key, _)| !present.contains(*key))
        .map(|(key, detail)| CompletionItem {
            label: key.to_string(),
            kind: Some(CompletionItemKind::FIELD),
            detail: Some(detail.to_string()),
            ..Default::default()
        })
        .collect()
}

/// Inline text-modifier names (dsl 0.37.0 §3.6) when `off` follows `:` (plus
/// a partial lowerCamel name) in a content line's text: the core `pause`
/// leaf and `speed` span, then the merged `textStyle` domain's members. A
/// colon glued to a preceding word (`Note:`) or escaped (`\:`) is prose.
fn modifier_items(
    doc: &Document,
    snapshot: &CapabilitySnapshot,
    imports: &SchemaImports,
    meta: &lute_check::TypedMeta,
    off: usize,
) -> Vec<CompletionItem> {
    let Some(line) = super::line_at(doc, off) else {
        return Vec::new();
    };
    let Some(local) = off.checked_sub(line.text_span.byte_start) else {
        return Vec::new();
    };
    let Some(before) = line.text.get(..local) else {
        return Vec::new();
    };
    let name = before.trim_end_matches(|c: char| c.is_ascii_alphanumeric());
    let Some(lead) = name.strip_suffix(':') else {
        return Vec::new();
    };
    if lead
        .chars()
        .next_back()
        .is_some_and(|c| c.is_ascii_alphanumeric() || c == '\\')
        || before[name.len()..].starts_with(|c: char| !c.is_ascii_lowercase())
    {
        return Vec::new();
    }
    let core = [
        ("pause", "leaf — `:pause{s=seconds}`"),
        ("speed", "span — `:speed[text]{rate=rate}`"),
    ];
    core.into_iter()
        .map(|(label, detail)| CompletionItem {
            label: label.to_string(),
            kind: Some(CompletionItemKind::FUNCTION),
            detail: Some(detail.to_string()),
            ..Default::default()
        })
        .chain(
            domain_members(snapshot, imports, meta, "textStyle")
                .into_iter()
                .map(|label| CompletionItem {
                    label,
                    kind: Some(CompletionItemKind::ENUM_MEMBER),
                    detail: Some("textStyle — `:name[text]`".to_string()),
                    ..Default::default()
                }),
        )
        .collect()
}

/// The members of closed domain `name` in the merged vocabulary
/// (`snapshot.domains` ∪ the project's domains), via the same
/// [`lute_check::schema_import::merge_domains`] seam `check()` uses. Empty for
/// an undeclared or open domain.
fn domain_members(
    snapshot: &CapabilitySnapshot,
    imports: &SchemaImports,
    meta: &lute_check::TypedMeta,
    name: &str,
) -> Vec<String> {
    let zero = lute_resolve::cursor::byte_span(0, 0);
    let (merged, _) = lute_check::schema_import::merge_domains(snapshot, imports, meta, zero);
    merged
        .get(name)
        .filter(|d| !d.open)
        .map(|d| d.members.clone())
        .unwrap_or_default()
}

/// Every `::label{name=…}` in the document, for a `::jump{to=…}` value (dsl
/// 0.37.0 §3.5: one document-wide label namespace), kind `REFERENCE`.
fn label_items(doc: &Document) -> Vec<CompletionItem> {
    let names: BTreeSet<&str> = super::label_decls(doc)
        .into_iter()
        .map(|(n, _)| n)
        .filter(|n| !n.is_empty())
        .collect();
    names
        .into_iter()
        .map(|name| CompletionItem {
            label: name.to_string(),
            kind: Some(CompletionItemKind::REFERENCE),
            detail: Some("::label".to_string()),
            ..Default::default()
        })
        .collect()
}

/// Jump targets no `::label` declares yet, for a `::label{name=…}` value.
fn unlabeled_target_items(doc: &Document) -> Vec<CompletionItem> {
    let declared: BTreeSet<&str> = super::label_decls(doc).into_iter().map(|(n, _)| n).collect();
    let targets: BTreeSet<&str> = super::jump_targets(doc)
        .into_iter()
        .map(|(n, _)| n)
        .filter(|n| !n.is_empty() && !declared.contains(n))
        .collect();
    targets
        .into_iter()
        .map(|name| CompletionItem {
            label: name.to_string(),
            kind: Some(CompletionItemKind::REFERENCE),
            detail: Some("::jump target without a label".to_string()),
            ..Default::default()
        })
        .collect()
}

/// The fixed (non-snapshot) attr-key set for a `<quest>`/`<on>`/`<objective>`
/// open tag (dsl 0.2.0 §6.3/§4/§6.4), kind `FIELD`. Unlike `::directive`
/// attrs, these three constructs have a small closed set that is NOT
/// capability-schema-driven, so the table is hardcoded here (mirroring how
/// the tree-sitter grammar's `cel_key` enumerates the CEL-valued attr names) —
/// always the full set, since `id`/`title`/`start`/`fail`/`event`/`when`/
/// `done`/`optional` are parsed into dedicated AST fields (not a `Vec<Attr>`),
/// so "already present" cannot be read back generically the way a
/// directive's `attrs` list allows. The key sets are exactly the checker's
/// closed tables (`lute_check::logic_attrs`), which reject every other key.
fn construct_attr_key_items(construct: QuestConstruct) -> Vec<CompletionItem> {
    construct_attr_keys(construct)
        .iter()
        .map(|(name, ty)| CompletionItem {
            label: name.to_string(),
            kind: Some(CompletionItemKind::FIELD),
            detail: Some(ty.to_string()),
            ..Default::default()
        })
        .collect()
}

fn construct_attr_keys(construct: QuestConstruct) -> &'static [(&'static str, &'static str)] {
    match construct {
        QuestConstruct::Quest => &[
            ("id", "string"),
            ("title", "string"),
            ("start", "cel<bool>"),
            ("fail", "cel<bool>"),
            ("follows", "prereq (graph metadata)"),
            ("tier", "\"user\" | \"run\" | \"season:<name>\""),
            ("activate", "\"accept\""),
            ("complete", "\"all\" | \"any\""),
            ("accept", "\"external\""),
            ("rearm", "cel<bool>"),
        ],
        QuestConstruct::On => &[
            ("event", "string"),
            ("when", "cel<bool>"),
            ("target", "string"),
        ],
        QuestConstruct::Objective => &[
            ("id", "string"),
            ("done", "cel<bool>"),
            ("quest", "string"),
            ("visibleWhen", "cel<bool> (visibility only)"),
            ("title", "string"),
            ("optional", "bool"),
            ("on", "string"),
            ("by", "cel<bool>"),
            ("target", "string"),
            ("until", "cel<bool>"),
        ],
        QuestConstruct::Entry => &[
            ("id", "string"),
            ("target", "string"),
            ("category", "string"),
            ("title", "string"),
            ("series", "string"),
            ("order", "integer"),
            ("when", "cel<bool>"),
            ("on", "string"),
            ("priority", "integer"),
            (
                "once",
                "\"run\" | \"user\" | \"day\" | \"week\" | \"slot\" | \"season:<name>\"",
            ),
            ("share", "string"),
            ("spentBy", "cel<bool>"),
            ("for", "\"kind:<kind>\""),
            ("advances", "\"slot\" | \"day\" | <whole number ≥ 1>"),
        ],
        QuestConstruct::Beat => &[
            ("id", "string"),
            ("on", "string"),
            ("target", "string"),
            ("title", "string"),
            ("when", "cel<bool>"),
            ("priority", "integer"),
            (
                "once",
                "\"run\" | \"user\" | \"false\" | \"day\" | \"week\" | \"slot\" | \"season:<name>\"",
            ),
            ("also", "bool"),
            ("share", "string"),
            ("spentBy", "cel<bool>"),
            ("after", "prereq"),
            ("use", "component (beat template)"),
            ("for", "\"kind:<kind>\""),
            ("advances", "\"slot\" | \"day\" | <whole number ≥ 1>"),
        ],
        QuestConstruct::Hub => &[("id", "string"), ("prompt", "string")],
    }
}

/// Content-line attribute keys (dsl 0.37.0 §3.4; `when`, dsl 0.4.0 §7.2): a
/// `@speaker{…}:` line's fixed vocabulary is, like
/// [`construct_attr_key_items`]'s constructs, NOT capability-schema-driven —
/// it is the checker's own [`KNOWN_ATTRS`] table plus `when` (extracted into
/// `Line.when` at parse time, so never in `l.attrs`), kind `FIELD`, minus keys
/// already written. `mono`/`os`/`vo` are the mutually exclusive bare delivery
/// flags selecting the line role; none is offered once one is written, nor on
/// a `narrator` line (`E-DELIVERY-NARRATOR`). A line's `voiceKey` and
/// `lineId` are derived (from `code` when written), never authored.
fn content_line_attr_key_items(line: &Line) -> Vec<CompletionItem> {
    const DELIVERY: [&str; 3] = ["mono", "os", "vo"];
    let present: BTreeSet<&str> = line
        .attrs
        .iter()
        .map(|a| a.key.as_str())
        .chain(line.when.as_ref().map(|_| "when"))
        .collect();
    let no_delivery =
        line.speaker == "narrator" || DELIVERY.iter().any(|flag| present.contains(flag));
    KNOWN_ATTRS
        .iter()
        .copied()
        .chain(std::iter::once("when"))
        .filter(|key| !present.contains(key))
        .filter(|key| !(no_delivery && DELIVERY.contains(key)))
        .map(|key| CompletionItem {
            label: key.to_string(),
            kind: Some(CompletionItemKind::FIELD),
            detail: content_line_attr_detail(key).map(str::to_string),
            ..Default::default()
        })
        .collect()
}

/// The one-line description of a content-line attribute key (dsl 0.37.0
/// §3.4), shared by completion details and hover.
pub(crate) fn content_line_attr_detail(key: &str) -> Option<&'static str> {
    Some(match key {
        "code" => "string — stable line code; the line's lineId/voiceKey derive from it",
        "emotion" => "domain `emotion`",
        "variant" => "number",
        "action" => "domain `action`",
        "dialogMotion" => "string",
        "mono" => "flag — role `mono`: the effective POV speaker or a `monoSpeakers` member",
        "os" => "flag — role `os`: spoken off-screen",
        "vo" => "flag — role `vo`: voice-over",
        "as" => "string",
        "when" => "condition",
        _ => return None,
    })
}

/// Content-line attribute VALUES: the `emotion`/`action` domain slots
/// ([`CONTENT_LINE_DOMAIN_SLOTS`]) offer their merged domain's members; every
/// other key is string/number/flag typed with no enumerable value domain.
fn content_line_attr_value_items(
    snapshot: &CapabilitySnapshot,
    imports: &SchemaImports,
    meta: &lute_check::TypedMeta,
    key: &str,
) -> Vec<CompletionItem> {
    if !CONTENT_LINE_DOMAIN_SLOTS.contains(&key) {
        return Vec::new();
    }
    domain_members(snapshot, imports, meta, key)
        .into_iter()
        .map(|label| CompletionItem {
            label,
            kind: Some(CompletionItemKind::ENUM_MEMBER),
            ..Default::default()
        })
        .collect()
}

/// Character/cast ids from the pinned `character` provider snapshot (same
/// well-known provider name the `CH` [`AssetKindDecl`]'s `characterId`
/// segment resolves against, dsl plugin §8/§10) — mirrors
/// [`asset_segment_items`]'s `Type::ProviderRef` lookup: dedup + sort across
/// every pinned snapshot. Empty (never fabricated) when no snapshot declares
/// `character` — 0.2.1 has no dedicated cast-catalog provider kind of its
/// own, so this reuses the one the asset-id grammar already established.
fn character_ids(providers: &ProviderSet) -> Vec<String> {
    providers
        .snapshots()
        .iter()
        .filter_map(|s| s.entries.get("character"))
        .flatten()
        .map(String::as_str)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(String::from)
        .collect()
}

/// `@speaker{…}:` name completion (dsl §7.1): the declared cast (dsl 0.23.0
/// §7 — plugin `cast` exports ∪ imported schemas' `cast:`, each member's
/// display name as the detail) when one is declared, since only those ids
/// check; otherwise the pinned `character` catalog ids (kind `VALUE`, as
/// [`asset_segment_items`] offers provider ids). The always-valid `narrator`
/// keyword (kind `KEYWORD`) follows either way.
fn speaker_items(
    providers: &ProviderSet,
    cast: &std::collections::BTreeMap<String, lute_manifest::schema::CastMember>,
) -> Vec<CompletionItem> {
    let ids: Vec<(String, Option<String>)> = if cast.is_empty() {
        character_ids(providers)
            .into_iter()
            .map(|id| (id, None))
            .collect()
    } else {
        cast.values()
            .map(|c| (c.id.clone(), c.name.clone()))
            .collect()
    };
    ids.into_iter()
        .map(|(id, name)| CompletionItem {
            label: id,
            kind: Some(CompletionItemKind::VALUE),
            detail: name,
            ..Default::default()
        })
        .chain(std::iter::once(CompletionItem {
            label: "narrator".to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            ..Default::default()
        }))
        .collect()
}

/// Every known lifecycle/world event name for an `<on event="…">` value
/// position (dsl 0.2.0 §4.5): the built-ins union the capability-declared
/// events, kind `EVENT`.
fn event_name_items(snapshot: &CapabilitySnapshot) -> Vec<CompletionItem> {
    let mut names: BTreeSet<String> = if snapshot.permissions.allows_quests() {
        lute_manifest::snapshot::BUILTIN_LIFECYCLE_EVENTS
            .iter()
            .map(|s| s.to_string())
            .collect()
    } else {
        BTreeSet::new()
    };
    names.extend(snapshot.events.keys().cloned());
    names
        .into_iter()
        .map(|n| CompletionItem {
            label: n,
            kind: Some(CompletionItemKind::EVENT),
            ..Default::default()
        })
        .collect()
}

/// `Some` (possibly empty) when `off` lands on the VALUE half of a `kind:`
/// line in the peeled frontmatter YAML (dsl 0.2.0 §3.1's `scene`/`quest`
/// discriminator, plus dsl 0.19.0's `lore`); `None` when `off` is not there,
/// so the caller falls
/// through to the normal body-cursor resolution. Mirrors
/// `super::find_yaml_key_span`'s line-scan + `FRONTMATTER_BASE` convention.
fn kind_value_items(
    doc: &Document,
    snapshot: &CapabilitySnapshot,
    off: usize,
) -> Option<Vec<CompletionItem>> {
    const FRONTMATTER_BASE: usize = 4; // len("---\n")
    let raw = &doc.meta.raw_yaml;
    if raw.is_empty() || off < FRONTMATTER_BASE {
        return None;
    }
    let local = off - FRONTMATTER_BASE;
    if local > raw.len() {
        return None;
    }
    let mut line_start = 0usize;
    for line in raw.split_inclusive('\n') {
        let line_end = line_start + line.len();
        if local < line_start || local > line_end {
            line_start = line_end;
            continue;
        }
        // `off` is on THIS line — either it's the `kind:` line (checked
        // below) or it isn't a completion position at all.
        let content = line.trim_start();
        let rest = content.strip_prefix("kind")?;
        let after_colon = rest.trim_start().strip_prefix(':')?;
        // The first VALUE byte sits right after the colon; everything from
        // there to end-of-line (incl. leading whitespace) is "value area".
        let value_start = line_end - after_colon.len();
        if local < value_start {
            return None; // cursor is on the `kind` KEY, not its value.
        }
        return Some(
            ["scene", "quest", "lore"]
                .into_iter()
                .filter(|kind| *kind != "quest" || snapshot.permissions.allows_quests())
                .map(|kind| CompletionItem {
                    label: kind.to_string(),
                    kind: Some(CompletionItemKind::ENUM_MEMBER),
                    ..Default::default()
                })
                .collect(),
        );
    }
    None
}

/// Every directive name (`::bg`, `::camera`, …), kind `FUNCTION`, plus the
/// language's core `::accept` (dsl 0.21.0 §7a.3), which no snapshot declares.
fn directive_items(snapshot: &CapabilitySnapshot) -> Vec<CompletionItem> {
    let mut items: Vec<CompletionItem> = snapshot
        .directives
        .values()
        .filter(|d| snapshot.permissions.allows_directive(&d.name))
        .filter(|d| {
            d.bridge.as_ref().is_none_or(|bridge| {
                snapshot
                    .permissions
                    .allows_bridge(&bridge.service, &bridge.operation)
            })
        })
        .map(|d| CompletionItem {
            label: d.name.clone(),
            kind: Some(CompletionItemKind::FUNCTION),
            detail: d.layer.as_ref().map(|l| format!("layer {l}")),
            ..Default::default()
        })
        .collect();
    if snapshot.permissions.allows_directive(ACCEPT_DIRECTIVE) {
        items.push(CompletionItem {
            label: ACCEPT_DIRECTIVE.to_string(),
            kind: Some(CompletionItemKind::FUNCTION),
            detail: Some("core — accept a quest".to_string()),
            ..Default::default()
        });
    }
    items
}

/// A directive's attribute keys, kind `FIELD`, minus keys already written on the
/// directive/line at the cursor (so the list narrows as attrs are filled in).
/// Every directive but `::clear` also takes the undeclared universal timing
/// keys (`duration`/`delay`/`wait`, dsl 0.37.0 §3.3).
fn attr_key_items(
    snapshot: &CapabilitySnapshot,
    directive: &str,
    doc: &Document,
    off: usize,
) -> Vec<CompletionItem> {
    if !snapshot.permissions.allows_directive(directive) {
        return Vec::new();
    }
    // dsl 0.21.0 §7a.3, 0.24.0 §2: the core `::accept` takes `quest` and `at`.
    if directive == ACCEPT_DIRECTIVE {
        let present = present_attr_keys(doc, off);
        return [("quest", "quest id"), ("at", "\"nextRun\"")]
            .into_iter()
            .filter(|(key, _)| !present.iter().any(|k| k == key))
            .map(|(key, detail)| CompletionItem {
                label: key.to_string(),
                kind: Some(CompletionItemKind::FIELD),
                detail: Some(detail.to_string()),
                ..Default::default()
            })
            .collect();
    }
    let Some(decl) = snapshot.directive(directive) else {
        return Vec::new();
    };
    if decl.bridge.as_ref().is_some_and(|bridge| {
        !snapshot
            .permissions
            .allows_bridge(&bridge.service, &bridge.operation)
    }) {
        return Vec::new();
    }
    let present = present_attr_keys(doc, off);
    let timing = UNIVERSAL_TIMING_ATTRS
        .iter()
        .filter(|_| directive != CLEAR_DIRECTIVE)
        .filter(|(name, _)| !decl.attrs.iter().any(|a| a.name == *name))
        .map(|(name, ty)| (name.to_string(), ty));
    decl.attrs
        .iter()
        .map(|a| (a.name.clone(), &a.ty))
        .chain(timing)
        .filter(|(name, _)| !present.contains(name))
        .map(|(name, ty)| CompletionItem {
            label: name,
            kind: Some(CompletionItemKind::FIELD),
            detail: Some(type_label(ty)),
            ..Default::default()
        })
        .collect()
}

/// The enum members of an enum- or domain-typed attribute value (data-catalog
/// foundation A5, resolved against the merged snapshot ∪ project-schema
/// vocabulary), kind `ENUM_MEMBER`. Empty for a non-enum, non-domain, or
/// open-domain attr.
fn enum_value_items(
    snapshot: &CapabilitySnapshot,
    imports: &SchemaImports,
    meta: &lute_check::TypedMeta,
    directive: &str,
    key: &str,
) -> Vec<CompletionItem> {
    attr_enum_values(snapshot, imports, meta, directive, key)
        .into_iter()
        .flatten()
        .map(|v| CompletionItem {
            label: v,
            kind: Some(CompletionItemKind::ENUM_MEMBER),
            ..Default::default()
        })
        .collect()
}

/// Per-segment completion for an `assetId` value typed `assetKind(kind)`: the
/// members of the segment under the cursor. A const segment offers its literal;
/// an enum segment its members; a providerRef segment the pinned snapshot's ids
/// for that provider (empty when no snapshot declares it); number/string offer
/// nothing.
fn asset_segment_items(
    kind: &AssetKindDecl,
    doc: &Document,
    providers: &ProviderSet,
    off: usize,
) -> Vec<CompletionItem> {
    let Some(attr) = attr_at(doc, off) else {
        return Vec::new();
    };
    let AttrValue::Str(value) = &attr.value else {
        return Vec::new();
    };
    let idx = asset_segment_index(kind, value, attr.value_span.byte_start, off);
    let Some(seg) = kind.segments.get(idx) else {
        return Vec::new();
    };
    if let Some(c) = &seg.r#const {
        return vec![CompletionItem {
            label: c.clone(),
            kind: Some(CompletionItemKind::CONSTANT),
            ..Default::default()
        }];
    }
    match &seg.ty {
        Some(Type::Enum(members)) => members
            .iter()
            .map(|m| CompletionItem {
                label: m.clone(),
                kind: Some(CompletionItemKind::ENUM_MEMBER),
                ..Default::default()
            })
            .collect(),
        // providerRef: offer the ids the pinned snapshot resolves for this
        // provider (§6.9), deduped and sorted across every snapshot in the set.
        // Empty when no snapshot declares the provider — honest, never fabricated.
        Some(Type::ProviderRef(provider)) => providers
            .snapshots()
            .iter()
            .filter_map(|s| s.entries.get(provider))
            .flatten()
            .map(String::as_str)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .map(|id| CompletionItem {
                label: id.to_string(),
                kind: Some(CompletionItemKind::VALUE),
                ..Default::default()
            })
            .collect(),
        // number / string / untyped segments have no enumerable domain.
        _ => Vec::new(),
    }
}

/// Author `defs:` + snapshot def names for an `@ref` position, kind `VARIABLE`.
fn def_items(meta: &lute_check::TypedMeta, snapshot: &CapabilitySnapshot) -> Vec<CompletionItem> {
    let mut names: std::collections::BTreeSet<String> = meta.defs.keys().cloned().collect();
    names.extend(snapshot.defs.keys().cloned());
    names
        .into_iter()
        .map(|n| CompletionItem {
            label: n,
            kind: Some(CompletionItemKind::VARIABLE),
            ..Default::default()
        })
        .collect()
}

fn host_items(raw: &str, local: usize, doc: &Document) -> Option<Vec<CompletionItem>> {
    const HOSTS: &[(&str, &str)] = &[
        ("holds", "holds(string, list(dyn)) -> bool"),
        ("count", "count(string, list(dyn)) -> int"),
        ("countDistinct", "countDistinct(string, list(dyn), int) -> int"),
        ("validAt", "validAt(string, list(dyn), int) -> bool"),
        ("now", "now() -> int"),
        ("visited", "visited(string) -> bool"),
        ("has", "has(select) -> bool"),
        ("int", "int(int|double) -> int"),
        ("double", "double(int|double) -> double"),
    ];
    let before = &raw[..local.min(raw.len())];
    let open = before.rfind('(')?;
    let name = before[..open].split(|c: char| !c.is_ascii_alphanumeric()).last()?;
    if !HOSTS.iter().any(|(n, _)| *n == name) {
        return None;
    }
    if matches!(name, "holds" | "count" | "countDistinct" | "validAt")
        && before[open + 1..].trim_start().starts_with(['\'', '"'])
    {
        let mut names = Vec::new();
        if let Ok(serde_yaml::Value::Mapping(map)) =
            serde_yaml::from_str::<serde_yaml::Value>(&doc.meta.raw_yaml)
        {
            if let Some(serde_yaml::Value::Sequence(relations)) =
                map.get(serde_yaml::Value::String("relations".into()))
            {
                for relation in relations {
                    if let Some(n) = relation.get("name").and_then(|v| v.as_str()) {
                        names.push(n.to_string());
                    }
                }
            }
        }
        return Some(names.into_iter().map(|label| CompletionItem {
            label,
            kind: Some(CompletionItemKind::FUNCTION),
            detail: Some("relation name".into()),
            ..Default::default()
        }).collect());
    }
    Some(HOSTS.iter().map(|(label, detail)| CompletionItem {
        label: (*label).into(),
        kind: Some(CompletionItemKind::FUNCTION),
        detail: Some((*detail).into()),
        ..Default::default()
    }).collect())
}

/// Declared state paths (`scene.*`, `run.*`, …), kind `PROPERTY`.
fn state_path_items(meta: &lute_check::TypedMeta) -> Vec<CompletionItem> {
    meta.state
        .decls
        .iter()
        .map(|(path, decl)| CompletionItem {
            label: path.clone(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: Some(type_label(&decl.ty)),
            ..Default::default()
        })
        .collect()
}

/// `scene.choices.<id>` ids from every `<branch>` (for a `<match subject=…>` subject).
fn choice_path_items(doc: &Document) -> Vec<CompletionItem> {
    let mut ids = Vec::new();
    for section in &doc.sections {
        collect_branch_ids(&section.body, &mut ids);
    }
    for quest in &doc.quests {
        collect_branch_ids(&quest.body, &mut ids);
    }
    ids.into_iter()
        .map(|id| CompletionItem {
            label: format!("scene.choices.{id}"),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: Some(format!("choice of <branch id=\"{id}\">")),
            ..Default::default()
        })
        .collect()
}

/// `<when is="…">` literal-pattern candidates for the enclosing `<match>`
/// subject's finite domain (dsl §7.3.1): enum members / bool ∪ `unset`, or the
/// branch/hub choice ids ∪ `unset` — sourced from the shared [`super::subject_domain`]
/// so hover + completion never diverge. Empty when the subject has no finite
/// domain. The whole `is=` value is ONE cursor span, so every domain member is
/// offered regardless of the cursor's position among prior `|`-alternatives.
fn is_pattern_items(
    doc: &Document,
    meta: &lute_check::TypedMeta,
    subject_path: &str,
) -> Vec<CompletionItem> {
    let Some(domain) = subject_domain(doc, meta, subject_path) else {
        return Vec::new();
    };
    domain
        .into_iter()
        .map(|v| CompletionItem {
            label: v,
            kind: Some(CompletionItemKind::ENUM_MEMBER),
            detail: Some("<when is> pattern".to_string()),
            ..Default::default()
        })
        .collect()
}

fn collect_branch_ids(nodes: &[Node], out: &mut Vec<String>) {
    for node in nodes {
        match node {
            Node::Branch(b) => {
                if !b.id.is_empty() {
                    out.push(b.id.clone());
                }
                for c in &b.choices {
                    collect_branch_ids(&c.body, out);
                }
            }
            Node::Hub(h) => {
                let id = h
                    .attrs
                    .iter()
                    .find(|a| a.key == "id")
                    .and_then(|a| match &a.value {
                        AttrValue::Str(s) => Some(s.as_str()),
                        _ => None,
                    });
                if let Some(id) = id {
                    if !id.is_empty() {
                        out.push(id.to_string());
                    }
                }
                for b in h.bodies() {
                    collect_branch_ids(b, out);
                }
            }
            Node::Match(m) => {
                for arm in &m.arms {
                    let body = match arm {
                        Arm::When { body, .. } | Arm::Otherwise { body, .. } => body,
                    };
                    collect_branch_ids(body, out);
                }
            }
            Node::On(o) => collect_branch_ids(&o.body, out),
            Node::Objective(ob) => collect_branch_ids(&ob.body, out),
            _ => {}
        }
    }
}

/// Attribute keys already present on the directive/line whose span contains `off`
/// — searched across every node so key completion can dedupe.
fn present_attr_keys(doc: &Document, off: usize) -> Vec<String> {
    fn scan(nodes: &[Node], off: usize, out: &mut Vec<String>) {
        for node in nodes {
            match node {
                Node::Directive(d) if span_contains(d.span, off) => {
                    out.extend(d.attrs.iter().map(|a| a.key.clone()));
                }
                Node::Line(l) if span_contains(l.span, off) => {
                    out.extend(l.attrs.iter().map(|a| a.key.clone()));
                }
                Node::Branch(b) if span_contains(b.span, off) => {
                    for c in &b.choices {
                        scan(&c.body, off, out);
                    }
                }
                Node::Hub(h) if span_contains(h.span, off) => {
                    for b in h.bodies() {
                        scan(b, off, out);
                    }
                }
                Node::Match(m) if span_contains(m.span, off) => {
                    for arm in &m.arms {
                        let body = match arm {
                            Arm::When { body, .. } | Arm::Otherwise { body, .. } => body,
                        };
                        scan(body, off, out);
                    }
                }
                Node::Timeline(t) if span_contains(t.span, off) => {
                    for track in &t.tracks {
                        for clip in &track.clips {
                            if let lute_syntax::ast::ClipNode::Directive(d) = &clip.node {
                            if span_contains(d.span, off) {
                                    out.extend(d.attrs.iter().map(|a| a.key.clone()));
                                }
                            }
                        }
                    }
                }
                Node::On(o) if span_contains(o.span, off) => scan(&o.body, off, out),
                Node::Objective(ob) if span_contains(ob.span, off) => {
                    scan(&ob.body, off, out)
                }
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    for section in &doc.sections {
        scan(&section.body, off, &mut out);
    }
    for quest in &doc.quests {
        scan(&quest.body, off, &mut out);
    }
    for entry in &doc.entries {
        scan(&entry.body, off, &mut out);
    }
    for beat in &doc.beats {
        scan(&beat.body, off, &mut out);
    }
    out
}

/// True if the byte just before `local` (slot-relative) sits in an `@`-prefixed
/// token — i.e. the cursor is completing a `@ref` name.
fn at_ref(raw: &str, local: usize) -> bool {
    let b = raw.as_bytes();
    let mut i = local.min(b.len());
    while i > 0 {
        let c = b[i - 1];
        if c == b'@' {
            return true;
        }
        if c.is_ascii_alphanumeric() || c == b'_' || c == b'-' {
            i -= 1;
        } else {
            return false;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use lute_manifest::core::load_core_snapshot;
    use lute_syntax::parse;

    fn parsed(text: &str) -> Document {
        parse(text).0
    }

    fn labels(items: &[CompletionItem]) -> Vec<&str> {
        items.iter().map(|i| i.label.as_str()).collect()
    }

    #[test]
    fn completion_after_double_colon_lists_directives() {
        let text = "## Shot 1.\n::";
        let doc = parsed(text);
        let off = text.find("::").unwrap() + 2; // just past `::`
        let items = complete_at(&doc, text, &load_core_snapshot(), &ProviderSet::default(), &SchemaImports::default(), off);
        assert!(items.iter().any(|i| i.label == "camera"));
        assert!(items.iter().any(|i| i.label == "bg"));
    }

    fn restrict_snapshot(
        mut snapshot: CapabilitySnapshot,
        layer: lute_manifest::permissions::PermissionSet,
    ) -> CapabilitySnapshot {
        snapshot.restrict_permissions(&lute_manifest::permissions::Permissions {
            layers: vec![layer],
        });
        snapshot
    }

    #[test]
    fn directive_completion_uses_effective_permission_snapshot() {
        let snapshot = restrict_snapshot(
            load_core_snapshot(),
            lute_manifest::permissions::PermissionSet {
                directives: Some(std::collections::BTreeSet::from(["camera".to_string()])),
                ..Default::default()
            },
        );
        let text = "## Shot 1.\n::";
        let items = complete_at(&parsed(text), text, &snapshot, &ProviderSet::default(), &SchemaImports::default(), text.len());
        assert_eq!(labels(&items), vec!["camera"]);
    }

    #[test]
    fn directive_completion_also_filters_a_denied_bridge() {
        let mut snapshot = load_core_snapshot();
        snapshot.directives.get_mut("camera").unwrap().bridge =
            Some(lute_manifest::schema::BridgeRef {
                service: "display".to_string(),
                operation: "focus".to_string(),
            });
        let snapshot = restrict_snapshot(
            snapshot,
            lute_manifest::permissions::PermissionSet {
                bridges: Some(std::collections::BTreeSet::new()),
                ..Default::default()
            },
        );
        let text = "## Shot 1.\n::";
        let items = complete_at(&parsed(text), text, &snapshot, &ProviderSet::default(), &SchemaImports::default(), text.len());
        assert!(!labels(&items).contains(&"camera"));
    }

    #[test]
    fn quest_completion_is_absent_when_quest_authoring_is_denied() {
        let snapshot = restrict_snapshot(
            load_core_snapshot(),
            lute_manifest::permissions::PermissionSet {
                quests: Some(false),
                ..Default::default()
            },
        );
        let text = "---\nkind: \n---\n";
        let off = text.find("kind: ").unwrap() + "kind: ".len();
        let items = complete_at(&parsed(text), text, &snapshot, &ProviderSet::default(), &SchemaImports::default(), off);
        // Only `quest` is permission-gated; `lore` is not quest authoring.
        assert_eq!(labels(&items), vec!["scene", "lore"]);
    }

    #[test]
    fn completion_of_attr_keys_inside_directive() {
        let text = "## Shot 1.\n::camera{}\n";
        let doc = parsed(text);
        let off = text.find("{}").unwrap() + 1; // between the braces
        let items = complete_at(&doc, text, &load_core_snapshot(), &ProviderSet::default(), &SchemaImports::default(), off);
        let ls = labels(&items);
        assert!(ls.contains(&"focus"), "has focus: {ls:?}");
        assert!(ls.contains(&"framing"), "has framing: {ls:?}");
    }

    #[test]
    fn attr_key_completion_dedupes_present_keys() {
        let text = "## Shot 1.\n::camera{focus=\"b\" }\n";
        let doc = parsed(text);
        // Cursor in the whitespace after the first attr (still the attr area).
        let off = text.find("\" }").unwrap() + 2;
        let items = complete_at(&doc, text, &load_core_snapshot(), &ProviderSet::default(), &SchemaImports::default(), off);
        let ls = labels(&items);
        assert!(
            !ls.contains(&"focus"),
            "focus already present, should be gone: {ls:?}"
        );
        assert!(ls.contains(&"framing"), "framing still offered: {ls:?}");
    }

    #[test]
    fn completion_of_enum_values_at_enum_attr() {
        let text = "## Shot 1.\n::actor{character=\"b\" anchor=\"\"}\n";
        let doc = parsed(text);
        // Cursor inside the empty `anchor=""` value.
        let off = text.find("anchor=\"").unwrap() + "anchor=\"".len();
        let items = complete_at(&doc, text, &lute_test_vocab::vocab_snapshot(), &ProviderSet::default(), &SchemaImports::default(), off);
        let ls = labels(&items);
        assert!(
            ls.contains(&"left") && ls.contains(&"center") && ls.contains(&"right"),
            "anchor enum members: {ls:?}"
        );
    }

    /// The declaration-site twin: with the CORE snapshot (dsl 0.9.0 D-A — no
    /// shipped members), the only source of `anchor`'s members is the document's
    /// OWN inline `enums:` projection, so these candidates prove completion
    /// resolves through the same `merge_domains` seam `check()` does.
    #[test]
    fn completion_of_inline_declared_domain_values() {
        let text = "---\nkind: scene\ncharacter: marina\nseason: 1\nepisode: 2\n\
                    enums:\n  anchor:\n    members: [portside, midships, starboard]\n    \
                    default: midships\n---\n## Shot 1.\n\
                    ::actor{character=\"b\" anchor=\"\"}\n";
        let doc = parsed(text);
        let off = text.find("anchor=\"\"").unwrap() + "anchor=\"".len();
        let items = complete_at(&doc, text, &load_core_snapshot(), &ProviderSet::default(), &SchemaImports::default(), off);
        let ls = labels(&items);
        assert!(
            ls.contains(&"portside") && ls.contains(&"midships") && ls.contains(&"starboard"),
            "inline-declared anchor members: {ls:?}"
        );
    }

    #[test]
    fn completion_of_def_names_after_at() {
        let text = "---\nkind: scene\ncharacter: marina\nseason: 1\nepisode: 2\ndefs:\n  fond: { type: bool, cel: \"scene.x >= 1\" }\n---\n## Shot 1.\n::set{scene.y = @}\n";
        let doc = parsed(text);
        let off = text.find("= @").unwrap() + 3; // just past `@`
        let items = complete_at(&doc, text, &load_core_snapshot(), &ProviderSet::default(), &SchemaImports::default(), off);
        assert!(
            items.iter().any(|i| i.label == "fond"),
            "offers def name: {:?}",
            labels(&items)
        );
    }

    #[test]
    fn completion_of_choice_ids_in_match_subject() {
        let text = "## Shot 1.\n<branch id=\"number\">\n  <choice id=\"a\" text=\"A\">\n    @f: a.\n  </choice>\n</branch>\n<match subject=\"\">\n  <otherwise>\n    @f: x.\n  </otherwise>\n</match>\n";
        let doc = parsed(text);
        let off = text.find("subject=\"").unwrap() + "subject=\"".len(); // inside the empty subject
        let items = complete_at(&doc, text, &load_core_snapshot(), &ProviderSet::default(), &SchemaImports::default(), off);
        assert!(
            items.iter().any(|i| i.label == "scene.choices.number"),
            "offers the choice path: {:?}",
            labels(&items)
        );
    }

    /// D2: `<match subject="">` subject completion must offer a `<branch id="inner">`
    /// that is nested inside a `<hub>` choice body (`collect_branch_ids` must
    /// descend into hub choices).
    #[test]
    fn completion_of_choice_ids_offers_hub_nested_branch() {
        let text = "## Shot 1.\n<hub id=\"chat\">\n<choice id=\"ask\" text=\"Ask\" once>\n<branch id=\"inner\">\n<choice id=\"a\" text=\"A\">\n@f: a.\n</choice>\n</branch>\n</choice>\n<choice id=\"leave\" text=\"Leave\" exit>\n@f: bye.\n</choice>\n</hub>\n<match subject=\"\">\n<otherwise>\n@f: x.\n</otherwise>\n</match>\n";
        let doc = parsed(text);
        let off = text.find("subject=\"").unwrap() + "subject=\"".len(); // inside the empty subject
        let items = complete_at(&doc, text, &load_core_snapshot(), &ProviderSet::default(), &SchemaImports::default(), off);
        assert!(
            items.iter().any(|i| i.label == "scene.choices.inner"),
            "offers the hub-nested branch choice path: {:?}",
            labels(&items)
        );
    }

    /// D2: a `<hub>` folds an implicit `scene.choices.<hubId>` enum (same shape
    /// as a `<branch>`), so `<match subject="">` subject completion must offer the
    /// hub's own id, not just ids nested inside its choice bodies.
    #[test]
    fn completion_of_choice_ids_offers_hub_own_id() {
        let text = "## Shot 1.\n<hub id=\"chatWithMarina\">\n<choice id=\"ask\" text=\"Ask\" once>\n@f: a.\n</choice>\n<choice id=\"leave\" text=\"Leave\" exit>\n@f: bye.\n</choice>\n</hub>\n<match subject=\"\">\n<otherwise>\n@f: x.\n</otherwise>\n</match>\n";
        let doc = parsed(text);
        let off = text.find("subject=\"").unwrap() + "subject=\"".len(); // inside the empty subject
        let items = complete_at(&doc, text, &load_core_snapshot(), &ProviderSet::default(), &SchemaImports::default(), off);
        assert!(
            items
                .iter()
                .any(|i| i.label == "scene.choices.chatWithMarina"),
            "offers the hub's own choice path: {:?}",
            labels(&items)
        );
    }

    /// D2: attr-key completion for a directive nested inside a `<hub>` choice
    /// body must dedupe an already-written key (`present_attr_keys` must descend
    /// into hub choices).
    #[test]
    fn attr_key_completion_dedupes_present_keys_in_hub_choice() {
        let text = "## Shot 1.\n<hub id=\"chat\">\n<choice id=\"ask\" text=\"Ask\" once>\n::camera{focus=\"b\" }\n</choice>\n<choice id=\"leave\" text=\"Leave\" exit>\n@f: bye.\n</choice>\n</hub>\n";
        let doc = parsed(text);
        // Cursor in the whitespace after the first attr (still the attr area).
        let off = text.find("\" }").unwrap() + 2;
        let items = complete_at(&doc, text, &load_core_snapshot(), &ProviderSet::default(), &SchemaImports::default(), off);
        let ls = labels(&items);
        assert!(
            !ls.contains(&"focus"),
            "focus already present in hub-nested directive, should be gone: {ls:?}"
        );
        assert!(ls.contains(&"framing"), "framing still offered: {ls:?}");
    }

    #[test]
    fn completion_of_state_paths_in_set_expr() {
        let text = "---\nkind: scene\ncharacter: marina\nseason: 1\nepisode: 2\nstate:\n  scene.affect.marina: { type: int, default: 0 }\n---\n## Shot 1.\n::set{scene.affect.marina = }\n";
        let doc = parsed(text);
        // Cursor after the `=` (expr slot) — state paths are offered.
        let off = text.rfind("= }").unwrap() + 2;
        let items = complete_at(&doc, text, &load_core_snapshot(), &ProviderSet::default(), &SchemaImports::default(), off);
        assert!(
            items.iter().any(|i| i.label == "scene.affect.marina"),
            "offers declared state path: {:?}",
            labels(&items)
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
                    ty: Some(Type::Int),
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
    fn completion_offers_emotion_enum() {
        // Cursor after the 3rd `.` → segment idx 3 (emotion enum).
        let text = "## Shot 1.\n::portrait{assetId=\"CH.marina.waitress.\"}\n";
        let doc = parsed(text);
        let off = text.find("waitress.").unwrap() + "waitress.".len();
        let items = complete_at(&doc, text, &asset_snapshot(), &ProviderSet::default(), &SchemaImports::default(), off);
        let ls = labels(&items);
        assert!(
            ls.contains(&"delighted") && ls.contains(&"content") && ls.contains(&"neutral"),
            "emotion enum members: {ls:?}"
        );
    }

    #[test]
    fn completion_offers_const_prefix() {
        // Cursor within the prefix segment (idx 0) → the const `CH`.
        let text = "## Shot 1.\n::portrait{assetId=\"CH.marina.waitress.delighted.3\"}\n";
        let doc = parsed(text);
        let off = text.find("CH.marina").unwrap() + 1; // on the `H` of `CH`
        let items = complete_at(&doc, text, &asset_snapshot(), &ProviderSet::default(), &SchemaImports::default(), off);
        let ls = labels(&items);
        assert!(ls.contains(&"CH"), "const prefix offered: {ls:?}");
    }

    #[test]
    fn completion_offers_provider_ids() {
        use lute_manifest::provider::ProviderSnapshot;
        use std::collections::BTreeMap;
        // Cursor within the `characterId` segment (idx 1), typed
        // `providerRef("character")`. The pinned snapshot lists two ids.
        let text = "## Shot 1.\n::portrait{assetId=\"CH.marina.waitress.delighted.3\"}\n";
        let doc = parsed(text);
        let off = text.find("marina").unwrap() + 2; // inside `marina`, segment idx 1
        let providers = ProviderSet::from_one(ProviderSnapshot {
            manifest_version: "1".to_string(),
            provider_version: "1".to_string(),
            entries: BTreeMap::from([(
                "character".to_string(),
                vec!["marina".to_string(), "ren".to_string()],
            )]),
            stale: false,
        });
        let items = complete_at(&doc, text, &asset_snapshot(), &providers, &SchemaImports::default(), off);
        let ls = labels(&items);
        assert!(
            ls.contains(&"marina") && ls.contains(&"ren"),
            "providerRef segment offers pinned ids: {ls:?}"
        );
        // An empty ProviderSet offers nothing for that segment (honest §6.9).
        let empty = complete_at(&doc, text, &asset_snapshot(), &ProviderSet::default(), &SchemaImports::default(), off);
        assert!(
            empty.is_empty(),
            "empty ProviderSet -> no provider ids: {:?}",
            labels(&empty)
        );
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
                ty: Type::Int,
                default: None,
                namespace: Namespace::Run,
                owner: None,
            },
        );
        imports.defs.insert(
            "helped".to_string(),
            serde_yaml::from_str("{ type: bool, cel: \"true\" }").unwrap(),
        );
        imports
    }

    #[test]
    fn completion_offers_imported_state_path() {
        // `run.gold` is only imported via `uses:`, not declared inline.
        let text =
            "---\nkind: scene\ncharacter: marina\nseason: 1\nepisode: 2\n---\n## Shot 1.\n::set{run.gold = }\n";
        let doc = parsed(text);
        let off = text.rfind("= }").unwrap() + 2;
        let items = complete_at(&doc, text, &load_core_snapshot(), &ProviderSet::default(), &schema_imports(), off);
        assert!(
            items.iter().any(|i| i.label == "run.gold"),
            "offers imported state path: {:?}",
            labels(&items)
        );
    }

    #[test]
    fn completion_offers_imported_def_name() {
        // `@helped` is only imported via `uses:`, not declared inline.
        let text =
            "---\nkind: scene\ncharacter: marina\nseason: 1\nepisode: 2\n---\n## Shot 1.\n::set{scene.y = @}\n";
        let doc = parsed(text);
        let off = text.find("= @").unwrap() + 3;
        let items = complete_at(&doc, text, &load_core_snapshot(), &ProviderSet::default(), &schema_imports(), off);
        assert!(
            items.iter().any(|i| i.label == "helped"),
            "offers imported def name: {:?}",
            labels(&items)
        );
    }

    /// D3: completion inside a `<when is="…">` value whose `<match>` subject is a
    /// declared enum offers the enum members ∪ `unset` (dsl §7.3.1) — not CEL
    /// state paths (the pre-D3 fall-through when `is` was discarded).
    #[test]
    fn completion_in_when_is_offers_enum_members() {
        let text = "---\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  scene.serve.debut.rank: { type: { enum: [gold, silver, bronze] } }\n---\n## Shot 1.\n<match subject=\"scene.serve.debut.rank\">\n<when is=\"gold\">\n@fixer: nice.\n</when>\n<otherwise>\n@fixer: ok.\n</otherwise>\n</match>\n";
        let doc = parsed(text);
        let off = text.find("is=\"gold\"").unwrap() + "is=\"".len() + 1; // inside "gold"
        let items = complete_at(&doc, text, &load_core_snapshot(), &ProviderSet::default(), &SchemaImports::default(), off);
        let ls = labels(&items);
        assert!(
            ls.contains(&"gold")
                && ls.contains(&"silver")
                && ls.contains(&"bronze")
                && ls.contains(&"unset"),
            "offers enum members ∪ unset: {ls:?}"
        );
    }

    /// D3: `<match subject="scene.choices.chat">` over a top-level `<hub id="chat">`
    /// (choices askCoffee/leave) — `is=` completion offers the hub's choice ids ∪
    /// `unset` (the implicit recording enum, dsl §11.1.3).
    #[test]
    fn completion_in_when_is_offers_hub_choice_ids() {
        let text = "## Shot 1.\n<hub id=\"chat\">\n<choice id=\"askCoffee\" text=\"Coffee?\" once>\n@f: a.\n</choice>\n<choice id=\"leave\" text=\"Bye\" exit>\n@f: bye.\n</choice>\n</hub>\n<match subject=\"scene.choices.chat\">\n<when is=\"askCoffee\">\n@f: x.\n</when>\n<otherwise>\n@f: y.\n</otherwise>\n</match>\n";
        let doc = parsed(text);
        let off = text.find("is=\"askCoffee\"").unwrap() + "is=\"".len() + 1;
        let items = complete_at(&doc, text, &load_core_snapshot(), &ProviderSet::default(), &SchemaImports::default(), off);
        let ls = labels(&items);
        assert!(
            ls.contains(&"askCoffee") && ls.contains(&"leave") && ls.contains(&"unset"),
            "offers hub choice ids ∪ unset: {ls:?}"
        );
    }

    /// D3: `<match subject="scene.visited.chat.askCoffee">` over a `<hub id="chat">`
    /// with a `<choice id="askCoffee">` — the folded per-choice bool (dsl §9.6,
    /// §11.1.3), so `is=` completion offers true/false/unset.
    #[test]
    fn completion_in_when_is_offers_visited_bool() {
        let text = "## Shot 1.\n<hub id=\"chat\">\n<choice id=\"askCoffee\" text=\"Coffee?\" once>\n@f: a.\n</choice>\n<choice id=\"leave\" text=\"Bye\" exit>\n@f: bye.\n</choice>\n</hub>\n<match subject=\"scene.visited.chat.askCoffee\">\n<when is=\"true\">\n@f: x.\n</when>\n<otherwise>\n@f: y.\n</otherwise>\n</match>\n";
        let doc = parsed(text);
        let off = text.find("is=\"true\"").unwrap() + "is=\"".len() + 1;
        let items = complete_at(&doc, text, &load_core_snapshot(), &ProviderSet::default(), &SchemaImports::default(), off);
        let ls = labels(&items);
        assert!(
            ls.contains(&"true") && ls.contains(&"false") && ls.contains(&"unset"),
            "offers bool ∪ unset: {ls:?}"
        );
    }

    /// D3 regression: a cursor on a `<when test="…">` value is UNCHANGED — still a
    /// CEL slot (offers def names), never the `is=` literal domain.
    #[test]
    fn completion_on_when_test_is_unchanged_cel() {
        let text = "---\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  scene.serve.debut.rank: { type: { enum: [gold, silver, bronze] } }\ndefs:\n  warm: { type: bool, cel: \"true\" }\n---\n## Shot 1.\n<match subject=\"scene.serve.debut.rank\">\n<when test=\"@warm\">\n@fixer: nice.\n</when>\n<otherwise>\n@fixer: ok.\n</otherwise>\n</match>\n";
        let doc = parsed(text);
        let off = text.find("@warm").unwrap() + 1; // just past `@`
        let items = complete_at(&doc, text, &load_core_snapshot(), &ProviderSet::default(), &SchemaImports::default(), off);
        let ls = labels(&items);
        assert!(
            ls.contains(&"warm"),
            "test= still CEL: offers def name: {ls:?}"
        );
        assert!(
            !ls.contains(&"unset") && !ls.contains(&"gold"),
            "test= must NOT offer the is= literal domain: {ls:?}"
        );
    }

    /// D3 fix (non-CEL subject): a hyphenated `<match subject="scene.choices.pick-one">`
    /// subject is NOT a pure CEL path — cel-parser reads `pick-one` as subtraction,
    /// so the checker's `subject_path` reconstruction (parse + `select_path`)
    /// yields `None` (an INFINITE subject, no `is=` menu). Even with a `<branch
    /// id="pick-one">` literally present, `is=` completion must offer NOTHING,
    /// matching the checker (the pre-fix `is_path_byte` scan wrongly offered its
    /// choices because `-` is a path byte).
    #[test]
    fn completion_in_when_is_rejects_non_cel_subject() {
        let text = "## Shot 1.\n<branch id=\"pick-one\">\n<choice id=\"a\" text=\"A\">\n@f: a.\n</choice>\n<choice id=\"b\" text=\"B\">\n@f: b.\n</choice>\n</branch>\n<match subject=\"scene.choices.pick-one\">\n<when is=\"a\">\n@f: x.\n</when>\n<otherwise>\n@f: y.\n</otherwise>\n</match>\n";
        let doc = parsed(text);
        let off = text.find("is=\"a\"").unwrap() + "is=\"".len() + 1;
        let items = complete_at(&doc, text, &load_core_snapshot(), &ProviderSet::default(), &SchemaImports::default(), off);
        assert!(
            items.is_empty(),
            "a non-path (hyphenated) subject has no finite domain, so `is=` offers nothing: {:?}",
            labels(&items)
        );
    }

    /// D3 fix (last-wins dup): a DUPLICATE `<branch id>` folds last-wins in the
    /// checker (`fold_branches` -> `schema.decls.insert` overwrites), even as it
    /// emits `E-DUP-BRANCH`. So `is=` completion over `scene.choices.dup` must
    /// offer the LAST `<branch id="dup">`'s choice ids (∪ unset), never the
    /// first's — the pre-fix walk early-returned on the FIRST match.
    #[test]
    fn completion_in_when_is_uses_last_duplicate_branch() {
        let text = "## Shot 1.\n<branch id=\"dup\">\n<choice id=\"first1\" text=\"F1\">\n@f: a.\n</choice>\n<choice id=\"first2\" text=\"F2\">\n@f: b.\n</choice>\n</branch>\n<branch id=\"dup\">\n<choice id=\"last1\" text=\"L1\">\n@f: c.\n</choice>\n<choice id=\"last2\" text=\"L2\">\n@f: d.\n</choice>\n</branch>\n<match subject=\"scene.choices.dup\">\n<when is=\"last1\">\n@f: x.\n</when>\n<otherwise>\n@f: y.\n</otherwise>\n</match>\n";
        let doc = parsed(text);
        let off = text.find("is=\"last1\"").unwrap() + "is=\"".len() + 1;
        let items = complete_at(&doc, text, &load_core_snapshot(), &ProviderSet::default(), &SchemaImports::default(), off);
        let ls = labels(&items);
        assert!(
            ls.contains(&"last1") && ls.contains(&"last2") && ls.contains(&"unset"),
            "offers the LAST duplicate branch's choice ids u unset: {ls:?}"
        );
        assert!(
            !ls.contains(&"first1") && !ls.contains(&"first2"),
            "must NOT offer the FIRST (overwritten) branch's choice ids: {ls:?}"
        );
    }

    // ---- dsl 0.2.0 §4/§6.3/§6.4: quest/on/objective + kind: completion ----

    fn complete(text: &str, off: usize) -> Vec<CompletionItem> {
        let doc = parsed(text);
        complete_at(&doc, text, &load_core_snapshot(), &ProviderSet::default(), &SchemaImports::default(), off)
    }

    #[test]
    fn on_event_value_completion_lists_builtin_lifecycle_events() {
        let text =
            "---\nkind: quest\n---\n<quest id=\"q\">\n<on event=\"quest\">\n</on>\n</quest>\n";
        let off = text.find("\"quest\"").unwrap() + 1;
        let items = complete(text, off);
        let ls = labels(&items);
        assert!(ls.contains(&"questComplete"), "{ls:?}");
        assert!(ls.contains(&"questActive"), "{ls:?}");
        assert!(ls.contains(&"questFailed"), "{ls:?}");
    }

    #[test]
    fn objective_attr_area_completion_lists_done_visible_when_optional() {
        let text = "---\nkind: quest\n---\n<quest id=\"q\">\n<objective id=\"o\" done=\"a\">\n</objective>\n</quest>\n";
        let off = text.find("<objective ").unwrap() + "<objective ".len();
        let items = complete(text, off);
        let ls = labels(&items);
        for k in ["id", "done", "visibleWhen", "title", "optional"] {
            assert!(ls.contains(&k), "missing {k}: {ls:?}");
        }
    }

    /// 0.21.1 T1-7: the checker closes these constructs' attribute sets
    /// (`E-UNKNOWN-ATTR`), so completion must offer exactly the permitted keys
    /// — a key offered here but rejected there (or legal there but never
    /// offered, like `after`/`quest=` were) is a drift bug.
    #[test]
    fn construct_attr_keys_match_the_checker_tables() {
        use lute_check::logic_attrs::{
            ENTRY_ATTRS, HUB_ATTRS, OBJECTIVE_ATTRS, ON_ATTRS, QUEST_ATTRS,
        };
        use lute_check::BUNDLE_BEAT_ATTRS;
        for (construct, table) in [
            (QuestConstruct::Quest, QUEST_ATTRS),
            (QuestConstruct::Objective, OBJECTIVE_ATTRS),
            (QuestConstruct::On, ON_ATTRS),
            (QuestConstruct::Entry, ENTRY_ATTRS),
            (QuestConstruct::Beat, BUNDLE_BEAT_ATTRS),
            (QuestConstruct::Hub, HUB_ATTRS),
        ] {
            let mut offered: Vec<&str> = construct_attr_keys(construct)
                .iter()
                .map(|(k, _)| *k)
                .collect();
            let mut permitted = table.to_vec();
            offered.sort_unstable();
            permitted.sort_unstable();
            assert_eq!(offered, permitted, "{construct:?}");
        }
    }

    /// dsl 0.23.0 §4: a cursor inside a `<hub …>` open tag offers `prompt`
    /// beside `id`; a cursor in the hub body past the first choice offers
    /// no hub attr keys.
    #[test]
    fn hub_attr_area_completion_offers_prompt() {
        let text =
            "## Shot 1.\n<hub id=\"h\" >\n<choice id=\"a\" text=\"A\" once>\n@f: a.\n</choice>\n\
                    \n<choice id=\"leave\" text=\"Leave\" exit>\n@f: bye.\n</choice>\n</hub>\n";
        let off = text.find("\" >").unwrap() + 2;
        let ls: Vec<String> = labels(&complete(text, off))
            .into_iter()
            .map(str::to_string)
            .collect();
        assert_eq!(ls, vec!["id".to_string(), "prompt".to_string()]);
        let between = text.find("\n\n<choice id=\"leave\"").unwrap() + 1;
        assert!(complete(text, between).is_empty());
    }

    #[test]
    fn on_attr_area_completion_lists_event_and_when() {
        let text = "---\nkind: quest\n---\n<quest id=\"q\">\n<on event=\"questComplete\">\n</on>\n</quest>\n";
        let off = text.find("<on ").unwrap() + "<on ".len();
        let items = complete(text, off);
        let ls = labels(&items);
        assert!(ls.contains(&"event"), "{ls:?}");
        assert!(ls.contains(&"when"), "{ls:?}");
    }

    #[test]
    fn quest_attr_area_completion_lists_id_title_start_fail() {
        let text =
            "---\nkind: quest\n---\n<quest id=\"q\">\n<objective id=\"o\" done=\"a\"/>\n</quest>\n";
        let off = text.find("<quest ").unwrap() + "<quest ".len();
        let items = complete(text, off);
        let ls = labels(&items);
        for k in ["id", "title", "start", "fail"] {
            assert!(ls.contains(&k), "missing {k}: {ls:?}");
        }
    }

    #[test]
    fn kind_frontmatter_value_completion_lists_scene_quest_and_lore() {
        let text = "---\nkind: \n---\n";
        let off = text.find("kind: ").unwrap() + "kind: ".len();
        let items = complete(text, off);
        let ls = labels(&items);
        assert!(ls.contains(&"scene"), "{ls:?}");
        assert!(ls.contains(&"quest"), "{ls:?}");
        assert!(ls.contains(&"lore"), "{ls:?}");
    }

    /// dsl 0.19.0 §3: a cursor inside an `<entry …>` open tag offers the
    /// seven entry attributes.
    #[test]
    fn entry_attr_area_completion_lists_all_seven_attrs() {
        let text = "---\nkind: lore\n---\n<entry id=\"e\" >\n@narrator: hi\n</entry>\n";
        let off = text.find("\" >").unwrap() + 2;
        let items = complete(text, off);
        let ls = labels(&items);
        for k in [
            "id", "target", "category", "title", "series", "order", "when", "on", "priority",
            "once",
        ] {
            assert!(ls.contains(&k), "missing {k}: {ls:?}");
        }
    }

    /// dsl 0.23.0 §4: a cursor inside a lore `<beat …>` open tag offers the
    /// beat attributes — `once` documented with its values (dsl 0.24.0 §1
    /// adds the clock's `day`/`slot`) — and never an entry-only key.
    #[test]
    fn beat_attr_area_completion_lists_beat_attrs() {
        let text =
            "---\nid: ship.records\nkind: lore\n---\n<beat id=\"b\" >\n@narrator: hi\n</beat>\n";
        let off = text.find("\" >").unwrap() + 2;
        let items = complete(text, off);
        let ls = labels(&items);
        for k in [
            "id", "on", "target", "title", "when", "priority", "once", "also",
        ] {
            assert!(ls.contains(&k), "missing {k}: {ls:?}");
        }
        for k in ["category", "series", "order"] {
            assert!(!ls.contains(&k), "entry-only {k} offered on a beat: {ls:?}");
        }
        let once = items.iter().find(|i| i.label == "once").unwrap();
        assert_eq!(
            once.detail.as_deref(),
            Some("\"run\" | \"user\" | \"false\" | \"day\" | \"week\" | \"slot\" | \"season:<name>\"")
        );
    }

    #[test]
    fn kind_frontmatter_key_position_offers_no_completion() {
        // The cursor on the `kind` KEY itself (not the value) is not a
        // completion position — only the value half detects.
        let text = "---\nkind: scene\n---\n";
        let off = text.find("kind").unwrap() + 1;
        assert!(complete(text, off).is_empty());
    }

    #[test]
    fn content_line_attr_key_completion_offers_delivery_flags_and_emotion() {
        // Cursor on the KEY half of an existing content-line attr (`code`, with
        // a `=` + quoted value so it's NOT a bareword `BoolTrue` attr, whose
        // key/value spans coincide and would resolve as an `AttrValue`
        // instead — resolves to `Cursor::AttrKey { directive: None, .. }`
        // (dsl §7.1 content lines have no owning directive/capability schema).
        let text = "## Shot 1.\n@x{code=\"c1\"}: hi\n";
        let off = text.find("code").unwrap() + 2; // inside `code`, before `=`
        let items = complete(text, off);
        let ls = labels(&items);
        for f in ["mono", "os", "vo"] {
            assert!(ls.contains(&f), "missing {f}: {ls:?}");
        }
        assert!(ls.contains(&"emotion"), "missing emotion: {ls:?}");
        assert!(
            !ls.contains(&"delivery"),
            "0.2.1 delivery key retired: {ls:?}"
        );
    }

    #[test]
    fn content_line_attr_key_completion_offers_when() {
        // dsl 0.4.0 §7.2: `when=` joins the content-line attr vocabulary — it
        // is extracted into `Line.when` at parse time (never left in
        // `l.attrs`), but the KEY itself still belongs in the completable
        // set when the cursor sits on a DIFFERENT existing attr's key.
        let text = "## Shot 1.\n@x{code=\"c1\"}: hi\n";
        let off = text.find("code").unwrap() + 2; // inside `code`, before `=`
        let items = complete(text, off);
        let ls = labels(&items);
        assert!(ls.contains(&"when"), "missing when: {ls:?}");
    }

    #[test]
    fn content_line_delivery_flag_key_has_no_value_completion() {
        // `mono`/`os`/`vo` are bare boolean flags (dsl 0.2.2 §D7) — no closed
        // `key="value"` domain (retires 0.2.1's 3-member `delivery=""` enum).
        let text = "## Shot 1.\n@x{mono=\"\"}: hi\n";
        let off = text.find("mono=\"").unwrap() + "mono=\"".len();
        assert!(complete(text, off).is_empty());
    }

    #[test]
    fn content_line_emotion_value_without_a_declared_domain_offers_nothing() {
        // The core snapshot ships no `emotion` members, so with no project
        // vocabulary the domain slot has nothing to offer.
        let text = "## Shot 1.\n@x{emotion=\"\"}: hi\n";
        let off = text.find("emotion=\"").unwrap() + "emotion=\"".len();
        assert!(complete(text, off).is_empty());
    }

    #[test]
    fn speaker_completion_offers_narrator_with_no_provider() {
        // Cursor on the speaker NAME itself -> `Cursor::Speaker`. No provider
        // snapshot pinned, so only the `narrator` keyword is offered.
        let text = "## Shot 1.\n@nar: hi\n";
        let off = text.find("@nar").unwrap() + 2; // inside `nar`
        let items = complete(text, off);
        let ls = labels(&items);
        assert_eq!(ls, vec!["narrator"], "narrator-only, no catalog: {ls:?}");
    }

    #[test]
    fn speaker_completion_offers_character_ids_when_provider_pinned() {
        use lute_manifest::provider::ProviderSnapshot;
        use std::collections::BTreeMap;
        let text = "## Shot 1.\n@bia: hi\n";
        let doc = parsed(text);
        let off = text.find("@bia").unwrap() + 2; // inside `bia`
        let providers = ProviderSet::from_one(ProviderSnapshot {
            manifest_version: "1".to_string(),
            provider_version: "1".to_string(),
            entries: BTreeMap::from([(
                "character".to_string(),
                vec!["marina".to_string(), "ren".to_string()],
            )]),
            stale: false,
        });
        let items = complete_at(&doc, text, &load_core_snapshot(), &providers, &SchemaImports::default(), off);
        let ls = labels(&items);
        assert!(
            ls.contains(&"marina") && ls.contains(&"ren") && ls.contains(&"narrator"),
            "catalog ids + narrator: {ls:?}"
        );
    }

    #[test]
    fn speaker_completion_offers_the_declared_cast_with_names() {
        // dsl 0.23.0 §7: a declared cast is what checks, so it replaces the
        // catalog; each member's display name rides along as the detail.
        let text = "## Shot 1.\n@ma: hi\n";
        let doc = parsed(text);
        let off = text.find("@ma").unwrap() + 2;
        let mut imports = SchemaImports::default();
        imports.cast.insert(
            "maud".into(),
            lute_manifest::schema::CastMember {
                id: "maud".into(),
                name: Some("Maud".into()),
                ..Default::default()
            },
        );
        let items = complete_at(&doc, text, &load_core_snapshot(), &ProviderSet::default(), &imports, off);
        assert_eq!(labels(&items), vec!["maud", "narrator"]);
        assert_eq!(items[0].detail.as_deref(), Some("Maud"));
    }

    fn complete_vocab(text: &str, off: usize) -> Vec<String> {
        let items = complete_at(
            &parsed(text),
            text,
            &lute_test_vocab::vocab_snapshot(),
            &ProviderSet::default(),
            &SchemaImports::default(),
            off,
        );
        items.into_iter().map(|i| i.label).collect()
    }

    /// dsl 0.37.0 §3.4: content-line keys are the checker's table — no
    /// authored `id` (lines derive their ids) and no `voiceKey` (derived) —
    /// with the `mono`/`os`/`vo` role flags; a bare key being typed completes.
    #[test]
    fn content_line_keys_are_the_037_set() {
        let text = "## Shot 1.\n@mira{mo}: hi\n";
        let off = text.find("{mo").unwrap() + 3;
        let ls = complete_vocab(text, off);
        for key in ["code", "emotion", "action", "mono", "os", "vo", "when"] {
            assert!(ls.iter().any(|l| l == key), "missing {key}: {ls:?}");
        }
        for gone in ["id", "voiceKey", "delivery"] {
            assert!(!ls.iter().any(|l| l == gone), "{gone} must not be offered: {ls:?}");
        }
    }

    /// The narrator carries no delivery flag (`E-DELIVERY-NARRATOR`), and one
    /// written flag excludes the other two (`E-DELIVERY-CONFLICT`).
    #[test]
    fn content_line_delivery_flags_respect_narrator_and_exclusivity() {
        let text = "## Shot 1.\n@narrator{code=\"a\"}: hi\n@mira{os code=\"b\"}: hi\n";
        let narrator = complete_vocab(text, text.find("code=\"a\"").unwrap() + 1);
        assert!(!narrator.iter().any(|l| ["mono", "os", "vo"].contains(&l.as_str())), "{narrator:?}");
        let flagged = complete_vocab(text, text.find("code=\"b\"").unwrap() + 1);
        assert!(!flagged.iter().any(|l| ["mono", "vo"].contains(&l.as_str())), "{flagged:?}");
    }

    /// A content line's `emotion` value offers the merged `emotion` domain.
    #[test]
    fn content_line_emotion_value_offers_domain_members() {
        let text = "## Shot 1.\n@mira{emotion=\"\"}: hi\n";
        let off = text.find("emotion=\"").unwrap() + "emotion=\"".len();
        let ls = complete_vocab(text, off);
        assert!(!ls.is_empty(), "the vocab emotion domain is offered");
        let vocab = lute_test_vocab::test_domains();
        for member in &vocab["emotion"].members {
            assert!(ls.contains(member), "missing {member}: {ls:?}");
        }
    }

    /// The directive list is the 0.37 snapshot: `actor`/`bg`/`cg`/`sequence`/
    /// `jump`/`label` are offered; the removed spellings are not.
    #[test]
    fn directive_names_are_the_037_set() {
        let text = "## Shot 1.\n::";
        let ls = complete_vocab(text, text.len());
        for name in ["actor", "bg", "cg", "sequence", "jump", "label", "camera", "music"] {
            assert!(ls.iter().any(|l| l == name), "missing {name}: {ls:?}");
        }
        for gone in ["auto", "cut", "next", "mark"] {
            assert!(!ls.iter().any(|l| l == gone), "{gone} removed: {ls:?}");
        }
    }

    /// `::camera` keys are the domain-valued slots plus timing; the removed
    /// numeric fields are gone.
    #[test]
    fn camera_keys_are_domain_slots_and_timing() {
        let text = "## Shot 1.\n::camera{}\n";
        let ls = complete_vocab(text, text.find("{}").unwrap() + 1);
        for key in ["focus", "framing", "move", "transition", "duration", "delay", "wait"] {
            assert!(ls.iter().any(|l| l == key), "missing {key}: {ls:?}");
        }
        for gone in ["zoom", "moveX", "moveY", "shake", "reset", "easing"] {
            assert!(!ls.iter().any(|l| l == gone), "{gone} removed: {ls:?}");
        }
        let cg = "## Shot 1.\n::cg{}\n";
        let ls = complete_vocab(cg, cg.find("{}").unwrap() + 1);
        for key in ["assetId", "display", "layout"] {
            assert!(ls.iter().any(|l| l == key), "missing {key}: {ls:?}");
        }
        assert!(!ls.iter().any(|l| l == "full" || l == "action"), "{ls:?}");
        let clear = "## Shot 1.\n::clear{}\n";
        assert!(complete_vocab(clear, clear.find("{}").unwrap() + 1).is_empty());
    }

    /// Domain-typed staging values offer their domain's members: `framing`
    /// and `move` from the project vocabulary, `transition`, `layout`,
    /// sequence `name` and music `playback` from inline / shared domains.
    #[test]
    fn staging_domain_values_offer_members() {
        let text = "---\nenums:\n  transition: [fade, wipe]\n  cgLayout: [full, inset]\n  sequence: [intro, outro]\n---\n## Shot 1.\n::camera{framing=\"\" move=\"\" transition=\"\"}\n::cg{assetId=\"a\" layout=\"\"}\n::sequence{name=\"\"}\n::music{playback=\"\"}\n";
        let at = |needle: &str| text.find(needle).unwrap() + needle.len();
        let cases: [(&str, &[&str]); 6] = [
            ("framing=\"", &["close", "tight"]),
            ("move=\"", &["shake"]),
            ("transition=\"", &["fade", "wipe"]),
            ("layout=\"", &["full", "inset"]),
            ("name=\"", &["intro", "outro"]),
            ("playback=\"", &["start", "stop"]),
        ];
        for (needle, want) in cases {
            let ls = complete_vocab(text, at(needle));
            for member in want {
                assert!(ls.iter().any(|l| l == member), "{needle} missing {member}: {ls:?}");
            }
        }
    }

    /// dsl 0.37.0 §3.6: after `:` in line text, the core modifiers and the
    /// project's `textStyle` members are offered; a colon glued to a word is
    /// prose.
    #[test]
    fn inline_modifier_names_offer_core_and_text_styles() {
        let text = "---\nenums:\n  textStyle: [emphasis, whisper]\n---\n## Shot 1.\n@mira: Wait :em now. Note: x\n";
        let ls = complete_vocab(text, text.find(":em").unwrap() + 3);
        for name in ["pause", "speed", "emphasis", "whisper"] {
            assert!(ls.iter().any(|l| l == name), "missing {name}: {ls:?}");
        }
        assert!(complete_vocab(text, text.find("Note:").unwrap() + 5).is_empty());
    }

    /// dsl 0.37.0 §3.5: a `::jump{to}` value offers the document's label
    /// names; a `::label{name}` value offers jump targets no label declares.
    #[test]
    fn jump_and_label_values_offer_label_names() {
        let text = "## Shot 1.\n::jump{to=\"\"}\n::jump{to=\"later\"}\n::label{name=\"here\"}\n@f: x.\n## Two\n::label{name=\"\"}\n::label{name=\"there\"}\n@f: y.\n";
        let to = complete_vocab(text, text.find("to=\"\"").unwrap() + "to=\"".len());
        assert_eq!(to, ["here", "there"]);
        let name = complete_vocab(text, text.find("name=\"\"").unwrap() + "name=\"".len());
        assert_eq!(name, ["later"]);
    }

    /// dsl 0.37.0 §3.5: a choice open tag offers `text` (not the removed
    /// `label`), hub choices add `once`/`exit`, written keys drop out, and a
    /// `<match>` open tag offers `subject`.
    #[test]
    fn choice_and_match_open_tags_offer_their_keys() {
        let text = "## Shot 1.\n<branch id=\"b\">\n<choice id=\"a\" >\n@f: a.\n</choice>\n</branch>\n<hub id=\"h\">\n<choice id=\"x\" text=\"X\" >\n@f: x.\n</choice>\n</hub>\n";
        let branch_choice = complete_vocab(text, text.find("\"a\" >").unwrap() + 4);
        assert_eq!(branch_choice, ["text", "when", "into", "value"]);
        let hub_choice = complete_vocab(text, text.find("\"X\" >").unwrap() + 4);
        assert_eq!(hub_choice, ["when", "into", "value", "once", "exit"]);
        let body = complete_vocab(text, text.find("@f: a.").unwrap());
        assert!(!body.iter().any(|l| l == "text"), "the body is not the open tag: {body:?}");
        let m = "## Shot 1.\n<match >\n<otherwise>\n@f: x.\n</otherwise>\n</match>\n";
        assert_eq!(complete_vocab(m, m.find("<match ").unwrap() + 7), ["subject"]);
    }
}
