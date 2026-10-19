---
title: 여러 작성자, 하나의 프로젝트
description: "리드 한 명과 여러 영역 작성자가 하나의 Lute 프로젝트를 공유하는 방법: 디렉터리별 소유권, add:로 조립하는 종류, 기본값, ID 규칙, 계약 비트, 인계 플레이, 그리고 충돌을 찾아내는 병합 게이트를 설명합니다."
---

어떤 규모의 게임이든 한 사람이 작성하지는 않습니다. 이 가이드는 **리드** 한 명이 세계를 소유하고, 여러
**영역 작성자**가 각각 맵의 한 부분을 소유하며, 모두 하나의 Lute 프로젝트와 하나의 git 저장소 안에서
작업하는 경우를 다룹니다. 작성자들이 병렬로 작업할 수 있게 해 주는 규칙, 이를 지원하는 언어 기능,
그리고 병합하기 전에 브랜치가 통과해야 하는 명령을 설명합니다.

여기의 모든 내용은 실제 작업 라운드에서 나온 것입니다. 리드 한 명과 영역 작성자 네 명이 동시에 작성한
몬스터 수집 RPG(비트 818개, 정적 팩트 211개, 문서 약 130개)입니다. 언어가 dsl 0.26.0에서 얻은
기능 대부분이 이 작업 때문에 존재합니다. [`add:`](#여러-파일에-걸쳐-조립하는-종류-add)를 사용해 여러
파일에 걸쳐 조립하는 종류, [`defaults:`](#모든-문서가-상속하는-기본값)의 glob 임포트와 기본
퀘스트 티어, [`E-STATE-DECL-CONFLICT`](#상태-공유-경로는-한-번-선언합니다), 컴포넌트의
[`@@who:`](#공유-컴포넌트), [`advance: { to }`](#시계-핸드오프), 그리고
[`lute refs`](#병합-게이트)가 여기에 포함됩니다.

## 형태: 세계 계약과 영역

프로젝트를 두 종류의 파일로 나눕니다.

- **세계 계약**은 리드에 속합니다. `lute.project.yaml`, 공유 스키마(상태, 관계, 규칙, 정의(def), 엔터티
  종류), 엔진 플러그인(계기, 이벤트, 보상 종류, 디렉티브, 브리지 호출), 공유 컴포넌트, 그리고 영역을
  연결하는 **스파인**(영역을 연결하는 문서와 플레이)이 여기에 해당합니다.
- **영역**은 한 명의 작성자에게 속합니다. 해당 영역의 씬, 로어, 퀘스트, 테스트, 스파인 플레이용
  steps 파일, 자체 스키마, 자체 캐스트 파일입니다.

경계는 **공유 파일 안의 블록이 아니라, 항상 디렉터리별로** 정합니다. 같은 파일을 편집하지 않는 두 작성자는
병합 충돌을 일으키지 않으며, 리뷰어는 경로만 보고 브랜치가 자기 영역을 지켰는지 알 수 있습니다.

```
lute.project.yaml                     lead
schema/world.schema.yaml              lead   shared state, relations, rules, defs
schema/roster.schema.yaml             lead   the kinds (person, trainer, place) and the lead's members
schema/areas/<area>.schema.yaml       area   the area's members (add:), private state, defs
plugins/<engine>/**                   lead   occasions, events, reward kinds, directives, bridge
plugins/<engine>/cast/<area>.yaml     area   the area's named characters
components/**                         lead   shared components
scenes/spine/ lore/spine/ quests/spine/ tests/spine/        lead
scenes/<area>/ lore/<area>/ quests/<area>/ tests/<area>/    area
plays/spine.play.yaml                 lead   includes every area's steps file, in map order
plays/steps/<area>.steps.yaml         area   keep the last step's expect: green
plays/<area>-*.play.yaml              area   the area's own playthroughs
```

이 표를 매니페스트 옆의 `AREAS.md`에 저장소의 일부로 기록하고, 각 영역이 사용할 수 있는 공유 id와
각 영역이 다음 영역에 넘기는 내용을 함께 적습니다. Lute는 누가 파일을 소유하는지 알지 못하므로,
그 판단은 이 표와 코드 리뷰가 담당합니다.

## 모든 문서가 상속하는 기본값

문서마다 반복해야 할 내용은 모두 매니페스트의 `defaults:`에 넣습니다. 그러면 새 영역 파일에는
`kind:`, `id:`, `title:` 외의 프론트매터가 필요 없습니다.

```yaml
# lute.project.yaml (lead)
pluginsDir: plugins/
defaultProfile: harbor
profiles:
  harbor:
    plugins: { harbor.engine: true }
defaults:
  luteVersion: "0.30.0"
  uses:
    - schema/world.schema.yaml
    - schema/roster.schema.yaml
    - schema/areas/*.schema.yaml    # one file per area; a new area needs no edit here
  questTier: run                    # every <quest> that writes no tier= is a run quest
  components:
    - components/obtain.component.lute
    - components/supply.component.lute
```

- **`uses:`의 glob** (dsl 0.26.0 §2.4). 항목은 glob(`*`, `**`)일 수 있으며, 경로 순서대로 확장됩니다.
  아무것도 일치하지 않는 glob은 디렉터리가 비어 있거나 아직 존재하지 않아도 아무것도 임포트하지 않으며
  오류도 아닙니다(git은 빈 디렉터리를 유지하지 않습니다). 따라서 리드는 첫 영역 스키마가 들어오기 전에
  glob을 추가할 수 있습니다. 리터럴 경로는 여전히 존재해야 합니다.
- **`questTier: run | user`**는 값을 쓰지 않는 모든 `<quest>`의 `tier=`를 설정합니다. 퀘스트는 자체
  값을 여전히 쓸 수 있으며, 서브퀘스트의 tier는 부모의 tier와 같아야 합니다(`E-QUEST-TIER-MIX`).
- **영역 문서에는 절대로 `uses:`나 `components:`를 쓰지 않습니다.** 어떤 문서가 키를 쓰면 해당 키의
  기본값을 병합 없이 통째로 대체합니다. 따라서 자체 `uses:`를 가진 문서 하나가 전체 세계 계약을
  조용히 잃게 됩니다.
- **`chapters:`도 리드의 것입니다.** 매니페스트의 `chapters:`는 하나의 계기에 답하는 씬을 순서대로
  배치하며, 계기마다 하나의 체인을 둡니다. 같은 계기에 두 번째 체인을 두면 `E-CHAPTERS`입니다.
  자체적으로 순서가 있는 씬 실행이 필요한 영역은 리드에게 자체 계기의 체인을 요청하거나, 자신이
  소유한 씬에 `after:`를 사용해 순서를 정합니다.

## 여러 파일에 걸쳐 조립하는 종류: `add:`

엔터티 종류(`person`, `place`, `trainer` 하위 종류)는 공유 어휘입니다. 계기의 대상, 관계의 인수,
타입이 지정된 디렉티브 속성이 모두 이를 읽습니다. 0.26 이전에는 종류를 선언한 하나의 파일에 모든
멤버를 나열해야 했으므로, 모든 영역이 리드의 roster를 편집했습니다. dsl 0.26.0 §2.3부터는
**정확히 하나의** 스키마가 종류를 선언하고, 임포트된 다른 스키마는 `add:`로 그 종류에 멤버를
추가합니다.

```yaml
# schema/roster.schema.yaml (lead): the kinds, the spine's members, the contract ids
entities:
  person:
    members:
      - mira
      - keeper        # contract, written by north
  trainer:
    subsetOf: person
    members: [r3YoungsterAda]
  guard:
    subsetOf: person
    members: [northGuard1]
  place:
    members:
      - dock
      - lighthouse    # contract, written by north
  keyItem:
    members: [lampOil]
    labels: { lampOil: lamp oil }
```

```yaml unverified="an area schema: its add: lists extend kinds that schema/roster.schema.yaml declares, so it checks only inside its project (whole-project check clean there)"
# schema/areas/north.schema.yaml (area north)
state:
  run.northFogLifted: { type: bool, default: false }
entities:
  person:  { add: [northGull] }
  guard:   { add: [northGuard2] }
  place:   { add: [northCliff] }
  keyItem: { add: [fogHorn], labels: { fogHorn: fog horn } }
```

- **하위 종류의 멤버는 부모의 멤버이기도 합니다.** 위의 `northGuard2`는 `guard`에만 추가되지만,
  `guard`가 `subsetOf: person`이므로 `person`이기도 합니다. 따라서
  `talk@npc.northGuard2`와 `spotted@guard.northGuard2`가 모두 작동합니다. `person`에도 함께
  나열하는 것은 허용되며 중복으로 취급되지 않습니다. (0.26 이전에는 부모에 없는 하위 종류 멤버가
  오류였지만, 해당 `E-ENTITY-KIND-SHAPE` 원인은 사라졌습니다.)
- **멤버를 두 번 나열하면 오류입니다.** 하나의 목록에 두 번 나열하거나 두 영역 파일 각각에서 한 번씩
  나열하는 경우 모두 해당하며, `E-ENTITY-KIND-SHAPE`가 두 파일과 줄을 모두 표시합니다(dsl 0.26.0
  §2.2). 이 규칙 덕분에 두 영역이 `youngsterAda`라는 트레이너 하나를 몰래 공유하는 일이 없습니다.
- **`add:`에는 기반이 필요합니다.** 어떤 임포트도 선언하지 않은 종류에 `add:`를 쓰면 did-you-mean과
  함께 `E-ENTITY-KIND-SHAPE`가 발생합니다. 같은 엔트리에서 `members:` / `open:`과 나란히 두거나
  `open:` 종류에 `add:`를 쓰는 경우도 마찬가지입니다.
- **레이블은 멤버와 함께 이동합니다.** 종류의 `labels:`는 각 멤버의 표시 텍스트를 제공하며, 영역은
  추가하는 멤버의 레이블을 `add:` 옆에 씁니다(위의 `fogHorn: fog horn`). 텍스트는 멤버를 이름으로
  표시하는 곳, 즉 `{{occasion.target}}` 및 종류로 타입이 지정된 컴포넌트 인수([계약 비트](#계약-비트) 참조)에서
  레이블을 표시합니다.
- **어떤 영역이든 어떤 종류에나 추가할 수 있습니다.** `add:`는 임포트가 선언한 모든 종류에 작동하며,
  다른 영역이 선언한 하위 종류도 포함합니다. 검사기는 어떤 영역이 어떤 종류를 소유하는지 알지
  못하므로, 그 내용은 `AREAS.md`에 기록합니다.

다른 스키마 오류와 마찬가지로 whole-project `lute check`는 이러한 오류를 해당 스키마 줄 끝에서 프로젝트 전체의
한 줄로 한 번 보고합니다(`(imported by N documents)`). 각 문서가 해당 스키마를 임포트할 때마다
반복해서 보고하지 않으며, 임포트하는 문서 자체는 `ok` 상태로 유지됩니다.

<!-- lute-diagnostics -->
```
schema/areas/south.schema.yaml:2:29: error [E-ENTITY-KIND-SHAPE] entity kind `person` lists `northGull` twice — in the `add:` of `schema/areas/north.schema.yaml` (line 5) and in the `add:` of `schema/areas/south.schema.yaml` (line 2); list each member once
```

영역 전용 선언은 영역 스키마에 두고, 각 이름에 영역 접두사를 붙입니다
(`run.northFogLifted`, `northFogDay`라는 정의(def), `northRumor`라는 관계). glob은 모든 영역 스키마를
모든 문서에 임포트하므로, 두 영역이 같은 이름을 선언하면 충돌합니다. `E-USES-DUP-STATE`,
`E-USES-DUP-DEF`, `E-USES-DUP-RELATION`이 각각 스키마 줄에서 한 번 보고됩니다.
## 상태: 공유 경로는 한 번 선언합니다

두 영역이 모두 읽거나 쓰는 경로는 스키마에 한 번 선언해야 합니다. 두 문서가 각자의 프런트매터 `state:`에
같은 경로를 선언하면 whole-project `lute check`가 선언을 비교합니다. 하나의 런타임 값을 공유하므로 `type`, `default`,
`per`, `owner`가 일치해야 합니다. 일치하지 않으면 **`E-STATE-DECL-CONFLICT`**(dsl 0.26.0 §2.1)가 발생하며
두 파일과 줄을 표시하고, `lute play`는 한 경로를 두 타입으로 선언한 프로젝트를 거부합니다.
`scene.*` 경로는 씬 로컬이므로 예외이며, `extends:`로 재정의한 기본값을 지정해 세부화한 선언은 충돌이 아닙니다.

<!-- lute-diagnostics -->
```
./scenes/spine/dock.lute:6:3: error [E-STATE-DECL-CONFLICT] state path `run.tide` is declared as bool, default false at `./scenes/north/lighthouse.lute:6` but as int, default 0 at `./scenes/spine/dock.lute:6`; every declaration of one path must agree on type, default, per and owner — they share one runtime value (declare it once in a schema both documents import)
```

## ID: 프로젝트 전체에서 접두사를 붙입니다

Lute 프로젝트의 대부분 ID는 프로젝트 전체에서 사용되므로, 같은 ID를 선택한 두 영역이 충돌합니다. 영역을
접두사로 붙입니다:

| 항목 | 규칙 | 예시 |
|---|---|---|
| 씬 ID | `<area>.<camelCase>`, `scenes/<area>/`에 파일 배치 | `scenes/north/lighthouse.lute`의 `north.lighthouse` |
| 로어 문서 ID | `<area>.<topic>` | `north.guards` |
| 번들 비트 | camelCase; 표준 ID는 `<doc id>.<beat>` | `north.guards.anyGuard` |
| 엔트리 ID | `<area><Topic>`, 프로젝트에서 고유 | `northGullAfterStorm` |
| 퀘스트 ID | `<area><Name>` | `northLostBuoy` |
| 브랜치 / 허브 ID | `<area><Name>` (플레이의 `choose:` 키는 프로젝트 전체에서 사용) | `northKeeperAsk` |
| `share=` 키 | `<area><Name>` | `northFogBell` |
| 엔티티 멤버 | 일반 인물은 `<placeOrRoute><Role><Name>` | `r3YoungsterAda`, `northGuard2` |
| 영역 상태, 정의(def), 관계 | 영역 접두사 | `run.northFogLifted`, `northFogDay` |

엔트리 ID에는 문서 접두사가 없으므로 충돌하기 가장 쉽습니다. 하나의 엔트리 ID를 선언한 두 문서는
`E-ENTRY-ID-DUP`입니다. dsl 0.26.0부터 플레이와 테스트에서는 엔트리를
`<document id>.<entry id>`(`north.guards.northGullAfterStorm`)로 이름 붙일 수 있어 플레이를 읽기 쉽게 만들지만,
엔트리 ID 자체는 여전히 고유해야 합니다. 하나의 `<branch>` 또는 `<hub>` ID를 가진 두 문서는
whole-project `lute check`에서 `W-BRANCH-ID-SHARED`를 발생시킵니다. 하나의 `choose:` 키가 두 메뉴에 답하게 되기 때문입니다.

## 표시 이름: 영역마다 캐스트 파일 하나를 둡니다

각 영역에 엔진 플러그인 아래 자체 캐스트 파일을 제공하고, 캐릭터의 표시 이름을 작성하는 유일한 장소로
캐스트를 사용합니다. `speaker` 파라미터로 말하는 공유 컴포넌트(`@@who:`, [아래](#공유-컴포넌트))는
`name=` 인자 없이도 올바른 이름을 표시합니다.

```yaml
# plugins/harbor.engine/cast/north.yaml (area north)
cast:
  northGull:   { name: Old Gull }
  northGuard1: { name: Tower Guard, sharedName: true }
  northGuard2: { name: Tower Guard, sharedName: true }
```

네 영역이 사람의 이름을 지정하면, 그중 두 영역이 서로 다른 사람에게 같은 표시 이름을 선택하는 일이 조만간
생깁니다. whole-project `lute check`는 두 캐스트 엔트리의 `name:`이 같거나, 캐스트 이름이
`::use{… name="…"}` 표시 문자열과 같거나, 두 문자열이 서로 다른 화자의 이름을 지정할 때
**`W-DISPLAY-NAME-DUP`**(dsl 0.26.0 §2.8, 권고; `lute lint`에서도)을 경고합니다. "Tower Guard" 또는
"Eclipse Grunt" 같은 역할처럼 공유할 이름은 캐스트 엔트리에 `sharedName: true`를 표시하면 집계하지 않습니다.

두 영역 파일이 모두 선언한 캐스트 ID는 `E-PLUGIN-DUP-ID`이며 나중 선언 위치에서 보고됩니다. 첫 번째 선언이 사용됩니다:

<!-- lute-diagnostics unverified="byte-exact whole-project check output; the plugin loader prefixes the file position to a composed message, so no single format! literal holds it" -->
```
./plugins/harbor.engine/cast/south.yaml:3:3: error [E-PLUGIN-DUP-ID] cast `keeper` is already declared at cast/north.yaml:2; this declaration is ignored — rename or remove one
```

## 공유 컴포넌트

리드는 모든 영역이 사용하는 컴포넌트(트레이너 배틀, 키 아이템 전달, 상점)를 작성합니다. dsl 0.26.0부터
컴포넌트는 영역이 예전에 직접 복사하던 작업을 수행할 수 있습니다:

- **`@@who: …`**는 각 `::use`에서 `speaker` 파라미터 `who`가 지정한 캐스트 멤버의 목소리로 말합니다.
  초상화, 음성, 캐스트 검사가 직접 대사와 동일하게 적용됩니다.
- **파라미터에는 `default:`**(리터럴 또는 `@def`)를 지정할 수 있습니다. 인자를 생략하면 이를 사용합니다.
- **컴포넌트는 자체 디렉티브 결과를 읽습니다**(자체 `::battle` 이후의 `scene.battle.fight.won`). 따라서 호스트가
  결과를 다시 전달하지 않습니다.
- **`::use{… when="…"}`**는 전체 확장을 실행하거나, 아예 실행하지 않습니다.
- **엔티티 종류로 타입 지정한 파라미터**(`{ type: { entity: keyItem } }`) 또는 그렇게 타입 지정한 속성에 통째로
  전달한 값은 각 `::use`에서 did-you-mean 제안과 함께 검사됩니다.
- **`beat:` 헤더**는 컴포넌트를 [비트 템플릿](/ko/language/beats/#비트-템플릿-use)으로 만듭니다. 헤더(`on`,
  `target`, `once`, `spentBy`, …)를 `@param`과 함께 한 번 작성하고, 영역의 비트는 파라미터를 전달하는
  `<beat use=…>` 하나가 됩니다.

```lute unverified="an excerpt of a multi-file project: the trainerSpotted occasion, the ::battle bridge, run.money and the defeated relation come from its engine plugin and schemas"
---
component: trainerBattle
effects: true
params:
  who: speaker     # the trainer: a cast member, trainer.<who>, defeated(<who>)
  intro: string
  win: string
  lose: string
  prize: { type: int, default: 100 }
beat:
  on: trainerSpotted
  target: "trainer.@who"
  once: user                          # once beaten, gone for good
  spentBy: "holds('defeated', [@who])"
---

@@who: {{@intro}}
::battle{foe=@who kind="trainer" resultKey="fight"}
<match subject="scene.battle.fight.won">
  <when is="true">
    @@who: {{@win}}
    ::assert{defeated(@who)}
    ::set{run.money += @prize}
  </when>
  <otherwise>
    @@who: {{@lose}}
  </otherwise>
</match>
```

그러면 영역의 경로 트레이너는 로어 문서에서 한 줄이 되고, 대화 상자의 이름은 캐스트에서 가져옵니다:

```lute
<beat use="trainerBattle" id="r3YoungsterAda" who="r3YoungsterAda" intro="I like shorts!" win="Aww, my shorts are torn." lose="Shorts win again!"/>
```

헤더는 비트에 `on: trainerSpotted`, `target: trainer.r3YoungsterAda`, `spentBy: holds('defeated', ['r3YoungsterAda'])`를
제공하며, `lute beats`는 이를 그렇게 나열합니다. `spentBy`가 있는 비트는 제시되었다고 소진되지 않습니다.
여기서는 조건이 성립한 첫 승리 시 소진되며, `once: user`가 영구적으로 소진된 상태를 유지합니다. 패배한 배틀은
아무것도 소진하지 않으므로 트레이너가 재대결을 위해 남아 있습니다.

영역이 의존하는 공유 컴포넌트의 파라미터는 한 번 정하면 동결합니다. `default:`가 있는 파라미터를 추가하는
것은 안전하지만, 이름을 바꾸거나 제거하면 모든 영역이 동시에 깨집니다. 전체 규칙은
[컴포넌트](/ko/language/components-and-extends/)를 참조합니다.

## 계약 비트

스파인은 영역에 의존합니다. 피날레에서 램프를 밝히려면 등대가 그 전에 램프 오일을 전달해야 합니다. 이런
각 의존성을 **계약 비트**로 만듭니다. 리드가 먼저 해당 영역 디렉터리에 스텁으로 작성하며, 그때부터 영역이
이를 소유합니다:

```lute unverified="an excerpt of a multi-file project: the enterPlace occasion, the place kind and the obtain component come from its manifest and schemas"
---
kind: scene
id: north.lighthouse
title: The lighthouse
on: enterPlace
target: place.lighthouse
once: run
---

/* CONTRACT (seeded by the lead, owned by north): the id, occasion and target
   stay; it produces hasItem(lampOil). Rewrite the lines, add sections, branches
   and after: edges freely. */

## The keeper

@keeper: You came for the oil. Everyone does, eventually.
::use{component="obtain" item="lampOil" who="keeper"}
::set{run.northFogLifted = true}
```

계약은 비트의 **ID, 계기와 대상, 그리고 비트가 생성하는 팩트와 상태**입니다. 영역은 나머지를 모두 다시 쓸 수
있습니다. 스텁이 첫날부터 존재하므로 스파인의 퀘스트에는 필요한 각 팩트의 생성자가 있고, 영역이 아직 한 줄도
작성하지 않았을 때도 whole-project `lute check`가 통과합니다. 영역이 비트를 삭제하거나 팩트의 단언을 중단하면 다음 검사에서
스파인의 목표가 `E-OBJECTIVE-UNSATISFIABLE`이 됩니다. 검사기가 계약을 강제하기 때문입니다.

`obtain`은 `item: { type: { entity: keyItem } }`을 받고 `You got the {{@item}}.`이라고 말합니다. 종류로 타입이
지정된 인자는 해당 종류의 레이블을 렌더링하므로 대사는 "You got the lamp oil."로 표시됩니다. 표시 텍스트는
각 사용처에서 `label=` 인자로 작성하는 대신 로스터의 `labels:`에 한 번 작성합니다.

**계약 비트 또는 `--wip`.** 계약 비트가 아직 하나도 없을 때 스파인을 초안 작성하는 리드는 대신
`lute check --project . --wip`로 검사할 수 있습니다. `E-OBJECTIVE-UNSATISFIABLE`, `E-BEAT-UNREACHABLE`,
`E-ENTRY-UNREACHABLE`, `E-ARM-DEAD`는 아직 아무것도 생성하지 않는 관계 때문에 발생한 경우 경고가 됩니다.
또한 dsl 0.26.0 §2.6부터는 공유 컴포넌트가 바인딩되지 않은 파라미터로만 단언하는 관계도 포함됩니다
(`obtain`의 `::assert{hasItem(@item)}`에 아직 `::use`가 없는 경우). 생성자는 존재하지만 절대 일치하지 않는
관계는 계속 오류입니다. 스파인 브랜치에서는 `--wip`를 사용하고, 병합 게이트는 이를 제외하고 실행합니다.
## 플레이: 하나의 스파인, 영역별 하나의 steps 파일

스파인 플레이는 `include:` steps를 통해 영역별로 전체 게임을 진행합니다. 각 영역은 자신의
steps 파일을 소유하고 steps를 자유롭게 추가할 수 있지만, **마지막 step의 `expect:`**는 유지합니다. 이것이
다음 영역과 스파인이 의존하는 해당 영역의 **핸드오프**입니다.

```yaml
# plays/spine.play.yaml (lead)
steps:
  - occasion: enterPlace
    target: place.dock
    expect: { winner: spine.dock, quests: { lamp: active } }
  - include: steps/north.steps.yaml
  - occasion: enterPlace
    target: place.dock
    expect: { winner: spine.dock }
expect:
  transcriptContains: ["You found oil."]
```

```yaml
# plays/steps/north.steps.yaml (area north)
steps:
  - occasion: spotted
    target: guard.northGuard2
    expect: { winner: north.guards.anyGuard }
  - occasion: enterPlace
    target: place.lighthouse
    label: north hand-off
    expect:
      winner: north.lighthouse
      facts: [hasItem(lampOil)]
      state: { run.northFogLifted: true }
      quests: { lamp: complete }
```

포함되는 steps 파일의 규칙은 다음과 같습니다.

- `choose:`와 `bridges:`는 steps 파일의 최상위가 아니라 **step**에 배치합니다
  (`bridges: { battle: [ { won: true } ] }`). 최상위는 스파인 플레이가 소유합니다. 0.27.0부터 스파인은
  자신의 `include:` step에도 이를 배치할 수 있습니다 (`- include: steps/north.steps.yaml` 옆에
  `choose: { gate: open }`): 이는 include가 삽입하는 모든 step에 대해 스파인의 자체 최상위 목록보다 먼저
  스크립트를 적용하며, 세그먼트가 끝나면 제거됩니다. 서로 다른 선택으로 같은 영역 파일을 두 번 플레이할 때
  자연스러운 방식입니다. [루틴 공유하기](/ko/tooling/play/#시계-앞으로-돌리기)를 참조하세요.
- 승자는 표준 id로 이름을 지정합니다. 씬은 id로, 번들 비트는 `<doc>.<beat>`로, 엔트리는 id 또는
  `<doc>.<entry>`로 지정합니다.
- 문서 외부에서 엔진이 수락하는 accept 기반 퀘스트(`accept="external"`, 퀘스트 보드)는 저장 시드 대신
  `engine: { accept: [northLostBuoy] }` step으로 플레이 중간에 수락합니다.

### 시계 핸드오프

포함된 파일은 그 이전 영역들이 시계를 얼마나 이동했는지 알 수 없습니다. dsl 0.26.0부터 step은 시계를 횟수가
아니라 위치로 이동하며, 시계가 서 있는 상태를 나타냅니다.

```yaml
steps:
  - advance: { to: night }                                   # the next night, never backward
    label: the tower by night
    expect: { clock: { slot: night } }
  - advance: { to: { weekday: Friday, slot: morning } }      # the next Friday morning
    expect: { clock: { weekday: Friday, slot: morning } }
```

`advance: { to }`는 항상 현재 위치 이후의 다음 해당 위치로 앞으로 이동합니다(절대 0 step이 아님. 시계가 이미
그 위치에 있더라도 다음 위치로 갑니다). 이는 한 번의 advance입니다. 슬롯 계기는 시계가 멈추는 지점에서
한 번 발생하고, 통과하는 자정마다 `dayEnd`와 `dayStart`가 발생합니다. 중간 슬롯에 콘텐츠가 응답한다면 별도의
`advance: slot` step을 작성합니다. 한 step의 `expect: { clock: { weekday, slot, day } }`는 그 step 이후 시계가
서 있는 위치를 판단합니다. 영역의 첫 step(도착)과 핸드오프에 하나씩 배치하여, 이전 영역의 시계 변경이
비트를 조용히 적격 상태에서 멈추게 하는 대신 실패한 예상으로 드러나도록 합니다. [스토리 플레이하기](/ko/tooling/play/)를
참조하세요.

## 병합 게이트

영역이 브랜치를 리드에게 핸드오프하기 전에 프로젝트 루트에서 다음을 실행합니다.

```sh
lute check --project . --deny-warnings   # 0 errors, 0 warnings
lute test --project .                  # every test and every play with expect:, spine included
lute beats --project .                           # no W-BEAT-PRIORITY-TIE / W-BEAT-SHADOWED you did not intend
lute refs --project . --attr give.item --reward ITEM   # who gives what
```

- **`lute check --project . --deny-warnings`**는 모든 경고를 오류로 바꿉니다. 여러 작성자가 있는 프로젝트에서 용인된
  경고는 아무도 읽지 않는 경고가 됩니다. 0만이 안정적인 숫자입니다. 의도한 경고는 플래그가 아니라 발생 지점에서
  (`sharedName: true`, `priority=`) 침묵시킵니다.
- **`lute test`**는 모든 `*.test.yaml`과 `expect:`가 있는 모든 플레이를 실행하므로, 스파인 플레이와 각 영역의
  핸드오프가 모든 브랜치에서 실행됩니다. dsl 0.26.0부터 프로젝트를 한 번 로드하고 검사한 뒤 모든 코어에서
  테스트와 플레이를 병렬로 실행합니다(`RAYON_NUM_THREADS`를 따름). 818개 비트 프로젝트에서는 전체 스위트가
  약 100초에서 10초 미만으로 줄었습니다.
- **`lute beats`**는 모든 계기와 대상에 대해 어떤 비트가 응답하는지와 그 순서를 표시합니다. 두 영역이
  같은 `talk@npc.<id>` 또는 같은 fallback에 동일한 우선순위로 응답하면 여기에 표시되고,
  `W-BEAT-PRIORITY-TIE`로도 표시됩니다. 0.26부터는 두 `when`이 배타적이지 않은 이유를 보여줍니다. 더 높은
  우선순위의 비트가 항상 이기는 fallback은 `covered by <id>`로 나열됩니다.
- **`lute refs`**(dsl 0.26.0 §2.5)는 디렉티브 속성의 모든 값 또는 모든 보상 대상을 사용 문서와 줄 번호와
  함께 나열합니다. 컴포넌트를 통해 전달된 값도 포함합니다(`via component <name>`). 리드가 배포 전에 “두 영역이
  모두 Super Rod를 준다”는 사실을 확인하는 방법입니다.

```
$ lute refs --project . --attr give.item
::give.item: 2 value(s)
  `flare` — 1 use(s) in 1 document(s)
    scenes/south/pier.lute:13 (via component `supply`)
  `rope` — 1 use(s) in 1 document(s)
    scenes/south/pier.lute:12
```

자신이 소유한 파일만 커밋합니다. 리드의 리뷰는 도구가 실행할 수 없는 유일한 게이트입니다.

## 도구가 잡는 것과 잡지 못하는 것

| 충돌 | 감지 도구 |
|---|---|
| 두 영역이 종류에 같은 멤버를 추가함 | `E-ENTITY-KIND-SHAPE`, 두 파일과 줄 표시 |
| 어떤 임포트도 선언하지 않은 종류에 `add:`를 사용함(오타) | `E-ENTITY-KIND-SHAPE`, did-you-mean 제안 포함 |
| 두 문서가 하나의 상태 경로를 서로 다르게 선언함 | `E-STATE-DECL-CONFLICT` |
| 두 영역 스키마가 같은 상태, 정의 또는 관계 이름을 선언함 | `E-USES-DUP-STATE`, `E-USES-DUP-DEF`, `E-USES-DUP-RELATION` |
| 하나의 씬, 퀘스트 또는 엔트리 ID를 가진 문서가 둘임 | `E-CONN-EPISODE-ID-DUP`, `E-QUEST-ID-DUP`, `E-ENTRY-ID-DUP` |
| 두 캐스트 파일이 하나의 캐스트 ID를 선언함 | `E-PLUGIN-DUP-ID`, 나중 파일과 줄에서 표시 |
| 두 문서가 하나의 `<branch>` 또는 `<hub>`에 같은 ID를 부여함 | whole-project `lute check`의 `W-BRANCH-ID-SHARED` |
| 존재하지 않는 엔진 ID (`::give{item="flair"}`) | did-you-mean과 함께 `E-BAD-ENUM`; 속성이 `{ entity: K }`로 타입 지정된 경우 컴포넌트 param을 통해서도 감지 |
| 대상이 없는 보상 또는 엔진이 알지 못하는 대상 | 보상 종류가 `target:` 계약을 선언할 때 `E-REWARD-TARGET` |
| 두 화자가 같은 이름으로 표시됨 | `W-DISPLAY-NAME-DUP` (`sharedName: true`이면 제외) |
| 두 영역이 동일 우선순위에서 하나의 계기와 대상에 응답함 | `W-BEAT-PRIORITY-TIE`, `W-BEAT-SHADOWED`, `lute beats` |
| 영역이 계약 비트를 제거하거나 깨뜨림 | 스파인 퀘스트에 `E-OBJECTIVE-UNSATISFIABLE`, `lute test`의 핸드오프 `expect:` |
| 영역이 다른 영역이 의존하는 시계를 이동함 | steps 파일의 `expect: { clock: … }` |

Lute가 알지 못하는 사항은 다음과 같습니다.

- **누가 파일을 소유하는지.** 영역이 리드의 roster나 다른 영역의 씬을 편집해도 모든 게이트를 통과하며, 다른 영역이
  선언한 종류에 `add:`를 해도 마찬가지입니다. 소유권은 `AREAS.md` 표와 리뷰가 담당합니다(호스팅 서비스에서는
  `CODEOWNERS` 파일로 강제합니다).
- **비공개 상태.** `defaults.uses` glob은 모든 영역 스키마를 모든 문서로 임포트하므로, 한 영역이 다른 영역의
  `run.southFossil`을 읽을 수 있습니다. 접두사는 리뷰에서 이를 보이게 하지만, 거부하는 것은 없습니다.
- **어느 규칙이 무엇을 도출하는지.** 영역 스키마의 `rules:`는 모든 문서에 적용됩니다. south 스키마가 north의
  파생 관계에 추가한 규칙은 north의 가드를 변경하지만, 이를 거부하는 것은 없습니다. 각 규칙은 자신이
  도출하는 관계를 선언한 스키마에 유지합니다.
- **두 영역이 같은 아이템을 주는 것이 버그인지 여부.** `lute refs`가 이를 나열하지만, 판단은 여러분의 몫입니다.
- **타입이 지정되지 않은 ID.** 일반 `string` 속성의 엔진 ID나, 두 영역이 같은 `share=` 키를 선택하는 경우(그러면
  해당 비트들이 함께 소모됨)는 검사되지 않습니다. 엔터티 종류로 엔진 ID를 타입 지정하고 프로젝트 전체 ID마다
  접두사를 붙입니다.
- **문체와 어조.** 다른 영역이 작성한 캐릭터가 캐릭터답지 않게 말하는 것은 리뷰에서 찾아야 할 사항입니다.
