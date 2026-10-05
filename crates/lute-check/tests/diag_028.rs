//! 0.28.0 diagnostics quality: one report per mistake. A beat the per-file
//! check rejected stays out of the project-wide tie and shadow passes, and a
//! tie explains itself only from what the author wrote (`when`, `once`,
//! `after:`), never from the facts a beat's own body asserts.

use std::path::PathBuf;

use lute_check::beats::ReportedErrors;
use lute_check::{check_project_beats, fold_env, CheckInput, FoldedEnv, Mode, ProjectDoc, SchemaImports};
use lute_core_span::Diagnostic;
use lute_syntax::ast::Document;

fn input(text: &str) -> CheckInput {
    CheckInput {
        text: text.to_string(),
        uri: "diag028".into(),
        snapshot: lute_manifest::core::load_core_snapshot(),
        providers: Default::default(),
        mode: Mode::Author,
        imports: SchemaImports::default(),
        components: Default::default(),
        defaults: Default::default(),
    }
}

const VOCAB: &str = "relations:\n  knows: { args: [person], tier: run }\n\
                     entities:\n  person: { members: [vesna] }\n\
                     state:\n  run.day: { type: int, default: 0 }\n";

/// `(path, text)` documents → the project beat diagnostics with `code`.
fn project(texts: &[&str], errors: &ReportedErrors, code: &str) -> Vec<Diagnostic> {
    let mut docs: Vec<(PathBuf, Document)> = Vec::new();
    let mut foldeds: Vec<FoldedEnv> = Vec::new();
    for (i, text) in texts.iter().enumerate() {
        let input = input(text);
        let (doc, _) = lute_syntax::parse(&input.text);
        foldeds.push(fold_env(&doc, &input).0);
        docs.push((PathBuf::from(format!("{i}.lute")), doc));
    }
    let refs: Vec<&FoldedEnv> = foldeds.iter().collect();
    let views: Vec<_> = docs
        .iter()
        .zip(&foldeds)
        .map(|((path, doc), folded)| ProjectDoc::new(path, doc, &folded.typed))
        .collect();
    let producers = lute_check::cast::fact_producers(&views, &Default::default());
    check_project_beats(&views, &refs, &producers, None, errors)
        .into_iter()
        .map(|(_, d)| d)
        .filter(|d| d.code == code)
        .collect()
}

fn lore(body: &str) -> String {
    format!("---\nkind: lore\nid: barks\n{VOCAB}---\n{body}")
}

fn entry(id: &str, when: &str) -> String {
    format!("<entry id=\"{id}\" on=\"talk\" when=\"{when}\">\n@narrator: {id}.\n</entry>\n")
}

#[test]
fn a_beat_with_an_error_ties_with_nothing() {
    let text = lore(&[entry("one", "run.day >= 2"), entry("two", "run.day >= 3")].concat());
    let tied = project(&[&text], &ReportedErrors::new(), "W-BEAT-PRIORITY-TIE");
    assert_eq!(tied.len(), 1, "the control ties: {tied:?}");
    // An error anywhere inside entry `two` (here its `when` value).
    let at = text.find("run.day >= 3").unwrap();
    let errors = ReportedErrors::from([(PathBuf::from("0.lute"), vec![at..at + 12])]);
    let tied = project(&[&text], &errors, "W-BEAT-PRIORITY-TIE");
    assert!(tied.is_empty(), "{tied:?}");
    // An error in another document leaves this one's tie standing.
    let errors = ReportedErrors::from([(PathBuf::from("1.lute"), vec![at..at + 12])]);
    assert_eq!(project(&[&text], &errors, "W-BEAT-PRIORITY-TIE").len(), 1);
}

