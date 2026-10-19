---
title: 커스텀 플레이어 호스트
description: Presenter, AudioPort, AssetLoader, PlayerHost로 렌더러 독립 Lute 플레이어 코어의 native 또는 테스트 호스트를 만듭니다.
---

PixiJS가 맞지 않을 때는 `@lute-lang/player`를 직접 사용합니다. 코어에는 네 가지 경계가 있습니다. `Presenter`는 cue와 화면을 렌더링하고, `AudioPort`는 바인딩된 사운드를 재생하며, `AssetLoader`는 바인딩된 파일을 로드하고, `PlayerHost`는 호스트별 bridge, plugin 레코드, idle 정책을 처리합니다.

## 코어 조합

```ts
import { Effect, Layer } from "effect"
import { LutePlayer } from "@lute-lang/player"
import * as RuntimeBun from "@lute-lang/runtime/bun"

const player = LutePlayer.make({
  bindings: bindingsYaml,
  presenter: myPresenter,
  audio: myAudio,
  assets: myAssets,
  host: {
    bridge: { "save-score": (request) => sendToNativeHost(request).pipe(Effect.map((fields) => ({ fields }))) },
    plugin: { well: (call) => game.drawFromWell(call.attrs) },
    idle: (await_) => Effect.succeed(undefined)
  }
})

const requirements = Layer.mergeAll(
  RuntimeBun.runtime,
  RuntimeBun.bundle("dist/bundle"),
  RuntimeBun.inputLog("my-game"),
  myKeyValueStore
)
```

`LutePlayerOptions`에는 `bindings`, `presenter`, `audio`, `assets`가 필요하며 `host`, `cueFactory`, `linePacing`, `autosave`와 시작점 하나를 선택적으로 받습니다. 시작점은 `seed`(기본 `{ derive: true }`), `snapshot`(런타임 스냅숏), `session`(호스트가 만든 런타임 세션 — 플레이어가 구동만 하고 닫지 않음) 중 하나입니다. `cueFactory`는 기본 `CueFactoryShape`를 정확히 한 번 감싸며, 출력 이벤트마다 cue 하나라는 순서를 유지하려면 `fromOutput`을 그대로 위임해야 합니다. 바인딩은 디코딩된 값이나 YAML 문자열일 수 있습니다. `LutePlayer.make`를 scope 안에서 실행하기 전에 runtime, bundle, input-log, key-value-store 서비스를 제공합니다. `session`을 넘기면 같은 bundle과 key-value store만 필요합니다.

## 호스팅 레시피 (0.44)

호스트가 플레이어를 조립하는 세 가지 방법입니다. 각 예제는 공개된 타입으로 검사했습니다(`bundle`, `startMinigame`, `storeCheckpoint` 같은 이름은 호스트의 값입니다). 공통 import:

```ts
import { Effect, Layer } from "effect"
import { BrowserKeyValueStore } from "@effect/platform-browser"
import { Cues, Director, LutePlayer, Scheduler, Session, Stores, type SaveRuntime } from "@lute-lang/player"
import { BundleSource, InputLog, Session as RuntimeSession } from "@lute-lang/runtime"
import * as RuntimeBrowser from "@lute-lang/runtime/browser"
```

### `Director.make` 직접 조립하기

`LutePlayer.make`는 director를 대신 만들어 줍니다. 직접 조립하는 호스트는 컴파일된 `bundle`(0.42부터 필수: director가 이것으로 기본 cue factory를 만듭니다), 플레이어 세션, 저장소, 스케줄러를 넘기고, 필요하면 `host`와 `cueFactory` 래퍼를 넘깁니다.

```ts
const director = Effect.gen(function* () {
  return yield* Director.make({
    bundle,                                    // 컴파일된 번들 값
    bindings, presenter, audio, assets,
    session: yield* Session.PlayerSession,     // 런타임 세션 위의 Session.layer({ bindings, … })
    profile: yield* Stores.ProfileStore,       // Stores.profileLayer({ project })
    saves: yield* Stores.SaveStore,            // Stores.layer({ project })
    scheduler: yield* Scheduler.make(),
    host: { plugin: { minigame: (call) => startMinigame(call.attrs) } },
    cueFactory: (base) => ({
      ...base,
      fromOutput: (output, readLines, locale) =>
        base.fromOutput(output, readLines, locale).pipe(Effect.tap((cues) => Effect.log(`${cues.length} cues`)))
    })
  })
})
```

