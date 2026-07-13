//! `--mock` surface + STRUCTURAL pre-walk validation (dsl 0.4.0 §4.3, Task
//! 18): the four Appendix A trace fixtures verbatim, a `derive:true`/
//! `reserved:true` relation fact accepted (D18), YAML/flag merge semantics
//! (facts union / choose replace-per-id / flag-state-wins-per-path), and the
//! structural-only pin — a forced choice whose guard would decide false
//! still passes THIS pre-walk pass (eligibility is Task 19's job, §4.4).
//!
//! The Appendix A + derived-fact fixtures resolve the REAL shipped
//! `docs/examples/choice-persist.lute` / `quest-rescue-halsin.lute` (the
//! worked examples the spec itself cites), assembled via the same
//! `resolve_imports`/`fold_env` pipeline `lute check`/`lute compile`'s test
//! suites use (`lute-compile/tests/e2e.rs`'s `input_for` idiom).

use std::collections::BTreeMap;
use std::path::Path;

use lute_check::{CheckInput, FoldedEnv, Mode};
use lute_core_span::Span;
use lute_syntax::ast::Document;
use lute_trace::{
    merge, parse_mock_yaml, validate, MockSet, E_TRACE_ACCEPT, E_TRACE_CHOICE, E_TRACE_EVENT,
    E_TRACE_MOCK_FACT, E_TRACE_MOCK_TYPE, E_TRACE_MOCK_UNDECLARED,
};

fn zero_span() -> Span {
    Span { byte_start: 0, byte_end: 0, line: 0, column: 0, utf16_range: (0, 0) }
}

/// Assemble `(FoldedEnv, Document)` for `text` exactly as `lute check`/
/// `lute compile` do (no `--project`; `base` resolves any `uses:`/
/// `components:` relative paths). Asserts the fold itself is diagnostic-free
/// — every fixture this file drives is a document `validate`'s callers
/// expect to have already passed `check` (§4.3's own pipeline note).
fn folded_and_doc(text: &str, uri: &str, base: &Path) -> (FoldedEnv, Document) {
    let (doc, parse_diags) = lute_syntax::parse(text);
    assert!(parse_diags.is_empty(), "fixture must parse clean: {parse_diags:?}");
    let (meta0, _) = lute_check::parse_meta(&doc.meta, &lute_manifest::snapshot::CapabilitySnapshot::default());
    let (snapshot, _) =
        lute_manifest::project::resolve_document_snapshot(None, meta0.profile.as_deref(), &meta0.plugins);
    let imports = lute_check::resolve_imports(base, &meta0.uses, &meta0.extends, doc.meta.span);
    let components = lute_check::resolve_components(base, &meta0.components, doc.meta.span);
    let input = CheckInput {
        text: text.to_string(),
        uri: uri.to_string(),
        snapshot,
        providers: lute_manifest::provider::ProviderSet::default(),
        mode: Mode::Ci,
        imports,
        components,
    };
    let (folded, fd1, fd2) = lute_check::fold_env(&doc, &input);
    assert!(fd1.is_empty() && fd2.is_empty(), "fixture must fold clean: {fd1:?} {fd2:?}");
    (folded, doc)
}

/// Load and fold a real `docs/examples/*.lute` fixture by path (relative to
/// this crate's `Cargo.toml`, the house `../../docs/examples/…` idiom).
fn load(path: &str) -> (FoldedEnv, Document) {
    let file = Path::new(path);
    let text = std::fs::read_to_string(file).unwrap_or_else(|e| panic!("read {path}: {e}"));
    let base = file.parent().unwrap_or_else(|| Path::new("."));
    folded_and_doc(&text, path, base)
}

fn codes(diags: &[lute_core_span::Diagnostic]) -> Vec<&str> {
    diags.iter().map(|d| d.code.as_str()).collect()
}

// ---------------------------------------------------------------------
// Appendix A, verbatim (dsl 0.4.0 §12 table).
// ---------------------------------------------------------------------

