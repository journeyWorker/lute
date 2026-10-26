//! dsl 0.50.0 §8.1 and 0.51.0 §6.2 cost acceptance for `node` and `pair`
//! lints, on the operation counts of `lute-bench lint-nodes` (deterministic,
//! so stable in CI; the wall times are recorded by running the command
//! itself).

use std::process::Command;

/// A point: `(axis, n, t, d, p)`.
type At = (&'static str, u64, u64, u64, u64);
/// Its counts: `(work − eval_chars − pair_slots, eval_chars, pair_evals,
/// pair_slots)`.
type Counts = (u64, u64, u64, u64);

/// The counts of each point. The first column of the `node` axes is
/// 0.50.0's logged work (0.50 plan, S7), which counted neither scanned
/// characters nor `pair` slots.
const WORK: [(At, Counts); 21] = [
    (("nodes", 5_000, 10, 3, 0), (211_028, 51_188, 0, 0)),
    (("nodes", 10_000, 10, 3, 0), (424_028, 103_688, 0, 0)),
    (("nodes", 20_000, 10, 3, 0), (856_527, 213_603, 0, 0)),
    (("tags", 10_000, 10, 3, 0), (424_028, 103_688, 0, 0)),
    (("tags", 10_000, 100, 3, 0), (578_325, 102_380, 0, 0)),
    (("tags", 10_000, 1_000, 3, 0), (704_714, 93_316, 0, 0)),
    (("depth", 10_000, 10, 2, 0), (424_031, 103_702, 0, 0)),
    (("depth", 10_000, 10, 8, 0), (407_779, 82_528, 0, 0)),
    (("depth", 10_000, 10, 32, 0), (399_907, 72_702, 0, 0)),
    (("size", 10_000, 10, 3, 0), (364_132, 67_491, 0, 0)),
    (("size", 10_000, 1_000, 3, 0), (581_374, 67_491, 0, 0)),
    (("size", 10_000, 10_000, 3, 0), (643_557, 67_491, 0, 0)),
    (
        ("pairNodes", 5_000, 0, 3, 0),
        (157_264, 33_742, 6_251, 5_001),
    ),
    (
        ("pairNodes", 10_000, 0, 3, 0),
        (315_264, 67_492, 12_501, 10_001),
    ),
    (
        ("pairNodes", 20_000, 0, 3, 0),
        (632_764, 134_992, 25_001, 20_001),
    ),
    (
        ("pairDepth", 10_000, 0, 2, 0),
        (295_277, 67_500, 12_500, 10_002),
    ),
    (
        ("pairDepth", 10_000, 0, 8, 0),
        (309_117, 54_000, 14_000, 10_002),
    ),
    (
        ("pairDepth", 10_000, 0, 32, 0),
        (315_285, 47_664, 14_704, 10_002),
    ),
    // 7 499 `::say` of P characters and 1 250 `<choice text="c">`.
    (
        ("payload", 10_000, 0, 3, 16),
        (390_253, 121_234, 20_000, 2_502),
    ),
    (
        ("payload", 10_000, 0, 3, 256),
        (2_190_013, 1_920_994, 20_000, 2_502),
    ),
    (
        ("payload", 10_000, 0, 3, 4_096),
        (30_986_173, 30_717_154, 20_000, 2_502),
    ),
];

#[test]
fn lint_cost_grows_by_rows_and_counted_payload_only() {
    let output = Command::new(env!("CARGO_BIN_EXE_lute-bench"))
        .args(["lint-nodes", "--samples", "1", "--check", "work"])
        .output()
        .unwrap();
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(output.status.success(), "{report:#}");
    assert_eq!(report["schemaVersion"], "0.51.0.lint-nodes");
    assert_eq!(report["pass"], true);

    let points = report["points"].as_array().unwrap();
    assert_eq!(points.len(), WORK.len());
    let u = |p: &serde_json::Value, key: &str| p[key].as_u64().unwrap();
    for (p, ((axis, n, t, d, size), (base, chars, evals, slots))) in points.iter().zip(WORK) {
        assert_eq!(
            (
                p["axis"].as_str().unwrap(),
                u(p, "n"),
                u(p, "t"),
                u(p, "d"),
                u(p, "p")
            ),
            (axis, n, t, d, size)
        );
        // Exactly the asked node count, and no copy of the scope map.
        assert_eq!(u(p, "rows"), n, "{p}");
        assert_eq!(u(p, "precedingMaterialized"), 0, "{p}");
        assert_eq!(u(p, "precedingCopied"), 0, "{p}");
        assert_eq!(
            (
                u(p, "work") - u(p, "evalChars") - u(p, "pairSlots"),
                u(p, "evalChars"),
                u(p, "pairEvals"),
                u(p, "pairSlots")
            ),
            (base, chars, evals, slots),
            "{p}"
        );
        if axis == "payload" {
            // The `text` values' characters, each scanned once.
            assert_eq!(u(p, "evalChars"), 7_499 * size + 1_250, "{p}");
            assert_eq!(u(p, "evalChars"), u(p, "payloadChars"), "{p}");
        }
        if !axis.starts_with("pair") && axis != "payload" {
            // No `pair` rule, no `pair` work.
            assert_eq!((u(p, "pairEvals"), u(p, "pairSlots")), (0, 0), "{p}");
        } else {
            // At most three predicate invocations per row.
            assert!(u(p, "pairEvals") <= 3 * n, "{p}");
        }
    }
    let ratios = report["ratios"].as_array().unwrap();
    let work: Vec<(&str, f64, f64)> = ratios
        .iter()
        .map(|r| {
            (
                r["axis"].as_str().unwrap(),
                r["work"].as_f64().unwrap(),
                r["bound"].as_f64().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        work.iter().map(|(a, _, b)| (*a, *b)).collect::<Vec<_>>(),
        [
            ("nodes", 2.3),
            ("nodes", 2.3),
            ("tags", 2.0),
            ("depth", 2.0),
            ("size", 2.0),
            ("pairNodes", 2.3),
            ("pairNodes", 2.3),
            ("pairDepth", 2.0),
            ("payload", 1.15 * 16.0),
            ("payload", 1.15 * 16.0),
        ]
    );
    for (axis, ratio, bound) in &work {
        assert!(ratio <= bound, "{axis}: work ratio {ratio} > {bound}");
    }
    // The work is linear in the rows: a doubling is about 2×. The tag count
    // adds only the ordered map's logarithmic steps, and the depth nothing.
    for (axis, ratio, _) in &work {
        if *axis == "nodes" || *axis == "pairNodes" {
            assert!(
                (1.9..=2.1).contains(ratio),
                "{axis}: work ratio {ratio} outside 1.9..=2.1"
            );
        }
    }
}
