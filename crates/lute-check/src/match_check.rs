//! `<match>` exhaustiveness + first-match-wins lint, and `<branch>` recording
//! (dsl §11.1, §11.2).
//!
//! ## `<match>` (§11.2)
//! Arms run **top-to-bottom, first match wins**. An `<otherwise>` is REQUIRED
//! unless the subject's domain is *finite* and *fully covered* by the `<when>`
//! arms. Domain finiteness is inferred from the subject's declared `state:` type
//! ([`StateSchema`]): a `bool` (domain `{true,false}`), an `enum` (domain = its
//! members), or a `scene.choices.<branchId>` path (domain = the branch's choice
//! ids ∪ `unset`) is FINITE; anything else (number/string/record/…) is INFINITE
//! and therefore requires `<otherwise>`. Diagnostics:
//!
//! - **`E-NONEXHAUSTIVE`** — no `<otherwise>` and the domain is either infinite,
//!   or finite but not every domain value is covered by a `<when>` arm.
//! - **`E-UNSET-UNCOVERED`** — the subject is *maybe-unset* (`scene.choices.*`, or
//!   a `run.*`/`user.*`/`app.*` decl with no schema `default`) and the `unset`
//!   case is not covered by an `unset`-matching arm nor an `<otherwise>`.
//! - **`E-AGE-GATE`** — an age-gated `<match on="app.rating">` that covers neither
//!   a `teen` arm nor an `<otherwise>` (a release-build hard gate, §11.2).
//! - **`W-OVERLAP-ARMS`** (Warning) — two `<when>` arms that *provably* match the
//!   same value (kept conservative: identical literal equality tests only, never
//!   general SAT). First-match-wins means the later arm is dead.
//!
//! ## `<branch>` (§11.1)
//! `<branch id>` MUST be unique within the episode (the `.lute` document);
//! selecting a choice records `scene.choices.<branchId> = <choiceId>`, an
//! implicitly-declared, episode-scoped path whose domain is the branch's choice
//! ids ∪ `unset`. [`check_branch`] emits **`E-DUP-BRANCH`** on a repeat id and
//! returns the implicit [`StateDecl`] to fold into the schema.
//!
//! ### Branch-dup threading (for T4.9 assembly)
//! Duplicate detection is *episode-wide*, but this module checks one branch at a
//! time. Rather than hide episode state in `Ctx` (which is per-check and cloned),
//! the caller threads a `&mut BTreeSet<String>` of seen branch ids in **document
//! order** and folds each returned [`BranchRecord::decl`] into the accumulating
//! `StateSchema`. This keeps `Ctx` immutable and the episode set explicit and
//! caller-owned — the T4.9 whole-document walk already iterates shots/nodes in
//! order, so it owns the set and the schema it grows.
//!
//! ## Spans / layer
//! All diagnostics are [`Layer::Logic`] (§9/§11 logic checks). Per the cel-parser
//! 0.10.1 carry-forward (T3.1/T4.3) arm-test byte offsets are unavailable, so
//! coverage is reconstructed from a throwaway re-parse of each slot's raw CEL and
//! diagnostics fall back to the enclosing match/arm/branch span.

use std::collections::BTreeSet;

use cel_parser::ast::Expr;
use cel_parser::reference::Val;
use lute_cel::CelArena;
use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_manifest::types::Type;
use lute_syntax::ast::{Arm, Branch, Match};

use crate::meta::{Namespace, StateDecl, StateSchema};
use crate::Ctx;

/// A concrete, statically-known value an arm can match against.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum DomainValue {
    Str(String),
    Bool(bool),
}

/// The inferred value domain of a `<match>` subject (dsl §11.2).
#[derive(Clone, Debug, PartialEq, Eq)]
enum Domain {
    /// Finite domain with a known, enumerable set of values.
    Finite(Vec<DomainValue>),
    /// Infinite / unknowable domain (number, string, unresolved subject): an
    /// `<otherwise>` is mandatory.
    Infinite,
}

