// ── dsl 0.24.0 §4: presence (`W-CAST-ABSENT`) and per-speaker emotions ──────

use std::collections::BTreeMap;
use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_manifest::schema::CastMember;
use lute_manifest::snapshot::Domain;
use lute_syntax::ast::{Arm, AttrValue, CelKind, ClipNode, Document, Line, Node};

/// Conservative public guard-implication decider for project constraints.
/// Exact guards are sufficient proof; an exact negated guard proves the
/// condition false. Conjunction is represented by one entry per enclosing
/// guard, so any conjunct can establish either implication.
pub fn decide_guard_implication(guards: &[&str], target: &str) -> Option<bool> {
    let target = target.trim();
    if target == "true" {
        return Some(true);
    }
    if target == "false" {
        return Some(false);
    }
    if guards.iter().any(|guard| guard.trim() == target) {
        return Some(true);
    }
    let target_negation = negate_guard(target);
    if guards.iter().any(|guard| guard.trim() == target_negation) {
        return Some(false);
    }
    None
}

fn negate_guard(raw: &str) -> String {
    let raw = raw.trim();
    if let Some(inner) = raw.strip_prefix('!') {
        inner.trim().to_string()
    } else {
        format!("!{raw}")
    }
}
/// `W-CAST-ABSENT` (dsl 0.24.0 §4): a content line by a speaker whose cast
/// entry declares `present:`, where the conjunction of the line's enclosing
/// guards does not imply that condition.
pub const W_CAST_ABSENT: &str = "W-CAST-ABSENT";

pub(crate) fn cast_diag(
    code: &str,
    severity: Severity,
    layer: Layer,
    message: String,
    span: Span,
) -> Diagnostic {
    let evidence = match crate::evidence::classification(code) {
        Some(crate::evidence::DiagnosticClass::Analysis { evidence }) => Some(evidence),
        _ => None,
    };
    Diagnostic {
        code: code.to_string(),
        severity,
        message,
        span,
        layer,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
        evidence,
    }
}

/// dsl 0.24.0 §4: a `present:` condition's own faults, at `span` — it must
/// parse (`E-CEL-PARSE`) and stay inside the CEL profile (`E-CEL-PROFILE`),
/// the gate a `defs:` body gets.
pub(crate) fn present_faults(id: &str, cel: &str, span: Span) -> Vec<Diagnostic> {
    let mut arena = lute_cel::CelArena::default();
    if let Err(err) = lute_cel::parse_slot(&mut arena, cel, span.byte_start) {
        let t = crate::cel_message::translate_cel_parse(cel, span, &err, CelKind::Condition);
        return vec![cast_diag(
            t.code,
            Severity::Error,
            Layer::Cel,
            format!("cast `{id}` `present:`: {}", t.message),
            span,
        )];
    }
    let prefix = format!("def `{id}`: ");
    crate::cel_resolve::check_def_body(id, cel, &[], span, &crate::meta::StateSchema::default())
        .into_iter()
        .filter(|d| d.code == crate::cel_resolve::E_CEL_PROFILE)
        .map(|mut d| {
            let body = d.message.strip_prefix(&prefix).unwrap_or(&d.message);
            d.message = format!("cast `{id}` `present:`: {body}");
            d
        })
        .collect()
}

/// dsl 0.24.0 §4: one cast entry's faults where it is declared (a schema
/// document's `cast:`), all at `span` — a `present:` that is not a
/// condition, and an `emotions:` member outside a closed `emotion` enum of
/// `domains`. A faulty `present` is dropped from the member, so presence is
/// never decided against a condition already reported.
pub(crate) fn validate_member(
    member: &mut CastMember,
    span: Span,
    domains: &BTreeMap<String, Domain>,
) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    if let Some(cel) = &member.present {
        out.extend(present_faults(&member.id, cel, span));
        if !out.is_empty() {
            member.present = None;
        }
    }
    if let (Some(emotions), Some(dom)) =
        (&member.emotions, domains.get("emotion").filter(|d| !d.open))
    {
        for e in emotions.iter().filter(|e| !dom.members.contains(e)) {
            out.push(cast_diag(
                "E-BAD-ENUM",
                Severity::Error,
                Layer::Content,
                format!(
                    "cast `{}` lists emotion `{e}`, which is not a member of the `emotion` enum \
                     (expected one of: {}) (dsl 0.24.0 §4)",
                    member.id,
                    dom.members.join(", ")
                ),
                span,
            ));
        }
    }
    out
}

