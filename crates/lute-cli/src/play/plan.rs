//! Planning: every step's usage-level check against the compiled project
//! before anything plays (exit 2), producing the plan the execution loop
//! walks, and the step `expect:` names checked up front.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;

use lute_manifest::schema::OccasionSelect;
use lute_trace::exec::session::{
    entry_flag, is_candidate, resolve_bridges, resolve_fact, resolve_state, seed_world,
    ExecProject, Pick, World, WorldSeed, Write, Writes,
};
use serde_json::Value as Json;

use super::script::{
    scalar_text, AdvanceBy, PlayScript, RawWrite, RawWrites, ScriptStep, Segment, StepAction,
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
    pub(super) choose: BTreeMap<String, Vec<String>>,
    pub(super) bridges: BTreeMap<String, VecDeque<lute_trace::BridgeAnswer>>,
}

/// Resolve an `engine:` step's / `newRun` seed's writes. A `quest.*` path is
/// the quest runner's (its lifecycle transitions fire handlers and grants),
/// never written directly — a save's quest status is top-level `quests:`.
fn resolve_writes(p: &ExecProject, n: usize, key: &str, raw: &RawWrites) -> Result<Writes, String> {
    let mut out = Writes::default();
    for (path, write) in &raw.state {
        let at = format!("step {n}: `{key}.state.{path}`");
        if path.starts_with("quest.") {
            return Err(format!(
                "{at}: quest state is written by the quest lifecycle, not the engine — seed a \
                 save's quest status with top-level `quests:`"
            ));
        }
        let write = match write {
            RawWrite::Lit(lit) => {
                Write::Set(resolve_state(p, path, lit).map_err(|e| format!("{at} {e}"))?)
            }
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
                if let Some(why) = why {
                    return Err(format!("{at} {why}"));
                }
                Write::Add(*d)
            }
        };
        out.state.push((path.clone(), write));
    }
    for (list, into, what) in [
        (&raw.facts, &mut out.facts, "facts"),
        (&raw.retract, &mut out.retract, "retract"),
    ] {
        for f in list {
            into.push(
                resolve_fact(p, f)
                    .map_err(|e| format!("step {n}: `{key}.{what}` entry `{f}` {e}"))?,
            );
        }
    }
    // dsl 0.26.0 §7 (T2-9): the engine accepts an accept-driven quest.
    for id in &raw.accept {
        if !p.accept_driven.contains(id) {
            return Err(format!(
                "step {n}: `{key}.accept` names `{id}`, which is no accept-driven quest of this \
                 project (a quest with no `start`, e.g. `accept=\"external\"`)"
            ));
        }
        out.accept.push(id.clone());
    }
    Ok(out)
}

