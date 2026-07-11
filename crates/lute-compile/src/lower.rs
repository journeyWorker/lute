//! Pass-1 direct lowering (§5): each primitive node → its typed record,
//! schema-driven and pure. `addr`/`lineId`/`voiceKey` stay empty here — the
//! addressing pass (Task 11) owns identity; the stage walker (Tasks 8–9)
//! owns order, stamps, and injection.

use std::collections::BTreeMap;

use lute_manifest::schema::{DirectiveDecl, WriteDecl, WriteValue};
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_manifest::types::{Literal, PathSegment, Type};
use lute_syntax::ast::{Assert, Attr, AttrValue, Directive, Line, Retract, Set};

use crate::ir::*;
use crate::normalize::{COMPONENT_BEGIN, COMPONENT_END};

/// Bare-ident delivery flag (dsl 0.2.2 §D7: `mono`/`os`/`vo`, `AttrValue::
/// BoolTrue` by grammar convention — the checker (`content_line.rs`) gates
/// at-most-one before this ever runs, so priority among the three is moot
/// on a checked document; still deterministic when called directly (unit
/// tests, or an author-mode document with the conflict warning suppressed).
fn has_delivery_flag(attrs: &[Attr], key: &str) -> bool {
    attrs.iter().any(|a| a.key == key && matches!(a.value, AttrValue::BoolTrue))
}

pub fn lower_line(line: &Line) -> Command {
    let get = |k: &str| attr_string(&line.attrs, k);
    let role = if line.speaker == "narrator" {
        Role::Narration
    } else if has_delivery_flag(&line.attrs, "mono") {
        Role::Monologue
    } else if has_delivery_flag(&line.attrs, "vo") {
        Role::Voiceover
    } else if has_delivery_flag(&line.attrs, "os") {
        Role::Offscreen
    } else {
        Role::Dialogue
    };
    Command::Line(LineCmd {
        addr: String::new(),
        role,
        speaker: line.speaker.clone(),
        text: line.text.clone(),
        emotion: get("emotion"),
        variant: get("variant").and_then(|v| v.parse::<i64>().ok()),
        action: get("action"),
        dialog_motion: get("dialogMotion"),
        as_label: get("as"),
        line_id: String::new(),
        voice_key: None,
        placeholders: line.interps.iter().map(placeholder_from_interp).collect(),
        code: get("code"),
        stamp: Stamp::default(),
    })
}

pub fn lower_set(set: &Set) -> Command {
    Command::Set(SetCmd {
        addr: String::new(),
        path: set.path.clone(),
        op: set.op.clone(),
        value: set.expr.raw.clone(),
        expr: crate::expr::lower_expr(&set.expr.raw),
        stamp: Stamp::default(),
    })
}

/// A [`FactTerm`] as its ground string (dsl 0.3.0 §5): `Ident` verbatim,
/// `Bool` as `"true"`/`"false"`, `Wildcard` as `"_"` (retract-pattern-only —
/// never emitted from an `::assert`, checker-enforced `E-RETRACT-WILDCARD-
/// ASSERT`).
fn fact_term_string(t: &lute_syntax::datalog::FactTerm) -> String {
    use lute_syntax::datalog::FactTerm;
    match t {
        FactTerm::Ident(s) => s.clone(),
        FactTerm::Bool(b) => b.to_string(),
        FactTerm::Wildcard => "_".to_string(),
    }
}

/// Lower an `::assert{ GroundFact }` (dsl 0.3.0 §5) to its delta command
/// record. Emitted as DATA only (D1) — no evaluation, no fact store; the
/// engine applies the write. The D13 malformed-parse sentinel
/// (`pattern.relation.is_empty()`) never reaches here: compile is check-
/// gated (`lib.rs`'s D6 gate) and a sentinel pattern is always paired with
/// an `E-DATALOG-PARSE`/`E-DATALOG-FUNCTION` Error diagnostic.
pub fn lower_assert(a: &Assert) -> Command {
    Command::Assert(AssertCmd {
        addr: String::new(),
        relation: a.pattern.relation.clone(),
        args: a.pattern.args.iter().map(|arg| fact_term_string(&arg.term)).collect(),
        stamp: Stamp::default(),
    })
}

