---
title: 대사와 캐스트
description: 콘텐츠 줄 — 대사와 내레이션을 위한 @speaker 구문 — 및 문서 섹션과 안정적인 id, 화자 집합을 닫는 선언된 캐스트(화자별 presence와 감정 포함), 전달 플래그와 mono 시점 규칙, 줄 속성과 voice key, 보간과 서수, 인라인 텍스트 수식자, 표시 이름을 설명합니다.
---

콘텐츠는 씬의 대사와 내레이션 텍스트입니다. 모든 콘텐츠 줄은 같은 형태를 가집니다.

```
@speaker{attributes}: the text they say
```

**화자**는 줄의 종류를 선택합니다.

- 예약된 **`narrator`** → **내레이션**(화자 없음)
- 그 밖의 화자 id → 해당 화자를 담는 **대사**

프런트매터의 `pov`는 씬의 시점 캐릭터를 지정하며, 없으면 프로젝트의 `defaults.pov`를 사용합니다.
이는 해당 화자의 일반 줄이 컴파일되는 방식을 바꾸지 않습니다. 주인공의 줄도 대사이고, 아티팩트에는 `pov`나
플레이어 역할이 들어 있지 않습니다. `pov`는 [`{mono}`](#전달-플래그)를 말할 수 있는 사람을 결정합니다.
주인공을 다르게 렌더링하는 엔진(초상화 없음, 플레이어가 선택한 이름을 라벨로 표시)은 화자 id로 이를 결정합니다.
줄 텍스트에서는 플레이어 이름에 `{{userName}}`을 씁니다.

별도의 생각 노드나 산문 노드는 없습니다 — 역할은 화자와 그 전달 방식(아래 참조)에서 파생됩니다.

```lute
@narrator: Venny's again. The chain restaurant that has never offended anyone.
@marina{code="0010" emotion="delighted" variant="1"}: Mr. Fixer! You came back!
@fixer{code="0010"}: I did.
```

*([`docs/examples/showcase/episode01.lute`](https://github.com/journeyWorker/lute/blob/main/docs/examples/showcase/episode01.lute)에서 가져왔습니다.)*

## 섹션

콘텐츠는 **섹션**에 들어갑니다. 섹션은 `## ` 헤딩에서 시작해 다음 헤딩까지 이어지며, 헤딩 텍스트는 작가와
도구를 위한 것입니다. 문서에는 본문 제목이 없습니다 — 제목은 프런트매터의 `title:`이고, 본문에 `# ` 헤딩을
쓰면 `E-INERT-TITLE`입니다.

헤딩은 끝에 안정적인 id `{#id}`를 붙일 수 있습니다. id는 문자로 시작하고 뒤에 최대 63개의 문자, 숫자,
`-`, `_`가 올 수 있으며 문서의 섹션 사이에서 고유해야 합니다(그렇지 않으면 `E-SECTION-DUP`). id는 헤딩
텍스트의 일부가 아닙니다.

```lute check
---
kind: scene
id: diner.night
title: Night Shift
---

## The Counter {#counter}

@narrator: The diner hums.

## Closing Time

@narrator: The lights go out one by one.
```

아티팩트는 문서 순서대로 섹션을 나열하며, 1부터 번호를 매깁니다.

```json
"sections": [
  {"section": 1, "heading": "The Counter", "id": "counter"},
  {"section": 2, "heading": "Closing Time"}
]
```

섹션의 번호는 그 안의 모든 record `position`의 첫 세그먼트이기도 합니다(`001-0100`은 섹션 1의 첫 record).
position과 번호는 소스 순서를 따르므로 섹션이 이동하면 바뀝니다. 편집을 거쳐도 섹션의 정체성을 유지해야 하는
도구는 `{#id}`를 읽으며, id가 없는 섹션은 안정적인 정체성이 없는 것으로 보고합니다. 섹션 id는 정체성일
뿐입니다. 점프 대상이 아니며 — [`::jump`](/ko/language/directives/#jump와-label--앞으로-이동)는 `::label`로 이동합니다.

## 줄 속성

`{…}` 안의 속성은 콘텐츠 메타데이터입니다. `code`(줄마다 안정적인 id), `emotion`, `variant`, `action`,
`dialogMotion`, `as`(한 줄에만 적용되는 화자 라벨 오버라이드)가 있습니다. 속성의 *도메인*은 문법이 아니라
프로젝트 어휘입니다 — `lute context <file>`로 프로젝트에서 허용하는 `emotion`/`variant` 값을 나열할 수 있습니다.
어느 속성도 필수는 아닙니다. `code`가 없으면 컴파일 시 결정론적으로 나중에 채워지며 `lute tag`로 저장할 수 있습니다.

모든 줄은 `lineId`와 **`voiceKey`**로 컴파일됩니다 — 내레이션과 `{mono}` 줄도 포함됩니다.

```json
{"kind": "line", "family": "content", "position": "001-0100", "role": "narration", "speaker": "narrator", "text": "The diner hums.", "lineId": "diner.night.narrator_0010", "voiceKey": "diner.night.narrator-0010"}
```

voice key는 녹음을 위한 조인 키이지 녹음이 존재한다는 주장이 아닙니다. 내레이션에 음성을 입히지 않는 프로젝트는
그 키에 해당하는 아티팩트가 없을 뿐입니다. 프로젝트에서 같은 `voiceKey`를 공유하는 두 줄은 동일한 텍스트를
말해야 합니다([인라인 수식자](#인라인-텍스트-수식자)를 제거한 뒤 비교). 그렇지 않으면 `check-project`가
`E-DUP-VOICEKEY`를 보고합니다.

`action=`은 **무대** 결과가 있는 유일한 줄 속성이며, 그것도 작습니다. 해당 줄에서 화자의 포즈를 설정하고
화자를 변경된 상태(dirty)로 표시하므로, 같은 화자의 다음 일반 줄 앞에 `posReset`이 삽입됩니다
(`provenance.by: "auto-pose-reset"`). 이것은 무대 규칙이 아니라 전달 세부사항입니다.
**캐릭터의 등장과 퇴장은 `actor`로 낮춰진 모든 스테이징 레코드(`::actor` 포함)**가 담당하며,
`exit: true` 필드가 해당 캐릭터의 presence를 끝냅니다. `::clear`(0.24.0)는 모두의 presence를
끝냅니다([코어 디렉티브](/ko/language/directives/) 참고). `lute context`가 코어 디렉티브의
`actor` 의미론에 `mayExitCharacter`를 나열하는 이유도 이것입니다. 콘텐츠 줄에 `action` 도메인의
`exits:` 목록 멤버를 쓰면 `W-EXIT-INERT`입니다. 포즈는 적용되지만 캐릭터는 무대에 남고 아티팩트에는
`exit` record가 생기지 않습니다. 선언된 actor 퇴장, (플러그인 디렉티브를 포함해) `bg`로 낮춰진
스테이징 레코드, 또는 `::clear`로 캐릭터가 떠난 뒤 actor 레코드로 다시 무대에 올리기 전 그
캐릭터의 줄은 `W-STAGE-ABSENT`입니다. 선택지와 `<match>` 갈래를 통하는 모든 경로에서 판정됩니다
([무대 상태](/ko/language/directives/#무대-상태) 참고).

```lute
@marina{as="???"}: ...who's there?
```

`as`는 해당 줄의 표시 라벨만 덮어씁니다. 없으면 줄은 화자 id를 라벨로 렌더링하고, 엔진은 그 id를 프로젝트
[캐스트](#캐스트)가 제공하는 표시 이름인 `name`에 매핑합니다. 소스에서 라벨을 설정하는 유일한 방법은 `as=`입니다.
더 풍부한 표시 이름 기능 — 의상과 이름 공개를 포함하는
[character/cast 제안](https://github.com/journeyWorker/lute/blob/main/docs/proposals/character-cast/0.0.1.md) — 은 초안이며,
그런 플러그인은 아직 제공되지 않습니다.

## 캐스트

프로젝트는 **캐스트**(dsl 0.23.0)를 선언할 수 있습니다. 캐스트는 줄에서 사용할 수 있는 화자 id의 집합이며,
각 id에 선택적인 표시 이름을 둘 수 있습니다. 스키마 문서는 `cast:` 아래에 이를 선언하고, 스키마를 가져오는
문서가 그에 맞는지 검사됩니다.

```yaml
# cast.schema.yaml
cast:
  mira: { name: Mira }
  oskar: { name: "Oskar Lind" }
  vesna: {}
```

플러그인도 같은 형태의 `cast/*.yaml` 파일을 `cast` export로 제공할 수 있습니다([매니페스트](/ko/plugins/manifests/#캐스트) 참고).
그러면 엔진 팩이 초상화와 음성이 존재하는 캐릭터를 선언할 수 있습니다.

캐스트가 선언되면 모든 화자는 캐스트 안에 있어야 합니다. 씬 줄, 퀘스트 본문, 로어 엔트리,
[번들 비트](/ko/language/beats/#비트-번들)를 모두 검사합니다. 캐스트 밖의 화자는 가까운 오타에 대한
 did-you-mean 제안(예: `@oskr`)과 함께 `E-CAST-UNKNOWN`이 됩니다. `narrator`는 항상 화자입니다. `pov` 화자는
특별하지 않습니다. `pov`가 `fixer`인 씬도 `fixer`가 캐스트에 있어야 합니다.

<!-- lute-diagnostics -->
```
./scenes/arrival.lute:12:2: error [E-CAST-UNKNOWN] speaker `fixer` is not in the declared cast
```

캐스트를 선언하지 않으면 0.23.0 이전과 마찬가지로 화자는 형태만 검사되므로 프로젝트는 캐릭터를 확정하기 전에도
대사를 작성할 수 있습니다. 캐스트는 씬 하나가 아니라 프로젝트에 속합니다. 씬 프런트매터의 `cast:` 키는
`E-META-UNKNOWN-KEY`입니다.

`lute context <file>`는 표시 이름과 함께 캐스트를 나열합니다(`--project <dir>`를 전달하면 플러그인의 캐스트도
포함). 언어 서버는 `@` 뒤에 화자를 완성할 때 캐스트를 제안합니다.

### 두 화자가 공유하는 표시 이름

대화 상자에서 같은 이름으로 표시되는 두 화자는 한 사람처럼 보입니다. 여러 작가가 하나의 프로젝트에 캐릭터를
추가할 때 두 지역에서 각각 `Hiker Gus`를 작성하는 식으로 우연히 발생합니다. `check-project`는 이를 권고
**`W-DISPLAY-NAME-DUP`**(dsl 0.26.0 §2.8)으로 보고하며, `lute lint`도 보고합니다. 다음 두 경우처럼
정확히 같은 `name:`을 가진 캐스트 항목, 컴포넌트의 `::use{… name="…"}` 표시 문자열과 같은 캐스트 이름,
서로 다른 화자에 대한 두 `who=` 문자열을 비교합니다.

<!-- lute-diagnostics -->
```
./scenes/a.lute:11:1: warning [W-DISPLAY-NAME-DUP] display name `Hiker Gus` is shown for 2 different speakers: `gus` (./scenes/a.lute:10), `gus2` (./scenes/a.lute:11) — the dialogue box cannot tell them apart; rename one
```

일부 이름은 의도적으로 공유합니다. 예를 들어 악당 팀의 계급처럼 여러 화자가 맡는 역할입니다. 그런 항목에
**`sharedName: true`**를 표시하면 집계되지 않습니다.

```yaml
cast:
  gus:    { name: Hiker Gus }
  gus2:   { name: Hiker Gus Jr. }
  grunt1: { name: Eclipse Grunt, sharedName: true }
  grunt2: { name: Eclipse Grunt, sharedName: true }
```

플러그인의 `cast/*.yaml` 항목도 같은 키를 받습니다. `check-project --deny-warnings`를 실행하는 병합 게이트는
의도한 역할 이름에 표시를 붙이면 다시 통과하고, `lute lint --deny W-DISPLAY-NAME-DUP`는 이를 자체적으로 코드로
승격합니다. [여러 작가 프로젝트](/ko/guides/multi-author/)를 참고하세요.

### 연출도 캐스트에 맞춰 검사됩니다

캐스트가 선언되면 연출 디렉티브가 지정하는 캐릭터도 화자와 마찬가지로 캐스트 안에 있어야 합니다(dsl 0.24.0 §4).
캐스트 밖의 `::actor{character}`와 `::camera{focus}`는 동일한 did-you-mean과 함께 `E-CAST-UNKNOWN`입니다.
타임라인 클립, `<match>` 갈래와 선택지의 본문도 검사됩니다.

<!-- lute-diagnostics unverified="composed in crates/lute-check/src/cast.rs from a runtime-built directive prefix plus a pushed did-you-mean, so no single format! literal pins it; copied verbatim from check-project output" -->
```
./scenes/ridge.lute:11:20: error [E-CAST-UNKNOWN] `::actor{character}` `corvn` is not in the declared cast — did you mean `corvin`?
./scenes/ridge.lute:12:17: error [E-CAST-UNKNOWN] `::camera{focus}` `isold` is not in the declared cast — did you mean `isolde`?
```

### 화자별 감정

캐스트 항목에는 캐릭터가 표현하는 감정 `emotions: [calm, fierce]`을 나열할 수 있습니다(dsl 0.24.0 §4).
각 멤버는 프로젝트의 `emotion` enum 멤버이기도 해야 합니다. 해당 화자의 줄에서, 또는 리터럴 `character=`가
그 화자를 지정한 디렉티브에서 목록 밖의 `emotion=`을 쓰면 `emotion` enum에 그 값이 있더라도 `E-BAD-ENUM`입니다.

<!-- lute-diagnostics -->
```
./scenes/ridge.lute:14:18: error [E-BAD-ENUM] `sad` is not one of `isolde`'s emotions (expected one of: calm, fierce) — the cast declares `emotions:` for `isolde`
```

`emotion` enum 자체가 거부하는 값은 기존의 오류 하나를 그대로 냅니다. `emotions:`가 없는 화자는 enum의 어떤
멤버든 사용할 수 있습니다.

### Presence: `present:`

파티 게임에서 동료가 항상 곁에 있어 말할 수 있는 것은 아닙니다. 캐스트 항목은 조건인
`present: "<condition>"`으로 캐릭터의 존재 시점을 지정할 수 있습니다(dsl 0.24.0 §4). 조건은 일반 상태나
팩트이므로 파티 멤버십에 새 메커니즘이 필요하지 않습니다.

```yaml
# world.schema.yaml
entities:
  person: { members: [isolde, corvin] }
relations:
  inParty: { args: [person], tier: run }
cast:
  isolde: { name: Isolde, present: "holds('inParty', ['isolde'])", emotions: [calm, fierce] }
  corvin: { name: "Corvin Hale" }
```

플러그인의 `cast/*.yaml` 항목도 같은 두 키를 받습니다([매니페스트](/ko/plugins/manifests/#캐스트) 참고).

그러면 검사기는 `isolde`가 말하는 모든 줄에 대해 줄을 둘러싼 가드가 그녀의 `present:` 조건을 **함의하는지** 묻습니다.
그렇지 않으면 줄은 **`W-CAST-ABSENT`**입니다.

```lute
@corvin: Where is Isolde?
@isolde: Right here.
@isolde{when="holds('inParty', ['isolde'])"}: Right behind you.
<branch id="ask">
  <choice id="call" text="Call her over" when="holds('inParty', ['isolde'])">
    @isolde{emotion="calm"}: I came.
    ::retract{ inParty(isolde) }
    @isolde: And now I'm leaving.
  </choice>
  <choice id="wait" text="Wait">
    @corvin: We wait, then.
  </choice>
</branch>
::assert{ inParty(isolde) }
@isolde: Back again.
```

`@isolde: Right here.`에는 가드가 없으므로 경고합니다. 다음 줄과 `I came.`은 조건 자체로 가드됩니다.
`And now I'm leaving.`은 선택지의 가드와 줄 사이에서 `::retract`가 일어나 가드가 말하던 조건을 거짓으로
만들기 때문에 다시 경고합니다. 메시지는 조건과 이를 만족할 가드를 이름으로 표시합니다.

<!-- lute-diagnostics -->
```
./scenes/camp.lute:12:2: warning [W-CAST-ABSENT] `isolde` may not be here: the cast declares `present: "holds('inParty', ['isolde'])"` for `isolde`, and the guards around this line do not imply it. Guard the line — `@isolde{when="holds('inParty', ['isolde'])"}` — or move it under a guard that implies it
```

유효한 가드는 다음과 같습니다. 줄 자체의 `when=`, (branch 또는 hub 안에서) 감싸는 모든 `<choice when>`,
`<match>` 갈래(갈래의 `is=` 또는 `test`, 그리고 앞선 갈래가 일치하지 않았다는 사실), `<on when>`,
`<objective done>`, 씬 비트의 `when:`, 엔트리의 `when=` 또는 번들 비트의 `when=`입니다. 그 안의 `@def`는
확장됩니다. 줄이 놓인 위치만으로 암시되는 가드도 있습니다.

- 로어 엔트리 본문은 자신의 `entry.<id>.read` / `everRead`를 전제로 합니다.
- 퀘스트 `<on>` 핸들러는 해당 이벤트에서 퀘스트의 상태를 전제로 하며, 한 번 참이 되면 유지되는 `start`의 결합항
  (`entry.<id>.everRead`, `visited(…)`)도 전제로 합니다.
- 퀘스트의 `questComplete` 핸들러도 퀘스트 완료를 전제로 합니다(dsl 0.27.0). 퀘스트는 필수 objective 중 하나가
  완료되면 완료되므로, 필수 objective 하나의 `done`이 참입니다. 필요한 objective 모두가 `@withToby`를 요구하는
  퀘스트의 핸들러라면 Tobias에게 줄을 주면서 가드를 반복하지 않아도 됩니다. objective 본문이나 이 퀘스트의
  이전 `questComplete` 핸들러에 있는 `::assert`, `::retract`, `::set`은 다른 가드와 마찬가지로 이 전제를 취소할
  수 있습니다.
- `check-project`에서는 같은 계기에 대해 항상 자격이 있는 `once` 비트 중 자신보다 순위가 높은 비트가 이미
  소진되었다고 전제합니다.

파생 관계에 대한 `holds(A)` 가드는 A의 규칙 본문도 함의합니다. `inParty(P) :- recruited(P), not departed(P)`라면
`holds('inParty', ['isolde'])`로 가드된 줄은 `present: "!holds('departed', ['isolde'])"`를 만족합니다. 한 번
바인딩되면 ground로 고정되는 관계의 규칙(예: `cel()`만 사용하는 일정)은 규칙 본문의 논리합으로 읽힙니다.

쓰기가 가드를 취소하는 것은 그 경로에서 가드를 거짓으로 만들 수 있을 때뿐입니다. `::assert`나 `::retract`는
같은 관계의 통합 가능한 인수를 가진 가드 atom을 무효화하거나, 작성한 관계를 전제로 하는 규칙을 통해 유도된
atom을 무효화합니다. 그것도 atom을 잘못된 방향으로 이동할 때만 그렇습니다. `inParty(corvin)`을 assert해도
`holds('inParty', ['isolde'])` 가드는 유지되며, 그 가드의 양의 전제를 assert해도 유지됩니다. `::set`은 해당 경로를
읽는 가드를, 그런 규칙을 통한 경우를 포함해 무효화합니다. 가드의 각 `&&` 결합항은 자체적으로 판정되며, 한
`<choice>`나 `<match>` 갈래의 쓰기는 형제 갈래에 도달하지 않습니다.

`{vo}` 줄은 예외입니다. voice-over는 화자가 그 자리에 있다는 뜻이 아니므로 기억이나 장면 위의 내레이션처럼
씬의 시간 밖에서 말할 수 있습니다. `{os}` 줄은 검사됩니다(dsl 0.25.0 D-E). `{os}`는 화자가 *씬 안에* 있지만
화면 밖에서 들린다는 뜻이므로 presence가 있어야 합니다. 라디오나 전화 너머의 원격 화자처럼 별도의 원격 화자
플래그는 아직 없습니다.

팩트는 `lute check`와 `check-project`가 다른 유일한 곳입니다. `check-project`는 줄까지 이어지는 모든 경로에서
참인 팩트도 셉니다. 모든 경로에서 앞서 assert되었거나, seed된 뒤 retract되지 않은 팩트입니다. 따라서 위 예시의
마지막 줄 `::assert{ inParty(isolde) }` 뒤의 `@isolde: Back again.`은 그곳에서는 깨끗합니다. 단일 파일
`lute check`는 그런 팩트를 볼 수 없으므로 그 줄에도 경고하고, `check-project`라면 볼 수 있다는 참고 사항을 추가합니다.
다른 경고와 마찬가지로 `--deny W-CAST-ABSENT`는 이를 오류로 올립니다.

Presence에는 엔진만 쓰는 두 번째 조건이 붙는 경우가 많습니다. 예를 들어 "파티에 있고, 쓰러지지 않았다"인
`present: "holds('inParty', ['isolde']) && !holds('fell', ['isolde'])"`이며 `fell`은 `reserved: true` 관계입니다.
검사기는 엔진이 `fell`을 언제 쓰는지 모르므로, 도움 없이는 각 줄 또는 비트에 가드가 있어야 그 절반을 만족하고
`isolde`의 모든 줄에서 경고합니다. **`assume: true`**(dsl 0.24.0 §4)를 추가한 캐스트 항목은 `present:`의
`reserved:` 관계에 대한 모든 부정 `holds`를 참으로 읽습니다. 직접 부정했든
`inParty(P) :- recruited(P), not fell(P)` 같은 규칙을 통해 부정했든 동일합니다. 그러면 위 줄들은 일반
`holds('inParty', ['isolde'])` 조건만 있었을 때와 정확히 같이 경고합니다. 대가는 엔진 이벤트 뒤에 말하는 줄에는
자체 가드가 필요하다는 것입니다. 단, 관계가 변경되는 이벤트를 [`changedOn`](#엔진-이벤트-이후의-presence-changedon),
dsl 0.25.0으로 이름 붙이면 예외입니다.

```yaml
cast:
  isolde: { name: Isolde, present: "holds('inParty', ['isolde']) && !holds('fell', ['isolde'])", assume: true }
```

`present:`, `emotions:`, `assume:`은 검사기의 입력입니다. 어느 것도 컴파일된 아티팩트에 들어가지 않습니다.
### 엔진 이벤트 이후의 presence: `changedOn`

`assume: true`는 예약된 관계가 의미하는 엔진 이벤트가 일어나기 전까지는 올바릅니다. 전투 후에는 `fell(isolde)`가 참일 수 있고, 바로 그곳에서 검사가 중요해집니다. 예약된 관계는 엔진이 어떤 계기에서 그 값을 바꾸는지 말할 수 있습니다(dsl 0.25.0 §6):

```yaml
entities:
  companion: { members: [isolde] }
relations:
  inParty: { args: [companion], tier: run }
  fell:    { args: [companion], tier: run, reserved: true, changedOn: [battleEnd] }
cast:
  isolde: { name: Isolde, present: "holds('inParty', ['isolde']) && !holds('fell', ['isolde'])", assume: true }
```

`changedOn`이 있으면 `assume: true`는 해당 계기에서 제시되는 단위에서 더 이상 `!holds('fell', ['…'])`를 참으로 읽지 않습니다. 여기에는 `on: battleEnd`에 응답하는 씬, 엔트리 또는 번들 비트, 그리고 그곳에서 판정되는 퀘스트 `<on event>` 핸들러나 `on=` 목표 본문이 포함됩니다. `check-project`에서는 [시나리오 그래프](/ko/connectivity/scene-graph/)에서 그러한 단위의 **이후 자손(after-descendant)** 모두에 대해서도 마찬가지이며, `after:` / `after=` / `[start]` 간선이 적용됩니다. 계기 자체에는 정적인 순서가 없으므로, 무엇이 “전투 이후”인지 말해 주는 것은 그래프입니다. `inParty(isolde)`를 단언하는 캠프 씬, `after: 'visited("camp")'`인 `battleEnd`의 여파 씬, 그리고 `after: 'visited("aftermath")'`인 길 씬을 생각해 보세요:

```lute
@isolde{when="holds('inParty', ['isolde'])"}: We should keep moving.
@isolde{os}: Wait for me!
@isolde{vo}: I remember that road.
```

길에서는 첫 두 줄이 다시 경고되며, 각각 `assume: true`가 적용되지 않은 이유를 말하는 참고 사항이 붙습니다. `{vo}` 줄은 계속 예외입니다:

<!-- lute-diagnostics unverified="verbatim lute check-project output; the W-CAST-ABSENT message is composed from a base literal plus the dsl 0.25.0 §6 changedOn suffix appended in crates/lute-check/src/cast.rs, so no single format! literal matches" -->
```
./scenes/road.lute:11:2: warning [W-CAST-ABSENT] `isolde` may not be here: the cast declares `present: "holds('inParty', ['isolde']) && !holds('fell', ['isolde'])"` for `isolde`, and the guards around this line do not imply it. Guard the line — `@isolde{when="holds('inParty', ['isolde']) && !holds('fell', ['isolde'])"}` — or move it under a guard that implies it; `assume: true` does not cover `fell`: this line follows an occasion its `changedOn:` names
```

그 줄들에 전체 조건인 `@isolde{when="holds('inParty', ['isolde']) && !holds('fell', ['isolde'])"}`으로 가드를 붙이면 경고가 사라집니다. 그래프에서 전투보다 앞에 있는 캠프 씬의 줄은 `assume: true`의 적용을 계속 받습니다. `changedOn`이 없으면 0.24 동작은 변하지 않으며, `assume: true`가 모든 줄에 적용됩니다. `reserved: true`가 아닌 관계에 `changedOn`을 쓰면 `E-RELATION-DECL`이고, 플러그인이 선언하지 않은 계기도 did-you-mean과 함께 같은 오류가 됩니다(`` `changedOn: batleEnd` is not a declared occasion — did you mean `battleEnd`? ``). 프로젝트의 플러그인이 계기를 하나도 선언하지 않으면 이름을 검사하지 않습니다.

### 무대에서 내보내기: `::clear`

presence는 플레이어와 *함께 있는* 사람이 누구인지에 대한 것입니다. *무대에 올라와 있는* 사람이 누구인지는 무대 상태입니다([무대 상태](/ko/language/directives/#무대-상태) 참고). `::actor`는 캐릭터를 무대에 올리고, 선언된 퇴장이나 `::bg`가 캐릭터를 무대에서 내립니다. 0.24.0부터 말단 `::clear`는 배경과 음악은 그대로 둔 채 모두를 한 번에 무대에서 내립니다([핵심 디렉티브](/ko/language/directives/#clear--무대-비우기) 참고). `::actor`로 다시 보여 주기 전에, 무대에서 지워진 캐릭터가 말하는 줄에는 `W-STAGE-ABSENT`가 표시되고 경고는 `::clear`를 지목합니다.

## 전달 플래그

**전달 플래그(delivery flag)**는 중괄호 안의 맨 단어(`=value` 없음)로, 줄이 전달되는 방식을 바꿉니다:

- **`{mono}`** — 속마음/생각(씬 안에서 소리 내어 말하지 않음).
- **`{os}`** — 화면 밖(off-screen): 화자의 목소리는 들리지만 현재 무대에 올라와 있거나 보이는 상태는 아님.
- **`{vo}`** — 보이스오버(voice-over): 씬 위에 겹쳐지는 내레이션 방식의 전달.

```lute
@fixer{mono}: An android, then. Which would, on reflection, explain the ramen.
```

세 플래그는 **상호 배타적**이며 한 줄에 최대 하나만 쓸 수 있습니다(둘을 함께 쓰면 `E-DELIVERY-CONFLICT`). 또한 `@narrator`에는 어느 것도 허용되지 않습니다(`E-DELIVERY-NARRATOR`).

역할은 화자와 전달 방식에서 파생되며, 아티팩트의 `role`도 같은 단어를 사용합니다: `narrator` → `narration`, `{mono}` → `mono`, `{os}` → `os`, `{vo}` → `vo`, 그 밖의 모든 캐릭터 → `dialogue`입니다.

### `{mono}`로 말할 수 있는 사람

`{mono}` 줄은 시점 인물의 속마음입니다. **유효 POV**는 문서의 `pov:`이고, 없으면 프로젝트의 `defaults.pov`입니다. 화자가 유효 POV이거나 **`monoSpeakers:`**에 나열되어 있을 때 `{mono}` 줄이 유효합니다. `monoSpeakers:`는 문서 프런트매터의 화자 id 목록이며, 프로젝트의 `defaults:`에 두어 모든 문서에 적용할 수도 있습니다(문서 자체의 목록이 우선합니다). 두 번째 주인공처럼 내면의 목소리를 공유하는 씬에 사용하세요:

```lute check
---
kind: scene
id: diner.night
pov: fixer
monoSpeakers: [mira]
---

## The Counter

@fixer{mono}: Another night, another coffee.
@mira{mono}: He looks tired.
@mira{os}: Order up!
@fixer{vo}: I remember that night.
```

`monoSpeakers: [mira]`가 없으면 `@mira{mono}`는 `E-MONO-POV`입니다. POV가 전혀 해석되지 않을 때 — `pov:`도 `defaults.pov`도 없을 때 — `monoSpeakers` 밖의 화자가 쓴 `{mono}` 줄은 `E-MONO-NO-POV`입니다:

<!-- lute-diagnostics -->
```
diner.lute:10:7: error [E-MONO-POV] `@mira{mono}` is an interior monologue, but `mira` is not this document's point of view (`fixer`) and not in `monoSpeakers:` (which lists none) — only the POV character and the speakers `monoSpeakers:` lists may speak `mono`; add `mira` to `monoSpeakers:`, or write the line as dialogue, `{os}` or `{vo}`
```

[컴포넌트](/ko/language/components-and-extends/)의 `{mono}` 줄은 각 `::use`에서 호출하는 문서의 POV와 `monoSpeakers`를 기준으로 검사됩니다. 오류는 `::use` 위치에 보고되며 컴포넌트의 해당 줄을 지목합니다.

## 보간

콘텐츠 `Text`와 `<choice>`의 `text`에는 렌더링 시 게임 상태를 읽는 **`{{…}}`** 보간을 넣을 수 있습니다:

```lute
@narrator: Good to see you, {{userName}}.
@marina{code="0010" emotion="delighted" variant="1"}: You came back! Warmth so far: {{run.affection}}.
```

*([`docs/examples/showcase/hub-demo.lute`](https://github.com/journeyWorker/lute/blob/main/docs/examples/showcase/hub-demo.lute)에서 가져왔습니다.)*

`{{userName}}`은 항상 사용할 수 있는 예약 토큰입니다. 그 밖의 보간은 **선언된** 상태 경로를 지정해야 합니다. 보간은 확정 할당 분석에서 *읽기*이므로, 가드 없이 설정되지 않을 수 있는 경로를 보간하면 `E-MAYBE-UNSET`입니다. 보간과 [인라인 수식자](#인라인-텍스트-수식자)를 제외하면 두 번째 콜론 뒤의 텍스트는 줄 끝까지 불투명합니다. 괄호, `<`, `//`, `#`, 단일 중괄호 등은 모두 리터럴이며 파싱되지 않습니다. 검사기는 Ink 또는 Yarn에서 마크업으로 쓰이는 모양을 플레이어가 보게 되는 경우 경고합니다. 단일 중괄호 `{run.oil}` 또는 `{run.oil > 0: …}`(`W-TEXT-SINGLE-BRACE`; 보간은 `{{run.oil}}`, 조건부 텍스트는 가드된 줄), ` // note`나 끝의 `# tag`(`W-TEXT-COMMENT-LIKE`; 주석은 자체 줄에 `//`를 씁니다)가 그 예입니다. [Ink 또는 Yarn에서 넘어오기](/ko/guides/coming-from-ink-yarn/)를 참고하세요.

표시 `labels:`가 붙은 enum에 대해 타입이 지정된 경로는 멤버 id가 아니라 라벨을 렌더링합니다: `Today is {{run.weekday}}.`는 `Today is Sunday.`로 읽힙니다([이름 붙은 enum으로 타입 지정된 경로](/ko/state/state-model/#이름-있는-열거형으로-타입을-지정한-경로) 참고).

### 서수: `{{x:ordinal}}`

숫자 값은 콜론 뒤의 **형식 힌트(format hint)**로 영어 서수로 렌더링할 수 있습니다(dsl 0.24.0 §4):

```lute check
---
kind: scene
id: tower.gate
state:
  user.climbs: { type: int, default: 1 }
---

## The Gate

@narrator: Welcome back, {{userName}}. This is your {{user.climbs:ordinal}} climb.
```

`lute run`, `lute play`, `lute trace`, `lute test`는 `1st` `2nd` `3rd` `4th` … `11th` `12th` `13th` … `21st` `22nd` … `101st` `111th`를 렌더링합니다. 서수가 없거나, 분수이거나, 음수인 값은 그대로 렌더링됩니다(`2.5`, `-3`). 아티팩트에서 줄의 플레이스홀더는 `"format": "ordinal"`을 가지며 엔진이 렌더링합니다. 힌트가 없는 플레이스홀더에는 `format` 키가 없으므로, 이를 사용하는 아티팩트는 변경되지 않습니다.

### 서수 단어: `{{x:ordinalWord}}`

`:ordinalWord`(dsl 0.25.0 §8)는 서수를 단어로 렌더링하여 글자로 풀어 쓴 편이 자연스러운 텍스트에 사용합니다:

```lute check
---
kind: scene
id: road.camp
state:
  run.day: { type: int, default: 1 }
---

## Camp

@narrator: The {{run.day:ordinalWord}} night on the road.
```

IR 플레이스홀더는 `"format": "ordinalWord"`를 가지며 엔진이 단어를 지역화합니다. `lute run`, `lute play`, `lute trace`, `lute test`는 1–20에 대해 영어 단어 `first` `second` … `twentieth`를 렌더링하고, 그 범위를 벗어나면 `:ordinal` 숫자로 대체합니다(`0th`, `21st`). 검사기는 `:ordinal`이 허용되는 모든 곳에서 `:ordinalWord`를 허용합니다.

### 수사 단어와 복수형: `{{x:cardinalWord}}`, `{{x:plural(one|other)}}`

`:cardinalWord`는 숫자를 `one` `two` … `twenty`로 풀어 쓰고 그 범위를 벗어나면 숫자로 대체합니다(`0`, `21`). 검사기는 `:ordinal`이 허용되는 모든 곳에서 이를 허용합니다. `:plural(singular|plural)`은 숫자가 1일 때 첫 번째 형식을, 그 밖에는 두 번째 형식을 고릅니다. 형식 안에서 `#`는 숫자, `#word`는 숫자의 기수 단어, `#Word`는 문장 첫머리에 맞춘 대문자 기수 단어입니다:

```lute check
---
kind: scene
id: road.wagons
state:
  run.wagons: { type: int, default: 11 }
---

## Camp

@narrator: {{run.wagons:plural(One wagon waits|#Word wagons wait)}} at the ford, {{run.wagons:cardinalWord}} in all.
```

이 줄은 `Eleven wagons wait at the ford, eleven in all.`로 읽힙니다. 복수형은 `|`로 구분하는 평문입니다. 따옴표로 감싼 형식(`plural("One wagon"|"# wagons")`)은 따옴표를 출력하고, `,` 구분자(`plural(wagon,wagons)`)는 하나의 형식이므로 둘 다 `E-PLURAL-FORM`이며 메시지에 수정 방법이 표시됩니다.

### 텍스트 힌트와 라벨 형식: `{{x:capitalize}}`, `{{x:start}}`, `{{x:indefinite}}`

세 힌트가 문자열, enum 또는 종류 라벨, 캐스트 이름, `{{occasion.target}}`, `{{userName}}`을 형식화합니다. `:capitalize`는 첫 글자를 대문자로 바꿉니다. `:start`와 `:indefinite`는 종류의 `labels:` 항목이 텍스트와 함께 선언할 수 있는 형식을 읽습니다:

```yaml
entities:
  spot:
    members: [cut, den]
    labels:
      cut: { text: "the smugglers' cut", start: "The smugglers' cut", indefinite: "a smugglers' cut" }
      den: ashwraith den
```

`{{occasion.target:start}} is quiet.`는 `cut`에 대해 `The smugglers' cut is quiet.`로 읽힙니다. 형식이 없으면 힌트가 대체 동작을 합니다. `:start`는 텍스트를 대문자로 만들고(`Ashwraith den`), `:indefinite`는 첫 글자가 모음이면 `an`, 그 밖에는 `a`를 텍스트 앞에 붙입니다(`an ashwraith den`). 규칙이 틀리는 단어(`an hour`, `a unicorn`)에는 `indefinite:`를 선언하세요. 라벨 항목은 문자열이거나 `{ text, start, indefinite }`이며 `text:`가 필수입니다. 다른 키는 모두 `E-ENTITY-KIND-SHAPE`입니다. IR은 힌트를 플레이스홀더의 `format`으로, 형식을 종류의 `entities[]` 항목과 종류로 타입 지정된 상태 경로의 `labelForms`로 전달하므로 엔진이 지역화할 수 있습니다. 리터럴로 삽입된 컴포넌트 매개변수는 대체 형식을 사용합니다.

힌트는 `int` 또는 `double`에 대한 `ordinal`, `ordinalWord`, `cardinalWord`, `plural`과 텍스트에 대한 `capitalize`, `start`, `indefinite`입니다. 그 밖의 힌트 — `{{run.floor:roman}}`, 잘못 쓴 `{{run.day:ordinalword}}` — 는 `E-CEL-PROFILE`입니다. 문자열, enum 또는 bool 경로/def에 숫자 힌트를 쓰거나 `{{userName}}`에 숫자 힌트를 쓰면 `E-REF-TYPE`이고, `int`, `double`, bool에 텍스트 힌트를 써도 같습니다.
## 인라인 텍스트 수식자

줄의 텍스트는 표현을 위해 일부를 마크업할 수 있습니다 — 스타일이 지정된 단어, 일시 정지, 느려진 구절 등입니다. **수식자(modifier)**는 콜론, lowerCamel 이름, 그리고 대괄호로 감싼 스팬, 중괄호 속성 목록 또는 둘 다로 이루어집니다:

- **스팬** — `:name[text]`는 텍스트를 감쌉니다.
- **말단** — `:name{attrs}`는 텍스트의 한 지점을 표시합니다.
- **속성이 있는 스팬** — `:name[text]{attrs}`.

수식자는 콘텐츠 줄의 두 번째 콜론 뒤 텍스트에서만 유효하며, 헤딩, 속성, 태그 텍스트 또는 `<choice>`의 `text`에서는 절대 유효하지 않습니다. 코어에는 다음 두 가지가 있습니다:

| 수식자 | 형식 | 의미 |
|---|---|---|
| `:pause{s=0.5}` | 말단 | `s`초 동안 일시 정지(`s` 필수, 음이 아닌 값) |
| `:speed[text]{rate=1.25}` | 스팬 | 감싼 텍스트를 `rate` 속도로 전달(`rate` 필수, 양수) |

그 밖의 모든 이름은 프로젝트의 `textStyle` 도메인에 속하는 **텍스트 스타일**이며, 다른 [연출 어휘](/ko/language/directives/#연출-어휘는-프로젝트가-소유합니다)와 마찬가지로 `enums:`에서 선언합니다. 스타일은 속성이 없는 스팬입니다. `emphasis`나 `whisper`를 어떻게 보일지는 엔진의 역할입니다.

```lute check
---
kind: scene
id: diner.night
pov: fixer
enums:
  textStyle: [emphasis, whisper]
state:
  run.tips: { type: int, default: 3 }
---

## The Counter

@mira: You look :emphasis[tired].:pause{s=0.5} Coffee?
@fixer: :speed[Just... one more cup.]{rate=0.75}
@mira: :whisper[I counted :emphasis[{{run.tips}}] tips tonight.]
@fixer: Write \:emphasis[this] to show the markup itself.
@fixer: The sign says 10:30 :) and nothing else.
```

아티팩트에서 `text`는 항상 **평문**입니다. 수식자 마크업은 제거되고, 이스케이프는 디코딩되며, `{{…}}`는 유지됩니다. 수식자를 사용하는 줄에는 활성 `styles`(바깥쪽부터)와 `rate`를 담은 텍스트 실행 구간, 그리고 `{pause}` 말단으로 이루어진 `segments`도 생깁니다. 수식자가 없는 줄에는 `segments`가 없습니다:

```json
{"kind": "line", "family": "content", "position": "001-0100", "role": "dialogue", "speaker": "mira", "text": "You look tired. Coffee?", "lineId": "diner.night.mira_0010", "voiceKey": "diner.night.mira-0010", "segments": [{"text": "You look "}, {"text": "tired", "styles": ["emphasis"]}, {"text": "."}, {"pause": 0.5}, {"text": " Coffee?"}]}
{"kind": "line", "family": "content", "position": "001-0200", "role": "dialogue", "speaker": "fixer", "text": "Just... one more cup.", "lineId": "diner.night.fixer_0010", "voiceKey": "diner.night.fixer-0010", "segments": [{"text": "Just... one more cup.", "rate": 0.75}]}
{"kind": "line", "family": "content", "position": "001-0300", "role": "dialogue", "speaker": "mira", "text": "I counted {{run.tips}} tips tonight.", "lineId": "diner.night.mira_0020", "voiceKey": "diner.night.mira-0020", "placeholders": [{"kind": "path", "path": "run.tips"}], "segments": [{"text": "I counted ", "styles": ["whisper"]}, {"text": "{{run.tips}}", "styles": ["whisper", "emphasis"]}, {"text": " tips tonight.", "styles": ["whisper"]}]}
{"kind": "line", "family": "content", "position": "001-0400", "role": "dialogue", "speaker": "fixer", "text": "Write :emphasis[this] to show the markup itself.", "lineId": "diner.night.fixer_0020", "voiceKey": "diner.night.fixer-0020"}
```

위 예제의 규칙은 줄마다 다음과 같습니다:

- **중첩.** 스팬은 중첩될 수 있고 보간은 스팬 안에 들어갈 수 있으며 `text`와 `placeholders`에 그대로 남습니다. 중첩 스타일은 쌓이고(`["whisper", "emphasis"]`) 중첩된 `:speed`는 가장 안쪽의 `rate`를 사용합니다. 스타일과 rate가 같은 인접 실행 구간은 합쳐집니다.
- **이스케이프.** `\:`는 수식자를 시작하지 않는 리터럴 콜론입니다. 수식자 안에서 `\[`, `\]`, `\{`, `\}` 및 `\\`는 괄호, 중괄호 또는 백슬래시를 씁니다. 그 밖의 이스케이프는 `E-TEXT-ESCAPE`입니다. `\{{` 리터럴 보간 이스케이프는 그대로입니다.
- **평범한 콜론은 텍스트로 남습니다.** 콜론 뒤에 이름과 `[` 또는 `{`가 이어질 때만 수식자를 시작하므로 `10:30`과 `:)`에는 이스케이프가 필요 없습니다.

형식이 잘못된 수식자는 `E-TEXT-MODIFIER`입니다. 여기에는 닫히지 않은 스팬, 코어 수식자도 `textStyle` 멤버도 아닌 이름, 스타일에 붙인 속성, 위치 인자(`:pause{0.5}` — `s=0.5`로 쓰세요)가 포함됩니다:

<!-- lute-diagnostics unverified="verbatim lute check output; crates/lute-check/src/content_line.rs splices a did-you-mean `{hint}` mid-message, empty here since no member is near `shout`, so no single format! literal matches" -->
```
diner.lute:11:33: error [E-TEXT-MODIFIER] inline modifier span is missing a closing `]`
diner.lute:12:10: error [E-TEXT-MODIFIER] `:shout` is neither a core modifier (`pause`, `speed`) nor a member of `textStyle` (it has `emphasis`)
diner.lute:13:26: error [E-TEXT-MODIFIER] text style `:emphasis` takes no attributes, got `size` — only the core `pause`/`speed` carry attributes
```

보이스 키는 평문을 비교하므로 줄의 스타일만 바꾸어도 녹음과 충돌하지 않습니다. [`lute loc export`](/ko/tooling/cli/#loc-export)는 소스 마크업을 내보내며, 번역은 동일한 수식자를 전달해야 합니다 — 이름, 형식, 속성이 같아야 하고 순서는 달라도 됩니다 — 그렇지 않으면 병합되지 않습니다(`E-L10N-MODIFIERS`). 번역된 로케일에는 항상 `texts`에 평문이 들어가며, 수식자가 있는 줄에는 해당 로케일의 `localeSegments`도 들어갑니다.

export의 각 행에는 `sourceHash`도 있습니다 — 줄의 평문(마크업과 이스케이프를 제거한, 컴파일된 `text`), 선택지 텍스트 또는 프롬프트의 `"sha256:<hex>"`입니다. 번역 파일에 그대로 두세요. [`lute loc import`](/ko/tooling/cli/#loc-import)가 이를 lineId와 로케일별로 번들(`schemaVersion: 2`, `sourceHashes`)에 저장하고, `lute compile --locales`는 번역의 바탕이 된 export 이후 줄이 수정되었으면 `W-L10N-STALE`을 보고합니다 — 다시 내보내 그 번역을 갱신하세요. 갱신하기 전까지 오래된 번역도 그대로 병합됩니다.
