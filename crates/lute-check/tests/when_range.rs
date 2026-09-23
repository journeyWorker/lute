//! dsl 0.18.0 §2/§4: numeric range literals in `<when is="…">` and the
//! `number` subject domain — `E-WHEN-RANGE`, the range arm of
//! `E-WHEN-LITERAL-DOMAIN`, interval coverage for `E-NONEXHAUSTIVE` /
//! `W-OVERLAP-ARMS` / `E-ARM-DEAD` / `W-OTHERWISE-DEAD`. Driven through the
//! assembled `check()` over inline `state:` frontmatter, mirroring
//! `tests/reachability.rs`'s `run()`/`codes()` harness.
use lute_check::{check, CheckInput, CheckResult, Mode, SchemaImports};
use lute_core_span::Diagnostic;
use lute_manifest::provider::ProviderSet;

fn run(text: &str) -> CheckResult {
    let input = CheckInput {
        text: text.to_string(),
        uri: "when_range".into(),
        snapshot: lute_manifest::core::load_core_snapshot(),
        providers: ProviderSet::default(),
        mode: Mode::Author,
        imports: SchemaImports::default(),
        components: Default::default(),
        defaults: Default::default(),
    };
    check(&input)
}

fn diags(text: &str) -> Vec<Diagnostic> {
    run(text).diagnostics
}

fn codes(text: &str) -> Vec<String> {
    diags(text).into_iter().map(|d| d.code).collect()
}

fn count(out: &[String], code: &str) -> usize {
    out.iter().filter(|c| c.as_str() == code).count()
}

// `run.n` (number, defaulted — never unset), `run.unbound` (number, NO
// default — maybe unset), `run.rank` (enum), `run.flag` (bool), `run.name`
// (string).
const HDR: &str = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  \
    run.n: { type: number, default: 0 }\n  \
    run.unbound: { type: number }\n  \
    run.rank: { type: { enum: [fail, bronze, silver, gold] }, default: fail }\n  \
    run.flag: { type: bool, default: false }\n  \
    run.name: { type: string, default: x }\n---\n## Shot 1.\n";

/// A `<match on=subject>` with one `<when is=…>` arm per pattern, plus an
/// `<otherwise>` when asked.
fn match_src(subject: &str, arms: &[&str], otherwise: bool) -> String {
    let mut s = format!("{HDR}<match on=\"{subject}\">\n");
    for (i, pat) in arms.iter().enumerate() {
        s.push_str(&format!("<when is=\"{pat}\">\n@narrator: a{i}\n</when>\n"));
    }
    if otherwise {
        s.push_str("<otherwise>\n@narrator: o\n</otherwise>\n");
    }
    s.push_str("</match>\n");
    s
}

// §4: `..0` then `0..` is the idiomatic boundary split — inclusive ends
// meet at 0, so the line is covered without `<otherwise>`, and sharing the
// endpoint is not an overlap (first match wins at 0).
#[test]
fn boundary_split_is_exhaustive_and_clean() {
    let out = diags(&match_src("run.n", &["..0", "0.."], false));
    assert!(out.is_empty(), "expected zero diagnostics: {out:?}");
}

// The same split inside ONE alternation is exhaustive too.
#[test]
fn boundary_split_in_one_alternation_is_exhaustive() {
    let out = diags(&match_src("run.n", &["..0 | 0.."], false));
    assert!(out.is_empty(), "expected zero diagnostics: {out:?}");
}

// D-A: numbers are real — `..0 | 1..` leaves (0, 1) uncovered, and the
// message names that gap.
#[test]
fn gap_between_ranges_is_nonexhaustive_with_gap_message() {
    let out = diags(&match_src("run.n", &["..0 | 1.."], false));
    let d = out
        .iter()
        .find(|d| d.code == "E-NONEXHAUSTIVE")
        .unwrap_or_else(|| panic!("expected E-NONEXHAUSTIVE: {out:?}"));
    assert_eq!(
        d.message,
        "non-exhaustive `<match>`: numbers strictly between 0 and 1 are not covered and there \
         is no `<otherwise>` (dsl 0.18.0 §4)"
    );
}