/// One branch's recording result: the implicit `scene.choices.<id>` decl to fold
/// into the schema, plus any diagnostic (`E-DUP-BRANCH`).
#[derive(Clone, Debug)]
pub struct BranchRecord {
    /// The declared state path, e.g. `scene.choices.couch`.
    pub path: String,
    /// The implicit declaration (`enum` of choice ids, scene-scoped, no default).
    pub decl: StateDecl,
    /// Diagnostics for this branch (currently only `E-DUP-BRANCH`).
    pub diags: Vec<Diagnostic>,
}

/// Validate a `<match>` for exhaustiveness, unset coverage, the age-gate, and
/// provably-overlapping arms (dsl §11.2). All diagnostics are [`Layer::Logic`].
pub fn check_match(m: &Match, schema: &StateSchema, ctx: &Ctx) -> Vec<Diagnostic> {
    let _ = ctx; // reserved: subject typing is owned by T4.3; unused here.
    let mut diags = Vec::new();
    let subject = subject_path(m);
    let info = infer_domain(subject.as_deref(), schema);
    let has_otherwise = m.arms.iter().any(|a| matches!(a, Arm::Otherwise { .. }));

    // One ordered pass over the `<when>` arms: accumulate covered values (+ the
    // `unset` case) and flag a provably-dead overlap. First-match-wins means an
    // arm whose concrete value was already covered by an EARLIER arm is dead.
    let mut covered: BTreeSet<DomainValue> = BTreeSet::new();
    let mut covers_unset = false;
    for arm in &m.arms {
        if let Arm::When { test, span, .. } = arm {
            let cov = analyze_arm(&test.raw, subject.as_deref());
            if cov.values.iter().any(|v| covered.contains(v)) {
                diags.push(diag(
                    "W-OVERLAP-ARMS",
                    Severity::Warning,
                    "this `<when>` provably overlaps an earlier arm; first-match-wins makes it \
                     unreachable (dsl §11.2)"
                        .to_string(),
                    *span,
                ));
            }
            for v in cov.values {
                covered.insert(v);
            }
            covers_unset |= cov.covers_unset;
        }
    }

    // Age-gate special case (§11.2): an age-gated `<match on="app.rating">` MUST
    // carry a `teen` arm or an `<otherwise>` — a release-build hard gate.
    if subject.as_deref() == Some("app.rating")
        && !has_otherwise
        && !covered.contains(&DomainValue::Str("teen".to_string()))
    {
        diags.push(diag(
            "E-AGE-GATE",
            Severity::Error,
            "age-gated `<match on=\"app.rating\">` must cover a `teen` arm or carry an \
             `<otherwise>` (dsl §11.2)"
                .to_string(),
            m.span,
        ));
    }

    // An `<otherwise>` makes the match exhaustive and covers `unset` (§11.2).
    if has_otherwise {
        return diags;
    }

    let fully_covered = match &info.domain {
        Domain::Finite(vals) => vals.iter().all(|v| covered.contains(v)),
        Domain::Infinite => false,
    };
    if !fully_covered {
        diags.push(diag(
            "E-NONEXHAUSTIVE",
            Severity::Error,
            "non-exhaustive `<match>`: the subject's domain is not fully covered and there is no \
             `<otherwise>` (dsl §11.2)"
                .to_string(),
            m.span,
        ));
    }

    // A maybe-unset subject's `unset` case must be covered (§11.2/§9.4).
    if info.maybe_unset && !covers_unset {
        diags.push(diag(
            "E-UNSET-UNCOVERED",
            Severity::Error,
            "maybe-unset `<match>` subject: the `unset` case is not covered by an `unset` arm or \
             an `<otherwise>` (dsl §11.2)"
                .to_string(),
            m.span,
        ));
    }

    diags
}

