//! 0.52.0 §4 (#208): the issue's reproduction through `lute check`. A quest
//! with an empty objective, then a narration line and a `::use` at its top
//! level: on v0.51.0 it checks clean and the runtime never presents either;
//! now each is `E-GRAMMAR-NOT-ADMITTED`, and the same content inside the
//! objective's body checks clean.

use std::path::Path;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

const PROJECT: &str = "defaultProfile: core\nprofiles: { core: { plugins: {} } }\n";

const COMPONENT: &str = "---\ncomponent: c\n---\n## C.\n@narrator: From the component.\n";

const CONTENT: &str = "@narrator: Never shown.\n  ::use{component=\"c\"}\n";

fn write(dir: &Path, rel: &str, text: &str) {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, text).unwrap();
}

/// The diagnostic lines of `lute check --project .` on a project whose
/// quest `q.lute` declares the component and holds `quest_body`.
fn check(tag: &str, quest_body: &str) -> Vec<String> {
    let dir = lute_scratch::ScratchDir::named(tag);
    write(&dir, "lute.project.yaml", PROJECT);
    write(&dir, "c.component.lute", COMPONENT);
    write(
        &dir,
        "q.lute",
        &format!(
            "---\nkind: quest\ncomponents: [c.component.lute]\n---\n<quest id=\"q\" title=\"Q\" \
             tier=\"run\" start=\"true\">\n{quest_body}</quest>\n"
        ),
    );
    let out = Command::new(BIN)
        .args(["check", "--project", "."])
        .current_dir(&dir)
        .env("LUTE_TELEMETRY", "0")
        .output()
        .unwrap();
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
    .lines()
    .filter(|l| l.contains(" [") && !l.starts_with("ok:") && !l.starts_with("failed:"))
    .map(str::to_string)
    .collect()
}

fn refusal(at: &str, what: &str) -> String {
    format!(
        "./q.lute:{at}: error [E-GRAMMAR-NOT-ADMITTED] {what} at the top level of `<quest \
         id=\"q\">` never runs — a quest runs only its `<on>` handlers and `<objective>` bodies: \
         move it into the `<on event=…>` or `<objective>` whose moment it belongs to"
    )
}

#[test]
fn issue_208_quest_top_level_content_is_refused() {
    assert_eq!(
        check(
            "quest-top-208",
            &format!("  <objective id=\"o\" title=\"O\" done=\"true\"/>\n  {CONTENT}")
        ),
        [
            refusal("7:3", "a content line"),
            refusal("8:3", "a `::directive`")
        ]
    );
}

#[test]
fn the_same_content_in_the_objective_body_checks_clean() {
    assert_eq!(
        check(
            "quest-top-208-moved",
            &format!(
                "  <objective id=\"o\" title=\"O\" done=\"true\">\n  {CONTENT}  </objective>\n"
            )
        ),
        Vec::<String>::new()
    );
}
