//! The `<choice into=…>` run-record sugar (dsl 0.6.0 §2) and the
//! `E-PERSIST-REMOVED` / `E-AS-REMOVED` migration fixits.

use super::*;

/// Diagnostic codes for the `<choice … into="run.<path>">` run-record sugar
/// (dsl 0.6.0 §2). `into=` alone records; the `persist=` attribute was REMOVED
/// in 0.6.0 (`E-PERSIST-REMOVED`, §2.2), carrying a machine-applicable deletion.
const E_PERSIST_REMOVED: &str = "E-PERSIST-REMOVED";
/// `E-AS-REMOVED` (dsl 0.10.0 §4, D-L): the `as=` record-target attribute was
/// renamed to `into=` in `0.1.0`. Mirrors [`E_PERSIST_REMOVED`] in severity,
/// layer, span behaviour and kind — and NOT in its edit; see
/// [`as_removed_diag`].
const E_AS_REMOVED: &str = "E-AS-REMOVED";
const E_INTO_TARGET: &str = "E-INTO-TARGET";
const E_INTO_UNDECLARED: &str = "E-INTO-UNDECLARED";
const E_INTO_VALUE: &str = "E-INTO-VALUE";
const W_INTO_SET_DUP: &str = "W-INTO-SET-DUP";

/// Validate a `<choice>`'s run-record sugar (dsl 0.6.0 §2): `into="run.<path>"
/// [value="<lit>"]` records a NAMED, declared `run.*` fact when the choice is
/// selected. The sugar is EXACTLY a `::set{run.<path> = <value>}` appended to
/// the arm — the engine materializes the write, so the checker only validates
/// well-formedness. `into=` ALONE drives the record now: the two-attribute
/// `persist="run" into=…` idiom of ≤0.5.2 is gone, so a `<choice>` carrying any
/// `persist=` attr is `E-PERSIST-REMOVED` (§2.2), migrated by deleting it. A
/// `<choice>` with no `into=` records nothing and is untouched. The
/// `persist`/`into`/`value` attrs are recognized here, so they are never
/// reported as unknown/extra. (The record target attribute is `into`, renamed
/// from 0.0.1 `as`; `as` survives only on content lines as the display-label
/// override, §7.1 — untouched here.)
pub(super) fn check_choice_record(
    choice: &Choice,
    ctx: &Ctx<'_>,
    src: &str,
    diags: &mut Vec<Diagnostic>,
) {
    // §2.2: `persist=` was REMOVED in 0.6.0 — any `persist=` attr (whatever its
    // value) is an error carrying a machine-applicable deletion fixit; `into=`
    // alone now records. Recognizing it here (rather than falling through to an
    // unknown-attr report) keeps `E-PERSIST-REMOVED` the sole report for it.
    if let Some(persist) = choice.attrs.iter().find(|a| a.key == "persist") {
        diags.push(persist_removed_diag(persist, src));
    }
    // §4 (D-L): `as=` was RENAMED to `into=` in 0.1.0, and `lute fix` already
    // performs the rename. Recognizing it here — beside `persist`, for the same
    // reason and by the same mechanism — keeps `E-AS-REMOVED` the sole report
    // for it and keeps the §4 closure rule out of it (`logic_attrs.rs`'s
    // `CHOICE_REMOVED_ATTRS`).
    if let Some(as_attr) = choice.attrs.iter().find(|a| a.key == "as") {
        diags.push(as_removed_diag(as_attr));
    }
    // The record sugar is driven by `into=` alone (§2.1). A `<choice>` with no
    // `into=` records nothing and is untouched.
    let Some(into_attr) = choice.attrs.iter().find(|a| a.key == "into") else {
        return;
    };
    let Some(into_path) = str_attr(into_attr) else {
        diags.push(into_diag(
            E_INTO_TARGET,
            "`into` must be a `run.<path>` string literal (dsl 0.6.0 §2.2)".to_string(),
            choice.span,
            Severity::Error,
        ));
        return;
    };
    // §2.2: cross-episode records live in the `run.*` namespace.
    if into_path
        .strip_prefix("run.")
        .filter(|rest| !rest.is_empty())
        .is_none()
    {
        diags.push(into_diag(
            E_INTO_TARGET,
            format!(
                "`into=\"{into_path}\"` must name a `run.<path>` fact (a bare `run` or a \
                 non-`run.*` path is not a valid run target) (dsl 0.6.0 §2.2)"
            ),
            choice.span,
            Severity::Error,
        ));
        return;
    }
    // §2.2: the `into` path MUST already be declared in the run schema — a
    // typo'd/undeclared `into` cannot silently create a field.
    let Some(ty) = resolve_type(into_path, &ctx.env.state) else {
        diags.push(into_diag(
            E_INTO_UNDECLARED,
            format!(
                "`into=\"{into_path}\"` is not declared in the run schema (dsl 0.6.0 §2.2); \
                 an undeclared/typo'd `into` cannot create a field"
            ),
            choice.span,
            Severity::Error,
        ));
        return;
    };
    // §2.2 rule 4: the `value` policy depends on the declared type — a
    // `{ domain: K }` path's is K's members (dsl 0.27.0 §2).
    let resolved = ctx
        .env
        .state
        .domain_members
        .get(into_path)
        .map(|(_, ms)| Type::Enum(ms.clone()));
    check_into_value(
        resolved.as_ref().unwrap_or(ty),
        choice.attrs.iter().find(|a| a.key == "value"),
        into_path,
        choice.span,
        diags,
    );
    // §2.2: the arm must not already `::set` the same path — the record write
    // would duplicate it.
    if choice
        .body
        .iter()
        .any(|n| matches!(n, Node::Set(s) if s.path == into_path))
    {
        diags.push(into_diag(
            W_INTO_SET_DUP,
            format!(
                "the choice arm already `::set`s `{into_path}`, which `into=\"{into_path}\"` \
                 also records (dsl 0.6.0 §2.2)"
            ),
            choice.span,
            Severity::Warning,
        ));
    }
}

