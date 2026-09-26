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

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_manifest::project::{MetaDefaults, Sequence, SequenceAnchor, E_SEQUENCE};
use lute_manifest::schema::{OccasionDecl, OccasionSelect};
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

/// Whether frontmatter `key` is one the manifest's `sequence:` derived
/// (below [`SEQUENCE_MARKER`]), not one the scene wrote.
pub fn derived(meta: &lute_syntax::ast::Meta, key: &str) -> bool {
    meta.raw_yaml.find(SEQUENCE_MARKER).is_some_and(|i| {
        meta.raw_yaml[i + SEQUENCE_MARKER.len()..]
            .lines()
            .any(|l| l.split_once(':').is_some_and(|(k, _)| k == key))
    })
}

/// Whether the sequence on `occasion` chains its scenes with `after:` —
/// every occasion but a `select: sequence` one, which presents every
/// eligible beat in one raise (the priority order is the chain there).
pub fn chained(occasion: &str, occasions: &BTreeMap<String, OccasionDecl>) -> bool {
    occasions
        .get(occasion)
        .is_none_or(|d| d.select != OccasionSelect::Sequence)
}

/// Scene id -> the `after:` `defaults`' `sequence:` derives for it
/// (`visited("<previous>")`), for a reason to say where an `after:` the
/// scene never wrote comes from. Empty without a sequence or on a
/// `select: sequence` occasion ([`chained`]).
pub fn derived_afters(
    defaults: &MetaDefaults,
    occasions: &BTreeMap<String, OccasionDecl>,
) -> BTreeMap<String, String> {
    let Some(sequence) = defaults.sequence() else {
        return BTreeMap::new();
    };
    let chained = chained(&sequence.occasion, occasions);
    sequence
        .scenes
        .iter()
        .filter_map(|id| {
            let (_, after) = sequence
                .derived(id, chained)?
                .into_iter()
                .find(|(k, _)| *k == "after")?;
            Some((id.clone(), after.as_str()?.to_string()))
        })
        .collect()
}

