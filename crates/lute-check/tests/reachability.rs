//! B3 (0.4.0 T3): `E-WHEN-LITERAL-DOMAIN` — an `<when is="…">` literal outside
//! the subject's decided finite domain (dsl 0.4 §5.2, §6.3). Per-literal
//! domain membership inside the existing exhaustiveness engine
//! (`match_check.rs`'s `DomainInfo`/`infer_domain`) — driven through the
//! assembled `check()` over inline `state:` frontmatter, mirroring
//! `tests/hub.rs`'s `run()`/`codes()` harness.
use lute_check::{check, CheckInput, CheckResult, Mode, SchemaImports};
use lute_manifest::provider::ProviderSet;

fn run(text: &str) -> CheckResult {
    let input = CheckInput {
        text: text.to_string(),
        uri: "reachability".into(),
        snapshot: lute_manifest::core::load_core_snapshot(),
        providers: ProviderSet::default(),
        mode: Mode::Author,
        imports: SchemaImports::default(),
        components: Default::default(),
    };
    check(&input)
}

fn codes(text: &str) -> Vec<String> {
    run(text).diagnostics.into_iter().map(|d| d.code).collect()
}

// `run.rank` (enum, defaulted — never unset), `run.flag` (bool, defaulted),
// `run.n` (number, defaulted), `run.unbound` (number, NO default — maybe
// unset, dsl §9.4).
const HDR: &str = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  \
    run.rank: { type: { enum: [fail, bronze, silver, gold] }, default: fail }\n  \
    run.flag: { type: bool, default: false }\n  \
    run.n: { type: number, default: 0 }\n  \
    run.unbound: { type: number }\n---\n## Shot 1.\n";

// (Appendix A) A foreign enum member — a typo (`platnum` against
// `[fail, bronze, silver, gold]`) — flags E-WHEN-LITERAL-DOMAIN.
#[test]
fn foreign_enum_member_is_literal_domain() {
    let out = codes(&format!(
        "{HDR}<match on=\"run.rank\">\n\
         <when is=\"platnum\">\n@narrator: x\n</when>\n\
         <otherwise>\n@narrator: o\n</otherwise>\n\
         </match>\n"
    ));
    assert!(
        out.contains(&"E-WHEN-LITERAL-DOMAIN".to_string()),
        "a foreign enum member typo must flag E-WHEN-LITERAL-DOMAIN: {out:?}"
    );
}

// The diagnostic's span slices the source to exactly the offending literal
// text, not the whole `is=` pattern.
#[test]
fn span_points_at_the_literal() {
    let text = format!(
        "{HDR}<match on=\"run.rank\">\n\
         <when is=\"platnum\">\n@narrator: x\n</when>\n\
         <otherwise>\n@narrator: o\n</otherwise>\n\
         </match>\n"
    );
    let result = run(&text);
    let diag = result
        .diagnostics
        .iter()
        .find(|d| d.code == "E-WHEN-LITERAL-DOMAIN")
        .unwrap_or_else(|| panic!("expected E-WHEN-LITERAL-DOMAIN: {:?}", result.diagnostics));
    assert_eq!(
        &text[diag.span.byte_start..diag.span.byte_end],
        "platnum",
        "span must bound exactly the literal, not the whole `is=` pattern"
    );
}

// `is="gold|platnum"`: only the foreign alternative (`platnum`) flags — the
// in-domain alternative (`gold`) contributes no diagnostic of its own, and
// (D4) the arm is never ALSO flagged E-ARM-DEAD.
#[test]
fn mixed_alternation_flags_only_foreign() {
    let out = codes(&format!(
        "{HDR}<match on=\"run.rank\">\n\
         <when is=\"gold|platnum\">\n@narrator: x\n</when>\n\
         <otherwise>\n@narrator: o\n</otherwise>\n\
         </match>\n"
    ));
    let flags = out
        .iter()
        .filter(|c| c.as_str() == "E-WHEN-LITERAL-DOMAIN")
        .count();
    assert_eq!(
        flags, 1,
        "exactly one alternative (`platnum`) is foreign: {out:?}"
    );
    assert!(
        !out.contains(&"E-ARM-DEAD".to_string()),
        "D4: this code owns the foreign-literal root, never piled with E-ARM-DEAD: {out:?}"
    );
}

