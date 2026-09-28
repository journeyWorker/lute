//! dsl 0.27.0 §5: seasons and `once="week"` in the checker — a schema's
//! `seasons:` shape, one window per season across the imports, every use of
//! a season name (`season.<name>.*` state, `once: season:<name>`,
//! `tier="season:<name>"`) against the declared ones, and `once="week"` on
//! entries and bundle beats against the clock's `week:`.
use lute_check::{check, resolve_imports, CheckInput, Mode, SchemaImports};
use lute_core_span::{Diagnostic, Severity, Span};
use lute_manifest::schema::{OccasionDecl, OccasionSelect};
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

/// `files` written to a fresh directory and imported (every one) through
/// the same resolver `check-project` uses.
fn imports(files: &[(&str, &str)]) -> SchemaImports {
    let n = UNIQ.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("lute_season_027_{}_{n}", std::process::id()));
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

fn diagnostics(text: &str, imports: SchemaImports) -> Vec<Diagnostic> {
    let mut snapshot = lute_manifest::core::load_core_snapshot();
    snapshot.occasions.insert(
        "chime".into(),
        OccasionDecl {
            name: "chime".into(),
            select: OccasionSelect::First,
            ..Default::default()
        },
    );
    check(&CheckInput {
        text: text.to_string(),
        uri: "season".into(),
        snapshot,
        providers: Default::default(),
        mode: Mode::Author,
        imports,
        components: Default::default(),
        defaults: Default::default(),
    })
    .diagnostics
}

/// The messages of every error `code` in `ds`.
fn with_code<'a>(ds: &'a [Diagnostic], code: &str) -> Vec<&'a str> {
    ds.iter()
        .filter(|d| d.severity == Severity::Error && d.code == code)
        .map(|d| d.message.as_str())
        .collect()
}

const STATE: &str = "state:\n  run.day: { type: number, default: 1, owner: engine }\n  \
                     run.open: { type: bool, default: false }\n";

/// A schema declaring `harvest` (plus `extra`, the state and a clock).
fn world(extra_state: &str, clock: &str) -> String {
    format!("{STATE}{extra_state}{clock}seasons:\n  harvest: {{ live: \"run.open\" }}\n")
}

const DAY: &str = "clock:\n  day: run.day\n";
const WEEK: &str = "clock:\n  day: run.day\n  week: { length: 7, first: 0 }\n";

fn lore(body: &str) -> String {
    format!("---\nkind: lore\nid: l\ntitle: L\n---\n{body}")
}

const QUIET: &str = "<entry id=\"e\" on=\"chime\" once=\"run\">\n  @narrator: hi\n</entry>\n";

#[test]
fn a_season_declared_twice_with_different_windows_is_one_error() {
    let a = "seasons:\n  harvest: { live: \"run.open\" }\n";
    let clash = "seasons:\n  harvest: { live: \"run.day >= 3\" }\n";
    let imps = imports(&[
        ("a.schema.yaml", &format!("{STATE}{a}")),
        ("b.schema.yaml", clash),
    ]);
    assert!(imps.diags.is_empty(), "{:#?}", imps.diags);
    let ds = diagnostics(&lore(QUIET), imps);
    let errs = with_code(&ds, "E-SEASON-DECL");
    assert_eq!(errs.len(), 1, "{ds:#?}");
    assert!(
        errs[0].contains("season `harvest` is declared twice")
            && errs[0].contains("run.open")
            && errs[0].contains("run.day >= 3"),
        "{}",
        errs[0]
    );
    // The same window declared twice is one season.
    let imps = imports(&[
        ("a.schema.yaml", &format!("{STATE}{a}")),
        ("b.schema.yaml", a),
    ]);
    let ds = diagnostics(&lore(QUIET), imps);
    assert!(with_code(&ds, "E-SEASON-DECL").is_empty(), "{ds:#?}");
}

