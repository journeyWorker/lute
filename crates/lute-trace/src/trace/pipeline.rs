use super::*;
/// The §4.3/§4.4/§4.5 pipeline, end to end: check gate -> mock validation
/// -> compile -> the walk -> the §4.5 report + exit code. Never panics —
/// every degrade path returns a (possibly empty) report alongside the
/// appropriate [`TraceExit`].
pub fn trace_document(input: &CheckInput, mocks: MockSet) -> (TraceReport, TraceExit) {
    trace_with_check(input, lute_check::check(input), mocks, None)
}

/// Like [`trace_document`] but gated on a CALLER-SUPPLIED [`CheckResult`]
/// instead of running `lute_check::check(input)` itself — the seam the
/// project-aware CLI gate (connectivity design spec §5) uses to feed the
/// target document's RECONCILED `check-project` verdict, mirroring
/// `lute_compile::compile_with_check`. The D1 quarantine is intact: the
/// reconciliation is pure graph math the CLI performs, never CEL/Datalog
/// evaluation, and `trace`'s evaluated subset is unchanged.
///
/// `project_asserts` is the project's May producer set — `check-project`'s
/// [`lute_check::connectivity::live_assert_relations`] for the root the
/// document belongs to — when the caller resolved one; it widens
/// `W-TRACE-MOCK-UNPRODUCIBLE`'s producer set past this document (T1-14).
/// `None` judges this document alone and says so.
pub fn trace_with_check(
    input: &CheckInput,
    result: CheckResult,
    mocks: MockSet,
    project_asserts: Option<&BTreeSet<String>>,
) -> (TraceReport, TraceExit) {
    trace_pipeline(
        input,
        result,
        mocks,
        Presentation::Document,
        project_asserts,
    )
}

/// `lute trace --entry <id>` (dsl 0.19.0 §8): the SAME pipeline as
/// [`trace_document`], then PRESENTS the one `<entry id="<id>">` instead of
/// walking shots/quests (`docs/runtime/lore-entries.md` `present()`:
/// first-read effects apply only while `entry.<id>.read` is false; seed it
/// `true` in the mock to preview a re-read). A non-lore document or an
/// unknown id is refused with [`crate::mock::E_TRACE_ENTRY`] (exit 1), like
/// `--accept` of an unknown quest.
pub fn trace_entry(input: &CheckInput, mocks: MockSet, entry: &str) -> (TraceReport, TraceExit) {
    trace_entry_with_check(input, lute_check::check(input), mocks, entry, None)
}

/// [`trace_entry`] gated on a caller-supplied [`CheckResult`] — the
/// project-aware seam [`trace_with_check`] documents.
pub fn trace_entry_with_check(
    input: &CheckInput,
    result: CheckResult,
    mocks: MockSet,
    entry: &str,
    project_asserts: Option<&BTreeSet<String>>,
) -> (TraceReport, TraceExit) {
    trace_entries_with_check(input, result, mocks, &[entry], project_asserts)
}

/// `lute test`'s `entries: [ids]` (dsl 0.22.0 §5): [`trace_entry_with_check`]
/// over a SEQUENCE — each entry is presented in order against the state the
/// previous ones left, with the engine's post-presentation `entry.<id>.read`
/// / `entry.<id>.everRead` writes between them, so a repeated id is a
/// re-read (first-read effects skipped). Every id is validated before any
/// is presented; the walk stops at the first entry that does not continue.
pub fn trace_entries_with_check(
    input: &CheckInput,
    result: CheckResult,
    mocks: MockSet,
    entries: &[&str],
    project_asserts: Option<&BTreeSet<String>>,
) -> (TraceReport, TraceExit) {
    trace_pipeline(
        input,
        result,
        mocks,
        Presentation::Entries(entries),
        project_asserts,
    )
}

/// `lute trace --beat <id>` (dsl 0.23.0 §4): the SAME pipeline as
/// [`trace_document`], then PRESENTS the one bundle `<beat>` of a lore
/// document — its body walked exactly as a scene shot body (choices, hubs
/// and matches honour `choose:`; every effect applies; `scene.*` fresh),
/// its `when` shown on the [`Step::Beat`] head, not enforced. `beat` is the
/// local `<beat id>` or the canonical `<document id>.<beat id>`. A non-lore
/// document or an unknown id is refused with [`crate::mock::E_TRACE_BEAT`]
/// (exit 1), like `--entry`.
pub fn trace_beat(input: &CheckInput, mocks: MockSet, beat: &str) -> (TraceReport, TraceExit) {
    trace_beat_with_check(input, lute_check::check(input), mocks, beat, None)
}

