//! dsl 0.27.0 §7: entity-kind `labels:` render wherever a value of the kind
//! is interpolated — `{{occasion.target}}` in a kind beat and a
//! `{ domain: <kind> }` or `{ entity: <kind> }` state path — in `lute play`
//! and `lute trace` alike,
//! and `{{n:plural(one|other)}}` picks the English form (the IR placeholder
//! carries both forms). The checker rejects a malformed plural hint and a
//! label for an id the kind does not have.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{json, Value as Json};

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn temp_dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("lute-text027-{tag}-{}-{n}", std::process::id()));
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

/// `visit` is raised for `place.<room>`; `ward` is a sub-kind of `room`, and
/// the parent's labels reach it. `run.lamps` counts with a plural hint.
fn project(tag: &str, rooms: &str) -> PathBuf {
    let dir = temp_dir(tag);
    write(
        &dir,
        "lute.project.yaml",
        "pluginsDir: plugins/\ndefaultProfile: g\nprofiles:\n  g:\n    plugins: { g.ward: true }\n",
    );
    write(
        &dir,
        "plugins/g.ward/plugin.yaml",
        "id: g.ward\nversion: 0.1.0\nkind: capability\ndepends: [ { id: lute.core, range: \"^0.0.1\" } ]\n\
         exports:\n  occasions: occasions/\n",
    );
    write(
        &dir,
        "plugins/g.ward/occasions/o.yaml",
        "occasions:\n  visit: { target: { prefix: place, entity: room } }\n",
    );
    write(
        &dir,
        "world.schema.yaml",
        &format!(
            "state:\n  run.lamps: {{ type: number, default: 1 }}\n  \
             run.last: {{ type: {{ domain: room }}, default: chapel }}\n  \
             run.near: {{ type: {{ entity: room }}, default: chapel }}\n\
             entities:\n{rooms}"
        ),
    );
    write(
        &dir,
        "lore/rooms.lute",
        "---\nkind: lore\nid: rooms\nuses: ../world.schema.yaml\n---\n\n\
         <beat id=\"enter\" on=\"visit\" target=\"kind:ward\" once=\"false\">\n\
         \x20 @narrator: You step into {{occasion.target}}, past {{run.last}} and {{run.near}}.\n\
         \x20 ::set{run.lamps += 1}\n\
         \x20 @narrator: {{run.lamps:plural(# lamp is|# lamps are)}} lit.\n\
         </beat>\n",
    );
    dir
}

const ROOMS: &str = "  room:\n    members: [chapel]\n    labels: { chapel: the chapel, childrensWard: \"the children's ward\" }\n  \
                     ward:\n    subsetOf: room\n    members: [childrensWard]\n";

fn lines(step: &Json) -> Vec<&str> {
    step["presented"]["commands"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["kind"] == "line")
        .map(|r| r["text"].as_str().unwrap())
        .collect()
}

#[test]
fn play_renders_kind_labels_and_the_plural_form() {
    let dir = project("play", ROOMS);
    let script = write(
        &dir,
        "s.play.yaml",
        "steps:\n  - occasion: visit\n    target: place.childrensWard\n  \
         - occasion: visit\n    target: place.childrensWard\n",
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
    assert_eq!(
        lines(&v["steps"][0]),
        [
            "You step into the children's ward, past the chapel and the chapel.",
            "2 lamps are lit."
        ],
        "{v}"
    );
    assert_eq!(lines(&v["steps"][1])[1], "3 lamps are lit.", "{v}");
}

#[test]
fn trace_renders_what_play_renders() {
    let dir = project("trace", ROOMS);
    let out = run(&[
        "trace",
        dir.join("lore/rooms.lute").to_str().unwrap(),
        "--project",
        dir.to_str().unwrap(),
        "--beat",
        "enter",
        "--state",
        "occasion.target=childrensWard",
        "--state",
        "run.lamps=0",
    ]);
    let t = text(&out);
    assert!(
        t.contains("You step into the children's ward, past the chapel and the chapel."),
        "{t}"
    );
    assert!(t.contains("1 lamp is lit."), "{t}");
}

#[test]
fn the_artifact_carries_labels_and_plural_forms() {
    let dir = project("ir", ROOMS);
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
        &std::fs::read_to_string(out_dir.join("lore/rooms.lute.json")).unwrap(),
    )
    .unwrap();
    let kind = |name: &str| {
        art["entities"]
            .as_array()
            .unwrap()
            .iter()
            .find(|k| k["name"] == name)
            .cloned()
            .unwrap()
    };
    // The sub-kind takes its parent's label for its own member.
    assert_eq!(
        kind("ward")["labels"],
        json!({ "childrensWard": "the children's ward" })
    );
    assert_eq!(kind("room")["labels"]["chapel"], "the chapel");
    let state = art["state"].as_array().unwrap();
    let last = state.iter().find(|e| e["path"] == "run.last").unwrap();
    assert_eq!(last["labels"]["chapel"], "the chapel", "{last}");
    // `{ entity: K }` is the same type as `{ domain: K }` and carries the labels too.
    let near = state.iter().find(|e| e["path"] == "run.near").unwrap();
    assert_eq!(near["labels"]["chapel"], "the chapel", "{near}");
    let line = art["commands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["kind"] == "line" && c["text"].as_str().unwrap().contains("lit."))
        .unwrap();
    assert_eq!(
        line["placeholders"],
        json!([{ "kind": "path", "path": "run.lamps", "format": "plural", "forms": ["# lamp is", "# lamps are"] }]),
        "{line}"
    );
}

/// A component param typed by an entity kind or a named enum renders the
/// kind's label for its argument in `{{@p}}` (a sub-kind member takes its
/// parent's label); a `string` param bound to the same id renders the id.
#[test]
fn a_kind_typed_component_arg_renders_its_label() {
    let dir = project("component", ROOMS);
    write(
        &dir,
        "components/tour.component.lute",
        "---\ncomponent: tour\nparams:\n  place: { entity: ward }\n  hall: { domain: room }\n  \
         note: string\nuses: ../world.schema.yaml\n---\n\n## Tour\n\n\
         @narrator: From {{@hall}} into {{@place}} ({{@note}}).\n",
    );
    write(
        &dir,
        "scenes/tour.lute",
        "---\nkind: scene\nid: tour\nuses: ../world.schema.yaml\n\
         components: [../components/tour.component.lute]\n---\n\n## S\n\n\
         ::use{component=\"tour\" place=\"childrensWard\" hall=\"chapel\" note=\"chapel\"}\n",
    );
    let expected = "From the chapel into the children's ward (chapel).";
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
        &std::fs::read_to_string(out_dir.join("scenes/tour.lute.json")).unwrap(),
    )
    .unwrap();
    let texts: Vec<&str> = art["commands"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["kind"] == "line")
        .map(|c| c["text"].as_str().unwrap())
        .collect();
    assert_eq!(texts, [expected], "{art}");

    let out = run(&[
        "trace",
        dir.join("scenes/tour.lute").to_str().unwrap(),
        "--project",
        dir.to_str().unwrap(),
    ]);
    let t = text(&out);
    assert!(t.contains(expected), "{t}");
}

