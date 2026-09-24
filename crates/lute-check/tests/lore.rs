//! dsl 0.19.0 lore entries through the assembled `check()` / `fix_document` /
//! project passes: the `kind: lore` document, `<entry>` attribute shape
//! (`E-ENTRY-ATTR`), identity (`E-ENTRY-ID-DUP`, `E-ENTRY-SERIES-ORDER`),
//! grammar admission, the entry-body walk, the reserved `entry.<id>.read`
//! path, and the `check-project` passes (`W-ENTRY-REF-UNKNOWN`).

use std::path::PathBuf;

use lute_check::{
    check, check_project_entry_ids, check_project_entry_refs, colliding_entry_occurrences,
    fix_document, CheckInput, CheckResult, Mode, SchemaImports,
};
use lute_core_span::{Diagnostic, Severity};
use lute_manifest::provider::ProviderSet;

fn run(text: &str) -> CheckResult {
    check(&CheckInput {
        text: text.to_string(),
        uri: "lore".into(),
        snapshot: lute_manifest::core::load_core_snapshot(),
        providers: ProviderSet::default(),
        mode: Mode::Author,
        imports: SchemaImports::default(),
        components: Default::default(),
        defaults: Default::default(),
    })
}

fn diags(text: &str) -> Vec<Diagnostic> {
    run(text).diagnostics
}

fn codes(text: &str) -> Vec<String> {
    diags(text).into_iter().map(|d| d.code).collect()
}

fn with_code<'a>(ds: &'a [Diagnostic], code: &str) -> Vec<&'a Diagnostic> {
    ds.iter().filter(|d| d.code == code).collect()
}

/// The source text a diagnostic is anchored at.
fn anchored<'s>(src: &'s str, d: &Diagnostic) -> &'s str {
    &src[d.span.byte_start..d.span.byte_end]
}

const LORE_HDR: &str = "---\nkind: lore\ntitle: Ship's records\nentities:\n  \
    person: { members: [vesna] }\n  project: { members: [project_lumen] }\nrelations:\n  \
    knows: { args: [person, project] }\nstate:\n  \
    run.labBurned: { type: bool, default: false }\n---\n";

fn lore(body: &str) -> String {
    format!("{LORE_HDR}{body}")
}

/// One entry wrapping `body`.
fn entry(body: &str) -> String {
    lore(&format!("<entry id=\"e\">\n{body}\n</entry>\n"))
}

const SCENE_HDR: &str = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n";

// --- the lore document -------------------------------------------------------

#[test]
fn valid_lore_document_checks_clean() {
    let src = lore(
        "<entry id=\"scientistLog1\" target=\"item.torn_note_1\" category=\"note\" \
         series=\"scientistLog\" order=\"1\" title=\"Research log, day 3\">\n\
         @scientist: Day three. Subject E does not respond to light.\n\
         ::assert{knows(vesna, project_lumen)}\n\
         </entry>\n\n\
         <entry id=\"scientistLog2\" series=\"scientistLog\" order=\"2\" \
         when=\"entry.scientistLog1.read\">\n\
         @scientist: Day four.\n\
         ::set{run.labBurned = true}\n\
         </entry>\n\n\
         <entry id=\"rustyKey\" target=\"item.rusty_key\" category=\"item\">\n\
         <match on=\"run.labBurned\">\n\
         <when is=\"true\">\n@narrator: A scorched key.\n</when>\n\
         <otherwise>\n@narrator: A rusty key, stamped \"Research wing B2\".\n</otherwise>\n\
         </match>\n\
         <match on=\"entry.rustyKey.read\">\n\
         <when is=\"true\">\n@narrator: You have seen this before.\n</when>\n\
         <when is=\"false\">\n@narrator: A key.\n</when>\n\
         </match>\n\
         </entry>\n",
    );
    let r = run(&src);
    assert!(r.diagnostics.is_empty(), "{:#?}", r.diagnostics);
    assert!(r.resolved.is_some());
}

#[test]
fn unknown_kind_message_lists_lore() {
    let ds = diags("---\nkind: codex\n---\n");
    let d = with_code(&ds, "E-UNKNOWN-KIND");
    assert_eq!(d.len(), 1, "{ds:?}");
    assert!(d[0].message.contains("`lore`"), "{}", d[0].message);
}