/// Lower a `::retract{ RetractPattern }` (dsl 0.3.0 §5) to its delta command
/// record — mirrors [`lower_assert`]; `_` wildcard args pass through as
/// `"_"` verbatim (§5 RetractPattern).
pub fn lower_retract(r: &Retract) -> Command {
    Command::Retract(RetractCmd {
        addr: String::new(),
        relation: r.pattern.relation.clone(),
        args: r.pattern.args.iter().map(|arg| fact_term_string(&arg.term)).collect(),
        stamp: Stamp::default(),
    })
}

/// Lower one directive. `None` for `::use` and the component sentinels (the
/// walker consumes those); `Some(Command::Other(..))` for plugin directives.
pub fn lower_directive(dir: &Directive, snapshot: &CapabilitySnapshot) -> Option<Command> {
    let get = |k: &str| attr_string(&dir.attrs, k);
    let get_f64 = |k: &str| attr_f64(&dir.attrs, k);
    let get_bool = |k: &str| attr_bool(&dir.attrs, k);
    let stamp = Stamp {
        wait: effective_wait(dir, snapshot),
        duration: get_f64("duration"),
        delay: get_f64("delay"),
        ..Stamp::default()
    };
    Some(match dir.tag.as_str() {
        "bg" => Command::Background(BackgroundCmd {
            addr: String::new(),
            location: get("location"),
            time: get("time"),
            asset_id: get("assetId"),
            stamp,
        }),
        "music" => Command::Music(MusicCmd {
            addr: String::new(),
            action: get("action").unwrap_or_default(),
            mood: get("mood"),
            volume: get("volume"),
            asset_id: get("assetId"),
            track: get("track"),
            stamp,
        }),
        "sfx" => Command::Sfx(SfxCmd {
            addr: String::new(),
            sound: get("sound"),
            asset_id: get("assetId"),
            name: get("name"),
            stamp,
        }),
        "vfx" => Command::Vfx(VfxCmd {
            addr: String::new(),
            vfx_type: get("type").unwrap_or_default(),
            label: get("label"),
            transition: get("transition"),
            stamp,
        }),
        "auto" => {
            let action = get("action");
            let exit = match action.as_deref() {
                Some(a) if is_exit_action(a) => Some(true),
                _ => None,
            };
            Command::Sprite(SpriteCmd {
                addr: String::new(),
                character: get("character").unwrap_or_default(),
                anchor: get("anchor"),
                action,
                exit,
                pos_reset: None,
                preload: None,
                emotion: None,
                costume: None,
                stamp,
            })
        }
        "camera" => Command::Camera(CameraCmd {
            addr: String::new(),
            focus: get("focus"),
            zoom: get_f64("zoom"),
            move_x: get_f64("move-x"),
            move_y: get_f64("move-y"),
            shake: get_f64("shake"),
            reset: get_bool("reset"),
            easing: get("easing"),
            stamp,
        }),
        "cut" => Command::Cut(CutCmd {
            addr: String::new(),
            asset_id: get("assetId").unwrap_or_default(),
            action: get("action"),
            full: get_bool("full"),
            stamp,
        }),
        "video" => Command::Video(VideoCmd {
            addr: String::new(),
            asset_id: get("assetId").unwrap_or_default(),
            action: get("action"),
            stamp,
        }),
        // `COMPONENT_BEGIN`/`END`: normalization sentinels → no record. `use`:
        // DEFENSIVE/unreachable — normalize.rs fail-louds a timeline-clip `::use`
        // (E-COMPILE-COMPONENT) so `compile()` aborts at the §5 diag gate before any
        // artifact is kept; a Node-position `::use` is already expanded away (D8).
        "use" | COMPONENT_BEGIN | COMPONENT_END => return None,
        _ => {
            // Plugin passthrough (plan spec-gap note 1): fields typed via the
            // directive's manifest AttrDecls when the decl is known.
            let decl = snapshot.directive(&dir.tag);
            let mut fields = BTreeMap::new();
            for a in &dir.attrs {
                if a.key == "wait" || a.key == "duration" || a.key == "delay" {
                    continue; // already resolved into the stamp
                }
                fields.insert(a.key.clone(), attr_json(a, decl));
            }
            // IR A12: resolve the manifest directive's declared `effects.writes`
            // into artifact-local bindings (fromAttr templates substituted).
            let effects = decl
                .and_then(|d| d.effects.as_ref())
                .map(|eff| eff.writes.iter().map(|w| resolve_effect(w, dir)).collect())
                .unwrap_or_default();
            Command::Other(OtherCmd {
                addr: String::new(),
                tag: dir.tag.clone(),
                fields,
                effects,
                stamp,
            })
        }
    })
}

