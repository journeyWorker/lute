use super::*;
use super::{core::*, rules::*, facts::*, state::*};
pub(crate) fn check_quest_state_isset(expr: &Expr, span: Span, diags: &mut Vec<Diagnostic>) {
    lute_cel::walk(expr, &mut |node| {
        let lute_cel::Node::Expr(expr) = node else {
            return lute_cel::Flow::Continue;
        };
        match expr {
            Expr::Select(sel) if sel.test => {
                if let Some(path) = crate::cel_paths::select_path(expr)
                    .filter(|p| crate::cel_paths::is_reserved_quest_state(p))
                {
                    diags.push(Diagnostic {
                        severity: Severity::Warning,
                        ..diag(
                            W_QUEST_STATE_HAS,
                            format!(
                                "`has({path})` is always true: a quest's state is always \
                                 assigned — `unset` until the quest activates, then `active`, \
                                 `complete` or `failed`. Test `{path} == 'unset'` (or \
                                 `!= 'unset'`) instead"
                            ),
                            span,
                        )
                    });
                }
                lute_cel::Flow::Continue
            }
            Expr::Call(_) | Expr::Select(_) => lute_cel::Flow::Continue,
            Expr::List(_) => lute_cel::Flow::Continue,
            Expr::Comprehension(_) | Expr::Map(_) | Expr::Struct(_)
            | Expr::Ident(_) | Expr::Literal(_) | Expr::Unspecified => lute_cel::Flow::Skip,
        }
    });
}

/// The [`GUARD_FIREWALL_CALLS`] walk (D7): a `Call` (with or without a
/// receiver — matched by name alone) reaches [`E_DATALOG_GUARD_FACT`] and
/// stops descending (the whole call is rejected, mirroring
/// [`check_cel_profile`]'s own stop-on-reject shape); everything else
/// recurses the same way `check_cel_profile` does.
pub(crate) fn check_guard_fact_access(expr: &Expr, span: Span, diags: &mut Vec<Diagnostic>) {
    lute_cel::walk(expr, &mut |node| {
        let lute_cel::Node::Expr(expr) = node else {
            return lute_cel::Flow::Continue;
        };
        match expr {
            Expr::Call(c) => {
                let name = c.func_name.as_str();
                if GUARD_FIREWALL_CALLS.contains(&name) {
                    diags.push(diag(
                        E_DATALOG_GUARD_FACT,
                        format!(
                            "`{name}(…)` reads the fact store or narrative time inside a rule guard; \
                             rules have no access to time or facts (dsl 0.3.0 §9.3, D7)"
                        ),
                        span,
                    ));
                    lute_cel::Flow::Skip
                } else {
                    lute_cel::Flow::Continue
                }
            }
            Expr::List(_) | Expr::Select(_) => lute_cel::Flow::Continue,
            Expr::Comprehension(_) | Expr::Map(_) | Expr::Struct(_)
            | Expr::Ident(_) | Expr::Literal(_) | Expr::Unspecified => lute_cel::Flow::Skip,
        }
    });
}

