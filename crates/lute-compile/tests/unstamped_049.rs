//! 0.49.0 §4.1 (#157): every core directive is classified by whether its
//! lowering carries a plugin stamp, and §4.2 (#174): a content line's `wait`
//! lowers to the line record's `timing.wait`.

use std::collections::BTreeMap;

use lute_check::{check, CheckInput, Mode};
use lute_compile::compile;
use lute_manifest::core::UNSTAMPED_DIRECTIVES;
use lute_manifest::schema::AttrDecl;
use lute_manifest::snapshot::{CapabilitySnapshot, Domain};
use lute_manifest::types::Type;

/// The shared test vocabulary, a `sequence` domain (`intro`) and the plugin
/// stamp `bonusId: string`.
fn snapshot() -> CapabilitySnapshot {
    let mut snap = lute_test_vocab::vocab_snapshot();
    let sequence = Domain {
        members: vec!["intro".into()],
        open: false,
        default: None,
        exits: Vec::new(),
        stops: Vec::new(),
        labels: BTreeMap::new(),
    };
    snap.enums
        .insert("sequence".into(), sequence.members.clone());
    snap.domains.insert("sequence".into(), sequence);
    snap.stamp_attrs.insert(
        "bonusId".into(),
        AttrDecl {
            name: "bonusId".into(),
            required: false,
            ty: Type::Str,
            default: None,
        },
    );
    snap
}

fn input(text: &str) -> CheckInput {
    CheckInput::new(
        text.to_string(),
        "unstamped_049".into(),
        Mode::Ci,
        lute_check::InputSources::from_parts(
            snapshot(),
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
        ),
    )
}

fn commands(text: &str) -> Vec<serde_json::Value> {
    let artifact =
        compile(&input(text)).unwrap_or_else(|e| panic!("compiles clean: {e:?}\n{text}"));
    let json = serde_json::to_value(&artifact).expect("artifact serializes");
    json["commands"].as_array().expect("commands array").clone()
}

const HDR: &str = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n\n## Shot 1.\n\n";

/// A document using `::<tag>` once, carrying `bonusId="b"`, for every core
/// directive with a stamp carrier. A core directive added without a row here
/// fails [`every_core_directive_is_classified_by_its_stamp_carrier`].
fn stamped_sample(tag: &str) -> Option<String> {
    let s = r#"bonusId="b""#;
    let body = match tag {
        "bg" => format!("::bg{{location=\"cafe\" time=\"night\" {s}}}\n@narrator: a\n"),
        "music" => format!("::music{{playback=\"start\" mood=\"tense\" {s}}}\n@narrator: a\n"),
        "sfx" => format!("::sfx{{assetId=\"x\" {s}}}\n@narrator: a\n"),
        "actor" => format!(
            "::actor{{character=\"elena\" anchor=\"center\" action=\"fadeInUp\" {s}}}\n@elena: a\n"
        ),
        "vfx" => format!("::vfx{{type=\"rain\" {s}}}\n@narrator: a\n"),
        "cg" => format!("::cg{{assetId=\"c\" {s}}}\n@narrator: a\n"),
        "video" => format!("::video{{assetId=\"v\" {s}}}\n@narrator: a\n"),
        "camera" => format!("::camera{{move=\"shake\" {s}}}\n@narrator: a\n"),
        "sequence" => format!("::sequence{{name=\"intro\" {s}}}\n@narrator: a\n"),
        "end" => format!("@narrator: a\n::end{{{s}}}\n"),
        _ => return None,
    };
    Some(format!("{HDR}{body}"))
}

/// The same for a directive with no stamp carrier: the stamp is refused.
fn unstamped_sample(tag: &str) -> Option<String> {
    let s = r#"bonusId="b""#;
    let body = match tag {
        "jump" => format!("@narrator: a\n::jump{{to=\"end\" {s}}}\n@narrator: b\n::label{{name=\"end\"}}\n@narrator: c\n"),
        "label" => format!("@narrator: a\n::label{{name=\"end\" {s}}}\n@narrator: b\n"),
        "clear" => format!(
            "::actor{{character=\"elena\" anchor=\"center\" action=\"fadeInUp\"}}\n@elena: a\n::clear{{{s}}}\n@narrator: b\n"
        ),
        _ => return None,
    };
    Some(format!("{HDR}{body}"))
}

