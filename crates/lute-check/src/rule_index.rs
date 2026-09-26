//! dsl 0.24.0 §3: a rule `cel()` guard reads entity-indexed state by a rule
//! variable — `loyal(P) :- inParty(P), cel("run.approval[P] >= 3")`, where
//! `run.approval` is declared `{ …, per: companion }`. `[P]` reads the member
//! the join bound to `P` through a positive atom; anywhere else a member is
//! addressed by name (`run.approval.isolde`).
//!
//! The IR keeps its contract that a rule guard is CEL over ground terms: a
//! rule reading `F[V]` is GROUNDED, one instance per member of `V`'s index
//! kind, with `V` replaced by the member in every atom and `F[V]` by
//! `F.<member>` in every guard ([`evaluable_rules`]). The compiler emits the
//! grounded rules and `lute trace` / `lute play` evaluate the same set, so no
//! evaluator ever sees an indexed read.

use std::borrow::Cow;
use std::collections::BTreeMap;

use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_manifest::relations::{kind_within, KindShape};
use lute_syntax::datalog::{BodyLiteral, Rule, RuleAtom, RuleTerm};

use crate::meta::RuleDecl;
use crate::rel_schema::RelVocab;

/// One `<family>[<Var>]` read inside a guard's CEL text: `range` spans
/// `[<Var>]` (brackets included).
#[derive(Clone, Debug, PartialEq)]
struct IndexUse {
    range: (usize, usize),
    family: String,
    var: String,
}

/// Every `<path>[<Ident>]` in `cel` outside string literals, where `<path>`
/// is a dotted identifier path ending right at `[` and `<Ident>` begins with
/// an uppercase letter (a Datalog rule variable). Anything else inside the
/// brackets (a literal, an expression) is not an indexed read and is left to
/// the ordinary CEL checks.
fn index_uses(cel: &str) -> Vec<IndexUse> {
    let mask = lute_cel::cel_string_mask(cel);
    let b = cel.as_bytes();
    let in_string = |i: usize| mask.get(i).copied().unwrap_or(false);
    let ident = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
    let mut out = Vec::new();
    for (open, _) in cel.match_indices('[') {
        if in_string(open) {
            continue;
        }
        let mut start = open;
        while start > 0 && (ident(b[start - 1]) || b[start - 1] == b'.') {
            start -= 1;
        }
        let family = &cel[start..open];
        if family.is_empty()
            || !family.as_bytes()[0].is_ascii_alphabetic()
            || family.ends_with('.')
            || !family.contains('.')
        {
            continue;
        }
        let mut i = open + 1;
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        let var_start = i;
        while i < b.len() && ident(b[i]) {
            i += 1;
        }
        let var = &cel[var_start..i];
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if var.is_empty() || !var.as_bytes()[0].is_ascii_uppercase() || b.get(i) != Some(&b']') {
            continue;
        }
        out.push(IndexUse {
            range: (open, i + 1),
            family: family.to_string(),
            var: var.to_string(),
        });
    }
    out
}

/// `cel` with each use's `[<Var>]` replaced by `.<member>` (`member_of`
/// names the member for a variable).
fn rewrite(cel: &str, uses: &[IndexUse], member_of: &dyn Fn(&str) -> String) -> String {
    let mut out = String::with_capacity(cel.len());
    let mut at = 0;
    for u in uses {
        out.push_str(&cel[at..u.range.0]);
        out.push('.');
        out.push_str(&member_of(&u.var));
        at = u.range.1;
    }
    out.push_str(&cel[at..]);
    out
}

/// dsl 0.27.0 §3 (T2-2): one bare rule variable in a guard's CEL text —
/// `run.stalker == S` — with the state path it is compared with (`==` /
/// `!=`, either side), `None` when it stands anywhere else. `range` spans
/// the variable.
#[derive(Clone, Debug, PartialEq)]
struct VarUse {
    range: (usize, usize),
    var: String,
    path: Option<String>,
}

