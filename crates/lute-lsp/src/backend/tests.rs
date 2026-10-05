use super::*;
use lute_core_span::{Span, TextIndex};
use tower_lsp_server::ls_types::{Position, Range, TextEdit as LspTextEdit};
use super::document::{uri_to_path, claimed_declaration_yaml};
use super::positions::position_to_byte;
use super::yaml_spans::find_key_span;

mod declarations;

#[test]
fn formatting_edit_uses_cli_formatter_and_full_document_range() {
    let text = "## S\r\n@narrator: hi  \r\n";
    let cli = lute_syntax::format_source(text, &lute_syntax::FormatOptions::default())
        .unwrap();
    let idx = TextIndex::new(text);
    let end = idx.position(text.len());
    let edit = LspTextEdit {
        range: Range {
            start: Position { line: 0, character: 0 },
            end: Position { line: end.line - 1, character: end.utf16_col },
        },
        new_text: cli.text.clone(),
    };
    assert_eq!(edit.new_text, cli.text);
    assert_eq!(edit.range.start, Position { line: 0, character: 0 });
    assert_eq!(edit.range.end.line, 2);
    assert_eq!(edit.range.end.character, 0);
}


/// `position_to_byte` is the exact inverse of `TextIndex::position` on a
/// multibyte document: every byte offset round-trips through its own Position.
#[test]
fn position_to_byte_round_trips_through_text_index() {
    let text = "## Shot 1.\n::café{x=\"π\"}\n:Ω: 世界\n";
    let idx = TextIndex::new(text);
    for byte in 0..=text.len() {
        // Only test char boundaries; a byte mid-char has no LSP position.
        if !text.is_char_boundary(byte) {
            continue;
        }
        let p = idx.position(byte);
        let pos = Position {
            line: p.line - 1,
            character: p.utf16_col,
        };
        assert_eq!(
            position_to_byte(text, pos),
            byte,
            "byte {byte} did not round-trip"
        );
    }
}

#[test]
fn position_to_byte_clamps_out_of_range() {
    let text = "ab\ncd\n";
    // Character past line end clamps to the line end (before the `\n`).
    assert_eq!(
        position_to_byte(
            text,
            Position {
                line: 0,
                character: 99
            }
        ),
        2
    );
    // Line past EOF clamps to text end.
    assert_eq!(
        position_to_byte(
            text,
            Position {
                line: 50,
                character: 0
            }
        ),
        text.len()
    );
}

#[test]
fn span_to_range_maps_utf16_columns() {
    let text = "π::x\n"; // `π` is 2 bytes, 1 UTF-16 unit.
    let idx = TextIndex::new(text);
    // Span over `::x` starts at byte 2 (after the 2-byte π) => UTF-16 col 1.
    let span = Span {
        byte_start: 2,
        byte_end: 5,
        line: 0,
        column: 0,
        utf16_range: (0, 0),
    };
    let range = span_to_range(&span, &idx);
    assert_eq!(
        range.start,
        Position {
            line: 0,
            character: 1
        }
    );
    assert_eq!(
        range.end,
        Position {
            line: 0,
            character: 4
        }
    );
}

#[test]
fn cli_at_and_lsp_position_use_the_same_resolution_fixture() {
    let text = "---\nkind: scene\nstate:\n  user.bond: { type: int }\n---\n\n## Query\n\n::set{user.bond = 1}\n";
    let (doc, _) = lute_syntax::parse(text);
    let byte = text.find("user.bond").unwrap();
    let index = TextIndex::new(text);
    let position = index.position(byte);
    let lsp_byte = position_to_byte(
        text,
        Position {
            line: position.line - 1,
            character: position.utf16_col,
        },
    );
    let snapshot = lute_manifest::core::load_core_snapshot();
    let cli = lute_resolve::resolve_position(
        lute_resolve::PositionQuery {
            document: &doc,
            source: text,
            byte_offset: byte,
        },
        &lute_check::SchemaImports::default(),
        &snapshot,
    )
    .unwrap();
    let lsp = lute_resolve::resolve_position(
        lute_resolve::PositionQuery {
            document: &doc,
            source: text,
            byte_offset: lsp_byte,
        },
        &lute_check::SchemaImports::default(),
        &snapshot,
    )
    .unwrap();
    assert_eq!(cli.expected_type, lsp.expected_type);
    assert_eq!(
        cli.visible_symbols
            .iter()
            .map(|symbol| (&symbol.name, &symbol.ty))
            .collect::<Vec<_>>(),
        lsp.visible_symbols
            .iter()
            .map(|symbol| (&symbol.name, &symbol.ty))
            .collect::<Vec<_>>()
    );
}

