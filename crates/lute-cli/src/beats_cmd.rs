//! `lute beats <dir> [--occasion O]… [--target T]… [--json]` (dsl 0.23.0
//! §1): the beat ladder. For every occasion — and, for a targeted one, every
//! target a beat names — the beats that answer it in selection order
//! (priority descending, then project order), each with its priority,
//! `once`, `after:`, `when`, title, and the static verdicts `check-project`
//! reaches about it: unreachable, shadowed, tied, and once-per-run over
//! user state — plus (dsl 0.26.0 §8) `covered by <id>`, a fallback an
//! earlier never-spent beat whose `when` it implies always beats
//! ([`lute_check::beats::coverers`]; informational, no diagnostic), and
//! (dsl 0.27.0 T3-9) `shadowed by <id>` per ladder cell: the beats that win
//! every time on THAT ladder ([`lute_check::beats::shadowers_at`]), even
//! where the beat wins another target's ladder. A kind beat's row names its
//! `kind:<kind>` target. dsl 0.27.0 §4: a ladder of a gated occasion says
//! when its `raisedWhen` can never hold for the targets it is raised for
//! (all of them, or which members), judged under the root's fact envelope;
//! dsl 0.28.0: a kind or `for=` beat's cell says `never for <target>` where
//! its `when` never holds for the ladder's target, judged per member as
//! `check-project` judges it ([`lute_check::fact_check::beat_never_for`]).
//!
//! Nothing is re-derived. The rows are [`lute_check::project_beats`] — the
//! beat list the project beat passes judge — and the verdicts are the
//! `check-project` diagnostics themselves (per-file and project-wide, the
//! same [`crate::reconcile_collected`] run), attached to the row they name.
//! Documents need not check clean: an unreachable beat is an error, and
//! showing it is the point.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::output::pretty_json;
use lute_check::{ProjectBeat, ProjectBeatKind};
use lute_core_span::Diagnostic;
use lute_manifest::schema::{OccasionDecl, OccasionSelect};
use serde_json::{json, Value as Json};

/// The codes that are verdicts about one beat, with the short word the
/// ladder prints.
const VERDICTS: &[(&str, &str)] = &[
    (lute_check::E_BEAT_UNREACHABLE, "unreachable"),
    (lute_check::E_ENTRY_UNREACHABLE, "unreachable"),
    (lute_check::W_BEAT_SHADOWED, "shadowed"),
    (lute_check::W_BEAT_PRIORITY_TIE, "tied"),
    (lute_check::W_BEAT_ONCE_RUN_USER, "once-run-user"),
];

/// One ladder: an occasion raised with (or without) a target.
struct Ladder<'a> {
    occasion: &'a str,
    /// The target the ladder is raised for; `None` on an untargeted
    /// occasion, or on a targeted one no beat names a target of (its
    /// untargeted beats answer any target). `kind:<kind>` (dsl 0.26.0 §5):
    /// each member of the kind that no beat names on its own.
    target: Option<String>,
    /// The targets the ladder is raised for: the named one, or a kind
    /// ladder's members; empty when raised without a target.
    members: Vec<String>,
    targeted: bool,
    select: OccasionSelect,
    /// Indices into the root's selection-ordered beat list.
    beats: Vec<usize>,
    /// dsl 0.27.0 §4: the occasion's `raisedWhen` gate, with the ladder
    /// targets it can never hold for ([`gate_marks`]).
    gate: Option<GateMark>,
}

/// dsl 0.27.0 §4: an occasion's gate on one ladder.
struct GateMark {
    /// The `raisedWhen` condition as declared.
    raised_when: String,
    /// The gate never holds for any target the ladder is raised for.
    never: bool,
    /// Otherwise the ladder targets (a kind ladder's members) it never
    /// holds for.
    never_for: Vec<String>,
}

