use crate::schema::{DirectiveDecl, Lowering};

/// plugin §8.1 closed vocabulary — owned by the core; a plugin MUST NOT invent flags.
pub const SEMANTICS_VOCAB: &[&str] = &[
    "writes.sceneState",
    "writes.characterState",
    "reads.onStage",
    "mayExitCharacter",
    "usesAnchor",
    "isExit",
    "isStateful",
    "mutatesScene",
    "requiresAnchor",
    "cancelsPrevious",
    "bridgeCall",
    // dsl 0.8.0: the directive unconditionally TERMINATES the walk at its own
    // record — nothing after it in the enclosing straight-line body ever runs.
    // Declared by `lute.core`'s `::end` ([`crate::core::END_DIRECTIVE`]) and by
    // nothing else: the flag DECLARES the semantics, but the compiler lowers a
    // terminator by TAG (`lower_directive`'s `"end"` arm → `Command::End`), so a
    // plugin cannot mint one by flagging a directive it also has to lower.
    "terminatesWalk",
];

/// The value kind a declarative-lowering target field accepts. The staging
/// commands only carry scalars, so this is the whole vocabulary — a record
/// field is never a list or a nested mapping.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LowerFieldKind {
    Str,
    Num,
    Bool,
}

impl LowerFieldKind {
    /// Label matching [`crate::types::type_str`]'s base spellings.
    pub fn label(self) -> &'static str {
        match self {
            LowerFieldKind::Str => "string",
            LowerFieldKind::Num => "number",
            LowerFieldKind::Bool => "bool",
        }
    }
}

/// The record kinds a plugin directive MAY lower into declaratively
/// (`lower: { record, fields }`, `docs/plugin-system.md`), each with its
/// bindable target-field names and kinds.
///
/// Deliberately narrow: the NON-control-flow staging kinds only. A declarative
/// mapping is a data↔code boundary — it can name a field and a source attr, but
/// it cannot express a branch target, a state write, a quest transition, or the
/// interpolation/addressing a `line` needs. Everything structural (`line`,
/// `set`, `assert`, `choice`, `match`, `hub`, `jump`, `barrier`, `quest`, `on`,
/// `end`) stays code-lowered, so it is absent here and rejected by
/// [`validate_directive`].
///
/// `addr` and the flattened `stamp` keys are intentionally NOT bindable: the
/// addressing pass owns `addr`, and `wait`/`duration`/`delay` resolve from the
/// authored timing attrs (dsl §7.5) uniformly for every record kind.
///
/// Field names are the SERIALIZED (camelCase) IR names, and the kind ORDER is
/// the spec'd order — [`lower_record_kinds`] renders it into the
/// `E-LOWER-RECORD-UNKNOWN` message, so message and table can never drift.
const LOWER_RECORDS: &[(&str, &[(&str, LowerFieldKind)])] = &[
    (
        "background",
        &[
            ("location", LowerFieldKind::Str),
            ("time", LowerFieldKind::Str),
            ("assetId", LowerFieldKind::Str),
        ],
    ),
    (
        "music",
        &[
            ("action", LowerFieldKind::Str),
            ("mood", LowerFieldKind::Str),
            ("volume", LowerFieldKind::Str),
            ("assetId", LowerFieldKind::Str),
            ("track", LowerFieldKind::Str),
        ],
    ),
    (
        "sfx",
        &[
            ("sound", LowerFieldKind::Str),
            ("assetId", LowerFieldKind::Str),
            ("name", LowerFieldKind::Str),
        ],
    ),
    (
        "vfx",
        &[
            ("vfxType", LowerFieldKind::Str),
            ("label", LowerFieldKind::Str),
            ("transition", LowerFieldKind::Str),
        ],
    ),
    (
        "sprite",
        &[
            ("character", LowerFieldKind::Str),
            ("anchor", LowerFieldKind::Str),
            ("action", LowerFieldKind::Str),
            ("exit", LowerFieldKind::Bool),
            ("posReset", LowerFieldKind::Bool),
            ("preload", LowerFieldKind::Bool),
            ("emotion", LowerFieldKind::Str),
            ("costume", LowerFieldKind::Str),
        ],
    ),
    (
        "camera",
        &[
            ("focus", LowerFieldKind::Str),
            ("zoom", LowerFieldKind::Num),
            ("moveX", LowerFieldKind::Num),
            ("moveY", LowerFieldKind::Num),
            ("shake", LowerFieldKind::Num),
            ("reset", LowerFieldKind::Bool),
            ("easing", LowerFieldKind::Str),
        ],
    ),
    (
        "cut",
        &[
            ("assetId", LowerFieldKind::Str),
            ("action", LowerFieldKind::Str),
            ("full", LowerFieldKind::Bool),
        ],
    ),
    (
        "video",
        &[
            ("assetId", LowerFieldKind::Str),
            ("action", LowerFieldKind::Str),
        ],
    ),
];

