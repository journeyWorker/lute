//! dsl 0.28.0 §3: `occasion.target` can be written — `::set{F[occasion.target]
//! …}`, `::assert{r(occasion.target)}`, a member-typed directive attribute and
//! a component argument — in a beat that targets a kind or runs for each
//! member of one. The checker judges each write per member; `lute play`
//! writes the member the beat ran for. Outside such a beat the write is one
//! scope error.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn temp_dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("lute-tw028-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(dir: &Path, rel: &str, text: &str) {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, text).unwrap();
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

/// `landed` is raised for a fish; `camp` is an untargeted sequence. `::haul`
/// asserts `caught(@fish)`; `reaction` speaks as `who` and bumps `mate`.
fn project(tag: &str, lore: &str) -> PathBuf {
    let dir = temp_dir(tag);
    write(
        &dir,
        "lute.project.yaml",
        "pluginsDir: plugins/\ndefaultProfile: g\nprofiles: { g: { plugins: { p: true } } }\n\
         defaults:\n  uses: [w.schema.yaml]\n  components: [components/reaction.component.lute]\n",
    );
    write(
        &dir,
        "plugins/p/plugin.yaml",
        "id: p\nversion: 0.1.0\nkind: capability\ndepends: [ { id: lute.core, range: \"^0.0.1\" } ]\n\
         exports: { occasions: occasions/, directives: directives/ }\n",
    );
    write(
        &dir,
        "plugins/p/occasions/o.yaml",
        "occasions:\n  landed: { select: first, target: { prefix: fish, entity: fish } }\n  \
         camp: { select: sequence }\n",
    );
    write(
        &dir,
        "plugins/p/directives/d.yaml",
        "directives:\n  - name: haul\n    attrs:\n      - { name: fish, required: true, type: { entity: fish } }\n    \
         effects:\n      asserts: [\"hauled(@fish)\"]\n",
    );
    write(
        &dir,
        "w.schema.yaml",
        "entities:\n  fish: { members: [cod, eel] }\n  pal: { members: [ren, kai] }\n\
         state:\n  run.count: { type: number, default: 0, per: fish }\n  \
         run.bond: { type: number, default: 0, per: pal }\n\
         relations:\n  caught: { args: [fish], tier: run }\n  hauled: { args: [fish], tier: run }\n  \
         met: { args: [pal], tier: run }\n\
         cast:\n  ren: { name: Ren }\n  kai: { name: Kai }\n",
    );
    write(
        &dir,
        "components/reaction.component.lute",
        "---\ncomponent: reaction\nparams:\n  who: speaker\n  mate: { entity: pal }\neffects: true\n---\n\n\
         ## Reaction\n\n@@who: Count me in.\n::set{run.bond[@mate] += 1}\n",
    );
    write(
        &dir,
        "lore/a.lute",
        &format!("---\nkind: lore\nid: a\n---\n\n{lore}"),
    );
    dir
}

const WRITES: &str = "<beat id=\"land\" on=\"landed\" target=\"kind:fish\" once=\"false\">\n  \
    @narrator: {{occasion.target}}.\n  \
    ::set{run.count[occasion.target] += 1}\n  \
    ::assert{caught(occasion.target)}\n  \
    ::haul{fish=occasion.target}\n\
    </beat>\n";

#[test]
fn a_kind_beat_writes_only_the_member_it_ran_for() {
    let dir = project("writes", WRITES);
    let check = run(&dir, &["check-project", "."]);
    assert!(check.status.success(), "{}", text(&check));
    write(
        &dir,
        "plays/p.play.yaml",
        "steps:\n  - occasion: landed\n    target: fish.cod\n  - occasion: landed\n    target: fish.cod\n\
         expect:\n  state: { run.count.cod: 2, run.count.eel: 0 }\n  facts: [caught(cod), hauled(cod)]\n",
    );
    let play = run(&dir, &["play", ".", "--script", "plays/p.play.yaml"]);
    let out = text(&play);
    assert!(play.status.success(), "{out}");
    assert!(out.contains("set run.count.cod = 2"), "{out}");
    assert!(!out.contains("eel"), "{out}");
}

/// A fact asserted through `occasion.target` — by `::assert` or a directive's
/// declared effect — may hold for every member of the kind, so a condition
/// reading it for one member is not impossible.
#[test]
fn a_fact_written_through_the_target_is_producible_for_every_member() {
    let lore = format!(
        "{WRITES}\n<beat id=\"brag\" on=\"camp\" once=\"false\" \
         when=\"holds(caught(eel)) && holds(hauled(cod))\">\n  @narrator: Brag.\n</beat>\n"
    );
    let dir = project("producer", &lore);
    let check = run(&dir, &["check-project", "."]);
    let out = text(&check);
    assert!(check.status.success(), "{out}");
    assert!(!out.contains("E-BEAT-UNREACHABLE"), "{out}");
}

#[test]
fn a_component_argument_passes_the_member_it_ran_for() {
    let lore = "<beat id=\"join\" on=\"camp\" for=\"kind:pal\" once=\"false\" when=\"!holds(met(occasion.target))\">\n  \
        ::assert{met(occasion.target)}\n  \
        ::use{component=\"reaction\" who=occasion.target mate=occasion.target}\n\
        </beat>\n";
    let dir = project("component", lore);
    let check = run(&dir, &["check-project", "."]);
    assert!(check.status.success(), "{}", text(&check));
    write(
        &dir,
        "plays/p.play.yaml",
        "facts: [met(ren)]\nsteps:\n  - occasion: camp\n\
         expect:\n  state: { run.bond.kai: 1, run.bond.ren: 0 }\n",
    );
    let play = run(&dir, &["play", ".", "--script", "plays/p.play.yaml"]);
    let out = text(&play);
    assert!(play.status.success(), "{out}");
    assert!(out.contains("@kai: Count me in."), "{out}");
    assert!(!out.contains("@ren:"), "{out}");
}

#[test]
fn a_member_outside_the_argument_domain_is_judged_like_that_literal() {
    // `fish` members are no `pal`s: the argument fits no member, and each
    // report names `occasion.target` with the members, as a literal `cod`
    // would be reported.
    let lore = "<beat id=\"land\" on=\"landed\" target=\"kind:fish\" once=\"false\">\n  \
        ::use{component=\"reaction\" who=narrator mate=occasion.target}\n\
        </beat>\n";
    let dir = project("domain", lore);
    let out = text(&run(&dir, &["check-project", "."]));
    let errors: Vec<&str> = out.lines().filter(|l| l.contains(": error [")).collect();
    assert!(!errors.is_empty(), "{out}");
    for e in &errors {
        assert!(
            e.contains("`occasion.target` is not a member of entity kind `pal`"),
            "{out}"
        );
        assert!(e.contains("every member `cod`, `eel`"), "{out}");
    }
}

#[test]
fn a_write_outside_a_kind_beat_is_one_scope_error() {
    let lore = "<beat id=\"stray\" on=\"camp\" once=\"false\">\n  \
        ::set{run.count[occasion.target] += 1}\n\
        </beat>\n";
    let dir = project("scope", lore);
    let out = text(&run(&dir, &["check-project", "."]));
    let errors: Vec<&str> = out.lines().filter(|l| l.contains(": error [")).collect();
    assert_eq!(errors.len(), 1, "{out}");
    assert!(errors[0].contains("[E-UNDECLARED]"), "{out}");
    assert!(errors[0].contains("targets a kind"), "{out}");
    assert!(!out.contains("did you mean"), "{out}");
}