// A point can never fill an open gap between two closed intervals, and a
// lone half-line leaves the other side uncovered.
#[test]
fn points_do_not_close_open_gaps() {
    let out = diags(&match_src("run.n", &["..0", "0.5", "1.."], false));
    let d = out
        .iter()
        .find(|d| d.code == "E-NONEXHAUSTIVE")
        .unwrap_or_else(|| panic!("expected E-NONEXHAUSTIVE: {out:?}"));
    assert!(
        d.message
            .contains("numbers strictly between 0 and 0.5 are not covered"),
        "{}",
        d.message
    );
    let out = diags(&match_src("run.n", &["2.."], false));
    let d = out.iter().find(|d| d.code == "E-NONEXHAUSTIVE").unwrap();
    assert!(
        d.message.contains("numbers below 2 are not covered"),
        "{}",
        d.message
    );
}

// Point literals keep 0.4.0 behavior on a number subject: `<otherwise>`
// still required, and no domain complaint.
#[test]
fn point_literals_alone_stay_nonexhaustive() {
    let out = codes(&match_src("run.n", &["1", "2"], false));
    assert_eq!(count(&out, "E-NONEXHAUSTIVE"), 1, "{out:?}");
    assert_eq!(count(&out, "E-WHEN-LITERAL-DOMAIN"), 0, "{out:?}");
}

// §4 W-OVERLAP-ARMS: a range NEVER warns — partial overlap is the
// descending-threshold cascade (first match wins), and full containment is
// E-ARM-DEAD's.
#[test]
fn partially_overlapping_ranges_do_not_warn() {
    for arms in [["1..5", "3..8"], ["1..5", "5..8"]] {
        let out = codes(&match_src("run.n", &arms, true));
        assert_eq!(count(&out, "W-OVERLAP-ARMS"), 0, "{arms:?}: {out:?}");
        assert_eq!(count(&out, "E-ARM-DEAD"), 0, "not wholly covered: {out:?}");
    }
}

// The descending-threshold cascade — THE common numeric idiom — is clean.
#[test]
fn descending_threshold_cascade_is_clean() {
    let out = diags(&match_src("run.n", &["3..", "1.."], true));
    assert!(out.is_empty(), "expected zero diagnostics: {out:?}");
}

// A point (or degenerate `n..n` range) already covered still warns
// (`| 9` keeps the arm live, so C4 does not fold the warning into an
// E-ARM-DEAD).
#[test]
fn covered_point_still_warns_overlap() {
    for lit in ["5", "5..5", "3"] {
        let out = codes(&match_src("run.n", &["1..5", &format!("{lit} | 9")], true));
        assert_eq!(count(&out, "W-OVERLAP-ARMS"), 1, "`{lit}`: {out:?}");
        assert_eq!(count(&out, "E-ARM-DEAD"), 0, "{out:?}");
    }
}

// §4 E-ARM-DEAD: an arm whose whole `is` set lies inside the union of
// earlier unguarded arms — a sub-range, a point, or a union of two arms.
#[test]
fn subsumed_range_or_point_is_arm_dead() {
    for later in ["2..4", "3", "1..5", "5.0"] {
        let out = diags(&match_src("run.n", &["1..5", later], true));
        let dead: Vec<_> = out.iter().filter(|d| d.code == "E-ARM-DEAD").collect();
        assert_eq!(
            dead.len(),
            1,
            "`{later}` after `1..5` must be dead: {out:?}"
        );
        assert!(
            dead[0].message.contains("(`1..5`)"),
            "cites the covering arm: {}",
            dead[0].message
        );
    }
    let out = diags(&match_src("run.n", &["..0", "0..10", "-3..3"], true));
    let dead: Vec<_> = out.iter().filter(|d| d.code == "E-ARM-DEAD").collect();
    assert_eq!(dead.len(), 1, "covered by two arms together: {out:?}");
    assert!(
        dead[0].message.contains("arms together, the first at")
            && dead[0].message.contains("(`..0`)"),
        "a jointly covered range must not claim one arm covers it: {}",
        dead[0].message
    );
    // Covered by a LATER single arm, not the earliest overlapping one: cite it.
    let out = diags(&match_src("run.n", &["3..", "1..", "2..4"], true));
    let dead: Vec<_> = out.iter().filter(|d| d.code == "E-ARM-DEAD").collect();
    assert_eq!(dead.len(), 1, "{out:?}");
    assert!(
        dead[0].message.contains("unguarded arm at") && dead[0].message.contains("(`1..`)"),
        "{}",
        dead[0].message
    );
}

