//! 0.51.0 §5.1–§5.4 and §7 (#165, #176): localizable directive text — a
//! directive attribute typed `text`, its checks (the choice-label grammar,
//! `@ref` refusal, `code=` admission), its identity
//! (`{prefix}~{tag}~{code}~{attr}`, `E-LINE-CODE-RESERVED`,
//! `E-IDENTITY-TEMPLATE`, `E-DUP-LINE-CODE`, `W-LINE-CODE-UNTAGGED`,
//! project-wide `E-LINE-ID-DUP`), `lute tag`, the stamp rules and the IR
//! `localized` shape.
//!
//! On 0.50.0 `type: text` is an unknown type: the plugin below fails to load
//! (`E-PLUGIN-PARSE`, then `E-PLUGIN-MISSING-ACTIVE`), so no project here
//! checks clean and no record carries `localized`.

use std::path::Path;
use std::process::{Command, Output};

use serde_json::Value;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

const PROJECT: &str = "pluginsDir: plugins/\ndefaultProfile: game\nprofiles:\n  game:\n    \
                       plugins:\n      acme.ui: true\ndefaults:\n  contentLang: en\n  uses: \
                       [schema/world.schema.yaml]\n";

const PLUGIN: &str = "id: acme.ui\nversion: 0.1.0\nkind: capability\ndepends:\n  - { id: \
                      lute.core, range: \"^0.0.1\" }\nexports:\n  directives: directives/\n";

const DIRECTIVES: &str = concat!(
    "directives:\n",
    "  - name: banner\n",
    "    attrs:\n",
    "      - { name: title, required: true, type: text }\n",
    "      - { name: subtitle, type: text }\n",
    "      - { name: style, type: { enum: [toast, card] } }\n",
    "  - name: note\n",
    "    attrs:\n",
    "      - { name: body, type: text }\n",
);

/// Two defs the host also reads (`host: true`), so a test that does not use
/// them draws no `W-DEF-UNUSED`.
const SCHEMA: &str =
    "state:\n  run.gold: { type: int, default: 0 }\n  run.name: { type: string }\n\
                      defs:\n  count: { cel: \"run.gold + 1\", host: true }\n  flag: { cel: \
                      \"run.gold > 1\", host: true }\n";

fn write(dir: &Path, rel: &str, text: &str) {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, text).unwrap();
}

/// The plugin above plus `files` (a later file of the same path wins).
fn project(tag: &str, files: &[(&str, &str)]) -> lute_scratch::ScratchDir {
    let dir = lute_scratch::ScratchDir::named(tag);
    write(&dir, "lute.project.yaml", PROJECT);
    write(&dir, "plugins/acme.ui/plugin.yaml", PLUGIN);
    write(&dir, "plugins/acme.ui/directives/d.yaml", DIRECTIVES);
    write(&dir, "schema/world.schema.yaml", SCHEMA);
    for (rel, text) in files {
        write(&dir, rel, text);
    }
    dir
}

