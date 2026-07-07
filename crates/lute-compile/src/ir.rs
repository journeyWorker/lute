//! Typed JSON IR (spec §4): tagged records with camelCase fields; only
//! relevant fields present (D3). Field DECLARATION ORDER is the serialized
//! order — part of the byte-stability contract; never reorder.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::expr::ExprNode;

/// Envelope (§4.1): version + meta + folded state schema + flat command array.
#[derive(Clone, Debug, Serialize)]
pub struct Artifact {
    pub lute: String,
    pub meta: ArtifactMeta,
    pub state: Vec<StateEntry>,
    pub commands: Vec<Command>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactMeta {
    pub character: String,
    pub season: i64,
    pub episode: i64,
    pub episode_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

/// One folded state slot (§4.1): the engine's init/type table.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StateEntry {
    pub path: String,
    #[serde(rename = "type")]
    pub ty: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provenance: Option<String>,
}

/// Cross-cutting optional stamps (§4.3), flattened into every stamped record:
/// resolved blocking, timing, timeline clip placement, injection provenance,
/// component source.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stamp {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wait: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delay: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeline: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provenance: Option<lute_check::Provenance>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
}

/// `source { component }` on component-expanded records (§4.3, D8).
#[derive(Clone, Debug, Serialize)]
pub struct Source {
    pub component: String,
}

/// `:line` role (§4.4). Voiced roles carry a `voiceKey` (§4.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Dialogue,
    Narration,
    Monologue,
    Voiceover,
}

impl Role {
    pub fn voiced(self) -> bool {
        matches!(self, Role::Dialogue | Role::Voiceover)
    }
}