#[test]
fn a_malformed_seasons_block_is_a_season_decl_error() {
    for (seasons, needle) in [
        ("seasons: [harvest]\n", "maps each season name"),
        (
            "seasons:\n  lantern fest: { live: \"run.open\" }\n",
            "season name `lantern fest` is not a name: letters, digits, `_` or `-`, not \
             starting with `-`",
        ),
        (
            "seasons:\n  harvest: { live: \"\" }\n",
            "season `harvest` has no condition",
        ),
        (
            "seasons:\n  harvest:\n",
            "season `harvest` has no condition",
        ),
        (
            "seasons:\n  harvest: { live: \"run.open\", opens: 3 }\n",
            "season `harvest` has no key `opens`",
        ),
        (
            "seasons:\n  harvest: { lvie: \"run.open\" }\n",
            "has no key `lvie` — did you mean `live`?",
        ),
        (
            "seasons:\n  harvest: { live: true }\n",
            "`live: true` is not a condition string — quote it",
        ),
    ] {
        let imps = imports(&[("w.schema.yaml", &format!("{STATE}{seasons}"))]);
        let season_diags: Vec<&Diagnostic> = imps
            .diags
            .iter()
            .flat_map(|d| d.related.iter().map(|r| &r.diagnostic))
            .filter(|d| d.code == "E-SEASON-DECL")
            .collect();
        assert_eq!(season_diags.len(), 1, "{seasons}: {:#?}", imps.diags);
        assert!(
            season_diags[0].message.contains(needle),
            "{seasons}: {}",
            season_diags[0].message
        );
        // A malformed season declares nothing usable: it is either absent or
        // registered with an empty `live` (so its state paths don't cascade).
        assert!(
            imps.seasons
                .iter()
                .all(|(_, s, _)| s.values().all(|d| d.live.is_empty())),
            "{seasons}: {:?}",
            imps.seasons
        );
    }
}

/// A bad season entry is reported at the entry — its name, or its `live:`
/// — not at the `seasons:` line above them (line 4 here).
#[test]
fn a_bad_season_entry_is_reported_at_its_own_key() {
    for (seasons, at) in [
        ("seasons:\n  lantern fest: { live: \"run.open\" }\n", (5, 3)),
        ("seasons:\n  harvest: { live: true }\n", (5, 14)),
        ("seasons:\n  harvest:\n    live: \"\"\n", (6, 5)),
    ] {
        let imps = imports(&[("w.schema.yaml", &format!("{STATE}{seasons}"))]);
        let found: Vec<(u32, u32)> = imps
            .diags
            .iter()
            .flat_map(|d| d.related.iter().map(|r| &r.diagnostic))
            .filter(|d| d.code == "E-SEASON-DECL")
            .map(|d| (d.span.line, d.span.column))
            .collect();
        assert_eq!(found, [at], "{seasons}: {:#?}", imps.diags);
    }
}

#[test]
fn a_season_may_map_straight_to_its_condition() {
    let imps = imports(&[(
        "w.schema.yaml",
        &format!("{STATE}seasons:\n  harvest: \"run.open\"\n"),
    )]);
    assert!(imps.diags.is_empty(), "{:#?}", imps.diags);
    let live: Vec<&str> = imps
        .seasons
        .iter()
        .flat_map(|(_, s, _)| s.values().map(|d| d.live.as_str()))
        .collect();
    assert_eq!(live, ["run.open"]);
}

#[test]
fn a_state_path_under_an_undeclared_season_is_named() {
    let imps = imports(&[(
        "w.schema.yaml",
        &world(
            "  season.harvest.tokens: { type: number, default: 0 }\n  \
             season.harvst.gold: { type: number, default: 0 }\n",
            DAY,
        ),
    )]);
    assert!(imps.diags.is_empty(), "{:#?}", imps.diags);
    let ds = diagnostics(&lore(QUIET), imps);
    let errs = with_code(&ds, "E-SEASON-DECL");
    assert_eq!(errs.len(), 1, "{ds:#?}");
    assert!(
        errs[0].contains("state path `season.harvst.gold`")
            && errs[0].contains("declared: harvest")
            && errs[0].contains("did you mean `harvest`?"),
        "{}",
        errs[0]
    );
}

