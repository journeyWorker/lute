//! Label forms and number words: a kind's `labels:` entry may declare
//! `{ text, start, indefinite }`, read by `{{…:start}}` / `{{…:indefinite}}`
//! (with fallbacks when a form is absent); `:capitalize` upper-cases any
//! text; `:cardinalWord` spells `one` … `twenty`; and `#word` / `#Word` in a
//! `plural(…)` form is the number as a word. `lute play` and `lute trace`
//! render the same text, and the IR carries the hints and the forms.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{json, Value as Json};

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn temp_dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("lute-text028-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(dir: &Path, rel: &str, text: &str) -> PathBuf {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, text).unwrap();
    p
}

fn run(args: &[&str]) -> Output {
    Command::new(BIN).args(args).output().unwrap()
}

fn text(o: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

/// `roam` is raised for `place.<spot>`: `cut` declares both forms, `ash`
/// none (the fallbacks), and `run.here` is typed by the kind.
fn project(tag: &str) -> PathBuf {
    let dir = temp_dir(tag);
    write(
        &dir,
        "lute.project.yaml",
        "pluginsDir: plugins/\ndefaultProfile: g\nprofiles:\n  g:\n    plugins: { g.roam: true }\n",
    );
    write(
        &dir,
        "plugins/g.roam/plugin.yaml",
        "id: g.roam\nversion: 0.1.0\nkind: capability\ndepends: [ { id: lute.core, range: \"^0.0.1\" } ]\n\
         exports:\n  occasions: occasions/\n",
    );
    write(
        &dir,
        "plugins/g.roam/occasions/o.yaml",
        "occasions:\n  roam: { target: { prefix: place, entity: spot } }\n",
    );
    write(
        &dir,
        "world.schema.yaml",
        "state:\n  run.wagons: { type: int, default: 11 }\n  \
         run.here: { type: { entity: spot }, default: cut }\n\
         entities:\n  spot:\n    members: [cut, ash]\n    labels:\n      \
         cut: { text: \"the smugglers' cut\", start: \"The smugglers' cut\", indefinite: \"a smugglers' cut\" }\n      \
         ash: ashwraith den\n",
    );
    write(
        &dir,
        "lore/roam.lute",
        "---\nkind: lore\nid: roam\nuses: ../world.schema.yaml\n---\n\n\
         <beat id=\"arrive\" on=\"roam\" target=\"kind:spot\" once=\"false\">\n\
         \x20 @narrator: {{occasion.target:start}} is quiet; you found {{occasion.target:indefinite}}.\n\
         \x20 @narrator: {{run.here:start}}, {{occasion.target:capitalize}}.\n\
         \x20 @narrator: {{run.wagons:plural(One wagon|#Word wagons)}} wait, {{run.wagons:cardinalWord}} in all.\n\
         </beat>\n",
    );
    dir
}

fn lines(step: &Json) -> Vec<&str> {
    step["presented"]["commands"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["kind"] == "line")
        .map(|r| r["text"].as_str().unwrap())
        .collect()
}

const CUT: [&str; 3] = [
    "The smugglers' cut is quiet; you found a smugglers' cut.",
    "The smugglers' cut, The smugglers' cut.",
    "Eleven wagons wait, eleven in all.",
];

const ASH: [&str; 3] = [
    "Ashwraith den is quiet; you found an ashwraith den.",
    "The smugglers' cut, Ashwraith den.",
    "Eleven wagons wait, eleven in all.",
];

#[test]
fn play_renders_label_forms_and_number_words() {
    let dir = project("play");
    let script = write(
        &dir,
        "s.play.yaml",
        "steps:\n  - occasion: roam\n    target: place.cut\n  \
         - occasion: roam\n    target: place.ash\n",
    );
    let out = run(&[
        "play",
        dir.to_str().unwrap(),
        "--script",
        script.to_str().unwrap(),
        "--json",
    ]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let v: Json = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(lines(&v["steps"][0]), CUT, "{v}");
    assert_eq!(lines(&v["steps"][1]), ASH, "{v}");
}

#[test]
fn trace_renders_what_play_renders() {
    let dir = project("trace");
    for (member, want) in [("cut", CUT), ("ash", ASH)] {
        let target = format!("occasion.target={member}");
        let out = run(&[
            "trace",
            dir.join("lore/roam.lute").to_str().unwrap(),
            "--project",
            dir.to_str().unwrap(),
            "--beat",
            "arrive",
            "--state",
            &target,
        ]);
        let t = text(&out);
        for line in want {
            assert!(t.contains(line), "{member}: {line}\n{t}");
        }
    }
}

#[test]
fn the_artifact_carries_the_hints_and_the_label_forms() {
    let dir = project("ir");
    let out_dir = dir.join("out");
    let out = run(&[
        "compile",
        "--all",
        "--project",
        dir.to_str().unwrap(),
        "-o",
        out_dir.to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let art: Json = serde_json::from_str(
        &std::fs::read_to_string(out_dir.join("lore/roam.lute.json")).unwrap(),
    )
    .unwrap();
    let spot = art["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|k| k["name"] == "spot")
        .unwrap();
    assert_eq!(spot["labels"]["cut"], "the smugglers' cut", "{spot}");
    assert_eq!(
        spot["labelForms"],
        json!({ "cut": { "start": "The smugglers' cut", "indefinite": "a smugglers' cut" } }),
        "{spot}"
    );
    let here = art["state"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["path"] == "run.here")
        .unwrap();
    assert_eq!(here["labelForms"], spot["labelForms"], "{here}");
    let placeholders: Vec<&Json> = art["commands"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["kind"] == "line")
        .flat_map(|c| c["placeholders"].as_array().unwrap())
        .collect();
    assert_eq!(
        placeholders,
        [
            &json!({ "kind": "occasionTarget", "entityKind": "spot", "format": "start" }),
            &json!({ "kind": "occasionTarget", "entityKind": "spot", "format": "indefinite" }),
            &json!({ "kind": "path", "path": "run.here", "format": "start" }),
            &json!({ "kind": "occasionTarget", "entityKind": "spot", "format": "capitalize" }),
            &json!({ "kind": "path", "path": "run.wagons", "format": "plural", "forms": ["One wagon", "#Word wagons"] }),
            &json!({ "kind": "path", "path": "run.wagons", "format": "cardinalWord" }),
        ]
    );
}

/// A label's forms are checked: an unknown key names the nearest form.
#[test]
fn a_misspelt_label_form_is_reported() {
    let dir = project("badform");
    write(
        &dir,
        "world.schema.yaml",
        "state:\n  run.wagons: { type: int, default: 11 }\n  \
         run.here: { type: { entity: spot }, default: cut }\n\
         entities:\n  spot:\n    members: [cut, ash]\n    labels:\n      \
         cut: { text: the cut, indefinte: a cut }\n",
    );
    let out = run(&["check-project", dir.to_str().unwrap()]);
    let t = text(&out);
    assert_ne!(out.status.code(), Some(0), "{t}");
    assert!(
        t.contains("E-ENTITY-KIND-SHAPE") && t.contains("did you mean `indefinite`?"),
        "{t}"
    );
}
