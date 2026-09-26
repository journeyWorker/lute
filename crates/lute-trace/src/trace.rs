//! `lute trace` / `lute test` / `trace_source` on the one walker
//! (`docs/design/runtime-unification.md` §3, S4): the document is gated,
//! its mocks validated, then COMPILED ([`lute_compile::compile_mapped`]) and
//! the artifact executed by [`Machine`] with a [`TraceDriver`] — the same
//! walk `lute run` and `lute play` make, with trace's policies (§3.3):
//! scripted `choose:` decisions else an automatic pick, a refusal of a pick
//! that is not offered, the mock's `bridges:` answers, and a halt at an
//! undecidable arm or menu while quest-level unknowns are recorded and the
//! walk goes on. The driver turns the Machine's records into the
//! [`TraceReport`] through the compile-time [`SourceMap`] (spans, authored
//! texts, rendered guards).
//!
//! ## Pipeline (§4.3)
//! 1. The caller's check verdict — any `Error` → [`TraceExit::Refused`].
//! 2. Parse + `lute_cel::fill_document` + `lute_check::fold_env` (the same
//!    re-derivation `lute_compile::compile` performs after its own gate).
//! 3. [`crate::mock::validate`] (and the `--entry` / `--beat` id checks) —
//!    any `E-TRACE-*` → `Refused`.
//! 4. `normalize_document` + `expand_document`, as compile runs them: the
//!    static notes and the AST-only report texts (a line's authored
//!    delivery, a `::next` label) read this tree.
//! 5. `compile_mapped` → artifact + source map; the Machine walks it.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use lute_check::cel_expand::DefTable;
use lute_check::{CheckInput, CheckResult, FoldedEnv};
use lute_compile::source_map::{ArmSource, SourceInfo, SourceMarker};
use lute_compile::SourceMap;
use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_syntax::ast::{Arm, AttrValue, Document, Line, Node};
use lute_syntax::datalog::FactTerm;
use serde_json::Value as Json;

use crate::exec::session::{
    ExecProject, Premise, Verdict as SessionVerdict, World as SessionWorld,
};
use crate::exec::{
    self, guard_premise, BridgeCall, BridgeReply, Driver, Forced, GuardRead, Machine, Menu,
    MenuKind, OnUnknown, Pick, Seed, SiteKind, UnknownSite, Verdict,
};
use crate::mock::{self, BridgeAnswer, MockSet, W_TRACE_MOCK_UNPRODUCIBLE};
use crate::report::{
    self, ComponentBoundary, Coverage, CoverageCount, Decision, GrantCredit, GrantReward, Seeds,
    Step, TraceExit, TraceReport, UnresolvedEntry,
};
use crate::value::{UnresolvedAtom, Value};
use lute_compile::index::BeatKind;

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
enum Presentation<'a> {
    /// The scene's shots, or the quest document's lifecycle.
    Document,
    /// These lore `<entry>` ids, in order, read flags written between them.
    Entries(&'a [&'a str]),
    /// One lore bundle `<beat>`, by local or canonical id.
    Beat(&'a str),
}

