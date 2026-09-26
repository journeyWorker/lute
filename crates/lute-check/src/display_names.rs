//! dsl 0.26.0 §2.8 (T3-13): two speakers the player cannot tell apart. Ids
//! are unique project-wide, but the dialogue box shows a display name: two
//! cast entries with the same `name:`, a cast `name:` equal to a component
//! `::use{… name="…"}` display string, or two such strings for different
//! speakers, read as one person. `W-DISPLAY-NAME-DUP` is advisory
//! (`check-project` and `lute lint`); names are compared exactly.
//!
//! A `::use` display string belongs to the speaker its `who=` names (the
//! speaker-param convention, §3.2), so a trainer's battle and rematch are one
//! speaker; a `::use` without `who=` is its own speaker. A `::use` whose
//! component speaks as a `speaker` param (`@@who:`) is where the member it
//! binds speaks.

use std::collections::BTreeMap;
use std::path::PathBuf;

use lute_core_span::{Diagnostic, Layer, RelatedDiagnostic, Severity, Span};
use lute_manifest::schema::CastMember;
use lute_syntax::ast::{Arm, AttrValue, ClipNode, Directive, Document, Line, Node};

pub const W_DISPLAY_NAME_DUP: &str = "W-DISPLAY-NAME-DUP";

/// One speaker showing a display name, and the first place it is shown.
struct Speaker {
    /// The cast id or `who=` value; a `who`-less `::use` is `None` (its own
    /// speaker).
    id: Option<String>,
    site: Option<(usize, Span)>,
}

/// `W-DISPLAY-NAME-DUP` over one project root. `casts` is index-aligned with
/// `docs` (each document's declared cast, [`crate::cast::declared_cast`]);
/// their union is the project cast. `use_lines` is index-aligned too: each
/// document's bound `@@p:` lines by `::use` offset
/// ([`crate::FoldedEnv::use_lines`]). One warning per shared display name,
/// at the first place the second speaker is shown; the other speakers'
/// sites are `related`.
pub fn check_display_names(
    docs: &[(PathBuf, Document)],
    casts: &[&BTreeMap<String, CastMember>],
    use_lines: &[&BTreeMap<usize, Vec<Line>>],
) -> Vec<(PathBuf, Diagnostic)> {
    // display name -> speakers showing it, in first-seen order
    let mut shown: BTreeMap<&str, Vec<Speaker>> = BTreeMap::new();
    let mut cast_names: BTreeMap<&str, &str> = BTreeMap::new();
    for cast in casts {
        for (id, member) in cast.iter() {
            if let Some(name) = member.name.as_deref().filter(|n| !n.trim().is_empty()) {
                cast_names.entry(id.as_str()).or_insert(name);
            }
        }
    }
    // first line each cast id speaks, in project order
    let mut spoke: BTreeMap<&str, (usize, Span)> = BTreeMap::new();
    let mut uses: Vec<(&str, Option<&str>, usize, Span)> = Vec::new();
    for (i, (_, doc)) in docs.iter().enumerate() {
        for_each_node(doc, &mut |node| match node {
            Visit::Line(l) => {
                spoke.entry(l.speaker.as_str()).or_insert((i, l.span));
            }
            Visit::Directive(d) if d.tag == "use" => {
                // dsl 0.26.0 §3.2: a `@@who:` line speaks at the `::use`.
                let bound = use_lines.get(i).and_then(|m| m.get(&d.span.byte_start));
                for l in bound.into_iter().flatten() {
                    spoke.entry(l.speaker.as_str()).or_insert((i, d.span));
                }
                let Some((name, span)) = literal(d, "name") else {
                    return;
                };
                uses.push((name, literal(d, "who").map(|(w, _)| w), i, span));
            }
            Visit::Directive(_) => {}
        });
    }
    for (id, name) in &cast_names {
        shown.entry(name).or_default().push(Speaker {
            id: Some((*id).to_string()),
            site: spoke.get(id).copied(),
        });
    }
    for (name, who, doc, span) in uses {
        let speakers = shown.entry(name).or_default();
        match who.and_then(|w| speakers.iter_mut().find(|s| s.id.as_deref() == Some(w))) {
            Some(s) => {
                if s.site
                    .is_none_or(|site| (doc, span.byte_start) < (site.0, site.1.byte_start))
                {
                    s.site = Some((doc, span));
                }
            }
            None => speakers.push(Speaker {
                id: who.map(str::to_string),
                site: Some((doc, span)),
            }),
        }
    }

    let mut out = Vec::new();
    for (name, mut speakers) in shown {
        if speakers.len() < 2 {
            continue;
        }
        // project order; a cast entry nobody speaks as sorts last
        speakers.sort_by_key(|s| s.site.map_or((usize::MAX, 0), |(d, sp)| (d, sp.byte_start)));
        let at = |s: &Speaker| -> String {
            match s.site {
                Some((d, sp)) => format!("{}:{}", docs[d].0.display(), sp.line),
                None => "the cast, never spoken".to_string(),
            }
        };
        let label = |s: &Speaker| -> String {
            match &s.id {
                Some(id) => format!("`{id}` ({})", at(s)),
                None => format!("a `::use` without `who=` ({})", at(s)),
            }
        };
        let listed = speakers.iter().map(label).collect::<Vec<_>>().join(", ");
        let message = format!(
            "display name `{name}` is shown for {} different speakers: {listed} — the dialogue \
             box cannot tell them apart; rename one (dsl 0.26.0 §2.8)",
            speakers.len()
        );
        // Anchor at the second speaker's first site, else the first site
        // anyone has, else the first document's head.
        let anchor = speakers[1]
            .site
            .or(speakers[0].site)
            .map(|(d, sp)| (docs[d].0.clone(), sp));
        let Some((path, span)) =
            anchor.or_else(|| docs.first().map(|(p, d)| (p.clone(), d.meta.span)))
        else {
            continue;
        };
        let related = speakers
            .iter()
            .filter_map(|s| s.site)
            .filter(|(d, sp)| !(docs[*d].0 == path && *sp == span))
            .map(|(d, sp)| RelatedDiagnostic {
                file: docs[d].0.display().to_string(),
                diagnostic: warning(message.clone(), sp, Vec::new()),
            })
            .collect();
        out.push((path, warning(message, span, related)));
    }
    out
}

