//! Task A3/A4 — project-authored `enums:`/`entities:` declarations feed the
//! merged domain vocabulary (dsl data-catalog foundation, 0.3.0 draft §3.1).
//! A schema doc's `enums:`/`entities:` lift into `SchemaImports.domains`
//! exactly like `state:`/`defs:` (`resolve_imports`); `merge_domains` unions
//! that with the plugin/core baseline (`CapabilitySnapshot.domains`, A2).
//! A cross-source PROJECT-PROJECT collision (0.3.0 T6, decision D2) no
//! longer reuses `E-DOMAIN-DUP`: a peer `enums:`/`relations:` name dup is
//! `E-USES-DUP-RELATION` and a peer `entities:` name dup is
//! `E-KIND-NAME-CLASH` (`crates/lute-check/tests/rel_compose.rs` covers the
//! full `uses`/`extends` composition surface) — `E-DOMAIN-DUP` now fires
//! ONLY for a plugin/core-vs-project or cross-plugin clash (tests below).
//! Value-level membership validation — a `{domain: X}`-typed attr
//! accepting/rejecting a value against the SAME merged view
//! (`check_attr_value`'s `Type::Domain` arm) — is A4 (tests below).
use lute_check::directives::check_directive;
use lute_check::resolve_imports;
use lute_check::schema_import::merge_domains;
use lute_check::ctx::{Ctx, Env};
use lute_check::{check, CheckInput, Mode};
use lute_core_span::Span;
use lute_manifest::core::load_core_snapshot;
use lute_manifest::provider::ProviderSet;
use lute_manifest::schema::{AttrDecl, DirectiveDecl, Lowering, ProviderDecl};
use lute_manifest::snapshot::{capability_version, CapabilitySnapshot, Domain};
use lute_manifest::types::Type;
use lute_syntax::ast::{Attr, AttrValue, Directive};
use std::collections::BTreeMap;
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

/// The inline-declaration argument for a `merge_domains` call that exercises
/// the IMPORTED route in isolation: the document declares nothing of its own.
/// (The inline route has its own section at the end of this file.)
fn no_inline() -> &'static lute_check::TypedMeta {
    static NONE: std::sync::LazyLock<lute_check::TypedMeta> =
        std::sync::LazyLock::new(lute_check::TypedMeta::default);
    &NONE
}

/// A project `enums:` declaration of the `action` slot in the dsl 0.9.0 D-D
/// long form. `action` is in `SLOT_REQUIRES_EXITS`, so a bare member list is
/// an `E-ENUM-MISSING-SEMANTICS` error; every test below that needs a
/// project-only domain reuses this one fixture.
const ACTION_SCHEMA: &str =
    "---\nenums:\n  action:\n    members: [wave, bow]\n    exits: [bow]\n---\n";

