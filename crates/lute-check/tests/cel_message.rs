//! §8.1 (T1–T3): the writer-voiced `E-CEL-PARSE` message contract, exercised
//! through the full `check()` pipeline (a slot's failed CEL fragment must
//! never surface the embedded backend parser's own vocabulary — dsl 0.4 §8.1).
//! `Translation`/`translate_cel_parse` are `pub(crate)` to `lute-check`, so
//! every assertion here goes through the public `check()` entry point, the
//! same surface the CLI/LSP consume.
use lute_check::{check, CheckInput, CheckResult, Mode, SchemaImports};
use lute_core_span::Diagnostic;
use lute_manifest::provider::ProviderSet;

const FM: &str = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n";
const FM_STR: &str = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  run.s: { type: string, default: \"\" }\n---\n";

fn run(text: &str) -> CheckResult {
    let input = CheckInput {
        text: text.to_string(),
        uri: "cel_message".into(),
        snapshot: lute_manifest::core::load_core_snapshot(),
        providers: ProviderSet::default(),
        mode: Mode::Author,
        imports: SchemaImports::default(),
        components: Default::default(),
    };
    check(&input)
}

/// One `<branch>` with a single `when="<raw>"` guard choice, plus one
/// unguarded exit choice (keeps `E-BRANCH-ALL-GUARDED` out of the way so
/// every assertion below only has to reason about the guard slot itself).
fn doc_with_when(raw: &str) -> String {
    format!(
        "{FM}## Shot 1.\n\
         <branch id=\"b\">\n\
         <choice id=\"a\" label=\"A\" when=\"{raw}\">\n\
         @narrator: a.\n\
         </choice>\n\
         <choice id=\"leave\" label=\"Leave\" exit>\n\
         @narrator: bye.\n\
         </choice>\n\
         </branch>\n"
    )
}

fn diagnostics_for(raw: &str) -> (String, Vec<Diagnostic>) {
    let text = doc_with_when(raw);
    let diags = run(&text).diagnostics;
    (text, diags)
}

fn messages_for(raw: &str) -> Vec<String> {
    diagnostics_for(raw).1.into_iter().map(|d| d.message).collect()
}

fn cel_parse_diag(raw: &str) -> Diagnostic {
    let (_, diags) = diagnostics_for(raw);
    diags
        .into_iter()
        .find(|d| d.code == "E-CEL-PARSE")
        .unwrap_or_else(|| panic!("expected E-CEL-PARSE for {raw:?}"))
}

#[test]
fn assignment_eq_gets_suggestion() {
    let (text, diags) = diagnostics_for("run.act = 1");
    let d = diags
        .iter()
        .find(|d| d.code == "E-CEL-PARSE")
        .unwrap_or_else(|| panic!("expected E-CEL-PARSE; got {diags:?}"));
    assert!(
        d.message.contains("did you mean `run.act == 1`"),
        "{}",
        d.message
    );
    assert_eq!(d.fixits.len(), 1, "expected exactly one fixit; got {:?}", d.fixits);
    let edit = &d.fixits[0].edit[0];
    let mut spliced = text.clone();
    spliced.replace_range(edit.span.byte_start..edit.span.byte_end, &edit.new_text);
    assert!(
        spliced.contains("run.act == 1"),
        "splicing the fixit must produce `run.act == 1`; got: {spliced}"
    );
}

#[test]
fn all_t2_rows_translate() {
    // (authored, substring every message essence must contain)
    let rows: &[(&str, &str)] = &[
        ("a & b", "&&"),
        ("a | b", "||"),
        ("a and b", "&&"),
        ("a or b", "||"),
        ("not a", "!"),
        ("x =< 1", "<="),
        ("x => 1", ">="),
        ("'unterminated", "unclosed quote"),
    ];
    for (raw, essence) in rows {
        let d = cel_parse_diag(raw);
        assert!(
            d.message.contains(essence),
            "{raw:?}: expected message to contain {essence:?}; got {:?}",
            d.message
        );
    }
}

#[test]
fn no_backend_vocabulary_ever() {
    const LEAKS: &[&str] = &[
        "viable alternative",
        "token recognition",
        "mismatched input",
        "extraneous input",
        "no viable",
    ];
    for bad in ["run.act = 1", "a &", "(", "1 +", "'unterminated", "a and b", "@"] {
        let msgs = messages_for(bad);
        for m in &msgs {
            for tok in LEAKS {
                assert!(!m.contains(tok), "backend leak for {bad:?}: {m}");
            }
        }
    }
}

#[test]
fn c2_parse_failure_suppresses_slot() {
    // `run.nope` is UNDECLARED — if the slot parsed, it would also trip
    // E-UNDECLARED (and possibly E-CEL-PROFILE); the parse failure must
    // suppress every downstream per-slot analysis (dsl §8.2 C2).
    let (_, diags) = diagnostics_for("run.nope = 1");
    let codes: Vec<&str> = diags.iter().map(|d| d.code.as_str()).collect();
    assert!(codes.contains(&"E-CEL-PARSE"), "expected E-CEL-PARSE; got {codes:?}");
    assert!(
        !codes.contains(&"E-UNDECLARED"),
        "C2 must suppress E-UNDECLARED for a slot that failed to parse; got {codes:?}"
    );
    assert!(
        !codes.contains(&"E-CEL-PROFILE"),
        "C2 must suppress E-CEL-PROFILE for a slot that failed to parse; got {codes:?}"
    );
}

#[test]
fn fallback_is_neutral() {
    // "(" matches none of the six T2 rules (no quote/eq/logical/word-op
    // trigger) — it must fall to the neutral T3 form.
    let d = cel_parse_diag("(");
    assert!(
        d.message.contains("not a valid condition expression"),
        "{}",
        d.message
    );
    assert!(d.fixits.is_empty(), "the T3 fallback carries no fixit");
}

#[test]
fn masked_operators_stay_silent() {
    // The string literal's CONTENT contains `=` and `&`, but it is a
    // well-formed comparison — the backend parses it fine, so no
    // `translate_cel_parse` scan ever runs and no E-CEL-PARSE appears.
    let text = format!(
        "{FM_STR}## Shot 1.\n<branch id=\"b\">\n\
         <choice id=\"a\" label=\"A\" when=\"run.s == 'a = b & c'\">\n\
         @narrator: a.\n\
         </choice>\n\
         <choice id=\"leave\" label=\"Leave\" exit>\n\
         @narrator: bye.\n\
         </choice>\n\
         </branch>\n"
    );
    let diags = run(&text).diagnostics;
    let codes: Vec<&str> = diags.iter().map(|d| d.code.as_str()).collect();
    assert!(
        !codes.contains(&"E-CEL-PARSE"),
        "a valid condition whose string content contains operator bytes must not \
         trip E-CEL-PARSE; got {codes:?}"
    );
}

/// T13 adaptation (flagged in the task report): the plan names ONE `E-CEL-
/// PARSE` construction site, but `validate_components` builds a SECOND one
/// for a component body's own CEL slots (re-anchored to the scene frontmatter
/// span, dsl §13). Both must translate — a component-body typo is exactly as
/// writer-facing as a scene-body one — and a component-body fixit's edit span
/// (in the COMPONENT file's own byte-space) must NOT ride along once
/// re-anchored to the scene's span, since it cannot be spliced into the
/// scene's text.
#[test]
fn component_body_cel_parse_is_translated_and_reanchored() {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("lute_cel_message_component_{nanos}"));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("reaction.lute"),
        "---\ncomponent: reaction\nparams:\n  tier: { enum: [cold, warm, fond] }\n---\n\
         ## Scene 1.\n\
         <match on=\"@tier\">\n\
         <when test=\"run.act = 1\">\n@narrator: hi\n</when>\n\
         <otherwise>\n@narrator: bye\n</otherwise>\n\
         </match>\n",
    )
    .unwrap();
    let scene = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\ncomponents: [reaction.lute]\n---\n\
         ## Shot 1.\n::use{component=\"reaction\" tier=\"fond\"}\n";
    let (doc, _) = lute_syntax::parse(scene);
    let (meta0, _) = lute_check::parse_meta(&doc.meta, &lute_manifest::snapshot::CapabilitySnapshot::default());
    let components = lute_check::resolve_components(&dir, &meta0.components, doc.meta.span);
    let input = CheckInput {
        text: scene.to_string(),
        uri: "scene".into(),
        snapshot: lute_manifest::core::load_core_snapshot(),
        providers: ProviderSet::default(),
        mode: Mode::Ci,
        imports: SchemaImports::default(),
        components,
    };
    let diags = check(&input).diagnostics;
    let d = diags
        .iter()
        .find(|d| d.code == "E-CEL-PARSE")
        .unwrap_or_else(|| panic!("expected E-CEL-PARSE from the component body; got {diags:?}"));
    assert!(
        d.message.contains("did you mean `run.act == 1`"),
        "component-body translation must reuse the same T2 rule; got {}",
        d.message
    );
    assert!(
        d.fixits.is_empty(),
        "a re-anchored component-body diagnostic must not carry a fixit whose edit span \
         belongs to a different document; got {:?}",
        d.fixits
    );
    assert_eq!(
        (d.span.byte_start, d.span.byte_end),
        (doc.meta.span.byte_start, doc.meta.span.byte_end),
        "a component-body diagnostic is re-anchored to the scene frontmatter span's byte range"
    );
}
