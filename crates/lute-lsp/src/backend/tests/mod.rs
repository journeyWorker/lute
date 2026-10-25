use super::document::{claimed_declaration_yaml, uri_to_path};
use super::positions::position_to_byte;
use super::yaml_spans::find_key_span;
use super::*;
use lute_core_span::{Span, TextIndex};
use tower_lsp_server::ls_types::{Position, Range, TextEdit as LspTextEdit};

mod declarations;

#[test]
fn formatting_edit_uses_cli_formatter_and_full_document_range() {
    let text = "## S\r\n@narrator: hi  \r\n";
    let cli = lute_syntax::format_source(text, &lute_syntax::FormatOptions::default()).unwrap();
    let idx = TextIndex::new(text);
    let end = idx.position(text.len());
    let edit = LspTextEdit {
        range: Range {
            start: Position {
                line: 0,
                character: 0,
            },
            end: Position {
                line: end.line - 1,
                character: end.utf16_col,
            },
        },
        new_text: cli.text.clone(),
    };
    assert_eq!(edit.new_text, cli.text);
    assert_eq!(
        edit.range.start,
        Position {
            line: 0,
            character: 0
        }
    );
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
    let root_scratch = lute_scratch::ScratchDir::new();
    let root = root_scratch.join("lute_lsp_cycle");
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
        resolver.get("severity").and_then(serde_json::Value::as_u64),
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
    assert_eq!(
        start.get("line").and_then(serde_json::Value::as_u64),
        Some(0)
    );
    assert_eq!(
        start.get("character").and_then(serde_json::Value::as_u64),
        Some(0)
    );
}

/// dsl 0.49.0 §7.1 (#175): the LSP publishes `W-PROJECT-CHECK-SKIPPED` as
/// information at the first read of a quest the document and its imports do
/// not declare, once per id.
#[tokio::test(flavor = "current_thread")]
async fn analyze_names_the_quest_references_it_cannot_judge() {
    use futures::StreamExt;
    use std::fs;
    use tower::{Service, ServiceExt};
    use tower_lsp_server::jsonrpc::Request as RpcRequest;
    use tower_lsp_server::LspService;

    let root_scratch = lute_scratch::ScratchDir::new();
    let root = root_scratch.join("lute_lsp_175");
    fs::create_dir_all(root.join("quests")).unwrap();
    fs::write(
        root.join("lute.project.yaml"),
        "defaultProfile: s\nprofiles:\n  s:\n    plugins: {}\n",
    )
    .unwrap();
    let text = "---\nkind: quest\nid: acme.quests\n---\n\n<quest id=\"q\" tier=\"run\" title=\"Q\" start=\"true\">\n  <objective id=\"c1\" title=\"Do it\" done=\"quest.gone.state == 'complete' && quest.gone.state != 'failed' && quest.q.state == 'active'\"/>\n</quest>\n";
    let path = root.join("quests/q.lute");
    fs::write(&path, text).unwrap();

    let (mut service, mut socket) = LspService::new(Backend::new);
    let init = RpcRequest::build("initialize")
        .params(serde_json::json!({ "capabilities": {} }))
        .id(1)
        .finish();
    service.ready().await.unwrap().call(init).await.unwrap();
    let open = RpcRequest::build("textDocument/didOpen")
        .params(serde_json::json!({
            "textDocument": {
                "uri": format!("file://{}", path.display()), "languageId": "lute",
                "version": 1, "text": text
            }
        }))
        .finish();
    service.ready().await.unwrap().call(open).await.unwrap();
    let opened = socket.next().await.expect("didOpen should publish");
    let diags = opened
        .params()
        .and_then(|p| p.get("diagnostics").cloned())
        .and_then(|d| d.as_array().cloned())
        .expect("publish carries a diagnostics array");
    let skipped: Vec<_> = diags
        .iter()
        .filter(|d| d.get("code").and_then(|c| c.as_str()) == Some("W-PROJECT-CHECK-SKIPPED"))
        .collect();
    assert_eq!(skipped.len(), 1, "{diags:?}");
    assert_eq!(
        skipped[0]
            .get("severity")
            .and_then(serde_json::Value::as_u64),
        Some(3),
        "information severity: {diags:?}"
    );
    assert!(
        skipped[0]
            .get("message")
            .and_then(|m| m.as_str())
            .is_some_and(|m| m.starts_with("`quest.gone.state` names quest `gone`")),
        "{diags:?}"
    );
}

