---
title: Player bindings reference
description: Complete player.bindings.yaml sections, effect catalogue, parameters, and check-project diagnostics for Lute 0.41.
---

`player.bindings.yaml` is optional and sits beside `lute.project.yaml`. `lute check-project` reads it after compilation. It does not affect the project fingerprint or runtime execution. Relative paths are resolved under `assetsBase`, which defaults to `assets/` beside the bindings file.

```yaml
bindingsVersion: "0.41"
canvas: { width: 1920, height: 1080 }
assetsBase: assets/
assets: { theme: audio/theme.ogg }
backgrounds:
  foyer: { image: bg/foyer.png, times: { night: bg/foyer-night.png } }
characters:
  guide:
    sprite: { default: char/guide.png, emotions: { happy: char/guide-happy.png } }
    costumes: { coat: { default: char/guide-coat.png } }
    nameColor: "#e0c080"
anchors: { default: { x: 0.5 } }
actions: { enter: { effect: fadeIn, duration: 0.4 } }
transitions: { fade: { effect: fade, duration: 0.6 } }
cameraMove: { shake: { effect: shake, duration: 0.5, strength: 12 } }
framing: { closeUp: { zoom: 1.35 } }
cgLayouts: { full: { fit: cover }, inset: { fit: contain, scale: 0.6 } }
textStyles: { emphasis: { fontWeight: bold }, whisper: { alpha: 0.6 } }
vfx:
  flash: { effect: flash, color: "#fff", duration: 0.3 }
  lanterns: { effect: custom }
  tideRise: { effect: custom, name: tide-rise, duration: 2 }
music: { moods: { calm: audio/calm.ogg } }
volumes: { silent: 0, quiet: 0.25, normal: 0.5, loud: 0.75, full: 1 }
sfx: { door: audio/door.ogg }
sequences: { doorOpens: { video: video/door-opens.mp4 } }
voice: { pattern: "voice/{voiceKey}.ogg" }
ui:
  font: Noto Sans KR
  title: { background: bg/title.png, music: theme }
  theme: { text: "#ffffff", accent: "#e0c080" }
flow:
  newGame: { occasion: start }
  idle: { menu: [{ label: Foyer, occasion: visit, target: place.foyer }] }
```

## Top-level sections

- `bindingsVersion` is exactly `"0.41"`. `canvas` requires positive `width` and `height`; `assetsBase` is optional.
- `assets` maps compiled `assetId` values to files.
- `backgrounds` maps `bg.location` values to `image`; `times` maps `bg.time` values to alternate files.
- `characters` is keyed by cast id. Each value has `sprite.default`, optional `sprite.emotions`, optional `costumes` (each with the same sprite shape), and optional `nameColor`.
- `anchors`, `actions`, `transitions`, `framing`, `cameraMove`, `cgLayouts`, `textStyles`, and `sequences` are keyed by their declared staging domains. Anchors have `x` plus optional `y` and `scale`; framing has `zoom` plus optional `x` and `y`; CG layouts have `fit: cover|contain` plus optional `scale`, `x`, and `y`; text styles accept `fontWeight`, `fontStyle`, `color`, `alpha`, `scale`, and `letterSpacing`.
- `music.moods`, `sfx`, and `vfx` are keyed by values used by compiled documents. `sequences` values contain `video`; `voice.pattern` is a `{voiceKey}` path template.
- `volumes` (0.42) maps each member of the project's `volume` domain to a gain, a finite number from `0` to `1`. A music cue's `volume` is an absolute level: the player plays it at that member's gain and never parses a number. A cue with no `volume` plays at gain `1`; a member without a gain plays at `0`, never at full volume. An unknown key is `E-BINDING-KEY`, a member with no key is `W-BINDING-MISSING`, and a gain that is not a finite number in `[0, 1]` is `E-BINDING-SHAPE` at its value.
- `ui` contains `font`, a title `background` and `music`, and arbitrary theme colour names.
- `flow.newGame` raises one `occasion`; `flow.idle` is either a `menu` list of `{label, occasion, target?}` entries or `raise: {occasion, target?}`.

## Effects

Effect objects have an `effect` key and only the parameters listed here:

| Effect | Parameters |
| --- | --- |
| `fade`, `dissolve`, `wipeLeft`, `wipeRight`, `wipeUp`, `wipeDown`, `slideLeft`, `slideRight` | `duration?` |
| `cut`, `none` | none |
| `fadeIn`, `fadeOut`, `slideIn`, `slideOut`, `hop` | `duration?` |
| `shake` | `duration?`, `strength?` |
| `zoom` | `duration?`, `scale?` |
| `pan` | `duration?`, `x?`, `y?` |
| `flash` | `duration?`, `color?` |
| `blur` | `duration?`, `strength?` |
| `tint` | `duration?`, `color?` |
| `custom` (only under `vfx`) | `name?`, `duration?` |

A `vfx` entry MAY be `{ effect: custom, name?, duration? }`: a presenter-defined effect outside the catalogue. `name` defaults to the binding key; the player passes the entry through as a `Vfx` cue whose effect is `custom` with that `name`. `actions`, `transitions`, and `cameraMove` keep the closed catalogue: `effect: custom` there is `E-BINDING-EFFECT`.

Unknown YAML keys are rejected. Unknown effects or parameters produce `E-BINDING-EFFECT`. Malformed YAML or an invalid shape produces `E-BINDING-SHAPE` with the YAML position. A referenced file missing under `assetsBase` produces `E-BINDING-FILE`.

## Project checks and diagnostics

Domain-keyed binding keys must be members of the merged project domains. `characters` keys must be declared cast ids. Flow occasions and occasion targets must be declared. These violations produce `E-BINDING-KEY`, with a nearby-name suggestion when one exists.

A declared domain member, on-stage cast member, or compiled usage without a binding produces `W-BINDING-MISSING`. A binding in an asset/background/mood/sfx/vfx usage section that no compiled document uses produces `W-BINDING-UNUSED`. Diagnostics are anchored in `player.bindings.yaml` and appear in `lute check-project --json`.

Use `lute check-project <dir>` after `lute compile --all`; a clean project with no bindings remains valid because the file is optional.

The normative repository reference is [`docs/player/bindings.md`](https://github.com/journeyWorker/lute/blob/main/docs/player/bindings.md).
