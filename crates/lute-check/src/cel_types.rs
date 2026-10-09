//! dsl 0.28.0 §1 (T1-4): comparisons are typed — `E-CEL-TYPE`.
//!
//! The runtime compares values of different types as unequal (`run.oil ==
//! true` never holds, `run.day == 'monday'` never holds) and orders only
//! numeric values (`run.hour >= 'h03'` and `visited('gallery') > 2` evaluate
//! unknown and halt play). This pass types each operand with the closed
//! procedure `::set` right-hand sides and def result types use
//! ([`crate::set_type`]) and reports, at the slot:
//!
//! - an `==` / `!=` / `in` whose two sides are of different types (a bool, an
//!   `int` or `double`, a string or enum member);
//! - an ordering (`<` `<=` `>` `>=`) with an operand that is not numeric;
//! - an `&&` / `||` / `!` / `?:` operand, or a whole condition, that is
//!   numeric or a string rather than a bool;
//! - arithmetic that cannot be computed (`run.flag + 1`).
//!
//! An operand the procedure cannot type is accepted, exactly as `E-SET-TYPE`
//! accepts it: a false report is worse than the silence this closes.
//! [`crate::decide::analyze_literal_comparisons`] treats a mistyped
//! comparison as the root of the dead guard it causes, so the guard is
//! reported once, here.

use cel_parser::ast::{operators as op, CallExpr, Expr};
use cel_parser::reference::Val;
use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_manifest::clock::ClockDecl;
use lute_manifest::types::Type;

use crate::cel_paths::select_path;
use crate::meta::StateSchema;
use crate::set_type::{decide, Decision, DefTypes};

/// What an operand is typed against.
pub(crate) struct Typing<'a> {
    pub schema: &'a StateSchema,
    pub defs: &'a DefTypes,
    /// The type of `$` (the `<match>` subject), when known.
    pub dollar: Option<Type>,
    /// The project's clock, for the weekday and slot hints.
    pub clock: Option<&'a ClockDecl>,
    /// Report arithmetic that cannot be computed. Off for a `::set`
    /// right-hand side, where `E-SET-TYPE` owns it.
    pub arithmetic: bool,
}

impl<'a> Typing<'a> {
    /// The typing of a slot checked in `ctx`: `$` typed by the enclosing
    /// `<match>` subject (a state path or a whole `@def`).
    pub(crate) fn of(ctx: &'a crate::Ctx<'a>, arithmetic: bool) -> Self {
        let env = ctx.env;
        let dollar =
            ctx.match_subject
                .as_deref()
                .map(str::trim)
                .and_then(|s| match s.strip_prefix('@') {
                    Some(def) => env.def_types.get(def).cloned(),
                    None => crate::set_op::resolve_type(s, &env.state).cloned(),
                });
        Typing {
            schema: &env.state,
            defs: &env.def_types,
            dollar,
            clock: env.clock.as_ref(),
            arithmetic,
        }
    }
}

/// The kinds of value a comparison can tell apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Family {
    Bool,
    Number,
    Text,
    Time,
}

fn family(t: &Type) -> Option<Family> {
    match t {
        Type::Bool => Some(Family::Bool),
        Type::Int | Type::Double => Some(Family::Number),
        Type::Str | Type::Enum(_) | Type::EnumFromOption(_) | Type::Domain(_) | Type::Entity(_) => {
            Some(Family::Text)
        }
        Type::NarrativeTime => Some(Family::Time),
        _ => None,
    }
}

/// How a message names a type.
fn describe(t: &Type) -> String {
    match t {
        Type::Bool => "a bool".to_string(),
        Type::Int => "an int".to_string(),
        Type::Double => "a double".to_string(),
        Type::Str => "a string".to_string(),
        Type::Enum(_) | Type::EnumFromOption(_) => "an enum member".to_string(),
        Type::Domain(d) | Type::Entity(d) => format!("a `{d}` member"),
        Type::NarrativeTime => "a narrative time".to_string(),
        _ => "a string".to_string(),
    }
}

