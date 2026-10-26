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
  hostTokens: hosttokens/
  lints: lints/
  docs: docs/
options:                   # OPTIONAL — typed activation options
  - { name: resultScope,  type: { enum: [scene, run] }, default: scene }
  - { name: allowedKinds, type: { list: { enum: [rhythm, puzzle, timing] } }, default: [rhythm, puzzle, timing] }
```

`depends[].range`는 두 형태로만 고정됩니다 — 캐럿(`^x.y.z`, 1.0 이전 의미론) 또는 세 요소로 된 정확한 버전입니다. 그 밖의 표기는 정의상 만족시킬 수 없습니다.

옵션의 값은 활성 프로필이 준 값이고, 없으면 선언된 `default:`입니다. `{ enumFromOption: <option> }` 타입의 속성은 이 실제 값으로 검사합니다(0.41.1). 위 매니페스트와 플러그인을 켜기만 하는 프로필(`plugins: { arcia.minigame: true }`)에서는 `{ enumFromOption: allowedKinds }` 타입 속성의 `kind="racing"`이 `rhythm, puzzle, timing`에 대해 `E-BAD-ENUM`이며, 생략된 속성의 선언된 `default:`가 목록 밖이어도 마찬가지입니다. 프로필이 `allowedKinds: [rhythm]`을 주면 그 목록으로 좁혀집니다. 프로필 값도 `default:`도 없는 옵션이면 속성을 검사하지 않습니다.

## Export 파일

각 export 종류에는 규범 스키마가 있습니다. 모두 하나의 작은 매니페스트 타입 시스템(`bool` / `int` / `double` / `string`, `enum`, `list`, `record`, `map`, 그리고 `enumFromOption`, `providerRef`, `slotId`, `assetKind`, `domain`, shape 참조)으로 타입이 지정됩니다. dsl 0.26.0부터 디렉티브 속성은 `{ entity: <kind> }`로도 타입을 지정할 수 있고([엔진 id를 엔티티 종류로 타입 지정](#엔티티-종류로-형식이-지정된-엔진-id) 참고), 0.51부터는 `text`로도 타입을 지정할 수 있습니다([아래](#현지화-가능한-텍스트-속성-type-text) 참고). 상태 경로는 `$name` 보간이 아니라 **구조화된 세그먼트**를 사용합니다. *디렉티브 속성*이나 `stampAttrs` 항목의 최상위 타입은 `list`, `map`, `record`일 수 없습니다(0.49부터). 속성 값은 스칼라 하나로 쓰므로 어떤 `.lute` 값도 이를 만족시킬 수 없고, 플러그인 로드는 속성 이름을 밝히며(스탬프는 `` stamp attribute `tags` ``) `E-PLUGIN-INVALID-DIRECTIVE`로 실패합니다. 0.51부터 메시지는 다음 힌트로 끝납니다: `` … so no `.lute` value can satisfy a `list`, `map` or `record` (structured attribute values are not supported yet): where it fits, give each field its own attribute (a stamp can be scoped to its surfaces with `on:`), use a `text` attribute for player-visible text in several languages, or write a list of options as `<choice>` elements ``. 스탬프는 `text`가 될 수 없으므로 스탬프의 힌트에서는 `text` 절이 빠집니다. 구조화된 작성 자체(`::tagged{tags=…}` 같은 enum 목록 메타데이터, 언어별 음성 자산 맵)는 보류되었습니다. 0.51은 이를 위한 표기를 제공하지 않으므로 힌트가 가리키는 스칼라 분해를 사용하세요. 플러그인 `options:`, frontmatter 키, 상태 shape, 브리지 결과는 YAML로 쓸 수 있으므로 이 타입을 그대로 유지합니다.

- `directives/*.yaml` — `::name` 디렉티브 선언([브리지](/ko/plugins/bridge/) 참고)입니다.
- `state/*.yaml` — 재사용 가능한 타입 지정 레코드 shape(`stateShapes:`)와 구조화된 경로 템플릿(`stateTemplates:`)입니다. 한 파일에 둘 다 둘 수 있습니다(0.24.0부터, 이전에는 두 번째 항목이 버려졌습니다).
- `providers/*.yaml` — 고정된 스냅숏에 대해 해석되는 id 레지스트리입니다.
- `bridge/*.yaml` — 타입이 지정된 런타임 브리지 capability입니다.
- `defs/*.yaml` — 공유 타입 지정 CEL `@refs`입니다.
- `assetKinds`(`assetkinds/*.yaml`) — 순서가 지정된 `fallback` 훅과 함께하는 아티팩트 id 세그먼트 템플릿(조합/조회 모드)입니다.
- `stampAttrs`(`stampattrs/*.yaml`) — 콘텐츠 줄, 디렉티브, 그리고(0.50부터) `<branch>` / `<hub>` / `<choice>`에 붙는 추가 속성입니다. 횡단으로 두거나 `on:`으로 이름 붙은 표면에 한정할 수 있습니다(아래 참고).
- `enums/*.yaml`, `frontmatter/*.yaml`, `docs/*.md` — 이름 붙은 enum 도메인, 플러그인이 소유하는 메타 키, 호버 문서입니다.
- `events/*.yaml` — 퀘스트의 `<on event>`가 이름 붙일 수 있고 `lute trace --event`가 발생시킬 수 있는 월드 이벤트입니다.
- `occasions/*.yaml` — [비트](/ko/language/beats/)가 응답하는 엔진의 순간입니다(dsl 0.21.0). 각 순간은 프로젝트 엔티티 종류에서 가져온 대상을 위해 발생할 수 있고(dsl 0.22.0), 하나의 승자·제공 목록·시퀀스로 제시되며(dsl 0.23.0), [`raisedWhen`](#계기-게이트-raisedwhen) 조건으로 선택적으로 게이트할 수 있습니다(dsl 0.27.0).
- `rewardKinds`(`rewardkinds/*.yaml`) — `<reward kind>` 값의 닫힌 집합입니다. 선택적인 [대상 계약](#보상-대상-계약), 추가 속성, 보상을 기록하는 상태 경로(dsl 0.23.0), 그리고(0.50부터) 지급이 적용하는 팩트와 퀘스트 accept([보상 효과](#보상-효과))를 가질 수 있습니다.
- `cast/*.yaml` — 콘텐츠 줄에서 사용할 수 있는 화자와 표시 이름입니다(dsl 0.23.0).
- `hostTokens`(`hosttokens/*.yaml`) — 엔진이 렌더 시점에 `{{name}}` 보간에 치환해 넣는 이름입니다(0.50부터, [아래](#호스트-보간-토큰hosttokens) 참고).
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

occasion에는 다음 키만 사용할 수 있으며, 그 밖의 키는 사용할 수 없습니다: `select`, `target`, `description`, `judge`, `raisedWhen`, `payload`, `outsideRun`, 그리고(0.51부터) `raisedBy`.

occasion의 `select:`는 발생했을 때 엔진이 무엇을 제시할지 말합니다. `first`(기본값)는 하나의 승리 비트와 그 뒤의 자격 있는 [`also`](/ko/language/beats/#also로-곁가지-발언) 비트를 제시합니다. `all`은 자격 있는 모든 비트를 제공하고 플레이어가 하나를 고릅니다. `sequence`(dsl 0.23.0)는 선택 순서대로 자격 있는 모든 비트를 제시합니다. 예를 들어 저녁 루틴 뒤에 그날의 사건을 제시하는 방식입니다([계기 조합하기](/ko/language/beats/#계기-구성하기) 참고). `all` 또는 `sequence` occasion에서 비트의 `also: true`는 `E-BEAT-ATTR`입니다. 같은 이름의 월드 이벤트도 `events:` 아래에 선언되어 있다면, occasion이 발생할 때마다 비트 뒤에 그 이벤트가 발생하고 occasion이 `on=` 목표를 판정하기 전에 실행됩니다([계기](/ko/language/beats/#계기) 참고).

occasion의 `judge:`(dsl 0.24.0 §2)는 `on=`으로 자신을 이름 붙인 퀘스트 목표를 발생 시점에 언제 판정할지 말합니다. 기본값 `after`는 비트가 제시된 뒤 판정하며, 이는 0.21.0부터의 순서입니다. `before`는 비트를 결정하기 전에 판정하고 퀘스트를 정착시키므로, `dayEnd`의 에필로그가 퀘스트가 어떻게 끝났는지 읽을 수 있습니다(`quest.<id>.failedBy`, [퀘스트와 씬](/ko/language/quests-and-scenes/) 참고). 옮겨지는 것은 판정뿐입니다. 발생에 응답하는 `<on>` 핸들러 본문, 즉 같은 이름의 `<on event>` 핸들러와 정착되는 퀘스트의 `questComplete` / `questFailed` 핸들러는 여전히 비트 뒤에 실행되므로 내레이션은 씬 뒤에 나옵니다. 그 밖의 값은 `E-PLUGIN-PARSE`입니다. `judge: before`가 없는 occasion은 기존 `capabilitySnapshot` 스탬프를 유지합니다.

occasion의 `target:`은 무엇을 위해 발생하는지 말합니다. 없거나 `false`이면 대상이 없습니다. `true`는 0.21.0의 의미를 유지합니다. 어떤 점으로 구분된 id를 위해 occasion이 발생하고, 비트의 대상은 모양만 검사됩니다. **도메인** `{ prefix, entity }`(dsl 0.22.0)도 집합을 닫습니다. 대상은 `<prefix>.<member>`이고, `entity`는 프로젝트가 스키마의 `entities:` 아래 선언한 엔티티 종류입니다. 플러그인은 접두사와 종류를 제공하고, 프로젝트는 멤버를 제공하거나 엔진이 채우는 멤버를 위해 종류를 `open:`으로 선언합니다. 도메인 밖의 비트 대상은 did-you-mean과 함께 `E-BEAT-ATTR`이며, occasion의 종류를 문서 스키마가 선언하지 않은 경우 모든 대상도 마찬가지입니다([비트](/ko/language/beats/#대상-도메인) 참고). `target:`이 bool도 아니고 `{ prefix, entity }` 맵(선택적인 `members:` 포함)도 아니면 플러그인 로드가 `E-PLUGIN-PARSE`로 실패합니다.

도메인의 선택적인 **`members:`** 목록은 종류의 하위 집합으로 범위를 좁힙니다. 위의 `bossDefeated`는 프로젝트의 `foe` 종류에 멤버가 더 있어도 `boss.gatekeeper`와 `boss.warden`에 대해서만 발생합니다. 종류의 멤버는 플러그인이 볼 수 없는 프로젝트 어휘이므로 체커가 목록을 판정합니다. 종류가 선언하지 않은 멤버를 목록에 적으면 해당 occasion의 모든 대상이 그 잘못된 멤버를 이름으로 지정하는 `E-BEAT-ATTR`이 됩니다. `open:` 종류에서는 목록을 적은 그대로 사용합니다. 하위 집합은 도메인의 모든 소비자에 똑같이 적용됩니다: 비트와 목표 대상, [`lute play`](/ko/tooling/play/) 단계의 `target:`, `lute new scene --target`, `lute calendar --target`입니다. 로더 자체는 occasion 파일에서 `E-PLUGIN-PARSE`로 다음 두 형태를 거부합니다:

- 빈 목록: ``occasion `bossDefeated`'s `target.members` is empty; list at least one member of entity kind `foe`, or drop `members` to draw targets from the whole kind``
- 멤버 중복: ``occasion `bossDefeated`'s `target.members` lists `warden` more than once``

occasion의 **`raisedWhen:`**(dsl 0.27.0 §4)는 엔진이 occasion을 아예 언제 발생시킬 수 있는지 말합니다. 위의 `enter`는 플레이어가 방에 들어갈 수 있을 때만 그 방에 대해 발생하는 순간입니다. 아래 [계기 게이트](#계기-게이트-raisedwhen)를 참고하세요.

occasion의 **`payload:`**는 매번 발생할 때 비트에 넘겨주는 타입 지정 값을 선언합니다. 이는 위의 매니페스트 타입 시스템에서 필드와 타입을 대응시키는 맵입니다(`copies: int`). 예를 들어 가챠 추첨은 뽑힌 영웅과 함께 온 복사본 수를 전달합니다. 비트는 이를 `occasion.payload.<field>`로 읽으며, 해당 occasion의 `<objective on=>`과 그 이름에 대응하는 `<on event=>` 핸들러도 마찬가지입니다([occasion payload](/ko/language/beats/#계기-페이로드) 참고). 모든 발생은 모든 필드를 제공합니다. 하나를 빠뜨린 `lute play` 단계는 그 필드를 이름 붙이는 사용 오류가 되며, `lute trace`와 `lute test`는 mock이 시드하지 않은 필드를 읽는 줄에서 불완전한 상태로 중단하고, payload가 있는 occasion을 이름 붙인 시계 `raise:`는 `E-CLOCK-DECL`입니다.

occasion의 **`outsideRun: true`**는 런 바깥에 존재하는 순간, 즉 타이틀 화면이나 런 사이의 갤러리를 표시합니다. 엔진은 프로젝트의 [`terminal:`](/ko/state/schemas/#게임-종료-terminal)이 성립한 뒤에도 이를 발생시키며, 체커는 `!terminal` 아래에서 그 비트를 판정하지 않습니다. 컴파일된 아티팩트는 이런 occasion을 `outsideRun` 아래에 나열합니다.

occasion의 **`raisedBy: declared`**(0.51)는 호스트가 디렉티브나 보상 종류의 [선언된 `raises`](#선언된-occasion-raise)가 이름 붙인 곳에서만 그 occasion을 발생시킨다는 뜻입니다. 값은 `declared` 하나뿐이며, 키를 생략하면 호스트가 언제든 occasion을 발생시킬 수 있는 기본 동작이 유지됩니다. 어떤 `raises` 항목도 닿지 못하는 `raisedBy: declared` occasion은 그에 응답하는 씬이나 비트에서 `W-OCCASION-NEVER-RAISED`가 됩니다. 프로젝트 [시계](/ko/language/clock/)가 발생시키는 occasion은 선언된 raise와 무관하게 발생하므로, 그 occasion에는 `raisedBy: declared`가 앵커를 만들지 않고 `W-OCCASION-NEVER-RAISED`도 침묵합니다. 그 밖의 `raisedBy` 값은 `E-PLUGIN-PARSE`입니다. 이 키는 선언한 스냅숏의 `capabilitySnapshot`만 바꿉니다.

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

### 현지화 가능한 텍스트 속성: `type: text`

0.51부터 디렉티브 속성은 **`text`**로 타입을 지정할 수 있습니다. 번역할 수 있는 플레이어 노출 텍스트로, 대사 줄과 같은 방식으로 주소가 매겨지고 현지화됩니다.

```yaml unverified="0.51 feature; verified after S1-S8 land"
directives:
  - name: banner
    attrs:
      - { name: title,    required: true, type: text }
      - { name: subtitle, type: text }
      - { name: style,    type: { enum: [toast, card] } }
```

`text`는 `plugin` 레코드로 컴파일되는 디렉티브(어떤 종류의 `lower:`도 없고 `bridge:`도 없는 디렉티브)의 속성(`attrs:`) 타입으로**만** 허용됩니다. 다음은 속성 위치에서 `E-PLUGIN-PARSE`로 플러그인 로드가 실패합니다:

- `lower:`(스테이징 레코드나 빌트인)나 `bridge:`가 있는 디렉티브의 `text`: `` `title` of `::banner` is `text`, but `::banner` lowers to the `actor` record, which carries no localized text — declare it `string`, or drop `lower:` so the call stays a `plugin` record ``;
- `default:`가 있는 `text`. 번역할 수 없는 원문이 되기 때문입니다;
- 타입 문법을 읽는 그 밖의 모든 곳의 `text`: 스탬프(`stampAttrs`, 구조화 타입 거부와 함께 어셈블리에서 거부), 플러그인 `options:`, occasion `payload:`, 보상 종류의 `attrs:`, 브리지 결과, `{ list: text }`. 각 위치는 자기 고유의 잘못된 타입 진단을 유지하며, 그 진단은 `text`가 허용되는 유일한 위치를 알려 줍니다. [호스트 토큰](#호스트-보간-토큰hosttokens)도 `text`가 될 수 없습니다. 그 타입은 `string`, `int`, enum입니다;
- `text` 속성이 있는 디렉티브가 `code`라는 이름의 속성도 선언하는 경우. `code`는 그런 디렉티브의 식별 속성이기 때문입니다.

`on: [banner]`로 한정되고 이름이 `code`인 스탬프는 그 코어 속성에 가려져 `E-PLUGIN-RESERVED-STAMP-ATTR`가 됩니다. 이름이 `code`인 교차 스탬프는 다른 디렉티브에는 계속 적용되며, `text` 속성이 있는 디렉티브에서는 줄의 자체 `code`처럼 코어 `code`가 먼저 매칭됩니다.

문서에서 `text` 값은 선택지 라벨 문법의 따옴표 문자열입니다. 상태 경로, `@def`, 호스트 토큰의 `{{…}}` 보간(수정자 포함)과 리터럴 중괄호를 위한 `\{{`를 쓸 수 있고, 인라인 마크업은 파싱하지 않습니다. `@ref` 값은 `E-ATTR-TYPE`입니다. 그러면 디렉티브는 내용 줄처럼 `code=`(문자열)를 받으며, 이는 `text` 속성이 하나 이상 있는 디렉티브에서 허용됩니다. 다른 디렉티브에서 `code`는 계속 `E-UNKNOWN-ATTR`입니다.

```lute unverified="0.51 feature; verified after S1-S8 land"
::banner{code="0010" title="Welcome to the hall, {{userName}}" style="card"}
```

각 `text` 속성의 현지화 id는 `{prefix}~{tag}~{code}~{attr}`(위에서는 `intro~banner~0010~title`)이며, `~`가 들어간 `code`는 `E-LINE-CODE-RESERVED`입니다. `lute tag`는 빠진 `code=` 값을 써 넣고, `lute loc export`는 `text` 속성마다 `directive` 행 하나를 내보내며, `lute loc import`와 `lute loc report`가 이를 받습니다. 컴파일된 `plugin` 레코드는 원문 문자열을 `fields.<attr>`에 유지하고 `localized` 맵을 추가합니다([런타임 계약](/ko/tooling/runtime-contract/#051-추가-사항)). 현지화를 모르는 호스트는 전처럼 `fields`를 읽습니다. `text` 속성이 없는 디렉티브는 0.50과 바이트 단위로 같게 컴파일되며, `text`는 선언한 스냅숏의 `capabilitySnapshot`만 바꿉니다.

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

계약은 `entity:` 또는 `provider:` 중 하나를 지정하며 둘 다 지정하지는 않습니다. 둘 다 지정하면 플러그인 로드가 실패합니다(`E-PLUGIN-PARSE`). `target:`이 없는 종류는 이전과 같이 어떤 `target=`이든 받을 수 있고 생략할 수도 있습니다. 대상을 받지 않는 종류는 `target: { forbidden: true }`로 이를 선언하며, 이 키는 **단독**으로만 쓸 수 있습니다. `provider`, `entity`, `required`와 함께 쓰면 플러그인 로드가 실패합니다. 대상을 받지 않는 종류가 id 공간을 이름으로 지정하거나 대상을 요구할 수는 없기 때문입니다. `COINS: { target: { forbidden: true } }`라면 ``<reward kind="COINS" target="x"/>``는 `E-REWARD-TARGET`입니다: `` `<reward kind="COINS">` takes no `target=`: its reward kind declares `target: { forbidden: true }` — remove the attribute ``. `forbidden: true`가 없는 선언은 `capabilitySnapshot`을 그대로 유지하고, 이를 설정하면 스냅숏이 바뀝니다. 위의 ITEM 계약은 수동으로 관리하는 목록이 놓치는 두 실수를 모두 잡아냅니다.

<!-- lute-diagnostics -->
```
./quests/dex.lute:11:3: error [E-REWARD-TARGET] `<reward kind="ITEM">` needs a `target=`: the reward kind declares `target: { required: true }`
./quests/dex.lute:12:31: error [E-REWARD-TARGET] `goodRd` is not a member of entity kind `bagItem`, which `<reward kind="ITEM">` targets — did you mean `goodRod`?
```

작성한 값이 따옴표로 묶인 `target="…"`일 때만 아티팩트에 실립니다. 보상의 대상은 엔진이 지급 시점에 쓰는 정적 데이터이기 때문입니다. 따라서 `@ref`(`target=@n`)나 값 없는 `target`은 0.49부터 계약이 무엇이든, 계약이 없는 종류에서도 값 위치에서 `E-REWARD-ATTR`입니다: ``<reward target=…> takes a quoted id, e.g. `target="potion"`: `target=@n` is a reference, and a reward's target is static data the engine grants against``. 값 없는 `target`은 대신 `a bare `target` has no value`라고 알려 줍니다. `required: true`는 작성한 대상을 누락으로 한 번 더 보고하지 않고, `entity:`나 `provider:` 계약도 더하는 것이 없습니다. 이 거부가 유일한 진단입니다. `forbidden: true`에서는 `E-REWARD-TARGET`만 보고됩니다.

엔진이 소유한 id에는 어떤 형식을 사용해야 할까요? id를 저장소에 나열하고 그곳에서 검토할 수 있다면 **엔티티 종류**를 사용합니다(위의 `schema/items.schema.yaml` 같은 리드 소유 스키마 하나). 엔진의 카탈로그가 기준 정보이고 프로젝트가 고정된 스냅숏으로 이를 검사한다면 **프로바이더**를 사용합니다([프로바이더와 카탈로그](/ko/tooling/providers-and-catalog/) 참조). 어느 쪽이든 디렉티브 속성과 보상 종류를 **동일한** 종류 또는 프로바이더로 형식 지정해야 합니다. 그러면 `::give{item}`과 `<reward kind="ITEM" target>`이 하나의 id 공간을 공유하고, `lute refs --project <DIR> --attr give.item --reward ITEM`이 두 항목을 모두 나열합니다(대상이 없는 보상은 `(no target)`으로 표시).

### 보상 효과

0.50부터 보상 종류는 **`effects:`**를 선언할 수 있습니다. 지급이 assert하거나 retract하는 팩트, 그리고 accept하는 퀘스트입니다. 각 목록은 모두 선택 사항입니다.

```yaml
# rewardkinds/league.yaml
rewardKinds:
  BADGE:       { target: { entity: person, required: true }, effects: { asserts: ["met(@target)"] } }
  REVOKE:      { target: { entity: person, required: true }, effects: { retracts: ["met(@target)"] } }
  OFFER_QUEST: { target: { required: true }, effects: { accepts: ["@target"] } }
```

`effects`는 `asserts`, `retracts`, `accepts`, 그리고(0.51부터) [`raises`](#선언된-occasion-raise)만 받고 다른 키는 받지 않습니다. 빈 `effects: {}`는 `E-PLUGIN-PARSE`입니다("declares no effect". 메시지는 키 목록에 `raises:`를 포함합니다). 반면 `effects: { raises: [...] }`만 있는 블록은 유효합니다. `asserts` / `retracts`의 각 항목은 [디렉티브 효과](#디렉티브가-assert하거나-retract하는-팩트)와 같은 팩트 패턴 `relation(arg, …)`입니다.

- **`writes`는 허용되지 않습니다.** 보상의 상태 쓰기는 `credits:`이므로 `effects.writes`는 `E-PLUGIN-KEY`이며, 메시지에 `` — a reward's state write is `credits:` ``가 덧붙습니다.
- **패턴 인자**는 멤버, `true` / `false`, `@target`, 그리고 `_`(`retracts`에서만)입니다. 그 밖의 `@name`은 `E-PLUGIN-PARSE`입니다: `` reward kind `BADGE` effect `met(@rarity)` reads `@rarity`; a reward kind's effects read only `@target` (a `<reward>` takes no other attribute) ``. 보상 종류 자체의 `attrs:`는 `<reward>`에 쓸 수 없으므로 읽을 다른 값이 없습니다.
- **`accepts`** 항목은 `@target` 또는 리터럴 퀘스트 id(식별자)입니다. 그 밖은 `E-PLUGIN-PARSE`입니다.
- `target: { forbidden: true }`인 종류의 효과가 `@target`을 읽으면 `E-PLUGIN-PARSE`입니다. 그 효과는 바인딩될 수 없기 때문입니다.
- `@target`은 보상의 따옴표로 묶인 `target=`에 바인딩됩니다. `target=`이 없는 `<reward>`는 그 `@target`을 읽는 패턴의 효과를 적용하지 않으며, 추가 진단도 없습니다. `target: { required: true }`인 종류는 이미 `E-REWARD-TARGET`을 보고합니다.

체커는 이런 종류의 `<reward>`마다 `@target`을 바인딩해 판정합니다. 팩트 패턴은 그것의 `::assert` / `::retract`와 똑같은 기준(관계 선언, arity, 인자 종류와 멤버)으로, 같은 코드를 사용해 보상의 `kind=` 위치에서 판정되며, 각 메시지는 `` `<reward kind="BADGE">` asserts `met(mira)` (its kind's `effects`): `` 접두사를 가집니다. `accepts`의 id는 `::accept`의 규칙으로 판정됩니다. 알 수 없는 id(did-you-mean 포함), `start`가 있는 퀘스트, 자동 활성화되는 자식 퀘스트는 `E-ACCEPT-TARGET`입니다. id가 `@target`에서 왔다면 보상의 `target=` 값 위치에, 아니면 `kind=` 위치에 보고됩니다: `` `<reward kind="OFFER_QUEST" target="secnd">` accepts quest `secnd` (its kind's `effects.accepts`), but no quest in the project declares that id — did you mean `second`? ``. 지급 시점에 효과가 무엇을 의미하고 분석이 이를 어떻게 읽는지는 [퀘스트와 씬](/ko/language/quests-and-scenes/)에 있습니다. `effects:`가 없는 종류의 해시는 이전과 같습니다.

### 선언된 occasion raise

0.51부터 디렉티브의 `effects:`와 보상 종류의 `effects:`는 호출이나 지급에 대한 응답으로 **호스트**가 발생시키는 occasion을 선언할 수 있습니다. Lute는 raise를 실행하지 않습니다. occasion을 발생시키는 것은 호스트의 일입니다. 선언된 raise는 호스트가 무엇을 하는지에 대한 약속이며, 체커가 호출을 그 occasion에 응답하는 씬이나 비트와 연결할 수 있게 해 줍니다.

```yaml unverified="0.51 feature; verified after S1-S8 land"
# plugins/acme.stage/occasions/o.yaml
occasions:
  cutscene: { select: first, target: { prefix: scene, entity: cutsceneId }, raisedBy: declared }
```

```yaml unverified="0.51 feature; verified after S1-S8 land"
# plugins/acme.stage/directives/d.yaml
directives:
  - name: playScene
    attrs:
      - { name: scene, required: true, type: { entity: cutsceneId } }
    effects:
      raises: [{ occasion: cutscene, target: "@scene" }]
```

```yaml unverified="0.51 feature; verified after S1-S8 land"
# plugins/acme.stage/rewardkinds/r.yaml
rewardKinds:
  CUTSCENE: { target: { entity: cutsceneId, required: true }, effects: { raises: [{ occasion: cutscene, target: "@target" }] } }
```

```lute unverified="0.51 feature; verified after S1-S8 land"
--- # scenes/chapter2.lute
kind: scene
id: chapter2
on: cutscene
target: scene.chapter2
---
```

- **`effects.raises`**는 `{ occasion, target? }` 항목의 비어 있지 않은 목록이며 다른 키는 없습니다. 디렉티브에서는 `writes`, `asserts`, `retracts` 옆에, 보상 종류에서는 `asserts`, `retracts`, `accepts` 옆에 놓입니다. `raises`만 선언한 보상 종류는 `lute.quest.rewardEffects/1` 시맨틱 id를 `requiredSemantics`에 넣지 않습니다.
- **`target`**은 다음 중 하나입니다. 생략(대상 없이 선언된 occasion에만 허용), 디렉티브에서의 `@<attr>`(선언된 속성 이름), 보상 종류에서의 `@target`(보상의 `target=`), 또는 비트가 쓰듯 통째로 적은 리터럴 대상(`scene.chapter2`). `target: { forbidden: true }`인 종류는 `@target`으로 raise할 수 없습니다.
- occasion의 **`raisedBy: declared`**는 호스트가 선언된 raise를 통해서만 그것을 발생시키게 합니다([occasion 키](#export-파일) 참고). 없으면 선언된 raise는 아티팩트에 기록되고 검사되지만 그래프 엣지를 그리지 않습니다.

**로드 오류**(`E-PLUGIN-PARSE`, 항목 위치): `raises: []`; `occasion`과 선택적 `target`을 가진 매핑이 아닌 항목; `@<name>`도 점으로 구분된 대상도 아닌 `target`; 디렉티브에서 선언되지 않은 속성을 가리키는 `@<name>`; 보상 종류에서 `@target`이 아닌 `@<name>`, 또는 `target: { forbidden: true }`인 종류의 `@target`; 알 수 없는 `raisedBy` 값.

raise를 실을 곳이 없는 낮춤, 즉 어떤 `lower:`(스테이징 레코드나 빌트인)든 가진 디렉티브는 `effects.raises`를 선언할 수 없습니다. `raises` 키에서 `E-PLUGIN-INVALID-DIRECTIVE`입니다: `` directive `::playVideo` lowers to the `video` record, which carries no effects (only a `plugin` record does), so its `effects.raises` could never reach the host — drop `lower:` so the call stays a `plugin` record, or raise the occasion from a passthrough directive ``. 이는 `effects.writes`가 레코드 낮춤에서 이미 갖는 경계와 같습니다.

**어셈블리.** occasion은 다른 플러그인에서 올 수 있으므로 플러그인이 어셈블될 때 조회합니다. 거부된 항목은 `E-PLUGIN-RAISES`이며 버려지고, 형제 항목은 그대로 반영됩니다:

| 오류 | 메시지 형태 |
|---|---|
| occasion이 스냅숏에 없음 | `` directive `::playScene` (`acme.stage`) raises occasion `cutscne`, which no active plugin declares — did you mean `cutscene`? `` (다른 플러그인의 occasion을 raise하는 플러그인은 그것을 `depends:`에 나열해야 하며, 설치되었지만 비활성인 플러그인에 occasion이 있으면 메시지가 그렇게 알려 줍니다) |
| 대상 없는 occasion에 `target`이 있거나, 대상이 있는 occasion에 없음 | `` … raises `cutscene` without a target, but `cutscene` is raised for a target (`scene.<cutsceneId>`) — add `target: "@<attr>"` `` |
| 도메인 occasion `{ prefix, entity }`에서 선언된 타입이 `{ entity: <entity> }`가 아닌 `@attr`(보상 종류에서는 `target:` 계약이 `{ entity: <entity> }`가 아님) | `` … raises `cutscene` for `@scene`, which is typed `string`; the target of `cutscene` is a `cutsceneId` member, so type the attribute `{ entity: cutsceneId }` `` |
| occasion의 도메인 형태(`<prefix>.<member>`, `members:`가 있으면 그 목록) 밖의 리터럴 대상 | `` … raises `cutscene` for `room.office`, but its targets are `scene.<cutsceneId>` `` |

**호출별, 지급별 검사.** 프로젝트 패스(`lute check --project`)는 `@attr` 대상이면 호출의 속성 값 위치에, 리터럴이면 디렉티브 헤드에, 보상이면 `target=`(또는 `kind=`) 위치에 `E-RAISES-TARGET`을 보고합니다. 해석된 대상은 호출에 쓴 값이고, 없으면 속성의 선언된 `default:`입니다. 둘 다 없는 대상만 바인딩되지 않으며, 바인딩되지 않은 대상은 추가 진단 없이 raise를 억제합니다(`required`가 이미 보고합니다). 해석된 각 대상에 대해:

- 프로젝트 종류(또는 occasion의 `members:`)의 멤버가 아닌 도메인 대상 `<prefix>.<value>`는 did-you-mean과 함께 `E-RAISES-TARGET`입니다;
- 어떤 씬이나 비트도 응답하지 않는 대상(그 occasion을 `on:`으로 갖고 그 대상을 `target:`으로 갖거나 `target:`이 없는 문서가 없음)은 `E-RAISES-TARGET`입니다: `` `::playScene{scene="chaptr2"}` raises `cutscene` for `scene.chaptr2`, but no scene or beat answers it — did you mean `scene.chapter2`? ``.

`@def` 속성 값은 모든 속성 값처럼 상수로 접히므로 정적 대상이며, 컴포넌트 본문 안의 호출은 각 `::use`에서 그 인자가 바인딩된 채 해석됩니다. 유일한 동적 대상은 `occasion.target`을 읽는 값(비트 안의 호출)입니다. 이는 여기서 판정하지 않으며, 그래프에서는 그 occasion에 응답하는 모든 씬과 비트를 대신합니다.

**호스트가 하는 일.** 디렉티브 호출을 실행한 프레젠테이션이 끝난 뒤(`presentationEnd`), 호스트는 런타임이 idle이 되면 나열된 각 occasion을 해석된 대상에 대해 레코드 순서와 목록 순서대로 `raiseOccasion` 입력으로 발생시킵니다. 보상의 `grant` 레코드를 담은 출력이 적용된 뒤에는 보상 종류의 raise도 같은 식으로 발생시킵니다. occasion 자체의 게이트(`raisedWhen`)가 raise를 거부할 수 있으며, 그 거부를 보고하는 것은 호스트의 몫이므로 Lute는 선언된 raise를 보장이 아닌 *가능*으로만 읽습니다. 어디에도 팩트, 쓰기, Must 주장을 더하지 않으며, 모든 분석은 이를 may 엣지로 읽습니다. [씬 그래프](/ko/connectivity/scene-graph/#선언된-raise가-그리는-엣지)와 [커스텀 호스트](/ko/player/custom-hosts/#선언된-raise)의 호스트 책무를 참고하세요.

컴파일된 `plugin` 레코드와 `grant` 레코드는 해석된 `raises` 목록(`{ occasion, target? }`)을 담습니다. `@attr`은 속성 값으로 바인딩되고 도메인 접두사가 적용되며(`scene.chapter2`), 비어 있으면 필드를 생략하므로 `raises`가 없는 매니페스트는 모든 아티팩트와 `capabilitySnapshot`을 그대로 둡니다.

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

엔트리는 일반적인 `AttrDecl`입니다. 즉, 디렉티브 속성이 사용하는 것과 동일한 `{ name, required?, type, default? }` 형태입니다. `on:`이 없는 엔트리는 **횡단**입니다. 스탬프 캐리어가 있는 **모든** 디렉티브와 콘텐츠 라인(`@speaker{…}: text`)에서 해당 표면 자체의 속성에 더해 사용할 수 있고, `<branch>`, `<hub>`, `<choice>`에서는 쓸 수 없습니다. `on:`이 있는 엔트리는 이름 붙은 표면에 **한정**됩니다(0.50부터, [아래](#표면에-스탬프-한정하기-on)). 해석은 엄격합니다. 표면 자체의 선언이 먼저 적용되고, 그다음 `stampAttrs`, 그다음 `E-UNKNOWN-ATTR`이 적용됩니다. 값 형식 지정은 기존 속성 경로를 따르므로 잘못 입력하면 새 규칙이 아니라 일반적인 `E-ATTR-TYPE` / `E-BAD-ENUM`이 됩니다.

`::sfx{sound="chime" bonusId="b-02"}` 다음에 `@marina{code="0010" bonusId="b-01" bonusScore="7"}: Welcome back.`이 오면, 각각의 레코드가 핵심 키(`kind`, `family`, `position`, `timing`) 뒤의 **중첩된 `stamps` 객체 하나**(0.49부터)에 속성을 담은 두 레코드로 컴파일됩니다. 레코드 자체의 키 옆에 평탄화되지도, 레코드의 `fields` 안에 들어가지도 않습니다.

```json
{ "kind": "sfx", "family": "staging", "position": "001-0100", "sound": "chime", "stamps": { "bonusId": "b-02" } }
{ "kind": "line", "family": "content", "position": "001-0200", "role": "dialogue",
  "speaker": "marina", "text": "Welcome back.", "lineId": "marina.s01ep01.marina_0010",
  "voiceKey": "marina.s01ep01.marina-0010", "stamps": { "bonusId": "b-01", "bonusScore": 7 } }
```

각 항목의 JSON 값은 이전처럼 선언한 플러그인의 속성 선언으로 타입이 지정됩니다. 작성된 스탬프가 없으면 객체는 생략되므로 스탬프가 없는 레코드는 바뀌지 않습니다. 스탬프가 자기 키 아래에 있으므로, 운반하는 레코드의 키(`tag`, `speaker`, `text`, `label`, `name`, `id` 등)와 같은 이름의 플러그인 스탬프도 더 이상 충돌하지 않습니다. 0.49 이전에는 그런 스탬프가 같은 키를 두 번 썼습니다. 같은 `stamps` 객체는 플러그인 이벤트 레코드와 [브리지](/ko/plugins/bridge/) 호출의 `attrs` 옆에, `attrs`에 병합되지 않은 채 호스트에 전달됩니다.

**작성되지 않은** 스탬프 속성은 선언에 `default`가 있어도 주입되지 않습니다. 없다는 것은 없는 것이므로, 교차 어휘를 선언하고 그중 아무것도 작성하지 않으면 아티팩트는 바이트 단위로 동일합니다. 다만 선언이 무료인 것은 아닙니다. `stampAttrs`는 `capabilitySnapshot`에 참여합니다. 교차 어휘가 변경되면 capability 표면이 변경된 것이고 엔진이 불일치를 거부할 수 있어야 하기 때문입니다.

### 표면에 스탬프 한정하기: `on:`

0.50부터 엔트리는 키 하나를 더 가질 수 있습니다. 비어 있지 않은 표면 이름 목록인 **`on:`**입니다. 한정된 스탬프는 이름 붙은 표면에서만 쓸 수 있으며 `required`일 수 있습니다.

```yaml
# plugins/acme.stage/stampattrs/columns.yaml
stampAttrs:
  - { name: voiceAssetId, type: { providerRef: voice }, required: true, on: [line] }
  - { name: assetName,    type: string, on: [bg, actor] }
  - { name: menuType,     type: { enum: [plain, timed, minigame] }, on: [branch, hub] }
  - { name: optionKey,    type: string, required: true, on: [choice] }
  - { name: cue,          type: string }                          # cross-cutting, as before
```

```lute
<branch id="route" prompt="Which way?" menuType="plain">
  <choice id="goLeft" text="Left" optionKey="L1">
    @mira{voiceAssetId="VO.mira.0010"}: Left it is.
  </choice>
</branch>
```

표면 어휘는 닫혀 있습니다.

| 표면 이름 | 가리키는 대상 |
|---|---|
| `line` | 컴포넌트, 퀘스트, 목표, `<on>`, 엔트리, 비트 본문의 줄을 포함한 모든 콘텐츠 줄(`@speaker{…}: text`) |
| `branch`, `hub` | 해당 이름의 요소 |
| `choice` | `<branch>`나 `<hub>` 안의 모든 `<choice>` |
| 디렉티브 태그 | 해석된 스냅숏의 디렉티브 중 스탬프 캐리어가 있는 것, 즉 `jump`, `label`, `clear`가 아닌 것 |

`line`, `branch`, `hub`, `choice`는 항상 코어 표면을 뜻합니다(`branch`, `hub`, `choice`는 이미 예약된 플러그인 디렉티브 이름이며 `E-PLUGIN-RESERVED-NAME`입니다). 따라서 `line`이라는 태그의 플러그인 디렉티브는 `on:`에 쓸 수 없습니다. `on:`의 순서는 의미가 없습니다.

**`on:`이 없으면** 엔트리의 의미는 그대로입니다. 스탬프 캐리어가 있는 모든 디렉티브와 모든 콘텐츠 줄에서 쓸 수 있고, `<branch>`, `<hub>`, `<choice>`에서는 **쓸 수 없습니다**. 이 세 요소는 횡단 스탬프에 닫혀 있으며, `on:`이 이름으로 지정한 스탬프만 거기서 허용됩니다.

**로드 오류**(패키지별, 엔트리 위치의 `E-PLUGIN-PARSE`):

| 오류 | 메시지 |
|---|---|
| `on:`이 문자열 목록이 아니거나 비어 있음 | `` stamp attribute `X` has an empty `on:` — list the surfaces it applies to, or omit `on:` for every directive and content line `` |
| `on:`에 같은 표면이 반복됨 | `` stamp attribute `X` lists `bg` twice in `on:` `` |
| `on:` 없이 `required: true` | `` stamp attribute `X` is `required: true` but has no `on:`; a stamp required on every record is not a column — name its surfaces with `on: […]` `` |

횡단 스탬프는 `required`일 수 없습니다. 누락을 보고할 단일 위치가 없기 때문입니다. 0.50 이전에는 이런 `required: true`가 조용히 아무 일도 하지 않았습니다. `on:`이 붙은 디렉티브 속성은 여전히 `E-PLUGIN-KEY`입니다. `on:`은 `stampAttrs` 엔트리 전용입니다.

**패키지 내 유일성.** 두 엔트리는 둘 다 `on:`이 있고 표면 집합이 서로소일 때만 이름을 공유할 수 있습니다. 그래서 한 이름을 표면마다 다르게 타입 지정할 수 있고(`bg`에는 `assetName: string`, `actor`에는 `assetName: int`), IR은 각각 자기 타입으로 직렬화합니다. 겹침은 모두(`on:`이 없는 엔트리는 모든 엔트리와 겹칩니다) `E-PLUGIN-DUP-ID`이며 공유된 표면을 이름으로 밝힙니다: `` stampAttr `assetName` is declared twice on the surface `bg` — give each entry disjoint `on:` surfaces ``.

**Assembly.** 활성 스냅숏의 디렉티브가 알려지면 각 엔트리를 다음 순서로 검사합니다. 거부된 엔트리는 버려지고 형제 엔트리는 그대로 적용됩니다.

1. 코어 스탬프 예약([아래](#예약된-스탬프-키)): `E-PLUGIN-RESERVED-STAMP-ATTR`.
2. `on:`의 각 이름이 표면으로 해석됩니다. 네 코어 표면도 아니고 스냅숏의 디렉티브도 아닌 이름은 **`E-PLUGIN-STAMP-SURFACE`**입니다: `` stamp attribute `assetName` (`acme.stage`) names `bgg` in `on:`, which is not `line`, `branch`, `hub`, `choice` or a directive of the resolved profile — did you mean `bg`? ``. 스탬프 캐리어가 없는 디렉티브도 같은 코드입니다: `` … names `jump` in `on:`, but `::jump` emits no stamped record ``(`jump`, `label`, `clear`가 그렇습니다). 다른 플러그인의 디렉티브에 스탬프를 한정하는 플러그인은 그 플러그인을 `depends:`에 선언해, 해당 디렉티브가 이를 활성화하는 모든 프로필에 있게 해야 합니다. 태그가 설치되었지만 비활성인 플러그인에 있으면 메시지가 이를 알려 줍니다.
3. **가림(shadowing).** 한정된 엔트리의 이름이 지정된 표면이 이미 소유한 속성이면 `E-PLUGIN-RESERVED-STAMP-ATTR`입니다: `` stamp attribute `prompt` (`acme.stage`) is scoped to `branch`, but `prompt` is an attribute of `<branch>` itself, so the stamp could never be read ``. 표면 자체의 속성은 다음과 같습니다. 디렉티브는 선언의 `attrs`(`::jump`는 `when` 추가), `line`은 콘텐츠 줄의 알려진 속성(`wait` 포함)과 가드 `when`, `branch`는 `id`, `prompt`, `timeout`, `default`, `hub`는 `id`, `prompt`, `choice`는 `id`, `text`, `when`, `into`, `value`, `once`, `exit`와 이름이 바뀌거나 제거된 `label`, `as`, `persist`입니다. 횡단 엔트리는 이전 규칙을 유지합니다. 표면 자체의 선언이 우선하고 엔트리는 거부되지 않습니다.
4. **플러그인 간 유일성.** 두 플러그인의 엔트리가 이름이 같고 표면이 겹치면 `E-PLUGIN-DUP-ACROSS`이며 두 플러그인과 공유된 표면을 밝힙니다. 두 플러그인의 서로소 엔트리는 합법입니다.

**검사.** 표면의 속성 `k`는 엄격하게, 처음 일치하는 것으로 해석됩니다. 표면 자체의 속성, 디렉티브라면 스탬프 없음 거부와 범용 타이밍 키, 해당 표면에 선언된 스탬프(그 선언으로 형식 지정: `E-ATTR-TYPE`, `E-BAD-ENUM`, `@ref` 타이핑), 이름이 바뀌거나 제거된 속성 진단, 마지막으로 `E-UNKNOWN-ATTR` 순서입니다. 이름이 다른 표면에만 선언되어 있으면 `E-UNKNOWN-ATTR`은 표면의 속성을 나열하는 대신 스탬프가 적용되는 곳을 알려 줍니다.

- `` `::bg` has no attribute `voiceAssetId`: it is a stamp of `acme.stage` for content lines only ``
- `` `<branch>` has no attribute `optionKey`: it is a stamp of `acme.stage` for `<choice>` only ``

표면이 작성하지 않은 **필수(required)** 스탬프는 표면의 머리(디렉티브나 요소의 여는 태그, 또는 줄) 위치에서 `E-MISSING-ATTR`입니다.

- `` `::bg` requires attribute `assetName` (a stamp of `acme.stage`) ``
- `` this content line requires attribute `voiceAssetId` (a stamp of `acme.stage` for content lines) ``
- `` `<choice id="goLeft">` requires attribute `optionKey` (a stamp of `acme.stage`) ``

이 규칙은 컴포넌트 본문을 포함해 표면이 작성되는 모든 곳에 적용됩니다. `::use`가 끼워 넣는 줄은 해당 컴포넌트에서 한 번만 검사됩니다. 스탬프는 주입되지 않으므로 선언된 `default:`는 보지 않습니다. 로직 태그 표면은 [선택지와 허브](/ko/language/choices-and-hubs/#branch-hub-choice의-플러그인-스탬프)를 참고하세요.

**스냅숏.** 한정된 엔트리는 정렬된 표면 목록과 함께 `capabilitySnapshot`에 접힙니다. `on:`이 없는 엔트리는 이전과 똑같이 해시되므로 `on:`을 쓰지 않는 매니페스트는 스냅숏을 유지합니다. `lute context`는 한정된 스탬프마다 표면을 출력합니다(`"on": "::bg, ::actor (stampAttrs)"`).

### 예약된 스탬프 키

코어 스탬프는 `at`, `duration`, `delay`, `wait`, `timeline`, `timing`, `provenance`, `source`라는 8개의 이름을 소유합니다. `at`, `duration`, `delay`, `wait`, `timeline`은 `timing`으로 컴파일되는 작성용 키이고, `timing`, `provenance`, `source`는 평탄화된 레코드 키로 남습니다. 중첩된 `stamps` 객체는 그 외의 모든 이름에서 *JSON* 충돌을 없애지만, 이 여덟 이름은 계속 예약됩니다. 같은 이름의 플러그인 스탬프는 작성자에게 여전히 모호하기 때문입니다. 플러그인이 그중 하나로 속성을 선언하면 assembly에서 **`E-PLUGIN-RESERVED-STAMP-ATTR`**과 함께 거부됩니다.

<!-- lute-diagnostics -->
```
$ lute check scene.lute --project .
lute: E-PLUGIN-RESERVED-STAMP-ATTR: plugin `arcia.bonus` declares reserved stamp attribute `duration`; `at`/`duration`/`delay`/`wait`/`timeline`/`timing`/`provenance`/`source` are owned by the core stamp (plugin §14)
```

두 표면 모두 적용됩니다. `stampAttrs` export와 일반적인 디렉티브별 `attrs` 엔트리가 모두 해당하므로, 플러그인은 어느 경로로도 예약 키에 접근할 수 없습니다. 문제가 있는 선언은 병합하지 않고 삭제되며, 예약되지 않은 형제 항목은 그대로 적용됩니다. 이것이 차단 플러그인 디렉티브가 자체 플래그를 `sync`로 지정하고 `wait`로 지정하지 않는 이유입니다([브리지](/ko/plugins/bridge/) 참조).

## 호스트 보간 토큰(`hostTokens`)

줄에는 엔진만 아는 값을 위해 맨 이름 `{{playerName}}`을 둘 수 있습니다. 플러그인 export `hostTokens`(`hosttokens/*.yaml`, 0.50부터)가 이런 이름을 선언합니다.

```yaml
# hosttokens/tokens.yaml
hostTokens:
  playerName: { type: string }
  rank:       { type: int }
  platform:   { type: { enum: [ios, android, web] } }
```

- 키가 토큰 이름이고 값은 `{ type }`이며, `type`은 `string`, `int`, `{ enum: [members…] }` 중 하나입니다. 다른 타입은 `E-PLUGIN-PARSE`, 다른 키는 `E-PLUGIN-KEY`입니다.
- 토큰 이름은 **식별자**입니다. ASCII 문자 또는 `_`로 시작하고 문자, 숫자, `_`가 이어집니다(단순한 name이 아닙니다. 호스트 토큰은 `{{…}}` 안에 맨 이름으로 쓰이기 때문입니다). 식별자가 아닌 키는 `E-PLUGIN-PARSE`입니다: `` host token `player-name` is not an identifier (letters, digits and `_`, not starting with a digit) ``(`player-name`, `2ndName`, `-x`, 빈 키 모두 거부됩니다).
- 상태 루트나 CEL 리터럴인 이름(`scene`, `run`, `user`, `app`, `quest`, `entry`, `prev`, `clock`, `occasion`, `season`, `true`, `false`, `null`)은 assembly에서 `E-PLUGIN-RESERVED-NAME`이며, 메시지에 이유가 담깁니다: `` `{{run}}` would read as a state root ``([예약어](/ko/reference/reserved-names/) 참고).
- **`lute.core`는 `userName: { type: string }`을 선언합니다.** 다른 플러그인이나 `lute.core`가 선언한 토큰을 다시 선언하면 `E-PLUGIN-DUP-ACROSS`입니다.
- `hostTokens`는 `lute.core`가 소유하지 않은 토큰이 있을 때만, 그리고 그 토큰만 `capabilitySnapshot`에 접힙니다. 따라서 토큰을 선언하지 않은 프로젝트는 스냅숏을 유지합니다. `lute context`는 선언된 비코어 토큰(이름, 타입, 플러그인)을 나열합니다.

텍스트에서 토큰이 어떻게 검사되는지, 그리고 런타임이 이를 그대로 두는 것은 [대화와 캐스트](/ko/language/dialogue-and-cast/#호스트-토큰)에 있습니다.

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
