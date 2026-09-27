//! A fault in an imported schema file is reported on the importing `.lute`
//! (the check anchors it there) with `relatedInformation` at the schema
//! file's own range, in UTF-16; while the schema is open it shows the fault
//! too, pointing back at the importer.

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::mpsc::Receiver;

fn send(child: &mut Child, msg: &serde_json::Value) {
    let body = msg.to_string();
    let stdin = child.stdin.as_mut().unwrap();
    write!(stdin, "Content-Length: {}\r\n\r\n{body}", body.len()).unwrap();
    stdin.flush().unwrap();
}

/// The next message; `None` once the server has exited.
fn recv(out: &mut BufReader<ChildStdout>) -> Option<serde_json::Value> {
    let mut len = 0;
    loop {
        let mut line = String::new();
        if out.read_line(&mut line).unwrap() == 0 {
            return None;
        }
        let line = line.trim();
        if line.is_empty() {
            break;
        }
        if let Some(n) = line.strip_prefix("Content-Length: ") {
            len = n.parse().unwrap();
        }
    }
    let mut body = vec![0; len];
    out.read_exact(&mut body).unwrap();
    Some(serde_json::from_slice(&body).unwrap())
}

/// The diagnostics of the next `publishDiagnostics` for `uri`.
fn published_for(rx: &Receiver<serde_json::Value>, uri: &str) -> Vec<serde_json::Value> {
    loop {
        let m = rx
            .recv_timeout(std::time::Duration::from_secs(60))
            .expect("the server stopped answering");
        if m["method"] == "textDocument/publishDiagnostics" && m["params"]["uri"] == uri {
            return m["params"]["diagnostics"].as_array().unwrap().clone();
        }
    }
}

/// `text`'s part a same-line LSP range covers, reading its characters as
/// UTF-16 code units (the LSP default).
fn covered(text: &str, range: &serde_json::Value) -> String {
    let (s, e) = (&range["start"], &range["end"]);
    assert_eq!(s["line"], e["line"], "{range}");
    let line = text
        .lines()
        .nth(s["line"].as_u64().unwrap() as usize)
        .unwrap();
    let units: Vec<u16> = line.encode_utf16().collect();
    let (a, b) = (
        s["character"].as_u64().unwrap(),
        e["character"].as_u64().unwrap(),
    );
    String::from_utf16(&units[a as usize..b as usize]).unwrap()
}

#[test]
fn a_schema_fault_carries_the_schema_location() {
    let dir = std::env::temp_dir().join(format!("lute-lsp-related-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("scenes")).unwrap();
    std::fs::create_dir_all(dir.join("plugins/game.occasions/occasions")).unwrap();
    std::fs::write(
        dir.join("lute.project.yaml"),
        "pluginsDir: plugins/\ndefaultProfile: game\nprofiles:\n  game:\n    \
         plugins: { game.occasions: true }\ndefaults:\n  uses: [world.schema.yaml]\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("plugins/game.occasions/plugin.yaml"),
        "id: game.occasions\nversion: 0.1.0\nkind: capability\n\
         depends: [ { id: lute.core, range: \"^0.0.1\" } ]\nexports:\n  occasions: occasions/\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("plugins/game.occasions/occasions/game.yaml"),
        "occasions:\n  evening: { select: first }\n",
    )
    .unwrap();
    // Korean before the fault on its line: a byte column would miss it.
    let schema = "state:\n  run.note: { type: string, default: \"\" }\n  \
                  run.fate: { type: { domain: fate }, default: alive }\n\
                  enums:\n  fate: [alive, taken]\n\
                  terminal: \"run.note != '민수 🌙' && run.fate != 'alvie'\"\n";
    std::fs::write(dir.join("world.schema.yaml"), schema).unwrap();
    // A scene answering an occasion: the check judges the terminal for it.
    let scene =
        "---\nkind: scene\nid: s\ntitle: S\non: evening\n---\n\n## S\n\n@narrator: Hello.\n";
    let scene_path = dir.join("scenes/s.lute");
    std::fs::write(&scene_path, scene).unwrap();
    let scene_uri = format!("file://{}", scene_path.display());
    let schema_uri = format!("file://{}", dir.join("world.schema.yaml").display());

    let mut child = Command::new(env!("CARGO_BIN_EXE_lute-lsp"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .current_dir(&dir)
        .spawn()
        .unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        while let Some(m) = recv(&mut out) {
            if tx.send(m).is_err() {
                break;
            }
        }
    });
    send(
        &mut child,
        &serde_json::json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"capabilities": {}}}),
    );
    while rx
        .recv_timeout(std::time::Duration::from_secs(60))
        .expect("initialize reply")["id"]
        != 1
    {}
    send(
        &mut child,
        &serde_json::json!({"jsonrpc": "2.0", "method": "initialized", "params": {}}),
    );
    send(
        &mut child,
        &serde_json::json!({"jsonrpc": "2.0", "method": "textDocument/didOpen", "params": {
            "textDocument": {"uri": schema_uri, "languageId": "yaml", "version": 1, "text": schema}}}),
    );
    published_for(&rx, &schema_uri);
    send(
        &mut child,
        &serde_json::json!({"jsonrpc": "2.0", "method": "textDocument/didOpen", "params": {
            "textDocument": {"uri": scene_uri, "languageId": "lute", "version": 1, "text": scene}}}),
    );
    let on_scene = published_for(&rx, &scene_uri);
    // The importer's publish republishes the open schema with the fault.
    let on_schema = published_for(&rx, &schema_uri);
    let _ = child.kill();
    let _ = std::fs::remove_dir_all(&dir);

    let fault = on_scene
        .iter()
        .find(|d| d["code"] == "E-WHEN-LITERAL-DOMAIN")
        .unwrap_or_else(|| panic!("the schema's terminal fault on the importer: {on_scene:#?}"));
    let related = fault["relatedInformation"]
        .as_array()
        .unwrap_or_else(|| panic!("relatedInformation at the schema: {fault:#}"));
    let at = &related[0]["location"];
    assert_eq!(at["uri"], schema_uri, "{fault:#}");
    assert_eq!(
        at["range"]["start"]["line"], 5,
        "the `terminal:` line: {fault:#}"
    );
    // The terminal value as written, quotes included: a byte-counted range
    // would end inside the emoji or past the line.
    let text = covered(schema, &at["range"]);
    assert_eq!(
        text.trim_matches('"'),
        "run.note != '민수 🌙' && run.fate != 'alvie'",
        "the range covers the terminal condition in UTF-16 units"
    );

    let there = on_schema
        .iter()
        .find(|d| d["relatedInformation"][0]["location"]["uri"] == scene_uri)
        .unwrap_or_else(|| panic!("the fault on the open schema, pointing back: {on_schema:#?}"));
    assert_eq!(there["code"], "E-WHEN-LITERAL-DOMAIN", "{there:#}");
    assert_eq!(there["range"], at["range"], "{there:#}");
}