// A bool literal against an enum domain is a domain-SHAPE mismatch (rule 2),
// not merely a missing member.
#[test]
fn bool_literal_against_enum_flags() {
    let out = codes(&format!(
        "{HDR}<match on=\"run.rank\">\n\
         <when is=\"true\">\n@narrator: x\n</when>\n\
         <otherwise>\n@narrator: o\n</otherwise>\n\
         </match>\n"
    ));
    assert!(
        out.contains(&"E-WHEN-LITERAL-DOMAIN".to_string()),
        "a bool literal against an enum domain is a shape mismatch: {out:?}"
    );
}

// `unset` on a subject that is never unset (a defaulted `bool` AND a
// defaulted `number` path) flags — rule 3, incl. rule 4's `Domain::Infinite`
// carve-out for the number subject (only the `unset` check applies there).
#[test]
fn unset_on_defaulted_path_flags() {
    let out = codes(&format!(
        "{HDR}<match on=\"run.flag\">\n\
         <when is=\"unset\">\n@narrator: a\n</when>\n\
         <otherwise>\n@narrator: b\n</otherwise>\n\
         </match>\n\
         <match on=\"run.n\">\n\
         <when is=\"unset\">\n@narrator: c\n</when>\n\
         <otherwise>\n@narrator: d\n</otherwise>\n\
         </match>\n"
    ));
    let flags = out
        .iter()
        .filter(|c| c.as_str() == "E-WHEN-LITERAL-DOMAIN")
        .count();
    assert_eq!(
        flags, 2,
        "`unset` on two never-unset (defaulted) subjects must flag twice: {out:?}"
    );
}

// `unset` on a genuinely maybe-unset subject (an un-defaulted `run.*` path)
// stays legal — no false positive.
#[test]
fn unset_on_maybe_unset_path_is_clean() {
    let out = codes(&format!(
        "{HDR}<match on=\"run.unbound\">\n\
         <when is=\"unset\">\n@narrator: x\n</when>\n\
         <otherwise>\n@narrator: o\n</otherwise>\n\
         </match>\n"
    ));
    assert!(
        !out.contains(&"E-WHEN-LITERAL-DOMAIN".to_string()),
        "`unset` on a maybe-unset (un-defaulted) path is legal: {out:?}"
    );
}

// An unresolved subject (an undeclared path) is silent here — it already
// gets its own E-UNDECLARED elsewhere, and this code never piles on with an
// unprovable domain claim.
#[test]
fn unresolved_subject_is_silent() {
    let out = codes(&format!(
        "{HDR}<match on=\"scene.nonsense.x\">\n\
         <when is=\"whatever\">\n@narrator: x\n</when>\n\
         <otherwise>\n@narrator: o\n</otherwise>\n\
         </match>\n"
    ));
    assert!(
        out.contains(&"E-UNDECLARED".to_string()),
        "sanity: `scene.nonsense.x` really is undeclared: {out:?}"
    );
    assert!(
        !out.contains(&"E-WHEN-LITERAL-DOMAIN".to_string()),
        "an unresolved (undeclared) subject must not flag: {out:?}"
    );
}

// Control (no false positives): a legitimate in-domain literal never flags.
#[test]
fn in_domain_literal_never_flags() {
    let out = codes(&format!(
        "{HDR}<match on=\"run.rank\">\n\
         <when is=\"gold\">\n@narrator: x\n</when>\n\
         <otherwise>\n@narrator: o\n</otherwise>\n\
         </match>\n"
    ));
    assert!(
        !out.contains(&"E-WHEN-LITERAL-DOMAIN".to_string()),
        "a legitimate in-domain literal must never flag: {out:?}"
    );
}
