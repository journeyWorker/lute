//! dsl 0.28.0 §1: every place that accepts a key refuses one it does not
//! define, at the key, and no author can declare or write a name the engine
//! owns. One mistake is one located report.
use lute_check::{check, CheckInput, Mode, SchemaImports};
use lute_core_span::{Diagnostic, Severity};
use lute_manifest::provider::ProviderSet;

fn diags(text: &str) -> Vec<Diagnostic> {
    check(&CheckInput {
        text: text.to_string(),
        uri: "strict_028".into(),
        snapshot: lute_manifest::core::load_core_snapshot(),
        providers: ProviderSet::default(),
        mode: Mode::Author,
        imports: SchemaImports::default(),
        components: Default::default(),
        defaults: Default::default(),
    })
    .diagnostics
}

/// `(line, column, message)` of every `code` diagnostic.
fn located(ds: &[Diagnostic], code: &str) -> Vec<(usize, usize, String)> {
    ds.iter()
        .filter(|d| d.code == code)
        .map(|d| {
            (
                d.span.line as usize,
                d.span.column as usize,
                d.message.clone(),
            )
        })
        .collect()
}

fn scene(frontmatter: &str, body: &str) -> String {
    format!("---\nkind: scene\nid: s\n{frontmatter}---\n## A\n{body}@narrator: a\n")
}

fn at<'d>(got: &'d [(usize, usize, String)], line: usize) -> &'d str {
    got.iter()
        .find(|(l, ..)| *l == line)
        .map(|(_, _, m)| m.as_str())
        .unwrap_or_else(|| panic!("nothing on line {line}: {got:#?}"))
}

/// A `state:` row key outside `type`/`default`/`owner`/`per` is refused at
/// the key — a near spelling gets its did-you-mean, and the two keys other
/// layers use (`reserved:`, `tier:`) name the state row's own spelling.
#[test]
fn state_row_unknown_keys_are_refused_at_the_key() {
    let text = scene(
        "state:\n  run.n: { type: int, defualt: 3 }\n  run.m:\n    type: bool\n    ownr: engine\n  \
         run.k: { type: bool, reserved: true }\n  run.t: { type: int, tier: user }\n",
        "",
    );
    let got = located(&diags(&text), "E-STATE-DECL");
    assert_eq!(got.len(), 4, "{got:#?}");
    // Each on the key's own line and column, not the frontmatter's 1:1.
    assert_eq!((got[0].0, got[0].1), (5, 23), "{got:#?}");
    assert!(at(&got, 5).contains("did you mean `default`?"), "{got:#?}");
    assert!(at(&got, 8).contains("did you mean `owner`?"), "{got:#?}");
    assert!(at(&got, 9).contains("`owner: engine`"), "{got:#?}");
    assert!(at(&got, 10).contains("first segment"), "{got:#?}");
}

/// An inline enum's `default:` must be one of its members.
#[test]
fn inline_enum_default_outside_its_members_is_refused() {
    let text = scene(
        "state:\n  run.mood: { type: { enum: [calm, tense] }, default: angry }\n",
        "",
    );
    let got = located(&diags(&text), "E-STATE-DECL");
    assert_eq!(got.len(), 1, "{got:#?}");
    assert!(got[0].2.contains("angry"), "{got:#?}");
    assert_eq!(got[0].0, 5, "{got:#?}");
}

/// The engine's namespaces cannot be declared as author state: each row is
/// one `E-STATE-NAMESPACE` at its path. `quest.<id>.<field>` stays legal.
#[test]
fn engine_namespaces_cannot_be_declared() {
    let text = scene(
        "state:\n  scene.choices.door: { type: string, default: \"\" }\n  \
         scene.visited.lamp: { type: bool, default: false }\n  \
         occasion.payload.seconds: { type: int, default: 0 }\n  \
         quest.notes: { type: int, default: 0 }\n  \
         season.summer: { type: bool, default: false }\n  \
         quest.q.count: { type: int, default: 0 }\n",
        "",
    );
    let got = located(&diags(&text), "E-STATE-NAMESPACE");
    let lines: Vec<usize> = got.iter().map(|(l, ..)| *l).collect();
    assert_eq!(lines, [5, 6, 7, 8, 9], "{got:#?}");
    assert!(got.iter().all(|(_, col, _)| *col == 3), "{got:#?}");
    assert!(
        at(&got, 8).contains("`quest.<questId>.<field>`"),
        "{got:#?}"
    );
    assert!(at(&got, 9).contains("declared in `seasons:`"), "{got:#?}");
}

