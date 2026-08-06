//! dsl 0.10.0 §4 (backlog #6, D-J, D-L): the six logic constructs close their
//! attribute sets, emitting `E-UNKNOWN-ATTR` at the offending attribute's own
//! span. Driven through the assembled `check()` over inline `state:`
//! frontmatter, mirroring `tests/reachability.rs`'s harness.
use lute_check::{check, CheckInput, CheckResult, Mode, SchemaImports};
use lute_manifest::provider::ProviderSet;

fn run(text: &str) -> CheckResult {
    let input = CheckInput {
        text: text.to_string(),
        uri: "logic_attrs".into(),
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

const HDR: &str = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  \
    run.x: { type: bool, default: false }\n  \
    run.rank: { type: { enum: [a, b] }, default: a }\n---\n## Shot 1.\n";

fn unknown_attrs(text: &str) -> usize {
    codes(text).iter().filter(|c| *c == "E-UNKNOWN-ATTR").count()
}

/// T8.2: `<choice goto=…>` — a routing declaration discarded in silence, on a
/// file `lute check` calls ok.
#[test]
fn choice_goto_is_unknown_attr() {
    let t = format!(
        "{HDR}<branch id=\"b\">\n<choice id=\"c\" label=\"L\" goto=\"ep08\">\n\
         @narrator: hi\n</choice>\n</branch>\n"
    );
    assert_eq!(unknown_attrs(&t), 1, "{:?}", codes(&t));
}

/// The diagnostic anchors column-exact at the attribute's own KEY, matching
/// `E-PERSIST-REMOVED`'s existing behaviour.
#[test]
fn span_is_the_attribute_key() {
    let t = format!(
        "{HDR}<branch id=\"b\">\n<choice id=\"c\" label=\"L\" goto=\"ep08\">\n\
         @narrator: hi\n</choice>\n</branch>\n"
    );
    let d = run(&t)
        .diagnostics
        .into_iter()
        .find(|d| d.code == "E-UNKNOWN-ATTR")
        .expect("expected E-UNKNOWN-ATTR");
    assert_eq!(&t[d.span.byte_start..d.span.byte_end], "goto=\"ep08\"");
}

/// D-L: the two `<choice>` positions have DIFFERENT permitted sets. `once` and
/// `exit` are hub-choice flags; the hub reducer is their only reader. Enforcing
/// one merged set would leave a branch choice carrying `exit` silent, which is
/// T8.2's defect wearing a smaller hat.
#[test]
fn once_and_exit_are_hub_only() {
    let branch = format!(
        "{HDR}<branch id=\"b\">\n<choice id=\"c\" label=\"L\" once exit>\n\
         @narrator: hi\n</choice>\n</branch>\n"
    );
    assert_eq!(unknown_attrs(&branch), 2, "{:?}", codes(&branch));
    let hub = format!(
        "{HDR}<hub id=\"h\">\n<choice id=\"c\" label=\"L\" once>\n@narrator: hi\n</choice>\n\
         <choice id=\"c2\" label=\"M\" exit>\n@narrator: bye\n</choice>\n</hub>\n"
    );
    assert_eq!(unknown_attrs(&hub), 0, "{:?}", codes(&hub));
}

/// `<hub>` has NO `id` field in the AST (`ast.rs:120-125`): its `id=` survives
/// in the residual list and the checker reads it from there
/// (`match_check.rs:432`). `id` MUST be permitted or the rule rejects every hub
/// in existence.
#[test]
fn hub_id_is_permitted() {
    let t = format!(
        "{HDR}<hub id=\"h\">\n<choice id=\"c\" label=\"L\" exit>\n@narrator: hi\n</choice>\n</hub>\n"
    );
    assert_eq!(unknown_attrs(&t), 0, "{:?}", codes(&t));
}

/// Task 1's retained lists, now read: `<match>`, `<when>` and `<otherwise>`
/// each close.
#[test]
fn match_when_otherwise_close() {
    let t = format!(
        "{HDR}<match on=\"run.rank\" bogus=\"1\">\n\
         <when is=\"a\" nonsense=\"2\">\n@narrator: a\n</when>\n\
         <otherwise junk=\"3\">\n@narrator: o\n</otherwise>\n</match>\n"
    );
    assert_eq!(unknown_attrs(&t), 3, "{:?}", codes(&t));
}

/// D-J: `<otherwise>` joins the table as the empty set and the PARSER's
/// attribute arm is deleted. An attribute there is now `E-UNKNOWN-ATTR` like
/// every other logic tag, never `E-LOGIC-CONTENT` — that code survives
/// unchanged for its three body-shape rules and only those.
#[test]
fn otherwise_attr_is_no_longer_logic_content() {
    let t = format!(
        "{HDR}<match on=\"run.rank\">\n<when is=\"a\">\n@narrator: a\n</when>\n\
         <otherwise junk=\"3\">\n@narrator: o\n</otherwise>\n</match>\n"
    );
    let cs = codes(&t);
    assert!(cs.contains(&"E-UNKNOWN-ATTR".to_string()), "{cs:?}");
    assert!(!cs.contains(&"E-LOGIC-CONTENT".to_string()), "{cs:?}");
}

/// `E-LOGIC-CONTENT`'s three BODY-SHAPE rules are untouched.
#[test]
fn logic_content_still_owns_body_shape() {
    let t = format!("{HDR}<branch id=\"b\">\n@narrator: stray\n</branch>\n");
    assert!(
        codes(&t).contains(&"E-LOGIC-CONTENT".to_string()),
        "{:?}",
        codes(&t)
    );
}

/// §4's fourth column is NOT a permitted set and the closure rule SKIPS it.
/// `persist` has had a dedicated code with a column-exact span since 0.6.0, and
/// `check.rs:2564-2565` records the invariant that it is "never reported as
/// unknown/extra". A table built without the carve-out breaks that invariant on
/// the exact attribute §4 uses to argue its case.
#[test]
fn persist_is_told_once_by_its_own_code() {
    let t = format!(
        "{HDR}<branch id=\"b\">\n<choice id=\"c\" label=\"L\" persist=\"run\" into=\"run.x\">\n\
         @narrator: hi\n</choice>\n</branch>\n"
    );
    let cs = codes(&t);
    assert!(cs.contains(&"E-PERSIST-REMOVED".to_string()), "{cs:?}");
    assert!(!cs.contains(&"E-UNKNOWN-ATTR".to_string()), "{cs:?}");
}

/// `as` is carved out for the same reason. Until Task 4 lands it draws NOTHING
/// — which is HEAD's behaviour and therefore no regression; what matters here
/// is that the closure rule does not claim it.
#[test]
fn as_is_not_claimed_by_the_closure_rule() {
    let t = format!(
        "{HDR}<branch id=\"b\">\n<choice id=\"c\" label=\"L\" as=\"run.x\">\n\
         @narrator: hi\n</choice>\n</branch>\n"
    );
    assert!(
        !codes(&t).contains(&"E-UNKNOWN-ATTR".to_string()),
        "{:?}",
        codes(&t)
    );
}

/// Every key the shipped corpus actually uses is permitted — §13.2's census,
/// expressed as a test so it cannot rot.
#[test]
fn the_corpus_vocabulary_is_permitted() {
    let t = format!(
        "{HDR}<branch id=\"b\">\n\
         <choice id=\"c\" label=\"L\" when=\"run.x\" into=\"run.x\" value=\"true\">\n\
         @narrator: hi\n</choice>\n</branch>\n\
         <hub id=\"h\">\n<choice id=\"h1\" label=\"L\" once>\n@narrator: a\n</choice>\n\
         <choice id=\"h2\" label=\"M\" exit>\n@narrator: b\n</choice>\n</hub>\n\
         <match on=\"run.rank\">\n<when is=\"a\">\n@narrator: c\n</when>\n\
         <when test=\"run.x\">\n@narrator: d\n</when>\n\
         <otherwise>\n@narrator: e\n</otherwise>\n</match>\n"
    );
    assert_eq!(unknown_attrs(&t), 0, "{:?}", codes(&t));
}