/// A scene document `id` with `body`.
fn scene(id: &str, body: &str) -> String {
    format!("---\nkind: scene\nid: {id}\n---\n\n## Shot 1.\n\n{body}\n")
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

/// Every diagnostic line of `lute check --project .` (`path:line:col:
/// severity [CODE] message`).
fn check(dir: &Path) -> Vec<String> {
    let out = lute(dir, &["check", "--project", "."]);
    text(&out)
        .lines()
        .filter(|l| l.contains(" [") && !l.starts_with("ok:") && !l.starts_with("failed:"))
        .map(str::to_string)
        .collect()
}

/// The compiled commands of `file` (a check-clean document).
fn commands(dir: &Path, file: &str) -> Vec<Value> {
    let out_path = dir.join("out.json");
    let out = lute(
        dir,
        &[
            "compile",
            file,
            "--project",
            ".",
            "-o",
            out_path.to_str().unwrap(),
        ],
    );
    assert!(out.status.success(), "{}", text(&out));
    let artifact: Value =
        serde_json::from_str(&std::fs::read_to_string(&out_path).unwrap()).unwrap();
    artifact["commands"].as_array().unwrap().clone()
}

fn plugins(cmds: &[Value]) -> Vec<&Value> {
    cmds.iter().filter(|c| c["kind"] == "plugin").collect()
}

/// Every `lineId` of `cmds`: lines, options, prompts and directive texts.
fn line_ids(cmds: &[Value]) -> Vec<String> {
    let mut ids = Vec::new();
    for c in cmds {
        if let Some(id) = c["lineId"].as_str() {
            ids.push(id.to_string());
        }
        if let Some(id) = c["promptLineId"].as_str() {
            ids.push(id.to_string());
        }
        for o in c["options"].as_array().into_iter().flatten() {
            ids.push(o["lineId"].as_str().unwrap().to_string());
        }
        for (_, t) in c["localized"].as_object().into_iter().flatten() {
            ids.push(t["lineId"].as_str().unwrap().to_string());
        }
    }
    ids
}

#[test]
fn a_text_attribute_compiles_to_localized_with_its_directive_identity() {
    let dir = project(
        "dt-ir",
        &[(
            "scenes/intro.lute",
            &scene(
                "intro",
                "::banner{code=\"0010\" title=\"Welcome, {{userName}}: {{run.gold}} {{@count}}\" \
                 subtitle=\"Hi\" style=\"card\"}\n::bg{location=\"hall\"}",
            ),
        )],
    );
    assert!(check(&dir).is_empty(), "{:#?}", check(&dir));
    let cmds = commands(&dir, "scenes/intro.lute");
    let banner = plugins(&cmds)[0];
    // `fields` keeps the source string with its markers; `code` is no field.
    assert_eq!(
        banner["fields"],
        serde_json::json!({
            "style": "card",
            "subtitle": "Hi",
            "title": "Welcome, {{userName}}: {{run.gold}} {{@count}}",
        })
    );
    let localized = &banner["localized"];
    assert_eq!(localized["title"]["lineId"], "intro~banner~0010~title");
    assert_eq!(
        localized["subtitle"]["lineId"],
        "intro~banner~0010~subtitle"
    );
    // Placeholders as an option label's: a host token, a path, a def with
    // its inlined body.
    let kinds: Vec<&str> = localized["title"]["placeholders"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["reserved", "path", "ref"]);
    assert!(localized["title"]["placeholders"][2]["expr"].is_object());
    assert!(localized["subtitle"].get("placeholders").is_none());
    assert!(localized["title"].get("texts").is_none());
    assert!(banner.get("stamps").is_none());
    // A record without `text` attributes is unchanged.
    let bg = cmds.iter().find(|c| c["kind"] == "bg").unwrap();
    assert!(bg.get("localized").is_none());
}

#[test]
fn text_values_are_checked_as_label_interpolations() {
    let dir = project(
        "dt-interp",
        &[(
            "scenes/intro.lute",
            &scene(
                "intro",
                "::banner{code=\"0010\" title=\"{{userName}} {{run.gold}} {{@count:ordinal}}\"}\n\
                 ::banner{code=\"0020\" title=\"{{playerName}}\"}\n\
                 ::banner{code=\"0030\" title=\"{{run.name}}\"}\n\
                 ::banner{code=\"0040\" title=\"{{@flag:ordinal}}\"}",
            ),
        )],
    );
    let diags = check(&dir);
    let mut codes: Vec<(&str, &str)> = diags
        .iter()
        .map(|d| {
            let line = d.split(':').nth(1).unwrap();
            let code = d.split('[').nth(1).unwrap().split(']').next().unwrap();
            (line, code)
        })
        .collect();
    codes.sort_unstable();
    // `{{userName}}` (a core host token), a defaulted path and a
    // number def are clean (line 8); an undeclared token is `E-CEL-PROFILE`
    // naming the declared ones, a read of a path nothing sets is a label
    // read (`E-STATE-MAYBE-UNAVAILABLE` in a project), a modifier typed
    // wrong `E-REF-TYPE`.
    assert_eq!(
        codes,
        [
            ("10", "E-STATE-MAYBE-UNAVAILABLE"),
            ("11", "E-REF-TYPE"),
            ("9", "E-CEL-PROFILE"),
        ],
        "{diags:#?}"
    );
    assert!(
        diags
            .iter()
            .any(|d| d.contains("E-CEL-PROFILE") && d.contains("(`userName`)")),
        "{diags:#?}"
    );
}

#[test]
fn ref_values_and_code_admission() {
    let dir = project(
        "dt-ref",
        &[(
            "scenes/intro.lute",
            &scene(
                "intro",
                "::banner{code=\"0010\" title=@count}\n::bg{code=\"0010\" location=\"hall\"}\n\
                 ::banner{code=@count title=\"x\"}\n::banner{code=\"a~1\" title=\"y\"}\n\
                 @mira{code=\"b~2\"}: Hello.",
            ),
        )],
    );
    let diags = check(&dir);
    let has = |line: u32, code: &str, needle: &str| {
        diags.iter().any(|d| {
            d.contains(&format!("intro.lute:{line}:"))
                && d.contains(&format!("[{code}]"))
                && d.contains(needle)
        })
    };
    assert!(
        has(
            8,
            "E-ATTR-TYPE",
            "`title` of `::banner` is localizable `text`, written as a quoted string; a `@def` \
             value has no translation of its own"
        ),
        "{diags:#?}"
    );
    assert!(
        has(
            9,
            "E-UNKNOWN-ATTR",
            "`code` is the localization identity of a directive with a `text` attribute, and \
             `::bg` declares none"
        ),
        "{diags:#?}"
    );
    assert!(
        has(
            10,
            "E-ATTR-TYPE",
            "written as a quoted string (`code=\"0010\"`)"
        ),
        "{diags:#?}"
    );
    assert!(
        has(
            11,
            "E-LINE-CODE-RESERVED",
            "`code=\"a~1\"` contains `~`, which is reserved for directive text identities"
        ),
        "{diags:#?}"
    );
    assert!(
        has(12, "E-LINE-CODE-RESERVED", "`code=\"b~2\"`"),
        "{diags:#?}"
    );
    // `@count` on `title` is not also judged as a typed `@ref`.
    assert!(
        !diags.iter().any(|d| d.contains("E-REF-TYPE")),
        "{diags:#?}"
    );
}

#[test]
fn duplicate_and_untagged_directive_codes() {
    let body = "::banner{code=\"0010\" title=\"a\"}\n::banner{code=\" 0010 \" title=\"b\"}\n\
                ::banner{title=\"c\"}\n::note{code=\"0010\"}";
    let dir = project("dt-dup", &[("scenes/intro.lute", &scene("intro", body))]);
    let diags = check(&dir);
    assert!(
        diags.iter().any(|d| d.contains("intro.lute:9:")
            && d.contains(
                "[E-DUP-LINE-CODE] `::banner` calls at lines 8 and 9 share \
                           `code=\"0010\"`; a (directive, code) pair is the localization \
                           identity of its `text` attributes"
            )),
        "{diags:#?}"
    );
    // `::note{code="0010"}` has its own keyspace; no warning without the policy.
    assert_eq!(diags.len(), 1, "{diags:#?}");
    // With `identity.requireStable`, the untagged call warns.
    write(
        &dir,
        "lute.project.yaml",
        &format!("{PROJECT}identity:\n  requireStable: true\n"),
    );
    let diags = check(&dir);
    assert!(
        diags.iter().any(|d| d.contains(
            "[W-LINE-CODE-UNTAGGED] `::banner` has `text` \
                                         attributes but no `code`; run `lute tag` to allocate \
                                         one"
        )),
        "{diags:#?}"
    );
}

/// The compiler's back-fill and `lute tag` agree: tagging an untagged
/// document renames no `lineId` it compiled to.
#[test]
fn back_fill_matches_lute_tag() {
    let body = "::banner{title=\"first\"}\n@mira: Hello.\n::banner{code=\"0030\" title=\"x\"}\n\
                ::banner{title=\"second\" subtitle=\"s\"}\n::note\n<timeline>\n\
                <track channel=\"ui\">\n::banner{at=\"2s\" title=\"late\"}\n\
                ::banner{at=\"1s\" title=\"early\"}\n</track>\n</timeline>";
    let dir = project("dt-tag", &[("scenes/intro.lute", &scene("intro", body))]);
    let before = line_ids(&commands(&dir, "scenes/intro.lute"));
    let out = lute(&dir, &["tag", "scenes/intro.lute"]);
    assert!(out.status.success(), "{}", text(&out));
    let tagged = std::fs::read_to_string(dir.join("scenes/intro.lute")).unwrap();
    assert!(
        tagged.contains("::banner{code=\"0040\" title=\"first\"}"),
        "{tagged}"
    );
    assert!(
        tagged.contains("::banner{code=\"0050\" title=\"second\""),
        "{tagged}"
    );
    // A tag of its own keyspace; a call without a block gains one.
    assert!(tagged.contains("::note{code=\"0010\"}"), "{tagged}");
    // Source order, though the timeline plays `early` first.
    assert!(
        tagged.contains("::banner{code=\"0060\" at=\"2s\" title=\"late\"}"),
        "{tagged}"
    );
    assert!(
        tagged.contains("::banner{code=\"0070\" at=\"1s\" title=\"early\"}"),
        "{tagged}"
    );
    assert!(tagged.contains("@mira{code=\"0010\"}: Hello."), "{tagged}");
    let after = line_ids(&commands(&dir, "scenes/intro.lute"));
    assert_eq!(before, after);
    assert!(
        before.contains(&"intro~banner~0070~title".to_string()),
        "{before:#?}"
    );
    // Idempotent.
    lute(&dir, &["tag", "scenes/intro.lute"]);
    assert_eq!(
        std::fs::read_to_string(dir.join("scenes/intro.lute")).unwrap(),
        tagged
    );
    // `--renumber` renumbers directive codes per tag in source order.
    let out = lute(
        &dir,
        &["tag", "--renumber", "--commit", "scenes/intro.lute"],
    );
    assert!(out.status.success(), "{}", text(&out));
    let renumbered = std::fs::read_to_string(dir.join("scenes/intro.lute")).unwrap();
    assert!(
        renumbered.contains("::banner{code=\"0010\" title=\"first\"}"),
        "{renumbered}"
    );
    assert!(
        renumbered.contains("::banner{code=\"0020\" title=\"x\"}"),
        "{renumbered}"
    );
    assert!(renumbered.contains("::note{code=\"0010\"}"), "{renumbered}");
}

/// Directive text ids never meet a line, option or prompt id of the same
/// parts, and component instances keep theirs apart.
#[test]
fn identity_collisions_are_all_distinct() {
    let component = "---\ncomponent: card\n---\n## Card\n::banner{title=\"in a card\"}\n";
    let body = "::banner{code=\"0010\" title=\"t\"}\n@banner{code=\"0010\"}: I am a speaker.\n\
                <branch id=\"banner_0010\" prompt=\"Pick\">\n<choice id=\"title\" text=\"T\">\n\
                @mira: ok\n</choice>\n</branch>\n::use{component=\"card\" instance=\"use-001\"}\n\
                ::use{component=\"card\" instance=\"use-002\"}";
    let doc = format!(
        "---\nkind: scene\nid: intro\ncomponents: [card.component.lute]\n---\n\n## Shot 1.\n\n{body}\n"
    );
    let dir = project(
        "dt-ids",
        &[
            ("scenes/intro.lute", &doc),
            ("scenes/card.component.lute", component),
        ],
    );
    assert!(check(&dir).is_empty(), "{:#?}", check(&dir));
    let ids = line_ids(&commands(&dir, "scenes/intro.lute"));
    let mut unique = ids.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), ids.len(), "{ids:#?}");
    for id in [
        "intro~banner~0010~title",
        "intro.banner_0010",
        "intro.banner_0010.title",
        "intro.banner_0010.prompt",
        "intro.card#use-001~banner~0010~title",
        "intro.card#use-002~banner~0010~title",
    ] {
        assert!(ids.iter().any(|i| i == id), "{id} not in {ids:#?}");
    }
}

