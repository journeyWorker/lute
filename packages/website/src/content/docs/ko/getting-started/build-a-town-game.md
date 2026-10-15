---
title: 마을 게임 만들기
description: Lute 0.46과 player-web으로 시계가 있는 작은 마을을 만듭니다.
---

이 튜토리얼에서는 작은 Stardew 스타일 마을 **Quayside**를 만듭니다. 완성된
프로젝트는 [`docs/examples/quayside/`](https://github.com/journeyWorker/lute/tree/main/docs/examples/quayside)에
있습니다. 아래의 코드는 프로젝트의 실제 파일이므로 문서 검사기가 읽는 즉시
컴파일합니다.

## 1. 스키마와 시계

먼저 상태 도메인을 선언합니다. `run.slot`은 이름 있는 `slot` 도메인이고,
그냥 문자열이 아닙니다. `run.ending`은 두 결말을 포함하는 닫힌 도메인입니다.
따라서 `none`이 아닌 결말을 선택하는 순간 `terminal` 식이 런을 끝냅니다.

```yaml check="docs/examples/quayside/world.schema.yaml"
enums:
  slot: [morning, afternoon, evening]
  ending: [none, restore, retire]
state:
  run.day: { type: int, default: 1, owner: engine }
  run.slot: { type: { domain: slot }, default: morning, owner: engine }
  run.ending: { type: { domain: ending }, default: none }
  run.visits: { type: int, default: 0 }
clock:
  day: run.day
  slot: run.slot
  slots: [morning, afternoon, evening]
  raise: { slot: slotStart, dayEnd: dayEnd }
  raiseAtStart: true
  week:
    length: 7
    first: 0
    labels: [Monday, Tuesday, Wednesday, Thursday, Friday, Saturday, Sunday]
entities:
  place: { members: [square, pier, bakery] }
relations: {}
defs:
  currentSlot: "run.slot"
terminal: "run.ending != 'none'"
cast:
  player: { name: You }
  june: { name: June }
  tom: { name: Tom }
```

`clock.index`는 순서를 위한 숫자로 유지됩니다. HUD에서 표시할 슬롯 이름과
요일 이름은 직접 숫자로 만들지 말고 `program.view(state).clock`에서 읽으세요.

## 2. 장면, 비트, 바크

각 슬롯마다 한 arm을 갖는 배경 컴포넌트를 만듭니다. 디렉티브 필드는 빌드
시점 값이므로 현재 슬롯을 컴포넌트에 전달하고 각 `::bg`는 리터럴로 둡니다.
시계 정책이 필요한 모든 장면에서 이 컴포넌트를 사용합니다.

```lute check="docs/examples/quayside/components/clock-backdrop.component.lute"
---
component: clockBackdrop
params:
  slot: { type: { domain: slot } }
---
## Backdrop
<match subject="@slot">
  <when is="morning">
    ::bg{location="quayside" time="morning"}
  </when>
  <when is="afternoon">
    ::bg{location="quayside" time="afternoon"}
  </when>
  <when is="evening">
    ::bg{location="quayside" time="evening"}
  </when>
</match>
```

`start` 비트로 마을을 소개합니다.

```lute check="docs/examples/quayside/scenes/opening.lute"
---
kind: scene
id: opening
components: [../components/clock-backdrop.component.lute]
on: start
priority: 10
---

## Morning at Quayside

::use{component="clockBackdrop" slot=@currentSlot instance="opening-backdrop"}
@june: The tide is out, and Quayside is already awake.
@narrator: A hand-painted sign points toward the square, the pier, and the bakery.
```

방문 비트는 선택지를 보여 주고 결말을 기록할 수 있습니다.

```lute check="docs/examples/quayside/scenes/market.lute"
---
kind: scene
id: market
components: [../components/clock-backdrop.component.lute]
on: visit
target: place.square
priority: 10
once: false
---

## The market square

::use{component="clockBackdrop" slot=@currentSlot instance="market-backdrop"}
@june: Every stall has something to mend, trade, or remember.
::set{run.visits += 1}

<branch id="closingBell" prompt="The harbor bell rings. What will you do with the old town hall?">
  <choice id="restore" text="Restore it as a community hall">
    ::set{run.ending = 'restore'}
    @june: We will fill it with lanterns and neighbors.
  </choice>
  <choice id="wait" text="Wait for the bell to ring again">
    @june: We can take one more evening before deciding.
  </choice>
  <choice id="retire" text="Let the building rest with the tide">
    ::set{run.ending = 'retire'}
    @june: Some doors close gently, and that is a kind of care.
  </choice>
</branch>

<match subject="run.ending">
  <when is="restore">
    @narrator: The square keeps its lights on for the hall.
  </when>
  <when is="retire">
    @narrator: The square grows quiet around the old hall.
  </when>
  <when is="none">
    @narrator: The bell waits for your answer.
  </when>
</match>
```

`tide` 비트는 저녁까지 게이트가 닫혀 있습니다.


```lute check="docs/examples/quayside/scenes/tide.lute"
---
kind: scene
id: tide
components: [../components/clock-backdrop.component.lute]
on: tide
---

## Evening tide

::use{component="clockBackdrop" slot=@currentSlot instance="tide-backdrop"}
@narrator: At evening tide, the town lights come on one by one.
```

로어 문서는 정적인 배경 바크를 제공합니다. 커스텀 호스트가 플레이어가
기다리는 동안 보여 주어도 상태를 쓰거나 프레젠테이션을 소비하지 않습니다.

```lute check="docs/examples/quayside/lore/barks.lute"
---
kind: lore
id: barks
title: Quayside background voices
---

<entry id="marketBark" category="bark" on="bark" priority="-1">
  @june: Fresh bread, repaired nets, and one more story before sunset.
</entry>

<entry id="pierBark" category="bark" on="visit" target="place.pier" priority="-1">
  @tom: Mind the loose board. The pier remembers every storm.
</entry>
```

## 3. 플레이, 거부, 커버리지

두 결말을 각각 플레이합니다. 첫 스크립트는 아침에 `tide`를 일부러 발생시킵니다.
거부를 기대하면 상태는 바뀌지 않고 다음 단계로 계속 진행합니다.

```yaml check="docs/examples/quayside/plays/restore.play.yaml"
# Rebuild the old town hall and keep Quayside's doors open.
steps:
  - occasion: start
    expect: { winner: opening }
  - occasion: tide
    expect: { refused: E-OCCASION-GATE, state: { run.ending: none } }
  - occasion: visit
    target: place.square
    choose: { closingBell: restore }
    expect: { winner: market, state: { run.ending: restore } }
expect:
  end: terminal
  state: { run.ending: restore }
```

```yaml check="docs/examples/quayside/plays/retire.play.yaml"
# Let the old town hall rest with the tide.
steps:
  - occasion: start
    expect: { winner: opening }
  - occasion: visit
    target: place.square
    choose: { closingBell: retire }
    expect: { winner: market, state: { run.ending: retire } }
expect:
  end: terminal
  state: { run.ending: retire }
```

커버리지 플레이는 오후와 저녁까지 진행하고 `tide`를 열며 두 정적 바크도
제시합니다:

```yaml check="docs/examples/quayside/plays/coverage.play.yaml"
# Exercise every backdrop slot, the gated occasion, the static barks, and ending none.
steps:
  - occasion: start
    expect: { winner: opening }
  - occasion: bark
    expect: { winner: marketBark }
  - occasion: visit
    target: place.pier
    expect: { winner: pierBark }
  - occasion: visit
    target: place.square
    choose: { closingBell: wait }
    expect: { winner: market, state: { run.ending: none } }
  - advance: { to: afternoon }
  - occasion: visit
    target: place.square
    choose: { closingBell: wait }
    expect: { winner: market, state: { run.ending: none } }
  - advance: { to: evening }
  - occasion: tide
    expect: { winner: tide }
  - occasion: visit
    target: place.square
    choose: { closingBell: retire }
    expect: { winner: market, state: { run.ending: retire } }
expect:
  end: terminal
  state: { run.ending: retire }
```

이제 coverage는 market 배경 match `3/3`, ending match `3/3`, 세 branch
선택지, 두 bark entry를 보고합니다. opening과 tide 컴포넌트는 의도적으로
`1/3`입니다. 각각 런 시작(아침)과 저녁 게이트에서만 진입하기 때문입니다.

## 4. 바인딩과 이름 있는 결말 화면

바인딩 경로는 `assetsBase` 아래에서 해석됩니다. `ui.title.music`은 `assets:`의
`title-music` 키를 사용하고, `ui.ended`는 `run.ending`을 읽어 각 결말 화면을
선택합니다.

```yaml check="docs/examples/quayside/player.bindings.yaml"
bindingsVersion: "0.46"
canvas: { width: 1280, height: 720 }
assetsBase: assets/
assets:
  title-music: audio/title.wav
backgrounds:
  quayside:
    image: bg/quayside-morning.png
    times:
      morning: bg/quayside-morning.png
      afternoon: bg/quayside-afternoon.png
      evening: bg/quayside-evening.png
characters:
  june:
    sprite: { default: char/june.png }
    nameColor: "#f2b56b"
  tom:
    sprite: { default: char/tom.png }
    nameColor: "#78b5e8"
ui:
  font: "Georgia, serif"
  title: { background: bg/title.png, music: title-music }
  ended:
    by: run.ending
    screens:
      restore: { title: "The Hall Opens", text: "Quayside gathers under one roof.", background: bg/restore.png }
      retire: { title: "A Gentle Closing", text: "The town lets the old hall rest.", background: bg/retire.png }
```

참조한 placeholder 파일은 모두 `docs/examples/quayside/assets/`에 커밋되어
있습니다. 전체 형식은 [바인딩 레퍼런스](/ko/player/bindings/)를 참고하세요.

## 5. player-web 호스트

걸어 다니는 마을 지도는 호스트의 Pixi 애플리케이션이 소유하게 합니다.
`player-web`은 비공개 로더를 사용하며 Pixi 전역 `Assets`를 초기화하지 않습니다.
따라서 마운트 전후에 호스트가 지도를 로드할 수 있습니다.

```ts
import { Assets, Application, Sprite } from "pixi.js"
import { mountPlayer } from "@lute-lang/player-web"

const app = new Application()
await app.init({ resizeTo: window })
const map = Sprite.from(await Assets.load("/town-map.png"))
app.stage.addChild(map) // 플레이어 바깥에 있는 호스트 지도입니다.

// `Assets`는 지도 앱만 소유하며 player-web은 비공개 로더를 사용합니다.
const options = {
  bindings: { url: "/quayside/player.bindings.yaml" },
  bundleUrl: "/quayside/bundle/",
  wasmUrl: "/wasm/lute_runtime_wasm_bg.wasm"
} as const

// Promise facade가 unmount까지 마운트 Scope를 유지합니다.
const player = await mountPlayer(document.querySelector("#player")!, options)
```

그 `player`는 페이지가 살아 있는 동안 유지하고, 페이지를 떠날 때
`player.unmount()`를 호출합니다. `program` 질의는 세션의 현재 상태
(`player.session.current`)를 받습니다. `program.gate`로 먼저 확인한 뒤 열려 있을
때만 `send`로 계기를 발생시킵니다.

```ts
import { Effect, SubscriptionRef } from "effect"

const current = await Effect.runPromise(SubscriptionRef.get(player.session.current))
const gate = await Effect.runPromise(player.program.gate(current.state, "tide"))
if (gate.open) await player.send({ type: "raiseOccasion", occasion: "tide" })
```

선택지나 브리지에서 기다리는 동안에는 두 번째 프레젠테이션을 시작하지 않고
정적 바크를 보여 줄 수 있습니다. `program.candidates`에서 적격 `entry`를
찾고 `player.bundle`의 placeholder 없는 줄을 읽습니다. 전체 artifact 순회
도우미는 [커스텀 호스트](/ko/player/custom-hosts/)에 있으며 핵심 패턴은 다음과
같습니다.

```ts
declare const showAmbientFromBundle: (bundle: unknown, candidate: unknown) => void
const current = await Effect.runPromise(SubscriptionRef.get(player.session.current))
const candidates = await Effect.runPromise(
  player.program.candidates(current.state, "visit", "place.pier")
)
const bark = candidates.find((candidate) => candidate.kind === "entry")
if (bark !== undefined) {
  showAmbientFromBundle(player.bundle, bark) // 읽기/once 기록이 없는 정적 줄
}
```

placeholder, `match`, `once`, 또는 쓰기가 필요한 바크는 idle로 돌아온 뒤
`send`로 발생시켜야 합니다.

## 6. 저장, 재개, 시계 HUD

Promise facade도 Effect 핸들과 같은 저장 작업을 제공합니다. `save.snapshot`으로
새 마운트를 시작하고 전체 저장을 복원합니다.

```ts
import { mountPlayer } from "@lute-lang/player-web"

const player = await mountPlayer(document.querySelector("#player")!, options)
const save = await player.snapshot()
await player.unmount()

const resumed = await mountPlayer(document.querySelector("#player")!, {
  ...options,
  snapshot: save.snapshot
})
await resumed.restore(save)
```

Effect API에서도 같은 옵션을 사용합니다. `mount`는 `Scope`가 필요하고 플레이어는
정확히 그 스코프만큼 살아 있으므로, 페이지 수명 동안 유지되는 스코프로 실행하세요
(`Effect.scoped`는 이펙트가 끝나는 즉시 스코프를 닫습니다).

```ts
import { Effect } from "effect"
import { mount } from "@lute-lang/player-web"

// Effect<void, unknown, Scope.Scope>: 실행할 때 페이지의 스코프를 제공합니다.
const resumeInPageScope = Effect.gen(function* () {
  const resumed = yield* mount(document.querySelector("#player")!, {
    ...options,
    snapshot: save.snapshot
  })
  yield* resumed.restore(save)
})
```

HUD는 읽기 전용 view를 사용합니다. 순서에는 `program.clock`을 사용할 수 있지만,
표시 이름은 view가 제공하는 값을 사용하세요.

```ts
const current = await Effect.runPromise(SubscriptionRef.get(player.session.current))
const world = await Effect.runPromise(player.program.view(current.state))
const clock = world.clock
const label = clock == null
  ? "No clock"
  : `${clock.weekdayLabel ?? `Day ${clock.day}`} — ${clock.slot ?? "slot"}`
```

다음으로 [시계](/ko/language/clock/), [스토리 플레이](/ko/tooling/play/),
[런타임 계약](/ko/tooling/runtime-contract/)을 읽어 이 튜토리얼의 언어와 호스트
표면을 더 알아보세요.
