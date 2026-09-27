//! dsl 0.28.0 §1: comparisons are typed (`E-CEL-TYPE`), every condition
//! slot runs the same literal checks, a def body's reads are checked where
//! it is expanded, and a def that reads one member of a family by index
//! takes the family's type.
use lute_check::{check, resolve_imports, CheckInput, Mode, SchemaImports};
use lute_core_span::{Diagnostic, Severity, Span};
use lute_manifest::schema::{OccasionDecl, OccasionSelect, OccasionTarget};
use std::sync::atomic::{AtomicU64, Ordering};

static UNIQ: AtomicU64 = AtomicU64::new(0);

fn zero_span() -> Span {
    Span {
        byte_start: 0,
        byte_end: 0,
        line: 1,
        column: 1,
        utf16_range: (0, 0),
    }
}

/// `files` written to a fresh directory and imported through the resolver
/// `check-project` uses.
fn imports(files: &[(&str, &str)]) -> SchemaImports {
    let n = UNIQ.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("lute_types_028_{}_{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for (name, text) in files {
        std::fs::write(dir.join(name), text).unwrap();
    }
    let names: Vec<String> = files.iter().map(|(n, _)| n.to_string()).collect();
    let out = resolve_imports(&dir, &names, &[], zero_span());
    let _ = std::fs::remove_dir_all(&dir);
    out
}

/// `text` checked with occasion `chime` (untargeted) and `explore` (one
/// raise per `place`, gated by `explore_gate` when it is not empty).
fn diagnostics(text: &str, imports: SchemaImports, explore_gate: &str) -> Vec<Diagnostic> {
    let mut snapshot = lute_manifest::core::load_core_snapshot();
    snapshot.occasions.insert(
        "chime".into(),
        OccasionDecl {
            name: "chime".into(),
            select: OccasionSelect::First,
            ..Default::default()
        },
    );
    snapshot.occasions.insert(
        "explore".into(),
        OccasionDecl {
            name: "explore".into(),
            select: OccasionSelect::First,
            target: OccasionTarget::Domain {
                prefix: "place".into(),
                entity: "place".into(),
                members: None,
            },
            raised_when: (!explore_gate.is_empty()).then(|| explore_gate.to_string()),
            ..Default::default()
        },
    );
    check(&CheckInput {
        text: text.to_string(),
        uri: "types".into(),
        snapshot,
        providers: Default::default(),
        mode: Mode::Author,
        imports,
        components: Default::default(),
        defaults: Default::default(),
    })
    .diagnostics
}

/// Every error, as `(code, message)`.
fn errors(ds: &[Diagnostic]) -> Vec<(&str, &str)> {
    ds.iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| (d.code.as_str(), d.message.as_str()))
        .collect()
}

const WORLD: &str = "state:\n  run.day: { type: number, default: 1, owner: engine }\n  \
                     run.oil: { type: number, default: 1 }\n  \
                     run.knows: { type: bool, default: false }\n  \
                     run.name: { type: string, default: \"minsu\" }\n  \
                     run.hour: { type: { enum: [h02, h03, h04] }, default: h02 }\n  \
                     run.route: { type: { domain: route }, default: none }\n\
                     enums:\n  route: [none, ren, mika]\n\
                     entities:\n  place: { members: [village, lighthouse] }\n";

fn world(extra: &str) -> SchemaImports {
    imports(&[("world.schema.yaml", &format!("{WORLD}{extra}"))])
}

fn lore(body: &str) -> String {
    format!("---\nkind: lore\nid: l\ntitle: L\n---\n{body}")
}

fn beat(lines: &str) -> String {
    lore(&format!(
        "<beat id=\"b\" on=\"chime\" once=\"false\">\n{lines}</beat>\n"
    ))
}