/// `--state run.metHelpfuly=true` (typo, missing the second `l`) against the
/// `choice-persist` schema -> `E-TRACE-MOCK-UNDECLARED`.
#[test]
fn appendix_a_state_typo_is_undeclared() {
    let (folded, doc) = load("../../docs/examples/choice-persist.lute");
    let mocks = MockSet {
        state: vec![("run.metHelpfuly".to_string(), "true".to_string(), zero_span())],
        ..Default::default()
    };
    let diags = validate(&mocks, &folded, &doc);
    assert_eq!(codes(&diags), vec![E_TRACE_MOCK_UNDECLARED], "{diags:?}");
}

/// `--state run.tip=warm` against `run.tip: { type: number }` ->
/// `E-TRACE-MOCK-TYPE`.
#[test]
fn appendix_a_state_wrong_type_is_type_error() {
    let (folded, doc) = load("../../docs/examples/choice-persist.lute");
    let mocks = MockSet {
        state: vec![("run.tip".to_string(), "warm".to_string(), zero_span())],
        ..Default::default()
    };
    let diags = validate(&mocks, &folded, &doc);
    assert_eq!(codes(&diags), vec![E_TRACE_MOCK_TYPE], "{diags:?}");
}

/// `--fact "inParty(sofia, grove)"` against unary `inParty: { args:
/// [character] }` (`docs/examples/act1.schema.yaml`, imported by
/// `quest-rescue-halsin.lute`) -> `E-TRACE-MOCK-FACT` (arity).
#[test]
fn appendix_a_fact_arity_mismatch_is_fact_error() {
    let (folded, doc) = load("../../docs/examples/quest-rescue-halsin.lute");
    let mocks = MockSet {
        facts: vec!["inParty(sofia, grove)".to_string()],
        ..Default::default()
    };
    let diags = validate(&mocks, &folded, &doc);
    assert_eq!(codes(&diags), vec![E_TRACE_MOCK_FACT], "{diags:?}");
}

/// `--choose sofaHelp=shrug` against the `sofaHelp` branch (choices `help`/
/// `warmly`/`tip`) -> `E-TRACE-CHOICE`.
#[test]
fn appendix_a_choose_unknown_choice_is_choice_error() {
    let (folded, doc) = load("../../docs/examples/choice-persist.lute");
    let mocks = MockSet {
        choose: BTreeMap::from([("sofaHelp".to_string(), vec!["shrug".to_string()])]),
        ..Default::default()
    };
    let diags = validate(&mocks, &folded, &doc);
    assert_eq!(codes(&diags), vec![E_TRACE_CHOICE], "{diags:?}");
}

/// `--choose` naming an unknown branch/hub id entirely (not merely an
/// unknown choice within a real one) is the same code, structurally.
#[test]
fn choose_unknown_branch_id_is_choice_error() {
    let (folded, doc) = load("../../docs/examples/choice-persist.lute");
    let mocks = MockSet {
        choose: BTreeMap::from([("noSuchBranch".to_string(), vec!["x".to_string()])]),
        ..Default::default()
    };
    let diags = validate(&mocks, &folded, &doc);
    assert_eq!(codes(&diags), vec![E_TRACE_CHOICE], "{diags:?}");
}

/// `--event questActive` names a built-in lifecycle event — engine-derived,
/// never user-fired via `--event` (§4.3/§4.4) -> `E-TRACE-EVENT`.
#[test]
fn appendix_a_event_lifecycle_name_is_event_error() {
    let (folded, doc) = load("../../docs/examples/quest-rescue-halsin.lute");
    let mocks = MockSet { events: vec!["questActive".to_string()], ..Default::default() };
    let diags = validate(&mocks, &folded, &doc);
    assert_eq!(codes(&diags), vec![E_TRACE_EVENT], "{diags:?}");
}

