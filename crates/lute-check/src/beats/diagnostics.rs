use super::*;

pub(super) fn tie_warnings(beats: &[Beat<'_>], ties: &[(usize, usize)]) -> Vec<(PathBuf, Diagnostic)> {
    let mut root: Vec<usize> = (0..beats.len()).collect();
    fn find(root: &mut [usize], mut x: usize) -> usize {
        while root[x] != x {
            root[x] = root[root[x]];
            x = root[x];
        }
        x
    }
    for &(a, b) in ties {
        let (ra, rb) = (find(&mut root, a), find(&mut root, b));
        root[ra.max(rb)] = ra.min(rb);
    }
    let mut groups: BTreeMap<usize, (Vec<usize>, Vec<(usize, usize)>)> = BTreeMap::new();
    for &(a, b) in ties {
        let g = groups.entry(find(&mut root, a)).or_default();
        for x in [a, b] {
            if !g.0.contains(&x) {
                g.0.push(x);
            }
        }
        g.1.push((a, b));
    }
    let mut out = Vec::new();
    for (_, (mut members, pairs)) in groups {
        members.sort_unstable();
        let first = &beats[members[0]];
        let names: Vec<&str> = members.iter().map(|&m| beats[m].name.as_str()).collect();
        let target = match first.target {
            Some(t) if members.iter().all(|&m| beats[m].target == Some(t)) => {
                format!(" for `{t}`")
            }
            _ => String::new(),
        };
        // Each distinct reason once; the beats with no condition at all
        // said together.
        let mut reasons: Vec<String> = Vec::new();
        let mut no_when = false;
        for &(a, b) in &pairs {
            match why_not_exclusive(&beats[a], &beats[b]) {
                Why::NoWhen => no_when = true,
                Why::Other(r) => {
                    if !reasons.contains(&r) {
                        reasons.push(r);
                    }
                }
            }
        }
        if no_when {
            let unconstrained: Vec<&Beat<'_>> = members
                .iter()
                .map(|&m| &beats[m])
                .filter(|x| x.stated.is_none())
                .collect();
            // A scene's or bundle beat's `once: run` sets no flag a `when`
            // can read.
            let run = unconstrained
                .iter()
                .any(|x| x.once == BeatOnce::Run && !x.name.starts_with("entry"));
            let reason = match unconstrained.as_slice() {
                [x] => format!(
                    "{} has no `when`{}",
                    x.name,
                    if run {
                        ", and its `once: run` sets no flag a `when` can read"
                    } else {
                        ""
                    }
                ),
                xs => format!(
                    "{}{}",
                    if xs.len() == members.len() {
                        "none of them has a `when`".to_string()
                    } else {
                        let ns: Vec<&str> = xs.iter().map(|x| x.name.as_str()).collect();
                        format!("{} have no `when`", and_list(&ns))
                    },
                    if run {
                        ", and `once: run` sets no flag a `when` can read"
                    } else {
                        ""
                    }
                ),
            };
            reasons.insert(0, reason);
        }
        // dsl 0.28.0 §4 (T3-18): a priority `chapters:` derived is not in
        // the scene's file — say where it comes from, and fix the others.
        let (mut derived, mut written): (Vec<&str>, Vec<&str>) = (Vec::new(), Vec::new());
        for b in members.iter().map(|&m| &beats[m]) {
            if b.priority_derived {
                derived.push(&b.name);
            } else {
                written.push(&b.name);
            }
        }
        let provenance = match derived.as_slice() {
            [] => String::new(),
            [one] => format!(
                " ({one}'s is written by `chapters:` in lute.project.yaml, from its place in its \
                 chain)"
            ),
            many => format!(
                " (the priority of {} is written by `chapters:` in lute.project.yaml, from their \
                 places in their chains)",
                and_list(many)
            ),
        };
        let fix = match (derived.as_slice(), written.as_slice()) {
            ([], _) if members.len() == 2 => "give one a different `priority`".to_string(),
            ([], _) => "give them different priorities".to_string(),
            (_, []) => "write a `priority:` of its own in one of the scenes".to_string(),
            (_, [one]) => format!("give {one} a different `priority`"),
            (_, many) => format!("give {} different priorities", and_list(many)),
        };
        out.push((
            first.path.to_path_buf(),
            beat_diag(
                W_BEAT_PRIORITY_TIE,
                Severity::Warning,
                format!(
                    "{} share priority {}{provenance} on occasion `{}`{target} and can be \
                     eligible at once, so file order picks the winner (today {}, and renaming or \
                     moving a file changes it) — {fix}, or make their `when`s exclusive; they \
                     are not provably exclusive because {} (dsl 0.22.0 §13)",
                    and_list(&names),
                    first.priority,
                    first.on,
                    first.name,
                    reasons.join("; "),
                ),
                first.anchor,
                Layer::Logic,
            ),
        ));
    }
    out
}

/// `a`, `b` and `c`.
pub(super) fn and_list(items: &[&str]) -> String {
    match items {
        [] => String::new(),
        [one] => one.to_string(),
        [init @ .., last] => format!("{} and {last}", init.join(", ")),
    }
}

/// dsl 0.24.0 (T1-8): for an untargeted `b` on an occasion whose target
/// domain is closed (enumerable members) — or (dsl 0.26.0 §5) a kind beat,
/// over its kind's members — the shadowing beat per target: `Some` only when
/// EVERY target has an earlier (in `earlier`, selection order) non-`also`
/// beat on the occasion answering it that is always eligible and never spent.
pub(super) fn shadowed_on_every_target<'b, 'a>(
    b: &Beat<'a>,
    earlier: &'b [Beat<'a>],
) -> Option<Vec<(String, &'b Beat<'a>)>> {
    let targets: Vec<String> = match &b.kind_targets {
        Some(ts) => ts.clone(),
        None => {
            let decl = b.folded.occasions.get(b.on)?;
            let OccasionTarget::Domain { prefix, entity, .. } = &decl.target else {
                return None;
            };
            let kind_members = match &b.folded.env.rel_vocab.kinds.get(entity)?.shape {
                KindShape::Members(ms) => Some(ms.as_slice()),
                KindShape::Open | KindShape::Invalid => None,
            };
            decl.target
                .domain_members(kind_members)?
                .iter()
                .map(|m| format!("{prefix}.{m}"))
                .collect()
        }
    };
    if targets.is_empty() {
        return None;
    }
    targets
        .into_iter()
        .map(|t| {
            let a = earlier.iter().find(|a| {
                !a.also && a.on == b.on && a.cells().answers(&t) && a.always && a.unspent
            })?;
            Some((t, a))
        })
        .collect()
}

