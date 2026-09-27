//! A diagnostic's column counts characters on every CLI surface: Korean text
//! or an em dash before the error on its line is one column per character,
//! never its UTF-8 byte length. A literal-domain error points at the literal.

use std::path::PathBuf;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn temp_file(tag: &str, text: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lute-columns-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("doc.lute");
    std::fs::write(&path, text).unwrap();
    path
}

fn check(path: &PathBuf, json: bool) -> String {
    let mut args = vec!["check"];
    if json {
        args.push("--json");
    }
    let path = path.to_str().unwrap();
    args.push(path);
    let o = Command::new(BIN).args(&args).output().unwrap();
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

/// Every `{code, span}` object in `v`, as `(code, line, column)`.
fn spans(v: &serde_json::Value, out: &mut Vec<(String, u64, u64)>) {
    match v {
        serde_json::Value::Object(m) => {
            if let (Some(code), Some(span)) =
                (m.get("code").and_then(|c| c.as_str()), m.get("span"))
            {
                if let (Some(l), Some(c)) = (span["line"].as_u64(), span["column"].as_u64()) {
                    out.push((code.to_string(), l, c));
                }
            }
            m.values().for_each(|x| spans(x, out));
        }
        serde_json::Value::Array(a) => a.iter().for_each(|x| spans(x, out)),
        _ => {}
    }
}

/// The 1-based character column of `needle` on 1-based line `line`.
fn char_column(text: &str, line: usize, needle: &str) -> u64 {
    let l = text.lines().nth(line - 1).unwrap();
    l[..l.find(needle).unwrap()].chars().count() as u64 + 1
}

fn json_spans(path: &PathBuf) -> Vec<(String, u64, u64)> {
    let out = check(path, true);
    let start = out
        .find(['{', '['])
        .unwrap_or_else(|| panic!("no JSON: {out}"));
    let v: serde_json::Value =
        serde_json::from_str(out[start..].trim()).unwrap_or_else(|e| panic!("{e}: {out}"));
    let mut found = Vec::new();
    spans(&v, &mut found);
    found
}

#[test]
fn a_column_after_korean_text_counts_characters() {
    let text =
        "---\nkind: scene\nid: ko\n---\n## 장면\n\n@narrator: 안녕 — 반가워 {{run.nope}} 끝.\n";
    let path = temp_file("korean", text);
    let col = char_column(text, 7, "{{run.nope}}");
    assert_eq!(col, 21);
    let human = check(&path, false);
    assert!(
        human.contains(&format!("doc.lute:7:{col}: error [E-UNDECLARED]")),
        "the character column, not the byte column 33: {human}"
    );
    let found = json_spans(&path);
    assert!(
        found.contains(&("E-UNDECLARED".to_string(), 7, col)),
        "--json span.column is the same character column: {found:?}"
    );
}

#[test]
fn a_foreign_literal_in_a_condition_points_at_the_literal() {
    let text = "---\nkind: scene\nid: lit\nstate:\n  \
                run.route: { type: { enum: [ren, mika] }, default: ren }\n  \
                run.aff: { type: number, default: 0 }\n---\n## 장면\n\n\
                @narrator{when=\"run.aff >= 1 && run.route == 'rne'\"}: 안녕.\n";
    let path = temp_file("literal", text);
    let col = char_column(text, 10, "'rne'");
    let human = check(&path, false);
    assert!(
        human.contains(&format!("doc.lute:10:{col}: error [E-WHEN-LITERAL-DOMAIN]")),
        "at the quoted literal, not the value start: {human}"
    );
    let found = json_spans(&path);
    assert!(
        found.contains(&("E-WHEN-LITERAL-DOMAIN".to_string(), 10, col)),
        "{found:?}"
    );
}
