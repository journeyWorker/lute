//! `lute-bench lint-nodes`: the `node` lint cost grid (dsl 0.50.0 §8.1).
//!
//! Generated documents, one `node` rule reading `preceding` by key. Three
//! axes, each with the other two fixed:
//!
//! - node count N ∈ {5 000, 10 000, 20 000} at T = 10 tags, depth D = 3:
//!   each doubling costs ≤ 2.3×;
//! - distinct tags T ∈ {10, 100, 1 000} at N = 10 000, D = 3: 10 → 1 000
//!   costs ≤ 2×;
//! - depth D ∈ {2, 8, 32} at N = 10 000, T = 10: 2 → 32 costs ≤ 2×.
//!
//! Two costs are measured per point: `work`, the walk's operation count
//! ([`lute_lint::nodes::NodeWalkStats::work`], deterministic), and the best
//! of `--samples` wall times of one `lute_lint::lint` call. `--check work`
//! gates on the operation counts only (stable in CI); `--check both` (the
//! default) also gates on the times. Whole-map reads of `preceding` must be
//! zero at every point.

use std::hint::black_box;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use lute_lint::nodes::NodeWalkStats;
use lute_lint::{lint, LintConfig, LintDocInput, LintLevel, LintRuleDecl, LintScope, LintTarget};
use serde::Serialize;

const USAGE: &str =
    "usage: lute-bench lint-nodes [--samples <n>] [--check work|both] [--json <path>|-]";

/// The rule every point runs: a directive whose tag was seen before on its
/// path with the same `value`.
const WHEN: &str = r#"node.kind == "directive" && "value" in node.attrs && node.tag in node.preceding && "value" in node.preceding[node.tag].attrs && node.preceding[node.tag].attrs["value"] == node.attrs["value"]"#;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Check {
    Work,
    Both,
}

/// One grid axis: its name, its points `(n, t, d)` and the bound on the
/// cost ratio between the points it compares.
struct Axis {
    name: &'static str,
    points: [(usize, usize, usize); 3],
    /// `(from, to, bound)` indexes into `points`.
    ratios: &'static [(usize, usize, f64)],
}

const AXES: [Axis; 3] = [
    Axis {
        name: "nodes",
        points: [(5_000, 10, 3), (10_000, 10, 3), (20_000, 10, 3)],
        ratios: &[(0, 1, 2.3), (1, 2, 2.3)],
    },
    Axis {
        name: "tags",
        points: [(10_000, 10, 3), (10_000, 100, 3), (10_000, 1_000, 3)],
        ratios: &[(0, 2, 2.0)],
    },
    Axis {
        name: "depth",
        points: [(10_000, 10, 2), (10_000, 10, 8), (10_000, 10, 32)],
        ratios: &[(0, 2, 2.0)],
    },
];

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Point {
    axis: &'static str,
    n: usize,
    t: usize,
    d: usize,
    rows: u64,
    work: u64,
    preceding_materialized: u64,
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
    when: &'static str,
    points: Vec<Point>,
    ratios: Vec<Ratio>,
    pass: bool,
}

/// `lute-bench lint-nodes [flags]`: returns the process exit code.
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
    let mut pass = true;
    for axis in &AXES {
        let first = points.len();
        for &(n, t, d) in &axis.points {
            points.push(measure(axis.name, n, t, d, samples));
        }
        let at = &points[first..];
        for &(from, to, bound) in axis.ratios {
            let (a, b) = (&at[from], &at[to]);
            #[expect(
                clippy::cast_precision_loss,
                reason = "an operation count of a generated document, far below 2^52"
            )]
            let work = b.work as f64 / a.work as f64;
            let time = b.best_ms / a.best_ms;
            let ratio = Ratio {
                axis: axis.name,
                from: label(a),
                to: label(b),
                bound,
                work: round3(work),
                time: round3(time),
                work_within: work <= bound,
                time_within: time <= bound,
            };
            pass &= ratio.work_within && (check == Check::Work || ratio.time_within);
            ratios.push(ratio);
        }
    }
    pass &= points.iter().all(|p| p.preceding_materialized == 0);
    Report {
        schema_version: "0.50.0.lint-nodes",
        samples,
        when: WHEN,
        points,
        ratios,
        pass,
    }
}

