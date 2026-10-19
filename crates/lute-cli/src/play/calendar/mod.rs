//! `lute calendar [--project <dir>] [--axis <axis>=<values>]… [--occasion O]…
//! [--occasion-axes O=<axis>[=<value>],…]… [--target T]… [--relation <relation>]… [--script
//! route.play.yaml [--until <step>]] [--where <cel>] [--json | --format csv]` (dsl
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
//! path, written as an `engine:` step writes it; a `per:` family (dsl
//! 0.24.0 §3) as a whole — `run.aff.*`, every member at the cell's value,
//! or `run.aff[run.route]`, only the member another axis's value names at
//! the cell (the others keep what the world holds), where the bare family
//! `run.aff` is a usage error naming those forms; `quest.<id>.state`, the
//! quest's status seeded as a save's `quests:` does;
//! `quest.<id>.objectives.<oid>.done`, objective progress as a save keeps
//! it; `holds(<fact>)=true,false`, a base fact asserted or retracted;
//! `visited('<id>')=true,false`, a scene or bundle-beat id added to or
//! removed from the visited set a save's `visited:` seeds (what `after:` and
//! CEL `visited()` read). An axis the calendar cannot apply is a usage
//! error naming the kinds, never silently dropped.
//!
//! Per-occasion axes: `--occasion-axes dayEnd=run.day` varies only `run.day` for
//! `dayEnd` — the occasion is evaluated in the cells where every other axis
//! is at its first value (once per `run.day` value) and is blank elsewhere;
//! `dayEnd=run.day,run.slot=night` holds `run.slot` at `night` instead. An
//! occasion the engine raises once per day is read once per day.
//!
//! Presence: `--relation <relation>` lists, per cell, the facts of that
//! relation that hold once the cell has settled (the runner's fixpoint, as
//! play's end-of-play `facts:` judge them) — in text one table per relation,
//! a row per first argument and a column per cell.
//!
//! Output: a grid (text), `--json`, or `--format csv`. At the end: beats that were
//! a candidate somewhere but eligible in no cell, with why; and beats
//! eligible somewhere but presented in no cell, with the beats presented
//! over them (`?` where an unknown `when` decided the cell).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::Path;
use std::process::ExitCode;

use crate::envelope::{self, CliError};
use lute_ir::IndexBeat;
use lute_manifest::relations::KindShape;
use lute_manifest::schema::OccasionSelect;
use lute_runtime::datalog::Fact;
use lute_runtime::{BridgeQueues, Value};
use serde_json::{json, Value as Json};

use lute_runtime::session::{
    advance_quests, clock_at, deciding_unknown, describe_atoms, eligible_at, presented,
    quest_state_id, refresh_clock, render_fact, seed_quest, seed_world, unknown_id, Candidate,
    Session, WorldSeed, QUEST_STATES,
};

use super::{
    compile_project, domain_members, entry_flag, execute, is_candidate, kind_label,
    parse_script_with, plan_steps, resolve_fact, resolve_state, value_to_json, ExecProject,
    PlayScript, ScriptStep, Verdict, World,
};

mod axes;
mod cells;
mod occasions;
mod render;
mod run;

pub(crate) use axes::parse_axis_flag;
use axes::{
    apply_axis, clock_axis_at, resolve_axis, settled_away, tied_outside, written_paths, Apply,
    Axis, MAX_CELLS,
};
use cells::{cell_facts, columns, evaluate};
use occasions::{
    describe_positions, occasion_pins, parse_occasion, OccasionSpec, Outcome, Pin, RaiseRule,
};
use render::{render_csv, render_json, render_text, undecided_why};
use run::{holds_at, start_world};

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

/// One cell: its values, quest-settle notes, one outcome per column (`None`
/// where a per-occasion `@` list leaves the column out of the cell), and
/// per `--relation` relation the facts that hold there.
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
    /// Per occasion, the positions where it was eligible but the clock does
    /// not raise the occasion — one reason each, once the grid is done.
    unraised: BTreeMap<String, BTreeSet<lute_manifest::clock::ClockAt>>,
}

/// A `--relation` relation: its name, argument domains, and the members of
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
    /// `--occasion-axes O=<AXES>`: `(occasion, axes)` pairs.
    pub occasion_axes: &'a [(String, String)],
    /// `--relation <RELATION>`: relations whose facts each cell lists.
    pub relations: &'a [String],
    pub targets: &'a [String],
    pub script: Option<&'a Path>,
    pub until: Option<&'a str>,
    pub where_: Option<&'a str>,
    pub json: bool,
    pub csv: bool,
    pub build: super::Build<'a>,
}

