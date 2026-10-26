//! `lute-bench lint-nodes`: the `node` and `pair` lint cost grid (dsl 0.50.0
//! §8.1, 0.51.0 §6.2).
//!
//! Generated documents, one rule per point. Seven axes, each with the others
//! fixed:
//!
//! - node count N ∈ {5 000, 10 000, 20 000} at T = 10 tags, depth D = 3:
//!   each doubling costs ≤ 2.3×;
//! - distinct tags T ∈ {10, 100, 1 000} at N = 10 000, D = 3: 10 → 1 000
//!   costs ≤ 2×;
//! - depth D ∈ {2, 8, 32} at N = 10 000, T = 10: 2 → 32 costs ≤ 2×;
//! - `size`: a rule reading `size(node.preceding)` at every row, T ∈ {10,
//!   1 000, 10 000} at N = 10 000, D = 3 — every directive a distinct tag at
//!   the last point, so the scope grows to N entries: 10 → 10 000 costs ≤ 2×;
//! - `pairNodes`: the `pair` rule [`PAIR`] over alternating
//!   `::group{mode="begin"}`/`::group{mode="end"}` markers, N ∈ {5 000,
//!   10 000, 20 000} at D = 3: each doubling costs ≤ 2.3×;
//! - `pairDepth`: the same at N = 10 000, D ∈ {2, 8, 32}: 2 → 32 costs ≤ 2×;
//! - `payload`: a `pair` rule whose `open` reads `size(node.attrs["text"])`
//!   ([`PAYLOAD_OPEN`]) over `text` values of P ∈ {16, 256, 4 096}
//!   characters at N = 10 000, D = 3: `eval_chars` is exactly the characters
//!   of every `text` value, and each step costs ≤ 1.15× its N × P ratio
//!   (16×) — the cost of reading a value is counted, not absent.
//!
//! The first three axes run [`WHEN`], which reads `preceding` by key.
//!
//! Two costs are measured per point: `work`, the units of input the walk
//! processed ([`lute_lint::nodes::NodeWalkStats::work`]: rows, head bytes,
//! attributes, modeled ordered-map steps (⌈log₂(entries + 1)⌉ per
//! operation, not observed comparisons), copied values, characters the
//! predicates scanned and `pair` slot operations; deterministic), and the
//! best of `--samples` wall times of one `lute_lint::lint` call in the
//! profile the binary was built with (the dev profile in RG: growth ratios,
//! not optimized performance).
//! `--check work` gates on the work only (stable in CI); `--check both` (the
//! default) also gates on the times. Whole-map builds of `preceding` must be
//! zero at every point, the `node` axes do no `pair` work, and a payload
//! point's `eval_chars` equals its `text` characters.

use std::hint::black_box;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use lute_lint::nodes::NodeWalkStats;
use lute_lint::{
    lint, LintConfig, LintDocInput, LintLevel, LintRuleBody, LintRuleDecl, LintScope, LintTarget,
    PairScope, PairSpec,
};
use serde::Serialize;

const USAGE: &str =
    "usage: lute-bench lint-nodes [--samples <n>] [--check work|both] [--json <path>|-]";

/// The rule of the first three axes: a directive whose tag was seen before
/// on its path with the same `value`.
const WHEN: &str = r#"node.kind == "directive" && "value" in node.attrs && node.tag in node.preceding && "value" in node.preceding[node.tag].attrs && node.preceding[node.tag].attrs["value"] == node.attrs["value"]"#;

/// The rule of the `size` axis: reads the scope's size at every directive.
const WHEN_SIZE: &str = r#"node.kind == "directive" && size(node.preceding) > 100000"#;

