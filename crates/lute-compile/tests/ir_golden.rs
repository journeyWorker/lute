//! Golden-per-kind serialization (spec §4.4): one exact-JSON assertion per
//! record kind pins the discriminator, camelCase field names, field order,
//! and None-field omission — the byte-stability contract everything else
//! (addresses, e2e goldens, determinism) rides on.

use std::collections::BTreeMap;

use lute_compile::*;
use lute_compile::expr::{ExprNode, LitVal};
fn j(cmd: &Command) -> String {
    serde_json::to_string(cmd).unwrap()
}

#[test]
fn line_serializes_per_spec() {
    let cmd = Command::Line(LineCmd {
        position: "002-0500".into(),
        role: Role::Dialogue,
        speaker: "marina".into(),
        text: "Oh!".into(),
        emotion: Some("surprised".into()),
        variant: Some(0),
        action: None,
        dialog_motion: None,
        as_label: None,
        line_id: "marina.s01ep02.marina_0010".into(),
        voice_key: "marina-0010".into(),
        placeholders: Vec::new(),
        texts: Default::default(),
        code: Some("0010".into()),
        stamp: Stamp::default(),
    });
    // `code` is #[serde(skip)] — the 3-id model (§4.2) admits no code field.
    assert_eq!(
        j(&cmd),
        r#"{"kind":"line","category":"content","position":"002-0500","role":"dialogue","speaker":"marina","text":"Oh!","emotion":"surprised","variant":0,"lineId":"marina.s01ep02.marina_0010","voiceKey":"marina-0010"}"#
    );
}

