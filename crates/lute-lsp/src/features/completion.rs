//! `textDocument/completion` (Task 6.3): candidates for the cursor position.
//!
//! A pure function over a parsed [`Document`] + [`CapabilitySnapshot`] + byte
//! offset. Resolves the cursor ([`super::resolve`]) and returns:
//! - after `::` (a directive head) -> directive names;
//! - inside a directive's `{ … }` at a key position -> that directive's attr keys
//!   (per its schema, minus keys already present);
//! - at an enum-typed attr value -> the enum's members;
//! - `@` in a CEL slot -> author `defs:` + snapshot def names;
//! - a `<match on=…>` subject -> `scene.choices.<id>` ids from every `<branch>`;
//! - any other state-path position in CEL -> declared state paths.
//!
//! Empty result (`vec![]`) when nothing is offerable — never a placeholder item.

use std::collections::BTreeSet;

use lute_check::{parse_meta, SchemaImports};
use lute_manifest::provider::ProviderSet;
use lute_manifest::schema::AssetKindDecl;
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_manifest::types::Type;
use lute_syntax::ast::{Arm, AttrValue, Document, Node};
use tower_lsp_server::ls_types::{CompletionItem, CompletionItemKind};

use super::{attr_enum_values, type_label, Cursor, QuestConstruct};

/// Completion candidates at byte offset `off`. Empty when the cursor is somewhere
/// with nothing to offer.
pub fn complete_at(
    doc: &Document,
    snapshot: &CapabilitySnapshot,
    providers: &ProviderSet,
    imports: &SchemaImports,
    off: usize,
) -> Vec<CompletionItem> {
    // `kind:` frontmatter value completion (dsl 0.2.0 §3.1) — `resolve()` is
    // BODY-only (it walks `doc.shots`/`doc.quests`, never the frontmatter
    // YAML), so this is a small dedicated detector, checked first.
    if let Some(items) = kind_value_items(doc, off) {
        return items;
    }
    let (mut meta, _) = parse_meta(&doc.meta, snapshot);
    super::merge_imports(&mut meta, imports);
    let Some(cursor) = super::resolve(doc, off) else {
        return Vec::new();
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
            key,
        } => {
            if let Some(kind) = super::asset_kind_for(snapshot, dir, key) {
                asset_segment_items(kind, doc, providers, off)
            } else {
                enum_value_items(snapshot, imports, dir, key)
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
            } else {
                state_path_items(&meta)
            }
        }
        // A content-line attr key/value (dsl §7.1's `@speaker{…}:` — no owning
        // directive/capability schema, unlike a `::directive`'s attrs).
        Cursor::AttrKey {
            directive: None, ..
        } => content_line_attr_key_items(),
        Cursor::AttrValue {
            directive: None,
            key,
        } => content_line_attr_value_items(key),
        Cursor::SetPath { .. } => state_path_items(&meta),
        // Interp interiors (dsl §7.6) get hover/def/references (Task D1) but no
        // completion — a `{{…}}` referent is authored inline, matching the prior
        // behavior (interps resolved to no cursor before D1).
        Cursor::Interp(_) => Vec::new(),
        Cursor::IsPattern { subject_path } => is_pattern_items(doc, &meta, subject_path),
        Cursor::OnEventValue(_) => event_name_items(snapshot),
        Cursor::ConstructAttrArea { construct } => construct_attr_key_items(construct),
        Cursor::Speaker => speaker_items(providers),
    }
}

