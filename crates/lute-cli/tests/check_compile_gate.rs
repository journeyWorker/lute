//! dsl 0.10.0 §9:962 — *"`lute trace` on a component and `lute check` on the
//! same file stop disagreeing"* (T9.12, issue #23).
//!
//! They disagreed because `check` stopped one pass short of where `trace` and
//! `compile` stop. Both of those run `normalize_document` then
//! `expand_document` AFTER the `check` gate; `check` ran neither, so every
//! `E-COMPILE-*` fault was invisible to it and fatal to them — and `trace`'s
//! refusal line said *"has check error(s) — run `lute check` first"*, advice
//! that could not be followed for a code `check` never computes.
//!
//! This is not a component-only hole. The scene fixtures below carry no
//! component at all and reproduce it exactly. The component case is where it
//! was FILED, and it needs one thing more: a component is not a root document
//! (its `params:` are bound at each `::use`), so `check`'s gate binds the
//! params as a call site would, while `trace`/`compile` refuse the invocation.

use std::path::PathBuf;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "lute-gate-{tag}-{}-{:?}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(args: &[&str]) -> std::process::Output {
    Command::new(BIN).args(args).output().unwrap()
}

/// Every diagnostic line (the `path:line:col: severity [CODE] message` form) a
/// run printed, in order. Comparing these ACROSS commands is the contract:
/// "stop disagreeing" means the same fault, at the same position, worded the
/// same way — not merely "both are red".
fn diag_lines(out: &std::process::Output) -> Vec<String> {
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| l.contains(": error [") || l.contains(": warning ["))
        .map(str::to_string)
        .collect()
}

/// A scene whose two `defs:` bodies reference each other. `check` resolves a
/// `@ref` through `decide()`, whose D3 marker path DECLINES an unexpandable
/// reference rather than erroring — correctly, for its own purposes. The
/// compile expander must not decline: its output has to be `@`-free, so the
/// cycle is `E-COMPILE-EXPAND`. Nothing here is a component.
fn scene_with_a_def_cycle(tag: &str, cycle: bool) -> PathBuf {
    let dir = temp_dir(tag);
    let file = dir.join("s.lute");
    let b = if cycle { "@a + 1" } else { "1" };
    std::fs::write(
        &file,
        format!(
            "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n\
             state:\n  run.n: {{ type: number, default: 0 }}\n\
             defs:\n  a:\n    cel: \"@b + 1\"\n    type: number\n\
             \x20 b:\n    cel: \"{b}\"\n    type: number\n---\n\n\
             ## Shot 1.\n::set{{run.n = @a}}\n@x: hi\n"
        ),
    )
    .unwrap();
    file
}

/// The general defect, on a scene: `check` reported `ok` while `trace` refused
/// over a fault of the same document and sent the author back to `check`.
///
/// The assertion is on EQUALITY of the two diagnostic streams, not on "check is
/// red". An implementation that reddens `check` with some other diagnostic
/// still leaves the author holding two different stories about one file.
#[test]
fn check_and_trace_report_the_same_compile_gate_fault() {
    let file = scene_with_a_def_cycle("cycle", true);
    let path = file.to_str().unwrap();

    let checked = run(&["check", path]);
    let traced = run(&["trace", path]);

    assert_eq!(
        checked.status.code(),
        Some(1),
        "check must not report `ok` for a document trace refuses over; got:\n{}",
        String::from_utf8_lossy(&checked.stdout)
    );
    assert_eq!(traced.status.code(), Some(1), "trace still refuses");
    assert_eq!(
        diag_lines(&checked),
        diag_lines(&traced),
        "the two legs must report the SAME fault at the SAME position — that is \
         what makes `trace`'s \"run `lute check` first\" followable"
    );
    assert!(
        diag_lines(&checked)
            .iter()
            .any(|l| l.contains("E-COMPILE-EXPAND") && l.contains("def expansion cycle: a -> b -> a")),
        "and the fault is the real one; got:\n{:?}",
        diag_lines(&checked)
    );
}

/// The control. Without the cycle the identical fixture is clean in both legs —
/// so the test above is measuring the fault, not a gate that reddens everything
/// it touches.
#[test]
fn a_document_with_no_compile_fault_stays_clean_in_both() {
    let file = scene_with_a_def_cycle("nocycle", false);
    let path = file.to_str().unwrap();

    let checked = run(&["check", path]);
    assert_eq!(
        checked.status.code(),
        Some(0),
        "got:\n{}",
        String::from_utf8_lossy(&checked.stdout)
    );
    assert!(diag_lines(&checked).is_empty(), "got:\n{:?}", diag_lines(&checked));
    assert_eq!(
        run(&["trace", path]).status.code(),
        Some(0),
        "and trace walks it"
    );
}

