use crate::schema::{DirectiveDecl, Lowering, OpBy, RewardKindDecl, WriteValue};
use crate::types::{Literal, Type};

/// plugin §8.1 closed vocabulary — owned by the core; a plugin MUST NOT invent flags.
pub const SEMANTICS_VOCAB: &[&str] = &[
    // `isStateful` and `cancelsPrevious` were removed in plugin 0.0.3: no
    // shipped directive declared either and neither had a consumer, and a
    // CLOSED vocabulary must not advertise flags that do nothing.
    "writes.sceneState",
    "writes.characterState",
    "reads.onStage",
    "mayExitCharacter",
    "usesAnchor",
    "isExit",
    "mutatesScene",
    "requiresAnchor",
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
    Bool,
}

impl LowerFieldKind {
    /// Label matching [`crate::types::type_str`]'s base spellings.
    pub fn label(self) -> &'static str {
        match self {
            LowerFieldKind::Str => "string",
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
        "bg",
        &[
            ("location", LowerFieldKind::Str),
            ("time", LowerFieldKind::Str),
            ("assetId", LowerFieldKind::Str),
        ],
    ),
    (
        "music",
        &[
            ("playback", LowerFieldKind::Str),
            ("mood", LowerFieldKind::Str),
            ("volume", LowerFieldKind::Str),
            ("assetId", LowerFieldKind::Str),
        ],
    ),
    (
        "sfx",
        &[
            ("sound", LowerFieldKind::Str),
            ("assetId", LowerFieldKind::Str),
        ],
    ),
    (
        "vfx",
        &[
            ("type", LowerFieldKind::Str),
            ("label", LowerFieldKind::Str),
            ("transition", LowerFieldKind::Str),
        ],
    ),
    (
        "actor",
        &[
            ("character", LowerFieldKind::Str),
            ("anchor", LowerFieldKind::Str),
            ("action", LowerFieldKind::Str),
            ("exit", LowerFieldKind::Bool),
            ("emotion", LowerFieldKind::Str),
            ("costume", LowerFieldKind::Str),
            ("posReset", LowerFieldKind::Bool),
            ("preload", LowerFieldKind::Bool),
        ],
    ),
    (
        "camera",
        &[
            ("focus", LowerFieldKind::Str),
            ("framing", LowerFieldKind::Str),
            ("move", LowerFieldKind::Str),
            ("transition", LowerFieldKind::Str),
        ],
    ),
    (
        "cg",
        &[
            ("assetId", LowerFieldKind::Str),
            ("display", LowerFieldKind::Str),
            ("layout", LowerFieldKind::Str),
        ],
    ),
    (
        "video",
        &[
            ("assetId", LowerFieldKind::Str),
            ("display", LowerFieldKind::Str),
        ],
    ),
    ("sequence", &[("name", LowerFieldKind::Str)]),
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
    /// dsl 0.41.0 §5.5: an attr's declared `default` does not inhabit its
    /// declared `type` — the compiler ships the default as the call's field,
    /// so it must be a value an author could have written.
    AttrDefaultType {
        directive: String,
        attr: String,
        ty: String,
        default: String,
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
    /// dsl 0.27.0 §2 (T1-4): an `effects.writes` value reads `fromAttr: <a>`
    /// but the directive declares no attr `a`, or a `by: { fromAttr: a }`
    /// names an attr that is not `type: int`.
    EffectWrite {
        directive: String,
        detail: String,
    },
    /// dsl 0.27.0 §4: an `effects.asserts` / `effects.retracts` entry reads
    /// `@a` but the directive declares no attr `a`, or an assert has `_`.
    EffectFact {
        directive: String,
        list: &'static str,
        detail: String,
    },
    /// 0.50.0 §5.1: a reward kind's `effects` reads an `@attr` other than
    /// `@target`, asserts `_`, or reads `@target` on a kind that takes none.
    RewardEffect {
        kind: String,
        detail: String,
    },
    /// 0.51.0 §4.2: an `effects.raises` target `@a` names no attribute of
    /// the directive.
    EffectRaise {
        directive: String,
        detail: String,
    },
    /// 0.51.0 §4.2 (Review051b F4): `effects.raises` on a directive whose
    /// `lower:` builds a staging record or runs a builtin hook — neither
    /// carries effects, so the raise could never reach the host.
    RaisesWithoutCarrier {
        directive: String,
        lowering: StagingLowering,
    },
    /// 0.49.0 §3.2 (#176): a directive attribute or a `stampAttrs` entry
    /// declared `list`, `map` or `record`. An attribute value is authored as
    /// one scalar, so no `.lute` value could satisfy the declaration.
    StructuredAttr {
        owner: AttrOwner,
        attr: String,
        ty: String,
    },
}

/// Who declares an attribute [`ManifestError::StructuredAttr`] names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AttrOwner {
    /// A directive's own `attrs:` entry; the directive name.
    Directive(String),
    /// A `stampAttrs` export entry; the declaring plugin id.
    Stamp(String),
}

/// A `lower:` that is not the `plugin` passthrough
/// ([`ManifestError::RaisesWithoutCarrier`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StagingLowering {
    /// `{ record, fields }`: the staging record it builds.
    Record(String),
    /// `{ kind: builtin, name }`: the hook it runs.
    Builtin(String),
}

