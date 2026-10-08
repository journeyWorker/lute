use super::*;
/// The traced document as a one-document [`ExecProject`] — what the
/// session's eligibility rule judges a presented beat over. `None` only if
/// the artifact does not assemble (it always does once compiled).
pub(super) fn judging_project(
    uri: &str,
    artifact: lute_compile::ExecutionIr,
    occasions: &BTreeMap<String, lute_manifest::schema::OccasionDecl>,
) -> Option<ExecProject> {
    let inputs = [lute_compile::index::IndexInput {
        path: uri.to_string(),
        artifact_path: format!("{uri}.json"),
        artifact: &artifact,
    }];
    let unions = lute_compile::index::IndexUnions {
        occasions: occasions.iter().map(|(name, decl)| (name.clone(), decl.into())).collect(),
        ..Default::default()
    };
    let index = lute_compile::index::build_index(lute_compile::LUTE_IR_VERSION, &inputs, &unions).ok()?;
    let artifacts = BTreeMap::from([(format!("{uri}.json"), serde_json::to_value(artifact).ok()?)]);
    ExecProject::load(lute_runtime::index::Bundle::new(artifacts, serde_json::to_value(index).ok()?)).ok()
}

/// Judge `row` by the session's ONE eligibility rule
/// ([`exec::session::judge_beat`]) at this point of the walk: the walk's
/// own Machine evaluates (`when`, `spentBy` over the mocks, three-valued),
/// over a world carrying what the mocks say about the rest of the project
/// — the mocked `visited:` (which, for a scene, is also what spends its
/// `once: user`), the quest states the walk holds, the entry's read flags.
pub(super) fn judge(
    p: &ExecProject,
    m: &mut Machine<&mut TraceDriver<'_>>,
    mocks: &MockSet,
    row: &lute_runtime::index::IndexBeat,
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
            if let lute_runtime::eval::Read::Value(Value::Str(s)) =
                m.read(&format!("quest.{q}.state"))
            {
                w.quests.insert(q, s);
            }
        }
    }
    let member = match m.read(lute_check::beats::OCCASION_TARGET) {
        lute_runtime::eval::Read::Value(Value::Str(s)) => Some(s),
        _ => None,
    };
    let cand = exec::session::judge_beat(p, &w, m, row, member.as_deref(), member.as_deref());
    (cand, w)
}

/// A session verdict as the report's tri-state eligibility.
pub(super) fn eligible_of(v: &SessionVerdict) -> Option<bool> {
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
pub(super) fn premise_text(
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
            chapters,
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
                format!(" — add {} to the mocks", mocks.join(" and "))
            };
            match kind {
                BeatKind::Bundle => format!("its `after=\"{raw}\"` is false{hint}"),
                BeatKind::Scene | BeatKind::Entry if *chapters => format!(
                    "its `after: {raw}`{} is false{hint}",
                    lute_check::chapters::PROVENANCE
                ),
                BeatKind::Scene | BeatKind::Entry => format!("its `after: {raw}` is false{hint}"),
            }
        }
        Premise::Spent {
            once: Some(lute_runtime::index::BeatOnce::User),
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
pub(super) fn authored_when(slot: &lute_syntax::ast::CelSlot) -> &str {
    slot.authored.as_deref().unwrap_or(&slot.raw)
}

/// OT-F-10: a false `when` (compiled `raw`, shown as `authored`) named by
/// its false conjunct(s) first, each with what it read — a negated fact
/// the mocks seeded said so — then the whole `when` when it has more than
/// the one conjunct.
pub(super) fn when_text(
    m: &mut Machine<&mut TraceDriver<'_>>,
    mocks: &MockSet,
    raw: &str,
    authored: &str,
) -> String {
    let seeded = |f: &str| {
        let bare = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
        mocks.facts.iter().any(|s| bare(s) == bare(f))
    };
    let conjuncts = lowered(raw)
        .map(|cond| m.false_conjuncts(&cond))
        .unwrap_or_default();
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
pub(super) fn note_premise(m: &mut Machine<&mut TraceDriver<'_>>, id: &str, prem: &Premise, why: String) {
    let d = m.driver_mut();
    d.premises.insert(id.to_string(), why);
    d.ineligible_by.insert(id.to_string(), prem.kind());
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
