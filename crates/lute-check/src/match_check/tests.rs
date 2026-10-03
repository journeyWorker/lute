use super::*;
use crate::ctx::Env;
use lute_core_span::StableId;
use lute_syntax::ast::{CelKind, CelSlot};
use std::collections::BTreeMap;
use std::sync::LazyLock;

fn span() -> Span {
    Span {
        byte_start: 0,
        byte_end: 0,
        line: 1,
        column: 1,
        utf16_range: (0, 0),
    }
}

fn subject_slot(raw: &str) -> CelSlot {
    CelSlot {
        kind: CelKind::MatchSubject,
        raw: raw.into(),
        ast: None,
        span: span(),
        id: StableId(0),
        authored: None,
    }
}

fn when_arm(test: &str) -> Arm {
    Arm::When {
        attrs: Vec::new(),
        is: None,
        test: CelSlot {
            kind: CelKind::Condition,
            raw: test.into(),
            ast: None,
            span: span(),
            id: StableId(0),
            authored: None,
        },
        body: Vec::new(),
        span: span(),
    }
}

/// A `<match on="run.rank">` over an enum subject: one `<when test="$ ==
/// '<v>'">` per covered value, plus an optional `<otherwise>`.
fn match_on_enum(_domain: &[&str], covered_arms: &[&str], has_otherwise: bool) -> Match {
    let mut arms: Vec<Arm> = covered_arms
        .iter()
        .map(|v| when_arm(&format!("$ == '{v}'")))
        .collect();
    if has_otherwise {
        arms.push(Arm::Otherwise {
            attrs: Vec::new(),
            body: Vec::new(),
            span: span(),
        });
    }
    Match {
        attrs: Vec::new(),
        subject: subject_slot("run.rank"),
        arms,
        span: span(),
    }
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
            owner: None,
        },
    );
    StateSchema {
        decls,
        ..Default::default()
    }
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
            owner: None,
        },
    );
    StateSchema {
        decls,
        ..Default::default()
    }
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
    Match {
        attrs: Vec::new(),
        subject: subject_slot(subject),
        arms,
        span: span(),
    }
}

fn schema_bool(path: &str, default: Option<bool>) -> StateSchema {
    let mut decls = BTreeMap::new();
    decls.insert(
        path.to_string(),
        StateDecl {
            ty: Type::Bool,
            default: default.map(lute_manifest::types::Literal::Bool),
            namespace: crate::meta::namespace_of(path).unwrap_or(Namespace::Run),
            owner: None,
        },
    );
    StateSchema {
        decls,
        ..Default::default()
    }
}

fn branch(id: &str, choice_ids: &[&str]) -> Branch {
    use lute_syntax::ast::Choice;
    let choices = choice_ids
        .iter()
        .map(|c| Choice {
            id: (*c).to_string(),
            id_span: span(),
            label: String::new(),
            label_span: span(),
            when: None,
            attrs: Vec::new(),
            body: Vec::new(),
            span: span(),
        })
        .collect();
    Branch {
        id: id.to_string(),
        id_span: span(),
        attrs: Vec::new(),
        choices,
        span: span(),
    }
}

// ---- E-NONEXHAUSTIVE / otherwise ----------------------------------------

#[test]
fn otherwise_makes_infinite_domain_exhaustive() {
    // number subject (infinite) is fine as long as `<otherwise>` is present.
    let mut decls = BTreeMap::new();
    decls.insert(
        "run.n".to_string(),
        StateDecl {
            ty: Type::Int,
            default: None,
            namespace: Namespace::Run,
            owner: None,
        },
    );
    let schema = StateSchema {
        decls,
        ..Default::default()
    };
    let m = match_with(
        "run.n",
        vec![
            when_arm("$ == 1"),
            Arm::Otherwise {
                attrs: Vec::new(),
                body: Vec::new(),
                span: span(),
            },
        ],
    );
    assert!(check_match(&m, &schema, &ctx()).is_empty());
}

#[test]
fn infinite_domain_without_otherwise_is_nonexhaustive() {
    let mut decls = BTreeMap::new();
    decls.insert(
        "run.n".to_string(),
        StateDecl {
            ty: Type::Int,
            default: Some(lute_manifest::types::Literal::Int(0)),
            namespace: Namespace::Run,
            owner: None,
        },
    );
    let schema = StateSchema {
        decls,
        ..Default::default()
    };
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
        vec![
            when_arm("$ == 'fail'"),
            when_arm("$ == 'gold'"),
            when_arm("$ == null"),
        ],
    );
    let errs = check_match(&m, &schema_maybe_unset_subject(), &ctx());
    assert!(
        !errs.iter().any(|e| e.code == "E-UNSET-UNCOVERED"),
        "got {errs:?}"
    );
}

#[test]
fn unset_covered_by_isset_negation_no_error() {
    let m = match_with(
        "run.rank",
        vec![
            when_arm("$ == 'fail'"),
            when_arm("$ == 'gold'"),
            when_arm("!isSet($)"),
        ],
    );
    let errs = check_match(&m, &schema_maybe_unset_subject(), &ctx());
    assert!(
        !errs.iter().any(|e| e.code == "E-UNSET-UNCOVERED"),
        "got {errs:?}"
    );
}

#[test]
fn defaulted_enum_full_coverage_is_not_unset_uncovered() {
    // WITH default => not maybe-unset => no E-UNSET-UNCOVERED even without an
    // unset arm. Also no E-NONEXHAUSTIVE (T4.4-interaction: this match IS
    // domain-exhaustive; T4.4 consumes `is_exhaustive` to drop its false +).
    let m = match_on_enum(&["fail", "gold"], &["fail", "gold"], false);
    let errs = check_match(&m, &schema_enum_subject(), &ctx());
    assert!(
        errs.is_empty(),
        "defaulted full-coverage enum should be clean, got {errs:?}"
    );
}

