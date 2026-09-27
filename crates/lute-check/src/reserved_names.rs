//! Reserved names refused where they are declared (dsl 0.28.0 §1): the
//! frontmatter/schema declarations ([`check_frontmatter`]) and a document's
//! ids ([`check_document_ids`]), each asking the one table,
//! [`lute_manifest::reserved`]. Relations are refused by
//! [`crate::rel_schema::validate_rel_decls`] and plugin exports by the
//! plugin assembly, from the same table.
//!
//! A refused name is reported once, at its declaration. The state paths
//! whose meaning it breaks — the bare root a member or def named `clock`
//! reads as, a row whose enum holds `unset` — go into
//! [`crate::meta::StateSchema::faulty`], so their reads are not judged a
//! second time.

use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_manifest::reserved::{refusal, Slot};
use lute_syntax::ast::{Arm, AttrValue, Document, Meta, Node};
use serde_yaml::Value;

use crate::meta::{meta_path_span, TypedMeta};

pub const E_RESERVED_NAME: &str = "E-RESERVED-NAME";

fn diag(message: String, span: Span) -> Diagnostic {
    Diagnostic {
        code: E_RESERVED_NAME.to_string(),
        severity: Severity::Error,
        message,
        span,
        layer: Layer::Content,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    }
}

/// The first of `slots` refusing `name`, as an `E-RESERVED-NAME` at `span`;
/// `what` names the declaration ("a def", "a `<choice>` id").
pub(crate) fn refuse(slots: &[Slot], name: &str, what: &str, span: Span) -> Option<Diagnostic> {
    slots
        .iter()
        .find_map(|slot| refusal(*slot, name))
        .map(|r| diag(r.message(what), span))
}

/// A member list item that is not a name: a YAML number, boolean or null
/// (`[1, 2, 3]`, `[lit, true]`). `None` for a string (a name, judged by
/// the table) or a non-scalar (a shape error reported elsewhere).
fn scalar_member(item: &Value) -> Option<(String, bool)> {
    match item {
        Value::Bool(b) => Some((b.to_string(), false)),
        Value::Null => Some(("null".to_string(), false)),
        Value::Number(n) => Some((n.to_string(), true)),
        _ => None,
    }
}

/// `E-RESERVED-NAME` for one member-list item, or `None` when it is fine.
fn member_diag(
    meta: &Meta,
    key: &[&str],
    item: &Value,
    slot: Slot,
    what: &str,
) -> Option<Diagnostic> {
    match item {
        Value::String(name) => refuse(&[slot], name, what, member_span(meta, key, name)),
        other => {
            let (written, number) = scalar_member(other)?;
            let span = member_span(meta, key, &written);
            if number {
                let instead: String = written
                    .chars()
                    .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                    .collect();
                Some(diag(
                    format!(
                        "`{written}` cannot name {what} because it is a number, which a condition reads \
                         as the number, never as a name — rename it (e.g. `n{instead}`)"
                    ),
                    span,
                ))
            } else {
                refuse(&[slot], &written, what, span)
            }
        }
    }
}

/// The span of member `written` in the list under the frontmatter key
/// `key`: its first word-bounded occurrence after the key, bare or quoted;
/// the key itself when the text does not show it.
fn member_span(meta: &Meta, key: &[&str], written: &str) -> Span {
    let anchor = meta_path_span(meta, key);
    let authored = crate::chapters::authored_yaml(&meta.raw_yaml);
    let Some(range) = lute_manifest::yaml_text::key_span(authored, key) else {
        return anchor;
    };
    let base = anchor.byte_start.saturating_sub(range.start);
    let rest = &authored[range.end..];
    let bytes = rest.as_bytes();
    let boundary = |b: Option<&u8>| {
        b.map_or(true, |b| {
            !(b.is_ascii_alphanumeric() || *b == b'_' || *b == b'.' || *b == b'-')
        })
    };
    let mut from = 0;
    while let Some(i) = rest[from..].find(written) {
        let at = from + i;
        let before = at.checked_sub(1).and_then(|p| bytes.get(p));
        let after = bytes.get(at + written.len());
        if boundary(before) && boundary(after) {
            let start = base + range.end + at;
            return Span {
                byte_start: start,
                byte_end: start + written.len(),
                line: 0,
                column: 0,
                utf16_range: (0, 0),
            };
        }
        from = at + written.len();
    }
    anchor
}

