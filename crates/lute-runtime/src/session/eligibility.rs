//! Eligibility and selection (dsl 0.21.0 §4): one beat's verdict
//! ([`judge_beat`], [`Premise`]), every candidate of an occasion in
//! selection order, and what an occasion presents.

use std::collections::BTreeMap;
use std::sync::Arc;

use lute_manifest::semantics::prereq::{atoms, Atom, PrereqFormula};
use lute_ir::{BeatKind, BeatOnce, IndexBeat};
use lute_manifest::schema::OccasionSelect;
use serde_json::Value as Json;

use super::project::ExecProject;
use super::walk::describe_atoms;
use super::world::{SessionEvalObserver, World};
use crate::{Driver, Machine, Slot};
use crate::{UnresolvedAtom, Value};

/// A candidate's verdict (dsl 0.21.0 §4).
#[derive(Clone, Debug, PartialEq)]
pub enum Verdict {
    Eligible,
    /// A premise decided false — the first in judgment order.
    Ineligible(Premise),
    /// Nothing decided false, but a premise (`after`, `spentBy`, `when`) is
    /// undecided — the detail names why.
    Unknown(String),
}

/// The premise that makes a beat ineligible (round-5 T3-12), structured
/// so each tool names it in its own words. `Display` is `lute play`'s and
/// `lute calendar`'s reason text.
#[derive(Clone, Debug, PartialEq)]
pub enum Premise {
    /// Its `once` is spent (by a presentation, a read, a clock period, a
    /// season window or a `share` sibling); `reason` says which.
    Spent {
        once: Option<BeatOnce>,
        reason: String,
    },
    /// Its `after:` / `after=` does not hold: `raw` as authored, `unmet`
    /// the atoms the world does not satisfy; `chapters` when a chain of the
    /// manifest's `chapters:` wrote it (dsl 0.28.0 §4), not the scene.
    After {
        raw: String,
        unmet: Vec<Atom>,
        chapters: bool,
    },
    /// dsl 0.27.0 §5: its `spentBy` condition holds.
    SpentBy(String),
    /// Its `when` decided false (`raw`: the compiled condition).
    When { raw: String },
    /// dsl 0.27.0 §4 (HW27-04): the engine would not raise the beat's
    /// occasion — its `raisedWhen` gate (`raw`) is false over `reads`.
    Gate {
        occasion: String,
        raw: String,
        reads: Vec<crate::GuardRead>,
    },
    /// dsl 0.27.0 §4 (HW27-04): the project's `terminal:` (`raw`) holds —
    /// the game is over and the engine raises no occasion (`occasion`, the
    /// beat's, included).
    Terminal { occasion: String, raw: String },
}

impl std::fmt::Display for Premise {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Premise::Spent { reason, .. } | Premise::SpentBy(reason) => f.write_str(reason),
            Premise::After { raw, chapters, .. } => {
                write!(f, "after: {raw} is not satisfied")?;
                if *chapters {
                    f.write_str(lute_manifest::semantics::chapters::PROVENANCE)?;
                }
                Ok(())
            }
            Premise::When { .. } => f.write_str("when: false"),
            Premise::Gate {
                occasion,
                raw,
                reads,
            } => write!(
                f,
                "`{occasion}` is not raised: its `raisedWhen: {raw}` is false{}",
                crate::seam::Closed::reads_text(reads)
            ),
            Premise::Terminal { raw, .. } => {
                write!(f, "the game is over: `terminal: {raw}` holds")
            }
        }
    }
}

impl Premise {
    /// The premise's name as a test states it (`eligible: { b: { false:
    /// spentBy } }`): `once` (spent), `after`, `spentBy`, `when`, `gate` or
    /// `terminal` ([`PREMISE_KINDS`]).
    pub fn kind(&self) -> &'static str {
        match self {
            Premise::Spent { .. } => "once",
            Premise::After { .. } => "after",
            Premise::SpentBy(_) => "spentBy",
            Premise::When { .. } => "when",
            Premise::Gate { .. } => "gate",
            Premise::Terminal { .. } => "terminal",
        }
    }
}

