//! Planning: every step's usage-level check against the compiled project
//! before anything plays (exit 2), producing the plan the execution loop
//! walks, and the step `expect:` names checked up front.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;

use lute_manifest::schema::OccasionSelect;
use lute_trace::exec::session::{
    atom_problem, entry_flag, is_candidate, project_decisions, resolve_bridges, resolve_fact,
    resolve_state, seed_world, ExecProject, Pick, SeedError, World, WorldSeed, Write, Writes,
};
use serde_json::Value as Json;

use super::script::{
    in_file_order, scalar_text, AdvanceBy, PlayScript, RawWrite, RawWrites, ScriptStep, Segment,
    StepAction,
};

/// What a planned step does.
pub(super) enum Action {
    Occasion {
        occasion: String,
        target: Option<String>,
        pick: Option<Pick>,
        choose: BTreeMap<String, Vec<String>>,
        /// dsl 0.27.0 §3: the raise's payload, typed, by
        /// `occasion.payload.<field>` path.
        payload: BTreeMap<String, lute_trace::Value>,
        /// dsl 0.27.0 §4: the step's `engine:` writes, applied first.
        writes: Option<Writes>,
    },
    NewRun(Writes),
    Engine(Writes),
    Event(String),
    /// dsl 0.24.0 §1: apply `writes` (the step's `engine:`), move the clock
    /// `by` — raising the clock's `dayEnd` / `dayStart` at every midnight it
    /// crosses — settle, then raise its `slot` occasion (when it declares
    /// one) with the step's `pick` and `choose`.
    Advance {
        by: lute_manifest::clock::Advance,
        writes: Writes,
        raise: lute_manifest::clock::RaiseMoments,
        pick: Option<Pick>,
        choose: BTreeMap<String, Vec<String>>,
    },
    /// `end: true` — the playthrough is over.
    End,
}

/// One step, validated against the project.
pub(super) struct Step {
    pub(super) n: usize,
    pub(super) label: Option<String>,
    pub(super) repeat: usize,
    pub(super) action: Action,
    /// The step's own `bridges:` (dsl 0.24.0 §5), resolved against the
    /// project's plugin calls — every run of the step gets them afresh.
    pub(super) bridges: BTreeMap<String, VecDeque<lute_trace::BridgeAnswer>>,
    /// dsl 0.27.0 (T3-22): the `include:` segments around the step,
    /// outermost first — one `Arc` per segment, shared by its steps.
    pub(super) segments: Vec<Arc<Scope>>,
}

/// A [`Segment`] resolved against the project: what its steps are scripted
/// by over the script's own `choose:` / `bridges:`.
pub(super) struct Scope {
    /// `include: <file>` at `file:line:col`, for messages.
    pub(super) include: String,
    pub(super) choose: BTreeMap<String, Vec<String>>,
    pub(super) bridges: BTreeMap<String, VecDeque<lute_trace::BridgeAnswer>>,
}

/// Resolve an `engine:` step's / `newRun` seed's writes. A `quest.*` path is
/// the quest runner's (its lifecycle transitions fire handlers and grants),
/// never written directly — a save's quest status is top-level `quests:`.
/// What the playthrough itself records — the raise (`occasion.*`), a taken
/// choice, a visit — has its own script spelling ([`recorded_by_play`]);
/// `raising` is set on an `occasion:` step's own `engine:`. `Err` is every
/// write that cannot be made, in the order they were written.
fn resolve_writes(
    p: &ExecProject,
    n: usize,
    key: &str,
    raw: &RawWrites,
    raising: bool,
) -> Result<Writes, Vec<String>> {
    let mut out = Writes::default();
    let mut errs = Vec::new();
    for (path, write) in &raw.state {
        let at = format!("step {n}: `{key}.state.{path}`");
        if path.starts_with("quest.") {
            errs.push(format!(
                "{at}: quest state is written by the quest lifecycle, not the engine — seed a \
                 save's quest status with top-level `quests:`"
            ));
            continue;
        }
        if let Some(instead) = recorded_by_play(path, raising) {
            errs.push(format!("{at}: {instead}"));
            continue;
        }
        let write = match write {
            RawWrite::Lit(lit) => resolve_state(p, path, lit).map(Write::Set),
            RawWrite::Add(d) => {
                let ty = p
                    .state_table
                    .get(path)
                    .map(|e| e.get("type").and_then(Json::as_str));
                let why = match ty {
                    _ if path.starts_with("scene.") => resolve_state(p, path, "").err(),
                    None if entry_flag(path).is_none() => resolve_state(p, path, "").err(),
                    Some(Some("number")) => None,
                    _ => Some("is not a `number` path, so it takes no `{ add: … }`".to_string()),
                };
                why.map_or(Ok(Write::Add(*d)), Err)
            }
        };
        match write {
            Ok(write) => out.state.push((path.clone(), write)),
            Err(why) => errs.push(format!("{at} {why}")),
        }
    }
    for (list, into, what) in [
        (&raw.facts, &mut out.facts, "facts"),
        (&raw.retract, &mut out.retract, "retract"),
    ] {
        for (i, f) in list.iter().enumerate() {
            let next = list.get(i + 1).map(String::as_str);
            match resolve_fact(p, f, next) {
                Ok(fact) => into.push(fact),
                Err(e) => errs.push(format!("step {n}: `{key}.{what}` entry `{f}` {e}")),
            }
        }
    }
    // dsl 0.26.0 §7 (T2-9): the engine accepts an accept-driven quest.
    for id in &raw.accept {
        if p.accept_driven.contains(id) {
            out.accept.push(id.clone());
        } else {
            errs.push(format!(
                "step {n}: `{key}.accept` names `{id}`, which is no accept-driven quest of this \
                 project (a quest with no `start`, e.g. `accept=\"external\"`)"
            ));
        }
    }
    if errs.is_empty() {
        Ok(out)
    } else {
        Err(errs)
    }
}

