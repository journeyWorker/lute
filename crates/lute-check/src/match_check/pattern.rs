//! What a `<when>` arm provably matches: `is` literal patterns (incl. ranges,
//! dsl 0.18.0 §2) and conservatively-analyzed `test` guards.

use super::*;

/// What a single `<when>` arm provably matches.
#[derive(Default)]
pub(super) struct ArmCoverage {
    /// Concrete finite-domain values (`bool`/enum-string) the arm covers.
    pub(super) values: Vec<DomainValue>,
    /// Numeric points/ranges the arm covers (dsl 0.18.0 §4).
    pub(super) intervals: Vec<Interval>,
    /// Whether the arm covers the `unset` case.
    pub(super) covers_unset: bool,
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

/// The full coverage of a `<when>` arm. `is` and `test` together mean
/// "pattern AND guard" (dsl §7.3.1), so an arm carrying both provably
/// matches only the INTERSECTION of its `is` literal pattern (the NORMATIVE
/// path, dsl §11.2) and its `test` guard. A pattern value survives when the
/// guard provably holds for it: either [`analyze_arm`] proves the guard
/// covers it, or the guard decides `true` with `$` bound to that very value
/// (`is="gold" test="$ != 'x'"`). An undecidable guard (`test="run.proof"`)
/// proves nothing, so such an arm covers nothing: it can neither shadow a
/// later arm (`W-OVERLAP-ARMS`) nor complete the domain (`E-NONEXHAUSTIVE`).
/// Both [`check_match`] and [`is_exhaustive`] fold coverage through here so
/// they stay consistent.
pub(super) fn arm_coverage(
    is: Option<&IsPattern>,
    test_raw: &str,
    subject: Option<&str>,
    schema: &StateSchema,
) -> ArmCoverage {
    let test = analyze_arm(test_raw, subject);
    let Some(pat) = is else {
        return test;
    };
    let mut pattern = ArmCoverage::default();
    analyze_is_pattern(&pat.raw, subject, &mut pattern);
    if test_raw.trim().is_empty() {
        return pattern;
    }
    let guard_holds_at = |value: crate::decide::Decided| {
        let no_params = std::collections::BTreeMap::new();
        let no_bodies = std::collections::BTreeMap::new();
        let no_def_params = std::collections::BTreeMap::new();
        let ctx = crate::decide::DecideCtx {
            schema,
            dollar: Some(crate::decide::DollarBinding::Value(value)),
            params: &no_params,
            facts: None,
        };
        let defs = crate::cel_expand::DefTable {
            bodies: &no_bodies,
            params: &no_def_params,
        };
        matches!(
            crate::decide::decide_slot(test_raw, &defs, &ctx),
            Some(crate::decide::Decided::Bool(true))
        )
    };
    ArmCoverage {
        values: pattern
            .values
            .into_iter()
            .filter(|v| {
                test.values.contains(v)
                    || guard_holds_at(match v {
                        DomainValue::Str(s) => crate::decide::Decided::Str(s.clone()),
                        DomainValue::Bool(b) => crate::decide::Decided::Bool(*b),
                    })
            })
            .collect(),
        intervals: pattern
            .intervals
            .iter()
            .flat_map(|a| {
                if a.is_point() && guard_holds_at(crate::decide::Decided::Num(a.lo)) {
                    return vec![*a];
                }
                test.intervals
                    .iter()
                    .filter_map(|b| a.intersect(*b))
                    .collect()
            })
            .collect(),
        covers_unset: pattern.covers_unset && test.covers_unset,
    }
}

/// dsl 0.23.1 (ashen N3): `true` iff the arm's own `is=` pattern proves the
/// subject SET inside the arm — it names at least one value and none of its
/// alternatives is `unset`, so an unset subject never matches it (the
/// arm's `test` can only narrow further).
pub(crate) fn is_pattern_proves_set(is: Option<&IsPattern>, subject: Option<&str>) -> bool {
    let Some(pat) = is else {
        return false;
    };
    let mut alternatives = is_alternatives(&pat.raw).peekable();
    alternatives.peek().is_some()
        && alternatives.all(|lit| {
            !matches!(
                classify_is_literal(lit).map(|l| quest_state_is_literal(l, subject)),
                Ok(IsLiteral::Unset)
            )
        })
}

/// dsl 0.23.1 (ashen N3): `true` iff the `<when>` arm provably takes EVERY
/// unset subject value (`is="unset"` with no narrowing `test`, or a test
/// like `!isSet($)`) — no later arm and no `<otherwise>` sees the subject
/// unset.
pub(crate) fn arm_takes_unset(
    is: Option<&IsPattern>,
    test_raw: &str,
    subject: Option<&str>,
    schema: &StateSchema,
) -> bool {
    arm_coverage(is, test_raw, subject, schema).covers_unset
}

/// Parse a `<when is="…">` literal pattern (dsl §7.3.1) into `cov`: every
/// alternative ([`is_alternatives`]) is classified by the shared
/// [`classify_is_literal`] — `true`/`false` are bool domain values, `unset`
/// covers the unset case (§9.4) — or, on a `quest.<id>.state` subject, the
/// `unset` member ([`quest_state_is_literal`]) — a decimal `Number` is a
/// point interval, a range (dsl 0.18.0 §2) its closed interval, and any other
/// ident is an enum member matched by string equality on the subject (§8.2).
/// A malformed or empty range (`E-WHEN-RANGE`) covers nothing.
fn analyze_is_pattern(raw: &str, subject: Option<&str>, cov: &mut ArmCoverage) {
    for lit in is_alternatives(raw) {
        match classify_is_literal(lit).map(|l| quest_state_is_literal(l, subject)) {
            Ok(IsLiteral::Bool(b)) => cov.values.push(DomainValue::Bool(b)),
            Ok(IsLiteral::Unset) => cov.covers_unset = true,
            Ok(IsLiteral::Str(s)) => cov.values.push(DomainValue::Str(s)),
            Ok(lit @ (IsLiteral::Num(_) | IsLiteral::Range(_))) => {
                cov.intervals.extend(Interval::of(&lit));
            }
            Err(_) => {}
        }
    }
}

/// The `quest.<id>.state` lifecycle members, in IR domain order (0.21.1 T1-1;
/// `lute-compile` emits the same list — the folded enum plus `unset`).
pub(crate) const QUEST_STATES: &[&str] = &["active", "complete", "failed", "unset"];

/// 0.21.1 T1-1: on a `quest.<id>.state` subject, `<when is="unset">` names the
/// lifecycle MEMBER `unset` — the value the engine stores before the quest
/// activates — not the never-set sentinel (that subject is always assigned,
/// [`infer_domain`]). dsl 0.24.0 §2: likewise on `quest.<id>.failedBy`,
/// whose `unset` member means "has not failed". Every other literal, and
/// every other subject, passes through unchanged. Applied wherever the
/// checker classifies an `is=` literal, so coverage, `E-WHEN-LITERAL-DOMAIN`
/// and reachability agree.
pub(crate) fn quest_state_is_literal(lit: IsLiteral, subject: Option<&str>) -> IsLiteral {
    let unset_is_member = |s: &str| {
        crate::cel_paths::is_reserved_quest_state(s)
            || crate::cel_paths::is_reserved_quest_failed_by(s)
    };
    match lit {
        IsLiteral::Unset if subject.is_some_and(unset_is_member) => {
            IsLiteral::Str("unset".to_string())
        }
        other => other,
    }
}

/// Per-literal sub-spans of a `<when is="…">` pattern (dsl §7.3.1, 0.4.0
/// §5.2): split on `|` exactly like [`analyze_is_pattern`], but keep each
/// trimmed literal's own byte range instead of folding it into coverage —
/// `E-WHEN-LITERAL-DOMAIN` must point AT THE LITERAL, not the whole pattern
/// (the §5.4 worked example). Byte offsets are computed relative to
/// `span.byte_start`; `line`/`column`/`utf16_range` are filled in from the
/// literal's own text (a same-line estimate — `check()`'s `normalize_spans`
/// pass, check.rs, re-derives every diagnostic's display position from
/// `byte_start`/`byte_end` before returning). An empty alternative (a stray
/// `|`) is skipped, matching `analyze_is_pattern`. `pub`: reused by Task 4
/// (subsumption per-literal identity), Task 7 (param `is=` checks), and
/// Task 8 (`lute-compile`'s §6.4 `fold_component_matches` — is-arm literal
/// membership against a decided subject constant).
pub fn is_pattern_literals(raw: &str, span: Span) -> Vec<(String, Span)> {
    let mut out = Vec::new();
    let mut offset = 0usize;
    for part in raw.split('|') {
        let trimmed = part.trim();
        if !trimmed.is_empty() {
            let lead = part.len() - part.trim_start().len();
            let rel = offset + lead;
            let start = span.byte_start + rel;
            let end = start + trimmed.len();
            // `raw` is one line: the prefix advances the character column and
            // the UTF-16 offset by its own count, never by its byte length.
            let prefix = &raw[..rel];
            let u16_start = span.utf16_range.0 + prefix.encode_utf16().count() as u32;
            out.push((
                trimmed.to_string(),
                Span {
                    byte_start: start,
                    byte_end: end,
                    line: span.line,
                    column: span.column + prefix.chars().count() as u32,
                    utf16_range: (u16_start, u16_start + trimmed.encode_utf16().count() as u32),
                },
            ));
        }
        offset += part.len() + 1;
    }
    out
}

/// Whether a classified `is=` literal is PROVABLY outside `dom`'s domain
/// (dsl 0.4.0 §5.2 rules 1-4; ranges dsl 0.18.0 §2). Requires `dom.resolved`
/// — an unresolvable subject (an unparseable `on=`, an undeclared path)
/// makes no domain claim, so nothing here is ever flagged (§5.1's Closure:
/// no unprovable pile-on). `unset` is checked against `maybe_unset`
/// regardless of domain shape (rule 3, including an `Infinite` subject —
/// rule 4). A numeric RANGE is foreign to every resolved non-`Number`
/// domain (a finite `bool`/`enum`/choice domain, or a declared `string`/
/// opaque subject). Every other literal kind is checked only when the domain
/// is `Finite` (rules 1-2) — a `Number`/`Infinite` subject makes no finite
/// claim to violate, so a point `Number` keeps its 0.4.0 behavior.
pub(crate) fn literal_is_foreign(lit: &IsLiteral, dom: &DomainInfo) -> bool {
    if !dom.resolved {
        return false;
    }
    match (lit, &dom.domain) {
        (IsLiteral::Unset, _) => !dom.maybe_unset,
        (IsLiteral::Range(_) | IsLiteral::Num(_), Domain::IntRange { lo, hi }) => {
            let iv = Interval::of(lit).expect("a numeric literal has an interval");
            let first = iv.lo.ceil().max(*lo as f64);
            let last = iv.hi.floor().min(*hi as f64);
            first > last
        }
        (IsLiteral::Range(_), domain) => !matches!(domain, Domain::Number),
        (IsLiteral::Bool(b), Domain::Finite(vals)) => !vals.contains(&DomainValue::Bool(*b)),
        (IsLiteral::Str(s), Domain::Finite(vals)) => !vals
            .iter()
            .any(|v| matches!(v, DomainValue::Str(x) if x == s)),
        // `Domain::Finite` is always bool/enum; a Num never fits.
        (IsLiteral::Num(_), Domain::Finite(_)) => true,
        (IsLiteral::Bool(_) | IsLiteral::Str(_), Domain::IntRange { .. }) => true,
        // dsl 0.28.0 §1 (T1-4): a number subject is matched by numbers and
        // ranges; `<=3` or `high` is compared as text and never matches.
        (IsLiteral::Bool(_) | IsLiteral::Str(_), Domain::Number) => true,
        (_, Domain::Number | Domain::Infinite) => false,
    }
}

/// Build the `E-WHEN-LITERAL-DOMAIN` message (dsl 0.4.0 §5.2): names the
/// offending literal and, for a `Finite` domain, its members (matching the
/// §5.4 worked example: `` `platnum` is not a member of the subject's domain
/// [fail, bronze, silver, gold] ``). The `unset`-on-a-never-unset-subject
/// case (rules 3-4) and a range against a non-numeric subject (dsl 0.18.0
/// §2) state the reason directly instead.
pub(super) fn foreign_literal_message(
    lit_display: &str,
    lit: &IsLiteral,
    domain: &Domain,
) -> String {
    match (lit, domain) {
        (IsLiteral::Unset, _) => "`unset` is not a member of the subject's domain: this \
                                  subject can never be unset (dsl 0.4 §5.2)"
            .to_string(),
        (_, Domain::IntRange { lo, hi }) => format!(
            "`{lit_display}` matches none of the subject's values, the whole numbers {lo}..{hi} \
             (dsl 0.24.0 §1)"
        ),
        (IsLiteral::Bool(_) | IsLiteral::Str(_), Domain::Number) => {
            let bound = |p: &str| lit_display.strip_prefix(p).map(str::trim);
            let hint = if let Some(n) = bound("<=") {
                format!(" — write `..{n}`")
            } else if let Some(n) = bound(">=") {
                format!(" — write `{n}..`")
            } else if let Some((sym, n)) = bound("<")
                .map(|n| ("<", n))
                .or(bound(">").map(|n| (">", n)))
            {
                format!(" — a range includes its bounds; write `test=\"$ {sym} {n}\"`")
            } else {
                String::new()
            };
            format!(
                "`{lit_display}` is not a number or a range, so it never matches this number \
                 subject — `is=` on a number takes numbers and ranges (`3`, `1..3`, `..3`, \
                 `4..`){hint} (dsl 0.28.0 §1)"
            )
        }
        (IsLiteral::Range(_), _) => format!(
            "`{lit_display}` is a numeric range, which cannot match a non-numeric subject \
             (dsl 0.18.0 §2)"
        ),
        (_, Domain::Finite(vals)) => {
            let display = domain_members_display(vals);
            let members = || display.split(", ").filter(|m| !m.is_empty());
            let parts: Vec<&str> = lit_display.split(',').map(str::trim).collect();
            let hint = if parts.len() > 1 && parts.iter().all(|p| members().any(|m| m == *p)) {
                format!(
                    " — separate alternatives with `|`: `is=\"{}\"`",
                    parts.join("|")
                )
            } else {
                lute_manifest::suggest::nearest(lit_display, members(), 2)
                    .map_or_else(String::new, |near| format!(" — did you mean `{near}`?"))
            };
            format!(
                "`{lit_display}` is not a member of the subject's domain [{display}]{hint} \
                 (dsl 0.4 §5.2)"
            )
        }
        (_, Domain::Number | Domain::Infinite) => {
            unreachable!("rule 4: a non-finite domain only ever flags `unset` or a range")
        }
    }
}

/// Build the `E-WHEN-RANGE` message (dsl 0.18.0 §2) for a literal the shared
/// classifier rejected.
pub(super) fn bad_range_message(lit_display: &str, err: IsLiteralError) -> String {
    match err {
        IsLiteralError::MalformedRange => format!(
            "malformed range literal `{lit_display}`: a range is `N..M`, `N..`, or `..M` with \
             decimal number bounds (dsl 0.18.0 §2)"
        ),
        IsLiteralError::EmptyRange => format!(
            "empty range literal `{lit_display}`: its lower bound is above its upper bound, so \
             it matches nothing (dsl 0.18.0 §2)"
        ),
    }
}

/// Render a `Domain::Finite` member list for a diagnostic message, e.g.
/// `fail, bronze, silver, gold` (bare, unquoted — matches the §5.4 worked
/// example's `[fail, bronze, silver, gold]`).
pub(super) fn domain_members_display(vals: &[DomainValue]) -> String {
    vals.iter()
        .map(|v| match v {
            DomainValue::Str(s) => s.clone(),
            DomainValue::Bool(b) => b.to_string(),
        })
        .collect::<Vec<_>>()
        .join(", ")
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
        Expr::Select(sel) if sel.test => crate::cel_paths::select_path(expr).as_deref() == subject,
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

/// Throwaway re-parse of a raw CEL fragment into its root [`Expr`]. Per the
/// cel-parser 0.10.1 carry-forward (T3.1) the AST is structure-only, so a fresh
/// parse yields identical structure; malformed CEL (already reported in Phase 3)
/// yields `None`.
pub(super) fn parse_expr(raw: &str) -> Option<Expr> {
    if raw.trim().is_empty() {
        return None;
    }
    let mut arena = CelArena::default();
    match lute_cel::parse_slot(&mut arena, raw, 0) {
        Ok(handle) => arena.get(handle).map(|root| root.expr.clone()),
        Err(_) => None,
    }
}