/// A non-lifecycle `--event` name (e.g. `npcSpoke`) passes clean — only the
/// three built-in lifecycle names are rejected.
#[test]
fn event_non_lifecycle_name_passes_structural_validation() {
    let (folded, doc) = load("../../docs/examples/quest-rescue-halsin.lute");
    let mocks = MockSet { events: vec!["npcSpoke".to_string()], ..Default::default() };
    let diags = validate(&mocks, &folded, &doc);
    assert!(diags.is_empty(), "{diags:?}");
}

/// `--accept rescueHalsin` against a quest that carries a `start` predicate
/// (declarative — activates on its own, needs no accept) -> `E-TRACE-ACCEPT`.
#[test]
fn appendix_a_accept_on_start_having_quest_is_accept_error() {
    let (folded, doc) = load("../../docs/examples/quest-rescue-halsin.lute");
    let mocks = MockSet { accepts: vec!["rescueHalsin".to_string()], ..Default::default() };
    let diags = validate(&mocks, &folded, &doc);
    assert_eq!(codes(&diags), vec![E_TRACE_ACCEPT], "{diags:?}");
}

/// `--accept noSuchQuest` names a quest id absent from the document ->
/// `E-TRACE-ACCEPT`.
#[test]
fn accept_unknown_quest_id_is_accept_error() {
    let (folded, doc) = load("../../docs/examples/quest-rescue-halsin.lute");
    let mocks = MockSet { accepts: vec!["noSuchQuest".to_string()], ..Default::default() };
    let diags = validate(&mocks, &folded, &doc);
    assert_eq!(codes(&diags), vec![E_TRACE_ACCEPT], "{diags:?}");
}

// ---------------------------------------------------------------------
// D18: derive:true / reserved:true relation facts are LEGAL mocks.
// ---------------------------------------------------------------------

/// `believesLocation` is `derive: true` (`act1.schema.yaml`) — the exact
/// §4.6 worked-example mock (`--fact "believesLocation(player, halsin,
/// grove)"`). A mock is a supplied answer, not a content write: `check_atom`
/// alone (never the `::assert`/`::retract` write-policy layer) validates it,
/// so it is structurally clean.
#[test]
fn derived_relation_fact_is_a_legal_mock() {
    let (folded, doc) = load("../../docs/examples/quest-rescue-halsin.lute");
    let mocks = MockSet {
        facts: vec!["believesLocation(player, halsin, grove)".to_string()],
        ..Default::default()
    };
    let diags = validate(&mocks, &folded, &doc);
    assert!(
        diags.is_empty(),
        "a derive:true relation fact is a supplied answer, not a content write (D18, §4.3): {diags:?}"
    );
}

/// A `reserved: true` relation fact is likewise legal (§4.3's own text names
/// both). No shipped example declares one, so this pins it on a minimal
/// inline `relations:` schema (the `lute-check/tests/rel_schema.rs` inline-
/// decl idiom).
#[test]
fn reserved_relation_fact_is_a_legal_mock() {
    let text = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n\
                entities:\n  character: { members: [halsin] }\n\
                relations:\n  flagged: { args: [character], reserved: true }\n\
                ---\n## Shot 1.\n@narrator: hi\n";
    let (folded, doc) = folded_and_doc(text, "reserved-fact", Path::new("."));
    let mocks = MockSet {
        facts: vec!["flagged(halsin)".to_string()],
        ..Default::default()
    };
    let diags = validate(&mocks, &folded, &doc);
    assert!(
        diags.is_empty(),
        "a reserved:true relation fact is a supplied answer, not a content write (§4.3): {diags:?}"
    );
}

// ---------------------------------------------------------------------
// Structural-only pin (§4.3/§4.4): a forced choice's GUARD is never
// evaluated by pre-walk validation, even when it is provably false.
// ---------------------------------------------------------------------

