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
            "seasons:\n  2x: { live: \"run.open\" }\n",
            "is no season name",
        ),
        ("seasons:\n  harvest: { live: \"\" }\n", "`live:` is empty"),
        (
            "seasons:\n  harvest: { live: \"run.open\", opens: 3 }\n",
            "unknown field `opens`",
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
