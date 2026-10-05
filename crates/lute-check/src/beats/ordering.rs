use super::*;

pub(super) fn well_formed_share(share: Option<&(String, Span)>) -> Option<(&str, Span)> {
    share
        .filter(|(k, _)| is_name(k))
        .map(|(k, s)| (k.as_str(), *s))
}

/// dsl 0.25.0 §2: what each beat's shared spend holds — `share key → the
/// beats of the key`, as indices into `beats`, in project order.
pub(super) fn share_groups(beats: &[ProjectBeat<'_>]) -> BTreeMap<String, Vec<usize>> {
    let mut groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (i, b) in beats.iter().enumerate() {
        if let Some((key, _)) = b.share {
            groups.entry(key.to_string()).or_default().push(i);
        }
    }
    groups
}

/// dsl 0.25.0 §2 ([`E_BEAT_ATTR`], `check-project`): every beat of one
/// `share` key declares the same `once` — the first beat of the key, in
/// project order, sets it; each beat that differs is reported at its
/// `share`.
pub(super) fn check_share_once(beats: &[ProjectBeat<'_>]) -> Vec<(PathBuf, Diagnostic)> {
    let mut out = Vec::new();
    for (key, members) in share_groups(beats) {
        let first = &beats[members[0]];
        for &i in &members[1..] {
            let b = &beats[i];
            if b.once == first.once {
                continue;
            }
            out.push((
                b.path.to_path_buf(),
                beat_diag(
                    E_BEAT_ATTR,
                    Severity::Error,
                    format!(
                        "{} shares `{key}` with {}, but declares `once: {}` where {} declares \
                         `once: {}`; the beats of one `share` key are spent together for one \
                         period, so every one of them declares the same `once` (dsl 0.25.0 §2)",
                        b.name(),
                        first.name(),
                        b.once.as_str(),
                        first.name(),
                        first.once.as_str(),
                    ),
                    b.share.map_or(b.anchor, |(_, s)| s),
                    Layer::Logic,
                ),
            ));
        }
    }
    out
}

/// One beat of the project, in selection-tiebreak order, with what the
/// selection passes judge about it.
pub(super) struct Beat<'a> {
    pub(super) path: &'a Path,
    /// `scene `k`` / `entry `id`` — the beat as messages name it.
    pub(super) name: String,
    pub(super) on: &'a str,
    pub(super) target: Option<&'a str>,
    /// dsl 0.26.0 §5: [`ProjectBeat::kind_targets`].
    pub(super) kind_targets: Option<Vec<String>>,
    pub(super) priority: i64,
    /// [`ProjectBeat::priority_derived`].
    pub(super) priority_derived: bool,
    pub(super) once: BeatOnce,
    /// dsl 0.23.0 §3: presented beside the winner, never instead of it.
    pub(super) also: bool,
    /// Always eligible: no `after:`, and a `when` absent or deciding true.
    pub(super) always: bool,
    /// Never spent: a scene's `once: false`, or an entry without `once`.
    pub(super) unspent: bool,
    /// The `when` after `@def` expansion in its own document (`None` when
    /// absent) — what messages quote.
    pub(super) when: Option<String>,
    /// The `when` slot's span — where the fact envelope's must set is read.
    pub(super) when_span: Option<Span>,
    /// dsl 0.24.0 (T3-3): the eligibility the tie check conjoins across
    /// documents — the expanded `when` AND what the beat's `once` requires
    /// ([`once_guard`]) AND (dsl 0.26.0 §8) `!holds(F)` for every fact `F`
    /// only its own unplayed presentation can assert
    /// ([`crate::cast::unit_facts`]); `None` when none constrains.
    pub(super) eligible: Option<String>,
    /// [`Self::eligible`] in disjunctive normal form, typed in its own
    /// document (a pure-schedule `holds(A)` contributing its rules' `cel()`
    /// guards).
    pub(super) dnf: crate::reachability::Dnf,
    /// The eligibility the author wrote — `when`, `once`, `spentBy` and
    /// `after:`, without the facts the beat's own presentation asserts — and
    /// its DNF when that differs from [`Self::dnf`]: what a tie message
    /// explains, since only these are the author's to change.
    pub(super) stated: Option<String>,
    pub(super) stated_dnf: Option<crate::reachability::Dnf>,
    /// Facts the beat asserts that may hold before it plays: diagnostic
    /// ground fact, canonical query pattern pseudo-path, and persistence reason.
    pub(super) persists: Vec<(String, String, String)>,
    /// The flag that stays set across runs once the beat played:
    /// `visited('<id>')` for a scene or bundle beat, `entry.<id>.everRead`
    /// for an entry.
    pub(super) ever_flag: String,
    /// A defaulted `once: run` (a scene's or bundle beat's; never an
    /// entry's, whose `once` is always written) with a `when` that reads
    /// only user-tier state (dsl 0.23.1).
    pub(super) run_once_user_when: bool,
    /// Where a warning anchors: the `on` key / attribute.
    pub(super) anchor: Span,
    /// dsl 0.27.0 §4: provably never presented — its `when` alone, or under
    /// its occasion's gate and `!terminal`, is false (a finite clock's range
    /// included). Such a beat ties with nothing.
    pub(super) never: bool,
    pub(super) folded: &'a FoldedEnv,
}

impl Beat<'_> {
    pub(super) fn cells(&self) -> BeatCells<'_> {
        BeatCells::of(self.target, self.kind_targets.as_deref())
    }

    /// [`Beat::stated`] in DNF.
    pub(super) fn stated_dnf(&self) -> &crate::reachability::Dnf {
        self.stated_dnf.as_ref().unwrap_or(&self.dnf)
    }
}