#[test]
fn custom_templates_and_project_wide_line_id_uniqueness() {
    let project_yaml = format!("{PROJECT}identity:\n  lineId: \"{{speaker}}_{{code}}\"\n");
    let dir = project(
        "dt-lineid",
        &[
            ("lute.project.yaml", &project_yaml),
            (
                "scenes/a.lute",
                &scene(
                    "a",
                    "@mira{code=\"0010\"}: One.\n::banner{code=\"0010\" title=\"t\"}",
                ),
            ),
            ("scenes/b.lute", &scene("b", "@mira{code=\"0010\"}: Two.")),
        ],
    );
    let diags = check(&dir);
    assert_eq!(
        diags,
        [
            "./scenes/b.lute:8:1: error [E-LINE-ID-DUP] lineId `mira_0010` of this line is also \
             the lineId of the line at scenes/a.lute:8 — a lineId keys one translation"
        ],
        "{diags:#?}"
    );
    // The custom template does not touch directive text.
    let cmds = commands(&dir, "scenes/a.lute");
    assert_eq!(
        plugins(&cmds)[0]["localized"]["title"]["lineId"],
        "a~banner~0010~title"
    );
    let out = lute(&dir, &["compile", "--project", ".", "-o", "build"]);
    assert!(!out.status.success());
    assert!(text(&out).contains("[E-LINE-ID-DUP] scenes/b.lute: lineId `mira_0010`"));
    // A `~` in the template is refused.
    write(
        &dir,
        "lute.project.yaml",
        &format!("{PROJECT}identity:\n  lineId: \"{{prefix}}~{{speaker}}_{{code}}\"\n"),
    );
    let out = lute(&dir, &["check", "--project", "."]);
    assert!(
        text(&out).contains(
            "[E-IDENTITY-TEMPLATE] identity template `lineId` contains `~`, which is reserved \
             for directive text identities"
        ),
        "{}",
        text(&out)
    );
}