/// dsl 0.27.0 §4: each ladder's [`GateMark`] — the gate judged per target
/// ([`lute_check::gates::gate_never_holds`]) in the ladder's first beat's
/// document, under the root's fact envelope when there is one (a
/// `raisedWhen: "holds(canEnter(occasion.target))"` whose fact nothing
/// produces for a room never lets that room's beats play).
fn gate_marks(
    ladders: &mut [Ladder<'_>],
    beats: &[ProjectBeat<'_>],
    env: Option<&lute_check::FactEnv>,
) {
    for ladder in ladders {
        let Some(first) = ladder.beats.first().map(|&i| &beats[i]) else {
            continue;
        };
        let folded = first.folded;
        let Some(gate) = lute_check::gates::gate_of(&folded.occasions, ladder.occasion) else {
            continue;
        };
        let facts = env.map(|e| (e, first.path, first.anchor));
        let dead = |c: &str| lute_check::gates::provably_false(c, folded, facts);
        let never_holds = |target: Option<&str>| {
            lute_check::gates::gate_never_holds(
                &folded.occasions,
                &folded.env.rel_vocab.kinds,
                ladder.occasion,
                target,
                dead,
            )
        };
        let never_for: Vec<String> = ladder
            .members
            .iter()
            .filter(|m| never_holds(Some(m.as_str())))
            .cloned()
            .collect();
        let never = if ladder.members.is_empty() {
            never_holds(None)
        } else {
            never_for.len() == ladder.members.len()
        };
        ladder.gate = Some(GateMark {
            raised_when: gate.to_string(),
            never,
            never_for: if never { Vec::new() } else { never_for },
        });
    }
}

/// The first backticked name in a message: the beat a verdict names
/// (``scene `k` ``, ``entry `id` ``, ``beat `k` ``).
fn named_beat(message: &str) -> Option<&str> {
    let (_, rest) = message.split_once('`')?;
    rest.split_once('`').map(|(name, _)| name)
}

/// Whether verdict `d` (reported in `path`) is about `b`: the beat its
/// message names first, in `b`'s document — or (dsl 0.27.0 T3-11) any beat a
/// `W-BEAT-PRIORITY-TIE` names before `share priority`, in any document
/// (one warning per tied group, anchored at its first beat).
pub(crate) fn names_beat(path: &Path, d: &Diagnostic, b: &ProjectBeat<'_>) -> bool {
    if d.code == lute_check::W_BEAT_PRIORITY_TIE {
        return d
            .message
            .split_once(" share priority ")
            .is_some_and(|(names, _)| names.contains(&b.name()));
    }
    path == b.path && named_beat(&d.message) == Some(b.id.as_str())
}

/// One root's rows and what the ladder cells print about them.
struct Cells<'r, 'a> {
    /// The root's beats in selection order.
    beats: &'r [ProjectBeat<'a>],
    /// The `check-project` verdicts per beat.
    verdicts: &'r [Vec<&'r Diagnostic>],
    covered: &'r [Option<&'r str>],
    /// [`lute_check::beats::always_eligible`] per beat.
    always: &'r [bool],
    /// dsl 0.28.0: per beat, the targets its `when` never holds for
    /// ([`lute_check::fact_check::beat_never_for`]).
    never_for: &'r [Vec<String>],
}

impl Cells<'_, '_> {
    /// dsl 0.27.0 (T3-9): the ids of the beats that win every time row `i`
    /// could on `ladder` — at each ladder target the row answers.
    fn shadowed_by(&self, ladder: &Ladder<'_>, i: usize) -> Option<Vec<&str>> {
        let b = &self.beats[i];
        let targets: Vec<&str> = ladder
            .members
            .iter()
            .map(String::as_str)
            .filter(|t| b.cells().answers(t))
            .collect();
        let found = lute_check::beats::shadowers_at(self.beats, self.always, i, &targets)?;
        Some(
            found
                .into_iter()
                .map(|a| self.beats[a].id.as_str())
                .collect(),
        )
    }

