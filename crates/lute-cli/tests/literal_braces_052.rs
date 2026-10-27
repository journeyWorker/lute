//! 0.52.0 §3 (#139, #211): literal braces in text.
//!
//! - §3.1: a `{{…}}` marker in line text whose preceding decoded literal text
//!   ends in an odd run of `{` is `E-INTERP-BRACE` — in a line, in a
//!   translation (`E-L10N-MODIFIERS`) and in a component line a `::use`
//!   argument splices (reported at the `::use`). On v0.51.0 each checks clean
//!   and renders the marker one brace early (`S }}{{3} E`).
//! - §3.2: `\{{` is a literal `{{` in a value that takes the label grammar
//!   and `E-STRING-ESCAPE` in any other quoted value. On v0.51.0 every `\{{`
//!   in a quoted value is `E-STRING-ESCAPE`.

use std::path::Path;
use std::process::{Command, Output};

use serde_json::Value;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

const PROJECT: &str = "pluginsDir: plugins/\ndefaultProfile: game\nprofiles:\n  game:\n    \
                       plugins:\n      acme.ui: true\ndefaults:\n  contentLang: en\n";

const PLUGIN: &str = "id: acme.ui\nversion: 0.1.0\nkind: capability\ndepends:\n  - { id: \
                      lute.core, range: \"^0.0.1\" }\nexports:\n  directives: directives/\n";

const DIRECTIVES: &str = "directives:\n  - name: banner\n    attrs:\n      - { name: title, \
                          required: true, type: text }\n      - { name: note, type: string }\n";

const COMPONENT: &str = "---\ncomponent: c\nparams:\n  p: string\n  q: string\n---\n\n## C\n\n\
                         @narrator: S {{@p}}{{@q}} E\n";

/// The outer component of a nested `::use`: it passes its `p` on.
const OUTER: &str = "---\ncomponent: outer\nparams:\n  p: string\ncomponents: [c.component.lute]\n\
                     ---\n\n## O\n\n::use{component=\"c\" instance=\"in\" p=@p q=\"{{run.a}}\"}\n";

const BRACE_MESSAGE: &str = "a literal `{` directly before `{{run.a}}` cannot be told apart from \
                             the marker in compiled text (lute 0.47.1 §3): put a character between \
                             them (`{ {{run.a}}`), or show the brace from a string state path \
                             (`{{run.lbrace}}{{run.a}}`, with `run.lbrace` declared `{ type: \
                             string, default: \"{\" }`)";

fn write(dir: &Path, rel: &str, text: &str) {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, text).unwrap();
}

/// The plugin and components above plus `files`.
fn project(tag: &str, files: &[(&str, &str)]) -> lute_scratch::ScratchDir {
    let dir = lute_scratch::ScratchDir::named(tag);
    write(&dir, "lute.project.yaml", PROJECT);
    write(&dir, "plugins/acme.ui/plugin.yaml", PLUGIN);
    write(&dir, "plugins/acme.ui/directives/d.yaml", DIRECTIVES);
    write(&dir, "c.component.lute", COMPONENT);
    write(&dir, "outer.component.lute", OUTER);
    for (rel, text) in files {
        write(&dir, rel, text);
    }
    dir
}

/// Scene `s` with `run.a` (3) and `run.x` (1) declared, and `body`.
fn scene(body: &str) -> String {
    format!(
        "---\nkind: scene\nid: s\ncomponents: [c.component.lute, outer.component.lute]\nstate:\n  \
         run.a: {{ type: int, default: 3 }}\n  run.x: {{ type: int, default: 1 }}\n  run.lbrace: \
         {{ type: string, default: \"{{\" }}\n---\n\n## S\n\n{body}\n"
    )
}

fn lute(dir: &Path, args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .current_dir(dir)
        .env("LUTE_TELEMETRY", "0")
        .output()
        .unwrap()
}