/// The `pair` axes' rule: the documented `group-balanced` recipe.
const PAIR: Pair = Pair {
    open: r#"node.tag == "group" && "mode" in node.attrs && node.attrs["mode"] == "begin""#,
    close: r#"node.tag == "group" && "mode" in node.attrs && node.attrs["mode"] == "end""#,
    inside: Some(r#"node.kind == "line" || node.tag in ["actor", "sfx"]"#),
};

/// The `payload` axis' `open`: reads every `text` value's size, and never
/// holds, so no finding is built.
const PAYLOAD_OPEN: &str = r#""text" in node.attrs && size(node.attrs["text"]) > 100000"#;

/// The `payload` axis' rule; its `close` reads no string.
const PAYLOAD: Pair = Pair {
    open: PAYLOAD_OPEN,
    close: r#""end" in node.attrs"#,
    inside: None,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Check {
    Work,
    Both,
}

/// A `pair` rule's predicates.
#[derive(Clone, Copy)]
struct Pair {
    open: &'static str,
    close: &'static str,
    inside: Option<&'static str>,
}

/// What a point lints with.
#[derive(Clone, Copy)]
enum Rule {
    /// A `node` rule's `when`.
    Node(&'static str),
    Pair(Pair),
}

/// The directive each leaf of a generated document is.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Leaf {
    /// `::t<i>{value="v<n>"}`, cycling through `t` tags.
    Value,
    /// `::group{mode="begin"}` and `::group{mode="end"}`, alternating.
    Marker,
    /// `::say{text="…"}`, the value `p` characters long.
    Payload,
}

/// One grid axis: its name, its rule and leaves, its points `(n, t, d, p)`
/// and the bound on the cost ratio between the points it compares.
struct Axis {
    name: &'static str,
    rule: Rule,
    leaf: Leaf,
    points: [(usize, usize, usize, usize); 3],
    /// `(from, to, bound)` indexes into `points`.
    ratios: &'static [(usize, usize, f64)],
}

/// The `payload` bound: 1.15× the 16× N × P step.
const PAYLOAD_BOUND: f64 = 1.15 * 16.0;

const AXES: [Axis; 7] = [
    Axis {
        name: "nodes",
        rule: Rule::Node(WHEN),
        leaf: Leaf::Value,
        points: [(5_000, 10, 3, 0), (10_000, 10, 3, 0), (20_000, 10, 3, 0)],
        ratios: &[(0, 1, 2.3), (1, 2, 2.3)],
    },
    Axis {
        name: "tags",
        rule: Rule::Node(WHEN),
        leaf: Leaf::Value,
        points: [
            (10_000, 10, 3, 0),
            (10_000, 100, 3, 0),
            (10_000, 1_000, 3, 0),
        ],
        ratios: &[(0, 2, 2.0)],
    },
    Axis {
        name: "depth",
        rule: Rule::Node(WHEN),
        leaf: Leaf::Value,
        points: [(10_000, 10, 2, 0), (10_000, 10, 8, 0), (10_000, 10, 32, 0)],
        ratios: &[(0, 2, 2.0)],
    },
    Axis {
        name: "size",
        rule: Rule::Node(WHEN_SIZE),
        leaf: Leaf::Value,
        points: [
            (10_000, 10, 3, 0),
            (10_000, 1_000, 3, 0),
            (10_000, 10_000, 3, 0),
        ],
        ratios: &[(0, 2, 2.0)],
    },
    Axis {
        name: "pairNodes",
        rule: Rule::Pair(PAIR),
        leaf: Leaf::Marker,
        points: [(5_000, 0, 3, 0), (10_000, 0, 3, 0), (20_000, 0, 3, 0)],
        ratios: &[(0, 1, 2.3), (1, 2, 2.3)],
    },
    Axis {
        name: "pairDepth",
        rule: Rule::Pair(PAIR),
        leaf: Leaf::Marker,
        points: [(10_000, 0, 2, 0), (10_000, 0, 8, 0), (10_000, 0, 32, 0)],
        ratios: &[(0, 2, 2.0)],
    },
    Axis {
        name: "payload",
        rule: Rule::Pair(PAYLOAD),
        leaf: Leaf::Payload,
        points: [
            (10_000, 0, 3, 16),
            (10_000, 0, 3, 256),
            (10_000, 0, 3, 4_096),
        ],
        ratios: &[(0, 1, PAYLOAD_BOUND), (1, 2, PAYLOAD_BOUND)],
    },
];

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Point {
    axis: &'static str,
    n: usize,
    t: usize,
    d: usize,
    p: usize,
    rows: u64,
    work: u64,
    preceding_materialized: u64,
    preceding_copied: u64,
    eval_chars: u64,
    pair_evals: u64,
    pair_slots: u64,
    /// The characters of the document's `text` values (`payload` axis).
    payload_chars: u64,
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
        for &point in &axis.points {
            points.push(measure(axis, point, samples));
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
        if let Rule::Node(_) = axis.rule {
            pass &= at.iter().all(|p| p.pair_evals == 0 && p.pair_slots == 0);
        }
        if axis.leaf == Leaf::Payload {
            pass &= at.iter().all(|p| p.eval_chars == p.payload_chars);
        }
    }
    pass &= points.iter().all(|p| p.preceding_materialized == 0);
    Report {
        schema_version: "0.51.0.lint-nodes",
        samples,
        points,
        ratios,
        pass,
    }
}

fn label(p: &Point) -> String {
    match (p.t, p.p) {
        (0, 0) => format!("N={} D={}", p.n, p.d),
        (_, 0) => format!("N={} T={} D={}", p.n, p.t, p.d),
        _ => format!("N={} D={} P={}", p.n, p.d, p.p),
    }
}

fn round3(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}

fn measure(axis: &Axis, (n, t, d, p): (usize, usize, usize, usize), samples: usize) -> Point {
    let generated = document(axis.leaf, n, t, d, p);
    let (doc, _) = lute_syntax::parse(&generated.text);
    let input = [LintDocInput {
        path: PathBuf::from("grid.lute"),
        doc,
        text: generated.text,
    }];
    let (target, body) = match axis.rule {
        Rule::Node(when) => (
            LintTarget::Node,
            LintRuleBody::Predicate {
                when: when.to_string(),
            },
        ),
        Rule::Pair(pair) => (
            LintTarget::Pair,
            LintRuleBody::Pair(PairSpec {
                open: pair.open.to_string(),
                close: pair.close.to_string(),
                inside: pair.inside.map(str::to_string),
                scope: PairScope::Body,
            }),
        ),
    };
    let rules = [(
        "grid/rule".to_string(),
        LintRuleDecl {
            id: "rule".to_string(),
            target,
            body,
            level: Some(LintLevel::Warn),
            message: "at {node.tag}".to_string(),
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
        axis: axis.name,
        n,
        t,
        d,
        p,
        rows: stats.rows,
        work: stats.work(),
        preceding_materialized: stats.preceding_materialized,
        preceding_copied: stats.preceding_copied,
        eval_chars: stats.eval_chars,
        pair_evals: stats.pair_evals,
        pair_slots: stats.pair_slots,
        payload_chars: generated.payload_chars,
        best_ms: round3(best.as_secs_f64() * 1000.0),
    }
}

/// Directives written in the top-level body before each nest.
const RUN: usize = 4;
/// Directives written in each `<choice>` body of a nest.
const PER_LEVEL: usize = 2;

/// A generated document and the characters of its `text` values.
struct Generated {
    text: String,
    payload_chars: u64,
}

/// A document of exactly `n` node rows. Its top-level body repeats a unit:
/// `RUN` directives, then a nest of `d / 2` `<branch>`/`<choice>` pairs with
/// `PER_LEVEL` directives in each choice body. The whole body sits in a lore
/// `<beat>` when `d` is odd, so the deepest rows are at depth `d`.
/// Each directive is a `leaf`: with [`Leaf::Value`], directives cycle
/// through `t` tags, and the top-level runs keep each tag in `preceding`
/// once written, so every later row and descent sees a scope of up to `t`
/// tags; with [`Leaf::Marker`], every body alternates begin and end markers
/// (`RUN` and `PER_LEVEL` are even); with [`Leaf::Payload`], every
/// directive carries a `text` of `p` characters.
fn document(leaf: Leaf, n: usize, t: usize, d: usize, p: usize) -> Generated {
    use std::fmt::Write as _;

    struct Doc {
        out: String,
        rows: usize,
        leaves: usize,
        leaf: Leaf,
        tags: usize,
        payload: String,
    }
    impl Doc {
        fn leaf(&mut self) {
            match self.leaf {
                Leaf::Value => {
                    let (tag, value) = (self.leaves % self.tags, self.leaves);
                    writeln!(self.out, "::t{tag}{{value=\"v{value}\"}}")
                        .expect("a String accepts writes");
                }
                Leaf::Marker => {
                    let mode = if self.leaves % 2 == 0 { "begin" } else { "end" };
                    writeln!(self.out, "::group{{mode=\"{mode}\"}}")
                        .expect("a String accepts writes");
                }
                Leaf::Payload => {
                    writeln!(self.out, "::say{{text=\"{}\"}}", self.payload)
                        .expect("a String accepts writes");
                }
            }
            self.leaves += 1;
            self.rows += 1;
        }
    }

    let pairs = d / 2;
    let in_beat = d % 2 == 1;
    let mut doc = Doc {
        out: String::with_capacity((40 + p) * n),
        rows: usize::from(in_beat),
        leaves: 0,
        leaf,
        tags: t.max(1),
        payload: "x".repeat(p),
    };
    if in_beat {
        doc.out
            .push_str("---\nid: grid\nkind: lore\n---\n<beat id=\"b\" on=\"dawn\">\n");
    } else {
        doc.out
            .push_str("---\nid: grid\nkind: scene\n---\n## Grid\n");
    }
    let mut nests = 0usize;
    let mut choices = 0usize;
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
            choices += 1;
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
    // Each `<choice text="c">` carries one character of `text` too.
    let payload_chars = match leaf {
        Leaf::Payload => (doc.leaves * p + choices) as u64,
        Leaf::Value | Leaf::Marker => 0,
    };
    Generated {
        text: doc.out,
        payload_chars,
    }
}