/// The script spelling of a state path the playthrough records itself, or
/// `None` when an `engine:` write may set the path. `raising`: the write is
/// an `occasion:` step's own, so the raise it belongs to is that step.
fn recorded_by_play(path: &str, raising: bool) -> Option<String> {
    let segs: Vec<&str> = path.split('.').collect();
    let step = if raising {
        "this step"
    } else {
        "the `occasion:` step"
    };
    Some(match segs.as_slice() {
        ["occasion", "payload", field, ..] if raising => format!(
            "the payload comes from the raise, not the engine — write `{field}` in this step's \
             `payload:` instead"
        ),
        ["occasion", "payload", ..] => {
            format!("the payload comes from the raise, not the engine — give {step} a `payload:`")
        }
        ["occasion", ..] => format!(
            "`occasion.*` comes from the raise, not the engine — give {step} a `target:` (and a \
             `payload:` for its fields)"
        ),
        ["scene", "choices", ..] => "the engine records a taken choice when it is taken — answer \
                                     the branch or hub with `choose:`"
            .to_string(),
        ["scene", "visited", ..] => "the engine records visits as scenes play — list the scenes \
                                     a save has visited in top-level `visited:`"
            .to_string(),
        _ => return None,
    })
}

/// Every step's usage-level check, before anything plays (exit 2 on `Err`),
/// producing the plan the walk executes. An occasion exists (under
/// shape-only vocabulary: a beat answers it or an objective is judged at
/// it); `target` is given exactly when the occasion takes one and lies in
/// its domain; `pick` is required exactly for `select: all` and names a beat
/// answering that occasion, or `none`; an `engine:`/`newRun` write names
/// declared paths and relations with values that fit; an `event:` names a
/// declared world event. `Err` is every usage error of every refused step
/// (`step N: …`, unlocated), in step order — a step's independent checks
/// all run, so one refusal never hides another.
pub(super) fn plan_steps(p: &ExecProject, steps: &[ScriptStep]) -> Result<Vec<Step>, Vec<String>> {
    let mut answered: BTreeSet<&str> = p.index.beats.iter().map(|b| b.on.as_str()).collect();
    answered.extend(p.objective_occasions.iter().map(String::as_str));
    let mut plan = Vec::with_capacity(steps.len());
    let mut errs: Vec<String> = Vec::new();
    // One resolved scope per segment, shared by every step it spliced in.
    let mut scopes: Vec<(Arc<Segment>, Arc<Scope>)> = Vec::new();
    let mut plan_step = |step: &ScriptStep| -> Option<Step> {
        let n = step.n;
        let before = errs.len();
        let action = match &step.action {
            StepAction::NewRun(raw) => resolve_writes(p, n, "newRun", raw, false)
                .map(Action::NewRun)
                .map_err(|es| errs.extend(es))
                .ok(),
            StepAction::Engine(raw) => resolve_writes(p, n, "engine", raw, false)
                .map(Action::Engine)
                .map_err(|es| errs.extend(es))
                .ok(),
            StepAction::End => Some(Action::End),
            StepAction::Event(name) => plan_event(p, &answered, n, name)
                .map(|()| Action::Event(name.clone()))
                .map_err(|e| errs.push(e))
                .ok(),
            StepAction::Occasion {
                occasion,
                target,
                pick,
                choose,
                payload,
                writes,
            } => {
                let pick = entry_pick(p, pick);
                let raised =
                    plan_occasion(p, &answered, n, occasion, target.as_deref(), pick.as_ref())
                        .map_err(|e| errs.push(e))
                        .ok();
                let payload = lute_trace::exec::session::typed_payload(p, occasion, payload)
                    .map_err(|e| errs.push(format!("step {n}: {e}")))
                    .ok();
                let writes = writes
                    .as_ref()
                    .map(|raw| resolve_writes(p, n, "engine", raw, true))
                    .transpose()
                    .map_err(|es| errs.extend(es))
                    .ok();
                match (raised, payload, writes) {
                    (Some(()), Some(payload), Some(writes)) => Some(Action::Occasion {
                        occasion: occasion.clone(),
                        target: target.clone(),
                        pick,
                        choose: choose.clone(),
                        payload,
                        writes,
                    }),
                    _ => None,
                }
            }
            StepAction::Advance {
                by,
                writes,
                pick,
                choose,
                occasion_expect,
            } => {
                let Some(clock) = &p.index.clock else {
                    errs.push(format!(
                        "step {n}: `advance:` moves the declared clock, and no schema of this \
                         project declares a `clock:`"
                    ));
                    return None;
                };
                let writes = resolve_writes(p, n, "engine", writes, false)
                    .map_err(|es| errs.extend(es))
                    .ok();
                if let Some((path, _)) = writes
                    .iter()
                    .flat_map(|w| &w.state)
                    .find(|(path, _)| *path == clock.day || clock.slot.as_ref() == Some(path))
                {
                    errs.push(format!(
                        "step {n}: `engine:` writes `{path}`, which the `advance:` beside it \
                         moves — write the clock in a step of its own, or let `advance:` move it"
                    ));
                }
                let by = resolve_advance(clock, n, by).map_err(|e| errs.push(e)).ok();
                let pick = entry_pick(p, pick);
                let raise = clock.raises();
                // dsl 0.27.0 (T3-8): `pick` answers the `select: all` slot
                // raise where the clock stops; `choose` and the selection
                // expectations judge whatever the step raises — its
                // midnights' `dayEnd` / `dayStart` too.
                if raise.slot.is_none() && pick.is_some() {
                    errs.push(format!(
                        "step {n}: `pick` answers the slot occasion an `advance:` raises where \
                         the clock stops, and the clock declares no `raise.slot` occasion"
                    ));
                }
                let raises_any =
                    raise.slot.is_some() || raise.day_start.is_some() || raise.day_end.is_some();
                let key = if !choose.is_empty() {
                    Some("`choose`".to_string())
                } else {
                    occasion_expect.map(|k| format!("`expect.{k}`"))
                };
                if let (Some(key), false) = (key, raises_any) {
                    errs.push(format!(
                        "step {n}: {key} judges what an `advance:` raises, and the clock \
                         declares no `raise:` occasion — the advance presents nothing"
                    ));
                }
                let moments = [
                    (&raise.slot, pick.as_ref()),
                    (&raise.day_start, None),
                    (&raise.day_end, None),
                ];
                for (occasion, pick) in moments {
                    let Some(occasion) = occasion else { continue };
                    if p.occasions
                        .get(occasion)
                        .is_some_and(|d| d.target.takes_target())
                    {
                        errs.push(format!(
                            "step {n}: the clock raises `{occasion}`, which is declared \
                             `target: true` — an `advance:` raises it for no target"
                        ));
                        continue;
                    }
                    // Shape-only vocabulary: an occasion no beat answers
                    // and no objective is judged at is raised to nobody.
                    if !p.occasions.is_empty()
                        || answered.contains(occasion.as_str())
                        || pick.is_some()
                    {
                        if let Err(e) = plan_occasion(p, &answered, n, occasion, None, pick) {
                            errs.push(e);
                        }
                    }
                }
                match (writes, by) {
                    (Some(writes), Some(by)) => Some(Action::Advance {
                        by,
                        writes,
                        raise,
                        pick,
                        choose: choose.clone(),
                    }),
                    _ => None,
                }
            }
        };
        let mut segments = Vec::with_capacity(step.segments.len());
        for seg in &step.segments {
            if let Some((_, scope)) = scopes.iter().find(|(s, _)| Arc::ptr_eq(s, seg)) {
                segments.push(scope.clone());
                continue;
            }
            match resolve_bridges(p, &format!("step {n}: {}", seg.include), &seg.bridges) {
                Ok(bridges) => {
                    let scope = Arc::new(Scope {
                        include: seg.include.clone(),
                        choose: seg.choose.clone(),
                        bridges,
                    });
                    scopes.push((seg.clone(), scope.clone()));
                    segments.push(scope);
                }
                Err(e) => errs.push(e),
            }
        }
        let bridges = resolve_bridges(p, &format!("step {n}"), &step.bridges)
            .map_err(|e| errs.push(e))
            .ok();
        match (action, bridges) {
            (Some(action), Some(bridges)) if errs.len() == before => Some(Step {
                n,
                label: step.label.clone(),
                repeat: step.repeat,
                action,
                bridges,
                segments,
            }),
            _ => None,
        }
    };
    for step in steps {
        if let Some(s) = plan_step(step) {
            plan.push(s);
        }
    }
    if errs.is_empty() {
        Ok(plan)
    } else {
        Err(errs)
    }
}