#[test]
fn defaulted_engine_clock_weekday_is_not_maybe_unset() {
    let mut schema = StateSchema::default();
    schema.decls.insert(
        lute_manifest::clock::CLOCK_WEEKDAY.to_string(),
        StateDecl {
            ty: Type::Int,
            default: Some(lute_manifest::types::Literal::Int(0)),
            namespace: Namespace::Run,
            owner: Some(lute_manifest::types::Owner::Engine),
        },
    );
    let info = infer_domain(Some(lute_manifest::clock::CLOCK_WEEKDAY), &schema);
    assert!(!info.maybe_unset, "defaulted engine clock is assigned: {info:?}");
}

// ---- RC3: foreign reserved quest path domain (dsl 0.2.0 §5.2) ----------

#[test]
fn foreign_quest_state_full_coverage_no_otherwise_is_clean() {
    // `quest.foo` is NOT locally declared/imported (empty schema) — the
    // reserved-shape branch in `infer_domain` must still synthesize the
    // engine's real domain so a `<match>` that fully covers it needs no
    // `<otherwise>`. 0.21.1 T1-1: that domain is the always-assigned
    // lifecycle enum `active|complete|failed|unset` — `unset` is covered by
    // `$ == 'unset'` (or `is="unset"`), no longer by `$ == null`.
    let m = match_with(
        "quest.foo.state",
        vec![
            when_arm("$ == 'active'"),
            when_arm("$ == 'complete'"),
            when_arm("$ == 'failed'"),
            when_arm("$ == 'unset'"),
        ],
    );
    let errs = check_match(&m, &StateSchema::default(), &ctx());
    assert!(
        errs.is_empty(),
        "foreign quest.state fully covered incl. unset should be clean: {errs:?}"
    );
}

#[test]
fn foreign_quest_state_missing_member_is_nonexhaustive() {
    // Same foreign quest, but the `failed` member is missing — still
    // E-NONEXHAUSTIVE, proving the fallback's domain is real (not
    // silently treated as always-covered).
    let m = match_with(
        "quest.foo.state",
        vec![
            when_arm("$ == 'active'"),
            when_arm("$ == 'complete'"),
            when_arm("$ == 'unset'"),
        ],
    );
    let errs = check_match(&m, &StateSchema::default(), &ctx());
    assert!(
        errs.iter().any(|e| e.code == "E-NONEXHAUSTIVE"),
        "missing `failed` member must still be E-NONEXHAUSTIVE: {errs:?}"
    );
}

/// 0.21.1 T1-1: `$ == null` never holds for an always-assigned quest
/// state, so it no longer covers `unset` — a match relying on it is
/// non-exhaustive (it used to be accepted, and the arm never fired).
#[test]
fn quest_state_null_arm_does_not_cover_unset() {
    let m = match_with(
        "quest.foo.state",
        vec![
            when_arm("$ == 'active'"),
            when_arm("$ == 'complete'"),
            when_arm("$ == 'failed'"),
            when_arm("$ == null"),
        ],
    );
    let errs = check_match(&m, &StateSchema::default(), &ctx());
    assert!(errs.iter().any(|e| e.code == "E-NONEXHAUSTIVE"), "{errs:?}");
}

#[test]
fn foreign_quest_objective_done_full_coverage_no_otherwise_is_clean() {
    // `quest.foo.objectives.bar.done` is bool, default false => never
    // maybe-unset; `true|false` fully covers it without `<otherwise>`.
    let m = match_with(
        "quest.foo.objectives.bar.done",
        vec![when_arm("$"), when_arm("!$")],
    );
    let errs = check_match(&m, &StateSchema::default(), &ctx());
    assert!(
        errs.is_empty(),
        "foreign objective.done fully covered should be clean: {errs:?}"
    );
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
            owner: None,
        },
    );
    let schema = StateSchema {
        decls,
        ..Default::default()
    };
    let m = match_with(
        "app.rating",
        vec![when_arm("$ == 'everyone'"), when_arm("$ == 'mature'")],
    );
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
            owner: None,
        },
    );
    let schema = StateSchema {
        decls,
        ..Default::default()
    };
    let m = match_with(
        "app.rating",
        vec![
            when_arm("$ == 'everyone'"),
            when_arm("$ == 'teen'"),
            when_arm("$ == 'mature'"),
        ],
    );
    let errs = check_match(&m, &schema, &ctx());
    assert!(!errs.iter().any(|e| e.code == "E-AGE-GATE"), "got {errs:?}");
}

// ---- W-OVERLAP-ARMS (conservative) --------------------------------------

#[test]
fn duplicate_literal_arms_warn_overlap() {
    let m = match_on_enum(&["fail", "gold"], &["gold", "gold", "fail"], false);
    let warns = check_match(&m, &schema_enum_subject(), &ctx());
    let overlaps: Vec<_> = warns
        .iter()
        .filter(|e| e.code == "W-OVERLAP-ARMS")
        .collect();
    assert_eq!(
        overlaps.len(),
        1,
        "exactly the duplicate `gold` arm warns, got {warns:?}"
    );
    assert_eq!(overlaps[0].severity, Severity::Warning);
}

#[test]
fn distinct_literal_arms_do_not_warn() {
    let m = match_on_enum(&["fail", "gold"], &["fail", "gold"], false);
    let warns = check_match(&m, &schema_enum_subject(), &ctx());
    assert!(
        !warns.iter().any(|e| e.code == "W-OVERLAP-ARMS"),
        "got {warns:?}"
    );
}

// ---- scene.choices.<id> domain ------------------------------------------