#[test]
fn an_undeclared_season_in_once_or_tier_is_named_with_a_suggestion() {
    let schema = world("", DAY);
    // A scene's `once:`.
    let scene =
        "---\nkind: scene\nid: s\non: chime\nonce: season:harvst\n---\n\n## S\n\n@narrator: x\n";
    let ds = diagnostics(scene, imports(&[("w.schema.yaml", &schema)]));
    let errs = with_code(&ds, "E-SEASON-DECL");
    assert_eq!(errs.len(), 1, "{ds:#?}");
    assert!(
        errs[0].contains("`once:` names season `harvst`")
            && errs[0].contains("did you mean `harvest`?"),
        "{}",
        errs[0]
    );
    // A quest's `tier=`.
    let quest = "---\nkind: quest\nid: q\n---\n\n\
                 <quest id=\"missions\" title=\"Missions\" tier=\"season:harvst\" start=\"run.open\">\n  \
                 <objective id=\"one\" title=\"One\" done=\"run.day >= 2\"/>\n</quest>\n";
    let ds = diagnostics(quest, imports(&[("w.schema.yaml", &schema)]));
    let errs = with_code(&ds, "E-SEASON-DECL");
    assert_eq!(errs.len(), 1, "{ds:#?}");
    assert!(
        errs[0].contains("`tier=\"season:harvst\"` names season `harvst`")
            && errs[0].contains("did you mean `harvest`?"),
        "{}",
        errs[0]
    );
    // The declared spelling is fine.
    let ds = diagnostics(
        &quest.replace("season:harvst", "season:harvest"),
        imports(&[("w.schema.yaml", &schema)]),
    );
    assert!(with_code(&ds, "E-SEASON-DECL").is_empty(), "{ds:#?}");
    // With no `seasons:` at all, the message says so.
    let ds = diagnostics(
        quest,
        imports(&[("w.schema.yaml", &format!("{STATE}{DAY}"))]),
    );
    let errs = with_code(&ds, "E-SEASON-DECL");
    assert_eq!(errs.len(), 1, "{ds:#?}");
    assert!(
        errs[0].contains("the project declares no `seasons:`"),
        "{}",
        errs[0]
    );
}

#[test]
fn entry_and_bundle_beat_once_season_is_checked_against_the_declared() {
    let body = |season: &str| {
        lore(&format!(
            "<entry id=\"e\" on=\"chime\" once=\"season:{season}\">\n  @narrator: hi\n</entry>\n\
             <beat id=\"b\" on=\"chime\" once=\"season:{season}\">\n  @narrator: yo\n</beat>\n"
        ))
    };
    let schema = world("", DAY);
    let ds = diagnostics(&body("harvest"), imports(&[("w.schema.yaml", &schema)]));
    assert!(with_code(&ds, "E-SEASON-DECL").is_empty(), "{ds:#?}");
    assert!(with_code(&ds, "E-BEAT-ATTR").is_empty(), "{ds:#?}");
    let ds = diagnostics(&body("harvst"), imports(&[("w.schema.yaml", &schema)]));
    let errs = with_code(&ds, "E-SEASON-DECL");
    assert_eq!(errs.len(), 2, "{ds:#?}");
    assert!(
        errs.iter().all(
            |m| m.contains("`once=\"season:harvst\"` names season `harvst`")
                && m.contains("did you mean `harvest`?")
        ),
        "{errs:#?}"
    );
}

#[test]
fn entry_and_bundle_beat_once_week_needs_a_clock_week() {
    let body = lore(
        "<entry id=\"e\" on=\"chime\" once=\"week\">\n  @narrator: hi\n</entry>\n\
         <beat id=\"b\" on=\"chime\" once=\"week\">\n  @narrator: yo\n</beat>\n",
    );
    let ds = diagnostics(&body, imports(&[("w.schema.yaml", &world("", WEEK))]));
    assert!(with_code(&ds, "E-BEAT-ATTR").is_empty(), "{ds:#?}");
    let ds = diagnostics(&body, imports(&[("w.schema.yaml", &world("", DAY))]));
    let errs = with_code(&ds, "E-BEAT-ATTR");
    assert_eq!(errs.len(), 2, "{ds:#?}");
    assert!(
        errs.iter()
            .all(|m| m.contains("`once=\"week\"`") && m.contains("declares no `week:`")),
        "{errs:#?}"
    );
}

/// The messages of every warning `code` in `ds`.
fn warned<'a>(ds: &'a [Diagnostic], code: &str) -> Vec<&'a str> {
    ds.iter()
        .filter(|d| d.severity == Severity::Warning && d.code == code)
        .map(|d| d.message.as_str())
        .collect()
}