impl ManifestError {
    /// Stable, machine-readable code per variant; the assembler stamps it onto
    /// the `AssembleError` wrapping this, so a consumer keys on the SPECIFIC
    /// failure rather than a generic "invalid directive".
    pub fn code(&self) -> &'static str {
        match self {
            ManifestError::UnknownSemanticsFlag { .. }
            | ManifestError::DuplicateAttr { .. }
            | ManifestError::AttrDefaultType { .. }
            | ManifestError::StructuredAttr { .. }
            | ManifestError::RaisesWithoutCarrier { .. } => "E-PLUGIN-INVALID-DIRECTIVE",
            ManifestError::UnknownLowerRecord { .. } => "E-LOWER-RECORD-UNKNOWN",
            ManifestError::LowerRecordField { .. } => "E-LOWER-RECORD-FIELD",
            ManifestError::EffectWrite { .. }
            | ManifestError::EffectFact { .. }
            | ManifestError::RewardEffect { .. }
            | ManifestError::EffectRaise { .. } => "E-PLUGIN-PARSE",
        }
    }

    /// Author-facing message: the [`std::fmt::Display`] text.
    pub fn message(&self) -> String {
        self.to_string()
    }
}

impl std::fmt::Display for ManifestError {
    /// Author-facing message: what is wrong with the declaration and the
    /// accepted set that fixes it — never a Rust `Debug` form.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ManifestError::UnknownSemanticsFlag { directive, flag } => write!(
                f,
                "directive `::{directive}` declares semantics flag `{flag}`, which is not one \
                 of {}{}",
                SEMANTICS_VOCAB.join(", "),
                crate::suggest::did_you_mean(flag, SEMANTICS_VOCAB.iter().copied())
            ),
            ManifestError::DuplicateAttr { directive, attr } => write!(
                f,
                "directive `::{directive}` declares attribute `{attr}` more than once — keep \
                 one declaration"
            ),
            ManifestError::AttrDefaultType {
                directive,
                attr,
                ty,
                default,
            } => write!(
                f,
                "directive `::{directive}` declares attribute `{attr}` as `{ty}` with \
                 `default: {default}`, which is not of type `{ty}` — give a default of the \
                 declared type"
            ),
            ManifestError::UnknownLowerRecord { directive, record } => write!(
                f,
                "directive `::{directive}` lowers to unknown record `{record}`; \
                 declarative lowering targets the staging kinds ({})",
                lower_record_kinds().collect::<Vec<_>>().join(", ")
            ),
            ManifestError::LowerRecordField {
                directive,
                record,
                detail,
            } => write!(
                f,
                "directive `::{directive}` lowers to record `{record}`: {detail}"
            ),
            ManifestError::EffectWrite { directive, detail } => {
                write!(f, "directive `::{directive}` effects.writes: {detail}")
            }
            ManifestError::EffectFact {
                directive,
                list,
                detail,
            } => write!(f, "directive `::{directive}` effects.{list}: {detail}"),
            ManifestError::RewardEffect { kind, detail } => {
                write!(f, "reward kind `{kind}` {detail}")
            }
            ManifestError::EffectRaise { directive, detail } => {
                write!(f, "directive `::{directive}` effects.raises: {detail}")
            }
            ManifestError::RaisesWithoutCarrier {
                directive,
                lowering,
            } => {
                match lowering {
                    StagingLowering::Record(record) => {
                        write!(
                            f,
                            "directive `::{directive}` lowers to the `{record}` record"
                        )?;
                    }
                    StagingLowering::Builtin(hook) => write!(
                        f,
                        "directive `::{directive}` lowers through the builtin `{hook}` hook"
                    )?,
                }
                write!(
                    f,
                    ", which carries no effects (only a `plugin` record does), so its \
                     `effects.raises` could never reach the host — drop `lower:` so the call \
                     stays a `plugin` record, or raise the occasion from a passthrough directive"
                )
            }
            ManifestError::StructuredAttr { owner, attr, ty } => {
                match owner {
                    AttrOwner::Directive(directive) => {
                        write!(f, "directive `::{directive}` declares attribute `{attr}`")?;
                    }
                    AttrOwner::Stamp(plugin) => {
                        write!(f, "plugin `{plugin}` declares stamp attribute `{attr}`")?;
                    }
                }
                write!(
                    f,
                    " as `{ty}`; an attribute value is written as one scalar (string, \
                     number, bool, an enum or entity member, a provider or asset id), so no \
                     `.lute` value can satisfy a `list`, `map` or `record` — declare it `string` \
                     and parse it in the host, or split it into one attribute per field"
                )
            }
        }
    }
}

impl std::error::Error for ManifestError {}

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
        if let Some(default) = a
            .default
            .as_ref()
            .filter(|v| !attr_default_accepts(&a.ty, v))
        {
            errs.push(ManifestError::AttrDefaultType {
                directive: d.name.clone(),
                attr: a.name.clone(),
                ty: crate::types::type_str(&a.ty),
                default: crate::types::lit_str(default),
            });
        }
        if let Some(e) = structured_attr(AttrOwner::Directive(d.name.clone()), a) {
            errs.push(e);
        }
    }
    if let Lowering::Record { record, fields } = &d.lower {
        validate_record_lowering(d, record, fields, &mut errs);
    }
    if let Some(lowering) = raises_without_carrier(d) {
        errs.push(ManifestError::RaisesWithoutCarrier {
            directive: d.name.clone(),
            lowering,
        });
    }
    errs.extend(validate_effects(d));
    errs
}

