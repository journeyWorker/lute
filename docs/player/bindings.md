# Player bindings

`player.bindings.yaml` is an optional file beside `lute.project.yaml`. It is read
by `lute check-project` after compilation and is not part of the project
fingerprint or runtime execution. Paths are relative to `assetsBase`, which
defaults to `assets/` beside this file.

```yaml
bindingsVersion: "0.46"
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
  ended:
    by: run.ending
    screens:
      restore: { title: "The Bell Rings Again", text: "Brinewake keeps its light.", background: dawnHarbor }
      retire: { title: "Quiet Water", text: "You hand the bell rope to Ivo." }
  theme: { text: "#ffffff", accent: "#e0c080" }
flow:
  newGame: { occasion: start }
  idle: { menu: [{ label: Foyer, occasion: visit, target: place.foyer }] }
```

## Sections

- `bindingsVersion` is `"0.46"`; `canvas` contains positive `width` and
  `height`. `assetsBase` is optional.
- `assets` maps compiled `assetId` values to files.
- `backgrounds` maps `bg.location` values to an `image`; `times` maps
  `bg.time` values to alternate files.
- `characters` is keyed by cast id. Each value has a `sprite.default`, optional
  `sprite.emotions`, optional `costumes` (each with the same sprite shape), and
  optional `nameColor`.
- `anchors`, `actions`, `transitions`, `framing`, `cameraMove`, `cgLayouts`,
  `textStyles`, and `sequences` are keyed by their declared staging domains.
  Anchors have `x` plus optional `y` and `scale`; framing has `zoom` plus
  optional `x` and `y`; CG layouts have `fit: cover|contain` plus optional
  `scale`, `x`, and `y`; text styles accept `fontWeight`, `fontStyle`, `color`,
  `alpha`, `scale`, and `letterSpacing`.
- `music.moods`, `sfx`, and `vfx` are keyed by values used by compiled
  documents. `sequences` values contain `video`; `voice.pattern` is a
  `{voiceKey}` path template.
- `volumes` (0.42) maps each member of the project's `volume` domain to a gain,
  a finite number in `[0, 1]`. The player resolves a music cue's `volume`
  member only through this table: no `volume` is gain `1`, a member without a
  gain is gain `0`, and a number is never parsed. `check-project` reports an
  unknown key (`E-BINDING-KEY`), a member without a key
  (`W-BINDING-MISSING`) and a gain outside `[0, 1]` or not finite
  (`E-BINDING-SHAPE`, at the value).
- `ui` contains `font`, a title `background` and `music`, optional `ended`, and arbitrary theme colour names. Every UI asset reference first checks `assets:`; a key counts as a use of that asset, while any other value is a file below `assetsBase` and must exist.
- `ui.ended` requires `by`, a declared state path with a closed enum/entity domain, and `screens`. Each screen requires `title` and `text` and may set `background`; unknown screen keys are `E-BINDING-KEY`, while unmapped members silently use the stock screen.
- When the ended screen is shown, player-web reads `program.view(state).state[by]`. A mapped member supplies title, text, and optional background; an absent or unmapped value, failed query, or absent `ui.ended` uses the stock screen.

## Effects

Effect objects have an `effect` key and only the parameters listed here:

| Effects | Parameters |
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

A `vfx` entry MAY be `{ effect: custom, name?, duration? }`: a
presenter-defined effect outside the catalogue. `name` defaults to the binding
key; the player passes the entry through as a `Vfx` cue whose effect is
`custom` with that `name`. `actions`, `transitions`, and `cameraMove` keep the
closed catalogue: `effect: custom` there is `E-BINDING-EFFECT`.

Unknown YAML keys are rejected. Unknown effects or parameters produce
`E-BINDING-EFFECT`; malformed YAML/shape produces `E-BINDING-SHAPE` with the
YAML position. Referenced files missing under `assetsBase` produce
`E-BINDING-FILE`.

## Project checks and diagnostics

Domain-keyed binding keys must be members of the merged project domains;
cast-keyed `characters` keys must be declared cast ids; flow occasions and
occasion targets must be declared. These violations are `E-BINDING-KEY` with a
nearby-name suggestion when one exists. A declared domain member, on-stage cast
member, or compiled usage without a binding produces `W-BINDING-MISSING`.
A binding in an asset/background/mood/sfx/vfx usage section that no compiled
document uses produces `W-BINDING-UNUSED`. All diagnostics are anchored in
`player.bindings.yaml` and appear in `check-project --json`.
