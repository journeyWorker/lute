# Flow and text (0.51)

A small, self-contained project for the 0.51 "flow and text" features. One plugin, `acme.flow`, declares
a bridge directive, a directive and a reward kind that raise a declared occasion, and a directive with
localizable text; a handful of documents use them:

```sh
export PATH="$HOME/.cargo/bin:$PATH"
cargo build -p lute-cli
./target/debug/lute check --project docs/examples/flow-and-text   # exit 0
./target/debug/lute test  --project docs/examples/flow-and-text   # every test and play passes
./target/debug/lute lint  --project docs/examples/flow-and-text   # the pair lint, exit 0
./target/debug/lute play  --project docs/examples/flow-and-text \
  --script docs/examples/flow-and-text/plays/flow.play.yaml        # the loop, the raise, its answer
./target/debug/lute compile docs/examples/flow-and-text/scenes/hall.lute \
  --project docs/examples/flow-and-text \
  --locale-bundle docs/examples/flow-and-text/loc/ko.json          # localized.title.texts.ko in the IR
```

> **This project needs the 0.51 toolchain.** It uses `::jump{max}`, `effects.raises`, `raisedBy`,
> `type: text` with `code=` on a directive, and `target: pair`, none of which an older `lute` loads.
> It was written from the 0.51.0 spec before those features were implemented, and it is **verified
> after S8 lands** (the release's last implementation slice): until then nothing here has been run.
> The guide that explains every recipe is
> [Porting a content set](https://lute-lang.vercel.app/guides/porting-a-content-set/).

## Layout

| Path | Role |
|---|---|
| `lute.project.yaml` | `pluginsDir`, the `flow` profile that activates `acme.flow`, `contentLang`, and the voice-key template |
| `plugins/acme.flow/plugin.yaml` | the plugin manifest; exports `directives`, `bridge`, `occasions`, `rewardKinds`, `state` |
| `plugins/acme.flow/directives/flow.yaml` | `::tryLock` (a bridge call), `::playScene` (`effects.raises`), `::banner` (`type: text`) and `::group` (the pair markers) |
| `plugins/acme.flow/bridge/locksmith.yaml` | the `locksmith.tryLock` capability and its `won` result |
| `plugins/acme.flow/occasions/occasions.yaml` | `start`, and `cutscene` with `raisedBy: declared` and target `{ prefix: scene, entity: cutsceneId }` |
| `plugins/acme.flow/rewardkinds/rewards.yaml` | `CUTSCENE`, a reward kind that only raises |
| `plugins/acme.flow/state/shapes.yaml` | the `lockResult` shape `::tryLock` writes |
| `world.schema.yaml` | the `cutsceneId` entity kind and the engine-owned `run.opened` |
| `scenes/lock.lute` | the retry loop over the bridge result, a nested loop, and `::playScene` |
| `scenes/chapter2.lute` | answers `cutscene` for `scene.chapter2` |
| `scenes/hall.lute` | `::banner{code="0010" …}` with `{{userName}}`, and balanced `::group` markers |
| `quests/finale.lute` | grants `CUTSCENE` for `chapter2` |
| `loc/ko.json` | the Korean locale bundle (`schemaVersion` 2, as `lute loc import` writes it) |
| `lute.lint.yaml` | the `group-balanced` rule, `target: pair` |
| `tests/*.test.yaml` | the loop run to exhaustion and left early (with `bridges:` answers), the hall, the finale grant |
| `plays/flow.play.yaml` | the loop to exhaustion, then the host raising `cutscene` twice |

## Feature map

| Feature | Where |
|---|---|
| a bounded retry loop over a bridge result | `scenes/lock.lute` (`::label{name="attempt"}`, `::jump{to="attempt" max="2"}`); `tests/lock-exhausted.test.yaml` runs it to exhaustion with three answers |
| leaving the loop early | `tests/lock-first-try.test.yaml`: one answer, the jump is never reached |
| a nested loop whose inner counter resets | `round` (max 1) around `knock` (max 2) in `scenes/lock.lute`; `scene.knocks` ends at 6 |
| a directive that raises a declared occasion | `::playScene` in `directives/flow.yaml`; used at the end of `scenes/lock.lute` |
| a reward kind that raises | `CUTSCENE` in `rewardkinds/rewards.yaml`; granted by `quests/finale.lute` |
| `raisedBy: declared` and the answering scene | `cutscene` in `occasions/occasions.yaml`; `scenes/chapter2.lute` (`on: cutscene`, `target: scene.chapter2`) |
| `text` attributes, `code=` and the lineId | `::banner` in `directives/flow.yaml`; `hall~banner~0010~title` in `scenes/hall.lute` |
| a locale bundle with directive text | `loc/ko.json` |
| a `pair` lint with `inside` | `lute.lint.yaml`; the markers in `scenes/hall.lute` |

## What to look at

The scenario graph shows the raises as edges (the two raisers are the scene's `::playScene` and the quest's reward):

<!-- rerun-after-S8 -->
```console
scene(lock) -> scene(chapter2) [raises]
quest(finale) -> scene(chapter2) [raises]
```

`lute trace scenes/lock.lute --project .` explains each take of the loop as `jump → attempt (again 1/2)`
and the exhausted jump as `jump → attempt not taken (2/2 used)`. `lute play` appends
`  → raises cutscene scene.chapter2` to the `::playScene` record's line and to the `CUTSCENE` grant line, but it
does not raise the occasion itself: the play script raises `cutscene` as its next step, as a host does.

`lute compile scenes/hall.lute … --locale-bundle loc/ko.json` merges the Korean text into the banner's
`plugin` record: `fields.title` keeps the English source with `{{userName}}` as authored, and
`localized.title` carries `{ lineId: "hall~banner~0010~title", …, texts: { ko: … } }`. A `lute test` script cannot
choose a locale, so the translated text is checked on the compiled artifact, not in a transcript.
`loc/ko.json`'s `sourceHashes` are the SHA-256 of each English source string, so editing the banner's `title`
makes `W-L10N-STALE` fire until the translation is updated.
