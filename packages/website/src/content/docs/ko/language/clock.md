---
title: 시계
description: "선언된 일일 시계(dsl 0.24.0 §1) — 엔진이 소유한 경로(day, 선택적 slot 및 slots, 하나의 계기 또는 slot / dayStart / dayEnd 맵으로서의 raise, week) 위의 스키마의 clock: —, 하루 단위 시계, E-CLOCK-DECL이 거부하는 항목, 읽기 전용 clock.index / clock.weekday / clock.weekdayLabel, once: day, once: week(dsl 0.27.0) 및 once: slot 비트와 엔트리, lute play의 advance: 및 include: 스텝으로 시간을 이동하는 방법, lute calendar 시계 축 — 그리고 그와 함께 출시된 세 가지 기능인 enum 멤버 표시 라벨, 정수 %, 가드된 쓰기 ::set{… when=…}를 설명합니다."
---

많은 게임은 시간을 하루와 하루의 일부로 유지합니다. 아침, 오후, 밤, 그리고 다음 아침입니다. 비주얼 노벨에서는 슬롯마다 한 장소를 방문하고, 생활 시뮬레이션에서는 토요일에 시장이 열리며, 상점 주인은 하루에 한 번 인사하고 나머지 시간에는 더 짧게 중얼거립니다.

DSL 0.24.0 전에는 그 시간을 두 개의 [엔진 소유](/ko/state/state-model/) 경로인 `run.day`와 `run.slot` 같은 상태에 보관할 수 있었습니다. 검사기는 서로 관계없는 두 값으로 보았습니다. 오후가 아침 다음인지, 6일이 토요일인지, 또는 "하루에 한 번"이 비트가 요청할 수 있는 것인지 알 수 없었습니다. **시계(clock)**가 이를 선언합니다(dsl 0.24.0 §1). 시계는 같은 두 경로 위에 선언되므로 저장 공간을 추가하는 것이 아니라 의미를 추가합니다. 즉 슬롯의 순서, 요일, 시간에서 단조롭게 증가하는 위치, 비트의 소진 규칙을 더합니다.

## 시계 선언하기

스키마는 자신이 읽는 상태 옆에 시계를 최대 하나 선언합니다:

```yaml
# world.schema.yaml
state:
  run.day:  { type: int, default: 1, owner: engine }
  run.slot: { type: { enum: [morning, afternoon, night] }, default: morning, owner: engine }
entities:
  person: { members: [wren, hale] }
clock:
  day: run.day
  slot: run.slot
  slots: [morning, afternoon, night]
  raise: slotStart
  week:
    length: 7
    first: 0
    labels: [Mon, Tue, Wed, Thu, Fri, Sat, Sun]
```