/// 0.51.0 §5.1 and Review051b F5: the implicit `code` is the directive's
/// own — a scoped `code` stamp on it is refused, a cross-cutting one is
/// neither a field nor a stamp there, and still a stamp elsewhere.
#[test]
fn code_stamps_and_text_stamps() {
    let plugin = format!("{PLUGIN}  stampAttrs: stamps/\n");
    let dir = project(
        "dt-stamps",
        &[
            ("plugins/acme.ui/plugin.yaml", &plugin),
            (
                "plugins/acme.ui/stamps/s.yaml",
                "stampAttrs:\n  - { name: code, type: string, on: [banner] }\n  - { name: note, \
                 type: text }\n",
            ),
            (
                "scenes/intro.lute",
                &scene("intro", "::banner{code=\"0010\" title=\"t\"}"),
            ),
        ],
    );
    let out = text(&lute(&dir, &["check", "--project", "."]));
    assert!(
        out.contains(
            "E-PLUGIN-RESERVED-STAMP-ATTR: stamp attribute `code` (`acme.ui`) is scoped to \
             `banner`, but `code` is an attribute of `::banner` itself"
        ),
        "{out}"
    );
    assert!(
        out.contains(
            "E-PLUGIN-INVALID-DIRECTIVE: plugin `acme.ui` declares stamp attribute `note` as \
             `text`"
        ),
        "{out}"
    );
    write(
        &dir,
        "plugins/acme.ui/stamps/s.yaml",
        "stampAttrs:\n  - { name: code, type: string }\n",
    );
    write(
        &dir,
        "scenes/intro.lute",
        &scene(
            "intro",
            "::banner{code=\"0010\" title=\"t\"}\n::sfx{code=\"x1\" sound=\"a\"}\n\
             @mira{code=\"0010\"}: Hi.",
        ),
    );
    assert!(check(&dir).is_empty(), "{:#?}", check(&dir));
    let cmds = commands(&dir, "scenes/intro.lute");
    let banner = plugins(&cmds)[0];
    assert!(banner["fields"].get("code").is_none(), "{banner}");
    assert!(banner.get("stamps").is_none(), "{banner}");
    assert_eq!(
        banner["localized"]["title"]["lineId"],
        "intro~banner~0010~title"
    );
    let sfx = cmds.iter().find(|c| c["kind"] == "sfx").unwrap();
    assert_eq!(sfx["stamps"]["code"], "x1");
    let line = cmds.iter().find(|c| c["kind"] == "line").unwrap();
    assert_eq!(line["lineId"], "intro.mira_0010");
}