- `host.plugin`은 기본 cue factory에 전달되므로 다시 연결할 필요가 없습니다.
- `cueFactory` **함수**는 그 기본 factory를 정확히 한 번 감쌉니다(출력 이벤트마다 cue 하나, 순서 유지).
- 미리 만든 factory **값**은 `Cues.make({ bundle, bindings, plugin })`으로 만든 것이어야 `host.plugin`이 합쳐집니다(같은 tag면 호스트 핸들러가 우선). 다른 값을 `host.plugin`과 함께 넘기면 핸들러를 버리지 않고 `Director.make`가 die합니다.

### 직접 만든 런타임 세션으로 `LutePlayer.make` 쓰기

게임이 런타임 세션을 소유할 때(질의하고, 계기를 발생시키고, input log를 관리할 때)는 세션을 먼저 만들어 플레이어에 넘깁니다. 플레이어에는 **같은** `BundleSource`(cue를 만들 때 아티팩트를 읽음)와 저장·프로필용 `KeyValueStore`가 필요합니다.

```ts
const bundleSource = BundleSource.value(bundleValue)   // 또는 BundleSource.directory(root) / BundleSource.url(base)
const runtimeSession = RuntimeSession.layer({ seed: { derive: true } }).pipe(   // 또는 { snapshot }
  Layer.provide(Layer.mergeAll(RuntimeBrowser.runtime(wasmUrl), bundleSource, InputLog.memory))
)

const game = Effect.scoped(Effect.gen(function* () {
  const session = yield* RuntimeSession.Session
  const player = yield* LutePlayer.make({ session, bindings, presenter, audio, assets, host })
  yield* player.actions({ type: "newGame" })
  // …
}))

Effect.runFork(game.pipe(Effect.provide(Layer.mergeAll(runtimeSession, bundleSource, Stores.memory))))
```

- `Stores.memory`는 저장과 프로필을 메모리에 둡니다(테스트, 미리보기). 새로고침 뒤에도 남는 저장에는 영구 `KeyValueStore`를 쓰세요. 브라우저에서는 `BrowserKeyValueStore.layerLocalStorage`, Bun/Node/네이티브 호스트에서는 직접 구현한 store입니다. 플레이어는 `save:quick`, `save:auto`, `save:1`…과 `profile:<project>`에 씁니다. 게임과 store를 함께 쓴다면 접두사를 붙인 store(`KeyValueStore.prefix`)를 주세요.
- `InputLog.memory`는 런타임 input log를 메모리에 두고, `InputLog.keyValueStore("game-log")`는 같은 store에 영구 저장합니다.
- 요구 사항: `make({ session })`에는 `Scope`, `BundleSource`, `KeyValueStore`가 필요합니다. `make({ seed })` / `make({ snapshot })`은 런타임 세션을 직접 만들므로 `LuteRuntime`과 `InputLog`도 필요합니다.

### 런타임 스냅숏과 플레이어 저장

| | 런타임 `Contract.Snapshot` | 플레이어 `SaveRuntime` |
|---|---|---|
| 담는 것 | 런타임 상태: `await`, `continuation?`, `project`, `request`, `snapshotVersion`, `world`(state, facts, stage, clock) | `{ snapshot, log, backlog }`: 그 런타임 스냅숏과 플레이어의 accepted-input log, backlog |
| 만드는 곳 | 런타임 `Session.save` | 플레이어 핸들의 `snapshot`(`DirectorHandle.snapshot`) |
| 쓰는 곳 | 런타임 `Session.restore`, `LutePlayer.make({ snapshot })`, `RuntimeSession.layer({ snapshot })` | 핸들의 `restore(save)`, 저장 슬롯 |
| 복원하는 것 | 이야기 위치 | 이야기와 플레이어 log·backlog. 체크포인트 뒤에 붙은 읽음 표시는 지우고, rollback 기록은 비우고, `auto` 슬롯을 다시 쓰고, stage를 다시 만들고, 그 음악은 재생이 시작되기를 기다리지 않고 시작하고, await를 다시 제시합니다 |

실행 중인 플레이어는 핸들로 체크포인트를 만들고 복원합니다.

```ts
const save: SaveRuntime = yield* player.snapshot   // send/restore와 선형화됨
yield* storeCheckpoint(save)
// … 나중에, 같은 플레이어에서:
yield* player.restore(save)
```

**새** 플레이어(새 페이지, 새 프로세스)에서 이어 하려면, 저장의 런타임 스냅숏에서 런타임을 시작한 뒤 저장을 복원해 log와 backlog를 되살립니다. `LutePlayer.make`는 플레이어가 `boot`를 벗어난 뒤 돌아오므로 바로 `restore`할 수 있습니다.

