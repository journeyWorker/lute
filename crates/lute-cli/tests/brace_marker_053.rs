//! dsl 0.53.0 §8 (#229): a `{{…}}` marker that opens on a literal brace run.
//!
//! `{{{run.a}}` is a literal `{` and the marker `{{run.a}}` to its author,
//! but the first `{{` opens the marker, whose expression is then `{run.a`.
//! In line text and in every label-grammar value (choice labels, prompts,
//! directive `text` attributes, component bodies) it is one `E-INTERP-BRACE`
//! naming the marker meant — the whole leading run stripped — and the two
//! spellings of its n braces. On v0.52.0 each is `E-CEL-PROFILE`
//! (``interpolation `{run.a` is not a valid `{{…}}` form``). Both suggested
//! spellings, substituted into the same line or label, check clean and
//! render the braces and the value: through `lute run` for a line and in the
//! `awaitChoice` menu `lute play --events` prints for a label and a prompt.

use std::path::Path;
use std::process::{Command, Output};

use serde_json::Value;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

const PROJECT: &str = "pluginsDir: plugins/\ndefaultProfile: game\nprofiles:\n  game:\n    \
                       plugins:\n      acme.ui: true\ndefaults:\n  contentLang: en\n";

const PLUGIN: &str = "id: acme.ui\nversion: 0.1.0\nkind: capability\ndepends:\n  - { id: \
                      lute.core, range: \"^0.0.1\" }\nexports:\n  directives: directives/\n  \
                      occasions: occasions/\n";

const DIRECTIVES: &str = "directives:\n  - name: banner\n    attrs:\n      - { name: title, \
                          required: true, type: text }\n";

const OCCASIONS: &str = "occasions:\n  visit: { select: first }\n";

/// A component whose body line and `text` attribute open a marker on a
/// literal brace.
const COMPONENT: &str = "---\ncomponent: c\nparams:\n  p: string\n---\n\n## C\n\n\
                         @narrator: S {{{@p}}\n::banner{title=\"T {{{{@p}}\"}\n";

fn write(dir: &Path, rel: &str, text: &str) {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, text).unwrap();
}

/// A project with scene `s` (answering `visit`; `run.a` is 3, `run.lbrace`
/// is `{`) whose body is `body`.
fn project(tag: &str, body: &str) -> lute_scratch::ScratchDir {
    project_with(
        tag,
        &format!(
            "---\nkind: scene\nid: s\non: visit\nstate:\n  run.a: {{ type: int, default: 3 }}\n  \
             run.lbrace: {{ type: string, default: \"{{\" }}\n---\n\n## S\n\n{body}\n"
        ),
    )
}

/// The project above with `scene` as the whole of `s.lute`.
fn project_with(tag: &str, scene: &str) -> lute_scratch::ScratchDir {
    let dir = lute_scratch::ScratchDir::named(tag);
    write(&dir, "lute.project.yaml", PROJECT);
    write(&dir, "plugins/acme.ui/plugin.yaml", PLUGIN);
    write(&dir, "plugins/acme.ui/directives/d.yaml", DIRECTIVES);
    write(&dir, "plugins/acme.ui/occasions/o.yaml", OCCASIONS);
    write(&dir, "s.lute", scene);
    dir
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

/// The `E-INTERP-BRACE` message for `n` braces before `{{run.a}}`, as §8
/// spells it.
fn message(n: usize) -> &'static str {
    match n {
        1 => {
            "a literal `{` directly before `{{run.a}}` cannot be told apart from the marker in \
             compiled text (lute 0.47.1 §3): put a character between them (`{ {{run.a}}`), or \
             show the brace from a string state path (`{{run.lbrace}}{{run.a}}`, with \
             `run.lbrace` declared `{ type: string, default: \"{\" }`)"
        }
        2 => {
            "a literal `{{` directly before `{{run.a}}` cannot be told apart from the marker in \
             compiled text (lute 0.47.1 §3): put a character between them (`\\{{ {{run.a}}`), or \
             show each brace from a string state path \
             (`{{run.lbrace}}{{run.lbrace}}{{run.a}}`, with `run.lbrace` declared `{ type: \
             string, default: \"{\" }`)"
        }
        _ => unreachable!(),
    }
}

/// The issue's repro: one `E-INTERP-BRACE` per marker, for one and two
/// braces, in a line, a prompt, a choice label and a directive `text`
/// attribute. No `E-CEL-PROFILE`.
#[test]
fn issue_229_a_marker_opened_on_literal_braces_is_e_interp_brace() {
    let dir = project(
        "brace-229",
        "@mira: Score {{{run.a}}\n@mira: Score {{{{run.a}}\n\
         <branch id=\"b\" prompt=\"P {{{run.a}}\">\n<choice id=\"c\" text=\"Score {{{run.a}}\">\n\
         @mira: ok\n</choice>\n</branch>\n\
         <branch id=\"b2\" prompt=\"P {{{{run.a}}\">\n<choice id=\"d\" text=\"Score {{{{run.a}}\">\n\
         @mira: ok\n</choice>\n</branch>\n::banner{title=\"T {{{run.a}}\"}",
    );
    let (one, two) = (message(1), message(2));
    assert_eq!(
        check(&dir),
        [
            format!("./s.lute:12:14: error [E-INTERP-BRACE] {one}"),
            format!("./s.lute:13:14: error [E-INTERP-BRACE] {two}"),
            format!("./s.lute:14:24: error [E-INTERP-BRACE] {one}"),
            format!("./s.lute:15:1: error [E-INTERP-BRACE] {one}"),
            format!("./s.lute:19:25: error [E-INTERP-BRACE] {two}"),
            format!("./s.lute:20:1: error [E-INTERP-BRACE] {two}"),
            format!("./s.lute:24:17: error [E-INTERP-BRACE] {one}"),
        ]
    );
}