/// Every §5.1 load fault, at its attribute.
#[test]
fn load_faults() {
    let directives = concat!(
        "directives:\n",
        "  - name: banner\n",
        "    attrs:\n",
        "      - { name: title, required: true, type: text }\n",
        "      - { name: code, type: string }\n",
        "  - name: card\n",
        "    lower: { record: cg, fields: { assetId: { fromAttr: asset } } }\n",
        "    attrs:\n",
        "      - { name: asset, type: string }\n",
        "      - { name: caption, type: text }\n",
        "  - name: toast\n",
        "    attrs:\n",
        "      - { name: msg, type: text, default: \"hi\" }\n",
        "      - { name: tags, type: { list: text } }\n",
    );
    let plugin = format!(
        "{PLUGIN}  occasions: occasions/\n  rewardKinds: rewardkinds/\n  bridge: bridge/\n\
         options:\n  - {{ name: greeting, type: text }}\n"
    );
    let dir = project(
        "dt-faults",
        &[
            ("plugins/acme.ui/plugin.yaml", &plugin),
            ("scenes/intro.lute", &scene("intro", "@mira: Hi.")),
            ("plugins/acme.ui/directives/d.yaml", directives),
            (
                "plugins/acme.ui/occasions/o.yaml",
                "occasions:\n  toast:\n    payload: { line: text }\n",
            ),
            (
                "plugins/acme.ui/rewardkinds/r.yaml",
                "rewardKinds:\n  BADGE:\n    attrs:\n      - { name: label, type: text }\n",
            ),
            (
                "plugins/acme.ui/bridge/b.yaml",
                "bridgeCapabilities:\n  - service: chat\n    operation: ask\n    result:\n      \
                 - { name: reply, type: text }\n",
            ),
        ],
    );
    let out = text(&lute(&dir, &["check", "--project", "."]));
    for (site, message) in [
        (
            "directives/d.yaml:5:9",
            "directive `::banner` declares an attribute `code` beside its `text` attribute \
             `title`; `code` is the identity attribute of a directive with `text` attributes (it \
             keys their translations) — rename the attribute",
        ),
        (
            "directives/d.yaml:10:9",
            "`caption` of `::card` is `text`, but `::card` lowers to the `cg` record, which \
             carries no localized text — declare it `string`, or drop `lower:` so the call stays \
             a `plugin` record",
        ),
        (
            "directives/d.yaml:13:9",
            "`msg` of `::toast` is `text` with `default: \"hi\"`; a default is not authored \
             source, so it has no `code` and no translation — drop `default:`",
        ),
        (
            "directives/d.yaml:14:29",
            "`directives[2].attrs[1].type` declares the type `text`, which is admitted only as \
             the type of a directive attribute",
        ),
        (
            "occasions/o.yaml:3:22",
            "payload field `line` has the type `text`, which is admitted only as the type of a \
             directive attribute",
        ),
        ("plugin.yaml:", "option `greeting` declares the type `text`"),
        (
            "rewardkinds/r.yaml:4:30",
            "`rewardKinds.BADGE.attrs[0].type` declares the type `text`",
        ),
        (
            "bridge/b.yaml:5:30",
            "`bridgeCapabilities[0].result[0].type` declares the type `text`",
        ),
    ] {
        assert!(
            out.lines()
                .any(|l| l.contains(site) && l.contains("[E-PLUGIN-PARSE]") && l.contains(message)),
            "{site}: {message}\n{out}"
        );
    }
}

/// `type: text` decodes since 0.51, so an author `state:` row must refuse it
/// rather than declare a phantom text-typed path.
#[test]
fn a_text_state_declaration_is_refused() {
    let dir = project(
        "dt-state",
        &[
            (
                "schema/world.schema.yaml",
                "state:\n  run.motto: { type: text }\n",
            ),
            ("scenes/intro.lute", &scene("intro", "@mira: Hi.")),
        ],
    );
    let diags = check(&dir);
    assert!(
        diags.iter().any(|d| d.contains(
            "[E-STATE-DECL] state path `run.motto` cannot declare \
                                         the type `text`"
        )),
        "{diags:#?}"
    );
}

/// A scene `intro` importing `components/card.component.lute` (`component`)
/// with `body`.
fn with_card(tag: &str, component: &str, body: &str) -> lute_scratch::ScratchDir {
    let doc = format!(
        "---\nkind: scene\nid: intro\ncomponents: [../components/card.component.lute]\n---\n\n\
         ## Shot 1.\n\n{body}\n"
    );
    project(
        tag,
        &[
            ("scenes/intro.lute", &doc),
            ("components/card.component.lute", component),
        ],
    )
}

