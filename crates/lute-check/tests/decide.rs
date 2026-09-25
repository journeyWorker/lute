//! Task 2 (0.4.0): `decide()` — the §5.1 decided-constant fragment
//! (`dsl 0.4.0 §5.1`). Exercises R1–R5 exactly (the Closure clause forbids
//! anything stronger), plus the two soundness pins this release hinges on:
//! an in-domain comparison NEVER decides (`soundness_no_guessing` — a §5
//! error on a document with a satisfying run would be a conformance bug),
//! and `@def` expansion runs BEFORE deciding (`def_expansion_precedes`, D2).

use std::collections::BTreeMap;

use lute_check::match_check::{Domain, DomainValue};
use lute_check::meta::{Namespace, StateDecl, StateSchema};
use lute_check::{decide_slot, DecideCtx, Decided, DefTable, DollarBinding, DomainInfo};
use lute_manifest::types::{Literal, Type};

/// `run.rank: enum [fail, bronze, silver, gold]`, `run.flag: bool` (default
/// `false`), `run.n: number` (default `0`) — mirrors the plan's Step 1 schema
/// — plus `run.m: number` with no default (maybe unset).
fn schema() -> StateSchema {
    let mut decls = BTreeMap::new();
    decls.insert(
        "run.rank".to_string(),
        StateDecl {
            ty: Type::Enum(vec![
                "fail".to_string(),
                "bronze".to_string(),
                "silver".to_string(),
                "gold".to_string(),
            ]),
            default: None,
            namespace: Namespace::Run,
            owner: None,
        },
    );
    decls.insert(
        "run.flag".to_string(),
        StateDecl {
            ty: Type::Bool,
            default: Some(Literal::Bool(false)),
            namespace: Namespace::Run,
            owner: None,
        },
    );
    decls.insert(
        "run.n".to_string(),
        StateDecl {
            ty: Type::Number,
            default: Some(Literal::Num(0.0)),
            namespace: Namespace::Run,
            owner: None,
        },
    );
    decls.insert(
        "run.m".to_string(),
        StateDecl {
            ty: Type::Number,
            default: None,
            namespace: Namespace::Run,
            owner: None,
        },
    );
    StateSchema { decls }
}

/// The same finite domain `run.rank` infers to, built by hand (`infer_domain`
/// itself is `pub(crate)`, not reachable from this external test crate).
fn rank_domain() -> DomainInfo {
    DomainInfo {
        domain: Domain::Finite(vec![
            DomainValue::Str("fail".to_string()),
            DomainValue::Str("bronze".to_string()),
            DomainValue::Str("silver".to_string()),
            DomainValue::Str("gold".to_string()),
        ]),
        maybe_unset: false,
        resolved: true,
    }
}

fn d(raw: &str) -> Option<Decided> {
    let schema = schema();
    let params = BTreeMap::new();
    let ctx = DecideCtx {
        schema: &schema,
        dollar: None,
        params: &params,
        facts: None,
    };
    let bodies = BTreeMap::new();
    let def_params = BTreeMap::new();
    let defs = DefTable {
        bodies: &bodies,
        params: &def_params,
    };
    decide_slot(raw, &defs, &ctx)
}

fn d_dollar(raw: &str) -> Option<Decided> {
    let schema = schema();
    let params = BTreeMap::new();
    let dom = rank_domain();
    let ctx = DecideCtx {
        schema: &schema,
        dollar: Some(DollarBinding::Domain(&dom)),
        params: &params,
        facts: None,
    };
    let bodies = BTreeMap::new();
    let def_params = BTreeMap::new();
    let defs = DefTable {
        bodies: &bodies,
        params: &def_params,
    };
    decide_slot(raw, &defs, &ctx)
}

fn d_val(raw: &str) -> Option<Decided> {
    let schema = schema();
    let params = BTreeMap::new();
    let ctx = DecideCtx {
        schema: &schema,
        dollar: Some(DollarBinding::Value(Decided::Str("fond".to_string()))),
        params: &params,
        facts: None,
    };
    let bodies = BTreeMap::new();
    let def_params = BTreeMap::new();
    let defs = DefTable {
        bodies: &bodies,
        params: &def_params,
    };
    decide_slot(raw, &defs, &ctx)
}

