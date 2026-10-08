use super::*;


/// The index of the `--until` step in `steps`: a 1-based step number or a
/// step `label:`.
pub(super) fn until_index(steps: &[ScriptStep], until: &str) -> Result<usize, String> {
    if let Ok(n) = until.trim().parse::<usize>() {
        return steps
            .iter()
            .position(|s| s.n == n)
            .ok_or_else(|| format!("`--until {until}`: the script has {} step(s)", steps.len()));
    }
    steps
        .iter()
        .position(|s| s.label.as_deref() == Some(until))
        .ok_or_else(|| {
            let labels = steps.iter().filter_map(|s| s.label.as_deref());
            let hint = lute_manifest::suggest::nearest(until, labels, 3)
                .map(|l| format!(" — did you mean `{l}`?"))
                .unwrap_or_default();
            format!("`--until {until}` names no step number or `label:` of the script{hint}")
        })
}

/// The world every cell starts from: the script's save, then its steps
/// replayed as `lute play` plays them — all of them, or those before the
/// `--until` step. A replay that halts is a usage error: the calendar
/// cannot say what a route that does not play reaches.
pub(super) fn start_world(
    p: &ExecProject,
    save: &PlayScript,
    script: Option<&Path>,
    until: Option<&str>,
) -> Result<(World, Origin), String> {
    let at = |e: String| match script {
        Some(s) => format!("{}: {e}", s.display()),
        None => e,
    };
    let w = seed_world(
        p,
        &WorldSeed {
            surfaces: &save.surfaces,
            save: &save.save,
            derive: save.derive,
        },
    )
    .map_err(|errs| super::super::plan::seed_errors(save, &errs).join("\n"))?;
    let stop = match until {
        Some(u) => until_index(&save.steps, u).map_err(at)?,
        None => save.steps.len(),
    };
    let origin = Origin {
        script: script.map(|s| s.display().to_string()),
        replayed: stop,
        until: until.map(|_| (save.steps[stop].n, save.steps[stop].label.clone())),
        clock_start: clock_at(p, &w),
    };
    if stop == 0 {
        return Ok((w, origin));
    }
    let plan = plan_steps(p, &save.steps[..stop]).map_err(|errs| {
        errs.into_iter()
            .map(|e| super::super::locate_step_error(&save.steps, &e).unwrap_or_else(|| at(e)))
            .collect::<Vec<_>>()
            .join("\n")
    })?;
    let play = execute(save, &plan, Session::resume(p, w));
    match play.outcome {
        Ok(_) => Ok((play.world, origin)),
        Err(h) => Err(at(format!(
            "replaying the script's steps for the calendar halted — {}",
            h.message()
        ))),
    }
}

/// `--where`: whether `cel` holds over the cell's world. Unknown is an
/// error — a cell is never dropped (or kept) on a guess.
pub(super) fn holds_at(p: &ExecProject, w: &World, cel: &str) -> Result<bool, String> {
    let Some(cond) = lute_trace::lowered(cel) else {
        return Err(describe_atoms(&[]));
    };
    let mut eval = w.evaluator(&p.eval_json).with_visited(&w.visited);
    eval.eval_guard(&cond).map_err(|atoms| describe_atoms(&atoms))
}

