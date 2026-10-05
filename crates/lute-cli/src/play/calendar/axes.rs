use super::*;


/// The most cells one invocation evaluates — a typo'd range (`1..70000`)
/// is refused rather than ground through.
pub(super) const MAX_CELLS: usize = 10_000;

/// Every axis kind the calendar applies — the usage error for an axis it
/// cannot apply lists them.
pub(crate) const AXIS_KINDS: &str = "a declared state path (`run.day=1..7`), \
     every member of a `per:` family (`run.aff.*=6,7`) or the member another axis names \
     (`run.aff[run.route]=6,7`), \
     `quest.<id>.state=<status>,…`, `quest.<id>.objectives.<oid>.done=true,false`, \
     `holds(<fact>)=true,false`, `visited('<scene or bundle-beat id>')=true,false`, \
     `clock[=<d1>..<d2>]` (every slot of those days, in order)";

/// dsl 0.24.0 §1: the axis over the declared clock — `clock=d1..d2` (or a
/// day list), every slot of each day in clock order; bare `clock` is one
/// week from day 1 (day 1 alone without a `week:`).
pub(super) const CLOCK_AXIS: &str = "clock";

/// Split `s` at every `sep` outside parentheses and quotes, so a
/// `holds(at(a, b))` or `visited('x')` axis path stays whole.
pub(super) fn split_top(s: &str, sep: char) -> Vec<&str> {
    let (mut depth, mut quote, mut start) = (0i32, None::<char>, 0);
    let mut out = Vec::new();
    for (i, c) in s.char_indices() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None if c == '\'' || c == '"' => quote = Some(c),
            None if c == '(' => depth += 1,
            None if c == ')' => depth -= 1,
            None if c == sep && depth == 0 => {
                out.push(&s[start..i]);
                start = i + c.len_utf8();
            }
            None => {}
        }
    }
    out.push(&s[start..]);
    out
}

/// clap `value_parser` for `--axis <path>=<values>`: an inclusive integer
/// range `lo..hi` or a comma list `a,b,c`. The path may be a
/// `holds(<fact>)` (its commas are the fact's, before the `=`).
pub(crate) fn parse_axis_flag(raw: &str) -> Result<(String, Vec<String>), String> {
    if raw.trim() == CLOCK_AXIS {
        return Ok((CLOCK_AXIS.to_string(), Vec::new()));
    }
    let (path, spec) = raw
        .split_once('=')
        .ok_or_else(|| format!("expected <path>=<lo>..<hi> or <path>=<a>,<b>,…, got `{raw}`"))?;
    let path = path.trim();
    if path.is_empty() {
        return Err(format!("`{raw}` names no state path before `=`"));
    }
    let spec = spec.trim();
    let values: Vec<String> = match spec.split_once("..") {
        Some((lo, hi)) => {
            let bound = |s: &str| {
                s.trim()
                    .parse::<i64>()
                    .map_err(|_| format!("`{spec}`: a range's bounds are integers (`1..7`)"))
            };
            let (lo, hi) = (bound(lo)?, bound(hi)?);
            if lo > hi {
                return Err(format!("`{spec}`: the range is empty ({lo} > {hi})"));
            }
            if hi.abs_diff(lo) >= MAX_CELLS as u64 {
                return Err(format!("`{spec}`: more than {MAX_CELLS} values"));
            }
            (lo..=hi).map(|n| n.to_string()).collect()
        }
        None => spec.split(',').map(|v| v.trim().to_string()).collect(),
    };
    if values.iter().any(String::is_empty) {
        return Err(format!("`{raw}` has an empty value"));
    }
    // `run.visits["lab-b2"]` names the member `run.visits.lab-b2` does.
    Ok((lute_trace::state_key(path), values))
}

/// How an axis value reaches a cell.
pub(super) enum Apply {
    /// A declared state path, written as an `engine:` step writes it.
    State,
    /// `quest.<id>.state`: the quest's status, seeded as a save's `quests:`
    /// entry — a plain state write would be overwritten by the quest
    /// lifecycle the cell settles.
    Quest(String),
    /// `holds(<fact>)`: a base fact asserted (`true`) or retracted (`false`).
    Fact(Fact),
    /// `visited('<id>')`: a scene or bundle-beat id in (`true`) or out of
    /// (`false`) the visited set — save state, as a save's `visited:`.
    Visited(String),
    /// dsl 0.24.0 §1: `clock`: a position on the declared clock (the value
    /// is its `clock.index`), written to the clock's day and slot paths.
    Clock,
    /// `<family>.*`: every member of a `per:` state family (dsl 0.24.0 §3),
    /// each written as [`Apply::State`] writes one path.
    Family {
        family: String,
        members: Vec<String>,
    },
    /// `<family>[<axis>]`: the one member of a `per:` family the value of
    /// another axis (`by`, its index) names at the cell; the other members
    /// keep what the cell's world holds.
    Tied {
        family: String,
        members: Vec<String>,
        by: usize,
    },
}

