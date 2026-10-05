use std::collections::{BTreeMap, BTreeSet};
use lute_cel::CelArena;
use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_syntax::ast::{Document, Node};
use super::quest_tree::tree_diag;

/// dsl 0.23.0 §6 (0.23.1): a subquest's `tier` must equal its parent's.
/// A run-tier quest resets to `unset` at every `newRun`; a user-tier one
/// keeps its status. Mixed, the tree locks: a run-tier parent's failure
/// cascade-fails its user-tier child for good (and the parent, reset next
/// run, waits on a child that stays failed); a user-tier parent that ends
/// never re-activates its run-tier children once they reset.
pub const E_QUEST_TIER_MIX: &str = "E-QUEST-TIER-MIX";

/// A quest's effective tier: `run` when authored `tier="run"`, else `user`.
pub(crate) fn quest_tier(q: &lute_syntax::ast::Quest) -> &'static str {
    match q.tier.as_ref() {
        Some((t, _)) if t == "run" => "run",
        _ => "user",
    }
}

pub(crate) fn tier_mix_diag(
    parent: &str,
    parent_tier: &str,
    child: &str,
    child_tier: &str,
    span: Span,
) -> Diagnostic {
    let lock = if parent_tier == "run" {
        format!(
            "when `{parent}` ends, its end cascades into `{child}`, which keeps that status \
             across runs, so `{parent}` restarts next run waiting on a child that never \
             becomes active again"
        )
    } else {
        format!(
            "`{child}` resets to `unset` at every new run while `{parent}` keeps its status, so \
             once `{parent}` has ended `{child}` is never activated again"
        )
    };
    tree_diag(
        E_QUEST_TIER_MIX,
        format!(
            "subquest `{child}` is tier `{child_tier}` but its parent `{parent}` is tier \
             `{parent_tier}`: {lock} — give both quests the same `tier` (dsl 0.23.0 §6)"
        ),
        span,
    )
}

/// [`E_QUEST_TIER_MIX`] for every `<objective quest="c">` whose parent and
/// child are both declared in `doc` (the per-file half; cross-document
/// edges are [`check_project_quest_tree`]'s).
pub fn check_doc_quest_tiers(doc: &Document) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for parent in &doc.quests {
        for node in &parent.body {
            let Node::Objective(o) = node else { continue };
            let Some(child) = o.quest.as_deref().filter(|c| !c.is_empty()) else {
                continue;
            };
            let Some(child_q) = doc.quests.iter().find(|q| q.id == child) else {
                continue;
            };
            let (pt, ct) = (quest_tier(parent), quest_tier(child_q));
            if pt != ct {
                out.push(tier_mix_diag(&parent.id, pt, child, ct, o.quest_span));
            }
        }
    }
    out
}

/// A subquest's `rearm=` has no round to start: a child activates with its
/// parent (or on `::accept` while the parent is active), so once the parent
/// has ended a rearmed child returns to `unset` and never activates again.
pub const E_SUBQUEST_REARM: &str = "E-SUBQUEST-REARM";

/// A `rearm=` that decides to a constant never turns from false to true, so
/// the quest never rearms.
pub const W_QUEST_REARM_CONSTANT: &str = "W-QUEST-REARM-CONSTANT";

pub(crate) fn subquest_rearm_diag(child: &str, parent: &str, span: Span) -> Diagnostic {
    tree_diag(
        E_SUBQUEST_REARM,
        format!(
            "subquest `{child}` has `rearm=`, but a subquest activates with its parent \
             `{parent}`: once `{parent}` has ended, a rearmed `{child}` returns to `unset` and \
             never activates again — put `rearm=` on `{parent}` (rearming it resets the round), \
             or make `{child}` a quest of its own"
        ),
        span,
    )
}

