//! The conformance corpus replay: `conformance/` is the executable acceptance
//! suite for the runtime contract (`conformance/README.md`), and this file is
//! CI's enforcement of it.
//!
//! Two invariants, each over EVERY fixture the corpus contains:
//!
//! 1. **Byte-identity replay** — `lute run <artifact> --mock <mock> --json`
//!    reproduces the checked-in `expected.json` byte for byte, and exits with
//!    the code the README's table pins to that transcript's `exit`. This is
//!    the contract a third-party engine is measured against.
//! 2. **Live stamps** — the recorded `artifact.json` carries TODAY's
//!    `irVersion` / `lute` / `capabilityVersion`, not whichever ones were
//!    current when it was last recorded. Invariant 1 alone cannot see this: a
//!    recorded artifact replayed against a recorded transcript stays green
//!    while the two are stale *together*. That is exactly how a
//!    `capabilityVersion` drift went unnoticed for two releases — `f876a2f`
//!    re-hashed the core capability snapshot and updated the insta snapshots,
//!    but never propagated to these fixtures.
//!
//! The fixture set is **discovered, never listed**: a seventh fixture is
//! picked up by both invariants the moment its directory lands. Requiring
//! someone to remember to add it to a list is precisely the failure mode that
//! produced the gap this file closes — five of the six fixtures were dead
//! weight in CI because only `end-reason` was ever replayed.
//!
//! What this file deliberately does NOT assert: full byte-equality between the
//! recorded `artifact.json` and a fresh recompile of `source.lute`. That holds
//! today, but the README freezes `artifact.json` on purpose — a third-party
//! engine must be able to conform without the Lute compiler — and golden-ing
//! whole compiler output is already the job of `lute-compile`'s insta corpus.
//! The stamps are the narrow, high-signal drift vector, so the stamps are what
//! is pinned to live values.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

/// Floor on the discovered fixture count. NOT a list of fixtures — adding one
/// needs no edit here — but a guard that discovery itself still works: a typo
/// in the corpus path or a lost directory would otherwise turn this whole
/// suite into a silent no-op, which is the same class of bug it exists to
/// catch. Removing an acceptance fixture is deliberate, so lowering this is
/// deliberate too.
const MIN_FIXTURES: usize = 6;

/// The corpus root, resolved from the crate manifest so the suite is
/// independent of the test process's working directory.
fn corpus_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../conformance")
}