#[test]
fn a_comparison_between_types_is_a_type_error() {
    for (guard, wants) in [
        (
            "visited('l') > 2",
            &["`visited('l')` is a bool", "not how often"][..],
        ),
        ("visited('l') == 1", &["compares a bool with a number"]),
        (
            "run.oil == true",
            &["compares a number with a bool", "never true"],
        ),
        (
            "run.hour >= 'h03'",
            &["`>=` compares numbers", "`run.hour in ['h03', 'h04']`"],
        ),
        ("run.day == 'monday'", &["compares a number with a string"]),
        (
            "run.oil == '3'",
            &["write the number without quotes: `run.oil == 3`"],
        ),
        (
            "run.knows > 0",
            &["test it directly (`run.knows` or `!run.knows`)"],
        ),
        ("run.knows + run.oil > 1", &["cannot be computed"]),
        (
            "run.oil && run.knows",
            &["`&&` takes conditions", "`run.oil > 0`"],
        ),
        ("run.route in [1, 2]", &["never matches"]),
    ] {
        let ds = diagnostics(
            &beat(&format!("  @narrator{{when=\"{guard}\"}}: x\n")),
            world(""),
            "",
        );
        let errs = errors(&ds);
        assert_eq!(errs.len(), 1, "{guard}: {errs:?}");
        let (code, message) = errs[0];
        assert_eq!(code, "E-CEL-TYPE", "{guard}: {message}");
        for want in wants {
            assert!(message.contains(want), "{guard}: {message}");
        }
    }
    // Same-typed comparisons are fine.
    for guard in [
        "run.oil > 0",
        "run.knows == false",
        "run.hour == 'h03'",
        "run.route != 'ren'",
        "run.name + '!' == 'minsu!'",
    ] {
        let ds = diagnostics(
            &beat(&format!("  @narrator{{when=\"{guard}\"}}: x\n")),
            world(""),
            "",
        );
        assert!(errors(&ds).is_empty(), "{guard}: {ds:?}");
    }
}

/// The mistyped comparison is the one report: the guard it makes false is
/// not also a dead arm or an unreachable beat.
#[test]
fn a_mistyped_comparison_owns_the_dead_guard() {
    let text = lore(
        "<beat id=\"b\" on=\"chime\" once=\"false\" when=\"run.knows == 1\">\n  @narrator: x\n</beat>\n\
         <entry id=\"e\" on=\"chime\" when=\"run.day == 'monday'\">\n  @narrator: y\n</entry>\n",
    );
    let ds = diagnostics(&text, world(""), "");
    let codes: Vec<&str> = errors(&ds).iter().map(|(c, _)| *c).collect();
    assert_eq!(codes, ["E-CEL-TYPE", "E-CEL-TYPE"], "{ds:?}");
}

#[test]
fn a_weekday_name_against_the_weekday_number_names_the_label_path() {
    let week = "clock:\n  day: run.day\n  week: { length: 7, first: 0, labels: [Monday, \
                Tuesday, Wednesday, Thursday, Friday, Saturday, Sunday] }\n";
    let ds = diagnostics(
        &beat("  @narrator{when=\"clock.weekday == 'Wednesday'\"}: x\n"),
        world(week),
        "",
    );
    let errs = errors(&ds);
    assert_eq!(errs.len(), 1, "{errs:?}");
    assert!(
        errs[0].1.contains("`clock.weekdayLabel == 'Wednesday'`")
            && errs[0].1.contains("`clock.weekday == 2`"),
        "{errs:?}"
    );
}

#[test]
fn an_is_literal_that_is_no_number_never_matches_a_number() {
    let ds = diagnostics(
        &beat(
            "  <match on=\"run.oil\">\n    <when is=\"<=3\">\n      @narrator: low\n    </when>\n    \
             <otherwise>\n      @narrator: high\n    </otherwise>\n  </match>\n",
        ),
        world(""),
        "",
    );
    let errs = errors(&ds);
    assert_eq!(errs.len(), 1, "{errs:?}");
    assert_eq!(errs[0].0, "E-WHEN-LITERAL-DOMAIN");
    assert!(errs[0].1.contains("write `..3`"), "{errs:?}");
}

