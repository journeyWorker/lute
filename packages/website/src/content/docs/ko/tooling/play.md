---
title: 스토리 플레이
description: "계기(occasion)와 비트(beat, dsl 0.21.0) — 어떤 스토리 조각이 어떤 엔진 순간에 응답하는지 프로젝트가 선언하는 방법 — 그리고 스크립트로 적은 플레이스루를 프로젝트 전체에 걸쳐 걷는 참조 플레이어 `lute play`: 발생시킨 계기, 엔진 자신의 쓰기, 출발점이 되는 세이브, `lute test`가 실행하는 단언(dsl 0.22.0)."
---

내러티브 게임은 저마다의 순간에 다음 스토리 조각을 고릅니다: 허브 방문, 방 입장, NPC와의 대화, 새로운
하루, 새 런의 시작. Lute는 그런 순간을 **계기(occasion)**, 그 순간에 응답하는 스토리 조각을
**비트(beat)** 라고 부릅니다(dsl 0.21.0). 엔진이 계기를 발생시키고, Lute는 어떤 비트가 자격이 있고 어느
비트가 이기는지를 정의합니다. `lute play`는 이 계약의 참조 플레이어입니다: 발생시킬 계기를 적은
스크립트를 주면 프로젝트 전체를 걸으며 모든 후보 비트와 그 판정, 승자를 출력하고, 승자를 `lute run`과
같은 참조 러너로 재생합니다.

