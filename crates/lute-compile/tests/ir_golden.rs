//! Golden-per-kind serialization (spec §4.4): one exact-JSON assertion per
//! record kind pins the discriminator, camelCase field names, field order,
//! and None-field omission — the byte-stability contract everything else
//! (addresses, e2e goldens, determinism) rides on.

use std::collections::BTreeMap;

use lute_compile::*;

fn j(cmd: &Command) -> String {
    serde_json::to_string(cmd).unwrap()
}

#[test]
fn line_serializes_per_spec() {
    let cmd = Command::Line(LineCmd {
        addr: "002-0500".into(),
        role: Role::Dialogue,
        speaker: "bianca".into(),
        text: "Oh!".into(),
        emotion: Some("surprised".into()),
        variant: Some(0),
        action: None,
        dialog_motion: None,
        as_label: None,
        line_id: "bianca.s01ep02.bianca_0010".into(),
        voice_key: Some("bianca-0010".into()),
        placeholders: Vec::new(),
        code: Some("0010".into()),
        stamp: Stamp::default(),
    });
    // `code` is #[serde(skip)] — the 3-id model (§4.2) admits no code field.
    assert_eq!(
        j(&cmd),
        r#"{"kind":"line","addr":"002-0500","role":"dialogue","speaker":"bianca","text":"Oh!","emotion":"surprised","variant":0,"lineId":"bianca.s01ep02.bianca_0010","voiceKey":"bianca-0010"}"#
    );
}

#[test]
fn unvoiced_line_has_no_voice_key() {
    let cmd = Command::Line(LineCmd {
        addr: "002-0400".into(),
        role: Role::Narration,
        speaker: "narrator".into(),
        text: "A hostess walked over.".into(),
        emotion: None,
        variant: None,
        action: None,
        dialog_motion: None,
        as_label: None,
        line_id: "bianca.s01ep02.narrator_0010".into(),
        voice_key: None,
        placeholders: Vec::new(),
        code: None,
        stamp: Stamp::default(),
    });
    assert!(!j(&cmd).contains("voiceKey"));
    assert!(!Role::Narration.voiced());
    assert!(!Role::Monologue.voiced());
    assert!(Role::Dialogue.voiced());
    assert!(Role::Voiceover.voiced());
}

#[test]
fn injected_sprite_carries_provenance() {
    let cmd = Command::Sprite(SpriteCmd {
        addr: "002-0200".into(),
        character: "bianca".into(),
        anchor: None,
        action: None,
        exit: None,
        pos_reset: None,
        preload: Some(true),
        emotion: Some("surprised".into()),
        costume: None,
        stamp: Stamp {
            provenance: Some(lute_check::Provenance {
                injected: true,
                by: "entry-emotion-lookahead".into(),
                reason: "pre-loading bianca's first emotion".into(),
            }),
            ..Stamp::default()
        },
    });
    assert_eq!(
        j(&cmd),
        r#"{"kind":"sprite","addr":"002-0200","character":"bianca","preload":true,"emotion":"surprised","provenance":{"injected":true,"by":"entry-emotion-lookahead","reason":"pre-loading bianca's first emotion"}}"#
    );
}

#[test]
fn choice_matches_spec_worked_example() {
    let cmd = Command::Choice(ChoiceCmd {
        addr: "004-0500".into(),
        branch_id: "number".into(),
        record_key: "scene.choices.number".into(),
        options: vec![ChoiceOption {
            id: "blunt".into(),
            label: "Just ask, flatly".into(),
            line_id: "bianca.s01ep02.number.blunt".into(),
            when: None,
            expr: None,
            target: "004-0600".into(),
            placeholders: Vec::new(),
        }],
        converge: "004-1100".into(),
        stamp: Stamp::default(),
    });
    assert_eq!(
        j(&cmd),
        r#"{"kind":"choice","addr":"004-0500","branchId":"number","recordKey":"scene.choices.number","options":[{"id":"blunt","label":"Just ask, flatly","lineId":"bianca.s01ep02.number.blunt","target":"004-0600"}],"converge":"004-1100"}"#
    );
}

