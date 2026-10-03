//! The `luteVersion` freshness signal (`W-LUTE-VERSION-STALE`, dsl 0.6.1 §3).

use super::*;

/// dsl 0.6.1 §3: a document's frontmatter `luteVersion` stamp is present but
/// differs from the toolchain's [`crate::LUTE_LANG_VERSION`].
pub const W_LUTE_VERSION_STALE: &str = "W-LUTE-VERSION-STALE";

/// LH N18: how a [`W_LUTE_VERSION_STALE`] message opens when the stamp is
/// inherited from the manifest's `defaults:` rather than written by the
/// document — `check-project` folds those into one report at the manifest.
pub const INHERITED_LUTE_VERSION: &str = "the project manifest's `defaults: luteVersion";

/// dsl 0.6.1 §3: the freshness signal. Fires `W-LUTE-VERSION-STALE` when the
/// frontmatter `luteVersion` stamp is PRESENT and differs from the toolchain's
/// [`crate::LUTE_LANG_VERSION`] — a warning-grade catch for a model
/// reproducing a stale stamp copied from an older example. D13 stands:
/// `luteVersion` is never validated against capabilities, so this is the ONLY
/// treatment it gets. An ABSENT key is clean (no stamp to be stale); a
/// CURRENT stamp is clean. `Layer::Content` (a frontmatter-key concern, same
/// layer `parse_meta_kind`'s own meta diagnostics use). The span points at the
/// `luteVersion:` key ([`crate::meta::meta_key_span`]).
///
/// The two versions are compared as `MAJOR.MINOR.PATCH` numbers, never as
/// strings: a stamp NEWER than the toolchain is not stale — the toolchain is
/// (0.21.1 T3-6, ashen F36: an older language server told every current
/// document to downgrade its stamp). That case names the toolchain as the
/// thing to update. A stamp that is not a numeric triple keeps the restamp
/// advice — there is no order to consult.
pub(super) fn check_lute_version_stale(
    typed: &crate::meta::TypedMeta,
    meta: &lute_syntax::ast::Meta,
) -> Option<Diagnostic> {
    let stamped = typed.lute_version.as_deref()?;
    if stamped == crate::LUTE_LANG_VERSION {
        return None;
    }
    let current = crate::LUTE_LANG_VERSION;
    let newer = matches!(
        (version_triple(stamped), version_triple(current)),
        (Some(s), Some(c)) if s > c
    );
    // LH N18: a stamp the document does not write itself comes from the
    // manifest's `defaults: luteVersion` — say so, so `check-project` can fold
    // every document's copy into one report at the manifest line.
    let own = meta.raw_yaml.lines().any(|l| {
        l.strip_prefix("luteVersion")
            .is_some_and(|rest| rest.trim_start().starts_with(':'))
    });
    let what = if own {
        "frontmatter `luteVersion"
    } else {
        INHERITED_LUTE_VERSION
    };
    let message = if newer {
        format!(
            "{what}: \"{stamped}\"` is newer than this toolchain (Lute \
             {current}) — upgrade the toolchain; do not downgrade the stamp. In an editor this \
             is its lute-lsp, and its other diagnostics may be wrong: run `lute doctor` to see \
             which install is behind (dsl 0.6.1 §3)"
        )
    } else {
        format!(
            "{what}: \"{stamped}\"` is stale — this toolchain is Lute \
             {current}; update the stamp to `luteVersion: \"{current}\"` (dsl 0.6.1 §3)"
        )
    };
    Some(Diagnostic {
        code: W_LUTE_VERSION_STALE.to_string(),
        severity: Severity::Warning,
        message,
        evidence: None,
        span: crate::meta::meta_key_span(meta, "luteVersion"),
        layer: Layer::Content,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    })
}

/// `MAJOR.MINOR.PATCH` as a comparable triple; `None` for anything else.
fn version_triple(v: &str) -> Option<(u64, u64, u64)> {
    let mut parts = v.trim().split('.');
    let triple = (
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
    );
    parts.next().is_none().then_some(triple)
}

