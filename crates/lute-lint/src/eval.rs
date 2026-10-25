//! Lint CEL fragment (spec §5).
//!
//! Parses `when` and `message` expressions with [`lute_cel::parse_slot`],
//! then walks the resulting AST as a GROUND evaluator over metric rows and
//! `options`. Scalar operators reuse [`lute_check::apply_op`]'s R3 ground-op
//! semantics — the SAME table the compile-time §6.4 fold and `lute-trace`
//! share — so a `+`/`==`/`<` never diverges between the checker and the
//! linter. Non-scalar shapes (list literals, `size(x)`, `x[y]` map indexing)
//! are handled here directly because [`apply_op`] only speaks in
//! [`lute_check::Decided`] scalars.
//!
//! Everything is ground: an unresolvable field, a type mismatch, a wrong
//! arity, or a non-`bool` `when` result is a structured [`EvalError`]. The
//! rule engine turns each into a single `E-LINT-EXPR` diagnostic anchored
//! to the rule's declaration site, then skips the rule (spec §5).
//!
//! **By reference** (dsl 0.50.0 §8.1 "Cost"). A select/index chain rooted at
//! a binding resolves to a [`Ref`] into the bound value: reading
//! `node.attrs["x"]` copies neither the row nor its attribute map. A binding
//! may also be a [`Record`], a host structure that answers one field at a
//! time (the `node` row and its `preceding` scope); it becomes a [`Value`]
//! only when a rule reads it whole (`size(node.preceding)`, comparing or
//! rendering it).
//!
//! [`apply_op`]: lute_check::apply_op

use std::borrow::Cow;
use std::collections::BTreeMap;

use cel_parser::ast::{operators as op, CallExpr, Expr, IdedExpr};
use cel_parser::reference::Val;

/// A ground value the evaluator produces or consumes. Distinct from
/// [`lute_check::Decided`] because the lint fragment is a superset —
/// metrics carry lists (attr keys, project spread) and maps
/// (`speaker.axis`, `speaker.attrShare`, `line.attrs`, `options`) that
/// scalar-only `Decided` cannot represent.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Double(f64),
    Str(String),
    List(Vec<Value>),
    Map(BTreeMap<String, Value>),
}

impl Value {
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }
    pub fn as_num(&self) -> Option<f64> {
        match self {
            Value::Int(n) => Some(*n as f64),
            Value::Double(n) => Some(*n),
            _ => None,
        }
    }
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s.as_str()),
            _ => None,
        }
    }
    fn type_name(&self) -> &'static str {
        match self {
            Value::Null => "null",
            Value::Bool(_) => "bool",
            Value::Int(_) => "int",
            Value::Double(_) => "double",
            Value::Str(_) => "string",
            Value::List(_) => "list",
            Value::Map(_) => "map",
        }
    }
}

/// A map-shaped structure the evaluator reads one field at a time. Its
/// fields are never copied by a select, an index or an `in` test; only
/// [`Record::materialize`] builds the whole map, for a rule that reads it
/// whole.
pub trait Record {
    /// The field `key`, or `None` when the record has no such key.
    fn field(&self, key: &str) -> Option<Ref<'_>>;
    /// Every field, as one map value.
    fn materialize(&self) -> Value;
}

/// What an expression resolves to: a value borrowed from a binding, a value
/// computed by the expression, or a [`Record`].
pub enum Ref<'a> {
    Borrowed(&'a Value),
    Owned(Value),
    Record(&'a dyn Record),
}

impl std::fmt::Debug for Ref<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Ref::Borrowed(v) => f.debug_tuple("Borrowed").field(v).finish(),
            Ref::Owned(v) => f.debug_tuple("Owned").field(v).finish(),
            Ref::Record(_) => f.write_str("Record(..)"),
        }
    }
}

