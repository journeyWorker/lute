//! Cast (dsl 0.23.0 §7): a plugin (`cast/*.yaml`) or a schema document
//! (`cast:`) MAY declare the speaker ids a project uses. Once any cast is
//! declared, a content line whose speaker is outside it is
//! [`E_CAST_UNKNOWN`] with a did-you-mean; without one, speakers stay
//! shape-only. `narrator` is always a speaker. Since dsl 0.24.0 §4 the
//! character a staging directive names — `::auto{character}` and
//! `::camera{focus}` — is held to the same cast.

use std::collections::BTreeMap;

use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_manifest::schema::CastMember;
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_syntax::ast::{Arm, AttrValue, ClipNode, Directive, Document, Line, Node};

use crate::schema_import::SchemaImports;

/// A content line's speaker is not in the declared cast (dsl 0.23.0 §7).
pub const E_CAST_UNKNOWN: &str = "E-CAST-UNKNOWN";

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
/// member, and (dsl 0.24.0 §4) for every `::auto{character}` /
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
    let bodies = doc
        .shots
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
/// `::auto{character}` and `::camera{focus}`.
fn staged_attr(tag: &str) -> Option<&'static str> {
    match tag {
        "auto" => Some("character"),
        "camera" => Some("focus"),
        _ => None,
    }
}

fn push_staged<'a>(d: &'a Directive, out: &mut Vec<(&'static str, &'a str, Span)>) {
    let Some(key) = staged_attr(&d.tag) else {
        return;
    };
    let what = if key == "character" {
        "`::auto{character}`"
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
            Node::Hub(h) => h.choices.iter().for_each(|c| collect_staged(&c.body, out)),
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
        span,
        layer: Layer::Content,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    }
}

// ── dsl 0.24.0 §4: presence (`W-CAST-ABSENT`) and per-speaker emotions ──────

use std::collections::BTreeSet;
use std::path::Path;

use cel_parser::ast::{operators as op, CallExpr, Expr, IdedExpr};
use cel_parser::reference::Val;
use lute_manifest::relations::KindShape;
use lute_manifest::snapshot::Domain;
use lute_syntax::ast::{CelKind, CelSlot, Match};
use lute_syntax::datalog::{is_anonymous_var, BodyLiteral, FactPattern, FactTerm, RuleTerm};
use lute_syntax::is_pattern::{classify_is_literal, is_alternatives, IsLiteral};

use crate::beats::BeatOnce;
use crate::cel_expand::{expand_cel, subject_text, DefTable};
use crate::check::FoldedEnv;
use crate::decide::{decide, DecideCtx, Decided};
use crate::fact_env::{FactEnv, FactScope};
use crate::match_check::DomainInfo;
use crate::rel_schema::RelVocab;

/// `W-CAST-ABSENT` (dsl 0.24.0 §4): a content line by a speaker whose cast
/// entry declares `present:`, where the conjunction of the line's enclosing
/// guards does not imply that condition.
pub const W_CAST_ABSENT: &str = "W-CAST-ABSENT";

fn cast_diag(
    code: &str,
    severity: Severity,
    layer: Layer,
    message: String,
    span: Span,
) -> Diagnostic {
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
    }
}