/// Resolved effective blocking (§4.3 / IR A8): author `wait` attr → manifest
/// `AttrDecl.default` → builtin fallback. The wait-family (compile-IR §4.4) is
/// `bg`/`video` (default `true`) and `cut`/`camera` (default `false`, v1
/// non-blocking); `camera` is normally resolved by its manifest decl above and
/// is listed here for completeness. `plugin` directives flow through steps 1–2
/// (author → manifest, else none). `music`/`sfx`/`vfx`/`sprite` define no
/// `wait` (§4.4) → `None` → the field is omitted, keeping them byte-stable.
///
/// Step 1 (author override) is only *reachable* through `compile()`'s D6 gate
/// for directives whose manifest declares a `wait` attr — `video`/`camera`
/// (dsl §999). `bg`/`cut` declare no `wait`, so an authored `wait` on them is
/// rejected `E-UNKNOWN-ATTR` and never reaches here; they always carry the
/// fixed resolved default (`bg`→`true`, `cut`→`false`).
pub fn effective_wait(dir: &Directive, snapshot: &CapabilitySnapshot) -> Option<bool> {
    if let Some(b) = attr_bool(&dir.attrs, "wait") {
        return Some(b);
    }
    if let Some(decl) = snapshot.directive(&dir.tag) {
        if let Some(a) = decl.attrs.iter().find(|a| a.name == "wait") {
            if let Some(Literal::Bool(b)) = &a.default {
                return Some(*b);
            }
        }
    }
    match dir.tag.as_str() {
        "bg" | "video" => Some(true),
        "cut" | "camera" => Some(false),
        _ => None,
    }
}

/// dsl Appendix A `::auto` exit vocabulary (mirrors `lute-check::inject`'s
/// private helper byte-for-byte).
fn is_exit_action(action: &str) -> bool {
    action.starts_with("fade-out") || action.starts_with("exit") || action == "hide"
}

pub(crate) fn attr_string(attrs: &[Attr], key: &str) -> Option<String> {
    attrs.iter().find(|a| a.key == key).map(|a| match &a.value {
        AttrValue::Str(s) => s.clone(),
        AttrValue::Ref(slot) => slot.raw.clone(),
        AttrValue::BoolTrue => "true".to_string(),
    })
}

fn attr_f64(attrs: &[Attr], key: &str) -> Option<f64> {
    attr_string(attrs, key).and_then(|s| s.parse::<f64>().ok())
}

pub(crate) fn attr_bool(attrs: &[Attr], key: &str) -> Option<bool> {
    attrs
        .iter()
        .find(|a| a.key == key)
        .and_then(|a| match &a.value {
            AttrValue::BoolTrue => Some(true),
            AttrValue::Str(s) => match s.as_str() {
                "true" => Some(true),
                "false" => Some(false),
                _ => None,
            },
            AttrValue::Ref(_) => None,
        })
}