/// T1-5: `rearm`, an entry's `when`, `spentBy`, a season's `live:` and a
/// rule `cel()` get the literal-domain check every `when` gets.
#[test]
fn every_condition_slot_checks_its_literals() {
    let foreign = |ds: &[Diagnostic]| -> Vec<String> {
        errors(ds)
            .into_iter()
            .map(|(c, m)| format!("{c} {m}"))
            .collect()
    };
    let quest = "---\nkind: quest\nid: q\ntitle: Q\n---\n\
                 <quest id=\"duty\" title=\"Duty\" rearm=\"run.hour == 'h5'\">\n  \
                 <objective id=\"o\" done=\"run.knows\" />\n</quest>\n";
    let errs = foreign(&diagnostics(quest, world(""), ""));
    assert!(
        errs.len() == 1 && errs[0].starts_with("E-WHEN-LITERAL-DOMAIN") && errs[0].contains("'h5'"),
        "{errs:?}"
    );
    let entry = lore(
        "<entry id=\"e\" on=\"chime\" when=\"run.route == 'rne'\">\n  @narrator: y\n</entry>\n\
         <beat id=\"s\" on=\"chime\" spentBy=\"run.route == 'mkia'\">\n  @narrator: z\n</beat>\n",
    );
    let errs = foreign(&diagnostics(&entry, world(""), ""));
    assert_eq!(errs.len(), 2, "{errs:?}");
    assert!(errs[0].contains("did you mean `'ren'`?"), "{errs:?}");
    assert!(errs[1].contains("did you mean `'mika'`?"), "{errs:?}");
    let seasons = "seasons:\n  fest: { live: \"run.route == 'rne'\" }\n";
    let errs = foreign(&diagnostics(&beat("  @narrator: x\n"), world(seasons), ""));
    assert!(
        errs.len() == 1 && errs[0].contains("season `fest`") && errs[0].contains("'ren'"),
        "{errs:?}"
    );
    let rules = "relations:\n  open: { args: [place], derive: true }\n\
                 rules:\n  - \"open(P) :- place(P), cel(\\\"run.route == 'rne'\\\")\"\n";
    let errs = foreign(&diagnostics(&beat("  @narrator: x\n"), world(rules), ""));
    assert!(
        errs.len() == 1
            && errs[0].starts_with("E-WHEN-LITERAL-DOMAIN")
            && errs[0].contains("'ren'"),
        "{errs:?}"
    );
}

/// T1-5: a gate compares `occasion.target` with the bare members it is
/// raised for; a prefixed member or a typo is named, and no beat is judged
/// dead under the faulty gate.
#[test]
fn a_gate_literal_for_the_target_is_checked_against_its_members() {
    let text = lore("<beat id=\"walk\" on=\"explore\" once=\"false\">\n  @narrator: x\n</beat>\n");
    for (gate, want) in [
        (
            "occasion.target != 'place.village'",
            "did you mean `'village'`? `occasion.target` holds the member alone, without the \
             `place.` prefix",
        ),
        ("occasion.target != 'vilage'", "did you mean `'village'`?"),
    ] {
        let ds = diagnostics(&text, world(""), gate);
        let errs = errors(&ds);
        assert_eq!(errs.len(), 1, "{gate}: {errs:?}");
        assert_eq!(errs[0].0, "E-WHEN-LITERAL-DOMAIN", "{errs:?}");
        assert!(errs[0].1.contains(want), "{gate}: {errs:?}");
    }
    assert!(errors(&diagnostics(
        &text,
        world(""),
        "occasion.target != 'village'"
    ))
    .is_empty());
}

