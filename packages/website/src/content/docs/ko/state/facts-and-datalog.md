---
title: 사실과 Datalog
description: Lute의 스칼라 계층 옆에 있는 관계형 사실 계층 — 선언된 엔터티, 하위 종류와 관계, 서로 배제하는 관계(excludes:), 델타로 주장하고 철회하는 그라운드 사실, 그리고 설계 단계에서 종료가 보장되는 전체 Datalog 도출 계층을 설명합니다.
---

스칼라 계층은 크기를 저장하지만 *엔터티 간 관계* — "Shadowheart가 파티에 있습니다", "플레이어가 Halsin에게 숲에 대해 말했습니다" — 를 표현할 수 없습니다. Lute는 스칼라 계층 옆에 **관계형 사실 커널**을 추가합니다. 이는 선언된 유한 어휘를 기반으로 하는 폐쇄형 n항 사실 데이터베이스입니다. 문서가 관계를 선언하기만 하면 이 기능을 사용하며, 관계가 하나도 없는 문서는 이전과 정확히 동일하게 동작합니다.

## 엔터티, 관계, 그라운드 사실

엔터티 **종류**는 멤버(`members:`)를 열거하거나 엔진 개방형(`open: engine`)으로 선언합니다. **관계**는 고정된 차수와 타입이 지정된 인자 시그니처를 가진 선언형 술어이며, 각 인자는 엔터티 종류, 이름 있는 열거형 또는 `bool` 범위에 속합니다. **사실**은 일치하는 엔터티에 관계를 적용한 것으로, *기호적*입니다(성립하거나 성립하지 않으며, 숫자 값 슬롯은 없습니다). 모든 사실은 유효 시간 구간을 가지므로, 철회는 삭제가 아니라 무덤표(tombstone)입니다.

```yaml
entities:
  character: { members: [shadowheart, halsin, player] }
  location:  { members: [camp, grove, moonrise] }
relations:
  inParty:    { args: [character], tier: run }
  atLocation: { args: [character, location], tier: run, key: [0] }
  canReach:   { args: [character, location], derive: true }
  wounded:    { args: [character], tier: run, reserved: true }   # asserted by the engine only
facts:
  - "inParty(shadowheart)"
  - "atLocation(player, camp)"
```