/// The bindable target fields of a declarative-lowering `record`, or `None`
/// when the name is not one of the staging kinds. The single source of truth
/// for BOTH assembly-time validation (here) and compile-time lowering
/// (`lute_compile::lower`).
pub fn lower_record_fields(record: &str) -> Option<&'static [(&'static str, LowerFieldKind)]> {
    LOWER_RECORDS
        .iter()
        .find(|(name, _)| *name == record)
        .map(|(_, fields)| *fields)
}

/// Every declarative-lowering record kind, in spec order.
pub fn lower_record_kinds() -> impl Iterator<Item = &'static str> {
    LOWER_RECORDS.iter().map(|(name, _)| *name)
}

#[derive(Clone, Debug, PartialEq)]
pub enum ManifestError {
    UnknownSemanticsFlag {
        directive: String,
        flag: String,
    },
    DuplicateAttr {
        directive: String,
        attr: String,
    },
    /// `lower: { record: X }` names something outside the staging kinds
    /// (`LOWER_RECORDS`) — the declaration would otherwise fall through to the
    /// `kind: "plugin"` passthrough and silently do nothing, so assembly
    /// rejects it.
    UnknownLowerRecord {
        directive: String,
        record: String,
    },
    /// `lower: { fields: … }` is malformed: a target field the record kind does
    /// not have, a `fromAttr` naming an attr the directive never declares, or a
    /// literal whose kind cannot fill the field.
    LowerRecordField {
        directive: String,
        record: String,
        detail: String,
    },
}

impl ManifestError {
    /// Stable, machine-readable code per variant; the assembler stamps it onto
    /// the `AssembleError` wrapping this, so a consumer keys on the SPECIFIC
    /// failure rather than a generic "invalid directive".
    pub fn code(&self) -> &'static str {
        match self {
            ManifestError::UnknownSemanticsFlag { .. } | ManifestError::DuplicateAttr { .. } => {
                "E-PLUGIN-INVALID-DIRECTIVE"
            }
            ManifestError::UnknownLowerRecord { .. } => "E-LOWER-RECORD-UNKNOWN",
            ManifestError::LowerRecordField { .. } => "E-LOWER-RECORD-FIELD",
        }
    }

    /// Author-facing message. The two structural errors keep their historical
    /// `Debug` rendering (consumers key on [`Self::code`], and the field names
    /// ARE the message); the declarative-lowering errors carry written prose,
    /// because their fix needs the accepted set spelled out.
    pub fn message(&self) -> String {
        match self {
            ManifestError::UnknownSemanticsFlag { .. } | ManifestError::DuplicateAttr { .. } => {
                format!("{self:?}")
            }
            ManifestError::UnknownLowerRecord { directive, record } => format!(
                "directive `::{directive}` lowers to unknown record `{record}`; \
                 declarative lowering targets the staging kinds ({})",
                lower_record_kinds().collect::<Vec<_>>().join(", ")
            ),
            ManifestError::LowerRecordField {
                directive,
                record,
                detail,
            } => format!("directive `::{directive}` lowers to record `{record}`: {detail}"),
        }
    }
}

pub fn validate_directive(d: &DirectiveDecl) -> Vec<ManifestError> {
    let mut errs = Vec::new();
    for flag in &d.semantics {
        if !SEMANTICS_VOCAB.contains(&flag.as_str()) {
            errs.push(ManifestError::UnknownSemanticsFlag {
                directive: d.name.clone(),
                flag: flag.clone(),
            });
        }
    }
    let mut seen = std::collections::BTreeSet::new();
    for a in &d.attrs {
        if !seen.insert(a.name.clone()) {
            errs.push(ManifestError::DuplicateAttr {
                directive: d.name.clone(),
                attr: a.name.clone(),
            });
        }
    }
    if let Lowering::Record { record, fields } = &d.lower {
        validate_record_lowering(d, record, fields, &mut errs);
    }
    errs
}

