//! Player bindings contract (`player.bindings.yaml`), spec 0.40 §5.
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Bindings {
    pub bindings_version: String,
    pub canvas: Canvas,
    #[serde(default = "default_assets_base")]
    pub assets_base: String,
    #[serde(default)]
    pub assets: BTreeMap<String, String>,
    #[serde(default)]
    pub backgrounds: BTreeMap<String, Background>,
    #[serde(default)]
    pub characters: BTreeMap<String, Character>,
    #[serde(default)]
    pub anchors: BTreeMap<String, Anchor>,
    #[serde(default)]
    pub actions: BTreeMap<String, EffectSpec>,
    #[serde(default)]
    pub transitions: BTreeMap<String, EffectSpec>,
    #[serde(default)]
    pub camera_move: BTreeMap<String, EffectSpec>,
    #[serde(default)]
    pub vfx: BTreeMap<String, EffectSpec>,
    #[serde(default)]
    pub framing: BTreeMap<String, Framing>,
    #[serde(default)]
    pub cg_layouts: BTreeMap<String, CgLayout>,
    #[serde(default)]
    pub text_styles: BTreeMap<String, TextStyle>,
    #[serde(default)]
    pub music: Music,
    #[serde(default)]
    pub sfx: BTreeMap<String, String>,
    #[serde(default)]
    pub sequences: BTreeMap<String, Sequence>,
    #[serde(default)]
    pub voice: Voice,
    #[serde(default)]
    pub ui: Ui,
    #[serde(default)]
    pub flow: Flow,
}

fn default_assets_base() -> String {
    "assets".to_string()
}

#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Canvas {
    pub width: u32,
    pub height: u32,
}

#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Background {
    pub image: String,
    #[serde(default)]
    pub times: BTreeMap<String, String>,
}

#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Character {
    pub sprite: Sprite,
    #[serde(default)]
    pub costumes: BTreeMap<String, Costume>,
    #[serde(default)]
    pub name_color: Option<String>,
}

#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Sprite {
    pub default: String,
    #[serde(default)]
    pub emotions: BTreeMap<String, String>,
}

pub type Costume = Sprite;

#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Anchor {
    pub x: f64,
    #[serde(default)]
    pub y: Option<f64>,
    #[serde(default)]
    pub scale: Option<f64>,
}

/// A named player effect. The externally tagged `effect` field deliberately
/// makes each effect's parameter set closed in both serde and schemars.
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "effect", rename_all = "camelCase", deny_unknown_fields)]
pub enum EffectSpec {
    Fade {
        #[serde(default)]
        duration: Option<f64>,
    },
    Dissolve {
        #[serde(default)]
        duration: Option<f64>,
    },
    WipeLeft {
        #[serde(default)]
        duration: Option<f64>,
    },
    WipeRight {
        #[serde(default)]
        duration: Option<f64>,
    },
    WipeUp {
        #[serde(default)]
        duration: Option<f64>,
    },
    WipeDown {
        #[serde(default)]
        duration: Option<f64>,
    },
    SlideLeft {
        #[serde(default)]
        duration: Option<f64>,
    },
    SlideRight {
        #[serde(default)]
        duration: Option<f64>,
    },
    Cut,
    FadeIn {
        #[serde(default)]
        duration: Option<f64>,
    },
    FadeOut {
        #[serde(default)]
        duration: Option<f64>,
    },
    SlideIn {
        #[serde(default)]
        duration: Option<f64>,
    },
    SlideOut {
        #[serde(default)]
        duration: Option<f64>,
    },
    Hop {
        #[serde(default)]
        duration: Option<f64>,
    },
    Shake {
        #[serde(default)]
        duration: Option<f64>,
        #[serde(default)]
        strength: Option<f64>,
    },
    None,
    Zoom {
        #[serde(default)]
        duration: Option<f64>,
        #[serde(default)]
        scale: Option<f64>,
    },
    Pan {
        #[serde(default)]
        duration: Option<f64>,
        #[serde(default)]
        x: Option<f64>,
        #[serde(default)]
        y: Option<f64>,
    },
    Flash {
        #[serde(default)]
        duration: Option<f64>,
        #[serde(default)]
        color: Option<String>,
    },
    Blur {
        #[serde(default)]
        duration: Option<f64>,
        #[serde(default)]
        strength: Option<f64>,
    },
    Tint {
        #[serde(default)]
        duration: Option<f64>,
        #[serde(default)]
        color: Option<String>,
    },
}

#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Framing {
    pub zoom: f64,
    #[serde(default)]
    pub x: Option<f64>,
    #[serde(default)]
    pub y: Option<f64>,
}

#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Fit {
    Cover,
    Contain,
}

#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CgLayout {
    pub fit: Fit,
    #[serde(default)]
    pub scale: Option<f64>,
    #[serde(default)]
    pub x: Option<f64>,
    #[serde(default)]
    pub y: Option<f64>,
}

#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextStyle {
    #[serde(default)]
    pub font_weight: Option<String>,
    #[serde(default)]
    pub font_style: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub alpha: Option<f64>,
    #[serde(default)]
    pub scale: Option<f64>,
    #[serde(default)]
    pub letter_spacing: Option<f64>,
}

#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Music {
    #[serde(default)]
    pub moods: BTreeMap<String, String>,
}

#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sequence {
    pub video: String,
}

#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Voice {
    #[serde(default)]
    pub pattern: Option<String>,
}

#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Ui {
    #[serde(default)]
    pub font: Option<String>,
    #[serde(default)]
    pub title: Option<Title>,
    #[serde(default)]
    pub theme: BTreeMap<String, String>,
}

#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Title {
    pub background: String,
    pub music: String,
}

#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Flow {
    #[serde(default)]
    pub new_game: Option<NewGame>,
    #[serde(default)]
    pub idle: Option<Idle>,
}

#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NewGame {
    pub occasion: String,
    #[serde(default)]
    pub target: Option<String>,
}

#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Idle {
    Menu { menu: Vec<MenuItem> },
    Raise { raise: Raise },
}

#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MenuItem {
    pub label: String,
    pub occasion: String,
    #[serde(default)]
    pub target: Option<String>,
}

#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Raise {
    pub occasion: String,
    #[serde(default)]
    pub target: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_the_spec_shape_and_rejects_unknown_effect_parameters() {
        let value: Bindings = serde_yaml::from_str(
            "bindingsVersion: '0.40'\ncanvas: {width: 1920, height: 1080}\nactions: {hop: {effect: hop, duration: 0.2}}\n",
        ).unwrap();
        assert!(matches!(
            value.actions["hop"],
            EffectSpec::Hop { duration: Some(_) }
        ));
        assert!(serde_yaml::from_str::<Bindings>(
            "bindingsVersion: '0.40'\ncanvas: {width: 1, height: 1}\nactions: {hop: {effect: hop, strength: 2}}\n",
        ).is_err());
    }

    #[test]
    fn strict_shape_errors_keep_yaml_position() {
        let error = serde_yaml::from_str::<Bindings>(
            "bindingsVersion: '0.40'\ncanvas: {width: 1, height: 1}\nunknown: true\n",
        )
        .unwrap_err();
        let location = error.location().expect("serde_yaml reports a location");
        assert_eq!((location.line(), location.column()), (3, 1));
    }
}