/// F5-R1 remainder: a quest read in a reference interpolation's arguments
/// (`{{@ready(quest.gone.state == 'complete')}}`) is named too, at the
/// interpolation.
#[tokio::test(flavor = "current_thread")]
async fn analyze_names_a_quest_read_in_a_reference_interpolation_argument() {
    use futures::StreamExt;
    use std::fs;
    use tower::{Service, ServiceExt};
    use tower_lsp_server::jsonrpc::Request as RpcRequest;
    use tower_lsp_server::LspService;

    let root_scratch = lute_scratch::ScratchDir::new();
    let root = root_scratch.join("lute_lsp_175_ref_args");
    fs::create_dir_all(root.join("scenes")).unwrap();
    fs::write(
        root.join("lute.project.yaml"),
        "defaultProfile: s\nprofiles:\n  s:\n    plugins: {}\n",
    )
    .unwrap();
    let text = "---\nkind: scene\nid: s\ndefs:\n  ready: { type: bool, params: { p: bool }, cel: \"true\" }\n---\n\n## A {#a}\n\n@narrator: {{@ready(quest.gone.state == 'complete')}}\n";
    let path = root.join("scenes/s.lute");
    fs::write(&path, text).unwrap();

    let (mut service, mut socket) = LspService::new(Backend::new);
    let init = RpcRequest::build("initialize")
        .params(serde_json::json!({ "capabilities": {} }))
        .id(1)
        .finish();
    service.ready().await.unwrap().call(init).await.unwrap();
    let open = RpcRequest::build("textDocument/didOpen")
        .params(serde_json::json!({
            "textDocument": {
                "uri": format!("file://{}", path.display()), "languageId": "lute",
                "version": 1, "text": text
            }
        }))
        .finish();
    service.ready().await.unwrap().call(open).await.unwrap();
    let opened = socket.next().await.expect("didOpen should publish");
    let diags = opened
        .params()
        .and_then(|p| p.get("diagnostics").cloned())
        .and_then(|d| d.as_array().cloned())
        .expect("publish carries a diagnostics array");
    let skipped: Vec<_> = diags
        .iter()
        .filter(|d| d.get("code").and_then(|c| c.as_str()) == Some("W-PROJECT-CHECK-SKIPPED"))
        .collect();
    assert_eq!(skipped.len(), 1, "{diags:?}");
    assert!(
        skipped[0]
            .get("message")
            .and_then(|m| m.as_str())
            .is_some_and(|m| m.starts_with("`quest.gone.state` names quest `gone`")),
        "{diags:?}"
    );
    let start = &skipped[0]["range"]["start"];
    assert_eq!(start["line"], 9, "{diags:?}");
    assert_eq!(start["character"], 11, "at the interpolation: {diags:?}");
}

/// `(code, severity)` of each diagnostic `service` publishes on opening
/// `path` with `text`.
async fn published_on_open(
    service: &mut tower_lsp_server::LspService<Backend>,
    socket: &mut tower_lsp_server::ClientSocket,
    path: &std::path::Path,
    text: &str,
) -> Vec<(String, u64)> {
    use futures::StreamExt;
    use tower::{Service, ServiceExt};
    use tower_lsp_server::jsonrpc::Request as RpcRequest;
    let open = RpcRequest::build("textDocument/didOpen")
        .params(serde_json::json!({
            "textDocument": {
                "uri": format!("file://{}", path.display()),
                "languageId": "lute", "version": 1, "text": text
            }
        }))
        .finish();
    service.ready().await.unwrap().call(open).await.unwrap();
    let note = socket.next().await.expect("didOpen should publish");
    assert_eq!(note.method(), "textDocument/publishDiagnostics");
    note.params()
        .and_then(|p| p.get("diagnostics").cloned())
        .and_then(|d| d.as_array().cloned())
        .expect("publish carries a diagnostics array")
        .iter()
        .map(|d| {
            (
                d["code"].as_str().unwrap_or_default().to_string(),
                d["severity"].as_u64().unwrap_or_default(),
            )
        })
        .collect()
}

/// dsl 0.50.0 §6.2: the LSP publishes with the project's `diagnostics:`
/// levels — a code `off` under `paths` is not published there and is
/// published elsewhere, a re-levelled one is published at its level — and
/// an open `lute.project.yaml` publishes its `E-DIAGNOSTICS-CONFIG` faults.
#[tokio::test(flavor = "current_thread")]
async fn analyze_publishes_with_the_project_diagnostic_levels() {
    use std::fs;
    use tower::{Service, ServiceExt};
    use tower_lsp_server::jsonrpc::Request as RpcRequest;
    use tower_lsp_server::LspService;

    let root_scratch = lute_scratch::ScratchDir::new();
    let root = root_scratch.join("lute_lsp_levels");
    fs::create_dir_all(root.join("scenes/imported")).unwrap();
    let manifest = root.join("lute.project.yaml");
    let manifest_text = "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n\
                         diagnostics:\n  \
                         W-STAGE-ABSENT: { level: off, paths: [\"scenes/imported/**\"] }\n  \
                         W-TEXT-GLUE: info\n  W-WIP: silent\n";
    fs::write(&manifest, manifest_text).unwrap();
    let scene = "---\nkind: scene\nid: s\nenums:\n  action:\n    members: [show, hide]\n    \
                 exits: [hide]\n  anchor:\n    members: [left, center, right]\n    default: \
                 center\n---\n\n## Shot 1.\n\n::actor{character=\"vesna\" action=\"show\"}\n\
                 ::actor{character=\"vesna\" action=\"hide\"}\n@vesna: Gone <> now.\n";
    let imported = root.join("scenes/imported/b.lute");
    let elsewhere = root.join("scenes/a.lute");
    fs::write(&imported, scene).unwrap();
    fs::write(&elsewhere, scene).unwrap();

    let (mut service, mut socket) = LspService::new(Backend::new);
    let init = RpcRequest::build("initialize")
        .params(serde_json::json!({ "capabilities": {} }))
        .id(1)
        .finish();
    service.ready().await.unwrap().call(init).await.unwrap();

    // LSP severity: 1 error, 2 warning, 3 information.
    let got = published_on_open(&mut service, &mut socket, &imported, scene).await;
    assert_eq!(got, [("W-TEXT-GLUE".to_string(), 3)], "{got:?}");
    let mut got = published_on_open(&mut service, &mut socket, &elsewhere, scene).await;
    got.sort();
    assert_eq!(
        got,
        [
            ("W-STAGE-ABSENT".to_string(), 2),
            ("W-TEXT-GLUE".to_string(), 3)
        ],
        "{got:?}"
    );
    let got = published_on_open(&mut service, &mut socket, &manifest, manifest_text).await;
    assert_eq!(got, [("E-DIAGNOSTICS-CONFIG".to_string(), 1)], "{got:?}");
}
