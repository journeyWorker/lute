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

/// Round-3 CR F5: an unresolvable `components:` import names the project
/// file it probably meant, spelled from the importing document; a `::use` of
/// an undeclared component names the nearest declared one.
#[test]
fn unresolved_component_imports_suggest_the_project_file() {
    let schema = format!("{KINDS}{GIFTED}");
    let nested = "---\nkind: scene\nid: s\ncomponents: [gift.component.lute]\n---\n\n## S\n\n\
                  ::use{component=\"gift\" who=\"sefa\" item=\"shell\"}\n";
    let typo = "---\nkind: scene\nid: t\n---\n\n## T\n\n\
                ::use{component=\"gfit\" who=\"sefa\" item=\"shell\"}\n";
    let dir = project(
        "comp-path",
        &schema,
        &[("scenes/run/recap.lute", nested), ("scenes/t.lute", typo)],
    );
    let (_, t) = run(&dir, &["check-project", "."]);
    assert!(
        t.contains("cannot resolve `components:` import `gift.component.lute`")
            && t.contains("did you mean `../../gift.component.lute`?"),
        "{t}"
    );
    assert!(t.contains("unknown component `gfit`: not declared in `components:` — did you mean `gift`?"), "{t}");
}

/// A project from `(path, text)` files alone.
fn raw_project(tag: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lute-sk-raw-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for (rel, text) in files {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
    dir
}

const CORE: &str = "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\ndefaults:\n  uses: [w.schema.yaml]\n  components: [react.component.lute]\n";

/// ER N6: an effects component writes and reads a `per:` family member
/// chosen by its param — `run.approval[@who]` — bound at each `::use`, whose
/// argument must be a member of the family's kind.
#[test]
fn a_component_indexes_a_per_family_by_its_param() {
    let schema = "state:\n  run.approval: { type: number, default: 0, per: companion }\n\
                  entities:\n  companion: { members: [isolde, corvin] }\n\
                  cast:\n  isolde: { name: Isolde }\n  corvin: { name: Corvin }\n  oda: { name: Oda }\n";
    let comp = "---\ncomponent: react\neffects: true\nparams:\n  who: speaker\n---\n\n## R\n\n\
                ::set{run.approval[@who] = run.approval[@who] + 2}\n";
    let ok = "---\nkind: scene\nid: s\non: visit\n---\n\n## S\n\n::use{component=\"react\" who=\"isolde\"}\n\
              @narrator: {{run.approval.isolde}}\n";
    let dir = raw_project(
        "per-index",
        &[("lute.project.yaml", CORE), ("w.schema.yaml", schema), ("react.component.lute", comp), ("s.lute", ok)],
    );
    let (code, t) = run(&dir, &["check-project", "."]);
    assert_eq!(code, Some(0), "{t}");
    std::fs::write(
        dir.join("p.play.yaml"),
        "steps:\n  - occasion: visit\nexpect:\n  state: { run.approval.isolde: 2, run.approval.corvin: 0 }\n",
    )
    .unwrap();
    let (code, t) = run(&dir, &["play", ".", "--script", "p.play.yaml"]);
    assert_eq!(code, Some(0), "{t}");

    std::fs::write(
        dir.join("t.lute"),
        "---\nkind: scene\nid: t\n---\n\n## T\n\n::use{component=\"react\" who=\"oda\"}\n",
    )
    .unwrap();
    let (_, t) = run(&dir, &["check-project", "."]);
    assert!(
        t.contains("./t.lute:8:")
            && t.contains("`oda` is not a member of entity kind `companion` [isolde, corvin]"),
        "{t}"
    );
    assert!(!t.contains("E-CEL-PARSE"), "{t}");
}

/// Docs024: `::clear` takes no attributes — the timing keys other
/// directives accept are `E-UNKNOWN-ATTR` on it.
#[test]
fn clear_takes_no_timing_attributes() {
    let scene = "---\nkind: scene\nid: s\n---\n\n## S\n\n::clear{duration=\"0.5\" wait=\"true\"}\n@narrator: x\n";
    let dir = raw_project("clear-attrs", &[("s.lute", scene)]);
    let (code, t) = run(&dir, &["check", "s.lute"]);
    assert_ne!(code, Some(0), "{t}");
    assert!(t.contains("`::clear` has no attribute `duration`"), "{t}");
    assert!(t.contains("`::clear` has no attribute `wait`"), "{t}");
}

/// Docs024: a plugin directive lowered by the `clearStage` builtin hook
/// compiles and traces as `::clear` — a sprite exit, not a passthrough.
#[test]
fn a_plugin_builtin_hook_lowers_to_the_core_op() {
    let dir = raw_project(
        "builtin-hook",
        &[
            (
                "lute.project.yaml",
                "pluginsDir: plugins/\ndefaultProfile: game\nprofiles:\n  game:\n    plugins: { lute.core: true, demo.pack: true }\n",
            ),
            (
                "plugins/demo.pack/plugin.yaml",
                "id: demo.pack\nversion: 0.1.0\nkind: capability\ndepends:\n  - { id: lute.core, range: \"^0.0.1\" }\nexports:\n  directives: directives/\n",
            ),
            (
                "plugins/demo.pack/directives/d.yaml",
                "directives:\n  - name: wipe\n    layer: staging\n    attrs: []\n    lower: { kind: builtin, name: clearStage }\n",
            ),
            (
                "b.lute",
                "---\nkind: scene\nid: b\nenums:\n  action:\n    members: [fade-in-up, fade-out-down]\n    exits: [fade-out-down]\n  anchor:\n    members: [left, center, right]\n    default: center\n---\n\n## B\n\n::auto{character=\"corvin\" action=\"fade-in-up\"}\n@corvin: Hi.\n::wipe\n@narrator: Gone.\n",
            ),
        ],
    );
    let (code, t) = run(&dir, &["compile", "b.lute", "--project", "."]);
    assert_eq!(code, Some(0), "{t}");
    assert!(t.contains("\"exit\": true"), "{t}");
    assert!(!t.contains("\"kind\": \"plugin\""), "{t}");
    let (_, t) = run(&dir, &["trace", "b.lute", "--project", "."]);
    assert!(t.contains("<clear exit>"), "{t}");
}

/// Docs024: a cast `present:` reading a path the document does not declare
/// is `E-UNDECLARED`, not a `W-CAST-ABSENT` suggesting that same guard.
#[test]
fn a_cast_present_on_an_undeclared_path_is_an_error() {
    let dir = raw_project(
        "present-undeclared",
        &[
            ("lute.project.yaml", "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\ndefaults:\n  uses: [w.schema.yaml]\n"),
            ("w.schema.yaml", "cast:\n  isolde: { name: Isolde, present: \"run.withIsolde\" }\n"),
            ("u.lute", "---\nkind: scene\nid: u\n---\n\n## U\n\n@isolde: Hello.\n"),
        ],
    );
    let (_, t) = run(&dir, &["check-project", "."]);
    assert!(t.contains("E-UNDECLARED") && t.contains("reads `run.withIsolde`"), "{t}");
    assert!(!t.contains("W-CAST-ABSENT"), "{t}");
}
