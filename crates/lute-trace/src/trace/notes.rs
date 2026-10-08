use super::*;

// ---------------------------------------------------------------------
// Texts, notes and pre-walk judgments over the normalized document.
/// One `UnresolvedAtom` -> its §4.6 "supply it as a mock" hint text.
pub(super) fn render_atom(a: &UnresolvedAtom) -> String {
    match a {
        UnresolvedAtom::Path(p) => format!("--state {p}=<value>"),
        UnresolvedAtom::Fact(f) | UnresolvedAtom::DerivedFact(f) => {
            let f = lute_runtime::datalog::fact_spelling(f);
            // A quoted argument keeps the shell's quotes apart from CEL's.
            if f.contains('"') {
                format!("--fact '{f}'")
            } else {
                format!("--fact \"{f}\"")
            }
        }
        UnresolvedAtom::Time => "no mock surface for narrative time".to_string(),
    }
}

pub(super) fn fact_term_text(t: &FactTerm) -> String {
    match t {
        FactTerm::Ident(s) => s.clone(),
        FactTerm::Bool(b) => b.to_string(),
        FactTerm::Wildcard => "_".to_string(),
        FactTerm::Param(p) => format!("@{p}"),
        FactTerm::Target => lute_manifest::semantics::beats::OCCASION_TARGET.to_string(),
    }
}

pub(super) fn fmt_fact(rel: &str, args: &[String]) -> String {
    format!("{rel}({})", args.join(", "))
}

/// Canonical `rel(a, b)` key for a fact/mock pattern (§3.1) — shared by
/// the schema's own seed `facts:` entries and the CLI's `--fact` mocks so
/// their declared/supplied tuples compare STRUCTURALLY, never by raw
/// source text (whitespace, quoting).
pub(super) fn fact_pattern_key(pat: &lute_manifest::fact::FactPattern) -> String {
    let args: Vec<String> = pat.args.iter().map(|a| fact_term_text(&a.term)).collect();
    fmt_fact(&pat.relation, &args)
}

/// A line's `{…}` attrs as authored (`mono`, `as="x"`, `emotion=@e`), in
/// source order — `when=` is already extracted, and `__`-prefixed compiler
/// bookkeeping is not source. `None` when nothing remains.
pub(super) fn line_delivery(l: &Line) -> Option<String> {
    let attrs: Vec<String> = l
        .attrs
        .iter()
        .filter(|a| !a.key.starts_with("__"))
        .map(|a| match &a.value {
            AttrValue::Str(s) => format!("{}=\"{}\"", a.key, s.replace('"', "\\\"")),
            AttrValue::Ref(slot) => format!("{}={}", a.key, slot.raw),
            AttrValue::BoolTrue => a.key.clone(),
        })
        .collect();
    (!attrs.is_empty()).then(|| attrs.join(" "))
}

/// A prerequisite formula as the CEL condition it stands for: `visited(K)`
/// reads the visited set, `completed(Q)` / `active(Q)` the quest's state.
pub(super) fn prereq_condition(f: &lute_check::PrereqFormula) -> String {
    use lute_check::PrereqFormula as F;
    match f {
        F::Visited(k) => format!("visited('{k}')"),
        F::Completed(q) => format!("quest.{q}.state == 'complete'"),
        F::Active(q) => format!("quest.{q}.state == 'active'"),
        F::And(a, b) => format!("({}) && ({})", prereq_condition(a), prereq_condition(b)),
        F::Or(a, b) => format!("({}) || ({})", prereq_condition(a), prereq_condition(b)),
    }
}

pub(super) fn seeds_summary(mocks: &MockSet) -> Seeds {
    Seeds {
        state_paths: mocks.state.len(),
        facts: mocks.facts.len(),
        choices: mocks.choose.values().map(Vec::len).sum(),
    }
}