#[test]
fn match_jump_barrier_serialize() {
    let m = Command::Match(MatchCmd {
        addr: "005-0700".into(),
        subject: "scene.choices.number".into(),
        arms: vec![MatchArm {
            test: "(scene.affect.bianca >= 1)".into(),
            target: "005-0800".into(),
            expr: expr::lower_expr("(scene.affect.bianca >= 1)"),
        }],
        otherwise: Some("005-1200".into()),
        converge: "005-1400".into(),
        stamp: Stamp::default(),
    });
    assert_eq!(
        j(&m),
        r#"{"kind":"match","addr":"005-0700","subject":"scene.choices.number","arms":[{"test":"(scene.affect.bianca >= 1)","target":"005-0800","expr":{"op":">=","l":{"path":"scene.affect.bianca"},"r":{"lit":1.0}}}],"otherwise":"005-1200","converge":"005-1400"}"#
    );
    let jm = Command::Jump(JumpCmd {
        addr: "004-0700".into(),
        target: "004-1100".into(),
    });
    assert_eq!(
        j(&jm),
        r#"{"kind":"jump","addr":"004-0700","target":"004-1100"}"#
    );
    let b = Command::Barrier(BarrierCmd {
        addr: "003-0800".into(),
        timeline: 1,
        at: 1.4,
    });
    assert_eq!(
        j(&b),
        r#"{"kind":"barrier","addr":"003-0800","timeline":1,"at":1.4}"#
    );
}

#[test]
fn stamped_camera_and_set_and_plugin_passthrough() {
    let cam = Command::Camera(CameraCmd {
        addr: "002-0300".into(),
        focus: Some("bianca".into()),
        zoom: Some(1.1),
        move_x: None,
        move_y: None,
        shake: None,
        reset: None,
        easing: None,
        stamp: Stamp {
            wait: Some(false),
            duration: Some(0.5),
            ..Stamp::default()
        },
    });
    assert_eq!(
        j(&cam),
        r#"{"kind":"camera","addr":"002-0300","focus":"bianca","zoom":1.1,"wait":false,"duration":0.5}"#
    );
    let set = Command::Set(SetCmd {
        addr: "004-0900".into(),
        path: "scene.affect.bianca".into(),
        op: "+=".into(),
        value: "1".into(),
        expr: expr::lower_expr("1"),
        stamp: Stamp::default(),
    });
    assert_eq!(
        j(&set),
        r#"{"kind":"set","addr":"004-0900","path":"scene.affect.bianca","op":"+=","value":"1","expr":{"lit":1.0}}"#
    );
    let mut fields = BTreeMap::new();
    fields.insert(
        "kind".to_string(),
        serde_json::Value::String("rhythm".into()),
    );
    let other = Command::Other(OtherCmd {
        addr: "001-0100".into(),
        tag: "minigame".into(),
        fields,
        effects: vec![],
        stamp: Stamp::default(),
    });
    assert_eq!(
        j(&other),
        r#"{"kind":"plugin","addr":"001-0100","tag":"minigame","fields":{"kind":"rhythm"}}"#
    );
}

#[test]
fn timeline_stamp_and_source_flatten() {
    let cmd = Command::Vfx(VfxCmd {
        addr: "003-0500".into(),
        vfx_type: "whiteOut".into(),
        label: None,
        transition: Some("flash".into()),
        stamp: Stamp {
            at: Some(0.5),
            timeline: Some(1),
            source: Some(Source {
                component: "stinger".into(),
            }),
            ..Stamp::default()
        },
    });
    assert_eq!(
        j(&cmd),
        r#"{"kind":"vfx","addr":"003-0500","vfxType":"whiteOut","transition":"flash","at":0.5,"timeline":1,"source":{"component":"stinger"}}"#
    );
}

#[test]
fn background_serializes_per_spec() {
    // location + assetId set, time omitted (None), `wait` via the flattened
    // stamp — pins camelCase `assetId`, None-omission, and `addr`.
    let cmd = Command::Background(BackgroundCmd {
        addr: "006-0100".into(),
        location: Some("cafe".into()),
        time: None,
        asset_id: Some("bg_cafe_evening".into()),
        stamp: Stamp {
            wait: Some(true),
            ..Stamp::default()
        },
    });
    assert_eq!(
        j(&cmd),
        r#"{"kind":"background","addr":"006-0100","location":"cafe","assetId":"bg_cafe_evening","wait":true}"#
    );
}

