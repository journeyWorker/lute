//! 0.50.0 §4 (#162): host interpolation tokens. A plugin's `hostTokens`
//! export declares the bare `{{name}}` markers the engine substitutes;
//! `lute.core` declares `userName`. A declared token compiles to a
//! `reserved` placeholder and takes the format hints of its type; an
//! undeclared bare name is `E-CEL-PROFILE` naming the declared tokens.
//!
//! On 0.49.1 `hostTokens` is not an export kind (`E-PLUGIN-UNKNOWN-EXPORT`,
//! the plugin fails to load) and `{{playerName}}` is `E-CEL-PROFILE`, so the
//! repro and every test that declares a token fail there.

use std::path::Path;
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_lute");

const PROJECT: &str = "pluginsDir: plugins/\ndefaultProfile: game\nprofiles:\n  game:\n    \
                       plugins:\n      acme.stage: true\n";

const PLUGIN: &str = "id: acme.stage\nversion: 0.1.0\nkind: capability\ndepends:\n  - { id: \
                      lute.core, range: \"^0.0.1\" }\nexports:\n  hostTokens: hosttokens/\n";

const TOKENS: &str = "hostTokens:\n  playerName: { type: string }\n  rank:       { type: int }\n  \
                      platform:   { type: { enum: [ios, android, web] } }\n";

fn write(dir: &Path, rel: &str, text: &str) {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, text).unwrap();
}

/// A scene document whose one line is `line`.
fn scene(line: &str) -> String {
    format!("---\nkind: scene\nid: a\n---\n\n## a\n\n@mira: {line}\n")
}