fn d_param(raw: &str) -> Option<Decided> {
    let schema = schema();
    let mut params = BTreeMap::new();
    params.insert(
        "tier".to_string(),
        DomainInfo {
            domain: Domain::Finite(vec![
                DomainValue::Str("cold".to_string()),
                DomainValue::Str("warm".to_string()),
                DomainValue::Str("fond".to_string()),
            ]),
            maybe_unset: false,
            resolved: true,
        },
    );
    let ctx = DecideCtx {
        schema: &schema,
        dollar: None,
        params: &params,
        facts: None,
    };
    let bodies = BTreeMap::new();
    let def_params = BTreeMap::new();
    let defs = DefTable {
        bodies: &bodies,
        params: &def_params,
    };
    decide_slot(raw, &defs, &ctx)
}

fn d_defs(raw: &str) -> Option<Decided> {
    let schema = schema();
    let params = BTreeMap::new();
    let ctx = DecideCtx {
        schema: &schema,
        dollar: None,
        params: &params,
        facts: None,
    };
    let mut bodies = BTreeMap::new();
    bodies.insert("never".to_string(), "1 > 2".to_string());
    let def_params = BTreeMap::new();
    let defs = DefTable {
        bodies: &bodies,
        params: &def_params,
    };
    decide_slot(raw, &defs, &ctx)
}

/// A cyclic `DefTable` (`a` -> `@b`, `b` -> `@a`) — `expand_ref`'s `stack`
/// guard (`cel_expand.rs`) must catch this and bail with `Err`, which
/// `decide_slot` falls back on (D3) rather than looping forever.
fn d_cycle(raw: &str) -> Option<Decided> {
    let schema = schema();
    let params = BTreeMap::new();
    let ctx = DecideCtx {
        schema: &schema,
        dollar: None,
        params: &params,
        facts: None,
    };
    let mut bodies = BTreeMap::new();
    bodies.insert("a".to_string(), "@b".to_string());
    bodies.insert("b".to_string(), "@a".to_string());
    let def_params = BTreeMap::new();
    let defs = DefTable {
        bodies: &bodies,
        params: &def_params,
    };
    decide_slot(raw, &defs, &ctx)
}

#[test]
fn r1_literals() {
    assert_eq!(d("true"), Some(Decided::Bool(true)));
    assert_eq!(d("3"), Some(Decided::Num(3.0)));
}

#[test]
fn r3_ground_ops() {
    assert_eq!(d("1 > 2"), Some(Decided::Bool(false)));
    assert_eq!(d("2 + 3 == 5"), Some(Decided::Bool(true)));
    assert_eq!(d("'a' == 'b'"), Some(Decided::Bool(false)));
}

#[test]
fn r4_connectives() {
    assert_eq!(d("1 > 2 && run.flag"), Some(Decided::Bool(false)));
    assert_eq!(d("1 < 2 || run.flag"), Some(Decided::Bool(true)));
    assert_eq!(d("!(1 > 2)"), Some(Decided::Bool(true)));
    assert_eq!(d("1 > 2 ? run.n : 7"), Some(Decided::Num(7.0)));
}

/// R4 non-short-circuit boundary (dsl 0.4.0 §5.1): a decided operand on the
/// side that does NOT short-circuit its connective must never promote the
/// whole expression to decided — the other, undecided operand could still
/// flip the runtime result. Only a decided `false` (`&&`) / decided `true`
/// (`||`) short-circuits; `decide()` must NEVER collect-then-`apply_op`.
#[test]
fn r4_non_short_circuit_is_undecided() {
    for e in [
        "1 < 2 && run.flag",
        "run.flag && true",
        "1 > 2 || run.flag",
        "run.flag || false",
    ] {
        assert_eq!(d(e), None, "{e}");
    }
}

#[test]
fn r2_domain_membership() {
    assert_eq!(d("run.rank == 'platnum'"), Some(Decided::Bool(false)));
    assert_eq!(d("run.rank != 'platnum'"), Some(Decided::Bool(true)));
    assert_eq!(d("run.rank == 'gold'"), None);
    assert_eq!(
        d("run.rank in ['platnum', 'wood']"),
        Some(Decided::Bool(false))
    );
}