fn trace_pipeline(
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
    let judging = judged_kind
        .then(|| judging_project(&input.uri, artifact, &input.snapshot.occasions))
        .flatten()
        .map(|mut p| {
            p.sequence_after =
                lute_check::sequence::derived_afters(&input.defaults, &input.snapshot.occasions);
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
    let mut driver = TraceDriver::new(TraceContext {
        art: &art,
        map: &map,
        ast: AstIndex::of(&doc),
        mocks: &mocks,
        folded: &folded,
        snapshot: &input.snapshot,
        content_reads: &content_reads,
        members,
        shots: matches!(present, Presentation::Document)
            && art.get("kind").and_then(Json::as_str).unwrap_or("scene") == "scene",
    });
    let seed = Seed::from(&mocks);
    let mut m = Machine::new(&art, seed.clone(), &mut driver).with_display_names(&names);

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
    } else {
        match present {
            Presentation::Document => {
                let walk = if scene_gated {
                    Walked::Continue
                } else {
                    run_walk(&mut m)
                };
                let world = World::of(&mut m);
                drop(m);
                (walk, world)
            }
            // Each entry is presented by its own Machine over the world the
            // previous one left (its carry): the first from the seed, every
            // later one resumed.
            Presentation::Entries(entries) => {
                drop(m);
                let mut walk = Walked::Continue;
                let mut world: Option<World> = None;
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
                    walk = present_entry(&mut em, entry, judging.as_ref(), &mocks);
                    let now = World::of(&mut em);
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
                let world = World::of(&mut m);
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
    })
}

/// A seed for a resumed presentation: the carry already holds the seeded
/// state and facts, so only the lifecycle surfaces ride along.
fn carried(seed: &Seed) -> Seed {
    Seed {
        state: Vec::new(),
        facts: Vec::new(),
        ..seed.clone()
    }
}

/// How the walk ended, before the report's exit code.
enum Walked {
    Continue,
    Ended,
    Incomplete,
    Refused(Vec<Diagnostic>),
}

/// Run the Machine and read how it ended.
fn run_walk(m: &mut Machine<&mut TraceDriver<'_>>) -> Walked {
    let result = m.run();
    let refused = m.refused();
    let (incomplete, terminated) = (m.incomplete(), m.terminated());
    match result {
        Err(msg) => Walked::Refused(vec![m.driver_mut().refusal(msg, refused)]),
        Ok(()) if incomplete => Walked::Incomplete,
        Ok(()) if terminated => Walked::Ended,
        Ok(()) => {
            m.driver_mut().finish_shots();
            Walked::Continue
        }
    }
}

/// Present ONE `<entry>` (dsl 0.19.0 §6, `docs/runtime/lore-entries.md`
/// `present()`): `firstRead = !entry.<id>.read`; the body runs in document
/// order with `::set`/`::assert`/`::retract` applied only on a first read
/// and reported [`Step::Skipped`] otherwise (the Machine's rule). Its
/// eligibility — `once` spent by its read flag (`entry.<id>.read` /
/// `.everRead`), then `when` — is the session's rule
/// ([`exec::session::judge_beat`]) over the mocks, SHOWN on the
/// [`Step::Entry`] head and enforced only under
/// [`MockSet::gate_eligibility`] (dsl 0.26.0 §7, T1-7).
fn present_entry(
    m: &mut Machine<&mut TraceDriver<'_>>,
    entry: &lute_syntax::ast::Entry,
    judging: Option<&ExecProject>,
    mocks: &MockSet,
) -> Walked {
    let first_read = !is_true(m, &lute_check::entry_read_path(&entry.id));
    let verdict = judging
        .and_then(|p| Some((p, p.lore_beat(&entry.id)?)))
        .map(|(p, row)| judge(p, m, mocks, &row).0.verdict);
    let eligible = verdict.as_ref().and_then(eligible_of);
    let spent = match &verdict {
        Some(SessionVerdict::Ineligible(Premise::Spent { once, .. })) => Some(
            once.as_ref()
                .map_or_else(String::new, |o| o.as_str().into_owned()),
        ),
        _ => None,
    };
    if let Some(SessionVerdict::Ineligible(prem)) = &verdict {
        let authored = entry.when.as_ref().map(authored_when);
        let why = premise_text(m, mocks, prem, BeatKind::Entry, authored);
        note_premise(m, &entry.id, prem, why);
    }
    if mocks.gate_eligibility && eligible == Some(false) {
        m.driver_mut().steps.push(Step::Entry {
            id: entry.id.clone(),
            first_read,
            eligible,
            spent,
        });
        return Walked::Continue;
    }
    if verdict.is_some() {
        m.driver_mut().head = Some(Head::Judged {
            eligible,
            spent,
            after_unmet: false,
        });
    }
    run_walk(m)
}

/// Present ONE bundle `<beat>` (dsl 0.23.0 §4): a scene-like beat declared
/// in a lore document, its body walked by the Machine with every effect
/// applied. Its eligibility — `after=` (dsl 0.25.0 §3) over the mocked
/// `visited:` and quest states, `spentBy`, `when` — is the session's rule
/// ([`exec::session::judge_beat`]), SHOWN on the [`Step::Beat`] head under
/// the canonical id and enforced only under [`MockSet::gate_eligibility`],
/// as on an entry. An `after=` the mocks leave undecided (a quest another
/// document declares) is reported unresolved.
fn present_beat(
    m: &mut Machine<&mut TraceDriver<'_>>,
    beat: &lute_syntax::ast::BundleBeat,
    canonical: &str,
    judging: Option<&ExecProject>,
    mocks: &MockSet,
) -> Walked {
    let Some((p, row)) = judging.and_then(|p| Some((p, p.lore_beat(canonical)?))) else {
        return run_walk(m);
    };
    let (cand, w) = judge(p, m, mocks, &row);
    if let Some((after, span)) = beat.after.as_ref().filter(|(a, _)| !a.trim().is_empty()) {
        if let Some(f) = lute_check::parse_prereq(after, *span).0 {
            if let Err(atoms) = exec::session::eval_prereq(p, &f, &w) {
                m.driver_mut().record_unresolved(
                    "beat",
                    canonical,
                    *span,
                    prereq_condition(&f),
                    &atoms,
                );
            }
        }
    }
    let eligible = eligible_of(&cand.verdict);
    let after_unmet = matches!(
        cand.verdict,
        SessionVerdict::Ineligible(Premise::After { .. })
    );
    if let SessionVerdict::Ineligible(prem) = &cand.verdict {
        let authored = beat.when.as_ref().map(authored_when);
        let why = premise_text(m, mocks, prem, BeatKind::Bundle, authored);
        note_premise(m, canonical, prem, why);
    }
    if mocks.gate_eligibility && eligible == Some(false) {
        m.driver_mut().steps.push(Step::Beat {
            id: canonical.to_string(),
            eligible,
            after_unmet,
        });
        return Walked::Continue;
    }
    m.driver_mut().head = Some(Head::Judged {
        eligible,
        spent: None,
        after_unmet,
    });
    run_walk(m)
}

/// The traced document as a one-document [`ExecProject`] — what the
/// session's eligibility rule judges a presented beat over. `None` only if
/// the artifact does not assemble (it always does once compiled).
fn judging_project(
    uri: &str,
    artifact: lute_compile::Artifact,
    occasions: &BTreeMap<String, lute_manifest::schema::OccasionDecl>,
) -> Option<ExecProject> {
    let docs = BTreeMap::from([(uri.to_string(), artifact)]);
    ExecProject::assemble(
        &docs,
        occasions.clone(),
        BTreeSet::new(),
        Default::default(),
        BTreeMap::new(),
    )
    .ok()
}

/// Judge `row` by the session's ONE eligibility rule
/// ([`exec::session::judge_beat`]) at this point of the walk: the walk's
/// own Machine evaluates (`when`, `spentBy` over the mocks, three-valued),
/// over a world carrying what the mocks say about the rest of the project
/// — the mocked `visited:` (which, for a scene, is also what spends its
/// `once: user`), the quest states the walk holds, the entry's read flags.
fn judge(
    p: &ExecProject,
    m: &mut Machine<&mut TraceDriver<'_>>,
    mocks: &MockSet,
    row: &lute_compile::index::IndexBeat,
) -> (exec::session::Candidate, SessionWorld) {
    let mut w = SessionWorld {
        visited: mocks.visited.iter().cloned().collect(),
        ..SessionWorld::default()
    };
    if row.kind == BeatKind::Scene {
        w.spent_user = w.visited.clone();
    }
    for flag in [
        lute_check::entry_read_path(&row.id),
        format!("entry.{}.everRead", row.id),
    ] {
        if is_true(m, &flag) {
            w.state.insert(flag, Value::Bool(true));
        }
    }
    let prereq = p
        .artifacts
        .get(&row.document)
        .and_then(|d| exec::session::beat_prereq(d, &row.id));
    for atom in prereq.iter().flat_map(lute_check::prereq::atoms) {
        if let lute_check::prereq::Atom::Completed(q) | lute_check::prereq::Atom::Active(q) = atom {
            if let crate::eval::Read::Value(Value::Str(s)) = m.read(&format!("quest.{q}.state")) {
                w.quests.insert(q, s);
            }
        }
    }
    let member = match m.read(lute_check::beats::OCCASION_TARGET) {
        crate::eval::Read::Value(Value::Str(s)) => Some(s),
        _ => None,
    };
    let cand = exec::session::judge_beat(p, &w, m, row, member.as_deref(), member.as_deref());
    (cand, w)
}

/// A session verdict as the report's tri-state eligibility.
fn eligible_of(v: &SessionVerdict) -> Option<bool> {
    match v {
        SessionVerdict::Eligible => Some(true),
        SessionVerdict::Ineligible(_) => Some(false),
        SessionVerdict::Unknown(_) => None,
    }
}

/// Prerelease N3 / round-5 T3-12: the false premise, named for an author
/// fixing a `*.test.yaml` — what [`TraceReport::premises`] carries. `when`
/// is the authored `when` where the caller has it (a scene's, before `@def`
/// expansion), else the compiled one. `m` is the walk at the judgement.
fn premise_text(
    m: &mut Machine<&mut TraceDriver<'_>>,
    mocks: &MockSet,
    prem: &Premise,
    kind: BeatKind,
    when: Option<&str>,
) -> String {
    use lute_check::prereq::Atom;
    match prem {
        Premise::When { raw } => when_text(m, mocks, raw, when.unwrap_or(raw.as_str())),
        Premise::After {
            raw,
            unmet,
            sequence,
        } => {
            let mocks: Vec<String> = unmet
                .iter()
                .map(|a| match a {
                    Atom::Visited(k) => format!("`visited: [{k}]`"),
                    Atom::Completed(q) => format!("`quests: {{ {q}: complete }}`"),
                    Atom::Active(q) => format!("`quests: {{ {q}: active }}`"),
                })
                .collect();
            let hint = if mocks.is_empty() {
                String::new()
            } else {
                format!(" — mock {}", mocks.join(", "))
            };
            match kind {
                BeatKind::Bundle => format!("its `after=\"{raw}\"` is false{hint}"),
                BeatKind::Scene | BeatKind::Entry if *sequence => format!(
                    "its `after: {raw}` (written by `sequence:` in lute.project.yaml) is \
                     false{hint}"
                ),
                BeatKind::Scene | BeatKind::Entry => format!("its `after: {raw}` is false{hint}"),
            }
        }
        Premise::Spent {
            once: Some(lute_compile::BeatOnce::User),
            ..
        } if kind == BeatKind::Scene => {
            "it is `once: user` and the mocked `visited:` already lists it".to_string()
        }
        Premise::Spent { reason, .. } => format!("it is spent ({reason})"),
        Premise::SpentBy(reason) => format!("its {reason}"),
        // dsl 0.27.0 §4 (HW27-04): the engine would not raise its occasion
        // — what `lute play` refuses with E-OCCASION-GATE.
        Premise::Gate {
            occasion,
            raw,
            reads,
        } => format!(
            "the engine does not raise `{occasion}`: its `raisedWhen: {raw}` is false{}",
            exec::seam::Closed::reads_text(reads)
        ),
        Premise::Terminal { raw, .. } => {
            format!("the game is over (`terminal: {raw}` holds), so the engine raises no occasion")
        }
    }
}

/// A `when` slot as its author wrote it (before `@def` expansion).
fn authored_when(slot: &lute_syntax::ast::CelSlot) -> &str {
    slot.authored.as_deref().unwrap_or(&slot.raw)
}

/// OT-F-10: a false `when` (compiled `raw`, shown as `authored`) named by
/// its false conjunct(s) first, each with what it read — a negated fact
/// the mocks seeded said so — then the whole `when` when it has more than
/// the one conjunct.
fn when_text(
    m: &mut Machine<&mut TraceDriver<'_>>,
    mocks: &MockSet,
    raw: &str,
    authored: &str,
) -> String {
    let seeded = |f: &str| {
        let bare = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
        mocks.facts.iter().any(|s| bare(s) == bare(f))
    };
    let conjuncts = m.false_conjuncts(raw);
    let authored = authored.trim();
    // What one false conjunct read, ` (…)`, or nothing.
    let found = |reads: &[GuardRead]| {
        if reads.is_empty() {
            return String::new();
        }
        let found: Vec<String> = reads
            .iter()
            .map(|r| match r {
                GuardRead::Holds(f) if seeded(f) => format!("{}, seeded by `facts:`", r.found()),
                r => r.found(),
            })
            .collect();
        format!(" ({})", found.join("; "))
    };
    match conjuncts.as_slice() {
        [] => format!("its `when` ({authored}) is false"),
        [(c, reads)] if c == lute_check::templates::unparen(raw) => {
            format!("its `when` ({authored}) is false{}", found(reads))
        }
        _ => {
            let parts: Vec<String> = conjuncts
                .iter()
                .map(|(c, reads)| format!("`{c}` is false{}", found(reads)))
                .collect();
            format!(
                "its `when` is false because {} — the whole `when`: {authored}",
                parts.join(" and ")
            )
        }
    }
}

/// Record the false premise of presented `id` ([`TraceReport::premises`]),
/// and when it is a closed seam — the engine would not raise the beat's
/// occasion — its structured form ([`TraceReport::not_raised`], HW27-04).
fn note_premise(m: &mut Machine<&mut TraceDriver<'_>>, id: &str, prem: &Premise, why: String) {
    let d = m.driver_mut();
    d.premises.insert(id.to_string(), why);
    let nr = match prem {
        Premise::Gate {
            occasion,
            raw,
            reads,
        } => crate::report::NotRaised {
            occasion: occasion.clone(),
            reason: "gate",
            condition: raw.clone(),
            false_reads: reads.iter().map(exec::GuardRead::found).collect(),
        },
        Premise::Terminal { occasion, raw } => crate::report::NotRaised {
            occasion: occasion.clone(),
            reason: "terminal",
            condition: raw.clone(),
            false_reads: Vec::new(),
        },
        _ => return,
    };
    d.not_raised.insert(id.to_string(), nr);
}

fn is_true(m: &Machine<&mut TraceDriver<'_>>, path: &str) -> bool {
    m.read(path) == crate::eval::Read::Value(Value::Bool(true))
}

/// The world a walk left, for the report's final state and facts.
#[derive(Default)]
struct World {
    state: BTreeMap<String, Value>,
    facts: BTreeSet<String>,
    undecided: BTreeSet<String>,
    reserved_reads: BTreeMap<String, bool>,
    derived_reads: BTreeSet<String>,
}

impl World {
    fn of(m: &mut Machine<&mut TraceDriver<'_>>) -> Self {
        World {
            state: m.state().clone(),
            facts: m
                .all_facts()
                .iter()
                .map(|(r, a)| exec::render_fact(r, a))
                .collect(),
            undecided: m.undecided().keys().cloned().collect(),
            reserved_reads: m.reserved_reads(),
            derived_reads: m.derived_reads().clone(),
        }
    }

    /// A later presentation's world over this one: its state and facts
    /// win; what every presentation read accumulates (first read kept).
    fn then(mut self, later: World) -> Self {
        for (p, defaulted) in later.reserved_reads {
            self.reserved_reads.entry(p).or_insert(defaulted);
        }
        self.derived_reads.extend(later.derived_reads);
        World {
            state: later.state,
            facts: later.facts,
            undecided: later.undecided,
            reserved_reads: self.reserved_reads,
            derived_reads: self.derived_reads,
        }
    }
}

struct Finish<'a, 'd> {
    input: &'a CheckInput,
    mocks: &'a MockSet,
    folded: &'a FoldedEnv,
    doc: &'a Document,
    project_asserts: Option<&'a BTreeSet<String>>,
    driver: TraceDriver<'d>,
    walk: Walked,
    beat_note: Option<String>,
    scene_eligible: Option<(String, Option<bool>)>,
    world: World,
}

/// The notes, the exit code and the report.
fn finish(f: Finish<'_, '_>) -> (TraceReport, TraceExit) {
    let Finish {
        input,
        mocks,
        folded,
        doc,
        project_asserts,
        mut driver,
        walk,
        beat_note,
        scene_eligible,
        world,
    } = f;
    let World {
        state,
        facts,
        undecided,
        reserved_reads,
        derived_reads,
    } = world;
    let doc_quest_ids: BTreeSet<&str> = doc.quests.iter().map(|q| q.id.as_str()).collect();
    let mut notes: Vec<String> = beat_note.into_iter().collect();
    // dsl 0.22.0 §6: under `derive: false` the seeds were not loaded and the
    // rules not applied — say so, once per derived relation read.
    if !mocks.derives() {
        notes.extend(seed_fact_notes(mocks, &folded.env.rel_vocab.facts));
        notes.extend(derived_read_notes(&derived_reads));
    }
    let quest_notes = reserved_quest_notes(mocks, &reserved_reads, &doc_quest_ids);
    let foreign_quests: BTreeSet<String> = quest_notes.iter().map(|(id, _)| id.clone()).collect();
    notes.extend(quest_notes.into_iter().map(|(_, note)| note));
    notes.extend(unmatched_event_notes(doc, &mocks.events));
    notes.extend(occasion_notes(doc, &mocks.occasions, &driver.decisions));
    notes.extend(std::mem::take(&mut driver.spent_accepts));
    notes.extend(mock_unproducible_notes(mocks, folded, doc, project_asserts));
    let ended = matches!(walk, Walked::Ended);
    let end_reason = driver.steps.iter().rev().find_map(|s| match s {
        Step::Directive {
            reason: Some(r), ..
        } => Some(r.clone()),
        _ => None,
    });
    // A quest-level unknown (an objective, a `start`, a handler) records an
    // unresolved entry without halting — it is a lifecycle fact the report
    // tables, not a gate the walk stops on — so `Incomplete` is driven by
    // EITHER a halt or a recorded unresolved entry (§4.5 exit 3). `Ended`
    // (dsl 0.8.0 `::end`) is a COMPLETE walk, downgraded only by the same
    // unresolved entries.
    let unresolved_empty = driver.unresolved.is_empty();
    let exit = match walk {
        Walked::Continue | Walked::Ended if unresolved_empty => TraceExit::Complete,
        Walked::Continue | Walked::Ended | Walked::Incomplete => TraceExit::Incomplete,
        Walked::Refused(ds) => TraceExit::Refused(ds),
    };
    let disposition = match &exit {
        TraceExit::Complete if ended => "ended",
        TraceExit::Complete => "complete",
        TraceExit::Incomplete => "incomplete",
        TraceExit::Refused(_) => "refused",
    }
    .to_string();
    let final_state = state
        .iter()
        .map(|(path, v)| {
            (
                path.clone(),
                report::value_text(v).unwrap_or_else(|| "unknown".to_string()),
            )
        })
        .collect();
    let report = TraceReport {
        file: input.uri.clone(),
        seeds: seeds_summary(mocks),
        steps: driver.steps,
        decisions: driver.decisions,
        unresolved: driver.unresolved,
        coverage: Coverage {
            choices: driver.coverage_choices,
            arms: driver.coverage_arms,
        },
        notes,
        disposition,
        end_reason,
        forced_unknown: driver.forced_unknown,
        final_state,
        final_facts: facts,
        final_undecided: undecided,
        foreign_quests,
        scene_eligible,
        premises: driver.premises,
        not_raised: driver.not_raised,
        said: driver.said,
    };
    (report, exit)
}

fn logic_diag(code: &str, message: String, span: Span) -> Diagnostic {
    Diagnostic {
        code: code.to_string(),
        severity: Severity::Error,
        message,
        span,
        layer: Layer::Logic,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    }
}

fn choice_diag(span: Span, id: &str, choice_id: &str, reason: &str) -> Diagnostic {
    logic_diag(
        mock::E_TRACE_CHOICE,
        format!(
            "`--choose {id}={choice_id}` is ineligible at its presentation point: {reason} (dsl 0.4.0 §4.4)"
        ),
        span,
    )
}

// ---------------------------------------------------------------------
// The AST texts the source map does not carry.
// ---------------------------------------------------------------------

/// A span's identity (the byte range).
fn key(span: &Span) -> (usize, usize) {
    (span.byte_start, span.byte_end)
}

/// Per content line its authored delivery, per `::next` its label — keyed by
/// the node's span, which the source map gives every record.
#[derive(Default)]
struct AstIndex {
    deliveries: HashMap<(usize, usize), Option<String>>,
    next_to: HashMap<(usize, usize), String>,
    /// Per `<entry>` id, its `when` slot's span.
    entry_when: HashMap<String, Span>,
    /// Per bundle `<beat>` canonical id suffix (`.<beat id>`), its `when`
    /// slot's span.
    beat_when: Vec<(String, Span)>,
}

impl AstIndex {
    fn of(doc: &Document) -> Self {
        let mut ix = AstIndex::default();
        for shot in &doc.shots {
            ix.nodes(&shot.body);
        }
        for q in &doc.quests {
            ix.nodes(&q.body);
        }
        for e in &doc.entries {
            ix.nodes(&e.body);
            if let Some(w) = &e.when {
                ix.entry_when.insert(e.id.clone(), w.span);
            }
        }
        for b in &doc.beats {
            ix.nodes(&b.body);
            if let Some(w) = &b.when {
                ix.beat_when.push((format!(".{}", b.id), w.span));
            }
        }
        ix
    }

    fn nodes(&mut self, nodes: &[Node]) {
        for node in nodes {
            match node {
                Node::Line(l) => {
                    self.deliveries.insert(key(&l.span), line_delivery(l));
                }
                Node::Directive(d) if d.tag == lute_manifest::core::NEXT_DIRECTIVE => {
                    let to = d
                        .attrs
                        .iter()
                        .find(|a| a.key == "to")
                        .and_then(|a| match &a.value {
                            AttrValue::Str(s) => Some(s.clone()),
                            _ => None,
                        });
                    if let Some(to) = to {
                        self.next_to.insert(key(&d.span), to);
                    }
                }
                Node::Branch(b) => b.choices.iter().for_each(|c| self.nodes(&c.body)),
                Node::Hub(h) => h.choices.iter().for_each(|c| self.nodes(&c.body)),
                Node::Match(m) => {
                    for arm in &m.arms {
                        match arm {
                            Arm::When { body, .. } | Arm::Otherwise { body, .. } => {
                                self.nodes(body)
                            }
                        }
                    }
                }
                Node::On(o) => self.nodes(&o.body),
                Node::Objective(o) => self.nodes(&o.body),
                Node::Directive(_)
                | Node::Set(_)
                | Node::Assert(_)
                | Node::Retract(_)
                | Node::Timeline(_) => {}
            }
        }
    }
}

// ---------------------------------------------------------------------
// The trace driver: trace's policies, and the report built from records.
// ---------------------------------------------------------------------

/// What a [`TraceDriver`] reads.
struct TraceContext<'a> {
    art: &'a Json,
    map: &'a SourceMap,
    ast: AstIndex,
    mocks: &'a MockSet,
    folded: &'a FoldedEnv,
    snapshot: &'a lute_manifest::snapshot::CapabilitySnapshot,
    content_reads: &'a BTreeSet<String>,
    /// The members an unbound `occasion.target` may take (its hint).
    members: Vec<String>,
    /// A scene walk: records open `Shot` heads.
    shots: bool,
}

/// The menu a pick answered, until its record arrives.
struct MenuSeen {
    addr: String,
    /// Every option offered at this presentation point.
    eligible: Vec<String>,
    auto: bool,
    /// The option a scripted pick forced past an undecided guard.
    forced: Option<String>,
}

/// A presentation head the pipeline judged before the walk, by the
/// session's rule: its eligibility, the `once` an earlier read spent (an
/// entry), and whether its `after=` is what is false (a bundle beat).
enum Head {
    Judged {
        eligible: Option<bool>,
        spent: Option<String>,
        after_unmet: bool,
    },
}

/// `lute trace`'s [`Driver`] (design §3.3) and report builder (§3.6).
pub(crate) struct TraceDriver<'a> {
    cx: TraceContext<'a>,
    /// `addr` → the artifact command.
    cmds: HashMap<String, Json>,
    /// Shot number → heading.
    headings: BTreeMap<i64, String>,
    /// Per `<branch>` id, how many decisions of a multi-decision `choose`
    /// list earlier presentations consumed.
    branch_cursor: BTreeMap<String, usize>,
    /// Per plugin directive tag, how many `bridges:` answers earlier calls
    /// consumed.
    bridge_cursor: BTreeMap<String, usize>,
    /// dsl 0.24.0 §5: result slot → `(tag, field, answer shape)` of a plugin
    /// call that found no answer for it — a read of it is hinted as the
    /// missing answer ([`TraceDriver::render_atom`]).
    bridge_unanswered: BTreeMap<String, (String, String, String)>,
    /// The answer the last bridge call consumed (`None`: none left), until
    /// its `plugin` record arrives.
    bridge_answer: Option<Option<BridgeAnswer>>,
    menu: Option<MenuSeen>,
    head: Option<Head>,
    /// The shot whose head was shown last (`0`: none yet).
    shot: i64,
    /// The menu `addr` a pick already reached (its heads and markers shown).
    reached: Option<String>,
    /// The last record was an authored `::next` jump.
    jumped: bool,
    /// The span of the last write, assert or retract record.
    last_write: Option<Span>,
    /// `exclusive` records since the last write.
    exclusive: Vec<String>,
    /// A refusal the driver ruled (`E-TRACE-CHOICE`).
    refused: Option<Diagnostic>,

    steps: Vec<Step>,
    decisions: Vec<Decision>,
    unresolved: Vec<UnresolvedEntry>,
    forced_unknown: Vec<UnresolvedEntry>,
    coverage_choices: BTreeMap<String, CoverageCount>,
    coverage_arms: BTreeMap<String, CoverageCount>,
    said: Vec<String>,
    spent_accepts: Vec<String>,
    /// Round-5 T3-12: presented scene / entry / beat id → the false premise
    /// its eligibility verdict names ([`TraceReport::premises`]).
    premises: BTreeMap<String, String>,
    /// HW27-04: the same ids whose premise is a closed seam
    /// ([`TraceReport::not_raised`]).
    not_raised: BTreeMap<String, crate::report::NotRaised>,
}