/// Step 1 (failing-first) assertion: a project schema declaring
/// `enums: { action: { members: [wave, bow], exits: [bow] } }` is visible — as
/// a `Domain` with members `[wave, bow]` — in the merged vocabulary the
/// checker consults, via `SchemaImports.domains` (the same lift path
/// `state:`/`defs:` already use). `action` is one of the dsl 0.9.0 D-D slots
/// that MUST declare its `exits:` members, so the fixture uses the long form;
/// the lift carries that declared semantics through untouched.
#[test]
fn project_enum_domain_is_visible_in_schema_imports() {
    let dir = unique_dir();
    write_lute(&dir, "schema.lute", ACTION_SCHEMA);
    let res = resolve_imports(&dir, &["schema.lute".to_string()], &[], zero_span());
    assert!(res.diags.is_empty(), "unexpected diags: {:?}", res.diags);
    let action = res
        .domains
        .get("action")
        .unwrap_or_else(|| panic!("action domain missing: {:?}", res.domains.keys().collect::<Vec<_>>()));
    assert_eq!(action.members, vec!["wave".to_string(), "bow".to_string()]);
    assert!(!action.open);
    // dsl 0.9.0 D-D: the long form's member semantics must survive the lift —
    // a projection down to a bare member list here would silently discard them.
    assert_eq!(action.exits, vec!["bow".to_string()]);
    assert_eq!(action.default, None);
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

/// Two `uses` peers declaring the SAME `enums:` name is a cross-source peer
/// collision — `E-USES-DUP-RELATION`, NOT `E-DOMAIN-DUP` (0.3.0 T6, D2; the
/// `entities:` peer-collision variant is `E-KIND-NAME-CLASH`, next test).
#[test]
fn enum_declared_by_two_peers_is_e_uses_dup_relation() {
    let dir = unique_dir();
    write_lute(&dir, "x.lute", "---\nenums:\n  mood: [calm]\n---\n");
    write_lute(&dir, "y.lute", "---\nenums:\n  mood: [tense]\n---\n");
    write_lute(&dir, "a.lute", "---\nuses: [x.lute, y.lute]\n---\n");
    let res = resolve_imports(&dir, &["a.lute".to_string()], &[], zero_span());
    let codes: Vec<&str> = res.diags.iter().map(|d| d.code.as_str()).collect();
    assert!(
        codes.contains(&"E-USES-DUP-RELATION"),
        "expected E-USES-DUP-RELATION, got {codes:?}"
    );
    assert!(!codes.contains(&"E-DOMAIN-DUP"), "D2 transition: {codes:?}");
}

/// The `entities:` counterpart: two `uses` peers declaring the SAME
/// entity-KIND name is `E-KIND-NAME-CLASH`, NOT `E-DOMAIN-DUP` (0.3.0 T6, D2).
#[test]
fn entity_kind_declared_by_two_peers_is_e_kind_name_clash() {
    let dir = unique_dir();
    write_lute(&dir, "x.lute", "---\nentities:\n  npc: { members: [ana] }\n---\n");
    write_lute(&dir, "y.lute", "---\nentities:\n  npc: { members: [bo] }\n---\n");
    write_lute(&dir, "a.lute", "---\nuses: [x.lute, y.lute]\n---\n");
    let res = resolve_imports(&dir, &["a.lute".to_string()], &[], zero_span());
    let codes: Vec<&str> = res.diags.iter().map(|d| d.code.as_str()).collect();
    assert!(
        codes.contains(&"E-KIND-NAME-CLASH"),
        "expected E-KIND-NAME-CLASH, got {codes:?}"
    );
    assert!(!codes.contains(&"E-DOMAIN-DUP"), "D2 transition: {codes:?}");
}

/// `merge_domains` unions a project schema's domains with the plugin/core
/// baseline — the ACTUAL "merged domain vocabulary the checker consults"
/// (A2's `snap.domains` ∪ A3's `SchemaImports.domains`), with no dup when the
/// names are distinct.
#[test]
fn merge_domains_unions_project_with_core() {
    let dir = unique_dir();
    write_lute(&dir, "schema.lute", ACTION_SCHEMA);
    let imports = resolve_imports(&dir, &["schema.lute".to_string()], &[], zero_span());
    // A baseline carrying exactly ONE domain, `emotion` (reusing
    // `lute_test_vocab`'s entry so its members have a single definition), and
    // deliberately NOT `action`: the union has to be observable in both
    // directions, so the project's `action` must be the only source of that
    // name while `emotion` can only come from the baseline. `vocab_snapshot()`
    // is unusable here — it ships an `action` of its own.
    let mut snapshot = load_core_snapshot();
    let emotion = lute_test_vocab::test_domains()
        .remove("emotion")
        .expect("lute_test_vocab provides an `emotion` domain");
    snapshot
        .enums
        .insert("emotion".to_string(), emotion.members.clone());
    snapshot.domains.insert("emotion".to_string(), emotion);
    snapshot.version = capability_version(&snapshot);
    assert!(!snapshot.domains.contains_key("action"));
    let (merged, diags) = merge_domains(&snapshot, &imports, no_inline(), zero_span());
    assert!(diags.is_empty(), "unexpected diags: {diags:?}");
    assert_eq!(
        merged.get("action").map(|d| d.members.clone()),
        Some(vec!["wave".to_string(), "bow".to_string()])
    );
    // Union, not replace: the snapshot's baseline domains are still present.
    assert!(merged.contains_key("emotion"));
}

/// A project schema declaring a domain name that already exists in the
/// plugin/snapshot vocabulary is a plugin/project clash — `E-DOMAIN-DUP`, the
/// snapshot wins (never a silent shadow of the vocabulary already in place).
#[test]
fn merge_domains_flags_clash_with_snapshot_domain() {
    let dir = unique_dir();
    write_lute(&dir, "schema.lute", "---\nenums:\n  emotion: [rogue]\n---\n");
    let imports = resolve_imports(&dir, &["schema.lute".to_string()], &[], zero_span());
    let snapshot = lute_test_vocab::vocab_snapshot();
    let snapshot_emotion_members = snapshot
        .domains
        .get("emotion")
        .expect("the test snapshot must provide an `emotion` domain to clash with")
        .members
        .clone();
    let (merged, diags) = merge_domains(&snapshot, &imports, no_inline(), zero_span());
    let codes: Vec<&str> = diags.iter().map(|d| d.code.as_str()).collect();
    assert!(
        codes.contains(&"E-DOMAIN-DUP"),
        "expected E-DOMAIN-DUP, got {codes:?}"
    );
    // The snapshot wins: the project's conflicting member list is dropped.
    assert_eq!(merged["emotion"].members, snapshot_emotion_members);
}

/// End-to-end through the real check pipeline: a scene `uses:` a schema
/// declaring the `action` slot (`ACTION_SCHEMA`) checks clean (no
/// `E-META-UNKNOWN-KEY`/`E-USES-PARSE` from the new frontmatter keys), and
/// the same `input.snapshot`/`input.imports` the pipeline consumed, fed to
/// `merge_domains`, exposes `action` in the merged vocabulary.
#[test]
fn scene_uses_enum_schema_checks_clean_and_domain_is_merged() {
    let dir = unique_dir();
    write_lute(&dir, "schema.lute", ACTION_SCHEMA);
    let text = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nuses: schema.lute\n---\n## Shot 1.\n@x: hi\n";
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
        defaults: Default::default(),
    };
    let result = check(&input);
    assert!(
        result.ok,
        "expected a clean check, got: {:?}",
        result.diagnostics
    );
    let (merged, diags) =
        merge_domains(&input.snapshot, &imports, no_inline(), zero_span());
    assert!(diags.is_empty());
    assert_eq!(
        merged.get("action").map(|d| d.members.clone()),
        Some(vec!["wave".to_string(), "bow".to_string()])
    );
}

