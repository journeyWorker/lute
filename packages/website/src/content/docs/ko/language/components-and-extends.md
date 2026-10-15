---
title: 컴포넌트와 extends
description: 두 가지 재사용 메커니즘 — ::use로 호출하는 재사용 가능한 콘텐츠 컴포넌트(각 호출 위치에 문장을 전달하는 문자열 매개변수, 본문이 말할 화자를 지정하는 화자 매개변수, 매개변수 기본값, 자체 플러그인 디렉티브 결과를 읽는 본문, 가드된 ::use, 상태를 쓰는 effects 컴포넌트 포함)와 기본 레이어보다 우선하는 오버라이드로 스키마를 조합하는 extends — 을 설명합니다.
---

Lute에는 서로 다른 용도를 위한 세 가지 재사용 메커니즘이 있습니다. `defs`는 타입이 있는 CEL *값*을 재사용하고,
`uses`/`extends`는 *스키마*를 재사용하며, **컴포넌트**는 *콘텐츠*를 재사용합니다. 이 페이지에서는 콘텐츠
컴포넌트와 스키마 `extends:` 조합을 다룹니다.

## 재사용 가능한 콘텐츠 컴포넌트

**컴포넌트**는 호출되는 곳마다 인라인으로 확장되는, 이름이 있고 매개변수화된 줄과 연출 블록입니다. 컴포넌트는
전용 **컴포넌트 파일** — 프런트매터에 `component: <name>`과 선택적인 `params:`( [def 매개변수](/ko/language/params/)와
동일하게 타입을 지정하며, 0.24.0부터 [`speaker`](#화자-매개변수) 타입도 포함) — 인 `.lute` 문서에 있습니다.
파일이 [`effects: true`](#상태를-쓰는-컴포넌트-effects-true)를 선언하지 않는 한 본문은 **프레젠테이션 템플릿**입니다.

```lute check="docs/examples/components/greet.component.lute"
---
component: greet
params:
  who: string
uses: ../base.schema.yaml
---

## A Familiar Face

::actor{character=@who action="fadeInUp"}
@narrator: A familiar face steps into the light.
```

*([`docs/examples/components/greet.component.lute`](https://github.com/journeyWorker/lute/blob/main/docs/examples/components/greet.component.lute)에서 가져왔습니다.)*

매개변수는 ref 및 속성 위치에서는 `@<param>`으로, 콘텐츠 텍스트 안에서는 `{{@param}}` 보간으로 참조합니다.
`@who`는 확장 시점에 호출 인수에 바인딩되므로 해당 속성이 `string` 타입인 `character` 위치에서 사용할 수 있습니다.

컴포넌트 줄의 `{{…}}` 보간은 **`int`, `double`, bool 또는 enum** 매개변수를 렌더링하며(§7.6),
dsl 0.23.0부터는 인수가 리터럴인 **`string`** 매개변수도 렌더링합니다. 따라서 위의 `who: string`은 줄 안에서
`{{@who}}`로도 나타낼 수 있습니다. [문자열 매개변수의 문장](#문자열-매개변수의-문장)을 참고하세요.

`uses:` 줄은 컴포넌트 자체의 [콘텐츠 어휘](/ko/language/vocabulary/) 가져오기입니다 — `0.9.0`부터
`action="fadeInUp"`은 선언된 `action` 도메인에 대해 해석되며, 컴포넌트 파일은 스스로 검사되려면 해당 도메인에
도달해야 합니다. `::use`를 통하면 적용되는 어휘는 **가져오는** 문서의 어휘입니다([알려진 제한](/ko/language/vocabulary/#알려진-제한-컴포넌트-본문은-가져오는-문서에-대해-해석됩니다) 참고).
따라서 양쪽 모두 어휘를 선언합니다.

씬은 `components:` 프런트매터 키로 컴포넌트를 가져옵니다(`uses:`와 마찬가지로 정규화되고, 순환을 검사하며,
다이아몬드 중복을 제거합니다 — [가져오기](/ko/language/imports/) 참고). 그런 다음 예약된 내장 디렉티브
**`::use`**로 하나를 호출합니다:

```lute check="docs/examples/components/scene.lute"
---
kind: scene
character: demo
season: 1
episode: 2
uses: ../base.schema.yaml
components: [greet.component.lute]
---

## Greeting by Component

::use{component="greet" who="marina"}
@narrator: And the scene carries on.
```

*([`docs/examples/components/scene.lute`](https://github.com/journeyWorker/lute/blob/main/docs/examples/components/scene.lute)에서 가져왔습니다.)*

`::use`는 이름이 지정된 컴포넌트의 본문을 인라인으로 확장하고 각 `@param`을 일치하는 이름 있는 인수에 바인딩합니다.
인수 개수와 타입은 검사되며(`E-COMPONENT-ARG`), 가져온 파일에 없는 컴포넌트 이름 지정은 `E-COMPONENT-UNDECLARED`입니다.
리터럴 인수는 확장에 치환되므로, 컴포넌트 줄의 `{{@weather}}`는 `weather="grey"`일 때 `Outside: grey.`로 컴파일됩니다.

`component`, `when`, `at`, `instance`는 `::use` 자체가 예약한 이름입니다.
컴포넌트 param에는 이 이름을 사용할 수 없습니다. `at`은 타임라인 클립 오프셋이고
`instance`는 use의 식별자입니다. 선언의 해당 `params:` 항목에서 전체 예약 목록과
이름 변경 제안을 포함한 `E-TEMPLATE` 하나가 보고되며, 컴포넌트 사용처에서는 같은
param 진단을 반복하지 않습니다. beat-template 헤더 키의 기존 예약 규칙은 그대로입니다.

인수는 **호출 문서의 def**일 수도 있습니다 — `::use{component="greet" tier=@mood}`처럼 씁니다.
컴포넌트는 여전히 스스로 상태를 읽지 않습니다. 상태 읽기가 있는 곳은 호출자의 def이며, 그 def가 들어간 위치에서 검사됩니다.

- enum 매개변수의 경우 def 본문이 만들 수 있는 모든 값이 멤버여야 하며, 그렇지 않으면 `E-COMPONENT-ARG`입니다
  (`@mood` = `"run.n > 2 ? 'warm' : 'hot'"`를 `{ enum: [cold, warm] }`에 대입하면 `hot`에서 실패합니다).
- `<match subject="@tier">`에서는 다른 subject와 마찬가지로 런타임에 분기합니다.
- 콘텐츠 텍스트에서는 `{{@n}}`가 호출자의 def를 이름 붙이는 플레이스홀더로 남고, 엔진이 이를 평가합니다
  (`int`, `double`, bool 또는 enum 매개변수이며, 줄에서 보간하는 `string` 매개변수는 리터럴만 취함,
  [아래](#문자열-매개변수의-문장) 참고).
- 디렉티브 속성에서는 상수로 접혀야 하며, 상태에 의존하는 def는 `E-ATTR-DEF-DYNAMIC`입니다.


### 문자열 매개변수의 문장

컴포넌트 줄은 `string` 매개변수를 보간할 수 있습니다(dsl 0.23.0). 리터럴 `::use` 인수는 다른 리터럴 인수와
마찬가지로 컴포넌트가 확장될 때 줄에 치환되므로, 각 호출 위치는 각자의 완성된 문장을 갖습니다:

```lute check
---
component: cheers
params:
  to: string
---

## A Toast

@mira: To {{@to}}!
@oskar: To {{@to}}.
```

`id: harbor.wake`인 씬은 이를 가져와 두 번 사용합니다:

```lute
::use{component="cheers" to="the harbor"}
@oskar: Again?
::use{component="cheers" to="absent friends"}
```

각 확장의 줄은 각자의 [컴포넌트 범위](#줄-식별) `lineId` 아래에 자체 텍스트를 전달합니다.

| `lineId` | 텍스트 |
|---|---|
| `harbor.wake.cheers#1.mira_0010` | To the harbor! |
| `harbor.wake.cheers#1.oskar_0010` | To the harbor. |
| `harbor.wake.cheers#2.mira_0010` | To absent friends! |
| `harbor.wake.cheers#2.oskar_0010` | To absent friends. |

따라서 번역자나 성우는 빈칸이 있는 템플릿이 아니라 `To absent friends!`를 독립적인 한 줄로 받습니다.
인수는 리터럴이어야 합니다. 호출자의 문자열 `@def`는 엔진이 평가할 때에야 알 수 있고, `{{…}}` 보간은 런타임에
`int`, `double`, bool, enum 값만 렌더링하므로 `::use{component="cheers" to=@place}`는 인수에서
`E-REF-TYPE`입니다. `check`는 중첩 컴포넌트를 통과하는 매개변수도 추적하므로, 자신의 `@to`를 `cheers`에
넘기는 컴포넌트도 호출자로부터 리터럴을 받아야 합니다.

리터럴 인수 자체에는 `{{…}}` 보간이 들어 있을 수 있습니다(dsl 0.26.0 §3.1). 확장 후에도 호스트에 작성한 줄과
정확히 같은 플레이스홀더 기록을 유지합니다. `::use{component="cheers" to="you, {{userName}}"}`는
`"placeholders": [{"kind": "reserved", "token": "userName"}]`와 함께 `To you, {{userName}}!` 텍스트로
컴파일되므로 엔진이 플레이어 이름을 렌더링합니다. 0.26.0 이전에는 플레이스홀더 기록이 삭제되어 중괄호가 리터럴
텍스트로 전달되었습니다.

### 컴포넌트 본문 규칙

컴포넌트 본문은 **프레젠테이션용**입니다. 줄, 연출 디렉티브, `@param` ref만 포함할 수 있습니다. 본문은
씬/런 상태를 **읽을 수 없고**, 파일이 [`effects: true`](#상태를-쓰는-컴포넌트-effects-true)를 선언하지
않는 한 상태를 **쓸 수 없으며**, 논리 블록을 포함할 수 없습니다(`E-COMPONENT-BODY`) — 대신 매개변수로 값을
전달합니다(위에서 설명한 호출자의 def는 합법적인 인수입니다). def도 예외가 아닙니다.
본문의 `@narrator{when="@late"}`는 `@late`에서 `E-COMPONENT-STATE`가 되며, 컴포넌트 자체 검사에서 한 번
보고되고, 메시지는 값을 전달하는 매개변수 `late: { type: bool, default: "@late" }`를 이름으로 표시합니다.
이를 선언해도 본문은 달라지지 않으며 각 `::use`는 호스트의 `@late`를 기본값으로 사용합니다. 한 가지 주목할
예외는 컴포넌트 자체 매개변수에 대해 분기하는 `<match>`입니다. 이는 주변 상태가 아니라 호출 인수를 순수하게
읽는 것이므로 허용됩니다:

```lute check="docs/examples/components/reaction.component.lute"
---
component: reaction
params:
  tier: { enum: [cold, warm, fond] }
uses: ../base.schema.yaml
---

## The Tiered Greeting

<match subject="@tier">
  <when is="fond">
    @marina{emotion="delighted"}: You remembered!
  </when>
  <when is="warm">
    @marina{emotion="content"}: Not bad at all, Mr. Fixer.
  </when>
  <when is="cold">
    @marina{emotion="neutral"}: ...Shall we begin?
  </when>
</match>
```

*([`docs/examples/components/reaction.component.lute`](https://github.com/journeyWorker/lute/blob/main/docs/examples/components/reaction.component.lute)에서 가져왔습니다.)* 세 갈래가 선언된 enum을 모두
포괄하므로 매개변수가 `unset`인 경우가 없고 `<otherwise>`도 필요하지 않습니다.

두 번째 예외(dsl 0.26.0 §3.3)는 본문 자체의 플러그인 디렉티브 결과입니다. `::battle{… resultKey="fight"}`를
호출하는 컴포넌트는 `scene.battle.fight.won`을 읽을 수 있습니다. [컴포넌트 자체 디렉티브 결과](#컴포넌트-자체-디렉티브-결과)를
참고하세요.

### 상태를 쓰는 컴포넌트: `effects: true`

재사용 가능한 콘텐츠 중에는 결과가 따르는 것도 있습니다. 동료의 끄덕임이 호감도를 올리거나, 수색이 찾은 사실을
확인하는 경우입니다. **`effects: true`**(dsl 0.24.0 §4)를 선언하는 컴포넌트 파일은 `::set`, `::assert`,
`::retract`, 상태를 쓰는 플러그인 디렉티브, 그리고 다른 effects 컴포넌트의 `::use`를 사용할 수 있습니다.

```lute check
---
component: nod
effects: true
params:
  who: speaker
  delta: int
state:
  run.approval: { type: int, default: 0, per: companion }
entities:
  companion: { members: [isolde, corvin] }
---

## A Nod

@narrator: {{@who}} nods.
<match subject="@who">
  <when is="isolde">
    ::set{run.approval.isolde += @delta}
  </when>
  <when is="corvin">
    ::set{run.approval.corvin += @delta}
  </when>
  <otherwise>
    @narrator: Nobody else minds.
  </otherwise>
</match>
```

(위의 `state:`와 `entities:`는 파일 자체를 검사할 수 있게 합니다. 프로젝트에서는 컴포넌트가 공유 스키마를
`uses:`로 가져오고, 실제로 적용되는 선언은 호스트의 것입니다.)

쓰기는 **호스트**에 속합니다. 각 쓰기는 모든 `::use`에서 호스트 문서의 스키마에 맞춰 검사됩니다 — 선언되지 않은
경로, 잘못된 타입의 `::set`, `owner: engine` 경로, 관계의 어휘와 도메인, 권한 등이 검사되며 오류는 `::use`에서
보고됩니다. `run.approval`을 선언하지 않은 스키마를 가진 호스트는 다음을 얻습니다.

<!-- lute-diagnostics -->
```
./scenes/market.lute:12:1: error [E-UNDECLARED] `::set` target `run.approval.isolde` is not declared in the `state:` schema
```

쓰기는 `::use`가 위치한 호스트 명령으로 컴파일되고, 매개변수에 대한 `<match>`는 인수가 선택하는 갈래만
남깁니다. 캐스트에서 `corvin`을 "Corvin Hale"로 이름 붙인 `camp.fire` 씬에서
`::use{component="nod" who="corvin" delta="2"}`는 다음으로 컴파일됩니다.

```json
{"kind": "line", "family": "content", "position": "001-0100", "role": "narration", "speaker": "narrator", "text": "Corvin Hale nods.", "lineId": "camp.fire.nod#1.narrator_0010", "voiceKey": "camp.fire.nod#1.narrator-0010", "source": {"component": "nod"}}
{"kind": "set", "family": "state", "position": "001-0200", "path": "run.approval.corvin", "op": "+=", "value": {"cel": "2", "expr": {"int": 2}}, "source": {"component": "nod"}}
```

본문의 팩트 원자는 `::assert{gifted(@who, @item)}` 또는 동일한 형태의 `::retract`처럼 매개변수를 인수로 받을 수
있습니다. 각 `::use`가 매개변수를 인수에 바인딩하고, 호스트는 자신이 직접 작성한 원자와 마찬가지로 그 위치에서
바인딩된 원자를 검사합니다. `item: string`과 `[locket, map]`인 `item` 종류가 있을 때
`::use{component="gift" who="isolde" item="locket"}`는 `gifted(isolde, locket)`의 `assert`로 컴파일되며,
`item="lockt"`은 `::use`에서 `E-FACT-DOMAIN`입니다. `check-project`의 팩트 분석도 바인딩된 원자를 보므로,
이후 `holds('gifted', ['isolde', 'locket'])`는 보장된 가드가 되고, 어떤 `::use`도 생성할 수 없는 질의는
도달 불가 상태가 됩니다(`E-ARM-DEAD`). 팩트의 인수는 변수가 없는 값이므로 인수는 상수여야 합니다. 엔티티 또는 enum 멤버 id,
`true` 또는 `false`여야 합니다. def 인수 `item=@best`는 `E-COMPONENT-ARG`이며 메시지에는 원자가 표시됩니다.
컴포넌트 본문 밖에서 팩트의 `@param`은 `E-FACT-DOMAIN`입니다.

`effects: true`가 해제하는 것은 쓰기뿐입니다. 본문의 가드나 match subject는 여전히 상태를 읽을 수 없습니다
(`E-COMPONENT-STATE`). 단, 자체 플러그인 디렉티브의 결과는 예외입니다([아래](#컴포넌트-자체-디렉티브-결과) 참고).
effects 컴포넌트를 `::use`하는 프레젠테이션 컴포넌트는 `E-COMPONENT-BODY`이고, 메시지는 그 컴포넌트에도
`effects: true`를 요구합니다. `effects: true`가 없으면 쓰기는 이전과 동일하게 `E-COMPONENT-BODY`입니다.

### 화자 매개변수

**`speaker`** 타입 매개변수(dsl 0.24.0 §4)는 위 `nod`의 `who`처럼 캐스트 id를 받습니다.

- 줄의 `{{@who}}`는 [캐스트](/ko/language/dialogue-and-cast/#캐스트)의 멤버 표시 `name`을 렌더링하며, 멤버에 이름이
  없으면 id를 렌더링합니다. 따라서 `who="corvin"`은 `Corvin Hale nods.`를 렌더링합니다.
- 줄 속성 `as=@who`도 표시 이름을 렌더링하며, 따옴표로 묶은 줄 속성 안의 `{{@who}}`도 마찬가지입니다
  (`as="{{@who}}, again"`은 `"as": "Corvin Hale, again"`으로 컴파일됩니다)(dsl 0.26.0 §3.1).
  0.26.0 이전에는 `as=@who`가 id를 전달했고 속성 문자열 안의 중괄호도 리터럴로 남았습니다.
- 그 밖의 속성(`::actor{character=@who}`, 플러그인 디렉티브의 `foe=@who`)과 `<match subject="@who">`는 id를
  봅니다. match의 범위는 선언된 캐스트와 `narrator`이므로, 캐스트 멤버 이름을 딴 갈래도 검사됩니다. 오타가 있는
  갈래는 `E-WHEN-LITERAL-DOMAIN`이고, 모든 멤버도 `<otherwise>`도 없는 match는 `E-NONEXHAUSTIVE`입니다
  (`narrator`도 멤버로 셉니다).
- 캐스트 밖의 인수는 `::use`에서 did-you-mean과 함께 `E-CAST-UNKNOWN`입니다(`who="corvn"`에는 `corvin`을
  제안합니다). 선언된 캐스트가 없으면 어떤 식별자든 허용됩니다.
- 인수는 리터럴이어야 합니다. `who=@lead`와 같은 def 인수는 이름이 런타임이 아니라 컴포넌트 확장 시 선택되므로
  `E-COMPONENT-ARG`입니다.
- 컴포넌트는 자신의 speaker를 넘길 수 있습니다(`::use{component="nod" who=@who}`). 바깥 `::use`가 제공하는
  id가 안쪽에도 그대로 전달되고, 호스트는 바인딩하는 위치에서 이를 판정합니다(캐스트 멤버 여부와 안쪽 본문이
  쓰는 `per:` family의 멤버 여부).
- 본문의 `mono` 줄은 각 `::use`에서 **호출자의** 시점으로 판정됩니다. 즉 해석된 화자는 호스트의 유효한 `pov`
  이거나 `monoSpeakers`에 나열되어야 합니다([프런트매터와 프로필](/ko/language/frontmatter-and-profiles/#선택-키) 참고).
  각 `::use`는 자체적으로 검사되므로 동일한 컴포넌트도 한 호스트에서는 통과하고 다른 호스트에서는 실패할 수 있습니다.
### 매개변수로 말하기: `@@who:`

컴포넌트 본문은 `speaker` 매개변수가 이름 붙인 멤버 **화자로** 말할 수 있습니다(dsl 0.26.0 §3.2). `@@who:`는
각 `::use`에서 `who`의 인수가 지정하는 화자가 되는 콘텐츠 줄입니다.

```lute check
---
component: greeter
params:
  who: speaker
  line: string
  times: { type: int, default: 1 }
---

## Greeter

@@who: {{@line}}
@narrator{as=@who}: ({{@times}} times today.)
```

`harbor.gate` 씬에서 `::use{component="greeter" who="mira" line="Morning."}`를 사용하면 첫 줄은
`"speaker": "mira"`와 함께 컴파일되고, `"lineId": "harbor.gate.greeter#1.mira_0010"`을 가집니다. 즉
호스트가 `@mira: Morning.`이라고 쓴 것처럼 멤버 자신의 초상화, 음성, 줄 식별을 사용합니다. 멤버의 캐스트 검사는
`::use`에서 판정됩니다. `emotions:` 밖의 `emotion=`은 `E-BAD-ENUM`이고, `present:`가 포괄하지 않는 줄은
`W-CAST-ABSENT`(멤버 및 가드마다 한 번)이며, 멤버가 무대를 떠난 뒤의 줄은 `W-STAGE-ABSENT`입니다.

`mono` 줄도 같은 규칙을 따릅니다. 컴포넌트 `aside`의 `@@who{mono}: Not again.`은 유효한 POV가 `fixer`인
호스트에서 `::use{component="aside" who="fixer"}`로 사용하면 통과하지만, 같은 호스트의
`::use{component="aside" who="corvin"}`은 호스트가 `corvin`을 `monoSpeakers:`에 나열하지 않는 한
`E-MONO-POV`입니다. 오류는 해당 `::use`에서 보고되고 컴포넌트 파일과 줄을 이름으로 표시하며, 컴포넌트 줄도
관련 위치로 함께 표시합니다.

매개변수가 `speaker` 매개변수가 아니거나, 어떤 매개변수에도 없는 이름이거나, 컴포넌트가 아닌 문서에서
`@@x`를 쓰면 `E-COMPONENT-ARG`입니다.

<!-- lute-diagnostics unverified="copied verbatim from lute check output; since the 0.27 module split the message literals live in crates/lute-check/src/check/component_body.rs, which names the code through the E_COMPONENT_ARG constant rather than a string literal, so the scraper cannot pair quote and code" -->
```
./components/challenge.component.lute:11:1: error [E-COMPONENT-ARG] `@@taunt:` speaks as the cast member a `speaker` param names, and `taunt` is not a `speaker` param — declare `taunt: speaker`
./scenes/pier.lute:9:1: error [E-COMPONENT-ARG] `@@mira:` speaks as a component's `speaker` param `mira`, and this document is no component — write the cast id (`@mira:`)
```

### 매개변수 기본값

매개변수는 **`default:`**(dsl 0.26.0 §3.3)를 긴 형식 `name: { type: <type>, default: <value> }`으로 선언할
수 있으며, 위 `greeter`의 `times`가 그 예입니다. 인수를 생략한 `::use`는 기본값을 사용하고, 기본값은 그 인수가
그대로 서 있는 것처럼 `::use`에서 판정됩니다. 값은 리터럴 또는 **호스트**가 해석하는 `@def`입니다. YAML에서
`@`로 시작하는 일반 값은 허용되지 않으므로 def를 따옴표로 감쌉니다.

```yaml
params:
  kind: { type: { enum: [rival, champion] }, default: rival }
  won:  { type: bool, default: "@wonFight" }
```

`name: <type>`도 긴 형식도 아닌 `params:` 항목은 `E-COMPONENT-PARSE`이며, 메시지에 두 형식을 모두 표시합니다.

### 컴포넌트 자체 디렉티브 결과

결과 슬롯을 선언하는 플러그인 디렉티브(예: `scene.battle.<resultKey>.*`에 쓰는 브리지 `::battle`)는 컴포넌트
본문에 놓을 수 있고, 본문은 자신이 선언한 슬롯을 읽을 수 있습니다(dsl 0.26.0 §3.3). 다음은 몬스터 수집 게임의
트레이너 배틀로, 모든 경로의 트레이너가 하나의 `::use`입니다.

```lute unverified="needs the engine plugin that declares the ::battle bridge directive and the cast; checked with lute check-project on a scratch project that declares both"
---
component: trainerBattle
effects: true
params:
  who: speaker
  intro: string
  win: string
  lose: string
  prize: { type: int, default: 100 }
---

## Trainer battle

@@who: {{@intro}}
::battle{foe=@who resultKey="fight"}
<match subject="scene.battle.fight.won">
  <when is="true">
    @@who: {{@win}}
    ::assert{defeated(@who)}
    ::set{run.money += @prize}
    @narrator: {{userName}} got {{@prize}} coins for winning!
  </when>
  <otherwise>
    @@who: {{@lose}}
  </otherwise>
</match>
```

`scene.battle.fight.won`은 주변 상태가 아닙니다. 본문의 자체 `::battle`이 선언하는 슬롯이므로 `<match>`, 가드,
`{{scene.battle.fight.turns}}` 보간이 이를 읽을 수 있습니다. 그 밖의 `scene.*` 또는 `run.*` 읽기는 여전히
`E-COMPONENT-STATE`입니다.

각 `::use`도 그 슬롯을 호스트에 선언하며, 해당 슬롯은 use의 인수에 바인딩됩니다(dsl 0.26.0 §3.1). 호스트의
검사, 컴파일된 `state` 표(`scene.battle.fight.won`, `scene.battle.fight.turns`), trace mock, `lute play`가 모두
이를 확인하며 호스트가 자체적으로 선언할 것은 없습니다. `lute play`는 브리지 응답을 해당 슬롯의 타입으로, 또는
capability의 `result:` 형태로 타입 지정하고, 문자열로 저장하는 대신 타입을 지정할 수 없는 응답을 거부합니다.
0.26.0 이전에는 호스트가 스키마에 슬롯을 선언해야 했고, 컴포넌트는 결과를 매개변수(`won=@wonFight`)로 받아야
했으며 슬롯을 직접 읽을 수 없었습니다.

### `::use` 가드하기: `when=`

`::use`는 [`::set`](/ko/language/directives/#디렉티브-가드하기-when)과 마찬가지로 `when="<condition>"`을
받습니다(dsl 0.26.0 §4). 전체 확장이 실행되거나, 아무것도 실행되지 않습니다. 줄, `::battle`, 쓰기 모두가
같이 적용됩니다. use의 인수 읽기는 해당 가드 아래에서 판정됩니다. 던전 층은 한 줄로 이미 쓰러진 트레이너를
건너뜁니다.

```lute
::use{component="trainerBattle" who="lassMina" intro="You look strong, {{userName}}." win="Oh no!" lose="Hehe." prize="200" when="!holds('defeated', ['lassMina'])"}
```

`lute play`는 인수와 함께 건너뛴 use를 출력합니다. `skip ::use{component="trainerBattle"
who="lassMina" …} — when: false`와 같은 형태이며, trace와 test도 같은 방식으로 건너뜁니다.

### 매개변수를 통한 엔진 id

플러그인은 프로젝트 엔티티 종류에 따라 디렉티브 속성의 타입을 지정할 수 있습니다. 예를 들어
`::give{item}`의 타입은 `{ entity: bagItem }`일 수 있습니다([엔티티 종류로 타입 지정하는 엔진 id](/ko/plugins/manifests/#엔티티-종류로-형식이-지정된-엔진-id) 참고).
검사는 컴포넌트를 통과해 이어집니다(dsl 0.26.0 §2.5). 본문이 이런 속성에 매개변수 전체를 전달하거나
(`::give{item=@item}`, 중첩된 `::use`를 통하는 경우도 포함), 또는 매개변수 자체가 `{ entity: bagItem }`나
`{ domain: … }` 타입이면 각 `::use` 인수 또는 매개변수의 `default:`를 종류에 맞춰 검사하고 유사 항목을
제안합니다.

<!-- lute-diagnostics -->
```
./scenes/route2.lute:9:30: error [E-BAD-ENUM] `superPotoin` is not a member of entity kind `bagItem` (attribute `item` of `::give` in component `gift`) — did you mean `superPotion`?
```

[`lute refs <dir> --attr give.item`](/ko/tooling/cli/)은 해당 값을 바인딩하는 `::use` 줄에 표시하며
``(via component `gift`)``로 표시합니다.

이런 매개변수도 텍스트로 렌더링됩니다(dsl 0.27.0 §7). `hall: { domain: room }`에 대한 `{{@hall}}`은 인수에
대한 종류의 `labels:` 항목을 보여 줍니다(`chapel`이면 `the chapel`). 캐스트 id에는 캐스트 멤버의 `name:`이
우선하고, 종류에 해당 라벨이 없으면 id 자체를 표시합니다. 인수는 `string` 매개변수와 마찬가지로 리터럴이어야 합니다.

### 비트 헤더: `beat:`

프런트매터에 `beat:` 헤더(`on`, `when`, `once`, `after` …, 값에는 `@param`을 자유롭게 사용)를 함께 선언한
컴포넌트는 **비트 템플릿**입니다(dsl 0.27.0 §6). 번들 비트가 `<beat use="<component>" id="…" …>`로 이를
이름 붙이고 매개변수를 속성으로 전달하면, 각 `@param`이 치환된 헤더를 얻습니다. 본문에는 번들 비트의 본문과 마찬가지로
`## ` 헤딩이 필요 없습니다. 헤더 규칙, `::body`, `E-TEMPLATE` 오용은 [비트 템플릿](/ko/language/beats/#비트-템플릿-use)을
참고하세요.

### 줄 식별

각 `::use` 확장은 고유한 식별 범위입니다. 컴포넌트에서 확장된 줄은
`{prefix}.{component}#{instance}.{speaker}_{code}`로 주소를 지정합니다. 여기서 `instance`는 형제 use의 개수가
아니라 작성자가 지정한 `instance="…"` 토큰입니다. 키가 없으면 `lute tag`가 기록할 때까지 경고를 동반한
대체값을 사용합니다. 음성이 있는 줄의 기본 `voiceKey`도 같은 범위를 사용합니다(`identity:` 템플릿은
[프런트매터와 프로필](/ko/language/frontmatter-and-profiles/) 참고).

```lute check
---
component: toast
---
## A Toast
@mira{code="0010"}: To the harbor.
```

태그가 있는 줄 하나와 태그가 없는 줄 하나가 있는 컴포넌트를 보겠습니다.

```lute check
---
component: toast
---

## A Toast

@mira{code="0010"}: To the harbor.
@oskar: To the harbor.
```

`id: harbor.dinner`인 씬은 이를 가져오고(`components: [toast.component.lute]`) 자신의 줄 사이에서 두 번 사용합니다.

```lute
@mira{code="0010"}: Sit, everyone.
::use{component="toast"}
@oskar: Again?
::use{component="toast"}
@mira: Last one.
```

모든 줄은 각자의 `lineId`를 가집니다.

| 줄 | `lineId` |
|---|---|
| `@mira{code="0010"}: Sit, everyone.` | `harbor.dinner.mira_0010` |
| 첫 번째 `toast` | `harbor.dinner.toast#1.mira_0010`, `harbor.dinner.toast#1.oskar_0010` |
| `@oskar: Again?` | `harbor.dinner.oskar_0010` |
| 두 번째 `toast` | `harbor.dinner.toast#2.mira_0010`, `harbor.dinner.toast#2.oskar_0010` |
| `@mira: Last one.` | `harbor.dinner.mira_0020` |

- **각 확장은 자체의 태그 없는 코드를 나중에 채웁니다.** 컴포넌트의 `@oskar`는 호스트 줄이 앞에 몇 줄이나
  있든 모든 use에서 `0010`입니다. 호스트 자신의 태그 없는 줄은 `lute tag`가 기록한 코드를 유지합니다
  (`@oskar: Again?` → `0010`, `@mira: Last one.` → `0020`).
- **경계를 넘어 코드가 더는 충돌하지 않습니다.** 태그된 컴포넌트를 두 번 사용하거나, 컴포넌트 줄이 호스트 줄과
  코드를 공유해도(위에서 두 `@mira` 줄이 모두 `0010`) 서로 다른 id로 깨끗하게 컴파일됩니다. 0.22.0 이전에는
  둘 다 `E-DUP-LINE-CODE`였습니다.
- **중첩된 `::use`는 세그먼트를 하나 추가합니다.** 둘씩 사용된 `feast` 컴포넌트가 `toast`를 사용하면
  `…feast#1.toast#1.mira_0010`과 `…feast#2.toast#1.mira_0010`이 됩니다.
- **`lute loc export`도 동일한 id를 출력합니다.**

0.21에서 마이그레이션할 때: 컴포넌트가 기여하는 모든 줄에 새로운 `lineId`와 `voiceKey`가 생기므로,
`::use`하는 각 문서의 현지화 및 음성 매니페스트를 다시 내보내세요.
## 스키마 `extends:`

`uses:`가 **동료** 스키마를 합치고(두 동료가 같은 이름을 선언하면 오류), **`extends:`**는 문서가
**정제하는** 하나 이상의 **기본** 스키마를 지정합니다. 기본 스키마는 우선순위가 더 낮은 레이어입니다.

```yaml
# base.schema.yaml
state:
  run.blessed: { type: bool, default: false }
defs:
  wealthy: { type: bool, cel: "run.blessed" }
```

```yaml
# child.schema.yaml
extends: base.schema.yaml
state:
  run.blessed: { type: bool, default: true }   # overrides the base default
```

*([`docs/examples/child.schema.yaml`](https://github.com/journeyWorker/lute/blob/main/docs/examples/child.schema.yaml)와 [`base.schema.yaml`](https://github.com/journeyWorker/lute/blob/main/docs/examples/base.schema.yaml)에서 가져왔습니다.)*

우선순위는 낮은 순서에서 높은 순서로 다음과 같습니다. 문서의 `extends` 기본 스키마(재귀적) < 문서의
`uses` 동료 < 문서 자체의 인라인 `state:`/`defs:`. 확장 레이어가 기본 스키마의 이름을 다시 선언하면
그 이름을 **오버라이드**하며 중복 오류가 발생하지 않습니다. `defs` 항목은 통째로 교체됩니다. `state` 항목도
오버라이드되지만, 영속 상태는 안정적인 타입을 유지해야 하므로 선언된 **타입**을 바꾸는 오버라이드는
`E-EXTENDS-STATE-TYPE`입니다. 타입이 같은 `default`만의 정제는 조용히 허용됩니다. `extends` 간선은
`uses:`와 동일한 DAG 규율을 공유하므로, 순환·누락된 파일·파싱 오류는 `E-USES-*` 진단을 재사용합니다.