/// Every [`Premise::kind`], in the order a beat's eligibility judges them.
pub const PREMISE_KINDS: [&str; 6] = ["terminal", "gate", "once", "after", "spentBy", "when"];

pub struct Candidate {
    pub id: String,
    pub kind: BeatKind,
    pub document: String,
    pub priority: i64,
    pub verdict: Verdict,
    /// dsl 0.23.0 §11: an entry beat already read this run
    /// (`entry.<id>.read`) — an interview menu shows it as read.
    pub read: bool,
    /// dsl 0.23.0 §3: a scene beat's `also: true` — presented after the
    /// `select: first` winner, never the winner itself.
    pub also: bool,
    /// dsl 0.27.0 §3 (T2-10): the member a `for="kind:<kind>"` beat's
    /// candidate is judged (and presented) for; `None` for any other beat.
    pub for_member: Option<String>,
    /// dsl 0.28.0 (T2-10): the verdict was judged again at the beat's turn
    /// in a `select: sequence` raise, after an earlier beat of the raise
    /// played — it may differ from the verdict when the occasion was raised.
    pub rejudged: bool,
}

pub fn kind_label(kind: BeatKind) -> &'static str {
    match kind {
        BeatKind::Scene => "scene",
        BeatKind::Entry => "entry",
        BeatKind::Bundle => "beat",
    }
}

/// A scene's declared `after:` or (dsl 0.25.0 §3) a bundle beat's `after=`
/// — the `formula` the compiler parsed onto the `prereqEdges` row whose
/// `node` is the beat's id. A blank `after` has none.
pub fn beat_prereq(doc_json: &Json, id: &str) -> Option<PrereqFormula> {
    serde_json::from_value(beat_prereq_row(doc_json, id)?.get("formula")?.clone()).ok()
}

/// The beat's `prereqEdges` row.
fn beat_prereq_row<'a>(doc_json: &'a Json, id: &str) -> Option<&'a Json> {
    doc_json
        .get("prereqEdges")
        .and_then(Json::as_array)?
        .iter()
        .find(|e| e.get("node").and_then(Json::as_str) == Some(id))
}

/// The `after` text of the beat's `prereqEdges` row, blank → `None`.
fn beat_prereq_raw<'a>(doc_json: &'a Json, id: &str) -> Option<&'a str> {
    beat_prereq_row(doc_json, id)?
        .get("after")
        .and_then(Json::as_str)
        .filter(|raw| !raw.trim().is_empty())
}

/// A prerequisite over the world, three-valued: `visited(K)` reads the
/// presented set; `completed(Q)` / `active(Q)` the quest's state — `unset`
/// for a quest the project declares that the world holds no state for, and
/// undecided (`Err`, the `quest.<Q>.state` read) for one it does not
/// declare (a single-document trace of a scene gated on another
/// document's quest).
pub fn eval_prereq(
    p: &ExecProject,
    f: &PrereqFormula,
    w: &World,
) -> Result<bool, Vec<UnresolvedAtom>> {
    let quest = |q: &String, want: &str| match w.quests.get(q) {
        Some(s) => Ok(s == want),
        None if p.quest_objectives.contains_key(q) => Ok(want == "unset"),
        None => Err(vec![UnresolvedAtom::Path(format!("quest.{q}.state"))]),
    };
    match f {
        PrereqFormula::Visited(k) => Ok(w.visited.contains(k)),
        PrereqFormula::Completed(q) => quest(q, "complete"),
        PrereqFormula::Active(q) => quest(q, "active"),
        PrereqFormula::And(a, b) => match (eval_prereq(p, a, w), eval_prereq(p, b, w)) {
            (Ok(false), _) | (_, Ok(false)) => Ok(false),
            (Ok(true), Ok(true)) => Ok(true),
            (x, y) => Err(x.err().into_iter().chain(y.err()).flatten().collect()),
        },
        PrereqFormula::Or(a, b) => match (eval_prereq(p, a, w), eval_prereq(p, b, w)) {
            (Ok(true), _) | (_, Ok(true)) => Ok(true),
            (Ok(false), Ok(false)) => Ok(false),
            (x, y) => Err(x.err().into_iter().chain(y.err()).flatten().collect()),
        },
    }
}