/// dsl 0.24.0 (T3-3): the readable flag a beat's own presentation sets and
/// its `once` reads — an entry's `once="user"` `entry.<id>.everRead`,
/// `once="run"` `entry.<id>.read`; a scene's or bundle beat's `once: user`
/// `visited('<id>')` (the save-scoped visited set). A scene's `once: run`
/// (or a clock period) has no readable flag, and a repeatable beat none.
pub(super) fn spend_flag(pb: &ProjectBeat<'_>) -> Option<String> {
    match (pb.kind, &pb.once) {
        (ProjectBeatKind::Entry, BeatOnce::User) => Some(format!("entry.{}.everRead", pb.id)),
        (ProjectBeatKind::Entry, BeatOnce::Run) => Some(format!("entry.{}.read", pb.id)),
        (ProjectBeatKind::Scene, BeatOnce::User)
            if crate::meta::canonical_scene_key(&pb.folded.typed).is_some() =>
        {
            Some(format!("visited('{}')", pb.id))
        }
        (ProjectBeatKind::Bundle, BeatOnce::User) => Some(format!("visited('{}')", pb.id)),
        _ => None,
    }
}

/// The beats whose presentation spends `pb` (dsl 0.25.0 §2): every beat of
/// its `share` key, else `pb` alone.
pub(super) fn spenders<'b, 'a>(
    pb: &'b ProjectBeat<'a>,
    beats: &'b [ProjectBeat<'a>],
    groups: &BTreeMap<String, Vec<usize>>,
) -> Vec<&'b ProjectBeat<'a>> {
    match pb.share.and_then(|(k, _)| groups.get(k)) {
        Some(members) => members.iter().map(|&i| &beats[i]).collect(),
        None => vec![pb],
    }
}

