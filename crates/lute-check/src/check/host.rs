//! 0.41.0 §3.6: a host condition (`lute expr`) — a CEL condition a host
//! evaluates over a run state through the runtime's `evaluate` query, checked
//! here so the host lowers it at build time (the runtime stays parser-free).

use super::*;

/// A checked host condition.
#[derive(Clone, Debug)]
pub struct HostCondition {
    /// The condition's diagnostics, spans in the condition text itself
    /// (line 1, columns from 1), in the checker's order.
    pub diagnostics: Vec<Diagnostic>,
    /// The `@def`-expanded condition — `None` when an error was found.
    pub cel: Option<String>,
}

/// Check `raw` as the `when:` of a scene document at the project root.
///
/// `input` is that document: a frontmatter declaring only `uses:` of every
/// schema file the project's documents import, resolved against the
/// project's plugin set. `defs` replaces the document's own def table — the
/// project index's defs, parameterised defs included, each body already
/// `@def`-expanded with its parameters as bare identifiers. The condition's
/// expected type is `bool`. On success the condition is expanded through
/// `defs` exactly as a compiled seam condition is.
pub fn check_host_condition(input: &CheckInput, defs: &DefTable<'_>, raw: &str) -> HostCondition {
    let (doc, _) = parse(&input.text);
    // The synthetic frontmatter's own diagnostics (two schemas declaring one
    // name, …) belong to the documents that import those files.
    let (mut folded, _, _) = fold_env(&doc, input);
    folded.env.defs = defs.bodies.keys().cloned().collect();
    folded.env.def_params = defs.params.clone();
    folded.env.def_types.retain(|name, _| defs.bodies.contains_key(name));
    folded.def_bodies = defs.bodies.clone();
    let scope = crate::defassign::Scope::of(&folded);
    let ctx = Ctx {
        env: &folded.env,
        in_match: false,
        match_subject: None,
    };

    let idx = TextIndex::new(raw);
    let span = Span::from_bytes(&idx, 0, raw.len());
    let mut arena = CelArena::default();
    let mut slot = CelSlot::raw(CelKind::Condition, raw.to_string(), span);
    let mut diags = match parse_slot(&mut arena, raw, 0) {
        Ok(handle) => {
            slot.ast = Some(handle);
            let mut diags = check_beat_when(&slot, &arena, &ctx, &scope);
            diags.extend(crate::gates::condition_literals(
                raw,
                &scope.defs,
                scope.schema,
                Some(raw),
                span,
            ));
            diags
        }
        Err(err) => {
            let t = translate_cel_parse(raw, span, &err, CelKind::Condition);
            vec![Diagnostic {
                code: t.code.to_string(),
                severity: Severity::Error,
                message: t.message,
                evidence: None,
                span: t.span.unwrap_or(span),
                layer: Layer::Cel,
                fixits: t.fixits,
                provenance: None,
                covered: Vec::new(),
                related: Vec::new(),
            }]
        }
    };
    let failed = diags.iter().any(|d| d.severity == Severity::Error);
    let cel = if failed {
        None
    } else {
        match crate::cel_expand::expand_cel(raw, defs, None, &mut Vec::new()) {
            Ok(cel) => Some(cel),
            Err(error) => {
                diags.push(Diagnostic {
                    code: "E-COMPILE-EXPAND".to_string(),
                    severity: Severity::Error,
                    message: error.to_string(),
                    evidence: None,
                    span,
                    layer: Layer::Cel,
                    fixits: Vec::new(),
                    provenance: None,
                    covered: Vec::new(),
                    related: Vec::new(),
                });
                None
            }
        }
    };
    // The checker's own final passes, as `check()` runs them.
    let mut diags = dedup_undeclared(dedup_rehomed(diags));
    normalize_spans(&idx, raw, &mut diags);
    order_diagnostics(&mut diags);
    let mut diags = collapse_same_root(diags);
    crate::evidence::annotate(&mut diags);
    HostCondition {
        diagnostics: diags,
        cel,
    }
}
