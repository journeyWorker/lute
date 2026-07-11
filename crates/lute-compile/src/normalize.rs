//! D8: AST normalization BEFORE lowering — (a) `::use` → the component body
//! inlined as real `Node`s with each `@param` bound (recursive; acyclic per
//! the checker's E-COMPONENT-CYCLE); (b) `<choice persist="run" …>` → a
//! synthesized trailing `::set` node (dsl §11.1.1: the sugar IS exactly a
//! `::set{run.<path> = <value>}` appended to the arm).
//!
//! Component-sourced regions are wrapped in `__component-begin`/`-end`
//! sentinel directives (reserved `__` prefix — the parser can never produce
//! them from source). The stage walker (Task 8) consumes them into
//! `source { component }` stamps; they emit no records.

use std::collections::BTreeMap;

use lute_check::meta::StateSchema;
use lute_check::{
    decide_slot, is_pattern_literals, ComponentSet, DecideCtx, Decided, DefTable, DollarBinding,
};
use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_manifest::types::Type;
use lute_syntax::ast::{
    Arm, Attr, AttrValue, CelKind, CelSlot, Choice, ClipNode, Directive, Document, Line, Match,
    Node, Set,
};

pub const COMPONENT_BEGIN: &str = "__component-begin";
pub const COMPONENT_END: &str = "__component-end";

/// Normalize the tree in place: no `::use` survives; persists are real `Set`s.
/// Total; failures (gate-proven unreachable) degrade to `E-COMPILE-COMPONENT`.
pub fn normalize_document(
    doc: &mut Document,
    components: &ComponentSet,
    schema: &StateSchema,
) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for shot in &mut doc.shots {
        normalize_nodes(&mut shot.body, components, schema, &mut diags);
    }
    for quest in &mut doc.quests {
        normalize_nodes(&mut quest.body, components, schema, &mut diags);
    }
    diags
}

fn normalize_nodes(
    nodes: &mut Vec<Node>,
    components: &ComponentSet,
    schema: &StateSchema,
    diags: &mut Vec<Diagnostic>,
) {
    let mut i = 0;
    while i < nodes.len() {
        let is_use = matches!(&nodes[i], Node::Directive(d) if d.tag == "use");
        if is_use {
            let d = match nodes.remove(i) {
                Node::Directive(d) => d,
                other => {
                    // Structurally impossible (guarded above); stay total.
                    nodes.insert(i, other);
                    i += 1;
                    continue;
                }
            };
            let spliced = expand_use(&d, components, schema, diags);
            let n = spliced.len();
            nodes.splice(i..i, spliced);
            i += n; // bodies were normalized recursively — skip past them
            continue;
        }
        // §7.2/§7.4 (D8): a gated content line desugars to a one-arm
        // `<match>` BEFORE expand/stage/address — same identity-preserving
        // idiom as the `is_use` splice above (remove, rebuild, reinsert at
        // the SAME index so the outer loop's `i += 1` below still lands
        // past it). Recursion into `Node::Branch`/`Node::Hub`/`Node::Match`/
        // `Node::On`/`Node::Objective` bodies below already re-enters this
        // function, so a gated line nested in any of those is caught on
        // that recursive call — no extra wiring needed here.
        let is_gated_line = matches!(&nodes[i], Node::Line(l) if l.when.is_some());
        if is_gated_line {
            let line = match nodes.remove(i) {
                Node::Line(l) => l,
                other => {
                    // Structurally impossible (guarded above); stay total.
                    nodes.insert(i, other);
                    i += 1;
                    continue;
                }
            };
            nodes.insert(i, synth_when_match(line));
            i += 1;
            continue;
        }
        match &mut nodes[i] {
            Node::Branch(b) => {
                for c in &mut b.choices {
                    synth_persist(c, schema);
                    normalize_nodes(&mut c.body, components, schema, diags);
                }
            }
            Node::Hub(h) => {
                for c in &mut h.choices {
                    synth_persist(c, schema);
                    normalize_nodes(&mut c.body, components, schema, diags);
                }
            }
            Node::Match(m) => {
                for arm in &mut m.arms {
                    match arm {
                        Arm::When { body, .. } | Arm::Otherwise { body, .. } => {
                            normalize_nodes(body, components, schema, diags)
                        }
                    }
                }
            }
            Node::Timeline(t) => {
                // A `ClipNode` is `Directive|Set` (§13/AST), so a `::use` inside a
                // timeline clip cannot be inline-expanded. Fail loud here (before the
                // stage walk) rather than let it reach lowering and be dropped
                // silently (spec-gap note 9). Requires `use lute_syntax::ast::ClipNode;`.
                for track in &t.tracks {
                    for clip in &track.clips {
                        if let ClipNode::Directive(cd) = &clip.node {
                            if cd.tag == "use" {
                                diags.push(Diagnostic {
                                    code: "E-COMPILE-COMPONENT".to_string(),
                                    severity: Severity::Error,
                                    message: "`::use` is not allowed inside a <timeline> clip"
                                        .to_string(),
                                    span: cd.span,
                                    layer: Layer::Content,
                                    fixits: Vec::new(),
                                    provenance: None,
                                });
                            }
                        }
                    }
                }
            }
            Node::On(on) => normalize_nodes(&mut on.body, components, schema, diags),
            Node::Objective(o) => normalize_nodes(&mut o.body, components, schema, diags),
            _ => {}
        }
        i += 1;
    }
}

