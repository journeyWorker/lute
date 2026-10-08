//! The IR's portable expression AST (`exprNode`, IR A7), decoded once and
//! evaluated without a CEL parser (spec 0.38.0 §13).
//!
//! Every CEL slot of an execution IR is a `{cel, expr}` pair. The executor
//! evaluates `expr` only; `cel` is kept for messages. [`Expr::decode`] turns
//! one `exprNode` JSON value into an [`Expr`] when the record holding the
//! slot is decoded — never per evaluation.
//!
//! Evaluation is three-valued (K3): a value the walk cannot know is
//! [`Value::Unknown`] and records the [`UnresolvedAtom`] that made it so.
//! The rules are the Lute CEL profile's (`docs/runtime/cel-and-facts.md`):
//! `false && x` and `true || x` never evaluate `x`; otherwise a `false`
//! (`&&`) or `true` (`||`) on either side decides, an error beats unknown;
//! every other operator is unknown when an operand is, an error when an
//! operand is; `has(path)` is definite; `holds` / `count` /
//! `countDistinct` query the fact store; `now()` reads the narrative clock;
//! `validAt(…)` is always unknown.

use std::sync::Arc;

use serde_json::Value as Json;

use crate::datalog::Binding;
use crate::eval::{EvalEnv, Pat, Read};
use crate::{UnresolvedAtom, Value};

/// `occasion.target`: the member an occasion is raised for.
const OCCASION_TARGET: &str = lute_check::beats::OCCASION_TARGET;

/// One decoded `exprNode`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Expr {
    Lit(Value),
    /// A canonical dotted state path (`quest.zero-coke-001.state`).
    Path(Box<str>),
    /// `has(path)`: whether the path has an effective value.
    Has(Box<str>),
    Not(Box<Expr>),
    Neg(Box<Expr>),
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
    In(Box<Expr>, Box<Expr>),
    /// A ground binary operator: `== != < <= > >= + - * / %`.
    Binary(BinOp, Box<Expr>, Box<Expr>),
    Cond(Box<Expr>, Box<Expr>, Box<Expr>),
    List(Vec<Expr>),
    Index(Box<Expr>, Box<Expr>),
    Call(Func, Vec<Expr>),
    /// A node outside the profile; evaluates unknown without an atom.
    Invalid,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BinOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Add,
    Sub,
    Mul,
    Div,
    Rem,
}

impl BinOp {
    fn of(sym: &str) -> Option<BinOp> {
        Some(match sym {
            "==" => BinOp::Eq,
            "!=" => BinOp::Ne,
            "<" => BinOp::Lt,
            "<=" => BinOp::Le,
            ">" => BinOp::Gt,
            ">=" => BinOp::Ge,
            "+" => BinOp::Add,
            "-" => BinOp::Sub,
            "*" => BinOp::Mul,
            "/" => BinOp::Div,
            "%" => BinOp::Rem,
            _ => return None,
        })
    }

    fn symbol(self) -> &'static str {
        match self {
            BinOp::Eq => "==",
            BinOp::Ne => "!=",
            BinOp::Lt => "<",
            BinOp::Le => "<=",
            BinOp::Gt => ">",
            BinOp::Ge => ">=",
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Rem => "%",
        }
    }
}

/// The host functions and conversions the profile admits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Func {
    Holds,
    Count,
    CountDistinct,
    Visited,
    ValidAt,
    Now,
    Int,
    Double,
    /// Any other name: out of the profile, evaluates unknown.
    Other,
}

impl Func {
    fn of(name: &str) -> Func {
        match name {
            "holds" => Func::Holds,
            "count" => Func::Count,
            "countDistinct" => Func::CountDistinct,
            "visited" => Func::Visited,
            "validAt" => Func::ValidAt,
            "now" => Func::Now,
            "int" => Func::Int,
            "double" => Func::Double,
            _ => Func::Other,
        }
    }
}