```ts
const resume = Effect.scoped(Effect.gen(function* () {
  const save = yield* loadCheckpoint
  const player = yield* LutePlayer.make({ snapshot: save.snapshot, bindings, presenter, audio, assets })
  yield* player.restore(save)
}))
```


### 새 player-web 마운트에서 저장 재개

player-web Promise facade는 scoped handle과 같은 저장 Effect를 제공합니다.
`SaveRuntime`을 저장한 다음 runtime snapshot으로 새 마운트를 시작하고 전체
save를 복원합니다.

```ts
import { mountPlayer } from "@lute-lang/player-web"
import type { SaveRuntime } from "@lute-lang/player"

const mountOptions = {
  bindings: { url: "/player.bindings.yaml" },
  bundleUrl: "/bundle/",
  wasmUrl: "/wasm/lute_runtime_wasm_bg.wasm"
} as const
const player = await mountPlayer(document.querySelector("#player")!, mountOptions)
const save: SaveRuntime = await player.snapshot()
await player.unmount()

const fresh = await mountPlayer(document.querySelector("#player")!, {
  ...mountOptions,
  snapshot: save.snapshot
})
await fresh.restore(save)
await fresh.send({ type: "raiseOccasion", occasion: "ambient", target: "npc.guide" })
const bundle = fresh.bundle
await fresh.unmount()
```

`mountPlayer`는 `program`도 직접 제공하며 `send`, `snapshot`, `restore`가
실패하면 내부 Effect와 같은 실패를 reject합니다. `bundle`은 해당 플레이어가
로드한 번들이므로 `index.documents`와 `artifacts`를 두 번 로드하지 않고
확인할 수 있습니다.

## 대화 중 배경 bark (0.45)

호스트는 이야기가 선택지나 브리지에서 기다리는 동안, 두 번째 프레젠테이션 없이 주변 대사를 보여 줄 수 있습니다(런타임은 열린 프레젠테이션을 하나만 유지하며, 동시 프레젠테이션은 계획하지 않습니다). 읽기 전용 질의 — `player.program`의 `candidates`, `evaluate`, `def`, `gate`, `view` — 는 모든 await에서 응답하고 상태를 바꾸지 않습니다. 그래서 대화가 멈춰 있는 동안 bark를 고르고, 그 대사를 번들에서 읽을 수 있습니다.

```ts
import { Effect, Schema, SubscriptionRef } from "effect"
import type { LutePlayerHandle } from "@lute-lang/player"
import type { Bundle, Contract } from "@lute-lang/runtime"

interface AmbientLine { readonly text: string; readonly speaker?: string; readonly voiceKey: string; readonly lineId: string }
declare const showAmbient: (lines: ReadonlyArray<AmbientLine>) => Effect.Effect<void>   // the host's own UI

// The record fields the pattern reads; other fields are ignored.
const Artifact = Schema.Struct({
  commands: Schema.Array(Schema.Struct({
    kind: Schema.String,
    position: Schema.String,
    id: Schema.optionalKey(Schema.String),
    body: Schema.optionalKey(Schema.String),
    speaker: Schema.optionalKey(Schema.String),
    text: Schema.optionalKey(Schema.String),
    texts: Schema.optionalKey(Schema.Record(Schema.String, Schema.String)),
    voiceKey: Schema.optionalKey(Schema.String),
    lineId: Schema.optionalKey(Schema.String),
    placeholders: Schema.optionalKey(Schema.Unknown)
  }))
})

/** A static entry's lines, or undefined when the bark must be raised instead. */
const staticBarkLines = (bundle: Bundle.Bundle, candidate: Contract.Candidate, locale?: string): ReadonlyArray<AmbientLine> | undefined => {
  if (candidate.kind !== "entry" || candidate.verdict.verdict !== "eligible") return undefined
  const document = bundle.index.documents.find((item) => item.path === candidate.document)
  if (document === undefined) return undefined
  const { commands } = Schema.decodeUnknownSync(Artifact)(bundle.artifacts[document.artifact])
  const head = commands.find((record) => record.kind === "entry" && record.id === candidate.id)
  const start = commands.findIndex((record) => record.position === head?.body)
  if (start < 0) return undefined
  const next = commands.findIndex((record, index) => index > start && (record.kind === "entry" || record.kind === "beat"))
  const body = commands.slice(start, next < 0 ? commands.length : next)
  const lines: Array<AmbientLine> = []
  for (const record of body) {
    if (record.kind !== "line" || record.placeholders !== undefined) return undefined
    if (record.text === undefined || record.voiceKey === undefined || record.lineId === undefined) return undefined
    const speaker = record.speaker === undefined ? undefined : bundle.index.cast?.[record.speaker]
    lines.push({
      text: (locale === undefined ? undefined : record.texts?.[locale]) ?? record.text,
      voiceKey: record.voiceKey,
      lineId: record.lineId,
      ...(speaker === undefined ? {} : { speaker })
    })
  }
  return lines
}

// While the story waits at a choice or a bridge:
const ambientBark = (player: LutePlayerHandle, bundle: Bundle.Bundle) => Effect.gen(function* () {
  const { state } = yield* SubscriptionRef.get(player.session.current)
  const candidates = yield* player.program.candidates(state, "ambient", "npc.guide")
  for (const candidate of candidates) {
    const lines = staticBarkLines(bundle, candidate)
    if (lines !== undefined) return yield* showAmbient(lines)
  }
})
```