/// §3.1 under `derive: false` (dsl 0.22.0 §6): the effective fact set is
/// exactly the supplied `--fact`/`--mock` mocks, never the schema's own
/// seed `facts:` block (with `derive: true`, the default, the seeds ARE
/// loaded and this note is not produced). This function ONLY decides
/// whether to render informational signage about that model: when the
/// resolved schema DECLARES seed facts and NONE of THEM (specifically —
/// not merely "the mock list happens to be empty") were supplied as
/// mocks, a `start`/`done`/guard reading one of those seeded relations
/// will decide `unknown`/`false` off an empty explicit set — which reads
/// like a bug unless the author is told why. Compares each declared seed
/// against every supplied `--fact` STRUCTURALLY (relation + args, via
/// [`fact_pattern_key`]) rather than by "is the mock list empty", so an
/// unrelated `--fact` (naming a real relation the schema just never
/// seeded) still triggers the note — only a supplied mock that matches a
/// DECLARED SEED tuple counts as "supplied". Names the first declared
/// seed with no matching supplied mock (deterministic; schema/import
/// order, `rel_schema::build_rel_vocab`'s own "imports first, then
/// inline" order) — informational only, never an error and never a
/// reachability claim; never consulted by the exit-code decision below.
pub(super) fn seed_fact_notes(mocks: &MockSet, seed_facts: &[lute_check::meta::FactDecl]) -> Vec<String> {
    if seed_facts.is_empty() {
        return Vec::new();
    }
    let supplied: std::collections::HashSet<String> = mocks
        .facts
        .iter()
        .filter_map(|raw| lute_manifest::fact::parse_fact(raw).ok())
        .map(|pat| fact_pattern_key(&pat))
        .collect();
    // "None were supplied" (§3.1) holds iff the intersection of declared
    // seed tuples and supplied mock tuples is empty — ANY one supplied
    // seed silences the note, even if other declared seeds remain
    // un-supplied (that gap is exactly what `--fact`/`--state` mocks are
    // for; this is signage about the MODEL, not a per-relation checklist).
    if seed_facts
        .iter()
        .any(|fd| supplied.contains(&fact_pattern_key(&fd.fact)))
    {
        return Vec::new();
    }
    vec![format!(
        "the schema declares seed facts (e.g. `{}`) but under `derive: false` trace does not \
         auto-load them (the explicit-world model) — supply seeded relations explicitly \
         via --fact",
        seed_facts[0].fact.relation
    )]
}

/// dsl 0.22.0 §6: under `derive: false` every derived relation a query read
/// was looked up, not derived — an unmocked atom of it read unknown. One
/// note per relation, sorted; informational only.
pub(super) fn derived_read_notes(relations: &BTreeSet<String>) -> Vec<String> {
    relations
        .iter()
        .map(|rel| {
            format!(
                "derived relation `{rel}` read under `derive: false`: its rules were not \
                 applied, so an unmocked `{rel}(…)` is unknown — supply it via --fact, or drop \
                 `derive: false`"
            )
        })
        .collect()
}

/// dsl 0.5.1 §1.3: one informational note per FOREIGN quest `<id>` — a
/// quest id NOT defined by an in-document `<quest id="…">` — that either
/// (a) has an ADMITTED `--state` mock on one of its reserved paths (§1.1)
/// — sourced from `mocks.state` DIRECTLY (post `mock::validate`, every
/// reserved entry there is already proven admitted/domain-valid), so this
/// fires regardless of whether the walk ever actually reaches a read of
/// it: "an explicitly-mocked typo is therefore never silent" (§1.1's own
/// text) holds even when the mocked path sits inside a branch/event arm
/// the walk never takes — or (b) had a reserved path actually resolve to
/// its DEFAULT during the walk (§1.2, sourced from `reserved_reads`,
/// [`lute_runtime::eval::EffectiveState::reserved_reads`] — a default is
/// necessarily read-time, there is no "admitted but unread" analog for
/// it). Grouped by quest id (one note per id, never per path);
/// informational only, never an error, never a reachability claim, exit
/// code unchanged.
pub(super) fn reserved_quest_notes(
    mocks: &MockSet,
    reserved_reads: &BTreeMap<String, bool>,
    doc_quest_ids: &BTreeSet<&str>,
) -> Vec<(String, String)> {
    let mut defaulted_by_id: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (path, defaulted) in reserved_reads {
        if !defaulted {
            continue; // a seeded read is redundant with `mocks.state` below.
        }
        let id = reserved_quest_id(path);
        if doc_quest_ids.contains(id) {
            continue; // derived by an in-document `<quest>` — not foreign.
        }
        defaulted_by_id.entry(id).or_default().push(path.as_str());
    }
    let mut ids: BTreeSet<&str> = defaulted_by_id.keys().copied().collect();
    for (path, _, _) in &mocks.state {
        if !lute_runtime::eval::is_reserved_quest_path(path) {
            continue;
        }
        let id = reserved_quest_id(path);
        if doc_quest_ids.contains(id) {
            continue;
        }
        ids.insert(id);
    }
    let mut notes = Vec::new();
    for id in ids {
        let mut defaults: Vec<String> = defaulted_by_id
            .get(id)
            .into_iter()
            .flatten()
            .map(|path| {
                format!(
                    "`{path}` defaults to `{}` (override via --state)",
                    reserved_default_text(path)
                )
            })
            .collect();
        defaults.sort();
        let mut msg = format!(
            "{}(run `check-project`, or trace with `--project`, to confirm it is defined by a \
             project quest)",
            unverified_quest_note_head(id)
        );
        if !defaults.is_empty() {
            msg.push_str("; ");
            msg.push_str(&defaults.join("; "));
        }
        notes.push((id.to_string(), msg));
    }
    notes
}