impl Expr {
    /// Decode one `exprNode` (`schemas/lute-ir-0.38.schema.json`
    /// `$defs/exprNode`). A shape outside the schema decodes to
    /// [`Expr::Invalid`].
    pub(crate) fn decode(node: &Json) -> Expr {
        let Some(obj) = node.as_object() else {
            return Expr::Invalid;
        };
        let sub = |key: &str| Box::new(obj.get(key).map_or(Expr::Invalid, Expr::decode));
        if let Some(v) = obj.get("int") {
            return v
                .as_i64()
                .map_or(Expr::Invalid, |n| Expr::Lit(Value::Int(n)));
        }
        if let Some(v) = obj.get("double") {
            return v
                .as_f64()
                .map_or(Expr::Invalid, |n| Expr::Lit(Value::Double(n)));
        }
        if let Some(v) = obj.get("bool") {
            return v
                .as_bool()
                .map_or(Expr::Invalid, |b| Expr::Lit(Value::Bool(b)));
        }
        if let Some(v) = obj.get("string") {
            return v
                .as_str()
                .map_or(Expr::Invalid, |s| Expr::Lit(Value::Str(s.to_string())));
        }
        if let Some(p) = obj.get("path") {
            return p.as_str().map_or(Expr::Invalid, |p| Expr::Path(p.into()));
        }
        if let Some(p) = obj.get("has") {
            return p.as_str().map_or(Expr::Invalid, |p| Expr::Has(p.into()));
        }
        if let Some(op) = obj.get("op").and_then(Json::as_str) {
            if !obj.contains_key("r") {
                return match op {
                    "!" => Expr::Not(sub("l")),
                    "-" => Expr::Neg(sub("l")),
                    _ => Expr::Invalid,
                };
            }
            return match op {
                "&&" => Expr::And(sub("l"), sub("r")),
                "||" => Expr::Or(sub("l"), sub("r")),
                "in" => Expr::In(sub("l"), sub("r")),
                sym => match BinOp::of(sym) {
                    Some(op) => Expr::Binary(op, sub("l"), sub("r")),
                    None => Expr::Invalid,
                },
            };
        }
        if obj.contains_key("cond") {
            return Expr::Cond(sub("cond"), sub("then"), sub("else"));
        }
        if let Some(items) = obj.get("list") {
            return items.as_array().map_or(Expr::Invalid, |items| {
                Expr::List(items.iter().map(Expr::decode).collect())
            });
        }
        if obj.contains_key("index") {
            return Expr::Index(sub("index"), sub("key"));
        }
        if let Some(name) = obj.get("call").and_then(Json::as_str) {
            let args = obj.get("args").and_then(Json::as_array);
            return Expr::Call(
                Func::of(name),
                args.map(|a| a.iter().map(Expr::decode).collect())
                    .unwrap_or_default(),
            );
        }
        Expr::Invalid
    }

    /// The canonical dotted path this expression names statically: a path,
    /// a `has()` test of one, or a string-literal index of one
    /// (`run.visits["lab-b2"]` → `run.visits.lab-b2`).
    pub(crate) fn static_path(&self) -> Option<String> {
        match self {
            Expr::Path(p) | Expr::Has(p) => Some(p.to_string()),
            Expr::Index(target, key) => match key.as_ref() {
                Expr::Lit(Value::Str(k)) => Some(format!("{}.{k}", target.static_path()?)),
                _ => None,
            },
            _ => None,
        }
    }

    fn is_occasion_target(&self) -> bool {
        matches!(self, Expr::Path(p) | Expr::Has(p) if &**p == OCCASION_TARGET)
    }

    /// The operands of a left-associated `&&` chain cut into `parts`
    /// conjuncts, left to right: `a && b && c` is `(a && b) && c`, so two
    /// parts are `[a && b, c]` and three are `[a, b, c]`. `None` when the
    /// chain is shorter than `parts`.
    pub(crate) fn conjuncts(&self, parts: usize) -> Option<Vec<&Expr>> {
        let mut out = Vec::with_capacity(parts);
        let mut at = self;
        for _ in 1..parts {
            let Expr::And(l, r) = at else {
                return None;
            };
            out.push(r.as_ref());
            at = l;
        }
        out.push(at);
        out.reverse();
        Some(out)
    }
}

/// A CEL slot of the IR: its text (for messages) and its decoded `expr`.
#[derive(Clone, Debug, PartialEq)]
pub struct Slot {
    pub(crate) raw: String,
    pub(crate) expr: Expr,
}

impl Slot {
    /// Decode a `{cel, expr}` pair; `None` when `pair` is not one.
    pub fn of(pair: &Json) -> Option<Arc<Slot>> {
        let raw = pair.get("cel").and_then(Json::as_str)?;
        let expr = pair.get("expr").map_or(Expr::Invalid, Expr::decode);
        Some(Arc::new(Slot {
            raw: raw.to_string(),
            expr,
        }))
    }

    /// A slot over an expression built by the executor itself.
    pub(crate) fn synthetic(raw: String, expr: Expr) -> Slot {
        Slot { raw, expr }
    }

