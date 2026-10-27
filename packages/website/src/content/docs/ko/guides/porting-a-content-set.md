---
title: 콘텐츠 세트 이식하기
description: "스프레드시트나 다른 엔진의 내보내기처럼 이미 존재하는 대규모 콘텐츠 세트를 임포터로 Lute에 옮기는 방법: 표면별 스탬프 열, 호스트 토큰, 보상 효과, 임포트된 하위 트리의 프로젝트 진단 수준, 화자 무대 등장, 구조 및 종속 속성 규칙을 위한 node·pair lint, 한도 있는 재시도 루프, 디렉티브나 보상이 raise하는 장면, 번역할 수 있는 디렉티브 텍스트, 구조화된 값의 현재 한계를 설명합니다."
---

이 가이드는 콘텐츠 세트가 이미 다른 형태(스프레드시트 행, 엔진의 내보내기, 이전 스크립트 형식)로 존재하고,
`.lute` 문서를 만들어 내는 **임포터**를 작성하는 팀을 위한 것입니다. 이 경우 작성자는 임포터입니다. 시트가 어떻게
생겼는지를 Lute가 그대로 받아들여야 하는 것은 아닙니다. 시트의 열, 토큰, 보상이 무엇을 뜻하는지는 플러그인이
선언하고, `lute check`가 임포트된 모든 행을 그 선언에 맞춰 검사합니다.