impl<'a> TraceDriver<'a> {
    fn new(cx: TraceContext<'a>) -> Self {
        let cmds = cx
            .art
            .get("commands")
            .and_then(Json::as_array)
            .into_iter()
            .flatten()
            .filter_map(|c| Some((c.get("addr")?.as_str()?.to_string(), c.clone())))
            .collect();
        let headings = cx
            .art
            .get("shots")
            .and_then(Json::as_array)
            .into_iter()
            .flatten()
            .filter_map(|s| {
                Some((
                    s.get("shot")?.as_i64()?,
                    s.get("heading")
                        .and_then(Json::as_str)
                        .unwrap_or("")
                        .to_string(),
                ))
            })
            .collect();
        TraceDriver {
            cx,
            cmds,
            headings,
            branch_cursor: BTreeMap::new(),
            bridge_cursor: BTreeMap::new(),
            bridge_unanswered: BTreeMap::new(),
            bridge_answer: None,
            menu: None,
            head: None,
            shot: 0,
            jumped: false,
            reached: None,
            last_write: None,
            exclusive: Vec::new(),
            refused: None,
            steps: Vec::new(),
            decisions: Vec::new(),
            unresolved: Vec::new(),
            forced_unknown: Vec::new(),
            coverage_choices: BTreeMap::new(),
            coverage_arms: BTreeMap::new(),
            said: Vec::new(),
            spent_accepts: Vec::new(),
            premises: BTreeMap::new(),
            not_raised: BTreeMap::new(),
        }
    }