fn text(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// Every diagnostic line of `lute check --project .`.
fn check(dir: &Path) -> Vec<String> {
    let out = lute(dir, &["check", "--project", "."]);
    text(&out)
        .lines()
        .filter(|l| l.contains(" [") && !l.starts_with("ok:") && !l.starts_with("failed:"))
        .map(str::to_string)
        .collect()
}

/// Compile `s.lute` (check-clean) with `extra` arguments.
fn compile(dir: &Path, extra: &[&str]) -> Output {
    let out_path = dir.join("s.json");
    let mut args = vec!["compile", "s.lute", "--project", ".", "-o"];
    args.push(out_path.to_str().unwrap());
    args.extend_from_slice(extra);
    lute(dir, &args)
}

fn commands(dir: &Path) -> Vec<Value> {
    let out = compile(dir, &[]);
    assert!(out.status.success(), "{}", text(&out));
    let artifact: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("s.json")).unwrap()).unwrap();
    artifact["commands"].as_array().unwrap().clone()
}

/// The `narrator:` lines `lute run` prints for the compiled `s.lute`.
fn rendered(dir: &Path) -> Vec<String> {
    commands(dir);
    let out = lute(dir, &["run", "s.json"]);
    assert!(out.status.success(), "{}", text(&out));
    text(&out)
        .lines()
        .filter_map(|l| l.split_once("narrator: ").map(|(_, t)| t.to_string()))
        .collect()
}

#[test]
fn issue_139_a_literal_brace_before_a_marker_is_e_interp_brace() {
    let dir = project(
        "brace-139",
        &[("s.lute", &scene("@narrator: S }}{\\{{{{run.a}}} E"))],
    );
    assert_eq!(
        check(&dir),
        [format!(
            "./s.lute:13:20: error [E-INTERP-BRACE] {BRACE_MESSAGE}"
        )]
    );
}

#[test]
fn a_modifier_boundary_does_not_end_the_run() {
    let dir = project(
        "brace-modifier",
        &[("s.lute", &scene("@narrator: x{:pause{s=0.5}{{run.a}}"))],
    );
    assert_eq!(
        check(&dir),
        [format!(
            "./s.lute:13:27: error [E-INTERP-BRACE] {BRACE_MESSAGE}"
        )]
    );
}

#[test]
fn an_even_run_and_the_two_suggested_spellings_check_clean_and_render() {
    let dir = project(
        "brace-clean",
        &[(
            "s.lute",
            &scene(
                "@narrator: S \\{{{{run.a}} E\n@narrator: S { {{run.a}} E\n\
                 @narrator: S {{run.lbrace}}{{run.a}} E",
            ),
        )],
    );
    assert_eq!(check(&dir), Vec::<String>::new());
    assert_eq!(rendered(&dir), ["S {{3 E", "S { 3 E", "S {3 E"]);
}

#[test]
fn a_translation_with_the_sequence_is_not_merged() {
    let dir = project(
        "brace-l10n",
        &[("s.lute", &scene("@narrator{code=\"0010\"}: S {{run.a}} E"))],
    );
    assert_eq!(check(&dir), Vec::<String>::new());
    let bundle = serde_json::json!({
        "schemaVersion": 1,
        "locales": ["ko"],
        "entries": { "s.narrator_0010": { "ko": "S }}{\\{{{{run.a}}} E" } }
    });
    write(&dir, "b.json", &bundle.to_string());
    let out = compile(&dir, &["--locale-bundle", "b.json"]);
    assert_eq!(out.status.code(), Some(1), "{}", text(&out));
    assert!(
        text(&out).contains(&format!(
            "[E-L10N-MODIFIERS] `ko` text for `s.narrator_0010` is not merged: its inline markup \
             does not parse (E-INTERP-BRACE: {BRACE_MESSAGE})"
        )),
        "{}",
        text(&out)
    );
}

#[test]
fn a_spliced_argument_is_reported_at_the_use() {
    let dir = project(
        "brace-use",
        &[(
            "s.lute",
            &scene(
                "::use{component=\"c\" instance=\"a\" p=\"{\" q=\"{{run.a}}\"}\n\
                 ::use{component=\"c\" instance=\"b\" p=\"{ \" q=\"{{run.a}}\"}\n\
                 ::use{component=\"c\" instance=\"c\" p=\"{\\\\{{\" q=\"{{run.a}}\"}\n\
                 ::use{component=\"outer\" instance=\"d\" p=\"x{\"}",
            ),
        )],
    );
    assert_eq!(
        check(&dir),
        [
            format!("./s.lute:13:1: error [E-INTERP-BRACE] {BRACE_MESSAGE}"),
            format!("./s.lute:15:1: error [E-INTERP-BRACE] {BRACE_MESSAGE}"),
            format!("./s.lute:16:1: error [E-INTERP-BRACE] {BRACE_MESSAGE}"),
        ]
    );
}