impl Typing<'_> {
    /// The decided type of `e`; `None` when undecidable or ill-typed.
    fn ty(&self, e: &Expr) -> Option<Type> {
        if matches!(e, Expr::Ident(n) if n == "_") {
            return self.dollar.clone();
        }
        match decide(e, self.schema, self.defs) {
            Decision::Ty(t) => Some(t),
            _ => None,
        }
    }
}

/// Every type fault of the condition (or other slot) `expr`, at `span`.
/// `want_bool`: the slot is a condition, so its whole value must be a bool.
pub(crate) fn check_types(
    expr: &Expr,
    span: Span,
    want_bool: bool,
    t: &Typing<'_>,
    diags: &mut Vec<Diagnostic>,
) {
    if want_bool && !is_whole_ref_or_path(expr) {
        if let Some(ty) = t.ty(expr) {
            if matches!(family(&ty), Some(Family::Number | Family::Text)) {
                diags.push(diag(
                    format!(
                        "`{}` is {}, not a bool, so it never holds as a condition — compare it \
                         ({}) (dsl 0.28.0 §1)",
                        show(expr),
                        describe(&ty),
                        compare_example(&show(expr), &ty)
                    ),
                    span,
                ));
            }
        }
    }
    lute_cel::walk(expr, &mut |node| {
        let lute_cel::Node::Expr(expr) = node else {
            return lute_cel::Flow::Continue;
        };
        match expr {
            Expr::Call(c) => {
                check_call(expr, c, span, t, diags);
                lute_cel::Flow::Continue
            }
            Expr::List(_) | Expr::Select(_) => lute_cel::Flow::Continue,
            // The closed profile rejects these containers before nested typing
            // (docs/runtime/cel-and-facts.md:12-16).
            Expr::Comprehension(_) | Expr::Map(_) | Expr::Struct(_)
            | Expr::Ident(_) | Expr::Literal(_) | Expr::Unspecified => lute_cel::Flow::Skip,
        }
    });
}

/// A bare state path or `@def` is typed against a condition by `E-REF-TYPE`.
fn is_whole_ref_or_path(expr: &Expr) -> bool {
    match expr {
        Expr::Ident(n) => n != "_",
        Expr::Select(_) => select_path(expr).is_some(),
        Expr::Call(_) if crate::cel_paths::is_path_index(expr) => true,
        Expr::Call(c) => c.func_name.starts_with(lute_cel::REF_MARKER),
        _ => false,
    }
}