/// R2 must DECLINE (not guess) whenever the subject's domain isn't finite,
/// the subject is undeclared, or a literal/list gives the runtime value
/// room to still land on the subject's side (dsl 0.4.0 §5.1 R2).
#[test]
fn r2_declines_on_unresolved_or_ambiguous() {
    // `run.n` is `Number` -> `infer_domain` always returns `Infinite`; R2
    // requires a FINITE domain (bool/enum only).
    assert_eq!(d("run.n == 1"), None);
    // An undeclared path has no schema decl -> `Infinite`/unresolved too.
    assert_eq!(d("run.ghost == 'x'"), None);
    // `'gold'` is a genuine `run.rank` member: the runtime value MIGHT be
    // `'gold'`, so `in` can't decide `false` (non-empty intersection).
    assert_eq!(d("run.rank in ['gold', 'platnum']"), None);
    // A non-literal element (`run.flag`) can't be ruled out as a match for
    // the eventual subject value, even though every OTHER element is
    // foreign to the domain.
    assert_eq!(d("run.rank in ['wood', run.flag]"), None);
}

#[test]
fn r2_dollar_domain() {
    assert_eq!(d_dollar("$ == 'gone'"), Some(Decided::Bool(false)));
    assert_eq!(d_dollar("$ == 'gold'"), None);
}

/// `$`'s R2 boundary: an in-domain literal on the other side of `!=`/`in`
/// must decline exactly like a named-path subject does; the all-foreign
/// mirror (every literal outside the domain) still decides.
#[test]
fn r2_dollar_boundary_and_all_foreign() {
    assert_eq!(d_dollar("$ != 'gold'"), None);
    assert_eq!(d_dollar("$ in ['gold']"), None);
    assert_eq!(d_dollar("$ != 'gone'"), Some(Decided::Bool(true)));
    assert_eq!(d_dollar("$ in ['gone']"), Some(Decided::Bool(false)));
}

#[test]
fn r2_dollar_value() {
    assert_eq!(d_val("$ == 'fond'"), Some(Decided::Bool(true)));
    assert_eq!(d_val("$ == 'cold'"), Some(Decided::Bool(false)));
}

#[test]
fn r2_param_domain() {
    assert_eq!(d_param("@tier == 'gone'"), Some(Decided::Bool(false)));
    assert_eq!(d_param("@tier == 'warm'"), None);
}

/// R2's `unset` side (`0.1 §11.2`): the CEL `null` literal decides via
/// `dom.maybe_unset`, exactly like any other domain-membership comparison
/// — never a runtime read. A defaulted path or a bound param is never
/// unset (`maybe_unset` false); an un-defaulted `run.*` path might be.
#[test]
fn r2_unset_domain_membership() {
    assert_eq!(d("run.flag == null"), Some(Decided::Bool(false)));
    assert_eq!(d_param("@tier == null"), Some(Decided::Bool(false)));
    assert_eq!(d("run.rank == null"), None);
}

#[test]
fn r5_undecided() {
    for e in [
        "run.n > 1",
        "isSet(run.flag)",
        "has(run.rank)",
        "holds(inParty(x))",
        "count(r(_)) > 0",
        "now() < run.t",
    ] {
        assert_eq!(d(e), None, "{e}");
    }
}

#[test]
fn def_expansion_precedes() {
    assert_eq!(d_defs("@never"), Some(Decided::Bool(false)));
}

/// R5's `@ref` failure path (D3): an unresolved def name (no known body,
/// `expand_ref`'s "names no known def body" error) leaves the ORIGINAL raw
/// text intact and re-parses as a marked identifier — declines, same as
/// any other unreadable reference, rather than treating the failed macro as
/// a decided value.
#[test]
fn def_unknown_ref_is_undecided() {
    assert_eq!(d("@ghost"), None);
}

/// The soundness pin (dsl 0.4.0 §5.1): an in-domain comparison NEVER
/// decides — the actual runtime value is unknown, so R2 must return
/// undecided rather than guess a boolean. A §5 error built on a false
/// "decision" here would be a conformance bug per the spec's own boundary.
#[test]
fn soundness_no_guessing() {
    assert_eq!(d("run.flag == true"), None);
}