impl Apply {
    /// A family axis's family path and member names.
    pub(super) fn family(&self) -> Option<(&str, &[String])> {
        match self {
            Apply::Family { family, members }
            | Apply::Tied {
                family, members, ..
            } => Some((family, members)),
            _ => None,
        }
    }
}

/// The id of a `visited('<id>')` axis path (`'…'`, `"…"` or bare).
pub(super) fn visited_id(path: &str) -> Option<&str> {
    let inner = path.strip_prefix("visited(")?.strip_suffix(')')?.trim();
    let unquoted = ['\'', '"']
        .iter()
        .find_map(|q| inner.strip_prefix(*q)?.strip_suffix(*q))
        .unwrap_or(inner);
    Some(unquoted.trim())
}

/// One axis, its values resolved.
pub(super) struct Axis {
    pub(super) path: String,
    pub(super) apply: Apply,
    pub(super) values: Vec<(String, Value)>,
}

/// `quest.<id>.objectives.<oid>.done` — a quest path the resumed lifecycle
/// honours as written (a save's objective progress).
pub(super) fn is_objective_done(path: &str) -> bool {
    let parts: Vec<&str> = path.split('.').collect();
    matches!(parts.as_slice(), ["quest", id, "objectives", oid, "done"] if !id.is_empty() && !oid.is_empty())
}

/// The declared path a written one is typed by: a save made after a run
/// ended carries `prev.run.*`, typed by the `run.*` path it mirrors (dsl
/// 0.23.0 §6).
pub(super) fn declared_path(path: &str) -> &str {
    match path.strip_prefix("prev.") {
        Some(run) if run.starts_with("run.") => run,
        _ => path,
    }
}

/// dsl 0.24.0 §3: the `per:` state family `path` (a declared path) names —
/// its kind and members. The compiled state table carries one path per
/// member (`run.aff.ren`, …), so the family is the closed entity kind every
/// member of which has a declared `<path>.<member>` — the largest such kind,
/// since a kind's sub-kinds are covered with it.
pub(super) fn state_family<'a>(p: &'a ExecProject, path: &str) -> Option<(&'a str, &'a [String])> {
    p.kinds
        .iter()
        .rev()
        .filter_map(|(kind, decl)| match &decl.shape {
            KindShape::Members(ms) if !ms.is_empty() => Some((kind.as_str(), ms.as_slice())),
            _ => None,
        })
        .filter(|(_, ms)| {
            ms.iter()
                .all(|m| p.state_table.contains_key(&format!("{path}.{m}")))
        })
        .max_by_key(|(_, ms)| ms.len())
}

/// An axis over a `per:` family: `<family>.*` (`None`) or
/// `<family>[<axis>]` (the indexing axis's path).
pub(super) fn family_form(path: &str) -> Option<(&str, Option<&str>)> {
    if let Some(family) = path.strip_suffix(".*") {
        return Some((family.trim(), None));
    }
    let (family, rest) = path.split_once('[')?;
    Some((family.trim(), Some(rest.strip_suffix(']')?.trim())))
}