    /// dsl 0.28.0: the targets row `i` never plays for on `ladder` — the
    /// ladder's own (every one on a ladder raised without a target: a `for=`
    /// beat's members) — unless the beat is unreachable outright, which its
    /// own verdict already says.
    fn never_for(&self, ladder: &Ladder<'_>, i: usize) -> Vec<&str> {
        if self.verdicts[i].iter().any(|d| {
            d.code == lute_check::E_BEAT_UNREACHABLE || d.code == lute_check::E_ENTRY_UNREACHABLE
        }) {
            return Vec::new();
        }
        self.never_for[i]
            .iter()
            .filter(|t| ladder.members.is_empty() || ladder.members.contains(t))
            .map(String::as_str)
            .collect()
    }
}

fn evidence_json(evidence: Option<&lute_core_span::Evidence>) -> Option<Json> {
    let evidence = evidence?;
    let mut obj = serde_json::Map::new();
    let value = match evidence {
        lute_core_span::Evidence::Proven => "proven",
        lute_core_span::Evidence::Witnessed => "witnessed",
        lute_core_span::Evidence::Bounded { scope } => {
            obj.insert("scope".into(), json!(scope));
            "bounded"
        }
        lute_core_span::Evidence::Heuristic => "heuristic",
        lute_core_span::Evidence::Unknown => "unknown",
    };
    obj.insert("evidence".into(), json!(value));
    Some(Json::Object(obj))
}

fn kind_label(kind: ProjectBeatKind) -> &'static str {
    match kind {
        ProjectBeatKind::Scene => "scene",
        ProjectBeatKind::Entry => "entry",
        ProjectBeatKind::Bundle => "bundle",
    }
}
/// `when` as one line (a multi-line frontmatter scalar folds to spaces).
fn one_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A beat's `when` as the author wrote it (`@runsAtLeast(2)`), or — with
/// `--expand` — with its `@def`s expanded (dsl 0.24.0 T3-12).
fn when_text(b: &ProjectBeat<'_>, expand: bool) -> Option<String> {
    if expand {
        b.when.as_deref().map(one_line)
    } else {
        b.when_slot.map(|s| one_line(&s.raw))
    }
}

