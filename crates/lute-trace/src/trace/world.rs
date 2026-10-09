use super::*;
pub(super) fn is_true(m: &Machine<&mut TraceDriver<'_>>, path: &str) -> bool {
    m.read(path) == lute_runtime::eval::Read::Value(Value::Bool(true))
}

/// The world a walk left, for the report's final state and facts.
#[derive(Default)]
pub(super) struct World {
    pub(super) state: BTreeMap<String, Value>,
    pub(super) facts: BTreeSet<String>,
    pub(super) undecided: BTreeSet<String>,
    pub(super) reserved_reads: BTreeMap<String, bool>,
    pub(super) derived_reads: BTreeSet<String>,
    /// Whether the project's `terminal:` holds in it ([`terminal_at`]).
    pub(super) terminal: Option<bool>,
}

/// Whether the project's `terminal:` condition holds in the walk's world —
/// `None` without one, or when the mocks leave it undecided.
pub(super) fn terminal_at(
    m: &mut Machine<&mut TraceDriver<'_>>,
    terminal: Option<&exec::Slot>,
) -> Option<bool> {
    m.eval_guard(terminal?).ok()
}

impl World {
    pub(super) fn of(m: &mut Machine<&mut TraceDriver<'_>>) -> Self {
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
            terminal: None,
        }
    }

    /// A later presentation's world over this one: its state and facts
    /// win; what every presentation read accumulates (first read kept).
    pub(super) fn then(mut self, later: World) -> Self {
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
            terminal: later.terminal,
        }
    }
}

pub(super) struct Finish<'a, 'd> {
    pub(super) input: &'a CheckInput,
    pub(super) mocks: &'a MockSet,
    pub(super) folded: &'a FoldedEnv,
    pub(super) doc: &'a Document,
    pub(super) project_asserts: Option<&'a BTreeSet<String>>,
    pub(super) driver: TraceDriver<'d>,
    pub(super) walk: Walked,
    pub(super) beat_note: Option<String>,
    pub(super) scene_eligible: Option<(String, Option<bool>)>,
    pub(super) world: World,
    /// The occasions whose mocked raise bound the presented unit's
    /// `occasion.target`: those raises did what they were for.
    pub(super) binding_ons: BTreeSet<String>,
}

/// The notes, the exit code and the report.
pub(super) fn finish(f: Finish<'_, '_>) -> (TraceReport, TraceExit) {
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
        binding_ons,
    } = f;
    let World {
        state,
        facts,
        undecided,
        reserved_reads,
        derived_reads,
        terminal,
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
    notes.extend(occasion_notes(
        doc,
        &mocks.occasions,
        &binding_ons,
        &driver.decisions,
    ));
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
    let runtime_witness = !driver.steps.is_empty();
    let report = TraceReport {
        evidence: match &exit {
            TraceExit::Complete => lute_core_span::Evidence::Witnessed,
            TraceExit::Incomplete => lute_core_span::Evidence::Unknown,
            TraceExit::Refused(_) if runtime_witness => lute_core_span::Evidence::Witnessed,
            TraceExit::Refused(_) => lute_core_span::Evidence::Unknown,
        },
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
        ineligible_by: driver.ineligible_by,
        said: driver.said,
        terminal,
    };
    (report, exit)
}

pub(super) fn logic_diag(code: &str, message: String, span: Span) -> Diagnostic {
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
        evidence: None,
    }
}

pub(super) fn choice_diag(span: Span, id: &str, choice_id: &str, reason: &str) -> Diagnostic {
    let mut diagnostic = logic_diag(
        mock::E_TRACE_CHOICE,
        format!(
            "`--choose {id}={choice_id}` is ineligible at its presentation point: {reason} (dsl 0.4.0 §4.4)"
        ),
        span,
    );
    diagnostic.evidence = Some(lute_core_span::Evidence::Witnessed);
    diagnostic
}
