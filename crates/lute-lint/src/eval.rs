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

use std::collections::BTreeMap;

use cel_parser::ast::{operators as op, CallExpr, Expr};
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
    Num(f64),
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
            Value::Num(n) => Some(*n),
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
            Value::Num(_) => "num",
            Value::Str(_) => "string",
            Value::List(_) => "list",
            Value::Map(_) => "map",
        }
    }
}

/// A binding table for the target-row env (`line`, `shot`, `scene`,
/// `speaker`, `group`, `project`, `options`). Undeclared bindings resolve
/// to a missing-field error the moment they're read.
#[derive(Default, Clone)]
pub struct Env {
    pub bindings: BTreeMap<String, Value>,
}

impl Env {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with(mut self, name: &str, v: Value) -> Self {
        self.bindings.insert(name.to_string(), v);
        self
    }
    pub fn bind(&mut self, name: &str, v: Value) {
        self.bindings.insert(name.to_string(), v);
    }
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.bindings.get(name)
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

/// Evaluate a top-level CEL expression under `env`.
pub fn eval(expr: &Expr, env: &Env) -> Result<Value, EvalError> {
    match expr {
        Expr::Literal(v) => literal_value(v),
        Expr::Ident(name) => env
            .get(name)
            .cloned()
            .ok_or_else(|| EvalError::new(format!("unknown binding `{name}`"))),
        Expr::Select(s) => {
            let base = eval(&s.operand.expr, env)?;
            select_field(&base, &s.field)
        }
        Expr::List(l) => {
            let mut out = Vec::with_capacity(l.elements.len());
            for e in &l.elements {
                out.push(eval(&e.expr, env)?);
            }
            Ok(Value::List(out))
        }
        Expr::Call(c) => eval_call(c, env),
        // Map / struct literals aren't authored in lint rules (options ships
        // through YAML), so they're not evaluable here — no rule needs them.
        Expr::Map(_) | Expr::Struct(_) | Expr::Comprehension(_) | Expr::Unspecified => Err(
            EvalError::new("unsupported expression shape in lint fragment"),
        ),
    }
}

fn literal_value(v: &Val) -> Result<Value, EvalError> {
    Ok(match v {
        Val::Null => Value::Null,
        Val::Boolean(b) => Value::Bool(*b),
        Val::Int(i) => Value::Num(*i as f64),
        Val::UInt(u) => Value::Num(*u as f64),
        Val::Double(d) => Value::Num(*d),
        Val::String(s) => Value::Str(s.clone()),
        Val::Bytes(_) => {
            return Err(EvalError::new(
                "bytes literals are not supported in lint fragment",
            ))
        }
    })
}

fn select_field(base: &Value, field: &str) -> Result<Value, EvalError> {
    match base {
        Value::Map(m) => m
            .get(field)
            .cloned()
            .ok_or_else(|| EvalError::new(format!("missing key `{field}`"))),
        Value::Null => Err(EvalError::new(format!(
            "select `.{field}` on null (missing binding upstream)"
        ))),
        other => Err(EvalError::new(format!(
            "select `.{field}` on {} value",
            other.type_name()
        ))),
    }
}

fn eval_call(c: &CallExpr, env: &Env) -> Result<Value, EvalError> {
    let name = c.func_name.as_str();
    // Short-circuit connectives (Kleene-style, matching decide.rs). A bare
    // scalar op is dispatched to `apply_op`; anything list/map-shaped is
    // handled inline.
    match (name, c.args.as_slice()) {
        (op::LOGICAL_AND, [a, b]) => {
            let av = eval(&a.expr, env)?;
            let ab = to_bool(&av)?;
            if !ab {
                return Ok(Value::Bool(false));
            }
            let bv = eval(&b.expr, env)?;
            Ok(Value::Bool(to_bool(&bv)?))
        }
        (op::LOGICAL_OR, [a, b]) => {
            let av = eval(&a.expr, env)?;
            if to_bool(&av)? {
                return Ok(Value::Bool(true));
            }
            let bv = eval(&b.expr, env)?;
            Ok(Value::Bool(to_bool(&bv)?))
        }
        (op::LOGICAL_NOT, [a]) => {
            let v = eval(&a.expr, env)?;
            Ok(Value::Bool(!to_bool(&v)?))
        }
        (op::CONDITIONAL, [c_, t, e]) => {
            let cv = eval(&c_.expr, env)?;
            if to_bool(&cv)? {
                eval(&t.expr, env)
            } else {
                eval(&e.expr, env)
            }
        }
        (op::INDEX, [container, key]) => {
            let base = eval(&container.expr, env)?;
            let k = eval(&key.expr, env)?;
            index_value(&base, &k)
        }
        (op::IN, [needle, haystack]) => {
            let n = eval(&needle.expr, env)?;
            let h = eval(&haystack.expr, env)?;
            match h {
                Value::List(items) => Ok(Value::Bool(items.iter().any(|v| v == &n))),
                Value::Map(m) => match n {
                    Value::Str(s) => Ok(Value::Bool(m.contains_key(&s))),
                    _ => Err(EvalError::new("`in` needle for a map must be string")),
                },
                other => Err(EvalError::new(format!(
                    "`in` right side is {} (need list or map)",
                    other.type_name()
                ))),
            }
        }
        ("size", [x]) => {
            let v = eval(&x.expr, env)?;
            match v {
                Value::Str(s) => Ok(Value::Num(s.chars().count() as f64)),
                Value::List(l) => Ok(Value::Num(l.len() as f64)),
                Value::Map(m) => Ok(Value::Num(m.len() as f64)),
                other => Err(EvalError::new(format!(
                    "size() on {} value",
                    other.type_name()
                ))),
            }
        }
        // Scalar arithmetic/comparison/equality via apply_op.
        (
            op::ADD
            | op::SUBSTRACT
            | op::MULTIPLY
            | op::DIVIDE
            | op::GREATER
            | op::GREATER_EQUALS
            | op::LESS
            | op::LESS_EQUALS,
            [a, b],
        ) => {
            let av = eval(&a.expr, env)?;
            let bv = eval(&b.expr, env)?;
            apply_scalar(name, &[av, bv])
        }
        (op::EQUALS | op::NOT_EQUALS, [a, b]) => {
            let av = eval(&a.expr, env)?;
            let bv = eval(&b.expr, env)?;
            apply_scalar(name, &[av, bv])
        }
        (op::NEGATE, [a]) => {
            let av = eval(&a.expr, env)?;
            apply_scalar(name, &[av])
        }
        _ => Err(EvalError::new(format!(
            "unsupported call `{name}` with {} args",
            c.args.len()
        ))),
    }
}

/// Dispatch a scalar op to [`lute_check::apply_op`]. Values that are neither
/// bool/num/string bail out with a type mismatch — an authored `speaker.axis
/// > 3` (map compared to a number) is a fragment error, not a silent pass.
fn apply_scalar(name: &str, args: &[Value]) -> Result<Value, EvalError> {
    let decided: Result<Vec<lute_check::Decided>, EvalError> = args
        .iter()
        .map(|v| match v {
            Value::Bool(b) => Ok(lute_check::Decided::Bool(*b)),
            Value::Num(n) => {
                if n.is_finite() {
                    Ok(lute_check::Decided::Num(*n))
                } else {
                    Err(EvalError::new("non-finite number in scalar op"))
                }
            }
            Value::Str(s) => Ok(lute_check::Decided::Str(s.clone())),
            other => Err(EvalError::new(format!(
                "scalar op `{name}` on {} value",
                other.type_name()
            ))),
        })
        .collect();
    let decided = decided?;
    let out = lute_check::apply_op(name, &decided).ok_or_else(|| {
        EvalError::new(format!("scalar op `{name}` failed (arity/type/overflow)"))
    })?;
    Ok(decided_to_value(out))
}

fn decided_to_value(d: lute_check::Decided) -> Value {
    match d {
        lute_check::Decided::Bool(b) => Value::Bool(b),
        lute_check::Decided::Num(n) => Value::Num(n),
        lute_check::Decided::Str(s) => Value::Str(s),
    }
}

fn index_value(base: &Value, key: &Value) -> Result<Value, EvalError> {
    match (base, key) {
        (Value::Map(m), Value::Str(k)) => m
            .get(k)
            .cloned()
            .ok_or_else(|| EvalError::new(format!("missing key `{k}`"))),
        (Value::List(l), Value::Num(n)) => {
            let i = *n as isize;
            if i < 0 || (i as usize) >= l.len() {
                return Err(EvalError::new(format!(
                    "list index {i} out of bounds (size {})",
                    l.len()
                )));
            }
            Ok(l[i as usize].clone())
        }
        _ => Err(EvalError::new(format!(
            "index {}[{}] type mismatch",
            base.type_name(),
            key.type_name()
        ))),
    }
}

fn to_bool(v: &Value) -> Result<bool, EvalError> {
    v.as_bool()
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
pub fn render_message(template: &str, env: &Env) -> String {
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

fn render_path(path: &str, format: Option<&str>, env: &Env) -> String {
    let segments: Vec<&str> = path.split('.').collect();
    if segments.is_empty() {
        return "?".into();
    }
    let mut cur = match env.get(segments[0]) {
        Some(v) => v.clone(),
        None => return "?".into(),
    };
    for seg in &segments[1..] {
        cur = match &cur {
            Value::Map(m) => match m.get(*seg) {
                Some(v) => v.clone(),
                None => return "?".into(),
            },
            _ => return "?".into(),
        };
    }
    match format {
        Some("%") => match cur {
            Value::Num(n) => format!("{}%", (n * 100.0).round() as i64),
            _ => "?".into(),
        },
        _ => render_scalar(&cur),
    }
}

fn render_scalar(v: &Value) -> String {
    match v {
        Value::Null => "".into(),
        Value::Bool(b) => b.to_string(),
        Value::Num(n) => trim_number(*n),
        Value::Str(s) => s.clone(),
        Value::List(l) => l.iter().map(render_scalar).collect::<Vec<_>>().join(", "),
        Value::Map(m) => m.keys().cloned().collect::<Vec<_>>().join(", "),
    }
}

/// `1.0` → `"1"`, `1.5` → `"1.5"`, `72.4` → `"72.4"` — no trailing `.0`
/// (spec §5 "numbers rendered trimmed").
fn trim_number(n: f64) -> String {
    if n.fract() == 0.0 && n.is_finite() {
        return (n as i64).to_string();
    }
    let s = format!("{n}");
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use cel_parser::ast::IdedExpr;

    fn parse_when(raw: &str) -> IdedExpr {
        let mut arena = lute_cel::CelArena::default();
        let h = lute_cel::parse_slot(&mut arena, raw, 0).expect("parse");
        arena.get(h).expect("handle").clone()
    }

    fn scenario_env() -> Env {
        let mut e = Env::new();
        let mut line = BTreeMap::new();
        line.insert("words".to_string(), Value::Num(41.0));
        line.insert("speaker".to_string(), Value::Str("alice".into()));
        let mut attrs = BTreeMap::new();
        attrs.insert("emotion".to_string(), Value::Str("fond".into()));
        line.insert("attrs".to_string(), Value::Map(attrs));
        e.bind("line", Value::Map(line));
        let mut opts = BTreeMap::new();
        opts.insert("maxWords".to_string(), Value::Num(40.0));
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
        emotion.insert("run".to_string(), Value::Num(5.0));
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
        assert_eq!(eval(&ex2.expr, &e), Ok(Value::Num(1.0)));
    }

    #[test]
    fn missing_field_is_error() {
        let e = scenario_env();
        let ex = parse_when("line.absent > 0");
        assert!(eval(&ex.expr, &e).is_err());
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
    fn message_percentage_and_number_trim() {
        let mut e = Env::new();
        e.bind(
            "s",
            Value::Map({
                let mut m = BTreeMap::new();
                m.insert("share".into(), Value::Num(0.72));
                m.insert("run".into(), Value::Num(5.0));
                m
            }),
        );
        let msg = render_message("run={s.run}, share={s.share:%}", &e);
        assert_eq!(msg, "run=5, share=72%");
    }
}