/// Every bare `<Var>` (an identifier starting with an uppercase letter,
/// outside string literals) in `cel` that is not a `F[<Var>]` index, a call,
/// a field (`x.Y`) or a `@ref`.
fn var_uses(cel: &str) -> Vec<VarUse> {
    let mask = lute_cel::cel_string_mask(cel);
    let b = cel.as_bytes();
    let in_string = |i: usize| mask.get(i).copied().unwrap_or(false);
    let ident = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
    let path_byte = |c: u8| ident(c) || c == b'.';
    let skip_ws_back = |mut i: usize| {
        while i > 0 && b[i - 1].is_ascii_whitespace() {
            i -= 1;
        }
        i
    };
    let skip_ws = |mut i: usize| {
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        i
    };
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if in_string(i)
            || !b[i].is_ascii_uppercase()
            || (i > 0 && (path_byte(b[i - 1]) || matches!(b[i - 1], b'@' | b'$')))
        {
            i += 1;
            continue;
        }
        let start = i;
        while i < b.len() && ident(b[i]) {
            i += 1;
        }
        let end = i;
        let before = skip_ws_back(start);
        let after = skip_ws(end);
        let next = b.get(after).copied();
        let prev = before.checked_sub(1).map(|k| b[k]);
        if next == Some(b'(') || next == Some(b'.') || (prev == Some(b'[') && next == Some(b']')) {
            continue;
        }
        // `path == V` / `path != V`, or `V == path` / `V != path`. Bytes,
        // not `str` slices: a neighbouring multi-byte char (`≥`) would put
        // a slice bound inside it (FS-F1).
        let op_before = before >= 2
            && matches!(&b[before - 2..before], b"==" | b"!=")
            && before
                .checked_sub(3)
                .is_none_or(|k| !matches!(b[k], b'=' | b'!' | b'<' | b'>'));
        let op_after = after + 2 <= b.len() && matches!(&b[after..after + 2], b"==" | b"!=");
        let path = if op_before {
            let e = skip_ws_back(before - 2);
            let mut s = e;
            while s > 0 && path_byte(b[s - 1]) {
                s -= 1;
            }
            Some(&cel[s..e])
        } else if op_after {
            let s = skip_ws(after + 2);
            let mut e = s;
            while e < b.len() && path_byte(b[e]) {
                e += 1;
            }
            Some(&cel[s..e])
        } else {
            None
        };
        let path = path
            .filter(|p| p.contains('.') && p.as_bytes()[0].is_ascii_lowercase())
            .map(str::to_string);
        out.push(VarUse {
            range: (start, end),
            var: cel[start..end].to_string(),
            path,
        });
    }
    out
}

