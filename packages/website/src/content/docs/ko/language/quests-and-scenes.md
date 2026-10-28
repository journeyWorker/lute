---
title: 퀘스트와 씬
description: 시간축 문서 종류 — after:로 순서를 정하는 씬, 목표와 파생 완료, 런·사용자·시즌 tier, 퀘스트를 다시 실행하는 rearm 조건, 생명주기 이벤트 반응을 갖는 퀘스트 — 그리고 조회하는 콘텐츠를 위한 lore 종류입니다.
---

모든 `.lute` 문서는 **`kind`**를 선언합니다: `scene`, `quest`, 또는 `lore`. kind는 필요한 프런트매터 키, 허용되는 문법, 식별 모델을 선택합니다. 씬은 한 줄씩 작성하는 플레이 가능한 에피소드이고, 퀘스트는 씬을 진행시키고 보상하는 완료까지 실행되는 목표 기계입니다. 둘 다 이야기의 시간축 위에 놓입니다. 세 번째 종류인 `lore`(dsl 0.19.0)는 플레이하는 대신 엔진이 조회하는 콘텐츠 — 아이템 설명, 발견한 쪽지, 짧은 대사 — 를 담으며, 자체 페이지인 [로어 엔트리](/ko/language/lore-entries/)가 있습니다.

## 씬과 `after:`

씬은 하나의 에피소드입니다 — 프로젝트 전체에서 씬을 이름 붙이는 프런트매터 `id:`와 그 섹션들로 이루어집니다. (이전 씬은 대신 `character`/`season`/`episode`로 이름을 정하고, 이름은 `{character}.{episodeId}`로 만들어집니다. 여전히 동작하지만 새 씬은 `id:`를 사용합니다.) 씬은 프런트매터 키 **`after:`**로 *순서를 정합니다*. 이 키는 검사기와 `lute scenario`가 이 씬에 도달한다고 가정하는 경로를 선언합니다. 이는 점프가 아니라 권고용 순서 메타데이터입니다. 씬이 실제로 차례로 플레이되게 하려면 [씬을 이야기로 잇기](/ko/getting-started/connect-scenes/)를 참고하세요. 이 키의 어휘는 `&&` / `||`로 결합하는 정확히 세 가지 술어입니다:

- `visited("<id>")` — 플레이어가 해당 `id:`의 씬을 본 뒤에는 참입니다.
- `completed("<questId>")` — 해당 퀘스트가 끝난 뒤에는 참입니다.
- `active("<questId>")` — 해당 퀘스트가 시작되었고 아직 해결되지 않은 동안 참입니다.

```yaml
after: 'visited("mira.s01ep01") && active("theCoffeeDebt")'
```

`active`는 0.8.0에 새로 추가되었으며, 비대칭을 해소합니다. 퀘스트 생명주기는 `unset` → `active` → `complete` | `failed`이지만, 사전 조건 어휘는 관찰 가능한 세 상태 중 두 가지만 이름 붙일 수 있었습니다 — "빚이 아직 남아 있는 동안 도달 가능"을 쓸 방법이 전혀 없었습니다. 도달 가능성 그래프에서 `active` 간선은 `completed` 간선과 **동일합니다**. 둘 다 "이 노드는 이 노드보다 먼저 도달 가능해야 한다"고 주장하므로 순환 및 도달 불가 분석은 바뀌지 않습니다. 그것이 전달하는 상태 엔벨로프는 **더 약합니다** — `completed(q)` 뒤에는 소비자가 `quest.q.state == complete`이라고 가정할 수 있지만, `active(q)` 뒤에는 퀘스트가 `active`에 도달했다는 것만 가정할 수 있습니다.

