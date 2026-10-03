//! Quest `rearm=`: an attribute that means "repeat" names `rearm`, a
//! constant `rearm` warns (it never turns from false to true), and a
//! subquest's `rearm` is an error (its parent's end leaves it `unset` for
//! good) — in one document and across documents.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn temp_dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("lute-rearm028-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(dir: &Path, rel: &str, text: &str) {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, text).unwrap();
}

fn check_project(tag: &str, docs: &[(&str, &str)]) -> (Output, String) {
    let dir = temp_dir(tag);
    write(
        &dir,
        "lute.project.yaml",
        "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n",
    );
    write(
        &dir,
        "world.schema.yaml",
        "state:\n  run.day: { type: int, default: 1, owner: engine }\n  \
         run.floors: { type: int, default: 0, owner: engine }\n\
         clock:\n  day: run.day\n  week: { length: 7, first: 0 }\n\
         defs:\n  always: \"1 == 1\"\n",
    );
    for (rel, body) in docs {
        write(&dir, rel, body);
    }
    let out = Command::new(BIN)
        .arg("check-project")
        .arg(&dir)
        .output()
        .unwrap();
    let t = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out, t)
}

fn quests(body: &str) -> String {
    format!("---\nkind: quest\nid: tower\nuses: ../world.schema.yaml\n---\n\n{body}")
}

const CLIMB: &str = "<objective id=\"climb\" title=\"Climb\" done=\"run.floors >= 10\"/>";

#[test]
fn repeatable_names_rearm() {
    let (out, t) = check_project(
        "repeatable",
        &[(
            "quests/tower.lute",
            &quests(&format!(
                "<quest id=\"weekly\" title=\"Weekly\" repeatable=\"true\">\n  {CLIMB}\n</quest>\n"
            )),
        )],
    );
    assert!(!out.status.success(), "{t}");
    assert!(
        t.contains("[E-UNKNOWN-ATTR]")
            && t.contains("has no attribute `repeatable`")
            && t.contains("`rearm=\"<condition>\"`"),
        "{t}"
    );
}

#[test]
fn a_constant_rearm_warns() {
    let (out, t) = check_project(
        "constant",
        &[(
            "quests/tower.lute",
            &quests(&format!(
                "<quest id=\"a\" title=\"A\" rearm=\"true\">\n  {CLIMB}\n</quest>\n\
                 <quest id=\"b\" title=\"B\" rearm=\"false\">\n  {CLIMB}\n</quest>\n\
                 <quest id=\"c\" title=\"C\" rearm=\"@always\">\n  {CLIMB}\n</quest>\n\
                 <quest id=\"d\" title=\"D\" rearm=\"clock.weekday == 0\">\n  {CLIMB}\n</quest>\n"
            )),
        )],
    );
    assert!(out.status.success(), "{t}");
    let warned: Vec<&str> = t
        .lines()
        .filter(|l| l.contains("[W-QUEST-REARM-CONSTANT]"))
        .collect();
    assert_eq!(warned.len(), 3, "{t}");
    assert!(warned[0].contains("quest `a` is always true"), "{t}");
    assert!(warned[1].contains("quest `b` is never true"), "{t}");
    assert!(warned[2].contains("quest `c` is always true"), "{t}");
}

#[test]
fn a_subquest_rearm_is_an_error_in_one_document_and_across_documents() {
    let parent = |child: &str| {
        format!(
            "<quest id=\"p{child}\" title=\"P\">\n  \
             <objective id=\"o\" title=\"O\" quest=\"{child}\"/>\n</quest>\n"
        )
    };
    let child = |id: &str| {
        format!(
            "<quest id=\"{id}\" title=\"C\" rearm=\"clock.weekday == 0\">\n  {CLIMB}\n</quest>\n"
        )
    };
    let (out, t) = check_project(
        "subquest",
        &[
            (
                "quests/tower.lute",
                &quests(&format!(
                    "{}{}{}",
                    parent("near"),
                    child("near"),
                    parent("far")
                )),
            ),
            (
                "quests/far.lute",
                &format!(
                    "---\nkind: quest\nid: faraway\nuses: ../world.schema.yaml\n---\n\n{}",
                    child("far")
                ),
            ),
        ],
    );
    assert!(!out.status.success(), "{t}");
    let errs: Vec<&str> = t
        .lines()
        .filter(|l| l.contains("[E-SUBQUEST-REARM]"))
        .collect();
    assert_eq!(errs.len(), 2, "{t}");
    assert!(
        errs.iter()
            .any(|l| l.contains("tower.lute") && l.contains("subquest `near`")),
        "{t}"
    );
    assert!(
        errs.iter()
            .any(|l| l.contains("far.lute") && l.contains("subquest `far`")),
        "{t}"
    );
}
