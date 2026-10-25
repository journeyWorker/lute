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

### `into=`는 먼저 씁니다

`into="run.<path>"`(선택적 `value` 포함)는 `::set{run.<path> = <value>}`를 분기 안 **맨 앞**, 어떤 줄이나 디렉티브보다 먼저 두는 것과 같습니다. 0.49.0부터 검사기도 그렇게 봅니다. 분기 안에서 그 경로를 읽으면 이미 대입된 상태이므로 거기서 `has()` 가드나 `unset` 갈래가 필요 없습니다. `::jump`로 끝나는 분기도 떠나기 전에 쓰기를 기록합니다. `E-INTO-*` 진단과, 분기가 같은 경로를 이미 `::set`하는 경우의 오류는 그대로입니다.

`into="run.<path>"`만 지정하면 그 `::set{run.<path> = <value>}`가 됩니다. 대상 경로는
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

## 자유 텍스트 입력

Lute에는 텍스트 입력 구성물이 없고, 필요하지도 않습니다. 플레이어에게 답을 입력받는 일은 [브리지](/ko/plugins/bridge/) 호출입니다. 플러그인이 `effects.writes` 항목이 `fromBridgeResult`를 읽는 블로킹 브리지 디렉티브를 선언하면, 호스트가 텍스트 상자를 띄우고 입력한 답이 디렉티브가 선언한 상태 슬롯에 들어갑니다. 이후 `<match>`나 `<branch>`가 그 슬롯을 다른 상태처럼 읽습니다.

```lute
@guard: Halt. What is the password?

::ask{slot="password"}

<match subject="scene.ask.password.answer">
  <when is="swordfish">
    @guard: Pass, friend.
  </when>
  <otherwise>
    @guard: Wrong. Move along.
  </otherwise>
</match>
```

