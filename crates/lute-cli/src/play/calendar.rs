//! `lute calendar <dir> [--axis <path>=<values>]… [--occasion O]… [--target
//! T]… [--script route.play.yaml [--until <step>]] [--where <cel>] [--json |
//! --csv]` (dsl 0.23.0 §1, D-A; 0.23.1).
//!
//! The calendar is a tool over play, not a schedule file. Every cell of the
//! axes' product starts from one world: the `--script`'s save (0.22.0 §3)
//! with its `steps:` replayed exactly as `lute play` plays them — up to the
//! `--until` step, which is not played — or the declared defaults. The
//! cell's values are then written as an `engine:` step would write them (a
//! `quest.<id>.state` axis seeds the quest's status as a save's `quests:`
//! does; a `holds(<fact>)` axis asserts or retracts a base fact), the quest
//! lifecycle settles, `--where` drops the cell when it does not hold, and
//! every listed occasion (and target) is evaluated with play's own
//! eligibility, [`super::eligible_at`]: a cell reads exactly what `lute
//! play` would decide there. Cells are independent — no presentation
//! happens, so nothing one cell does is seen by the next. An axis the
//! calendar cannot apply is a usage error, never silently dropped.
//!
//! Output: a grid (text), `--json`, or `--csv`; beats that were a candidate
//! somewhere but eligible in no cell are listed at the end with why.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::Path;
use std::process::ExitCode;

use lute_compile::index::IndexBeat;
use lute_manifest::schema::OccasionSelect;
use lute_trace::Value;
use serde_json::{json, Value as Json};

use super::{
    advance_quests, compile_project, deciding_unknown, describe_atoms, eligible_at, execute,
    is_candidate, kind_label, parse_script_with, plan_steps, presented, quest_state_id,
    resolve_fact, resolve_state, seed_quest, seed_world, value_to_json, Candidate, PlayScript,
    Project, ScriptStep, Verdict, World, QUEST_STATES,
};
use crate::runner::{Fact, Runner};

/// The most cells one invocation evaluates — a typo'd range (`1..70000`)
/// is refused rather than ground through.
const MAX_CELLS: usize = 10_000;