/// §7.2/§7.4 (D8): `Node::Line{when: Some(g), ..}` → `Node::Match{ subject:
/// g, arms: [When{is: None, test: synthesized "$" Condition slot, body:
/// [the line, when=None]}, Otherwise{body: []}] }` — the guard `g` is
/// HOISTED to be the match subject verbatim (not re-typed to
/// `CelKind::MatchSubject`; downstream lowering (`stage::walk_match`,
/// `expr::synth_arm_expr`) reads only `.raw`, so the slot's `kind` never
/// reaches the artifact) and the arm's `test` is the literal text `"$"` —
/// the arm fires iff the guard itself decides true, exactly
/// `<match on="G"><when test="$">…</when><otherwise/></match>` (§7.4's
/// "MUST lower to that same match record", pinned by
/// `when_sugar::sugared_line_lowers_to_canonical_match_record`). The
/// `<otherwise>` alternative is the sugar's implicit empty else-case
/// (§7.2) — already a legal zero-body arm, no new IR shape. `line.when` is
/// cleared on the nested copy so a re-normalized desugared line can never
/// re-enter this rewrite (idempotent by construction).
fn synth_when_match(mut line: Line) -> Node {
    let guard = line
        .when
        .take()
        .expect("caller guarantees `line.when.is_some()`");
    let span = line.span;
    let test = CelSlot::raw(CelKind::Condition, "$".to_string(), span);
    Node::Match(Match {
        subject: guard,
        arms: vec![
            Arm::When {
                is: None,
                test,
                body: vec![Node::Line(line)],
                span,
            },
            Arm::Otherwise {
                body: Vec::new(),
                span,
            },
        ],
        span,
    })
}

/// `::use{component="name" <arg>=…}` → `[begin, …bound body…, end]`.
fn expand_use(
    d: &Directive,
    components: &ComponentSet,
    schema: &StateSchema,
    diags: &mut Vec<Diagnostic>,
) -> Vec<Node> {
    let name = d
        .attrs
        .iter()
        .find(|a| a.key == "component")
        .and_then(|a| match &a.value {
            AttrValue::Str(s) => Some(s.clone()),
            _ => None,
        });
    let Some(def) = name.as_deref().and_then(|n| components.table.get(n)) else {
        // Gate-proven unreachable (E-COMPONENT-UNDECLARED); degrade.
        diags.push(Diagnostic {
            code: "E-COMPILE-COMPONENT".to_string(),
            severity: Severity::Error,
            message: "`::use` names no resolvable component (gate should have caught this)"
                .to_string(),
            span: d.span,
            layer: Layer::Content,
            fixits: Vec::new(),
            provenance: None,
        });
        return Vec::new();
    };
    let name = name.unwrap_or_default();
    let args: BTreeMap<String, AttrValue> = d
        .attrs
        .iter()
        .filter(|a| a.key != "component")
        .map(|a| (a.key.clone(), a.value.clone()))
        .collect();
    // Defensive arg/param validation (checker gate: E-COMPONENT-ARG). The
    // invocation's arg key set MUST match `def.params` exactly — no missing,
    // no extra. Compile gates on a clean check, so reaching here with a
    // mismatch is gate-proven unreachable; degrade fail-loud (like the
    // unresolvable-component arm) rather than expand with an unbound `@param`.
    let missing: Vec<&str> = def
        .params
        .iter()
        .map(|(p, _)| p.as_str())
        .filter(|p| !args.contains_key(*p))
        .collect();
    let extra: Vec<&str> = args
        .keys()
        .map(String::as_str)
        .filter(|k| !def.params.iter().any(|(p, _)| p == k))
        .collect();
    if !missing.is_empty() || !extra.is_empty() {
        let mut parts = Vec::new();
        if !missing.is_empty() {
            parts.push(format!("missing [{}]", missing.join(", ")));
        }
        if !extra.is_empty() {
            parts.push(format!("unknown [{}]", extra.join(", ")));
        }
        diags.push(Diagnostic {
            code: "E-COMPILE-COMPONENT".to_string(),
            severity: Severity::Error,
            message: format!(
                "`::use` args for component `{name}` do not match its params: {} (gate should have caught this)",
                parts.join("; ")
            ),
            span: d.span,
            layer: Layer::Content,
            fixits: Vec::new(),
            provenance: None,
        });
        return Vec::new();
    }
    let mut body: Vec<Node> = def
        .body
        .shots
        .iter()
        .flat_map(|s| s.body.iter().cloned())
        .collect();
    bind_params(&mut body, &args, &def.params);
    // Nested `::use` in the body expands recursively (acyclic per checker).
    normalize_nodes(&mut body, components, schema, diags);
    // §6.4: static selection / residual dispatch for any param-scoped
    // `<match>` in the bound body — runs ONLY here, on this clone (B2).
    fold_component_matches(&mut body, schema);

    let span = d.span;
    let begin = Node::Directive(Directive {
        tag: COMPONENT_BEGIN.to_string(),
        attrs: vec![Attr {
            key: "component".to_string(),
            value: AttrValue::Str(name),
            value_span: span,
            span,
        }],
        span,
    });
    let end = Node::Directive(Directive {
        tag: COMPONENT_END.to_string(),
        attrs: Vec::new(),
        span,
    });
    let mut out = Vec::with_capacity(body.len() + 2);
    out.push(begin);
    out.append(&mut body);
    out.push(end);
    out
}