impl Ref<'_> {
    /// The same reference, borrowed from `self`.
    fn reborrow(&self) -> Ref<'_> {
        match self {
            Ref::Borrowed(v) => Ref::Borrowed(v),
            Ref::Owned(v) => Ref::Borrowed(v),
            Ref::Record(r) => Ref::Record(*r),
        }
    }

    /// The value itself; a [`Record`] is materialized.
    fn as_value(&self) -> Cow<'_, Value> {
        match self {
            Ref::Borrowed(v) => Cow::Borrowed(v),
            Ref::Owned(v) => Cow::Borrowed(v),
            Ref::Record(r) => Cow::Owned(r.materialize()),
        }
    }

    fn into_value(self) -> Value {
        match self {
            Ref::Borrowed(v) => v.clone(),
            Ref::Owned(v) => v,
            Ref::Record(r) => r.materialize(),
        }
    }

    /// The value, unless this is a [`Record`].
    fn value(&self) -> Option<&Value> {
        match self {
            Ref::Borrowed(v) => Some(v),
            Ref::Owned(v) => Some(v),
            Ref::Record(_) => None,
        }
    }

    fn type_name(&self) -> &'static str {
        self.value().map_or("map", Value::type_name)
    }
}

/// A binding table for the target-row env (`line`, `section`, `scene`,
/// `speaker`, `group`, `project`, `node`, `options`). Undeclared bindings
/// resolve to a missing-field error the moment they're read. A binding is
/// owned, borrowed (a shared `options` map is bound once per rule, not
/// copied per row) or a [`Record`].
#[derive(Debug, Default)]
pub struct Env<'a> {
    bindings: Vec<(&'a str, Ref<'a>)>,
}

impl<'a> Env<'a> {
    pub fn new() -> Self {
        Self::default()
    }
    /// Bind `name` to an owned value.
    pub fn with(mut self, name: &'a str, v: Value) -> Self {
        self.bind(name, v);
        self
    }
    /// Bind `name` to a borrowed value.
    pub fn with_ref(mut self, name: &'a str, v: &'a Value) -> Self {
        self.put(name, Ref::Borrowed(v));
        self
    }
    /// Bind `name` to a [`Record`].
    pub fn with_record(mut self, name: &'a str, r: &'a dyn Record) -> Self {
        self.put(name, Ref::Record(r));
        self
    }
    pub fn bind(&mut self, name: &'a str, v: Value) {
        self.put(name, Ref::Owned(v));
    }
    fn put(&mut self, name: &'a str, r: Ref<'a>) {
        match self.bindings.iter_mut().find(|(n, _)| *n == name) {
            Some(slot) => slot.1 = r,
            None => self.bindings.push((name, r)),
        }
    }
    /// The binding `name`, by reference.
    pub fn get(&self, name: &str) -> Option<Ref<'_>> {
        self.bindings
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, r)| r.reborrow())
    }
}

/// A ground evaluation failure. `path` is the dotted trail into the row
/// (`speaker.axis.emotion.run`) that triggered the failure — used by the
/// engine to produce a spec-shaped `E-LINT-EXPR` message ("path X: reason").
#[derive(Clone, Debug, PartialEq)]
pub struct EvalError {
    pub message: String,
}

impl EvalError {
    pub fn new(msg: impl Into<String>) -> Self {
        Self {
            message: msg.into(),
        }
    }
}

/// The reason alone; the engine adds the row path for `E-LINT-EXPR`.
impl std::fmt::Display for EvalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for EvalError {}

/// Evaluate a top-level CEL expression under `env`.
pub fn eval(expr: &Expr, env: &Env<'_>) -> Result<Value, EvalError> {
    resolve(expr, env).map(Ref::into_value)
}

