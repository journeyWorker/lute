use super::*;

/// B3 claim rule: only a `.yaml`/`.yml` under a discovered project's
/// `schema/` or `catalog/` subdirectory, or named `*.schema.yaml`, is
/// claimed. `lute.project.yaml` itself, a play script, a `.lute` file
/// (wrong extension, even under `schema/`), and a `.yaml` with no project
/// above it must all resolve to `None` — B3 must not claim more than the
/// declarations.
#[test]
fn claimed_declaration_yaml_claims_schema_and_catalog_dirs_only() {
    use std::fs;

    let root = std::env::temp_dir().join(format!("lute_lsp_claim_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("schema")).unwrap();
    fs::create_dir_all(root.join("catalog")).unwrap();
    fs::write(root.join("lute.project.yaml"), "defaultProfile: default\n").unwrap();

    assert_eq!(
        claimed_declaration_yaml(&root.join("schema/state.yaml")),
        Some(root.clone()),
        "a .yaml under schema/ must be claimed"
    );
    assert_eq!(
        claimed_declaration_yaml(&root.join("catalog/enums.yml")),
        Some(root.clone()),
        "a .yml under catalog/ must be claimed"
    );
    assert_eq!(
        claimed_declaration_yaml(&root.join("lute.project.yaml")),
        None,
        "the project manifest itself must NOT be claimed"
    );
    assert_eq!(
        claimed_declaration_yaml(&root.join("schema/scene.lute")),
        None,
        "a non-.yaml file under schema/ must NOT be claimed"
    );
    assert_eq!(
        claimed_declaration_yaml(&root.join("other/loose.yaml")),
        None,
        "a .yaml outside schema/catalog must NOT be claimed"
    );
    assert_eq!(
        claimed_declaration_yaml(&root.join("world.schema.yaml")),
        Some(root.clone()),
        "a `*.schema.yaml` anywhere in the project must be claimed (LF28-9)"
    );
    assert_eq!(
        claimed_declaration_yaml(&root.join("plays/p.play.yaml")),
        None,
        "a play script must NOT be claimed"
    );
    fs::remove_dir_all(&root).ok();
}

/// B3: opening a project declaration `.yaml` (under `schema/`) whose
/// `defs:` entry's `cel:` reads an undeclared state path publishes an
/// `E-UNDECLARED` diagnostic ON that same `.yaml` URI — today (pre-B3) the
/// file is unclaimed and no semantic diagnostic is ever published for it.
/// Drives a real `LspService<Backend>`: initialize -> didOpen the `.yaml`
/// under a temp project -> assert the publish for THAT uri carries the
/// undeclared-path diagnostic.
#[tokio::test(flavor = "current_thread")]
async fn analyze_declaration_yaml_flags_undeclared_cel_path() {
    use futures::StreamExt;
    use std::fs;
    use tower::{Service, ServiceExt};
    use tower_lsp_server::jsonrpc::Request as RpcRequest;
    use tower_lsp_server::LspService;

    let root = std::env::temp_dir().join(format!("lute_lsp_decl_dirty_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("schema")).unwrap();
    fs::write(
        root.join("lute.project.yaml"),
        "defaultProfile: default\nprofiles:\n  default: {}\n",
    )
    .unwrap();
    let decl_path = root.join("schema/state.yaml");
    // `run.nope` is never declared under `state:` — a bad/undeclared path.
    fs::write(
        &decl_path,
        "state:\n  run.trust: { type: int, default: 0 }\ndefs:\n  x: { type: bool, cel: \"run.nope\" }\n",
    )
    .unwrap();
    let uri_str = format!("file://{}", decl_path.display());

    let (mut service, mut socket) = LspService::new(Backend::new);
    let init = RpcRequest::build("initialize")
        .params(serde_json::json!({ "capabilities": {} }))
        .id(1)
        .finish();
    service.ready().await.unwrap().call(init).await.unwrap();

    let open = RpcRequest::build("textDocument/didOpen")
        .params(serde_json::json!({
            "textDocument": {
                "uri": uri_str, "languageId": "yaml", "version": 1,
                "text": fs::read_to_string(&decl_path).unwrap()
            }
        }))
        .finish();
    service.ready().await.unwrap().call(open).await.unwrap();
    let opened = socket.next().await.expect("didOpen should publish");
    assert_eq!(opened.method(), "textDocument/publishDiagnostics");
    let params = opened.params().expect("publish carries params");
    assert_eq!(
        params.get("uri").and_then(|u| u.as_str()),
        Some(uri_str.as_str()),
        "the diagnostic must publish ON the declaration .yaml's own URI"
    );
    let diags = params
        .get("diagnostics")
        .and_then(|d| d.as_array())
        .expect("publish carries a diagnostics array");
    let undeclared = diags.iter().find(|d| {
        d.get("code").and_then(|c| c.as_str()) == Some("E-UNDECLARED")
            && d.get("message")
                .and_then(|m| m.as_str())
                .is_some_and(|m| m.contains("run.nope"))
    });
    assert!(
        undeclared.is_some(),
        "expected an E-UNDECLARED diagnostic for `run.nope`, got {diags:?}"
    );
    assert_eq!(
        undeclared.unwrap().get("source").and_then(|s| s.as_str()),
        Some("lute")
    );
    fs::remove_dir_all(&root).ok();
}

/// B3: a clean declaration `.yaml` (every `defs:` CEL path declared)
/// publishes NO diagnostics.
#[tokio::test(flavor = "current_thread")]
async fn analyze_declaration_yaml_clean_publishes_no_diagnostics() {
    use futures::StreamExt;
    use std::fs;
    use tower::{Service, ServiceExt};
    use tower_lsp_server::jsonrpc::Request as RpcRequest;
    use tower_lsp_server::LspService;

    let root = std::env::temp_dir().join(format!("lute_lsp_decl_clean_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("schema")).unwrap();
    fs::write(
        root.join("lute.project.yaml"),
        "defaultProfile: default\nprofiles:\n  default: {}\n",
    )
    .unwrap();
    let decl_path = root.join("schema/state.yaml");
    fs::write(
        &decl_path,
        "state:\n  run.trust: { type: int, default: 0 }\ndefs:\n  x: { type: bool, cel: \"run.trust > 0\" }\n",
    )
    .unwrap();
    let uri_str = format!("file://{}", decl_path.display());

    let (mut service, mut socket) = LspService::new(Backend::new);
    let init = RpcRequest::build("initialize")
        .params(serde_json::json!({ "capabilities": {} }))
        .id(1)
        .finish();
    service.ready().await.unwrap().call(init).await.unwrap();

    let open = RpcRequest::build("textDocument/didOpen")
        .params(serde_json::json!({
            "textDocument": {
                "uri": uri_str, "languageId": "yaml", "version": 1,
                "text": fs::read_to_string(&decl_path).unwrap()
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
    assert!(
        diags.is_empty(),
        "a clean declaration .yaml must publish no diagnostics, got {diags:?}"
    );
    fs::remove_dir_all(&root).ok();
}

/// LF28-9: a `.yaml` that is no declaration (a play script) is never
/// walked as a `.lute` document — that flagged E-KIND-MISSING and one
/// E-UNCLASSIFIED per line — and a root `world.schema.yaml` is analysed
/// as the declaration it is.
#[tokio::test(flavor = "current_thread")]
async fn a_yaml_is_never_walked_as_a_lute_document() {
    use futures::StreamExt;
    use std::fs;
    use tower::{Service, ServiceExt};
    use tower_lsp_server::jsonrpc::Request as RpcRequest;
    use tower_lsp_server::LspService;

    let root = std::env::temp_dir().join(format!("lute_lsp_yaml_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("plays")).unwrap();
    fs::write(
        root.join("lute.project.yaml"),
        "defaultProfile: default\nprofiles:\n  default: {}\n",
    )
    .unwrap();
    let files = [
        ("plays/p.play.yaml", "steps:\n  - occasion: visit\n", None),
        (
            "world.schema.yaml",
            "state:\n  run.trust: { type: int, default: 0 }\ndefs:\n  x: { type: bool, cel: \"run.nope\" }\n",
            Some("E-UNDECLARED"),
        ),
    ];
    let (mut service, mut socket) = LspService::new(Backend::new);
    let init = RpcRequest::build("initialize")
        .params(serde_json::json!({ "capabilities": {} }))
        .id(1)
        .finish();
    service.ready().await.unwrap().call(init).await.unwrap();
    for (rel, text, want) in files {
        let path = root.join(rel);
        fs::write(&path, text).unwrap();
        let open = RpcRequest::build("textDocument/didOpen")
            .params(serde_json::json!({
                "textDocument": {
                    "uri": format!("file://{}", path.display()), "languageId": "yaml",
                    "version": 1, "text": text
                }
            }))
            .finish();
        service.ready().await.unwrap().call(open).await.unwrap();
        let opened = socket.next().await.expect("didOpen should publish");
        let codes: Vec<String> = opened
            .params()
            .and_then(|p| p.get("diagnostics").cloned())
            .and_then(|d| d.as_array().cloned())
            .expect("publish carries a diagnostics array")
            .iter()
            .filter_map(|d| d.get("code").and_then(|c| c.as_str()).map(String::from))
            .collect();
        assert_eq!(
            codes,
            want.map(String::from).into_iter().collect::<Vec<_>>(),
            "{rel}"
        );
    }
    fs::remove_dir_all(&root).ok();
}

/// Declaration-path CEL-parse fixture helper (B3 + this locator's own
/// tests): opens `text` as a claimed `schema/state.yaml`, returns every
/// published `E-CEL-PARSE` diagnostic in PUBLISH order — the SAME order
/// `analyze_declaration` pushed them (`typed.defs`'s `BTreeMap` iterates
/// by name, ascending — never source-text order).
async fn open_cel_parse_fixture(text: &str) -> (Vec<serde_json::Value>, std::path::PathBuf) {
    use futures::StreamExt;
    use std::fs;
    use tower::{Service, ServiceExt};
    use tower_lsp_server::jsonrpc::Request as RpcRequest;
    use tower_lsp_server::LspService;

    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "lute_lsp_decl_cel_parse_{}_{n}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("schema")).unwrap();
    fs::write(
        root.join("lute.project.yaml"),
        "defaultProfile: default\nprofiles:\n  default: {}\n",
    )
    .unwrap();
    let decl_path = root.join("schema/state.yaml");
    fs::write(&decl_path, text).unwrap();
    let uri_str = format!("file://{}", decl_path.display());

    let (mut service, mut socket) = LspService::new(Backend::new);
    let init = RpcRequest::build("initialize")
        .params(serde_json::json!({ "capabilities": {} }))
        .id(1)
        .finish();
    service.ready().await.unwrap().call(init).await.unwrap();

    let open = RpcRequest::build("textDocument/didOpen")
        .params(serde_json::json!({
            "textDocument": {
                "uri": uri_str, "languageId": "yaml", "version": 1,
                "text": text
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
    let cel_parse: Vec<serde_json::Value> = diags
        .into_iter()
        .filter(|d| d.get("code").and_then(|c| c.as_str()) == Some("E-CEL-PARSE"))
        .collect();
    (cel_parse, root)
}

/// `(line, character)` start/end pair for byte range `[byte_start,
/// byte_end)` of `text`, through the SAME `TextIndex`/`byte_to_position`
/// conversion the backend itself uses for every published range.
fn range_at(text: &str, byte_start: usize, byte_end: usize) -> ((u64, u64), (u64, u64)) {
    let idx = TextIndex::new(text);
    let s = byte_to_position(byte_start, &idx);
    let e = byte_to_position(byte_end, &idx);
    (
        (s.line as u64, s.character as u64),
        (e.line as u64, e.character as u64),
    )
}

/// [`range_at`] for the FIRST occurrence of `needle` in `text`.
fn expect_range(text: &str, needle: &str) -> ((u64, u64), (u64, u64)) {
    let start = text
        .find(needle)
        .unwrap_or_else(|| panic!("fixture must contain {needle:?}: {text}"));
    range_at(text, start, start + needle.len())
}

fn actual_range(diag: &serde_json::Value) -> ((u64, u64), (u64, u64)) {
    let range = diag.get("range").expect("diagnostic carries a range");
    (
        (
            range["start"]["line"].as_u64().unwrap(),
            range["start"]["character"].as_u64().unwrap(),
        ),
        (
            range["end"]["line"].as_u64().unwrap(),
            range["end"]["character"].as_u64().unwrap(),
        ),
    )
}

/// §8.1 leak-fix fold-in (flagged by Task 13 as out-of-scope for
/// `lute-check`, closed here): a declaration `.yaml`'s `defs:` `cel:`
/// body that fails to parse must publish an `E-CEL-PARSE` message with
/// NONE of the embedded backend's own ANTLR vocabulary — the exact
/// `no_backend_vocabulary_ever` contract `lute-check/tests/cel_message.rs`
/// pins for the main `check()` path, now pinned for this SECOND
/// construction site too. `run.act = 1` is T2's own bare-`=` fixture
/// (`cel_message.rs` rule 4): a real parse failure `lute_cel::parse_slot`
/// rejects, translated through the SAME `translate_cel_parse` (Task 15's
/// fold-in of the T13 finding) instead of `e.message`.
#[tokio::test(flavor = "current_thread")]
async fn analyze_declaration_cel_parse_error_has_no_antlr_vocabulary() {
    // `run.act = 1` is a bare-`=` CEL parse failure (T2 rule 4) — CEL
    // wants `==`, so this never parses.
    let text = "state:\n  run.act: { type: number, default: 0 }\ndefs:\n  x: { type: bool, cel: \"run.act = 1\" }\n";
    let (cel_parse, root) = open_cel_parse_fixture(text).await;
    let hit = cel_parse
        .first()
        .unwrap_or_else(|| panic!("expected an E-CEL-PARSE diagnostic"));
    let message = hit
        .get("message")
        .and_then(|m| m.as_str())
        .expect("E-CEL-PARSE carries a message");
    for tok in [
        "viable alternative",
        "token recognition",
        "mismatched input",
        "extraneous input",
        "no viable",
    ] {
        assert!(
            !message.contains(tok),
            "declaration-path E-CEL-PARSE leaked backend vocabulary {tok:?}: {message}"
        );
    }
    assert!(
        message.contains("did you mean"),
        "expected the writer-voiced T2 bare-`=` suggestion, got: {message}"
    );
    // Finding 7 (flipped, this locator's own fix): the range now
    // anchors on the WHOLE `cel:` value scalar span (quotes included),
    // via `find_def_cel_value_span` — never the `defs:` key span, and
    // never a `translate_cel_parse`-rebased offset.
    assert_eq!(
        actual_range(hit),
        expect_range(text, "\"run.act = 1\""),
        "E-CEL-PARSE range must equal the whole `cel:` value scalar span"
    );
    std::fs::remove_dir_all(&root).ok();
}

/// Block-style `cel:` (`x:\n  type: ...\n  cel: "..."`, no inline flow
/// mapping) must anchor on the same whole-scalar span the inline flow
/// form does — the locator is YAML-aware, not tied to one entry shape.
#[tokio::test(flavor = "current_thread")]
async fn analyze_declaration_cel_parse_error_anchors_block_style_cel_value() {
    let text = "state:\n  run.act: { type: number, default: 0 }\ndefs:\n  x:\n    type: bool\n    cel: \"run.act = 1\"\n";
    let (cel_parse, root) = open_cel_parse_fixture(text).await;
    let hit = cel_parse
        .first()
        .unwrap_or_else(|| panic!("expected an E-CEL-PARSE diagnostic"));
    assert_eq!(
        actual_range(hit),
        expect_range(text, "\"run.act = 1\""),
        "block-style `cel:` must anchor on its own scalar value span"
    );
    std::fs::remove_dir_all(&root).ok();
}

/// Two defs sharing byte-identical `cel:` text: the locator is scoped to
/// EACH def's own `defs.<name>.cel` entry, so it must anchor on the
/// occurrence structurally inside `name`'s own entry — never the first
/// textual occurrence in the file (a naive substring search would pick
/// `b`'s span for BOTH diagnostics). `defs` iterates by name (`a` <
/// `b`), so `a`'s diagnostic — even though `a` is defined SECOND in
/// source — publishes first.
#[tokio::test(flavor = "current_thread")]
async fn analyze_declaration_cel_parse_error_picks_right_duplicate_occurrence() {
    let text = "state:\n  run.act: { type: number, default: 0 }\ndefs:\n  b: { type: bool, cel: \"run.act = 1\" }\n  a: { type: bool, cel: \"run.act = 1\" }\n";
    let (cel_parse, root) = open_cel_parse_fixture(text).await;
    assert_eq!(
        cel_parse.len(),
        2,
        "both `a` and `b` must fail to parse: {cel_parse:?}"
    );
    let needle = "\"run.act = 1\"";
    let first_occ = text.find(needle).expect("fixture contains the cel text");
    let second_occ = text[first_occ + needle.len()..]
        .find(needle)
        .map(|p| p + first_occ + needle.len())
        .expect("fixture contains the cel text twice");
    assert_ne!(first_occ, second_occ);
    assert_eq!(
        actual_range(&cel_parse[0]),
        range_at(text, second_occ, second_occ + needle.len()),
        "`a` (defined second in source) must anchor on its own occurrence, not `b`'s"
    );
    assert_eq!(
        actual_range(&cel_parse[1]),
        range_at(text, first_occ, first_occ + needle.len()),
        "`b` (defined first in source) must anchor on its own occurrence"
    );
    std::fs::remove_dir_all(&root).ok();
}

/// A double-quoted `cel:` scalar containing an escaped inner quote
/// (`cel: "run.act = \"x\""`) still fails the SAME bare-`=` rule; the
/// locator must skip over the `\"` escapes rather than stopping at the
/// first one, so the anchored span covers the WHOLE outer-quoted
/// scalar (both escaped inner quotes included), the message stays
/// ANTLR-free, and the diagnostic carries no fixit — `E-CEL-PARSE` here
/// is message-only from `translate_cel_parse`, per the decoded-vs-source
/// offset gap this locator exists to route around (never `t.span`/
/// `t.fixits`, both offsets into the DECODED `raw`).
#[tokio::test(flavor = "current_thread")]
async fn analyze_declaration_cel_parse_error_escaped_scalar_whole_span_no_fixit() {
    use futures::StreamExt;
    use std::fs;
    use tower::{Service, ServiceExt};
    use tower_lsp_server::jsonrpc::Request as RpcRequest;
    use tower_lsp_server::LspService;

    let text = "state:\n  run.act: { type: number, default: 0 }\ndefs:\n  x: { type: bool, cel: \"run.act = \\\"x\\\"\" }\n";
    let needle = "\"run.act = \\\"x\\\"\"";
    assert!(
        text.contains(needle),
        "sanity: fixture must contain the escaped scalar literally: {text}"
    );

    let root = std::env::temp_dir().join(format!(
        "lute_lsp_decl_cel_parse_escaped_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("schema")).unwrap();
    fs::write(
        root.join("lute.project.yaml"),
        "defaultProfile: default\nprofiles:\n  default: {}\n",
    )
    .unwrap();
    let decl_path = root.join("schema/state.yaml");
    fs::write(&decl_path, text).unwrap();
    let uri_str = format!("file://{}", decl_path.display());

    let (mut service, mut socket) = LspService::new(Backend::new);
    let init = RpcRequest::build("initialize")
        .params(serde_json::json!({ "capabilities": {} }))
        .id(1)
        .finish();
    service.ready().await.unwrap().call(init).await.unwrap();

    let open = RpcRequest::build("textDocument/didOpen")
        .params(serde_json::json!({
            "textDocument": {
                "uri": uri_str, "languageId": "yaml", "version": 1,
                "text": text
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
    let cel_parse = diags
        .iter()
        .find(|d| d.get("code").and_then(|c| c.as_str()) == Some("E-CEL-PARSE"))
        .unwrap_or_else(|| panic!("expected an E-CEL-PARSE diagnostic, got {diags:?}"));
    let message = cel_parse
        .get("message")
        .and_then(|m| m.as_str())
        .expect("carries a message");
    for tok in [
        "viable alternative",
        "token recognition",
        "mismatched input",
        "extraneous input",
        "no viable",
    ] {
        assert!(
            !message.contains(tok),
            "leaked backend vocabulary {tok:?}: {message}"
        );
    }
    assert!(
        message.contains("did you mean"),
        "expected the T2 bare-`=` suggestion, got: {message}"
    );
    assert_eq!(
        actual_range(cel_parse),
        expect_range(text, needle),
        "must anchor on the WHOLE outer-quoted scalar, escapes included"
    );

    // Fixits stay empty: `code_action` maps ONLY cached `fixits` to
    // `CodeAction`s, so an overlapping request returning nothing proves
    // this `E-CEL-PARSE` carried none.
    let range = cel_parse.get("range").cloned().expect("range");
    let code_action_req = RpcRequest::build("textDocument/codeAction")
        .params(serde_json::json!({
            "textDocument": { "uri": uri_str },
            "range": range,
            "context": { "diagnostics": [] }
        }))
        .id(2)
        .finish();
    let resp = service
        .ready()
        .await
        .unwrap()
        .call(code_action_req)
        .await
        .unwrap();
    let actions = resp.and_then(|r| r.result().cloned());
    assert!(
        matches!(actions, None | Some(serde_json::Value::Null))
            || actions
                .as_ref()
                .and_then(|a| a.as_array())
                .map(|a| a.is_empty())
                .unwrap_or(false),
        "an E-CEL-PARSE diagnostic must carry no fixit-derived code action: {actions:?}"
    );

    fs::remove_dir_all(&root).ok();
}

/// A `cel: |` literal block scalar resolves to its OWN content-lines
/// span (not the `defs:` key span): the chomping indicator plays no
/// part in locating source lines, only in decoding them, so the
/// anchored range is simply every line more-indented than `cel:`
/// itself.
#[tokio::test(flavor = "current_thread")]
async fn analyze_declaration_cel_parse_error_anchors_literal_block_scalar_content() {
    let text = "state:\n  run.act: { type: number, default: 0 }\ndefs:\n  x:\n    type: bool\n    cel: |\n      run.act = 1\n";
    let (cel_parse, root) = open_cel_parse_fixture(text).await;
    let hit = cel_parse
        .first()
        .unwrap_or_else(|| panic!("expected an E-CEL-PARSE diagnostic"));
    let content_idx = text
        .find("run.act = 1")
        .expect("fixture contains the block content");
    let line_start = text[..content_idx].rfind('\n').map(|p| p + 1).unwrap_or(0);
    let line_end = text[content_idx..]
        .find('\n')
        .map(|p| content_idx + p)
        .unwrap_or(text.len());
    assert_eq!(
        actual_range(hit),
        range_at(text, line_start, line_end),
        "a `|` block scalar must anchor on its own content-lines span"
    );
    std::fs::remove_dir_all(&root).ok();
}

/// Same as the literal (`|`) case, for the FOLDED (`>`) indicator —
/// proving the locator isn't special-cased to one of the two block
/// scalar styles.
#[tokio::test(flavor = "current_thread")]
async fn analyze_declaration_cel_parse_error_anchors_folded_block_scalar_content() {
    let text = "state:\n  run.act: { type: number, default: 0 }\ndefs:\n  x:\n    type: bool\n    cel: >\n      run.act = 1\n";
    let (cel_parse, root) = open_cel_parse_fixture(text).await;
    let hit = cel_parse
        .first()
        .unwrap_or_else(|| panic!("expected an E-CEL-PARSE diagnostic"));
    let content_idx = text
        .find("run.act = 1")
        .expect("fixture contains the block content");
    let line_start = text[..content_idx].rfind('\n').map(|p| p + 1).unwrap_or(0);
    let line_end = text[content_idx..]
        .find('\n')
        .map(|p| content_idx + p)
        .unwrap_or(text.len());
    assert_eq!(
        actual_range(hit),
        range_at(text, line_start, line_end),
        "a `>` folded block scalar must anchor on its own content-lines span"
    );
    std::fs::remove_dir_all(&root).ok();
}

/// Rock-solid fallback for a GENUINELY unresolvable entry: an anchor
/// tag (`&x_anchor`) sits between `x:`'s colon and its flow mapping's
/// `{`, a shape [`find_entry_extent`] doesn't recognize (its `{}`
/// detection expects the flow mapping to start immediately). The
/// locator declines rather than guess, and the diagnostic falls back to
/// the honest `defs:` KEY span exactly like the pre-locator behavior —
/// `find_key_span` itself is untouched by the anchor tag, since it only
/// looks for `x` immediately followed by `:`.
#[tokio::test(flavor = "current_thread")]
async fn analyze_declaration_cel_parse_error_falls_back_to_key_span_for_unresolvable_entry() {
    let text = "state:\n  run.act: { type: number, default: 0 }\ndefs:\n  x: &x_anchor { type: bool, cel: \"run.act = 1\" }\n";
    let (cel_parse, root) = open_cel_parse_fixture(text).await;
    let hit = cel_parse
        .first()
        .unwrap_or_else(|| panic!("expected an E-CEL-PARSE diagnostic"));
    let key_span = find_key_span(text, "x").expect("`x` is a `defs:` key in the fixture");
    assert_eq!(
        actual_range(hit),
        range_at(text, key_span.byte_start, key_span.byte_end),
        "an anchor-tagged entry value must fall back to the `defs:` key span"
    );
    std::fs::remove_dir_all(&root).ok();
}