/// [`plan_steps`] for one `event:` step: a declared world event, not a
/// quest lifecycle event.
fn plan_event(
    p: &ExecProject,
    answered: &BTreeSet<&str>,
    n: usize,
    name: &str,
) -> Result<(), String> {
    if lute_manifest::snapshot::BUILTIN_LIFECYCLE_EVENTS.contains(&name) {
        return Err(format!(
            "step {n}: `{name}` is a quest lifecycle event — the quest runner fires it on a \
             transition; it cannot be fired from a script"
        ));
    }
    if p.world_events.contains(name) {
        return Ok(());
    }
    let hint = if p.occasions.contains_key(name) || answered.contains(name) {
        format!(" — `{name}` is an occasion; raise it with `occasion: {name}`")
    } else if p.world_events.is_empty() {
        " (no plugin declares world events)".to_string()
    } else {
        let declared: Vec<&str> = p.world_events.iter().map(String::as_str).collect();
        format!(" (declared: {})", declared.join(", "))
    };
    Err(format!(
        "step {n}: `event: {name}` names no declared world event{hint}"
    ))
}

/// dsl 0.26.0 §7 (T3-10): a `pick:` naming an entry as `<document id>.<entry
/// id>` picks the entry.
fn entry_pick(p: &ExecProject, pick: &Option<Pick>) -> Option<Pick> {
    pick.as_ref().map(|pk| match pk {
        Pick::Beat(id) => Pick::Beat(p.entry_id(id).to_string()),
        Pick::Pass => Pick::Pass,
    })
}

