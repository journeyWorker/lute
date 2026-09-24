//! dsl 0.21.0 §7b: the declaration shape of one `defs:` entry — `E-DEF-DECL`.
//!
//! A def is either the SHORTHAND `name: "<CEL>"` or the long form
//! `name: { type?: <Type>, params?: { p: <Type> }, cel: "<CEL>" }`. Both lift
//! to one canonical mapping here, so every consumer of `TypedMeta.defs` reads
//! the body from `cel:` and never has to know the shorthand exists — the
//! defect this module closes was exactly a def whose NAME resolved while its
//! body was invisible to `fold_env`, which then passed `check` and failed
//! `compile` with `E-COMPILE-EXPAND`.
//!
//! Two stages, because they need different context:
//!
//! 1. [`lift_def`] — structural, context-free, run by the meta parse on every
//!    document (a scene's frontmatter, an imported `.schema.yaml`, a
//!    standalone `lute check <schema.yaml>`).
//! 2. [`settle_def_type`] — the produced type. An absent `type:` is INFERRED
//!    from the body by the same closed procedure `E-SET-TYPE` uses
//!    ([`crate::set_type::decide_raw`]), against the folded state schema, so
//!    it runs in `fold_env` where that schema exists.

use lute_manifest::types::Type;
use serde_yaml::Value;

use crate::cel_resolve::{compatible, ty_desc};
use crate::ctx::ExpectedType;
use crate::meta::{yaml_shape, StateSchema};
use crate::set_type::{decide_raw, Decision};

pub const E_DEF_DECL: &str = "E-DEF-DECL";

/// The keys a long-form def may carry (dsl 0.1.0 §8.1).
const DEF_KEYS: [&str; 3] = ["type", "params", "cel"];

/// Lift one `defs:` value to its canonical long-form mapping, or return the
/// `E-DEF-DECL` message naming what is wrong with it.
///
/// Each arm returns its message WHOLE as a single `format!` literal, the
/// convention `state_decl_message` documents: `scripts/check-doc-snippets.py`
/// pins quoted diagnostics against the scraped literals in `crates/*/src`.
pub(crate) fn lift_def(name: &str, v: &Value) -> Result<Value, String> {
    let map = match v {
        Value::String(cel) => {
            let mut m = serde_yaml::Mapping::new();
            m.insert(Value::String("cel".into()), Value::String(cel.clone()));
            return Ok(Value::Mapping(m));
        }
        Value::Mapping(m) => m,
        other => {
            return Err(format!(
                "invalid def `{name}`: a def is a CEL string, `{name}: \"<CEL>\"`, or a \
                 mapping, `{name}: {{ type: bool, cel: \"…\" }}` — but this is {} \
                 (dsl 0.21.0 §7b)",
                yaml_shape(other)
            ))
        }
    };
    if let Some(key) = map
        .keys()
        .find(|k| !k.as_str().is_some_and(|k| DEF_KEYS.contains(&k)))
    {
        let key = key.as_str().map_or_else(|| yaml_shape(key).to_string(), str::to_string);
        return Err(format!(
            "invalid def `{name}`: `{key}:` is not a def key; a def is \
             `{{ type: bool, cel: \"…\" }}` with an optional `params: {{ p: <type> }}`, or \
             the shorthand `{name}: \"<CEL>\"` (dsl 0.21.0 §7b)"
        ));
    }
    match v.get("cel") {
        Some(Value::String(_)) => {}
        None => {
            return Err(format!(
                "invalid def `{name}`: the def has no `cel:` key, the CEL body it expands \
                 to — write `{name}: {{ type: <bool|number|enum>, cel: \"…\" }}`, or the \
                 shorthand `{name}: \"<CEL>\"` (dsl 0.21.0 §7b)"
            ))
        }
        Some(other) => {
            return Err(format!(
                "invalid def `{name}`: `cel:` must be a string of CEL, but this is {}; quote \
                 it, as `{{ type: bool, cel: \"…\" }}` (dsl 0.21.0 §7b)",
                yaml_shape(other)
            ))
        }
    }
    match v.get("type") {
        Some(t) if serde_yaml::from_value::<Type>(t.clone()).is_err() => Err(format!(
            "invalid def `{name}`: `type:` is not a type; a def produces `bool`, `number`, \
             `string` or an `enum`, as `{{ type: bool, cel: \"…\" }}` or \
             `{{ type: {{ enum: [...] }}, cel: \"…\" }}` — or drop `type:` and let the body's \
             type be inferred (dsl 0.21.0 §7b)"
        )),
        None if v.get("params").is_some() => Err(format!(
            "invalid def `{name}`: a def with `params:` must declare its `type:` — its body \
             reads parameters whose values arrive at each call, so its type cannot be \
             inferred; write `{{ type: <bool|number|enum>, params: {{ p: <type> }}, cel: \"…\" }}` \
             (dsl 0.21.0 §7b)"
        )),
        _ => Ok(v.clone()),
    }
}

/// Settle a LIFTED def's produced type against the folded `schema`
/// (dsl 0.21.0 §7b). An absent `type:` is inferred from the body and written
/// into `def`, so every downstream reader of `type:` sees it exactly as if
/// the author had spelled it. An explicit `type:` is kept, and must agree with
/// the body's type whenever that type is decidable.
///
/// Returns the `E-DEF-DECL` message when the type is neither inferable nor
/// consistent. Returns `None` — and changes nothing — for a def [`lift_def`]
/// already rejected, and for a body that does not parse (malformed CEL is
/// `E-CEL-PARSE`'s, never a type question).
pub(crate) fn settle_def_type(name: &str, def: &mut Value, schema: &StateSchema) -> Option<String> {
    let cel = def.get("cel")?.as_str()?.to_string();
    let declared = match def.get("type") {
        Some(t) => Some(serde_yaml::from_value::<Type>(t.clone()).ok()?),
        None => None,
    };
    let decision = decide_raw(&cel, schema)?;
    match (declared, decision) {
        (Some(declared), Decision::Ty(produced)) => {
            if compatible(&produced, &ExpectedType::Ty(declared.clone())) {
                None
            } else {
                Some(format!(
                    "def `{name}` declares `type:` {} but its body `{cel}` produces {} — drop \
                     `type:` to take the body's type, or fix the body (dsl 0.21.0 §7b)",
                    ty_desc(&declared),
                    ty_desc(&produced)
                ))
            }
        }
        (Some(_), _) => None,
        // `lift_def` rejects `params:` without `type:`; nothing to settle.
        (None, _) if def.get("params").is_some() => None,
        (None, Decision::Ty(produced)) => {
            let ty = serde_yaml::to_value(&produced).ok()?;
            if let Value::Mapping(m) = def {
                m.insert(Value::String("type".into()), ty);
            }
            None
        }
        (None, Decision::Ill(what)) => Some(format!(
            "def `{name}` has no `type:`, and its body `{cel}` is ill-typed: {what} \
             (dsl 0.21.0 §7b)"
        )),
        // 0.21.1 T1-6: the hint names the type as a placeholder. It used to
        // say `type: bool` here — a guess, and wrong for `run.day % 7`.
        (None, Decision::Undecidable) => Some(format!(
            "def `{name}` has no `type:`, and the type its body `{cel}` produces cannot be \
             inferred; write the long form, `{name}: {{ type: <bool|number|enum>, cel: {cel:?} }}`, \
             naming the type it produces (dsl 0.21.0 §7b)"
        )),
    }
}
