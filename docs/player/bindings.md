# Player bindings

`player.bindings.yaml` is an optional file beside `lute.project.yaml`. It is read
by `lute check-project` after compilation and is not part of the project
fingerprint or runtime execution. Paths are relative to `assetsBase`, which
defaults to `assets/` beside this file.

```yaml
bindingsVersion: "0.47"
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
  title: { text: Brinewake, background: bg/title.png, music: theme }
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

- `bindingsVersion` is the toolchain line, `"MAJOR.MINOR"` (see
  [`bindingsVersion`](#bindingsversion)); `canvas` contains positive `width`
  and `height`. `assetsBase` is optional.
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
- `ui` contains `font`, optional `title`, optional `ended`, and arbitrary theme colour names. Every UI asset reference first checks `assets:`; a key counts as a use of that asset, while any other value is a file below `assetsBase` and must exist.
- `ui.title` (0.47) has an optional `text` — the game's name, drawn as the
  title screen's heading — and an optional `background` and `music`, resolved
  like every UI asset reference. When written, it binds at least one of the
  three, and `text` is a non-empty string; `ui.title: {}` or an empty `text` is
  `E-BINDING-SHAPE`. Without `text` the title screen draws no heading; without
  `background` or `music` it draws no title backdrop or plays no title music.
- `ui.ended` requires `by`, a declared state path with a closed enum/entity domain, and `screens`. Each screen requires `title` and `text` and may set `background`; unknown screen keys are `E-BINDING-KEY`, while unmapped members silently use the stock screen.
- When the ended screen is shown, player-web reads the entry at `by` in the
  runtime view, `program.view(state).state[by]`. That entry is a runtime value
  `{ kind, value }`, not a bare string: player-web uses its `value` only when it
  is a `{ kind: "str" }` entry and `screens` maps that member, which then
  supplies title, text, and optional background. An absent entry, an entry of
  another kind, an unmapped member, a failed query, or an absent `ui.ended` uses
  the stock screen.

## `bindingsVersion`

`bindingsVersion` follows the `luteVersion` rule. Its value is
`"MAJOR.MINOR"`; the **toolchain line** is the `MAJOR.MINOR` of the toolchain
version (`"0.47"` for Lute 0.47.x).

- `lute check-project` reports `W-BINDINGS-VERSION-STALE` at the value when it
  differs from the toolchain line: older — update it to the toolchain line
  (`lute fix` does it); newer — upgrade the toolchain, do not downgrade the
  stamp; not `MAJOR.MINOR` (such as `"0.47.0"`) — restamp it.
- A section newer than the declared line is `E-BINDINGS-VERSION` at that
  section's key, naming the line it needs:
  `` `ui.ended` needs bindingsVersion "0.46" or newer; this file declares "0.45" ``.
  Sections are not checked when the value is not `MAJOR.MINOR`.
- `lute fix` raises an older `MAJOR.MINOR` value in `player.bindings.yaml` to
  the toolchain line, keeping the rest of the file byte for byte (quotes and
  comments included); a newer or malformed value (leading zeros such as
  `"0.046"` included) is left alone, and so is a file written as one flow
  mapping (`{ bindingsVersion: …, … }`); the warning names `lute fix` only when
  it raises the value.

| Section | Needs `bindingsVersion` |
| --- | --- |
| `vfx.<name>` with `effect: custom` | `"0.41"` |
| `volumes` | `"0.42"` |
| `ui.ended` | `"0.46"` |
| `ui.title.text` | `"0.47"` |
| every other section | `"0.40"` |

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
cast-keyed `characters` keys must be declared cast ids; flow occasions must be
declared. These violations are `E-BINDING-KEY` with a
nearby-name suggestion when one exists. A flow target (`flow.newGame.target`,
`flow.idle.raise.target`, `flow.idle.menu[].target`) is the full occasion
target `raiseOccasion` takes, written in the prefixed form `<prefix>.<member>`
of the occasion's target domain — `place.square`, never the bare `square`
(lute 0.47.1). `lute check-project` checks it with the same rule as beat
targets and `lute play` step targets: an `open:` entity kind admits any
`<prefix>.<id>`; a closed kind or a `members:` subset admits exactly its
members under the prefix. Anything else is `E-BINDING-KEY` (the message starts
``flow target `…`:`` and, when close, suggests ``did you mean `place.square`?``).
A target on an occasion declared without a target (no `target:` or
`target: false`) is `E-BINDING-KEY`, and so is a target on an occasion whose
entity kind the project does not declare. A declared domain member, on-stage cast
member, or compiled usage without a binding produces `W-BINDING-MISSING`.
A binding in an asset/background/mood/sfx/vfx usage section that no compiled
document uses produces `W-BINDING-UNUSED`. All diagnostics are anchored in
`player.bindings.yaml` and appear in `check-project --json`.