fn label(p: &Point) -> String {
    format!("N={} T={} D={}", p.n, p.t, p.d)
}

fn round3(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}

fn measure(axis: &'static str, n: usize, t: usize, d: usize, samples: usize) -> Point {
    let text = document(n, t, d);
    let (doc, _) = lute_syntax::parse(&text);
    let input = [LintDocInput {
        path: PathBuf::from("grid.lute"),
        doc,
        text,
    }];
    let rules = [(
        "grid/preceding-value".to_string(),
        LintRuleDecl {
            id: "preceding-value".to_string(),
            target: LintTarget::Node,
            when: WHEN.to_string(),
            level: Some(LintLevel::Warn),
            message: "repeats {node.tag}".to_string(),
            options: yaml_serde::Mapping::new(),
        },
    )];
    let config = LintConfig::default();
    let providers = lute_manifest::provider::ProviderSet::default();
    let span = lute_core_span::Span {
        byte_start: 0,
        byte_end: 0,
        line: 1,
        column: 1,
        utf16_range: (0, 0),
    };
    let mut best = Duration::MAX;
    let mut stats = NodeWalkStats::default();
    for _ in 0..samples {
        let start = Instant::now();
        let outcome = lint(
            &input,
            &config,
            &rules,
            &providers,
            None,
            span,
            LintScope::Document,
        );
        best = best.min(start.elapsed());
        stats = outcome.node_stats;
        black_box(outcome);
    }
    Point {
        axis,
        n,
        t,
        d,
        rows: stats.rows,
        work: stats.work(),
        preceding_materialized: stats.preceding_materialized,
        best_ms: round3(best.as_secs_f64() * 1000.0),
    }
}

/// Directives written in the top-level body before each nest.
const RUN: usize = 4;
/// Directives written in each `<choice>` body of a nest.
const PER_LEVEL: usize = 2;

/// A document of exactly `n` node rows. Its top-level body repeats a unit:
/// `RUN` directives, then a nest of `d / 2` `<branch>`/`<choice>` pairs with
/// `PER_LEVEL` directives in each choice body. The whole body sits in a lore
/// `<beat>` when `d` is odd, so the deepest rows are at depth `d`.
/// Directives cycle through `t` tags, and the top-level runs keep each tag
/// in `preceding` once written, so every later row and descent sees a scope
/// of up to `t` tags.
fn document(n: usize, t: usize, d: usize) -> String {
    use std::fmt::Write as _;

    struct Doc {
        out: String,
        rows: usize,
        leaves: usize,
        tags: usize,
    }
    impl Doc {
        fn leaf(&mut self) {
            let (tag, value) = (self.leaves % self.tags, self.leaves);
            writeln!(self.out, "::t{tag}{{value=\"v{value}\"}}").expect("a String accepts writes");
            self.leaves += 1;
            self.rows += 1;
        }
    }

    let pairs = d / 2;
    let in_beat = d % 2 == 1;
    let mut doc = Doc {
        out: String::with_capacity(40 * n),
        rows: usize::from(in_beat),
        leaves: 0,
        tags: t.max(1),
    };
    if in_beat {
        doc.out
            .push_str("---\nid: grid\nkind: lore\n---\n<beat id=\"b\" on=\"dawn\">\n");
    } else {
        doc.out
            .push_str("---\nid: grid\nkind: scene\n---\n## Grid\n");
    }
    let mut nests = 0usize;
    while doc.rows < n {
        for _ in 0..RUN.min(n - doc.rows) {
            doc.leaf();
        }
        let mut open = 0;
        while open < pairs && doc.rows + 2 <= n {
            writeln!(
                doc.out,
                "<branch id=\"b{nests}_{open}\">\n<choice id=\"c{nests}_{open}\" text=\"c\">"
            )
            .expect("a String accepts writes");
            doc.rows += 2;
            open += 1;
            for _ in 0..PER_LEVEL.min(n - doc.rows) {
                doc.leaf();
            }
        }
        for _ in 0..open {
            doc.out.push_str("</choice>\n</branch>\n");
        }
        nests += 1;
    }
    if in_beat {
        doc.out.push_str("</beat>\n");
    }
    doc.out
}