/// Every step's usage-level check, before anything plays (exit 2 on `Err`),
/// producing the plan the walk executes. An occasion exists (under
/// shape-only vocabulary: a beat answers it or an objective is judged at
/// it, dsl 0.21.0 §7a.2); `target` is given exactly when the occasion takes
/// one and lies in its domain (dsl 0.22.0 §8); `pick` is required exactly
/// for `select: all` and names a beat answering that occasion, or `none`
/// (§10); an `engine:`/`newRun` write names declared paths and relations
/// with values that fit (§1.1); an `event:` names a declared world event
/// (§9).
pub(super) fn plan_steps(p: &ExecProject, steps: &[ScriptStep]) -> Result<Vec<Step>, String> {
    let mut answered: BTreeSet<&str> = p.index.beats.iter().map(|b| b.on.as_str()).collect();
    answered.extend(p.objective_occasions.iter().map(String::as_str));
    let mut plan = Vec::with_capacity(steps.len());
    // One resolved scope per segment, shared by every step it spliced in.
    let mut scopes: Vec<(Arc<Segment>, Arc<Scope>)> = Vec::new();
    for step in steps {
        let n = step.n;
        let action = match &step.action {
            StepAction::NewRun(raw) => Action::NewRun(resolve_writes(p, n, "newRun", raw)?),
            StepAction::Engine(raw) => Action::Engine(resolve_writes(p, n, "engine", raw)?),
            StepAction::End => Action::End,
            StepAction::Event(name) => {
                if lute_manifest::snapshot::BUILTIN_LIFECYCLE_EVENTS.contains(&name.as_str()) {
                    return Err(format!(
                        "step {n}: `{name}` is a quest lifecycle event — the quest runner fires it \
                         on a transition; it cannot be fired from a script"
                    ));
                }
                if !p.world_events.contains(name) {
                    let hint = if p.occasions.contains_key(name) || answered.contains(name.as_str())
                    {
                        format!(" — `{name}` is an occasion; raise it with `occasion: {name}`")
                    } else if p.world_events.is_empty() {
                        " (no plugin declares world events)".to_string()
                    } else {
                        let declared: Vec<&str> =
                            p.world_events.iter().map(String::as_str).collect();
                        format!(" (declared: {})", declared.join(", "))
                    };
                    return Err(format!(
                        "step {n}: `event: {name}` names no declared world event{hint}"
                    ));
                }
                Action::Event(name.clone())
            }
            StepAction::Occasion {
                occasion,
                target,
                pick,
                choose,
                payload,
                writes,
            } => {
                let pick = entry_pick(p, pick);
                plan_occasion(p, &answered, n, occasion, target.as_deref(), pick.as_ref())?;
                let payload = lute_trace::exec::session::typed_payload(p, occasion, payload)
                    .map_err(|e| format!("step {n}: {e}"))?;
                let writes = match writes {
                    Some(raw) => Some(resolve_writes(p, n, "engine", raw)?),
                    None => None,
                };
                Action::Occasion {
                    occasion: occasion.clone(),
                    target: target.clone(),
                    pick,
                    choose: choose.clone(),
                    payload,
                    writes,
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
                    return Err(format!(
                        "step {n}: `advance:` moves the declared clock, and no schema of this \
                         project declares a `clock:`"
                    ));
                };
                let writes = resolve_writes(p, n, "engine", writes)?;
                if let Some((path, _)) = writes
                    .state
                    .iter()
                    .find(|(path, _)| *path == clock.day || clock.slot.as_ref() == Some(path))
                {
                    return Err(format!(
                        "step {n}: `engine:` writes `{path}`, which the `advance:` beside it \
                         moves — write the clock in a step of its own, or let `advance:` move it"
                    ));
                }
                let by = resolve_advance(clock, n, by)?;
                let pick = &entry_pick(p, pick);
                let raise = clock.raises();
                // dsl 0.27.0 (T3-8): `pick` answers the `select: all` slot
                // raise where the clock stops; `choose` and the selection
                // expectations judge whatever the step raises — its
                // midnights' `dayEnd` / `dayStart` too.
                if raise.slot.is_none() && pick.is_some() {
                    return Err(format!(
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
                    return Err(format!(
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
                        return Err(format!(
                            "step {n}: the clock raises `{occasion}`, which is declared \
                             `target: true` — an `advance:` raises it for no target"
                        ));
                    }
                    // Shape-only vocabulary: an occasion no beat answers
                    // and no objective is judged at is raised to nobody.
                    if !p.occasions.is_empty()
                        || answered.contains(occasion.as_str())
                        || pick.is_some()
                    {
                        plan_occasion(p, &answered, n, occasion, None, pick)?;
                    }
                }
                Action::Advance {
                    by,
                    writes,
                    raise,
                    pick: pick.clone(),
                    choose: choose.clone(),
                }
            }
        };
        let mut segments = Vec::with_capacity(step.segments.len());
        for seg in &step.segments {
            if let Some((_, scope)) = scopes.iter().find(|(s, _)| Arc::ptr_eq(s, seg)) {
                segments.push(scope.clone());
                continue;
            }
            let scope = Arc::new(Scope {
                choose: seg.choose.clone(),
                bridges: resolve_bridges(p, &format!("step {n}: {}", seg.include), &seg.bridges)?,
            });
            scopes.push((seg.clone(), scope.clone()));
            segments.push(scope);
        }
        plan.push(Step {
            n,
            label: step.label.clone(),
            repeat: step.repeat,
            action,
            bridges: resolve_bridges(p, &format!("step {n}"), &step.bridges)?,
            segments,
        });
    }
    Ok(plan)
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

/// [`plan_steps`] with its usage error located ([`locate_step_error`]),
/// else prefixed with `script_path`.
fn plan_located(
    p: &ExecProject,
    steps: &[ScriptStep],
    script_path: &Path,
) -> Result<Vec<Step>, String> {
    plan_steps(p, steps).map_err(|e| {
        locate_step_error(steps, &e).unwrap_or_else(|| format!("{}: {e}", script_path.display()))
    })
}

/// Round-5 T3-13: what a step `expect:` names that the project must have —
/// a beat id (`winner`, `offered`, `notOffered`, `presented`; an entry by
/// its `<document id>.<entry id>` alias too) and a clock position (`clock:
/// slot` a declared slot, `weekday` a `week.labels` label or a weekday
/// number) — checked before the play runs, with did-you-mean. A typo is a
/// usage error (exit 2), not a miss.
fn check_expect_names(p: &ExecProject, script: &PlayScript) -> Result<(), String> {
    let near = |needle: &str, known: &[&str]| {
        lute_manifest::suggest::nearest(needle, known.iter().copied(), 2)
            .map(|k| format!(" — did you mean `{k}`?"))
            .unwrap_or_default()
    };
    let beat_ids: Vec<&str> = p
        .index
        .beats
        .iter()
        .map(|b| b.id.as_str())
        .chain(p.entry_aliases.keys().map(String::as_str))
        .collect();
    for (n, _, expect) in &script.step_expects {
        let n = *n;
        for key in ["winner", "offered", "notOffered", "presented"] {
            let ids: Vec<String> = match expect.get(key) {
                Some(serde_yaml::Value::Sequence(items)) => {
                    items.iter().filter_map(scalar_text).collect()
                }
                Some(v) => scalar_text(v).into_iter().collect(),
                None => continue,
            };
            for id in ids {
                if (key == "winner" && id == "none") || beat_ids.contains(&id.as_str()) {
                    continue;
                }
                return Err(format!(
                    "step {n}: `expect.{key}` names `{id}`, which no beat of the project has{}",
                    near(&id, &beat_ids)
                ));
            }
        }
        let Some(serde_yaml::Value::Mapping(clock_want)) = expect.get("clock") else {
            continue;
        };
        let Some(clock) = &p.index.clock else {
            return Err(format!(
                "step {n}: `expect.clock` judges the declared clock, and no schema of this \
                 project declares a `clock:`"
            ));
        };
        if let Some(slot) = clock_want.get("slot").and_then(serde_yaml::Value::as_str) {
            let slots: Vec<&str> = clock.slots.iter().map(String::as_str).collect();
            if clock.slot.is_none() {
                return Err(format!(
                    "step {n}: `expect.clock.slot: {slot}` — the clock is day-granular (it \
                     declares no `slots:`)"
                ));
            }
            if !slots.contains(&slot) {
                return Err(format!(
                    "step {n}: `expect.clock.slot: {slot}` names no slot of the clock{} (slots: {})",
                    near(slot, &slots),
                    slots.join(", ")
                ));
            }
        }
        if let Some(want) = clock_want.get("weekday") {
            let Some(week) = clock.week.as_ref() else {
                return Err(format!(
                    "step {n}: `expect.clock.weekday` — the clock declares no `week:`"
                ));
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
                return Err(format!(
                    "step {n}: `expect.clock.weekday: {text}` names no weekday{} — a weekday is \
                     a number 0..{}{named}",
                    near(&text, &labels),
                    week.length.saturating_sub(1)
                ));
            }
        }
    }
    Ok(())
}

/// Plan `script`'s steps over `project` and seed its save (exit 2 with the
/// usage error, prefixed with the script path).
pub(super) fn plan_script(
    project: &ExecProject,
    script: &PlayScript,
    script_path: &Path,
    no_derive: bool,
) -> Result<(Vec<Step>, World), (ExitCode, String)> {
    let at = |e: String| (ExitCode::from(2), format!("{}: {e}", script_path.display()));
    let usage = |e: String| (ExitCode::from(2), e);
    let plan = plan_located(project, &script.steps, script_path).map_err(usage)?;
    check_expect_names(project, script)
        .map_err(|e| usage(locate_step_error(&script.steps, &e).unwrap_or_else(|| at(e).1)))?;
    let mut world = seed_world(
        project,
        &WorldSeed {
            surfaces: &script.surfaces,
            save: &script.save,
            derive: script.derive,
        },
    )
    .map_err(at)?;
    if no_derive {
        world.derive = Some(false);
    }
    Ok((plan, world))
}
