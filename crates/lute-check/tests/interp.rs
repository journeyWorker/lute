//! B3 — `{{…}}` interpolation referent validation (dsl §7.6). An interpolation
//! is a state READ: a `Path` gets the SAME cel-layer + definite-assignment
//! treatment as a `<when>` guard / `::set` RHS read (`E-UNDECLARED` /
//! `E-MAYBE-UNSET`), a `Ref` resolves against `defs:` (`E-UNDECLARED-REF`) and
//! must produce a renderable type (number/bool/enum → else `E-REF-TYPE`), and the
//! reserved `userName` token is always ok. Fed through the assembled `check()`
//! over inline `state:`/`defs:` frontmatter (mirrors `ref_type.rs`'s harness).
use lute_check::{check, CheckInput, Mode, SchemaImports};
use lute_manifest::provider::ProviderSet;

const HDR: &str = "---\ncharacter: x\nseason: 1\nepisode: 1\n";

fn codes(text: &str) -> Vec<String> {
    let input = CheckInput {
        text: text.to_string(),
        uri: "interp".into(),
        snapshot: lute_manifest::core::load_core_snapshot(),
        providers: ProviderSet::default(),
        mode: Mode::Author,
        imports: SchemaImports::default(),
        components: Default::default(),
    };
    check(&input)
        .diagnostics
        .into_iter()
        .map(|d| d.code)
        .collect()
}

// `{{run.ghost}}` — `run.ghost` is not a declared state path ⇒ E-UNDECLARED
// (the cel-layer read-check owns undeclared reads, not E-MAYBE-UNSET).
#[test]
fn interp_undeclared_path() {
    let t = format!(
        "{HDR}state:\n  app.lang: {{ type: string, default: en }}\n---\n## Shot 1.\n\
         :bianca: I sense a {{{{run.ghost}}}}\n"
    );
    let c = codes(&t);
    assert!(c.contains(&"E-UNDECLARED".to_string()), "got {c:?}");
}

// `{{run.x}}` — declared (number, no default), no dominating `::set`, no guard ⇒
// E-MAYBE-UNSET (definite-assignment), NOT E-UNDECLARED (it IS declared).
#[test]
fn interp_maybe_unset_path() {
    let t = format!(
        "{HDR}state:\n  run.x: {{ type: number }}\n---\n## Shot 1.\n\
         :bianca: you have {{{{run.x}}}}\n"
    );
    let c = codes(&t);
    assert!(c.contains(&"E-MAYBE-UNSET".to_string()), "got {c:?}");
    assert!(!c.contains(&"E-UNDECLARED".to_string()), "declared ⇒ no E-UNDECLARED; got {c:?}");
}

// `{{@nope}}` — `@nope` is not a declared `defs:` entry ⇒ E-UNDECLARED-REF.
#[test]
fn interp_undeclared_ref() {
    let t = format!(
        "{HDR}state:\n  app.lang: {{ type: string, default: en }}\n---\n## Shot 1.\n\
         :bianca: {{{{@nope}}}}\n"
    );
    let c = codes(&t);
    assert!(c.contains(&"E-UNDECLARED-REF".to_string()), "got {c:?}");
}

// `{{userName}}` — the reserved token always renders ⇒ no interp diagnostic.
#[test]
fn interp_username_ok() {
    let t = format!("{HDR}---\n## Shot 1.\n:bianca: hello {{{{userName}}}}\n");
    let c = codes(&t);
    for code in ["E-UNDECLARED", "E-MAYBE-UNSET", "E-UNDECLARED-REF", "E-REF-TYPE"] {
        assert!(!c.contains(&code.to_string()), "{code} unexpected; got {c:?}");
    }
}

// `{{app.lang}}` — declared with a default ⇒ definitely assigned ⇒ no diagnostic.
#[test]
fn interp_declared_path_ok() {
    let t = format!(
        "{HDR}state:\n  app.lang: {{ type: string, default: en }}\n---\n## Shot 1.\n\
         :bianca: language is {{{{app.lang}}}}\n"
    );
    let c = codes(&t);
    for code in ["E-UNDECLARED", "E-MAYBE-UNSET", "E-UNDECLARED-REF", "E-REF-TYPE"] {
        assert!(!c.contains(&code.to_string()), "{code} unexpected; got {c:?}");
    }
}

// §7.6 rendering: an interpolated `@ref` MUST resolve to a renderable type
// (number/bool/enum). A `str` def is not renderable ⇒ E-REF-TYPE.
#[test]
fn interp_ref_nonrenderable_type() {
    let t = format!(
        "{HDR}defs:\n  greeting: {{ type: string, cel: \"'hi'\" }}\n---\n## Shot 1.\n\
         :bianca: {{{{@greeting}}}}\n"
    );
    let c = codes(&t);
    assert!(c.contains(&"E-REF-TYPE".to_string()), "str def in interp must flag E-REF-TYPE; got {c:?}");
}

// A renderable `@ref` (number) interpolates cleanly ⇒ no ref diagnostic.
#[test]
fn interp_ref_renderable_ok() {
    let t = format!(
        "{HDR}defs:\n  coins: {{ type: number, cel: \"1\" }}\n---\n## Shot 1.\n\
         :bianca: you have {{{{@coins}}}} coins\n"
    );
    let c = codes(&t);
    for code in ["E-UNDECLARED-REF", "E-REF-TYPE", "E-MAYBE-UNSET", "E-UNDECLARED"] {
        assert!(!c.contains(&code.to_string()), "{code} unexpected; got {c:?}");
    }
}

// `$` (the match subject) is legal ONLY in a `<when test>` (dsl §8.2), never in a
// content interpolation — even one nested inside a `<match>` arm (§7.6 admits only
// Path/Ref/userName). The parser classifies `{{$}}` as a `Path` raw `"$"`; it must
// still be rejected regardless of the enclosing arm scope ⇒ E-DOLLAR-OUTSIDE-MATCH.
#[test]
fn interp_dollar_in_match_arm_rejected() {
    let t = format!(
        "{HDR}state:\n  scene.n: {{ type: number, default: 0 }}\n---\n## Shot 1.\n\
         <match on=\"scene.n\">\n\
         <when test=\"scene.n > 0\">\n:bianca: value {{{{$}}}}\n</when>\n\
         <otherwise>\n:bianca: none\n</otherwise>\n\
         </match>\n"
    );
    let c = codes(&t);
    assert!(
        c.contains(&"E-DOLLAR-OUTSIDE-MATCH".to_string()),
        "`$` in a content interpolation is out of match scope (dsl §8.2); got {c:?}"
    );
}
