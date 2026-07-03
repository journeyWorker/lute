//! `::set` op/type matrix + write policy (dsl §7.3.4, §9.5).
//!
//! Validates a single `::set{Path AssignOp CelExpr}` directive against the
//! inline `state:` schema. Three static errors, kept distinct:
//!
//! - **`E-APP-READONLY`** (§9.5) — the target's tier is `app.*`. `app.*` is
//!   read-only to content; the engine/settings layer owns those writes, so any
//!   `::set{app.*}` is a static error regardless of op or type. This short-
//!   circuits: an `app.*` target is never additionally reported undeclared or
//!   op/type-mismatched (its declaration and value shape are engine business).
//! - **`E-UNDECLARED`** (§9.4/§9.5) — a non-`app` state-tier target whose path is
//!   absent from the inline `state:` schema. `::set` MUST target a declared path
//!   (§7.3.4: "The `Path` MUST be a declared state path").
//! - **`E-SET-OP-TYPE`** (§7.3.4) — the `AssignOp` is incompatible with the
//!   declared type of the target. `=` is a pure write, valid for any type;
//!   the compound/arithmetic ops `+=`/`-=`/`*=` read-modify-write a numeric
//!   accumulator and are valid only when the target's declared type is
//!   `number`. A compound op on a `bool`/`str`/`enum`/… target is an error.
//!
//! This module does NOT perform definite-assignment (dsl §9.4, [`crate::defassign`],
//! `E-MAYBE-UNSET`) nor RHS value-type compatibility (T4.3/T4.6 territory); it is
//! the op/type/write-policy matrix only.

use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_manifest::types::Type;
use lute_syntax::ast::Set;

use crate::meta::{namespace_of, Namespace, StateSchema};
use crate::Ctx;

/// Check a `::set` directive's target write-policy and op/type compatibility
/// (dsl §7.3.4, §9.5). Reads nothing from `Ctx` today; it is threaded for
/// parity with the other `check_*` entrypoints and for future modes.
pub fn check_set(set: &Set, schema: &StateSchema, _ctx: &Ctx) -> Vec<Diagnostic> {
    let mut diags = Vec::new();

    // §9.5 write policy: `app.*` is read-only to content. Short-circuit — the
    // engine owns app declarations and their value shapes, so we neither report
    // it undeclared nor run the op/type matrix on it.
    if namespace_of(&set.path) == Some(Namespace::App) {
        diags.push(diag(
            "E-APP-READONLY",
            format!(
                "`::set` cannot write `{}`: the `app.*` namespace is read-only to content \
                 (dsl §9.5); the engine/settings layer owns these writes",
                set.path
            ),
            set.path_span,
        ));
        return diags;
    }

    // The target must resolve to a declared state path (§7.3.4/§9.4). §9.1 admits
    // no bare, un-namespaced state names, so BOTH a declared-tier path absent from
    // the schema AND a non-tier target (`namespace_of == None`, e.g. `foo.bar`)
    // are undeclared and reported — the latter used to be silently accepted (C3).
    let Some(ty) = resolve_type(&set.path, schema) else {
        let msg = if namespace_of(&set.path).is_some() {
            format!(
                "`::set` target `{}` is not declared in the `state:` schema (dsl §7.3.4)",
                set.path
            )
        } else {
            format!(
                "`::set` target `{}` is not a state path: it must begin with a \
                 `scene.`/`run.`/`user.`/`app.` namespace (dsl §7.3.4/§9.1)",
                set.path
            )
        };
        diags.push(diag("E-UNDECLARED", msg, set.path_span));
        return diags;
    };

    // §7.3.4 op/type matrix. `=` is a pure write (any type); the compound ops
    // read-modify-write a numeric accumulator and require a `number` target.
    if is_compound_op(&set.op) && ty != &Type::Number {
        diags.push(diag(
            "E-SET-OP-TYPE",
            format!(
                "compound assignment `{}` requires a `number` target, but `{}` is declared `{}` \
                 (dsl §7.3.4)",
                set.op,
                set.path,
                type_name(ty)
            ),
            set.span,
        ));
    }

    diags
}

