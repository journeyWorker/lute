//! `::set` op/type matrix + write policy (dsl §7.3.4, §9.5, dsl 0.2.0 §5.4).
//!
//! Validates a single `::set{Path AssignOp CelExpr}` directive against the
//! inline `state:` schema. The write-policy half is a reusable [`WriteOwner`]
//! classification ([`classify_write`], 0.3.0-forward: a relation owner slots in
//! as one more variant), distinguishing these static errors:
//!
//! - **`E-APP-READONLY`** (§9.5) — the target's tier is `app.*`. `app.*` is
//!   read-only to content; the engine/settings layer owns those writes, so any
//!   `::set{app.*}` is a static error regardless of op or type. This short-
//!   circuits: an `app.*` target is never additionally reported undeclared or
//!   op/type-mismatched (its declaration and value shape are engine business).
//! - **`E-QUEST-RESERVED-WRITE`** (dsl 0.2.0 §5.4) — the target is a RESERVED
//!   quest path (`quest.<id>.state` / `quest.<id>.objectives.<oid>.done`,
//!   §5.2): engine-populated, author-unwritable. Short-circuits identically to
//!   `app.*`.
//! - **`E-QUEST-RESERVED-WRITE`** also covers every `entry.*` target (dsl
//!   0.19.0 §5): `entry.<id>.read` is engine-written and the `entry` root has
//!   no author-writable path at all.
//! - **`E-ENGINE-OWNED-WRITE`** (dsl 0.22.0 §1.2) — the target (or the
//!   declared ancestor it descends from) is declared `owner: engine`: the
//!   engine writes it, content only reads it. Short-circuits like `app.*`.
//! - **`E-UNDECLARED`** (§9.4/§9.5) — a non-`app`/reserved-quest state-tier
//!   target whose path is absent from the inline `state:` schema. `::set` MUST
//!   target a declared path (§7.3.4: "The `Path` MUST be a declared state
//!   path").
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

use crate::cel_paths::{
    is_entry_path, is_reserved_quest_path, state_path_has_hyphen, E_PATH_IDENT,
};
use crate::meta::{namespace_of, Namespace, StateSchema};
use crate::Ctx;

/// A content `::set` of a path declared `owner: engine` (dsl 0.22.0 §1.2).
pub const E_ENGINE_OWNED_WRITE: &str = "E-ENGINE-OWNED-WRITE";

/// The OWNER of a `::set` target path (dsl §9.5, dsl 0.2.0 §5.4): which
/// write-policy tier governs it. 0.3.0-forward: this is the reusable
/// write-policy seam — a future relation write-owner adds a variant here
/// rather than growing ad hoc booleans in [`check_set`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WriteOwner {
    /// An ordinary declared content path — the existing declared/op-type
    /// matrix applies.
    Content,
    /// `app.*` (dsl §9.5): read-only to content, the engine/settings layer
    /// owns these writes.
    AppReadonly,
    /// A reserved `quest.<id>.state` / `quest.<id>.objectives.<oid>.done` path
    /// (dsl 0.2.0 §5.2, §5.4): engine-populated, author-unwritable.
    QuestReserved,
    /// Any `entry.*` path (dsl 0.19.0 §5): `entry.<id>.read` is
    /// engine-written, and the read-only `entry` root has nothing else to
    /// write. Reported with the quest-reserved code — "writing an `entry.*`
    /// path is rejected, as writing a `quest.*` path is".
    EntryReserved,
    /// Any `prev.*` path (dsl 0.23.0 §6): the read-only mirror of the
    /// previous run's `run.*` values, snapshotted by the engine at run end.
    PrevReserved,
    /// Any `clock.*` path (dsl 0.24.0 §1): derived from the clock's `day`
    /// and `slot`, read-only.
    ClockReserved,
    /// A declared path marked `owner: engine` (dsl 0.22.0 §1.2): the engine
    /// writes it at runtime (and `engine:` play steps / trace mocks in the
    /// toolchain); content may only read it.
    Engine,
}

