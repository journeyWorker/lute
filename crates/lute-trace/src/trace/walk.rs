use super::*;
/// A seed for a resumed presentation: the carry already holds the seeded
/// state and facts, so only the lifecycle surfaces ride along.
pub(super) fn carried(seed: &Seed) -> Seed {
    Seed {
        state: Vec::new(),
        facts: Vec::new(),
        ..seed.clone()
    }
}

/// How the walk ended, before the report's exit code.
pub(super) enum Walked {
    Continue,
    Ended,
    Incomplete,
    Refused(Vec<Diagnostic>),
}

/// Run the Machine and read how it ended.
pub(super) fn run_walk(m: &mut Machine<&mut TraceDriver<'_>>) -> Walked {
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
pub(super) fn present_entry(
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
    run_presented(m)
}

/// [`run_walk`] for one presented entry or bundle beat: a body that ran to
/// its end shows the source-only steps after its last record too (a body
/// ending in a `::use` closes that component's frame).
pub(super) fn run_presented(m: &mut Machine<&mut TraceDriver<'_>>) -> Walked {
    let walked = run_walk(m);
    if matches!(walked, Walked::Continue) {
        m.driver_mut().close_unit();
    }
    walked
}

/// Present ONE bundle `<beat>` (dsl 0.23.0 §4): a scene-like beat declared
/// in a lore document, its body walked by the Machine with every effect
/// applied. Its eligibility — `after=` (dsl 0.25.0 §3) over the mocked
/// `visited:` and quest states, `spentBy`, `when` — is the session's rule
/// ([`exec::session::judge_beat`]), SHOWN on the [`Step::Beat`] head under
/// the canonical id and enforced only under [`MockSet::gate_eligibility`],
/// as on an entry. An `after=` the mocks leave undecided (a quest another
/// document declares) is reported unresolved.
pub(super) fn present_beat(
    m: &mut Machine<&mut TraceDriver<'_>>,
    beat: &lute_syntax::ast::BundleBeat,
    canonical: &str,
    judging: Option<&ExecProject>,
    mocks: &MockSet,
) -> Walked {
    let Some((p, row)) = judging.and_then(|p| Some((p, p.lore_beat(canonical)?))) else {
        return run_presented(m);
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
    run_presented(m)
}

/// The member a mocked raise of `on` for a target binds as
/// `occasion.target` for `unit`, which runs for `members`: `landed@fish.cod`
/// binds `cod` (a bare `landed@cod` too), as the engine binds it presenting
/// the unit. `None` when no raise of `on` names a target; a target that is
/// none of `members` is refused.
pub(super) fn raised_member(
    mocks: &MockSet,
    occasions: &BTreeMap<String, lute_manifest::schema::OccasionDecl>,
    unit: &str,
    on: &str,
    members: &[String],
) -> Result<Option<String>, Diagnostic> {
    let Some((raise, target)) =
        mocks
            .occasions
            .iter()
            .find_map(|r| match lute_runtime::split_occasion(r) {
                (name, Some(t)) if name == on => Some((r, t)),
                _ => None,
            })
    else {
        return Ok(None);
    };
    let member = occasions
        .get(on)
        .and_then(|d| lute_manifest::semantics::gates::target_member(d, target))
        .unwrap_or_else(|| target.to_string());
    if members.contains(&member) {
        return Ok(Some(member));
    }
    // `departure@npc.maud` on an occasion raised for no target: the member
    // is written with a prefix the occasion does not draw.
    let untargeted = occasions.get(on).is_some_and(|d| !d.target.takes_target());
    let hint = match member.rsplit_once('.') {
        Some((_, bare)) if untargeted && members.iter().any(|m| m == bare) => format!(
            " — write `{on}@{bare}`: `{on}` is raised for no target of its own, so the raise \
             names the member alone"
        ),
        _ => lute_manifest::suggest::did_you_mean(&member, members.iter().map(String::as_str)),
    };
    Err(logic_diag(
        mock::E_TRACE_MOCK_TYPE,
        format!(
            "the raise `{raise}` binds `occasion.target` to `{member}`, which is not a member \
             {unit} runs for ({}){hint}",
            members.join(", "),
        ),
        mock::synthetic_span(),
    ))
}
