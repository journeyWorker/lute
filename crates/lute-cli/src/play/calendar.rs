//! `lute calendar <dir> --axis <path>=<values> … [--occasion O]… [--target
//! T]… [--script save.play.yaml] [--json | --csv]` (dsl 0.23.0 §1, D-A).
//!
//! The calendar is a tool over play, not a schedule file. For every cell of
//! the axes' product it starts from the script's save (0.22.0 §3) — or the
//! declared defaults — writes the cell's values as an `engine:` step would,
//! settles the quest lifecycle, and evaluates every listed occasion (and
//! target) with play's own eligibility, [`super::eligible_at`]: a cell reads
//! exactly what `lute play` would decide there. Cells are independent — no
//! presentation happens, so nothing one cell does is seen by the next.
//!
//! Output: a grid (text), `--json`, or `--csv`; beats that were a candidate
//! somewhere but eligible in no cell are listed at the end with why.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::Path;
use std::process::ExitCode;

use lute_compile::index::IndexBeat;
use lute_manifest::relations::KindShape;
use lute_manifest::schema::{OccasionSelect, OccasionTarget};
use lute_trace::Value;
use serde_json::{json, Value as Json};

use super::{
    advance_quests, compile_project, deciding_unknown, eligible_at, is_candidate, kind_label,
    parse_script_with, presented, resolve_state, seed_world, value_to_json, Candidate, PlayScript,
    Project, Stop, Verdict, World,
};

/// The most cells one invocation evaluates — a typo'd range (`1..70000`)
/// is refused rather than ground through.
const MAX_CELLS: usize = 10_000;

/// clap `value_parser` for `--axis <path>=<values>`: an inclusive integer
/// range `lo..hi` or a comma list `a,b,c`.
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

/// One axis, its values resolved against the declared type.
struct Axis {
    path: String,
    values: Vec<(String, Value)>,
}

/// One evaluated occasion (and target).
struct Column {
    occasion: String,
    target: Option<String>,
    select: OccasionSelect,
}

