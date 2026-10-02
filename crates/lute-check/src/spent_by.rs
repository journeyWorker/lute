//! A beat's `spentBy` judged like its `when` (HW27-03). A `spentBy` beat is
//! spent by its condition instead of by being presented: once the condition
//! has held, the beat stays spent for its `once` period (`run` unless
//! written). So
//!
//! - one whose `when && !spentBy` is provably false is never eligible —
//!   `E-BEAT-UNREACHABLE` (`E-ENTRY-UNREACHABLE` for an entry), the verdict
//!   `when: false` gets;
//! - one whose `spentBy` already holds at the start of play — every state
//!   path at its declared default, the seeds the only facts, no scene
//!   presented, no entry read, a quest `active` when its `start` holds
//!   there and `unset` when it does not — is spent before it can play at
//!   all: `W-BEAT-SPENT-AT-START`. The common slip is reading `spentBy` as
//!   "repeat while" (`spentBy: "!run.balloonUp"`), or migrating
//!   `when="!holds(X)"` without dropping the `!`. A quest another document
//!   declares is judged by the project pass ([`check_project_spent_by`]);
//!   and
//! - one whose condition can turn false again after it has held, while the
//!   beat stays spent for the run (no `once` written), behaves unlike a
//!   condition judged afresh at each raise: `W-SPENT-BY-REVERSIBLE`
//!   (project-wide). It reads a fact some `::retract` / `::assert` or a
//!   directive's declared effect can undo, the state, facts or quests of a
//!   season (reset each time the season opens), or a quest that `rearm`
//!   returns to `unset`. The message names the rewrite: `once: false` +
//!   `when: "!(…)"` to judge it afresh, `once: season:<name>` for a season,
//!   `once: run` to keep the latch on purpose — a written `once` states the
//!   period, so it silences the warning.
//!
//! A `spentBy` whose literal comparison is already reported (a member
//! outside the path's domain, a mistyped literal) draws none of these: the
//! literal is the cause. One report per beat: an unreachable or
//! spent-at-start beat draws no `W-SPENT-BY-REVERSIBLE`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use cel_parser::ast::{operators as op, EntryExpr, Expr, IdedExpr};
use cel_parser::reference::Val;
use lute_core_span::{Diagnostic, Severity, Span};
use lute_manifest::types::Literal;
use lute_syntax::ast::{CelSlot, Document, Node, Quest};

use crate::cast::{Atom, FactWrite, Pol};
use crate::cel_expand::{expand_cel, DefTable};
use crate::check::FoldedEnv;
use crate::decide::{decide, decide_slot, DecideCtx, Decided};
use crate::match_check::DomainInfo;
use crate::meta::Namespace;

/// `W-BEAT-SPENT-AT-START`: a beat's `spentBy` already holds at the start
/// of play, so the beat is spent before it can ever be presented.
pub const W_BEAT_SPENT_AT_START: &str = "W-BEAT-SPENT-AT-START";

/// `W-SPENT-BY-REVERSIBLE`: a beat's `spentBy` can turn false again after
/// it has held, but the beat stays spent for the run (see the module doc).
pub const W_SPENT_BY_REVERSIBLE: &str = "W-SPENT-BY-REVERSIBLE";

/// One `spentBy` beat of a document.
struct SpentBeat<'d> {
    /// ``beat `b.chore` `` / ``entry `dial` ``.
    name: String,
    /// The unreachable verdict's code for its shape.
    code: &'static str,
    when: Option<&'d CelSlot>,
    spent: &'d CelSlot,
    /// The element's span: where its environment is found.
    at: Span,
    /// `once` is written beside `spentBy`: its period is the author's.
    once_written: bool,
}

