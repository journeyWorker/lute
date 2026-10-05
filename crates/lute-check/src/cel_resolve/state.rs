use super::*;
/// as an exempt `@ref`. Its presence is therefore itself out of profile (§8.4).
pub(crate) fn raw_uses_reserved_marker(raw: &str) -> bool {
    let marker = lute_cel::REF_MARKER.as_bytes();
    if raw.len() < marker.len() {
        return false;
    }
    let mask = lute_cel::cel_string_mask(raw);
    raw.as_bytes()
        .windows(marker.len())
        .enumerate()
        .any(|(i, w)| w == marker && !mask[i])
}

/// Conservative type-compatibility for `E-REF-TYPE` (dsl §8): return `true` (no
/// flag) for everything not PROVABLY incompatible.
pub(crate) fn compatible(produced: &Type, expected: &ExpectedType) -> bool {
    match expected {
        ExpectedType::Bool => matches!(produced, Type::Bool),
        ExpectedType::Ty(t) => {
            if is_id_type(produced) || is_id_type(t) {
                return true; // id types: always compatible (never flag)
            }
            if is_string_family(produced) && is_string_family(t) {
                return true; // {Str, Enum, EnumFromOption} mutually compatible
            }
            produced == t // structural equality (Type: PartialEq)
        }
    }
}

/// Namespaced/provider id types — value-level strings whose membership validity
/// is a separate concern; always treated as compatible.
pub(crate) fn is_id_type(t: &Type) -> bool {
    matches!(
        t,
        Type::ProviderRef(_)
            | Type::Domain(_)
            | Type::Entity(_)
            | Type::SlotId { .. }
            | Type::AssetKind(_)
    )
}

/// The mutually-compatible string family: an enum value is a string at the
/// value level, and def CEL produces string-ish values.
pub(crate) fn is_string_family(t: &Type) -> bool {
    matches!(t, Type::Str | Type::Enum(_) | Type::EnumFromOption(_))
}

/// Short human label for a produced [`Type`] in an `E-REF-TYPE` message.
pub(crate) fn ty_desc(t: &Type) -> String {
    match t {
        Type::Bool => "a bool".to_string(),
        Type::Int => "an int".to_string(),
        Type::Double => "a double".to_string(),
        Type::Str => "a string".to_string(),
        Type::Enum(_) | Type::EnumFromOption(_) => "an enum".to_string(),
        Type::List(_) => "a list".to_string(),
        Type::Record(_) => "a record".to_string(),
        Type::Map { .. } => "a map".to_string(),
        Type::ProviderRef(_) => "a provider ref".to_string(),
        Type::Domain(_) => "a domain ref".to_string(),
        Type::Entity(_) => "an entity id".to_string(),
        Type::SlotId { .. } => "a slot id".to_string(),
        Type::AssetKind(_) => "an asset kind".to_string(),
        Type::NarrativeTime => "a narrative-time value".to_string(),
    }
}

/// Short human label for an [`ExpectedType`] in an `E-REF-TYPE` message.
pub(crate) fn expected_desc(e: &ExpectedType) -> String {
    match e {
        ExpectedType::Bool => "a bool".to_string(),
        ExpectedType::Ty(t) => ty_desc(t),
    }
}

/// Best-effort static type of a single call argument's raw source (dsl §8.1
/// `@name(args)`). Returns `None` (skip — never flag) for anything not trivially
/// typeable, keeping `E-REF-ARG-TYPE` conservative (no false positives).
pub(crate) fn resolve_arg_type(arg_raw: &str, ctx: &Ctx<'_>) -> Option<Type> {
    let a = arg_raw.trim();
    if a == "true" || a == "false" {
        return Some(Type::Bool);
    }
    if a.parse::<i64>().is_ok() {
        return Some(Type::Int);
    }
    if a.parse::<f64>().is_ok() {
        return Some(Type::Double);
    }
    if (a.starts_with('\'') && a.ends_with('\'') && a.len() >= 2)
        || (a.starts_with('"') && a.ends_with('"') && a.len() >= 2)
    {
        return Some(Type::Str);
    }
    if let Some(name) = a.strip_prefix('@') {
        // a nested bare `@ref` (no call) -> its produced type
        if name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
        {
            return ctx.env.def_types.get(name).cloned();
        }
    }
    // a bare, resolvable state path
    crate::set_op::resolve_type(a, &ctx.env.state).cloned()
}

