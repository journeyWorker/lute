//! dsl 0.10.0 §3 (backlog #1, D-M): `E-SET-TYPE` — a `::set` right-hand side
//! typed against the declared type of the path it writes. Driven through the
//! assembled `check()` over inline `state:` frontmatter, mirroring
//! `tests/reachability.rs`'s `run()`/`codes()` harness.
use lute_check::{check, CheckInput, CheckResult, Mode, SchemaImports};
use lute_manifest::provider::ProviderSet;

fn run(text: &str) -> CheckResult {
    let input = CheckInput {
        text: text.to_string(),
        uri: "set_type".into(),
        snapshot: lute_manifest::core::load_core_snapshot(),
        providers: ProviderSet::default(),
        mode: Mode::Author,
        imports: SchemaImports::default(),
        components: Default::default(),
        defaults: Default::default(),
    };
    check(&input)
}

fn codes(text: &str) -> Vec<String> {
    run(text).diagnostics.into_iter().map(|d| d.code).collect()
}

// `run.n` number, `run.flag` bool, `run.note` string, `run.pick`/`run.other`
// two DIFFERENT enums.
const HDR: &str = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  \
    run.n: { type: number, default: 0 }\n  \
    run.flag: { type: bool, default: false }\n  \
    run.note: { type: string, default: \"\" }\n  \
    run.pick: { type: { enum: [blake, cass, dana] }, default: blake }\n  \
    run.other: { type: { enum: [red, green] }, default: red }\n---\n## Shot 1.\n";

fn set_codes(body: &str) -> Vec<String> {
    codes(&format!("{HDR}{body}\n"))
}

/// T3.2 probe 1: a string into a `number` target.
#[test]
fn string_into_number_is_set_type() {
    let cs = set_codes("::set{run.n += \"two\"}");
    assert!(cs.contains(&"E-SET-TYPE".to_string()), "{cs:?}");
}

/// T3.2 probe 2: a bool into a `number` target.
#[test]
fn bool_into_number_is_set_type() {
    let cs = set_codes("::set{run.n = true}");
    assert!(cs.contains(&"E-SET-TYPE".to_string()), "{cs:?}");
}

/// T3.2 probe 3: §3.3 rule 5 — `bool * 3` is ill-typed, and the message names
/// the comparison rather than the whole write.
#[test]
fn bool_times_number_is_set_type_naming_the_comparison() {
    let text = format!("{HDR}::set{{run.n += (run.n > 0) * 3}}\n");
    let d = run(&text)
        .diagnostics
        .into_iter()
        .find(|d| d.code == "E-SET-TYPE")
        .expect("rule 5 must reject a bool operand under `*`");
    assert!(
        d.message.contains("`>`") && d.message.contains("`bool`"),
        "the message must name the offending comparison: {}",
        d.message
    );
}

/// The message states the produced type, the path and the declared type
/// (§3.4), and it anchors at the RIGHT-HAND SIDE's own span, not the
/// directive's — the target is correct and the author is looking at the value.
#[test]
fn message_and_span_follow_3_4() {
    let text = format!("{HDR}::set{{run.n = true}}\n");
    let d = run(&text)
        .diagnostics
        .into_iter()
        .find(|d| d.code == "E-SET-TYPE")
        .expect("expected E-SET-TYPE");
    assert!(
        d.message.contains("writes a `bool` into `run.n`, declared `number`"),
        "got: {}",
        d.message
    );
    assert_eq!(
        &text[d.span.byte_start..d.span.byte_end],
        "true",
        "anchored at the RHS, not the whole `::set`"
    );
}

/// A well-typed write of every scalar kind draws nothing.
#[test]
fn well_typed_writes_are_clean() {
    for body in [
        "::set{run.n = 1}",
        "::set{run.n += 1}",
        "::set{run.flag = true}",
        "::set{run.note = \"hi\"}",
        "::set{run.pick = \"cass\"}",
        "::set{run.n = run.n + 1}",
        "::set{run.note = run.note + \"!\"}",
        "::set{run.flag = run.n > 0}",
    ] {
        let cs = set_codes(body);
        assert!(
            !cs.contains(&"E-SET-TYPE".to_string()),
            "{body} must be clean: {cs:?}"
        );
    }
}

/// §3.2's enum row: a string literal that is not a declared member is
/// `E-SET-TYPE`, carrying a did-you-mean over the members (the T5.4 case).
#[test]
fn foreign_enum_member_is_set_type_with_suggestion() {
    let text = format!("{HDR}::set{{run.pick = \"blaek\"}}\n");
    let d = run(&text)
        .diagnostics
        .into_iter()
        .find(|d| d.code == "E-SET-TYPE")
        .expect("a foreign enum member must be E-SET-TYPE");
    assert!(d.message.contains("did you mean `blake`?"), "got: {}", d.message);
}

/// D-M applied to §3.2's own table: an `enum`-typed READ satisfies a required
/// `string`, and copying one enum path into a DIFFERENT enum is undecidable
/// (the checker cannot know which member the source holds). Both must be clean
/// — this is the false positive D-M forbids, arriving through the table meant
/// to prevent it.
#[test]
fn enum_reads_never_false_positive() {
    for body in [
        "::set{run.note = run.pick}",
        "::set{run.other = run.pick}",
        "::set{run.pick = run.pick}",
    ] {
        let cs = set_codes(body);
        assert!(
            !cs.contains(&"E-SET-TYPE".to_string()),
            "{body} is an ordinary write and must be clean: {cs:?}"
        );
    }
}

/// §3.1: where `E-SET-OP-TYPE` fires, `E-SET-TYPE` is SUPPRESSED — the target
/// is wrong, and reporting the value against a type the target does not have
/// sends the author to the wrong end of the line.
#[test]
fn set_op_type_suppresses_set_type() {
    let cs = set_codes("::set{run.flag += \"two\"}");
    assert!(cs.contains(&"E-SET-OP-TYPE".to_string()), "{cs:?}");
    assert!(!cs.contains(&"E-SET-TYPE".to_string()), "{cs:?}");
}

/// §3.3's closing paragraph: an expression outside the decidable set is
/// ACCEPTED with no diagnostic. Never a guess (D-M).
#[test]
fn undecidable_right_hand_sides_pass() {
    for body in [
        "::set{run.n = [1, 2]}",
        "::set{run.note = run.flag ? \"a\" : 1}",
    ] {
        let cs = set_codes(body);
        assert!(
            !cs.contains(&"E-SET-TYPE".to_string()),
            "{body} is undecidable and must pass: {cs:?}"
        );
    }
}

/// §3.3 rule 4: `now()` produces `narrativeTime`, which is not an
/// author-declarable state type, so it is `E-SET-TYPE` against every legal
/// target. This is the ONLY way §3.2's `narrativeTime` clause is DERIVABLE
/// from §3.3's closed rule set.
#[test]
fn now_into_a_scalar_is_set_type() {
    let cs = set_codes("::set{run.n = now()}");
    assert!(cs.contains(&"E-SET-TYPE".to_string()), "{cs:?}");
}
