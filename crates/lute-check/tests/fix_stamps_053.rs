//! spec 0.53.0 §4 (#224, #235): one predicate for an old attribute
//! spelling. A key the document's snapshot admits on a directive (a plugin
//! stamp) is not the 0.37 spelling: the checker judges the renamed
//! attribute as absent, and `lute fix` keeps the site instead of renaming
//! it. Without a resolved snapshot `lute fix` renames nothing and reports
//! every such site as kept.

use lute_check::{check, fix_document, CheckInput, FixRule, KeptReason, Mode, SchemaImports};
use lute_manifest::provider::ProviderSet;
use lute_manifest::schema::AttrDecl;
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_manifest::stamps::{StampAttrDecl, StampSurface};
use lute_manifest::types::{Literal, Type};

const HDR: &str = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n";

fn action() -> AttrDecl {
    AttrDecl {
        name: "action".to_string(),
        required: false,
        ty: Type::Str,
        default: None,
    }
}

/// The core snapshot with `action: string` declared as a stamp: on every
/// directive (`on: None`) or on the named directives only.
fn with_action_stamp(on: Option<&[&str]>) -> CapabilitySnapshot {
    let mut snap = lute_manifest::core::load_core_snapshot();
    let decl = match on {
        None => StampAttrDecl::cross_cutting("acme.stage", action()),
        Some(tags) => StampAttrDecl::scoped(
            "acme.stage",
            action(),
            tags.iter()
                .map(|t| StampSurface::Directive((*t).to_string())),
        ),
    };
    snap.stamp_attrs.insert(decl).unwrap();
    snap
}

fn diagnostics(text: &str, snapshot: CapabilitySnapshot) -> Vec<lute_core_span::Diagnostic> {
    let input = CheckInput::new(
        text.to_string(),
        "fix_stamps_053".into(),
        Mode::Author,
        lute_check::InputSources::from_parts(
            snapshot,
            ProviderSet::default(),
            SchemaImports::default(),
            Default::default(),
            Default::default(),
        ),
    );
    check(&input).diagnostics
}

const ISSUE_BODY: &str = "::cg{assetId=\"CUT.a\" action=\"fade-in\"}\n\
                          ::music{assetId=\"BGM.a\" action=\"duck\"}\n\
                          ::video{assetId=\"MOV.a\" action=\"slow\"}\n\
                          @mira: Hello.\n";

/// #224: a stamp `action` on every directive keeps all three sites, and the
/// document is unchanged.
#[test]
fn a_stamp_is_kept_not_renamed() {
    let src = format!("{HDR}{ISSUE_BODY}");
    let snap = with_action_stamp(None);
    let out = fix_document(&src, Some(&snap));
    assert_eq!(out.changed, 0, "got:\n{}", out.text);
    assert_eq!(out.text, src);
    assert_eq!(out.rules.total(), 0);
    let kept: Vec<(u32, u32, &str, KeptReason)> = out
        .kept
        .iter()
        .map(|k| (k.line, k.column, k.tag.as_str(), k.reason))
        .collect();
    assert_eq!(
        kept,
        [
            (8, 22, "cg", KeptReason::Stamp),
            (9, 25, "music", KeptReason::Stamp),
            (10, 25, "video", KeptReason::Stamp),
        ]
    );
    assert!(out.kept.iter().all(|k| k.rule == FixRule::RenamedAttr));
    assert_eq!(
        out.kept[0].message(),
        "8:22: kept `action` on `::cg` — the active profile declares it as a stamp attribute; \
         the 0.37 rename to `display` does not apply"
    );
}

/// A real 0.37 spelling (no stamp) is still renamed, and counted under
/// `renamed-attr`.
#[test]
fn a_real_037_spelling_is_still_renamed() {
    let src = format!("{HDR}::cg{{assetId=\"C\" action=\"show\"}}\n");
    let core = lute_manifest::core::load_core_snapshot();
    let out = fix_document(&src, Some(&core));
    assert_eq!(
        out.text,
        format!("{HDR}::cg{{assetId=\"C\" display=\"show\"}}\n")
    );
    assert_eq!(out.changed, 1);
    assert_eq!(out.rules.get(FixRule::RenamedAttr), 1);
    assert_eq!(
        out.rules.applied().collect::<Vec<_>>(),
        [(FixRule::RenamedAttr, 1)]
    );
    assert!(out.kept.is_empty());
    let again = fix_document(&out.text, Some(&core));
    assert_eq!(again.changed, 0, "idempotent");
    assert_eq!(again.text, out.text);
}

