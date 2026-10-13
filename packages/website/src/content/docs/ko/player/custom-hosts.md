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
- `send(input)`은 batch가 돌지 않고 이야기가 `idle` 또는 `ended`에서 기다릴 때 런타임에 입력을 넘깁니다(엔딩에서 `newRun`이나 outside-run 계기를 보내면 이어서 진행됩니다). 그 밖에서는 `DirectorBusy { flow, story }`로 실패하고 아무것도 바꾸지 않습니다. 런타임이 거부하면 그 오류로 실패하고 director는 같은 await에 머뭅니다. 입력이 수락되어 batch가 시작되면 완료되므로, send하는 핸들러가 자기 연출을 기다리는 일은 없습니다. `{ type: "send", input }` action도 같은 동작입니다.
- `restore(save)`는 boot를 제외한 모든 flow 상태에서 호스트 체크포인트(핸들의 `snapshot` — 런타임 스냅숏, input log, backlog)로 이어 갑니다. 진행 중인 연출을 버리고, 체크포인트 뒤에 추가된 읽음 표시를 지우고, `auto` 슬롯을 다시 쓰며(autosave가 꺼져 있지 않으면), stage와 음악을 다시 만들고, 복원된 await를 다시 제시합니다(choice는 메뉴, bridge는 핸들러, idle은 idle 흐름, 엔딩은 엔딩 화면).

## 포트 계약

`Presenter`는 `stage(cue, mode)`, `showLine(line, mode)`, `revealLine`, `clearLine`, `restore(stage)`, `showChoice(menu, remaining)`, `hideChoice`, `showScreen(screen)`, `hideScreen`, `setUiHidden`, `applySettings`, `capture`와 `actions` stream을 구현합니다. stage 또는 line Effect는 시각 효과나 typewriter가 끝날 때 완료되어야 합니다. `mode: "skip"`은 끝 상태로 즉시 이동해야 합니다. load, rollback, presentation 시작에서 `restore`가 사용되므로 예전 레코드를 재생하지 말고 런타임 stage를 직접 복원합니다.

`StageCue.Vfx`는 `EffectSpec`을 담습니다. 카탈로그 효과(`flash`, `shake`, …)이거나, `{ effect: custom }` vfx 바인딩(`name` 또는 바인딩 키)이나 바인딩 없는 vfx 타입이면 `name`이 있는 `effect: "custom"`입니다. `cue.effect.effect === "custom"`으로 분기하고 `cue.effect.name`을 읽습니다. `LineView.speakerName`은 bundle index의 cast 이름(내레이션은 빈 문자열)이며 `emotion`과 `variant`는 line 자체의 값입니다.

`AudioPort`는 `play(channel, sound)`, `stop(channel, fade)`, `setVolumes`를 구현합니다. 채널은 `music`, `sfx`, `voice`입니다. `AudioPlay.file`은 이미 바인딩에서 해석되며 `loop`, `volume`(0–1), `fade`, `restart`가 명시됩니다. music의 `volume`은 레코드의 멤버를 `bindings.volumes`가 대응시킨 gain입니다(멤버 없음: 1, gain이 없는 멤버: 0). music 레코드는 `playback`을 따릅니다. `start`는 처음부터 재생하고(`restart: true`), `stop` / `fadeOut`은 멈추며(`duration`만큼 fade, `fadeOut` 기본 1초), 그 밖의 값은 해당 파일로 바꿉니다. 이미 재생 중인 파일을 `restart` 없이 재생하면 볼륨만 바뀝니다. `AssetLoader.ensure(files)`는 열린 scope 동안 파일을 유지하고 stream으로 progress를 보고합니다. 없는 파일은 `AssetMissing`로 실패합니다.

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

## 자동 저장

`autosave`는 `auto` 슬롯을 제어합니다. 생략하면 presentation 시작과 모든 choice에서 쓰고, `"presentation"` 또는 `"choice"`는 그곳에서만, `false`는 쓰지 않습니다(스냅숏도 `capture`도 없음). 퀵 저장과 슬롯 저장은 영향을 받지 않습니다.

## 저장과 프로필 데이터

코어는 Effect `KeyValueStore`로 저장합니다. 슬롯은 `quick`, `auto`, 번호 슬롯 `1..24`입니다. 저장 형식은 다음과 같습니다.

```json
{
  "saveVersion": "0.41",
  "project": "<bundle fingerprint>",
  "savedAt": "<ISO timestamp>",
  "label": "Chapter 1",
  "runtime": { "snapshot": "<runtime snapshot>", "log": [], "backlog": [] },
  "thumbnail": "data:image/png;base64,..."
}
```

런타임 스냅숏에는 `world.stage`와 필요한 경우 `continuation.stage`가 포함되고, 플레이어는 deterministic input log와 backlog를 추가합니다. 프로필은 `profileVersion: "0.41"`, project fingerprint, `readLines`, settings을 사용합니다. 다른 프로젝트나 형식 버전이면 `SaveIncompatible`, 손상된 데이터면 `SaveCorrupt`를 반환합니다.

## 롤백과 기록

세션은 `settings.rollbackDepth`(기본 100)개의 input point를 보관합니다. 각 point에는 runtime state, await, stage, backlog 길이와 log 길이가 있습니다. rollback은 현재 cue fiber를 중단하고 선택한 point의 stage를 `Presenter.restore`로 복원한 뒤 음악을 재시작하고 해당 await에서 재개합니다. 이전 point가 없으면 `NothingToRollback`입니다. XState actor 스냅숏은 저장하지 않습니다.

## 읽기 기록과 백로그

각 line에는 안정적인 `lineId`가 있습니다. profile의 `readLines` set은 line 표시 뒤 해당 id를 기록합니다. `LineView.read`가 읽기 인식 skip을 제어하며 기본 read-only skip은 읽지 않은 line, choice, bridge, idle에서 멈춥니다. backlog는 line id, voice key, speaker name, text, `hasVoice`를 보관합니다.

## ChoiceView와 복원된 자산

`Presenter.showChoice`는 작성된 prompt와 locale별 option text, runtime
verdict, timeout, default, once, exit 정보를 담은 `ChoiceView`를 받습니다.
load와 rollback에서는 director가 `Presenter.restore`를 호출하기 전에 복원된
stage의 바인딩 파일을 먼저 보장합니다.
