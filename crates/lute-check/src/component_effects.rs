//! dsl 0.24.0 §4: component `effects: true` and `speaker` params — the
//! pieces of `::use` expansion the checker and the compiler share.
//!
//! * **Binding and folding** (dsl §13, 0.4.0 §6.4): `@param` → argument
//!   substitution ([`bind_attrs`], [`bind_slot_raw`]) and the param-scoped
//!   `<match>` fold ([`fold_component_matches`]). `lute-compile`'s
//!   `normalize` expands a `::use` with these; the checker binds an effects
//!   body's writes with the SAME functions, so both see the same writes.
//! * **Effects splice** ([`splice_component_effects`]): right after every
//!   `::use` of an `effects: true` component, the bound, folded writes of its
//!   body — `::set` / `::assert` / `::retract`, a directive declaring state
//!   writes, and any `<match>` still holding one — are inserted into the
//!   HOST document, every span re-anchored at the `::use`. The host's own
//!   passes (schema, set type, ownership, relation vocabulary, permissions,
//!   definite assignment, fact Must/may) then judge each write where it
//!   happens, against the host's schema. The `::use` itself stays; `lute
//!   compile` expands the whole body through `normalize` as before.
//! * **Speaker params**: a `speaker` param is an enum over the host's cast
//!   ids (plus `narrator`) when a cast is declared, a string otherwise
//!   ([`host_param_types`]); `{{@p}}` renders the cast member's name
//!   ([`speaker_display_args`]).

use std::collections::BTreeMap;

use lute_core_span::Span;
use lute_manifest::schema::CastMember;
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_manifest::types::Type;
use lute_syntax::ast::{Arm, Attr, AttrValue, CelSlot, Directive, Document, Node};
use lute_syntax::datalog::{FactPattern, FactTerm};
use lute_syntax::is_pattern::{classify_is_literal, IsLiteral};

use crate::cel_expand::DefTable;
use crate::component_import::{ComponentDef, ComponentSet};
use crate::decide::{decide_slot, DecideCtx, Decided, DollarBinding};
use crate::match_check::is_pattern_literals;
use crate::meta::StateSchema;

/// The type a `speaker` param has at a host (dsl 0.24.0 §4): the host's cast
/// ids plus `narrator` (always a speaker) as an enum when any cast is
/// declared; a string when speakers are shape-only.
pub fn speaker_param_type(cast: &BTreeMap<String, CastMember>) -> Type {
    if cast.is_empty() {
        return Type::Str;
    }
    let mut ids: Vec<String> = cast.keys().cloned().collect();
    if !cast.contains_key("narrator") {
        ids.push("narrator".to_string());
        ids.sort();
    }
    Type::Enum(ids)
}

/// A component's params as a host sees them: each `speaker` param typed by
/// [`speaker_param_type`], every other param as declared.
pub fn host_param_types(
    params: &[(String, Type)],
    speakers: &[String],
    cast: &BTreeMap<String, CastMember>,
) -> Vec<(String, Type)> {
    params
        .iter()
        .map(|(name, ty)| {
            if speakers.contains(name) {
                (name.clone(), speaker_param_type(cast))
            } else {
                (name.clone(), ty.clone())
            }
        })
        .collect()
}

/// The `::use` arguments by name (every attr but `component`).
pub fn use_args(d: &Directive) -> BTreeMap<String, AttrValue> {
    d.attrs
        .iter()
        .filter(|a| a.key != "component")
        .map(|a| (a.key.clone(), a.value.clone()))
        .collect()
}

/// For text interpolation only: each `speaker` param bound to a literal cast
/// id, mapped to that member's display name (the id itself when the member
/// has no `name`, or the id is not in the cast). Attribute positions keep the
/// id; this map is only ever handed to the line-text binder.
pub fn speaker_display_args(
    def: &ComponentDef,
    args: &BTreeMap<String, AttrValue>,
    cast: &BTreeMap<String, CastMember>,
) -> BTreeMap<String, AttrValue> {
    def.speakers
        .iter()
        .filter_map(|p| match args.get(p)? {
            AttrValue::Str(id) => {
                let name = cast
                    .get(id)
                    .and_then(|c| c.name.clone())
                    .unwrap_or_else(|| id.clone());
                Some((p.clone(), AttrValue::Str(name)))
            }
            _ => None,
        })
        .collect()
}