/// One record (§4.4). Internally tagged on `kind`; the `Other` variant is the
/// plugin-directive passthrough (plan spec-gap note 1) and serializes as
/// `kind: "plugin"`.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Command {
    Line(LineCmd),
    Background(BackgroundCmd),
    Music(MusicCmd),
    Sfx(SfxCmd),
    Vfx(VfxCmd),
    Sprite(SpriteCmd),
    Camera(CameraCmd),
    Cut(CutCmd),
    Video(VideoCmd),
    Set(SetCmd),
    Choice(ChoiceCmd),
    Match(MatchCmd),
    Hub(HubCmd),
    Jump(JumpCmd),
    Barrier(BarrierCmd),
    #[serde(rename = "plugin")]
    Other(OtherCmd),
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LineCmd {
    pub addr: String,
    pub role: Role,
    pub speaker: String,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emotion: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variant: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dialog_motion: Option<String>,
    #[serde(rename = "as", skip_serializing_if = "Option::is_none")]
    pub as_label: Option<String>,
    pub line_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voice_key: Option<String>,
    /// Authored (or back-filled) per-speaker `code` — feeds `lineId`/`voiceKey`
    /// in the addressing pass, NEVER serialized (3-id model, §4.2).
    #[serde(skip)]
    pub code: Option<String>,
    #[serde(flatten)]
    pub stamp: Stamp,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundCmd {
    pub addr: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asset_id: Option<String>,
    #[serde(flatten)]
    pub stamp: Stamp,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MusicCmd {
    pub addr: String,
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mood: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asset_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track: Option<String>,
    #[serde(flatten)]
    pub stamp: Stamp,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SfxCmd {
    pub addr: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sound: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asset_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(flatten)]
    pub stamp: Stamp,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VfxCmd {
    pub addr: String,
    pub vfx_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transition: Option<String>,
    #[serde(flatten)]
    pub stamp: Stamp,
}

/// Authored `::auto` OR an injected sprite command (§7.4) — injected records
/// are SEPARATE records with `provenance` in their stamp.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpriteCmd {
    pub addr: String,
    pub character: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anchor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pos_reset: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preload: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emotion: Option<String>,
    #[serde(flatten)]
    pub stamp: Stamp,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CameraCmd {
    pub addr: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub focus: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zoom: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub move_x: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub move_y: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shake: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reset: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub easing: Option<String>,
    #[serde(flatten)]
    pub stamp: Stamp,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CutCmd {
    pub addr: String,
    pub asset_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub full: Option<bool>,
    #[serde(flatten)]
    pub stamp: Stamp,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoCmd {
    pub addr: String,
    pub asset_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    #[serde(flatten)]
    pub stamp: Stamp,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetCmd {
    pub addr: String,
    pub path: String,
    pub op: String,
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expr: Option<ExprNode>,
    #[serde(flatten)]
    pub stamp: Stamp,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChoiceCmd {
    pub addr: String,
    pub branch_id: String,
    pub record_key: String,
    pub options: Vec<ChoiceOption>,
    pub converge: String,
    #[serde(flatten)]
    pub stamp: Stamp,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChoiceOption {
    pub id: String,
    pub label: String,
    pub line_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub when: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expr: Option<ExprNode>,
    pub target: String,
}

/// `<hub>` (§7.3.2, IR A2): structurally a `choice` plus revisit flags. The
/// hub record is the loop head; re-presentation is a RUNTIME property of the
/// `hub` kind, so no backward jump is emitted (D2/§3.2).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HubCmd {
    pub addr: String,
    pub id: String,
    pub record_key: String,
    pub options: Vec<HubOption>,
    pub converge: String,
    #[serde(flatten)]
    pub stamp: Stamp,
}

/// One `<hub>` option: a `<choice>` option plus always-present `once`/`exit`
/// revisit flags. `when`/`expr` appear only when the choice is guarded.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HubOption {
    pub id: String,
    pub label: String,
    pub line_id: String,
    pub once: bool,
    pub exit: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub when: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expr: Option<ExprNode>,
    pub target: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchCmd {
    pub addr: String,
    pub subject: String,
    pub arms: Vec<MatchArm>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub otherwise: Option<String>,
    pub converge: String,
    #[serde(flatten)]
    pub stamp: Stamp,
}

#[derive(Clone, Debug, Serialize)]
pub struct MatchArm {
    pub test: String,
    pub target: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expr: Option<ExprNode>,
}

#[derive(Clone, Debug, Serialize)]
pub struct JumpCmd {
    pub addr: String,
    pub target: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct BarrierCmd {
    pub addr: String,
    pub timeline: u32,
    pub at: f64,
}

/// Plugin-directive passthrough (plan spec-gap note 1): `kind: "plugin"`,
/// the authored tag, and its attrs typed via the manifest `AttrDecl`s.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OtherCmd {
    pub addr: String,
    pub tag: String,
    pub fields: BTreeMap<String, serde_json::Value>,
    #[serde(flatten)]
    pub stamp: Stamp,
}

impl Command {
    /// The record's `addr` slot (filled by the addressing pass, Task 11).
    pub fn addr_mut(&mut self) -> &mut String {
        match self {
            Command::Line(c) => &mut c.addr,
            Command::Background(c) => &mut c.addr,
            Command::Music(c) => &mut c.addr,
            Command::Sfx(c) => &mut c.addr,
            Command::Vfx(c) => &mut c.addr,
            Command::Sprite(c) => &mut c.addr,
            Command::Camera(c) => &mut c.addr,
            Command::Cut(c) => &mut c.addr,
            Command::Video(c) => &mut c.addr,
            Command::Set(c) => &mut c.addr,
            Command::Choice(c) => &mut c.addr,
            Command::Match(c) => &mut c.addr,
            Command::Hub(c) => &mut c.addr,
            Command::Jump(c) => &mut c.addr,
            Command::Barrier(c) => &mut c.addr,
            Command::Other(c) => &mut c.addr,
        }
    }

    /// Visit every control-flow target field (option/arm `target`s,
    /// `otherwise`, `converge`, jump `target`) — the addressing pass rewrites
    /// symbolic labels to concrete `addr`s through this single seam.
    pub fn for_each_target(&mut self, f: &mut impl FnMut(&mut String)) {
        match self {
            Command::Jump(j) => f(&mut j.target),
            Command::Choice(c) => {
                for o in &mut c.options {
                    f(&mut o.target);
                }
                f(&mut c.converge);
            }
            Command::Match(m) => {
                for a in &mut m.arms {
                    f(&mut a.target);
                }
                if let Some(o) = &mut m.otherwise {
                    f(o);
                }
                f(&mut m.converge);
            }
            Command::Hub(c) => {
                for o in &mut c.options {
                    f(&mut o.target);
                }
                f(&mut c.converge);
            }
            _ => {}
        }
    }

    /// The record's stamp, when it has one (`jump`/`barrier` do not).
    pub fn stamp_mut(&mut self) -> Option<&mut Stamp> {
        match self {
            Command::Line(c) => Some(&mut c.stamp),
            Command::Background(c) => Some(&mut c.stamp),
            Command::Music(c) => Some(&mut c.stamp),
            Command::Sfx(c) => Some(&mut c.stamp),
            Command::Vfx(c) => Some(&mut c.stamp),
            Command::Sprite(c) => Some(&mut c.stamp),
            Command::Camera(c) => Some(&mut c.stamp),
            Command::Cut(c) => Some(&mut c.stamp),
            Command::Video(c) => Some(&mut c.stamp),
            Command::Set(c) => Some(&mut c.stamp),
            Command::Choice(c) => Some(&mut c.stamp),
            Command::Match(c) => Some(&mut c.stamp),
            Command::Hub(c) => Some(&mut c.stamp),
            Command::Other(c) => Some(&mut c.stamp),
            Command::Jump(_) | Command::Barrier(_) => None,
        }
    }
}