/// #221: a plain `{{` at the end of a `::use` argument swallowed the marker
/// spliced after it (v0.51.0 checks clean, compiles `S \{{\{{run.a}} E` and
/// renders `S {{{{run.a}} E`). It is `E-INTERP-BRACE` at the `::use`; the
/// 0.47.1 spelling `\\{{` checks clean and renders a literal `{{`.
#[test]
fn issue_221_a_plain_opener_spliced_before_a_marker_is_refused() {
    let dir = project(
        "brace-221",
        &[(
            "s.lute",
            &scene("::use{component=\"c\" instance=\"c\" p=\"{{\" q=\"{{run.a}}\"}"),
        )],
    );
    assert_eq!(
        check(&dir),
        ["./s.lute:13:1: error [E-INTERP-BRACE] a `::use` argument ending in a plain `{{` merges \
          with the `{{run.a}}` spliced after it, and the marker is lost: pass a literal `{{` in an \
          argument as `\\\\{{` (lute 0.47.1)"]
    );
    let dir = project(
        "brace-221-spelling",
        &[(
            "s.lute",
            &scene("::use{component=\"c\" instance=\"c\" p=\"\\\\{{\" q=\"{{run.a}}\"}"),
        )],
    );
    assert_eq!(check(&dir), Vec::<String>::new());
    assert_eq!(rendered(&dir), ["S {{3 E"]);
}

/// The `options` of the first `choice` record.
fn first_options(cmds: &[Value]) -> Vec<Value> {
    cmds.iter()
        .find(|c| c["kind"] == "choice")
        .and_then(|c| c["options"].as_array())
        .cloned()
        .unwrap()
}

#[test]
fn issue_211_an_escaped_opener_in_a_label_is_a_literal() {
    // The issue's repro.
    let dir = project(
        "brace-211",
        &[(
            "s.lute",
            &scene(
                "<branch id=\"b\" prompt=\"Pick \\{{one}}\">\n<choice id=\"c\" text=\"a \\{{x}}\">\n\
                 @mira: ok\n</choice>\n<choice id=\"d\" text=\"b \\\\{{run.x}}\">\n@mira: ok\n\
                 </choice>\n</branch>\n::banner{title=\"t \\{{y}}\"}",
            ),
        )],
    );
    assert_eq!(check(&dir), Vec::<String>::new());
    let cmds = commands(&dir);
    let options = first_options(&cmds);
    assert_eq!(options[0]["text"], "a \\{{x}}");
    assert!(options[0].get("placeholders").is_none(), "{options:?}");
    // An even run: a backslash, then a live marker.
    assert_eq!(options[1]["text"], "b \\\\{{run.x}}");
    assert_eq!(
        options[1]["placeholders"],
        serde_json::json!([{"kind": "path", "path": "run.x"}])
    );
    let choice = cmds.iter().find(|c| c["kind"] == "choice").unwrap();
    assert_eq!(choice["prompt"], "Pick \\{{one}}");
    let banner = cmds.iter().find(|c| c["kind"] == "plugin").unwrap();
    assert_eq!(banner["fields"]["title"], "t \\{{y}}");
}

