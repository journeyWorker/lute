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
/// `false`), `run.n: number` (default `0`) — mirrors the plan's Step 1 schema.
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
        },
    );
    decls.insert(
        "run.flag".to_string(),
        StateDecl {
            ty: Type::Bool,
            default: Some(Literal::Bool(false)),
            namespace: Namespace::Run,
        },
    );
    decls.insert(
        "run.n".to_string(),
        StateDecl {
            ty: Type::Number,
            default: Some(Literal::Num(0.0)),
            namespace: Namespace::Run,
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

#[test]
fn r2_dollar_domain() {
    assert_eq!(d_dollar("$ == 'gone'"), Some(Decided::Bool(false)));
    assert_eq!(d_dollar("$ == 'gold'"), None);
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

/// The soundness pin (dsl 0.4.0 §5.1): an in-domain comparison NEVER
/// decides — the actual runtime value is unknown, so R2 must return
/// undecided rather than guess a boolean. A §5 error built on a false
/// "decision" here would be a conformance bug per the spec's own boundary.
#[test]
fn soundness_no_guessing() {
    assert_eq!(d("run.flag == true"), None);
}
