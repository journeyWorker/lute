//! dsl 0.27.0 §3 (T2-1, T2-2, T2-10) at runtime: a `for="kind:K"` beat on a
//! `select: sequence` occasion is presented once per member whose `when`
//! holds, in member order, and spends as one beat; a raise's typed payload
//! is readable as `occasion.payload.<field>` for that raise only; `lute
//! test` / `lute trace` judge a kind beat reading `occasion.target` as a
//! fact argument and a family index from the mocked member.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value as Json;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn temp_dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("lute-ob027-{tag}-{}-{n}", std::process::id()));
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

fn run(dir: &Path, args: &[&str]) -> Output {
    Command::new(BIN)
        .current_dir(dir)
        .args(args)
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

/// `dailyReset` is an untargeted sequence; `summon` is raised for a hero and
/// carries `copies`. `g.bday` greets every hero with a birthday once a run;
/// `g.pull` reads the payload; `g.dupe` asks about the raised member.
fn project(tag: &str) -> PathBuf {
    let dir = temp_dir(tag);
    write(
        &dir,
        "lute.project.yaml",
        "pluginsDir: plugins/\ndefaultProfile: g\nprofiles:\n  g:\n    plugins: { g.x: true }\n",
    );
    write(
        &dir,
        "plugins/g.x/plugin.yaml",
        "id: g.x\nversion: 0.1.0\nkind: capability\ndepends: [ { id: lute.core, range: \"^0.0.1\" } ]\n\
         exports:\n  occasions: occasions/\n",
    );
    write(
        &dir,
        "plugins/g.x/occasions/o.yaml",
        "occasions:\n  dailyReset: { select: sequence }\n  \
         summon: { target: { prefix: hero, entity: hero }, payload: { copies: number } }\n",
    );
    write(
        &dir,
        "world.schema.yaml",
        "entities:\n  hero: { members: [aria, bram, cyra] }\n\
         relations:\n  owned: { args: [hero], tier: run, reserved: true }\n  birthday: { args: [hero], tier: run, reserved: true }\n\
         state:\n  user.bond: { type: number, default: 0, per: hero, owner: engine }\n  \
         run.total: { type: number, default: 0 }\n",
    );
    write(
        &dir,
        "lore/g.lute",
        "---\nkind: lore\nid: g\nuses: ../world.schema.yaml\n---\n\n\
         <beat id=\"bday\" on=\"dailyReset\" for=\"kind:hero\" once=\"run\" when=\"holds(birthday(occasion.target))\">\n\
         \x20 @narrator: Happy birthday, {{occasion.target}}!\n</beat>\n\n\
         <beat id=\"pull\" on=\"summon\" target=\"kind:hero\" once=\"false\" when=\"occasion.payload.copies >= 1\">\n\
         \x20 @narrator: {{occasion.payload.copies}} copies of {{occasion.target}}.\n\
         \x20 ::set{run.total += occasion.payload.copies}\n</beat>\n\n\
         <beat id=\"dupe\" on=\"summon\" target=\"kind:hero\" once=\"false\" priority=\"5\" \
         when=\"holds(owned(occasion.target)) && user.bond[occasion.target] >= 2\">\n\
         \x20 @narrator: {{occasion.target}} again, and close.\n</beat>\n",
    );
    let out = run(&dir, &["check-project", "."]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    dir
}

fn play(dir: &Path, script: &str) -> (Option<i32>, Json, String) {
    write(dir, "s.play.yaml", script);
    let out = run(dir, &["play", ".", "--script", "s.play.yaml", "--json"]);
    let v = serde_json::from_slice(&out.stdout).unwrap_or(Json::Null);
    (out.status.code(), v, text(&out))
}

/// Every line a step presented: its first presentation, then each `then`.
fn step_lines(step: &Json) -> Vec<String> {
    std::iter::once(&step["presented"])
        .chain(step["then"].as_array().into_iter().flatten())
        .flat_map(|p| p["commands"].as_array().into_iter().flatten())
        .filter(|c| c["kind"] == "line")
        .map(|c| c["text"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn a_for_beat_is_presented_per_member_in_member_order_and_spends_once() {
    let dir = project("for");
    let (code, v, t) = play(
        &dir,
        "facts: [birthday(cyra), birthday(aria)]\nsteps:\n  - occasion: dailyReset\n  - occasion: dailyReset\n",
    );
    assert_eq!(code, Some(0), "{t}");
    let first = &v["steps"][0];
    // One candidate per member, member order, each naming its member.
    let cands: Vec<(&str, &str, bool)> = first["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            (
                c["id"].as_str().unwrap(),
                c["for"].as_str().unwrap(),
                c["eligible"] == true,
            )
        })
        .collect();
    assert_eq!(
        cands,
        [
            ("g.bday", "aria", true),
            ("g.bday", "bram", false),
            ("g.bday", "cyra", true)
        ],
        "{first}"
    );
    // Presented for each member whose `when` holds, member order (the facts
    // were seeded cyra first).
    assert_eq!(
        step_lines(first),
        ["Happy birthday, aria!", "Happy birthday, cyra!"],
        "{first}"
    );
    // `once: run` spends the beat, not one member: the next raise presents
    // nobody.
    let second = &v["steps"][1];
    assert!(step_lines(second).is_empty(), "{second}");
    assert!(
        second["candidates"]
            .as_array()
            .unwrap()
            .iter()
            .all(|c| c["eligible"] == false),
        "{second}"
    );
}

#[test]
fn a_payload_is_bound_for_its_raise_only() {
    let dir = project("payload");
    let (code, v, t) = play(
        &dir,
        "steps:\n  - occasion: summon\n    target: hero.bram\n    payload: { copies: 2 }\n",
    );
    assert_eq!(code, Some(0), "{t}");
    let step = &v["steps"][0];
    assert_eq!(step_lines(step), ["2 copies of bram."], "{step}");
    assert_eq!(step["presented"]["stateDelta"]["run.total"], 2, "{step}");

    // The next raise carries no payload: the previous raise's copies are not
    // reused — a raise that leaves out a declared field is a usage error at
    // its step, naming the field.
    let (code, _, t) = play(
        &dir,
        "steps:\n  - occasion: summon\n    target: hero.bram\n    payload: { copies: 2 }\n  \
         - occasion: summon\n    target: hero.bram\n",
    );
    assert_eq!(code, Some(2), "{t}");
    assert!(t.contains("s.play.yaml:5:"), "{t}");
    assert!(t.contains("`copies`") && t.contains("missing"), "{t}");
}

#[test]
fn an_undeclared_payload_field_is_a_usage_error_naming_the_declared_ones() {
    let dir = project("badfield");
    let (code, _, t) = play(
        &dir,
        "steps:\n  - occasion: summon\n    target: hero.bram\n    payload: { copy: 2 }\n",
    );
    assert_eq!(code, Some(2), "{t}");
    assert!(t.contains("s.play.yaml:4:"), "{t}");
    assert!(t.contains("`copy`") && t.contains("`copies`"), "{t}");
    // An occasion that declares no payload refuses one.
    let (code, _, t) = play(
        &dir,
        "steps:\n  - occasion: dailyReset\n    payload: { copies: 1 }\n",
    );
    assert_eq!(code, Some(2), "{t}");
    assert!(t.contains("declares no `payload:`"), "{t}");
}

#[test]
fn test_and_trace_judge_a_kind_beat_by_the_mocked_member() {
    let dir = project("trace");
    write(
        &dir,
        "tests/dupe.test.yaml",
        "file: ../lore/g.lute\nbeat: dupe\nstate: { occasion.target: bram, user.bond.bram: 3 }\n\
         facts: [owned(bram)]\nexpect:\n  transcriptContains: [\"bram again, and close.\"]\n",
    );
    // Another member owned: the fact argument is the mocked member, not any.
    write(
        &dir,
        "tests/other.test.yaml",
        "file: ../lore/g.lute\nbeat: dupe\nstate: { occasion.target: bram, user.bond.bram: 3 }\n\
         facts: [owned(aria)]\nexpect:\n  eligible: false\n",
    );
    // The family slot read is the mocked member's.
    write(
        &dir,
        "tests/bond.test.yaml",
        "file: ../lore/g.lute\nbeat: dupe\nstate: { occasion.target: bram, user.bond.aria: 3, user.bond.bram: 1 }\n\
         facts: [owned(bram)]\nexpect:\n  eligible: false\n",
    );
    write(
        &dir,
        "tests/pull.test.yaml",
        "file: ../lore/g.lute\nbeat: pull\nstate: { occasion.target: cyra, occasion.payload.copies: 3, run.total: 1 }\n\
         expect:\n  transcriptContains: [\"3 copies of cyra.\"]\n  state: { run.total: 4 }\n",
    );
    let out = run(&dir, &["test", "tests", "--project", "."]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    assert!(text(&out).contains("4 passed, 0 failed"), "{}", text(&out));

    // Without the member, trace stops and names the mock once, with the
    // kind's members.
    let out = run(
        &dir,
        &[
            "trace",
            "lore/g.lute",
            "--project",
            ".",
            "--beat",
            "dupe",
            "--state",
            "user.bond.bram=3",
            "--fact",
            "owned(bram)",
        ],
    );
    let t = text(&out);
    assert_eq!(out.status.code(), Some(3), "{t}");
    assert_eq!(
        t.matches("--state occasion.target=<aria|bram|cyra>")
            .count(),
        1,
        "{t}"
    );
}

/// `lute trace --occasion O@<target>` binds `occasion.target` for a kind
/// beat answering O, as the engine's raise does: its writes and text name
/// the member, prefixed or bare. A raise outside O's domain is refused, and
/// so is one for another target than a fixed-target beat answers; with no
/// raise, a write or a `{{occasion.target}}` line halts incomplete instead
/// of printing the marker raw or passing as an empty `match`.
#[test]
fn trace_binds_the_member_a_raise_names() {
    let dir = temp_dir("raise-binds");
    write(
        &dir,
        "lute.project.yaml",
        "pluginsDir: plugins/\ndefaultProfile: g\nprofiles: { g: { plugins: { p: true } } }\n\
         defaults: { luteVersion: \"0.29.0\", uses: [w.schema.yaml] }\n",
    );
    write(
        &dir,
        "plugins/p/plugin.yaml",
        "id: p\nversion: 0.1.0\nkind: capability\ndepends: [ { id: lute.core, range: \"^0.0.1\" } ]\n\
         exports: { occasions: occasions/ }\n",
    );
    write(
        &dir,
        "plugins/p/occasions/o.yaml",
        "occasions:\n  landed: { select: first, target: { prefix: fish, entity: fish } }\n",
    );
    write(
        &dir,
        "w.schema.yaml",
        "entities:\n  fish: { members: [cod, marlin] }\nrelations:\n  caught: { args: [fish], tier: run }\n",
    );
    write(
        &dir,
        "lore/catch.lute",
        "---\nkind: lore\nid: catch\n---\n\n<beat id=\"land\" on=\"landed\" target=\"kind:fish\" once=\"false\">\n  \
         @narrator: You land a {{occasion.target}}.\n  ::assert{caught(occasion.target)}\n</beat>\n\n\
         <beat id=\"big\" on=\"landed\" target=\"fish.marlin\" priority=\"5\">\n  @narrator: A marlin!\n</beat>\n",
    );
    let trace = |beat: &str, extra: &[&str]| {
        let mut args = vec!["trace", "lore/catch.lute", "--project", ".", "--beat", beat];
        args.extend_from_slice(extra);
        let out = run(&dir, &args);
        (out.status.code(), text(&out))
    };

    let (code, t) = trace("land", &["--occasion", "landed@fish.cod"]);
    assert_eq!(code, Some(0), "{t}");
    assert!(t.contains("You land a cod."), "{t}");
    assert!(t.contains("caught(cod)"), "{t}");
    assert!(!t.contains("judged by no `<objective on>`"), "{t}");
    let (code, t) = trace("land", &["--occasion", "landed@cod"]);
    assert_eq!(code, Some(0), "{t}");
    assert!(t.contains("caught(cod)"), "{t}");

    let (code, t) = trace("land", &["--occasion", "landed@fish.tuna"]);
    assert_eq!(code, Some(1), "{t}");
    assert!(
        t.contains("the raise `landed@fish.tuna` is never made: target `fish.tuna` is outside"),
        "{t}"
    );

    let (code, t) = trace("big", &["--occasion", "landed@fish.cod"]);
    assert_eq!(code, Some(1), "{t}");
    assert!(
        t.contains("the beat `big` answers `landed` only for `fish.marlin`"),
        "{t}"
    );
    let (code, t) = trace("big", &["--occasion", "landed@marlin"]);
    assert_eq!(code, Some(0), "{t}");
    assert!(t.contains("A marlin!"), "{t}");

    let (code, t) = trace("land", &[]);
    assert_eq!(code, Some(3), "{t}");
    assert!(!t.contains("{{occasion.target}}"), "{t}");
    assert!(!t.contains("match ``"), "{t}");
}

/// Every `forKind` object in an artifact, depth-first.
fn for_kinds(v: &Json, out: &mut Vec<Json>) {
    match v {
        Json::Object(m) => {
            if let Some(fk) = m.get("forKind") {
                out.push(fk.clone());
            }
            m.values().for_each(|x| for_kinds(x, out));
        }
        Json::Array(a) => a.iter().for_each(|x| for_kinds(x, out)),
        _ => {}
    }
}

/// G-8: a union kind (`hero: { members: [] }`) lists its sub-kinds' members
/// in the order the sub-kinds are declared, not by sub-kind name — the
/// order `for=` presents in, the artifact's `forKind.members`, and trace's
/// mock hint.
#[test]
fn a_union_kind_orders_its_members_as_the_schema_declares_them() {
    let dir = project("union");
    write(
        &dir,
        "world.schema.yaml",
        "entities:\n  hero: { members: [] }\n  ssr: { subsetOf: hero, members: [aria, cyra] }\n  \
         sr: { subsetOf: hero, members: [bram] }\n  limited: { subsetOf: ssr, members: [cyra] }\n\
         relations:\n  owned: { args: [hero], tier: run, reserved: true }\n  birthday: { args: [hero], tier: run, reserved: true }\n\
         state:\n  user.bond: { type: number, default: 0, per: hero, owner: engine }\n  \
         run.total: { type: number, default: 0 }\n",
    );
    let (code, v, t) = play(
        &dir,
        "facts: [birthday(bram), birthday(cyra), birthday(aria)]\nsteps:\n  - occasion: dailyReset\n",
    );
    assert_eq!(code, Some(0), "{t}");
    assert_eq!(
        step_lines(&v["steps"][0]),
        [
            "Happy birthday, aria!",
            "Happy birthday, cyra!",
            "Happy birthday, bram!"
        ],
        "{t}"
    );

    let out = run(&dir, &["compile", "lore/g.lute", "--project", "."]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let art: Json = serde_json::from_slice(&out.stdout).unwrap();
    let mut found = Vec::new();
    for_kinds(&art, &mut found);
    assert!(!found.is_empty(), "{art}");
    for fk in &found {
        assert_eq!(
            fk["members"],
            serde_json::json!(["aria", "cyra", "bram"]),
            "{fk}"
        );
    }
    let out = run(
        &dir,
        &[
            "trace",
            "lore/g.lute",
            "--project",
            ".",
            "--beat",
            "dupe",
            "--state",
            "user.bond.bram=3",
            "--fact",
            "owned(bram)",
        ],
    );
    let t = text(&out);
    assert!(t.contains("occasion.target=<aria|cyra|bram>"), "{t}");
}
