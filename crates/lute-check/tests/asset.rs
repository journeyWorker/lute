//! Authored `assetId` validation against its declared `assetKind`
//! (plugin §6.9, checker half of precedence step-1). A segment-bearing `CH`
//! compose kind decomposes + per-segment validates; a segment-less `BG` query
//! kind checks provider-existence only; a `PLACEHOLDER_*` id warns.
use lute_check::{check, CheckInput, Mode};
use lute_manifest::provider::{ProviderSet, ProviderSnapshot};
use lute_manifest::schema::{
    AssetKindDecl, AssetResolve, AssetSegment, AttrDecl, DirectiveDecl, Lowering,
};
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_manifest::types::Type;

fn seg(name: &str, r#const: Option<&str>, ty: Option<Type>) -> AssetSegment {
    AssetSegment {
        name: name.into(),
        r#const: r#const.map(Into::into),
        ty,
    }
}

/// A synthetic directive whose only attr is `assetId: Type::AssetKind(kind)`.
fn asset_directive(name: &str, kind: &str) -> DirectiveDecl {
    DirectiveDecl {
        name: name.into(),
        layer: None,
        attrs: vec![AttrDecl {
            name: "assetId".into(),
            required: false,
            ty: Type::AssetKind(kind.into()),
            default: None,
        }],
        semantics: vec![],
        state: None,
        effects: None,
        bridge: None,
        lower: Lowering::Builtin {
            kind: "builtin".into(),
            name: "n".into(),
        },
    }
}

/// Core snapshot + a segment-bearing `CH` compose kind (`::portrait`) and a
/// segment-less `BG` query kind (`::backdrop`).
fn snapshot_with_assets() -> CapabilitySnapshot {
    let mut snap = lute_manifest::core::load_core_snapshot();
    // CH: `<CH>.<character>.<outfit>.<expression>.<variant>` (5 segments).
    snap.asset_kinds.insert(
        "CH".into(),
        AssetKindDecl {
            kind: "CH".into(),
            sep: ".".into(),
            resolve: AssetResolve::Compose,
            segments: vec![
                seg("kind", Some("CH"), None),
                seg(
                    "character",
                    None,
                    Some(Type::ProviderRef("character".into())),
                ),
                seg("outfit", None, Some(Type::Str)),
                seg("expression", None, Some(Type::Str)),
                seg("variant", None, Some(Type::Number)),
            ],
            provider: None,
            match_: vec![],
            aliases: Default::default(),
            fallback: vec![],
            persistence: None,
        },
    );
    // BG: segment-less query kind — provider-existence only.
    snap.asset_kinds.insert(
        "BG".into(),
        AssetKindDecl {
            kind: "BG".into(),
            sep: ".".into(),
            resolve: AssetResolve::Query,
            segments: vec![],
            provider: Some("backgrounds".into()),
            match_: vec![],
            aliases: Default::default(),
            fallback: vec![],
            persistence: None,
        },
    );
    snap.directives
        .insert("portrait".into(), asset_directive("portrait", "CH"));
    snap.directives
        .insert("backdrop".into(), asset_directive("backdrop", "BG"));
    snap
}

fn providers() -> ProviderSet {
    ProviderSet::from_one(ProviderSnapshot {
        manifest_version: "cap".into(),
        provider_version: "1".into(),
        entries: [
            ("character".to_string(), vec!["bianca".to_string()]),
            ("backgrounds".to_string(), vec!["bg_home".to_string()]),
        ]
        .into_iter()
        .collect(),
        stale: false,
    })
}

fn scene(directive: &str) -> String {
    format!("---\ncharacter: bianca\nseason: 1\nepisode: 5\n---\n## Shot 1.\n{directive}\n")
}

fn check_codes(text: &str, snap: CapabilitySnapshot, providers: ProviderSet) -> Vec<String> {
    let input = CheckInput {
        text: text.into(),
        uri: "t".into(),
        snapshot: snap,
        providers,
        mode: Mode::Author,
    };
    check(&input)
        .diagnostics
        .into_iter()
        .map(|d| d.code)
        .collect()
}

fn has_asset_error(codes: &[String]) -> bool {
    codes.iter().any(|c| c.starts_with("E-ASSET-"))
}

#[test]
fn asset_valid_id_clean() {
    let text = scene("::portrait{assetId=\"CH.bianca.waitress.delighted.1\"}");
    let codes = check_codes(&text, snapshot_with_assets(), providers());
    assert!(
        !has_asset_error(&codes),
        "a well-formed asset id must not raise any E-ASSET-*; got {codes:?}"
    );
}

#[test]
fn asset_bad_provider_segment() {
    let text = scene("::portrait{assetId=\"CH.zzz.waitress.delighted.1\"}");
    let codes = check_codes(&text, snapshot_with_assets(), providers());
    assert!(
        codes.contains(&"E-ASSET-SEGMENT".to_string()),
        "an unknown provider segment must raise E-ASSET-SEGMENT; got {codes:?}"
    );
}

#[test]
fn asset_arity_decompose() {
    let text = scene("::portrait{assetId=\"CH.bianca\"}");
    let codes = check_codes(&text, snapshot_with_assets(), providers());
    assert!(
        codes.contains(&"E-ASSET-DECOMPOSE".to_string()),
        "a wrong segment count must raise E-ASSET-DECOMPOSE; got {codes:?}"
    );
}

#[test]
fn asset_placeholder_warns() {
    let text = scene("::portrait{assetId=\"PLACEHOLDER_face\"}");
    let codes = check_codes(&text, snapshot_with_assets(), providers());
    assert!(
        codes.contains(&"W-ASSET-PLACEHOLDER".to_string()),
        "a PLACEHOLDER_* id must warn W-ASSET-PLACEHOLDER; got {codes:?}"
    );
    assert!(
        !has_asset_error(&codes),
        "a placeholder id must not raise any E-ASSET-*; got {codes:?}"
    );
}

#[test]
fn asset_query_unknown_id() {
    let missing = scene("::backdrop{assetId=\"bg_missing\"}");
    let codes = check_codes(&missing, snapshot_with_assets(), providers());
    assert!(
        codes.contains(&"E-ASSET-UNKNOWN-ID".to_string()),
        "an absent query id must raise E-ASSET-UNKNOWN-ID; got {codes:?}"
    );

    let known = scene("::backdrop{assetId=\"bg_home\"}");
    let codes = check_codes(&known, snapshot_with_assets(), providers());
    assert!(
        !codes.contains(&"E-ASSET-UNKNOWN-ID".to_string()),
        "a known query id must not raise E-ASSET-UNKNOWN-ID; got {codes:?}"
    );
}