/// Every fixture in the corpus, discovered by walking the tree: a directory
/// holding an `artifact.json`, which is the README replay loop's own
/// `[ -f "$d/artifact.json" ] || continue` filter. Name-sorted so failure
/// reports are deterministic.
fn fixtures() -> Vec<(String, PathBuf)> {
    let root = corpus_dir();
    let mut found: BTreeMap<String, PathBuf> = BTreeMap::new();
    let entries = std::fs::read_dir(&root)
        .unwrap_or_else(|e| panic!("cannot read corpus dir {}: {e}", root.display()));
    for entry in entries {
        let path = entry.expect("corpus dir entry").path();
        if !path.join("artifact.json").is_file() {
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        found.insert(name, path);
    }
    found.into_iter().collect()
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

fn json_at(path: &Path) -> serde_json::Value {
    serde_json::from_str(&read(path))
        .unwrap_or_else(|e| panic!("cannot parse {} as JSON: {e}", path.display()))
}

/// A scratch dir private to one fixture — no fixture shares mutable temp state
/// with another, so the suite is order- and concurrency-independent.
fn scratch(fixture: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join(format!("lute-conformance-{fixture}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The README's exit-code table: a complete walk exits `0`, an incomplete one
/// exits `3` (the §4.5 incomplete convention).
fn expected_code(exit: &str) -> i32 {
    match exit {
        "complete" => 0,
        "incomplete" => 3,
        other => panic!("unknown `exit` value {other:?} in a fixture transcript"),
    }
}

/// Invariant 1. Every fixture replays byte-identically to its checked-in
/// `expected.json`, with the README-pinned exit code.
///
/// Failures accumulate rather than aborting on the first, so one run names
/// every broken fixture instead of only the alphabetically-first one.
#[test]
fn every_fixture_replays_byte_identically() {
    let fixtures = fixtures();
    assert!(
        fixtures.len() >= MIN_FIXTURES,
        "discovered only {} fixture(s) in {} — expected at least {MIN_FIXTURES}; \
         corpus replay would be a no-op",
        fixtures.len(),
        corpus_dir().display(),
    );

    let mut failures: Vec<String> = Vec::new();
    for (name, dir) in &fixtures {
        let expected_path = dir.join("expected.json");
        let expected = read(&expected_path);
        let transcript: serde_json::Value = serde_json::from_str(&expected)
            .unwrap_or_else(|e| panic!("fixture {name}: expected.json is not JSON: {e}"));
        let want_code = expected_code(
            transcript["exit"]
                .as_str()
                .unwrap_or_else(|| panic!("fixture {name}: transcript has no `exit`")),
        );

        let mock = dir.join("mock.yaml");
        let mut args = vec!["run".to_string(), path_arg(&dir.join("artifact.json"))];
        if mock.is_file() {
            args.push("--mock".into());
            args.push(path_arg(&mock));
        }
        args.push("--json".into());

        let out = Command::new(BIN)
            .args(&args)
            .output()
            .unwrap_or_else(|e| panic!("fixture {name}: cannot spawn {BIN}: {e}"));

        let code = out.status.code();
        if code != Some(want_code) {
            failures.push(format!(
                "{name}: exit {code:?}, want {want_code} (transcript `exit` is {:?})\n  \
                 stderr: {}",
                transcript["exit"].as_str().unwrap_or("?"),
                String::from_utf8_lossy(&out.stderr).trim(),
            ));
            continue;
        }

        let got = String::from_utf8_lossy(&out.stdout).into_owned();
        if got != expected {
            failures.push(format!(
                "{name}: transcript is NOT byte-identical to {}\n{}",
                expected_path.display(),
                first_difference(&got, &expected),
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "{} of {} conformance fixture(s) failed to replay:\n\n{}",
        failures.len(),
        fixtures.len(),
        failures.join("\n\n"),
    );
}

/// Invariant 2. Every fixture's recorded `artifact.json` carries the stamps
/// the toolchain emits TODAY.
///
/// `irVersion` and `lute` are pinned to the live constants directly.
/// `capabilityVersion` is a content hash of the resolved capability snapshot,
/// so no constant exists to compare against — the live value is obtained by
/// recompiling the fixture's own `source.lute` through the same resolution
/// `lute compile` uses, which is step one of the README's regenerate recipe.
/// Comparing against the recompile (rather than a hardcoded core-only stamp)
/// keeps this honest for a future fixture that resolves a richer snapshot.
///
/// This is the assertion that would have caught `f876a2f`: the fixtures' stamp
/// sat at `78a2f619…` while the rest of the tree moved to `0678492f…`, and a
/// recorded-artifact-to-recorded-transcript replay could not see it.
#[test]
fn every_fixture_carries_live_stamps() {
    let fixtures = fixtures();
    assert!(
        fixtures.len() >= MIN_FIXTURES,
        "discovered only {} fixture(s) in {} — expected at least {MIN_FIXTURES}; \
         stamp check would be a no-op",
        fixtures.len(),
        corpus_dir().display(),
    );

    let mut failures: Vec<String> = Vec::new();
    for (name, dir) in &fixtures {
        let recorded = json_at(&dir.join("artifact.json"));

        for (field, live) in [
            ("irVersion", lute_compile::LUTE_IR_VERSION),
            ("lute", lute_compile::LUTE_LANG_VERSION),
        ] {
            let got = recorded[field].as_str().unwrap_or("<missing>");
            if got != live {
                failures.push(format!(
                    "{name}: artifact.json `{field}` is {got:?} but the live \
                     toolchain emits {live:?} — the fixture is stale; \
                     re-record it per conformance/README.md",
                ));
            }
        }

        // The `--json` transcript pins the major.minor IR line the engine
        // gated on, so it drifts on an IR bump the same way the artifact does.
        let transcript = json_at(&dir.join("expected.json"));
        let live_line = ir_line(lute_compile::LUTE_IR_VERSION);
        let got_line = transcript["irVersion"].as_str().unwrap_or("<missing>");
        if got_line != live_line {
            failures.push(format!(
                "{name}: expected.json `irVersion` is {got_line:?} but the live \
                 IR line is {live_line:?} — the transcript is stale",
            ));
        }

        let scratch = scratch(name);
        let fresh_path = scratch.join("artifact.json");
        let out = Command::new(BIN)
            .args(["compile", &path_arg(&dir.join("source.lute")), "-o", &path_arg(&fresh_path)])
            .output()
            .unwrap_or_else(|e| panic!("fixture {name}: cannot spawn {BIN}: {e}"));
        assert!(
            out.status.success(),
            "fixture {name}: recompiling source.lute failed: {}",
            String::from_utf8_lossy(&out.stderr),
        );

        let fresh = json_at(&fresh_path);
        let (want, got) = (
            fresh["capabilityVersion"].as_str().unwrap_or("<missing>"),
            recorded["capabilityVersion"].as_str().unwrap_or("<missing>"),
        );
        if got != want {
            failures.push(format!(
                "{name}: artifact.json `capabilityVersion` is {got} but \
                 recompiling source.lute today yields {want} — the capability \
                 surface moved and this fixture was left behind; re-record it \
                 per conformance/README.md",
            ));
        }
        let _ = std::fs::remove_dir_all(&scratch);
    }

    assert!(
        failures.is_empty(),
        "{} stale stamp(s) across {} conformance fixture(s):\n\n{}",
        failures.len(),
        fixtures.len(),
        failures.join("\n\n"),
    );
}

/// The corpus is well-formed: every discovered fixture carries the files the
/// README's layout table declares, so a half-added fixture fails loudly here
/// rather than being skipped by the replay's `artifact.json` filter.
#[test]
fn every_fixture_has_the_documented_layout() {
    let fixtures = fixtures();
    assert!(
        fixtures.len() >= MIN_FIXTURES,
        "discovered only {} fixture(s) in {} — expected at least {MIN_FIXTURES}",
        fixtures.len(),
        corpus_dir().display(),
    );

    let mut failures: Vec<String> = Vec::new();
    for (name, dir) in &fixtures {
        // `schema.yaml` is present only when the source needs one, so it is
        // not required; the other four always are.
        for file in ["source.lute", "artifact.json", "mock.yaml", "expected.json"] {
            if !dir.join(file).is_file() {
                failures.push(format!("{name}: missing {file}"));
            }
        }
    }

    assert!(failures.is_empty(), "malformed conformance fixture(s):\n{}", failures.join("\n"));
}

/// The major.minor line of a full `x.y.z` version — what the `--json`
/// transcript's `irVersion` records.
fn ir_line(full: &str) -> String {
    let mut parts = full.split('.');
    match (parts.next(), parts.next()) {
        (Some(major), Some(minor)) => format!("{major}.{minor}"),
        _ => full.to_string(),
    }
}

fn path_arg(p: &Path) -> String {
    p.to_str().unwrap_or_else(|| panic!("non-UTF-8 path {}", p.display())).to_string()
}

/// The first differing line, so a byte-identity failure points at the clause
/// that moved instead of dumping two whole transcripts.
fn first_difference(got: &str, want: &str) -> String {
    for (n, (g, w)) in got.lines().zip(want.lines()).enumerate() {
        if g != w {
            return format!("  line {}:\n    got:  {g}\n    want: {w}", n + 1);
        }
    }
    format!(
        "  identical for {} shared line(s); got has {} line(s), want has {}",
        got.lines().count().min(want.lines().count()),
        got.lines().count(),
        want.lines().count(),
    )
}
