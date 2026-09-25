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
/// to the def rules. Silent when `cast` is empty (shape-only).
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
    let known = |id: &str| id == "narrator" || cast.contains_key(id);
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
    let Some(key) = staged_attr(&d.tag) else { return };
    let what = if key == "character" { "`::auto{character}`" } else { "`::camera{focus}`" };
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

pub(crate) fn unknown(what: String, id: &str, span: Span, cast: &BTreeMap<String, CastMember>) -> Diagnostic {
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

fn cast_diag(code: &str, severity: Severity, layer: Layer, message: String, span: Span) -> Diagnostic {
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
    if let (Some(emotions), Some(dom)) = (&member.emotions, domains.get("emotion").filter(|d| !d.open)) {
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
    attrs.iter().find(|a| a.key == key).and_then(|a| match &a.value {
        AttrValue::Str(s) => Some((s.as_str(), a.value_span)),
        _ => None,
    })
}

/// dsl 0.24.0 §4: `E-BAD-ENUM` for an `emotion=` outside the declared
/// `emotions:` of the content line's speaker — or of the literal
/// `character=` a directive (a timeline clip included) names. A value the
/// closed `emotion` enum itself rejects is left to that check, so each bad
/// value is one error.
pub fn check_emotions(
    doc: &Document,
    cast: &BTreeMap<String, CastMember>,
    domains: &BTreeMap<String, Domain>,
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
        if allowed.iter().any(|e| e == value) || closed.is_some_and(|d| !d.members.iter().any(|m| m == value)) {
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
    facts: Option<&FactEnv>,
    ladder: &BTreeMap<usize, Vec<String>>,
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
    let mut w = Presence {
        path,
        folded,
        facts,
        params: &params,
        defs: DefTable {
            bodies: &folded.def_bodies,
            params: &folded.env.def_params,
        },
        jumps,
        guards: Vec::new(),
        base: 0,
        quest: None,
        present: BTreeMap::new(),
        out: Vec::new(),
    };
    let ladder_at = |at: Span| ladder.get(&at.byte_start).map_or(&[][..], Vec::as_slice);

    let scene_ladder = match &folded.typed.beat {
        Some(_) => ladder_at(crate::beats::top_key_span(&doc.meta, "on")),
        None => &[],
    };
    let scene_when = w.slot_cond(folded.typed.beat.as_ref().and_then(|b| b.when.as_ref()));
    w.unit(scene_when.into_iter().collect(), scene_ladder, doc.shots.iter().map(|s| &s.body[..]));
    for quest in &doc.quests {
        let mut conds = Vec::new();
        if let Some((start, _)) = w.slot_cond(quest.start.as_ref()) {
            let mut cs = Vec::new();
            conjuncts(&start, &mut cs);
            conds.extend(cs.into_iter().filter_map(|c| stable_text(&c).map(|t| (c, t))));
        }
        w.quest = (!quest.id.is_empty()).then(|| quest.id.clone());
        w.unit(conds, &[], std::iter::once(&quest.body[..]));
        w.quest = None;
    }
    for entry in &doc.entries {
        let mut conds: Vec<(Expr, String)> = w.slot_cond(entry.when.as_ref()).into_iter().collect();
        if !entry.id.is_empty() {
            conds.extend(w.parse(&format!("entry.{0}.everRead && entry.{0}.read", entry.id), None));
        }
        let ladder = entry.on.as_ref().map_or(&[][..], |(_, s)| ladder_at(*s));
        w.unit(conds, ladder, std::iter::once(&entry.body[..]));
    }
    for beat in &doc.beats {
        let conds = w.slot_cond(beat.when.as_ref()).into_iter().collect();
        let ladder = beat.on.as_ref().map_or(&[][..], |(_, s)| ladder_at(*s));
        w.unit(conds, ladder, std::iter::once(&beat.body[..]));
    }
    w.out
}

/// dsl 0.24.0 §4, `check-project`: re-decide one document's per-file
/// `W-CAST-ABSENT`s under the root's fact envelope and the document's
/// beat-ladder assumptions ([`crate::beats::presence_ladder`]) — a line the
/// Must set or the ladder shows present is dropped, the rest keep the
/// project's wording.
pub fn reconcile_presence(
    diags: &mut Vec<Diagnostic>,
    path: &Path,
    doc: &Document,
    folded: &FoldedEnv,
    env: &FactEnv,
    ladder: &BTreeMap<usize, Vec<String>>,
) {
    if !diags.iter().any(|d| d.code == W_CAST_ABSENT) {
        return;
    }
    let still: Vec<Diagnostic> = check_presence(path, doc, folded, Some(env), ladder)
        .into_iter()
        .filter(|d| d.code == W_CAST_ABSENT)
        .collect();
    diags.retain_mut(|d| {
        if d.code != W_CAST_ABSENT {
            return true;
        }
        match still
            .iter()
            .find(|s| (s.span.byte_start, s.span.byte_end) == (d.span.byte_start, d.span.byte_end))
        {
            Some(s) => {
                d.message = s.message.clone();
                true
            }
            None => false,
        }
    });
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
        Expr::Select(sel) if !sel.test => Some(format!("{}.{}", select_text(&sel.operand.expr)?, sel.field)),
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
        Expr::List(l) => l.elements.iter().fold(true, |ok, x| read_paths(&x.expr, out) && ok),
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
            && self.args.iter().zip(&other.args).all(|(a, b)| a.is_none() || a == b)
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
                            pol: if c.func_name == "holds" { pol } else { Pol::Both },
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
                && a.args.iter().zip(&e.args).all(|(x, y)| x.is_none() || y.is_none() || x == y)
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
            && f.fact.args.iter().zip(args).all(|(a, k)| match (&a.term, k) {
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
                BodyLiteral::Pos(a) | BodyLiteral::Neg(a) => a.terms.iter().collect(),
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
                        let member = match (a.terms.as_slice(), vocab.kinds.get(&a.relation).map(|k| &k.shape)) {
                            ([t], Some(KindShape::Members(ms))) => value(t).map(|v| ms.contains(&v)),
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
                BodyLiteral::Cmp { lhs, rhs, negated, .. } => match (value(lhs), value(rhs)) {
                    (Some(l), Some(r)) => {
                        if (l == r) == *negated {
                            dead = true;
                            break;
                        }
                    }
                    _ => def.exact = false,
                },
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
                                .flat_map(|x| r.iter().map(move |y| x.iter().chain(y).cloned().collect()))
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
        return Some(if *b != neg { vec![Vec::new()] } else { Vec::new() });
    }
    let lit = if neg { call(op::LOGICAL_NOT, vec![e.clone()]) } else { e.clone() };
    Some(vec![vec![lit]])
}

/// `a && (b && (…))`.
fn conjoin(items: &[Expr]) -> Option<Expr> {
    let (last, rest) = items.split_last()?;
    Some(rest.iter().rev().fold(last.clone(), |acc, x| call(op::LOGICAL_AND, vec![x.clone(), acc])))
}

/// `e` provably false: decided `false` whole, or every disjunct of its
/// normal form is.
fn refuted(e: &Expr, ctx: &DecideCtx<'_>) -> bool {
    if decide(e, ctx) == Some(Decided::Bool(false)) {
        return true;
    }
    let Some(terms) = dnf(e, false) else { return false };
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
    params: &'a BTreeMap<String, DomainInfo>,
    defs: DefTable<'a>,
    /// Every `::next{to}` target of the document.
    jumps: BTreeSet<String>,
    guards: Vec<Guard>,
    /// `guards[..base]` are the current unit's own assumptions.
    base: usize,
    /// The quest whose body is being walked.
    quest: Option<String>,
    /// Speaker → `present`'s conjuncts; `None` once reported unparseable.
    present: BTreeMap<String, Option<PresentConjuncts>>,
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
        args: args.into_iter().map(|expr| IdedExpr { id: 0, expr }).collect(),
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
                (Some(lo), Some(hi)) => format!("{s} >= {} && {s} <= {}", num_text(lo), num_text(hi)),
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
    /// negative `holds` of an engine-`reserved` relation reads `false`.
    /// The rule `cel()`s read are pushed onto `cels`.
    fn expand(&self, e: &Expr, pol: Pol, side: Side, depth: u8, cels: &mut Vec<String>) -> Expr {
        let Expr::Call(c) = e else { return e.clone() };
        if c.target.is_some() {
            return e.clone();
        }
        match (c.func_name.as_str(), c.args.as_slice()) {
            (n, [a]) if n == op::LOGICAL_NOT => call(n, vec![self.expand(&a.expr, pol.flip(), side, depth, cels)]),
            (n, [a, b]) if n == op::LOGICAL_AND || n == op::LOGICAL_OR => call(
                n,
                vec![
                    self.expand(&a.expr, pol, side, depth, cels),
                    self.expand(&b.expr, pol, side, depth, cels),
                ],
            ),
            ("holds", [a]) => {
                let Expr::Call(atom) = &a.expr else { return e.clone() };
                if atom.target.is_some() {
                    return e.clone();
                }
                let vocab = &self.folded.env.rel_vocab;
                if side == (Side::Present { assume: true })
                    && pol == Pol::Neg
                    && vocab.relations.get(&atom.func_name).is_some_and(|d| d.reserved)
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
    /// its own assumptions and the ladder's.
    fn unit<'n>(&mut self, conds: Vec<(Expr, String)>, ladder: &[String], bodies: impl Iterator<Item = &'n [Node]>) {
        self.guards.clear();
        for c in conds {
            self.push(Some(c));
        }
        for l in ladder {
            let c = self.parse(l, None);
            self.push(c);
        }
        self.base = self.guards.len();
        for body in bodies {
            self.walk(body);
        }
    }

    /// Walk `body` under `conds` added to the guards pushed so far; they
    /// are popped when it ends. A write inside still reaches the outer
    /// guards: the region runs before what follows it.
    fn region(&mut self, conds: Vec<(Expr, String)>, body: &[Node]) {
        let depth = self.guards.len();
        for c in conds {
            self.push(Some(c));
        }
        self.walk(body);
        self.guards.truncate(depth);
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
            .map(|c| c.when.as_ref().and_then(|w| self.parse(&w.raw, None)).into_iter().collect())
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
                    self.fork(conds.into_iter().zip(b.choices.iter().map(|c| &c.body[..])).collect());
                }
                Node::Hub(h) => {
                    // A hub body runs again and again: whatever one round
                    // writes, the next round's guards may not survive.
                    for c in &h.choices {
                        self.kill_writes(&c.body);
                    }
                    let conds = self.choice_conds(&h.choices);
                    self.fork(conds.into_iter().zip(h.choices.iter().map(|c| &c.body[..])).collect());
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
                    self.region(conds, &o.body);
                }
                Node::Objective(o) => {
                    let cond = self.parse(&o.done.raw, None);
                    self.region(cond.into_iter().collect(), &o.body);
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
                .map(|(expr, text)| (call(op::LOGICAL_NOT, vec![expr.clone()]), format!("!({text})")))
                .collect();
            let own = match arm {
                Arm::When { is, test, .. } => {
                    let is = match is {
                        Some(p) => is_condition(&p.raw, &s, path.as_deref()).map(Some),
                        None => Some(None),
                    };
                    let test = (!test.raw.trim().is_empty()).then(|| test.raw.as_str());
                    match (is, test) {
                        (Some(Some(is)), Some(t)) => self.parse(&format!("({is}) && ({t})"), Some(&subject)),
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
        let prefixes: Vec<String> = (2..=segments.len()).map(|n| segments[..n].join(".")).collect();
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
        let args = pattern
            .args
            .iter()
            .map(|a| match &a.term {
                FactTerm::Ident(s) if s.starts_with(|c: char| c.is_ascii_alphabetic()) => Some(s.clone()),
                FactTerm::Bool(b) => Some(b.to_string()),
                _ => None,
            })
            .collect();
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
    /// rule-read form. A plugin `present:` that is not a condition is
    /// reported here, once, at `span`.
    fn present_of(&mut self, speaker: &str, span: Span) -> Option<PresentConjuncts> {
        if let Some(cached) = self.present.get(speaker) {
            return cached.clone();
        }
        let member = self.folded.cast.get(speaker)?;
        let raw = member.present.clone()?;
        let side = Side::Present {
            assume: member.assume == Some(true),
        };
        let faults = present_faults(speaker, &raw, span);
        let parsed = if faults.is_empty() {
            self.parse(&raw, None).map(|(e, _)| {
                let mut out = Vec::new();
                conjuncts(&e, &mut out);
                out.into_iter()
                    .map(|p| {
                        let read = self.expand(&p, Pol::Pos, side, EXPAND_DEPTH, &mut Vec::new());
                        (p, read)
                    })
                    .collect()
            })
        } else {
            if self.facts.is_none() {
                self.out.extend(faults.into_iter().map(|mut d| {
                    d.message.push_str(" — fix the plugin's `cast` export (dsl 0.24.0 §4)");
                    d
                }));
            }
            None
        };
        self.present.insert(speaker.to_string(), parsed.clone());
        parsed
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
        let Some(present) = self.present_of(&l.speaker, span) else {
            return;
        };
        let own = l.when.as_ref().and_then(|w| self.parse(&w.raw, None));
        let own = own.map(|(e, _)| self.expand(&e, Pol::Pos, Side::Guard, EXPAND_DEPTH, &mut Vec::new()));
        // The Must slot `fact_must` records for this line.
        let slot = l.when.as_ref().map_or(l.span, |w| w.span);
        if self.implied(&present, own.as_ref(), slot) {
            return;
        }
        let raw = self.folded.cast[&l.speaker].present.clone().unwrap_or_default();
        let mut message = format!(
            "`{who}` may not be here: the cast declares `present: \"{raw}\"` for `{who}`, and the \
             guards around this line do not imply it (dsl 0.24.0 §4). Guard the line — \
             `@{who}{{when=\"{raw}\"}}` — or move it under a guard that implies it",
            who = l.speaker
        );
        if self.facts.is_none() && (raw.contains("holds(") || raw.contains("count(")) {
            message.push_str(
                " (a single-file check does not see facts asserted on every route to this \
                 line; `lute check-project` does)",
            );
        }
        self.out.push(cast_diag(W_CAST_ABSENT, Severity::Warning, Layer::Logic, message, span));
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
        present.iter().all(|(p, read)| refutes(p) || (read != p && refutes(read)))
    }
}
