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
    assert!(
        out.status.success(),
        "stderr: {}\nstdout: {stdout}",
        String::from_utf8_lossy(&out.stderr)
    );

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
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );

    let v: serde_json::Value = serde_json::from_slice(&first.stdout).unwrap_or_else(|e| {
        panic!(
            "--json output must parse: {e}\n{}",
            String::from_utf8_lossy(&first.stdout)
        )
    });
    for key in [
        "file",
        "seeds",
        "steps",
        "decisions",
        "unresolved",
        "coverage",
    ] {
        assert!(v.get(key).is_some(), "top-level key `{key}` missing: {v}");
    }

    let second = trace(&args);
    assert_eq!(
        first.stdout, second.stdout,
        "identical inputs must produce byte-identical --json output (dsl 0.4.0 §4.5)"
    );
}

// --- §4.3: "MUST refuse a document with check errors (exit 1; run check
// first)". `arcia-project/date-minigame.lute` carries real check errors when
// resolved core-only (no `--project`) — the SAME fixture `cli.rs`'s
// `check_file_with_errors_exits_one` pins for `lute check`.

#[test]
fn refused_on_check_errors() {
    let out = trace(&["../../docs/examples/arcia-project/date-minigame.lute"]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "a check-error document must refuse with exit 1"
    );
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
    assert_eq!(
        out.status.code(),
        Some(1),
        "an invalid mock must refuse with exit 1"
    );
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
// rejected, §4.3); under `--no-derive` `reach`/`learn` read derived
// relations with no supplying `--fact` -> unresolved -> exit 3.

#[test]
fn incomplete_exits_3() {
    let out = trace(&[
        "../../docs/examples/quest-rescue-halsin.lute",
        "--fact",
        "inParty(shadowheart)",
        "--project",
        "../../docs/examples",
        "--no-derive",
    ]);
    assert_eq!(
        out.status.code(),
        Some(3),
        "an unresolved objective atom must halt the trace incomplete: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("derived relation `believesLocation`"),
        "each derived read is noted under --no-derive: {stdout}"
    );
}

// --- dsl 0.22.0 §6: by default the seeds load and the rules apply — the
// same trace completes, both derived objectives decided by the rules.

#[test]
fn derivation_completes_the_quest_by_default() {
    let out = trace(&[
        "../../docs/examples/quest-rescue-halsin.lute",
        "--project",
        "../../docs/examples",
    ]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(0), "{stdout}");
    assert!(stdout.contains("rescueHalsin"), "{stdout}");
    assert!(!stdout.contains("unresolved"), "{stdout}");
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
    assert_eq!(
        out.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("E-TRACE-EVENT"),
        "expected E-TRACE-EVENT in output: {stdout}"
    );
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
    assert_eq!(
        out.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("E-TRACE-ACCEPT"),
        "expected E-TRACE-ACCEPT in output: {stdout}"
    );
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

// --- §3.1 under `--no-derive` (dsl 0.22.0 §6): the resolved schema (`act1.schema.yaml`, imported via `uses:`)
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
        "--no-derive",
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
    assert!(
        stdout.contains("--fact"),
        "note must point authors at --fact: {stdout}"
    );
    // The banner still reports the seeded (mock) counts unaffected by the note.
    assert!(
        stdout.contains("0 facts"),
        "seeds banner must still report the (unaffected) mock count: {stdout}"
    );
}

// --- §3.3: a component-expanding trace's human transcript must not leak
// the internal `__component-begin`/`__component-end` sentinels, nor any
// doubled marker word ("begin begin" / "end end").

#[test]
fn component_expansion_transcript_has_no_sentinel_leak() {
    let out = trace(&["../../docs/examples/components/scene.lute"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "stderr: {}\nstdout: {stdout}",
        String::from_utf8_lossy(&out.stderr)
    );
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
    assert_eq!(
        out.status.code(),
        Some(2),
        "a mock naming the wrong subject is an input error:\n{text}"
    );
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
    let out = trace(&[
        "../../docs/examples/haven/scenes/wake.lute",
        "--project",
        "../../docs/examples/haven",
    ]);
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(
        text.contains("<auto exit>"),
        "the exit must be marked: {text}"
    );
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
    let out = trace(&[
        "../../docs/examples/haven/scenes/bridge.lute",
        "--project",
        "../../docs/examples/haven",
    ]);
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(text.contains("<end reason=bridge-reached>"), "{text}");

    let out = trace(&[
        "../../docs/examples/haven/scenes/bridge.lute",
        "--project",
        "../../docs/examples/haven",
        "--json",
    ]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("trace --json");
    assert_eq!(
        v["disposition"], "ended",
        "a harness must tell a terminated walk from a spent one"
    );
    assert_eq!(v["endReason"], "bridge-reached", "{v:#?}");

    // A scene that runs out of nodes is `complete`, not `ended`, and carries
    // no reason — that is the distinction the field exists for.
    let out = trace(&[
        "../../docs/examples/haven/scenes/wake.lute",
        "--project",
        "../../docs/examples/haven",
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
    let out = trace(&[
        "../../docs/examples/haven/scenes/hydroponics.lute",
        "--project",
        "../../docs/examples/haven",
    ]);
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
    assert!(
        text.contains("-> right"),
        "the supplied selection is taken:\n{text}"
    );

    let (code, text) = run("selections:\n  pick: right\n");
    assert_eq!(
        code,
        Some(2),
        "a mis-keyed mock surface is an input error:\n{text}"
    );
    assert!(
        text.contains("E-TRACE-MOCK-PARSE") && text.contains("`selections`"),
        "{text}"
    );
    assert!(
        !text.contains("(auto)"),
        "the excluded arm must never be reached — that was the whole defect:\n{text}"
    );
}

// ── 0.21.0 §7a: quests meet scenes and occasions ───────────────────────

/// A shape-only project: scene `haven.shed` whose `take` choice accepts the
/// start-less `sideJob`, and quest `holdLine` whose `sawShed` objective reads
/// `visited('haven.shed')` and whose `calm` objective is judged at `runEnd`
/// (true whenever judged: `run.pressure` defaults to 0).
fn quest_occasion_project(tag: &str) -> PathBuf {
    let dir = temp_dir(tag);
    let files = [
        (
            "lute.project.yaml",
            "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n",
        ),
        (
            "world.schema.yaml",
            "state:\n  run.pressure: { type: number, default: 0 }\n",
        ),
        (
            "scenes/shed.lute",
            "---\nkind: scene\nid: haven.shed\nuses: ../world.schema.yaml\non: hubVisit\n---\n\n\
             ## Shed\n\n@guard: The shed is quiet.\n\n<branch id=\"offer\">\n\
             <choice id=\"take\" label=\"Take the job\">\n@guard: Deal.\n\
             ::accept{quest=\"sideJob\"}\n</choice>\n\
             <choice id=\"pass\" label=\"Pass\">\n@guard: Suit yourself.\n</choice>\n</branch>\n",
        ),
        (
            "quests/hold.lute",
            "---\nkind: quest\nuses: ../world.schema.yaml\ntitle: Hold\n---\n\n\
             <quest id=\"holdLine\" title=\"Hold the line\" start=\"true\">\n\
             <objective id=\"sawShed\" title=\"See the shed\" done=\"visited('haven.shed')\"/>\n\
             <objective id=\"calm\" title=\"Keep calm\" on=\"runEnd\" done=\"run.pressure < 2\"/>\n\
             </quest>\n\n\
             <quest id=\"sideJob\" title=\"Side job\">\n\
             <objective id=\"paid\" title=\"Get paid\" done=\"run.pressure > 5\"/>\n\
             </quest>\n",
        ),
    ];
    for (rel, text) in files {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
    dir
}

/// `lute trace <dir>/quests/hold.lute --project <dir> --json <extra…>`:
/// asserts exit 0 and returns the report.
fn trace_quest_json(dir: &std::path::Path, extra: &[&str]) -> serde_json::Value {
    let quest = dir.join("quests/hold.lute");
    let mut args = vec![
        quest.to_str().unwrap(),
        "--project",
        dir.to_str().unwrap(),
        "--json",
    ];
    args.extend_from_slice(extra);
    let out = trace(&args);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

/// Every outcome recorded for `construct` `id`, in walk order.
fn outcomes<'a>(v: &'a serde_json::Value, construct: &str, id: &str) -> Vec<&'a str> {
    v["decisions"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|d| d["construct"] == construct && d["id"] == id)
        .map(|d| d["outcome"].as_str().unwrap())
        .collect()
}

fn has_note(v: &serde_json::Value, needle: &str) -> bool {
    v["notes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|n| n.as_str().unwrap().contains(needle))
}

#[test]
fn trace_visited_mock_makes_a_visited_objective_done() {
    let dir = quest_occasion_project("trace-visited");

    // No `visited:`: closed-world false — pending, never unresolved, exit 0.
    let v = trace_quest_json(&dir, &[]);
    let saw = outcomes(&v, "objective", "sawShed");
    assert!(!saw.is_empty() && saw.iter().all(|o| *o == "pending"), "{saw:?}");
    assert_eq!(v["unresolved"], serde_json::json!([]));

    std::fs::write(dir.join("m.yaml"), "visited: [haven.shed]\n").unwrap();
    let v = trace_quest_json(&dir, &["--mock", dir.join("m.yaml").to_str().unwrap()]);
    assert!(outcomes(&v, "objective", "sawShed").contains(&"done"), "{v}");
}

#[test]
fn trace_judges_an_on_objective_only_when_the_occasion_is_raised() {
    let dir = quest_occasion_project("trace-occasion");
    std::fs::write(dir.join("seen.yaml"), "visited: [haven.shed]\n").unwrap();
    std::fs::write(
        dir.join("seen-end.yaml"),
        "visited: [haven.shed]\noccasions: [runEnd]\n",
    )
    .unwrap();
    let mock = |m: &str| dir.join(m).to_str().unwrap().to_string();

    // Every continuous objective done, but `runEnd` never raised: `calm` is
    // never judged, the quest stays active, and the note says why.
    let v = trace_quest_json(&dir, &["--mock", &mock("seen.yaml")]);
    assert!(outcomes(&v, "objective", "calm").is_empty(), "{v}");
    assert_eq!(outcomes(&v, "quest", "holdLine"), ["active"]);
    assert!(
        has_note(
            &v,
            "objective `holdLine.calm` is judged at occasion `runEnd`, which this walk never \
             raised (supply `--occasion runEnd` or `occasions: [runEnd]`)"
        ),
        "{}",
        v["notes"]
    );

    // Raised by the mock key, or by the flag: judged, and the quest completes.
    for extra in [
        vec!["--mock".to_string(), mock("seen-end.yaml")],
        vec![
            "--mock".to_string(),
            mock("seen.yaml"),
            "--occasion".to_string(),
            "runEnd".to_string(),
        ],
    ] {
        let args: Vec<&str> = extra.iter().map(String::as_str).collect();
        let v = trace_quest_json(&dir, &args);
        assert_eq!(outcomes(&v, "objective", "calm"), ["done"], "{extra:?}");
        assert_eq!(outcomes(&v, "quest", "holdLine"), ["active", "complete"], "{extra:?}");
        assert!(!has_note(&v, "never raised"), "{extra:?}: {}", v["notes"]);
    }

    // An occasion no objective is judged at is noted, not an error.
    let v = trace_quest_json(&dir, &["--occasion", "hubVisit"]);
    assert!(
        has_note(&v, "occasion `hubVisit` is judged by no `<objective on>`"),
        "{}",
        v["notes"]
    );
}

#[test]
fn trace_prints_a_scene_accept_on_the_branch_that_takes_it() {
    let dir = quest_occasion_project("trace-accept");
    let scene = dir.join("scenes/shed.lute");
    let run = |choice: &str| {
        let out = trace(&[
            scene.to_str().unwrap(),
            "--project",
            dir.to_str().unwrap(),
            "--choose",
            &format!("offer={choice}"),
        ]);
        assert_eq!(out.status.code(), Some(0));
        String::from_utf8_lossy(&out.stdout).into_owned()
    };
    let text = run("take");
    assert!(
        text.lines().any(|l| l == "    quest sideJob accepted"),
        "{text}"
    );
    let text = run("pass");
    assert!(!text.contains("accepted"), "{text}");
}

#[test]
fn run_raises_an_occasion_on_a_compiled_quest_artifact() {
    let dir = quest_occasion_project("run-occasion");
    let art = dir.join("hold.json");
    let out = Command::new(BIN)
        .args([
            "compile",
            dir.join("quests/hold.lute").to_str().unwrap(),
            "--project",
            dir.to_str().unwrap(),
            "-o",
            art.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "compile: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let mock = dir.join("m.yaml");
    std::fs::write(&mock, "visited: [haven.shed]\n").unwrap();
    let run = |extra: &[&str]| {
        let mut args = vec![
            "run",
            art.to_str().unwrap(),
            "--mock",
            mock.to_str().unwrap(),
        ];
        args.extend_from_slice(extra);
        let out = Command::new(BIN).args(&args).output().unwrap();
        assert_eq!(
            out.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    };

    let v: serde_json::Value = serde_json::from_str(&run(&["--json"])).unwrap();
    assert_eq!(v["quests"]["holdLine"], "active", "{v}");

    let v: serde_json::Value =
        serde_json::from_str(&run(&["--occasion", "runEnd", "--json"])).unwrap();
    assert_eq!(v["quests"]["holdLine"], "complete", "{v}");
    let records = v["commands"].as_array().unwrap();
    let at = |pred: &dyn Fn(&serde_json::Value) -> bool| {
        records
            .iter()
            .position(|r| pred(r))
            .unwrap_or_else(|| panic!("{v}"))
    };
    let raised = at(&|r| r["kind"] == "occasion" && r["occasion"] == "runEnd");
    let judged = at(&|r| r["kind"] == "objective" && r["objective"] == "calm");
    let complete = at(&|r| r["kind"] == "quest" && r["state"] == "complete");
    assert!(raised < judged && judged < complete, "{v}");

    let text = run(&["--occasion", "runEnd"]);
    assert!(text.lines().any(|l| l == "  occasion runEnd"), "{text}");
}

/// dsl 0.24.0 §1 (T2-1): integer `%` checks clean and `lute trace` / `lute
/// test` evaluate it; a fractional literal operand is `E-CEL-TYPE`. `%` used
/// to be `E-CEL-PROFILE`, so trace and test refused the document.
#[test]
fn integer_modulo_checks_and_evaluates_in_trace_and_test() {
    let dir = temp_dir("modulo");
    let scene = |test: &str| {
        format!(
            "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n\
             state:\n  run.day: {{ type: number, default: 1 }}\n---\n\n## S\n\n\
             <match on=\"run.day\">\n<when test=\"{test}\">\n@narrator: Sunday.\n</when>\n\
             <otherwise>\n@narrator: Weekday.\n</otherwise>\n</match>\n"
        )
    };
    let file = dir.join("s.lute");
    std::fs::write(&file, scene("$ % 7 == 0")).unwrap();
    let f = file.to_str().unwrap();
    let transcript = |day: &str| {
        let out = trace(&[f, "--state", &format!("run.day={day}")]);
        assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8_lossy(&out.stdout).to_string()
    };
    let t = transcript("14");
    assert!(t.contains("Sunday.") && !t.contains("Weekday."), "{t}");
    let t = transcript("15");
    assert!(t.contains("Weekday.") && !t.contains("Sunday."), "{t}");

    std::fs::write(
        dir.join("t.test.yaml"),
        "file: s.lute\nstate:\n  run.day: 21\nexpect:\n  transcriptContains: [\"Sunday.\"]\n",
    )
    .unwrap();
    let out = Command::new(BIN).args(["test", dir.to_str().unwrap()]).output().unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stdout));

    std::fs::remove_file(dir.join("t.test.yaml")).unwrap();
    std::fs::write(&file, scene("$ % 2.5 == 0")).unwrap();
    let out = Command::new(BIN).args(["check", f]).output().unwrap();
    let all = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.status.code(), Some(1), "{all}");
    assert!(all.contains("E-CEL-TYPE") && all.contains("`2.5` is not an integer"), "{all}");
    assert!(!all.contains("E-CEL-PROFILE"), "{all}");
}

/// dsl 0.24.0 T3-12: a `<match>` subject and an arm guard print as the
/// author wrote them (`@weekday`, `@atLeast(3)`); `--expand` prints the
/// expansion the walk evaluated, with no parentheses around an atomic
/// argument; `--json` keeps the expansion and adds the authored text.
#[test]
fn trace_prints_authored_def_refs_and_expands_on_request() {
    let dir = temp_dir("authored-defs");
    std::fs::create_dir_all(dir.join("scenes")).unwrap();
    std::fs::write(
        dir.join("world.schema.yaml"),
        "state:\n  run.day: { type: number, default: 1 }\n\
         defs:\n  weekday: \"run.day == 1 ? 'mon' : 'tue'\"\n  \
         atLeast: { type: bool, cel: \"run.day >= n\", params: { n: number } }\n",
    )
    .unwrap();
    let scene = dir.join("scenes/m.lute");
    std::fs::write(
        &scene,
        "---\nkind: scene\ncharacter: a\nseason: 1\nepisode: 1\nuses: [../world.schema.yaml]\n---\n\n## M\n\n\
         <match on=\"@weekday\">\n<when is=\"'mon'\">\n@a: Monday.\n</when>\n\
         <otherwise>\n@a: Other.\n</otherwise>\n</match>\n\
         <match on=\"true\">\n<when test=\"@atLeast(3)\">\n@a: Late.\n</when>\n\
         <otherwise>\n@a: Early.\n</otherwise>\n</match>\n",
    )
    .unwrap();
    let path = scene.to_str().unwrap();
    let human = |extra: &[&str]| {
        let mut args = vec![path, "--state", "run.day=4"];
        args.extend_from_slice(extra);
        let out = trace(&args);
        let s = String::from_utf8_lossy(&out.stdout).to_string();
        assert_eq!(out.status.code(), Some(0), "{s}{}", String::from_utf8_lossy(&out.stderr));
        s
    };

    let authored = human(&[]);
    assert!(authored.contains("<match @weekday>   -> otherwise"), "{authored}");
    assert!(authored.contains("<match true>   -> arm 1 (@atLeast(3))"), "{authored}");
    assert!(authored.contains("arms 1/2 (@weekday @"), "{authored}");
    assert!(!authored.contains("run.day"), "{authored}");

    let expanded = human(&["--expand"]);
    assert!(
        expanded.contains("<match (run.day == 1 ? 'mon' : 'tue')>   -> otherwise"),
        "{expanded}"
    );
    assert!(expanded.contains("-> arm 1 ((run.day >= 3))"), "{expanded}");

    let json = human(&["--json"]);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    let first = &v["decisions"][0];
    assert_eq!(first["id"], "(run.day == 1 ? 'mon' : 'tue')");
    assert_eq!(first["authoredId"], "@weekday");
    assert_eq!(v["decisions"][1]["guard"], "(run.day >= 3)");
    assert_eq!(v["decisions"][1]["authoredGuard"], "@atLeast(3)");
    // An unchanged construct carries no authored key.
    assert!(v["decisions"][1].get("authoredId").is_none(), "{json}");
}