/// See [`crate::Command::Beats`].
pub(crate) fn run_beats(
    dir: &Path,
    occasions: &[String],
    targets: &[String],
    json_out: bool,
    expand: bool,
) -> ExitCode {
    let (file_results, by_root) = match crate::collect_project_docs(dir, None, false) {
        Ok(v) => v,
        Err(code) => return code,
    };
    let (file_results, project_diags, _, fact_envs) =
        crate::reconcile_collected(file_results, &by_root, false);
    let mut verdict_diags: Vec<(&PathBuf, &Diagnostic)> = Vec::new();
    for (path, result) in &file_results {
        verdict_diags.extend(result.diagnostics.iter().map(|d| (path, d)));
    }
    verdict_diags.extend(project_diags.iter().map(|(p, d)| (p, d)));
    verdict_diags.retain(|(_, d)| VERDICTS.iter().any(|(code, _)| d.code == *code));

    let mut text = String::new();
    let mut roots_json = Vec::new();
    let mut known_occasions: BTreeSet<String> = BTreeSet::new();
    for (root, group) in &by_root {
        let docs = lute_model::project_docs(group);
        let foldeds: Vec<&lute_check::FoldedEnv> = group.iter().map(|(_, _, f)| f).collect();
        // Selection order (dsl 0.26.0 §5, dsl 0.27.0 T3-10): priority
        // descending, member > sub-kind > kind, project order within.
        let beats =
            lute_check::beats::in_selection_order(lute_check::project_beats(&docs, &foldeds));
        let mut decls: BTreeMap<&str, &OccasionDecl> = BTreeMap::new();
        for f in &foldeds {
            for (name, d) in &f.occasions {
                decls.entry(name.as_str()).or_insert(d);
            }
        }
        known_occasions.extend(decls.keys().map(|k| k.to_string()));
        known_occasions.extend(beats.iter().map(|b| b.on.to_string()));
        let verdicts: Vec<Vec<&Diagnostic>> = beats
            .iter()
            .map(|b| {
                verdict_diags
                    .iter()
                    .filter(|(p, d)| names_beat(p, d, b))
                    .map(|(_, d)| *d)
                    .collect()
            })
            .collect();
        let always: Vec<bool> = beats
            .iter()
            .map(lute_check::beats::always_eligible)
            .collect();
        let covered: Vec<Option<&str>> = lute_check::beats::coverers(&beats)
            .into_iter()
            .map(|c| c.map(|i| beats[i].id.as_str()))
            .collect();
        let mut ladders = ladders(&beats, &decls, occasions, targets);
        gate_marks(&mut ladders, &beats, fact_envs.get(root));
        // dsl 0.28.0: a kind or `for=` beat's `when` judged per member, as
        // `check-project` judges it.
        let no_facts = lute_check::FactEnv::default();
        let env = fact_envs.get(root).unwrap_or(&no_facts);
        let never_for: Vec<Vec<String>> = beats
            .iter()
            .map(|b| lute_check::fact_check::beat_never_for(b, env))
            .collect();
        let cells = Cells {
            beats: &beats,
            verdicts: &verdicts,
            covered: &covered,
            always: &always,
            never_for: &never_for,
        };
        if json_out {
            roots_json.push(root_json(root, &cells, &ladders));
        } else {
            render_root(&mut text, root, &cells, &ladders, expand);
        }
    }
    if let Some(o) = occasions.iter().find(|o| !known_occasions.contains(*o)) {
        // The target is its own flag here, as in a play step.
        if let Some((name, target)) = o
            .split_once('@')
            .filter(|(name, _)| known_occasions.contains(*name))
        {
            eprintln!(
                "lute beats: `--occasion {o}`: give the target on its own — `--occasion {name} \
                 --target {target}`"
            );
            return ExitCode::from(2);
        }
        let known: Vec<&str> = known_occasions.iter().map(String::as_str).collect();
        eprintln!(
            "lute beats: `--occasion {o}` is not an occasion of this project{} (known: {})",
            lute_manifest::suggest::did_you_mean(o, known.iter().copied()),
            known.join(", ")
        );
        return ExitCode::from(2);
    }
    let out = if json_out {
        let mut s = pretty_json(&json!({ "roots": roots_json })).unwrap_or_default();
        s.push('\n');
        s
    } else if by_root.is_empty() {
        "lute: no .lute files found\n".to_string()
    } else {
        text
    };
    match crate::write_stdout(&out) {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::from(2),
    }
}

