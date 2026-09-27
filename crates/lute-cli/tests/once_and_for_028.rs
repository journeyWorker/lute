//! A beat's `target` / `for` / `once` spellings, checked through the built
//! `lute`: a `target="kind:K"` on an occasion raised for no target points
//! at `for` and is one report; `once` values and quoted frontmatter
//! scalars name the fix; Yarn's `when: once` points at the `once` key.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn temp_dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("lute-of028-{tag}-{}-{n}", std::process::id()));
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
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

/// `lute check-project` over a project whose occasions are `evening`
/// (`select: sequence`), `morning` (`select: first`), both raised for no
/// target, and `talk` (raised for a hero), plus every `(path, text)`.
fn check(tag: &str, docs: &[(&str, &str)]) -> String {
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
        "occasions:\n  evening: { select: sequence }\n  morning: {}\n  \
         talk: { target: { prefix: hero, entity: hero } }\n",
    );
    write(
        &dir,
        "world.schema.yaml",
        "entities:\n  hero: { members: [aria, bram] }\n\
         relations:\n  birthday: { args: [hero], tier: run }\n\
         state:\n  run.done: { type: bool, default: false }\n",
    );
    for (rel, body) in docs {
        write(&dir, rel, body);
    }
    text(
        &Command::new(BIN)
            .arg("check-project")
            .arg(&dir)
            .output()
            .unwrap(),
    )
}

fn lore(body: &str) -> String {
    format!("---\nkind: lore\nid: g\nuses: ../world.schema.yaml\n---\n\n{body}")
}

fn scene(keys: &str) -> String {
    format!(
        "---\nkind: scene\nid: s\nuses: ../world.schema.yaml\n{keys}---\n\n## S\n\n@narrator: hi\n"
    )
}

#[test]
fn a_kind_target_on_an_untargeted_occasion_points_at_for_and_is_one_report() {
    let t = check(
        "target-kind",
        &[(
            "lore/g.lute",
            &lore(
                "<beat id=\"b\" on=\"evening\" target=\"kind:hero\" \
                 when=\"holds(birthday(occasion.target))\">\n  @narrator: Hi, {{occasion.target}}.\n</beat>\n",
            ),
        )],
    );
    assert!(
        t.contains(
            "[E-BEAT-ATTR] occasion `evening` is not raised for a target (declared without \
             `target: true`), so a beat on it cannot restrict itself to one; to present this \
             beat once for each member of `hero`, write `for=\"kind:hero\"` instead of `target`"
        ),
        "{t}"
    );
    // Its reads of `occasion.target` are not reported again.
    assert!(!t.contains("E-UNDECLARED"), "{t}");
    assert!(t.contains("(1 error(s)"), "{t}");

    // A scene says it in frontmatter spelling.
    let t = check(
        "target-kind-scene",
        &[(
            "scenes/s.lute",
            &scene("on: evening\ntarget: \"kind:hero\"\n"),
        )],
    );
    assert!(
        t.contains("write `for: \"kind:hero\"` instead of `target:`"),
        "{t}"
    );

    // An entry, whose `target=` is otherwise metadata there, says it too.
    let t = check(
        "target-kind-entry",
        &[(
            "lore/g.lute",
            &lore("<entry id=\"e\" on=\"evening\" target=\"kind:hero\">\n  @narrator: Hi.\n</entry>\n"),
        )],
    );
    assert!(
        t.contains(
            "to present this entry once for each member of `hero`, write `for=\"kind:hero\"` \
             instead of `target`"
        ),
        "{t}"
    );

    // On a `select: first` occasion `for` would be refused too: say why.
    let t = check(
        "target-kind-first",
        &[(
            "lore/g.lute",
            &lore(
                "<beat id=\"b\" on=\"morning\" target=\"kind:hero\">\n  @narrator: Hi.\n</beat>\n",
            ),
        )],
    );
    assert!(
        t.contains(
            "remove `target` (`for=\"kind:hero\"` presents a beat once for each member, but \
             only on a `select: sequence` occasion, and `morning` is `select: first`)"
        ),
        "{t}"
    );
}

#[test]
fn a_for_naming_a_kind_without_its_prefix_suggests_it() {
    let t = check(
        "for-bare",
        &[(
            "lore/g.lute",
            &lore("<beat id=\"b\" on=\"evening\" for=\"hero\">\n  @narrator: Hi.\n</beat>\n"),
        )],
    );
    assert!(
        t.contains("`for=\"hero\"` must name a kind, `for=\"kind:<kind>\"` — did you mean `for=\"kind:hero\"`?"),
        "{t}"
    );
    let t = check(
        "for-bare-scene",
        &[("scenes/s.lute", &scene("on: evening\nfor: heros\n"))],
    );
    assert!(
        t.contains("`for: \"heros\"` must name a kind, `for: \"kind:<kind>\"` — did you mean `for: \"kind:hero\"`?"),
        "{t}"
    );
}

#[test]
fn entry_once_false_is_no_once_and_a_bare_once_lists_its_values() {
    let t = check(
        "entry-once-false",
        &[(
            "lore/g.lute",
            &lore(
                "<entry id=\"e\" on=\"morning\" once=\"false\">\n  ::set{run.done = true}\n</entry>\n",
            ),
        )],
    );
    assert!(!t.contains("error"), "{t}");
    assert!(
        t.contains("[W-ENTRY-WRITE-REREAD] `<entry id=\"e\">` has no `once`"),
        "{t}"
    );

    for (tag, body, element) in [
        (
            "bare-entry",
            "<entry id=\"e\" on=\"morning\" once>\n  @narrator: Hi.\n</entry>\n",
            "entry",
        ),
        (
            "bare-beat",
            "<beat id=\"b\" on=\"morning\" once>\n  @narrator: Hi.\n</beat>\n",
            "beat",
        ),
    ] {
        let t = check(tag, &[("lore/g.lute", &lore(body))]);
        assert!(
            t.contains(&format!(
                "`<{element}>` attribute `once` must be a quoted string: `once=\"…\"` takes `run`"
            )),
            "{t}"
        );
    }
}

#[test]
fn a_quoted_frontmatter_scalar_says_write_it_unquoted() {
    let t = check(
        "quoted",
        &[("scenes/s.lute", &scene("on: morning\npriority: \"10\"\n"))],
    );
    assert!(
        t.contains("the quoted string `\"10\"` — write it unquoted, `10`"),
        "{t}"
    );
}

#[test]
fn yarn_when_once_points_at_the_once_key() {
    for (tag, keys) in [
        ("when-once", "on: morning\nwhen: once\n"),
        ("when-always", "on: morning\nwhen: always\n"),
    ] {
        let t = check(tag, &[("scenes/s.lute", &scene(keys))]);
        assert!(
            t.contains(
                "is not a condition: how often a beat plays is its own key, `once` — `once: run`"
            ),
            "{t}"
        );
    }
}