/// `once: season:X` only sets how long a beat stays spent: a beat whose
/// `when` does not imply the season's `live` plays while the season has
/// never opened, and says so.
#[test]
fn a_season_spent_beat_without_the_season_gate_warns() {
    let schema = format!("{}defs:\n  harvestOn: \"run.open\"\n", world("", DAY));
    let imps = || imports(&[("w.schema.yaml", &schema)]);
    // A scene beat with no `when`.
    let scene = |when: &str| {
        format!(
            "---\nkind: scene\nid: s\non: chime\nonce: season:harvest\n{when}---\n\n## S\n\n\
             @narrator: x\n"
        )
    };
    let ds = diagnostics(&scene(""), imps());
    let w = warned(&ds, "W-SEASON-UNGATED");
    assert_eq!(w.len(), 1, "{ds:#?}");
    assert!(
        w[0].contains("beat `s` is not gated on season `harvest`")
            && w[0].contains("`live: run.open`")
            && w[0].contains("plays even while season `harvest` has never opened")
            && w[0].contains("add `when: \"@harvestOn\"`"),
        "{}",
        w[0]
    );
    // A `when` that does not imply the window warns; one that does (the
    // condition itself, a def of it, as one conjunct) does not.
    let ds = diagnostics(&scene("when: \"run.day > 2\"\n"), imps());
    let w = warned(&ds, "W-SEASON-UNGATED");
    assert_eq!(w.len(), 1, "{ds:#?}");
    assert!(
        w[0].contains("add `&& @harvestOn` to its `when`"),
        "{}",
        w[0]
    );
    for when in ["run.open", "@harvestOn", "run.day > 2 && @harvestOn"] {
        let ds = diagnostics(&scene(&format!("when: \"{when}\"\n")), imps());
        assert!(
            warned(&ds, "W-SEASON-UNGATED").is_empty(),
            "{when}: {ds:#?}"
        );
    }
    // A beat spent per run is not about the season.
    let ds = diagnostics(&scene("").replace("season:harvest", "run"), imps());
    assert!(warned(&ds, "W-SEASON-UNGATED").is_empty(), "{ds:#?}");
}

#[test]
fn season_spent_entries_bundle_beats_and_season_tier_quests_need_the_gate() {
    let imps = || imports(&[("w.schema.yaml", &world("", DAY))]);
    let body = |when: &str| {
        lore(&format!(
            "<entry id=\"e\" on=\"chime\" once=\"season:harvest\"{when}>\n  @narrator: hi\n\
             </entry>\n<beat id=\"b\" on=\"chime\" once=\"season:harvest\"{when}>\n  \
             @narrator: yo\n</beat>\n"
        ))
    };
    let ds = diagnostics(&body(""), imps());
    let w = warned(&ds, "W-SEASON-UNGATED");
    assert_eq!(w.len(), 2, "{ds:#?}");
    assert!(
        w[0].contains("entry `e`") && w[1].contains("beat `l.b`"),
        "{w:#?}"
    );
    // No def reads the window: the fix names the condition itself.
    assert!(
        w.iter()
            .all(|m| m.contains("add `when=\"run.open\"`, or a def that reads it")),
        "{w:#?}"
    );
    let ds = diagnostics(&body(" when=\"run.open\""), imps());
    assert!(warned(&ds, "W-SEASON-UNGATED").is_empty(), "{ds:#?}");

    let quest = |start: &str| {
        format!(
            "---\nkind: quest\nid: q\n---\n\n<quest id=\"missions\" title=\"Missions\" \
             tier=\"season:harvest\" start=\"{start}\">\n  \
             <objective id=\"one\" title=\"One\" done=\"run.day >= 2\"/>\n</quest>\n"
        )
    };
    let ds = diagnostics(&quest("run.day > 1"), imps());
    let w = warned(&ds, "W-SEASON-UNGATED");
    assert_eq!(w.len(), 1, "{ds:#?}");
    assert!(
        w[0].contains("quest `missions` is not gated on season `harvest`")
            && w[0].contains("add `&& run.open` to its `start`"),
        "{}",
        w[0]
    );
    let ds = diagnostics(&quest("run.open && run.day > 1"), imps());
    assert!(warned(&ds, "W-SEASON-UNGATED").is_empty(), "{ds:#?}");
}

