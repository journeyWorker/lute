//! Round-3 prerelease fixes, through the built `lute` binary: per-member
//! `default:` maps on a `per:` family (ER N1), `W-DOMAIN-UNREAD` counting
//! `per:` / `subsetOf:` / rule-body kind reads and anchoring at the schema
//! line (CR N1, ER N10), and component `@param`s in fact atoms (CR N5).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn project(tag: &str, schema: &str, docs: &[(&str, &str)]) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("lute-sk-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let write = |rel: &str, text: &str| {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    };
    write(
        "lute.project.yaml",
        "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\ndefaults:\n  uses: [world.schema.yaml]\n  components: [gift.component.lute]\n",
    );
    write("world.schema.yaml", schema);
    write(
        "gift.component.lute",
        "---\ncomponent: gift\neffects: true\nparams:\n  who: { enum: [sefa, quill] }\n  item: { enum: [compass, shell] }\n---\n\n## Gift\n\n::assert{gifted(@who, @item)}\n@narrator: A gift.\n",
    );
    for (rel, text) in docs {
        write(rel, text);
    }
    dir
}

fn run(dir: &Path, args: &[&str]) -> (Option<i32>, String) {
    let o: Output = Command::new(BIN).args(args).current_dir(dir).output().unwrap();
    (
        o.status.code(),
        format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr)),
    )
}

const KINDS: &str = "entities:\n  npc: { members: [sefa, quill, tavi] }\n  \
                     item: { members: [compass, shell] }\n";
const GIFTED: &str = "relations:\n  gifted: { args: [npc, item] }\n";

fn scene(body: &str) -> String {
    format!("---\nkind: scene\nid: s\non: visit\n---\n\n## S\n\n{body}")
}

/// ER N1: a map `default:` gives each member of a `per:` family its own
/// default, `_` the rest; run and play read them.
#[test]
fn per_member_defaults_reach_play() {
    let schema = format!(
        "state:\n  run.rep: {{ type: number, default: {{ _: 0, sefa: 2, tavi: -1 }}, per: npc }}\n{KINDS}{GIFTED}"
    );
    let body = scene(
        "@narrator: {{run.rep.sefa}} {{run.rep.quill}} {{run.rep.tavi}}.\n\
         @narrator{when=\"run.rep.tavi < 0\"}: Tavi is hostile.\n",
    );
    let dir = project("per-ok", &schema, &[("s.lute", &body)]);
    let (code, t) = run(&dir, &["check-project", "."]);
    assert_eq!(code, Some(0), "{t}");
    std::fs::write(dir.join("p.play.yaml"), "steps:\n  - occasion: visit\n").unwrap();
    let (_, t) = run(&dir, &["play", ".", "--script", "p.play.yaml"]);
    assert!(t.contains("@narrator: 2 0 -1."), "{t}");
    assert!(t.contains("Tavi is hostile."), "{t}");
}

#[test]
fn per_member_default_maps_are_checked() {
    let bad = format!(
        "state:\n  run.rep: {{ type: number, default: {{ sefa: 2, zed: 1, quill: x }}, per: npc }}\n  \
         run.flat: {{ type: number, default: {{ a: 1 }} }}\n{KINDS}{GIFTED}"
    );
    let dir = project("per-bad", &bad, &[("s.lute", &scene("@narrator: hi\n"))]);
    let (_, t) = run(&dir, &["check-project", "."]);
    assert!(t.contains("`zed`, which is not a member of entity kind `npc`"), "{t}");
    assert!(t.contains("gives `quill` the value"), "{t}");
    assert!(t.contains("gives no value for `tavi`"), "{t}");
    assert!(t.contains("`run.flat` has no `per:`"), "{t}");
}

/// CR N1 / ER N10: `per:`, `subsetOf:` and a rule-body kind atom are reads;
/// an unread kind is reported at its schema line, not `1:1` of an importer.
#[test]
fn kind_reads_and_schema_anchor() {
    let schema = format!(
        "state:\n  user.bond: {{ type: number, default: 0, per: bonded }}\n\
         entities:\n  npc: {{ members: [sefa, quill, tavi] }}\n  item: {{ members: [compass, shell] }}\n  \
         bonded: {{ subsetOf: npc, members: [sefa, quill] }}\n  confidant: {{ subsetOf: bonded, members: [sefa] }}\n  \
         unused: {{ members: [x] }}\n\
         {GIFTED}  trusts: {{ args: [npc], derive: true }}\n\
         rules:\n  - \"trusts(P) :- confidant(P), cel(\\\"user.bond[P] >= 3\\\")\"\n"
    );
    let body = scene("@narrator{when=\"holds(trusts(sefa))\"}: Trusted.\n@narrator: x\n");
    let dir = project("unread", &schema, &[("s.lute", &body)]);
    let (_, t) = run(&dir, &["check-project", "."]);
    let unread: Vec<&str> = t.lines().filter(|l| l.contains("W-DOMAIN-UNREAD")).collect();
    assert_eq!(unread.len(), 1, "{t}");
    assert!(unread[0].starts_with("./world.schema.yaml:8:3:"), "{t}");
    assert!(unread[0].contains("domain `unused`"), "{t}");
}

/// CR N5: an effects component passes its params into a fact atom, bound and
/// checked per `::use`; a non-constant argument is a clear `E-COMPONENT-ARG`.
#[test]
fn component_params_bind_into_fact_atoms() {
    let schema = format!("{KINDS}{GIFTED}");
    let ok = scene(
        "::use{component=\"gift\" who=\"sefa\" item=\"compass\"}\n\
         @narrator{when=\"holds(gifted(sefa, compass))\"}: Gifted.\n@narrator: x\n",
    );
    let dir = project("param-ok", &schema, &[("s.lute", &ok)]);
    let (code, t) = run(&dir, &["check-project", "."]);
    assert_eq!(code, Some(0), "{t}");
    std::fs::write(dir.join("p.play.yaml"), "steps:\n  - occasion: visit\n").unwrap();
    let (_, t) = run(&dir, &["play", ".", "--script", "p.play.yaml"]);
    assert!(t.contains("assert gifted(sefa, compass)") && t.contains("Gifted."), "{t}");

    let schema = format!("{KINDS}{GIFTED}defs:\n  pick: \"'shell'\"\n");
    let bad = scene(
        "::use{component=\"gift\" who=\"sefa\" item=@pick}\n\
         ::assert{gifted(@who, shell)}\n@narrator: x\n",
    );
    let dir = project("param-bad", &schema, &[("s.lute", &bad)]);
    let (_, t) = run(&dir, &["check-project", "."]);
    assert!(t.contains("binds `@item` in the fact atom `gifted(@who, @item)`"), "{t}");
    assert!(t.contains("argument 0 is `@who`, a component param"), "{t}");
    assert!(!t.contains("E-DATALOG-PARSE"), "{t}");
}