/// An `advance:` against the declared clock (dsl 0.26.0 §7, T2-5): a `to:`
/// slot is one of `slots`, a `to:` weekday a `week.labels` label or a
/// `clock.weekday` number of the declared `week:`.
fn resolve_advance(
    clock: &lute_manifest::clock::ClockDecl,
    n: usize,
    by: &AdvanceBy,
) -> Result<lute_manifest::clock::Advance, String> {
    let (weekday, slot) = match by {
        AdvanceBy::By(by) => return Ok(*by),
        AdvanceBy::To { weekday, slot } => (weekday, slot),
    };
    let slot = match slot {
        None => None,
        Some(s) if clock.slot.is_none() => {
            return Err(format!(
                "step {n}: `advance: {{ to: … slot: {s} }}` — the clock is day-granular (it \
                 declares no `slots:`)"
            ))
        }
        Some(s) => Some(clock.slot_index(s).ok_or_else(|| {
            format!(
                "step {n}: `advance:` to slot `{s}` — the clock's slots are: {}",
                clock.slots.join(", ")
            )
        })?),
    };
    let weekday = match weekday {
        None => None,
        Some(wd) => {
            let Some(week) = clock.week.as_ref() else {
                return Err(format!(
                    "step {n}: `advance:` to weekday `{wd}` — the clock declares no `week:`"
                ));
            };
            let index = match week.labels.iter().position(|l| l == wd) {
                Some(i) => Some(i as i64),
                None => wd
                    .parse::<i64>()
                    .ok()
                    .filter(|i| (0..i64::from(week.length)).contains(i)),
            };
            Some(index.ok_or_else(|| {
                let labels = if week.labels.is_empty() {
                    String::new()
                } else {
                    format!(" or one of: {}", week.labels.join(", "))
                };
                format!(
                    "step {n}: `advance:` to weekday `{wd}` — a weekday is a number 0..{}{labels}",
                    week.length.saturating_sub(1)
                )
            })?)
        }
    };
    Ok(lute_manifest::clock::Advance::To { weekday, slot })
}

/// [`plan_steps`] for one occasion step.
fn plan_occasion(
    p: &ExecProject,
    answered: &BTreeSet<&str>,
    n: usize,
    occasion: &str,
    target: Option<&str>,
    pick: Option<&Pick>,
) -> Result<(), String> {
    let decl = p.occasions.get(occasion);
    // Round-5 T3-13: the nearest occasion name.
    let near = |known: &mut dyn Iterator<Item = &str>| {
        let known: Vec<&str> = known.collect();
        lute_manifest::suggest::nearest(occasion, known, 2)
            .map(|k| format!(" — did you mean `{k}`?"))
            .unwrap_or_default()
    };
    // T3-42: `o@target` is the joined spelling trace's `--occasion` and a
    // test's `occasions:` take; a step has a `target:` of its own.
    if let Some((name, t)) = occasion.split_once('@') {
        if p.occasions.contains_key(name) || answered.contains(name) {
            return Err(format!(
                "step {n}: occasion `{occasion}`: give the target on its own — `occasion: \
                 {name}` + `target: {t}`"
            ));
        }
    }
    if p.occasions.is_empty() {
        if !answered.contains(occasion) {
            let sugg = near(&mut answered.iter().copied());
            return Err(format!(
                "step {n}: occasion `{occasion}` is answered by no beat and judges no \
                 objective in this project{sugg} (no plugin declares occasions, so the beats' and \
                 objectives' `on` values are the vocabulary)"
            ));
        }
    } else if decl.is_none() {
        let declared: Vec<&str> = p.occasions.keys().map(String::as_str).collect();
        let hint = if p.world_events.contains(occasion) {
            format!(" — `{occasion}` is a world event; fire it with `event: {occasion}`")
        } else {
            near(&mut declared.iter().copied())
        };
        return Err(format!(
            "step {n}: occasion `{occasion}` is declared by no resolved plugin{hint} (declared: {})",
            declared.join(", ")
        ));
    }
    // Round-5 T3-13: under shape-only vocabulary nothing declares that the
    // occasion takes a target — but when every beat answering it has one,
    // a raise for no target presents none of them.
    if target.is_none() && decl.is_none() && !p.objective_occasions.contains(occasion) {
        let targets: BTreeSet<&str> = p
            .index
            .beats
            .iter()
            .filter(|b| b.on == occasion)
            .map(|b| b.target.as_deref())
            .collect::<Option<_>>()
            .unwrap_or_default();
        if !targets.is_empty() {
            let shown: Vec<&str> = targets.iter().take(3).copied().collect();
            let more = if targets.len() > shown.len() {
                ", …"
            } else {
                ""
            };
            return Err(format!(
                "step {n}: `target:` is missing — every beat on `{occasion}` has a target \
                 (`{}`{more}), so raised for no target it presents none of them; did you forget \
                 `target:`?",
                shown.join("`, `")
            ));
        }
    }
    match (target, decl) {
        (Some(t), Some(d)) if !d.target.takes_target() => {
            return Err(format!(
                "step {n}: occasion `{occasion}` is not declared `target: true`, so it cannot \
                 be raised for `{t}`"
            ));
        }
        (None, Some(d)) if d.target.takes_target() => {
            return Err(format!(
                "step {n}: occasion `{occasion}` is declared `target: true` — name what it is \
                 raised for with `target:`"
            ));
        }
        (Some(t), Some(d)) => {
            lute_check::occasion_target_ok(d, t, &p.kinds).map_err(|e| format!("step {n}: {e}"))?;
        }
        _ => {}
    }
    let select = p.select_of(occasion);
    match (select, pick) {
        // dsl 0.24.0 (T3-10): no `pick:` is `pick: none` when the list is
        // empty; a non-empty list without one halts at the step (the offered
        // list is only known when the occasion is raised).
        (OccasionSelect::All, None) => Ok(()),
        (OccasionSelect::First | OccasionSelect::Sequence, Some(pk)) => Err(format!(
            "step {n}: `pick: {}` applies only to a `select: all` occasion; \
             `{occasion}` is `select: {}`",
            match pk {
                Pick::Beat(id) => id.as_str(),
                Pick::Pass => "none",
            },
            select.as_str()
        )),
        (OccasionSelect::All, Some(Pick::Beat(pk))) => {
            if p.index
                .beats
                .iter()
                .any(|b| &b.id == pk && is_candidate(b, occasion, target))
            {
                Ok(())
            } else {
                Err(format!(
                    "step {n}: `pick: {pk}` names no beat answering `{occasion}`{}",
                    target.map(|t| format!(" for `{t}`")).unwrap_or_default()
                ))
            }
        }
        (OccasionSelect::All, Some(Pick::Pass))
        | (OccasionSelect::First | OccasionSelect::Sequence, None) => Ok(()),
    }
}