#[test]
fn a_beat_with_an_error_shadows_nothing() {
    let text = lore(
        &[
            "<entry id=\"always\" on=\"talk\">\n@narrator: Hi.\n</entry>\n".to_string(),
            entry("later", "run.day >= 3"),
        ]
        .concat(),
    );
    assert_eq!(
        project(&[&text], &ReportedErrors::new(), "W-BEAT-SHADOWED").len(),
        1,
        "the control is shadowed"
    );
    let at = text.find("id=\"always\"").unwrap();
    let errors = ReportedErrors::from([(PathBuf::from("0.lute"), vec![at..at + 2])]);
    assert!(project(&[&text], &errors, "W-BEAT-SHADOWED").is_empty());
}

#[test]
fn a_tie_is_explained_by_what_the_author_wrote() {
    // `cafe.ok` has no `when`; its body asserts `knows(vesna)`. The tie is
    // real, but the reason is its missing `when`, not a read of its own fact.
    let ok = format!(
        "---\nkind: scene\nid: cafe.ok\non: talk\n{VOCAB}---\n## Ok\n@narrator: Hi.\n\
         ::assert{{knows(vesna)}}\n"
    );
    let other = format!(
        "---\nkind: scene\nid: cafe.two\non: talk\nwhen: \"run.day >= 1\"\n{VOCAB}---\n\
         ## Two\n@narrator: Two.\n"
    );
    let tied = project(
        &[&ok, &other],
        &ReportedErrors::new(),
        "W-BEAT-PRIORITY-TIE",
    );
    assert_eq!(tied.len(), 1, "{tied:?}");
    let m = &tied[0].message;
    assert!(m.contains("scene `cafe.ok` has no `when`"), "{m}");
    assert!(!m.contains("holds(knows(vesna))"), "{m}");
}

/// T3-31: the `once` periods a `share` message names are the ones the
/// checker accepts — `week` and `season:<name>` included.
#[test]
fn share_without_once_names_every_spending_period() {
    for period in lute_check::beats::SPENDING_ONCE {
        let raw = period.replace("<name>", "fair");
        assert!(
            lute_check::BeatOnce::parse(&raw).is_some(),
            "`{period}` is listed but not accepted"
        );
    }
    let text = format!(
        "---\nkind: scene\nid: a.one\non: talk\nshare: news\n{VOCAB}---\n## A\n@narrator: A.\n"
    );
    let out = lute_check::check(&input(&text));
    let d = out
        .diagnostics
        .iter()
        .find(|d| d.message.contains("`share` `news` without `once`"))
        .unwrap_or_else(|| panic!("{:?}", out.diagnostics));
    for period in ["`week`", "`season:<name>`", "`day`", "`user`"] {
        assert!(d.message.contains(period), "{}", d.message);
    }
}

/// T3-55: an objective takes one target; handed a kind it says so instead
/// of quoting a grammar.
#[test]
fn an_objective_handed_a_kind_target_says_objectives_take_one() {
    let text = format!(
        "---\nkind: quest\n{VOCAB}  run.done: {{ type: bool, default: false }}\n---\n\
         <quest id=\"q\" title=\"Q\">\n\
         <objective id=\"o\" title=\"O\" done=\"run.done\" on=\"talk\" target=\"kind:person\"/>\n\
         </quest>\n"
    );
    let out = lute_check::check(&input(&text));
    let d = out
        .diagnostics
        .iter()
        .find(|d| d.message.contains("target=\"kind:person\""))
        .unwrap_or_else(|| panic!("{:?}", out.diagnostics));
    assert!(d.message.contains("takes one target"), "{}", d.message);
    assert!(
        d.message.contains("kind targets are for beats and entries"),
        "{}",
        d.message
    );
    assert!(!d.message.contains("Segment"), "{}", d.message);
}

// --- T3-1: one cause, one report ---------------------------------------------

fn codes(text: &str, imports: SchemaImports) -> Vec<String> {
    let mut input = input(text);
    input.imports = imports;
    lute_check::check(&input)
        .diagnostics
        .into_iter()
        .map(|d| d.code)
        .collect()
}