/// The cycle guard (dsl 0.4.0 §5.1 R5 / D2): a cyclic `@def` chain must
/// decline promptly — never hang, never panic — exactly like any other
/// expansion failure (D3 fallback to the original raw text).
#[test]
fn def_cycle_guard_returns_none_promptly() {
    assert_eq!(d_cycle("@a"), None);
}

// --- dsl 0.23.0 §9: per-path reasoning across `&&` / `||` ----------------

fn f() -> Option<Decided> {
    Some(Decided::Bool(false))
}

fn t() -> Option<Decided> {
    Some(Decided::Bool(true))
}

/// Every contradiction the seven-days dogfood (T1-17) found passing clean
/// now decides false: two equalities on one path, an empty interval, and a
/// flag with its own negation.
#[test]
fn contradictory_conjunction_decides_false() {
    for e in [
        "run.rank == 'gold' && run.rank == 'fail'",
        "run.n == 3 && run.n == 4",
        "run.n > 5 && run.n < 3",
        "run.flag && !run.flag",
        "3 == run.n && run.n != 3",
        "run.n >= 3 && run.n <= 3 && run.n != 3",
        "!(run.n > 5) && run.n > 7",
        // De Morgan: `!(a || b)` contributes `!a` and `!b`.
        "!(run.n <= 5 || run.flag) && run.n < 3",
        // The contradiction may sit among unrelated operands.
        "run.flag && run.rank == 'gold' && visited('x') && run.rank == 'silver'",
        // A nested disjunction over one finite path is a set too.
        "(run.rank == 'gold' || run.rank == 'silver') && run.rank == 'fail'",
        "run.rank in ['gold', 'silver'] && run.rank == 'fail'",
    ] {
        assert_eq!(d(e), f(), "{e}");
    }
}

/// Satisfiable conjunctions stay undecided: the numbers are the REALS, and
/// operands on different paths never combine.
#[test]
fn satisfiable_conjunction_is_undecided() {
    for e in [
        "run.n > 1 && run.n < 2",
        "run.n >= 3 && run.n <= 3",
        "run.n != 3 && run.n != 4",
        "run.rank != 'gold' && run.rank != 'fail'",
        "run.flag && run.n > 5",
        "(run.rank == 'gold' || run.rank == 'silver') && run.rank != 'gold'",
    ] {
        assert_eq!(d(e), None, "{e}");
    }
}

/// `unset` is a value too. On `unset` an ordering or a bare boolean read
/// errs — neither true nor false — so a conjunction of them over a
/// maybe-unset path is not provably FALSE (its value there is an error);
/// an equality is false on `unset`, and `isSet` rules `unset` out.
#[test]
fn maybe_unset_paths_count_unset_as_a_value() {
    assert_eq!(d("run.m > 5 && run.m < 3"), None);
    assert_eq!(d("isSet(run.m) && run.m > 5 && run.m < 3"), f());
    assert_eq!(d("run.m == 1 && run.m == 2"), f());
    // `run.rank` has no default: every `!=` holds on `unset`.
    assert_eq!(
        d("run.rank != 'fail' && run.rank != 'bronze' && run.rank != 'silver' && run.rank != 'gold'"),
        None
    );
    assert_eq!(d("!isSet(run.rank) && run.rank == 'gold'"), f());
}

/// An undeclared path may hold a value of any kind: equalities still
/// exclude each other, but an ordering errs on a non-number.
#[test]
fn undeclared_path_keeps_only_equality_reasoning() {
    assert_eq!(d("run.ghost == 'a' && run.ghost == 'b'"), f());
    assert_eq!(d("run.ghost > 5 && run.ghost < 3"), None);
}

