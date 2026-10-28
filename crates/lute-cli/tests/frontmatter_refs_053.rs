//! 0.53.0 §6 (#226), the issue project through the built `lute`: a
//! misspelled entity in a `record`-typed plugin frontmatter key is reported
//! like a top-level one, and an entity kind only frontmatter keys are typed
//! by is read (no `W-DOMAIN-UNREAD`).
//!
//! On v0.52.0 `talk.partner: mirra` checks silently and `person` draws
//! `W-DOMAIN-UNREAD`.

use std::path::Path;
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_lute");

const PROJECT: &str = "pluginsDir: plugins/\ndefaultProfile: game\nprofiles:\n  game:\n    \
                       plugins:\n      acme.stage: true\ndefaults:\n  contentLang: en\n  uses: \
                       [schema/world.schema.yaml]\n";

const PLUGIN: &str = "id: acme.stage\nversion: 0.1.0\nkind: capability\ndepends:\n  - { id: \
                      lute.core, range: \"^0.0.1\" }\nexports:\n  frontmatter: frontmatter/\n";

const KEYS: &str = "frontmatter:\n  - { key: lead, schema: { entity: person } }\n  - { key: \
                    guests, schema: { list: { entity: person } } }\n  - key: talk\n    schema:\n      \
                    record:\n        - { name: partner, type: { entity: person } }\n        - { \
                    name: turns, type: int }\n";

const SCHEMA: &str = "entities:\n  person: { members: [mira, theo] }\ncast:\n  mira: { name: Mira \
                      }\n  theo: { name: Theo }\n";

fn write(dir: &Path, rel: &str, text: &str) {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, text).unwrap();
}

/// The issue's project, `keys` as the plugin's frontmatter keys and
/// `front` as the scene's plugin frontmatter.
fn project(tag: &str, keys: &str, front: &str) -> lute_scratch::ScratchDir {
    let dir = lute_scratch::ScratchDir::named(tag);
    write(&dir, "lute.project.yaml", PROJECT);
    write(&dir, "plugins/acme.stage/plugin.yaml", PLUGIN);
    write(&dir, "plugins/acme.stage/frontmatter/keys.yaml", keys);
    write(&dir, "schema/world.schema.yaml", SCHEMA);
    write(
        &dir,
        "scenes/intro.lute",
        &format!("---\nkind: scene\nid: intro\n{front}---\n\n## Hall {{#hall}}\n\n@mira: Hello.\n"),
    );
    dir
}

/// Every diagnostic line of `lute check --project .`, as `file:line:col
/// [CODE] message` (the severity dropped).
fn check(dir: &Path) -> Vec<String> {
    let out: Output = Command::new(BIN)
        .args(["check", "--project", "."])
        .current_dir(dir)
        .env("LUTE_TELEMETRY", "0")
        .output()
        .unwrap();
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
    .lines()
    .filter_map(|l| {
        let l = l.strip_prefix("./")?;
        let (at, rest) = l.split_once(": ")?;
        let (_, d) = rest.split_once(' ')?;
        d.starts_with('[').then(|| format!("{at} {d}"))
    })
    .collect()
}

/// #226: the three misspellings are reported, each at its key, the record
/// field named by its path; `person` is read by the frontmatter keys.
#[test]
fn the_issue_project_reports_the_record_field_and_reads_the_kind() {
    let dir = project(
        "fm-refs-053",
        KEYS,
        "lead: mirra\nguests: [mira, theoo]\ntalk: { partner: mirra, turns: 3 }\n",
    );
    assert_eq!(
        check(&dir),
        vec![
            "scenes/intro.lute:4:1 [E-BAD-ENUM] `mirra` is not a member of entity kind `person` \
             (attribute `lead` of `frontmatter`) — did you mean `mira`?",
            "scenes/intro.lute:5:1 [E-BAD-ENUM] `theoo` is not a member of entity kind `person` \
             (attribute `guests` of `frontmatter`) — did you mean `theo`?",
            "scenes/intro.lute:6:1 [E-BAD-ENUM] `mirra` is not a member of entity kind `person` \
             (field `partner` of `talk` of `frontmatter`) — did you mean `mira`?",
        ]
    );
}

/// 0.53.0 §6.2: a kind typed only by a nested record field is read, even
/// when no document authors the key; with no frontmatter key typed by it,
/// the warning names frontmatter keys among the readers it looked for.
#[test]
fn a_frontmatter_type_reads_its_kind() {
    let nested = "frontmatter:\n  - key: talk\n    schema:\n      record:\n        - { name: \
                  partner, type: { list: { entity: person } } }\n";
    let read = check(&project("fm-unread-053", nested, ""));
    assert!(
        !read.iter().any(|l| l.contains("W-DOMAIN-UNREAD")),
        "{read:#?}"
    );

    let none = "frontmatter:\n  - { key: turns, schema: int }\n";
    let unread = check(&project("fm-unread-none-053", none, ""));
    let warning: Vec<&String> = unread
        .iter()
        .filter(|l| l.contains("W-DOMAIN-UNREAD"))
        .collect();
    assert_eq!(warning.len(), 1, "{unread:#?}");
    assert!(
        warning[0].starts_with("schema/world.schema.yaml:2:3 [W-DOMAIN-UNREAD] domain `person`")
            && warning[0].contains("no plugin frontmatter key is typed by it"),
        "{unread:#?}"
    );
}