/// 0.51.0 §4.2 (Review051b F4): the lowering of `d` when it declares
/// `effects.raises` but lowers to a staging record or a builtin hook, which
/// carry no effects ([`ManifestError::RaisesWithoutCarrier`]). Assembly
/// drops such raises, so no consumer ever reads one.
pub fn raises_without_carrier(d: &DirectiveDecl) -> Option<StagingLowering> {
    if d.effects.as_ref().is_none_or(|e| e.raises.is_empty()) {
        return None;
    }
    match &d.lower {
        Lowering::Record { record, .. } => Some(StagingLowering::Record(record.clone())),
        Lowering::Builtin { name, .. } => Some(StagingLowering::Builtin(name.clone())),
        Lowering::Passthrough => None,
    }
}

/// 0.49.0 §3.2: [`ManifestError::StructuredAttr`] when `a`'s top-level type is
/// `list`, `map` or `record`. Plugin `options:`, frontmatter keys, state
/// shapes and bridge results keep those types: YAML can write them.
pub fn structured_attr(owner: AttrOwner, a: &crate::schema::AttrDecl) -> Option<ManifestError> {
    matches!(a.ty, Type::List(_) | Type::Map { .. } | Type::Record(_)).then(|| {
        ManifestError::StructuredAttr {
            owner,
            attr: a.name.clone(),
            ty: crate::types::type_str(&a.ty),
        }
    })
}

/// Whether an attr's declared `default` is a value of its declared type: the
/// shared [`crate::types::type_accepts`], plus a YAML integer for a `double`
/// (`default: 1` reads `1.0`) and any string for the types whose members
/// resolve against the project (`enumFromOption`, `providerRef`).
pub fn attr_default_accepts(ty: &Type, default: &Literal) -> bool {
    match (ty, default) {
        (Type::Double, Literal::Int(_)) => true,
        (Type::EnumFromOption(_) | Type::ProviderRef(_), Literal::Str(_)) => true,
        _ => crate::types::type_accepts(ty, default),
    }
}

/// 0.41.1 §3.5: a directive declared `layer: bridge` with `bridgeCall`
/// semantics but no `effects.writes` entry reading a bridge result — it
/// would compile to a non-blocking record whose answer nothing waits for.
pub const E_BRIDGE_NO_RESULT_WRITE: &str = "E-BRIDGE-NO-RESULT-WRITE";

/// The [`E_BRIDGE_NO_RESULT_WRITE`] message for `d`, when `d` is such a
/// declaration. The loader reports it at the declaration's `name:`.
pub fn bridge_without_result_write(d: &DirectiveDecl) -> Option<String> {
    let nominal =
        d.layer.as_deref() == Some("bridge") && d.semantics.iter().any(|s| s == "bridgeCall");
    let reads_result = d
        .effects
        .iter()
        .flat_map(|e| &e.writes)
        .any(|w| matches!(w.value, WriteValue::FromBridgeResult { .. }));
    (nominal && !reads_result).then(|| {
        format!(
            "bridge directive `{}` declares `layer: bridge` but no `effects.writes` reads a \
             bridge result; it will not block — add a `fromBridgeResult` write or remove \
             `layer: bridge`",
            d.name
        )
    })
}

/// dsl 0.27.0 §2, §4, 0.51.0 §4.2: the directive's declared `effects`
/// against its own attrs. The loader runs this where it knows the file, so
/// a fault names its line ([`crate::loader`]); assembly re-runs it for a
/// declaration that never came from a file.
pub fn validate_effects(d: &DirectiveDecl) -> Vec<ManifestError> {
    let mut errs = Vec::new();
    validate_effect_writes(d, &mut errs);
    validate_effect_facts(d, &mut errs);
    validate_effect_raises(d, &mut errs);
    errs
}

/// 0.51.0 §4.2: every `@attr` a declared raise targets names one of the
/// directive's own attrs. Whether the occasion exists and takes a target of
/// that type is judged at assembly (`E-PLUGIN-RAISES`), where every active
/// plugin's occasions are known.
fn validate_effect_raises(d: &DirectiveDecl, errs: &mut Vec<ManifestError>) {
    for r in d.effects.iter().flat_map(|e| &e.raises) {
        let Some(attr) = r
            .target
            .as_ref()
            .and_then(crate::schema::RaiseTargetDecl::attr)
        else {
            continue;
        };
        if d.attrs.iter().any(|a| a.name == attr) {
            continue;
        }
        let hint = crate::suggest::nearest(attr, d.attrs.iter().map(|a| a.name.as_str()), 2)
            .map(|s| format!("; did you mean `@{s}`?"))
            .unwrap_or_default();
        let declared: Vec<&str> = d.attrs.iter().map(|a| a.name.as_str()).collect();
        errs.push(ManifestError::EffectRaise {
            directive: d.name.clone(),
            detail: format!(
                "`@{attr}` names no attribute of the directive (declared: {}){hint}; a raise's \
                 `target: \"@<attr>\"` reads one of the directive's own attributes",
                if declared.is_empty() {
                    "none".to_string()
                } else {
                    declared.join(", ")
                }
            ),
        });
    }
}