    /// The slot's CEL text, for messages.
    pub fn raw(&self) -> &str {
        &self.raw
    }
}

/// Evaluate `expr` over `env`; `binding` substitutes datalog rule variables
/// (a rule-body guard reads them as bare names).
pub(crate) fn eval(
    expr: &Expr,
    env: &EvalEnv<'_>,
    binding: Option<&Binding>,
    atoms: &mut Vec<UnresolvedAtom>,
) -> Value {
    Eval { env, binding }.eval(expr, atoms)
}

struct Eval<'e, 'a> {
    env: &'e EvalEnv<'a>,
    binding: Option<&'e Binding>,
}

impl Eval<'_, '_> {
    fn eval(&self, expr: &Expr, atoms: &mut Vec<UnresolvedAtom>) -> Value {
        match expr {
            Expr::Lit(v) => v.clone(),
            Expr::Path(p) => match self.bound(p) {
                Some(v) => v,
                None => self.read(p, atoms),
            },
            Expr::Has(p) => self.present(p),
            Expr::Not(a) => self.ground(UnOp::Not, std::slice::from_ref(a.as_ref()), atoms),
            Expr::Neg(a) => self.ground(UnOp::Neg, std::slice::from_ref(a.as_ref()), atoms),
            Expr::And(a, b) => {
                let va = self.eval(a, atoms);
                if va == Value::Bool(false) {
                    return va;
                }
                match (va, self.eval(b, atoms)) {
                    (Value::Bool(true), Value::Bool(true)) => Value::Bool(true),
                    (Value::Bool(false), _) | (_, Value::Bool(false)) => Value::Bool(false),
                    (Value::Error(e), _) | (_, Value::Error(e)) => Value::Error(e),
                    _ => Value::Unknown,
                }
            }
            Expr::Or(a, b) => {
                let va = self.eval(a, atoms);
                if va == Value::Bool(true) {
                    return va;
                }
                match (va, self.eval(b, atoms)) {
                    (Value::Bool(false), Value::Bool(false)) => Value::Bool(false),
                    (Value::Bool(true), _) | (_, Value::Bool(true)) => Value::Bool(true),
                    (Value::Error(e), _) | (_, Value::Error(e)) => Value::Error(e),
                    _ => Value::Unknown,
                }
            }
            Expr::In(needle, list) => self.eval_in(needle, list, atoms),
            Expr::Binary(op, a, b) => {
                let values = [self.eval(a, atoms), self.eval(b, atoms)];
                lift(&values, |v| binary(*op, &v[0], &v[1]))
            }
            Expr::Cond(c, t, e) => match self.eval(c, atoms) {
                Value::Bool(true) => self.eval(t, atoms),
                Value::Bool(false) => self.eval(e, atoms),
                Value::Error(e) => Value::Error(e),
                _ => Value::Unknown,
            },
            Expr::Index(target, key) => self.eval_index(target, key, atoms),
            Expr::Call(f, args) => self.call(*f, args, atoms),
            Expr::List(_) | Expr::Invalid => Value::Unknown,
        }
    }

    /// A rule variable's ground value: numeric text is a number, any other
    /// a string — what substituting it into the guard text would read.
    fn bound(&self, name: &str) -> Option<Value> {
        let v = self.binding?.get(name)?;
        Some(if let Ok(n) = v.parse::<i64>() {
            Value::Int(n)
        } else if let Ok(d) = v.parse::<f64>() {
            // `inf` / `NaN` substitute as bare names, i.e. path reads.
            if !d.is_finite() {
                return None;
            }
            Value::Double(d)
        } else {
            Value::Str(v.clone())
        })
    }

    /// A literal operand, a bound rule variable included (`run.aff[S]` with
    /// `S` bound to `mira` reads `run.aff.mira`).
    fn literal(&self, e: &Expr) -> Option<Value> {
        match e {
            Expr::Lit(v) => Some(v.clone()),
            Expr::Path(p) => self.bound(p),
            _ => None,
        }
    }

    fn read(&self, path: &str, atoms: &mut Vec<UnresolvedAtom>) -> Value {
        match self.env.state.read(path) {
            Read::Value(v) => {
                if v == Value::Unknown {
                    atoms.push(UnresolvedAtom::Path(path.to_string()));
                }
                v
            }
            Read::Unset => {
                atoms.push(UnresolvedAtom::Path(path.to_string()));
                Value::Unknown
            }
        }
    }

    /// `has(path)` is definite: true iff an effective value exists.
    fn present(&self, path: &str) -> Value {
        Value::Bool(!matches!(self.env.state.read(path), Read::Unset))
    }