/// A project activating `acme.stage`, whose `hosttokens/t.yaml` is `tokens`,
/// with `scenes/a.lute` holding `line`.
fn project(tag: &str, tokens: &str, line: &str) -> lute_scratch::ScratchDir {
    let dir = lute_scratch::ScratchDir::named(tag);
    write(&dir, "lute.project.yaml", PROJECT);
    write(&dir, "plugins/acme.stage/plugin.yaml", PLUGIN);
    write(&dir, "plugins/acme.stage/hosttokens/t.yaml", tokens);
    write(&dir, "scenes/a.lute", &scene(line));
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

fn text(o: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

/// Every `[CODE] message` of `lute check --project .`, in output order.
fn check(dir: &Path) -> Vec<String> {
    let out = lute(dir, &["check", "--project", "."]);
    text(&out)
        .lines()
        .filter_map(|l| {
            let at = l.find(" [")?;
            let d = &l[at + 1..];
            (d.starts_with("[E-") || d.starts_with("[W-")).then(|| d.to_string())
        })
        .collect()
}

fn codes(diags: &[String]) -> Vec<&str> {
    diags
        .iter()
        .filter_map(|d| d.split_once('[')?.1.split_once(']').map(|(c, _)| c))
        .collect()
}

/// The compiled artifact of `scenes/a.lute`.
fn compile(dir: &Path) -> serde_json::Value {
    let out = dir.join("out.json");
    let o = lute(
        dir,
        &[
            "compile",
            "scenes/a.lute",
            "--project",
            ".",
            "-o",
            out.to_str().unwrap(),
        ],
    );
    assert!(o.status.success(), "{}", text(&o));
    serde_json::from_str(&std::fs::read_to_string(out).unwrap()).unwrap()
}

/// The first `line` record of `artifact`.
fn first_line(artifact: &serde_json::Value) -> serde_json::Value {
    fn find(v: &serde_json::Value) -> Option<serde_json::Value> {
        match v {
            serde_json::Value::Object(m)
                if m.get("kind").and_then(|k| k.as_str()) == Some("line") =>
            {
                Some(v.clone())
            }
            serde_json::Value::Object(m) => m.values().find_map(find),
            serde_json::Value::Array(a) => a.iter().find_map(find),
            _ => None,
        }
    }
    find(artifact).expect("a line record")
}

/// The #162 repro: `{{playerName}}`, declared by the plugin, checks clean
/// and compiles to a `reserved` placeholder; `{{userName}}` beside it is
/// unchanged.
#[test]
fn a_declared_host_token_compiles_to_a_reserved_placeholder() {
    let dir = project("ht-repro", TOKENS, "Hi {{playerName}} and {{userName}}.");
    assert_eq!(check(&dir), Vec::<String>::new());
    let line = first_line(&compile(&dir));
    assert_eq!(line["text"], "Hi {{playerName}} and {{userName}}.");
    assert_eq!(
        line["placeholders"],
        serde_json::json!([
            { "kind": "reserved", "token": "playerName" },
            { "kind": "reserved", "token": "userName" },
        ])
    );
}

/// §4.4: a token takes the format hints of its type — number hints on an
/// `int`, text hints on a `string` or `enum`.
#[test]
fn a_host_token_s_type_decides_its_format_hints() {
    for (line, want) in [
        ("{{rank:ordinal}}", Vec::<&str>::new()),
        ("{{rank:cardinalWord}}", vec![]),
        ("{{rank:capitalize}}", vec!["E-REF-TYPE"]),
        ("{{platform:capitalize}}", vec![]),
        ("{{platform:ordinal}}", vec!["E-REF-TYPE"]),
        ("{{playerName:indefinite}}", vec![]),
        ("{{playerName:ordinal}}", vec!["E-REF-TYPE"]),
        ("{{userName:capitalize}}", vec![]),
        ("{{userName:ordinal}}", vec!["E-REF-TYPE"]),
    ] {
        let dir = project("ht-hints", TOKENS, line);
        let diags = check(&dir);
        assert_eq!(codes(&diags), want, "{line}: {diags:?}");
    }
    let dir = project("ht-hint-msg", TOKENS, "{{rank:capitalize}}");
    assert_eq!(
        check(&dir),
        [
            "[E-REF-TYPE] `:capitalize` formats text, but `rank` is an int — write `{{rank}}` \
          without the hint, or an int/double hint (`:cardinalWord`, `:ordinal`)"
        ]
    );
}

/// §4.2: an undeclared bare name is `E-CEL-PROFILE`, naming the declared
/// tokens and where to declare it.
#[test]
fn an_undeclared_host_token_names_the_declared_ones() {
    let dir = project("ht-undeclared", TOKENS, "Hi {{nickname}}.");
    assert_eq!(
        check(&dir),
        [
            "[E-CEL-PROFILE] interpolation `nickname` is not a valid `{{…}}` form — only a state \
          path, a family path indexed by the raised member (`user.bond[occasion.target]`), a \
          def `@ref` / `@ref(args)`, or a declared host token (`platform`, `playerName`, \
          `rank`, `userName`) are permitted; declare `nickname` in a plugin's `hostTokens:` \
          export if the engine substitutes it; name a computed number with a `@def` and show \
          `{{@name}}`; text shown only while a condition holds is a line of its own — \
          `@who{when=\"…\"}: …`"
        ]
    );
}

/// §4.2: a referent that is not an identifier stays in the `Path` bucket
/// with its 0.49 `E-CEL-PROFILE` (no "declare" advice), and so does a bare
/// state root.
#[test]
fn a_non_identifier_referent_keeps_its_path_diagnostic() {
    for line in ["{{player-name}}", "{{2nd}}"] {
        let dir = project("ht-path", TOKENS, line);
        let diags = check(&dir);
        assert_eq!(codes(&diags), ["E-CEL-PROFILE"], "{line}: {diags:?}");
        assert!(
            !diags[0].contains("hostTokens:"),
            "{line} is not a host token spelling: {diags:?}"
        );
        assert!(
            diags[0].contains("or a declared host token (`platform`, `playerName`, `rank`, `userName`) are permitted;"),
            "{diags:?}"
        );
    }
}

/// §4.1 name rejection: a key that is not an identifier is `E-PLUGIN-PARSE`
/// at load, at its line.
#[test]
fn a_host_token_name_must_be_an_identifier() {
    for (key, shown) in [
        ("player-name", "`player-name`"),
        ("2ndName", "`2ndName`"),
        ("\"-x\"", "`-x`"),
        ("\"\"", "``"),
    ] {
        let dir = project(
            "ht-name",
            &format!("hostTokens:\n  {key}: {{ type: string }}\n"),
            "hi",
        );
        let out = text(&lute(&dir, &["check", "--project", "."]));
        assert!(
            out.contains(&format!(
                "./plugins/acme.stage/hosttokens/t.yaml:2:3: error [E-PLUGIN-PARSE] host token \
                 {shown} is not an identifier (letters, digits and `_`, not starting with a \
                 digit)"
            )),
            "{key}: {out}"
        );
    }
}

/// The empty host-token key is located by the host-token lookup alone:
/// another export's empty name keeps its 0.49 report, with no position.
#[test]
fn the_empty_key_locator_is_host_token_only() {
    let dir = project("ht-occ-empty", "hostTokens: {}\n", "hi");
    write(
        &dir,
        "plugins/acme.stage/plugin.yaml",
        &PLUGIN.replace("hostTokens: hosttokens/", "occasions: occasions/"),
    );
    write(
        &dir,
        "plugins/acme.stage/occasions/o.yaml",
        "occasions:\n  \"\": {}\n",
    );
    let out = text(&lute(&dir, &["check", "--project", "."]));
    assert!(
        out.contains(
            "lute: E-PLUGIN-PARSE: ./plugins/acme.stage/occasions/o.yaml: occasion `` is not a \
             name"
        ),
        "{out}"
    );
}

/// §4.1: only `string`, `int` and `{ enum: [...] }` are token types, and
/// a token takes only `type:`.
#[test]
fn a_host_token_takes_a_string_int_or_enum_type_only() {
    let dir = project("ht-type", "hostTokens:\n  flag: { type: bool }\n", "hi");
    let diags = check(&dir);
    assert!(
        diags.iter().any(|d| d.starts_with("[E-PLUGIN-PARSE]")
            && d.contains(
                "a host token's `type` is `string`, `int` or `{ enum: [...] }`, not `bool`"
            )),
        "{diags:?}"
    );
    let dir = project(
        "ht-key",
        "hostTokens:\n  flag: { type: string, default: x }\n",
        "hi",
    );
    assert!(
        codes(&check(&dir)).contains(&"E-PLUGIN-KEY"),
        "{:?}",
        check(&dir)
    );
}

/// §4.1: a state root or CEL literal is `E-PLUGIN-RESERVED-NAME` at
/// assembly, and the token is dropped.
#[test]
fn a_host_token_named_like_a_state_root_is_reserved() {
    let dir = project(
        "ht-reserved",
        "hostTokens:\n  run: { type: string }\n  \"true\": { type: string }\n",
        "hi",
    );
    let diags = check(&dir);
    assert_eq!(
        diags,
        [
            "[E-PLUGIN-RESERVED-NAME] plugin `acme.stage`: `run` cannot name a plugin host \
             token because it is a state root: `{{run}}` would read as a state root — rename it \
             (e.g. `hostRun`)",
            "[E-PLUGIN-RESERVED-NAME] plugin `acme.stage`: `true` cannot name a plugin host \
             token because it is a CEL literal: `{{true}}` would read as the literal — rename \
             it (e.g. `hostTrue`)",
        ],
    );
}

/// §4.1: a token another plugin — or `lute.core` — declares is
/// `E-PLUGIN-DUP-ACROSS`; the first declaration is kept.
#[test]
fn a_host_token_has_one_owner() {
    let dir = project(
        "ht-dup",
        "hostTokens:\n  userName: { type: int }\n",
        "{{userName:capitalize}}",
    );
    let diags = check(&dir);
    assert_eq!(codes(&diags), ["E-PLUGIN-DUP-ACROSS"], "{diags:?}");
    assert!(
        diags[0].contains("hostToken `userName` is declared by both `lute.core` and `acme.stage`"),
        "{diags:?}"
    );

    let dir = project("ht-dup2", TOKENS, "{{playerName}}");
    write(
        &dir,
        "lute.project.yaml",
        "pluginsDir: plugins/\ndefaultProfile: game\nprofiles:\n  game:\n    plugins:\n      \
         acme.stage: true\n      beta.stage: true\n",
    );
    write(
        &dir,
        "plugins/beta.stage/plugin.yaml",
        &PLUGIN.replace("acme.stage", "beta.stage"),
    );
    write(
        &dir,
        "plugins/beta.stage/hosttokens/t.yaml",
        "hostTokens:\n  playerName: { type: int }\n",
    );
    let diags = check(&dir);
    assert_eq!(codes(&diags), ["E-PLUGIN-DUP-ACROSS"], "{diags:?}");
    assert!(
        diags[0].contains(
            "hostToken `playerName` is declared by both `acme.stage` (at \
             ./plugins/acme.stage/hosttokens/t.yaml:2) and `beta.stage`"
        ),
        "{diags:?}"
    );
}

/// §4.2 in a component body: a declared token renders (and keeps its hint
/// check); an undeclared bare name keeps its 0.49 `E-COMPONENT-STATE`,
/// including the missing-`@` hint for a param.
#[test]
fn a_component_body_renders_a_declared_host_token() {
    let dir = project("ht-component", TOKENS, "hi");
    write(
        &dir,
        "scenes/a.lute",
        "---\nkind: scene\nid: a\n---\n\n## a\n\n::use{component=\"greet\" who=\"x\"}\n",
    );
    write(
        &dir,
        "lute.project.yaml",
        &format!("{PROJECT}defaults:\n  components: [components/greet.component.lute]\n"),
    );
    let body = |line: &str| {
        format!("---\ncomponent: greet\nparams:\n  who: string\n---\n\n## Greet\n\n@mira: {line}\n")
    };
    write(
        &dir,
        "components/greet.component.lute",
        &body("Hi {{playerName}}, {{@who}}."),
    );
    assert_eq!(check(&dir), Vec::<String>::new());
    let line = first_line(&compile(&dir));
    assert_eq!(
        line["placeholders"][0],
        serde_json::json!({ "kind": "reserved", "token": "playerName" })
    );

    write(
        &dir,
        "components/greet.component.lute",
        &body("{{rank:capitalize}} {{who}} {{nickname}}"),
    );
    let diags = check(&dir);
    assert_eq!(
        codes(&diags),
        ["E-REF-TYPE", "E-COMPONENT-STATE", "E-COMPONENT-STATE"],
        "{diags:?}"
    );
    assert!(diags[1].contains("write `{{@who}}`"), "{diags:?}");
    assert!(
        diags[2].contains("`nickname` reads ambient state"),
        "{diags:?}"
    );
}

/// §4.1/§4.3: a plugin token changes `capabilitySnapshot` and is listed by
/// `lute context`; a project whose plugin declares none keeps the core
/// stamp and lists no `hostTokens`.
#[test]
fn plugin_tokens_fold_into_the_snapshot_and_the_context_surface() {
    let context = |dir: &Path| -> serde_json::Value {
        let o = lute(
            dir,
            &["context", "scenes/a.lute", "--project", ".", "--json"],
        );
        assert!(o.status.success(), "{}", text(&o));
        serde_json::from_slice(&o.stdout).unwrap()
    };
    let with = project("ht-ctx", TOKENS, "{{playerName}}");
    let without = project("ht-ctx-none", "hostTokens: {}\n", "hi");
    let core_only = lute_scratch::ScratchDir::named("ht-ctx-core");
    write(&core_only, "scenes/a.lute", &scene("hi"));

    let with = context(&with);
    let without = context(&without);
    let core_only = context(&core_only);
    assert_eq!(
        with["hostTokens"],
        serde_json::json!([
            { "name": "platform", "type": { "enum": ["ios", "android", "web"] }, "plugin": "acme.stage" },
            { "name": "playerName", "type": "string", "plugin": "acme.stage" },
            { "name": "rank", "type": "int", "plugin": "acme.stage" },
            { "name": "userName", "type": "string", "plugin": "lute.core" },
        ])
    );
    assert!(without.get("hostTokens").is_none(), "{without:#}");
    assert!(core_only.get("hostTokens").is_none(), "{core_only:#}");
    assert_ne!(with["capabilitySnapshot"], without["capabilitySnapshot"]);
    // The core stamp for the same document. `lute-0.49.1 context` printed
    // `babc4707…`, and declaring `userName` in `lute.core` (0.50) did not move
    // it; the core `::jump` declaring `max` (dsl 0.51.0 §3.1, §13 class 4)
    // moved it once, to this value.
    assert_eq!(
        core_only["capabilitySnapshot"],
        "529a74383209e1736d16b44daa4c140501893cd74051ec6b53ad0c8ebfe457ae"
    );
    // A plugin whose `hostTokens` export is empty hashes as the same plugin
    // without the export.
    let no_export = project("ht-ctx-noexport", "hostTokens: {}\n", "hi");
    write(
        &no_export,
        "plugins/acme.stage/plugin.yaml",
        &PLUGIN.replace("exports:\n  hostTokens: hosttokens/\n", "exports: {}\n"),
    );
    assert_eq!(
        context(&no_export)["capabilitySnapshot"],
        without["capabilitySnapshot"]
    );
}