/// Classify one reconstructed state path and emit its diagnostic, if any.
pub(crate) fn check_state_path(path: &str, slot: &CelSlot, ctx: &Ctx<'_>, diags: &mut Vec<Diagnostic>) {
    // Reserved `run.choiceLog.*` read inside a guard/condition (dsl §9.6).
    if is_guard(slot.kind) && (path == "run.choiceLog" || path.starts_with("run.choiceLog.")) {
        diags.push(diag(
            "E-CHOICELOG-READ",
            format!("`{path}` is reserved and cannot be read in a guard/condition (dsl §9.6)"),
            slot.span,
        ));
        return;
    }
    // A namespace root is a map in the membership form (`'k' in run`).
    // It is not itself a scalar state declaration, so do not report the
    // synthetic root as undeclared.
    if crate::cel_paths::STATE_ROOTS.iter().any(|root| *root == path)
        && slot.raw.contains(&format!("in {path}"))
    {
        return;
    }
    // dsl 0.24.0 §3/§4: `run.approval[@who]` in a component body reads the
    // member a `::use` binds (`component_effects::bind_slot_raw`), checked
    // there; a `@name` that is no param is `E-UNDECLARED-REF`'s.
    let param_indexed = ctx.env.rel_vocab.indexed_state.contains_key(path)
        && slot.raw.match_indices(&format!("{path}[@")).any(|(at, m)| {
            let rest = &slot.raw[at + m.len()..];
            let name_len = rest
                .bytes()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == b'_')
                .count();
            name_len > 0
                && rest.as_bytes().get(name_len) == Some(&b']')
                && !ctx.env.defs.contains(&rest[..name_len])
        });
    // T3-1: a path whose own declaration was reported is judged no further.
    if !param_indexed && !is_declared(path, ctx) && !ctx.env.state.is_faulty(path) {
        let mut msg = format!("state path `{path}` is not declared in `state:` (dsl §9.4)");
        // dsl 0.5.0 §2.2 "did you mean": suggest the nearest declared path
        // within a small edit distance, advisory text only (no new code).
        if let Some(kind) = ctx.env.rel_vocab.indexed_state.get(path) {
            // dsl 0.24.0 §3: the family itself is not a value.
            msg = format!(
                "state path `{path}` is entity-indexed (`per: {kind}`), so read one member: \
                 `{path}.<member>` by name, `{path}[occasion.target]` for the beat's target, \
                 `{path}[@param]` in a component or a beat template header, or `{path}[X]` \
                 with a rule variable inside a rule `cel()` guard"
            );
        } else if let Some(field) = path
            .strip_prefix(crate::occasion_bind::OCCASION_PAYLOAD)
            .and_then(|f| f.strip_prefix('.'))
        {
            // dsl 0.27.0 §3, 0.28.0 (T2-8): payload fields come from the
            // occasion being raised.
            msg = format!(
                "`{path}` is not a `payload:` field of any occasion this document answers — \
                 `occasion.payload` is bound only while its occasion is raised, and a beat, an \
                 `<objective on>` or an `<on event>` handler reads the payload of the occasion it \
                 answers, so declare `{field}` under that occasion's `payload:` (dsl 0.27.0 §3)"
            );
            if let Some(sugg) = crate::cel_paths::nearest_declared_path(path, &ctx.env.state, 2)
                .filter(|s| s.starts_with(crate::occasion_bind::OCCASION_PAYLOAD))
            {
                msg.push_str(&format!(" — did you mean `{sugg}`?"));
            }
        } else {
            // dsl 0.28.0 (T3-8): an engine namespace says what it holds —
            // unless an authored `scene.*` path is the nearer reading.
            let near = crate::cel_paths::nearest_declared_path(path, &ctx.env.state, 2);
            let engine = near
                .filter(|_| path.starts_with("scene."))
                .is_none()
                .then(|| crate::cel_paths::engine_path_hint(path, &|p| is_declared(p, ctx)))
                .flatten();
            if let Some(hint) = engine {
                msg = hint;
            } else if let Some(sugg) = near {
                msg.push_str(&format!(" — did you mean `{sugg}`?"));
            }
        }
        diags.push(diag("E-UNDECLARED", msg, slot.span));
    }
}

/// A path is declared when it exactly matches a `state:` key or is a descendant
/// field of one (`scene.player` declared => `scene.player.hp` reads are ok), OR
/// is a RESERVED `quest.<id>.state`/`quest.<id>.objectives.<oid>.done` path
/// (dsl 0.2.0 §5.2): those are implicitly declared UNCONDITIONALLY, regardless
/// of whether THIS document folds the owning `<quest>` (mirrors
/// `set_op.rs::classify_write`, which shape-checks a reserved write target
/// before consulting the schema at all) — a scene that merely references
/// another document's quest instance is not "undeclared". The reserved
/// lore flag `entry.<id>.read` (dsl 0.19.0 §5) is admitted by the same
/// shape rule, wherever its `<entry>` lives.
pub(crate) fn is_declared(path: &str, ctx: &Ctx<'_>) -> bool {
    is_reserved_quest_path(path)
        || is_reserved_entry_read(path)
        || ctx
            .env
            .state
            .decls
            .keys()
            .any(|k| path == k || path.starts_with(&format!("{k}.")))
}

/// Guard/condition slots (dsl §9.6): a `<match>` subject or any boolean guard.
pub(crate) fn is_guard(kind: CelKind) -> bool {
    matches!(kind, CelKind::Condition | CelKind::MatchSubject)
}

/// Map a `scan_refs` byte span (relative to `slot.raw`) into the document by
/// offsetting with the slot's start byte. Line/column/utf16 stay zeroed: the
/// caller's `TextIndex` recomputes them at report time (matching `scan_refs`).
pub(crate) fn map_span(slot: &CelSlot, local: Span) -> Span {
    let base = slot.span.byte_start;
    Span {
        byte_start: base + local.byte_start,
        byte_end: base + local.byte_end,
        line: 0,
        column: 0,
        utf16_range: (0, 0),
    }
}

/// Build a `Layer::Cel` error diagnostic.
pub(crate) fn diag(code: &str, message: String, span: Span) -> Diagnostic {
    Diagnostic {
        code: code.to_string(),
        severity: Severity::Error,
        message,
        evidence: None,
        span,
        layer: Layer::Cel,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    }
}

