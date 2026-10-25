//! dsl 0.50.0 §8.1 cost acceptance for `node` lints, on the operation
//! counts of `lute-bench lint-nodes` (deterministic, so stable in CI; the
//! wall times are recorded by running the command itself).

use std::process::Command;

#[test]
fn node_lint_cost_grows_by_rows_only() {
    let output = Command::new(env!("CARGO_BIN_EXE_lute-bench"))
        .args(["lint-nodes", "--samples", "1", "--check", "work"])
        .output()
        .unwrap();
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(output.status.success(), "{report:#}");
    assert_eq!(report["schemaVersion"], "0.50.0.lint-nodes");
    assert_eq!(report["pass"], true);

    let points = report["points"].as_array().unwrap();
    assert_eq!(points.len(), 12);
    for p in points {
        // Exactly the asked node count, and no copy of the scope map.
        assert_eq!(p["rows"], p["n"], "{p}");
        assert_eq!(p["precedingMaterialized"], 0, "{p}");
        assert_eq!(p["precedingCopied"], 0, "{p}");
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
            ("size", 2.0)
        ]
    );
    for (axis, ratio, bound) in &work {
        assert!(ratio <= bound, "{axis}: work ratio {ratio} > {bound}");
    }
    // The work is linear in the rows: a doubling is about 2×. The tag count
    // adds only the ordered map's logarithmic steps, and the depth nothing.
    for (axis, ratio, _) in &work {
        if *axis == "nodes" {
            assert!(
                (1.9..=2.1).contains(ratio),
                "{axis}: work ratio {ratio} outside 1.9..=2.1"
            );
        }
    }
}
