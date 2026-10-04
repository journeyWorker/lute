//! Bounded parser totality property for the 0.36 release.
//!
//! Replay contract: parser failures write proptest's minimized input and the
//! fixed seed to `target/property-failures/parser.replay`; rerun this test with
//! that seed before reporting a parser panic.

use std::fs;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;

use proptest::prelude::*;
use proptest::test_runner::{Config, RngAlgorithm, RngSeed, TestCaseError, TestError, TestRunner};

const SEED: u64 = 0x0360_2026_10_05;
const MAX_INPUT: usize = 4 * 1024;
const CASES: u32 = 256;

fn config() -> Config {
    let mut config = Config::default();
    config.cases = CASES;
    config.max_shrink_iters = 256;
    config.rng_algorithm = RngAlgorithm::ChaCha;
    config.rng_seed = RngSeed::Fixed(SEED);
    config.failure_persistence = None;
    config
}

fn bounded_lossy(bytes: &[u8]) -> String {
    let mut text = String::from_utf8_lossy(bytes).into_owned();
    while text.len() > MAX_INPUT {
        text.pop();
    }
    text
}

fn check_case(bytes: &[u8]) -> Result<(), String> {
    let source = bounded_lossy(bytes);
    catch_unwind(AssertUnwindSafe(|| {
        // Parser diagnostics are valid outcomes; only a panic is a property
        // failure. The parser owns recovery for malformed editor buffers.
        let _ = lute_syntax::parse(&source);
    }))
    .map_err(|_| "parser panicked on arbitrary input".to_string())
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
fn parser_property() {
    let strategy = prop::collection::vec(any::<u8>(), 0..=MAX_INPUT);
    let mut runner = TestRunner::new(config());
    let result = runner.run(&strategy, |bytes| match check_case(&bytes) {
        Ok(()) => Ok(()),
        Err(error) => Err(TestCaseError::fail(error)),
    });
    if let Err(error) = result {
        let replay_path = persist_failure("parser", &error);
        let mut replay_runner = TestRunner::new(config());
        let replay = replay_runner.run(&strategy, |bytes| match check_case(&bytes) {
            Ok(()) => Ok(()),
            Err(error) => Err(TestCaseError::fail(error)),
        });
        assert!(replay.is_err(), "fixed-seed parser failure did not replay");
        panic!("parser property failed with seed 0x{SEED:016x}; replay: {}\n{error}", replay_path.display());
    }
}