/// The fixed head of a foreign quest's "existence is unverified" note —
/// what [`crate::TraceReport::verify_quests`] finds it by.
pub(crate) fn unverified_quest_note_head(id: &str) -> String {
    format!("quest `{id}`'s existence is unverified by trace ")
}

/// The `<id>` segment of a reserved `quest.<id>.state`/`quest.<id>.
/// objectives.<oid>.done` path.
pub(super) fn reserved_quest_id(path: &str) -> &str {
    path.split('.').nth(1).unwrap_or(path)
}

pub(super) fn reserved_default_text(path: &str) -> &'static str {
    if lute_runtime::eval::is_reserved_quest_objective_done_path(path) {
        "false"
    } else {
        "unset"
    }
}

/// dsl 0.5.1 §4: a `--event <name>` matching no `<on event=…>` handler
/// ANYWHERE in the traced document (a scene has none at all; a quest
/// document's own handlers are collected regardless of whether the owning
/// quest ever activated — purely structural: "does the document define a
/// handler for this name") gets an informational note instead of a silent
/// no-op. A built-in lifecycle name is rejected pre-walk (`E-TRACE-EVENT`,
/// [`mock::validate`]), so it can never reach here. Deduplicated per name,
/// `events` order.
pub(super) fn unmatched_event_notes(doc: &Document, events: &[String]) -> Vec<String> {
    let mut handled = BTreeSet::new();
    for quest in &doc.quests {
        collect_on_events(&quest.body, &mut handled);
    }
    let mut seen = BTreeSet::new();
    let mut notes = Vec::new();
    for name in events {
        if handled.contains(name.as_str()) || !seen.insert(name.as_str()) {
            continue;
        }
        notes.push(format!("event `{name}` matched no `<on>` handler"));
    }
    notes
}

