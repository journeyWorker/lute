use crate::schema::AssetKindDecl;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{AssetKindDecl, AssetResolve, AssetSegment};

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
                    ty: None,
                },
                AssetSegment {
                    name: "costume".to_string(),
                    r#const: None,
                    ty: None,
                },
                AssetSegment {
                    name: "emotion".to_string(),
                    r#const: None,
                    ty: None,
                },
                AssetSegment {
                    name: "variant".to_string(),
                    r#const: None,
                    ty: None,
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
}
