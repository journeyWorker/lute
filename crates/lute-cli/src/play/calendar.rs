//! `lute calendar <dir> [--axis <axis>=<values>]… [--occasion
//! O[@<axis>[=<value>],…]]… [--target T]… [--facts <relation>]… [--script
//! route.play.yaml [--until <step>]] [--where <cel>] [--json | --csv]` (dsl
//! 0.23.0 §1, D-A; 0.23.1; 0.24.0).
//!
//! The calendar is a tool over play, not a schedule file. Every cell of the
//! axes' product starts from one world: the `--script`'s save (0.22.0 §3)
//! with its `steps:` replayed exactly as `lute play` plays them — up to the
//! `--until` step, which is not played — or the declared defaults. The
//! cell's values are then written into that world, the quest lifecycle
//! settles, `--where` drops the cell when it does not hold, and every listed
//! occasion (and target) is evaluated with play's own eligibility,
//! [`super::eligible_at`]: a cell reads exactly what `lute play` would
//! decide there. Cells are independent — no presentation happens, so
//! nothing one cell does is seen by the next.
//!
//! Axis kinds ([`AXIS_KINDS`], one [`Apply`] arm each): a declared state
//! path, written as an `engine:` step writes it; `quest.<id>.state`, the
//! quest's status seeded as a save's `quests:` does;
//! `quest.<id>.objectives.<oid>.done`, objective progress as a save keeps
//! it; `holds(<fact>)=true,false`, a base fact asserted or retracted;
//! `visited('<id>')=true,false`, a scene or bundle-beat id added to or
//! removed from the visited set a save's `visited:` seeds (what `after:` and
//! CEL `visited()` read). An axis the calendar cannot apply is a usage
//! error naming the kinds, never silently dropped.
//!
//! Per-occasion axes: `--occasion dayEnd@run.day` varies only `run.day` for
//! `dayEnd` — the occasion is evaluated in the cells where every other axis
//! is at its first value (once per `run.day` value) and is blank elsewhere;
//! `dayEnd@run.day,run.slot=night` holds `run.slot` at `night` instead. An
//! occasion the engine raises once per day is read once per day.
//!
//! Presence: `--facts <relation>` lists, per cell, the facts of that
//! relation that hold once the cell has settled (the runner's fixpoint, as
//! play's end-of-play `facts:` judge them) — in text one table per relation,
//! a row per first argument and a column per cell.
//!
//! Output: a grid (text), `--json`, or `--csv`. At the end: beats that were
//! a candidate somewhere but eligible in no cell, with why; and beats
//! eligible somewhere but presented in no cell, with the beats presented
//! over them (`?` where an unknown `when` decided the cell).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::Path;
use std::process::ExitCode;

use lute_compile::index::IndexBeat;
use lute_manifest::schema::OccasionSelect;
use lute_trace::Value;
use serde_json::{json, Value as Json};

use super::{
    advance_quests, compile_project, deciding_unknown, describe_atoms, domain_members,
    eligible_at, entry_flag, execute, is_candidate, kind_label, parse_script_with, plan_steps,
    presented, quest_state_id, render_fact, resolve_fact, resolve_state, seed_quest, seed_world,
    unknown_id, value_to_json, PlayScript, Project, ScriptStep, Verdict, World, QUEST_STATES,
};
use crate::runner::{Fact, Runner};

/// The most cells one invocation evaluates — a typo'd range (`1..70000`)
/// is refused rather than ground through.
const MAX_CELLS: usize = 10_000;

/// Every axis kind the calendar applies — the usage error for an axis it
/// cannot apply lists them.
pub(crate) const AXIS_KINDS: &str = "a declared state path (`run.day=1..7`), \
     `quest.<id>.state=<status>,…`, `quest.<id>.objectives.<oid>.done=true,false`, \
     `holds(<fact>)=true,false`, `visited('<scene or bundle-beat id>')=true,false`, \
     `clock[=<d1>..<d2>]` (every slot of those days, in order)";

/// dsl 0.24.0 §1: the axis over the declared clock — `clock=d1..d2` (or a
/// day list), every slot of each day in clock order; bare `clock` is one
/// week from day 1 (day 1 alone without a `week:`).
const CLOCK_AXIS: &str = "clock";

/// Split `s` at every `sep` outside parentheses and quotes, so a
/// `holds(at(a, b))` or `visited('x')` axis path stays whole.
fn split_top(s: &str, sep: char) -> Vec<&str> {
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
    Ok((path.to_string(), values))
}

/// How an axis value reaches a cell.
enum Apply {
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
}

/// The id of a `visited('<id>')` axis path (`'…'`, `"…"` or bare).
fn visited_id(path: &str) -> Option<&str> {
    let inner = path.strip_prefix("visited(")?.strip_suffix(')')?.trim();
    let unquoted = ['\'', '"']
        .iter()
        .find_map(|q| inner.strip_prefix(*q)?.strip_suffix(*q))
        .unwrap_or(inner);
    Some(unquoted.trim())
}

/// One axis, its values resolved.
struct Axis {
    path: String,
    apply: Apply,
    values: Vec<(String, Value)>,
}

/// `quest.<id>.objectives.<oid>.done` — a quest path the resumed lifecycle
/// honours as written (a save's objective progress).
fn is_objective_done(path: &str) -> bool {
    let parts: Vec<&str> = path.split('.').collect();
    matches!(parts.as_slice(), ["quest", id, "objectives", oid, "done"] if !id.is_empty() && !oid.is_empty())
}

