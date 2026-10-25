//! dsl 0.51.0 §3.7: `lute trace` explains each bounded jump — which take it
//! was, or that it was not taken — over the `bounded-jump` conformance
//! fixture.

use std::path::{Path, PathBuf};
use std::process::Command;

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance/bounded-jump")
}

fn trace(extra: &[&str]) -> String {
    let dir = fixture();
    let out = Command::new(env!("CARGO_BIN_EXE_lute"))
        .arg("trace")
        .arg(dir.join("source.lute"))
        .arg("--mock")
        .arg(dir.join("mock.yaml"))
        .args(extra)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn trace_prints_each_take_and_the_fall_through() {
    let text = trace(&[]);
    let jumps: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with("<jump -> attempt"))
        .collect();
    assert_eq!(
        jumps,
        [
            "<jump -> attempt (again 1/2)>",
            "<jump -> attempt (again 2/2)>",
            "<jump -> attempt not taken (2/2 used)>",
        ],
        "{text}"
    );
    // The guard let go before the bound: one take, no fall-through line.
    let again: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with("<jump -> again"))
        .collect();
    assert_eq!(again, ["<jump -> again (again 1/5)>"], "{text}");
    // The inner loop starts over on each outer iteration.
    assert_eq!(
        text.matches("<jump -> inner (again 1/1)>").count(),
        2,
        "{text}"
    );
}

#[test]
fn trace_json_carries_the_bound() {
    let report: serde_json::Value = serde_json::from_str(&trace(&["--json"])).unwrap();
    let bounds: Vec<&serde_json::Value> = report["steps"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|s| s["kind"] == "jump" && s["to"] == "attempt")
        .map(|s| &s["bounded"])
        .collect();
    assert_eq!(
        bounds,
        [
            &serde_json::json!({ "outcome": "again", "take": 1, "max": 2 }),
            &serde_json::json!({ "outcome": "again", "take": 2, "max": 2 }),
            &serde_json::json!({ "outcome": "notTaken", "max": 2 }),
        ]
    );
}

/// S3 review P2: an empty loop (its label right before its jump, which the
/// IR compiles to a jump that targets itself) inside an outer loop starts
/// over on each outer iteration.
#[test]
fn an_empty_loop_starts_over_on_each_outer_iteration() {
    let dir = lute_scratch::ScratchDir::named("empty-loop");
    let source = dir.join("scene.lute");
    std::fs::write(
        &source,
        "---\nkind: scene\nluteVersion: \"0.50.0\"\nid: empty\ntitle: T\n---\n\n## Shot 1.\n\n\
         ::label{name=\"outer\"}\n@narrator: Round.\n::label{name=\"inner\"}\n\
         ::jump{to=\"inner\" max=\"1\"}\n::jump{to=\"outer\" max=\"1\"}\n@narrator: Done.\n",
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_lute"))
        .arg("trace")
        .arg(&source)
        .output()
        .unwrap();
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(
        out.status.success(),
        "{text}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let inner: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with("<jump -> inner"))
        .collect();
    assert_eq!(
        inner,
        [
            "<jump -> inner (again 1/1)>",
            "<jump -> inner not taken (1/1 used)>",
            "<jump -> inner (again 1/1)>",
            "<jump -> inner not taken (1/1 used)>",
        ],
        "{text}"
    );
}

/// Compile and run a scene using component `c` (both written to a scratch
/// directory); the `lute run` transcript.
fn run_with_component(tag: &str, component: &str, host: &str) -> String {
    let dir = lute_scratch::ScratchDir::named(tag);
    std::fs::write(dir.join("c.component.lute"), component).unwrap();
    let source = dir.join("host.lute");
    std::fs::write(&source, host).unwrap();
    let artifact = dir.join("host.json");
    let lute = || Command::new(env!("CARGO_BIN_EXE_lute"));
    let out = lute()
        .arg("compile")
        .arg(&source)
        .arg("-o")
        .arg(&artifact)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let out = lute().arg("run").arg(&artifact).output().unwrap();
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(
        out.status.success(),
        "{text}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    text
}

/// #207 (S3 re-review): a component used twice binds its labels per
/// instance, so each instance's bounded loop runs to its own bound and the
/// artifact passes the runtime's load check (on 0.51 S3 the second
/// instance's jump targeted the first instance's label: crossing loops).
#[test]
fn each_component_instance_loops_on_its_own_label() {
    let text = run_with_component(
        "component-loops",
        "---\ncomponent: c\n---\n## Body\n::label{name=\"inner\"}\n@narrator: In.\n\
         ::jump{to=\"inner\" max=\"1\"}\n",
        "---\nkind: scene\nid: host\nluteVersion: \"0.50.0\"\ntitle: Host\n\
         components: [c.component.lute]\n---\n## Shot 1.\n\
         ::use{component=\"c\" instance=\"first\"}\n::label{name=\"outer\"}\n@narrator: Mid.\n\
         ::use{component=\"c\" instance=\"second\"}\n::jump{to=\"outer\" max=\"1\"}\n\
         @narrator: End.\n",
    );
    assert_eq!(text.matches("narrator: In.").count(), 6, "{text}");
    assert_eq!(text.matches("narrator: Mid.").count(), 2, "{text}");
    assert!(text.contains("narrator: End."), "{text}");
}

/// #207: a forward `::jump` in a component used twice skips within its own
/// instance (on 0.50.0 the second instance jumped back into the first and
/// the run never ended).
#[test]
fn a_forward_jump_stays_in_its_component_instance() {
    let text = run_with_component(
        "component-forward",
        "---\ncomponent: c\n---\n## Body\n@narrator: In.\n::jump{to=\"skip\"}\n\
         @narrator: Skipped.\n::label{name=\"skip\"}\n@narrator: After.\n",
        "---\nkind: scene\nid: host\nluteVersion: \"0.50.0\"\ntitle: Host\n\
         components: [c.component.lute]\n---\n## Shot 1.\n\
         ::use{component=\"c\" instance=\"first\"}\n@narrator: Between.\n\
         ::use{component=\"c\" instance=\"second\"}\n@narrator: End.\n",
    );
    let lines: Vec<&str> = text
        .lines()
        .filter_map(|l| l.split_once("narrator: ").map(|(_, t)| t))
        .collect();
    assert_eq!(
        lines,
        ["In.", "After.", "Between.", "In.", "After.", "End."],
        "{text}"
    );
}