/// T1-5: `terminal:` is judged between steps: reading `occasion.*` there is
/// one error, the same whether or not the document has a kind beat.
#[test]
fn terminal_cannot_read_the_occasion() {
    let terminal = "terminal: \"occasion.target == 'village'\"\n";
    for text in [
        lore("<beat id=\"a\" on=\"chime\" once=\"false\">\n  @narrator: x\n</beat>\n"),
        lore("<beat id=\"a\" on=\"explore\" target=\"kind:place\" once=\"false\">\n  @narrator: x\n</beat>\n"),
    ] {
        let ds = diagnostics(&text, world(terminal), "");
        let errs = errors(&ds);
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert!(
            errs[0].0 == "E-UNDECLARED" && errs[0].1.contains("outside any occasion"),
            "{errs:?}"
        );
    }
}

/// T1-17: a path a def reads is declared, checked where the def is used.
#[test]
fn a_def_body_reads_only_declared_paths() {
    let defs = "defs:\n  a: \"run.nmae == 'x'\"\n  b: \"run.oill >= 2\"\n";
    let text = lore(
        "<beat id=\"s\" on=\"chime\" once=\"false\" when=\"@a\">\n  @narrator{when=\"@b\"}: x\n</beat>\n",
    );
    let ds = diagnostics(&text, world(defs), "");
    let errs = errors(&ds);
    assert_eq!(errs.len(), 2, "{errs:?}");
    assert!(
        errs.iter().any(|(c, m)| *c == "E-UNDECLARED"
            && m.contains("`@a` reads state path `run.nmae`")
            && m.contains("did you mean `run.name`?")),
        "{errs:?}"
    );
    assert!(
        errs.iter()
            .any(|(c, m)| *c == "E-UNDECLARED" && m.contains("`@b` reads state path `run.oill`")),
        "{errs:?}"
    );
}

/// A read or a foreign literal written in a def another def uses names the
/// def that writes it, and the def the site used to reach it.
#[test]
fn a_nested_def_fault_names_the_def_that_writes_it() {
    let defs = "defs:\n  inner: \"run.nmae == 'x'\"\n  outer: \"@inner && run.oil >= 0\"\n  \
                onRen: \"run.route == 'rne'\"\n";
    let text = lore(
        "<beat id=\"s\" on=\"chime\" once=\"false\" when=\"@outer\">\n  \
         @narrator{when=\"@onRen\"}: x\n</beat>\n",
    );
    let ds = diagnostics(&text, world(defs), "");
    let errs = errors(&ds);
    assert!(
        errs.iter().any(|(c, m)| *c == "E-UNDECLARED"
            && m.contains("`@inner` (used by `@outer`) reads state path `run.nmae`")),
        "{errs:?}"
    );
    assert!(
        errs.iter().any(|(c, m)| *c == "E-WHEN-LITERAL-DOMAIN"
            && m.starts_with("in `@onRen`: `'rne'` is not a member")),
        "{errs:?}"
    );
}

/// T3-39: a def that reads one member of an indexed family takes the
/// family's type.
#[test]
fn an_indexed_family_read_has_the_family_type() {
    let family = "  user.bond: { type: number, default: 0, per: place }\n";
    let schema = WORLD.replacen("state:\n", &format!("state:\n{family}"), 1);
    let imps = imports(&[(
        "world.schema.yaml",
        &format!("{schema}defs:\n  bondNow: \"user.bond[occasion.target]\"\n"),
    )]);
    let text = lore(
        "<beat id=\"s\" on=\"explore\" target=\"kind:place\" once=\"false\">\n  @narrator: {{@bondNow}}\n</beat>\n",
    );
    let ds = diagnostics(&text, imps, "");
    assert!(errors(&ds).is_empty(), "{ds:?}");
}