/// Record a `<branch>` (dsl §11.1): flag a duplicate id within the episode
/// (`E-DUP-BRANCH`) and return the implicit `scene.choices.<id>` declaration.
/// `seen` is the caller-owned, document-order set of branch ids seen so far (see
/// the module docs for why dup-detection is threaded rather than held in `Ctx`).
pub fn check_branch(branch: &Branch, seen: &mut BTreeSet<String>) -> BranchRecord {
    let path = format!("scene.choices.{}", branch.id);
    let mut diags = Vec::new();
    // `insert` returns `false` when the id was already present => a duplicate.
    if !seen.insert(branch.id.clone()) {
        diags.push(diag(
            "E-DUP-BRANCH",
            Severity::Error,
            format!(
                "duplicate `<branch id=\"{}\">`; branch ids must be unique within the episode \
                 (dsl §11.1)",
                branch.id
            ),
            branch.span,
        ));
    }
    // Implicit decl: enum of the branch's choice ids, scene-scoped, no default
    // (so it is maybe-unset — the domain is choice ids ∪ `unset`, §11.1).
    let members = branch.choices.iter().map(|c| c.id.clone()).collect();
    let decl = StateDecl { ty: Type::Enum(members), default: None, namespace: Namespace::Scene };
    BranchRecord { path, decl, diags }
}

/// Whether a `<match>` is provably exhaustive (dsl §11.2): it has an
/// `<otherwise>`, or its finite domain — including the `unset` member when the
/// subject is maybe-unset — is fully covered by the `<when>` arms. Exposed for
/// T4.4 (definite-assignment) so a domain-exhaustive match without `<otherwise>`
/// is not treated as a possible fall-through (its arms' join is an intersection,
/// not the pre-block set). See the report's "exhaustiveness result shape".
pub fn is_exhaustive(m: &Match, schema: &StateSchema) -> bool {
    if m.arms.iter().any(|a| matches!(a, Arm::Otherwise { .. })) {
        return true;
    }
    let subject = subject_path(m);
    let info = infer_domain(subject.as_deref(), schema);
    let mut covered: BTreeSet<DomainValue> = BTreeSet::new();
    let mut covers_unset = false;
    for arm in &m.arms {
        if let Arm::When { test, .. } = arm {
            let cov = analyze_arm(&test.raw, subject.as_deref());
            for v in cov.values {
                covered.insert(v);
            }
            covers_unset |= cov.covers_unset;
        }
    }
    let domain_covered = match &info.domain {
        Domain::Finite(vals) => vals.iter().all(|v| covered.contains(v)),
        Domain::Infinite => false,
    };
    domain_covered && (!info.maybe_unset || covers_unset)
}

/// The inferred domain of a subject plus whether the subject is maybe-unset.
struct DomainInfo {
    domain: Domain,
    maybe_unset: bool,
}

/// Infer the subject's value domain (dsl §11.2). A `bool`/`enum` decl or a
/// `scene.choices.<id>` path is FINITE; anything else is INFINITE (requires
/// `<otherwise>`). Maybe-unset: a `scene.choices.*` subject always (a branch may
/// not have been reached), or a `run.*`/`user.*`/`app.*` decl with no `default`.
fn infer_domain(subject: Option<&str>, schema: &StateSchema) -> DomainInfo {
    let Some(path) = subject else {
        return DomainInfo { domain: Domain::Infinite, maybe_unset: false };
    };
    // `scene.choices.<branchId>`: domain = branch choice ids ∪ `unset` (§11.1).
    if path.strip_prefix("scene.choices.").is_some() {
        let domain = match enum_members(path, schema) {
            Some(vals) => Domain::Finite(vals),
            // Members unknown (branch decl not folded in yet) => can't prove
            // coverage; treat as infinite so `<otherwise>` is required.
            None => Domain::Infinite,
        };
        return DomainInfo { domain, maybe_unset: true };
    }
    match schema.decls.get(path) {
        Some(decl) => {
            let domain = match &decl.ty {
                Type::Bool => {
                    Domain::Finite(vec![DomainValue::Bool(true), DomainValue::Bool(false)])
                }
                Type::Enum(members) => {
                    Domain::Finite(members.iter().map(|m| DomainValue::Str(m.clone())).collect())
                }
                _ => Domain::Infinite,
            };
            let maybe_unset = decl.default.is_none()
                && matches!(decl.namespace, Namespace::Run | Namespace::User | Namespace::App);
            DomainInfo { domain, maybe_unset }
        }
        None => DomainInfo { domain: Domain::Infinite, maybe_unset: false },
    }
}

