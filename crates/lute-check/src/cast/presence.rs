mod conditions;
mod guards;
mod lines;
mod walk;

pub(crate) use guards::{write_effects, Atom, Effect, Pol};
use guards::*;

use super::*;

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
/// stays true once it held (`entry.<id>.everRead`, `visited('…')`); a
/// `questComplete` handler also assumes the quest's completion — one
/// required objective's `done` (dsl 0.27.0 §9); and
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
/// arm. A hub's writes count from its first round, and a `::jump` target
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
                if d.tag == lute_manifest::core::JUMP_DIRECTIVE {
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
        quest_body: &[],
        completion: None,
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
    let mut scene_conds: Vec<(Expr, String)> = w
        .slot_cond(folded.typed.beat.as_ref().and_then(|b| b.when.as_ref()))
        .into_iter()
        .collect();
    if let Some(b) = &folded.typed.beat {
        scene_conds.extend(w.seam_cond(&b.on, b.target.as_deref()));
    }
    // A `spentBy` beat is not spent by being presented: it may play again,
    // as a `once: false` one, until its condition has held.
    let scene_absent = w.absent_facts(
        0,
        folded
            .typed
            .beat
            .as_ref()
            .filter(|b| b.spent_by.is_none())
            .map_or(BeatOnce::None, |b| b.once.clone()),
    );
    let scene_after = after(0, folded.typed.beat.as_ref().map(|b| b.on.as_str()));
    w.unit(
        scene_conds,
        scene_ladder,
        scene_absent,
        &scene_after,
        doc.sections.iter().map(|s| &s.body[..]),
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
        w.quest_body = &quest.body;
        w.completion = w.completion(quest);
        let quest_after = after(quest.span.byte_start, None);
        w.unit(
            conds,
            &[],
            Vec::new(),
            &quest_after,
            std::iter::once(&quest.body[..]),
        );
        w.quest = None;
        w.quest_body = &[];
        w.completion = None;
    }
    for entry in &doc.entries {
        let mut conds: Vec<(Expr, String)> = w.slot_cond(entry.when.as_ref()).into_iter().collect();
        if !entry.id.is_empty() {
            conds.extend(w.parse(
                &format!("entry.{0}.everRead && entry.{0}.read", entry.id),
                None,
            ));
        }
        if let Some((on, _)) = &entry.on {
            conds.extend(w.seam_cond(on, entry.target.as_ref().map(|(t, _)| t.as_str())));
        }
        let ladder = entry.on.as_ref().map_or(&[][..], |(_, s)| ladder_at(*s));
        let once = match entry.once.as_ref().map(|(o, _)| o.as_str()) {
            Some(raw) if entry.spent_by.is_none() => BeatOnce::parse(raw).unwrap_or(BeatOnce::None),
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
        let mut conds: Vec<(Expr, String)> = w.slot_cond(beat.when.as_ref()).into_iter().collect();
        if let Some((on, _)) = &beat.on {
            conds.extend(w.seam_cond(on, beat.target.as_ref().map(|(t, _)| t.as_str())));
        }
        let ladder = beat.on.as_ref().map_or(&[][..], |(_, s)| ladder_at(*s));
        let once = match beat.spent_by {
            Some(_) => BeatOnce::None,
            None => crate::bundles::bundle_beat_once(beat),
        };
        let absent = w.absent_facts(beat.span.byte_start, once);
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

/// Which side of the implication a condition is on.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Side {
    /// An enclosing guard: a derived atom may add what it implies.
    Guard,
    /// `present`: only an exact rewrite; `assume` reads a negated reserved
    /// atom as true.
    Present { assume: bool },
}

/// `e`'s disjunctive normal form (of `!e` when `neg`) as literal
/// conjunctions; `None` past [`DNF_CAP`] terms.
pub(super) fn dnf(e: &Expr, neg: bool) -> Option<Vec<Vec<Expr>>> {
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
pub(super) fn conjoin(items: &[Expr]) -> Option<Expr> {
    let (last, rest) = items.split_last()?;
    Some(rest.iter().rev().fold(last.clone(), |acc, x| {
        call(op::LOGICAL_AND, vec![x.clone(), acc])
    }))
}

/// `e` provably false: decided `false` whole, or every disjunct of its
/// normal form is.
pub(super) fn refuted(e: &Expr, ctx: &DecideCtx<'_>) -> bool {
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
pub(super) struct Guard {
    pub(super) expr: Expr,
    pub(super) text: String,
    pub(super) atoms: Vec<Atom>,
    pub(super) live: bool,
}

/// A speaker's `present`: each top-level conjunct as written and read
/// through the rules (exact rewrites only).
pub(super) type PresentConjuncts = Vec<(Expr, Expr)>;

pub(super) struct Presence<'a> {
    path: &'a Path,
    folded: &'a FoldedEnv,
    facts: Option<&'a FactEnv>,
    /// `check-project`: every `::assert` site of the root.
    producers: Option<&'a FactProducers>,
    params: &'a BTreeMap<String, DomainInfo>,
    defs: DefTable<'a>,
    /// Every `::jump{to}` target of the document.
    jumps: BTreeSet<String>,
    guards: Vec<Guard>,
    /// `guards[..base]` are the current unit's own assumptions.
    base: usize,
    /// The quest whose body is being walked.
    quest: Option<String>,
    /// dsl 0.27.0 §9 (T3-25): that quest's body and its completion
    /// ([`Self::completion`]), for its `questComplete` handlers.
    quest_body: &'a [Node],
    completion: Option<(Expr, String)>,
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
pub(super) fn conjuncts(e: &Expr, out: &mut Vec<Expr>) {
    if let Expr::Call(c) = e {
        if c.func_name == op::LOGICAL_AND && c.target.is_none() && c.args.len() == 2 {
            conjuncts(&c.args[0].expr, out);
            conjuncts(&c.args[1].expr, out);
            return;
        }
    }
    out.push(e.clone());
}

pub(super) fn call(name: &str, args: Vec<Expr>) -> Expr {
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
pub(super) fn num_text(n: f64) -> String {
    format!("{n}")
}

/// A `<when is="…">` pattern as a condition over the subject text `s` —
/// its alternatives joined by `||`; `None` when any alternative does not
/// classify (the arm then assumes nothing).
pub(super) fn is_condition(raw: &str, s: &str, path: Option<&str>) -> Option<String> {
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
}