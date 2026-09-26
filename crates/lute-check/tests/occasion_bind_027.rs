//! dsl 0.27.0 §3 (T2-1, T2-2, T2-10): members bound by occasions —
//! `occasion.target` as a ground fact argument / family index, `for=` beats,
//! rule variables compared with a domain-typed path, typed payloads.
use lute_check::{check, CheckInput, Mode, SchemaImports};
use lute_manifest::schema::{OccasionDecl, OccasionSelect, OccasionTarget};
use lute_manifest::types::Type;

fn input(text: &str) -> CheckInput {
    let mut snapshot = lute_manifest::core::load_core_snapshot();
    snapshot.occasions.insert(
        "summon".into(),
        OccasionDecl {
            name: "summon".into(),
            select: OccasionSelect::First,
            target: OccasionTarget::Domain {
                prefix: "hero".into(),
                entity: "hero".into(),
                members: None,
            },
            payload: [("copies".to_string(), Type::Number)].into_iter().collect(),
            ..Default::default()
        },
    );
    for (name, select) in [
        ("dailyReset", OccasionSelect::Sequence),
        ("morning", OccasionSelect::First),
    ] {
        snapshot.occasions.insert(
            name.into(),
            OccasionDecl {
                name: name.into(),
                select,
                ..Default::default()
            },
        );
    }
    CheckInput {
        text: text.to_string(),
        uri: "occasion_bind".into(),
        snapshot,
        providers: Default::default(),
        mode: Mode::Author,
        imports: SchemaImports::default(),
        components: Default::default(),
        defaults: Default::default(),
    }
}

fn errors(text: &str) -> Vec<(String, String)> {
    check(&input(text))
        .diagnostics
        .into_iter()
        .filter(|d| d.severity == lute_core_span::Severity::Error)
        .map(|d| (d.code, d.message))
        .collect()
}

const VOCAB: &str = "entities:\n  hero: { members: [aria, bram, cyra] }\n  \
                     ssr: { subsetOf: hero, members: [aria] }\n  \
                     room: { members: [lobby, chapel] }\n\
                     relations:\n  owned: { args: [hero], reserved: true }\n  \
                     birthday: { args: [hero] }\n  \
                     visitedRoom: { args: [room] }\n\
                     state:\n  user.bond: { type: number, default: 0, per: hero, owner: engine }\n  \
                     user.heat: { type: number, default: 0, per: room, owner: engine }\n";

fn lore(body: &str) -> String {
    format!("---\nkind: lore\nid: lore.gacha\ntitle: Gacha\n{VOCAB}---\n{body}")
}

#[test]
fn a_kind_beat_queries_facts_and_families_by_its_member() {
    let errs = errors(&lore(
        "<beat id=\"dupe\" on=\"summon\" target=\"kind:hero\" once=\"false\" when=\"holds(owned(occasion.target)) && user.bond[occasion.target] >= 2\">\n  \
         @narrator{when=\"!holds(owned(occasion.target))\"}: New!\n  \
         @narrator: {{occasion.target}} again.\n</beat>\n",
    ));
    assert!(errs.is_empty(), "{errs:?}");
}

#[test]
fn a_member_outside_the_relation_domain_is_named() {
    // `visitedRoom` takes a room: every hero member is outside its domain,
    // so the finding is shared and reported once, without a member suffix.
    let errs = errors(&lore(
        "<beat id=\"x\" on=\"summon\" target=\"kind:hero\" once=\"false\" when=\"holds(visitedRoom(occasion.target))\">\n  @narrator: hi\n</beat>\n",
    ));
    let dom: Vec<_> = errs.iter().filter(|(c, _)| c == "E-FACT-DOMAIN").collect();
    assert_eq!(
        dom.len(),
        3,
        "one per member (each message names it): {errs:?}"
    );
}

#[test]
fn a_family_of_another_kind_is_undeclared_per_member() {
    let errs = errors(&lore(
        "<beat id=\"x\" on=\"summon\" target=\"kind:hero\" once=\"false\" when=\"user.heat[occasion.target] > 1\">\n  @narrator: hi\n</beat>\n",
    ));
    assert!(
        errs.iter().any(|(c, m)| c == "E-UNDECLARED"
            && m.contains("user.heat.aria")
            && m.contains("`occasion.target` = `aria`")),
        "{errs:?}"
    );
}