fn attr_json(attr: &Attr, decl: Option<&DirectiveDecl>) -> serde_json::Value {
    let ty = decl
        .and_then(|d| d.attrs.iter().find(|a| a.name == attr.key))
        .map(|a| &a.ty);
    match &attr.value {
        AttrValue::BoolTrue => serde_json::Value::Bool(true),
        AttrValue::Ref(slot) => serde_json::Value::String(slot.raw.clone()),
        AttrValue::Str(s) => match ty {
            Some(Type::Number) => s
                .parse::<f64>()
                .ok()
                .map(serde_json::Value::from)
                .unwrap_or_else(|| serde_json::Value::String(s.clone())),
            Some(Type::Bool) => match s.as_str() {
                "true" => serde_json::Value::Bool(true),
                "false" => serde_json::Value::Bool(false),
                _ => serde_json::Value::String(s.clone()),
            },
            _ => serde_json::Value::String(s.clone()),
        },
    }
}

/// Resolve one manifest `WriteDecl` into an artifact-local [`Effect`] (IR A12).
/// The path is `scope` + each segment joined by `.`, with `fromAttr` segments
/// replaced by the record's attr value (e.g. `resultKey="debut"` → `debut`).
/// The source is the bridge-result key, the `op`/`by` increment (integral `by`),
/// or a literal — all integral-collapsed via `literal_json` (no duplication).
fn resolve_effect(w: &WriteDecl, dir: &Directive) -> Effect {
    let mut segments = vec![w.scope.clone()];
    for seg in &w.path {
        match seg {
            PathSegment::Literal(s) => segments.push(s.clone()),
            PathSegment::FromAttr { from_attr } => {
                segments.push(attr_string(&dir.attrs, &from_attr.name).unwrap_or_default())
            }
        }
    }
    let from = match &w.value {
        WriteValue::FromBridgeResult { from_bridge_result } => EffectSource::BridgeResult {
            bridge_result: from_bridge_result.clone(),
        },
        WriteValue::Op { op, by } => EffectSource::Op {
            op: op.clone(),
            by: crate::literal_json(&Literal::Num(*by)),
        },
        WriteValue::Literal(lit) => EffectSource::Literal(crate::literal_json(lit)),
    };
    Effect {
        path: segments.join("."),
        from,
    }
}

#[cfg(test)]
mod tests {
    use lute_core_span::Severity;
    use lute_manifest::snapshot::CapabilitySnapshot;
    use lute_syntax::ast::Node;

    use super::*;

    fn nodes(body: &str) -> Vec<Node> {
        let src =
            format!("---\nkind: scene\ncharacter: bianca\nseason: 1\nepisode: 2\n---\n\n## Shot 1.\n\n{body}\n");
        let (doc, diags) = lute_syntax::parse(&src);
        assert!(
            diags.iter().all(|d| d.severity != Severity::Error),
            "{diags:#?}"
        );
        doc.shots[0].body.clone()
    }

    fn snap() -> CapabilitySnapshot {
        lute_manifest::core::load_core_snapshot()
    }

    fn lower_first(body: &str) -> serde_json::Value {
        let ns = nodes(body);
        let cmd = match &ns[0] {
            Node::Line(l) => lower_line(l),
            Node::Directive(d) => lower_directive(d, &snap()).expect("lowers"),
            Node::Set(s) => lower_set(s),
            other => panic!("unexpected node {other:?}"),
        };
        serde_json::to_value(&cmd).unwrap()
    }

    #[test]
    fn lowers_assert_and_retract() {
        let ns = nodes("::assert{ inParty(ana) }\n::retract{ atLoc(ana, _) }");
        let Node::Assert(a) = &ns[0] else { panic!() };
        let v = serde_json::to_value(lower_assert(a)).unwrap();
        assert_eq!(v["kind"], "assert");
        assert_eq!(v["relation"], "inParty");
        assert_eq!(v["args"], serde_json::json!(["ana"]));
        let Node::Retract(r) = &ns[1] else { panic!() };
        let v = serde_json::to_value(lower_retract(r)).unwrap();
        assert_eq!(v["kind"], "retract");
        assert_eq!(v["args"], serde_json::json!(["ana", "_"]));
    }

