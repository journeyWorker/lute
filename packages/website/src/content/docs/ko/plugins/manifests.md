---
title: 매니페스트와 해석
description: plugin.yaml 매니페스트와 그 export 파일, 횡단 stampAttrs, 코어 스테이징 레코드로의 선언적 낮춤, 설치된 플러그인이 결정적으로 하나의 capability 스냅숏으로 해석되는 방식입니다.
---

플러그인은 진입점이 하나의 `plugin.yaml`인 디렉터리입니다. `exports` 맵은 로더가 읽을 하위 디렉터리를 지정하며, 나열되지 않은 디렉터리는 무시됩니다. 모든 것은 하나의 플러그인 id 아래에 있는 선언적 YAML이며 — 소비자는 id만 참조합니다.

## `plugin.yaml`(매니페스트 진입점)

```yaml
id: arcia.minigame          # REQUIRED — reverse-dotted, globally unique id
version: 0.1.0              # REQUIRED — the plugin's own semver
kind: capability           # REQUIRED — only "capability" is defined
depends:                   # OPTIONAL — { id, range } against other plugins
  - { id: lute.core, range: "^0.0.1" }
exports:                   # REQUIRED — which sub-directories the loader reads (list only what you ship)
  directives: directives/
  state: state/
  providers: providers/
  bridge: bridge/
  assetKinds: assetkinds/
  defs: defs/
  stampAttrs: stampattrs/
  enums: enums/
  frontmatter: frontmatter/
  events: events/
  occasions: occasions/
  rewardKinds: rewardkinds/
  cast: cast/
  lints: lints/
  docs: docs/
options:                   # OPTIONAL — typed activation options
  - { name: resultScope,  type: { enum: [scene, run] }, default: scene }
  - { name: allowedKinds, type: { list: { enum: [rhythm, puzzle, timing] } }, default: [rhythm, puzzle, timing] }
```

`depends[].range`는 두 형태로만 고정됩니다 — 캐럿(`^x.y.z`, 1.0 이전 의미론) 또는 세 요소로 된 정확한 버전입니다. 그 밖의 표기는 정의상 만족시킬 수 없습니다.

옵션의 값은 활성 프로필이 준 값이고, 없으면 선언된 `default:`입니다. `{ enumFromOption: <option> }` 타입의 속성은 이 실제 값으로 검사합니다(0.41.1). 위 매니페스트와 플러그인을 켜기만 하는 프로필(`plugins: { arcia.minigame: true }`)에서는 `{ enumFromOption: allowedKinds }` 타입 속성의 `kind="racing"`이 `rhythm, puzzle, timing`에 대해 `E-BAD-ENUM`이며, 생략된 속성의 선언된 `default:`가 목록 밖이어도 마찬가지입니다. 프로필이 `allowedKinds: [rhythm]`을 주면 그 목록으로 좁혀집니다. 프로필 값도 `default:`도 없는 옵션이면 속성을 검사하지 않습니다.

## Export 파일