/// Round-5 T3-13: a `step N: …` usage error located at step N's file line
/// (its play, or the steps file an `include:` spliced it from); `None` when
/// the error names no step.
pub(super) fn locate_step_error(steps: &[ScriptStep], e: &str) -> Option<String> {
    let n: usize = e
        .strip_prefix("step ")?
        .split(|c: char| !c.is_ascii_digit())
        .next()?
        .parse()
        .ok()?;
    let step = steps.iter().find(|s| s.n == n)?;
    Some(step.at.locate(e))
}

/// Every `step N: …` usage error located ([`locate_step_error`]), else
/// prefixed with `script_path`.
fn locate_all(steps: &[ScriptStep], script_path: &Path, errs: Vec<String>) -> Vec<String> {
    errs.into_iter()
        .map(|e| {
            locate_step_error(steps, &e)
                .unwrap_or_else(|| format!("{}: {e}", script_path.display()))
        })
        .collect()
}

/// A usage error about step `n`'s node `keys` (item `item` of that list,
/// when it names one), located there: `<file:line:col>: step N: <msg>`.
fn step_error(
    script: &PlayScript,
    n: usize,
    keys: &[&str],
    item: Option<usize>,
    msg: &str,
) -> String {
    let msg = format!("step {n}: {msg}");
    match script.steps.iter().find(|s| s.n == n) {
        Some(step) => match item {
            Some(i) => step.at.locate_item(keys, i, &msg),
            None => step.at.locate_keys(keys, &msg),
        },
        None => msg,
    }
}

/// A usage error about the script's top-level node `keys` (item `item` of
/// that list, when it names one), located there.
fn top_error(script: &PlayScript, keys: &[&str], item: Option<usize>, msg: &str) -> String {
    let at = match item {
        Some(i) => script.source.at_item(keys, i),
        None => script.source.at(keys),
    };
    format!("{at}: {msg}")
}