| 키 | 의미 |
|---|---|
| `day` | 필수입니다. `owner: engine`으로 선언한 `int` 상태 경로입니다. day 1이 첫날입니다. |
| `slot` | 선택 사항입니다. `owner: engine`으로 선언한 enum 상태 경로이며, 인라인(`{ enum: [...] }`) 또는 이름 있는 도메인(`{ domain: slot }`)입니다. `slot`과 `slots`는 함께 쓰거나 둘 다 쓰지 않습니다. |
| `slots` | `slot`과 함께 씁니다. 슬롯 enum의 멤버를 하루가 진행되는 순서대로 각각 정확히 한 번씩 적습니다. enum 자체의 멤버 순서는 중요하지 않으며, 이 목록이 시계의 순서입니다. |
| `raise` | 선택 사항입니다. 시계가 이동할 때마다 시계가 멈추는 곳에서 엔진이 발생시키는 계기입니다. 비트가 "새 슬롯이 시작되었다"에 응답할 수 있게 합니다. 또는 각 순간의 계기를 지정하는 맵일 수 있으며, 각 키는 선택 사항입니다: `{ slot, dayStart, dayEnd }`([시간 이동](#시간-이동) 참고). |
| `raiseAtStart` | 선택 사항(dsl 0.28.0)이며 기본값은 `false`입니다. `true`이면 런이 시작하는 위치에서 엔진이 `slot` 계기와 `dayStart` 계기를 직접 발생시킨다고 뜻합니다. 그 위치에서는 어떤 이동도 멈추지 않습니다([런이 시작하는 위치](#런이-시작하는-위치) 참고). |
| `week` | 선택 사항입니다. `length`(`week` 안에서 필수이며 최소 1), `first`(day 1의 요일 인덱스, 0부터 시작하며 기본값 `0`), `labels`(요일 순서대로 요일마다 하나의 표시 텍스트)를 지정합니다. |
| `last` / `days` | 선택 사항(dsl 0.27.0)이며 시계가 끝나는 곳입니다. `last: { day: 1, slot: h05 }`는 마지막 위치를 지정합니다(`slot` 생략 시 그날의 마지막 슬롯이며, 슬롯이 없는 시계에서는 `day`만 지정합니다). `days: N`은 `last: { day: N }`의 축약형입니다. 둘 중 하나만 선언합니다. [끝나는 시계](#끝나는-시계)를 참고하세요. |

`slot`과 `slots`가 없는 시계는 하루 전체를 셉니다. 각 날은 하나의 슬롯이고, 위치는 그 날 자체(`day 3`)이며, `clock.index`는 `day - 1`이고, `once: slot`은 `once: day`처럼 소진됩니다. 날짜만 세는 게임에는 시계를 만들기 위해 멤버 하나짜리 슬롯 enum이 더 이상 필요하지 않습니다.

시계는 스키마, `uses:`로 도달한 파일, 또는 매니페스트의 `defaults:` 안에서만 존재합니다. 씬 자체의 프런트매터에는 시계를 선언할 수 없습니다. 그곳의 `clock:`은 `E-META-UNKNOWN-KEY`입니다. 이 페이지의 예제는 플러그인이 두 계기를 선언하는 프로젝트를 사용합니다:

```yaml
occasions:
  slotStart: { select: sequence }
  talk:      { select: first, target: { prefix: npc, entity: person } }
```

두 경로 모두 여전히 엔진이 소유합니다. 콘텐츠는 이전처럼 `run.day`와 `run.slot`을 읽지만 절대 쓰지 않습니다(`E-ENGINE-OWNED-WRITE`). 선언은 모든 컴파일된 아티팩트와 `project.index.json`에 `clock`으로 엔진까지 전달되며, 프로젝트에 시계가 없으면 생략됩니다.

### `E-CLOCK-DECL`이 거부하는 항목

시계에서 잘못된 것은 모두 `E-CLOCK-DECL`입니다:

- 잘못된 선언: `day` 누락, 알 수 없는 키, 계기 이름도 `{ slot, dayStart, dayEnd }` 맵도 아닌 `raise`, `slot`만 있거나 `slots`만 있는 경우, 비어 있거나 반복된 `slots` 목록, `week.length`가 0인 경우, `week.first`가 `0..length-1` 밖인 경우, 또는 길이가 `week.length`가 아닌 `week.labels` 목록(`week.labels`는 요일마다 라벨 하나를 가지는 목록이며 enum이나 엔티티 종류의 `labels:`는 맵입니다);
- 선언되지 않았거나 올바른 타입이 아닌(`day`는 `int`, `slot`은 enum) 경로, 같은 경로를 두 번 쓴 경우, 또는 `owner: engine`이 아닌 `day`나 `slot` 경로; 기본값이 1보다 작은 `day` 경로(시계는 day 1부터 셉니다);
- 슬롯 enum의 멤버와 정확히 일치하지 않는 `slots`;
- 어떤 플러그인도 선언하지 않은 `raise` 계기 또는 `raise` 맵의 계기(did-you-mean이 함께 표시됨). 프로젝트의 플러그인이 계기를 하나도 선언하지 않은 동안에는 `on:`과 마찬가지로 어떤 이름이든 통과합니다;
- `payload:`를 선언한 `raise` 계기. 이동에는 넘겨 줄 payload가 없으므로, 대신 엔진에서 해당 계기를 발생시켜야 합니다;
- 위치를 지정하지 않는 `last` / `days`: 두 키를 동시에 사용한 경우, `days: 0`, 1보다 작은 `last.day`, `slots`에 없는 `last.slot`(또는 슬롯이 없는 시계에서의 모든 `last.slot`), 또는 `day` / `slot`의 기본값이 이미 마지막 위치를 지난 시계(dsl 0.27.0 §4);
- 두 번째 시계: 한 프로젝트의 두 스키마가 모두 `clock:`을 선언하는 경우.

두 번째 시계를 제외하면(두 번째 시계만 프로젝트에서 알 수 있습니다) 각각은 스키마의 `clock:` 키가 있는 줄에 보고됩니다. `lute check world.schema.yaml`만 실행해도 직접 찾을 수 있습니다(스키마가 속한 프로젝트의 플러그인을 기준으로 `raise` 계기를 검사하며, 프로젝트 밖에서는 어떤 계기 이름이든 통과합니다). 이 스키마를 사용하는 문서는 단일 파일 `lute check`에서 같은 오류를 1번 줄에 전달하며, 스키마 이름과 그 아래 스키마의 줄을 함께 표시합니다. 프로젝트 전체 `lute check`는 대신 프로젝트 전체 오류로 한 번 보고하고, 스키마 줄에 `(imported by N documents)`를 덧붙이며, 가져온 문서들은 `ok`로 둡니다. `owner: engine` 없이 `run.day` 위에 선언한 시계는 다음과 같습니다:

<!-- lute-diagnostics unverified="verbatim lute check output, but composed at runtime: crates/lute-check/src/clock.rs wraps each clock problem as `clock:` {what}, a prefix literal below the admission floor, so no single format! literal spans it" -->
```
world.schema.yaml:4:1: error [E-CLOCK-DECL] `clock:` `day: run.day` must be declared `owner: engine` — only the engine moves the clock
```

## 시계 읽기

선언된 시계는 예약된 읽기 전용 경로를 추가합니다. 콘텐츠는 CEL 조건이나 보간이 허용되는 곳이면 어디서든 이를 읽습니다. 비트의 `when`, 줄의 `when=`, `<match>` 주체, 퀘스트 기한, `{{…}}` 등이 해당합니다.

| 경로 | 타입 | 값 |
|---|---|---|
| `clock.day` | int | day 경로의 값(위의 `run.day`) |
| `clock.slot` | `slots`의 enum | slot 경로의 값. 슬롯이 있는 시계에서만 존재합니다. |
| `clock.index` | int | `(day - 1) * len(slots) + ` `slots`에서 슬롯의 위치(0부터 시작); 슬롯이 없는 시계에서는 `day - 1`입니다. 이 값은 증가하기만 합니다. day 1 morning은 0, day 1 night는 2, day 2 morning은 3입니다. |
| `clock.weekday` | int `0..length-1` | `(week.first + day - 1) mod week.length`. `week:`가 있을 때만 존재합니다. |
| `clock.weekdayLabel` | `week.labels`의 enum | `week.labels[clock.weekday]`이며 `{{…}}`에서 렌더링할 수 있습니다. `week.labels`가 있을 때만 존재합니다. |
| `clock.ended` | bool | 유한한 시계가 끝나면 `true`입니다([끝나는 시계](#끝나는-시계) 참고). `last:` / `days:`가 있을 때만 존재합니다. |

엔진은 현재 `day`와 `slot`에서 이 값들을 파생합니다. 이 값들은 상태 행이 아니며 콘텐츠는 절대 쓰지 않습니다. `::set{clock.index = 3}`은 `E-QUEST-RESERVED-WRITE`이고, day나 slot 경로 자체에 대한 `::set`은 `E-ENGINE-OWNED-WRITE`입니다. 두 메시지 모두 `advance:` 스텝으로 시계를 이동하라고 안내합니다. 시계가 없으면 `clock.*`은 단순히 선언되지 않은 상태(`E-UNDECLARED`)이며, 슬롯이 없는 시계의 `clock.slot`, `week:`가 없는 시계의 `clock.weekday`, `week.labels`가 없는 시계의 `clock.weekdayLabel`, 끝나지 않는 시계의 `clock.ended`도 마찬가지입니다.

`day`라는 세 단어는 서로 다른 것을 뜻합니다. 선언의 `day:`는 날짜를 세는 상태 경로를 지정하고, `clock.day`는 그 경로의 값을 읽으며, `days: N`(또는 `last.day: N`)은 끝나는 시계의 마지막 날입니다.

위 시계에서 day 1은 월요일이므로 `clock.weekday == 5`는 매주 토요일을 뜻합니다:

```lute unverified="reads clock.* from the clock declared in world.schema.yaml, which a standalone check cannot reach and no example project in the repo declares; checked by hand as scenes/market.lute in a scratch project with that schema"
---
kind: scene
id: square.market
on: slotStart
when: "clock.weekday == 5 && run.slot == 'morning'"
once: false
---

## Opening {#opening}

@narrator: {{clock.weekdayLabel}}, day {{run.day}}. The market is up before the bells.
```

`clock.index`는 날짜를 가로질러 시간을 정렬하는 유일한 숫자이며, 이것이 기한이 원하는 값입니다. `clock.index >= 20`은 그날 어떤 슬롯이 먼저 오든 day 7 night부터 성립합니다. `run.day`와 `run.slot`만으로는 이 조건을 슬롯마다 일일이 적어야 합니다.

두 요일 경로는 타입이 엄격하므로 검사기는 폐쇄형 도메인처럼 다룹니다. `is="0..4"`와 `is="5..6"` 분기가 있는 `<match subject="clock.weekday">`는 `<otherwise>` 없이도 모든 경우를 다루며, 일요일을 빼면 간극을 명시하는 `E-NONEXHAUSTIVE`가 됩니다(`` `6` is not
covered ``). `is="7"`은 `E-WHEN-LITERAL-DOMAIN`이고, `clock.weekday == 7` 가드는 데드 코드입니다(줄에서 `E-ARM-DEAD`). 라벨은 `clock.weekdayLabel`의 멤버이므로 오타인 `is="Sundy"` 역시 `E-WHEN-LITERAL-DOMAIN`이며, dsl 0.26.0부터는 가드의 `== 'Sundy'`도 마찬가지입니다(가장 가까운 라벨을 표시: `` did you mean `'Sun'`? ``):

```lute
<match subject="clock.weekday">
  <when is="0..4">
    @narrator: Another working day.
  </when>
  <when is="5..6">
    @narrator: The weekend. The square is loud.
  </when>
</match>
```

컴포넌트 본문은 자신의 파라미터만 읽으므로, 요일을 보여 주는 컴포넌트는 이를 파라미터로 받아야 합니다. 라벨을 enum에 복사하는 대신 파라미터를 `{ domain: clock.weekdayLabel }`(또는 슬롯에는 `{ domain: clock.slot }`)으로 타입 지정합니다. 호스트는 실시간 값(`defs:`에 `today: "clock.weekdayLabel"`를 둔 `day=@today`) 또는 리터럴 라벨을 전달하며, 검사기는 컴포넌트의 `<match>` 분기와 모든 리터럴 인자를 시계 자체의 라벨에 대조합니다. 오타인 `<when is="Wednesdy">`는 컴포넌트 위치에서 가장 가까운 라벨을 명시하는 `E-WHEN-LITERAL-DOMAIN` 하나가 됩니다.

## 시계를 따라가는 배경

시계의 소유권은 엔진에 있지만, 프레젠테이션 컴포넌트는 슬롯마다 정적인
배경 하나를 선택할 수 있습니다. 스키마에 슬롯 어휘와 슬롯 타입 상태 필드를
선언하고, 그 필드를 읽는 def를 노출합니다:

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

컴포넌트의 각 arm에 `::bg`를 하나씩 둡니다. 컴포넌트의 `slot` 파라미터도
같은 폐쇄형 도메인이므로 `<match>`가 모든 경우를 다루며, 디렉티브 값은
컴파일 시점 상수로 유지됩니다:

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

이 정책이 필요한 모든 씬에서 컴포넌트를 사용합니다:


대안은 각 씬의 `::bg`에 `time="morning"`(또는 다른 리터럴)을 직접 쓰는
것입니다. 씬이 시계를 따르지 않고 의도적으로 시간을 고를 때 더 명확할 수
있습니다. 상태에서 파생된 `@def`를 디렉티브 필드에 넣지 마세요. 디렉티브
필드는 빌드 시점 값이므로 검사기는 `E-ATTR-DEF-DYNAMIC`을 보고합니다.
컴포넌트 본문은 주변 상태도 읽을 수 없으므로 파라미터로 값을 전달해야 하며,
그렇지 않으면 `E-COMPONENT-STATE`가 보고됩니다.
## 하루에 한 번, 일주일에 한 번, 슬롯에 한 번

시계가 있는 프로젝트에서는 비트의 `once`([비트](/ko/language/beats/) 참고)에 세 가지 값이 추가됩니다:

| `once` | 소진됨 | 다시 자격을 얻는 시점 |
|---|---|---|
| `day` | 제시된 때부터 시계의 날짜가 바뀔 때까지 | 다음 날 |
| `week` | 제시된 때부터 다음 시계 주가 시작될 때까지(dsl 0.27.0 §5); 시계에 `week:`가 필요합니다 | `clock.weekday`가 `week.first`로 돌아올 때 |
| `slot` | 제시된 때부터 시계의 슬롯이 바뀔 때까지(새 슬롯 또는 다른 날의 같은 슬롯) | 다음 슬롯 |

씬 비트는 프런트매터에(`once: day`), 번들 비트는 속성으로(`<beat … once="day">`), 엔트리는 `once="day"`, `once="week"`, 또는 `once="slot"`으로 씁니다. 엔진은 각 비트가 마지막으로 제시된 시간상의 위치를 보관합니다. 엔트리에서는 읽기 플래그가 아니라 제시 기록도 함께 보관합니다. 플래그는 여전히 엔트리의 효과를 적용할지 결정합니다([로어 엔트리](/ko/language/lore-entries/) 참고).

하루에 한 번 인사하고, 그 슬롯의 나머지 시간에는 짧게 말하는 빵집 주인입니다:

```lute unverified="once: day needs the project's clock (world.schema.yaml above); checked by hand as scenes/wren.lute in a scratch project"
---
kind: scene
id: wren.greeting
on: talk
target: npc.wren
once: day
---

## Opening {#opening}

@wren: Morning! It's {{clock.weekdayLabel}}, so the rye is fresh.
```

```lute unverified="an entry once of slot needs the project's clock (world.schema.yaml above); checked by hand as lore/barks.lute in a scratch project"
---
kind: lore
title: Bakery barks
---

<entry id="wrenBusy" on="talk" target="npc.wren" category="bark" priority="-1" once="slot">
  @wren: Busy, busy. Come back later.
</entry>
```

[`lute play`](/ko/tooling/play/)는 소진된 비트를 건너뛴 이유를 이름으로 표시합니다. 월요일 아침에 Wren에게 세 번 말을 걸고, 화요일에 한 번 말을 거는 경우입니다:

```
── step 2 · talk → npc.wren ──────────────
  ✓ wrenBusy [entry, priority -1]
  ✗ wren.greeting [scene, priority 0] — once: day — already presented today
  → wrenBusy
  entry wrenBusy (first read)
@wren: Busy, busy. Come back later.
── step 3 · talk → npc.wren ──────────────
  ✗ wren.greeting [scene, priority 0] — once: day — already presented today
  ✗ wrenBusy [entry, priority -1, read] — once: slot — already presented this slot
  → (no eligible beat — the occasion passes)
── step 4 · advance day: day 1 (Mon) morning → day 2 (Tue) morning ──────────────
  set run.day = 2
── step 4 · slotStart (select: sequence) ──────────────
  ✗ square.market [scene, priority 0] — when: false
  → (no eligible beat — the occasion passes)
── step 5 · talk → npc.wren ──────────────
  ✓ wren.greeting [scene, priority 0]
  ✓ wrenBusy [entry, priority -1, read]
  → wren.greeting
@wren: Morning! It's Tue, so the rye is fresh.
```

한 주는 day 1에서 시작하며, day 1은 `week.first` 요일입니다. week *n*은 `(day - 1) div week.length`가 *n*인 날짜들을 포함합니다. 따라서 시계의 주는 고정된 라벨이 아니라 항상 day 1의 요일에 시작합니다. 위 시계(`first: 0`, Monday)에서는 `once: week` 비트가 제시된 날부터 다음 월요일까지 소진되고, 그동안 `lute play`는 `once: week — already presented this week`로 이를 건너뜁니다. `first`가 `5`인 시계는 매주 토요일에 시작합니다. `week:`를 선언하지 않은 프로젝트에서 `once: week`을 쓰면, 시계가 없는 `once: day`와 마찬가지로 `E-BEAT-ATTR`입니다. 주간 편지의 예입니다:

```lute unverified="once: week needs the project's clock with its week: (world.schema.yaml above); checked by hand as scenes/letters.lute in a scratch project"
---
kind: scene
id: hale.letter
on: talk
target: npc.hale
once: week
---

## Opening {#opening}

@hale: Your weekly letter. Same handwriting as last {{clock.weekdayLabel}}.
```

시계가 없으면 `day`와 `slot`은 아무 의미가 없으므로, 시계를 선언하지 않은 프로젝트는 이를 거부합니다:

```lute expect="E-BEAT-ATTR"
---
kind: scene
id: square.bell
on: townVisit
once: day
---

## Opening {#opening}

@narrator: The bell rings.
```

메시지는 무엇을 해야 하는지도 말합니다. 스키마에 `clock:`을 선언하거나 `run`, `user`, `false`를 사용하세요. `once: week` 역시 시계의 `week:`가 필요합니다.

## 시간 이동

시계는 엔진만, 앞으로만 이동시킵니다. 이동할 때마다 엔진은 먼저 퀘스트를 정착시킵니다(새 시간이 통과한 [`by=` 기한](/ko/language/quests-and-scenes/)은 그때 실패합니다). 그다음 시계에 `raise`가 선언되어 있으면 일반 계기로 발생시킵니다. 두 경로는 다른 상태와 마찬가지로 계층에 따라 초기화되므로 `run.day`와 `run.slot` 위의 시계는 새 런마다 day 1 morning에서 다시 시작합니다.

`raise: slotStart` 하나만 있으면 이동할 때마다 시계가 멈추는 곳에서 한 번 발생하고, 지나가는 슬롯에서는 발생하지 않습니다. `lute play`는 이동이 그 사이에 지나친 슬롯을 표시합니다. 하지만 퀘스트는 이동이 지나가는 모든 슬롯에서 정착됩니다. [시즌](/ko/state/schemas/#시즌)이 이동 중에 열리고 닫히면 그 지점에서 퀘스트를 시작하고 기한을 실패시키며, [`rearm=`](/ko/language/quests-and-scenes/#돌아오는-퀘스트-시즌-tier와-rearm)은 조건이 참이 되는 날에 발동합니다. 따라서 한 번에 긴 이동을 해도 슬롯마다 이동한 것과 같은 퀘스트 상태가 됩니다. 하루의 끝이나 시작에 자체 스토리가 필요하면 `raise`를 맵으로 바꾸며, 각 키는 선택 사항입니다:

```yaml
clock:
  # day, slot, slots, week as above
  raise: { slot: slotStart, dayStart: dayStart, dayEnd: dayEnd }
```

| 키 | 발생 시점 |
|---|---|
| `slot` | 이동할 때마다 시계가 멈추는 곳에서 한 번. `raise: slotStart`는 `raise: { slot: slotStart }`의 축약형입니다. |
| `dayEnd` | 이동이 통과하는 각 자정에서, 통과하기 전: 그날의 마지막 슬롯에서 날짜가 아직 이동하지 않은 상태로 발생합니다. |
| `dayStart` | 이동이 통과하는 각 자정에서, 통과한 후: 다음 날의 첫 슬롯에서 발생합니다. |

각 자정은 그 자체로 하나의 정지 지점입니다. 시계가 그곳으로 이동하고, 퀘스트가 정착된 뒤 계기가 발생하므로 `dayEnd` 비트는 이전 날짜를 읽습니다. `lute play`의 `engine:` 스텝처럼 시계 경로를 직접 쓰면 세 계기 중 어느 것도 발생하지 않습니다.

### 런이 시작하는 위치

런은 어떤 이동도 멈추지 않은 위치에서 시작하므로 이 규칙에 따르면 그곳에서는 아무것도 발생하지 않습니다. `slot` 계기도, 런이 시작하는 날의 `dayStart`도 발생하지 않습니다. 하지만 많은 엔진은 런이 시작할 때 이를 발생시켜 첫 아침의 씬을 재생합니다. 시계에 다음처럼 선언하세요:

```yaml
clock:
  # day, slot, slots, raise as above
  raiseAtStart: true
```

`raiseAtStart: true`이면 런의 첫 위치는 `slot` 계기와 `dayStart`에 대해 발생한 것으로 계산됩니다. `dayEnd`는 바뀌지 않습니다. 검사기와 `lute calendar`가 이를 따릅니다. `lute play`는 시작할 때 자체적으로 아무것도 발생시키지 않으므로, 스크립트는 여전히 `occasion:` 스텝으로 첫 계기를 재생해야 합니다. 이 키가 없을 때 런이 시작하는 곳에서만 `when`이 성립하는 비트는 `W-BEAT-UNRAISED`가 됩니다. 이 진단은 엔진이 그곳에서 계기를 발생시키는 경우 키를 선언하라고 알려 주며, 시작 시 `slot` 계기나 `dayStart`를 발생시키는 `occasion:` 스텝에도 같은 내용의 메모가 붙습니다.

[`lute play`](/ko/tooling/play/)는 `advance:` 스텝으로 그 엔진을 대신합니다:

- `advance: slot`은 다음 슬롯으로 이동하며, 마지막 슬롯에서 다음 날로 넘어갑니다.
- `advance: <n>`은 한 번에 `n`개 슬롯을 이동합니다. 하루의 끝을 건너뛰지는 않습니다. `dayEnd`가 선언되어 있으면 그곳에서 발생시키기 위해 이동 중 각 날의 마지막 슬롯에 멈춥니다.
- `advance: day`는 현재 어느 슬롯에 있든 다음 날의 첫 슬롯으로 이동합니다. 시계가 서 있는 곳에서 `dayEnd`를 발생시키고, 나머지 하루는 건너뛰며 그 슬롯들은 정착되지 않습니다.

슬롯이 없는 시계에서는 세 가지 모두 하루 단위로 이동합니다. 제시된 비트는 씬 프런트매터에 `advances: slot`, `advances: day`, 또는 `advances: <n>`을 선언할 수 있고, `<entry>` / `<beat>`의 속성으로도 선언할 수 있습니다. 이 선언은 메타데이터이며, 해당 비트가 제시될 때 엔진이 퀘스트 정착과 선언된 시계의 발생을 포함해 이동을 수행합니다:

```yaml
---
kind: scene
id: square.market
on: slotStart
advances: slot
---
```

이는 시계 이동에 관한 `advance:`와 같습니다. 비트 직후에 명시적인 `advance:`를 두어 이를 반복하지 마세요. `lute play`는 두 이동을 모두 허용하지만 시계가 두 번 이동했다는 설명이 있는 메모를 출력합니다. 이 선언에는 프로젝트 시계가 필요합니다. 플러그인 설정 `spendsSlot: true`는 `advances: slot`으로, `spendsSlots: n`은 `advances: n`으로 마이그레이션하세요. 시계는 플러그인이 아니라 엔진이 소유합니다.

명시적인 `advance:` 스텝에서는 시계가 시계 경로를 쓰고 정착한 뒤 `slot`을 발생시키며, 발생한 계기의 `pick:`, `choose:`, 선택 `expect:`를 `occasion:` 스텝과 정확히 같은 방식으로 처리합니다. 월요일 아침에서 15개 슬롯을 이동하면 토요일 아침입니다:

```yaml
steps:
  - advance: 15
    expect: { presented: [square.market] }
```

```
── step 1 · advance 15: day 1 (Mon) morning → day 6 (Sat) morning ──────────────
  set run.day = 6
── step 1 · slotStart (select: sequence) ──────────────
  ✓ square.market [scene, priority 0]
  → square.market
@narrator: Sat, day 6. The market is up before the bells.
  note: passed day 1 (Mon) afternoon, night; day 2 (Tue) to day 5 (Fri), every slot without raising `slotStart` (1 beat answers it; an `advance:` raises it only where the clock stops)
```

위의 `raise` 맵과 `dayEnd` 씬을 사용하면, 월요일 오후에서 두 슬롯 이동할 때 월요일 밤에 멈춰 하루의 마감을 처리한 다음 화요일 아침으로 갑니다:

```
── step 2 · advance 2: day 1 (Mon) afternoon → day 2 (Tue) morning ──────────────
  set run.slot = "night"
── step 2 · day 1 (Mon) night · dayEnd ──────────────
  ✓ inn.closing [scene, priority 0]
  → inn.closing
@narrator: The inn shutters close on Mon.
  set run.day = 2
  set run.slot = "morning"
── step 2 · day 2 (Tue) morning · dayStart (select: sequence) ──────────────
  (no candidates)
  → (no eligible beat — the occasion passes)
── step 2 · slotStart (select: sequence) ──────────────
  ✗ square.market [scene, priority 0] — when: false
  → (no eligible beat — the occasion passes)
  note: passed day 1 (Mon) night without raising `slotStart` (1 beat answers it; an `advance:` raises it only where the clock stops)
```

이런 스텝에서 선택 `expect:`는 전체 스텝을 읽습니다. `presented`에는 각 자정 발생과 최종 발생이 제시한 모든 비트를 순서대로 나열하고, `winner`, `offered`, `notOffered`는 시계가 멈추는 최종 발생을 판정합니다.

`advance:` 스텝에는 같은 순간에 속한 `engine:` 쓰기를 함께 넣을 수 있습니다. 예를 들어 `engine: { state: { run.leg: 3 } }`를 `advance: day` 옆에 둘 수 있습니다. 쓰기는 시계가 도착한 곳에 적용됩니다. 이동 중 각 `dayEnd` / `dayStart` 뒤에 발생이 일어나므로, 그날 저녁의 `dayEnd`는 여전히 닫히는 날짜를 읽고, 최종 정착과 발생 전에 마지막 쓰기가 적용됩니다. 마지막 이동과 쓰기가 함께 끝난 뒤 정착은 한 번 일어납니다. 그곳에서 시계 자신의 `day` 또는 `slot` 경로를 쓰면 사용 오류(종료 코드 2)입니다. 시계 이동은 별도의 스텝 또는 `advance:`를 통해서만 수행합니다.

`engine:` 스텝은 여전히 `run.day`나 `run.slot`을 직접 쓸 수 있지만, `clock.index`를 뒤로 이동시키는 스텝은 두 위치를 모두 명시하는 사용 오류(종료 코드 2)입니다. 시계가 없는 프로젝트의 `advance:`도 사용 오류입니다. 여러 경로에서 같은 일과가 반복되는 경우가 많으므로 스텝은 `include: <file>`일 수도 있으며, 이때 해당 파일의 스텝이 그 자리에 삽입됩니다. 파일 경로는 include하는 스크립트를 기준으로 하며, include 순환은 사용 오류입니다. [스토리 플레이](/ko/tooling/play/)에서 전체 스텝 참조와 `advance.days` 아래에 자정 발생을 나열하는 `--json` 형태를 확인하세요.

`lute calendar --axis clock=<d1>..<d2>`는 지정된 날짜의 모든 슬롯을 시계 순서로 배치하므로 격자가 시간표처럼 읽힙니다. `--axis clock`만 쓰면 day 1부터 일주일을 표시합니다(`week:`가 없으면 day 1만 표시합니다):

```console
$ lute calendar --project . --axis clock=5..6 --occasion slotStart
calendar: . — 6 cell(s) × 1 column(s), from declared defaults

clock            slotStart (sequence)
5 Fri morning    -
5 Fri afternoon  -
5 Fri night      -
6 Sat morning    square.market
6 Sat afternoon  -
6 Sat night      -
```

하루에 한 번 발생하는 계기는 모든 슬롯 행에서가 아니라 하루에 한 번 읽어야 합니다. calendar는 시계의 raise 맵을 따릅니다. 어떤 이동도 계기를 발생시키지 않는 셀은 `not raised`로 읽힙니다. 즉 하루의 첫 슬롯이 아닌 곳의 `dayStart`, 하루의 마지막 슬롯이 아닌 곳의 `dayEnd`, 런이 시작하는 위치의 `slot` 계기가 그렇습니다. `raiseAtStart: true`이면 그 위치에서 `slot` 계기와 `dayStart`가 발생합니다. 그런 셀에서만 취할 수 있는 비트는 결코 자격이 없는 것으로 나열됩니다. `--occasion-axes dayEnd=clock.day`는 하루마다 한 번씩, 시계가 이를 발생시키는 그날의 마지막 슬롯에서 `dayEnd`를 평가합니다(`dayEnd=run.day`처럼 시계 자신의 경로도 사용할 수 있음). 다른 행은 비워 둡니다. `,clock.slot=afternoon`은 오후에 `advance: day`를 수행했을 때 하루를 닫는 위치에서 읽습니다.

나머지 calendar는 [개요](/ko/tooling/overviews/)를 참고하세요.
## 끝나는 시계

일부 게임은 정해진 시간 범위를 가집니다. 폐쇄된 병원에서 보내는 하룻밤, 밤 11시부터 새벽까지, 그다음 이야기가 끝나는 식입니다. dsl 0.27.0부터 시계는 끝나는 지점을 지정할 수 있습니다:

```yaml
clock:
  day: run.night
  slot: run.hour
  slots: [h23, h00, h01, h02, h03, h04, h05]
  raise: { slot: hourStrikes, dayEnd: dawn }
  last: { day: 1, slot: h05 }      # or: days: 1
```

`last:`는 마지막 위치를 지정합니다. `slot`을 생략하면 그날의 마지막 슬롯이며, 슬롯이 없는 시계에서는 `day`만 지정합니다. `days: N`은 `last: { day: N }`의 축약형입니다.

정확히 마지막 위치에 도착하는 이동은 일반적인 이동입니다. 마지막 위치를 지나가게 되는 이동은 마지막 위치까지만 이동하며, 언제나 그렇듯 도중에 만나는 모든 자정에서 `dayEnd` / `dayStart`를 발생시킵니다. 그런 다음 마지막 날의 `dayEnd`를 한 번 발생시키고 `slot` 계기는 발생시키지 않으며, 시계가 **끝납니다**. `last:`가 없으면 `h05`에서의 이동은 night 2의 `h23`으로 넘어가 다시 시간을 알립니다. `last:`가 있으면 다음과 같습니다:

```
── step 1 · advance 6: day 1 h23 → day 1 h05 ──────────────
  set run.hour = "h05"
── step 1 · hourStrikes ──────────────
  ✓ ward.strike [scene, priority 0]
  → ward.strike
@narrator: The clock strikes at h05.
  note: passed day 1 h00, h01, h02, h03, h04 without raising `hourStrikes` (1 beat answers it; an `advance:` raises it only where the clock stops)
── step 2 · advance slot: day 1 h05 → day 1 h05 · the clock ends (its last position) ──────────────
── step 2 · day 1 h05 · dawn ──────────────
  ✓ ward.dawn [scene, priority 0]
  → ward.dawn
@narrator: Dawn breaks on night 1.
── step 3 · advance slot: day 1 h05 → day 1 h05 ──────────────
── halted: step 3: `advance:` past the clock's last position (day 1 h05) — the clock ended; a `newRun` starts it over (E-CLOCK-END) ──────────────
```

끝난 뒤의 모든 `advance:`는 `E-CLOCK-END`(종료 코드 1)이며, `engine:` 스텝이 날짜를 이동시켜 끝난 시계보다 뒤에서 시작하게 만드는 경우도 마찬가지입니다. `newRun`은 day와 slot 경로를 초기화하고 시계를 다시 시작합니다. 단, 이 경로들이 run 계층일 때입니다. day 경로가 `user.*`인 시계는 런을 넘어 위치를 유지하며, 그와 함께 `once: day` / `once: slot` / `once: week`의 소진과 끝난 상태도 유지합니다. 플레이의 새 런 스텝은 다음과 같이 이를 설명합니다: ``the clock is kept: its day `user.dive` outlives the run, so its position,
its once: day / slot / week spends and its end stay``.

검사기도 끝을 알고 있습니다. `clock.index`는 시계가 시작하는 위치부터 마지막 위치까지의 모든 정수를 범위로 가지며, day 경로와 `clock.day`는 기본 day부터 마지막 날까지를 범위로 가집니다. 같은 날에 시작하고 끝나는 시계에서는 slot 경로와 `clock.slot`이 시작 슬롯부터 마지막 슬롯까지에만 해당합니다. 더 뒤의 위치에서만 성립하는 `when`은 결코 성립할 수 없습니다:

<!-- lute-diagnostics -->
```
scenes/second.lute:6:8: error [E-BEAT-UNREACHABLE] beat `ward.second` is never eligible: its `when` `run.night == 2` is provably false — the clock ends at its last position, so `run.night` only ranges over 1..1
```

엔트리의 `when`은 `E-ENTRY-UNREACHABLE`, 줄이나 arm의 가드는 `E-ARM-DEAD`가 되며 각각 시계의 끝을 명시합니다. day 경로나 `clock.index`에 대한 `<match>`는 범위의 모든 값을 다루면 완전한 것으로 판단됩니다. 하루짜리 시계의 slot 경로에 대한 `<match>`도 시계가 도달하는 모든 슬롯을 다루면 완전합니다. 끝 이후의 슬롯을 위한 arm은 `E-ARM-DEAD`입니다. 며칠에 걸친 시계에서는 모든 슬롯이 다시 돌아오므로 slot 경로 하나만으로는 범위가 좁혀지지 않습니다. 하지만 마지막 날에는 좁혀집니다. `last: { day: 2, slot: h02 }`일 때 `run.night == 2 && run.hour == 'h04'`라는 `when`은 도달할 수 없습니다. day 2에서 `run.hour`는 `h23`, h00, h01, h02만 가질 수 있기 때문입니다. `lute calendar --axis clock`은 마지막 위치에서 멈춥니다.

검사기는 가드의 시계 조건도 함께 읽습니다. day 경로, slot 경로, `clock.index`, `clock.day`, `clock.slot`, `clock.weekday`를 읽는 조건을 한꺼번에 만족하는 시계 위치가 하나도 없으면, 끝나는 시계든 끝나지 않는 시계든 가드는 명백히 거짓입니다. night 1의 `h01`은 `clock.index`가 2이므로 다음 가드는 결코 성립할 수 없습니다:

<!-- lute-diagnostics -->
```
scenes/over.lute:7:8: error [E-BEAT-UNREACHABLE] beat `ward.over` is never eligible: its `when` `run.night == 1 && run.hour == 'h01' && clock.index > 2` is provably false — no clock position has `run.night == 1`, `run.hour == 'h01'` and `clock.index > 2`
```

퀘스트 목표의 `by=` 기한도 같은 범위에서 판정됩니다. 하룻밤 시계에서 night 2를 지정하는 기한은 결코 성립할 수 없으므로 목표를 절대 실패시키지 않으며, 검사기는 `by`에서 경고합니다:

<!-- lute-diagnostics -->
```
quests/q.lute:5:59: warning [W-DEADLINE-NEVER] objective `leave` never fails: its deadline `by: run.night >= 2` can never hold (the clock ends at its last position, so `run.night` only ranges over 1..1) — write a deadline the clock can reach, or drop it
```

기한이 너무 이른 경우도 있습니다. 목표의 `done`이 `by=`가 이미 지나간 위치에서만 성립할 수 있다면, 목표는 완료되기 전에 실패합니다. 검사기는 `done`을 시계에 직접 대조하며, 해당 비트의 `when`이 성립할 수 있는 첫 위치에서의 `visited('<beat>')`를 읽습니다. 여기에는 시계에 대한 `cel()` 가드인 규칙으로 파생된 관계를 통한 경우도 포함됩니다. 어떤 위치에 도착했을 때 기한이 성립하면 그 위치의 비트가 재생되기 전에 목표가 실패합니다. 검사기는 `by`에서 경고합니다:

<!-- lute-diagnostics -->
```
quests/q.lute:8:69: warning [W-DEADLINE-BEFORE-WINDOW] objective `lamp` fails before it can be done: its `done` `visited('ward.lamp')` can first hold at day 2 h03, but its deadline `by: clock.index > 8` already holds at day 2 h01 — move the deadline after that window
```

여기서 `ward.lamp`는 `holds('lit', ['hall'])`이 성립하는 동안에만 자격을 얻으며, `lit(hall)`의 유일한 규칙은 `cel("run.night == 2 && run.hour == 'h03'")`입니다. 기한과 같은 도착 순간에 `done`이 참이 되는 것은 문제가 아닙니다. 동률에서는 `done`이 이기기 때문입니다.

### 시간의 끝에서 기한 설정하기

"플레이어가 새벽에도 여기에 있으면 실패"는 `by="clock.ended"`입니다. `clock.ended`는 시계를 끝내는 `advance:`의 정착 과정에서, 마지막 `dawn`이 발생하기 전에 `true`가 되고, `newRun`까지 계속 `true`입니다. 그러나 `done`이 성립하는 순간, 어느 시간이든 목표는 여전히 완료됩니다:

```lute unverified="needs the one-night clock from world.schema.yaml above; checked by hand in a scratch project"
<quest id="escape" title="Out before dawn" tier="run">
  <objective id="out" title="Leave the ward" done="visited('ward.exit')" by="clock.ended"/>
</quest>
```

플레이 스텝은 `expect: { clock: { ended: true } }`로 그 순간을 검사합니다(끝나지 않는 시계에는 검사할 `ended`가 없으며, 그렇게 쓰면 사용 오류입니다).

비슷해 보이지만 같은 뜻이 아닌 세 가지 표기가 있습니다:

- `by="clock.index >= 6"`은 마지막 위치를 지정하며 마지막 위치는 끝이 아닙니다. 마지막 위치인 `h05`에 도달하는 이동이 목표를 실패시키므로 시계가 끝나기 한 슬롯 전에 실패합니다. "마지막 시간까지"에는 사용하지만 "새벽에"에는 사용하지 마세요.
- `on="dawn" until="true"`도 새벽에 실패하지만, `on=`은 목표 전체를 그 발생으로 이동시킵니다. 따라서 `done`도 새벽에만 판정됩니다. `h01`에 탈출한 플레이어도 새벽까지 `active`로 남고 `questComplete` 핸들러가 그때 실행됩니다. 이것이 원하는 경우에만 사용하세요.
- `by="run.hour == 'h03'"`은 순간이며 기한이 아닙니다. 시계가 `h03`에 서 있을 때만 성립합니다. `h04` 이후에 활성화되는 목표는 성립을 볼 수 없으므로 절대 실패하지 않습니다. `by="clock.index >= 4"`는 `h03`부터 계속 성립하며, 목표가 활성화되는 즉시 실패시킵니다.

`terminal: "clock.ended"`는 시계가 끝날 때 게임을 끝냅니다. 런이 시작하는 날의 `dayStart`는 시계가 발생시키지 않습니다. 이동은 진입하는 각 날에 `dayStart`를 발생시키며, 런은 day 1 안에서 시작하기 때문입니다. 엔진이 런 시작 시 이를 발생시키면 `raiseAtStart: true`를 선언하세요([런이 시작하는 위치](#런이-시작하는-위치) 참고). 그렇지 않다면 플레이가 시작하는 계기에 첫 아침 비트를 배치하세요. 이 키가 없으면 `lute calendar --axis clock`은 day 1의 `dayStart` 셀을 `not raised`로 표시하며, day 1의 `dayStart`에서만 가능한 비트는 결코 자격을 얻지 못하는 것으로 표시합니다.

## 함께 출시된 기능

시계와 함께 세 가지 작은 기능이 도입되었습니다(dsl 0.24.0 §1). 어느 것도 시계를 필요로 하지 않습니다.

### enum 멤버 표시 라벨

enum 멤버는 이름이며, 이름이 언제나 플레이어가 읽어야 할 텍스트인 것은 아닙니다. 긴 형식의 enum은 `labels:`로 멤버의 표시 텍스트를 지정할 수 있습니다:

```yaml
enums:
  weekday:
    members: [mon, tue, wed, thu, fri, sat, sun]
    labels: { mon: Monday, tue: Tuesday, wed: Wednesday, thu: Thursday, fri: Friday, sat: Saturday, sun: Sunday }
state:
  run.weekday: { type: { domain: weekday }, default: mon }
```

해당 enum(`{ domain: weekday }`)으로 타입이 지정된 상태 경로의 `{{path}}`는 라벨을 렌더링하므로, 아래 줄은 `Today is sun.`이 아니라 `Today is Sunday.`로 읽힙니다. 라벨이 없는 멤버는 자신의 id를 렌더링합니다. `lute play`, `lute run`, `lute trace`, `lute test`는 모두 같은 방식으로 처리하며, 컴파일된 상태 항목에는 엔진을 위한 `labels`가 포함됩니다. 조건은 여전히 id를 비교합니다: `run.weekday == 'sun'`.

```lute check
---
kind: scene
id: square.notice
enums:
  weekday:
    members: [mon, tue, wed, thu, fri, sat, sun]
    labels: { mon: Monday, tue: Tuesday, wed: Wednesday, thu: Thursday, fri: Friday, sat: Saturday, sun: Sunday }
state:
  run.weekday: { type: { domain: weekday }, default: mon }
---

## Opening {#opening}

@narrator: Today is {{run.weekday}}.
```

멤버가 아닌 항목에 라벨을 지정하면 `E-ENUM-LABEL-NOT-MEMBER`, 문자열이 아닌 라벨은 `E-META-VALUE`입니다:

```lute expect="E-ENUM-LABEL-NOT-MEMBER"
---
kind: scene
id: square.notice
enums:
  weekday:
    members: [mon, tue, wed, thu, fri, sat, sun]
    labels: { mon: Monday, sunday: Sunday }
state:
  run.weekday: { type: { domain: weekday }, default: mon }
---

## Opening {#opening}

@narrator: Today is {{run.weekday}}.
```

라벨은 모든 긴 형식 enum에서 작동합니다. 문서의 `enums:`, 스키마의 enum, 플러그인의 `enums/` export가 해당합니다([어휘](/ko/language/vocabulary/) 참고). 이제 `{ domain: X }` 타입의 상태 경로는 `X`를 읽는 것으로 계산되므로, 상태 타입 지정에만 사용한 도메인도 더 이상 `W-DOMAIN-UNREAD`를 발생시키지 않습니다.

### 정수 `%`

`%`는 정수 나머지이며 CEL 프로필에 추가되었습니다(예전에는 `E-CEL-PROFILE`이었습니다). 이제 "7일마다"를 조건으로 쓸 수 있고, 이를 기반으로 한 def는 추론에 의해 `int` 타입이 됩니다:

```lute check
---
kind: scene
id: square.restDay
on: townVisit
when: "@restDay"
once: false
state:
  run.day: { type: int, default: 1 }
defs:
  restDay: "run.day % 7 == 0"
---

## Opening {#opening}

@narrator: The shutters stay down. Nobody works on the seventh day.
```

두 피연산자는 모두 정수여야 합니다. `int`가 아닌 피연산자(`'a' % 2`, bool 경로)나 소수 리터럴은 `E-CEL-TYPE`입니다:

```lute expect="E-CEL-TYPE"
---
kind: scene
id: square.restDay
on: townVisit
when: "run.day % 2.5 == 0"
state:
  run.day: { type: int, default: 1 }
---

## Opening {#opening}

@narrator: Half a rest.
```

숫자 경로가 정수라는 것을 검사기가 증명할 수 없으므로 나머지 규칙은 엔진에 달려 있습니다. 결과는 버림 나머지이며 피제수의 부호를 따릅니다. `-7 % 3 == -1`이고 `7 % -3 == 1`입니다. 소수 값이나 0인 제수의 결과는 알 수 없음이 되며, 소수 나머지가 되거나 충돌하지 않습니다. `lute trace`, `lute test`, `lute play`, 조건 판정기는 모두 이 규칙을 따르므로 `-7 % 3 == 2`는 데드 조건입니다.

### 가드된 쓰기: `::set{… when="…"}`

조건부 쓰기 하나를 쓰려면 예전에는 그 주위를 `<match>`나 `<when>`으로 감싸야 했습니다. `when="…"`은 `::set` 본문 안에서 한 줄로 이를 처리합니다. 조건이 성립할 때만 쓰기가 일어납니다.

```lute check
---
kind: scene
id: bakery.visit
state:
  run.day: { type: int, default: 1 }
  run.aff.wren: { type: int, default: 0 }
  run.warmed.wren: { type: int, default: 0 }
---

## Opening {#opening}

@wren: You again. Sit, the kettle's on.
::set{run.aff.wren += 1 when="run.warmed.wren < run.day"}
::set{run.warmed.wren = run.day}
```

Wren은 아무리 자주 방문해도 하루에 최대 한 번만 당신에게 마음을 엽니다. 가드는 줄의 `when=`과 같이 검사됩니다. bool CEL 조건이고 `$`는 쓰지 않으며(match subject가 없으며), 결코 성립할 수 없다는 것이 증명되면 `E-ARM-DEAD`가 됩니다:

```lute expect="E-ARM-DEAD"
---
kind: scene
id: bakery.visit
state:
  run.day: { type: int, default: 1 }
  run.aff.wren: { type: int, default: 0 }
---

## Opening {#opening}

@wren: Take this.
::set{run.aff.wren += 1 when="run.day > 5 && run.day < 3"}
```

가드된 쓰기는 확정 대입으로 계산되지 않습니다. `default`가 없는 경로를 나중에 읽으면 쓰기가 일어나지 않았을 수도 있으므로 여전히 `E-MAYBE-UNSET`입니다. `<track>` 안에서 가드된 `::set`은 `E-TIMELINE-CONTENT`입니다. 조건부 쓰기는 로직이므로 `<timeline>` 밖에 있어야 합니다. `when=`은 `::set` 본문이 받는 유일한 속성입니다. 그 밖의 내용은 표현식 텍스트로 읽히며, 그에 따른 `E-CEL-PARSE`는 그 자리에서 허용되는 속성이 `when=`뿐이라고 명시합니다.

이 쓰기는 가드된 줄이 컴파일되는 것과 같은 하나의 arm을 가진 `match`로 컴파일되므로 IR은 바뀌지 않습니다. `lute
play`는 건너뛴 쓰기를 `skip set run.aff.wren += 1 — when: false`로 출력합니다.
## 도구

- `lute play`는 `advance:` 스텝으로 시계를 이동하고, `include:`로 공유 스텝을 삽입하며, 모든 시계 이동을 `day 1 (Mon) night → day 2 (Tue) morning`처럼 표시합니다. [스토리 플레이](/ko/tooling/play/)를 참고하세요.
- `lute calendar --axis clock[=d1..d2]`는 시계를 슬롯마다 순회합니다. [개요](/ko/tooling/overviews/)를 참고하세요.
- `lute run`, `lute play`, `lute trace`는 현재 day와 slot에서 `clock.index`, `clock.weekday`, `clock.weekdayLabel`을 파생합니다. `run.day: 6`을 시드하는 trace mock은 `{{clock.weekdayLabel}}`을 `Sat`으로 렌더링합니다. 두 시계 경로인 `run.day`와 `run.slot`을 시드하세요. `clock.*` 경로를 시드하는 mock 또는 `--state`는 `E-TRACE-MOCK-UNDECLARED`입니다(`` `--state clock.index=…` seeds a path the clock
  derives from its day and slot, which no mock may set — seed `run.day` / `run.slot` instead ``).

엔진 계약(예약된 경로, `once: day` / `once: week` / `once: slot` 소진, 앞으로만 이동)은
[`docs/runtime/state-lifecycle.md`](https://github.com/journeyWorker/lute/blob/main/docs/runtime/state-lifecycle.md)에 있고, 정수 `%`는
[`docs/runtime/cel-and-facts.md`](https://github.com/journeyWorker/lute/blob/main/docs/runtime/cel-and-facts.md)에 있습니다. 규범 사양은
[`0.24.0.md`](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.24.0.md)
§1이며, `once: week`에 관한 초안은
[`0.27.0.md`](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.27.0.md)
§5입니다.