/// Bind every `@param` in `attrs`: a whole-slot `@param` value is replaced
/// VALUE-LEVEL (a string arg becomes a plain `Str` attr — what a
/// string-typed attr position needs); a `@param` inside a larger CEL is
/// substituted textually ([`bind_slot_raw`]).
pub fn bind_attrs(attrs: &mut [Attr], args: &BTreeMap<String, AttrValue>, params: &[(String, Type)]) {
    for a in attrs {
        let AttrValue::Ref(slot) = &mut a.value else {
            continue;
        };
        if let Some(name) = slot.raw.trim().strip_prefix('@') {
            if let Some(arg) = args.get(name) {
                a.value = arg.clone();
                continue;
            }
        }
        bind_slot_raw(slot, args, params);
    }
}

/// Textual `@param` → arg substitution inside a CEL fragment (right-to-left),
/// each arg rendered as CEL by its param's declared [`Type`].
pub fn bind_slot_raw(
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

/// The fact-atom constant a `::use` argument binds a `@param` to (dsl 0.24.0
/// §4): an identifier (an entity or enum member id) or `true`/`false`. `Err`
/// names why any other argument — a CEL expression, a `@def`, a number, a
/// string that is no identifier — cannot be a fact argument, which is ground.
pub fn fact_arg_constant(arg: &AttrValue) -> Result<FactTerm, String> {
    match arg {
        AttrValue::BoolTrue => Ok(FactTerm::Bool(true)),
        AttrValue::Str(s) => match s.as_str() {
            "true" => Ok(FactTerm::Bool(true)),
            "false" => Ok(FactTerm::Bool(false)),
            _ if s.starts_with(|c: char| c.is_ascii_alphabetic())
                && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') =>
            {
                Ok(FactTerm::Ident(s.clone()))
            }
            _ => Err(format!("`{s}` is not an identifier")),
        },
        AttrValue::Ref(slot) => Err(format!(
            "`{}` is an expression, decided only at runtime",
            slot.raw.trim()
        )),
    }
}

/// Bind every `@param` argument of `pattern` whose `::use` argument is a
/// constant ([`fact_arg_constant`]); `false` when one stays unbound (the
/// `::use` check reports why).
pub fn bind_fact(pattern: &mut FactPattern, args: &BTreeMap<String, AttrValue>) -> bool {
    let mut bound = true;
    for a in &mut pattern.args {
        if let FactTerm::Param(p) = &a.term {
            match args.get(p).map(fact_arg_constant) {
                Some(Ok(term)) => a.term = term,
                _ => bound = false,
            }
        }
    }
    bound
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

/// §6.4: fold a param-scoped `<match>` at `::use` expansion time (dsl 0.4.0
/// §6.4) — runs on the bound clone of a component body, and ONLY there (B2:
/// a scene-level `<match>` is never touched by this pass).
///
/// By the time this runs, every `@param` occurrence in `nodes` has ALREADY
/// been textually substituted with its bound arg's CEL text: a literal arg
/// becomes a literal (`'fond'`, `true`, `3`); a caller-side `@def` ref
/// (`tier=@currentTier`) becomes that ref's bare text (`@currentTier`). A
/// component body may never itself hold a `@def` (§6.2 purity —
/// `E-COMPONENT-STATE`), so no def table is ever needed here, and the
/// substituted `@currentTier` text is unexpandable (D3: a bodiless ref stays
/// a marker) — undecided by construction, exactly the case-2 split.
///
/// Case 1 (subject AND every needed arm condition decide): splice the
/// selected arm's body in place of the match — no match record emitted.
/// This function's OWN scan loop re-visits the spliced nodes at the SAME
/// index (it never advances `i` after a splice), so a nested param
/// `<match>` folds recursively too.
///
/// Case 2 (subject or any needed condition undecided): leave the
/// `Node::Match` intact — it lowers to the ordinary `MatchCmd` later — but
/// still recurse into every arm's body: an unrelated NESTED param `<match>`
/// inside a residual arm may still fold on its own terms.
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
        facts: None,
    };
    decide_slot(raw, &defs, &ctx)
}

