//! The `<match>` exhaustiveness engine (dsl §11.2, 0.4.0 §6.3): coverage,
//! unset coverage, the age gate, overlapping and duplicate arms.

use super::*;

/// `E-WHEN-PATTERN`: a `<when>` arm carrying neither an `is` literal pattern nor
/// a `test` guard (dsl §7.3.1, D-D). One of the two is REQUIRED — an empty
/// `<when>` has nothing to match on.
pub const E_WHEN_PATTERN: &str = "E-WHEN-PATTERN";

/// `E-WHEN-LITERAL-DOMAIN`: an `<when is="…">` literal outside the subject's
/// decided finite domain (dsl 0.4 §5.2, §6.3) — a foreign enum member (a
/// typo), a number/bool literal against a mismatched domain, or `unset` on a
/// subject that is never unset (a defaulted path; a component param, §6.3).
/// Such an arm can never fire and the cause is the literal itself; this code
/// OWNS that root — it never additionally piles on `E-ARM-DEAD` for the same
/// arm (D4), and the foreign literal contributes nothing to coverage/
/// subsumption downstream.
pub const E_WHEN_LITERAL_DOMAIN: &str = "E-WHEN-LITERAL-DOMAIN";

/// `E-WHEN-RANGE` (dsl 0.18.0 §2): a `<when is="…">` alternative containing
/// `..` that is not a valid range literal — malformed (`..`, `a..b`,
/// `1...2`, `1..2..3`) or empty (`3..1`, lower bound above upper). Anchored
/// at the literal's own span ([`is_pattern_literals`]). Such a literal
/// covers nothing: it is skipped by coverage, overlap, subsumption, and the
/// `E-WHEN-LITERAL-DOMAIN` domain check.
pub const E_WHEN_RANGE: &str = "E-WHEN-RANGE";

/// `E-MATCH-NO-SUBJECT`: a `<when is="…">` arm in a `<match>` with no `on=`.
/// An `is` literal is compared against the subject, and there is none, so the
/// arm could never be decided. A `<match>` with no `on` whose arms are all
/// `test=` stays legal (the form for fact queries).
pub const E_MATCH_NO_SUBJECT: &str = "E-MATCH-NO-SUBJECT";

/// [`E_MATCH_NO_SUBJECT`] at every `is=` value of a subject-less `<match>`.
/// A value that reads as a condition (`run.lamps >= 3`) is pointed at
/// `test=`; otherwise the message names the one declared finite path whose
/// members hold every `is` literal, when there is exactly one.
pub(crate) fn check_match_has_subject(m: &Match, schema: &StateSchema) -> Vec<Diagnostic> {
    if !m.subject.raw.trim().is_empty() {
        return Vec::new();
    }
    let patterns: Vec<&IsPattern> = m
        .arms
        .iter()
        .filter_map(|a| match a {
            Arm::When { is: Some(p), .. } => Some(p),
            _ => None,
        })
        .collect();
    if patterns.is_empty() {
        return Vec::new();
    }
    let subject = subject_for(&patterns, schema)
        .map_or_else(|| "on=\"…\"".to_string(), |p| format!("on=\"{p}\""));
    patterns
        .iter()
        .map(|p| {
            let raw = p.raw.trim();
            let message = if reads_as_condition(raw) {
                format!(
                    "`is=\"{raw}\"` compares against the `<match on>` subject; this `<match>` \
                     has none, and `{raw}` is a condition — write `<when test=\"{raw}\">`"
                )
            } else {
                format!(
                    "`is=\"{raw}\"` compares against the `<match on>` subject; this `<match>` \
                     has none — add `{subject}` to the `<match>`, or write `test=` for a \
                     condition"
                )
            };
            diag(E_MATCH_NO_SUBJECT, Severity::Error, message, p.span)
        })
        .collect()
}

/// An `is=` value no literal pattern spells: an operator, a call, a `@def`,
/// or a state path.
fn reads_as_condition(raw: &str) -> bool {
    raw.contains(['<', '>', '=', '!', '&', '(', '@'])
        || raw.contains("||")
        || ["scene.", "run.", "user.", "app.", "quest.", "clock."]
            .iter()
            .any(|t| raw.starts_with(t))
}