/// §6.4: fold a param-scoped `<match>` at `::use` expansion time (dsl 0.4.0
/// §6.4) — runs at the END of [`expand_use`] on the bound clone, and ONLY
/// there (B2: a scene-level `<match>` is never touched by this pass —
/// `normalize_nodes`'s own `Node::Match` arm recurses into arm bodies only
/// to expand a nested `::use`, never calling this fold).
///
/// By the time this runs, `bind_params` has ALREADY textually substituted
/// every `@param` occurrence in `nodes` with its bound arg's CEL text: a
/// literal arg becomes a literal (`'fond'`, `true`, `3`); a caller-side
/// `@def` ref (`tier=@currentTier`) becomes that ref's bare text
/// (`@currentTier`). A component body may never itself hold a `@def` (§6.2
/// purity — `E-COMPONENT-STATE`), so no def table is ever needed here, and
/// the substituted `@currentTier` text is unexpandable (D3: a bodiless ref
/// stays a marker) — undecided by construction, exactly the case-2 split.
///
/// Case 1 (subject AND every needed arm condition decide): splice the
/// selected arm's body in place of the match — no match record emitted.
/// This function's OWN scan loop re-visits the spliced nodes at the SAME
/// index (it never advances `i` after a splice), so a nested param
/// `<match>` folds recursively too — `normalize_nodes`'s outer `::use` loop
/// does NOT re-scan past a splice (`i += n`, above), so this recursion has
/// to live here, not there.
///
/// Case 2 (subject or any needed condition undecided): leave the
/// `Node::Match` intact — it lowers to the ordinary `MatchCmd` later
/// (stage.rs `walk_match`) — but still recurse into every arm's body: an
/// unrelated NESTED param `<match>` inside a residual arm may still fold on
/// its own terms.
pub fn fold_component_matches(nodes: &mut Vec<Node>, schema: &StateSchema) {
    let mut i = 0;
    while i < nodes.len() {
        let selection = if let Node::Match(m) = &nodes[i] {
            decide_component_expr(&m.subject.raw, None, schema)
                .and_then(|subj| select_component_arm(&m.arms, &subj, schema))
        } else {
            None
        };
        if let Some(idx) = selection {
            let Node::Match(m) = nodes.remove(i) else {
                unreachable!("`selection` is Some only when nodes[i] was Node::Match")
            };
            let body = match m.arms.into_iter().nth(idx) {
                Some(Arm::When { body, .. }) | Some(Arm::Otherwise { body, .. }) => body,
                None => unreachable!("idx came from select_component_arm over these SAME arms"),
            };
            nodes.splice(i..i, body);
            continue; // re-scan from `i`: the recursion this fold owns (see doc comment).
        }
        if let Node::Match(m) = &mut nodes[i] {
            for arm in &mut m.arms {
                match arm {
                    Arm::When { body, .. } | Arm::Otherwise { body, .. } => {
                        fold_component_matches(body, schema)
                    }
                }
            }
        }
        i += 1;
    }
}

/// Decide `raw` under §5.1 with an empty def table and an empty param-domain
/// map (nothing is ever left to resolve through either by the time this
/// runs — see [`fold_component_matches`]'s doc comment). `dollar` is
/// `Some(v)` in arm-decision mode (`$` bound to the already-decided
/// subject, `DollarBinding::Value`) and `None` in subject-decision mode (no
/// `$` in scope for the subject slot itself).
fn decide_component_expr(
    raw: &str,
    dollar: Option<Decided>,
    schema: &StateSchema,
) -> Option<Decided> {
    let empty_bodies = BTreeMap::new();
    let empty_def_params = BTreeMap::new();
    let defs = DefTable {
        bodies: &empty_bodies,
        params: &empty_def_params,
    };
    let empty_params = BTreeMap::new();
    let ctx = DecideCtx {
        schema,
        dollar: dollar.map(DollarBinding::Value),
        params: &empty_params,
    };
    decide_slot(raw, &defs, &ctx)
}

