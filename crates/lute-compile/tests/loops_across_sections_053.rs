//! dsl 0.53.0 §9.3, §9.5 (#231, Review053 R2/R3): the compiler's own stage
//! walk iterates a loop region across sections, so a sectioned document and
//! the same document with its headings removed emit the same commands apart
//! from positions and the `sections` table.

use lute_check::{CheckInput, Mode};
use serde_json::Value;

const FRONT: &str =
    "---\nkind: scene\nid: flow\nenums:\n  action:\n    members: [show, hide]\n    \
                     exits: [hide]\n  anchor:\n    members: [left, center, right]\n    \
                     default: center\n---\n";

/// `text`'s compiled commands with every position replaced by its rank in
/// document order (positions carry the section number; their order does
/// not change).
fn commands(text: &str) -> Vec<Value> {
    compiled(text, Default::default())
}

/// [`commands`] of `text` with its imported `components`.
fn compiled(text: &str, components: lute_check::ComponentSet) -> Vec<Value> {
    let input = CheckInput::new(
        text.to_string(),
        "flow".into(),
        Mode::Author,
        lute_check::InputSources::from_parts(
            lute_manifest::core::load_core_snapshot(),
            Default::default(),
            Default::default(),
            components,
            Default::default(),
        ),
    );
    let ir = lute_compile::compile(&input).expect("the fixture compiles");
    let json = serde_json::to_value(&ir).expect("an artifact serializes");
    let mut commands = json["commands"].as_array().expect("commands").clone();
    let positions: Vec<String> = commands
        .iter()
        .filter_map(|c| c["position"].as_str().map(str::to_string))
        .collect();
    for c in &mut commands {
        rank(c, &positions);
    }
    commands
}

fn rank(v: &mut Value, positions: &[String]) {
    match v {
        Value::String(s) => {
            if let Some(i) = positions.iter().position(|p| p == s) {
                *s = format!("#{i}");
            }
        }
        Value::Array(items) => {
            for i in items {
                rank(i, positions);
            }
        }
        Value::Object(map) => {
            for i in map.values_mut() {
                rank(i, positions);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

/// `body`'s sections as written, and flat (headings removed).
fn both(body: &str) -> (Vec<Value>, Vec<Value>) {
    let sectioned = format!("{FRONT}## One\n\n{body}\n");
    let flat_body: String = body
        .lines()
        .filter(|l| !l.starts_with("## "))
        .map(|l| format!("{l}\n"))
        .collect();
    let flat = format!("{FRONT}## One\n\n{flat_body}");
    (commands(&sectioned), commands(&flat))
}

/// The provenance rules of `cmds` (the injections at the head).
fn injected(cmds: &[Value]) -> Vec<String> {
    cmds.iter()
        .filter_map(|c| c["provenance"]["by"].as_str().map(str::to_string))
        .collect()
}

/// The head is in section 1, `mira` exits in section 3: at the head she may
/// be off stage, so the `::actor` after the head is a fresh show with its
/// injected anchor in both layouts. Failing with the compiler's walk left
/// per section (the head kept her on stage, no injection).
#[test]
fn a_later_section_staging_change_compiles_as_flat() {
    let (sectioned, flat) = both(
        "::actor{character=\"mira\"}\n::label{name=\"top\"}\n@mira: Again.\n\
         ::actor{character=\"mira\"}\n@mira: Here.\n\n## Two\n\n@narrator: Meanwhile.\n\n\
         ## Three\n\n::actor{character=\"mira\" action=\"hide\"}\n::jump{to=\"top\" max=\"2\"}",
    );
    assert_eq!(sectioned, flat);
    assert!(
        injected(&sectioned).contains(&"auto-anchor-on-show".to_string()),
        "{sectioned:?}"
    );
}

/// Two back edges to one head, the later after the exit in a later section:
/// the head sees the exit, and the layouts agree.
#[test]
fn two_back_edges_compile_as_flat() {
    let (sectioned, flat) = both(
        "::actor{character=\"mira\"}\n::label{name=\"top\"}\n@mira: Again.\n\
         ::actor{character=\"mira\"}\n::jump{to=\"top\" max=\"2\"}\n\n## Two\n\n\
         ::actor{character=\"mira\" action=\"hide\"}\n::jump{to=\"top\" max=\"3\"}",
    );
    assert_eq!(sectioned, flat);
    assert!(
        injected(&sectioned).contains(&"auto-anchor-on-show".to_string()),
        "{sectioned:?}"
    );
}

/// The commands of `body` using component `loopy`, a single-section loop
/// whose label is `label`.
fn with_component(label: &str, body: &str) -> Vec<Value> {
    let dir = lute_scratch::ScratchDir::new();
    std::fs::write(
        dir.join("loopy.lute"),
        format!(
            "---\ncomponent: loopy\n---\n## One\n\n::label{{name=\"{label}\"}}\n\
             ::actor{{character=\"mira\"}}\n@mira: Again.\n::jump{{to=\"{label}\" max=\"2\"}}\n"
        ),
    )
    .unwrap();
    let front = FRONT
        .strip_suffix("---\n")
        .expect("FRONT closes its frontmatter");
    let text = format!("{front}components: [loopy.lute]\n---\n## One\n\n{body}\n");
    let (doc, _) = lute_syntax::parse(&text);
    let (meta, _) = lute_check::parse_meta(
        &doc.meta,
        &lute_manifest::snapshot::CapabilitySnapshot::default(),
    );
    let components = lute_check::resolve_components(&dir, &meta.components, doc.meta.span);
    compiled(&text, components)
}

/// A component's labels are its own scope: its loop region stays inside its
/// expansion, so a caller loop of the same name after it, in the same
/// section or the next one, leaves its head alone (failing before: the
/// caller's hide reached the component's head and its `::actor` took an
/// injected `center` anchor over the caller's `left`).
#[test]
fn a_component_loop_stays_inside_its_expansion() {
    for between in ["\n", "\n\n## Two\n\n"] {
        let body = format!(
            "::actor{{character=\"mira\" anchor=\"left\"}}\n::use{{component=\"loopy\"}}{between}\
             ::label{{name=\"top\"}}\n@narrator: Later.\n::actor{{character=\"mira\" action=\"hide\"}}\n\
             ::jump{{to=\"top\" max=\"2\"}}"
        );
        let same = with_component("top", &body);
        assert!(
            !injected(&same).contains(&"auto-anchor-on-show".to_string()),
            "{between:?}: {same:?}"
        );
        assert_eq!(
            injected(&same),
            injected(&with_component("again", &body)),
            "{between:?}"
        );
    }
}