impl Column {
    fn label(&self) -> String {
        match &self.target {
            Some(t) => format!("{}@{t}", self.occasion),
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

/// One cell: its values, quest-settle note, and one outcome per column.
struct Cell {
    at: Vec<(String, String, Json)>,
    note: Option<String>,
    outcomes: Vec<Outcome>,
}

/// A candidate beat's record across every cell.
#[derive(Default)]
struct Seen {
    eligible: bool,
    reasons: BTreeSet<String>,
}

/// See [`crate::Command::Calendar`].
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_calendar(
    dir: &Path,
    axes: &[(String, Vec<String>)],
    occasions: &[String],
    targets: &[String],
    script: Option<&Path>,
    json_out: bool,
    csv_out: bool,
) -> ExitCode {
    let usage = |msg: String| {
        eprintln!("lute calendar: {msg}");
        ExitCode::from(2)
    };
    let save = match script {
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
    for (path, _) in axes {
        if !seen_paths.insert(path.as_str()) {
            return usage(format!("`--axis {path}` is given twice"));
        }
    }
    if !dir.is_dir() {
        return usage(format!("{} is not a project directory", dir.display()));
    }
    let p = match compile_project(dir) {
        Ok(p) => p,
        Err(code) => return code,
    };
    let mut resolved = Vec::with_capacity(axes.len());
    for (path, values) in axes {
        let mut typed = Vec::with_capacity(values.len());
        for v in values {
            match resolve_state(&p, path, v) {
                Ok(val) => typed.push((v.clone(), val)),
                Err(e) => return usage(format!("`--axis {path}`: `{path}` {e}")),
            }
        }
        resolved.push(Axis {
            path: path.clone(),
            values: typed,
        });
    }
    let cell_count = resolved
        .iter()
        .try_fold(1usize, |n, a| n.checked_mul(a.values.len()))
        .filter(|&n| n <= MAX_CELLS);
    let Some(cell_count) = cell_count else {
        return usage(format!("the axes' product exceeds {MAX_CELLS} cells"));
    };
    let at_script = |e: String| match script {
        Some(s) => format!("{}: {e}", s.display()),
        None => e,
    };
    if let Err(e) = seed_world(&p, &save) {
        return usage(at_script(e));
    }
    let columns = match columns(&p, occasions, targets) {
        Ok(c) => c,
        Err(e) => return usage(e),
    };

    let mut seen: BTreeMap<usize, Seen> = BTreeMap::new();
    let mut cells = Vec::with_capacity(cell_count);
    for n in 0..cell_count {
        // Odometer order: the first axis varies slowest.
        let mut rest = n;
        let mut picks = vec![0; resolved.len()];
        for (i, axis) in resolved.iter().enumerate().rev() {
            picks[i] = rest % axis.values.len();
            rest /= axis.values.len();
        }
        let Ok(mut w) = seed_world(&p, &save) else {
            unreachable!("the save seeded once above")
        };
        let mut at = Vec::with_capacity(resolved.len());
        for (axis, &i) in resolved.iter().zip(&picks) {
            let (text, value) = &axis.values[i];
            w.state.insert(axis.path.clone(), value.clone());
            at.push((axis.path.clone(), text.clone(), value_to_json(value)));
        }
        let note = match advance_quests(&p, &mut w).1 {
            None => None,
            Some(Stop::Halt(h)) => Some(format!("quest settle halted — {}", h.message())),
            Some(Stop::End(reason)) => Some(format!("a quest ended the playthrough: {reason}")),
        };
        let outcomes = columns
            .iter()
            .map(|col| evaluate(&p, &w, col, &mut seen))
            .collect();
        cells.push(Cell { at, note, outcomes });
    }
    let never: Vec<(&IndexBeat, &Seen)> = seen
        .iter()
        .filter(|(_, s)| !s.eligible)
        .map(|(&i, s)| (&p.index.beats[i], s))
        .collect();

    let out = if json_out {
        let mut s = serde_json::to_string_pretty(&render_json(&resolved, &columns, &cells, &never))
            .unwrap_or_default();
        s.push('\n');
        s
    } else if csv_out {
        render_csv(&resolved, &columns, &cells)
    } else {
        render_text(dir, script, &resolved, &columns, &cells, &never)
    };
    match crate::write_stdout(&out) {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::from(2),
    }
}

/// The columns: every listed occasion (default: every occasion a beat
/// answers, by name), each targeted one once per target — the `--target`s
/// it takes, else its declared domain's members, else the targets its beats
/// name.
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
        if !targeted {
            out.push(Column {
                occasion: occ.to_string(),
                target: None,
                select,
            });
            continue;
        }
        let list: Vec<String> = if !targets.is_empty() {
            let mut ok = Vec::new();
            for t in targets {
                let fits = match decl {
                    Some(d) => lute_check::occasion_target_ok(d, t, &p.kinds).is_ok(),
                    None => true,
                };
                if fits {
                    used_targets.insert(t.as_str());
                    ok.push(t.clone());
                }
            }
            ok
        } else {
            let members = match decl.map(|d| &d.target) {
                Some(OccasionTarget::Domain { prefix, entity }) => {
                    match p.kinds.get(entity).map(|k| &k.shape) {
                        Some(KindShape::Members(ms)) => {
                            Some(ms.iter().map(|m| format!("{prefix}.{m}")).collect())
                        }
                        _ => None,
                    }
                }
                _ => None,
            };
            members.unwrap_or_else(|| beat_targets.iter().map(|t| t.to_string()).collect())
        };
        out.extend(list.into_iter().map(|t| Column {
            occasion: occ.to_string(),
            target: Some(t),
            select,
        }));
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
    script: Option<&Path>,
    axes: &[Axis],
    columns: &[Column],
    cells: &[Cell],
    never: &[(&IndexBeat, &Seen)],
) -> String {
    let mut out = String::new();
    let from = script.map_or_else(
        || "declared defaults".to_string(),
        |s| format!("the save in {}", s.display()),
    );
    let _ = writeln!(
        out,
        "calendar: {} — {} cell(s) × {} column(s), from {from}",
        dir.display(),
        cells.len(),
        columns.len()
    );
    out.push('\n');
    // Two header rows: the axis paths and occasions, then the targets.
    let mut rows: Vec<Vec<String>> = Vec::with_capacity(cells.len() + 2);
    let targeted = columns.iter().any(|c| c.target.is_some());
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
        second.extend(columns.iter().map(|c| c.target.clone().unwrap_or_default()));
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
        if let Some(n) = &cell.note {
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

fn render_json(axes: &[Axis], columns: &[Column], cells: &[Cell], never: &[(&IndexBeat, &Seen)]) -> Json {
    let col_json = |c: &Column| {
        let mut m = serde_json::Map::new();
        m.insert("occasion".into(), json!(c.occasion));
        if let Some(t) = &c.target {
            m.insert("target".into(), json!(t));
        }
        m.insert("select".into(), json!(c.select.as_str()));
        m
    };
    json!({
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
            if let Some(n) = &cell.note {
                m.insert("note".into(), json!(n));
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
        ["occasion", "target", "select", "winner", "presented", "shadowed", "unknown", "note"]
            .map(str::to_string),
    );
    let _ = writeln!(out, "{}", head.iter().map(|h| csv_field(h)).collect::<Vec<_>>().join(","));
    for cell in cells {
        for (c, o) in columns.iter().zip(&cell.outcomes) {
            let mut row: Vec<String> = cell.at.iter().map(|(_, text, _)| text.clone()).collect();
            let unknown: Vec<&str> = o.unknown.iter().map(|(id, _)| id.as_str()).collect();
            row.extend([
                c.occasion.clone(),
                c.target.clone().unwrap_or_default(),
                c.select.as_str().to_string(),
                o.winner.clone().unwrap_or_default(),
                o.presented.join(";"),
                o.shadowed.join(";"),
                unknown.join(";"),
                cell.note.clone().unwrap_or_default(),
            ]);
            let _ = writeln!(out, "{}", row.iter().map(|f| csv_field(f)).collect::<Vec<_>>().join(","));
        }
    }
    out
}