/// The enum members declared at `path`, if the decl is a `Type::Enum`.
fn enum_members(path: &str, schema: &StateSchema) -> Option<Vec<DomainValue>> {
    match &schema.decls.get(path)?.ty {
        Type::Enum(members) => Some(members.iter().map(|m| DomainValue::Str(m.clone())).collect()),
        _ => None,
    }
}

/// What a single `<when>` arm provably matches.
#[derive(Default)]
struct ArmCoverage {
    /// Concrete finite-domain values (`bool`/enum-string) the arm covers.
    values: Vec<DomainValue>,
    /// Whether the arm covers the `unset` case.
    covers_unset: bool,
}

/// Analyze a `<when test>` and extract the finite-domain values it provably
/// matches. Kept CONSERVATIVE (only forms we can prove): `$ == <lit>` /
/// `<lit> == $`, bare `$` (bool true) and `!$` (bool false), `$ in [<lit>,…]`,
/// `$ == null` and `!isSet($)`/`!has(p)` (`unset`). Anything else (a `@ref`
/// guard, a relational test) yields no coverage — soundly leaving the domain
/// under-covered rather than falsely claiming exhaustiveness.
fn analyze_arm(raw: &str, subject: Option<&str>) -> ArmCoverage {
    let mut cov = ArmCoverage::default();
    if let Some(expr) = parse_expr(raw) {
        analyze_expr(&expr, subject, &mut cov);
    }
    cov
}

fn analyze_expr(expr: &Expr, subject: Option<&str>, cov: &mut ArmCoverage) {
    match expr {
        // `$ == <lit>` / `<lit> == $`.
        Expr::Call(c) if c.target.is_none() && c.func_name == "_==_" && c.args.len() == 2 => {
            let (a, b) = (&c.args[0].expr, &c.args[1].expr);
            if is_subject(a, subject) {
                push_literal(b, cov);
            } else if is_subject(b, subject) {
                push_literal(a, cov);
            }
        }
        // `$ in [<lit>, <lit>, …]`.
        Expr::Call(c)
            if c.target.is_none()
                && c.func_name == "@in"
                && c.args.len() == 2
                && is_subject(&c.args[0].expr, subject) =>
        {
            if let Expr::List(list) = &c.args[1].expr {
                for el in &list.elements {
                    push_literal(&el.expr, cov);
                }
            }
        }
        // `!$` (bool false) or `!isSet($)` / `!has(p)` (unset).
        Expr::Call(c) if c.target.is_none() && c.func_name == "!_" && c.args.len() == 1 => {
            let inner = &c.args[0].expr;
            if is_subject(inner, subject) {
                cov.values.push(DomainValue::Bool(false));
            } else if is_unset_test(inner, subject) {
                cov.covers_unset = true;
            }
        }
        // Bare `$` used as a boolean condition (bool true).
        _ if is_subject(expr, subject) => cov.values.push(DomainValue::Bool(true)),
        _ => {}
    }
}

/// Push a scalar literal onto the coverage: a string / bool is a finite-domain
/// value; `null` covers `unset`; numbers/bytes match no `bool`/`enum` member.
fn push_literal(expr: &Expr, cov: &mut ArmCoverage) {
    if let Expr::Literal(v) = expr {
        match v {
            Val::String(s) => cov.values.push(DomainValue::Str(s.clone())),
            Val::Boolean(b) => cov.values.push(DomainValue::Bool(*b)),
            Val::Null => cov.covers_unset = true,
            _ => {}
        }
    }
}

/// True when `expr` is the match subject: the substituted `$` (`Ident("_")`) or a
/// dotted chain equal to the subject path.
fn is_subject(expr: &Expr, subject: Option<&str>) -> bool {
    if let Expr::Ident(name) = expr {
        if name == "_" {
            return true;
        }
    }
    match (crate::cel_paths::select_path(expr), subject) {
        (Some(p), Some(s)) => p == s,
        _ => false,
    }
}