/// The one declared path with a finite domain holding every member literal
/// of `patterns`, if exactly one does.
fn subject_for(patterns: &[&IsPattern], schema: &StateSchema) -> Option<String> {
    let lits: Vec<IsLiteral> = patterns
        .iter()
        .flat_map(|p| is_alternatives(&p.raw))
        .filter_map(|lit| classify_is_literal(lit).ok())
        .filter(|lit| matches!(lit, IsLiteral::Str(_)))
        .collect();
    if lits.is_empty() {
        return None;
    }
    // A `prev.*` mirror holds the last run's value of the path beside it.
    let mut fits = schema.decls.keys().filter(|path| {
        let info = infer_domain(Some(path.as_str()), schema);
        !path.starts_with("prev.")
            && matches!(info.domain, Domain::Finite(_))
            && info.resolved
            && !lits.iter().any(|lit| literal_is_foreign(lit, &info))
    });
    let one = fits.next()?;
    fits.next().is_none().then(|| one.clone())
}

/// Validate a `<match>` for exhaustiveness, unset coverage, the age-gate, and
/// provably-overlapping arms (dsl §11.2). Thin wrapper: infers the subject's
/// domain from `schema` exactly as before, then delegates to
/// [`check_match_with_domain`] (0.4.0 T7 — the SAME engine [`check_param_match`]
/// drives over a component param's domain).
pub fn check_match(m: &Match, schema: &StateSchema, ctx: &Ctx<'_>) -> Vec<Diagnostic> {
    let subject = subject_path(m);
    let dom = infer_domain(subject.as_deref(), schema);
    check_match_with_domain(m, subject.as_deref(), dom, ctx, &|_| false)
}

/// One value a `<match>` subject can take, as coverage and narrowing see it
/// (dsl 0.4.0 §5.2, 0.18.0 §4): a finite member, a numeric point or range,
/// or `unset`.
pub(crate) enum CoverItem {
    Value(DomainValue),
    Num(Interval),
    Unset,
}