/// `text` checked with one plugin directive `::tip` whose declared
/// `effects.writes` are `writes` (scope, path segments).
fn with_tip(text: &str, writes: &[(&str, &[&str])]) -> Vec<Diagnostic> {
    use lute_manifest::schema::{
        DirectiveDecl, DirectiveEffects, Lowering, OpBy, WriteDecl, WriteValue,
    };
    use lute_manifest::types::PathSegment;
    let mut snapshot = lute_manifest::core::load_core_snapshot();
    snapshot.directives.insert(
        "tip".into(),
        DirectiveDecl {
            name: "tip".into(),
            layer: None,
            attrs: Vec::new(),
            semantics: Vec::new(),
            state: None,
            effects: Some(DirectiveEffects {
                writes: writes
                    .iter()
                    .map(|(scope, path)| WriteDecl {
                        scope: scope.to_string(),
                        path: path
                            .iter()
                            .map(|s| PathSegment::Literal(s.to_string()))
                            .collect(),
                        value: WriteValue::Op {
                            op: "increment".into(),
                            by: OpBy::Num(1.0),
                        },
                    })
                    .collect(),
                asserts: Vec::new(),
                retracts: Vec::new(),
            }),
            bridge: None,
            lower: Lowering::Passthrough,
        },
    );
    check(&CheckInput {
        text: text.to_string(),
        uri: "tip".into(),
        snapshot,
        providers: Default::default(),
        mode: Mode::Author,
        imports: world(""),
        components: Default::default(),
        defaults: Default::default(),
    })
    .diagnostics
}

/// T1-18: every state write a directive declares is judged at each call —
/// an undeclared path (with its did-you-mean), a season path without the
/// season, and the read-only `prev.*` / `clock.*`; a declared path is clean.
#[test]
fn a_directive_s_declared_writes_are_judged_at_the_call() {
    let text = "---\nkind: scene\nid: s\ntitle: S\n---\n## One\n@narrator: hi\n::tip\n";
    let clean = with_tip(text, &[("run", &["oil"])]);
    assert!(errors(&clean).is_empty(), "{clean:?}");
    let ds = with_tip(
        text,
        &[
            ("run", &["oill"]),
            ("season", &["hung"]),
            ("prev", &["run", "oil"]),
            ("clock", &["index"]),
        ],
    );
    let errs = errors(&ds);
    assert_eq!(errs.len(), 4, "{errs:?}");
    for path in ["run.oill", "season.hung", "prev.run.oil", "clock.index"] {
        assert!(
            errs.iter()
                .any(|(_, m)| m.contains(&format!("`{path}`")) && m.contains("::tip")),
            "{path}: {errs:?}"
        );
    }
    assert!(
        errs.iter()
            .any(|(_, m)| m.contains("did you mean `run.oil`?")),
        "{errs:?}"
    );
}

/// T1-19: a stored relation with no `tier:` warns (it is run-tier and a new
/// run clears it); a written tier or a derived relation does not.
#[test]
fn a_relation_without_a_tier_warns() {
    let rels = "relations:\n  owned: { args: [place], reserved: true }\n  \
                seen: { args: [place], tier: user }\n  \
                near: { args: [place], derive: true }\n\
                rules:\n  - \"near(P) :- seen(P)\"\n";
    let ds = diagnostics(&beat("  @narrator: x\n"), world(rels), "");
    let tiers: Vec<&Diagnostic> = ds
        .iter()
        .filter(|d| d.code == "W-RELATION-TIER-IMPLICIT")
        .collect();
    assert_eq!(tiers.len(), 1, "{ds:?}");
    assert!(
        tiers[0].message.contains("`owned`") && tiers[0].message.contains("tier: user"),
        "{tiers:?}"
    );
}