/// The atoms of `f` the world does not satisfy — what a mock would have to
/// add for the prerequisite to hold.
fn unmet_prereq(p: &ExecProject, f: &PrereqFormula, w: &World) -> Vec<Atom> {
    atoms(f)
        .into_iter()
        .filter(|a| {
            let holds = match a {
                Atom::Visited(k) => PrereqFormula::Visited(k.clone()),
                Atom::Completed(q) => PrereqFormula::Completed(q.clone()),
                Atom::Active(q) => PrereqFormula::Active(q.clone()),
            };
            eval_prereq(p, &holds, w) != Ok(true)
        })
        .collect()
}

/// The beat's `when`: a scene's `meta.beat.when`, an entry's own `when` on
/// its `entry` record, a bundle beat's on its `beat` record.
pub(crate) fn beat_when(p: &ExecProject, beat: &IndexBeat) -> Option<Arc<Slot>> {
    p.conds.beat(beat)?.when.clone()
}

/// dsl 0.23.0 §3: the beat's `also: true` — a scene's `meta.beat.also`, a
/// bundle beat's `also` on its `beat` record. An entry beat never rides
/// along.
pub fn beat_also(p: &ExecProject, beat: &IndexBeat) -> bool {
    match beat.kind {
        BeatKind::Scene => p
            .artifacts
            .get(&beat.document)
            .and_then(|d| d.pointer("/meta/beat/also"))
            .and_then(Json::as_bool)
            .unwrap_or(false),
        BeatKind::Bundle => p
            .artifacts
            .get(&beat.document)
            .and_then(|d| d.get("commands")?.as_array())
            .and_then(|cs| {
                cs.iter().find(|c| {
                    c.get("kind").and_then(Json::as_str) == Some("beat")
                        && c.get("id").and_then(Json::as_str) == Some(beat.id.as_str())
                })
            })
            .and_then(|c| c.get("also"))
            .and_then(Json::as_bool)
            .unwrap_or(false),
        BeatKind::Entry => false,
    }
}

/// The artifact record kind that declares a lore beat: `entry` / `beat`.
pub fn record_kind(kind: BeatKind) -> &'static str {
    match kind {
        BeatKind::Bundle => "beat",
        BeatKind::Scene | BeatKind::Entry => "entry",
    }
}

/// dsl 0.21.0 §4: a candidate answers `occasion` and its `target` is absent
/// or equal to the raised one (a target on an untargeted occasion was
/// cleared at load, dsl 0.24.0 §6); dsl 0.26.0 §5: a kind target answers
/// every member of the kind ([`IndexBeat::answers`]).
pub fn is_candidate(b: &IndexBeat, occasion: &str, target: Option<&str>) -> bool {
    b.answers(occasion, target).is_some()
}

/// Every beat answering `occasion`/`target`, in `ProjectIndex.beats` order,
/// with the member it is judged (and presented) for: a `for="kind:<kind>"`
/// beat once per member, in member order (dsl 0.27.0 §3); any other beat
/// once, for the member a kind target binds ([`IndexBeat::answers`]).
fn answering<'p>(
    p: &'p ExecProject,
    occasion: &str,
    target: Option<&str>,
) -> Vec<(usize, &'p IndexBeat, Option<String>)> {
    let mut out = Vec::new();
    for (idx, beat) in p.index.beats.iter().enumerate() {
        let Some(member) = beat.answers(occasion, target) else {
            continue;
        };
        match &beat.for_kind {
            Some(fk) => out.extend(fk.members.iter().map(|m| (idx, beat, Some(m.clone())))),
            None => out.push((idx, beat, member.map(str::to_string))),
        }
    }
    out
}

/// Every candidate for `occasion`/`target` with its verdict, in selection
/// order ([`lute_manifest::semantics::beats::selection_order`]): priority descending, a
/// kind beat after the other beats of its priority and a sub-kind's before
/// its parent's (dsl 0.26.0 §5, dsl 0.27.0), then `ProjectIndex.beats` order. Pure over
/// the world — what a play step presents from and what `lute calendar`
/// evaluates at every cell (dsl 0.23.0 §1).
pub fn eligible_at(
    p: &ExecProject,
    w: &World,
    occasion: &str,
    target: Option<&str>,
    observer: Option<SessionEvalObserver>,
) -> Vec<Candidate> {
    candidates(p, w, occasion, target, None, observer)
}