각 export 종류에는 규범 스키마가 있습니다. 모두 하나의 작은 매니페스트 타입 시스템(`bool` / `int` / `double` / `string`, `enum`, `list`, `record`, `map`, 그리고 `enumFromOption`, `providerRef`, `slotId`, `assetKind`, `domain`, shape 참조)으로 타입이 지정됩니다. dsl 0.26.0부터 디렉티브 속성은 `{ entity: <kind> }`로도 타입을 지정할 수 있습니다([엔진 id를 엔티티 종류로 타입 지정](#엔티티-종류로-형식이-지정된-엔진-id) 참고). 상태 경로는 `$name` 보간이 아니라 **구조화된 세그먼트**를 사용합니다. *디렉티브 속성*이나 `stampAttrs` 항목의 최상위 타입은 `list`, `map`, `record`일 수 없습니다(0.49부터). 속성 값은 스칼라 하나로 쓰므로 어떤 `.lute` 값도 이를 만족시킬 수 없고, 플러그인 로드는 속성 이름을 밝히며(스탬프는 `` stamp attribute `tags` ``) `E-PLUGIN-INVALID-DIRECTIVE`로 실패합니다. 호스트에서 파싱할 `string`으로 선언하거나 필드마다 속성 하나로 나누라고 안내합니다. 플러그인 `options:`, frontmatter 키, 상태 shape, 브리지 결과는 YAML로 쓸 수 있으므로 이 타입을 그대로 유지합니다.

- `directives/*.yaml` — `::name` 디렉티브 선언([브리지](/ko/plugins/bridge/) 참고)입니다.
- `state/*.yaml` — 재사용 가능한 타입 지정 레코드 shape(`stateShapes:`)와 구조화된 경로 템플릿(`stateTemplates:`)입니다. 한 파일에 둘 다 둘 수 있습니다(0.24.0부터, 이전에는 두 번째 항목이 버려졌습니다).
- `providers/*.yaml` — 고정된 스냅숏에 대해 해석되는 id 레지스트리입니다.
- `bridge/*.yaml` — 타입이 지정된 런타임 브리지 capability입니다.
- `defs/*.yaml` — 공유 타입 지정 CEL `@refs`입니다.
- `assetKinds`(`assetkinds/*.yaml`) — 순서가 지정된 `fallback` 훅과 함께하는 아티팩트 id 세그먼트 템플릿(조합/조회 모드)입니다.
- `stampAttrs`(`stampattrs/*.yaml`) — 모든 디렉티브와 콘텐츠 줄에 허용되는 횡단 속성입니다(아래 참고).
- `enums/*.yaml`, `frontmatter/*.yaml`, `docs/*.md` — 이름 붙은 enum 도메인, 플러그인이 소유하는 메타 키, 호버 문서입니다.
- `events/*.yaml` — 퀘스트의 `<on event>`가 이름 붙일 수 있고 `lute trace --event`가 발생시킬 수 있는 월드 이벤트입니다.
- `occasions/*.yaml` — [비트](/ko/language/beats/)가 응답하는 엔진의 순간입니다(dsl 0.21.0). 각 순간은 프로젝트 엔티티 종류에서 가져온 대상을 위해 발생할 수 있고(dsl 0.22.0), 하나의 승자·제공 목록·시퀀스로 제시되며(dsl 0.23.0), [`raisedWhen`](#계기-게이트-raisedwhen) 조건으로 선택적으로 게이트할 수 있습니다(dsl 0.27.0).
- `rewardKinds`(`rewardkinds/*.yaml`) — `<reward kind>` 값의 닫힌 집합입니다. 선택적인 [대상 계약](#보상-대상-계약), 추가 속성, 보상을 기록하는 상태 경로를 가질 수 있습니다(dsl 0.23.0).
- `cast/*.yaml` — 콘텐츠 줄에서 사용할 수 있는 화자와 표시 이름입니다(dsl 0.23.0).
- `lints/*.yaml` — 조언용 [lint 규칙](/ko/tooling/linting/)입니다. `<plugin-id>/<rule-id>`로 네임스페이스되며 capability 스냅숏에서는 제외됩니다.

이 목록 밖의 export 이름은 로드 오류입니다. 모든 export 파일도 엄격하게 읽힙니다(0.24.0부터): 선언이 모르는 키는 무시되지 않고 해당 줄에서 did-you-mean과 함께 `E-PLUGIN-KEY`가 됩니다(0.28.0부터는 `plugin.yaml` 자체도 같은 방식으로 읽습니다). 이는 디렉티브와 그 attrs, state, effects와 bridge, state shape와 template, provider, bridge capability, def, enum, event, frontmatter, asset kind, stamp attribute, reward kind, occasion, cast 항목, lint 모두에 적용됩니다. 예전에는 `{ selct: all }`이 아무 말 없이 `select: first`로 로드되었습니다:

<!-- lute-diagnostics unverified="the loader's E-PLUGIN-KEY line (crates/lute-manifest export_error); no single format! literal pins it; copied verbatim from lute check --project output" -->
```
./plugins/demo.pack/occasions/o.yaml:3:12: error [E-PLUGIN-KEY] `occasions.inbox` has no key `selct` — did you mean `select`? (its keys: select, target, description, judge, raisedWhen, payload, outsideRun)
```

이 문제에 이르는 흔한 방법은 flow map에서 따옴표 없는 description을 쓰는 것입니다. YAML은 쉼표에서 따옴표 없는 값을 끝내므로 `{ select: all, description: Pick one, the player picks one }`에서는 `the player picks one`이 값 없는 키로 남습니다. 오류가 그렇게 말하고 인용한 형태도 적어 줍니다:

<!-- lute-diagnostics unverified="serde's unknown-field text with the flow-map hint appended by crates/lute-manifest, wrapped in the loader's E-PLUGIN-PARSE line; copied verbatim from lute check --project output" -->
```
./plugins/demo.pack/occasions/o.yaml:3:48: error [E-PLUGIN-KEY] `occasions.inbox` has no key `the player picks one` (its keys: select, target, description, judge, raisedWhen, payload, outsideRun); `the player picks one` has no value — in a flow map `{ … }` an unquoted value ends at the first comma, so the rest became a key; quote the description: `description: "Pick one, the player picks one"`
```

`plugin.yaml`은 파싱되었지만 export 중 하나가 파싱되지 않은 플러그인은 전혀 로드되지 않습니다. 이를 활성화하는 프로필의 후속 오류는 플러그인이 **로드에 실패했다**고 말하고 파싱 오류를 가리킵니다. 0.24.0 이전에는 이미 설치된 플러그인을 설치하라고 했습니다:

<!-- lute-diagnostics unverified="assembled in crates/lute-manifest from the plugin id and a fixed suffix; copied verbatim from lute check --project output" -->
```
lute: E-PLUGIN-MISSING-ACTIVE: plugin `demo.pack` is activated by the project manifest but failed to load (see E-PLUGIN-KEY above); fix its package
```

최신 종류의 최소 파일 예시는 다음과 같습니다(각 파일은 최상위 키를 하나씩 가지며, id는 맵 키 또는 `name`/`id`입니다):

```yaml
# events/world.yaml
events:
  - { name: combatEnd }
  - { name: npcSpoke }
```

```yaml
# occasions/game.yaml — a bare {} is select: first, untargeted, judged after
occasions:
  townVisit: {}
  examine:   { select: first, target: true }
  talk:      { select: first, target: { prefix: npc, entity: person } }
  bossDefeated: { select: first, target: { prefix: boss, entity: foe, members: [gatekeeper, warden] } }
  inbox:     { select: all, description: Letters waiting at the fountain }
  evening:   { select: sequence }
  dayEnd:    { select: first, judge: before }
  enter:     { select: first, target: { prefix: room, entity: room }, raisedWhen: "holds('canEnter', [occasion.target])" }
  summon:    { select: first, target: { prefix: hero, entity: hero }, payload: { copies: int } }
  title:     { select: first, outsideRun: true }
```

occasion에는 다음 키만 사용할 수 있으며, 그 밖의 키는 사용할 수 없습니다: `select`, `target`, `description`, `judge`, `raisedWhen`, `payload`, `outsideRun`.

occasion의 `select:`는 발생했을 때 엔진이 무엇을 제시할지 말합니다. `first`(기본값)는 하나의 승리 비트와 그 뒤의 자격 있는 [`also`](/ko/language/beats/#also로-곁가지-발언) 비트를 제시합니다. `all`은 자격 있는 모든 비트를 제공하고 플레이어가 하나를 고릅니다. `sequence`(dsl 0.23.0)는 선택 순서대로 자격 있는 모든 비트를 제시합니다. 예를 들어 저녁 루틴 뒤에 그날의 사건을 제시하는 방식입니다([계기 조합하기](/ko/language/beats/#계기-구성하기) 참고). `all` 또는 `sequence` occasion에서 비트의 `also: true`는 `E-BEAT-ATTR`입니다. 같은 이름의 월드 이벤트도 `events:` 아래에 선언되어 있다면, occasion이 발생할 때마다 비트 뒤에 그 이벤트가 발생하고 occasion이 `on=` 목표를 판정하기 전에 실행됩니다([계기](/ko/language/beats/#계기) 참고).

occasion의 `judge:`(dsl 0.24.0 §2)는 `on=`으로 자신을 이름 붙인 퀘스트 목표를 발생 시점에 언제 판정할지 말합니다. 기본값 `after`는 비트가 제시된 뒤 판정하며, 이는 0.21.0부터의 순서입니다. `before`는 비트를 결정하기 전에 판정하고 퀘스트를 정착시키므로, `dayEnd`의 에필로그가 퀘스트가 어떻게 끝났는지 읽을 수 있습니다(`quest.<id>.failedBy`, [퀘스트와 씬](/ko/language/quests-and-scenes/) 참고). 옮겨지는 것은 판정뿐입니다. 발생에 응답하는 `<on>` 핸들러 본문, 즉 같은 이름의 `<on event>` 핸들러와 정착되는 퀘스트의 `questComplete` / `questFailed` 핸들러는 여전히 비트 뒤에 실행되므로 내레이션은 씬 뒤에 나옵니다. 그 밖의 값은 `E-PLUGIN-PARSE`입니다. `judge: before`가 없는 occasion은 기존 `capabilitySnapshot` 스탬프를 유지합니다.

occasion의 `target:`은 무엇을 위해 발생하는지 말합니다. 없거나 `false`이면 대상이 없습니다. `true`는 0.21.0의 의미를 유지합니다. 어떤 점으로 구분된 id를 위해 occasion이 발생하고, 비트의 대상은 모양만 검사됩니다. **도메인** `{ prefix, entity }`(dsl 0.22.0)도 집합을 닫습니다. 대상은 `<prefix>.<member>`이고, `entity`는 프로젝트가 스키마의 `entities:` 아래 선언한 엔티티 종류입니다. 플러그인은 접두사와 종류를 제공하고, 프로젝트는 멤버를 제공하거나 엔진이 채우는 멤버를 위해 종류를 `open:`으로 선언합니다. 도메인 밖의 비트 대상은 did-you-mean과 함께 `E-BEAT-ATTR`이며, occasion의 종류를 문서 스키마가 선언하지 않은 경우 모든 대상도 마찬가지입니다([비트](/ko/language/beats/#대상-도메인) 참고). `target:`이 bool도 아니고 `{ prefix, entity }` 맵(선택적인 `members:` 포함)도 아니면 플러그인 로드가 `E-PLUGIN-PARSE`로 실패합니다.

도메인의 선택적인 **`members:`** 목록은 종류의 하위 집합으로 범위를 좁힙니다. 위의 `bossDefeated`는 프로젝트의 `foe` 종류에 멤버가 더 있어도 `boss.gatekeeper`와 `boss.warden`에 대해서만 발생합니다. 종류의 멤버는 플러그인이 볼 수 없는 프로젝트 어휘이므로 체커가 목록을 판정합니다. 종류가 선언하지 않은 멤버를 목록에 적으면 해당 occasion의 모든 대상이 그 잘못된 멤버를 이름으로 지정하는 `E-BEAT-ATTR`이 됩니다. `open:` 종류에서는 목록을 적은 그대로 사용합니다. 하위 집합은 도메인의 모든 소비자에 똑같이 적용됩니다: 비트와 목표 대상, [`lute play`](/ko/tooling/play/) 단계의 `target:`, `lute new scene --target`, `lute calendar --target`입니다. 로더 자체는 occasion 파일에서 `E-PLUGIN-PARSE`로 다음 두 형태를 거부합니다:

- 빈 목록: ``occasion `bossDefeated`'s `target.members` is empty; list at least one member of entity kind `foe`, or drop `members` to draw targets from the whole kind``
- 멤버 중복: ``occasion `bossDefeated`'s `target.members` lists `warden` more than once``

occasion의 **`raisedWhen:`**(dsl 0.27.0 §4)는 엔진이 occasion을 아예 언제 발생시킬 수 있는지 말합니다. 위의 `enter`는 플레이어가 방에 들어갈 수 있을 때만 그 방에 대해 발생하는 순간입니다. 아래 [계기 게이트](#계기-게이트-raisedwhen)를 참고하세요.

occasion의 **`payload:`**는 매번 발생할 때 비트에 넘겨주는 타입 지정 값을 선언합니다. 이는 위의 매니페스트 타입 시스템에서 필드와 타입을 대응시키는 맵입니다(`copies: int`). 예를 들어 가챠 추첨은 뽑힌 영웅과 함께 온 복사본 수를 전달합니다. 비트는 이를 `occasion.payload.<field>`로 읽으며, 해당 occasion의 `<objective on=>`과 그 이름에 대응하는 `<on event=>` 핸들러도 마찬가지입니다([occasion payload](/ko/language/beats/#계기-페이로드) 참고). 모든 발생은 모든 필드를 제공합니다. 하나를 빠뜨린 `lute play` 단계는 그 필드를 이름 붙이는 사용 오류가 되며, `lute trace`와 `lute test`는 mock이 시드하지 않은 필드를 읽는 줄에서 불완전한 상태로 중단하고, payload가 있는 occasion을 이름 붙인 시계 `raise:`는 `E-CLOCK-DECL`입니다.

occasion의 **`outsideRun: true`**는 런 바깥에 존재하는 순간, 즉 타이틀 화면이나 런 사이의 갤러리를 표시합니다. 엔진은 프로젝트의 [`terminal:`](/ko/state/schemas/#게임-종료-terminal)이 성립한 뒤에도 이를 발생시키며, 체커는 `!terminal` 아래에서 그 비트를 판정하지 않습니다. 컴파일된 아티팩트는 이런 occasion을 `outsideRun` 아래에 나열합니다.

occasion은 capability 스냅숏의 일부이므로 `capabilitySnapshot`에 접힙니다. 도메인을 선언하면 스탬프가 다시 찍히며, `members:` 목록을 추가하거나 바꿔도 마찬가지입니다. `target: true` 또는 `false`만 말하는 occasion은 0.21.0에서 가졌던 스탬프를 유지합니다. 마찬가지로 `select: sequence`와 `raisedWhen`은 이를 선언하는 스냅숏의 스탬프만 바꿉니다.

```yaml
# rewardkinds/game.yaml
rewardKinds:
  XP: {}
  ITEM: { target: { provider: items }, attrs: [ { name: rarity, type: string } ] }   # `items` must be an active provider
  EMBERS: { credits: user.embers }
```

```yaml
# lints/style.yaml
lints:
  - id: too-many-choices
    target: scene
    when: "scene.choices > options.max"
    level: warn
    message: "scene has {scene.choices} choices (budget {options.max})"
    options: { max: 6 }
```

`enums/`는 프로젝트가 [콘텐츠 어휘](/ko/language/vocabulary/)를 얻는 세 번째 경로이며, 프로젝트 데이터가 아니라 capability인 유일한 경로입니다. `lute.core`는 어휘 슬롯을 선언하고 **비어 있는** `enums`를 export하므로, 엔진이나 장르 팩이 이를 활성화하는 모든 프로젝트에 멤버를 제공합니다. 항목은 작성자의 `enums:` 블록과 같은 긴 형태를 사용합니다. 단순 시퀀스는 `{ members: [...] }`의 축약형이며, `action`에는 `exits:`가, `anchor`에는 `default:`가 반드시 있어야 합니다.

### 계기 게이트: `raisedWhen`

occasion은 엔진이 이를 발생시키는 시점을 이름 붙이는 CEL 조건인 **`raisedWhen:`**(dsl 0.27.0 §4)을 선언할 수 있습니다. 엔진은 조건이 성립하는 동안에만 occasion을 발생시킵니다. 위의 `enter`는 플레이어가 방에 들어가는 순간이고, 프로젝트 스키마는 방과 게이트가 읽는 관계를 제공합니다:

```yaml
# world.schema.yaml
entities:
  room: { members: [lobby, office] }
relations:
  canEnter: { args: [room], tier: run }
```

게이트는 occasion이 발생하는 대상 멤버인 **`occasion.target`**을 읽을 수 있습니다. `room.office`를 위해 발생하면 `occasion.target`은 `office`이므로 게이트는 `holds('canEnter', ['office'])`를 묻습니다. 대상 없이 발생한 occasion에는 읽을 멤버가 없습니다. 대상 없는 occasion에서 `occasion.target`을 읽는 게이트는 `E-BEAT-ATTR`이며, 그 occasion에 응답하는 비트의 `on`에서 보고됩니다. occasion의 `target:`을 `{ prefix, entity }`로 선언하거나 읽기를 제거하세요.

게이트된 occasion에 응답하는 비트는 게이트가 성립하는 동안에만 제시될 수 있습니다. 따라서 체커는 해당 occasion에 응답하는 모든 비트와 [엔트리 비트](/ko/language/beats/#엔트리-비트)를 게이트와 함께 판정하며, 게이트가 `occasion.target`을 읽으면 멤버별로 판정합니다:

- 게이트의 텍스트는 모든 조건 슬롯처럼 검사됩니다. 오류는 비트의 `on`에서 ``occasion `enter`'s `raisedWhen: …`:`` 접두사와 함께 보고됩니다.
- 게이트가 명백히 거짓인 비트는 `E-BEAT-UNREACHABLE`(엔트리라면 `E-ENTRY-UNREACHABLE`)입니다. 엔진이 그 비트에 대해 occasion을 절대 발생시키지 않기 때문입니다. 게이트가 성립할 때마다 `when`이 명백히 거짓인 비트도 마찬가지입니다. 프로젝트 전체 `lute check`에서는 게이트의 `holds(…)`가 프로젝트의 [팩트 분석](/ko/state/facts-and-datalog/#프로젝트-전체-lute-check가-관계형-가드를-분석하는-방법)으로 결정됩니다. 프로젝트 어디에서도 `canEnter(office)`를 assert할 수 없다면 `room.office`의 모든 `enter` 비트는 절대 자격을 얻지 못합니다.
- 이런 방식으로 자격을 절대 얻지 못하는 비트는 `W-BEAT-PRIORITY-TIE`에 참여하지 않습니다.
- 게이트가 읽는 관계는 `W-RELATION-UNREAD`에서 읽힌 것으로 계산됩니다.

프로젝트의 [`terminal:`](/ko/state/schemas/#게임-종료-terminal) 조건도 게임이 끝나면 모든 occasion을 같은 방식으로 닫습니다.

[`lute beats`](/ko/tooling/cli/#beats)는 게이트된 사다리의 헤더에 게이트를 `` · raisedWhen: … ``로 표시하고, 한 번도 성립하지 않는 경우 헤더를 `` · raisedWhen: … · gate never holds ``로 끝냅니다. 종류 사다리라면 게이트가 성립하지 않는 멤버를 `` · raisedWhen: … · gate never holds for room.office ``처럼 이름 붙입니다. `--json`에서는 사다리에 `raisedWhen`과 `gateNeverHolds: true` 또는 `gateNeverHoldsFor: [<targets>]`가 포함됩니다.

[`lute play`](/ko/tooling/play/)는 엔진과 똑같이 occasion을 발생시킵니다. 게이트가 거짓일 때 이를 발생시키는 `occasion:` 단계는 `E-OCCASION-GATE`(종료 1)로 플레이스루를 중단합니다. `engine:` 쓰기 또는 이전 단계로 먼저 게이트를 성립시키거나 단계를 제거하세요. 시계가 만드는 발생(`raise.slot`, `dayStart`, `dayEnd`)은 게이트가 거짓이면 단순히 만들어지지 않으며, 단계에는 시계가 게이트를 넘겨 진행했다는 메모가 붙습니다.

컴파일된 아티팩트와 `project.index.json`은 게이트를 최상위에 `gates: [{ occasion, raisedWhen: { cel, expr, authored? } }]`로 담습니다. 각 조건은 `@def` 확장 뒤의 것이며, 어떤 occasion도 게이트를 선언하지 않으면 키를 생략합니다.

### 상태에 적립되는 보상

보상은 데이터입니다. 엔진이 부여하며 콘텐츠는 이를 읽지 않습니다. 보상 종류가 자체 상태로 추적하는 통화라면 `credits:`(dsl 0.23.0)가 부여량을 더할 상태 경로를 이름 붙입니다. 컴파일러는 그 종류의 모든 보상에 경로를 스탬프합니다(`RewardEntry.credits`가 IR에 있으며, 경로가 없는 종류에는 생략). 엔진은 부여 자체와 마찬가지로 이 쓰기도 소유합니다. [`lute run`](/ko/tooling/cli/#run)과 [`lute play`](/ko/tooling/play/)는 **스칼라** 금액에 대해 같은 일을 하고, 부여에 적립을 기록합니다:

```
grant climb EMBERS 100 (credits user.embers = 100)
```

범위 금액(`amount="10..20"`)은 엔진의 굴림이므로 도구체인은 무엇도 적립하지 않고 부여합니다. 손으로도 적립하면 두 번 지급됩니다. 같은 퀘스트의 `<on>` 핸들러나 목표 본문에서 적립된 경로를 콘텐츠 `::set`으로 쓰면 `W-REWARD-DOUBLE-CREDIT`입니다:

<!-- lute-diagnostics -->
```
./quests/climb.lute:11:11: warning [W-REWARD-DOUBLE-CREDIT] `::set` of `user.embers` in a handler of quest `climb`: its `<reward kind="EMBERS">` already credits `user.embers` when granted, so the player is paid twice — drop the `::set` or the reward
```

`credits:`가 없는 종류는 0.23.0 전과 정확히 같은 방식으로 해시되므로, 한 종류에 키를 추가해도 이를 선언하는 스냅숏만 다시 스탬프됩니다.

### 디렉티브 효과

디렉티브는 실행될 때 쓰는 상태를 선언할 수 있습니다: `effects.writes`, `{ scope, path, value }`의 목록입니다. [타입이 지정된 브리지 디렉티브](/ko/plugins/bridge/#이를-호출하는-디렉티브)는 결과 슬롯을 쓰는 브리지 디렉티브를 보여 줍니다. 패스스루 디렉티브도 쓸 수 있습니다. 예를 들어 플레이어의 정신력을 소모하는 공포 디렉티브입니다:

```yaml
# directives/ward.yaml
directives:
  - name: fright
    attrs:
      - { name: amount, type: int }
    effects:
      writes:
        - { scope: run, path: [sanity], value: { op: decrement, by: { fromAttr: amount } } }
```

`scope`와 `path`는 상태 경로를 이름 붙입니다. 여기서는 `run.sanity`입니다. [`owner: engine`](/ko/state/state-model/#owner-engine) 경로일 수도 있습니다. 콘텐츠는 이를 `::set`할 수 없지만(`E-ENGINE-OWNED-WRITE`), 디렉티브 효과는 엔진이 쓰는 것이며 [`lute play`](/ko/tooling/play/)는 디렉티브가 실행되는 곳에서 이를 적용합니다. `run.sanity`가 10일 때 `::fright{amount=2}`는 8을 남깁니다.

`value`는 네 가지 형태 중 하나입니다(dsl 0.27.0 §2):

| `value` | 쓰는 값 |
|---|---|
| bool, int, double 또는 string 리터럴 | 그 값 |
| `{ fromAttr: <attr> }` | 호출이 디렉티브 자체 속성 중 하나에 제공한 값, 또는 그 속성에 선언된 `default:` 값. 기본값이 없는 속성을 호출에서 생략하면 아무것도 쓰지 않습니다. |
| `{ fromBridgeResult: <field> }` | 브리지 응답의 필드([타입이 지정된 브리지 디렉티브](/ko/plugins/bridge/) 참고) |
| `{ op: increment \| decrement, by: <int> \| { fromAttr: <attr> } }` | `by`를 경로에 더하거나 뺍니다. `by:`에 이름 붙인 속성은 `type: int`여야 합니다. |

그 밖의 값은 `E-PLUGIN-PARSE`로 플러그인 로드를 실패시키며, 메시지에는 네 가지 형태가 나열됩니다. 디렉티브가 선언하지 않은 속성을 did-you-mean과 함께 `fromAttr`에서 이름 붙여도 마찬가지입니다.

#### 디렉티브가 assert하거나 retract하는 팩트

`writes:`와 더불어 디렉티브의 `effects:`는 **assert**하고 **retract**하는 팩트를 선언할 수 있습니다(dsl 0.27.0 §4). 각 목록은 선택적입니다. 플레이어가 아이템을 들고 다니는 인벤토리를 예로 들면, `give`는 가방에 아이템을 추가하고 `consume`은 그 아이템을 꺼냅니다:

```yaml
# directives/ward.yaml
directives:
  - name: give
    attrs:
      - { name: item, required: true, type: { entity: item } }
    effects:
      asserts: ["holding(@item)"]
  - name: consume
    attrs:
      - { name: item, required: true, type: { entity: item } }
    effects:
      retracts: ["holding(@item)"]
```

각 항목은 하나의 팩트 패턴 `relation(arg, …)`입니다. 인수는 멤버, `true` 또는 `false`, `@attr` 중 하나입니다. `@attr`는 호출이 디렉티브 자체 속성에 제공한 값 또는 그 속성에 선언된 `default:` 값입니다. `retracts:`에서는 어떤 값에도 일치하는 `_`도 인수가 될 수 있습니다. 단일 슬롯에만 사용하세요. `asserts: ["holding(@item)"]` 옆에 `retracts: ["holding(_)"]`를 두면 `give`가 집어 들기 전에 손에 들던 것을 내려놓는 양손이 되므로, 가방에서는 매번 줍는 순간 인벤토리를 비우게 됩니다. 파싱되지 않는 패턴, 디렉티브가 선언하지 않은 `@attr`(did-you-mean 포함), `asserts:` 안의 `_`는 모두 `E-PLUGIN-PARSE`로 플러그인 로드를 실패시킵니다. 호출이 값을 주지 않았고 기본값도 없는 `@attr`가 남으면 해당 팩트에는 아무것도 쓰지 않습니다.

관계와 인수는 프로젝트의 것이므로 체커는 각 호출에서 `::assert` 또는 같은 팩트의 `::retract`를 판정하듯 이를 판정합니다. 관계가 선언되어 있어야 하고, arity와 모든 인수의 종류가 맞아야 합니다. `holding(item)` 관계에 대해 `::give{item="brassKey"}`는 `holding(brassKey)`를 assert합니다. 오류는 호출과 팩트를 앞에 붙여 다음처럼 표시됩니다: `` `::give` asserts `holding(brassKey)` (its declared `effects.asserts`): ``. derived 또는 `app` 계층 관계는 `::assert`와 마찬가지로 거부되지만, `reserved: true` 관계는 이런 방식으로 쓸 수 있습니다. 엔진이 디렉티브의 효과를 직접 적용하므로 엔진 자신의 쓰기이기 때문입니다.

컴파일된 `plugin` 레코드는 `effects`와 함께 해석된 팩트를 `retracts`와 `asserts` 목록으로 담으며, 각 목록은 `@attr`가 이미 치환된 `{ relation, args }`입니다(와일드카드는 `"_"`). 비어 있으면 둘 다 생략되므로 팩트 효과가 없는 아티팩트는 바뀌지 않습니다. [`lute play`](/ko/tooling/play/), `lute test`, `lute trace`, `lute run`은 호출의 `writes` 뒤에 이를 적용하며, 먼저 retract하고 그다음 assert합니다. `::assert` / `::retract`와 같은 경로를 사용합니다. Play는 트랜스크립트 줄에서 호출을 이름 붙입니다:

```
assert holding(brassKey)  (effect of ::give)
```

콘텐츠가 assert하는 것을 읽는 모든 분석은 이 호출을 해당 팩트의 `::assert`와 `::retract`로 읽습니다. 프로젝트의 [팩트 분석](/ko/state/facts-and-datalog/#프로젝트-전체-lute-check가-관계형-가드를-분석하는-방법)(무엇이 성립할 수 있고 반드시 성립해야 하는지), `W-RELATION-UNREAD`, `E-FACT-EXCLUSIVE`(디렉티브에 빠진 `retracts:`를 선언하라고 제안), 캐스트 존재, `lute scenario --facts`, [`lute scenario knowledge`](/ko/tooling/overviews/#lute-scenario-knowledge), `lute lore`가 모두 여기에 해당합니다. 따라서 어떤 `::give` 호출이 `holding`을 assert할 수 있다면 `holds('holding', ['brassKey'])` 가드는 어디에도 `::assert`를 작성하지 않아도 만족할 수 있습니다.
### 엔티티 종류로 형식이 지정된 엔진 id

가방 아이템, 종족 또는 상점의 재고 목록처럼 엔진이 소유하는 무언가를 이름으로 지정하는 속성은 프로젝트가 선언한 **엔티티 종류**로 형식을 지정할 수 있습니다(dsl 0.26.0 §2.5).

```yaml
# directives/engine.yaml
directives:
  - name: give
    attrs:
      - { name: item, required: true, type: { entity: bagItem } }
      - { name: qty,  type: int, default: 1 }
```

플러그인은 종류의 이름을 지정하고 프로젝트는 그 멤버를 나열합니다. 계기의 [대상 도메인](/ko/language/beats/#대상-도메인)과 동일한 분할입니다. 종류는 모든 문서가 import하는 스키마에 있으며, 일반적으로 엔진이 제공하는 id가 들어 있는 lead 소유 파일 하나입니다.

```yaml
# schema/items.schema.yaml
entities:
  bagItem: { members: [potion, superPotion, goodRod, nugget] }
```

종류에 속하지 않는 값은 `E-BAD-ENUM`과 did-you-mean을 받습니다. 문서의 어떤 스키마에도 선언되지 않은 종류로 형식이 지정된 속성은 모든 사용 위치에서 `E-DOMAIN-UNKNOWN`입니다.

<!-- lute-diagnostics unverified="verbatim lute check --project output; the E-DOMAIN-UNKNOWN message spells the type with escaped braces, which the scraper does not read back as a literal" -->
```
./scenes/mart.lute:9:14: error [E-BAD-ENUM] `potoin` is not a member of entity kind `bagItem` (attribute `item` of `::give`) — did you mean `potion`?
./scenes/mart.lute:10:22: error [E-DOMAIN-UNKNOWN] attribute `species` of `::encounter` is typed `{ entity: monster }`, but `monster` is not a declared entity kind — declare it under `entities:` in a project schema this document reaches through `uses:`
```

컴포넌트 매개변수를 통해 속성에 도달하는 값은 `::use` 인자에서 또는 매개변수의 `default:`에서 검사합니다([매개변수를 통한 엔진 id](/ko/language/components-and-extends/#매개변수를-통한-엔진-id) 참조). `lute refs --project <DIR> --attr give.item`은 해당 값을 사용하는 문서와 줄을 함께 모든 값으로 나열하므로, 리드는 여러 작성자의 작업을 병합하기 전에 누가 무엇을 제공하는지 확인할 수 있습니다.

### 보상 대상 계약

보상 종류의 `target:`은 `<reward target="…">` 값이 무엇을 이름으로 지정하는지 선언하며, dsl 0.26.0 §2.5부터 체커는 보상에 이를 적용합니다.

```yaml
# rewardkinds/league.yaml
rewardKinds:
  MONEY: { credits: run.money }
  ITEM:  { target: { entity: bagItem, required: true } }
  TM:    { target: { entity: tm, required: true } }
```

| 계약 키 | 의미 |
|---|---|
| `entity: <kind>` | 대상이 해당 프로젝트 엔티티 종류의 멤버입니다. 멤버가 아니면 did-you-mean이 포함된 `E-REWARD-TARGET`입니다. |
| `provider: <name>` | 대상이 해당 프로바이더 카탈로그의 id입니다. 프로바이더가 활성 상태여야 하며(그렇지 않으면 플러그인 로드가 실패합니다), 알 수 없는 id는 `E-REWARD-TARGET`이고 오래된 스냅숏은 경고만 냅니다(`W-CATALOG-STALE`). |
| `required: true` | 해당 종류의 보상에 `target=`이 없으면 `E-REWARD-TARGET`입니다. 이 키가 없으면 대상을 생략할 수 있습니다. |
| `forbidden: true` | 해당 종류는 대상을 받지 않습니다. 이 종류의 `<reward>`에 `target=`이 있으면 `target=` 속성 위치에서 `E-REWARD-TARGET`입니다(0.49부터). |

계약은 `entity:` 또는 `provider:` 중 하나를 지정하며 둘 다 지정하지는 않습니다. 둘 다 지정하면 플러그인 로드가 실패합니다(`E-PLUGIN-PARSE`). `target:`이 없는 종류는 이전과 같이 어떤 `target=`이든 받을 수 있고 생략할 수도 있습니다. 대상을 받지 않는 종류는 `target: { forbidden: true }`로 이를 선언하며, 이 키는 **단독**으로만 쓸 수 있습니다. `provider`, `entity`, `required`와 함께 쓰면 플러그인 로드가 실패합니다. 대상을 받지 않는 종류가 id 공간을 이름으로 지정하거나 대상을 요구할 수는 없기 때문입니다. `COINS: { target: { forbidden: true } }`라면 ``<reward kind="COINS" target="x"/>``는 `E-REWARD-TARGET`입니다: `` <reward kind="COINS"> takes no `target=`: its reward kind declares `target: { forbidden: true }` — remove the attribute``. `forbidden: true`가 없는 선언은 `capabilitySnapshot`을 그대로 유지하고, 이를 설정하면 스냅숏이 바뀝니다. 위의 ITEM 계약은 수동으로 관리하는 목록이 놓치는 두 실수를 모두 잡아냅니다.

<!-- lute-diagnostics -->
```
./quests/dex.lute:11:3: error [E-REWARD-TARGET] `<reward kind="ITEM">` needs a `target=`: the reward kind declares `target: { required: true }`
./quests/dex.lute:12:31: error [E-REWARD-TARGET] `goodRd` is not a member of entity kind `bagItem`, which `<reward kind="ITEM">` targets — did you mean `goodRod`?
```

작성한 값이 따옴표로 묶인 `target="…"`일 때만 아티팩트에 실립니다. 보상의 대상은 엔진이 지급 시점에 쓰는 정적 데이터이기 때문입니다. 따라서 `@ref`(`target=@n`)나 값 없는 `target`은 0.49부터 계약이 무엇이든, 계약이 없는 종류에서도 값 위치에서 `E-REWARD-ATTR`입니다: ``<reward target=…> takes a quoted id, e.g. `target="potion"`: `target=@n` is a reference, and a reward's target is static data the engine grants against``. 값 없는 `target`은 대신 `a bare `target` has no value`라고 알려 줍니다. `required: true`는 작성한 대상을 누락으로 한 번 더 보고하지 않고, `entity:`나 `provider:` 계약도 더하는 것이 없습니다. 이 거부가 유일한 진단입니다. `forbidden: true`에서는 `E-REWARD-TARGET`만 보고됩니다.

엔진이 소유한 id에는 어떤 형식을 사용해야 할까요? id를 저장소에 나열하고 그곳에서 검토할 수 있다면 **엔티티 종류**를 사용합니다(위의 `schema/items.schema.yaml` 같은 리드 소유 스키마 하나). 엔진의 카탈로그가 기준 정보이고 프로젝트가 고정된 스냅숏으로 이를 검사한다면 **프로바이더**를 사용합니다([프로바이더와 카탈로그](/ko/tooling/providers-and-catalog/) 참조). 어느 쪽이든 디렉티브 속성과 보상 종류를 **동일한** 종류 또는 프로바이더로 형식 지정해야 합니다. 그러면 `::give{item}`과 `<reward kind="ITEM" target>`이 하나의 id 공간을 공유하고, `lute refs --project <DIR> --attr give.item --reward ITEM`이 두 항목을 모두 나열합니다(대상이 없는 보상은 `(no target)`으로 표시).

### 캐스트

```yaml
# cast/harbor.yaml
cast:
  mira:  { name: Mira }
  oskar: { name: "Oskar Lind", present: "run.oskarAboard", emotions: [calm, grim] }
  vesna: {}
```

`cast` export(dsl 0.23.0)는 엔진 팩이 대상으로 빌드되는 화자를 선언합니다. 각 맵 키는 화자 id이고 모든 필드는 선택 사항입니다. `name`은 표시 이름입니다. 0.24.0부터 엔트리는 캐릭터가 플레이어와 함께 있는 조건인 `present:`와 해당 캐릭터의 대사가 사용할 수 있는 `emotions:` 값인 `emotion=`도 선언할 수 있으며, `assume: true`를 지정하면 존재 여부 검사에서 엔진의 `reserved:` 팩트 중 `present:`가 부정하는 것을 부재로 간주할 수 있습니다. 가드가 `present:`를 함의하지 않는 대사는 `W-CAST-ABSENT`이고, `emotion=` 값이 `emotions:` 밖에 있으면 `E-BAD-ENUM`입니다([캐스트](/ko/language/dialogue-and-cast/#캐스트) 참조). 그 밖의 키는 `E-PLUGIN-PARSE`입니다. 플러그인 또는 스키마 문서의 `cast:` 키로 캐스트가 하나라도 선언되면, 캐스트 밖의 화자를 사용하는 콘텐츠 라인은 did-you-mean이 포함된 `E-CAST-UNKNOWN`을 받습니다. 캐스트 밖의 대상을 지정하는 `::actor{character}` 또는 `::camera{focus}`도 마찬가지입니다. 문서가 import하는 플러그인 캐스트와 스키마 캐스트는 합쳐지며, 공유하는 id에 대해서는 엔진의 표시 이름을 담은 플러그인 엔트리가 우선합니다. 동일한 id를 선언하는 활성 플러그인 두 개는 assembly에서 `E-PLUGIN-DUP-ACROSS`입니다. 비어 있지 않은 캐스트는 `capabilitySnapshot`에 반영되고, export하는 캐스트가 없는 플러그인은 스탬프를 그대로 둡니다. 0.24.0 키를 전혀 사용하지 않는 캐스트는 0.23에서 가지고 있던 스탬프를 유지합니다.

dsl 0.26.0 §2.8부터 엔트리는 `sharedName: true`도 지정할 수 있습니다. 이때 `name`은 여러 화자가 의도적으로 공유하는 역할 이름입니다. 예를 들어 악당 팀의 일반 병사 계급을 들 수 있습니다(`grunt1: { name: Eclipse Grunt, sharedName: true }`). 프로젝트 전체 `lute check`와 `lute lint`는 정확히 동일한 이름으로 표시되는 두 화자를 권고 수준의 `W-DISPLAY-NAME-DUP`으로 보고하며, `sharedName` 엔트리는 그 수에 포함하지 않습니다([두 화자가 공유하는 표시 이름](/ko/language/dialogue-and-cast/#두-화자가-공유하는-표시-이름) 참조).

## 교차 속성(`stampAttrs`)

일반 속성은 디렉티브별로 선언합니다. 모든 레코드에 동일한 키(분석 id, 실험 버킷, 보너스 훅 등)를 태그하는 엔진에는 이를 선언할 위치가 없었고, 콘텐츠 라인에도 전혀 넣을 수 없었습니다. `stampattrs/*.yaml`이 그 위치입니다.

```yaml
stampAttrs:
  - { name: bonusId,    type: string }
  - { name: bonusScore, type: int }
```

엔트리는 일반적인 `AttrDecl`입니다. 즉, 디렉티브 속성이 사용하는 것과 동일한 `{ name, required?, type, default? }` 형태이지만, 모든 디렉티브와 콘텐츠 라인(`@speaker{…}: text`)에서 해당 표면 자체의 속성에 더해 사용할 수 있습니다. 해석은 엄격합니다. 표면 자체의 선언이 먼저 적용되고, 그다음 `stampAttrs`, 그다음 `E-UNKNOWN-ATTR`이 적용됩니다. 값 형식 지정은 기존 속성 경로를 따르므로 잘못 입력하면 새 규칙이 아니라 일반적인 `E-ATTR-TYPE` / `E-BAD-ENUM`이 됩니다.

`::sfx{sound="chime" bonusId="b-02"}` 다음에 `@marina{code="0010" bonusId="b-01" bonusScore="7"}: Welcome back.`이 오면, 각각의 레코드가 핵심 키(`kind`, `family`, `position`, `timing`) 뒤의 **중첩된 `stamps` 객체 하나**(0.49부터)에 속성을 담은 두 레코드로 컴파일됩니다. 레코드 자체의 키 옆에 평탄화되지도, 레코드의 `fields` 안에 들어가지도 않습니다.

```json
{ "kind": "sfx", "family": "staging", "position": "001-0100", "sound": "chime", "stamps": { "bonusId": "b-02" } }
{ "kind": "line", "family": "content", "position": "001-0200", "role": "dialogue",
  "speaker": "marina", "text": "Welcome back.", "lineId": "marina.s01ep01.marina_0010",
  "voiceKey": "marina.s01ep01.marina-0010", "stamps": { "bonusId": "b-01", "bonusScore": 7 } }
```

각 항목의 JSON 값은 이전처럼 선언한 플러그인의 속성 선언으로 타입이 지정됩니다. 작성된 스탬프가 없으면 객체는 생략되므로 스탬프가 없는 레코드는 바뀌지 않습니다. 스탬프가 자기 키 아래에 있으므로, 운반하는 레코드의 키(`tag`, `speaker`, `text`, `label`, `name`, `id` 등)와 같은 이름의 플러그인 스탬프도 더 이상 충돌하지 않습니다. 0.49 이전에는 그런 스탬프가 같은 키를 두 번 썼습니다. 같은 `stamps` 객체는 플러그인 이벤트 레코드와 [브리지](/ko/plugins/bridge/) 호출의 `attrs` 옆에, `attrs`에 병합되지 않은 채 호스트에 전달됩니다.

**작성되지 않은** 스탬프 속성은 선언에 `default`가 있어도 주입되지 않습니다. 없다는 것은 없는 것이므로, 교차 어휘를 선언하고 그중 아무것도 작성하지 않으면 아티팩트는 바이트 단위로 동일합니다. 다만 선언이 무료인 것은 아닙니다. `stampAttrs`는 `capabilitySnapshot`에 참여합니다. 교차 어휘가 변경되면 capability 표면이 변경된 것이고 엔진이 불일치를 거부할 수 있어야 하기 때문입니다.

### 예약된 스탬프 키

코어 스탬프는 `at`, `duration`, `delay`, `wait`, `timeline`, `timing`, `provenance`, `source`라는 8개의 이름을 소유합니다. `at`, `duration`, `delay`, `wait`, `timeline`은 `timing`으로 컴파일되는 작성용 키이고, `timing`, `provenance`, `source`는 평탄화된 레코드 키로 남습니다. 중첩된 `stamps` 객체는 그 외의 모든 이름에서 *JSON* 충돌을 없애지만, 이 여덟 이름은 계속 예약됩니다. 같은 이름의 플러그인 스탬프는 작성자에게 여전히 모호하기 때문입니다. 플러그인이 그중 하나로 속성을 선언하면 assembly에서 **`E-PLUGIN-RESERVED-STAMP-ATTR`**과 함께 거부됩니다.

<!-- lute-diagnostics -->
```
$ lute check scene.lute --project .
lute: E-PLUGIN-RESERVED-STAMP-ATTR: plugin `arcia.bonus` declares reserved stamp attribute `duration`; `at`/`duration`/`delay`/`wait`/`timeline`/`timing`/`provenance`/`source` are owned by the core stamp (plugin §14)
```

두 표면 모두 적용됩니다. `stampAttrs` export와 일반적인 디렉티브별 `attrs` 엔트리가 모두 해당하므로, 플러그인은 어느 경로로도 예약 키에 접근할 수 없습니다. 문제가 있는 선언은 병합하지 않고 삭제되며, 예약되지 않은 형제 항목은 그대로 적용됩니다. 이것이 차단 플러그인 디렉티브가 자체 플래그를 `sync`로 지정하고 `wait`로 지정하지 않는 이유입니다([브리지](/ko/plugins/bridge/) 참조).

## 선언적 낮춤

디렉티브의 `lower:`는 컴파일러가 해당 디렉티브에 대해 무엇을 내보내는지 지정합니다. 선택 사항이며, 없으면 디렉티브가 일반 `kind: "plugin"` 패스스루 레코드로 컴파일됩니다([패스스루 소유권과 디스패치](#패스스루-소유권과-디스패치) 참조). 0.24.0 이전에는 `lower:`가 없으면 패스스루가 기본값으로 문서화되어 있었더라도 `E-PLUGIN-PARSE missing field lower`였습니다. 지정할 경우 다음 두 형식 중 하나입니다.

```yaml
lower: { record: <kind>, fields: { … } }   # a finite attrs → one core record
lower: { kind: builtin, name: <hook> }     # a named core hook
```

`builtin` 이름은 코어가 자체 디렉티브에 등록한 훅 중 하나여야 합니다. `actorStage`, `cameraTransform`, `clearStage`, `end`, `jump`, `label`, `sequence`가 해당합니다. 다른 이름이면 플러그인 로드 시 did-you-mean과 함께 `E-PLUGIN-PARSE`가 발생하며, 패스스루를 사용하려면 `lower:`를 생략하라는 안내가 나옵니다. 0.24.0 이전에는 등록되지 않은 이름이 조용한 패스스루로 낮춰졌고, 배포된 브리지 예제는 존재하지 않는 훅(`bridgeMinigame`, `bridgeServe`, …)을 지정했습니다. 브리지 디렉티브에는 `lower:`가 필요 없습니다. `bridge: { service, operation }`이 호출을 바인딩합니다([브리지](/ko/plugins/bridge/) 참조).

<!-- lute-diagnostics unverified="built in crates/lute-manifest/src/schema.rs from several format! pieces and wrapped in the loader's E-PLUGIN-PARSE line; copied verbatim from lute check --project output" -->
```
./plugins/demo.pack/directives/d.yaml:6:5: error [E-PLUGIN-PARSE] directives[1]: `clearStag` is not a builtin lowering hook (did you mean `clearStage`?); the core registers actorStage, cameraTransform, clearStage, end, jump, label, sequence; omit `lower:` for the generic `kind: "plugin"` passthrough
```

`record` 형식은 **control-flow가 아닌 9개의 스테이징 종류** — `bg`, `music`, `sfx`, `vfx`, `actor`, `camera`, `cg`, `video`, `sequence` — 중 하나를 대상으로 하며, 각 대상 필드를 `fromAttr` 참조 또는 리터럴에 바인딩합니다.

```yaml
directives:
  - name: backdrop
    attrs:
      - { name: img,  required: true, type: string }
      - { name: time, type: string }
    lower:
      record: bg
      fields:
        assetId: { fromAttr: img }
        time:    { fromAttr: time }
```

그러면 `::backdrop{img="bg.lounge" time="night"}`은 실제 `bg` 레코드로 컴파일됩니다. `kind: "plugin"` 패스스루가 아닙니다.

```json
{ "kind": "bg", "family": "staging", "position": "001-0300", "time": "night", "assetId": "bg.lounge", "timing": { "wait": true } }
```

내보낸 레코드는 **대상 종류**의 `wait` 기본값을 상속합니다(`bg`, `video`, `sequence`는 대기시키고, `cg`, `camera`는 대기시키지 않으며, 나머지는 키를 생략합니다). 따라서 코어 레코드와 같은 종류와 해석된 필드를 가집니다. 작성되지 않은 선택적 source 속성은 대상 필드를 없는 상태로 둡니다.

**스테이징 의미는 낮춰진 레코드 종류를 따릅니다(0.45).** `bg`로 낮춰진 플러그인 디렉티브는 `::bg`처럼 장면을 바꿉니다. 레코드 앞에서 무대의 캐릭터를 자동으로 숨기고 이탈을 기록하며 대기 중인 포즈 초기화를 버리고 새 배경을 기록합니다. `actor`로 낮춰진 디렉티브는 해석된 `character`를 `::actor`처럼 무대에 올리며, `exit: true` 필드는 그 캐릭터를 무대에서 내립니다. `character` 필드는 필수 `fromAttr` 속성(`required: true`) 또는 문자열 리터럴에 바인딩해야 하며, 그렇지 않으면 assembly에서 `E-LOWER-RECORD-FIELD`가 발생합니다. 바인딩된 `posReset`과 `preload` 필드는 무대 모델에 영향을 주지 않습니다. `music`은 음악 bookkeeping만 갱신하며, 다른 낮춤 레코드 종류는 무대에 영향을 주지 않습니다.

이 9개가 전체 어휘이며, 제외는 임의의 목록이 아니라 원칙에 따른 것입니다. control-flow 종류(`jump`, `choice`, `match`, `hub`, `barrier`, `end`, `quest`, `on`)는 컴파일러 자체 패스가 해석하는 position을 전달하고, 콘텐츠 종류(`line`)는 identity인 `lineId` / `voiceKey`를 작성된 `code`에서 파생해 전달합니다. 어느 쪽도 유한한 attrs→fields 매핑이 아니므로 data가 아닙니다.

이를 대신하는 코어 스테이징 디렉티브와 마찬가지로 `lower:` 레코드가 있는 디렉티브는 [`when=` 가드](/ko/language/directives/#디렉티브-가드하기-when)를 거부합니다(`E-UNKNOWN-ATTR`). 패스스루 또는 브리지 디렉티브는 이를 받습니다. dsl 0.26.0 §4부터 `when`은 모든 디렉티브에 적용되는 가드이므로, 더는 해당 이름의 플러그인 속성을 작성할 수 없습니다. 이를 사용하는 경우 `E-UNKNOWN-ATTR`이 발생하며 플러그인에서 속성 이름을 바꾸라는 안내가 나옵니다.

두 오류는 모두 **조립에서**, 무엇이든 낮춰지기 전에 잡힙니다. 검증에 실패한 선언은 컴파일러에 도달하지 않습니다.

- **`E-LOWER-RECORD-UNKNOWN`** — `record:`가 9개 외의 항목을 지정합니다.
- **`E-LOWER-RECORD-FIELD`** — 대상 필드가 해당 레코드 종류에 없거나, `fromAttr`가 디렉티브에 선언되지 않은 속성을 지정합니다.

<!-- lute-diagnostics -->
```
$ lute check scene.lute --project .
lute: E-LOWER-RECORD-FIELD: directive `::backdrop` lowers to record `bg`: unknown target field `mood` (record `bg` binds: location, time, assetId)
lute: E-LOWER-RECORD-UNKNOWN: directive `::sting` lowers to unknown record `line`; declarative lowering targets the staging kinds (bg, music, sfx, vfx, actor, camera, cg, video, sequence)
```

### 패스스루 소유권과 디스패치

디렉티브에 `lower:`가 선언되지 않으면 컴파일러는 일반 패스스루 레코드를 내보냅니다.

```json
{
  "kind": "plugin",
  "family": "plugin",
  "position": "001-0400",
  "tag": "host-panel",
  "plugin": "game.presentation",
  "fields": {
    "assetId": "CG.chapter_one.first_choice",
    "resultKey": "first-choice",
    "text": "First choice"
  }
}
```

선택적 `plugin` 필드는 소유 플러그인 패키지 id를 해석한 결과입니다. 이는 작성된
속성이 아니라 assembly 메타데이터이므로 소스에서 재정의할 수 없습니다. 호스트는
**`(plugin, tag)`**를 기준으로 확장 작업을 라우팅합니다. 코어 또는 해석되지 않은 패스스루
레코드는 이를 생략합니다. 플러그인 id는 동적인 `kind` 값이 되지 않습니다.

이는 선언적 프레젠테이션 낮춤을 변경하지 않습니다. `lower: { record: bg, fields: ... }`가
있는 디렉티브는 안정적인 코어 `bg` 레코드(허용된 다른 스테이징 종류도 동일)를 내보내며,
`kind: "plugin"` 레코드를 내보내지 않습니다. 소유자 메타데이터는
일반 패스스루 플러그인 레코드에만 적용됩니다.

## 설치 및 프로필 그래프

프로젝트의 `lute.project.yaml`은 `pluginsDir`, `defaultProfile`, 프로필 그래프를 선언합니다. 프로필은 최상위 capability 선택자입니다. 예약된 `global` 프로필은 다른 모든 프로필에 상속되며, 프로필은 `extends`를 통해 조합됩니다:

```yaml
pluginsDir: plugins/
defaultProfile: date-minigame
profiles:
  global:       { plugins: { lute.core: true } }
  story:        { plugins: { arcia.minigame: true } }
  date:         { extends: story }
  date-minigame:
    extends: date
    plugins:
      arcia.minigame: { resultScope: scene, allowedKinds: [rhythm, timing] }
```

`plugins`는 플러그인 id에서 타입이 지정된 옵션 객체로 매핑되는 **맵**입니다(또는 `true`로 지정하면 기본값으로 정규화됩니다). 유효한 키가 있으면 플러그인이 **활성화**됩니다 — `plugins.use` 목록은 없습니다. 선택 및 병합 규칙은 [프로필](/ko/plugins/profiles/)을 참조하세요.

## 해석 및 capability 스냅숏

동일하게 설치된 플러그인, 선택한 프로필, 씬 프런트매터가 주어지면 해석 결과는 **바이트 단위로 동일한** capability 스냅숏을 생성합니다. 해석에는 다음 순서를 정확히 적용합니다: `lute.core` → `profiles.global` → 선택한 프로필의 `extends` 체인(부모 먼저) → 선택한 프로필 → 씬 로컬 `plugins:` → 의존성 폐쇄. 스칼라 옵션은 재정의하고, 맵은 깊게 병합하며, 목록은 교체합니다.

스냅숏은 `plugins`, `enums`, `providers`, `stateShapes`, `stateTemplates`, `assetKinds`, `directives`, `bridgeCapabilities`, `frontmatter`, `events`, `stampAttrs`, `diagnostics` 등을 담은 하나의 불변 아티팩트입니다. 스냅숏의 `capabilitySnapshot`은 해석된 전체 표면에 대한 콘텐츠 해시입니다 — 플러그인 id+버전과 병합된 옵션 객체, 그리고 그 안의 모든 디렉티브, 열거형, 프로바이더, 상태 셰이프, 브리지 capability, def, 프런트매터 키, 스탬프 속성을 포괄합니다. 값이 채워진 필드가 조금이라도 달라지면 다른 해시가 생성되며, `lute.core`에 디렉티브를 추가하면 모든 프로젝트에서 해시가 바뀝니다. 생성되는 모든 아티팩트에는 해당 아티팩트가 대상으로 하는 `capabilitySnapshot`이 스탬프되고, 소비자는 일치하지 않는 스탬프를 거부합니다. 프로바이더는 **스냅숏 우선**입니다. 필요한 카탈로그 데이터가 없으면 컴파일러는 실패하지만 네트워크에서 절대로 대기하지 않으며, LSP는 잘못된 *unknown-id* 오류 대신 *catalog-stale* 진단과 함께 오래된 스냅숏을 유지합니다.
