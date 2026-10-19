# `@lute-lang/player-web` — architecture

Normative behaviour: `docs/proposals/scenario-dsl/0.40.0.md` §7, as amended by
`docs/proposals/scenario-dsl/0.41.0.md` §8. It implements the core's ports
(`@lute-lang/player` `ports/`) with PixiJS 8 (WebGL) and `@pixi/sound`, and
provides the screens, input and `mount`.

```text
mount ─► WebPlayer layer ─┬─ PixiApp (scoped Application, layers, letterbox, ticker)
                          ├─ World (entities + behaviours, systems run by the ticker)
                          ├─ PixiPresenter (implements Presenter; cue → entity ops)
                          ├─ PixiAudio (implements AudioPort; channels, fades)
                          ├─ PixiAssets (implements AssetLoader; manifest from bindings, refcounts)
                          ├─ Ui (screens: title, game HUD, backlog, save/load, settings, confirm, loading, halted)
                          └─ Input (devices → PlayerAction stream)
```

## Game patterns

- **Entity–behaviour (component) model.** A `World` holds entities (`Background`,
  `Actor`, `Cg`, `Video`, `DialogueWindow`, `ChoiceMenu`, `Camera`, `Overlay`),
  each a Pixi `Container` plus a list of behaviours. A **behaviour** is a small
  object `{ update(dt): "running" | "done"; finish(): void }` (tween, fade,
  crossfade, shake, hop, slide, wipe, typewriter, countdown, pulse). The
  ticker system steps all behaviours each frame; finished ones are removed and
  returned to a pool. Each behaviour has a `Deferred` the presenter awaits, so a
  cue's `Effect` completes exactly when its behaviours finish; `finish()` snaps
  to the end state (skip, `complete`).
- **Command pattern** (from the core): cues reach the presenter as data; the
  presenter maps each to entity operations (`EntityOps`), which attach
  behaviours.
- **State pattern for screens**: the core machine decides which screen is
  shown; `Ui` renders `ScreenModel`s and turns widget interactions into
  `PlayerAction`s — it holds no game state.
- **Object pools** for behaviours, tweens and text glyph runs: no allocation
  per frame in steady state.
- **Input mapping table** (device event → action), configurable.

## Module contracts (`src/`)

```ts
// app/
interface PixiApp {
  readonly app: Application                      // initialized with preference "webgl"
  readonly layers: Layers                        // bottom→top: background, camera(actors), cg, video, dialogue, choice, overlay, transition
  readonly design: { width: number; height: number }   // bindings.canvas
  readonly frames: Stream<number>                // dt seconds per tick
}
const PixiApp: Context.Service<PixiApp>          // Layer.scoped: init on acquire, destroy on release

// ecs/
interface Behaviour { update(dt: number): "running" | "done"; finish(): void; readonly done: Deferred<void> }
interface World {
  readonly attach: (target: Container, behaviour: Behaviour) => Effect<void>     // completes when done
  readonly finishAll: (target?: Container) => Effect<void>
  readonly step: (dt: number) => void            // called by the ticker system; pure w.r.t. Effect
}
// behaviours: Tween<T>(props, to, duration, easing), Fade, Crossfade(textures), Shake, Hop, Slide, Wipe(mask, dir),
//             Typewriter(text run), Countdown(bar), Pulse; easing functions (linear, quad, cubic in/out, back out)

// effects/
type EffectName = "fade" | "dissolve" | "wipeLeft" | "wipeRight" | "wipeUp" | "wipeDown" | "slideLeft" | "slideRight"
  | "cut" | "fadeIn" | "fadeOut" | "slideIn" | "slideOut" | "hop" | "shake" | "none" | "zoom" | "pan" | "flash" | "blur" | "tint"
// `custom` (a project vfx, 0.41.0 §7.7) is not a catalogue effect: the presenter handles it first.
interface EffectCatalogue {
  /** Behaviours for an effect on a target, with params from bindings and the cue's duration. */
  readonly build: (name: EffectName, target: Container, params: EffectParams, duration: number) => ReadonlyArray<Behaviour>
  /** Hide/show halves of a hiding effect (fade, dissolve, fadeOut, wipe*, slideLeft/Right, slideOut); undefined otherwise. */
  readonly hidingTransition: (name: EffectName, target: Container, params: EffectParams) => { hide(seconds): Behaviour; show(seconds): Behaviour } | undefined
}

// presenter/
interface EntityOps {                            // the vocabulary cues map to
  setBackground(bg: StageBg, transition: EffectSpec): Effect<void>
  upsertActor(actor: StageActor, action: EffectSpec | undefined): Effect<void>
  removeActor(character: string, action: EffectSpec | undefined): Effect<void>
  showCg(cg: StageCg | null, transition: EffectSpec): Effect<void>
  showVideo(video: StageVideo | null, wait: boolean): Effect<void>
  camera(op: CameraOp): Effect<void>             // framing/focus with a move or a transition; see "Camera transitions"
  vfx(op: VfxOp): Effect<void>
  restore(stage: Stage): Effect<void>            // rebuild everything at once; the camera rests (alpha 1, no mask, default framing)
}
class PixiPresenter implements Presenter          // uses EntityOps, DialogueWindow, ChoiceMenu, Ui; capture() via renderer.extract
// A StageCue.Vfx with effect.effect === "custom" plays nothing; the presenter logs effect.name once per name
// through PresenterOptions.debug (default console.debug).

// text/
interface DialogueWindow {
  show(line: LineView, settings: TextSettings, mode: PlayMode): Effect<void>   // typewriter over segments (styles, rate, pause), name plate
                                                                               // shows line.speakerName (index cast name); colour keyed by line.speaker
  reveal: Effect<void>; clear: Effect<void>; setOpacity(alpha: number): void; setHidden(hidden: boolean): void
}
interface ChoiceMenu { show(menu: MenuAwait, remaining: Stream<number>): Effect<string>; hide: Effect<void> }

// audio/
class PixiAudio implements AudioPort             // channels music (crossfade), sfx, voice; fades as Effects; unlock on first gesture
                                                 // music: restart plays from the start; without it the file already
                                                 // playing only changes volume (fading over `fade`); a music play or
                                                 // stop supersedes every earlier music play still waiting (for unlock,
                                                 // its file or the backend), so while locked only the latest survives

// assets/
class PixiAssets implements AssetLoader          // keys = binding paths; Pixi Assets manifest/bundles; refcounted ensure/release; progress stream

// ui/
interface Ui {                                    // renders ScreenModel; emits PlayerAction
  readonly show: (screen: ScreenModel) => Effect<void>
  readonly hide: Effect<void>
  readonly actions: Stream<PlayerAction>
}
// screens: Title (heading = bound ui.title.text, none when unbound; backdrop/music only when ui.title.background/music are bound),
//          Backlog (scroll, voice replay), SaveLoad (slot grid, thumbnails, confirm overwrite/delete),
//          Settings (sliders/toggles, locale), Confirm, Loading (progress), Halted (message, back to title), Hud (quick menu buttons)
// widgets: Button, Slider, Toggle, ScrollList, Panel — keyboard focus ring + pointer

// input/
interface InputMap { readonly keyboard: Record<string, PlayerAction>; readonly pointer: …; readonly gamepad: … }
const Input: Layer<Stream<PlayerAction>>          // keyboard/pointer/touch (tap, long press, swipe)/gamepad (polled on ticker)

// mount.ts
LuteWeb.mount(element, options): Effect<WebPlayerHandle, MountError, Scope>   // WebPlayerHandle = LutePlayerHandle (send, restore, session, program)
mountPlayer(element, options): Promise<{ act, view, unmount }>              // the only Promise API; options.host is the core's PlayerHost
```

