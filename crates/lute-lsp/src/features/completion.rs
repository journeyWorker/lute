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

use lute_check::parse_meta;
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_syntax::ast::{Arm, Document, Node};
use tower_lsp_server::ls_types::{CompletionItem, CompletionItemKind};

use super::{attr_enum_values, type_label, Cursor};

/// Completion candidates at byte offset `off`. Empty when the cursor is somewhere
/// with nothing to offer.
pub fn complete_at(
    doc: &Document,
    snapshot: &CapabilitySnapshot,
    off: usize,
) -> Vec<CompletionItem> {
    let (meta, _) = parse_meta(&doc.meta, snapshot);
    let Some(cursor) = super::resolve(doc, off) else { return Vec::new() };
    match cursor {
        Cursor::DirectiveName(_) => directive_items(snapshot),
        Cursor::DirectiveAttrArea { directive } => attr_key_items(snapshot, directive, doc, off),
        Cursor::AttrKey { directive: Some(dir), .. } => attr_key_items(snapshot, dir, doc, off),
        Cursor::AttrValue { directive: Some(dir), key } => enum_value_items(snapshot, dir, key),
        Cursor::Cel { slot, in_match_subject } => {
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
        Cursor::AttrKey { directive: None, .. } | Cursor::AttrValue { directive: None, .. } => {
            Vec::new()
        }
        Cursor::SetPath { .. } => state_path_items(&meta),
    }
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
    let Some(decl) = snapshot.directive(directive) else { return Vec::new() };
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

/// The enum members of an enum-typed attribute value, kind `ENUM_MEMBER`. Empty
/// for a non-enum attr.
fn enum_value_items(
    snapshot: &CapabilitySnapshot,
    directive: &str,
    key: &str,
) -> Vec<CompletionItem> {
    attr_enum_values(snapshot, directive, key)
        .into_iter()
        .flatten()
        .map(|v| CompletionItem {
            label: v,
            kind: Some(CompletionItemKind::ENUM_MEMBER),
            ..Default::default()
        })
        .collect()
}

/// Author `defs:` + snapshot def names for an `@ref` position, kind `VARIABLE`.
fn def_items(
    meta: &lute_check::TypedMeta,
    snapshot: &CapabilitySnapshot,
) -> Vec<CompletionItem> {
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
    ids.into_iter()
        .map(|id| CompletionItem {
            label: format!("scene.choices.{id}"),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: Some(format!("choice of <branch id=\"{id}\">")),
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
            Node::Match(m) => {
                for arm in &m.arms {
                    let body = match arm {
                        Arm::When { body, .. } | Arm::Otherwise { body, .. } => body,
                    };
                    collect_branch_ids(body, out);
                }
            }
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
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    for shot in &doc.shots {
        scan(&shot.body, off, &mut out);
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
        let items = complete_at(&doc, &load_core_snapshot(), off);
        assert!(items.iter().any(|i| i.label == "camera"));
        assert!(items.iter().any(|i| i.label == "bg"));
    }

    #[test]
    fn completion_of_attr_keys_inside_directive() {
        let text = "## Shot 1.\n::camera{}\n";
        let doc = parsed(text);
        let off = text.find("{}").unwrap() + 1; // between the braces
        let items = complete_at(&doc, &load_core_snapshot(), off);
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
        let items = complete_at(&doc, &load_core_snapshot(), off);
        let ls = labels(&items);
        assert!(!ls.contains(&"focus"), "focus already present, should be gone: {ls:?}");
        assert!(ls.contains(&"zoom"), "zoom still offered: {ls:?}");
    }

    #[test]
    fn completion_of_enum_values_at_enum_attr() {
        let text = "## Shot 1.\n::auto{character=\"b\" anchor=\"\"}\n";
        let doc = parsed(text);
        // Cursor inside the empty `anchor=""` value.
        let off = text.find("anchor=\"").unwrap() + "anchor=\"".len();
        let items = complete_at(&doc, &load_core_snapshot(), off);
        let ls = labels(&items);
        assert!(ls.contains(&"left") && ls.contains(&"center") && ls.contains(&"right"),
            "anchor enum members: {ls:?}");
    }

    #[test]
    fn completion_of_def_names_after_at() {
        let text = "---\ncharacter: bianca\nseason: 1\nepisode: 2\ndefs:\n  fond: { type: bool, cel: \"scene.x >= 1\" }\n---\n## Shot 1.\n::set{scene.y = @}\n";
        let doc = parsed(text);
        let off = text.find("= @").unwrap() + 3; // just past `@`
        let items = complete_at(&doc, &load_core_snapshot(), off);
        assert!(items.iter().any(|i| i.label == "fond"), "offers def name: {:?}", labels(&items));
    }

    #[test]
    fn completion_of_choice_ids_in_match_subject() {
        let text = "## Shot 1.\n<branch id=\"number\">\n  <choice id=\"a\" label=\"A\">\n    :line[f]: a.\n  </choice>\n</branch>\n<match on=\"\">\n  <otherwise>\n    :line[f]: x.\n  </otherwise>\n</match>\n";
        let doc = parsed(text);
        let off = text.find("on=\"").unwrap() + "on=\"".len(); // inside the empty subject
        let items = complete_at(&doc, &load_core_snapshot(), off);
        assert!(items.iter().any(|i| i.label == "scene.choices.number"),
            "offers the choice path: {:?}", labels(&items));
    }

    #[test]
    fn completion_of_state_paths_in_set_expr() {
        let text = "---\ncharacter: bianca\nseason: 1\nepisode: 2\nstate:\n  scene.affect.bianca: { type: number, default: 0 }\n---\n## Shot 1.\n::set{scene.affect.bianca = }\n";
        let doc = parsed(text);
        // Cursor after the `=` (expr slot) — state paths are offered.
        let off = text.rfind("= }").unwrap() + 2;
        let items = complete_at(&doc, &load_core_snapshot(), off);
        assert!(items.iter().any(|i| i.label == "scene.affect.bianca"),
            "offers declared state path: {:?}", labels(&items));
    }
}