/// Every `spentBy` beat of `doc`: the scene's own, its entries, its bundle
/// beats.
fn spent_beats<'d>(doc: &'d Document, folded: &'d FoldedEnv) -> Vec<SpentBeat<'d>> {
    let doc_id = folded.typed.id.as_deref().unwrap_or("this document");
    let mut beats = Vec::new();
    if let Some(b) = &folded.typed.beat {
        if let Some(s) = &b.spent_by {
            beats.push(SpentBeat {
                name: format!("beat `{}`", crate::beats::scene_beat_name(folded)),
                code: crate::beats::E_BEAT_UNREACHABLE,
                when: b.when.as_ref(),
                spent: s,
                at: s.span,
                once_written: serde_yaml::from_str::<serde_yaml::Mapping>(&doc.meta.raw_yaml)
                    .is_ok_and(|m| m.contains_key("once")),
            });
        }
    }
    for e in &doc.entries {
        if let Some(s) = &e.spent_by {
            beats.push(SpentBeat {
                name: format!("entry `{}`", e.id),
                code: crate::reachability::E_ENTRY_UNREACHABLE,
                when: e.when.as_ref(),
                spent: s,
                at: e.span,
                once_written: e.once.is_some(),
            });
        }
    }
    for b in &doc.beats {
        if let Some(s) = &b.spent_by {
            beats.push(SpentBeat {
                name: format!("beat `{}`", crate::bundles::bundle_beat_key(doc_id, &b.id)),
                code: crate::beats::E_BEAT_UNREACHABLE,
                when: b.when.as_ref(),
                spent: s,
                at: b.span,
                once_written: b.once.is_some(),
            });
        }
    }
    beats
}

/// The verdicts a beat's own document decides before the start of play is
/// judged: `Err(diagnostic)` for an unreachable beat, `Err(None)` when a
/// reported literal or a `when` already false alone is the verdict.
fn own_verdict(
    b: &SpentBeat<'_>,
    defs: &DefTable<'_>,
    ctx: &DecideCtx<'_>,
) -> Result<(), Option<Diagnostic>> {
    let raw = b.spent.raw.trim();
    // One report per mistake: a reported literal is the cause.
    if !crate::decide::analyze_literal_comparisons(raw, defs, ctx)
        .hits
        .is_empty()
    {
        return Err(None);
    }
    let when = b.when.map(|w| w.raw.trim()).filter(|w| !w.is_empty());
    let decides = |c: &str| decide_slot(c, defs, ctx);
    // A `when` already false alone is its own verdict.
    if when.is_some_and(|w| decides(w) == Some(Decided::Bool(false))) {
        return Err(None);
    }
    let eligible = match when {
        Some(w) => format!("({w}) && !({raw})"),
        None => format!("!({raw})"),
    };
    if decides(&eligible) != Some(Decided::Bool(false)) {
        return Ok(());
    }
    let whenever = match when {
        Some(w) => format!("whenever its `when` `{w}` does"),
        None => "always".to_string(),
    };
    Err(Some(crate::reachability::diag(
        b.code,
        Severity::Error,
        format!(
            "{} is never eligible: its `spentBy: {raw}` holds {whenever}, and a beat is spent \
             once its `spentBy` has held",
            b.name
        ),
        b.spent.span,
    )))
}

/// The `spentBy` verdicts of every beat of `doc` that its own document
/// decides (see the module doc); a quest another document declares is left
/// to [`check_project_spent_by`].
pub(crate) fn check_spent_by(
    doc: &Document,
    folded: &FoldedEnv,
    defs: &DefTable<'_>,
    ctx: &DecideCtx<'_>,
) -> Vec<Diagnostic> {
    let beats = spent_beats(doc, folded);
    let mut out = Vec::new();
    if beats.is_empty() {
        return out;
    }
    let mut world = StartWorld::new(folded);
    let quests = world.quest_starts(folded, doc.quests.iter(), &children([doc]), ctx.params);
    for b in beats.iter().filter(|b| !b.spent.raw.trim().is_empty()) {
        // dsl 0.28.0: judged over the beat's own environment.
        let own = DecideCtx {
            schema: &folded.env_at(b.at).state,
            dollar: None,
            params: ctx.params,
            facts: None,
        };
        match own_verdict(b, defs, &own) {
            Err(d) => out.extend(d),
            Ok(()) => {
                let raw = b.spent.raw.trim();
                if world.holds(folded, raw, b.at, b.spent.span, &quests, ctx.params) {
                    out.push(spent_at_start(&b.name, raw, b.spent.span));
                }
            }
        }
    }
    out
}