// --- Task A4: `{domain: X}`-typed attr values validate against the SAME
// merged vocabulary A3 built above -- proving `merge_domains` is wired into
// the LIVE `check_directive`/`check_attr_value` path, not dead code. ---

fn ctx() -> Ctx<'static> {
    static ENV: std::sync::LazyLock<Env> = std::sync::LazyLock::new(Env::default);
    Ctx {
        env: &ENV,
        in_match: false,
        match_subject: None,
    }
}

/// Register a synthetic directive `probe` (one attr `x`, typed by the YAML
/// `type_yaml`, e.g. `"{ domain: mood }"`) on a CLONE of `snapshot`, invoke it
/// with `x="<value>"` through the REAL `check_directive` entrypoint, and
/// return the produced diagnostic codes. `domains` is threaded exactly as
/// `check()`'s real pipeline threads it (`Walker`/`validate_components`), so
/// this exercises the SAME `Type::Domain` resolution arm the live checker
/// runs, not a reimplementation.
fn codes_with_domain_attr_against(
    type_yaml: &str,
    value: &str,
    snapshot: &CapabilitySnapshot,
    domains: &BTreeMap<String, Domain>,
) -> Vec<String> {
    let ty: Type = serde_yaml::from_str(type_yaml)
        .unwrap_or_else(|e| panic!("bad type yaml `{type_yaml}`: {e}"));
    let mut snap = snapshot.clone();
    snap.directives.insert(
        "probe".to_string(),
        DirectiveDecl {
            name: "probe".to_string(),
            layer: None,
            attrs: vec![AttrDecl {
                name: "x".to_string(),
                required: false,
                ty,
                default: None,
            }],
            semantics: Vec::new(),
            state: None,
            effects: None,
            bridge: None,
            lower: Lowering::Builtin {
                kind: "builtin".to_string(),
                name: "noop".to_string(),
            },
        },
    );
    let dir = Directive {
        tag: "probe".to_string(),
        attrs: vec![Attr {
            key: "x".to_string(),
            value: AttrValue::Str(value.to_string()),
            value_span: zero_span(),
            span: zero_span(),
        }],
        span: zero_span(),
    };
    check_directive(&dir, &snap, &ProviderSet::default(), domains, &ctx())
        .into_iter()
        .map(|d| d.code)
        .collect()
}

/// Convenience for the snapshot-baseline-only cases: no project schema is in
/// play, so the merged view IS `snapshot.domains` directly (A2's baseline).
/// The baseline is `lute_test_vocab::vocab_snapshot()` — from 0.9.0 the core
/// ships no content vocabulary, so the domains these cases resolve against
/// have to come from a declared test vocabulary.
fn codes_with_domain_attr(type_yaml: &str, value: &str) -> Vec<String> {
    let snapshot = lute_test_vocab::vocab_snapshot();
    let domains = snapshot.domains.clone();
    codes_with_domain_attr_against(type_yaml, value, &snapshot, &domains)
}

#[test]
fn unknown_domain_ref_errors() {
    // an attr typed { domain: nope } -> E-DOMAIN-UNKNOWN
    assert!(codes_with_domain_attr("{ domain: nope }", "x").contains(&"E-DOMAIN-UNKNOWN".into()));
}

#[test]
fn domain_member_ok_nonmember_errors() {
    // { domain: mood } — `mood` is a closed enum-style domain the test
    // vocabulary (`lute_test_vocab`) declares in the baseline snapshot.
    assert!(!codes_with_domain_attr("{ domain: mood }", "peaceful")
        .iter()
        .any(|c| c == "E-BAD-ENUM"));
    assert!(codes_with_domain_attr("{ domain: mood }", "zzz").contains(&"E-BAD-ENUM".into()));
}