#[test]
fn a_ground_read_outside_a_kind_beat_is_undeclared() {
    let errs = errors(&lore(
        "<beat id=\"x\" on=\"morning\" once=\"false\" when=\"holds(owned(occasion.target))\">\n  @narrator: hi\n</beat>\n",
    ));
    assert!(
        errs.iter()
            .any(|(c, m)| c == "E-UNDECLARED" && m.contains("readable only in a beat")),
        "{errs:?}"
    );
    assert!(!errs.iter().any(|(c, _)| c == "E-CEL-PROFILE"), "{errs:?}");
}

#[test]
fn a_for_beat_binds_each_member_on_a_sequence_occasion() {
    let errs = errors(&lore(
        "<beat id=\"bday\" on=\"dailyReset\" for=\"kind:hero\" once=\"false\" when=\"holds(birthday(occasion.target)) && holds(owned(occasion.target))\">\n  \
         @narrator: Happy birthday, {{occasion.target}}!\n</beat>\n",
    ));
    assert!(errs.is_empty(), "{errs:?}");
}

#[test]
fn for_needs_an_untargeted_sequence_occasion_and_a_closed_kind() {
    let cases = [
        ("on=\"morning\" for=\"kind:hero\"", "select: sequence"),
        ("on=\"summon\" for=\"kind:hero\"", "raised for one"),
        (
            "on=\"dailyReset\" for=\"kind:heros\"",
            "did you mean `kind:hero`",
        ),
        ("on=\"dailyReset\" for=\"hero\"", "must name a kind"),
        (
            "on=\"dailyReset\" for=\"kind:hero\" target=\"hero.aria\"",
            "remove one of the two",
        ),
    ];
    for (attrs, want) in cases {
        let errs = errors(&lore(&format!(
            "<beat id=\"x\" {attrs} once=\"false\">\n  @narrator: hi\n</beat>\n"
        )));
        assert!(
            errs.iter()
                .any(|(c, m)| c == "E-BEAT-ATTR" && m.contains(want)),
            "{attrs}: {errs:?}"
        );
    }
}

/// A scene's `for:` is the frontmatter spelling of `for=`: it binds
/// `occasion.target` across the scene, and is checked like the attribute.
#[test]
fn a_scene_for_key_binds_each_member_like_the_attribute() {
    let scene = |keys: &str, body: &str| {
        format!("---\nkind: scene\nid: bday\ntitle: Birthday\n{keys}\n{VOCAB}---\n\n# Birthday\n\n## Shot 1.\n\n{body}\n")
    };
    let ok = errors(&scene(
        "on: dailyReset\nfor: \"kind:hero\"\nonce: false\nwhen: \"holds(birthday(occasion.target))\"",
        "@narrator: Happy birthday, {{occasion.target}}!",
    ));
    assert!(ok.is_empty(), "{ok:?}");
    let wrong = errors(&scene(
        "on: morning\nfor: \"kind:hero\"\nonce: false",
        "@narrator: hi",
    ));
    assert!(
        wrong
            .iter()
            .any(|(c, m)| c == "E-BEAT-ATTR" && m.contains("select: sequence")),
        "{wrong:?}"
    );
    // Without `for:`, the scene is no kind beat: `occasion.target` is unread.
    let unbound = errors(&scene(
        "on: dailyReset\nonce: false",
        "@narrator: {{occasion.target}}",
    ));
    assert!(
        unbound.iter().any(|(c, _)| c == "E-UNDECLARED"),
        "{unbound:?}"
    );
    let shape = errors(&scene(
        "on: dailyReset\nfor: [hero]\nonce: false",
        "@narrator: hi",
    ));
    assert!(
        shape
            .iter()
            .any(|(c, m)| c == "E-BEAT-ATTR" && m.contains("`for:` must be a string")),
        "{shape:?}"
    );
}