fn check_call(whole: &Expr, c: &CallExpr, span: Span, t: &Typing<'_>, diags: &mut Vec<Diagnostic>) {
    let name = c.func_name.as_str();
    match (name, c.args.as_slice()) {
        (op::EQUALS | op::NOT_EQUALS, [a, b]) => {
            let (Some(ta), Some(tb)) = (t.ty(&a.expr), t.ty(&b.expr)) else {
                return;
            };
            let (Some(fa), Some(fb)) = (family(&ta), family(&tb)) else {
                return;
            };
            if fa == Family::Number && fb == Family::Number && ta != tb {
                diags.push(diag(
                    format!(
                        "`{}` compares {} with {}, but int and double cannot be mixed (dsl 0.28.0 §1)",
                        show(whole), describe(&ta), describe(&tb)
                    ),
                    span,
                ));
                return;
            }
            if fa == fb || fa == Family::Time || fb == Family::Time {
                return;
            }
            let verdict = if name == op::EQUALS {
                "never true"
            } else {
                "always true"
            };
            let hint = equality_hint(&a.expr, &ta, &b.expr, name, t)
                .or_else(|| equality_hint(&b.expr, &tb, &a.expr, name, t))
                .unwrap_or_default();
            diags.push(diag(
                format!(
                    "`{}` compares {} with {}, so it is {verdict}{hint} (dsl 0.28.0 §1)",
                    show(whole),
                    describe(&ta),
                    describe(&tb)
                ),
                span,
            ));
        }
        (op::LESS | op::LESS_EQUALS | op::GREATER | op::GREATER_EQUALS, [a, b]) => {
            if let (Some(ta), Some(tb)) = (t.ty(&a.expr), t.ty(&b.expr)) {
                if family(&ta) == Some(Family::Number)
                    && family(&tb) == Some(Family::Number)
                    && ta != tb
                {
                    diags.push(diag(
                        format!(
                            "`{}` compares int with double, which cannot be mixed (dsl 0.28.0 §1)",
                            show(whole)
                        ),
                        span,
                    ));
                    return;
                }
            }
            let sides = [(&a.expr, &b.expr), (&b.expr, &a.expr)];
            let Some((bad, other, ty)) = sides.into_iter().find_map(|(x, y)| {
                let ty = t.ty(x)?;
                matches!(family(&ty), Some(Family::Bool | Family::Text)).then_some((x, y, ty))
            }) else {
                return;
            };
            let sym = op_symbol(name).unwrap_or(name);
            let first_is_bad = std::ptr::eq(bad, &a.expr);
            diags.push(diag(
                format!(
                    "`{}`: `{sym}` compares numbers (int/double), and `{}` is {}{} (dsl 0.28.0 §1)",
                    show(whole),
                    show(bad),
                    describe(&ty),
                    ordering_hint(bad, &ty, other, name, first_is_bad, t)
                ),
                span,
            ));
        }
        (op::IN, [needle, list]) => {
            let Expr::List(items) = &list.expr else {
                return;
            };
            let Some(tn) = t.ty(&needle.expr) else {
                return;
            };
            let Some(fn_) = family(&tn) else {
                return;
            };
            let Some((el, te)) = items.elements.iter().find_map(|el| {
                let te = t.ty(&el.expr)?;
                let fe = family(&te)?;
                ((fe != fn_ || (fn_ == Family::Number && te != tn))
                    && fe != Family::Time
                    && fn_ != Family::Time)
                    .then_some((el, te))
            }) else {
                return;
            };
            diags.push(diag(
                format!(
                    "`{}` looks for {} (`{}`) among {}, so that element never matches (dsl \
                     0.28.0 §1)",
                    show(whole),
                    describe(&tn),
                    show(&needle.expr),
                    match describe(&te).as_str() {
                        "a bool" => "bools".to_string(),
                        "a number" => "numeric values".to_string(),
                        _ => format!("{} like `{}`", describe(&te), show(&el.expr)),
                    }
                ),
                span,
            ));
        }
        (op::LOGICAL_AND | op::LOGICAL_OR, [_, _]) | (op::LOGICAL_NOT, [_]) => {
            let sym = op_symbol(name).unwrap_or(name);
            for a in &c.args {
                if let Some(ty) = t.ty(&a.expr) {
                    if matches!(family(&ty), Some(Family::Number | Family::Text)) {
                        diags.push(diag(
                            format!(
                                "`{sym}` takes conditions, and `{}` is {}, not a bool — compare \
                                 it ({}) (dsl 0.28.0 §1)",
                                show(&a.expr),
                                describe(&ty),
                                compare_example(&show(&a.expr), &ty)
                            ),
                            span,
                        ));
                    }
                }
            }
        }
        (op::CONDITIONAL, [cond, _, _]) => {
            if let Some(ty) = t.ty(&cond.expr) {
                if matches!(family(&ty), Some(Family::Number | Family::Text)) {
                    diags.push(diag(
                        format!(
                            "`?:` takes a condition before `?`, and `{}` is {}, not a bool — \
                             compare it ({}) (dsl 0.28.0 §1)",
                            show(&cond.expr),
                            describe(&ty),
                            compare_example(&show(&cond.expr), &ty)
                        ),
                        span,
                    ));
                }
            }
        }
        (op::ADD | op::SUBSTRACT | op::MULTIPLY | op::DIVIDE | op::NEGATE, _) if t.arithmetic => {
            // Report the innermost ill-typed arithmetic only.
            let inner_ill = c
                .args
                .iter()
                .any(|a| matches!(decide(&a.expr, t.schema, t.defs), Decision::Ill(_)));
            if inner_ill {
                return;
            }
            if let Decision::Ill(why) = decide(whole, t.schema, t.defs) {
                diags.push(diag(
                    format!(
                        "`{}` cannot be computed: {why} (dsl 0.28.0 §1)",
                        show(whole)
                    ),
                    span,
                ));
            }
        }
        _ => {}
    }
}