    fn info(&self, addr: &str) -> Option<&SourceInfo> {
        self.cx.map.by_addr.get(addr)
    }

    fn span_at(&self, addr: &str) -> Span {
        self.info(addr)
            .map(|i| i.span)
            .unwrap_or_else(mock::synthetic_span)
    }

    fn arm(&self, addr: &str, i: usize) -> Option<&ArmSource> {
        self.info(addr)?.arms.get(i)
    }

    /// The option index of `option` in the menu record at `addr`.
    fn option_index(&self, addr: &str, option: &str) -> Option<usize> {
        self.cmds
            .get(addr)?
            .get("options")?
            .as_array()?
            .iter()
            .position(|o| o.get("id").and_then(Json::as_str) == Some(option))
    }

    fn option_span(&self, addr: &str, option: &str) -> Span {
        self.option_index(addr, option)
            .and_then(|i| self.arm(addr, i))
            .map(|a| a.span)
            .unwrap_or_else(|| self.span_at(addr))
    }

    fn option_guard(&self, addr: &str, option: &str) -> (Option<String>, Option<String>) {
        match self
            .option_index(addr, option)
            .and_then(|i| self.arm(addr, i))
        {
            Some(a) => (a.guard.clone(), a.authored_guard.clone()),
            None => (None, None),
        }
    }

    /// An `UnresolvedAtom` → its §4.6 "supply it as a mock" hint: a read of a
    /// result slot an unanswered plugin call left unknown (dsl 0.24.0 §5)
    /// names the `bridges:` answer that would decide it; an unbound
    /// `occasion.target` names its members (T1-9).
    fn render_atom(&self, a: &UnresolvedAtom) -> String {
        match a {
            UnresolvedAtom::Path(p) => match self.bridge_unanswered.get(p) {
                Some((tag, field, shape)) => format!(
                    "bridges: {{ {tag}: [ {shape} ] }} (plugin `{tag}` call unanswered; `{p}` \
                     reads its `{field}` result)"
                ),
                None if p == lute_check::beats::OCCASION_TARGET && !self.cx.members.is_empty() => {
                    format!("--state {p}=<{}>", self.cx.members.join("|"))
                }
                None => render_atom(a),
            },
            _ => render_atom(a),
        }
    }

    /// The mock that changes one read of a guard that decided false
    /// (round-5 T3-12), as a `lute trace` flag where one exists — the
    /// unresolved-atom hint ([`Self::render_atom`]) — else the `--mock`
    /// file key. A single-file trace knows no earlier scene: a
    /// `visited(…)` or quest-state read is only what the mock says.
    fn guard_hint(&self, r: &GuardRead) -> String {
        const EARLIER: &str =
            "in a `--mock` file; a single-file trace does not know what earlier scenes did";
        match r {
            GuardRead::Path(p, _) if exec::session::quest_state_id(p).is_some() => {
                format!("{} {EARLIER}", r.yaml_mock())
            }
            GuardRead::Path(p, _) => {
                format!("`{}`", self.render_atom(&UnresolvedAtom::Path(p.clone())))
            }
            GuardRead::Fact(f) | GuardRead::Derived { fact: f, .. } => {
                format!("`{}`", render_atom(&UnresolvedAtom::Fact(f.clone())))
            }
            GuardRead::Visited(_) | GuardRead::Seen(_) => format!("{} {EARLIER}", r.yaml_mock()),
            GuardRead::Holds(_) => r.yaml_mock(),
        }
    }