/// `cel` with each bare variable use replaced by the string literal of its
/// member (`run.stalker == S` under `S = morgue` reads
/// `run.stalker == 'morgue'`); variables `member_of` leaves `None` stay.
fn rewrite_vars(cel: &str, uses: &[VarUse], member_of: &dyn Fn(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(cel.len());
    let mut at = 0;
    for u in uses {
        let Some(m) = member_of(&u.var) else { continue };
        out.push_str(&cel[at..u.range.0]);
        out.push('\'');
        out.push_str(&m);
        out.push('\'');
        at = u.range.1;
    }
    out.push_str(&cel[at..]);
    out
}

/// The members a variable bound by several positive atoms can take: the
/// members every closed binding kind shares (`None` when no binding kind is
/// closed).
fn bound_members(rule: &Rule, var: &str, vocab: &RelVocab) -> Option<Vec<String>> {
    let mut out: Option<Vec<String>> = None;
    for (_, kind) in binding_kinds(rule, var, vocab) {
        let Some(ms) = closed_members(vocab, &kind) else {
            continue;
        };
        match &mut out {
            Some(have) => have.retain(|m| ms.contains(m)),
            None => out = Some(ms.to_vec()),
        }
    }
    out
}

/// Validate every bare rule variable in one guard of `rule` (dsl 0.27.0 §3,
/// T2-2) and return the guard text the ordinary guard checks should see —
/// each use replaced by `'<first member>'` of the variable's kind. `Err`
/// carries the diagnostics when a use is illegal:
///
/// * the variable must be compared (`==` / `!=`) with a state path
///   (`E-CEL-PROFILE`) typed `{ domain: K }` / `{ entity: K }`;
/// * it must be bound by a positive body atom (`E-DATALOG-UNSAFE`) over a
///   closed kind that shares members with `K` (`E-FACT-DOMAIN`).
pub fn check_var_guard(
    rule: &Rule,
    cel: &str,
    vocab: &RelVocab,
    state: &crate::meta::StateSchema,
    span: Span,
) -> Result<String, Vec<Diagnostic>> {
    let uses = var_uses(cel);
    if uses.is_empty() {
        return Ok(cel.to_string());
    }
    let mut diags = Vec::new();
    let mut first_member: BTreeMap<&str, String> = BTreeMap::new();
    for u in &uses {
        let v = &u.var;
        let Some(path) = &u.path else {
            diags.push(diag(
                "E-CEL-PROFILE",
                format!(
                    "rule guard reads rule variable `{v}` bare; a variable is read as `F[{v}]` \
                     (entity-indexed state) or compared with a domain-typed state path, \
                     `run.path == {v}` (dsl 0.24.0 §3, dsl 0.27.0 §3)"
                ),
                span,
            ));
            continue;
        };
        let kind = match state.decls.get(path).map(|d| &d.ty) {
            Some(lute_manifest::types::Type::Domain(k) | lute_manifest::types::Type::Entity(k)) => {
                k.clone()
            }
            Some(_) => {
                diags.push(diag(
                    "E-CEL-PROFILE",
                    format!(
                        "rule guard compares rule variable `{v}` with `{path}`, which is not typed \
                         by a kind; declare it `{{ type: {{ domain: <kind> }} }}` so `{v}` is \
                         instantiated per member (dsl 0.27.0 §3)"
                    ),
                    span,
                ));
                continue;
            }
            // An undeclared path is the ordinary guard check's `E-UNDECLARED`.
            None => String::new(),
        };
        let bound = binding_kinds(rule, v, vocab);
        if bound.is_empty() {
            diags.push(diag(
                "E-DATALOG-UNSAFE",
                format!(
                    "rule guard compares `{path}` with `{v}`, but `{v}` is not bound by a positive \
                     body atom; bind it first (dsl 0.27.0 §3)"
                ),
                span,
            ));
            continue;
        }
        let Some(members) = bound_members(rule, v, vocab) else {
            diags.push(diag(
                "E-FACT-DOMAIN",
                format!(
                    "rule guard compares `{path}` with `{v}`, but `{v}` ranges over no closed \
                     kind, so it cannot be instantiated per member; bind it with a kind that \
                     lists its `members:` (dsl 0.27.0 §3)"
                ),
                span,
            ));
            continue;
        };
        if !kind.is_empty()
            && !bound
                .iter()
                .any(|(_, k)| within(vocab, k, &kind) || within(vocab, &kind, k))
        {
            let by: Vec<String> = bound
                .iter()
                .map(|(a, k)| format!("`{a}` over `{k}`"))
                .collect();
            diags.push(diag(
                "E-FACT-DOMAIN",
                format!(
                    "rule guard compares `{path}` (a `{kind}`) with `{v}`, but `{v}` ranges over {} \
                     — the comparison can never hold (dsl 0.27.0 §3)",
                    by.join(", ")
                ),
                span,
            ));
            continue;
        }
        if let Some(m) = members.first() {
            first_member.insert(v, m.clone());
        }
    }
    if !diags.is_empty() {
        return Err(diags);
    }
    Ok(rewrite_vars(cel, &uses, &|v| first_member.get(v).cloned()))
}

/// The kind each positive body atom gives `var`: the atom's own name for an
/// entity-kind predicate `K(var)`, else the relation's declared argument
/// domain at `var`'s position. With the atom it came from, for messages.
fn binding_kinds(rule: &Rule, var: &str, vocab: &RelVocab) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for lit in &rule.body {
        let BodyLiteral::Pos(atom) = lit else {
            continue;
        };
        for (i, t) in atom.terms.iter().enumerate() {
            if !matches!(t, RuleTerm::Var(v) if v == var) {
                continue;
            }
            let kind = if vocab.kinds.contains_key(&atom.relation) {
                Some(atom.relation.clone())
            } else {
                vocab
                    .relations
                    .get(&atom.relation)
                    .and_then(|r| r.args.get(i).cloned())
            };
            if let Some(kind) = kind {
                out.push((render_atom(atom), kind));
            }
        }
    }
    out
}

