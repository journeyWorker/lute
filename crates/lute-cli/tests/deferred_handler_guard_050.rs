//! #203 (0.50.0 §5.4): a `judge: before` occasion judges a quest's
//! `questComplete` / `questFailed` handler guards when the quest settles and
//! runs the selected bodies after the occasion's beats (and after other
//! quests' grants and handlers, and the cadence's season openings). A fact
//! the guard's `holds(F)` requires is therefore Must in the handler body only
//! when nothing can remove it in between: no write anywhere in the project
//! retracts or displaces it, it is not derived or engine-owned, and its
//! relation is not `tier: season:<name>`.
//!
//! On 0.49.1 the repro below is a false `E-ARM-DEAD` (`lute play` takes the
//! arm the checker calls dead).

use std::path::Path;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

const PROJECT: &str = "pluginsDir: plugins/\ndefaultProfile: game\nprofiles:\n  game:\n    \
                       plugins:\n      acme: true\ndefaults:\n  contentLang: en\n  uses: \
                       [schema/world.schema.yaml]\n";

const PLUGIN: &str = "id: acme\nversion: 0.1.0\nkind: capability\ndepends:\n  - { id: \
                      lute.core, range: \"^0.0.1\" }\nexports:\n  occasions: occasions/\n";

const OCCASIONS: &str = "occasions:\n  chapterEnd: { select: first, judge: before }\n";

const SCHEMA: &str = "entities:\n  person: { members: [mira, theo] }\nrelations:\n  met: { \
                      args: [person], tier: user }\nstate:\n  run.other: { type: bool, default: \
                      false }\ncast:\n  mira: { name: Mira }\n";

/// A producer of `met(mira)`.
const MEET: &str = "---\nkind: scene\nid: meetMira\n---\n\n## Shot 1.\n\n@mira: Hi.\n\
                    ::assert{met(mira)}\n";

/// A `chapterEnd` beat retracting `met(mira)`: it runs after the quest's
/// handler guard is judged and before the handler body.
const PART: &str = "---\nkind: scene\nid: partWays\non: chapterEnd\n---\n\n## Shot 1.\n\n\
                    @mira: Bye.\n::retract{met(mira)}\n";

/// Quest `a`, completing on `chapterEnd`, whose `questComplete` handler is
/// guarded by `holds('met', ['mira'])` and holds a `<match>` testing `test`.
fn quest(test: &str) -> String {
    format!(
        "---\nkind: quest\nid: acme.quests\n---\n\n\
         <quest id=\"a\" tier=\"user\" title=\"A\" start=\"true\">\n  \
         <objective id=\"c1\" title=\"End\" on=\"chapterEnd\" done=\"true\"/>\n  \
         <on event=\"questComplete\" when=\"holds('met', ['mira'])\">\n    <match>\n      \
         <when test=\"{test}\">\n        ::set{{run.other=true}}\n      </when>\n      \
         <otherwise>\n        ::set{{run.other=false}}\n      </otherwise>\n    </match>\n  \
         </on>\n</quest>\n"
    )
}

fn write(dir: &Path, rel: &str, text: &str) {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, text).unwrap();
}

/// The #203 project: `schema` as its schema, `quest(test)` as its quest
/// document, `MEET`, and `scenes` (`(file, text)`) as further scenes.
fn project(
    tag: &str,
    schema: &str,
    test: &str,
    scenes: &[(&str, &str)],
) -> lute_scratch::ScratchDir {
    let dir = lute_scratch::ScratchDir::named(tag);
    write(&dir, "lute.project.yaml", PROJECT);
    write(&dir, "plugins/acme/plugin.yaml", PLUGIN);
    write(&dir, "plugins/acme/occasions/o.yaml", OCCASIONS);
    write(&dir, "schema/world.schema.yaml", schema);
    write(&dir, "scenes/meet.lute", MEET);
    write(&dir, "quests/q.lute", &quest(test));
    for (rel, text) in scenes {
        write(&dir, rel, text);
    }
    dir
}

/// Every diagnostic line of `lute check --project .`, as `file:line:col
/// [CODE] message` (the severity dropped).
fn check(dir: &Path) -> Vec<String> {
    let out = Command::new(BIN)
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

/// The #203 repro: a `chapterEnd` beat retracts the guard's fact, so the
/// handler's `!holds` arm is live and its `holds` test is not guaranteed.
#[test]
fn a_deferred_handler_guard_gives_no_fact_a_beat_retracts() {
    for (tag, test) in [
        ("dg-negated", "!holds('met', ['mira'])"),
        ("dg-plain", "holds('met', ['mira'])"),
    ] {
        let dir = project(tag, SCHEMA, test, &[("scenes/part.lute", PART)]);
        assert_eq!(check(&dir), Vec::<String>::new(), "{tag}");
    }
}

/// A season's opening resets its `tier: season:<name>` facts to the seeds
/// (the cadence is observed after each beat), so such a guard fact is not
/// Must in a deferred handler body either.
#[test]
fn a_deferred_handler_guard_gives_no_season_tier_fact() {
    let schema = SCHEMA
        .replace(
            "met: { args: [person], tier: user }",
            "met: { args: [person], tier: \"season:s\" }",
        )
        .replace(
            "state:\n",
            "state:\n  user.sOn: { type: bool, default: false }\n",
        )
        + "seasons:\n  s: { live: \"user.sOn\" }\n";
    let dir = project("dg-season", &schema, "!holds('met', ['mira'])", &[]);
    assert_eq!(check(&dir), Vec::<String>::new());
}

/// Precision kept: a fact nothing can remove (no retract, no displacement,
/// `tier: user`) still holds in the handler body, so the `!holds` arm is
/// dead.
#[test]
fn a_deferred_handler_guard_keeps_a_fact_nothing_removes() {
    let dir = project("dg-monotone", SCHEMA, "!holds('met', ['mira'])", &[]);
    let diags = check(&dir);
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert!(
        diags[0].starts_with("quests/q.lute:10:7 [E-ARM-DEAD]"),
        "{diags:?}"
    );
}