/// T3-8: a misread engine path names what the namespace holds, with the
/// nearest shape filled with the path's own ids — never "declare it in
/// `state:`".
#[test]
fn an_engine_path_typo_names_the_engine_shapes() {
    for (read, near) in [
        ("quest.q1.status == 'active'", "quest.q1.state"),
        ("quest.q1.failedby == 'fail'", "quest.q1.failedBy"),
        ("quest.q1.objective.o.done", "quest.q1.objectives.o.done"),
        (
            "quest.q1.objectives.o.complete",
            "quest.q1.objectives.o.done",
        ),
        ("entry.e1.everread", "entry.e1.everRead"),
        ("entry.e1.seen", "entry.e1.read"),
    ] {
        let text = lore(&format!(
            "<beat id=\"s\" on=\"chime\" once=\"false\" when=\"{read}\">\n  @narrator: x\n</beat>\n"
        ));
        let ds = diagnostics(&text, world(""), "");
        let errs = errors(&ds);
        assert!(
            errs.iter().any(|(c, m)| *c == "E-UNDECLARED"
                && m.contains(&format!("did you mean `{near}`?"))
                && !m.contains("declared in `state:`")),
            "{read}: {errs:?}"
        );
    }
}

/// T3-4: a CEL-profile refusal names the fix — `holds(…)` for a bare
/// relation atom or `has(atom)`, the quest state for `completed()` in a
/// condition, a quoted `visited()` id, and no internal operator names in
/// `after:`.
#[test]
fn a_profile_refusal_names_the_fix() {
    let rels = "relations:\n  knows: { args: [place], tier: run }\n";
    for (when, fix) in [
        ("knows(village)", "holds(knows(village))"),
        ("has(knows(village))", "holds(knows(village))"),
        ("completed('q1')", "quest.q1.state == 'complete'"),
        ("visited(village.gate)", "visited('village.gate')"),
        ("$oil > 1", "did you mean `run.oil`?"),
    ] {
        let text = lore(&format!(
            "<beat id=\"s\" on=\"chime\" once=\"false\" when=\"{when}\">\n  @narrator: x\n</beat>\n"
        ));
        let ds = diagnostics(&text, world(rels), "");
        assert!(
            errors(&ds).iter().any(|(_, m)| m.contains(fix)),
            "{when}: {ds:?}"
        );
    }
    let (_, ds) = lute_check::prereq::parse_prereq(
        "visited('garden') && run.accused == 'magpie'",
        zero_span(),
    );
    assert_eq!(ds.len(), 1, "{ds:?}");
    let m = &ds[0].message;
    assert!(
        m.contains("`run.accused == 'magpie'`")
            && m.contains("move it to `when:`")
            && !m.contains("_==_"),
        "{m}"
    );
    let (_, ds) = lute_check::prereq::parse_prereq("cafe.ok", zero_span());
    assert!(
        ds[0]
            .message
            .contains("did you mean `visited(\"cafe.ok\")`"),
        "{ds:?}"
    );
}

/// T3-45: `per:` reads the kinds of every merged schema, as `subsetOf:` and
/// `add:` do — the family's members and its indexed type included.
#[test]
fn a_per_family_may_index_a_kind_another_schema_declares() {
    let world = WORLD.replace(
        "entities:\n  place: { members: [village, lighthouse] }\n",
        "",
    );
    let family = "  user.bond: { type: number, default: 0, per: place }\n";
    let world = world.replacen("state:\n", &format!("state:\n{family}"), 1);
    let imps = imports(&[
        (
            "world.schema.yaml",
            &format!("{world}defs:\n  bondNow: \"user.bond[occasion.target]\"\n"),
        ),
        (
            "places.schema.yaml",
            "entities:\n  place: { members: [village, lighthouse] }\n",
        ),
    ]);
    let text = lore(
        "<beat id=\"s\" on=\"explore\" target=\"kind:place\" once=\"false\" when=\"user.bond.village >= 0\">\n  @narrator: {{@bondNow}}\n</beat>\n",
    );
    let ds = diagnostics(&text, imps, "");
    assert!(errors(&ds).is_empty(), "{ds:?}");
}