/// dsl 0.21.0 §7a.2: the two silences an occasion-judged objective invites,
/// named as notes rather than left for the author to infer. A raised
/// occasion that no `<objective on>` in the document answers did nothing;
/// an `on=` objective of a quest the walk left `active`, never done, whose
/// occasion the walk never raised was never judged at all — the quest is
/// not stuck, it is waiting for a moment the mock did not supply.
/// Read off the recorded decisions (the last `quest` outcome per id, an
/// `objective` `done` at the objective's own span), never the reserved
/// state paths, so the §1.3 reserved-read log is untouched. A raise that
/// bound the presented unit's `occasion.target` (`binding`) did something.
pub(super) fn occasion_notes(
    doc: &Document,
    occasions: &[String],
    binding: &BTreeSet<String>,
    decisions: &[Decision],
) -> Vec<String> {
    let mut notes = Vec::new();
    // 0.23.1: a same-named `<on event>` handler answers a raise too.
    let answered: BTreeSet<&str> = doc
        .quests
        .iter()
        .flat_map(|q| &q.body)
        .filter_map(|n| match n {
            Node::Objective(o) => o.on.as_ref().map(|(on, _)| on.as_str()),
            Node::On(on) => Some(on.event.as_str()),
            _ => None,
        })
        .collect();
    let mut seen = BTreeSet::new();
    for name in occasions {
        let (bare, _) = lute_runtime::split_occasion(name);
        if answered.contains(bare) || binding.contains(bare) || !seen.insert(name.as_str()) {
            continue;
        }
        notes.push(format!(
            "occasion `{name}` is judged by no `<objective on>` and fires no `<on event>` \
             handler in this document"
        ));
    }
    for quest in &doc.quests {
        let last = decisions
            .iter()
            .rev()
            .find(|d| d.construct == "quest" && d.id == quest.id)
            .map(|d| d.outcome.as_str());
        if last != Some("active") {
            continue;
        }
        for node in &quest.body {
            let Node::Objective(o) = node else { continue };
            let Some((on, _)) = &o.on else { continue };
            let target = o.target.as_ref().map(|(t, _)| t.as_str());
            let settled = decisions.iter().any(|d| {
                d.construct == "objective"
                    && d.span == o.span
                    && matches!(d.outcome.as_str(), "done" | "failed")
            });
            if settled
                || occasions
                    .iter()
                    .any(|r| lute_runtime::raise_judges(r, on, target))
            {
                continue;
            }
            let raise = target.map_or_else(|| on.clone(), |t| format!("{on}@{t}"));
            let what = target.map_or_else(|| format!("`{on}`"), |t| format!("`{on}` for `{t}`"));
            notes.push(format!(
                "objective `{}.{}` is judged at occasion {what}, which this walk never raised \
                 (supply `--occasion {raise}` or `occasions: [{raise}]`)",
                quest.id, o.id
            ));
        }
    }
    notes
}

/// spec §4 (0.6.1): one [`crate::mock::W_TRACE_MOCK_UNPRODUCIBLE`] note per
/// supplied `--fact`/mock-YAML fact whose relation the CHECKER's
/// [`lute_check::producible::producible`] judges NOT producible — the mocked
/// answer can never arise from authored producers, so a "complete" walk seeded
/// with it proves nothing about reachable play. WARNING only: rides the §3.1
/// additive `notes` surface (report.rs), which is never consulted for the
/// exit-code decision — a mock is a hypothesis and the walk verdict stands
/// (D1: the checker's `producible()` computes at build-input time from the SAME
/// `FoldedEnv` vocab check uses, and trace only DISPLAYS the result — it runs
/// no Datalog itself). A `reserved: true`/`open: engine`-argument relation is
/// producible by definition and never warns (already encoded in `producible()`).
///
/// `live_assert_relations` is passed CONSERVATIVELY: [`collect_assert_relations`]
/// gathers EVERY `::assert{R(…)}` site in the normalized (component-inlined)
/// document with NO reachability gating — trace has no T6 reachability pass,
/// unlike `check-project`'s [`lute_check::connectivity::live_assert_relations`].
/// A larger live set makes MORE relations producible, so a conservative
/// all-sites set only ever UNDER-warns (a relation `check` would prove dead via
/// reachability may still read producible here) — never a false positive.
/// Component-inlined asserts land in the post-normalize `doc`, so collecting
/// there (not pre-normalize) is what keeps a `::use`d producer from false-
/// warning its relation. An undeclared mock relation was already
/// `E-TRACE-MOCK-FACT`-refused upstream ([`crate::mock::validate`]), so every
/// relation reaching here has a `producible` entry; `Some(false)` alone warns.
/// Deduplicated per relation (deterministic `BTreeSet` order).
///
/// T1-14 (0.21.1): the document's own asserts are only the producers IN THIS
/// DOCUMENT. A relation asserted by a sibling scene is producible in play, so
/// judging by this document alone warned on every piece of evidence a
/// mystery's other scenes establish — and on every relation derived from
/// them. When the caller resolved the project (`--project`, or the nearest
/// `lute.project.yaml`), `project_asserts` is `check-project`'s own
/// reachability-gated May set ([`lute_check::connectivity::live_assert_relations`])
/// and is unioned in; without one the note says it judged this document only.
pub(super) fn mock_unproducible_notes(
    mocks: &MockSet,
    folded: &FoldedEnv,
    doc: &Document,
    project_asserts: Option<&BTreeSet<String>>,
) -> Vec<String> {
    if mocks.facts.is_empty() {
        return Vec::new();
    }
    let mut live_assert: BTreeSet<String> = project_asserts.cloned().unwrap_or_default();
    for shot in &doc.sections {
        collect_assert_relations(&shot.body, &mut live_assert);
    }
    for quest in &doc.quests {
        collect_assert_relations(&quest.body, &mut live_assert);
    }
    for entry in &doc.entries {
        collect_assert_relations(&entry.body, &mut live_assert);
    }
    for beat in &doc.beats {
        collect_assert_relations(&beat.body, &mut live_assert);
    }
    let producible = lute_check::producible::producible(&folded.env.rel_vocab, &live_assert);
    let mut unproducible: BTreeSet<String> = BTreeSet::new();
    for raw in &mocks.facts {
        let Ok(pat) = lute_manifest::fact::parse_fact(raw) else {
            continue;
        };
        if producible.get(&pat.relation) == Some(&false) {
            unproducible.insert(pat.relation.clone());
        }
    }
    let scope = if project_asserts.is_some() {
        "no reachable `::assert` anywhere in the project"
    } else {
        "no `::assert` in this document — judged against this document only; pass \
         `--project <dir>` to count the asserts of the project's other documents"
    };
    unproducible
        .into_iter()
        .map(|rel| {
            format!(
                "{W_TRACE_MOCK_UNPRODUCIBLE} — mock fact over relation `{rel}` is not \
                 producible (no `facts:` seed, {scope}, not `reserved`) — the supplied answer \
                 can never arise from authored producers, so a complete walk seeded with it \
                 proves nothing about reachable play"
            )
        })
        .collect()
}