/// [`eligible_at`], the seam taken from `seam` (decided when the occasion
/// was raised) when given.
pub(super) fn candidates(
    p: &ExecProject,
    w: &World,
    occasion: &str,
    target: Option<&str>,
    seam: Option<&RaiseSeam>,
    observer: Option<SessionEvalObserver>,
) -> Vec<Candidate> {
    let mut eval = w
        .evaluator_with_schema(
            &p.eval_json,
            p.store_schemas[w.derive.unwrap_or(true) as usize].clone(),
            observer,
        )
        .with_visited(&w.visited);
    let out: Vec<(usize, Candidate)> = answering(p, occasion, target)
        .into_iter()
        .map(|(idx, beat, member)| {
            let decided = seam.map(|s| s.of(idx, member.as_deref()));
            let mut c = judge_with(p, w, &mut eval, beat, member.as_deref(), target, decided);
            if beat.for_kind.is_some() {
                c.for_member = member;
            }
            (idx, c)
        })
        .collect();
    // dsl 0.26.0 §5, dsl 0.27.0 (T3-10): the checker's order — priority
    // descending, member > sub-kind > kind, then index order.
    let order = lute_manifest::semantics::beats::selection_order(
        &out.iter()
            .map(|(idx, c)| {
                let kind = p.index.beats[*idx].target_kind.as_ref();
                (occasion, c.priority, kind.map(|k| k.members.as_slice()))
            })
            .collect::<Vec<_>>(),
    );
    lute_manifest::semantics::beats::reorder(out, &order)
        .into_iter()
        .map(|(_, c)| c)
        .collect()
}

/// dsl 0.28.0 (T1-22): the seam — the project's `terminal:`, then the
/// occasion's `raisedWhen` gate for the candidate's member — of every
/// candidate of one raise, decided when the occasion is raised. The raise's
/// own beats are judged under it: a `judge: before` judgement, or an
/// earlier beat of a `select: sequence` raise, that makes `terminal:` hold
/// does not close the beats of the raise already made.
pub struct RaiseSeam(BTreeMap<(usize, Option<String>), Option<crate::seam::Closed>>);
impl RaiseSeam {

    pub fn decide(
        p: &ExecProject,
        w: &World,
        occasion: &str,
        target: Option<&str>,
        observer: Option<SessionEvalObserver>,
    ) -> Self {
        let mut eval = w
            .evaluator_with_schema(
                &p.eval_json,
                p.store_schemas[w.derive.unwrap_or(true) as usize].clone(),
                observer,
            )
            .with_visited(&w.visited);
        RaiseSeam(
            answering(p, occasion, target)
                .into_iter()
                .map(|(idx, beat, member)| {
                    let closed = crate::seam::closed_in(
                        p,
                        &mut eval,
                        &beat.on,
                        member.as_deref().or(beat.target.as_deref()).or(target),
                    );
                    ((idx, member), closed)
                })
                .collect(),
        )
    }

    fn of(&self, idx: usize, member: Option<&str>) -> Option<crate::seam::Closed> {
        self.0
            .get(&(idx, member.map(str::to_string)))
            .cloned()
            .flatten()
    }
}

/// dsl 0.28.0 (T2-10): `c`'s verdict judged again in `w` as it stands —
/// a `select: sequence` beat just before its turn, after the raise's
/// earlier beats played — under the seam decided at the raise.
pub(super) fn rejudge(
    p: &ExecProject,
    w: &World,
    occasion: &str,
    target: Option<&str>,
    c: &Candidate,
    seam: &RaiseSeam,
    observer: Option<SessionEvalObserver>,
) -> Verdict {
    let Some((idx, beat)) = p
        .index
        .beats
        .iter()
        .enumerate()
        .find(|(_, b)| b.id == c.id && b.document == c.document && b.kind == c.kind)
    else {
        return c.verdict.clone();
    };
    let member = match &c.for_member {
        Some(m) => Some(m.as_str()),
        None => beat.answers(occasion, target).flatten(),
    };
    let mut eval = w
        .evaluator_with_schema(
            &p.eval_json,
            p.store_schemas[w.derive.unwrap_or(true) as usize].clone(),
            observer,
        )
        .with_visited(&w.visited);
    let decided = Some(seam.of(idx, member));
    judge_with(p, w, &mut eval, beat, member, target, decided).verdict
}