`candidates`는 비트를 선택 순서대로 돌려줍니다. 헬퍼는 eligible인 lore entry를 골라 문서의 아티팩트(`index.documents`)에서 그 head를 찾고, head의 `body` 위치부터 다음 `entry`나 `beat` head 전까지의 레코드를 읽습니다. 그 레코드가 모두 `placeholders` 없는 `line`이면 그 bark는 **static**입니다. 각 대사는 텍스트(번들에 그 로케일이 있으면 `texts[locale]`), `index.cast`의 화자 표시 이름(내레이션은 없음), `voiceKey`, `lineId`를 줍니다.

이렇게 보여 준 bark는 아무것도 기록하지 않습니다. `entry.<id>.read`도, `once` 소비도, 쓰기도 없습니다. 기록해야 하는 bark — 첫 읽기, `once`, `::set` 같은 작성된 쓰기 — 나 본문이 동적인 bark(`{{…}}` placeholder, `match`)는 이야기가 `idle`로 돌아온 뒤 그 계기로 raise합니다.

```ts
yield* player.send({ type: "raiseOccasion", occasion: "ambient", target: "npc.guide" })
```

[0.45.0 §3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.45.0.md#3-background-barks-during-a-conversation-host-pattern)을 참고하세요.

## 0.40에서 올리기

화자 이름을 주는 플레이어 세션 옵션은 `cast`입니다(0.41.0부터). 세션을 `Session.layer`(`PlayerSession` 레이어)로 직접 조합하는 호스트는 0.40의 `castNames`(`ReadonlyMap`) 대신 `SessionOptions.cast`에 일반 레코드(cast id → 표시 이름), 즉 번들 인덱스의 `cast`를 넘깁니다. 예전 `castNamesFrom(artifacts)` 헬퍼는 없어졌으므로 이름은 `bundle.index.cast`에서 읽습니다.

```ts
import { Session } from "@lute-lang/player"

// 0.40: Session.layer({ bindings, castNames: Cues.castNamesFrom(artifacts) })
Session.layer({ bindings, cast: bundle.index.cast ?? {} })
```

`LutePlayer.make`는 번들에서 `cast`를 알아서 채웁니다. 0.41 migration 단계 전체는 [CHANGELOG](https://github.com/journeyWorker/lute/blob/v0.41.1/CHANGELOG.md#0411)에 있습니다.

## 핸들

`LutePlayer.make`는 `actions`, `view`, `send`, `snapshot`, `restore`, `shutdown`, `session`, `program`을 반환합니다.

- `snapshot`은 `Effect<SaveRuntime, Errors.RuntimeRejection>` 체크포인트입니다. 플레이어 세션의 lock을 잡으므로 `send`와 `restore`에 대해 선형화되며, batch가 연출 중일 때 호출해도 연출을 기다리지 않고 마지막으로 커밋된 런타임 스냅숏, 수락된 input log와 backlog(각 entry의 `voiceKey` 포함)를 반환합니다. presenter overlay, timer, profile state, renderer fiber는 저장하지 않습니다. 결과를 그대로 `restore(save)`에 넘기면 됩니다.
- `session`은 사용 중인 런타임 세션이고 `program`은 그 `session.program`입니다. bundle을 다시 로드하지 않고 `current`를 읽고 `events`를 구독하며 `view` / `evaluate` / `def` / `gate` / `candidates`를 실행합니다.
- `send(input)`은 batch가 돌지 않고 이야기가 `idle` 또는 `ended`에서 기다릴 때 런타임에 입력을 넘깁니다(`ended`에서는 `newRun` 또는 outside-run 계기만 받습니다). 그 밖에서는 `DirectorBusy { flow, story }`로 실패하고 아무것도 바꾸지 않습니다. 런타임이 거부하면 그 오류로 실패하고 director는 같은 await에 머뭅니다. 입력이 수락되어 batch가 시작되면 완료되므로, send하는 핸들러가 자기 연출을 기다리는 일은 없습니다. `{ type: "send", input }` action도 같은 동작입니다.
- **엔딩에 속한 쓰기 (0.44).** 이야기가 `ended`에 이르면 `hostWrite`는 거절됩니다(`RuntimeBusy`, 메시지가 이유를 알려 줍니다). 엔딩 메타데이터는 run을 끝내는 입력, 즉 `terminal`을 성립시키는 `raiseOccasion`, `advanceClock`, `bridgeResult`, `hostWrite`의 `writes`에 실어 보내세요(`send({ type: "raiseOccasion", occasion: "dawn", writes: { state: [{ path: "run.endedBy", value: "dawnSeven" }] } })`). `choose`, `timeout`, `worldEvent`는 쓰기를 싣지 않으므로, 이들이 run을 끝낸다면 그 상태를 스토리(갈래 안의 `::set`)나 그 전에 보낸 `hostWrite`에 기록하세요. `newRun`의 쓰기는 다음 run의 초기값이며, 그 run에서 끝난 run의 값은 `prev.run.*`로 읽힙니다.
- `restore(save)`는 boot를 제외한 모든 flow 상태에서 호스트 체크포인트(핸들의 `snapshot` — 런타임 스냅숏, input log, backlog)로 이어 갑니다. 진행 중인 연출을 버리고, 체크포인트 뒤에 추가된 읽음 표시를 지우고, `auto` 슬롯을 다시 쓰며(autosave가 꺼져 있지 않으면), stage를 다시 만들고 그 음악을 재생이 시작되기를 기다리지 않고 시작하며, 복원된 await를 다시 제시합니다(choice는 메뉴, bridge는 핸들러, idle은 idle 흐름, 엔딩은 엔딩 화면).

## 포트 계약

`Presenter`는 `stage(cue, mode)`, `showLine(line, mode)`, `revealLine`, `clearLine`, `restore(stage)`, `showChoice(menu, remaining)`, `hideChoice`, `showScreen(screen)`, `hideScreen`, `setUiHidden`, `applySettings`, `capture`와 `actions` stream을 구현합니다. stage 또는 line Effect는 시각 효과나 typewriter가 끝날 때 완료되어야 합니다. `mode: "skip"`은 끝 상태로 즉시 이동해야 합니다. load, rollback, presentation 시작에서 `restore`가 사용되므로 예전 레코드를 재생하지 말고 런타임 stage를 직접 복원합니다.

`StageCue.Vfx`는 `EffectSpec`을 담습니다. 카탈로그 효과(`flash`, `shake`, …)이거나, `{ effect: custom }` vfx 바인딩(`name` 또는 바인딩 키)이나 바인딩 없는 vfx 타입이면 `name`이 있는 `effect: "custom"`입니다. `cue.effect.effect === "custom"`으로 분기하고 `cue.effect.name`을 읽습니다. `LineView.speakerName`은 bundle index의 cast 이름(내레이션은 빈 문자열)이며 `emotion`과 `variant`는 line 자체의 값입니다.

`AudioPort`는 `play(channel, sound)`, `stop(channel, fade)`, `setVolumes`를 구현합니다. 채널은 `music`, `sfx`, `voice`입니다. `AudioPlay.file`은 이미 바인딩에서 해석되며 `loop`, `volume`(0–1), `fade`, `restart`가 명시됩니다. music의 `volume`은 레코드의 멤버를 `bindings.volumes`가 대응시킨 gain입니다(멤버 없음: 1, gain이 없는 멤버: 0). music 레코드는 `playback`을 따릅니다. `start`는 처음부터 재생하고(`restart: true`), `stop` / `fadeOut`은 멈추며(`duration`만큼 fade, `fadeOut` 기본 1초), 그 밖의 값은 해당 파일로 바꿉니다. 이미 재생 중인 파일을 `restart` 없이 재생하면 볼륨만 바뀝니다. `AssetLoader.ensure(files)`는 열린 scope 동안 파일을 유지하고 stream으로 progress를 보고합니다. 없는 파일은 `AssetMissing`로 실패합니다.

stage 복원(load, `restore`, rollback)은 그 음악 호출을 기다리지 않습니다. director가 음악 `play`(또는 `stop`)를 플레이어 scope의 fiber에서 시작하고 넘어가므로, 이야기가 이어질 때 그 호출이 아직 진행 중일 수 있습니다(플랫폼이 소리를 허용하기를, 또는 파일을 기다리는 중). `AudioPort`는 나중의 music `play`/`stop`이 아직 진행 중인 앞선 music `play`보다 우선하도록 해야 합니다. 앞선 호출은 시작하지 않고 완료되며, 이미 시작했다면 바로 멈춥니다. `PixiAudio`가 이렇게 하므로, 사용자 제스처 전의 load도 복원된 await에 바로 이르고 stage의 음악은 오디오가 풀릴 때 시작합니다.

## 대사 진행 주체

기본은 director가 대사를 진행합니다. `showLine`은 텍스트가 다 표시되면 완료되고, `advance`는 타이핑 중인 line을 드러내고 표시된 line을 해제합니다. 타이핑, 페이지 넘김, 확인 입력을 직접 처리하는 presenter에는 두 가지 방법이 있습니다.

- `linePacing: "presenter"`: `showLine`은 독자가 line을 해제했을 때 완료됩니다. 이때 `advance`는 line을 드러내거나 해제하지 않고 director는 line용 auto 타이머를 돌리지 않습니다(presenter가 `PlayerView.mode`를 읽음). `showLine(line, "skip")`은 여전히 즉시 완료됩니다.
- director 진행을 유지하고 `Presenter.confirmsLines: true`를 설정: 타이핑 중 들어온 `advance`를 드러내기로 소비하지 않고 보관했다가 line이 표시되면 해제로 적용합니다.

## Bridge

bridge 호출은 런타임을 중단합니다. tag로 핸들러를 추가합니다.

```ts
const host = {
  bridge: {
    "battle-result": (request) =>
      Effect.tryPromise(() => nativeBattle(request.attrs)).pipe(
        Effect.map((result) => ({
          fields: { won: result.won, score: result.score },
          writes: { state: [{ path: "run.gold", value: result.gold }] }
        }))
      )
  }
}
```

핸들러는 typed `BridgeAwait`(`request`, `tag`, `document`, `position`, 호출의 `attrs`, 답해야 할 typed `results`)를 받아 `BridgeAnswer { fields, writes? }`를 반환합니다. director가 `bridgeResult { request, fields, writes? }`를 보내고 cue 재생을 계속하며, writes는 뒤따르는 대사보다 먼저 반영됩니다. 핸들러는 별도 fiber에서 실행되므로 호스트가 게임을 진행하는 동안에도 메뉴가 동작합니다. 핸들러가 없으면 임의의 답을 만들지 않고 bridge-unhandled 실패를 보고합니다.

## Plugin 레코드

non-blocking plugin 지시문은 tag로 키를 둔 `PlayerHost.plugin`으로 호스트에 전달됩니다. 레코드는 대기 cue가 되어, batch 안 레코드 위치에서 `PluginCall { tag, attrs, document, position }`으로 핸들러를 실행하고 완료되면 batch를 이어 가며 skip 모드에서도 실행합니다(게임 동작은 연출이 아님). bridge로 답한 호출의 레코드는 `plugin`에 넘기지 않습니다. cue factory는 출력 이벤트마다 정확히 cue 하나를 순서대로 반환하므로 호스트가 factory를 감쌀 수도 있습니다.

`Director.make`도 같은 `bundle`, `bindings`, cue-factory wrapper, `host`
옵션으로 기본 factory를 만들 수 있으므로 hand-composed director에 별도
plugin wiring이 필요하지 않습니다. 미리 `Cues.make({ plugin })`으로 만든
factory와 `host.plugin`이 같은 tag를 처리하면 host handler가 우선하며,
양쪽의 고유 tag는 모두 남고 중복 handler는 한 번만 실행됩니다.
`Cues.make`로 만들지 않은 pre-composed factory에는 `host.plugin`을 합칠 수
없으므로 `Director.make`는 handler를 버리지 않고 die합니다. 이때는 wrapper를
넘기세요.

## Idle 흐름

`flow.newGame`은 runtime 시작 뒤 설정한 계기를 발생시킵니다. 첫 새 게임은 세션 시작(`started`)에 이어 그 계기를 제시하고, 이후 타이틀에서 시작한 새 게임은 시작을 다시 재생하지 않고 새 run(`newRun`, `user.*` 진행 유지)을 시작합니다. `idle`에서 `flow.idle`은 입력을 자동으로 보내거나 이름 있는 계기 메뉴를 표시할 수 있습니다. `PlayerHost.idle(await_)`가 별도 fiber에서 먼저 실행되며 `Input`을 반환해 bindings flow를 덮어쓸 수 있습니다. `undefined`를 반환하면 `flow.idle`에 위임합니다. 자기 게임(맵, HUD)에서 이야기를 구동하는 호스트는 `undefined`를 반환하고 게임이 움직일 때 `send`를 호출합니다. `send`는 대기 중인 `idle`을 중단합니다.

엔딩(`ended`, ended 스냅숏에서 복원한 세션 포함)에서의 새 게임은 `newRun`을 보내며 `started`를 다시 제시하지 않습니다. 엔딩에서 `toTitle`은 진행 상황 폐기 확인 없이 바로 타이틀로 갑니다(0.47: 끝난 run에는 잃을 것이 없고 lineage는 이미 저장되어 있습니다). 게임 내 메뉴와 오류 흐름의 `toTitle`은 여전히 확인을 묻습니다.

## 자동 저장

`autosave`는 `auto` 슬롯을 제어합니다. 생략하면 presentation 시작과 모든 choice에서 쓰고, `"presentation"` 또는 `"choice"`는 그곳에서만, `false`는 쓰지 않습니다(스냅숏도 `capture`도 없음). 퀵 저장과 슬롯 저장은 영향을 받지 않습니다.

## 저장과 프로필 데이터

코어는 Effect `KeyValueStore`로 저장합니다. 슬롯은 `quick`, `auto`, 번호 슬롯 `1..24`입니다. 저장 형식은 다음과 같습니다.

```json
{
  "saveVersion": "0.47",
  "project": "<bundle fingerprint>",
  "savedAt": "<ISO timestamp>",
  "label": "Chapter 1",
  "runtime": { "snapshot": "<runtime snapshot>", "log": [], "backlog": [] },
  "thumbnail": "data:image/png;base64,..."
}
```

런타임 스냅숏에는 `world.stage`와 필요한 경우 `continuation.stage`가 포함되고, 플레이어는 deterministic input log와 backlog를 추가합니다. 프로필은 `profileVersion`(릴리스 라인, `"0.47"`), project fingerprint, `readLines`, settings을 사용합니다. 슬롯 키(`save:quick`, `save:auto`, `save:1`…)에는 namespace가 없습니다. 각 저장이 project fingerprint를 담고 있어 다른 프로젝트의 저장은 거절되며, 프로필은 `profile:<project>`에 저장됩니다. 게임이 다른 데이터도 저장한다면 플레이어에게 별도(또는 접두사를 붙인) store를 주세요. 다른 프로젝트나 형식 버전이면 `SaveIncompatible`, 손상된 데이터면 `SaveCorrupt`를 반환합니다.

0.47부터 프로필은 기록된 **lineage**도 담습니다(아래 회차 간 모델 참고). 다른 `profileVersion`의 프로필도 거절되지 않습니다. 읽기 기록과 settings은 유지되고 lineage는 없습니다.

## 두 번째 회차: 회차 간 모델 (0.47)

페이지를 새로 고쳐도 남는 것:

- **사용자 tier** — `user.*` 값, user tier 사실, `once: user` 소모, entry 읽기 표시 — 와 **끝난 run의 값**(`newRun`이 `prev.run.*`로 바꿉니다). 둘 다 런타임 스냅숏에 있으며 lineage를 통해 새로 고침 뒤에도 남습니다.
- **읽기 기록과 settings**(`readLines`, `settings`) — 프로필에 있습니다.
- **진행 중인 run** — 저장(퀵, 자동, 슬롯)으로만 남습니다.

**lineage를 쓰는 때.** 세션이 `ended` 출력을 제시할 때마다 플레이어는 런타임 스냅숏(`Session.save`)을 떠서 프로필의 `lineage`로 저장하고 이전 것을 대체합니다. settings처럼 debounce하지 않고 바로 씁니다.

**lineage를 쓰는 곳.** `snapshot` / `session` 없이 시작한 플레이어(기본, 또는 호스트 `seed`)에서, 세션이 기다리는 곳에서 `newRun`을 보낼 수 없는 새 게임 — 새로 고침 뒤 손대지 않은 세션, 또는 메뉴로 버린 run — 은 시작 지점 대신 lineage를 복원한 뒤 `newRun`을 보냅니다. 이때 input log는 `{ snapshot: <lineage> }`로 시작합니다. lineage가 있는 동안 새 게임은 호스트 `seed`를 쓰지 않습니다. 런타임이 lineage를 거절하면(예: 다른 빌드의 스냅숏) 새 게임은 lineage가 없을 때처럼 시작 지점에서 시작하고 lineage는 버려집니다. `snapshot`이나 `session`을 넘긴 호스트는 상태를 스스로 고른 것이므로, 엔딩은 lineage를 쓰지만 새 게임은 그것을 복원하지 않습니다. `user.*`를 새로 고침 너머로 직접 옮기던 호스트는 그 코드를 지워도 됩니다.

## Locale (0.47)

locale은 마운트 옵션이 아니라 설정(`settings.locale`)입니다. 불러온 번들의 **원본 locale**은 프로젝트 index의 `contentLang`(매니페스트의 `defaults.contentLang`)이고, 없으면 meta에 `contentLang`을 선언한 첫 artifact(`index.documents` 순서)의 값, 그것도 없으면 `"en"`입니다. **컴파일된 locale**은 artifact `texts`와 `promptTexts` 맵의 키이므로(`lute compile --locale-bundle`) 프롬프트만 번역한 locale도 포함됩니다. `Settings` 화면 모델의 `locales`는 원본 locale 다음에 컴파일된 locale(정렬, 중복 없음)이 옵니다. 새 프로필은 원본 locale로 시작하고, 번들에 없는 저장된 locale(`en-US` 프로젝트에서 0.46 프로필의 `"en"`)은 원본 텍스트를 읽으며 `Settings`는 원본 locale을 선택된 것으로 보여 줍니다.

현재 locale에 대해 대사(`texts[locale]`, `localeSegments[locale]`), choice와 hub 선택지 라벨(`texts[locale]`), branch와 hub 프롬프트(`promptTexts[locale]`), 백로그는 번역을 읽고 없으면 원본 텍스트로 돌아갑니다. 화자 이름은 현지화하지 않습니다. `setSettings({ locale })`은 다음에 보이는 텍스트와 다음에 여는 백로그에 적용되며 새 게임이 필요 없습니다. cue factory는 호출마다 locale을 받습니다(`fromOutput(output, readLines, locale)`, `choiceView(await, locale)`). `cueFactory` wrapper나 직접 만든 factory도 그것을 받아 그대로 넘깁니다.

대사와 백로그 항목은 컴파일된 artifact가 아니라 런타임의 `line` 레코드에서 번역을 읽습니다. 이 레코드의 `texts`와 `localeSegments`는 모든 `{{…}}` 마커가 이미 값으로 바뀌어 있습니다. 번역의 마커는 원본 line에서 참조 대상이 같은 마커(`{{ }}` 안의 텍스트, 앞뒤 공백 제외)에 대해 렌더링한 값을 받으므로 번역자가 마커 순서를 바꿔도 됩니다. 원본 line에 없는 마커는 쓴 그대로 남고, 번역에도 리터럴 `{{`를 `\{{`로 쓸 수 있습니다(lute 0.47.1). 레코드는 디코딩된 텍스트를 담으며, placeholder를 직접 치환하는 엔진은 IR의 이스케이프된 보간 읽기 규칙을 적용해야 합니다([현지화 텍스트](/ko/tooling/runtime-contract/#현지화-텍스트) 참고). 선택지 라벨은 0.46과 같습니다(`{{…}}` 마커를 렌더링하지 않음).

## 롤백과 기록

세션은 `settings.rollbackDepth`(기본 100)개의 input point를 보관합니다. 각 point에는 runtime state, await, stage, backlog 길이와 log 길이가 있습니다. rollback은 현재 cue fiber를 중단하고 선택한 point의 stage를 `Presenter.restore`로 복원한 뒤 음악을 재생이 시작되기를 기다리지 않고 시작하고 해당 await에서 재개합니다. 이전 point가 없으면 `NothingToRollback`입니다. XState actor 스냅숏은 저장하지 않습니다.

## 읽기 기록과 백로그

각 line에는 안정적인 `lineId`가 있습니다. profile의 `readLines` set은 line 표시 뒤 해당 id를 기록합니다. `LineView.read`가 읽기 인식 skip을 제어하며 기본 read-only skip은 읽지 않은 line, choice, bridge, idle에서 멈춥니다. backlog는 line id, voice key, speaker name, 원본 text, `hasVoice`와 line의 번역(`texts`, 있으면 `localeSegments`; 저장된 backlog도 유지)을 보관합니다. `Backlog` 화면 모델의 항목 `text`는 화면을 여는 시점의 locale로 이미 해석되어 있습니다.

## ChoiceView와 복원된 자산

`Presenter.showChoice`는 작성된 prompt와 locale별 option text, runtime
verdict, timeout, default, once, exit 정보를 담은 `ChoiceView`를 받습니다.
load와 rollback에서는 director가 `Presenter.restore`를 호출하기 전에 복원된
stage의 바인딩 파일을 먼저 보장합니다.