/// Classify a `::set` target path's write owner (dsl §9.5, dsl 0.2.0 §5.4,
/// dsl 0.22.0 §1.2). Path shape decides the reserved tiers; the schema
/// decides `owner: engine` (the exact decl, or the nearest declared ancestor
/// a field path descends from).
pub(crate) fn classify_write(path: &str, schema: &StateSchema) -> WriteOwner {
    if namespace_of(path) == Some(Namespace::App) {
        WriteOwner::AppReadonly
    } else if is_reserved_quest_path(path) {
        WriteOwner::QuestReserved
    } else if is_entry_path(path) {
        WriteOwner::EntryReserved
    } else if crate::cel_paths::is_prev_path(path) {
        WriteOwner::PrevReserved
    } else if lute_manifest::clock::is_clock_path(path) {
        WriteOwner::ClockReserved
    } else if engine_owned(path, schema) {
        WriteOwner::Engine
    } else {
        WriteOwner::Content
    }
}

/// `path` is declared `owner: engine`, directly or through the declared
/// ancestor it is a field of.
fn engine_owned(path: &str, schema: &StateSchema) -> bool {
    let owned = |d: &crate::meta::StateDecl| d.owner == Some(lute_manifest::types::Owner::Engine);
    if let Some(decl) = schema.decls.get(path) {
        return owned(decl);
    }
    schema
        .decls
        .iter()
        .filter(|(k, _)| {
            path.starts_with(k.as_str()) && path.as_bytes().get(k.len()) == Some(&b'.')
        })
        .max_by_key(|(k, _)| k.len())
        .is_some_and(|(_, d)| owned(d))
}