#[test]
fn a_rule_variable_compared_with_a_domain_path_is_instantiated() {
    let schema = "entities:\n  room: { members: [lobby, morgue, chapel] }\n\
                  relations:\n  adjacent: { args: [room, room] }\n  close: { args: [room], derive: true }\n\
                  facts:\n  - \"adjacent(lobby, chapel)\"\n\
                  rules:\n  - \"close(R) :- adjacent(R, S), cel(\\\"run.stalker == S\\\")\"\n\
                  state:\n  run.stalker: { type: { domain: room }, default: morgue, owner: engine }\n  \
                  run.hp: { type: number, default: 3 }\n";
    let doc = |rule: &str| {
        format!(
            "---\nkind: lore\nid: lore.ward\ntitle: Ward\n{}---\n<beat id=\"b\" on=\"morning\" once=\"false\" when=\"holds(close(lobby))\">\n  @narrator: close\n</beat>\n",
            schema.replace("cel(\\\"run.stalker == S\\\")", rule)
        )
    };
    let ok = errors(&doc("cel(\\\"run.stalker == S\\\")"));
    assert!(ok.is_empty(), "{ok:?}");
    let flipped = errors(&doc("cel(\\\"S != run.stalker\\\")"));
    assert!(flipped.is_empty(), "{flipped:?}");
    let bare = errors(&doc("cel(\\\"run.hp > S\\\")"));
    assert!(
        bare.iter()
            .any(|(c, m)| c == "E-CEL-PROFILE"
                && m.contains("compared with a domain-typed state path")),
        "{bare:?}"
    );
    let untyped = errors(&doc("cel(\\\"run.hp == S\\\")"));
    assert!(
        untyped
            .iter()
            .any(|(c, m)| c == "E-CEL-PROFILE" && m.contains("not typed by a kind")),
        "{untyped:?}"
    );
}

#[test]
fn a_rule_variable_is_grounded_per_member_for_the_evaluator() {
    let text = "---\nkind: lore\nid: lore.ward\ntitle: Ward\n\
                entities:\n  room: { members: [lobby, morgue, chapel] }\n\
                relations:\n  adjacent: { args: [room, room] }\n  close: { args: [room], derive: true }\n\
                rules:\n  - \"close(R) :- adjacent(R, S), cel(\\\"run.stalker == S\\\")\"\n\
                state:\n  run.stalker: { type: { domain: room }, default: morgue, owner: engine }\n---\n";
    let (doc, _) = lute_syntax::parse(text);
    let (folded, _, _) = lute_check::fold_env(&doc, &input(text));
    let rules = lute_check::evaluable_rules(&folded.env.rel_vocab);
    let raws: Vec<&str> = rules.iter().map(|r| r.raw.as_str()).collect();
    assert_eq!(rules.len(), 3, "{raws:?}");
    assert!(raws.iter().any(|r| r.ends_with("[S = chapel]")), "{raws:?}");
    let guards: Vec<String> = rules
        .iter()
        .flat_map(|r| &r.rule.body)
        .filter_map(|l| match l {
            lute_syntax::datalog::BodyLiteral::Guard { cel, .. } => Some(cel.clone()),
            _ => None,
        })
        .collect();
    assert!(
        guards.contains(&"run.stalker == 'chapel'".to_string()),
        "{guards:?}"
    );
}

#[test]
fn a_payload_field_is_typed_and_scoped_to_its_occasion() {
    let ok = errors(&lore(
        "<beat id=\"many\" on=\"summon\" target=\"kind:hero\" once=\"false\" when=\"occasion.payload.copies >= 2\">\n  @narrator: {{occasion.payload.copies}} copies!\n</beat>\n",
    ));
    assert!(ok.is_empty(), "{ok:?}");
    let wrong = errors(&lore(
        "<beat id=\"many\" on=\"summon\" target=\"kind:hero\" once=\"false\">\n  @narrator: hi\n</beat>\n\
         <beat id=\"m\" on=\"morning\" once=\"false\" when=\"occasion.payload.copies >= 2\">\n  @narrator: hi\n</beat>\n",
    ));
    assert!(
        wrong
            .iter()
            .any(|(c, m)| c == "E-UNDECLARED" && m.contains("occasion `morning`'s payload")),
        "{wrong:?}"
    );
    let typo = errors(&lore(
        "<beat id=\"many\" on=\"summon\" target=\"kind:hero\" once=\"false\" when=\"occasion.payload.copy >= 2\">\n  @narrator: hi\n</beat>\n",
    ));
    assert!(typo.iter().any(|(c, _)| c == "E-UNDECLARED"), "{typo:?}");
}
