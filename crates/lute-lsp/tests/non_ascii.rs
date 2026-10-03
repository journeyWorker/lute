//! FS-F1: a non-ASCII character outside a CEL string literal (`≥`, a curly
//! quote, Hangul) is an ordinary `E-CEL-PARSE`, never a crash — the server
//! publishes it and keeps answering.

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdout, Command, Stdio};

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

#[test]
fn non_ascii_in_a_condition_is_a_diagnostic_not_a_crash() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_lute-lsp"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    // Messages arrive on a reader thread, so a server that stops answering
    // (a panicked handler leaves the process alive) fails, never hangs.
    let mut out = BufReader::new(child.stdout.take().unwrap());
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        while let Some(m) = recv(&mut out) {
            if tx.send(m).is_err() {
                break;
            }
        }
    });
    let next = || rx.recv_timeout(std::time::Duration::from_secs(60)).ok();
    send(
        &mut child,
        &serde_json::json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"capabilities": {}}}),
    );
    next().expect("initialize reply");
    send(
        &mut child,
        &serde_json::json!({"jsonrpc": "2.0", "method": "initialized", "params": {}}),
    );
    let text = "---\nkind: scene\nid: ge\nstate:\n  \
                run.clues: { type: int, default: 0 }\n  \
                run.who: { type: string, default: \"\" }\n---\n## A\n\
                @narrator{when=\"run.clues ≥ 2\"}: A line.\n\
                ::set{ run.who = “ruben” }\n";
    send(
        &mut child,
        &serde_json::json!({"jsonrpc": "2.0", "method": "textDocument/didOpen", "params": {
            "textDocument": {"uri": "file:///nowhere/ge.lute", "languageId": "lute", "version": 1, "text": text}}}),
    );
    let published = loop {
        let m = next().expect("the server stopped answering on didOpen");
        if m["method"] == "textDocument/publishDiagnostics" {
            break m["params"]["diagnostics"].as_array().unwrap().clone();
        }
    };
    send(
        &mut child,
        &serde_json::json!({"jsonrpc": "2.0", "id": 2, "method": "shutdown", "params": null}),
    );
    let alive = std::iter::from_fn(next).any(|m| m["id"] == 2);
    let _ = child.kill();
    assert!(alive, "the server must still answer after didOpen");
    let parse_errors = published
        .iter()
        .filter(|d| d["code"] == "E-CEL-PARSE")
        .count();
    assert_eq!(parse_errors, 2, "{published:#?}");
}