/// True when `expr` is a presence test of the subject (`isSet($)` or `has(p)`) —
/// negating it (in `analyze_expr`) is what covers the `unset` case.
fn is_unset_test(expr: &Expr, subject: Option<&str>) -> bool {
    match expr {
        // `has(p)` expands to a test-only Select of the subject path.
        Expr::Select(sel) if sel.test => {
            crate::cel_paths::select_path(expr).as_deref() == subject
        }
        // `isSet($)` — a DSL global with the subject as its sole argument.
        Expr::Call(c)
            if c.target.is_none()
                && c.func_name.eq_ignore_ascii_case("isSet")
                && c.args.len() == 1 =>
        {
            is_subject(&c.args[0].expr, subject)
        }
        _ => false,
    }
}

/// Reconstruct the subject's dotted path (`run.rank`, `scene.choices.x`). Returns
/// `None` for a non-path subject (`isSet(run.x)`, an empty/missing `on=`) — an
/// unresolved subject is treated as an infinite domain.
fn subject_path(m: &Match) -> Option<String> {
    let expr = parse_expr(&m.subject.raw)?;
    crate::cel_paths::select_path(&expr)
}

/// Throwaway re-parse of a raw CEL fragment into its root [`Expr`]. Per the
/// cel-parser 0.10.1 carry-forward (T3.1) the AST is structure-only, so a fresh
/// parse yields identical structure; malformed CEL (already reported in Phase 3)
/// yields `None`.
fn parse_expr(raw: &str) -> Option<Expr> {
    if raw.trim().is_empty() {
        return None;
    }
    let mut arena = CelArena::default();
    match lute_cel::parse_slot(&mut arena, raw, 0) {
        Ok(handle) => arena.get(handle).map(|root| root.expr.clone()),
        Err(_) => None,
    }
}