/// Check a `::set` directive's target write-policy and op/type compatibility
/// (dsl §7.3.4, §9.5). Reads nothing from `Ctx` today; it is threaded for
/// parity with the other `check_*` entrypoints and for future modes.
pub fn check_set(set: &Set, schema: &StateSchema, _ctx: &Ctx<'_>) -> Vec<Diagnostic> {
    let mut diags = Vec::new();

    // §8.4 identifier alignment: the `::set` LHS is a CEL-facing state path, so
    // every segment after the tier must be a `CelIdent` (no `-`). Emitted
    // independently of the write-policy / declaredness checks below (a `-` name
    // is illegal regardless), so it survives the `app`/undeclared short-circuits.
    if state_path_has_hyphen(&set.path) {
        diags.push(diag(
            E_PATH_IDENT,
            format!(
                "`::set` target `{}` has a `-` in a state-path segment; CEL-facing names \
                 forbid `-` (dsl §8.4)",
                set.path
            ),
            set.path_span,
        ));
    }

    // Write policy (dsl §9.5, dsl 0.2.0 §5.4): `app.*` is read-only to content;
    // a reserved `quest.<id>.state`/`…objectives.*.done` path is
    // engine-populated and author-unwritable. Both short-circuit — neither is
    // additionally reported undeclared or op/type-mismatched (their
    // declaration and value shape are engine business).
    match classify_write(&set.path, schema) {
        WriteOwner::AppReadonly => {
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
        WriteOwner::QuestReserved => {
            diags.push(diag(
                "E-QUEST-RESERVED-WRITE",
                format!(
                    "`::set` cannot write `{}`: it is a reserved quest path, \
                     engine-populated and author-unwritable (dsl 0.2.0 §5.2, §5.4)",
                    set.path
                ),
                set.path_span,
            ));
            return diags;
        }
        WriteOwner::EntryReserved => {
            diags.push(diag(
                "E-QUEST-RESERVED-WRITE",
                format!(
                    "`::set` cannot write `{}`: `entry.*` paths are reserved — \
                     `entry.<id>.read` is engine-written when an entry is first presented \
                     (dsl 0.19.0 §5)",
                    set.path
                ),
                set.path_span,
            ));
            return diags;
        }
        WriteOwner::PrevReserved => {
            diags.push(diag(
                "E-QUEST-RESERVED-WRITE",
                format!(
                    "`::set` cannot write `{}`: `prev.run.*` is the read-only mirror of the \
                     value `run.*` had when the previous run ended, snapshotted by the engine \
                     (dsl 0.23.0 §6)",
                    set.path
                ),
                set.path_span,
            ));
            return diags;
        }
        WriteOwner::ClockReserved => {
            diags.push(diag(
                "E-QUEST-RESERVED-WRITE",
                format!(
                    "`::set` cannot write `{}`: `clock.*` is derived from the declared clock's \
                     `day` and `slot` paths and is read-only; the engine moves the clock (in \
                     `lute play`, an `advance:` step) (dsl 0.24.0 §1)",
                    set.path
                ),
                set.path_span,
            ));
            return diags;
        }
        WriteOwner::Engine => {
            diags.push(diag(
                E_ENGINE_OWNED_WRITE,
                format!(
                    "`::set` cannot write `{}`: it is declared `owner: engine` — the engine \
                     writes it and content may only read it; in `lute play` write it with an \
                     `engine:` step, in a trace/test with a mock (dsl 0.22.0 §1.2)",
                    set.path
                ),
                set.path_span,
            ));
            return diags;
        }
        WriteOwner::Content => {}
    }

    // The target must resolve to a declared state path (§7.3.4/§9.4). §9.1 admits
    // no bare, un-namespaced state names, so BOTH a declared-tier path absent from
    // the schema AND a non-tier target (`namespace_of == None`, e.g. `foo.bar`)
    // are undeclared and reported — the latter used to be silently accepted (C3).
    let Some(ty) = resolve_type(&set.path, schema) else {
        let msg = if namespace_of(&set.path).is_some() {
            let mut m = format!(
                "`::set` target `{}` is not declared in the `state:` schema (dsl §7.3.4)",
                set.path
            );
            // dsl 0.5.0 §2.2 "did you mean": suggest the nearest declared
            // path within a small edit distance, advisory text only.
            if let Some(sugg) = crate::cel_paths::nearest_declared_path(&set.path, schema, 2) {
                m.push_str(&format!(" — did you mean `{sugg}`?"));
            }
            m
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
        Type::Domain(_) => "domain",
        Type::Entity(_) => "entity",
        Type::SlotId { .. } => "slotId",
        Type::AssetKind(_) => "assetKind",
        Type::NarrativeTime => "narrativeTime",
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
        covered: Vec::new(),
        related: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ctx::Env;
    use crate::meta::StateDecl;
    use lute_core_span::Span;
    use lute_manifest::types::Type;
    use lute_syntax::ast::{CelKind, CelSlot, Set};
    use std::sync::LazyLock;

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
            when: None,
        }
    }

    fn decl(ty: Type, namespace: Namespace) -> StateDecl {
        StateDecl {
            ty,
            default: None,
            namespace,
            owner: None,
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
        schema_of("scene.affect.marina", Type::Number, Namespace::Scene)
    }

    fn ctx() -> Ctx<'static> {
        static ENV: LazyLock<Env> = LazyLock::new(Env::default);
        Ctx {
            env: &ENV,
            in_match: false,
            match_subject: None,
        }
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
            &set("scene.affect.marina", "+=", "1"),
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
            &set("scene.affect.marina", "=", "5"),
            &schema_number(),
            &ctx(),
        );
        assert!(errs.is_empty(), "{errs:?}");
    }

    #[test]
    fn number_multiply_assign_ok() {
        let errs = check_set(
            &set("scene.affect.marina", "*=", "2"),
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

    #[test]
    fn undeclared_target_near_a_declared_path_suggests_it() {
        // dsl 0.5.0 §2.2 "did you mean": `scene.trsut` is one transposition
        // away from the declared `scene.trust`.
        let schema = schema_of("scene.trust", Type::Bool, Namespace::Scene);
        let errs = check_set(&set("scene.trsut", "=", "true"), &schema, &ctx());
        let e = errs
            .iter()
            .find(|e| e.code == "E-UNDECLARED")
            .unwrap_or_else(|| panic!("{errs:?}"));
        assert!(
            e.message.contains("did you mean `scene.trust`"),
            "{}",
            e.message
        );
    }
}