/// Evaluate `expr`, keeping a select/index chain as a reference into `env`.
fn resolve<'e>(expr: &Expr, env: &'e Env<'_>) -> Result<Ref<'e>, EvalError> {
    match expr {
        Expr::Literal(v) => literal_value(v).map(Ref::Owned),
        Expr::Ident(name) => env
            .get(name)
            .ok_or_else(|| EvalError::new(format!("unknown binding `{name}`"))),
        Expr::Select(s) => {
            let base = resolve(&s.operand.expr, env)?;
            select_field(base, &s.field)
        }
        Expr::List(l) => {
            let mut out = Vec::with_capacity(l.elements.len());
            for e in &l.elements {
                out.push(eval(&e.expr, env)?);
            }
            Ok(Ref::Owned(Value::List(out)))
        }
        Expr::Call(c) => eval_call(c, env),
        // Map / struct literals aren't authored in lint rules (options ships
        // through YAML), so they're not evaluable here — no rule needs them.
        Expr::Map(_) | Expr::Struct(_) | Expr::Comprehension(_) | Expr::Unspecified => Err(
            EvalError::new("unsupported expression shape in lint fragment"),
        ),
    }
}

#[expect(
    clippy::cast_possible_wrap,
    reason = "an authored CEL `uint` literal: one above i64::MAX wraps, as before"
)]
fn literal_value(v: &Val) -> Result<Value, EvalError> {
    Ok(match v {
        Val::Null => Value::Null,
        Val::Boolean(b) => Value::Bool(*b),
        Val::Int(i) => Value::Int(*i),
        Val::UInt(u) => Value::Int(*u as i64),
        Val::Double(d) => Value::Double(*d),
        Val::String(s) => Value::Str(s.clone()),
        Val::Bytes(_) => {
            return Err(EvalError::new(
                "bytes literals are not supported in lint fragment",
            ))
        }
    })
}

/// `base.field` (or `base["field"]`): a map's or record's entry, by
/// reference when `base` is.
fn select_field<'e>(base: Ref<'e>, field: &str) -> Result<Ref<'e>, EvalError> {
    let missing = || EvalError::new(format!("missing key `{field}`"));
    match base {
        Ref::Borrowed(Value::Map(m)) => m.get(field).map(Ref::Borrowed).ok_or_else(missing),
        Ref::Owned(Value::Map(mut m)) => m.remove(field).map(Ref::Owned).ok_or_else(missing),
        Ref::Record(r) => r.field(field).ok_or_else(missing),
        other => match other.value() {
            Some(Value::Null) => Err(EvalError::new(format!(
                "select `.{field}` on null (missing binding upstream)"
            ))),
            _ => Err(EvalError::new(format!(
                "select `.{field}` on {} value",
                other.type_name()
            ))),
        },
    }
}