콘텐츠는 리프 지시어 `::assert`와 `::retract`로 **델타**를 기록하며, 엔진은 누적된 시간 범위 뷰를 유지합니다. 함수형 `key:`는 대체된 사실을 자동으로 무효화합니다. 둘 중 `::retract`만 와일드카드(`_`)를 허용하며, 쿼리와 규칙 본문도 와일드카드를 사용합니다([익명 변수](#익명-변수와-countdistinct) 참고). `reserved: true` 관계(위의 `wounded`)는 엔진이 채우는 관계이며, [`owner: engine`](/ko/state/state-model/#owner-engine) 상태 경로에 대응하는 사실 버전입니다. 콘텐츠는 이를 주장하거나 철회하지 않습니다(`E-RELATION-RESERVED-WRITE`). 도구 체인에서는 트레이스 목(mock) 또는 `lute play`의 `engine:` 단계가 해당 사실을 공급합니다.

```lute
::assert{ atLocation(shadowheart, grove) }
::retract{ atLocation(shadowheart, _) }
```

예약된 관계는 엔진이 언제 이를 변경하는지도 지정할 수 있습니다. `changedOn: [battleEnd]`는 엔진이 이를 올릴 수 있는 시점을 나타냅니다(dsl 0.25.0 §6). 이는 검사기 입력이지 IR 필드가 아니며, 캐스트 존재 여부에 영향을 줍니다. [엔진 이벤트 후 존재 여부](/ko/language/dialogue-and-cast/#엔진-이벤트-이후의-presence-changedon)를 참고합니다. `reserved: true`가 아닌 관계에 `changedOn`을 지정하거나 어떤 플러그인도 선언하지 않은 시점을 이름으로 지정하면 `E-RELATION-DECL`이 됩니다.

관계는 CEL 호출, 매크로 또는 키워드의 이름을 사용할 수 없습니다. `has`, `holds`, `count`, `has`, `now` 등이 이에 해당합니다. 또한 `after:` 호출(`completed`, `active`)이나 규칙 단어(`cel`, `not`)의 이름도 사용할 수 없습니다. `has(lamp)`는 작성할 수 없으므로, 이러한 관계를 선언하면 선언 위치에서 `E-RESERVED-NAME`이 발생하며 대신 사용할 이름을 제안합니다(`visited` → `wasAt`). 모든 예약 이름은 [예약 이름](/ko/reference/reserved-names/) 페이지에 있습니다.

### 하위 종류: `subsetOf:`

ID는 정확히 하나의 엔터티 종류에 속하며, 두 종류가 같은 멤버를 나열하면 `E-ENTITY-KIND-CLASH`가 됩니다. 사람들 중 동료처럼 더 큰 그룹의 일부인 그룹은 대신 **하위 종류**입니다(dsl 0.24.0 §3).

```lute check
---
kind: scene
id: camp.roster
entities:
  person:    { members: [isolde, corvin, hollis, player] }
  companion: { subsetOf: person, members: [isolde, corvin] }
relations:
  inParty: { args: [companion], tier: run }
  trusts:  { args: [person, person], tier: run }
  loyal:   { args: [companion], derive: true }
rules:
  - "loyal(P) :- inParty(P), trusts(P, player)"
---

## The roster

::assert{ inParty(isolde) }
::assert{ trusts(isolde, player) }
@isolde{when="holds('loyal', ['isolde'])"}: I'm with you.
```

하위 종류의 멤버는 부모의 멤버이기도 합니다(dsl 0.26.0 §2.3). 부모나 형제 하위 종류와 공유하는 멤버도 충돌이 아니며, 부모가 해당 멤버를 다시 나열할 필요도 없습니다(그곳에 다시 적어도 허용되며 중복이 아닙니다). 하위 종류는 종류가 사용되는 모든 곳에서 사용할 수 있습니다. 관계 인자, `per:` 인덱스([상태 모델](/ko/state/state-model/)), 시점의 대상 도메인, [종류 대상](/ko/language/beats/#kind-대상)이 이에 해당합니다. 하위 종류의 값은 부모의 값이기도 하므로 `trusts(isolde, player)`는 유효하며, 위 규칙은 `companion` 인자와 `person` 인자를 결합합니다. 반대 방향은 성립하지 않습니다. `::assert{ inParty(hollis) }`는 `hollis`가 사람이지만 동료는 아니므로 `E-FACT-DOMAIN`입니다.

**멤버 순서.** 종류의 멤버를 순서대로 나열하는 모든 곳 — [`for="kind:…"`](/ko/language/beats/#멤버별-1회-for) 비트의 프레젠테이션, 아티팩트의 `forKind.members`, 대상 도메인, `lute test`와 `lute trace`가 목을 제안하는 멤버 목록 — 에서는 종류 자체의 `members:`가 먼저 오고(그다음 `add:`로 얻은 멤버가 옵니다), 하위 종류의 멤버가 하위 종류 선언 순서대로 이어집니다. 각 멤버는 처음 나타난 위치에서 한 번만 포함됩니다. `hero: { members: [] }` 다음에 `ssr: { subsetOf: hero, members: [aria, cyra] }`, `sr: { subsetOf: hero, members: [bram] }`이 있으면 `hero`는 `aria, cyra, bram` 순서입니다. 서로 다른 스키마 파일에 있는 하위 종류는 파일별로, 가장 얕은 import를 먼저 하고 그다음 경로순으로 처리합니다.

하위 종류는 자체 `members:`를 나열합니다. 문서가 선언하지 않은 부모, `open:`으로 선언된 부모, `open:`으로 선언된 하위 종류, 그리고 자기 자신으로 되돌아가는 `subsetOf:` 순환은 `E-ENTITY-KIND-SHAPE`이며 스키마 줄에서 한 번 보고됩니다. 0.26.0 이전에는 부모 목록에 없는 하위 종류 멤버도 오류였습니다. [`add:`](/ko/state/schemas/#여러-파일에-걸쳐-조합되는-엔터티-종류-add)를 사용하면 여러 스키마 파일에서 종류의 멤버를 조합할 수도 있습니다.

프로젝트 전체 `lute check`의 `W-DOMAIN-UNREAD`는 아무것도 읽지 않는 종류를 표시합니다. 관계 인자와 함께 하위 종류의 `subsetOf:` 부모, `per:` 인덱스로 사용되는 종류, 규칙 본문이나 `holds(…)` 조건의 종류 원자를 읽기로 셉니다. 경고는 해당 종류를 선언한 스키마의 줄 또는 문서 자체의 `entities:` 키에 표시되며, 첫 번째 import 지점에는 표시되지 않습니다.

### 같은 철자의 id를 쓰는 두 id 공간

두 종류가 같은 멤버를 나열하는 이유가 단지 두 id 공간이 `{id}`를 같게 표기하기 때문이라면 합치지 마세요. 두 공간의 멤버를 모두 나열하는 **공유 루트 종류**를 선언하고 각 종류를 그 하위 종류로 만드세요. `{first}: { subsetOf: <root>, members: […] }`입니다. 모든 참조는 여전히 자기 종류에 대해 검사됩니다. 0.49.0부터 `E-ENTITY-KIND-CLASH`가 이 해결책을 알려 줍니다:

```yaml
entities:
  speaker: { members: [mira, vendor-reply] }
  person:  { subsetOf: speaker, members: [mira] }
  vendor:  { subsetOf: speaker, members: [vendor-reply] }
```

### 이름이 아닌 id: `providerRef`

엔터티나 enum 멤버는 **이름**(글자, 숫자, `_`, `-`)이어야 하므로 다른 시스템의 id인 `vendor:reply`는 `E-PATH-IDENT`입니다. 이름이 아닌 id를 검증하려면 프로바이더를 선언하고(`plugins/<id>/providers/`, `catalogDir`에 고정된 스냅샷) 속성을 `{ providerRef: <provider> }`로 타입 지정하세요. 0.49.0부터 엔터티/enum 멤버에 대한 `E-PATH-IDENT`가 이 방법을 알려 줍니다. [프로바이더와 카탈로그](/ko/tooling/providers-and-catalog/)를 참고하세요.

### 배타적 관계: `excludes:`

일부 관계는 같은 인자에 대해 함께 성립할 수 없습니다. 쓰러진 사람은 살아 있는 상태로 보이지 않으며, 증인은 정직하면서 동시에 거짓말쟁이일 수 없습니다. 관계는 배제하는 관계를 선언할 수 있습니다(dsl 0.25.0 §1).

```yaml
entities:
  character: { members: [elias, maren] }
  place:     { members: [gallery, landing] }
relations:
  at:        { args: [character, place], tier: run }
  damaged:   { args: [place], tier: run }
  seen:      { args: [character], tier: run }
  seenAfter: { args: [character], derive: true, excludes: [fell] }
  fell:      { args: [character], derive: true }
  calm:      { args: [character], tier: run, excludes: [panicked] }
  panicked:  { args: [character], tier: run }
rules:
  - "seenAfter(S) :- seen(S)"
  - "fell(S) :- at(S, gallery), damaged(gallery), not seenAfter(S)"
```

선언은 대칭적입니다. `fell`은 다시 명시하지 않아도 `seenAfter`를 배제하며, 컴파일된 `RelationEntry`는 대칭적 폐쇄를 정렬된 형태로 전달합니다(`fell`의 `"excludes": ["seenAfter"]`, 비어 있으면 생략). 각 상대는 관계 자신이 아닌 선언된 관계여야 하고, 동일한 인자 종류를 가져야 합니다. 그렇지 않으면 선언에서 `E-RELATION-DECL`이 발생합니다. 예를 들어 `` `dwon` is not a declared relation — did you mean `down`? ``, `` `near` takes [c, p] but `up` takes [c] ``, 또는 `cannot exclude itself`라는 관계가 이에 해당합니다. 기본 관계와 도출 관계는 어떤 조합으로든 서로 배제할 수 있습니다.

배제는 검사기가 규칙에서 추론하는 것이 아니라 작성자가 선언하는 불변식입니다. 프로젝트 전체 `lute check`는 관계형 가드를 결정하는 모든 곳에서 이를 사용합니다([관계형 가드를 검사기가 분석하는 방법](#프로젝트-전체-lute-check가-관계형-가드를-분석하는-방법) 참고).

- `holds('A', ['x']) && holds('B', ['x'])`는 거짓이므로 둘 다 필요한 가드는 실행 불가능합니다. 줄이나 arm에는 `E-ARM-DEAD`, 비트에는 `E-BEAT-UNREACHABLE`이 표시되며 메시지는 배제 관계를 이름으로 지정합니다.
- `holds('A', ['x'])`에서 `!holds('B', ['x'])`가 따라옵니다. 같은 가드 안, 바깥 가드 안, 또는 모든 경로에서 `A(x)`가 가드까지 성립하기 때문일 수 있습니다. 이런 가드는 `W-FACT-GUARANTEED`이며, [캐스트 존재 여부](/ko/language/dialogue-and-cast/#presence-present)도 같은 방식으로 읽습니다. 엔진 `reserved:` 관계에 대한 가드도 해당 가드 영역 안에서는 동일하게 셉니다.
- 모든 경로에서 `B(x)`가 성립하는 위치의 `::assert{A(x)}`는 새로운 **`E-FACT-EXCLUSIVE`**입니다.
- 배제를 깨뜨릴 수밖에 없는 규칙은 규칙 위치에서 새로운 **`E-RULE-EXCLUSIVE`**가 됩니다. `dark`가 `lit`을 배제하는데 `dark(X) :- lit(X)`이면, 같은 인자에서 `lit`이 성립하는 곳에서만 `dark`를 도출하므로 모든 도출이 배제를 깨뜨립니다. 규칙 또는 선언을 수정합니다.

`excludes:` 쌍에 이름이 지정된 관계는 읽힌 것으로 간주하므로, 쌍을 선언해도 [`W-RELATION-UNREAD`](#아무도-읽지-않는-관계)는 발생하지 않습니다.

폭풍 장면이 `at(elias, gallery)`와 `damaged(gallery)`, 그리고 모든 경로에서 `panicked(maren)`을 주장한다고 합니다. 그런 다음 `after: 'visited("storm")'`로 새벽 장면을 순서화합니다.

```lute
@narrator{when="holds('seenAfter', ['elias']) && holds('fell', ['elias'])"}: Both at once.
@narrator{when="holds('seenAfter', ['elias']) && !holds('fell', ['elias'])"}: Elias walks on.
::assert{ calm(maren) }
```

<!-- lute-diagnostics unverified="verbatim whole-project check output; the relational E-ARM-DEAD message names its code through the reachability::E_ARM_DEAD constant, so the scraper cannot pair quote and code" -->
```
./scenes/dawn.lute:20:17: error [E-ARM-DEAD] this gated line can never be shown: its `when` guard `holds('seenAfter', ['elias']) && holds('fell', ['elias'])` is provably false — `seenAfter(elias)` and `fell(elias)` can never hold together (`seenAfter` excludes `fell`)
./scenes/dawn.lute:21:17: warning [W-FACT-GUARANTEED] guard `holds('seenAfter', ['elias']) && !holds('fell', ['elias'])` is redundant: `!holds('fell', ['elias'])` follows from this guard's `holds('seenAfter', ['elias'])`: `seenAfter` excludes `fell`
./scenes/dawn.lute:22:1: error [E-FACT-EXCLUSIVE] `::assert{calm(maren)}` would make `calm(maren)` and `panicked(maren)` both hold: `panicked(maren)` is asserted on every route to here (./scenes/storm.lute:14), and `calm` excludes `panicked` — retract `panicked(maren)` first, or assert only where it does not hold
```

두 사실이 모두 가능할 뿐인 경우, 예를 들어 폭풍의 한 분기에서만 `panicked(maren)`이 주장되면 정적 판정은 없습니다. 워크 도구는 이를 조용히 넘기지 않고 포착합니다. `lute play`는 두 사실이 모두 성립하게 만든 쓰기 아래에 `✗ exclusive: calm(maren) and panicked(maren) both hold`를 출력하고 그 위치에서 중단합니다(종료 코드 1). `lute trace`와 `lute test`는 쓰기 위치에 같은 줄을 출력하고 `E-FACT-EXCLUSIVE`와 함께 워크를 거부합니다(종료 코드 1). 이미 배제를 깨뜨리는 시드 사실은 어떤 것도 워크하기 전에 거부됩니다. 도출 관계는 도출 과정을 통해 검사합니다. 규칙 실행 후 검사가 이루어지므로, 어떤 규칙이 배제된 상대를 결론으로 내리게 하는 쓰기는 그 쓰기에서 포착됩니다. [스토리 재생](/ko/tooling/play/#배타-관계)과 [트레이싱](/ko/tooling/tracing/#배타-관계)을 참고합니다.

## `validAt`으로 기록 조회하기

모든 사실이 유효 시간 구간을 가지므로, 가드는 지금이 아니라 *과거의 특정 시점에* 사실이 성립했는지를 물을 수 있습니다. 즉, 현재 유효한 `holds('rel', ['args'])`와 `count('rel', ['args']) OP n` 옆에 `validAt('rel', ['args'], t)`를 사용할 수 있습니다([CEL 표현식](/ko/state/cel/) 참고). 이는 기본 관계와, 입력 계층 어디에도 CEL 가드가 없는 규칙을 가진 도출 관계에서 허용됩니다. 가드는 스칼라 상태를 읽는데 스칼라는 이력을 보존하지 않으므로, 스칼라를 거치는 과거 시점 도출은 정의되지 않습니다(`E-VALIDAT-DERIVED`).

`t`는 `narrativeTime` 표현식이어야 하며, 작성자가 쓸 수 있는 것은 정확히 하나입니다. 퀘스트가 `unset` → `active`로 바뀔 때 엔진이 기록하는 시점인 **`quest.<id>.activatedAt`**입니다([퀘스트와 장면](/ko/language/quests-and-scenes/) 참고). 직접 `narrativeTime` 경로를 선언하면 `E-TEMPORAL-ARG`가 됩니다. 따라서 0.8.0 이전에는 이 슬롯이 예약되어 있어도 `validAt`이 가리킬 기준점이 없었고 쿼리는 실제로 사용할 수 없었습니다.

이를 함께 사용하면 퀘스트가 실제로 원하는 게이트를 표현할 수 있습니다. *이 일이 한 번이라도 일어났는가*가 아니라 *퀘스트를 수락한 뒤에 일어났는가*를 묻습니다.

```lute
<quest id="theCoffeeDebt" title="Settle the coffee debt">
  <objective id="visitedSinceAccept" title="Go back to the station" done="holds('arrivedSpace', ['station_front']) && !validAt('arrivedSpace', ['station_front'], quest.theCoffeeDebt.activatedAt)"/>
</quest>
```

이 사실은 현재 유효하지만 활성화 시점에는 *유효하지 않았으므로*, 목표는 빚을 수락한 후 방문했을 때만 완료됩니다. 플레이어가 이미 방문한 기록으로는 완료되지 않습니다. (태그와 모든 속성은 하나의 물리적 줄에 있어야 하므로, 해당 `<objective …/>`는 줄바꿈할 수 없습니다.)

## Datalog 도출

`derive: true`로 표시된 관계는 `rules:`로 계산합니다. 규칙은 함수 기호가 없는 계층화된 부정을 사용하는 Horn 절입니다. 하나의 헤드에 여러 규칙이 있으면 합집합을 이룹니다.

```yaml
rules:
  - "canReach(C, L)  :- atLocation(C, L)"
  - "canReach(C, L2) :- canReach(C, L1), connected(L1, L2)"
```

규칙은 `head :- body` 형식이며, 본문은 다음 리터럴을 쉼표로 연결한 논리곱입니다.

| 리터럴 | 예시 | 의미 |
| --- | --- | --- |
| 관계 원자 | `atLocation(C, L)` | 기본 또는 도출 사실이 일치합니다. 변수는 대문자로 쓰고 상수는 엔터티 멤버입니다. |
| 엔터티 종류 | `character(C)` | `C`가 엔터티 종류 `character`의 멤버 범위를 가집니다. 이는 멤버십 검사이며 사실 조회가 아닙니다. |
| 부정 | `not awake(P)` | 일치하는 사실이 성립하지 않습니다(계층화됨: `awake`는 이 헤드에 의존해서는 안 됩니다). |
| 익명 변수 | `sawAt(W, '_', _)` | 각 `_`는 무엇이든 일치하는 새로운 변수입니다(dsl 0.24.0). `not` 아래에서는 일치하는 튜플이 전혀 존재하지 않아야 합니다. |
| 부등식 | `A != B` | 바인딩된 두 항이 다릅니다. |
| 스칼라 가드 | `cel("run.act == 1")` | 스칼라 상태에 대한 CEL 조건이며 사실 쿼리가 아닙니다. |
| 개수 | `count(hasBadge(_)) >= 7` | dsl 0.26.0 §6: 일치하는 사실의 개수를 `>=`, `>`, `<=`, `<`, `==`, `!=`와 비교합니다. `countDistinct(R(…), V…) op n`도 사용합니다. [규칙 본문의 개수](#규칙-본문의-개수)를 참고합니다. |

```yaml
rules:
  - "suspect(P) :- character(P), not inParty(P)"
  - "rivals(A, B) :- inParty(A), inParty(B), A != B"
  - "canReach(player, moonrise) :- cel(\"run.act >= 2\")"
```

함수 기호가 없으므로 Herbrand 기초는 유한합니다. 따라서 상향식 평가가 유한한 단계 안에 최소 고정점에 도달하고, **모든 도출이 종료됩니다**. 이는 Prolog가 아니라 Datalog입니다. 안전성 규칙에 따라 헤드, 부정, 가드의 모든 변수는 양의 본문 원자에 나타나야 합니다( `character(P)`와 같은 엔터티 종류 원자도 포함됩니다). 위반하면 `E-DATALOG-UNSAFE`가 됩니다. 변수 없는 헤드를 가진 규칙은 양의 원자가 전혀 필요하지 않으므로, 스칼라 가드만 뒤에 있는 그라운드 헤드 `canReach(player, moonrise) :- cel("run.act >= 2")`도 유효한 규칙입니다. 부정 순환은 `E-DATALOG-UNSTRATIFIED`가 되고, 함수 항을 작성하려 하면 `E-DATALOG-FUNCTION`이 됩니다. 규칙 본문에는 스칼라 CEL 가드(`cel("run.act == 1")`)를 넣을 수 있지만, 사실 쿼리는 넣을 수 없습니다. 이 방화벽(`E-DATALOG-GUARD-FACT`)은 모든 의존성이 분석에 드러나도록 보장합니다. 도출 관계와 `reserved:` 관계는 콘텐츠에서 읽기 전용입니다. 전체 계층은 엔진이 결정론적으로 평가하는 데이터로 축소되며, 작성자가 반복을 제어하지 않습니다.

엔터티 종류 원자는 종류의 멤버에 대한 검사이지 `character(…)` 사실 조회가 아닙니다. 종류 이름으로 사실을 주장하는 일은 없습니다. `lute trace`, `test`, `play`, `run`은 조인, `not` 아래, `--derivation`에서 이를 같은 방식으로 평가합니다. 0.24.0 이전에는 이런 사실을 찾았으므로 `inParty(P) :- companion(P), …` 같은 규칙이 `check`를 통과해도 런타임에는 아무것도 도출하지 않았습니다.

### 익명 변수와 `countDistinct`

규칙 본문의 `_`는 규칙이 관심을 두지 않는 값을 나타냅니다. 각 `_`는 별도의 새로운 변수이므로 `sawAt(W, '_', _)`의 두 변수는 같을 필요가 없습니다. 바인딩할 필요가 없으므로 안전성에 영향을 주지 않습니다. `not` 아래에서는 존재 양화 변수입니다. 즉 `W`가 어디에서도 아무도 보지 않았을 때 `not sawAt(W, '_', _)`가 성립합니다. 규칙 헤드의 `_`는 `E-DATALOG-PARSE`입니다. 헤드 인자는 바인딩된 변수 또는 상수여야 하기 때문입니다. 비교식의 `_`(`X != _`)도 마찬가지입니다.

일치하는 사실의 개수를 세는 조건은 리스트 형식 호출을 사용합니다. `count('R', ['_'])`는 튜플을 세고, `countDistinct('R', ['_', '_'], 0)`은 0부터 시작하는 위치에서 서로 다른 값을 셉니다(해당 위치의 리스트 요소는 `_`여야 합니다).

```lute check
---
kind: scene
id: inquest.hall
entities:
  person: { members: [ada, bram, cole, dora] }
  place:  { members: [dock, mill] }
relations:
  sawAt:     { args: [person, person, place], tier: run }
  testified: { args: [person], derive: true }
  silent:    { args: [person], derive: true }
rules:
  - "testified(W) :- sawAt(W, _, _)"
  - "silent(W) :- person(W), not sawAt(W, _, _)"
facts:
  - "sawAt(ada, bram, dock)"
  - "sawAt(ada, cole, mill)"
  - "sawAt(bram, cole, dock)"
---

## The inquest

@narrator{when="count('sawAt', ['_', '_', '_']) >= 3"}: Three sightings are on record.
@narrator{when="countDistinct('sawAt', ['_', '_', '_'], 0) >= 2"}: At least two witnesses came forward.
@narrator{when="holds('testified', ['bram']) && holds('silent', ['dora'])"}: Bram spoke. Dora saw nothing at all.
```

목격은 세 건이고 목격자는 두 명입니다. `ada`가 두 명을 보았고 `bram`이 한 명을 보았습니다. `<Var>`는 패턴에 나타나는 변수여야 하며, 그렇지 않으면 `E-CEL-PROFILE`이 됩니다. 프로젝트 전체 `lute check`는 `count`와 마찬가지로 사실 외피(envelope)에서 `countDistinct`를 결정하며, trace, test, play가 이를 평가합니다. `count` 및 `holds`와 마찬가지로 사실 저장소를 읽으므로 규칙 가드에서는 금지됩니다(`E-DATALOG-GUARD-FACT`).

### 규칙 본문의 개수

dsl 0.26.0 §6부터 규칙 본문은 Datalog 그라운드/패턴 표기법을 사용해 개수를 셀 수 있습니다. `count(R(…)) <op> n` 및 `countDistinct(R(…), V…) <op> n` 형식이며, `<op>`는 `>=`, `>`, `<=`, `<`, `==`, `!=` 중 하나입니다. 규칙의 다른 리터럴이 바인딩하는 변수는 읽히므로 개수는 해당 바인딩마다 계산됩니다. 그 밖의 변수는 사실 전체를 범위로 가집니다.

```lute check
---
kind: scene
id: inquest.verdict
entities:
  person: { members: [ada, bram, cole, dora] }
  place:  { members: [dock, mill] }
relations:
  sawAt:        { args: [person, person, place], tier: run }
  corroborated: { args: [person], derive: true }
  caseOpen:     { args: [place], derive: true }
rules:
  - "corroborated(S) :- person(S), count(sawAt(_, S, _)) >= 2"
  - "caseOpen(dock) :- countDistinct(sawAt(W, _, dock), W) >= 2"
facts:
  - "sawAt(ada, cole, dock)"
  - "sawAt(bram, cole, mill)"
  - "sawAt(bram, dora, dock)"
---

## The verdict

@narrator{when="holds('corroborated', ['cole'])"}: Two people put Cole at the scene.
@narrator{when="holds('caseOpen', ['dock'])"}: Two witnesses were at the dock.
@narrator{when="!holds('corroborated', ['dora'])"}: Only one saw Dora.
```

`lute trace`는 세 arm을 모두 취합니다. IR은 개수 리터럴을 `{ kind: "count", atom, distinct?, op, n }`으로 전달합니다. `trace`, `test`, `play`, `run`은 개수를 세는 관계보다 한 계층 위에서 이를 평가하므로, 헤드를 도출하기 전에 개수가 확정됩니다. 규칙 자체의 헤드에 의존하는 관계를 세는 개수에는 이러한 계층이 없으며 **`E-RULE-AGGREGATE-CYCLE`**이 됩니다.

<!-- lute-diagnostics -->
```
cr/cyc.lute:13:6: error [E-RULE-AGGREGATE-CYCLE] a rule deriving `popular` counts `known`, which depends on `popular` itself (cycle: `known`, `popular`) — a `count(…)` / `countDistinct(…)` may only read a relation its rule's head does not feed, so the count is final before the head is derived
```

해당 규칙은 `known(A, B) :- popular(A), person(B)` 옆의 `popular(P) :- person(P), count(known(_, P)) >= 2`였습니다. 일곱 개의 배지에서 열리는 게이트는 `canPass(earthGymDoor) :- count(hasBadge(_)) >= 7`이라는 하나의 규칙으로 표현하며, `lute scenario knowledge`는 개수가 세는 사실의 생산자까지 추적합니다.

### 규칙 가드의 `@def`

규칙 가드는 조건이 호출할 수 있는 모든 def를 인자를 포함해 호출할 수 있습니다. 0.24.0부터 가드는 다른 항목이 읽기 전에 프로젝트의 def 테이블에 대해 확장되므로 `check`, `trace`, `test`, `play`가 모두 같은 본문을 확인합니다.

```lute check
---
kind: scene
id: lamp.room
state:
  run.day: { type: int, default: 1 }
defs:
  firstDay: { type: bool, cel: "run.day == 1" }
entities:
  item: { members: [lamp] }
relations:
  lit: { args: [item], derive: true }
rules:
  - "lit(lamp) :- cel(\"@firstDay\")"
---

## The lamp room

@narrator{when="holds('lit', ['lamp'])"}: The lamp is already burning.
```

0.24.0 이전에는 이 구성이 `check`를 통과했지만 이후 도출되지 않았습니다. 가드가 확장되지 않은 채 평가되어 미결정으로 읽혔고 규칙이 조용히 삭제되었기 때문입니다. 확장된 본문도 방화벽을 통과해야 하므로, 사실을 조회하는 def는 그 지점에서 `E-DATALOG-GUARD-FACT`가 됩니다. 가드를 확장할 수 없는 def는 `E-RULE-GUARD-DEF`입니다. 정의되지 않은 이름, 잘못된 인자 개수, 또는 규칙에 일치하는 주제가 없는 `$`가 이에 해당합니다. 플레이 시점에도 가드가 미결정 상태라면, 규칙의 관계를 조회하는 조건은 거짓이 아니라 알 수 없음으로 읽히며 play는 그 위치에서 중단하고 무엇이 이를 결정할 수 있는지 이름을 표시합니다.
### 규칙 가드의 엔티티 색인 상태

[`per:`](/ko/state/state-model/)로 선언한 경로는 종류의 각 멤버마다 `int` 또는 `double` 하나를 보유합니다. 문서에서는 멤버를 이름으로 지정하고(`run.approval.isolde`), 컴포넌트의 `::set`에서는 파라미터를 통해 색인할 수 있습니다(`run.approval[@who]`, 각 `::use`에서 검사됨. [상태 모델](/ko/state/state-model/) 참고). 규칙의 `cel()` 가드에서는 `run.approval[P]`가 규칙 변수 `P`에 바인딩된 멤버를 읽습니다(dsl 0.24.0 §3).

```lute check
---
kind: scene
id: camp.fire
state:
  run.approval: { type: int, default: 0, per: companion }
entities:
  person:    { members: [isolde, corvin, hollis] }
  companion: { subsetOf: person, members: [isolde, corvin] }
relations:
  inParty: { args: [companion], tier: run }
  devoted: { args: [companion], derive: true }
rules:
  - "devoted(P) :- inParty(P), cel(\"run.approval[P] >= 5\")"
---

## Camp

::assert{ inParty(isolde) }
::set{run.approval.isolde += 5}
@isolde{when="holds('devoted', ['isolde'])"}: I would follow you anywhere.
```

`P`는 인덱스 종류 또는 그 하위 종류를 대상으로 순회하는 양의 본문 원자에 의해 바인딩되어야 합니다. 여기서 `inParty(P)`는 `companion`을 대상으로 순회합니다. 이를 잘못 작성하는 방법마다 고유한 코드가 있습니다.

- 더 넓은 종류인 `person(P)`만으로 `P`를 바인딩하면 `E-FACT-DOMAIN`입니다. 일부 바인딩(`hollis`)은 선언되지 않은 경로를 읽게 됩니다.
- 어떤 양의 원자도 `P`를 바인딩하지 않으면 `E-DATALOG-UNSAFE`입니다.
- `per:` 없이 선언된 경로에 `run.trust[P]`를 쓰면 `E-UNDECLARED`입니다.

이러한 규칙은 **grounded**로 컴파일됩니다. 즉 종류의 멤버마다 IR 규칙 하나가 생기고, 각 가드는 ground 경로를 대상으로 하며, 규칙의 `raw` 텍스트에는 바인딩이 접미사로 붙습니다. 위 규칙은 각 동료에 대해 하나씩, 다음 두 규칙이 됩니다.

```json
{"head": {"relation": "devoted", "terms": [{"kind": "const", "value": "isolde"}]}, "body": [{"kind": "atom", "atom": {"relation": "inParty", "terms": [{"kind": "const", "value": "isolde"}]}, "negated": false}, {"kind": "guard", "cel": "run.approval.isolde >= 5"}], "raw": "devoted(P) :- inParty(P), cel(\"run.approval[P] >= 5\") [P = isolde]"}
```

따라서 IR 가드는 ground 항 위의 CEL로 유지되며, 엔진은 이를 평가하기 위해 새로운 기능이 필요하지 않습니다. `lute trace`, `play`, `run`은 동일한 인스턴스를 평가합니다.

### 도메인 타입 경로 옆의 규칙 변수

`{ domain: <kind> }`으로 타입이 지정된 상태 경로는 종류의 멤버 하나를 보유합니다. 스토커가 어디에 있는지, 어떤
동료가 앞장서는지 같은 값입니다. 규칙 `cel()` 가드는 그러한 경로를 규칙 변수와 `==` 또는 `!=`로 비교할 수 있습니다
(dsl 0.27.0 §3). 이것이 상태 계층과 팩트 계층을 연결하는 방법입니다. 여기서 질서관은 병동을 돌아다니고(엔진이
`run.stalker`를 이동시킴), 그가 옆방에 있을 때 방은 `close`입니다.

```yaml
state:
  run.stalker: { type: { domain: room }, default: morgue, owner: engine }
entities:
  room: { members: [lobby, morgue, chapel] }
relations:
  adjacent: { args: [room, room], tier: run }
  close:    { args: [room], derive: true }
facts:
  - "adjacent(lobby, chapel)"
  - "adjacent(chapel, morgue)"
rules:
  - "close(R) :- adjacent(R, S), cel(\"run.stalker == S\")"
```

색인 경로와 마찬가지로 `S`는 양의 본문 원자에 의해 바인딩되어야 합니다(`adjacent(R, S)`가 `room`을 대상으로 순회함). 이
규칙은 ground로 컴파일되며, 멤버마다 IR 규칙 하나가 생기고 가드는 단순 비교인
`run.stalker == 'chapel'`이 되며 `raw`에는 `[S = chapel]`이 붙습니다. 종류 비트에서는 해당
멤버 자체에 대해 `holds('close', [occasion.target])`으로 질문합니다. 두 가지 형태는
`E-CEL-PROFILE`입니다. 변수를 다른 방식으로 비교하는 경우(`run.hp > S`), 그리고
종류로 타입이 지정되지 않은 경로와 비교하는 경우(`run.hp == S`)입니다. 이 경로는 종류 멤버
일부를 나열하는 enum으로 타입을 지정할 수도 있습니다(`route: [none, ren, mika]`와 함께
`run.route: { type: { domain: route } }`를 선언하고, `suitor: { members: [ren, mika, kai] }` 위에서
`suitor(S)`를 사용). 두 선언에 모두 나오는 멤버에 대해서는 비트의 `when`에서 하는 동일한 비교와 마찬가지로 비교가
성립합니다. 종류 또는 enum이 변수의 종류와 공유하는 멤버가 전혀 없는 경로는 비교가 절대 성립할 수 없으므로 `E-FACT-DOMAIN`입니다.

## trace, test, play에서의 파생

엔진은 런타임에 최소 모델을 계산하며, 0.22.0부터는 콘텐츠를 실행하는 모든 곳에서 도구 체인도 같은 모델을 계산합니다. `lute trace`, `lute test`, `lute play`는 프로젝트의 시드 `facts:`를 로드하고, 사용자가 mock한 팩트와 콘텐츠가 진행 중 assert한 팩트에 `rules:`를 적용합니다(계층화된 부정 포함). 세 명령은 모두 참조 러너(`lute run`)와 하나의 평가기를 공유하므로, 규칙이 무엇을 결론내리는지 도구 체인 자체가 서로 다르게 판단할 수 없습니다.

따라서 파생 팩트도 다른 팩트와 똑같이 동작합니다. 다음을 가정합시다.

```yaml
entities:
  item: { members: [lamp] }
relations:
  knows:   { args: [item], tier: run }
  broken:  { args: [item], tier: run }
  canMend: { args: [item], derive: true }
rules:
  - "canMend(I) :- knows(I), not broken(I)"
```

`holds('canMend', ['lamp'])`라는 가드는 규칙으로 결정됩니다. `knows(lamp)`를 mock하면 성립하고, `broken(lamp)`를 추가하면 성립하지 않습니다. 결론을 mock할 필요는 없으며, “부정된 전제가 성립하므로 거짓”인 경우도 테스트할 수 있습니다. mock한 파생 원자도 그대로 허용되며 시드처럼 기반 팩트에 결합됩니다. 규칙의 `cel(…)` 가드가 trace에서 아직 결정하지 않은 상태를 읽으면 해당 규칙은 아무것도 파생하지 않고, trace는 결론을 unknown으로 보고하면서 이를 결정할 상태 경로의 이름을 표시합니다.

trace mock의 키, `*.test.yaml`, play 스크립트의 키인 `derive: false` 또는 이 키보다 우선하는 `--no-derive` 플래그를 사용하면 0.21 모델로 돌아갑니다. 시드 `facts:`를 로드하지 않고, mock하지 않은 파생 원자는 unknown이 되며, 읽힌 각 파생 관계의 이름이 note로 표시됩니다. 이 모델을 기준으로 작성한 테스트(unknown을 읽는 mock하지 않은 파생 원자 또는 빈 값을 읽는 시드 관계)는 판정을 고정하기 위해 `derive: false`를 지정합니다. [Tracing guide](/ko/tooling/tracing/)와 [Playing a story](/ko/tooling/play/)를 참고하세요.

어떤 원자가 *왜* 성립하는지 보려면 `lute play --derivation <ATOM>`을 사용합니다(반복 가능). 플레이스루 끝에 파생을 출력합니다. 사용한 규칙, 각 전제 자체의 근거(시드 팩트, assert, 또는 그 차례에 파생된 팩트), 그리고 `(absent)`로 표시된 부정 전제가 나옵니다.

```
explain canMend(lamp): holds
  canMend(lamp)  ⇐ canMend(I) :- knows(I), not broken(I)
  ├─ knows(lamp)  (asserted)
  └─ not broken(lamp)  (absent)
```

원자가 성립하지 않으면, 해당 원자를 결론으로 낼 수 있는 모든 규칙과 실패한 전제를 나열합니다.

```
explain canMend(lamp): does not hold
  canMend(I) :- knows(I), not broken(I)
  ├─ knows(lamp)  (asserted)
  └─ ✗ not broken(lamp)  (but it holds: asserted)
```

## 프로젝트 전체 `lute check`가 관계형 가드를 분석하는 방법

지식을 전제로 하는 가드가 올바른 도구입니다. `@eris{when="holds('knows', ['player', 'lumen'])"}`은 플레이어가 알고 난 뒤에만 줄을 표시합니다. 0.20.0부터 프로젝트 전체 `lute check`(`lute check --project <DIR>`)는 모든 가드 슬롯의 모든 관계형 질의(`holds(…)`, `count(…)`)를 다음 세 판정 중 하나로 결정합니다. 줄의 `when=`, `<choice when>`, `<when test>`, `::jump` 가드, 로어 항목의 `when`, 퀘스트의 `start`/`fail`, objective의 `done`/`visibleWhen`이 대상입니다.

- **impossible** — 프로젝트 어디에도 일치하는 팩트를 생성할 수 있는 시드, assert, 규칙 또는 엔진 관계가 없습니다.
- **guaranteed** — 가드에 도달하는 선언된 모든 경로에서 일치하는 팩트가 성립합니다.
- **possible** — 그 밖의 모든 경우입니다. 일부 경로에서는 성립하고 일부에서는 성립하지 않거나, 분석으로 분리할 수 없습니다. 가드에서 일반적인 경우이며 아무 출력도 하지 않습니다.

판정은 두 집합에서 나옵니다.

- **May** — 프로젝트 전체에 걸치며 흐름에 무관합니다. 모든 실행의 어느 시점에서든 살아 있을 수 있는 모든 그라운드 팩트입니다. `facts:` 시드, 프로젝트 루트 어디에서든 assert된 모든 `::assert`의 팩트(씬, 퀘스트 `<on>`/`<objective>` 본문, 로어 항목에 있으며 문서가 도달 불가능하다고 증명되지 않은 것), `reserved:` 관계의 모든 팩트(엔진이 어느 것이든 채울 수 있음), 그리고 그 집합 위에서 `rules:`가 파생하는 모든 것이 포함됩니다. May는 어떤 경로가 팩트를 생성할 수 *있다는 것*만 알 뿐, 언제 생성하는지는 알지 못합니다. 부정된 본문 원자를 성립 가능한 것으로 읽지만, 한 가지 예외가 있습니다(dsl 0.23.0). 프로젝트 어디에서도 retract하지 않고 `key:`를 통해 대체하지도 않는 시드 팩트에 대한 부정은 거짓입니다. `inParty(shadowheart)`를 시드한 경우 `suspect(P) :- character(P), not inParty(P)`는 `suspect(shadowheart)`를 결코 결론내릴 수 없으므로 `holds('suspect', ['shadowheart'])` 가드는 dead이며, `suspect(astarion)`은 여전히 따라옵니다. `::retract{inParty(shadowheart)}`가 하나라도 존재하면 부정은 다시 성립 가능합니다.
- **Must** — 경로에 민감합니다. 한 프로그램 지점으로 가는 모든 경로에서 팩트가 살아 있습니다. 문서 안에서는 정방향으로 순회합니다. `::assert`는 팩트를 추가하고(`key:`가 대체하는 팩트는 제거), `::retract`는 일치하는 모든 것을 제거하며, `<branch>`/`<match>`의 arm은 다시 합쳐지는 지점에서 **교집합**을 취합니다(가드 없는 선택지가 없는 branch나 망라적이지 않은 match도 어떤 arm도 실행되지 않을 수 있으므로 블록 전 집합과 교집합을 취함). `<hub>` 본문은 0회 이상 실행될 수 있습니다. 씬 사이에서는 [스칼라 엔벨로프](/ko/connectivity/envelopes/)와 정확히 같이 `after:` 그래프를 따릅니다. 씬은 시드와 `after:` 수식이 보장하는 팩트에서 시작합니다. `visited(A)`는 `A`가 끝날 때 성립하는 것을 기여하고, `&&`는 합집합을 취하며, `||`는 교집합을 취하고, `completed(q)`/`active(q)`는 아무것도 기여하지 않습니다. 퀘스트 본문과 로어 항목 본문은 엔진이 정한 시점에 실행되므로 시드와 자체 가드에서 시작합니다.

Must를 sound하게 유지하는 세 규칙이 있습니다.

- **작성자가 제어하지 않는 시간을 넘어가는 것은 단조 팩트뿐입니다.** 어떤 팩트가 문서 경계를 넘어 살아남으려면 프로젝트 어디에도 그 팩트와 일치하는 `::retract`가 없고, 다른 assert가 다른 값으로 같은 `key:` 튜플을 공유하지 않으며, 그 관계가 `reserved:`가 아니고 `open: engine` 인자도 없어야 합니다. `tier: scene` 및 `tier: quest` 관계의 팩트는 문서 경계를 넘지 않습니다. 엔진이 해당 에피소드 또는 퀘스트와 함께 지우기 때문입니다.
- **가드는 가정입니다.** `<when test="holds(F)">`, `<choice when="holds(F)">`, 가드가 있는 `::jump`, `<on when>`, objective `done`, 엔트리 `when`, 또는 씬 전체에 적용되는 비트 씬의 frontmatter `when:` 안에서, 그라운드 `F`를 가진 양의 최상위 `&&` 결합자 `holds(F)`는 각각 Must에 들어갑니다. 따라서 같은 팩트에 대한 중첩 가드는 redundant로 표시됩니다. 줄의 `when=`은 해당 줄만 가드합니다.
- **개수는 구간입니다.** `count(P)`는 `P`와 일치하는 Must 팩트 수와 May 팩트 수 사이에 있습니다. `n`과의 비교는 전체 구간이 한쪽에 놓일 때 결정됩니다.

가드는 로어 항목이 읽혔다는 것도 전제로 삼을 수 있습니다(dsl 0.24.0 §6). `entry.X.read`(또는 `entry.X.read == true`) 아래에서 Must는 X 본문이 모든 경로에서 assert하고 프로젝트 어디에서도 retract하지 않는 팩트를 얻습니다. 플레이어가 해당 항목을 읽었으므로 assert가 실행된 것입니다. 의심을 기록하는 항목을 보겠습니다.

```lute
<entry id="keeperLog" category="place">
  @narrator: The keeper's log names Isolde twice.
  ::assert{ suspects(isolde) }
</entry>
```

그리고 그것을 읽은 플레이어에게만 선택지를 제공하는 씬입니다.

```lute
<branch id="ask">
  <choice id="press" text="Ask about the log" when="entry.keeperLog.read">
    @narrator{when="holds('suspects', ['isolde'])"}: The log named her.
  </choice>
  <choice id="leave" text="Leave">
    @narrator: You leave the office.
  </choice>
</branch>
```

안쪽 가드는 성립할 수밖에 없으므로 프로젝트 전체 `lute check`는 이를 redundant로 보고하고 보장하는 assert를 이름으로 표시합니다.

<!-- lute-diagnostics -->
```
./scenes/office.lute:13:21: warning [W-FACT-GUARANTEED] guard `holds('suspects', ['isolde'])` is redundant: `suspects(isolde)` is asserted on every route to here (./lore/harbor.lute:9)
```

그 부정인 `!holds('suspects', ['isolde'])`은 dead 가드가 됩니다. 엔트리의 `<match>`에서 한 arm에만 내려가는 assert는 아무것도 보장하지 않습니다. `entry.X.everRead`는 항목이 어떤 실행에서, 어쩌면 이전 실행에서 읽혔다는 것만 뜻하므로, 새 실행에서도 유지되는 엔트리의 `tier: user` 및 `tier: app` 팩트만 추가합니다. `suspects` 같은 run-tier 팩트는 `everRead` 아래에서 추가되지 않습니다.

이 판정은 이미 dead 스칼라 가드를 보고하는 동일한 결정 절차에 들어갑니다. 따라서 결코 성립할 수 없는 관계형 가드는 해당 슬롯이 이미 소유한 코드로 보고되고, 가드 안에서 항상 성립하는 가드는 redundant로 표시됩니다.

| 슬롯 | 성립할 수 없음 | 항상 성립 |
| --- | --- | --- |
| `<when test>`, `<choice when>`, 콘텐츠 줄 `when=`, `::jump` 가드 | `E-ARM-DEAD` | `W-FACT-GUARANTEED` (`::jump` 제외) |
| 로어 엔트리 `when` | `E-ENTRY-UNREACHABLE` | `W-FACT-GUARANTEED` |
| objective `done` | `E-OBJECTIVE-UNSATISFIABLE` | — (가드가 아닌 술어) |
| required objective `visibleWhen` | `W-OBJECTIVE-HIDDEN` | — |
| 퀘스트 `start` / `fail` | `E-QUEST-UNREACHABLE` (`fail`은 항상 성립할 때도 해당) | — |

문서가 아예 표현할 수 없는 질의(선언되지 않은 관계, 잘못된 arity, 닫힌 도메인 밖의 인자)는 undecided로 남으며, 자체 오류(`E-RELATION-UNKNOWN`, `E-RELATION-ARITY`, `E-FACT-DOMAIN`)가 이를 소유합니다. `E-ENTRY-UNREACHABLE`은 이 분석과 함께 새로 생겼으며, 스칼라만으로 false를 결정하는 엔트리 `when`에 대해서는 단일 파일 `lute check`에서도 발생합니다. 모든 관계형 게이트를 “입증되지 않음”으로 표시하던 `W-UNPROVEN-RELATIONAL`은 사라졌습니다. 이제 그 전제가 성립하지 않으며, `--deny`에 이름을 지정하면 사용법 오류입니다.

작은 예를 보겠습니다. archive는 팩트 하나를 무조건 assert하고, 다른 하나는 한 branch arm에서만 assert합니다.

```lute
## The Archive

@narrator: The log is still open on the console.
::assert{ knows(player, lumen) }
<branch id="dig">
  <choice id="readOn" text="Keep reading">
    ::assert{ knows(player, heading) }
    @narrator: The heading was changed eleven years ago.
  </choice>
  <choice id="close" text="Close the log">
    @narrator: You close it.
  </choice>
</branch>
```

bridge는 `after: 'visited("demo.archive")'`로 순서가 정해지고 세 줄에 가드를 겁니다.

```lute
## The Bridge

@eris{when="holds('knows', ['player', 'lumen'])"}: So you read the log.
@eris{when="holds('knows', ['player', 'heading'])"}: Then you know where we are going.
@eris{when="holds('knows', ['eris', 'heading'])"}: I changed it myself.
```

프로젝트 전체 `lute check`는 첫 번째와 세 번째를 판정하고 두 번째는 그대로 둡니다. `knows(player, heading)`은 플레이어가 `readOn`을 선택한 경우에만 성립하며, 바로 그 경우를 위해 해당 가드가 있기 때문입니다.

<!-- lute-diagnostics -->
```
./scenes/bridge.lute:10:13: warning [W-FACT-GUARANTEED] guard `holds('knows', ['player', 'lumen'])` is redundant: `knows(player, lumen)` is asserted on every route to here (./scenes/archive.lute:10)
```

<!-- lute-diagnostics unverified="the relational E-ARM-DEAD message is composed in crates/lute-check/src/fact_check.rs, which names the code through the reachability::E_ARM_DEAD constant rather than a string literal, so the scraper cannot pair quote and code; copied verbatim from whole-project check output" -->
```
./scenes/bridge.lute:12:13: error [E-ARM-DEAD] this gated line can never be shown: its `when` guard `holds('knows', ['eris', 'heading'])` is provably false — no seed, assert, rule, or engine relation produces `knows(eris, heading)` under your declared routes
```

동일한 두 집합으로 개수도 결정합니다. archive 뒤에서 `count('knows', ['player', '_'])`는 1과 2 사이이므로, `count('knows', ['player', '_']) >= 3` 가드는 dead이고 `count('knows', ['player', '_']) >= 1`은 redundant입니다.

이 분석에는 전체 프로젝트가 필요합니다. 형제 문서의 assert는 단일 파일에서 보이지 않으며, 한 파일만 보고 “절대 assert되지 않음”이라고 주장하면 거짓이 되기 때문입니다. 따라서 분석은 프로젝트 전체 `lute check`에서만 실행됩니다. `lute check <file>`은 각 관계형 질의를 undecided로 두지만, 한 조건 안의 두 질의가 모순되는 것은 여전히 알아냅니다. `holds(P) && !holds(P)`는 무엇이 성립하든 false입니다(dsl 0.23.0, [`when` 결정 방법](/ko/language/beats/#when의-판정-방식) 참고). 가드가 guaranteed로 판단된 이유를 보려면 `lute scenario --project <DIR> envelope --node <scene>`이 스칼라 표 옆에 씬 진입 시점의 **guaranteed facts**를 나열합니다([엔벨로프](/ko/connectivity/envelopes/#보장된-팩트) 참고). 가드가 질의하는 팩트를 어떤 문서가 생성하는지 보려면 `lute scenario --project <DIR> knowledge`가 각 질의 원자를 규칙을 따라 producer까지, 또는 **no producer**까지 추적합니다([개요](/ko/tooling/overviews/#lute-scenario-knowledge) 참고).

### 아무도 읽지 않는 관계

반대 문제는 쓰이지만 한 번도 읽히지 않는 관계입니다. 프로젝트 전체 `lute check`(dsl 0.24.0 §7)는 선언되었고 `reserved:`가 아니며 assert, seed 또는 derive되었지만 어떤 조건도 질의하지 않고(`holds`, `count`, `countDistinct`), 어떤 규칙 본문도 사용하지 않으며 어떤 def도 읽지 않는 관계에 `W-RELATION-UNREAD`를 보고합니다. 그 팩트는 아무것도 바꾸지 않습니다.

<!-- lute-diagnostics -->
```
./world.schema.yaml:9:3: warning [W-RELATION-UNREAD] relation `rumor` is written (asserted, seeded, or derived) but never read: no condition queries it (`holds` / `count`), no rule body uses it, and no def reads it — the facts it records change nothing; read it where it matters, or drop it
```

프로젝트마다 선언 위치에서 한 번 보고됩니다. 스키마 파일의 해당 줄이나 문서 자체의 `relations:` 키입니다. play 스크립트와 scenario 테스트는 읽기로 간주되지 않으므로, 테스트만 검사하는 관계도 이 경고를 받습니다. def에 대응하는 경고는 `W-DEF-UNUSED`입니다([상태 스키마](/ko/state/schemas/) 참고).

### 아직 쓰이지 않은 팩트 위의 가드

스토리를 작성하는 동안에는 가드가 dead인 이유가 그 팩트를 assert하는 씬이 아직 존재하지 않기 때문인 경우가 많습니다. `lute check --project <DIR> --wip`(dsl 0.23.0)는 관계에 **producer가 전혀 없기 때문에** 가드가 dead인 경우 `E-ENTRY-UNREACHABLE`, `E-BEAT-UNREACHABLE`, `E-OBJECTIVE-UNSATISFIABLE`을 경고로 보고합니다. 즉 시드도, 어디에도 `::assert`도, 규칙도, `reserved:`도 없어야 합니다. 경고의 note가 그 이유를 말합니다. producer는 있지만 질의와 절대 일치할 수 없는 관계는 여전히 오류이며, dead 콘텐츠가 downstream에서 발생시키는 `E-CONN-UNREACHABLE` 같은 것도 마찬가지입니다. [작업 중](/ko/language/beats/#작업-중)을 참고하세요.

## 보상 효과가 만드는 사실

플러그인 [보상 종류](/ko/language/quests-and-scenes/#보상-효과)는 `effects: { asserts, retracts }`(0.50)를 선언할 수 있습니다. 이런 보상이 쓰는 사실은 평범한 사실입니다. 프로젝트 전체 `lute check`는 각 효과를 **소유자의 지급 지점 맨 앞에 해당 `::assert` / `::retract`를 쓴 것처럼** 읽으므로 위의 모든 내용이 그대로 적용됩니다: May와 Must 집합, `E-QUEST-UNREACHABLE`, `E-ARM-DEAD`, `E-OBJECTIVE-UNSATISFIABLE`, `W-FACT-GUARANTEED`, `W-RELATION-UNREAD`(보상의 assert는 생산자입니다), `E-FACT-EXCLUSIVE`. 캐스트 존재 분석도 효과를 읽습니다.

| 보상 | 지급 지점 | 사실이 적용되는 곳 |
|---|---|---|
| 퀘스트 수준, `outcome` 없음 | 퀘스트 완료 | 퀘스트의 모든 `<on event="questComplete">` 본문, 그리고 `after:` / `completed(...)`를 따라 흐르는 완료 상태 |
| 퀘스트 수준, `outcome="failed"` | 퀘스트 실패 | 모든 `<on event="questFailed">` 본문, 마찬가지 |
| 목표 수준 | 목표 완료 | `done` 이후 목표 본문의 진입점, 그리고 목표 완료 상태 |

- **가드 없는 assert**는 지급 지점부터 May와 Must 모두에 들어갑니다.
- **가드 있는 assert**(`<reward when="…">`)는 가드 있는 `::assert`처럼 분기합니다. 항상 May에 들어가고, Must에는 가드가 성립하는 경로에서만 들어갑니다. `lute check`는 `when ⇒ fact` 함의를 유지하지 않으므로 지급 뒤 그 사실은 *가능*일 뿐이며, 이를 검사하는 가드는 `W-FACT-GUARANTEED`를 내지 않습니다.
- **retract**는 가드 유무와 관계없이 일치하는 것을 Must에서 제거하며 `E-FACT-EXCLUSIVE`를 위해 May에 기록됩니다.
- 한 보상 안의 순서: `retracts`, 이어서 `asserts`. 보상은 선언 순서이며 목표 보상이 퀘스트 보상보다 먼저입니다. 같은 사실을 assert한 뒤 retract하는 두 보상은 지급 뒤 사실이 없는 상태로 남깁니다. 같은 패턴의 `retracts`와 `asserts`를 모두 가진 한 종류는 사실이 있는 상태로 남깁니다.
- `accepts` 효과는 사실 효과가 없습니다. [도달 가능성](/ko/connectivity/reachability/#보상-효과가-그리는-엣지)과 `W-QUEST-NEVER-ACCEPTED`에만 반영됩니다.

`@target`을 읽는데 `target=`이 없는 보상은 아무것도 적용하지 않으므로 사실을 만들지 않습니다. `lute scenario knowledge`와 `lute lore`는 보상을 생산자로 지목합니다:

```text
met(mira) — produced by reward BADGE of quest `firstVisit`
```
<!-- rerun-after-S8 -->
