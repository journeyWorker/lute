//! dsl 0.30.0 §2: a name is written bare when it is an identifier and quoted
//! as an index otherwise; the two spellings are one path. A name that is not
//! an identifier after a `.` is one `E-PATH-IDENT` naming the quoted form.
use lute_check::{check, CheckInput, Mode, SchemaImports};
use lute_core_span::{Diagnostic, Severity};
use lute_manifest::provider::ProviderSet;

const FM: &str = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  \
                  run.lab-b2: { type: int, default: 0 }\n  \
                  run.labB2: { type: int, default: 0 }\n---\n";

fn errors(body: &str) -> Vec<Diagnostic> {
    let input = CheckInput {
        text: format!("{FM}## Shot 1.\n{body}\n@narrator: end.\n"),
        uri: "paths_030".into(),
        snapshot: lute_manifest::core::load_core_snapshot(),
        providers: ProviderSet::default(),
        mode: Mode::Author,
        imports: SchemaImports::default(),
        components: Default::default(),
        defaults: Default::default(),
    };
    check(&input)
        .diagnostics
        .into_iter()
        .filter(|d| d.severity == Severity::Error)
        .collect()
}

fn guarded(cond: &str) -> String {
    format!("@narrator{{when=\"{cond}\"}}: hi.")
}

/// Bare and quoted spellings of one name read the same path: a quest id,
/// an objective id, a state path, single and double quotes, in a guard, a
/// `::set` target and an interpolation.
#[test]
fn bare_and_quoted_spellings_are_one_path() {
    for body in [
        guarded("quest.zeroCoke001.state == 'active'"),
        guarded("quest['zero-coke-001'].state == 'active'"),
        guarded("quest['q-1'].objectives['o-1'].done"),
        guarded("run['lab-b2'] > 0 && run.labB2 > 0 && run['labB2'] > 0"),
        guarded("'lab-b2' in run"),
        "::set{run[\"lab-b2\"] += 1}".to_string(),
        "::set{run['labB2'] = 2}".to_string(),
        "@narrator: {{run[\"lab-b2\"]}} and {{run.labB2}}.".to_string(),
    ] {
        let errs = errors(&body);
        assert!(errs.is_empty(), "{body}: {errs:?}");
    }
}

/// A non-identifier name after a `.` — in a guard (whether CEL parses it as
/// a subtraction or not at all), a `::set` target or an interpolation — is
/// one `E-PATH-IDENT` that names the quoted spelling.
#[test]
fn a_dotted_non_identifier_is_one_path_ident_naming_the_quoted_form() {
    for (body, quoted) in [
        (
            guarded("quest.zero-coke-001.state == 'active'"),
            "quest[\"zero-coke-001\"].state",
        ),
        (guarded("run.lab-b2 > 0"), "run[\"lab-b2\"]"),
        (guarded("run.visits.001 > 0"), "run.visits[\"001\"]"),
        ("::set{run.lab-b2 += 1}".to_string(), "run[\"lab-b2\"]"),
        ("@narrator: {{run.lab-b2}}.".to_string(), "run[\"lab-b2\"]"),
    ] {
        let errs = errors(&body);
        assert_eq!(errs.len(), 1, "{body}: {errs:?}");
        assert_eq!(errs[0].code, "E-PATH-IDENT", "{body}");
        assert!(
            errs[0].message.contains(&format!("write `{quoted}`")),
            "{body}: {}",
            errs[0].message
        );
    }
}

/// A subtraction written without spaces stays a subtraction.
#[test]
fn a_numeric_subtraction_is_not_a_name() {
    let errs = errors(&guarded("run.labB2-1 > 0"));
    assert!(errs.is_empty(), "{errs:?}");
}

/// A quoted name is checked like a bare one: an undeclared member is
/// `E-UNDECLARED`, spelled as written.
#[test]
fn a_quoted_undeclared_path_is_undeclared() {
    let errs = errors(&guarded("run['lab-b3'] > 0"));
    assert_eq!(errs.len(), 1, "{errs:?}");
    assert_eq!(errs[0].code, "E-UNDECLARED");
}