    /// §3.2 de-duplication: the unresolved set reports a byte-identical
    /// entry (same construct, id, span, expression) once, not once per
    /// re-evaluation pass.
    fn record_unresolved(
        &mut self,
        construct: &str,
        id: &str,
        span: Span,
        expression: String,
        atoms: &[UnresolvedAtom],
    ) {
        let dup = self.unresolved.iter().any(|u| {
            u.construct == construct && u.id == id && u.span == span && u.expression == expression
        });
        if dup {
            return;
        }
        // One hint per mock: an expression reading `occasion.target` twice
        // (`holds(owned(occasion.target)) && user.bond[occasion.target]`)
        // records the atom twice.
        let mut rendered: Vec<String> = Vec::new();
        for hint in atoms.iter().map(|a| self.render_atom(a)) {
            if !rendered.contains(&hint) {
                rendered.push(hint);
            }
        }
        self.unresolved.push(UnresolvedEntry {
            construct: construct.to_string(),
            id: id.to_string(),
            span,
            expression,
            atoms: rendered,
        });
    }

    fn push_decision(&mut self, d: Decision) {
        self.decisions.push(d.clone());
        self.steps.push(Step::Decision(d));
    }

    /// The walk's refusal: the driver's own ruling, else the exclusivity
    /// the last write broke, else the Machine's message.
    fn refusal(&mut self, msg: String, refused: bool) -> Diagnostic {
        if let Some(d) = self.refused.take() {
            return d;
        }
        if refused && !self.exclusive.is_empty() {
            return logic_diag(
                lute_check::fact_check::E_FACT_EXCLUSIVE,
                format!(
                    "this write makes exclusive relations hold together: {} (dsl 0.25.0 §1)",
                    self.exclusive.join("; ")
                ),
                self.last_write.unwrap_or_else(mock::synthetic_span),
            );
        }
        logic_diag("E-COMPILE-INTERNAL", msg, mock::synthetic_span())
    }

    // -- shots ------------------------------------------------------------

    /// A record of addressing unit `unit` is about to be reported: open the
    /// heads of the shots the walk entered since the last one (every shot
    /// in between on a straight walk, only the landing shot after a
    /// `::next`), with the trailing source-only steps of the shots it left.
    fn enter_unit(&mut self, unit: i64) {
        if !self.cx.shots || unit <= self.shot {
            return;
        }
        if self.jumped {
            self.shot_head(unit);
        } else {
            let from = self.shot.max(0);
            if from > 0 {
                self.trailing(from);
            }
            for n in from + 1..=unit {
                self.shot_head(n);
                if n < unit {
                    self.trailing(n);
                }
            }
        }
        self.shot = unit;
    }

    fn shot_head(&mut self, n: i64) {
        let heading = self.headings.get(&n).cloned().unwrap_or_default();
        self.steps.push(Step::Shot { number: n, heading });
    }

    fn trailing(&mut self, unit: i64) {
        let markers = self.cx.map.trailing.get(&unit).cloned().unwrap_or_default();
        for mk in &markers {
            self.marker(mk);
        }
    }

    /// A scene walk that ran to its end: the rest of the current shot and
    /// every later (empty) shot.
    fn finish_shots(&mut self) {
        if !self.cx.shots {
            return;
        }
        let last = self.headings.keys().copied().max().unwrap_or(0);
        if self.shot > 0 {
            self.trailing(self.shot);
        }
        for n in self.shot + 1..=last {
            self.shot_head(n);
            self.trailing(n);
        }
        self.shot = last;
    }

    fn marker(&mut self, mk: &SourceMarker) {
        let component_boundary = mk.component.map(|c| match c {
            lute_compile::source_map::ComponentBoundary::Begin => ComponentBoundary::Begin,
            lute_compile::source_map::ComponentBoundary::End => ComponentBoundary::End,
        });
        self.steps.push(Step::Directive {
            tag: mk.tag.clone(),
            component_boundary,
            exit: mk.tag == lute_manifest::core::CLEAR_DIRECTIVE,
            reason: None,
        });
    }

    // -- records ----------------------------------------------------------

    /// The walk reached the record at `addr`: open the shot heads it
    /// entered and show the source-only steps that precede it.
    fn reach(&mut self, addr: &str) {
        if let Some(unit) = addr.split('-').next().and_then(|u| u.parse::<i64>().ok()) {
            self.enter_unit(unit);
        }
        let before = self
            .info(addr)
            .map(|i| i.before.clone())
            .unwrap_or_default();
        for mk in &before {
            self.marker(mk);
        }
    }

    /// [`Self::reach`] ahead of the record: a pick or a halt at `addr`
    /// happens before (or instead of) its record.
    fn reach_once(&mut self, addr: &str) {
        if self.reached.as_deref() != Some(addr) {
            self.reach(addr);
            self.reached = Some(addr.to_string());
        }
    }

    fn record(&mut self, rec: Json) {
        let kind = rec
            .get("kind")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string();
        let addr = rec
            .get("addr")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string();
        if kind == "exclusive" {
            let text = rec
                .get("text")
                .and_then(Json::as_str)
                .unwrap_or("")
                .to_string();
            self.exclusive.push(text.clone());
            self.steps.push(Step::Exclusive { text });
            return;
        }
        if kind == "grant" {
            self.grant(&rec);
            return;
        }
        if addr.is_empty() {
            // `quest` / `objective` / `occasion` records: the observations
            // carry what trace reports of them.
            return;
        }
        let info = self.info(&addr).cloned();
        if self.reached.take().as_deref() != Some(addr.as_str()) {
            self.reach(&addr);
        }
        self.jumped = false;
        if info.as_ref().is_some_and(|i| i.injected) {
            return;
        }
        let str_of = |k: &str| rec.get(k).and_then(Json::as_str).unwrap_or("").to_string();
        match kind.as_str() {
            "line" => {
                let span = info.as_ref().map(|i| i.span);
                let delivery = span
                    .and_then(|s| self.cx.ast.deliveries.get(&key(&s)).cloned())
                    .flatten();
                self.said.push(exec::said_line(&rec, self.cmds.get(&addr)));
                self.steps.push(Step::Line {
                    speaker: str_of("speaker"),
                    text: str_of("text"),
                    delivery,
                });
            }
            "set" => {
                self.last_write = info.as_ref().map(|i| i.span);
                self.exclusive.clear();
                self.steps.push(Step::Set {
                    path: str_of("path"),
                    value: json_text(rec.get("value")),
                    sugar: info.as_ref().is_some_and(|i| i.sugar),
                });
            }
            "assert" => {
                self.last_write = info.as_ref().map(|i| i.span);
                self.exclusive.clear();
                self.steps.push(Step::Assert {
                    text: str_of("fact"),
                });
            }
            "retract" => {
                self.last_write = info.as_ref().map(|i| i.span);
                self.exclusive.clear();
                self.steps.push(Step::Retract {
                    text: str_of("pattern"),
                });
            }
            "skipped" => {
                let text = info
                    .as_ref()
                    .and_then(|i| i.write_text.clone())
                    .unwrap_or_else(|| {
                        ["path", "fact", "pattern"]
                            .iter()
                            .find_map(|k| rec.get(*k).and_then(Json::as_str))
                            .unwrap_or("")
                            .to_string()
                    });
                self.steps.push(Step::Skipped {
                    effect: str_of("effect"),
                    text,
                });
            }
            "jump" => {
                if info.as_ref().is_some_and(|i| i.authored_jump) {
                    let to = info
                        .as_ref()
                        .and_then(|i| self.cx.ast.next_to.get(&key(&i.span)).cloned())
                        .unwrap_or_default();
                    self.steps.push(Step::Jump { to });
                    self.jumped = true;
                }
            }
            "choice" | "hub" => self.menu_record(&rec, &kind, &addr),
            "match" => self.match_record(&rec, &addr),
            "end" => self.steps.push(Step::Directive {
                tag: lute_manifest::core::END_DIRECTIVE.to_string(),
                component_boundary: None,
                exit: false,
                reason: rec.get("reason").and_then(Json::as_str).map(str::to_string),
            }),
            "plugin" => {
                let tag = str_of("tag");
                self.steps.push(Step::Directive {
                    tag: tag.clone(),
                    component_boundary: None,
                    exit: false,
                    reason: None,
                });
                if let Some(answer) = self.bridge_answer.take() {
                    self.steps.push(Step::Bridge {
                        tag,
                        answered: answer,
                    });
                }
            }
            "accept" => self.steps.push(Step::Accept {
                quest: str_of("quest"),
                next_run: rec.get("at").and_then(Json::as_str) == Some("nextRun"),
            }),
            "entry" => {
                let (eligible, spent) = match self.head.take() {
                    Some(Head::Judged {
                        eligible, spent, ..
                    }) => (eligible, spent),
                    None => (rec.get("eligible").and_then(Json::as_bool), None),
                };
                self.steps.push(Step::Entry {
                    id: str_of("id"),
                    first_read: rec.get("firstRead").and_then(Json::as_bool).unwrap_or(true),
                    eligible,
                    spent,
                });
            }
            "beat" => {
                let (eligible, after_unmet) = match self.head.take() {
                    Some(Head::Judged {
                        eligible,
                        after_unmet,
                        ..
                    }) => (eligible, after_unmet),
                    None => (rec.get("eligible").and_then(Json::as_bool), false),
                };
                self.steps.push(Step::Beat {
                    id: str_of("id"),
                    eligible,
                    after_unmet,
                });
            }
            "barrier" | "occasion" => {}
            _ => {
                // A staging record: the directive it was lowered from.
                let tag = info
                    .as_ref()
                    .and_then(|i| i.directive.clone())
                    .unwrap_or(kind.clone());
                let exit = tag == lute_manifest::core::CLEAR_DIRECTIVE
                    || tag == "auto" && self.auto_exits(&addr);
                self.steps.push(Step::Directive {
                    tag,
                    component_boundary: None,
                    exit,
                    reason: None,
                });
            }
        }
    }