/// `W-BEAT-SPENT-AT-START` for `name`'s `spentBy: raw`, with the rewrite.
fn spent_at_start(name: &str, raw: &str, span: Span) -> Diagnostic {
    let hint = match raw
        .strip_prefix('!')
        .map(str::trim)
        .filter(|r| !r.is_empty())
    {
        Some(r) => format!(
            " — `spentBy` names what spends the beat, not what keeps it: did you mean \
             `spentBy: \"{r}\"`, or `when: \"{raw}\"` (with `once: false` to repeat it) for a \
             beat that plays while it holds?"
        ),
        None => format!(
            " — to offer the beat while it does not hold, drop `spentBy` and write `when: \
             \"{}\"` (with `once: false` to repeat it)",
            negated(raw)
        ),
    };
    crate::reachability::diag(
        W_BEAT_SPENT_AT_START,
        Severity::Warning,
        format!(
            "{name} is spent before it can play: its `spentBy: {raw}` already holds at the start \
             (every state path at its default, only the seed facts, each quest `unset` until its \
             `start` holds), and a `spentBy` beat stays spent once its condition has held{hint}"
        ),
        span,
    )
}

/// `!raw`, bracketed unless `raw` is one read or call.
fn negated(raw: &str) -> String {
    let mut arena = lute_cel::CelArena::default();
    let simple = lute_cel::parse_slot_marked_refs(&mut arena, raw)
        .and_then(|h| arena.get(h))
        .is_some_and(|e| match &e.expr {
            Expr::Ident(_) | Expr::Select(_) => true,
            Expr::Call(c) => c.func_name.starts_with(|ch: char| ch.is_ascii_alphabetic()),
            _ => false,
        });
    if simple {
        format!("!{raw}")
    } else {
        format!("!({raw})")
    }
}

/// The quests a quest of `docs` names as a `<objective quest=…>` child: a
/// child activates with its parent, never at the start by its own `start`.
fn children<'d>(docs: impl IntoIterator<Item = &'d Document>) -> BTreeSet<String> {
    docs.into_iter()
        .flat_map(|d| &d.quests)
        .flat_map(|q| &q.body)
        .filter_map(|n| match n {
            Node::Objective(o) => o.quest.clone().filter(|c| !c.is_empty()),
            _ => None,
        })
        .collect()
}

/// What `quest.<id>.state` reads at the start of play for each quest known:
/// `active` when its `start` holds there, `unset` when it waits (for its
/// `start`, or for an `::accept`); `None` when that is not decided (a
/// subquest, an externally accepted quest, a `start` undecided at start).
type QuestStarts = BTreeMap<String, Option<&'static str>>;

/// The start of play in one document's environment (see the module doc).
struct StartWorld {
    env: crate::FactEnv,
    pins: BTreeMap<String, Val>,
}

impl StartWorld {
    fn new(folded: &FoldedEnv) -> Self {
        Self {
            env: initial_facts(folded),
            pins: start_values(folded),
        }
    }

    /// Decide `raw` at the start of play: over the environment at `at`, the
    /// seed facts placed at `slot`, `quests` pinned to their start states.
    fn decide(
        &mut self,
        folded: &FoldedEnv,
        raw: &str,
        at: Span,
        slot: Span,
        quests: &QuestStarts,
        params: &BTreeMap<String, DomainInfo>,
    ) -> Option<Decided> {
        let defs = DefTable {
            bodies: &folded.def_bodies,
            params: &folded.env.def_params,
        };
        let path = Path::new("");
        let mut must = crate::MustMap::default();
        must.insert(path, slot, seeds(folded));
        self.env.must = must;
        let ctx = DecideCtx {
            schema: &folded.env_at(at).state,
            dollar: None,
            params,
            facts: Some(crate::FactScope {
                env: &self.env,
                vocab: &folded.env.rel_vocab,
                path,
                span: slot,
                wip: false,
            }),
        };
        decide_at_start(raw, &defs, &self.pins, quests, &ctx)
    }

    /// `raw` already holds at the start of play.
    fn holds(
        &mut self,
        folded: &FoldedEnv,
        raw: &str,
        at: Span,
        slot: Span,
        quests: &QuestStarts,
        params: &BTreeMap<String, DomainInfo>,
    ) -> bool {
        self.decide(folded, raw, at, slot, quests, params) == Some(Decided::Bool(true))
    }