    /// A ground unary operator lifted over unknown and error.
    fn ground(&self, op: UnOp, args: &[Expr], atoms: &mut Vec<UnresolvedAtom>) -> Value {
        let values: Vec<Value> = args.iter().map(|a| self.eval(a, atoms)).collect();
        lift(&values, |v| unary(op, &v[0]))
    }

    /// `x in [a, b]` (every element evaluated), or `key in map.path` — a
    /// state family having that member.
    fn eval_in(&self, needle: &Expr, list: &Expr, atoms: &mut Vec<UnresolvedAtom>) -> Value {
        if let Some(prefix) = list.static_path() {
            let key = match self.eval(needle, atoms) {
                Value::Str(s) => s,
                Value::Error(e) => return Value::Error(e),
                Value::Unknown => return Value::Unknown,
                _ => return Value::Error("map membership key must be a string".into()),
            };
            let path = format!("{prefix}.{key}");
            let below = format!("{path}.");
            return Value::Bool(
                self.env
                    .state
                    .effective_paths()
                    .iter()
                    .any(|p| p == &path || p.starts_with(&below)),
            );
        }
        let Expr::List(items) = list else {
            return Value::Unknown;
        };
        let mut values = Vec::with_capacity(items.len() + 1);
        values.push(self.eval(needle, atoms));
        values.extend(items.iter().map(|i| self.eval(i, atoms)));
        lift(&values, |v| Value::Bool(v[1..].iter().any(|x| *x == v[0])))
    }

    /// `family[occasion.target]` reads the bound member's path; a quoted
    /// member (`run.visits["lab-b2"]`) its dotted path; a list literal its
    /// element. Anything else is unknown.
    fn eval_index(&self, target: &Expr, key: &Expr, atoms: &mut Vec<UnresolvedAtom>) -> Value {
        if let (Some(family), true) = (target.static_path(), key.is_occasion_target()) {
            return match self.eval(key, atoms) {
                Value::Str(member) => self.read(&format!("{family}.{member}"), atoms),
                _ => Value::Unknown,
            };
        }
        if let (Some(family), Some(Value::Str(member))) = (target.static_path(), self.literal(key))
        {
            return self.read(&format!("{family}.{member}"), atoms);
        }
        let Expr::List(items) = target else {
            return Value::Unknown;
        };
        match self.eval(key, atoms) {
            Value::Int(n) if n >= 0 => items
                .get(n as usize)
                .map_or(Value::Unknown, |item| self.eval(item, atoms)),
            _ => Value::Unknown,
        }
    }

    fn call(&self, f: Func, args: &[Expr], atoms: &mut Vec<UnresolvedAtom>) -> Value {
        match (f, args) {
            (Func::Holds | Func::Count, [rel, Expr::List(list)]) => {
                self.fact_query(f, rel, list, None, atoms)
            }
            (Func::CountDistinct, [rel, Expr::List(list), column]) => {
                let Expr::Lit(Value::Int(i)) = column else {
                    return Value::Error("countDistinct column must be an int".into());
                };
                if *i < 0 {
                    return Value::Error("countDistinct column must be non-negative".into());
                }
                self.fact_query(f, rel, list, Some(*i as usize), atoms)
            }
            (Func::Holds | Func::Count, [_]) | (Func::CountDistinct, [_, _]) => {
                Value::Error("legacy evaluator form is not supported".into())
            }
            (Func::Int, [a]) => match self.eval(a, atoms) {
                Value::Int(i) => Value::Int(i),
                Value::Double(d)
                    if d.is_finite() && d >= i64::MIN as f64 && d < (i64::MAX as f64) + 1.0 =>
                {
                    Value::Int(d as i64)
                }
                Value::Error(e) => Value::Error(e),
                _ => Value::Error("int() expects a number".into()),
            },
            (Func::Double, [a]) => match self.eval(a, atoms) {
                Value::Double(d) => Value::Double(d),
                Value::Int(i) => Value::Double(i as f64),
                Value::Error(e) => Value::Error(e),
                _ => Value::Error("double() expects a number".into()),
            },
            (Func::Visited, [a]) => match a {
                Expr::Lit(Value::Str(id)) => Value::Bool(self.env.facts.visited(id)),
                _ => Value::Error("visited() expects a string".into()),
            },
            (Func::ValidAt, [_, _] | [_, _, _]) => {
                atoms.push(UnresolvedAtom::Time);
                Value::Unknown
            }
            (Func::Now, []) => {
                for path in ["clock.tick", "clock.day"] {
                    if let Read::Value(Value::Int(n)) = self.env.state.read(path) {
                        return Value::Int(n);
                    }
                }
                atoms.push(UnresolvedAtom::Time);
                Value::Unknown
            }
            _ => Value::Unknown,
        }
    }

