//! `lute trace` end-to-end (dsl 0.4.0 §4.3/§4.5/§4.6, Task 21): spawn the
//! built `lute` binary and assert exit codes + output, the `examples_check.rs`
//! binary-spawn idiom. Pins the CLI grammar, the exit-code map (`Complete`->0,
//! `Refused`->1, `Incomplete`->3), the `--json` determinism contract, and the
//! ONE reverse Cargo edge this task wires (D15).

use std::path::PathBuf;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn trace(args: &[&str]) -> std::process::Output {
    Command::new(BIN).arg("trace").args(args).output().unwrap()
}

// --- §4.6 worked example: `docs/examples/choice-persist.lute`
// `--choose sofaHelp=help` -> exit 0; the transcript names the branch
// decision, the into-sugar `::set`, the arm-1 match decision, and the
// trailing coverage summary ("choices 1/3", "arms 1/2").

#[test]
fn choice_persist_worked_example() {
    let out = trace(&[
        "../../docs/examples/choice-persist.lute",
        "--choose",
        "sofaHelp=help",
    ]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "stderr: {}\nstdout: {stdout}", String::from_utf8_lossy(&out.stderr));

    assert!(
        stdout.contains("<branch sofaHelp>") && stdout.contains("-> help"),
        "missing branch decision line: {stdout}"
    );
    assert!(
        stdout.contains("::set  run.metHelpfully = true  (into sugar)"),
        "missing into-sugar ::set: {stdout}"
    );
    assert!(
        stdout.contains("<match run.metHelpfully>") && stdout.contains("-> arm 1"),
        "missing arm-1 match decision: {stdout}"
    );
    assert!(
        stdout.contains("1/3") && stdout.contains("1/2"),
        "missing coverage summary (choices 1/3, arms 1/2): {stdout}"
    );
}

// --- Machine form (§4.5): top-level keys `file`/`seeds`/`steps`/
// `decisions`/`unresolved`/`coverage` are normative; identical inputs (same
// document, mocks, flag order) MUST produce byte-identical output.

#[test]
fn json_contract() {
    let args = [
        "../../docs/examples/choice-persist.lute",
        "--choose",
        "sofaHelp=help",
        "--json",
    ];
    let first = trace(&args);
    assert!(first.status.success(), "{}", String::from_utf8_lossy(&first.stderr));

    let v: serde_json::Value = serde_json::from_slice(&first.stdout)
        .unwrap_or_else(|e| panic!("--json output must parse: {e}\n{}", String::from_utf8_lossy(&first.stdout)));
    for key in ["file", "seeds", "steps", "decisions", "unresolved", "coverage"] {
        assert!(v.get(key).is_some(), "top-level key `{key}` missing: {v}");
    }

    let second = trace(&args);
    assert_eq!(
        first.stdout, second.stdout,
        "identical inputs must produce byte-identical --json output (dsl 0.4.0 §4.5)"
    );
}

// --- §4.3: "MUST refuse a document with check errors (exit 1; run check
// first)". `idola-project/date-minigame.lute` carries real check errors when
// resolved core-only (no `--project`) — the SAME fixture `cli.rs`'s
// `check_file_with_errors_exits_one` pins for `lute check`.

#[test]
fn refused_on_check_errors() {
    let out = trace(&["../../docs/examples/idola-project/date-minigame.lute"]);
    assert_eq!(out.status.code(), Some(1), "a check-error document must refuse with exit 1");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("[E-UNKNOWN-DIRECTIVE]") || stdout.contains("[E-UNDECLARED]"),
        "check diagnostics must render in the check-diagnostic line format: {stdout}"
    );
    assert!(
        stdout.to_lowercase().contains("run") && stdout.to_lowercase().contains("check"),
        "refusal message must say to run check first: {stdout}"
    );
}

// --- §4.3: an undeclared `--state` path is `E-TRACE-MOCK-UNDECLARED`
// ("state-by-typo MUST fail in mocks exactly as in documents") — a typo'd
// `run.metHelpfuly` against choice-persist's declared `run.metHelpfully`.

#[test]
fn bad_mock_exits_1() {
    let out = trace(&[
        "../../docs/examples/choice-persist.lute",
        "--state",
        "run.metHelpfuly=true",
    ]);
    assert_eq!(out.status.code(), Some(1), "an invalid mock must refuse with exit 1");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("[E-TRACE-MOCK-UNDECLARED]"),
        "must render E-TRACE-MOCK-UNDECLARED in the check-diagnostic line format: {stdout}"
    );
}

// --- §4.6 quest transcript: rescue-halsin activates DECLARATIVELY on the
// supplied `inParty` fact (`start="holds(inParty(shadowheart))"`, dsl
// 0.4.0 §4.4); `questActive` fires automatically from that ONE transition
// (no `--event questActive` — that lifecycle name is now `E-TRACE-EVENT`-
// rejected, §4.3); `reach`/`learn` read derived relations with no
// supplying `--fact` -> unresolved -> trace incomplete, exit 3.

