use super::*;

/// What declares a [`ProjectBeat`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectBeatKind {
    /// A scene's frontmatter beat (dsl 0.21.0 §3.1).
    Scene,
    /// A lore `<entry on=…>` (dsl 0.21.0 §3.2).
    Entry,
    /// A lore `<beat>` of a beat bundle (dsl 0.23.0 §4).
    Bundle,
}

/// One well-formed beat of a project root (dsl 0.21.0 §4, 0.23.0 §1) — what
/// the project beat passes judge and `lute beats` lists. A beat whose `on`,
/// `target`, `priority` or `once` is malformed is `E-BEAT-ATTR`'s and is not
/// listed.
#[derive(Clone, Debug)]
pub struct ProjectBeat<'a> {
    pub path: &'a PathBuf,
    pub kind: ProjectBeatKind,
    /// A scene's canonical key (`this scene` without one), an entry's id, a
    /// bundle beat's canonical `<document id>.<beat id>`.
    pub id: String,
    pub on: &'a str,
    pub target: Option<&'a str>,
    /// dsl 0.26.0 §5: a `target="kind:<kind>"` beat's `<prefix>.<member>`
    /// targets (`target` keeps the authored `kind:<kind>`).
    pub kind_targets: Option<Vec<String>>,
    /// dsl 0.27.0 §3: the authored `for` (`kind:<kind>`) and, when the
    /// checker accepts it ([`crate::occasion_bind::for_kind_members`]), its
    /// kind and members — the beat is presented once per member.
    pub for_kind: Option<(&'a str, Option<(String, Vec<String>)>)>,
    pub priority: i64,
    /// dsl 0.28.0 §4: a scene's `priority` the manifest's `chapters:`
    /// derived from its place in the chain, not one it wrote.
    pub priority_derived: bool,
    /// A scene's policy; an entry's authored `once` ([`BeatOnce::None`] when
    /// absent — an entry without `once` is repeatable).
    pub once: BeatOnce,
    /// `once` is written rather than defaulted (an entry's `once` always is).
    pub once_authored: bool,
    /// dsl 0.23.0 §3: a scene's `also: true`.
    pub also: bool,
    /// A scene's / bundle beat's non-blank `after:` / `after=`, raw.
    pub after: Option<&'a str>,
    /// dsl 0.25.0 §2: the well-formed `share` key and where it is written.
    pub share: Option<(&'a str, Span)>,
    /// The `when` slot as authored.
    pub when_slot: Option<&'a CelSlot>,
    /// The `when` after `@def` expansion in its own document.
    pub when: Option<String>,
    /// A `<beat use>` whose own `when=` replaces its template's `when:`
    /// (`W-TEMPLATE-OVERRIDE`): the template's name and that `when:` as its
    /// header writes it.
    pub replaces_when: Option<(&'a str, &'a str)>,
    /// dsl 0.27.0 §5: the `spentBy` condition after `@def` expansion — the
    /// beat is eligible only while it does not hold.
    pub spent_by: Option<String>,
    /// The scene's frontmatter `title:` / the entry's `title=`.
    pub title: Option<String>,
    /// The `on` key / attribute — where a beat diagnostic anchors.
    pub anchor: Span,
    /// dsl 0.31.0 §1: the clock movement performed when this beat presents.
    pub advances: Option<AdvanceSpec>,
    pub folded: &'a FoldedEnv,
    /// The unit the beat presents, as [`crate::cast::FactProducers`] keys
    /// its assert sites: `0` for a scene, else the entry's / bundle beat's
    /// span start.
    pub unit: usize,
    /// The bytes of the beat's declaration in its document — the whole
    /// document for a scene, the element for an entry or bundle beat. An
    /// error reported inside it (or in the document's frontmatter) leaves
    /// the beat out of the selection passes ([`check_project_beats`]).
    pub extent: std::ops::Range<usize>,
}

impl ProjectBeat<'_> {
    /// The beat as messages name it: ``scene `k` `` / ``entry `id` `` /
    /// ``beat `doc.id` ``.
    pub fn name(&self) -> String {
        match self.kind {
            ProjectBeatKind::Scene => format!("scene `{}`", self.id),
            ProjectBeatKind::Entry => format!("entry `{}`", self.id),
            ProjectBeatKind::Bundle => format!("beat `{}`", self.id),
        }
    }

    /// The targets the beat answers.
    pub fn cells(&self) -> BeatCells<'_> {
        BeatCells::of(self.target, self.kind_targets.as_deref())
    }
}