/// `world.schema.yaml` written to a fresh directory, resolved as `uses:`.
fn schema_imports(tag: &str, schema: &str) -> SchemaImports {
    let dir = std::env::temp_dir().join(format!("lute_d028_{tag}_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("world.schema.yaml"), schema).unwrap();
    let zero = lute_core_span::Span {
        byte_start: 0,
        byte_end: 0,
        line: 1,
        column: 1,
        utf16_range: (0, 0),
    };
    let imports = lute_check::resolve_imports(&dir, &["world.schema.yaml".into()], &[], zero);
    let _ = std::fs::remove_dir_all(&dir);
    imports
}

const NIGHT: &str = "---\nkind: lore\nid: night\n---\n\
                     <entry id=\"door\" on=\"talk\" once=\"day\" when=\"clock.weekday == 1 && run.hour > 0\">\n\
                     @nell: The door is locked.\n\
                     @nell: It is hour {{run.hour}}.\n\
                     </entry>\n";

/// A schema whose YAML does not parse gives its importer nothing to judge
/// against: the import error is the one report, not every speaker, clock
/// period and path the schema would have declared.
#[test]
fn an_unparsable_schema_is_its_importers_only_report() {
    let schema = "cast:\n  nell: { name: Nell }\n\
                  state:\n  run.hour: { type: int, default: 0 }\n\
                  clock:\n  day: run.hour\n  day: 1\n";
    assert_eq!(
        codes(NIGHT, schema_imports("yaml", schema)),
        ["E-USES-PARSE"]
    );
}

/// A rejected `clock:` leaves the project clockless because of that one
/// error: `once="day"` and `clock.*` reads are not reported against it.
#[test]
fn a_rejected_clock_is_not_reported_again_where_a_clock_is_needed() {
    let schema = "cast:\n  nell: { name: Nell }\n\
                  state:\n  run.hour: { type: int, default: 0 }\n\
                  clock:\n  day: run.hour\n  week: { length: 2, labels: { 0: Work, 1: Rest } }\n";
    assert_eq!(
        codes(NIGHT, schema_imports("clock", schema)),
        ["E-USES-PARSE"]
    );
}

/// A state row that was reported (a `default:` outside its enum, an unknown
/// row key, a refused member) is the cause: its reads are not also unset,
/// its `<match>` not also uncovered, its literals not judged again.
#[test]
fn a_reported_state_row_is_not_judged_again_at_its_reads() {
    let text = "---\nkind: scene\nid: a\nstate:\n\
                \x20 run.route: { type: { enum: [none, ren, mika] }, default: rne }\n\
                \x20 run.gold: { type: int, defualt: 0 }\n\
                \x20 run.lock: { type: { enum: [open, unset] }, default: open }\n\
                ---\n## One\n\
                <match on=\"run.route\">\n<when is=\"ren\">\n@narrator: Ren.\n</when>\n\
                <when is=\"mkia\">\n@narrator: Mika.\n</when>\n</match>\n\
                <match on=\"run.lock\">\n<when is=\"open\">\n@narrator: Open.\n</when>\n</match>\n\
                ::set{run.gold = run.gold + 1}\n\
                @narrator: {{run.gold}} and {{run.route}}.\n";
    let mut got = codes(text, SchemaImports::default());
    got.sort();
    assert_eq!(got, ["E-RESERVED-NAME", "E-STATE-DECL", "E-STATE-DECL"]);
}

/// A match that misses a member but has an `unset` arm is uncovered once;
/// its subject read is still guarded against absence.
#[test]
fn an_unset_arm_guards_the_read_of_an_uncovered_match() {
    let text = "---\nkind: scene\nid: a\nstate:\n\
                \x20 run.killedBy: { type: { enum: [eel, choir, archivist] } }\n\
                ---\n## Rail\n\
                <match on=\"run.killedBy\">\n<when is=\"eel\">\n@narrator: Teeth.\n</when>\n\
                <when is=\"unset\">\n@narrator: Nobody knows.\n</when>\n</match>\n";
    let mut got = codes(text, SchemaImports::default());
    got.sort();
    assert_eq!(got, ["E-NONEXHAUSTIVE"]);
}

/// Content above the first `##` is one region, reported once over its
/// whole extent — not once per line, nor once per close tag in it.
#[test]
fn content_before_the_first_shot_is_one_report() {
    let text = "---\nkind: scene\nid: probe\n---\n\n\
                <match on=\"run.route\">\n  <when is=\"ren\">\n    @narrator: Ren.\n  </when>\n\
                \x20 <when is=\"mika\">\n    @narrator: Mika.\n  </when>\n</match>\n\n\
                ## The jetty\n\n@narrator: Hello.\n";
    let out = lute_check::check(&input(text));
    let region: Vec<_> = out
        .diagnostics
        .iter()
        .filter(|d| {
            d.code == "E-CONTENT-OUTSIDE-SHOT"
                || d.code == "E-UNCLOSED-TAG"
                || d.code == "E-UNCLASSIFIED"
        })
        .collect();
    assert_eq!(region.len(), 1, "{:?}", out.diagnostics);
    assert_eq!(region[0].code, "E-CONTENT-OUTSIDE-SHOT");
    assert!(
        region[0].message.ends_with("(this covers lines 6–13)"),
        "{}",
        region[0].message
    );
}

/// On one position the cause prints before what it causes, whatever the
/// codes' spelling order.
#[test]
fn on_one_position_the_cause_comes_first() {
    let at = |code: &str| Diagnostic {
        code: code.to_string(),
        severity: lute_core_span::Severity::Error,
        message: String::new(),
        evidence: None,
        span: lute_core_span::Span {
            byte_start: 10,
            byte_end: 12,
            line: 1,
            column: 11,
            utf16_range: (10, 12),
        },
        layer: lute_core_span::Layer::Cel,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    };
    let mut ds = vec![
        at("E-ARM-DEAD"),
        at("E-BEAT-UNREACHABLE"),
        at("E-WHEN-LITERAL-DOMAIN"),
    ];
    ds.sort_by(lute_check::diagnostic_order);
    assert_eq!(ds[0].code, "E-WHEN-LITERAL-DOMAIN");
}

/// An `exit` written with a value that is no flag is taken as meant: its
/// `E-FLAG-VALUE` is the one report, not also `E-HUB-NO-EXIT` (TH28-4b).
#[test]
fn a_refused_exit_flag_is_one_report() {
    let text = "---\nkind: scene\nid: k\n---\n## K\n<hub id=\"ask\">\n\
                <choice id=\"oven\" label=\"Oven\">\n@cook: Cake.\n</choice>\n\
                <choice id=\"leave\" label=\"Go\" exit=\"yes\">\n@cook: Bye.\n</choice>\n</hub>\n";
    let codes: Vec<String> = lute_check::check(&input(text))
        .diagnostics
        .into_iter()
        .map(|d| d.code)
        .collect();
    assert_eq!(codes, ["E-FLAG-VALUE"]);
}

/// An objective's literal fault prints before the unsatisfiable objective
/// it makes, though the objective's report starts earlier on the line
/// (TH28-4c).
#[test]
fn a_literal_fault_prints_before_the_objective_it_kills() {
    let text = "---\nkind: quest\nid: spoon\nstate:\n  run.accused: { type: { enum: [nobody, cook] }, \
                default: nobody }\n---\n<quest id=\"spoon\" title=\"Find\" start=\"true\" tier=\"run\">\n  \
                <objective id=\"name\" title=\"Name\" done=\"run.accused == 'magpie'\"/>\n</quest>\n";
    let codes: Vec<String> = lute_check::check(&input(text))
        .diagnostics
        .into_iter()
        .map(|d| d.code)
        .collect();
    assert_eq!(
        codes,
        ["E-WHEN-LITERAL-DOMAIN", "E-OBJECTIVE-UNSATISFIABLE"]
    );
}

/// `{{a ? 'x' : 'y'}}` is told to write one line per case, spelled out — a
/// string `@def` is not shown either (ledger LG28-2).
#[test]
fn a_text_ternary_interpolation_is_told_to_split_into_lines() {
    let text = "---\nkind: scene\nid: t\nstate:\n  run.k: { type: bool, default: false }\n---\n\
                ## T\n@narrator: {{run.k ? 'Yes.' : 'No.'}}\n";
    let ds = lute_check::check(&input(text)).diagnostics;
    assert_eq!(ds.len(), 1, "{ds:#?}");
    assert!(
        ds[0]
            .message
            .contains("`@who{when=\"run.k\"}: Yes.` and `@who{when=\"!run.k\"}: No.`"),
        "{}",
        ds[0].message
    );
}

/// A `::set` the write policy refuses is reported once, by the refusal —
/// its value's type against the engine's record is moot (ledger INK-06).
#[test]
fn a_refused_write_is_one_report() {
    let text = "---\nkind: scene\nid: d\n---\n## D\n<branch id=\"door\">\n\
                <choice id=\"a\" label=\"A\">\n@narrator: A.\n</choice>\n\
                <choice id=\"b\" label=\"B\">\n@narrator: B.\n</choice>\n</branch>\n\
                ::set{scene.choices.door = true}\n";
    assert_eq!(
        codes(text, SchemaImports::default()),
        ["E-QUEST-RESERVED-WRITE"]
    );
}

/// Yarn's `$oil` is one report, naming the path — not also a stray match
/// subject; a bare `$` outside a `<match>` still is one (ledger INK-16).
#[test]
fn a_yarn_variable_is_one_report() {
    let text = "---\nkind: scene\nid: y\nstate:\n  run.oil: { type: int, default: 1 }\n---\n\
                ## Y\n@narrator{when=\"$oil > 2\"}: Lots.\n@narrator{when=\"$ > 2\"}: Some.\n";
    assert_eq!(
        codes(text, SchemaImports::default()),
        ["E-CEL-PROFILE", "E-DOLLAR-OUTSIDE-MATCH"]
    );
}

/// A choice named `true` is refused where it is named; the `is="true"`
/// that meant it is not reported again as foreign to a domain that lists
/// it (ledger INK-19).
#[test]
fn a_literal_choice_name_is_reported_where_it_is_named() {
    let text = "---\nkind: scene\nid: t\n---\n## T\n<branch id=\"b\">\n\
                <choice id=\"true\" label=\"Yes\">\n@narrator: Y.\n</choice>\n\
                <choice id=\"no\" label=\"No\">\n@narrator: N.\n</choice>\n</branch>\n\
                <match on=\"scene.choices.b\">\n<when is=\"true\">\n@narrator: yes\n</when>\n\
                <otherwise>\n@narrator: no\n</otherwise>\n</match>\n";
    assert_eq!(codes(text, SchemaImports::default()), ["E-RESERVED-NAME"]);
}

/// A scene with a `title:` and no `id:` is told the id its title spells,
/// at the title (ledger INK-17).
#[test]
fn a_titled_scene_without_an_id_is_offered_one() {
    let text = "---\nkind: scene\ntitle: Lamp_Room\n---\n## T\n@narrator: Hi.\n";
    let ds = lute_check::check(&input(text)).diagnostics;
    assert_eq!(ds.len(), 1, "{ds:#?}");
    assert_eq!(ds[0].code, "E-META-MISSING");
    assert_eq!(ds[0].span.line, 3);
    assert!(
        ds[0].message.contains("write `id: lampRoom`"),
        "{}",
        ds[0].message
    );
}