/// `path > 0` / `path != ''` / `path == 'x'` — how to turn a value into a
/// condition.
fn compare_example(shown: &str, ty: &Type) -> String {
    match ty {
        Type::Int | Type::Double => format!("for example `{shown} > 0`"),
        Type::Enum(members) if !members.is_empty() => {
            format!("for example `{shown} == '{}'`", members[0])
        }
        _ => format!("for example `{shown} != ''`"),
    }
}

/// Why `x` (of type `tx`) and `y` cannot be equal, when there is
/// more to say than their types — a fix the author can paste.
fn equality_hint(x: &Expr, tx: &Type, y: &Expr, name: &str, t: &Typing<'_>) -> Option<String> {
    let sym = op_symbol(name).unwrap_or(name);
    if let Some(scene) = visited_arg(x) {
        return Some(format!(" — {}", visited_note(&scene)));
    }
    let lit = match y {
        Expr::Literal(Val::String(s)) => Some(s.as_str()),
        _ => None,
    };
    let path = select_path(x);
    match (tx, lit) {
        (Type::Int | Type::Double, Some(s)) => {
            if let Some(clock) = t.clock {
                let labels = clock.week.as_ref().map(|w| w.labels.as_slice());
                // The label the literal names — as declared (`'monday'` is
                // `Monday`, `'Mon'` too), or as written when it names none.
                let label = labels
                    .and_then(|ls| {
                        let ls = || ls.iter().map(String::as_str);
                        ls().find(|l| *l == s)
                            .or_else(|| lute_manifest::suggest::nearest(s, ls(), 2))
                            .or_else(|| lute_manifest::suggest::abbreviated(s, ls()))
                    })
                    .unwrap_or(s);
                let index = labels.and_then(|ls| ls.iter().position(|l| l == label));
                let is_weekday = path.as_deref() == Some(lute_manifest::clock::CLOCK_WEEKDAY);
                let is_day = path.as_deref() == Some(clock.day.as_str())
                    || path.as_deref() == Some(lute_manifest::clock::CLOCK_DAY);
                if (is_weekday || is_day) && labels.is_some_and(|ls| !ls.is_empty()) {
                    let what = if is_weekday {
                        "the weekday's number (0 is the week's first label)".to_string()
                    } else {
                        "which day it is, counted from 1".to_string()
                    };
                    let or_number = match (is_weekday, index) {
                        (true, Some(i)) => format!(" or `clock.weekday {sym} {i}`"),
                        _ => String::new(),
                    };
                    return Some(format!(
                        " — `{}` is {what}; the weekday's name is `clock.weekdayLabel`: write \
                         `clock.weekdayLabel {sym} '{label}'`{or_number}",
                        path.unwrap_or_default()
                    ));
                }
            }
            if s.trim().parse::<f64>().is_ok() {
                if let Some(p) = path.as_deref().map(lute_manifest::text::bracket_spelling_of) {
                    return Some(format!(
                        " — write the number without quotes: `{p} {sym} {}`",
                        s.trim()
                    ));
                }
            }
            None
        }
        (Type::Bool, Some(s @ ("true" | "false"))) => path
            .map(|p| lute_manifest::text::bracket_spelling_of(&p))
            .map(|p| format!(" — write the bool without quotes: `{p} {sym} {s}`")),
        (Type::Bool, None) => {
            let p = lute_manifest::text::bracket_spelling_of(&path?);
            let is_num = matches!(
                y,
                Expr::Literal(Val::Int(_) | Val::UInt(_) | Val::Double(_))
            );
            is_num.then(|| {
                format!(" — a bool is `true` or `false`: test it directly (`{p}` or `!{p}`)")
            })
        }
        (Type::Enum(members), None) if !members.is_empty() => {
            let p = lute_manifest::text::bracket_spelling_of(&path?);
            Some(format!(" — `{p}` is one of {}", quoted(members)))
        }
        _ => None,
    }
}