/// Every ladder of one root, occasions by name: an untargeted occasion's
/// one ladder, or a targeted occasion's ladder per target its beats name
/// (untargeted beats answer every target, dsl 0.21.0 §4) — one "any
/// target" ladder when none does. A kind beat (dsl 0.26.0 §5) gets one
/// `kind:<kind>` ladder for the members no beat names on its own; a named
/// member's ladder lists it after that member's own beats. `--occasion` /
/// `--target` filter (`--target` may name any member of a kind beat).
fn ladders<'a>(
    beats: &'a [ProjectBeat<'a>],
    decls: &BTreeMap<&str, &OccasionDecl>,
    occasions: &[String],
    targets: &[String],
) -> Vec<Ladder<'a>> {
    let answered: BTreeSet<&str> = beats.iter().map(|b| b.on).collect();
    let mut out = Vec::new();
    for occ in answered {
        if !occasions.is_empty() && !occasions.iter().any(|o| o == occ) {
            continue;
        }
        let decl = decls.get(occ);
        let select = decl.map_or(OccasionSelect::First, |d| d.select);
        let on_occ = || beats.iter().enumerate().filter(move |(_, b)| b.on == occ);
        let named: BTreeSet<&str> = on_occ()
            .filter_map(|(_, b)| match b.cells() {
                lute_check::beats::BeatCells::One(t) => Some(t),
                _ => None,
            })
            .collect();
        // Each kind target with the members no beat names on its own.
        let mut kinds: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for (_, b) in on_occ() {
            if let (Some(label), lute_check::beats::BeatCells::Kind(ms)) = (b.target, b.cells()) {
                let rest = kinds.entry(label).or_default();
                for m in ms {
                    if !named.contains(m.as_str()) && !rest.contains(&m.as_str()) {
                        rest.push(m);
                    }
                }
            }
        }
        let targeted = decl.map_or(!named.is_empty() || !kinds.is_empty(), |d| {
            d.target.takes_target()
        });
        let rows_for = |pred: &dyn Fn(lute_check::beats::BeatCells<'_>) -> bool| -> Vec<usize> {
            on_occ()
                .filter(|(_, b)| pred(b.cells()))
                .map(|(i, _)| i)
                .collect()
        };
        let mut push = |target: Option<String>, members: Vec<String>, rows: Vec<usize>| {
            out.push(Ladder {
                occasion: occ,
                target,
                members,
                targeted,
                select,
                beats: rows,
                gate: None,
            });
        };
        if !targets.is_empty() {
            for t in targets {
                let raised = on_occ().any(|(_, b)| {
                    !matches!(b.cells(), lute_check::beats::BeatCells::Any) && b.cells().answers(t)
                });
                if raised {
                    push(
                        Some(t.clone()),
                        vec![t.clone()],
                        rows_for(&|c| c.answers(t)),
                    );
                }
            }
            continue;
        }
        if named.is_empty() && kinds.is_empty() {
            push(
                None,
                Vec::new(),
                rows_for(&|c| matches!(c, lute_check::beats::BeatCells::Any)),
            );
            continue;
        }
        for t in &named {
            push(
                Some(t.to_string()),
                vec![t.to_string()],
                rows_for(&|c| c.answers(t)),
            );
        }
        for (label, rest) in &kinds {
            if rest.is_empty() {
                continue;
            }
            push(
                Some(label.to_string()),
                rest.iter().map(|m| m.to_string()).collect(),
                rows_for(&|c| rest.iter().any(|m| c.answers(m))),
            );
        }
    }
    out
}

/// The verdict cell: the `check-project` verdicts about the beat, with
/// (dsl 0.27.0 T3-9) `shadowed by <id>` where this ladder's earlier beats
/// win every time it could — even when it wins on another ladder — and
/// (dsl 0.28.0) `never for <target>` where its `when` never holds for this
/// ladder's target.
fn verdict_words(
    ds: &[&Diagnostic],
    covered: Option<&str>,
    shadowed_by: Option<&[&str]>,
    never_for: &[&str],
) -> String {
    let words: BTreeSet<&str> = ds
        .iter()
        .filter_map(|d| VERDICTS.iter().find(|(c, _)| d.code == *c).map(|(_, w)| *w))
        .collect();
    let mut words: Vec<String> = words
        .into_iter()
        .map(|w| match (w, shadowed_by) {
            ("shadowed", Some(by)) => format!("shadowed by {} [heuristic]", by.join(" / ")),
            _ => w.to_string(),
        })
        .collect();
    if let Some(by) =
        shadowed_by.filter(|_| !words.iter().any(|w| w.starts_with("shadowed")))
    {
        words.insert(0, format!("shadowed by {} [heuristic]", by.join(" / ")));
    }
    if !never_for.is_empty() {
        words.insert(0, format!("never for {}", never_for.join(" / ")));
    }
    if let Some(id) = covered {
        words.push(format!("covered by {id} [heuristic]"));
    }
    if words.is_empty() {
        "-".to_string()
    } else {
        words.join(", ")
    }
}