/// clap `value_parser` for `--axis <path>=<values>`: an inclusive integer
/// range `lo..hi` or a comma list `a,b,c`. The path may be a
/// `holds(<fact>)` (its commas are the fact's, before the `=`).
pub(crate) fn parse_axis_flag(raw: &str) -> Result<(String, Vec<String>), String> {
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
    let apply = if let Some(inner) = path.strip_prefix("holds(").and_then(|s| s.strip_suffix(')')) {
        Apply::Fact(resolve_fact(p, inner).map_err(|e| at(format!("`{inner}` {e}")))?)
    } else if let Some(id) = quest_state_id(path) {
        if !p.quest_ids.contains(id) {
            return Err(at(super::unknown_id(
                "it",
                id,
                "quest",
                p.quest_ids.iter().map(String::as_str),
            )));
        }
        Apply::Quest(id.to_string())
    } else if path.starts_with("quest.") && !is_objective_done(path) {
        return Err(at(format!(
            "`{path}` is the quest lifecycle's own bookkeeping and cannot be set per cell — \
             an axis over a quest is `quest.<id>.state` (its status) or \
             `quest.<id>.objectives.<oid>.done`"
        )));
    } else {
        Apply::State
    };
    let mut typed = Vec::with_capacity(values.len());
    for v in values {
        let value = match &apply {
            Apply::Fact(_) => match v.as_str() {
                "true" => Value::Bool(true),
                "false" => Value::Bool(false),
                _ => {
                    return Err(at(format!(
                        "a `holds(…)` axis takes `true` (asserted) and `false` (absent), not `{v}`"
                    )))
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
        };
        typed.push((v.clone(), value));
    }
    Ok(Axis {
        path: path.to_string(),
        apply,
        values: typed,
    })
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
    }
}

/// What the settle did to an axis value the cell was given — a quest
/// handler's write, a seeded quest status the lifecycle moved on.
fn settled_away(w: &World, axis: &Axis, text: &str, value: &Value) -> Option<String> {
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
        Apply::Fact(_) => None,
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
}

impl Column {
    fn label(&self) -> String {
        match &self.target {
            Some(t) => format!("{}@{t}", self.occasion),
            None if self.any_target => format!("{}@(any)", self.occasion),
            None => self.occasion.clone(),
        }
    }
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

/// One cell: its values, quest-settle notes, and one outcome per column.
struct Cell {
    at: Vec<(String, String, Json)>,
    notes: Vec<String>,
    outcomes: Vec<Outcome>,
}

/// A candidate beat's record across every cell.
#[derive(Default)]
struct Seen {
    eligible: bool,
    reasons: BTreeSet<String>,
}

/// `lute calendar`'s options (see [`crate::Command::Calendar`]).
pub(crate) struct CalendarArgs<'a> {
    pub axes: &'a [(String, Vec<String>)],
    pub occasions: &'a [String],
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
            match parse_script_with(&text, false) {
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
    let cell_count = resolved
        .iter()
        .try_fold(1usize, |n, a| n.checked_mul(a.values.len()))
        .filter(|&n| n <= MAX_CELLS);
    let Some(cell_count) = cell_count else {
        return usage(format!("the axes' product exceeds {MAX_CELLS} cells"));
    };
    let columns = match columns(&p, args.occasions, args.targets) {
        Ok(c) => c,
        Err(e) => return usage(e),
    };
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
            notes.extend(settled_away(&w, axis, text, value));
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
            .map(|col| evaluate(&p, &w, col, &mut seen))
            .collect();
        cells.push(Cell { at, notes, outcomes });
    }
    let never: Vec<(&IndexBeat, &Seen)> = seen
        .iter()
        .filter(|(_, s)| !s.eligible)
        .map(|(&i, s)| (&p.index.beats[i], s))
        .collect();

    let from = origin.describe();
    let out = if args.json {
        let mut s = serde_json::to_string_pretty(&render_json(
            &from, pruned, &resolved, &columns, &cells, &never,
        ))
        .unwrap_or_default();
        s.push('\n');
        s
    } else if args.csv {
        render_csv(&resolved, &columns, &cells)
    } else {
        render_text(dir, &from, pruned, &resolved, &columns, &cells, &never)
    };
    match crate::write_stdout(&out) {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::from(2),
    }
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
    for c in &cands {
        let Some(row) = p.index.beats.iter().position(|b| {
            b.id == c.id && b.document == c.document && is_candidate(b, &col.occasion, col.target.as_deref())
        }) else {
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
    let shown: Vec<&Candidate> = if undecided {
        Vec::new()
    } else {
        presented(col.select, &cands).into_iter().map(|i| &cands[i]).collect()
    };
    let shadowed = cands
        .iter()
        .filter(|c| matches!(c.verdict, Verdict::Eligible))
        .filter(|c| !shown.iter().any(|s| std::ptr::eq(*s, *c)))
        .map(|c| c.id.clone())
        .collect();
    Outcome {
        winner: (col.select == OccasionSelect::First)
            .then(|| shown.iter().find(|c| !c.also).map(|c| c.id.clone()))
            .flatten(),
        presented: shown.iter().map(|c| c.id.clone()).collect(),
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

fn render_text(
    dir: &Path,
    from: &str,
    pruned: usize,
    axes: &[Axis],
    columns: &[Column],
    cells: &[Cell],
    never: &[(&IndexBeat, &Seen)],
) -> String {
    let mut out = String::new();
    let pruned = match pruned {
        0 => String::new(),
        n => format!(" ({n} dropped by --where)"),
    };
    let _ = writeln!(
        out,
        "calendar: {} — {} cell(s){pruned} × {} column(s), from {from}",
        dir.display(),
        cells.len(),
        columns.len()
    );
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
        row.extend(cell.outcomes.iter().map(cell_text));
        rows.push(row);
    }
    let widths: Vec<usize> = (0..rows[0].len())
        .map(|i| rows.iter().map(|r| r[i].chars().count()).max().unwrap_or(0))
        .collect();
    for row in &rows {
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

    let mut shadowed = String::new();
    let mut undecided = String::new();
    let mut notes = String::new();
    for cell in cells {
        let label = cell_label(cell);
        for n in &cell.notes {
            let _ = writeln!(notes, "  {label}: {n}");
        }
        for (col, o) in columns.iter().zip(&cell.outcomes) {
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
    let _ = write!(out, "\nnever eligible in any cell: ");
    if never.is_empty() {
        out.push_str("none\n");
    } else {
        let _ = writeln!(out, "{}", never.len());
        for (b, s) in never {
            let on = match &b.target {
                Some(t) => format!("{}@{t}", b.on),
                None => b.on.clone(),
            };
            let why: Vec<&str> = s.reasons.iter().map(String::as_str).collect();
            let _ = writeln!(
                out,
                "  {} [{}, {}] {on} — {}",
                b.id,
                kind_label(b.kind),
                b.document,
                why.join("; ")
            );
        }
    }
    out
}

fn render_json(
    from: &str,
    pruned: usize,
    axes: &[Axis],
    columns: &[Column],
    cells: &[Cell],
    never: &[(&IndexBeat, &Seen)],
) -> Json {
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
    json!({
        "from": from,
        "pruned": pruned,
        "axes": axes.iter().map(|a| json!({
            "path": a.path,
            "values": a.values.iter().map(|(_, v)| value_to_json(v)).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "columns": columns.iter().map(|c| Json::Object(col_json(c))).collect::<Vec<_>>(),
        "cells": cells.iter().map(|cell| {
            let at: serde_json::Map<String, Json> =
                cell.at.iter().map(|(p, _, v)| (p.clone(), v.clone())).collect();
            let mut m = serde_json::Map::new();
            m.insert("at".into(), Json::Object(at));
            if !cell.notes.is_empty() {
                m.insert("notes".into(), json!(cell.notes));
            }
            m.insert("results".into(), Json::Array(columns.iter().zip(&cell.outcomes).map(|(c, o)| {
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
                Json::Object(r)
            }).collect()));
            Json::Object(m)
        }).collect::<Vec<_>>(),
        "neverEligible": never.iter().map(|(b, s)| {
            let mut m = serde_json::Map::new();
            m.insert("id".into(), json!(b.id));
            m.insert("kind".into(), json!(kind_label(b.kind)));
            m.insert("document".into(), json!(b.document));
            m.insert("on".into(), json!(b.on));
            if let Some(t) = &b.target {
                m.insert("target".into(), json!(t));
            }
            m.insert("reasons".into(), json!(s.reasons));
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

/// One row per cell × column; list fields `;`-joined.
fn render_csv(axes: &[Axis], columns: &[Column], cells: &[Cell]) -> String {
    let mut out = String::new();
    let mut head: Vec<String> = axes.iter().map(|a| a.path.clone()).collect();
    head.extend(
        ["occasion", "target", "select", "winner", "presented", "shadowed", "unknown", "notes"]
            .map(str::to_string),
    );
    let _ = writeln!(out, "{}", head.iter().map(|h| csv_field(h)).collect::<Vec<_>>().join(","));
    for cell in cells {
        for (c, o) in columns.iter().zip(&cell.outcomes) {
            let mut row: Vec<String> = cell.at.iter().map(|(_, text, _)| text.clone()).collect();
            let unknown: Vec<&str> = o.unknown.iter().map(|(id, _)| id.as_str()).collect();
            row.extend([
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
            let _ = writeln!(out, "{}", row.iter().map(|f| csv_field(f)).collect::<Vec<_>>().join(","));
        }
    }
    out
}
