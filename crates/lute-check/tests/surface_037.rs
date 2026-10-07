//! dsl 0.37.0 §3.1, §3.4–§3.6, §2.1: the S2b checker rules — `mono` speaker
//! and POV (per `::use` site), section id duplicates, reward ids, inline
//! modifier names/attributes, and the lowerCamelCase core vocabulary.

use lute_check::{
    check, parse_meta, resolve_components, CheckInput, ComponentSet, Mode, SchemaImports,
};
use lute_core_span::Diagnostic;
use lute_manifest::provider::ProviderSet;
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_test_vocab::vocab_snapshot;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static UNIQ: AtomicU64 = AtomicU64::new(0);

fn unique_dir() -> PathBuf {
    let n = UNIQ.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("lute_s037_{}_{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn check_text(text: String, components: ComponentSet) -> Vec<Diagnostic> {
    check(&CheckInput {
        text,
        uri: "scene".into(),
        snapshot: vocab_snapshot(),
        providers: ProviderSet::default(),
        mode: Mode::Author,
        imports: SchemaImports::default(),
        components,
        defaults: Default::default(),
    })
    .diagnostics
}

/// A scene with extra frontmatter `front` and section body `body`.
fn scene(front: &str, body: &str) -> Vec<Diagnostic> {
    check_text(
        format!("---\nkind: scene\nid: s\n{front}---\n## One\n{body}"),
        Default::default(),
    )
}

fn with(diags: &[Diagnostic], code: &str) -> Vec<String> {
    diags
        .iter()
        .filter(|d| d.code == code)
        .map(|d| d.message.clone())
        .collect()
}

// -- mono / POV (§3.4) ------------------------------------------------------

#[test]
fn mono_by_the_pov_is_clean() {
    let d = scene("pov: mira\n", "@mira{mono}: I should leave.\n");
    assert!(with(&d, "E-MONO-POV").is_empty() && with(&d, "E-MONO-NO-POV").is_empty(), "{d:?}");
}

#[test]
fn mono_by_an_allow_listed_speaker_is_clean() {
    let d = scene("pov: fixer\nmonoSpeakers: [mira]\n", "@mira{mono}: I should leave.\n");
    assert!(with(&d, "E-MONO-POV").is_empty(), "{d:?}");
}

#[test]
fn mono_by_another_speaker_is_mono_pov() {
    let d = scene("pov: fixer\n", "@mira{mono}: I should leave.\n");
    let m = with(&d, "E-MONO-POV");
    assert_eq!(m.len(), 1, "{d:?}");
    assert!(m[0].contains("`fixer`") && m[0].contains("monoSpeakers"), "{}", m[0]);
}

#[test]
fn mono_without_any_pov_is_mono_no_pov() {
    let d = scene("", "@mira{mono}: I should leave.\n");
    let m = with(&d, "E-MONO-NO-POV");
    assert_eq!(m.len(), 1, "{d:?}");
    assert!(m[0].contains("defaults"), "{}", m[0]);
}

/// One component, two callers with different POVs: the component's `mono`
/// line passes where its speaker is the caller's POV and fails, at the
/// `::use`, naming the component, where it is not.
#[test]
fn component_mono_is_judged_per_caller() {
    let dir = unique_dir();
    std::fs::write(
        dir.join("c.lute"),
        "---\ncomponent: c\n---\n## Inner\n@mira{mono}: Not again.\n",
    )
    .unwrap();
    let caller = |pov: &str| {
        let text = format!(
            "---\nkind: scene\nid: s\npov: {pov}\ncomponents: [c.lute]\n---\n## One\n\
             ::use{{component=\"c\"}}\n"
        );
        let (doc, _) = lute_syntax::parse(&text);
        let (meta, _) = parse_meta(&doc.meta, &CapabilitySnapshot::default());
        let components = resolve_components(&dir, &meta.components, doc.meta.span);
        check_text(text, components)
    };
    let ok = caller("mira");
    assert!(with(&ok, "E-MONO-POV").is_empty(), "{ok:?}");
    let bad = caller("fixer");
    let hits: Vec<&Diagnostic> = bad.iter().filter(|d| d.code == "E-MONO-POV").collect();
    assert_eq!(hits.len(), 1, "{bad:?}");
    assert!(hits[0].message.starts_with("component `c`"), "{}", hits[0].message);
    assert_eq!(hits[0].related.len(), 1, "the component line rides along");
    assert_eq!(hits[0].span.line, 8, "anchored at the `::use`");
}

// -- sections (§3.1) --------------------------------------------------------

#[test]
fn duplicate_section_id_is_section_dup() {
    let d = check_text(
        "---\nkind: scene\nid: s\n---\n## A {#x}\n@a: one\n## B {#y}\n@a: two\n## C {#x}\n@a: three\n"
            .into(),
        Default::default(),
    );
    let m = with(&d, "E-SECTION-DUP");
    assert_eq!(m.len(), 1, "{d:?}");
    assert!(m[0].contains("section 1"), "{}", m[0]);
}

// -- rewards (§3.5) ---------------------------------------------------------

fn quest(rewards: &str) -> Vec<Diagnostic> {
    check_text(
        format!(
            "---\nkind: quest\n---\n<quest id=\"q\" title=\"Q\" start=\"true\">\n{rewards}\
             <objective id=\"o\" title=\"O\" done=\"true\">\n<reward id=\"bonus\" kind=\"gold\" amount=\"1\"/>\n</objective>\n</quest>\n"
        ),
        Default::default(),
    )
}

#[test]
fn distinct_reward_ids_are_clean() {
    let d = quest("<reward id=\"pay\" kind=\"gold\" amount=\"5\"/>\n");
    assert!(with(&d, "E-REWARD-DUP").is_empty() && with(&d, "E-UNKNOWN-ATTR").is_empty(), "{d:?}");
}

#[test]
fn duplicate_reward_id_across_quest_and_objective_is_reward_dup() {
    let d = quest("<reward id=\"bonus\" kind=\"gold\" amount=\"5\"/>\n");
    assert_eq!(with(&d, "E-REWARD-DUP").len(), 1, "{d:?}");
}

// -- inline modifiers (§3.6) ------------------------------------------------

fn modifier(front: &str, text: &str) -> Vec<String> {
    with(&scene(front, &format!("@a: {text}\n")), "E-TEXT-MODIFIER")
}

#[test]
fn core_modifiers_well_formed_are_clean() {
    assert!(modifier("", "Hi :pause{s=0.5} :speed[there]{rate=1.25}.").is_empty());
}

#[test]
fn pause_faults() {
    assert_eq!(modifier("", "a :pause{s=-1} b").len(), 1, "negative seconds");
    assert_eq!(modifier("", "a :pause{x=1} b").len(), 2, "unknown attr + missing s");
    assert_eq!(modifier("", "a :pause[b]{s=1}").len(), 1, "pause is a leaf");
}

#[test]
fn speed_faults() {
    assert_eq!(modifier("", "a :speed[b]{rate=0}").len(), 1, "rate must be positive");
    assert_eq!(modifier("", "a :speed[b]{}").len(), 1, "rate is required");
    assert_eq!(modifier("", "a :speed{rate=2} b").len(), 1, "speed is a span");
}

#[test]
fn text_style_members() {
    let front = "enums:\n  textStyle: [emphasis]\n";
    assert!(modifier(front, ":emphasis[Hello]").is_empty());
    assert_eq!(modifier(front, ":emphasis[Hello]{loud}").len(), 1, "a style takes no attrs");
    assert_eq!(modifier(front, ":emphasys[Hello]").len(), 1, "not a member");
    let d = scene("", "@a: :emphasis[Hello]\n");
    assert_eq!(with(&d, "E-DOMAIN-UNKNOWN").len(), 1, "no textStyle domain: {d:?}");
}

// -- lowerCamelCase (§2.1, D14) ---------------------------------------------

fn is_lower_camel(key: &str) -> bool {
    key.chars().next().is_some_and(|c| c.is_ascii_lowercase())
        && key.chars().all(|c| c.is_ascii_alphanumeric())
}

#[test]
fn core_vocabulary_is_lower_camel() {
    let snap = lute_manifest::core::load_core_snapshot();
    let mut keys: Vec<String> = Vec::new();
    for d in snap.directives.values() {
        keys.extend(d.attrs.iter().map(|a| a.name.clone()));
    }
    keys.extend(snap.domains.keys().cloned());
    for list in [
        lute_check::content_line::KNOWN_ATTRS,
        lute_check::meta::SCENE_KEYS,
        lute_check::logic_attrs::REWARD_ATTRS,
        lute_check::logic_attrs::QUEST_ATTRS,
        lute_check::logic_attrs::OBJECTIVE_ATTRS,
        &lute_manifest::project::MANIFEST_KEYS,
        &lute_manifest::project::DEFAULTABLE_KEYS,
    ] {
        keys.extend(list.iter().map(|k| k.to_string()));
    }
    let bad: Vec<&String> = keys.iter().filter(|k| !is_lower_camel(k)).collect();
    assert!(bad.is_empty(), "not lowerCamelCase: {bad:?}");
}

#[test]
fn snake_case_key_is_author_case() {
    let d = scene("", "@a{voice_key=\"x\"}: hi\n");
    let m = with(&d, "E-AUTHOR-CASE");
    assert_eq!(m.len(), 1, "{d:?}");
    assert!(m[0].contains("`voiceKey`"), "{}", m[0]);
    assert!(with(&d, "E-UNKNOWN-ATTR").is_empty(), "one report: {d:?}");
}

/// A clip inside `<timeline><track>` is authored too: its snake_case key is
/// `E-AUTHOR-CASE`, not a silent pass.
#[test]
fn snake_case_key_in_a_timeline_clip_is_author_case() {
    let d = scene(
        "",
        "<timeline>\n<track channel=\"vfx\">\n::vfx{type=\"shed\" snake_key=\"x\" at=\"0\"}\n\
         </track>\n</timeline>\n",
    );
    let m = with(&d, "E-AUTHOR-CASE");
    assert_eq!(m.len(), 1, "{d:?}");
    assert!(m[0].contains("`snakeKey`"), "{}", m[0]);
}

/// A `@@p{mono}` line inside a `<branch>` choice of a component takes the
/// speaker each `::use` passes: judged against the caller's POV.
#[test]
fn component_speaker_param_mono_inside_a_branch_is_bound() {
    let dir = unique_dir();
    std::fs::write(
        dir.join("c.lute"),
        "---\ncomponent: c\nparams:\n  who: speaker\n---\n## Inner\n\
         <branch id=\"b\">\n<choice id=\"x\" text=\"X\">\n@@who{mono}: Not again.\n</choice>\n\
         <choice id=\"y\" text=\"Y\">\n@narrator: Fine.\n</choice>\n</branch>\n",
    )
    .unwrap();
    let caller = |pov: &str| {
        let text = format!(
            "---\nkind: scene\nid: s\npov: {pov}\ncomponents: [c.lute]\n---\n## One\n\
             ::use{{component=\"c\" who=\"mira\"}}\n"
        );
        let (doc, _) = lute_syntax::parse(&text);
        let (meta, _) = parse_meta(&doc.meta, &CapabilitySnapshot::default());
        let components = resolve_components(&dir, &meta.components, doc.meta.span);
        check_text(text, components)
    };
    let ok = caller("mira");
    assert!(with(&ok, "E-MONO-POV").is_empty(), "{ok:?}");
    let bad = caller("fixer");
    let m = with(&bad, "E-MONO-POV");
    assert_eq!(m.len(), 1, "{bad:?}");
    assert!(m[0].contains("`@mira{mono}`"), "{}", m[0]);
}