fn eval_call<'e>(c: &CallExpr, env: &'e Env<'_>) -> Result<Ref<'e>, EvalError> {
    let name = c.func_name.as_str();
    if let Some(target) = &c.target {
        return member_call(name, target, &c.args, env).map(Ref::Owned);
    }
    // Short-circuit connectives (Kleene-style, matching decide.rs). A bare
    // scalar op is dispatched to `apply_op`; anything list/map-shaped is
    // handled inline.
    match (name, c.args.as_slice()) {
        (op::LOGICAL_AND, [a, b]) => {
            if !to_bool(&resolve(&a.expr, env)?)? {
                return Ok(Ref::Owned(Value::Bool(false)));
            }
            Ok(Ref::Owned(Value::Bool(to_bool(&resolve(&b.expr, env)?)?)))
        }
        (op::LOGICAL_OR, [a, b]) => {
            if to_bool(&resolve(&a.expr, env)?)? {
                return Ok(Ref::Owned(Value::Bool(true)));
            }
            Ok(Ref::Owned(Value::Bool(to_bool(&resolve(&b.expr, env)?)?)))
        }
        (op::LOGICAL_NOT, [a]) => Ok(Ref::Owned(Value::Bool(!to_bool(&resolve(&a.expr, env)?)?))),
        (op::CONDITIONAL, [c_, t, e]) => {
            if to_bool(&resolve(&c_.expr, env)?)? {
                resolve(&t.expr, env)
            } else {
                resolve(&e.expr, env)
            }
        }
        (op::INDEX, [container, key]) => {
            let base = resolve(&container.expr, env)?;
            let k = resolve(&key.expr, env)?;
            index_value(base, &k.as_value())
        }
        (op::IN, [needle, haystack]) => {
            let n = resolve(&needle.expr, env)?;
            let n = n.as_value();
            let found = match resolve(&haystack.expr, env)? {
                Ref::Record(r) => match n.as_ref() {
                    Value::Str(s) => r.field(s).is_some(),
                    _ => return Err(EvalError::new("`in` needle for a map must be string")),
                },
                h => match h.value() {
                    Some(Value::List(items)) => items.iter().any(|v| v == n.as_ref()),
                    Some(Value::Map(m)) => match n.as_ref() {
                        Value::Str(s) => m.contains_key(s),
                        _ => return Err(EvalError::new("`in` needle for a map must be string")),
                    },
                    _ => {
                        return Err(EvalError::new(format!(
                            "`in` right side is {} (need list or map)",
                            h.type_name()
                        )))
                    }
                },
            };
            Ok(Ref::Owned(Value::Bool(found)))
        }
        ("size", [x]) => {
            let v = resolve(&x.expr, env)?;
            let len = match v.as_value().as_ref() {
                Value::Str(s) => s.chars().count(),
                Value::List(l) => l.len(),
                Value::Map(m) => m.len(),
                other => {
                    return Err(EvalError::new(format!(
                        "size() on {} value",
                        other.type_name()
                    )))
                }
            };
            Ok(Ref::Owned(Value::Int(
                i64::try_from(len).expect("a length is at most isize::MAX"),
            )))
        }
        // Scalar arithmetic/comparison/equality via apply_op.
        (
            op::ADD
            | op::SUBSTRACT
            | op::MULTIPLY
            | op::DIVIDE
            | op::MODULO
            | op::GREATER
            | op::GREATER_EQUALS
            | op::LESS
            | op::LESS_EQUALS
            | op::EQUALS
            | op::NOT_EQUALS,
            [a, b],
        ) => {
            let av = resolve(&a.expr, env)?;
            let bv = resolve(&b.expr, env)?;
            apply_scalar(name, &[&av.as_value(), &bv.as_value()]).map(Ref::Owned)
        }
        (op::NEGATE, [a]) => {
            let av = resolve(&a.expr, env)?;
            apply_scalar(name, &[&av.as_value()]).map(Ref::Owned)
        }
        _ => Err(EvalError::new(format!(
            "unsupported call `{name}` with {} args",
            c.args.len()
        ))),
    }
}

/// A member call `target.name(args)`: the CEL string members `startsWith`,
/// `endsWith` and `contains` (dsl 0.50.0 §8.2), each with one argument and
/// both operands strings. No other member is defined.
fn member_call(
    name: &str,
    target: &IdedExpr,
    args: &[IdedExpr],
    env: &Env<'_>,
) -> Result<Value, EvalError> {
    let test: fn(&str, &str) -> bool = match (name, args.len()) {
        ("startsWith", 1) => |s, t| s.starts_with(t),
        ("endsWith", 1) => |s, t| s.ends_with(t),
        ("contains", 1) => |s, t| s.contains(t),
        _ => {
            return Err(EvalError::new(format!(
                "unsupported member call `.{name}` with {} args",
                args.len()
            )))
        }
    };
    let s = resolve(&target.expr, env)?;
    let t = resolve(&args[0].expr, env)?;
    match (s.value(), t.value()) {
        (Some(Value::Str(s)), Some(Value::Str(t))) => Ok(Value::Bool(test(s, t))),
        _ => Err(EvalError::new(format!(
            "`.{name}` needs string operands, got {} and {}",
            s.type_name(),
            t.type_name()
        ))),
    }
}

