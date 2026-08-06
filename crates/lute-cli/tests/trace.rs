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

// ── 0.10.0 §8 / D-AC: the mock's subject key on a trace command line ───

/// A fresh unique temp dir (matches `check_project.rs`'s own helper — each
/// integration test binary is compiled separately, so this is intentionally
/// duplicated rather than shared).
fn temp_dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("lute-cli-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// D-AC's third rule: `lute trace <doc> --mock m.yaml` supplies the subject
/// on the command line and THAT wins. A `file:` naming a different document
/// is `E-MOCK-SUBJECT` — the two ways of saying what a mock is for must not
/// be able to disagree in silence.
#[test]
fn trace_refuses_a_mock_whose_file_names_a_different_document() {
    let dir = temp_dir("mock-subject-disagree");
    std::fs::create_dir_all(dir.join("scenes")).unwrap();
    let scene = "---\nkind: scene\ncharacter: a\nseason: 1\nepisode: 1\n---\n\n## S\n\n@a: hi\n";
    std::fs::write(dir.join("scenes/one.lute"), scene).unwrap();
    std::fs::write(dir.join("scenes/two.lute"), scene).unwrap();
    std::fs::write(dir.join("m.yaml"), "file: scenes/two.lute\n").unwrap();

    let out = std::process::Command::new(BIN)
        .args([
            "trace",
            dir.join("scenes/one.lute").to_str().unwrap(),
            "--mock",
            dir.join("m.yaml").to_str().unwrap(),
        ])
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.status.code(), Some(2), "a mock naming the wrong subject is an input error:\n{text}");
    assert!(text.contains("E-MOCK-SUBJECT"), "{text}");
}

/// The agreeing case, and the absent case, both run.
#[test]
fn trace_accepts_an_agreeing_or_absent_file_key() {
    let dir = temp_dir("mock-subject-agree");
    std::fs::create_dir_all(dir.join("scenes")).unwrap();
    std::fs::write(
        dir.join("scenes/one.lute"),
        "---\nkind: scene\ncharacter: a\nseason: 1\nepisode: 1\n---\n\n## S\n\n@a: hi\n",
    )
    .unwrap();
    std::fs::write(dir.join("agree.yaml"), "file: scenes/one.lute\n").unwrap();
    std::fs::write(dir.join("absent.yaml"), "state: {}\n").unwrap();
    for m in ["agree.yaml", "absent.yaml"] {
        let out = std::process::Command::new(BIN)
            .args([
                "trace",
                dir.join("scenes/one.lute").to_str().unwrap(),
                "--mock",
                dir.join(m).to_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert_eq!(
            out.status.code(),
            Some(0),
            "{m}: {}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

/// #32 / T2.5: the entrance and the exit are the same construct with the same
/// attribute names, and the entire difference is which of `brace` and
/// `go-under` appears in a list in another file. `trace` printed both as
/// `<auto>`. wake.lute's LAST line is the corpus's single declared exit.
#[test]
fn trace_marks_an_exiting_auto_as_an_exit() {
    let out = trace(&["../../docs/examples/anseo/scenes/wake.lute", "--project", "../../docs/examples/anseo"]);
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(text.contains("<auto exit>"), "the exit must be marked: {text}");
    assert!(
        text.lines().any(|l| l.trim() == "<auto>"),
        "the entrance must NOT be marked: {text}"
    );
}

/// #32 / T5.9: `reason` is not one attribute among several — it is the
/// terminator's entire payload, the only thing distinguishing `::end` from
/// falling off the end of the document. A project with several endings
/// previewed them all as an identical `<end>`.
#[test]
fn trace_renders_the_end_reason_and_reports_a_disposition() {
    let out = trace(&["../../docs/examples/anseo/scenes/bridge.lute", "--project", "../../docs/examples/anseo"]);
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(text.contains("<end reason=bridge-reached>"), "{text}");

    let out = trace(&[
        "../../docs/examples/anseo/scenes/bridge.lute",
        "--project",
        "../../docs/examples/anseo",
        "--json",
    ]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("trace --json");
    assert_eq!(v["disposition"], "ended", "a harness must tell a terminated walk from a spent one");
    assert_eq!(v["endReason"], "bridge-reached", "{v:#?}");

    // A scene that runs out of nodes is `complete`, not `ended`, and carries
    // no reason — that is the distinction the field exists for.
    let out = trace(&[
        "../../docs/examples/anseo/scenes/wake.lute",
        "--project",
        "../../docs/examples/anseo",
        "--json",
    ]);
    let vw: serde_json::Value = serde_json::from_slice(&out.stdout).expect("trace --json");
    assert_eq!(vw["disposition"], "complete");
    assert!(vw["endReason"].is_null(), "{vw:#?}");
}

/// #10 row h: the shipped binary printed `## Shot 1.` on the doc page's own
/// file and command, while the heading sat in the IR
/// (`"shots":[{"shot":1,"heading":"Hydroponics"}]`). Doing the tool fix
/// retires the docs row.
#[test]
fn trace_prints_the_shot_heading_it_is_holding() {
    let out = trace(&["../../docs/examples/anseo/scenes/hydroponics.lute", "--project", "../../docs/examples/anseo"]);
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(text.contains("## Hydroponics"), "{text}");
}

/// T3.10: a mis-keyed `choose:` used to be dropped in silence, and the walk
/// then auto-picked the FIRST eligible arm — the one the file excluded — and
/// exited 0. The mock family's key set is closed as of 0.10.0 §8, so the
/// typo is now an input error and the excluded arm is never reached.
#[test]
fn trace_refuses_a_mock_that_mis_keys_a_surface() {
    let dir = temp_dir("mock-closed-keys");
    std::fs::create_dir_all(dir.join("scenes")).unwrap();
    std::fs::write(
        dir.join("scenes/one.lute"),
        "---\nkind: scene\ncharacter: a\nseason: 1\nepisode: 1\n---\n\n## S\n\n\
         <branch id=\"pick\">\n\
         <choice id=\"left\" label=\"L\">\n@a: left\n</choice>\n\
         <choice id=\"right\" label=\"R\">\n@a: right\n</choice>\n\
         </branch>\n",
    )
    .unwrap();
    let scene = dir.join("scenes/one.lute");
    let run = |mock: &str| {
        std::fs::write(dir.join("m.yaml"), mock).unwrap();
        let out = std::process::Command::new(BIN)
            .args([
                "trace",
                scene.to_str().unwrap(),
                "--mock",
                dir.join("m.yaml").to_str().unwrap(),
            ])
            .output()
            .unwrap();
        (
            out.status.code(),
            format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            ),
        )
    };

    // Control: the correctly keyed file still runs, and picks `right` — so a
    // gate that refused every mock could not pass this test.
    let (code, text) = run("choose:\n  pick: right\n");
    assert_eq!(code, Some(0), "{text}");
    assert!(text.contains("-> right"), "the supplied selection is taken:\n{text}");

    let (code, text) = run("selections:\n  pick: right\n");
    assert_eq!(code, Some(2), "a mis-keyed mock surface is an input error:\n{text}");
    assert!(text.contains("E-TRACE-MOCK-PARSE") && text.contains("`selections`"), "{text}");
    assert!(
        !text.contains("(auto)"),
        "the excluded arm must never be reached — that was the whole defect:\n{text}"
    );
}