호출이 블로킹이므로 `scene.ask.password.answer`는 `::ask` 뒤에 대입된 상태가 되고([동기화 의미론](/ko/plugins/bridge/#동기화-의미론) 참고) `<match>`는 가드 없이 읽습니다. `wait="false"`는 호출하고 계속 진행하는 경우에만 쓰세요. 논블로킹 호출은 확정 할당 보장을 더하지 않으므로, 기본값이 없고 다른 것으로 보호되지 않는 결과를 읽으면 `E-MAYBE-UNSET`이 보고될 수 있습니다. 호스트 쪽 — 무엇을 받고 어떻게 답하는지 — 은 [런타임 계약](/ko/tooling/runtime-contract/)에 있습니다. 검사되는 예제는 [`docs/examples/free-text-input/`](https://github.com/journeyWorker/lute/tree/main/docs/examples/free-text-input)이며, 맞는 답과 틀린 답에 대한 `lute test`가 있습니다.

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
  "promptLineId": "bar.counter.number.prompt",
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
- 프롬프트도 선택지 라벨처럼 번역할 수 있습니다(dsl 0.47.0). lineId **`promptLineId`**는
  `{prefix}.{branch id}.prompt`이며 `prompt`가 있을 때만 존재합니다.
  [`lute loc export`](/ko/tooling/cli/#loc-export)는 이를 `kind: "prompt"` 행으로 쓰고,
  `lute compile --locale-bundle`는 번역을 **`promptTexts`**(로케일 → 텍스트, 비어 있으면 생략)에
  병합합니다. `prompt`는 원문 그대로입니다. 프롬프트와 선택지는 `{prefix}.{branch id}.…`
  네임스페이스를 공유하므로 `prompt=`가 있는 branch 안의 `<choice id="prompt">`는
  `E-CHOICE-DUP`입니다 — 선택지 이름을 바꾸세요. `prompt=`가 없으면 이 id도 쓸 수 있습니다.

`<hub>`는 같은 형태의 `hub` 레코드로 컴파일됩니다. 여기에는 `id`, `selectionKey`
(`scene.choices.<hubId>`), `once` 및 `exit` 플래그를 추가하는 항목들로 구성된 `options`,
`converge`, 선택적인 `prompt`(branch와 같이 `promptLineId`, `promptTexts` 포함), 그리고 [`<return>`](#돌아오기-return) 블록이 있는 허브의 경우
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
마찬가지로 `E-BRANCH-PROMPT`입니다. 브랜치 프롬프트처럼 번역할 수 있습니다. lineId는
`{prefix}.{hub id}.prompt`(`promptLineId`, 여기서는 `bar.marina.chat.prompt`)이고 번역은
`promptTexts`에 들어가며, 프롬프트가 있는 허브 안의 `<choice id="prompt">`는 `E-CHOICE-DUP`입니다.

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

`--choice lamp=ledger,ledger,leave`와 함께 `lute trace`를 실행하면 "The lamp room again."이
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

## 허브와 루프

둘 다 플레이어에게 다시 묻지만 답하는 질문이 다릅니다. 0.51부터는 한정된
[`::jump{to="…" max="…"}`](/ko/language/directives/#한정된-뒤로-점프--max)가 보행을 앞선 `::label`로
되돌릴 수 있으므로 `<branch>`도 여러 번 물을 수 있습니다.

- **`<hub>`를 쓰는 경우:** 플레이어가 언제 떠날지 정할 때입니다. 여러 화제가 있는 대화, `exit` 선택지를
  고를 때까지 남아 있는 메뉴, 사라지는 `once` 선택지, 방문 사이에 실행되는 `<return>` 블록이 여기에 해당합니다.
  플레이어의 협조로 끝나며, 검사기는 끝날 수 있음을 증명합니다([막다른 길 없음](#막다른-길-없음)).
- **루프를 쓰는 경우:** *게임*이 몇 번 반복할지 정할 때입니다. 미니게임 실패 후 재시도, "다시 해 보기(최대
  세 번)", 정해진 횟수만 반복되는 프롬프트가 해당합니다. 한계는 1부터 99까지의 리터럴 `max`이며, 플레이어의
  협조 없이도 보행이 끝납니다.

```lute unverified="0.51 feature; verified after S1-S8 land"
::label{name="retry"}
<branch id="again" prompt="The lock is still stuck.">
  <choice id="push" text="Push harder">
    @mira: It won't budge.
    ::jump{to="retry" max="2"}
  </choice>
  <choice id="leave" text="Give up">
    @mira: Fine. Another way, then.
  </choice>
</branch>
```

여기서 `push`는 보행을 라벨로 되돌리고, 두 번 실행된 뒤 세 번째 `push`는 `<branch>` 뒤로 이어집니다. 다시
도달한 `<branch>`는 새 메뉴이며 `presentation`은 `0`으로 유지됩니다. 루프 안의 `<hub>`는 방문마다
새로 시작하므로 `once` 선택지를 다시 쓸 수 있습니다. 루프는 허브 옵션 본문 안에 둘 수 있지만(그 옵션 안에
`::label`을 두세요), 허브 옵션 안에서 그 바깥으로 되돌아가는 루프는 `E-JUMP-LOOP-SCOPE`입니다.

## `<branch>`, `<hub>`, `<choice>`의 플러그인 스탬프

0.50부터 플러그인은 이 세 요소에 자체 속성을 붙일 수 있습니다. `on:`이 `branch`, `hub`, `choice`를 이름으로 지정하는 [`stampAttrs`](/ko/plugins/manifests/#표면에-스탬프-한정하기-on) 엔트리는 그곳에서 허용되며, 그 밖의 곳에서는 허용되지 않습니다. 횡단 스탬프(`on:` 없음)는 `<branch>`, `<hub>`, `<choice>`에서 여전히 `E-UNKNOWN-ATTR`입니다. 다음 매니페스트가 있으면

```yaml
stampAttrs:
  - { name: menuType, type: { enum: [plain, timed, minigame] }, on: [branch, hub] }
  - { name: optionKey, type: string, required: true, on: [choice] }
```

메뉴는 다음과 같이 씁니다.

```lute
<branch id="route" prompt="Which way?" menuType="plain">
  <choice id="goLeft" text="Left" optionKey="L1">
    @mira: Left it is.
  </choice>
  <choice id="goRight" text="Right" optionKey="R1">
    @mira: Right, then.
  </choice>
</branch>
```

같은 속성을 세 요소 중 여러 곳에 한정할 수 있고(`on: [branch, choice]`), 허브의 선택지도 `choice`입니다. 해석은 평소의 엄격한 순서를 따릅니다. 요소 자체의 속성(선택지는 `id`, `text`, `when`, `into`, `value`, `once`, `exit`, 브랜치는 `id`, `prompt`, `timeout`, `default`, 허브는 `id`, `prompt`)이 먼저이고, 그다음 해당 표면에 선언된 스탬프, 그다음 `E-UNKNOWN-ATTR`입니다.

- **알 수 없는 속성.** 다른 표면에만 있는 스탬프 이름은 요소의 속성을 나열하는 대신 적용되는 곳을 알려 주는 `E-UNKNOWN-ATTR`입니다: `` `<branch>` has no attribute `optionKey`: it is a stamp of `acme.stage` for `<choice>` only ``. 스탬프가 전혀 아닌 이름이면 메시지가 요소의 속성과 허용되는 스탬프를 나열합니다: `` `<choice>` has no attribute `cue`; its attributes are `id`, `text`, `when`, `into`, `value` and the stamps `optionKey` (`acme.stage`) ``.
- **필수.** 해당 표면에 `required: true`로 선언되었는데 빠진 스탬프는 요소의 여는 태그 위치에서 `E-MISSING-ATTR`입니다: `` `<choice id="goLeft">` requires attribute `optionKey` (a stamp of `acme.stage`) ``. 선언된 `default:`가 이를 채우지는 않습니다.
- **형식 지정.** 값은 그 표면의 선언으로 형식이 지정됩니다. enum 밖으로 접힌 값은 `E-BAD-ENUM`(`@m`이 enum에 없는 `"grid"`로 접히는 `<hub menuType=@m>`), 잘못된 스칼라는 `E-ATTR-TYPE`입니다. `@ref` 값도 같은 선언으로 형식이 지정됩니다. `@m`이 `int` def인 `<branch menuType=@m>`은 `E-REF-TYPE`이고, `@k`가 문자열 def인 `<choice optionKey=@k>`는 깨끗합니다.

**아티팩트에서.** `choice` 또는 `hub` 기록은 `<branch>` / `<hub>` 요소의 스탬프를 자체 `stamps` 객체에 담고, 각 옵션은 `<choice>` 요소의 스탬프를 마지막에 직렬화되는 자체 `stamps`에 담습니다. 각 값은 해당 표면의 선언으로 형식이 지정됩니다. 스탬프가 없는 옵션은 이전과 바이트 단위로 동일하며, 작성하지 않은 스탬프는 주입되지 않습니다.

```json
{
  "kind": "choice",
  "branchId": "route",
  "options": [
    {"id": "goLeft", "text": "Left", "lineId": "…", "target": "001-0200", "stamps": {"optionKey": "L1"}},
    {"id": "goRight", "text": "Right", "lineId": "…", "target": "001-0400", "stamps": {"optionKey": "R1"}}
  ],
  "stamps": {"menuType": "plain"}
}
```

스탬프는 설명용입니다. 런타임은 이를 읽지 않으며 `awaitChoice` 메뉴도 바뀌지 않습니다. 레퍼런스 플레이어의 `ChoiceView`는 메뉴 주소의 IR 기록에서 읽은 요소의 `stamps`와 각 옵션의 `stamps`를 담고, 없으면 생략하며, 프레젠터는 `showChoice`로 이를 받습니다. 레퍼런스 플레이어를 쓰지 않는 호스트는 옵션 텍스트를 읽듯 IR 기록에서 같은 필드를 읽습니다([런타임 계약](/ko/tooling/runtime-contract/) 참고).