/// What a step `expect:` names that the project must have — a beat id
/// (`winner`, `offered`, `notOffered`, `presented`; an entry by its
/// `<document id>.<entry id>` alias too), an occasion the clock raises (an
/// `advance:` step's `presented: { <occasion>: [...] }`) and a clock
/// position (`clock: slot` a declared slot, `weekday` a `week.labels` label
/// or a weekday number) — checked before the play runs, with did-you-mean.
/// A typo is a usage error (exit 2), not a miss; every one is reported, at
/// its entry.
fn check_expect_names(p: &ExecProject, script: &PlayScript, errs: &mut Vec<String>) {
    let near = |needle: &str, known: &[&str]| {
        lute_manifest::suggest::did_you_mean(needle, known.iter().copied())
    };
    let beat_ids: Vec<&str> = p
        .index
        .beats
        .iter()
        .map(|b| b.id.as_str())
        .chain(p.entry_aliases.keys().map(String::as_str))
        .collect();
    // The occasions an `advance:` raises, in the clock's `raise:` order.
    let mut raised_owned: Vec<String> = Vec::new();
    if let Some(clock) = &p.index.clock {
        let m = clock.raises();
        for o in [m.slot, m.day_start, m.day_end].into_iter().flatten() {
            if !raised_owned.contains(&o) {
                raised_owned.push(o);
            }
        }
    }
    let raised: Vec<&str> = raised_owned.iter().map(String::as_str).collect();
    for (n, _, expect) in &script.step_expects {
        let n = *n;
        // Every list of beat ids: `expect.<key>`, or `expect.presented.<o>`.
        let mut lists: Vec<(Vec<&str>, &serde_yaml::Value)> = Vec::new();
        for key in ["winner", "offered", "notOffered", "presented"] {
            match expect.get(key) {
                Some(serde_yaml::Value::Mapping(keyed)) if key == "presented" => {
                    for (occasion, v) in keyed {
                        let Some(occasion) = occasion.as_str() else {
                            continue;
                        };
                        if !raised.contains(&occasion) {
                            errs.push(step_error(
                                script,
                                n,
                                &["expect", key, occasion],
                                None,
                                &format!(
                                    "`expect.presented` names `{occasion}`, which the clock's \
                                     `advance:` does not raise{} (it raises: {})",
                                    near(occasion, &raised),
                                    if raised.is_empty() {
                                        "nothing".to_string()
                                    } else {
                                        raised.join(", ")
                                    }
                                ),
                            ));
                            continue;
                        }
                        lists.push((vec!["expect", key, occasion], v));
                    }
                }
                Some(v) => lists.push((vec!["expect", key], v)),
                None => {}
            }
        }
        for (keys, v) in lists {
            let key = keys[1..].join(".");
            let ids: Vec<(Option<usize>, String)> = match v {
                serde_yaml::Value::Sequence(items) => items
                    .iter()
                    .enumerate()
                    .filter_map(|(i, v)| Some((Some(i), scalar_text(v)?)))
                    .collect(),
                v => scalar_text(v).map(|id| (None, id)).into_iter().collect(),
            };
            for (item, id) in ids {
                if (key == "winner" && id == "none") || beat_ids.contains(&id.as_str()) {
                    continue;
                }
                let mut refuse = |msg: String| errs.push(step_error(script, n, &keys, item, &msg));
                // `<id> for <member>` names a `for` beat's presentation for
                // one member of its kind.
                if let Some((beat, member)) = id.split_once(" for ") {
                    let members = p
                        .index
                        .beats
                        .iter()
                        .filter(|b| b.id == beat)
                        .find_map(|b| b.for_kind.as_ref())
                        .map(|k| k.members.iter().map(String::as_str).collect::<Vec<_>>());
                    match members {
                        Some(ms) if ms.contains(&member) => continue,
                        Some(ms) => {
                            refuse(format!(
                                "`expect.{key}` names `{id}`, and `{member}` is no member \
                                 `{beat}` is presented for{} (members: {})",
                                near(member, &ms),
                                ms.join(", ")
                            ));
                            continue;
                        }
                        None if beat_ids.contains(&beat) => {
                            refuse(format!(
                                "`expect.{key}` names `{id}`, and `{beat}` is no `for` beat — \
                                 name it bare"
                            ));
                            continue;
                        }
                        None => {}
                    }
                }
                refuse(format!(
                    "`expect.{key}` names `{id}`, which no beat of the project has{}",
                    near(&id, &beat_ids)
                ));
            }
        }
        let Some(serde_yaml::Value::Mapping(clock_want)) = expect.get("clock") else {
            continue;
        };
        let mut refuse = |keys: &[&str], msg: String| {
            errs.push(step_error(script, n, keys, None, &msg));
        };
        let Some(clock) = &p.index.clock else {
            refuse(
                &["expect", "clock"],
                "`expect.clock` judges the declared clock, and no schema of this project \
                 declares a `clock:`"
                    .to_string(),
            );
            continue;
        };
        if let Some(slot) = clock_want.get("slot").and_then(serde_yaml::Value::as_str) {
            let slots: Vec<&str> = clock.slots.iter().map(String::as_str).collect();
            if clock.slot.is_none() {
                refuse(
                    &["expect", "clock", "slot"],
                    format!(
                        "`expect.clock.slot: {slot}` — the clock is day-granular (it declares \
                         no `slots:`)"
                    ),
                );
            } else if !slots.contains(&slot) {
                refuse(
                    &["expect", "clock", "slot"],
                    format!(
                        "`expect.clock.slot: {slot}` names no slot of the clock{} (slots: {})",
                        near(slot, &slots),
                        slots.join(", ")
                    ),
                );
            }
        }
        if clock_want.contains_key("ended") && !clock.is_finite() {
            refuse(
                &["expect", "clock", "ended"],
                "`expect.clock.ended` — the clock never ends (it declares no `last:` or `days:`)"
                    .to_string(),
            );
        }
        if let Some(want) = clock_want.get("weekday") {
            let Some(week) = clock.week.as_ref() else {
                refuse(
                    &["expect", "clock", "weekday"],
                    "`expect.clock.weekday` — the clock declares no `week:`".to_string(),
                );
                continue;
            };
            let labels: Vec<&str> = week.labels.iter().map(String::as_str).collect();
            let text = scalar_text(want).unwrap_or_default();
            let ok = labels.contains(&text.as_str())
                || text
                    .parse::<i64>()
                    .is_ok_and(|i| (0..i64::from(week.length)).contains(&i));
            if !ok {
                let named = if labels.is_empty() {
                    String::new()
                } else {
                    format!(" or one of: {}", labels.join(", "))
                };
                refuse(
                    &["expect", "clock", "weekday"],
                    format!(
                        "`expect.clock.weekday: {text}` names no weekday{} — a weekday is a \
                         number 0..{}{named}",
                        near(&text, &labels),
                        week.length.saturating_sub(1)
                    ),
                );
            }
        }
    }
}