/// Per document: [`W_QUEST_REARM_CONSTANT`] for each quest whose `rearm=`
/// decides to `true` or `false` without facts (`rearm="true"`, a def or a
/// comparison that folds to a constant), and [`E_SUBQUEST_REARM`] for a
/// `rearm=` on a quest an `<objective quest=…>` of this document names
/// (cross-document parents are [`check_project_quest_tree`]'s).
pub fn check_doc_quest_rearm(doc: &Document, folded: &crate::check::FoldedEnv) -> Vec<Diagnostic> {
    use crate::decide::{decide_slot, DecideCtx, Decided};
    let mut out = Vec::new();
    let defs = crate::cel_expand::DefTable {
        bodies: &folded.def_bodies,
        params: &folded.env.def_params,
    };
    let params = BTreeMap::new();
    let ctx = DecideCtx {
        schema: &folded.env.state,
        dollar: None,
        params: &params,
        facts: None,
    };
    for q in &doc.quests {
        let Some(rearm) = &q.rearm else { continue };
        // A literal no member matches is the error that makes it constant;
        // it is reported at the literal.
        if crate::decide::analyze_literal_comparisons(&rearm.raw, &defs, &ctx).owns_dead_guard() {
            continue;
        }
        if let Some(Decided::Bool(b)) = decide_slot(&rearm.raw, &defs, &ctx) {
            let never = if b {
                "is always true, so it never turns from false to true"
            } else {
                "is never true"
            };
            out.push(Diagnostic {
                code: W_QUEST_REARM_CONSTANT.to_string(),
                severity: Severity::Warning,
                message: format!(
                    "`rearm=\"{}\"` on quest `{}` {never}, so the quest never rearms — rearm \
                     fires each time its condition turns from false to true; write the \
                     condition that starts a new round (`rearm=\"clock.weekday == 0\"`), or \
                     remove `rearm=`",
                    rearm.raw.trim(),
                    q.id
                ),
                evidence: None,
                span: rearm.span,
                layer: Layer::Logic,
                fixits: Vec::new(),
                provenance: None,
                covered: Vec::new(),
                related: Vec::new(),
            });
        }
    }
    for parent in &doc.quests {
        for node in &parent.body {
            let Node::Objective(o) = node else { continue };
            let Some(child) = o.quest.as_deref().filter(|c| !c.is_empty()) else {
                continue;
            };
            if let Some(rearm) = doc
                .quests
                .iter()
                .find(|q| q.id == child)
                .and_then(|q| q.rearm.as_ref())
            {
                out.push(subquest_rearm_diag(child, &parent.id, rearm.span));
            }
        }
    }
    out
}

/// dsl 0.27.0 §9 (round-5 T3-24): a `<quest>` with no `tier=` (and none
/// from `defaults.questTier`) is user-tier, but every state its conditions
/// read is run-tier — it looks meant to reset each run.
pub const W_QUEST_TIER_IMPLICIT: &str = "W-QUEST-TIER-IMPLICIT";