/// The Lute-CEL profile gate (dsl §8.4). The environment is **closed**: host
/// calls use their dedicated list-form signatures (`holds('rel', [args])`,
/// `count('rel', [args])`, and the other declared host calls). Everything else
/// the profile permits — a fixed set of CEL operators, literals, list literals,
/// the `in` membership operator, and the ternary conditional — is *not* a
/// user-callable function. Any other function/method call (`size`, `matches`,
/// `startsWith`, …) or comprehension macro (`map`, `filter`, `exists`, `all`,
/// `existsOne`) is a static error ([`E_CEL_PROFILE`]) at the slot span.
///
/// Runs over the **marker re-parse** (`parse_slot_marked_refs`), where each DSL
/// `@ref` sigil was rewritten to [`lute_cel::REF_MARKER`]. That distinction is
/// load-bearing:
/// * a compile-time `@name(args)` reference (dsl §8.1) parses to a `Call` whose
///   `func_name` starts with `REF_MARKER` — exempt, but we still recurse into its
///   args so a nested out-of-profile call is caught (`@pick(size(x))` flags
///   `size`). A same-named *runtime* call keeps its bare name and is NOT exempt,
///   closing the `@gate && gate(x)` bypass.
/// * CEL lowers operators to synthetic `Call` names ([`is_profile_operator`]),
///   matched against an EXPLICIT allow-list — leading-dot global calls and the
///   optional operators are therefore rejected, not blanket-accepted.
/// * the valid `has(path)` macro parses as a test-only [`Expr::Select`], never a
///   `Call` — so a residual `Call` named `has` (`has(x,y)`, `x.has()`) is NOT the
///   macro and IS rejected. Host calls are recognized separately by their
///   list-form signatures.
/// * comprehension macros lower to [`Expr::Comprehension`]; map/struct literals
///   to [`Expr::Map`]/[`Expr::Struct`] (only *list* literals are in profile) —
///   all rejected.
/// * a `holds`/`count`/`validAt`/`now` fact-query/narrative-time call
///   ([`is_profile_fact_query`], dsl 0.3.0 §6/§8, T11) is exempt but does
///   NOT get the ordinary structural recursion: the pattern arg (`holds`/
///   `count`'s sole arg, `validAt`'s first arg) is a relation `Call`, not a
///   CEL sub-expression — its bare idents would otherwise trip
///   [`is_profile_ident_root`] below. Only `validAt`'s SECOND arg (a genuine
///   CEL expr, e.g. `now()`) is recursed into; the pattern itself is
///   validated by [`check_fact_queries`] instead.
///
/// `scope` names the extra bare identifiers in scope and what the hints
/// consult ([`ProfileScope`]).
pub(crate) fn check_cel_profile(
    expr: &Expr,
    slot: &CelSlot,
    scope: &ProfileScope<'_>,
    diags: &mut Vec<Diagnostic>,
) {
    match expr {
        Expr::Call(c) => {
            let name = c.func_name.as_str();
            let visited =
                slot.kind == CelKind::Condition && visited_call_target(c).is_some();
            if name.starts_with(lute_cel::REF_MARKER)
                || is_profile_operator(name)
                || is_fact_query_name(c)
                || is_profile_fact_query(c)
                || is_profile_conversion(c)
                || visited
            {
                if visited {
                } else if is_fact_query_name(c) {
                    if c.func_name == "validAt" && is_profile_fact_query(c) {
                        if let Some(t) = c.args.get(2) {
                            check_cel_profile(&t.expr, slot, scope, diags);
                        }
                    }
                } else if is_profile_conversion(c) {
                    for a in &c.args {
                        check_cel_profile(&a.expr, slot, scope, diags);
                    }
                } else {
                    if let Some(t) = &c.target {
                        check_cel_profile(&t.expr, slot, scope, diags);
                    }
                    for a in &c.args {
                        check_cel_profile(&a.expr, slot, scope, diags);
                    }
                }
            } else {
                let hint = if name == VISITED_FN && slot.kind != CelKind::Condition {
                    " — `visited(…)` is only legal in a condition slot".to_string()
                } else {
                    call_hint(c, scope)
                };
                diags.push(diag(
                    E_CEL_PROFILE,
                    format!(
                        "`{name}(…)` is outside the Lute-CEL profile{} — the profile has \
                         operators, int/double/bool/string literals, lists, `?:`, `in`, \
                         `has()`, `int()`, `double()`, `holds()`, `count()`, \
                         `countDistinct(string, list, int)`, `validAt()`, `now()`, and `visited(string)`",
                        hint
                    ),
                    slot.span,
                ));
            }
        }
        Expr::Comprehension(_) => diags.push(diag(
            E_CEL_PROFILE,
            "comprehension macros (map/filter/exists/all/existsOne) are outside the \
             Lute-CEL profile — only operators, literals, lists, `?:`, `in`, \
             `has()`, `int()`, and `double()` are permitted (dsl §8.4)"
                .to_string(),
            slot.span,
        )),
        Expr::Map(_) | Expr::Struct(_) => diags.push(diag(
            E_CEL_PROFILE,
            "map/struct literals are outside the Lute-CEL profile — only list \
             literals are permitted (dsl §8.4)"
                .to_string(),
            slot.span,
        )),
        Expr::List(list) => {
            for el in &list.elements {
                check_cel_profile(&el.expr, slot, scope, diags);
            }
        }
        Expr::Select(sel) => check_cel_profile(&sel.operand.expr, slot, scope, diags),
        Expr::Ident(name) => {
            if !is_profile_ident_root(name) && !scope.bound.iter().any(|b| b == name) {
                let message = match name.strip_prefix('_').filter(|n| !n.is_empty()) {
                    Some(bare) => format!(
                        "`${bare}`: a state path takes no `$` (`$` alone is the `<match>` \
                         subject) — write the path with its tier{}",
                        scope.path_ending_in(bare).map_or_else(
                            || format!(", such as `run.{bare}`"),
                            |p| format!(" — did you mean `{p}`?")
                        )
                    ),
                    None if matches!(name.as_str(), "once" | "always")
                        && slot.raw.trim() == name.as_str() => {
                        format!(
                            "`{name}` is not a condition: how often a beat plays is its own key, \
                             `once` — `once: run` (once per run, the default), `once: user` \
                             (once ever) or `once: false` (every time); on a `<beat>` or \
                             `<entry>` it is `once=\"run\"` — drop this `when`"
                        )
                    }
                    None => format!(
                        "`{name}` is not a state path (`scene`/`run`/`user`/`app`), the match \
                         subject `$`, or a def `@ref` — {}",
                        scope.path_ending_in(name).map_or_else(
                            || format!(
                                "a string is quoted (`'{name}'`), and a `<when>` compares its \
                                 `<match>` subject with `is=\"{name}\"`"
                            ),
                            |p| format!("did you mean `{p}`?")
                        )
                    ),
                };
                diags.push(diag(E_CEL_PROFILE, message, slot.span));
            }
        }
        Expr::Literal(Val::UInt(_) | Val::Bytes(_) | Val::Null) => diags.push(diag(
            E_CEL_PROFILE,
            "uint, bytes, and null literals are outside the Lute-CEL profile".to_string(),
            slot.span,
        )),
        Expr::Literal(Val::Int(n)) if n.unsigned_abs() > (1_u64 << 53) => diags.push(diag(
            E_CEL_PROFILE,
            "int literals must be within ±2^53".to_string(),
            slot.span,
        )),
        Expr::Literal(_) | Expr::Unspecified => {}
    }
}