// `1` and `1.0` are the same point (dsl 0.18.0 §6).
#[test]
fn equal_points_in_different_spellings_are_arm_dead() {
    let out = codes(&match_src("run.n", &["1", "1.0"], true));
    assert_eq!(count(&out, "E-ARM-DEAD"), 1, "{out:?}");
}

// A partially covered range is not dead, and a GUARDED earlier arm never
// subsumes (dsl 0.4.0 §5.2 rule 2).
#[test]
fn partial_or_guarded_coverage_is_not_arm_dead() {
    let out = codes(&match_src("run.n", &["1..5", "4..6"], true));
    assert_eq!(count(&out, "E-ARM-DEAD"), 0, "{out:?}");
    let out = codes(&format!(
        "{HDR}<match on=\"run.n\">\n\
         <when is=\"1..5\" test=\"run.flag\">\n@narrator: a\n</when>\n\
         <when is=\"2\">\n@narrator: b\n</when>\n\
         <otherwise>\n@narrator: o\n</otherwise>\n\
         </match>\n"
    ));
    assert_eq!(count(&out, "E-ARM-DEAD"), 0, "{out:?}");
}

// §4 W-OTHERWISE-DEAD: unguarded ranges cover the whole line of a
// defaulted (never-unset) number.
#[test]
fn full_line_coverage_makes_otherwise_dead() {
    let out = codes(&match_src("run.n", &["..0", "0.."], true));
    assert_eq!(count(&out, "W-OTHERWISE-DEAD"), 1, "{out:?}");
    let out = codes(&match_src("run.n", &["..0", "1.."], true));
    assert_eq!(
        count(&out, "W-OTHERWISE-DEAD"),
        0,
        "gap (0,1) reaches otherwise: {out:?}"
    );
}

// A maybe-unset number (`run.*`, no default): covering the line is not
// enough — the `unset` case still needs an arm (or `<otherwise>`, which is
// then live).
#[test]
fn maybe_unset_number_still_requires_unset_arm() {
    let out = codes(&match_src("run.unbound", &["..0", "0.."], false));
    assert_eq!(count(&out, "E-UNSET-UNCOVERED"), 1, "{out:?}");
    assert_eq!(
        count(&out, "E-NONEXHAUSTIVE"),
        0,
        "the line is covered: {out:?}"
    );

    let out = codes(&match_src("run.unbound", &["..0", "0..", "unset"], false));
    assert!(out.is_empty(), "line + unset covered: {out:?}");

    let out = codes(&match_src("run.unbound", &["..0", "0.."], true));
    assert_eq!(
        count(&out, "W-OTHERWISE-DEAD"),
        0,
        "otherwise catches unset: {out:?}"
    );
    let out = codes(&match_src("run.unbound", &["..0", "0..", "unset"], true));
    assert_eq!(count(&out, "W-OTHERWISE-DEAD"), 1, "{out:?}");
}