0.22.0부터 플레이 스크립트는 엔진의 나머지 역할도 대신합니다. `engine:` 스텝은 엔진이 소유한 상태와
팩트 — 넘어가는 하루, 런 카운터, 처치 기록 — 를 쓰고, 스크립트는 세이브에서 시작할 수 있으며, 결정은
스텝마다 달라질 수 있고, 스텝이 월드 이벤트를 발생시킬 수 있으며, `expect:`는 플레이스루를
[`lute test`](/tooling/cli/#test)가 시나리오 테스트와 나란히 실행하는 단언으로 바꿉니다. 이제 엔진을
흉내 내기 위해서만 존재하는 씬, 계기, 플러그인은 필요 없습니다.

규범 텍스트는 [0.21.0 제안서](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.21.0.md)이고,
[0.22.0 제안서](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.22.0.md)(플레이·테스트
하네스)가 이를 확장합니다. 엔진 측 계약(IR 필드와 엔진이 구현하는 선택 알고리즘)은
[`docs/runtime/beats-and-occasions.md`](https://github.com/journeyWorker/lute/blob/main/docs/runtime/beats-and-occasions.md)입니다.
0.21.0 이전의 `lute play`는 틱 클록 스케줄 파일을 걸었습니다. 그 레이어와 클록/레인/배치 모델, 그리고
관련 플래그는 모두 제거되었습니다. 이제 시간은 틀이 아니라 비트 조건의 입력 중 하나입니다.

## 계기

계기는 **엔진 어휘**입니다: 엔진이 발생시키는 이름 붙은 순간이며, 선택적으로 어떤 대상을 *위해*
발생합니다(`talk` → `npc.achilles`). 플러그인은 `occasions` export로 자신의 계기를 선언합니다 —
`plugin.yaml`에 `exports: { occasions: occasions/ }`로 나열하고, 맵은 `occasions/*.yaml`에 둡니다:

```yaml
occasions:
  hubVisit:  { select: first }
  talk:      { select: first, target: { prefix: npc, entity: person } }
  roomEnter: { select: first, target: true }
  inbox:     { select: all, description: Letters waiting at the fountain }
```

- `select: first`(기본값) — 엔진은 이긴 비트 하나를 제시합니다.
- `select: all` — 엔진은 자격 있는 비트를 모두 선택 순서대로 제시하고 플레이어가 하나를 고릅니다(메시지
  수신함, 지역 지도, "순서 무관" 퀘스트 제공자).
- `target: { prefix, entity }`(dsl 0.22.0) — 계기가 무언가를 **위해** 발생하며, 그 대상이 하나의
  어휘가 됩니다: `<prefix>.<member>`이고 `<member>`는 `entities:` 종류 `entity`의 멤버입니다(`open:`
  종류라면 아무 id). 스키마가 `person: { members: [achilles, patroclus] }`를 선언하면 위의 `talk`는
  `npc.achilles`나 `npc.patroclus`를 위해 발생합니다.
- `target: true` — 대상을 위해 발생하지만 모양만 검사합니다: 점으로 구분된 아무 id. 기본값 `false`는
  아무것도 위하지 않고 발생합니다.
- `description` — 도구용 선택적 설명.

해석된 플러그인 중 계기를 선언한 것이 없으면 계기 이름은 **모양만(shape-only)** 검사됩니다: 어떤
식별자든 받아들이므로, 엔진 플러그인이 생기기 전에도 비트를 쓸 수 있습니다. 어느 플러그인이든 계기를
선언하는 순간, 선언되지 않은 계기를 가리키는 비트는 `E-OCCASION-UNKNOWN`이 되고, 대상 없이 선언된
계기에 `target`을 붙이면 `E-BEAT-ATTR`입니다. 대상 도메인이 있는 계기에서 그 밖의 비트 대상 —
`npc.achilles` 대신 쓴 `npc.achiles` — 은 did-you-mean과 함께 `E-BEAT-ATTR`이며, 프로젝트가 선언하지 않은 엔티티
종류를 가리키는 도메인도 마찬가지입니다. 이 export는 가드된 섹션으로 캐퍼빌리티 스냅샷에 접히므로,
플러그인이 계기를 선언하지 않는 프로젝트는 `capabilityVersion`이 그대로입니다. 대상 도메인은 스냅샷의
일부이며, `target: true`는 0.21과 정확히 같게 해시됩니다.

계기에서 판정되는 퀘스트 목표(`<objective on="runEnd">`)는 `on=`만 받습니다. 0.22.0에서 목표는 대상을
갖지 않습니다.

## 비트

### 씬 비트

씬은 프런트매터에 자신이 응답하는 계기를 적으면 비트가 됩니다:

```yaml
---
kind: scene
id: achilles.gift
on: talk
target: npc.achilles
when: 'user.runs >= 10'
priority: 50
once: user
---
```

| 키 | 의미 |
|---|---|
| `on` | 이 씬이 응답하는 계기. 씬을 비트로 만듭니다. |
| `target` | 선택. 계기가 이 대상을 위해 발생했을 때만 씬이 후보가 됩니다(점으로 구분된 id, `<entry target>`과 같은 모양. 계기가 대상 도메인을 선언했다면 `<prefix>.<member>`). |
| `when` | 선택. `run` / `user` / `app` 상태, `quest.*`, `entry.<id>.read` / `entry.<id>.everRead`, 팩트 질의에 대한 CEL 조건. 씬 자신의 `scene.*` 상태는 아직 존재하지 않으므로 거부됩니다. |
| `priority` | 선택 정수, 기본값 `0`. 높을수록 이깁니다. |
| `once` | `run`(기본값 — 런마다 한 번), `user`(평생 한 번), 또는 `false`(반복 가능). |

`after:`는 의미가 그대로입니다 — [연결성](/connectivity/scene-graph/) 분석이 다루는 `visited` /
`completed` / `active`에 대한 구조적 전제조건 — 그리고 비트는 `after:`와 `when`이 모두 성립할 때만
자격이 있습니다. `on` 없이 `when`, `target`, `priority`, `once`를 쓰면 `E-BEAT-ATTR`입니다: 어떤 계기에도
응답하지 않는 씬은 이전처럼 명시적인 흐름으로 도달합니다.

### 엔트리 비트

[로어 엔트리](/language/lore-entries/)는 기존의 `target`, `when` 옆에 `on=`과 `priority=`를, 그리고
0.22.0부터는 선택적인 `once=`를 적어 계기에 응답합니다:

```lute
<entry id="achillesBark" on="talk" target="npc.achilles" category="bark" priority="10" once="run" when="user.runs >= 3">
  @achilles: Back again, lad.
</entry>
```

`once`가 없는 엔트리는 반복 가능합니다: 다시 제시되는 것이 엔트리의 본성이며, 효과는 첫 읽기에만
적용됩니다. `once="run"`이면 run 등급 `entry.<id>.read` 플래그가 세워진 뒤로 자격을 잃으므로 런마다 한
번 들립니다 — `newRun`이 플래그를 초기화합니다. `once="user"`이면 `entry.<id>.everRead`가 세워진 뒤로
자격을 잃습니다: `read` 옆에 있는 예약된 **user 등급** 플래그로, 첫 읽기에 세워지고 새 런에도 초기화되지
않으며, `entry.<id>.read`를 읽을 수 있는 곳이면 어디서든 읽을 수 있습니다. 잘못된 `once`, 또는 `on` 없는
`once`는 `E-BEAT-ATTR`입니다.

## 선택

엔진이 계기 `O`를, 선택적으로 대상 `T`를 위해 발생시키면:

1. **후보**는 `on: O`이고 `target`이 없거나 `T`와 같은 비트입니다. 대상 없이 발생한 계기에는 대상이
   없는 후보만 있습니다.
2. 후보는 `after:`가 성립하고(씬 비트), `when`이 성립하고, `once`가 소진되지 않았을 때 **자격이
   있습니다**. 씬의 `once`: `run` — 이번 런에 아직 제시되지 않음, `user` — 한 번도 제시되지 않음,
   `false` — 소진되지 않음. 엔트리의 `once`: `run` — `entry.<id>.read`가 세워지지 않음, `user` —
   `entry.<id>.everRead`가 세워지지 않음, 없음 — 소진되지 않음.
3. 자격 있는 비트는 **priority 내림차순, 그다음 프로젝트 순서**로 정렬됩니다: 문서 경로, 그다음 문서 안의
   선언 순서 — `project.index.json`의 `beats` 순서입니다. 같은 계기의 씬 비트와 엔트리 비트는 한 목록에서
   경쟁합니다.
4. `select: first`는 첫 번째 자격 있는 비트를 제시하고, `select: all`은 정렬된 목록을 제시한 뒤
   플레이어가 고른 것을 제시합니다.
5. **자격 있는 비트가 없으면** — 계기는 스토리 없이 지나가고, 그 순간에 대한 엔진의 기본 동작이
   적용됩니다.

선택은 결정적입니다: 같은 상태, 팩트, 제시 이력이면 어느 엔진에서든 같은 비트를 고릅니다. 가중 무작위나
쿨다운은 그 위에 얹는 엔진 정책이며, 참조 도구는 정확히 이 순서를 구현합니다. `select: first` 승자를
파일 순서가 정하는 경우 — priority가 같고 `when`이 서로 배타적임을 증명할 수 없는 두 비트 —
`check-project`는 `W-BEAT-PRIORITY-TIE`를 경고합니다. 이 경고와 다른 비트 권고는 [비트](/language/beats/)
문서를 보세요.

## `lute play`

```console
$ lute play <PROJECT_DIR> --script <FILE> [--json] [--no-derive] [--explain <ATOM>]…
```

- `<PROJECT_DIR>` — 프로젝트 루트(`lute.project.yaml`과 그 플러그인). 프로젝트는 `compile --all`과 같은
  게이트와 선언 유니온(씬, 퀘스트, 로어 문서)으로 메모리에서 통째로 컴파일됩니다.
- `--script <FILE>` — 필수: 플레이 스크립트, `*.play.yaml` 파일.
- `--json` — 같은 트랜스크립트를 stdout에 JSON 객체 하나로 출력합니다.
- `--no-derive` — 프로젝트의 Datalog 규칙을 적용하지 않습니다(dsl 0.22.0 §6). 스크립트의 `derive:`보다
  우선합니다. [파생과 `--explain`](#파생과---explain)을 보세요.
- `--explain <ATOM>` — 반복 가능: 플레이가 끝난 뒤, 그라운드 원자가 왜 성립하는지 또는 왜 성립하지
  않는지 출력합니다.

명령줄에서 따로 시드할 것은 없습니다: 상태, 팩트, 세이브, 결정, 단언이 모두 스크립트에 있으므로 하나의
플레이스루는 리뷰 가능한 파일 하나입니다.

이 절의 예제는 작은 로그라이크 프로젝트를 플레이합니다: 플레이어가 런마다 오르는 탑입니다. 플러그인은
계기 셋과 월드 이벤트 하나를 선언합니다(`events` export — [매니페스트](/plugins/manifests/) 참고):

```yaml
occasions:
  hubVisit: { select: first }
  talk:     { select: first, target: { prefix: npc, entity: person } }
  board:    { select: all, description: Notices pinned by the stair }
```

```yaml
events:
  - name: storm
```

월드 스키마는 엔진에게 층수와 런 카운터, 예약된 처치 팩트, 그리고 규칙 하나 — 워든은 처치될 때까지
위협이다 — 를 줍니다:

```yaml
state:
  run.floor: { type: number, default: 0, owner: engine }
  user.runs: { type: number, default: 0, owner: engine }
entities:
  person: { members: [maud, oskar] }
  foe:    { members: [warden, hound] }
relations:
  boss:   { args: [foe] }
  slew:   { args: [foe], tier: run, reserved: true }
  threat: { args: [foe], derive: true }
facts:
  - "boss(warden)"
rules:
  - "threat(F) :- boss(F), not slew(F)"
```

비트: `hub.idle`은 매번 `hubVisit`에 응답하고(`once: false`), `hub.victory`(priority 10,
`when: "!holds(threat(warden))"`, 역시 `once: false`)는 워든이 더 이상 위협이 아니게 되는 순간 그보다
앞섭니다. `maud.talk`는 `npc.maud`를 위한 `talk`에 응답하고, 엔트리 셋이
`board`에 응답합니다 — `notice`(`once="user"`), `memo`(`once="run"`), `old`
(`when="entry.notice.everRead"`). 퀘스트 문서 하나에 퀘스트 셋이 있고, 모두 `start="true"`입니다:

```lute
<quest id="climb" title="Reach the fifth floor" start="true" tier="run">
  <objective id="high" title="Reach floor five" done="run.floor >= 5"/>
  <on event="storm">
    @narrator: Thunder rolls over the stair.
  </on>
</quest>

<quest id="veteran" title="Three runs" start="true">
  <objective id="three" title="Climb three times" done="user.runs >= 3"/>
</quest>

<quest id="notices" title="Read the board" start="true">
  <objective id="looked" title="Look at the board" on="board" done="true"/>
</quest>
```

### 플레이 스크립트

플레이 스크립트는 YAML 매핑입니다. `steps`는 필수이고, 나머지 최상위 키는 모두 선택입니다:

| 키 | 의미 |
|---|---|
| `steps` | 필수, 비어 있으면 안 됨: 일어나는 일, 순서대로. |
| `state` | 시드: 상태 경로 → 스칼라 리터럴, 선언된 기본값 위에. |
| `facts` | 프로젝트의 시드 팩트에 더해지는 그라운드 팩트. |
| `choose` | branch/hub id → 선택지 id(hub의 방문 순서, 또는 매번 다르게 결정하는 branch라면 목록): 모든 제시에 대한 결정. |
| `visited`, `presented`, `quests`, `entriesRead` | 플레이가 출발하는 세이브 — [세이브에서 시작하기](#세이브에서-시작하기) 참고. |
| `expect` | 플레이 종료 시점에 대한 단언 — [기대값](#기대값) 참고. |
| `derive` | `false`면 프로젝트의 Datalog 규칙 적용을 멈춥니다 — [파생과 `--explain`](#파생과---explain) 참고. |

모든 스텝은 정확히 한 가지 일을 합니다 — `occasion`을 발생시키거나, `engine` 쓰기를 적용하거나,
`newRun`을 시작하거나, `event`를 발생시킵니다 — 그리고 어떤 스텝이든 `label`과 `repeat` 횟수를 가질 수
있습니다. 탑에 대해 모든 모양을 한 번씩 둘러보면:

```yaml
state: { user.runs: 2 }                   # path -> scalar literal, over the declared defaults
facts: ["slew(hound)"]                    # ground facts, added to the project's seed facts
entriesRead: { user: [notice] }           # the save this play starts from
steps:                                    # required, non-empty
  - occasion: hubVisit                    # raise an occasion
    expect: { winner: hub.idle }          # assert what this step did
  - occasion: talk
    target: npc.maud                      # a targeted occasion: a target in its domain
  - occasion: board
    pick: none                            # `select: all`: a beat id, or `none`
  - event: storm                          # fire a declared world event
  - label: the warden falls on floor six  # printed in the step header
    engine:                               # write what the engine owns
      state: { run.floor: 6, user.runs: { add: 1 } }
      facts: [slew(warden)]
  - newRun: { state: { run.floor: 1 } }   # start a new run (`newRun: true` without a seed)
  - occasion: hubVisit
    repeat: 2                             # the same step, twice
expect:                                   # assert the end of the play
  quests: { climb: active, veteran: complete }
  notFacts: [slew(warden)]
```

이 스크립트의 트랜스크립트는 [트랜스크립트](#트랜스크립트)의 예제입니다.

`state:`, `facts:`, `choose:`는 [`lute trace --mock`](/tooling/tracing/) 파일과 정확히 같은 문법을
씁니다.

- `state:` 시드는 선언된 경로 — `scene.*`는 안 됨 — 를 가리키며, 값은 선언된 타입에 맞아야
  합니다(`number`면 숫자, enum이면 멤버). 그 밖의 경우는 사용법 오류(종료 코드 2)입니다.
  `quest.<id>.state` 시드(`state: { quest.lostCup.state: active }`)는 `quests:` 항목과 똑같이 처음부터
  그 퀘스트의 라이프사이클 상태가 됩니다.
- `facts:` 항목은 선언된 비파생 관계의 그라운드 원자로, 인자 수가 맞고 닫힌 인자 도메인의 멤버를 써야
  합니다. **예약된(reserved)** 관계도 허용됩니다 — 그것을 단언하는 주체가 바로 엔진입니다.
- `choose:`의 결정 하나는 그 branch나 hub가 제시될 때마다 답합니다. **hub**의 목록은 방문 순서 하나이며
  제시될 때마다 다시 쓰입니다. **branch**에 두 개 이상의 목록을 주면 플레이스루 전체에 걸쳐 제시될
  때마다 순서대로 하나씩 소비됩니다. 그래서 사흘 밤 재생되는 씬이 밤마다 다르게 결정할 수 있습니다.
  목록이 바닥나면 워크는 그 사실을 알리며 미완료(종료 코드 3)로 멈춥니다.
- 그 순간 메뉴가 제시하지 않는 결정은 워크를 오류(종료 코드 1)로 멈춥니다: 가드가 거짓인 선택지, 또는
  이미 고른 `once` hub 선택지(`lute trace`와 같은 `E-TRACE-CHOICE`).

### 계기 스텝

`{ occasion, target?, pick?, choose?, expect? }`는 엔진과 똑같이 계기를 발생시킵니다.

- `target` — 대상과 함께 선언된 계기에는 필수, 대상 없는 계기에는 거부됩니다. 대상 도메인이 있으면
  대상은 그 도메인의 `<prefix>.<member>`여야 하며, 벗어나면 did-you-mean이 붙은 사용법 오류입니다(`` target `npc.mawd` is outside occasion `talk`'s domain `npc.<person>` (`npc.maud`, `npc.oskar`) — did you mean `npc.maud`? (dsl 0.22.0 §8) ``).
  어떤 비트도 응답하지 않는 멤버는 합법입니다: 계기가 그냥 지나갑니다.
- `pick` — `select: all` 계기에는 필수, `select: first`에는 거부됩니다: 그 계기에 응답하는 비트의
  id(그 순간 자격이 없는 pick은 오류, 종료 코드 1) 또는 `pick: none`. `none`은 목록을 닫습니다 —
  플레이어가 게시판을 지나칩니다: 아무것도 제시되지 않고 아무것도 소진되지 않지만, 그 계기의
  `<objective on>` 목표는 여전히 판정됩니다:

```yaml
steps:
  - occasion: board
    pick: none
```

```
── start ──────────────
  quest climb -> active
  quest veteran -> active
  quest notices -> active
── step 1 · board (select: all, pick: none) ──────────────
  ✓ notice [entry, priority 0]
  ✓ memo [entry, priority 0]
  ✗ old [entry, priority 0] — when: false
  → (pick: none — the list closes; nothing presented)
  notices.looked done
  quest notices -> complete
── end: complete (1 step) ──────────────
```

- `choose` — 이 스텝만의 결정. 이번 제시에서는 스크립트의 `choose:`를 키 단위로 대체하고, 스크립트의
  맵은 다른 모든 스텝의 기본값으로 남습니다. 스텝 로컬 목록은 첫 항목부터 시작하며, 같은 id에 대한
  스크립트 전역 목록의 소비 위치는 건드리지 않습니다. [예제](#한-스텝만-다르게-결정하기)에서 씁니다.
- `expect` — 이 스텝이 했어야 하는 일. [기대값](#기대값)을 보세요.

### 엔진 스텝

`{ engine: { state?, facts?, retract? } }`는 엔진이 계기 사이에 하듯이 엔진이 소유한 것을 씁니다:

- `state:` — 선언된 경로 → 리터럴, 또는 `number` 경로의 현재 값에 더하는 `{ add: <number> }`. 선언되지
  않은 경로, `scene.*`, `quest.*` 경로는 거부됩니다: 퀘스트 상태는 전이가 핸들러와 보상을 발동시키는
  퀘스트 라이프사이클의 것이니, 세이브의 퀘스트 상태는 최상위 `quests:`로 시드하세요.
- `facts:` / `retract:` — 선언된 기반 관계의 그라운드 원자, **예약된 관계 포함**. 최상위 `facts:`와 같은
  검사를 받습니다. 성립하지 않는 원자를 철회하면 거부되지 않고 기록됩니다.

쓰기는 그 순서대로 — 상태, 팩트, 철회 — 적용됩니다. 이 스텝은 아무것도 제시하지 않고 계기도 발생시키지
않으며, 바로 뒤에 퀘스트 라이프사이클이 정착하므로 쓰기 하나로 그 스텝에서 퀘스트가 완료되거나 실패할 수
있습니다. [`owner: engine`](/state/state-model/#owner-engine) 상태가 바로 이 스텝을 위한 것입니다:
콘텐츠는 그것을 `::set`할 수 없지만 `engine:` 스텝은 쓸 수 있습니다 — `scene.*`와 `quest.*`를 제외한
다른 선언된 상태도 마찬가지입니다.

```yaml
steps:
  - occasion: hubVisit
  - label: the warden falls on floor six
    engine:
      state: { run.floor: 6 }
      facts: [slew(warden)]
  - occasion: hubVisit
  - engine:
      retract: [slew(warden), slew(hound)]
  - occasion: hubVisit
```

```
── start ──────────────
  quest climb -> active
  quest veteran -> active
  quest notices -> active
── step 1 · hubVisit ──────────────
  ✓ hub.idle [scene, priority 0]
  ✗ hub.victory [scene, priority 10] — when: false
  → hub.idle
@maud: Quiet night.
── step 2 (the warden falls on floor six) · engine ──────────────
  set run.floor = 6
  assert slew(warden)
  climb.high done
  quest climb -> complete
── step 3 · hubVisit ──────────────
  ✓ hub.victory [scene, priority 10]
  ✓ hub.idle [scene, priority 0]
  → hub.victory
@maud: The warden is dead. I never thought I'd say it.
── step 4 · engine ──────────────
  retract slew(warden)
  retract slew(hound) (did not hold)
── step 5 · hubVisit ──────────────
  ✓ hub.idle [scene, priority 0]
  ✗ hub.victory [scene, priority 10] — when: false
  → hub.idle
@maud: Quiet night.
── end: complete (5 steps) ──────────────
```

처치로 `threat(warden)`이 더 이상 파생되지 않으므로 스텝 3에서 `hub.victory`가 자격을 얻고, 철회가
그것을 다시 닫습니다.

### 이벤트

`{ event: <name> }`은 어떤 플러그인이 선언한 월드 이벤트를 trace의 `events:`와 똑같이 발생시킵니다: 모든
**활성** 퀘스트의 `<on event="<name>">` 핸들러가 실행되고, 라이프사이클이 정착합니다. 계기를 가리키는
`event:`나 월드 이벤트를 가리키는 `occasion:`은 올바른 키를 알려 주는 사용법 오류입니다
(`` `event: dayEnd` names no declared world event — `dayEnd` is an occasion; raise it with `occasion: dayEnd` ``).
퀘스트 라이프사이클 이벤트 `questActive` / `questComplete` / `questFailed`는 전이 때 발생하며 스크립트가
발생시킬 수 없습니다.

```yaml
steps:
  - event: storm
  - engine: { state: { run.floor: 5 } }
  - event: storm
```

```
── start ──────────────
  quest climb -> active
  quest veteran -> active
  quest notices -> active
── step 1 · event storm ──────────────
@narrator: Thunder rolls over the stair.
── step 2 · engine ──────────────
  set run.floor = 5
  climb.high done
  quest climb -> complete
── step 3 · event storm ──────────────
── end: complete (3 steps) ──────────────
```

`climb`이 스텝 2에서 완료되었으므로, 그 핸들러는 두 번째 폭풍에 더 이상 응답하지 않습니다.

### 레이블과 반복

`label: <text>`는 스텝에 이름을 붙입니다: 스텝 헤더에 출력되고(`── step 4 (the engine closes the day) · engine`),
`--json`에 실리며, 그 스텝의 모든 기대값 불일치가 이 이름을 댑니다. `repeat: <n>`(1 이상의 정수)은 스텝을
`n`번 실행합니다 — `── step 7 [1/2] · hubVisit`, `── step 7 [2/2] · hubVisit` — 그리고 각 반복은 자신의
스텝 레코드이고, 퀘스트 라이프사이클을 따로 정착시키며, `── end: complete (<n> steps)`에 세어집니다.
`repeat`는 엔진의 일상에 어울립니다: 세 번의 런 종료(`engine: { state: { user.runs: { add: 1 } } }`,
`repeat: 3`), 또는 플레이어가 매일 하는 방문.

### 세이브에서 시작하기

최상위 키 네 개가 스텝 1 전에 플레이스루의 이력을 시드하므로, 스크립트는 그 앞을 모두 다시 재생하는
대신 실제 플레이어의 세이브가 있는 지점에서 시작할 수 있습니다:

| 키 | 의미 |
|---|---|
| `visited: [scene ids]` | 이 세이브에서 제시된 씬 — `visited('<id>')`와 `after: visited(…)`가 읽습니다. |
| `presented: { run: [beat ids], user: [beat ids] }` | 이미 제시된 씬 비트: `user` — 이전 런에서, 그래서 `once: user` 비트가 소진됨. `run` — 현재 런에서, 그래서 `once: run`과 `once: user`가 모두 소진됨. 나열된 모든 씬은 방문한 것으로도 셉니다. |
| `quests: { <id>: unset \| active \| complete \| failed }` | 퀘스트 라이프사이클 상태. 시작 정착은 퀘스트를 처음부터 다시 시작하지 않고 이 상태를 이어받습니다. |
| `entriesRead: { run: [entry ids], user: [entry ids] }` | `run` — 현재 런에서 읽음: `entry.<id>.read`와 `entry.<id>.everRead`. `user` — 이전 런에서 읽음: `entry.<id>.everRead`만. |

프로젝트가 선언하지 않은 id는 did-you-mean이 붙은 사용법 오류이며
(`` `visited:` names `hub.welcom`, which is no scene in this project — did you mean `hub.welcome`? ``),
`presented:` 아래의 엔트리(엔트리의 읽기 이력은 `entriesRead:`입니다)나 네 상태 밖의 값도 마찬가지입니다.

```yaml
quests: { veteran: complete }
entriesRead: { user: [notice], run: [memo] }
steps:
  - occasion: board
    pick: old
```

```
── start ──────────────
  quest climb -> active
  quest notices -> active
── step 1 · board (select: all, pick: old) ──────────────
  ✓ old [entry, priority 0]
  ✗ notice [entry, priority 0] — once: user — already read
  ✗ memo [entry, priority 0] — once: run — already read this run
  → old
  entry old (first read)
@maud: The same notice as ever.
  notices.looked done
  quest notices -> complete
── end: complete (1 step) ──────────────
```

`veteran`은 완료 상태로 남고 — 시작 시 다시 활성화되지 않습니다 — 세이브의 읽기 기록이 두 엔트리를
소진시키면서 `old`를 엽니다.

### 런 경계

`newRun: true`는 새 런을 시작합니다. 다음을 초기화합니다:

- `run.*` 상태를 선언된 기본값으로, 그리고 모든 run 등급 `entry.<id>.read` 플래그를. 그래서 새 런의 첫
  읽기에서 엔트리 효과가 다시 적용되고 `once="run"` 엔트리가 다시 자격을 얻습니다.
- run 등급 팩트를 프로젝트의 시드 팩트로.
- 모든 `<quest tier="run">` 퀘스트를 `unset`으로, 목표도 되돌려서. `start`가 있는 퀘스트는 뒤따르는
  정착에서 다시 활성화되고, 수락으로 시작하는 퀘스트는 새 수락을 기다립니다.
- `once: run` 소진 기록.

`user.*` / `app.*` 상태, user 등급 퀘스트(기본 `tier`), `entry.<id>.everRead`, user·app 등급 팩트,
`visited` 이력, `once: user` 소진 기록은 유지됩니다. 서로 다른 `lute play` 호출 사이의 `once: user`는
모델링되지 않습니다 — 여러 런을 한 스크립트에 넣고 `newRun` 스텝으로 나누거나, 세이브에서 시작하세요.

긴 형태 `newRun: { state: {…}, facts: […] }`는 그다음 쓰기를 새 런의 시드로 적용합니다 — `engine:`
스텝과 같은 `state:`(리터럴 또는 `{ add: n }`)와 `facts:` 규칙이며, `retract:`는 없습니다. 그다음 퀘스트
라이프사이클이 정착합니다.

```yaml
steps:
  - label: a run ends
    engine:
      state: { run.floor: 6, user.runs: { add: 1 } }
      facts: [slew(warden)]
  - newRun: { state: { run.floor: 1 } }
  - occasion: hubVisit
  - label: a run ends
    engine:
      state: { user.runs: { add: 1 } }
    repeat: 2
```

```
── start ──────────────
  quest climb -> active
  quest veteran -> active
  quest notices -> active
── step 1 (a run ends) · engine ──────────────
  set run.floor = 6
  set user.runs = 1
  assert slew(warden)
  climb.high done
  quest climb -> complete
── step 2 · new run ──────────────
  run.* state, run-tier facts and once: run reset
  quest climb -> unset (tier: run)
  set run.floor = 1
  quest climb -> active
── step 3 · hubVisit ──────────────
  ✓ hub.idle [scene, priority 0]
  ✗ hub.victory [scene, priority 10] — when: false
  → hub.idle
@maud: Quiet night.
── step 4 (a run ends) [1/2] · engine ──────────────
  set user.runs = 2
── step 4 (a run ends) [2/2] · engine ──────────────
  set user.runs = 3
  veteran.three done
  quest veteran -> complete
── end: complete (5 steps) ──────────────
```

`slew`는 run 등급 관계이므로 처치 기록은 새 런까지 살아남지 못하고 `hub.victory`는 다시 닫힙니다.
`climb`(`tier="run"`)은 처음부터 다시 시작하지만 `veteran`은 계속 셉니다.

런 경계를 넘는 엔트리 `once`, `board` 엔트리로:

```yaml
steps:
  - occasion: board
    pick: notice
  - occasion: board
    pick: memo
  - occasion: board
    pick: old
  - newRun: true
  - occasion: board
    pick: memo
```

```
── start ──────────────
  quest climb -> active
  quest veteran -> active
  quest notices -> active
── step 1 · board (select: all, pick: notice) ──────────────
  ✓ notice [entry, priority 0]
  ✓ memo [entry, priority 0]
  ✗ old [entry, priority 0] — when: false
  → notice
  entry notice (first read)
@maud: "Climbers wanted. No refunds."
  notices.looked done
  quest notices -> complete
── step 2 · board (select: all, pick: memo) ──────────────
  ✓ memo [entry, priority 0]
  ✓ old [entry, priority 0]
  ✗ notice [entry, priority 0] — once: user — already read
  → memo
  entry memo (first read)
@maud: "Floor three is flooded again."
── step 3 · board (select: all, pick: old) ──────────────
  ✓ old [entry, priority 0]
  ✗ notice [entry, priority 0] — once: user — already read
  ✗ memo [entry, priority 0] — once: run — already read this run
  → old
  entry old (first read)
@maud: The same notice as ever.
── step 4 · new run ──────────────
  run.* state, run-tier facts and once: run reset
  quest climb -> unset (tier: run)
  quest climb -> active
── step 5 · board (select: all, pick: memo) ──────────────
  ✓ memo [entry, priority 0]
  ✓ old [entry, priority 0]
  ✗ notice [entry, priority 0] — once: user — already read
  → memo
  entry memo (first read)
@maud: "Floor three is flooded again."
── end: complete (5 steps) ──────────────
```

### 기대값

계기 스텝은 `expect:`를 가질 수 있고, 그 스텝이 한 일에 대해 판정됩니다:

| 키 | 성립 조건 |
|---|---|
| `winner: <beat id>` | 그 비트가 제시됨. `winner: none` — 아무것도 제시되지 않음(자격 있는 비트가 없거나 `pick: none`) |
| `offered: [beat ids]` | 나열된 모든 비트가 그 스텝에서 자격이 있었음 — 부분집합, 순서 무관 |
| `notOffered: [beat ids]` | 나열된 비트 중 어느 것도 자격이 없었음 |

최상위 `expect:`는 플레이의 끝을 판정합니다:

| 키 | 성립 조건 |
|---|---|
| `exit: complete \| incomplete \| error` | 워크가 그렇게 끝남 |
| `quests: { <id>: <status> }` | 퀘스트가 그 상태로 끝남(아무것도 활성화하지 않은 퀘스트는 `unset`) |
| `state: { <path>: <value> }` | 경로의 최종 **유효** 값 — 마지막 쓰기, 없으면 시드, 없으면 선언된 기본값 — 이 그 값과 같음. 타입까지 비교(`1`은 `"1"`이 아님) |
| `facts: [atoms]` / `notFacts: [atoms]` | 각 원자가 끝에서, **파생 이후** 성립함 / 성립하지 않음 |
| `transcriptContains: [text]` / `transcriptLacks: [text]` | 각 텍스트가 사람이 읽는 트랜스크립트의 부분 문자열임 / 아님 |

`repeat:` 스텝의 기대값은 반복마다 판정되며, 워크가 도달하지 못한 스텝의 기대값은 그 자체로 불일치입니다.
각 `expect:`는 아무것도 재생하기 전에 검증됩니다 — 알 수 없는 키는 합법 키 목록과 did-you-mean을 붙인
사용법 오류(종료 코드 2)이며, 그 키가 다른 수준에 속하면 그렇다고 알려 줍니다
(`` unknown top-level `expect:` key `winner` (`winner` belongs in a step `expect:`) ``).

트랜스크립트 뒤에, 기대값이 있는 스크립트는 `── expect: every expectation held` 또는
`── expect: <n> missed`를 출력하고, 불일치마다 스텝, 레이블, 계기, 반복 번호, 실제 값을 댄 줄을 하나씩
출력합니다:

```
── expect: 3 missed ──────────────
  ✗ step 3 (ask about the oil) at talk npc.tomas: expect winner: expected tomasOil, actual tomasBusy
  ✗ end of play: expect quests lampOut: expected active, actual unset
  ✗ end of play: expect state user.bond.mara: expected 1, actual 0
```

불일치가 있으면 `lute play`는 종료 코드 1로 끝납니다. 단, 워크 자체가 이미 오류(종료 코드 1)나 잘못된
산출물로 인한 러너 실패(종료 코드 2)로 끝났다면 그 코드를 따릅니다. 모든 기대값이 성립했어도 미완료(3)로
멈춘 워크는 여전히 3으로 끝납니다.

`lute test`는 디렉터리 아래에서 `expect:`를 — 스텝에든 최상위에든 — 가진 모든 `*.play.yaml`을
`*.test.yaml` 시나리오 테스트와 나란히, `--project`에 대해 또는 없으면 플레이 위쪽의 가장 가까운
`lute.project.yaml`에 대해 실행하고, 각각 `PASS` / `FAIL` 줄을 출력합니다(`--json`: `"kind": "play"`와
`misses`를 가진 항목). `expect:`가 없는 플레이는 테스트가 아니므로 건너뜁니다. 멈춘 플레이는 최상위
`expect:`가 종료를 선언하지 않는 한(`expect: { exit: incomplete }`) 실패합니다. `--coverage`에서는 플레이가
제시한 모든 문서와, 라이프사이클을 움직인 모든 퀘스트 문서가 커버된 것으로 셉니다.
[`lute test`](/tooling/cli/#test)를 보세요.

### 파생과 `--explain`

`lute play`는 모든 `when`, `done`, 가드를 **프로젝트의 Datalog 규칙을 적용한**(계층화된 부정) 실시간
팩트에 대해 평가합니다 — `lute run`, 그리고 0.22.0부터 `lute trace`와 `lute test`가 쓰는 것과 같은
평가기입니다. 프로젝트의 시드 팩트가 로드되고, 스크립트의 `facts:`, `engine:` 쓰기, 제시 중의 모든
`::assert` / `::retract`가 고정점에 들어갑니다.

스크립트의 `derive: false`, 또는 명령줄의 `--no-derive`(플래그가 우선)는 규칙 적용을 멈춥니다: 시드
팩트는 여전히 로드되지만, 파생 관계의 원자는 모두 unknown으로 읽힙니다 — 스크립트는 그런 원자를 단언할 수
없습니다. 그런 원자에 의존하는 `when`은 워크를 미완료(종료 코드 3)로 멈추고, 플레이 종료 시점의
`facts:` / `notFacts:` 기대값은 기반 팩트만 봅니다:

```console
$ lute play tower --script tower/plays/night.play.yaml --no-derive
```

```
── start ──────────────
  quest climb -> active
  quest veteran -> active
  quest notices -> active
── step 1 · hubVisit ──────────────
  ✓ hub.idle [scene, priority 0]
  ? hub.victory [scene, priority 10] — when: unknown (`!holds(threat(warden))` evaluates unknown: fact `threat(warden)` is undetermined)
── halted: step 1: the `when` of scene `hub.victory` (scenes/hub-victory.lute) decides the hubVisit outcome but `!holds(threat(warden))` evaluates unknown: fact `threat(warden)` is undetermined ──────────────
```

`--explain <atom>`(반복 가능)은 플레이가 끝난 뒤 그라운드 원자가 끝에서 왜 성립하는지, 또는 왜 성립하지
않는지 출력합니다. 성립하면: 사용된 규칙과 각 전제의 근거 — `seed fact`, 플레이 중 `asserted`, 또는 다시
파생된 것이면 그 아래 들여쓰기로 — 가 나오고, 부정 전제는 `(absent)`로 표시됩니다. 성립하지 않으면: 그것을
결론지을 수 있는 모든 규칙이 전제 표시와 함께 나옵니다 — 없는 기반 팩트는 `✗ <atom>  (absent)`, 없는 파생
팩트는 `✗ <atom>  (not derived)`(그 자체도 설명됨), 존재하는 부정 전제는 `✗ not <atom>  (but it holds: …)`,
비교나 가드는 `✗ <test>  (false)` / `? <test>  (undecided)`, 첫 실패 뒤의 전제는
`· <premise>  (not reached)`. `plays/night.play.yaml`이 `hubVisit` 스텝 하나일 때:

```console
$ lute play tower --script tower/plays/night.play.yaml --explain "threat(warden)" --explain "threat(hound)"
```

```
── start ──────────────
  quest climb -> active
  quest veteran -> active
  quest notices -> active
── step 1 · hubVisit ──────────────
  ✓ hub.idle [scene, priority 0]
  ✗ hub.victory [scene, priority 10] — when: false
  → hub.idle
@maud: Quiet night.
── end: complete (1 step) ──────────────
explain threat(warden): holds
  threat(warden)  ⇐ threat(F) :- boss(F), not slew(F)
  ├─ boss(warden)  (seed fact)
  └─ not slew(warden)  (absent)
explain threat(hound): does not hold
  threat(F) :- boss(F), not slew(F)
  ├─ ✗ boss(hound)  (absent)
  └─ · not slew(hound)  (not reached)
```

`engine:` 스텝이 `slew(warden)`을 단언한 뒤에는 같은 원자가 이렇게 읽힙니다:

```
explain threat(warden): does not hold
  threat(F) :- boss(F), not slew(F)
  ├─ boss(warden)  (seed fact)
  └─ ✗ not slew(warden)  (but it holds: asserted)
```

설명은 `── expect:` 블록 앞에 출력되며, `--json`은 이를 `explain`에 담습니다. 그라운드가 아닌
원자(`knows(X)`, `slew(_)`)는 사용법 오류(종료 코드 2)입니다. `--explain`은 `--no-derive`에서도 최종 팩트와
상태에 대해 규칙을 평가합니다.

### 사용법 오류

다음 경우 스크립트는 아무것도 재생하기 전에 거부됩니다 — **사용법 오류, 종료 코드 2**, 스텝 번호를 댐:

- 읽을 수 없거나 잘못된 YAML, 알 수 없는 최상위 키, `steps`가 없거나 비어 있음.
- 스텝이 동작을 하나도 적지 않거나 둘 이상 적음, 알 수 없는 키, `occasion`이 아닌 스텝의 `target` /
  `pick` / `choose` / `expect`, 1 이상의 정수가 아닌 `repeat`.
- 계기 스텝이 해석된 플러그인 중 아무도 선언하지 않은 계기를 가리킴(어떤 플러그인이 계기를 선언한 경우),
  또는 모양만 검사하는 프로젝트에서 어떤 비트도 응답하지 않고 어떤 `<objective on>`도 판정하지 않는 계기를
  가리킴. 대상이 있는 계기를 `target` 없이, 또는 대상 없는 계기를 `target`과 함께 발생시킴. 계기 도메인
  밖의 대상. `select: first` 계기의 `pick`, `select: all` 계기의 `pick` 누락, 그 계기에 응답하지 않는
  비트를 고르는 `pick`.
- `event:`가 선언된 월드 이벤트를 가리키지 않거나, 퀘스트 라이프사이클 이벤트를 가리킴.
- `engine:`이나 `newRun` 쓰기가 선언되지 않았거나 `scene.*` / `quest.*`인 경로, 선언된 타입에 맞지 않는
  값, `number`가 아닌 경로의 `{ add: … }`, 또는 그라운드가 아니거나 선언되지 않았거나 파생된 관계를
  가리키거나 인자 수가 틀리거나 닫힌 도메인의 멤버가 아닌 팩트를 가짐. 또는 아무것도 쓰지 않음.
- `state:` / `facts:` 시드가 같은 검사에 실패하거나, 세이브 시드가 알 수 없는 id나 퀘스트 상태를 가리킴.
- `expect:`에 알 수 없는 키나 잘못된 값이 있음.

메시지는 키와 맞는 값을 알려 줍니다:
`` step 1: `engine.state.quest.climb.state`: quest state is written by the quest lifecycle, not the engine — seed a save's quest status with top-level `quests:` ``.

### 각 스텝이 하는 일

계기 스텝:

1. **후보** — 프로젝트의 비트 목록에서 `on`이 스텝의 계기와 같고 `target`이 없거나 스텝의 `target`과
   같은 모든 비트.
2. **판정** — 후보는 `once`가 소진되지 않았고(씬: `run` — 마지막 `newRun` 이후 제시되지 않음,
   `user` — 이 플레이나 세이브에서 한 번도 제시되지 않음, `false` — 소진되지 않음. 엔트리: `run` —
   `entry.<id>.read`가 세워지지 않음, `user` — `entry.<id>.everRead`가 세워지지 않음, `once` 없음 —
   소진되지 않음), `after:`가 성립하고(씬 비트; 제시된 씬의 **실시간** `visited` 집합과 실제 퀘스트
   상태의 `completed` / `active` 집합에 대해 평가), `when`이 성립할 때(참조 러너의 CEL 평가기가 실시간
   상태와 팩트에 대해, Datalog 규칙을 적용해 평가) 자격이 있습니다. `when`이 unknown으로 평가되면 —
   `validAt(…)`, `now()`, 또는 `--no-derive`에서의 파생 원자 — 그 비트를 이름 붙여
   **미완료(종료 코드 3)**로 정지합니다. 단, `select: first` 계기에서 확실히 자격 있는 비트가 그보다
   앞서면 승자를 바꿀 수 없으므로 정지하지 않습니다.
3. **순서** — 자격 있는 비트를 priority 내림차순, 그다음 프로젝트 순서로.
4. **선택** — 계기의 `select`는 해석된 플러그인의 `occasions` export에서 옵니다(선언되지 않은 계기는
   `first`). `first`: 첫 번째 자격 있는 비트가 이기며, 자격 있는 비트가 없으면 계기는 스토리 없이
   지나갑니다. `all`: 스텝의 `pick`이 제시되며, 그 순간 자격이 없는 pick은 **오류(종료 코드 1)**입니다.
   `pick: none`은 아무것도 제시하지 않습니다.
5. **제시** — 씬 비트는 참조 러너(`lute run`의 평가기)로 실행됩니다: `scene.*`는 씬 자신의 기본값으로
   초기화되고, `run.*` / `user.*` / `app.*` / `quest.*` 상태와 팩트는 이어지며, 스크립트의 `choose:` —
   그 위에 스텝 자신의 `choose:` — 가 branch와 hub를 결정합니다. 스크립트에 없는 결정은
   **미완료(종료 코드 3)**로 정지합니다. 엔트리 비트는 로어 엔트리 규칙으로 제시됩니다: 효과는 첫 읽기에만
   적용되고, 그 뒤 `entry.<id>.read`와 `entry.<id>.everRead`가 true가 됩니다. 씬의 `::end`는 플레이스루
   전체를 완료로 끝냅니다. 단, 그 스텝이 먼저 정산됩니다: 그 스텝의 퀘스트 진행(6)과 계기의 목표 판정(7)이
   실행된 뒤 워크가 멈춥니다. 씬의 `::accept{quest="<id>"}`는 `quest <id> accepted`를 출력하고, 퀘스트는
   제시 직후의 진행에서 활성화됩니다. 씬은 제시가 끝나면 — `after:`와 모든 조건의 `visited('<id>')`에
   대해 — 방문한 것으로 셉니다.
6. **퀘스트** — 매 제시 후, 모든 퀘스트 라이프사이클이 `lute run`이 퀘스트 산출물을 진행시키는 것과
   정확히 같게 진행됩니다: 활성화(`start`, 없으면 제시된 씬의 `::accept` — `start` 없는 퀘스트는 스스로
   활성화되지 않음), 목표 완료(단조적이며 목표 본문은 한 번만 재생), 완료 전의 `fail`, `<on>` 핸들러,
   `<reward>` 지급. 그래서 이후의 `quest.*`에 대한 `when`이나 `after: completed(…)` / `active(…)`는 실제
   진행을 봅니다.
7. **계기 판정 목표** — 이어서 스텝의 계기가 모든 **활성** 퀘스트의 `<objective on="<occasion>">`
   목표를 판정하고(dsl 0.21.0 §7a.2) 라이프사이클이 다시 정착하므로, 퀘스트가 정확히 그 스텝에서 완료(또는
   실패)할 수 있습니다. `on` 목표는 그 밖의 시점에는 판정되지 않습니다. 목표만 판정하는 계기도 shape-only
   프로젝트에서 합법적인 스텝이며, 제시할 비트가 없으면 `(no candidates)`를 출력하고 지나간 뒤 판정합니다.

라이프사이클은 스텝 1 전에 한 번(세이브의 퀘스트 상태가 이미 반영된 채로), 모든 `engine:` 스텝 뒤, 모든
`newRun` 뒤(초기화, 시드, 그다음 정착)에도 정착합니다. `event:` 스텝은 이벤트를 모든 퀘스트 문서에 한 번씩
발생시킨 뒤 정착합니다.

### 종료 코드

| 코드 | 의미 |
|---|---|
| `0` | 완료 — 모든 스텝이 재생되었거나 씬의 `::end`가 플레이스루를 끝냈고, 모든 기대값이 성립함. |
| `1` | 오류 — 프로젝트 컴파일 실패, 어휘 충돌, 자격이 없는 `pick`, 메뉴가 제시하지 않는 `choose:` 결정(자격 없는 선택지나 이미 고른 `once` hub 선택지), 또는 기대값 불일치. |
| `2` | 사용법 또는 I/O — 잘못된 스크립트([사용법 오류](#사용법-오류) 참고), 알 수 없는 계기나 월드 이벤트, 누락되었거나 도메인 밖인 `target`, 잘못된 시드나 `engine:` 쓰기, 그라운드가 아닌 `--explain` 원자, 읽을 수 없는 프로젝트, 잘못된 산출물. |
| `3` | 미완료 — 스크립트에 없는 choice나 hub, 바닥난 branch `choose:` 목록, unknown으로 평가되는 `when`이나 퀘스트 목표, 또는 해석되지 않은 `now()` / `validAt()` / 플러그인 `bridgeResult`. |

## 트랜스크립트

사람이 읽는 트랜스크립트는 모든 스텝을 이름 붙이고, 후보와 그 판정을 나열하고, 제시된 비트가 재생되기
전에 승자를 표시합니다. [플레이 스크립트](#플레이-스크립트)의 둘러보기 스크립트는 이렇게 출력합니다:

```
── start ──────────────
  quest climb -> active
  quest veteran -> active
  quest notices -> active
── step 1 · hubVisit ──────────────
  ✓ hub.idle [scene, priority 0]
  ✗ hub.victory [scene, priority 10] — when: false
  → hub.idle
@maud: Quiet night.
── step 2 · talk → npc.maud ──────────────
  ✓ maud.talk [scene, priority 0]
  → maud.talk
@maud: Up again?
── step 3 · board (select: all, pick: none) ──────────────
  ✓ memo [entry, priority 0]
  ✓ old [entry, priority 0]
  ✗ notice [entry, priority 0] — once: user — already read
  → (pick: none — the list closes; nothing presented)
  notices.looked done
  quest notices -> complete
── step 4 · event storm ──────────────
@narrator: Thunder rolls over the stair.
── step 5 (the warden falls on floor six) · engine ──────────────
  set run.floor = 6
  set user.runs = 3
  assert slew(warden)
  climb.high done
  quest climb -> complete
  veteran.three done
  quest veteran -> complete
── step 6 · new run ──────────────
  run.* state, run-tier facts and once: run reset
  quest climb -> unset (tier: run)
  set run.floor = 1
  quest climb -> active
── step 7 [1/2] · hubVisit ──────────────
  ✓ hub.idle [scene, priority 0]
  ✗ hub.victory [scene, priority 10] — when: false
  → hub.idle
@maud: Quiet night.
── step 7 [2/2] · hubVisit ──────────────
  ✓ hub.idle [scene, priority 0]
  ✗ hub.victory [scene, priority 10] — when: false
  → hub.idle
@maud: Quiet night.
── end: complete (8 steps) ──────────────
── expect: every expectation held ──────────────
```

- 모든 헤더는 텍스트 뒤에 고정된 `──────────────` 선이 붙습니다. `── start`는 스텝 1 전에 일어난 퀘스트
  전이를 담습니다. 각 스텝은 `── step <n>`으로 시작하고, 그 뒤에 `(label)`, 반복이면 `[k/n]`, 그리고 하는
  일이 옵니다: `· <occasion>` — 대상이 있는 스텝에는 `→ <target>`, `select: all` 계기에는
  `(select: all, pick: <id>)` — 또는 `· engine`, `· new run`, `· event <name>`.
- 후보는 자격 있는 것(`✓`)을 선택 순서대로 먼저, 그다음 나머지를 선택 순서대로 나열하며, 각각 종류와
  priority를 표시합니다. 자격 없는 후보(`✗`)에는 이유가 붙습니다:
  `once: run — already presented this run`, `once: user — already presented`,
  `once: run — already read this run`, `once: user — already read`,
  `after: prerequisite not satisfied`, 또는 `when: false`. `when`이 unknown으로 평가된 후보는
  `when: unknown (<detail>)`과 함께 `?`로 표시됩니다. 어떤 비트도 응답하지 않는 계기의 스텝은
  `(no candidates)`를 나열합니다.
- `→ <id>`가 승자를 가리킵니다. 승자가 없으면 `→ (no eligible beat — the occasion passes)` 또는
  `→ (pick: none — the list closes; nothing presented)`로 표시됩니다.
- 그 뒤에 제시된 비트 자신의 트랜스크립트가 소스처럼 읽히게 이어집니다: 콘텐츠 줄은 `@speaker: text`로,
  전달 방식을 유지합니다(`@wren{mono}: …`, `@maud{as="Barkeep"}: …`). `when=`이 거짓인 줄은
  `skip @maud "You again." — when: false`로 표시됩니다. 작가가 쓴 연출 디렉티브는 그대로 나오고,
  컴파일러가 주입한 연출(프리로드, 포즈 리셋, `::bg` 자동 숨김)은 빠집니다. 결정은
  `▷ choice <id>: … ← chosen: <id>`(또는 `▷ hub <id>: …`)이며, 메뉴에서 고른 선택지는 `[table]`,
  가드가 거짓인 선택지는 `piano✗`, 이미 고른 `once` 선택지는 `table(spent)`로 표시됩니다. 상태 쓰기는
  `set <path> = <value>`, 씬의 `::accept`는 `quest <id> accepted`(JSON: `presented.commands`의
  `{"kind": "accept", "quest": "<id>"}` 레코드), 엔트리는 `entry <id> (first read)` — 또는
  `entry <id> (re-read: effects skipped)`와 함께 건너뛴 각 효과에 `(skipped: re-read)` 표시. 퀘스트
  전이가 마지막에 옵니다 — 제시가 일으킨 것, 그다음 스텝의 계기가 판정한 것: `<quest>.<objective> done`,
  `quest <id> -> <state>`, 보상 지급. JSON에서는 둘 다 스텝의 `quests`에 들어갑니다.
- `engine:` 스텝은 쓰기를 나열합니다: `set <path> = <value>`, `assert <atom>`, `retract <atom>` — 팩트가
  아니었으면 `retract <atom> (did not hold)`.
- `newRun` 스텝은 `run.* state, run-tier facts and once: run reset`을 출력하고, 이어서 run 등급 퀘스트마다
  `quest <id> -> unset (tier: run)`, 그다음 시드의 쓰기를 출력합니다.
- `event:` 스텝은 실행된 핸들러를 출력합니다. 라이프사이클 전이는 모든 종류의 스텝 뒤에 따라옵니다.
- `--json`에서 `presented.commands`의 줄 레코드는 해당하는 경우 `role`, `lineId`, `voiceKey`, `as`,
  `emotion`을 담고, choice와 hub 레코드는 제시되지 않은 선택지를 `ineligible`에, 이미 고른 `once`
  선택지를 `spent`에 나열합니다.
- 워크는 `── end: complete (<n> steps)` — 모든 반복을 셈 — 로, 중간에 멈추면 `── halted: <message>`로
  끝납니다. 그 뒤에 `--explain` 트리, 그다음 `── expect:` 블록이 옵니다.

플러그인이 없는 모양만 검사하는 프로젝트에서, 부업을 제안하는 허브 씬과 `calm` 목표가 `runEnd`에서
판정되는 퀘스트:

```lute unverified="one file of a multi-file project: the scene answering hubVisit and a world schema declaring run.pressure sit beside it"
<quest id="holdLine" title="Hold the line" start="true">
  <objective id="sawShed" title="See the shed" done="visited('haven.shed')"/>
  <objective id="calm" title="Keep it calm" on="runEnd" done="run.pressure < 2"/>
</quest>

<quest id="sideJob" title="Side job">
  <objective id="mind" title="Mind the shed" done="run.pressure < 5"/>
</quest>
```

`steps: [{occasion: hubVisit}, {occasion: runEnd}]`와 `choose: { offer: take }` — 본문이
`::accept{quest="sideJob"}`인 선택지 — 로, 플레이스루는 제시 중에 부업을 수락하고, 바로 뒤에 활성화하며,
`runEnd`가 발생해야만 `holdLine`을 완료합니다:

```
── start ──────────────
  quest holdLine -> active
── step 1 · hubVisit ──────────────
  ✓ haven.shed [scene, priority 0]
  → haven.shed
@vesna: Somebody has to mind the shed.
▷ choice offer: [take] leave        ← chosen: take
  quest sideJob accepted
@vesna: Good. It's yours.
  quest sideJob -> active
  holdLine.sawShed done
  sideJob.mind done
  quest sideJob -> complete
── step 2 · runEnd ──────────────
  (no candidates)
  → (no eligible beat — the occasion passes)
  holdLine.calm done
  quest holdLine -> complete
── end: complete (2 steps) ──────────────
```

`--json`은 같은 워크를 객체 하나로 출력합니다:

```ts
type PlayTranscript = {
  exit: "complete" | "incomplete" | "error";
  start: { quests: QuestGroup[] };         // transitions made before step 1
  steps: Step[];                           // one record per repetition
  endReason?: string;
  error?: { message: string };
  expect?: { misses: ExpectMiss[] };       // when the script carries an `expect:`
  explain?: Explanation[];                 // one per `--explain` atom
};

type Step = (OccasionStep | EngineStep | NewRunStep | EventStep) & {
  step: number;                            // the script step (shared by its repetitions)
  label?: string;
  iteration?: number;                      // 1-based repetition, when `repeat` > 1
  repeat?: number;
  quests: QuestGroup[];                    // transitions this step caused
};

type OccasionStep = {
  occasion: string;
  target?: string;
  select: "first" | "all";
  pick?: string;                           // a beat id, or "none"
  candidates: {
    id: string;
    kind: "scene" | "entry";
    document: string;
    priority: number;
    eligible: boolean | null;              // null: the `when` evaluated to unknown
    reason?: string;                       // e.g. "when: false", "when: unknown (<detail>)"
  }[];
  winner: string | null;
  presented?: {
    id: string;
    kind: "scene" | "entry";
    document: string;
    commands: RunnerRecord[];              // the records `lute run --json` emits
    stateDelta: Record<string, unknown>;   // path -> value
  };
};

type EngineStep = { engine: WriteRecord[] };
type NewRunStep = { newRun: true; seed: WriteRecord[] };
type EventStep = { event: string };

type WriteRecord =
  | { kind: "set"; path: string; value: unknown }
  | { kind: "assert"; fact: string }
  | { kind: "retract"; pattern: string; held: boolean };

type QuestGroup = {
  document: string;                        // the quest document
  commands: RunnerRecord[];                // its `objective` / `quest` / `grant` / handler records
};

type ExpectMiss = {
  step: number | null;                     // null: the top-level `expect:`
  label: string | null;
  occasion: string | null;                 // "talk npc.tomas"
  repetition: number | null;
  key: string;                             // "winner", "quests lampOut", "state run.day", …
  expected: string;
  actual: string;
};

type Explanation =
  | { atom: string; holds: true; proof: Proof }
  | { atom: string; holds: false; derived: boolean; attempts: Attempt[] };
type Proof =
  | { fact: string; support: "seed fact" | "asserted" }
  | { fact: string; support: "derived"; rule: string; premises: Premise[] };
type Attempt = { rule: string; premises: Premise[] };
type Premise =
  | { status: "holds"; proof: Proof }
  | { status: "missing"; atom: string; attempts: Attempt[] }
  | { status: "absent"; negated: string }
  | { status: "present"; proof: Proof }
  | { status: "test"; test: string; holds: boolean | null }
  | { status: "unreached"; premise: string };
```

## 예제

`lute init --template beats`는 스캐폴드된 그대로 `check-project`, `test`, `play`를 통과하는 작은 프로젝트를
만듭니다: 마을 광장의 첫날, 꺼져 버린 등불, 그리고 그 이유를 아는 두 사람.

```console
$ lute init --template beats town
```

```
town/
├── lute.project.yaml
├── world.schema.yaml
├── vocabulary.schema.yaml
├── plugins/game.occasions/
│   ├── plugin.yaml
│   └── occasions/game.yaml
├── scenes/
│   ├── hub/welcome.lute
│   ├── hub/morning.lute
│   ├── hub/day-end.lute
│   ├── talk/mara-first.lute
│   └── talk/mara-idle.lute
├── quests/lamp.lute
├── lore/tomas.lute
├── plays/first-day.play.yaml
├── tests/
│   ├── mara-first.test.yaml
│   └── lamp-quest.test.yaml
└── README.md
```

매니페스트는 엔진의 계기만 export하는 플러그인 하나를 활성화하고, 그 `defaults:`가 모든 문서에 언어
버전과 두 스키마 import를 줍니다. 계기, `plugins/game.occasions/occasions/game.yaml`:

```yaml
occasions:
  hubVisit: { select: first, description: The player arrives at the hub }
  talk:     { select: first, target: { prefix: npc, entity: npc }, description: The player talks to someone (npc.<name>) }
  dayEnd:   { select: first, description: "The engine closed the day; run.day is already advanced" }
```

공유 상태, `world.schema.yaml`. 하루는 엔진의 것입니다: 콘텐츠는 `run.day`를 읽지만, 쓰는 것은
엔진뿐이고 — 그래서 `engine:` 스텝뿐입니다:

```yaml
state:
  run.day:        { type: number, default: 1, owner: engine }
  user.bond.mara: { type: number, default: 0 }

entities:
  npc:  { members: [mara, tomas] }
  item: { members: [lamp] }

relations:
  knows: { args: [item], tier: run }

defs:
  firstDay: "run.day == 1"
  trusted:  "user.bond.mara >= 1"
```

비트:

| 비트 | 문서 | 응답 | 조건 | `once` |
|---|---|---|---|---|
| `hub.welcome` | `scenes/hub/welcome.lute` | `hubVisit` | priority 10 | `user` |
| `hub.morning` | `scenes/hub/morning.lute` | `hubVisit` | `when: '!@firstDay'` — `Day {{run.day}}.`를 출력 | `false` |
| `hub.dayEnd` | `scenes/hub/day-end.lute` | `dayEnd` | — | `false` |
| `mara.first` | `scenes/talk/mara-first.lute` | `talk` → `npc.mara` | priority 10 | `user` |
| `mara.idle` | `scenes/talk/mara-idle.lute` | `talk` → `npc.mara` | — | `false` |
| `tomasOil`(엔트리) | `lore/tomas.lute` | `talk` → `npc.tomas` | priority 10, `when="quest.lampOut.state == 'active'"`, `knows(lamp)`를 단언 | — |
| `tomasBusy`(엔트리) | `lore/tomas.lute` | `talk` → `npc.tomas` | — | — |

`mara.first`가 하루를 가르는 질문을 던집니다:

```lute
<branch id="maraAsk" prompt="What do you say?">
  <choice id="lamp" label="Offer to find out why">
    @mara{emotion="delighted"}: Would you? Tomas keeps the oil. Ask him.
    ::set{ user.bond.mara += 1 }
    ::accept{quest="lampOut"}
  </choice>
  <choice id="leave" label="Say nothing">
    @mara: Suit yourself.
  </choice>
</branch>
```

그 선택지가 수락하는 퀘스트 `quests/lamp.lute`는 엔진이 하루를 닫아야만 끝납니다:

```lute
<quest id="lampOut" title="The lamp by the door">
  <objective id="ask" title="Ask Tomas about the oil" done="holds(knows(lamp))"/>
  <objective id="wait" title="Wait for the day to end" on="dayEnd" done="run.day >= 2"/>
  <on event="questComplete">
    @narrator: By morning the lamp by the door is burning again.
  </on>
</quest>
```

### 첫날

스캐폴드의 플레이 스크립트 `plays/first-day.play.yaml`은 하루를 재생하고, 일어나야 할 일을 단언합니다:

```yaml
choose:
  maraAsk: lamp
steps:
  - occasion: hubVisit
    expect: { winner: hub.welcome }
  - occasion: talk
    target: npc.mara
    expect: { winner: mara.first }
  - occasion: talk
    target: npc.tomas
    expect: { winner: tomasOil, offered: [tomasOil, tomasBusy] }
  - label: the engine closes the day
    engine:
      state: { run.day: { add: 1 } }
  - occasion: dayEnd
  - occasion: hubVisit
    expect: { winner: hub.morning, notOffered: [hub.welcome] }
expect:
  exit: complete
  quests: { lampOut: complete }
  state: { run.day: 2, user.bond.mara: 1 }
  facts: [knows(lamp)]
```

프로젝트 디렉터리에서:

```console
$ lute play . --script plays/first-day.play.yaml
```

```
── step 1 · hubVisit ──────────────
  ✓ hub.welcome [scene, priority 10]
  ✗ hub.morning [scene, priority 0] — when: false
  → hub.welcome
::background{location="hub" time="day" wait=true}
@narrator: The lamps along the square are lit — all but the one by the door.
── step 2 · talk → npc.mara ──────────────
  ✓ mara.first [scene, priority 10]
  ✓ mara.idle [scene, priority 0]
  → mara.first
@mara{emotion="content"}: You're new. The lamp by the door has been dark for a week.
▷ choice maraAsk "What do you say?": [lamp] leave        ← chosen: lamp
@mara{emotion="delighted"}: Would you? Tomas keeps the oil. Ask him.
  set user.bond.mara = 1
  quest lampOut accepted
  quest lampOut -> active
── step 3 · talk → npc.tomas ──────────────
  ✓ tomasOil [entry, priority 10]
  ✓ tomasBusy [entry, priority 0]
  → tomasOil
  entry tomasOil (first read)
@tomas: Oil? Top shelf. Tell Mara it's the wick, not the oil.
  assert knows(lamp)
  lampOut.ask done
── step 4 (the engine closes the day) · engine ──────────────
  set run.day = 2
── step 5 · dayEnd ──────────────
  ✓ hub.dayEnd [scene, priority 0]
  → hub.dayEnd
@narrator: One by one, the lamps go out.
  lampOut.wait done
  quest lampOut -> complete
@narrator: By morning the lamp by the door is burning again.
── step 6 · hubVisit ──────────────
  ✓ hub.morning [scene, priority 0]
  ✗ hub.welcome [scene, priority 10] — once: user — already presented
  → hub.morning
@narrator: Day 2. The square is already awake.
── end: complete (6 steps) ──────────────
── expect: every expectation held ──────────────
```

스텝별로 읽으면:

- **시작** — `lampOut`에는 `start`가 없으므로 첫 스텝 전에 아무것도 활성화되지 않고, `── start` 블록도
  없습니다.
- **스텝 1** — 1일째이므로 `hub.morning`의 `@firstDay` 가드가 그것을 막고, 환영 인사가 재생됩니다.
- **스텝 2** — `talk`가 `npc` 종류의 멤버인 `npc.mara`를 위해 발생합니다. 마라의 씬 둘이 모두 후보이고
  첫 만남이 대체 씬보다 앞섭니다. `lamp`를 고르면 유대가 오르고 퀘스트가 수락되며, 퀘스트는 제시 직후
  활성화됩니다.
- **스텝 3** — 퀘스트가 활성이므로 토마스의 기름 엔트리가 자격을 얻고 그의 짧은 대사보다 앞섭니다. 첫
  읽기가 `knows(lamp)`를 단언하여 첫 번째 목표를 완료합니다.
- **스텝 4** — 엔진이 하루를 닫습니다. `engine:` 스텝은 콘텐츠가 쓸 수 없는(`owner: engine`) `run.day`를
  쓰며, 아무것도 제시되지 않고 계기도 발생하지 않습니다.
- **스텝 5** — `dayEnd`가 밤 씬을 제시한 뒤 `lampOut.wait`(`on="dayEnd"`)을 판정하고, 퀘스트가
  완료됩니다. 그 `questComplete` 핸들러가 재생됩니다.
- **스텝 6** — `hub.welcome`은 영구히 소진되었고(`once: user`) `run.day`가 2이므로 아침 씬이 재생됩니다.

### 한 스텝만 다르게 결정하기

스텝 자신의 `choose:`는 한 번의 제시만 바꾸고 나머지는 건드리지 않습니다. 여기서 플레이어는 마라에게
아무 말도 하지 않지만, 기대값은 여전히 첫날을 기술하므로 불일치가 납니다:

```yaml
choose:
  maraAsk: lamp
steps:
  - occasion: hubVisit
  - occasion: talk
    target: npc.mara
    choose: { maraAsk: leave }
  - occasion: talk
    target: npc.tomas
    label: ask about the oil
    expect: { winner: tomasOil }
expect:
  quests: { lampOut: active }
  state: { user.bond.mara: 1 }
```

```
── step 1 · hubVisit ──────────────
  ✓ hub.welcome [scene, priority 10]
  ✗ hub.morning [scene, priority 0] — when: false
  → hub.welcome
::background{location="hub" time="day" wait=true}
@narrator: The lamps along the square are lit — all but the one by the door.
── step 2 · talk → npc.mara ──────────────
  ✓ mara.first [scene, priority 10]
  ✓ mara.idle [scene, priority 0]
  → mara.first
@mara{emotion="content"}: You're new. The lamp by the door has been dark for a week.
▷ choice maraAsk "What do you say?": lamp [leave]        ← chosen: leave
@mara: Suit yourself.
── step 3 (ask about the oil) · talk → npc.tomas ──────────────
  ✓ tomasBusy [entry, priority 0]
  ✗ tomasOil [entry, priority 10] — when: false
  → tomasBusy
  entry tomasBusy (first read)
@tomas: Busy.
── end: complete (3 steps) ──────────────
── expect: 3 missed ──────────────
  ✗ step 3 (ask about the oil) at talk npc.tomas: expect winner: expected tomasOil, actual tomasBusy
  ✗ end of play: expect quests lampOut: expected active, actual unset
  ✗ end of play: expect state user.bond.mara: expected 1, actual 0
```

워크 자체는 완료되었고, 불일치 때문에 `lute play`가 종료 코드 1로 끝납니다.

### 다시 돌아오기

이후 세션은 첫날을 다시 재생하지 않습니다: 스크립트는 첫날을 마친 플레이어가 가졌을 세이브에서 시작하고,
엔진은 3일째에 두 번째 런을 시작합니다:

```yaml
state: { user.bond.mara: 1 }
presented: { user: [hub.welcome, mara.first] }
quests: { lampOut: complete }
steps:
  - label: the engine starts run two on day 3
    newRun: { state: { run.day: 3 } }
  - occasion: hubVisit
    expect: { winner: hub.morning, notOffered: [hub.welcome] }
  - occasion: talk
    target: npc.mara
    expect: { winner: mara.idle }
  - occasion: talk
    target: npc.tomas
    expect: { winner: tomasBusy, notOffered: [tomasOil] }
  - label: a quiet day passes
    engine:
      state: { run.day: { add: 1 } }
    repeat: 2
  - occasion: dayEnd
    expect: { winner: hub.dayEnd }
expect:
  exit: complete
  state: { run.day: 5 }
  transcriptContains: ["Any luck with the lamp?", "Day 3."]
  transcriptLacks: ["You're new."]
```

```
── step 1 (the engine starts run two on day 3) · new run ──────────────
  run.* state, run-tier facts and once: run reset
  set run.day = 3
── step 2 · hubVisit ──────────────
  ✓ hub.morning [scene, priority 0]
  ✗ hub.welcome [scene, priority 10] — once: user — already presented
  → hub.morning
@narrator: Day 3. The square is already awake.
── step 3 · talk → npc.mara ──────────────
  ✓ mara.idle [scene, priority 0]
  ✗ mara.first [scene, priority 10] — once: user — already presented
  → mara.idle
@mara{emotion="shy"}: Any luck with the lamp?
  skip @mara "Mm." — when: false
── step 4 · talk → npc.tomas ──────────────
  ✓ tomasBusy [entry, priority 0]
  ✗ tomasOil [entry, priority 10] — when: false
  → tomasBusy
  entry tomasBusy (first read)
@tomas: Busy.
── step 5 (a quiet day passes) [1/2] · engine ──────────────
  set run.day = 4
── step 5 (a quiet day passes) [2/2] · engine ──────────────
  set run.day = 5
── step 6 · dayEnd ──────────────
  ✓ hub.dayEnd [scene, priority 0]
  → hub.dayEnd
@narrator: One by one, the lamps go out.
── end: complete (7 steps) ──────────────
── expect: every expectation held ──────────────
```

- **세이브** — `presented.user`가 `once: user` 씬 둘을 소진시키고, `quests:`가 `lampOut`을 완료 상태로
  이어받으며(그래서 기름 엔트리의 `when`이 거짓), `user.bond.mara` 시드 덕분에 마라의 `@trusted` 대사가
  재생됩니다.
- **스텝 1** — 긴 형태의 `newRun`이 새 런을 시드합니다: `run.day`가 기본값으로 초기화된 뒤 시드가 3으로
  설정합니다.
- **스텝 5** — `engine:` 스텝 하나를 반복: 각 반복은 자신의 레코드이고, 런은 `run.day`가 5인 채로
  끝납니다.

### 테스트 스위트에서

`expect:`를 가진 모든 플레이는 `lute test`에서 시나리오 테스트와 나란히 실행됩니다. 위의 두 플레이를
`plays/say-nothing.play.yaml`과 `plays/returning.play.yaml`로 저장하면:

```console
$ lute test . --project .
```

```
PASS  ./tests/lamp-quest.test.yaml  (./tests/../quests/lamp.lute)
PASS  ./tests/mara-first.test.yaml  (./tests/../scenes/talk/mara-first.lute)
PASS  ./plays/first-day.play.yaml  (play of .)
PASS  ./plays/returning.play.yaml  (play of .)
FAIL  ./plays/say-nothing.play.yaml  (play of .)
      step 3 (ask about the oil) at talk npc.tomas: expect winner: expected tomasOil, actual tomasBusy
      end of play: expect quests lampOut: expected active, actual unset
      end of play: expect state user.bond.mara: expected 1, actual 0

4 passed, 1 failed
```

스캐폴드 자신의 플레이만 있을 때, `lute test . --project . --coverage`는 그것도 테스트로 셉니다:

```
PASS  ./tests/lamp-quest.test.yaml  (./tests/../quests/lamp.lute)
PASS  ./tests/mara-first.test.yaml  (./tests/../scenes/talk/mara-first.lute)
PASS  ./plays/first-day.play.yaml  (play of .)

3 passed, 0 failed

coverage over 2 traced path(s) and 1 play(s):
  branch/hub maraAsk (./tests/../scenes/talk/mara-first.lute:maraAsk): 1/2 chosen [lamp]; never chosen [leave]
  1 untested document(s) under . — no *.test.yaml names them and no play presents them:
    ./scenes/talk/mara-idle.lute
```

플레이가 환영 인사, 아침, 밤 씬, 마라와의 첫 만남, 토마스의 엔트리를 제시했으므로 남은 것은
`mara-idle.lute`뿐입니다 — `plays/returning.play.yaml`이 커버하는 씬입니다.