`after:`에는 부정, 산술, 상태 읽기가 없습니다. 이러한 선언이 도달 가능성 그래프를 구성하는 방식은 [씬 그래프와 after:](/ko/connectivity/scene-graph/)를 참고하세요. 0.21.0부터 `visited(…)`는 일반 조건 함수이기도 합니다 — 아래 [퀘스트가 씬 및 계기를 만나는 방식](#퀘스트가-씬-및-계기를-만나는-방식)을 참고하세요.

## quest 종류

퀘스트 문서는 `kind: quest`를 선언하고, 자신이 게이트하는 스키마를 위해 `uses:`를 담습니다. 본문은 하나 이상의 `<quest>` 선언입니다 — 퀘스트에서는 `<hub>`, `<timeline>`, `#`/`##` 헤딩을 금지합니다.

퀘스트는 `<on>` 핸들러 본문과 `<objective>` 본문만 실행하므로, 0.52부터(#208) `<quest>` 바로 안에는 이 둘과 `<reward>`만 올 수 있습니다. 퀘스트 최상위의 콘텐츠 줄, 디렉티브, `::set`, `::label`, `::jump`, `<branch>`, `<match>`, `::assert`, `::retract`는 `E-GRAMMAR-NOT-ADMITTED`입니다. 그 순간에 맞는 `<on event=…>`이나 `<objective>` 안으로 옮기세요:

<!-- lute-diagnostics -->
```
q.lute:6:3: error [E-GRAMMAR-NOT-ADMITTED] a content line at the top level of `<quest id="toll">` never runs — a quest runs only its `<on>` handlers and `<objective>` bodies: move it into the `<on event=…>` or `<objective>` whose moment it belongs to
```

0.51에서 이런 콘텐츠는 어떤 실행도 도달하지 않는 레코드로 컴파일되었습니다. 예외는 마지막 본문 뒤의 콘텐츠로, 그 본문의 일부처럼 이어서 실행되었습니다. 그 본문 안으로 옮기세요.

```lute
<quest id="rescueHalsinGrove" tier="run" title="Rescue the First Druid" start="run.act == 1" fail="run.npc.halsin.dead">
  <objective id="reachGrove" title="Reach the Emerald Grove" done="run.region == 'grove'"/>
  <objective id="freeHalsin" title="Free Halsin from the cage" done="run.npc.halsin.freed"/>

  <on event="questComplete">
    ::set{user.xp += 300}
    ::set{run.metHalsin = true}
    @narrator: Halsin rolled his shoulders and looked north. "Moonrise, then."
  </on>

  <on event="questFailed">
    ::set{run.groveOutcome = "halsinDead"}
    @narrator: The cage held only silence now.
  </on>
</quest>
```

*([`docs/examples/quest-grove.lute`](https://github.com/journeyWorker/lute/blob/main/docs/examples/quest-grove.lute)에서 가져왔습니다.)*

퀘스트 문서는 파일을 번들로 이름 붙이는 문서 **`id:`**도 선언할 수 있습니다(dsl 0.19.0 §2.1) — `haven.mainChain`처럼 위에서 아래로 작성한 퀘스트 연쇄입니다. 씬의 `id:`와 같은 형태 규칙(`E-META-ID`)을 따르고, 아티팩트의 `meta.id`와 `project.index.json`에서 문서의 키가 되며, 씬 및 로어 문서 id와 프로젝트 전체 네임스페이스를 공유합니다(`E-CONN-EPISODE-ID-DUP`). 선택 사항입니다. 이것이 없으면 문서는 이전과 정확히 같이 첫 번째로 선언된 퀘스트 id를 키로 사용합니다.

```yaml
kind: quest
id: haven.mainChain
uses: ../world.schema.yaml
```

### `<quest>`

`<quest id>`에는 프로젝트에서 유일하고 CEL에서 안전한 `id`가 필요합니다(`quest.<id>.*` 상태 tier의 키가 됩니다). 선택적 술어는 CEL 문자열입니다: **`start`**는 성립할 때 퀘스트를 `unset` → `active`로 전환하고, **`fail`**은 `active` → `failed`로 전환합니다. `fail`은 완료보다 우선합니다 — 같은 상태에서 둘 다 성립하면 퀘스트는 실패합니다(결정적인 우선 처리).

**`tier`**(dsl 0.22.0)는 퀘스트 결과가 얼마나 오래 지속되는지 말합니다. 기본값인 `tier="user"`는 상태를 런 사이에 유지하며, 0.22.0 이전에는 모든 퀘스트가 그랬습니다. `tier="run"`은 상태를 초기화합니다: 새 런이 시작되면 퀘스트 상태가 `unset`으로, 목표가 미완료로 돌아갑니다. 그러면 로그라이크의 런별 목표는 전체 세이브를 추적하는 사용자 tier 퀘스트와 나란히 매 런 새로 시작되고, 완료되거나 실패합니다:

```lute check
---
kind: quest
title: The climb
state:
  run.floor: { type: int, default: 0 }
  user.bestFloor: { type: int, default: 0 }
---

<quest id="climb" title="Reach the tenth floor" start="true" tier="run">
  <objective id="top" title="Reach floor ten" done="run.floor >= 10"/>
  <on event="questComplete">
    @narrator: The wind at the top is colder than you expected.
  </on>
</quest>

<quest id="legend" title="Become a legend" start="true">
  <objective id="deep" title="Reach floor ten in any run" done="user.bestFloor >= 10"/>
</quest>
```

초기화 뒤 생명주기는 평소처럼 정착합니다: `climb`의 `start`가 다시 성립하므로 모든 런의 시작에 재활성화되지만, 수락으로 시작하는 run-tier 퀘스트는 다시 수락될 때까지 `unset`으로 남습니다. `legend`는 런 사이에 상태를 유지합니다. tier는 문서가 아니라 각 `<quest>`에 속하므로 하나의 퀘스트 문서에 두 tier를 섞을 수 있습니다. `run`, `user`, 또는 `season:<name>`([아래](#돌아오는-퀘스트-시즌-tier와-rearm) 참고) 이외의 값은 모두 `E-ATTR-TYPE`입니다. [서브퀘스트](#서브퀘스트)는 예외로, 부모의 tier를 공유해야 합니다(`E-QUEST-TIER-MIX`). [`lute play`](/ko/tooling/play/)는 모든 `newRun` 스텝에서 초기화를 수행하므로 플레이 스크립트가 여러 런을 걷고 각 런을 단언할 수 있습니다.

모든 퀘스트가 run 퀘스트인 게임은 `lute.project.yaml`에서 한 번만 그렇게 지정합니다(dsl 0.26.0 §2.4):

```yaml
defaults:
  questTier: run
```

`tier=`를 쓰지 않는 모든 `<quest>`는 `run`을 취하고, `tier="user"`를 쓰는 퀘스트는 이를 유지합니다. `questTier`에는 `run`, `user`, 또는 `season:<name>`을 쓸 수 있습니다. 지정하지 않으면 기본값은 계속 `user`입니다. [프로젝트 기본값](/ko/language/imports/#프로젝트-기본값)을 참고하세요.

퀘스트는 프런트매터 키가 아니라 요소의 **속성** `follows=`로 시나리오 그래프에서 자신의 위치를 선언합니다 — 이 페이지의 첫 헤딩인 "씬과 `after:`"가 적용되지 않는 유일한 곳입니다:

```lute
<quest id="manifestGap" tier="run" title="The Manifest Gap" start="true" follows="visited('haven.s01ep06') && completed('whoWakes')">
  <objective id="reconcile" title="Reconcile the count" done="true"/>
</quest>
```

`follows=`는 **그래프 메타데이터일 뿐**입니다: `lute scenario`가 그리는 간선을 기록하며 퀘스트를 게이트하지 않습니다. 위 퀘스트는 `start`가 성립하는 즉시 활성화되고 `haven.s01ep06`이 플레이되었는지는 신경 쓰지 않습니다. 퀘스트가 씬을 기다리게 하려면 조건을 `start`에 넣으세요: `start="visited('haven.s01ep06')"`. (`0.28.0` 이전에는 이 속성을 `after=`라고 썼지만, 이제는 `E-UNKNOWN-ATTR`이며 메시지가 `follows=`를 알려 줍니다.)

`follows=`는 씬의 `after:` 키와 같은 제한된 사전 조건 프로필을 받습니다 — `visited("id")`, `completed("id")`, `active("id")`, `&&`, `||`뿐이며 다른 것은 없습니다(`E-CONN-PROFILE`). 퀘스트 *프런트매터*에 `after:` 또는 `follows:`를 쓰면 `E-META-UNKNOWN-KEY`이고, 진단은 속성 형태를 알려 줍니다:

<!-- lute-diagnostics -->
```
error [E-META-UNKNOWN-KEY] unknown top-level meta key `after` (not a core key and not owned by an active plugin) — a quest's graph edge is the `follows=` ATTRIBUTE on its `<quest>` element, not a frontmatter key; it does not gate the quest (to wait, write `start="visited('<scene id>')"`)
```

`follows=`(또는 씬의 `after:`) 안의 `visited()`는 [번들 비트](/ko/language/beats/#비트-번들)를 canonical id로 이름 붙일 수도 있습니다. `follows="visited('talks.maud')"`처럼 씁니다(dsl 0.24.0). 0.24.0 이전에는 이것이 `E-CONN-UNKNOWN-NODE`, "a bundle beat, not a scene"였습니다.

수락으로 시작하는 퀘스트(`start` 없음, [아래](#퀘스트가-씬-및-계기를-만나는-방식) 참고)는 보통 `follows=`가 전혀 필요 없습니다. 이것이 없으면 `::accept`하는 모든 씬, 번들 비트, 퀘스트 본문에 고정됩니다: `lute scenario`가 각각에서 `accept` 간선을 그립니다(`beat(talks.maud) -> quest(salvage) [accept]`). 따라서 퀘스트는 더 이상 고정되지 않은 것으로 세지 않습니다.

두 가지 앵커 종류도 `follows=`가 필요 없습니다(dsl 0.25.0 §4):

- **서브퀘스트.** 모든 중첩 퀘스트는 부모에 매달립니다: `quest(relight) -> quest(oil) [subquest]`. 자식이 있는 부모는 더 이상 고정되지 않은 것으로 나열되지 않으며 자식도 마찬가지입니다.
- **`start` 결합항.** `follows=`가 없는 퀘스트는 `visited('…')`, `entry.X.everRead`, 또는 `quest.Y.state == '…'`(어떤 상태든 `unset` 이외의 상태)를 읽는 `start`의 최상위 `&&` 결합항으로 고정됩니다. `start="entry.keeperLog.everRead && visited('arrival')"`은 `entry(keeperLog) -> quest(relight) [start]`와 `scene(arrival) -> quest(relight) [start]`를 그립니다. 엔트리는 `entry(<id>)` 노드로 그래프에 참여합니다. 이런 읽기의 `||`는 여러 소스를 가진 하나의 앵커입니다. 그러므로 `start` 기반 퀘스트에는 `start`를 `follows=`에 복사할 필요가 없습니다.

`lute scenario --project <DIR> reach --node quest:<id>`는 퀘스트의 앵커를 나열합니다(`--json`에서는 `anchors`). 앵커는 퀘스트가 도달 불가능하다는 것을 증명하지 않으며, 순환을 닫을 앵커는 그리지 않습니다. 명시적인 `follows=`가 있으면 `start` 앵커 대신 그것이 선언한 간선만 유지됩니다. 서브퀘스트 간선은 남고, `accept` 앵커도 남습니다. `follows=`는 수락을 게이트하지 않기 때문입니다([수락 기반 퀘스트로 들어오는 경로](#수락-기반-퀘스트로-들어오는-경로) 참고).

#### 돌아오는 퀘스트: 시즌 tier와 `rearm`

어떤 퀘스트는 런이 아니라 일정에 따라 돌아옵니다 — 매가을 열리는 수확 심부름 게시판, 매년 돌아오는 축제처럼 말입니다. 두 가지 속성이 이를 담당합니다(dsl 0.27.0 §5).

**`tier="season:<name>"`**은 퀘스트를 스키마가 선언한 [시즌](/ko/state/schemas/#시즌)에 속하게 합니다. 시즌이 다시 열리면 퀘스트는 `unset`으로, 목표는 미완료로 돌아갑니다. 새 런에서 run-tier 퀘스트가 초기화되는 것과 같습니다. 어떤 스키마도 선언하지 않은 시즌은 `E-SEASON-DECL`입니다.

**`rearm="<condition>"`**은 시즌 없이 하나의 퀘스트를 다시 실행합니다. 조건이 거짓에서 참으로 바뀔 때마다 퀘스트는 `unset`으로 돌아갑니다: 목표가 취소되고, `failedBy`가 지워지며, 기한을 잊습니다. 그런 다음 다시 수락되거나 시작될 수 있고, 성립하는 `start`는 같은 정착에서 활성화합니다:

```lute
<quest id="harvestMissions" title="Harvest missions" start="@harvestLive" rearm="@harvestLive">
  <objective id="bring" title="Bring in this week's barley" done="run.weekBarley >= 10"/>
</quest>
```

여기서 `@harvestLive`는 시계에 대한 def, 예를 들어 `clock.weekday == 5`입니다. 퀘스트는 첫 토요일에 시작하고, 플레이어가 전주에 끝냈든, 실패했든, 열린 채로 두었든 이후 매 토요일에 재장전되어 새로 시작합니다. 엔진은 플레이스루의 모든 퀘스트 정착에서 조건을 감시합니다. 긴 `advance:`가 지나가는 모든 시계 위치도 포함하므로, 금요일에서 월요일로 advance하면 통과하는 토요일에 퀘스트가 다시 장전됩니다. 조건을 처음 본 때는 기준선일 뿐이므로 플레이가 시작될 때 이미 참인 조건은 재장전하지 않습니다. `lute play`는 퀘스트가 끝난 상태와 함께 초기화를 출력합니다:
`quest harvestMissions -> unset (rearmed; was complete)`; 시즌 tier 퀘스트의 초기화는 `quest missions -> unset (season:harvest opened; was complete)`로 표시됩니다. 조건은 퀘스트의 `QuestCmd`에서 `rearm: {cel, expr, authored?}`로 컴파일됩니다.

rearm은 목표를 취소하지만 목표가 읽는 상태는 취소하지 않습니다. `run.weekBarley`는 주가 바뀔 때 엔진이 0으로 되돌리는 카운터입니다. 목표가 런 전체의 합계를 읽었다면 `done="run.barley >= 10"`, 다음 토요일에도 여전히 성립하고, 퀘스트를 재장전한 정착이 즉시 다시 시작하고 완료하며 보상까지 모두 지급했을 것입니다. 다시 장전되는 퀘스트의 목표에는 퀘스트와 함께 처음부터 시작하는 상태를 사용하세요: 조건이 참으로 바뀌는 순간보다 늦지 않게 엔진이 초기화하는 카운터(플레이 스크립트에서는 그 순간에 도달하는 `advance:` 전에 `engine:` 스텝으로, 0.41부터 `advance:` 자체의 `engine:` 쓰기는 시계가 움직이기 전에 적용됩니다), 또는 `tier="season:<name>"` 아래의 `season.<name>.*` 경로입니다. 검사기는 이에 대해 경고하지 않습니다.

거짓→참으로 바뀌는 순간에만 재장전하므로 상수 조건은 절대 재장전하지 않습니다: `rearm="true"`, `rearm="false"`, 또는 상수로 접히는 def나 비교는 `W-QUEST-REARM-CONSTANT`입니다. `repeatable=`이나 `repeat=`은 없습니다. 이를 쓰면 `E-UNKNOWN-ATTR`이고 메시지는 `rearm="<condition>"`을 가리킵니다. 서브퀘스트(`<objective quest=…>`가 이름 붙인 퀘스트)는 재장전할 수 없습니다. 부모와 함께 활성화되므로 부모가 끝난 뒤 재장전되는 자식은 영원히 `unset`에 머물기 때문입니다. 자식의 `rearm=`은 `E-SUBQUEST-REARM`입니다. 부모에 조건을 두세요. 부모의 rearm이 새 라운드를 시작합니다.

### `<objective>`

`<objective id done>`에는 선언된 상태에 대한 `done` 완료 술어가 필요합니다. `visibleWhen`은 목표가 표시되고 추적되는지만 제어하며 `done`을 게이트하지 않습니다(`done`이 조건을 기다리게 하려면 그 조건을 `done`에 넣으세요). `optional`은 완료 대상에서 제외하고, `by`와 `until`은 [기한](#기한)을 설정합니다. 비어 있는 본문을 가진 목표는 자체 닫는(`<objective …/>`) 형식으로 써야 합니다. 본문 — 로그 줄이나 목표별 `::set` 보상 — 은 목표가 처음 `done`이 될 때 **한 번** 출력됩니다.

**완료는 파생됩니다.** 작성자가 직접 쓰는 것이 아닙니다: 모든 `optional`이 아닌 목표가 `done`이 되면 퀘스트가 `complete`가 됩니다([`complete="any"`](#대안-completeany)에서는 그중 하나). 목표 완료는 단조적입니다 — 한 번 `done`으로 기록되면 계속 유지됩니다.

#### 목표 제목은 평문입니다

퀘스트 UI는 목표의 `title`을 쓰인 그대로 보여 주며 아무것도 렌더링하지 않습니다. 그래서 0.52부터 `<objective title>` 안의 `{{`는 마커든 이스케이프든 `E-OBJECTIVE-TITLE-INTERP`입니다. 낱말은 평문으로 쓰고, 바뀌는 값은 줄에서 보여 주세요:

<!-- lute-diagnostics -->
```
q.lute:7:30: error [E-OBJECTIVE-TITLE-INTERP] `<objective id="pay">`'s `title` is shown as written — a quest UI does not render `{{run.gold}}` — so it takes no `{{…}}`: write the words as plain text (`title="Pay the toll"`), and show a value that changes in a line
```

퀘스트 자체의 `title=`은 원래 보간되지 않았으며 바뀌지 않습니다.

### `<reward>` id

퀘스트 또는 목표의 직계 자식인 `<reward>`에는 선택적 `id`를 붙일 수 있습니다. 이는 `[A-Za-z][A-Za-z0-9_-]{0,63}`에 맞는 제한된 ASCII 토큰이어야 합니다. id는 해당 퀘스트의 보상 전체에서 유일해야 하며 목표의 보상도 포함합니다. 같은 퀘스트에서 두 번째 `<reward id="clearPay">`를 쓰면 **`E-REWARD-DUP`**입니다. 이는 `RewardEntry.id`로 낮춰지고 소스 맵과 엔진 모델에서 보상의 안정적인 키가 되므로 보상을 재배열하거나 삽입해도 이동하지 않습니다. 태그가 없는 보상은 IR에 `id`가 없고 선언 인덱스로 키가 지정됩니다.

```lute check
---
kind: quest
title: The climb
state:
  run.floor: { type: int, default: 0 }
---

<quest id="climb" title="Reach the tenth floor" start="true" tier="run">
  <objective id="top" title="Reach floor ten" done="run.floor >= 10">
    <reward id="topBonus" kind="EMBERS" amount="20"/>
  </objective>
  <reward id="clearPay" kind="EMBERS" amount="50"/>
  <reward kind="BADGE" amount="1"/>
</quest>
```

```json
"rewards": [
  { "id": "clearPay", "kind": "EMBERS", "amount": 50 },
  { "kind": "BADGE", "amount": 1 }
]
```

### 보상 효과

플러그인의 보상 종류는 **효과**(플러그인 매니페스트의 `effects:`, 0.50부터)를 선언할 수 있습니다. 그 종류의 `<reward>`가 지급될 때마다 종류가 assert하거나 retract하는 사실, 그리고 수락하는 퀘스트입니다. 예전에는 호스트가 `BADGE` 보상을 읽고 `met(mira)`를 직접 썼지만, 이제는 체커와 런타임이 대신하므로 그 사실이 도달 가능성 분석과 가드에 보입니다. 매니페스트 쪽은 [매니페스트](/ko/plugins/manifests/) 페이지에 있습니다:

```yaml
rewardKinds:
  BADGE:       { target: { entity: person, required: true }, effects: { asserts: ["met(@target)"] } }
  REVOKE:      { target: { entity: person, required: true }, effects: { retracts: ["met(@target)"] } }
  OFFER_QUEST: { target: { required: true }, effects: { accepts: ["@target"] } }
```

`@target`은 보상의 따옴표 붙은 `target=`입니다. 0.52부터 효과는 kind가 선언한 속성 `@<attr>`도 읽을 수 있습니다(아래). `writes`는 효과가 아닙니다. 보상의 상태 쓰기는 `credits:`입니다.

[plugin-surfaces 예제](https://github.com/journeyWorker/lute/tree/main/docs/examples/plugin-surfaces)는 이 세 종류를 선언하고 지급합니다:

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

**`<reward>`별 검사.** 효과를 선언한 종류의 각 보상은 해석된 효과를 `::assert`, `::retract`, `::accept`로 쓴 것처럼 판정합니다:

- **사실.** 해석된 각 패턴은 `::assert` / `::retract`의 검사를 받습니다. 관계가 선언되어 있는지, 인자 개수와 종류와 멤버가 맞는지, 닫힌 종류 멤버십이 성립하는지입니다. 코드는 그 디렉티브가 내는 것과 같고 보상의 `kind=`에서 보고하며, 메시지에는 `` `<reward kind="BADGE">` asserts `met(mira)` (its kind's `effects`): `` 접두사가 붙습니다.
- **수락.** 해석된 각 퀘스트 id는 프로젝트 전체에서 `::accept`의 규칙을 받습니다. 알 수 없는 id(did-you-mean 포함), `start`가 있는 퀘스트, 자동 활성화되는 자식에 대해 **`E-ACCEPT-TARGET`**입니다. 앵커는 id가 `@target`에서 왔다면 보상의 `target=` 값이고, 아니면 `kind=`입니다. 메시지는 보상을 지목합니다: `` `<reward kind="OFFER_QUEST" target="secnd">` accepts quest `secnd` (its kind's `effects.accepts`), but no quest in the project declares that id — did you mean `second`? ``
- **`target=`이 없을 때.** `@target`을 읽는 효과는 적용되지 않으며 추가 진단도 없습니다. `target`이 `required: true`인 종류는 이미 `E-REWARD-TARGET`을 보고합니다.
- **`W-QUEST-NEVER-ACCEPTED`.** 보상 `accepts`가 퀘스트를 지목하면 씬의 `::accept`와 마찬가지로 이 경고를 없앱니다. 메시지는 이제 ``… but no `::accept` or reward `accepts` in the project names it …``입니다.
- `W-REWARD-DOUBLE-CREDIT`은 그대로입니다.

**지급 시점의 순서.** 보상이 지급될 때(`outcome`이 전환과 맞고 `when=`이 성립할 때), 선언 순서대로 **보상별로**:

1. 이전처럼 `grant` 레코드와 `credits` 쓰기;
2. 종류의 `retracts`, 이어서 `asserts`를 `::retract` / `::assert`와 같은 경로로 처리(런타임 `E-FACT-EXCLUSIVE` 검사 포함);
3. `accepts`를 `::accept`의 경로로 처리: 수락 기반 퀘스트는 다음 생명주기 라운드에 활성화되고, 이미 `unset`을 지난 퀘스트는 그대로 두며 레코드가 그렇게 기록합니다.

그다음 보상으로 넘어갑니다. 이전과 같이 목표 보상이 퀘스트 보상보다 먼저 지급되고, 한 전환의 모든 지급은 그 전환의 생명주기 핸들러 본문보다 앞서므로 `questComplete` 핸들러는 보상이 assert한 사실을 읽습니다. 효과는 지급 한 번당 정확히 한 번 발생하며, 재생과 복원은 지급 자체의 인스턴스 정체성을 따릅니다. 가드가 거짓인 `<reward when="…">`는 효과를 포함해 아무것도 지급하지 않습니다.

분석은 각 효과를 소유자의 지급 지점에서 읽습니다. `outcome`이 없는 퀘스트 보상은 퀘스트 완료 시점(`questComplete` 핸들러 앞), `outcome="failed"`는 실패 시점(`questFailed` 앞), 목표 보상은 목표 완료 시점입니다. 보상의 assert는 생산자일 뿐입니다. 그 사실에 걸린 퀘스트를 도달 가능하게 하지만, 지급 뒤 그 사실이 *보장*되지는 않으므로(핸들러는 다른 지급 뒤로 미뤄질 수 있고, 호스트는 퀘스트를 완료로 시드할 수 있습니다) 그것을 검사하는 가드는 `W-FACT-GUARANTEED`도 `E-ARM-DEAD`도 받지 않습니다. 보상의 retract는 보장된 사실에서 그것을 제거합니다. 결과는 [사실과 Datalog](/ko/state/facts-and-datalog/#보상-효과가-만드는-사실)와 [도달 가능성](/ko/connectivity/reachability/#보상-효과가-그리는-엣지)에 있습니다.

#### 보상 kind 속성

보상 kind는 디렉티브가 속성을 선언하듯 매니페스트에 **`attrs:`**를 선언할 수 있습니다. 0.52부터(#201) 그 kind의 `<reward>`에 이 속성을 쓸 수 있고, kind의 효과는 이를 `@<attr>`로 읽습니다:

```yaml
rewardKinds:
  BADGE:
    target: { entity: person, required: true }
    attrs:
      - { name: rarity, type: { enum: [common, rare, gold] }, default: common }
    effects: { asserts: ["badge(@target, @rarity)"] }
```

```lute
<objective id="pay" done="run.gold > 2">
  <reward kind="BADGE" target="mira" rarity="gold"/>
</objective>
```

- **디렉티브 속성처럼 검사됩니다.** 값은 따옴표 리터럴이고(`bool`은 맨 플래그가 `true`), 선언에 따라 디렉티브와 같은 코드로 타입 검사됩니다. `rarity="silver"`는 `E-BAD-ENUM`(`` `silver` is not a valid value for `rarity` of `<reward kind="BADGE">` (expected one of: common, rare, gold) ``)이고, 빠진 `required: true` 속성은 `E-MISSING-ATTR`, `@ref` 값은 `E-REWARD-ATTR`입니다. 보상의 속성은 엔진이 지급할 때 기준으로 삼는 정적 데이터이기 때문입니다.
- **알 수 없는 속성.** `E-UNKNOWN-ATTR`는 kind가 선언한 속성을 나열합니다: `` `<reward>` has no attribute `rarety` — did you mean `rarity`?; its attributes are `id`, `kind`, `target`, `amount`, `when`, `outcome`, and kind `BADGE` declares `rarity` ``.
- **효과.** `@rarity`는 작성한 값에, 없으면 선언된 `default:`에 바인딩됩니다. 둘 다 없는 속성은 바인딩되지 않으며, 그것을 읽는 효과는 `target=`이 없을 때처럼 적용되지 않습니다. 검사기는 바인딩된 사실(`badge(mira, gold)`)을 위와 같이 판정합니다.
- **아티팩트와 지급.** 컴파일된 보상은 `attrs`(작성한 각 속성과 작성하지 않은 각 기본값, 타입 적용)를 담고, 런타임의 `grant` 레코드는 이를 `reward`에 복사합니다:

```json
{"index":0,"instance":1,"kind":"grant","objective":"pay","quest":"first","reward":{"amount":1,"attrs":{"rarity":"gold"},"kind":"BADGE","target":"mira"}}
```

`attrs:`가 없는 kind의 보상은 예전과 똑같이 컴파일되고 지급됩니다. 선언 규칙은 [매니페스트](/ko/plugins/manifests/#보상-kind-속성) 페이지에 있습니다.

### 퀘스트가 씬 및 계기를 만나는 방식

세 가지 작은 추가 기능(dsl 0.21.0 §7a)으로 씬이 상태를 거쳐 플래그를 전달하지 않고 퀘스트를 직접 움직일 수 있습니다.

**모든 조건에서 `visited()`.** 0.21.0 전에는 `after:` 안에서만 허용되었던 `visited('<scene id>')`가 이제 **모든 조건 슬롯**의 Lute-CEL 함수입니다: 퀘스트 `start` / `fail`, 목표 `done`, 비트와 엔트리의 `when`, 콘텐츠 줄과 선택지의 `when=`에서 사용할 수 있습니다. 해당 씬이 이 세이브에서 제시된 뒤 참입니다(`after:`가 읽는 것과 같은 visited 집합이며 새 런에서도 지워지지 않습니다). 따라서 씬을 플레이하는 것만으로 퀘스트를 진행할 수 있습니다:

```lute
<objective id="heardVesna" title="Hear Vesna out" done="visited('haven.s01ep04')"/>
```

인수는 문자열 리터럴 하나입니다. 프로젝트에 없는 씬을 이름 붙이면 `after:`와 정확히 같이 프로젝트 전체 `lute check`에서 `E-CONN-UNKNOWN-NODE`가 됩니다. 단일 파일 `check`는 씬이 플레이되었는지 알 수 없으므로 고립된 `visited()`를 참 또는 거짓으로 결정하지 않습니다. 다만 조건 자체의 모순은 발견합니다: `visited('a.b') && !visited('a.b')`는 무엇이 플레이되었든 거짓입니다(dsl 0.23.0, [`when` 판정 방식](/ko/language/beats/#when의-판정-방식) 참고). [번들 비트](/ko/language/beats/#비트-번들)도 정식 id `<document id>.<beat id>` 아래에서 방문됩니다.

**계기에서 판정하는 목표.** `<objective on="<occasion>">`은 퀘스트가 active인 동안 계기가 발생할 때만 해당 목표의 `done`을 평가합니다 — 계속 평가하는 조건으로는 표현할 수 없는 런 종료 검사 시점입니다. `on`이 없으면 이전처럼 목표를 계속 평가합니다.

```lute
<objective id="lowPressure" title="Keep the shed calm" on="runEnd" done="run.shedPressure < 2"/>
```

dsl 0.23.0부터 `on` 목표는 비트처럼 **대상**도 이름 붙일 수 있습니다: `<objective on="talk" target="npc.maud" …>`는 `talk`가 `npc.maud`를 위해 발생할 때만 판정됩니다. 다른 사람에게 말해도 그대로 둡니다. `target`이 없으면 대상이 무엇이든 계기가 발생할 때마다 목표를 판정합니다.

```lute
<objective id="thankMaud" title="Thank Maud in person" on="talk" target="npc.maud" done="run.answered"/>
```

계기는 비트의 `on`과 정확히 같은 방식으로 어휘에 대조됩니다([비트](/ko/language/beats/#계기) 참고): 플러그인이 계기를 선언했는데 이 계기가 그중에 없으면 `E-OCCASION-UNKNOWN`, 그 외에는 모양만 검사됩니다. 이름이 아닌 `on`은 `E-BEAT-ATTR`입니다. `target`도 비트 대상처럼 검사되며 역시 `E-BEAT-ATTR`입니다: 따옴표로 감싼 점 구분 id여야 하고, `on`이 필요하며, 해당 계기가 대상을 받아야 하고, [대상 도메인](/ko/language/beats/#대상-도메인)이 그것을 포함해야 합니다. `lute trace` / `lute run`은 mock 키 `occasions: [runEnd]` 또는 `--occasion runEnd`(반복 가능)로 퀘스트 보행을 위한 계기를 발생시키며, 보행이 정착된 뒤 순서대로 적용합니다. 대상 계기는 `<occasion>@<target>`으로 하나를 발생시킵니다(`occasions: [talk@npc.maud]`, `--occasion talk@npc.maud`). `lute play`는 계기를 발생시키는 모든 스텝에서, 그 스텝의 `target:`에 대해 목표를 판정하며, 목표만 참조하는 계기도 합법적인 스텝입니다. 같은 이름의 월드 이벤트가 선언되어 있으면 모든 발생이 먼저 그것을 실행하므로, active 퀘스트의 해당 이름 `<on event>` 핸들러가 목표 판정보다 먼저 실행되고 핸들러가 쓴 값을 목표가 읽을 수 있습니다([비트](/ko/language/beats/#계기) 참고).

기본적으로 계기는 먼저 비트를 제시하고 나중에 목표를 판정하므로 `runEnd`의 비트는 여전히 퀘스트가 `active`인 것을 봅니다. 챕터를 닫는 계기는 반대 순서, 즉 챕터의 퀘스트가 어떻게 끝났는지 아는 에필로그를 원할 때가 많습니다. 플러그인은 [`occasions:` export](/ko/plugins/manifests/)에서 해당 계기를 **`judge: before`**로 선언합니다(dsl 0.24.0 §2):

```yaml
occasions:
  chapterEnd: { judge: before }
```

`chapterEnd`를 발생시키면 비트를 결정하기 **전에** `on=` 목표를 판정하고 퀘스트를 정착합니다. 그러므로 `when: "quest.tide.state == 'complete'"`인 비트가 `tide`를 완료한 같은 발생에서 이길 수 있습니다. 이동하는 것은 판정뿐입니다. 발생이 답하는 핸들러 본문, `<on event="chapterEnd">` 핸들러 및 정착시킨 퀘스트의 `questComplete` / `questFailed` 핸들러는 여전히 비트 뒤에 실행되므로, 그 내레이션은 씬 뒤에 나옵니다:

```
── step 3 · chapterEnd ──────────────
  tide.home done
  quest tide -> complete
  ✓ end.tide [scene, priority 5]
  → end.tide
@narrator: Home before the water.
@narrator: Handler on chapterEnd.
@narrator: Tide beaten.
```

`lute play`는 스텝 헤더 아래, 후보 위에 전환을 출력합니다(`--json`: 스텝의 `judgedBefore`; `quests`는 핸들러 본문과 함께 그 이후의 퀘스트입니다). 기본값인 `judge: after`는 0.21.0의 순서를 유지합니다.

**씬에서 퀘스트 수락하기.** `start`가 없는 퀘스트는 *수락으로 시작됩니다*. 엔진의 "퀘스트 수락" 액션(및 `lute trace --accepted-quest`)에 대응하는 씬 쪽 형태는 핵심 디렉티브 [`::accept{quest="<id>"}`](/ko/language/directives/#accept--퀘스트-수락)이며, 보통 플레이어가 동의하는 선택지 안에 둡니다:

```lute
<branch id="request">
  <choice id="accept" text="I'll keep it calm">
    ::accept{quest="calmTheShed"}
    @vesna: Thank you.
  </choice>
  <choice id="decline" text="Not now">
    @vesna: Another time, then.
  </choice>
</branch>
```

퀘스트가 이미 active, complete 또는 failed이면 수락해도 아무 일도 일어나지 않습니다. `::accept`에는 가드를 붙일 수 있습니다. `::accept{quest="calmTheShed" when="run.shedPressure > 2"}`처럼 쓰며(dsl 0.26.0 §4, [디렉티브에 가드하기](/ko/language/directives/#디렉티브-가드하기-when) 참고), 가드가 거짓이면 건너뜁니다. 프로젝트 전체 `lute check`는 어떤 `::accept`도, [보상 `accepts`](#보상-효과)도 이름 붙이지 않는 수락 기반 퀘스트에 **`W-QUEST-NEVER-ACCEPTED`**를 경고합니다(dsl 0.24.0 §2). 프로젝트의 어떤 것도 활성화할 수 없기 때문입니다. dsl 0.25.0 §5부터 `accepts:` mock(`mocks/*.yaml`, `*.test.yaml`)은 더 이상 이를 세지 않습니다. mock은 테스트만 증명하고 게임은 증명하지 않으므로, mock만 수락하는 퀘스트도 계속 경고하며 메시지에 mock 파일을 표시합니다. 문서 바깥에서 엔진이 수락하는 퀘스트는 [`accept="external"`](#스크립트-바깥에서-수락하기-acceptexternal)을 대신 선언합니다. `--deny W-QUEST-NEVER-ACCEPTED`는 경고를 오류로 만듭니다.

<!-- lute-diagnostics -->
```
./quests/lamp.lute:24:12: warning [W-QUEST-NEVER-ACCEPTED] quest `lostDog` is accept-driven (no `start`), but no `::accept` or reward `accepts` in the project names it (only the `accepts:` mock of `tests/dog.test.yaml` does, and a test mock is no acceptance in the game), so it never activates; accept it from a scene with `::accept{quest="lostDog"}`, declare `accept="external"` if the engine accepts it outside the script (a quest board, a menu), or give it a `start` condition
```

**다음 런에 수락하기.** 런이 끝난 뒤 허브나 현상금 게시판에서 수락한 run-tier 퀘스트는 끝나는 런에서 활성화되었다가 `newRun`으로 초기화됩니다. `::accept{quest="<id>" at="nextRun"}`(dsl 0.24.0 §2)는 대신 수락을 대기열에 넣습니다. 다음 `newRun` 초기화 직후 적용되어 새 런의 첫 정착에서 퀘스트를 활성화합니다:

```lute check
---
kind: scene
id: town.board
title: The bounty board
---

## Board

@maud: The bounties reset at dawn. Sign now and the hunt is yours next run.

<branch id="bounty">
  <choice id="sign" text="Sign for the next run">
    ::accept{quest="wolfBounty" at="nextRun"}
    @maud: Your name's on the board.
  </choice>
  <choice id="later" text="Not this time">
    @maud: Suit yourself.
  </choice>
</branch>
```

`nextRun`만 `at`에 쓸 수 있으며, 다른 값은 `E-ACCEPT-TARGET`입니다. 선택지가 실행된 곳에서 `lute play`는 `quest wolfBounty accepted (queued: applies after the next newRun)`을 출력하고, 다음 `newRun` 스텝 아래에 `quest wolfBounty accepted (queued at="nextRun")`을 출력합니다.

**시나리오 테스트의 퀘스트 결과.** `lute test`는 trace가 실행한 생명주기를 `expect.quests: {<questId>: unset | active | complete | failed}`로 단언합니다 — 완료를 관찰하기 위해 부수 효과 `::set`을 쓸 필요가 없습니다. 테스트, trace mock 또는 플레이 스크립트는 최상위 `quests: {<questId>: complete}`(dsl 0.22.0)로 저장된 상태에서 시작할 수도 있습니다. 이것이 상태를 시드하는 방법입니다. 플레이 스크립트의 `engine:` 스텝은 `quest.*` 쓰기를 거부합니다. 상태 변경은 핸들러와 보상을 발생시키는 생명주기에 속하기 때문입니다. dsl 0.26.0 §7부터 테스트 또는 `lute trace --project`는 프로젝트 전체에 걸쳐 `accepts:`의 퀘스트 id를 해석합니다. 따라서 한 씬의 테스트가 다른 문서에 선언된 퀘스트를 수락할 수 있습니다(프로젝트가 선언한 퀘스트가 없는 id는 `E-TRACE-ACCEPT`). 퀘스트 문서의 테스트는 자신의 퀘스트를 시드할 수 있습니다(`quests: { salvage: active }`): 보행이 그곳에서 퀘스트를 시작합니다.

#### 스크립트 바깥에서 수락하기: `accept="external"`

문서가 아닌 곳에서 퀘스트를 시작하는 경우가 있습니다. 퀘스트 게시판, 메뉴, 일지 UI처럼 말입니다. 수락 순간은 엔진의 것이고, 어떤 씬도 `::accept`할 수 없습니다. **`<quest accept="external">`**(dsl 0.25.0 §5)가 이를 선언합니다:

```lute check
---
kind: quest
title: The wreck
state:
  run.dived: { type: bool, default: false }
---

<quest id="salvage" title="Salvage the wreck" accept="external">
  <objective id="dive" title="Dive the wreck" done="run.dived"/>
</quest>
```

이 퀘스트는 수락 기반입니다. 플레이어가 선택하는 어느 때든 엔진이 수락할 때까지 `unset`으로 남으며, IR은 `QuestCmd`에 기본적으로 생략되는 `"accept": "external"`을 담습니다. `accept="external"`은 `W-QUEST-NEVER-ACCEPTED`를 억제하는 유일한 콘텐츠 외부 수락 소스입니다. 씬이 여전히 이것을 `::accept`할 수도 있습니다.

툴체인은 엔진의 수락을 대신합니다:

- 테스트 또는 `lute trace`는 `accepts: [salvage]`(또는 `--accepted-quest salvage`)로 퀘스트를 시작합니다. 이것이 테스트가 외부 퀘스트를 걷는 방법이며, 테스트만 증명합니다. 0.25.0부터 `accept="external"`도 `::accept`도 없는 퀘스트에 대해서는 `W-QUEST-NEVER-ACCEPTED`를 더 이상 억제하지 않습니다.
- 플레이 스크립트는 퀘스트 게시판처럼 미리 상태를 시드하지 않고 `engine:` 스텝으로 플레이 중간에 수락합니다(dsl 0.26.0 §7):

  ```yaml
  steps:
    - engine: { accept: [salvage] }
      expect: { quests: { salvage: active } }
  ```

  `lute play`는 `quest salvage accepted (engine)`을 출력하고 생명주기를 정착하므로 그 스텝에서 퀘스트의 `questActive` 핸들러가 실행됩니다. `engine: { accept }`는 수락 기반 퀘스트만 시작하며 `start`가 있는 퀘스트는 거부합니다.

`accept`에는 `external`만 쓸 수 있습니다. 다른 값은 `E-ATTR-TYPE`입니다. `start`와 함께 `accept="external"`을 쓰는 경우도 마찬가지입니다. 퀘스트가 활성화되는 방법이 두 가지가 되기 때문이며, 메시지는 호스트의 조건을 쓰는 표기로 `follows=`를 알려 줍니다(아래). [서브퀘스트](#서브퀘스트) 자식이 부모와 함께 활성화될 때는 엔진의 수락이 아무 일도 하지 않으므로 프로젝트 전체 `lute check`가 `E-ACCEPT-TARGET`을 보고합니다. 자식에도 [`activate="accept"`](#대화에서-시작하기-activateaccept)를 선언하면 부모가 active인 동안 엔진을 기다립니다:

<!-- lute-diagnostics -->
```
./quests/board.lute:10:41: error [E-ACCEPT-TARGET] quest `child` declares `accept="external"`, but it activates with its parent `board`, so the engine's acceptance does nothing; declare `activate="accept"` on it too so it waits to be accepted
```

#### 수락 기반 퀘스트로 들어오는 경로

수락 기반 퀘스트는 수락될 때 활성화되며, 수락할 수 있는 것은 세 가지입니다. 누가 결정하느냐가 다릅니다:

- **스크립트 안의 수락** — 씬의 `::accept` 또는 보상 kind의 `effects.accepts` — 은 `follows=`가 무엇이든 **상관없이** 다음 생명주기 라운드에 퀘스트를 활성화합니다. `follows`는 그래프와 호스트를 위한 메타데이터이며 런타임은 읽지 않습니다.
- **호스트의 부여**: `accept="external"` 퀘스트(게시판, 메뉴, 서버)는 호스트가 스스로 결정합니다. 이런 퀘스트의 `follows=`는 **호스트가 부여하는 조건**입니다. Lute는 이를 호스트에 전달하고, 분석은 호스트 정책으로 읽습니다. Lute가 강제하지는 않습니다.
- **그 밖에는 없습니다.** `accept="external"`이 아니고 `::accept`나 보상이 이름 붙이지 않는 퀘스트에는 `follows=`가 있든 없든 경로가 없습니다. `follows`는 순서 메타데이터일 뿐 경로가 아닙니다. `follows`가 있으면 `reach`는 `Unknown`이고(아래), 없으면 그 퀘스트는 간선 없는 퀘스트의 보수적인 판정대로 `Unanchored`로 남습니다.

따라서 다른 퀘스트가 끝나야 호스트가 부여하는 퀘스트는 `accept="external" follows="…"`로 씁니다. `accept="external"`과 `start`의 조합은 계속 거부됩니다. 아래 두 퀘스트는 깨끗하게 검사되며, `salvage`는 `survey`가 끝난 뒤 호스트를 통해 도달 가능합니다:

```lute check
---
kind: quest
title: The board
state:
  run.dived: { type: bool, default: false }
---

<quest id="survey" title="Survey the reef" tier="run" start="true">
  <objective id="look" title="Look at the reef" done="run.dived"/>
</quest>

<quest id="salvage" title="Salvage the wreck" tier="run" accept="external" follows="completed('survey')">
  <objective id="dive" title="Dive the wreck" done="run.dived"/>
</quest>
```

`lute scenario reach`는 퀘스트의 accept 앵커 소스 가운데 호스트 경로를 맨 앞에 나열합니다. 외부 퀘스트는 `follows`를 선언했거나 스크립트 안의 무언가가 수락할 때 그 앵커를 가집니다. 둘 다 없으면 앵커가 없어 `Unanchored`로 남습니다(`[accept]` 줄 없음). `follows`가 없으면 호스트 경로는 `[accept] the host (accept="external")`로 읽히고 증거는 `proven`입니다. `follows`가 있으면 호스트 경로가 호스트 정책에 기대므로, 호스트 경로에만 의존하는 판정의 증거가 **`heuristic`**입니다(실행될 수 있는 스크립트 안 수락이 있으면 `proven`). `follows` 줄에는 메타데이터라는 표시가 붙습니다:

```console
$ lute scenario --project . reach --node quest:gated
project root: .
reach quest(gated):
  verdict: Reachable — reachable if the host grants it once `follows` holds (host policy; Lute does not enforce it). [heuristic]
  follows: completed("intro") (declared ordering; not an activation route)
  anchored; each anchor holds before it activates, through any one of its sources:
    [accept] the host once follows: completed("intro")
  referenced node(s) (see `follows` and the anchors above — this is NOT a flat requirement list):
    - quest(intro): Reachable — a satisfiable route exists under your declared routes.
```

수락 기반이고 `accept="external"`이 아니며 `follows`를 선언했지만 어떤 `::accept`나 보상도 이름 붙이지 않는 퀘스트에는 경로가 전혀 없습니다. `reach`는 `Unknown`이고 `W-QUEST-NEVER-ACCEPTED`가 발생합니다. `follows` 간선은 그대로 그려집니다.

```console
$ lute scenario --project . reach --node quest:board
project root: .
reach quest(board):
  verdict: Unknown — this analysis cannot prove reachability either way under your declared routes. [unknown]
  follows: completed("intro") (declared ordering; not an activation route)
  anchored; each anchor holds before it activates, through any one of its sources:
    [accept] nothing accepts it (no ::accept or reward names it, and it is not accept="external")
  referenced node(s) (see `follows` and the anchors above — this is NOT a flat requirement list):
    - quest(intro): Reachable — a satisfiable route exists under your declared routes.
```

**수락 교착.** 순환 밖의 어떤 소스도 들어오지 않고 호스트 경로도 열려 있지 않은 `start` 조건과 스크립트 안 수락의 순환은 **`E-QUEST-START-CYCLE`**이며, 강연결 집합마다 하나씩 보고됩니다. 가장 작은 퀘스트의 `start`에서 보고하고, 그 퀘스트에 `start`가 없으면 `id`에서 보고합니다. 경로는 간선 종류(`-[start]->`, `-[accept]->`)를 이름으로 밝힙니다. 아래에서 `intro`는 `letter`가 active일 때 시작하고, `letter`는 `intro`의 보상으로만 수락됩니다:

<!-- lute-diagnostics -->
```
./quests/q.lute:7:36: error [E-QUEST-START-CYCLE] quest start cycle: quest(intro) -[start]-> quest(letter) -[accept]-> quest(intro) — each waits for the next (a `start` for its condition, an accept for the `::accept` or reward that grants it) and no source outside the cycle starts any of them; start one of them from something outside the cycle, or let the host grant one with `accept="external"`
```

보상이 서로를 수락하는 두 퀘스트도 같은 방식으로 교착하며, `a`의 `id`에서 보고됩니다:

<!-- lute-diagnostics -->
```
./quests/q.lute:7:12: error [E-QUEST-START-CYCLE] quest start cycle: quest(a) -[accept]-> quest(b) -[accept]-> quest(a) — each waits for the next (a `start` for its condition, an accept for the `::accept` or reward that grants it) and no source outside the cycle starts any of them; start one of them from something outside the cycle, or let the host grant one with `accept="external"`
```

세 번째 소스가 있으면 순환이 풀립니다. 스스로 시작해서 그중 하나를 수락하는 퀘스트, 씬의 `::accept`, 또는 그중 하나의 `accept="external"`입니다.

**`follows=`를 가리키는 메시지.** `start`와 함께 쓴 `accept="external"`은 `E-ATTR-TYPE`, 자기 퀘스트의 상태를 읽는 `start`는 `E-QUEST-START-SELF`, `start`가 있는 퀘스트를 수락하면 `E-ACCEPT-TARGET`입니다. 이제 각 메시지가 대신 무엇을 쓸지 알려 줍니다:

<!-- lute-diagnostics -->
```
./quests/q.lute:7:35: error [E-ATTR-TYPE] `<quest id="bad">` carries both `accept="external"` and `start`: a `start` condition activates a quest by itself, so the host could never decide; drop `start`. If the host grants it only once some scenes or quests are done, write that as `follows="…"` (for example `follows="completed('intro')"`): the host applies it, and analysis reads it as the host's condition
./quests/q.lute:11:54: error [E-QUEST-START-SELF] `start` of quest `selfq` reads its own state (`quest.selfq.state`), which has no value before the quest starts, so this condition can never start it as intended; a quest that only the host grants is written `accept="external"` without a `start`; if the host grants it only once other scenes or quests are done, write those as `follows="…"`, which the host applies and analysis reads
./quests/q.lute:21:38: error [E-ACCEPT-TARGET] `<reward kind="OFFER_QUEST" target="tgt">` accepts quest `tgt` (its kind's `effects.accepts`), which has a `start` condition and activates on its own; only an accept-driven quest (no `start`) can be accepted; drop its `start` — the accept activates it whatever other condition it had (a `follows="…"` records such conditions for analysis and the host, but does not gate the accept)
```

### 기한

어떤 목표에는 창이 있습니다. 네 번째 날 전에 편지에 답하거나, 종이 울리기 전에 관문에 도달하는 경우입니다. 목표의 **`by`**(dsl 0.23.0)가 그 창을 닫습니다. `done`과 같은 조건 슬롯입니다:

```lute check
---
kind: quest
title: The letter
state:
  run.day: { type: int, default: 1 }
  run.answered: { type: bool, default: false }
  run.apologized: { type: bool, default: false }
---

<quest id="letter" title="Answer the letter" start="true" tier="run">
  <objective id="reply" title="Write back before the fourth day" done="run.answered" by="run.day >= 4"/>
  <objective id="thank" title="Thank Maud in person" on="talk" target="npc.maud" done="run.answered" by="run.day >= 4"/>
  <objective id="sorry" title="Apologize to Oskar when you next see him" on="talk" target="npc.oskar" done="run.apologized" until="run.day >= 4" optional/>
  <on event="questFailed">
    @narrator: The letter goes unanswered. Maud stops asking.
  </on>
</quest>
```

목표가 아직 done이 아닐 때 `by`가 참이 되는 **첫** 순간 목표는 **실패**하며, 다시 판정되지 않습니다. 필수 목표가 실패하면 `fail` 조건과 마찬가지로 퀘스트도 실패합니다. 퀘스트의 `failed` 보상이 지급되고 `questFailed` 핸들러가 실행되며 실패가 아직 active인 서브퀘스트로 전파됩니다. 실패한 `optional` 목표는 퀘스트에 영향을 주지 않습니다.

- 모든 정착에서 `done`은 `by`보다 먼저 판정됩니다. 따라서 `done`과 `by`가 같은 순간 참이 되는 목표는 done으로 처리되고, done인 목표는 나중에 실패하지 않습니다. `on=` 목표의 계기를 발생시키는 스텝에서는 해당 스텝의 모든 제시와 정착에서 그 목표의 `done`을 먼저, `by`를 나중에 판정합니다. 따라서 그 판정이 `by`도 참으로 만들더라도 올바른 판결을 기록하는 hearing 비트가 목표를 완료합니다.
- `by`는 순간입니다. `on=` 여부와 관계없이 모든 목표에 대해 모든 생명주기 정착에서 판정됩니다(dsl 0.24.0 §2.1). 위의 `thank`는 플레이어가 다시 Maud에게 말하는지와 관계없이 `run.answered`가 여전히 false인 채 4일째에 도달하는 정착에서 실패합니다. (0.23.1에서는 `on=` 목표의 `by`를 계기에서만 판정했지만, 0.24.0에서 뒤집혔습니다.)
- `until="<condition>"`은 장소에 묶인 기한입니다. 목표의 계기가 발생할 때(대상을 이름 붙였다면 그 대상에 대해) `done` 직후에만 판정합니다. 따라서 계기가 답하는 순간이 판정과 기한을 겸하며, 그 계기의 비트가 `until`이 읽는 상태를 쓰더라도 올바른 답을 실패로 만들 수 없습니다. 위의 `sorry`는 사과하지 않은 채 4일째나 그 이후에 Oskar에게 말할 때만 실패하고, 다시 그를 만나지 않는 플레이어에게는 열린 채로 남습니다. `until`에는 `on=`이 필요합니다(`on=`이 없으면 `E-BEAT-ATTR`: 어디서든 성립하는 기한은 `by=`입니다).
- 퀘스트는 계기가 제시하는 모든 비트 뒤에 정착하므로 하나의 [`select: sequence`](/ko/language/beats/#루틴-다음에-오늘의-이벤트) 계기에서 두 비트 사이에 `by`가 지날 수 있습니다.
- 실패를 읽을 수 있습니다: `quest.<id>.objectives.<oid>.failed`는 그때부터 `true`이고, 실패한 필수 목표는 퀘스트의 `failedBy`를 `by` 또는 `until`로 설정합니다. `tier="run"` 퀘스트는 새 런에서 둘 다 지웁니다.
- `by`와 `until`은 `done`처럼 검사됩니다. 선언되지 않은 경로는 `E-UNDECLARED`이고, 비어 있을 수 있는 읽기는 `E-MAYBE-UNSET`입니다.

`lute trace`는 목표의 결정을 `failed`로 기록하고, `lute run` / `lute play`는 `letter.thank failed (by)` 또는 `failed (until)`을 출력합니다. 세 도구 모두 `done`과 기한을 같은 순서로 판정합니다. 퀘스트 전환도 [`failedBy`](#퀘스트가-실패한-이유)를 같은 방식으로 이름 붙입니다: `quest letter -> failed (by)`, 그리고 `(until)`, `(fail)`, `(subquest)`, `(cascade)`, `(superseded)`도 같습니다. [스토리 플레이](/ko/tooling/play/#기한과-대상-지정-목표)를 참고하세요.

`on=`과 `by=`가 있고 `until=`은 없는 목표에서 `done`이 `by`를 필연적으로 함의한다면, 그 목표는 계기를 발생시키는 스텝에서만 완료될 수 있습니다. 그 밖의 곳에서는 계기가 발생하지 않은 정착에서 `by`가 `done`보다 늦지 않게 참이 되고, `done`을 판정하기도 전에 목표를 실패시킵니다. 이는 0.23.1의 계기 발생 시에만 판정하는 `by`를 위해 작성된 콘텐츠입니다. `lute check`와 프로젝트 전체 `lute check`는 `by=`에서 **`W-DEADLINE-BEFORE-DONE`**을 경고하고(dsl 0.24.0 §2.1), 같은 조건의 `until=`을 제안합니다. `--deny W-DEADLINE-BEFORE-DONE`은 이를 오류로 만듭니다:

```lute check
---
kind: quest
title: The hearing
state:
  run.verdict: { type: { enum: [undecided, guilty, acquitted] }, default: undecided }
---

<quest id="hearing" title="See the hearing through" tier="run" start="true">
  <objective id="verdict" title="Hear the verdict" on="hearing" done="run.verdict == 'guilty'" by="run.verdict != 'undecided'"/>
</quest>
```

<!-- lute-diagnostics -->
```
hearing.lute:9:100: warning [W-DEADLINE-BEFORE-DONE] objective `verdict`'s `by="run.verdict != 'undecided'"` holds whenever its `done="run.verdict == 'guilty'"` does, and `by` is a moment judged at every settle: it fails the objective at the settle it comes true, before occasion `hearing` ever judges `done` (unless both happen in the step that raises `hearing`) — write `until="run.verdict != 'undecided'"` to judge the deadline only when `hearing` is raised
```
### 서브퀘스트

`<objective quest="childId"/>`는 완료가 목표인 자식 퀘스트를 이름 붙입니다. 한 목표에서 `quest=`와 `done=`은 함께 쓸 수 없습니다(`E-OBJECTIVE-QUEST-DONE` — 정확히 하나가 필요합니다). 다른 모든 목표 속성은 `quest=`와 함께 사용할 수 있습니다(`when=`, `optional`, `title=`, 완료 본문). 본문도 이전과 정확히 같이 한 번만 재생됩니다 — 자식이 완료되어 목표가 처음 `done`이 될 때이며, 이는 자연스러운 "자식 해결" 일지 슬롯입니다. 자식은 같은 문서 또는 다른 문서에 있는 일반 `<quest>`입니다. 한 줄 태그 규칙, id 규칙, 퀘스트 본문 문법은 그대로이고, 작성한 `done=` 목표와 서브퀘스트 `quest=` 목표는 하나의 부모 안에서 자유롭게 섞을 수 있습니다.

```lute
<quest id="saveTheGrove" tier="run" title="Save the Grove" start="run.act == 1">
<objective id="halsin" title="Find Halsin" quest="findHalsin"/>
<objective id="ritual" title="Stop the ritual" quest="stopRitual"/>
<objective id="scout" title="Scout the perimeter" quest="scoutPerimeter" optional/>
<objective id="talkRath" title="Speak to Rath" done="run.spokeRath"/>
</quest>
```

새 태그나 새 명령 종류는 없으며 어디에도 `parent=` 속성이 없습니다 — 부모 → 자식 참조를 사용하고, 기존 두 표면과 엔진에서 파생된 두 규칙을 재사용합니다:

| 방향 | 규칙 |
| --- | --- |
| 목표 완료 | 컴파일러가 `done = "quest.<child>.state == 'complete'"`를 합성합니다. 파생되는 부모 완료 — "`optional`이 아닌 모든 목표가 `done`" — 는 바뀌지 않습니다. 서브퀘스트 목표의 `optional`은 자식 결과가 부모를 게이트하지 않는다는 뜻입니다. [`complete="any"`](#대안-completeany) 부모는 하나만 필요합니다. |
| 상향 실패 | 컴파일러가 부모의 `fail`을 작성된 `fail`(있다면)과 문서 순서대로 각 **필수** 자식에 대해 하나씩 둔 `quest.<c>.state == 'failed'` 검사의 이접으로 합성합니다. `fail`이 완료보다 우선하는 것은 같으므로 필수 자식이 실패하면 나머지 목표가 달리 완료될 수 있더라도 부모가 `failed`로 해결됩니다(`failedBy: subquest`). `complete="any"`에서는 합성된 부분이 연접이 됩니다. |
| 하향 전파 | 엔진 규칙: 부모가 종료 상태(`failed` 또는 `complete`)로 전환되면 아직 `active`인 모든 자식이 `failed`로 전환됩니다(`failedBy: cascade`). 부모 완료 시 필수 자식은 `active`일 수 없습니다 — 그 자식의 `complete`가 파생 완료의 일부이기 때문입니다. 따라서 `complete` 분기는 여전히 실행 중인 **optional** 자식만 실패시키며, `complete="any"`에서는 다른 모든 실행 중인 자식을 `superseded`로 실패시킵니다. 재귀적으로 적용됩니다. |
| 활성화 | `start`가 없는 참조 자식은 부모가 활성화될 때 활성화됩니다(오늘날의 수락 기반 기본 동작을 대체함). 단, [`activate="accept"`](#대화에서-시작하기-activateaccept)를 선언한 자식은 제외합니다. `start`가 있는 자식은 부모가 `active`인 동안에만 술어를 평가합니다(유효한 게이트는 두 조건의 연접). 참조되지 않은 퀘스트는 오늘날의 의미를 그대로 유지합니다. |

프로젝트의 부모→자식 트리는 **작성하는 것이 아니라 파생됩니다**: `ObjectiveEntry.quest`가 아티팩트마다 참조를 기록하고, 엔진은 `relations`/`rules`/`prereqEdges`를 합치는 것과 정확히 같이 모든 아티팩트의 필드를 합칩니다. 다단계 트리는 자연스럽게 생깁니다 — 자식이 스스로 `quest=` 목표를 가질 수도 있습니다 — 아래의 구조 검사만으로 제한됩니다. 자식 id는 평평하고 프로젝트에서 유일하게 유지되며 `parent.child` 네임스페이스는 없습니다.

형태를 지키는 진단은 다섯 가지입니다:

- `E-OBJECTIVE-QUEST-DONE` — 한 목표에 `quest=`와 `done=`이 함께 있습니다.
- `E-QUEST-REF-UNKNOWN` — `quest=`가 알려진 퀘스트 id가 아닌 것을 이름 붙입니다. `after` 대상과 같은 분할입니다: 같은 문서 참조는 `check`에서 해석되고, 문서를 가로지르는 해석은 프로젝트 전체 `lute check`가 합니다.
- `E-QUEST-MULTI-PARENT` — 하나의 퀘스트가 두 부모에서 `quest=`로 참조됩니다(그래프가 아닌 트리).
- `E-QUEST-TREE-CYCLE` — 부모→자식 간선이 순환을 이룹니다. 자기 참조는 길이 1인 순환이며 부모와 자식이 같은 문서를 공유하면 `check`가 일찍 발견합니다.
- `E-QUEST-TIER-MIX` — 서브퀘스트의 `tier`가 부모와 다릅니다. 오류는 두 퀘스트와 두 tier를 모두 이름 붙입니다. 역시 분할은 같습니다. 부모와 자식이 같은 문서면 `check`가 보고하고, 문서를 가로지르면 프로젝트 전체 `lute check`가 보고합니다. 혼합된 트리는 영원히 잠길 수 있습니다. `tier="run"` 부모 아래의 user-tier 자식은 부모의 종료 때 상태를 유지하므로, 부모가 다음 런에 다시 시작되어도 자식이 다시 active가 되지 않습니다. user-tier 부모 아래의 run-tier 자식은 매 새 런마다 `unset`으로 초기화되지만 부모는 상태를 유지하므로, 부모가 끝난 뒤에는 자식이 다시 활성화되지 않습니다. 두 퀘스트에 같은 `tier`를 주세요.

도달 가능성은 참조를 따라 전파됩니다. 자식이 `E-QUEST-UNREACHABLE`인 필수 서브퀘스트 목표는 목표의 `quest=` 범위에 대해 `E-OBJECTIVE-UNSATISFIABLE`을 보고합니다. 전체 엔진 처리(두 방향의 cascade, 활성화, 트리 재구성)는 [quest-lifecycle.md](https://github.com/journeyWorker/lute/blob/main/docs/runtime/quest-lifecycle.md)에 있고, 작동 예시는 [`docs/examples/quest-subquest.lute`](https://github.com/journeyWorker/lute/blob/main/docs/examples/quest-subquest.lute)입니다.

#### 대화에서 시작하기: `activate="accept"`

`start`가 없는 자식은 부모가 활성화되는 순간 일지에 들어갑니다. 플레이어가 대화에서 선택하는 우회 경로는 제시될 때만 들어가야 합니다. 자식의 **`activate="accept"`**(dsl 0.24.0 §2)는 `::accept`(또는 `accepts:` mock, `lute trace --accepted-quest`)를 기다리게 하며, `start`가 없는 루트 퀘스트와 같습니다. 부모가 `active`인 동안에만 활성화됩니다. 부모가 active가 아닐 때 실행된 수락은 효과 없이 소모되며 도구가 이를 알려 줍니다. `lute play`는 `note: accept of quest firewood spent — its parent quest camp is not active yet …`를 출력합니다(`--json`: `acceptSpent` 레코드). `lute trace` / `lute test`도 `accepts:` mock에 대해 같은 사실을 표시합니다.

#### 대안: `complete="any"`

"설득, 통행료, 또는 무력으로 강을 건넌다"는 세 경로를 가진 하나의 목표입니다. **`complete="any"`**를 선언하는 부모(dsl 0.24.0 §2)는 자식 퀘스트든 일반 목표든 필수 목표 중 **하나**가 done이면 완료됩니다. 아직 active인 다른 자식은 `failedBy: superseded`로 실패하므로 플레이어가 선택하지 않은 경로가 스스로 닫히며, 누구도 형제의 `fail=`에 cascade를 복사하지 않습니다. 기본값은 `complete="all"`입니다.

```lute check
---
kind: quest
title: The river crossing
state:
  run.talked: { type: bool, default: false }
  run.paid: { type: bool, default: false }
  run.won: { type: bool, default: false }
---

<quest id="crossing" title="Cross the river" start="true" complete="any">
  <objective id="byParley" title="Talk your way across" quest="parley"/>
  <objective id="byToll" title="Pay the toll" quest="toll"/>
  <objective id="byForce" title="Force the bridge" quest="force"/>
</quest>

<quest id="parley" title="Parley with the warden" activate="accept">
  <objective id="terms" title="Win the warden over" done="run.talked"/>
</quest>

<quest id="toll" title="Pay the toll" activate="accept">
  <objective id="pay" title="Pay the ferryman" done="run.paid"/>
</quest>

<quest id="force" title="Force the bridge">
  <objective id="fight" title="Beat the guards" done="run.won"/>
</quest>
```

`force`에는 `activate`가 없으므로 `crossing`과 함께 활성화됩니다. `parley`와 `toll`은 씬에서 `::accept`할 때까지 기다립니다. 플레이어가 `toll`을 수락한 다음 `parley`를 수락하고 감시관을 설득했다고 합시다. `parley`가 완료되고 `crossing`이 완료되며, 아직 열린 경로인 `toll`과 `force`는 superseded로 실패합니다. 플레이어가 `toll`을 수락하지 않았다면 `unset`으로 남습니다. `lute play`는 다음과 같이 정착을 출력합니다:

```
  parley.terms done
  quest parley -> complete
  crossing.byParley done
  quest crossing -> complete
  quest toll -> failed (superseded)
  quest force -> failed (superseded)
```

실패한 경로 하나가 나머지를 닫지는 않습니다. `complete="any"` 퀘스트의 합성된 `fail`은 필수 목표에 대한 **연접**입니다. 각 자식은 `quest.<c>.state == 'failed'`, 각 일반 목표는 `quest.<q>.objectives.<o>.failed` 플래그로 표현됩니다. 따라서 다른 경로가 하나도 남지 않을 때만 퀘스트가 실패합니다(`failedBy: fail`).

수락 쪽의 형태를 지키는 진단은 세 가지입니다. `start` 옆의 `activate="accept"`는 `E-ATTR-TYPE`입니다(하나를 고르세요). 부모와 함께 활성화되는 자식에 대한 `::accept`는 예전에는 아무 일도 하지 않았지만, 이제 프로젝트 전체 `lute check`에서 부모를 이름 붙이는 `E-ACCEPT-TARGET`입니다. 그리고 `W-QUEST-NEVER-ACCEPTED`는 `::accept`가 도달하지 않는 `activate="accept"` 자식을 표시합니다:

```lute expect="E-ATTR-TYPE"
---
kind: quest
title: The river crossing
state:
  run.talked: { type: bool, default: false }
---

<quest id="crossing" title="Cross the river" start="true">
  <objective id="byParley" title="Talk your way across" quest="parley"/>
</quest>

<quest id="parley" title="Parley with the warden" activate="accept" start="true">
  <objective id="terms" title="Win the warden over" done="run.talked"/>
</quest>
```

<!-- lute-diagnostics unverified="verbatim whole-project lute check output shape (names from this example); the W-QUEST-NEVER-ACCEPTED message is assembled from several pieces in crates/lute-check/src/accept.rs, so no single format! literal matches" -->
```
scenes/ferry.lute:17:21: error [E-ACCEPT-TARGET] `::accept` targets quest `force`, which activates with its parent `crossing`; declare `activate="accept"` on it to accept it from a scene
quests/crossing.lute:20:12: warning [W-QUEST-NEVER-ACCEPTED] quest `toll` waits for an acceptance (`activate="accept"`), but no `::accept` or reward `accepts` in the project names it, so it never activates; accept it from a scene with `::accept{quest="toll"}`, declare `accept="external"` if the engine accepts it outside the script (a quest board, a menu), or remove `activate="accept"` so it activates with its parent `crossing`
```

### `<on>`을 이용한 생명주기 반응

`<on event>`은 언어의 이벤트-조건-액션 규칙입니다. 이벤트가 발생하고 선택적 `when` 가드가 성립하면 분기의 노드가 출력됩니다. 세 가지 기본 **생명주기 이벤트**는 퀘스트 범위에 속합니다 — **`questActive`**, **`questComplete`**, **`questFailed`** — 각 이벤트는 자신을 감싼 퀘스트에서만 발생합니다. 월드 이벤트(예: `combatEnd`)는 플러그인이 기능으로 제공합니다.

대상 계기이기도 한 월드 이벤트는 하나의 대상에 대해서만 답할 수 있습니다(dsl 0.24.0 §2). `<on event="talk" target="npc.maud">`는 `talk`가 `npc.maud`를 위해 발생할 때 실행되고, 다른 대상이나 일반 `event:` 스텝 또는 생명주기 전환에서는 실행되지 않습니다:

```lute
<on event="talk" target="npc.maud">
  @maud: Coin first, talk later.
</on>
```

대상은 [목표](#퀘스트가-씬-및-계기를-만나는-방식)의 대상과 같이 검사됩니다(`E-BEAT-ATTR`). 따옴표로 감싼 점 구분 id여야 하고, 이벤트로 이름 붙인 계기가 대상을 받아야 하며, 대상은 그 도메인 안에 있어야 합니다. 생명주기 이벤트는 대상에 대해 절대 발생하지 않으므로 `questActive`, `questComplete`, `questFailed`에 `target`을 쓰는 것도 `E-BEAT-ATTR`입니다. `target`이 없으면 핸들러는 이벤트의 모든 발생에 답합니다.

핸들러 본문은 자신의 문서에 있는 다른 퀘스트를 `::accept`할 수 있으므로, 하나를 시작하면 다음 것을 열 수 있습니다:

```lute
<quest id="salvage" title="Salvage the wreck" accept="external">
  <objective id="dive" title="Dive the wreck" done="run.dived"/>
  <on event="questActive">
    @narrator: The wreck is marked on your map.
    ::accept{quest="tides"}
  </on>
</quest>
```

`lute play`는 그러한 수락을 적용합니다. dsl 0.26.0 §7부터 `lute trace`와 `lute test`도 적용하므로, `accepts: [salvage]`로 이 문서를 테스트하면 `play`와 같이 `tides`가 active인 상태로 끝납니다. 0.26.0 전에는 trace와 test가 적용하지 않아 play와 결과가 달랐습니다.

실패할 수 없는 퀘스트의 `questFailed` 핸들러는 데드 코드입니다. 프로젝트 전체 `lute check`는 퀘스트에 `fail`이 없고, `by=` 또는 `until=` [기한](#기한)이 있는 필수 목표도 없고, 스스로 실패할 수 있는 필수 서브퀘스트 목표도 없고(실패하는 필수 자식은 부모도 실패시킴), 부모 퀘스트도 없을 때 핸들러의 `event`에서 `W-QUEST-HANDLER-DEAD`를 경고합니다(부모가 끝나면 아직 active인 자식을 cascade로 실패시킴). `fail=` 조건을 추가하거나 핸들러를 제거하세요:

<!-- lute-diagnostics -->
```
./q.lute:10:14: warning [W-QUEST-HANDLER-DEAD] `<on event="questFailed">` never runs: quest `lampOut` cannot fail — it has no `fail` condition, no required objective with a `by=` deadline, no required subquest that can fail (a failing required child fails it), and no parent quest whose end would cascade to it; add a `fail=` condition or remove the handler
```

같은 코드(dsl 0.24.0)는 완료된 퀘스트에서만 실행될 수 있는 월드 이벤트 핸들러도 이름 붙입니다. 즉 `when`에 모든 필수 목표의 `done`이 포함되는 핸들러입니다(`complete="any"`에서는 하나, `@def`는 확장됨). 그 `when`이 성립하면 퀘스트가 즉시 `complete`로 정착하고, 이벤트는 active 퀘스트의 핸들러에만 도달합니다. `lute check`도 이를 보고합니다. 생명주기 이벤트와 `on=` 또는 `quest=` 목표가 있는 퀘스트는 제외됩니다:

<!-- lute-diagnostics unverified="verbatim whole-project lute check output; the message is composed in crates/lute-check/src/reachability.rs, which names the code through the project_check::W_QUEST_HANDLER_DEAD constant rather than a string literal, so the scraper cannot pair quote and code" -->
```
quests/road.lute:25:14: warning [W-QUEST-HANDLER-DEAD] `<on event="combatEnd">` never runs: its `when` implies every required objective of quest `brawl` is done, so the quest has already completed when it holds, and an event reaches only an active quest's handlers; move the body to `<on event="questComplete">` or the objective's own body
```

다른 곳의 콘텐츠도 예약된 `quest.<id>.state` 경로를 읽어 퀘스트 생명주기를 게이트할 수 있습니다. 이 경로는 **항상 할당됩니다**: 퀘스트가 활성화될 때까지 `unset`, 그다음 `active`, `complete`, 또는 `failed`입니다. 따라서 읽기에 `has` 가드가 필요하지 않습니다(`has(quest.<id>.state)`는 항상 참이며 `W-QUEST-STATE-HAS`). `when="quest.rescueHalsin.state == 'complete'"`은 완료 가드이고, "아직 시작되지 않음"은 `quest.rescueHalsin.state == 'unset'` 또는 `<when is="unset">` 분기입니다:

```lute
<match subject="quest.rescueHalsin.state">
  <when is="complete">
    @shadowheart: You did well back there.
  </when>
  <when is="failed">
    @shadowheart: We were too late.
  </when>
  <otherwise>
    @shadowheart: We should keep moving.
  </otherwise>
</match>
```

퀘스트는 관계형 팩트로도 게이트할 수 있습니다 — `start="holds('inParty', ['shadowheart'])"` — 팩트 표면은 [팩트와 Datalog](/ko/state/facts-and-datalog/)을 참고하고, 전체 사용 예시는 [`docs/examples/quest-rescue-halsin.lute`](https://github.com/journeyWorker/lute/blob/main/docs/examples/quest-rescue-halsin.lute)에 있습니다.

### 퀘스트가 실패한 이유

`failed`는 퀘스트가 나쁘게 끝났다는 뜻이지 이유는 말하지 않습니다. 예약된 읽기 전용 경로 두 가지가 이를 알려 줍니다(dsl 0.24.0 §2):

- **`quest.<id>.failedBy`**는 퀘스트가 실패할 때까지 `unset`을 읽고, 그 뒤 `fail`(`fail` 조건이 성립함), `by` 또는 `until`(필수 목표가 [기한](#기한)을 놓침), `subquest`(필수 서브퀘스트가 실패함), `cascade`(부모가 active인 자식이 있는 채 종료됨), 또는 `superseded`([`complete="any"`](#대안-completeany) 부모가 다른 경로로 완료됨) 중 하나를 읽습니다.
- **`quest.<id>.objectives.<oid>.failed`**는 목표의 `by` 또는 `until`이 목표를 실패시킨 뒤 `true`, 그 전에는 `false`를 읽습니다.

모든 조건 또는 보간은 이를 읽을 수 있으므로 에필로그가 플레이어가 버린 경로와 놓친 기한을 구별할 수 있습니다:

```lute check
---
kind: scene
id: river.epilogue
title: Across the river
---

## Epilogue

<match subject="quest.toll.failedBy">
  <when is="superseded">
    @narrator: The ferryman is still waiting for a coin that never came.
  </when>
  <when is="by|until">
    @narrator: You reached the ferry too late.
  </when>
  <when is="fail|subquest|cascade">
    @narrator: The ferry burned before you could pay.
  </when>
  <when is="unset">
    @narrator: The river is behind you.
  </when>
</match>

@narrator{when="quest.letter.objectives.thank.failed"}: Maud's letter is still in your coat.
```

`failedBy`는 이 일곱 값의 enum이므로 위 match는 `<otherwise>` 없이도 모든 경우를 다룹니다. 오타가 난 arm(`is="superceded"`)은 `E-WHEN-LITERAL-DOMAIN`입니다. 놓친 목표와 `fail`이 같은 정착에서 퀘스트를 실패시키면 목표의 종류가 우선합니다. 어느 경로든 `state:`에 선언하면 `E-QUEST-RESERVED-DECL`이고, `::set`으로 쓰면 `E-QUEST-RESERVED-WRITE`입니다. run-tier 퀘스트의 `newRun` 초기화는 둘 다 지웁니다. `lute run`, `lute play`,
 `lute trace`, `lute test`는 이 값에 동의하며, `lute run --json`은 실패한 `quest` 레코드에 `failedBy`를 담습니다.

0.49.0부터 프로젝트 전체 `lute check`는 조건뿐 아니라 이런 읽기를 모두 확인합니다. 줄이나 선택지 라벨의 `{{ quest.<id>.state }}` 보간, 보간 인자, `defs:`에 있는 def의 `cel:` 본문에서 그 id의 퀘스트가 프로젝트에 없으면 각각 `E-QUEST-REF-UNKNOWN`입니다(``quest.nope.state references quest nope, which no project quest defines``). 0.52부터(#212) 같은 검사는 [라벨 문법](/ko/language/dialogue-and-cast/#라벨과-프롬프트)을 따르는 모든 값, 즉 `<branch>`/`<hub>` 프롬프트와 플러그인 디렉티브의 `text` 속성도 읽으므로, `::banner{… subtitle="{{quest.finalle.state}}"}`도 줄에 쓴 같은 읽기처럼 보고됩니다.

### `quest.<id>.activatedAt`

`quest.<id>.state` 옆에는 모든 퀘스트가 두 번째 예약 경로 **`quest.<id>.activatedAt`**도 가집니다. 엔진이 `unset` → `active` 전환에서 찍는 `narrativeTime` 순간입니다. `quest.<id>.state`와 마찬가지로 엔진이 채우므로 작성자가 선언할 수 없고(`E-QUEST-RESERVED-DECL`), 작성자가 쓸 수도 없습니다(`E-QUEST-RESERVED-WRITE`). 하지만 콘텐츠는 이를 *읽을* 수 있으며 이것이 핵심입니다. `validAt(rel, t)`에는 없었던 시간 앵커입니다.

```lute
<objective id="visitedSinceAccept" title="Go back to the station" done="holds('arrivedSpace', ['station_front']) && !validAt('arrivedSpace', ['station_front'], quest.theCoffeeDebt.activatedAt)"/>
```

태그와 그 모든 속성은 **하나의 물리적 줄**에 있어야 하므로, 가독성을 위해 긴 `<objective …/>`를 줄바꿈하지 마세요 — 파싱 오류가 됩니다. 형태에 유의하세요: `validAt`는 *시점* 질의이며, 해당 순간에 팩트가 유효했을 때만 참입니다. 따라서 "활성화 이후"는 "지금 참이고 활성화 시점에는 아직 참이 아니었음"이라는 연접입니다 — `validAt(R, …activatedAt)`만 쓰면 반대 질문을 하게 됩니다. `validAt` 바깥에서 narrative-time 값에는 순서 비교 `<`, `<=`, `==`, `>`, `>=`만 허용됩니다. `!=`는 `E-TEMPORAL-ARG`이며, 그 값에 대한 산술도 마찬가지입니다. 구간 의미론은 [팩트와 Datalog](/ko/state/facts-and-datalog/)을 참고하세요.

`activatedAt`는 `E-MAYBE-UNSET`에서도 제외됩니다. 이 값에 maybe-unset 판정을 내리면 해소할 수 없기 때문입니다. `narrativeTime`에는 값을 담는 리터럴이 없으므로 `default:`를 줄 수 없고, 두 가드 형태 — `has(p)`와 `has(p)` — 자체가 narrative-time 피연산자에 대한 `E-TEMPORAL-ARG`입니다. (`quest.<id>.state`는 항상 할당되므로 다른 이유로 제외됩니다.) 엔진은 활성화된 모든 인스턴스에 스탬프가 존재하도록 보장하며, 이 읽기는 하나의 활성화된 인스턴스 안에서만 의미가 있습니다.

## 퀘스트 JSON 보고서

`lute scenario --json`은 `command`, `schemaVersion: "0.43.0.scenario"`, `ok`를
포함한 보고서 객체 하나를 반환하며, `scenario reach --json`의 버전은
`0.43.0.scenario-reach`입니다. 컴파일 아티팩트와 `run --json`은 별도의 계약이므로 봉투로
감싸지지 않습니다. 자동화에서는 앵커, 간선, 퀘스트 상태를 읽기 전에 봉투를 먼저 검사하세요.
재현 가능한 예제에는 `LUTE_TELEMETRY=0`을 사용하세요.