// §2 E-WHEN-RANGE: empty (`3..1`) and malformed (`a..b`, `..`, `1...2`,
// `1..2..3`) ranges, anchored at the literal itself.
#[test]
fn bad_ranges_are_e_when_range_at_the_literal() {
    for (lit, kind) in [
        ("3..1", "empty"),
        ("a..b", "malformed"),
        ("..", "malformed"),
        ("1...2", "malformed"),
        ("1..2..3", "malformed"),
    ] {
        let text = match_src("run.n", &[&format!("0 | {lit}")], true);
        let out = diags(&text);
        let hits: Vec<_> = out.iter().filter(|d| d.code == "E-WHEN-RANGE").collect();
        assert_eq!(hits.len(), 1, "`{lit}`: {out:?}");
        assert_eq!(&text[hits[0].span.byte_start..hits[0].span.byte_end], lit);
        assert!(hits[0]
            .message
            .starts_with(&format!("{kind} range literal `{lit}`")));
        assert!(
            !out.iter().any(|d| d.code == "E-WHEN-LITERAL-DOMAIN"),
            "a bad range is not also a domain error: {out:?}"
        );
    }
    let out = diags(&match_src("run.n", &["3..1"], true));
    assert_eq!(
        out.iter()
            .find(|d| d.code == "E-WHEN-RANGE")
            .map(|d| d.message.as_str()),
        Some(
            "empty range literal `3..1`: its lower bound is above its upper bound, so it \
             matches nothing (dsl 0.18.0 §2)"
        )
    );
    let out = diags(&match_src("run.n", &["a..b"], true));
    assert_eq!(
        out.iter()
            .find(|d| d.code == "E-WHEN-RANGE")
            .map(|d| d.message.as_str()),
        Some(
            "malformed range literal `a..b`: a range is `N..M`, `N..`, or `..M` with decimal \
             number bounds (dsl 0.18.0 §2)"
        )
    );
}

// A bad range covers nothing: it neither completes coverage nor makes a
// later arm dead, and fires regardless of the subject's domain.
#[test]
fn bad_range_contributes_no_coverage() {
    let out = codes(&match_src("run.n", &["..0 | 9..1", "0.."], false));
    assert_eq!(count(&out, "E-WHEN-RANGE"), 1, "{out:?}");
    assert_eq!(count(&out, "E-NONEXHAUSTIVE"), 0, "{out:?}");
    let out = codes(&match_src("run.n", &["1..", "..1", "5..2"], false));
    assert_eq!(count(&out, "E-WHEN-RANGE"), 1, "{out:?}");
    assert_eq!(
        count(&out, "E-ARM-DEAD"),
        0,
        "E-WHEN-RANGE owns the root: {out:?}"
    );
    let out = codes(&match_src("run.rank", &["x..y"], true));
    assert_eq!(count(&out, "E-WHEN-RANGE"), 1, "{out:?}");
}

// §2: a range against a resolved non-numeric subject (enum, bool, string)
// is E-WHEN-LITERAL-DOMAIN, anchored at the literal.
#[test]
fn range_on_non_numeric_subject_is_literal_domain() {
    for subject in ["run.rank", "run.flag", "run.name"] {
        let text = match_src(subject, &["1..2"], true);
        let out = diags(&text);
        let hits: Vec<_> = out
            .iter()
            .filter(|d| d.code == "E-WHEN-LITERAL-DOMAIN")
            .collect();
        assert_eq!(hits.len(), 1, "{subject}: {out:?}");
        assert_eq!(
            &text[hits[0].span.byte_start..hits[0].span.byte_end],
            "1..2"
        );
        assert_eq!(
            hits[0].message,
            "`1..2` is a numeric range, which cannot match a non-numeric subject (dsl 0.18.0 §2)"
        );
        assert!(!out.iter().any(|d| d.code == "E-ARM-DEAD"), "D4: {out:?}");
    }
    // A number subject accepts ranges; an unresolved subject makes no claim.
    let out = codes(&match_src("run.n", &["1..2"], true));
    assert_eq!(count(&out, "E-WHEN-LITERAL-DOMAIN"), 0, "{out:?}");
    let out = codes(&match_src("run.undeclared", &["1..2"], true));
    assert_eq!(count(&out, "E-WHEN-LITERAL-DOMAIN"), 0, "{out:?}");
}

// Signed and decimal bounds participate as real intervals.
#[test]
fn negative_and_decimal_bounds_cover_correctly() {
    let out = diags(&match_src(
        "run.n",
        &["..-3", "-3..-1", "-1..0.5", "0.5.."],
        false,
    ));
    assert!(out.is_empty(), "expected zero diagnostics: {out:?}");
    let out = diags(&match_src("run.n", &["..-3", "-1.."], false));
    let d = out.iter().find(|d| d.code == "E-NONEXHAUSTIVE").unwrap();
    assert!(
        d.message
            .contains("numbers strictly between -3 and -1 are not covered"),
        "{}",
        d.message
    );
}