/// S1: closing a document MUST publish an EMPTY diagnostics set for its URI.
/// LSP diagnostics are server-owned and persist in the client until replaced,
/// so without this the last analyze()'s squiggles linger after the buffer is
/// gone. Drives a real `LspService<Backend>`: initialize -> didOpen (expect a
/// non-empty publish) -> didClose (expect an empty publish for the same URI).
#[tokio::test(flavor = "current_thread")]
async fn did_close_publishes_empty_diagnostics() {
    use futures::StreamExt;
    use tower::{Service, ServiceExt};
    use tower_lsp_server::jsonrpc::Request as RpcRequest;
    use tower_lsp_server::LspService;

    let (mut service, mut socket) = LspService::new(Backend::new);
    let uri_str = "file:///t.lute";
    // A body line that matches no §4.3 rule → a guaranteed parse diagnostic.
    let text = "## Shot 1.\ngarbage prose line\n";

    let init = RpcRequest::build("initialize")
        .params(serde_json::json!({ "capabilities": {} }))
        .id(1)
        .finish();
    service.ready().await.unwrap().call(init).await.unwrap();

    let open = RpcRequest::build("textDocument/didOpen")
        .params(serde_json::json!({
            "textDocument": {
                "uri": uri_str, "languageId": "lute", "version": 1, "text": text
            }
        }))
        .finish();
    service.ready().await.unwrap().call(open).await.unwrap();
    let opened = socket.next().await.expect("didOpen should publish");
    assert_eq!(opened.method(), "textDocument/publishDiagnostics");
    let odiags = opened
        .params()
        .and_then(|p| p.get("diagnostics"))
        .and_then(|d| d.as_array())
        .expect("publish carries a diagnostics array");
    assert!(
        !odiags.is_empty(),
        "an errored open doc should publish squiggles"
    );

    let close = RpcRequest::build("textDocument/didClose")
        .params(serde_json::json!({ "textDocument": { "uri": uri_str } }))
        .finish();
    service.ready().await.unwrap().call(close).await.unwrap();
    let closed = tokio::time::timeout(std::time::Duration::from_secs(2), socket.next())
        .await
        .expect("did_close must publish (empty) diagnostics; none arrived")
        .expect("socket closed without a close-publish");
    assert_eq!(closed.method(), "textDocument/publishDiagnostics");
    let cparams = closed.params().expect("close publish carries params");
    assert_eq!(
        cparams.get("uri").and_then(|u| u.as_str()),
        Some(uri_str),
        "close publish targets the closed URI"
    );
    let cdiags = cparams
        .get("diagnostics")
        .and_then(|d| d.as_array())
        .expect("close publish carries a diagnostics array");
    assert!(
        cdiags.is_empty(),
        "closing must clear diagnostics, got {cdiags:?}"
    );
}

#[test]
fn uri_to_path_rejects_non_file_schemes() {
    use std::str::FromStr;
    // file: URI -> Some path
    let f = Uri::from_str("file:///tmp/x/doc.lute").unwrap();
    assert!(
        uri_to_path(&f).is_some(),
        "file: URI must resolve to a path"
    );
    // non-file (virtual/unsaved) schemes -> None (core-only fallback)
    for s in [
        "untitled:/repo/sub/doc.lute",
        "vscode-vfs://host/repo/doc.lute",
        "untitled:Untitled-1",
    ] {
        let u = Uri::from_str(s).unwrap();
        assert!(
            uri_to_path(&u).is_none(),
            "non-file URI {s:?} must NOT resolve to a filesystem path"
        );
    }
}