/// Resolve `--axis <family>.*` / `<family>[<by>]`; `axes` are every
/// `--axis` as given, the one a tied axis names among them.
pub(super) fn resolve_family_axis(
    p: &ExecProject,
    family: &str,
    by: Option<&str>,
    axes: &[(String, Vec<String>)],
) -> Result<Apply, String> {
    let Some((kind, members)) = state_family(p, declared_path(family)) else {
        let families: BTreeSet<&str> = p
            .state_table
            .keys()
            .filter_map(|k| k.rsplit_once('.').map(|(f, _)| f))
            .filter(|f| state_family(p, f).is_some())
            .collect();
        let hint = lute_manifest::suggest::nearest(family, families.iter().copied(), 3)
            .map(|k| format!(" — did you mean `{k}`?"))
            .unwrap_or_default();
        return Err(format!(
            "`{family}` is no `per:` state family of this project{hint}"
        ));
    };
    let members = members.to_vec();
    let Some(by) = by else {
        return Ok(Apply::Family {
            family: family.to_string(),
            members,
        });
    };
    // Every other axis: the one a tie can name (never this axis itself).
    let own = format!("{family}[{by}]");
    let others: Vec<&str> = axes
        .iter()
        .map(|(a, _)| a.as_str())
        .filter(|a| *a != own && *a != CLOCK_AXIS)
        .collect();
    let index = axes
        .iter()
        .position(|(a, _)| a == by)
        .filter(|_| by != CLOCK_AXIS)
        .ok_or_else(|| {
            let hint = lute_manifest::suggest::nearest(by, others.iter().copied(), 3)
                .map(|k| format!(" — did you mean `{k}`?"))
                .unwrap_or_default();
            let names = if others.is_empty() {
                "none".to_string()
            } else {
                others.join(", ")
            };
            format!(
                "`{by}` is no `--axis` of this calendar{hint} (other axes: {names}) — \
                 `{family}[<axis>]` sets, at each cell, the member of `{kind}` that another \
                 axis's value names"
            )
        })?;
    // OT-F-12: a value naming no member (a route outside the family) sets
    // nothing at its cells; only an axis naming no member at all is no tie.
    if !axes[index].1.iter().any(|v| members.contains(v)) {
        return Err(format!(
            "`--axis {by}` takes no member of `{kind}` ({}) — `{family}[{by}]` sets, at each \
             cell, the member the value of `{by}` names",
            members.join(", ")
        ));
    }
    Ok(Apply::Tied {
        family: family.to_string(),
        members,
        by: index,
    })
}

/// Resolve one `--axis` against the project; `axes` are every `--axis` as
/// given (what a tied family axis names). `Err` is the usage error.
pub(super) fn resolve_axis(
    p: &ExecProject,
    path: &str,
    values: &[String],
    axes: &[(String, Vec<String>)],
) -> Result<Axis, String> {
    let at = |e: String| format!("`--axis {path}`: {e}");
    if path == CLOCK_AXIS {
        return resolve_clock_axis(p, values).map_err(at);
    }
    let unsupported = |why: String| at(format!("{why}; an axis is one of: {AXIS_KINDS}"));
    let apply = if let Some(inner) = path
        .strip_prefix("holds(")
        .and_then(|s| s.strip_suffix(')'))
    {
        Apply::Fact(resolve_fact(p, inner, None).map_err(|e| at(format!("`{inner}` {e}")))?)
    } else if let Some(id) = visited_id(path) {
        if !p.scene_ids.contains(id) {
            return Err(at(unknown_id(
                "it",
                id,
                "scene or bundle beat",
                p.scene_ids.iter().map(String::as_str),
            )));
        }
        Apply::Visited(id.to_string())
    } else if let Some(id) = quest_state_id(path) {
        if !p.quest_objectives.contains_key(id) {
            return Err(at(unknown_id(
                "it",
                id,
                "quest",
                p.quest_objectives.keys().map(String::as_str),
            )));
        }
        Apply::Quest(id.to_string())
    } else if path.starts_with("quest.") && !is_objective_done(path) {
        return Err(at(format!(
            "`{path}` is the quest lifecycle's own bookkeeping and cannot be set per cell — \
             an axis over a quest is `quest.<id>.state` (its status) or \
             `quest.<id>.objectives.<oid>.done`"
        )));
    } else if let Some((family, by)) = family_form(path) {
        resolve_family_axis(p, family, by, axes).map_err(at)?
    } else if path.contains('(') {
        return Err(unsupported(format!(
            "`{path}` is no axis the calendar can apply"
        )));
    } else {
        let declared = declared_path(path);
        if !path.starts_with("scene.")
            && entry_flag(path).is_none()
            && !p.state_table.contains_key(declared)
        {
            // dsl 0.24.0 §3: a family is no one value — name how the axis
            // reaches its members.
            if let Some((kind, members)) = state_family(p, declared) {
                let tie = axes
                    .iter()
                    .find(|(a, vs)| {
                        a != path && !vs.is_empty() && vs.iter().all(|v| members.contains(v))
                    })
                    .map_or("<axis>", |(a, _)| a.as_str());
                return Err(at(format!(
                    "`{path}` is a `per: {kind}` family — name a member (`{path}.{}`), all of \
                     them (`{path}.*`), or tie it to an axis (`{path}[{tie}]`)",
                    members[0]
                )));
            }
            let hint =
                lute_manifest::suggest::nearest(path, p.state_table.keys().map(String::as_str), 3)
                    .map(|k| format!(" — did you mean `{k}`?"))
                    .unwrap_or_default();
            return Err(unsupported(format!(
                "`{path}` is not a declared state path in this project{hint}"
            )));
        }
        Apply::State
    };
    let mut typed = Vec::with_capacity(values.len());
    for v in values {
        let value = match &apply {
            Apply::Fact(_) | Apply::Visited(_) => match v.as_str() {
                "true" => Value::Bool(true),
                "false" => Value::Bool(false),
                _ => {
                    let (kind, yes) = match &apply {
                        Apply::Fact(_) => ("holds(…)", "asserted"),
                        _ => ("visited(…)", "visited"),
                    };
                    return Err(at(format!(
                        "a `{kind}` axis takes `true` ({yes}) and `false` (absent), not `{v}`"
                    )));
                }
            },
            Apply::Quest(_) if QUEST_STATES.contains(&v.as_str()) => Value::Str(v.clone()),
            Apply::Quest(_) => {
                return Err(at(format!(
                    "`{v}` is not a quest status ({})",
                    QUEST_STATES.join(", ")
                )))
            }
            Apply::State => resolve_state(p, path, v).map_err(|e| at(format!("`{path}` {e}")))?,
            Apply::Family { family, members }
            | Apply::Tied {
                family, members, ..
            } => resolve_state(p, &format!("{family}.{}", members[0]), v)
                .map_err(|e| at(format!("`{path}` {e}")))?,
            Apply::Clock => unreachable!("a clock axis is resolved by `resolve_clock_axis`"),
        };
        typed.push((v.clone(), value));
    }
    Ok(Axis {
        path: path.to_string(),
        apply,
        values: typed,
    })
}