여기의 모든 내용은 0.50부터 사용할 수 있습니다. 이 레시피들은 실제 대규모 이식에서 부딪힌 일곱 가지 문제(이슈 #162,
#163, #164, #166, #169, #172, #177)에 대한 답이며, 각 절은 도구가 무엇을 잡아내는지로 끝납니다. 이 모두를 사용하는
실행 가능한 프로젝트는
[`docs/examples/plugin-surfaces/`](https://github.com/journeyWorker/lute/tree/main/docs/examples/plugin-surfaces)입니다.

```sh
lute check --project docs/examples/plugin-surfaces
lute test  --project docs/examples/plugin-surfaces
lute lint  --project docs/examples/plugin-surfaces
```

플러그인 매니페스트 어휘는 [플러그인 매니페스트](/ko/plugins/manifests/)에 있습니다. 이 페이지는 그 주변의 이식
레시피를 모았습니다.

0.51부터 이 가이드는 이슈 #159, #165, #170, #172, #176이 보고한 문제도 다룹니다.
[재시도 루프](#한도-있는-jump로-만드는-재시도-루프), [디렉티브나 보상이 raise하는 장면](#디렉티브나-보상이-raise하는-장면),
[플레이어가 읽는 디렉티브 텍스트](#플레이어가-읽는-디렉티브-텍스트), [균형 규칙](#pair로-쓰는-균형-규칙),
[구조화된 속성 값](#구조화된-값은-아직-지원하지-않습니다)입니다. 이들을 실행해 볼 수 있는 프로젝트는
[`docs/examples/flow-and-text/`](https://github.com/journeyWorker/lute/tree/main/docs/examples/flow-and-text)입니다.

## 열은 자기 표면의 스탬프가 됩니다

시트에는 행의 종류마다 다른 열이 있습니다. 대사 행에는 음성 에셋, 배경 행에는 에셋 이름, 선택지 행에는 메뉴 종류,
각 옵션에는 옵션 키가 있습니다. **스탬프**는 디렉티브 자체 선언에 속하지 않는 플러그인 속성입니다. 0.49부터 스탬프는
모든 디렉티브와 모든 콘텐츠 줄에 허용되는데, 한 종류의 행에만 속하는 열에는 너무 넓은 범위입니다. 0.50부터
`stampAttrs` 항목은 `on:`으로 자신의 표면을 지정합니다.

```yaml
# plugins/acme.stage/stampattrs/columns.yaml
stampAttrs:
  - { name: voiceAssetId, type: { providerRef: voice }, required: true, on: [line] }
  - { name: assetName,    type: string, on: [bg, actor] }
  - { name: menuType,     type: { enum: [plain, timed, minigame] }, on: [branch, hub] }
  - { name: optionKey,    type: string, required: true, on: [choice] }
  - { name: cue,          type: string }                          # cross-cutting, as in 0.49
```

```lute
<branch id="route" prompt="Which way?" menuType="plain">
  <choice id="goLeft" text="Left" optionKey="L1">
    @mira{voiceAssetId="VO.mira.0010"}: Left it is.
  </choice>
</branch>
```

`on:`은 **표면 이름**의 비어 있지 않은 목록을 받습니다. 어휘는 닫혀 있습니다.

| 표면 이름 | 스탬프를 쓸 수 있는 곳 |
|---|---|
| `line` | 모든 콘텐츠 줄(`@speaker{…}: text`). 컴포넌트, 퀘스트, 목표, `<on>`, 엔트리, 비트 본문의 줄을 포함합니다 |
| `branch`, `hub` | 그 이름의 요소 |
| `choice` | `<branch>`나 `<hub>` 안의 모든 `<choice>` |
| 디렉티브 태그 | 활성 프로필의 디렉티브이면서 스탬프 운반체가 있는 그 디렉티브: `bg`, `actor`, 플러그인 디렉티브. 스탬프가 붙는 레코드를 내지 않는 `jump`, `label`, `clear`는 제외됩니다 |

`line`, `branch`, `hub`, `choice` 네 이름은 언제나 코어 표면을 뜻하며, 이미 플러그인 디렉티브 이름으로 예약되어
있으므로 `line`이라는 태그의 플러그인 디렉티브는 `on:`에 쓸 수 없습니다. `on:` 안의 순서는 중요하지 않습니다.

### 열이 할 수 있는 것과 없는 것

- **`on:`이 없으면** 스탬프는 0.49의 의미를 그대로 유지합니다. 스탬프 운반체가 있는 모든 디렉티브와 모든 콘텐츠
  줄입니다. `<branch>`, `<hub>`, `<choice>`에는 허용되지 **않습니다**. 로직 태그는 스탬프가 이름을 지목하지 않는 한
  닫혀 있으므로, `branch`, `hub`, `choice`는 횡단 스탬프에 허용되지 않습니다.
- **범위가 지정된 스탬프는 다른 곳에서 거부됩니다.** `::bg`에 `voiceAssetId`를 쓰면 `E-UNKNOWN-ATTR`이며, 메시지는
  그 표면의 자체 속성을 나열하는 대신 스탬프가 적용되는 곳을 알려 줍니다.
- **`required: true`에는 `on:`이 필요합니다.** 횡단 스탬프는 필수로 지정할 수 없습니다. 모든 레코드에 필수인 스탬프는
  열이 아니기 때문입니다. 0.50 이전에는 이 플래그가 조용히 무시되었지만, 이제는 로드 시 `E-PLUGIN-PARSE`입니다. 범위가
  지정된 스탬프에서는 강제됩니다. 스탬프를 쓰지 않은 표면은 디렉티브, 줄, 또는 `<branch>`/`<hub>`/`<choice>` 요소
  전체에서 `E-MISSING-ATTR`입니다. 이 규칙은 표면이 작성되는 모든 곳에 적용되며 컴포넌트 본문도 포함합니다. `::use`가 끼워
  넣는 줄은 해당 컴포넌트 안에서 한 번만 검사됩니다. 스탬프는 주입되지 않으므로 선언된 `default:`는 이 조건을
  만족시키지 못합니다.
- **한 이름이 두 타입을 가질 수 있습니다.** 항목들의 표면 집합이 서로소이기만 하면 됩니다. `assetName`은 `bg`에서는
  `string`, `actor`에서는 `int`일 수 있습니다. 각 표면은 자신의 타입으로 값을 직렬화합니다. 겹치는 경우는 기존 중복
  코드입니다. 한 플러그인 패키지 안에서는 `E-PLUGIN-DUP-ID`, 두 플러그인 사이에서는 `E-PLUGIN-DUP-ACROSS`이며, 둘 다
  공유하는 표면을 이름으로 알려 줍니다. `on:`이 없는 항목은 같은 이름의 다른 모든 항목과 겹칩니다.
- **범위가 지정된 스탬프는 표면의 자체 속성을 가릴 수 없습니다.** `branch`의 `prompt`, `choice`의 `text`, `line`의
  `code`는 읽을 수 없으므로 그 선언은 `E-PLUGIN-RESERVED-STAMP-ATTR`입니다. 표면의 자체 속성은 디렉티브가 선언한
  속성, 줄의 알려진 속성과 `when`, `<branch>`의 `id`, `prompt`, `timeout`, `default`, `<hub>`의 `id`, `prompt`,
  그리고 `<choice>`의 `id`, `text`, `when`, `into`, `value`, `once`, `exit`와 폐기된 `label`, `as`, `persist`입니다.
- **표면이 아닌 `on:` 이름**은 어셈블리 시 `E-PLUGIN-STAMP-SURFACE`이며 did-you-mean이 붙습니다(`bg` 대신 쓴 `bgg`).
  다른 플러그인의 디렉티브는 그 플러그인이 여러분의 `depends:`에 있을 때에만 표면이 되며, 그래야 여러분의 플러그인을
  활성화하는 모든 프로필에 그 디렉티브가 있습니다.

로드 시 오류는 모두 항목에서 보고되는 `E-PLUGIN-PARSE`입니다. 문자열 목록이 아니거나 비어 있는 `on:`, `on:`에서 반복된
표면, `on:` 없는 `required: true`입니다.

값은 그것이 쓰인 표면의 선언으로 타입이 지정되므로, 잘못된 값은 일반적인 `E-ATTR-TYPE`이나 `E-BAD-ENUM`입니다.
`<branch>`, `<hub>`, `<choice>`의 `@ref` 값도 마찬가지입니다. 타입이 잘못된 def는 `E-REF-TYPE`이고, 열거형 밖으로
접히는 def는 `E-BAD-ENUM`입니다.

[예제 프로젝트](https://github.com/journeyWorker/lute/tree/main/docs/examples/plugin-surfaces)에서 `::bg`에
`voiceAssetId`를 쓰고(8행) `optionKey` 없는 `<choice id="goLeft">`를 둔(10행) 장면은 다음을 보고합니다.

<!-- lute-diagnostics unverified="real lute check output (0.50 build); it matches both the scoped-stamp and the generic `has no attribute` format literals, so the matcher reports it ambiguous" -->
```
./scenes/a.lute:8:33: error [E-UNKNOWN-ATTR] `::bg` has no attribute `voiceAssetId`: it is a stamp of `acme.stage` for content lines only
./scenes/a.lute:10:3: error [E-MISSING-ATTR] `<choice id="goLeft">` requires attribute `optionKey` (a stamp of `acme.stage`)
```

### 아티팩트에 도달하는 것

런타임은 스탬프를 읽지 않습니다. 레코드의 스탬프는 0.49와 같이 그 레코드의 `stamps` 객체에 들어갑니다.
`<branch>`나 `<hub>`의 스탬프는 `choice`나 `hub` 레코드의 `stamps`에 들어가고, 각 옵션은 자신의 `<choice>` 요소에서
온 `stamps` 객체를 따로 얻습니다(없으면 생략). 작성되지 않은 스탬프는 주입되지 않습니다. 런타임이 대기하는 메뉴는
주소와 판정만 담고 있으므로, 레퍼런스 플레이어를 쓰지 않는 호스트는 옵션 텍스트를 읽는 것과 같이 메뉴 주소의 IR
레코드에서 스탬프를 읽습니다. 레퍼런스 플레이어의 `ChoiceView`는 요소와 각 옵션의 `stamps`를 담습니다.
[런타임 계약](/ko/tooling/runtime-contract/)을 참고하세요.

## `{{playerName}}`: 호스트가 대체하는 토큰

임포터는 엔진이 렌더링 시점에 채우는 이름을 만납니다. 플레이어 이름, 등급, 플랫폼 같은 것들입니다. 각각을
`hostTokens` export(`exports: { hostTokens: hosttokens/ }`)로 선언합니다.

```yaml
# plugins/acme.stage/hosttokens/tokens.yaml
hostTokens:
  playerName: { type: string }
  rank:       { type: int }
  platform:   { type: { enum: [ios, android, web] } }
```

```lute
@mira: Welcome, {{playerName}}. You are our {{rank:ordinal}} guest, and {{platform:capitalize}} is fine.
```

- 토큰 이름은 **식별자**입니다(ASCII 문자나 `_`로 시작하고, 이후 문자, 숫자, `_`). 토큰은 `{{…}}` 안에 그대로
  쓰이므로 `player-name`과 `2ndName`은 로드 시 `E-PLUGIN-PARSE`입니다. 값은 `string`, `int`, `{ enum: [...] }`
  중 하나의 `{ type }`입니다.
- 상태 루트나 CEL 리터럴로 읽히는 이름(`scene`, `run`, `user`, `app`, `quest`, `entry`, `prev`, `clock`,
  `occasion`, `season`, `true`, `false`, `null`)은 `E-PLUGIN-RESERVED-NAME`입니다. `{{run}}`은 상태 루트로 읽히기
  때문입니다.
- `lute.core`는 `userName: { type: string }`을 선언합니다. 다른 플러그인이나 `lute.core`가 이미 선언한 토큰을 다시
  선언하면 `E-PLUGIN-DUP-ACROSS`입니다.
- 형식 힌트는 토큰의 타입으로 검사됩니다. `string`이나 `enum` 토큰은 텍스트 힌트(`{{platform:capitalize}}`)를 받고,
  숫자 힌트는 `E-REF-TYPE`입니다. `int` 토큰은 숫자 힌트(`{{rank:ordinal}}`)를 받고, 텍스트 힌트는 `E-REF-TYPE`입니다.
- **선언되지 않은** 맨 이름은 `E-CEL-PROFILE`이며, 메시지는 이제 플러그인의 `hostTokens:` export에 그 이름을
  선언하라고 안내합니다. `{{player-name}}`과 `{{2nd}}`는 식별자가 아니므로 이전과 같은 `E-CEL-PROFILE`을 유지합니다.
- 이스케이프 `\{{name}}`은 변하지 않습니다. 절대 플레이스홀더가 되지 않습니다.

호스트 토큰은 상태 읽기가 아니며 런타임이 대체하지도 않습니다. 모든 토큰 마커는 렌더링된 텍스트에 그대로 남고, 호스트나
프레젠터가 그것을 바꿉니다. 이는 `userName`만이 아니라 모든 토큰에 해당합니다. IR 플레이스홀더는
`{ "kind": "reserved", "token": "playerName" }`이고, lint에서 보간은 이전과 같이 한 단어로 셉니다. `lute context`는
선언된 토큰을 나열합니다.

## 단언하고, 철회하고, 수락하는 보상

이식 대상에는 "이 퀘스트는 배지를 준다"나 "이 퀘스트를 끝내면 다음 퀘스트가 제안된다" 같은 행이 자주 있습니다. 0.50
이전에는 보상 종류가 상태 경로에 값을 적립하는 것밖에 할 수 없었으므로, 배지가 뜻하는 팩트는 검사기에 보이지 않았고
그 팩트로 열리는 퀘스트는 거짓 `E-QUEST-UNREACHABLE`을 받았습니다. 이제 보상 종류는 디렉티브 어휘로 `effects:`를
선언합니다.

```yaml
# plugins/acme.stage/rewardkinds/rewards.yaml
rewardKinds:
  BADGE:       { target: { entity: person, required: true }, effects: { asserts: ["met(@target)"] } }
  REVOKE:      { target: { entity: person, required: true }, effects: { retracts: ["met(@target)"] } }
  OFFER_QUEST: { target: { required: true }, effects: { accepts: ["@target"] } }
```

예제 프로젝트의 퀘스트가 세 가지를 모두 씁니다(`quests/visits.lute`).

```lute check-in-project="docs/examples/plugin-surfaces/quests/visits.lute"
---
kind: quest
id: visits.quests
title: Visits
---
/*
Reward effects (0.50):
  - firstVisit's BADGE asserts met(mira) at the grant; secondVisit starts on
    that fact, so it is reachable only because the reward produces it. Its
    `follows` places it in the scenario graph, where `lute scenario --facts`
    draws the fact edge firstVisit -> secondVisit [met(mira)].
  - firstVisit's OFFER_QUEST accepts giftErrand, an accept-driven quest no
    `::accept` names.
  - secondVisit's REVOKE retracts met(mira) when it completes.
*/

<quest id="firstVisit" tier="run" title="Say hello to Mira" start="true">
  <reward kind="BADGE" amount="1" target="mira"/>
  <reward kind="OFFER_QUEST" target="giftErrand"/>
  <objective id="talk" title="Talk to Mira" done="run.talked"/>
</quest>

<quest id="secondVisit" tier="run" title="Visit Mira again" follows="completed('firstVisit')" start="holds('met', ['mira'])">
  <reward kind="REVOKE" target="mira"/>
  <objective id="chat" title="Have a second chat" done="run.chatted"/>
</quest>

<quest id="giftErrand" tier="run" title="Carry Mira's gift to Tomas">
  <objective id="deliver" title="Deliver the gift" done="run.delivered"/>
</quest>
```

`OFFER_QUEST`는 수락 주도 퀘스트(`start` 없음)인 `giftErrand`를 지목합니다. `start`가 있는 `secondVisit`을 지목하면
그것을 `::accept`할 때와 같이 `E-ACCEPT-TARGET`입니다.

### 효과가 말할 수 있는 것

- `effects`는 `asserts`, `retracts`, `accepts`를 가지며 각각 선택 사항입니다. 빈 `effects: {}`는 `E-PLUGIN-PARSE`
  ("declares no effect")입니다.
- **`writes`는 허용되지 않습니다.** 보상의 상태 쓰기는 `credits:`이며, `effects.writes`는 그렇게 알려 주는
  `E-PLUGIN-KEY`입니다.
- `asserts`와 `retracts`의 패턴 인자는 멤버, `true` / `false`, `@target`, 그리고 `retracts`에서만 `_`입니다. 그 밖의
  `@name`은 `E-PLUGIN-PARSE`입니다. `<reward>`는 자신의 속성 외에는 받지 않으므로 종류의 효과는 `@target`만
  읽습니다.
- `accepts` 항목은 `@target` 또는 리터럴 퀘스트 id입니다. `target: { forbidden: true }`인 종류가 `@target`을 읽는
  효과를 가지면 효과가 결코 바인딩될 수 없으므로 `E-PLUGIN-PARSE`입니다.

### 각 `<reward>`가 판정되는 방식

`@target`은 보상의 따옴표 친 `target=`에 바인딩됩니다. 해석된 각 팩트 패턴은 그것의 `::assert`나 `::retract`가
판정되는 것과 같이 판정되며(관계 선언, 인자 개수, 인자 종류와 멤버, 닫힌 종류의 멤버십), 같은 코드가 보상의 `kind=`에서
보고되고, 메시지에는 `` `<reward kind="BADGE">` asserts `met(mira)` (its kind's `effects`): `` 접두사가 붙습니다. `accepts`의
해석된 각 퀘스트 id는 `::accept`가 판정하는 방식대로 프로젝트 전체에서 판정됩니다. 알 수 없는 id(did-you-mean 포함),
`start`가 있는 퀘스트, 자동 활성화되는 자식은 `E-ACCEPT-TARGET`이며, id가 `@target`에서 왔으면 보상의 `target=`에,
아니면 `kind=`에 보고됩니다.

**`target=`이 없는** 보상은 `@target`을 읽는 효과를 적용하지 않으며 추가 진단도 없습니다(디렉티브 규칙).
`target: { required: true }`인 종류는 이미 이에 대해 `E-REWARD-TARGET`을 보고합니다.

### 언제 일어나는가

보상이 부여되는 시점(`outcome` 필터가 일치하고 `when=`이 참일 때)에, 선언 순서대로 **보상마다** 다음이 일어납니다.

1. `grant` 레코드와 그 `credits` 쓰기, 이전과 같습니다.
2. 보상의 `retracts`, 그다음 `asserts`가 런타임의 하나뿐인 assert/retract 경로(`E-FACT-EXCLUSIVE` 검사 포함)를
   거칩니다.
3. 보상의 `accepts`가 `::accept`의 경로를 거칩니다. 수락 주도 퀘스트는 다음 라이프사이클 라운드에 활성화되고, 이미
   `unset`을 지난 퀘스트는 그대로 두며 레코드가 그 사실을 알려 줍니다.

그다음 다음 보상으로 넘어갑니다. 목표 보상이 퀘스트 보상보다 먼저이고, 한 전환의 모든 부여는 그 라이프사이클 핸들러
본문보다 먼저이므로 실행 시 `questComplete` 핸들러는 보상이 단언한 팩트를 읽습니다. 효과는 부여당 정확히 한 번 일어납니다.

각 효과 레코드는 `grant` 키로 자신이 뒤따르는 부여를 명시하며, `lute play`, `lute run`, `lute trace`가 이를
출력합니다. 예제의 플레이 스크립트(`lute play --project . --script plays/visits.play.yaml`):

```console
── step 1 (the player talks to Mira) · engine ──────────────
  set run.talked = true
  firstVisit.talk done
  quest firstVisit -> complete
  grant[#1 i0] firstVisit BADGE 1 -> mira
  assert met(mira)  (effect of reward BADGE)
  grant[#1 i1] firstVisit OFFER_QUEST 1 -> giftErrand
  quest giftErrand accepted  (effect of reward OFFER_QUEST)
  quest giftErrand -> active
  quest secondVisit -> active
```

### 모든 분석이 보는 것

모든 분석은 보상 효과를 소유자의 부여 지점에서 읽습니다. 그 지점은 퀘스트 완료(`questComplete` 핸들러 이전),
`outcome="failed"`의 경우 실패(`questFailed` 이전), 또는 목표 완료입니다. 보상의 **assert는 생산자일 뿐입니다**. 가드가
있든 없든 *할 수도 있는* 집합에만 더해지고, 핸들러 본문, 목표 본문, 그 퀘스트에 걸린 어떤 것에서도 *반드시* 성립하는
집합에는 들어가지 않습니다. 라이프사이클 핸들러는 다른 퀘스트의 부여와 occasion의 비트 뒤로 미뤄질 수 있고, `user`
티어 퀘스트는 팩트를 지우는 리셋 뒤에도 완료로 남으며, 호스트는 부여 없이 퀘스트를 완료로 시드할 수 있으므로, 부여에서
팩트로 가는 함의는 건전하지 않습니다. 보상의 **retract**는 가드 있는 `::retract`처럼 일치하는 것을 *반드시* 집합에서
제거합니다. 따라서 다음과 같습니다.

- 위의 `secondVisit`은 도달 가능합니다. 보상이 그 `start` 팩트의 생산자이므로 `E-QUEST-UNREACHABLE`이 없습니다.
  `E-OBJECTIVE-UNSATISFIABLE`, `W-RELATION-UNREAD`, `E-FACT-EXCLUSIVE`도 이 효과를 봅니다. 부여 뒤에 `met(mira)`를
  검사하는 가드는 `W-FACT-GUARANTEED`도 `E-ARM-DEAD`도 받지 않습니다. 그곳에서 그 팩트는 보장이 아니라 가능일 뿐입니다.
- `lute scenario`는 팩트 엣지와 accept 엣지를 그리고, `lute lore` / `lute scenario knowledge`는 생산자를 이름으로
  보여 줍니다. `` met(mira) — produced by reward BADGE of quest `firstVisit` ``.
- `W-QUEST-NEVER-ACCEPTED`는 이제 "no `::accept` or reward `accepts` in the project names it"라고 말하며, 보상의
  accept가 이를 해소합니다.
- **시작 순환 검출은 변하지 않습니다.** 보상 accept는 도달 가능성과 연결성을 위한 엣지만 만들 뿐 퀘스트의 시작을
  가로막지 않으므로 `E-QUEST-START-CYCLE`이나 `E-QUEST-START-SELF`를 만들지 않습니다.

보상 효과가 있는 아티팩트는 시맨틱 id `lute.quest.rewardEffects/1`을 가집니다. 이 필드를 무시하는 엔진은 팩트와 퀘스트
활성화를 잃게 되므로, 이 id를 구현하지 않는 엔진은 아티팩트를 거부합니다. [런타임 계약](/ko/tooling/runtime-contract/)을
참고하세요.

## 임포트된 하위 트리의 수준

첫 임포트는 시끄럽습니다. 212개 행의 무대 경고는 사실이지만 첫날 고칠 가치는 없고, CI의 `--deny-warnings`는 결정하지
않은 경고를 둘 수 없다는 뜻입니다. 프로젝트 매니페스트는 진단 코드별로, 선택적으로 경로별로 수준을 정합니다.

```yaml
# lute.project.yaml
diagnostics:
  W-DISPLAY-NAME-DUP: error
  W-STAGE-ABSENT: { level: off, paths: ["scenes/imported/**"] }
```

- `diagnostics:`는 코드를 수준(`off`, `info`, `warn`, `error`) 또는 `{ level, paths }`에 대응시킵니다. 코드당 항목은
  하나입니다.
- `paths`는 `lute.lint.yaml`의 `ignore:`와 같은 문법(세그먼트 안의 `*`, 세그먼트를 가로지르는 `**`)의 프로젝트 루트
  기준 glob입니다. `paths`가 있으면 일치하는 문서의 진단에만 그 수준이 적용되고, 그 밖에서는 코드가 원래 심각도를
  유지합니다. 문서 경로가 없는 진단(매니페스트 진단)은 `paths`가 없는 항목에만 일치합니다.
- **`W-*` 코드만 설정할 수 있습니다.** `E-*` 코드는 계약 위반이므로 수준을 바꿀 수 없고, `L-*` lint 규칙은
  `lute.lint.yaml`의 `rules:`에서 설정합니다. 네 수준 중 하나가 아닌 값, 툴체인이 정의하지 않은 코드(did-you-mean 포함),
  또는 비어 있거나 절대 경로이거나 `..`를 포함하는 `paths`는 매니페스트당 한 번 보고되는 `E-DIAGNOSTICS-CONFIG`이며
  그 항목은 버려집니다.

4행에 `W-STAGE-ABSNT: off`가 있는 매니페스트는 다음을 보고합니다.

<!-- lute-diagnostics -->
```
./lute.project.yaml:4:3: error [E-DIAGNOSTICS-CONFIG] `W-STAGE-ABSNT` is not a diagnostic code — did you mean `W-STAGE-ABSENT`?
```

수준은 진단을 출력하는 지점에서 다음 순서로 적용됩니다.

1. 코드와 문서 경로에 대한 프로젝트 수준.
2. `--deny CODE`가 그 코드를 지목하면 **프로젝트가 `off`로 두었더라도** 오류로 승격됩니다. 명시적인 명령줄이 설정보다
   우선하기 때문입니다.
3. `--deny-warnings`는 1단계 이후에도 여전히 경고인 것을 승격합니다. `off` 코드는 꺼진 채로, `info` 코드는 info로
   남습니다.

오류는 절대 낮추지 않습니다. `error`로 바뀐 코드는 `--deny`처럼 판정과 종료 상태를 실패시킵니다. JSON에서는
`"severity": "error"`와 `"configured": "lute.project.yaml"`을 가지며, 사람이 읽는 줄에는 `[denied]` 옆에
`[project]` 표지가 붙습니다. `info`나 `warn`으로 바뀐 코드는 새 심각도와 같은 표지를 가집니다.

**수준은 보고되는 내용과 종료 상태만 바꿉니다.** 분석, 아티팩트, 제약 판정, 시나리오 그래프, `.test.yaml`이나
`.play.yaml` 기대값이 관찰하는 내용은 절대 바꾸지 않습니다. 수준은 `lute check`, `lute compile`, `lute lint`에
적용됩니다. 시뮬레이션 명령(`lute test`, `lute play`, `lute calendar`)은 수준을 전혀 적용하지 않습니다. 원래 진단을
출력하며, 프로젝트의 `error` 수준이 있어도 `lute test`의 결과와 종료 상태는 그것이 없을 때와 같습니다. LSP도 같은 수준을
적용하며 `off` 진단은 게시되지 않습니다.

아무것도 말없이 숨겨지지 않습니다. 진단을 세는 모든 요약 줄은 N이 0보다 클 때에만 `, N suppressed by
lute.project.yaml`을 덧붙이고, 모든 JSON 보고서는 비어 있지 않을 때에만 `"suppressed": { "<code>": n }` 맵을 가집니다.

예제 프로젝트에서 `lute check --project .`는 다음을 출력합니다.

```console
ok: ./quests/visits.lute (0 warning(s))
ok: ./scenes/cafe.lute (0 warning(s))
ok: ./scenes/imported/street.lute (0 warning(s), 1 suppressed by lute.project.yaml)
ok: . (3 file(s), 0 project-wide warning(s))
```

`lute.lint.yaml`의 `rules:` 항목이 검사기 코드를 지목하면 `E-LINT-CONFIG`이며, 메시지가 이 페이지의 방법을 가리킵니다.
`` `W-STAGE-ABSENT` is a checker code, not a lint rule — set its level in `lute.project.yaml` `diagnostics:` ``.

## 화자를 무대에 올리는 줄

`W-STAGE-ABSENT`는 캐릭터가 무대를 떠난 뒤(선언된 퇴장, `::bg` 장면 전환, `::clear`) 아무것도 그를 다시 보여 주지
않았는데 말할 때 경고합니다. 무대 밖 화자를 보여 주는 행이라는 것을 아는 임포터는 먼저 무대 행을 내보냅니다. 새로운
구성 요소는 필요 없습니다. 캐릭터를 무대에 올리는 레코드는 `actor` 레코드이며, `::actor`로 만들거나 그 레코드로
낮춰지는 플러그인 디렉티브로 만들 수 있기 때문입니다.

```yaml
# plugins/acme.stage/directives/stage.yaml
directives:
  - name: showSpeaker
    attrs:
      - { name: character, required: true, type: string }
      - { name: spriteId, type: string }
    lower:
      record: actor
      fields:
        character: { fromAttr: character }
        costume: { fromAttr: spriteId }
```

```lute
::bg{location="street" time="night"}
::showSpeaker{character="tomas" spriteId="SPR_tomas_02"}
@tomas: Still here?
```

`actor`로 낮춰지는 디렉티브는 `::actor`처럼 `character`를 무대에 올리며(0.45부터), `character`는 필수 속성이나
리터럴에 바인딩되어야 합니다. actor 레코드에는 스프라이트 전용 필드가 없으므로 이 예제는 임포터의 스프라이트 id를
`costume`에 담았습니다. 플러그인은 `on: [showSpeaker]`로 범위를 지정한 스탬프에 담을 수도 있습니다.
`W-STAGE-ABSENT` 힌트는 이제 "Show them again with an `::actor` (or a plugin directive declared `lower: { record:
actor, fields: { character: { fromAttr: character } } }`)"라고 말합니다. `lower: { record: actor }`만으로는 로드에서
거부되고(`fields:`가 필요합니다) 빈 `fields: {}`는 `E-LOWER-RECORD-FIELD`이므로 전체 형태를 적습니다.

이식을 검토하는 동안에는 위의 [수준](#임포트된-하위-트리의-수준)에서처럼 임포트된 하위 트리에 대해 이 코드를 `off`로
설정하고, 임포터가 무대 행을 내보내면 그 항목을 지웁니다.

```yaml
diagnostics:
  W-STAGE-ABSENT: { level: off, paths: ["scenes/imported/**"] }
```

## 구조와 종속 값을 위한 lint

어떤 규칙은 노드가 무엇을 담는지가 아니라 어디에 있는지에 관한 것입니다. 모든 섹션은 `::bg`로 시작한다, 어떤 종류의
`::signal` 뒤에는 `<branch>`가 와야 한다, 비트 안의 라벨은 `b_`로 시작한다 같은 것입니다. 다른 규칙은 한 속성이 다른
속성에 종속되는 것입니다. 합법적인 `speed`는 `mode`에 달려 있고, 합법적인 옵션 키는 둘러싼 `<branch>`의 메뉴 종류에
달려 있습니다. 둘 다 AST 위에서 실행되는 `target: node` lint이며, `lute.lint.yaml`의 `custom:` 규칙, 플러그인의
`lints:` export, LSP에서 쓸 수 있습니다.

`node` 규칙은 검사기가 훑는 모든 본문에서 문서 순서대로 노드마다 한 행을 받습니다. 섹션, 목표와 `<on>` 핸들러를 가진
`<quest>` 선언, `<reward>` 자식, 최상위 `<entry>`와 `<beat>`, `<branch>`와 `<hub>`의 선택지, 선택지 본문, 허브의
`<return>`, `<match>` 갈래와 그 본문, `<timeline>`(한 행이며 클립은 행이 아닙니다)입니다. 바인딩은 `node`, `scene`(문서
행), `options`입니다.

| 필드 | 의미 |
|---|---|
| `node.kind` | `line`, `directive`(코어든 플러그인이든 모든 `::tag{…}`, `::set`과 `::end` 포함), 또는 요소 이름(`branch`, `hub`, `choice`, `return`, `match`, `when`, `otherwise`, `timeline`, `quest`, `objective`, `on`, `reward`, `entry`, `beat`) |
| `node.tag` | 디렉티브 태그나 요소 이름. 줄은 `""` |
| `node.speaker` | 줄의 화자. 그 밖에는 `""` |
| `node.attrs` | **작성된 그대로의** 모든 속성의 문자열 → 문자열 맵. 작성자가 쓴 경우에만 키가 있으며, 값은 따옴표를 뺀 작성된 텍스트입니다. 끌어올려진 속성(`<branch id>`, `<reward kind target amount>`, `::jump{to}`)과 스탬프가 들어 있습니다. 맨 플래그는 `"true"`, `@ref`는 그 소스 텍스트이며, `::set`, `::assert`, `::retract`는 `{}`입니다 |
| `node.index`, `node.first`, `node.last` | 본문 안의 0부터 시작하는 위치. 그 본문의 처음과 끝 |
| `node.depth` | 섹션이나 문서의 최상위 노드는 0, 둘러싼 요소마다 +1 |
| `node.prev`, `node.next` | 같은 본문에서 형제의 `{ kind, tag, attrs }`. 본문 경계에서는 모든 필드가 빕니다 |
| `node.parent` | 둘러싼 요소의 `{ kind, tag, attrs }`. 섹션 최상위에서는 `kind`가 `"section"`, 퀘스트나 로어 문서의 최상위에서는 `"document"` |
| `node.preceding` | 태그 → `{ attrs }` 맵. 각 태그에 대해 이 노드와 각 조상의 앞선 형제들, 그리고 조상 자신 가운데 그 태그를 가진 가장 가까운 노드입니다. 앞선 형제의 본문으로는 **들어가지 않으며**(다른 갈래는 이 경로 위에 없습니다), 섹션과 선언마다 초기화되고 줄은 키로 쓰이지 않습니다 |
| `node.section` | 둘러싼 섹션의 `{ index, title, id }`. 섹션 밖에서는 `{ 0, "", "" }` |

`node.attrs`는 소스에서 읽으므로 `<reward kind="COINS" amount="+005"/>`는 `"5"`가 아니라 `"+005"`를 주고,
`amount` 없이 쓴 보상에는 `amount` 키가 없습니다. 결과는 노드를 가리키고, 메시지에는 `{node.attrs.value}`가
보간됩니다.

lint를 정직하게 유지하는 규칙이 둘 있습니다.

- **모든 인덱스를 가드하세요.** 없는 키는 CEL 의미를 유지합니다. 없는 키에 대한 `node.attrs["x"]`는 평가 오류
  (`E-LINT-EXPR`)입니다. `&&`는 단락 평가되므로 `"x" in node.attrs && node.attrs["x"] …`,
  `"t" in node.preceding && "k" in node.preceding["t"].attrs && …`, 그리고 옵션 맵을 인덱싱하기 전에
  `key in options.cases && …`처럼 씁니다.
- **알 수 없는 판별자는 다른 규칙의 발견입니다.** 종속 값 규칙은 자신에게 경우가 없는 판별자를 제외하고, 별도의
  `*-known` 규칙이 판별자 자체를 보고합니다. 그러면 각 위반은 그것을 지목하는 규칙에 의해 한 번만 보고됩니다.

lint 표현식 언어에는 CEL 문자열 멤버 `s.startsWith(t)`, `s.endsWith(t)`, `s.contains(t)`도 있습니다. 멤버 호출 형식이며
두 피연산자 모두 문자열이어야 합니다(문자열이 아닌 피연산자는 평가 오류). 정규식과 매크로는 없습니다.

아래 레시피는 플러그인 `lints:` 항목입니다. `lute.lint.yaml`에서는 같은 항목을 맨 id로 `custom:` 아래에 넣으며,
플러그인 규칙의 id는 `<plugin-id>/<rule-id>`입니다.

```yaml
lints:
  - id: signal-needs-menu
    target: node
    when: 'node.tag == "signal" && "value" in node.attrs && node.attrs["value"] == "puzzle" && node.next.kind != "branch"'
    level: warn
    message: "a puzzle ::signal must be followed by a <branch>"
  - id: section-opens-with-bg
    target: node
    when: 'node.first && node.parent.kind == "section" && node.tag != "bg"'
    level: warn
    message: "section {node.section.title} opens with {node.tag}"
  - id: beat-label-prefix
    target: node
    when: 'node.tag == "label" && scene.kind == "beat" && "name" in node.attrs && !node.attrs["name"].startsWith("b_")'
    level: error
    message: "a beat's labels start with b_"
```

**한 속성이 다른 속성의 값을 결정합니다**(이동의 `mode`가 합법적인 `speed`를 고릅니다). 첫 규칙은 알 수 없는 mode를
보고하고, 둘째 규칙은 그것을 건너뜁니다.

```yaml
lints:
  - id: move-mode-known
    target: node
    when: 'node.tag == "move" && "mode" in node.attrs && !(node.attrs["mode"] in options.speeds)'
    level: error
    message: "unknown move mode {node.attrs.mode}"
    options: { speeds: { walk: [slow, normal], run: [normal, fast] } }
  - id: speed-fits-mode
    target: node
    when: >-
      node.tag == "move" && "speed" in node.attrs && "mode" in node.attrs &&
      node.attrs["mode"] in options.speeds &&
      !(node.attrs["speed"] in options.speeds[node.attrs["mode"]])
    level: error
    message: "speed {node.attrs.speed} is not legal in mode {node.attrs.mode}"
    options: { speeds: { walk: [slow, normal], run: [normal, fast] } }
```

**분기 앞의 마커 디렉티브가 합법적인 옵션 값을 결정합니다.** `node.preceding`은 각 `::option`에 이르는 경로에서 가장
가까운 `::interaction`을 찾습니다.

```yaml
lints:
  - id: interaction-kind-known
    target: node
    when: 'node.tag == "interaction" && (!("kind" in node.attrs) || !(node.attrs["kind"] in options.cases))'
    level: error
    message: "::interaction needs a known kind"
    options: { cases: { greet: [wave, bow], gift: [accept, decline] } }
  - id: option-fits-interaction
    target: node
    when: >-
      node.tag == "option" && "value" in node.attrs &&
      "interaction" in node.preceding && "kind" in node.preceding["interaction"].attrs &&
      node.preceding["interaction"].attrs["kind"] in options.cases &&
      !(node.attrs["value"] in options.cases[node.preceding["interaction"].attrs["kind"]])
    level: error
    message: "option {node.attrs.value} does not belong to this interaction"
    options: { cases: { greet: [wave, bow], gift: [accept, decline] } }
```

**부모 `<branch>`가 판별자를 가집니다.** 이 가이드 첫 절의 범위가 지정된 스탬프입니다.

```yaml
lints:
  - id: menu-type-known
    target: node
    when: 'node.kind == "branch" && "menuType" in node.attrs && !(node.attrs["menuType"] in options.keys)'
    level: error
    message: "unknown menu type {node.attrs.menuType}"
    options: { keys: { plain: [L1, R1], timed: [T1] } }
  - id: choice-fits-menu
    target: node
    when: >-
      node.kind == "choice" && "optionKey" in node.attrs &&
      "menuType" in node.parent.attrs && node.parent.attrs["menuType"] in options.keys &&
      !(node.attrs["optionKey"] in options.keys[node.parent.attrs["menuType"]])
    level: error
    message: "option key {node.attrs.optionKey} is not a {node.parent.attrs.menuType} key"
    options: { keys: { plain: [L1, R1], timed: [T1] } }
```

### `pair`로 쓰는 균형 규칙

균형 규칙은 반드시 닫혀야 하는 여는 요소에 관한 규칙입니다. `::group{mode="begin"}`과 `::group{mode="end"}`, 열고 닫는
창이 그 예입니다. 이것은 `target: pair` lint(0.51부터)이며 `node` lint와 같은 자리에서 쓸 수 있습니다. `pair` 규칙은 같은
`node` 행 위의 `open`과 `close` 술어, 선택적인 `inside` 술어, `scope`를 선언하며 `when`은 없습니다. `pair` 규칙에 `when`을
쓰거나 다른 target의 규칙에 `open`, `close`, `inside`, `scope`를 쓰면 플러그인 `lints:`에서는 `E-PLUGIN-PARSE`,
`lute.lint.yaml`에서는 `E-LINT-CONFIG`입니다.

```yaml
lints:
  - id: group-balanced
    target: pair
    open:   'node.tag == "group" && "mode" in node.attrs && node.attrs["mode"] == "begin"'
    close:  'node.tag == "group" && "mode" in node.attrs && node.attrs["mode"] == "end"'
    inside: 'node.kind == "line" || node.tag in ["actor", "sfx"]'   # optional
    scope: body                                                    # body (default) | section
    level: error
    message: "::group begin/end"
```

```lute
::group{mode="begin"}
@mira: Look at this.
@tomas: And this.
::group{mode="end"}
```

이 쌍은 균형이 맞습니다. 각 scope는 열린 슬롯 하나로 한 번 훑습니다.

- **`scope: body`**(기본)는 각 본문의 직접 자식, 즉 형제인 행끼리 순서대로 짝짓습니다. **`scope: section`**은 한 섹션이나
  선언의 모든 행을 깊이와 상관없이 문서 순서대로 짝짓습니다. 그래서 한 `<choice>`의 `open`과 그 형제의 `close`가 짝이 됩니다.
  `section`은 경로가 아니라 문서 순서입니다.
- 쌍이 열려 있는 동안 `close`가 성립하는 행은 그것을 닫습니다. `open`과 `close`가 모두 성립하는 행은 쌍이 열려 있으면 닫는
  행이고, 아니면 여는 행입니다.
- 쌍이 열려 있는 동안 `open`이 성립하는 행은 **중첩된 open**이며 바깥 쌍은 열린 채로 남습니다. 열린 쌍이 없을 때
  `close`가 성립하는 행은 **열린 적 없는 close**입니다. scope가 끝났는데 열린 쌍이 있으면 **닫히지 않은 open**입니다.
- `inside`가 있으면 여는 행과 닫는 행 사이의 모든 행 가운데 `inside`가 거짓인 행은 **inside 위반**입니다. `body` scope에서는
  형제만 세며 그 자손은 세지 않습니다.

각 발견에는 규칙의 코드와 수준이 붙고 문제의 행을 가리키며, 규칙의 `message` 뒤에 고정 문구가 붙습니다.
` — opened here, with no close in this body`(`section` scope에서는 `in this section`),
` — a close with no open pair in this body`, ` — opened again while the pair from line N is still open`,
` — not allowed inside the pair opened at line N`입니다. 중첩된 open과 inside 위반은 여는 행을 관련 위치로 가리킵니다.

`inside` 위반의 예입니다. 마커 사이의 `::bg`는 줄도 `::actor`도 `::sfx`도 아니므로 위의 규칙은 1행에서 열린 쌍 안에는
허용되지 않는다고 보고합니다.

```lute
::group{mode="begin"}
@mira: Look at this.
::bg{location="street" time="night"}
::group{mode="end"}
```

쌍은 `<branch>`를 가로지를 수 있습니다. `scope: section`에서는 `<branch>` 앞의 begin과 뒤의 end가 짝이 되며, 한
`<choice>` 안의 begin과 다음 `<choice>` 안의 end도 짝이 됩니다. `body` 규칙이라면 이를 닫히지 않은 open과 열린 적 없는
close로 보고합니다.

```yaml
lints:
  - id: window-balanced
    target: pair
    open:   'node.tag == "window" && "mode" in node.attrs && node.attrs["mode"] == "open"'
    close:  'node.tag == "window" && "mode" in node.attrs && node.attrs["mode"] == "close"'
    scope: section
    level: error
    message: "::window open/close"
```

```lute
::window{mode="open"}
<branch id="buy" prompt="Buy something?">
  <choice id="yes" text="Yes">
    @shopkeeper: Here you go.
  </choice>
  <choice id="no" text="No">
    @shopkeeper: Next time.
  </choice>
</branch>
::window{mode="close"}
```

`node` lint의 가드 규칙은 여기서도 그대로입니다. `"mode" in node.attrs && …`처럼 쓰세요. 가드 없는 술어에서 없는 키는
`E-LINT-EXPR`이며 그 문서에서 그 규칙을 멈춥니다. 행마다 `pair` 규칙당 최대 세 번의 술어 호출만 듭니다.
`lute.lint.yaml`의 나머지는 [린트](/ko/tooling/linting/)를 참고하세요.

## 한도 있는 `::jump`로 만드는 재시도 루프

"미니게임을 하고, 실패하면 최대 두 번까지 다시 제안한다" 같은 시트 행은 루프입니다. 0.51 이전에는 `::jump`가 앞으로만 갈 수
있어서 임포터가 루프를 풀어 쓰거나, 어떤 분석도 볼 수 없는 passthrough 디렉티브에 숨겼습니다. 0.51부터는 `max`가 있는
`::jump`가 **앞선** `::label`을 가리킬 수 있습니다.

```lute
::label{name="attempt"}
::tryLock{door="vault"}
<match subject="scene.lock.vault.won">
  <when is="false">
    @mira: Again!
    ::jump{to="attempt" max="2"}
  </when>
  <when is="true">
    @mira: It clicks open.
  </when>
</match>
@theo: That's enough for today.
```

`::tryLock`은 호스트의 답으로 `scene.lock.vault.won`을 쓰는 블로킹 브리지 호출입니다. `max`는 점프를 따라갈 수 있는 횟수이며,
`max`번 따라간 뒤에는 점프하지 않고 다음 노드로 넘어갑니다. 따라서 이 루프는 최대 세 번(처음 한 번과 재시도 두 번) 묻습니다.

- **`max`는 1~99의 정수 리터럴입니다.** `@ref`, `0`, 음수, 99 초과는 `E-JUMP-MAX`이며, 루프 한도는 정적이기 때문입니다.
  `max="two"`는 `E-ATTR-TYPE`뿐입니다. 앞으로 가는 점프의 `max`도 아무 뜻이 없으므로 `E-JUMP-MAX`입니다. `max` 없이 뒤로
  가는 점프는 계속 `E-JUMP-BACKWARD`이며, 메시지는 이제 `max`와 선택지를 다시 보여 주는 `<hub>`를 알려 줍니다.
- **`when=`과 `max`는 함께 쓸 수 있습니다.** `when`이 성립하고 횟수가 남아 있는 동안 점프합니다.
- **루프는 한 본문 안에 머뭅니다.** 레이블은 점프와 같은 본문에 있거나, `<branch>`의 `<choice>` 본문과 `<match>` 갈래
  본문만 거쳐 점프를 감싸는 본문에 있어야 합니다. 점프가 든 `<hub>` 옵션 밖의 레이블은
  `E-JUMP-LOOP-SCOPE`입니다.
- **루프는 머리에서만 진입합니다.** 바깥에서 출발해 머리 뒤, 루프 안의 레이블로 가는 앞으로 가는 점프는
  `E-JUMP-INTO-LOOP`입니다. 머리로 가는 앞으로 가는 점프와 루프 밖으로 나가는 점프는 괜찮습니다.
- **루프는 중첩될 뿐 교차하지 않습니다.** 두 루프는 중첩되거나 서로소여야 하며, 아니면 나중 점프에서 `E-JUMP-LOOP-CROSS`입니다.
  두 `max` 점프가 머리 하나를 공유할 수 있고, 각자 자기 카운터를 가집니다.

중첩된 루프는 각자 셉니다. 안쪽 카운터는 제어가 안쪽 루프 바깥에서 안쪽 머리에 도착할 때마다 0으로 돌아가므로,
바깥 `max="1"` 안의 안쪽 `max="2"`는 두 라운드마다 안쪽 본문을 세 번씩 실행합니다.

```lute
::label{name="round"}
@theo: A new round.
::label{name="knock"}
@theo: Knock.
::jump{to="knock" max="2"}
@theo: Round done.
::jump{to="round" max="1"}
```

두 번 따라가는 사이에 제어는 앞으로만 움직이므로, 중첩 깊이 *d*인 영역은 감싸는 점프들의 (`max` + 1)의 곱만큼만 실행됩니다.
모든 walk는 플레이어의 도움 없이 끝나며 분석은 유한하게 유지됩니다.

모든 흐름 분석은 레이블에 닿는 모든 점프의 상태를 앞쪽이든 뒤쪽이든 합류시키고, 루프 머리가 안정될 때까지 반복합니다.
점프가 `::set`을 건너뛰어 닿는 레이블 뒤의 읽기는 `E-MAYBE-UNSET`, 다시 등장하는 지점을 건너뛴 점프는
`W-STAGE-ABSENT`이며, `max`가 있는 점프는 본문을 끝내지 않으므로(한도에 이르면 다음으로 넘어갑니다)
`W-CODE-AFTER-JUMP`가 조용합니다. 앞으로 가는 점프의 틈 두 곳도 함께 고쳐지므로, 쓰기를 건너뛰는 점프가 있는 문서는 0.50에
없던 `E-MAYBE-UNSET`을 새로 받을 수 있습니다.

플레이와 테스트에서: 카운터는 한 번의 walk에 국한되고 리플레이로 다시 만들어지므로 런타임 스냅숏에 새로 필요한 것이 없습니다. 이후
반복에서 다시 닿은 `<branch>`는 새 메뉴이며, 그 `choose:` 목록은 제시마다 결정 하나씩 소비됩니다(`choose: { retry: [yes,
yes, no] }`가 세 번의 반복을 이끕니다). 루프 안의 `<hub>`는 방문마다 첫 제시에서 시작하며, 앞선 방문에서 고른 `once`
선택지는 계속 소진된 상태입니다. `lute trace`는 `<jump -> attempt (again 1/2)>`와 `<jump -> attempt not taken (2/2 used)>`를
출력합니다. 참조 런타임은 적재 시 루프를 다시 검사해, 뒤로 가는 이동이 잘 중첩된 한정 점프가 아닌 아티팩트를 거부합니다.
컴포넌트 안의 `::label`은 `::use` 인스턴스마다 따로 컴파일되므로 각 인스턴스는 자기 라벨에서 루프를 돕니다(#207).

"같은 구간을 한도 안에서 다시"에는 루프를 쓰고, 플레이어가 `exit` 선택지를 고를 때까지 메뉴를 다시 보여 줄 때는 `<hub>`를
쓰세요.

## 디렉티브나 보상이 raise하는 장면

"컷신 X를 재생한다" 같은 시트 행은 컷신을 실행하지 않습니다. 호스트가 그 장면이 응답하는 occasion을 raise해서 실행합니다.
occasion을 raise하는 것은 호스트의 일이고 Lute는 raise하지 않으므로, 0.51 이전에는 그 occasion에 응답하는 장면에 그 행에서
오는 엣지가 없어서 시나리오 그래프가 둘의 순서를 알 수 없었습니다. 0.51부터 디렉티브나 보상 kind는 자신에 답해 호스트가
raise하는 occasion을 선언하고, occasion은 선언된 raise로만 raise된다고 말할 수 있습니다.

```yaml
# plugins/acme.flow/occasions/occasions.yaml
occasions:
  cutscene: { select: first, target: { prefix: scene, entity: cutsceneId }, raisedBy: declared }
# plugins/acme.flow/directives/flow.yaml
directives:
  - name: playScene
    attrs:
      - { name: scene, required: true, type: { entity: cutsceneId } }
    effects:
      raises: [{ occasion: cutscene, target: "@scene" }]
# plugins/acme.flow/rewardkinds/rewards.yaml
rewardKinds:
  CUTSCENE: { target: { entity: cutsceneId, required: true }, effects: { raises: [{ occasion: cutscene, target: "@target" }] } }
```

```lute check-in-project="docs/examples/flow-and-text/scenes/chapter2.lute"
---
kind: scene
id: chapter2
on: cutscene
target: scene.chapter2
---
```

장면의 `::playScene{scene="chapter2"}`나 퀘스트의 `<reward kind="CUTSCENE" target="chapter2"/>`가 이제 `scene.chapter2`를
대상으로 `cutscene`을 raise합니다.

- **`effects.raises`**는 `{ occasion, target? }`의 비어 있지 않은 목록이며, 디렉티브에서는 `writes`/`asserts`/`retracts`
  옆에, 보상 kind에서는 `asserts`/`retracts`/`accepts` 옆에 둡니다. 보상 kind는 `raises`만 선언할 수도 있습니다.
  `target`은 디렉티브에서는 `@<attr>`(선언된 속성), 보상 kind에서는 `@target`(보상의 `target=`), 또는 통째로 쓴 리터럴
  target(`scene.chapter2`)이며, target 없이 선언된 occasion에서만 생략합니다.
- occasion의 **`raisedBy: declared`**는 호스트가 선언된 `raises`가 말하는 곳에서만 그것을 raise한다는 뜻입니다. 없으면 선언된
  raise는 기록되고 검사되지만 그래프 엣지는 그리지 않습니다.
- 모양 오류는 로드 시 `E-PLUGIN-PARSE`입니다. `lower:`가 있는 디렉티브는 효과를 싣지 못하는 레코드로 컴파일되므로 그
  `raises`는 호스트에 닿을 수 없습니다: `E-PLUGIN-INVALID-DIRECTIVE`. 어셈블리에서는 `E-PLUGIN-RAISES`가 occasion(did-you-mean
  포함)과 target의 모양·타입을 검사합니다. occasion이 다른 플러그인의 것이면 그 플러그인을 `depends:`에 둡니다.
- `check --project`에서는 `E-RAISES-TARGET`이 해석된 각 target을 검사합니다. 엔티티 kind의 멤버여야 하고 어떤 장면이나
  비트가 응답해야 합니다. 값은 호출의 속성 값, 없으면 속성의 `default:`이며, 값이 없는 target은 아무것도 raise하지 않고
  `occasion.target`을 읽는 target은 동적이라 판정하지 않습니다.
- `W-OCCASION-NEVER-RAISED`는 프로젝트의 어떤 `effects.raises`도 닿지 않는 `raisedBy: declared` occasion에 응답하는 장면이나
  비트에서 경고합니다. `lute play` 스텝의 raise는 근원이 아니고, 프로젝트 클록이 raise하는 occasion은 경고하지 않습니다.
  `lute.project.yaml`의 `diagnostics:`에서 다른 `W-*`처럼 수준을 바꿀 수 있습니다.

모든 분석은 선언된 raise를 **가능하지만 보장은 아님**으로 읽습니다. occasion의 게이트가 호스트의 raise를 거절할 수 있으므로
raise는 응답하는 장면에 팩트, 쓰기, must 주장을 더하지 않습니다. `lute scenario`와 `lute scenario reach`는
raise하는 쪽에서 응답하는 모든 장면과 비트로 `[raises]` 엣지를 그리며, `after:`를 선언한 장면은 `after:`를 자신의 경로로
유지합니다. (0.52부터 `lute impact`도 `[raises]`와 `[accept]` 엣지를 따라갑니다. #210.)
[flow-and-text 예제](https://github.com/journeyWorker/lute/tree/main/docs/examples/flow-and-text)에서는 다음과 같습니다.

```console
$ lute scenario --project docs/examples/flow-and-text
project root: docs/examples/flow-and-text
  topological layers:
    layer 0: scene(hall), scene(lock), quest(finale)
    layer 1: scene(chapter2)
  edges (prerequisite -> dependent) [atom kind(s)]:
    scene(lock) -> scene(chapter2) [raises]
    quest(finale) -> scene(chapter2) [raises]
```

raise는 호스트가 수행합니다. 호출을 실행한 제시가 끝난 뒤 런타임이 한가해지면, 호스트가 각 raise를 레코드 순서대로
`raiseOccasion` 입력으로 보냅니다. 보상은 부여를 정산한 뒤에 그렇게 합니다. 컴파일된 `plugin`과 `grant` 레코드가 해석된
`raises`를 담고, `lute play`는 그 줄에 `  → raises cutscene scene.chapter2`를 출력합니다. 레퍼런스 플레이어는 이를 대신
수행하고 대기 중인 raise를 저장과 롤백 지점에 보관하며, 커스텀 호스트도 같은 일을 합니다
([커스텀 호스트](/ko/player/custom-hosts/) 참고). `lute play`는 스스로 raise하지 않으며, 스크립트가 다음 스텝으로 그
occasion을 raise합니다.

## 플레이어가 읽는 디렉티브 텍스트

연출되는 배너, 토스트, 챕터 제목은 플레이어가 읽는 텍스트이므로 번역할 수 있어야 합니다. 그런 속성은 `type: text`로 선언합니다.

```yaml
directives:
  - name: banner
    attrs:
      - { name: title,    required: true, type: text }
      - { name: subtitle, type: text }
      - { name: style,    type: { enum: [toast, card] } }
```

```lute
::banner{code="0010" title="Welcome to the hall, {{userName}}" style="card"}
```

- `text` 값은 선택지 라벨 문법의 따옴표 친 문자열입니다. 상태 경로, `@def`, 호스트 토큰의 `{{…}}` 보간은 라벨과 똑같이
  검사됩니다. `\{{`는 리터럴 `{{`를 씁니다(0.52부터, #211. 0.51에서는 `E-STRING-ESCAPE`였습니다). `@ref` 값은 `E-ATTR-TYPE`입니다.
- `text`는 `plugin` 레코드로 남는 디렉티브의 속성에만 허용됩니다. `lower:`도 `bridge:`도 없어야 합니다. `default:`를 가질 수
  없고, 스탬프, 플러그인 옵션, 페이로드, 보상 kind 속성의 타입으로도 쓸 수 없습니다. `text` 속성이 있는 디렉티브는 `code`라는
  이름의 속성을 선언할 수 없습니다. 각 오류는 그 속성에서 `E-PLUGIN-PARSE`이고, 그런 디렉티브에 범위를 지정한 `code` 스탬프는
  `E-PLUGIN-RESERVED-STAMP-ATTR`입니다.
- **`code=`**는 줄과 같이 그런 디렉티브의 식별자입니다. 각 `text` 속성의 `lineId`는 `{prefix}~{tag}~{code}~{attr}`이며
  `hall~banner~0010~title` 같은 모양입니다. `~`는 이 id를 위해 예약되어 있으므로 `~`가 든 `code`는 `E-LINE-CODE-RESERVED`,
  `~`가 든 `lineId` 템플릿은 `E-IDENTITY-TEMPLATE`입니다. 한 범위의 `::banner` 두 호출이 같은 `code`를 가지면
  `E-DUP-LINE-CODE`, 프로젝트 어디서든 두 단위의 최종 `lineId`가 같으면 `E-LINE-ID-DUP`입니다. `text` 속성이 없는 디렉티브의
  `code=`는 `E-UNKNOWN-ATTR`입니다.
- **`lute tag`**는 `text` 속성이 있고 코드가 없는 모든 디렉티브에, 컴파일러가 채우는 순서 그대로 `code="…"`를 씁니다.
  컴포넌트는 그 사용자들의 스냅숏으로 태그하며, 사용자들이 서로 다르면 `E-COMPONENT-TEXT-DIRECTIVE`입니다.
  `identity.requireStable`이면 코드 없는 디렉티브를 `W-LINE-CODE-UNTAGGED`가 보고합니다.

번역 왕복은 줄이 이미 쓰는 것과 같으며, 텍스트 속성은 행 종류가 하나 늘 뿐입니다.

```sh
lute tag scenes
lute loc export --project . -o export/en.json
# export/ko.json으로 복사하고(파일 이름이 로케일입니다) "text" 값을 번역합니다.
# 모든 {{…}} 마커를 그대로 유지하세요
lute loc import export/ko.json -o loc/ko.json          # 로케일 번들
lute compile scenes/hall.lute --project . --locale-bundle loc/ko.json
```

내보낸 행은 `{ "kind": "directive", "lineId": "hall~banner~0010~title", "key": "banner.title", … }` 모양입니다. 디렉티브에
`code`가 생기기 전에는 `lineId`가 `null`이므로 먼저 `lute tag`를 실행하세요. `lute loc report`는 디렉티브 텍스트
(`directiveTexts`)를 프롬프트처럼 셉니다. 컴파일된 `plugin` 레코드에서 `fields.title`은 마커를 작성된 그대로 둔 원문을 유지하고,
`localized.title`은 병합된 번역과 함께 `{ lineId, placeholders, texts }`를 담습니다. 로케일이 없으면 `W-L10N-MISSING`, 원문이
바뀌었으면 `W-L10N-STALE`이며 줄과 같습니다. 런타임이 마커를 렌더링하고(호스트 토큰은 그대로 남습니다), 레퍼런스 플레이어의
`PluginCall.localized`가 호스트 핸들러에 플레이어 로케일의 각 텍스트를 줍니다.

## 구조화된 값은 아직 지원하지 않습니다

목록이나 레코드를 담는 열(태그 목록, 언어별로 키가 달린 음성 에셋 맵)은 디렉티브 속성에 맞지 않습니다. `list`, `map`,
`record` 속성 및 스탬프 타입은 어떤 `.lute` 값으로도 만족할 수 없으므로 `E-PLUGIN-INVALID-DIRECTIVE`로 계속 거부됩니다.
구조화된 작성(#176)은 **미루며 다루지 않습니다**. 0.51에는 `::tagged{tags=…}` 같은 일반적인 enum 목록 메타데이터를 쓸
문법도, 언어별로 키가 달린 음성 에셋 맵을 쓸 문법도 없습니다. 로케일 없이 해석되는 줄당 음성 키 하나는 언어별 에셋 맵이
아닙니다.

스칼라로 나눌 수 있으면 그렇게 하세요.

| 행이 가진 것 | 쓰는 방법 |
|---|---|
| 이름 있는 몇 개의 필드 | 필드마다 속성 하나. 스탬프는 `on:`으로 표면을 정합니다 |
| 여러 언어로 플레이어가 읽는 텍스트 | `text` 속성 하나 |
| 정말로 옵션의 집합인 목록 | `<choice>` 요소 |

거부 메시지도 같은 말을 합니다. 필드마다 속성을 하나씩 두거나, 플레이어에게 보이는 텍스트에는 `text` 속성을, 옵션 목록에는
`<choice>` 요소를 쓰라는 것입니다. 스탬프는 `text`가 될 수 없으므로 스탬프에는 `text` 안내가 빠집니다.

## 도구가 잡아내는 것과 잡아내지 못하는 것

| 이식에서의 문제 | 잡아내는 것 |
|---|---|
| 속하지 않는 행에 쓰인 열 | `E-UNKNOWN-ATTR`, 스탬프의 표면을 알려 줌 |
| 필수 열이 빠진 행 | 범위가 지정된 `required: true` 스탬프의 `E-MISSING-ATTR` |
| 모든 레코드에 필수로 선언된 스탬프 | 로드 시 `E-PLUGIN-PARSE` |
| 표면이 이미 가진 속성의 이름을 딴 스탬프 | `E-PLUGIN-RESERVED-STAMP-ATTR` |
| `on:`의 오타 난 표면 | `E-PLUGIN-STAMP-SURFACE`, did-you-mean 포함 |
| 겹치는 표면에서 한 열을 두 번 선언 | `E-PLUGIN-DUP-ID`, `E-PLUGIN-DUP-ACROSS` |
| 플러그인이 선언하지 않은 `{{playerName}}` | `E-CEL-PROFILE`, 선언된 토큰을 알려 줌 |
| 텍스트 토큰의 숫자 힌트, 또는 그 반대 | `E-REF-TYPE` |
| 어떤 퀘스트도 볼 수 없는 배지 팩트 | 사라짐: `E-QUEST-UNREACHABLE`이 보상 효과를 읽음 |
| 존재하지 않는 퀘스트를 수락하는 보상 | `E-ACCEPT-TARGET`, did-you-mean 포함 |
| 임포트된 하위 트리의 경고 홍수 | `paths:`가 있는 `diagnostics:`, 요약에 집계됨 |
| 무대 밖에 보인 화자 | 임포터가 무대 행을 내보내지 않으면 `W-STAGE-ABSENT` |
| 구조 또는 종속 값 규칙 | `node` lint |
| 검사기가 볼 수 없던 루프 | `::jump{max}`와 `E-JUMP-MAX`, `E-JUMP-LOOP-SCOPE`, `E-JUMP-INTO-LOOP`, `E-JUMP-LOOP-CROSS` |
| 아무것도 raise하지 않는 장면 | `W-OCCASION-NEVER-RAISED`. 잘못 쓴 raise target은 `E-RAISES-TARGET` |
| 어떤 플러그인도 선언하지 않은 occasion의 raise | `E-PLUGIN-RAISES`, did-you-mean 포함 |
| 번역할 수 없는 플레이어용 디렉티브 텍스트 | `type: text`, `lute tag`, `lute loc` |
| id가 같은 두 번역 단위 | `E-LINE-ID-DUP`. `code`의 `~`는 `E-LINE-CODE-RESERVED` |
| 닫히지 않는 여는 요소 | `pair` lint |
| 목록이나 레코드 열 | 거부됨. 스칼라로 나누세요 |

Lute가 알지 **못하는** 것은 다음과 같습니다.

- **스탬프의 값이 여러분의 엔진에서 의미가 있는지.** `voiceAssetId="VO.mira.0010"`은 문자열(카탈로그를 고정하면
  `providerRef`)입니다. 엔진의 에셋 테이블은 여러분의 것입니다.
- **엔진이 토큰으로 무엇을 하는지.** 호스트 토큰은 호스트가 그것을 대체한다는 약속입니다. 호스트가 잊으면 마커가
  텍스트에 남습니다.
- **조용한 하위 트리가 올바른지.** `off`는 경고를 숨길 뿐 행을 고치지 않습니다. `paths:` glob은 좁게 유지하고,
  검토가 끝나면 항목을 지우세요.
- **호스트가 선언된 raise를 수행하는지.** `raises` 항목은 호스트에 대한 약속일 뿐 장면이 재생된다는 보장이 아닙니다. occasion의
  게이트가 거절할 수 있습니다.
- **번역이 올바른지.** `W-L10N-STALE`은 내보낸 뒤 원문이 바뀌었다고 알려 줄 뿐, 번역을 읽지는 않습니다.