/// Dispatch a scalar op to [`lute_check::apply_op`]. Values that are neither
/// bool/num/string bail out with a type mismatch — an authored `speaker.axis
/// > 3` (map compared to a number) is a fragment error, not a silent pass.
fn apply_scalar(name: &str, args: &[&Value]) -> Result<Value, EvalError> {
    let bad = || EvalError::new(format!("scalar op `{name}` failed (type/overflow)"));
    match (name, args) {
        (op::NEGATE, [Value::Int(a)]) => a.checked_neg().map(Value::Int).ok_or_else(bad),
        (op::NEGATE, [Value::Double(a)]) => Ok(Value::Double(-a)),
        (op::EQUALS | op::NOT_EQUALS, [a, b]) => {
            let eq = match (a, b) {
                (Value::Bool(x), Value::Bool(y)) => x == y,
                (Value::Str(x), Value::Str(y)) => x == y,
                (Value::Int(x), Value::Int(y)) => x == y,
                (Value::Double(x), Value::Double(y)) => x == y,
                _ => return Err(bad()),
            };
            Ok(Value::Bool(if name == op::EQUALS { eq } else { !eq }))
        }
        (op::ADD | op::SUBSTRACT | op::MULTIPLY, [Value::Int(a), Value::Int(b)]) => {
            let r = match name {
                op::ADD => a.checked_add(*b),
                op::SUBSTRACT => a.checked_sub(*b),
                _ => a.checked_mul(*b),
            };
            r.map(Value::Int).ok_or_else(bad)
        }
        (op::ADD | op::SUBSTRACT | op::MULTIPLY, [Value::Double(a), Value::Double(b)]) => {
            let r = match name {
                op::ADD => a + b,
                op::SUBSTRACT => a - b,
                _ => a * b,
            };
            Ok(Value::Double(r))
        }
        (op::DIVIDE, [Value::Int(a), Value::Int(b)]) if *b != 0 => {
            a.checked_div(*b).map(Value::Int).ok_or_else(bad)
        }
        (op::DIVIDE, [Value::Double(a), Value::Double(b)]) if *b != 0.0 => Ok(Value::Double(a / b)),
        (op::MODULO, [Value::Int(a), Value::Int(b)]) if *b != 0 => {
            a.checked_rem(*b).map(Value::Int).ok_or_else(bad)
        }
        (op::GREATER | op::GREATER_EQUALS | op::LESS | op::LESS_EQUALS, [a, b]) => {
            let ord = match (a, b) {
                (Value::Int(a), Value::Int(b)) => a.partial_cmp(b),
                (Value::Double(a), Value::Double(b)) => a.partial_cmp(b),
                (Value::Str(a), Value::Str(b)) => a.partial_cmp(b),
                _ => None,
            }
            .ok_or_else(bad)?;
            Ok(Value::Bool(match name {
                op::GREATER => ord.is_gt(),
                op::GREATER_EQUALS => !ord.is_lt(),
                op::LESS => ord.is_lt(),
                _ => !ord.is_gt(),
            }))
        }
        _ => Err(bad()),
    }
}

/// `base[key]`: a map's or record's string key, or a list's whole index —
/// by reference when `base` is.
fn index_value<'e>(base: Ref<'e>, key: &Value) -> Result<Ref<'e>, EvalError> {
    let mismatch = |base: &Ref<'_>| {
        EvalError::new(format!(
            "index {}[{}] type mismatch",
            base.type_name(),
            key.type_name()
        ))
    };
    if let Value::Str(k) = key {
        if matches!(base, Ref::Record(_)) || matches!(base.value(), Some(Value::Map(_))) {
            return select_field(base, k);
        }
    }
    let Some(Value::List(list)) = base.value() else {
        return Err(mismatch(&base));
    };
    let len = list.len();
    let out_of_bounds =
        |shown: String| EvalError::new(format!("list index {shown} out of bounds (size {len})"));
    let i = match key {
        Value::Int(n) => {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "an authored index: on a 32-bit target one past the isize range \
                          truncates, as before"
            )]
            let i = *n as isize;
            match usize::try_from(i) {
                Ok(u) if u < len => u,
                _ => return Err(out_of_bounds(i.to_string())),
            }
        }
        Value::Double(n) if n.fract() == 0.0 && *n >= 0.0 => {
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "a whole non-negative authored index: past the usize range `as` \
                          saturates, as before"
            )]
            let i = *n as usize;
            if i >= len {
                return Err(out_of_bounds(i.to_string()));
            }
            i
        }
        _ => return Err(mismatch(&base)),
    };
    match base {
        Ref::Borrowed(Value::List(l)) => Ok(Ref::Borrowed(&l[i])),
        Ref::Owned(Value::List(mut l)) => Ok(Ref::Owned(l.swap_remove(i))),
        other => Err(mismatch(&other)),
    }
}