/// Rule 4 of the record sugar (dsl 0.6.0 §2.2): a `bool` path's `value` is
/// OPTIONAL (defaults to `true`) and, when present, MUST be a bool literal;
/// every other path (`number`/`enum`/…) REQUIRES a type-compatible literal.
fn check_into_value(
    ty: &Type,
    value: Option<&Attr>,
    into_path: &str,
    span: Span,
    diags: &mut Vec<Diagnostic>,
) {
    let accepted = |v: &Attr| into_literal(ty, &v.value).is_some_and(|lit| type_accepts(ty, &lit));
    match ty {
        Type::Bool => {
            if let Some(v) = value {
                if !accepted(v) {
                    diags.push(into_diag(
                        E_INTO_VALUE,
                        format!(
                            "`value` for the bool path `{into_path}` must be a bool literal \
                             (`true`/`false`) (dsl 0.6.0 §2.2)"
                        ),
                        span,
                        Severity::Error,
                    ));
                }
            }
        }
        _ => match value {
            None => diags.push(into_diag(
                E_INTO_VALUE,
                format!(
                    "`value` is required for `{into_path}` (only a `bool` path defaults to \
                     `true`) (dsl 0.6.0 §2.2)"
                ),
                span,
                Severity::Error,
            )),
            Some(v) if !accepted(v) => diags.push(into_diag(
                E_INTO_VALUE,
                into_value_mismatch(ty, v, into_path),
                v.value_span,
                Severity::Error,
            )),
            Some(_) => {}
        },
    }
}

/// The `E-INTO-VALUE` message for a `value` the declared type refuses
/// (OT-F-11): it quotes the value and names what the path takes — an enum's
/// members with a did-you-mean, or the scalar type.
fn into_value_mismatch(ty: &Type, v: &Attr, into_path: &str) -> String {
    let shown = match &v.value {
        AttrValue::Str(s) => format!("`value=\"{s}\"`"),
        AttrValue::BoolTrue => "a bare `value`".to_string(),
        AttrValue::Ref(r) => format!("`value={}`", r.raw),
    };
    let takes = match ty {
        Type::Enum(members) => {
            let hint = str_attr(v)
                .and_then(|s| {
                    lute_manifest::suggest::nearest(s, members.iter().map(String::as_str), 2)
                })
                .map_or_else(String::new, |near| format!(" — did you mean `{near}`?"));
            format!("one of its members ({}){hint}", members.join(", "))
        }
        Type::Int => "an integer literal".to_string(),
        Type::Double => "a double literal".to_string(),
        Type::Str => "a string literal".to_string(),
        _ => "a literal of its declared type".to_string(),
    };
    format!("{shown} cannot be recorded into `{into_path}`, which takes {takes} (dsl 0.6.0 §2.2)")
}

/// The string value of an attr, when it is a plain string literal (`key="s"`).
/// A bare (`BoolTrue`) or `@ref`-valued attr yields `None`.
fn str_attr(attr: &Attr) -> Option<&str> {
    match &attr.value {
        AttrValue::Str(s) => Some(s),
        _ => None,
    }
}