/// The items of a member list: `members:`/`add:` of a mapping, or a flat
/// sequence, with the frontmatter key path they sit under.
fn member_lists<'v>(name: &'v str, body: &'v Value) -> Vec<(Vec<&'v str>, &'v [Value])> {
    match body {
        Value::Sequence(items) => vec![(vec![name], items.as_slice())],
        Value::Mapping(m) => ["members", "add"]
            .into_iter()
            .filter_map(|k| {
                m.get(k)
                    .and_then(Value::as_sequence)
                    .map(|items| (vec![name, k], items.as_slice()))
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// Every reserved name a frontmatter (a schema file's or a document's)
/// declares: entity kind members, enum members (an `enums:` entry or a
/// state row's `{ enum: [...] }`), defs, seasons, `cast:` ids, and state
/// path segments. The state paths a refused name breaks go into
/// `typed.state.faulty`.
pub(crate) fn check_frontmatter(
    meta: &Meta,
    map: &serde_yaml::Mapping,
    typed: &mut TypedMeta,
) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    let get = |key: &str| map.get(Value::String(key.to_string()));
    // Kinds and enums whose members were refused: the state rows typed by
    // them are faulty.
    let mut broken_domains: Vec<String> = Vec::new();

    for (block, slot, what) in [
        ("entities", Slot::EntityMember, "entity kind"),
        ("enums", Slot::EnumMember, "enum"),
    ] {
        let Some(Value::Mapping(entries)) = get(block) else {
            continue;
        };
        for (name, body) in entries {
            let Some(name) = name.as_str() else { continue };
            for (sub, items) in member_lists(name, body) {
                let mut key = vec![block];
                key.extend(sub);
                for item in items {
                    let what = format!("a member of {what} `{name}`");
                    if let Some(d) = member_diag(meta, &key, item, slot, &what) {
                        diags.push(d);
                        broken_domains.push(name.to_string());
                        // A member named like a state root is read as that
                        // root wherever it is written bare.
                        if let Some(root) = item
                            .as_str()
                            .filter(|s| lute_manifest::reserved::STATE_ROOTS.contains(s))
                        {
                            typed.state.faulty.insert(root.to_string());
                        }
                    }
                }
            }
        }
    }

    if let Some(Value::Mapping(defs)) = get("defs") {
        for name in defs.keys().filter_map(Value::as_str) {
            let span = meta_path_span(meta, &["defs", name]);
            if let Some(d) = refuse(&[Slot::Def], name, "a def", span) {
                diags.push(d);
                typed.state.faulty.insert(name.to_string());
            }
        }
    }
    if let Some(Value::Mapping(seasons)) = get("seasons") {
        for name in seasons.keys().filter_map(Value::as_str) {
            let span = meta_path_span(meta, &["seasons", name]);
            diags.extend(refuse(&[Slot::Season], name, "a season", span));
        }
    }
    if let Some(Value::Mapping(cast)) = get("cast") {
        for id in cast.keys().filter_map(Value::as_str) {
            let span = meta_path_span(meta, &["cast", id]);
            diags.extend(refuse(&[Slot::Cast], id, "a cast id", span));
        }
    }

    if let Some(Value::Mapping(rows)) = get("state") {
        for (path, row) in rows {
            let Some(path) = path.as_str() else { continue };
            let span = meta_path_span(meta, &["state", path]);
            for seg in path.split('.').skip(1) {
                let what = format!("a segment of state path `{path}`");
                if let Some(d) = refuse(&[Slot::PathSegment], seg, &what, span) {
                    diags.push(d);
                    typed.state.faulty.insert(path.to_string());
                    break;
                }
            }
            // A row's inline `{ enum: [...] }`: its string members (a
            // non-string one fails the row's type, `E-STATE-DECL`).
            let inline = row
                .get("type")
                .and_then(|t| t.get("enum"))
                .and_then(Value::as_sequence);
            for member in inline.into_iter().flatten().filter_map(Value::as_str) {
                let what = format!("a member of `{path}`'s enum");
                let at = member_span(meta, &["state", path], member);
                if let Some(d) = refuse(&[Slot::EnumMember], member, &what, at) {
                    diags.push(d);
                    typed.state.faulty.insert(path.to_string());
                }
            }
            // A row typed by a kind or enum a refused member broke.
            let typed_by = row
                .get("type")
                .and_then(|t| t.get("domain").or_else(|| t.get("entity")))
                .and_then(Value::as_str);
            if typed_by.is_some_and(|d| broken_domains.iter().any(|b| b == d)) {
                typed.state.faulty.insert(path.to_string());
            }
        }
    }
    diags
}

/// Every reserved id a document declares: its scene id, quest, objective,
/// entry and bundle-beat ids, and each `<branch>`/`<hub>`/`<choice>` id.
/// `text` is the document's source, for the span of an `id="…"` value.
pub(crate) fn check_document_ids(doc: &Document, typed: &TypedMeta, text: &str) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    if let Some(id) = &typed.id {
        let span = crate::meta::meta_key_span(&doc.meta, "id");
        diags.extend(refuse(&[Slot::Id], id, "a scene", span));
    }
    for q in &doc.quests {
        diags.extend(refuse(&[Slot::PathSegment], &q.id, "a quest", q.id_span));
        walk(&q.body, text, &mut diags);
    }
    for e in &doc.entries {
        diags.extend(refuse(
            &[Slot::Id, Slot::PathSegment],
            &e.id,
            "an entry",
            e.id_span,
        ));
        walk(&e.body, text, &mut diags);
    }
    for b in &doc.beats {
        diags.extend(refuse(&[Slot::Id], &b.id, "a beat", b.id_span));
        walk(&b.body, text, &mut diags);
    }
    for shot in &doc.shots {
        walk(&shot.body, text, &mut diags);
    }
    diags
}

/// The `id="…"` value inside `span` of `text`, else `span`.
fn id_span(text: &str, span: Span, id: &str) -> Span {
    let Some(src) = text.get(span.byte_start..span.byte_end) else {
        return span;
    };
    let at = [format!("id=\"{id}\""), format!("id='{id}'")]
        .iter()
        .find_map(|needle| src.find(needle.as_str()));
    match at {
        Some(i) => {
            let start = span.byte_start + i + "id=\"".len();
            Span {
                byte_start: start,
                byte_end: start + id.len(),
                line: 0,
                column: 0,
                utf16_range: (0, 0),
            }
        }
        None => span,
    }
}

fn walk(nodes: &[Node], text: &str, diags: &mut Vec<Diagnostic>) {
    for node in nodes {
        match node {
            Node::Branch(b) => {
                let at = id_span(text, b.span, &b.id);
                diags.extend(refuse(&[Slot::PathSegment], &b.id, "a `<branch>`", at));
                for c in &b.choices {
                    let at = id_span(text, c.span, &c.id);
                    diags.extend(refuse(&[Slot::Id], &c.id, "a `<choice>`", at));
                    walk(&c.body, text, diags);
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
                    let at = id_span(text, h.span, id);
                    diags.extend(refuse(&[Slot::PathSegment], id, "a `<hub>`", at));
                }
                // A hub choice is also a segment: `scene.visited.<hub>.<choice>`.
                for c in &h.choices {
                    let at = id_span(text, c.span, &c.id);
                    diags.extend(refuse(
                        &[Slot::Id, Slot::PathSegment],
                        &c.id,
                        "a `<choice>`",
                        at,
                    ));
                }
                for body in h.bodies() {
                    walk(body, text, diags);
                }
            }
            Node::Match(m) => {
                for arm in &m.arms {
                    match arm {
                        Arm::When { body, .. } | Arm::Otherwise { body, .. } => {
                            walk(body, text, diags)
                        }
                    }
                }
            }
            Node::On(on) => walk(&on.body, text, diags),
            Node::Objective(o) => {
                diags.extend(refuse(
                    &[Slot::PathSegment],
                    &o.id,
                    "an objective",
                    o.id_span,
                ));
            }
            _ => {}
        }
    }
}