/// dsl 0.27.0 §2 (T1-4): every `fromAttr` a write's value reads names one of
/// the directive's own attrs, and a `by:` one is `type: int`. The value's
/// SHAPE was already checked when the file parsed ([`WriteValue`]).
fn validate_effect_writes(d: &DirectiveDecl, errs: &mut Vec<ManifestError>) {
    for w in d.effects.iter().flat_map(|e| &e.writes) {
        let (attr, in_by) = match &w.value {
            WriteValue::FromAttr { from_attr } => (from_attr, false),
            WriteValue::Op {
                by: OpBy::FromAttr { from_attr },
                ..
            } => (from_attr, true),
            _ => continue,
        };
        let detail = match d.attrs.iter().find(|a| &a.name == attr) {
            None => {
                let hint =
                    crate::suggest::nearest(attr, d.attrs.iter().map(|a| a.name.as_str()), 2)
                        .map(|s| format!("; did you mean `{s}`?"))
                        .unwrap_or_default();
                let declared: Vec<&str> = d.attrs.iter().map(|a| a.name.as_str()).collect();
                format!(
                    "a write reads `fromAttr: {attr}`, but the directive declares no such attr \
                     (declared: {}){hint}",
                    declared.join(", ")
                )
            }
            Some(a) if in_by && a.ty != Type::Int => format!(
                "`by: {{ fromAttr: {attr} }}` needs an `int` attr, but `{attr}` is not an int"
            ),
            Some(_) => continue,
        };
        errs.push(ManifestError::EffectWrite {
            directive: d.name.clone(),
            detail,
        });
    }
}

/// dsl 0.27.0 §4: every `@attr` a declared fact effect reads names one of the
/// directive's own attrs, and an assert is ground once bound (no `_`). The
/// pattern's SHAPE was already checked when the file parsed
/// ([`crate::schema::FactEffect`]); the relation and its argument kinds are
/// the project's, so the checker judges them at each call.
fn validate_effect_facts(d: &DirectiveDecl, errs: &mut Vec<ManifestError>) {
    let Some(effects) = &d.effects else {
        return;
    };
    for (list, facts) in [
        ("asserts", &effects.asserts),
        ("retracts", &effects.retracts),
    ] {
        for fact in facts {
            let faults = fact_faults(fact, list == "asserts", |attr| {
                d.attrs.iter().any(|a| a.name == attr)
            });
            if faults.asserts_wildcard {
                errs.push(ManifestError::EffectFact {
                    directive: d.name.clone(),
                    list,
                    detail: format!(
                        "`{fact}` asserts `_`; an asserted fact is ground — name a member or an \
                         `@attr` (only a retract takes `_`)"
                    ),
                });
            }
            for attr in faults.foreign {
                let hint =
                    crate::suggest::nearest(attr, d.attrs.iter().map(|a| a.name.as_str()), 2)
                        .map(|s| format!("; did you mean `@{s}`?"))
                        .unwrap_or_default();
                let declared: Vec<&str> = d.attrs.iter().map(|a| a.name.as_str()).collect();
                errs.push(ManifestError::EffectFact {
                    directive: d.name.clone(),
                    list,
                    detail: format!(
                        "`{fact}` reads `@{attr}`, but the directive declares no such attr \
                         (declared: {}){hint}",
                        declared.join(", ")
                    ),
                });
            }
        }
    }
}

/// What one declared fact effect gets wrong against its owner's attributes
/// — a directive's own attrs, or a reward kind's `@target` alone.
struct FactFaults<'f> {
    /// An assert with a `_` argument (only a retract takes `_`).
    asserts_wildcard: bool,
    /// Each `@attr` the owner does not admit, in argument order.
    foreign: Vec<&'f str>,
}

fn fact_faults<'f>(
    fact: &'f crate::schema::FactEffect,
    assert: bool,
    admits: impl Fn(&str) -> bool,
) -> FactFaults<'f> {
    FactFaults {
        asserts_wildcard: assert
            && fact
                .args
                .iter()
                .any(|a| matches!(a, crate::schema::FactEffectArg::Wildcard)),
        foreign: fact.attrs().filter(|a| !admits(a)).collect(),
    }
}