fn to_bool(v: &Ref<'_>) -> Result<bool, EvalError> {
    v.value()
        .and_then(Value::as_bool)
        .ok_or_else(|| EvalError::new(format!("expected bool, got {}", v.type_name())))
}

// ---------------------------------------------------------------------------
// Message templating (spec §5).
// ---------------------------------------------------------------------------

/// Render a message template. The DSL is deliberately minimal:
/// `{path.to.field}` interpolates the path evaluated in `env` (numbers
/// rendered trimmed — no trailing `.0`; strings verbatim). Suffix `:%`
/// renders a share/ratio as a rounded percentage (`0.72` → `72%`). An
/// unresolvable path renders as `?` so a template mishap never masks the
/// underlying finding.
pub fn render_message(template: &str, env: &Env<'_>) -> String {
    let bytes = template.as_bytes();
    let mut out = String::with_capacity(template.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' && i + 1 < bytes.len() && bytes[i + 1] != b'{' {
            if let Some(end) = template[i + 1..].find('}') {
                let inner = &template[i + 1..i + 1 + end];
                let (path, format) = match inner.rsplit_once(':') {
                    Some((p, f)) => (p, Some(f)),
                    None => (inner, None),
                };
                let rendered = render_path(path.trim(), format, env);
                out.push_str(&rendered);
                i = i + 1 + end + 1;
                continue;
            }
        }
        // Literal char (multi-byte-safe: read a char boundary).
        let ch_end = next_char_boundary(bytes, i);
        out.push_str(&template[i..ch_end]);
        i = ch_end;
    }
    out
}

fn next_char_boundary(bytes: &[u8], i: usize) -> usize {
    // UTF-8 char at position i.
    let b = bytes[i];
    let width = if b < 0xC0 {
        1 // ASCII, or a continuation byte encountered (malformed): step by 1
    } else if b < 0xE0 {
        2
    } else if b < 0xF0 {
        3
    } else {
        4
    };
    (i + width).min(bytes.len())
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "a percentage of an authored number: past the i64 range `as` saturates, as before"
)]
fn render_path(path: &str, format: Option<&str>, env: &Env<'_>) -> String {
    let mut segments = path.split('.');
    let Some(mut cur) = segments.next().and_then(|root| env.get(root)) else {
        return "?".into();
    };
    for seg in segments {
        cur = match select_field(cur, seg) {
            Ok(next) => next,
            Err(_) => return "?".into(),
        };
    }
    let cur = cur.as_value();
    match format {
        Some("%") => match cur.as_ref() {
            Value::Int(n) => format!("{}%", (*n as f64 * 100.0).round() as i64),
            Value::Double(n) => format!("{}%", (n * 100.0).round() as i64),
            _ => "?".into(),
        },
        _ => render_scalar(&cur),
    }
}

fn render_scalar(v: &Value) -> String {
    match v {
        Value::Null => "".into(),
        Value::Bool(b) => b.to_string(),
        Value::Int(n) => n.to_string(),
        Value::Double(n) => trim_number(*n),
        Value::Str(s) => s.clone(),
        Value::List(l) => l.iter().map(render_scalar).collect::<Vec<_>>().join(", "),
        Value::Map(m) => m.keys().cloned().collect::<Vec<_>>().join(", "),
    }
}