#[test]
fn project_declared_domain_validates() {
    // A schema doc declares the `action` slot (`ACTION_SCHEMA`); imported, then an
    // attr { domain: action } accepts "wave" and errors "zzz" -- proving the
    // PROJECT domain (lifted by A3's `merge_domains`, absent from core) is
    // what `check_attr_value`'s `Type::Domain` arm actually resolved against.
    let dir = unique_dir();
    write_lute(&dir, "schema.lute", ACTION_SCHEMA);
    let imports = resolve_imports(&dir, &["schema.lute".to_string()], &[], zero_span());
    assert!(imports.diags.is_empty(), "unexpected import diags: {:?}", imports.diags);
    let snapshot = load_core_snapshot();
    // Core ships no "action" domain: it can ONLY resolve via the project fold.
    assert!(!snapshot.domains.contains_key("action"));
    let (merged, diags) = merge_domains(&snapshot, &imports, no_inline(), zero_span());
    assert!(diags.is_empty(), "unexpected merge diags: {diags:?}");
    assert!(
        !codes_with_domain_attr_against("{ domain: action }", "wave", &snapshot, &merged)
            .iter()
            .any(|c| c == "E-BAD-ENUM" || c == "E-DOMAIN-UNKNOWN"),
        "`wave` is a declared `action` member; must not error"
    );
    assert!(
        codes_with_domain_attr_against("{ domain: action }", "zzz", &snapshot, &merged)
            .contains(&"E-BAD-ENUM".to_string()),
        "`zzz` is not a declared `action` member"
    );
}

/// Constraint (data-catalog foundation design): an OPEN-style domain
/// (`entities: { <kind>: { open: engine } }`, A3) is NEVER closed-checked --
/// any string is accepted, unlike a closed `enums:`/`entities.members` domain.
#[test]
fn open_domain_accepts_any_string() {
    let dir = unique_dir();
    write_lute(&dir, "schema.lute", "---\nentities:\n  npc: { open: engine }\n---\n");
    let imports = resolve_imports(&dir, &["schema.lute".to_string()], &[], zero_span());
    assert!(imports.diags.is_empty(), "unexpected import diags: {:?}", imports.diags);
    let snapshot = load_core_snapshot();
    let (merged, diags) = merge_domains(&snapshot, &imports, no_inline(), zero_span());
    assert!(diags.is_empty());
    assert!(merged["npc"].open);
    let codes = codes_with_domain_attr_against(
        "{ domain: npc }",
        "any-runtime-minted-id",
        &snapshot,
        &merged,
    );
    assert!(codes.is_empty(), "open domain must always-accept, got {codes:?}");
}

/// Regression (foundation A4 order): a CLOSED domain whose name ALSO matches
/// a declared provider (`snapshot.providers`) must resolve by the domain's
/// static `members` -- NOT the provider path. The A4 draft order let ANY
/// same-named provider win over even a closed domain; A5 reuses this
/// resolver for content-line `emotion`/`action`, so a real provider/domain
/// name collision would silently skip enum-membership checking.
///
/// No shipped core domain/provider pair collides today, so this constructs
/// the minimal artificial collision by hand: a project-declared closed
/// `action` domain (`ACTION_SCHEMA`, an A3 lift), plus a synthetic
/// `action` `ProviderDecl` inserted directly into a `snapshot.providers`
/// clone (there is no schema-level `providers:` import key to drive this
/// through `uses:`, so this mirrors how `codes_with_domain_attr_against`
/// already clones its `snapshot` arg to register a synthetic directive).
///
/// With the test's empty `ProviderSet` (`codes_with_domain_attr_against`
/// always passes `ProviderSet::default()`), the provider path resolves ANY
/// id to `E-UNKNOWN-ID` (`IdStatus::Absent`); the closed-domain path
/// resolves `zzz` to `E-BAD-ENUM` and `wave` clean. The two paths are
/// cleanly distinguishable, so this proves which one actually ran.
#[test]
fn closed_domain_membership_wins_over_same_named_provider() {
    let dir = unique_dir();
    write_lute(&dir, "schema.lute", ACTION_SCHEMA);
    let imports = resolve_imports(&dir, &["schema.lute".to_string()], &[], zero_span());
    assert!(imports.diags.is_empty(), "unexpected import diags: {:?}", imports.diags);
    let mut snapshot = load_core_snapshot();
    // Core ships no "action" domain or provider: both are exclusively this
    // test's synthetic setup, so nothing outside this test can collide.
    assert!(!snapshot.domains.contains_key("action"));
    assert!(!snapshot.providers.contains_key("action"));
    snapshot.providers.insert(
        "action".to_string(),
        ProviderDecl {
            name: "action".to_string(),
            id_shape: None,
            snapshot: "test".to_string(),
        },
    );
    let (merged, diags) = merge_domains(&snapshot, &imports, no_inline(), zero_span());
    assert!(diags.is_empty(), "unexpected merge diags: {diags:?}");
    assert_eq!(merged["action"].members, vec!["wave".to_string(), "bow".to_string()]);
    assert!(!merged["action"].open, "`enums:` lifts as a CLOSED domain");

    // A declared `action` MEMBER validates clean. Pre-fix (provider-first)
    // this ALSO fails: the provider path (empty `ProviderSet`) resolves
    // every id, including a real member, to `E-UNKNOWN-ID`.
    let ok_codes =
        codes_with_domain_attr_against("{ domain: action }", "wave", &snapshot, &merged);
    assert!(
        !ok_codes
            .iter()
            .any(|c| c == "E-BAD-ENUM" || c == "E-UNKNOWN-ID" || c == "E-DOMAIN-UNKNOWN"),
        "`wave` is a declared `action` member; must not error, got {ok_codes:?}"
    );

    // The discriminating case: a NON-member value. Provider-first order
    // (pre-fix) resolves it to `E-UNKNOWN-ID`, never `E-BAD-ENUM` --
    // this assertion FAILS under the current (A4) provider-first order.
    let bad_codes =
        codes_with_domain_attr_against("{ domain: action }", "zzz", &snapshot, &merged);
    assert!(
        bad_codes.contains(&"E-BAD-ENUM".to_string()),
        "closed-domain membership must win over the same-named provider; got {bad_codes:?}"
    );
    assert!(
        !bad_codes.iter().any(|c| c == "E-UNKNOWN-ID"),
        "provider path must NOT run for a name that resolves to a closed domain; got {bad_codes:?}"
    );
}