    /// `holds` / `count` / `countDistinct` over the fact store. A pattern
    /// argument is a string (`'_'` matches anything), a bool, or
    /// `occasion.target` (the bound member; unbound, the query is unknown).
    fn fact_query(
        &self,
        f: Func,
        rel: &Expr,
        list: &[Expr],
        column: Option<usize>,
        atoms: &mut Vec<UnresolvedAtom>,
    ) -> Value {
        let Expr::Lit(Value::Str(relation)) = rel else {
            return Value::Error("relation name must be a string literal".into());
        };
        let mut pats = Vec::with_capacity(list.len());
        for arg in list {
            pats.push(match arg {
                Expr::Lit(Value::Str(s)) if s == "_" => Pat::Wildcard,
                Expr::Lit(Value::Str(s)) => Pat::Ground(s.clone()),
                Expr::Lit(Value::Bool(b)) => Pat::Ground(b.to_string()),
                e if e.static_path().as_deref() == Some(OCCASION_TARGET) => {
                    match self.read(OCCASION_TARGET, atoms) {
                        Value::Str(member) => Pat::Ground(member),
                        _ => return Value::Unknown,
                    }
                }
                _ => return Value::Error("relation arguments must be literals".into()),
            });
        }
        if let Some(slot) = column.and_then(|i| pats.get_mut(i)) {
            *slot = Pat::Wildcard;
        }
        match self
            .env
            .facts
            .lookup_distinct(relation, &pats, column, self.env.state)
        {
            Ok(n) if f == Func::Holds => Value::Bool(n > 0),
            Ok(n) => Value::Int(n as i64),
            Err(found) => {
                atoms.extend(found);
                Value::Unknown
            }
        }
    }
}

#[derive(Clone, Copy)]
enum UnOp {
    Not,
    Neg,
}

/// A ground operator over evaluated operands: unknown when one is unknown,
/// else the first error, else `op`.
fn lift(values: &[Value], op: impl FnOnce(&[Value]) -> Value) -> Value {
    if values.iter().any(|v| matches!(v, Value::Unknown)) {
        return Value::Unknown;
    }
    if let Some(Value::Error(e)) = values.iter().find(|v| matches!(v, Value::Error(_))) {
        return Value::Error(e.clone());
    }
    op(values)
}

fn unary(op: UnOp, v: &Value) -> Value {
    match (op, v) {
        (UnOp::Not, Value::Bool(b)) => Value::Bool(!b),
        (UnOp::Neg, Value::Int(n)) => n
            .checked_neg()
            .map_or_else(|| Value::Error("integer overflow".into()), Value::Int),
        (UnOp::Neg, Value::Double(d)) => Value::Double(-d),
        (UnOp::Not, _) => invalid("!_"),
        (UnOp::Neg, _) => invalid("-_"),
    }
}

/// The error a mistyped operand gives, named by the CEL operator function.
fn invalid(name: &str) -> Value {
    Value::Error(format!("CEL operation `{name}` has invalid operands"))
}