/// `+=` / `-=` / `*=` are compound (read-modify-write) ops; `=` is a pure write.
fn is_compound_op(op: &str) -> bool {
    matches!(op, "+=" | "-=" | "*=")
}

/// Resolve the declared [`Type`] of a state path against the schema: an exact
/// `state:` key, or a descendant field reached by walking `Record`/`Map` types
/// from the nearest declared ancestor. Returns `None` when no declared ancestor
/// covers the path (→ `E-UNDECLARED`).
pub(crate) fn resolve_type<'s>(path: &str, schema: &'s StateSchema) -> Option<&'s Type> {
    if let Some(decl) = schema.decls.get(path) {
        return Some(&decl.ty);
    }
    // Nearest declared ancestor (longest matching key prefix), then descend the
    // remaining dotted segments through Record fields / Map values.
    let (key, decl) = schema
        .decls
        .iter()
        .filter(|(k, _)| path.starts_with(&format!("{k}.")))
        .max_by_key(|(k, _)| k.len())?;
    let rest = &path[key.len() + 1..];
    descend(&decl.ty, rest)
}

/// Walk `ty` through the remaining `.`-separated `segments`, following `Record`
/// field types and `Map` value types. `None` if a segment has no field / the
/// type is not descendable.
fn descend<'s>(ty: &'s Type, segments: &str) -> Option<&'s Type> {
    let mut cur = ty;
    for seg in segments.split('.') {
        cur = match cur {
            Type::Record(fields) => &fields.iter().find(|f| f.name == seg)?.ty,
            Type::Map { value, .. } => value,
            _ => return None,
        };
    }
    Some(cur)
}

/// Human-readable type name for diagnostics.
fn type_name(ty: &Type) -> &'static str {
    match ty {
        Type::Bool => "bool",
        Type::Number => "number",
        Type::Str => "str",
        Type::Enum(_) => "enum",
        Type::List(_) => "list",
        Type::Record(_) => "record",
        Type::Map { .. } => "map",
        Type::EnumFromOption(_) => "enumFromOption",
        Type::ProviderRef(_) => "providerRef",
        Type::SlotId { .. } => "slotId",
        Type::AssetKind(_) => "assetKind",
    }
}