/// `<choice when="1 > 2">` — a `decide()`-provably-false guard (the
/// `lute-check/tests/reachability.rs` idiom for a decidedly-false constant).
/// Forcing it via `--choose` still passes STRUCTURAL validation: eligibility
/// is a walk-time, presentation-point property (Task 19), never evaluated
/// here.
#[test]
fn guard_false_forcing_choice_passes_structural_validation() {
    let text = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n\
                <branch id=\"approach\">\n\
                <choice id=\"soft\" label=\"Soft\" when=\"1 > 2\">\n@narrator: a\n</choice>\n\
                <choice id=\"blunt\" label=\"Blunt\">\n@narrator: b\n</choice>\n\
                </branch>\n";
    let (folded, doc) = folded_and_doc(text, "guard-false", Path::new("."));
    let mocks = MockSet {
        choose: BTreeMap::from([("approach".to_string(), vec!["soft".to_string()])]),
        ..Default::default()
    };
    let diags = validate(&mocks, &folded, &doc);
    assert!(
        diags.is_empty(),
        "structural pre-walk validation MUST NOT evaluate a forced choice's guard \
         (§4.3/§4.4 — eligibility is Task 19's job): {diags:?}"
    );
}

// ---------------------------------------------------------------------
// `parse_mock_yaml` — the §4.3 `--mock` shape.
// ---------------------------------------------------------------------

#[test]
fn parse_mock_yaml_reads_all_four_surfaces() {
    let yaml = "state:\n  run.metHelpfully: true\n  run.tip: 5\n\
                facts:\n  - \"inParty(shadowheart)\"\n\
                choose:\n  sofaHelp: help\n  h: [a, b]\n\
                events:\n  - questActive\n";
    let mocks = parse_mock_yaml(yaml).expect("valid mock yaml");
    assert_eq!(mocks.facts, vec!["inParty(shadowheart)".to_string()]);
    assert_eq!(mocks.choose.get("sofaHelp"), Some(&vec!["help".to_string()]));
    assert_eq!(mocks.choose.get("h"), Some(&vec!["a".to_string(), "b".to_string()]));
    assert_eq!(mocks.events, vec!["questActive".to_string()]);
    let state: BTreeMap<_, _> = mocks.state.iter().map(|(p, v, _)| (p.clone(), v.clone())).collect();
    assert_eq!(state.get("run.metHelpfully"), Some(&"true".to_string()));
    assert_eq!(state.get("run.tip"), Some(&"5".to_string()));
}

#[test]
fn parse_mock_yaml_reads_accept_and_accepts_keys() {
    let mocks = parse_mock_yaml("accept:\n  - rescueHalsin\n").expect("valid mock yaml");
    assert_eq!(mocks.accepts, vec!["rescueHalsin".to_string()]);
    let mocks = parse_mock_yaml("accepts:\n  - rescueHalsin\n  - anotherQuest\n").expect("valid mock yaml");
    assert_eq!(mocks.accepts, vec!["rescueHalsin".to_string(), "anotherQuest".to_string()]);
}

#[test]
fn parse_mock_yaml_empty_document_is_an_empty_mockset() {
    assert_eq!(parse_mock_yaml("").unwrap(), MockSet::default());
    assert_eq!(parse_mock_yaml("state: {}\n").unwrap(), MockSet::default());
}

#[test]
fn parse_mock_yaml_rejects_malformed_shape() {
    let err = parse_mock_yaml("state: not-a-map\n").unwrap_err();
    assert_eq!(err.code, "E-TRACE-MOCK-PARSE", "{err:?}");
    let err = parse_mock_yaml("choose:\n  h: 5\n").unwrap_err();
    assert_eq!(err.code, "E-TRACE-MOCK-PARSE", "{err:?}");
    let err = parse_mock_yaml("state: [1, 2\n").unwrap_err();
    assert_eq!(err.code, "E-TRACE-MOCK-PARSE", "{err:?}");
}

// ---------------------------------------------------------------------
// `merge` — YAML/flag composition semantics (§4.3).
// ---------------------------------------------------------------------