/// FINDING 1 guard: a document under a project whose plugin graph has a
/// `DependsCycle` MUST publish that resolver diagnostic as an LSP diagnostic,
/// even when the document itself is core-clean. Before the model assembly
/// path was wired through, `analyze` discarded resolver diagnostics and the
/// editor silently mis-validated against a broken project. Drives a
/// real `LspService<Backend>` end to end: initialize -> didOpen a `file://`
/// scene under a temp project with two mutually-depending plugins, then assert
/// the published set carries a `DependsCycle` diagnostic sourced "lute" at the
/// document start.
#[tokio::test(flavor = "current_thread")]
async fn analyze_publishes_project_resolver_diagnostics() {
    use futures::StreamExt;
    use std::fs;
    use tower::{Service, ServiceExt};
    use tower_lsp_server::jsonrpc::Request as RpcRequest;
    use tower_lsp_server::LspService;

    // Temp project with a plugin dependency cycle: a.x -> a.dep -> a.x.
    let root = std::env::temp_dir().join(format!("lute_lsp_cycle_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    for (id, dep) in [("a.x", "a.dep"), ("a.dep", "a.x")] {
        let pdir = root.join("plugins").join(id);
        fs::create_dir_all(&pdir).unwrap();
        fs::write(
            pdir.join("plugin.yaml"),
            format!(
                "id: {id}\nversion: 0.1.0\nkind: capability\ndepends: [ {{ id: {dep}, range: \"^0.1.0\" }} ]\nexports: {{}}\n"
            ),
        )
        .unwrap();
    }
    fs::write(
        root.join("lute.project.yaml"),
        "pluginsDir: plugins/\ndefaultProfile: s\nprofiles:\n  s:\n    plugins: { a.x: true, a.dep: true }\n",
    )
    .unwrap();
    let scene_path = root.join("scene.lute");
    let uri_str = format!("file://{}", scene_path.display());

    let (mut service, mut socket) = LspService::new(Backend::new);
    let init = RpcRequest::build("initialize")
        .params(serde_json::json!({ "capabilities": {} }))
        .id(1)
        .finish();
    service.ready().await.unwrap().call(init).await.unwrap();

    let open = RpcRequest::build("textDocument/didOpen")
        .params(serde_json::json!({
            "textDocument": {
                "uri": uri_str, "languageId": "lute", "version": 1, "text": "## Shot 1.\n"
            }
        }))
        .finish();
    service.ready().await.unwrap().call(open).await.unwrap();
    let opened = socket.next().await.expect("didOpen should publish");
    assert_eq!(opened.method(), "textDocument/publishDiagnostics");
    let diags = opened
        .params()
        .and_then(|p| p.get("diagnostics").cloned())
        .and_then(|d| d.as_array().cloned())
        .expect("publish carries a diagnostics array");
    // Keyed on the stable `E-DEPENDS-CYCLE` code, not message text: the
    // resolver's `ResolveError::Display` now renders prose ("plugin `a.x`
    // is part of a `depends` cycle"), not the `Debug` struct name this
    // lookup used to grep for (`crate::resolve::ResolveError`'s `{e:?}` →
    // `{e}` fix, mirroring `LoadError`'s identical 0.10.1 fix).
    let resolver = diags
        .iter()
        .find(|d| d.get("code").and_then(|c| c.as_str()) == Some("E-DEPENDS-CYCLE"));
    let resolver = resolver.unwrap_or_else(|| {
        panic!("resolver DependsCycle diagnostic must be published, got {diags:?}")
    });
    assert_eq!(
        resolver.get("source").and_then(|s| s.as_str()),
        Some("lute"),
        "resolver diagnostic must be sourced \"lute\""
    );
    assert_eq!(
        resolver.get("severity").and_then(|s| s.as_u64()),
        Some(1),
        "resolver diagnostic must be Error severity"
    );
    assert_eq!(
        resolver.get("code").and_then(|c| c.as_str()),
        Some("E-DEPENDS-CYCLE"),
        "resolver diagnostic must carry the stable E-DEPENDS-CYCLE code"
    );
    let start = resolver
        .get("range")
        .and_then(|r| r.get("start"))
        .expect("range.start present");
    assert_eq!(start.get("line").and_then(|l| l.as_u64()), Some(0));
    assert_eq!(start.get("character").and_then(|c| c.as_u64()), Some(0));
    fs::remove_dir_all(&root).ok();
}
