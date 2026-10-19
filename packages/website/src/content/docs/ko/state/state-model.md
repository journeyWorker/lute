---
title: 상태 모델
description: Lute의 계층형 스칼라 상태 — 실행, 사용자, 앱 수명 네임스페이스(에피소드 로컬 scene 및 스키마로 선언하는 season 계층도 포함), 경로를 선언하는 방법(열거형 타입 경로와 엔터티별 경로 포함), 읽기와 쓰기를 지배하는 경로 민감 확정 대입 규칙, 엔진만 쓰는 경로, 그리고 이전 실행 또는 시즌 윈도우의 최종값인 prev.run / prev.season입니다.
---

Lute 스칼라 상태는 타입이 지정된 경로(`int`, `double`, `bool`, `string`, `enum`)의 집합이며, **재설정 경계로 이름 붙인 네임스페이스** — 엔진이 값을 지우는 순간 — 로 묶입니다. 한 축(*언제 재설정되는가?*)에는 네 가지 계층이 있습니다:


0.32부터 숫자 상태 타입은 CEL의 `int`와 `double`이며 `number`는
제거되었습니다. 컴파일된 **실행 IR**에는 엔진이 소유한 선언에
`owner: "engine"`이 들어가며, 런타임은 콘텐츠가 해당 경로에 쓰는 것을 거부합니다.
| 네임스페이스 | 재설정 경계 | 일반적인 용도 |
|---|---|---|
| `scene.*` | 에피소드 종료(하나의 `.lute` 문서, 해당 섹션 전체에서 유지) | 무대 위 상태, `scene.choices.*`, `scene.visited.*` |
| `run.*` | 새 실행 — 한 번의 시도, 에피소드의 연속 | 시도별 플래그, 정서, 한 시도 안에서 에피소드 간 전달 |
| `user.*` | 프로필/계정 삭제 — 실행이 바뀌어도 유지 | 레벨, 잠금 해제, 메타 진행 |
| `app.*` | 앱 제거 — ID와 무관 | 언어, 연령 등급, 설정 |

엔진이 **모든 재설정을 소유하고 실행**하며, 언어는 재설정을 트리거하지 않습니다. 세 가지 영속 계층 — `run` / `user` / `app` — 은 게임/시즌 전역이므로, 장면이 `uses:`로 가져오는 하나의 공유 스키마 문서에 들어갑니다([상태 스키마](/ko/state/schemas/) 참조). 진정으로 에피소드 로컬인 `scene.*` 선언만 장면 안에 인라인으로 둘 수 있으며, 장면은 가져온 계층을 재선언하거나 재정의해서는 안 됩니다(MUST NOT).

