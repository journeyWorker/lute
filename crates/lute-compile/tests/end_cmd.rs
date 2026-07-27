//! dsl 0.8.0 `::end` lowering: the `end` record's exact JSON shape, its
//! byte-stability contract (an `::end`-free document is untouched), and its
//! participation in addressing.

use lute_check::{CheckInput, Mode};
use lute_compile::{compile, Command};

fn input(text: &str) -> CheckInput {
    CheckInput {
        text: text.to_string(),
        uri: "end_cmd".into(),
        snapshot: lute_manifest::core::load_core_snapshot(),
        providers: Default::default(),
        mode: Mode::Ci,
        imports: Default::default(),
        components: Default::default(),
    }
}

const HDR: &str = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n\n## Shot 1.\n\n";

fn artifact(text: &str) -> lute_compile::Artifact {
    match compile(&input(text)) {
        Ok(a) => a,
        Err(diags) => panic!("compile must be clean: {diags:?}"),
    }
}

#[test]
fn end_lowers_to_an_end_record_with_its_reason() {
    let a = artifact(&format!("{HDR}@narrator: bye\n::end{{reason=\"completed\"}}\n"));
    let end = a.commands.last().expect("a record");
    assert!(matches!(end, Command::End(_)), "last record is `end`: {end:?}");
    assert_eq!(
        serde_json::to_string(end).unwrap(),
        r#"{"kind":"end","addr":"001-0200","reason":"completed"}"#
    );
}

#[test]
fn a_reasonless_end_omits_the_field() {
    let a = artifact(&format!("{HDR}::end\n"));
    assert_eq!(
        serde_json::to_string(&a.commands[0]).unwrap(),
        r#"{"kind":"end","addr":"001-0100"}"#
    );
}

/// `::end` declares no `wait` attr, so `effective_wait` resolves `None` and
/// the stamp stays entirely omitted — the same treatment `music`/`sfx`/`vfx`
/// get (compile-IR §4.4). A stamped `wait:false` here would be a silent
/// behavioral claim the DSL never made.
#[test]
fn end_carries_no_resolved_wait() {
    let a = artifact(&format!("{HDR}::end{{reason=\"x\"}}\n"));
    let json = serde_json::to_string(&a.commands[0]).unwrap();
    assert!(!json.contains("wait"), "{json}");
    assert!(!json.contains("duration"), "{json}");
}

/// The 0.8.0 byte-stability contract: a document that uses none of the new
/// feature must serialize exactly as it did before `Command::End` existed.
#[test]
fn a_document_without_end_is_unchanged() {
    let a = artifact(&format!("{HDR}@narrator: hello\n::sfx{{sound=\"door\"}}\n"));
    assert_eq!(
        serde_json::to_string(&a.commands).unwrap(),
        r#"[{"kind":"line","addr":"001-0100","role":"narration","speaker":"narrator","text":"hello","lineId":"x.s01ep01.narrator_0010"},{"kind":"sfx","addr":"001-0200","sound":"door"}]"#
    );
}

/// `::end` is an ordinary addressed record: it consumes an address slot and
/// never disturbs the +100 gapping of the records around it.
#[test]
fn end_occupies_an_ordinary_address_slot() {
    let a = artifact(&format!(
        "{HDR}@narrator: a\n::end{{reason=\"r\"}}\n\n## Shot 2.\n\n@narrator: b\n"
    ));
    let addrs: Vec<&str> = a
        .commands
        .iter()
        .map(|c| match c {
            Command::Line(l) => l.addr.as_str(),
            Command::End(e) => e.addr.as_str(),
            other => panic!("unexpected record {other:?}"),
        })
        .collect();
    assert_eq!(addrs, ["001-0100", "001-0200", "002-0100"]);
}

/// Two arms, each terminating with its own reason — the conformance fixture's
/// shape, asserted at the unit level so a lowering regression is caught here
/// before the fixture's checked-in artifact is regenerated.
#[test]
fn each_choice_arm_lowers_its_own_end() {
    let a = artifact(&format!(
        "{HDR}<branch id=\"exit\">\n\
         <choice id=\"stay\" label=\"Stay\">\n::end{{reason=\"stayed\"}}\n</choice>\n\
         <choice id=\"leave\" label=\"Leave\">\n::end{{reason=\"left\"}}\n</choice>\n\
         </branch>\n"
    ));
    let reasons: Vec<&str> = a
        .commands
        .iter()
        .filter_map(|c| match c {
            Command::End(e) => Some(e.reason.as_deref().unwrap_or("")),
            _ => None,
        })
        .collect();
    assert_eq!(reasons, ["stayed", "left"]);
}