/// What to write instead of ordering `bad` (of type `ty`) against `other`.
fn ordering_hint(
    bad: &Expr,
    ty: &Type,
    other: &Expr,
    name: &str,
    bad_is_left: bool,
    t: &Typing<'_>,
) -> String {
    if let Some(scene) = visited_arg(bad) {
        return format!(" — {}", visited_note(&scene));
    }
    let path = select_path(bad);
    match ty {
        Type::Bool => match path.as_deref() {
            Some(p) if p.starts_with("scene.visited.") => format!(
                " — `scene.visited.<hub>.<choice>` is whether that choice was ever taken, not \
                 how often; count picks in an `int` path you `::set` (for example \
                 `::set{{run.picks += 1}}`)"
            ),
            Some(p) => {
                let p = lute_manifest::text::bracket_spelling_of(p);
                format!(" — a bool is `true` or `false`: test it directly (`{p}` or `!{p}`)")
            }
            None => " — a bool is `true` or `false`, not a quantity".to_string(),
        },
        Type::Enum(members) => {
            let shown = path
                .as_deref()
                .map(lute_manifest::text::bracket_spelling_of)
                .unwrap_or_else(|| show(bad));
            let clock_slot = t.clock.and_then(|c| c.slot.as_deref());
            let is_slot = path
                .as_deref()
                .is_some_and(|p| Some(p) == clock_slot || p == lute_manifest::clock::CLOCK_SLOT);
            let position = if is_slot {
                "; or compare the clock's position, `clock.index`"
            } else {
                ""
            };
            match ordered_members(members, other, name, bad_is_left) {
                Some(picked) if !picked.is_empty() => format!(
                    " — enum members have no order; list the ones you mean: `{shown} in {}`\
                     {position}",
                    quoted(&picked)
                ),
                _ => format!(
                    " — enum members have no order; list the ones you mean with `{shown} in \
                     [...]`{position}"
                ),
            }
        }
        _ => " — strings have no order in a condition".to_string(),
    }
}

/// The members of `members` (in declaration order) the ordering `bad OP
/// other` would select if the enum's declaration order were its order —
/// `None` unless `other` is one of the members.
fn ordered_members(
    members: &[String],
    other: &Expr,
    name: &str,
    bad_is_left: bool,
) -> Option<Vec<String>> {
    let Expr::Literal(Val::String(lit)) = other else {
        return None;
    };
    let pivot = members.iter().position(|m| m == lit)?;
    // Normalise to `member OP pivot`.
    let name = if bad_is_left {
        name
    } else {
        match name {
            op::LESS => op::GREATER,
            op::LESS_EQUALS => op::GREATER_EQUALS,
            op::GREATER => op::LESS,
            op::GREATER_EQUALS => op::LESS_EQUALS,
            n => n,
        }
    };
    let keep = |i: usize| match name {
        op::LESS => i < pivot,
        op::LESS_EQUALS => i <= pivot,
        op::GREATER => i > pivot,
        op::GREATER_EQUALS => i >= pivot,
        _ => false,
    };
    Some(
        members
            .iter()
            .enumerate()
            .filter(|(i, _)| keep(*i))
            .map(|(_, m)| m.clone())
            .collect(),
    )
}