/// The shared `<match>` engine (dsl §11.2, 0.4.0 §6.3): exhaustiveness, unset
/// coverage, the age-gate, and provably-overlapping arms, over an
/// ALREADY-RESOLVED subject path and `dom`ain. [`check_match`] infers `dom`
/// from a scene's `state:` schema; [`check_param_match`] (0.4.0 T7) passes a
/// component param's [`param_domain`] instead — same rules, same codes,
/// different domain source (§6.3: "apply inside component bodies exactly as
/// at scene level"); a `@def` subject arrives through [`resolve_subject`].
/// `ruled_out` is what the enclosing body's beat/entry `when` excludes as the
/// subject's value (dsl 0.24.0): such a member needs no arm, exactly as
/// `E-ARM-DEAD` and `W-OTHERWISE-DEAD` already read the narrowed domain.
pub(crate) fn check_match_with_domain(
    m: &Match,
    subject: Option<&str>,
    info: DomainInfo,
    ctx: &Ctx<'_>,
    ruled_out: &dyn Fn(&CoverItem) -> bool,
) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    let has_otherwise = m.arms.iter().any(|a| matches!(a, Arm::Otherwise { .. }));

    // §11.2: a `<match>` admits AT MOST ONE `<otherwise>`. With more than one,
    // flatten routes only the last, making earlier otherwise bodies unreachable
    // — so flag every otherwise past the first at its own span (mirroring the
    // per-repeat shape of E-CHOICE-DUP).
    let mut seen_otherwise = false;
    for arm in &m.arms {
        if let Arm::Otherwise { span, .. } = arm {
            if seen_otherwise {
                diags.push(diag(
                    "E-MATCH-DUP-OTHERWISE",
                    Severity::Error,
                    "duplicate `<otherwise>` in `<match>`; at most one `<otherwise>` is allowed \
                     (dsl §11.2)"
                        .to_string(),
                    *span,
                ));
            }
            seen_otherwise = true;
        }
    }

    // One ordered pass over the `<when>` arms: accumulate covered values (+ the
    // `unset` case, + numeric intervals, dsl 0.18.0 §4) and flag a
    // provably-dead overlap. First-match-wins means an arm whose concrete
    // value was already covered by an EARLIER arm is dead.
    // T3-1: a subject whose own declaration was reported gets no domain
    // verdicts (a literal outside it, a member left uncovered, `unset`).
    let faulty = subject.is_some_and(|p| ctx.env.state.is_faulty(p));
    let mut covered: BTreeSet<DomainValue> = BTreeSet::new();
    let mut covered_num = NumCoverage::default();
    let mut covers_unset = false;
    for arm in &m.arms {
        if let Arm::When { is, test, span, .. } = arm {
            // §7.3.1 (D-D): a `<when>` needs an `is` pattern and/or a `test` guard.
            if is.is_none() && test.raw.trim().is_empty() {
                diags.push(diag(
                    E_WHEN_PATTERN,
                    Severity::Error,
                    "`<when>` needs an `is` literal pattern and/or a `test` guard (dsl §7.3.1)"
                        .to_string(),
                    *span,
                ));
            }
            for (lit_raw, lit_span) in is
                .as_ref()
                .map(|pat| is_pattern_literals(&pat.raw, pat.span))
                .unwrap_or_default()
            {
                let lit = match classify_is_literal(&lit_raw) {
                    Ok(lit) => quest_state_is_literal(lit, subject),
                    Err(err) => {
                        diags.push(diag(
                            E_WHEN_RANGE,
                            Severity::Error,
                            bad_range_message(&lit_raw, err),
                            lit_span,
                        ));
                        continue;
                    }
                };
                if !faulty && literal_is_foreign(&lit, &info) {
                    diags.push(diag(
                        E_WHEN_LITERAL_DOMAIN,
                        Severity::Error,
                        foreign_literal_message(&lit_raw, &lit, &info.domain),
                        lit_span,
                    ));
                }
            }
            let cov = arm_coverage(is.as_ref(), &test.raw, subject, &ctx.env.state);
            if cov.values.iter().any(|v| covered.contains(v))
                || cov.intervals.iter().any(|iv| covered_num.overlaps(*iv))
            {
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
            for iv in cov.intervals {
                covered_num.add(iv);
            }
            covers_unset |= cov.covers_unset;
        }
    }

    // Age-gate special case (§11.2): an age-gated `<match on="app.rating">` MUST
    // carry a `teen` arm or an `<otherwise>` — a release-build hard gate.
    if subject == Some("app.rating")
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
    // dsl 0.25.0 §9 (SU N8): an undeclared subject path has no domain to be
    // exhaustive over — its read is reported once (`E-UNDECLARED`, or under a
    // broken schema import the import error), never as a follow-on
    // `E-NONEXHAUSTIVE` pointing at the match.
    let undeclared = subject
        .is_some_and(|p| !info.resolved && !crate::defassign::is_declared(p, &ctx.env.state));
    if has_otherwise || undeclared || faulty {
        return diags;
    }

    let (fully_covered, gap) = match &info.domain {
        Domain::Finite(vals) => {
            let missing: Vec<DomainValue> = vals
                .iter()
                .filter(|v| !covered.contains(v) && !ruled_out(&CoverItem::Value((*v).clone())))
                .cloned()
                .collect();
            let shown: Vec<String> = missing
                .iter()
                .map(|v| domain_members_display(std::slice::from_ref(v)))
                .collect();
            (missing.is_empty(), not_covered(&shown))
        }
        Domain::IntRange { lo, hi } => {
            let missing: Vec<String> = covered_num
                .uncovered_ints(*lo, *hi)
                .into_iter()
                .filter(|k| {
                    let p = *k as f64;
                    !ruled_out(&CoverItem::Num(Interval { lo: p, hi: p }))
                })
                .map(|k| k.to_string())
                .collect();
            // A finite clock's `clock.index` may span many values (dsl
            // 0.27.0 §4): name the first few and count the rest.
            let shown = if missing.len() > 8 {
                let mut shown = missing[..6].to_vec();
                shown.push(format!("… ({} more)", missing.len() - 6));
                shown
            } else {
                missing.clone()
            };
            (missing.is_empty(), not_covered(&shown))
        }
        Domain::Number => (covered_num.covers_all(), covered_num.first_gap()),
        Domain::IntNumber => (covered_num.covers_all(), covered_num.first_gap()),
        Domain::Infinite => (false, None),
    };
    if !fully_covered {
        let message = match (gap, &info.domain) {
            (Some(gap), Domain::Number | Domain::IntNumber | Domain::IntRange { .. }) => format!(
                "non-exhaustive `<match>`: {gap} and there is no `<otherwise>` (dsl 0.18.0 §4)"
            ),
            (Some(gap), _) => {
                format!("non-exhaustive `<match>`: {gap} and there is no `<otherwise>` (dsl §11.2)")
            }
            (None, _) => "non-exhaustive `<match>`: the subject's domain is not fully covered and \
                     there is no `<otherwise>` (dsl §11.2)"
                .to_string(),
        };
        diags.push(diag("E-NONEXHAUSTIVE", Severity::Error, message, m.span));
    }

    // A maybe-unset subject's `unset` case must be covered (§11.2/§9.4). This is
    // scoped to subjects whose nullability is derivable from the schema alone —
    // `run`/`user`/`app` (maybe-unset at scene entry). A `scene.*` subject's
    // maybe-unset status is path-sensitive — a branch's `scene.choices.*` pick
    // record is set once the branch picked (dsl 0.28.0, T3-61) — so it is owned
    // by `check_definite_assignment` (E-MAYBE-UNSET, which names the branch);
    // emitting E-UNSET-UNCOVERED here would false-positive the written case.
    let unset_owned_here = subject.is_some_and(|p| !p.starts_with("scene."));
    if info.maybe_unset && unset_owned_here && !covers_unset && !ruled_out(&CoverItem::Unset) {
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

/// Validate a component-body param-subject `<match>` (dsl 0.4.0 §6.3):
/// delegates wholesale to [`check_match_with_domain`] over `dom` (the
/// dispatched param's [`param_domain`]) — `E-NONEXHAUSTIVE`,
/// `E-MATCH-DUP-OTHERWISE`, `W-OVERLAP-ARMS`, `E-WHEN-LITERAL-DOMAIN`, and
/// `E-AGE-GATE` all apply exactly as at scene level (§6.3's own text).
/// `E-UNSET-UNCOVERED` never fires here: `dom.maybe_unset` is always
/// `false` for a param domain, so the check's own `info.maybe_unset` guard
/// is structurally unreachable — no special-casing needed, the SAME engine
/// proves it by construction.
pub(crate) fn check_param_match(m: &Match, dom: DomainInfo, ctx: &Ctx<'_>) -> Vec<Diagnostic> {
    check_match_with_domain(m, subject_path(m).as_deref(), dom, ctx, &|_| false)
}

/// "`a` is not covered" / "`a`, `b` are not covered" — `None` for none.
fn not_covered(missing: &[String]) -> Option<String> {
    match missing {
        [] => None,
        [one] => Some(format!("`{one}` is not covered")),
        many => Some(format!(
            "{} are not covered",
            many.iter()
                .map(|v| format!("`{v}`"))
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

/// Whether a `<match>` is provably exhaustive (dsl §11.2): it has an
/// `<otherwise>`, or its finite domain — or, for a `number` subject, the whole
/// real line (dsl 0.18.0 §4) — including the `unset` member when the
/// subject is maybe-unset, is fully covered by the `<when>` arms. Exposed for
/// T4.4 (definite-assignment) so a domain-exhaustive match without `<otherwise>`
/// is not treated as a possible fall-through (its arms' join is an intersection,
/// not the pre-block set). See the report's "exhaustiveness result shape".
pub fn is_exhaustive(m: &Match, schema: &StateSchema) -> bool {
    let subject = subject_path(m);
    let info = infer_domain(subject.as_deref(), schema);
    is_exhaustive_resolved(m, subject.as_deref(), &info, schema)
}

/// [`is_exhaustive`] over an already-resolved subject path and domain
/// ([`resolve_subject`]).
pub(crate) fn is_exhaustive_resolved(
    m: &Match,
    subject: Option<&str>,
    info: &DomainInfo,
    schema: &StateSchema,
) -> bool {
    if m.arms.iter().any(|a| matches!(a, Arm::Otherwise { .. })) {
        return true;
    }
    let mut covered: BTreeSet<DomainValue> = BTreeSet::new();
    let mut covered_num = NumCoverage::default();
    let mut covers_unset = false;
    for arm in &m.arms {
        if let Arm::When { is, test, .. } = arm {
            let cov = arm_coverage(is.as_ref(), &test.raw, subject, schema);
            for v in cov.values {
                covered.insert(v);
            }
            for iv in cov.intervals {
                covered_num.add(iv);
            }
            covers_unset |= cov.covers_unset;
        }
    }
    let domain_covered = match &info.domain {
        Domain::Finite(vals) => vals.iter().all(|v| covered.contains(v)),
        Domain::Number => covered_num.covers_all(),
        Domain::IntNumber => covered_num.covers_all(),
        Domain::IntRange { lo, hi } => covered_num.uncovered_ints(*lo, *hi).is_empty(),
        Domain::Infinite => false,
    };
    domain_covered && (!info.maybe_unset || covers_unset)
}

/// `true` iff an unset subject can never fall out of `m` unhandled: an
/// `<otherwise>` or a `<when>` arm that takes the unset case. A match that
/// misses a member is `E-NONEXHAUSTIVE` on its own; its subject read is
/// still guarded against absence, so it earns no second report.
pub(crate) fn handles_unset_resolved(
    m: &Match,
    subject: Option<&str>,
    schema: &StateSchema,
) -> bool {
    m.arms.iter().any(|a| match a {
        Arm::Otherwise { .. } => true,
        Arm::When { is, test, .. } => {
            arm_coverage(is.as_ref(), &test.raw, subject, schema).covers_unset
        }
    })
}
