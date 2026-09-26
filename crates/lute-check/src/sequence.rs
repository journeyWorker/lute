//! dsl 0.27.0 §8: linear chapters. `lute.project.yaml` MAY declare
//! `sequence: { occasion: chapter, scenes: [prologue, counter, kitchen] }`;
//! every listed scene then answers `occasion` (`on:`), waits for the one
//! before it (`after: visited("<previous>")`) and takes a descending
//! `priority:` — unless its own frontmatter already writes that key.
//!
//! The derivation is a DESUGAR applied right after parsing
//! ([`apply_sequence`], beside `questTier`'s default): the derived keys are
//! appended to the scene's frontmatter below [`SEQUENCE_MARKER`], so every
//! pass, tool and the compiled artifact read one ordinary scene beat. A
//! derived key has no text of its own in the scene; [`crate::meta::meta_key_span`]
//! anchors it at the scene's `id:` (the entry that puts it in the chain).
//!
//! The manifest's shape is checked at load (`lute_manifest::project`,
//! `E-SEQUENCE`); [`check_project_sequence`] checks its ids against the
//! project's scenes.

use std::path::{Path, PathBuf};

use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_manifest::project::{MetaDefaults, Sequence, E_SEQUENCE};
use lute_syntax::ast::Document;

use crate::meta::DocKind;

/// The line that separates a scene's authored frontmatter from the keys
/// [`apply_sequence`] derived. A YAML comment, so the frontmatter still
/// parses as one mapping.
pub const SEQUENCE_MARKER: &str = "# lute: derived from lute.project.yaml `sequence:`\n";

/// The authored part of a frontmatter (`raw_yaml` up to [`SEQUENCE_MARKER`]).
pub fn authored_yaml(raw_yaml: &str) -> &str {
    raw_yaml
        .find(SEQUENCE_MARKER)
        .map_or(raw_yaml, |i| &raw_yaml[..i])
}

/// Append the keys `defaults`' `sequence:` derives for this scene (dsl
/// 0.27.0 §8) to its frontmatter. A key the scene writes itself wins. A
/// no-op for a document that is no scene, a scene the sequence does not
/// list, a frontmatter that does not parse, or one already applied —
/// idempotent, so every surface may call it on the documents it parses.
pub fn apply_sequence(doc: &mut Document, defaults: &MetaDefaults) {
    let Some(sequence) = defaults.sequence() else {
        return;
    };
    if doc.meta.raw_yaml.contains(SEQUENCE_MARKER)
        || crate::meta::resolve_doc_kind_with_defaults(&doc.meta, defaults).0
            != Some(DocKind::Scene)
    {
        return;
    }
    let Ok(serde_yaml::Value::Mapping(map)) =
        serde_yaml::from_str::<serde_yaml::Value>(&doc.meta.raw_yaml)
    else {
        return;
    };
    let Some(id) = map.get("id").and_then(serde_yaml::Value::as_str) else {
        return;
    };
    let Some(derived) = sequence.derived(id.trim()) else {
        return;
    };
    let mut lines = String::new();
    for (key, value) in derived {
        if map.contains_key(key) {
            continue;
        }
        let text = match value {
            // `visited("prev")`: single-quoted so the inner quotes survive.
            serde_yaml::Value::String(s) if key == "after" => format!("'{s}'"),
            serde_yaml::Value::String(s) => s,
            serde_yaml::Value::Number(n) => n.to_string(),
            _ => continue,
        };
        lines.push_str(&format!("{key}: {text}\n"));
    }
    if lines.is_empty() {
        return;
    }
    let raw = &mut doc.meta.raw_yaml;
    if !raw.is_empty() && !raw.ends_with('\n') {
        raw.push('\n');
    }
    raw.push_str(SEQUENCE_MARKER);
    raw.push_str(&lines);
}

