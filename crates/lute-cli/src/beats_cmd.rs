//! `lute beats <dir> [--occasion O]… [--target T]… [--json]` (dsl 0.23.0
//! §1): the beat ladder. For every occasion — and, for a targeted one, every
//! target a beat names — the beats that answer it in selection order
//! (priority descending, then project order), each with its priority,
//! `once`, `after:`, `when`, title, and the static verdicts `check-project`
//! reaches about it: unreachable, shadowed, tied, and once-per-run over
//! user state.
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

use lute_check::{BeatOnce, ProjectBeat, ProjectBeatKind};
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
    /// untargeted beats answer any target).
    target: Option<&'a str>,
    targeted: bool,
    select: OccasionSelect,
    /// Indices into the root's selection-ordered beat list.
    beats: Vec<usize>,
}

/// The first backticked name in a message: the beat a verdict names
/// (``scene `k` ``, ``entry `id` ``, ``beat `k` ``).
fn named_beat(message: &str) -> Option<&str> {
    let (_, rest) = message.split_once('`')?;
    rest.split_once('`').map(|(name, _)| name)
}

fn kind_label(kind: ProjectBeatKind) -> &'static str {
    match kind {
        ProjectBeatKind::Scene => "scene",
        ProjectBeatKind::Entry => "entry",
        ProjectBeatKind::Bundle => "bundle",
    }
}

fn once_label(once: BeatOnce) -> &'static str {
    match once {
        BeatOnce::Run => "run",
        BeatOnce::User => "user",
        BeatOnce::None => "no",
        BeatOnce::Day => "day",
        BeatOnce::Slot => "slot",
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
    let (file_results, project_diags, _) = crate::reconcile_collected(file_results, &by_root, false);
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
        let docs: Vec<(PathBuf, lute_syntax::ast::Document)> =
            group.iter().map(|(p, d, _)| (p.clone(), d.clone())).collect();
        let foldeds: Vec<&lute_check::FoldedEnv> = group.iter().map(|(_, _, f)| f).collect();
        let mut beats = lute_check::project_beats(&docs, &foldeds);
        // Selection order: priority descending, project order within (stable).
        beats.sort_by(|a, b| b.priority.cmp(&a.priority));
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
                    .filter(|(p, d)| *p == b.path && named_beat(&d.message) == Some(b.id.as_str()))
                    .map(|(_, d)| *d)
                    .collect()
            })
            .collect();
        let ladders = ladders(&beats, &decls, occasions, targets);
        if json_out {
            roots_json.push(root_json(root, &beats, &verdicts, &ladders));
        } else {
            render_root(&mut text, root, &beats, &verdicts, &ladders, expand);
        }
    }
    if let Some(o) = occasions.iter().find(|o| !known_occasions.contains(*o)) {
        let known: Vec<&str> = known_occasions.iter().map(String::as_str).collect();
        eprintln!(
            "lute beats: `--occasion {o}` is not an occasion of this project (known: {})",
            known.join(", ")
        );
        return ExitCode::from(2);
    }
    let out = if json_out {
        let mut s = serde_json::to_string_pretty(&json!({ "roots": roots_json })).unwrap_or_default();
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
/// target" ladder when none does. `--occasion` / `--target` filter.
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
        let named: BTreeSet<&str> = beats
            .iter()
            .filter(|b| b.on == occ)
            .filter_map(|b| b.target)
            .collect();
        let targeted = decl.map_or(!named.is_empty(), |d| d.target.takes_target());
        let raised: Vec<Option<&str>> = if named.is_empty() {
            vec![None]
        } else {
            named.into_iter().map(Some).collect()
        };
        for target in raised {
            if !targets.is_empty() && !target.is_some_and(|t| targets.iter().any(|x| x == t)) {
                continue;
            }
            let rows = beats
                .iter()
                .enumerate()
                .filter(|(_, b)| {
                    b.on == occ && (b.target.is_none() || b.target == target)
                })
                .map(|(i, _)| i)
                .collect();
            out.push(Ladder {
                occasion: occ,
                target,
                targeted,
                select,
                beats: rows,
            });
        }
    }
    out
}

fn verdict_words(ds: &[&Diagnostic]) -> String {
    let words: BTreeSet<&str> = ds
        .iter()
        .filter_map(|d| VERDICTS.iter().find(|(c, _)| d.code == *c).map(|(_, w)| *w))
        .collect();
    if words.is_empty() {
        "-".to_string()
    } else {
        words.into_iter().collect::<Vec<_>>().join(", ")
    }
}

fn render_root(
    out: &mut String,
    root: &Path,
    beats: &[ProjectBeat<'_>],
    verdicts: &[Vec<&Diagnostic>],
    ladders: &[Ladder<'_>],
    expand: bool,
) {
    let _ = writeln!(out, "project root: {}", root.display());
    if ladders.is_empty() {
        let _ = writeln!(out, "  (no beats)");
        return;
    }
    for ladder in ladders {
        let target = match ladder.target {
            Some(t) => format!(" @ {t}"),
            None if ladder.targeted => " (any target)".to_string(),
            None => String::new(),
        };
        let _ = writeln!(
            out,
            "\n  {}{target} — select: {}",
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
            let mut once = once_label(b.once).to_string();
            if b.also {
                once.push_str(", also");
            }
            let mut id = b.id.clone();
            if let Some(t) = &b.title {
                let _ = write!(id, " \"{t}\"");
            }
            rows.push([
                (rank + 1).to_string(),
                b.priority.to_string(),
                id,
                kind_label(b.kind).to_string(),
                once,
                verdict_words(&verdicts[i]),
                b.after.map_or_else(|| "-".to_string(), one_line),
                when_text(b, expand).unwrap_or_else(|| "-".to_string()),
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

fn root_json(
    root: &Path,
    beats: &[ProjectBeat<'_>],
    verdicts: &[Vec<&Diagnostic>],
    ladders: &[Ladder<'_>],
) -> Json {
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
                    if let Some(t) = b.target {
                        m.insert("target".into(), json!(t));
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
                    if let Some(t) = &b.title {
                        m.insert("title".into(), json!(t));
                    }
                    m.insert(
                        "verdicts".into(),
                        Json::Array(
                            verdicts[i]
                                .iter()
                                .map(|d| {
                                    json!({
                                        "code": d.code,
                                        "severity": crate::severity_str(d.severity),
                                        "message": d.message,
                                    })
                                })
                                .collect(),
                        ),
                    );
                    Json::Object(m)
                })
                .collect();
            let mut m = serde_json::Map::new();
            m.insert("occasion".into(), json!(l.occasion));
            match l.target {
                Some(t) => {
                    m.insert("target".into(), json!(t));
                }
                None if l.targeted => {
                    m.insert("anyTarget".into(), json!(true));
                }
                None => {}
            }
            m.insert("select".into(), json!(l.select.as_str()));
            m.insert("beats".into(), Json::Array(rows));
            Json::Object(m)
        })
        .collect();
    json!({ "root": root.display().to_string(), "ladders": ladders })
}
