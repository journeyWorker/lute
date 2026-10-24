//! dsl 0.47.0 §3 + §2.2 (index) through the CLI: localizable branch/hub
//! prompts (`promptLineId`/`promptTexts`, `kind: "prompt"` export rows, the
//! prompt-id `E-CHOICE-DUP`), source hashes (`sourceHash` rows, bundle
//! `schemaVersion: 2`, `W-L10N-STALE`) and the project index's `contentLang`.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value as Json;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn write(dir: &Path, rel: &str, text: &str) -> PathBuf {
    let path = dir.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(&path, text).unwrap();
    path
}

fn run(args: &[&str]) -> Output {
    Command::new(BIN).args(args).output().unwrap()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).to_string()
}

fn read_json(path: &Path) -> Json {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn hash(text: &str) -> String {
    format!(
        "sha256:{}",
        lute_manifest::snapshot::sha256_hex(text.as_bytes())
    )
}

const PROJECT_YAML: &str = "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n";

/// A component line with inline markup and an escape: its plain text (what
/// the compiler emits) differs from the authored text (what export shows).
const COMPONENT: &str = "\
---
component: greet
---

## Greeting
@marina{code=\"0100\"}: :speed[Welcome]{rate=0.8} aboard, \\[friend\\].
";

/// A prompted branch and a prompted hub, a line with markup and an escape, and
/// a `::use` of the component above.
const SCENE: &str = "\
---
kind: scene
character: marina
season: 1
episode: 1
components: [components/greet.component.lute]
---

## Opening.

@marina{code=\"0010\"}: Hello :speed[there]{rate=1.5} \\{friend\\}.
::use{component=\"greet\"}

<branch id=\"pick\" prompt=\"Where now?\">
  <choice id=\"go\" text=\"Go on\">
    @marina{code=\"0020\"}: Onward.
  </choice>
  <choice id=\"stay\" text=\"Stay here\">
    @marina{code=\"0030\"}: Fine.
  </choice>
</branch>

<hub id=\"look\" prompt=\"Look where?\">
  <choice id=\"sea\" text=\"The sea\" once>
    @marina{code=\"0040\"}: Blue.
  </choice>
  <choice id=\"leave\" text=\"Leave\" exit>
    @marina{code=\"0050\"}: Done.
  </choice>
</hub>
";

const PICK_PROMPT: &str = "marina.s01ep01.pick.prompt";
const LOOK_PROMPT: &str = "marina.s01ep01.look.prompt";
const HELLO: &str = "marina.s01ep01.marina_0010";

fn project(tag: &str) -> lute_scratch::ScratchDir {
    let dir = lute_scratch::ScratchDir::named(tag);
    write(&dir, "lute.project.yaml", PROJECT_YAML);
    write(&dir, "components/greet.component.lute", COMPONENT);
    write(&dir, "a.lute", SCENE);
    dir
}

fn export_rows(project: &Path) -> Vec<Json> {
    let out = run(&["loc", "export", "--project", project.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    serde_json::from_slice(&out.stdout).unwrap()
}

/// Export, prefix every `text` with `KO ` (dropping `drop`), write it as
/// `ko.json` in `work`, and import it into `work/bundle.json`.
fn translate_and_import(project: &Path, work: &Path, drop: &[&str]) -> PathBuf {
    let rows: Vec<Json> = export_rows(project)
        .into_iter()
        .filter(|r| !drop.contains(&r["lineId"].as_str().unwrap_or_default()))
        .map(|mut r| {
            let old = r["text"].as_str().unwrap().to_string();
            r["text"] = Json::String(format!("KO {old}"));
            r
        })
        .collect();
    let file = work.join("ko.json");
    std::fs::write(&file, serde_json::to_string_pretty(&rows).unwrap()).unwrap();
    let bundle = work.join("bundle.json");
    let out = run(&[
        "loc",
        "import",
        file.to_str().unwrap(),
        "-o",
        bundle.to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    bundle
}

fn compile_with(project: &Path, bundle: &Path, extra: &[&str]) -> Output {
    let mut owned: Vec<String> = vec![
        "compile".into(),
        project.join("a.lute").to_str().unwrap().into(),
        "--project".into(),
        project.to_str().unwrap().into(),
        "--locale-bundle".into(),
        bundle.to_str().unwrap().into(),
    ];
    owned.extend(extra.iter().map(std::string::ToString::to_string));
    let refs: Vec<&str> = owned.iter().map(String::as_str).collect();
    run(&refs)
}

fn compile_all_with(project: &Path, out: &Path, bundle: Option<&Path>, extra: &[&str]) -> Output {
    let mut owned: Vec<String> = vec![
        "compile".into(),
        "--project".into(),
        project.to_str().unwrap().into(),
        "-o".into(),
        out.to_str().unwrap().into(),
    ];
    if let Some(b) = bundle {
        owned.push("--locale-bundle".into());
        owned.push(b.to_str().unwrap().into());
    }
    owned.extend(extra.iter().map(std::string::ToString::to_string));
    let refs: Vec<&str> = owned.iter().map(String::as_str).collect();
    run(&refs)
}

fn command<'a>(artifact: &'a Json, kind: &str) -> &'a Json {
    artifact["commands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["kind"] == kind)
        .unwrap_or_else(|| panic!("no `{kind}` command"))
}

// --- §2.2: the project index carries `contentLang` ------------------------

#[test]
fn the_project_index_carries_content_lang_from_the_manifest_defaults() {
    for (tag, manifest, expected) in [
        (
            "index-lang",
            format!("{PROJECT_YAML}defaults:\n  contentLang: en-US\n"),
            Some("en-US"),
        ),
        ("index-nolang", PROJECT_YAML.to_string(), None),
    ] {
        let dir = project(tag);
        write(&dir, "lute.project.yaml", &manifest);
        let out = dir.join("out");
        let result = compile_all_with(&dir, &out, None, &[]);
        assert_eq!(result.status.code(), Some(0), "{}", stderr(&result));
        let text = std::fs::read_to_string(out.join("project.index.json")).unwrap();
        let index: Json = serde_json::from_str(&text).unwrap();
        match expected {
            Some(lang) => assert_eq!(index["contentLang"], lang, "{text}"),
            None => assert!(
                !text.contains("\"contentLang\""),
                "omitted without defaults.contentLang: {text}"
            ),
        }
    }
}

// --- §3.1: prompts --------------------------------------------------------

#[test]
fn compile_stamps_prompt_line_ids_and_prompt_less_records_stay_unchanged() {
    let dir = project("prompt-ids");
    let out = run(&["compile", dir.join("a.lute").to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let artifact: Json = serde_json::from_slice(&out.stdout).unwrap();
    let choice = command(&artifact, "choice");
    assert_eq!(choice["prompt"], "Where now?");
    assert_eq!(choice["promptLineId"], PICK_PROMPT);
    assert!(
        choice.get("promptTexts").is_none(),
        "skip-if-empty: {choice}"
    );
    let hub = command(&artifact, "hub");
    assert_eq!(hub["prompt"], "Look where?");
    assert_eq!(hub["promptLineId"], LOOK_PROMPT);

    // Without `prompt=` neither key appears.
    let plain = lute_scratch::ScratchDir::named("prompt-less");
    write(&plain, "lute.project.yaml", PROJECT_YAML);
    write(
        &plain,
        "a.lute",
        "---\nkind: scene\ncharacter: marina\nseason: 1\nepisode: 1\n---\n\n## A.\n\n\
         <branch id=\"pick\">\n  <choice id=\"go\" text=\"Go\">\n    @marina{code=\"0010\"}: Go.\n  </choice>\n</branch>\n",
    );
    let out = run(&["compile", plain.join("a.lute").to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(!text.contains("prompt"), "{text}");
}

#[test]
fn export_emits_a_prompt_row_before_the_options_of_its_branch_or_hub() {
    let dir = project("prompt-rows");
    let rows = export_rows(&dir);
    let pos = |id: &str| {
        rows.iter()
            .position(|r| r["lineId"] == id)
            .unwrap_or_else(|| panic!("no row for `{id}`: {rows:#?}"))
    };
    let pick = &rows[pos(PICK_PROMPT)];
    assert_eq!(pick["kind"], "prompt");
    assert_eq!(pick["key"], "pick.prompt");
    assert_eq!(pick["text"], "Where now?");
    assert_eq!(pick["line"], 14);
    assert!(pick["source"].is_null());
    assert!(pick["file"].as_str().unwrap().ends_with("a.lute"));
    assert!(pos(PICK_PROMPT) < pos("marina.s01ep01.pick.go"));
    let look = &rows[pos(LOOK_PROMPT)];
    assert_eq!(look["kind"], "prompt");
    assert_eq!(look["key"], "look.prompt");
    assert_eq!(look["text"], "Look where?");
    assert!(pos(LOOK_PROMPT) < pos("marina.s01ep01.look.sea"));
    assert!(pos("marina.s01ep01.marina_0030") < pos(LOOK_PROMPT));
}

#[test]
fn prompts_round_trip_into_prompt_texts_single_and_all() {
    let dir = project("prompt-merge");
    let work = lute_scratch::ScratchDir::named("prompt-merge-work");
    let bundle = translate_and_import(&dir, &work, &[]);
    let entries = &read_json(&bundle)["entries"];
    assert_eq!(entries[PICK_PROMPT]["ko"], "KO Where now?");
    assert_eq!(entries[LOOK_PROMPT]["ko"], "KO Look where?");

    let out = compile_with(&dir, &bundle, &[]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert!(out.stderr.is_empty(), "complete bundle: {}", stderr(&out));
    let artifact: Json = serde_json::from_slice(&out.stdout).unwrap();
    let choice = command(&artifact, "choice");
    assert_eq!(choice["prompt"], "Where now?", "source stays");
    assert_eq!(choice["promptTexts"]["ko"], "KO Where now?");
    assert_eq!(
        command(&artifact, "hub")["promptTexts"]["ko"],
        "KO Look where?"
    );

    let all = work.join("all");
    let out = compile_all_with(&dir, &all, Some(&bundle), &[]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let artifact = read_json(&all.join("a.lute.json"));
    assert_eq!(
        command(&artifact, "choice")["promptTexts"]["ko"],
        "KO Where now?"
    );
    assert_eq!(
        command(&artifact, "hub")["promptLineId"],
        LOOK_PROMPT,
        "--all stamps it too"
    );
}

#[test]
fn an_untranslated_prompt_is_w_l10n_missing() {
    let dir = project("prompt-missing");
    let work = lute_scratch::ScratchDir::named("prompt-missing-work");
    let bundle = translate_and_import(&dir, &work, &[PICK_PROMPT]);
    let out = compile_with(&dir, &bundle, &[]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let err = stderr(&out);
    let hits: Vec<&str> = err
        .lines()
        .filter(|l| l.contains("W-L10N-MISSING"))
        .collect();
    assert_eq!(hits.len(), 1, "{err}");
    assert!(
        hits[0].contains(&format!("no `ko` text for `{PICK_PROMPT}`")),
        "{err}"
    );
}

#[test]
fn a_choice_named_prompt_is_e_choice_dup_only_under_a_prompted_branch_or_hub() {
    let dir = lute_scratch::ScratchDir::named("prompt-dup");
    write(&dir, "lute.project.yaml", PROJECT_YAML);
    let scene = |branch_prompt: &str, hub_prompt: &str| {
        format!(
            "---\nkind: scene\ncharacter: marina\nseason: 1\nepisode: 1\n---\n\n## A.\n\n\
             <branch id=\"pick\"{branch_prompt}>\n  <choice id=\"prompt\" text=\"Prompt\">\n    @marina{{code=\"0010\"}}: A.\n  </choice>\n</branch>\n\n\
             <hub id=\"look\"{hub_prompt}>\n  <choice id=\"prompt\" text=\"Prompt\" exit>\n    @marina{{code=\"0020\"}}: B.\n  </choice>\n</hub>\n"
        )
    };
    let check = |text: &str| {
        let file = write(&dir, "a.lute", text);
        let out = run(&["check", file.to_str().unwrap()]);
        let all = format!("{}{}", String::from_utf8_lossy(&out.stdout), stderr(&out));
        (out.status.code(), all)
    };
    let (code, all) = check(&scene(" prompt=\"?\"", " prompt=\"?\""));
    assert_eq!(code, Some(1), "{all}");
    let hits: Vec<&str> = all.lines().filter(|l| l.contains("E-CHOICE-DUP")).collect();
    assert_eq!(hits.len(), 2, "{all}");
    assert!(
        all.contains("choice id `prompt` is the lineId of this branch's prompt; rename the choice"),
        "{all}"
    );
    assert!(all.contains("this hub's prompt"), "{all}");

    let (code, all) = check(&scene("", ""));
    assert_eq!(code, Some(0), "legal without prompt=: {all}");
    assert!(!all.contains("E-CHOICE-DUP"), "{all}");
}

// --- §3.2: source hashes and stale translations ---------------------------

#[test]
fn every_export_row_carries_the_source_hash_of_its_plain_text_in_json_and_csv() {
    let dir = project("hash-rows");
    let rows = export_rows(&dir);
    let by_id = |id: &str| rows.iter().find(|r| r["lineId"] == id).unwrap();
    // The authored text keeps markup and escapes; the hash is of the plain
    // text the compiler emits.
    let hello = by_id(HELLO);
    assert_eq!(hello["text"], "Hello :speed[there]{rate=1.5} \\{friend\\}.");
    assert_eq!(hello["sourceHash"], hash("Hello there {friend}."));
    assert_eq!(by_id(PICK_PROMPT)["sourceHash"], hash("Where now?"));
    assert_eq!(by_id("marina.s01ep01.pick.go")["sourceHash"], hash("Go on"));
    assert!(rows.iter().all(|r| r["sourceHash"]
        .as_str()
        .is_some_and(|h| h.starts_with("sha256:"))));

    let csv = run(&[
        "loc",
        "export",
        "--project",
        dir.to_str().unwrap(),
        "--format",
        "csv",
    ]);
    assert_eq!(csv.status.code(), Some(0), "{}", stderr(&csv));
    let text = String::from_utf8(csv.stdout).unwrap();
    let mut lines = text.split("\r\n");
    assert_eq!(
        lines.next().unwrap(),
        "kind,file,line,lineId,code,speaker,key,text,sourceFile,sourceLine,sourceHash"
    );
    let hello_row = lines.find(|l| l.contains(HELLO)).unwrap();
    assert!(
        hello_row.ends_with(&format!(",{}", hash("Hello there {friend}."))),
        "{hello_row}"
    );
    let prompt_row = text
        .split("\r\n")
        .find(|l| l.contains(PICK_PROMPT))
        .unwrap();
    assert!(prompt_row.starts_with("prompt,"), "{prompt_row}");
    assert!(prompt_row.ends_with(&hash("Where now?")), "{prompt_row}");
}

#[test]
fn import_writes_a_v2_bundle_with_source_hashes_byte_identically() {
    let dir = project("bundle-v2");
    let work = lute_scratch::ScratchDir::named("bundle-v2-work");
    let bundle = translate_and_import(&dir, &work, &[]);
    let first = std::fs::read_to_string(&bundle).unwrap();
    assert!(
        first.starts_with(
            "{\n  \"schemaVersion\": 2,\n  \"locales\": [\n    \"ko\"\n  ],\n  \"entries\": {"
        ),
        "{first}"
    );
    let value: Json = serde_json::from_str(&first).unwrap();
    let keys: Vec<&String> = value.as_object().unwrap().keys().collect();
    assert_eq!(keys.last().unwrap().as_str(), "sourceHashes");
    assert_eq!(
        value["sourceHashes"][HELLO]["ko"],
        hash("Hello there {friend}.")
    );
    assert_eq!(value["sourceHashes"][PICK_PROMPT]["ko"], hash("Where now?"));

    // A second import of the same rows is byte-identical.
    let again = work.join("again.json");
    let out = run(&[
        "loc",
        "import",
        work.join("ko.json").to_str().unwrap(),
        "-o",
        again.to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert_eq!(std::fs::read_to_string(&again).unwrap(), first);

    // Rows without hashes still write `sourceHashes: {}`.
    let bare = write(
        &work,
        "ja.json",
        &format!(r#"[{{"kind":"line","lineId":"{HELLO}","text":"やあ"}}]"#),
    );
    let out = run(&["loc", "import", bare.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.ends_with("  \"sourceHashes\": {}\n}\n"), "{text}");
}

#[test]
fn a_v1_bundle_is_still_read_with_no_stale_check() {
    let dir = project("bundle-v1");
    let work = lute_scratch::ScratchDir::named("bundle-v1-work");
    let bundle = write(
        &work,
        "bundle.json",
        r#"{"schemaVersion":1,"locales":["ko"],"entries":{"marina.s01ep01.pick.go":{"ko":"가자"}}}"#,
    );
    let out = compile_with(&dir, &bundle, &[]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert!(!stderr(&out).contains("W-L10N-STALE"), "{}", stderr(&out));
    let artifact: Json = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        command(&artifact, "choice")["options"][0]["texts"]["ko"],
        "가자"
    );

    // The same entry as v2 with a hash of another source text is stale.
    let v2 = write(
        &work,
        "bundle2.json",
        &format!(
            r#"{{"schemaVersion":2,"locales":["ko"],"entries":{{"marina.s01ep01.pick.go":{{"ko":"가자"}}}},"sourceHashes":{{"marina.s01ep01.pick.go":{{"ko":"{}"}}}}}}"#,
            hash("Go")
        ),
    );
    let out = compile_with(&dir, &v2, &[]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert!(stderr(&out).contains("W-L10N-STALE"), "{}", stderr(&out));
}

#[test]
fn no_stale_warning_right_after_export_import_compile() {
    let dir = project("fresh");
    let work = lute_scratch::ScratchDir::named("fresh-work");
    let bundle = translate_and_import(&dir, &work, &[]);
    // The hashes are stored — of the plain text, for the authored line and
    // the component line alike — and match the compiled records.
    let hashes = &read_json(&bundle)["sourceHashes"];
    assert_eq!(hashes[HELLO]["ko"], hash("Hello there {friend}."));
    assert_eq!(
        hashes["marina.s01ep01.greet#1.marina_0100"]["ko"],
        hash("Welcome aboard, [friend].")
    );
    let out = compile_with(&dir, &bundle, &[]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert!(out.stderr.is_empty(), "{}", stderr(&out));
    let all = work.join("all");
    let out = compile_all_with(&dir, &all, Some(&bundle), &["--deny-warnings"]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert!(!stderr(&out).contains("W-L10N-STALE"), "{}", stderr(&out));
}

#[test]
fn a_source_edit_after_export_is_w_l10n_stale_merged_and_deniable() {
    let dir = project("stale");
    let work = lute_scratch::ScratchDir::named("stale-work");
    let bundle = translate_and_import(&dir, &work, &[]);
    // Edit a line, an option label and a prompt after the export.
    let edited = SCENE
        .replace("Hello :speed[there]", "Hi :speed[there]")
        .replace("text=\"Go on\"", "text=\"Go ahead\"")
        .replace("prompt=\"Look where?\"", "prompt=\"Look at what?\"");
    write(&dir, "a.lute", &edited);

    let out = compile_with(&dir, &bundle, &[]);
    assert_eq!(out.status.code(), Some(0), "a warning: {}", stderr(&out));
    let err = stderr(&out);
    let hits: Vec<&str> = err.lines().filter(|l| l.contains("W-L10N-STALE")).collect();
    assert_eq!(hits.len(), 3, "{err}");
    for id in [HELLO, "marina.s01ep01.pick.go", LOOK_PROMPT] {
        assert!(
            hits.iter().any(|h| h.contains(&format!(
                "`ko` text for `{id}` was translated from an older source text; re-export with \
                 `lute loc export` and update the translation"
            ))),
            "{id}: {err}"
        );
    }
    // Still merged.
    let artifact: Json = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        command(&artifact, "hub")["promptTexts"]["ko"],
        "KO Look where?"
    );

    for flag in ["--deny=W-L10N-STALE", "--deny-warnings"] {
        let out = compile_with(&dir, &bundle, &[flag]);
        assert_eq!(out.status.code(), Some(1), "{flag}: {}", stderr(&out));
        assert!(out.stdout.is_empty(), "{flag}");
        assert!(stderr(&out).contains("error [W-L10N-STALE]"), "{flag}");
    }

    let all = work.join("all");
    let out = compile_all_with(&dir, &all, Some(&bundle), &[]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert_eq!(
        stderr(&out)
            .lines()
            .filter(|l| l.contains("W-L10N-STALE"))
            .count(),
        3,
        "{}",
        stderr(&out)
    );
    let denied = work.join("denied");
    let out = compile_all_with(&dir, &denied, Some(&bundle), &["--deny=W-L10N-STALE"]);
    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    assert!(!denied.join("project.index.json").exists());
}

// --- §3.1: `loc report` counts prompts ------------------------------------

#[test]
fn loc_report_counts_prompts_and_their_words() {
    let dir = project("report-prompts");
    let out = run(&[
        "loc",
        "report",
        "--project",
        dir.to_str().unwrap(),
        "--json",
    ]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let report: Json = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["schemaVersion"], "0.47.0.loc-report");
    let totals = &report["totals"];
    assert_eq!(totals["prompts"], 2, "{report:#}");
    assert_eq!(totals["choices"], 4, "{report:#}");
    // Lines: 2 (scene) + 1 (component) + 5 arm lines; their words plus the
    // prompts' "Where now?" (2) and "Look where?" (2).
    let line_words: u64 = totals["speakers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["words"].as_u64().unwrap())
        .sum();
    assert_eq!(
        totals["words"].as_u64().unwrap(),
        line_words + 4,
        "{report:#}"
    );
    assert_eq!(report["documents"][0]["prompts"], 2, "{report:#}");

    let human = run(&["loc", "report", "--project", dir.to_str().unwrap()]);
    assert_eq!(human.status.code(), Some(0), "{}", stderr(&human));
    let text = String::from_utf8(human.stdout).unwrap();
    assert!(
        text.lines().next().unwrap().ends_with("choices  prompts"),
        "{text}"
    );
    assert!(text.contains("4 choice(s), 2 prompt(s)"), "{text}");
}
