//! `lute-bench flow-loops`: the loop-head fixed point cost grid (dsl 0.51.0
//! §3.4, §3.8) and the hub head axis (dsl 0.52.0 §5.4).
//!
//! Generated scenes, each a nest of bounded loops: every loop's region holds
//! `R` nodes of its own — its head label, a show of `mira`, lines, her exit,
//! a `::retract{met(mira)}` late in the body (so the fact head changes once)
//! and the bounded jump — with the next loop nested halfway through, to
//! depth `D`. Two axes:
//!
//! - region size R ∈ {100, 200, 400} at depth 1: each doubling costs ≤ 2.3×;
//! - depth D ∈ {1, 2, 3} at R = 100: within the product bound
//!   Σₖ (P + 1)^(k−1) · P · Nₖ, where `Nₖ` is the size of the depth-`k`
//!   region (nested regions included) and `P` the most probe passes any head
//!   took — a head at depth `k` is iterated once per probe and final pass of
//!   each enclosing head.
//!
//! A control point, the R = 100 document without a loop (a forward jump and
//! its label instead), must record no head and no probe work in any walk.
//!
//! The `hub` axis: a nest of hubs of `R` non-exit options each (a show of
//! `mira`, a line, her exit — so the hub head changes once) and one exit
//! option, the next hub nested in the first option, to depth `D`; points
//! R ∈ {8, 16, 32} at D = 1 and D ∈ {2, 3} at R = 8. Only the stage walks
//! iterate a hub head. Each doubling of R costs ≤ 2.3× in work (gated);
//! its time ratio is reported, not gated. Every point is within the product
//! bound above, `Nₖ` the size of the depth-`k` hub.
//!
//! Each point runs the walks on the calling thread — `check()` (definite
//! assignment, the stage fold, cast presence), the project fact pass
//! (`compute_must`) and `lute compile`'s stage walk — and reads their
//! [`lute_check::LoopWalkStats`]. `work` (probe nodes) is deterministic; the
//! times are the best of `--samples` runs of the three phases in the
//! profile the binary was built with (the dev profile in RG: growth ratios,
//! not optimized timings). `--check work` gates on the work only; `--check
//! both` (the default) also gates the region axis's time ratios.

use std::hint::black_box;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use lute_check::connectivity::{
    ambiguous_quest_ids, assemble_graph, check_reachability, live_assert_sites, quest_id_set,
    scene_key_set, unreachable_quest_ids,
};
use lute_check::{
    check, compute_must, fold_env, loop_walk_stats, reset_loop_walk_stats, stable_seeds,
    CheckInput, CheckResult, LoopWalkStats, MaySet, Mode, ProjectDocs, RootVocab, WalkKind,
};
use serde::Serialize;

const USAGE: &str =
    "usage: lute-bench flow-loops [--samples <n>] [--check work|both] [--json <path>|-]";

