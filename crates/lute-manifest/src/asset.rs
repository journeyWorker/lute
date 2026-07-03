use crate::provider::{IdStatus, ProviderSet};
use crate::schema::AssetKindDecl;
use crate::types::Type;

/// One decomposed asset-id segment: a `name` (from the kind's segment decl, or
/// the kind itself for a pure-query kind) bound to its `value` (the matching
/// slice of the id).
#[derive(Clone, Debug, PartialEq)]
pub struct Segment {
    pub name: String,
    pub value: String,
}

/// Failure decomposing an asset id against a kind's segment grammar.
#[derive(Clone, Debug, PartialEq)]
pub enum DecomposeError {
    /// The id split into a different number of parts than the kind declares.
    Arity { expected: usize, found: usize },
    /// A `const` segment did not match its declared literal.
    ConstMismatch {
        name: String,
        expected: String,
        found: String,
    },
}

/// Split `id` by the kind's `sep` and zip with the kind's `segments`; a `const`
/// segment must match verbatim. Returns typed [`Segment`]s (name↔value) or an
/// arity/const error. A segment-less kind (pure query) decomposes to a SINGLE
/// opaque segment carrying the whole id. Pure, deterministic, total (never
/// panics): it does not consult a provider set.
pub fn decompose(kind: &AssetKindDecl, id: &str) -> Result<Vec<Segment>, DecomposeError> {
    if kind.segments.is_empty() {
        return Ok(vec![Segment {
            name: kind.kind.clone(),
            value: id.to_string(),
        }]);
    }

    let parts: Vec<&str> = id.split(kind.sep.as_str()).collect();
    if parts.len() != kind.segments.len() {
        return Err(DecomposeError::Arity {
            expected: kind.segments.len(),
            found: parts.len(),
        });
    }

    let mut segments = Vec::with_capacity(parts.len());
    for (seg, part) in kind.segments.iter().zip(parts.iter()) {
        if let Some(c) = seg.r#const.as_deref() {
            if *part != c {
                return Err(DecomposeError::ConstMismatch {
                    name: seg.name.clone(),
                    expected: c.to_string(),
                    found: part.to_string(),
                });
            }
        }
        segments.push(Segment {
            name: seg.name.clone(),
            value: part.to_string(),
        });
    }
    Ok(segments)
}

/// True for a `PLACEHOLDER_*` sentinel id (§6.9 exemption from resolution).
pub fn is_placeholder(id: &str) -> bool {
    id.starts_with("PLACEHOLDER_")
}

/// A per-segment validation finding produced by [`validate_segments`]. These are
/// pure findings, not diagnostics: the checker (A4/A5) maps them to codes/spans.
#[derive(Clone, Debug, PartialEq)]
pub enum AssetIssue {
    /// A `const` segment's value did not match its declared literal.
    BadConst {
        segment: String,
        expected: String,
        found: String,
    },
    /// An `enum`-typed segment's value is not one of the declared members.
    NotEnumMember {
        segment: String,
        value: String,
        members: Vec<String>,
    },
    /// A `number`-typed segment's value does not parse as a number.
    NotNumber { segment: String, value: String },
    /// A `providerRef`-typed segment's value is absent from a fresh snapshot.
    UnknownProviderId {
        segment: String,
        provider: String,
        value: String,
    },
    /// A `providerRef`-typed segment's value is absent, but from a stale
    /// (offline) snapshot — a warning, not a hard unknown-id error (§10).
    StaleProviderId {
        segment: String,
        provider: String,
        value: String,
    },
}

