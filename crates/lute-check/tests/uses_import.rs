//! Task U1 — `SchemaImports` + `CheckInput.imports` pure merge (dsl §9.2).
//! Imported state resolves like inline decls; a scene redeclaring an imported
//! tier flags `E-STATE-REDECLARE` (imported wins); import diags are surfaced.
use lute_check::meta::{Namespace, StateDecl, StateSchema};
use lute_check::resolve_imports;
use lute_check::schema_import::SchemaImports;
use lute_check::{check, CheckInput, Mode};
use lute_core_span::Span;
use lute_manifest::provider::ProviderSet;
use lute_manifest::types::{Literal, Type};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

fn check_codes(text: &str, imports: SchemaImports) -> Vec<String> {
    let input = CheckInput {
        text: text.into(),
        uri: "t".into(),
        snapshot: lute_manifest::core::load_core_snapshot(),
        providers: ProviderSet::default(),
        mode: Mode::Author,
        imports,
        components: Default::default(),
    };
    check(&input)
        .diagnostics
        .into_iter()
        .map(|d| d.code)
        .collect()
}

// Minimal valid scene reading an imported run path via <match>.
const SCENE_READS_RUN: &str = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n\
<match on=\"run.choseHelp\">\n<when test=\"$ == true\">@x: a\n</when>\n\
<otherwise>@x: b\n</otherwise>\n</match>\n";
// Same but the scene ALSO inline-declares run.x which the import owns.
const SCENE_REDECLARES: &str = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  run.x: { type: bool }\n---\n## Shot 1.\n@x: hi\n";
const MINIMAL_SCENE: &str =
    "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n@x: hi\n";

fn run_bool(default: bool) -> StateDecl {
    StateDecl {
        ty: Type::Bool,
        default: Some(Literal::Bool(default)),
        namespace: Namespace::Run,
    }
}

#[test]
fn imported_run_path_resolves_no_undeclared() {
    let mut st = StateSchema::default();
    st.decls.insert("run.choseHelp".into(), run_bool(false));
    let imports = SchemaImports {
        state: st,
        ..Default::default()
    };
    let codes = check_codes(SCENE_READS_RUN, imports);
    assert!(
        !codes.contains(&"E-UNDECLARED".to_string()),
        "imported path must resolve; got {codes:?}"
    );
}

#[test]
fn scene_override_of_imported_tier_flags_redeclare() {
    let mut st = StateSchema::default();
    st.decls.insert(
        "run.x".into(),
        StateDecl {
            ty: Type::Bool,
            default: None,
            namespace: Namespace::Run,
        },
    );
    let imports = SchemaImports {
        state: st,
        ..Default::default()
    };
    let codes = check_codes(SCENE_REDECLARES, imports);
    assert!(
        codes.contains(&"E-STATE-REDECLARE".to_string()),
        "got {codes:?}"
    );
}

#[test]
fn import_diags_are_surfaced() {
    let d = lute_core_span::Diagnostic {
        code: "E-USES-CYCLE".to_string(),
        severity: lute_core_span::Severity::Error,
        message: "synthetic".to_string(),
        span: lute_core_span::Span {
            byte_start: 0,
            byte_end: 0,
            line: 1,
            column: 1,
            utf16_range: (0, 0),
        },
        layer: lute_core_span::Layer::Content,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
    };
    let imports = SchemaImports {
        diags: vec![d],
        ..Default::default()
    };
    let codes = check_codes(MINIMAL_SCENE, imports);
    assert!(codes.contains(&"E-USES-CYCLE".to_string()));
}

// --- Task U2 — `resolve_imports` DAG resolver (dsl §9.2) ---

fn zero_span() -> Span {
    Span {
        byte_start: 0,
        byte_end: 0,
        line: 1,
        column: 1,
        utf16_range: (0, 0),
    }
}

static UNIQ: AtomicU64 = AtomicU64::new(0);