스키마는 선언된 **시즌**마다 계층을 하나 더 추가할 수 있습니다(dsl 0.27.0 §5). `season.<name>.*`은 시즌이 열릴 때마다
기본값으로 재설정되고, `prev.season.<name>.*`은 이전
윈도우가 끝났을 때의 값을 유지합니다. [시즌](/ko/state/schemas/#시즌)을 참조하세요.

### 계층 한눈에 보기

무언가를 유지하거나 잊어야 하는 곳이라면 같은 수명 구분이 다시 등장합니다. 각 키는 다음
부분집합 중 하나를 취합니다.

| 위치 | 키 | 허용 값 |
|---|---|---|
| 상태 경로 | 루트 | `scene`, `run`, `user`, `app`, 또는 `season.<name>` |
| 관계 | `tier:` | `scene`, `run`, `user`, `app`, `quest`, `season:<name>` |
| 퀘스트 | `tier=` | `run`, `user`(기본값), `season:<name>` |
| 프로젝트의 모든 퀘스트 | `defaults.questTier` | `run`, `user`, `season:<name>` |
| 비트(장면, `<entry>`, `<beat>`) | `once` | `run`, `user`, `day`, `slot`, `week`, `season:<name>`, `false` |
| 다시 돌아오는 퀘스트 | `rearm=` | 계층이 아닌 조건: false에서 true로 바뀔 때 퀘스트가 재설정됨 |

`day`, `slot`, `week`에는 선언된 [클록](/ko/language/clock/)이 필요하고(`week`에는 해당 `week:`도 필요),
`season:<name>`에는 선언된 [시즌](/ko/state/schemas/#시즌)이 필요합니다. 계층 값은
`season:<name>`으로 표기하며, 같은 시즌의 상태 네임스페이스는
`season.<name>.*`입니다.

## 선언

읽거나 *쓰는* 모든 경로에는 `type`과 선택적 `default`를 지정해 선언해야 합니다(MUST). 네임스페이스가 없는 단순 상태 이름은 없습니다.

```yaml
state:
  scene.affect.elena: { type: int, default: 0 }
  run.choseHelp:      { type: bool,   default: false }
  user.level:         { type: int, default: 1 }
  app.rating:         { type: { enum: [teen, adult] }, default: teen }
```

작성자가 사용하는 `state:`는 **스칼라만 가능**합니다 — `int`, `double`, `bool`, `string`, `enum`뿐이며 다른 것은 허용되지 않습니다. `enum` 경로는 멤버를 `type:` 안에 중첩합니다 — `{ type: { enum: [teen, adult] } }`. 같은 단계의 `values:` 키는 선언 키가 전혀 아닙니다. `{ type: enum, values: [teen, adult] }`는 스키마에서 `E-STATE-DECL`이며, 해당 스키마를 `uses:`하는 문서는 그 아래에 `E-STATE-DECL`을 담은 `E-USES-PARSE`로 실패합니다. [`docs/examples/showcase/schema/base.schema.yaml:9`](https://github.com/journeyWorker/lute/blob/main/docs/examples/showcase/schema/base.schema.yaml)는 올바르게 작성한 같은 선언입니다. `type`이 `list`, `record`, `map`인 선언은 `E-STATE-COLLECTION`이며, 선언은 *설치되지 않습니다*. 이후 해당 경로를 읽으면 가상의 컬렉션 타입 슬롯을 해석하는 대신 일반 `E-UNDECLARED`를 보고합니다.

이 검사는 0.8.0에 새로 추가되었지만 제한을 추가한 것이 아니라 모호함을 제거합니다. 규범적 텍스트에서는 늘 스칼라라고 했지만, 셰이프 검증기는 전체 타입 유니온을 허용했고 런타임 문서에서는 `list<…>` / `map<…>` / `record`를 유효한 항목 타입으로 설명했습니다 — 세 출처가 세 가지 답을 내놓은 셈입니다. 이제 세 곳의 설명이 모두 일치합니다. 컬렉션은 원래 **관계형으로** 모델링해야 했습니다. 인벤토리는 `list<string>`이 아니라 `ownsItem(item)`이므로 [`relations:`](/ko/state/facts-and-datalog/)를 사용하세요. 컬렉션 형태의 항목 타입은 여전히 컴파일된 아티팩트에 도달할 수 있지만, 플러그인의 `state_shapes` 확장을 통해서만 가능하며 작성자의 `state:` 블록을 통해서는 절대 불가능합니다.

### 경로 하나, 선언 하나

프런트매터의 `state:` 선언은 해당 문서에만 국한되지 않습니다. 두 문서에서 선언한
`run.southFossil`은 런타임에 하나의 값입니다. dsl 0.26.0 §2.1부터 프로젝트 전체 `lute check`는 인라인이든 가져온 것이든 경로의 모든
선언을 비교하고, `type`, `default`, `per` 또는 `owner`가 다르면 두
파일과 줄을 명시한 **`E-STATE-DECL-CONFLICT`**를 보고합니다. `lute play`는 한
경로를 두 타입으로 선언한 프로젝트를 거부합니다.

<!-- lute-diagnostics -->
```
./scenes/b.lute:5:3: error [E-STATE-DECL-CONFLICT] state path `run.southFossil` is declared as bool, default false at `./scenes/a.lute:5` but as enum(none|dome|spiral), default "none" at `./scenes/b.lute:5`; every declaration of one path must agree on type, default, per and owner — they share one runtime value (declare it once in a schema both documents import)
```

`scene.*` 경로는 장면 로컬이므로 예외이며, [`extends:`](/ko/state/schemas/#구성-uses와-extends)로 해당 경로를
세분화하는 선언(재정의된 기본값)은 충돌이 아닙니다.
경로를 공유하려면 두 문서가 가져오는 스키마에 한 번만 선언하세요.

### 이름 있는 열거형으로 타입을 지정한 경로

경로는 멤버를 인라인으로 나열하는 대신 이름 있는 열거형에서 타입을 가져올 수 있습니다. `{ type: { domain: weekday } }`는 스키마의 `weekday` 열거형에서 멤버를 읽으며, 이는 해당 열거형을 읽는 것으로 간주되므로 열거형에는 `W-DOMAIN-UNREAD`가 발생하지 않습니다. `0.24.0`부터 긴 형식의 열거형은 멤버에 표시 **레이블**을 지정할 수 있으며, 해당 타입을 사용하는 경로를 보간하면 멤버 id가 아니라 레이블이 렌더링됩니다.

```yaml
state:
  run.weekday: { type: { domain: weekday }, default: sun }
enums:
  weekday:
    members: [sun, mon, tue]
    labels: { sun: Sunday, mon: Monday }
```

`@narrator: Today is {{run.weekday}}.`는 `Today is Sunday.`로 렌더링됩니다. 레이블이 없는 멤버는 id(`tue`)를 렌더링합니다. 조건에서는 여전히 id를 비교합니다: `run.weekday == 'sun'`. 멤버가 아닌 항목의 레이블은 `E-ENUM-LABEL-NOT-MEMBER`이고, 문자열이 아닌 레이블은 `E-META-VALUE`입니다. 선언된 [클록](/ko/language/clock/)도 요일 이름에 같은 메커니즘을 사용합니다.

`domain:`은 열거형 대신 [엔터티 종류](/ko/state/facts-and-datalog/#엔터티-관계-그라운드-사실)의 이름을 지정할 수도 있습니다. 그러면 경로에는 해당 종류의 멤버 하나가 저장됩니다. 이는 맵 종류에 대해 엔진이 유지하는 위치, 예를 들어 스토커가 있는 방을 타입 지정하는 방법입니다.

```yaml
state:
  run.stalker: { type: { domain: room }, default: chapel, owner: engine }
entities:
  room: { members: [chapel, ward, cellar] }
```

조건에서는 멤버와 비교합니다. `run.stalker == 'ward'`. 전체 종류에 답하는 비트에서는 해당 이벤트가 발생한 대상 멤버와 비교합니다. `run.stalker == occasion.target`([종류 대상](/ko/language/beats/#kind-대상) 참조). `<match subject="run.stalker">`는 종류의 모든 멤버를 포함하거나 `<otherwise>`를 가져야 합니다(`E-NONEXHAUSTIVE`). `domain:`이 열거형을 가리키든 종류를 가리키든 리터럴은 그 멤버 중 하나여야 합니다. `run.stalker == 'wrd'`는 did-you-mean을 포함한 `E-WHEN-LITERAL-DOMAIN`이고, 이러한 경로에 `::set`으로 멤버가 아닌 값을 넣으면 `E-SET-TYPE`입니다. 이 경로는 해당 종류를 읽는 것으로 간주되므로 종류에는 `W-DOMAIN-UNREAD`가 발생하지 않습니다.

### 엔터티마다 하나의 경로: `per:`

동료의 호감도처럼 그룹의 각 멤버에 대해 유지하는 숫자는 멤버마다 한 줄씩 작성하는 대신 **`per:`**가 있는 하나의 `int` 또는 `double` 선언으로 지정합니다(dsl 0.24.0 §3).

```yaml
state:
  run.approval: { type: int, default: 0, per: companion }
entities:
  person:    { members: [isolde, corvin, hollis] }
  companion: { subsetOf: person, members: [isolde, corvin] }
```

이는 각각 `{ type: int, default: 0 }`인 `run.approval.isolde`와 `run.approval.corvin`을 선언하며, 컴파일된 상태 테이블에는 멤버마다 항목 하나가 들어갑니다. 콘텐츠에서는 이름으로 멤버를 지정합니다: `::set{run.approval.isolde += 1}`, `when="run.approval.corvin >= 3"`, `{{run.approval.isolde}}`. 해당 패밀리 자체는 경로가 아니므로 `when="run.approval > 1"`은 `E-UNDECLARED`이며, 메시지는 해당 경로가 엔터티 인덱스 경로이고 멤버를 요구한다고 알려 줍니다. 이름 이외의 방식으로 멤버에 접근하는 곳은 세 군데입니다.

- Datalog 규칙은 변수로 읽습니다. `cel("run.approval[P] >= 3")` ([Facts and Datalog](/ko/state/facts-and-datalog/#규칙-가드의-엔티티-색인-상태) 참조).
- 컴포넌트 또는 비트 템플릿의 헤더는 매개변수로 이름을 지정합니다. `F[@param]`. `effects: true`인 컴포넌트는 `::set{run.approval[@who] += @delta}`를 쓰며, 각 `::use`는 인수가 지정한 멤버에 씁니다. 종류 밖의 인수는 `E-COMPONENT-ARG`입니다([컴포넌트](/ko/language/components-and-extends/) 참조). [비트 템플릿](/ko/language/beats/#비트-템플릿-use) 헤더는 `when: "run.approval[@who] >= 3"`을 읽습니다. 점 형식 `run.approval.@who`도 여기서 작동하며, 대괄호 형식을 사용하라는 `W-TEMPLATE-DOT-PARAM` 힌트가 함께 표시됩니다.
- 종류를 대상으로 하는 비트나 엔트리(`target="kind:<kind>"`) 또는 종류의 각 멤버마다 한 번씩 실행하는 비트나 엔트리(`for="kind:<kind>"`)는 실행 대상 멤버를 `occasion.target`으로 지정합니다. 즉 `when="run.approval[occasion.target] >= 3"`으로 읽고 `{{run.approval[occasion.target]}}`으로 보며, `::set{run.approval[occasion.target] += 1}`로 씁니다([종류 대상](/ko/language/beats/#kind-대상) 참조). 그 밖의 위치에서 `occasion.target`을 사용하면 `E-UNDECLARED`입니다.

family를 인덱싱하는 다른 방법은 없습니다. 상태 경로로 인덱싱한 `::set`은 `E-SET-SHAPE`입니다: `` `run.approval[run.fav]`: a `::set` path is indexed only by `[occasion.target]` (in a beat or entry that targets a kind or runs for each member of one) or a component's `[@param]` ``. `{{…}}`에서 읽을 때도 마찬가지입니다(`E-CEL-PROFILE`). 대신 `<match subject="run.fav">`로 멤버를 선택하세요.

`per:`는 해당 경로와 같은 문서에 선언된 **폐쇄형** 엔터티 종류, 즉 `members:`가 있는 종류를 지정합니다. `open:`으로 선언된 종류, 문서에서 선언하지 않은 종류, 또는 잘못된 종류는 `E-STATE-DECL`입니다. 검사기가 그 종류가 선언할 경로를 열거할 수 없기 때문입니다. 종류는 [하위 종류](/ko/state/facts-and-datalog/#하위-종류-subsetof)일 수 있습니다. 경로를 종류로 인덱싱하는 것은 종류를 읽는 것으로 간주되므로 `W-DOMAIN-UNREAD`가 발생하지 않습니다.

멤버마다 시작값을 다르게 지정할 수 있습니다. `default:` 맵은 각 멤버에 고유한 값을 주며, `_`는 이름을 지정하지 않은 멤버의 대체값입니다.

```yaml
state:
  run.approval: { type: int, default: { _: 0, isolde: 2 }, per: companion }
entities:
  companion: { members: [isolde, corvin] }
```

`run.approval.isolde`는 2에서 시작하고 `run.approval.corvin`은 0에서 시작합니다. 컴파일된 상태 테이블에는 멤버별 기본값이 들어가며, `check`, `trace`, `run`, `play`가 모두 이를 읽습니다. 맵은 엄격하게 검사되며, 다음 각각은 `E-STATE-DECL`입니다. 멤버가 아닌 키, 경로 타입의 스칼라가 아닌 값, 자체 값도 `_`도 없는 멤버, `per:`가 없는 경로의 맵 `default:`, 목록 `default:`.

## 읽기와 쓰기

`::set{path <op> celExpr}`는 디렉티브마다 경로 하나에 씁니다(`=`, `+=`, `-=`, `*=`). 쓰기 대상은 `scene.*` / `run.*` / `user.*`이며, `app.*`은 **콘텐츠에서 읽기만 가능**합니다(설정 계층이 소유하므로 `::set{app.*}`은 정적 오류). 엔진이 쓰는 모든 경로도 마찬가지입니다([아래의 엔진이 쓰는 경로](#엔진이-쓰는-경로) 참조).

확정 대입은 **경로에 따라 달라집니다**. 선언되지 않은 경로를 읽으면 `E-UNDECLARED`입니다. `scene.*` 읽기에는 일반적인 흐름 분석이 적용됩니다. `run`/`user`/`app` 경로는 스키마 `default`가 없는 한 장면 진입 시점에 **설정되지 않았을 수 있으며**, 진입 후에는 지배하는 `::set{p = …}` 쓰기나 가드(식별자 경로에는 `has(p)`, 식별자가 아닌 키에는 `'k' in m`)가 이를 증명합니다 — 그렇지 않으면 `E-MAYBE-UNSET`입니다. 복합 대입(`+=`/`-=`/`*=`)은 먼저 기존 값을 읽으므로 `=`만 경로의 첫 쓰기가 될 수 있습니다. 기본값이 있는 경로는 항상 대입된 상태이며, 검사기와 엔진은 하나의 스키마 스냅숏을 공유하므로 서로 다른 결과를 낼 수 없습니다.

쓰기에는 자체 가드를 붙일 수 있습니다. `::set{run.best = run.floor when="run.floor > 3"}`(dsl 0.24.0 §1)은 조건이 참일 때만 쓰며, 이를 감싼 한 갈래 `<match>`와 같습니다. 쓰기가 실행되지 않을 수 있으므로 **절대로** 확정 대입이 되지 않습니다. 해당 쓰기가 쓰는 기본값 없는 경로를 나중에 읽어도 여전히 설정되지 않았을 수 있습니다.

```lute expect="E-MAYBE-UNSET"
---
kind: scene
id: tower.landing
state:
  run.floor: { type: int, default: 1 }
  run.best: { type: int }
---

## The Landing

::set{run.best = run.floor when="run.floor > 3"}
@narrator: Your best is floor {{run.best}}.
```

`run.best`에 기본값을 지정하거나 `has(run.best)`로 읽기를 가드하세요. `when=`은 줄의 가드와 같은 방식으로 검사됩니다. 절대로 참이 될 수 없는 가드는 `E-ARM-DEAD`이며, `::set`이 받는 속성은 `when=`뿐입니다. 연산자 뒤의 나머지는 모두 식이기 때문입니다.
## 엔진이 쓰는 경로

일부 경로는 엔진에 속합니다. 콘텐츠는 상태를 읽는 곳이라면 어디서든(가드, `<match subject>`, 보간) 이 경로를 읽을 수 있지만, 절대로 쓰지는 않습니다:

| 경로 | 기록하는 주체 | 해당 경로의 `::set` |
|---|---|---|
| `app.*` | 설정 계층 | `E-APP-READONLY` |
| `quest.<id>.state`, `quest.<id>.activatedAt`, `quest.<id>.objectives.<o>.done`, 그리고 0.24.0부터 `quest.<id>.failedBy`, `quest.<id>.objectives.<o>.failed` | 퀘스트 수명 주기([퀘스트와 씬](/ko/language/quests-and-scenes/) 참고) | `E-QUEST-RESERVED-WRITE` |
| `entry.<id>.read` | 엔진, 로어 엔트리가 런에서 처음 제시될 때 — **run** 등급 | `E-QUEST-RESERVED-WRITE` |
| `entry.<id>.everRead` | 엔진, 로어 엔트리가 처음 제시될 때 — **user** 등급 | `E-QUEST-RESERVED-WRITE` |
| `prev.run.<path>` | 엔진, 런이 끝날 때: 그때 `run.<path>`가 갖고 있던 값([아래](#이전-런) 참고) | `E-QUEST-RESERVED-WRITE` |
| `prev.season.<name>.<field>` | 엔진, 선언된 [시즌](/ko/state/schemas/#시즌)이 다시 열릴 때: 그때 `season.<name>.<field>`가 갖고 있던 값(0.27.0) | `E-QUEST-RESERVED-WRITE` |
| `clock.index`, `clock.weekday`, `clock.weekdayLabel` | 스키마에 선언된 [시계](/ko/language/clock/)의 날과 슬롯에서 파생(0.24.0) | `E-QUEST-RESERVED-WRITE` |
| 스키마가 `owner: engine`을 선언한 경로 | 엔진 | `E-ENGINE-OWNED-WRITE` |

퀘스트와 엔트리 경로는 이름이 예약되어 있습니다. 모든 문서가 선언하지 않고도 읽을 수 있으며, `state:`에 선언하면 `E-QUEST-RESERVED-DECL`입니다. 두 엔트리 플래그는 `bool`이며, 엔트리가 처음 제시될 때까지 `false`입니다([로어 엔트리](/ko/language/lore-entries/) 참고). `entry.<id>.read`는 런과 함께 초기화되므로 새 런의 첫 읽기에서 엔트리 효과가 다시 적용됩니다. `entry.<id>.everRead`(0.22.0)는 처음 읽을 때 설정되며 새 런에서도 초기화되지 않습니다.

두 퀘스트 경로는 무언가가 실패한 이유를 말합니다. `quest.<id>.failedBy`는 퀘스트가 실패할 때까지 `unset`을 읽고, 그 뒤 원인을 이름으로 읽습니다: `fail`(퀘스트의 `fail` 술어), `by` 또는 `until`(목표의 기한), `subquest`(필수 하위 퀘스트 실패), `cascade`(부모 퀘스트 실패), 또는 `superseded`(`complete="any"` 부모가 다른 대안을 통해 완료됨)입니다. `quest.<id>.objectives.<o>.failed`는 목표의 `by` 또는 `until`이 목표를 실패시킨 뒤 `true`입니다. 따라서 에필로그는 `<match subject="quest.hunt.failedBy">`로 놓친 기한과 선택하지 않은 길을 구분할 수 있습니다. run 등급 퀘스트가 초기화되면 둘 다 지워집니다.

`clock.*` 경로는 스키마가 `clock:`을 선언한 경우에만 존재합니다. 선언하지 않았는데 `clock.index`를 읽으면 `E-UNDECLARED`입니다. `clock.index`는 하루 1의 시작부터 위치를 셉니다(슬롯이 있으면 슬롯 단위, 슬롯이 없는 시계면 전체 날짜 단위). 따라서 값은 계속 증가하기만 하며, `clock.weekday`와 `clock.weekdayLabel`에는 시계의 `week:`가 필요합니다. [시계](/ko/language/clock/)를 참고하세요.

### `owner: engine`

엔진만 써야 하는 자체 상태 — 날짜 카운터, 런의 결과, 게임 루프가 진행하는 무엇이든 — 를 선언할 때는 선언에 **`owner: engine`**을 추가하세요(0.22.0):

```yaml
state:
  run.day:        { type: int, default: 1, owner: engine }
  run.outcome:    { type: { enum: [fell, fled, won] }, owner: engine }
  user.bond.mara: { type: int, default: 0 }
```

읽기에는 제한이 없습니다. `when="run.day > 1"`과 `{{run.day}}`는 일반적인 읽기입니다. 콘텐츠가 해당 경로 또는 그 아래 필드를 `::set`으로 쓰면 `E-ENGINE-OWNED-WRITE`입니다:

<!-- lute-diagnostics -->
```
./scenes/town/day-end.lute:12:8: error [E-ENGINE-OWNED-WRITE] `::set` cannot write `run.day`: it is declared `owner: engine` — the engine writes it and content may only read it; in `lute play` write it with an `engine:` step, in a trace/test with a mock
```

`engine`은 선언이 이름 붙일 수 있는 유일한 소유자입니다. 다른 `owner:` 값은 `E-STATE-DECL`이며, `owner:`가 없는 경로는 계속 콘텐츠가 씁니다. 이 키는 경로를 쓸 수 있는 주체만 바꾸고 타입, 계층 또는 기본값은 바꾸지 않습니다. 콘텐츠를 바인딩하므로 검사기가 이를 적용하지만 컴파일된 아티팩트에는 포함되지 않습니다. 상태 테이블의 경로 항목은 이것이 있을 때나 없을 때나 같습니다. `lute context`는 상태 목록에서 이런 경로를 `(owner: engine)`으로 표시합니다.

도구 체인에서 엔진의 쓰기는 콘텐츠 바깥에서 들어옵니다. [`lute play`](/ko/tooling/play/) 스크립트는 `engine:` 스텝으로 이를 씁니다(리터럴 또는 숫자 델타를 위한 `{ add: n }`). [`lute trace`](/ko/tooling/tracing/)와 `lute test`는 mock으로 시드합니다:

```yaml
steps:
  - occasion: dayEnd
  - label: the engine closes the day
    engine:
      state: { run.day: { add: 1 } }
```

따라서 하네스는 플레이스루를 진행하기 위해 `::set`으로 엔진 상태를 설정하는 대역 씬이 필요하지 않습니다. `owner: engine`이 그것을 거부하는 역할을 하기 때문입니다. 팩트에서는 [`reserved: true` 관계](/ko/state/facts-and-datalog/)가 같은 역할을 합니다. 콘텐츠는 이를 assert하거나 retract하지 않으며, `engine:` 스텝은 이를 쓸 수 있습니다.

### 이전 런

런 사이의 허브에서는 방금 끝난 런 — 플레이어가 어디서 쓰러졌는지, 얼마나 진행했는지, 어떻게 떠났는지 — 에 대해 이야기하고 싶을 때가 많습니다. **`prev.run.<path>`**(dsl 0.23.0)는 이전 런이 끝났을 때 `run.<path>`가 갖고 있던 값을 읽습니다.

```lute check
---
kind: scene
id: hearth.welcomeBack
state:
  run.floor: { type: int, default: 0 }
  run.outcome: { type: { enum: [fell, fled, won] } }
---

## The hearth

<match subject="prev.run.outcome">
  <when is="fell">
    @wren: You fell, last time. Slower, this time.
  </when>
  <when is="fled">
    @wren: You ran. No shame in that.
  </when>
  <when is="won">
    @wren: Back already? You won, last time.
  </when>
  <when is="unset">
    @wren: First climb? Stay close to the wall.
  </when>
</match>
@wren{when="has(prev.run.floor) && prev.run.floor >= 5"}: Floor five, though. Better than most.
```

- **아무것도 선언하지 않습니다.** 선언된 모든 `run.<path>`에는 같은 타입의 미러 `prev.run.<path>`가 생깁니다. `prev.*`는 예약되어 있으므로 그 아래 경로를 선언하면 `E-STATE-NAMESPACE`이고, `prev.run.flor` 같은 오타는 did-you-mean과 함께 `E-UNDECLARED`입니다.
- **설정되지 않을 수 있습니다.** 첫 런이 시작할 때는 끝난 런이 없으므로 미러에 기본값이 없습니다. `run.<path>`에 기본값이 있더라도 모든 읽기에는 `has(…)` 가드 또는 `unset` arm이 필요하며, 그렇지 않으면 `E-MAYBE-UNSET`입니다. 위의 `unset` arm은 첫 런과 `run.outcome`을 설정하지 않고 끝난 런에 응답합니다.
- **읽기 전용입니다.** 콘텐츠가 이를 `::set`으로 쓰면 `E-QUEST-RESERVED-WRITE`입니다.
- **엔진이 스냅숏을 만듭니다.** 런이 끝나면 엔진은 새 런이 `run.*`을 초기화하기 전에 모든 `run.*` 값을 `prev.run.*`으로 복사합니다. 미러는 검사기의 선언이지 컴파일된 상태 테이블의 행이 아니므로 아티팩트는 바뀌지 않습니다.

도구 체인에서 [`lute play`](/ko/tooling/play/)는 모든 `newRun` 스텝에서 스냅숏을 만듭니다. 플레이 스크립트의 `state:` 시드나 trace mock도 `prev.run.*`을 직접 설정할 수 있으므로, 더 나중 런에서 시작할 수 있습니다.
