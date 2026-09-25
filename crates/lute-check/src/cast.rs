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
use lute_manifest::snapshot::Domain;
use lute_syntax::ast::{CelKind, CelSlot, Match};
use lute_syntax::is_pattern::{classify_is_literal, is_alternatives, IsLiteral};

use crate::cel_expand::{expand_cel, subject_text, DefTable};
use crate::check::FoldedEnv;
use crate::decide::{decide, DecideCtx, Decided};
use crate::fact_env::{FactEnv, FactScope};
use crate::match_check::DomainInfo;

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
/// ([`decide`]).
///
/// The guards of a line: its own `when=`, every enclosing `<choice when>`
/// (branch and hub), `<match>` arm (its `is=`/`test` over the subject, and
/// the negation of every earlier arm — first match wins), `<on when>`,
/// `<objective done>` (its body runs on completion), and the unit's own
/// `when` — a scene beat's frontmatter `when:`, an entry's or a bundle
/// beat's `when=`. `@def`s are expanded first. A guard stops counting once
/// something that may change it runs after it (a `::set` of a path it reads,
/// an `::assert`/`::retract` of a relation it queries — any, when it queries
/// a derived relation — an `::accept` of a quest it reads; a hub's writes
/// count from its first round), and a `::next` target label drops the guards
/// of the region it sits in (the jump may come from outside it).
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
        present: BTreeMap::new(),
        out: Vec::new(),
    };
    let beat_when = folded.typed.beat.as_ref().and_then(|b| b.when.as_ref());
    w.unit(beat_when, doc.shots.iter().map(|s| &s.body[..]));
    for quest in &doc.quests {
        w.unit(None, std::iter::once(&quest.body[..]));
    }
    for entry in &doc.entries {
        w.unit(entry.when.as_ref(), std::iter::once(&entry.body[..]));
    }
    for beat in &doc.beats {
        w.unit(beat.when.as_ref(), std::iter::once(&beat.body[..]));
    }
    w.out
}