    /// The start state of every quest of `quests` (declared in `folded`'s
    /// document), `children` being every quest some objective names.
    fn quest_starts<'q>(
        &mut self,
        folded: &FoldedEnv,
        quests: impl Iterator<Item = &'q Quest>,
        children: &BTreeSet<String>,
        params: &BTreeMap<String, DomainInfo>,
    ) -> QuestStarts {
        let none = QuestStarts::new();
        quests
            .filter(|q| !q.id.is_empty())
            .map(|q| {
                let state = if children.contains(&q.id) {
                    None
                } else {
                    match &q.start {
                        None if q.accepted_externally() => None,
                        None => Some("unset"),
                        Some(s) => {
                            match self.decide(folded, s.raw.trim(), q.span, s.span, &none, params) {
                                Some(Decided::Bool(true)) => Some("active"),
                                Some(Decided::Bool(false)) => Some("unset"),
                                _ => None,
                            }
                        }
                    }
                };
                (q.id.clone(), state)
            })
            .collect()
    }
}

/// The project half of the `spentBy` verdicts (see the module doc), over
/// one resolved root: `docs` and their folded environments, in the same
/// order, `root` the directory messages name files from.
///
/// - `W-BEAT-SPENT-AT-START` for a `spentBy` that holds at the start only
///   once another document's quest is known (its own document left the
///   read undecided).
/// - `W-SPENT-BY-REVERSIBLE` for one with no `once` written that can turn
///   false again after it has held.
pub fn check_project_spent_by(
    root: &Path,
    docs: &[(PathBuf, Document)],
    foldeds: &[&FoldedEnv],
) -> Vec<(PathBuf, Diagnostic)> {
    let mut out = Vec::new();
    let spent: Vec<(usize, Vec<SpentBeat<'_>>)> = docs
        .iter()
        .zip(foldeds)
        .enumerate()
        .map(|(i, ((_, doc), folded))| (i, spent_beats(doc, folded)))
        .filter(|(_, b)| !b.is_empty())
        .collect();
    if spent.is_empty() {
        return out;
    }
    let params = BTreeMap::new();
    let all_children = children(docs.iter().map(|(_, d)| d));
    let mut project_quests = QuestStarts::new();
    // Quest name → (its `rearm` is written, its `season:<name>` tier).
    let mut quest_cadence: BTreeMap<&str, (bool, Option<&str>)> = BTreeMap::new();
    for ((_, doc), folded) in docs.iter().zip(foldeds) {
        if doc.quests.is_empty() {
            continue;
        }
        let mut world = StartWorld::new(folded);
        project_quests.extend(world.quest_starts(
            folded,
            doc.quests.iter(),
            &all_children,
            &params,
        ));
        for q in &doc.quests {
            let season = q.tier.as_ref().and_then(|(t, _)| t.strip_prefix("season:"));
            quest_cadence.insert(q.id.as_str(), (q.rearm.is_some(), season));
        }
    }
    let effects = crate::directive_facts::root_table(foldeds.iter().copied());
    let writes = crate::cast::fact_writes(docs, &effects);
    for (i, beats) in spent {
        let (path, doc) = &docs[i];
        let folded = foldeds[i];
        let defs = DefTable {
            bodies: &folded.def_bodies,
            params: &folded.env.def_params,
        };
        let mut world = StartWorld::new(folded);
        let own_quests = world.quest_starts(folded, doc.quests.iter(), &children([doc]), &params);
        for b in beats.iter().filter(|b| !b.spent.raw.trim().is_empty()) {
            let ctx = DecideCtx {
                schema: &folded.env_at(b.at).state,
                dollar: None,
                params: &params,
                facts: None,
            };
            if own_verdict(b, &defs, &ctx).is_err() {
                continue;
            }
            let raw = b.spent.raw.trim();
            // Its own document's check already warned.
            if world.holds(folded, raw, b.at, b.spent.span, &own_quests, &params) {
                continue;
            }
            if world.holds(folded, raw, b.at, b.spent.span, &project_quests, &params) {
                out.push((path.clone(), spent_at_start(&b.name, raw, b.spent.span)));
                continue;
            }
            if b.once_written {
                continue;
            }
            let Some(expr) = expanded(raw, &defs) else {
                continue;
            };
            let mut reads = Reads::default();
            reads.walk(&expr.expr, Pol::Pos);
            if let Some(undo) = reads.undo(folded, &quest_cadence, &writes) {
                out.push((
                    path.clone(),
                    crate::reachability::diag(
                        W_SPENT_BY_REVERSIBLE,
                        Severity::Warning,
                        reversible_message(&b.name, raw, &undo, root),
                        b.spent.span,
                    ),
                ));
            }
        }
    }
    out
}

/// `raw` with its `@def`s expanded, parsed.
fn expanded(raw: &str, defs: &DefTable<'_>) -> Option<IdedExpr> {
    let mut stack = Vec::new();
    let text = expand_cel(raw, defs, Some("$"), &mut stack).unwrap_or_else(|_| raw.to_string());
    let mut arena = lute_cel::CelArena::default();
    let handle = lute_cel::parse_slot_marked_refs(&mut arena, &text)?;
    arena.get(handle).cloned()
}

/// What a `spentBy` condition reads: its state paths and its fact atoms,
/// each atom with the direction a write must move it to make the condition
/// false.
#[derive(Default)]
struct Reads {
    paths: Vec<String>,
    atoms: Vec<Atom>,
}

impl Reads {
    fn atom(&mut self, e: &Expr, pol: Pol) -> bool {
        let Expr::Call(q) = e else { return false };
        if q.target.is_some()
            || !matches!(q.func_name.as_str(), "holds" | "count" | "countDistinct")
        {
            return false;
        }
        if let Some(query) = crate::fact_env::QueryPattern::from_call(q) {
            self.atoms.push(Atom {
                rel: query.relation,
                args: query.args,
                pol,
            });
        }
        true
    }

    fn walk(&mut self, e: &Expr, pol: Pol) {
        if let Some(p) = crate::cel_paths::select_path(e) {
            self.paths.push(p);
            return;
        }
        match e {
            Expr::Call(c) if c.target.is_none() => {
                match (c.func_name.as_str(), c.args.as_slice()) {
                    (n, [a]) if n == op::LOGICAL_NOT => self.walk(&a.expr, pol.flip()),
                    (n, [a, b]) if n == op::LOGICAL_AND || n == op::LOGICAL_OR => {
                        self.walk(&a.expr, pol);
                        self.walk(&b.expr, pol);
                    }
                    // `count(…) >= n` holds while facts are added; `<=` while
                    // they are taken away.
                    (n, [a, b])
                        if [op::GREATER, op::GREATER_EQUALS, op::LESS, op::LESS_EQUALS]
                            .contains(&n) =>
                    {
                        let grows = n == op::GREATER || n == op::GREATER_EQUALS;
                        for (side, other, left) in [(a, b, true), (b, a, false)] {
                            let up = if grows == left { pol } else { pol.flip() };
                            if matches!(&side.expr, Expr::Call(q) if q.func_name != "holds")
                                && self.atom(&side.expr, up)
                            {
                                self.walk(&other.expr, Pol::Both);
                                return;
                            }
                        }
                        self.walk(&a.expr, Pol::Both);
                        self.walk(&b.expr, Pol::Both);
                    }
                    // A bare `count(…)` (outside a comparison) moves either way.
                    (n, _) if self.atom(e, if n == "holds" { pol } else { Pol::Both }) => {}
                    _ => c.args.iter().for_each(|a| self.walk(&a.expr, Pol::Both)),
                }
            }
            Expr::Call(c) => {
                c.target.iter().for_each(|t| self.walk(&t.expr, Pol::Both));
                c.args.iter().for_each(|a| self.walk(&a.expr, Pol::Both));
            }
            Expr::Select(s) => self.walk(&s.operand.expr, Pol::Both),
            Expr::List(l) => l
                .elements
                .iter()
                .for_each(|x| self.walk(&x.expr, Pol::Both)),
            Expr::Map(m) => m.entries.iter().for_each(|x| match &x.expr {
                EntryExpr::MapEntry(m) => {
                    self.walk(&m.key.expr, Pol::Both);
                    self.walk(&m.value.expr, Pol::Both);
                }
                EntryExpr::StructField(f) => self.walk(&f.value.expr, Pol::Both),
            }),
            Expr::Comprehension(c) => {
                for x in [
                    &c.iter_range,
                    &c.accu_init,
                    &c.loop_cond,
                    &c.loop_step,
                    &c.result,
                ] {
                    self.walk(&x.expr, Pol::Both);
                }
            }
            _ => {}
        }
    }

    /// Why the condition can turn false again after it has held within a
    /// run, if it can: a season it reads opening again, a write that moves
    /// a fact it reads the wrong way, a quest it reads rearming.
    fn undo<'w>(
        &self,
        folded: &FoldedEnv,
        quests: &BTreeMap<&str, (bool, Option<&str>)>,
        writes: &'w [FactWrite],
    ) -> Option<Undo<'w>> {
        let vocab = &folded.env.rel_vocab;
        let quest_of = |p: &str| {
            let id = p.strip_prefix("quest.")?.split('.').next()?;
            Some((id.to_string(), quests.get(id)?))
        };
        // A season's reset — the rewrite is its `once:` period.
        for p in &self.paths {
            if let Some(s) = p
                .strip_prefix("season.")
                .or_else(|| p.strip_prefix("prev.season."))
                .and_then(|r| r.split('.').next())
            {
                return Some(Undo::Season {
                    season: s.to_string(),
                    what: format!("`{p}` goes back to its default"),
                });
            }
            if let Some((id, (_, Some(s)))) = quest_of(p) {
                return Some(Undo::Season {
                    season: s.to_string(),
                    what: format!("quest `{id}` (`tier=\"season:{s}\"`) returns to `unset`"),
                });
            }
        }
        for a in &self.atoms {
            let season = vocab
                .relations
                .get(&a.rel)
                .and_then(|d| d.tier.as_deref())
                .and_then(|t| t.strip_prefix("season:"));
            if let Some(s) = season {
                return Some(Undo::Season {
                    season: s.to_string(),
                    what: format!(
                        "relation `{}` (`tier: season:{s}`) goes back to its seed facts",
                        a.rel
                    ),
                });
            }
        }
        // A write that moves an atom the wrong way: the atom itself, or a
        // derived atom through its rules.
        let derived = |rel: &str| {
            vocab.relations.get(rel).is_some_and(|d| d.derive) || vocab.unparsed_heads.contains(rel)
        };
        let any_derived = self.atoms.iter().any(|a| derived(&a.rel));
        let falsifies = |e: &crate::cast::Effect| {
            self.atoms.iter().any(|a| {
                a.rel == e.rel
                    && a.args.len() == e.args.len()
                    && a.args
                        .iter()
                        .zip(&e.args)
                        .all(|(x, y)| x.is_none() || y.is_none() || x == y)
                    && match a.pol {
                        Pol::Both => true,
                        Pol::Pos => !e.up,
                        Pol::Neg => e.up,
                    }
            })
        };
        if !self.atoms.is_empty() {
            for w in writes {
                let direct = crate::cast::Effect {
                    rel: w.rel.clone(),
                    args: w.args.clone(),
                    up: w.up,
                };
                let hit = if any_derived {
                    crate::cast::write_effects(vocab, &w.rel, w.args.clone(), w.up)
                        .iter()
                        .any(falsifies)
                } else {
                    falsifies(&direct)
                };
                if hit {
                    return Some(Undo::Write(w));
                }
            }
        }
        // A quest `rearm` returns to `unset`.
        self.paths.iter().find_map(|p| match quest_of(p) {
            Some((id, (true, _))) => Some(Undo::Rearm(id)),
            _ => None,
        })
    }
}