/// What [`check_cel_profile`] admits beyond the profile's roots — a def
/// body's own `params:` (0.21.1 T1-6), empty for every ordinary slot — and
/// what its hints consult (dsl 0.28.0, T3-4): the relations a bare atom may
/// name, and the declared paths a bare name may be the tail of.
pub(crate) struct ProfileScope<'a> {
    pub(crate) bound: &'a [String],
    pub(crate) relations:
        Option<&'a std::collections::BTreeMap<String, lute_manifest::relations::RelationDecl>>,
    pub(crate) schema: Option<&'a crate::meta::StateSchema>,
}

impl<'a> ProfileScope<'a> {
    pub(crate) fn of(ctx: &'a Ctx<'_>) -> Self {
        ProfileScope {
            bound: &[],
            relations: Some(&ctx.env.rel_vocab.relations),
            schema: Some(&ctx.env.state),
        }
    }

    /// The first declared path whose last segment is `name` (any case).
    fn path_ending_in(&self, name: &str) -> Option<&'a str> {
        self.schema?.decls.keys().map(String::as_str).find(|k| {
            !crate::cel_paths::is_engine_owned_path(k)
                && k.rsplit_once('.')
                    .is_some_and(|(_, last)| last.eq_ignore_ascii_case(name))
        })
    }
}