/// Validate a declarative `lower: { record, fields }` against `LOWER_RECORDS`
/// and the directive's OWN attrs. Runs at ASSEMBLY so a declaration that fails
/// here never reaches `lute_compile::lower`, which then binds every mapped
/// field without re-checking.
///
/// NOTE: a record-lowered directive's `effects.writes` are NOT carried — the
/// staging commands have no `effects` slot (only the `kind: "plugin"`
/// passthrough does). That is a property of the chosen target record, not a
/// malformed declaration, so it is documented here rather than diagnosed.
fn validate_record_lowering(
    d: &DirectiveDecl,
    record: &str,
    fields: &serde_yaml::Value,
    errs: &mut Vec<ManifestError>,
) {
    let Some(decls) = lower_record_fields(record) else {
        errs.push(ManifestError::UnknownLowerRecord {
            directive: d.name.clone(),
            record: record.to_string(),
        });
        return;
    };
    let mut bad = |detail: String| {
        errs.push(ManifestError::LowerRecordField {
            directive: d.name.clone(),
            record: record.to_string(),
            detail,
        });
    };
    // `fields:` with nothing after it deserializes to Null — a record lowering
    // that binds nothing is legal (every target field takes its default).
    let empty = serde_yaml::Mapping::new();
    let map = match fields {
        serde_yaml::Value::Mapping(m) => m,
        serde_yaml::Value::Null => &empty,
        _ => {
            bad(
                "`fields` must be a mapping of targetField -> { fromAttr: <attr> } | <literal>"
                    .to_string(),
            );
            return;
        }
    };
    for (k, v) in map {
        let Some(target) = k.as_str() else {
            bad("`fields` keys must be strings".to_string());
            continue;
        };
        let Some((_, kind)) = decls.iter().find(|(n, _)| *n == target) else {
            bad(format!(
                "unknown target field `{target}` (record `{record}` binds: {})",
                decls.iter().map(|(n, _)| *n).collect::<Vec<_>>().join(", ")
            ));
            continue;
        };
        match v {
            // Binding form: `{ fromAttr: <attrName> }`.
            serde_yaml::Value::Mapping(m)
                if m.contains_key(serde_yaml::Value::from("fromAttr")) =>
            {
                let Some(attr) = m
                    .get(serde_yaml::Value::from("fromAttr"))
                    .and_then(|a| a.as_str())
                else {
                    bad(format!(
                        "field `{target}`: `fromAttr` must be an attribute name string"
                    ));
                    continue;
                };
                if let Some(extra) = m.keys().filter_map(|k| k.as_str()).find(|k| *k != "fromAttr")
                {
                    bad(format!(
                        "field `{target}`: the binding form accepts only `fromAttr`, got `{extra}`"
                    ));
                    continue;
                }
                if !d.attrs.iter().any(|a| a.name == attr) {
                    let declared = if d.attrs.is_empty() {
                        "none".to_string()
                    } else {
                        d.attrs
                            .iter()
                            .map(|a| a.name.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    };
                    bad(format!(
                        "field `{target}` reads `fromAttr: {attr}`, but directive `::{}` \
                         declares no such attr (declared: {declared})",
                        d.name
                    ));
                }
            }
            // Literal form: a YAML scalar whose kind must fill the field.
            _ => {
                let ok = matches!(
                    (kind, v),
                    (LowerFieldKind::Str, serde_yaml::Value::String(_))
                        | (LowerFieldKind::Num, serde_yaml::Value::Number(_))
                        | (LowerFieldKind::Bool, serde_yaml::Value::Bool(_))
                );
                if !ok {
                    bad(format!(
                        "field `{target}` expects {} or {{ fromAttr: <attr> }}, got {}",
                        kind.label(),
                        yaml_kind(v)
                    ));
                }
            }
        }
    }
}

/// The YAML shape word for a value that could not fill a record field.
fn yaml_kind(v: &serde_yaml::Value) -> &'static str {
    match v {
        serde_yaml::Value::Null => "null",
        serde_yaml::Value::Bool(_) => "bool",
        serde_yaml::Value::Number(_) => "number",
        serde_yaml::Value::String(_) => "string",
        serde_yaml::Value::Sequence(_) => "a sequence",
        serde_yaml::Value::Mapping(_) => "a mapping",
        serde_yaml::Value::Tagged(_) => "a tagged value",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{AttrDecl, DirectiveDecl, Lowering};
    use crate::types::Type;

    fn dir(name: &str, semantics: &[&str]) -> DirectiveDecl {
        DirectiveDecl {
            name: name.into(),
            layer: None,
            attrs: vec![AttrDecl {
                name: "x".into(),
                required: false,
                ty: Type::Bool,
                default: None,
            }],
            semantics: semantics.iter().map(|s| s.to_string()).collect(),
            state: None,
            effects: None,
            bridge: None,
            lower: Lowering::Builtin {
                kind: "builtin".into(),
                name: "noop".into(),
            },
        }
    }

    #[test]
    fn unknown_semantics_flag_is_error() {
        let errs = validate_directive(&dir("d", &["writes.sceneState", "totallyMadeUp"]));
        assert!(errs.iter().any(|e| matches!(e, ManifestError::UnknownSemanticsFlag { flag, .. } if flag == "totallyMadeUp")));
    }

    #[test]
    fn known_semantics_flags_pass() {
        let errs = validate_directive(&dir("d", &["writes.sceneState", "bridgeCall"]));
        assert!(errs.is_empty());
    }

    #[test]
    fn duplicate_attr_name_is_error() {
        let mut d = dir("d", &[]);
        d.attrs.push(d.attrs[0].clone());
        let errs = validate_directive(&d);
        assert!(errs
            .iter()
            .any(|e| matches!(e, ManifestError::DuplicateAttr { .. })));
    }

    /// A directive named `backdrop` declaring one `img` attr and a declarative
    /// `lower: { record, fields }` built from the given YAML.
    fn record_dir(record: &str, fields_yaml: &str) -> DirectiveDecl {
        let mut d = dir("backdrop", &[]);
        d.attrs = vec![AttrDecl {
            name: "img".into(),
            required: true,
            ty: Type::Str,
            default: None,
        }];
        d.lower = Lowering::Record {
            record: record.into(),
            fields: serde_yaml::from_str(fields_yaml).expect("fixture yaml"),
        };
        d
    }

    #[test]
    fn valid_record_lowering_passes() {
        let errs = validate_directive(&record_dir(
            "background",
            "{ assetId: { fromAttr: img }, time: dusk }",
        ));
        assert!(errs.is_empty(), "{errs:?}");
    }

    #[test]
    fn unknown_record_name_is_rejected() {
        // `line` is control-flow/structural — deliberately NOT a staging kind.
        let errs = validate_directive(&record_dir("line", "{ speaker: { fromAttr: img } }"));
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert_eq!(errs[0].code(), "E-LOWER-RECORD-UNKNOWN");
        assert_eq!(
            errs[0].message(),
            "directive `::backdrop` lowers to unknown record `line`; declarative lowering \
             targets the staging kinds (background, music, sfx, vfx, sprite, camera, cut, video)"
        );
    }

    #[test]
    fn unknown_target_field_is_rejected() {
        let errs = validate_directive(&record_dir(
            "background",
            "{ backdropId: { fromAttr: img } }",
        ));
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert_eq!(errs[0].code(), "E-LOWER-RECORD-FIELD");
        assert_eq!(
            errs[0].message(),
            "directive `::backdrop` lowers to record `background`: unknown target field \
             `backdropId` (record `background` binds: location, time, assetId)"
        );
    }

    #[test]
    fn from_attr_naming_an_undeclared_attr_is_rejected() {
        let errs = validate_directive(&record_dir("background", "{ assetId: { fromAttr: nope } }"));
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert_eq!(errs[0].code(), "E-LOWER-RECORD-FIELD");
        assert_eq!(
            errs[0].message(),
            "directive `::backdrop` lowers to record `background`: field `assetId` reads \
             `fromAttr: nope`, but directive `::backdrop` declares no such attr (declared: img)"
        );
    }

    #[test]
    fn literal_of_the_wrong_kind_is_rejected() {
        // `camera.zoom` is a number; a bare string cannot fill it.
        let errs = validate_directive(&record_dir("camera", "{ zoom: wide }"));
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert_eq!(errs[0].code(), "E-LOWER-RECORD-FIELD");
        assert!(
            errs[0]
                .message()
                .ends_with("field `zoom` expects number or { fromAttr: <attr> }, got string"),
            "{}",
            errs[0].message()
        );
    }

    #[test]
    fn empty_field_map_is_legal() {
        assert!(validate_directive(&record_dir("video", "{}")).is_empty());
        assert!(validate_directive(&record_dir("video", "null")).is_empty());
    }

    #[test]
    fn builtin_lowering_is_never_record_checked() {
        // The whole `Lowering::Builtin` surface must stay untouched by C1/C2 work.
        let mut d = dir("legacy", &[]);
        d.lower = Lowering::Builtin {
            kind: "builtin".into(),
            name: "minigame".into(),
        };
        assert!(validate_directive(&d).is_empty());
    }

    #[test]
    fn structural_kinds_are_all_outside_the_record_table() {
        for k in [
            "line", "set", "assert", "choice", "match", "hub", "jump", "barrier", "quest", "on",
            "end", "plugin",
        ] {
            assert!(
                lower_record_fields(k).is_none(),
                "`{k}` must not be declaratively lowerable"
            );
        }
        for k in lower_record_kinds() {
            assert!(lower_record_fields(k).is_some(), "`{k}` must be in the table");
        }
    }
}