/// How a `spentBy` condition can turn false again after it has held.
enum Undo<'w> {
    /// A season's reset, each time it opens: `what` goes back.
    Season { season: String, what: String },
    /// A fact write that moves a fact it reads the wrong way.
    Write(&'w FactWrite),
    /// A quest it reads that `rearm` returns to `unset`.
    Rearm(String),
}

/// The `W-SPENT-BY-REVERSIBLE` message: why the latch differs from judging
/// the condition afresh, and the rewrite.
fn reversible_message(name: &str, raw: &str, undo: &Undo<'_>, root: &Path) -> String {
    let head = format!(
        "{name} stays spent for the rest of the run once its `spentBy: {raw}` has held, but"
    );
    let afresh = format!(
        "to offer it again while the condition does not hold, drop `spentBy` and write `once: \
         false` with `when: \"{}\"`; to keep it spent for the run on purpose, write `once: run`",
        negated(raw)
    );
    match undo {
        Undo::Season { season, what } => format!(
            "{head} {what} each time the `{season}` season opens — write `once: season:{season}` \
             beside `spentBy` to keep it spent for one `{season}` window only, or `once: run` to \
             keep it spent for the run on purpose"
        ),
        Undo::Write(w) => {
            let file = w.path.strip_prefix(root).unwrap_or(&w.path);
            format!(
                "{head} {} ({}:{}) can make the condition false again — {afresh}",
                w.text,
                file.display(),
                w.span.line
            )
        }
        Undo::Rearm(q) => format!(
            "{head} quest `{q}` returns to `unset` when its `rearm` holds, and the condition can \
             turn false again — {afresh}"
        ),
    }
}