/// S6 review #1: a component's untagged directive calls take their codes
/// from the whole component source, before its arguments fold a `<match>`
/// — the codes `lute tag` writes into the component file.
#[test]
fn component_directive_codes_survive_folded_matches_and_tagging() {
    let component = "---\ncomponent: card\nparams:\n  mode: string\n---\n\n## Card\n\n\
                     <match subject=\"@mode\">\n<when is=\"a\">\n::banner{title=\"First\"}\n\
                     </when>\n<otherwise>\n::banner{title=\"Second\"}\n</otherwise>\n</match>\n";
    let dir = with_card(
        "dt-comp-fold",
        component,
        "::use{component=\"card\" instance=\"one\" mode=\"b\"}",
    );
    assert!(check(&dir).is_empty(), "{:#?}", check(&dir));
    let before = line_ids(&commands(&dir, "scenes/intro.lute"));
    assert_eq!(before, ["intro.card#one~banner~0020~title"]);
    let out = lute(&dir, &["tag", "components/card.component.lute"]);
    assert!(out.status.success(), "{}", text(&out));
    let tagged = std::fs::read_to_string(dir.join("components/card.component.lute")).unwrap();
    assert!(
        tagged.contains("::banner{code=\"0020\" title=\"Second\"}"),
        "{tagged}"
    );
    assert_eq!(line_ids(&commands(&dir, "scenes/intro.lute")), before);
}

/// S6 review #5: a `{{@param}}` in a component's `text` attribute is bound
/// at expansion — a literal argument spliced, as in a content line — never
/// read as a host def.
#[test]
fn component_params_bind_inside_text_attributes() {
    let component = "---\ncomponent: card\nparams:\n  mode: string\n---\n\n## Card\n\n\
                     ::banner{title=\"Mode {{@mode}} for {{userName}}\"}\n";
    let dir = with_card(
        "dt-comp-param",
        component,
        "::use{component=\"card\" instance=\"one\" mode=\"fast\"}",
    );
    assert!(check(&dir).is_empty(), "{:#?}", check(&dir));
    let cmds = commands(&dir, "scenes/intro.lute");
    let banner = plugins(&cmds)[0];
    assert_eq!(banner["fields"]["title"], "Mode fast for {{userName}}");
    let kinds: Vec<&str> = banner["localized"]["title"]["placeholders"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["reserved"]);
}

/// S6 review #4: a component body's `text` attributes are judged as its
/// content lines' text is — an ambient read or an undeclared name is
/// refused there, through a `::use` and standalone alike.
#[test]
fn component_text_attributes_are_checked_as_body_text() {
    let component = "---\ncomponent: card\nparams:\n  mode: string\n---\n\n## Card\n\n\
                     ::banner{title=\"{{notADeclaredHostToken}} {{@mode}}\"}\n\
                     ::banner{title=\"{{@nope}}\"}\n";
    let dir = with_card(
        "dt-comp-interp",
        component,
        "::use{component=\"card\" instance=\"one\" mode=\"fast\"}",
    );
    let diags = check(&dir);
    for code in ["E-COMPONENT-STATE", "E-UNDECLARED-REF"] {
        assert!(
            diags
                .iter()
                .any(|d| d.contains("components/card.component.lute") && d.contains(code)),
            "{code}: {diags:#?}"
        );
    }
    assert!(
        diags
            .iter()
            .any(|d| d.contains("E-COMPONENT-STATE") && d.contains("notADeclaredHostToken")),
        "{diags:#?}"
    );
}

/// S6 review #7: an `E-LINE-ID-DUP` unit expanded from a component is
/// placed in the component's file, at its line.
#[test]
fn a_component_line_id_collision_points_into_the_component() {
    let component = "---\ncomponent: card\n---\n\n## Card\n\n@mira{code=\"0010\"}: Hello.\n";
    let doc = "---\nkind: scene\nid: intro\ncomponents: [../components/card.component.lute]\n\
               ---\n\n## Shot 1.\n\n::use{component=\"card\" instance=\"one\"}\n\
               ::use{component=\"card\" instance=\"two\"}\n";
    let project_yaml = format!("{PROJECT}identity:\n  lineId: \"{{speaker}}_{{code}}\"\n");
    let dir = project(
        "dt-comp-dup",
        &[
            ("lute.project.yaml", &project_yaml),
            ("scenes/intro.lute", doc),
            ("components/card.component.lute", component),
        ],
    );
    let out = text(&lute(&dir, &["check", "--project", "."]));
    assert!(
        out.lines().any(|l| l
            .contains("components/card.component.lute:7:1: error [E-LINE-ID-DUP]")
            && l.contains("at components/card.component.lute:7 —")),
        "{out}"
    );
}

/// A project whose default profile activates no plugin: a scene opts into
/// `acme.ui` through its own `plugins:`.
const PROJECT_NO_UI: &str = "pluginsDir: plugins/\ndefaultProfile: game\nprofiles:\n  game:\n    \
                             plugins: {}\ndefaults:\n  contentLang: en\n  uses: \
                             [schema/world.schema.yaml]\n";

