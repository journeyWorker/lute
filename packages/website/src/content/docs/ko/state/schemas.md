---
title: 상태 스키마
description: 실행/사용자/앱 상태, 정의, 엔터티, 캐스트, 시계, 시즌 및 게임 종료에 관한 공유되고 임포트되는 단일 소스 — 스키마 문서 형태와 uses가 피어를 구성하고 extends가 기본 계층을 세분화하는 방식입니다.
---

`run` / `user` / `app` 계층은 게임/시즌 전역입니다. 저장된 하나의 값이 장면별 타입을 담을 수는 없습니다. 따라서 이들은 모든 장면이 `uses:`로 임포트하는 단일 **스키마 문서**에 들어갑니다. 장면은 자신의 `scene.*` 로컬만 선언합니다. 0.2.2부터 선언 파일은 평범한 `.yaml`입니다(`---`/Lute 엔벨로프 없음) — 장면이 아닌 순수 선언입니다.

## 스키마 형태

스키마는 `state:`(스칼라 계층), `defs:`(이름이 지정된 타입 CEL 매크로), `enums:`(선언된 멤버 목록)를 선언하며, 관계형 계층을 사용하는 경우에는 `entities:` / `relations:` / `facts:` / `rules:`도 선언합니다. 프로젝트의 `cast:`(dsl 0.23.0), `0.24.0`부터 하나의 `clock:`, `0.27.0`부터 `seasons:` 및 [`terminal:`](#게임-종료-terminal) 조건도 선언할 수 있습니다.

```yaml
state:
  run.choseHelp: { type: bool, default: false }
  run.day:       { type: int, default: 1, owner: engine }
  user.level:    { type: int, default: 1 }
defs:
  helped: { type: bool, cel: "run.choseHelp" }
```

각 `<path>` 세그먼트는 이름입니다(문자, 숫자, `_` 또는 `-`이며 `-`로 시작하지 않음). 조건에서는 식별자가 아닌 세그먼트를 인용하여 `run.visits["lab-b2"]`처럼 씁니다. 선언은 `{ type, default?, owner?, per? }`입니다. `type: { domain: <name> }`은 이름이 지정된 열거형 또는 엔터티 종류에서 멤버를 가져옵니다. 이는 맵 종류 위에서 엔진이 유지하는 위치의 타입을 지정하는 방법입니다. [이름이 지정된 열거형으로 타입이 지정된 경로](/ko/state/state-model/#이름-있는-열거형으로-타입을-지정한-경로)를 참조하세요. `default`는 스키마를 로드할 때 계층의 초기 상태에 **구체화되고**, 엔진이 해당 계층의 리셋을 실행할 때마다 다시 구체화됩니다. 따라서 기본값이 있는 경로에는 항상 값이 할당되며, 체커와 엔진은 동일한 스냅숏을 읽습니다. `owner: engine`(0.22.0)은 콘텐츠가 읽을 수 있지만 `::set`할 수 없는 경로(`E-ENGINE-OWNED-WRITE`)를 표시합니다. `engine`이 가능한 유일한 값입니다 — [`owner: engine`](/ko/state/state-model/#owner-engine)을 참조하세요. `per: <kind>`(0.24.0)는 폐쇄된 엔터티 종류의 각 멤버마다 하나의 경로를 선언합니다 — [엔터티별 경로 하나](/ko/state/state-model/#엔터티마다-하나의-경로-per)를 참조하세요.

`enums:` 블록은 두 가지 역할을 합니다. 해당 도메인은 [관계형 계층](/ko/state/facts-and-datalog/)의 인수 타입이며, 언어 `0.9.0`부터는 프로젝트가 **콘텐츠 어휘**를 선언하는 방법이기도 합니다 — `emotion`, `action`, `anchor`, `costume`, `mood`, `volume`, `musicPlayback`, `vfxType`, `framing`, `cameraMove`, `transition`, `cgLayout`, `sequence`, `textStyle`입니다. 컴파일러는 이러한 슬롯에 대한 멤버를 제공하지 않으므로 `uses:`를 통해 도달하는 스키마를 멀티 문서 프로젝트에서 이들을 선언할 때 우선해야 합니다. `action`과 `anchor`는 추가로 필수 멤버 의미를 가집니다. 이는 이 페이지에서 다루는 스칼라 계층과는 별개의 문제입니다 — [콘텐츠 어휘](/ko/language/vocabulary/)를 참조하세요.

### 0.24.0에서 추가된 키

일일 시계가 있는 파티 게임의 스키마는 이들 대부분을 한 번에 사용합니다.

```yaml
state:
  run.day:      { type: int, default: 1, owner: engine }
  run.slot:     { type: { domain: slot }, default: dawn, owner: engine }
  run.approval: { type: int, default: 0, per: companion }
enums:
  slot:
    members: [dawn, noon, dusk]
    labels: { dawn: Dawn, noon: Midday, dusk: Dusk }
  emotion: [calm, fierce, tired]
entities:
  person:    { members: [isolde, corvin, hollis] }
  companion: { subsetOf: person, members: [isolde, corvin] }
relations:
  inParty: { args: [companion], tier: run }
clock:
  day: run.day
  slot: run.slot
  slots: [dawn, noon, dusk]
cast:
  isolde: { name: Isolde, present: "holds('inParty', ['isolde'])", emotions: [calm, fierce] }
  corvin: { name: "Corvin Hale", present: "holds('inParty', ['corvin'])" }
  hollis: { name: Hollis }
```

- **`clock:`**은 날짜를 보유하는 `owner: engine` 경로를 지정하며, 슬롯이 있는 시계의 경우 슬롯과 슬롯 순서를 보유하는 경로도 지정합니다(시계는 전체 일수만 셀 수도 있습니다). 스키마는 최대 하나만 선언할 수 있으며, 잘못된 시계는 `E-CLOCK-DECL`입니다. 읽기 전용 `clock.*` 경로와 `once: day` / `once: slot` 비트를 추가하고, `week:`를 선언하는 경우 0.27.0부터 `once: week`도 추가합니다. [시계](/ko/language/clock/)를 참조하세요.
- **`labels:`**는 긴 형식의 열거형에서 멤버의 표시 텍스트를 제공합니다. `{{run.slot}}`은 `Dawn`으로 렌더링되지만 조건은 계속 `run.slot == 'dawn'`으로 비교합니다. 멤버가 아닌 값에 대한 레이블은 `E-ENUM-LABEL-NOT-MEMBER`이고, 문자열이 아닌 레이블은 `E-META-VALUE`입니다.
- **`subsetOf:`**는 엔터티 종류에서 멤버가 모두 부모에 속하는 하위 종류를 선언합니다. [하위 종류](/ko/state/facts-and-datalog/#하위-종류-subsetof)를 참조하세요.
- **`per:`**는 상태 경로에서 `run.approval.isolde`와 `run.approval.corvin`을 선언합니다. `default:`는 모든 멤버에 적용되는 하나의 값일 수도 있고, 멤버별 값과 `_` 대체값을 포함한 맵 `{ _: 0, isolde: 2 }`일 수도 있습니다. [엔터티별 경로 하나](/ko/state/state-model/#엔터티마다-하나의-경로-per)를 참조하세요.
- **`present:`, `emotions:`, `assume:`**는 캐스트 항목에서 캐릭터가 플레이어와 함께 있는 시기, 해당 캐릭터의 대사에서 사용할 수 있는 `emotion=` 값, 그리고 현재 상태가 엔진의 `reserved:` 사실을 부재로 취급할 수 있는지를 지정합니다. 스키마에서 파싱되지 않는 `present:`는 `E-CEL-PARSE`, CEL 프로필을 벗어나는 것은 `E-CEL-PROFILE`, 스키마의 `emotion` 열거형에 없는 `emotions:` 멤버는 `E-BAD-ENUM`이며, 각각 항목의 키에 보고됩니다. [캐스트](/ko/language/dialogue-and-cast/#캐스트)를 참조하세요.

엔터티 종류에는 한 가지 표기만 있습니다. 닫힌 종류는 `kind: { members: [a, b] }`,
열린 종류는 `kind: { open: true }`로 작성합니다. YAML 목록 단축형 `kind: [a, b]`는
허용되지 않으며, `E-ENTITY-KIND-SHAPE`가 작성한 멤버를 넣은 정확한
`{ members: [a, b] }` 형태를 보여 줍니다.

### 시즌

라이브 게임에서는 오고 가는 이벤트가 실행됩니다. 매년 가을의 수확 축제나, 스튜디오가 원할 때 켜는 별똥별 주간 같은 이벤트입니다. 각 기간은 새로 시작하며 직전 기간이 어떻게 끝났는지 돌아볼 수도 있습니다. 스키마는 이런 기간을 **시즌**(dsl 0.27.0 §5)으로 선언하고, 각 시즌이 활성 상태인지 알려 주는 조건도 함께 선언합니다.

```yaml
state:
  user.month:             { type: int, default: 1, owner: engine }
  user.starfallOn:        { type: bool, default: false, owner: engine }
  season.harvest.bushels: { type: int, default: 0 }
  season.starfall.wishes: { type: int, default: 0 }
defs:
  harvestLive:  { type: bool, cel: "user.month >= 9 && user.month <= 10" }
  starfallLive: { type: bool, cel: "user.starfallOn" }
seasons:
  harvest:  { live: "@harvestLive" }
  starfall: { live: "@starfallLive" }
```

각 시즌은 이름이 지정된 상태 계층입니다.

- **`season.<name>.<field>`** 경로는 `run.*` 및 `user.*`와 마찬가지로 `state:` 아래에 기본값과 함께 선언하며, 콘텐츠에서 이를 읽고 씁니다(`::set{season.harvest.bushels += 1}`).
- **`prev.season.<name>.<field>`**는 읽기 전용 미러입니다. 시즌의 직전 기간이 끝났을 때의 값을 담습니다. 시즌이 두 번째로 열릴 때까지 설정되지 않으므로 [`prev.run.*`](/ko/state/state-model/#이전-런)와 마찬가지로 `has`로 읽기를 보호하세요. 여기에 쓰면 `prev.run.*`와 마찬가지로 `E-QUEST-RESERVED-WRITE`입니다.
- 장면 비트의 **`once: season:<name>`**(항목 또는 번들 비트에서는 `once="season:<name>"`)는 시즌이 다시 열릴 때까지 해당 비트를 소진합니다. [비트](/ko/language/beats/#장면-비트-키)를 참조하세요.
- **`<quest tier="season:<name>">`**은 시즌이 다시 열릴 때 퀘스트를 `unset`으로 되돌립니다. [다시 돌아오는 퀘스트](/ko/language/quests-and-scenes/#돌아오는-퀘스트-시즌-tier와-rearm)를 참조하세요.

시즌은 `live` 조건이 거짓에서 참으로 바뀔 때 **열립니다**. 그 순간 엔진은 현재 `season.<name>.*` 값을 `prev.season.<name>.*`으로 복사하고, `season.<name>.*`을 선언된 기본값으로 리셋하며, 시즌의 `once` 소진을 지우고, 시즌 계층 퀘스트를 `unset`으로 되돌립니다. 닫힐 때는 아무것도 리셋되지 않습니다. 시즌은 서로 독립적이며 자유롭게 겹칩니다. harvest와 starfall이 동시에 활성 상태일 수도 있습니다. `lute play`는 각 전환과 함께 리셋한 내용 및 직전 기간이 끝날 때의 값을 표시합니다.

```
season harvest opens — season.harvest.* reset to defaults; last window: prev.season.harvest.tokens = 2
season harvest closes
```

```lute
@narrator{when="has(prev.season.harvest.bushels)"}: Last harvest you brought in {{prev.season.harvest.bushels}} bushels.
::set{season.harvest.bushels += 1}
```

컴파일된 아티팩트와 `project.index.json`은 선언을 다음과 같이 담습니다.
`seasons: [{ name, live: {cel, expr, authored?} }]`. 여기서 `live` 조건은 `@def` 확장 후의 것이므로, 엔진은 체커가 읽은 동일한 소스에서 각 시즌을 열고 리셋합니다.

시즌과 관련된 모든 오류는 `E-SEASON-DECL`입니다. 즉 맵이 아닌 `seasons:` 항목, 누락되었거나 비어 있는 `live`, 알 수 없는 키, 잘못된 이름, 하나의 시즌을 서로 다르게 선언한 두 스키마, 그리고 선언되지 않은 시즌을 이름으로 지정하는 `season.<name>.*` 경로, `once: season:<name>`, `tier="season:<name>"`입니다.

### 게임 종료: `terminal:`

일부 게임은 끝납니다. 병동이 플레이어를 데려가거나 실행의 운명이 결정되는 경우입니다. 스키마는 **`terminal:`** CEL 조건으로 종료 시점을 지정할 수 있습니다. 이 조건이 성립하면 게임이 끝나고, 해당 계기가 [`outsideRun: true`](/ko/plugins/manifests/)를 선언하지 않는 한 타이틀 화면이나 실행 사이의 메뉴를 포함해 엔진은 계기를 발생시키지 않습니다.

```yaml
state:
  run.fate:   { type: { domain: fate }, default: open }
  run.sanity: { type: int, default: 10 }
enums:
  fate: [open, escaped, taken]
terminal: "run.fate == 'taken' || run.sanity <= 0"
```

`terminal:`은 스키마 문서만 선언하며 `lute.project.yaml`은 절대 선언하지 않습니다. `defaults.uses`가 이미 하나의 스키마를 프로젝트 전역에 적용하므로, 매니페스트 키를 두면 같은 내용을 확인할 위치가 하나 더 생깁니다. 여러 스키마가 각각 하나씩 선언할 수 있으며, 프로젝트 조건은 이들의 `||`이고 어느 하나라도 성립하면 게임이 끝납니다. 프런트매터 JSON 스키마(`schemas/lute.schema.json`)는 이 키를 두 형식 모두 허용합니다. 조건 문자열 또는 아래의 긴 형식 `{ when, persists }`입니다.

체커는 다른 조건과 같은 방식으로 이 조건을 읽습니다.

- 텍스트는 조건 슬롯으로 검사됩니다. 오류에는 `` `terminal: …`: `` 접두사가 붙고, 임포트된 스키마의 `terminal:` 오류는 해당 스키마의 줄에서 보고됩니다.
- 문자열도 아니고 `{ when, persists }` 매핑도 아니며 조건을 지정하지 않는 값은 `E-META-VALUE`입니다. 즉 `terminal:`은 게임이 끝나는 시점을 지정해야 합니다.
- 조건이 성립하면 계기가 발생하지 않으므로 모든 비트는 `!terminal` 아래에서 판단됩니다. 게임이 아직 끝나지 않았는데 `when`이 명백히 거짓인 비트, 또는 `run.fate == 'taken'`을 필요로 하는 비트는 `E-BEAT-UNREACHABLE`입니다. 동일한 경계가 계기의 [`raisedWhen`](/ko/plugins/manifests/#계기-게이트-raisedwhen)을 통해 계기를 한 번에 하나씩 제한합니다.

[`lute play`](/ko/tooling/play/)는 엔진을 따릅니다. `terminal:`이 성립한 뒤의 단계에는 게임이 끝났다는 메모가 추가되고, 그 지점에서 끝나는 플레이스루는 `end: terminal`을 보고합니다(`--json` 루트의 `"end": "terminal"`). 이후의 `occasion:` 또는 `advance:` 단계는 `E-OCCASION-GATE`(종료 코드 1)입니다. `newRun: true` 단계는 새 실행을 시작하며, 새 실행에서 조건이 거짓이 될 때만 플레이가 계속됩니다. 실행 상태에 대한 `terminal:`은 새 실행으로 리셋되지만, 새 실행에서 유지되는 것(`user.*`, `visited(…)`)을 읽는 조건은 리셋되지 않습니다. 긴 형식에서 `persists: true`라고 하지 않으면 `check-project`가 `W-TERMINAL-PERSISTENT`를 경고합니다. `persists: true`이면 메모와 거부 메시지가 게임이 영원히 끝났다고 알리며 `newRun`을 절대 제안하지 않습니다.

조건은 계기가 발생할 때 판단되므로, 조건을 성립시키는 발생은 끝까지 진행됩니다. 게임을 끝내는 쓰기가 포함된 비트는 완료되고, `select: sequence` 발생의 나머지도 실행되며, 해당 발생으로 정리되는 `questComplete` / `questFailed` 핸들러가 마지막으로 실행을 확정합니다. 에필로그는 해당 쓰기 뒤에 오거나, 퀘스트의 `terminal:`이 읽는 `<on event="questComplete">` 핸들러 안에 옵니다. 콘텐츠의 `::end`는 게임을 절대 끝내지 않습니다. "end"의 모든 의미는 [게임이 끝남](/ko/tooling/play/#게임이-끝났을-때)을 참조하세요.

컴파일된 아티팩트와 `project.index.json`은 최상위에 다음과 같이 조건을 담습니다.
`terminal: { cel, expr, authored? }`. 여러 스키마가 하나씩 선언한 경우 `@def` 확장 후 `||`로 결합되므로, 엔진은 체커가 읽은 동일한 소스에서 계기 발생을 중지합니다. 그 옆의 `terminalPersists: true`는 지속되는 종료를 표시합니다(그렇지 않으면 생략). 게임이 끝난 뒤 새 실행을 제공하는 엔진은 이 경우 새 실행을 제공하지 않습니다.

#### 실행을 초월하는 종료: `persists: true`

새 실행은 보통 게임을 다시 엽니다. 실행 상태에 대한 `terminal:`은 새 실행으로 리셋됩니다. 로그라이크의 영구 엔딩처럼 모든 실행을 초월해야 하는 종료는 새 실행에서도 유지되는 상태를 읽고, 긴 형식으로 이를 표시합니다.

```yaml
state:
  user.crowned: { type: bool, default: false }
terminal: { when: "user.crowned", persists: true }
```

`when`은 조건이며, 짧은 형식에서 작성하는 방식과 정확히 같습니다. `persists`는 `true` 또는 `false`입니다(기본값은 `false`이며 짧은 형식과 같습니다). 긴 형식은 다른 키를 받지 않습니다. `persist:`처럼 잘못 쓰면 `E-META-VALUE`가 되고, 의도한 키를 표시합니다. `persists: true`는 설계를 명시하므로 `W-TERMINAL-PERSISTENT`가 조용히 유지되고, `lute play`는 새 실행을 제공하는 대신 게임이 영원히 끝났다고 말합니다. 새 실행에서 잊히는 상태(`run.*`, 시계, 실행 계층 퀘스트 및 관계)만 읽는 조건에서 `persists: true`는 `E-META-VALUE`입니다. 그러한 종료는 실행을 초월할 수 없습니다. 여러 스키마가 있는 경우 프로젝트의 종료가 지속되는 것은 모든 선언이 그렇게 말할 때뿐입니다.

### 사용되지 않는 Def

`check-project`는 선언된 `@def`를 콘텐츠, 다른 def 또는 규칙 가드가 참조하지 않을 때 `W-DEF-UNUSED`(dsl 0.24.0 §7)를 보고합니다. 프로젝트당 한 번, 선언 위치에서 보고됩니다. 스키마 파일의 줄 또는 문서 자체의 `defs:` 키 위치입니다. 플레이 스크립트와 테스트는 사용처가 아닙니다. 관계형 대응 항목은 `W-RELATION-UNREAD`입니다([읽는 항목이 없는 관계](/ko/state/facts-and-datalog/#아무도-읽지-않는-관계)를 참조).

호스트만 읽는 스키마 파일의 def(예: 게임의 `Program.def`)는 죽은 코드가 아닙니다. 메시지가 안내하듯 `host: true`로 표시하면 경고가 나오지 않습니다(0.41.1). 문서 자체의 frontmatter def는 `host:`를 받을 수 없으므로(`E-DEF-DECL`), 그 경고는 def를 쓰거나 지우라고만 안내합니다.

<!-- lute-diagnostics -->
```
./world.schema.yaml:5:3: warning [W-DEF-UNUSED] def `stale` is declared but no `@stale` reference uses it anywhere in the project (content, other defs, or rule guards); use it, drop it, or mark it `host: true` if the host reads it (for example, `Program.def`)
```
## 구성: `uses`와 `extends`

가져오기는 **DAG**를 이룹니다. 사이클은 정적 오류이고(진단에는 체인이 출력됨), 모든 씬보다 먼저 스키마를 로드하고 검사하며, 가져오기 간 중복 `defs` 이름은 오류입니다(자동 섀도잉 없음). 한 파일로 가는 두 경로는 하나의 식별자로 해석됩니다.

`uses:`는 **피어** 스키마를 합칩니다 — 두 피어가 선언한 이름은 충돌입니다. `extends:`는 이 문서가 보강하는 하나 이상의 **기반** 스키마 이름을 지정합니다. 기반은 우선순위가 낮은 레이어이므로, 확장 문서(또는 그 피어)가 기반 이름을 다시 선언하면 확장 선언이 **재정의**합니다 — 중복 오류는 없습니다.

```yaml
# base.schema.yaml
state:
  run.blessed: { type: bool, default: false }
```

```yaml
# child.schema.yaml — refines the base
extends: base.schema.yaml
state:
  run.blessed: { type: bool, default: true }   # default-only override
```

우선순위는 낮은 순서에서 높은 순서로 적용됩니다. 문서의 `extends` 기반(재귀적으로) < 해당 문서의 `uses` 피어 < 문서 자체의 인라인 `state:`/`defs:`입니다. 저장된 상태는 안정적인 타입을 유지해야 하므로, 경로에 선언된 `type`을 변경하는 재정의는 `E-EXTENDS-STATE-TYPE`이고, 같은 타입의 `default`만 보강하는 것은 조용히 허용됩니다. `extends` 간선은 `uses:`와 동일한 사이클 / 파일 없음 / 파싱 진단(`E-USES-{CYCLE,NOT-FOUND,PARSE}`)을 재사용합니다.

### 여러 파일에 걸쳐 조합되는 엔터티 종류: `add:`

여러 작성자가 하나의 프로젝트를 공유하면 각 영역은 리드가 소유한 종류에 자신의 사람과 장소를
추가하려고 합니다. dsl 0.26.0 §2.3부터 스키마는 문서의 가져오기 중 정확히 하나가 선언한
종류에 **멤버를 추가**할 수 있습니다:

```yaml
# schema/roster.schema.yaml (the lead's)
entities:
  person:  { members: [professorOak, mom] }
  trainer: { subsetOf: person, members: [rival] }
```

```yaml unverified="an add: needs the base schema above beside it; both checked together with lute check-project"
# schema/areas/south.schema.yaml (one area's)
entities:
  person:  { add: [grannyWren, oldSalt] }
  trainer: { add: [youngsterTodd, lassMina] }
```

두 문서를 모두 가져오는 문서는 모든 멤버가 있는 `person`을 보며, 추가된 trainer의 멤버도 person에 속합니다
([하위 종류](/ko/state/facts-and-datalog/#하위-종류-subsetof)의 멤버는 부모에 속합니다). 다음 각각은
두 파일의 이름을 함께 표시하는 `E-ENTITY-KIND-SHAPE`입니다.

- 가져오기에서 선언하지 않은 종류에 대한 `add:`이며 유사 이름 제안을 제공합니다(`persn`이면 `person`을 제안함);
- 멤버가 엔진에 등록되는 `open:` 종류에 대한 `add:`;
- 한 선언에서 `members:` 또는 `open:`과 나란히 놓인 `add:`;
- 멤버가 두 번 나열된 경우: 두 `add:` 목록에서 나열되거나, 선언 위에 다시 추가되거나, 하나의
  종류 또는 열거형 자체 목록에 두 번 나열된 경우(dsl 0.26.0 §2.2). 두 번째 항목의 줄에서 두 줄을 함께 보고합니다.

<!-- lute-diagnostics -->
```
schema/areas/south.schema.yaml:3:35: error [E-ENTITY-KIND-SHAPE] entity kind `trainer` lists `lassMina` twice — in the `add:` of `schema/areas/east.schema.yaml` (line 7) and in the `add:` of `schema/areas/south.schema.yaml` (line 3); list each member once
```

### 문서 간 선언

두 문서의 프런트매터 `state:`에 선언된 경로는 하나의 런타임 값이므로, dsl 0.26.0
§2.1부터 `check-project`는 선언을 비교하고 `type`, `default`, `per` 또는 `owner`가 다르면
`E-STATE-DECL-CONFLICT`를 보고합니다([선언](/ko/state/state-model/#선언) 참고).

모든 임포터가 반복해서 보고하게 되는 스키마 오류 — 하위 종류 및 `add:`의 `E-ENTITY-KIND-SHAPE`,
`E-USES-DUP-STATE`, `E-USES-DUP-DEF`, `E-USES-DUP-RELATION`, 피어의 `E-KIND-NAME-CLASH`,
그리고 가져온 def의 `E-DEF-DECL` — 는 가져오는 문서마다 보고하지 않고 `check-project`가 한 번만,
`(imported by N documents)`로 끝나는 스키마 줄의 프로젝트 전체 진단으로 보고합니다
(dsl 0.26.0 §2.7). 임포터는 `ok` 상태로 남습니다.

상태 스키마는 *게임 콘텐츠*이며, 별도의 엔진 **기능 매니페스트**(엔진 어휘)와 구분됩니다. 후자는 자체 소유자와 변경 주기를 가집니다.