/// Validate each decomposed [`Segment`] against its declared segment type,
/// pairing by POSITION with `kind.segments` (decompose emits segments in
/// declared order, so index-pairing is the contract; a segment-less kind →
/// empty zip → `[]`). Per segment: a `const` decl must match verbatim; otherwise
/// the declared type decides — `enum` membership, `number` parses as `f64`,
/// `str` accepts anything, `providerRef` resolves against `providers`
/// (`Fresh` ok / `Stale` → [`AssetIssue::StaleProviderId`] / `Absent` →
/// [`AssetIssue::UnknownProviderId`]). Any other type (or an untyped segment) is
/// accepted. Returns findings in segment order — the checker maps them to
/// diagnostics later. Pure, deterministic, total (never panics: no unwrap, no
/// indexing, `parse` is total).
pub fn validate_segments(
    kind: &AssetKindDecl,
    segs: &[Segment],
    providers: &ProviderSet,
) -> Vec<AssetIssue> {
    let mut issues = Vec::new();
    for (decl, seg) in kind.segments.iter().zip(segs) {
        if let Some(c) = &decl.r#const {
            if seg.value != *c {
                issues.push(AssetIssue::BadConst {
                    segment: decl.name.clone(),
                    expected: c.clone(),
                    found: seg.value.clone(),
                });
            }
            continue;
        }
        match &decl.ty {
            Some(Type::Enum(members)) => {
                if !members.iter().any(|m| m == &seg.value) {
                    issues.push(AssetIssue::NotEnumMember {
                        segment: decl.name.clone(),
                        value: seg.value.clone(),
                        members: members.clone(),
                    });
                }
            }
            Some(Type::Number) => {
                if seg.value.parse::<f64>().is_err() {
                    issues.push(AssetIssue::NotNumber {
                        segment: decl.name.clone(),
                        value: seg.value.clone(),
                    });
                }
            }
            Some(Type::Str) => {}
            Some(Type::ProviderRef(provider)) => match providers.contains(provider, &seg.value) {
                IdStatus::Fresh => {}
                IdStatus::Stale => issues.push(AssetIssue::StaleProviderId {
                    segment: decl.name.clone(),
                    provider: provider.clone(),
                    value: seg.value.clone(),
                }),
                IdStatus::Absent => issues.push(AssetIssue::UnknownProviderId {
                    segment: decl.name.clone(),
                    provider: provider.clone(),
                    value: seg.value.clone(),
                }),
            },
            _ => {}
        }
    }
    issues
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{ProviderSet, ProviderSnapshot};
    use crate::schema::{AssetKindDecl, AssetResolve, AssetSegment};
    use crate::types::Type;

    fn emotions() -> Vec<String> {
        vec![
            "neutral".to_string(),
            "delighted".to_string(),
            "sad".to_string(),
            "angry".to_string(),
        ]
    }

    fn fresh_providers() -> ProviderSet {
        ProviderSet::from_one(ProviderSnapshot {
            manifest_version: "cap".into(),
            provider_version: "1".into(),
            entries: [("character".to_string(), vec!["bianca".to_string()])]
                .into_iter()
                .collect(),
            stale: false,
        })
    }

    fn stale_providers() -> ProviderSet {
        ProviderSet::from_one(ProviderSnapshot {
            manifest_version: "cap".into(),
            provider_version: "1".into(),
            entries: [("character".to_string(), vec!["ren".to_string()])]
                .into_iter()
                .collect(),
            stale: true,
        })
    }

    fn ch_kind() -> AssetKindDecl {
        AssetKindDecl {
            kind: "CH".to_string(),
            sep: ".".to_string(),
            resolve: AssetResolve::Compose,
            segments: vec![
                AssetSegment {
                    name: "prefix".to_string(),
                    r#const: Some("CH".to_string()),
                    ty: None,
                },
                AssetSegment {
                    name: "characterId".to_string(),
                    r#const: None,
                    ty: Some(Type::ProviderRef("character".to_string())),
                },
                AssetSegment {
                    name: "costume".to_string(),
                    r#const: None,
                    ty: Some(Type::Str),
                },
                AssetSegment {
                    name: "emotion".to_string(),
                    r#const: None,
                    ty: Some(Type::Enum(emotions())),
                },
                AssetSegment {
                    name: "variant".to_string(),
                    r#const: None,
                    ty: Some(Type::Number),
                },
            ],
            provider: None,
            match_: Vec::new(),
            aliases: std::collections::BTreeMap::new(),
            fallback: Vec::new(),
            persistence: None,
        }
    }

    fn bg_query_kind() -> AssetKindDecl {
        AssetKindDecl {
            kind: "BG".to_string(),
            sep: ".".to_string(),
            resolve: AssetResolve::Query,
            segments: Vec::new(),
            provider: Some("backgrounds".to_string()),
            match_: Vec::new(),
            aliases: std::collections::BTreeMap::new(),
            fallback: Vec::new(),
            persistence: None,
        }
    }

    #[test]
    fn decompose_ch_ok() {
        let got = decompose(&ch_kind(), "CH.bianca.waitress.delighted.1").unwrap();
        let pairs: Vec<(&str, &str)> = got
            .iter()
            .map(|s| (s.name.as_str(), s.value.as_str()))
            .collect();
        assert_eq!(
            pairs,
            [
                ("prefix", "CH"),
                ("characterId", "bianca"),
                ("costume", "waitress"),
                ("emotion", "delighted"),
                ("variant", "1"),
            ]
        );
    }

    #[test]
    fn decompose_arity_too_few() {
        assert_eq!(
            decompose(&ch_kind(), "CH.bianca"),
            Err(DecomposeError::Arity {
                expected: 5,
                found: 2,
            })
        );
    }

    #[test]
    fn decompose_arity_too_many() {
        assert_eq!(
            decompose(&ch_kind(), "CH.a.b.c.d.e.f"),
            Err(DecomposeError::Arity {
                expected: 5,
                found: 7,
            })
        );
    }

    #[test]
    fn decompose_const_mismatch() {
        assert_eq!(
            decompose(&ch_kind(), "XX.bianca.waitress.delighted.1"),
            Err(DecomposeError::ConstMismatch {
                name: "prefix".to_string(),
                expected: "CH".to_string(),
                found: "XX".to_string(),
            })
        );
    }

    #[test]
    fn decompose_query_kind_opaque() {
        assert_eq!(
            decompose(&bg_query_kind(), "backgrounds/foo"),
            Ok(vec![Segment {
                name: "BG".to_string(),
                value: "backgrounds/foo".to_string(),
            }])
        );
    }

    #[test]
    fn is_placeholder_true_false() {
        assert!(is_placeholder("PLACEHOLDER_x"));
        assert!(!is_placeholder("BG.a.b"));
    }

    #[test]
    fn validate_ok() {
        let ch = ch_kind();
        let segs = decompose(&ch, "CH.bianca.waitress.delighted.1").unwrap();
        assert_eq!(
            validate_segments(&ch, &segs, &fresh_providers()),
            Vec::<AssetIssue>::new()
        );
    }

    #[test]
    fn validate_unknown_provider() {
        let ch = ch_kind();
        let segs = decompose(&ch, "CH.zzz.waitress.delighted.1").unwrap();
        assert_eq!(
            validate_segments(&ch, &segs, &fresh_providers()),
            vec![AssetIssue::UnknownProviderId {
                segment: "characterId".to_string(),
                provider: "character".to_string(),
                value: "zzz".to_string(),
            }]
        );
    }

    #[test]
    fn validate_bad_enum() {
        let ch = ch_kind();
        let segs = decompose(&ch, "CH.bianca.waitress.badmood.1").unwrap();
        assert_eq!(
            validate_segments(&ch, &segs, &fresh_providers()),
            vec![AssetIssue::NotEnumMember {
                segment: "emotion".to_string(),
                value: "badmood".to_string(),
                members: emotions(),
            }]
        );
    }

    #[test]
    fn validate_not_number() {
        let ch = ch_kind();
        let segs = decompose(&ch, "CH.bianca.waitress.delighted.x").unwrap();
        assert_eq!(
            validate_segments(&ch, &segs, &fresh_providers()),
            vec![AssetIssue::NotNumber {
                segment: "variant".to_string(),
                value: "x".to_string(),
            }]
        );
    }

    #[test]
    fn validate_stale_provider() {
        let ch = ch_kind();
        let segs = decompose(&ch, "CH.bianca.waitress.delighted.1").unwrap();
        assert_eq!(
            validate_segments(&ch, &segs, &stale_providers()),
            vec![AssetIssue::StaleProviderId {
                segment: "characterId".to_string(),
                provider: "character".to_string(),
                value: "bianca".to_string(),
            }]
        );
    }
}