/// `1.0` → `"1"`, `1.5` → `"1.5"`, `72.4` → `"72.4"` — no trailing `.0`
/// (spec §5 "numbers rendered trimmed"), and at most two decimals
/// (`3.3333…` → `"3.33"`): a message is read by an author, and a ratio's
/// sixteenth digit is float noise (dsl 0.22.0 §13).
fn trim_number(n: f64) -> String {
    if !n.is_finite() {
        return format!("{n}");
    }
    let s = format!("{:.2}", (n * 100.0).round() / 100.0);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" {
        "0".to_string()
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_when(raw: &str) -> IdedExpr {
        let mut arena = lute_cel::CelArena::default();
        let h = lute_cel::parse_slot(&mut arena, raw, 0).expect("parse");
        arena.get(h).expect("handle").clone()
    }

    fn scenario_env() -> Env<'static> {
        let mut e = Env::new();
        let mut line = BTreeMap::new();
        line.insert("words".to_string(), Value::Int(41));
        line.insert("speaker".to_string(), Value::Str("alice".into()));
        let mut attrs = BTreeMap::new();
        attrs.insert("emotion".to_string(), Value::Str("fond".into()));
        line.insert("attrs".to_string(), Value::Map(attrs));
        e.bind("line", Value::Map(line));
        let mut opts = BTreeMap::new();
        opts.insert("maxWords".to_string(), Value::Int(40));
        e.bind("options", Value::Map(opts));
        e
    }

    #[test]
    fn scalar_gt() {
        let ex = parse_when("line.words > options.maxWords");
        assert_eq!(eval(&ex.expr, &scenario_env()), Ok(Value::Bool(true)));
    }

    #[test]
    fn indexing_and_size() {
        let mut e = scenario_env();
        let mut axis = BTreeMap::new();
        let mut emotion = BTreeMap::new();
        emotion.insert("run".to_string(), Value::Int(5));
        axis.insert("emotion".to_string(), Value::Map(emotion));
        e.bind(
            "speaker",
            Value::Map({
                let mut m = BTreeMap::new();
                m.insert("axis".to_string(), Value::Map(axis));
                m
            }),
        );
        let ex = parse_when(r#"speaker.axis["emotion"].run > 3"#);
        assert_eq!(eval(&ex.expr, &e), Ok(Value::Bool(true)));

        let ex2 = parse_when(r#"size(line.attrs)"#);
        assert_eq!(eval(&ex2.expr, &e), Ok(Value::Int(1)));
    }

    #[test]
    fn missing_field_is_error() {
        let e = scenario_env();
        let ex = parse_when("line.absent > 0");
        eval(&ex.expr, &e).unwrap_err();
    }

    #[test]
    fn logical_short_circuit() {
        let e = scenario_env();
        let ex = parse_when("false && line.absent");
        assert_eq!(eval(&ex.expr, &e), Ok(Value::Bool(false)));
        let ex2 = parse_when("true || line.absent");
        assert_eq!(eval(&ex2.expr, &e), Ok(Value::Bool(true)));
    }

    #[test]
    fn integer_modulo_shares_apply_op() {
        let e = scenario_env();
        let ex = parse_when("line.words % 2 == 1");
        assert_eq!(eval(&ex.expr, &e), Ok(Value::Bool(true)));
        // A fractional operand is not an integer remainder (dsl 0.24.0 §1).
        eval(&parse_when("line.words % 2.5").expr, &e).unwrap_err();
    }

    #[test]
    fn message_percentage_and_number_trim() {
        let mut e = Env::new();
        e.bind(
            "s",
            Value::Map({
                let mut m = BTreeMap::new();
                m.insert("share".into(), Value::Double(0.72));
                m.insert("run".into(), Value::Int(5));
                m
            }),
        );
        let msg = render_message("run={s.run}, share={s.share:%}", &e);
        assert_eq!(msg, "run=5, share=72%");
    }

    #[test]
    fn message_numbers_round_to_two_decimals() {
        let mut e = Env::new();
        e.bind(
            "p",
            Value::Map({
                let mut m = BTreeMap::new();
                m.insert("ratio".into(), Value::Double(10.0 / 3.0));
                m.insert("half".into(), Value::Double(1.5));
                m.insert("tiny".into(), Value::Double(-0.001));
                m
            }),
        );
        let msg = render_message("{p.ratio} {p.half} {p.tiny}", &e);
        assert_eq!(msg, "3.33 1.5 0");
    }

    fn run(raw: &str, env: &Env<'_>) -> Result<Value, EvalError> {
        eval(&parse_when(raw).expr, env)
    }

    // dsl 0.50.0 §8.2: `startsWith`, `endsWith`, `contains`, member form,
    // both operands strings.
    #[test]
    fn string_members() {
        let e = scenario_env();
        for (raw, want) in [
            (r#"line.speaker.startsWith("al")"#, true),
            (r#"line.speaker.startsWith("ce")"#, false),
            (r#"line.speaker.endsWith("ice")"#, true),
            (r#"line.speaker.endsWith("al")"#, false),
            (r#"line.speaker.contains("lic")"#, true),
            (r#"line.speaker.contains("bob")"#, false),
            (r#"line.attrs["emotion"].startsWith("")"#, true),
            (r#""별빛".startsWith("별")"#, true),
        ] {
            assert_eq!(run(raw, &e), Ok(Value::Bool(want)), "{raw}");
        }
    }

    #[test]
    fn string_members_refuse_other_operands_and_forms() {
        let e = scenario_env();
        for (raw, reason) in [
            (
                "line.words.startsWith(\"4\")",
                "`.startsWith` needs string operands, got int and string",
            ),
            (
                "line.speaker.endsWith(1)",
                "`.endsWith` needs string operands, got string and int",
            ),
            (
                "line.attrs.contains(\"emotion\")",
                "`.contains` needs string operands, got map and string",
            ),
            (
                "line.speaker.contains()",
                "unsupported member call `.contains` with 0 args",
            ),
            (
                "line.speaker.matches(\"a.*\")",
                "unsupported member call `.matches` with 1 args",
            ),
            (
                "startsWith(line.speaker, \"al\")",
                "unsupported call `startsWith` with 2 args",
            ),
        ] {
            assert_eq!(run(raw, &e), Err(EvalError::new(reason)), "{raw}");
        }
    }

    /// A record that counts its whole reads.
    struct Counted {
        inner: Value,
        reads: std::cell::Cell<u32>,
    }

    impl Record for Counted {
        fn field(&self, key: &str) -> Option<Ref<'_>> {
            match &self.inner {
                Value::Map(m) => m.get(key).map(Ref::Borrowed),
                _ => None,
            }
        }
        fn materialize(&self) -> Value {
            self.reads.set(self.reads.get() + 1);
            self.inner.clone()
        }
    }

    // dsl 0.50.0 §8.1: a select/index/`in` chain reads a record by
    // reference; only a whole read builds it.
    #[test]
    fn records_are_read_by_reference_until_read_whole() {
        let scope = Counted {
            inner: Value::Map(BTreeMap::from([(
                "t".to_string(),
                Value::Map(BTreeMap::from([("k".to_string(), Value::Str("v".into()))])),
            )])),
            reads: std::cell::Cell::new(0),
        };
        let e = scenario_env().with_record("scope", &scope);
        for raw in [
            r#""t" in scope && "k" in scope["t"] && scope["t"]["k"] == "v""#,
            r#"!("u" in scope) && scope.t.k.startsWith("v")"#,
        ] {
            assert_eq!(run(raw, &e), Ok(Value::Bool(true)), "{raw}");
        }
        assert_eq!(render_message("{scope.t.k}", &e), "v");
        assert_eq!(scope.reads.get(), 0);
        assert_eq!(run("size(scope)", &e), Ok(Value::Int(1)));
        assert_eq!(scope.reads.get(), 1);
        assert_eq!(
            run(r#"scope["u"]"#, &e),
            Err(EvalError::new("missing key `u`"))
        );
        assert_eq!(
            run("scope[1]", &e),
            Err(EvalError::new("index map[int] type mismatch"))
        );
    }
}