/// dsl 0.24.0 §4, `check-project`: re-decide one document's per-file
/// `W-CAST-ABSENT`s under the root's fact envelope — a line the Must set
/// shows present is dropped, the rest keep the project's wording.
pub fn reconcile_presence(
    diags: &mut Vec<Diagnostic>,
    path: &Path,
    doc: &Document,
    folded: &FoldedEnv,
    env: &FactEnv,
) {
    if !diags.iter().any(|d| d.code == W_CAST_ABSENT) {
        return;
    }
    let still: Vec<Diagnostic> = check_presence(path, doc, folded, Some(env))
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

/// One enclosing guard: its parsed condition and expanded text; `live`
/// turns false once a write it reads may have changed it.
struct Guard {
    expr: Expr,
    text: String,
    live: bool,
}

struct Presence<'a> {
    path: &'a Path,
    folded: &'a FoldedEnv,
    facts: Option<&'a FactEnv>,
    params: &'a BTreeMap<String, DomainInfo>,
    defs: DefTable<'a>,
    /// Every `::next{to}` target of the document.
    jumps: BTreeSet<String>,
    guards: Vec<Guard>,
    /// `guards[..base]` are the current unit's own `when`.
    base: usize,
    /// Speaker → `present`'s top-level conjuncts; `None` once reported
    /// unparseable.
    present: BTreeMap<String, Option<Vec<Expr>>>,
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
    /// `raw` with its `@def`s (and `$`, as `subject`) expanded, and parsed.
    fn parse(&self, raw: &str, subject: Option<&str>) -> Option<(Expr, String)> {
        if raw.trim().is_empty() {
            return None;
        }
        let mut stack = Vec::new();
        let text = expand_cel(raw, &self.defs, subject, &mut stack).unwrap_or_else(|_| raw.to_string());
        let mut arena = lute_cel::CelArena::default();
        let handle = lute_cel::parse_slot_marked_refs(&mut arena, &text)?;
        Some((arena.get(handle)?.expr.clone(), text))
    }

    fn push(&mut self, cond: Option<(Expr, String)>) {
        if let Some((expr, text)) = cond {
            self.guards.push(Guard {
                expr,
                text,
                live: true,
            });
        }
    }

    fn push_slot(&mut self, slot: Option<&CelSlot>) {
        let cond = slot.and_then(|s| self.parse(&s.raw, None));
        self.push(cond);
    }

    /// One unit (a scene's shots, a quest, an entry, a bundle beat) under
    /// its own `when`.
    fn unit<'n>(&mut self, when: Option<&CelSlot>, bodies: impl Iterator<Item = &'n [Node]>) {
        self.guards.clear();
        self.push_slot(when);
        self.base = self.guards.len();
        for body in bodies {
            self.walk(body);
        }
    }

    /// Walk `nodes` under the guards pushed so far; a region pushes its
    /// guards and pops them when it ends.
    fn region(&mut self, cond: Option<(Expr, String)>, body: &[Node]) {
        let depth = self.guards.len();
        self.push(cond);
        self.walk(body);
        self.guards.truncate(depth);
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
                Node::Assert(a) => self.kill_relation(&a.pattern.relation),
                Node::Retract(r) => self.kill_relation(&r.pattern.relation),
                Node::Branch(b) => {
                    for c in &b.choices {
                        let cond = c.when.as_ref().and_then(|w| self.parse(&w.raw, None));
                        self.region(cond, &c.body);
                    }
                }
                Node::Hub(h) => {
                    // A hub body runs again and again: whatever one round
                    // writes, the next round's guards may not survive.
                    for c in &h.choices {
                        self.kill_writes(&c.body);
                    }
                    for c in &h.choices {
                        let cond = c.when.as_ref().and_then(|w| self.parse(&w.raw, None));
                        self.region(cond, &c.body);
                    }
                }
                Node::Match(m) => self.match_arms(m),
                Node::On(o) => {
                    let cond = o.when.as_ref().and_then(|w| self.parse(&w.raw, None));
                    self.region(cond, &o.body);
                }
                Node::Objective(o) => {
                    let cond = self.parse(&o.done.raw, None);
                    self.region(cond, &o.body);
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
        let subject = {
            let mut stack = Vec::new();
            expand_cel(&m.subject.raw, &self.defs, None, &mut stack).unwrap_or_else(|_| m.subject.raw.clone())
        };
        let path = crate::match_check::subject_path(m);
        let s = subject_text(&subject);
        let mut earlier: Vec<Option<(Expr, String)>> = Vec::new();
        for arm in &m.arms {
            let depth = self.guards.len();
            for (expr, text) in earlier.iter().flatten() {
                let negated = (call(op::LOGICAL_NOT, vec![expr.clone()]), format!("!({text})"));
                self.push(Some(negated));
            }
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
            self.push(own.clone());
            let (Arm::When { body, .. } | Arm::Otherwise { body, .. }) = arm;
            self.walk(body);
            self.guards.truncate(depth);
            earlier.push(own);
        }
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

    /// A write of `relation`: every guard querying it, or any derived
    /// relation, stops counting.
    fn kill_relation(&mut self, relation: &str) {
        let vocab = &self.folded.env.rel_vocab;
        let mut touched: Vec<String> = vec![format!("{relation}(")];
        touched.extend(
            vocab
                .relations
                .iter()
                .filter(|(_, d)| d.derive)
                .map(|(name, _)| format!("{name}(")),
        );
        for g in &mut self.guards {
            if touched.iter().any(|t| g.text.contains(t.as_str())) {
                g.live = false;
            }
        }
    }

    /// Every write anywhere in `nodes`.
    fn kill_writes(&mut self, nodes: &[Node]) {
        let mut paths = Vec::new();
        let mut relations = Vec::new();
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
            Node::Assert(a) => relations.push(a.pattern.relation.clone()),
            Node::Retract(r) => relations.push(r.pattern.relation.clone()),
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
        for r in &relations {
            self.kill_relation(r);
        }
    }

    /// `present`'s conjuncts for `speaker`, parsed once. A plugin `present:`
    /// that is not a condition is reported here, once, at `span`.
    fn present_of(&mut self, speaker: &str, span: Span) -> Option<Vec<Expr>> {
        if let Some(cached) = self.present.get(speaker) {
            return cached.clone();
        }
        let raw = self.folded.cast.get(speaker)?.present.clone()?;
        let faults = present_faults(speaker, &raw, span);
        let parsed = if faults.is_empty() {
            self.parse(&raw, None).map(|(e, _)| {
                let mut out = Vec::new();
                conjuncts(&e, &mut out);
                out
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
        // The Must slot `fact_must` records for this line.
        let slot = l.when.as_ref().map_or(l.span, |w| w.span);
        if self.implied(&present, own.as_ref().map(|(e, _)| e), slot) {
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
    /// conjunct of `present`: `guards && !p` decides `false`.
    fn implied(&self, present: &[Expr], own: Option<&Expr>, slot: Span) -> bool {
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
        let guards: Vec<&Expr> = self
            .guards
            .iter()
            .filter(|g| g.live)
            .map(|g| &g.expr)
            .chain(own)
            .collect();
        present.iter().all(|p| {
            let expr = guards.iter().rev().fold(call(op::LOGICAL_NOT, vec![p.clone()]), |acc, g| {
                call(op::LOGICAL_AND, vec![(*g).clone(), acc])
            });
            decide(&expr, &ctx) == Some(Decided::Bool(false))
        })
    }
}
