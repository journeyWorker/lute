//! 0.41.1 §3.3 (#75): `W-DEF-UNUSED` on a project schema's def names the
//! `host: true` escape; on a document's own frontmatter def — where `host:`
//! is refused with `E-DEF-DECL` — it does not.

use std::path::Path;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn write(dir: &Path, rel: &str, text: &str) {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, text).unwrap();
}

fn stdout(args: &[&str], path: &Path) -> String {
    let out = Command::new(BIN).args(args).arg(path).output().unwrap();
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn schema_def_unused_names_the_host_escape() {
    let dir = lute_scratch::ScratchDir::named("host-guidance-041");
    write(
        &dir,
        "lute.project.yaml",
        "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\ndefaults:\n  uses: [world.schema.yaml]\n",
    );
    write(
        &dir,
        "world.schema.yaml",
        "state:\n  user.level: { type: int, default: 0 }\ndefs:\n  unused: \"user.level > 3\"\n",
    );
    write(
        &dir,
        "a.lute",
        "---\nkind: scene\nid: a\ndefs:\n  local: \"user.level > 1\"\n---\n## A\n@narrator: {{user.level}}\n",
    );
    let text = stdout(&["check", "--project"], &dir);
    let schema = dir.join("world.schema.yaml");
    assert!(
        text.contains(&format!(
            "{}:4:3: warning [W-DEF-UNUSED] def `unused` is declared but no `@unused` reference \
             uses it anywhere in the project (content, other defs, or rule guards); use it, drop \
             it, or mark it `host: true` if the host reads it (for example, `Program.def`)\n",
            schema.display()
        )),
        "{text}"
    );
    assert!(
        text.contains(
            "warning [W-DEF-UNUSED] def `local` is declared but no `@local` reference uses it \
             anywhere in the project (content, other defs, or rule guards); use it or drop it\n"
        ),
        "{text}"
    );
    assert_eq!(text.matches("host: true").count(), 1, "{text}");
}

#[test]
fn frontmatter_host_stays_a_def_declaration_error() {
    let dir = lute_scratch::ScratchDir::named("host-guidance-041-fm");
    write(
        &dir,
        "b.lute",
        "---\nkind: scene\nid: b\nstate:\n  user.level: { type: int, default: 0 }\ndefs:\n  \
         local: { cel: \"user.level > 1\", host: true }\n---\n## B\n@narrator: {{@local}}\n",
    );
    let text = stdout(&["check"], &dir.join("b.lute"));
    assert!(
        text.contains("[E-DEF-DECL] invalid def `local`: `host:` is not a def key"),
        "{text}"
    );
    assert!(!text.contains("W-DEF-UNUSED"), "{text}");
}