fn check_codes(dir: &Path) -> String {
    let out = run(&[
        "check",
        dir.join("lore/rooms.lute").to_str().unwrap(),
        "--project",
        dir.to_str().unwrap(),
    ]);
    text(&out)
}

#[test]
fn a_label_for_an_id_the_kind_lacks_is_refused() {
    let dir = project(
        "notmember",
        "  room:\n    members: [chapel]\n    labels: { chapl: the chapel }\n  \
         ward:\n    subsetOf: room\n    members: [childrensWard]\n",
    );
    let t = check_codes(&dir);
    assert!(t.contains("E-ENTITY-KIND-SHAPE"), "{t}");
    assert!(t.contains("did you mean `chapel`?"), "{t}");
}

#[test]
fn a_plural_hint_needs_two_forms() {
    let dir = project("forms", ROOMS);
    write(
        &dir,
        "lore/rooms.lute",
        "---\nkind: lore\nid: rooms\nuses: ../world.schema.yaml\n---\n\n\
         <entry id=\"note\">\n@narrator: {{run.lamps:plural(lamp)}} and {{run.last:plural(a|b)}}.\n</entry>\n",
    );
    let t = check_codes(&dir);
    assert!(
        t.contains("[E-PLURAL-FORM]") && t.contains("needs a singular and a plural form"),
        "{t}"
    );
    assert!(t.contains("[E-REF-TYPE]"), "a plural of a non-number: {t}");
}

/// A kind member that is also a cast id renders the kind's label in text —
/// `{{occasion.target}}` in play and trace alike — not the cast `name:`
/// (the speaker head of a line is where the cast name belongs), and
/// `check-project` has nothing to warn about.
#[test]
fn a_kind_label_wins_over_a_cast_name_in_text() {
    let dir = project(
        "castlabel",
        &format!(
            "{ROOMS}cast:\n  childrensWard: {{ name: Ward }}\n  chapel: {{ name: Old Chapel }}\n"
        ),
    );
    let out = run(&["check-project", dir.to_str().unwrap()]);
    let t = text(&out);
    assert_eq!(out.status.code(), Some(0), "{t}");
    assert!(!t.contains("warning ["), "{t}");
    let script = write(
        &dir,
        "s.play.yaml",
        "steps:\n  - occasion: visit\n    target: place.childrensWard\n",
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
    assert_eq!(
        lines(&v["steps"][0])[0],
        "You step into the children's ward, past the chapel and the chapel.",
        "{v}"
    );
    let out = run(&[
        "trace",
        dir.join("lore/rooms.lute").to_str().unwrap(),
        "--project",
        dir.to_str().unwrap(),
        "--beat",
        "enter",
        "--state",
        "occasion.target=childrensWard",
    ]);
    let t = text(&out);
    assert!(t.contains("You step into the children's ward,"), "{t}");
    // A kind-typed component argument renders the label too.
    write(
        &dir,
        "components/tour.component.lute",
        "---\ncomponent: tour\nparams:\n  place: { entity: ward }\n  hall: { domain: room }\n\
         uses: ../world.schema.yaml\n---\n\n## Tour\n\n@narrator: From {{@hall}} into {{@place}}.\n",
    );
    write(
        &dir,
        "scenes/tour.lute",
        "---\nkind: scene\nid: tour\nuses: ../world.schema.yaml\n\
         components: [../components/tour.component.lute]\n---\n\n## S\n\n\
         ::use{component=\"tour\" place=\"childrensWard\" hall=\"chapel\"}\n",
    );
    let out = run(&[
        "trace",
        dir.join("scenes/tour.lute").to_str().unwrap(),
        "--project",
        dir.to_str().unwrap(),
    ]);
    let t = text(&out);
    assert!(
        t.contains("From the chapel into the children's ward."),
        "{t}"
    );
}