/// dsl 0.28.0 (T3-4): what an out-of-profile call was likely meant as — a
/// relation atom asked with the 0.32 list form, `isSet` replaced by `has`/`in`,
/// `completed()` / `active()` read as the quest's state, or an unquoted
/// `visited()` id.
pub(crate) fn call_hint(c: &cel_parser::ast::CallExpr, scope: &ProfileScope<'_>) -> String {
    let name = c.func_name.as_str();
    if name.eq_ignore_ascii_case("isSet") {
        return isset_hint(c);
    }
    if matches!(name, "holds" | "count" | "countDistinct" | "validAt") {
        if let Some(hint) = legacy_fact_hint(c) {
            return hint;
        }
    }
    if c.target.is_none() && scope.relations.is_some_and(|r| r.contains_key(name)) {
        let args = legacy_atom_args(c);
        return format!(
            " — `{name}` is a relation, and a fact is asked about with \
             `holds('{name}', [{}])`",
            args.join(", ")
        );
    }
    let id = match c.args.as_slice() {
        [a] if c.target.is_none() => match &a.expr {
            Expr::Literal(Val::String(s)) => Some(s.clone()),
            e => crate::cel_paths::select_path(e),
        },
        _ => None,
    };
    let meant = match name {
        "completed" | "active" | VISITED_FN => Some(name),
        _ if c.target.is_none() => {
            lute_manifest::suggest::nearest(name, ["completed", "active", VISITED_FN], 2)
        }
        _ => None,
    };
    let not = if meant == Some(name) {
        String::new()
    } else {
        format!(" (not `{name}`)")
    };
    match (meant, id) {
        (Some(m @ ("completed" | "active")), Some(id)) => format!(
            " — `{m}()`{not} belongs to `after:`; a condition reads the quest's state: \
             `quest.{id}.state == '{}'`",
            if m == "completed" { "complete" } else { "active" }
        ),
        (Some(VISITED_FN), Some(id)) if meant == Some(name) => {
            format!(" — the scene id is quoted: `visited('{id}')`")
        }
        (Some(VISITED_FN), Some(id)) => format!(" — did you mean `visited('{id}')`?"),
        _ => String::new(),
    }
}

pub(crate) fn isset_hint(c: &cel_parser::ast::CallExpr) -> String {
    let Some(arg) = c.args.first().map(|a| &a.expr) else {
        return " — `isSet` was removed; use `has(a.b)` or `'k' in a.b`".to_string();
    };
    if let Expr::Call(index) = arg {
        use cel_parser::ast::operators as op;
        if index.func_name == op::INDEX && index.args.len() == 2 {
            if let (Some(path), Expr::Literal(Val::String(key))) = (
                crate::cel_paths::select_path(&index.args[0].expr),
                &index.args[1].expr,
            ) {
                return format!(
                    " — `isSet` was removed; use `'{key}' in {path}`"
                );
            }
        }
    }
    let path = crate::cel_paths::select_path(arg).unwrap_or_else(|| "a.b".to_string());
    format!(" — `isSet` was removed; use `has({path})` or `'k' in a.b`")
}

pub(crate) fn legacy_fact_hint(c: &cel_parser::ast::CallExpr) -> Option<String> {
    let pattern = match &c.args.first()?.expr {
        Expr::Call(atom) => atom,
        _ => return None,
    };
    let args = legacy_atom_args(pattern).join(", ");
    let relation = &pattern.func_name;
    let suffix = match c.func_name.as_str() {
        "validAt" => c
            .args
            .get(1)
            .map(|arg| format!(", {}", crate::cel_types::show(&arg.expr)))
            .unwrap_or_default(),
        "countDistinct" => {
            let column = c.args.get(1).and_then(|arg| match &arg.expr {
                Expr::Ident(name) => pattern.args.iter().position(|p| {
                    matches!(&p.expr, Expr::Ident(candidate) if candidate == name)
                }),
                _ => None,
            });
            format!(", {}", column.unwrap_or(0))
        }
        _ => String::new(),
    };
    Some(format!(
        " — use `{}('{relation}', [{}]{suffix})`",
        c.func_name, args
    ))
}

pub(crate) fn legacy_atom_args(c: &cel_parser::ast::CallExpr) -> Vec<String> {
    c.args
        .iter()
        .map(|arg| match &arg.expr {
            Expr::Ident(name)
                if name == "_" || name.chars().next().is_some_and(char::is_uppercase) =>
            {
                "'_'".to_string()
            }
            Expr::Ident(name) => format!("'{name}'"),
            Expr::Literal(Val::Boolean(value)) => value.to_string(),
            Expr::Literal(Val::String(value)) => format!("'{value}'"),
            expr => crate::cel_types::show(expr),
        })
        .collect()
}

