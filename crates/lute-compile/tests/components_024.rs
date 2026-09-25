//! dsl 0.24.0 §4, compile side: an `effects: true` component's writes lower
//! into the host's command stream at the `::use` (only the arm a literal
//! argument selects), and `{{@p}}` over a `speaker` param renders the cast
//! member's name.
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use lute_check::{parse_meta, resolve_components, CheckInput, Mode};
use lute_compile::compile;
use lute_manifest::provider::ProviderSet;
use lute_manifest::schema::CastMember;
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
        std::env::temp_dir().join(format!("lute_cc024_{}_{}_{}", std::process::id(), n, nanos));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Compile `body` in a scene declaring `run.approval.{isolde,corvin}` with
/// `REACTION` importable and the cast `isolde` ("Isolde Vane") / `corvin`.
fn compile_body(body: &str) -> serde_json::Value {
    let dir = unique_dir();
    std::fs::write(dir.join("reaction.lute"), REACTION).unwrap();
    let text = format!(
        "---\nkind: scene\ncharacter: demo\nseason: 1\nepisode: 1\ncomponents: [reaction.lute]\n\
         state:\n  run.approval.isolde: {{ type: number, default: 0 }}\n  \
         run.approval.corvin: {{ type: number, default: 0 }}\n---\n## Shot 1.\n{body}\n"
    );
    let (doc, _) = lute_syntax::parse(&text);
    let (meta0, _) = parse_meta(&doc.meta, &CapabilitySnapshot::default());
    let mut snapshot = vocab_snapshot();
    for (id, name) in [("isolde", Some("Isolde Vane")), ("corvin", None)] {
        snapshot.cast.insert(
            id.into(),
            CastMember {
                id: id.into(),
                name: name.map(Into::into),
                ..Default::default()
            },
        );
    }
    let input = CheckInput {
        text: text.clone(),
        uri: "scene.lute".into(),
        snapshot,
        providers: ProviderSet::default(),
        mode: Mode::Ci,
        imports: Default::default(),
        components: resolve_components(&dir, &meta0.components, doc.meta.span),
        defaults: Default::default(),
    };
    let artifact = compile(&input).unwrap_or_else(|e| panic!("compiles clean: {e:#?}\n{text}"));
    serde_json::to_value(&artifact).unwrap()
}

const REACTION: &str =
    "---\ncomponent: reaction\neffects: true\nparams:\n  who: speaker\n  delta: number\n---\n\
## Scene 1.\n@narrator: {{@who}} approves.\n<match on=\"@who\">\n\
<when is=\"isolde\">\n::set{run.approval.isolde += @delta}\n</when>\n\
<when is=\"corvin\">\n::set{run.approval.corvin += @delta}\n</when>\n\
<otherwise>\n@narrator: Nobody keeps count.\n</otherwise>\n</match>\n";

fn commands(a: &serde_json::Value) -> &[serde_json::Value] {
    a["commands"].as_array().expect("commands")
}

fn sets(a: &serde_json::Value) -> Vec<(String, String, String)> {
    commands(a)
        .iter()
        .filter(|c| c["kind"] == "set")
        .map(|c| {
            (
                c["path"].as_str().unwrap().to_string(),
                c["op"].as_str().unwrap().to_string(),
                c["value"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

fn texts(a: &serde_json::Value) -> Vec<&str> {
    commands(a)
        .iter()
        .filter(|c| c["kind"] == "line")
        .filter_map(|c| c["text"].as_str())
        .collect()
}

#[test]
fn speaker_param_renders_the_cast_name_and_the_write_lands_in_the_host() {
    let a = compile_body("::use{component=\"reaction\" who=\"isolde\" delta=\"2\"}");
    assert!(
        texts(&a).contains(&"Isolde Vane approves."),
        "{:#?}",
        texts(&a)
    );
    assert_eq!(
        sets(&a),
        vec![("run.approval.isolde".into(), "+=".into(), "2".into())],
        "only the selected arm's write, bound, in the host stream"
    );
    assert!(
        !commands(&a).iter().any(|c| c["kind"] == "match"),
        "a literal speaker arg folds the match away"
    );
}

#[test]
fn each_use_writes_only_its_own_member_and_a_nameless_member_renders_its_id() {
    let a = compile_body(
        "::use{component=\"reaction\" who=\"corvin\" delta=\"-1\"}\n\
         ::use{component=\"reaction\" who=\"narrator\" delta=\"5\"}",
    );
    assert_eq!(
        sets(&a),
        vec![("run.approval.corvin".into(), "+=".into(), "-1".into())]
    );
    let t = texts(&a);
    assert!(t.contains(&"corvin approves."), "{t:#?}");
    assert!(t.contains(&"narrator approves."), "{t:#?}");
    assert!(t.contains(&"Nobody keeps count."), "{t:#?}");
}