/// Every `choose:` of the script — the top level's, an `include:`'s, a
/// step's — names a branch or hub of the project and only options it
/// declares, and every `include:`'s `bridges:` resolves; checked before
/// anything plays, with did-you-mean, located at the key. A typo is a usage
/// error (exit 2), not a decision silently dropped; every one is reported.
fn check_decisions(p: &ExecProject, script: &PlayScript, errs: &mut Vec<String>) {
    let known = project_decisions(p);
    for (id, picks) in &script.surfaces.choose {
        if let Err(e) = choose_entry(&known, id, picks) {
            errs.push(top_error(script, &["choose", id], None, &e));
        }
    }
    let mut seen: Vec<&Arc<Segment>> = Vec::new();
    for step in &script.steps {
        let own = match &step.action {
            StepAction::Occasion { choose, .. } | StepAction::Advance { choose, .. } => {
                Some(choose)
            }
            _ => None,
        };
        for (id, picks) in own.into_iter().flatten() {
            if let Err(e) = choose_entry(&known, id, picks) {
                errs.push(
                    step.at
                        .locate_keys(&["choose", id], &format!("step {}: {e}", step.n)),
                );
            }
        }
        for seg in &step.segments {
            if seen.iter().any(|s| Arc::ptr_eq(s, seg)) {
                continue;
            }
            seen.push(seg);
            for (id, picks) in &seg.choose {
                if let Err(e) = choose_entry(&known, id, picks) {
                    errs.push(seg.at.locate_keys(&["choose", id], &e));
                }
            }
            for (tag, answers) in &seg.bridges {
                let one = BTreeMap::from([(tag.clone(), answers.clone())]);
                if let Err(e) = resolve_bridges(p, "", &one) {
                    errs.push(
                        seg.at
                            .locate_keys(&["bridges", tag], e.trim_start_matches(": ")),
                    );
                }
            }
        }
    }
}

/// One `choose:` entry against the project's decisions: `id` a branch or
/// hub, each pick one of its options.
fn choose_entry(
    known: &BTreeMap<String, Vec<String>>,
    id: &str,
    picks: &[String],
) -> Result<(), String> {
    let hint = |needle: &str, among: &[&str], what: &str| match lute_manifest::suggest::nearest(
        needle,
        among.iter().copied(),
        2,
    ) {
        Some(near) => format!(" — did you mean `{near}`?"),
        None if among.is_empty() => format!(" (the project declares no {what})"),
        None => format!(" ({what}: {})", among.join(", ")),
    };
    let Some(options) = known.get(id) else {
        let ids: Vec<&str> = known.keys().map(String::as_str).collect();
        return Err(format!(
            "`choose.{id}` names no branch or hub of the project{}",
            hint(id, &ids, "branch or hub ids")
        ));
    };
    let options: Vec<&str> = options.iter().map(String::as_str).collect();
    for pick in picks {
        if !options.contains(&pick.as_str()) {
            return Err(format!(
                "`choose.{id}` picks `{pick}`, which is no option of `{id}`{}",
                hint(pick, &options, "its options")
            ));
        }
    }
    Ok(())
}

/// Every seed the project cannot take ([`seed_world`]), located at the
/// script's key (or list item) it was written at.
pub(super) fn seed_errors(script: &PlayScript, errs: &[SeedError]) -> Vec<String> {
    errs.iter()
        .map(|e| {
            let keys: Vec<&str> = e.keys.iter().map(String::as_str).collect();
            top_error(script, &keys, e.item, &e.msg)
        })
        .collect()
}

/// Plan `script`'s steps over `project` and seed its save. `Err` is exit 2
/// with every usage error of the script, each located, in the order they
/// were written ([`in_file_order`]).
pub(super) fn plan_script(
    project: &ExecProject,
    script: &PlayScript,
    script_path: &Path,
    no_derive: bool,
) -> Result<(Vec<Step>, World), (ExitCode, String)> {
    let mut errs = Vec::new();
    check_decisions(project, script, &mut errs);
    let plan = match plan_steps(project, &script.steps) {
        Ok(plan) => Some(plan),
        Err(es) => {
            errs.extend(locate_all(&script.steps, script_path, es));
            None
        }
    };
    check_expect_names(project, script, &mut errs);
    check_needles(project, script, &mut errs);
    check_expect_state_values(project, script, &mut errs);
    check_expect_facts(project, script, &mut errs);
    let world = seed_world(
        project,
        &WorldSeed {
            surfaces: &script.surfaces,
            save: &script.save,
            derive: script.derive,
        },
    );
    let world = match world {
        Ok(world) => Some(world),
        Err(es) => {
            errs.extend(seed_errors(script, &es));
            None
        }
    };
    match (plan, world) {
        (Some(plan), Some(mut world)) if errs.is_empty() => {
            if no_derive {
                world.derive = Some(false);
            }
            Ok((plan, world))
        }
        _ => Err((ExitCode::from(2), in_file_order(errs))),
    }
}