/// The fixed (non-snapshot) attr-key set for a `<quest>`/`<on>`/`<objective>`
/// open tag (dsl 0.2.0 §6.3/§4/§6.4), kind `FIELD`. Unlike `::directive`
/// attrs, these three constructs have a small closed set that is NOT
/// capability-schema-driven, so the table is hardcoded here (mirroring how
/// the tree-sitter grammar's `cel_key` enumerates the CEL-valued attr names) —
/// always the full set, since `id`/`title`/`start`/`fail`/`event`/`when`/
/// `done`/`optional` are parsed into dedicated AST fields (not a `Vec<Attr>`),
/// so "already present" cannot be read back generically the way a
/// directive's `attrs` list allows.
fn construct_attr_key_items(construct: QuestConstruct) -> Vec<CompletionItem> {
    let keys: &[(&str, &str)] = match construct {
        QuestConstruct::Quest => &[
            ("id", "string"),
            ("title", "string"),
            ("start", "cel<bool>"),
            ("fail", "cel<bool>"),
        ],
        QuestConstruct::On => &[("event", "string"), ("when", "cel<bool>")],
        QuestConstruct::Objective => &[
            ("id", "string"),
            ("done", "cel<bool>"),
            ("when", "cel<bool>"),
            ("title", "string"),
            ("optional", "bool"),
        ],
    };
    keys.iter()
        .map(|(name, ty)| CompletionItem {
            label: name.to_string(),
            kind: Some(CompletionItemKind::FIELD),
            detail: Some(ty.to_string()),
            ..Default::default()
        })
        .collect()
}

/// Content-line attribute keys (dsl 0.2.2 §7.1, §D7): a `@speaker{…}:` line's
/// fixed 9-key vocabulary is, like [`construct_attr_key_items`]'s three quest
/// constructs, NOT capability-schema-driven (it belongs to the content-line
/// grammar itself, validated by `lute_check::content_line` rather than a
/// [`snapshot`]-declared `DirectiveDecl`) — always the full set; kind `FIELD`.
/// `mono`/`os`/`vo` are bare boolean delivery flags (`AttrValue::BoolTrue`,
/// mutually exclusive — `E-DELIVERY-CONFLICT` on more than one), not
/// `key="value"` attrs, so they carry no completable value domain.
fn content_line_attr_key_items() -> Vec<CompletionItem> {
    const KEYS: &[(&str, &str)] = &[
        ("code", "string"),
        ("emotion", "enum"),
        ("variant", "number"),
        ("action", "string"),
        ("dialogMotion", "string"),
        ("mono", "flag"),
        ("os", "flag"),
        ("vo", "flag"),
        ("as", "string"),
    ];
    KEYS.iter()
        .map(|(name, ty)| CompletionItem {
            label: name.to_string(),
            kind: Some(CompletionItemKind::FIELD),
            detail: Some(ty.to_string()),
            ..Default::default()
        })
        .collect()
}