/// dsl 0.24.0 §4: a `present:` condition's own faults, at `span` — it must
/// parse (`E-CEL-PARSE`) and stay inside the CEL profile (`E-CEL-PROFILE`),
/// the gate a `defs:` body gets.
fn present_faults(id: &str, cel: &str, span: Span) -> Vec<Diagnostic> {
    let mut arena = lute_cel::CelArena::default();
    if let Err(err) = lute_cel::parse_slot(&mut arena, cel, span.byte_start) {
        let t = crate::cel_message::translate_cel_parse(cel, span, &err, CelKind::Condition);
        return vec![cast_diag(
            "E-CEL-PARSE",
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
fn doc_bodies(doc: &Document) -> impl Iterator<Item = &Vec<Node>> {
    doc.shots
        .iter()
        .map(|s| &s.body)
        .chain(doc.quests.iter().map(|q| &q.body))
        .chain(doc.entries.iter().map(|e| &e.body))
        .chain(doc.beats.iter().map(|b| &b.body))
}

/// Visit every node of `nodes`, nested bodies and timeline clips' directives
/// included.
fn visit<'n>(nodes: &'n [Node], f: &mut impl FnMut(&'n Node)) {
    for node in nodes {
        f(node);
        match node {
            Node::Branch(b) => b.choices.iter().for_each(|c| visit(&c.body, f)),
            Node::Hub(h) => h.choices.iter().for_each(|c| visit(&c.body, f)),
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

fn literal_attr<'a>(attrs: &'a [lute_syntax::ast::Attr], key: &str) -> Option<(&'a str, Span)> {
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

/// `W-CAST-ABSENT` for every content line of `doc` (scene shots, quest
/// bodies, lore entries and bundle beats) whose speaker's cast entry
/// declares `present:` and whose enclosing guards do not imply it — decided
/// per conjunct `p` of `present` as "`guards && !p` is provably false"
/// ([`decide`], split into its disjuncts when it is not decided whole).
///
/// The guards of a line: its own `when=`, every enclosing `<choice when>`
/// (branch and hub), `<match>` arm (its `is=`/`test` over the subject, and
/// the negation of every earlier arm — first match wins), `<on when>`,
/// `<objective done>` (its body runs on completion), and the unit's own
/// `when` — a scene beat's frontmatter `when:`, an entry's or a bundle
/// beat's `when=`. A lore entry's body also assumes its own
/// `entry.<id>.everRead`/`.read` (the reading is under way); a quest's
/// `<on>` handler assumes the quest's state at that event (`active` for a
/// world event and `questActive`, `complete`/`failed` for
/// `questComplete`/`questFailed`) and every conjunct of its `start` that
/// stays true once it held (`entry.<id>.everRead`, `visited('…')`); and
/// `ladder` (keyed by the unit's `on` key/attribute offset,
/// [`crate::beats::presence_ladder`]) adds what the beats above it on the
/// same ladder must have spent. `@def`s are expanded first.
///
/// A `holds(A)` of a derived relation is read through its rules (no seed
/// fact, not engine-`reserved`): where every unifying rule's body is ground
/// once the head is bound the atom is EXACTLY the disjunction of those
/// bodies (a pure `cel()` schedule is the disjunction of its guards) — on
/// both sides — and a guard's positive `holds(A)` implies that disjunction
/// (with the non-ground literals dropped) either way. A cast entry's
/// `assume: true` reads a negated `holds` of an engine-`reserved` relation
/// in `present` as true.
///
/// A guard stops counting once something that may falsify it runs after
/// it on the same path: a `::set` of a path it reads (a rule `cel()` it
/// reads through included), an `::assert`/`::retract` that can move one of
/// its fact atoms the wrong way — same relation with unifiable arguments, or
/// through a rule where the written relation is a premise (a positive
/// premise moves the head with the write, a negated one against it) — an
/// `::accept` of a quest it reads. Sibling `<choice>`es and `<match>` arms
/// are separate paths: a guard survives a branch when it survives every
/// arm. A hub's writes count from its first round, and a `::next` target
/// label drops the guards of the region it sits in (the jump may come from
/// outside it). A `{vo}` line is exempt: a voice-over does not put its
/// speaker in the room (an `{os}` speaker is there, out of frame).
///
/// `facts` is `check-project`'s fact envelope: with it, a `holds(F)` in
/// `present` is also implied when `F` is in the Must set at the line
/// ([`crate::fact_must`] records every unguarded line's own slot). Without
/// it (single-file `check`) no fact query decides, so the warning is a
/// superset of the project's; [`reconcile_presence`] narrows it.
///
/// A `present:` that does not parse here came from a plugin (a schema's is
/// dropped where it is declared, [`validate_member`]); it is reported once
/// per document at its speaker's first line and not decided.
pub fn check_presence(
    path: &Path,
    doc: &Document,
    folded: &FoldedEnv,
    project: Option<&PresenceProject<'_>>,
) -> Vec<Diagnostic> {
    if !folded.cast.values().any(|c| c.present.is_some()) {
        return Vec::new();
    }
    let params = BTreeMap::new();
    let mut jumps = BTreeSet::new();
    for body in doc_bodies(doc) {
        visit(body, &mut |node| {
            if let Node::Directive(d) = node {
                if d.tag == lute_manifest::core::NEXT_DIRECTIVE {
                    if let Some((to, _)) = literal_attr(&d.attrs, "to") {
                        jumps.insert(to.to_string());
                    }
                }
            }
        });
    }
    // dsl 0.25.0 §6: occasion → the engine-`reserved` relations it may change.
    let mut changed_on: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (rel, decl) in &folded.env.rel_vocab.relations {
        if decl.reserved {
            for occasion in &decl.changed_on {
                changed_on
                    .entry(occasion.clone())
                    .or_default()
                    .insert(rel.clone());
            }
        }
    }
    let mut w = Presence {
        path,
        folded,
        facts: project.map(|p| p.env),
        producers: project.map(|p| p.producers),
        params: &params,
        defs: DefTable {
            bodies: &folded.def_bodies,
            params: &folded.env.def_params,
        },
        jumps,
        guards: Vec::new(),
        base: 0,
        quest: None,
        changed_on,
        changed: BTreeSet::new(),
        present: BTreeMap::new(),
        reads: BTreeMap::new(),
        out: Vec::new(),
    };
    let ladder_at = |at: Span| {
        project
            .and_then(|p| p.ladder.get(&at.byte_start))
            .map_or(&[][..], Vec::as_slice)
    };
    // The occasions unit `key` follows: its own (`on`), and — `check-project`
    // — every one a scenario-graph ancestor is presented on.
    let after = |key: usize, own: Option<&str>| -> Vec<String> {
        let graph = project
            .and_then(|p| p.after.get(&key))
            .into_iter()
            .flatten();
        own.into_iter()
            .map(str::to_string)
            .chain(graph.cloned())
            .collect()
    };

    let scene_ladder = match &folded.typed.beat {
        Some(_) => ladder_at(crate::beats::top_key_span(&doc.meta, "on")),
        None => &[],
    };
    let scene_when = w.slot_cond(folded.typed.beat.as_ref().and_then(|b| b.when.as_ref()));
    let scene_absent = w.absent_facts(
        0,
        folded
            .typed
            .beat
            .as_ref()
            .map_or(BeatOnce::None, |b| b.once),
    );
    let scene_after = after(0, folded.typed.beat.as_ref().map(|b| b.on.as_str()));
    w.unit(
        scene_when.into_iter().collect(),
        scene_ladder,
        scene_absent,
        &scene_after,
        doc.shots.iter().map(|s| &s.body[..]),
    );
    for quest in &doc.quests {
        let mut conds = Vec::new();
        if let Some((start, _)) = w.slot_cond(quest.start.as_ref()) {
            let mut cs = Vec::new();
            conjuncts(&start, &mut cs);
            conds.extend(
                cs.into_iter()
                    .filter_map(|c| stable_text(&c).map(|t| (c, t))),
            );
        }
        w.quest = (!quest.id.is_empty()).then(|| quest.id.clone());
        let quest_after = after(quest.span.byte_start, None);
        w.unit(
            conds,
            &[],
            Vec::new(),
            &quest_after,
            std::iter::once(&quest.body[..]),
        );
        w.quest = None;
    }
    for entry in &doc.entries {
        let mut conds: Vec<(Expr, String)> = w.slot_cond(entry.when.as_ref()).into_iter().collect();
        if !entry.id.is_empty() {
            conds.extend(w.parse(
                &format!("entry.{0}.everRead && entry.{0}.read", entry.id),
                None,
            ));
        }
        let ladder = entry.on.as_ref().map_or(&[][..], |(_, s)| ladder_at(*s));
        let once = match entry.once.as_ref().map(|(o, _)| o.as_str()) {
            Some("run") => BeatOnce::Run,
            Some("user") => BeatOnce::User,
            _ => BeatOnce::None,
        };
        let absent = w.absent_facts(entry.span.byte_start, once);
        let entry_after = after(
            entry.span.byte_start,
            entry.on.as_ref().map(|(o, _)| o.as_str()),
        );
        w.unit(
            conds,
            ladder,
            absent,
            &entry_after,
            std::iter::once(&entry.body[..]),
        );
    }
    for beat in &doc.beats {
        let conds = w.slot_cond(beat.when.as_ref()).into_iter().collect();
        let ladder = beat.on.as_ref().map_or(&[][..], |(_, s)| ladder_at(*s));
        let absent = w.absent_facts(beat.span.byte_start, crate::bundles::bundle_beat_once(beat));
        let beat_after = after(
            beat.span.byte_start,
            beat.on.as_ref().map(|(o, _)| o.as_str()),
        );
        w.unit(
            conds,
            ladder,
            absent,
            &beat_after,
            std::iter::once(&beat.body[..]),
        );
    }
    w.out
}

/// dsl 0.24.0 §4, `check-project`: what presence reads beyond one document
/// — the root's fact envelope, the document's beat-ladder assumptions
/// ([`crate::beats::presence_ladder`], keyed by a unit's `on` offset),
/// every `::assert` site of the root ([`fact_producers`]) and (dsl 0.25.0
/// §6) the occasions each unit of the document follows in the scenario
/// graph ([`occasions_before`], keyed like [`FactProducers`]' units).
pub struct PresenceProject<'a> {
    pub env: &'a FactEnv,
    pub ladder: &'a BTreeMap<usize, Vec<String>>,
    pub producers: &'a FactProducers,
    pub after: &'a BTreeMap<usize, BTreeSet<String>>,
}

/// dsl 0.25.0 §6 (D-D): per document, per unit (`0` for a scene's shots,
/// else the span start of the quest, entry or bundle beat — the
/// [`FactProducers`] keys), every occasion a unit presented on it precedes
/// the unit by: the unit is its after-descendant over `after:` / `after=`
/// and `[start]` edges of the scenario graph ([`crate::connectivity`]).
/// Accept anchors and subquest edges order nothing here. Only occasions
/// some relation of the root names in `changedOn:` are followed. `docs` and
/// `foldeds` are aligned (one resolved root).
pub fn occasions_before(
    docs: &[(std::path::PathBuf, Document)],
    foldeds: &[&FoldedEnv],
    graph: &crate::connectivity::ConnGraph,
) -> BTreeMap<std::path::PathBuf, BTreeMap<usize, BTreeSet<String>>> {
    use crate::connectivity::{EdgeKind, NodeId};
    let mut out: BTreeMap<std::path::PathBuf, BTreeMap<usize, BTreeSet<String>>> = BTreeMap::new();
    let named: BTreeSet<&str> = foldeds
        .iter()
        .flat_map(|f| f.env.rel_vocab.relations.values())
        .filter(|d| d.reserved)
        .flat_map(|d| d.changed_on.iter().map(String::as_str))
        .collect();
    if named.is_empty() {
        return out;
    }
    // A node's document index, unit key and the occasion it is presented on.
    let unit_of = |id: &NodeId| -> Option<(usize, usize, Option<&str>)> {
        let info = graph.nodes.get(id)?;
        let i = docs.iter().position(|(p, _)| *p == info.path)?;
        let doc = &docs[i].1;
        match id {
            NodeId::Scene(_) => Some((
                i,
                0,
                foldeds.get(i)?.typed.beat.as_ref().map(|b| b.on.as_str()),
            )),
            NodeId::Quest(q) => doc
                .quests
                .iter()
                .find(|x| x.id == *q)
                .map(|x| (i, x.span.byte_start, None)),
            NodeId::Beat(key) => {
                let doc_id = crate::connectivity::bundle_id(doc)?;
                doc.beats
                    .iter()
                    .find(|b| crate::bundles::bundle_beat_key(&doc_id, &b.id) == *key)
                    .map(|b| (i, b.span.byte_start, b.on.as_ref().map(|(o, _)| o.as_str())))
            }
            NodeId::Entry(e) => doc
                .entries
                .iter()
                .find(|x| x.id == *e)
                .map(|x| (i, x.span.byte_start, x.on.as_ref().map(|(o, _)| o.as_str()))),
        }
    };
    let orders = |from: &NodeId, to: &NodeId| {
        graph.edge_kinds_for(from, to).is_some_and(|ks| {
            ks.iter().any(|k| {
                matches!(
                    k,
                    EdgeKind::Visited | EdgeKind::Completed | EdgeKind::Active | EdgeKind::Start
                )
            })
        })
    };
    for id in graph.nodes.keys() {
        let Some(occasion) = unit_of(id)
            .and_then(|(_, _, on)| on)
            .filter(|o| named.contains(o))
        else {
            continue;
        };
        let mut seen = BTreeSet::new();
        let mut stack = vec![id];
        while let Some(from) = stack.pop() {
            for to in graph.edges.get(from).into_iter().flatten() {
                if orders(from, to) && seen.insert(to) {
                    stack.push(to);
                }
            }
        }
        for to in seen {
            if let Some((i, key, _)) = unit_of(to) {
                out.entry(docs[i].0.clone())
                    .or_default()
                    .entry(key)
                    .or_default()
                    .insert(occasion.to_string());
            }
        }
    }
    out
}

/// Every `::assert` site of one project root, by relation: its document,
/// its unit (`0` for a scene's shots, else the span start of the quest,
/// entry or bundle beat) and its arguments (`None` for anything but a
/// constant — a component param, `_`).
#[derive(Default)]
pub struct FactProducers(BTreeMap<String, Vec<(std::path::PathBuf, usize, Vec<Option<String>>)>>);

impl FactProducers {
    /// Every assert site of `relation`: document, unit, arguments.
    pub(crate) fn sites(
        &self,
        relation: &str,
    ) -> impl Iterator<Item = &(std::path::PathBuf, usize, Vec<Option<String>>)> {
        self.0.get(relation).into_iter().flatten()
    }
}

/// The [`FactProducers`] of `docs` (one resolved root). A component
/// document's own sites are not producers: its writes happen where a `::use`
/// performs them, bound — the host documents carry them spliced
/// ([`crate::component_effects::splice_component_effects`]) — so an unused
/// component produces nothing and a used one only its bound arguments.
pub fn fact_producers(docs: &[(std::path::PathBuf, Document)]) -> FactProducers {
    let mut out = FactProducers::default();
    for (path, doc) in docs {
        if crate::meta::infer_meta_kind_from_shape(&doc.meta, true)
            == Some(crate::meta::MetaKind::Component)
        {
            continue;
        }
        let units = std::iter::once((0, doc.shots.iter().map(|s| &s.body[..]).collect::<Vec<_>>()))
            .chain(
                doc.quests
                    .iter()
                    .map(|q| (q.span.byte_start, vec![&q.body[..]])),
            )
            .chain(
                doc.entries
                    .iter()
                    .map(|e| (e.span.byte_start, vec![&e.body[..]])),
            )
            .chain(
                doc.beats
                    .iter()
                    .map(|b| (b.span.byte_start, vec![&b.body[..]])),
            );
        for (key, bodies) in units {
            for body in bodies {
                visit(body, &mut |node| {
                    if let Node::Assert(a) = node {
                        if !a.pattern.relation.is_empty() {
                            out.0.entry(a.pattern.relation.clone()).or_default().push((
                                path.clone(),
                                key,
                                pattern_args(&a.pattern),
                            ));
                        }
                    }
                });
            }
        }
    }
    out
}

/// A fact pattern's arguments: a constant, else `None`.
fn pattern_args(pattern: &FactPattern) -> Vec<Option<String>> {
    pattern
        .args
        .iter()
        .map(|a| match &a.term {
            FactTerm::Ident(s) if s.starts_with(|c: char| c.is_ascii_alphabetic()) => {
                Some(s.clone())
            }
            FactTerm::Bool(b) => Some(b.to_string()),
            _ => None,
        })
        .collect()
}

/// Two argument lists that may name the same fact.
fn unifiable(a: &[Option<String>], b: &[Option<String>]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(x, y)| x.is_none() || y.is_none() || x == y)
}

/// One ground fact a unit of the root asserts ([`unit_facts`]).
pub(crate) struct UnitFact {
    /// `holds(rel(a, b))`.
    pub(crate) query: String,
    /// `None` when the fact cannot hold before the unit is spent: no other
    /// unit can assert it (no unifiable site elsewhere, a `::use` site's
    /// bound component writes included), no seed names it, and it cannot
    /// survive from an earlier presentation of the unit — a `tier: run`
    /// relation in a unit presented at most once per run (`once: run` or
    /// `user`), a `tier: user`/`app` one in a `once: user` unit; derived
    /// and engine-`reserved` relations never qualify. Otherwise why it may.
    pub(crate) persists: Option<String>,
}

/// Every ground fact unit `key` of document `path` asserts, with whether it
/// can hold while the unit (spent by `once`) is still unplayed — what
/// `W-CAST-ABSENT` assumes absent at the unit's start (dsl 0.24.0 §4) and
/// `W-BEAT-PRIORITY-TIE` conjoins to the unit's eligibility (dsl 0.26.0 §8).
pub(crate) fn unit_facts(
    producers: &FactProducers,
    vocab: &crate::rel_schema::RelVocab,
    path: &Path,
    key: usize,
    once: BeatOnce,
) -> Vec<UnitFact> {
    let mut out = Vec::new();
    let here = |p: &std::path::PathBuf, k: usize| p.as_path() == path && k == key;
    for (rel, sites) in &producers.0 {
        let Some(decl) = vocab.relations.get(rel) else {
            continue;
        };
        let tier = decl.tier.as_deref().unwrap_or("run");
        for (_, _, args) in sites.iter().filter(|(p, k, _)| here(p, *k)) {
            let Some(ground) = args.iter().cloned().collect::<Option<Vec<String>>>() else {
                continue;
            };
            let fact = format!("{rel}({})", ground.join(", "));
            let spent_by_run = matches!(once, BeatOnce::Run | BeatOnce::User);
            let persists = if decl.derive {
                Some(format!("rules may derive `{fact}` too"))
            } else if decl.reserved {
                Some(format!(
                    "the engine may assert `{fact}` (a reserved relation)"
                ))
            } else if !spent_by_run {
                Some(match once {
                    BeatOnce::None => {
                        format!("it is never spent, so it may play again after asserting `{fact}`")
                    }
                    period => format!(
                        "`once: {}` spends it for one period, not one run, so it may play again \
                         after asserting `{fact}`",
                        period.as_str()
                    ),
                })
            } else if matches!(tier, "user" | "app") && once != BeatOnce::User {
                Some(format!(
                    "`{fact}` is `tier: {tier}`, which persists across runs; `once: run` does \
                     not, so in a later run `{fact}` holds before it plays again"
                ))
            } else if !matches!(tier, "run" | "user" | "app") {
                Some(format!("`{fact}` is `tier: {tier}`"))
            } else if let Some((other, _, _)) = sites
                .iter()
                .find(|(p, k, a)| !here(p, *k) && unifiable(a, args))
            {
                Some(format!("`{fact}` is also asserted in {}", other.display()))
            } else if vocab
                .facts
                .iter()
                .any(|f| f.fact.relation == *rel && unifiable(&pattern_args(&f.fact), args))
            {
                Some(format!("`{fact}` is a `facts:` seed"))
            } else {
                None
            };
            out.push(UnitFact {
                query: format!("holds({fact})"),
                persists,
            });
        }
    }
    out
}

/// dsl 0.24.0 §4, `check-project`: re-decide one document's per-file
/// `W-CAST-ABSENT`s under the root's fact envelope and the document's
/// beat-ladder assumptions ([`crate::beats::presence_ladder`]) — a line the
/// Must set or the ladder shows present is dropped, the rest keep the
/// project's wording. Returns the lines only the project shows may be
/// absent (dsl 0.25.0 §6): those in a unit that follows, in the scenario
/// graph, an occasion on which the engine changes a relation `assume: true`
/// read as unchanged. Their spans carry byte offsets only; the caller
/// positions and adds them.
pub fn reconcile_presence(
    diags: &mut Vec<Diagnostic>,
    path: &Path,
    doc: &Document,
    folded: &FoldedEnv,
    project: &PresenceProject<'_>,
) -> Vec<Diagnostic> {
    if project.after.is_empty() && !diags.iter().any(|d| d.code == W_CAST_ABSENT) {
        return Vec::new();
    }
    let mut still: Vec<Diagnostic> = check_presence(path, doc, folded, Some(project))
        .into_iter()
        .filter(|d| d.code == W_CAST_ABSENT)
        .collect();
    let at = |d: &Diagnostic| (d.span.byte_start, d.span.byte_end);
    diags.retain_mut(|d| {
        if d.code != W_CAST_ABSENT {
            return true;
        }
        match still.iter().find(|s| at(s) == at(d)) {
            Some(s) => {
                d.message = s.message.clone();
                true
            }
            None => false,
        }
    });
    still.retain(|s| {
        !diags
            .iter()
            .any(|d| d.code == W_CAST_ABSENT && at(d) == at(s))
    });
    still
}

/// A `start` conjunct that stays true once it held — `entry.<id>.everRead`
/// or `visited('<id>')` — as its text; `None` for any other shape.
fn stable_text(e: &Expr) -> Option<String> {
    match e {
        Expr::Select(_) => {
            let path = select_text(e)?;
            crate::cel_paths::is_entry_ever_read(&path).then_some(path)
        }
        Expr::Call(c) if c.target.is_none() && c.func_name == crate::cel_resolve::VISITED_FN => {
            match c.args.as_slice() {
                [a] => match &a.expr {
                    Expr::Literal(Val::String(s)) => Some(format!("visited('{s}')")),
                    _ => None,
                },
                _ => None,
            }
        }
        _ => None,
    }
}

/// A dotted `a.b.c` path's text.
fn select_text(e: &Expr) -> Option<String> {
    match e {
        Expr::Ident(root) => Some(root.clone()),
        Expr::Select(sel) if !sel.test => {
            Some(format!("{}.{}", select_text(&sel.operand.expr)?, sel.field))
        }
        _ => None,
    }
}

/// Every state path `e` reads (the dotted prefix of an index read
/// included) onto `out`; `false` for a shape it cannot see into (a
/// comprehension, a map or struct literal).
fn read_paths(e: &Expr, out: &mut Vec<String>) -> bool {
    match e {
        Expr::Select(sel) => match select_text(e) {
            Some(p) => {
                out.push(p);
                true
            }
            None => read_paths(&sel.operand.expr, out),
        },
        Expr::Call(c) => {
            let mut ok = c.target.as_ref().is_none_or(|t| read_paths(&t.expr, out));
            for a in &c.args {
                ok &= read_paths(&a.expr, out);
            }
            ok
        }
        Expr::List(l) => l
            .elements
            .iter()
            .fold(true, |ok, x| read_paths(&x.expr, out) && ok),
        Expr::Ident(_) | Expr::Literal(_) => true,
        _ => false,
    }
}

/// How a fact atom sits in a condition: a write that can make it true
/// falsifies a `Neg` occurrence, one that can make it false a `Pos` one;
/// `Both` (under `count`, a comparison, a ternary…) either way.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Pol {
    Pos,
    Neg,
    Both,
}

impl Pol {
    fn flip(self) -> Self {
        match self {
            Pol::Pos => Pol::Neg,
            Pol::Neg => Pol::Pos,
            Pol::Both => Pol::Both,
        }
    }
}

/// A fact atom a guard queries: relation, arguments (`None` for `_` or
/// anything not a constant) and polarity.
struct Atom {
    rel: String,
    args: Vec<Option<String>>,
    pol: Pol,
}

/// What a write may do to the facts: atoms matching `rel(args)` (`None`
/// matches anything) may become true (`up`) or false.
#[derive(Clone, PartialEq)]
struct Effect {
    rel: String,
    args: Vec<Option<String>>,
    up: bool,
}

impl Effect {
    /// `self` already stands for every atom `other` names.
    fn covers(&self, other: &Effect) -> bool {
        self.rel == other.rel
            && self.up == other.up
            && self.args.len() == other.args.len()
            && self
                .args
                .iter()
                .zip(&other.args)
                .all(|(a, b)| a.is_none() || a == b)
    }
}

/// Past this many effects a write is taken to move every derived relation.
const EFFECT_CAP: usize = 256;

/// How deep a derived atom is read through its rules.
const EXPAND_DEPTH: u8 = 4;

/// A condition's disjunctive normal form past this many terms is not split.
const DNF_CAP: usize = 256;

/// An atom call's arguments: a constant identifier, string or bool, else
/// `None` (the `_` wildcard, a param, anything computed).
fn atom_args(atom: &CallExpr) -> Vec<Option<String>> {
    atom.args
        .iter()
        .map(|a| match &a.expr {
            Expr::Ident(n) if n != "_" && !n.starts_with(lute_cel::REF_MARKER) => Some(n.clone()),
            Expr::Literal(Val::String(s)) => Some(s.to_string()),
            Expr::Literal(Val::Boolean(b)) => Some(b.to_string()),
            _ => None,
        })
        .collect()
}

/// Every `holds`/`count`/`countDistinct` atom of `e`, with its polarity.
fn fact_atoms(e: &Expr, pol: Pol, out: &mut Vec<Atom>) {
    let Expr::Call(c) = e else { return };
    if c.target.is_none() {
        match (c.func_name.as_str(), c.args.as_slice()) {
            (n, [a]) if n == op::LOGICAL_NOT => return fact_atoms(&a.expr, pol.flip(), out),
            (n, [a, b]) if n == op::LOGICAL_AND || n == op::LOGICAL_OR => {
                fact_atoms(&a.expr, pol, out);
                return fact_atoms(&b.expr, pol, out);
            }
            ("holds" | "count" | "countDistinct", [first, ..]) => {
                if let Expr::Call(atom) = &first.expr {
                    if atom.target.is_none() {
                        out.push(Atom {
                            rel: atom.func_name.clone(),
                            args: atom_args(atom),
                            pol: if c.func_name == "holds" {
                                pol
                            } else {
                                Pol::Both
                            },
                        });
                    }
                }
                return;
            }
            _ => {}
        }
    }
    if let Some(t) = &c.target {
        fact_atoms(&t.expr, Pol::Both, out);
    }
    for a in &c.args {
        fact_atoms(&a.expr, Pol::Both, out);
    }
}

/// Bind rule `terms` against `args` (`None` matches anything) on top of `b`;
/// `None` when a constant or an already-bound variable disagrees.
fn bind(
    terms: &[RuleTerm],
    args: &[Option<String>],
    mut b: BTreeMap<String, Option<String>>,
) -> Option<BTreeMap<String, Option<String>>> {
    for (t, a) in terms.iter().zip(args) {
        match t {
            RuleTerm::Const(c) => {
                if a.as_ref().is_some_and(|a| a != c) {
                    return None;
                }
            }
            RuleTerm::Bool(x) => {
                if a.as_ref().is_some_and(|a| *a != x.to_string()) {
                    return None;
                }
            }
            RuleTerm::Var(v) => match b.get(v) {
                Some(Some(prev)) => {
                    if a.as_ref().is_some_and(|a| a != prev) {
                        return None;
                    }
                }
                _ => {
                    b.insert(v.clone(), a.clone());
                }
            },
        }
    }
    Some(b)
}

/// Everything asserting (`up`) or retracting `rel(args)` may move: the atom
/// itself, then every rule head with a premise it unifies with — in the
/// write's direction through a positive premise, against it through a
/// negated one — to a fixpoint. A relation whose rules did not parse moves
/// either way on any write.
fn write_effects(vocab: &RelVocab, rel: &str, args: Vec<Option<String>>, up: bool) -> Vec<Effect> {
    let mut out = vec![Effect {
        rel: rel.to_string(),
        args,
        up,
    }];
    let everything = |out: &mut Vec<Effect>, names: &mut dyn Iterator<Item = &String>| {
        for name in names {
            let arity = vocab.relations.get(name).map_or(0, |d| d.args.len());
            for up in [true, false] {
                out.push(Effect {
                    rel: name.clone(),
                    args: vec![None; arity],
                    up,
                });
            }
        }
    };
    let mut i = 0;
    while i < out.len() {
        let e = out[i].clone();
        i += 1;
        for r in &vocab.rules {
            let rule = &r.rule;
            for lit in &rule.body {
                let (atom, positive) = match lit {
                    BodyLiteral::Pos(a) => (a, true),
                    BodyLiteral::Neg(a) => (a, false),
                    _ => continue,
                };
                if atom.relation != e.rel || atom.terms.len() != e.args.len() {
                    continue;
                }
                let Some(b) = bind(&atom.terms, &e.args, BTreeMap::new()) else {
                    continue;
                };
                let head = rule
                    .head
                    .terms
                    .iter()
                    .map(|t| match t {
                        RuleTerm::Var(v) => b.get(v).cloned().flatten(),
                        RuleTerm::Const(c) => Some(c.clone()),
                        RuleTerm::Bool(x) => Some(x.to_string()),
                    })
                    .collect();
                let next = Effect {
                    rel: rule.head.relation.clone(),
                    args: head,
                    up: e.up == positive,
                };
                if !out.iter().any(|o| o.covers(&next)) {
                    out.push(next);
                }
            }
        }
        if out.len() > EFFECT_CAP {
            let derived: Vec<String> = vocab
                .relations
                .iter()
                .filter(|(_, d)| d.derive)
                .map(|(n, _)| n.clone())
                .collect();
            everything(&mut out, &mut derived.iter());
            break;
        }
    }
    everything(&mut out, &mut vocab.unparsed_heads.iter());
    out
}

/// A guard one of `effects` may falsify: an atom it queries unifies with
/// the effect's and the effect moves it the wrong way. A relation named in
/// the guard's text but not found as an atom (an unusual shape) counts.
fn affected(effects: &[Effect], g: &Guard) -> bool {
    effects.iter().any(|e| {
        let mut named = false;
        let hit = g.atoms.iter().any(|a| {
            if a.rel != e.rel {
                return false;
            }
            named = true;
            a.args.len() == e.args.len()
                && a.args
                    .iter()
                    .zip(&e.args)
                    .all(|(x, y)| x.is_none() || y.is_none() || x == y)
                && match a.pol {
                    Pol::Both => true,
                    Pol::Pos => !e.up,
                    Pol::Neg => e.up,
                }
        });
        hit || (!named && g.text.contains(&format!("{}(", e.rel)))
    })
}

/// A derived atom read through its rules: the disjuncts (one per unifying
/// rule, the conjunction of its body literals with the head bound), whether
/// that disjunction is EXACTLY the atom (every literal kept), and the rule
/// `cel()` texts it reads.
struct Definition {
    disjuncts: Vec<String>,
    exact: bool,
    cels: Vec<String>,
}

/// `rel(args)` through its rules — `None` unless `rel` is derived, not
/// engine-`reserved`, has parsed rules, no seed fact unifies with the atom,
/// and some rule's head does. A literal left non-ground by the binding is
/// dropped (inexact) — except a positive atom's variable used nowhere else
/// and an anonymous `_`, which read as `_`. An entity-kind premise and a
/// ground comparison decide statically; a `cel()` guard is kept only in a
/// rule without variables (it may read one).
fn definition(vocab: &RelVocab, rel: &str, args: &[Option<String>]) -> Option<Definition> {
    let decl = vocab.relations.get(rel)?;
    if !decl.derive || decl.reserved || vocab.unparsed_heads.contains(rel) {
        return None;
    }
    let seeded = vocab.facts.iter().any(|f| {
        f.fact.relation == rel
            && f.fact.args.len() == args.len()
            && f.fact
                .args
                .iter()
                .zip(args)
                .all(|(a, k)| match (&a.term, k) {
                    (FactTerm::Ident(i), Some(k)) => i == k,
                    (FactTerm::Bool(b), Some(k)) => b.to_string() == *k,
                    _ => true,
                })
    });
    if seeded {
        return None;
    }
    let is_var = |t: &RuleTerm| matches!(t, RuleTerm::Var(_));
    let mut def = Definition {
        disjuncts: Vec::new(),
        exact: true,
        cels: Vec::new(),
    };
    let mut unified = false;
    for r in vocab.rules.iter() {
        let rule = &r.rule;
        if rule.head.relation != rel || rule.head.terms.len() != args.len() {
            continue;
        }
        let Some(b) = bind(&rule.head.terms, args, BTreeMap::new()) else {
            continue;
        };
        unified = true;
        let mut uses: BTreeMap<&str, usize> = BTreeMap::new();
        let mut has_vars = rule.head.terms.iter().any(is_var);
        for lit in &rule.body {
            let terms: Vec<&RuleTerm> = match lit {
                BodyLiteral::Pos(a) | BodyLiteral::Neg(a) | BodyLiteral::Count { atom: a, .. } => {
                    a.terms.iter().collect()
                }
                BodyLiteral::Cmp { lhs, rhs, .. } => vec![lhs, rhs],
                BodyLiteral::Guard { .. } => Vec::new(),
            };
            for t in terms {
                if let RuleTerm::Var(v) = t {
                    has_vars = true;
                    *uses.entry(v.as_str()).or_default() += 1;
                }
            }
        }
        let value = |t: &RuleTerm| -> Option<String> {
            match t {
                RuleTerm::Const(c) => Some(c.clone()),
                RuleTerm::Bool(x) => Some(x.to_string()),
                RuleTerm::Var(v) => b.get(v).cloned().flatten(),
            }
        };
        let mut conj = Vec::new();
        let mut dead = false;
        for lit in &rule.body {
            match lit {
                BodyLiteral::Pos(a) | BodyLiteral::Neg(a) => {
                    let neg = matches!(lit, BodyLiteral::Neg(_));
                    if !vocab.relations.contains_key(&a.relation) {
                        // An entity-kind premise: static membership.
                        let member = match (
                            a.terms.as_slice(),
                            vocab.kinds.get(&a.relation).map(|k| &k.shape),
                        ) {
                            ([t], Some(KindShape::Members(ms))) => {
                                value(t).map(|v| ms.contains(&v))
                            }
                            _ => None,
                        };
                        match member {
                            Some(m) if m != neg => {}
                            Some(_) => {
                                dead = true;
                                break;
                            }
                            None => def.exact = false,
                        }
                        continue;
                    }
                    let mut ground = true;
                    let mut texts = Vec::with_capacity(a.terms.len());
                    for t in &a.terms {
                        texts.push(value(t).unwrap_or_else(|| {
                            if let RuleTerm::Var(v) = t {
                                let lone = !neg && uses.get(v.as_str()) == Some(&1);
                                if !(is_anonymous_var(v) || lone) {
                                    ground = false;
                                }
                            }
                            "_".to_string()
                        }));
                    }
                    if !ground {
                        def.exact = false;
                        // `!holds(r(_))` would claim more than the rule does.
                        if neg {
                            continue;
                        }
                    }
                    conj.push(format!(
                        "{}holds({}({}))",
                        if neg { "!" } else { "" },
                        a.relation,
                        texts.join(", ")
                    ));
                }
                BodyLiteral::Guard { cel, .. } => {
                    def.cels.push(cel.clone());
                    if has_vars {
                        def.exact = false;
                    } else {
                        conj.push(format!("({cel})"));
                    }
                }
                BodyLiteral::Cmp {
                    lhs, rhs, negated, ..
                } => match (value(lhs), value(rhs)) {
                    (Some(l), Some(r)) => {
                        if (l == r) == *negated {
                            dead = true;
                            break;
                        }
                    }
                    _ => def.exact = false,
                },
                // dsl 0.26.0 §6: the same count as a condition, when every
                // argument is ground or the count's own (`_`, or the one
                // counted variable).
                BodyLiteral::Count {
                    atom,
                    distinct,
                    op,
                    n,
                    ..
                } => {
                    let mut texts = Vec::with_capacity(atom.terms.len());
                    let mut ground = distinct.len() <= 1;
                    for t in &atom.terms {
                        texts.push(match (value(t), t) {
                            (Some(v), _) => v,
                            (None, RuleTerm::Var(v)) if distinct.contains(v) => v.clone(),
                            (None, RuleTerm::Var(v))
                                if is_anonymous_var(v) || uses.get(v.as_str()) == Some(&1) =>
                            {
                                "_".to_string()
                            }
                            _ => {
                                ground = false;
                                "_".to_string()
                            }
                        });
                    }
                    if !ground {
                        def.exact = false;
                        continue;
                    }
                    let call = match distinct.first() {
                        Some(v) => format!(
                            "countDistinct({}({}), {v})",
                            atom.relation,
                            texts.join(", ")
                        ),
                        None => format!("count({}({}))", atom.relation, texts.join(", ")),
                    };
                    conj.push(format!("{call} {} {n}", op.as_str()));
                }
            }
        }
        if !dead {
            def.disjuncts.push(if conj.is_empty() {
                "true".to_string()
            } else {
                conj.join(" && ")
            });
        }
    }
    unified.then_some(def)
}

/// Which side of the implication a condition is on.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
    /// An enclosing guard: a derived atom may add what it implies.
    Guard,
    /// `present`: only an exact rewrite; `assume` reads a negated reserved
    /// atom as true.
    Present { assume: bool },
}

/// `e`'s disjunctive normal form (of `!e` when `neg`) as literal
/// conjunctions; `None` past [`DNF_CAP`] terms.
fn dnf(e: &Expr, neg: bool) -> Option<Vec<Vec<Expr>>> {
    if let Expr::Call(c) = e {
        if c.target.is_none() {
            match (c.func_name.as_str(), c.args.as_slice()) {
                (n, [a]) if n == op::LOGICAL_NOT => return dnf(&a.expr, !neg),
                (n, [a, b]) if n == op::LOGICAL_AND || n == op::LOGICAL_OR => {
                    let l = dnf(&a.expr, neg)?;
                    let r = dnf(&b.expr, neg)?;
                    if (n == op::LOGICAL_AND) != neg {
                        if l.len() * r.len() > DNF_CAP {
                            return None;
                        }
                        return Some(
                            l.iter()
                                .flat_map(|x| {
                                    r.iter().map(move |y| x.iter().chain(y).cloned().collect())
                                })
                                .collect(),
                        );
                    }
                    if l.len() + r.len() > DNF_CAP {
                        return None;
                    }
                    let mut out = l;
                    out.extend(r);
                    return Some(out);
                }
                _ => {}
            }
        }
    }
    if let Expr::Literal(Val::Boolean(b)) = e {
        return Some(if *b != neg {
            vec![Vec::new()]
        } else {
            Vec::new()
        });
    }
    let lit = if neg {
        call(op::LOGICAL_NOT, vec![e.clone()])
    } else {
        e.clone()
    };
    Some(vec![vec![lit]])
}

/// `a && (b && (…))`.
fn conjoin(items: &[Expr]) -> Option<Expr> {
    let (last, rest) = items.split_last()?;
    Some(rest.iter().rev().fold(last.clone(), |acc, x| {
        call(op::LOGICAL_AND, vec![x.clone(), acc])
    }))
}

/// `e` provably false: decided `false` whole, or every disjunct of its
/// normal form is.
fn refuted(e: &Expr, ctx: &DecideCtx<'_>) -> bool {
    if decide(e, ctx) == Some(Decided::Bool(false)) {
        return true;
    }
    let Some(terms) = dnf(e, false) else {
        return false;
    };
    terms
        .iter()
        .all(|t| conjoin(t).is_some_and(|c| decide(&c, ctx) == Some(Decided::Bool(false))))
}

/// One enclosing guard: its parsed condition (derived atoms read through
/// their rules), expanded text (rule `cel()`s it reads appended), and fact
/// atoms; `live` turns false once a write may have falsified it.
struct Guard {
    expr: Expr,
    text: String,
    atoms: Vec<Atom>,
    live: bool,
}

/// A speaker's `present`: each top-level conjunct as written and read
/// through the rules (exact rewrites only).
type PresentConjuncts = Vec<(Expr, Expr)>;

struct Presence<'a> {
    path: &'a Path,
    folded: &'a FoldedEnv,
    facts: Option<&'a FactEnv>,
    /// `check-project`: every `::assert` site of the root.
    producers: Option<&'a FactProducers>,
    params: &'a BTreeMap<String, DomainInfo>,
    defs: DefTable<'a>,
    /// Every `::next{to}` target of the document.
    jumps: BTreeSet<String>,
    guards: Vec<Guard>,
    /// `guards[..base]` are the current unit's own assumptions.
    base: usize,
    /// The quest whose body is being walked.
    quest: Option<String>,
    /// dsl 0.25.0 §6: occasion → the engine-`reserved` relations whose
    /// `changedOn:` names it.
    changed_on: BTreeMap<String, BTreeSet<String>>,
    /// The `changedOn` relations the engine may have changed before the
    /// code being walked: `assume: true` no longer reads them as unchanged.
    changed: BTreeSet<String>,
    /// Speaker → `present`'s conjuncts as written; `None` once reported
    /// unparseable.
    present: BTreeMap<String, Option<Vec<Expr>>>,
    /// (speaker, [`Self::changed`] as it applies to the speaker) →
    /// `present`'s conjuncts, each with its rule-read form.
    reads: BTreeMap<(String, BTreeSet<String>), PresentConjuncts>,
    out: Vec<Diagnostic>,
}

/// The top-level `&&` conjuncts of `e`.
fn conjuncts(e: &Expr, out: &mut Vec<Expr>) {
    if let Expr::Call(c) = e {
        if c.func_name == op::LOGICAL_AND && c.target.is_none() && c.args.len() == 2 {
            conjuncts(&c.args[0].expr, out);
            conjuncts(&c.args[1].expr, out);
            return;
        }
    }
    out.push(e.clone());
}

fn call(name: &str, args: Vec<Expr>) -> Expr {
    Expr::Call(CallExpr {
        func_name: name.to_string(),
        target: None,
        args: args
            .into_iter()
            .map(|expr| IdedExpr { id: 0, expr })
            .collect(),
    })
}

/// A decimal as CEL literal text (`3`, `2.5`, `-1`).
fn num_text(n: f64) -> String {
    format!("{n}")
}

/// A `<when is="…">` pattern as a condition over the subject text `s` —
/// its alternatives joined by `||`; `None` when any alternative does not
/// classify (the arm then assumes nothing).
fn is_condition(raw: &str, s: &str, path: Option<&str>) -> Option<String> {
    let mut alts = Vec::new();
    for lit in is_alternatives(raw) {
        let lit = crate::match_check::quest_state_is_literal(classify_is_literal(lit).ok()?, path);
        alts.push(match lit {
            IsLiteral::Bool(b) => format!("{s} == {b}"),
            IsLiteral::Unset => format!("!isSet({s})"),
            IsLiteral::Str(v) => format!("{s} == '{v}'"),
            IsLiteral::Num(n) => format!("{s} == {}", num_text(n)),
            IsLiteral::Range(r) => match (r.lo, r.hi) {
                (Some(lo), Some(hi)) => {
                    format!("{s} >= {} && {s} <= {}", num_text(lo), num_text(hi))
                }
                (Some(lo), None) => format!("{s} >= {}", num_text(lo)),
                (None, Some(hi)) => format!("{s} <= {}", num_text(hi)),
                (None, None) => return None,
            },
        });
    }
    (!alts.is_empty()).then(|| {
        alts.iter()
            .map(|a| format!("({a})"))
            .collect::<Vec<_>>()
            .join(" || ")
    })
}

impl Presence<'_> {
    /// `raw` with its `@def`s (and `$`, as `subject`) expanded.
    fn expand_text(&self, raw: &str, subject: Option<&str>) -> String {
        let mut stack = Vec::new();
        expand_cel(raw, &self.defs, subject, &mut stack).unwrap_or_else(|_| raw.to_string())
    }

    /// `raw` with its `@def`s (and `$`, as `subject`) expanded, and parsed.
    fn parse(&self, raw: &str, subject: Option<&str>) -> Option<(Expr, String)> {
        if raw.trim().is_empty() {
            return None;
        }
        let text = self.expand_text(raw, subject);
        let mut arena = lute_cel::CelArena::default();
        let handle = lute_cel::parse_slot_marked_refs(&mut arena, &text)?;
        Some((arena.get(handle)?.expr.clone(), text))
    }

    fn slot_cond(&self, slot: Option<&CelSlot>) -> Option<(Expr, String)> {
        slot.and_then(|s| self.parse(&s.raw, None))
    }

    /// `e` with each `holds(A)` of a derived relation reached through
    /// `!`/`&&`/`||` read through its rules ([`definition`]): on the guard
    /// side a positive `holds(A)` becomes `holds(A) && D` (what it implies)
    /// and, when `D` is exact, a negative one `holds(A) || D`; on the
    /// `present` side an exact `D` replaces it, and under `assume` a
    /// negative `holds` of an engine-`reserved` relation reads `false` —
    /// unless (dsl 0.25.0 §6) the relation is in [`Self::changed`].
    /// The rule `cel()`s read are pushed onto `cels`.
    fn expand(&self, e: &Expr, pol: Pol, side: Side, depth: u8, cels: &mut Vec<String>) -> Expr {
        let Expr::Call(c) = e else { return e.clone() };
        if c.target.is_some() {
            return e.clone();
        }
        match (c.func_name.as_str(), c.args.as_slice()) {
            (n, [a]) if n == op::LOGICAL_NOT => {
                call(n, vec![self.expand(&a.expr, pol.flip(), side, depth, cels)])
            }
            (n, [a, b]) if n == op::LOGICAL_AND || n == op::LOGICAL_OR => call(
                n,
                vec![
                    self.expand(&a.expr, pol, side, depth, cels),
                    self.expand(&b.expr, pol, side, depth, cels),
                ],
            ),
            ("holds", [a]) => {
                let Expr::Call(atom) = &a.expr else {
                    return e.clone();
                };
                if atom.target.is_some() {
                    return e.clone();
                }
                let vocab = &self.folded.env.rel_vocab;
                if side == (Side::Present { assume: true })
                    && pol == Pol::Neg
                    && vocab
                        .relations
                        .get(&atom.func_name)
                        .is_some_and(|d| d.reserved)
                    && !self.changed.contains(&atom.func_name)
                {
                    return Expr::Literal(Val::Boolean(false));
                }
                if depth == 0 || pol == Pol::Both {
                    return e.clone();
                }
                let Some(def) = definition(vocab, &atom.func_name, &atom_args(atom)) else {
                    return e.clone();
                };
                cels.extend(def.cels.iter().map(|c| self.expand_text(c, None)));
                let text = if def.disjuncts.is_empty() {
                    "false".to_string()
                } else {
                    def.disjuncts
                        .iter()
                        .map(|d| format!("({d})"))
                        .collect::<Vec<_>>()
                        .join(" || ")
                };
                let Some((d, _)) = self.parse(&text, None) else {
                    return e.clone();
                };
                let d = self.expand(&d, pol, side, depth - 1, cels);
                match (side, pol, def.exact) {
                    (Side::Guard, Pol::Pos, _) => call(op::LOGICAL_AND, vec![e.clone(), d]),
                    (Side::Guard, Pol::Neg, true) => call(op::LOGICAL_OR, vec![e.clone(), d]),
                    (Side::Present { .. }, _, true) => d,
                    _ => e.clone(),
                }
            }
            _ => e.clone(),
        }
    }

    /// Push `cond` as one guard per top-level conjunct, so a write that
    /// may falsify one conjunct leaves the others standing.
    fn push(&mut self, cond: Option<(Expr, String)>) {
        let Some((expr, text)) = cond else { return };
        let mut parts = Vec::new();
        conjuncts(&expr, &mut parts);
        for part in parts {
            let mut paths = Vec::new();
            let mut text = if read_paths(&part, &mut paths) {
                paths.join(" ")
            } else {
                text.clone()
            };
            let mut cels = Vec::new();
            let expr = self.expand(&part, Pol::Pos, Side::Guard, EXPAND_DEPTH, &mut cels);
            for cel in cels {
                text.push_str(" ; ");
                text.push_str(&cel);
            }
            let mut atoms = Vec::new();
            fact_atoms(&expr, Pol::Pos, &mut atoms);
            self.guards.push(Guard {
                expr,
                text,
                atoms,
                live: true,
            });
        }
    }

    /// One unit (a scene's shots, a quest, an entry, a bundle beat) under
    /// its own assumptions and the ladder's; `absent` ([`Self::absent_facts`])
    /// holds at its start, as path state the body's writes may end. `after`
    /// are the occasions the unit follows (dsl 0.25.0 §6): the relations they
    /// change are no longer assumed unchanged — nor are those its own
    /// assumptions require a fact of ([`Self::required`]).
    fn unit<'n>(
        &mut self,
        conds: Vec<(Expr, String)>,
        ladder: &[String],
        absent: Vec<String>,
        after: &[String],
        bodies: impl Iterator<Item = &'n [Node]>,
    ) {
        self.guards.clear();
        self.changed.clear();
        for occasion in after {
            self.follow(occasion);
        }
        for c in conds {
            let required = self.required(&c.0, EXPAND_DEPTH);
            self.changed.extend(required);
            self.push(Some(c));
        }
        for l in ladder {
            let c = self.parse(l, None);
            self.push(c);
        }
        self.base = self.guards.len();
        for a in absent {
            let c = self.parse(&a, None);
            self.push(c);
        }
        for body in bodies {
            self.walk(body);
        }
    }

    /// dsl 0.25.0 §6: the code walked from here on follows `occasion`, so the
    /// relations it changes join [`Self::changed`]. Returns the set before.
    fn follow(&mut self, occasion: &str) -> BTreeSet<String> {
        let before = self.changed.clone();
        if let Some(rels) = self.changed_on.get(occasion) {
            self.changed.extend(rels.iter().cloned());
        }
        before
    }

    /// dsl 0.25.0 §6: the `changedOn` relations `e` cannot hold without a
    /// fact of — a positive `holds(R(…))`, a `count(R(…))` compared to be at
    /// least one, or a derived relation every rule of which needs one, joined
    /// through `&&` (either side) and `||` (both sides). Such a fact is the
    /// engine's, written on one of R's occasions, so code guarded by `e` runs
    /// after that occasion and `assume: true` no longer covers R there.
    fn required(&self, e: &Expr, depth: u8) -> BTreeSet<String> {
        let Expr::Call(c) = e else {
            return BTreeSet::new();
        };
        if c.target.is_some() {
            return BTreeSet::new();
        }
        let atom_rel = |x: &Expr| match x {
            Expr::Call(f)
                if f.target.is_none()
                    && matches!(f.func_name.as_str(), "count" | "countDistinct") =>
            {
                match f.args.as_slice() {
                    [a] => match &a.expr {
                        Expr::Call(atom) if atom.target.is_none() => Some(atom.func_name.clone()),
                        _ => None,
                    },
                    _ => None,
                }
            }
            _ => None,
        };
        let num = |x: &Expr| match x {
            Expr::Literal(Val::Int(i)) => Some(*i as f64),
            Expr::Literal(Val::UInt(u)) => Some(*u as f64),
            Expr::Literal(Val::Double(d)) => Some(*d),
            _ => None,
        };
        let n = c.func_name.as_str();
        match c.args.as_slice() {
            [a, b] if n == op::LOGICAL_AND => {
                let mut out = self.required(&a.expr, depth);
                out.extend(self.required(&b.expr, depth));
                out
            }
            [a, b] if n == op::LOGICAL_OR => {
                let left = self.required(&a.expr, depth);
                let right = self.required(&b.expr, depth);
                left.intersection(&right).cloned().collect()
            }
            [a] if n == "holds" => match &a.expr {
                Expr::Call(atom) if atom.target.is_none() => {
                    self.required_rel(&atom.func_name, depth)
                }
                _ => BTreeSet::new(),
            },
            [a, b] => {
                // `count(R) >= k` (k ≥ 1), `> k` (k ≥ 0), `== k` (k ≥ 1), either way round.
                let (rel, k, cmp) = match (
                    atom_rel(&a.expr),
                    num(&b.expr),
                    atom_rel(&b.expr),
                    num(&a.expr),
                ) {
                    (Some(r), Some(k), _, _) => (r, k, n.to_string()),
                    (_, _, Some(r), Some(k)) => {
                        let flipped = if n == op::LESS_EQUALS {
                            op::GREATER_EQUALS
                        } else if n == op::LESS {
                            op::GREATER
                        } else {
                            n
                        };
                        (r, k, flipped.to_string())
                    }
                    _ => return BTreeSet::new(),
                };
                let at_least_one = (cmp == op::GREATER_EQUALS && k >= 1.0)
                    || (cmp == op::GREATER && k >= 0.0)
                    || (cmp == op::EQUALS && k >= 1.0);
                if at_least_one {
                    self.required_rel(&rel, depth)
                } else {
                    BTreeSet::new()
                }
            }
            _ => BTreeSet::new(),
        }
    }

    /// [`Self::required`] for one relation's fact: the relation itself when
    /// some occasion changes it; for a derived relation without seed facts,
    /// what every one of its rules' positive premises requires.
    fn required_rel(&self, rel: &str, depth: u8) -> BTreeSet<String> {
        if self.changed_on.values().any(|rels| rels.contains(rel)) {
            return BTreeSet::from([rel.to_string()]);
        }
        let vocab = &self.folded.env.rel_vocab;
        let derived = vocab
            .relations
            .get(rel)
            .is_some_and(|d| d.derive && !d.reserved);
        if depth == 0
            || !derived
            || vocab.unparsed_heads.contains(rel)
            || vocab.facts.iter().any(|f| f.fact.relation == rel)
        {
            return BTreeSet::new();
        }
        let mut out: Option<BTreeSet<String>> = None;
        for r in vocab.rules.iter().filter(|r| r.rule.head.relation == rel) {
            let mut needs = BTreeSet::new();
            for lit in &r.rule.body {
                if let BodyLiteral::Pos(a) = lit {
                    needs.extend(self.required_rel(&a.relation, depth - 1));
                }
            }
            out = Some(match out {
                None => needs,
                Some(acc) => acc.intersection(&needs).cloned().collect(),
            });
        }
        out.unwrap_or_default()
    }

    /// `check-project`: the facts known absent when unit `key` of this
    /// document starts, as `!holds(F)` ([`unit_facts`]).
    fn absent_facts(&self, key: usize, once: BeatOnce) -> Vec<String> {
        let Some(producers) = self.producers else {
            return Vec::new();
        };
        let vocab = &self.folded.env.rel_vocab;
        let out: BTreeSet<String> = unit_facts(producers, vocab, self.path, key, once)
            .into_iter()
            .filter(|f| f.persists.is_none())
            .map(|f| format!("!{}", f.query))
            .collect();
        out.into_iter().collect()
    }

    /// Walk `body` under `conds` added to the guards pushed so far; they
    /// are popped when it ends. A write inside still reaches the outer
    /// guards: the region runs before what follows it.
    fn region(&mut self, conds: Vec<(Expr, String)>, body: &[Node]) {
        let depth = self.guards.len();
        let before = self.changed.clone();
        for c in conds {
            let required = self.required(&c.0, EXPAND_DEPTH);
            self.changed.extend(required);
            self.push(Some(c));
        }
        self.walk(body);
        self.guards.truncate(depth);
        self.changed = before;
    }

    /// Alternative paths (a branch's choices, a match's arms): each starts
    /// from the guards as they are here, and an outer guard is still live
    /// after them only when it is live at the end of every one.
    fn fork(&mut self, arms: Vec<(Vec<(Expr, String)>, &[Node])>) {
        let saved: Vec<bool> = self.guards.iter().map(|g| g.live).collect();
        let mut joined = saved.clone();
        for (conds, body) in arms {
            for (g, &l) in self.guards.iter_mut().zip(&saved) {
                g.live = l;
            }
            self.region(conds, body);
            for (j, g) in joined.iter_mut().zip(&self.guards) {
                *j &= g.live;
            }
        }
        for (g, l) in self.guards.iter_mut().zip(joined) {
            g.live = l;
        }
    }

    fn choice_conds(&self, choices: &[lute_syntax::ast::Choice]) -> Vec<Vec<(Expr, String)>> {
        choices
            .iter()
            .map(|c| {
                c.when
                    .as_ref()
                    .and_then(|w| self.parse(&w.raw, None))
                    .into_iter()
                    .collect()
            })
            .collect()
    }

    fn walk(&mut self, nodes: &[Node]) {
        for node in nodes {
            match node {
                Node::Line(l) => {
                    if let Some((id, _)) = literal_attr(&l.attrs, "id") {
                        self.label(id);
                    }
                    self.line(l);
                }
                Node::Directive(d) => self.directive(d),
                Node::Set(s) => self.kill_path(&s.path),
                Node::Timeline(t) => {
                    for clip in t.tracks.iter().flat_map(|tr| &tr.clips) {
                        match &clip.node {
                            ClipNode::Set(s) => self.kill_path(&s.path),
                            ClipNode::Directive(d) => self.directive(d),
                        }
                    }
                }
                Node::Assert(a) => self.kill_fact(&a.pattern, true),
                Node::Retract(r) => self.kill_fact(&r.pattern, false),
                Node::Branch(b) => {
                    let conds = self.choice_conds(&b.choices);
                    self.fork(
                        conds
                            .into_iter()
                            .zip(b.choices.iter().map(|c| &c.body[..]))
                            .collect(),
                    );
                }
                Node::Hub(h) => {
                    // A hub body runs again and again: whatever one round
                    // writes, the next round's guards may not survive.
                    for c in &h.choices {
                        self.kill_writes(&c.body);
                    }
                    let conds = self.choice_conds(&h.choices);
                    self.fork(
                        conds
                            .into_iter()
                            .zip(h.choices.iter().map(|c| &c.body[..]))
                            .collect(),
                    );
                }
                Node::Match(m) => self.match_arms(m),
                Node::On(o) => {
                    let mut conds = Vec::new();
                    if let Some(q) = &self.quest {
                        let state = match o.event.as_str() {
                            "questComplete" => "complete",
                            "questFailed" => "failed",
                            _ => "active",
                        };
                        conds.extend(self.parse(&format!("quest.{q}.state == '{state}'"), None));
                    }
                    conds.extend(o.when.as_ref().and_then(|w| self.parse(&w.raw, None)));
                    // Raising an occasion fires the same-named event: the
                    // handler runs on it (dsl 0.25.0 §6).
                    let before = self.follow(&o.event);
                    self.region(conds, &o.body);
                    self.changed = before;
                }
                Node::Objective(o) => {
                    let cond = self.parse(&o.done.raw, None);
                    // An `on=` objective completes when its occasion judges it.
                    let before = o.on.as_ref().map(|(on, _)| self.follow(on));
                    self.region(cond.into_iter().collect(), &o.body);
                    if let Some(before) = before {
                        self.changed = before;
                    }
                }
            }
        }
    }

    fn directive(&mut self, d: &Directive) {
        use lute_manifest::core::MARK_DIRECTIVE;
        if d.tag == MARK_DIRECTIVE {
            if let Some((id, _)) = literal_attr(&d.attrs, "id") {
                self.label(id);
            }
        } else if let Some((quest, _)) = d.accept_quest() {
            self.kill_path(&format!("quest.{quest}"));
        } else if d.tag == "use" {
            self.use_lines(d);
        }
    }

    /// dsl 0.26.0 §3.2: the `@@p:` lines a `::use` speaks, each by the
    /// member it binds, under the `::use`'s guard and the line's own —
    /// reported at the `::use`, once per member and guard.
    fn use_lines(&mut self, d: &Directive) {
        let folded = self.folded;
        let Some(lines) = folded.use_lines.get(&d.span.byte_start) else {
            return;
        };
        let component = literal_attr(&d.attrs, "component").map(|(c, _)| c);
        let mut seen = std::collections::BTreeSet::new();
        for l in lines {
            if l.attrs.iter().any(|a| a.key == "vo") {
                continue;
            }
            let raw = match (&d.when, &l.when) {
                (Some(g), Some(a)) => Some(format!("({}) && ({})", g.raw, a.raw)),
                (Some(g), None) => Some(g.raw.clone()),
                (None, Some(a)) => Some(a.raw.clone()),
                (None, None) => None,
            };
            if !seen.insert((l.speaker.as_str(), raw.clone())) {
                continue;
            }
            let own = raw.and_then(|r| self.parse(&r, None));
            let before = self.changed.clone();
            if let Some((e, _)) = &own {
                let required = self.required(e, EXPAND_DEPTH);
                self.changed.extend(required);
            }
            self.decide_line(l, d.span, own, component);
            self.changed = before;
        }
    }

    /// Each arm assumes its own pattern and that no earlier arm matched.
    fn match_arms(&mut self, m: &Match) {
        let subject = self.expand_text(&m.subject.raw, None);
        let path = crate::match_check::subject_path(m);
        let s = subject_text(&subject);
        let mut earlier: Vec<Option<(Expr, String)>> = Vec::new();
        let mut arms = Vec::new();
        for arm in &m.arms {
            let mut conds: Vec<(Expr, String)> = earlier
                .iter()
                .flatten()
                .map(|(expr, text)| {
                    (
                        call(op::LOGICAL_NOT, vec![expr.clone()]),
                        format!("!({text})"),
                    )
                })
                .collect();
            let own = match arm {
                Arm::When { is, test, .. } => {
                    let is = match is {
                        Some(p) => is_condition(&p.raw, &s, path.as_deref()).map(Some),
                        None => Some(None),
                    };
                    let test = (!test.raw.trim().is_empty()).then(|| test.raw.as_str());
                    match (is, test) {
                        (Some(Some(is)), Some(t)) => {
                            self.parse(&format!("({is}) && ({t})"), Some(&subject))
                        }
                        (Some(Some(is)), None) => self.parse(&is, Some(&subject)),
                        (Some(None), Some(t)) => self.parse(t, Some(&subject)),
                        // No pattern at all, or one that does not classify.
                        (Some(None), None) | (None, _) => None,
                    }
                }
                Arm::Otherwise { .. } => None,
            };
            conds.extend(own.clone());
            let (Arm::When { body, .. } | Arm::Otherwise { body, .. }) = arm;
            arms.push((conds, &body[..]));
            earlier.push(own);
        }
        self.fork(arms);
    }

    /// A `::next` target: the jump may arrive from outside the enclosing
    /// regions, so their guards stop counting from here.
    fn label(&mut self, id: &str) {
        if self.jumps.contains(id) {
            for g in &mut self.guards[self.base..] {
                g.live = false;
            }
        }
    }

    /// A write of state `path`: every guard reading it, a record it sits
    /// in, or a field under it stops counting.
    fn kill_path(&mut self, path: &str) {
        let segments: Vec<&str> = path.split('.').collect();
        let prefixes: Vec<String> = (2..=segments.len())
            .map(|n| segments[..n].join("."))
            .collect();
        for g in &mut self.guards {
            if g.text.contains(path) || prefixes.iter().any(|p| g.text.contains(p.as_str())) {
                g.live = false;
            }
        }
    }

    /// An `::assert` (`up`) or `::retract` of `pattern`: every guard it may
    /// falsify ([`write_effects`], [`affected`]) stops counting.
    fn kill_fact(&mut self, pattern: &FactPattern, up: bool) {
        if pattern.relation.is_empty() {
            return;
        }
        let args = pattern_args(pattern);
        let effects = write_effects(&self.folded.env.rel_vocab, &pattern.relation, args, up);
        for g in &mut self.guards {
            if g.live && affected(&effects, g) {
                g.live = false;
            }
        }
    }

    /// Every write anywhere in `nodes`.
    fn kill_writes(&mut self, nodes: &[Node]) {
        let mut paths = Vec::new();
        let mut facts = Vec::new();
        let mut quests = Vec::new();
        visit(nodes, &mut |node| match node {
            Node::Set(s) => paths.push(s.path.clone()),
            Node::Timeline(t) => {
                for clip in t.tracks.iter().flat_map(|tr| &tr.clips) {
                    if let ClipNode::Set(s) = &clip.node {
                        paths.push(s.path.clone());
                    }
                }
            }
            Node::Assert(a) => facts.push((&a.pattern, true)),
            Node::Retract(r) => facts.push((&r.pattern, false)),
            Node::Directive(d) => {
                if let Some((q, _)) = d.accept_quest() {
                    quests.push(format!("quest.{q}"));
                }
            }
            _ => {}
        });
        for p in paths.iter().chain(&quests) {
            self.kill_path(p);
        }
        for (pattern, up) in facts {
            self.kill_fact(pattern, up);
        }
    }

    /// `present`'s conjuncts for `speaker`, parsed once, each with its
    /// rule-read form under the relations [`Self::changed`] holds (dsl
    /// 0.25.0 §6). A plugin `present:` that is not a condition is reported
    /// here, once, at `span`.
    fn present_of(&mut self, speaker: &str, span: Span) -> Option<PresentConjuncts> {
        let written = match self.present.get(speaker) {
            Some(cached) => cached.clone()?,
            None => {
                let written = self.written_present(speaker, span);
                self.present.insert(speaker.to_string(), written.clone());
                written?
            }
        };
        let assume = self.folded.cast.get(speaker)?.assume == Some(true);
        let changed = if assume {
            self.changed.clone()
        } else {
            BTreeSet::new()
        };
        let key = (speaker.to_string(), changed);
        if let Some(read) = self.reads.get(&key) {
            return Some(read.clone());
        }
        let side = Side::Present { assume };
        let read: PresentConjuncts = written
            .into_iter()
            .map(|p| {
                let read = self.expand(&p, Pol::Pos, side, EXPAND_DEPTH, &mut Vec::new());
                (p, read)
            })
            .collect();
        self.reads.insert(key, read.clone());
        Some(read)
    }

    /// `present`'s top-level conjuncts for `speaker` as written; `None` (the
    /// faults reported at `span`, single-file) when it is not a condition.
    fn written_present(&mut self, speaker: &str, span: Span) -> Option<Vec<Expr>> {
        let member = self.folded.cast.get(speaker)?;
        let raw = member.present.clone()?;
        let mut faults = present_faults(speaker, &raw, span);
        // A `present:` reading a state path this document does not declare
        // is no condition here — `W-CAST-ABSENT` would suggest a guard that
        // is itself `E-UNDECLARED`.
        if faults.is_empty() {
            if let Some((e, _)) = self.parse(&raw, None) {
                let mut paths = Vec::new();
                read_paths(&e, &mut paths);
                paths.sort();
                paths.dedup();
                for p in paths.iter().filter(|p| {
                    crate::meta::namespace_of(p).is_some()
                        && !crate::defassign::is_declared(p, &self.folded.env.state)
                }) {
                    faults.push(cast_diag(
                        "E-UNDECLARED",
                        Severity::Error,
                        Layer::Cel,
                        format!(
                            "cast `{speaker}` `present: \"{raw}\"` reads `{p}`, which this \
                             document's `state:` does not declare — declare it where every \
                             document the member speaks in imports it, or fix the condition"
                        ),
                        span,
                    ));
                }
            }
        }
        if faults.is_empty() {
            return self.parse(&raw, None).map(|(e, _)| {
                let mut out = Vec::new();
                conjuncts(&e, &mut out);
                out
            });
        }
        if self.facts.is_none() {
            self.out.extend(faults.into_iter().map(|mut d| {
                d.message.push_str(if d.code == "E-UNDECLARED" {
                    " (dsl 0.24.0 §4)"
                } else {
                    " — fix the plugin's `cast` export (dsl 0.24.0 §4)"
                });
                d
            }));
        }
        None
    }

    fn line(&mut self, l: &Line) {
        // A voice-over (a letter, a log page, a memory) does not put its
        // speaker in the room. An `{os}` line still does: the speaker is
        // there, only out of frame.
        if l.attrs.iter().any(|a| a.key == "vo") {
            return;
        }
        // The speaker id sits just past the line's leading `@`.
        let start = l.span.byte_start + 1;
        let span = Span {
            byte_start: start,
            byte_end: start + l.speaker.len(),
            line: 0,
            column: 0,
            utf16_range: (0, 0),
        };
        let own = l.when.as_ref().and_then(|w| self.parse(&w.raw, None));
        // dsl 0.25.0 §6: a line whose own guard needs a `changedOn` fact runs
        // after the occasion that writes it.
        let before = self.changed.clone();
        if let Some((e, _)) = &own {
            let required = self.required(e, EXPAND_DEPTH);
            self.changed.extend(required);
        }
        self.decide_line(l, span, own, None);
        self.changed = before;
    }

    /// [`Self::line`] once its own `when` is parsed (and [`Self::changed`]
    /// includes what it requires). `via`: the component whose `@@p:` line
    /// this is, bound at a `::use` ([`Self::use_lines`]).
    fn decide_line(
        &mut self,
        l: &Line,
        span: Span,
        own: Option<(Expr, String)>,
        via: Option<&str>,
    ) {
        let Some(present) = self.present_of(&l.speaker, span) else {
            return;
        };
        let own =
            own.map(|(e, _)| self.expand(&e, Pol::Pos, Side::Guard, EXPAND_DEPTH, &mut Vec::new()));
        // The Must slot `fact_must` records for this line.
        let slot = l.when.as_ref().map_or(l.span, |w| w.span);
        if self.implied(&present, own.as_ref(), slot) {
            return;
        }
        let raw = self.folded.cast[&l.speaker]
            .present
            .clone()
            .unwrap_or_default();
        let mut message = match via {
            None => format!(
                "`{who}` may not be here: the cast declares `present: \"{raw}\"` for `{who}`, and \
                 the guards around this line do not imply it (dsl 0.24.0 §4). Guard the line — \
                 `@{who}{{when=\"{raw}\"}}` — or move it under a guard that implies it",
                who = l.speaker
            ),
            Some(component) => format!(
                "`{who}` may not be here: component `{component}` speaks as `{who}` at this \
                 `::use`, the cast declares `present: \"{raw}\"` for `{who}`, and the guards \
                 around the `::use` do not imply it (dsl 0.24.0 §4, 0.26.0 §3.2). Guard it — \
                 `::use{{… when=\"{raw}\"}}` — or move it under a guard that implies it",
                who = l.speaker
            ),
        };
        // dsl 0.25.0 §6: say so when only `changedOn` took `assume` away.
        if !self.changed.is_empty() && self.folded.cast[&l.speaker].assume == Some(true) {
            let changed = std::mem::take(&mut self.changed);
            let assumed = self.present_of(&l.speaker, span);
            let rels: Vec<String> = changed.iter().map(|r| format!("`{r}`")).collect();
            self.changed = changed;
            if assumed.is_some_and(|a| self.implied(&a, own.as_ref(), slot)) {
                message.push_str(&format!(
                    "; `assume: true` does not cover {}: this line runs after an occasion its \
                     `changedOn:` names — it follows one, or its guards need a fact only one \
                     writes (dsl 0.25.0 §6)",
                    rels.join(", ")
                ));
            }
        }
        if self.facts.is_none() && (raw.contains("holds(") || raw.contains("count(")) {
            message.push_str(
                " (a single-file check does not see facts asserted on every route to this \
                 line; `lute check-project` does)",
            );
        }
        self.out.push(cast_diag(
            W_CAST_ABSENT,
            Severity::Warning,
            Layer::Logic,
            message,
            span,
        ));
    }

    /// `true` iff the live guards (and the line's own `when`) imply every
    /// conjunct of `present`: `guards && !p` is refuted, for `p` as written
    /// or read through the rules.
    fn implied(&self, present: &[(Expr, Expr)], own: Option<&Expr>, slot: Span) -> bool {
        let ctx = DecideCtx {
            schema: &self.folded.env.state,
            dollar: None,
            params: self.params,
            facts: self.facts.map(|env| FactScope {
                env,
                vocab: &self.folded.env.rel_vocab,
                path: self.path,
                span: slot,
                wip: false,
            }),
        };
        let guards: Vec<Expr> = self
            .guards
            .iter()
            .filter(|g| g.live)
            .map(|g| g.expr.clone())
            .chain(own.cloned())
            .collect();
        let refutes = |p: &Expr| {
            let mut items = guards.clone();
            items.push(call(op::LOGICAL_NOT, vec![p.clone()]));
            conjoin(&items).is_some_and(|e| refuted(&e, &ctx))
        };
        present
            .iter()
            .all(|(p, read)| refutes(p) || (read != p && refutes(read)))
    }
}