// --- dsl 0.9.0 D-D: member-semantics validation is PROVENANCE-aware. A
// domain reaching `merge_domains` came from either `enums:` or the
// `entities:` kind projection; only the former can carry `exits:`/`default:`,
// so the diagnostic for a kind-derived slot must name a fix that exists. ---

/// Collect the `E-ENUM-MISSING-SEMANTICS` messages `merge_domains` produces
/// for a project schema body, against the real core baseline.
fn missing_semantics_messages(body: &str) -> Vec<String> {
    missing_semantics_messages_files(&[("schema.lute", body)], &["schema.lute"])
}

/// Multi-file variant: write every `(file, body)` pair into one temp dir,
/// resolve `roots`, and collect the same messages. Needed for the
/// mixed-provenance case, where the SAME domain name must arrive from two
/// DIFFERENT imported files (one `entities:`, one `enums:`).
fn missing_semantics_messages_files(files: &[(&str, &str)], roots: &[&str]) -> Vec<String> {
    let dir = unique_dir();
    for (name, body) in files {
        write_lute(&dir, name, body);
    }
    let roots: Vec<String> = roots.iter().map(|r| (*r).to_string()).collect();
    let imports = resolve_imports(&dir, &roots, &[], zero_span());
    let snapshot = load_core_snapshot();
    let (_merged, diags) = merge_domains(&snapshot, &imports, no_inline(), zero_span());
    diags
        .iter()
        .filter(|d| d.code == "E-ENUM-MISSING-SEMANTICS")
        .map(|d| d.message.clone())
        .collect()
}

/// An `entities:` kind named `action` still lands in the merged vocabulary as
/// a closed `action` domain with no `exits:` — a real semantics loss, so it
/// MUST be diagnosed. But `EntityKindDecl` has no `exits:` key, so the
/// generic "must declare `exits:`" wording is unsatisfiable: the message has
/// to point at `enums:`, the shape that can express it.
#[test]
fn kind_declared_slot_points_author_at_enums() {
    let msgs =
        missing_semantics_messages("---\nentities:\n  action: { members: [wave, bow] }\n---\n");
    assert_eq!(
        msgs.len(),
        1,
        "expected exactly one E-ENUM-MISSING-SEMANTICS, got {msgs:?}"
    );
    let msg = &msgs[0];
    assert!(
        msg.contains("`action`"),
        "message must name the domain: {msg}"
    );
    assert!(
        msg.contains("enums:"),
        "message must point at `enums:`, the declaration shape that can carry the \
         semantics: {msg}"
    );
    assert!(
        msg.contains("entities:"),
        "message must name the provenance that cannot express it: {msg}"
    );
    // The pre-fix wording — the shared validator's generic text — tells the
    // author to add a key `entities:` silently discards. Reject it explicitly.
    assert!(
        !msg.contains("the compiler reads it instead of inferring"),
        "generic validator wording is unsatisfiable for a kind-derived domain: {msg}"
    );
}

/// Guard against over-broad validation: an ordinary entity-kind name is not a
/// semantics-bearing slot, so it gets no member-semantics diagnostic at all.
#[test]
fn kind_declared_ordinary_name_has_no_semantics_diag() {
    let msgs = missing_semantics_messages("---\nentities:\n  npc: { members: [ana, bo] }\n---\n");
    assert!(
        msgs.is_empty(),
        "ordinary kind name must not be slot-validated: {msgs:?}"
    );
}

/// Path 1 is unchanged: an `enums:`-declared slot CAN carry `exits:`, so the
/// long form stays clean and the bare member list still errors.
#[test]
fn enum_declared_slot_validation_is_unchanged() {
    let ok = missing_semantics_messages(
        "---\nenums:\n  action:\n    members: [sway, hide]\n    exits: [hide]\n---\n",
    );
    assert!(
        ok.is_empty(),
        "declared `exits:` must satisfy the slot: {ok:?}"
    );

    let bad = missing_semantics_messages("---\nenums:\n  action: [sway, hide]\n---\n");
    assert_eq!(
        bad.len(),
        1,
        "expected exactly one E-ENUM-MISSING-SEMANTICS, got {bad:?}"
    );
    assert!(
        bad[0].contains("must declare `exits:`"),
        "enum path keeps the shared validator's wording: {}",
        bad[0]
    );
}