#[test]
fn narration_line_carries_voice_key() {
    let cmd = Command::Line(LineCmd {
        position: "002-0400".into(),
        role: Role::Narration,
        speaker: "narrator".into(),
        text: "A hostess walked over.".into(),
        emotion: None,
        variant: None,
        action: None,
        dialog_motion: None,
        as_label: None,
        line_id: "marina.s01ep02.narrator_0010".into(),
        voice_key: "marina.s01ep02.narrator-0010".into(),
        placeholders: Vec::new(),
        texts: Default::default(),
        code: None,
        stamp: Stamp::default(),
    });
    // dsl 0.37.0 D6: every line carries its voice join, narration included.
    assert!(j(&cmd).contains(r#""voiceKey":"marina.s01ep02.narrator-0010""#));
}

#[test]
fn offscreen_line_serializes_as_voiced() {
    // dsl 0.37.0 D6: `{os}` lowers to `Role::Os`, serialized as `os`.
    let cmd = Command::Line(LineCmd {
        position: "002-0600".into(),
        role: Role::Os,
        speaker: "fixer".into(),
        text: "Behind the door.".into(),
        emotion: None,
        variant: None,
        action: None,
        dialog_motion: None,
        as_label: None,
        line_id: "marina.s01ep02.fixer_0010".into(),
        voice_key: "fixer-0010".into(),
        placeholders: Vec::new(),
        texts: Default::default(),
        code: None,
        stamp: Stamp::default(),
    });
    assert_eq!(
        j(&cmd),
        r#"{"kind":"line","category":"content","position":"002-0600","role":"os","speaker":"fixer","text":"Behind the door.","lineId":"marina.s01ep02.fixer_0010","voiceKey":"fixer-0010"}"#
    );
}

#[test]
fn injected_actor_carries_provenance() {
    let cmd = Command::Actor(ActorCmd {
        position: "002-0200".into(),
        character: "marina".into(),
        anchor: None,
        action: None,
        exit: None,
        emotion: Some("surprised".into()),
        costume: None,
        pos_reset: None,
        preload: Some(true),
        stamp: Stamp {
            provenance: Some(lute_check::Provenance {
                by: "entry-emotion-lookahead".into(),
                explanation: "pre-loading marina's first emotion".into(),
            }),
            ..Stamp::default()
        },
    });
    assert_eq!(
        j(&cmd),
        r#"{"kind":"actor","category":"staging","position":"002-0200","character":"marina","emotion":"surprised","preload":true,"provenance":{"by":"entry-emotion-lookahead","explanation":"pre-loading marina's first emotion"}}"#
    );
}

#[test]
fn choice_matches_spec_worked_example() {
    let cmd = Command::Choice(ChoiceCmd {
        position: "004-0500".into(),
        branch_id: "number".into(),
        selection_key: "scene.choices.number".into(),
        options: vec![ChoiceOption {
            id: "blunt".into(),
            text: "Just ask, flatly".into(),
            line_id: "marina.s01ep02.number.blunt".into(),
            when: None,
            target: "004-0600".into(),
            placeholders: Vec::new(),
            texts: Default::default(),
        }],
        converge: "004-1100".into(),
        prompt: None,
        timeout: None,
        stamp: Stamp::default(),
    });
    assert_eq!(
        j(&cmd),
        r#"{"kind":"choice","category":"control","position":"004-0500","branchId":"number","selectionKey":"scene.choices.number","options":[{"id":"blunt","text":"Just ask, flatly","lineId":"marina.s01ep02.number.blunt","target":"004-0600"}],"converge":"004-1100"}"#
    );
}

/// dsl 0.23.0 §4: an authored `<hub prompt>` serializes as `"prompt"` after
/// `converge`; an unprompted hub record carries no `prompt` key at all, and
/// a hub without a `<return>` block no `return` key (dsl 0.28.0 §5).
#[test]
fn hub_prompt_serializes_only_when_authored() {
    let hub = |prompt: Option<&str>, back: Option<&str>| {
        Command::Hub(HubCmd {
            position: "003-0200".into(),
            id: "look".into(),
            selection_key: "scene.choices.look".into(),
            options: vec![HubOption {
                id: "leave".into(),
                text: "Leave".into(),
                line_id: "s.look.leave".into(),
                once: false,
                exit: true,
                when: None,
                target: "003-0300".into(),
                placeholders: Vec::new(),
                texts: Default::default(),
            }],
            converge: "003-0400".into(),
            prompt: prompt.map(str::to_string),
            on_return: back.map(str::to_string),
            stamp: Stamp::default(),
        })
    };
    assert_eq!(
        j(&hub(Some("Where do you look?"), None)),
        r#"{"kind":"hub","category":"control","position":"003-0200","id":"look","selectionKey":"scene.choices.look","options":[{"id":"leave","text":"Leave","lineId":"s.look.leave","once":false,"exit":true,"target":"003-0300"}],"converge":"003-0400","prompt":"Where do you look?"}"#
    );
    assert!(!j(&hub(None, None)).contains("prompt"));
    assert!(
        j(&hub(None, Some("003-0350"))).ends_with(r#""converge":"003-0400","return":"003-0350"}"#)
    );
}

#[test]
fn match_jump_barrier_serialize() {
    let m = Command::Match(MatchCmd {
        position: "005-0700".into(),
        subject: Some(CelPair::from_raw("scene.choices.number")),
        arms: vec![MatchArm {
            is: None,
            test: CelPair::from_raw("(scene.affect.marina >= 1)"),
            target: "005-0800".into(),
        }],
        otherwise: Some("005-1200".into()),
        converge: "005-1400".into(),
        stamp: Stamp::default(),
    });
    assert_eq!(
        j(&m),
        r#"{"kind":"match","category":"control","position":"005-0700","subject":{"cel":"scene.choices.number","expr":{"path":"scene.choices.number"}},"arms":[{"test":{"cel":"(scene.affect.marina >= 1)","expr":{"op":">=","l":{"path":"scene.affect.marina"},"r":{"int":1}}},"target":"005-0800"}],"otherwise":"005-1200","converge":"005-1400"}"#
    );
    let jm = Command::Jump(JumpCmd {
        position: "004-0700".into(),
        target: "004-1100".into(),
    });
    assert_eq!(
        j(&jm),
        r#"{"kind":"jump","category":"control","position":"004-0700","target":"004-1100"}"#
    );
    let b = Command::Barrier(BarrierCmd {
        position: "003-0800".into(),
        timeline: 1,
        at: 1.4,
    });
    assert_eq!(
        j(&b),
        r#"{"kind":"barrier","category":"control","position":"003-0800","timeline":1,"at":1.4}"#
    );
}

#[test]
fn stamped_camera_and_set_and_plugin_passthrough() {
    let cam = Command::Camera(CameraCmd {
        position: "002-0300".into(),
        focus: Some("marina".into()),
        framing: Some("closeUp".into()),
        camera_move: None,
        transition: None,
        stamp: Stamp {
            timing: Timing {
                wait: Some(false),
                duration: Some(0.5),
                ..Timing::default()
            },
            ..Stamp::default()
        },
    });
    assert_eq!(
        j(&cam),
        r#"{"kind":"camera","category":"staging","position":"002-0300","focus":"marina","framing":"closeUp","timing":{"wait":false,"duration":0.5}}"#
    );
    let set = Command::Set(SetCmd {
        position: "004-0900".into(),
        path: "scene.affect.marina".into(),
        op: "+=".into(),
        value: CelPair::from_raw("1"),
        stamp: Stamp::default(),
    });
    assert_eq!(
        j(&set),
        r#"{"kind":"set","category":"state","position":"004-0900","path":"scene.affect.marina","op":"+=","value":{"cel":"1","expr":{"int":1}}}"#
    );
    let mut fields = BTreeMap::new();
    fields.insert(
        "kind".to_string(),
        serde_json::Value::String("rhythm".into()),
    );
    let other = Command::Plugin(PluginCmd {
        position: "001-0100".into(),
        tag: "minigame".into(),
        plugin: None,
        fields,
        effects: vec![],
        retracts: vec![],
        asserts: vec![],
        stamp: Stamp::default(),
    });
    assert_eq!(
        j(&other),
        r#"{"kind":"plugin","category":"plugin","position":"001-0100","tag":"minigame","fields":{"kind":"rhythm"}}"#
    );
}

#[test]
fn timeline_stamp_and_source_flatten() {
    let cmd = Command::Vfx(VfxCmd {
        position: "003-0500".into(),
        r#type: "whiteOut".into(),
        label: None,
        transition: Some("flash".into()),
        stamp: Stamp {
            timing: Timing {
                at: Some(0.5),
                timeline: Some(1),
                ..Timing::default()
            },
            source: Some(Source {
                component: "stinger".into(),
                scope: "stinger#1".into(),
                stable: false,
            }),
            ..Stamp::default()
        },
    });
    assert_eq!(
        j(&cmd),
        r#"{"kind":"vfx","category":"staging","position":"003-0500","type":"whiteOut","transition":"flash","timing":{"at":0.5,"timeline":1},"source":{"component":"stinger"}}"#
    );
}

#[test]
fn bg_serializes_per_spec() {
    // location + assetId set, time omitted (None), `wait` in `timing` —
    // pins camelCase `assetId`, None-omission, and `position`.
    let cmd = Command::Bg(BgCmd {
        position: "006-0100".into(),
        location: Some("cafe".into()),
        time: None,
        asset_id: Some("bg_cafe_evening".into()),
        stamp: Stamp {
            timing: Timing { wait: Some(true), ..Timing::default() },
            ..Stamp::default()
        },
    });
    assert_eq!(
        j(&cmd),
        r#"{"kind":"bg","category":"staging","position":"006-0100","location":"cafe","assetId":"bg_cafe_evening","timing":{"wait":true}}"#
    );
}

#[test]
fn music_serializes_per_spec() {
    // playback + mood + assetId set, volume omitted (None).
    let cmd = Command::Music(MusicCmd {
        position: "006-0200".into(),
        playback: Some("play".into()),
        mood: Some("tense".into()),
        volume: None,
        asset_id: Some("mus_theme_a".into()),
        stamp: Stamp::default(),
    });
    assert_eq!(
        j(&cmd),
        r#"{"kind":"music","category":"staging","position":"006-0200","playback":"play","mood":"tense","assetId":"mus_theme_a"}"#
    );
}

#[test]
fn sfx_serializes_per_spec() {
    // sound set, assetId omitted (None).
    let cmd = Command::Sfx(SfxCmd {
        position: "006-0300".into(),
        sound: Some("door_slam".into()),
        asset_id: None,
        stamp: Stamp::default(),
    });
    assert_eq!(
        j(&cmd),
        r#"{"kind":"sfx","category":"staging","position":"006-0300","sound":"door_slam"}"#
    );
}

#[test]
fn cg_serializes_per_spec() {
    // required `assetId` + resolved `display`; layout set, `wait` in `timing`.
    let cmd = Command::Cg(CgCmd {
        position: "006-0400".into(),
        asset_id: "cg_intro".into(),
        display: "show".into(),
        layout: Some("full".into()),
        stamp: Stamp {
            timing: Timing { wait: Some(false), ..Timing::default() },
            ..Stamp::default()
        },
    });
    assert_eq!(
        j(&cmd),
        r#"{"kind":"cg","category":"staging","position":"006-0400","assetId":"cg_intro","display":"show","layout":"full","timing":{"wait":false}}"#
    );
}

#[test]
fn video_serializes_per_spec() {
    // required `assetId` + resolved `display`, `wait` in `timing`.
    let cmd = Command::Video(VideoCmd {
        position: "006-0500".into(),
        asset_id: "vid_ending".into(),
        display: "show".into(),
        stamp: Stamp {
            timing: Timing { wait: Some(true), ..Timing::default() },
            ..Stamp::default()
        },
    });
    assert_eq!(
        j(&cmd),
        r#"{"kind":"video","category":"staging","position":"006-0500","assetId":"vid_ending","display":"show","timing":{"wait":true}}"#
    );
}

#[test]
fn retarget_and_position_helpers_visit_every_flow_field() {
    let mut cmd = Command::Choice(ChoiceCmd {
        position: String::new(),
        branch_id: "b".into(),
        selection_key: "scene.choices.b".into(),
        options: vec![ChoiceOption {
            id: "x".into(),
            text: "X".into(),
            line_id: String::new(),
            when: None,
            target: "@1".into(),
            placeholders: Vec::new(),
            texts: Default::default(),
        }],
        converge: "@2".into(),
        prompt: None,
        timeout: None,
        stamp: Stamp::default(),
    });
    *cmd.position_mut() = "001-0100".into();
    let mut seen = Vec::new();
    cmd.for_each_target(&mut |t: &mut String| {
        seen.push(t.clone());
        *t = "RESOLVED".into();
    });
    assert_eq!(seen, vec!["@1".to_string(), "@2".to_string()]);
    assert!(!j(&cmd).contains('@'));
    assert!(cmd.stamp_mut().is_some());
    let mut jm = Command::Jump(JumpCmd {
        position: String::new(),
        target: "@3".into(),
    });
    assert!(jm.stamp_mut().is_none());
    let mut n = 0;
    jm.for_each_target(&mut |_| n += 1);
    assert_eq!(n, 1);
}

#[test]
fn envelope_serializes_with_state_entries() {
    let a = ExecutionIr {
        kind: DocKind::Scene,
        lute: "0.3.0".into(),
        ir_version: "0.3.0".into(),
        capability_snapshot: "cap-sha".into(),
        identity_renames: vec![],
        required_semantics: vec![],
        meta: ArtifactMeta::Scene(SceneMeta {
            id: "marina.s01ep02".into(),
            character: Some("marina".into()),
            season: Some(1),
            episode: Some(2),
            episode_id: Some("s01ep02".into()),
            title: Some("T".into()),
            extra: BTreeMap::new(),
            plugin: BTreeMap::new(),
            beat: None,
        }),
        state: vec![StateEntry {
            path: "scene.choices.number".into(),
            ty: "enum".into(),
            domain: Some(vec!["blunt".into(), "soft".into(), "unset".into()]),
            default: None,
            provenance: Some("branch:number".into()),
            owner: None,
            labels: BTreeMap::new(),
            label_forms: BTreeMap::new(),
            member_domain: None,
        }],
        entities: Vec::new(),
        enums: Vec::new(),
        relations: Vec::new(),
        seed_facts: Vec::new(),
        rules: Vec::new(),
        commands: Vec::new(),
        prereq_edges: Vec::new(),
        sections: Vec::new(),
        clock: None,
        gates: Vec::new(),
        terminal: None,
        terminal_persists: false,
        seasons: Vec::new(),
        outside_run: Vec::new(),
        cel_env: Default::default(),
    };
    assert_eq!(
        serde_json::to_string(&a).unwrap(),
        r#"{"kind":"scene","lute":"0.3.0","irVersion":"0.3.0","capabilitySnapshot":"cap-sha","requiredSemantics":[],"meta":{"id":"marina.s01ep02","character":"marina","season":1,"episode":2,"episodeId":"s01ep02","title":"T"},"state":[{"path":"scene.choices.number","type":"enum","domain":["blunt","soft","unset"],"provenance":"branch:number"}],"commands":[],"outsideRun":[],"celEnv":{"variables":[],"functions":[]}}"#
    );
}

#[test]
fn quest_record_serializes_per_spec() {
    let cmd = Command::Quest(QuestCmd {
        position: "001-0100".into(),
        id: "rescueHalsin".into(),
        title: Some("Rescue".into()),
        title_line_id: Some("rescueHalsin.title".into()),
        start: Some(CelPair::from_raw("run.act == 1")),
        fail: None,
        objectives: vec![ObjectiveEntry {
            id: "reachGrove".into(),
            title: Some("Reach".into()),
            title_line_id: Some("rescueHalsin.reachGrove".into()),
            done: CelPair::from_raw("run.region == \"grove\""),
            visible_when: None,
            optional: false,
            body: None,
            quest: None,
            rewards: Vec::new(),
            on: None,
            by: None,
            target: None,
            until: None,
        }],
        rewards: Vec::new(),
        tier: None,
        activate: None,
        complete: None,
        accept: None,
        rearm: None,
        stamp: Stamp::default(),
    });
    assert_eq!(
        j(&cmd),
        r#"{"kind":"quest","category":"declaration","position":"001-0100","id":"rescueHalsin","title":"Rescue","titleLineId":"rescueHalsin.title","start":{"cel":"run.act == 1","expr":{"op":"==","l":{"path":"run.act"},"r":{"int":1}}},"objectives":[{"id":"reachGrove","title":"Reach","titleLineId":"rescueHalsin.reachGrove","done":{"cel":"run.region == \"grove\"","expr":{"op":"==","l":{"path":"run.region"},"r":{"string":"grove"}}},"optional":false,"body":null}]}"#
    );
}

#[test]
fn cel_pair_preserves_apostrophes_inside_double_quoted_strings() {
    let pair = CelPair::from_raw(r#"occasion.text == "it's here""#);
    assert_eq!(pair.raw, r#"occasion.text == "it's here""#);
    assert_eq!(
        serde_json::to_value(&pair).unwrap()["expr"],
        serde_json::json!({
            "op": "==",
            "l": {"path": "occasion.text"},
            "r": {"string": "it's here"}
        })
    );
}

#[test]
fn synthesized_double_cel_preserves_double_literals() {
    let expr = ExprNode::Binary {
        op: ">=".into(),
        l: Box::new(ExprNode::Path {
            path: "run.score".into(),
        }),
        r: Box::new(ExprNode::Lit {
            lit: LitVal::Num(1.0),
        }),
    };
    let pair = CelPair::from_expr(expr, Some("is=1..".into()));
    assert_eq!(pair.raw, "(run.score >= 1.0)");
    assert!(pair.raw.contains("1.0"));
    assert_eq!(
        serde_json::to_value(&pair.expr).unwrap(),
        serde_json::json!({
            "op": ">=",
            "l": {"path": "run.score"},
            "r": {"double": 1.0}
        })
    );
}

#[test]
fn synthesized_int_cel_preserves_integer_literals() {
    let expr = ExprNode::Binary {
        op: ">=".into(),
        l: Box::new(ExprNode::Path {
            path: "run.score".into(),
        }),
        r: Box::new(ExprNode::Lit {
            lit: LitVal::Int(1),
        }),
    };
    let pair = CelPair::from_expr(expr, Some("is=1..".into()));
    assert_eq!(pair.raw, "(run.score >= 1)");
    assert_eq!(
        serde_json::to_value(&pair.expr).unwrap(),
        serde_json::json!({
            "op": ">=",
            "l": {"path": "run.score"},
            "r": {"int": 1}
        })
    );
}

/// dsl 0.16.0 §2/§3 (Global Constraints): the load-bearing `RewardEntry`
/// wire shape. Field DECLARATION ORDER (byte-stability contract) is
/// `id?` (dsl 0.37.0 D10), `kind`, `target?`, `amount?`, `amountMin?`, `amountMax?`, `when?`, `on?`.
/// Every Option is `skip_serializing_if`, and exactly one of `amount` XOR
/// (`amountMin`+`amountMax`) is present after amount defaulting; `on` is
/// only ever `"failed"`, and only on a quest-level entry.
#[test]
fn reward_entry_scalar_serializes_per_spec() {
    let r = RewardEntry {
        id: Some("xp".into()),
        kind: "XP".into(),
        target: None,
        amount: Some(100),
        amount_min: None,
        amount_max: None,
        when: None,
        outcome: None,
        credits: None,
    };
    assert_eq!(
        serde_json::to_string(&r).unwrap(),
        r#"{"id":"xp","kind":"XP","amount":100}"#
    );
}

#[test]
fn reward_entry_range_serializes_amount_min_and_max() {
    let r = RewardEntry {
        id: None,
        kind: "GOLD".into(),
        target: Some("party".into()),
        amount: None,
        amount_min: Some(50),
        amount_max: Some(200),
        when: Some(CelPair::from_raw("run.freed")),
        outcome: None,
        credits: None,
    };
    assert_eq!(
        serde_json::to_string(&r).unwrap(),
        r#"{"kind":"GOLD","target":"party","amountMin":50,"amountMax":200,"when":{"cel":"run.freed","expr":{"path":"run.freed"}}}"#
    );
}

#[test]
fn reward_entry_on_failed_serializes_only_when_quest_level() {
    // `outcome="failed"` reaches the wire only on a quest-level entry (dsl
    // 0.16.0 §2). This golden pins the exact key + position — appearing
    // last, as the field declaration order dictates.
    let r = RewardEntry {
        id: None,
        kind: "TROPHY".into(),
        target: Some("halsin".into()),
        amount: Some(1),
        amount_min: None,
        amount_max: None,
        when: None,
        outcome: Some("failed".into()),
        credits: None,
    };
    assert_eq!(
        serde_json::to_string(&r).unwrap(),
        r#"{"kind":"TROPHY","target":"halsin","amount":1,"outcome":"failed"}"#
    );
}

/// A `RewardAmount::Range(lo, hi)` lifts into `amountMin`/`amountMax` with
/// `amount` skipped; an unauthored amount defaults to `Some(1)`; a stray
/// `on` value on an objective-level entry is DROPPED (never reaches the
/// wire — the checker rejects it upstream, this stays defensive).
#[test]
fn reward_entry_from_ast_defaults_amount_and_gates_on() {
    use lute_core_span::Span;
    use lute_syntax::ast::{Reward, RewardAmount};
    const ZERO: Span = Span {
        byte_start: 0,
        byte_end: 0,
        line: 1,
        column: 1,
        utf16_range: (0, 0),
    };
    let base = Reward {
        id: None,
        kind: "SHARD".into(),
        kind_span: ZERO,
        target: None,
        target_span: None,
        amount: None,
        amount_span: None,
        when: None,
        outcome: None,
        outcome_span: None,
        attrs: Vec::new(),
        span: ZERO,
        self_closing: true,
    };
    // Unauthored amount → default 1 on a quest-level entry.
    let e = RewardEntry::from_ast(&base, true);
    assert_eq!(e.amount, Some(1));
    assert!(e.amount_min.is_none() && e.amount_max.is_none());

    // Range lifts verbatim; `amount` stays None.
    let ranged = Reward {
        amount: Some(RewardAmount::Range(-3, 5)),
        ..base.clone()
    };
    let e = RewardEntry::from_ast(&ranged, true);
    assert!(e.amount.is_none());
    assert_eq!(e.amount_min, Some(-3));
    assert_eq!(e.amount_max, Some(5));

    // `outcome="failed"` on a quest-level entry survives; anything else is dropped.
    let quest_failed = Reward {
        outcome: Some("failed".into()),
        ..base.clone()
    };
    assert_eq!(
        RewardEntry::from_ast(&quest_failed, true)
            .outcome
            .as_deref(),
        Some("failed")
    );
    let quest_stray = Reward {
        outcome: Some("banana".into()),
        ..base.clone()
    };
    assert!(RewardEntry::from_ast(&quest_stray, true).outcome.is_none());

    // Objective-level entries never carry `on`, whatever the AST holds.
    let obj_failed = Reward {
        outcome: Some("failed".into()),
        ..base.clone()
    };
    assert!(RewardEntry::from_ast(&obj_failed, false).outcome.is_none());
}

#[test]
fn on_record_serializes_per_spec() {
    let cmd = Command::On(OnCmd {
        position: "001-0400".into(),
        event: "questComplete".into(),
        when: None,
        body: "001-0500".into(),
        target: None,
        stamp: Stamp::default(),
    });
    assert_eq!(
        j(&cmd),
        r#"{"kind":"on","category":"declaration","position":"001-0400","event":"questComplete","body":"001-0500"}"#
    );
}

/// dsl 0.24.0 §2 / 0.25.0 §5: the quest modes, an `<on target>` and a
/// queued accept serialize their non-default values, appended after the
/// 0.23 fields.
#[test]
fn quest_structure_fields_serialize_when_authored() {
    let quest = Command::Quest(QuestCmd {
        position: "001-0100".into(),
        id: "toll".into(),
        title: None,
        title_line_id: None,
        start: None,
        fail: None,
        objectives: Vec::new(),
        rewards: Vec::new(),
        tier: None,
        activate: Some(lute_compile::ir::QuestActivate::Accept),
        complete: Some(lute_compile::ir::QuestComplete::Any),
        accept: Some(lute_compile::ir::QuestAccept::External),
        rearm: None,
        stamp: Stamp::default(),
    });
    assert_eq!(
        j(&quest),
        r#"{"kind":"quest","category":"declaration","position":"001-0100","id":"toll","objectives":[],"activate":"accept","complete":"any","accept":"external"}"#
    );
    let on = Command::On(OnCmd {
        position: "001-0400".into(),
        event: "bossDefeated".into(),
        when: None,
        body: "001-0500".into(),
        target: Some("foe.regent".into()),
        stamp: Stamp::default(),
    });
    assert_eq!(
        j(&on),
        r#"{"kind":"on","category":"declaration","position":"001-0400","event":"bossDefeated","body":"001-0500","target":"foe.regent"}"#
    );
    let accept = Command::Accept(lute_compile::ir::AcceptCmd {
        position: "001-0100".into(),
        quest: "eelBounty".into(),
        applies: Some(lute_compile::ir::AcceptAt::NextRun),
        stamp: Stamp::default(),
    });
    assert_eq!(
        j(&accept),
        r#"{"kind":"accept","category":"declaration","position":"001-0100","quest":"eelBounty","applies":"nextRun"}"#
    );
}

/// dsl 0.37.0 D3: `::sequence{name}` lowers to a `sequence` staging record
/// carrying only its name and the resolved `timing`.
#[test]
fn sequence_serializes_per_spec() {
    let cmd = Command::Sequence(SequenceCmd {
        position: "002-0700".into(),
        name: "harborArrival".into(),
        stamp: Stamp {
            timing: Timing { wait: Some(true), ..Timing::default() },
            ..Stamp::default()
        },
    });
    assert_eq!(
        j(&cmd),
        r#"{"kind":"sequence","category":"staging","position":"002-0700","name":"harborArrival","timing":{"wait":true}}"#
    );
}

/// dsl 0.37.0 §5.1: the normative kind → category table (the `entry`/`beat`
/// declaration heads share `Command::category`'s exhaustive match), and
/// `kind`, `category`, `position` serialized first, in that order.
#[test]
fn every_kind_has_its_spec_category() {
    let stamp = Stamp::default;
    let p = String::new;
    let cel = || CelPair::from_raw("true");
    let cases: Vec<(Command, &str, &str)> = vec![
        (
            Command::Line(LineCmd {
                position: p(), role: Role::Mono, speaker: "wren".into(), text: "Hm.".into(),
                emotion: None, variant: None, action: None, dialog_motion: None, as_label: None,
                line_id: p(), voice_key: p(), placeholders: vec![], texts: BTreeMap::new(),
                code: None, stamp: stamp(),
            }),
            "line", "content",
        ),
        (Command::Bg(BgCmd { position: p(), location: None, time: None, asset_id: None, stamp: stamp() }), "bg", "staging"),
        (Command::Music(MusicCmd { position: p(), playback: None, mood: None, volume: None, asset_id: None, stamp: stamp() }), "music", "staging"),
        (Command::Sfx(SfxCmd { position: p(), sound: None, asset_id: None, stamp: stamp() }), "sfx", "staging"),
        (Command::Vfx(VfxCmd { position: p(), r#type: "flash".into(), label: None, transition: None, stamp: stamp() }), "vfx", "staging"),
        (
            Command::Actor(ActorCmd {
                position: p(), character: "wren".into(), anchor: None, action: None, exit: None,
                emotion: None, costume: None, pos_reset: None, preload: None, stamp: stamp(),
            }),
            "actor", "staging",
        ),
        (Command::Camera(CameraCmd { position: p(), focus: None, framing: None, camera_move: Some("shake".into()), transition: None, stamp: stamp() }), "camera", "staging"),
        (Command::Cg(CgCmd { position: p(), asset_id: "a".into(), display: "show".into(), layout: None, stamp: stamp() }), "cg", "staging"),
        (Command::Video(VideoCmd { position: p(), asset_id: "v".into(), display: "show".into(), stamp: stamp() }), "video", "staging"),
        (Command::Sequence(SequenceCmd { position: p(), name: "s".into(), stamp: stamp() }), "sequence", "staging"),
        (Command::Set(SetCmd { position: p(), path: "scene.x".into(), op: "=".into(), value: cel(), stamp: stamp() }), "set", "state"),
        (Command::Assert(AssertCmd { position: p(), relation: "r".into(), args: vec![], stamp: stamp() }), "assert", "state"),
        (Command::Retract(RetractCmd { position: p(), relation: "r".into(), args: vec![], stamp: stamp() }), "retract", "state"),
        (
            Command::Choice(ChoiceCmd {
                position: p(), branch_id: "b".into(), selection_key: "scene.choices.b".into(),
                options: vec![], converge: p(), prompt: None, timeout: Some(5), stamp: stamp(),
            }),
            "choice", "control",
        ),
        (Command::Match(MatchCmd { position: p(), subject: None, arms: vec![], otherwise: None, converge: p(), stamp: stamp() }), "match", "control"),
        (
            Command::Hub(HubCmd {
                position: p(), id: "h".into(), selection_key: "scene.choices.h".into(), options: vec![],
                converge: p(), prompt: None, on_return: None, stamp: stamp(),
            }),
            "hub", "control",
        ),
        (Command::Jump(JumpCmd { position: p(), target: p() }), "jump", "control"),
        (Command::End(EndCmd { position: p(), reason: None, stamp: stamp() }), "end", "control"),
        (Command::Barrier(BarrierCmd { position: p(), timeline: 0, at: 1.0 }), "barrier", "control"),
        (
            Command::Quest(QuestCmd {
                position: p(), id: "q".into(), title: None, title_line_id: None, start: None, fail: None,
                objectives: vec![], rewards: vec![], tier: None, activate: None, complete: None,
                accept: None, rearm: None, stamp: stamp(),
            }),
            "quest", "declaration",
        ),
        (Command::On(OnCmd { position: p(), event: "e".into(), when: None, body: p(), target: None, stamp: stamp() }), "on", "declaration"),
        (Command::Accept(lute_compile::ir::AcceptCmd { position: p(), quest: "q".into(), applies: None, stamp: stamp() }), "accept", "declaration"),
        (
            Command::Plugin(PluginCmd {
                position: p(), tag: "minigame".into(), plugin: None, fields: BTreeMap::new(),
                effects: vec![], retracts: vec![], asserts: vec![], stamp: stamp(),
            }),
            "plugin", "plugin",
        ),
    ];
    for (cmd, kind, category) in cases {
        assert_eq!(cmd.kind(), kind);
        assert_eq!(cmd.category().as_str(), category);
        let json = j(&cmd);
        let head = format!(r#"{{"kind":"{kind}","category":"{category}","position":"#);
        assert!(json.starts_with(&head), "{kind}: {json}");
    }
}