/// Conservative all-sites collection of every `::assert{R(…)}` relation name in
/// a node stream (spec §4, [`mock_unproducible_notes`]) — mirrors
/// [`lute_check::connectivity::live_assert_relations`]'s own
/// `collect_assert_relations` traversal shape, but WITHOUT its reachability
/// gate (trace has none). A parse-failed assert (`relation.is_empty()`, D13)
/// contributes nothing. The `match` is exhaustive (no `_` arm) so a new
/// [`Node`] variant that can host an assert forces a compile error here rather
/// than silently under-collecting into a false-positive warning.
pub(super) fn collect_assert_relations(nodes: &[Node], out: &mut BTreeSet<String>) {
    for node in nodes {
        match node {
            Node::Assert(a) => {
                if !a.pattern.relation.is_empty() {
                    out.insert(a.pattern.relation.clone());
                }
            }
            Node::Match(m) => {
                for arm in &m.arms {
                    let body = match arm {
                        Arm::When { body, .. } | Arm::Otherwise { body, .. } => body,
                    };
                    collect_assert_relations(body, out);
                }
            }
            Node::Branch(b) => {
                for choice in &b.choices {
                    collect_assert_relations(&choice.body, out);
                }
            }
            Node::Hub(h) => {
                for b in h.bodies() {
                    collect_assert_relations(b, out);
                }
            }
            Node::On(o) => collect_assert_relations(&o.body, out),
            Node::Objective(o) => collect_assert_relations(&o.body, out),
            Node::Line(_)
            | Node::Directive(_)
            | Node::Set(_)
            | Node::Timeline(_)
            | Node::Retract(_) => {}
        }
    }
}