/// One beat's verdict in `w`, `member` bound as `occasion.target` (dsl
/// 0.26.0 §5): the seam (dsl 0.27.0 §4 — the project's `terminal:`, then
/// the occasion's `raisedWhen` gate for the beat's member, its own target,
/// else `raised`, the target the occasion is raised for), `once` spending,
/// `after:`, `spentBy`, then `when` — THE eligibility rule: what
/// [`eligible_at`] selects by, [`Session::eligibility`] reports, and `lute
/// trace` / `lute test` judge a presented scene, entry or bundle beat by
/// (their evaluator is the walk's own Machine, over the mocks; `w` then
/// carries the mocked `visited:` / quest states / read flags).
///
/// [`Session::eligibility`]: super::Session::eligibility
pub fn judge_beat<D: Driver>(
    p: &ExecProject,
    w: &World,
    eval: &mut Machine<D>,
    beat: &IndexBeat,
    member: Option<&str>,
    raised: Option<&str>,
) -> Candidate {
    judge_with(p, w, eval, beat, member, raised, None)
}

/// [`judge_beat`], its seam `decided` at the raise when given.
fn judge_with<D: Driver>(
    p: &ExecProject,
    w: &World,
    eval: &mut Machine<D>,
    beat: &IndexBeat,
    member: Option<&str>,
    raised: Option<&str>,
    decided: Option<Option<crate::seam::Closed>>,
) -> Candidate {
    let flag = |path: String| w.state.get(&path) == Some(&Value::Bool(true));
    // dsl 0.26.0 §5: bind before every eligibility read so the evaluator
    // snapshot and condition dump retain the engine's occasion root.
    eval.bind_occasion_target(member);
    // dsl 0.27.0 §4 (HW27-04): a beat of an occasion the engine would not
    // raise now is not eligible — the same seam `lute play` refuses a raise
    // by, decided by this evaluator (or when the raise was made).
    let seam = match decided {
        Some(closed) => closed,
        None => crate::seam::closed_in(
            p,
            eval,
            &beat.on,
            member.or(beat.target.as_deref()).or(raised),
        ),
    };
    // A scene's (or bundle beat's) `once` is spent by presenting it; an
    // entry's (dsl 0.22.0 §7) by its read flag; a clock period or a season
    // window (dsl 0.24.0 §1, 0.27.0 §5) until it moves on; a `share` key's
    // sibling names itself (dsl 0.25.0 §2). dsl 0.28.0: a `for` beat's
    // `once` is spent per member.
    let spent = crate::cadence::once_spent(p, w, beat, member);
    // `spentBy` — spent once its condition has held (for this member),
    // until its `once` period ends.
    let spent_by = crate::cadence::spent_by(p, w, eval, beat, member);
    // A scene's `after:` / a bundle beat's `after=` (dsl 0.25.0 §3).
    let after = matches!(beat.kind, BeatKind::Scene | BeatKind::Bundle)
        .then(|| p.artifacts.get(&beat.document))
        .flatten()
        .and_then(|doc| Some((beat_prereq_raw(doc, &beat.id)?, beat_prereq(doc, &beat.id)?)))
        .map(|(raw, f)| (raw, eval_prereq(p, &f, w), f));
    let when = |eval: &mut Machine<D>| {
        beat_when(p, beat).map(|cond| {
            let v = eval.eval_guard(&cond);
            (cond.raw().to_string(), v)
        })
    };
    let verdict = if let Some(closed) = seam {
        use crate::seam::Closed;
        match closed {
            Closed::Terminal(raw) => Verdict::Ineligible(Premise::Terminal {
                occasion: beat.on.clone(),
                raw,
            }),
            Closed::Gate { raw, reads } => Verdict::Ineligible(Premise::Gate {
                occasion: beat.on.clone(),
                raw,
                reads,
            }),
            Closed::Unknown(why) => Verdict::Unknown(why),
        }
    } else if let Some(reason) = spent {
        Verdict::Ineligible(Premise::Spent {
            once: beat.once.clone(),
            reason,
        })
    } else if let Some((raw, Ok(false), f)) = &after {
        Verdict::Ineligible(Premise::After {
            raw: raw.trim().to_string(),
            unmet: unmet_prereq(p, f, w),
            chapters: p.chapter_afters.contains(&beat.id),
        })
    } else if let Ok(Some(reason)) = &spent_by {
        Verdict::Ineligible(Premise::SpentBy(reason.clone()))
    } else if let Err(atoms) = &spent_by {
        Verdict::Unknown(format!(
            "`{}` (spentBy) evaluates unknown: {}",
            beat.spent_by.as_deref().unwrap_or_default(),
            describe_atoms(atoms)
        ))
    } else {
        match (when(eval), &after) {
            (Some((raw, Ok(false))), _) => Verdict::Ineligible(Premise::When { raw }),
            // An undecided `after` (a quest this project does not declare):
            // eligible only if it holds, so unknown unless `when` is false.
            (_, Some((raw, Err(atoms), _))) => Verdict::Unknown(format!(
                "`after: {}` evaluates unknown: {}",
                raw.trim(),
                describe_atoms(atoms)
            )),
            (None | Some((_, Ok(true))), _) => Verdict::Eligible,
            (Some((raw, Err(atoms))), _) => Verdict::Unknown(format!(
                "`{raw}` evaluates unknown: {}",
                describe_atoms(&atoms)
            )),
        }
    };
    Candidate {
        id: beat.id.clone(),
        kind: beat.kind,
        document: beat.document.clone(),
        priority: beat.priority,
        verdict,
        read: beat.kind == BeatKind::Entry
            && flag(crate::cadence::entry_read_flag(beat, member)),
        also: beat_also(p, beat),
        for_member: None,
        rejudged: false,
    }
}