/// Where every cell starts, for the header.
struct Origin {
    /// `None`: the declared defaults.
    script: Option<String>,
    /// Steps replayed (`repeat:` counted once per step).
    replayed: usize,
    /// The `--until` step, not played: `(n, label)`.
    until: Option<(usize, Option<String>)>,
    /// Where the run starts on the declared clock: the save's position,
    /// before any replayed step (`None` without a clock).
    clock_start: Option<lute_manifest::clock::ClockAt>,
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
fn calendar_human(message: &str) -> String {
    message
        .lines()
        .map(|line| format!("lute calendar: {}", lute_core_span::plain_message(line)))
        .collect::<Vec<_>>()
        .join("\n")
}

fn calendar_argument(message: &str) -> Option<&'static str> {
    [
        ("`--axis", "--axis"),
        ("axes' product", "--axis"),
        ("`--where", "--where"),
        ("`--occasion-axes", "--occasion-axes"),
        ("`--occasion", "--occasion"),
        ("`--target", "--target"),
        ("`--relation", "--relation"),
        ("`--until", "--until"),
    ]
    .iter()
    .find_map(|(needle, argument)| message.contains(needle).then_some(*argument))
}
/// See [`crate::Command::Calendar`].
pub(crate) fn run_calendar(dir: &Path, args: &CalendarArgs<'_>) -> ExitCode {
    let usage = |msg: String| {
        let human = calendar_human(&msg);
        let error = CliError::usage(human);
        if let Some(argument) = calendar_argument(&msg) {
            envelope::fail(error.with_argument(argument))
        } else {
            envelope::fail(error)
        }
    };
    let save = match args.script {
        None => PlayScript {
            surfaces: lute_runtime::MockSet::default(),
            bridges: std::collections::BTreeMap::new(),
            choose: std::collections::BTreeMap::new(),
            save: lute_runtime::session::SaveSeed::default(),
            steps: Vec::new(),
            source: super::script::ScriptSource::default(),
            step_expects: Vec::new(),
            expect: None,
            derive: None,
        },
        Some(path) => {
            let text = match std::fs::read_to_string(path) {
                Ok(t) => t,
                Err(e) => {
                    let e = lute_manifest::io_reason(&e);
                    return envelope::fail(CliError::io(
                        path,
                        format!("lute calendar: cannot read {}: {e}", path.display()),
                    ));
                }
            };
            match parse_script_with(&text, path, false) {
                Ok(s) => s,
                Err(e) => {
                    return envelope::fail(CliError::input(path, calendar_human(&e)));
                }
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
        return envelope::fail(CliError::io(
            dir,
            format!(
                "lute calendar: {} is not a project directory",
                dir.display()
            ),
        ));
    }
    let p = match compile_project(
        &lute_model::ModelMemo::default(),
        dir,
        super::project::CALENDAR,
        &crate::EngineMatrix::reference(),
        &args.build,
    ) {
        Ok((p, _)) => p,
        Err(code) if code == ExitCode::from(2) => {
            return envelope::fail_quietly(CliError::io(
                dir,
                format!(
                    "lute calendar: cannot build project under {}",
                    dir.display()
                ),
            ));
        }
        Err(code) => return code,
    };
    let mut resolved = Vec::with_capacity(args.axes.len());
    for (path, values) in args.axes {
        match resolve_axis(&p, path, values, args.axes) {
            Ok(a) => resolved.push(a),
            Err(e) => return usage(e),
        }
    }
    // dsl 0.24.0 §3: a family axis sets its members — no other axis may.
    let may_write = |a: &Axis| -> Vec<String> {
        match a.apply.family() {
            Some((family, members)) => members.iter().map(|m| format!("{family}.{m}")).collect(),
            None if matches!(a.apply, Apply::State) => vec![a.path.clone()],
            None => Vec::new(),
        }
    };
    for (i, a) in resolved.iter().enumerate() {
        let writes = may_write(a);
        for b in &resolved[i + 1..] {
            if let Some(both) = may_write(b).iter().find(|s| writes.contains(s)) {
                return usage(format!(
                    "`--axis {}` and `--axis {}` both set `{both}` — give each state path one axis",
                    a.path, b.path
                ));
            }
        }
    }
    // dsl 0.24.0 §1: the clock axis writes the day and slot paths itself.
    if let (Some(clock), true) = (
        &p.index.clock,
        resolved.iter().any(|a| matches!(a.apply, Apply::Clock)),
    ) {
        if let Some(a) = resolved
            .iter()
            .find(|a| a.path == clock.day || clock.slot.as_ref() == Some(&a.path))
        {
            let paths = match &clock.slot {
                Some(slot) => format!("`{}` and `{slot}`", clock.day),
                None => format!("`{}`", clock.day),
            };
            return usage(format!(
                "`--axis {}` and `--axis clock` both set the clock — the clock axis already \
                 varies {paths}; vary one part per occasion with `--occasion-axes O={}` (or `O=clock.day`)",
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
    // `--occasion-axes O=<AXES>` is the 0.47.2 `--occasion O@<AXES>` spelling:
    // it parses through the same grammar and selects `O`. `--occasion`
    // itself takes bare occasion names.
    for raw in args.occasions {
        if raw.contains('@') {
            let (name, axes) = raw.split_once('@').unwrap_or((raw, ""));
            return usage(format!(
                "`--occasion {raw}`: `--occasion` takes a bare occasion name — vary only some \
                 axes with `--occasion-axes {}={axes}`",
                name.trim()
            ));
        }
    }
    let axes_raws: Vec<String> = args
        .occasion_axes
        .iter()
        .map(|(occasion, axes)| format!("{occasion}@{axes}"))
        .collect();
    let mut specs: Vec<OccasionSpec<'_>> =
        Vec::with_capacity(args.occasions.len() + axes_raws.len());
    let mut from_axes: Vec<bool> = Vec::new();
    for (raw, is_axes) in args
        .occasions
        .iter()
        .map(|raw| (raw.as_str(), false))
        .chain(axes_raws.iter().map(|raw| (raw.as_str(), true)))
    {
        match parse_occasion(raw) {
            Ok(s) => {
                if let Some(at) = specs.iter().position(|o| o.name == s.name) {
                    return usage(if is_axes && from_axes[at] {
                        format!("`--occasion-axes {}` is given twice", s.name)
                    } else if is_axes || from_axes[at] {
                        format!(
                            "`--occasion {0}` and `--occasion-axes {0}=…` name the same \
                             occasion — give it once: `--occasion-axes` alone selects it",
                            s.name
                        )
                    } else {
                        format!("`--occasion {}` is given twice", s.name)
                    });
                }
                specs.push(s);
                from_axes.push(is_axes);
            }
            Err(e) => {
                let e = if is_axes {
                    let (name, axes) = raw.split_once('@').unwrap_or((raw, ""));
                    e.replace(
                        &format!("`--occasion {raw}`"),
                        &format!("`--occasion-axes {name}={axes}`"),
                    )
                } else {
                    e
                };
                return usage(e);
            }
        }
    }
    let names: Vec<String> = specs.iter().map(|s| s.name.to_string()).collect();
    let mut columns = match columns(&p, &names, args.targets) {
        Ok(c) => c,
        Err(e) => {
            // An occasion named only through `--occasion-axes` is reported
            // under that option.
            let mut e = e;
            for (spec, is_axes) in specs.iter().zip(&from_axes) {
                if *is_axes {
                    let (name, axes) = spec.raw.split_once('@').unwrap_or((spec.raw, ""));
                    e = e.replace(
                        &format!("`--occasion {name}` is not an occasion"),
                        &format!("`--occasion-axes {name}={axes}` is not an occasion"),
                    );
                }
            }
            return usage(e);
        }
    };
    for spec in &specs {
        let decl = p.occasions.get(spec.name);
        let is_target = |t: &str| {
            decl.is_some_and(|d| {
                d.target.takes_target() && lute_check::occasion_target_ok(d, t, &p.kinds).is_ok()
            })
        };
        let pins = match occasion_pins(spec, &resolved, p.index.clock.as_ref(), &is_target) {
            Ok(p) => p,
            Err(e) => {
                let (name, axes) = spec.raw.split_once('@').unwrap_or((spec.raw, ""));
                return usage(e.replace(
                    &format!("`--occasion {}`", spec.raw),
                    &format!("`--occasion-axes {name}={axes}`"),
                ));
            }
        };
        for col in columns.iter_mut().filter(|c| c.occasion == spec.name) {
            col.pins.clone_from(&pins);
        }
    }
    let mut rels: Vec<FactsRel> = Vec::with_capacity(args.relations.len());
    for name in args.relations {
        let Some(r) = p.index.relations.iter().find(|r| &r.name == name) else {
            return usage(unknown_id(
                "`--relation`",
                name,
                "relation",
                p.index.relations.iter().map(|r| r.name.as_str()),
            ));
        };
        if rels.iter().any(|f| &f.name == name) {
            return usage(format!("`--relation {name}` is given twice"));
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
        Err(e) if e.contains("`--until") => return usage(e),
        Err(e) => {
            if let Some(path) = args.script {
                return envelope::fail(CliError::input(path, calendar_human(&e)));
            }
            return usage(e);
        }
    };
    // With `--axis clock`, or at the position a replayed script stopped at,
    // a column of an occasion the clock's `raise:` map raises is judged
    // only where the clock raises it.
    let clock_axis = resolved
        .iter()
        .position(|a| matches!(a.apply, Apply::Clock));
    let by_position = clock_axis.is_some() || origin.replayed > 0;
    let rules: Vec<Option<RaiseRule>> = columns
        .iter()
        .map(
            |c| match (by_position, &p.index.clock, origin.clock_start) {
                (true, Some(clock), Some(start)) => RaiseRule::of(clock, &c.occasion, start),
                _ => None,
            },
        )
        .collect();
    let mut unraised: Vec<(String, String, bool)> = Vec::new();

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
        // OT-F-12: at a value naming no member of its family a tied axis
        // sets nothing, so its first value's cell stands for all of them.
        if resolved
            .iter()
            .zip(&picks)
            .any(|(a, &i)| i > 0 && tied_outside(a, &resolved, &picks).is_some())
        {
            continue;
        }
        let mut w = base.clone();
        let mut at = Vec::with_capacity(resolved.len());
        let mut notes = Vec::new();
        for (axis, &i) in resolved.iter().zip(&picks) {
            let (text, value) = &axis.values[i];
            apply_axis(
                &p,
                &mut w,
                axis,
                text,
                value,
                &written_paths(axis, &resolved, &picks),
            );
            match (tied_outside(axis, &resolved, &picks), axis.apply.family()) {
                (Some(v), Some((family, _))) => {
                    notes.push(format!(
                        "`{v}` names no `{family}` member: `{}` sets nothing here",
                        axis.path
                    ));
                    at.push((axis.path.clone(), "(none)".to_string(), Json::Null));
                }
                _ => at.push((axis.path.clone(), text.clone(), value_to_json(value))),
            }
        }
        let mut eval_driver = super::driver::PlayDriverState::for_project(
            &p,
            BTreeMap::new(),
            BridgeQueues::default(),
        );
        if let Some(h) = advance_quests(&p, &mut w, &mut eval_driver).1 {
            notes.push(format!("quest settle halted — {}", h.message()));
        }
        for (axis, &i) in resolved.iter().zip(&picks) {
            let (text, value) = &axis.values[i];
            let paths = written_paths(axis, &resolved, &picks);
            notes.extend(settled_away(&p, &w, axis, text, value, &paths));
        }
        if let Some(cel) = args.where_ {
            match holds_at(&p, &w, cel) {
                Ok(true) => {}
                Ok(false) => {
                    pruned += 1;
                    continue;
                }
                Err(why) => {
                    let label: Vec<String> = at
                        .iter()
                        .map(|(path, text, _)| format!("{path}={text}"))
                        .collect();
                    return usage(format!(
                        "`--where {cel}` is unknown at the cell {}: {why}",
                        label.join(" ")
                    ));
                }
            }
        }
        let outcomes = columns
            .iter()
            .zip(&rules)
            .map(|(col, rule)| {
                col.applies(&picks).then(|| {
                    let (Some(rule), Some(clock)) = (rule, &p.index.clock) else {
                        return evaluate(&p, &w, col, &mut seen);
                    };
                    let (here, slot_named) = match clock_axis {
                        Some(ci) => (
                            clock_axis_at(clock, &resolved[ci].values[picks[ci]].1),
                            matches!(
                                col.pins.as_ref().map(|pins| &pins[ci]),
                                Some(Pin::Clock {
                                    slot_named: true,
                                    ..
                                })
                            ),
                        ),
                        None => match clock_at(&p, &w) {
                            Some(at) => (at, false),
                            None => return evaluate(&p, &w, col, &mut seen),
                        },
                    };
                    // The last `dayEnd` is raised by the advance that ends
                    // the clock, after `clock.ended` turns true and the
                    // quests settle: judged there, and not raised at all
                    // when the game is over by then.
                    let ending = rule.rule.ending(clock, here).then(|| {
                        let mut ended = w.clone();
                        lute_runtime::clock::set_ended(clock, &mut ended.state, true);
                        let mut eval_driver = super::driver::PlayDriverState::for_project(
                            &p,
                            BTreeMap::new(),
                            BridgeQueues::default(),
                        );
                        advance_quests(&p, &mut ended, &mut eval_driver);
                        ended
                    });
                    let judged = ending.as_ref().unwrap_or(&w);
                    let over = ending.as_ref().and_then(|e| {
                        match lute_runtime::seam::closed(&p, e, &col.occasion, None, None) {
                            Some(lute_runtime::seam::Closed::Terminal(t)) => Some(t),
                            _ => None,
                        }
                    });
                    if over.is_none() && rule.raises(clock, here, slot_named) {
                        return evaluate(&p, judged, col, &mut seen);
                    }
                    // Judged all the same, into a scratch record: a beat
                    // eligible only here is never presented, and says why.
                    // Past the game's end every beat is closed: judged
                    // as the clock stands before it, for the reason.
                    let mut here_seen = BTreeMap::new();
                    evaluate(
                        &p,
                        if over.is_some() { &w } else { judged },
                        col,
                        &mut here_seen,
                    );
                    for (row, s) in here_seen {
                        let into = seen.entry(row).or_default();
                        into.reasons.extend(s.reasons);
                        if !s.eligible {
                            continue;
                        }
                        match &over {
                            Some(t) => {
                                into.reasons.insert(format!(
                                    "eligible at {}, where only the advance that ends the clock \
                                     raises `{}` — after `clock.ended` turns true, when \
                                     `terminal: {t}` already holds and the game is over",
                                    clock.describe(here),
                                    col.occasion
                                ));
                            }
                            None => {
                                into.unraised
                                    .entry(col.occasion.clone())
                                    .or_default()
                                    .insert(here);
                            }
                        }
                    }
                    let why = match &over {
                        Some(t) => format!(
                            "`{}` is not raised at {}: only the advance that ends the clock \
                             raises it there, after `clock.ended` turns true, and `terminal: {t}` \
                             holds by then",
                            col.occasion,
                            clock.describe(here)
                        ),
                        None => rule.describe(clock, &col.occasion),
                    };
                    if !unraised
                        .iter()
                        .any(|(o, w, _)| *o == col.occasion && *w == why)
                    {
                        unraised.push((col.occasion.clone(), why, over.is_none()));
                    }
                    Outcome::not_raised()
                })
            })
            .collect();
        let facts = cell_facts(&p, &w, &rels);
        cells.push(Cell {
            at,
            notes,
            outcomes,
            facts,
        });
    }
    // One reason per occasion, its positions in clock order.
    if let Some(clock) = &p.index.clock {
        for s in seen.values_mut() {
            for (occasion, at) in std::mem::take(&mut s.unraised) {
                s.reasons.insert(format!(
                    "eligible at {}, where the clock does not raise `{occasion}`",
                    describe_positions(clock, &at)
                ));
            }
        }
    }
    let listed = |keep: fn(&Seen) -> bool| -> Vec<(&IndexBeat, &Seen)> {
        seen.iter()
            .filter(|(_, s)| keep(s))
            .map(|(&i, s)| (&p.index.beats[i], s))
            .collect()
    };
    let scope = format!(
        "axes: {}; product: {} cells × {} columns; seed: {}; where: {}",
        resolved
            .iter()
            .map(|a| a.path.as_str())
            .collect::<Vec<_>>()
            .join(", "),
        cells.len(),
        columns.len(),
        origin.describe(),
        args.where_.unwrap_or("none"),
    );
    let report = Report {
        from: origin.describe(),
        scope,
        pruned,
        axes: &resolved,
        columns: &columns,
        cells: &cells,
        rels: &rels,
        never_eligible: listed(|s| !s.eligible),
        never_presented: listed(|s| s.eligible && !s.presented),
        unraised: &unraised,
    };
    let out = if args.json {
        let wrapped =
            crate::envelope::report(crate::envelope::version::CALENDAR, 0, render_json(&report));
        let mut s = serde_json::to_string_pretty(&wrapped).unwrap_or_default();
        s.push('\n');
        s
    } else if args.csv {
        render_csv(&report)
    } else {
        render_text(dir, &report)
    };
    match crate::write_stdout(&out) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => envelope::fail_quietly(CliError::io_without_path(format!(
            "lute calendar: cannot write stdout: {e}"
        ))),
    }
}

/// Everything a rendering reads.
struct Report<'a> {
    /// Where every cell starts ([`Origin::describe`]).
    from: String,
    /// Finite axes/product, replay seed, and optional `--where` filter.
    scope: String,
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
    /// With `--axis clock`: `(occasion, where the clock raises it)` of every
    /// column some cell shows `not raised`.
    unraised: &'a [(String, String, bool)],
}