fn diag(message: String, span: Span) -> Diagnostic {
    Diagnostic {
        code: E_SEQUENCE.to_string(),
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

/// The span of scene id `id` in the manifest's `scenes:` list: its first
/// word-bounded occurrence after the `sequence:` key (the whole key when
/// the text cannot be found).
fn id_span(manifest: &str, id: &str) -> Span {
    let from = manifest.find("sequence:").unwrap_or(0);
    let bounded = |i: usize| {
        let ok =
            |c: Option<char>| !c.is_some_and(|c| c.is_ascii_alphanumeric() || "_.-".contains(c));
        ok(manifest[..i].chars().next_back()) && ok(manifest[i + id.len()..].chars().next())
    };
    let at = manifest[from..]
        .match_indices(id)
        .map(|(i, _)| from + i)
        .find(|&i| bounded(i));
    let (start, len) = at.map_or((from, "sequence".len()), |i| (i, id.len()));
    Span {
        byte_start: start,
        byte_end: start + len,
        line: 0,
        column: 0,
        utf16_range: (0, 0),
    }
}

/// dsl 0.27.0 §8, project-wide: every id `sequence.scenes` lists names a
/// scene of the project (`E-SEQUENCE`, with a did-you-mean), and no listed
/// scene answers another occasion with its own `on:` — the chain would skip
/// it. Anchored in `lute.project.yaml` at the id. `docs` are the root's
/// documents, already desugared.
pub fn check_project_sequence(
    root: &Path,
    docs: &[(PathBuf, Document)],
) -> Vec<(PathBuf, Diagnostic)> {
    let Ok(Some(project)) = lute_manifest::project::load_project(root) else {
        return Vec::new();
    };
    let Some(sequence) = project.defaults.sequence() else {
        return Vec::new();
    };
    let manifest_path = root.join("lute.project.yaml");
    let manifest = std::fs::read_to_string(&manifest_path).unwrap_or_default();
    check_sequence(sequence, &manifest, docs)
        .into_iter()
        .map(|d| (manifest_path.clone(), d))
        .collect()
}

/// [`check_project_sequence`] over an already-loaded sequence and the
/// manifest text its spans point into.
pub fn check_sequence(
    sequence: &Sequence,
    manifest: &str,
    docs: &[(PathBuf, Document)],
) -> Vec<Diagnostic> {
    let scenes = crate::connectivity::scene_key_set(docs);
    let mut out = Vec::new();
    for id in &sequence.scenes {
        let span = id_span(manifest, id);
        let Some(homes) = scenes.get(id) else {
            let other = docs
                .iter()
                .any(|(_, d)| crate::connectivity::bundle_id(d).as_deref() == Some(id.as_str()));
            let hint = lute_manifest::suggest::nearest(id, scenes.keys().map(String::as_str), 2)
                .map(|n| format!(" — did you mean `{n}`?"))
                .unwrap_or_default();
            out.push(diag(
                if other {
                    format!(
                        "`sequence.scenes` lists `{id}`, but `{id}` is a quest or lore document, \
                         not a scene — a sequence chains scenes (dsl 0.27.0 §8)"
                    )
                } else {
                    format!(
                        "`sequence.scenes` lists `{id}`, but no scene in this project declares \
                         `id: {id}`{hint} (dsl 0.27.0 §8)"
                    )
                },
                span,
            ));
            continue;
        };
        for (path, _) in homes {
            let Some((_, doc)) = docs.iter().find(|(p, _)| p == path) else {
                continue;
            };
            let authored =
                serde_yaml::from_str::<serde_yaml::Value>(authored_yaml(&doc.meta.raw_yaml));
            let own_on = authored
                .ok()
                .and_then(|v| v.get("on")?.as_str().map(str::to_string));
            if let Some(on) = own_on.filter(|on| on.trim() != sequence.occasion) {
                out.push(diag(
                    format!(
                        "`sequence.scenes` lists `{id}`, but its frontmatter says `on: {on}`, so it \
                         never answers `{occasion}` and the chain stops before it — remove its `on:` \
                         or take it out of the sequence (dsl 0.27.0 §8)",
                        on = on.trim(),
                        occasion = sequence.occasion,
                    ),
                    span,
                ));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seq() -> MetaDefaults {
        MetaDefaults::default().with_sequence(Some(Sequence {
            occasion: "chapter".to_string(),
            scenes: vec!["prologue".into(), "counter".into(), "kitchen".into()],
        }))
    }

    fn scene(front: &str) -> Document {
        lute_syntax::parse(&format!(
            "---\nkind: scene\n{front}---\n\n## S\n\n@narrator: hi\n"
        ))
        .0
    }

    fn map(doc: &Document) -> serde_yaml::Mapping {
        serde_yaml::from_str(&doc.meta.raw_yaml).unwrap()
    }

    #[test]
    fn listed_scenes_derive_on_after_and_descending_priority() {
        let mut first = scene("id: prologue\n");
        apply_sequence(&mut first, &seq());
        let m = map(&first);
        assert_eq!(m.get("on").unwrap().as_str(), Some("chapter"));
        assert!(
            m.get("after").is_none(),
            "the first scene waits for nothing"
        );
        assert_eq!(m.get("priority").unwrap().as_i64(), Some(30));

        let mut third = scene("id: kitchen\n");
        apply_sequence(&mut third, &seq());
        let m = map(&third);
        assert_eq!(
            m.get("after").unwrap().as_str(),
            Some("visited(\"counter\")")
        );
        assert_eq!(m.get("priority").unwrap().as_i64(), Some(10));
    }

    #[test]
    fn explicit_keys_win_and_apply_is_idempotent() {
        let mut doc = scene("id: counter\npriority: 5\nafter: 'visited(\"x\")'\n");
        apply_sequence(&mut doc, &seq());
        let once = doc.meta.raw_yaml.clone();
        apply_sequence(&mut doc, &seq());
        assert_eq!(doc.meta.raw_yaml, once);
        let m = map(&doc);
        assert_eq!(m.get("priority").unwrap().as_i64(), Some(5));
        assert_eq!(m.get("after").unwrap().as_str(), Some("visited(\"x\")"));
        assert_eq!(m.get("on").unwrap().as_str(), Some("chapter"));
    }

    #[test]
    fn unlisted_scene_and_non_scene_are_untouched() {
        let mut other = scene("id: epilogue\n");
        apply_sequence(&mut other, &seq());
        assert!(!other.meta.raw_yaml.contains(SEQUENCE_MARKER));
        let mut quest = lute_syntax::parse(
            "---\nkind: quest\nid: prologue\n---\n<quest id=\"q\" title=\"Q\">\n</quest>\n",
        )
        .0;
        apply_sequence(&mut quest, &seq());
        assert!(!quest.meta.raw_yaml.contains(SEQUENCE_MARKER));
    }

    #[test]
    fn derived_key_spans_anchor_at_the_scene_id() {
        let mut doc = scene("id: counter\n");
        let id_span = crate::meta::meta_key_span(&doc.meta, "id");
        apply_sequence(&mut doc, &seq());
        assert_eq!(
            crate::meta::meta_key_span(&doc.meta, "after").byte_start,
            id_span.byte_start
        );
        assert_eq!(
            crate::meta::meta_key_span(&doc.meta, "id").byte_start,
            id_span.byte_start
        );
    }

    #[test]
    fn project_check_names_unknown_ids_and_on_conflicts() {
        let s = Sequence {
            occasion: "chapter".to_string(),
            scenes: vec!["prologue".into(), "countr".into(), "kitchen".into()],
        };
        let manifest = "defaultProfile: core\nsequence: { occasion: chapter, scenes: [prologue, countr, kitchen] }\n";
        let docs = vec![
            (PathBuf::from("a.lute"), scene("id: prologue\n")),
            (PathBuf::from("b.lute"), scene("id: counter\n")),
            (PathBuf::from("c.lute"), scene("id: kitchen\non: dinner\n")),
        ];
        let diags = check_sequence(&s, manifest, &docs);
        assert_eq!(diags.len(), 2, "{diags:#?}");
        assert!(
            diags[0].message.contains("did you mean `counter`"),
            "{}",
            diags[0].message
        );
        assert_eq!(
            &manifest[diags[0].span.byte_start..diags[0].span.byte_end],
            "countr"
        );
        assert!(
            diags[1].message.contains("on: dinner"),
            "{}",
            diags[1].message
        );
        assert_eq!(
            &manifest[diags[1].span.byte_start..diags[1].span.byte_end],
            "kitchen"
        );
    }
}