/// Coerce an `into` record `value` attr into a manifest [`Literal`] *in the
/// resolved target type's domain* so [`type_accepts`] can judge it — mirroring
/// the directive attr coercion (`directives::literal_of`). An `int` target
/// parses the string as `i64`; a `double` target parses it as `f64`; a `bool`
/// target accepts the bare `value` ident or the strings `"true"`/`"false"`;
/// every other target (`enum`/`str`/…) keeps the value VERBATIM as
/// [`Literal::Str`], so an enum member spelled like a bool or number (`"true"`,
/// `"3"`) still resolves by string membership. Returns `None` when the value
/// cannot inhabit the target's shape (a hard type error) or is a `@ref`.
pub(super) fn into_literal(ty: &Type, v: &AttrValue) -> Option<Literal> {
    match (ty, v) {
        (Type::Int, AttrValue::Str(s)) => s.parse::<i64>().ok().map(Literal::Int),
        (Type::Double, AttrValue::Str(s)) => s.parse::<f64>().ok().map(Literal::Double),
        (Type::Bool, AttrValue::BoolTrue) => Some(Literal::Bool(true)),
        (Type::Bool, AttrValue::Str(s)) => match s.as_str() {
            "true" => Some(Literal::Bool(true)),
            "false" => Some(Literal::Bool(false)),
            _ => None,
        },
        (_, AttrValue::Str(s)) => Some(Literal::Str(s.clone())),
        // A bare-ident `value` against a non-bool target is a type error.
        (_, AttrValue::BoolTrue) => None,
        (_, AttrValue::Ref(_)) => None,
    }
}

/// Build a `Layer::Logic` diagnostic for the choice record sugar (a dsl 0.6.0
/// §2 branch check); `severity` is `Error` for the `E-INTO-*` gates and
/// `Warning` for `W-INTO-SET-DUP`.
fn into_diag(code: &str, message: String, span: Span, severity: Severity) -> Diagnostic {
    Diagnostic {
        code: code.to_string(),
        severity,
        message,
        evidence: None,
        span,
        layer: Layer::Logic,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    }
}

/// Build the `E-PERSIST-REMOVED` diagnostic (dsl 0.6.0 §2.2) for a `<choice>`
/// carrying a `persist=` attr — the attribute was REMOVED in 0.6.0; `into=`
/// alone records now. ONE `"migrate"` fixit deletes the whole attr plus one
/// adjacent separating space (dsl §4.5 attrs are whitespace-separated, so no
/// stray double space is left behind), `confidence` 100. `kind: "migrate"` so
/// `lute fix` applies it unprompted in the shape it can prove (§2.3 — the
/// deletion is meaning-preserving in BOTH directions for a `persist="run"` +
/// `into=` pair). Mirrors `fix.rs`'s `widen_removed_attr` (trailing separator
/// preferred) so the LSP fixit and `lute fix` yield byte-identical output.
/// `src` is the raw document text the checker is validating.
fn persist_removed_diag(persist_attr: &Attr, src: &str) -> Diagnostic {
    let (start, end) = widen_attr_delete(
        src.as_bytes(),
        persist_attr.span.byte_start,
        persist_attr.span.byte_end,
    );
    Diagnostic {
        code: E_PERSIST_REMOVED.to_string(),
        severity: Severity::Error,
        message: "the `persist` attribute was removed in 0.6.0 — `into=` alone now records \
                  the run fact (dsl 0.6.0 §2.2)"
            .to_string(),
        evidence: None,
        span: persist_attr.span,
        layer: Layer::Logic,
        fixits: vec![Fixit {
            title: "remove persist=".to_string(),
            kind: "migrate".to_string(),
            edit: vec![TextEdit {
                span: zeroed_span(start, end),
                new_text: String::new(),
            }],
            confidence: 100,
        }],
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    }
}

/// Build the `E-AS-REMOVED` diagnostic (dsl 0.10.0 §4, D-L) for a `<choice>`
/// carrying an `as=` attr — renamed to `into=` in 0.1.0. ONE `"migrate"` fixit,
/// `confidence` 100.
///
/// **The edit is deliberately NOT [`persist_removed_diag`]'s.** That one is a
/// DELETION widened by [`widen_attr_delete`] to swallow one adjacent separating
/// space so no stray double space is left behind. This is a two-byte-to-
/// four-byte KEY REPLACEMENT with no widening at all: `Attr::span` starts at the
/// key's first byte (`scan_attrs` builds it as `span(key_start, …)`), so the key
/// occupies `[byte_start, byte_start + key.len())` and the value is untouched.
/// Reusing the deletion helper would eat the separator before `as` and emit
/// `label="L"into="…"`. It therefore takes no `src`, because it needs to read
/// none.
///
/// Mirrors `fix.rs:134-135`'s own `as` -> `into` edit exactly. `lute fix` never
/// reads `Diagnostic.fixits` (`lute-lsp/src/code_action.rs:7`), so these are two
/// independent implementations that MUST stay byte-identical — the same hazard
/// [`persist_removed_diag`]'s doc records, and
/// `tests/choice_persist.rs::lsp_fixit_and_lute_fix_agree_byte_for_byte` is what
/// holds them together.
fn as_removed_diag(as_attr: &Attr) -> Diagnostic {
    let start = as_attr.span.byte_start;
    let end = start + as_attr.key.len();
    Diagnostic {
        code: E_AS_REMOVED.to_string(),
        severity: Severity::Error,
        message: "the `as` attribute was renamed to `into` in 0.1.0 — `into=` names the run \
                  fact this choice records (dsl 0.10.0 §4); `lute fix` performs the rename"
            .to_string(),
        evidence: None,
        span: as_attr.span,
        layer: Layer::Logic,
        fixits: vec![Fixit {
            title: "rename as= to into=".to_string(),
            kind: "migrate".to_string(),
            edit: vec![TextEdit {
                span: zeroed_span(start, end),
                new_text: "into".to_string(),
            }],
            confidence: 100,
        }],
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    }
}