/// Every `<on event=…>` name reachable in `nodes`, recursing into nested
/// constructs (a `<branch>`/`<hub>` choice body, a `<match>` arm body, an
/// `<objective>`/`<on>` body) — mirrors `mock.rs`'s
/// `collect_choice_ids_nodes` recursion shape.
pub(super) fn collect_on_events<'a>(nodes: &'a [Node], out: &mut BTreeSet<&'a str>) {
    for node in nodes {
        match node {
            Node::Branch(b) => {
                for choice in &b.choices {
                    collect_on_events(&choice.body, out);
                }
            }
            Node::Hub(h) => {
                for b in h.bodies() {
                    collect_on_events(b, out);
                }
            }
            Node::Match(m) => {
                for arm in &m.arms {
                    match arm {
                        Arm::When { body, .. } | Arm::Otherwise { body, .. } => {
                            collect_on_events(body, out)
                        }
                    }
                }
            }
            Node::On(o) => {
                out.insert(o.event.as_str());
                collect_on_events(&o.body, out);
            }
            Node::Objective(o) => collect_on_events(&o.body, out),
            Node::Line(_)
            | Node::Directive(_)
            | Node::Set(_)
            | Node::Timeline(_)
            | Node::Assert(_)
            | Node::Retract(_) => {}
        }
    }
}

/// The prefix every beat-`when` note ([`beat_when_note`]) starts with, so a
/// harness can surface that note without re-deriving it.
pub const NOTE_BEAT_WHEN: &str = "beat `when`";

/// dsl 0.24.0 §2 (ER N15): the prefix of the note for an `accepts:` of an
/// `activate="accept"` child spent while its parent was not active —
/// `lute test` surfaces it beside a failing quest expectation.
pub const NOTE_ACCEPT_SPENT: &str = "accept of";

/// T1-13: the note for a beat scene (dsl 0.21.0 §3.1) whose frontmatter
/// `when` does not hold under the supplied mocks. Trace walks the body
/// regardless — it was asked to — so without this a scene the selector would
/// never present read as a plain `complete`. The slot is expanded exactly as
/// `lute compile` expands it (`expand_beat_when`) and evaluated against the
/// pre-walk state. `false` and `unknown` are both named; `true` and "no
/// `when`" say nothing. Informational only — never the exit code.
pub(super) fn beat_when_note(
    folded: &FoldedEnv,
    table: &DefTable<'_>,
    m: &mut Machine<&mut TraceDriver<'_>>,
) -> Option<String> {
    let beat = folded.typed.beat.as_ref()?;
    let mut slot = beat.when.clone()?;
    let _ = lute_compile::expand::expand_beat_when(&mut slot, table);
    let v = match lowered(&slot.raw) {
        Some(cond) => m.eval_guard(&cond),
        None => Err(Vec::new()),
    };
    let raw = beat.when.as_ref().map(|s| s.raw.trim()).unwrap_or_default();
    match v {
        Ok(false) => Some(format!(
            "{NOTE_BEAT_WHEN} ({raw}) is false under these mocks — the `{}` selector would never \
             present this scene; the walk below shows it as if it had been presented",
            beat.on
        )),
        Ok(true) => None,
        Err(atoms) => {
            let supply: Vec<String> = atoms.iter().map(render_atom).collect();
            Some(format!(
                "{NOTE_BEAT_WHEN} ({raw}) is undecided under these mocks{} — whether the `{}` \
                 selector presents this scene is unknown",
                if supply.is_empty() {
                    String::new()
                } else {
                    format!(" (supply {})", supply.join(", "))
                },
                beat.on
            ))
        }
    }
}

pub(super) fn empty_report(uri: &str, mocks: &MockSet) -> TraceReport {
    TraceReport {
        evidence: lute_core_span::Evidence::Unknown,
        file: uri.to_string(),
        seeds: seeds_summary(mocks),
        steps: Vec::new(),
        decisions: Vec::new(),
        unresolved: Vec::new(),
        coverage: Coverage::default(),
        notes: Vec::new(),
        disposition: "refused".to_string(),
        end_reason: None,
        forced_unknown: Vec::new(),
        final_state: BTreeMap::new(),
        final_facts: BTreeSet::new(),
        final_undecided: BTreeSet::new(),
        foreign_quests: BTreeSet::new(),
        scene_eligible: None,
        premises: BTreeMap::new(),
        not_raised: BTreeMap::new(),
        ineligible_by: BTreeMap::new(),
        said: Vec::new(),
        terminal: None,
    }
}