/// Provenance must come from the WINNING projection. When separate imported
/// files declare `action` under `entities:` AND under `enums:`,
/// `resolve_imports` retains it in BOTH `rel.kinds` and `rel.enums`, and
/// builds `domains` as enums-then-`.extend(kinds_to_domains(..))` — so the
/// KIND projection is the `Domain` in hand, and it cannot carry `exits:`
/// however the enum peer declared it. Reading provenance off the losing
/// (enum) source emits the shared validator's generic wording against a
/// value that can never satisfy it.
#[test]
fn mixed_provenance_uses_the_winning_kind_projection() {
    let msgs = missing_semantics_messages_files(
        &[
            (
                "k.lute",
                "---\nentities:\n  action: { members: [wave, bow] }\n---\n",
            ),
            (
                "e.lute",
                "---\nenums:\n  action:\n    members: [wave, bow]\n    exits: [bow]\n---\n",
            ),
            ("a.lute", "---\nuses: [k.lute, e.lute]\n---\n"),
        ],
        &["a.lute"],
    );
    assert_eq!(
        msgs.len(),
        1,
        "expected exactly one E-ENUM-MISSING-SEMANTICS, got {msgs:?}"
    );
    let msg = &msgs[0];
    assert!(
        msg.contains("enums:") && msg.contains("entities:"),
        "kind-derived wording must point at `enums:` and name the `entities:` provenance: {msg}"
    );
    assert!(
        !msg.contains("must declare `exits:`"),
        "generic validator wording is unsatisfiable for the kind-derived winner: {msg}"
    );
}

/// An OPEN kind-derived slot is not merely unvalidated, it is unworkable: a
/// registry-style domain cannot enumerate its exits, yet `kinds_to_domains`
/// hands the compiler an `action` with empty `exits` and no `default`. So the
/// openness escape `validate_domain` takes for MEMBERSHIP rules must not
/// apply here — the slot's semantics are a compiler input.
#[test]
fn open_kind_declared_slot_points_author_at_enums() {
    let msgs = missing_semantics_messages("---\nentities:\n  action: { open: engine }\n---\n");
    assert_eq!(
        msgs.len(),
        1,
        "expected exactly one E-ENUM-MISSING-SEMANTICS, got {msgs:?}"
    );
    assert!(
        msgs[0].contains("enums:"),
        "message must point at `enums:`: {}",
        msgs[0]
    );
}

/// The guard against over-broad validation: an open registry kind under an
/// ORDINARY name is a normal, supported declaration — no semantics diagnostic.
#[test]
fn open_kind_declared_ordinary_name_has_no_semantics_diag() {
    let msgs = missing_semantics_messages("---\nentities:\n  npc: { open: engine }\n---\n");
    assert!(
        msgs.is_empty(),
        "open registry kinds are a normal declaration: {msgs:?}"
    );
}

// --- dsl 0.9.0: a document's OWN domain projection reaches the merge. ---
//
// `enums:` is in `UNIVERSAL_KEYS`, so an inline block is deliberately legal
// syntax in ANY document, and `TypedMeta::domains` is the identical two-step
// projection (`parse_enums`, then `.extend(kinds_to_domains(..))`) that
// `resolve_imports` builds for an IMPORTED file. That value simply never
// reached `merge_domains`, so an author's own declaration parsed and was
// dropped — and `E-DOMAIN-UNKNOWN` then told them to declare what they had
// just declared, on the line above.
//
// Every test below pins the INLINE route against the IMPORTED one, because a
// second route that is not pinned to the first drifts.

/// Run the FULL check pipeline on `text` in `dir`, resolving the document's own
/// `uses:`/`extends:` exactly as the CLI's `build_input` does, and return the
/// diagnostic codes. The parity tests differ ONLY in where a declaration lives,
/// so both sides must go through one shared entrypoint.
fn scene_codes(dir: &Path, text: &str, snapshot: CapabilitySnapshot) -> Vec<String> {
    let (doc, _) = lute_syntax::parse(text);
    let (meta, _) = lute_check::parse_meta(&doc.meta, &snapshot);
    let imports = resolve_imports(dir, &meta.uses, &meta.extends, doc.meta.span);
    let input = CheckInput {
        text: text.into(),
        uri: "t".into(),
        snapshot,
        providers: ProviderSet::default(),
        mode: Mode::Author,
        imports,
        components: Default::default(),
        defaults: Default::default(),
    };
    let mut codes: Vec<String> =
        check(&input).diagnostics.iter().map(|d| d.code.clone()).collect();
    codes.sort();
    codes.dedup();
    codes
}