/// A scene `id` using `components/card.component.lute`, with `acme.ui`
/// active through its own `plugins:` when `ui`.
fn card_user(id: &str, ui: bool) -> String {
    let plugins = if ui { "plugins:\n  acme.ui: {}\n" } else { "" };
    format!(
        "---\nkind: scene\nid: {id}\ncomponents: [../components/card.component.lute]\n\
         {plugins}---\n\n## Shot 1.\n\n::use{{component=\"card\" instance=\"one\"}}\n"
    )
}

const CARD: &str = "---\ncomponent: card\n---\n\n## Card\n\n::banner{title=\"Card\"}\n";

/// S6 review #6: an imported component's `text` directive calls are tagged
/// under the snapshot of the scenes that use it — alone and in a tree.
#[test]
fn a_component_is_tagged_under_its_users_snapshot() {
    for operand in ["components/card.component.lute", "."] {
        let dir = project(
            "dt-tag-users",
            &[
                ("lute.project.yaml", PROJECT_NO_UI),
                ("scenes/a.lute", &card_user("a", true)),
                ("components/card.component.lute", CARD),
            ],
        );
        // The component file alone, under the default profile, does not know
        // `::banner`; the scene that uses it does.
        let before = line_ids(&commands(&dir, "scenes/a.lute"));
        assert_eq!(before, ["a.card#one~banner~0010~title"]);
        let out = lute(&dir, &["tag", operand]);
        assert!(out.status.success(), "{operand}: {}", text(&out));
        let tagged = std::fs::read_to_string(dir.join("components/card.component.lute")).unwrap();
        assert!(
            tagged.contains("::banner{code=\"0010\" title=\"Card\"}"),
            "{operand}: {tagged}"
        );
        assert_eq!(line_ids(&commands(&dir, "scenes/a.lute")), before);
    }
}

/// S6 review #6: users whose snapshots disagree on whether a call is a
/// `text` directive give it no one identity — refused, both users named.
#[test]
fn users_disagreeing_on_a_text_directive_refuse_tagging() {
    let dir = project(
        "dt-tag-conflict",
        &[
            ("lute.project.yaml", PROJECT_NO_UI),
            ("scenes/a.lute", &card_user("a", true)),
            ("scenes/b.lute", &card_user("b", false)),
            ("components/card.component.lute", CARD),
        ],
    );
    let out = lute(&dir, &["tag", "components/card.component.lute"]);
    assert_eq!(out.status.code(), Some(1), "{}", text(&out));
    let shown = text(&out);
    assert!(
        shown.contains(
            "components/card.component.lute:7:1: error [E-COMPONENT-TEXT-DIRECTIVE] `::banner` \
             has `text` attributes under the capability snapshot of scenes/a.lute but not under \
             that of scenes/b.lute"
        ),
        "{shown}"
    );
    assert!(shown.contains("scenes/a.lute:11:1"), "{shown}");
    assert!(shown.contains("scenes/b.lute:9:1"), "{shown}");
    assert_eq!(
        std::fs::read_to_string(dir.join("components/card.component.lute")).unwrap(),
        CARD
    );
}

/// The tagged component text: its `::banner` call got `code="0010"`.
fn assert_card_tagged(dir: &Path, rel: &str, out: &Output) {
    assert!(out.status.success(), "{}", text(out));
    let tagged = std::fs::read_to_string(dir.join(rel)).unwrap();
    assert!(
        tagged.contains("::banner{code=\"0010\" title=\"Card\"}"),
        "{tagged}"
    );
}

/// S6 re-review #1: a document that imports the component but never
/// `::use`s it is no user — the checker's use closure decides, not the
/// import table.
#[test]
fn an_unused_import_is_no_user() {
    let dir = project(
        "dt-tag-unused-import",
        &[
            ("lute.project.yaml", PROJECT_NO_UI),
            ("scenes/a.lute", &card_user("a", true)),
            (
                "scenes/b.lute",
                "---\nkind: scene\nid: b\ncomponents: [../components/card.component.lute]\n---\n\n\
                 ## Shot 1.\n\n@narrator: Hi.\n",
            ),
            ("components/card.component.lute", CARD),
        ],
    );
    let out = lute(&dir, &["tag", "components/card.component.lute"]);
    assert_card_tagged(&dir, "components/card.component.lute", &out);
}

