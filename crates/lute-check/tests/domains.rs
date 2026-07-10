//! Task A3 — project-authored `enums:`/`entities:` declarations feed the
//! merged domain vocabulary (dsl data-catalog foundation, 0.3.0 draft §3.1).
//! A schema doc's `enums:`/`entities:` lift into `SchemaImports.domains`
//! exactly like `state:`/`defs:` (`resolve_imports`); `merge_domains` unions
//! that with the plugin/core baseline (`CapabilitySnapshot.domains`, A2),
//! reusing `E-DOMAIN-DUP` (A2) on a cross-source name collision. Value-level
//! membership validation (an attr typed `{domain: action}` accepting/
//! rejecting a value) is A4 — out of scope here.
use lute_check::resolve_imports;
use lute_check::schema_import::merge_domains;
use lute_check::{check, CheckInput, Mode};
use lute_core_span::Span;
use lute_manifest::core::load_core_snapshot;
use lute_manifest::provider::ProviderSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

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
/// Mirrors `uses_import.rs`'s helper of the same shape.
fn unique_dir() -> PathBuf {
    let n = UNIQ.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir =
        std::env::temp_dir().join(format!("lute_domains_{}_{}_{}", std::process::id(), n, nanos));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_lute(dir: &Path, name: &str, body: &str) {
    std::fs::write(dir.join(name), body).unwrap();
}

/// Step 1 (failing-first) assertion: a project schema declaring
/// `enums: { action: [wave, bow] }` is visible — as a `Domain` with members
/// `[wave, bow]` — in the merged vocabulary the checker consults, via
/// `SchemaImports.domains` (the same lift path `state:`/`defs:` already use).
#[test]
fn project_enum_domain_is_visible_in_schema_imports() {
    let dir = unique_dir();
    write_lute(&dir, "schema.lute", "---\nenums:\n  action: [wave, bow]\n---\n");
    let res = resolve_imports(&dir, &["schema.lute".to_string()], &[], zero_span());
    assert!(res.diags.is_empty(), "unexpected diags: {:?}", res.diags);
    let action = res
        .domains
        .get("action")
        .unwrap_or_else(|| panic!("action domain missing: {:?}", res.domains.keys().collect::<Vec<_>>()));
    assert_eq!(action.members, vec!["wave".to_string(), "bow".to_string()]);
    assert!(!action.open);
}

/// `entities: { <kind>: { members: [...] } }` lifts as a closed domain too.
#[test]
fn project_entities_closed_members_domain_is_visible() {
    let dir = unique_dir();
    write_lute(
        &dir,
        "schema.lute",
        "---\nentities:\n  character: { members: [shadowheart, halsin] }\n---\n",
    );
    let res = resolve_imports(&dir, &["schema.lute".to_string()], &[], zero_span());
    assert!(res.diags.is_empty(), "unexpected diags: {:?}", res.diags);
    let character = res.domains.get("character").expect("character domain missing");
    assert_eq!(
        character.members,
        vec!["shadowheart".to_string(), "halsin".to_string()]
    );
    assert!(!character.open);
}

/// `entities: { <kind>: { open: engine } }` lifts as an OPEN domain: no
/// static member list, `open == true` — the minimal registry-style flag A4
/// treats as always-accept.
#[test]
fn project_entities_open_engine_domain_is_open() {
    let dir = unique_dir();
    write_lute(&dir, "schema.lute", "---\nentities:\n  npc: { open: engine }\n---\n");
    let res = resolve_imports(&dir, &["schema.lute".to_string()], &[], zero_span());
    assert!(res.diags.is_empty(), "unexpected diags: {:?}", res.diags);
    let npc = res.domains.get("npc").expect("npc domain missing");
    assert!(npc.open);
    assert!(npc.members.is_empty());
}

/// Two `uses` peers declaring the SAME domain name is a cross-source
/// collision — reuses A2's `E-DOMAIN-DUP` (not a new `E-USES-DUP-*` code).
#[test]
fn domain_declared_by_two_peers_is_e_domain_dup() {
    let dir = unique_dir();
    write_lute(&dir, "x.lute", "---\nenums:\n  mood: [calm]\n---\n");
    write_lute(&dir, "y.lute", "---\nenums:\n  mood: [tense]\n---\n");
    write_lute(&dir, "a.lute", "---\nuses: [x.lute, y.lute]\n---\n");
    let res = resolve_imports(&dir, &["a.lute".to_string()], &[], zero_span());
    let codes: Vec<&str> = res.diags.iter().map(|d| d.code.as_str()).collect();
    assert!(
        codes.contains(&"E-DOMAIN-DUP"),
        "expected E-DOMAIN-DUP, got {codes:?}"
    );
}

/// `merge_domains` unions a project schema's domains with the plugin/core
/// baseline — the ACTUAL "merged domain vocabulary the checker consults"
/// (A2's `snap.domains` ∪ A3's `SchemaImports.domains`), with no dup when the
/// names are distinct.
#[test]
fn merge_domains_unions_project_with_core() {
    let dir = unique_dir();
    write_lute(&dir, "schema.lute", "---\nenums:\n  action: [wave, bow]\n---\n");
    let imports = resolve_imports(&dir, &["schema.lute".to_string()], &[], zero_span());
    let snapshot = load_core_snapshot();
    // Core ships no "action" domain (only emotion/mood/volume/anchor/vfxType/musicAction).
    assert!(!snapshot.domains.contains_key("action"));
    let (merged, diags) = merge_domains(&snapshot, &imports, zero_span());
    assert!(diags.is_empty(), "unexpected diags: {diags:?}");
    assert_eq!(
        merged.get("action").map(|d| d.members.clone()),
        Some(vec!["wave".to_string(), "bow".to_string()])
    );
    // Union, not replace: the core baseline domains are still present.
    assert!(merged.contains_key("emotion"));
}

/// A project schema declaring a domain name that already exists in the
/// plugin/core vocabulary is a plugin/project clash — `E-DOMAIN-DUP`, core
/// wins (never a silent shadow of the fixed-core vocabulary).
#[test]
fn merge_domains_flags_clash_with_core_domain() {
    let dir = unique_dir();
    write_lute(&dir, "schema.lute", "---\nenums:\n  emotion: [rogue]\n---\n");
    let imports = resolve_imports(&dir, &["schema.lute".to_string()], &[], zero_span());
    let snapshot = load_core_snapshot();
    let core_emotion_members = snapshot.domains["emotion"].members.clone();
    let (merged, diags) = merge_domains(&snapshot, &imports, zero_span());
    let codes: Vec<&str> = diags.iter().map(|d| d.code.as_str()).collect();
    assert!(
        codes.contains(&"E-DOMAIN-DUP"),
        "expected E-DOMAIN-DUP, got {codes:?}"
    );
    // Core wins: the project's conflicting member list is dropped.
    assert_eq!(merged["emotion"].members, core_emotion_members);
}

/// End-to-end through the real check pipeline: a scene `uses:` a schema
/// declaring `enums: { action: [wave, bow] }` checks clean (no
/// `E-META-UNKNOWN-KEY`/`E-USES-PARSE` from the new frontmatter keys), and
/// the same `input.snapshot`/`input.imports` the pipeline consumed, fed to
/// `merge_domains`, exposes `action` in the merged vocabulary.
#[test]
fn scene_uses_enum_schema_checks_clean_and_domain_is_merged() {
    let dir = unique_dir();
    write_lute(&dir, "schema.lute", "---\nenums:\n  action: [wave, bow]\n---\n");
    let text = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nuses: schema.lute\n---\n## Shot 1.\n:x: hi\n";
    let imports = resolve_imports(&dir, &["schema.lute".to_string()], &[], zero_span());
    assert!(imports.diags.is_empty(), "unexpected import diags: {:?}", imports.diags);
    let snapshot = load_core_snapshot();
    let input = CheckInput {
        text: text.into(),
        uri: "t".into(),
        snapshot,
        providers: ProviderSet::default(),
        mode: Mode::Author,
        imports: imports.clone(),
        components: Default::default(),
    };
    let result = check(&input);
    assert!(
        result.ok,
        "expected a clean check, got: {:?}",
        result.diagnostics
    );
    let (merged, diags) = merge_domains(&input.snapshot, &imports, zero_span());
    assert!(diags.is_empty());
    assert_eq!(
        merged.get("action").map(|d| d.members.clone()),
        Some(vec!["wave".to_string(), "bow".to_string()])
    );
}