/// `['a', 'b']`.
fn quoted(members: &[String]) -> String {
    format!(
        "[{}]",
        members
            .iter()
            .map(|m| format!("'{m}'"))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn visited_arg(e: &Expr) -> Option<String> {
    let Expr::Call(c) = e else { return None };
    crate::cel_resolve::visited_call_target(c).map(str::to_string)
}

fn visited_note(scene: &str) -> String {
    format!(
        "`visited('{scene}')` is a bool, whether the scene was ever presented, not how often; \
         count visits in an `int` path you `::set` (for example `::set{{run.visits += 1}}`)"
    )
}

/// The authored spelling of a CEL operator's synthetic name.
fn op_symbol(name: &str) -> Option<&'static str> {
    Some(match name {
        op::EQUALS => "==",
        op::NOT_EQUALS => "!=",
        op::LESS => "<",
        op::LESS_EQUALS => "<=",
        op::GREATER => ">",
        op::GREATER_EQUALS => ">=",
        op::ADD => "+",
        op::SUBSTRACT => "-",
        op::MULTIPLY => "*",
        op::DIVIDE => "/",
        op::MODULO => "%",
        op::LOGICAL_AND => "&&",
        op::LOGICAL_OR => "||",
        op::LOGICAL_NOT => "!",
        op::IN => "in",
        _ => return None,
    })
}

/// A readable rendering of `e` for a message (the parse keeps no source
/// text): paths, literals, `@refs`, `$`, calls and operators.
pub(crate) fn show(e: &Expr) -> String {
    match e {
        Expr::Literal(v) => match v {
            Val::String(s) => format!("'{s}'"),
            Val::Int(i) => i.to_string(),
            Val::UInt(u) => u.to_string(),
            Val::Double(d) => d.to_string(),
            Val::Boolean(b) => b.to_string(),
            Val::Null => "null".to_string(),
            Val::Bytes(_) => "…".to_string(),
        },
        Expr::Ident(n) if n == "_" => "$".to_string(),
        Expr::Ident(n) => n.replace(lute_cel::REF_MARKER, "@"),
        Expr::Select(_) => select_path(e)
            .map(|p| lute_manifest::text::bracket_spelling_of(&p).replace(lute_cel::REF_MARKER, "@"))
            .unwrap_or_else(|| "…".to_string()),
        Expr::Call(_) if crate::cel_paths::is_path_index(e) => select_path(e)
            .map(|p| lute_manifest::text::bracket_spelling_of(&p))
            .unwrap_or_default(),
        Expr::Call(c) if c.func_name == op::INDEX && c.args.len() == 2 => {
            format!(
                "{}[{}]",
                show_operand(&c.args[0].expr),
                show(&c.args[1].expr)
            )
        }
        Expr::List(l) => format!(
            "[{}]",
            l.elements
                .iter()
                .map(|x| show(&x.expr))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Expr::Call(c) => match (op_symbol(&c.func_name), c.args.as_slice()) {
            (Some(sym), [a]) => format!("{sym}{}", show_operand(&a.expr)),
            (Some(sym), [a, b]) => {
                format!("{} {sym} {}", show_operand(&a.expr), show_operand(&b.expr))
            }
            _ if c.func_name == op::NEGATE && c.args.len() == 1 => {
                format!("-{}", show_operand(&c.args[0].expr))
            }
            _ => format!(
                "{}({})",
                c.func_name.replace(lute_cel::REF_MARKER, "@"),
                c.args
                    .iter()
                    .map(|a| show(&a.expr))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        },
        _ => "…".to_string(),
    }
}

/// [`show`], parenthesized when `e` is itself an operator expression.
fn show_operand(e: &Expr) -> String {
    match e {
        Expr::Call(c) if op_symbol(&c.func_name).is_some() || c.func_name == op::NEGATE => {
            format!("({})", show(e))
        }
        _ => show(e),
    }
}

fn diag(message: String, span: Span) -> Diagnostic {
    Diagnostic {
        code: crate::cel_resolve::E_CEL_TYPE.to_string(),
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