/// dsl 0.24.0 §1: `--axis clock[=<days>]` — every slot of each day, in
/// clock order; each value is the position's `clock.index`, its text `day
/// slot` (`1 Mon morning` with week labels).
pub(super) fn resolve_clock_axis(p: &ExecProject, days: &[String]) -> Result<Axis, String> {
    let Some(clock) = &p.index.clock else {
        return Err(format!(
            "no schema of this project declares a `clock:`; an axis is one of: \
             {AXIS_KINDS}"
        ));
    };
    // dsl 0.27.0 §4: a finite clock has no position past its last one.
    let last = clock.last_at();
    let days: Vec<i64> = if days.is_empty() {
        let length = clock
            .week
            .as_ref()
            .map_or(1, |w| i64::from(w.length.max(1)));
        (1..=last.map_or(length, |l| length.min(l.day))).collect()
    } else {
        days.iter()
            .map(|d| {
                let day = d.parse::<i64>().ok().filter(|d| *d >= 1).ok_or_else(|| {
                    format!("`{d}` is not a day — a clock axis takes days ≥ 1 (`clock=1..7`)")
                })?;
                match last.filter(|l| day > l.day) {
                    Some(l) => Err(format!(
                        "day {day} is past the clock's last position ({}) — the clock ends \
                         there",
                        clock.describe(l)
                    )),
                    None => Ok(day),
                }
            })
            .collect::<Result<_, _>>()?
    };
    let mut values = Vec::with_capacity(days.len() * clock.slot_count());
    for day in days {
        for slot in 0..clock.slot_count() {
            let at = lute_manifest::clock::ClockAt { day, slot };
            if clock.is_past_end(at) {
                break;
            }
            let label = clock
                .weekday_label(day)
                .map(|l| format!(" {l}"))
                .unwrap_or_default();
            let name = clock
                .slot_name(slot)
                .map(|n| format!(" {n}"))
                .unwrap_or_default();
            values.push((
                format!("{day}{label}{name}"),
                Value::Int(clock.index(at) as i64),
            ));
        }
    }
    Ok(Axis {
        path: CLOCK_AXIS.to_string(),
        apply: Apply::Clock,
        values,
    })
}

/// The position of a clock axis value (its `clock.index`).
pub(super) fn clock_axis_at(
    clock: &lute_manifest::clock::ClockDecl,
    value: &Value,
) -> lute_manifest::clock::ClockAt {
    let Value::Int(index) = value else {
        unreachable!("a clock axis value is its index")
    };
    let len = clock.slot_count() as i64;
    let index = *index as i64;
    lute_manifest::clock::ClockAt {
        day: index.div_euclid(len) + 1,
        slot: index.rem_euclid(len) as usize,
    }
}

