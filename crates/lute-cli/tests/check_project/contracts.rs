//! `lute check-project <dir>` acceptance (0.2.1 §6.3 gap #3): project-wide
//! `<quest id>` uniqueness for quest docs NOT connected by an import edge —
//! `lute check`'s own `E-QUEST-ID-DUP` (0.2.0 F4) only sees a collision within
//! one document or across ITS OWN `uses:`/`extends:` import graph.

use super::support::{run_cli, TempProject, BIN};

use std::path::PathBuf;

fn temp_dir(tag: &str) -> TempProject {
    TempProject::new(tag)
}

fn write(dir: &std::path::Path, rel: &str, text: &str) -> PathBuf {
    let path = dir.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(&path, text).unwrap();
    path
}

fn run(args: &[&str]) -> std::process::Output {
    run_cli(args)
}
/// A self-contained, otherwise-CLEAN `kind: quest` doc declaring exactly one
/// quest id (its own state decl + a `done` slot that reads it, so no other
/// diagnostic — E-UNDECLARED/E-MAYBE-UNSET/etc — fires). `start="true"`: an
/// accept-driven quest nothing accepts is W-QUEST-NEVER-ACCEPTED (dsl 0.24.0).
fn clean_quest_doc(quest_id: &str, state_path: &str) -> String {
    format!(
        "---\nkind: quest\nstate:\n  {state_path}: {{ type: bool, default: false }}\n---\n\
         <quest id=\"{quest_id}\" start=\"true\">\n<objective id=\"o\" done=\"{state_path}\"/>\n</quest>\n"
    )
}

#[path = "cross_file.rs"]
mod cross_file;
#[path = "connectivity.rs"]
mod connectivity;
#[path = "liveness.rs"]
mod liveness;
#[path = "schema.rs"]
mod schema;
#[path = "late.rs"]
mod late;