/// 0.50.0 §5.1: a reward kind's declared `effects` against what a
/// `<reward>` can bind. A fact reads only `@target` (a `<reward>` takes no
/// other attribute, so a kind's `attrs:` cannot bind one), an assert is
/// ground once bound, and a kind whose `target:` is `forbidden` reads no
/// `@target` (no reward of it could bind one). 0.51.0 §4.2: a raise targets
/// `@target` or a literal, under the same `forbidden` rule. The shapes were
/// checked when the file parsed ([`crate::schema::RewardEffects`]).
pub fn validate_reward_effects(kind: &RewardKindDecl) -> Vec<ManifestError> {
    use crate::schema::REWARD_TARGET_ATTR;
    let mut errs = Vec::new();
    let Some(effects) = &kind.effects else {
        return errs;
    };
    let mut fault = |detail: String| {
        errs.push(ManifestError::RewardEffect {
            kind: kind.name.clone(),
            detail,
        });
    };
    for (fact, assert) in effects.facts() {
        let faults = fact_faults(fact, assert, |attr| attr == REWARD_TARGET_ATTR);
        if faults.asserts_wildcard {
            fault(format!(
                "effect `{fact}` asserts `_`; an asserted fact is ground — name a member or \
                 `@{REWARD_TARGET_ATTR}` (only a retract takes `_`)"
            ));
        }
        for attr in faults.foreign {
            fault(format!(
                "effect `{fact}` reads `@{attr}`; a reward kind's effects read only \
                 `@{REWARD_TARGET_ATTR}` (a `<reward>` takes no other attribute)"
            ));
        }
    }
    for r in &effects.raises {
        let Some(attr) = r
            .target
            .as_ref()
            .and_then(crate::schema::RaiseTargetDecl::attr)
            .filter(|a| *a != REWARD_TARGET_ATTR)
        else {
            continue;
        };
        fault(format!(
            "raises `{}` for `@{attr}`; a reward kind's raise targets `@{REWARD_TARGET_ATTR}` \
             (the `<reward>`'s `target=`) or a literal target — a `<reward>` takes no other \
             attribute",
            r.occasion
        ));
    }
    if kind.target.as_ref().is_some_and(|t| t.forbidden) && effects.reads_target() {
        let reads: Vec<String> = effects
            .facts()
            .filter(|(f, _)| f.attrs().any(|a| a == REWARD_TARGET_ATTR))
            .map(|(f, _)| f.to_string())
            .chain(
                effects
                    .accepts
                    .iter()
                    .filter(|a| **a == crate::schema::AcceptEffect::Target)
                    .map(ToString::to_string),
            )
            .collect();
        let forbidden = format!(
            "`@{REWARD_TARGET_ATTR}`, but the kind declares `target: {{ forbidden: true }}`, so \
             no `<reward>` of it has a target to bind"
        );
        match (reads.first(), effects.target_raises().next()) {
            (Some(read), _) => fault(format!("effect `{read}` reads {forbidden}")),
            (None, Some(r)) => fault(format!("raises `{}` for {forbidden}", r.occasion)),
            (None, None) => {}
        }
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
    fields: &yaml_serde::Value,
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
    let empty = yaml_serde::Mapping::new();
    let map = match fields {
        yaml_serde::Value::Mapping(m) => m,
        yaml_serde::Value::Null => &empty,
        _ => {
            bad(
                "`fields` must be a mapping of targetField -> { fromAttr: <attr> } | <literal>"
                    .to_string(),
            );
            return;
        }
    };
    if record == "actor" {
        match map.get(yaml_serde::Value::from("character")) {
            Some(yaml_serde::Value::String(_)) => {}
            Some(yaml_serde::Value::Mapping(binding))
                if binding
                    .get(yaml_serde::Value::from("fromAttr"))
                    .and_then(|value| value.as_str())
                    .and_then(|name| d.attrs.iter().find(|attr| attr.name == name))
                    .is_some_and(|attr| attr.required) => {}
            Some(yaml_serde::Value::Mapping(binding)) => {
                let attr = binding
                    .get(yaml_serde::Value::from("fromAttr"))
                    .and_then(|value| value.as_str())
                    .unwrap_or("<missing>");
                bad(format!(
                    "field `character` must bind `fromAttr: {attr}` to a required attribute \
                     (`required: true`), or use a literal string; make `::{}`'s character \
                     attribute required or bind a literal string",
                    d.name
                ));
            }
            Some(_) | None => bad(format!(
                "field `character` must bind to a required attribute \
                 (`fromAttr` with `required: true`) or a literal string; add a required \
                 character attribute to `::{}` or bind a literal string",
                d.name
            )),
        }
    }
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
            yaml_serde::Value::Mapping(m)
                if m.contains_key(yaml_serde::Value::from("fromAttr")) =>
            {
                let Some(attr) = m
                    .get(yaml_serde::Value::from("fromAttr"))
                    .and_then(|a| a.as_str())
                else {
                    bad(format!(
                        "field `{target}`: `fromAttr` must be an attribute name string"
                    ));
                    continue;
                };
                if let Some(extra) = m
                    .keys()
                    .filter_map(|k| k.as_str())
                    .find(|k| *k != "fromAttr")
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
                    (LowerFieldKind::Str, yaml_serde::Value::String(_))
                        | (LowerFieldKind::Bool, yaml_serde::Value::Bool(_))
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
fn yaml_kind(v: &yaml_serde::Value) -> &'static str {
    match v {
        yaml_serde::Value::Null => "null",
        yaml_serde::Value::Bool(_) => "bool",
        yaml_serde::Value::Number(_) => "number",
        yaml_serde::Value::String(_) => "string",
        yaml_serde::Value::Sequence(_) => "a sequence",
        yaml_serde::Value::Mapping(_) => "a mapping",
        yaml_serde::Value::Tagged(_) => "a tagged value",
    }
}

/// dsl 0.9.0 D-D: the domain slots whose members carry compiler-relevant
/// semantics. The core owns the SLOT (it knows `action` needs to say which
/// members exit); the project owns the MEMBERS. A declaration of one of these
/// names without its semantics key is an error, never a fallback — a silent
/// fallback to a `fadeOut*` prefix rule is the hidden coupling 0.9.0 removes.
/// dsl 0.9.0 D-D: slots that must declare the member semantics.
pub const SLOT_REQUIRES_EXITS: &[&str] = &["action"];
/// dsl 0.9.0 D-D: slots that must declare the member used when absent.
pub const SLOT_REQUIRES_DEFAULT: &[&str] = &["anchor"];
/// dsl 0.40.0 §2.2: `musicPlayback` may declare members that stop current music.
pub const SLOT_ALLOWS_STOPS: &[&str] = &["musicPlayback"];

/// A member-semantics defect in one domain declaration (dsl 0.9.0 D-D).
/// Shape mirrors [`crate::asset::AssetIssue`]: a data-only finding the caller
/// renders into its own diagnostic type, so the plugin path
/// (`AssembleError`) and the project path (`lute-check`'s `uses_diag`) share
/// one rule set instead of duplicating it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DomainIssue {
    DefaultNotMember {
        name: String,
        value: String,
    },
    ExitNotMember {
        name: String,
        value: String,
    },
    StopsNotMember {
        name: String,
        value: String,
    },
    MissingSemantics {
        name: String,
        key: &'static str,
    },
    UnexpectedSemantics {
        name: String,
        key: &'static str,
    },
    /// dsl 0.24.0 §1: a `labels:` key that is not a member, with the
    /// did-you-mean for the member it most likely means (or `""`).
    LabelNotMember {
        name: String,
        value: String,
        hint: String,
    },
}

impl DomainIssue {
    pub fn code(&self) -> &'static str {
        match self {
            DomainIssue::DefaultNotMember { .. } => "E-ENUM-DEFAULT-NOT-MEMBER",
            DomainIssue::ExitNotMember { .. } => "E-ENUM-EXITS-NOT-MEMBER",
            DomainIssue::StopsNotMember { .. } => "E-ENUM-STOPS-NOT-MEMBER",
            DomainIssue::MissingSemantics { .. } => "E-ENUM-MISSING-SEMANTICS",
            DomainIssue::UnexpectedSemantics { .. } => "E-ENUM-UNEXPECTED-SEMANTICS",
            DomainIssue::LabelNotMember { .. } => "E-ENUM-LABEL-NOT-MEMBER",
        }
    }

    pub fn message(&self) -> String {
        match self {
            DomainIssue::DefaultNotMember { name, value } => format!(
                "domain `{name}` declares `default: {value}`, which is not one of its members"
            ),
            DomainIssue::ExitNotMember { name, value } => format!(
                "domain `{name}` lists `{value}` in `exits:`, which is not one of its members"
            ),
            DomainIssue::StopsNotMember { name, value } => format!(
                "domain `{name}` lists `{value}` in `stops:`, which is not one of its members"
            ),
            DomainIssue::MissingSemantics { name, key } => format!(
                "domain `{name}` must declare `{key}:` — the compiler reads it instead of \
                 inferring member semantics (dsl 0.9.0 D-D)"
            ),
            DomainIssue::UnexpectedSemantics { name, key } => format!(
                "domain `{name}` declares `{key}:`, which has no meaning for this slot \
                 (dsl 0.9.0 D-D)"
            ),
            DomainIssue::LabelNotMember { name, value, hint } => format!(
                "domain `{name}` declares a label for `{value}` in `labels:`, which is not one \
                 of its members{hint} (dsl 0.24.0 §1)"
            ),
        }
    }
}

