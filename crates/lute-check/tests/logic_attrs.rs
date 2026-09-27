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
    codes(text)
        .iter()
        .filter(|c| *c == "E-UNKNOWN-ATTR")
        .count()
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

/// Round-5 T3-1: word-processor curly quotes around a value split it into one
/// `E-UNKNOWN-ATTR` per word, each with an unrelated did-you-mean (`the` →
/// `when`). Now one `E-ATTR-QUOTE` anchored at the curly quote, and nothing
/// else.
#[test]
fn curly_quoted_value_is_one_attr_quote_error() {
    let t = format!(
        "{HDR}<branch id=\"b\">\n<choice id=\"oven\" label=“Open the bread oven”>\n\
         @narrator: Hot.\n</choice>\n</branch>\n"
    );
    let ds = run(&t).diagnostics;
    let codes: Vec<&str> = ds.iter().map(|d| d.code.as_str()).collect();
    assert_eq!(codes, ["E-ATTR-QUOTE"], "{ds:?}");
    let d = &ds[0];
    assert!(t[d.span.byte_start..].starts_with('“'), "{d:?}");
    assert!(
        d.message.contains("straight quotes `\"` — found `“`"),
        "{}",
        d.message
    );
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

/// dsl 0.11.0 (branch prompt/timeout): `prompt`/`timeout` join `id` in the
/// permitted set (no `E-UNKNOWN-ATTR`) and, well-formed, draw no diagnostic
/// at all.
#[test]
fn branch_prompt_and_timeout_are_permitted_and_valid() {
    let t = format!(
        "{HDR}<branch id=\"b\" prompt=\"What now?\" timeout=\"10\">\n\
         <choice id=\"c\" label=\"L\">\n@narrator: hi\n</choice>\n</branch>\n"
    );
    assert_eq!(codes(&t), Vec::<String>::new(), "{:?}", codes(&t));
}

/// `timeout="0"` and a non-numeric `timeout` both draw `E-BRANCH-TIMEOUT`:
/// the engine wire's countdown cannot count down from zero or parse
/// garbage — the two ways `str::parse::<u32>` combined with the positivity
/// check reject a value.
#[test]
fn branch_timeout_zero_or_non_numeric_is_rejected() {
    for bad in ["0", "abc"] {
        let t = format!(
            "{HDR}<branch id=\"b\" prompt=\"What now?\" timeout=\"{bad}\">\n\
             <choice id=\"c\" label=\"L\">\n@narrator: hi\n</choice>\n</branch>\n"
        );
        assert_eq!(
            codes(&t),
            vec!["E-BRANCH-TIMEOUT".to_string()],
            "{bad:?}: {:?}",
            codes(&t)
        );
    }
}

/// An empty `prompt` is not a valid "no prompt" spelling — `E-BRANCH-PROMPT`,
/// column-exact at the attribute (matching `E-UNKNOWN-ATTR`'s own anchor
/// convention).
#[test]
fn branch_empty_prompt_is_rejected_at_its_own_span() {
    let t = format!(
        "{HDR}<branch id=\"b\" prompt=\"\" timeout=\"10\">\n\
         <choice id=\"c\" label=\"L\">\n@narrator: hi\n</choice>\n</branch>\n"
    );
    let d = run(&t)
        .diagnostics
        .into_iter()
        .find(|d| d.code == "E-BRANCH-PROMPT")
        .expect("expected E-BRANCH-PROMPT");
    assert_eq!(&t[d.span.byte_start..d.span.byte_end], "prompt=\"\"");
}

const QUEST_HDR: &str =
    "---\nkind: quest\nstate:\n  run.flag: { type: bool, default: false }\n---\n";

fn unknown_attr_messages(text: &str) -> Vec<String> {
    run(text)
        .diagnostics
        .into_iter()
        .filter(|d| d.code == "E-UNKNOWN-ATTR")
        .map(|d| d.message)
        .collect()
}

/// 0.21.1 T1-7: `<quest>`, `<objective>` and `<on>` accepted any invented
/// attribute and dropped it from the IR (`fial=` compiled to a quest with no
/// fail condition). Each is now `E-UNKNOWN-ATTR`, with a did-you-mean when a
/// real key is close.
#[test]
fn quest_objective_on_close_their_attrs_with_did_you_mean() {
    let t = format!(
        "{QUEST_HDR}<quest id=\"q\" start=\"run.flag\" fial=\"run.flag\" banana=\"yes\">\n\
         <objective id=\"o\" done=\"run.flag\" deadline=\"run.flag\" optinal/>\n\
         <on event=\"questComplete\" whn=\"run.flag\">\n@x: done\n</on>\n</quest>\n"
    );
    let msgs = unknown_attr_messages(&t);
    assert_eq!(msgs.len(), 5, "{msgs:?}");
    for (key, near) in [
        ("fial", Some("fail")),
        ("banana", None),
        ("deadline", None),
        ("optinal", Some("optional")),
        ("whn", Some("when")),
    ] {
        let m = msgs
            .iter()
            .find(|m| m.contains(&format!("attribute `{key}`")))
            .unwrap_or_else(|| panic!("no E-UNKNOWN-ATTR for `{key}`: {msgs:?}"));
        match near {
            Some(near) => assert!(m.contains(&format!("did you mean `{near}`?")), "{m}"),
            None => assert!(!m.contains("did you mean"), "{m}"),
        }
    }
}

/// Every key the parser extracts stays legal — the closure must not reject a
/// well-formed quest (`after`, subquest `quest=`, `optional`, objective `on`).
#[test]
fn quest_objective_on_permitted_keys_are_clean() {
    let t = format!(
        "{QUEST_HDR}<quest id=\"p\" title=\"P\" start=\"run.flag\" fail=\"run.flag\">\n\
         <objective id=\"o\" title=\"O\" done=\"run.flag\" visibleWhen=\"run.flag\" optional/>\n\
         <objective id=\"c\" quest=\"child\"/>\n\
         <on event=\"questComplete\" when=\"run.flag\">\n@x: done\n</on>\n</quest>\n\
         <quest id=\"child\" follows=\"active(p)\" start=\"run.flag\">\n\
         <objective id=\"v\" on=\"visit\"/>\n</quest>\n"
    );
    assert!(unknown_attr_messages(&t).is_empty(), "{:?}", codes(&t));
}

/// Round-6 R-10/R-21/R-22: the three renamed quest-layer attributes. The old
/// spelling is `E-UNKNOWN-ATTR` at that attribute, and the message names the
/// new one (clean cutover: the old key is never read).
#[test]
fn renamed_quest_layer_attrs_name_their_new_spelling() {
    let t = format!(
        "{QUEST_HDR}<quest id=\"q\" start=\"true\" after=\"active(p)\">\n\
         <reward kind=\"gold\" on=\"failed\"/>\n\
         <objective id=\"o\" done=\"run.flag\" when=\"run.flag\"/>\n</quest>\n\
         <quest id=\"p\" start=\"true\">\n<objective id=\"d\" done=\"true\"/>\n</quest>\n"
    );
    let diags: Vec<_> = run(&t)
        .diagnostics
        .into_iter()
        .filter(|d| d.code == "E-UNKNOWN-ATTR")
        .collect();
    for (old, new) in [
        ("after", "follows="),
        ("on", "outcome="),
        ("when", "visibleWhen="),
    ] {
        let d = diags
            .iter()
            .find(|d| t[d.span.byte_start..d.span.byte_end].starts_with(&format!("{old}=")))
            .unwrap_or_else(|| panic!("no E-UNKNOWN-ATTR at `{old}=`: {diags:?}"));
        assert!(
            d.message.contains(&format!("is now `{new}`")),
            "{}",
            d.message
        );
    }
    assert_eq!(diags.len(), 3, "{diags:?}");
}

const LORE_HDR: &str =
    "---\nkind: lore\nid: lr\nstate:\n  run.flag: { type: bool, default: false }\n---\n";

/// A key another construct or layer owns names where it lives (`occasion=`
/// on a beat is `on=`, `rearm=` is a quest's, `spentBy=` a beat's), once:
/// the construct's own "names no occasion" / "has no `event`" does not
/// repeat it.
#[test]
fn a_key_of_another_construct_names_where_it_lives() {
    let entry =
        |attr: &str| format!("{LORE_HDR}<entry id=\"e\" {attr} title=\"E\">\n@x: e\n</entry>\n");
    let beat = |attr: &str| format!("{LORE_HDR}<beat id=\"b\" {attr}>\n@x: b\n</beat>\n");
    let quest = |attr: &str, body: &str| {
        format!("{QUEST_HDR}<quest id=\"q\" start=\"true\" {attr}>\n{body}</quest>\n")
    };
    let done = "<objective id=\"d\" done=\"run.flag\"/>\n";
    for (text, needle) in [
        (
            entry("occasion=\"dusk\""),
            "names the occasion it answers with `on=`",
        ),
        (
            entry("event=\"dusk\" when=\"run.flag\""),
            "names the occasion it answers with `on=`",
        ),
        (
            entry("on=\"dusk\" rearm=\"run.flag\""),
            "`rearm=` is an attribute of a `<quest>`",
        ),
        (
            entry("on=\"dusk\" tier=\"run\""),
            "`tier=` is an attribute of a `<quest>`",
        ),
        (
            beat("occasion=\"dusk\""),
            "names the occasion it answers with `on=`",
        ),
        (
            beat("event=\"dusk\" when=\"run.flag\""),
            "names the occasion it answers with `on=`",
        ),
        (
            beat("on=\"dusk\" rearm=\"run.flag\""),
            "`rearm=` is an attribute of a `<quest>`",
        ),
        (
            beat("on=\"dusk\" tier=\"run\""),
            "`tier=` is an attribute of a `<quest>`",
        ),
        (
            quest("spentBy=\"run.flag\"", done),
            "`spentBy=` is a beat attribute",
        ),
        (
            quest("on=\"dusk\"", done),
            "an objective does, `<objective on=\"<occasion>\">`",
        ),
        (
            quest("occasion=\"dusk\"", done),
            "an objective does, `<objective on=\"<occasion>\">`",
        ),
        (
            quest(
                "",
                "<objective id=\"o\" occasion=\"dusk\" done=\"run.flag\"/>\n",
            ),
            "names the occasion that judges it with `on=`",
        ),
        (
            quest(
                "",
                "<objective id=\"o\" event=\"dusk\" done=\"run.flag\"/>\n",
            ),
            "names the occasion that judges it with `on=`",
        ),
        (
            quest(
                "",
                &format!("{done}<on occasion=\"questComplete\">\n@x: y\n</on>\n"),
            ),
            "names what it answers with `event=`",
        ),
    ] {
        let diags = run(&text).diagnostics;
        let errors: Vec<_> = diags
            .iter()
            .filter(|d| d.severity == lute_core_span::Severity::Error)
            .collect();
        assert_eq!(errors.len(), 1, "{text}\n{errors:?}");
        assert_eq!(errors[0].code, "E-UNKNOWN-ATTR", "{text}\n{errors:?}");
        assert!(
            errors[0].message.contains(needle),
            "{text}\n{}",
            errors[0].message
        );
    }
}

fn flag_values(text: &str) -> Vec<(String, String)> {
    run(text)
        .diagnostics
        .into_iter()
        .filter(|d| d.code == "E-FLAG-VALUE")
        .map(|d| {
            (
                text[d.span.byte_start..d.span.byte_end].to_string(),
                d.message,
            )
        })
        .collect()
}

/// dsl 0.28.0 §1 (round-6 T1-3): a flag given a value that is not
/// `true`/`false` read as `false` without a word — `optional="yes"` made the
/// objective required, `exit="yes"` left the choice a non-exit. Every flag is
/// now `E-FLAG-VALUE` at the attribute, naming the bare form.
#[test]
fn a_flag_with_a_non_flag_value_is_flag_value_at_the_attribute() {
    let quest = format!(
        "{QUEST_HDR}<quest id=\"q\" start=\"run.flag\">\n\
         <objective id=\"o\" done=\"run.flag\" optional=\"yes\"/>\n</quest>\n"
    );
    let fv = flag_values(&quest);
    assert_eq!(fv.len(), 1, "{:?}", codes(&quest));
    assert_eq!(fv[0].0, "optional=\"yes\"");
    assert!(
        fv[0].1.contains("write it bare (`<objective … optional>`)")
            && fv[0].1.contains("`optional=\"false\"`"),
        "{}",
        fv[0].1
    );

    let hub = format!(
        "{HDR}<hub id=\"h\">\n<choice id=\"a\" label=\"A\" exit=\"yes\">\n@x: a\n</choice>\n\
         <choice id=\"b\" label=\"B\" exit>\n@x: b\n</choice>\n</hub>\n"
    );
    let fv = flag_values(&hub);
    assert_eq!(fv.len(), 1, "{:?}", codes(&hub));
    assert_eq!(fv[0].0, "exit=\"yes\"");
    assert!(fv[0].1.contains("`exit` is a flag"), "{}", fv[0].1);

    let beat =
        "---\nkind: lore\nid: l\n---\n<beat id=\"b\" on=\"talk\" also=\"yes\">\n@n: hi\n</beat>\n";
    let fv = flag_values(beat);
    assert_eq!(fv.len(), 1, "{:?}", codes(beat));
    assert_eq!(fv[0].0, "also=\"yes\"");
    assert!(!codes(beat).contains(&"E-BEAT-ATTR".to_string()));
}

/// R-6: a beat/entry repetition period on a hub choice read as `false` (not
/// once at all). A choice's `once` has one meaning; the period is refused.
#[test]
fn a_period_on_a_choice_once_says_it_is_a_beat_key() {
    for period in ["run", "user", "day", "week", "slot", "season:harvest"] {
        let t = format!(
            "{HDR}<hub id=\"h\">\n<choice id=\"a\" label=\"A\" once=\"{period}\">\n@x: a\n</choice>\n\
             <choice id=\"b\" label=\"B\" exit>\n@x: b\n</choice>\n</hub>\n"
        );
        let fv = flag_values(&t);
        assert_eq!(fv.len(), 1, "{period}: {:?}", codes(&t));
        assert_eq!(fv[0].0, format!("once=\"{period}\""));
        assert!(
            fv[0].1.contains("is a beat/entry `once` key")
                && fv[0].1.contains("once per hub visit — write it bare"),
            "{}",
            fv[0].1
        );
    }
}

/// `="true"` really is true and `="false"` really is false: `exit="true"`
/// is an exit (it used to draw `E-HUB-NO-EXIT`), `once="false"` is not
/// `once`, and none of the three spellings is `E-FLAG-VALUE`.
#[test]
fn true_and_false_flag_values_mean_what_they_say() {
    let hub = |a: &str, b: &str| {
        format!(
            "{HDR}<hub id=\"h\">\n<choice id=\"a\" label=\"A\" {a}>\n@x: a\n</choice>\n\
             <choice id=\"b\" label=\"B\" {b}>\n@x: b\n</choice>\n</hub>\n"
        )
    };
    let exit_true = hub("once=\"true\"", "exit=\"true\"");
    assert!(run(&exit_true).ok, "{:?}", codes(&exit_true));
    let all_once = hub("once", "once=\"true\"");
    assert!(run(&all_once).ok, "{:?}", codes(&all_once));
    let once_off = hub("once", "once=\"false\"");
    let cs = codes(&once_off);
    assert!(cs.contains(&"E-HUB-NO-EXIT".to_string()), "{cs:?}");
    assert!(!cs.contains(&"E-FLAG-VALUE".to_string()), "{cs:?}");
}