fn render_atom(atom: &RuleAtom) -> String {
    let terms: Vec<String> = atom
        .terms
        .iter()
        .map(|t| match t {
            RuleTerm::Var(v) | RuleTerm::Const(v) => v.clone(),
            RuleTerm::Bool(b) => b.to_string(),
        })
        .collect();
    format!("{}({})", atom.relation, terms.join(", "))
}

fn closed_members<'a>(vocab: &'a RelVocab, kind: &str) -> Option<&'a [String]> {
    match &vocab.kinds.get(kind)?.shape {
        KindShape::Members(ms) => Some(ms),
        _ => None,
    }
}

/// `kind` ranges inside `index_kind`: it is `index_kind` or a `subsetOf:`
/// descendant, or a closed kind whose every member is one of `index_kind`'s.
fn within(vocab: &RelVocab, kind: &str, index_kind: &str) -> bool {
    kind_within(&vocab.kinds, kind, index_kind)
        || match (
            closed_members(vocab, kind),
            closed_members(vocab, index_kind),
        ) {
            (Some(ms), Some(outer)) => ms.iter().all(|m| outer.contains(m)),
            _ => false,
        }
}

fn diag(code: &str, message: String, span: Span) -> Diagnostic {
    Diagnostic {
        code: code.to_string(),
        severity: Severity::Error,
        message,
        span,
        layer: Layer::Logic,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    }
}

/// Validate every `<family>[<Var>]` read in one guard of `rule` (dsl 0.24.0
/// §3) and return the guard text the ordinary guard checks should see —
/// each `[<Var>]` replaced by `.<first member>` of the index kind, so path
/// declaredness and the CEL profile judge a real member path. `Err` carries
/// the diagnostics when a read is illegal; the caller then skips the
/// remaining checks for this guard (no cascade onto `P` or the family path).
///
/// * `<family>` must be declared with `per:` (`E-UNDECLARED`);
/// * `<Var>` must be bound by a positive body atom (`E-DATALOG-UNSAFE`);
/// * one of those atoms must range `<Var>` over the index kind or a sub-kind
///   of it (`E-FACT-DOMAIN`) — otherwise some binding would read a path that
///   is not declared.
pub fn check_indexed_guard(
    rule: &Rule,
    cel: &str,
    vocab: &RelVocab,
    span: Span,
) -> Result<String, Vec<Diagnostic>> {
    let uses = index_uses(cel);
    if uses.is_empty() {
        return Ok(cel.to_string());
    }
    let mut diags = Vec::new();
    let mut first_member: BTreeMap<&str, String> = BTreeMap::new();
    for u in &uses {
        let Some(kind) = vocab.indexed_state.get(&u.family) else {
            diags.push(diag(
                "E-UNDECLARED",
                format!(
                    "rule guard reads `{}[{}]`, but `{}` is not entity-indexed state; declare it \
                     `{{ type: …, per: <kind> }}` to index it by a rule variable (dsl 0.24.0 §3)",
                    u.family, u.var, u.family
                ),
                span,
            ));
            continue;
        };
        let bound = binding_kinds(rule, &u.var, vocab);
        if bound.is_empty() {
            diags.push(diag(
                "E-DATALOG-UNSAFE",
                format!(
                    "rule guard reads `{}[{}]`, but `{}` is not bound by a positive body atom; \
                     bind it first, e.g. `{kind}({})` (dsl 0.24.0 §3)",
                    u.family, u.var, u.var, u.var
                ),
                span,
            ));
            continue;
        }
        if !bound.iter().any(|(_, k)| within(vocab, k, kind)) {
            let by: Vec<String> = bound
                .iter()
                .map(|(a, k)| format!("`{a}` over `{k}`"))
                .collect();
            diags.push(diag(
                "E-FACT-DOMAIN",
                format!(
                    "rule guard reads `{}[{}]`, declared `per: {kind}`, but `{}` ranges over {} — \
                     not `{kind}` or a sub-kind of it, so some binding reads an undeclared path; \
                     bind `{}` with `{kind}({})` or a relation over `{kind}` (dsl 0.24.0 §3)",
                    u.family,
                    u.var,
                    u.var,
                    by.join(", "),
                    u.var,
                    u.var
                ),
                span,
            ));
            continue;
        }
        if let Some(m) = closed_members(vocab, kind).and_then(|ms| ms.first()) {
            first_member.insert(&u.var, m.clone());
        }
    }
    if !diags.is_empty() {
        return Err(diags);
    }
    Ok(rewrite(cel, &uses, &|v| {
        first_member.get(v).cloned().unwrap_or_default()
    }))
}