/// [`W_QUEST_TIER_IMPLICIT`] for every quest of `doc` whose tier is implicit
/// (runs after [`crate::meta::apply_quest_tier_default`], so `tier == None`
/// means neither the quest nor the project wrote one) and whose `start`,
/// `fail`, and objectives' `done` / `by` / `until` (`@def`s expanded) read
/// at least one run-tier piece of state and no user- or app-tier one
/// ([`crate::beats::read_tier`]). `clock.*` is derived from the clock's
/// `day` path and reads its tier; `visited()` is save state (a new run does
/// not clear it, dsl 0.21.0 §7a.1), so it reads as neither.
///
/// A `<objective quest="c">` reads `c`'s tier (ML-F3): an explicit one, or
/// run when `c` is itself flagged here — so a parent of run-looking
/// subquests is flagged with them, and each warning names the quests of its
/// tree that must change together (a subquest's tier must equal its
/// parent's, [`E_QUEST_TIER_MIX`]). A quest that reads nothing takes no
/// side: it neither keeps its parent user-tier nor is flagged alone, but it
/// is flagged with a flagged quest of its tree (round-4 League R1). A
/// run-looking subquest whose parent stays user-tier is not flagged: alone
/// it cannot change. A quest of this document with an explicit tier
/// classifies its `quest.<id>.*`; any other quest's is unknown.
pub fn check_quest_tier_implicit(
    doc: &Document,
    folded: &crate::check::FoldedEnv,
) -> Vec<Diagnostic> {
    use crate::beats::{read_tiers, ReadTier, UserTier};
    let quests: BTreeMap<&str, bool> = doc
        .quests
        .iter()
        .filter_map(|q| q.tier.as_ref().map(|(t, _)| (q.id.as_str(), t != "run")))
        .collect();
    let tiers = UserTier {
        relations: &folded.env.rel_vocab.relations,
        quests: &quests,
    };
    let defs = crate::cel_expand::DefTable {
        bodies: &folded.def_bodies,
        params: &folded.env.def_params,
    };
    let clock_tier = folded.env.clock.as_ref().map(|c| {
        let head = c.day.split('.').next().unwrap_or("");
        match head {
            "run" => ReadTier::Run,
            "user" => ReadTier::User,
            "app" => ReadTier::App,
            _ => ReadTier::Other,
        }
    });
    // Each implicit quest's own reads and its `quest=` children, and the
    // seasons whose state it reads (dsl 0.28.0 §5: such a read is as
    // transient as a run read, and names the tier to suggest).
    let mut own: BTreeMap<&str, (Vec<ReadTier>, Vec<&str>)> = BTreeMap::new();
    let mut seasons_read: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
    for q in doc
        .quests
        .iter()
        .filter(|q| q.tier.is_none() && !q.id.is_empty())
    {
        let mut children = Vec::new();
        let slots = q
            .start
            .iter()
            .chain(&q.fail)
            .chain(q.body.iter().flat_map(|n| {
                let Node::Objective(o) = n else {
                    return Vec::new();
                };
                children.extend(o.quest.as_deref().filter(|c| !c.is_empty()));
                std::iter::once(&o.done)
                    .chain(&o.by)
                    .chain(&o.until)
                    .collect()
            }))
            .collect::<Vec<_>>();
        let mut read = Vec::new();
        for slot in slots.into_iter().filter(|s| !s.raw.trim().is_empty()) {
            let raw = crate::cel_expand::expand_cel(&slot.raw, &defs, None, &mut Vec::new())
                .unwrap_or_else(|_| slot.raw.clone());
            let mut arena = CelArena::default();
            if let Some(ided) =
                lute_cel::parse_slot_marked_refs(&mut arena, &raw).and_then(|h| arena.get(h))
            {
                read_tiers(&ided.expr, &tiers, &mut read);
                if let Some(t) = clock_tier.filter(|_| reads_clock(&ided.expr)) {
                    read.push(t);
                }
                reads_seasons(
                    &ided.expr,
                    &folded.env.rel_vocab.relations,
                    seasons_read.entry(q.id.as_str()).or_default(),
                );
            }
        }
        own.insert(q.id.as_str(), (read, children));
    }
    // Each implicit quest's standing, a join over what it reads — its own
    // conditions and its `quest=` children: nothing (neutral) < only run
    // state (run-looking) < any user or app state. A quest that reads
    // nothing (a `done="true"` sibling, round-4 League R1) takes no side.
    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    enum Standing {
        Neutral,
        Run,
        User,
    }
    let of_reads = |reads: &[ReadTier]| {
        if reads
            .iter()
            .any(|t| matches!(t, ReadTier::User | ReadTier::App))
        {
            Standing::User
        } else if reads.contains(&ReadTier::Run) {
            Standing::Run
        } else {
            Standing::Neutral
        }
    };
    // Monotone: a standing only rises, so the loop settles.
    let mut standing: BTreeMap<&str, Standing> =
        own.keys().map(|id| (*id, Standing::Neutral)).collect();
    loop {
        let mut changed = false;
        for (id, (reads, children)) in &own {
            let from_children = children.iter().map(|c| match quests.get(c) {
                Some(true) => Standing::User,
                Some(false) => Standing::Run,
                None => standing.get(c).copied().unwrap_or(Standing::Neutral),
            });
            // A season read is as transient as a run read.
            let season = if seasons_read.get(id).is_some_and(|s| !s.is_empty()) {
                Standing::Run
            } else {
                Standing::Neutral
            };
            let now = from_children.fold(of_reads(reads).max(season), Ord::max);
            if standing[id] != now {
                standing.insert(id, now);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let mut flagged: BTreeSet<&str> = standing
        .iter()
        .filter(|(_, s)| **s == Standing::Run)
        .map(|(id, _)| *id)
        .collect();
    let parents_of = |id: &str| -> Vec<&str> {
        own.iter()
            .filter(|(_, (_, cs))| cs.contains(&id))
            .map(|(p, _)| *p)
            .chain(
                doc.quests
                    .iter()
                    .filter(|p| p.tier.is_some())
                    .filter(|p| {
                        p.body.iter().any(
                            |n| matches!(n, Node::Objective(o) if o.quest.as_deref() == Some(id)),
                        )
                    })
                    .map(|p| p.id.as_str()),
            )
            .collect()
    };
    // A neutral quest in the tree of a flagged one changes with it: the
    // tree must share one tier.
    loop {
        let joining: Vec<&str> = standing
            .iter()
            .filter(|(id, s)| **s == Standing::Neutral && !flagged.contains(*id))
            .map(|(id, _)| *id)
            .filter(|id| {
                own[id]
                    .1
                    .iter()
                    .chain(&parents_of(id))
                    .any(|n| flagged.contains(n))
            })
            .collect();
        if joining.is_empty() {
            break;
        }
        flagged.extend(joining);
    }
    // A subquest whose parent stays user-tier keeps it: not flagged.
    let stays_user =
        |p: &str| quests.get(p) == Some(&true) || (own.contains_key(p) && !flagged.contains(p));
    let reported: BTreeSet<&str> = flagged
        .iter()
        .copied()
        .filter(|id| !parents_of(id).into_iter().any(stays_user))
        .collect();
    let mut out = Vec::new();
    for q in doc
        .quests
        .iter()
        .filter(|q| reported.contains(q.id.as_str()))
    {
        // The implicit quests of `q`'s tree that must change with it.
        let mut tree: BTreeSet<&str> = BTreeSet::new();
        let mut stack = vec![q.id.as_str()];
        while let Some(id) = stack.pop() {
            let near = own
                .get(id)
                .map(|(_, cs)| cs.clone())
                .unwrap_or_default()
                .into_iter()
                .chain(parents_of(id));
            for n in near {
                if reported.contains(n) && n != q.id && tree.insert(n) {
                    stack.push(n);
                }
            }
        }
        // dsl 0.28.0 §5 (T2-7): a tree reading one season's state resets
        // with that season — suggest its tier, never `run`.
        let seasons: BTreeSet<&str> = std::iter::once(q.id.as_str())
            .chain(tree.iter().copied())
            .filter_map(|id| seasons_read.get(id))
            .flatten()
            .map(String::as_str)
            .collect();
        let (tier, state) = match seasons.iter().next() {
            Some(s) if seasons.len() == 1 => (format!("season:{s}"), format!("season `{s}`")),
            _ => ("run".to_string(), "run".to_string()),
        };
        let fix = if tree.is_empty() {
            format!(
                "write `tier=\"{tier}\"` (or `tier=\"user\"` if it should persist), or set \
                 `defaults.questTier` in lute.project.yaml"
            )
        } else {
            let names = tree
                .iter()
                .map(|t| format!("`{t}`"))
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "its quest tree must share one tier, so write `tier=\"{tier}\"` on it and on \
                 {names} together (or `tier=\"user\"` on all of them if they should persist), \
                 or set `defaults.questTier` in lute.project.yaml"
            )
        };
        let any_run = std::iter::once(q.id.as_str())
            .chain(tree.iter().copied())
            .filter_map(|id| own.get(id))
            .any(|(reads, _)| reads.contains(&ReadTier::Run));
        let state = if any_run && tier != "run" {
            format!("run and {state}")
        } else {
            state
        };
        out.push(Diagnostic {
            code: W_QUEST_TIER_IMPLICIT.to_string(),
            severity: Severity::Warning,
            message: format!(
                "quest `{id}` has no `tier=`, so it is user-tier (it persists across runs), but \
                 {reads} — {fix}",
                id = q.id,
                reads = if standing.get(q.id.as_str()) == Some(&Standing::Neutral) {
                    format!(
                        "it reads no state itself and the rest of its quest tree reads only \
                         {state} state"
                    )
                } else {
                    format!("its conditions read only {state} state")
                }
            ),
            evidence: None,
            span: q.id_span,
            layer: Layer::Logic,
            fixits: Vec::new(),
            provenance: None,
            covered: Vec::new(),
            related: Vec::new(),
        });
    }
    out
}

/// dsl 0.28.0 §5 (T2-7): the seasons whose state `expr` reads —
/// `season.<name>.*` paths and queries of `tier: season:<name>` relations.
fn reads_seasons(
    expr: &cel_parser::ast::Expr,
    relations: &BTreeMap<String, lute_manifest::relations::RelationDecl>,
    out: &mut BTreeSet<String>,
) {
    use cel_parser::ast::Expr;
    match expr {
        Expr::Select(_) | Expr::Ident(_) => {
            if let Some(p) = crate::cel_paths::select_path(expr) {
                if let Some(s) = lute_manifest::season::season_of_path(&p) {
                    out.insert(s.to_string());
                }
            }
        }
        Expr::Call(c) => {
            if matches!(c.func_name.as_str(), "holds" | "count" | "countDistinct") {
                if let Some(query) = crate::fact_env::QueryPattern::from_call(c) {
                    if let Some(s) = relations
                        .get(&query.relation)
                        .and_then(|r| r.tier.as_deref())
                        .and_then(lute_manifest::season::season_ref)
                    {
                        out.insert(s.to_string());
                    }
                }
            }
            for e in c
                .target
                .iter()
                .map(|t| &t.expr)
                .chain(c.args.iter().map(|a| &a.expr))
            {
                reads_seasons(e, relations, out);
            }
        }
        Expr::List(l) => l
            .elements
            .iter()
            .for_each(|e| reads_seasons(&e.expr, relations, out)),
        _ => {}
    }
}

/// Whether `expr` reads a `clock.*` path.
fn reads_clock(expr: &cel_parser::ast::Expr) -> bool {
    use cel_parser::ast::Expr;
    match expr {
        Expr::Select(_) | Expr::Ident(_) => {
            crate::cel_paths::select_path(expr).is_some_and(|p| p.starts_with("clock."))
        }
        Expr::Call(c) => c
            .target
            .iter()
            .map(|t| &t.expr)
            .chain(c.args.iter().map(|a| &a.expr))
            .any(reads_clock),
        Expr::List(l) => l.elements.iter().any(|e| reads_clock(&e.expr)),
        _ => false,
    }
}