/// Build a `Layer::Staging` error diagnostic (`::set` is a staging directive,
/// dsl §7.3.4).
fn diag(code: &str, message: String, span: Span) -> Diagnostic {
    Diagnostic {
        code: code.to_string(),
        severity: Severity::Error,
        message,
        span,
        layer: Layer::Staging,
        fixits: Vec::new(),
        provenance: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meta::StateDecl;
    use lute_core_span::Span;
    use lute_manifest::types::Type;
    use lute_syntax::ast::{CelKind, CelSlot, Set};

    fn test_span() -> Span {
        Span {
            byte_start: 0,
            byte_end: 0,
            line: 1,
            column: 1,
            utf16_range: (0, 0),
        }
    }

    fn set(path: &str, op: &str, rhs: &str) -> Set {
        Set {
            path: path.to_string(),
            path_span: test_span(),
            op: op.to_string(),
            expr: CelSlot::raw(CelKind::SetExpr, rhs.to_string(), test_span()),
            span: test_span(),
        }
    }

    fn decl(ty: Type, namespace: Namespace) -> StateDecl {
        StateDecl {
            ty,
            default: None,
            namespace,
        }
    }

    fn schema_of(path: &str, ty: Type, namespace: Namespace) -> StateSchema {
        let mut s = StateSchema::default();
        s.decls.insert(path.to_string(), decl(ty, namespace));
        s
    }

    fn schema_app_lang() -> StateSchema {
        schema_of("app.lang", Type::Str, Namespace::App)
    }

    fn schema_bool_flag() -> StateSchema {
        schema_of("scene.flags.saw", Type::Bool, Namespace::Scene)
    }

    fn schema_number() -> StateSchema {
        schema_of("scene.affect.bianca", Type::Number, Namespace::Scene)
    }

    fn ctx() -> Ctx {
        Ctx::default()
    }

    #[test]
    fn app_write_errors() {
        let errs = check_set(&set("app.lang", "=", "'en'"), &schema_app_lang(), &ctx());
        assert!(errs.iter().any(|e| e.code == "E-APP-READONLY"));
    }

    #[test]
    fn bool_compound_assign_errors() {
        let errs = check_set(
            &set("scene.flags.saw", "+=", "1"),
            &schema_bool_flag(),
            &ctx(),
        );
        assert!(errs.iter().any(|e| e.code == "E-SET-OP-TYPE"));
    }

    #[test]
    fn number_increment_ok() {
        let errs = check_set(
            &set("scene.affect.bianca", "+=", "1"),
            &schema_number(),
            &ctx(),
        );
        assert!(errs.is_empty(), "{errs:?}");
    }

    // ---- Coverage beyond the brief's three (matrix corners) -----------------

    #[test]
    fn undeclared_state_target_errors() {
        // A state-tier path absent from the schema → E-UNDECLARED.
        let errs = check_set(&set("run.hp", "=", "1"), &StateSchema::default(), &ctx());
        assert!(errs.iter().any(|e| e.code == "E-UNDECLARED"), "{errs:?}");
    }

    #[test]
    fn bool_plain_assign_ok() {
        // `=` is a pure write, valid for a bool target.
        let errs = check_set(
            &set("scene.flags.saw", "=", "true"),
            &schema_bool_flag(),
            &ctx(),
        );
        assert!(errs.is_empty(), "{errs:?}");
    }

    #[test]
    fn number_plain_assign_ok() {
        let errs = check_set(
            &set("scene.affect.bianca", "=", "5"),
            &schema_number(),
            &ctx(),
        );
        assert!(errs.is_empty(), "{errs:?}");
    }

    #[test]
    fn number_multiply_assign_ok() {
        let errs = check_set(
            &set("scene.affect.bianca", "*=", "2"),
            &schema_number(),
            &ctx(),
        );
        assert!(errs.is_empty(), "{errs:?}");
    }

    #[test]
    fn str_compound_assign_errors() {
        // `+=` on a str target is an op/type mismatch (numbers only).
        let errs = check_set(&set("app.lang", "+=", "'x'"), &schema_app_lang(), &ctx());
        // app.* short-circuits to read-only before the op/type matrix runs.
        assert!(errs.iter().any(|e| e.code == "E-APP-READONLY"), "{errs:?}");
        assert!(!errs.iter().any(|e| e.code == "E-SET-OP-TYPE"), "{errs:?}");
    }

    #[test]
    fn descended_record_field_type_gates_op() {
        // `run.player` declared as a record; `run.player.name` is a str field →
        // a compound op on it is E-SET-OP-TYPE.
        let mut schema = StateSchema::default();
        schema.decls.insert(
            "run.player".to_string(),
            decl(
                Type::Record(vec![lute_manifest::types::Field {
                    name: "name".to_string(),
                    ty: Type::Str,
                    default: None,
                    required: false,
                    shape: None,
                }]),
                Namespace::Run,
            ),
        );
        let errs = check_set(&set("run.player.name", "+=", "'x'"), &schema, &ctx());
        assert!(errs.iter().any(|e| e.code == "E-SET-OP-TYPE"), "{errs:?}");
    }

    #[test]
    fn non_state_target_errors() {
        // C3 (dsl §7.3.4/§9.1): a `::set` target with no known tier (`foo.bar`) is
        // not a declared state path — §9.1 admits no bare, un-namespaced names — so
        // it MUST be reported, not silently accepted.
        let errs = check_set(&set("foo.bar", "+=", "1"), &StateSchema::default(), &ctx());
        assert!(
            errs.iter().any(|e| e.code == "E-UNDECLARED"),
            "non-tier ::set target must be diagnosed, got {errs:?}"
        );
    }
}