/// drowned-crown NEW-1: a `live` over an operator the decider cannot model
/// (`%`) is still implied by a condition that has it as a conjunct — the
/// def itself, repeated, or beside another conjunct — and a condition that
/// reads none of its paths still warns; one that reads them otherwise is
/// not judged either way.
#[test]
fn a_modulo_season_window_is_implied_by_its_own_conjunct() {
    let schema = "state:\n  user.runs: { type: number, default: 0 }\n  \
                  run.day: { type: number, default: 1, owner: engine }\n\
                  clock:\n  day: run.day\n\
                  defs:\n  neapLive: \"user.runs % 6 >= 4\"\n\
                  seasons:\n  neap: { live: \"@neapLive\" }\n";
    let imps = || imports(&[("w.schema.yaml", schema)]);
    let entry = |when: &str| {
        lore(&format!(
            "<entry id=\"e\" on=\"chime\" once=\"season:neap\" when=\"{when}\">\n  \
             @narrator: hi\n</entry>\n"
        ))
    };
    for when in [
        "@neapLive",
        "@neapLive && @neapLive",
        "run.day > 2 && @neapLive",
        "user.runs % 6 >= 4",
        "user.runs % 6 == 5",
    ] {
        let ds = diagnostics(&entry(when), imps());
        assert!(
            warned(&ds, "W-SEASON-UNGATED").is_empty(),
            "{when}: {ds:#?}"
        );
    }
    let ds = diagnostics(&entry("run.day > 2"), imps());
    assert_eq!(warned(&ds, "W-SEASON-UNGATED").len(), 1, "{ds:#?}");
    let quest = |start: &str| {
        format!(
            "---\nkind: quest\nid: q\n---\n\n<quest id=\"salvage\" title=\"Salvage\" \
             tier=\"season:neap\" start=\"{start}\">\n  \
             <objective id=\"one\" title=\"One\" done=\"run.day >= 2\"/>\n</quest>\n"
        )
    };
    let ds = diagnostics(&quest("@neapLive"), imps());
    assert!(warned(&ds, "W-SEASON-UNGATED").is_empty(), "{ds:#?}");
    let ds = diagnostics(&quest("true"), imps());
    assert_eq!(warned(&ds, "W-SEASON-UNGATED").len(), 1, "{ds:#?}");
}

/// A season whose name is refused (`E-RESERVED-NAME`) is that error's: a
/// quest on it is not also told it is ungated.
#[test]
fn a_refused_season_name_is_not_judged_for_its_gate() {
    let schema = format!("{STATE}seasons:\n  run: {{ live: \"run.open\" }}\n");
    let text = "---\nkind: quest\nid: q\n---\n\n<quest id=\"hush\" title=\"Hush\" \
                tier=\"season:run\" start=\"true\">\n  \
                <objective id=\"one\" title=\"One\" done=\"run.day >= 2\"/>\n</quest>\n";
    let imps = imports(&[("w.schema.yaml", &schema)]);
    let import_diags = imps.diags.clone();
    let ds = diagnostics(text, imps);
    // The refusal is nested under the import's `E-USES-PARSE`.
    let all = format!("{import_diags:?} {ds:?}");
    assert!(all.contains("E-RESERVED-NAME"), "{all}");
    assert!(warned(&ds, "W-SEASON-UNGATED").is_empty(), "{ds:#?}");
}

