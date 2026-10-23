use std::fs;
use std::path::PathBuf;
use std::process::Command;

#[test]
fn binary_emits_schema_and_200ms_sample() {
    let scratch = lute_scratch::ScratchDir::named("bench");
    let output = scratch.join("report.json");
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let status = Command::new(env!("CARGO_BIN_EXE_lute-bench"))
        .args([
            "--root",
            root.to_str().unwrap(),
            "--tier",
            "tiny",
            "--phase",
            "cold-load",
            "--samples",
            "1",
            "--json",
            output.to_str().unwrap(),
        ])
        .status()
        .unwrap();
    assert!(status.success());
    let report: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&output).unwrap()).unwrap();
    assert_eq!(report["schemaVersion"], "0.36.0.bench");
    let sample = &report["samples"][0];
    assert_eq!(sample["exit"], "ok");
    assert!(sample["iterations"].as_u64().unwrap() > 0);
    assert!(sample["elapsedMs"].as_f64().unwrap() >= 200.0);
}

fn bench() -> Command {
    Command::new(env!("CARGO_BIN_EXE_lute-bench"))
}

fn files(root: &std::path::Path) -> Vec<String> {
    let mut found = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else {
                let relative = path.strip_prefix(root).unwrap();
                found.push(relative.to_str().unwrap().replace('\\', "/"));
            }
        }
    }
    found.sort();
    found
}

#[test]
fn generate_vocab_writes_the_issue_173_project() {
    let scratch = lute_scratch::ScratchDir::named("bench");
    let out = scratch.join("proj");
    let status = bench()
        .args(["generate-vocab", out.to_str().unwrap()])
        .args(["-S", "2", "-N", "2", "-M", "3", "-K", "2", "-P", "2"])
        .status()
        .unwrap();
    assert!(status.success());
    assert_eq!(
        files(&out),
        [
            "lute.project.yaml",
            "quests/q0001.lute",
            "quests/q0002.lute",
            "scenes/s000.lute",
            "scenes/s001.lute",
            "schema/world.schema.yaml",
        ]
    );
    // `gen.py -S 2 -N 2 -M 3 -K 2 -P 2` (issue #173), verbatim.
    assert_eq!(
        fs::read_to_string(out.join("quests/q0002.lute")).unwrap(),
        "---\nkind: quest\nuses: ../schema/world.schema.yaml\n---\n\n\
         <quest id=\"q0002\" title=\"Quest 2\" start=\"user.s2.k0m0 >= 0 && user.s0.k0m0 >= 0 && quest.q0001.state == 'complete'\">\n\
         \x20 <objective id=\"a\" title=\"Step A\" done=\"user.s1.k1m1 >= 1\"/>\n\
         \x20 <objective id=\"b\" title=\"Step B\" done=\"user.s2.k0m1 >= 2 || user.s0.k0m0 >= 1\"/>\n\
         \x20 <on event=\"questComplete\">\n\
         \x20   ::set{user.s1.k1m1 += 1}\n\
         \x20   ::set{user.s2.k0m0 += 1}\n\
         \x20 </on>\n\
         </quest>\n"
    );
    assert_eq!(
        fs::read_to_string(out.join("lute.project.yaml")).unwrap(),
        "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n"
    );
}

#[test]
fn generate_vocab_rejects_bad_parameters() {
    let scratch = lute_scratch::ScratchDir::named("bench");
    let out = scratch.join("proj");
    let usage = bench()
        .args(["generate-vocab", out.to_str().unwrap(), "-X", "1"])
        .output()
        .unwrap();
    assert_eq!(usage.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&usage.stderr).contains("unknown generate-vocab argument `-X`"));
    let no_kinds = bench()
        .args([
            "generate-vocab",
            out.to_str().unwrap(),
            "-M",
            "1",
            "-K",
            "0",
        ])
        .output()
        .unwrap();
    assert_eq!(no_kinds.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&no_kinds.stderr).contains("K must be >= 1 when M > 0"));
    assert!(!out.exists());
}

#[test]
fn vocab_tier_generates_into_tmpdir_and_removes_it() {
    let scratch = lute_scratch::ScratchDir::named("bench");
    let tmp = scratch.join("tmp");
    fs::create_dir(&tmp).unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = bench()
        .args(["--root", root.to_str().unwrap()])
        .args([
            "--tier",
            "vocab",
            "--phase",
            "cold-load",
            "--samples",
            "1",
            "--json",
            "-",
        ])
        .env("TMPDIR", &tmp)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["corpus"]["vocab"]["project"], "vocab");
    assert_eq!(report["samples"].as_array().unwrap().len(), 1);
    let sample = &report["samples"][0];
    assert_eq!(sample["tier"], "vocab");
    assert_eq!(sample["phase"], "cold-load");
    assert_eq!(sample["exit"], "ok", "{sample}");
    assert_eq!(
        fs::read_dir(&tmp).unwrap().count(),
        0,
        "the generated project is removed"
    );
}

/// `scripts/bench-ab.py` (`supports_tier`, `supports_phase`) recognizes an
/// older base by these exact exit codes and messages.
#[test]
fn unknown_tier_and_unmeasured_phase_are_usage_errors() {
    let tier = bench()
        .args(["--tier", "bogus", "--json", "-"])
        .output()
        .unwrap();
    assert_eq!(tier.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&tier.stderr).contains("invalid tier `bogus`"));
    let phase = bench()
        .args(["--tier", "vocab", "--phase", "playback", "--json", "-"])
        .output()
        .unwrap();
    assert_eq!(phase.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&phase.stderr)
        .contains("phase `playback` is not measured on tier `vocab`"));
}

fn fixture(path: &PathBuf, value: f64) {
    let report = serde_json::json!({
        "schemaVersion": "0.36.0.bench",
        "runner": {"os": "test", "arch": "test", "cpu": "test"},
        "corpus": {"tiny": {"project": "ledger", "projectRevision": "rev"}},
        "samples": [{
            "tier": "tiny", "project": "ledger", "phase": "analysis", "sample": 0,
            "testCount": 1, "iterations": 1, "elapsedMs": 200.0,
            "perIterationUs": value, "exit": "ok"
        }]
    });
    fs::write(path, serde_json::to_vec(&report).unwrap()).unwrap();
}

#[test]
fn comparator_accepts_110_and_rejects_111() {
    let scratch = lute_scratch::ScratchDir::named("bench");
    let base = scratch.join("base.json");
    let head = scratch.join("head.json");
    fixture(&base, 100.0);
    fixture(&head, 110.0);
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scripts/compare-bench.py");
    let accepted = Command::new("python3")
        .args([
            script.to_str().unwrap(),
            base.to_str().unwrap(),
            head.to_str().unwrap(),
        ])
        .status()
        .unwrap();
    assert!(accepted.success());
    fixture(&head, 111.0);
    let rejected = Command::new("python3")
        .args([
            script.to_str().unwrap(),
            base.to_str().unwrap(),
            head.to_str().unwrap(),
        ])
        .status()
        .unwrap();
    assert_eq!(rejected.code(), Some(1));
}