#[test]
fn lore_takes_quest_frontmatter_keys() {
    // Scene-only keys are unknown on a lore doc, exactly as on a quest doc;
    // `extra:` (a root-kind key) is legal.
    let cs = codes(
        "---\nkind: lore\ncharacter: x\nextra:\n  arc: main\n---\n<entry id=\"e\">\n@n: hi\n</entry>\n",
    );
    assert_eq!(
        cs.iter().filter(|c| *c == "E-META-UNKNOWN-KEY").count(),
        1,
        "{cs:?}"
    );
}

// --- E-ENTRY-ATTR / E-UNKNOWN-ATTR -------------------------------------------

/// The `E-ENTRY-ATTR` diagnostics of a one-entry doc with `open` as its tag.
fn entry_attr_diags(open: &str) -> (String, Vec<Diagnostic>) {
    let src = lore(&format!("{open}\n@n: hi\n</entry>\n"));
    let ds = diags(&src)
        .into_iter()
        .filter(|d| d.code == "E-ENTRY-ATTR")
        .collect();
    (src, ds)
}

#[test]
fn missing_id_is_entry_attr() {
    let (src, ds) = entry_attr_diags("<entry category=\"note\">");
    assert_eq!(ds.len(), 1, "{ds:?}");
    assert!(ds[0].message.contains("no `id`"), "{}", ds[0].message);
    assert_eq!(ds[0].severity, Severity::Error);
    assert!(anchored(&src, &ds[0]).starts_with("<entry"));
}

#[test]
fn non_ident_id_is_entry_attr() {
    let (src, ds) = entry_attr_diags("<entry id=\"9lives\">");
    assert_eq!(ds.len(), 1, "{ds:?}");
    assert_eq!(anchored(&src, &ds[0]), "9lives");
}

#[test]
fn non_string_id_is_entry_attr_once() {
    let (src, ds) = entry_attr_diags("<entry id>");
    assert_eq!(ds.len(), 1, "a bare `id` is one fault, not also `missing`: {ds:?}");
    assert!(anchored(&src, &ds[0]).starts_with("id"));
}

#[test]
fn malformed_target_is_entry_attr() {
    for bad in ["item..key", "9item.key", "item.", "item.rusty key"] {
        let (src, ds) = entry_attr_diags(&format!("<entry id=\"e\" target=\"{bad}\">"));
        assert_eq!(ds.len(), 1, "{bad}: {ds:?}");
        assert_eq!(anchored(&src, &ds[0]), bad);
    }
    for good in ["npc", "item.rusty_key", "place.lab-b2", "item.9"] {
        let (_, ds) = entry_attr_diags(&format!("<entry id=\"e\" target=\"{good}\">"));
        assert!(ds.is_empty(), "{good}: {ds:?}");
    }
}

#[test]
fn non_ident_category_and_series_are_entry_attr() {
    let (src, ds) = entry_attr_diags("<entry id=\"e\" category=\"field note\">");
    assert_eq!(ds.len(), 1, "{ds:?}");
    assert_eq!(anchored(&src, &ds[0]), "field note");
    let (src, ds) = entry_attr_diags("<entry id=\"e\" series=\"log.1\">");
    assert_eq!(ds.len(), 1, "{ds:?}");
    assert_eq!(anchored(&src, &ds[0]), "log.1");
}

#[test]
fn order_must_be_a_non_negative_integer() {
    for bad in ["-1", "1.5", "first", "", "99999999999"] {
        let (src, ds) = entry_attr_diags(&format!("<entry id=\"e\" series=\"s\" order=\"{bad}\">"));
        assert_eq!(ds.len(), 1, "{bad:?}: {ds:?}");
        assert_eq!(anchored(&src, &ds[0]), bad);
    }
    let (_, ds) = entry_attr_diags("<entry id=\"e\" series=\"s\" order=\"0\">");
    assert!(ds.is_empty(), "{ds:?}");
}

#[test]
fn order_without_series_is_entry_attr() {
    let (src, ds) = entry_attr_diags("<entry id=\"e\" order=\"1\">");
    assert_eq!(ds.len(), 1, "{ds:?}");
    assert!(ds[0].message.contains("requires `series`"), "{}", ds[0].message);
    assert_eq!(anchored(&src, &ds[0]), "1");
}