/// A stamp scoped `on: [music]` keeps `::music{action}` and still renames
/// `::cg{action}`.
#[test]
fn a_scoped_stamp_keeps_only_its_surface() {
    let src = format!(
        "{HDR}::music{{assetId=\"BGM.a\" action=\"duck\"}}\n::cg{{assetId=\"C\" action=\"show\"}}\n"
    );
    let snap = with_action_stamp(Some(&["music"]));
    let out = fix_document(&src, Some(&snap));
    assert_eq!(
        out.text,
        format!(
            "{HDR}::music{{assetId=\"BGM.a\" action=\"duck\"}}\n::cg{{assetId=\"C\" display=\"show\"}}\n"
        )
    );
    assert_eq!(out.rules.get(FixRule::RenamedAttr), 1);
    assert_eq!(out.kept.len(), 1);
    assert_eq!(out.kept[0].tag, "music");
    assert_eq!(out.kept[0].reason, KeptReason::Stamp);
}

/// Under an old tag the attribute is judged on the new tag's surface:
/// `::cut{action}` with a stamp `action` on `::cg` renames the tag only.
#[test]
fn an_old_tag_is_judged_on_the_new_surface() {
    let src = format!("{HDR}::cut{{assetId=\"C\" action=\"x\"}}\n");
    let snap = with_action_stamp(Some(&["cg"]));
    let out = fix_document(&src, Some(&snap));
    assert_eq!(
        out.text,
        format!("{HDR}::cg{{assetId=\"C\" action=\"x\"}}\n")
    );
    assert_eq!(
        out.rules.applied().collect::<Vec<_>>(),
        [(FixRule::RenamedTag, 1)]
    );
    assert_eq!(out.kept.len(), 1);
    assert_eq!(out.kept[0].tag, "cg");
    // The checker's hint on the old tag agrees: no "its `action=` is now".
    let diags = diagnostics(&src, snap);
    let renamed = diags
        .iter()
        .find(|d| d.code == "E-RENAMED-DIRECTIVE")
        .expect("the old tag is reported");
    assert!(
        !renamed.message.contains("is now `display=`"),
        "{}",
        renamed.message
    );
}

/// A document whose snapshot did not resolve (a missing plugin): every
/// rule-8 site is kept with `unresolved`; every other rule still runs.
#[test]
fn an_unresolved_snapshot_keeps_every_site() {
    let src = format!("{HDR}::cg{{assetId=\"C\" action=\"show\"}}\n::auto{{character=\"m\"}}\n");
    let out = fix_document(&src, None);
    assert_eq!(
        out.text,
        format!("{HDR}::cg{{assetId=\"C\" action=\"show\"}}\n::actor{{character=\"m\"}}\n")
    );
    assert_eq!(
        out.rules.applied().collect::<Vec<_>>(),
        [(FixRule::RenamedTag, 1)]
    );
    assert_eq!(out.kept.len(), 1);
    assert_eq!(out.kept[0].reason, KeptReason::Unresolved);
    assert_eq!(
        out.kept[0].message(),
        "8:18: kept `action` on `::cg` — the document's capability snapshot did not resolve, so \
         `lute fix` cannot tell a stamp from the 0.37 spelling; run `lute check` and fix the \
         project first"
    );
}

/// #235: with a stamp `action` on `::cg`, `action` is not `display`, so
/// `display`'s declared default is judged (here an invalid one). Before,
/// `display` counted as present and the default was skipped.
#[test]
fn a_stamp_does_not_hide_the_renamed_attributes_default() {
    let mut snap = with_action_stamp(None);
    let cg = snap.directives.get_mut("cg").expect("core `::cg`");
    let display = cg
        .attrs
        .iter_mut()
        .find(|a| a.name == "display")
        .expect("`::cg` declares `display`");
    display.default = Some(Literal::Str("bogus".to_string()));
    let src = format!("{HDR}::cg{{assetId=\"C\" action=\"x\"}}\n");
    let judged: Vec<_> = diagnostics(&src, snap)
        .into_iter()
        .filter(|d| d.message.contains("declared `default:` of `display`"))
        .collect();
    assert_eq!(judged.len(), 1, "{judged:?}");
    assert_eq!(judged[0].code, "E-BAD-ENUM");
}

/// Without a stamp the old spelling is still `E-RENAMED-ATTR`, and counts as
/// the renamed attribute for its default (no second report).
#[test]
fn without_a_stamp_the_old_spelling_is_reported() {
    let mut snap = lute_manifest::core::load_core_snapshot();
    let cg = snap.directives.get_mut("cg").expect("core `::cg`");
    let display = cg
        .attrs
        .iter_mut()
        .find(|a| a.name == "display")
        .expect("`::cg` declares `display`");
    display.default = Some(Literal::Str("bogus".to_string()));
    let src = format!("{HDR}::cg{{assetId=\"C\" action=\"x\"}}\n");
    let diags = diagnostics(&src, snap);
    assert!(
        diags.iter().any(|d| d.code == "E-RENAMED-ATTR"),
        "{diags:?}"
    );
    assert!(
        !diags
            .iter()
            .any(|d| d.message.contains("declared `default:` of `display`")),
        "{diags:?}"
    );
}