/// The fact envelope of a run's first moment in `folded`'s document: the
/// seeds (and what the rules derive from them) may hold, nothing any
/// content asserts does yet. A relation the engine writes (unbounded) stays
/// undecided. Its must set is the seeds, placed at the slot being judged.
fn initial_facts(folded: &FoldedEnv) -> crate::FactEnv {
    let mut vocab = crate::RootVocab::default();
    vocab.add(&folded.env.rel_vocab, &folded.env.domains);
    let may = crate::MaySet::build(&vocab, std::iter::empty(), &BTreeSet::new());
    crate::FactEnv::new(may, crate::MustMap::default())
}

/// The document's seed facts, as must facts.
fn seeds(folded: &FoldedEnv) -> Vec<crate::fact_env::MustFact> {
    folded
        .env
        .rel_vocab
        .facts
        .iter()
        .filter_map(|s| crate::GroundFact::from_pattern(&s.fact))
        .map(|fact| crate::fact_env::MustFact {
            fact,
            provenance: crate::fact_env::Provenance::Seed,
        })
        .collect()
}

/// The value every persistent state path holds at the start of play: its
/// declared scalar default (`run.*` / `user.*` / `app.*` / `season.*`). A path without
/// one stays a read with no value.
fn start_values(folded: &FoldedEnv) -> BTreeMap<String, Val> {
    folded
        .env
        .state
        .decls
        .iter()
        .filter(|(_, d)| {
            matches!(
                d.namespace,
                Namespace::Run | Namespace::User | Namespace::App | Namespace::Season
            )
        })
        .filter_map(|(path, d)| {
            let v = match d.default.as_ref()? {
                Literal::Bool(b) => Val::Boolean(*b),
                Literal::Int(n) => Val::Int(*n),
                Literal::Double(n) => Val::Double(*n),
                Literal::Str(s) => Val::String(s.clone()),
                Literal::List(_) | Literal::Map(_) => return None,
            };
            Some((path.clone(), v))
        })
        .collect()
}