/// A fresh temp dir per call; schema `.lute` files are written into it.
fn unique_dir() -> PathBuf {
    let n = UNIQ.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir =
        std::env::temp_dir().join(format!("lute_uses_{}_{}_{}", std::process::id(), n, nanos));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_lute(dir: &Path, name: &str, body: &str) {
    std::fs::write(dir.join(name), body).unwrap();
}

fn resolve_codes(res: &SchemaImports) -> Vec<&str> {
    res.diags.iter().map(|d| d.code.as_str()).collect()
}

#[test]
fn resolves_single_import() {
    let dir = unique_dir();
    write_lute(
        &dir,
        "schema.lute",
        "---\nstate:\n  run.x: { type: bool, default: false }\n---\n",
    );
    let res = resolve_imports(&dir, &["schema.lute".to_string()], &[], zero_span());
    assert!(res.diags.is_empty(), "unexpected diags: {:?}", res.diags);
    assert!(
        res.state.decls.contains_key("run.x"),
        "run.x missing: {:?}",
        res.state.decls.keys().collect::<Vec<_>>()
    );
}

// --- Task B2 — `.yaml`/`.yml` targets resolve as pure declaration maps
// (data-catalog foundation 0.3.0 draft): no `---` envelope, no body -- the
// whole file IS the declaration, merged identically to `.schema.lute`.

#[test]
fn resolves_single_yaml_import() {
    let dir = unique_dir();
    write_lute(
        &dir,
        "schema.yaml",
        "state:\n  run.x: { type: bool, default: false }\ndefs:\n  greeting: hi\n",
    );
    let res = resolve_imports(&dir, &["schema.yaml".to_string()], &[], zero_span());
    assert!(res.diags.is_empty(), "unexpected diags: {:?}", res.diags);
    assert!(
        res.state.decls.contains_key("run.x"),
        "run.x missing: {:?}",
        res.state.decls.keys().collect::<Vec<_>>()
    );
    assert_eq!(
        res.defs.get("greeting").and_then(|v| v.as_str()),
        Some("hi"),
        "greeting def missing or wrong: {:?}",
        res.defs
    );
}

#[test]
fn yaml_import_lifts_enums_and_entities_domains() {
    let dir = unique_dir();
    write_lute(
        &dir,
        "schema.yml",
        "enums:\n  action: [wave, bow]\nentities:\n  character: { members: [shadowheart, halsin] }\n",
    );
    let res = resolve_imports(&dir, &["schema.yml".to_string()], &[], zero_span());
    assert!(res.diags.is_empty(), "unexpected diags: {:?}", res.diags);
    let action = res
        .domains
        .get("action")
        .unwrap_or_else(|| panic!("action domain missing: {:?}", res.domains.keys().collect::<Vec<_>>()));
    assert_eq!(action.members, vec!["wave".to_string(), "bow".to_string()]);
    let character = res
        .domains
        .get("character")
        .expect("character domain missing");
    assert_eq!(
        character.members,
        vec!["shadowheart".to_string(), "halsin".to_string()]
    );
}

#[test]
fn schema_lute_import_still_resolves_alongside_yaml_support() {
    // Regression guard: `.schema.lute` targets keep working unchanged (B2 is
    // strictly additive) after the resolver learns `.yaml`/`.yml`.
    let dir = unique_dir();
    write_lute(
        &dir,
        "schema.lute",
        "---\nstate:\n  run.y: { type: bool, default: true }\n---\n",
    );
    let res = resolve_imports(&dir, &["schema.lute".to_string()], &[], zero_span());
    assert!(res.diags.is_empty(), "unexpected diags: {:?}", res.diags);
    assert!(
        res.state.decls.contains_key("run.y"),
        "run.y missing: {:?}",
        res.state.decls.keys().collect::<Vec<_>>()
    );
}

#[test]
fn cycle_is_e_uses_cycle() {
    let dir = unique_dir();
    write_lute(&dir, "a.lute", "---\nuses: b.lute\n---\n");
    write_lute(&dir, "b.lute", "---\nuses: a.lute\n---\n");
    let res = resolve_imports(&dir, &["a.lute".to_string()], &[], zero_span());
    let codes = resolve_codes(&res);
    assert!(
        codes.contains(&"E-USES-CYCLE"),
        "expected E-USES-CYCLE, got {codes:?}"
    );
}

#[test]
fn dup_def_across_imports_errors() {
    let dir = unique_dir();
    write_lute(&dir, "x.lute", "---\ndefs:\n  foo: 1\n---\n");
    write_lute(&dir, "y.lute", "---\ndefs:\n  foo: 2\n---\n");
    write_lute(&dir, "a.lute", "---\nuses: [x.lute, y.lute]\n---\n");
    let res = resolve_imports(&dir, &["a.lute".to_string()], &[], zero_span());
    let codes = resolve_codes(&res);
    assert!(
        codes.contains(&"E-USES-DUP-DEF"),
        "expected E-USES-DUP-DEF, got {codes:?}"
    );
}

#[test]
fn missing_file_is_not_found() {
    let dir = unique_dir();
    let res = resolve_imports(&dir, &["nope.lute".to_string()], &[], zero_span());
    let codes = resolve_codes(&res);
    assert!(
        codes.contains(&"E-USES-NOT-FOUND"),
        "expected E-USES-NOT-FOUND, got {codes:?}"
    );
}

#[test]
fn diamond_is_one_identity_no_dup() {
    let dir = unique_dir();
    write_lute(&dir, "d.lute", "---\ndefs:\n  x: 1\n---\n");
    write_lute(&dir, "b.lute", "---\nuses: d.lute\n---\n");
    write_lute(&dir, "c.lute", "---\nuses: d.lute\n---\n");
    write_lute(&dir, "a.lute", "---\nuses: [b.lute, c.lute]\n---\n");
    let res = resolve_imports(&dir, &["a.lute".to_string()], &[], zero_span());
    let codes = resolve_codes(&res);
    assert!(
        !codes.contains(&"E-USES-DUP-DEF"),
        "unexpected E-USES-DUP-DEF for diamond: {codes:?}"
    );
    assert!(
        res.defs.contains_key("x"),
        "def x missing: {:?}",
        res.defs.keys().collect::<Vec<_>>()
    );
    assert_eq!(
        res.defs.len(),
        1,
        "expected exactly one def: {:?}",
        res.defs
    );
}

#[test]
fn malformed_schema_is_e_uses_parse() {
    let dir = unique_dir();
    // Valid frontmatter, but the BODY has an unterminated `/* … */` block
    // comment -> `lute_syntax::parse` emits E-COMMENT-UNTERMINATED in its parse
    // diagnostics (pdiags). Before the fix those were dropped, so the malformed
    // import was silently treated as empty (no E-USES-PARSE).
    write_lute(
        &dir,
        "bad.lute",
        "---\nstate:\n  run.x: { type: bool, default: false }\n---\n/* unterminated",
    );
    let out = resolve_imports(&dir, &["bad.lute".to_string()], &[], zero_span());
    let codes = resolve_codes(&out);
    assert!(
        codes.contains(&"E-USES-PARSE"),
        "malformed schema must flag E-USES-PARSE; got {codes:?}"
    );
}

// --- FEAT-2 — `extends:` base composition with override (dsl §9.2) ---

#[test]
fn extends_overrides_base_def() {
    let dir = unique_dir();
    write_lute(
        &dir,
        "base.lute",
        "---\ndefs:\n  helped: { type: bool, cel: \"false\" }\n---\n",
    );
    write_lute(
        &dir,
        "child.lute",
        "---\nextends: base.lute\ndefs:\n  helped: { type: bool, cel: \"true\" }\n---\n",
    );
    // Bring the child in as a peer; its `extends: base` lays the base BELOW it,
    // so the child's `helped` overrides the base's without a dup error.
    let res = resolve_imports(&dir, &["child.lute".to_string()], &[], zero_span());
    let codes = resolve_codes(&res);
    assert!(
        !codes.contains(&"E-USES-DUP-DEF"),
        "extends override must not dup-flag; got {codes:?}"
    );
    let helped = res.defs.get("helped").expect("helped def missing");
    let cel = helped
        .get("cel")
        .and_then(|v| v.as_str())
        .expect("cel missing");
    assert_eq!(cel, "true", "child def must override base def");
}

#[test]
fn extends_state_default_override_ok() {
    let dir = unique_dir();
    write_lute(
        &dir,
        "base.lute",
        "---\nstate:\n  run.gold: { type: number, default: 0 }\n---\n",
    );
    write_lute(
        &dir,
        "child.lute",
        "---\nextends: base.lute\nstate:\n  run.gold: { type: number, default: 5 }\n---\n",
    );
    let res = resolve_imports(&dir, &["child.lute".to_string()], &[], zero_span());
    let codes = resolve_codes(&res);
    assert!(
        codes.is_empty(),
        "a same-type default refinement must be silent; got {codes:?}"
    );
    let decl = res.state.decls.get("run.gold").expect("run.gold missing");
    assert_eq!(
        decl.default,
        Some(Literal::Num(5.0)),
        "child default must override base default"
    );
}

#[test]
fn extends_state_type_change_errors() {
    let dir = unique_dir();
    write_lute(
        &dir,
        "base.lute",
        "---\nstate:\n  run.gold: { type: number }\n---\n",
    );
    write_lute(
        &dir,
        "child.lute",
        "---\nextends: base.lute\nstate:\n  run.gold: { type: string }\n---\n",
    );
    let res = resolve_imports(&dir, &["child.lute".to_string()], &[], zero_span());
    let codes = resolve_codes(&res);
    assert!(
        codes.contains(&"E-EXTENDS-STATE-TYPE"),
        "a state override changing type must flag E-EXTENDS-STATE-TYPE; got {codes:?}"
    );
    // The override still wins: the merged decl carries the child's type.
    let decl = res.state.decls.get("run.gold").expect("run.gold missing");
    assert_eq!(
        decl.ty,
        Type::Str,
        "override decl must win despite type change"
    );
}

#[test]
fn uses_peer_dup_still_errors() {
    let dir = unique_dir();
    write_lute(&dir, "x.lute", "---\ndefs:\n  foo: 1\n---\n");
    write_lute(&dir, "y.lute", "---\ndefs:\n  foo: 2\n---\n");
    // Both peers via `uses:` at the same level -> peer dup, unchanged.
    let res = resolve_imports(
        &dir,
        &["x.lute".to_string(), "y.lute".to_string()],
        &[],
        zero_span(),
    );
    let codes = resolve_codes(&res);
    assert!(
        codes.contains(&"E-USES-DUP-DEF"),
        "two uses peers declaring the same def must dup; got {codes:?}"
    );
}

#[test]
fn extends_cycle_errors() {
    let dir = unique_dir();
    write_lute(&dir, "a.lute", "---\nextends: b.lute\n---\n");
    write_lute(&dir, "b.lute", "---\nextends: a.lute\n---\n");
    let res = resolve_imports(&dir, &[], &["a.lute".to_string()], zero_span());
    let codes = resolve_codes(&res);
    assert!(
        codes.contains(&"E-USES-CYCLE"),
        "an extends cycle must flag E-USES-CYCLE; got {codes:?}"
    );
}

// --- FEAT-2 fix wave — order-independence, base-base dup, scene-inline refine ---

#[test]
fn extends_dup_detection_is_order_independent() {
    // root uses [a, b, c]: `a extends x` reaches x at depth 1, `b uses x`
    // reaches x at depth 0 (its SHALLOWEST), and `c` is a depth-0 peer declaring
    // the same def/state as x. So `c` (depth 0) vs `x` (depth 0) is a same-depth
    // peer dup that MUST be reported identically for [a,b,c] and [c,b,a].
    let run = |order: &[&str]| -> Vec<String> {
        let dir = unique_dir();
        write_lute(
            &dir,
            "x.lute",
            "---\ndefs:\n  foo: 1\nstate:\n  run.gold: { type: number }\n---\n",
        );
        write_lute(
            &dir,
            "c.lute",
            "---\ndefs:\n  foo: 2\nstate:\n  run.gold: { type: number }\n---\n",
        );
        write_lute(&dir, "a.lute", "---\nextends: x.lute\n---\n");
        write_lute(&dir, "b.lute", "---\nuses: x.lute\n---\n");
        let uses: Vec<String> = order.iter().map(|s| format!("{s}.lute")).collect();
        let res = resolve_imports(&dir, &uses, &[], zero_span());
        let mut codes: Vec<String> = res.diags.iter().map(|d| d.code.clone()).collect();
        codes.sort();
        codes
    };
    let forward = run(&["a", "b", "c"]);
    let reverse = run(&["c", "b", "a"]);
    assert_eq!(
        forward, reverse,
        "extends dup detection must be order-independent: {forward:?} vs {reverse:?}"
    );
    assert!(
        forward.contains(&"E-USES-DUP-DEF".to_string())
            && forward.contains(&"E-USES-DUP-STATE".to_string()),
        "c (depth 0) vs x (depth 0) must be a same-depth peer dup; got {forward:?}"
    );
}

#[test]
fn base_base_dup_not_hidden_by_override() {
    // `child` (depth 0) overrides run.gold / bar AND extends TWO unrelated bases
    // A and B, both declaring run.gold / bar at depth 1. The A-vs-B same-depth
    // collision MUST still error even though the child's override wins.
    let dir = unique_dir();
    write_lute(
        &dir,
        "A.lute",
        "---\nstate:\n  run.gold: { type: number }\ndefs:\n  bar: 1\n---\n",
    );
    write_lute(
        &dir,
        "B.lute",
        "---\nstate:\n  run.gold: { type: number }\ndefs:\n  bar: 2\n---\n",
    );
    write_lute(
        &dir,
        "child.lute",
        "---\nextends: [A.lute, B.lute]\nstate:\n  run.gold: { type: number }\ndefs:\n  bar: 0\n---\n",
    );
    let res = resolve_imports(&dir, &["child.lute".to_string()], &[], zero_span());
    let codes = resolve_codes(&res);
    assert!(
        codes.contains(&"E-USES-DUP-STATE"),
        "base-base state collision must not be hidden by a child override; got {codes:?}"
    );
    assert!(
        codes.contains(&"E-USES-DUP-DEF"),
        "base-base def collision must not be hidden by a child override; got {codes:?}"
    );
    // The child override still wins the resolved value (min-depth winner).
    let decl = res.state.decls.get("run.gold").expect("run.gold missing");
    assert_eq!(decl.ty, Type::Number, "child override must win");
}

#[test]
fn scene_inline_refines_extends_base_default() {
    // A SCENE that `extends: base` and inline-refines an extends-imported state
    // path: the inline decl OVERRIDES the base (dsl §9.2) — NOT E-STATE-REDECLARE.
    let dir = unique_dir();
    write_lute(
        &dir,
        "base.lute",
        "---\nstate:\n  run.gold: { type: number, default: 0 }\n---\n",
    );
    let imports = resolve_imports(&dir, &[], &["base.lute".to_string()], zero_span());
    assert!(
        imports.state_overridable.contains("run.gold"),
        "an extends-base state path must be marked overridable; got {:?}",
        imports.state_overridable
    );

    // Same-type refine (default 0 -> 5): accepted, no redeclare, no type error.
    let refine = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  run.gold: { type: number, default: 5 }\n---\n## Shot 1.\n@x: hi\n";
    let codes = check_codes(refine, imports.clone());
    assert!(
        !codes.contains(&"E-STATE-REDECLARE".to_string()),
        "scene inline must refine an extends-base path, not redeclare; got {codes:?}"
    );
    assert!(
        !codes.contains(&"E-EXTENDS-STATE-TYPE".to_string()),
        "a same-type refinement must be silent; got {codes:?}"
    );

    // Type-change refine (number -> string): flagged E-EXTENDS-STATE-TYPE, still
    // not E-STATE-REDECLARE (a scene may override, just never change the type).
    let retype = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  run.gold: { type: string }\n---\n## Shot 1.\n@x: hi\n";
    let codes = check_codes(retype, imports.clone());
    assert!(
        codes.contains(&"E-EXTENDS-STATE-TYPE".to_string()),
        "a type-changing refinement must flag E-EXTENDS-STATE-TYPE; got {codes:?}"
    );
    assert!(
        !codes.contains(&"E-STATE-REDECLARE".to_string()),
        "a type-change must not also be E-STATE-REDECLARE; got {codes:?}"
    );

    // "Inline wins / merged default" is observable: with a NO-DEFAULT extends
    // base, an inline decl carrying a default REPLACES it, so a later read is no
    // longer maybe-unset (it would be if the imported no-default decl had won).
    let dir2 = unique_dir();
    write_lute(
        &dir2,
        "base.lute",
        "---\nstate:\n  run.gold: { type: number }\n---\n",
    );
    let imports2 = resolve_imports(&dir2, &[], &["base.lute".to_string()], zero_span());
    let reads = |state: &str| -> Vec<String> {
        let text = format!(
            "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n{state}---\n## Shot 1.\n<match on=\"run.gold\">\n<when test=\"$ == 5\">@x: a\n</when>\n</match>\n"
        );
        check_codes(&text, imports2.clone())
    };
    assert!(
        reads("").contains(&"E-MAYBE-UNSET".to_string()),
        "sanity: reading a no-default imported path must be maybe-unset"
    );
    let refined = reads("state:\n  run.gold: { type: number, default: 5 }\n");
    assert!(
        !refined.contains(&"E-MAYBE-UNSET".to_string()),
        "the inline default must win over the extends base; got {refined:?}"
    );
    assert!(
        !refined.contains(&"E-STATE-REDECLARE".to_string()),
        "refining an extends base must not redeclare; got {refined:?}"
    );
}

// --- CheckFix F4: project-wide `<quest id>` uniqueness via the import graph
// (dsl 0.2.0 §6.3) -----------------------------------------------------------

#[test]
fn sibling_imports_declaring_same_quest_id_errors() {
    // Two DIFFERENT `uses` peers declaring the same `<quest id>` collide even
    // though NEITHER is the importing document itself — quest ids are unique
    // project-wide (§6.3), not merely per document.
    let dir = unique_dir();
    write_lute(
        &dir,
        "x.lute",
        "---\nkind: quest\n---\n<quest id=\"q\">\n<objective id=\"o\" done=\"a\"/>\n</quest>\n",
    );
    write_lute(
        &dir,
        "y.lute",
        "---\nkind: quest\n---\n<quest id=\"q\">\n<objective id=\"o2\" done=\"b\"/>\n</quest>\n",
    );
    let res = resolve_imports(
        &dir,
        &["x.lute".to_string(), "y.lute".to_string()],
        &[],
        zero_span(),
    );
    let codes = resolve_codes(&res);
    assert!(
        codes.contains(&"E-QUEST-ID-DUP"),
        "expected E-QUEST-ID-DUP, got {codes:?}"
    );
    assert!(
        res.imported_quest_ids.contains_key("q"),
        "imported_quest_ids missing `q`: {:?}",
        res.imported_quest_ids
    );
}

#[test]
fn single_imported_quest_id_is_recorded_without_a_dup() {
    let dir = unique_dir();
    write_lute(
        &dir,
        "x.lute",
        "---\nkind: quest\n---\n<quest id=\"q\">\n<objective id=\"o\" done=\"a\"/>\n</quest>\n",
    );
    let res = resolve_imports(&dir, &["x.lute".to_string()], &[], zero_span());
    let codes = resolve_codes(&res);
    assert!(!codes.contains(&"E-QUEST-ID-DUP"), "{codes:?}");
    let origin = res.imported_quest_ids.get("q");
    assert!(
        origin.is_some_and(|p| p.ends_with("x.lute")),
        "expected `q` -> x.lute, got {origin:?}"
    );
}