/// Walk `arms` top-to-bottom against the decided subject `subj` (§6.4 step
/// 2): an `is` pattern is literal-set membership ([`is_pattern_literals`] +
/// [`is_literal_matches`]) — always decidable given a decided subject, no
/// runtime unknowns possible; a `test` guard decides via `decide_slot` with
/// `$` bound to `subj`. `is` + `test` together is AND (dsl §7.3.1) — an
/// `is` miss skips the arm WITHOUT needing `test` to decide (sound: the arm
/// provably doesn't fire regardless of `test`'s value).
///
/// Returns `Some(idx)` — the DEFINITELY-selected arm — only when every arm
/// visited before it definitely does NOT fire; `None` the instant an arm's
/// firing is itself undecided (§6.4 case 2 — the caller leaves the whole
/// match as a residual record). `<otherwise>` always selects when reached
/// (exhaustiveness is proven statically, §6.3).
fn select_component_arm(arms: &[Arm], subj: &Decided, schema: &StateSchema) -> Option<usize> {
    for (idx, arm) in arms.iter().enumerate() {
        match arm {
            Arm::Otherwise { .. } => return Some(idx),
            Arm::When { is, test, .. } => {
                if let Some(pat) = is {
                    let literals = is_pattern_literals(&pat.raw, pat.span);
                    if !literals.iter().any(|(lit, _)| is_literal_matches(lit, subj)) {
                        continue; // `is` provably misses: skip, `test` irrelevant.
                    }
                }
                if test.raw.trim().is_empty() {
                    return Some(idx); // `is` matched (or absent) and no guard to add.
                }
                match decide_component_expr(&test.raw, Some(subj.clone()), schema) {
                    Some(Decided::Bool(true)) => return Some(idx),
                    Some(Decided::Bool(false)) => continue,
                    _ => return None, // undecided (or an ill-typed non-bool verdict): bail.
                }
            }
        }
    }
    None // exhaustiveness is a checker invariant; total fallback: stay residual.
}

/// Mirror `lute_check::match_check`'s `classify_when_literal` classification
/// (dsl §7.3.1: `EnumMember | "true" | "false" | Number | "unset"`) to
/// compare one `is=` literal against a §6.4-decided constant. A param is
/// never `unset` (§6.3, checker-enforced via `E-WHEN-LITERAL-DOMAIN`), so
/// `unset` never matches here.
fn is_literal_matches(lit: &str, decided: &Decided) -> bool {
    match lit {
        "unset" => false,
        "true" => matches!(decided, Decided::Bool(true)),
        "false" => matches!(decided, Decided::Bool(false)),
        _ => match decided {
            Decided::Num(n) => lit.parse::<f64>().map(|v| v == *n).unwrap_or(false),
            Decided::Str(s) => s == lit,
            Decided::Bool(_) => false,
        },
    }
}

/// Bind `@param` uses to `::use` args. A whole-slot `@param` attr value is
/// replaced VALUE-LEVEL (a string arg becomes a plain `Str` attr — what a
/// string-typed attr position needs); a `@param` inside a larger CEL is
/// substituted textually, typed by the param's declared [`Type`].
fn bind_params(nodes: &mut [Node], args: &BTreeMap<String, AttrValue>, params: &[(String, Type)]) {
    for node in nodes {
        match node {
            Node::Line(l) => {
                // T11 fix: a component-body gated line's `when=` slot is a
                // CEL fragment just like any attr — it must see the SAME
                // `@param` -> arg substitution `l.attrs` gets, or the T11
                // desugar (and T8's `fold_component_matches`, which runs
                // right after this on the same bound clone) would fold/
                // decide against an unbound `@tier`-shaped marker instead
                // of the caller's actual argument text.
                if let Some(w) = &mut l.when {
                    bind_slot_raw(w, args, params);
                }
                bind_attrs(&mut l.attrs, args, params);
            }
            Node::Directive(d) => bind_attrs(&mut d.attrs, args, params),
            Node::Set(s) => bind_slot(&mut s.expr, args, params),
            Node::Branch(b) => {
                for c in &mut b.choices {
                    if let Some(w) = &mut c.when {
                        bind_slot(w, args, params);
                    }
                    bind_attrs(&mut c.attrs, args, params);
                    bind_params(&mut c.body, args, params);
                }
            }
            Node::Match(m) => {
                bind_slot(&mut m.subject, args, params);
                for arm in &mut m.arms {
                    match arm {
                        Arm::When { test, body, .. } => {
                            bind_slot(test, args, params);
                            bind_params(body, args, params);
                        }
                        Arm::Otherwise { body, .. } => bind_params(body, args, params),
                    }
                }
            }
            Node::Timeline(t) => {
                for track in &mut t.tracks {
                    for clip in &mut track.clips {
                        match &mut clip.node {
                            ClipNode::Directive(d) => bind_attrs(&mut d.attrs, args, params),
                            ClipNode::Set(s) => bind_slot(&mut s.expr, args, params),
                        }
                    }
                }
            }
            Node::Hub(h) => {
                for c in &mut h.choices {
                    if let Some(w) = &mut c.when {
                        bind_slot(w, args, params);
                    }
                    bind_attrs(&mut c.attrs, args, params);
                    bind_params(&mut c.body, args, params);
                }
            }
            Node::On(on) => {
                if let Some(w) = &mut on.when {
                    bind_slot(w, args, params);
                }
                bind_attrs(&mut on.attrs, args, params);
                bind_params(&mut on.body, args, params);
            }
            Node::Objective(o) => {
                bind_slot(&mut o.done, args, params);
                if let Some(w) = &mut o.when {
                    bind_slot(w, args, params);
                }
                bind_attrs(&mut o.attrs, args, params);
                bind_params(&mut o.body, args, params);
            }
            // Fact args are ground — no `@param` binding target (0.3.0 T2).
            Node::Assert(_) | Node::Retract(_) => {}
        }
    }
}