/// The marker meant keeps everything after the run as written — spacing and
/// a format hint — and a longer run is named in full.
#[test]
fn the_whole_leading_run_is_stripped_and_the_rest_kept() {
    let dir = project(
        "brace-229-run",
        "@mira: A {{{{{ run.a:ordinal }}\n@mira: B {{ {run.a}}",
    );
    let found = check(&dir);
    assert_eq!(found.len(), 2, "{found:#?}");
    assert_eq!(
        found[0],
        "./s.lute:12:10: error [E-INTERP-BRACE] a literal `{{{` directly before \
         `{{ run.a:ordinal }}` cannot be told apart from the marker in compiled text (lute \
         0.47.1 §3): put a character between them (`\\{{{ {{ run.a:ordinal }}`), or show each \
         brace from a string state path \
         (`{{run.lbrace}}{{run.lbrace}}{{run.lbrace}}{{ run.a:ordinal }}`, with `run.lbrace` \
         declared `{ type: string, default: \"{\" }`)"
    );
    // A space after `{{` is an opened marker: no brace opened it.
    assert!(
        found[1].starts_with("./s.lute:13:10: error [E-CEL-PROFILE] interpolation `{run.a`"),
        "{found:#?}"
    );
}

/// A component body line and `text` attribute are judged the same way (on
/// v0.52.0 the line is `E-COMPONENT-STATE`, the attribute `E-CEL-PROFILE`).
#[test]
fn a_component_body_reports_the_brace_too() {
    let dir = project_with(
        "brace-229-component",
        "---\nkind: scene\nid: s\ncomponents: [c.component.lute]\n---\n\n## S\n\n\
         ::use{component=\"c\" instance=\"i\" p=\"x\"}\n",
    );
    write(&dir, "c.component.lute", COMPONENT);
    let found = check(&dir);
    assert_eq!(found.len(), 2, "{found:#?}");
    assert!(
        found[0].contains("[E-INTERP-BRACE]")
            && found[0].contains("a literal `{` directly before `{{@p}}`"),
        "{found:#?}"
    );
    assert!(
        found[1].contains("[E-INTERP-BRACE]")
            && found[1].contains("a literal `{{` directly before `{{@p}}`"),
        "{found:#?}"
    );
}

/// The awaiting menu `lute play --events` prints for the unscripted branch.
fn played_menu(dir: &Path) -> Value {
    write(dir, "script.play.yaml", "steps:\n  - occasion: visit\n");
    let out = lute(
        dir,
        &[
            "play",
            "--project",
            ".",
            "--script",
            "script.play.yaml",
            "--events",
        ],
    );
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .find_map(|v| {
            let menu = &v["output"]["await"]["menu"];
            (!menu.is_null()).then(|| menu.clone())
        })
        .unwrap_or_else(|| panic!("an awaitChoice menu: {}", text(&out)))
}

/// The `mira:` lines `lute run` prints for the compiled `s.lute` (it stops
/// at the unscripted branch).
fn rendered_lines(dir: &Path) -> Vec<String> {
    let out_path = dir.join("s.json");
    let compiled = lute(
        dir,
        &[
            "compile",
            "s.lute",
            "--project",
            ".",
            "-o",
            out_path.to_str().unwrap(),
        ],
    );
    assert!(compiled.status.success(), "{}", text(&compiled));
    let out = lute(dir, &["run", "s.json"]);
    text(&out)
        .lines()
        .filter_map(|l| l.split_once("mira: ").map(|(_, t)| t.to_string()))
        .collect()
}

/// Both spellings of each message, substituted into a line, a prompt and a
/// choice label: clean, and rendered as the braces then the value.
#[test]
fn both_suggested_spellings_check_clean_and_render() {
    for (i, (spelling, shown)) in [
        ("{ {{run.a}}", "{ 3"),
        ("{{run.lbrace}}{{run.a}}", "{3"),
        ("\\{{ {{run.a}}", "{{ 3"),
        ("{{run.lbrace}}{{run.lbrace}}{{run.a}}", "{{3"),
        // The #139 parser site's run of three (0.53.0 §8: the same message).
        ("\\{{{ {{run.a}}", "{{{ 3"),
        (
            "{{run.lbrace}}{{run.lbrace}}{{run.lbrace}}{{run.a}}",
            "{{{3",
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let dir = project(
            &format!("brace-229-spelling-{i}"),
            &format!(
                "@mira: Score {spelling}\n<branch id=\"b\" prompt=\"P {spelling}\">\n\
                 <choice id=\"c\" text=\"Score {spelling}\">\n@mira: ok\n</choice>\n\
                 <choice id=\"d\" text=\"Keep\">\n@mira: ok\n</choice>\n</branch>"
            ),
        );
        assert_eq!(check(&dir), Vec::<String>::new(), "{spelling}");
        assert_eq!(
            rendered_lines(&dir),
            [format!("Score {shown}")],
            "{spelling}"
        );
        let menu = played_menu(&dir);
        assert_eq!(menu["prompt"], format!("P {shown}"), "{spelling}: {menu:#}");
        assert_eq!(
            menu["options"][0]["text"],
            format!("Score {shown}"),
            "{spelling}: {menu:#}"
        );
    }
}