fn warning(message: String, span: Span, related: Vec<RelatedDiagnostic>) -> Diagnostic {
    Diagnostic {
        code: W_DISPLAY_NAME_DUP.to_string(),
        severity: Severity::Warning,
        message,
        span,
        layer: Layer::Content,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related,
    }
}

/// The literal string value of `key` on `d`, with its value span.
fn literal<'d>(d: &'d Directive, key: &str) -> Option<(&'d str, Span)> {
    d.attrs
        .iter()
        .find(|a| a.key == key)
        .and_then(|a| match &a.value {
            AttrValue::Str(s) if !s.trim().is_empty() => Some((s.as_str(), a.value_span)),
            _ => None,
        })
}

enum Visit<'d> {
    Line(&'d Line),
    Directive(&'d Directive),
}

/// Every content line and directive of `doc`, in document order: shots,
/// quests, entries, bundle beats, and every nested body and timeline clip.
fn for_each_node<'d>(doc: &'d Document, f: &mut dyn FnMut(Visit<'d>)) {
    let bodies = doc
        .shots
        .iter()
        .map(|s| &s.body)
        .chain(doc.quests.iter().map(|q| &q.body))
        .chain(doc.entries.iter().map(|e| &e.body))
        .chain(doc.beats.iter().map(|b| &b.body));
    for body in bodies {
        walk(body, f);
    }
}

fn walk<'d>(nodes: &'d [Node], f: &mut dyn FnMut(Visit<'d>)) {
    for node in nodes {
        match node {
            Node::Line(l) => f(Visit::Line(l)),
            Node::Directive(d) => f(Visit::Directive(d)),
            Node::Timeline(t) => {
                for clip in t.tracks.iter().flat_map(|tr| &tr.clips) {
                    if let ClipNode::Directive(d) = &clip.node {
                        f(Visit::Directive(d));
                    }
                }
            }
            Node::Match(m) => {
                for arm in &m.arms {
                    let (Arm::When { body, .. } | Arm::Otherwise { body, .. }) = arm;
                    walk(body, f);
                }
            }
            Node::Branch(b) => b.choices.iter().for_each(|c| walk(&c.body, f)),
            Node::Hub(h) => h.choices.iter().for_each(|c| walk(&c.body, f)),
            Node::On(o) => walk(&o.body, f),
            Node::Objective(o) => walk(&o.body, f),
            Node::Set(_) | Node::Assert(_) | Node::Retract(_) => {}
        }
    }
}