#[test]
fn merge_facts_union_dedupes_and_preserves_file_then_flag_order() {
    let file = MockSet { facts: vec!["a(x)".into(), "b(y)".into()], ..Default::default() };
    let flags = MockSet { facts: vec!["b(y)".into(), "c(z)".into()], ..Default::default() };
    let merged = merge(file, flags);
    assert_eq!(merged.facts, vec!["a(x)".to_string(), "b(y)".to_string(), "c(z)".to_string()]);
}

#[test]
fn merge_choose_flag_replaces_file_entry_per_id() {
    let file = MockSet {
        choose: BTreeMap::from([
            ("sofaHelp".to_string(), vec!["help".to_string()]),
            ("otherHub".to_string(), vec!["a".to_string(), "b".to_string()]),
        ]),
        ..Default::default()
    };
    let flags = MockSet {
        choose: BTreeMap::from([("sofaHelp".to_string(), vec!["tip".to_string()])]),
        ..Default::default()
    };
    let merged = merge(file, flags);
    assert_eq!(merged.choose.get("sofaHelp"), Some(&vec!["tip".to_string()]), "flag replaces file, per id");
    assert_eq!(
        merged.choose.get("otherHub"),
        Some(&vec!["a".to_string(), "b".to_string()]),
        "an id absent from flags keeps the file entry"
    );
}

#[test]
fn merge_flag_state_wins_per_path() {
    let file = MockSet {
        state: vec![
            ("run.metHelpfully".to_string(), "true".to_string(), zero_span()),
            ("run.tip".to_string(), "5".to_string(), zero_span()),
        ],
        ..Default::default()
    };
    let flags = MockSet {
        state: vec![("run.metHelpfully".to_string(), "false".to_string(), zero_span())],
        ..Default::default()
    };
    let merged = merge(file, flags);
    let values: BTreeMap<_, _> = merged.state.iter().map(|(p, v, _)| (p.clone(), v.clone())).collect();
    assert_eq!(values.get("run.metHelpfully"), Some(&"false".to_string()), "flag wins over file, same path");
    assert_eq!(values.get("run.tip"), Some(&"5".to_string()), "file-only path passes through");
    assert_eq!(merged.state.len(), 2, "the shadowed file entry for run.metHelpfully is dropped, not duplicated");
}

#[test]
fn merge_events_compose_file_then_flags_in_order() {
    let file = MockSet { events: vec!["questActive".to_string()], ..Default::default() };
    let flags = MockSet { events: vec!["npcSpoke".to_string()], ..Default::default() };
    let merged = merge(file, flags);
    assert_eq!(merged.events, vec!["questActive".to_string(), "npcSpoke".to_string()]);
}

#[test]
fn merge_accepts_union_dedupes_and_preserves_file_then_flag_order() {
    let file = MockSet { accepts: vec!["a".into(), "b".into()], ..Default::default() };
    let flags = MockSet { accepts: vec!["b".into(), "c".into()], ..Default::default() };
    let merged = merge(file, flags);
    assert_eq!(merged.accepts, vec!["a".to_string(), "b".to_string(), "c".to_string()]);
}

/// End-to-end: a `--mock` file seeds a typo'd path, a `--state` flag corrects
/// it (flag wins), and validation is clean.
#[test]
fn end_to_end_yaml_file_plus_flag_state_win_validates_clean() {
    let yaml = "state:\n  run.metHelpfully: false\n";
    let file = parse_mock_yaml(yaml).expect("valid mock yaml");
    let flags = MockSet {
        state: vec![("run.metHelpfully".to_string(), "true".to_string(), zero_span())],
        ..Default::default()
    };
    let merged = merge(file, flags);
    assert_eq!(merged.state.len(), 1);
    assert_eq!(merged.state[0].1, "true");

    let (folded, doc) = load("../../docs/examples/choice-persist.lute");
    assert!(validate(&merged, &folded, &doc).is_empty());
}

// ---------------------------------------------------------------------
// dsl 0.5.1 §1.1: reserved quest paths are seedable when the document
// references them.
// ---------------------------------------------------------------------