fn render_root(
    out: &mut String,
    root: &Path,
    cells: &Cells<'_, '_>,
    ladders: &[Ladder<'_>],
    expand: bool,
) {
    let beats = cells.beats;
    let _ = writeln!(out, "project root: {}", root.display());
    if ladders.is_empty() {
        let _ = writeln!(out, "  (no beats)");
        return;
    }
    for ladder in ladders {
        let target = match &ladder.target {
            Some(t) => format!(" @ {t}"),
            None if ladder.targeted => " (any target)".to_string(),
            None => String::new(),
        };
        // dsl 0.27.0 §4: the occasion's gate, and the targets it never
        // lets play.
        let gate = match &ladder.gate {
            Some(g) if g.never => format!(" · raisedWhen: {} · gate never holds", g.raised_when),
            Some(g) if !g.never_for.is_empty() => format!(
                " · raisedWhen: {} · gate never holds for {}",
                g.raised_when,
                g.never_for.join(", ")
            ),
            Some(g) => format!(" · raisedWhen: {}", g.raised_when),
            None => String::new(),
        };
        let _ = writeln!(
            out,
            "\n  {}{target} — select: {}{gate}",
            ladder.occasion,
            ladder.select.as_str()
        );
        let mut rows: Vec<[String; 8]> = vec![[
            "#".into(),
            "priority".into(),
            "beat".into(),
            "kind".into(),
            "once".into(),
            "verdict".into(),
            "after".into(),
            "when".into(),
        ]];
        for (rank, &i) in ladder.beats.iter().enumerate() {
            let b = &beats[i];
            // A `spentBy` beat is spent once its condition has held, for its
            // `once` period (`run` unless written — then it is named).
            let mut once = match &b.spent_by {
                Some(by) if b.once == lute_check::BeatOnce::Run => {
                    format!("spentBy: {}", one_line(by))
                }
                Some(by) => format!("spentBy: {}, {}", one_line(by), b.once.as_str()),
                None => b.once.as_str().into_owned(),
            };
            if b.also {
                once.push_str(", also");
            }
            // dsl 0.25.0 §2: the spend this beat shares.
            if let Some((key, _)) = b.share {
                let _ = write!(once, ", share {key}");
            }
            let mut id = b.id.clone();
            // dsl 0.27.0 (T3-9): a kind beat's row says which kind it
            // answers the ladder's target through.
            if let (Some(t), lute_check::beats::BeatCells::Kind(_)) = (b.target, b.cells()) {
                let _ = write!(id, " ({t})");
            }
            // A `for` beat is presented once per member of its kind.
            if let Some((raw, _)) = b.for_kind {
                let _ = write!(id, " (for {raw})");
            }
            if let Some(t) = &b.title {
                let _ = write!(id, " \"{t}\"");
            }
            let shadowed_by = cells.shadowed_by(ladder, i);
            // A use's own `when=` replaces its template's `when:` whole.
            let mut when = when_text(b, expand).unwrap_or_else(|| "-".to_string());
            if let Some((template, dropped)) = b.replaces_when {
                let _ = write!(
                    when,
                    " (replaces template `{template}`'s `when: {}`)",
                    one_line(dropped)
                );
            }
            rows.push([
                (rank + 1).to_string(),
                b.priority.to_string(),
                id,
                kind_label(b.kind).to_string(),
                once,
                verdict_words(
                    &cells.verdicts[i],
                    cells.covered[i],
                    shadowed_by.as_deref(),
                    &cells.never_for(ladder, i),
                ),
                b.after.map_or_else(|| "-".to_string(), one_line),
                when,
            ]);
        }
        let widths: Vec<usize> = (0..8)
            .map(|c| rows.iter().map(|r| r[c].chars().count()).max().unwrap_or(0))
            .collect();
        for r in &rows {
            let mut line = String::from("   ");
            for (c, cell) in r.iter().enumerate() {
                if c == 7 {
                    line.push(' ');
                    line.push_str(cell);
                } else {
                    let _ = write!(line, " {cell:<w$} ", w = widths[c]);
                }
            }
            out.push_str(line.trim_end());
            out.push('\n');
        }
    }
    out.push('\n');
}

fn root_json(root: &Path, cells: &Cells<'_, '_>, ladders: &[Ladder<'_>]) -> Json {
    let (beats, verdicts, covered) = (cells.beats, cells.verdicts, cells.covered);
    let rel = |p: &Path| p.strip_prefix(root).unwrap_or(p).display().to_string();
    let ladders: Vec<Json> = ladders
        .iter()
        .map(|l| {
            let rows: Vec<Json> = l
                .beats
                .iter()
                .map(|&i| {
                    let b = &beats[i];
                    let mut m = serde_json::Map::new();
                    m.insert("id".into(), json!(b.id));
                    m.insert("kind".into(), json!(kind_label(b.kind)));
                    m.insert("document".into(), json!(rel(b.path)));
                    m.insert("priority".into(), json!(b.priority));
                    m.insert("once".into(), json!(b.once.as_str()));
                    if b.also {
                        m.insert("also".into(), json!(true));
                    }
                    if let Some((key, _)) = b.share {
                        m.insert("share".into(), json!(key));
                    }
                    if let Some(by) = &b.spent_by {
                        m.insert("spentBy".into(), json!(by));
                    }
                    if let Some(t) = b.target {
                        m.insert("target".into(), json!(t));
                    }
                    // The compiled index's `forKind`, beside the authored `for`.
                    if let Some((raw, kind)) = &b.for_kind {
                        m.insert("for".into(), json!(raw));
                        if let Some((kind, members)) = kind {
                            m.insert(
                                "forKind".into(),
                                json!({ "kind": kind, "members": members }),
                            );
                        }
                    }
                    if let Some(a) = b.after {
                        m.insert("after".into(), json!(a));
                    }
                    if let Some(w) = &b.when {
                        m.insert("when".into(), json!(w));
                    }
                    // T3-12: the author's text beside the expansion.
                    if let Some(w) = when_text(b, false) {
                        m.insert("whenAuthored".into(), json!(w));
                    }
                    if let Some((template, dropped)) = b.replaces_when {
                        m.insert(
                            "replacesTemplateWhen".into(),
                            json!({ "template": template, "when": one_line(dropped) }),
                        );
                    }
                    if let Some(t) = &b.title {
                        m.insert("title".into(), json!(t));
                    }
                    if let Some(id) = covered[i] {
                        m.insert("coveredBy".into(), json!(id));
                        m.insert("coveredByEvidence".into(), json!("heuristic"));
                    }
                    // dsl 0.27.0 (T3-9): this ladder's own verdict.
                    if let Some(by) = cells.shadowed_by(l, i) {
                        m.insert("shadowedBy".into(), json!(by));
                        m.insert("shadowedByEvidence".into(), json!("heuristic"));
                    }
                    // dsl 0.28.0: the ladder's targets its `when` never
                    // holds for.
                    let never_for = cells.never_for(l, i);
                    if !never_for.is_empty() {
                        m.insert("neverFor".into(), json!(never_for));
                    }
                    m.insert(
                        "verdicts".into(),
                        Json::Array(
                            verdicts[i]
                                .iter()
                                .map(|d| {
                                    let mut v = serde_json::Map::new();
                                    v.insert("code".into(), json!(d.code));
                                    v.insert("severity".into(), json!(crate::severity_str(d.severity)));
                                    v.insert("message".into(), json!(d.text()));
                                    if let Some(fields) = evidence_json(d.evidence.as_ref()) {
                                        if let Json::Object(fields) = fields {
                                            v.extend(fields);
                                        }
                                    }
                                    Json::Object(v)
                                })
                                .collect(),
                        ),
                    );
                    Json::Object(m)
                })
                .collect();
            let mut m = serde_json::Map::new();
            m.insert("occasion".into(), json!(l.occasion));
            match &l.target {
                Some(t) => {
                    m.insert("target".into(), json!(t));
                }
                None if l.targeted => {
                    m.insert("anyTarget".into(), json!(true));
                }
                None => {}
            }
            m.insert("select".into(), json!(l.select.as_str()));
            // dsl 0.27.0 §4: the occasion's gate, and where it never holds.
            if let Some(g) = &l.gate {
                m.insert("raisedWhen".into(), json!(g.raised_when));
                if g.never {
                    m.insert("gateNeverHolds".into(), json!(true));
                } else if !g.never_for.is_empty() {
                    m.insert("gateNeverHoldsFor".into(), json!(g.never_for));
                }
            }
            m.insert("beats".into(), Json::Array(rows));
            Json::Object(m)
        })
        .collect();
    json!({ "root": root.display().to_string(), "ladders": ladders })
}