/// Resolve one `--axis` against the project; `Err` is the usage error.
fn resolve_axis(p: &Project, path: &str, values: &[String]) -> Result<Axis, String> {
    let at = |e: String| format!("`--axis {path}`: {e}");
    if path == CLOCK_AXIS {
        return resolve_clock_axis(p, values).map_err(at);
    }
    let unsupported = |why: String| at(format!("{why}; an axis is one of: {AXIS_KINDS}"));
    let apply = if let Some(inner) = path.strip_prefix("holds(").and_then(|s| s.strip_suffix(')')) {
        Apply::Fact(resolve_fact(p, inner).map_err(|e| at(format!("`{inner}` {e}")))?)
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
    } else if path.contains('(') {
        return Err(unsupported(format!("`{path}` is no axis the calendar can apply")));
    } else {
        let declared = match path.strip_prefix("prev.") {
            Some(run) if run.starts_with("run.") => run,
            _ => path,
        };
        if !path.starts_with("scene.")
            && entry_flag(path).is_none()
            && !p.state_table.contains_key(declared)
        {
            let hint = lute_manifest::suggest::nearest(path, p.state_table.keys().map(String::as_str), 3)
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
fn resolve_clock_axis(p: &Project, days: &[String]) -> Result<Axis, String> {
    let Some(clock) = &p.index.clock else {
        return Err(format!(
            "no schema of this project declares a `clock:` (dsl 0.24.0 §1); an axis is one of: \
             {AXIS_KINDS}"
        ));
    };
    let days: Vec<i64> = if days.is_empty() {
        let length = clock.week.as_ref().map_or(1, |w| i64::from(w.length.max(1)));
        (1..=length).collect()
    } else {
        days.iter()
            .map(|d| {
                d.parse::<i64>()
                    .ok()
                    .filter(|d| *d >= 1)
                    .ok_or_else(|| format!("`{d}` is not a day — a clock axis takes days ≥ 1 (`clock=1..7`)"))
            })
            .collect::<Result<_, _>>()?
    };
    let mut values = Vec::with_capacity(days.len() * clock.slot_count());
    for day in days {
        for slot in 0..clock.slot_count() {
            let at = lute_manifest::clock::ClockAt { day, slot };
            let label = clock.weekday_label(day).map(|l| format!(" {l}")).unwrap_or_default();
            let name = clock.slot_name(slot).map(|n| format!(" {n}")).unwrap_or_default();
            values.push((format!("{day}{label}{name}"), Value::Num(clock.index(at) as f64)));
        }
    }
    Ok(Axis {
        path: CLOCK_AXIS.to_string(),
        apply: Apply::Clock,
        values,
    })
}

/// The position of a clock axis value (its `clock.index`).
fn clock_axis_at(clock: &lute_manifest::clock::ClockDecl, value: &Value) -> lute_manifest::clock::ClockAt {
    let Value::Num(index) = value else { unreachable!("a clock axis value is its index") };
    let len = clock.slot_count() as i64;
    let index = *index as i64;
    lute_manifest::clock::ClockAt {
        day: index.div_euclid(len) + 1,
        slot: index.rem_euclid(len) as usize,
    }
}

/// Write one axis value into a cell's world.
fn apply_axis(p: &Project, w: &mut World, axis: &Axis, text: &str, value: &Value) {
    match &axis.apply {
        Apply::State => {
            w.state.insert(axis.path.clone(), value.clone());
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
            let clock = p.index.clock.as_ref().expect("a clock axis was resolved against a clock");
            let at = clock_axis_at(clock, value);
            w.state.insert(clock.day.clone(), Value::Num(at.day as f64));
            if let (Some(path), Some(name)) = (&clock.slot, clock.slot_name(at.slot)) {
                w.state.insert(path.clone(), Value::Str(name.to_string()));
            }
            super::refresh_clock(p, w);
        }
    }
}

/// What the settle did to an axis value the cell was given — a quest
/// handler's write, a seeded quest status the lifecycle moved on.
fn settled_away(p: &Project, w: &World, axis: &Axis, text: &str, value: &Value) -> Option<String> {
    match &axis.apply {
        Apply::State => {
            let now = w.state.get(&axis.path)?;
            (now != value).then(|| {
                format!(
                    "{} settled to {}",
                    axis.path,
                    value_to_json(now)
                )
            })
        }
        Apply::Quest(id) => {
            let now = w.quests.get(id).map_or("unset", String::as_str);
            (now != text).then(|| format!("{} settled to {now}", axis.path))
        }
        Apply::Fact(_) | Apply::Visited(_) => None,
        Apply::Clock => {
            let clock = p.index.clock.as_ref()?;
            let now = super::clock_at(p, w)?;
            (now != clock_axis_at(clock, value)).then(|| format!("clock settled to {}", clock.describe(now)))
        }
    }
}

/// One evaluated occasion (and target).
struct Column {
    occasion: String,
    target: Option<String>,
    /// A targeted occasion raised for no particular target (its beats name
    /// none): only its untargeted beats are candidates.
    any_target: bool,
    select: OccasionSelect,
    /// `--occasion O@<axes>`: per axis, how the occasion treats it. `None`
    /// for an occasion that varies over every axis.
    pins: Option<Vec<Pin>>,
}

impl Column {
    fn label(&self) -> String {
        match &self.target {
            Some(t) => format!("{}@{t}", self.occasion),
            None if self.any_target => format!("{}@(any)", self.occasion),
            None => self.occasion.clone(),
        }
    }

    /// Whether the column is evaluated at the cell whose axis value indices
    /// are `picks`.
    fn applies(&self, picks: &[usize]) -> bool {
        self.pins.as_ref().is_none_or(|pins| {
            pins.iter().zip(picks).all(|(pin, k)| match pin {
                Pin::Varies => true,
                Pin::At(p) => p == k,
                Pin::Clock { allowed, .. } => allowed.contains(k),
            })
        })
    }
}

/// One `--occasion`: its name and, after `@`, the axes it varies over
/// (`path`) or is held at (`path=value`); every axis it does not name is
/// held at its first value.
struct OccasionSpec<'a> {
    raw: &'a str,
    name: &'a str,
    only: Option<Vec<(&'a str, Option<&'a str>)>>,
}

fn parse_occasion(raw: &str) -> Result<OccasionSpec<'_>, String> {
    let Some((name, list)) = raw.split_once('@') else {
        return Ok(OccasionSpec { raw, name: raw.trim(), only: None });
    };
    let mut only = Vec::new();
    for item in split_top(list, ',') {
        let (path, value) = match split_top(item, '=').as_slice() {
            [path] => (path.trim(), None),
            [path, value] => (path.trim(), Some(value.trim())),
            _ => return Err(format!("`--occasion {raw}`: `{item}` has more than one `=`")),
        };
        if path.is_empty() || value == Some("") {
            return Err(format!(
                "`--occasion {raw}`: expected <occasion>@<axis>[=<value>],…, with no empty item"
            ));
        }
        only.push((path, value));
    }
    Ok(OccasionSpec { raw, name: name.trim(), only: Some(only) })
}

/// How a per-occasion column (`--occasion O@…`) treats one axis.
#[derive(Clone)]
enum Pin {
    /// The occasion varies over the axis.
    Varies,
    /// Held at one value index.
    At(usize),
    /// dsl 0.24.0 §1: the clock axis, varied or held per part
    /// (`O@clock.day`, `O@run.day,run.slot=night`): the value indices whose
    /// position matches, and each part's held value (`None`: it varies).
    Clock {
        allowed: BTreeSet<usize>,
        day: Option<i64>,
        slot: Option<String>,
        /// The clock has a slot part (not a day-granular clock).
        slotted: bool,
    },
}

/// Resolve an [`OccasionSpec`]'s `@` list against the axes into
/// [`Column::pins`]. With a `clock` axis, its parts are named `clock.day` /
/// `clock.slot` or by the clock's own day / slot paths (`run.day`); an
/// unnamed part is held at its first value.
fn occasion_pins(
    spec: &OccasionSpec<'_>,
    axes: &[Axis],
    clock: Option<&lute_manifest::clock::ClockDecl>,
) -> Result<Option<Vec<Pin>>, String> {
    let Some(only) = &spec.only else {
        return Ok(None);
    };
    let raw = spec.raw;
    let mut pins = vec![Pin::At(0); axes.len()];
    let mut named = BTreeSet::new();
    let clock_axis = axes.iter().position(|a| matches!(a.apply, Apply::Clock));
    // Per clock part: `None` unnamed, `Some(None)` varies, `Some(Some(v))` held.
    let (mut day_part, mut slot_part): (Option<Option<&str>>, Option<Option<&str>>) = (None, None);
    for &(path, value) in only {
        let Some(i) = axes.iter().position(|a| a.path == path) else {
            if let (Some(ci), Some(clock)) = (clock_axis, clock) {
                let day = path == "clock.day" || path == clock.day;
                let slot = path == "clock.slot" || clock.slot.as_deref() == Some(path);
                if day || slot {
                    if named.contains(&ci) {
                        return Err(format!("`--occasion {raw}`: `{path}` is part of `clock`, named already"));
                    }
                    if slot && clock.slot.is_none() {
                        return Err(format!(
                            "`--occasion {raw}`: the clock counts whole days — it has no slot to vary or hold"
                        ));
                    }
                    let part = if day { &mut day_part } else { &mut slot_part };
                    if part.replace(value).is_some() {
                        return Err(format!("`--occasion {raw}`: `{path}` is named twice"));
                    }
                    continue;
                }
            }
            let mut paths: Vec<&str> = axes.iter().map(|a| a.path.as_str()).collect();
            if clock_axis.is_some() {
                paths.extend(["clock.day", "clock.slot"]);
            }
            let hint = lute_manifest::suggest::nearest(path, paths.iter().copied(), 3)
                .map(|k| format!(" — did you mean `{k}`?"))
                .unwrap_or_default();
            return Err(format!(
                "`--occasion {raw}`: `{path}` is no `--axis` of this calendar (axes: {}){hint}",
                if paths.is_empty() { "none".to_string() } else { paths.join(", ") }
            ));
        };
        if !named.insert(i) || (Some(i) == clock_axis && (day_part.is_some() || slot_part.is_some())) {
            return Err(format!("`--occasion {raw}`: `{path}` is named twice"));
        }
        pins[i] = match value {
            None => Pin::Varies,
            Some(v) => Pin::At(axes[i].values.iter().position(|(t, _)| t == v).ok_or_else(|| {
                let vals: Vec<&str> = axes[i].values.iter().map(|(t, _)| t.as_str()).collect();
                format!(
                    "`--occasion {raw}`: `{v}` is not a value of `--axis {path}` ({})",
                    vals.join(", ")
                )
            })?),
        };
    }
    if let (Some(ci), Some(clock)) = (clock_axis, clock) {
        if day_part.is_some() || slot_part.is_some() {
            let positions: Vec<lute_manifest::clock::ClockAt> =
                axes[ci].values.iter().map(|(_, v)| clock_axis_at(clock, v)).collect();
            let first = positions[0];
            let day = match day_part {
                None => Some(first.day),
                Some(None) => None,
                Some(Some(v)) => Some(
                    v.parse::<i64>()
                        .ok()
                        .filter(|d| positions.iter().any(|at| at.day == *d))
                        .ok_or_else(|| format!("`--occasion {raw}`: `{v}` is not a day of `--axis clock`"))?,
                ),
            };
            let slot = match slot_part {
                None => Some(first.slot),
                Some(None) => None,
                Some(Some(v)) => Some(clock.slot_index(v).ok_or_else(|| {
                    format!(
                        "`--occasion {raw}`: `{v}` is not a slot of the clock ({})",
                        clock.slots.join(", ")
                    )
                })?),
            };
            let allowed = positions
                .iter()
                .enumerate()
                .filter(|(_, at)| day.is_none_or(|d| at.day == d) && slot.is_none_or(|s| at.slot == s))
                .map(|(k, _)| k)
                .collect();
            pins[ci] = Pin::Clock {
                allowed,
                day,
                slot: slot.and_then(|s| clock.slot_name(s)).map(str::to_string),
                slotted: clock.slot.is_some(),
            };
        }
    }
    Ok(Some(pins))
}

/// What one column decides at one cell.
struct Outcome {
    /// `select: first`: the beat presented; `None` when nothing is eligible
    /// or an unknown `when` decides the outcome.
    winner: Option<String>,
    /// Every beat presented, in order: the winner, or the whole offered /
    /// sequenced list.
    presented: Vec<String>,
    /// Eligible but not presented (shadowed by the winner).
    shadowed: Vec<String>,
    /// `(id, why)` of every candidate whose `when` evaluated unknown.
    unknown: Vec<(String, String)>,
    /// An unknown `when` decides this outcome — play would halt here.
    undecided: bool,
}

/// One cell: its values, quest-settle notes, one outcome per column (`None`
/// where a per-occasion `@` list leaves the column out of the cell), and
/// per `--facts` relation the facts that hold there.
struct Cell {
    at: Vec<(String, String, Json)>,
    notes: Vec<String>,
    outcomes: Vec<Option<Outcome>>,
    facts: Vec<Vec<Fact>>,
}

/// A candidate beat's record across every cell.
#[derive(Default)]
struct Seen {
    eligible: bool,
    reasons: BTreeSet<String>,
    /// Presented in some cell where it was eligible.
    presented: bool,
    /// What was presented over it where it was eligible but not presented
    /// (`?`: an unknown `when` decided the cell).
    beaten_by: BTreeSet<String>,
}

/// A `--facts` relation: its name, argument domains, and the members of
/// its first argument's domain when closed (every row the table shows).
struct FactsRel {
    name: String,
    args: Vec<String>,
    members: Vec<String>,
}

/// `lute calendar`'s options (see [`crate::Command::Calendar`]).
pub(crate) struct CalendarArgs<'a> {
    pub axes: &'a [(String, Vec<String>)],
    pub occasions: &'a [String],
    pub facts: &'a [String],
    pub targets: &'a [String],
    pub script: Option<&'a Path>,
    pub until: Option<&'a str>,
    pub where_: Option<&'a str>,
    pub json: bool,
    pub csv: bool,
}