    /// #32 / T2.5: an `::auto` whose `action=` names a declared exit member
    /// ENDS a presence — `Domain.exits`, the list `lute-check::inject` and
    /// `lute-compile::lower` read.
    fn auto_exits(&self, addr: &str) -> bool {
        let action = self
            .cmds
            .get(addr)
            .and_then(|c| c.get("action"))
            .and_then(Json::as_str);
        action.is_some_and(|a| {
            self.cx
                .folded
                .domains
                .get("action")
                .is_some_and(|d| d.exits.iter().any(|e| e == a))
        })
    }

    fn menu_record(&mut self, rec: &Json, kind: &str, addr: &str) {
        let seen = self.menu.take();
        let Some(chose) = rec.get("chose").and_then(Json::as_str) else {
            return;
        };
        let (construct, id) = if kind == "choice" {
            ("branch", rec.get("branch"))
        } else {
            ("hub", rec.get("hub"))
        };
        let id = id.and_then(Json::as_str).unwrap_or("").to_string();
        let (guard, authored_guard) = self.option_guard(addr, chose);
        let total = self
            .cmds
            .get(addr)
            .and_then(|c| c.get("options"))
            .and_then(Json::as_array)
            .map_or(0, Vec::len);
        let seen = seen.filter(|s| s.addr == addr);
        self.push_decision(Decision {
            construct: construct.to_string(),
            id: id.clone(),
            span: self.option_span(addr, chose),
            outcome: chose.to_string(),
            guard,
            forced: seen.as_ref().and_then(|s| s.forced.as_deref()) == Some(chose),
            auto: seen.as_ref().is_some_and(|s| s.auto),
            eligible: seen.map(|s| s.eligible).unwrap_or_default(),
            authored_id: None,
            authored_guard,
        });
        self.coverage_choices
            .entry(id.clone())
            .and_modify(|c| c.visited += 1)
            .or_insert(CoverageCount {
                visited: 1,
                total,
                label: id,
                authored_label: None,
            });
    }

    fn match_record(&mut self, rec: &Json, addr: &str) {
        let subject = self.subject(addr);
        let info = self.info(addr).cloned();
        let span = self.span_at(addr);
        let total = info.as_ref().map_or(0, |i| i.arms.len());
        let authored_id = info.as_ref().and_then(|i| i.authored_id.clone());
        let result = rec.get("result").and_then(Json::as_str).unwrap_or("");
        let (outcome, arm) = match result.strip_prefix("arm ") {
            Some(n) => {
                let i = n.parse::<usize>().unwrap_or(1).saturating_sub(1);
                (
                    result.to_string(),
                    info.as_ref().and_then(|x| x.arms.get(i).cloned()),
                )
            }
            None if result == "otherwise" => ("otherwise".to_string(), None),
            None => ("no arm".to_string(), None),
        };
        let visited = usize::from(outcome != "no arm");
        self.push_decision(Decision {
            construct: "match".to_string(),
            id: subject.clone(),
            span,
            outcome,
            guard: arm.as_ref().and_then(|a| a.guard.clone()),
            forced: false,
            auto: false,
            eligible: Vec::new(),
            authored_id: authored_id.clone(),
            authored_guard: arm.and_then(|a| a.authored_guard),
        });
        let count = CoverageCount {
            visited,
            total,
            label: subject,
            authored_label: authored_id,
        };
        let site = report::site_key(&span);
        if visited == 1 {
            self.coverage_arms.insert(site, count);
        } else {
            self.coverage_arms.entry(site).or_insert(count);
        }
    }

    /// A `match` record's subject text.
    fn subject(&self, addr: &str) -> String {
        self.cmds
            .get(addr)
            .and_then(|c| c.get("subject"))
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string()
    }

    fn grant(&mut self, rec: &Json) {
        let reward = rec.get("reward").cloned().unwrap_or(Json::Null);
        let str_of = |v: &Json, k: &str| v.get(k).and_then(Json::as_str).map(str::to_string);
        self.steps.push(Step::Grant {
            quest: str_of(rec, "quest").unwrap_or_default(),
            objective: str_of(rec, "objective"),
            reward: GrantReward {
                kind: str_of(&reward, "kind").unwrap_or_default(),
                target: str_of(&reward, "target"),
                amount: reward.get("amount").and_then(Json::as_i64),
                amount_min: reward.get("amountMin").and_then(Json::as_i64),
                amount_max: reward.get("amountMax").and_then(Json::as_i64),
            },
            on_failed: rec.get("onFailed").and_then(Json::as_bool) == Some(true),
            credited: rec.get("credited").map(|c| GrantCredit {
                path: str_of(c, "path").unwrap_or_default(),
                value: json_text(c.get("value")),
            }),
        });
    }

    // -- observations -------------------------------------------------------

    fn observation(&mut self, rec: Json) {
        let str_of = |k: &str| rec.get(k).and_then(Json::as_str).unwrap_or("").to_string();
        let guard = rec.get("guard").and_then(Json::as_str).map(str::to_string);
        let outcome = str_of("outcome");
        let quest = str_of("quest");
        let quest_src = self.cx.map.quests.get(&quest);
        match rec.get("kind").and_then(Json::as_str) {
            Some("quest") => {
                let span = quest_src
                    .map(|q| q.span)
                    .unwrap_or_else(mock::synthetic_span);
                if matches!(outcome.as_str(), "complete" | "failed") {
                    // §3.2: a terminal quest's objectives "can no longer
                    // affect any outcome" — their unresolved entries go.
                    let spans: Vec<Span> = quest_src
                        .map(|q| q.objectives.values().map(|o| o.span).collect())
                        .unwrap_or_default();
                    self.unresolved
                        .retain(|u| !(u.construct == "objective" && spans.contains(&u.span)));
                }
                let forced = rec.get("forced").and_then(Json::as_bool) == Some(true);
                self.push_decision(bare_decision("quest", &quest, span, outcome, guard, forced));
            }
            Some("objective") => {
                let objective = str_of("objective");
                let span = quest_src
                    .and_then(|q| q.objectives.get(&objective))
                    .map(|o| o.span)
                    .unwrap_or_else(mock::synthetic_span);
                self.push_decision(bare_decision(
                    "objective",
                    &objective,
                    span,
                    outcome,
                    guard,
                    false,
                ));
            }
            Some("on") => {
                let span = quest_src
                    .and_then(|q| q.handlers.get(&str_of("addr")))
                    .copied()
                    .unwrap_or_else(mock::synthetic_span);
                let event = str_of("event");
                self.push_decision(bare_decision("on", &event, span, outcome, guard, false));
            }
            Some("acceptSpent") => self.spent_accepts.push(format!(
                "{NOTE_ACCEPT_SPENT} `{quest}` spent: its parent quest `{}` is {} — an \
                 `activate=\"accept\"` child activates only while its parent is active",
                str_of("parent"),
                str_of("why"),
            )),
            _ => {}
        }
    }
}