/// T3-45: an enum and an entity kind of one name, across schemas or in
/// one, clash; a path that is a value and another's prefix is refused.
#[test]
fn schemas_are_cross_checked() {
    let imps = imports(&[
        ("world.schema.yaml", WORLD),
        ("clock.schema.yaml", "enums:\n  place: [dawn, dusk]\n"),
    ]);
    let ds = diagnostics(&beat("  @narrator: x\n"), imps, "");
    assert!(
        ds.iter().any(|d| d.code == "E-DOMAIN-NAME-CLASH"
            && d.message.contains("[dawn, dusk]")
            && d.message.contains("[village, lighthouse]")),
        "{ds:?}"
    );
    let text = "---\nkind: lore\nid: l\ntitle: L\nenums:\n  slot: [dawn, dusk]\n\
                entities:\n  slot: { members: [a, b] }\n---\n<beat id=\"b\" on=\"chime\" once=\"false\">\n  @narrator: x\n</beat>\n";
    let ds = diagnostics(text, world(""), "");
    assert!(ds.iter().any(|d| d.code == "E-DOMAIN-NAME-CLASH"), "{ds:?}");
    let prefix = "  run.lanterns: { type: number, default: 0 }\n  \
                  run.lanterns.gold: { type: number, default: 0 }\n";
    let schema = WORLD.replacen("state:\n", &format!("state:\n{prefix}"), 1);
    let ds = diagnostics(
        &beat("  @narrator: x\n"),
        imports(&[("world.schema.yaml", &schema)]),
        "",
    );
    assert!(
        ds.iter().any(|d| d.code == "E-STATE-DECL"
            && d.message.contains("`run.lanterns` is declared as a value")
            && d.message.contains("`run.lanterns.gold`")),
        "{ds:?}"
    );
}

/// A `{ domain: K }` / `{ entity: K }` path must name a declared K, and its
/// `default:` must be one of K's members — played, a value no `<match>` arm
/// takes. An imported schema's is reported at its line; a document's own at
/// the `default:` key; a `per:` family once.
#[test]
fn a_domain_typed_path_names_a_declared_kind_and_defaults_to_a_member() {
    let bad = "  run.r: { type: { domain: route }, default: nnoe }\n  \
               run.where: { type: { entity: place }, default: rof }\n  \
               run.isle: { type: { entity: plac }, default: village }\n  \
               run.ok: { type: { entity: place }, default: village }\n";
    let schema = WORLD.replacen("state:\n", &format!("state:\n{bad}"), 1);
    let ds = diagnostics(
        &beat("  @narrator: x\n"),
        imports(&[("world.schema.yaml", &schema)]),
        "",
    );
    let errs = errors(&ds);
    assert_eq!(errs.len(), 3, "{ds:?}");
    assert!(errs.iter().any(|(c, m)| *c == "E-STATE-DECL"
        && m.contains("`run.r`'s `default: nnoe` is not a member of enum `route` [none, ren, mika] — did you mean `none`?")));
    assert!(errs.iter().any(|(c, m)| *c == "E-STATE-DECL"
        && m.contains("`run.where`'s `default: rof` is not a member of entity kind `place`")));
    assert!(errs.iter().any(|(c, m)| *c == "E-DOMAIN-UNKNOWN"
        && m.contains("`run.isle` is typed `{ entity: plac }`, but `plac` is not a declared enum or entity kind — did you mean `place`?")));

    let text = "---\nkind: lore\nid: l\ntitle: L\nstate:\n  \
                user.bond: { type: { entity: place }, default: lighthose, per: place }\n\
                ---\n<beat id=\"b\" on=\"chime\" once=\"false\">\n  @narrator: x\n</beat>\n";
    let ds = diagnostics(text, world(""), "");
    let decl: Vec<&Diagnostic> = ds.iter().filter(|d| d.code == "E-STATE-DECL").collect();
    assert_eq!(decl.len(), 1, "{ds:?}");
    assert!(
        decl[0]
            .message
            .starts_with("`user.bond`'s `default: lighthose`"),
        "{ds:?}"
    );
    assert_eq!((decl[0].span.line, decl[0].span.column), (6, 41), "{ds:?}");
}