/// Where every cell starts, for the header.
struct Origin {
    /// `None`: the declared defaults.
    script: Option<String>,
    /// Steps replayed (`repeat:` counted once per step).
    replayed: usize,
    /// The `--until` step, not played: `(n, label)`.
    until: Option<(usize, Option<String>)>,
}

impl Origin {
    fn describe(&self) -> String {
        let Some(script) = &self.script else {
            return "declared defaults".to_string();
        };
        let replayed = match self.replayed {
            0 => String::new(),
            1 => ", then its step 1 replayed".to_string(),
            k => format!(", then its steps 1–{k} replayed"),
        };
        let until = match &self.until {
            Some((n, Some(label))) => format!(" (stopping before step {n}, {label})"),
            Some((n, None)) => format!(" (stopping before step {n})"),
            None => String::new(),
        };
        format!("the save in {script}{replayed}{until}")
    }
}

/// The index of the `--until` step in `steps`: a 1-based step number or a
/// step `label:`.
fn until_index(steps: &[ScriptStep], until: &str) -> Result<usize, String> {
    if let Ok(n) = until.trim().parse::<usize>() {
        return steps.iter().position(|s| s.n == n).ok_or_else(|| {
            format!(
                "`--until {until}`: the script has {} step(s)",
                steps.len()
            )
        });
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
fn start_world(
    p: &Project,
    save: &PlayScript,
    script: Option<&Path>,
    until: Option<&str>,
) -> Result<(World, Origin), String> {
    let at = |e: String| match script {
        Some(s) => format!("{}: {e}", s.display()),
        None => e,
    };
    let w = seed_world(p, save).map_err(at)?;
    let stop = match until {
        Some(u) => until_index(&save.steps, u).map_err(at)?,
        None => save.steps.len(),
    };
    let origin = Origin {
        script: script.map(|s| s.display().to_string()),
        replayed: stop,
        until: until.map(|_| (save.steps[stop].n, save.steps[stop].label.clone())),
    };
    if stop == 0 {
        return Ok((w, origin));
    }
    let plan = plan_steps(p, &save.steps[..stop]).map_err(at)?;
    let play = execute(p, save, &plan, w);
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
fn holds_at(p: &Project, w: &World, cel: &str) -> Result<bool, String> {
    let mut eval = Runner::with_carryover(
        &p.eval_json,
        w.mock(),
        w.state.clone(),
        w.facts.clone(),
        w.quests.clone(),
    )
    .with_visited(&w.visited);
    eval.eval_guard(cel).map_err(|atoms| describe_atoms(&atoms))
}

/// See [`crate::Command::Calendar`].
pub(crate) fn run_calendar(dir: &Path, args: &CalendarArgs<'_>) -> ExitCode {
    let usage = |msg: String| {
        eprintln!("lute calendar: {msg}");
        ExitCode::from(2)
    };
    let save = match args.script {
        None => PlayScript {
            surfaces: lute_trace::MockSet::default(),
            save: super::SaveSeed::default(),
            steps: Vec::new(),
            step_expects: Vec::new(),
            expect: None,
            derive: None,
        },
        Some(path) => {
            let text = match std::fs::read_to_string(path) {
                Ok(t) => t,
                Err(e) => return usage(format!("cannot read {}: {e}", path.display())),
            };
            match parse_script_with(&text, path, false) {
                Ok(s) => s,
                Err(e) => return usage(format!("invalid play script {}: {e}", path.display())),
            }
        }
    };
    let mut seen_paths = BTreeSet::new();
    for (path, _) in args.axes {
        if !seen_paths.insert(path.as_str()) {
            return usage(format!("`--axis {path}` is given twice"));
        }
    }
    if let Some(cel) = args.where_ {
        let mut arena = lute_cel::CelArena::default();
        if let Err(e) = lute_cel::parse_slot(&mut arena, cel, 0) {
            return usage(format!("`--where {cel}` is not a CEL expression: {e:?}"));
        }
    }
    if !dir.is_dir() {
        return usage(format!("{} is not a project directory", dir.display()));
    }
    let p = match compile_project(dir) {
        Ok(p) => p,
        Err(code) => return code,
    };
    let mut resolved = Vec::with_capacity(args.axes.len());
    for (path, values) in args.axes {
        match resolve_axis(&p, path, values) {
            Ok(a) => resolved.push(a),
            Err(e) => return usage(e),
        }
    }
    // dsl 0.24.0 §1: the clock axis writes the day and slot paths itself.
    if let (Some(clock), true) = (&p.index.clock, resolved.iter().any(|a| matches!(a.apply, Apply::Clock))) {
        if let Some(a) = resolved.iter().find(|a| a.path == clock.day || clock.slot.as_ref() == Some(&a.path)) {
            let paths = match &clock.slot {
                Some(slot) => format!("`{}` and `{slot}`", clock.day),
                None => format!("`{}`", clock.day),
            };
            return usage(format!(
                "`--axis {}` and `--axis clock` both set the clock — the clock axis already \
                 varies {paths}; vary one part per occasion with `--occasion O@{}` (or `O@clock.day`)",
                a.path, clock.day
            ));
        }
    }
    let cell_count = resolved
        .iter()
        .try_fold(1usize, |n, a| n.checked_mul(a.values.len()))
        .filter(|&n| n <= MAX_CELLS);
    let Some(cell_count) = cell_count else {
        return usage(format!("the axes' product exceeds {MAX_CELLS} cells"));
    };
    let mut specs = Vec::with_capacity(args.occasions.len());
    for raw in args.occasions {
        match parse_occasion(raw) {
            Ok(s) if specs.iter().any(|o: &OccasionSpec<'_>| o.name == s.name) => {
                return usage(format!("`--occasion {}` is given twice", s.name))
            }
            Ok(s) => specs.push(s),
            Err(e) => return usage(e),
        }
    }
    let names: Vec<String> = specs.iter().map(|s| s.name.to_string()).collect();
    let mut columns = match columns(&p, &names, args.targets) {
        Ok(c) => c,
        Err(e) => return usage(e),
    };
    for spec in &specs {
        let pins = match occasion_pins(spec, &resolved, p.index.clock.as_ref()) {
            Ok(p) => p,
            Err(e) => return usage(e),
        };
        for col in columns.iter_mut().filter(|c| c.occasion == spec.name) {
            col.pins.clone_from(&pins);
        }
    }
    let mut rels: Vec<FactsRel> = Vec::with_capacity(args.facts.len());
    for name in args.facts {
        let Some(r) = p.index.relations.iter().find(|r| &r.name == name) else {
            return usage(unknown_id(
                "`--facts`",
                name,
                "relation",
                p.index.relations.iter().map(|r| r.name.as_str()),
            ));
        };
        if rels.iter().any(|f| &f.name == name) {
            return usage(format!("`--facts {name}` is given twice"));
        }
        rels.push(FactsRel {
            name: name.clone(),
            members: r
                .args
                .first()
                .and_then(|d| domain_members(&p, d))
                .map(<[String]>::to_vec)
                .unwrap_or_default(),
            args: r.args.clone(),
        });
    }
    let (base, origin) = match start_world(&p, &save, args.script, args.until) {
        Ok(b) => b,
        Err(e) => return usage(e),
    };

    let mut seen: BTreeMap<usize, Seen> = BTreeMap::new();
    let mut cells = Vec::with_capacity(cell_count);
    let mut pruned = 0usize;
    for n in 0..cell_count {
        // Odometer order: the first axis varies slowest.
        let mut rest = n;
        let mut picks = vec![0; resolved.len()];
        for (i, axis) in resolved.iter().enumerate().rev() {
            picks[i] = rest % axis.values.len();
            rest /= axis.values.len();
        }
        let mut w = base.clone();
        let mut at = Vec::with_capacity(resolved.len());
        for (axis, &i) in resolved.iter().zip(&picks) {
            let (text, value) = &axis.values[i];
            apply_axis(&p, &mut w, axis, text, value);
            at.push((axis.path.clone(), text.clone(), value_to_json(value)));
        }
        let mut notes = Vec::new();
        if let Some(h) = advance_quests(&p, &mut w).1 {
            notes.push(format!("quest settle halted — {}", h.message()));
        }
        for (axis, &i) in resolved.iter().zip(&picks) {
            let (text, value) = &axis.values[i];
            notes.extend(settled_away(&p, &w, axis, text, value));
        }
        if let Some(cel) = args.where_ {
            match holds_at(&p, &w, cel) {
                Ok(true) => {}
                Ok(false) => {
                    pruned += 1;
                    continue;
                }
                Err(why) => {
                    let label: Vec<String> =
                        at.iter().map(|(path, text, _)| format!("{path}={text}")).collect();
                    return usage(format!(
                        "`--where {cel}` is unknown at the cell {}: {why}",
                        label.join(" ")
                    ));
                }
            }
        }
        let outcomes = columns
            .iter()
            .map(|col| col.applies(&picks).then(|| evaluate(&p, &w, col, &mut seen)))
            .collect();
        let facts = cell_facts(&p, &w, &rels);
        cells.push(Cell { at, notes, outcomes, facts });
    }
    let listed = |keep: fn(&Seen) -> bool| -> Vec<(&IndexBeat, &Seen)> {
        seen.iter()
            .filter(|(_, s)| keep(s))
            .map(|(&i, s)| (&p.index.beats[i], s))
            .collect()
    };
    let report = Report {
        from: origin.describe(),
        pruned,
        axes: &resolved,
        columns: &columns,
        cells: &cells,
        rels: &rels,
        never_eligible: listed(|s| !s.eligible),
        never_presented: listed(|s| s.eligible && !s.presented),
    };
    let out = if args.json {
        let mut s = serde_json::to_string_pretty(&render_json(&report)).unwrap_or_default();
        s.push('\n');
        s
    } else if args.csv {
        render_csv(&report)
    } else {
        render_text(dir, &report)
    };
    match crate::write_stdout(&out) {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::from(2),
    }
}

/// Everything a rendering reads.
struct Report<'a> {
    /// Where every cell starts ([`Origin::describe`]).
    from: String,
    /// Cells `--where` dropped.
    pruned: usize,
    axes: &'a [Axis],
    columns: &'a [Column],
    cells: &'a [Cell],
    rels: &'a [FactsRel],
    /// Candidates eligible in no cell.
    never_eligible: Vec<(&'a IndexBeat, &'a Seen)>,
    /// Eligible in some cell, presented in none.
    never_presented: Vec<(&'a IndexBeat, &'a Seen)>,
}

/// Per `--facts` relation, the facts of it that hold over the settled
/// cell: the runner's fixpoint, as [`super::world_view`] derives them.
fn cell_facts(p: &Project, w: &World, rels: &[FactsRel]) -> Vec<Vec<Fact>> {
    if rels.is_empty() {
        return Vec::new();
    }
    let eval = Runner::with_carryover(
        &p.eval_json,
        w.mock(),
        w.state.clone(),
        w.facts.clone(),
        w.quests.clone(),
    );
    rels.iter()
        .map(|r| eval.all_facts().iter().filter(|(rel, _)| *rel == r.name).cloned().collect())
        .collect()
}

/// The columns: every listed occasion (default: every occasion a beat
/// answers, by name), each targeted one once per target — the `--target`s
/// it takes, else every target its beats name (0.23.1: never the rest of
/// its declared domain, where no beat answers and every cell would read as
/// a hole). A targeted occasion none of whose beats names a target gets one
/// column its untargeted beats answer.
fn columns(p: &Project, occasions: &[String], targets: &[String]) -> Result<Vec<Column>, String> {
    let answered: BTreeSet<&str> = p.index.beats.iter().map(|b| b.on.as_str()).collect();
    let listed: Vec<&str> = if occasions.is_empty() {
        answered.iter().copied().collect()
    } else {
        occasions.iter().map(String::as_str).collect()
    };
    for occ in &listed {
        let known = if p.occasions.is_empty() {
            answered.contains(occ) || p.objective_occasions.contains(*occ)
        } else {
            p.occasions.contains_key(*occ)
        };
        if !known {
            let vocab: Vec<&str> = if p.occasions.is_empty() {
                answered.iter().copied().collect()
            } else {
                p.occasions.keys().map(String::as_str).collect()
            };
            return Err(format!(
                "`--occasion {occ}` is not an occasion of this project (known: {})",
                vocab.join(", ")
            ));
        }
    }
    let mut used_targets = BTreeSet::new();
    let mut out = Vec::new();
    for occ in listed {
        let select = p.select_of(occ);
        let decl = p.occasions.get(occ);
        let beat_targets: BTreeSet<&str> = p
            .index
            .beats
            .iter()
            .filter(|b| b.on == occ)
            .filter_map(|b| b.target.as_deref())
            .collect();
        let targeted = decl.map_or(!beat_targets.is_empty(), |d| d.target.takes_target());
        let column = |target: Option<String>| Column {
            occasion: occ.to_string(),
            any_target: targeted && target.is_none(),
            target,
            select,
            pins: None,
        };
        if !targeted {
            out.push(column(None));
            continue;
        }
        if !targets.is_empty() {
            for t in targets {
                let fits = match decl {
                    Some(d) => lute_check::occasion_target_ok(d, t, &p.kinds).is_ok(),
                    None => true,
                };
                if fits {
                    used_targets.insert(t.as_str());
                    out.push(column(Some(t.clone())));
                }
            }
        } else if beat_targets.is_empty() {
            out.push(column(None));
        } else {
            out.extend(beat_targets.iter().map(|t| column(Some(t.to_string()))));
        }
    }
    if let Some(t) = targets.iter().find(|t| !used_targets.contains(t.as_str())) {
        return Err(format!(
            "`--target {t}` is a target of none of the listed targeted occasions"
        ));
    }
    Ok(out)
}

/// One column at one cell, recording every candidate's verdict in `seen`
/// (keyed by its `ProjectIndex.beats` row).
fn evaluate(p: &Project, w: &World, col: &Column, seen: &mut BTreeMap<usize, Seen>) -> Outcome {
    let cands = eligible_at(p, w, &col.occasion, col.target.as_deref());
    let rows: Vec<Option<usize>> = cands
        .iter()
        .map(|c| {
            p.index.beats.iter().position(|b| {
                b.id == c.id
                    && b.document == c.document
                    && is_candidate(b, &col.occasion, col.target.as_deref())
            })
        })
        .collect();
    for (c, row) in cands.iter().zip(&rows) {
        let Some(row) = *row else {
            continue;
        };
        let s = seen.entry(row).or_default();
        match &c.verdict {
            Verdict::Eligible => s.eligible = true,
            Verdict::Ineligible(why) => {
                s.reasons.insert(why.clone());
            }
            Verdict::Unknown(why) => {
                s.reasons.insert(format!("when: unknown ({why})"));
            }
        }
    }
    let unknown: Vec<(String, String)> = cands
        .iter()
        .filter_map(|c| match &c.verdict {
            Verdict::Unknown(why) => Some((c.id.clone(), why.clone())),
            _ => None,
        })
        .collect();
    let undecided = deciding_unknown(&cands, col.select).is_some();
    let shown: Vec<usize> = if undecided {
        Vec::new()
    } else {
        presented(col.select, &cands)
    };
    let winner = (col.select == OccasionSelect::First)
        .then(|| shown.iter().map(|&i| &cands[i]).find(|c| !c.also).map(|c| c.id.clone()))
        .flatten();
    let presented: Vec<String> = shown.iter().map(|&i| cands[i].id.clone()).collect();
    // What a shadowed beat lost to: the winner, else the presented list.
    let beater = if undecided {
        "?".to_string()
    } else {
        winner.clone().unwrap_or_else(|| presented.join(", "))
    };
    let mut shadowed = Vec::new();
    for (i, c) in cands.iter().enumerate() {
        if !matches!(c.verdict, Verdict::Eligible) {
            continue;
        }
        let s = rows[i].map(|row| seen.entry(row).or_default());
        if shown.contains(&i) {
            if let Some(s) = s {
                s.presented = true;
            }
        } else {
            shadowed.push(c.id.clone());
            if let Some(s) = s {
                s.beaten_by.insert(beater.clone());
            }
        }
    }
    Outcome {
        winner,
        presented,
        shadowed,
        unknown,
        undecided,
    }
}

/// The grid's cell text: the winner (or the offered list), `?` when an
/// unknown `when` decides it, `-` when nothing is presented; `+N` counts
/// the eligible beats it shadows.
fn cell_text(o: &Outcome) -> String {
    let mut s = if o.undecided {
        "?".to_string()
    } else if o.presented.is_empty() {
        "-".to_string()
    } else {
        o.presented.join(", ")
    };
    if !o.shadowed.is_empty() {
        let _ = write!(s, " +{}", o.shadowed.len());
    }
    s
}

fn cell_label(cell: &Cell) -> String {
    cell.at
        .iter()
        .map(|(path, text, _)| format!("{path}={text}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// A cell's values alone, `/`-joined — a presence table's column head.
fn cell_short(cell: &Cell) -> String {
    if cell.at.is_empty() {
        return "(start)".to_string();
    }
    cell.at.iter().map(|(_, text, _)| text.as_str()).collect::<Vec<_>>().join("/")
}

/// `occasion[@target]` of a beat.
fn beat_on(b: &IndexBeat) -> String {
    match &b.target {
        Some(t) => format!("{}@{t}", b.on),
        None => b.on.clone(),
    }
}

/// Left-aligned columns two spaces apart; trailing blanks trimmed.
fn table(rows: &[Vec<String>]) -> String {
    let mut out = String::new();
    let widths: Vec<usize> = (0..rows.first().map_or(0, Vec::len))
        .map(|i| rows.iter().map(|r| r[i].chars().count()).max().unwrap_or(0))
        .collect();
    for row in rows {
        let mut line = String::new();
        for (i, v) in row.iter().enumerate() {
            if i + 1 == row.len() {
                line.push_str(v);
            } else {
                let _ = write!(line, "{v:<w$}  ", w = widths[i]);
            }
        }
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out
}

/// `(varies, held)` of a per-occasion column: the axes (or clock parts) it
/// varies over and `(path, value text, value)` of every one it is held at.
fn pinned(pins: &[Pin], axes: &[Axis]) -> (Vec<String>, Vec<(String, String, Value)>) {
    let mut varies = Vec::new();
    let mut held = Vec::new();
    for (pin, axis) in pins.iter().zip(axes) {
        match pin {
            Pin::Varies => varies.push(axis.path.clone()),
            Pin::At(k) => {
                let (text, value) = &axis.values[*k];
                held.push((axis.path.clone(), text.clone(), value.clone()));
            }
            Pin::Clock { day, slot, slotted, .. } => {
                match day {
                    None => varies.push("clock.day".to_string()),
                    Some(d) => held.push(("clock.day".to_string(), d.to_string(), Value::Num(*d as f64))),
                }
                match slot {
                    None if !slotted => {}
                    None => varies.push("clock.slot".to_string()),
                    Some(s) => held.push(("clock.slot".to_string(), s.clone(), Value::Str(s.clone()))),
                }
            }
        }
    }
    (varies, held)
}

/// A presence table's rows: per first argument (the closed domain's
/// members, then any other first argument some cell holds), its text at
/// every cell — the remaining arguments, `yes` for a unary relation, `-`
/// when nothing holds. A nullary relation is one row under its name.
fn fact_rows(ri: usize, rel: &FactsRel, cells: &[Cell]) -> Vec<(String, Vec<String>)> {
    if rel.args.is_empty() {
        let row = cells
            .iter()
            .map(|c| if c.facts[ri].is_empty() { "-" } else { "yes" }.to_string())
            .collect();
        return vec![(rel.name.clone(), row)];
    }
    let mut firsts = rel.members.clone();
    let seen: BTreeSet<&str> = cells
        .iter()
        .flat_map(|c| c.facts[ri].iter().map(|(_, args)| args[0].as_str()))
        .filter(|a| !rel.members.iter().any(|m| m == a))
        .collect();
    firsts.extend(seen.into_iter().map(str::to_string));
    firsts
        .into_iter()
        .map(|first| {
            let row = cells
                .iter()
                .map(|c| {
                    let here: Vec<String> = c.facts[ri]
                        .iter()
                        .filter(|(_, args)| args[0] == first)
                        .map(|(_, args)| {
                            if args.len() == 1 { "yes".to_string() } else { args[1..].join(", ") }
                        })
                        .collect();
                    if here.is_empty() { "-".to_string() } else { here.join(" | ") }
                })
                .collect();
            (first, row)
        })
        .collect()
}

fn render_text(dir: &Path, r: &Report<'_>) -> String {
    let (axes, columns, cells) = (r.axes, r.columns, r.cells);
    let mut out = String::new();
    let pruned = match r.pruned {
        0 => String::new(),
        n => format!(" ({n} dropped by --where)"),
    };
    let _ = writeln!(
        out,
        "calendar: {} — {} cell(s){pruned} × {} column(s), from {}",
        dir.display(),
        cells.len(),
        columns.len(),
        r.from
    );
    let mut noted = BTreeSet::new();
    for c in columns {
        let Some(pins) = &c.pins else { continue };
        if !noted.insert(c.occasion.as_str()) {
            continue;
        }
        let (varies, held) = pinned(pins, axes);
        let varies = if varies.is_empty() { "no axis".to_string() } else { varies.join(", ") };
        let held: Vec<String> = held.iter().map(|(p, t, _)| format!("{p}={t}")).collect();
        let held = if held.is_empty() { String::new() } else { format!(", at {}", held.join(" ")) };
        let _ = writeln!(out, "  {}: varies over {varies} only{held}; blank elsewhere", c.occasion);
    }
    out.push('\n');
    // Two header rows: the axis paths and occasions, then the targets.
    let mut rows: Vec<Vec<String>> = Vec::with_capacity(cells.len() + 2);
    let targeted = columns.iter().any(|c| c.target.is_some() || c.any_target);
    let mut head: Vec<String> = axes.iter().map(|a| a.path.clone()).collect();
    head.extend(columns.iter().map(|c| {
        if c.select == OccasionSelect::First {
            c.occasion.clone()
        } else {
            format!("{} ({})", c.occasion, c.select.as_str())
        }
    }));
    rows.push(head);
    if targeted {
        let mut second: Vec<String> = axes.iter().map(|_| String::new()).collect();
        second.extend(columns.iter().map(|c| match &c.target {
            Some(t) => t.clone(),
            None if c.any_target => "(any)".to_string(),
            None => String::new(),
        }));
        rows.push(second);
    }
    for cell in cells {
        let mut row: Vec<String> = cell.at.iter().map(|(_, text, _)| text.clone()).collect();
        row.extend(cell.outcomes.iter().map(|o| o.as_ref().map(cell_text).unwrap_or_default()));
        rows.push(row);
    }
    out.push_str(&table(&rows));

    for (ri, rel) in r.rels.iter().enumerate() {
        let _ = writeln!(out, "\nfacts {}({}):", rel.name, rel.args.join(", "));
        let first = rel.args.first().unwrap_or(&rel.name).clone();
        let mut rows = vec![std::iter::once(first).chain(cells.iter().map(cell_short)).collect::<Vec<_>>()];
        rows.extend(fact_rows(ri, rel, cells).into_iter().map(|(label, row)| {
            std::iter::once(label).chain(row).collect()
        }));
        out.push_str(&table(&rows));
    }

    let mut shadowed = String::new();
    let mut undecided = String::new();
    let mut notes = String::new();
    for cell in cells {
        let label = cell_label(cell);
        for n in &cell.notes {
            let _ = writeln!(notes, "  {label}: {n}");
        }
        for (col, o) in columns.iter().zip(&cell.outcomes) {
            let Some(o) = o else { continue };
            if !o.shadowed.is_empty() {
                // An undecided cell presents nothing: the eligible beats wait
                // behind the unknown `when`, which `?` stands for.
                let over = if o.undecided { "?".to_string() } else { o.presented.join(", ") };
                let _ = writeln!(
                    shadowed,
                    "  {label}  {}: {over} over {}",
                    col.label(),
                    o.shadowed.join(", ")
                );
            }
            if o.undecided {
                for (id, why) in &o.unknown {
                    let _ = writeln!(undecided, "  {label}  {}: {id} — {why}", col.label());
                }
            }
        }
    }
    for (title, body) in [
        ("shadowed (eligible, not presented):", shadowed),
        ("undecided (an unknown `when` decides the cell; play halts there):", undecided),
        ("notes:", notes),
    ] {
        if !body.is_empty() {
            let _ = write!(out, "\n{title}\n{body}");
        }
    }
    for (title, list, detail) in [
        ("never eligible in any cell", &r.never_eligible, "" as &str),
        ("eligible but never presented in any cell", &r.never_presented, "lost to "),
    ] {
        let _ = write!(out, "\n{title}: ");
        if list.is_empty() {
            out.push_str("none\n");
            continue;
        }
        let _ = writeln!(out, "{}", list.len());
        for (b, s) in list {
            let why: Vec<&str> = if detail.is_empty() { &s.reasons } else { &s.beaten_by }
                .iter()
                .map(String::as_str)
                .collect();
            let _ = writeln!(
                out,
                "  {} [{}, {}] {} — {detail}{}",
                b.id,
                kind_label(b.kind),
                b.document,
                beat_on(b),
                why.join("; ")
            );
        }
    }
    out
}

fn render_json(r: &Report<'_>) -> Json {
    let col_json = |c: &Column| {
        let mut m = serde_json::Map::new();
        m.insert("occasion".into(), json!(c.occasion));
        if let Some(t) = &c.target {
            m.insert("target".into(), json!(t));
        }
        if c.any_target {
            m.insert("anyTarget".into(), json!(true));
        }
        m.insert("select".into(), json!(c.select.as_str()));
        m
    };
    let beat_json = |b: &IndexBeat| {
        let mut m = serde_json::Map::new();
        m.insert("id".into(), json!(b.id));
        m.insert("kind".into(), json!(kind_label(b.kind)));
        m.insert("document".into(), json!(b.document));
        m.insert("on".into(), json!(b.on));
        if let Some(t) = &b.target {
            m.insert("target".into(), json!(t));
        }
        m
    };
    json!({
        "from": r.from,
        "pruned": r.pruned,
        "axes": r.axes.iter().map(|a| json!({
            "path": a.path,
            "values": a.values.iter().map(|(_, v)| value_to_json(v)).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "columns": r.columns.iter().map(|c| {
            let mut m = col_json(c);
            if let Some(pins) = &c.pins {
                let (varies, held) = pinned(pins, r.axes);
                m.insert("varies".into(), json!(varies));
                m.insert("heldAt".into(), Json::Object(
                    held.iter().map(|(p, _, v)| (p.to_string(), value_to_json(v))).collect(),
                ));
            }
            Json::Object(m)
        }).collect::<Vec<_>>(),
        "cells": r.cells.iter().map(|cell| {
            let at: serde_json::Map<String, Json> =
                cell.at.iter().map(|(p, _, v)| (p.clone(), v.clone())).collect();
            let mut m = serde_json::Map::new();
            m.insert("at".into(), Json::Object(at));
            if !cell.notes.is_empty() {
                m.insert("notes".into(), json!(cell.notes));
            }
            m.insert("results".into(), Json::Array(r.columns.iter().zip(&cell.outcomes).filter_map(|(c, o)| {
                let o = o.as_ref()?;
                let mut r = col_json(c);
                r.insert("winner".into(), json!(o.winner));
                r.insert("presented".into(), json!(o.presented));
                r.insert("shadowed".into(), json!(o.shadowed));
                if !o.unknown.is_empty() {
                    r.insert("unknown".into(), Json::Array(o.unknown.iter().map(|(id, why)| {
                        json!({ "id": id, "reason": why })
                    }).collect()));
                }
                if o.undecided {
                    r.insert("undecided".into(), json!(true));
                }
                Some(Json::Object(r))
            }).collect()));
            if !r.rels.is_empty() {
                m.insert("facts".into(), Json::Object(r.rels.iter().zip(&cell.facts).map(|(rel, fs)| {
                    (rel.name.clone(), json!(fs.iter().map(render_fact).collect::<Vec<_>>()))
                }).collect()));
            }
            Json::Object(m)
        }).collect::<Vec<_>>(),
        "neverEligible": r.never_eligible.iter().map(|(b, s)| {
            let mut m = beat_json(b);
            m.insert("reasons".into(), json!(s.reasons));
            Json::Object(m)
        }).collect::<Vec<_>>(),
        "neverPresented": r.never_presented.iter().map(|(b, s)| {
            let mut m = beat_json(b);
            m.insert("beatenBy".into(), json!(s.beaten_by));
            Json::Object(m)
        }).collect::<Vec<_>>(),
    })
}

/// RFC 4180 field: quoted when it holds a comma, quote or newline.
fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn csv_line(out: &mut String, fields: &[String]) {
    let _ = writeln!(out, "{}", fields.iter().map(|f| csv_field(f)).collect::<Vec<_>>().join(","));
}

/// One row per cell × evaluated column (a cell no column is evaluated at
/// still gets one row when `--facts` asks for its facts); list fields
/// `;`-joined, a `facts:<relation>` field per `--facts`. The beats eligible
/// somewhere but never presented follow as a second table after a blank
/// line, when there are any.
fn render_csv(r: &Report<'_>) -> String {
    let mut out = String::new();
    let mut head: Vec<String> = r.axes.iter().map(|a| a.path.clone()).collect();
    head.extend(
        ["occasion", "target", "select", "winner", "presented", "shadowed", "unknown", "notes"]
            .map(str::to_string),
    );
    head.extend(r.rels.iter().map(|rel| format!("facts:{}", rel.name)));
    csv_line(&mut out, &head);
    for cell in r.cells {
        let facts: Vec<String> = cell
            .facts
            .iter()
            .map(|fs| fs.iter().map(render_fact).collect::<Vec<_>>().join(";"))
            .collect();
        let mut rows: Vec<[String; 8]> = Vec::new();
        for (c, o) in r.columns.iter().zip(&cell.outcomes) {
            let Some(o) = o else { continue };
            let unknown: Vec<&str> = o.unknown.iter().map(|(id, _)| id.as_str()).collect();
            rows.push([
                c.occasion.clone(),
                c.target.clone().unwrap_or_else(|| {
                    if c.any_target { "(any)".to_string() } else { String::new() }
                }),
                c.select.as_str().to_string(),
                o.winner.clone().unwrap_or_default(),
                o.presented.join(";"),
                o.shadowed.join(";"),
                unknown.join(";"),
                cell.notes.join(";"),
            ]);
        }
        if rows.is_empty() && !r.rels.is_empty() {
            let mut bare: [String; 8] = Default::default();
            bare[7] = cell.notes.join(";");
            rows.push(bare);
        }
        for fields in rows {
            let mut row: Vec<String> = cell.at.iter().map(|(_, text, _)| text.clone()).collect();
            row.extend(fields);
            row.extend(facts.iter().cloned());
            csv_line(&mut out, &row);
        }
    }
    if !r.never_presented.is_empty() {
        out.push('\n');
        csv_line(
            &mut out,
            &["neverPresented", "kind", "document", "occasion", "target", "beatenBy"].map(str::to_string),
        );
        for (b, s) in &r.never_presented {
            csv_line(
                &mut out,
                &[
                    b.id.clone(),
                    kind_label(b.kind).to_string(),
                    b.document.clone(),
                    b.on.clone(),
                    b.target.clone().unwrap_or_default(),
                    s.beaten_by.iter().cloned().collect::<Vec<_>>().join(";"),
                ],
            );
        }
    }
    out
}