fn binary(op: BinOp, a: &Value, b: &Value) -> Value {
    let name = || format!("_{}_", op.symbol());
    let overflow = || Value::Error("integer overflow".into());
    match op {
        BinOp::Eq | BinOp::Ne => {
            let eq = match (a, b) {
                (Value::Bool(x), Value::Bool(y)) => x == y,
                (Value::Str(x), Value::Str(y)) => x == y,
                (Value::Int(x), Value::Int(y)) => x == y,
                (Value::Double(x), Value::Double(y)) => x == y,
                // `!=` is `!(a == b)`: a mistyped pair is `==`'s error.
                _ => return invalid("_==_"),
            };
            Value::Bool(eq == (op == BinOp::Eq))
        }
        BinOp::Add | BinOp::Sub | BinOp::Mul => match (a, b) {
            (Value::Int(x), Value::Int(y)) => match op {
                BinOp::Add => x.checked_add(*y),
                BinOp::Sub => x.checked_sub(*y),
                _ => x.checked_mul(*y),
            }
            .map_or_else(overflow, Value::Int),
            (Value::Double(x), Value::Double(y)) => Value::Double(match op {
                BinOp::Add => x + y,
                BinOp::Sub => x - y,
                _ => x * y,
            }),
            _ => invalid(&name()),
        },
        BinOp::Div | BinOp::Rem => match (a, b) {
            (Value::Int(_), Value::Int(0)) => Value::Error("division by zero".into()),
            (Value::Int(x), Value::Int(y)) => if op == BinOp::Div {
                x.checked_div(*y)
            } else {
                x.checked_rem(*y)
            }
            .map_or_else(overflow, Value::Int),
            (Value::Double(x), Value::Double(y)) if op == BinOp::Div => Value::Double(x / y),
            _ => invalid(&name()),
        },
        BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
            let ord = match (a, b) {
                (Value::Int(x), Value::Int(y)) => Some(x.cmp(y)),
                (Value::Double(x), Value::Double(y)) => x.partial_cmp(y),
                (Value::Str(x), Value::Str(y)) => Some(x.cmp(y)),
                _ => None,
            };
            match ord {
                Some(o) => Value::Bool(match op {
                    BinOp::Lt => o.is_lt(),
                    BinOp::Le => !o.is_gt(),
                    BinOp::Gt => o.is_gt(),
                    _ => !o.is_lt(),
                }),
                None => invalid(&name()),
            }
        }
    }
}

/// One read of a guard (round-5 T3-12): a dotted state path, a fact pattern
/// (rendered `rel(a, _)`), a scene id `visited(…)` asks about — or a family
/// read by the bound member, `user.bond[occasion.target]` (the family).
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum GuardAtom {
    Path(String),
    Fact(String),
    Visited(String),
    Indexed(String),
}

/// Every read of `expr`, in document order, once each — the premises a
/// guard that decided false is false over. A fact pattern's own arguments
/// are part of the pattern, never reads of their own.
pub(crate) fn guard_atoms(expr: &Expr, out: &mut Vec<GuardAtom>) {
    fn push(out: &mut Vec<GuardAtom>, a: GuardAtom) {
        if !out.contains(&a) {
            out.push(a);
        }
    }
    match expr {
        Expr::Path(p) | Expr::Has(p) => {
            if p.contains('.') {
                push(out, GuardAtom::Path(p.to_string()));
            }
        }
        Expr::Index(target, key) if matches!(key.as_ref(), Expr::Lit(Value::Str(_))) => {
            match expr.static_path() {
                Some(path) => push(out, GuardAtom::Path(path)),
                None => guard_atoms(target, out),
            }
        }
        Expr::Index(target, key) if key.is_occasion_target() => match target.static_path() {
            Some(family) => push(out, GuardAtom::Indexed(family)),
            None => guard_atoms(target, out),
        },
        Expr::Call(Func::Holds | Func::Count | Func::CountDistinct, args)
            if matches!(
                args.as_slice(),
                [Expr::Lit(Value::Str(_)), Expr::List(_), ..]
            ) =>
        {
            let [Expr::Lit(Value::Str(rel)), Expr::List(list), ..] = args.as_slice() else {
                return;
            };
            let terms: Vec<String> = list
                .iter()
                .map(|a| match a {
                    Expr::Lit(Value::Str(s)) if lute_manifest::ident::is_ident(s) => s.clone(),
                    Expr::Lit(Value::Str(s)) => format!("\"{s}\""),
                    Expr::Lit(Value::Bool(b)) => b.to_string(),
                    e => e.static_path().unwrap_or_else(|| "_".to_string()),
                })
                .collect();
            push(out, GuardAtom::Fact(format!("{rel}({})", terms.join(", "))));
        }
        Expr::Call(Func::Visited, args) if args.len() == 1 => {
            if let Expr::Lit(Value::Str(id)) = &args[0] {
                push(out, GuardAtom::Visited(id.clone()));
            }
        }
        Expr::Call(_, args) | Expr::List(args) => {
            for a in args {
                guard_atoms(a, out);
            }
        }
        Expr::Not(a) | Expr::Neg(a) => guard_atoms(a, out),
        Expr::And(a, b) | Expr::Or(a, b) | Expr::In(a, b) | Expr::Binary(_, a, b) => {
            guard_atoms(a, out);
            guard_atoms(b, out);
        }
        Expr::Index(a, b) => {
            guard_atoms(a, out);
            guard_atoms(b, out);
        }
        Expr::Cond(c, t, e) => {
            guard_atoms(c, out);
            guard_atoms(t, out);
            guard_atoms(e, out);
        }
        Expr::Lit(_) | Expr::Invalid => {}
    }
}