/// A disjunction decides true when its cases cover the path's domain —
/// `unset` included for a maybe-unset path.
#[test]
fn covering_disjunction_decides_true() {
    for e in [
        "run.flag || !run.flag",
        "run.n < 5 || run.n >= 5",
        "run.n <= 5 || run.n > 5",
        "run.n < 5 || run.n > 5 || run.n == 5",
        "run.n != 5 || run.n == 5",
        "run.rank != 'gold' || run.rank == 'gold'",
        "run.rank == 'fail' || run.rank == 'bronze' || run.rank == 'silver' || run.rank == 'gold' \
         || run.rank == null",
        "!(run.n < 5 && run.flag) || run.n < 5",
        "run.ghost != 'a' || run.ghost != 'b'",
    ] {
        assert_eq!(d(e), t(), "{e}");
    }
    for e in [
        "run.n < 5 || run.n > 5",
        // `run.rank` may be unset, which no `==` member covers.
        "run.rank == 'fail' || run.rank == 'bronze' || run.rank == 'silver' || run.rank == 'gold'",
        "run.m < 5 || run.m >= 5",
        // An undeclared path may hold a non-number.
        "run.ghost < 5 || run.ghost >= 5",
    ] {
        assert_eq!(d(e), None, "{e}");
    }
}

/// A nested disjunction of comparisons on ONE number path is a union of
/// intervals: a conjunction whose other operand falls in its gap decides
/// false (seven-days N5), in either operand order. Only a union covering the
/// whole real line on a never-unset path decides true; a gap, a two-path
/// disjunction, or a maybe-unset path stays undecided.
#[test]
fn numeric_disjunction_is_a_union_of_intervals() {
    for e in [
        "run.n == 3 && (run.n > 3 || run.n < 3)",
        "(run.n > 3 || run.n < 3) && run.n == 3",
        "run.n > 5 && (run.n < 3 || run.n == 4)",
        "run.n == 3 && !(run.n >= 3 && run.n <= 3)",
        "run.n == 3 && (run.n > 3 || (run.n < 3 && run.n > 1) || run.n < 1)",
        // `==` is false on `unset`, so a maybe-unset path decides too.
        "run.m == 3 && (run.m > 3 || run.m < 3)",
    ] {
        assert_eq!(d(e), f(), "{e}");
    }
    for e in [
        "run.n >= 3 || run.n < 3",
        "run.n > 3 || run.n < 5",
        "(run.n > 1 && run.n < 5) || run.n <= 1 || run.n >= 5",
    ] {
        assert_eq!(d(e), t(), "{e}");
    }
    for e in [
        "run.n > 3 || run.n < 3",
        "run.n == 3 && (run.n >= 3 || run.n < 1)",
        "run.n == 3 && (run.n > 3 || run.m < 3)",
        "run.n == 3 && (run.n > 3 || run.flag)",
        "(run.m > 1 && run.m < 5) || run.m <= 1 || run.m >= 5",
    ] {
        assert_eq!(d(e), None, "{e}");
    }
}

/// Relational calls are never-unset paths keyed by their text: a query and
/// its negation exclude each other with no fact envelope in scope.
#[test]
fn relational_queries_are_paths() {
    assert_eq!(d("holds(inParty(x)) && !holds(inParty(x))"), f());
    assert_eq!(d("visited('a') || !visited('a')"), t());
    assert_eq!(d("count(r(_)) > 2 && count(r(_)) < 1"), f());
    assert_eq!(d("holds(inParty(x)) && !holds(inParty(y))"), None);
}

/// The `$` subject and component params are paths too.
#[test]
fn dollar_and_params_are_paths() {
    assert_eq!(d_dollar("$ == 'gold' && $ == 'fail'"), f());
    assert_eq!(d_dollar("$ == 'gold' || $ != 'gold'"), t());
    assert_eq!(d_param("@tier == 'cold' && @tier == 'warm'"), f());
}

/// `@def`s expand before deciding, so a schedule def combines with a raw
/// comparison (seven-days: `@festivalNight && run.day == 2`).
#[test]
fn expanded_defs_combine() {
    let schema = schema();
    let params = BTreeMap::new();
    let ctx = DecideCtx {
        schema: &schema,
        dollar: None,
        params: &params,
        facts: None,
    };
    let mut bodies = BTreeMap::new();
    bodies.insert(
        "festival".to_string(),
        "run.n == 5 && run.rank == 'gold'".to_string(),
    );
    let def_params = BTreeMap::new();
    let defs = DefTable {
        bodies: &bodies,
        params: &def_params,
    };
    assert_eq!(decide_slot("@festival && run.n == 2", &defs, &ctx), f());
    assert_eq!(decide_slot("@festival && run.n != 2", &defs, &ctx), None);
}