#[cfg(test)]
mod lute_version_tests {
    use super::*;

    fn meta(raw: &str) -> lute_syntax::ast::Meta {
        lute_syntax::ast::Meta {
            raw_yaml: raw.to_string(),
            span: Span {
                byte_start: 0,
                byte_end: raw.len(),
                line: 1,
                column: 1,
                utf16_range: (0, 0),
            },
        }
    }

    /// dsl 0.6.1 §3: a PRESENT `luteVersion` stamp differing from the
    /// toolchain version warns, naming both the stale stamp and the current
    /// version (the fix).
    #[test]
    fn stale_stamp_warns() {
        let typed = crate::meta::TypedMeta {
            lute_version: Some("0.5.0".to_string()),
            ..Default::default()
        };
        let d = check_lute_version_stale(&typed, &meta("luteVersion: \"0.5.0\"\n"))
            .expect("a stale stamp must warn");
        assert_eq!(d.code, W_LUTE_VERSION_STALE);
        assert_eq!(d.severity, Severity::Warning);
        assert!(
            d.message.contains("0.5.0"),
            "names the stale stamp: {}",
            d.message
        );
        assert!(
            d.message.contains(crate::LUTE_LANG_VERSION),
            "names the current version: {}",
            d.message
        );
    }

    /// 0.21.1 T3-6 (ashen F36): a stamp NEWER than the toolchain is not stale —
    /// the toolchain is. The message names the toolchain as the thing to
    /// upgrade and never advises rewriting the stamp downwards. `0.9.0` is the
    /// control: string-greater than any `0.1x`/`0.2x` toolchain yet numerically
    /// older, so it must keep the restamp advice.
    #[test]
    fn newer_stamp_says_upgrade_the_toolchain() {
        let stamp = |v: &str| {
            let typed = crate::meta::TypedMeta {
                lute_version: Some(v.to_string()),
                ..Default::default()
            };
            check_lute_version_stale(&typed, &meta(&format!("luteVersion: \"{v}\"\n")))
                .expect("a differing stamp warns")
                .message
        };
        let newer = stamp("999.0.0");
        assert!(
            newer.contains("upgrade the toolchain") && newer.contains("run `lute doctor`"),
            "a newer stamp names the toolchain: {newer}"
        );
        assert!(
            !newer.contains("update the stamp"),
            "a newer stamp is never told to downgrade: {newer}"
        );
        let older = stamp("0.9.0");
        assert!(
            older.contains("is stale") && older.contains("update the stamp"),
            "numerically older (though string-greater) keeps the restamp advice: {older}"
        );
    }

    /// A stamp matching the toolchain version is clean (nothing stale).
    #[test]
    fn current_stamp_clean() {
        let typed = crate::meta::TypedMeta {
            lute_version: Some(crate::LUTE_LANG_VERSION.to_string()),
            ..Default::default()
        };
        assert!(check_lute_version_stale(&typed, &meta("")).is_none());
    }

    /// An ABSENT `luteVersion` key is clean — no stamp to be stale (D13).
    #[test]
    fn absent_stamp_clean() {
        let typed = crate::meta::TypedMeta::default();
        assert!(check_lute_version_stale(&typed, &meta("")).is_none());
    }

    /// `docs/versioning.md`'s alignment rule, pinned so the release cannot
    /// half-land: the language constant this check compares against and the
    /// workspace (toolchain) version must both read the release number.
    /// `0.31.0` is a language AND IR release: declared beat/entry advancement
    /// and the two clock-window diagnostics move all axes to the same number.
    #[test]
    fn language_ir_and_toolchain_are_aligned_at_0_34_0() {
        assert_eq!(crate::LUTE_LANG_VERSION, "0.34.0");
        assert_eq!(env!("CARGO_PKG_VERSION"), "0.34.0");
    }
}