/// The members each indexed variable of `rule` is grounded over: for a
/// variable reading several families, the members every index kind shares.
/// dsl 0.27.0 §3 (T2-2): a variable compared bare with a domain-typed path
/// is grounded over the members its closed binding kinds share. Empty when
/// the rule reads no indexed state and compares no variable. A family that
/// is not entity-indexed (already an error at check) grounds nothing.
fn grounding(rule: &Rule, vocab: &RelVocab) -> BTreeMap<String, Vec<String>> {
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut constrain = |var: &str, members: &[String]| match out.get_mut(var) {
        Some(have) => have.retain(|m| members.contains(m)),
        None => {
            out.insert(var.to_string(), members.to_vec());
        }
    };
    for lit in &rule.body {
        let BodyLiteral::Guard { cel, .. } = lit else {
            continue;
        };
        for u in index_uses(cel) {
            let members = vocab
                .indexed_state
                .get(&u.family)
                .and_then(|k| closed_members(vocab, k))
                .unwrap_or_default();
            constrain(&u.var, members);
        }
        for u in var_uses(cel) {
            let members = bound_members(rule, &u.var, vocab).unwrap_or_default();
            constrain(&u.var, &members);
        }
    }
    out
}

fn ground_term(t: &RuleTerm, at: &BTreeMap<&str, &str>) -> RuleTerm {
    match t {
        RuleTerm::Var(v) => match at.get(v.as_str()) {
            Some(m) => RuleTerm::Const((*m).to_string()),
            None => t.clone(),
        },
        other => other.clone(),
    }
}

fn ground_atom(a: &RuleAtom, at: &BTreeMap<&str, &str>) -> RuleAtom {
    RuleAtom {
        terms: a.terms.iter().map(|t| ground_term(t, at)).collect(),
        ..a.clone()
    }
}

/// A rule guard's CEL with each indexed read `F[V]` whose variable `at`
/// binds replaced by `F.<member>` (`run.aff[P]` under `P = sol` reads
/// `run.aff.sol`) and each bare use by the member's string literal
/// (`run.stalker == S` under `S = morgue` reads `run.stalker ==
/// 'morgue'`, dsl 0.27.0 §3); unbound variables are left as written.
pub fn ground_guard(cel: &str, at: &BTreeMap<&str, &str>) -> String {
    let uses: Vec<IndexUse> = index_uses(cel)
        .into_iter()
        .filter(|u| at.contains_key(u.var.as_str()))
        .collect();
    let indexed = rewrite(cel, &uses, &|v| {
        at.get(v).map(|m| (*m).to_string()).unwrap_or_default()
    });
    rewrite_vars(&indexed, &var_uses(&indexed), &|v| {
        at.get(v).map(|m| (*m).to_string())
    })
}

fn ground_rule(rule: &Rule, at: &BTreeMap<&str, &str>) -> Rule {
    let body = rule
        .body
        .iter()
        .map(|lit| match lit {
            BodyLiteral::Pos(a) => BodyLiteral::Pos(ground_atom(a, at)),
            BodyLiteral::Neg(a) => BodyLiteral::Neg(ground_atom(a, at)),
            BodyLiteral::Guard { cel, span } => BodyLiteral::Guard {
                cel: ground_guard(cel, at),
                span: *span,
            },
            BodyLiteral::Cmp {
                lhs,
                rhs,
                negated,
                span,
            } => BodyLiteral::Cmp {
                lhs: ground_term(lhs, at),
                rhs: ground_term(rhs, at),
                negated: *negated,
                span: *span,
            },
            BodyLiteral::Count {
                atom,
                distinct,
                op,
                n,
                span,
            } => BodyLiteral::Count {
                atom: ground_atom(atom, at),
                distinct: distinct.clone(),
                op: *op,
                n: *n,
                span: *span,
            },
        })
        .collect();
    Rule {
        head: ground_atom(&rule.head, at),
        body,
    }
}