/// Widen a to-be-deleted attr's `[start, end)` byte span to also swallow ONE
/// adjacent whitespace separator (dsl §4.5): deleting a first/middle attr among
/// siblings leaves no stray double space, and deleting the last leaves no stray
/// space before `>`/`}`. Prefers the TRAILING separator, falling back to the
/// LEADING one — identical to `fix.rs`'s `widen_removed_attr` so the LSP fixit
/// and `lute fix` agree byte-for-byte.
fn widen_attr_delete(bytes: &[u8], start: usize, end: usize) -> (usize, usize) {
    let mut new_end = end;
    while new_end < bytes.len() && matches!(bytes[new_end], b' ' | b'\t') {
        new_end += 1;
    }
    if new_end > end {
        return (start, new_end);
    }
    let mut new_start = start;
    while new_start > 0 && matches!(bytes[new_start - 1], b' ' | b'\t') {
        new_start -= 1;
    }
    (new_start, end)
}

/// A byte-range `Span` with `line`/`column`/`utf16_range` left zeroed — the
/// house zero-then-normalize convention every other ad hoc span producer in
/// this module follows (`cel_resolve.rs`, `datalog_check.rs`, `meta.rs`);
/// `normalize_spans` below recomputes real positions for it (diagnostic AND
/// fixit-edit spans alike) from its bytes through one shared `TextIndex`.
fn zeroed_span(byte_start: usize, byte_end: usize) -> Span {
    Span {
        byte_start,
        byte_end,
        line: 0,
        column: 0,
        utf16_range: (0, 0),
    }
}

#[cfg(test)]
mod persist_fixit_tests {
    use super::*;

    fn mkspan(byte_start: usize, byte_end: usize) -> Span {
        Span {
            byte_start,
            byte_end,
            line: 0,
            column: 0,
            utf16_range: (0, 0),
        }
    }

    fn persist_attr(src: &str) -> Attr {
        let p_start = src.find("persist").unwrap();
        let p_end = p_start + "persist=\"run\"".len();
        Attr {
            key: "persist".to_string(),
            value: AttrValue::Str("run".to_string()),
            value_span: mkspan(p_start + 9, p_end - 1),
            span: mkspan(p_start, p_end),
        }
    }

    /// `E-PERSIST-REMOVED`'s `"migrate"` fixit deletes the whole `persist=` attr
    /// plus its ONE trailing whitespace separator (dsl §4.5) when the attr sits
    /// among siblings — no stray double space left behind, and the code/kind/
    /// confidence match the §2.3 auto-apply contract.
    #[test]
    fn persist_removed_fixit_deletes_attr_and_trailing_space() {
        let src = "<choice id=\"a\" persist=\"run\" into=\"run.x\">";
        let diag = persist_removed_diag(&persist_attr(src), src);
        assert_eq!(diag.code, E_PERSIST_REMOVED);
        assert_eq!(diag.severity, Severity::Error);
        let fx = &diag.fixits[0];
        assert_eq!(
            fx.kind, "migrate",
            "lute fix auto-applies only `migrate` fixits"
        );
        assert_eq!(fx.confidence, 100);
        let edit = &fx.edit[0];
        let mut spliced = src.to_string();
        spliced.replace_range(edit.span.byte_start..edit.span.byte_end, &edit.new_text);
        assert_eq!(
            spliced, "<choice id=\"a\" into=\"run.x\">",
            "got {spliced:?}"
        );
    }

    /// When `persist=` is the LAST attr (no trailing whitespace to take), the
    /// fixit falls back to the LEADING separator so no stray space is left
    /// before `>`.
    #[test]
    fn persist_removed_fixit_falls_back_to_leading_space_when_last() {
        let src = "<choice id=\"a\" persist=\"run\">";
        let diag = persist_removed_diag(&persist_attr(src), src);
        let edit = &diag.fixits[0].edit[0];
        let mut spliced = src.to_string();
        spliced.replace_range(edit.span.byte_start..edit.span.byte_end, &edit.new_text);
        assert_eq!(spliced, "<choice id=\"a\">", "got {spliced:?}");
    }
}