/// An illegal `once` / `tier` value that names a declared season — its bare
/// name, its state-path spelling `season.<name>`, or a near miss — suggests
/// `season:<name>`; a quest tier's wrong case suggests the legal tier.
#[test]
fn an_illegal_once_or_tier_naming_a_declared_season_suggests_its_spelling() {
    let rel = "entities:\n  crew: { members: [ana] }\n\
               relations:\n  met: { args: [crew], tier: harvest }\n";
    let imps = || imports(&[("w.schema.yaml", &world(rel, DAY))]);
    let meant = "did you mean `season:harvest`?";
    for once in ["harvest", "season.harvest", "harvst"] {
        let scene = format!(
            "---\nkind: scene\nid: s\non: chime\nonce: {once}\n---\n\n## S\n\n@narrator: x\n"
        );
        let ds = diagnostics(&scene, imps());
        let errs = with_code(&ds, "E-BEAT-ATTR");
        assert!(
            errs.len() == 1 && errs[0].ends_with(meant),
            "{once}: {ds:#?}"
        );
    }
    let ds = diagnostics(
        &lore(
            "<entry id=\"e\" on=\"chime\" once=\"harvest\">\n  @narrator: hi\n</entry>\n\
             <beat id=\"b\" on=\"chime\" once=\"season.harvest\">\n  @narrator: yo\n</beat>\n",
        ),
        imps(),
    );
    let errs = with_code(&ds, "E-BEAT-ATTR");
    assert!(
        errs.len() == 2 && errs.iter().all(|m| m.ends_with(meant)),
        "{ds:#?}"
    );
    let quest = |tier: &str| {
        format!(
            "---\nkind: quest\nid: q\n---\n\n<quest id=\"missions\" title=\"Missions\" \
             tier=\"{tier}\" start=\"run.open\">\n  \
             <objective id=\"one\" title=\"One\" done=\"run.day >= 2\"/>\n</quest>\n"
        )
    };
    let ds = diagnostics(&quest("harvest"), imps());
    let errs = with_code(&ds, "E-ATTR-TYPE");
    assert!(errs.len() == 1 && errs[0].ends_with(meant), "{ds:#?}");
    let ds = diagnostics(&quest("Run"), imps());
    let errs = with_code(&ds, "E-ATTR-TYPE");
    assert!(
        errs.len() == 1 && errs[0].ends_with("did you mean `run`?"),
        "{ds:#?}"
    );
    // A relation's `tier:` in the imported schema.
    let ds = diagnostics(&lore(QUIET), imps());
    let errs = with_code(&ds, "E-RELATION-DOMAIN");
    assert!(
        !errs.is_empty()
            && errs
                .iter()
                .all(|m| m.contains("`tier: harvest` — did you mean `season:harvest`?")),
        "{ds:#?}"
    );
    // A value naming no declared season keeps the plain legal-value list.
    let ds = diagnostics(&quest("storm"), imps());
    let errs = with_code(&ds, "E-ATTR-TYPE");
    assert!(
        errs.len() == 1 && !errs[0].contains("did you mean"),
        "{ds:#?}"
    );
}

/// An undeclared `once: season:<x>` is reported at the value, not the line.
#[test]
fn an_undeclared_scene_once_season_is_anchored_at_the_value() {
    let scene =
        "---\nkind: scene\nid: s\non: chime\nonce: season:harvst\n---\n\n## S\n\n@narrator: x\n";
    let ds = diagnostics(scene, imports(&[("w.schema.yaml", &world("", DAY))]));
    let d = ds
        .iter()
        .find(|d| d.code == "E-SEASON-DECL")
        .unwrap_or_else(|| panic!("{ds:#?}"));
    assert_eq!((d.span.line, d.span.column), (5, 7), "{d:#?}");
    assert_eq!(&scene[d.span.byte_start..d.span.byte_end], "season:harvst");
}

/// A scene's legacy `season:` (the episode number) naming a declared season
/// is an error that says how a scene is tied to a season, in place of the
/// legacy-key warning; a number stays the legacy warning.
#[test]
fn a_legacy_scene_season_naming_a_declared_season_is_an_error() {
    let scene = |season: &str| {
        format!(
            "---\nkind: scene\nid: s\nseason: {season}\non: chime\n---\n\n## S\n\n@narrator: x\n"
        )
    };
    let imps = || imports(&[("w.schema.yaml", &world("", DAY))]);
    let ds = diagnostics(&scene("harvest"), imps());
    let errs = with_code(&ds, "E-SEASON-DECL");
    assert_eq!(errs.len(), 1, "{ds:#?}");
    assert!(
        errs[0].contains("legacy episode number")
            && errs[0].contains("`once: season:harvest`")
            && errs[0].contains("`when: \"run.open\"`"),
        "{}",
        errs[0]
    );
    assert!(warned(&ds, "W-META-LEGACY").is_empty(), "{ds:#?}");
    let ds = diagnostics(&scene("2"), imps());
    assert!(with_code(&ds, "E-SEASON-DECL").is_empty(), "{ds:#?}");
    assert_eq!(warned(&ds, "W-META-LEGACY").len(), 1, "{ds:#?}");
}
