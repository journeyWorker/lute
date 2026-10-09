//! Portable expression AST (`expr`) for CEL slots (IR addendum A7): the wire
//! type the compiler lowers every CEL slot to and a runtime reads.
//!
//! ## Serialized shape (byte-stability contract)
//! [`ExprNode`] is a serde **`untagged`** enum: each variant serializes as its
//! bare struct body (no discriminant), producing exactly these JSON shapes —
//! field declaration order = serialized order:
//! - literal   → `{"lit": <int|double|bool|string>}` (numeric literals retain
//!   their `int` or `double` kind)
//! - path      → `{"path": "user.level"}`
//! - unary     → `{"op": "!"|"-", "l": <node>}`
//! - binary    → `{"op": "<sym>", "l": <node>, "r": <node>}` where `<sym>` ∈
//!   `&& || == != < <= > >= + - * / % in` (`%` is integer remainder, dsl
//!   0.24.0 §1)
//! - ternary   → `{"cond": <node>, "then": <node>, "else": <node>}`
//! - list      → `{"list": [<node>, ...]}`
//! - `has(p)`  → `{"has": "<path>"}`

use serde::{Deserialize, Deserializer, Serialize};

/// One node of the portable expression AST (dsl §8.4 profile). See the module
/// docs for the exact serialized JSON shape of each variant.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub enum ExprNode {
    /// Typed scalar literal, flattened to `{int}`, `{double}`, `{bool}` or `{string}`.
    Lit { #[serde(flatten)] lit: LitVal },
    /// Static state/subject path: `{"path": "a.b.c"}`.
    Path { path: String },
    /// Unary operator (`!`/`-`): `{"op": "<sym>", "l": <node>}`.
    Unary { op: &'static str, l: Box<ExprNode> },
    /// Binary operator: `{"op": "<sym>", "l": <node>, "r": <node>}`.
    Binary {
        op: &'static str,
        l: Box<ExprNode>,
        r: Box<ExprNode>,
    },
    /// Ternary conditional: `{"cond": <node>, "then": <node>, "else": <node>}`.
    Cond {
        cond: Box<ExprNode>,
        then: Box<ExprNode>,
        #[serde(rename = "else")]
        otherwise: Box<ExprNode>,
    },
    /// List literal.
    List { list: Vec<ExprNode> },
    /// Computed map/list index.
    Index { index: Box<ExprNode>, key: Box<ExprNode> },
    /// Numeric conversion or engine host function.
    Call { call: String, args: Vec<ExprNode> },
    /// Presence test over a canonical path.
    Has { has: String },
}

/// A scalar literal value. Serialized untagged, so it emits a bare JSON number,
/// bool, or string as the value of the `lit` field. All numeric CEL literals
/// (`Int`/`UInt`/`Double`) collapse to an f64 double.
#[derive(Clone, Debug, PartialEq)]
pub enum LitVal {
    Int(i64),
    Num(f64),
    Bool(bool),
    Str(String),
}

impl serde::Serialize for LitVal {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut out = serializer.serialize_struct("LitVal", 1)?;
        match self {
            LitVal::Int(v) => out.serialize_field("int", v)?,
            LitVal::Num(v) => out.serialize_field("double", v)?,
            LitVal::Bool(v) => out.serialize_field("bool", v)?,
            LitVal::Str(v) => out.serialize_field("string", v)?,
        }
        out.end()
    }
}

/// Reads exactly one of `int`, `double`, `bool`, `string`.
impl<'de> Deserialize<'de> for LitVal {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Raw {
            int: Option<i64>,
            double: Option<f64>,
            bool: Option<bool>,
            string: Option<String>,
        }
        match Raw::deserialize(d)? {
            Raw { int: Some(v), double: None, bool: None, string: None } => Ok(Self::Int(v)),
            Raw { int: None, double: Some(v), bool: None, string: None } => Ok(Self::Num(v)),
            Raw { int: None, double: None, bool: Some(v), string: None } => Ok(Self::Bool(v)),
            Raw { int: None, double: None, bool: None, string: Some(v) } => Ok(Self::Str(v)),
            _ => Err(serde::de::Error::custom("invalid expression literal")),
        }
    }
}

/// The operator symbols an `op` field may hold (module docs).
const OPS: &[&str] = &[
    "!", "-", "&&", "||", "==", "!=", "<", "<=", ">", ">=", "+", "*", "/", "%", "in",
];