/// Ordering, which is what makes the two streams byte-identical rather than
/// merely overlapping: both downstream pipelines gate on `check` FIRST and
/// reach `normalize`/`expand` only past it. A document with a check error AND a
/// compile fault must therefore report only the check error, in both legs.
#[test]
fn the_compile_gate_runs_only_past_the_check_gate() {
    let dir = temp_dir("order");
    let file = dir.join("s.lute");
    // `run.n` is never declared (a check error) AND the defs cycle.
    std::fs::write(
        &file,
        "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n\
         defs:\n  a:\n    cel: \"@b + 1\"\n    type: number\n\
         \x20 b:\n    cel: \"@a + 1\"\n    type: number\n---\n\n\
         ## Shot 1.\n::set{run.n = @a}\n@x: hi\n",
    )
    .unwrap();
    let path = file.to_str().unwrap();

    let checked = run(&["check", path]);
    let traced = run(&["trace", path]);
    assert_eq!(diag_lines(&checked), diag_lines(&traced), "same stream");
    assert!(
        diag_lines(&checked).iter().any(|l| l.contains("E-UNDECLARED")),
        "got:\n{:?}",
        diag_lines(&checked)
    );
    assert!(
        !diag_lines(&checked)
            .iter()
            .any(|l| l.contains("E-COMPILE-EXPAND")),
        "the compile gate must not run on a red document — reporting the \
         consequences of errors already printed; got:\n{:?}",
        diag_lines(&checked)
    );
}

/// A component with a declared param used as a `<match>` subject — legal, and
/// the one logic block a component body admits (dsl 0.4.0 §6.2) — and, when
/// `cycle` is set, a genuinely broken `defs:` pair reached from a content-line
/// attribute.
fn component(tag: &str, cycle: bool) -> PathBuf {
    let dir = temp_dir(tag);
    let file = dir.join("c.component.lute");
    let b = if cycle { "\"@a\"" } else { "\"0010\"" };
    std::fs::write(
        &file,
        format!(
            "---\ncomponent: interject\nparams:\n  pressure: string\n\
             defs:\n  a:\n    cel: \"@b\"\n    type: string\n\
             \x20 b:\n    cel: {b}\n    type: string\n---\n\n\
             ## Interjection\n<match on=\"@pressure\">\n<when is=\"rising\">\n\
             @purser{{code=@a}}: The schedule advances.\n</when>\n\
             <otherwise>\n@purser: Allocation is nominal.\n</otherwise>\n</match>\n"
        ),
    )
    .unwrap();
    file
}

/// A component is not a root document: `trace` and `compile` refuse it, and
/// they refuse it for the reason that is TRUE.
///
/// Before, the refusal leaked the expander's internal invariant assertion —
/// `` `@pressure` names no known def body (gate should have caught this) `` —
/// and blamed `check`, which reported the same file `ok`. Both halves are
/// asserted: the assertion must not ship, and the advice must not point at a
/// tool that contradicts it.
#[test]
fn a_component_is_refused_as_a_root_for_the_true_reason() {
    let file = component("root", false);
    let path = file.to_str().unwrap();

    for command in ["trace", "compile"] {
        let out = run(&[command, path]);
        let text = String::from_utf8_lossy(&out.stdout);
        assert_eq!(out.status.code(), Some(1), "{command} refuses; got:\n{text}");
        assert!(
            text.contains("has no standalone compiled form") && text.contains("`::use`"),
            "{command} must name the real reason; got:\n{text}"
        );
        assert!(
            !text.contains("gate should have caught this"),
            "{command} must not ship an internal invariant assertion at an \
             author; got:\n{text}"
        );
        assert!(
            !text.contains("run `lute check` first"),
            "{command} must not send the author to a tool that reports this \
             same file clean — that IS T9.12; got:\n{text}"
        );
    }

    // And the tool it no longer blames agrees: nothing in this component's own
    // body is broken.
    let checked = run(&["check", path]);
    let text = String::from_utf8_lossy(&checked.stdout);
    assert_eq!(checked.status.code(), Some(0), "got:\n{text}");
    assert!(!text.contains("E-COMPILE-"), "got:\n{text}");
}

/// The discriminating case for the component leg of the gate.
///
/// One component carries BOTH shapes: `@pressure`, a declared param that is
/// unbound only because there is no call site, and `a -> b -> a`, a real fault
/// of the body. `check` must report the second and not the first.
///
/// Three implementations are separated here. Skipping the gate for components
/// reports neither. Running it against the component's raw body — the input
/// `trace` uses — reports both, reddening every parameterised component in
/// every corpus over the absence of a caller. Only binding the params as
/// `::use` binds them measures the body.
#[test]
fn a_components_own_compile_fault_is_reported_but_its_unbound_params_are_not() {
    let broken = run(&["check", component("broken", true).to_str().unwrap()]);
    let lines = diag_lines(&broken);
    assert_eq!(
        broken.status.code(),
        Some(1),
        "a real compile fault in a component body gates; got:\n{lines:?}"
    );
    assert!(
        lines
            .iter()
            .any(|l| l.contains("E-COMPILE-EXPAND") && l.contains("def expansion cycle")),
        "the body's own fault is reported; got:\n{lines:?}"
    );
    assert!(
        !lines.iter().any(|l| l.contains("names no known def body")),
        "`@pressure` is a DECLARED param, unbound only because a standalone \
         check has no call site — that is not a fault of the component; \
         got:\n{lines:?}"
    );

    // The other direction: the same component without the cycle is clean, so
    // the assertion above is not passing on an implementation that reports
    // nothing at all.
    let clean = run(&["check", component("clean", false).to_str().unwrap()]);
    assert_eq!(
        clean.status.code(),
        Some(0),
        "got:\n{:?}",
        diag_lines(&clean)
    );
}
