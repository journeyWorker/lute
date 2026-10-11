---
title: 선택지와 허브
description: 선택지 메커니즘 — 가드와 into= 실행 기록 축약 문법을 사용하는 시점 — 그리고 once/exit 플래그, 허브 프롬프트, 막다른 길 없음이 보장되는 <hub> 대화를 재방문합니다.
---

`<choice>`는 [`<branch>`](/ko/language/branch-match-when/) 또는 `<hub>` 안에 있는 하나의 선택지입니다. 모든
선택지에는 **`id`** (기록되는 키)와 **`text`** (보간될 수 있는 버튼 텍스트)가 필요합니다. 그 외에도 가드와
실행 기록 축약 문법을 담습니다.

## 가드

선택지에는 **`when`** CEL 가드를 지정할 수 있으며, 가드가 성립할 때만 선택지가 제공됩니다. 메뉴가
비어 있음이 증명되는 일이 없도록, 분기에는 가드가 없는 선택지가 하나 이상 포함되어야 합니다
(`E-BRANCH-ALL-GUARDED`).

```lute
<choice id="soft" text="Ask gently" when="@showcaseReady">
  @fixer{code="0052"}: Marina — would you mind terribly if I had your number?
</choice>
```

## 선택지 기록하기

선택지를 선택하면 해당 id가 예약된 경로 `scene.choices.<branchId>`에 기록됩니다(도메인: 분기의
선택지 id ∪ `unset`). 이 경로는 에피소드가 끝날 때 지워지므로 **에피소드 내** 반응만 구동합니다 —
이후 섹션의 `<match subject="scene.choices.…">`가 이를 사용합니다.

선택지가 동일한 실행의 **이후 에피소드**에 영향을 주도록 하려면, `into=` 축약 문법으로 이름 있는
`run.*` 팩트를 기록합니다.

```lute
<branch id="sofaHelp">
  <choice id="help" text="Help her up" into="run.metHelpfully">
    @elena: Thank you. I won't forget this.
  </choice>
  <choice id="warmly" text="Help, and stay a while" into="run.sofaHelpOutcome" value="warm">
    @elena: You're very kind — really.
  </choice>
  <choice id="tip" text="Leave a little something" into="run.tip" value="5">
    @elena: Oh — you didn't have to.
  </choice>
</branch>
```

