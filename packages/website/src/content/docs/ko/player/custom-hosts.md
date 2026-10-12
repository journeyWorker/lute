---
title: 커스텀 플레이어 호스트
description: Presenter, AudioPort, AssetLoader, PlayerHost로 렌더러 독립 Lute 플레이어 코어의 native 또는 테스트 호스트를 만듭니다.
---

PixiJS가 맞지 않을 때는 `@lute-lang/player`를 직접 사용합니다. 코어에는 네 가지 경계가 있습니다. `Presenter`는 cue와 화면을 렌더링하고, `AudioPort`는 바인딩된 사운드를 재생하며, `AssetLoader`는 바인딩된 파일을 로드하고, `PlayerHost`는 호스트별 bridge와 idle 정책을 처리합니다.

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
    bridge: { "save-score": (request) => sendToNativeHost(request) },
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

`LutePlayerOptions`에는 `bindings`, `presenter`, `audio`, `assets`가 필요하며 `host`와 `seed`는 선택 사항입니다. 바인딩은 디코딩된 값이나 YAML 문자열일 수 있습니다. `LutePlayer.make`를 scope 안에서 실행하기 전에 runtime, bundle, input-log, key-value-store 서비스를 제공합니다.

## 포트 계약

`Presenter`는 `stage(cue, mode)`, `showLine(line, mode)`, `revealLine`, `clearLine`, `restore(stage)`, `showChoice(menu, remaining)`, `hideChoice`, `showScreen(screen)`, `hideScreen`, `setUiHidden`, `applySettings`, `capture`와 `actions` stream을 구현합니다. stage 또는 line Effect는 시각 효과나 typewriter가 끝날 때 완료되어야 합니다. `mode: "skip"`은 끝 상태로 즉시 이동해야 합니다. load, rollback, presentation 시작에서 `restore`가 사용되므로 예전 레코드를 재생하지 말고 런타임 stage를 직접 복원합니다.

`AudioPort`는 `play(channel, sound)`, `stop(channel, fade)`, `setVolumes`를 구현합니다. 채널은 `music`, `sfx`, `voice`입니다. `AudioPlay.file`은 이미 바인딩에서 해석되며 `loop`, `volume`(0–1), `fade`가 명시됩니다. `AssetLoader.ensure(files)`는 열린 scope 동안 파일을 유지하고 stream으로 progress를 보고합니다. 없는 파일은 `AssetMissing`로 실패합니다.

## Bridge

bridge 호출은 런타임을 중단합니다. tag로 핸들러를 추가합니다.

```ts
const host = {
  bridge: {
    "battle-result": (request) =>
      Effect.tryPromise(() => nativeBattle(request.fields)).pipe(
        Effect.map((fields) => ({ won: fields.won, score: fields.score }))
      )
  }
}
```

핸들러는 typed `BridgeAwait`를 받고 record Effect를 반환합니다. director가 결과를 `bridgeResult`로 보내고 cue 재생을 계속합니다. 핸들러가 없으면 임의의 답을 만들지 않고 bridge-unhandled 실패를 보고합니다.

## Idle 흐름

`flow.newGame`은 runtime 시작 뒤 설정한 계기를 발생시킵니다. `idle`에서 `flow.idle`은 입력을 자동으로 보내거나 이름 있는 계기 메뉴를 표시할 수 있습니다. `PlayerHost.idle(await_)`가 먼저 실행되며 `Input`을 반환해 bindings flow를 덮어쓸 수 있습니다. `undefined`를 반환하면 `flow.idle`에 위임합니다.

## 저장과 프로필 데이터

코어는 Effect `KeyValueStore`로 저장합니다. 슬롯은 `quick`, `auto`, 번호 슬롯 `1..24`입니다. 저장 형식은 다음과 같습니다.

```json
{
  "saveVersion": "0.40",
  "project": "<bundle fingerprint>",
  "savedAt": "<ISO timestamp>",
  "label": "Chapter 1",
  "runtime": { "snapshot": "<runtime snapshot>", "log": [], "backlog": [] },
  "thumbnail": "data:image/png;base64,..."
}
```

런타임 스냅숏에는 `world.stage`와 필요한 경우 `continuation.stage`가 포함되고, 플레이어는 deterministic input log와 backlog를 추가합니다. 프로필은 `profileVersion: "0.40"`, project fingerprint, `readLines`, settings을 사용합니다. 다른 프로젝트나 형식 버전이면 `SaveIncompatible`, 손상된 데이터면 `SaveCorrupt`를 반환합니다.

## 롤백과 기록

세션은 `settings.rollbackDepth`(기본 100)개의 input point를 보관합니다. 각 point에는 runtime state, await, stage, backlog 길이와 log 길이가 있습니다. rollback은 현재 cue fiber를 중단하고 선택한 point의 stage를 `Presenter.restore`로 복원한 뒤 음악을 재시작하고 해당 await에서 재개합니다. 이전 point가 없으면 `NothingToRollback`입니다. XState actor 스냅숏은 저장하지 않습니다.

## 읽기 기록과 백로그

각 line에는 안정적인 `lineId`가 있습니다. profile의 `readLines` set은 line 표시 뒤 해당 id를 기록합니다. `LineView.read`가 읽기 인식 skip을 제어하며 기본 read-only skip은 읽지 않은 line, choice, bridge, idle에서 멈춥니다. backlog는 line id, voice key, speaker name, text, `hasVoice`를 보관합니다.

## ChoiceView와 복원된 자산

`Presenter.showChoice`는 작성된 prompt와 locale별 option text, runtime
verdict, timeout, default, once, exit 정보를 담은 `ChoiceView`를 받습니다.
load와 rollback에서는 director가 `Presenter.restore`를 호출하기 전에 복원된
stage의 바인딩 파일을 먼저 보장합니다.