#[test]
fn unknown_entry_attr_is_unknown_attr() {
    let src = lore("<entry id=\"e\" anchor=\"item.key\">\n@n: hi\n</entry>\n");
    let ds = diags(&src);
    let u = with_code(&ds, "E-UNKNOWN-ATTR");
    assert_eq!(u.len(), 1, "{ds:?}");
    assert_eq!(u[0].message, "`<entry>` has no attribute `anchor` (dsl 0.10.0 §4)");
    assert!(with_code(&ds, "E-ENTRY-ATTR").is_empty(), "{ds:?}");
}

#[test]
fn hyphenated_entry_id_is_path_ident() {
    // A valid `Ident`, but `entry.<id>.read` is CEL-facing — the quest-id rule.
    let cs = codes(&lore("<entry id=\"torn-note\">\n@n: hi\n</entry>\n"));
    assert!(cs.contains(&"E-PATH-IDENT".to_string()), "{cs:?}");
    assert!(!cs.contains(&"E-ENTRY-ATTR".to_string()), "{cs:?}");
}

// --- identity ----------------------------------------------------------------

#[test]
fn duplicate_entry_id_in_a_document() {
    let src = lore(
        "<entry id=\"note\">\n@n: a\n</entry>\n<entry id=\"note\">\n@n: b\n</entry>\n",
    );
    let ds = diags(&src);
    let dup = with_code(&ds, "E-ENTRY-ID-DUP");
    assert_eq!(dup.len(), 1, "{ds:?}");
    assert_eq!(anchored(&src, dup[0]), "note");
    assert!(
        dup[0].span.byte_start > src.find("<entry id=\"note\">\n@n: b").unwrap(),
        "the second occurrence is flagged"
    );
}

#[test]
fn duplicate_series_order_in_a_document() {
    let src = lore(
        "<entry id=\"a\" series=\"log\" order=\"1\">\n@n: a\n</entry>\n\
         <entry id=\"b\" series=\"log\" order=\"01\">\n@n: b\n</entry>\n\
         <entry id=\"c\" series=\"other\" order=\"1\">\n@n: c\n</entry>\n",
    );
    let ds = diags(&src);
    let dup = with_code(&ds, "E-ENTRY-SERIES-ORDER");
    assert_eq!(dup.len(), 1, "`01` is position 1; another series is free: {ds:?}");
    assert_eq!(anchored(&src, dup[0]), "01");
    assert!(dup[0].message.contains("`<entry id=\"a\">`"), "{}", dup[0].message);
}

// --- admission ---------------------------------------------------------------

fn not_admitted(text: &str) -> Vec<Diagnostic> {
    diags(text)
        .into_iter()
        .filter(|d| d.code == "E-GRAMMAR-NOT-ADMITTED")
        .collect()
}

#[test]
fn entry_body_rejects_played_constructs() {
    for body in [
        "<branch id=\"b\">\n<choice id=\"c\" label=\"Go\">\n@n: x\n</choice>\n</branch>",
        "<hub id=\"h\">\n<choice id=\"c\" label=\"Go\" exit>\n@n: x\n</choice>\n</hub>",
        "::bg{asset=\"room\"}",
        "::use{component=\"greet\"}",
        "<on event=\"questComplete\">\n@n: x\n</on>",
        "<objective id=\"o\" done=\"true\"/>",
        "<timeline duration=\"2s\">\n<clip at=\"0s\">\n::sfx{sound=\"a\"}\n</clip>\n</timeline>",
    ] {
        let ds = not_admitted(&entry(body));
        assert_eq!(ds.len(), 1, "{body}: {ds:#?}");
        assert!(
            ds[0].message.contains("entry bodies are looked up, not played"),
            "{}",
            ds[0].message
        );
    }
}

// --- per-entry identity scope and producer sites -------------------------------

#[test]
fn each_entry_is_its_own_line_code_scope() {
    // The same (speaker, code) in two entries is fine; twice in one is not.
    let two = lore(
        "<entry id=\"a\">\n@n{code=\"0010\"}: a\n</entry>\n\
         <entry id=\"b\">\n@n{code=\"0010\"}: b\n</entry>\n",
    );
    assert!(codes(&two).is_empty(), "{:?}", codes(&two));
    let one = entry("@n{code=\"0010\"}: a\n@n{code=\"0010\"}: b");
    assert!(codes(&one).contains(&"E-DUP-LINE-CODE".to_string()));
}