/// Content cannot write what the engine records: a taken choice, a visit,
/// or anything of the raise a beat answers.
#[test]
fn content_cannot_write_engine_records() {
    for (set, needle) in [
        ("scene.choices.door = \"stay\"", "`scene.choices.<id>`"),
        (
            "scene.visited.lamp = false",
            "`scene.visited.<hub>.<choice>`",
        ),
        ("occasion.target = \"mira\"", "`occasion.*`"),
        (
            "occasion.payload.seconds = 2",
            "the payload comes from the raise",
        ),
    ] {
        let ds = diags(&scene("", &format!("::set{{{set}}}\n")));
        let got = located(&ds, "E-QUEST-RESERVED-WRITE");
        assert_eq!(got.len(), 1, "{set}: {ds:#?}");
        assert!(got[0].2.contains(needle), "{set}: {}", got[0].2);
        assert!(
            !ds.iter().any(|d| d.code == "E-UNDECLARED"),
            "{set}: one report, not an undeclared path too: {ds:#?}"
        );
    }
}

/// A frontmatter never closed is ONE error at the opener, naming where the
/// `---` belongs — not a denial of the keys written above (`E-KIND-MISSING`,
/// `E-META-MISSING`) nor its lines read as body text.
#[test]
fn unclosed_frontmatter_is_one_error() {
    let ds = diags("---\nkind: scene\nid: s\ntitle: Tea\n\n## A\n@narrator: a\n");
    let errors: Vec<&Diagnostic> = ds
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert_eq!(errors.len(), 1, "{errors:#?}");
    assert_eq!(errors[0].code, "E-META-PARSE");
    assert!(
        errors[0].message.contains("add a `---` line after line 4"),
        "{}",
        errors[0].message
    );
    assert_eq!(errors[0].fixits.len(), 1, "{errors:#?}");
}

/// The YAML run goes on past a blank line (`on:` below it is still a key,
/// not a content line missing its `@`), and a line of dashes that is not
/// exactly `---` is named as the fence it was meant to be.
#[test]
fn unclosed_frontmatter_spans_blank_lines_and_names_a_near_fence() {
    let one_error = |text: &str, needle: &str| {
        let ds = diags(text);
        let errors: Vec<&Diagnostic> = ds
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        assert_eq!(errors.len(), 1, "{errors:#?}");
        assert!(errors[0].message.contains(needle), "{}", errors[0].message);
    };
    one_error(
        "---\nkind: scene\nid: s\ntitle: Tea\n\non: chapter\n\n## A\n@narrator: a\n",
        "add a `---` line after line 6",
    );
    one_error(
        "---\nkind: scene\nid: s\ntitle: Tea\n--\n\n## A\n@narrator: a\n",
        "`--` on line 5 is not a closing fence",
    );
}

/// An unknown top-level frontmatter key sits at the key, not at `1:1`.
#[test]
fn unknown_frontmatter_key_is_located_at_the_key() {
    let ds = diags(&scene("title: Tea\nstrat: early\n", ""));
    let got = located(&ds, "E-META-UNKNOWN-KEY");
    assert_eq!(got.len(), 1, "{ds:#?}");
    assert_eq!((got[0].0, got[0].1), (5, 1), "{got:#?}");
    assert!(got[0].2.contains("`strat`"), "{got:#?}");
}

/// A reserved name is refused where it is declared, not at the first use.
#[test]
fn reserved_name_is_refused_at_its_declaration() {
    let ds = diags(&scene(
        "entities:\n  crew: { members: [mira, unset] }\n",
        "",
    ));
    let got = located(&ds, "E-RESERVED-NAME");
    assert_eq!(got.len(), 1, "{ds:#?}");
    assert_eq!((got[0].0, got[0].1), (5, 27), "{got:#?}");
    assert!(got[0].2.contains("`unset`"), "{got:#?}");
}
