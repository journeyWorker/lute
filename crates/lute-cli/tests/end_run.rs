//! dsl 0.8.0 `::end` in the reference engine (`lute run`, `src/runner.rs`):
//! the `end` record stops the walk, surfaces its `reason`, and still exits
//! `complete` — behaviorally identical to running off the end of `commands`.
//!
//! The `conformance/end-reason` replay that used to live here moved to
//! `tests/conformance.rs`, which replays EVERY fixture in the corpus instead
//! of this one — a strict superset of the check it replaces.

use std::path::PathBuf;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn temp_dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("lute-end-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Compile `source` in a scratch dir and run it (optionally under `mock`),
/// returning the `--json` machine transcript.
fn compile_and_run(tag: &str, source: &str, mock: Option<&str>) -> serde_json::Value {
    let dir = temp_dir(tag);
    let src = dir.join("source.lute");
    let art = dir.join("artifact.json");
    std::fs::write(&src, source).unwrap();

    let out = Command::new(BIN)
        .args([
            "compile",
            src.to_str().unwrap(),
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

    let mut args = vec!["run".to_string(), art.to_str().unwrap().to_string()];
    if let Some(m) = mock {
        let mp = dir.join("mock.yaml");
        std::fs::write(&mp, m).unwrap();
        args.push("--mock".into());
        args.push(mp.to_str().unwrap().to_string());
    }
    args.push("--json".into());
    let out = Command::new(BIN).args(&args).output().unwrap();
    assert!(
        out.status.success(),
        "run: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

const HDR: &str = "---\nkind: scene\nluteVersion: \"0.8.0\"\ncharacter: hero\nseason: 1\n\
    episode: 1\ntitle: T\n---\n\n## Shot 1.\n\n";

fn kinds(v: &serde_json::Value) -> Vec<&str> {
    v["commands"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["kind"].as_str().unwrap())
        .collect()
}

/// 0.13.0 version negotiation: the gate is MAJOR-only. A minor/patch
/// difference within the implemented major line is compatible-by-default
/// (the aligned releases 0.11.0/0.12.0 moved the minor while changing
/// nothing an engine reads); a MAJOR mismatch still refuses at exit 2.
#[test]
fn run_gates_on_major_only() {
    let dir = temp_dir("gate");
    let src = dir.join("source.lute");
    let art = dir.join("artifact.json");
    std::fs::write(&src, format!("{HDR}@narrator: hi\n")).unwrap();
    let out = Command::new(BIN)
        .args([
            "compile",
            src.to_str().unwrap(),
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

    let mut v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&art).unwrap()).unwrap();

    // Same major, wildly different minor -> accepted.
    v["irVersion"] = serde_json::json!("0.999.7");
    std::fs::write(&art, serde_json::to_string(&v).unwrap()).unwrap();
    let ok = Command::new(BIN)
        .args(["run", art.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        ok.status.success(),
        "same-major minor drift must run: {}",
        String::from_utf8_lossy(&ok.stderr)
    );

    // Different major -> refused, exit 2, message names the MAJOR policy.
    v["irVersion"] = serde_json::json!("1.0.0");
    std::fs::write(&art, serde_json::to_string(&v).unwrap()).unwrap();
    let no = Command::new(BIN)
        .args(["run", art.to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(no.status.code(), Some(2));
    let err = String::from_utf8_lossy(&no.stderr);
    assert!(
        err.contains("unsupported irVersion") && err.contains("MAJOR"),
        "{err}"
    );
}

#[test]
fn end_stops_the_walk_and_surfaces_its_reason() {
    let v = compile_and_run(
        "linear",
        &format!("{HDR}@narrator: seen\n::end{{reason=\"completed\"}}\n@narrator: unreachable\n"),
        None,
    );
    assert_eq!(kinds(&v), ["line", "end"], "{v}");
    assert_eq!(v["commands"][1]["reason"], "completed");
    assert_eq!(
        v["exit"], "complete",
        "an `end` walk is COMPLETE, not incomplete"
    );
}

/// The whole point of the record: identical to falling off the end of the
/// command array — same `exit`, same absence of later records — except that
/// the reason is surfaced.
#[test]
fn ending_matches_falling_off_the_end_except_for_the_reason() {
    let ended = compile_and_run("ended", &format!("{HDR}@narrator: seen\n::end\n"), None);
    let fell_off = compile_and_run("felloff", &format!("{HDR}@narrator: seen\n"), None);
    assert_eq!(ended["exit"], fell_off["exit"]);
    assert_eq!(ended["state"], fell_off["state"]);
    assert_eq!(kinds(&ended), ["line", "end"]);
    assert_eq!(kinds(&fell_off), ["line"]);
    // No `reason` authored -> the key is present but null, never a fabricated value.
    assert!(ended["commands"][1]["reason"].is_null(), "{ended}");
}

/// An `end` inside a hub option body unwinds the BOUNDED segment and the hub
/// loop with it: no further option is presented and the converge never runs.
#[test]
fn end_inside_a_hub_option_terminates_the_whole_walk() {
    let v = compile_and_run(
        "hub",
        &format!(
            "{HDR}<hub id=\"camp\">\n\
             <choice id=\"talk\" label=\"Talk\">\n@hero: A word?\n::end{{reason=\"walked off\"}}\n</choice>\n\
             <choice id=\"go\" label=\"Leave\" exit>\n@hero: Later.\n</choice>\n\
             </hub>\n\
             @narrator: after the hub\n"
        ),
        Some("choose:\n  camp: [talk, go]\n"),
    );
    assert_eq!(kinds(&v), ["hub", "line", "end"], "{v}");
    assert_eq!(v["commands"][2]["reason"], "walked off");
    assert_eq!(v["exit"], "complete");
}

/// Regression guard for the byte-stability contract: a document using none of
/// the 0.8.0 feature must produce a transcript with no `end` record at all.
#[test]
fn a_walk_without_end_is_untouched() {
    let v = compile_and_run("plain", &format!("{HDR}@narrator: a\n@narrator: b\n"), None);
    assert_eq!(kinds(&v), ["line", "line"]);
    assert_eq!(v["exit"], "complete");
}

/// #20 / T8.5's matched pair, which is the acceptance test: the guard-false
/// mock must be refused and the guard-true mock with identical seeds must
/// still play. `lute trace` refuses the identical selection on the same
/// document; `run` played it in full at exit 0.
#[test]
fn run_refuses_a_selection_whose_guard_is_false_and_plays_the_true_one() {
    let dir = temp_dir("run-ineligible");
    let src = dir.join("s.lute");
    let art = dir.join("s.json");
    std::fs::write(
        &src,
        "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n\
         state:\n  run.open: { type: bool, default: false }\n---\n\
         \n## One\n\n<branch id=\"pick\">\n\
         <choice id=\"gated\" label=\"Gated\" when=\"run.open\">\n@narrator: gated.\n</choice>\n\
         <choice id=\"free\" label=\"Free\">\n@narrator: free.\n</choice>\n</branch>\n",
    )
    .unwrap();
    let c = Command::new(BIN)
        .args([
            "compile",
            src.to_str().unwrap(),
            "-o",
            art.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(c.status.success(), "{}", String::from_utf8_lossy(&c.stderr));

    let bad = dir.join("false.yaml");
    std::fs::write(&bad, "state:\n  run.open: false\nchoose:\n  pick: gated\n").unwrap();
    let out = Command::new(BIN)
        .args([
            "run",
            art.to_str().unwrap(),
            "--mock",
            bad.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_ne!(
        out.status.code(),
        Some(0),
        "an ineligible selection must not play: {combined}"
    );
    assert!(combined.contains("E-TRACE-CHOICE"), "{combined}");
    assert!(
        combined.contains("gated"),
        "the refused option must be named: {combined}"
    );
    assert!(
        !combined.contains("narrator: gated."),
        "the arm must not have played: {combined}"
    );

    let good = dir.join("true.yaml");
    std::fs::write(&good, "state:\n  run.open: true\nchoose:\n  pick: gated\n").unwrap();
    let ok = Command::new(BIN)
        .args([
            "run",
            art.to_str().unwrap(),
            "--mock",
            good.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    let oktext = String::from_utf8_lossy(&ok.stdout).to_string();
    assert_eq!(
        ok.status.code(),
        Some(0),
        "the guard-true twin must still play: {oktext}"
    );
    assert!(oktext.contains("gated."), "{oktext}");
}

/// An UNDECIDED guard is not a false guard. `run` computes a real Datalog
/// fixpoint and evaluates CEL over live state, so `Value::Unknown` here means
/// the guard read something with no mock surface — `runner.rs`'s module doc
/// names `now()`/`validAt(...)` — and refusing on that would refuse a legal
/// replay. Only a DECIDED false refuses.
#[test]
fn run_does_not_refuse_a_selection_whose_guard_is_undecided() {
    let dir = temp_dir("run-unknown-guard");
    let src = dir.join("s.lute");
    let art = dir.join("s.json");
    // `now() > 0` is E-TEMPORAL-ARG at check time, so the undecided guard is
    // a `validAt(...)` point-in-time query instead — the other member of the
    // no-mock-surface class the runner's own module doc names.
    std::fs::write(
        &src,
        "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n\
         entities:\n  crew: { members: [vesna] }\n\
         relations:\n  awake: { args: [crew], tier: run }\n---\n\
         \n## One\n\n<branch id=\"pick\">\n\
         <choice id=\"timed\" label=\"Timed\" when=\"validAt(awake(vesna), now())\">\n\
         @narrator: timed.\n</choice>\n\
         <choice id=\"free\" label=\"Free\">\n@narrator: free.\n</choice>\n</branch>\n",
    )
    .unwrap();
    let c = Command::new(BIN)
        .args([
            "compile",
            src.to_str().unwrap(),
            "-o",
            art.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(c.status.success(), "{}", String::from_utf8_lossy(&c.stderr));
    let m = dir.join("m.yaml");
    std::fs::write(&m, "choose:\n  pick: timed\n").unwrap();
    let out = Command::new(BIN)
        .args(["run", art.to_str().unwrap(), "--mock", m.to_str().unwrap()])
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "an undecided guard is not a false one: {text}"
    );
    assert!(text.contains("timed."), "{text}");
}

/// `do_hub` is the SECOND dispatch site the refusal lands on, and it is not
/// covered by the `<branch>` pair above: a hub replays an ordered visit
/// sequence, so the guard is evaluated per visit against live state. Both
/// halves are pinned here — a guard false at the first visit refuses, and the
/// SAME option played after an earlier option's `::set` flipped it plays,
/// which is precisely what a hub is for. Without the second half a refusal
/// hoisted out of the loop would look correct (#20, T8.5, D-C).
#[test]
fn run_refuses_an_ineligible_hub_visit_but_not_one_an_earlier_visit_enabled() {
    let dir = temp_dir("run-ineligible-hub");
    let src = dir.join("h.lute");
    let art = dir.join("h.json");
    std::fs::write(
        &src,
        "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n\
         state:\n  run.open: { type: bool, default: false }\n---\n\
         \n## One\n\n<hub id=\"desk\">\n\
         <choice id=\"unlock\" label=\"Unlock\" once>\n@narrator: unlocked.\n\
         ::set{run.open = true}\n</choice>\n\
         <choice id=\"gated\" label=\"Gated\" when=\"run.open\">\n@narrator: gated.\n</choice>\n\
         <choice id=\"leave\" label=\"Leave\" exit>\n@narrator: leave.\n</choice>\n</hub>\n",
    )
    .unwrap();
    let c = Command::new(BIN)
        .args([
            "compile",
            src.to_str().unwrap(),
            "-o",
            art.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(c.status.success(), "{}", String::from_utf8_lossy(&c.stderr));

    let bad = dir.join("bad.yaml");
    std::fs::write(&bad, "choose:\n  desk: [gated, leave]\n").unwrap();
    let out = Command::new(BIN)
        .args([
            "run",
            art.to_str().unwrap(),
            "--mock",
            bad.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_ne!(
        out.status.code(),
        Some(0),
        "the hub leg must refuse too: {combined}"
    );
    assert!(combined.contains("E-TRACE-CHOICE"), "{combined}");
    assert!(
        combined.contains("desk: gated"),
        "the hub and option must be named: {combined}"
    );
    assert!(
        !combined.contains("narrator: gated."),
        "the arm must not have played: {combined}"
    );

    let good = dir.join("good.yaml");
    std::fs::write(&good, "choose:\n  desk: [unlock, gated, leave]\n").unwrap();
    let ok = Command::new(BIN)
        .args([
            "run",
            art.to_str().unwrap(),
            "--mock",
            good.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    let oktext = String::from_utf8_lossy(&ok.stdout).to_string();
    assert_eq!(
        ok.status.code(),
        Some(0),
        "an option a PRIOR visit enabled is eligible; the guard is re-read per visit: {oktext}"
    );
    assert!(oktext.contains("narrator: gated."), "{oktext}");
}

/// Regression: a `<when is="…">` arm compiles to an EMPTY raw `test` plus a
/// structured `expr` (IR A13). The runner used to evaluate only `test`, so
/// EVERY `is` arm read unknown and the whole match fell through to
/// `<otherwise>` — first observed as six onboarding routes all greeting the
/// player with the otherwise-arm line. The runner must read the compiled
/// `expr` surface.
#[test]
fn match_is_arm_selects_on_compiled_expr_not_raw_test() {
    let dir = temp_dir("match-is");
    let src = dir.join("source.lute");
    let art = dir.join("artifact.json");
    std::fs::write(
        &src,
        "---\nkind: scene\nluteVersion: \"0.10.0\"\ncharacter: hero\nseason: 1\nepisode: 1\ntitle: T\n\
         state:\n  run.who: { type: { enum: [a, b] }, default: a }\n---\n\n## Shot 1.\n\n\
         <match on=\"run.who\">\n<when is=\"a\">\n@narrator: arm-a.\n</when>\n<when is=\"b\">\n@narrator: arm-b.\n</when>\n\
         <otherwise>\n@narrator: fell-through.\n</otherwise>\n</match>\n",
    )
    .unwrap();
    let out = Command::new(BIN)
        .args([
            "compile",
            src.to_str().unwrap(),
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

    let mock = dir.join("mock.yaml");
    std::fs::write(&mock, "state:\n  run.who: b\n").unwrap();
    let out = Command::new(BIN)
        .args([
            "run",
            art.to_str().unwrap(),
            "--mock",
            mock.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    assert_eq!(out.status.code(), Some(0), "{text}");
    assert!(text.contains("arm-b."), "the is=b arm must play: {text}");
    assert!(
        !text.contains("fell-through."),
        "otherwise must not play: {text}"
    );
    assert!(!text.contains("arm-a."), "{text}");

    // Default state (run.who=a) picks arm-a — both directions decided, never otherwise.
    let out = Command::new(BIN)
        .args(["run", art.to_str().unwrap()])
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(
        text.contains("arm-a.") && !text.contains("fell-through."),
        "{text}"
    );
}
