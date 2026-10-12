//! 0.41.0 §3.6: `lute expr <CEL> [--project <dir>] [--json]` lowers a host
//! condition to an IR `exprNode` against a project — every schema the
//! project's documents import, its plugin set, and the project index defs —
//! with checker diagnostics and exit codes 0 / 1 / 2.

use std::path::Path;
use std::process::{Command, Output};

use serde_json::{json, Value};

const BIN: &str = env!("CARGO_BIN_EXE_lute");
const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/expr-041");

fn expr(args: &[&str]) -> Output {
    Command::new(BIN)
        .arg("expr")
        .args(args)
        .args(["--project", FIXTURE])
        .output()
        .unwrap()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn json_report(out: &Output) -> Value {
    let stdout = text(&out.stdout);
    assert_eq!(stdout.lines().count(), 1, "one JSON object on one line: {stdout}");
    serde_json::from_str(&stdout).unwrap()
}

#[test]
fn a_condition_prints_its_expr_node_on_one_line() {
    let out = expr(&["run.gold >= 3 && run.slot != 'night'"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert_eq!(
        text(&out.stdout),
        "{\"op\":\"&&\",\"l\":{\"op\":\">=\",\"l\":{\"path\":\"run.gold\"},\"r\":{\"int\":3}},\
         \"r\":{\"op\":\"!=\",\"l\":{\"path\":\"run.slot\"},\"r\":{\"string\":\"night\"}}}\n"
    );
    assert_eq!(text(&out.stderr), "");
}

#[test]
fn the_json_report_carries_the_cel_slot_a_compiled_beat_carries() {
    let dir = lute_scratch::ScratchDir::named("expr-041-compile");
    let compiled = Command::new(BIN)
        .args(["compile", "--all", FIXTURE, "-o"])
        .arg(&*dir)
        .output()
        .unwrap();
    assert_eq!(compiled.status.code(), Some(0), "{}", text(&compiled.stderr));
    let artifact: Value = serde_json::from_str(
        &std::fs::read_to_string(dir.join("scenes/market.lute.json")).unwrap(),
    )
    .unwrap();
    // `market`'s beat is `when: "@atLeast(3)"`.
    let when = &artifact["meta"]["beat"]["when"];
    assert_eq!(when["authored"], "@atLeast(3)", "{artifact}");

    let out = expr(&["--json", "@atLeast(3)"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let report = json_report(&out);
    assert_eq!(
        report,
        json!({
            "ok": true,
            "cel": when["cel"],
            "authored": when["authored"],
            "expr": when["expr"],
            "diagnostics": [],
        })
    );
    assert_eq!(report["cel"], "(run.gold >= 3)");

    // Without expansion there is no `authored`.
    let report = json_report(&expr(&["--json", "run.gold >= 3"]));
    assert_eq!(
        report,
        json!({
            "ok": true,
            "cel": "run.gold >= 3",
            "expr": {"op": ">=", "l": {"path": "run.gold"}, "r": {"int": 3}},
            "diagnostics": [],
        })
    );
}

#[test]
fn the_vocabulary_is_every_imported_schema_the_plugins_and_the_index_defs() {
    // `run.reputation` and `@respected` come from a schema only the guild
    // scene imports; `@marketOpen` is the plugin's host def.
    let report = json_report(&expr(&["--json", "@respected && @marketOpen && run.reputation < 9"]));
    assert_eq!(report["ok"], true, "{report}");
    assert_eq!(
        report["cel"],
        "(run.reputation > 2) && (run.slot != 'night') && run.reputation < 9"
    );

    // `rich` is one condition in `world.schema.yaml` and another in
    // `guild.schema.yaml`: the index omits it, so `lute expr` does too.
    let out = expr(&["--json", "@rich"]);
    assert_eq!(out.status.code(), Some(1));
    let report = json_report(&out);
    assert_eq!(report["ok"], false);
    assert_eq!(report.as_object().unwrap().len(), 2, "{report}");
    assert_eq!(report["diagnostics"][0]["code"], "E-UNDECLARED-REF", "{report}");
}

#[test]
fn errors_are_checker_diagnostics_in_the_expression_text() {
    let out = expr(&["run.gld >= 3"]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(text(&out.stdout), "");
    assert_eq!(
        text(&out.stderr),
        "<expr>:1:1: error [E-UNDECLARED] state path `run.gld` is not declared in `state:` — did you \
         mean `run.gold`?\n"
    );

    // Columns count from 1 in the expression.
    let out = expr(&["--json", "run.slot == 'nite'"]);
    assert_eq!(out.status.code(), Some(1));
    let report = json_report(&out);
    let diagnostic = &report["diagnostics"][0];
    assert_eq!(diagnostic["code"], "E-WHEN-LITERAL-DOMAIN", "{report}");
    assert_eq!(diagnostic["span"]["line"], 1);
    assert_eq!(diagnostic["span"]["column"], 13);

    for (cel, code) in [
        ("run.gold >=", "E-CEL-PARSE"),
        ("size([1]) > 0", "E-CEL-PROFILE"),
        ("run.gold > 2.5", "E-CEL-TYPE"),
        ("run.gold", "E-REF-TYPE"),
        ("@atLeast()", "E-REF-ARITY"),
        ("holds('knows', ['kai'])", "E-FACT-DOMAIN"),
        ("scene.x > 1", "E-BEAT-ATTR"),
    ] {
        let out = expr(&["--json", cel]);
        assert_eq!(out.status.code(), Some(1), "{cel}");
        let report = json_report(&out);
        assert_eq!(report["ok"], false, "{cel}: {report}");
        assert_eq!(report["diagnostics"][0]["code"], code, "{cel}: {report}");
        assert!(text(&out.stderr).is_empty(), "{cel}: {}", text(&out.stderr));
    }
}

#[test]
fn warnings_go_to_stderr_and_the_condition_still_lowers() {
    let out = expr(&["has(quest.errand.state)"]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(text(&out.stdout), "{\"has\":\"quest.errand.state\"}\n");
    assert!(
        text(&out.stderr).starts_with("<expr>:1:1: warning [W-QUEST-STATE-HAS] "),
        "{}",
        text(&out.stderr)
    );

    let report = json_report(&expr(&["--json", "has(quest.errand.state)"]));
    assert_eq!(report["ok"], true);
    assert_eq!(report["diagnostics"][0]["code"], "W-QUEST-STATE-HAS");
}

#[test]
fn a_project_that_does_not_resolve_reports_in_the_json_report_alone() {
    let dir = lute_scratch::ScratchDir::named("expr-041-unresolved");
    std::fs::write(
        dir.join("lute.project.yaml"),
        "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n",
    )
    .unwrap();
    std::fs::write(dir.join("world.schema.yaml"), "state:\n  run.gold: { type: int, default: 0 }\n")
        .unwrap();
    std::fs::create_dir_all(dir.join("scenes")).unwrap();
    std::fs::write(
        dir.join("scenes/a.lute"),
        "---\nkind: scene\nid: a\nuses: [../world.schema.yaml, ../missing.schema.yaml]\n---\n## A\n@narrator: Hi.\n",
    )
    .unwrap();
    let run = |json: bool| {
        let mut command = Command::new(BIN);
        command.args(["expr", "run.gold > 0", "--project"]).arg(&*dir);
        if json {
            command.arg("--json");
        }
        command.output().unwrap()
    };

    let out = run(true);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(text(&out.stderr), "", "stdout is self-contained");
    let report = json_report(&out);
    assert_eq!(report.as_object().unwrap().len(), 2, "{report}");
    assert_eq!(report["ok"], false);
    let diagnostics = report["diagnostics"].as_array().unwrap();
    assert_eq!(diagnostics.len(), 1, "{report}");
    assert_eq!(diagnostics[0]["code"], "E-USES-NOT-FOUND");
    assert_eq!(diagnostics[0]["severity"], "error");
    assert!(
        diagnostics[0]["message"].as_str().unwrap().contains("`missing.schema.yaml`"),
        "{report}"
    );

    let out = run(false);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(text(&out.stdout), "");
    assert!(
        text(&out.stderr).starts_with(
            "<expr>: error [E-USES-NOT-FOUND] cannot resolve `uses:` import `missing.schema.yaml`"
        ),
        "{}",
        text(&out.stderr)
    );
}

#[test]
fn the_project_defaults_to_the_current_directory_and_must_have_a_manifest() {
    let out = Command::new(BIN)
        .args(["expr", "run.gold > 0"])
        .current_dir(FIXTURE)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert_eq!(text(&out.stdout), "{\"op\":\">\",\"l\":{\"path\":\"run.gold\"},\"r\":{\"int\":0}}\n");

    let scenes = Path::new(FIXTURE).join("scenes");
    for args in [vec!["run.gold > 0"], vec!["--json", "run.gold > 0"]] {
        let out = Command::new(BIN)
            .arg("expr")
            .args(&args)
            .arg("--project")
            .arg(&scenes)
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        assert_eq!(text(&out.stdout), "");
        assert!(text(&out.stderr).contains("no lute.project.yaml"), "{}", text(&out.stderr));
    }
}
