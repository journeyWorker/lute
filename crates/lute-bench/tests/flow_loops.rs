//! dsl 0.51.0 §3.4/§3.8 cost acceptance for the loop-head fixed point, on
//! the deterministic counters of `lute-bench flow-loops` (stable in CI; the
//! wall times are recorded by running the command itself).

use std::process::Command;

/// `(axis, R, D)` → the probe nodes of each walk, in `WalkKind::ALL` order
/// (facts, assign, stage, stageCompile, presence).
const WORK: [(&str, u64, u64, [u64; 5]); 7] = [
    ("control", 100, 0, [0, 0, 0, 0, 0]),
    ("region", 100, 1, [198, 99, 198, 198, 0]),
    ("region", 200, 1, [398, 199, 398, 398, 0]),
    ("region", 400, 1, [798, 399, 798, 798, 0]),
    ("depth", 100, 1, [198, 99, 198, 198, 0]),
    ("depth", 100, 2, [794, 397, 992, 992, 0]),
    ("depth", 100, 3, [2186, 1093, 3574, 3574, 0]),
];

#[test]
fn loop_head_cost_is_linear_in_the_region_and_product_shaped_in_depth() {
    let output = Command::new(env!("CARGO_BIN_EXE_lute-bench"))
        .args(["flow-loops", "--samples", "1", "--check", "work"])
        .output()
        .unwrap();
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(output.status.success(), "{report:#}");
    assert_eq!(report["schemaVersion"], "0.51.0.flow-loops");
    assert_eq!(report["pass"], true);

    let points = report["points"].as_array().unwrap();
    assert_eq!(points.len(), WORK.len());
    for (p, (axis, r, d, work)) in points.iter().zip(WORK) {
        assert_eq!(
            (
                p["axis"].as_str().unwrap(),
                p["r"].as_u64().unwrap(),
                p["d"].as_u64().unwrap()
            ),
            (axis, r, d)
        );
        let walks = p["walks"].as_array().unwrap();
        let got: Vec<u64> = walks
            .iter()
            .map(|w| w["probeNodes"].as_u64().unwrap())
            .collect();
        assert_eq!(got, work, "{p}");
        for w in walks {
            // Probe passes per head never exceed the walk's bound: the fact
            // and stage heads change once (2 passes), definite assignment
            // never (1); presence iterates nothing.
            let bound = match w["kind"].as_str().unwrap() {
                "assign" => 1,
                "presence" => 0,
                _ => 2,
            };
            assert!(w["maxHeadPasses"].as_u64().unwrap() <= bound, "{w}");
            assert_eq!(w["widened"], 0, "{w}");
            assert!(
                w["probeNodes"].as_u64().unwrap() <= w["productBound"].as_u64().unwrap(),
                "{w}"
            );
            // A head is met once per loop in a document without nesting,
            // and none at all without a loop.
            if d <= 1 {
                assert_eq!(w["heads"].as_u64().unwrap(), d, "{w}");
            }
        }
    }
    let ratios = report["ratios"].as_array().unwrap();
    assert_eq!(ratios.len(), 2);
    for ratio in ratios {
        let work = ratio["work"].as_f64().unwrap();
        // Each doubling of the region costs about 2×.
        assert!((1.9..=2.1).contains(&work), "{ratio}");
        assert!(work <= ratio["bound"].as_f64().unwrap(), "{ratio}");
    }
}