fn bind_attrs(attrs: &mut [Attr], args: &BTreeMap<String, AttrValue>, params: &[(String, Type)]) {
    for a in attrs {
        let AttrValue::Ref(slot) = &mut a.value else {
            continue;
        };
        // Whole-slot `@param` → value-level replacement.
        if let Some(name) = slot.raw.trim().strip_prefix('@') {
            if let Some(arg) = args.get(name) {
                a.value = arg.clone();
                continue;
            }
        }
        bind_slot_raw(slot, args, params);
    }
}

fn bind_slot(slot: &mut CelSlot, args: &BTreeMap<String, AttrValue>, params: &[(String, Type)]) {
    bind_slot_raw(slot, args, params);
}

/// Textual `@param` → arg substitution inside a CEL fragment (right-to-left).
fn bind_slot_raw(
    slot: &mut CelSlot,
    args: &BTreeMap<String, AttrValue>,
    params: &[(String, Type)],
) {
    let refs = lute_cel::scan_refs(&slot.raw);
    for r in refs.iter().rev() {
        if r.is_dollar || r.call.is_some() {
            continue; // params are 0-arity; calls/`$` belong to the expander
        }
        let Some(arg) = args.get(&r.name) else {
            continue;
        };
        let ty = params.iter().find(|(n, _)| n == &r.name).map(|(_, t)| t);
        let text = arg_cel_text(arg, ty);
        slot.raw
            .replace_range(r.span.byte_start..r.span.byte_end, &text);
    }
}

fn arg_cel_text(arg: &AttrValue, ty: Option<&Type>) -> String {
    match arg {
        AttrValue::BoolTrue => "true".to_string(),
        AttrValue::Ref(slot) => slot.raw.clone(),
        AttrValue::Str(s) => match ty {
            Some(Type::Number) | Some(Type::Bool) => s.clone(),
            _ => cel_string_literal(s),
        },
    }
}

/// `<choice … persist="run" into="run.<path>" [value="<lit>"]>` → append
/// `Node::Set(run.<path> = <value>)` (dsl §11.1.1). Well-formedness is
/// gate-proven (E-PERSIST-*); anything unresolvable here is skipped, total.
fn synth_persist(choice: &mut Choice, schema: &StateSchema) {
    let find = |k: &str| choice.attrs.iter().find(|a| a.key == k);
    let persists = matches!(
        find("persist").map(|a| &a.value),
        Some(AttrValue::Str(s)) if s == "run"
    );
    if !persists {
        return;
    }
    let Some(AttrValue::Str(into_path)) = find("into").map(|a| &a.value) else {
        return; // gate: E-PERSIST-MISSING-INTO
    };
    let into_path = into_path.clone();
    let Some(decl) = schema.decls.get(into_path.as_str()) else {
        return; // gate: E-PERSIST-TARGET
    };
    let value = find("value").and_then(|a| match &a.value {
        AttrValue::Str(s) => Some(s.clone()),
        AttrValue::BoolTrue => Some("true".to_string()),
        AttrValue::Ref(_) => None, // gate: E-PERSIST-VALUE
    });
    let cel = persist_value_cel(&decl.ty, value.as_deref());
    let span = find("into").map(|a| a.span).unwrap_or(choice.span);
    push_set(choice, into_path, cel, span);
}

fn push_set(choice: &mut Choice, path: String, cel: String, span: Span) {
    choice.body.push(Node::Set(Set {
        path,
        path_span: span,
        op: "=".to_string(),
        expr: CelSlot::raw(CelKind::SetExpr, cel, span),
        span,
    }));
}