fn bare_decision(
    construct: &str,
    id: &str,
    span: Span,
    outcome: String,
    guard: Option<String>,
    forced: bool,
) -> Decision {
    Decision {
        construct: construct.to_string(),
        id: id.to_string(),
        span,
        outcome,
        guard: guard.filter(|g| !g.is_empty()),
        forced,
        auto: false,
        eligible: Vec::new(),
        authored_id: None,
        authored_guard: None,
    }
}

/// A record value as display text (`unknown` for an undecided one).
fn json_text(v: Option<&Json>) -> String {
    match v {
        Some(Json::String(s)) => s.clone(),
        Some(Json::Bool(b)) => b.to_string(),
        Some(Json::Number(n)) => n
            .as_f64()
            .map(report::format_num)
            .unwrap_or_else(|| n.to_string()),
        _ => "unknown".to_string(),
    }
}

impl Driver for TraceDriver<'_> {
    /// `choose:` (a mock's, `--choose`) with the runner's cursor rule, else
    /// an automatic pick: a branch takes its first open option; a hub makes
    /// one document-order pass over its open non-`exit` options, then takes
    /// the first open `exit`. A hub's scripted list is its whole visit: once
    /// consumed, the hub converges.
    fn choose(&mut self, menu: &Menu<'_>) -> Pick {
        self.reach_once(menu.addr);
        let eligible = menu
            .options
            .iter()
            .filter(|o| o.verdict == Verdict::Open)
            .map(|o| o.id.clone())
            .collect();
        let list = self.cx.mocks.choose.get(menu.id).map(Vec::as_slice);
        let pick = match (menu.construct, list) {
            (MenuKind::Branch, None | Some([])) => Pick::AutoFirst,
            (MenuKind::Branch, Some([only])) => Pick::Option(only.clone()),
            (MenuKind::Branch, Some(list)) => {
                let used = self.branch_cursor.entry(menu.id.to_string()).or_insert(0);
                match list.get(*used) {
                    Some(next) => {
                        *used += 1;
                        Pick::Option(next.clone())
                    }
                    None => {
                        let expression = format!(
                            "choose list exhausted — {} decision(s) scripted, presented again",
                            list.len()
                        );
                        let span = self.span_at(menu.addr);
                        self.record_unresolved("branch", menu.id, span, expression, &[]);
                        Pick::Unscripted {
                            scripted: list.len(),
                        }
                    }
                }
            }
            (MenuKind::Hub, None) => Pick::HubAutoPass,
            (MenuKind::Hub, Some(list)) => match list.get(menu.presentation) {
                Some(next) => Pick::Option(next.clone()),
                None => Pick::Leave,
            },
        };
        if let Pick::Option(id) = &pick {
            if !menu.options.iter().any(|o| &o.id == id) {
                let what = match menu.construct {
                    MenuKind::Branch => "names no choice in this branch",
                    MenuKind::Hub => "names no choice in this hub",
                };
                let span = self.span_at(menu.addr);
                self.refused = Some(choice_diag(span, menu.id, id, what));
            }
        }
        self.menu = Some(MenuSeen {
            addr: menu.addr.to_string(),
            eligible,
            auto: matches!(pick, Pick::AutoFirst | Pick::HubAutoPass),
            forced: None,
        });
        pick
    }

    /// A scripted pick past an undecided guard is taken and reported as
    /// forced (§4.4); past a guard decided false — or a spent `once` hub
    /// option — it is refused (`E-TRACE-CHOICE`).
    fn forced(&mut self, menu: &Menu<'_>, option: &str, verdict: &Verdict) -> Forced {
        match verdict {
            Verdict::Open => Forced::Take,
            Verdict::Unknown(atoms) => {
                let (guard, _) = self.option_guard(menu.addr, option);
                let construct = match menu.construct {
                    MenuKind::Branch => "branch",
                    MenuKind::Hub => "hub",
                };
                let atoms = atoms.iter().map(|a| self.render_atom(a)).collect();
                self.forced_unknown.push(UnresolvedEntry {
                    construct: construct.to_string(),
                    id: format!("{} -> {option}", menu.id),
                    span: self.option_span(menu.addr, option),
                    expression: guard.unwrap_or_default(),
                    atoms,
                });
                if let Some(seen) = &mut self.menu {
                    seen.forced = Some(option.to_string());
                }
                Forced::Take
            }
            Verdict::Spent => {
                let reason = format!(
                    "it is `once` and already taken in this visit of hub `{}`",
                    menu.id
                );
                let span = self.option_span(menu.addr, option);
                self.refused = Some(choice_diag(span, menu.id, option, &reason));
                Forced::Refuse
            }
            Verdict::Closed(reads) => {
                let (guard, authored) = self.option_guard(menu.addr, option);
                let guard = authored.or(guard).unwrap_or_default();
                let premise = guard_premise(reads, |r| self.guard_hint(r));
                let reason = if premise.is_empty() {
                    format!("its guard `{}` decided false", guard.trim())
                } else {
                    format!("its guard `{}` decided false: {premise}", guard.trim())
                };
                let span = self.option_span(menu.addr, option);
                self.refused = Some(choice_diag(span, menu.id, option, &reason));
                Forced::Refuse
            }
        }
    }

    /// The mock's `bridges:` answers, one per call of the tag, in order. A
    /// result field left unanswered reads UNKNOWN, and a read of it is hinted
    /// as the missing answer: every field of the tag content reads, typed.
    fn bridge(&mut self, call: &BridgeCall<'_>) -> BridgeReply {
        let used = self.bridge_cursor.entry(call.tag.to_string()).or_insert(0);
        let answer = self
            .cx
            .mocks
            .bridges
            .get(call.tag)
            .and_then(|list| list.get(*used))
            .cloned();
        if answer.is_some() {
            *used += 1;
        }
        let shape = self
            .cx
            .snapshot
            .directives
            .get(call.tag)
            .map(|decl| {
                let writes = mock::bridge_result_writes(decl);
                // A read the textual scan cannot see (a component body) still
                // halts on UNKNOWN; its hint then names every field.
                let read = mock::bridge_fields_read(&writes, self.cx.content_reads);
                let decls = &self.cx.folded.env.state.decls;
                mock::bridge_answer_shape(
                    call.reads
                        .iter()
                        .filter(|(f, _)| read.is_empty() || read.contains(f.as_str()))
                        .map(|(f, p)| (f.as_str(), decls.get(p).map(|d| &d.ty))),
                )
            })
            .unwrap_or_default();
        for (field, path) in call.reads {
            let answered = answer
                .as_ref()
                .is_some_and(|a| a.iter().any(|(f, _)| f == field));
            if answered {
                self.bridge_unanswered.remove(path);
            } else {
                self.bridge_unanswered.insert(
                    path.clone(),
                    (call.tag.to_string(), field.clone(), shape.clone()),
                );
            }
        }
        self.bridge_answer = Some(answer.clone());
        match answer {
            Some(a) => BridgeReply::Answer(a),
            None => BridgeReply::Unanswered,
        }
    }

    /// Trace halts where control flow needs the value (an arm, a menu with
    /// nothing open, an unbound `occasion.target`) and records every other
    /// unknown that decides something — a quest's `start`, an objective, a
    /// handler, a presentation head — without halting: the walk goes on
    /// and the exit is incomplete.
    fn unknown(&mut self, site: &UnknownSite<'_>) -> OnUnknown {
        if matches!(
            site.kind,
            SiteKind::Arm
                | SiteKind::OccasionTarget
                | SiteKind::BranchAllUnknown
                | SiteKind::HubAllUnknown
                | SiteKind::Guard
        ) && self.cx.map.by_addr.contains_key(site.addr)
        {
            self.reach_once(site.addr);
        }
        let raw = site.raw.trim().to_string();
        match site.kind {
            SiteKind::Arm | SiteKind::OccasionTarget => {
                let span = self.span_at(site.addr);
                let expr = site
                    .arm
                    .and_then(|i| self.arm(site.addr, i))
                    .and_then(|a| a.guard.clone())
                    .unwrap_or_default();
                self.record_unresolved("match", site.id, span, expr, site.atoms);
                let info = self.info(site.addr);
                let count = CoverageCount {
                    visited: 0,
                    total: info.map_or(0, |i| i.arms.len()),
                    label: site.id.to_string(),
                    authored_label: info.and_then(|i| i.authored_id.clone()),
                };
                self.coverage_arms
                    .entry(report::site_key(&span))
                    .or_insert(count);
                OnUnknown::Halt
            }
            SiteKind::BranchAllUnknown | SiteKind::HubAllUnknown => {
                let (construct, expression) = if site.kind == SiteKind::BranchAllUnknown {
                    ("branch", "eligibility")
                } else {
                    ("hub", "exit eligibility")
                };
                let span = self.span_at(site.addr);
                self.record_unresolved(
                    construct,
                    site.id,
                    span,
                    expression.to_string(),
                    site.atoms,
                );
                let total = self
                    .cmds
                    .get(site.addr)
                    .and_then(|c| c.get("options"))
                    .and_then(Json::as_array)
                    .map_or(0, Vec::len);
                self.coverage_choices
                    .entry(site.id.to_string())
                    .or_insert(CoverageCount {
                        visited: 0,
                        total,
                        label: site.id.to_string(),
                        authored_label: None,
                    });
                OnUnknown::Halt
            }
            SiteKind::Guard => OnUnknown::Halt,
            SiteKind::EntryWhen => {
                if !matches!(self.head, Some(Head::Judged { spent: Some(_), .. })) {
                    let span = self
                        .cx
                        .ast
                        .entry_when
                        .get(site.id)
                        .copied()
                        .unwrap_or_else(|| self.span_at(site.addr));
                    self.record_unresolved("entry", site.id, span, raw, site.atoms);
                }
                OnUnknown::Continue
            }
            SiteKind::BeatWhen => {
                let span = self
                    .cx
                    .ast
                    .beat_when
                    .iter()
                    .find(|(suffix, _)| site.id.ends_with(suffix.as_str()))
                    .map(|(_, s)| *s)
                    .unwrap_or_else(|| self.span_at(site.addr));
                self.record_unresolved("beat", site.id, span, raw, site.atoms);
                OnUnknown::Continue
            }
            SiteKind::QuestStart => {
                let span = self
                    .cx
                    .map
                    .quests
                    .get(site.id)
                    .map(|q| q.span)
                    .unwrap_or_else(mock::synthetic_span);
                self.record_unresolved("quest", site.id, span, raw, site.atoms);
                OnUnknown::Continue
            }
            SiteKind::ObjectiveDone | SiteKind::ObjectiveBy | SiteKind::ObjectiveUntil => {
                let span = site
                    .quest
                    .and_then(|q| self.cx.map.quests.get(q))
                    .and_then(|q| q.objectives.get(site.id))
                    .map(|o| o.span)
                    .unwrap_or_else(mock::synthetic_span);
                self.record_unresolved("objective", site.id, span, raw, site.atoms);
                OnUnknown::Continue
            }
            SiteKind::Handler => {
                let span = site
                    .quest
                    .and_then(|q| self.cx.map.quests.get(q))
                    .and_then(|q| q.handlers.get(site.addr))
                    .copied()
                    .unwrap_or_else(mock::synthetic_span);
                self.record_unresolved("on", site.id, span, raw, site.atoms);
                OnUnknown::Continue
            }
            // Trace reports none of these: an undecided `fail` does not fail
            // the quest, an undecided reward is not granted, an undecided
            // write or bridge result is written unknown and hinted where read.
            SiteKind::QuestFail
            | SiteKind::Reward
            | SiteKind::SetValue
            | SiteKind::BridgeResult => OnUnknown::Continue,
        }
    }

    fn emit(&mut self, rec: Json) {
        self.record(rec);
    }

    fn observe(&mut self, rec: Json) {
        if rec.get("kind").and_then(Json::as_str) == Some("jump") {
            self.record(rec);
        } else {
            self.observation(rec);
        }
    }
}