/// The unknown `when` that could change this step's outcome, if any. On a
/// `select: first` occasion: a main beat ordered BEFORE the first
/// definitely-eligible main beat (a later one can never win), or any `also`
/// beat (each rides along on its own, dsl 0.23.0 §3). On `select: all` /
/// `sequence` any (the offered or presented list itself depends on it).
pub fn deciding_unknown(cands: &[Candidate], select: OccasionSelect) -> Option<&Candidate> {
    let unknown = |c: &&Candidate| matches!(c.verdict, Verdict::Unknown(_));
    if select != OccasionSelect::First {
        return cands.iter().find(unknown);
    }
    cands
        .iter()
        .filter(|c| !c.also)
        .take_while(|c| !matches!(c.verdict, Verdict::Eligible))
        .find(unknown)
        .or_else(|| cands.iter().filter(|c| c.also).find(unknown))
}

/// What an occasion presents without a `pick:` (dsl 0.21.0 §4, 0.23.0 §3),
/// as indices into `cands` (selection order), in presentation order:
/// `select: first` — the first eligible non-`also` beat (the winner), then
/// every eligible `also` beat; `select: sequence` — every eligible beat;
/// `select: all` — every eligible beat, which is the OFFERED list (the
/// player's `pick:` presents one of them). Decided from the verdicts when
/// the occasion is raised; a played `select: sequence` raise judges each
/// beat again at its turn (dsl 0.28.0, [`rejudge`]). Pure: `lute calendar`
/// reports it.
pub fn presented(select: OccasionSelect, cands: &[Candidate]) -> Vec<usize> {
    let eligible = |c: &Candidate| matches!(c.verdict, Verdict::Eligible);
    match select {
        OccasionSelect::First => cands
            .iter()
            .position(|c| !c.also && eligible(c))
            .into_iter()
            .chain((0..cands.len()).filter(|&i| cands[i].also && eligible(&cands[i])))
            .collect(),
        OccasionSelect::All | OccasionSelect::Sequence => {
            (0..cands.len()).filter(|&i| eligible(&cands[i])).collect()
        }
    }
}