/// dsl §11.1.1 rule 4: bool target's value is optional (defaults `true`);
/// number stays bare; everything else (enum/str) is a CEL string literal.
fn persist_value_cel(ty: &Type, value: Option<&str>) -> String {
    match ty {
        Type::Bool => value.unwrap_or("true").to_string(),
        Type::Number => value.unwrap_or("0").to_string(),
        _ => cel_string_literal(value.unwrap_or_default()),
    }
}

/// Quote `s` as a single-quoted CEL string literal (backslash escaping, §4.4).
pub fn cel_string_literal(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for c in s.chars() {
        if c == '\\' || c == '\'' {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('\'');
    out
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use lute_check::meta::{Namespace, StateDecl, StateSchema};
    use lute_check::resolve_components;
    use lute_core_span::Severity;
    use lute_manifest::types::{Literal, Type};
    use lute_syntax::ast::{AttrValue, Node};

    use super::*;

    fn parse_clean(src: &str) -> lute_syntax::ast::Document {
        let (doc, diags) = lute_syntax::parse(src);
        assert!(
            diags.iter().all(|d| d.severity != Severity::Error),
            "{diags:#?}"
        );
        doc
    }

    #[test]
    fn use_expands_component_inline_with_bound_params_and_sentinels() {
        // Real fixture: docs/examples/components/greet.component.lute declares
        // `component: greet`, `params: { who: string }`, body =
        // `::auto{character=@who action="fade-in-up"}` + a narrator line.
        let base = Path::new("../../docs/examples/components");
        let scene = std::fs::read_to_string(base.join("scene.lute")).unwrap();
        let mut doc = parse_clean(&scene);
        let comps = resolve_components(base, &["greet.component.lute".to_string()], doc.meta.span);
        assert!(comps.diags.is_empty(), "{:#?}", comps.diags);
        let diags = normalize_document(&mut doc, &comps, &StateSchema::default());
        assert!(diags.is_empty(), "{diags:#?}");

        let body = &doc.shots[0].body;
        // ::use replaced by: begin sentinel, ::auto (param bound), line, end sentinel, then the scene's own line.
        let tags: Vec<String> = body
            .iter()
            .map(|n| match n {
                Node::Directive(d) => format!("::{}", d.tag),
                Node::Line(l) => format!("@{}", l.speaker),
                _ => "other".to_string(),
            })
            .collect();
        assert_eq!(
            tags,
            vec![
                format!("::{COMPONENT_BEGIN}"),
                "::auto".to_string(),
                "@narrator".to_string(),
                format!("::{COMPONENT_END}"),
                "@narrator".to_string(),
            ]
        );
        // `character=@who` became the VALUE-LEVEL string arg (whole-slot bind).
        let Node::Directive(auto) = &body[1] else {
            panic!("auto")
        };
        let ch = auto.attrs.iter().find(|a| a.key == "character").unwrap();
        assert!(
            matches!(&ch.value, AttrValue::Str(s) if s == "bianca"),
            "{ch:?}"
        );
        // No `::use` survives normalization (D8).
        assert!(body
            .iter()
            .all(|n| !matches!(n, Node::Directive(d) if d.tag == "use")));
    }

    #[test]
    fn use_inside_a_timeline_clip_fails_loud() {
        // A `::use` clip cannot be inline-expanded (a ClipNode is Directive|Set),
        // so normalization emits E-COMPILE-COMPONENT rather than dropping it
        // silently (spec-gap note 9); compile() then aborts at the §5 diag gate,
        // so no artifact is produced. RED before the Timeline arm in normalize_nodes.
        let src = r#"---
kind: scene
character: x
season: 1
episode: 1
---
## Shot 1.
<timeline>
  <track channel="fg">
    ::use{component="greet"}
  </track>
</timeline>
"#;
        let mut doc = parse_clean(src);
        let comps = resolve_components(Path::new("."), &[], doc.meta.span);
        let diags = normalize_document(&mut doc, &comps, &StateSchema::default());
        assert!(
            diags.iter().any(|d| d.code == "E-COMPILE-COMPONENT"),
            "expected E-COMPILE-COMPONENT for a ::use timeline clip, got {diags:#?}"
        );
    }

    #[test]
    fn use_with_mismatched_args_fails_loud_no_unbound_param_leaks() {
        // Defensive backstop for the checker's E-COMPONENT-ARG: greet declares
        // `params: { who: string }`, but this `::use` supplies no `who` and an
        // unknown `extra` arg. Normalization must degrade to E-COMPILE-COMPONENT
        // rather than expand the body with an unbound `@who` — no component
        // sentinels, no spliced `::auto`, no residual `::use`.
        let base = Path::new("../../docs/examples/components");
        let src = r#"---
kind: scene
character: demo
season: 1
episode: 1
components: [greet.component.lute]
---

## Shot 1.

::use{component="greet" extra="oops"}
@narrator: And the scene carries on.
"#;
        let mut doc = parse_clean(src);
        let comps = resolve_components(base, &["greet.component.lute".to_string()], doc.meta.span);
        assert!(comps.diags.is_empty(), "{:#?}", comps.diags);
        let diags = normalize_document(&mut doc, &comps, &StateSchema::default());
        let arg_err = diags
            .iter()
            .find(|d| d.code == "E-COMPILE-COMPONENT" && d.severity == Severity::Error);
        assert!(
            arg_err.is_some(),
            "expected E-COMPILE-COMPONENT for mismatched ::use args, got {diags:#?}"
        );
        // Message names the component and both the missing and the extra arg.
        let msg = &arg_err.unwrap().message;
        assert!(
            msg.contains("greet") && msg.contains("who") && msg.contains("extra"),
            "diagnostic should name component + mismatched params: {msg:?}"
        );
        // No expansion leaked: no sentinels, no `::auto` body, no `::use` remnant.
        let body = &doc.shots[0].body;
        assert!(
            body.iter().all(|n| !matches!(n, Node::Directive(d)
                if d.tag == COMPONENT_BEGIN
                    || d.tag == COMPONENT_END
                    || d.tag == "auto"
                    || d.tag == "use")),
            "no component body should splice in on arg mismatch: {body:#?}"
        );
    }

    #[test]
    fn persist_synthesizes_trailing_set_nodes() {
        let src = r#"---
kind: scene
character: sofia
season: 1
episode: 1
---

## Shot 1.

<branch id="sofaHelp">
  <choice id="help" label="Help her up" persist="run" into="run.metHelpfully">
    @sofia: Thank you.
  </choice>
  <choice id="warmly" label="Stay a while" persist="run" into="run.outcome" value="warm">
    @sofia: Kind.
  </choice>
  <choice id="tip" label="Leave a tip" persist="run" into="run.tip" value="5">
    @sofia: Oh.
  </choice>
</branch>
"#;
        let mut doc = parse_clean(src);
        let mut schema = StateSchema::default();
        schema.decls.insert(
            "run.metHelpfully".to_string(),
            StateDecl {
                ty: Type::Bool,
                default: Some(Literal::Bool(false)),
                namespace: Namespace::Run,
            },
        );
        schema.decls.insert(
            "run.outcome".to_string(),
            StateDecl {
                ty: Type::Enum(vec!["warm".into(), "cold".into()]),
                default: None,
                namespace: Namespace::Run,
            },
        );
        schema.decls.insert(
            "run.tip".to_string(),
            StateDecl {
                ty: Type::Number,
                default: None,
                namespace: Namespace::Run,
            },
        );
        let diags = normalize_document(&mut doc, &Default::default(), &schema);
        assert!(diags.is_empty(), "{diags:#?}");

        let Node::Branch(b) = &doc.shots[0].body[0] else {
            panic!("branch")
        };
        let last_set = |i: usize| -> (&str, &str, &str) {
            let Some(Node::Set(s)) = b.choices[i].body.last() else {
                panic!("choice {i} ends in a synthesized Set");
            };
            (s.path.as_str(), s.op.as_str(), s.expr.raw.as_str())
        };
        // bool target, no value => `= true` (dsl §11.1.1 rule 4).
        assert_eq!(last_set(0), ("run.metHelpfully", "=", "true"));
        // enum target => quoted CEL string literal.
        assert_eq!(last_set(1), ("run.outcome", "=", "'warm'"));
        // number target => bare numeric literal.
        assert_eq!(last_set(2), ("run.tip", "=", "5"));
        // The authored line plus exactly one synthesized Set per persisting choice.
        assert_eq!(b.choices[0].body.len(), 2);
    }

    fn tag_of(n: &Node) -> String {
        match n {
            Node::Directive(d) => format!("::{}", d.tag),
            Node::Line(l) => format!("@{}", l.speaker),
            Node::Objective(_) => "objective".to_string(),
            Node::On(_) => "on".to_string(),
            _ => "other".to_string(),
        }
    }

    /// No `::use` survives normalization anywhere, including nested
    /// `<on>`/`<objective>` bodies.
    fn no_use_directive(nodes: &[Node]) -> bool {
        nodes.iter().all(|n| match n {
            Node::Directive(d) => d.tag != "use",
            Node::On(o) => no_use_directive(&o.body),
            Node::Objective(o) => no_use_directive(&o.body),
            _ => true,
        })
    }

    // Plan D review (Important finding 1): `normalize_nodes` fell into
    // `_ => {}` for `Node::On`/`Node::Objective`, so a `::use` inside an
    // `<on>`/`<objective>` body was never inline-expanded — it survived as a
    // literal `::use` directive, which `lower_directive` silently drops at
    // lowering time (the component's content never reaches the artifact).
    #[test]
    fn normalize_document_traverses_quest_bodies_expanding_use_in_on_and_objective() {
        let base = Path::new("../../docs/examples/components");
        let src = r#"---
kind: quest
components: [greet.component.lute]
---

<quest id="q1">
<objective id="o1" done="true"/>

::use{component="greet" who="bianca"}

<on event="questComplete">
::use{component="greet" who="halsin"}
</on>
</quest>
"#;
        let mut doc = parse_clean(src);
        assert_eq!(doc.quests.len(), 1, "fixture must parse one <quest>");
        let comps = resolve_components(base, &["greet.component.lute".to_string()], doc.meta.span);
        assert!(comps.diags.is_empty(), "{:#?}", comps.diags);
        let diags = normalize_document(&mut doc, &comps, &StateSchema::default());
        assert!(diags.is_empty(), "{diags:#?}");

        let quest = &doc.quests[0];
        let tags: Vec<String> = quest.body.iter().map(tag_of).collect();
        assert_eq!(
            tags,
            vec![
                "objective".to_string(),
                format!("::{COMPONENT_BEGIN}"),
                "::auto".to_string(),
                "@narrator".to_string(),
                format!("::{COMPONENT_END}"),
                "on".to_string(),
            ]
        );
        let Node::On(on) = &quest.body[5] else {
            panic!("expected on, got {:?}", quest.body.get(5));
        };
        let on_tags: Vec<String> = on.body.iter().map(tag_of).collect();
        assert_eq!(
            on_tags,
            vec![
                format!("::{COMPONENT_BEGIN}"),
                "::auto".to_string(),
                "@narrator".to_string(),
                format!("::{COMPONENT_END}"),
            ],
            "the ::use inside <on> must expand, not be dropped silently"
        );
        assert!(no_use_directive(&quest.body));
    }

    // Plan D review (Important finding 1, `bind_params`): line 270 was
    // `Node::On(_) | Node::Objective(_) => {}`, so a component whose OWN
    // body declares an `<on>`/`<objective>` guarded by `@param` leaked the
    // unbound `@param` into the spliced quest content instead of the
    // resolved `::use` arg.
    #[test]
    fn use_binds_params_inside_component_on_and_objective_bodies() {
        let component_src = r#"---
component: reactor
params:
  n: number
---

## Scene 1.

<on event="questComplete" when="@n > 0" foo=@n>
::set{run.score = run.score + @n}
</on>
<objective id="bonus" done="@n > 3" when="@n > 1">
::set{run.bonus = @n}
</objective>
"#;
        let comp_doc = parse_clean(component_src);
        let mut table = BTreeMap::new();
        table.insert(
            "reactor".to_string(),
            lute_check::ComponentDef {
                params: vec![("n".to_string(), Type::Number)],
                body: comp_doc,
                src: std::path::PathBuf::from("test://reactor"),
            },
        );
        let comps = ComponentSet {
            table,
            diags: Vec::new(),
        };

        let src = r#"---
kind: quest
---

<quest id="q1">
<objective id="o1" done="true"/>

::use{component="reactor" n=5}
</quest>
"#;
        let mut doc = parse_clean(src);
        let diags = normalize_document(&mut doc, &comps, &StateSchema::default());
        assert!(diags.is_empty(), "{diags:#?}");

        let quest = &doc.quests[0];
        let Node::On(on) = &quest.body[2] else {
            panic!("expected on, got {:?}", quest.body.get(2));
        };
        let when = on.when.as_ref().expect("on.when");
        assert_eq!(when.raw, "5 > 0");
        assert!(!when.raw.contains('@'));
        let foo = on.attrs.iter().find(|a| a.key == "foo").expect("foo attr");
        assert!(
            matches!(&foo.value, AttrValue::Str(s) if s == "5"),
            "{foo:?}"
        );
        let Node::Set(s) = &on.body[0] else {
            panic!("expected set, got {:?}", on.body.first());
        };
        assert_eq!(s.expr.raw, "run.score + 5");

        let Node::Objective(obj) = &quest.body[3] else {
            panic!("expected objective, got {:?}", quest.body.get(3));
        };
        assert_eq!(obj.done.raw, "5 > 3");
        assert!(!obj.done.raw.contains('@'));
        let owhen = obj.when.as_ref().expect("objective.when");
        assert_eq!(owhen.raw, "5 > 1");
        let Node::Set(s2) = &obj.body[0] else {
            panic!("expected set, got {:?}", obj.body.first());
        };
        assert_eq!(s2.expr.raw, "5");
    }

    #[test]
    fn cel_string_literal_escapes_quotes_and_backslashes() {
        assert_eq!(cel_string_literal("warm"), "'warm'");
        assert_eq!(cel_string_literal("it's"), "'it\\'s'");
        assert_eq!(cel_string_literal("a\\b"), "'a\\\\b'");
    }
}