/// S6 re-review #2: a component is a document kind (`component:`
/// frontmatter), not a `.component.lute` file name — a wrapper component
/// is no independent user, and a component named `card.lute` is tagged
/// under its users' snapshot.
#[test]
fn components_are_known_by_frontmatter() {
    let wrapper = "---\ncomponent: wrapper\ncomponents: [card.component.lute]\n---\n\n\
                   ## Wrapper\n\n::use{component=\"card\" instance=\"in\"}\n";
    let scene = "---\nkind: scene\nid: a\ncomponents: [../components/wrapper.lute]\nplugins:\n  \
                 acme.ui: {}\n---\n\n## Shot 1.\n\n::use{component=\"wrapper\" instance=\"w\"}\n";
    let dir = project(
        "dt-tag-wrapper",
        &[
            ("lute.project.yaml", PROJECT_NO_UI),
            ("scenes/a.lute", scene),
            ("components/wrapper.lute", wrapper),
            ("components/card.component.lute", CARD),
        ],
    );
    let out = lute(&dir, &["tag", "components/card.component.lute"]);
    assert_card_tagged(&dir, "components/card.component.lute", &out);

    let scene = "---\nkind: scene\nid: a\ncomponents: [../components/card.lute]\nplugins:\n  \
                 acme.ui: {}\n---\n\n## Shot 1.\n\n::use{component=\"card\" instance=\"one\"}\n";
    let dir = project(
        "dt-tag-plain-name",
        &[
            ("lute.project.yaml", PROJECT_NO_UI),
            ("scenes/a.lute", scene),
            ("components/card.lute", CARD),
        ],
    );
    let out = lute(&dir, &["tag", "components/card.lute"]);
    assert_card_tagged(&dir, "components/card.lute", &out);
}

/// S6 re-review #3: a user in a nested project is read under that
/// project, whose defaults activate `acme.ui`, not the outer one's.
#[test]
fn a_nested_project_user_is_read_under_its_own_project() {
    let nested = "pluginsDir: ../plugins/\ndefaultProfile: game\nprofiles:\n  game:\n    \
                  plugins:\n      acme.ui: true\ndefaults:\n  contentLang: en\n  uses: \
                  [../schema/world.schema.yaml]\n";
    let dir = project(
        "dt-tag-nested-project",
        &[
            ("lute.project.yaml", PROJECT_NO_UI),
            ("scenes/a.lute", &card_user("a", true)),
            ("sub/lute.project.yaml", nested),
            (
                "sub/scenes/n.lute",
                "---\nkind: scene\nid: n\ncomponents: [../../components/card.component.lute]\n\
                 ---\n\n## Shot 1.\n\n::use{component=\"card\" instance=\"one\"}\n",
            ),
            ("components/card.component.lute", CARD),
        ],
    );
    let out = lute(&dir, &["tag", "components/card.component.lute"]);
    assert_card_tagged(&dir, "components/card.component.lute", &out);
}

/// S6 review #2: only a declared type is judged — a literal `default:`
/// that happens to hold a `type: text` entry is data, not a type.
#[test]
fn a_literal_default_holding_type_text_is_not_a_type() {
    let plugin = format!("{PLUGIN}  state: state/\n");
    let dir = project(
        "dt-default-type",
        &[
            ("plugins/acme.ui/plugin.yaml", &plugin),
            (
                "plugins/acme.ui/state/s.yaml",
                "stateShapes:\n  - name: tags\n    fields:\n      - name: labels\n        \
                 type: { map: { key: string, value: string } }\n        \
                 default: { type: text }\n        required: false\n",
            ),
            ("scenes/intro.lute", &scene("intro", "@mira: Hi.")),
        ],
    );
    let out = text(&lute(&dir, &["check", "--project", "."]));
    assert!(!out.contains("E-PLUGIN-PARSE"), "{out}");
    assert!(check(&dir).is_empty(), "{out}");
}

/// S6 review #3: a frontmatter declaration's `schema:` is a type position,
/// bare or nested, as is a def's parameter in either spelling.
#[test]
fn a_text_frontmatter_schema_is_refused() {
    let plugin = format!("{PLUGIN}  frontmatter: frontmatter/\n  defs: defs/\n");
    let dir = project(
        "dt-frontmatter",
        &[
            ("plugins/acme.ui/plugin.yaml", &plugin),
            (
                "plugins/acme.ui/frontmatter/f.yaml",
                "frontmatter:\n  - { key: heading, schema: text }\n  - key: tags\n    \
                 schema: { list: text }\n",
            ),
            (
                "plugins/acme.ui/defs/d.yaml",
                "defs:\n  - name: greet\n    type: string\n    params: { who: text }\n    \
                 cel: \"'hi'\"\n  - name: wave\n    type: string\n    params:\n      \
                 - { name: who, type: { list: text } }\n    cel: \"'hi'\"\n",
            ),
            ("scenes/intro.lute", &scene("intro", "@mira: Hi.")),
        ],
    );
    let out = text(&lute(&dir, &["check", "--project", "."]));
    for (site, message) in [
        (
            "frontmatter/f.yaml:2:29",
            "`frontmatter[0].schema` declares the type `text`, which is admitted only as the \
             type of a directive attribute",
        ),
        (
            "frontmatter/f.yaml:4:13",
            "`frontmatter[1].schema` declares the type `text`",
        ),
        (
            "defs/d.yaml:4:20",
            "`defs[0].params.who` declares the type `text`",
        ),
        (
            "defs/d.yaml:9:28",
            "`defs[1].params[0].type` declares the type `text`",
        ),
    ] {
        assert!(
            out.lines()
                .any(|l| l.contains(site) && l.contains("[E-PLUGIN-PARSE]") && l.contains(message)),
            "{site}: {message}\n{out}"
        );
    }
}