#[test]
fn scene_choices_full_coverage_leaves_unset_to_definite_assignment() {
    // domain = {help, ignore} ∪ unset; cover both choice ids but not unset.
    // dsl 0.28.0 (T3-61): whether the pick record can still be unset is
    // path-sensitive (it is set once the branch picked), so the `unset` case
    // is `check_definite_assignment`'s `E-MAYBE-UNSET` (tests/reachability.rs),
    // never a path-blind `E-UNSET-UNCOVERED` here.
    let mut decls = BTreeMap::new();
    decls.insert(
        "scene.choices.couch".to_string(),
        StateDecl {
            ty: Type::Enum(vec!["help".into(), "ignore".into()]),
            default: None,
            namespace: Namespace::Scene,
            owner: None,
        },
    );
    let schema = StateSchema {
        decls,
        ..Default::default()
    };
    let m = match_with(
        "scene.choices.couch",
        vec![when_arm("$ == 'help'"), when_arm("$ == 'ignore'")],
    );
    let errs = check_match(&m, &schema, &ctx());
    assert!(
        !errs.iter().any(|e| e.code == "E-UNSET-UNCOVERED"),
        "the pick record's unset case is not judged here: {errs:?}"
    );
    assert!(
        !errs.iter().any(|e| e.code == "E-NONEXHAUSTIVE"),
        "choice ids fully covered: {errs:?}"
    );
}

// ---- E-DUP-BRANCH + recording (§11.1) -----------------------------------

#[test]
fn branch_records_scene_choices_decl() {
    let mut seen = BTreeSet::new();
    let rec = check_branch(&branch("couch", &["help", "ignore"]), &mut seen);
    assert_eq!(rec.path, "scene.choices.couch");
    assert_eq!(rec.decl.namespace, Namespace::Scene);
    assert_eq!(
        rec.decl.ty,
        Type::Enum(vec!["help".into(), "ignore".into()])
    );
    assert!(rec.decl.default.is_none());
    assert!(rec.diags.is_empty());
}