/// Every body of `doc` a content line can sit in: scene shots, quest
/// bodies, lore entries and bundle beats (the bodies [`check_speakers`]
/// visits).
pub(crate) fn doc_bodies(doc: &Document) -> impl Iterator<Item = &Vec<Node>> {
    doc.sections
        .iter()
        .map(|s| &s.body)
        .chain(doc.quests.iter().map(|q| &q.body))
        .chain(doc.entries.iter().map(|e| &e.body))
        .chain(doc.beats.iter().map(|b| &b.body))
}

/// Visit every node of `nodes`, nested bodies and timeline clips' directives
/// included.
pub(crate) fn visit<'n>(nodes: &'n [Node], f: &mut impl FnMut(&'n Node)) {
    for node in nodes {
        f(node);
        match node {
            Node::Branch(b) => b.choices.iter().for_each(|c| visit(&c.body, f)),
            Node::Hub(h) => h.bodies().for_each(|b| visit(b, f)),
            Node::Match(m) => {
                for arm in &m.arms {
                    let (Arm::When { body, .. } | Arm::Otherwise { body, .. }) = arm;
                    visit(body, f);
                }
            }
            Node::On(o) => visit(&o.body, f),
            Node::Objective(o) => visit(&o.body, f),
            Node::Line(_)
            | Node::Directive(_)
            | Node::Set(_)
            | Node::Timeline(_)
            | Node::Assert(_)
            | Node::Retract(_) => {}
        }
    }
}

pub(crate) fn literal_attr<'a>(attrs: &'a [lute_syntax::ast::Attr], key: &str) -> Option<(&'a str, Span)> {
    attrs
        .iter()
        .find(|a| a.key == key)
        .and_then(|a| match &a.value {
            AttrValue::Str(s) => Some((s.as_str(), a.value_span)),
            _ => None,
        })
}

/// dsl 0.24.0 §4: `E-BAD-ENUM` for an `emotion=` outside the declared
/// `emotions:` of the content line's speaker — or of the literal
/// `character=` a directive (a timeline clip included) names. A value the
/// closed `emotion` enum itself rejects is left to that check, so each bad
/// value is one error. dsl 0.26.0 §3.2: a component's `@@p:` lines are
/// judged at each `::use` (`use_lines`, [`FoldedEnv::use_lines`]) against
/// the member it binds.
pub fn check_emotions(
    doc: &Document,
    cast: &BTreeMap<String, CastMember>,
    domains: &BTreeMap<String, Domain>,
    use_lines: &BTreeMap<usize, Vec<Line>>,
) -> Vec<Diagnostic> {
    if !cast.values().any(|c| c.emotions.is_some()) {
        return Vec::new();
    }
    let closed = domains.get("emotion").filter(|d| !d.open);
    let mut out = Vec::new();
    let mut check = |who: &str, attrs: &[lute_syntax::ast::Attr]| {
        let Some(allowed) = cast.get(who).and_then(|c| c.emotions.as_ref()) else {
            return;
        };
        let Some((value, span)) = literal_attr(attrs, "emotion") else {
            return;
        };
        if allowed.iter().any(|e| e == value)
            || closed.is_some_and(|d| !d.members.iter().any(|m| m == value))
        {
            return;
        }
        out.push(cast_diag(
            "E-BAD-ENUM",
            Severity::Error,
            Layer::Content,
            format!(
                "`{value}` is not one of `{who}`'s emotions (expected one of: {}) — the cast \
                 declares `emotions:` for `{who}` (dsl 0.24.0 §4)",
                allowed.join(", ")
            ),
            span,
        ));
    };
    for body in doc_bodies(doc) {
        visit(body, &mut |node| match node {
            Node::Line(l) => check(&l.speaker, &l.attrs),
            Node::Directive(d) if d.tag == "use" => {
                // Every bound line is anchored at the `::use`: one report per
                // member and value there.
                let mut seen = std::collections::BTreeSet::new();
                for l in use_lines.get(&d.span.byte_start).into_iter().flatten() {
                    let emotion = literal_attr(&l.attrs, "emotion").map(|(e, _)| e);
                    if seen.insert((l.speaker.as_str(), emotion)) {
                        check(&l.speaker, &l.attrs);
                    }
                }
            }
            Node::Directive(d) => {
                if let Some((who, _)) = literal_attr(&d.attrs, "character") {
                    check(who, &d.attrs);
                }
            }
            Node::Timeline(t) => {
                for clip in t.tracks.iter().flat_map(|tr| &tr.clips) {
                    if let ClipNode::Directive(d) = &clip.node {
                        if let Some((who, _)) = literal_attr(&d.attrs, "character") {
                            check(who, &d.attrs);
                        }
                    }
                }
            }
            _ => {}
        });
    }
    out
}