/// #157: every core directive outside `UNSTAMPED_DIRECTIVES`, lowered with
/// one plugin stamp, yields a record whose `stamps` carries it; every one in
/// the set reports `E-UNKNOWN-ATTR` at the stamp. The list is the core
/// snapshot's, so a new core directive must be classified. Fails on 0.48.1,
/// where the unstamped three checked clean.
#[test]
fn every_core_directive_is_classified_by_its_stamp_carrier() {
    let core = lute_manifest::core::load_core_snapshot();
    for tag in UNSTAMPED_DIRECTIVES {
        assert!(
            core.directives.contains_key(tag),
            "`{tag}` is not a core directive"
        );
    }
    for tag in core.directives.keys() {
        if UNSTAMPED_DIRECTIVES.contains(&tag.as_str()) {
            let text = unstamped_sample(tag)
                .unwrap_or_else(|| panic!("no unstamped sample for `::{tag}`"));
            let diags = check(&input(&text)).diagnostics;
            let refused: Vec<_> = diags
                .iter()
                .filter(|d| d.code == "E-UNKNOWN-ATTR")
                .map(|d| &text[d.span.byte_start..d.span.byte_end])
                .collect();
            assert_eq!(refused, vec![r#"bonusId="b""#], "`::{tag}`: {diags:#?}");
            continue;
        }
        let text = stamped_sample(tag).unwrap_or_else(|| {
            panic!(
                "core directive `::{tag}` is not classified: add it to `UNSTAMPED_DIRECTIVES` \
                 (0.49.0 §4.1) or give it a stamped sample here"
            )
        });
        let cmds = commands(&text);
        assert!(
            cmds.iter()
                .any(|c| c["stamps"] == serde_json::json!({ "bonusId": "b" })),
            "`::{tag}` dropped its plugin stamp: {cmds:#?}"
        );
    }
}

/// #174: a line's authored `wait` is its record's `timing.wait`; absent
/// stays omitted. Fails on 0.48.1 (`wait` on a line did not check).
#[test]
fn a_line_wait_lowers_to_timing_wait() {
    let text = format!(
        "{HDR}@elena{{wait=\"false\"}}: a\n::sfx{{assetId=\"x\"}}\n@elena{{wait=\"true\"}}: b\n@elena: c\n"
    );
    let lines: Vec<serde_json::Value> = commands(&text)
        .into_iter()
        .filter(|c| c["kind"] == "line")
        .collect();
    assert_eq!(lines.len(), 3, "{lines:#?}");
    assert_eq!(lines[0]["timing"], serde_json::json!({ "wait": false }));
    assert_eq!(lines[1]["timing"], serde_json::json!({ "wait": true }));
    assert!(lines[2].get("timing").is_none(), "{:#?}", lines[2]);
    assert!(
        lines.iter().all(|l| l.get("wait").is_none()),
        "`wait` is no line field: {lines:#?}"
    );
}

/// F2-R1: a bool constant reference on a line's `wait` folds into
/// `timing.wait` (it is not dropped).
#[test]
fn a_line_wait_bool_reference_lowers_to_timing_wait() {
    let text = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n\
                defs:\n  off: { type: bool, cel: \"false\" }\n---\n\n## Shot 1.\n\n\
                @elena{wait=@off}: a\n";
    let line = commands(text)
        .into_iter()
        .find(|c| c["kind"] == "line")
        .expect("a line record");
    assert_eq!(
        line["timing"],
        serde_json::json!({ "wait": false }),
        "{line:#?}"
    );
}

/// #193: a bool constant reference on a directive's undeclared `wait` folds
/// into the record's `timing.wait`, overriding the kind's default.
#[test]
fn a_directive_wait_bool_reference_lowers_to_timing_wait() {
    let text = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n\
                defs:\n  off: { type: bool, cel: \"false\" }\n---\n\n## Shot 1.\n\n\
                ::bg{location=\"cafe\" wait=@off}\n@narrator: a\n";
    let bg = commands(text)
        .into_iter()
        .find(|c| c["kind"] == "bg")
        .expect("a bg record");
    assert_eq!(
        bg["timing"],
        serde_json::json!({ "wait": false }),
        "{bg:#?}"
    );
}