#[test]
fn tag_restarts_the_counter_per_entry() {
    let src = lore("<entry id=\"a\">\n@n: a\n@n: b\n</entry>\n<entry id=\"b\">\n@n: c\n</entry>\n");
    let out = lute_check::tag_document(&src);
    assert_eq!(out.added, 3);
    assert!(out.text.contains("@n{code=\"0010\"}: a\n@n{code=\"0020\"}: b"), "{}", out.text);
    assert!(out.text.contains("@n{code=\"0010\"}: c"), "{}", out.text);
}

#[test]
fn entry_asserts_are_live_producer_sites() {
    // Lore documents are not graph nodes; an entry's `::assert` is never
    // proven unreachable, so it always seeds `producible()`.
    let src = entry("::assert{knows(vesna, project_lumen)}");
    let docs = parse_docs(&[("notes.lute", &src)]);
    let live = lute_check::connectivity::live_assert_relations(
        &docs,
        &Default::default(),
        &Default::default(),
        &Default::default(),
    );
    assert!(live.contains("knows"), "{live:?}");
}

#[test]
fn nested_match_keeps_entry_admission() {
    let ds = not_admitted(&entry(
        "<match on=\"run.labBurned\">\n<when is=\"true\">\n::bg{asset=\"room\"}\n</when>\n\
         <otherwise>\n@n: x\n</otherwise>\n</match>",
    ));
    assert_eq!(ds.len(), 1, "{ds:#?}");
}

#[test]
fn entry_body_admits_lines_match_and_effects() {
    let ds = not_admitted(&entry(
        "@n: hi\n::set{run.labBurned = true}\n::assert{knows(vesna, project_lumen)}\n\
         ::retract{knows(vesna, project_lumen)}\n\
         <match on=\"run.labBurned\">\n<when is=\"true\">\n@n: a\n</when>\n\
         <otherwise>\n::set{run.labBurned = false}\n</otherwise>\n</match>",
    ));
    assert!(ds.is_empty(), "{ds:#?}");
}

#[test]
fn lore_top_level_rejects_title_shot_and_quest() {
    let title = not_admitted(&lore("# Records\n<entry id=\"e\">\n@n: hi\n</entry>\n"));
    assert_eq!(title.len(), 1, "{title:#?}");
    assert!(title[0].message.contains("lore document"), "{}", title[0].message);

    let shot = not_admitted(&lore("<entry id=\"e\">\n@n: hi\n</entry>\n## Shot 1.\n@n: x\n"));
    assert_eq!(shot.len(), 1, "{shot:#?}");

    let quest = not_admitted(&lore(
        "<entry id=\"e\">\n@n: hi\n</entry>\n<quest id=\"q\">\n<objective id=\"o\" done=\"true\"/>\n</quest>\n",
    ));
    assert_eq!(quest.len(), 1, "{quest:#?}");
    assert!(quest[0].message.contains("<quest id=\"q\">"), "{}", quest[0].message);
}

#[test]
fn empty_lore_document_is_not_admitted() {
    let ds = not_admitted("---\nkind: lore\n---\n");
    assert_eq!(ds.len(), 1, "{ds:#?}");
    assert!(ds[0].message.contains("declares no `<entry>`"), "{}", ds[0].message);
}

#[test]
fn entry_in_scene_or_quest_document_is_not_admitted() {
    // Top level (before the first shot), where the parser takes a declaration
    // — the placement a misplaced `<quest>` in a scene document has too.
    let scene = not_admitted(
        "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n\
         <entry id=\"e\">\n@n: hi\n</entry>\n## Shot 1.\n@x: hi\n",
    );
    assert_eq!(scene.len(), 1, "{scene:#?}");
    assert!(scene[0].message.contains("scene document"), "{}", scene[0].message);

    let quest = not_admitted(
        "---\nkind: quest\n---\n<quest id=\"q\">\n<objective id=\"o\" done=\"true\"/>\n</quest>\n\
         <entry id=\"e\">\n@n: hi\n</entry>\n",
    );
    assert_eq!(quest.len(), 1, "{quest:#?}");
    assert!(quest[0].message.contains("quest document"), "{}", quest[0].message);
}