/// True when `func_name` is one of the CEL built-in **operators** the Lute-CEL
/// profile permits (dsl §8.4). cel-parser 0.10.1 lowers each operator to a fixed
/// synthetic name; we match that EXACT allow-list (via `cel_parser::ast::operators`
/// constants) so out-of-profile operators are NOT accepted just for being
/// punctuated. Deliberately EXCLUDED: the optional operators `_[?_]`/`_?._`
/// and the internal `@not_strictly_false`. Integer `%` (`_%_`) is in since
/// dsl 0.24.0 §1; its operand typing is [`check_modulo_operands`].
pub(crate) fn is_profile_operator(func_name: &str) -> bool {
    use cel_parser::ast::operators as op;
    // The profile's operators: `? :`, `&& || !`, `+ - * / %`, `== != >= <= > <`,
    // unary `-`, index `[]`, and `in`. EXCLUDES the optional operators and the
    // internal `@not_strictly_false`.
    const ALLOWED: &[&str] = &[
        op::CONDITIONAL,
        op::LOGICAL_AND,
        op::LOGICAL_OR,
        op::LOGICAL_NOT,
        op::ADD,
        op::SUBSTRACT,
        op::MULTIPLY,
        op::DIVIDE,
        op::MODULO,
        op::EQUALS,
        op::NOT_EQUALS,
        op::GREATER_EQUALS,
        op::LESS_EQUALS,
        op::GREATER,
        op::LESS,
        op::NEGATE,
        op::INDEX,
        op::IN,
    ];
    ALLOWED.contains(&func_name)
}

/// dsl 0.24.0 §1: both operands of every `%` in `expr` must be integers —
/// [`E_CEL_TYPE`] at `span` for a non-`number` operand or a fractional
/// literal ([`crate::set_type::modulo_operand_fault`], the same typing
/// `::set` right-hand sides and def result types use). Walks every
/// sub-expression, so a `%` nested in a def-ref argument or another `%` is
/// checked too.
pub(crate) fn check_modulo_operands(
    expr: &Expr,
    span: Span,
    schema: &crate::meta::StateSchema,
    diags: &mut Vec<Diagnostic>,
) {
    match expr {
        Expr::Call(c) => {
            if c.func_name == cel_parser::ast::operators::MODULO {
                for a in &c.args {
                    if let Some(why) = crate::set_type::modulo_operand_fault(&a.expr, schema) {
                        diags.push(diag(
                            E_CEL_TYPE,
                            format!("`%` takes two integers: {why} (dsl 0.24.0 §1)"),
                            span,
                        ));
                    }
                }
            }
            if let Some(t) = &c.target {
                check_modulo_operands(&t.expr, span, schema, diags);
            }
            for a in &c.args {
                check_modulo_operands(&a.expr, span, schema, diags);
            }
        }
        Expr::List(list) => {
            for el in &list.elements {
                check_modulo_operands(&el.expr, span, schema, diags);
            }
        }
        Expr::Select(sel) => check_modulo_operands(&sel.operand.expr, span, schema, diags),
        _ => {}
    }
}

/// Standard CEL numeric conversions, with one argument and no receiver.
pub(crate) fn is_profile_conversion(c: &cel_parser::ast::CallExpr) -> bool {
    matches!(c.func_name.as_str(), "int" | "double")
        && c.target.is_none()
        && c.args.len() == 1
}

/// The Lute-CEL name of the presentation-history query (dsl 0.21.0 §7a.1).
pub const VISITED_FN: &str = "visited";

/// The scene id of an in-profile `visited('<scene id>')` call (dsl 0.21.0
/// §7a.1): named exactly `visited`, NO receiver, exactly one argument, and
/// that argument a string literal. `None` for anything else — a malformed
/// `visited(…)` (`visited(scene.x)`, `visited()`, `x.visited('a')`) is an
/// ordinary out-of-profile call ([`E_CEL_PROFILE`]).
///
/// `visited` is not a state path: it reads the save's presentation history
/// (the tier `after:` uses, never cleared by `newRun`), so definite
/// assignment and the unset-sentinel rules do not apply to it, and `decide`
/// treats it as an undecided atom. Its id resolves against the project's
/// scene keys at `check-project` (`E-CONN-UNKNOWN-NODE`,
/// [`crate::connectivity::resolve_nodes`]).
pub fn visited_call_target(c: &cel_parser::ast::CallExpr) -> Option<&str> {
    if c.func_name != VISITED_FN || c.target.is_some() || c.args.len() != 1 {
        return None;
    }
    match &c.args[0].expr {
        Expr::Literal(Val::String(s)) => Some(s.as_str()),
        _ => None,
    }
}

