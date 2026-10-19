// THIS FILE IS GENERATED — DO NOT EDIT.
// Generator: packages/runtime/scripts/contract-codegen.ts
// Source: schemas/lute-player-bindings-0.47.schema.json

import * as Schema from "effect/Schema";
import type * as SchemaAST from "effect/SchemaAST";

export const parseOptions = { onExcessProperty: "error" } as const satisfies SchemaAST.ParseOptions;

export const Anchor = Schema.Struct({
  "scale": Schema.optionalKey(Schema.NullOr(Schema.Number)),
  "x": Schema.Number,
  "y": Schema.optionalKey(Schema.NullOr(Schema.Number))
});
export type Anchor = typeof Anchor.Type;

export const Background = Schema.Struct({
  "image": Schema.String,
  "times": Schema.optionalKey(Schema.Record(Schema.String, Schema.String))
});
export type Background = typeof Background.Type;

export const Canvas = Schema.Struct({
  "height": Schema.Int
  .pipe(
    Schema.check(Schema.isGreaterThanOrEqualTo(0))
  ),
  "width": Schema.Int
  .pipe(
    Schema.check(Schema.isGreaterThanOrEqualTo(0))
  )
});
export type Canvas = typeof Canvas.Type;

export const CgLayout = Schema.Struct({
  "fit": Schema.suspend(() => Fit),
  "scale": Schema.optionalKey(Schema.NullOr(Schema.Number)),
  "x": Schema.optionalKey(Schema.NullOr(Schema.Number)),
  "y": Schema.optionalKey(Schema.NullOr(Schema.Number))
});
export type CgLayout = typeof CgLayout.Type;

export const Character = Schema.Struct({
  "costumes": Schema.optionalKey(Schema.Record(Schema.String, Schema.suspend(() => Sprite))),
  "nameColor": Schema.optionalKey(Schema.NullOr(Schema.String)),
  "sprite": Schema.suspend(() => Sprite)
});
export type Character = typeof Character.Type;

/**
 * The `custom` effect tag of `CustomVfx`.
 */
export const CustomEffect = Schema.Literals([
  "custom"
]);
export type CustomEffect = typeof CustomEffect.Type;

/**
 * `{ effect: custom, name?, duration? }`; `name` defaults to the binding key.
 */
export const CustomVfx = Schema.Struct({
  "duration": Schema.optionalKey(Schema.NullOr(Schema.Number)),
  "effect": Schema.suspend(() => CustomEffect),
  "name": Schema.optionalKey(Schema.NullOr(Schema.String))
});
export type CustomVfx = typeof CustomVfx.Type;

/**
 * A named player effect. The externally tagged `effect` field deliberately
 * makes each effect's parameter set closed in both serde and schemars.
 */
