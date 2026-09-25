//! dsl 0.23.0 §5 (lamplight F2/F30) — a component `string` param interpolated
//! as `{{@p}}` is substituted at expansion: each `::use` site ships its OWN
//! sentence as plain line text (no placeholder, no ref to a param that no
//! longer exists), under that call site's component-scoped `lineId`/`voiceKey`
//! (0.22.0 §11) — so each sentence is its own translation unit.
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use lute_check::{check, parse_meta, resolve_components, resolve_imports, CheckInput, Mode};
use lute_compile::compile;
use lute_compile::locale::{merge_locales, LocaleBundle};
use lute_manifest::provider::ProviderSet;
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_test_vocab::vocab_snapshot;

static UNIQ: AtomicU64 = AtomicU64::new(0);

fn unique_dir() -> PathBuf {
    let n = UNIQ.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir =
        std::env::temp_dir().join(format!("lute_cstr_{}_{}_{}", std::process::id(), n, nanos));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn input_for(dir: &Path, scene_text: &str) -> CheckInput {
    let (doc, _) = lute_syntax::parse(scene_text);
    let (meta0, _) = parse_meta(&doc.meta, &CapabilitySnapshot::default());
    let components = resolve_components(dir, &meta0.components, doc.meta.span);
    let imports = resolve_imports(dir, &meta0.uses, &meta0.extends, doc.meta.span);
    CheckInput {
        text: scene_text.to_string(),
        uri: "scene.lute".into(),
        snapshot: vocab_snapshot(),
        providers: ProviderSet::default(),
        mode: Mode::Ci,
        imports,
        components,
        defaults: Default::default(),
    }
}

const LAMPS_OUT: &str = "---\ncomponent: lampsOut\nparams:\n  memory: string\n---\n\
## Lamps out\n@narrator{code=\"0010\"}: The lamps gutter.\n\
@marina{code=\"0020\"}: {{@memory}} Then nothing.\n";

const SCENE: &str = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n\
components: [lamps-out.component.lute]\n---\n## Shot 1.\n\
::use{component=\"lampsOut\" memory=\"Four minutes.\"}\n\
@narrator{code=\"0100\"}: The train rolls on.\n\
::use{component=\"lampsOut\" memory=\"The Ossery lamps, again.\"}\n";

#[test]
fn each_use_site_ships_its_own_sentence_under_its_own_line_id() {
    let dir = unique_dir();
    std::fs::write(dir.join("lamps-out.component.lute"), LAMPS_OUT).unwrap();
    let input = input_for(&dir, SCENE);
    let res = check(&input);
    assert!(
        !res.diagnostics.iter().any(|d| d.code.starts_with("E-")),
        "{:#?}",
        res.diagnostics
    );
    let mut artifact = compile(&input).unwrap_or_else(|e| panic!("{e:#?}"));

    let lines: Vec<serde_json::Value> = artifact
        .commands
        .iter()
        .map(|c| serde_json::to_value(c).unwrap())
        .filter(|v| v["kind"] == "line" && v["text"].as_str().unwrap().ends_with("Then nothing."))
        .collect();
    assert_eq!(lines.len(), 2, "one thought line per `::use`: {lines:#?}");
    assert_eq!(lines[0]["text"], "Four minutes. Then nothing.");
    assert_eq!(lines[1]["text"], "The Ossery lamps, again. Then nothing.");
    for l in &lines {
        assert!(
            l.get("placeholders").is_none(),
            "the param is spliced, never left as a placeholder: {l:#?}"
        );
    }
    let ids: Vec<&str> = lines
        .iter()
        .map(|l| l["lineId"].as_str().unwrap())
        .collect();
    assert_ne!(ids[0], ids[1], "each call site is its own translation unit");
    assert_ne!(lines[0]["voiceKey"], lines[1]["voiceKey"]);
    assert!(
        ids[0].contains("lampsOut#1") && ids[1].contains("lampsOut#2"),
        "keyed by the host's component scope (0.22.0 §11): {ids:?}"
    );

    // A locale bundle keyed by those lineIds translates each call site's
    // sentence separately.
    let bundle = LocaleBundle::from_triples([
        (ids[0].to_string(), "ko".to_string(), "4분.".to_string()),
        (
            ids[1].to_string(),
            "ko".to_string(),
            "오서리의 등불.".to_string(),
        ),
    ]);
    merge_locales(&mut artifact, &bundle);
    let texts: Vec<serde_json::Value> = artifact
        .commands
        .iter()
        .map(|c| serde_json::to_value(c).unwrap())
        .filter(|v| ids.contains(&v["lineId"].as_str().unwrap_or("")))
        .map(|v| v["texts"]["ko"].clone())
        .collect();
    assert_eq!(texts, ["4분.", "오서리의 등불."]);
}