// --- the entry-body walk -------------------------------------------------------

#[test]
fn assert_errors_inside_an_entry_are_reported() {
    let cs = codes(&entry("::assert{knows(vesna)}"));
    assert!(cs.contains(&"E-RELATION-ARITY".to_string()), "{cs:?}");
    let cs = codes(&entry("::assert{knows(project_lumen, vesna)}"));
    assert!(cs.contains(&"E-FACT-DOMAIN".to_string()), "{cs:?}");
}

#[test]
fn entry_when_is_a_checked_condition_slot() {
    // An undeclared read in `when` is reported like one in `<quest start>`.
    let cs = codes(&lore("<entry id=\"e\" when=\"run.ghost\">\n@n: hi\n</entry>\n"));
    assert!(cs.contains(&"E-UNDECLARED".to_string()), "{cs:?}");
    // A maybe-unset read (no default) in `when` is E-MAYBE-UNSET.
    let cs = codes(
        "---\nkind: lore\nstate:\n  run.seen: { type: bool }\n---\n\
         <entry id=\"e\" when=\"run.seen\">\n@n: hi\n</entry>\n",
    );
    assert!(cs.contains(&"E-MAYBE-UNSET".to_string()), "{cs:?}");
}

// --- entry.<id>.read ---------------------------------------------------------

#[test]
fn entry_read_matches_exhaustively_in_a_scene() {
    // A foreign entry's flag: declared by shape, a finite bool domain, never
    // maybe-unset — `true`/`false` cover it without `<otherwise>`.
    let cs = codes(&format!(
        "{SCENE_HDR}<match on=\"entry.scientistLog1.read\">\n\
         <when is=\"true\">\n@x: I know.\n</when>\n\
         <when is=\"false\">\n@x: I wonder.\n</when>\n</match>\n\
         @x{{when=\"entry.rustyKey.read && !entry.scientistLog1.read\"}}: Hm.\n"
    ));
    assert!(cs.is_empty(), "{cs:?}");
}

#[test]
fn entry_read_domain_is_bool() {
    let cs = codes(&format!(
        "{SCENE_HDR}<match on=\"entry.note.read\">\n\
         <when is=\"yes\">\n@x: a\n</when>\n<otherwise>\n@x: b\n</otherwise>\n</match>\n"
    ));
    assert!(cs.contains(&"E-WHEN-LITERAL-DOMAIN".to_string()), "{cs:?}");
}

#[test]
fn other_entry_paths_are_undeclared() {
    let cs = codes(&format!("{SCENE_HDR}@x{{when=\"entry.note.seen\"}}: Hm.\n"));
    assert!(cs.contains(&"E-UNDECLARED".to_string()), "{cs:?}");
}

#[test]
fn writing_an_entry_path_is_rejected() {
    for target in ["entry.note.read", "entry.note.count"] {
        let ds = diags(&format!("{SCENE_HDR}::set{{{target} = true}}\n"));
        let w = with_code(&ds, "E-QUEST-RESERVED-WRITE");
        assert_eq!(w.len(), 1, "{target}: {ds:?}");
        assert!(w[0].message.contains(target), "{}", w[0].message);
        assert!(with_code(&ds, "E-UNDECLARED").is_empty(), "{ds:?}");
    }
    // Inside an entry too: content never writes its own read flag.
    let cs = codes(&entry("::set{entry.e.read = true}"));
    assert!(cs.contains(&"E-QUEST-RESERVED-WRITE".to_string()), "{cs:?}");
}

#[test]
fn entry_read_cannot_be_author_declared() {
    let cs = codes(&format!(
        "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  \
         entry.note.read: {{ type: bool, default: false }}\n---\n## Shot 1.\n@x: hi\n"
    ));
    assert!(cs.contains(&"E-STATE-NAMESPACE".to_string()), "{cs:?}");
}

// --- W-WHEN-TEST-LITERAL / lute fix reach entry arms --------------------------