#[test]
fn incomplete_exits_3() {
    let out = trace(&[
        "../../docs/examples/quest-rescue-halsin.lute",
        "--fact",
        "inParty(shadowheart)",
        "--project",
        "../../docs/examples",
    ]);
    assert_eq!(
        out.status.code(),
        Some(3),
        "an unresolved objective atom must halt the trace incomplete: {}",
        String::from_utf8_lossy(&out.stdout)
    );
}

// --- §4.3/§4.4: `--event questActive` (a built-in lifecycle event) is
// `E-TRACE-EVENT` — engine-derived, never user-fired — exit 1, refused.

#[test]
fn event_lifecycle_name_exits_1_with_trace_event() {
    let out = trace(&[
        "../../docs/examples/quest-rescue-halsin.lute",
        "--fact",
        "inParty(shadowheart)",
        "--event",
        "questActive",
        "--project",
        "../../docs/examples",
    ]);
    assert_eq!(out.status.code(), Some(1), "{}", String::from_utf8_lossy(&out.stdout));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("E-TRACE-EVENT"), "expected E-TRACE-EVENT in output: {stdout}");
}

// --- §4.3/§4.4: `--accept` on rescueHalsin (a `start`-having, declarative
// quest) is `E-TRACE-ACCEPT` — it activates on its own and needs no accept.

#[test]
fn accept_on_start_having_quest_exits_1_with_trace_accept() {
    let out = trace(&[
        "../../docs/examples/quest-rescue-halsin.lute",
        "--accept",
        "rescueHalsin",
        "--project",
        "../../docs/examples",
    ]);
    assert_eq!(out.status.code(), Some(1), "{}", String::from_utf8_lossy(&out.stdout));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("E-TRACE-ACCEPT"), "expected E-TRACE-ACCEPT in output: {stdout}");
}

// --- D15/T17: the positive half of the quarantine test — `lute-cli`'s OWN
// manifest names `lute-trace` (the ONE reverse edge); `lute-trace/tests/
// quarantine.rs` pins the negative half (the seven non-CLI crates never do).

#[test]
fn quarantine_edge_is_cli_only() {
    let manifest_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    let manifest = std::fs::read_to_string(&manifest_path).unwrap();
    assert!(
        manifest.contains("lute-trace"),
        "lute-cli/Cargo.toml must name lute-trace (D15, the ONE reverse edge): {manifest}"
    );
}

// --- §3.1: the resolved schema (`act1.schema.yaml`, imported via `uses:`)
// declares seed `facts:` but NO `--fact` is supplied at all -> trace prints
// an informational note naming a declared seed relation and saying schema
// facts are not auto-loaded, supplied via `--fact`. Never an error: exit
// stays whatever the (unaffected) walk decides on the empty explicit set.

#[test]
fn declares_seed_facts_with_no_mocks_prints_not_auto_loaded_note() {
    let out = trace(&[
        "../../docs/examples/quest-rescue-halsin.lute",
        "--project",
        "../../docs/examples",
    ]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.code() == Some(0) || out.status.code() == Some(3),
        "an informational note must never change the exit code: {}\n{stdout}",
        String::from_utf8_lossy(&out.stderr)
    );
    let lower = stdout.to_lowercase();
    assert!(
        lower.contains("note:") && lower.contains("not auto-load"),
        "missing §3.1 not-auto-loaded note: {stdout}"
    );
    assert!(stdout.contains("--fact"), "note must point authors at --fact: {stdout}");
    // The banner still reports the seeded (mock) counts unaffected by the note.
    assert!(stdout.contains("0 facts"), "seeds banner must still report the (unaffected) mock count: {stdout}");
}

// --- §3.3: a component-expanding trace's human transcript must not leak
// the internal `__component-begin`/`__component-end` sentinels, nor any
// doubled marker word ("begin begin" / "end end").

#[test]
fn component_expansion_transcript_has_no_sentinel_leak() {
    let out = trace(&["../../docs/examples/components/scene.lute"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "stderr: {}\nstdout: {stdout}", String::from_utf8_lossy(&out.stderr));
    assert!(
        !stdout.contains("__component-begin") && !stdout.contains("__component-end"),
        "the internal component sentinel must never leak into the human transcript: {stdout}"
    );
    assert!(
        !stdout.contains("begin begin") && !stdout.contains("end end"),
        "a doubled marker word must never appear: {stdout}"
    );
    // The boundary is still visible in some clean form (a trace reader can
    // still tell inlined component content apart from the document's own).
    assert!(
        stdout.contains("component begin") && stdout.contains("component end"),
        "the component boundary itself should still be signposted, just cleanly: {stdout}"
    );
}