/// The rules an evaluator runs (dsl 0.24.0 §3): `vocab.rules` with every
/// rule that reads entity-indexed state by a variable replaced by one
/// grounded instance per member of the variable's index kind (the cartesian
/// product when a rule indexes by several variables), each keeping the
/// authored span, its `raw` text suffixed with the instance
/// (`… [P = isolde]`). Borrowed unchanged when no rule reads indexed state —
/// the common case costs nothing.
pub fn evaluable_rules(vocab: &RelVocab) -> Cow<'_, [RuleDecl]> {
    if !vocab.rules.iter().any(|r| {
        r.rule
            .body
            .iter()
            .any(|l| matches!(l, BodyLiteral::Guard { cel, .. } if !index_uses(cel).is_empty() || !var_uses(cel).is_empty()))
    }) {
        return Cow::Borrowed(&vocab.rules);
    }
    let mut out = Vec::with_capacity(vocab.rules.len());
    for r in &vocab.rules {
        let vars = grounding(&r.rule, vocab);
        if vars.is_empty() {
            out.push(r.clone());
            continue;
        }
        // Cartesian product over the indexed variables, in name order.
        let mut assignments: Vec<BTreeMap<&str, &str>> = vec![BTreeMap::new()];
        for (var, members) in &vars {
            assignments = assignments
                .into_iter()
                .flat_map(|a| {
                    members.iter().map(move |m| {
                        let mut next = a.clone();
                        next.insert(var.as_str(), m.as_str());
                        next
                    })
                })
                .collect();
        }
        for at in &assignments {
            let instance: Vec<String> = at.iter().map(|(v, m)| format!("{v} = {m}")).collect();
            out.push(RuleDecl {
                rule: ground_rule(&r.rule, at),
                // The authored rule plus its instance: distinct per member,
                // so a project-wide union (keyed by head and `raw`) keeps
                // every instance, and an explanation names the one it used.
                raw: format!("{} [{}]", r.raw, instance.join(", ")),
                span: r.span,
            });
        }
    }
    Cow::Owned(out)
}

/// The entity-indexed family a state path belongs to and its member —
/// `run.approval.isolde` → (`run.approval`, `isolde`) — when `path` is one.
pub fn indexed_family<'a>(vocab: &'a RelVocab, path: &str) -> Option<(&'a str, &'a str)> {
    let (family, _) = path.rsplit_once('.')?;
    vocab
        .indexed_state
        .get_key_value(family)
        .map(|(f, k)| (f.as_str(), k.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_indexed_reads_outside_strings() {
        let u = index_uses(
            "run.approval[P] >= 3 && run.rep[ F ] < 0 && 'x[Y]' == 'a' && run.xs[0] == 1",
        );
        let got: Vec<(&str, &str)> = u
            .iter()
            .map(|u| (u.family.as_str(), u.var.as_str()))
            .collect();
        assert_eq!(got, vec![("run.approval", "P"), ("run.rep", "F")]);
    }

    #[test]
    fn rewrites_each_read_to_a_member_path() {
        let cel = "run.approval[P] >= 3 && run.approval[P] < 9";
        let uses = index_uses(cel);
        assert_eq!(
            rewrite(cel, &uses, &|_| "isolde".to_string()),
            "run.approval.isolde >= 3 && run.approval.isolde < 9"
        );
    }

    /// FS-F1: a multi-byte char beside a variable (`≥`, a full-width `＝`)
    /// is not a comparison operator — and never a slice inside a char.
    #[test]
    fn a_multibyte_neighbour_is_no_operator() {
        for cel in [
            "run.x ≥ S",
            "S ≥ run.x",
            "run.x ＝ S",
            "S＝ run.x",
            "run.x == S",
        ] {
            let uses = var_uses(cel);
            assert_eq!(uses.len(), 1, "{cel}");
            let compared = cel.contains("==");
            assert_eq!(uses[0].path.is_some(), compared, "{cel}");
        }
    }
}
