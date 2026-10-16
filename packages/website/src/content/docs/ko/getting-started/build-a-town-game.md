---
title: 마을 게임 만들기
description: Lute와 player-web으로 시계가 있는 작은 마을을 만듭니다 — 결말, 두 번째 회차, 한국어 번역까지.
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
  user.endings: { type: int, default: 0 }
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
`user.endings`는 유일한 user tier 경로입니다. 런이 끝나도 남으며
[7단계](#7-두-번째-회차)에서 사용합니다.

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

`start` 비트로 마을을 소개합니다. 각 줄에는 `lute tag`가 한 번 써 넣는 안정적인
`code`가 있고([8단계](#8-quayside-번역하기) 참고), 게이트가 걸린 줄은 돌아온
플레이어를 맞이합니다([7단계](#7-두-번째-회차) 참고).

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
@june{code="0010" when="has(prev.run.ending) && prev.run.ending == 'restore'"}: The hall's lanterns burned all night. Welcome back.
@june{code="0020" when="has(prev.run.ending) && prev.run.ending == 'retire'"}: The old hall sleeps, but the town remembers you.
@june{code="0030"}: The tide is out, and Quayside is already awake.
@narrator{code="0010" when="user.endings > 0"}: Bell answers given so far: {{user.endings}}.
@narrator{code="0020"}: A hand-painted sign points toward the square, the pier, and the bakery.
```

방문 비트는 선택지를 보여 주고 결말을 기록할 수 있습니다. 결말을 정하는 선택지는
대답 횟수도 user tier에 셉니다.

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
@june{code="0010"}: Every stall has something to mend, trade, or remember.
::set{run.visits += 1}

<branch id="closingBell" prompt="The harbor bell rings. What will you do with the old town hall?">
  <choice id="restore" text="Restore it as a community hall">
    ::set{run.ending = 'restore'}
    ::set{user.endings += 1}
    @june{code="0020"}: We will fill it with lanterns and neighbors.
  </choice>
  <choice id="wait" text="Wait for the bell to ring again">
    @june{code="0030"}: We can take one more evening before deciding.
  </choice>
  <choice id="retire" text="Let the building rest with the tide">
    ::set{run.ending = 'retire'}
    ::set{user.endings += 1}
    @june{code="0040"}: Some doors close gently, and that is a kind of care.
  </choice>
</branch>

<match subject="run.ending">
  <when is="restore">
    @narrator{code="0010"}: The square keeps its lights on for the hall.
  </when>
  <when is="retire">
    @narrator{code="0020"}: The square grows quiet around the old hall.
  </when>
  <when is="none">
    @narrator{code="0030"}: The bell waits for your answer.
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
@narrator{code="0010"}: At evening tide, the town lights come on one by one.
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
  @june{code="0010"}: Fresh bread, repaired nets, and one more story before sunset.
</entry>

<entry id="pierBark" category="bark" on="visit" target="place.pier" priority="-1">
  @tom{code="0010"}: Mind the loose board. The pier remembers every storm.
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
선택합니다. `flow.newGame`은 새 게임이 런을 시작할 때마다 `start`를 발생시키므로
호스트가 직접 발생시키지 않습니다.

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
flow:
  newGame: { occasion: start }
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

## 7. 두 번째 회차

결말은 런을 끝낼 뿐 게임을 끝내지 않습니다. **새 런**(`newRun`)은 모든 `run.*`
경로를 기본값으로 되돌리므로 `run.ending`은 다시 `none`이 되고 `terminal`도 더
이상 성립하지 않습니다. 반면 **user tier**는 이어집니다. `user.*` 값, user tier
사실, `once: user` 소모, `entry.<id>.everRead`(런마다의 `entry.<id>.read`는
초기화됩니다)가 그렇습니다. Quayside는 user tier
카운터 `user.endings` 하나를 두고(1단계에서 선언), `market`의 두 결말 선택지가
여기에 1을 더합니다(`::set{user.endings += 1}`).

`newRun`은 끝난 런도 스냅숏으로 남깁니다. 모든 `run.<path>`는
[`prev.run.<path>`](/ko/state/state-model/#이전-런)로 읽을 수 있고, 그 값은
런이 끝났을 때의 값입니다. 따로 선언할 것은 없고 콘텐츠가 쓸 수도 없으며, 런이
한 번 끝나기 전까지는 `unset`이므로 읽을 때마다 `has(…)` 가드가 필요합니다.
2단계의 opening은 두 tier를 모두 씁니다. June의 처음 두 줄은
`prev.run.ending`에, narrator 줄은 `user.endings`에 게이트가 걸려 있고
narrator 줄은 그 값을 출력합니다. 첫 런에서는 셋 다 나오지 않습니다.

런마다가 아니라 플레이어마다 한 번만 나와야 하는 비트(`once: user`)나
entry(`once="user"`)는 새 런에서도 소모된 채로 남습니다.
[엔트리 비트](/ko/tooling/play/#엔트리-비트)를 참고하세요. Quayside에는 필요하지
않습니다.

플레이 스크립트는 `newRun` 스텝으로 두 번째 런을 시작한 뒤 `start`를 다시
발생시킵니다.

```yaml check="docs/examples/quayside/plays/second-run.play.yaml"
# Finish a run, start a new one, and hear Quayside remember the first.
steps:
  - occasion: start
    expect: { winner: opening }
  - occasion: visit
    target: place.square
    choose: { closingBell: restore }
    expect: { winner: market, state: { run.ending: restore, user.endings: 1 } }
  - newRun: true
    expect: { state: { prev.run.ending: restore, run.ending: none, user.endings: 1 } }
  - occasion: start
    expect: { winner: opening }
expect:
  end: complete
  state: { prev.run.ending: restore, user.endings: 1 }
  transcriptContains:
    - "@june: The hall's lanterns burned all night. Welcome back."
    - "@narrator: Bell answers given so far: 1."
```

`lute play --script plays/second-run.play.yaml .`은 새 런이 이전 런에서 가져온
값과 이번에 opening이 하는 말을 보여 줍니다(출력의 3–4단계).

```text
── step 3 · new run ──────────────
  run.* state, run-tier facts and once: run reset; prev.run.* holds the ended run (4 values)
  prev.run.day = 1
  prev.run.ending = "restore"
  prev.run.slot = "morning"
  prev.run.visits = 1
── step 4 · start ──────────────
  ✓ opening [scene, priority 10]
  → opening
  match -> arm 1
::bg{location="quayside" time="morning"}
@june: The hall's lanterns burned all night. Welcome back.
  skip @june "The old hall sleeps, but the town remembers you." — when: false
@june: The tide is out, and Quayside is already awake.
@narrator: Bell answers given so far: 1.
@narrator: A hand-painted sign points toward the square, the pier, and the bakery.
── end: complete (4 steps) ──────────────
```

`lute test docs/examples/quayside`가 다른 플레이와 함께 실행합니다.

player-web에서는 5단계의 발생 외에 호스트 코드 없이 두 번째 런이 일어나고, 새로
고침 뒤에도 이어집니다. 플레이어가 호스트 지도에서 광장으로 걸어가면 호스트가
`place.square`에 대해 `visit`을 발생시킵니다
(`player.send({ type: "raiseOccasion", occasion: "visit", target: "place.square" })`).
`newRun`, lineage, `start` 발생(`flow.newGame`, 4단계)은 플레이어 안에서
일어납니다.

1. **첫 런.** 새 게임이 `start`를 발생시킵니다. 광장으로 걸어가 "Restore it as
   a community hall"을 고릅니다. `terminal`이 성립하고 플레이어는 `restore` 결말
   화면("The Hall Opens")을 보여 줍니다.
2. **lineage 기록.** 세션이 그 `ended` 출력을 제시하면 플레이어는 런타임
   스냅숏 — user tier와 끝난 런의 값을 담은 **lineage** — 을 떠서 프로필에 바로
   저장하고 이전 것을 대체합니다.
3. **새 게임.** 타이틀로 돌아간 뒤(끝난 런에서는 확인 창이 없습니다) 새 게임을
   누릅니다. 세션이 `ended`에 있으므로 새 게임은 `newRun`을 보낸 뒤 `start`를
   발생시키고, opening이 `prev.run` 줄과 "Bell answers given so far: 1."로
   맞이합니다.
4. **새로 고침 뒤 새 게임.** 새 세션은 손대지 않은 상태라 `newRun`을 보낼 수
   없으므로, 새 게임은 먼저 프로필의 lineage를 복원하고 `newRun`을 보낸 뒤
   `start`를 발생시킵니다. 같은 두 줄이 나옵니다. 진행 중인 런은 lineage에
   들어가지 않으며 저장(퀵, 자동, 슬롯)으로만 남습니다.
5. **런을 포기한 뒤 새 게임.** 메뉴에서 런을 포기하고 새 게임을 시작해도
   같습니다. lineage를 복원한 뒤 `newRun`을 보내므로, user tier를 포함해 그 런이
   바꾼 것은 모두 버려집니다.

`snapshot`이나 `session`으로 마운트한 호스트는 상태를 스스로 고른 것입니다.
결말은 여전히 lineage를 쓰지만 새 게임은 그것을 복원하지 않습니다. lineage가
있는 동안 새 게임은 호스트 `seed`를 쓰지 않습니다. 전체 규칙은
[커스텀 호스트: 회차 간 모델](/ko/player/custom-hosts/#두-번째-회차-회차-간-모델-047)에
있습니다.

## 8. Quayside 번역하기

매니페스트에 원본 언어를 적습니다. 컴파일된 프로젝트 index는 그것을
`contentLang`으로 가지고, 플레이어는 원본 locale로 사용합니다.

```yaml check="docs/examples/quayside/lute.project.yaml"
pluginsDir: plugins/
defaultProfile: game
profiles:
  game:
    plugins: { quayside.occasions: true }
defaults:
  contentLang: en
  uses: [world.schema.yaml]
```

왕복은 `docs/examples/quayside`에서 실행하는 명령 네 개입니다.

```console
$ lute tag .
$ lute loc export . -o locales/ko.json
$ lute loc import locales/ko.json -o locales/bundle.json
$ lute compile --all --project . -o bundle/ --locales locales/bundle.json
```

**태그.** 줄의 `lineId`는 `code`로 만들어집니다. `code`가 없는 줄은
`lineId: null`로 내보내지고 `loc import`가 건너뛰므로 `lute tag .`을 먼저
실행합니다. 2단계의 `code="0010"`, `code="0020"`… 속성은 이 명령이 화자별로
문서 순서대로 써 넣은 것이며, 이미 있는 code는 건드리지 않습니다. 번역이 생긴
뒤에는 code를 바꾸지 마세요.

**내보내기.** `lute loc export`는 줄, 선택지 라벨, 프롬프트마다 한 행을 문서
순서대로 씁니다. `closingBell` 프롬프트도 자기 행을 가집니다. `kind: "prompt"`,
key `closingBell.prompt`, lineId `market.closingBell.prompt`이며 세 선택지
행(`market.closingBell.restore`, …) 바로 앞에 옵니다. 모든 행에는
`sourceHash`가 있습니다. 레코드의 plain 원본 텍스트에 대한 SHA-256입니다.
`--format csv`도 같은 열을 쓰고 `sourceHash`가 마지막 열입니다.

**번역.** 파일 이름(확장자 제외)이 locale 태그이므로 `locales/ko.json`은 `ko`
번역입니다. 각 `text`만 바꾸고 나머지는 그대로 둡니다. `lineId`와
`sourceHash`를 유지하고 `{{user.endings}}` 같은 placeholder와 인라인 수식자도
유지하세요. 커밋된
[`locales/ko.json`](https://github.com/journeyWorker/lute/tree/main/docs/examples/quayside/locales/ko.json)의
두 행입니다(해시는 줄였습니다).

```json
  {
    "code": "0010",
    "file": "./scenes/market.lute",
    "kind": "line",
    "line": 14,
    "lineId": "market.june_0010",
    "source": null,
    "sourceHash": "sha256:058c16387037…",
    "speaker": "june",
    "text": "가게마다 고치거나, 바꾸거나, 기억할 것이 있어."
  },
  {
    "file": "./scenes/market.lute",
    "key": "closingBell.prompt",
    "kind": "prompt",
    "line": 17,
    "lineId": "market.closingBell.prompt",
    "source": null,
    "sourceHash": "sha256:bbea0ecc733d…",
    "text": "항구의 종이 울립니다. 옛 마을 회관을 어떻게 할까요?"
  },
```

**가져오기.** `lute loc import`는 번역한 행을 locale 번들(`schemaVersion: 2`)로
바꿉니다. lineId와 locale별 텍스트, 그리고 그 옆에 각 번역의 바탕이 된
`sourceHash`가 들어갑니다. 같은 행을 다시 가져오면 같은 바이트가 나옵니다.
[`locales/bundle.json`](https://github.com/journeyWorker/lute/tree/main/docs/examples/quayside/locales/bundle.json)의
일부입니다(항목 하나, 압축 표기, 해시 줄임).

```json
{
  "schemaVersion": 2,
  "locales": ["ko"],
  "entries": {
    "market.closingBell.prompt": { "ko": "항구의 종이 울립니다. 옛 마을 회관을 어떻게 할까요?" }
  },
  "sourceHashes": {
    "market.closingBell.prompt": { "ko": "sha256:bbea0ecc733d…" }
  }
}
```

**컴파일.** `compile --all --locales`는 번들을 모든 artifact에 병합합니다. 줄과
선택지 라벨에는 `texts` 맵이 생기고, `closingBell` branch에는 원본 `prompt`와
`promptLineId` 옆에 `promptTexts`가 생깁니다. 원본 텍스트는 바뀌지 않습니다.
번들이 선언한 locale의 번역이 없는 레코드는 lineId와 locale마다 하나씩
`W-L10N-MISSING`이 되고 원본 텍스트를 유지합니다. Quayside는 모든 행을
번역했으므로 경고 없이 컴파일됩니다. 일부만 번역했다면 아직 번역하지 않은 쌍마다
경고가 나옵니다. 출시할 때는 번역을 완성하고, CI에서는 `--deny W-L10N-MISSING`으로
빈 곳을 거절하세요.

**원본이 바뀌면.** 내보낸 뒤 June의 첫 market 줄을 고치고(예: "Every stall has
something to mend or trade.") 다시 컴파일합니다. 한국어 텍스트는 여전히
병합되지만 저장된 해시가 새 원본 텍스트와 맞지 않습니다.

<!-- lute-diagnostics -->
```text
./scenes/market.lute:1:1: warning [W-L10N-STALE] `ko` text for `market.june_0010` was translated from an older source text; re-export with `lute loc export` and update the translation
```

다시 내보내고 그 행의 `text`를 고친 뒤(새 행에는 새 `sourceHash`가 있습니다)
다시 가져옵니다. `--deny W-L10N-STALE`이나 `--deny-warnings`를 쓰면 번역이 낡은
동안 컴파일이 거절됩니다. locale 번들은 `lute check-project`와 `lute test`를
바꾸지 않습니다. 플레이는 원본 텍스트로 실행됩니다.

**플레이어에서.** `--locales`로 컴파일한 `bundle/`을 5단계의 `bundleUrl`로
제공합니다. Settings 화면의 Locale 행은 원본 locale — `contentLang`에서 온
`en` — 을 먼저, 그다음 컴파일된 locale(여기서는 `ko`)을 보여 줍니다. 새
프로필은 `en`으로 시작합니다. 런 도중 `ko`를 고르면 다음 줄, 선택지 라벨,
`closingBell` 프롬프트, 백로그가 새 게임 없이 한국어로 바뀝니다. 화자 이름은
현지화하지 않습니다. locale은 마운트 옵션이 아니라 설정(`settings.locale`)입니다.
[커스텀 호스트: Locale](/ko/player/custom-hosts/#locale-047)을 참고하세요.

다음으로 [시계](/ko/language/clock/), [스토리 플레이](/ko/tooling/play/),
[상태 모델](/ko/state/state-model/), [`lute loc` 명령](/ko/tooling/cli/#loc-export),
[런타임 계약](/ko/tooling/runtime-contract/)을 읽어 이 튜토리얼의 언어와 호스트
표면을 더 알아보세요.
