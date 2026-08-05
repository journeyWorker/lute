//! 0.10.0 §9 rule 2: one diagnostic per PROBLEM, not per caller. Eleven modules
//! importing one broken component must not produce eleven identical messages.
//!
//! `validate_components` runs once per importing document, so N callers of one
//! broken component produce N separate `check()` runs and N byte-identical
//! diagnostics, each at line 1 column 1 of a file that is entirely correct. The
//! roll-up happens where those runs meet — `run_check_project`.
//!
//! The fixture is T6.3's exact shape, which is what §9 exists for: the
//! component's own `uses:` accepts the authored `emotion`, so the STANDALONE leg
//! is clean, while the vocabulary that actually applies at each `::use` site
//! rejects it. `write_two_callers_one_specific` then gives the two callers
//! DIFFERENT vocabularies, so the same authored line yields two different
//! messages — the caller-independent / caller-specific boundary, made
//! observable. §9 rule 4's suite reuses both builders: with a single caller,
//! rule 4's intersection over call sites and a union are indistinguishable.

use std::path::PathBuf;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

/// The component's path relative to a fixture root, shared by every builder in
/// this file. §9 rule 4's tests resolve the component as
/// `dir.join(COMPONENT_REL)`; one spelling, one place to change it.
pub const COMPONENT_REL: &str = "components/interject.component.lute";

/// A fresh unique temp dir (no `tempfile` dev-dep needed for these small tests).
pub fn temp_dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("lute-cli-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The component body's fault: `emotion="smug"` on a content line. Its own
/// `uses:` DECLARES `smug`, so a standalone check of this file resolves clean —
/// the one vocabulary that never applies at runtime (0.9.0 §6.1). `E-BAD-ENUM`'s
/// message enumerates the resolved domain, so two callers with different
/// vocabularies produce two different messages for this one authored line.
const COMPONENT: &str = "---\ncomponent: interject\nuses: [../vocab-own.yaml]\n---\n\
## Interjection\n@purser{emotion=\"smug\"}: Allocation is nominal.\n";

/// The component's OWN vocabulary: `smug` is a member here and nowhere else.
pub const VOCAB_OWN: &str = "enums:\n  emotion: [level, wry, smug]\n";
/// The `emotion` vocabulary every caller shares in the identical-report case.
pub const VOCAB_A: &str = "enums:\n  emotion: [level, wry]\n";
/// A DIFFERENT `emotion` vocabulary: the same authored `smug` fails here too,
/// but the message enumerates a different member list.
pub const VOCAB_B: &str = "enums:\n  emotion: [level, calm]\n";

fn scene(episode: u32, vocab: &str) -> String {
    format!(
        "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: {episode}\n\
uses: [{vocab}]\ncomponents: [../{COMPONENT_REL}]\n---\n\
## Shot 1.\n::use{{component=\"interject\"}}\n"
    )
}

fn write_project(tag: &str, vocabs: &[(&str, &str)], scenes: &[(&str, String)]) -> PathBuf {
    let dir = temp_dir(tag);
    std::fs::create_dir_all(dir.join("components")).unwrap();
    std::fs::create_dir_all(dir.join("scenes")).unwrap();
    std::fs::write(dir.join(COMPONENT_REL), COMPONENT).unwrap();
    std::fs::write(dir.join("vocab-own.yaml"), VOCAB_OWN).unwrap();
    for (name, text) in vocabs {
        std::fs::write(dir.join(name), text).unwrap();
    }
    for (name, text) in scenes {
        std::fs::write(dir.join("scenes").join(name), text).unwrap();
    }
    dir
}

/// A project with one component carrying a body fault and THREE callers, every
/// caller resolving the SAME vocabulary — so all three messages are
/// byte-identical.
pub fn write_three_callers(tag: &str) -> PathBuf {
    write_project(
        tag,
        &[("vocab-a.yaml", VOCAB_A)],
        &[
            ("a.lute", scene(1, "../vocab-a.yaml")),
            ("b.lute", scene(2, "../vocab-a.yaml")),
            ("c.lute", scene(3, "../vocab-a.yaml")),
        ],
    )
}

/// Two callers whose vocabularies DIFFER, so the same authored line produces two
/// different messages. Rule 2 must not fold them, and rule 4's intersection over
/// call sites must report neither.
pub fn write_two_callers_one_specific(tag: &str) -> PathBuf {
    write_project(
        tag,
        &[("vocab-a.yaml", VOCAB_A), ("vocab-b.yaml", VOCAB_B)],
        &[
            ("a.lute", scene(1, "../vocab-a.yaml")),
            ("b.lute", scene(2, "../vocab-b.yaml")),
        ],
    )
}

pub fn run(args: &[&str]) -> std::process::Output {
    Command::new(BIN).args(args).output().unwrap()
}

/// Every `(path, code, message)` a `check-project --json` run reports for a
/// document under `scenes/` — i.e. the CALLERS, excluding the component file's
/// own standalone verdict, which §9 rule 4 owns separately.
fn caller_diags(dir: &PathBuf, code: &str) -> Vec<(String, String)> {
    let out = run(&["check-project", dir.to_str().unwrap(), "--json"]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(&out.stdout)));
    let mut hits = Vec::new();
    for f in v["files"].as_array().unwrap() {
        let path = f["path"].as_str().unwrap_or_default().to_string();
        if !path.contains("scenes") {
            continue;
        }
        for d in f["diagnostics"].as_array().into_iter().flatten() {
            if d["code"] == code {
                hits.push((
                    path.clone(),
                    d["message"].as_str().unwrap_or_default().to_string(),
                ));
            }
        }
    }
    hits
}

/// Fixture wiring, pinned: every caller RESOLVES the component. A mis-rooted
/// `components:` path would make each caller report `E-COMPONENT-PARSE` instead,
/// and the roll-up assertion below would then pass on a fixture where the
/// duplication it claims to fold never existed.
#[test]
fn every_caller_resolves_the_component() {
    let dir = write_three_callers("rollup-wiring");
    let unresolved = caller_diags(&dir, "E-COMPONENT-PARSE");
    assert!(
        unresolved.is_empty(),
        "the fixture's `components:` path must resolve from every caller; got {unresolved:#?}"
    );
}

#[test]
fn identical_caller_reports_roll_up_to_one() {
    let dir = write_three_callers("rollup");
    let hits = caller_diags(&dir, "E-BAD-ENUM");
    assert_eq!(
        hits.len(),
        1,
        "one diagnostic per problem, not per caller; got {hits:#?}"
    );
    assert!(
        hits[0].1.ends_with("(+2 more callers)"),
        "the remaining callers must be summarised, not silently dropped; got {}",
        hits[0].1
    );
}

/// The roll-up folds identical problems only. A caller-SPECIFIC fault stays with
/// its own caller, where the caller is visible — that is rule 4's boundary and
/// the roll-up must not erase it.
#[test]
fn a_caller_specific_fault_is_not_rolled_up() {
    let dir = write_two_callers_one_specific("specific");
    let hits = caller_diags(&dir, "E-BAD-ENUM");
    assert_eq!(
        hits.len(),
        2,
        "two different problems stay two, one per caller; got {hits:#?}"
    );
    assert!(
        hits.iter().all(|(_, m)| !m.contains("more callers")),
        "two different problems are not one problem; got {hits:#?}"
    );
}