/// [`trace_beat`] gated on a caller-supplied [`CheckResult`] — the
/// project-aware seam [`trace_with_check`] documents.
pub fn trace_beat_with_check(
    input: &CheckInput,
    result: CheckResult,
    mocks: MockSet,
    beat: &str,
    project_asserts: Option<&BTreeSet<String>>,
) -> (TraceReport, TraceExit) {
    trace_pipeline(
        input,
        result,
        mocks,
        Presentation::Beat(beat),
        project_asserts,
    )
}

/// What [`trace_pipeline`] walks once the document is gated and compiled.
#[derive(Clone, Copy)]
pub(super) enum Presentation<'a> {
    /// The scene's shots, or the quest document's lifecycle.
    Document,
    /// These lore `<entry>` ids, in order, read flags written between them.
    Entries(&'a [&'a str]),
    /// One lore bundle `<beat>`, by local or canonical id.
    Beat(&'a str),
}

pub(super) fn trace_pipeline(
    input: &CheckInput,
    result: CheckResult,
    mocks: MockSet,
    present: Presentation<'_>,
    project_asserts: Option<&BTreeSet<String>>,
) -> (TraceReport, TraceExit) {
    // 1. `check` gate (§4.3): any Error -> Refused, run check first.
    if !result.ok {
        return (
            empty_report(&input.uri, &mocks),
            TraceExit::Refused(result.diagnostics),
        );
    }

    // 2. Re-derive the parsed, CEL-filled document + folded environment
    //    (mirrors `lute_compile::compile`'s own re-derivation after ITS
    //    gate — check's own diagnostics were already reported above).
    let (mut doc, _parse_diags) = lute_syntax::parse(&input.text);
    let _ = lute_check::desugar_document(&mut doc, input);
    let mut arena = lute_cel::CelArena::default();
    let _ = lute_cel::fill_document(&mut arena, &mut doc);
    let (folded, _fd1, _fd2) = lute_check::fold_env(&doc, input);

    // 3. Mock validation (§4.3): any E-TRACE-* -> Refused. `--entry` (dsl
    //    0.19.0 §8) / `--beat` (dsl 0.23.0 §4) first: a non-lore document /
    //    unknown id is refused before any mock is judged against it.
    let mut mock_diags: Vec<Diagnostic> = Vec::new();
    let mut beat_at: Option<(usize, String)> = None;
    match present {
        Presentation::Document => {}
        Presentation::Entries(entries) => {
            for id in entries {
                // dsl 0.26.0 §7 (T3-10): `<document id>.<entry id>` too.
                let id = mock::entry_local_id(&doc, folded.typed.id.as_deref(), id);
                for d in mock::validate_entry(&folded, &doc, id) {
                    // A non-lore document refuses every id with the same line.
                    if !mock_diags.iter().any(|m| m.message == d.message) {
                        mock_diags.push(d);
                    }
                }
            }
        }
        Presentation::Beat(id) => match mock::resolve_beat(&folded, &doc, id) {
            Ok(found) => beat_at = Some(found),
            Err(d) => mock_diags.push(d),
        },
    }
    // dsl 0.25.0 §7: a bridge result field no content reads may go unanswered.
    let content_reads = mock::content_read_paths(&input.text, &folded.def_bodies);
    if mock_diags.is_empty() {
        mock_diags = mock::validate(&mocks, &folded, &doc);
        mock_diags.extend(mock::validate_bridges(
            &mocks,
            &folded,
            &input.snapshot,
            &content_reads,
        ));
    }
    if !mock_diags.is_empty() {
        return (
            empty_report(&input.uri, &mocks),
            TraceExit::Refused(mock_diags),
        );
    }

    // 4. The same lute-compile passes, in the same order, `compile` runs —
    //    the tree the static notes and the AST-only report texts read.
    let cast = lute_check::declared_cast(&input.snapshot, &input.imports, &folded.typed.cast);
    let mut diags = lute_compile::normalize::normalize_document(
        &mut doc,
        &input.components,
        &cast,
        &folded.env.domains,
        &folded.env.state,
        &folded.env.occasion_scopes,
    );
    lute_check::builtin_lowering::canonicalize_builtin_directives(&mut doc, &input.snapshot);
    let table = DefTable {
        bodies: &folded.def_bodies,
        params: &folded.env.def_params,
    };
    diags.extend(lute_compile::expand::expand_document(&mut doc, &table));
    if diags.iter().any(|d| d.severity == Severity::Error) {
        return (empty_report(&input.uri, &mocks), TraceExit::Refused(diags));
    }

    // 5. Compile: the artifact the Machine walks and the map the report is
    //    built through. A refusal of a check-clean document is a compile
    //    bug; it is reported, never walked around.
    let identity = lute_manifest::project::IdentityTemplates::default();
    let (artifact, map) = match lute_compile::compile_mapped(input, result, &identity) {
        Ok(compiled) => compiled,
        Err(diags) => return (empty_report(&input.uri, &mocks), TraceExit::Refused(diags)),
    };
    let art = serde_json::to_value(&artifact).unwrap_or(Json::Null);
    let authored: BTreeMap<String, String> = artifact
        .commands
        .iter()
        .filter_map(|c| c.authored())
        .map(|(addr, text)| (addr.to_string(), text.to_string()))
        .collect();
    // Leftover (a): the traced document as a one-document project, so a
    // presented scene beat, entry or bundle beat is judged by the session's
    // ONE eligibility rule ([`exec::session::judge_beat`]) — `once`,
    // `after`, `spentBy`, `when` — never a trace-side port of it.
    let judged_kind = match present {
        Presentation::Document => {
            folded.doc_kind == lute_check::DocKind::Scene && folded.typed.beat.is_some()
        }
        Presentation::Entries(_) | Presentation::Beat(_) => true,
    };
    // dsl 0.28.0 (T1-25): a quest document's mocked raises are judged by the
    // engine's seam too, over the same one-document project.
    let quest_raises = folded.doc_kind == lute_check::DocKind::Quest && !mocks.occasions.is_empty();
    let judging = (judged_kind || quest_raises)
        .then(|| judging_project(&input.uri, artifact, &input.snapshot.occasions))
        .flatten()
        .map(|mut p| {
            p.chapter_afters
                .extend(lute_check::chapters::derived_after(&doc, &folded.typed));
            p
        });

    let names: BTreeMap<String, String> = cast
        .iter()
        .filter_map(|(id, c)| Some((id.clone(), c.name.clone()?)))
        .collect();
    // The members an unbound `occasion.target` may take: the presented
    // beat's / entries' own kind, else every kind beat's of the document.
    let scopes = lute_check::occasion_bind::occasion_scopes(
        &doc,
        folded.typed.beat.as_ref(),
        &folded.occasions,
        &folded.env.rel_vocab.kinds,
    );
    let named = |id: &str, local: &str| {
        id == local || id.strip_suffix(local).is_some_and(|p| p.ends_with('.'))
    };
    let spans: Vec<Span> = match present {
        Presentation::Document => Vec::new(),
        Presentation::Beat(id) => doc
            .beats
            .iter()
            .filter(|b| named(id, &b.id))
            .map(|b| b.span)
            .collect(),
        Presentation::Entries(ids) => doc
            .entries
            .iter()
            .filter(|e| ids.iter().any(|id| named(id, &e.id)))
            .map(|e| e.span)
            .collect(),
    };
    let mut members: Vec<String> = Vec::new();
    for m in spans
        .iter()
        .filter_map(|s| scopes.members_at(s.byte_start, s.byte_end))
        .flatten()
    {
        if !members.contains(m) {
            members.push(m.clone());
        }
    }
    if members.is_empty() {
        members = scopes.members();
    }
    // A mocked raise for a target (`--occasion landed@fish.cod`) binds
    // `occasion.target` for the presented unit answering it that runs for a
    // kind's members (`target="kind:K"`, `for="kind:K"`) — the member the
    // engine binds presenting it. A seeded `--state occasion.target=…` wins.
    // Keyed by entry id; the scene or `--beat` beat under "".
    let mut raised: BTreeMap<String, String> = BTreeMap::new();
    let mut binding_ons: BTreeSet<String> = BTreeSet::new();
    // A raise for a target outside its occasion's domain is one the engine
    // never makes. A bare member (`gift@ren` for `npc.ren`) stands for the
    // prefixed target.
    let kinds = &folded.env.rel_vocab.kinds;
    let outside: Vec<Diagnostic> = mocks
        .occasions
        .iter()
        .filter_map(|raw| {
            let (on, Some(target)) = lute_runtime::split_occasion(raw) else {
                return None;
            };
            let decl = input.snapshot.occasions.get(on)?;
            let why = lute_check::beats::occasion_target_ok(decl, target, kinds).err()?;
            if !target.contains('.')
                && lute_check::gates::domain_members(decl, kinds)
                    .is_some_and(|ms| ms.iter().any(|m| m == target))
            {
                return None;
            }
            Some(logic_diag(
                mock::E_TRACE_MOCK_TYPE,
                format!("the raise `{raw}` is never made: {why}"),
                mock::synthetic_span(),
            ))
        })
        .collect();
    if !outside.is_empty() {
        return (
            empty_report(&input.uri, &mocks),
            TraceExit::Refused(outside),
        );
    }
    if !mocks
        .state
        .iter()
        .any(|(p, _, _)| p == lute_manifest::semantics::beats::OCCASION_TARGET)
    {
        fn text(v: &Option<(String, Span)>) -> Option<&str> {
            v.as_ref().map(|(s, _)| s.as_str())
        }
        // (key, unit as messages name it, its `on`, its fixed `target`, its
        // extent)
        type Unit<'u> = (String, String, &'u str, Option<&'u str>, (usize, usize));
        let units: Vec<Unit<'_>> = match present {
            Presentation::Document => folded
                .typed
                .beat
                .iter()
                .map(|b| {
                    (
                        String::new(),
                        "the scene".to_string(),
                        b.on.as_str(),
                        b.target.as_deref(),
                        (0, usize::MAX),
                    )
                })
                .collect(),
            Presentation::Beat(_) => beat_at
                .iter()
                .filter_map(|(i, _)| {
                    let b = &doc.beats[*i];
                    let s = (b.span.byte_start, b.span.byte_end);
                    Some((
                        String::new(),
                        format!("the beat `{}`", b.id),
                        text(&b.on)?,
                        text(&b.target),
                        s,
                    ))
                })
                .collect(),
            Presentation::Entries(ids) => ids
                .iter()
                .filter_map(|id| {
                    let id = mock::entry_local_id(&doc, folded.typed.id.as_deref(), id);
                    let e = doc.entries.iter().find(|e| e.id == id)?;
                    let s = (e.span.byte_start, e.span.byte_end);
                    Some((
                        e.id.clone(),
                        format!("the entry `{}`", e.id),
                        text(&e.on)?,
                        text(&e.target),
                        s,
                    ))
                })
                .collect(),
        };
        let mut refused: Vec<Diagnostic> = Vec::new();
        for (key, unit, on, target, (start, end)) in units {
            // Only a unit that runs for members opens a scope.
            let Some(members) = scopes.members_at(start, end) else {
                // A unit answering one fixed target is presented only when
                // its occasion is raised for that target.
                let decl = input
                    .snapshot
                    .occasions
                    .get(on)
                    .filter(|d| d.target.takes_target());
                if let (Some(fixed), Some(decl)) = (target, decl) {
                    let raises: Vec<(&String, &str)> = mocks
                        .occasions
                        .iter()
                        .filter_map(|r| match lute_runtime::split_occasion(r) {
                            (name, Some(t)) if name == on => Some((r, t)),
                            _ => None,
                        })
                        .collect();
                    let bare = lute_manifest::semantics::gates::target_member(decl, fixed);
                    if let (false, Some((raw, other))) = (
                        raises
                            .iter()
                            .any(|(_, t)| *t == fixed || bare.as_deref() == Some(*t)),
                        raises.first(),
                    ) {
                        refused.push(logic_diag(
                            mock::E_TRACE_MOCK_TYPE,
                            format!(
                                "the raise `{raw}` is for `{other}`, but {unit} answers `{on}` \
                                 only for `{fixed}`, so it would not be presented — raise \
                                 `{on}@{fixed}`"
                            ),
                            mock::synthetic_span(),
                        ));
                    }
                }
                continue;
            };
            match raised_member(&mocks, &input.snapshot.occasions, &unit, on, members) {
                Ok(Some(m)) => {
                    raised.insert(key, m);
                    binding_ons.insert(on.to_string());
                }
                Ok(None) => {}
                Err(d) => refused.push(d),
            }
        }
        if !refused.is_empty() {
            return (
                empty_report(&input.uri, &mocks),
                TraceExit::Refused(refused),
            );
        }
    }
    let mut driver = TraceDriver::new(TraceContext {
        art: &art,
        map: &map,
        ast: AstIndex::of(&doc),
        mocks: &mocks,
        folded: &folded,
        snapshot: &input.snapshot,
        content_reads: &content_reads,
        members,
        authored,
        component_files: input
            .components
            .table
            .iter()
            .map(|(name, def)| (name.clone(), def.src.display().to_string()))
            .collect(),
        shots: matches!(present, Presentation::Document)
            && art.get("kind").and_then(Json::as_str).unwrap_or("scene") == "scene",
    });
    let seed = Seed::from(&mocks);
    let mut m = Machine::new(&art, seed.clone(), &mut driver).with_display_names(&names);
    if let Some(member) = raised.get("") {
        m.bind_occasion_target(Some(member));
    }

    // T1-13: a beat scene is only presented when its frontmatter `when`
    // holds. Judged against the mocks BEFORE the walk writes anything — the
    // selector decides at presentation time — and reported, never enforced:
    // tracing a scene is asking to see it.
    let beat_note = match present {
        Presentation::Document => beat_when_note(&folded, &table, &mut m),
        Presentation::Entries(_) | Presentation::Beat(_) => None,
    };
    // dsl 0.26.0 §7 (T1-7): a scene BEAT's own eligibility (it answers an
    // occasion, so a selector presents it), judged at the same moment; under
    // `gate_eligibility` an ineligible scene is not walked. A scene reached by
    // explicit flow has no presentation gate: its `after:` is structural.
    let (scene_eligible, scene_why) = match (present, &judging) {
        (Presentation::Document, Some(p)) if judged_kind => {
            let id = lute_check::meta::canonical_scene_key(&folded.typed)
                .unwrap_or_else(|| input.uri.clone());
            let row = p.index.beats.iter().find(|b| b.kind == BeatKind::Scene);
            let verdict = row.map(|b| judge(p, &mut m, &mocks, b).0.verdict);
            let authored = folded
                .typed
                .beat
                .as_ref()
                .and_then(|b| b.when.as_ref())
                .map(|w| w.raw.as_str());
            let why = match &verdict {
                Some(SessionVerdict::Ineligible(prem)) => Some((
                    premise_text(&mut m, &mocks, prem, BeatKind::Scene, authored),
                    prem.clone(),
                )),
                _ => None,
            };
            (Some((id, verdict.as_ref().and_then(eligible_of))), why)
        }
        _ => (None, None),
    };
    let scene_gated = mocks.gate_eligibility
        && scene_eligible
            .as_ref()
            .is_some_and(|(_, e)| *e == Some(false));
    if let (Some((id, _)), Some((why, prem))) = (&scene_eligible, scene_why) {
        note_premise(&mut m, id, &prem, why);
    }
    // dsl 0.28.0 (T1-25): a quest document's mocked raise is one the
    // engine makes — refused, as a play step raising it is, when the engine
    // would not raise it (its `raisedWhen` false, or the game over), so an
    // expectation of what it judges cannot hold vacuously. Judged over the
    // seeded world, before the walk.
    let mut gate_refusals: Vec<Diagnostic> = Vec::new();
    if let Some(p) = judging.as_ref().filter(|_| quest_raises) {
        for raised in &mocks.occasions {
            let (name, target) = lute_runtime::split_occasion(raised);
            let closed = exec::seam::closed_in(p, &mut m, name, target);
            m.bind_occasion_target(None);
            let why = match closed {
                Some(exec::seam::Closed::Gate { raw, reads }) => format!(
                    "its `raisedWhen: {raw}` is false{} — seed the state that opens it (`state:` \
                     / `--state`), or drop the raise",
                    exec::seam::Closed::reads_text(&reads)
                ),
                Some(exec::seam::Closed::Terminal(t)) => format!(
                    "the game is over (`terminal: {t}` holds), so the engine raises no occasion \
                     — seed state under which `terminal:` does not hold (`state:` / `--state`), \
                     or drop the raise; to judge the raise whose beat ends the game, play it \
                     (an `occasion: {name}` step in a `*.play.yaml`); if the engine raises \
                     `{name}` outside a run too (a title screen, a gallery), declare it \
                     `outsideRun: true`"
                ),
                // Undecided under the mocks: the walk reports what it needs.
                Some(exec::seam::Closed::Unknown(_)) | None => continue,
            };
            // A mocked raise from a test's `occasions:` or a `--occasion`
            // flag: named as raised, located by the caller that knows which.
            gate_refusals.push(logic_diag(
                lute_manifest::semantics::gates::E_OCCASION_GATE,
                format!("the engine would not raise `{raised}` here: {why}"),
                mock::synthetic_span(),
            ));
        }
    }

    // How the walk ended, in play's words: the project's `terminal:` (the
    // artifact carries it) holding in the world it left is `end: terminal`.
    let terminal = art.get("terminal").and_then(exec::Slot::of);
    let terminal = terminal.as_deref();
    // dsl 0.25.0 §1 (LH N16): the seeded world — the mock's `facts:` /
    // `--fact`, the project's seeds, and what the rules derive over them —
    // must not already hold exclusive relations together; the walk would
    // otherwise show content no run can reach, silently.
    let seeded = m.exclusive_violations();
    let (walk, world) = if !seeded.is_empty() {
        for v in &seeded {
            m.driver_mut()
                .steps
                .push(Step::Exclusive { text: v.clone() });
        }
        let walk = Walked::Refused(vec![logic_diag(
            lute_check::fact_check::E_FACT_EXCLUSIVE,
            format!(
                "the seeded facts (the mock's `facts:` / `--fact`, the project's `facts:` seeds, \
                 and what the rules derive from them) hold exclusive relations together before \
                 the walk starts: {} (dsl 0.25.0 §1) — fix the mock (or the `excludes:` \
                 declaration)",
                seeded.join("; ")
            ),
            mock::synthetic_span(),
        )]);
        let world = World::of(&mut m);
        drop(m);
        (walk, world)
    } else if !gate_refusals.is_empty() {
        let world = World::of(&mut m);
        drop(m);
        (Walked::Refused(gate_refusals), world)
    } else {
        match present {
            Presentation::Document => {
                let walk = if scene_gated {
                    Walked::Continue
                } else {
                    run_walk(&mut m)
                };
                let mut world = World::of(&mut m);
                world.terminal = terminal_at(&mut m, terminal);
                drop(m);
                (walk, world)
            }
            // Each entry is presented by its own Machine over the world the
            // previous one left (its carry): the first from the seed, every
            // later one resumed.
            Presentation::Entries(entries) => {
                // Presenting nothing (a lore test judging `eligible:` by id
                // alone) still asserts against the seeded world — the
                // mock's facts, the project's seeds and what the rules
                // derive — never an empty one.
                let mut world: Option<World> = entries.is_empty().then(|| World::of(&mut m));
                drop(m);
                let mut walk = Walked::Continue;
                let mut carry: Option<exec::Carry> = None;
                for id in entries {
                    let id = mock::entry_local_id(&doc, folded.typed.id.as_deref(), id);
                    // `validate_entry` proved every id is declared.
                    let Some(entry) = doc.entries.iter().find(|e| e.id == id) else {
                        continue;
                    };
                    let em = match carry.take() {
                        None => Machine::new(&art, seed.clone(), &mut driver),
                        Some(c) => Machine::resume(&art, carried(&seed), c, &mut driver),
                    };
                    let mut em = em.with_display_names(&names).with_entry(id);
                    // The raise binds this entry's member, never the one a
                    // previous entry ran for.
                    if !raised.is_empty() {
                        em.bind_occasion_target(raised.get(id).map(String::as_str));
                    }
                    walk = present_entry(&mut em, entry, judging.as_ref(), &mocks);
                    let mut now = World::of(&mut em);
                    now.terminal = terminal_at(&mut em, terminal);
                    world = Some(match world.take() {
                        None => now,
                        Some(before) => before.then(now),
                    });
                    carry = Some(em.into_carry().0);
                    if !matches!(walk, Walked::Continue) {
                        break;
                    }
                }
                (walk, world.unwrap_or_default())
            }
            // `resolve_beat` proved the index; normalize/expand never reorder
            // `doc.beats`.
            Presentation::Beat(_) => {
                let mut walk = Walked::Continue;
                if let Some((i, canonical)) = &beat_at {
                    let beat = &doc.beats[*i];
                    let mut bm = m.with_bundle_beat(canonical);
                    walk = present_beat(&mut bm, beat, canonical, judging.as_ref(), &mocks);
                    m = bm;
                }
                let mut world = World::of(&mut m);
                world.terminal = terminal_at(&mut m, terminal);
                drop(m);
                (walk, world)
            }
        }
    };
    finish(Finish {
        input,
        mocks: &mocks,
        folded: &folded,
        doc: &doc,
        project_asserts,
        driver,
        walk,
        beat_note,
        scene_eligible,
        world,
        binding_ons,
    })
}