## Camera transitions

Spec 0.47.1 §4. The `camera` layer (the actors) rests at alpha 1 with no
mask. A `::camera` cue with a `move` supersedes a running transition, rests
the layer, applies the framing, then plays the move (a move takes precedence
over a transition). A cue with a `transition`:

- **Hiding effect** (`fade`, `dissolve`, `fadeOut`, `wipeLeft`/`Right`/`Up`/`Down`,
  `slideLeft`, `slideRight`, `slideOut`): the effect hides the layer over the
  first half of the duration (the layer stays hidden once that half ended),
  the framing and focus are applied, and the layer is shown again over the
  second half: alpha back to 1, a wipe reveal in the same direction, or a
  slide back into place (`slideLeft`/`slideOut` leave to the left and come
  back from the right, `slideRight` the other way). A wipe sweeps the design
  stage (its mask lives in the camera layer's parent space), not the actors'
  span.
- **Any other effect**: the framing is applied, then the effect plays.

Whatever the effect, the cue ends with alpha 1, no mask and the new
framing's scale and position. `cut`, `none`, a zero duration and an instant
cue (skip) apply the framing at once. The transition runs on a fiber of the
camera's own, forked in the player's scope (`PresenterOptions.scope`; unmount
interrupts it), and the cue joins it: a cue the scheduler does not wait for is
interrupted when its batch ends (e.g. right before a choice), and a started
transition still plays out. (With `autosave` `false` or `"presentation"`, a
non-waiting camera cue that is the last of a batch before a choice is
currently not started at all: a player-core scheduler behaviour, tracked
separately.) Its behaviours run under their own world key. A later camera
cue (a transition or a move) supersedes it: its behaviours are cancelled and
it does no more. Skip supersedes it only when skip starts while the cue still
runs (the scheduler then stages the cue instantly); a transition whose cue
already ended finishes at normal speed until the next camera cue. `restore`
(load, rollback) supersedes it too, cancels every running camera behaviour,
sets alpha 1 and no mask, then applies the default framing, so a load during
a transition never leaves the actors hidden.

## Testing

- Unit (no WebGL): easing, tween math, each behaviour's update/finish, pools,
  effect catalogue param decoding and behaviour lists, input mapping, asset
  manifest building/refcounts, text segmentation (graphemes, styles, pauses).
- Browser (Vitest browser mode, Playwright Chromium, WebGL via SwiftShader
  if needed): PixiApp init/destroy; presenter cue → scene graph assertions;
  restore(stage) equals the stage reached by cues; dialogue typewriter and
  reveal; choice menu click and countdown; screens render and emit actions;
  audio channel state (with a silent fake AudioContext if needed); screenshot
  of a fixed frame with tolerance.

## Package checks

`@lute-lang/player-web` uses `bun run typecheck` as its package typecheck; it
runs `bun check`. `bun run build` remains the declaration-emitting
`scripts/build.ts`/`tsc` path. CI pins Bun 1.4.3.