/// Append the keys `defaults`' `sequence:` derives for this scene (dsl
/// 0.27.0 §8) to its frontmatter. A key the scene writes itself wins. A
/// no-op for a document that is no scene, a scene the sequence does not
/// list, a frontmatter that does not parse, or one already applied —
/// idempotent, so every surface may call it on the documents it parses.
/// `occasions` is the resolved vocabulary ([`chained`]).
pub fn apply_sequence(
    doc: &mut Document,
    defaults: &MetaDefaults,
    occasions: &BTreeMap<String, OccasionDecl>,
) {
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
    let chained = chained(&sequence.occasion, occasions);
    let Some(derived) = sequence.derived(id.trim(), chained) else {
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

/// A listed scene that may never play (its own `when:`) stalls every scene
/// after it that waits on it through the derived `after:` (dsl 0.27.0 §8).
pub const W_SEQUENCE_STALL: &str = "W-SEQUENCE-STALL";

fn span_at(start: usize, len: usize) -> Span {
    Span {
        byte_start: start,
        byte_end: start + len,
        line: 0,
        column: 0,
        utf16_range: (0, 0),
    }
}

/// Where `key:` sits in `text` from byte `from` on — as a mapping key (at a
/// line start after indent, or after `{` / `,` in a flow mapping).
fn key_at(text: &str, key: &str, from: usize) -> Option<usize> {
    text[from..]
        .match_indices(key)
        .map(|(i, _)| from + i)
        .find(|&i| {
            let before = text[..i].trim_end_matches([' ', '\t']);
            let opens = before.is_empty() || before.ends_with(['\n', '{', ',']);
            opens
                && text[i + key.len()..]
                    .trim_start_matches([' ', '\t'])
                    .starts_with(':')
        })
}

/// The span `anchor` points at in the manifest text: the `sequence:` key,
/// a key of the block, or the `nth` word-bounded occurrence of an entry's
/// text after `scenes:` — falling back to the block's key, then byte 0.
fn locate(manifest: &str, anchor: &SequenceAnchor) -> Span {
    let block = key_at(manifest, "sequence", 0);
    let from = block.unwrap_or(0);
    let whole = || span_at(from, if block.is_some() { "sequence".len() } else { 0 });
    match anchor {
        SequenceAnchor::Block => whole(),
        SequenceAnchor::Key(key) => {
            key_at(manifest, key, from).map_or_else(whole, |i| span_at(i, key.len()))
        }
        SequenceAnchor::Entry(text, nth) => {
            let from = key_at(manifest, "scenes", from).unwrap_or(from);
            let bounded = |i: usize| {
                let ok = |c: Option<char>| {
                    !c.is_some_and(|c| c.is_ascii_alphanumeric() || "_.-".contains(c))
                };
                ok(manifest[..i].chars().next_back())
                    && ok(manifest[i + text.len()..].chars().next())
            };
            if text.is_empty() {
                return whole();
            }
            manifest[from..]
                .match_indices(text.as_str())
                .map(|(i, _)| from + i)
                .filter(|&i| bounded(i))
                .nth(*nth)
                .map_or_else(whole, |i| span_at(i, text.len()))
        }
    }
}

/// dsl 0.27.0 §8, project-wide, anchored in `lute.project.yaml`: a
/// malformed `sequence:` (the load's shape diagnostics, located — the
/// documents are still checked; a malformed chain is simply not applied),
/// then [`check_sequence`]. `docs` are the root's documents, already
/// desugared; `occasions` the root's resolved vocabulary.
pub fn check_project_sequence(
    root: &Path,
    docs: &[(PathBuf, Document)],
    occasions: &BTreeMap<String, OccasionDecl>,
) -> Vec<(PathBuf, Diagnostic)> {
    let Ok(Some(project)) = lute_manifest::project::load_project(root) else {
        return Vec::new();
    };
    let manifest_path = root.join("lute.project.yaml");
    let manifest = std::fs::read_to_string(&manifest_path).unwrap_or_default();
    let shape = project
        .sequence_diags
        .iter()
        .map(|d| diag(d.message.clone(), locate(&manifest, &d.anchor)));
    let ids = project
        .defaults
        .sequence()
        .map(|s| check_sequence(s, &manifest, docs, occasions))
        .unwrap_or_default();
    shape
        .chain(ids)
        .map(|d| (manifest_path.clone(), d))
        .collect()
}

/// Every occasion a beat of `docs` answers by its own hand — a scene's
/// authored `on:`, an entry's or bundle beat's `on=` — with how many.
fn answered_occasions(docs: &[(PathBuf, Document)]) -> BTreeMap<String, usize> {
    let mut out: BTreeMap<String, usize> = BTreeMap::new();
    for (_, doc) in docs {
        let scene_on = serde_yaml::from_str::<serde_yaml::Value>(authored_yaml(&doc.meta.raw_yaml))
            .ok()
            .and_then(|v| v.get("on")?.as_str().map(str::to_string));
        let ons = scene_on
            .into_iter()
            .chain(
                doc.entries
                    .iter()
                    .filter_map(|e| Some(e.on.as_ref()?.0.clone())),
            )
            .chain(
                doc.beats
                    .iter()
                    .filter_map(|b| Some(b.on.as_ref()?.0.clone())),
            );
        for on in ons {
            *out.entry(on.trim().to_string()).or_default() += 1;
        }
    }
    out
}

/// [`check_project_sequence`] over an already-loaded sequence and the
/// manifest text its spans point into:
///
/// * `sequence.occasion` names a declared occasion (with a did-you-mean);
///   while no plugin declares occasions (shape-only), a near-miss of an
///   occasion other beats answer when none answers the sequence's;
/// * every listed id names a scene of the project (with a did-you-mean);
/// * no listed scene answers another occasion with its own `on:` — the
///   chain would skip it;
/// * [`W_SEQUENCE_STALL`]: a listed scene with its own `when:` stalls the
///   next one when that one waits on it through the derived `after:`.
pub fn check_sequence(
    sequence: &Sequence,
    manifest: &str,
    docs: &[(PathBuf, Document)],
    occasions: &BTreeMap<String, OccasionDecl>,
) -> Vec<Diagnostic> {
    let scenes = crate::connectivity::scene_key_set(docs);
    let mut out = Vec::new();
    let occasion = &sequence.occasion;
    let occasion_span = || locate(manifest, &SequenceAnchor::Key("occasion".to_string()));
    if !occasions.is_empty() && !occasions.contains_key(occasion) {
        let hint =
            lute_manifest::suggest::nearest(occasion, occasions.keys().map(String::as_str), 2)
                .map(|n| format!(" — did you mean `{n}`?"))
                .unwrap_or_default();
        out.push(diag(
            format!(
                "`sequence.occasion` is `{occasion}`, which no resolved plugin or manifest \
                 declares{hint} (dsl 0.27.0 §8)"
            ),
            occasion_span(),
        ));
    } else if occasions.is_empty() {
        let answered = answered_occasions(docs);
        if !answered.contains_key(occasion) {
            let near =
                lute_manifest::suggest::nearest(occasion, answered.keys().map(String::as_str), 2);
            if let Some(near) = near {
                let n = answered[near];
                out.push(diag(
                    format!(
                        "`sequence.occasion` is `{occasion}`, which no other beat answers, while \
                         {n} beat{s} answer{v} `{near}` — did you mean `{near}`? (dsl 0.27.0 §8)",
                        s = if n == 1 { "" } else { "s" },
                        v = if n == 1 { "s" } else { "" },
                    ),
                    occasion_span(),
                ));
            }
        }
    }
    let chained = chained(occasion, occasions);
    for (i, id) in sequence.scenes.iter().enumerate() {
        let span = locate(manifest, &SequenceAnchor::Entry(id.clone(), 0));
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
        let next = sequence.scenes.get(i + 1);
        for (path, _) in homes {
            let Some((_, doc)) = docs.iter().find(|(p, _)| p == path) else {
                continue;
            };
            let authored =
                serde_yaml::from_str::<serde_yaml::Value>(authored_yaml(&doc.meta.raw_yaml)).ok();
            let own = |key: &str| {
                authored.as_ref().and_then(|v| match v.get(key)? {
                    serde_yaml::Value::String(s) => Some(s.trim().to_string()),
                    serde_yaml::Value::Bool(b) => Some(b.to_string()),
                    _ => None,
                })
            };
            if let Some(on) = own("on").filter(|on| on != occasion) {
                out.push(diag(
                    format!(
                        "`sequence.scenes` lists `{id}`, but its frontmatter says `on: {on}`, so it \
                         never answers `{occasion}` and the chain stops before it — remove its `on:` \
                         or take it out of the sequence (dsl 0.27.0 §8)"
                    ),
                    span,
                ));
            }
            // The next scene waits on this one only through an `after:`
            // the sequence derived for it.
            let stalls = chained
                && next.is_some_and(|n| {
                    scenes.get(n).is_some_and(|homes| {
                        homes.iter().any(|(p, _)| {
                            docs.iter()
                                .find(|(q, _)| q == p)
                                .is_some_and(|(_, d)| derived(&d.meta, "after"))
                        })
                    })
                });
            if let (Some(when), true) = (own("when").filter(|w| w != "true"), stalls) {
                let next = next.map_or("", String::as_str);
                out.push(Diagnostic {
                    code: W_SEQUENCE_STALL.to_string(),
                    severity: Severity::Warning,
                    ..diag(
                        format!(
                            "`sequence.scenes` lists `{id}`, which plays only `when: {when}`; when \
                             it does not, `{next}` waits forever on the `after: visited(\"{id}\")` \
                             the sequence writes for it — give `{next}` its own `after:` (e.g. \
                             `after: visited(\"{prev}\")`), or take `{id}` out of the list \
                             (dsl 0.27.0 §8)",
                            prev = i
                                .checked_sub(1)
                                .map_or(id.as_str(), |p| sequence.scenes[p].as_str()),
                        ),
                        span,
                    )
                });
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
        apply_sequence(&mut first, &seq(), &BTreeMap::new());
        let m = map(&first);
        assert_eq!(m.get("on").unwrap().as_str(), Some("chapter"));
        assert!(
            m.get("after").is_none(),
            "the first scene waits for nothing"
        );
        assert_eq!(m.get("priority").unwrap().as_i64(), Some(30));

        let mut third = scene("id: kitchen\n");
        apply_sequence(&mut third, &seq(), &BTreeMap::new());
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
        apply_sequence(&mut doc, &seq(), &BTreeMap::new());
        let once = doc.meta.raw_yaml.clone();
        apply_sequence(&mut doc, &seq(), &BTreeMap::new());
        assert_eq!(doc.meta.raw_yaml, once);
        let m = map(&doc);
        assert_eq!(m.get("priority").unwrap().as_i64(), Some(5));
        assert_eq!(m.get("after").unwrap().as_str(), Some("visited(\"x\")"));
        assert_eq!(m.get("on").unwrap().as_str(), Some("chapter"));
    }

    #[test]
    fn unlisted_scene_and_non_scene_are_untouched() {
        let mut other = scene("id: epilogue\n");
        apply_sequence(&mut other, &seq(), &BTreeMap::new());
        assert!(!other.meta.raw_yaml.contains(SEQUENCE_MARKER));
        let mut quest = lute_syntax::parse(
            "---\nkind: quest\nid: prologue\n---\n<quest id=\"q\" title=\"Q\">\n</quest>\n",
        )
        .0;
        apply_sequence(&mut quest, &seq(), &BTreeMap::new());
        assert!(!quest.meta.raw_yaml.contains(SEQUENCE_MARKER));
    }

    #[test]
    fn derived_key_spans_anchor_at_the_scene_id() {
        let mut doc = scene("id: counter\n");
        let id_span = crate::meta::meta_key_span(&doc.meta, "id");
        apply_sequence(&mut doc, &seq(), &BTreeMap::new());
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
        let diags = check_sequence(&s, manifest, &docs, &BTreeMap::new());
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