export const EffectSpec = Schema.Union([
  Schema.Struct({
    "duration": Schema.optionalKey(Schema.NullOr(Schema.Number)),
    "effect": Schema.Literal("fade")
  }),
  Schema.Struct({
    "duration": Schema.optionalKey(Schema.NullOr(Schema.Number)),
    "effect": Schema.Literal("dissolve")
  }),
  Schema.Struct({
    "duration": Schema.optionalKey(Schema.NullOr(Schema.Number)),
    "effect": Schema.Literal("wipeLeft")
  }),
  Schema.Struct({
    "duration": Schema.optionalKey(Schema.NullOr(Schema.Number)),
    "effect": Schema.Literal("wipeRight")
  }),
  Schema.Struct({
    "duration": Schema.optionalKey(Schema.NullOr(Schema.Number)),
    "effect": Schema.Literal("wipeUp")
  }),
  Schema.Struct({
    "duration": Schema.optionalKey(Schema.NullOr(Schema.Number)),
    "effect": Schema.Literal("wipeDown")
  }),
  Schema.Struct({
    "duration": Schema.optionalKey(Schema.NullOr(Schema.Number)),
    "effect": Schema.Literal("slideLeft")
  }),
  Schema.Struct({
    "duration": Schema.optionalKey(Schema.NullOr(Schema.Number)),
    "effect": Schema.Literal("slideRight")
  }),
  Schema.Struct({
    "effect": Schema.Literal("cut")
  }),
  Schema.Struct({
    "duration": Schema.optionalKey(Schema.NullOr(Schema.Number)),
    "effect": Schema.Literal("fadeIn")
  }),
  Schema.Struct({
    "duration": Schema.optionalKey(Schema.NullOr(Schema.Number)),
    "effect": Schema.Literal("fadeOut")
  }),
  Schema.Struct({
    "duration": Schema.optionalKey(Schema.NullOr(Schema.Number)),
    "effect": Schema.Literal("slideIn")
  }),
  Schema.Struct({
    "duration": Schema.optionalKey(Schema.NullOr(Schema.Number)),
    "effect": Schema.Literal("slideOut")
  }),
  Schema.Struct({
    "duration": Schema.optionalKey(Schema.NullOr(Schema.Number)),
    "effect": Schema.Literal("hop")
  }),
  Schema.Struct({
    "duration": Schema.optionalKey(Schema.NullOr(Schema.Number)),
    "effect": Schema.Literal("shake"),
    "strength": Schema.optionalKey(Schema.NullOr(Schema.Number))
  }),
  Schema.Struct({
    "effect": Schema.Literal("none")
  }),
  Schema.Struct({
    "duration": Schema.optionalKey(Schema.NullOr(Schema.Number)),
    "effect": Schema.Literal("zoom"),
    "scale": Schema.optionalKey(Schema.NullOr(Schema.Number))
  }),
  Schema.Struct({
    "duration": Schema.optionalKey(Schema.NullOr(Schema.Number)),
    "effect": Schema.Literal("pan"),
    "x": Schema.optionalKey(Schema.NullOr(Schema.Number)),
    "y": Schema.optionalKey(Schema.NullOr(Schema.Number))
  }),
  Schema.Struct({
    "color": Schema.optionalKey(Schema.NullOr(Schema.String)),
    "duration": Schema.optionalKey(Schema.NullOr(Schema.Number)),
    "effect": Schema.Literal("flash")
  }),
  Schema.Struct({
    "duration": Schema.optionalKey(Schema.NullOr(Schema.Number)),
    "effect": Schema.Literal("blur"),
    "strength": Schema.optionalKey(Schema.NullOr(Schema.Number))
  }),
  Schema.Struct({
    "color": Schema.optionalKey(Schema.NullOr(Schema.String)),
    "duration": Schema.optionalKey(Schema.NullOr(Schema.Number)),
    "effect": Schema.Literal("tint")
  })
]);
export type EffectSpec = typeof EffectSpec.Type;

export const Ended = Schema.Struct({
  "by": Schema.String,
  "screens": Schema.Record(Schema.String, Schema.suspend(() => EndedScreen))
});
export type Ended = typeof Ended.Type;

export const EndedScreen = Schema.Struct({
  "background": Schema.optionalKey(Schema.NullOr(Schema.String)),
  "text": Schema.String,
  "title": Schema.String
});
export type EndedScreen = typeof EndedScreen.Type;

export const Fit = Schema.Literals([
  "cover",
  "contain"
]);
export type Fit = typeof Fit.Type;

export const Flow = Schema.Struct({
  "idle": Schema.optionalKey(Schema.NullOr(Schema.suspend(() => Idle))),
  "newGame": Schema.optionalKey(Schema.NullOr(Schema.suspend(() => NewGame)))
});
export type Flow = typeof Flow.Type;

export const Framing = Schema.Struct({
  "x": Schema.optionalKey(Schema.NullOr(Schema.Number)),
  "y": Schema.optionalKey(Schema.NullOr(Schema.Number)),
  "zoom": Schema.Number
});
export type Framing = typeof Framing.Type;

export const Idle = Schema.Union([
  Schema.Struct({
    "menu": Schema.Array(Schema.suspend(() => MenuItem))
  }),
  Schema.Struct({
    "raise": Schema.suspend(() => Raise)
  })
]);
export type Idle = typeof Idle.Type;

export const MenuItem = Schema.Struct({
  "label": Schema.String,
  "occasion": Schema.String,
  "target": Schema.optionalKey(Schema.NullOr(Schema.String))
});
export type MenuItem = typeof MenuItem.Type;

export const Music = Schema.Struct({
  "moods": Schema.optionalKey(Schema.Record(Schema.String, Schema.String))
});
export type Music = typeof Music.Type;

export const NewGame = Schema.Struct({
  "occasion": Schema.String,
  "target": Schema.optionalKey(Schema.NullOr(Schema.String))
});
export type NewGame = typeof NewGame.Type;

export const Raise = Schema.Struct({
  "occasion": Schema.String,
  "target": Schema.optionalKey(Schema.NullOr(Schema.String))
});
export type Raise = typeof Raise.Type;

export const Sequence = Schema.Struct({
  "video": Schema.String
});
export type Sequence = typeof Sequence.Type;