/// `W-BEAT-PRIORITY-TIE` (dsl 0.22.0 §13): two beats on one `select: first`
/// occasion whose selection falls to file order.
pub const W_BEAT_PRIORITY_TIE: &str = "W-BEAT-PRIORITY-TIE";
/// `W-BEAT-ONCE-RUN-USER` (dsl 0.22.0 §13 advisory, dsl 0.23.1): a beat
/// spent once per run by DEFAULT whose `when` reads only user-tier state, so
/// it replays every run. An authored `once: run` silences it.
pub const W_BEAT_ONCE_RUN_USER: &str = "W-BEAT-ONCE-RUN-USER";

/// The error-severity diagnostics the per-file `check()` reported, as byte
/// ranges per document — what [`check_project_beats`] leaves out.
pub type ReportedErrors = BTreeMap<PathBuf, Vec<std::ops::Range<usize>>>;

/// Whether an error sits inside `pb`'s declaration ([`ProjectBeat::extent`])
/// or its document's frontmatter.
pub(super) fn reported_in(
    pb: &ProjectBeat<'_>,
    docs: &[crate::ProjectDoc<'_>],
    errors: &ReportedErrors,
) -> bool {
    let Some(spans) = errors.get(pb.path) else {
        return false;
    };
    let meta = docs
        .iter()
        .find(|item| item.path == pb.path)
        .map(|item| item.doc.meta.span.byte_start..item.doc.meta.span.byte_end);
    spans.iter().any(|s| {
        pb.extent.contains(&s.start) || meta.as_ref().is_some_and(|m| m.contains(&s.start))
    })
}

/// The project beat passes over one resolved project root (dsl 0.21.0 §4,
/// §5; dsl 0.22.0 §13). `docs` and `foldeds` are parallel and in
/// `check-project` order, which is the `ProjectIndex.beats` tiebreak order
/// (documents in order, declaration order within). Beats are in
/// [`selection_order`]: priority descending, member > sub-kind > kind, then
/// that order.
///
/// - [`W_BEAT_SHADOWED`]: a beat `B` on a `select: first` occasion (an
///   undeclared occasion counts as `first`) is shadowed by the first earlier
///   beat `A` on the same occasion whose target is absent or equal to `B`'s —
///   so `A` is a candidate whenever `B` is — that is always eligible (no
///   `after:`, `when` absent or deciding true without facts) and never spent
///   (`once: false`, or an entry without `once`). `A` then wins every time
///   `B` could. Conservative: an undecided `when` never shadows. dsl 0.24.0
///   (T1-8): an untargeted `B` on an occasion whose target domain is closed
///   is also shadowed when, for EVERY `<prefix>.<member>` of the domain, some
///   earlier such `A` targets that member (or none) — it can win no ladder.
/// - [`W_BEAT_PRIORITY_TIE`]: an unshadowed `B` with EQUAL priority to
///   earlier beats on the same `select: first` occasion that can be
///   candidates at once (either target absent, or equal) and whose
///   eligibilities are not provably exclusive — neither the decider folds
///   their conjunction to `false` nor two of their conjuncts pin one path to
///   disjoint values. dsl 0.24.0 (T3-3): the eligibility is the `when` AND
///   what the `once` requires ([`once_guard`]) AND (dsl 0.27.0, T3-11) the
///   `visited(…)` its `after:` requires ([`after_premise`]), and a
///   `holds(A)` conjunct of a pure-schedule derived atom contributes its
///   rules' `cel()` guards. File order then picks the winner. dsl 0.27.0
///   (T3-11): one warning per group of beats tying one another
///   ([`tie_warnings`]), naming each and each distinct reason once.
/// - [`W_BEAT_ONCE_RUN_USER`]: a beat whose `once: run` is DEFAULTED (not
///   written — dsl 0.23.1) and whose `when` reads state, all of it user-tier
///   ([`reads_only_user`]: `user.*`, `entry.<id>.everRead`, a user-tier
///   quest's `quest.<id>.*`, a `tier: user` relation's `holds`/`count`;
///   `prev.run.*` is run history, not user-tier): once true it stays true
///   across runs, so the beat plays again at the start of every run.
///
/// A beat with an error inside its declaration or its document's
/// frontmatter (`errors`, what the per-file `check()` reported) is left out
/// of every pass above: that error is the one report about it, and a
/// ranking of a beat the checker rejected would be judged on text the author
/// is about to change.
pub fn check_project_beats(
    docs: &[crate::ProjectDoc<'_>],
    foldeds: &[&FoldedEnv],
    producers: &crate::cast::FactProducers,
    env: Option<&FactEnv>,
    errors: &ReportedErrors,
) -> Vec<(PathBuf, Diagnostic)> {
    let params = BTreeMap::new();
    // dsl 0.23.0 §6: a quest's tier (`tier="run"`, else user) — project-wide,
    // since a beat may read a quest declared anywhere.
    let quest_tiers: BTreeMap<&str, bool> = docs
        .iter()
        .flat_map(|item| &item.doc.quests)
        .filter(|q| !q.id.is_empty())
        .map(|q| {
            (
                q.id.as_str(),
                // dsl 0.27.0 §5: a `season:<name>` quest resets like a run one.
                !q.tier
                    .as_ref()
                    .is_some_and(|(t, _)| t == "run" || t.starts_with("season:")),
            )
        })
        .collect();
    let mut pbs = project_beats(docs, foldeds);
    let share_diags = check_share_once(&pbs);
    pbs.retain(|pb| !reported_in(pb, docs, errors));
    let groups = share_groups(&pbs);
    let guards: Vec<Option<String>> = pbs.iter().map(|pb| once_guard(pb, &pbs, &groups)).collect();
    // Visited key → `after:` text, for the tie check's after-closure (G-1);
    // a key two beats share is ambiguous and left out.
    let mut afters: BTreeMap<String, Option<&str>> = BTreeMap::new();
    for pb in pbs.iter().filter(|pb| pb.kind != ProjectBeatKind::Entry) {
        afters
            .entry(pb.id.clone())
            .and_modify(|a| *a = None)
            .or_insert(pb.after);
    }
    let afters: BTreeMap<String, &str> = afters
        .into_iter()
        .filter_map(|(k, a)| Some((k, a?)))
        .collect();
    let order = selection_order(
        &pbs.iter()
            .map(|b| (b.on, b.priority, b.kind_targets.as_deref()))
            .collect::<Vec<_>>(),
    );
    let beats: Vec<Beat<'_>> = pbs
        .into_iter()
        .zip(guards)
        .map(|(pb, guard)| {
            let folded = pb.folded;
            let defs = DefTable {
                bodies: &folded.def_bodies,
                params: &folded.env.def_params,
            };
            let ctx = DecideCtx {
                schema: &folded.env.state,
                dollar: None,
                params: &params,
                facts: None,
            };
            let holds = pb.when_slot.is_none_or(|w| {
                matches!(decide_slot(&w.raw, &defs, &ctx), Some(Decided::Bool(true)))
            });
            // dsl 0.26.0 §8: a fact only this beat's unplayed presentation
            // asserts does not hold while it is eligible.
            let mut absent = Vec::new();
            let mut persists = Vec::new();
            for f in crate::cast::unit_facts(
                producers,
                &folded.env.rel_vocab,
                pb.path,
                pb.unit,
                // A `spentBy` beat is not spent by being presented.
                &if pb.spent_by.is_some() {
                    BeatOnce::None
                } else {
                    pb.once.clone()
                },
            ) {
                match f.persists {
                    None => absent.push(format!("!{}", f.query)),
                    Some(why) => persists.push((f.key, f.pattern, why)),
                }
            }
            // dsl 0.27.0 (T3-11): what the beat's `after:` requires — a
            // `once: user` beat `X` and one waiting on `visited('X')` are
            // never eligible together.
            let after = pb.after.and_then(|a| after_premise(a, &afters));
            let stated = pb
                .when
                .as_deref()
                .map(|w| format!("({w})"))
                .into_iter()
                .chain(guard)
                // dsl 0.27.0 §5: eligible only while `spentBy` does not hold.
                .chain(pb.spent_by.as_deref().map(|s| format!("!({s})")))
                .chain(after)
                .reduce(|acc, c| format!("{acc} && {c}"));
            let eligible = stated
                .iter()
                .cloned()
                .chain(absent)
                .reduce(|acc, c| format!("{acc} && {c}"));
            let user_tier = UserTier {
                relations: &folded.env.rel_vocab.relations,
                quests: &quest_tiers,
            };
            let never = crate::gates::beat_never_eligible(
                folded,
                pb.on,
                pb.target,
                pb.when_slot.map(|w| w.raw.as_str()),
                |c| {
                    let span = pb.when_slot.map_or(pb.anchor, |w| w.span);
                    crate::gates::provably_false(
                        c,
                        folded,
                        env.map(|e| (e, pb.path, span)),
                    )
                },
            );
            let dnf_of = |e: Option<&str>| {
                e.map_or_else(Default::default, |e| {
                    crate::reachability::when_dnf(
                        e,
                        &defs,
                        &folded.env.state,
                        Some(&folded.env.rel_vocab),
                    )
                })
            };
            let dnf = dnf_of(eligible.as_deref());
            Beat {
                path: pb.path,
                name: pb.name(),
                on: pb.on,
                target: pb.target,
                kind_targets: pb.kind_targets,
                priority: pb.priority,
                priority_derived: pb.priority_derived,
                once: pb.once.clone(),
                also: pb.also,
                always: pb.after.is_none() && holds && pb.spent_by.is_none(),
                unspent: pb.once == BeatOnce::None && pb.spent_by.is_none(),
                run_once_user_when: pb.once == BeatOnce::Run
                    && !pb.once_authored
                    && pb
                        .when
                        .as_deref()
                        .is_some_and(|w| reads_only_user(w, &user_tier)),
                stated_dnf: (stated != eligible).then(|| dnf_of(stated.as_deref())),
                dnf,
                stated,
                eligible,
                persists,
                ever_flag: match pb.kind {
                    ProjectBeatKind::Entry => format!("entry.{}.everRead", pb.id),
                    ProjectBeatKind::Scene | ProjectBeatKind::Bundle => {
                        format!("visited('{}')", pb.id)
                    }
                },
                when_span: pb.when_slot.map(|w| w.span),
                when: pb.when,
                anchor: pb.anchor,
                folded,
                never,
            }
        })
        .collect();
    // dsl 0.26.0 §5, dsl 0.27.0 (T3-10): member > sub-kind > kind at equal
    // priority, then the tiebreak order.
    let beats = reorder(beats, &order);

    let mut out = share_diags;
    for b in beats.iter().filter(|b| b.run_once_user_when) {
        out.push((
            b.path.to_path_buf(),
            beat_diag(
                W_BEAT_ONCE_RUN_USER,
                Severity::Warning,
                format!(
                    "{} defaults to `once: run` and its `when` `{}` reads only user-tier \
                     state; it may replay on later runs. Write `once: run` to make the \
                     policy explicit, use `once: user` for a beat heard once ever, or \
                     gate it on run-tier state (dsl 0.22.0 §13)",
                    b.name,
                    b.when.as_deref().unwrap_or_default().trim()
                ),
                b.anchor,
                Layer::Logic,
            ),
        ));
    }
    let mut ties: Vec<(usize, usize)> = Vec::new();
    for (j, b) in beats.iter().enumerate() {
        let select = b
            .folded
            .occasions
            .get(b.on)
            .map_or(OccasionSelect::First, |o| o.select);
        // dsl 0.23.0 §3: an `also` beat never competes for the win — it is
        // presented beside the winner — so it is neither shadowed nor
        // shadows, and its order among other `also` beats is no tie.
        if select != OccasionSelect::First || b.also {
            continue;
        }
        let target = b.target.map_or_else(String::new, |t| format!(" for `{t}`"));
        if let Some(a) = beats[..j].iter().find(|a| {
            !a.also && a.on == b.on && a.cells().covers(b.cells()) && a.always && a.unspent
        }) {
            let spent = if a.name.starts_with("entry") {
                "an entry without `once`"
            } else {
                "`once: false`"
            };
            out.push((
                b.path.to_path_buf(),
                beat_diag(
                    W_BEAT_SHADOWED,
                    Severity::Warning,
                    format!(
                        "{} can never win occasion `{}`{target}: {} (priority {}) is ordered \
                         before it, is always eligible (no `after:`, and its `when` is absent or \
                         always true), and is never spent ({spent}), so it wins every time \
                         (dsl 0.21.0 §5)",
                        b.name, b.on, a.name, a.priority
                    ),
                    b.anchor,
                    Layer::Logic,
                ),
            ));
            continue;
        }
        // dsl 0.24.0 (T1-8): an untargeted `B` on an occasion with a closed
        // target domain is a candidate at every member — and shadowed when,
        // at EVERY member, an earlier always-eligible never-spent beat for
        // that member wins (the per-target ladders of `lute beats`). dsl
        // 0.26.0 §5: so is a kind beat at every member of its kind.
        if b.target.is_none() || b.cells().is_kind() {
            if let Some(shadowers) = shadowed_on_every_target(b, &beats[..j]) {
                let listed: Vec<String> = shadowers
                    .iter()
                    .map(|(t, a)| format!("`{t}`: {}", a.name))
                    .collect();
                out.push((
                    b.path.to_path_buf(),
                    beat_diag(
                        W_BEAT_SHADOWED,
                        Severity::Warning,
                        format!(
                            "{} can never win occasion `{}`: it answers every {}, but on \
                             every {} an earlier beat for that \
                             target is always eligible (no `after:`, and its `when` is absent \
                             or always true) and never spent, so it wins every time — {} \
                             (dsl 0.21.0 §5)",
                            b.name,
                            b.on,
                            b.target.filter(|_| b.cells().is_kind()).map_or_else(
                                || "target".to_string(),
                                |t| format!("member of `{t}`")
                            ),
                            if b.cells().is_kind() {
                                "one of them"
                            } else {
                                "target of the occasion's domain"
                            },
                            listed.join(", ")
                        ),
                        b.anchor,
                        Layer::Logic,
                    ),
                ));
                continue;
            }
        }
        for (i, a) in beats[..j].iter().enumerate() {
            if !a.also
                && a.on == b.on
                && a.priority == b.priority
                && a.cells().meets(b.cells())
                // dsl 0.26.0 §5: at equal priority the beat naming the
                // member (or none) outranks a kind beat, and (dsl 0.27.0,
                // T3-10) a sub-kind beat its parent's — no file order.
                && a.cells().is_kind() == b.cells().is_kind()
                && !a.cells().nested(b.cells())
                // dsl 0.27.0 §4: a beat that is never presented ties with
                // nothing (its unreachable verdict says why).
                && !a.never
                && !b.never
                && !provably_exclusive(a, b, env)
            {
                ties.push((i, j));
            }
        }
    }
    out.extend(tie_warnings(&beats, &ties));
    out
}

