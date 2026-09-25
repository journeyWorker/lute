//! dsl 0.24.0 §3 end to end through the built `lute` binary: a sub-kind used
//! as a rule-body predicate, `per:` state, and a rule reading
//! `run.approval[P]` — `check-project` is clean, `lute play` derives per
//! member, and `--explain` names the grounded instance.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn temp_dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("lute-party-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(dir: &Path, rel: &str, text: &str) {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, text).unwrap();
}

fn text(o: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))
}

const SCHEMA: &str = r#"state:
  run.approval: { type: number, default: 0, per: companion }
entities:
  person:    { members: [isolde, corvin, oda] }
  companion: { subsetOf: person, members: [isolde, corvin] }
relations:
  recruited: { args: [person], tier: run }
  inParty:   { args: [companion], derive: true }
  loyal:     { args: [companion], derive: true }
facts:
  - "recruited(isolde)"
  - "recruited(corvin)"
  - "recruited(oda)"
rules:
  - "inParty(P) :- companion(P), recruited(P)"
  - "loyal(P) :- inParty(P), cel(\"run.approval[P] >= 3\")"
"#;

const CAMP: &str = "---\nkind: scene\nid: camp.fire\nuses: ../world.schema.yaml\non: visit\n---\n\n## Camp\n\n\
                    ::set{run.approval.isolde += 3}\n\
                    @narrator{when=\"holds(loyal(isolde))\"}: Isolde keeps watch.\n";

fn project(tag: &str) -> PathBuf {
    let dir = temp_dir(tag);
    write(&dir, "lute.project.yaml", "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n");
    write(&dir, "world.schema.yaml", SCHEMA);
    write(&dir, "scenes/camp.lute", CAMP);
    dir
}

#[test]
fn a_sub_kind_predicate_and_an_indexed_rule_derive_per_member_in_play() {
    let dir = project("play");
    let out = Command::new(BIN).arg("check-project").arg(&dir).output().unwrap();
    assert!(out.status.success(), "{}", text(&out));

    write(
        &dir,
        "s.play.yaml",
        "steps:\n  - occasion: visit\nexpect:\n  state: { run.approval.isolde: 3, run.approval.corvin: 0 }\n  \
         facts: [inParty(isolde), inParty(corvin), loyal(isolde)]\n  notFacts: [inParty(oda), loyal(corvin)]\n",
    );
    let out = Command::new(BIN)
        .args(["play", &dir.display().to_string(), "--script"])
        .arg(dir.join("s.play.yaml"))
        .args(["--explain", "loyal(isolde)"])
        .output()
        .unwrap();
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    assert!(t.contains("Isolde keeps watch."), "{t}");
    assert!(t.contains("[P = isolde]"), "the explanation names the grounded instance: {t}");
}