fn quest_state_reader_text() -> &'static str {
    "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n\
     <match on=\"quest.foo.state\">\n\
     <when is=\"active\" test=\"quest.foo.objectives.bar.done\">\n@x: a\n</when>\n\
     <when is=\"complete | failed\">\n@x: b\n</when>\n\
     <otherwise>\n@x: c\n</otherwise>\n\
     </match>\n"
}

/// `--state quest.foo.state=complete` against a scene that reads
/// `quest.foo.state` (the match subject) validates clean — the document
/// REFERENCES the exact path (§1.1).
#[test]
fn reserved_quest_state_mock_admitted_when_document_references_it() {
    let (folded, doc) = folded_and_doc(quest_state_reader_text(), "quest-state-reader", Path::new("."));
    let mocks = MockSet {
        state: vec![("quest.foo.state".to_string(), "complete".to_string(), zero_span())],
        ..Default::default()
    };
    let diags = validate(&mocks, &folded, &doc);
    assert!(diags.is_empty(), "{diags:?}");
}

/// `--state quest.foo.objectives.bar.done=true` against the SAME document
/// (referenced via the `<when test=…>` guard, not just the match subject)
/// also validates clean.
#[test]
fn reserved_quest_objective_done_mock_admitted_when_document_references_it() {
    let (folded, doc) = folded_and_doc(quest_state_reader_text(), "quest-state-reader", Path::new("."));
    let mocks = MockSet {
        state: vec![("quest.foo.objectives.bar.done".to_string(), "true".to_string(), zero_span())],
        ..Default::default()
    };
    let diags = validate(&mocks, &folded, &doc);
    assert!(diags.is_empty(), "{diags:?}");
}

/// `--state quest.bar.state=complete` names a reserved path the document
/// does NOT reference (the document only reads `quest.foo.*`) -> STILL
/// `E-TRACE-MOCK-UNDECLARED` (§1.1: "you can only preview a read the
/// document actually makes").
#[test]
fn reserved_quest_path_mock_rejected_when_document_does_not_reference_it() {
    let (folded, doc) = folded_and_doc(quest_state_reader_text(), "quest-state-reader", Path::new("."));
    let mocks = MockSet {
        state: vec![("quest.bar.state".to_string(), "complete".to_string(), zero_span())],
        ..Default::default()
    };
    let diags = validate(&mocks, &folded, &doc);
    assert_eq!(codes(&diags), vec![E_TRACE_MOCK_UNDECLARED], "{diags:?}");
}

/// `--state quest.foo.state=paused` — a value outside the reserved
/// domain (`active|complete|failed|unset`) on a REFERENCED reserved path
/// -> a typed `E-TRACE-MOCK-TYPE`, never silently admitted (§1.1's own
/// text: "a malformed value ... is `E-TRACE-MOCK-*` as for any typed
/// mock").
#[test]
fn reserved_quest_state_mock_outside_domain_is_type_error() {
    let (folded, doc) = folded_and_doc(quest_state_reader_text(), "quest-state-reader", Path::new("."));
    let mocks = MockSet {
        state: vec![("quest.foo.state".to_string(), "paused".to_string(), zero_span())],
        ..Default::default()
    };
    let diags = validate(&mocks, &folded, &doc);
    assert_eq!(codes(&diags), vec![E_TRACE_MOCK_TYPE], "{diags:?}");
}

/// Same domain check for the objective `done` shape: only `true`/`false`
/// are legal, even on a referenced path.
#[test]
fn reserved_quest_objective_done_mock_outside_domain_is_type_error() {
    let (folded, doc) = folded_and_doc(quest_state_reader_text(), "quest-state-reader", Path::new("."));
    let mocks = MockSet {
        state: vec![("quest.foo.objectives.bar.done".to_string(), "yes".to_string(), zero_span())],
        ..Default::default()
    };
    let diags = validate(&mocks, &folded, &doc);
    assert_eq!(codes(&diags), vec![E_TRACE_MOCK_TYPE], "{diags:?}");
}