/// dsl 0.24.0 (T3-3): what a beat's `once` adds to its eligibility — no
/// [`spend_flag`] of a beat that spends it is set. dsl 0.25.0 §2: a shared
/// beat is spent by every beat of its key, so each member's flag counts
/// (`!visited('a') && !entry.b.everRead`). `None` when no spender has one.
pub(super) fn once_guard(
    pb: &ProjectBeat<'_>,
    beats: &[ProjectBeat<'_>],
    groups: &BTreeMap<String, Vec<usize>>,
) -> Option<String> {
    let flags: Vec<String> = spenders(pb, beats, groups)
        .into_iter()
        .filter_map(spend_flag)
        .map(|f| format!("!{f}"))
        .collect();
    (!flags.is_empty()).then(|| flags.join(" && "))
}

/// What "`pb` is spent" reads as, for the presence ladder: some beat that
/// spends it was presented — its own [`spend_flag`], or (dsl 0.25.0 §2)
/// the disjunction over its `share` key. `None` unless EVERY spender has a
/// readable flag: one without makes the spend unreadable.
pub(super) fn spent_condition(
    pb: &ProjectBeat<'_>,
    beats: &[ProjectBeat<'_>],
    groups: &BTreeMap<String, Vec<usize>>,
) -> Option<String> {
    let flags: Option<Vec<String>> = spenders(pb, beats, groups)
        .into_iter()
        .map(spend_flag)
        .collect();
    match flags?.as_slice() {
        [] => None,
        [one] => Some(one.clone()),
        many => Some(format!("({})", many.join(" || "))),
    }
}

/// dsl 0.24.0 §4 (`W-CAST-ABSENT`): what each beat may assume about the
/// ladder above it. A `select: first`, non-`also` beat `B` wins only once
/// every beat ordered before it on the same occasion that is a candidate
/// whenever `B` is (untargeted, or `B`'s target), always eligible (no
/// `after:`, `when` absent or always true) and spent by a readable flag
/// ([`spent_condition`]) has been spent: `entry.<id>.everRead`,
/// `entry.<id>.read` or `visited('<id>')` — dsl 0.25.0 §2: for a shared
/// beat, any of its key's flags. Keyed by document, then by the
/// beat's `on` key/attribute offset ([`ProjectBeat::anchor`]).
pub fn presence_ladder(
    docs: &[crate::ProjectDoc<'_>],
    foldeds: &[&FoldedEnv],
) -> BTreeMap<PathBuf, BTreeMap<usize, Vec<String>>> {
    let pbs = in_selection_order(project_beats(docs, foldeds));
    let groups = share_groups(&pbs);
    let spent_conds: Vec<Option<String>> = pbs
        .iter()
        .map(|pb| spent_condition(pb, &pbs, &groups))
        .collect();
    let beats: Vec<(ProjectBeat<'_>, bool, Option<String>)> = pbs
        .into_iter()
        .zip(spent_conds)
        .map(|(pb, spent)| {
            let always = always_eligible(&pb);
            (pb, always, spent)
        })
        .collect();
    let mut out: BTreeMap<PathBuf, BTreeMap<usize, Vec<String>>> = BTreeMap::new();
    for (j, (b, _, _)) in beats.iter().enumerate() {
        let select = b
            .folded
            .occasions
            .get(b.on)
            .map_or(OccasionSelect::First, |o| o.select);
        if select != OccasionSelect::First || b.also {
            continue;
        }
        let spent: Vec<String> = beats[..j]
            .iter()
            .filter(|(a, always, _)| {
                *always && !a.also && a.on == b.on && a.cells().covers(b.cells())
            })
            .filter_map(|(_, _, spent)| spent.clone())
            .collect();
        if !spent.is_empty() {
            out.entry(b.path.to_path_buf())
                .or_default()
                .insert(b.anchor.byte_start, spent);
        }
    }
    out
}

/// A beat that is always eligible: no `after:`, no `spentBy` (dsl 0.27.0 §5:
/// it drops out once its condition holds), and a `when` absent or deciding
/// true without facts.
pub fn always_eligible(pb: &ProjectBeat<'_>) -> bool {
    let params = BTreeMap::new();
    let defs = DefTable {
        bodies: &pb.folded.def_bodies,
        params: &pb.folded.env.def_params,
    };
    let ctx = DecideCtx {
        schema: &pb.folded.env.state,
        dollar: None,
        params: &params,
        facts: None,
    };
    pb.after.is_none()
        && pb.spent_by.is_none()
        && pb
            .when_slot
            .is_none_or(|w| matches!(decide_slot(&w.raw, &defs, &ctx), Some(Decided::Bool(true))))
}

/// dsl 0.27.0 (T3-9, `lute beats`): the verdict of one ladder cell — the
/// earlier beats (indices into `beats`, one root's beats in
/// [`selection_order`]) that win every time `beats[j]` could on the
/// occasion raised for each of `targets` (empty: raised without a target):
/// for every target, the first earlier non-`also` beat answering it that is
/// [`always_eligible`] (`always`, parallel to `beats`) and never spent.
/// `None` unless every target has one, or on a non-`select: first`
/// occasion, or for an `also` beat. Where [`W_BEAT_SHADOWED`] is
/// project-wide (every target), this is per ladder.
pub fn shadowers_at(
    beats: &[ProjectBeat<'_>],
    always: &[bool],
    j: usize,
    targets: &[&str],
) -> Option<Vec<usize>> {
    let b = &beats[j];
    let select = b
        .folded
        .occasions
        .get(b.on)
        .map_or(OccasionSelect::First, |o| o.select);
    if select != OccasionSelect::First || b.also {
        return None;
    }
    let wins = |i: usize, answers: &dyn Fn(BeatCells<'_>) -> bool| {
        let a = &beats[i];
        !a.also && a.on == b.on && always[i] && a.once == BeatOnce::None && answers(a.cells())
    };
    let mut out: Vec<usize> = Vec::new();
    let firsts: Vec<Option<usize>> = if targets.is_empty() {
        vec![(0..j).find(|&i| wins(i, &|c| matches!(c, BeatCells::Any)))]
    } else {
        targets
            .iter()
            .map(|t| (0..j).find(|&i| wins(i, &|c| c.answers(t))))
            .collect()
    };
    for i in firsts {
        let i = i?;
        if !out.contains(&i) {
            out.push(i);
        }
    }
    Some(out)
}

/// dsl 0.27.0 (T3-11): what an `after:` requires, as a condition the tie
/// check conjoins — each `visited('<id>')` it needs (a `once: user` beat's
/// own guard is `!visited('<id>')`), and, since a beat is visited only once
/// its own `after:` held, what that beat's `after:` required in turn (G-1:
/// `r3` after `visited(r2)`, `r2` after `visited(r1)` ⇒ `visited(r1)`),
/// through `afters` (visited key → `after:` text). `completed` / `active`
/// read no flag the eligibility has, so they drop out (weakening, never
/// strengthening: an `||` with a dropped side drops whole). `None` when
/// nothing remains or the value is out of profile.
pub(super) fn after_premise(raw: &str, afters: &BTreeMap<String, &str>) -> Option<String> {
    use lute_manifest::semantics::prereq::PrereqFormula as F;
    fn cel(f: &F, afters: &BTreeMap<String, &str>, seen: &mut Vec<String>) -> Option<String> {
        match f {
            F::Visited(k) => {
                let own = format!("visited('{k}')");
                if seen.contains(k) {
                    return Some(own);
                }
                seen.push(k.clone());
                let before = afters
                    .get(k.as_str())
                    .and_then(|raw| crate::prereq::parse_prereq(raw, bare_span(0, 0)).0)
                    .and_then(|f| cel(&f, afters, seen));
                seen.pop();
                Some(match before {
                    Some(b) => format!("{own} && {b}"),
                    None => own,
                })
            }
            F::Completed(_) | F::Active(_) => None,
            F::And(a, b) => match (cel(a, afters, seen), cel(b, afters, seen)) {
                (Some(a), Some(b)) => Some(format!("{a} && {b}")),
                (a, b) => a.or(b),
            },
            F::Or(a, b) => Some(format!(
                "(({}) || ({}))",
                cel(a, afters, seen)?,
                cel(b, afters, seen)?
            )),
        }
    }
    if raw.trim().is_empty() {
        return None;
    }
    let span = bare_span(0, 0);
    cel(
        &crate::prereq::parse_prereq(raw, span).0?,
        afters,
        &mut Vec::new(),
    )
}

/// `a` and `b` can never be eligible together: two of the alternatives of
/// their eligibilities (`when`, `once` and the facts only their own
/// presentation asserts — [`Beat::eligible`]), negations normalized (dsl
/// 0.26.0 §8), pin one path to disjoint values in every pairing; or the
/// decider folds their conjunction to `false` — in either beat's document,
/// under `env`'s must set at that beat's `when` slot when `check-project`
/// has one (both are judged in the same state, so a fact guaranteed at
/// either slot holds). An unconstrained eligibility never excludes
/// anything.
pub(super) fn provably_exclusive(a: &Beat<'_>, b: &Beat<'_>, env: Option<&FactEnv>) -> bool {
    let (Some(wa), Some(wb)) = (&a.eligible, &b.eligible) else {
        return false;
    };
    if crate::reachability::provably_exclusive(&a.dnf, &b.dnf) {
        return true;
    }
    let params = BTreeMap::new();
    let both = format!("({wa}) && ({wb})");
    [b, a].into_iter().any(|x| {
        let defs = DefTable {
            bodies: &x.folded.def_bodies,
            params: &x.folded.env.def_params,
        };
        let ctx = DecideCtx {
            schema: &x.folded.env.state,
            dollar: None,
            params: &params,
            facts: env.zip(x.when_span).map(|(env, span)| FactScope {
                env,
                vocab: &x.folded.env.rel_vocab,
                path: x.path,
                span,
                wip: false,
            }),
        };
        matches!(decide_slot(&both, &defs, &ctx), Some(Decided::Bool(false)))
    })
}

/// Why two beats are not [`provably_exclusive`] ([`why_not_exclusive`]).
pub(super) enum Why {
    /// One of the two constrains nothing (its eligibility is unconditional).
    NoWhen,
    /// Any other reason, as a clause.
    Other(String),
}

/// dsl 0.26.0 §8 (T3-2): why `a` and `b` are not [`provably_exclusive`]:
/// a beat that constrains nothing, a flag that outlives a `once: run`
/// spend, a fact one asserts that may hold before it plays, or the paths
/// the two alternatives that overlap constrain.
pub(super) fn why_not_exclusive(a: &Beat<'_>, b: &Beat<'_>) -> Why {
    let Some((da, db)) = crate::reachability::non_exclusive_witness(a.stated_dnf(), b.stated_dnf())
    else {
        return Why::Other("their conjunction does not decide `false`".to_string());
    };
    for (x, y, dy) in [(a, b, db), (b, a, da)] {
        if x.once == BeatOnce::Run && dy.requires_true(&x.ever_flag) {
            return Why::Other(format!(
                "`{}` persists across runs; `once: run` does not, so in a later run both are \
                 eligible",
                x.ever_flag
            ));
        }
        if let Some((fact, _, why)) = x
            .persists
            .iter()
            .find(|(_, pattern, _)| dy.requires_true(pattern))
        {
            return Why::Other(format!(
                "{} reads `{fact}`, which {} asserts, but {why}",
                y.name, x.name
            ));
        }
    }
    if a.stated.is_none() || b.stated.is_none() {
        return Why::NoWhen;
    }
    let list = |d: &crate::reachability::Disjunct| {
        let paths: std::collections::BTreeSet<&str> = d.paths().collect();
        paths
            .into_iter()
            .map(|p| format!("`{p}`"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let shared: std::collections::BTreeSet<&str> =
        da.paths().filter(|p| db.paths().any(|q| q == *p)).collect();
    if !shared.is_empty() {
        let shared: Vec<String> = shared.into_iter().map(|p| format!("`{p}`")).collect();
        return Why::Other(format!(
            "both allow a common value of {}",
            shared.join(", ")
        ));
    }
    Why::Other(match (list(da), list(db)) {
        (pa, pb) if pa.is_empty() && pb.is_empty() => {
            "neither `when` compares a path the checker can read".to_string()
        }
        (pa, pb) if pa.is_empty() => format!(
            "{}'s condition compares no path the checker can read; {} reads {pb}",
            a.name, b.name
        ),
        (pa, pb) if pb.is_empty() => format!(
            "{}'s condition compares no path the checker can read; {} reads {pa}",
            b.name, a.name
        ),
        (pa, pb) => format!(
            "{} reads {pa} and {} reads {pb}: no path both constrain",
            a.name, b.name
        ),
    })
}

/// What [`read_tier`] needs to know about tiers: the relations the reading
/// document sees, and the quests whose tier is known (`true` = user, `false`
/// = run; an absent quest's tier is unknown).
pub(crate) struct UserTier<'a> {
    pub(crate) relations: &'a BTreeMap<String, lute_manifest::relations::RelationDecl>,
    pub(crate) quests: &'a BTreeMap<&'a str, bool>,
}

/// The lifetime tier of one state read (dsl 0.23.0 §6, dsl 0.24.0 T3-4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReadTier {
    /// Reset by every new run: `run.*`, a `tier="run"` quest's
    /// `quest.<id>.*`, a query of a relation without `tier:` or `tier: run`.
    Run,
    /// Kept across runs: `user.*`, `entry.<id>.everRead`, a user-tier
    /// quest's `quest.<id>.*`, a query of a `tier: user` relation.
    User,
    /// Kept across users: `app.*`, a query of a `tier: app` relation.
    App,
    /// Anything else — `scene.*`, `prev.run.*` (run history, replaced every
    /// run: dsl 0.23.1), a quest of unknown tier, a local variable.
    Other,
}

/// The tier `expr` reads when it is a read leaf — a state path, or a
/// `holds` / `count` / `countDistinct` query — else `None`.
pub(crate) fn read_tier(expr: &cel_parser::ast::Expr, tiers: &UserTier<'_>) -> Option<ReadTier> {
    use cel_parser::ast::Expr;
    match expr {
        e if matches!(e, Expr::Ident(_) | Expr::Select(_))
            || crate::cel_paths::is_path_index(e) =>
        {
            let Some(p) = crate::cel_paths::select_path(expr) else {
                return Some(ReadTier::Other);
            };
            Some(
                if p.starts_with("user.") || crate::cel_paths::is_entry_ever_read(&p) {
                    ReadTier::User
                } else if p.starts_with("app.") {
                    ReadTier::App
                } else if p.starts_with("run.") {
                    ReadTier::Run
                } else {
                    match p
                        .strip_prefix("quest.")
                        .and_then(|rest| rest.split('.').next())
                        .and_then(|id| tiers.quests.get(id))
                    {
                        Some(true) => ReadTier::User,
                        Some(false) => ReadTier::Run,
                        None => ReadTier::Other,
                    }
                },
            )
        }
        Expr::Call(c) if matches!(c.func_name.as_str(), "holds" | "count" | "countDistinct") => {
            let relation = crate::fact_env::QueryPattern::from_call(c)
                .map(|q| q.relation);
            Some(match relation
                .as_deref()
                .and_then(|name| tiers.relations.get(name))
                .map(|r| r.tier.as_deref())
            {
                Some(None | Some("run")) => ReadTier::Run,
                Some(Some("user")) => ReadTier::User,
                Some(Some("app")) => ReadTier::App,
                _ => ReadTier::Other,
            })
        }
        _ => None,
    }
}

/// Every read tier `expr` touches, in walk order ([`read_tier`] at each
/// leaf, recursing through every other sub-expression).
pub(crate) fn read_tiers(
    expr: &cel_parser::ast::Expr,
    tiers: &UserTier<'_>,
    out: &mut Vec<ReadTier>,
) {
    use cel_parser::ast::{EntryExpr, Expr};
    if let Some(t) = read_tier(expr, tiers) {
        out.push(t);
        return;
    }
    match expr {
        Expr::Call(c) => {
            for e in c
                .target
                .iter()
                .map(|t| &t.expr)
                .chain(c.args.iter().map(|a| &a.expr))
            {
                read_tiers(e, tiers, out);
            }
        }
        Expr::List(l) => l
            .elements
            .iter()
            .for_each(|e| read_tiers(&e.expr, tiers, out)),
        Expr::Map(m) => m.entries.iter().for_each(|e| match &e.expr {
            EntryExpr::MapEntry(m) => {
                read_tiers(&m.key.expr, tiers, out);
                read_tiers(&m.value.expr, tiers, out);
            }
            EntryExpr::StructField(f) => read_tiers(&f.value.expr, tiers, out),
        }),
        Expr::Struct(s) => s.entries.iter().for_each(|e| match &e.expr {
            EntryExpr::MapEntry(m) => {
                read_tiers(&m.key.expr, tiers, out);
                read_tiers(&m.value.expr, tiers, out);
            }
            EntryExpr::StructField(f) => read_tiers(&f.value.expr, tiers, out),
        }),
        Expr::Comprehension(c) => {
            for e in [
                &c.iter_range,
                &c.accu_init,
                &c.loop_cond,
                &c.loop_step,
                &c.result,
            ] {
                read_tiers(&e.expr, tiers, out);
            }
        }
        _ => {}
    }
}

/// `when` (already `@def`-expanded) reads at least one piece of state, every
/// one of them user-tier ([`ReadTier::User`]), and calls no function but the
/// CEL operators, `has`, and a query of a user-tier relation. A run-tier
/// relation or quest, `visited()`, or `now()` may change within a run.
/// `prev.run.*` is the previous run's snapshot, which every run replaces:
/// run history, not user-tier (dsl 0.23.1).
pub(super) fn reads_only_user(when: &str, tiers: &UserTier<'_>) -> bool {
    use cel_parser::ast::Expr;
    fn walk(expr: &Expr, tiers: &UserTier<'_>, reads: &mut usize) -> bool {
        if let Some(tier) = read_tier(expr, tiers) {
            let user = tier == ReadTier::User;
            *reads += usize::from(user);
            return user;
        }
        match expr {
            Expr::Call(c) => {
                let operator = !c.func_name.starts_with(|ch: char| ch.is_ascii_alphabetic())
                    || matches!(c.func_name.as_str(), "isSet" | "has");
                c.target.is_none() && operator && c.args.iter().all(|a| walk(&a.expr, tiers, reads))
            }
            Expr::List(l) => l.elements.iter().all(|e| walk(&e.expr, tiers, reads)),
            Expr::Literal(_) => true,
            _ => false,
        }
    }
    let mut arena = lute_cel::CelArena::default();
    let Some(ided) = lute_cel::parse_slot_marked_refs(&mut arena, when).and_then(|h| arena.get(h))
    else {
        return false;
    };
    let mut reads = 0;
    walk(&ided.expr, tiers, &mut reads) && reads > 0
}

/// The YAML value an author wrote, for the "got …" half of a message. A
/// quoted `"false"` / `"10"` is a string where a bool or number is meant:
/// the message says so instead of printing `false` against `false`.
pub(super) fn describe(v: &serde_yaml::Value) -> String {
    match v {
        serde_yaml::Value::String(s)
            if s == "true" || s == "false" || s.trim().parse::<f64>().is_ok() =>
        {
            format!("the quoted string `\"{s}\"` — write it unquoted, `{s}`")
        }
        serde_yaml::Value::String(s) => format!("`{s}`"),
        serde_yaml::Value::Bool(b) => format!("`{b}`"),
        serde_yaml::Value::Number(n) => format!("`{n}`"),
        serde_yaml::Value::Null => "an empty value".to_string(),
        serde_yaml::Value::Sequence(_) => "a list".to_string(),
        serde_yaml::Value::Mapping(_) => "a mapping".to_string(),
        serde_yaml::Value::Tagged(_) => "a tagged value".to_string(),
    }
}

/// Where the frontmatter interior starts in the document (see
/// [`crate::meta::meta_key_span`] for the envelope rule).
pub(super) fn interior_base(meta: &Meta) -> usize {
    const OPENER_LEN: usize = 4; // "---\n"
    let enveloped = meta.span.byte_end.saturating_sub(meta.span.byte_start) != meta.raw_yaml.len();
    meta.span.byte_start + if enveloped { OPENER_LEN } else { 0 }
}

pub(super) fn bare_span(byte_start: usize, byte_end: usize) -> Span {
    Span {
        byte_start,
        byte_end,
        line: 0,
        column: 0,
        utf16_range: (0, 0),
    }
}

/// The TOP-LEVEL (unindented) `key:` line of the AUTHORED frontmatter:
/// `(line start offset in raw_yaml, the text after the colon)`. A nested key
/// of the same name (`extra: { on: … }`) never matches, and neither does a
/// key a `chapters:` chain derived — it has no text in the file (T3-18).
pub(super) fn top_key_line<'m>(meta: &'m Meta, key: &str) -> Option<(usize, &'m str)> {
    let mut line_start = 0usize;
    for line in crate::chapters::authored_yaml(&meta.raw_yaml).split_inclusive('\n') {
        if let Some(rest) = line.strip_prefix(key) {
            if let Some(after) = rest.trim_start_matches([' ', '\t']).strip_prefix(':') {
                return Some((line_start, after));
            }
        }
        line_start += line.len();
    }
    None
}

/// The span of a top-level frontmatter `key` (the key text itself), falling
/// back to [`crate::meta::meta_key_span`]. Line/column stay zeroed — the
/// diagnostic normalizers recompute them from bytes.
pub(crate) fn top_key_span(meta: &Meta, key: &str) -> Span {
    match top_key_line(meta, key) {
        Some((at, _)) => {
            let start = interior_base(meta) + at;
            bare_span(start, start + key.len())
        }
        None => crate::meta::meta_key_span(meta, key),
    }
}

