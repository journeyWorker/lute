//! 0.41.0 §3.4: `W-OBJECTIVE-STRANDED` skips a quest that re-arms and an
//! objective a beat of another document completes; when it fires it is a
//! warning stating its evidence level.

use std::path::Path;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn write(dir: &Path, rel: &str, text: &str) {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, text).unwrap();
}

fn project(tag: &str, quest_attrs: &str, same_doc: bool) -> String {
    let dir = lute_scratch::ScratchDir::named(tag);
    write(
        &dir,
        "lute.project.yaml",
        "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\ndefaults:\n  luteVersion: \"0.48.1\"\n  uses: [world.schema.yaml]\n",
    );
    write(
        &dir,
        "world.schema.yaml",
        "state:\n  run.day: { type: int, default: 1, owner: engine }\n  \
         run.slot: { type: { enum: [morning, evening] }, default: evening, owner: engine }\n  \
         run.done: { type: bool, default: false }\nclock:\n  day: run.day\n  slot: run.slot\n  \
         slots: [morning, evening]\n  raise: tick\n  raiseAtStart: true\n",
    );
    let quest = format!(
        "<quest id=\"main\" {quest_attrs} start=\"true\">\n<objective id=\"done\" done=\"run.done\"/>\n</quest>\n"
    );
    let scene =
        "---\nkind: scene\nid: main\non: tick\nwhen: \"clock.index == 1\"\nadvances: slot\n---\n";
    if same_doc {
        write(
            &dir,
            "scenes/main.lute",
            &format!("{scene}{quest}\n## Main\n@narrator: Main.\n::set{{run.done = true}}\n"),
        );
    } else {
        write(
            &dir,
            "quests/main.lute",
            &format!("---\nkind: quest\nid: main\n---\n{quest}"),
        );
        write(
            &dir,
            "scenes/main.lute",
            &format!("{scene}## Main\n@narrator: Main.\n::set{{run.done = true}}\n"),
        );
    }
    let out = Command::new(BIN)
        .arg("check")
        .arg("--project")
        .arg(&*dir)
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn stranded_fires_only_on_this_documents_beats_of_a_quest_that_does_not_rearm() {
    let fired = project("stranded-041-same", "tier=\"user\"", true);
    assert!(fired.contains(
        "warning [W-OBJECTIVE-STRANDED] required objective `done` can only be completed by beats \
         of this document whose clock windows close at day 1 evening; it has no `until=` or `by=` \
         — add `until=` or `by=` so a missed window fails it (evidence: bounded — declared clock \
         windows of this document's beats, no path search)"
    ), "{fired}");
    for (tag, attrs, same) in [
        ("stranded-041-run", "tier=\"run\"", true),
        (
            "stranded-041-rearm",
            "tier=\"user\" rearm=\"run.day > 1\"",
            true,
        ),
        ("stranded-041-outside", "tier=\"user\"", false),
    ] {
        let text = project(tag, attrs, same);
        assert!(!text.contains("W-OBJECTIVE-STRANDED"), "{tag}:\n{text}");
    }
}
