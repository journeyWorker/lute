//! dsl 0.27.0 §2 — the checker says no: inputs 0.26 accepted silently.
use std::path::{Path, PathBuf};

fn unique_dir() -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let d = std::env::temp_dir().join(format!(
        "lute_strict027_{}_{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn span() -> lute_core_span::Span {
    lute_core_span::Span {
        byte_start: 0,
        byte_end: 0,
        line: 1,
        column: 1,
        utf16_range: (0, 0),
    }
}

fn imports(files: &[(&str, &str)]) -> lute_check::SchemaImports {
    let dir = unique_dir();
    for (name, body) in files {
        std::fs::write(Path::new(&dir).join(name), body).unwrap();
    }
    let uses: Vec<String> = files.iter().map(|(n, _)| n.to_string()).collect();
    lute_check::resolve_imports(&dir, &uses, &[], span())
}

fn messages(diags: &[lute_core_span::Diagnostic], code: &str) -> Vec<String> {
    diags
        .iter()
        .filter(|d| d.code == code)
        .map(|d| d.message.clone())
        .collect()
}

// -- T3-15: an entity kind rejects unknown keys ------------------------------

#[test]
fn an_entity_kind_with_a_misspelt_key_is_a_shape_error_with_a_suggestion() {
    let imp = imports(&[(
        "w.yaml",
        "entities:\n  room: { members: [lobby, chapel], membrs: [x], subsetof: place }\n  \
         place: { members: [lobby, chapel] }\n",
    )]);
    let errs = messages(&imp.diags, "E-ENTITY-KIND-SHAPE");
    assert_eq!(errs.len(), 2, "{:?}", imp.diags);
    assert!(
        errs.iter()
            .any(|m| m.contains("`membrs:`") && m.contains("did you mean `members`?")),
        "{errs:?}"
    );
    assert!(
        errs.iter()
            .any(|m| m.contains("`subsetof:`") && m.contains("did you mean `subsetOf`?")),
        "{errs:?}"
    );
}

#[test]
fn entity_kind_labels_are_refused_until_supported() {
    for key in ["labels", "lables"] {
        let imp = imports(&[(
            "w.yaml",
            &format!("entities:\n  room: {{ members: [lobby], {key}: {{ lobby: the lobby }} }}\n"),
        )]);
        let errs = messages(&imp.diags, "E-ENTITY-KIND-SHAPE");
        assert_eq!(errs.len(), 1, "{key}: {:?}", imp.diags);
        assert!(errs[0].contains("labels are not supported"), "{}", errs[0]);
    }
}

#[test]
fn every_documented_entity_kind_key_is_clean() {
    let imp = imports(&[(
        "w.yaml",
        "entities:\n  npc: { members: [ada] }\n  hero: { members: [ada], subsetOf: npc }\n  \
         guest: { open: engine }\n",
    )]);
    assert!(
        messages(&imp.diags, "E-ENTITY-KIND-SHAPE").is_empty(),
        "{:?}",
        imp.diags
    );
}

// -- T1-2: a `{ domain: K }` path is member-checked like an inline enum ------

fn check_scene(body: &str) -> Vec<lute_core_span::Diagnostic> {
    let text = format!(
        "---\nkind: scene\nid: s\nstate:\n  \
         run.ending: {{ type: {{ domain: ending }}, default: none }}\n  \
         run.room: {{ type: {{ domain: room }}, default: lobby }}\n  \
         run.guest: {{ type: {{ entity: room }} }}\n\
         enums:\n  ending: {{ members: [none, good, bad], labels: {{ none: nothing }} }}\n\
         entities:\n  room: {{ members: [lobby, chapel] }}\n---\n## One\n{body}\n"
    );
    lute_check::check(&lute_check::CheckInput {
        text,
        uri: "strict027".into(),
        snapshot: lute_manifest::core::load_core_snapshot(),
        providers: lute_manifest::provider::ProviderSet::default(),
        mode: lute_check::Mode::Author,
        imports: lute_check::SchemaImports::default(),
        components: Default::default(),
        defaults: Default::default(),
    })
    .diagnostics
}

fn codes(diags: &[lute_core_span::Diagnostic]) -> Vec<&str> {
    diags.iter().map(|d| d.code.as_str()).collect()
}

#[test]
fn a_set_outside_a_domain_is_a_set_type_error_naming_the_domain() {
    for (set, domain) in [
        (
            "::set{run.ending = 'tragic'}",
            "`domain: ending` (none, good, bad)",
        ),
        (
            "::set{run.room = 'chapl'}",
            "`domain: room` (lobby, chapel)",
        ),
        (
            "::set{run.guest = 'attic'}",
            "`entity: room` (lobby, chapel)",
        ),
    ] {
        let d = check_scene(set);
        let errs = messages(&d, "E-SET-TYPE");
        assert_eq!(errs.len(), 1, "{set}: {d:?}");
        assert!(errs[0].contains(domain), "{set}: {}", errs[0]);
    }
    let d = check_scene("::set{run.room = 'chapl'}");
    assert!(messages(&d, "E-SET-TYPE")[0].contains("did you mean `chapel`?"));
    for ok in ["::set{run.ending = 'good'}", "::set{run.guest = 'chapel'}"] {
        let d = check_scene(ok);
        assert!(!codes(&d).contains(&"E-SET-TYPE"), "{ok}: {d:?}");
    }
}

#[test]
fn a_foreign_literal_compared_with_a_domain_path_is_refused() {
    for guard in [
        "run.ending == 'tragic'",
        "run.room != 'attic'",
        "run.guest in ['lobby', 'attic']",
    ] {
        let d = check_scene(&format!("@narrator{{when=\"{guard}\"}}: Hi."));
        assert!(
            codes(&d).contains(&"E-WHEN-LITERAL-DOMAIN"),
            "{guard}: {d:?}"
        );
    }
}

#[test]
fn an_into_value_outside_a_domain_is_refused() {
    let d = check_scene(
        "<branch id=\"b\">\n<choice id=\"c\" label=\"End\" into=\"run.ending\" value=\"tragic\">\n\
         </choice>\n</branch>",
    );
    assert!(codes(&d).contains(&"E-INTO-VALUE"), "{d:?}");
    let d = check_scene(
        "<branch id=\"b\">\n<choice id=\"c\" label=\"End\" into=\"run.ending\" value=\"good\">\n\
         </choice>\n</branch>",
    );
    assert!(!codes(&d).contains(&"E-INTO-VALUE"), "{d:?}");
}

#[test]
fn a_match_over_a_domain_path_is_exhaustive_over_its_members() {
    let arms = |is: &[&str]| {
        let mut s = "<match on=\"run.ending\">\n".to_string();
        for m in is {
            s.push_str(&format!("<when is=\"{m}\">\n@narrator: {m}\n</when>\n"));
        }
        s + "</match>"
    };
    let d = check_scene(&arms(&["none", "good", "bad"]));
    assert!(!codes(&d).contains(&"E-NONEXHAUSTIVE"), "{d:?}");
    let d = check_scene(&arms(&["none", "good"]));
    let missing = messages(&d, "E-NONEXHAUSTIVE");
    assert_eq!(missing.len(), 1, "{d:?}");
    assert!(missing[0].contains("bad"), "{}", missing[0]);
    let d = check_scene(&arms(&["none", "good", "bad", "tragic"]));
    assert!(codes(&d).contains(&"E-WHEN-LITERAL-DOMAIN"), "{d:?}");
}

// -- T1-6: `per: K` covers members `subsetOf:` children add -------------------

#[test]
fn a_per_family_declares_the_members_its_sub_kinds_add() {
    let imp = imports(&[(
        "w.yaml",
        "state:\n  user.seen: { type: bool, default: false, per: npc }\n  \
         user.bond: { type: number, default: { sefa: 5, _: 0 }, per: bonded }\n\
         entities:\n  npc: { members: [tavi] }\n  bonded: { subsetOf: npc, members: [brann] }\n  \
         confidant: { subsetOf: bonded, members: [sefa] }\n",
    )]);
    assert!(
        messages(&imp.diags, "E-STATE-DECL").is_empty(),
        "{:?}",
        imp.diags
    );
    for path in [
        "user.seen.tavi",
        "user.seen.brann",
        "user.seen.sefa",
        "user.bond.brann",
    ] {
        assert!(
            imp.state.decls.contains_key(path),
            "{path}: {:?}",
            imp.state.decls.keys()
        );
    }
    assert!(!imp.state.decls.contains_key("user.bond.tavi"));
    let sefa = &imp.state.decls["user.bond.sefa"];
    assert_eq!(sefa.default, Some(lute_manifest::types::Literal::Num(5.0)));
}
