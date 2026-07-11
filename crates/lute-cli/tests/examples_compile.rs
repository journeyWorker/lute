// crates/lute-cli/tests/examples_compile.rs
// Mirrors the harness in crates/lute-cli/tests/examples_check.rs (assert_cmd
// style), but drives `lute compile --json` instead of `lute check`.
//
// Regression guard for the silent-dialogue-loss bug fixed in 27653b6: writing
// a speaker line INLINE on the same source line as a `<when test="…">` or
// `<otherwise>` block-open tag (e.g. `<when test="X">@sofia: text`) parses
// fine under tree-sitter, but the line-based Rust compiler silently DROPPED
// the trailing content — the match arm lowered to a bare `jump` with no
// preceding `line` command, and no diagnostic was raised. `examples_check.rs`
// never caught this because it only runs `check` (never inspects compiled
// commands) and only touches two of the four examples that had the bug.
//
// Each test below compiles one of the four previously-affected example files
// and asserts BOTH the count and the exact ordered (speaker, text) pairs of
// every emitted `line` command. If an arm's dialogue line were dropped again,
// the affected arm would compile to a bare `jump` and the corresponding
// (speaker, text) pair would vanish from this list — reddening the test.

use std::process::Command;

fn compile(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_lute"))
        .arg("compile")
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap()
}

/// Run `lute compile <args> --json`, assert exit success, and return the
/// ordered (speaker, text) pairs of every `line` command in the artifact's
/// `commands` array.
fn compiled_line_pairs(args: &[&str]) -> Vec<(String, String)> {
    let mut full_args: Vec<&str> = args.to_vec();
    full_args.push("--json");
    let out = compile(&full_args);
    assert!(
        out.status.success(),
        "compile {:?} failed (status {:?}); stdout: {}\nstderr: {}",
        args,
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let artifact: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("compiled artifact is valid JSON");
    artifact["commands"]
        .as_array()
        .expect("artifact has a `commands` array")
        .iter()
        .filter(|c| c["kind"] == "line")
        .map(|c| {
            let speaker = c["speaker"]
                .as_str()
                .expect("`line` command has a string `speaker`")
                .to_string();
            let text = c["text"]
                .as_str()
                .expect("`line` command has a string `text`")
                .to_string();
            (speaker, text)
        })
        .collect()
}

fn owned_pairs(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(s, t)| (s.to_string(), t.to_string()))
        .collect()
}

// --- carry-ep.lute: `<when test="run.choseHelp == true">`/`<otherwise>` arms
// each carry a `@sofia:` line. Before 27653b6 both arms' dialogue was written
// inline on the block-open tag line and silently dropped; now they're on
// their own body lines and must survive compilation intact.

#[test]
fn carry_ep_example_compiles_with_all_when_otherwise_arm_lines_present() {
    let pairs = compiled_line_pairs(&["../../docs/examples/carry-ep.lute"]);
    assert_eq!(
        pairs,
        owned_pairs(&[
            ("narrator", "Previously, a choice was made."),
            ("sofia", "Thanks for helping me back then."),
            ("sofia", "..."),
        ]),
        "carry-ep.lute must compile all 3 dialogue lines, including both \
         <when>/<otherwise> arm lines — a dropped arm line means silent \
         dialogue loss has regressed"
    );
}

// --- extends-demo.lute: `<when test="run.blessed">`/`<otherwise>` arms each
// carry a `@sofia:` line, under a `--project` resolution (declaration
// inheritance via `extends:`).

#[test]
fn extends_demo_example_compiles_with_all_when_otherwise_arm_lines_present() {
    let pairs = compiled_line_pairs(&[
        "../../docs/examples/extends-demo.lute",
        "--project",
        "../../docs/examples",
    ]);
    assert_eq!(
        pairs,
        owned_pairs(&[
            ("sofia", "Fortune smiles on me today."),
            ("sofia", "Still waiting on my luck to turn."),
        ]),
        "extends-demo.lute must compile both <when>/<otherwise> arm lines — a \
         dropped arm line means silent dialogue loss has regressed"
    );
}

// --- param-def.lute: `<when test="scene.score >= 1">`/`<otherwise>` arms
// each carry a `@narrator:` line, exercising a plugin-declared parameter
// (`param-def.schema.yaml`-style) condition.

#[test]
fn param_def_example_compiles_with_all_when_otherwise_arm_lines_present() {
    let pairs = compiled_line_pairs(&["../../docs/examples/param-def.lute"]);
    assert_eq!(
        pairs,
        owned_pairs(&[("narrator", "high enough"), ("narrator", "not yet")]),
        "param-def.lute must compile both <when>/<otherwise> arm lines — a \
         dropped arm line means silent dialogue loss has regressed"
    );
}

// --- plugin-def.lute: `<when test="true">`/`<otherwise>` arms each carry a
// `@narrator:` line, under a `--project` resolving a plugin definition
// (`plugindef-project`).

#[test]
fn plugin_def_example_compiles_with_all_when_otherwise_arm_lines_present() {
    let pairs = compiled_line_pairs(&[
        "../../docs/examples/plugindef-project/plugin-def.lute",
        "--project",
        "../../docs/examples/plugindef-project",
    ]);
    assert_eq!(
        pairs,
        owned_pairs(&[("narrator", "warm path"), ("narrator", "cold path")]),
        "plugin-def.lute must compile both <when>/<otherwise> arm lines — a \
         dropped arm line means silent dialogue loss has regressed"
    );
}
