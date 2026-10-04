//! Bounded formatter properties for the 0.36 release.
//!
//! Replay contract: failures are written to `target/property-failures/` with the
//! fixed seed and proptest's minimized input. Re-run this test with the same
//! checkout (and seed in the replay file) before filing a formatter defect.

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
    // The release harness owns persistence so replay files have one stable,
    // explicit format instead of relying on proptest's source-relative file.
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

fn generated_source(bytes: &[u8]) -> String {
    match bytes.first().copied().unwrap_or_default() % 3 {
        // A valid generated document exercises the successful formatter path.
        0 => {
            let header = "## Generated\n@narrator: ";
            let room = MAX_INPUT.saturating_sub(header.len() + 1);
            let payload: String = bytes[1..]
                .iter()
                .map(|byte| match byte % 4 {
                    0 => 'a',
                    1 => ' ',
                    2 => '0',
                    _ => '_',
                })
                .take(room)
                .collect();
            format!("{header}{}\n", if payload.is_empty() { "text" } else { &payload })
        }
        // This is intentionally malformed; formatter errors are accepted.
        1 => bounded_lossy(&[b'<', b'm', b'a', b't', b'c', b'h', b'\n', b'>']),
        // Arbitrary UTF-8-lossy editor input covers malformed delimiters and
        // non-ASCII text without exceeding the fixed input bound.
        _ => bounded_lossy(bytes),
    }
}

fn check_source(source: &str) -> Result<(), String> {
    let first = catch_unwind(AssertUnwindSafe(|| {
        lute_syntax::format_source(source, &lute_syntax::FormatOptions::default())
    }))
    .map_err(|_| "formatter panicked".to_string())?;
    let Ok(first) = first else {
        return Ok(());
    };
    let second = catch_unwind(AssertUnwindSafe(|| {
        lute_syntax::format_source(&first.text, &lute_syntax::FormatOptions::default())
    }))
    .map_err(|_| "formatter panicked on its own output".to_string())?
    .map_err(|error| format!("formatter rejected its own output: {error:?}"))?;
    if first.text != second.text {
        return Err(format!(
            "formatter is not idempotent\nfirst={:?}\nsecond={:?}",
            first.text, second.text
        ));
    }
    Ok(())
}

fn check_case(bytes: &[u8]) -> Result<(), String> {
    let source = generated_source(bytes);
    check_source(&source)
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
fn formatter_property() {
    // These are repository corpus samples, not just generated strings.
    for relative in [
        "../../conformance/choice-basic/source.lute",
        "../../conformance/command-lifecycle/source.lute",
        "../../conformance/cel-facts/source.lute",
        "../../conformance/hub-once-exit/source.lute",
    ] {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(relative);
        let source = fs::read_to_string(path).expect("read formatter corpus sample");
        assert!(source.len() <= MAX_INPUT);
        check_source(&source).unwrap_or_else(|error| panic!("corpus sample: {error}"));
    }

    let strategy = prop::collection::vec(any::<u8>(), 0..=MAX_INPUT);
    let mut runner = TestRunner::new(config());
    let result = runner.run(&strategy, |bytes| match check_case(&bytes) {
        Ok(()) => Ok(()),
        Err(error) => Err(TestCaseError::fail(error)),
    });
    if let Err(error) = result {
        let replay_path = persist_failure("formatter", &error);
        let mut replay_runner = TestRunner::new(config());
        let replay = replay_runner.run(&strategy, |bytes| match check_case(&bytes) {
            Ok(()) => Ok(()),
            Err(error) => Err(TestCaseError::fail(error)),
        });
        assert!(replay.is_err(), "fixed-seed formatter failure did not replay");
        panic!("formatter property failed with seed 0x{SEED:016x}; replay: {}\n{error}", replay_path.display());
    }
}