/// Decide `raw` at the start of play: its `@def`s expanded, every state
/// path with a start value replaced by it, every quest `quests` knows at its
/// start state, every entry unread, no scene visited — then the ordinary
/// decision.
fn decide_at_start(
    raw: &str,
    defs: &DefTable<'_>,
    pins: &BTreeMap<String, Val>,
    quests: &QuestStarts,
    ctx: &DecideCtx<'_>,
) -> Option<Decided> {
    let mut ided = expanded(raw, defs)?;
    pin_start(&mut ided, pins, quests);
    decide(&ided.expr, ctx)
}

/// Replace, in place, every read [`decide_at_start`] knows the start value of.
fn pin_start(e: &mut IdedExpr, pins: &BTreeMap<String, Val>, quests: &QuestStarts) {
    if let Some(path) = crate::cel_paths::select_path(&e.expr) {
        let quest = || {
            let id = path.strip_prefix("quest.")?.split('.').next()?;
            *quests.get(id)?
        };
        let start = if let Some(v) = pins.get(&path) {
            Some(v.clone())
        } else if crate::cel_paths::is_reserved_quest_state(&path) {
            quest().map(|s| Val::String(s.to_string()))
        } else if crate::cel_paths::is_reserved_quest_objective_done(&path) {
            quest().map(|_| Val::Boolean(false))
        } else if crate::cel_paths::is_reserved_entry_read(&path)
            || crate::cel_paths::is_entry_ever_read(&path)
        {
            Some(Val::Boolean(false))
        } else {
            None
        };
        if let Some(v) = start {
            e.expr = Expr::Literal(v);
        }
        return;
    }
    match &mut e.expr {
        Expr::Call(c) => {
            if c.target.is_none() && c.func_name == crate::cel_resolve::VISITED_FN {
                e.expr = Expr::Literal(Val::Boolean(false));
                return;
            }
            if let Some(t) = c.target.as_deref_mut() {
                pin_start(t, pins, quests);
            }
            for a in &mut c.args {
                pin_start(a, pins, quests);
            }
        }
        Expr::List(l) => {
            for x in &mut l.elements {
                pin_start(x, pins, quests);
            }
        }
        _ => {}
    }
}