/// dsl 0.26.0 §5: the targets one beat answers — every target (untargeted),
/// one, or each member of a kind (`target="kind:<kind>"`).
#[derive(Clone, Copy, Debug)]
pub enum BeatCells<'a> {
    Any,
    One(&'a str),
    Kind(&'a [String]),
}

impl<'a> BeatCells<'a> {
    pub fn of(target: Option<&'a str>, kind_targets: Option<&'a [String]>) -> Self {
        match (kind_targets, target) {
            (Some(ts), _) => BeatCells::Kind(ts),
            (None, Some(t)) => BeatCells::One(t),
            (None, None) => BeatCells::Any,
        }
    }

    /// Whether the beat is a candidate when the occasion is raised for `t`.
    pub fn answers(self, t: &str) -> bool {
        match self {
            BeatCells::Any => true,
            BeatCells::One(x) => x == t,
            BeatCells::Kind(ts) => ts.iter().any(|x| x == t),
        }
    }

    /// Whether every target `other` answers, this beat answers too.
    pub fn covers(self, other: BeatCells<'_>) -> bool {
        match other {
            BeatCells::Any => matches!(self, BeatCells::Any),
            BeatCells::One(t) => self.answers(t),
            BeatCells::Kind(ts) => ts.iter().all(|t| self.answers(t)),
        }
    }

    /// Whether some raise has both beats as candidates.
    pub fn meets(self, other: BeatCells<'_>) -> bool {
        match (self, other) {
            (BeatCells::Any, _) | (_, BeatCells::Any) => true,
            (BeatCells::One(t), o) | (o, BeatCells::One(t)) => o.answers(t),
            (BeatCells::Kind(a), BeatCells::Kind(b)) => a.iter().any(|t| b.contains(t)),
        }
    }

    /// dsl 0.26.0 §5: at equal priority a kind beat ranks after every other
    /// candidate (the beat naming the member outranks it).
    pub fn is_kind(self) -> bool {
        matches!(self, BeatCells::Kind(_))
    }

    /// dsl 0.27.0 (T3-10): both are kind beats and one's members are a
    /// strict subset of the other's — a sub-kind and its parent, which
    /// [`selection_order`] ranks by specificity, never by file order.
    pub fn nested(self, other: BeatCells<'_>) -> bool {
        match (self, other) {
            (BeatCells::Kind(a), BeatCells::Kind(b)) => strict_subset(a, b) || strict_subset(b, a),
            _ => false,
        }
    }
}

pub(super) fn strict_subset(a: &[String], b: &[String]) -> bool {
    a.len() < b.len() && a.iter().all(|m| b.contains(m))
}

/// dsl 0.26.0 §5, dsl 0.27.0 (T3-10): the selection order of beats given in
/// project order as `(on, priority, kind members)` — the indices, priority
/// descending; at equal priority a beat naming its target (or none) before
/// a kind beat, and a kind beat before every kind beat on its occasion whose
/// members strictly include its own (a sub-kind before its parent: member >
/// sub-kind > kind); otherwise project order. Each beat in turn is placed
/// just before the first placed beat of its occasion, priority and rank that
/// strictly includes it, else last — so the order restricted to one raise's
/// candidates (which hold every beat including a candidate kind beat) is the
/// order of those candidates alone. The one rule `check-project`,
/// `lute beats` and `lute play` rank by.
pub fn selection_order(keys: &[(&str, i64, Option<&[String]>)]) -> Vec<usize> {
    let mut sorted: Vec<usize> = (0..keys.len()).collect();
    sorted.sort_by_key(|&i| (std::cmp::Reverse(keys[i].1), keys[i].2.is_some()));
    let mut out: Vec<usize> = Vec::with_capacity(sorted.len());
    for i in sorted {
        let (on, priority, members) = keys[i];
        let at = members.and_then(|ms| {
            out.iter().position(|&o| {
                let (on2, p2, ms2) = keys[o];
                on2 == on && p2 == priority && ms2.is_some_and(|ms2| strict_subset(ms, ms2))
            })
        });
        match at {
            Some(k) => out.insert(k, i),
            None => out.push(i),
        }
    }
    out
}

/// `items` permuted into `order` (a permutation of its indices).
pub fn reorder<T>(items: Vec<T>, order: &[usize]) -> Vec<T> {
    let mut slots: Vec<Option<T>> = items.into_iter().map(Some).collect();
    order.iter().filter_map(|&i| slots[i].take()).collect()
}

/// [`project_beats`] (project order) in [`selection_order`].
pub fn in_selection_order(beats: Vec<ProjectBeat<'_>>) -> Vec<ProjectBeat<'_>> {
    let order = selection_order(
        &beats
            .iter()
            .map(|b| (b.on, b.priority, b.kind_targets.as_deref()))
            .collect::<Vec<_>>(),
    );
    reorder(beats, &order)
}

/// Every well-formed beat of one project root, in `ProjectIndex.beats`
/// order: `docs` (parallel to `foldeds`) in `check-project` order — the
/// selection tiebreak after priority — then declaration order within a
/// document (a lore document's entry beats and bundle beats interleave by
/// source position, as their compiled records do).
pub fn project_beats<'a>(
    docs: &'a [(PathBuf, Document)],
    foldeds: &[&'a FoldedEnv],
) -> Vec<ProjectBeat<'a>> {
    let mut beats = Vec::new();
    for ((path, doc), &folded) in docs.iter().zip(foldeds) {
        let defs = DefTable {
            bodies: &folded.def_bodies,
            params: &folded.env.def_params,
        };
        let expand = |when: Option<&CelSlot>| {
            when.map(|w| {
                let mut stack = Vec::new();
                crate::cel_expand::expand_cel(&w.raw, &defs, None, &mut stack)
                    .unwrap_or_else(|_| w.raw.clone())
            })
        };
        // dsl 0.26.0 §5: `Some(None)` for a beat without a `kind:` target,
        // `Some(Some(targets))` for one whose kind resolves, `None` for one
        // `E-BEAT-ATTR` rejects (not listed).
        let kind_cells = |on: &str, target: Option<&str>| -> Option<Option<Vec<String>>> {
            let Some(kind) = target.and_then(kind_target) else {
                return Some(None);
            };
            let decl = folded.occasions.get(on)?;
            let (prefix, members) =
                kind_target_members(decl, kind, &folded.env.rel_vocab.kinds).ok()?;
            Some(Some(
                members.iter().map(|m| format!("{prefix}.{m}")).collect(),
            ))
        };
        let for_cell = |on: &str, raw: Option<&'a str>, has_target: bool| {
            raw.map(|raw| {
                let kind = crate::occasion_bind::for_kind_members(
                    on,
                    raw,
                    has_target,
                    &folded.occasions,
                    &folded.env.rel_vocab.kinds,
                );
                (raw, kind.ok())
            })
        };
        if let Some((beat, kind_targets)) = folded
            .typed
            .beat
            .as_ref()
            .and_then(|b| Some((b, kind_cells(&b.on, b.target.as_deref())?)))
        {
            let title = serde_yaml::from_str::<serde_yaml::Mapping>(&doc.meta.raw_yaml)
                .ok()
                .and_then(|m| {
                    m.get(serde_yaml::Value::String("title".to_string()))?
                        .as_str()
                        .map(str::to_string)
                });
            beats.push(ProjectBeat {
                path,
                kind: ProjectBeatKind::Scene,
                id: scene_beat_name(folded),
                on: &beat.on,
                target: beat.target.as_deref(),
                kind_targets,
                for_kind: for_cell(
                    &beat.on,
                    beat.for_kind.as_ref().map(|(f, _)| f.as_str()),
                    beat.target.is_some(),
                ),
                priority: beat.priority,
                priority_derived: crate::chapters::derived(&doc.meta, "priority"),
                once: beat.once.clone(),
                once_authored: beat.once_authored,
                also: beat.also,
                after: folded
                    .typed
                    .after
                    .as_deref()
                    .filter(|a| !a.trim().is_empty()),
                // dsl 0.25.0 §2: a `share` without a written, spending `once`
                // is `E-BEAT-ATTR`'s alone; it joins no key.
                share: beat
                    .share
                    .as_deref()
                    .filter(|_| beat.once_authored && beat.once != BeatOnce::None)
                    .map(|k| (k, top_value_span(&doc.meta, "share"))),
                when_slot: beat.when.as_ref(),
                when: expand(beat.when.as_ref()),
                advances: beat.advances,
                replaces_when: None,
                spent_by: expand(beat.spent_by.as_ref()),
                title,
                anchor: top_key_span(&doc.meta, "on"),
                folded,
                unit: 0,
                extent: 0..usize::MAX,
            });
        }
        // A lore document's entry beats and bundle beats, by source position.
        let mut lore: Vec<(usize, ProjectBeat<'a>)> = Vec::new();
        for entry in &doc.entries {
            let Some((on, on_span)) = entry.on.as_ref().filter(|(on, _)| is_name(on)) else {
                continue;
            };
            let priority = match &entry.priority {
                None => 0,
                Some((raw, _)) => match parse_beat_priority(raw) {
                    Some(p) => p,
                    None => continue,
                },
            };
            // dsl 0.24.0 §6: on an occasion declared without a target the
            // entry's `target=` is metadata, not a candidate restriction.
            let target = match &entry.target {
                None => None,
                Some((t, _)) if kind_target(t).is_some() => Some(t.as_str()),
                Some((t, _)) if is_entry_target(t) => {
                    beat_target_restricts(on, &folded.occasions).then_some(t.as_str())
                }
                Some(_) => continue,
            };
            let Some(kind_targets) = kind_cells(on, target) else {
                continue;
            };
            let once = match entry.once.as_ref().map(|(o, _)| o.as_str()) {
                // A `spentBy` entry stays spent for its `once` period, `run`
                // unless written.
                None if entry.spent_by.is_some() => BeatOnce::Run,
                None | Some("false") => BeatOnce::None,
                Some(raw) => match BeatOnce::parse(raw) {
                    Some(once) => once,
                    None => continue,
                },
            };
            lore.push((
                entry.span.byte_start,
                ProjectBeat {
                    path,
                    kind: ProjectBeatKind::Entry,
                    id: entry.id.clone(),
                    on,
                    target,
                    kind_targets,
                    for_kind: for_cell(
                        on,
                        entry.for_kind.as_ref().map(|(f, _)| f.as_str()),
                        entry.target.is_some(),
                    ),
                    priority,
                    priority_derived: false,
                    once_authored: entry.once.is_some() || entry.spent_by.is_some(),
                    also: false,
                    after: None,
                    share: well_formed_share(entry.share.as_ref())
                        .filter(|_| once != BeatOnce::None),
                    once,
                    when_slot: entry.when.as_ref(),
                    when: expand(entry.when.as_ref()),
                    advances: entry
                        .advances
                        .as_ref()
                        .and_then(|(raw, _)| parse_advances_text(raw)),
                    replaces_when: None,
                    spent_by: expand(entry.spent_by.as_ref()),
                    title: entry.title.as_ref().map(|(t, _)| t.clone()),
                    anchor: *on_span,
                    folded,
                    unit: entry.span.byte_start,
                    extent: entry.span.byte_start..entry.span.byte_end,
                },
            ));
        }
        // dsl 0.23.0 §4: bundle beats — skipped without a document `id:`
        // (their canonical id hangs off it; `E-BEAT-ATTR`) or with a
        // malformed `on` / `target` / `priority` / `once`.
        if let Some(doc_id) = folded.typed.id.as_deref() {
            for beat in &doc.beats {
                let Some((on, on_span)) = beat.on.as_ref().filter(|(on, _)| is_name(on)) else {
                    continue;
                };
                if beat.id.is_empty()
                    // A use of a faulty template derives only what is sound;
                    // the header's one report stands for the beat.
                    || beat.template.as_ref().is_some_and(|t| t.failed)
                    || beat
                        .target
                        .as_ref()
                        .is_some_and(|(t, _)| !is_beat_target(t))
                    || beat
                        .priority
                        .as_ref()
                        .is_some_and(|(p, _)| parse_beat_priority(p).is_none())
                    || beat
                        .once
                        .as_ref()
                        .is_some_and(|(o, _)| o != "false" && BeatOnce::parse(o).is_none())
                {
                    continue;
                }
                let target = beat.target.as_ref().map(|(t, _)| t.as_str());
                let Some(kind_targets) = kind_cells(on, target) else {
                    continue;
                };
                lore.push((
                    beat.span.byte_start,
                    ProjectBeat {
                        path,
                        kind: ProjectBeatKind::Bundle,
                        id: crate::bundles::bundle_beat_key(doc_id, &beat.id),
                        on,
                        target,
                        kind_targets,
                        for_kind: for_cell(
                            on,
                            beat.for_kind.as_ref().map(|(f, _)| f.as_str()),
                            beat.target.is_some(),
                        ),
                        priority: crate::bundles::bundle_beat_priority(beat),
                        priority_derived: false,
                        once: crate::bundles::bundle_beat_once(beat),
                        once_authored: beat.once.is_some() || beat.spent_by.is_some(),
                        also: crate::bundles::bundle_beat_also(beat),
                        after: beat
                            .after
                            .as_ref()
                            .map(|(a, _)| a.as_str())
                            .filter(|a| !a.trim().is_empty()),
                        share: well_formed_share(beat.share.as_ref())
                            .filter(|_| beat.once.as_ref().is_some_and(|(o, _)| o != "false")),
                        when_slot: beat.when.as_ref(),
                        when: expand(beat.when.as_ref()),
                        advances: beat
                            .advances
                            .as_ref()
                            .and_then(|(raw, _)| parse_advances_text(raw)),
                        replaces_when: beat
                            .template
                            .as_ref()
                            .and_then(|t| t.replaced_when.as_deref().map(|w| (t.name.as_str(), w))),
                        spent_by: expand(beat.spent_by.as_ref()),
                        title: beat.title.as_ref().map(|(t, _)| t.clone()),
                        anchor: *on_span,
                        folded,
                        unit: beat.span.byte_start,
                        extent: beat.span.byte_start..beat.span.byte_end,
                    },
                ));
            }
        }
        lore.sort_by_key(|(at, _)| *at);
        beats.extend(lore.into_iter().map(|(_, b)| b));
    }
    beats
}