#[test]
fn when_test_literal_and_fix_reach_entry_arms() {
    let src = entry(
        "<match on=\"run.labBurned\">\n<when test=\"$ == true\">\n@n: a\n</when>\n\
         <otherwise>\n@n: b\n</otherwise>\n</match>",
    );
    let ds = diags(&src);
    let w = with_code(&ds, "W-WHEN-TEST-LITERAL");
    assert_eq!(w.len(), 1, "{ds:?}");
    assert_eq!(anchored(&src, w[0]), "test=\"$ == true\"");

    let fixed = fix_document(&src);
    assert_eq!(fixed.changed, 1);
    assert!(fixed.text.contains("<when is=\"true\">"), "{}", fixed.text);
    assert!(codes(&fixed.text).is_empty(), "{:?}", codes(&fixed.text));
}

// --- check-project -------------------------------------------------------------

fn parse_docs(files: &[(&str, &str)]) -> Vec<(PathBuf, lute_syntax::ast::Document)> {
    files
        .iter()
        .map(|(p, t)| (PathBuf::from(p), lute_syntax::parse(t).0))
        .collect()
}

#[test]
fn project_entry_id_dup_across_files() {
    let a = lore("<entry id=\"note\">\n@n: a\n</entry>\n");
    let b = lore("<entry id=\"other\">\n@n: x\n</entry>\n<entry id=\"note\">\n@n: b\n</entry>\n");
    let docs = parse_docs(&[("a.lute", &a), ("b.lute", &b)]);
    let out = check_project_entry_ids(&docs);
    assert_eq!(out.len(), 1, "{out:?}");
    let (path, d) = &out[0];
    assert_eq!(path, &PathBuf::from("b.lute"));
    assert_eq!(d.code, "E-ENTRY-ID-DUP");
    assert_eq!(anchored(&b, d), "note");
    assert!(d.message.contains("a.lute"), "{}", d.message);

    // Both occurrences are covered, so check-project suppresses the per-file
    // copies of the same collision.
    let covered = colliding_entry_occurrences(&docs);
    assert_eq!(covered.len(), 2, "{covered:?}");
}

#[test]
fn project_series_order_dup_across_files() {
    let a = lore("<entry id=\"log1\" series=\"log\" order=\"1\">\n@n: a\n</entry>\n");
    let b = lore(
        "<entry id=\"log2\" series=\"log\" order=\"2\">\n@n: b\n</entry>\n\
         <entry id=\"log1b\" series=\"log\" order=\"1\">\n@n: c\n</entry>\n",
    );
    let docs = parse_docs(&[("a.lute", &a), ("b.lute", &b)]);
    let out = check_project_entry_ids(&docs);
    assert_eq!(out.len(), 1, "{out:?}");
    let (path, d) = &out[0];
    assert_eq!(path, &PathBuf::from("b.lute"));
    assert_eq!(d.code, "E-ENTRY-SERIES-ORDER");
    assert!(d.message.contains("`<entry id=\"log1\">`"), "{}", d.message);
    assert_eq!(anchored(&b, d), "1");
}

#[test]
fn project_entry_ref_unknown() {
    let notes = lore("<entry id=\"scientistLog1\">\n@n: a\n</entry>\n");
    let scene = format!(
        "{SCENE_HDR}<match on=\"entry.scientistLog1.read\">\n\
         <when is=\"true\">\n@x: a\n</when>\n<when is=\"false\">\n@x: b\n</when>\n</match>\n\
         @x{{when=\"entry.scientistLog9.read\"}}: typo\n"
    );
    let docs = parse_docs(&[("notes.lute", &notes), ("scene.lute", &scene)]);
    let out = check_project_entry_refs(&docs);
    assert_eq!(out.len(), 1, "{out:?}");
    let (path, d) = &out[0];
    assert_eq!(path, &PathBuf::from("scene.lute"));
    assert_eq!(d.code, "W-ENTRY-REF-UNKNOWN");
    assert_eq!(d.severity, Severity::Warning);
    assert!(d.message.contains("entry.scientistLog9.read"), "{}", d.message);

    // Every read resolves once the entry is declared.
    let fixed = scene.replace("scientistLog9", "scientistLog1");
    let docs = parse_docs(&[("notes.lute", &notes), ("scene.lute", &fixed)]);
    assert!(check_project_entry_refs(&docs).is_empty());
}