/// Reads the shapes the module docs list; an `op` outside them is an error.
impl<'de> Deserialize<'de> for ExprNode {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        // Untagged: the first shape whose fields are all present wins, and
        // extra fields are ignored — so `Binary` (`op`, `l`, `r`) is tried
        // before `Unary` (`op`, `l`), which would otherwise drop `r`.
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Lit {
                #[serde(flatten)]
                lit: LitVal,
            },
            Path {
                path: String,
            },
            Binary {
                op: String,
                l: Box<Raw>,
                r: Box<Raw>,
            },
            Unary {
                op: String,
                l: Box<Raw>,
            },
            Cond {
                cond: Box<Raw>,
                then: Box<Raw>,
                #[serde(rename = "else")]
                otherwise: Box<Raw>,
            },
            List {
                list: Vec<Raw>,
            },
            Index {
                index: Box<Raw>,
                key: Box<Raw>,
            },
            Call {
                call: String,
                args: Vec<Raw>,
            },
            Has {
                has: String,
            },
        }
        fn op<E: serde::de::Error>(op: String) -> Result<&'static str, E> {
            OPS.iter()
                .find(|known| **known == op)
                .copied()
                .ok_or_else(|| E::custom(format!("invalid expression operator {op:?}")))
        }
        fn node<E: serde::de::Error>(raw: Raw) -> Result<ExprNode, E> {
            let boxed = |raw: Box<Raw>| node(*raw).map(Box::new);
            let all = |raws: Vec<Raw>| raws.into_iter().map(node).collect::<Result<Vec<_>, E>>();
            Ok(match raw {
                Raw::Lit { lit } => ExprNode::Lit { lit },
                Raw::Path { path } => ExprNode::Path { path },
                Raw::Unary { op: o, l } => ExprNode::Unary { op: op(o)?, l: boxed(l)? },
                Raw::Binary { op: o, l, r } => ExprNode::Binary {
                    op: op(o)?,
                    l: boxed(l)?,
                    r: boxed(r)?,
                },
                Raw::Cond { cond, then, otherwise } => ExprNode::Cond {
                    cond: boxed(cond)?,
                    then: boxed(then)?,
                    otherwise: boxed(otherwise)?,
                },
                Raw::List { list } => ExprNode::List { list: all(list)? },
                Raw::Index { index, key } => ExprNode::Index {
                    index: boxed(index)?,
                    key: boxed(key)?,
                },
                Raw::Call { call, args } => ExprNode::Call { call, args: all(args)? },
                Raw::Has { has } => ExprNode::Has { has },
            })
        }
        node(Raw::deserialize(d)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b(node: ExprNode) -> Box<ExprNode> {
        Box::new(node)
    }

    /// What the compiler writes, a reader reads back unchanged — every
    /// shape, nested, including a binary node beside a unary one.
    #[test]
    fn every_shape_reads_back_as_written() {
        let path = |p: &str| ExprNode::Path { path: p.into() };
        let lit = |lit| ExprNode::Lit { lit };
        let node = ExprNode::Cond {
            cond: b(ExprNode::Binary {
                op: "&&",
                l: b(ExprNode::Unary { op: "!", l: b(ExprNode::Has { has: "run.a".into() }) }),
                r: b(ExprNode::Binary {
                    op: "in",
                    l: b(lit(LitVal::Str("x".into()))),
                    r: b(ExprNode::List { list: vec![lit(LitVal::Int(3)), lit(LitVal::Bool(true))] }),
                }),
            }),
            then: b(ExprNode::Index {
                index: b(path("user.map")),
                key: b(ExprNode::Call { call: "int".into(), args: vec![lit(LitVal::Num(2.5))] }),
            }),
            otherwise: b(ExprNode::Unary { op: "-", l: b(path("run.n")) }),
        };
        let json = serde_json::to_value(&node).unwrap();
        assert_eq!(serde_json::from_value::<ExprNode>(json).unwrap(), node);
    }

    #[test]
    fn an_unknown_operator_is_refused() {
        let json = serde_json::json!({ "op": "<>", "l": { "path": "a" }, "r": { "path": "b" } });
        assert!(serde_json::from_value::<ExprNode>(json).is_err());
    }
}