/// Content-line attribute VALUES: 0.2.2 retires the closed `delivery="…"`
/// enum (dsl §D7 replaces it with the bare `mono`/`os`/`vo` flags, which have
/// no `key="value"` form to complete into) — every content-line key is now
/// `string`/`number`/flag typed with no enumerable value domain, so this
/// offers nothing.
fn content_line_attr_value_items(_key: &str) -> Vec<CompletionItem> {
    Vec::new()
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

/// `@speaker{…}:` name completion (dsl §7.1): pinned `character` catalog ids
/// (kind `VALUE`, as [`asset_segment_items`] offers provider ids) plus the
/// always-valid `narrator` keyword (kind `KEYWORD`). Speaker-id VALIDATION
/// stays out of scope for 0.2.1 (deferred to the 0.2.2 foundation minor) —
/// this only completes + claims the span.
fn speaker_items(providers: &ProviderSet) -> Vec<CompletionItem> {
    character_ids(providers)
        .into_iter()
        .map(|id| CompletionItem {
            label: id,
            kind: Some(CompletionItemKind::VALUE),
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
    let mut names: BTreeSet<String> = lute_manifest::snapshot::BUILTIN_LIFECYCLE_EVENTS
        .iter()
        .map(|s| s.to_string())
        .collect();
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
/// discriminator); `None` when `off` is not there, so the caller falls
/// through to the normal body-cursor resolution. Mirrors
/// `super::find_yaml_key_span`'s line-scan + `FRONTMATTER_BASE` convention.
fn kind_value_items(doc: &Document, off: usize) -> Option<Vec<CompletionItem>> {
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
            ["scene", "quest"]
                .into_iter()
                .map(|k| CompletionItem {
                    label: k.to_string(),
                    kind: Some(CompletionItemKind::ENUM_MEMBER),
                    ..Default::default()
                })
                .collect(),
        );
    }
    None
}

/// Every directive name (`::bg`, `::camera`, …), kind `FUNCTION`.
fn directive_items(snapshot: &CapabilitySnapshot) -> Vec<CompletionItem> {
    snapshot
        .directives
        .values()
        .map(|d| CompletionItem {
            label: d.name.clone(),
            kind: Some(CompletionItemKind::FUNCTION),
            detail: d.layer.as_ref().map(|l| format!("layer {l}")),
            ..Default::default()
        })
        .collect()
}

/// A directive's attribute keys, kind `FIELD`, minus keys already written on the
/// directive/line at the cursor (so the list narrows as attrs are filled in).
fn attr_key_items(
    snapshot: &CapabilitySnapshot,
    directive: &str,
    doc: &Document,
    off: usize,
) -> Vec<CompletionItem> {
    let Some(decl) = snapshot.directive(directive) else {
        return Vec::new();
    };
    let present = present_attr_keys(doc, off);
    decl.attrs
        .iter()
        .filter(|a| !present.contains(&a.name))
        .map(|a| CompletionItem {
            label: a.name.clone(),
            kind: Some(CompletionItemKind::FIELD),
            detail: Some(type_label(&a.ty)),
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
    directive: &str,
    key: &str,
) -> Vec<CompletionItem> {
    attr_enum_values(snapshot, imports, directive, key)
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
    let Some(attr) = super::attr_at(doc, off) else {
        return Vec::new();
    };
    let AttrValue::Str(value) = &attr.value else {
        return Vec::new();
    };
    let idx = super::asset_segment_index(kind, value, attr.value_span.byte_start, off);
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

/// `scene.choices.<id>` ids from every `<branch>` (for a `<match on=…>` subject).
fn choice_path_items(doc: &Document) -> Vec<CompletionItem> {
    let mut ids = Vec::new();
    for shot in &doc.shots {
        collect_branch_ids(&shot.body, &mut ids);
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
    let Some(domain) = super::subject_domain(doc, meta, subject_path) else {
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
                let id = h.attrs.iter().find(|a| a.key == "id").and_then(|a| match &a.value {
                    AttrValue::Str(s) => Some(s.as_str()),
                    _ => None,
                });
                if let Some(id) = id {
                    if !id.is_empty() {
                        out.push(id.to_string());
                    }
                }
                for c in &h.choices {
                    collect_branch_ids(&c.body, out);
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
                Node::Directive(d) if super::span_contains(d.span, off) => {
                    out.extend(d.attrs.iter().map(|a| a.key.clone()));
                }
                Node::Line(l) if super::span_contains(l.span, off) => {
                    out.extend(l.attrs.iter().map(|a| a.key.clone()));
                }
                Node::Branch(b) if super::span_contains(b.span, off) => {
                    for c in &b.choices {
                        scan(&c.body, off, out);
                    }
                }
                Node::Hub(h) if super::span_contains(h.span, off) => {
                    for c in &h.choices {
                        scan(&c.body, off, out);
                    }
                }
                Node::Match(m) if super::span_contains(m.span, off) => {
                    for arm in &m.arms {
                        let body = match arm {
                            Arm::When { body, .. } | Arm::Otherwise { body, .. } => body,
                        };
                        scan(body, off, out);
                    }
                }
                Node::Timeline(t) if super::span_contains(t.span, off) => {
                    for track in &t.tracks {
                        for clip in &track.clips {
                            if let lute_syntax::ast::ClipNode::Directive(d) = &clip.node {
                                if super::span_contains(d.span, off) {
                                    out.extend(d.attrs.iter().map(|a| a.key.clone()));
                                }
                            }
                        }
                    }
                }
                Node::On(o) if super::span_contains(o.span, off) => scan(&o.body, off, out),
                Node::Objective(ob) if super::span_contains(ob.span, off) => {
                    scan(&ob.body, off, out)
                }
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    for shot in &doc.shots {
        scan(&shot.body, off, &mut out);
    }
    for quest in &doc.quests {
        scan(&quest.body, off, &mut out);
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
        let items = complete_at(
            &doc,
            &load_core_snapshot(),
            &ProviderSet::default(),
            &SchemaImports::default(),
            off,
        );
        assert!(items.iter().any(|i| i.label == "camera"));
        assert!(items.iter().any(|i| i.label == "bg"));
    }

    #[test]
    fn completion_of_attr_keys_inside_directive() {
        let text = "## Shot 1.\n::camera{}\n";
        let doc = parsed(text);
        let off = text.find("{}").unwrap() + 1; // between the braces
        let items = complete_at(
            &doc,
            &load_core_snapshot(),
            &ProviderSet::default(),
            &SchemaImports::default(),
            off,
        );
        let ls = labels(&items);
        assert!(ls.contains(&"focus"), "has focus: {ls:?}");
        assert!(ls.contains(&"zoom"), "has zoom: {ls:?}");
    }

    #[test]
    fn attr_key_completion_dedupes_present_keys() {
        let text = "## Shot 1.\n::camera{focus=\"b\" }\n";
        let doc = parsed(text);
        // Cursor in the whitespace after the first attr (still the attr area).
        let off = text.find("\" }").unwrap() + 2;
        let items = complete_at(
            &doc,
            &load_core_snapshot(),
            &ProviderSet::default(),
            &SchemaImports::default(),
            off,
        );
        let ls = labels(&items);
        assert!(
            !ls.contains(&"focus"),
            "focus already present, should be gone: {ls:?}"
        );
        assert!(ls.contains(&"zoom"), "zoom still offered: {ls:?}");
    }

    #[test]
    fn completion_of_enum_values_at_enum_attr() {
        let text = "## Shot 1.\n::auto{character=\"b\" anchor=\"\"}\n";
        let doc = parsed(text);
        // Cursor inside the empty `anchor=""` value.
        let off = text.find("anchor=\"").unwrap() + "anchor=\"".len();
        let items = complete_at(
            &doc,
            &load_core_snapshot(),
            &ProviderSet::default(),
            &SchemaImports::default(),
            off,
        );
        let ls = labels(&items);
        assert!(
            ls.contains(&"left") && ls.contains(&"center") && ls.contains(&"right"),
            "anchor enum members: {ls:?}"
        );
    }

    #[test]
    fn completion_of_def_names_after_at() {
        let text = "---\nkind: scene\ncharacter: bianca\nseason: 1\nepisode: 2\ndefs:\n  fond: { type: bool, cel: \"scene.x >= 1\" }\n---\n## Shot 1.\n::set{scene.y = @}\n";
        let doc = parsed(text);
        let off = text.find("= @").unwrap() + 3; // just past `@`
        let items = complete_at(
            &doc,
            &load_core_snapshot(),
            &ProviderSet::default(),
            &SchemaImports::default(),
            off,
        );
        assert!(
            items.iter().any(|i| i.label == "fond"),
            "offers def name: {:?}",
            labels(&items)
        );
    }

    #[test]
    fn completion_of_choice_ids_in_match_subject() {
        let text = "## Shot 1.\n<branch id=\"number\">\n  <choice id=\"a\" label=\"A\">\n    @f: a.\n  </choice>\n</branch>\n<match on=\"\">\n  <otherwise>\n    @f: x.\n  </otherwise>\n</match>\n";
        let doc = parsed(text);
        let off = text.find("on=\"").unwrap() + "on=\"".len(); // inside the empty subject
        let items = complete_at(
            &doc,
            &load_core_snapshot(),
            &ProviderSet::default(),
            &SchemaImports::default(),
            off,
        );
        assert!(
            items.iter().any(|i| i.label == "scene.choices.number"),
            "offers the choice path: {:?}",
            labels(&items)
        );
    }

    /// D2: `<match on="">` subject completion must offer a `<branch id="inner">`
    /// that is nested inside a `<hub>` choice body (`collect_branch_ids` must
    /// descend into hub choices).
    #[test]
    fn completion_of_choice_ids_offers_hub_nested_branch() {
        let text = "## Shot 1.\n<hub id=\"chat\">\n<choice id=\"ask\" label=\"Ask\" once>\n<branch id=\"inner\">\n<choice id=\"a\" label=\"A\">\n@f: a.\n</choice>\n</branch>\n</choice>\n<choice id=\"leave\" label=\"Leave\" exit>\n@f: bye.\n</choice>\n</hub>\n<match on=\"\">\n<otherwise>\n@f: x.\n</otherwise>\n</match>\n";
        let doc = parsed(text);
        let off = text.find("on=\"").unwrap() + "on=\"".len(); // inside the empty subject
        let items = complete_at(
            &doc,
            &load_core_snapshot(),
            &ProviderSet::default(),
            &SchemaImports::default(),
            off,
        );
        assert!(
            items.iter().any(|i| i.label == "scene.choices.inner"),
            "offers the hub-nested branch choice path: {:?}",
            labels(&items)
        );
    }

    /// D2: a `<hub>` folds an implicit `scene.choices.<hubId>` enum (same shape
    /// as a `<branch>`), so `<match on="">` subject completion must offer the
    /// hub's own id, not just ids nested inside its choice bodies.
    #[test]
    fn completion_of_choice_ids_offers_hub_own_id() {
        let text = "## Shot 1.\n<hub id=\"chatWithBianca\">\n<choice id=\"ask\" label=\"Ask\" once>\n@f: a.\n</choice>\n<choice id=\"leave\" label=\"Leave\" exit>\n@f: bye.\n</choice>\n</hub>\n<match on=\"\">\n<otherwise>\n@f: x.\n</otherwise>\n</match>\n";
        let doc = parsed(text);
        let off = text.find("on=\"").unwrap() + "on=\"".len(); // inside the empty subject
        let items = complete_at(
            &doc,
            &load_core_snapshot(),
            &ProviderSet::default(),
            &SchemaImports::default(),
            off,
        );
        assert!(
            items.iter().any(|i| i.label == "scene.choices.chatWithBianca"),
            "offers the hub's own choice path: {:?}",
            labels(&items)
        );
    }

    /// D2: attr-key completion for a directive nested inside a `<hub>` choice
    /// body must dedupe an already-written key (`present_attr_keys` must descend
    /// into hub choices).
    #[test]
    fn attr_key_completion_dedupes_present_keys_in_hub_choice() {
        let text = "## Shot 1.\n<hub id=\"chat\">\n<choice id=\"ask\" label=\"Ask\" once>\n::camera{focus=\"b\" }\n</choice>\n<choice id=\"leave\" label=\"Leave\" exit>\n@f: bye.\n</choice>\n</hub>\n";
        let doc = parsed(text);
        // Cursor in the whitespace after the first attr (still the attr area).
        let off = text.find("\" }").unwrap() + 2;
        let items = complete_at(
            &doc,
            &load_core_snapshot(),
            &ProviderSet::default(),
            &SchemaImports::default(),
            off,
        );
        let ls = labels(&items);
        assert!(
            !ls.contains(&"focus"),
            "focus already present in hub-nested directive, should be gone: {ls:?}"
        );
        assert!(ls.contains(&"zoom"), "zoom still offered: {ls:?}");
    }

    #[test]
    fn completion_of_state_paths_in_set_expr() {
        let text = "---\nkind: scene\ncharacter: bianca\nseason: 1\nepisode: 2\nstate:\n  scene.affect.bianca: { type: number, default: 0 }\n---\n## Shot 1.\n::set{scene.affect.bianca = }\n";
        let doc = parsed(text);
        // Cursor after the `=` (expr slot) — state paths are offered.
        let off = text.rfind("= }").unwrap() + 2;
        let items = complete_at(
            &doc,
            &load_core_snapshot(),
            &ProviderSet::default(),
            &SchemaImports::default(),
            off,
        );
        assert!(
            items.iter().any(|i| i.label == "scene.affect.bianca"),
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
    fn completion_offers_emotion_enum() {
        // Cursor after the 3rd `.` → segment idx 3 (emotion enum).
        let text = "## Shot 1.\n::portrait{assetId=\"CH.bianca.waitress.\"}\n";
        let doc = parsed(text);
        let off = text.find("waitress.").unwrap() + "waitress.".len();
        let items = complete_at(
            &doc,
            &asset_snapshot(),
            &ProviderSet::default(),
            &SchemaImports::default(),
            off,
        );
        let ls = labels(&items);
        assert!(
            ls.contains(&"delighted") && ls.contains(&"content") && ls.contains(&"neutral"),
            "emotion enum members: {ls:?}"
        );
    }

    #[test]
    fn completion_offers_const_prefix() {
        // Cursor within the prefix segment (idx 0) → the const `CH`.
        let text = "## Shot 1.\n::portrait{assetId=\"CH.bianca.waitress.delighted.3\"}\n";
        let doc = parsed(text);
        let off = text.find("CH.bianca").unwrap() + 1; // on the `H` of `CH`
        let items = complete_at(
            &doc,
            &asset_snapshot(),
            &ProviderSet::default(),
            &SchemaImports::default(),
            off,
        );
        let ls = labels(&items);
        assert!(ls.contains(&"CH"), "const prefix offered: {ls:?}");
    }

    #[test]
    fn completion_offers_provider_ids() {
        use lute_manifest::provider::ProviderSnapshot;
        use std::collections::BTreeMap;
        // Cursor within the `characterId` segment (idx 1), typed
        // `providerRef("character")`. The pinned snapshot lists two ids.
        let text = "## Shot 1.\n::portrait{assetId=\"CH.bianca.waitress.delighted.3\"}\n";
        let doc = parsed(text);
        let off = text.find("bianca").unwrap() + 2; // inside `bianca`, segment idx 1
        let providers = ProviderSet::from_one(ProviderSnapshot {
            manifest_version: "1".to_string(),
            provider_version: "1".to_string(),
            entries: BTreeMap::from([(
                "character".to_string(),
                vec!["bianca".to_string(), "ren".to_string()],
            )]),
            stale: false,
        });
        let items = complete_at(
            &doc,
            &asset_snapshot(),
            &providers,
            &SchemaImports::default(),
            off,
        );
        let ls = labels(&items);
        assert!(
            ls.contains(&"bianca") && ls.contains(&"ren"),
            "providerRef segment offers pinned ids: {ls:?}"
        );
        // An empty ProviderSet offers nothing for that segment (honest §6.9).
        let empty = complete_at(
            &doc,
            &asset_snapshot(),
            &ProviderSet::default(),
            &SchemaImports::default(),
            off,
        );
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
    fn completion_offers_imported_state_path() {
        // `run.gold` is only imported via `uses:`, not declared inline.
        let text =
            "---\nkind: scene\ncharacter: bianca\nseason: 1\nepisode: 2\n---\n## Shot 1.\n::set{run.gold = }\n";
        let doc = parsed(text);
        let off = text.rfind("= }").unwrap() + 2;
        let items = complete_at(
            &doc,
            &load_core_snapshot(),
            &ProviderSet::default(),
            &schema_imports(),
            off,
        );
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
            "---\nkind: scene\ncharacter: bianca\nseason: 1\nepisode: 2\n---\n## Shot 1.\n::set{scene.y = @}\n";
        let doc = parsed(text);
        let off = text.find("= @").unwrap() + 3;
        let items = complete_at(
            &doc,
            &load_core_snapshot(),
            &ProviderSet::default(),
            &schema_imports(),
            off,
        );
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
        let text = "---\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  scene.serve.debut.rank: { type: { enum: [gold, silver, bronze] } }\n---\n## Shot 1.\n<match on=\"scene.serve.debut.rank\">\n<when is=\"gold\">\n@fixer: nice.\n</when>\n<otherwise>\n@fixer: ok.\n</otherwise>\n</match>\n";
        let doc = parsed(text);
        let off = text.find("is=\"gold\"").unwrap() + "is=\"".len() + 1; // inside "gold"
        let items = complete_at(
            &doc,
            &load_core_snapshot(),
            &ProviderSet::default(),
            &SchemaImports::default(),
            off,
        );
        let ls = labels(&items);
        assert!(
            ls.contains(&"gold")
                && ls.contains(&"silver")
                && ls.contains(&"bronze")
                && ls.contains(&"unset"),
            "offers enum members ∪ unset: {ls:?}"
        );
    }

    /// D3: `<match on="scene.choices.chat">` over a top-level `<hub id="chat">`
    /// (choices askCoffee/leave) — `is=` completion offers the hub's choice ids ∪
    /// `unset` (the implicit recording enum, dsl §11.1.3).
    #[test]
    fn completion_in_when_is_offers_hub_choice_ids() {
        let text = "## Shot 1.\n<hub id=\"chat\">\n<choice id=\"askCoffee\" label=\"Coffee?\" once>\n@f: a.\n</choice>\n<choice id=\"leave\" label=\"Bye\" exit>\n@f: bye.\n</choice>\n</hub>\n<match on=\"scene.choices.chat\">\n<when is=\"askCoffee\">\n@f: x.\n</when>\n<otherwise>\n@f: y.\n</otherwise>\n</match>\n";
        let doc = parsed(text);
        let off = text.find("is=\"askCoffee\"").unwrap() + "is=\"".len() + 1;
        let items = complete_at(
            &doc,
            &load_core_snapshot(),
            &ProviderSet::default(),
            &SchemaImports::default(),
            off,
        );
        let ls = labels(&items);
        assert!(
            ls.contains(&"askCoffee") && ls.contains(&"leave") && ls.contains(&"unset"),
            "offers hub choice ids ∪ unset: {ls:?}"
        );
    }

    /// D3: `<match on="scene.visited.chat.askCoffee">` over a `<hub id="chat">`
    /// with a `<choice id="askCoffee">` — the folded per-choice bool (dsl §9.6,
    /// §11.1.3), so `is=` completion offers true/false/unset.
    #[test]
    fn completion_in_when_is_offers_visited_bool() {
        let text = "## Shot 1.\n<hub id=\"chat\">\n<choice id=\"askCoffee\" label=\"Coffee?\" once>\n@f: a.\n</choice>\n<choice id=\"leave\" label=\"Bye\" exit>\n@f: bye.\n</choice>\n</hub>\n<match on=\"scene.visited.chat.askCoffee\">\n<when is=\"true\">\n@f: x.\n</when>\n<otherwise>\n@f: y.\n</otherwise>\n</match>\n";
        let doc = parsed(text);
        let off = text.find("is=\"true\"").unwrap() + "is=\"".len() + 1;
        let items = complete_at(
            &doc,
            &load_core_snapshot(),
            &ProviderSet::default(),
            &SchemaImports::default(),
            off,
        );
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
        let text = "---\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  scene.serve.debut.rank: { type: { enum: [gold, silver, bronze] } }\ndefs:\n  warm: { type: bool, cel: \"true\" }\n---\n## Shot 1.\n<match on=\"scene.serve.debut.rank\">\n<when test=\"@warm\">\n@fixer: nice.\n</when>\n<otherwise>\n@fixer: ok.\n</otherwise>\n</match>\n";
        let doc = parsed(text);
        let off = text.find("@warm").unwrap() + 1; // just past `@`
        let items = complete_at(
            &doc,
            &load_core_snapshot(),
            &ProviderSet::default(),
            &SchemaImports::default(),
            off,
        );
        let ls = labels(&items);
        assert!(ls.contains(&"warm"), "test= still CEL: offers def name: {ls:?}");
        assert!(
            !ls.contains(&"unset") && !ls.contains(&"gold"),
            "test= must NOT offer the is= literal domain: {ls:?}"
        );
    }

    /// D3 fix (non-CEL subject): a hyphenated `<match on="scene.choices.pick-one">`
    /// subject is NOT a pure CEL path — cel-parser reads `pick-one` as subtraction,
    /// so the checker's `subject_path` reconstruction (parse + `select_path`)
    /// yields `None` (an INFINITE subject, no `is=` menu). Even with a `<branch
    /// id="pick-one">` literally present, `is=` completion must offer NOTHING,
    /// matching the checker (the pre-fix `is_path_byte` scan wrongly offered its
    /// choices because `-` is a path byte).
    #[test]
    fn completion_in_when_is_rejects_non_cel_subject() {
        let text = "## Shot 1.\n<branch id=\"pick-one\">\n<choice id=\"a\" label=\"A\">\n@f: a.\n</choice>\n<choice id=\"b\" label=\"B\">\n@f: b.\n</choice>\n</branch>\n<match on=\"scene.choices.pick-one\">\n<when is=\"a\">\n@f: x.\n</when>\n<otherwise>\n@f: y.\n</otherwise>\n</match>\n";
        let doc = parsed(text);
        let off = text.find("is=\"a\"").unwrap() + "is=\"".len() + 1;
        let items = complete_at(
            &doc,
            &load_core_snapshot(),
            &ProviderSet::default(),
            &SchemaImports::default(),
            off,
        );
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
        let text = "## Shot 1.\n<branch id=\"dup\">\n<choice id=\"first1\" label=\"F1\">\n@f: a.\n</choice>\n<choice id=\"first2\" label=\"F2\">\n@f: b.\n</choice>\n</branch>\n<branch id=\"dup\">\n<choice id=\"last1\" label=\"L1\">\n@f: c.\n</choice>\n<choice id=\"last2\" label=\"L2\">\n@f: d.\n</choice>\n</branch>\n<match on=\"scene.choices.dup\">\n<when is=\"last1\">\n@f: x.\n</when>\n<otherwise>\n@f: y.\n</otherwise>\n</match>\n";
        let doc = parsed(text);
        let off = text.find("is=\"last1\"").unwrap() + "is=\"".len() + 1;
        let items = complete_at(
            &doc,
            &load_core_snapshot(),
            &ProviderSet::default(),
            &SchemaImports::default(),
            off,
        );
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
        complete_at(
            &doc,
            &load_core_snapshot(),
            &ProviderSet::default(),
            &SchemaImports::default(),
            off,
        )
    }

    #[test]
    fn on_event_value_completion_lists_builtin_lifecycle_events() {
        let text = "---\nkind: quest\n---\n<quest id=\"q\">\n<on event=\"quest\">\n</on>\n</quest>\n";
        let off = text.find("\"quest\"").unwrap() + 1;
        let items = complete(text, off);
        let ls = labels(&items);
        assert!(ls.contains(&"questComplete"), "{ls:?}");
        assert!(ls.contains(&"questActive"), "{ls:?}");
        assert!(ls.contains(&"questFailed"), "{ls:?}");
    }

    #[test]
    fn objective_attr_area_completion_lists_done_when_optional() {
        let text = "---\nkind: quest\n---\n<quest id=\"q\">\n<objective id=\"o\" done=\"a\">\n</objective>\n</quest>\n";
        let off = text.find("<objective ").unwrap() + "<objective ".len();
        let items = complete(text, off);
        let ls = labels(&items);
        for k in ["id", "done", "when", "title", "optional"] {
            assert!(ls.contains(&k), "missing {k}: {ls:?}");
        }
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
        let text = "---\nkind: quest\n---\n<quest id=\"q\">\n<objective id=\"o\" done=\"a\"/>\n</quest>\n";
        let off = text.find("<quest ").unwrap() + "<quest ".len();
        let items = complete(text, off);
        let ls = labels(&items);
        for k in ["id", "title", "start", "fail"] {
            assert!(ls.contains(&k), "missing {k}: {ls:?}");
        }
    }

    #[test]
    fn kind_frontmatter_value_completion_lists_scene_and_quest() {
        let text = "---\nkind: \n---\n";
        let off = text.find("kind: ").unwrap() + "kind: ".len();
        let items = complete(text, off);
        let ls = labels(&items);
        assert!(ls.contains(&"scene"), "{ls:?}");
        assert!(ls.contains(&"quest"), "{ls:?}");
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
        assert!(!ls.contains(&"delivery"), "0.2.1 delivery key retired: {ls:?}");
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
    fn content_line_other_attr_key_has_no_value_completion() {
        // No content-line key carries a closed value domain in 0.2.2 (the
        // 0.2.1 `delivery` enum is retired in favor of bare flags, §D7).
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
                vec!["bianca".to_string(), "ren".to_string()],
            )]),
            stale: false,
        });
        let items = complete_at(
            &doc,
            &load_core_snapshot(),
            &providers,
            &SchemaImports::default(),
            off,
        );
        let ls = labels(&items);
        assert!(
            ls.contains(&"bianca") && ls.contains(&"ren") && ls.contains(&"narrator"),
            "catalog ids + narrator: {ls:?}"
        );
    }
}