/// Every in-profile `visited('<scene id>')` target in `expr`, in source
/// order (dsl 0.21.0 §7a.1). Walks every sub-expression, including the
/// arguments of other calls.
pub fn visited_targets(expr: &Expr) -> Vec<String> {
    let mut out = Vec::new();
    lute_cel::walk(expr, &mut |node| {
        let lute_cel::Node::Expr(expr) = node else {
            return lute_cel::Flow::Continue;
        };
        match expr {
            Expr::Call(c) => {
                if let Some(id) = visited_call_target(c) {
                    out.push(id.to_string());
                    lute_cel::Flow::Skip
                } else {
                    lute_cel::Flow::Continue
                }
            }
            Expr::List(_) | Expr::Select(_) => lute_cel::Flow::Continue,
            Expr::Comprehension(_) | Expr::Map(_) | Expr::Struct(_)
            | Expr::Ident(_) | Expr::Literal(_) | Expr::Unspecified => lute_cel::Flow::Skip,
        }
    });
    out
}

/// Whether `c` names a host fact-query function, regardless of its shape.
/// Shape validation belongs to [`check_fact_query_call`] so malformed queries
/// receive `E-FACT-QUERY`, not the generic profile error.
pub(crate) fn is_fact_query_name(c: &cel_parser::ast::CallExpr) -> bool {
    c.target.is_none()
        && matches!(
            c.func_name.as_str(),
            "holds" | "count" | "countDistinct" | "validAt"
        )
}

/// A list-form host query. Argument contents and the numeric column/time
/// types are checked separately so malformed queries retain their own code.
pub(crate) fn is_profile_fact_query(c: &cel_parser::ast::CallExpr) -> bool {
    if !is_fact_query_name(c) {
        return c.target.is_none() && c.func_name == "now" && c.args.is_empty();
    }
    match c.func_name.as_str() {
        "holds" | "count" => c.args.len() == 2,
        "countDistinct" | "validAt" => c.args.len() == 3,
        _ => false,
    }
}

pub(crate) fn query_relation(c: &cel_parser::ast::CallExpr) -> Option<&str> {
    match &c.args.first()?.expr {
        Expr::Literal(Val::String(name)) => Some(name),
        _ => None,
    }
}

/// The zero-based counted column must be a wildcard in the query list.
pub(crate) fn count_distinct_column(c: &cel_parser::ast::CallExpr) -> Option<usize> {
    if c.target.is_some() || c.func_name != "countDistinct" || c.args.len() != 3 {
        return None;
    }
    let (Expr::List(list), Expr::Literal(Val::Int(column))) =
        (&c.args[1].expr, &c.args[2].expr) else {
        return None;
    };
    let column = usize::try_from(*column).ok()?;
    matches!(&list.elements.get(column)?.expr, Expr::Literal(Val::String(s)) if s == "_")
        .then_some(column)
}

/// Vocabulary-aware fact-query pass (dsl 0.3.0 §6/§8, T11): validates every
/// `holds`/`count`/`validAt` pattern against `ctx.env.rel_vocab`. Mirrors
/// [`check_cel_profile`]'s own recursion shape (own recursion into `Call`
/// target/args, `List` elements, `Select` operand; leaves are inert) so a
/// fact query nested inside an operator call (`count(x) + 1 <= 3`) or a
/// list is still found. Runs on the SAME marker re-parse as the profile
/// gate — called from `check_cel_slot` right after `check_cel_profile`, so a
/// malformed (non-admitted) fact-query shape is already `E_CEL_PROFILE`-
/// flagged there and is left alone here (an ordinary `Call` with no relation

const _: () = ();