/// Build a `Layer::Logic` diagnostic (a §9/§11 logic check).
fn diag(code: &str, severity: Severity, message: String, span: Span) -> Diagnostic {
    Diagnostic {
        code: code.to_string(),
        severity,
        message,
        span,
        layer: Layer::Logic,
        fixits: Vec::new(),
        provenance: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use lute_core_span::StableId;
    use lute_syntax::ast::{CelKind, CelSlot};

    fn span() -> Span {
        Span { byte_start: 0, byte_end: 0, line: 1, column: 1, utf16_range: (0, 0) }
    }

    fn subject_slot(raw: &str) -> CelSlot {
        CelSlot { kind: CelKind::MatchSubject, raw: raw.into(), ast: None, span: span(), id: StableId(0) }
    }

    fn when_arm(test: &str) -> Arm {
        Arm::When {
            test: CelSlot { kind: CelKind::Condition, raw: test.into(), ast: None, span: span(), id: StableId(0) },
            body: Vec::new(),
            span: span(),
        }
    }

    /// A `<match on="run.rank">` over an enum subject: one `<when test="$ ==
    /// '<v>'">` per covered value, plus an optional `<otherwise>`.
    fn match_on_enum(_domain: &[&str], covered_arms: &[&str], has_otherwise: bool) -> Match {
        let mut arms: Vec<Arm> = covered_arms.iter().map(|v| when_arm(&format!("$ == '{v}'"))).collect();
        if has_otherwise {
            arms.push(Arm::Otherwise { body: Vec::new(), span: span() });
        }
        Match { subject: subject_slot("run.rank"), arms, span: span() }
    }

    /// `run.rank` declared as an enum WITH a default => finite, never unset.
    fn schema_enum_subject() -> StateSchema {
        let mut decls = BTreeMap::new();
        decls.insert(
            "run.rank".to_string(),
            StateDecl {
                ty: Type::Enum(vec!["fail".into(), "gold".into()]),
                default: Some(lute_manifest::types::Literal::Str("fail".into())),
                namespace: Namespace::Run,
            },
        );
        StateSchema { decls }
    }

    /// `run.rank` declared as an enum WITHOUT a default => finite but maybe-unset.
    fn schema_maybe_unset_subject() -> StateSchema {
        let mut decls = BTreeMap::new();
        decls.insert(
            "run.rank".to_string(),
            StateDecl {
                ty: Type::Enum(vec!["fail".into(), "gold".into()]),
                default: None,
                namespace: Namespace::Run,
            },
        );
        StateSchema { decls }
    }

    fn ctx() -> Ctx {
        Ctx::default()
    }

    #[test]
    fn enum_domain_without_otherwise_and_missing_arm_errors() {
        // subject domain {fail,gold}; arms cover only gold; no otherwise
        let m = match_on_enum(&["fail", "gold"], &["gold"], false);
        let errs = check_match(&m, &schema_enum_subject(), &ctx());
        assert!(errs.iter().any(|e| e.code == "E-NONEXHAUSTIVE"));
    }

    #[test]
    fn full_coverage_no_error() {
        let m = match_on_enum(&["fail", "gold"], &["fail", "gold"], false);
        let errs = check_match(&m, &schema_enum_subject(), &ctx());
        assert!(!errs.iter().any(|e| e.code == "E-NONEXHAUSTIVE"));
    }

    #[test]
    fn maybe_unset_subject_needs_unset_or_otherwise() {
        let m = match_on_enum(&["fail", "gold"], &["fail", "gold"], false); // no unset arm/otherwise
        let errs = check_match(&m, &schema_maybe_unset_subject(), &ctx());
        assert!(errs.iter().any(|e| e.code == "E-UNSET-UNCOVERED"));
    }

    // ---- helpers for the remaining behaviors --------------------------------

    fn match_with(subject: &str, arms: Vec<Arm>) -> Match {
        Match { subject: subject_slot(subject), arms, span: span() }
    }

    fn schema_bool(path: &str, default: Option<bool>) -> StateSchema {
        let mut decls = BTreeMap::new();
        decls.insert(
            path.to_string(),
            StateDecl {
                ty: Type::Bool,
                default: default.map(lute_manifest::types::Literal::Bool),
                namespace: crate::meta::namespace_of(path).unwrap_or(Namespace::Run),
            },
        );
        StateSchema { decls }
    }

    fn branch(id: &str, choice_ids: &[&str]) -> Branch {
        use lute_syntax::ast::Choice;
        let choices = choice_ids
            .iter()
            .map(|c| Choice {
                id: (*c).to_string(),
                label: String::new(),
                when: None,
                attrs: Vec::new(),
                body: Vec::new(),
                span: span(),
            })
            .collect();
        Branch { id: id.to_string(), attrs: Vec::new(), choices, span: span() }
    }

    // ---- E-NONEXHAUSTIVE / otherwise ----------------------------------------

    #[test]
    fn otherwise_makes_infinite_domain_exhaustive() {
        // number subject (infinite) is fine as long as `<otherwise>` is present.
        let mut decls = BTreeMap::new();
        decls.insert(
            "run.n".to_string(),
            StateDecl { ty: Type::Number, default: None, namespace: Namespace::Run },
        );
        let schema = StateSchema { decls };
        let m = match_with(
            "run.n",
            vec![when_arm("$ == 1"), Arm::Otherwise { body: Vec::new(), span: span() }],
        );
        assert!(check_match(&m, &schema, &ctx()).is_empty());
    }

    #[test]
    fn infinite_domain_without_otherwise_is_nonexhaustive() {
        let mut decls = BTreeMap::new();
        decls.insert(
            "run.n".to_string(),
            StateDecl { ty: Type::Number, default: Some(lute_manifest::types::Literal::Num(0.0)), namespace: Namespace::Run },
        );
        let schema = StateSchema { decls };
        let m = match_with("run.n", vec![when_arm("$ == 1"), when_arm("$ == 2")]);
        let errs = check_match(&m, &schema, &ctx());
        assert!(errs.iter().any(|e| e.code == "E-NONEXHAUSTIVE"));
    }

    // ---- bool domain --------------------------------------------------------

    #[test]
    fn bool_full_coverage_true_false_no_error() {
        // `<when test="$">` (true) + `<when test="!$">` (false) covers a bool.
        let m = match_with("scene.sealed", vec![when_arm("$"), when_arm("!$")]);
        let errs = check_match(&m, &schema_bool("scene.sealed", None), &ctx());
        assert!(errs.is_empty(), "bool fully covered, got {errs:?}");
    }

    #[test]
    fn bool_missing_false_is_nonexhaustive() {
        let m = match_with("scene.sealed", vec![when_arm("$")]);
        let errs = check_match(&m, &schema_bool("scene.sealed", None), &ctx());
        assert!(errs.iter().any(|e| e.code == "E-NONEXHAUSTIVE"));
    }

    // ---- E-UNSET-UNCOVERED coverage forms -----------------------------------

    #[test]
    fn unset_covered_by_null_arm_no_error() {
        // full enum coverage + `$ == null` arm covers the maybe-unset case.
        let m = match_with(
            "run.rank",
            vec![when_arm("$ == 'fail'"), when_arm("$ == 'gold'"), when_arm("$ == null")],
        );
        let errs = check_match(&m, &schema_maybe_unset_subject(), &ctx());
        assert!(!errs.iter().any(|e| e.code == "E-UNSET-UNCOVERED"), "got {errs:?}");
    }

    #[test]
    fn unset_covered_by_isset_negation_no_error() {
        let m = match_with(
            "run.rank",
            vec![when_arm("$ == 'fail'"), when_arm("$ == 'gold'"), when_arm("!isSet($)")],
        );
        let errs = check_match(&m, &schema_maybe_unset_subject(), &ctx());
        assert!(!errs.iter().any(|e| e.code == "E-UNSET-UNCOVERED"), "got {errs:?}");
    }

    #[test]
    fn defaulted_enum_full_coverage_is_not_unset_uncovered() {
        // WITH default => not maybe-unset => no E-UNSET-UNCOVERED even without an
        // unset arm. Also no E-NONEXHAUSTIVE (T4.4-interaction: this match IS
        // domain-exhaustive; T4.4 consumes `is_exhaustive` to drop its false +).
        let m = match_on_enum(&["fail", "gold"], &["fail", "gold"], false);
        let errs = check_match(&m, &schema_enum_subject(), &ctx());
        assert!(errs.is_empty(), "defaulted full-coverage enum should be clean, got {errs:?}");
    }

    // ---- age-gate (§11.2) ---------------------------------------------------

    #[test]
    fn age_gate_without_teen_or_otherwise_errors() {
        let mut decls = BTreeMap::new();
        decls.insert(
            "app.rating".to_string(),
            StateDecl {
                ty: Type::Enum(vec!["everyone".into(), "teen".into(), "mature".into()]),
                default: Some(lute_manifest::types::Literal::Str("everyone".into())),
                namespace: Namespace::App,
            },
        );
        let schema = StateSchema { decls };
        let m = match_with("app.rating", vec![when_arm("$ == 'everyone'"), when_arm("$ == 'mature'")]);
        let errs = check_match(&m, &schema, &ctx());
        assert!(errs.iter().any(|e| e.code == "E-AGE-GATE"), "got {errs:?}");
    }

    #[test]
    fn age_gate_with_teen_arm_ok() {
        let mut decls = BTreeMap::new();
        decls.insert(
            "app.rating".to_string(),
            StateDecl {
                ty: Type::Enum(vec!["everyone".into(), "teen".into(), "mature".into()]),
                default: Some(lute_manifest::types::Literal::Str("everyone".into())),
                namespace: Namespace::App,
            },
        );
        let schema = StateSchema { decls };
        let m = match_with(
            "app.rating",
            vec![when_arm("$ == 'everyone'"), when_arm("$ == 'teen'"), when_arm("$ == 'mature'")],
        );
        let errs = check_match(&m, &schema, &ctx());
        assert!(!errs.iter().any(|e| e.code == "E-AGE-GATE"), "got {errs:?}");
    }

    // ---- W-OVERLAP-ARMS (conservative) --------------------------------------

    #[test]
    fn duplicate_literal_arms_warn_overlap() {
        let m = match_on_enum(&["fail", "gold"], &["gold", "gold", "fail"], false);
        let warns = check_match(&m, &schema_enum_subject(), &ctx());
        let overlaps: Vec<_> = warns.iter().filter(|e| e.code == "W-OVERLAP-ARMS").collect();
        assert_eq!(overlaps.len(), 1, "exactly the duplicate `gold` arm warns, got {warns:?}");
        assert_eq!(overlaps[0].severity, Severity::Warning);
    }

    #[test]
    fn distinct_literal_arms_do_not_warn() {
        let m = match_on_enum(&["fail", "gold"], &["fail", "gold"], false);
        let warns = check_match(&m, &schema_enum_subject(), &ctx());
        assert!(!warns.iter().any(|e| e.code == "W-OVERLAP-ARMS"), "got {warns:?}");
    }

    // ---- scene.choices.<id> domain ------------------------------------------

    #[test]
    fn scene_choices_full_coverage_still_needs_unset() {
        // domain = {help, ignore} ∪ unset; cover both choice ids but not unset.
        let mut decls = BTreeMap::new();
        decls.insert(
            "scene.choices.couch".to_string(),
            StateDecl {
                ty: Type::Enum(vec!["help".into(), "ignore".into()]),
                default: None,
                namespace: Namespace::Scene,
            },
        );
        let schema = StateSchema { decls };
        let m = match_with(
            "scene.choices.couch",
            vec![when_arm("$ == 'help'"), when_arm("$ == 'ignore'")],
        );
        let errs = check_match(&m, &schema, &ctx());
        assert!(errs.iter().any(|e| e.code == "E-UNSET-UNCOVERED"), "got {errs:?}");
        assert!(!errs.iter().any(|e| e.code == "E-NONEXHAUSTIVE"), "choice ids fully covered: {errs:?}");
    }

    // ---- E-DUP-BRANCH + recording (§11.1) -----------------------------------

    #[test]
    fn branch_records_scene_choices_decl() {
        let mut seen = BTreeSet::new();
        let rec = check_branch(&branch("couch", &["help", "ignore"]), &mut seen);
        assert_eq!(rec.path, "scene.choices.couch");
        assert_eq!(rec.decl.namespace, Namespace::Scene);
        assert_eq!(rec.decl.ty, Type::Enum(vec!["help".into(), "ignore".into()]));
        assert!(rec.decl.default.is_none());
        assert!(rec.diags.is_empty());
    }

    #[test]
    fn duplicate_branch_id_errors_second_time() {
        let mut seen = BTreeSet::new();
        let first = check_branch(&branch("couch", &["help"]), &mut seen);
        assert!(first.diags.is_empty(), "first occurrence is clean");
        let second = check_branch(&branch("couch", &["help"]), &mut seen);
        assert!(second.diags.iter().any(|e| e.code == "E-DUP-BRANCH"), "got {:?}", second.diags);
    }

    // ---- is_exhaustive (T4.4 consumer) --------------------------------------

    #[test]
    fn is_exhaustive_true_for_full_finite_coverage() {
        let m = match_on_enum(&["fail", "gold"], &["fail", "gold"], false);
        assert!(is_exhaustive(&m, &schema_enum_subject()));
    }

    #[test]
    fn is_exhaustive_false_for_missing_unset() {
        let m = match_on_enum(&["fail", "gold"], &["fail", "gold"], false);
        assert!(!is_exhaustive(&m, &schema_maybe_unset_subject()));
    }

    #[test]
    fn is_exhaustive_true_with_otherwise() {
        let m = match_on_enum(&["fail", "gold"], &["gold"], true);
        assert!(is_exhaustive(&m, &schema_maybe_unset_subject()));
    }
}