/// A scene with frontmatter `front` (already newline-terminated) and one
/// content line carrying `body`.
fn scene(front: &str, body: &str) -> String {
    format!(
        "---\nkind: scene\ncharacter: demo\nseason: 1\nepisode: 1\n{front}---\n\
         ## Shot 1.\n{body}\n"
    )
}

/// The reported fixture, verbatim: `emotion` declared INLINE on line 6 and
/// used on line 10 of the same document.
const INLINE_FIXTURE: &str = "---\nkind: scene\ncharacter: demo\nseason: 1\nepisode: 1\n\
     enums:\n  emotion: [neutral, gleeful]\n---\n## Shot 1.\n\
     @bianca{emotion=\"gleeful\"}: declared INLINE in this very document.\n";

/// 1. An inline `enums:` declaration satisfies a content line in the SAME
/// document. This is the whole defect: it parsed, it was dropped, and the
/// diagnostic pointed the author at the declaration they had already written.
#[test]
fn inline_enum_declaration_satisfies_a_content_line() {
    let dir = unique_dir();
    let codes = scene_codes(&dir, INLINE_FIXTURE, load_core_snapshot());
    assert!(
        codes.is_empty(),
        "an inline `enums:` block must declare the domain it names: {codes:?}"
    );
}

/// 2. The dsl 0.9.0 D-D long form works inline too: `action`'s `exits:` and
/// `anchor`'s `default:` carry the same member semantics an IMPORTED
/// declaration does. Reaching the D-D validator at all is what proves the
/// semantics survived the lift — a projection down to a bare member list
/// would silently discard them and the bare-list cases below would pass.
/// Mirrors `enum_declared_slot_validation_is_unchanged`, which pins exactly
/// this for the imported route.
#[test]
fn inline_long_form_enum_carries_the_slot_member_semantics() {
    let dir = unique_dir();
    let long_action = "enums:\n  action:\n    members: [wave, bow]\n    exits: [bow]\n";
    let ok = scene_codes(
        &dir,
        &scene(long_action, "@bianca{action=\"wave\"}: line."),
        load_core_snapshot(),
    );
    assert!(
        ok.is_empty(),
        "an inline long-form `action` declaring `exits:` must satisfy the slot: {ok:?}"
    );

    // The bare member list cannot carry `exits:`, so it is the same error the
    // imported route reports — never silence.
    let bare = scene_codes(
        &dir,
        &scene("enums:\n  action: [wave, bow]\n", "@bianca{action=\"wave\"}: line."),
        load_core_snapshot(),
    );
    assert!(
        bare.contains(&"E-ENUM-MISSING-SEMANTICS".to_string()),
        "a bare inline `action:` list must still be flagged: {bare:?}"
    );

    // `default:` is `anchor`'s slot semantics, not `action`'s (D-D splits
    // `SLOT_REQUIRES_EXITS` from `SLOT_REQUIRES_DEFAULT`), so pin it on the
    // slot that actually accepts it.
    let long_anchor = "enums:\n  anchor:\n    members: [left, center]\n    default: center\n";
    let anchor_ok = scene_codes(&dir, &scene(long_anchor, "@bianca: line."), load_core_snapshot());
    assert!(
        !anchor_ok.contains(&"E-ENUM-MISSING-SEMANTICS".to_string()),
        "an inline long-form `anchor` declaring `default:` must satisfy the slot: {anchor_ok:?}"
    );
    let anchor_bare =
        scene_codes(&dir, &scene("enums:\n  anchor: [left, center]\n", "@bianca: line."), load_core_snapshot());
    assert!(
        anchor_bare.contains(&"E-ENUM-MISSING-SEMANTICS".to_string()),
        "a bare inline `anchor:` list must still be flagged: {anchor_bare:?}"
    );
}