/// Every top-level `transcriptContains` / `transcriptLacks` needle names
/// only speakers and attributes a presented line can carry in this project
/// ([`lute_trace::exec::record::needle_problem`]) — otherwise the needle can
/// never match, and a `transcriptLacks` holds although the line was said. A
/// usage error (exit 2) before anything plays, located at the needle.
fn check_needles(p: &ExecProject, script: &PlayScript, errs: &mut Vec<String>) {
    let Some(expect) = &script.expect else {
        return;
    };
    for key in ["transcriptContains", "transcriptLacks"] {
        let Some(serde_yaml::Value::Sequence(needles)) = expect.get(key) else {
            continue;
        };
        for (i, needle) in needles.iter().enumerate() {
            let Some(needle) = needle.as_str() else {
                continue;
            };
            if let Some(why) = lute_trace::exec::record::needle_problem(needle, &p.needles) {
                errs.push(top_error(
                    script,
                    &["expect", key],
                    Some(i),
                    &format!("`expect.{key}` {why}"),
                ));
            }
        }
    }
}

/// Every `facts:` / `notFacts:` atom of a step or end-of-play `expect:`
/// names a fact the project can hold — a declared relation (derived ones
/// too) at its arity whose closed-domain arguments are members
/// ([`lute_trace::exec::session::atom_problem`]). A misspelt `notFacts` atom
/// would hold vacuously, a `facts` one could only miss: a usage error (exit
/// 2) with a did-you-mean, located at the atom.
fn check_expect_facts(p: &ExecProject, script: &PlayScript, errs: &mut Vec<String>) {
    let problems = |expect: &serde_yaml::Value| -> Vec<(&'static str, usize, String)> {
        let mut out = Vec::new();
        for key in ["facts", "notFacts"] {
            let Some(serde_yaml::Value::Sequence(atoms)) = expect.get(key) else {
                continue;
            };
            for (i, atom) in atoms.iter().enumerate() {
                let Some(atom) = scalar_text(atom) else {
                    continue;
                };
                let Some((rel, args)) = crate::play_expect::parse_atom(&atom) else {
                    continue;
                };
                if let Some(why) = atom_problem(p, &rel, &args) {
                    out.push((key, i, format!("`expect.{key}` entry `{atom}` {why}")));
                }
            }
        }
        out
    };
    for (n, _, expect) in &script.step_expects {
        for (key, i, why) in problems(expect) {
            errs.push(step_error(script, *n, &["expect", key], Some(i), &why));
        }
    }
    for (key, i, why) in script.expect.as_ref().map(problems).unwrap_or_default() {
        errs.push(top_error(
            script,
            &["expect", key],
            Some(i),
            &format!("end of play: {why}"),
        ));
    }
}

/// A step or end-of-play `expect.state` value of a path typed over a closed
/// domain — `{ domain: K }` / `{ entity: K }`, an enum — is one of its
/// members ([`lute_trace::exec::session::member_of`]): a typo can never
/// hold, so it is a usage error (exit 2) with the members and the nearest
/// one, not a miss after the play ran.
fn check_expect_state_values(p: &ExecProject, script: &PlayScript, errs: &mut Vec<String>) {
    let domain_of = |path: &str| -> Option<(String, Vec<String>)> {
        if let Some(d) = p.state_domains.get(path) {
            return Some(d.clone());
        }
        let entry = p.state_table.get(path)?;
        (entry.get("type").and_then(Json::as_str) == Some("enum")).then_some(())?;
        let members = entry
            .get("domain")?
            .as_array()?
            .iter()
            .filter_map(|m| m.as_str().map(str::to_string))
            .collect();
        Some((path.to_string(), members))
    };
    let problems = |expect: &serde_yaml::Value| -> Vec<(String, String)> {
        let Some(serde_yaml::Value::Mapping(state)) = expect.get("state") else {
            return Vec::new();
        };
        state
            .iter()
            .filter_map(|(path, want)| {
                let (path, want) = (path.as_str()?, want.as_str()?);
                let (domain, members) = domain_of(path)?;
                let why = lute_trace::exec::session::member_of(&domain, &members, want).err()?;
                Some((
                    path.to_string(),
                    format!("`expect.state.{path}: {want}` can never hold: {why}"),
                ))
            })
            .collect()
    };
    for (n, _, expect) in &script.step_expects {
        for (path, why) in problems(expect) {
            let msg = format!("step {n}: {why}");
            errs.push(match script.steps.iter().find(|s| s.n == *n) {
                Some(step) => step.at.locate_value(&["expect", "state", &path], &msg),
                None => msg,
            });
        }
    }
    for (path, why) in script.expect.as_ref().map(problems).unwrap_or_default() {
        let at = script.source.at_value(&["expect", "state", &path]);
        errs.push(format!("{at}: end of play: {why}"));
    }
}