/// The state paths a declared-path axis writes at the cell whose axis value
/// indices are `picks`: its path, every member of a `<family>.*`, or the
/// member a `<family>[<axis>]`'s indexing axis names there (none when that
/// value names no member, [`tied_outside`]). Empty for the other kinds.
pub(super) fn written_paths(axis: &Axis, axes: &[Axis], picks: &[usize]) -> Vec<String> {
    match &axis.apply {
        Apply::State => vec![axis.path.clone()],
        Apply::Family { family, members } => {
            members.iter().map(|m| format!("{family}.{m}")).collect()
        }
        Apply::Tied { family, by, .. } if tied_outside(axis, axes, picks).is_none() => {
            vec![format!("{family}.{}", axes[*by].values[picks[*by]].0)]
        }
        Apply::Tied { .. }
        | Apply::Quest(_)
        | Apply::Fact(_)
        | Apply::Visited(_)
        | Apply::Clock => Vec::new(),
    }
}

/// OT-F-12: the value of a `<family>[<axis>]`'s indexing axis at `picks`
/// when it names no member of the family (a route outside it) — the tied
/// axis sets nothing at that cell.
pub(super) fn tied_outside<'a>(axis: &Axis, axes: &'a [Axis], picks: &[usize]) -> Option<&'a str> {
    let Apply::Tied { members, by, .. } = &axis.apply else {
        return None;
    };
    let v = &axes[*by].values[picks[*by]].0;
    (!members.contains(v)).then_some(v.as_str())
}

/// Write one axis value into a cell's world; `paths` are the state paths
/// it writes there ([`written_paths`]).
pub(super) fn apply_axis(
    p: &ExecProject,
    w: &mut World,
    axis: &Axis,
    text: &str,
    value: &Value,
    paths: &[String],
) {
    match &axis.apply {
        Apply::State | Apply::Family { .. } | Apply::Tied { .. } => {
            for path in paths {
                w.state.insert(path.clone(), value.clone());
            }
        }
        Apply::Quest(id) => {
            // An `unset`/`active` status keeps no progress a replayed route
            // made: the axis names the status, not the route's objectives.
            if text == "unset" || text == "active" {
                let prefix = format!("quest.{id}.objectives.");
                for (k, v) in &mut w.state {
                    if k.starts_with(&prefix) && k.ends_with(".done") {
                        *v = Value::Bool(false);
                    }
                }
                let owner = format!("{id}.");
                w.failed_objectives.retain(|k| !k.starts_with(&owner));
            }
            if let Err(e) = seed_quest(p, w, "--axis", id, text) {
                unreachable!("the axis was resolved against the project: {e}");
            }
        }
        Apply::Fact(f) => {
            if value == &Value::Bool(true) {
                w.facts.insert(f.clone());
            } else {
                w.facts.remove(f);
            }
        }
        Apply::Visited(id) => {
            if value == &Value::Bool(true) {
                w.visited.insert(id.clone());
            } else {
                w.visited.remove(id);
            }
        }
        Apply::Clock => {
            let clock = p
                .index
                .clock
                .as_ref()
                .expect("a clock axis was resolved against a clock");
            let at = clock_axis_at(clock, value);
            w.state.insert(clock.day.clone(), Value::Int(at.day));
            if let (Some(path), Some(name)) = (&clock.slot, clock.slot_name(at.slot)) {
                w.state.insert(path.clone(), Value::Str(name.to_string()));
            }
            refresh_clock(p, w);
        }
    }
}

/// What the settle did to an axis value the cell was given — a quest
/// handler's write, a seeded quest status the lifecycle moved on.
pub(super) fn settled_away(
    p: &ExecProject,
    w: &World,
    axis: &Axis,
    text: &str,
    value: &Value,
    paths: &[String],
) -> Option<String> {
    match &axis.apply {
        Apply::State | Apply::Family { .. } | Apply::Tied { .. } => {
            let moved: Vec<String> = paths
                .iter()
                .filter_map(|path| {
                    let now = w.state.get(path)?;
                    (now != value).then(|| format!("{path} settled to {}", value_to_json(now)))
                })
                .collect();
            (!moved.is_empty()).then(|| moved.join("; "))
        }
        Apply::Quest(id) => {
            let now = w.quests.get(id).map_or("unset", String::as_str);
            (now != text).then(|| format!("{} settled to {now}", axis.path))
        }
        Apply::Fact(_) | Apply::Visited(_) => None,
        Apply::Clock => {
            let clock = p.index.clock.as_ref()?;
            let now = clock_at(p, w)?;
            (now != clock_axis_at(clock, value))
                .then(|| format!("clock settled to {}", clock.describe(now)))
        }
    }
}