export const Sprite = Schema.Struct({
  "default": Schema.String,
  "emotions": Schema.optionalKey(Schema.Record(Schema.String, Schema.String))
});
export type Sprite = typeof Sprite.Type;

export const TextStyle = Schema.Struct({
  "alpha": Schema.optionalKey(Schema.NullOr(Schema.Number)),
  "color": Schema.optionalKey(Schema.NullOr(Schema.String)),
  "fontStyle": Schema.optionalKey(Schema.NullOr(Schema.String)),
  "fontWeight": Schema.optionalKey(Schema.NullOr(Schema.String)),
  "letterSpacing": Schema.optionalKey(Schema.NullOr(Schema.Number)),
  "scale": Schema.optionalKey(Schema.NullOr(Schema.Number))
});
export type TextStyle = typeof TextStyle.Type;

/**
 * `ui.title` (0.47.0 §4.1): the title heading `text` (non-empty) and the
 * title backdrop / music references, each optional; a written `ui.title`
 * binds at least one (both checked by whole-project `check`, `E-BINDING-SHAPE`).
 */
export const Title = Schema.Struct({
  "background": Schema.optionalKey(Schema.NullOr(Schema.String)),
  "music": Schema.optionalKey(Schema.NullOr(Schema.String)),
  "text": Schema.optionalKey(Schema.NullOr(Schema.String))
});
export type Title = typeof Title.Type;

export const Ui = Schema.Struct({
  "ended": Schema.optionalKey(Schema.NullOr(Schema.suspend(() => Ended))),
  "font": Schema.optionalKey(Schema.NullOr(Schema.String)),
  "theme": Schema.optionalKey(Schema.Record(Schema.String, Schema.String)),
  "title": Schema.optionalKey(Schema.NullOr(Schema.suspend(() => Title)))
});
export type Ui = typeof Ui.Type;

/**
 * A `vfx` binding: a catalogue effect, or a presenter-defined custom effect
 * (spec 0.41 §7.7). `custom` is admitted only here; actions, transitions and
 * camera moves keep the closed `EffectSpec` catalogue.
 */
export const VfxSpec = Schema.Union([
  Schema.suspend(() => CustomVfx),
  Schema.suspend(() => EffectSpec)
]);
export type VfxSpec = typeof VfxSpec.Type;

export const Voice = Schema.Struct({
  "pattern": Schema.optionalKey(Schema.NullOr(Schema.String))
});
export type Voice = typeof Voice.Type;

export const Bindings = Schema.Struct({
  "actions": Schema.optionalKey(Schema.Record(Schema.String, Schema.suspend(() => EffectSpec))),
  "anchors": Schema.optionalKey(Schema.Record(Schema.String, Schema.suspend(() => Anchor))),
  "assets": Schema.optionalKey(Schema.Record(Schema.String, Schema.String)),
  "assetsBase": Schema.optionalKey(Schema.String),
  "backgrounds": Schema.optionalKey(Schema.Record(Schema.String, Schema.suspend(() => Background))),
  "bindingsVersion": Schema.String,
  "cameraMove": Schema.optionalKey(Schema.Record(Schema.String, Schema.suspend(() => EffectSpec))),
  "canvas": Schema.suspend(() => Canvas),
  "cgLayouts": Schema.optionalKey(Schema.Record(Schema.String, Schema.suspend(() => CgLayout))),
  "characters": Schema.optionalKey(Schema.Record(Schema.String, Schema.suspend(() => Character))),
  "flow": Schema.optionalKey(Schema.suspend(() => Flow)),
  "framing": Schema.optionalKey(Schema.Record(Schema.String, Schema.suspend(() => Framing))),
  "music": Schema.optionalKey(Schema.suspend(() => Music)),
  "sequences": Schema.optionalKey(Schema.Record(Schema.String, Schema.suspend(() => Sequence))),
  "sfx": Schema.optionalKey(Schema.Record(Schema.String, Schema.String)),
  "textStyles": Schema.optionalKey(Schema.Record(Schema.String, Schema.suspend(() => TextStyle))),
  "transitions": Schema.optionalKey(Schema.Record(Schema.String, Schema.suspend(() => EffectSpec))),
  "ui": Schema.optionalKey(Schema.suspend(() => Ui)),
  "vfx": Schema.optionalKey(Schema.Record(Schema.String, Schema.suspend(() => VfxSpec))),
  "voice": Schema.optionalKey(Schema.suspend(() => Voice)),
  "volumes": Schema.optionalKey(Schema.Record(Schema.String, Schema.Number))
});
export type Bindings = typeof Bindings.Type;

