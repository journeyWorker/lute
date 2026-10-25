//! dsl 0.50.0 §7 (#166a): each `W-STAGE-ABSENT` line hint names, beside
//! `::actor`, the plugin directive lowered to the `actor` record — and the
//! spelling it names is one the toolchain accepts. The test feeds the hint's
//! own `lower:` text into a plugin and checks that re-showing the speaker with
//! that directive clears the warning after a declared exit, a `::bg` scene
//! change and a `::clear`.
//!
//! On 0.49.1 the hints name only `::actor`, so no `lower:` spelling is found
//! and the test fails there.

use std::path::Path;
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_lute");

/// The three departures a `Staged::Line` hint covers, each as the staging
/// between `vesna` being shown and her next line.
const DEPARTURES: [(&str, &str); 3] = [
    ("exit", "::actor{character=\"vesna\" action=\"hide\"}\n"),
    ("bg", "::bg{location=\"cafe\"}\n"),
    ("clear", "::clear\n"),
];

/// A scene that shows `vesna`, takes her off stage by `departure`, runs
/// `reshow` (empty: nothing), then gives her a line.
fn scene(id: &str, departure: &str, reshow: &str) -> String {
    format!(
        "---\nkind: scene\nid: {id}\nenums:\n  action:\n    members: [show, hide]\n    \
         exits: [hide]\n  anchor:\n    members: [left, center, right]\n    default: center\n\
         ---\n\n## Shot 1.\n\n::actor{{character=\"vesna\" action=\"show\"}}\n\
         {departure}{reshow}@vesna: Here.\n"
    )
}

fn lute(dir: &Path, args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .current_dir(dir)
        .env("LUTE_TELEMETRY", "0")
        .output()
        .unwrap()
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

/// The backticked `lower: …` spelling a `W-STAGE-ABSENT` message names.
fn named_lowering(message: &str) -> Option<&str> {
    let start = message.find("`lower: ")? + 1;
    let len = message[start..].find('`')?;
    Some(&message[start..start + len])
}

#[test]
fn the_line_hint_names_an_actor_lowering_that_clears_the_warning() {
    // 1. Core only: each departure draws one `W-STAGE-ABSENT` whose hint
    //    names `::actor` and one `lower:` spelling, the same in all three.
    let bare = lute_scratch::ScratchDir::named("stage-hint-bare");
    std::fs::write(
        bare.join("lute.project.yaml"),
        "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n",
    )
    .unwrap();
    std::fs::create_dir_all(bare.join("scenes")).unwrap();
    for (id, departure) in DEPARTURES {
        std::fs::write(
            bare.join(format!("scenes/{id}.lute")),
            scene(id, departure, ""),
        )
        .unwrap();
    }
    let out = lute(&bare, &["check"]);
    let text = stdout(&out);
    assert_eq!(out.status.code(), Some(0), "{text}");
    let warnings: Vec<&str> = text
        .lines()
        .filter(|line| line.contains("warning [W-STAGE-ABSENT]"))
        .collect();
    assert_eq!(warnings.len(), DEPARTURES.len(), "{text}");
    let mut lowering = None;
    for warning in &warnings {
        assert!(
            warning.contains("Show them again with an `::actor`"),
            "{warning}"
        );
        let named = named_lowering(warning).unwrap_or_else(|| panic!("no `lower:`: {warning}"));
        assert!(named.contains("record: actor"), "{named}");
        assert_eq!(*lowering.get_or_insert(named), named, "{text}");
    }
    let lowering = lowering.unwrap();

    // 2. A plugin directive declared with exactly that spelling loads, and
    //    re-showing `vesna` with it after each departure clears the warning.
    let fixed = lute_scratch::ScratchDir::named("stage-hint-fixed");
    std::fs::write(
        fixed.join("lute.project.yaml"),
        "pluginsDir: plugins/\ndefaultProfile: p\nprofiles:\n  p:\n    \
         plugins: { lute.core: true, stage.hint: true }\n",
    )
    .unwrap();
    let plugin = fixed.join("plugins/stage.hint");
    std::fs::create_dir_all(plugin.join("directives")).unwrap();
    std::fs::write(
        plugin.join("plugin.yaml"),
        "id: stage.hint\nversion: 0.1.0\nkind: capability\ndepends:\n  \
         - { id: lute.core, range: \"^0.0.1\" }\nexports:\n  directives: directives/\n",
    )
    .unwrap();
    std::fs::write(
        plugin.join("directives/show.yaml"),
        format!(
            "directives:\n  - name: show\n    attrs:\n      \
             - {{ name: character, required: true, type: string }}\n    {lowering}\n"
        ),
    )
    .unwrap();
    std::fs::create_dir_all(fixed.join("scenes")).unwrap();
    for (id, departure) in DEPARTURES {
        std::fs::write(
            fixed.join(format!("scenes/{id}.lute")),
            scene(id, departure, "::show{character=\"vesna\"}\n"),
        )
        .unwrap();
    }
    let out = lute(&fixed, &["check"]);
    let text = stdout(&out);
    let all = format!("{text}{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(out.status.code(), Some(0), "{all}");
    assert!(!all.contains("W-STAGE-ABSENT"), "{all}");
    assert!(!all.contains("error"), "{all}");
    for (id, _) in DEPARTURES {
        assert!(
            text.contains(&format!("ok: ./scenes/{id}.lute (0 warning(s))\n")),
            "{text}"
        );
    }
}
