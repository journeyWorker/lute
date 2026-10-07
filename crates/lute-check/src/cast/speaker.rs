use super::*;

use std::collections::BTreeMap;

use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_manifest::schema::CastMember;
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_syntax::ast::{Arm, AttrValue, ClipNode, Directive, Document, Line, Node};

use crate::schema_import::SchemaImports;


/// The cast a document is checked against: the active plugins' `cast`
/// exports, the import-reachable schemas' `cast:` and — for a schema
/// document itself — its own `cast:`. A plugin entry wins a same-id clash
/// (it carries the engine's display name).
pub fn declared_cast(
    snapshot: &CapabilitySnapshot,
    imports: &SchemaImports,
    own: &[CastMember],
) -> BTreeMap<String, CastMember> {
    let mut cast = snapshot.cast.clone();
    for c in imports.cast.values().chain(own) {
        cast.entry(c.id.clone()).or_insert_with(|| c.clone());
    }
    cast
}

/// `E-CAST-UNKNOWN` for every line of `doc` (scene shots, quest bodies, lore
/// entries and bundle beats) whose speaker is neither `narrator` nor a cast
/// member, and (dsl 0.24.0 §4) for every `::actor{character}` /
/// `::camera{focus}` literal naming such an id — timeline clips included.
/// A def-valued attribute (`character=@who`) is not a literal id and is left
/// to the def rules. Silent when `cast` is empty (shape-only). A `@@p:`
/// speaker param (dsl 0.26.0 §3.2) names no member here: each `::use` binds
/// it, and its argument is held to the cast there.
pub fn check_speakers(doc: &Document, cast: &BTreeMap<String, CastMember>) -> Vec<Diagnostic> {
    if cast.is_empty() {
        return Vec::new();
    }
    let mut lines: Vec<&Line> = Vec::new();
    let mut staged: Vec<(&str, &str, Span)> = Vec::new();
    let bodies = doc.sections
        .iter()
        .map(|s| &s.body)
        .chain(doc.quests.iter().map(|q| &q.body))
        .chain(doc.entries.iter().map(|e| &e.body))
        .chain(doc.beats.iter().map(|b| &b.body));
    for body in bodies {
        crate::match_check::collect_lines(body, &mut lines);
        collect_staged(body, &mut staged);
    }
    let known = |id: &str| id == "narrator" || id.starts_with('@') || cast.contains_key(id);
    let mut out: Vec<Diagnostic> = lines
        .into_iter()
        .filter(|l| !known(&l.speaker))
        .map(|l| {
            // The speaker id sits just past the line's leading `@`;
            // line/column are recomputed from the bytes when the checker
            // normalizes spans.
            let start = l.span.byte_start + 1;
            let span = Span {
                byte_start: start,
                byte_end: start + l.speaker.len(),
                line: 0,
                column: 0,
                utf16_range: (0, 0),
            };
            unknown(format!("speaker `{}`", l.speaker), &l.speaker, span, cast)
        })
        .collect();
    out.extend(
        staged
            .into_iter()
            .filter(|(_, id, _)| !known(id))
            .map(|(what, id, span)| unknown(format!("{what} `{id}`"), id, span, cast)),
    );
    out.sort_by_key(|d| d.span.byte_start);
    out
}

/// The character-naming attribute of a staging directive (dsl 0.24.0 §4):
/// `::actor{character}` and `::camera{focus}`.
fn staged_attr(tag: &str) -> Option<&'static str> {
    match tag {
        lute_manifest::core::ACTOR_DIRECTIVE => Some("character"),
        "camera" => Some("focus"),
        _ => None,
    }
}

fn push_staged<'a>(d: &'a Directive, out: &mut Vec<(&'static str, &'a str, Span)>) {
    let Some(key) = staged_attr(&d.tag) else {
        return;
    };
    let what = if key == "character" {
        "`::actor{character}`"
    } else {
        "`::camera{focus}`"
    };
    for a in d.attrs.iter().filter(|a| a.key == key) {
        if let AttrValue::Str(id) = &a.value {
            out.push((what, id.as_str(), a.value_span));
        }
    }
}

/// Every staging directive in document order, descending the same bodies as
/// [`crate::match_check::collect_lines`] plus timeline clips.
fn collect_staged<'a>(nodes: &'a [Node], out: &mut Vec<(&'static str, &'a str, Span)>) {
    for node in nodes {
        match node {
            Node::Directive(d) => push_staged(d, out),
            Node::Timeline(t) => {
                for clip in t.tracks.iter().flat_map(|tr| &tr.clips) {
                    if let ClipNode::Directive(d) = &clip.node {
                        push_staged(d, out);
                    }
                }
            }
            Node::Branch(b) => b.choices.iter().for_each(|c| collect_staged(&c.body, out)),
            Node::Hub(h) => h.bodies().for_each(|b| collect_staged(b, out)),
            Node::Match(m) => {
                for arm in &m.arms {
                    match arm {
                        Arm::When { body, .. } | Arm::Otherwise { body, .. } => {
                            collect_staged(body, out)
                        }
                    }
                }
            }
            Node::Objective(o) => collect_staged(&o.body, out),
            Node::On(o) => collect_staged(&o.body, out),
            Node::Line(_) | Node::Set(_) | Node::Assert(_) | Node::Retract(_) => {}
        }
    }
}

pub(crate) fn unknown(
    what: String,
    id: &str,
    span: Span,
    cast: &BTreeMap<String, CastMember>,
) -> Diagnostic {
    let mut message = format!("{what} is not in the declared cast (dsl 0.23.0 §7)");
    let candidates = cast.keys().map(String::as_str).chain(["narrator"]);
    if let Some(near) = lute_manifest::suggest::nearest(id, candidates, 2) {
        message.push_str(&format!(" — did you mean `{near}`?"));
    }
    Diagnostic {
        code: E_CAST_UNKNOWN.to_string(),
        severity: Severity::Error,
        message,
        evidence: None,
        span,
        layer: Layer::Content,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    }
}
