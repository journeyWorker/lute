//! Bounded execution-IR wire round trips for the 0.36 release.
//!
//! Replay contract: a minimized failing byte vector and the fixed seed are
//! written to `target/property-failures/ir-round-trip.replay`; rerun this test
//! with that seed before reporting a serialization defect.

use std::fs;
use std::path::Path;

use proptest::prelude::*;
use proptest::test_runner::{Config, RngAlgorithm, RngSeed, TestCaseError, TestError, TestRunner};
use lute_check::{CheckInput, Mode};
use lute_compile::compile;
use serde_json::Value;

const SEED: u64 = 0x0360_2026_10_05;
const MAX_INPUT: usize = 4 * 1024;
const CASES: u32 = 128;

fn config() -> Config {
    let mut config = Config::default();
    config.cases = CASES;
    config.max_shrink_iters = 256;
    config.rng_algorithm = RngAlgorithm::ChaCha;
    config.rng_seed = RngSeed::Fixed(SEED);
    config.failure_persistence = None;
    config
}

fn input(text: String) -> CheckInput {
    CheckInput {
        text,
        uri: "property-round-trip".into(),
        snapshot: lute_test_vocab::vocab_snapshot(),
        providers: Default::default(),
        mode: Mode::Ci,
        imports: Default::default(),
        components: Default::default(),
        defaults: Default::default(),
    }
}

fn generated_source(bytes: &[u8]) -> String {
    let payload: String = bytes
        .iter()
        .map(|byte| match byte % 5 {
            0 => 'a',
            1 => ' ',
            2 => '0',
            3 => '_',
            _ => 'z',
        })
        .take(MAX_INPUT.saturating_sub(96))
        .collect();
    format!(
        "---\nkind: scene\nid: property-round-trip\n---\n## Generated\n@narrator: {}\n",
        if payload.is_empty() { "text" } else { &payload }
    )
}

fn check_case(bytes: &[u8]) -> Result<(), String> {
    let artifact = compile(&input(generated_source(bytes)))
        .map_err(|diagnostics| format!("generated source did not compile: {diagnostics:?}"))?;
    let semantic = serde_json::to_value(&artifact)
        .map_err(|error| format!("IR value serialization failed: {error}"))?;
    let wire = serde_json::to_string(&artifact)
        .map_err(|error| format!("IR JSON serialization failed: {error}"))?;
    let round_trip: Value = serde_json::from_str(&wire)
        .map_err(|error| format!("IR JSON deserialization failed: {error}"))?;
    if semantic != round_trip {
        return Err("IR semantic value changed across JSON round trip".into());
    }
    Ok(())
}

fn assert_artifact_round_trip(path: &Path) {
    let source = fs::read_to_string(path).expect("read IR corpus artifact");
    let value: Value = serde_json::from_str(&source).expect("parse IR corpus artifact");
    let wire = serde_json::to_string(&value).expect("serialize IR corpus value");
    let round_trip: Value = serde_json::from_str(&wire).expect("deserialize IR corpus value");
    assert_eq!(value, round_trip, "semantic artifact changed: {}", path.display());
}

fn persist_failure(name: &str, error: &TestError<Vec<u8>>) -> std::path::PathBuf {
    let dir = option_env!("CARGO_TARGET_DIR")
        .map(|target| Path::new(target).join("property-failures"))
        .unwrap_or_else(|| Path::new("target/property-failures").to_path_buf());
    fs::create_dir_all(&dir).expect("create property replay directory");
    let path = dir.join(format!("{name}.replay"));
    fs::write(
        &path,
        format!("seed=0x{SEED:016x}\nmax_input={MAX_INPUT}\n{error:#?}\n"),
    )
    .expect("write property replay artifact");
    path
}

#[test]
fn round_trip_property() {
    for relative in [
        "../../conformance/choice-basic/artifact.json",
        "../../conformance/command-lifecycle/artifact.json",
        "../../conformance/cel-facts/artifact.json",
        "../../conformance/hub-once-exit/artifact.json",
    ] {
        assert_artifact_round_trip(&Path::new(env!("CARGO_MANIFEST_DIR")).join(relative));
    }

    let strategy = prop::collection::vec(any::<u8>(), 0..=MAX_INPUT);
    let mut runner = TestRunner::new(config());
    let result = runner.run(&strategy, |bytes| match check_case(&bytes) {
        Ok(()) => Ok(()),
        Err(error) => Err(TestCaseError::fail(error)),
    });
    if let Err(error) = result {
        let replay_path = persist_failure("ir-round-trip", &error);
        let mut replay_runner = TestRunner::new(config());
        let replay = replay_runner.run(&strategy, |bytes| match check_case(&bytes) {
            Ok(()) => Ok(()),
            Err(error) => Err(TestCaseError::fail(error)),
        });
        assert!(replay.is_err(), "fixed-seed IR failure did not replay");
        panic!("IR round-trip property failed with seed 0x{SEED:016x}; replay: {}\n{error}", replay_path.display());
    }
}