([`docs/examples/choice-persist.lute`](https://github.com/journeyWorker/lute/blob/main/docs/examples/choice-persist.lute)에서 가져왔습니다.)

`into="run.<path>"`만 지정하면 해당 분기에 `::set{run.<path> = <value>}`가 추가됩니다. 대상 경로는
**`into`**로 이름을 지정하며 스키마에 선언되어 있어야 합니다. `value`는 bool 경로의 경우
`true`로 기본 설정되며, `enum` 또는 숫자(`int` 또는 `double`) 경로에는 명시적인 `value`가
필요합니다. (0.6.0에서 이전의 `persist="run"` 속성이 제거되었습니다. 남아 있는 `persist=`는 이제
`E-PERSIST-REMOVED`가 되며, `lute fix`가 자동으로 삭제합니다.) 이후 에피소드는 이미 지워진 원시
선택지 키가 아니라 이름 있는 팩트를 읽어 반응합니다.

```lute
<match subject="run.metHelpfully">
  <when is="true">
    @elena: You helped me back then. I've been meaning to thank you again.
  </when>
  <otherwise>
    @elena: ...Have we met before?
  </otherwise>
</match>
```

## 컴파일된 기록

`<branch>`는 하나의 `choice` 레코드로 컴파일됩니다. `prompt="…"`는 호스트가 옵션 위에 표시하는
줄이며, `timeout="N"`은 초 단위의 정수 카운트다운입니다(양의 정수가 아니면
`E-BRANCH-TIMEOUT`입니다). 시간 제한이 있는 branch는 `default="<choice id>"`로 시간이 다 되었을 때의
결과도 반드시 지정해야 합니다. 두 속성은 서로를 요구하며, id는 이 branch의 직접 옵션이어야 하고,
그 옵션에는 `when`을 붙일 수 없습니다(`E-BRANCH-DEFAULT`).

```lute check
---
kind: scene
id: bar.counter
title: The number
---

## Counter

<branch id="number" prompt="Ask for her number?" timeout="10" default="soft">
  <choice id="blunt" text="Just ask, flatly">
    @fixer: Marina. Your number.
  </choice>
  <choice id="soft" text="Ask gently">
    @fixer: Would you mind terribly?
  </choice>
</branch>
```

```json
{
  "kind": "choice",
  "family": "control",
  "position": "001-0100",
  "branchId": "number",
  "selectionKey": "scene.choices.number",
  "options": [
    {"id": "blunt", "text": "Just ask, flatly", "lineId": "bar.counter.number.blunt", "target": "001-0200"},
    {"id": "soft", "text": "Ask gently", "lineId": "bar.counter.number.soft", "target": "001-0400"}
  ],
  "converge": "001-0600",
  "prompt": "Ask for her number?",
  "timeout": 10,
  "default": "soft"
}
```

- **`selectionKey`**는 선택 결과가 기록되는 경로인 `scene.choices.<branchId>`입니다.
- 각 옵션에는 버튼 **`text`**, 번역을 위한 `{prefix}.{branch id}.{choice id}` 형식의 `lineId`, 그리고
  해당 분기가 시작되는 `target` 위치가 있습니다. 모든 분기는 `converge`에서 계속됩니다.
- **`prompt`**와 **`timeout`**은 작성된 경우에만 존재합니다. `timeout`은 선택 없이 분기를 끝낼 수
  있으므로, 그 뒤 `scene.choices.number`를 대상으로 하는 `<match>`에는 `is="unset"` 분기 또는
  `<otherwise>`가 필요합니다.

`<hub>`는 같은 형태의 `hub` 레코드로 컴파일됩니다. 여기에는 `id`, `selectionKey`
(`scene.choices.<hubId>`), `once` 및 `exit` 플래그를 추가하는 항목들로 구성된 `options`,
`converge`, 선택적인 `prompt`, 그리고 [`<return>`](#돌아오기-return) 블록이 있는 허브의 경우
해당 블록이 시작되는 `return` 위치가 들어 있습니다.

## 허브 재방문

`<hub id>`는 재방문 대화입니다. 진입할 때와 `exit`이 아닌 각 분기가 완료된 뒤에 현재 자격이 있는
모든 선택지를 **다시 제시**하므로, 플레이어는 원하는 순서로 대화 사이를 오갈 수 있습니다. 허브
선택지에는 **`once`**와 **`exit`**이라는 두 가지 추가 불리언 플래그가 있습니다.

```lute
<hub id="chatWithMarina">
  <choice id="askCoffee" text="Ask about the coffee" once>
    @marina{code="0020" emotion="content" variant="0"}: House blend. Bold, like the clientele.
  </choice>
  <choice id="compliment" text="Say she was kind earlier" when="@helped">
    @fixer{code="0030"}: You were gentle about it before. It stuck with me.
    ::set{scene.affect.marina += 1}
  </choice>
  <choice id="leave" text="Head out" exit>
    @fixer{code="0040"}: I'd better get moving.
  </choice>
</hub>
```

([`docs/examples/showcase/hub-demo.lute`](https://github.com/journeyWorker/lute/blob/main/docs/examples/showcase/hub-demo.lute)에서 가져왔습니다.)

- **자격.** 선택지는 `when` 가드가 있다면 그 가드가 성립하고, `once`가 지정되었다면 이 장면에서
  아직 선택되지 않았을 때 자격이 있습니다.
- **`once`.** 첫 번째로 선택된 뒤에는 `once` 선택지가 자격이 있는 집합에서 제거됩니다. `once`가
  없는 선택지는 선택 가능한 상태로 남아 다시 선택할 수 있습니다.
- **기록.** `once` 여부와 관계없이 *모든* 선택은 `scene.visited.<hub>.<choice> = true`를 설정합니다
  (기본값은 `false`임). 이를 통해 엔진은 이미 본 주제를 회색으로 표시할 수 있습니다. 선택지가
  선택될 때 분기가 실행되기 전에 설정되므로, 해당 분기 안에서는 항상 `true`입니다(그 안에서
  `false`가 필요한 `<match>` 분기는 `E-ARM-DEAD`입니다). 첫 선택과 이후 선택을 구분하려면 분기가
  설정하는 `scene.*` int 경로에서 선택 횟수를 셉니다. 허브는 `scene.choices.<hubId>` (마지막으로
  선택된 enum)도 접습니다 — 둘 다 `<match>`에서 읽을 수 있습니다.
- **종료.** `exit` 선택지를 선택하면 해당 분기가 실행된 다음 허브를 떠납니다. 제시 시점에 자격이
  있는 선택지가 없으면 허브가 자동으로 종료됩니다.
- **플래그에는 값을 지정하지 않습니다.** `once`와 `exit`는 단독으로 작성합니다
  (`exit="true"` / `exit="false"`도 작성된 것으로 읽습니다). 그 밖의 값은 `E-FLAG-VALUE`이며,
  `once="run"`과 같은 비트/엔트리 기간도 마찬가지입니다. 선택지의 `once`는 허브 방문당 한 번을
  의미할 뿐 더 긴 기간을 의미하지 않습니다. 같은 규칙이 `<objective optional>`과 `<beat also>`에도
  적용됩니다.

### 허브 프롬프트

허브는 무엇을 묻고 있는지 말할 수 있습니다. `prompt="…"` (dsl 0.23.0)는 허브가 옵션을 제시할
때마다 호스트가 해당 옵션과 함께 표시하는 줄을 붙입니다. 일회성 메뉴에 대해
`<branch prompt>` (dsl 0.11.1)가 하는 방식과 같습니다.

```lute check
---
kind: scene
id: bar.marina
title: The bar
---

## Bar

<hub id="chat" prompt="Marina polishes a glass and waits.">
  <choice id="coffee" text="Ask about the coffee" once>
    @marina: House blend. Bold, like the clientele.
  </choice>
  <choice id="leave" text="Head out" exit>
    @fixer: I'd better get moving.
  </choice>
</hub>
```

프롬프트는 허브 레코드의 `prompt` (`HubCmd`에 포함되며, 작성하지 않으면 생략됨)로 컴파일되고,
`lute run` / `lute play`는 허브가 제시될 때마다 프롬프트를 출력합니다. 빈 프롬프트는 브랜치와
마찬가지로 `E-BRANCH-PROMPT`입니다.

### 돌아오기: `<return>`

`<hub>` 앞에 작성한 텍스트는 한 번 실행됩니다. 플레이어가 허브로 돌아올 때마다 실행할 텍스트는
허브 안에 `<return>` 블록을 넣습니다 (dsl 0.28.0). 이 블록은 `exit`이 아닌 각 분기 뒤, 옵션이 다시
표시되기 전에 실행됩니다. 첫 메뉴 전에 실행되지 않으며 `exit` 분기 뒤에도 실행되지 않습니다.
`once` 선택으로 메뉴가 비게 되더라도 허브가 닫히기 직전에 실행됩니다.

```lute check
---
kind: scene
id: lighthouse.lamp
title: The lighthouse
---

## Lamp room

@narrator: A lamp room, all brass and salt.
<hub id="lamp">
  <return>
    @narrator: The lamp room again.
  </return>
  <choice id="ledger" text="Read the ledger">
    @narrator: The last entry is smudged.
  </choice>
  <choice id="leave" text="Go down the stairs" exit>
    @narrator: You leave the lamp burning.
  </choice>
</hub>
```

`--choose lamp=ledger,ledger,leave`와 함께 `lute trace`를 실행하면 "The lamp room again."이
두 번 출력됩니다. 허브는 `<return>` 블록을 최대 하나만 가질 수 있으며, 블록에는 속성을 지정할 수
없습니다. 텍스트를 다르게 하려면 해당 줄에 `when=`으로 가드를 지정하거나 `<match>` 안에 넣습니다.
[Ink 또는 Yarn에서 가져오기](/ko/guides/coming-from-ink-yarn/)에서는 플레이어가 계속 돌아오는 Ink knot를
이 방식으로 포팅합니다.

### 막다른 길 없음

허브는 종료할 수 있음을 보장해야 합니다. **가드가 없는 `exit`** 선택지가 하나 이상 있거나, 모든
선택지가 `once`여야 합니다(따라서 자격이 있는 집합이 확실히 비게 되어 자동 종료가 실행됩니다).
둘 다 만족하지 않는 허브는 `E-HUB-NO-EXIT`입니다. 허브는 빌드 시 하나의 유한한 옵션 테이블과
그 분기들로 축소되므로, 런타임에 다시 제시하더라도 계산이 무한히 늘어나지 않으며 — 전체성이
보존됩니다.