/// 3. Inline-vs-IMPORTED is a decision-D5 refinement, NOT `E-DOMAIN-DUP`.
/// Both sides are the PROJECT, and D2 reserves `E-DOMAIN-DUP` for a clash that
/// involves a plugin (a project-project collision is `E-USES-DUP-RELATION` /
/// `E-KIND-NAME-CLASH`, raised where the collision is seen — see this file's
/// header and `rel_compose.rs`). The document is depth 0 to any import's depth
/// >= 1, so `resolve_imports`'s own shallowest-wins rule makes the INLINE list
/// live — which is also what `build_rel_vocab` already does for the same names
/// in `RelVocab.enums`. The two maps must never disagree about which member
/// list is in force, so the non-superset diagnostic stays `build_rel_vocab`'s
/// alone: one owner, one diagnostic.
#[test]
fn inline_and_imported_enum_is_a_d5_refinement_never_domain_dup() {
    let dir = unique_dir();
    write_lute(&dir, "vocab.lute", "---\nenums:\n  emotion: [neutral, gleeful]\n---\n");

    // Superset re-declaration: legal, and the inline list is the live one — a
    // member only the inline decl adds must be ACCEPTED.
    let grow = scene_codes(
        &dir,
        &scene(
            "uses: vocab.lute\nenums:\n  emotion: [neutral, gleeful, feral]\n",
            "@bianca{emotion=\"feral\"}: inline grows the imported list.",
        ),
        load_core_snapshot(),
    );
    assert!(
        grow.is_empty(),
        "a superset inline re-declaration is a legal D5 refinement and wins: {grow:?}"
    );

    // Non-superset: the established D5 code, and still not `E-DOMAIN-DUP`.
    let shrink = scene_codes(
        &dir,
        &scene(
            "uses: vocab.lute\nenums:\n  emotion: [neutral]\n",
            "@bianca{emotion=\"neutral\"}: inline drops a base member.",
        ),
        load_core_snapshot(),
    );
    assert!(
        shrink.contains(&"E-EXTENDS-RELATION-SIG".to_string()),
        "a non-superset inline re-declaration is E-EXTENDS-RELATION-SIG: {shrink:?}"
    );
    assert!(
        !shrink.contains(&"E-DOMAIN-DUP".to_string()),
        "D2 reserves E-DOMAIN-DUP for plugin-involving clashes: {shrink:?}"
    );
    assert_eq!(
        shrink.iter().filter(|c| *c == "E-EXTENDS-RELATION-SIG").count(),
        1,
        "the D5 diagnostic has exactly one owner (`build_rel_vocab`): {shrink:?}"
    );
}

/// 4. Inline-vs-PLUGIN is `E-DOMAIN-DUP`, exactly as project-vs-plugin already
/// is (`merge_domains_flags_clash_with_snapshot_domain`, above): a domain name
/// must be declared by exactly one source, and the plugin/core entry wins by
/// the same drop-and-report semantics — never a silent shadow.
#[test]
fn inline_enum_clashing_with_a_plugin_domain_is_domain_dup() {
    let dir = unique_dir();
    let snapshot = lute_test_vocab::vocab_snapshot();
    assert!(
        !snapshot.domains["emotion"].members.contains(&"gleeful".to_string()),
        "the baseline must NOT already provide `gleeful`, or the clash is unobservable"
    );
    let codes = scene_codes(
        &dir,
        &scene(
            "enums:\n  emotion: [neutral, gleeful]\n",
            "@bianca{emotion=\"gleeful\"}: shadowing a plugin domain.",
        ),
        snapshot,
    );
    assert!(
        codes.contains(&"E-DOMAIN-DUP".to_string()),
        "an inline decl clashing with the baseline is E-DOMAIN-DUP: {codes:?}"
    );
    // The plugin wins, so the inline-only member is rejected — proving the
    // inline list was DROPPED rather than merged over the baseline.
    assert!(
        codes.contains(&"E-BAD-ENUM".to_string()),
        "the plugin/core entry wins the clash, so `gleeful` is not a member: {codes:?}"
    );
}

/// 5. PARITY — the assertion that keeps the two routes from drifting: the same
/// declaration expressed INLINE and in an imported schema yields the same
/// checking outcome for the same content. Runs both `enums:` and `entities:`,
/// against an accepted and a rejected value, so a divergence in either the
/// declaration shape or the membership decision fails here.
#[test]
fn inline_and_imported_declarations_check_identically() {
    // (label, inline frontmatter block, the schema body declaring the same thing)
    let routes: [(&str, &str, &str); 3] = [
        (
            "enums short form",
            "enums:\n  emotion: [neutral, gleeful]\n",
            "---\nenums:\n  emotion: [neutral, gleeful]\n---\n",
        ),
        (
            "entities closed members",
            "entities:\n  emotion: { members: [neutral, gleeful] }\n",
            "---\nentities:\n  emotion: { members: [neutral, gleeful] }\n---\n",
        ),
        (
            "enums long form on a semantics-bearing slot",
            "enums:\n  action:\n    members: [neutral, gleeful]\n    exits: [gleeful]\n",
            "---\nenums:\n  action:\n    members: [neutral, gleeful]\n    exits: [gleeful]\n---\n",
        ),
    ];
    for (label, inline_front, schema) in routes {
        // `action` is the slot the third route declares; the others declare
        // `emotion`. Use whichever the block names so the content line is
        // actually resolved against it.
        let slot = if inline_front.contains("action") { "action" } else { "emotion" };
        for value in ["gleeful", "zzz"] {
            let body = format!("@bianca{{{slot}=\"{value}\"}}: line.");
            let dir = unique_dir();
            let inline = scene_codes(&dir, &scene(inline_front, &body), load_core_snapshot());
            let dir2 = unique_dir();
            write_lute(&dir2, "vocab.lute", schema);
            let imported =
                scene_codes(&dir2, &scene("uses: vocab.lute\n", &body), load_core_snapshot());
            assert_eq!(
                inline, imported,
                "{label} with {slot}={value:?}: inline and imported must agree"
            );
        }
    }
}