#[test]
fn music_serializes_per_spec() {
    // required `action`; mood + assetId set, volume + track omitted (None).
    let cmd = Command::Music(MusicCmd {
        addr: "006-0200".into(),
        action: "play".into(),
        mood: Some("tense".into()),
        volume: None,
        asset_id: Some("mus_theme_a".into()),
        track: None,
        stamp: Stamp::default(),
    });
    assert_eq!(
        j(&cmd),
        r#"{"kind":"music","addr":"006-0200","action":"play","mood":"tense","assetId":"mus_theme_a"}"#
    );
}

#[test]
fn sfx_serializes_per_spec() {
    // sound + name set, assetId omitted (None).
    let cmd = Command::Sfx(SfxCmd {
        addr: "006-0300".into(),
        sound: Some("door_slam".into()),
        asset_id: None,
        name: Some("slam".into()),
        stamp: Stamp::default(),
    });
    assert_eq!(
        j(&cmd),
        r#"{"kind":"sfx","addr":"006-0300","sound":"door_slam","name":"slam"}"#
    );
}

#[test]
fn cut_serializes_per_spec() {
    // required `assetId`; action + full set, `wait` via the flattened stamp.
    let cmd = Command::Cut(CutCmd {
        addr: "006-0400".into(),
        asset_id: "cut_intro".into(),
        action: Some("show".into()),
        full: Some(true),
        stamp: Stamp {
            wait: Some(false),
            ..Stamp::default()
        },
    });
    assert_eq!(
        j(&cmd),
        r#"{"kind":"cut","addr":"006-0400","assetId":"cut_intro","action":"show","full":true,"wait":false}"#
    );
}

#[test]
fn video_serializes_per_spec() {
    // required `assetId`; action omitted (None), `wait` via the flattened stamp.
    let cmd = Command::Video(VideoCmd {
        addr: "006-0500".into(),
        asset_id: "vid_ending".into(),
        action: None,
        stamp: Stamp {
            wait: Some(true),
            ..Stamp::default()
        },
    });
    assert_eq!(
        j(&cmd),
        r#"{"kind":"video","addr":"006-0500","assetId":"vid_ending","wait":true}"#
    );
}

#[test]
fn retarget_and_addr_helpers_visit_every_flow_field() {
    let mut cmd = Command::Choice(ChoiceCmd {
        addr: String::new(),
        branch_id: "b".into(),
        record_key: "scene.choices.b".into(),
        options: vec![ChoiceOption {
            id: "x".into(),
            label: "X".into(),
            line_id: String::new(),
            when: None,
            expr: None,
            target: "@1".into(),
            placeholders: Vec::new(),
        }],
        converge: "@2".into(),
        stamp: Stamp::default(),
    });
    *cmd.addr_mut() = "001-0100".into();
    let mut seen = Vec::new();
    cmd.for_each_target(&mut |t: &mut String| {
        seen.push(t.clone());
        *t = "RESOLVED".into();
    });
    assert_eq!(seen, vec!["@1".to_string(), "@2".to_string()]);
    assert!(!j(&cmd).contains('@'));
    assert!(cmd.stamp_mut().is_some());
    let mut jm = Command::Jump(JumpCmd {
        addr: String::new(),
        target: "@3".into(),
    });
    assert!(jm.stamp_mut().is_none());
    let mut n = 0;
    jm.for_each_target(&mut |_| n += 1);
    assert_eq!(n, 1);
}

#[test]
fn envelope_serializes_with_state_entries() {
    let a = Artifact {
        lute: "0.1.0".into(),
        ir_version: "0.1.0".into(),
        capability_version: "cap-sha".into(),
        meta: ArtifactMeta {
            character: "bianca".into(),
            season: 1,
            episode: 2,
            episode_id: "s01ep02".into(),
            title: Some("T".into()),
        },
        state: vec![StateEntry {
            path: "scene.choices.number".into(),
            ty: "enum".into(),
            domain: Some(vec!["blunt".into(), "soft".into(), "unset".into()]),
            default: None,
            provenance: Some("branch:number".into()),
        }],
        commands: Vec::new(),
    };
    assert_eq!(
        serde_json::to_string(&a).unwrap(),
        r#"{"lute":"0.1.0","irVersion":"0.1.0","capabilityVersion":"cap-sha","meta":{"character":"bianca","season":1,"episode":2,"episodeId":"s01ep02","title":"T"},"state":[{"path":"scene.choices.number","type":"enum","domain":["blunt","soft","unset"],"provenance":"branch:number"}],"commands":[]}"#
    );
}
