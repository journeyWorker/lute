---
title: 프런트매터와 프로필
description: 모든 .lute 문서를 여는 YAML 프런트매터 블록 — 필수 및 선택 키와 프로필/플러그인 capability 선택기입니다.
---

모든 `.lute` 문서는 두 `---` 줄로 구분된 **YAML 프런트매터 블록**으로 시작합니다. 이 블록은 본문 내용보다 앞선 문서의 첫 번째 구문이어야 합니다. "이 문서는 무엇이며 어떤 capability를 사용하는가?"에 답합니다.

```yaml
---
kind: scene
title: The Full Showcase
character: marina
season: 1
episode: 1
pov: fixer
profile: showcase
uses: schema/game.schema.yaml
state:
  scene.affect.marina: { type: int, default: 0 }
defs:
  fond: { type: bool, cel: "scene.affect.marina >= 1" }
---
```

*([`docs/examples/showcase/episode01.lute`](https://github.com/journeyWorker/lute/blob/main/docs/examples/showcase/episode01.lute)의 프런트매터 발췌입니다.)*

## 필수 키

루트 문서는 **`kind`** — `scene`, `quest`, 또는 [`lore`](/ko/language/lore-entries/) — 를 선언해야 하며, 씬은 **`id`** 또는 **`character`**, **`season`**, **`episode`**의 세 항목으로 정체성을 선언해야 합니다. 이를 생략하면 정적 오류(`E-KIND-MISSING`, `E-META-MISSING`)가 발생합니다.

## 선택 키

- **`id`** — 씬의 정식 키(`town.welcome`, `mara.first`)입니다. `visited('…')`와 `after:`가 이름으로 사용하는 값, 아티팩트의 `meta.id`, 모든 파생 `lineId`의 `{prefix}`입니다. 문자, 숫자, `_`, `.`, `-`만 사용할 수 있습니다(`E-META-ID`). 생략하면 키가 `{character}.{episodeId}`로 파생됩니다.
- **`episodeId`** — 파생 키에 입력되는 안정적인 불투명 에피소드 id입니다. 생략하면 `s{season:02}ep{episode:02}`로 기본 설정됩니다(예: `season: 1, episode: 2` → `s01ep02`). 명시적으로 고정하면 번역 또는 음성 키를 깨뜨리지 않고 `season`/`episode`의 번호를 다시 매길 수 있습니다.
- **`title`** — 선택적인 사람이 읽는 제목(현지화 가능)이며 아티팩트의 `meta.title`로 전달됩니다. 문서의 제목은 이것 하나뿐입니다. 본문에 `# ` 헤딩을 쓰면 `E-INERT-TITLE`이 되고, 메시지는 `title:`을 지정하라고 안내합니다.
- **`pov`** — 시점(주인공) 화자의 id입니다. 누가 `mono`를 말할 수 있는지 결정합니다. `{mono}` 줄의 화자는 유효한 POV이거나 `monoSpeakers`에 있어야 하며, 그렇지 않으면 `E-MONO-POV`가 발생합니다(POV가 전혀 해석되지 않으면 `E-MONO-NO-POV`). 유효한 POV는 문서의 `pov:` 또는 `lute.project.yaml`의 프로젝트 `defaults.pov`입니다. 둘 다 없으면 POV 화자의 줄은 일반 줄로 컴파일되고 아티팩트에 `pov`가 포함되지 않습니다([대사와 캐스트](/ko/language/dialogue-and-cast/) 참고).
- **`monoSpeakers`** — POV와 함께 `mono`를 말할 수 있는 추가 화자 id 목록입니다. 예를 들어 `mira`에게도 속마음을 말하게 하는 씬에는 `monoSpeakers: [mira]`를 씁니다. `pov`처럼 프로젝트의 `defaults.monoSpeakers`에서 한 번 설정할 수 있으며, 문서 자체의 목록이 프로젝트 목록을 대체합니다. [컴포넌트](/ko/language/components-and-extends/#화자-매개변수)에서 확장된 줄은 그것을 확장하는 `::use`가 속한 문서의 POV와 `monoSpeakers`에 대해 검사됩니다.
- **`luteVersion`** — 언어 버전 고정값이며 `app.lang` 게임 상태와는 다릅니다.
- **`contentLang`** — 원본 작성 언어(예: `en-US` 또는 `ko-KR` 같은 BCP 47 코드)입니다.
- **`mode`** — 작성 모드이며, 정의된 형식은 `inline`뿐입니다.
- **`after`** — 씬의 연결성 전제조건입니다([퀘스트와 씬](/ko/language/quests-and-scenes/) 참고).
- **`uses` / `components` / `extends`** — 공유 상태 스키마와 재사용 가능한 콘텐츠 컴포넌트를 가져옵니다([가져오기](/ko/language/imports/), [컴포넌트와 extends](/ko/language/components-and-extends/) 참고).
- 인라인 **`state`** 및 **`defs`** 블록([상태 모델](/ko/state/state-model/), [Params](/ko/language/params/) 참고).

코어 키도 아니고 활성 플러그인이 소유하지도 않은 최상위 키는 정적 오류입니다. 플러그인은 매니페스트의 `frontmatter` export를 통해 추가 프런트매터 키를 제공할 수 있습니다. 예를 들어 초안 character/cast capability가 `cast:` 키를 소유할 수 있습니다.

## `identity:` — 프로젝트가 컴파일하는 id 형태

위의 `episodeId`는 더 큰 기계에 입력되는 값 중 하나이며, 이 기계는 문서별이 아니라 프로젝트별로 한 번 선언합니다. `lute.project.yaml`에서 다음과 같이 씁니다.

```yaml
identity:
  lineId: "{prefix}.{speaker}_{code}"
  voiceKey: "{prefix}.{speaker}-{code}"
```

위의 두 값은 모두 **기본값**입니다. `identity:` 블록을 선언하지 않는 프로젝트는 이 블록을 선언한 것과 정확히 동일하게 컴파일되며, 두 키 중 하나만 선언하는 프로젝트에서는 다른 키가 기본값으로 남습니다.

`{prefix}`는 작성하는 값이 아니라 파생되는 값입니다. 문서의 키 — 문서의 `id:` 또는 `{character}.{episodeId}` — 이며, `episodeId`는 위에서 설명한 대로 `s{season:02}ep{episode:02}`로 기본 설정됩니다. 따라서 `character: haven`, `season: 1`, `episode: 2`이면 prefix는 `haven.s01ep02`가 되고, `@purser{code="0020"}` 줄은 `lineId: "haven.s01ep02.purser_0020"`, `voiceKey: "haven.s01ep02.purser-0020"`로 컴파일됩니다. 같은 씬에서 `episodeId: pilot`을 고정하면 `haven.pilot.purser_0020`이 되고, `id: haven.deck`을 선언하면 `haven.deck.purser_0020`이 됩니다.

**0.22.0에서 변경됨: 기본 `voiceKey`가 `{prefix}`를 포함합니다.** 0.21까지는 모든 문서에서 동일한 `{speaker}-{code}`였습니다. 즉 에피소드 3의 `@purser{code="0020"}` 줄도 `purser-0020`이었고, 서로 다른 두 줄이 하나의 음성 아티팩트를 공유했습니다(0.21.1부터는 이를 `E-DUP-VOICEKEY`로 거부하기 시작했습니다). prefix를 사용하면 프로젝트 전체에서 기본값이 고유해집니다. 이전 키로 녹음한 오디오가 있는 프로젝트는 `lute.project.yaml`에서 이전 템플릿을 고정해 계속 사용할 수 있습니다.

```yaml
identity:
  voiceKey: "{speaker}-{code}"
```

이렇게 고정하면 하나의 키 아래 서로 다른 내용을 말하는 두 줄은 여전히 빌드가 거부됩니다. 프로젝트 전체 `lute check`와 프로젝트 전체 `lute compile`(문서 피연산자 없음, `-o <DIR>`)은 키에 해당하는 모든 줄을 이름 붙여 `E-DUP-VOICEKEY`를 보고합니다. 이미 `voiceKey: "{prefix}.{speaker}-{code}"`를 선언한 프로젝트는 변경 없이 컴파일되며 고정을 제거할 수 있습니다.

[컴포넌트](/ko/language/components-and-extends/#줄-식별)에서 확장된 줄은 자체 범위를 얻습니다(0.22.0). `::use` 확장 내부에서 `{prefix}`는 `{prefix}.{component}#{n}`이며, `n`은 호스트가 해당 컴포넌트를 사용한 횟수입니다. 따라서 `harbor.dinner`의 두 번째 `::use{component="toast"}`는 그 안의 `@mira{code="0010"}` 줄을 `harbor.dinner.toast#2.mira_0010`으로 컴파일하고, 기본 `voiceKey`는 `harbor.dinner.toast#2.mira-0010`이 됩니다. `{prefix}`가 없는 고정 템플릿은 `voiceKey`에서 이 범위를 제거하므로 컴포넌트 줄이 호스트 자체의 `@mira{code="0010"}`와 `mira-0010`을 공유합니다.

두 템플릿은 **말하는 콘텐츠 줄만** 제어합니다. 아티팩트에 있는 다른 두 id도 `lineId`/`titleLineId`라 불리지만 템플릿을 적용하지 않습니다.

- branch 선택지의 `lineId`는 항상 `{prefix}.{branch id}.{choice id}`입니다.
- 퀘스트의 `titleLineId`는 항상 `{quest id}.title`이고, 목표의 것은 `{quest id}.{objective id}`입니다. 퀘스트에는 `{prefix}`가 전혀 없습니다.

토큰 집합은 닫혀 있습니다 — `{prefix}`, `{speaker}`, `{code}` — 잘못된 토큰은 무엇이든 `E-IDENTITY-TEMPLATE`에 열거됩니다.

<!-- lute-diagnostics -->
```
lute: E-IDENTITY-TEMPLATE: unknown token `{character}` in identity template `lineId`; valid tokens are {prefix}, {speaker}, {code}
```

## 프로필과 플러그인

Lute의 작성 어휘 — 어떤 디렉티브, 속성, enum, 이벤트가 적법한지 — 는 **capability 프로필**에서 해석됩니다.

- **`profile`**은 루트 수준 capability 선택기입니다. 프로젝트의 `lute.project.yaml`에 선언된 프로필 중 하나의 이름을 지정합니다. 없으면 프로젝트의 `defaultProfile`이 적용됩니다. 예약된 프로필 이름 `global`은 다른 모든 프로필이 상속합니다.
- **`plugins`**는 선택된 프로필 위에 씬 로컬 플러그인 활성화와 옵션을 추가합니다. 플러그인 id를 타입이 지정된 옵션 객체에 매핑하며, id가 존재하면 해당 씬에서 플러그인이 활성화됩니다. 빈 객체는 "기본값으로 활성"을 뜻하고, 축약형 `true`도 동일하게 정규화됩니다. 맵도 `true`도 아닌 값은 정적 오류(`E-PROFILE-PLUGIN-VALUE`)입니다.

```yaml
profile: showcase
plugins:
  showcase.pack:
    resultScope: scene
    allowedKinds: [rhythm]
```

유효 capability는 결정적으로 해석됩니다: `lute.core` → `global` → 선택한 프로필의 `extends` 체인(부모 먼저) → 선택한 프로필 → 씬 로컬 `plugins` → 의존성 폐쇄. 활성화는 순전히 가산적입니다. 검사기, LSP, 컴파일러는 모두 같은 해석된 capability 스냅숏에 대해 문서를 검사하므로, 검사에 통과하는 것이 그대로 컴파일되는 것과 일치합니다.

## 플러그인 frontmatter 키

플러그인의 `frontmatter/*.yaml` export는 문서가 가질 수 있는 키를 `schema`와 함께 선언합니다([플러그인 개념](/ko/plugins/concepts/) 참고). schema에 맞지 않는 값은 `E-FRONTMATTER-SCHEMA`입니다.

0.53부터 참조는 키의 최상위뿐 아니라 값 **어디에 있든** 검사합니다. `entity`, `domain`, `enumFromOption`, `providerRef`, `assetKind`로 타입이 정해진 키는 그 타입의 속성과 똑같이 검사되고, `list`·`record` 필드·`map`의 키나 값 안에 있는 같은 타입의 값도 깊이에 상관없이 똑같이 검사됩니다. 진단은 속성 검사가 내는 코드(`E-BAD-ENUM`, `E-UNKNOWN-ID`, `E-ASSET-*`)를 그대로 쓰고, 메시지에 **필드 경로**를 적습니다. 아래 출력처럼 record의 필드, list의 항목, map의 키와 항목이 경로에 나옵니다. 위치는 키가 있는 줄입니다. 선언에 없는 `record` 필드와 참조가 아닌 타입의 필드는 참조로 취급하지 않습니다. schema로 표현할 수 없는 값(`null`, `!tag`, 지원하지 않는 타입의 map 키)과 모양이 맞지 않는 값은 그대로 `E-FRONTMATTER-SCHEMA`입니다.

```yaml
# plugins/acme.stage/frontmatter/keys.yaml
frontmatter:
  - { key: lead, schema: { entity: person } }
  - { key: guests, schema: { list: { entity: person } } }
  - key: talk
    schema:
      record:
        - { name: partner, type: { entity: person } }
        - { name: turns, type: int }
  - key: mood
    schema:
      map: { key: { entity: person }, value: { list: { entity: person } } }
```

`person: { members: [mira, theo] }`가 선언되어 있을 때, 씬이 각각을 잘못 쓰면:

```yaml
lead: mirra
guests: [mira, theoo]
talk: { partner: mirra, turns: 3 }
mood:
  mira: [theo]
  theoo: [mirra]
```

<!-- lute-diagnostics -->
```
./scenes/intro.lute:4:1: error [E-BAD-ENUM] `mirra` is not a member of entity kind `person` (attribute `lead` of `frontmatter`) — did you mean `mira`?
./scenes/intro.lute:5:1: error [E-BAD-ENUM] `theoo` is not a member of entity kind `person` (attribute `guests` of `frontmatter`) — did you mean `theo`?
```

<!-- lute-diagnostics unverified="the field, map key and map entry path forms are not in the doc matcher's message templates; verbatim lute check output" -->
```
./scenes/intro.lute:6:1: error [E-BAD-ENUM] `mirra` is not a member of entity kind `person` (field `partner` of `talk` of `frontmatter`) — did you mean `mira`?
./scenes/intro.lute:7:1: error [E-BAD-ENUM] `theoo` is not a member of entity kind `person` (key `theoo` of `mood` of `frontmatter`) — did you mean `theo`?
./scenes/intro.lute:7:1: error [E-BAD-ENUM] `mirra` is not a member of entity kind `person` (item 1 of entry `theoo` of `mood` of `frontmatter`) — did you mean `mira`?
```

플러그인 frontmatter 타입은 `W-DOMAIN-UNREAD`의 **읽는 쪽**으로도 셉니다. frontmatter 키 타입이(깊이에 상관없이) 이름을 든 엔티티 종류나 enum은 읽히는 것으로 보므로 더 이상 경고가 나지 않습니다. 그 타입을 쓰는 frontmatter 키가 없으면 경고는 찾아본 읽는 쪽 목록에 "no plugin frontmatter key is typed by it"을 포함합니다:

<!-- lute-diagnostics -->
```
./schema/world.schema.yaml:2:3: warning [W-DOMAIN-UNREAD] domain `person` is declared but no active construct reads it: no directive attribute, content-line slot, or state path is typed `{ domain: person }`, no plugin frontmatter key is typed by it, no `relations:` entry takes it as an argument, no `per: person` state family or `subsetOf: person` kind builds on it, and no rule or condition queries `person(…)` — so it enforces nothing and only reaches the artifact's `enums` array. Read it, or remove the declaration
```