#[test]
fn duplicate_branch_id_errors_second_time() {
    let mut seen = BTreeSet::new();
    let first = check_branch(&branch("couch", &["help"]), &mut seen);
    assert!(first.diags.is_empty(), "first occurrence is clean");
    let second = check_branch(&branch("couch", &["help"]), &mut seen);
    assert!(
        second.diags.iter().any(|e| e.code == "E-DUP-BRANCH"),
        "got {:?}",
        second.diags
    );
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

// ---- E-CHOICE-DUP (dsl §11.1) -------------------------------------------

#[test]
fn duplicate_choice_ids_flag_e_choice_dup() {
    use lute_syntax::ast::Choice;
    let sp = Span {
        byte_start: 0,
        byte_end: 0,
        line: 1,
        column: 1,
        utf16_range: (0, 0),
    };
    let choice = |id: &str| Choice {
        id: id.into(),
        id_span: sp,
        label: id.into(),
        label_span: sp,
        when: None,
        attrs: Vec::new(),
        body: Vec::new(),
        span: sp,
    };
    let branch = Branch {
        id: "number".into(),
        id_span: sp,
        attrs: Vec::new(),
        choices: vec![choice("blunt"), choice("soft"), choice("blunt")],
        span: sp,
    };
    let mut seen = BTreeSet::new();
    let rec = check_branch(&branch, &mut seen);
    let dups: Vec<_> = rec
        .diags
        .iter()
        .filter(|d| d.code == "E-CHOICE-DUP")
        .collect();
    assert_eq!(
        dups.len(),
        1,
        "exactly one E-CHOICE-DUP for the one repeat id"
    );
    assert_eq!(dups[0].severity, Severity::Error);
    assert!(dups[0].message.contains("blunt"), "{}", dups[0].message);

    // Unique ids stay clean.
    let ok = Branch {
        id: "other".into(),
        id_span: sp,
        attrs: Vec::new(),
        choices: vec![choice("a"), choice("b")],
        span: sp,
    };
    let rec = check_branch(&ok, &mut seen);
    assert!(rec.diags.iter().all(|d| d.code != "E-CHOICE-DUP"));
}

// ---- E-BRANCH-EMPTY (dsl §7.3 `Choice+`) --------------------------------

#[test]
fn empty_branch_flags_e_branch_empty() {
    let mut seen = BTreeSet::new();
    let rec = check_branch(&branch("dead", &[]), &mut seen);
    let empties: Vec<_> = rec
        .diags
        .iter()
        .filter(|d| d.code == "E-BRANCH-EMPTY")
        .collect();
    assert_eq!(empties.len(), 1, "one E-BRANCH-EMPTY, got {:?}", rec.diags);
    assert_eq!(empties[0].severity, Severity::Error);
    assert_eq!(empties[0].layer, Layer::Logic);
    assert!(
        empties[0].message.contains("dead"),
        "{}",
        empties[0].message
    );
}

#[test]
fn well_formed_branch_has_no_empty_diag() {
    let mut seen = BTreeSet::new();
    let rec = check_branch(&branch("couch", &["help", "ignore"]), &mut seen);
    assert!(rec.diags.iter().all(|d| d.code != "E-BRANCH-EMPTY"));
}

// ---- E-BRANCH-ALL-GUARDED (dsl §11.1, S5) -------------------------------

/// Build a branch whose choices each carry an optional `when` guard (raw CEL
/// text; `None` = unguarded). Only `when.is_some()` matters to `check_branch`.
fn guarded_branch(id: &str, choices: &[(&str, Option<&str>)]) -> Branch {
    use lute_syntax::ast::Choice;
    let choices = choices
        .iter()
        .map(|(cid, guard)| Choice {
            id: (*cid).to_string(),
            id_span: span(),
            label: String::new(),
            label_span: span(),
            when: guard.map(|g| CelSlot::raw(CelKind::Condition, g.into(), span())),
            attrs: Vec::new(),
            body: Vec::new(),
            span: span(),
        })
        .collect();
    Branch {
        id: id.to_string(),
        id_span: span(),
        attrs: Vec::new(),
        choices,
        span: span(),
    }
}

#[test]
fn branch_all_guarded_rejected() {
    // Every `<choice>` carries a `when` → the eligible set can be empty, so
    // the branch could present an empty menu: one E-BRANCH-ALL-GUARDED at the
    // branch span.
    let mut seen = BTreeSet::new();
    let b = guarded_branch(
        "approach",
        &[("soft", Some("scene.x")), ("blunt", Some("scene.y"))],
    );
    let rec = check_branch(&b, &mut seen);
    let guarded: Vec<_> = rec
        .diags
        .iter()
        .filter(|d| d.code == E_BRANCH_ALL_GUARDED)
        .collect();
    assert_eq!(
        guarded.len(),
        1,
        "one E-BRANCH-ALL-GUARDED, got {:?}",
        rec.diags
    );
    assert_eq!(guarded[0].severity, Severity::Error);
    assert!(
        guarded[0].message.contains("approach"),
        "{}",
        guarded[0].message
    );
}

#[test]
fn branch_one_unguarded_ok() {
    // At least one `when`-less choice → the menu is never provably empty.
    let mut seen = BTreeSet::new();
    let b = guarded_branch("approach", &[("soft", Some("scene.x")), ("blunt", None)]);
    let rec = check_branch(&b, &mut seen);
    assert!(
        rec.diags.iter().all(|d| d.code != E_BRANCH_ALL_GUARDED),
        "got {:?}",
        rec.diags
    );
}

#[test]
fn empty_branch_is_not_all_guarded() {
    // An empty branch is E-BRANCH-EMPTY, NOT also E-BRANCH-ALL-GUARDED
    // (all-guarded applies only to a non-empty branch).
    let mut seen = BTreeSet::new();
    let rec = check_branch(&branch("dead", &[]), &mut seen);
    assert!(
        rec.diags.iter().any(|d| d.code == "E-BRANCH-EMPTY"),
        "got {:?}",
        rec.diags
    );
    assert!(
        rec.diags.iter().all(|d| d.code != E_BRANCH_ALL_GUARDED),
        "empty branch must not be double-flagged; got {:?}",
        rec.diags
    );
}

// ---- E-MATCH-DUP-OTHERWISE (dsl §11.2 at-most-one) ----------------------

#[test]
fn two_otherwise_flag_e_match_dup_otherwise() {
    let second_sp = Span {
        byte_start: 42,
        byte_end: 50,
        line: 3,
        column: 1,
        utf16_range: (42, 50),
    };
    let m = match_with(
        "run.rank",
        vec![
            Arm::Otherwise {
                attrs: Vec::new(),
                body: Vec::new(),
                span: span(),
            },
            Arm::Otherwise {
                attrs: Vec::new(),
                body: Vec::new(),
                span: second_sp,
            },
        ],
    );
    let errs = check_match(&m, &schema_enum_subject(), &ctx());
    let dups: Vec<_> = errs
        .iter()
        .filter(|d| d.code == "E-MATCH-DUP-OTHERWISE")
        .collect();
    assert_eq!(
        dups.len(),
        1,
        "one dup for the second otherwise, got {errs:?}"
    );
    assert_eq!(dups[0].severity, Severity::Error);
    assert_eq!(dups[0].layer, Layer::Logic);
    assert_eq!(dups[0].span, second_sp, "flagged at the second otherwise");
}

#[test]
fn single_otherwise_has_no_dup_diag() {
    let m = match_with(
        "run.rank",
        vec![
            when_arm("$ == 'gold'"),
            Arm::Otherwise {
                attrs: Vec::new(),
                body: Vec::new(),
                span: span(),
            },
        ],
    );
    let errs = check_match(&m, &schema_enum_subject(), &ctx());
    assert!(errs.iter().all(|d| d.code != "E-MATCH-DUP-OTHERWISE"));
}

// ---- E-DUP-LINE-CODE (dsl §12, unique (speaker, code)) ------------------

fn code_line(speaker: &str, code: Option<&str>, byte: usize) -> Node {
    use lute_syntax::ast::{Attr, Line};
    let sp = Span {
        byte_start: byte,
        byte_end: byte,
        line: 1,
        column: 1,
        utf16_range: (byte as u32, byte as u32),
    };
    let attrs = match code {
        Some(c) => vec![Attr {
            key: "code".into(),
            value: AttrValue::Str(c.into()),
            value_span: sp,
            span: sp,
        }],
        None => Vec::new(),
    };
    Node::Line(Line {
        speaker: speaker.into(),
        attrs,
        when: None,
        text: "…".into(),
        text_span: sp,
        interps: Vec::new(),
        span: sp,
    })
}

fn doc_with(body: Vec<Node>) -> Document {
    use lute_syntax::ast::{Meta, Shot};
    Document {
        meta: Meta {
            raw_yaml: String::new(),
            span: span(),
        },
        title: None,
        shots: vec![Shot {
            heading: "Shot 1".into(),
            body,
            span: span(),
        }],
        quests: Vec::new(),
        entries: Vec::new(),
        beats: Vec::new(),
        span: span(),
    }
}

#[test]
fn duplicate_line_code_flags_at_second_occurrence() {
    // (marina, 0050) twice => one E-DUP-LINE-CODE at the SECOND occurrence.
    // The (fixer, 0050) line is a different speaker (distinct lineId) and the
    // (marina, 0060) line a distinct code — both stay clean.
    let doc = doc_with(vec![
        code_line("marina", Some("0050"), 10),
        code_line("fixer", Some("0050"), 20),
        code_line("marina", Some("0050"), 30),
        code_line("marina", Some("0060"), 40),
    ]);
    let diags = check_line_codes(&doc);
    assert_eq!(
        diags.len(),
        1,
        "exactly one E-DUP-LINE-CODE for the one repeat pair, got {diags:?}"
    );
    assert_eq!(diags[0].code, "E-DUP-LINE-CODE");
    assert_eq!(diags[0].severity, Severity::Error);
    assert_eq!(diags[0].layer, Layer::Logic);
    assert_eq!(
        diags[0].span.byte_start, 30,
        "flagged at the second (marina, 0050)"
    );
    assert!(diags[0].message.contains("0050"), "{}", diags[0].message);
    assert!(diags[0].message.contains("marina"), "{}", diags[0].message);
}

#[test]
fn distinct_codes_and_speakers_have_no_dup_line_code() {
    let doc = doc_with(vec![
        code_line("marina", Some("0050"), 10),
        code_line("fixer", Some("0050"), 20),
        code_line("marina", Some("0060"), 30),
        code_line("marina", None, 40), // untagged: no static collision
    ]);
    assert!(check_line_codes(&doc).is_empty());
}

#[test]
fn line_code_collision_is_trimmed_and_descends_into_arms() {
    use lute_syntax::ast::Choice;
    // ` 0050 ` and `0050` trim to the same key => collide (the addressing
    // pass keys `lineId`/`voiceKey` on the trimmed string). The colliding
    // occurrence sits inside a `<branch>` choice body, proving the walk
    // descends into nested bodies (mirroring tag.rs).
    let branch = Branch {
        id: "b".into(),
        id_span: span(),
        attrs: Vec::new(),
        choices: vec![Choice {
            id: "a".into(),
            id_span: span(),
            label: String::new(),
            label_span: span(),
            when: None,
            attrs: Vec::new(),
            body: vec![code_line("marina", Some("0050"), 60)],
            span: span(),
        }],
        span: span(),
    };
    let doc = doc_with(vec![
        code_line("marina", Some(" 0050 "), 10),
        Node::Branch(branch),
    ]);
    let diags = check_line_codes(&doc);
    assert_eq!(diags.len(), 1, "trimmed codes collide, got {diags:?}");
    assert_eq!(diags[0].code, "E-DUP-LINE-CODE");
    assert_eq!(
        diags[0].span.byte_start, 60,
        "flagged at the nested second occurrence"
    );
}

/// dsl 0.23.0 §4: each lore `<beat>` bundle is its own identity scope —
/// a pair repeated across an entry and two beats is clean, a pair repeated
/// inside one beat (even nested in a branch choice) is flagged.
#[test]
fn bundle_beat_line_codes_are_scoped_per_beat() {
    let src = "---\nid: ship.records\nkind: lore\n---\n\
               <entry id=\"e\">\n@n{code=\"0010\"}: a\n</entry>\n\
               <beat id=\"b1\" on=\"talk\">\n@n{code=\"0010\"}: b\n</beat>\n\
               <beat id=\"b2\" on=\"talk\">\n@n{code=\"0010\"}: c\n\
               <branch id=\"k\">\n<choice id=\"x\" label=\"X\">\n@n{code=\"0010\"}: d\n</choice>\n</branch>\n\
               </beat>\n";
    let (doc, _) = lute_syntax::parse(src);
    let diags = check_line_codes(&doc);
    assert_eq!(diags.len(), 1, "{diags:?}");
    assert_eq!(
        diags[0].span.byte_start,
        src.find("@n{code=\"0010\"}: d").unwrap()
    );
}
// ---- B4: <when is> literal-pattern coverage + E-WHEN-PATTERN (§7.3.1) ----

/// Build a `<when>` arm from an optional `is` pattern + a `test` guard raw
/// (empty `test` = absent, mirroring the parser's empty-slot default).
fn when_is(is_raw: Option<&str>, test_raw: &str) -> Arm {
    Arm::When {
        attrs: Vec::new(),
        is: is_raw.map(|r| IsPattern {
            raw: r.trim().to_string(),
            span: span(),
        }),
        test: CelSlot {
            kind: CelKind::Condition,
            raw: test_raw.into(),
            ast: None,
            span: span(),
            id: StableId(0),
            authored: None,
        },
        body: Vec::new(),
        span: span(),
    }
}

/// `run.rank` as the 4-member enum `fail|bronze|silver|gold`. With a default
/// it is finite-and-never-unset; without, finite-but-maybe-unset.
fn schema_rank4(with_default: bool) -> StateSchema {
    let mut decls = BTreeMap::new();
    decls.insert(
        "run.rank".to_string(),
        StateDecl {
            ty: Type::Enum(vec![
                "fail".into(),
                "bronze".into(),
                "silver".into(),
                "gold".into(),
            ]),
            default: with_default.then(|| lute_manifest::types::Literal::Str("fail".into())),
            namespace: Namespace::Run,
            owner: None,
        },
    );
    StateSchema {
        decls,
        ..Default::default()
    }
}

#[test]
fn is_arms_cover_enum_no_otherwise_ok() {
    // `is="fail | bronze"`, `is="silver"`, `is="gold"` covers the enum with NO
    // <otherwise> => exhaustive (`is` is the NORMATIVE coverage path, §11.2).
    let m = match_with(
        "run.rank",
        vec![
            when_is(Some("fail | bronze"), ""),
            when_is(Some("silver"), ""),
            when_is(Some("gold"), ""),
        ],
    );
    let errs = check_match(&m, &schema_rank4(true), &ctx());
    assert!(errs.is_empty(), "is arms fully cover the enum: {errs:?}");
}

#[test]
fn is_arms_missing_member_nonexhaustive() {
    // omit `gold`, no <otherwise> => E-NONEXHAUSTIVE.
    let m = match_with(
        "run.rank",
        vec![
            when_is(Some("fail | bronze"), ""),
            when_is(Some("silver"), ""),
        ],
    );
    let errs = check_match(&m, &schema_rank4(true), &ctx());
    assert!(
        errs.iter().any(|e| e.code == "E-NONEXHAUSTIVE"),
        "missing `gold` arm: {errs:?}"
    );
}

#[test]
fn is_unset_covers_unset() {
    // maybe-unset subject: `is="unset"` covers the unset member (§9.4/§11.2).
    let m = match_with(
        "run.rank",
        vec![
            when_is(Some("fail|bronze"), ""),
            when_is(Some("silver|gold"), ""),
            when_is(Some("unset"), ""),
        ],
    );
    let errs = check_match(&m, &schema_rank4(false), &ctx());
    assert!(
        !errs.iter().any(|e| e.code == "E-UNSET-UNCOVERED"),
        "`is=unset` covers unset: {errs:?}"
    );
    assert!(
        !errs.iter().any(|e| e.code == "E-NONEXHAUSTIVE"),
        "members fully covered: {errs:?}"
    );
}

#[test]
fn when_with_neither_is_nor_test_is_e_when_pattern() {
    // a `<when>` with neither `is` nor `test` => E-WHEN-PATTERN (§7.3.1, D-D).
    let m = match_with(
        "run.rank",
        vec![
            when_is(None, ""),
            Arm::Otherwise {
                attrs: Vec::new(),
                body: Vec::new(),
                span: span(),
            },
        ],
    );
    let errs = check_match(&m, &schema_rank4(true), &ctx());
    assert!(
        errs.iter().any(|e| e.code == E_WHEN_PATTERN),
        "empty <when> must be E-WHEN-PATTERN: {errs:?}"
    );
}

#[test]
fn is_and_test_both_ok_is_drives_coverage() {
    // `is="gold" test="$ != 'x'"` parses+checks; `is` drives coverage so the
    // match stays exhaustive despite the extra guard.
    let m = match_with(
        "run.rank",
        vec![
            when_is(Some("fail|bronze"), ""),
            when_is(Some("silver"), ""),
            when_is(Some("gold"), "$ != 'x'"),
        ],
    );
    let errs = check_match(&m, &schema_rank4(true), &ctx());
    assert!(
        !errs.iter().any(|e| e.code == "E-NONEXHAUSTIVE"),
        "is drives coverage even with a guard: {errs:?}"
    );
    assert!(
        !errs.iter().any(|e| e.code == E_WHEN_PATTERN),
        "an arm carrying `is` is never E-WHEN-PATTERN: {errs:?}"
    );
}

#[test]
fn is_exhaustive_consults_is_coverage() {
    // is_exhaustive (shared with defassign) MUST see `is` coverage too.
    let full = match_with(
        "run.rank",
        vec![
            when_is(Some("fail|bronze"), ""),
            when_is(Some("silver|gold"), ""),
        ],
    );
    assert!(is_exhaustive(&full, &schema_rank4(true)));
    let partial = match_with(
        "run.rank",
        vec![
            when_is(Some("fail|bronze"), ""),
            when_is(Some("silver"), ""),
        ],
    );
    assert!(!is_exhaustive(&partial, &schema_rank4(true)));
}

#[test]
fn is_true_false_covers_bool() {
    // bool subject covered by `is="true"` + `is="false"`, no <otherwise>.
    let m = match_with(
        "scene.sealed",
        vec![when_is(Some("true"), ""), when_is(Some("false"), "")],
    );
    let errs = check_match(&m, &schema_bool("scene.sealed", Some(false)), &ctx());
    assert!(errs.is_empty(), "bool covered by is=true/false: {errs:?}");
}

// --- CheckFix F6/F7: `<quest id>`/`<objective id>` required (§6.3/§6.4) ---

fn objective(id: &str, done_raw: &str) -> lute_syntax::ast::Objective {
    lute_syntax::ast::Objective {
        id: id.to_string(),
        id_span: span(),
        done: CelSlot::raw(CelKind::Condition, done_raw.to_string(), span()),
        quest: None,
        quest_span: span(),
        visible_when: None,
        title: None,
        optional: false,
        on: None,
        by: None,
        target: None,
        until: None,
        attrs: Vec::new(),
        body: Vec::new(),
        rewards: Vec::new(),
        span: span(),
    }
}

fn quest_with_body(id: &str, body: Vec<Node>) -> Quest {
    Quest {
        id: id.to_string(),
        id_span: span(),
        title: None,
        start: None,
        fail: None,
        follows: None,
        follows_span: span(),
        tier: None,
        activate: None,
        complete: None,
        accept: None,
        rearm: None,
        attrs: Vec::new(),
        body,
        rewards: Vec::new(),
        span: span(),
    }
}

#[test]
fn quest_missing_id_skips_reserved_fold() {
    let q = quest_with_body("", vec![Node::Objective(objective("o", "a"))]);
    let mut seen = BTreeSet::new();
    let rec = check_quest(&q, &mut seen);
    assert!(
        rec.diags.iter().any(|d| d.code == "E-QUEST-ID-MISSING"),
        "{:?}",
        rec.diags
    );
    assert!(
        rec.decls.is_empty(),
        "a quest with no id must fold NO reserved decls (both its own state \
         and every objective's done): {:?}",
        rec.decls
    );
}

#[test]
fn objective_missing_id_skips_only_its_own_decl() {
    let q = quest_with_body(
        "q",
        vec![
            Node::Objective(objective("", "a")),
            Node::Objective(objective("o2", "b")),
        ],
    );
    let mut seen = BTreeSet::new();
    let rec = check_quest(&q, &mut seen);
    assert!(
        rec.diags.iter().any(|d| d.code == "E-OBJECTIVE-ID-MISSING"),
        "{:?}",
        rec.diags
    );
    let paths: Vec<&str> = rec.decls.iter().map(|(p, _)| p.as_str()).collect();
    assert!(
        !paths.iter().any(|p| p.contains("..")),
        "no malformed (doubled-dot) reserved path: {paths:?}"
    );
    assert_eq!(
        paths,
        vec![
            "quest.q.state",
            "quest.q.activatedAt",
            "quest.q.objectives.o2.done"
        ],
        "the quest's own state + activatedAt decls and the well-formed objective's \
         done decl still fold; only the id-less objective's decl is skipped"
    );
}

#[test]
fn two_quests_missing_id_are_not_flagged_as_duplicates() {
    // Two DIFFERENT quests with no `id` must not collide on the shared
    // empty-string key in `seen_quests` (that would wrongly fire
    // E-QUEST-ID-DUP instead of two independent E-QUEST-ID-MISSING).
    let mut seen = BTreeSet::new();
    let a = quest_with_body("", vec![]);
    let b = quest_with_body("", vec![]);
    let rec_a = check_quest(&a, &mut seen);
    let rec_b = check_quest(&b, &mut seen);
    assert!(!rec_a.diags.iter().any(|d| d.code == "E-QUEST-ID-DUP"));
    assert!(!rec_b.diags.iter().any(|d| d.code == "E-QUEST-ID-DUP"));
    assert!(rec_a.diags.iter().any(|d| d.code == "E-QUEST-ID-MISSING"));
    assert!(rec_b.diags.iter().any(|d| d.code == "E-QUEST-ID-MISSING"));
}

// --- Subquest doc-level checks (subquest design 2026-08-31 §1/§4) ---

/// Helper: an objective with a `quest=` reference and an optional
/// `done=` raw. Mirrors [`objective`] but sets `quest` / `quest_span`
/// (the parser fallback: the objective's open-tag span, span() here).
fn subquest_objective(id: &str, quest: &str, done_raw: &str) -> lute_syntax::ast::Objective {
    lute_syntax::ast::Objective {
        id: id.to_string(),
        id_span: span(),
        done: CelSlot::raw(CelKind::Condition, done_raw.to_string(), span()),
        quest: Some(quest.to_string()),
        quest_span: span(),
        visible_when: None,
        title: None,
        optional: false,
        on: None,
        by: None,
        target: None,
        until: None,
        attrs: Vec::new(),
        body: Vec::new(),
        rewards: Vec::new(),
        span: span(),
    }
}

#[test]
fn quest_and_done_together_error_objective_quest_done() {
    // §1: `quest=` + non-empty `done=` on one objective is mutually
    // exclusive — E-OBJECTIVE-QUEST-DONE fires. `E-OBJECTIVE-MISSING-DONE`
    // must NOT fire (the `done` is present); `E-QUEST-TREE-CYCLE` must
    // NOT fire (the child is not the enclosing quest).
    let q = quest_with_body(
        "parent",
        vec![Node::Objective(subquest_objective(
            "o",
            "child",
            "run.spokeRath",
        ))],
    );
    let mut seen = BTreeSet::new();
    let rec = check_quest(&q, &mut seen);
    assert!(
        rec.diags.iter().any(|d| d.code == E_OBJECTIVE_QUEST_DONE),
        "expected E-OBJECTIVE-QUEST-DONE: {:?}",
        rec.diags
    );
    assert!(
        !rec.diags
            .iter()
            .any(|d| d.code == "E-OBJECTIVE-MISSING-DONE"),
        "MISSING-DONE must not co-fire when `done=` is populated: {:?}",
        rec.diags
    );
    assert!(
        !rec.diags.iter().any(|d| d.code == E_QUEST_TREE_CYCLE),
        "child != enclosing id, cycle must not fire: {:?}",
        rec.diags
    );
}

#[test]
fn quest_alone_suppresses_missing_done() {
    // §1: an empty `done=` alongside `quest=` is the CANONICAL subquest
    // shape — the completion predicate is synthesised downstream
    // (`quest.<child>.state == 'complete'`, spec §2.1). No
    // `E-OBJECTIVE-MISSING-DONE`, no `E-OBJECTIVE-QUEST-DONE` (the two
    // are not co-present), no cycle.
    let q = quest_with_body(
        "parent",
        vec![Node::Objective(subquest_objective("o", "child", ""))],
    );
    let mut seen = BTreeSet::new();
    let rec = check_quest(&q, &mut seen);
    assert!(
        !rec.diags
            .iter()
            .any(|d| d.code == "E-OBJECTIVE-MISSING-DONE"),
        "MISSING-DONE must be suppressed when `quest=` delegates completion: {:?}",
        rec.diags
    );
    assert!(
        !rec.diags.iter().any(|d| d.code == E_OBJECTIVE_QUEST_DONE),
        "QUEST-DONE fires only when both coexist: {:?}",
        rec.diags
    );
    assert!(
        !rec.diags.iter().any(|d| d.code == E_QUEST_TREE_CYCLE),
        "child != enclosing id, cycle must not fire: {:?}",
        rec.diags
    );
    // Reserved-decl fold is UNCHANGED for subquest objectives (Task B
    // constraint): the `quest.parent.objectives.o.done` decl still
    // participates in the schema so downstream reads of the folded
    // done bit stay declared (its synthesised true-value comes at
    // compile time).
    let paths: Vec<&str> = rec.decls.iter().map(|(p, _)| p.as_str()).collect();
    assert!(
        paths.contains(&"quest.parent.objectives.o.done"),
        "subquest objective's reserved `done` decl must still fold: {paths:?}"
    );
}

#[test]
fn neither_quest_nor_done_keeps_missing_done() {
    // §1: with NO `quest=` reference AND an empty `done=` the classic
    // missing-`done` diagnostic still fires — nothing about the
    // subquest path suppresses the plain-objective obligation.
    let q = quest_with_body("parent", vec![Node::Objective(objective("o", ""))]);
    let mut seen = BTreeSet::new();
    let rec = check_quest(&q, &mut seen);
    assert!(
        rec.diags
            .iter()
            .any(|d| d.code == "E-OBJECTIVE-MISSING-DONE"),
        "plain objective with empty done must still fire MISSING-DONE: {:?}",
        rec.diags
    );
    assert!(
        !rec.diags.iter().any(|d| d.code == E_OBJECTIVE_QUEST_DONE),
        "QUEST-DONE requires `quest=` to fire: {:?}",
        rec.diags
    );
}

#[test]
fn self_referential_quest_ref_is_length_1_cycle() {
    // §4 (tree, not DAG): `<objective quest="self">` inside
    // `<quest id="self">` names its own enclosing quest — a length-1
    // cycle, caught early at the doc level (deeper cycles are the
    // project pass's job). Empty `done=` here so we do not also trip
    // E-OBJECTIVE-QUEST-DONE.
    let q = quest_with_body(
        "self",
        vec![Node::Objective(subquest_objective("o", "self", ""))],
    );
    let mut seen = BTreeSet::new();
    let rec = check_quest(&q, &mut seen);
    assert!(
        rec.diags.iter().any(|d| d.code == E_QUEST_TREE_CYCLE),
        "expected E-QUEST-TREE-CYCLE for a self-reference: {:?}",
        rec.diags
    );
    assert!(
        !rec.diags.iter().any(|d| d.code == E_OBJECTIVE_QUEST_DONE),
        "empty `done=` here — QUEST-DONE must not co-fire: {:?}",
        rec.diags
    );
    assert!(
        !rec.diags
            .iter()
            .any(|d| d.code == "E-OBJECTIVE-MISSING-DONE"),
        "the `quest=` reference itself satisfies the completion obligation; \
         MISSING-DONE must not fire on a self-reference: {:?}",
        rec.diags
    );
}

#[test]
fn unknown_quest_ref_is_silent_at_doc_level() {
    // §4: an unknown child id (not the enclosing quest, and not
    // otherwise defined in this doc) stays silent at the doc level —
    // it may resolve in another artifact; `check_project` owns
    // cross-doc `E-QUEST-REF-UNKNOWN`. `check_quest` sees one quest
    // in isolation, so we only assert the subquest-owned codes stay
    // quiet here.
    let q = quest_with_body(
        "parent",
        vec![Node::Objective(subquest_objective("o", "elsewhere", ""))],
    );
    let mut seen = BTreeSet::new();
    let rec = check_quest(&q, &mut seen);
    for code in [
        E_OBJECTIVE_QUEST_DONE,
        E_QUEST_TREE_CYCLE,
        "E-OBJECTIVE-MISSING-DONE",
    ] {
        assert!(
            !rec.diags.iter().any(|d| d.code == code),
            "{code} must not fire on a bare same-doc subquest reference: {:?}",
            rec.diags
        );
    }
}

// ---- number domain intervals (dsl 0.18.0 §4) ---------------------------

fn iv(lit: &str) -> Interval {
    Interval::of(&classify_is_literal(lit).expect("valid literal")).expect("numeric literal")
}

fn num_cov(lits: &[&str]) -> NumCoverage {
    let mut c = NumCoverage::default();
    for l in lits {
        c.add(iv(l));
    }
    c
}

#[test]
fn num_coverage_touching_ends_cover_the_line() {
    // Inclusive ends: `..0` and `0..` share the point 0 and fuse.
    assert!(num_cov(&["..0", "0.."]).covers_all());
    assert_eq!(num_cov(&["0..", "..0"]).first_gap(), None);
}

#[test]
fn num_coverage_names_the_first_gap() {
    // Reals are dense: `..0 | 1..` leaves (0, 1) — and a point can't fill it.
    assert_eq!(
        num_cov(&["..0", "1..", "0.5"]).first_gap().as_deref(),
        Some("numbers strictly between 0 and 0.5 are not covered")
    );
    assert_eq!(
        num_cov(&["2..", "-1.5..0"]).first_gap().as_deref(),
        Some("numbers below -1.5 are not covered")
    );
    assert_eq!(
        num_cov(&["..2", "3"]).first_gap().as_deref(),
        Some("numbers strictly between 2 and 3 are not covered")
    );
    assert_eq!(
        num_cov(&["..2.0"]).first_gap().as_deref(),
        Some("numbers above 2 are not covered")
    );
    assert_eq!(
        NumCoverage::default().first_gap().as_deref(),
        Some("no number is covered")
    );
}

#[test]
fn num_coverage_contains_and_overlaps() {
    let c = num_cov(&["1..5"]);
    assert!(c.contains(iv("2..4")) && c.contains(iv("5")) && c.contains(iv("1.0")));
    assert!(!c.contains(iv("4..6")) && !c.contains(iv("5.5")));
    // A range never overlaps (cascade idiom; containment is E-ARM-DEAD's);
    // a point inside coverage (even at an end) does.
    assert!(!c.overlaps(iv("5..")));
    assert!(!c.overlaps(iv("3..8")));
    assert!(!c.overlaps(iv("2..4")));
    assert!(c.overlaps(iv("5")));
    assert!(c.overlaps(iv("5..5")));
    assert!(!c.overlaps(iv("6")));
}

#[test]
fn is_exhaustive_agrees_with_number_coverage() {
    // `is_exhaustive` (definite assignment) must match E-NONEXHAUSTIVE on
    // a number subject: the whole line (+ `unset` when maybe-unset).
    let schema = |default: Option<i64>| {
        let mut decls = BTreeMap::new();
        decls.insert(
            "run.n".to_string(),
            StateDecl {
                ty: Type::Int,
                default: default.map(Literal::Int),
                namespace: Namespace::Run,
                owner: None,
            },
        );
        StateSchema {
            decls,
            ..Default::default()
        }
    };
    let split = match_with(
        "run.n",
        vec![when_is(Some("..0"), ""), when_is(Some("0.."), "")],
    );
    let gap = match_with(
        "run.n",
        vec![when_is(Some("..0"), ""), when_is(Some("1.."), "")],
    );
    let with_unset = match_with(
        "run.n",
        vec![when_is(Some("..0 | unset"), ""), when_is(Some("0.."), "")],
    );
    assert!(is_exhaustive(&split, &schema(Some(0))));
    assert!(!is_exhaustive(&gap, &schema(Some(0))));
    assert!(!is_exhaustive(&split, &schema(None)));
    assert!(is_exhaustive(&with_unset, &schema(None)));
    for (m, s) in [(&split, schema(Some(0))), (&with_unset, schema(None))] {
        assert!(check_match(m, &s, &ctx()).is_empty());
    }
}
