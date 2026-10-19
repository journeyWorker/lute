//! 0.41.0 §3.2: `type: number` is `E-TYPE-NUMBER` at every declaration kind,
//! with the `int` suggestion when every value the declaration takes is whole.

use std::path::Path;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_lute");
const BASE: &str = "`type: number` is no longer a numeric type — choose `type: int` for whole \
                    numbers or `type: double` for fractional values";

fn write(dir: &Path, rel: &str, text: &str) {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, text).unwrap();
}

fn check(dir: &Path, file: &str) -> String {
    let out = Command::new(BIN)
        .arg("check")
        .arg(dir.join(file))
        .output()
        .unwrap();
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

#[test]
fn frontmatter_number_declarations_name_the_choice() {
    let dir = lute_scratch::ScratchDir::named("type-number-041");
    write(
        &dir,
        "a.lute",
        "---\nkind: scene\nid: a\nluteVersion: \"0.47.2\"\nstate:\n  \
         run.coins: { type: number, default: 3 }\n  run.rate: { type: number, default: 0.5 }\n  \
         run.bare: { type: number }\ndefs:\n  rich: { type: bool, params: { n: number }, cel: \"run.coins > n\" }\n---\n\
         ## A\n@narrator: {{run.coins}} {{run.rate}} {{run.bare}}\n",
    );
    let text = check(&dir, "a.lute");
    let lines: Vec<&str> = text
        .lines()
        .filter(|l| l.contains("[E-TYPE-NUMBER]"))
        .collect();
    assert_eq!(lines.len(), 4, "{text}");
    assert!(
        lines[0].contains(&format!(
            "{BASE}; every value it takes here is whole (3), so `int` fits"
        )),
        "{text}"
    );
    for line in &lines[1..] {
        assert!(line.ends_with(BASE), "{text}");
    }
    for absent in ["E-STATE-DECL", "E-DEF-DECL", "E-UNDECLARED", "E-CEL-"] {
        assert!(!text.contains(absent), "{absent}:\n{text}");
    }
}

#[test]
fn the_suggestion_lists_at_most_three_values() {
    assert_eq!(
        lute_manifest::types::number_type_message(&[1.0, 2.0, 2.0, 3.0, 4.0]),
        format!("{BASE}; every value it takes here is whole (1, 2, 3, …), so `int` fits")
    );
    assert_eq!(lute_manifest::types::number_type_message(&[1.0, 0.5]), BASE);
    assert_eq!(lute_manifest::types::number_type_message(&[]), BASE);
}