/// Validate one domain declaration's member semantics (dsl 0.9.0 D-D). Pure
/// and total. An OPEN (registry-style) domain has no static member list, so
/// every rule here is vacuous for it.
pub fn validate_domain(name: &str, d: &crate::snapshot::Domain) -> Vec<DomainIssue> {
    let mut out = Vec::new();
    if d.open {
        return out;
    }
    let wants_exits = SLOT_REQUIRES_EXITS.contains(&name);
    let wants_default = SLOT_REQUIRES_DEFAULT.contains(&name);

    if let Some(v) = &d.default {
        if !wants_default {
            out.push(DomainIssue::UnexpectedSemantics {
                name: name.to_string(),
                key: "default",
            });
        } else if !d.members.contains(v) {
            out.push(DomainIssue::DefaultNotMember {
                name: name.to_string(),
                value: v.clone(),
            });
        }
    } else if wants_default {
        out.push(DomainIssue::MissingSemantics {
            name: name.to_string(),
            key: "default",
        });
    }

    if !d.exits.is_empty() {
        if !wants_exits {
            out.push(DomainIssue::UnexpectedSemantics {
                name: name.to_string(),
                key: "exits",
            });
        } else {
            for v in &d.exits {
                if !d.members.contains(v) {
                    out.push(DomainIssue::ExitNotMember {
                        name: name.to_string(),
                        value: v.clone(),
                    });
                }
            }
        }
    } else if wants_exits {
        out.push(DomainIssue::MissingSemantics {
            name: name.to_string(),
            key: "exits",
        });
    }
    if !d.stops.is_empty() {
        if !SLOT_ALLOWS_STOPS.contains(&name) {
            out.push(DomainIssue::UnexpectedSemantics {
                name: name.to_string(),
                key: "stops",
            });
        } else {
            for v in &d.stops {
                if !d.members.contains(v) {
                    out.push(DomainIssue::StopsNotMember {
                        name: name.to_string(),
                        value: v.clone(),
                    });
                }
            }
        }
    }
    // dsl 0.24.0 §1: labels are display text for members, legal on any slot.
    for member in d.labels.keys() {
        if !d.members.contains(member) {
            out.push(DomainIssue::LabelNotMember {
                name: name.to_string(),
                value: member.clone(),
                hint: crate::suggest::did_you_mean(member, d.members.iter().map(String::as_str)),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::AttrDecl;

    fn dom(members: &[&str], default: Option<&str>, exits: &[&str]) -> crate::snapshot::Domain {
        crate::snapshot::Domain {
            members: members
                .iter()
                .map(std::string::ToString::to_string)
                .collect(),
            open: false,
            default: default.map(str::to_string),
            exits: exits.iter().map(std::string::ToString::to_string).collect(),
            stops: Vec::new(),
            labels: Default::default(),
        }
    }

    #[test]
    fn action_requires_exits_anchor_requires_default() {
        let missing_exits = validate_domain("action", &dom(&["sway"], None, &[]));
        assert_eq!(
            missing_exits
                .iter()
                .map(super::DomainIssue::code)
                .collect::<Vec<_>>(),
            ["E-ENUM-MISSING-SEMANTICS"]
        );
        let missing_default = validate_domain("anchor", &dom(&["left"], None, &[]));
        assert_eq!(
            missing_default
                .iter()
                .map(super::DomainIssue::code)
                .collect::<Vec<_>>(),
            ["E-ENUM-MISSING-SEMANTICS"]
        );
        assert!(validate_domain("action", &dom(&["sway", "hide"], None, &["hide"])).is_empty());
        assert!(validate_domain("anchor", &dom(&["left"], Some("left"), &[])).is_empty());
    }

    #[test]
    fn semantics_must_reference_members() {
        assert_eq!(
            validate_domain("action", &dom(&["sway"], None, &["zzz"]))
                .iter()
                .map(super::DomainIssue::code)
                .collect::<Vec<_>>(),
            ["E-ENUM-EXITS-NOT-MEMBER"]
        );
        assert_eq!(
            validate_domain("anchor", &dom(&["left"], Some("zzz"), &[]))
                .iter()
                .map(super::DomainIssue::code)
                .collect::<Vec<_>>(),
            ["E-ENUM-DEFAULT-NOT-MEMBER"]
        );
    }
    #[test]
    fn music_playback_stops_are_optional_and_member_checked() {
        let mut music = dom(&["start", "stop"], None, &[]);
        assert!(validate_domain("musicPlayback", &music).is_empty());
        music.stops = vec!["stop".into()];
        assert!(validate_domain("musicPlayback", &music).is_empty());
        music.stops.push("fadeOut".into());
        assert_eq!(
            validate_domain("musicPlayback", &music)
                .iter()
                .map(super::DomainIssue::code)
                .collect::<Vec<_>>(),
            ["E-ENUM-STOPS-NOT-MEMBER"]
        );
        music.stops = vec!["start".into()];
        assert_eq!(
            validate_domain("emotion", &music)
                .iter()
                .map(super::DomainIssue::code)
                .collect::<Vec<_>>(),
            ["E-ENUM-UNEXPECTED-SEMANTICS"]
        );
    }

    #[test]
    fn labels_must_name_members_on_any_slot() {
        let mut d = dom(&["mon", "sun"], None, &[]);
        d.labels.insert("sun".into(), "Sunday".into());
        assert!(validate_domain("weekday", &d).is_empty());
        d.labels.insert("thur".into(), "Thursday".into());
        let issues = validate_domain("weekday", &d);
        assert_eq!(
            issues
                .iter()
                .map(super::DomainIssue::code)
                .collect::<Vec<_>>(),
            ["E-ENUM-LABEL-NOT-MEMBER"]
        );
        assert!(
            issues[0].message().contains("`thur`"),
            "{}",
            issues[0].message()
        );
    }

    #[test]
    fn semantics_on_an_unrelated_slot_is_rejected() {
        let issues = validate_domain("emotion", &dom(&["neutral"], Some("neutral"), &["neutral"]));
        let codes: Vec<&str> = issues.iter().map(super::DomainIssue::code).collect();
        assert_eq!(
            codes,
            ["E-ENUM-UNEXPECTED-SEMANTICS", "E-ENUM-UNEXPECTED-SEMANTICS"]
        );
    }

    #[test]
    fn open_domain_is_exempt() {
        // A registry-style domain has no static members, so member-semantics
        // requirements cannot apply to it.
        let mut d = dom(&[], None, &[]);
        d.open = true;
        assert!(validate_domain("action", &d).is_empty());
    }

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
            semantics: semantics
                .iter()
                .map(std::string::ToString::to_string)
                .collect(),
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

    /// dsl 0.41.0 §5.5: a declared attr `default` is a value of its type —
    /// `int` refuses `1.5`; `double` takes a YAML integer.
    #[test]
    fn attr_default_must_inhabit_the_declared_type() {
        let with = |ty: Type, default: Literal| {
            let mut d = dir("d", &[]);
            d.attrs[0].ty = ty;
            d.attrs[0].default = Some(default);
            validate_directive(&d)
        };
        let errs = with(Type::Int, Literal::Double(1.5));
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert_eq!(errs[0].code(), "E-PLUGIN-INVALID-DIRECTIVE");
        assert_eq!(
            errs[0].message(),
            "directive `::d` declares attribute `x` as `int` with `default: 1.5`, which is \
             not of type `int` — give a default of the declared type"
        );
        assert!(!with(Type::Bool, Literal::Str("yes".into())).is_empty());
        assert!(with(Type::Double, Literal::Int(1)).is_empty());
        assert!(with(Type::Enum(vec!["a".into()]), Literal::Str("a".into())).is_empty());
        assert!(with(Type::ProviderRef("p".into()), Literal::Str("id".into())).is_empty());
    }

    /// dsl 0.27.0 §4: an `effects:` block parsed from `yaml` on a directive
    /// with one `item` attr.
    fn fact_dir(yaml: &str) -> DirectiveDecl {
        let mut d = dir("give", &[]);
        d.attrs[0].name = "item".into();
        d.effects = Some(yaml_serde::from_str(yaml).expect("fixture yaml"));
        d
    }

    #[test]
    fn fact_effects_reading_declared_attrs_pass() {
        let errs = validate_directive(&fact_dir(
            "{ asserts: [\"holding(@item)\"], retracts: [\"holding(_)\", \"seen(key, true)\"] }",
        ));
        assert!(errs.is_empty(), "{errs:?}");
    }

    #[test]
    fn fact_effect_reading_an_undeclared_attr_is_a_parse_error() {
        let errs = validate_directive(&fact_dir("{ asserts: [\"holding(@iten)\"] }"));
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert_eq!(errs[0].code(), "E-PLUGIN-PARSE");
        let msg = errs[0].message();
        assert!(msg.contains("effects.asserts"), "{msg}");
        assert!(msg.contains("did you mean `@item`?"), "{msg}");
    }

    #[test]
    fn asserted_wildcard_is_a_parse_error_but_a_retracted_one_is_not() {
        let errs = validate_directive(&fact_dir("{ asserts: [\"holding(_)\"] }"));
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert!(
            errs[0].message().contains("asserts `_`"),
            "{}",
            errs[0].message()
        );
        assert!(validate_directive(&fact_dir("{ retracts: [\"holding(_)\"] }")).is_empty());
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
            fields: yaml_serde::from_str(fields_yaml).expect("fixture yaml"),
        };
        d
    }

    #[test]
    fn valid_record_lowering_passes() {
        let errs = validate_directive(&record_dir(
            "bg",
            "{ assetId: { fromAttr: img }, time: dusk }",
        ));
        assert!(errs.is_empty(), "{errs:?}");
    }

    #[test]
    fn actor_character_must_bind_required_attr_or_literal() {
        let errs = validate_directive(&record_dir("actor", "{ exit: false }"));
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert_eq!(errs[0].code(), "E-LOWER-RECORD-FIELD");
        assert!(
            errs[0].message().contains("character"),
            "{}",
            errs[0].message()
        );
        assert!(
            errs[0].message().contains("required"),
            "{}",
            errs[0].message()
        );
    }

    #[test]
    fn actor_character_from_optional_attr_is_rejected() {
        let mut d = record_dir("actor", "{ character: { fromAttr: img } }");
        d.attrs[0].required = false;
        let errs = validate_directive(&d);
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert_eq!(errs[0].code(), "E-LOWER-RECORD-FIELD");
        assert!(
            errs[0].message().contains("required: true"),
            "{}",
            errs[0].message()
        );
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
             targets the staging kinds (bg, music, sfx, vfx, actor, camera, cg, video, sequence)"
        );
    }

    #[test]
    fn unknown_target_field_is_rejected() {
        let errs = validate_directive(&record_dir("bg", "{ backdropId: { fromAttr: img } }"));
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert_eq!(errs[0].code(), "E-LOWER-RECORD-FIELD");
        assert_eq!(
            errs[0].message(),
            "directive `::backdrop` lowers to record `bg`: unknown target field \
             `backdropId` (record `bg` binds: location, time, assetId)"
        );
    }

    #[test]
    fn from_attr_naming_an_undeclared_attr_is_rejected() {
        let errs = validate_directive(&record_dir("bg", "{ assetId: { fromAttr: nope } }"));
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert_eq!(errs[0].code(), "E-LOWER-RECORD-FIELD");
        assert_eq!(
            errs[0].message(),
            "directive `::backdrop` lowers to record `bg`: field `assetId` reads \
             `fromAttr: nope`, but directive `::backdrop` declares no such attr (declared: img)"
        );
    }

    #[test]
    fn literal_of_the_wrong_kind_is_rejected() {
        // `actor.exit` is a bool; a bare string cannot fill it.
        let errs = validate_directive(&record_dir(
            "actor",
            "{ character: { fromAttr: img }, exit: wide }",
        ));
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert_eq!(errs[0].code(), "E-LOWER-RECORD-FIELD");
        assert!(
            errs[0]
                .message()
                .ends_with("field `exit` expects bool or { fromAttr: <attr> }, got string"),
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
            assert!(
                lower_record_fields(k).is_some(),
                "`{k}` must be in the table"
            );
        }
    }
}