// ---------------------------------------------------------------------
// Texts, notes and pre-walk judgments over the normalized document.
// ---------------------------------------------------------------------

/// One `UnresolvedAtom` -> its §4.6 "supply it as a mock" hint text.
fn render_atom(a: &UnresolvedAtom) -> String {
    match a {
        UnresolvedAtom::Path(p) => format!("--state {p}=<value>"),
        UnresolvedAtom::Fact(f) | UnresolvedAtom::DerivedFact(f) => format!("--fact \"{f}\""),
        UnresolvedAtom::Time => "no mock surface for narrative time".to_string(),
    }
}

fn fact_term_text(t: &FactTerm) -> String {
    match t {
        FactTerm::Ident(s) => s.clone(),
        FactTerm::Bool(b) => b.to_string(),
        FactTerm::Wildcard => "_".to_string(),
        FactTerm::Param(p) => format!("@{p}"),
    }
}

fn fmt_fact(rel: &str, args: &[String]) -> String {
    format!("{rel}({})", args.join(", "))
}

/// Canonical `rel(a, b)` key for a fact/mock pattern (§3.1) — shared by
/// the schema's own seed `facts:` entries and the CLI's `--fact` mocks so
/// their declared/supplied tuples compare STRUCTURALLY, never by raw
/// source text (whitespace, quoting).
fn fact_pattern_key(pat: &lute_syntax::datalog::FactPattern) -> String {
    let args: Vec<String> = pat.args.iter().map(|a| fact_term_text(&a.term)).collect();
    fmt_fact(&pat.relation, &args)
}

/// A line's `{…}` attrs as authored (`mono`, `as="x"`, `emotion=@e`), in
/// source order — `when=` is already extracted, and `__`-prefixed compiler
/// bookkeeping is not source. `None` when nothing remains.
fn line_delivery(l: &Line) -> Option<String> {
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
fn prereq_condition(f: &lute_check::PrereqFormula) -> String {
    use lute_check::PrereqFormula as F;
    match f {
        F::Visited(k) => format!("visited('{k}')"),
        F::Completed(q) => format!("quest.{q}.state == 'complete'"),
        F::Active(q) => format!("quest.{q}.state == 'active'"),
        F::And(a, b) => format!("({}) && ({})", prereq_condition(a), prereq_condition(b)),
        F::Or(a, b) => format!("({}) || ({})", prereq_condition(a), prereq_condition(b)),
    }
}

fn seeds_summary(mocks: &MockSet) -> Seeds {
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
fn seed_fact_notes(mocks: &MockSet, seed_facts: &[lute_check::meta::FactDecl]) -> Vec<String> {
    if seed_facts.is_empty() {
        return Vec::new();
    }
    let supplied: std::collections::HashSet<String> = mocks
        .facts
        .iter()
        .filter_map(|raw| lute_syntax::datalog::parse_fact(raw).ok())
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
fn derived_read_notes(relations: &BTreeSet<String>) -> Vec<String> {
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
/// [`crate::eval::EffectiveState::reserved_reads`] — a default is
/// necessarily read-time, there is no "admitted but unread" analog for
/// it). Grouped by quest id (one note per id, never per path);
/// informational only, never an error, never a reachability claim, exit
/// code unchanged.
fn reserved_quest_notes(
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
        if !crate::eval::is_reserved_quest_path(path) {
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
fn reserved_quest_id(path: &str) -> &str {
    path.split('.').nth(1).unwrap_or(path)
}

fn reserved_default_text(path: &str) -> &'static str {
    if crate::eval::is_reserved_quest_objective_done_path(path) {
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
fn unmatched_event_notes(doc: &Document, events: &[String]) -> Vec<String> {
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
/// state paths, so the §1.3 reserved-read log is untouched.
fn occasion_notes(doc: &Document, occasions: &[String], decisions: &[Decision]) -> Vec<String> {
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
        let (bare, _) = crate::mock::split_occasion(name);
        if answered.contains(bare) || !seen.insert(name.as_str()) {
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
                    .any(|r| crate::mock::raise_judges(r, on, target))
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
fn mock_unproducible_notes(
    mocks: &MockSet,
    folded: &FoldedEnv,
    doc: &Document,
    project_asserts: Option<&BTreeSet<String>>,
) -> Vec<String> {
    if mocks.facts.is_empty() {
        return Vec::new();
    }
    let mut live_assert: BTreeSet<String> = project_asserts.cloned().unwrap_or_default();
    for shot in &doc.shots {
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
        let Ok(pat) = lute_syntax::datalog::parse_fact(raw) else {
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
fn collect_assert_relations(nodes: &[Node], out: &mut BTreeSet<String>) {
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
                for choice in &h.choices {
                    collect_assert_relations(&choice.body, out);
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
fn collect_on_events<'a>(nodes: &'a [Node], out: &mut BTreeSet<&'a str>) {
    for node in nodes {
        match node {
            Node::Branch(b) => {
                for choice in &b.choices {
                    collect_on_events(&choice.body, out);
                }
            }
            Node::Hub(h) => {
                for choice in &h.choices {
                    collect_on_events(&choice.body, out);
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
fn beat_when_note(
    folded: &FoldedEnv,
    table: &DefTable<'_>,
    m: &mut Machine<&mut TraceDriver<'_>>,
) -> Option<String> {
    let beat = folded.typed.beat.as_ref()?;
    let mut slot = beat.when.clone()?;
    let _ = lute_compile::expand::expand_beat_when(&mut slot, table);
    let v = m.eval_guard(&slot.raw);
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

fn empty_report(uri: &str, mocks: &MockSet) -> TraceReport {
    TraceReport {
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
        said: Vec::new(),
    }
}