    #[test]
    fn line_roles_derive_from_speaker_and_delivery() {
        let v = lower_first("@narrator: Venny's.");
        assert_eq!(v["kind"], "line");
        assert_eq!(v["role"], "narration");
        let v = lower_first("@fixer{mono}: Hm.");
        assert_eq!(v["role"], "monologue");
        let v = lower_first("@fixer{vo}: Later.");
        assert_eq!(v["role"], "voiceover");
        let v = lower_first("@fixer{os}: Behind the door.");
        assert_eq!(v["role"], "offscreen");
        let v = lower_first(
            "@bianca{code=\"0010\" emotion=\"surprised\" variant=\"0\" as=\"Hostess\"}: Oh!",
        );
        assert_eq!(v["role"], "dialogue");
        assert_eq!(v["speaker"], "bianca");
        assert_eq!(v["text"], "Oh!");
        assert_eq!(v["emotion"], "surprised");
        assert_eq!(v["variant"], 0);
        assert_eq!(v["as"], "Hostess");
        // `code` is consumed into identity later — never a JSON field.
        assert!(v.get("code").is_none());
    }

    #[test]
    fn bg_defaults_wait_true_camera_defaults_wait_false() {
        let v =
            lower_first("::bg{location=\"family_restaurant\" time=\"afternoon\" assetId=\"BG.x\"}");
        assert_eq!(v["kind"], "background");
        assert_eq!(v["location"], "family_restaurant");
        assert_eq!(v["time"], "afternoon");
        assert_eq!(v["assetId"], "BG.x");
        assert_eq!(v["wait"], true);
        let v = lower_first(
            "::camera{focus=\"bianca\" zoom=\"1.1\" move-x=\"0.2\" duration=\"0.5\" easing=\"ease-out\"}",
        );
        assert_eq!(v["kind"], "camera");
        assert_eq!(v["zoom"], 1.1);
        assert_eq!(v["moveX"], 0.2);
        assert_eq!(v["duration"], 0.5);
        assert_eq!(v["easing"], "ease-out");
        assert_eq!(v["wait"], false); // manifest default (arch §1 open question)
        let v = lower_first("::camera{shake=\"0.6\" wait=\"true\"}");
        assert_eq!(v["wait"], true); // author override beats the default
    }

    #[test]
    fn wait_family_materialized_cut_gains_false_others_carry_none() {
        // IR A8 / compile-IR §4.4: the wait-family (bg/video/camera/cut/plugin)
        // MUST carry a resolved `wait`; music/sfx/vfx/sprite carry NO `wait`.
        // THE FIX: `::cut` resolves to a concrete `false` (v1 non-blocking).
        let v = lower_first("::cut{assetId=\"CUT.x\"}");
        assert_eq!(v["kind"], "cut");
        assert_eq!(v["wait"], false);
        // bg/video default true; camera default false (manifest) — unchanged.
        assert_eq!(lower_first("::bg{location=\"r\"}")["wait"], true);
        assert_eq!(
            lower_first("::video{assetId=\"MOVIE.x\" action=\"show\"}")["wait"],
            true
        );
        assert_eq!(lower_first("::camera{shake=\"0.6\"}")["wait"], false);
        // Non-wait families (§4.4) carry NO `wait` key.
        assert!(lower_first("::music{action=\"start\"}").get("wait").is_none());
        assert!(lower_first("::sfx{sound=\"ding\"}").get("wait").is_none());
        assert!(lower_first("::vfx{type=\"whiteOut\"}").get("wait").is_none());
        assert!(
            lower_first("::auto{character=\"bianca\" anchor=\"center\"}")
                .get("wait")
                .is_none()
        );
    }

