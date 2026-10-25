//! 0.49.1 (rules note §4): the typed errors of `inline_interp_ref` and
//! `expand_rule_guard` render the 0.49.0 reason text and keep their cause
//! as `source()`.

use std::collections::BTreeMap;
use std::error::Error;

use lute_check::cel_resolve::{expand_rule_guard, RuleGuardError};
use lute_check::{inline_interp_ref, DefTable, ExpandError, InterpRefError};
use lute_manifest::types::Type;

type Tables = (
    BTreeMap<String, String>,
    BTreeMap<String, Vec<(String, Type)>>,
);

fn tables(bodies: &[(&str, &str)], params: &[(&str, &[&str])]) -> Tables {
    let b = bodies
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect();
    let p = params
        .iter()
        .map(|(k, ps)| {
            (
                (*k).to_string(),
                ps.iter().map(|n| ((*n).to_string(), Type::Int)).collect(),
            )
        })
        .collect();
    (b, p)
}

#[test]
fn interp_ref_error_text_and_source() {
    let t = tables(&[("broken", "1 +"), ("loop", "@loop")], &[]);
    let defs = DefTable {
        bodies: &t.0,
        params: &t.1,
    };
    let parse = inline_interp_ref("@broken", &defs).unwrap_err();
    let InterpRefError::Parse { body, .. } = &parse else {
        panic!("expected a parse refusal, got {parse:?}");
    };
    assert_eq!(
        parse.to_string(),
        format!("its expansion `{body}` is not a well-formed CEL expression")
    );
    assert!(parse.source().is_some(), "the CEL parse error is the cause");

    let cycle = inline_interp_ref("@loop", &defs).unwrap_err();
    let InterpRefError::Expand(cause @ ExpandError::Cycle { .. }) = &cycle else {
        panic!("expected an expansion cycle, got {cycle:?}");
    };
    assert_eq!(cycle.to_string(), cause.to_string());
    assert_eq!(
        cycle.source().map(ToString::to_string),
        Some(cause.to_string())
    );
}

#[test]
fn rule_guard_error_text() {
    let t = tables(&[("open", "L > 0"), ("loop", "@loop")], &[("open", &["L"])]);
    let defs = DefTable {
        bodies: &t.0,
        params: &t.1,
    };
    let text = |cel: &str| expand_rule_guard(cel, &defs).unwrap_err().to_string();
    assert_eq!(
        text("$ > 1"),
        "`$` (a match subject) has no meaning in a rule guard"
    );
    assert_eq!(text("@nope"), "`@nope` names no def");
    assert_eq!(text("@open"), "`@open` takes 1 arg(s), got 0");
    let cycle = expand_rule_guard("@loop", &defs).unwrap_err();
    assert!(matches!(cycle, RuleGuardError::Expand(_)), "{cycle:?}");
    assert!(cycle.to_string().starts_with("def expansion cycle: "));
    assert!(cycle.source().is_some());
}
