//! The persistent presentation stage carried by a runtime world.

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Stage {
    pub bg: Option<StageBackground>,
    pub music: Option<StageMusic>,
    pub actors: Vec<StageActor>,
    pub cg: Option<StageCg>,
    pub video: Option<StageVideo>,
}

#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StageBackground {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset_id: Option<String>,
}

#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StageMusic {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub playback: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mood: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset_id: Option<String>,
}

#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StageActor {
    pub character: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emotion: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub costume: Option<String>,
}

#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StageCg {
    pub asset_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<String>,
}

#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StageVideo {
    pub asset_id: String,
}

impl Stage {
    /// Apply one emitted staging record in command order.
    pub fn apply(&mut self, kind: &str, record: &Json) {
        match kind {
            "bg" => {
                self.bg = Some(StageBackground {
                    location: string(record, "location"),
                    time: string(record, "time"),
                    asset_id: string(record, "assetId"),
                });
            }
            "music" => {
                if record.get("stop").and_then(Json::as_bool) == Some(true) {
                    self.music = None;
                } else {
                    self.music = Some(StageMusic {
                        playback: string(record, "playback"),
                        mood: string(record, "mood"),
                        volume: string(record, "volume"),
                        asset_id: string(record, "assetId"),
                    });
                }
            }
            "actor" => {
                let Some(character) = string(record, "character") else {
                    return;
                };
                if record.get("exit").and_then(Json::as_bool) == Some(true) {
                    self.actors.retain(|actor| actor.character != character);
                    return;
                }
                if let Some(actor) = self
                    .actors
                    .iter_mut()
                    .find(|actor| actor.character == character)
                {
                    if let Some(anchor) = string(record, "anchor") {
                        actor.anchor = Some(anchor);
                    }
                    if let Some(emotion) = string(record, "emotion") {
                        actor.emotion = Some(emotion);
                    }
                    if let Some(costume) = string(record, "costume") {
                        actor.costume = Some(costume);
                    }
                } else {
                    self.actors.push(StageActor {
                        character,
                        anchor: string(record, "anchor"),
                        emotion: string(record, "emotion"),
                        costume: string(record, "costume"),
                    });
                }
            }
            "cg" => {
                if record.get("display").and_then(Json::as_str) == Some("hide") {
                    self.cg = None;
                } else if let Some(asset_id) = string(record, "assetId") {
                    self.cg = Some(StageCg {
                        asset_id,
                        layout: string(record, "layout"),
                    });
                }
            }
            "video" => {
                if record.get("display").and_then(Json::as_str) == Some("hide") {
                    self.video = None;
                } else if let Some(asset_id) = string(record, "assetId") {
                    self.video = Some(StageVideo { asset_id });
                }
            }
            _ => {}
        }
    }
}

fn string(record: &Json, key: &str) -> Option<String> {
    record.get(key).and_then(Json::as_str).map(str::to_owned)
}
