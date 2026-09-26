//! dsl 0.26.0 §8 (T3-12): a running `lute-lsp` whose binary was replaced
//! publishes one "stale server, restart" diagnostic instead of results from
//! its older build.

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdout, Command, Stdio};

fn send(child: &mut Child, msg: &serde_json::Value) {
    let body = msg.to_string();
    let stdin = child.stdin.as_mut().unwrap();
    write!(stdin, "Content-Length: {}\r\n\r\n{body}", body.len()).unwrap();
    stdin.flush().unwrap();
}

fn recv(out: &mut BufReader<ChildStdout>) -> serde_json::Value {
    let mut len = 0;
    loop {
        let mut line = String::new();
        out.read_line(&mut line).unwrap();
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
    serde_json::from_slice(&body).unwrap()
}

fn diagnostics(out: &mut BufReader<ChildStdout>) -> Vec<serde_json::Value> {
    loop {
        let m = recv(out);
        if m["method"] == "textDocument/publishDiagnostics" {
            return m["params"]["diagnostics"].as_array().unwrap().clone();
        }
    }
}

#[test]
fn a_replaced_binary_publishes_one_stale_diagnostic() {
    let dir = std::env::temp_dir().join(format!("lute-lsp-stale-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let bin = dir.join("lute-lsp");
    std::fs::copy(env!("CARGO_BIN_EXE_lute-lsp"), &bin).unwrap();
    let mut child = Command::new(&bin)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());
    send(
        &mut child,
        &serde_json::json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"capabilities": {}}}),
    );
    let _ = recv(&mut out);
    send(
        &mut child,
        &serde_json::json!({"jsonrpc": "2.0", "method": "initialized", "params": {}}),
    );
    let uri = "file:///nowhere/a.lute";
    // An undeclared read: a current server reports it.
    let text = "---\nkind: scene\n---\n## A\n::set{run.nope = 1}\n";
    send(
        &mut child,
        &serde_json::json!({"jsonrpc": "2.0", "method": "textDocument/didOpen", "params": {
            "textDocument": {"uri": uri, "languageId": "lute", "version": 1, "text": text}}}),
    );
    let first = diagnostics(&mut out);
    assert!(!first.is_empty());
    assert!(
        first.iter().all(|d| d["code"] != "lute-lsp-stale"),
        "{first:?}"
    );

    // Reinstall: a new file renamed over the running binary.
    let fresh = dir.join("lute-lsp.new");
    std::fs::copy(env!("CARGO_BIN_EXE_lute-lsp"), &fresh).unwrap();
    std::fs::rename(&fresh, &bin).unwrap();
    send(
        &mut child,
        &serde_json::json!({"jsonrpc": "2.0", "method": "textDocument/didChange", "params": {
            "textDocument": {"uri": uri, "version": 2},
            "contentChanges": [{"text": text}]}}),
    );
    let after = diagnostics(&mut out);
    let _ = child.kill();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(after.len(), 1, "{after:?}");
    assert_eq!(after[0]["code"], "lute-lsp-stale");
    let message = after[0]["message"].as_str().unwrap();
    assert!(message.contains(env!("CARGO_PKG_VERSION")), "{message}");
    assert!(message.contains("restart the language server"), "{message}");
}

/// Round-5 T3-19: a project whose manifest stamps `defaults: luteVersion`
/// newer than this server means the editor runs an older `lute-lsp` than
/// the project targets. The server says so once, in the client's UI, naming
/// `lute doctor`, and marks the inherited stamp with the checker's
/// `W-LUTE-VERSION-STALE` (which names `lute doctor` too); a second document
/// does not repeat the message.
#[test]
fn a_project_stamp_newer_than_the_server_names_lute_doctor_once() {
    let dir = std::env::temp_dir().join(format!("lute-lsp-newer-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("scenes")).unwrap();
    std::fs::write(
        dir.join("lute.project.yaml"),
        "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\ndefaults:\n  luteVersion: \"999.0.0\"\n",
    )
    .unwrap();
    let text = "---\nkind: scene\nid: a\n---\n## A\n@narrator: Hi.\n";
    std::fs::write(dir.join("scenes/a.lute"), text).unwrap();
    std::fs::write(dir.join("scenes/b.lute"), text.replace("id: a", "id: b")).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_lute-lsp"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());
    send(
        &mut child,
        &serde_json::json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"capabilities": {}}}),
    );
    let _ = recv(&mut out);
    send(
        &mut child,
        &serde_json::json!({"jsonrpc": "2.0", "method": "initialized", "params": {}}),
    );
    // Every `window/showMessage` before the next `publishDiagnostics`, and
    // that publish's diagnostics.
    let mut open = |name: &str, text: &str| {
        let uri = format!("file://{}", dir.join("scenes").join(name).display());
        send(
            &mut child,
            &serde_json::json!({"jsonrpc": "2.0", "method": "textDocument/didOpen", "params": {
                "textDocument": {"uri": uri, "languageId": "lute", "version": 1, "text": text}}}),
        );
        let mut shown = Vec::new();
        loop {
            let m = recv(&mut out);
            if m["method"] == "window/showMessage" {
                shown.push(m["params"]["message"].as_str().unwrap().to_string());
            } else if m["method"] == "textDocument/publishDiagnostics" {
                return (
                    shown,
                    m["params"]["diagnostics"].as_array().unwrap().clone(),
                );
            }
        }
    };
    let (shown, diags) = open("a.lute", text);
    let (shown_again, _) = open("b.lute", &text.replace("id: a", "id: b"));
    let _ = child.kill();
    let _ = std::fs::remove_dir_all(&dir);

    assert_eq!(shown.len(), 1, "{shown:?}");
    assert!(
        shown[0].contains("older than the project targets (luteVersion 999.0.0)")
            && shown[0].contains("run `lute doctor`"),
        "{shown:?}"
    );
    assert!(
        shown_again.is_empty(),
        "shown once per session: {shown_again:?}"
    );
    let stamp = diags
        .iter()
        .find(|d| d["code"] == "W-LUTE-VERSION-STALE")
        .unwrap_or_else(|| panic!("the inherited stamp is marked: {diags:?}"));
    let message = stamp["message"].as_str().unwrap();
    assert!(
        message.contains("999.0.0") && message.contains("run `lute doctor`"),
        "{message}"
    );
}