#[test]
fn an_escaped_opener_outside_the_label_grammar_is_e_string_escape() {
    let generic = "`\\{{` (a literal `{{`) is defined only in line text and in the values that \
                   take the label grammar — `<choice text>`, `prompt` and a plugin `text` \
                   attribute;";
    let dir = project(
        "brace-escape",
        &[(
            "s.lute",
            &scene(
                "::banner{title=\"t\" note=\"n \\{{y}}\"}\n<branch id=\"b\">\n<choice id=\"c\" \
                 text=\"c\" when=\"run.x == 1 \\{{\">\n@mira: ok\n</choice>\n</branch>\n\
                 ::use{component=\"c\" instance=\"a\" p=\"\\{{\" q=\"x\"}\n::bg{location=\"q\\{r\"}",
            ),
        )],
    );
    let diags = check(&dir);
    let escapes: Vec<&String> = diags
        .iter()
        .filter(|d| d.contains("[E-STRING-ESCAPE]"))
        .collect();
    assert_eq!(
        escapes,
        [
            &format!("./s.lute:13:26: error [E-STRING-ESCAPE] {generic} `note` of `::banner` is not one"),
            &format!("./s.lute:15:31: error [E-STRING-ESCAPE] {generic} `when` of `<choice>` is not one"),
            &format!(
                "./s.lute:19:37: error [E-STRING-ESCAPE] {generic} `p` of `::use` is not one; a \
                 `::use` argument passes a literal `{{{{` as `\\\\{{{{` (lute 0.47.1)"
            ),
            &"./s.lute:20:17: error [E-STRING-ESCAPE] only \\\" \\\\ \\n \\t are defined escapes, and \
              `\\{{` for a literal `{{` in a label"
                .to_string(),
        ],
        "{diags:#?}"
    );
}

/// Outside the label grammar a value has no markers: `\{{` inside a
/// `{{…}}` of a plain string is `E-STRING-ESCAPE` too (as v0.51.0's lexer
/// refused it), not shielded by the `{{`.
#[test]
fn an_escaped_opener_inside_braces_of_a_plain_value_is_refused() {
    let dir = project(
        "brace-escape-inner",
        &[(
            "s.lute",
            &scene("::banner{title=\"t\" note=\"{{ \\{{ }}\"}"),
        )],
    );
    let diags = check(&dir);
    assert!(
        diags
            .iter()
            .any(|d| d.starts_with("./s.lute:13:26: error [E-STRING-ESCAPE]")
                && d.ends_with("`note` of `::banner` is not one")),
        "{diags:#?}"
    );
}

/// An imported component body is judged at its `::use` host too, under the
/// host's snapshot: checking only the host still refuses `\{{` in the body's
/// plain value and in a nested `::use` argument (v0.51.0's lexer refused
/// both when the import parsed), and keeps it in the body's `text` attr.
#[test]
fn an_escaped_opener_in_an_imported_body_is_refused_at_the_host() {
    let dir = project(
        "brace-escape-import",
        &[
            (
                "e.component.lute",
                "---\ncomponent: e\ncomponents: [c.component.lute]\n---\n\n## E\n\n\
                 ::banner{title=\"t \\{{ok}}\" note=\"\\{{literal}}\"}\n\
                 ::use{component=\"c\" instance=\"n\" p=\"\\{{\" q=\"x\"}\n",
            ),
            (
                "s.lute",
                "---\nkind: scene\nid: s\ncomponents: [e.component.lute]\n---\n\n## S\n\n\
                 ::use{component=\"e\" instance=\"a\"}\n",
            ),
        ],
    );
    let out = lute(&dir, &["check", "s.lute", "--project", "."]);
    let report = text(&out);
    assert_eq!(out.status.code(), Some(1), "{report}");
    // Anchored at the `::use`, each with the body's own position related.
    let escapes: Vec<&str> = report
        .lines()
        .filter(|l| l.contains("[E-STRING-ESCAPE]"))
        .collect();
    assert_eq!(escapes.len(), 4, "{report}");
    assert!(
        escapes[0]
            .starts_with("s.lute:9:1: error [E-STRING-ESCAPE] component `e` (e.component.lute):")
            && escapes[0].ends_with("`note` of `::banner` is not one"),
        "{report}"
    );
    assert!(
        escapes[1]
            .trim_start()
            .starts_with("e.component.lute:8:34: error [E-STRING-ESCAPE]"),
        "{report}"
    );
    assert!(
        escapes[2].starts_with("s.lute:9:1: error [E-STRING-ESCAPE] component `e`")
            && escapes[2].contains("`p` of `::use` is not one; a `::use` argument passes"),
        "{report}"
    );
    assert!(
        escapes[3]
            .trim_start()
            .starts_with("e.component.lute:9:37: error [E-STRING-ESCAPE]"),
        "{report}"
    );
}