    #[test]
    fn remaining_core_directives_lower_to_their_kinds() {
        let v = lower_first(
            "::music{action=\"start\" mood=\"peaceful\" volume=\"down\" assetId=\"m.mp3\"}",
        );
        assert_eq!(v["kind"], "music");
        assert_eq!(v["action"], "start");
        assert_eq!(v["mood"], "peaceful");
        assert_eq!(v["volume"], "down");
        let v = lower_first("::sfx{sound=\"hum\" assetId=\"s.mp3\"}");
        assert_eq!(v["kind"], "sfx");
        assert_eq!(v["sound"], "hum");
        let v = lower_first("::vfx{type=\"whiteOut\" transition=\"flash\"}");
        assert_eq!(v["kind"], "vfx");
        assert_eq!(v["vfxType"], "whiteOut");
        let v = lower_first("::cut{assetId=\"CUT.x\" full}");
        assert_eq!(v["kind"], "cut");
        assert_eq!(v["assetId"], "CUT.x");
        assert_eq!(v["full"], true);
        let v = lower_first("::video{assetId=\"MOVIE.x\" action=\"show\"}");
        assert_eq!(v["kind"], "video");
        assert_eq!(v["wait"], true);
        let v = lower_first("::auto{character=\"bianca\" anchor=\"center\" action=\"fade-in-up\"}");
        assert_eq!(v["kind"], "sprite");
        assert_eq!(v["character"], "bianca");
        assert_eq!(v["anchor"], "center");
        assert!(v.get("exit").is_none());
        let v = lower_first("::auto{character=\"bianca\" action=\"fade-out-down\"}");
        assert_eq!(v["exit"], true);
    }

    #[test]
    fn set_ops_lower_verbatim() {
        for op in ["=", "+=", "-=", "*="] {
            let v = lower_first(&format!("::set{{scene.affect.bianca {op} 1}}"));
            assert_eq!(v["kind"], "set");
            assert_eq!(v["path"], "scene.affect.bianca");
            assert_eq!(v["op"], *op);
            assert_eq!(v["value"], "1");
        }
    }

    #[test]
    fn plugin_directive_passes_through_with_typed_fields() {
        // `::minigame` is NOT in the core snapshot => generic passthrough
        // (plan spec-gap note 1); untyped attrs stay strings.
        let v = lower_first("::minigame{kind=\"rhythm\" id=\"x\" resultKey=\"service01\"}");
        assert_eq!(v["kind"], "plugin");
        assert_eq!(v["tag"], "minigame");
        assert_eq!(v["fields"]["kind"], "rhythm");
        assert_eq!(v["fields"]["resultKey"], "service01");
    }

    #[test]
    fn use_and_sentinels_lower_to_nothing() {
        let ns = nodes("::use{component=\"greet\" who=\"bianca\"}");
        let Node::Directive(d) = &ns[0] else { panic!() };
        assert!(lower_directive(d, &snap()).is_none());
        let begin = lute_syntax::ast::Directive {
            tag: crate::normalize::COMPONENT_BEGIN.to_string(),
            attrs: Vec::new(),
            span: d.span,
        };
        assert!(lower_directive(&begin, &snap()).is_none());
    }

    #[test]
    fn camera_shake_and_zoom_serialize_as_json_numbers() {
        // IR A10: typed numeric camera attrs are JSON numbers, not strings.
        // `shake` must match `zoom`/`moveX`/`moveY` (the audit found it emitted
        // as the string "0.4" beside `zoom: 1.2`).
        let v = lower_first("::camera{shake=\"0.4\" zoom=\"1.2\"}");
        assert_eq!(v["kind"], "camera");
        assert!(v["shake"].is_number(), "shake must be a JSON number, got {}", v["shake"]);
        assert_eq!(v["shake"], 0.4);
        assert!(v["zoom"].is_number(), "zoom must be a JSON number, got {}", v["zoom"]);
        assert_eq!(v["zoom"], 1.2);
    }

    #[test]
    fn camera_bool_attr_serializes_as_json_bool() {
        // IR A10: a typed bool attr is a JSON bool, not a string (confirms the
        // existing `get_bool` coercion for core records).
        let v = lower_first("::camera{shake=\"0.4\" reset=\"true\"}");
        assert!(v["reset"].is_boolean(), "reset must be a JSON bool, got {}", v["reset"]);
        assert_eq!(v["reset"], true);
    }

    #[test]
    fn sprite_record_omits_costume_until_cast_ships() {
        // IR A1 (schema-only): `costume` is always None until the character-cast
        // plugin ships, so it never serializes (skip-if-none).
        let v = lower_first("::auto{character=\"bianca\" anchor=\"center\"}");
        assert_eq!(v["kind"], "sprite");
        assert!(v.get("costume").is_none(), "costume must be absent, got {:?}", v.get("costume"));
    }
}