/// The bound on a region doubling's cost ratio.
const DOUBLING: f64 = 2.3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Check {
    Work,
    Both,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Walk {
    kind: &'static str,
    heads: u64,
    probe_passes: u64,
    probe_nodes: u64,
    max_head_passes: u64,
    widened: u64,
    /// The product bound on `probe_nodes` (see the module doc).
    product_bound: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Point {
    axis: &'static str,
    r: usize,
    d: usize,
    /// Node rows of the outermost region (nested regions included).
    region_nodes: usize,
    /// Probe nodes over every walk.
    work: u64,
    walks: Vec<Walk>,
    best_ms: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Ratio {
    axis: &'static str,
    from: String,
    to: String,
    bound: f64,
    work: f64,
    time: f64,
    work_within: bool,
    time_within: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Report {
    schema_version: &'static str,
    samples: usize,
    points: Vec<Point>,
    ratios: Vec<Ratio>,
    pass: bool,
}

/// `lute-bench flow-loops [flags]`: returns the process exit code.
pub(crate) fn main(args: &[String]) -> i32 {
    let mut samples = 7usize;
    let mut check = Check::Both;
    let mut json = "-".to_string();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        let value = match args.next() {
            Some(v) if arg.starts_with("--") => v,
            _ => return usage(&format!("unexpected argument `{arg}`")),
        };
        match arg.as_str() {
            "--samples" => match value.parse() {
                Ok(n) if n > 0 => samples = n,
                _ => return usage("--samples must be a positive integer"),
            },
            "--check" => {
                check = match value.as_str() {
                    "work" => Check::Work,
                    "both" => Check::Both,
                    _ => return usage("--check is `work` or `both`"),
                }
            }
            "--json" => json.clone_from(value),
            other => return usage(&format!("unknown argument `{other}`")),
        }
    }
    let report = run(samples, check);
    let mut text = match serde_json::to_string_pretty(&report) {
        Ok(text) => text,
        Err(error) => {
            eprintln!("lute-bench: cannot serialize report: {error}");
            return 2;
        }
    };
    text.push('\n');
    if json == "-" {
        print!("{text}");
    } else if let Err(error) = std::fs::write(&json, text) {
        eprintln!("lute-bench: cannot write {json}: {error}");
        return 2;
    }
    i32::from(!report.pass)
}

fn usage(error: &str) -> i32 {
    eprintln!("lute-bench: {error}");
    eprintln!("{USAGE}");
    2
}

fn run(samples: usize, check: Check) -> Report {
    let mut points = Vec::new();
    let mut ratios = Vec::new();
    points.push(measure("control", 100, 0, samples));
    for r in [100, 200, 400] {
        points.push(measure("region", r, 1, samples));
    }
    for d in [1, 2, 3] {
        points.push(measure("depth", 100, d, samples));
    }
    for r in [8, 16, 32] {
        points.push(measure("hub", r, 1, samples));
    }
    for d in [2, 3] {
        points.push(measure("hub", 8, d, samples));
    }
    let mut pass = points[0]
        .walks
        .iter()
        .all(|w| w.heads == 0 && w.probe_nodes == 0);
    // The region axis gates work and (with `--check both`) time; the hub
    // axis gates work and reports time.
    for (axis, from, to) in [
        ("region", 1, 2),
        ("region", 2, 3),
        ("hub", 7, 8),
        ("hub", 8, 9),
    ] {
        let (a, b) = (&points[from], &points[to]);
        #[expect(
            clippy::cast_precision_loss,
            reason = "an operation count of a generated document, far below 2^52"
        )]
        let work = b.work as f64 / a.work as f64;
        let time = b.best_ms / a.best_ms;
        let ratio = Ratio {
            axis,
            from: label(a),
            to: label(b),
            bound: DOUBLING,
            work: round3(work),
            time: round3(time),
            work_within: work <= DOUBLING,
            time_within: time <= DOUBLING,
        };
        let time_gated = axis == "region" && check == Check::Both;
        pass &= ratio.work_within && (!time_gated || ratio.time_within);
        ratios.push(ratio);
    }
    for p in &points {
        pass &= p
            .walks
            .iter()
            .all(|w| w.widened == 0 && w.probe_nodes <= w.product_bound);
    }
    Report {
        schema_version: "0.52.0.flow-loops",
        samples,
        points,
        ratios,
        pass,
    }
}

fn label(p: &Point) -> String {
    format!("R={} D={}", p.r, p.d)
}

fn round3(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}

/// The core snapshot with a cast whose `mira` is present while she has met
/// the player (so the presence walk runs).
fn snapshot() -> lute_manifest::snapshot::CapabilitySnapshot {
    let mut snapshot = lute_manifest::core::load_core_snapshot();
    snapshot.cast.insert(
        "mira".into(),
        lute_manifest::schema::CastMember {
            id: "mira".into(),
            name: Some("Mira".into()),
            present: Some("holds('met', ['mira'])".into()),
            emotions: None,
            assume: None,
            shared_name: None,
        },
    );
    snapshot
}

fn input_for(text: &str) -> CheckInput {
    CheckInput::new(
        text.to_string(),
        "grid.lute".into(),
        Mode::Author,
        lute_check::InputSources::from_parts(
            snapshot(),
            Default::default(),
            lute_check::SchemaImports::default(),
            Default::default(),
            Default::default(),
        ),
    )
}

/// The inputs of the project fact pass over the one document, built once
/// per point (outside the timed phases).
struct FactPass {
    docs: ProjectDocs<'static>,
    folded: lute_check::FoldedEnv,
    graph: lute_check::connectivity::ConnGraph,
    vocab: RootVocab,
    may: MaySet,
}

impl FactPass {
    fn new(input: &CheckInput, result: CheckResult) -> Self {
        let path = PathBuf::from("grid.lute");
        let (doc, _) = lute_syntax::parse(&input.text);
        let (folded, _, _) = fold_env(&doc, input);
        let docs = ProjectDocs::parse(
            vec![(path.clone(), doc)],
            lute_manifest::core::core_snapshot(),
        );
        let views = docs.views();
        let key_set = scene_key_set(&views);
        let quest_ids = quest_id_set(&views);
        let (graph, _) = assemble_graph(
            &views,
            &key_set,
            &quest_ids,
            &lute_check::raises::RaiseAnchors::default(),
        );
        let results = [(path, result)];
        let lifecycle = unreachable_quest_ids(&views, &results);
        let ambiguous = ambiguous_quest_ids(&views);
        let (reach, _) = check_reachability(&graph, &quest_ids, &ambiguous, &lifecycle);
        let mut vocab = RootVocab::default();
        vocab.add(&folded.env.rel_vocab, &folded.env.domains);
        vocab.note_unreadable_documents(&views);
        let facts: Vec<_> =
            live_assert_sites(&views, &reach, &ambiguous, &lifecycle, &Default::default())
                .into_iter()
                .flat_map(|(_, a)| vocab.asserted_facts(&a))
                .collect();
        let may = MaySet::build(&vocab, facts, &stable_seeds(&views, &vocab));
        FactPass {
            docs,
            folded,
            graph,
            vocab,
            may,
        }
    }

    fn run(&self) {
        let must = compute_must(
            &self.docs.views(),
            &[&self.folded],
            &self.graph,
            &self.vocab,
            &self.may,
        );
        black_box(must);
    }
}

fn measure(axis: &'static str, r: usize, d: usize, samples: usize) -> Point {
    let (text, region_nodes, sizes) = if axis == "hub" {
        hub_document(r, d)
    } else {
        document(r, d)
    };
    let input = input_for(&text);
    let checked = check(&input);
    assert!(
        checked.ok,
        "the generated document checks clean: {:?}",
        checked.diagnostics
    );
    let facts = FactPass::new(&input, checked.clone());
    let mut best = Duration::MAX;
    let mut stats = Vec::new();
    for _ in 0..samples {
        reset_loop_walk_stats();
        let start = Instant::now();
        let result = check(&input);
        facts.run();
        let compiled = lute_compile::compile_with_check(
            &input,
            result,
            &lute_manifest::project::IdentityTemplates::default(),
        );
        best = best.min(start.elapsed());
        assert!(compiled.is_ok(), "the generated document compiles");
        let _ = black_box(compiled);
        stats = WalkKind::ALL
            .iter()
            .map(|&kind| (kind, loop_walk_stats(kind)))
            .collect();
    }
    let walks: Vec<Walk> = stats
        .into_iter()
        .map(|(kind, s)| walk(kind, &s, &sizes))
        .collect();
    Point {
        axis,
        r,
        d,
        region_nodes,
        work: walks.iter().map(|w| w.probe_nodes).sum(),
        walks,
        best_ms: round3(best.as_secs_f64() * 1000.0),
    }
}

/// The report row of one walk, with its product bound over the region
/// sizes `sizes` (outermost first).
fn walk(kind: WalkKind, s: &LoopWalkStats, sizes: &[usize]) -> Walk {
    let p = s.max_head_passes;
    let mut enclosing = 1u64;
    let mut bound = 0u64;
    for &n in sizes {
        bound += enclosing * p * u64::try_from(n).expect("a generated region size fits u64");
        enclosing *= p + 1;
    }
    Walk {
        kind: kind.name(),
        heads: s.heads,
        probe_passes: s.probe_passes,
        probe_nodes: s.work(),
        max_head_passes: p,
        widened: s.widened,
        product_bound: bound,
    }
}

/// The scene of point `(r, d)`, the node count of its outermost region and
/// each depth's region size (nested regions included), outermost first. At
/// `d == 0` the body is the same size, with a forward jump and its label
/// instead of a loop.
fn document(r: usize, d: usize) -> (String, usize, Vec<usize>) {
    use std::fmt::Write as _;

    /// The nodes of a region that are not its lines: label, show, exit,
    /// retract and jump.
    const FIXED: usize = 5;
    let mut out = format!("{FRONT}{START}");
    let lines = r.saturating_sub(FIXED);
    let mut line = 0usize;
    let mut next_line = |out: &mut String| {
        writeln!(out, "@mira: Line {line}.").expect("a String accepts writes");
        line += 1;
    };
    if d == 0 {
        out.push_str("::jump{to=\"past\" when=\"run.f\"}\n");
        for _ in 0..lines {
            next_line(&mut out);
        }
        out.push_str("::label{name=\"past\"}\n@mira: Done.\n");
        return (out, 0, Vec::new());
    }
    for k in 1..=d {
        writeln!(
            out,
            "::label{{name=\"l{k}\"}}\n::actor{{character=\"mira\" action=\"show\"}}"
        )
        .expect("a String accepts writes");
        for _ in 0..lines / 2 {
            next_line(&mut out);
        }
    }
    for k in (1..=d).rev() {
        for _ in lines / 2..lines {
            next_line(&mut out);
        }
        writeln!(
            out,
            "::actor{{character=\"mira\" action=\"hide\"}}\n::retract{{met(mira)}}\n\
             ::jump{{to=\"l{k}\" max=\"2\"}}"
        )
        .expect("a String accepts writes");
    }
    out.push_str("::actor{character=\"mira\" action=\"show\"}\n@mira: Done.\n");
    let own = FIXED + lines;
    let sizes: Vec<usize> = (1..=d).map(|k| own * (d - k + 1)).collect();
    (out, sizes[0], sizes)
}

/// The scene's start: `mira` met (so the presence walk admits her) and shown.
const START: &str = "::assert{met(mira)}\n::actor{character=\"mira\" action=\"show\"}\n";

/// The scene's front matter and heading, shared by both generators.
const FRONT: &str = "---\nkind: scene\nid: grid\nentities:\n  person: { members: [mira] }\n\
                     relations:\n  met: { args: [person], tier: run }\n\
                     state:\n  run.f: { type: bool, default: false }\n\
                     enums:\n  action:\n    members: [show, hide]\n    exits: [hide]\n  \
                     anchor:\n    members: [left, center, right]\n    default: center\n\
                     ---\n## Shot 1.\n";

/// The `hub` axis scene of point `(r, d)`: a nest of `d` hubs of `r`
/// non-exit options and one exit option, the next hub in the first option;
/// as [`document`], the outermost hub's node count and each depth's.
fn hub_document(r: usize, d: usize) -> (String, usize, Vec<usize>) {
    use std::fmt::Write as _;

    fn hub(out: &mut String, k: usize, r: usize, d: usize) {
        writeln!(out, "<hub id=\"h{k}\">").expect("a String accepts writes");
        for i in 0..r {
            writeln!(out, "<choice id=\"o{i}\" text=\"Option {i}.\">")
                .expect("a String accepts writes");
            if i == 0 && k < d {
                hub(out, k + 1, r, d);
            }
            writeln!(
                out,
                "::actor{{character=\"mira\" action=\"show\"}}\n@mira: Line {k}.{i}.\n\
                 ::actor{{character=\"mira\" action=\"hide\"}}\n</choice>"
            )
            .expect("a String accepts writes");
        }
        out.push_str(
            "<choice id=\"leave\" text=\"Leave.\" exit>\n@narrator: Bye.\n</choice>\n</hub>\n",
        );
    }

    let mut out = format!("{FRONT}{START}");
    hub(&mut out, 1, r, d);
    out.push_str("@narrator: Done.\n");
    // The hub node, three nodes per option, the exit option's line.
    let own = 3 * r + 2;
    let sizes: Vec<usize> = (1..=d).map(|k| own * (d - k + 1)).collect();
    (out, sizes[0], sizes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_region_holds_r_nodes_of_its_own() {
        let (text, outer, sizes) = document(100, 2);
        assert_eq!((outer, sizes), (200, vec![200, 100]));
        assert_eq!(text.matches("::label").count(), 2);
        assert_eq!(text.matches("max=\"2\"").count(), 2);
        let (control, _, none) = document(100, 0);
        assert!(none.is_empty());
        assert!(!control.contains("max="));
    }

    #[test]
    fn a_hub_holds_r_options_and_an_exit() {
        let (text, outer, sizes) = hub_document(8, 2);
        assert_eq!((outer, sizes), (52, vec![52, 26]));
        assert_eq!(text.matches("<hub ").count(), 2);
        assert_eq!(text.matches("<choice ").count(), 18);
        assert_eq!(text.matches(" exit>").count(), 2);
    }
}