/// Walk `arms` top-to-bottom against the decided subject `subj` (§6.4 step
/// 2): an `is` pattern is literal-set membership ([`is_pattern_literals`] +
/// [`is_literal_matches`]) — always decidable given a decided subject, no
/// runtime unknowns possible; a `test` guard decides via `decide_slot` with
/// `$` bound to `subj`. `is` + `test` together is AND (dsl §7.3.1) — an
/// `is` miss skips the arm WITHOUT needing `test` to decide.
///
/// Returns `Some(idx)` — the DEFINITELY-selected arm — only when every arm
/// visited before it definitely does NOT fire; `None` the instant an arm's
/// firing is itself undecided (§6.4 case 2). `<otherwise>` always selects
/// when reached (exhaustiveness is proven statically, §6.3).
fn select_component_arm(arms: &[Arm], subj: &Decided, schema: &StateSchema) -> Option<usize> {
    for (idx, arm) in arms.iter().enumerate() {
        match arm {
            Arm::Otherwise { .. } => return Some(idx),
            Arm::When { is, test, .. } => {
                if let Some(pat) = is {
                    let literals = is_pattern_literals(&pat.raw, pat.span);
                    if !literals
                        .iter()
                        .any(|(lit, _)| is_literal_matches(lit, subj))
                    {
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

/// Compare one `is=` literal, classified by the shared
/// [`classify_is_literal`], against a §6.4-decided constant. A param is never
/// `unset` (§6.3), so `unset` never matches here. A numeric range matches a
/// decided number inside its inclusive bounds and never a non-number; a
/// malformed range (`E-WHEN-RANGE`) matches nothing.
fn is_literal_matches(lit: &str, decided: &Decided) -> bool {
    match (classify_is_literal(lit), decided) {
        (Ok(IsLiteral::Bool(b)), Decided::Bool(d)) => b == *d,
        (Ok(IsLiteral::Num(n)), Decided::Num(d)) => n == *d,
        (Ok(IsLiteral::Range(range)), Decided::Num(d)) => range.contains(*d),
        (Ok(IsLiteral::Str(s)), Decided::Str(d)) => s == *d,
        _ => false,
    }
}

/// Insert, right after every `::use` of an `effects: true` component in
/// `doc` (shots, quests, entries, bundle beats, at any depth), that use's
/// bound and folded writes — see the module docs. A no-op when no imported
/// component declares `effects: true`, and on a component file itself (it is
/// no host: its writes land wherever it is used).
pub fn splice_component_effects(
    doc: &mut Document,
    components: &ComponentSet,
    snapshot: &CapabilitySnapshot,
) {
    if !components.table.values().any(|d| d.effects)
        || crate::meta::infer_meta_kind_from_shape(&doc.meta, true)
            == Some(crate::meta::MetaKind::Component)
    {
        return;
    }
    let bodies = doc
        .shots
        .iter_mut()
        .map(|s| &mut s.body)
        .chain(doc.quests.iter_mut().map(|q| &mut q.body))
        .chain(doc.entries.iter_mut().map(|e| &mut e.body))
        .chain(doc.beats.iter_mut().map(|b| &mut b.body));
    for body in bodies {
        splice_nodes(body, components, snapshot);
    }
}

fn splice_nodes(nodes: &mut Vec<Node>, components: &ComponentSet, snapshot: &CapabilitySnapshot) {
    let mut i = 0;
    while i < nodes.len() {
        let writes = match &mut nodes[i] {
            Node::Directive(d) if d.tag == "use" => {
                use_writes(d, components, snapshot, &mut Vec::new())
            }
            Node::Branch(b) => {
                b.choices.iter_mut().for_each(|c| splice_nodes(&mut c.body, components, snapshot));
                Vec::new()
            }
            Node::Hub(h) => {
                h.choices.iter_mut().for_each(|c| splice_nodes(&mut c.body, components, snapshot));
                Vec::new()
            }
            Node::Match(m) => {
                for arm in &mut m.arms {
                    match arm {
                        Arm::When { body, .. } | Arm::Otherwise { body, .. } => {
                            splice_nodes(body, components, snapshot)
                        }
                    }
                }
                Vec::new()
            }
            Node::On(o) => {
                splice_nodes(&mut o.body, components, snapshot);
                Vec::new()
            }
            Node::Objective(o) => {
                splice_nodes(&mut o.body, components, snapshot);
                Vec::new()
            }
            _ => Vec::new(),
        };
        let n = writes.len();
        nodes.splice(i + 1..i + 1, writes);
        i += n + 1;
    }
}

/// The writes one `::use` performs in its host, every span at the `::use`.
/// Empty for a presentational component, an unknown one, or a `::use`
/// cycle (each diagnosed elsewhere).
fn use_writes(
    d: &Directive,
    components: &ComponentSet,
    snapshot: &CapabilitySnapshot,
    stack: &mut Vec<String>,
) -> Vec<Node> {
    let Some((name, def)) = d.attrs.iter().find_map(|a| match (&*a.key, &a.value) {
        ("component", AttrValue::Str(s)) => components.table.get(s).map(|def| (s, def)),
        _ => None,
    }) else {
        return Vec::new();
    };
    if !def.effects || stack.contains(name) {
        return Vec::new();
    }
    let mut writes = Vec::new();
    for shot in &def.body.shots {
        write_skeleton(&shot.body, snapshot, &mut writes);
    }
    let args = use_args(d);
    bind_writes(&mut writes, &args, &def.params);
    fold_component_matches(&mut writes, &StateSchema::default());
    stack.push(name.clone());
    expand_nested(&mut writes, components, snapshot, stack);
    stack.pop();
    respan(&mut writes, d.span);
    writes
}

/// The write-bearing part of a component body: `::set`/`::assert`/
/// `::retract`, a directive whose decl declares state writes, a nested
/// `::use` of another effects component, and a `<match>` holding any of
/// them (every arm kept, so arm selection is unchanged).
fn write_skeleton(nodes: &[Node], snapshot: &CapabilitySnapshot, out: &mut Vec<Node>) {
    for node in nodes {
        match node {
            Node::Set(_) | Node::Assert(_) | Node::Retract(_) => out.push(node.clone()),
            Node::Directive(d)
                if d.tag == "use" || crate::check::directive_writes_state(snapshot, &d.tag) =>
            {
                out.push(node.clone())
            }
            Node::Match(m) => {
                let mut arms = Vec::with_capacity(m.arms.len());
                let mut any = false;
                for arm in &m.arms {
                    let mut arm = arm.clone();
                    let body = match &mut arm {
                        Arm::When { body, .. } | Arm::Otherwise { body, .. } => body,
                    };
                    let mut kept = Vec::new();
                    write_skeleton(body, snapshot, &mut kept);
                    any |= !kept.is_empty();
                    *body = kept;
                    arms.push(arm);
                }
                if any {
                    let mut m = m.clone();
                    m.arms = arms;
                    out.push(Node::Match(m));
                }
            }
            _ => {}
        }
    }
}

/// Bind every `@param` in the skeleton. An `::assert` / `::retract` whose
/// param argument is no constant is dropped: the `::use` check reports it
/// (`E-COMPONENT-ARG`), and an unbound atom would only pile on.
fn bind_writes(nodes: &mut Vec<Node>, args: &BTreeMap<String, AttrValue>, params: &[(String, Type)]) {
    nodes.retain_mut(|node| match node {
        Node::Set(s) => {
            bind_slot_raw(&mut s.expr, args, params);
            if let Some(w) = &mut s.when {
                bind_slot_raw(w, args, params);
            }
            true
        }
        Node::Assert(a) => bind_fact(&mut a.pattern, args),
        Node::Retract(r) => bind_fact(&mut r.pattern, args),
        Node::Directive(d) => {
            bind_attrs(&mut d.attrs, args, params);
            true
        }
        Node::Match(m) => {
            bind_slot_raw(&mut m.subject, args, params);
            for arm in &mut m.arms {
                match arm {
                    Arm::When { test, body, .. } => {
                        bind_slot_raw(test, args, params);
                        bind_writes(body, args, params);
                    }
                    Arm::Otherwise { body, .. } => bind_writes(body, args, params),
                }
            }
            true
        }
        _ => true,
    });
}

/// Replace each nested `::use` left in a bound skeleton with ITS writes (a
/// presentational nested component contributes none).
fn expand_nested(
    nodes: &mut Vec<Node>,
    components: &ComponentSet,
    snapshot: &CapabilitySnapshot,
    stack: &mut Vec<String>,
) {
    let mut i = 0;
    while i < nodes.len() {
        if let Node::Directive(d) = &nodes[i] {
            if d.tag == "use" {
                let inner = use_writes(d, components, snapshot, stack);
                let n = inner.len();
                nodes.splice(i..=i, inner);
                i += n;
                continue;
            }
        }
        if let Node::Match(m) = &mut nodes[i] {
            for arm in &mut m.arms {
                match arm {
                    Arm::When { body, .. } | Arm::Otherwise { body, .. } => {
                        expand_nested(body, components, snapshot, stack)
                    }
                }
            }
        }
        i += 1;
    }
}

/// Anchor every span of a spliced write at the `::use` (the component
/// file's own offsets mean nothing in the host document).
fn respan(nodes: &mut [Node], at: Span) {
    let slot = |s: &mut CelSlot| s.span = at;
    let attrs = |attrs: &mut [Attr]| {
        for a in attrs {
            a.span = at;
            a.value_span = at;
            if let AttrValue::Ref(s) = &mut a.value {
                s.span = at;
            }
        }
    };
    for node in nodes {
        match node {
            Node::Set(s) => {
                s.span = at;
                s.path_span = at;
                slot(&mut s.expr);
                if let Some(w) = &mut s.when {
                    slot(w);
                }
            }
            Node::Assert(a) => {
                a.span = at;
                a.pattern_base = at.byte_start;
            }
            Node::Retract(r) => {
                r.span = at;
                r.pattern_base = at.byte_start;
            }
            Node::Directive(d) => {
                d.span = at;
                attrs(&mut d.attrs);
                if let Some(w) = &mut d.when {
                    slot(w);
                }
            }
            Node::Match(m) => {
                m.span = at;
                slot(&mut m.subject);
                attrs(&mut m.attrs);
                for arm in &mut m.arms {
                    match arm {
                        Arm::When {
                            is,
                            test,
                            attrs: a,
                            body,
                            span,
                        } => {
                            *span = at;
                            if let Some(p) = is {
                                p.span = at;
                            }
                            slot(test);
                            attrs(a);
                            respan(body, at);
                        }
                        Arm::Otherwise {
                            attrs: a,
                            body,
                            span,
                        } => {
                            *span = at;
                            attrs(a);
                            respan(body, at);
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cel_string_literal_escapes_quotes_and_backslashes() {
        assert_eq!(cel_string_literal("warm"), "'warm'");
        assert_eq!(cel_string_literal("it's"), "'it\\'s'");
        assert_eq!(cel_string_literal("a\\b"), "'a\\\\b'");
    }

    /// dsl 0.18.0: a range literal decides against a folded number constant
    /// by inclusive-bound membership, never against a non-number; a
    /// malformed/empty range (`E-WHEN-RANGE`) matches nothing.
    #[test]
    fn range_literal_matches_decided_numbers_inclusively() {
        let num = |n: f64| Decided::Num(n);
        assert!(is_literal_matches("1..3", &num(1.0)));
        assert!(is_literal_matches("1..3", &num(3.0)));
        assert!(!is_literal_matches("1..3", &num(3.5)));
        assert!(is_literal_matches("..0", &num(-1e9)));
        assert!(is_literal_matches("2..", &num(1e9)));
        assert!(!is_literal_matches("2..", &num(1.9)));
        assert!(!is_literal_matches("1..3", &Decided::Str("2".into())));
        assert!(!is_literal_matches("3..1", &num(2.0)));
        assert!(!is_literal_matches("1..2..3", &num(2.0)));
    }
}
