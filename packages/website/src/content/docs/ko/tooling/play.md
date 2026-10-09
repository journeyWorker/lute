---
title: 스토리 플레이
description: "계기(occasion)와 비트(beat, dsl 0.21.0) — 어떤 스토리 조각이 어떤 엔진 순간에 응답하는지 프로젝트가 선언하는 방법 — 그리고 스크립트로 적은 플레이스루를 프로젝트 전체에 걸쳐 걷는 참조 플레이어 `lute play`: 발생시킨 계기, 엔진 자신의 쓰기, 출발점이 되는 세이브, `lute test`가 실행하는 단언(dsl 0.22.0), 시퀀스 전체를 재생하는 계기, 곁들이는 대사, 기한, 대상 지정 목표, 비트 번들(dsl 0.23.0), 스크립트가 앞으로 돌리는 시계, 플러그인 브리지 호출에 대한 응답, 트랜스크립트에 드러나는 퀘스트 구조(dsl 0.24.0), 그리고 이름 붙은 슬롯이나 요일까지 돌리는 시계, 엔진이 수락하는 퀘스트, 엔티티 종류 전체에 응답하는 비트(dsl 0.26.0, 초안)."
---

내러티브 게임은 저마다의 순간에 다음 스토리 조각을 고릅니다: 마을 방문, 방 입장, NPC와의 대화, 새로운
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

0.34.0부터 프로젝트 명령은 가장 가까운 manifest 루트마다 하나의
프로젝트 모델을 공유합니다. `play`와 `test`의 분석 결과에는
`proven`, `witnessed`, `bounded`, `heuristic`, `unknown` 중 하나인
`evidence`가 포함될 수 있습니다. `bounded`는 선언된 범위 안에서만
성립한다는 뜻이며, “범위 안에서 경로를 찾지 못함”을 “경로가 없음”으로
표현하지 않습니다.

0.23.0부터 계기는 자격 있는 비트를 **모두** 차례로 제시할 수 있고(`select: sequence`), 비트는 승자 뒤에
곁들여 나올 수 있으며(`also: true`), 목표는 기한(`by=`)을 갖거나 한 대상만 기다릴 수 있고(`target=`),
로어 문서 하나가 씬 같은 **번들 비트**를 여럿 담을 수 있습니다. 플레이스루 하나를 걷는 대신 모든
상태에서 어떤 비트가 재생될지 보려면 [스토리 개요](/tooling/overviews/)를 쓰세요 — `lute calendar`는 이
페이지의 자격 판정을 상태 값 격자 전체에 대해 평가합니다.

0.24.0부터 스크립트는 선언된 [시계](/language/clock/)를 앞으로 돌리고(`advance:`), 여러 경로가 일과 하나를
나눠 쓰며(`include:`), 실제 엔진이라면 서비스에 넘겼을 플러그인 브리지 호출에 직접 답합니다(`bridges:`).
트랜스크립트는 각 퀘스트가 왜 실패했는지 밝히고, `judge: before` 계기가 비트보다 먼저 퀘스트를 정착시키는
모습을 보여 주며, `newRun`이 스냅숏한 값과 이전 런에서 넘겨받아 적용하는 것을 출력합니다.

dsl 0.26.0(초안)부터 스크립트는 시계를 다음의 이름 붙은 슬롯이나 요일까지 돌리고(`advance: { to: night }`)
시계가 어디에 있는지 단언하므로(`expect: { clock: … }`), 나눠 쓰는 steps 파일이 자신이 기대하는 시각을 밝힐 수
있습니다. `engine:` 스텝은 플레이 도중에 수락 방식 퀘스트를 수락하고, 비트 하나가 엔티티 종류의 모든 멤버에
응답할 수 있으며(`target="kind:trainer"`), 실행되지 않은 가드된 디렉티브는 건너뛴 것으로 표시되고, 엔트리는
`<문서 id>.<엔트리 id>`로 가리킬 수 있습니다. 컴포넌트가 하는 호출이라도 브리지 응답은 결과 슬롯의 타입을
따르고, 트랜스크립트 바늘(needle)은 복사해 온 줄 속성을 그대로 가져도 됩니다.

규범 텍스트는 [0.21.0 제안서](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.21.0.md)이고,
[0.22.0 제안서](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.22.0.md)(플레이·테스트
하네스), [0.23.0 제안서](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.23.0.md)(계기
조합, 기한, 번들), [0.24.0 제안서](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.24.0.md)(시계,
퀘스트 구조, 브리지 응답), [0.26.0 제안서](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.26.0.md)(시계
목표, 엔진 수락, 종류 대상, 테스트와 플레이의 일치), [0.27.0 제안서](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.27.0.md)(하나의 런타임, 엔진 게이트와 종료 상태, 끝이 있는 시계, 시즌, 비트 템플릿, 지금은 `chapters:`인 매니페스트의 장면 사슬), [0.28.0 제안서](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.28.0.md)(`chapters:`, 한 번 성립하면 유지되는 `spentBy`, 차례마다 다시 판정하는 `select: sequence`, `expect.end`와 `expect.options`, `outsideRun:` 계기, `clock.ended`), [0.29.0 제안서](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.29.0.md)(런을 넘어 유지되는 결말, `terminal: { when, persists: true }`), [0.30.0 제안서](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.30.0.md)(식별자가 아닌 이름을 따옴표 인덱스로 쓰기, `state: { 'run.visits["lab-b2"]': 1 }`)가 이를 확장합니다. 엔진 측 계약(IR 필드와 엔진이 구현하는 선택 알고리즘)은
[`docs/runtime/beats-and-occasions.md`](https://github.com/journeyWorker/lute/blob/main/docs/runtime/beats-and-occasions.md)입니다.
0.21.0 이전의 `lute play`는 틱 클록 스케줄 파일을 걸었습니다. 그 레이어와 클록/레인/배치 모델, 그리고
관련 플래그는 모두 제거되었습니다. 이제 시간은 틀이 아니라 비트 조건의 입력 중 하나입니다.

## 계기

계기는 **엔진 어휘**입니다: 엔진이 발생시키는 이름 붙은 순간이며, 선택적으로 어떤 대상을 *위해*
발생합니다(`talk` → `npc.achilles`). 플러그인은 `occasions` export로 자신의 계기를 선언합니다 —
`plugin.yaml`에 `exports: { occasions: occasions/ }`로 나열하고, 맵은 `occasions/*.yaml`에 둡니다:

```yaml
occasions:
  townVisit: { select: first }
  talk:      { select: first, target: { prefix: npc, entity: person } }
  roomEnter: { select: first, target: true }
  inbox:     { select: all, description: Letters waiting at the fountain }
```

- `select: first`(기본값) — 엔진은 이긴 비트 하나를 제시합니다.
- `select: all` — 엔진은 자격 있는 비트를 모두 선택 순서대로 제시하고 플레이어가 하나를 고릅니다(메시지
  수신함, 지역 지도, "순서 무관" 퀘스트 제공자).
- `select: sequence`(dsl 0.23.0) — 엔진은 자격 있는 비트를 **모두** 선택 순서대로 하나씩 차례로
  제시합니다: 일과 뒤에 그날의 사건, 런의 시작 뒤에 지난 런 회상. [계기 조합하기](#계기-조합하기)를 보세요.
- `target: { prefix, entity }`(dsl 0.22.0) — 계기가 무언가를 **위해** 발생하며, 그 대상이 하나의
  어휘가 됩니다: `<prefix>.<member>`이고 `<member>`는 `entities:` 종류 `entity`의 멤버입니다(`open:`
  종류라면 아무 id). 스키마가 `person: { members: [achilles, patroclus] }`를 선언하면 위의 `talk`는
  `npc.achilles`나 `npc.patroclus`를 위해 발생합니다.
- `target: true` — 대상을 위해 발생하지만 모양만 검사합니다: 점으로 구분된 아무 id. 기본값 `false`는
  아무것도 위하지 않고 발생합니다.
- `description` — 도구용 선택적 설명.
- `judge: before`(dsl 0.24.0 §2) — 계기가 자신의 `on=` 목표를 판정하고 퀘스트를 정착시키는 일을 비트를
  정하기 **전에** 합니다. 그래서 그 계기의 에필로그는 퀘스트가 어떻게 끝났는지 읽을 수 있습니다. 옮겨지는
  것은 판정뿐입니다: 발생이 응답하는 핸들러 본문 — 같은 이름의 `<on event>` 핸들러, 그리고 정착한 퀘스트의
  `questComplete` / `questFailed` 핸들러 — 은 여전히 비트 뒤에 실행되므로, 그 내레이션은 씬 뒤에 나옵니다.
  기본값 `after`는 제시가 끝난 뒤에 판정합니다. [퀘스트 구조](#퀘스트-구조)를 보세요.

해석된 플러그인 중 계기를 선언한 것이 없으면 계기 이름은 **모양만(shape-only)** 검사됩니다: 어떤
이름이든 받아들이므로, 엔진 플러그인이 생기기 전에도 비트를 쓸 수 있습니다. 엔진 플러그인이 전혀 없는
이야기도 이렇게 플레이합니다: [씬을 이야기로 잇기](/getting-started/connect-scenes/)는 모든 씬이 모양만
검사되는 계기 하나에 응답하게 하고 `lute play`로 플레이합니다. 어느 플러그인이든 계기를
선언하는 순간, 선언되지 않은 계기를 가리키는 비트는 `E-OCCASION-UNKNOWN`이 되고, 대상 없이 선언된
계기에 `target`을 붙이면 `E-BEAT-ATTR`입니다. 대상 도메인이 있는 계기에서 그 밖의 비트 대상 —
`npc.achilles` 대신 쓴 `npc.achiles` — 은 did-you-mean과 함께 `E-BEAT-ATTR`이며, 프로젝트가 선언하지 않은 엔티티
종류를 가리키는 도메인도 마찬가지입니다. 이 export는 가드된 섹션으로 캐퍼빌리티 스냅샷에 접히므로,
플러그인이 계기를 선언하지 않는 프로젝트는 `capabilitySnapshot`이 그대로입니다. 대상 도메인은 스냅샷의
일부이며, `target: true`는 0.21과 정확히 같게 해시됩니다.

계기에서 판정되는 퀘스트 목표(`<objective on="runEnd">`)는 `on=`을 받고, 0.23.0부터는 선택적인
`target=`도 받습니다 — `<objective on="talk" target="npc.maud">`는 `talk`가 `npc.maud`를 위해 발생했을
때만 판정되며, 비트 대상처럼 검사됩니다(`E-BEAT-ATTR`: 점으로 구분된 id, `on` 옆에서만, 대상을 받는
계기에서, 그 도메인 안에서). [기한과 대상 지정 목표](#기한과-대상-지정-목표)를 보세요.

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
| `target` | 선택. 계기가 이 대상을 위해 발생했을 때만 씬이 후보가 됩니다(점으로 구분된 id, `<entry target>`과 같은 모양. 계기가 대상 도메인을 선언했다면 `<prefix>.<member>`). dsl 0.26.0부터 `kind:<kind>`는 한 종류의 모든 멤버에 응답합니다 — [종류 대상](#종류-대상) 참고. |
| `when` | 선택. `run` / `user` / `app` 상태, `quest.*`, `entry.<id>.read` / `entry.<id>.everRead`, 팩트 질의에 대한 CEL 조건. 씬 자신의 `scene.*` 상태는 아직 존재하지 않으므로 거부됩니다. |
| `priority` | 선택 정수, 기본값 `0`. 높을수록 이깁니다. |
| `once` | `run`(기본값 — 런마다 한 번), `user`(평생 한 번), 또는 `false`(반복 가능). [시계](/language/clock/)가 선언되어 있으면 `day`나 `slot`(dsl 0.24.0 §1): 시계의 날이나 슬롯이 바뀔 때까지 소진됩니다. |
| `also` | 선택 `true`(dsl 0.23.0): `select: first` 계기에서 이 비트는 이기지 않습니다 — 승자 **뒤에** 곁들이는 대사로 제시되며, 주 비트가 하나도 자격이 없을 때도 제시됩니다. [계기 조합하기](#계기-조합하기)를 보세요. |

`after:`는 의미가 그대로입니다 — [연결성](/connectivity/scene-graph/) 분석이 다루는 `visited` /
`completed` / `active`에 대한 구조적 전제조건 — 그리고 비트는 `after:`와 `when`이 모두 성립할 때만
자격이 있습니다. `on` 없이 `when`, `target`, `priority`, `once`, `also`를 쓰면 `E-BEAT-ATTR`입니다. 어떤
계기에도 응답하지 않는 씬은 계기가 제시하지 않습니다: 엔진이 씬 id로 직접 시작하고, `lute trace`와
`lute run`은 그 씬 하나를 따로 걸으며, `lute play`는 그 씬을 제시하지 않습니다. 씬들을 이어 `lute play`로
플레이할 수 있는 이야기로 만들려면 [씬을 이야기로 잇기](/getting-started/connect-scenes/)를 보세요.
bool이 아닌 `also`, 그리고 자격 있는 비트가 이미 모두 제시되거나 제시 목록에 오르는 `select: all` /
`select: sequence` 계기의 `also: true`도 `E-BEAT-ATTR`입니다.

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
않으며, `entry.<id>.read`를 읽을 수 있는 곳이면 어디서든 읽을 수 있습니다. 시계가 선언되어 있으면
`once="day"` / `once="slot"`은 시계의 날이나 슬롯이 바뀔 때까지 엔트리를 소진시킵니다(dsl 0.24.0 §1). 잘못된
`once`, 또는 `on` 없는 `once`는 `E-BEAT-ATTR`입니다.

### 번들 비트

씬은 파일 하나에 비트 하나입니다. 한 캐릭터에게 짧은 씬이 많을 때 — 방문할 때마다 하는 NPC의 말,
심문 질문 한 벌 — [로어 문서](/language/lore-entries/#entries-and-beats-in-one-file)가 그것들을 **번들
비트**(dsl 0.23.0)로 담을 수 있습니다: 씬 본문 전체 — 대사, branch, hub, `<match>`, 지시문 — 를 가진
`<beat>` 블록이며, 씬 비트가 프런트매터에 적는 것과 같은 속성(`on`, `target`, `when`, `priority`, `once`,
`also`, dsl 0.25.0부터 `share`와 `after`, 그리고 `select: all` 메뉴용 `title`)을 받습니다. 비트 자신의
`after="…"`는 씬의 것처럼 검사되며(`✗ keeper.greeting [beat, priority 0] — after: prerequisite not satisfied`),
[`share`](/language/beats/#one-event-several-places-share) 키를 가진 비트는 같은 키의 다른 비트가 `once` 기간 안에
제시되었으면 함께 소진됩니다(``✗ talks.solRoof [beat, priority 0] — once: day — `share: solWarm` already spent today by talks.solRadio``). [아래 예제](#lute-play)가 오르는 탑에서
`lore/oskar.lute`는 `id: oskar`를 선언하고 오스카에게 둘을 줍니다:

```lute
<beat id="hunt" on="talk" target="npc.oskar" priority="10" title="The hound">
  @oskar: The hound took my dog's collar. Bring it back before you pass floor four.
  ::accept{quest="houndHunt"}
</beat>

<beat id="rumor" on="talk" target="npc.oskar" also="true" once="false" when="run.floor >= 2">
  @oskar: They say the warden sleeps on floor six.
</beat>
```

번들 비트는 엔트리가 아니라 씬 비트처럼 동작합니다: id는 `<문서 id>.<비트 id>` — `oskar.hunt`,
`oskar.rumor` — 이므로 문서에 `id:`가 필요합니다. `once`의 기본값은 `run`이고 제시로 소진됩니다. 제시하면
그 id가 방문한 것으로 기록되므로 어떤 조건에서든 `visited('oskar.hunt')`로 읽을 수 있습니다(`after:`는
여전히 씬과 퀘스트만 가리킵니다). `check-project`는 다른 비트처럼 판정합니다(`E-BEAT-UNREACHABLE`,
`W-BEAT-SHADOWED`, `W-BEAT-PRIORITY-TIE`, `W-BEAT-ONCE-RUN-USER`). 언어 규칙은
[비트](/language/beats/#beat-bundles)에 있습니다. 트랜스크립트에서 번들 비트의 종류는 `beat`이며,
`lute trace --beat`와 `lute run --beat`는 번들 비트 하나를 따로 제시합니다([트레이싱](/tooling/tracing/#bundle-beats)
참고).

### 종류 대상

비트는 멤버 하나 대신 엔티티 종류 전체에 응답할 수 있습니다(dsl 0.26.0 §5): 씬 프런트매터의
`target: kind:<kind>`, 엔트리나 번들 비트의 `target="kind:<kind>"`. 그 종류는 계기의 대상 도메인 안의 닫힌
종류 — 도메인 자신의 종류이거나 그것의 `subsetOf:`로 선언된 종류 — 여야 하며, 비트는 계기가 그 멤버 누구를
위해 발생하든 후보가 됩니다. 발생한 멤버는 비트의 `when`, 가드, 텍스트에서 `occasion.target`으로 읽을 수
있고, 타입은 그 종류이며, 비트가 제시되는 동안에만 바인딩됩니다. 도메인 밖의 종류는 did-you-mean이 붙은
`E-BEAT-ATTR`이고, 종류를 대상으로 하지 않는 비트에서 `occasion.target`을 읽으면 `E-UNDECLARED`입니다. 도전
비트 하나가 길 위의 모든 트레이너에 응답합니다:

```yaml
entities:
  person:  { members: [wren, sol] }
  trainer: { subsetOf: person, members: [gus, r16Gus] }
```

```lute
<beat id="challenge" on="talk" target="kind:trainer" title="A trainer squares up" once="false">
  @narrator: {{occasion.target}} squares up. "You look like you've never lost."
</beat>

<beat id="gusRematch" on="talk" target="npc.gus" title="Gus wants a rematch" once="false">
  @gus: Back for another round?
</beat>
```

priority가 같으면 종류 비트는 다른 후보들 뒤에 옵니다. 그래서 `npc.gus`에서는 거스 자신의 비트가 — 둘 사이에
`W-BEAT-PRIORITY-TIE` 없이 — 이기고, 종류 비트는 나머지 트레이너 모두에 응답합니다. `lute play`는
`{{occasion.target}}`을 멤버가 cast id이면 그 cast `name:`으로, 아니면 id로 렌더링합니다. 컴파일된
placeholder는 `{"kind": "occasionTarget", "entityKind": "trainer"}`이므로 엔진은 자신의 표시 이름을
렌더링합니다. `r16Gus`의 cast 이름이 "Hiker Brom"이고 로어 문서 id가 `trainers`일 때:

```
── step 1 · talk → npc.r16Gus ──────────────
  ✓ trainers.challenge [beat, priority 0]
  → trainers.challenge
@narrator: Hiker Brom squares up. "You look like you've never lost."
── step 2 · talk → npc.gus ──────────────
  ✓ trainers.gusRematch [beat, priority 0]
  ✓ trainers.challenge [beat, priority 0]
  → trainers.gusRematch
@gus: Back for another round?
```

`lute beats`는 종류 비트를, 어떤 비트가 이름으로 가리키는 멤버마다의 사다리와, 어떤 비트도 따로 가리키지 않는
멤버를 위한 `kind:<kind>` 사다리에 나열합니다([스토리 개요](/tooling/overviews/#lute-beats) 참고). `lute trace`와
`lute test`는 멤버를 `state: { occasion.target: r16Gus }` 시드에서 읽습니다([`lute test`](/tooling/cli/#test)
참고). 언어 규칙은 [비트](/language/beats/)에 있습니다.

대상 없는 `select: sequence` 계기에서 `for="kind:<kind>"`(dsl 0.27.0 §3, 씬 프론트매터에서는 `for:`)는 비트 하나를
`when`이 성립하는 멤버마다 한 번씩, 멤버 순서대로 제시하고, 그때마다 `occasion.target`을 그 멤버로 묶습니다.
스텝은 멤버마다 후보를 하나씩 나열하고, `--json`은 후보의 `for`에 멤버를 적습니다:

```
── step 1 · dailyReset (select: sequence) ──────────────
  ✓ g.bday for aria [beat, priority 0]
  ✓ g.bday for cyra [beat, priority 0]
  ✗ g.bday for bram [beat, priority 0] — when: false
  → g.bday for aria
  → g.bday for cyra
@narrator: Happy birthday, aria!
@narrator: Happy birthday, cyra!
```

스텝의 `winner`, `offered`, `notOffered`, `presented`는 트랜스크립트처럼 한 멤버의 제시를
`<beat id> for <member>`로 가리킵니다. 로어 문서 `greet`의 비트 `wave`가 `for="kind:npc"`(`mara`, `tomas`),
`once="run"`일 때:

```yaml
steps:
  - occasion: morning
    expect: { presented: [greet.wave for mara, greet.wave for tomas] }
  - occasion: morning
    expect: { notOffered: [greet.wave for mara], winner: none }
```

비트가 제시되지 않는 멤버는 did-you-mean이 붙은 사용법 오류입니다:
`` `expect.notOffered` names `greet.wave for mra`, and `mra` is no member `greet.wave` is presented for — did you mean `mara`? (members: mara, tomas) ``.
`once`는 멤버마다 따로 소진되므로 Mara에게 재생된 `once: run` 비트도 Tomas에게는 재생될 수 있고, 같은 이유로
소진된 멤버는 한 줄로 찍힙니다(`✗ greet.wave for mara, tomas [beat, priority 0] — once: run — already presented this run`).

[멤버마다 한 번](/language/beats/#once-per-member-for)을 참고하세요.

## 선택

엔진이 계기 `O`를, 선택적으로 대상 `T`를 위해 발생시키면:

1. **후보**는 `on: O`이고 `target`이 없거나 `T`와 같은 비트, 그리고 dsl 0.26.0부터는 `T`의 멤버가 속한
   종류 `K`를 `kind:<K>`로 대상 삼는 비트입니다([종류 대상](#종류-대상)). 대상 없이 발생한 계기에는 대상이
   없는 후보만 있습니다.
2. 후보는 `after:`가 성립하고(씬 비트), `when`이 성립하고, `once`가 소진되지 않았을 때 **자격이
   있습니다**. 씬(또는 번들 비트)의 `once`: `run` — 이번 런에 아직 제시되지 않음, `user` — 한 번도
   제시되지 않음, `day` / `slot` — 시계의 날 / 슬롯이 마지막으로 바뀐 뒤 아직 제시되지 않음(dsl 0.24.0 §1),
   `false` — 소진되지 않음. 엔트리의 `once`: `run` — `entry.<id>.read`가 세워지지 않음, `user` —
   `entry.<id>.everRead`가 세워지지 않음, `day` / `slot` — 씬과 같음, 없음 — 소진되지 않음.
3. 자격 있는 비트는 **priority 내림차순, 그다음 프로젝트 순서**로 정렬됩니다: 문서 경로, 그다음 문서 안의
   선언 순서 — `project.index.json`의 `beats` 순서입니다. 같은 계기의 씬·엔트리·번들 비트는 한 목록에서
   경쟁합니다. priority가 같으면 종류 비트는 다른 후보들 뒤에 옵니다(dsl 0.26.0 §5).
4. `select: first`는 `also`가 아닌 첫 번째 자격 있는 비트를 제시하고, 이어서 자격 있는 `also` 비트를
   모두 제시합니다. `select: all`은 정렬된 목록을 제시한 뒤 플레이어가 고른 것을 제시하고,
   `select: sequence`는 정렬된 목록 전체를 제시합니다.
5. **자격 있는 비트가 없으면** — 계기는 스토리 없이 지나가고, 그 순간에 대한 엔진의 기본 동작이
   적용됩니다.

선택은 결정적입니다: 같은 상태, 팩트, 제시 이력이면 어느 엔진에서든 같은 비트를 고릅니다. 가중 무작위나
쿨다운은 그 위에 얹는 엔진 정책이며, 참조 도구는 정확히 이 순서를 구현합니다. `select: first` 승자를
파일 순서가 정하는 경우 — priority가 같고 `when`이 서로 배타적임을 증명할 수 없는 두 비트 —
`check-project`는 `W-BEAT-PRIORITY-TIE`를 경고합니다. 이 경고와 다른 비트 권고는 [비트](/language/beats/)
문서를 보세요.

`select: sequence`에서 후보는 계기가 발생할 때 정해지고, 그 발생의 비트 하나가 재생된 뒤로는 뒤의 비트마다
제 차례 직전에 다시 판정됩니다: 앞선 비트가 `when`을 거짓으로 만든 비트는 건너뛰고, 참으로 만든 비트는
재생됩니다. 그런 후보의 줄 끝에는 `(judged at its turn, after an earlier beat of this raise)`가 붙습니다.
`also` 비트는 계기가 발생할 때 한 번 판정됩니다. 퀘스트 라이프사이클은 제시마다 정산되므로
[기한](#기한과-대상-지정-목표)은 비트 사이에서 판정되고, 그 발생이 답하는 `<on event>` 핸들러는 모든 비트
뒤에 실행됩니다. `W-BEAT-SHADOWED`와 `W-BEAT-PRIORITY-TIE`는
승리를 다투지 않는 `also` 비트를 무시합니다.

## `lute play`

```console
$ lute play <PROJECT_DIR> --script <FILE> [--json] [--ir] [--quiet] [--no-derive] [--events] [--explain <ATOM>]…
```

### 런타임 이벤트 스트림(`--events`)

`--events`는 재개 가능한 런타임 스트림을 compact JSON Lines로 출력합니다. 첫 줄에는 시드와 그 출력이,
그 다음 줄부터는 플레이어가 보낸 각 입력과 출력이 짝을 이룹니다(`choose`, `bridgeResult` 응답도 포함).
모든 출력에는 `eventVersion: "0.38.0"`이 붙습니다. `--json`, `--ir`, `--explain`, `--dump-conditions`와
함께 사용할 수 없습니다.

허브 적합성 프로젝트에서 얻은 짧은 실제 스트림은 다음과 같습니다(첫 선택 뒤에 두 번째 대기가 열립니다).

```json
{"seed":{"derive":true},"output":{"await":{"type":"idle"},"eventVersion":"0.38.0","events":[]}}
{"input":{"occasion":"visit","type":"raiseOccasion"},"output":{"await":{"menu":{"construct":"hub","document":"hub.lute","id":"desk","options":[{"exit":false,"id":"take","once":true,"verdict":"open"},{"exit":true,"id":"leave","once":false,"verdict":"open"}],"position":"001-0200","presentation":0},"request":1,"type":"awaitChoice"},"eventVersion":"0.38.0","events":[{"beat":"hub","document":"hub.lute","kind":"scene","occasion":"visit","type":"presentation"},{"document":"hub.lute","record":{"kind":"line","lineId":"hub.narrator_0010","position":"001-0100","role":"narration","speaker":"narrator","text":"Before the hub.","voiceKey":"hub.narrator-0010"},"type":"record"}]}}

- `<PROJECT_DIR>` — 프로젝트 루트(`lute.project.yaml`과 그 플러그인). 프로젝트는 `compile --all`과 같은
  게이트와 선언 유니온(씬, 퀘스트, 로어 문서)으로 메모리에서 통째로 컴파일됩니다. 문서들이 한 상태 경로를
  두 타입으로 선언한 프로젝트는 그 게이트에서 실패하고(`E-STATE-DECL-CONFLICT`, dsl 0.26.0 §2.1), 플레이는
  시작을 거부합니다(종료 코드 1). 프로젝트는 플레이마다 한 번 로드됩니다. dsl 0.26.0부터 팩트 분석을 비트마다가
  아니라 프로젝트 루트마다 한 번 준비하므로, Monster League(비트 818개, 정적 팩트 211개)의 로드는 약 12초에서
  약 1.5초로 줄었고, 긴 플레이의 시간은 로드가 아니라 스텝이 정합니다.
- `--script <FILE>` — 필수: 플레이 스크립트, `*.play.yaml` 파일.
- `--json` — 같은 트랜스크립트를 stdout에 JSON 객체 하나로 출력합니다.
- `--ir` — 스테이징을 작성한 그대로의 지시어 대신 낮춰진 IR 레코드(`::bg`, `::actor`, 그리고 컴파일러가
  주입한 프리로드와 포즈 리셋)로 출력합니다. [트랜스크립트](#트랜스크립트)를 보세요.
- `--quiet` — 각 발생에서 적격이 아니었던 후보를 빼고 출력합니다. 승자, 대사, 퀘스트, 기대값은 그대로
  출력됩니다(dsl 0.27.0, round-5 T3-16). 이 플래그가 없으면 한 발생에서 `when: false`인 후보가 다섯 개
  이상일 때 한 줄로 접힙니다: `✗ 8 beats — when: false: a, b, c, …`. `--json`은 언제나 모든 후보를 나열합니다.
- `--no-derive` — 프로젝트의 Datalog 규칙을 적용하지 않습니다(dsl 0.22.0 §6). 스크립트의 `derive:`보다
  우선합니다. [파생과 `--explain`](#파생과---explain)을 보세요.
- `--explain <ATOM>` — 반복 가능: 플레이가 끝난 뒤, 그라운드 원자가 왜 성립하는지 또는 왜 성립하지
  않는지 출력합니다.

명령줄에서 따로 시드할 것은 없습니다: 상태, 팩트, 세이브, 결정, 단언이 모두 스크립트에 있으므로 하나의
플레이스루는 리뷰 가능한 파일 하나입니다.

이 절의 예제는 작은 로그라이크 프로젝트를 플레이합니다: 플레이어가 런마다 오르는 탑입니다. 플러그인은
계기 넷, 월드 이벤트 하나(`events` export — [매니페스트](/plugins/manifests/) 참고), 그리고 지급이
[상태에 적립되는](/plugins/manifests/#rewards-that-credit-state) 보상 종류 하나(dsl 0.23.0)를 선언합니다:

```yaml
occasions:
  townVisit: { select: first }
  talk:      { select: first, target: { prefix: npc, entity: person } }
  board:     { select: all, description: Notices pinned by the stair }
  runStart:  { select: sequence, description: A run begins at the foot of the stair }
```

```yaml
events:
  - name: storm
```

```yaml
rewardKinds:
  EMBERS: { credits: user.embers }
```

월드 스키마는 엔진에게 층수와 런 카운터를, 플레이어에게 불씨(embers) 주머니를 주고, 예약된 처치 팩트와
규칙 하나 — 워든은 처치될 때까지 위협이다 — 를 줍니다:

```yaml
state:
  run.floor:   { type: int, default: 0, owner: engine }
  user.runs:   { type: int, default: 0, owner: engine }
  user.embers: { type: int, default: 0 }
entities:
  person: { members: [maud, oskar] }
  foe:    { members: [warden, hound] }
relations:
  boss:   { args: [foe], tier: run }
  slew:   { args: [foe], tier: run, reserved: true }
  threat: { args: [foe], derive: true }
facts:
  - "boss(warden)"
rules:
  - "threat(F) :- boss(F), not slew(F)"
```

비트: `town.idle`은 매번 `townVisit`에 응답하고(`once: false`), `town.victory`(priority 10,
`when: "!holds('threat', ['warden'])"`, 역시 `once: false`)는 워든이 더 이상 위협이 아니게 되는 순간 그보다
앞섭니다. `maud.talk`는 `npc.maud`를 위한 `talk`에 응답하고, 오스카의 [번들 비트](#번들-비트) 둘 —
`oskar.hunt`와 곁들이는 대사 `oskar.rumor` — 는 `npc.oskar`를 위한 `talk`에 응답합니다.
`start.gear`(priority 10, `once: false`)와 `start.recap`(`once: false`,
`when: "has(prev.run.floor)"`, [지난 런](#런-경계)의 층수)은 `runStart`에 응답하고, 엔트리 셋이
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

두 번째 문서 `quests/hound.lute`에는 `oskar.hunt`가 수락하는 퀘스트가 있습니다. 목걸이에는 기한이 있고,
보고는 오스카를 기다리며, 보상은 불씨로 지급됩니다:

```lute
<quest id="houndHunt" title="The hound's collar">
  <reward kind="EMBERS" amount="50"/>
  <objective id="collar" title="Take the collar before floor four" done="holds('slew', ['hound'])" by="run.floor >= 4"/>
  <objective id="report" title="Bring it to Oskar" on="talk" target="npc.oskar" done="holds('slew', ['hound'])"/>
  <on event="questFailed">
    @oskar: Floor four already? Then it's gone to ground.
  </on>
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
| `bridges` | 브리지 결과를 읽는 플러그인 호출에 대한 응답. 플레이 전체에 걸쳐 호출 순서대로 소비됩니다(dsl 0.24.0 §5) — [브리지 호출에 답하기](#브리지-호출에-답하기) 참고. |

모든 스텝은 정확히 한 가지 일을 합니다 — `occasion`을 발생시키거나, `engine` 쓰기를 적용하거나,
`newRun`을 시작하거나, `event`를 발생시키거나, 선언된 시계를 움직이거나(`advance`), 플레이스루를
끝냅니다(`end: true`) — 그리고 어떤 스텝이든 `label`, `repeat` 횟수, [`expect:`](#기대값), 그리고 자신만의
[`bridges:`](#브리지-호출에-답하기) 응답을 가질 수 있습니다. 단, `end` 스텝은 `label`만 받습니다. 예외인
조합은 하나: `advance`는 같은 순간의 `engine:` 쓰기를 함께 가질 수 있습니다. 스텝은
`include: <file>`일 수도 있는데, 그 파일의 스텝을 그 자리에 끼워 넣습니다([시계 앞으로 돌리기](#시계-앞으로-돌리기)
참고). dsl 0.27.0부터 이 스텝은 `repeat: n`(파일을 n번 끼워 넣음)과 자신만의 `choose:` / `bridges:`를 가질 수
있습니다. 이것들은 끼워 넣는 모든 스텝에 대해 스크립트 자신의 것 위에 놓입니다 — 키별로, 태그별로, 반복마다
처음부터 소비되고, 구간이 끝나면 버려집니다 — 그래서 여러 학기짜리 플레이는 학기마다 자신의 선택을 그 학기
옆에 적습니다. include가 덮어쓴 키에 대한 스크립트 자신의 목록은 그 자리에 머뭅니다: 최상위
`choose: { firstLook: [soren, mika] }`에 첫 학기가 `choose: { firstLook: ren }`으로 include하면, 둘째 학기는
`mika`가 아니라 목록의 첫 항목 `soren`을 고릅니다. 위치 목록을 include별 `choose:`로 바꿀 때는 한 학기씩이
아니라 모든 학기를 한꺼번에 바꾸세요. 옆에 `label`이나 `expect:`를 쓰면 사용법 오류입니다. 스텝의 사용법 오류는 그 오류가 가리키는
키의 파일, 줄, 열을 댑니다 — 스텝이 포함된 파일에서 왔으면 그 파일에서, 뒤에
`(included from <play>:<line>:<col>)`을 붙여서 — 그리고 계기 이름과 키에는 did-you-mean을 붙입니다. 탑에
대해 모든 모양을 한 번씩 둘러보면:

```yaml
state: { user.runs: 2 }                   # path -> scalar literal, over the declared defaults
facts: ["slew(hound)"]                    # ground facts, added to the project's seed facts
entriesRead: { user: [notice] }           # the save this play starts from
steps:                                    # required, non-empty
  - occasion: townVisit                   # raise an occasion
    expect: { winner: town.idle }         # assert what this step did
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
  - occasion: townVisit
    repeat: 2                             # the same step, twice
expect:                                   # assert the end of the play
  quests: { climb: active, veteran: complete }
  notFacts: [slew(warden)]
```

이 스크립트의 트랜스크립트는 [트랜스크립트](#트랜스크립트)의 예제입니다.

`state:`, `facts:`, `choose:`, `bridges:`는 [`lute trace --mock`](/tooling/tracing/) 파일과 정확히 같은
문법을 씁니다.

- `state:` 시드는 선언된 경로 — `scene.*`는 안 됨 — 를 가리키며, 값은 선언된 타입에 맞아야
  합니다(숫자 경로면 `int` 또는 `double`, enum이면 멤버). 그 밖의 경우는 사용법 오류(종료 코드 2)입니다.
  `quest.<id>.state` 시드(`state: { quest.lostCup.state: active }`)는 `quests:` 항목과 똑같이 처음부터
  그 퀘스트의 라이프사이클 상태가 되고, `quest.<id>.objectives.<oid>.done: true` 시드는 세이브가 이미
  완료한 목표입니다 — [세이브에서 시작하기](#세이브에서-시작하기)를 보세요. `prev.run.<path>` 시드(dsl 0.23.0)는
  지난 런이 끝났을 때 `run.<path>`가 가졌던 값이며, 그 run 경로의 타입을 따릅니다 —
  [런 경계](#런-경계)를 보세요.
- `facts:` 항목은 선언된 비파생 관계의 그라운드 원자로, 인자 수가 맞고 닫힌 인자 도메인의 멤버를 써야
  합니다. **예약된(reserved)** 관계도 허용됩니다 — 그것을 단언하는 주체가 바로 엔진입니다.
- `choose:`의 결정 하나는 그 branch나 hub가 제시될 때마다 답합니다. **hub**의 목록은 방문 순서 하나이며
  제시될 때마다 다시 쓰입니다. **branch**에 두 개 이상의 목록을 주면 플레이스루 전체에 걸쳐 제시될
  때마다 순서대로 하나씩 소비됩니다. 그래서 사흘 밤 재생되는 씬이 밤마다 다르게 결정할 수 있습니다.
  목록이 바닥나면 워크는 그 사실을 알리며 미완료(종료 코드 3)로 멈춥니다.
- 그 순간 메뉴가 제시하지 않는 결정은 워크를 오류(종료 코드 1)로 멈춥니다: 가드가 거짓인 선택지, 또는
  이미 고른 `once` hub 선택지(`lute trace`와 같은 `E-TRACE-CHOICE`).

### 계기 스텝

`{ occasion, target?, payload?, engine?, pick?, choose?, expect?, bridges? }`는 엔진과 똑같이 계기를 발생시킵니다.

- `target` — 대상과 함께 선언된 계기에는 필수, 대상 없는 계기에는 거부됩니다. 대상 도메인이 있으면
  대상은 그 도메인의 `<prefix>.<member>`여야 하며, 벗어나면 did-you-mean이 붙은 사용법 오류입니다(`` target `npc.mawd` is outside occasion `talk`'s domain `npc.<person>` (`npc.maud`, `npc.oskar`) — did you mean `npc.maud`? ``).
  어떤 비트도 응답하지 않는 멤버는 합법입니다: 계기가 그냥 지나갑니다. 대상은 그 스텝이 판정할
  [대상 지정 목표](#기한과-대상-지정-목표)도 정합니다.
- `payload` — `payload:`를 선언한 계기에서 그 발생이 넘기는 타입 있는 값입니다(dsl 0.27.0 §3):
  `payload: { copies: 2 }`. 이 발생의 비트들은 `occasion.payload.copies`로 읽고, 다음 발생은 그 값 없이
  시작합니다. 계기가 선언하지 않은 필드, 타입에 맞지 않는 값, `payload:`를 선언하지 않은 계기에 준 payload는
  사용법 오류입니다(`` step 1: `payload.copy` — occasion `summon` declares no payload field `copy` (declared: `copies`) ``).
  [계기 페이로드](/language/beats/#occasion-payloads)를 참고하세요.
- `engine` — 같은 순간의 엔진 쓰기입니다(dsl 0.27.0 §4). [엔진 스텝](#엔진-스텝)과 같은
  `{ state?, facts?, retract?, accept? }`이며, 먼저 스텝의 `· engine` 레코드로 적용되고 퀘스트가 정착한 뒤에
  계기가 발생하므로, 그 비트들과 계기의 관문이 이 쓰기를 봅니다. `target: room.office`와
  `engine: { facts: [canEnter(office)] }`를 가진 `- occasion: enter`는 한 스텝에 문을 열고 들어갑니다.
- [`raisedWhen:` 관문](/plugins/manifests/)을 선언한 계기는 관문이 성립하는 동안에만 발생합니다(dsl 0.27.0 §4).
  관문이 거짓일 때 그 계기를 발생시키는 스텝은 플레이를 멈춥니다(종료 코드 1):
  `` step 1: E-OCCASION-GATE: the engine raises `enter` for `room.office` only when `holds('canEnter', [occasion.target])` (its `raisedWhen`), which is false here since `canEnter(office)` does not hold — make it hold first (an `engine:` write, an earlier step), or drop the step ``.
  멈춤은 적힌 스텝 자리(`plays/locked.play.yaml:2:5: step 1: …`)에 위치가 잡히고, 관문을 거짓으로 만든 읽기를
  댑니다 — 유도 사실이면 그 규칙이 놓친 전제까지. `lute test`도 관문이 있는 계기의 비트를 같은 규칙으로
  판정합니다: 그런 비트에 대한 `eligible: true`는 관문과 그 거짓 읽기(또는 성립한 `terminal:`)를 대며
  불일치하고, `--json`에서 그 기대값은 `notRaised: { occasion, reason: "gate" | "terminal", condition, falseReads }`를
  가집니다. 게임이 끝난 뒤(스키마의 [`terminal:`](#게임이-끝났을-때)이 성립)에는 모든 `occasion:` 스텝이 같은
  식으로 거부됩니다.
- `pick` — `select: first`와 `select: sequence`에는 거부됩니다(`` step 1: `pick: start.gear` applies only to a `select: all` occasion; `runStart` is `select: sequence` ``):
  그 계기에 응답하는 비트의 id(그 순간 자격이 없는 pick은 오류, 종료 코드 1) 또는 `pick: none`.
  `select: all` 스텝은 목록이 비어 있지 않으면 `pick`이 필요합니다 — 없으면 워크는 그 자리에서 제시된 목록을
  대며 오류(종료 코드 1)로 멈춥니다(`` step 1: occasion `board` is `select: all` and offers [notice, memo] — name the beat the player takes with `pick:` (or `pick: none` to close the list) ``).
  자격 있는 비트가 없으면 고를 것도 없으므로, 스텝은 `pick: none`으로 지나가고 헤더에
  `(select: all, pick: none (nothing offered))`가 찍힙니다(dsl 0.24.0). `none`은 목록을 닫습니다 —
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
- 스크립트가 엔트리를 가리키는 곳이면 어디서든 — `pick:`, 세이브의 `entriesRead:`, 스텝의 `winner`,
  `offered`, `notOffered`, `presented` — 엔트리를 `<문서 id>.<엔트리 id>`로 쓸 수 있습니다(dsl 0.26.0 §8).
  그래서 `lore.tomas.tomasOil`은 토마스의 기름 엔트리가 어느 문서에 있는지 밝힙니다. 트랜스크립트와 불일치는
  엔트리 id를 출력합니다. 번들 비트는 언제나 정식 id `<문서 id>.<비트 id>`로 가리킵니다.

### 엔진 스텝

`{ engine: { state?, facts?, retract?, accept? } }`는 엔진이 계기 사이에 하듯이 엔진이 소유한 것을 씁니다:

- `state:` — 선언된 경로 → 리터럴, 또는 `int` 경로의 현재 값에 더하는 `{ add: <int> }`. 선언되지
  않은 경로, `scene.*`, `quest.*` 경로는 거부됩니다: 퀘스트 상태는 전이가 핸들러와 보상을 발동시키는
  퀘스트 라이프사이클의 것이니, 세이브의 퀘스트 상태는 최상위 `quests:`로 시드하세요.
- `facts:` / `retract:` — 선언된 기반 관계의 그라운드 원자, **예약된 관계 포함**. 최상위 `facts:`와 같은
  검사를 받습니다. 성립하지 않는 원자를 철회하면 거부되지 않고 기록됩니다.
- `accept:`(dsl 0.26.0) — 엔진이 이 순간에 수락하는 퀘스트 id들. 플레이어가 게시판에서 공지를 떼어 갈 때
  엔진이 하는 일입니다: 각각 수락 방식 퀘스트(`start` 없음, 보통 `accept="external"`)여야 하며, 스텝 바로 뒤의
  정착에서 활성화됩니다. 트랜스크립트는 `quest <id> accepted (engine)`을 출력하고, `--json`은 스텝의 쓰기
  사이에 `{ "kind": "accept", "quest": "<id>", "by": "engine" }`을 기록합니다. 이미 활성이거나 완료·실패한
  퀘스트는 다시 수락되지 않습니다: 트랜스크립트는 `note: quest <id> is already active — engine accept ignored`(그
  상태로)를 출력하고, `--json`은 `{ "kind": "acceptIgnored", "quest": "<id>", "status": "<status>" }`을 기록하며,
  아무것도 바뀌지 않습니다. `start`가 있는 퀘스트나 어떤
  퀘스트도 선언하지 않은 id는 사용법 오류(종료 코드 2)입니다:
  `` step 1: `engine.accept` names `auto`, which is no accept-driven quest of this project (a quest with no `start`, e.g. `accept="external"`) ``.
  0.26.0 전에는 그런 퀘스트를 최상위 `quests:`로만 시드할 수 있었고, 스텝 1부터 활성이었습니다.

쓰기는 그 순서대로 — 상태, 팩트, 철회, 수락 — 적용됩니다. 이 스텝은 아무것도 제시하지 않고 계기도 발생시키지
않으며, 바로 뒤에 퀘스트 라이프사이클이 정착하므로 쓰기 하나로 그 스텝에서 퀘스트가 완료되거나 실패할 수
있습니다. [`owner: engine`](/state/state-model/#owner-engine) 상태가 바로 이 스텝을 위한 것입니다:
콘텐츠는 그것을 `::set`할 수 없지만 `engine:` 스텝은 쓸 수 있습니다 — `scene.*`와 `quest.*`를 제외한
다른 선언된 상태도 마찬가지입니다.

```yaml
steps:
  - occasion: townVisit
  - label: the warden falls on floor six
    engine:
      state: { run.floor: 6 }
      facts: [slew(warden)]
  - occasion: townVisit
  - engine:
      retract: [slew(warden), slew(hound)]
  - occasion: townVisit
```

```
── start ──────────────
  quest climb -> active
  quest veteran -> active
  quest notices -> active
── step 1 · townVisit ──────────────
  ✓ town.idle [scene, priority 0]
  ✗ town.victory [scene, priority 10] — when: false
  → town.idle
@maud: Quiet night.
── step 2 (the warden falls on floor six) · engine ──────────────
  set run.floor = 6
  assert slew(warden)
  climb.high done
  quest climb -> complete
── step 3 · townVisit ──────────────
  ✓ town.victory [scene, priority 10]
  ✓ town.idle [scene, priority 0]
  → town.victory
@maud: The warden is dead. I never thought I'd say it.
── step 4 · engine ──────────────
  retract slew(warden)
  retract slew(hound) (did not hold)
── step 5 · townVisit ──────────────
  ✓ town.idle [scene, priority 0]
  ✗ town.victory [scene, priority 10] — when: false
  → town.idle
@maud: Quiet night.
── end: complete (5 steps) ──────────────
```

처치로 `threat(warden)`이 더 이상 파생되지 않으므로 스텝 3에서 `town.victory`가 자격을 얻고, 철회가
그것을 다시 닫습니다.

수락도 엔진의 일입니다. [예제](#예제)의 마을에 게시판 퀘스트 `parcel` — `accept="external"`로 선언하고 목표
하나가 `on="talk" target="npc.tomas"`인 퀘스트 — 을 더하고, 플레이어가 플레이 도중에 공지를 떼어 갑니다:

```yaml
steps:
  - occasion: townVisit
  - label: the player takes a notice from the board
    engine: { accept: [parcel] }
    expect: { quests: { parcel: active } }
  - occasion: talk
    target: npc.tomas
expect:
  quests: { parcel: complete }
```

```
── step 1 · townVisit ──────────────
  ✓ town.welcome [scene, priority 10]
  ✗ town.morning [scene, priority 0] — when: false
  → town.welcome
::bg{location="town" time="day"}
@narrator: The lamps along the square are lit — all but the one by the door.
── step 2 (the player takes a notice from the board) · engine ──────────────
  quest parcel accepted (engine)
  quest parcel -> active
── step 3 · talk → npc.tomas ──────────────
  ✓ tomasBusy [entry, priority 0]
  ✗ tomasOil [entry, priority 10] — when: false
  → tomasBusy
  entry tomasBusy (first read)
@tomas: Busy.
  parcel.deliver done
  quest parcel -> complete
── end: complete (3 steps) ──────────────
── expect: every expectation held ──────────────
```

### 이벤트

`{ event: <name> }`은 어떤 플러그인이 선언한 월드 이벤트를 trace의 `events:`와 똑같이 발생시킵니다: 모든
**활성** 퀘스트의 `<on event="<name>">` 핸들러가 실행되고, 라이프사이클이 정착합니다. 계기를 가리키는
`event:`나, 어떤 플러그인도 계기로 선언하지 않은 월드 이벤트를 가리키는 `occasion:`은 올바른 키를 알려
주는 사용법 오류입니다
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
  <on event=storm> of quest climb skipped — quest complete
── end: complete (3 steps) ──────────────
```

`climb`이 스텝 2에서 완료되었으므로, 그 핸들러는 두 번째 폭풍에 더 이상 응답하지 않으며, 스텝은 아무것도
출력하지 않는 대신 그렇다고 알립니다(dsl 0.24.0).

계기와 월드 이벤트는 이름을 공유할 수 있습니다 — 엔진이 둘을 한꺼번에 발생시키므로 `occasions:`와
`events:` 양쪽에 `bossDefeated`를 선언하는 경우입니다. 그런 계기를 발생시키면 이벤트도 함께 발생합니다:
모든 활성 퀘스트의 `<on event="bossDefeated">` 핸들러가 **먼저** 실행되고, 그다음 계기가
`on="bossDefeated"` 목표를 판정하므로, 목표의 `done`은 핸들러가 방금 쓴 값을 읽을 수 있습니다. 스텝은
그 둘보다 앞서 비트를 제시합니다. `lute run`과 `lute trace`도 계기를 같은 방식으로 발생시킵니다.

### 플레이스루 끝내기

`end: true`는 플레이스루를 완료(종료 코드 0)로 끝냅니다. 이 스텝은 다른 일을 하지 않으며, 이후의 모든
스텝은 건너뛴 것으로 표시되고, `── end:` 줄이 그 스텝을 댑니다:

```yaml
steps:
  - occasion: townVisit
  - end: true
  - label: never reached
    occasion: townVisit
```

```
── start ──────────────
  quest climb -> active
  quest veteran -> active
  quest notices -> active
── step 1 · townVisit ──────────────
  ✓ town.idle [scene, priority 0]
  ✗ town.victory [scene, priority 10] — when: false
  → town.idle
@maud: Quiet night.
── step 2 · end (the playthrough ends) ──────────────
── step 3 (never reached) · skipped (the playthrough ended) ──────────────
── end: `end: true` at step 2 (1 later step skipped) ──────────────
```

`--json`에서 이 스텝은 `{ "step": 2, "end": true, … }`이고, `endReason`은 `── end:` 뒤의 텍스트이며,
`skipped`는 재생되지 않은 스텝을 나열합니다(`[{ "step": 3, "label": "never reached" }]`). `true`만
받습니다 — `end: false`는 사용법 오류(종료 코드 2)이고, `end` 옆의 `repeat`나 `expect`도 마찬가지입니다:
기대값은 그 앞 스텝이나 최상위에 두세요. 최상위 기대값은 여전히 플레이의 끝을 판정합니다.

콘텐츠의 `::end`는 플레이스루를 끝내지 **않습니다**. 그것이 실행된 제시만 — 퀘스트 핸들러 안이라면 그
퀘스트 문서의 진행만 — 끝내고, 플레이는 다음 스텝으로 이어집니다. [각 스텝이 하는 일](#각-스텝이-하는-일)을
보세요. 0.23.1 전에는 씬의 `::end`가 플레이 전체를 멈췄습니다. 그 동작에 기대던 스크립트는 씬이 끝나는
스텝 뒤에 `- end: true`를 추가하세요.

### 게임이 끝났을 때

스키마는 게임이 끝나는 때를 선언할 수 있습니다: `terminal: "run.fate == 'taken'"`(dsl 0.27.0 §4,
[스키마](/state/schemas/) 참고). 그것이 성립하면 엔진은 어떤 계기도 발생시키지 않고, `lute play`도 그렇게
합니다. 그것을 성립시킨 스텝이 그렇다고 알리고, 모든 스텝이 재생된 플레이스루는 그 상태로 끝납니다(종료 코드 0):

```
── step 1 · knock ──────────────
  ✓ taken [scene, priority 0]
  → taken
  set run.fate = "taken"
@narrator: Something takes you.
  note: the game is over — `terminal: run.fate == 'taken'` holds, so the engine raises no occasion from here (`occasion:` / `advance:` steps are refused; `newRun: true` starts a new run)
── end: terminal — `terminal: run.fate == 'taken'` holds ──────────────
```

`--json`에서는 루트에 `"exit": "complete"` 옆으로 `"end": "terminal"`이 붙고, 최상위 `expect: { end: terminal }`이
그것을 단언합니다. 그 뒤의 `occasion:` 스텝은
`E-OCCASION-GATE`입니다(종료 코드 1:
`` step 2: E-OCCASION-GATE: the game is over — `terminal: run.fate == 'taken'` holds, so the engine raises no occasion (`enter` for `room.lobby` included); start a new run (`newRun: true`) to play on, or, if the engine raises `enter` outside a run too (a title screen, a gallery), declare it `outsideRun: true` ``).
그 뒤의 `advance:`도 마찬가지이며 시계도 움직이지 않습니다. `engine:` 스텝은 엔진 자신의 쓰기이므로 여전히 받아들여집니다.
[`outsideRun: true`](/plugins/manifests/)를 선언한 계기(타이틀 화면, 런 사이의 갤러리)는 그래도 발생합니다.

`newRun: true`는 새 런을 시작하지만, 플레이가 이어지는 것은 새 런이 `terminal:`을 거짓으로 만들 때뿐입니다.
런 상태(`run.*`, 런 티어 퀘스트나 관계)를 읽는 `terminal:`은 초기화되고, 새 런이 유지하는 것(`user.*`,
`visited(…)`)을 읽는 것은 그대로 성립하며 `check-project`가 스키마에 `W-TERMINAL-PERSISTENT`를 경고합니다.
다음 발생은 멈춥니다:
`` step 4: E-OCCASION-GATE: the game is over — `terminal: user.crowned` holds, so the engine raises no occasion (`townVisit` included); it still holds after a new run: it reads `user.crowned`, which a new run keeps ``.

모든 런을 넘어 남도록 의도한 결말은 스키마에서 그렇다고 적습니다:
`terminal: { when: "user.crowned", persists: true }`([스키마](/state/schemas/#an-ending-that-outlives-runs-persists-true)).
그러면 `check-project`는 경고하지 않고, `lute play`는 새 런을 권하는 대신 게임이 영영 끝났다고 말합니다:
게임을 끝낸 스텝, 그 뒤의 `newRun: true`, 이후의 발생이 모두 그렇게 말하며 어느 것도 `newRun`을 권하지
않습니다:

```
── step 1 · knock ──────────────
  ✓ crown [scene, priority 0]
  → crown
  set user.crowned = true
@narrator: The crown is yours.
  note: the game is over for good — `terminal: user.crowned` holds and persists (`persists: true`), so the engine raises no occasion from here, in this run or any later one (`occasion:` / `advance:` steps are refused; a new run does not reopen it)
── step 2 · new run ──────────────
  note: the game is over for good — the new run does not reopen it: `terminal: user.crowned` holds and persists (`persists: true`), so the engine raises no occasion
── step 3 · visit ──────────────
  (not raised: the game is over)
── halted: s.play.yaml:4:5: step 3: E-OCCASION-GATE: the game is over — `terminal: user.crowned` holds, so the engine raises no occasion (`visit` included); the ending persists (`persists: true`), so the game is over for good and no new run reopens it — drop the step, or, if the engine raises `visit` outside a run too (a title screen, a gallery), declare it `outsideRun: true` ──────────────
```

`terminal:`은 계기가 발생할 때 판정되므로, 그것을 성립시키는 발생은 끝까지 진행됩니다: 게임을 끝내는 쓰기가
있는 비트는 마지막 줄까지 재생되고, `select: sequence` 발생의 나머지 비트와 그 발생이 정산한
`questComplete` / `questFailed` 핸들러가 뒤따릅니다. 그래서 에필로그는 `terminal:`을 성립시키는 쓰기 뒤나,
`terminal:`이 읽는 퀘스트의 `<on event="questComplete">` 핸들러에 둡니다.

플레이스루에서 "끝"은 다섯 가지이며, 게임을 끝내는 것은 `terminal:`뿐입니다:

| 쓰는 것 | 끝나는 것 | 플레이는 이어지나? |
|---|---|---|
| 콘텐츠의 `::end` | 그것이 실행되는 제시(퀘스트 핸들러에서는 그 퀘스트 문서의 진행) | 예, 다음 스텝으로 |
| 스크립트의 `- end: true` | 그 스텝에서 플레이스루; 이후 스텝은 건너뜀 | 아니요 — `end: complete` |
| 스키마의 `terminal:` | 게임: 새 런이 거짓으로 만들 때까지 엔진은 계기를 발생시키지 않음(`outsideRun` 제외). `persists: true`면 영영 | 그것을 초기화하는 `newRun` 뒤에만 — `end: terminal`. `persists: true`면 이어지지 않음 |
| 시계의 `last:` / `days:` | 시계: `clock.ended`가 참이 되고, 마지막 위치를 넘는 `advance:`는 플레이를 멈춤(`E-CLOCK-END`) | 예, 시계를 옮기지 않는 스텝으로; 거기서 게임을 끝내려면 `terminal: "clock.ended"` |
| `expect: { end: … }` | 없음: 워크가 어떻게 끝났는지 단언(`complete`, `terminal`, `incomplete`, `error`) | — |

관문이 있는 시계 발생: 시계가 발생시키는 계기(`raise.slot`, `dayStart`, `dayEnd`)의 `raisedWhen:` 관문이
시계가 선 자리에서 거짓이면 전진은 그 계기를 발생시키지 않습니다. 오류가 아닙니다: 시계는 움직이고 퀘스트도
정착하며, 스텝에 메모가 붙습니다(`--json`: 스텝의 `notes`):
`` note: `hourStrikes` was not raised at day 1 h01 — its `raisedWhen: run.hp > 1` is false there; the clock moved on without it ``.

### 시계 앞으로 돌리기

스키마가 [시계](/language/clock/)를 선언하면(dsl 0.24.0 §1), `advance: slot`, `advance: <n>`(그만큼의 슬롯),
`advance: day`(시계가 어느 슬롯에 있든 다음 날의 첫 슬롯)가 한 스텝에 시계를 앞으로 움직입니다. 이 스텝은
시계의 `day`와 `slot` 경로를 쓰고 — 마지막 슬롯을 넘으면 다음 날로 넘어갑니다 — 모든 퀘스트를 정착시킨
뒤(새 시각이 지나친 `by` 기한은 여기서, 아무것도 제시되기 전에 실패합니다), 시계가 `raise` 계기를
선언했다면 그 계기를 `occasion:` 스텝과 똑같이 발생시킵니다: 시계가 멈춘 곳에서 한 번이며, 지나친 슬롯에서는
발생시키지 않습니다. 그래서 다음 한 쌍은

```yaml
  - engine: { state: { run.slot: night } }
  - occasion: slotStart
```

(오후에서 시작하면) `- advance: slot` 하나와 같습니다. advance 스텝의 `choose:`는 그 스텝이 발생시키는 모든
계기에 답하고, `pick:`은 시계가 멈춘 곳에서 발생시키는 `slot` 계기에 답합니다. 그 `expect:`는 선택을 판정할
수 있습니다: `winner`, `offered`, `notOffered`는 스텝의 마지막 발생 — 시계가 멈춘 곳의 `slot` 계기, 또는
`dayStart` / `dayEnd`만 발생시키는 시계라면 마지막 자정의 것 — 을 판정하고, `presented`는 스텝이 제시한 비트
전부 — 자정의 발생 각각(아래), 그다음 마지막 발생의 것, 순서대로 — 를 나열하며, `options`는 그 모두를
합칩니다. dsl 0.27.0부터(round-5 T3-8) 불일치는 스텝을 마지막 발생에 놓고, `presented` 불일치는 각 비트에
그것을 제시한 발생을 붙입니다:
`` ✗ step 1 at advance day → slotStart: expect presented: expected [day.slot], actual [day.close (dayEnd at day 1 (Mon) morning), day.slot (slotStart at day 2 (Tue) morning)] ``.
`raise:`가 `slot` 계기를 정하지 않은 시계에서 `pick`은 사용법 오류이고
(`` step 1: `pick` answers the slot occasion an `advance:` raises where the clock stops, and the clock declares no `raise.slot` occasion ``),
아무것도 발생시키지 않는 시계에서는 `choose`와 선택 키도 사용법 오류입니다
(`` step 1: `expect.winner` judges what an `advance:` raises, and the clock declares no `raise:` occasion — the advance presents nothing ``).

전진이 `slot` 계기를 발생시키지 않고 위치를 지나칠 때 — `advance: 2`, 또는 아침에서 `{ to: night }` — 그 계기에
응답하는 비트가 있으면, 스텝은 지나친 위치를 날별로 대는 메모로 그렇다고 알립니다:
`` note: passed day 1 (Mon) afternoon without raising `slotStart` (2 beats answer it; an `advance:` raises it only where the clock stops) ``.
출발한 위치와 멈춘 위치는 나열하지 않으며, `advance: day`는 지나치는 위치가 없습니다: 그날의 나머지를 자고
넘깁니다. 비트 수는 그 계기에 응답하는 비트를 셉니다. 거기서 자격이 있든 없든 같습니다. `--json`은 메모를 스텝의
`notes`에, 위치를 `advance.passed`에 담습니다.

여기의 예제는 탑 대신 작은 하루 시계 프로젝트를 씁니다. 날과 슬롯은 엔진이 소유하고, 시계는 매 advance
뒤에 `slotStart`(`select: sequence` 계기)를 발생시킵니다:

```yaml
state:
  run.day:  { type: int, default: 1, owner: engine }
  run.slot: { type: { enum: [morning, afternoon, night] }, default: morning, owner: engine }
clock:
  day: run.day
  slot: run.slot
  slots: [morning, afternoon, night]
  raise: slotStart
  week: { length: 7, first: 0, labels: [Mon, Tue, Wed, Thu, Fri, Sat, Sun] }
```

`routine.morning`과 `routine.night`는 `slotStart`에 응답하고(priority 5, `once: slot`, 각각 자신의
`run.slot`으로 가드, 아침 쪽은 `{{clock.weekdayLabel}} morning.`을 출력), `cafe.wren`은 `once: day`로
`visit`에 응답하며, 퀘스트 `fest`(`start="true"`)에는 목표 하나 `on="visit" done="run.slot == 'night'" by="run.day >= 3"`이
있습니다:

```yaml
steps:
  - occasion: visit
  - occasion: visit
  - advance: slot
  - advance: slot
  - advance: slot
    expect: { presented: [routine.morning] }
  - advance: day
```

```
── start ──────────────
  quest fest -> active
── step 1 · visit ──────────────
  ✓ cafe.wren [scene, priority 0]
  → cafe.wren
@wren: Back again?
── step 2 · visit ──────────────
  ✗ cafe.wren [scene, priority 0] — once: day — already presented today
  → (no eligible beat — the occasion passes)
── step 3 · advance slot: day 1 (Mon) morning → day 1 (Mon) afternoon ──────────────
  set run.slot = "afternoon"
── step 3 · slotStart (select: sequence) ──────────────
  ✗ routine.morning [scene, priority 5] — when: false
  ✗ routine.night [scene, priority 5] — when: false
  → (no eligible beat — the occasion passes)
── step 4 · advance slot: day 1 (Mon) afternoon → day 1 (Mon) night ──────────────
  set run.slot = "night"
── step 4 · slotStart (select: sequence) ──────────────
  ✓ routine.night [scene, priority 5]
  ✗ routine.morning [scene, priority 5] — when: false
  → routine.night
@narrator: The lamps go out on day 1.
── step 5 · advance slot: day 1 (Mon) night → day 2 (Tue) morning ──────────────
  set run.day = 2
  set run.slot = "morning"
── step 5 · slotStart (select: sequence) ──────────────
  ✓ routine.morning [scene, priority 5]
  ✗ routine.night [scene, priority 5] — when: false
  → routine.morning
@narrator: Tue morning. The kettle sings.
── step 6 · advance day: day 2 (Tue) morning → day 3 (Wed) morning ──────────────
  set run.day = 3
  fest.go failed (by)
  quest fest -> failed (by)
── step 6 · slotStart (select: sequence) ──────────────
  ✓ routine.morning [scene, priority 5]
  ✗ routine.night [scene, priority 5] — when: false
  → routine.morning
@narrator: Wed morning. The kettle sings.
── end: complete (6 steps) ──────────────
── expect: every expectation held ──────────────
```

- **스텝 2** — `cafe.wren`은 `once: day`이므로 1일째의 두 번째 방문에서는 소진되어 있습니다:
  `once: day — already presented today`(`once: slot`이면 `already presented this slot`).
- **스텝 3–6** — 각 advance는 먼저 이동을 출력합니다 — `advance <slot | day | n>: <from> → <to>`이며, 위치는
  `day <n> (<요일 레이블>) <slot>`이고 요일 레이블은 시계의 `week:`가 레이블을 선언했을 때만 붙습니다 — 그
  아래에 쓰기와 정착이 옵니다. 발생은 같은 스텝 번호 아래에 이어집니다. 스텝 5는 밤을 넘겨 2일째 아침으로
  넘어가고, `once: slot` 덕분에 아침 일과가 다시 재생됩니다.
- **스텝 6** — `advance: day`는 3일째의 첫 슬롯으로 가고, 이동 직후의 정착이 `fest`를 실패시킵니다: 기한은
  순간이므로, 플레이어가 `visit`을 다시 발생시키든 말든 판정됩니다([기한](#기한과-대상-지정-목표) 참고).

`--json`에서 advance 스텝은 발생한 계기 자신의 필드(`occasion`, `candidates`, `winner`, …, 그리고 발생 뒤의
전이인 `quests`) 옆에 `advance: { by, from, to, writes, quests }`를 담습니다 — `by`는 `"slot"`, `"day"`, 또는
슬롯 수를 적은 문자열, `writes`는 움직인 날·슬롯 경로, `quests`는 이동 직후의 정착입니다. 스텝 6의 이동:

```json
{
  "by": "day",
  "from": "day 2 (Tue) morning",
  "to": "day 3 (Wed) morning",
  "writes": [ { "kind": "set", "path": "run.day", "value": 3 } ],
  "quests": [
    { "document": "quests/fest.lute", "commands": [
      { "kind": "objective", "quest": "fest", "objective": "go", "failed": true, "failedBy": "by" },
      { "kind": "quest", "quest": "fest", "state": "failed", "failedBy": "by" }
    ] }
  ]
}
```

`slot` 발생 없이 위치를 지나친 전진은 `advance`에
`"passed": { "occasion": "slotStart", "beats": 2, "at": [ { "day": 1, "slot": "afternoon" } ] }`를 더합니다(슬롯
없는 시계에서는 `slot`이 빠짐).

**자정: `dayEnd`와 `dayStart`.** `raise:`는 순간마다 계기를 정하는 맵일 수도 있으며, 키는 모두
선택입니다 — `raise: { slot, dayStart, dayEnd }`. 스칼라 `raise: slotStart`는 `slot` 형태입니다. advance는
넘는 자정마다 그 날의 마지막 슬롯에서, 날이 아직 넘어가기 전에 `dayEnd`를 발생시키고, 이어서 다음 날의 첫
슬롯에서 `dayStart`를, 마지막으로 멈춘 곳에서 `slot`을 한 번 발생시킵니다. 자정마다 한 번씩 멈춥니다: 시계가
그곳으로 움직이고, 퀘스트가 정착한 뒤, 계기가 발생합니다. `advance: <n>`은 하루의 마감을 건너뛰지 않습니다 —
`dayEnd`를 발생시키려고 매일의 마지막 슬롯까지 걸어갑니다 — 그리고 `advance: day`는 시계가 서 있는 곳에서 그
날을 닫고 나머지를 건너뜁니다. 시계 경로에 대한 `engine:` 쓰기는 셋 중 어느 것도 발생시키지 않습니다. 하루
시계에 이 맵을 주고 — 플러그인은 `dayEnd`와 `dayStart`를 `select: first` 계기로 더 선언합니다 — `dayEnd`에
`day.close`(`Day {{run.day}} closes.`)가, `dayStart`에 `day.open`(`{{clock.weekdayLabel}} begins.`)이
응답하게 하면(둘 다 `once: false`):

```yaml
state:
  run.day:  { type: int, default: 1, owner: engine }
  run.slot: { type: { enum: [morning, afternoon, night] }, default: morning, owner: engine }
clock:
  day: run.day
  slot: run.slot
  slots: [morning, afternoon, night]
  raise: { slot: slotStart, dayStart: dayStart, dayEnd: dayEnd }
  week: { length: 7, first: 0, labels: [Mon, Tue, Wed, Thu, Fri, Sat, Sun] }
```

```yaml
steps:
  - advance: slot
  - advance: 2
  - advance: day
expect:
  transcriptContains: ["Day 1 closes.", "Wed begins."]
```

```
── start ──────────────
  quest fest -> active
── step 1 · advance slot: day 1 (Mon) morning → day 1 (Mon) afternoon ──────────────
  set run.slot = "afternoon"
── step 1 · slotStart (select: sequence) ──────────────
  ✗ routine.morning [scene, priority 5] — when: false
  ✗ routine.night [scene, priority 5] — when: false
  → (no eligible beat — the occasion passes)
── step 2 · advance 2: day 1 (Mon) afternoon → day 2 (Tue) morning ──────────────
  set run.slot = "night"
── step 2 · day 1 (Mon) night · dayEnd ──────────────
  ✓ day.close [scene, priority 0]
  → day.close
@narrator: Day 1 closes.
  set run.day = 2
  set run.slot = "morning"
── step 2 · day 2 (Tue) morning · dayStart ──────────────
  ✓ day.open [scene, priority 0]
  → day.open
@narrator: Tue begins.
── step 2 · slotStart (select: sequence) ──────────────
  ✓ routine.morning [scene, priority 5]
  ✗ routine.night [scene, priority 5] — when: false
  → routine.morning
@narrator: Tue morning. The kettle sings.
  note: passed day 1 (Mon) night without raising `slotStart` (2 beats answer it; an `advance:` raises it only where the clock stops)
── step 3 · advance day: day 2 (Tue) morning → day 3 (Wed) morning ──────────────
── step 3 · day 2 (Tue) morning · dayEnd ──────────────
  ✓ day.close [scene, priority 0]
  → day.close
@narrator: Day 2 closes.
  set run.day = 3
  fest.go failed (by)
  quest fest -> failed (by)
── step 3 · day 3 (Wed) morning · dayStart ──────────────
  ✓ day.open [scene, priority 0]
  → day.open
@narrator: Wed begins.
── step 3 · slotStart (select: sequence) ──────────────
  ✓ routine.morning [scene, priority 5]
  ✗ routine.night [scene, priority 5] — when: false
  → routine.morning
@narrator: Wed morning. The kettle sings.
── end: complete (3 steps) ──────────────
── expect: every expectation held ──────────────
```

- **스텝 2** — 오후에서 시작한 `advance: 2`는 밤에 멈춰 1일째를 닫고(`── step 2 · day 1 (Mon) night · dayEnd`),
  자정을 넘어 2일째를 연 뒤, 끝나는 아침에서 `slotStart`를 한 번 발생시킵니다: 가는 길의 밤 일과는 발생하지
  않으며, 스텝 아래의 메모가 그렇다고 알립니다. 각 이동의 쓰기와 정착은 그 이동이 닿는 멈춤 바로 앞에 출력됩니다.
- **스텝 3** — 아침에서 시작한 `advance: day`는 시계가 서 있는 곳에서 2일째를 닫고, 3일째로의 이동이
  `dayStart` 전의 정착에서 `fest`를 실패시킵니다.
- 자정의 발생도 여느 스텝처럼 콘텐츠를 재생합니다: `transcriptContains` / `transcriptLacks`는 그 줄과
  일치하고, [`--explain`](#파생과---explain)은 거기서 단언된 팩트를 댑니다
  (`asserted by scene `day.close`, step 3`). `lute test --coverage`에서는 거기서 제시된 문서도 커버된 것으로
  셉니다. 스텝 2에서 `expect: { presented: [day.close, day.open, routine.morning] }`는 성립하고,
  `winner: routine.morning`은 마지막 발생을 판정합니다.
- 여러 계기를 발생시키는 전진은 `presented`를 계기별로 나눌 수 있습니다: 스텝 2에서
  `expect: { presented: { dayStart: [day.open], slotStart: [routine.morning] } }`는 그 두 발생만 각각 순서대로
  판정하고 `dayEnd`는 판정하지 않습니다. 불일치는 계기를 댑니다:
  `` expect presented dayEnd: expected [day.open], actual [day.close (dayEnd at day 1 (Mon) night)] ``. 시계가
  발생시키지 않는 키는 제안과 함께 사용법 오류입니다
  (`` `expect.presented` names `dayStrat`, which the clock's `advance:` does not raise — did you mean `dayStart`? (it raises: slotStart, dayStart, dayEnd) ``).
  목록 형태는 여전히 스텝의 모든 비트를 판정합니다. `occasion:` 스텝은 계기를 하나만 발생시키므로 그
  `presented`는 언제나 목록입니다.

시계가 `dayEnd`와 `dayStart`를 스스로 발생시키므로, 그중 하나를 다시 발생시키는 `occasion:` 스텝은 advance가
그 순간을 지나면 그것을 두 번 재생합니다. 스텝은 그래도 재생되며, 노트가 붙습니다:

```
── step 1 · dayEnd ──────────────
  ✓ day.close [scene, priority 0]
  → day.close
@narrator: Day 1 closes.
  note: `dayEnd` is the clock's `raise: { dayEnd: dayEnd }` — an `advance:` raises it at each midnight it crosses, before the clock leaves the day; this step raises it again, so the same day's `dayEnd` runs twice once an `advance:` passes it (drop the step and let `advance:` raise it; dsl 0.24.0 §1)
```

`--json`에서는 스텝의 `notes`에 담깁니다.

실행이 시작되는 위치에서는 어떤 advance도 멈춘 적이 없으므로, 시계는 거기서 `slot` 계기도 `dayStart`도
발생시키지 않습니다. 그 위치에서 둘 중 하나를 발생시키는 `occasion:` 스텝은 엔진만 발생시킬 수 있는 것을
재생하는 것이며, 노트로 그렇게 알립니다: `` note: `dayStart` is the clock's `raise: { dayStart: dayStart }`,
which the clock does not raise at day 1 (Mon) morning, where the run starts — if the engine raises
it when a run starts, declare `raiseAtStart: true` on the clock ``. 시계가 `raiseAtStart: true`를 선언하면
그 스텝은 엔진 자신의 발생을 대신하는 것이므로 노트가 붙지 않습니다. 플레이는 여전히 시작 위치에서
스스로 아무것도 발생시키지 않습니다.

`--json`에서 자정의 멈춤 하나하나는 advance의 `days` 항목이 되며 순서대로입니다: `at`(위치), `occasion`,
`writes`(시계를 그곳으로 옮긴 이동), `settled`(그 이동 뒤의 정착), 그리고 발생한 계기 자신의 필드 —
`select`, `candidates`, `winner`, `presented`, `then`, `quests`(발생 뒤의 전이). advance 자신의 `writes`와
`quests`는 마지막 이동, 즉 멈추는 곳으로의 이동입니다. 스텝 3의 두 번째 멈춤:

```json
{
  "at": "day 3 (Wed) morning",
  "occasion": "dayStart",
  "writes": [ { "kind": "set", "path": "run.day", "value": 3 } ],
  "settled": [
    { "document": "quests/fest.lute", "commands": [
      { "kind": "objective", "quest": "fest", "objective": "go", "failed": true, "failedBy": "by" },
      { "kind": "quest", "quest": "fest", "state": "failed", "failedBy": "by" }
    ] }
  ],
  "select": "first",
  "candidates": [ { "id": "day.open", "kind": "scene", "document": "scenes/day-start.lute", "priority": 0, "eligible": true } ],
  "winner": "day.open",
  "presented": { "id": "day.open", "kind": "scene", "document": "scenes/day-start.lute", "commands": […], "stateDelta": {} },
  "quests": []
}
```

**날을 세는 시계.** `slot:` / `slots:`가 없는(둘은 함께 선언하거나 함께 빠집니다) 시계는 하루 단위로
셉니다: 위치는 `day 3`으로 읽히고, `advance: slot`과 `advance: day`는 모두 하루를 움직이며, `clock.index`는
`day - 1`이고, `once: slot`은 `once: day`처럼 소진됩니다. 걸어서 하는 여행:

```yaml
state:
  run.day: { type: int, default: 1, owner: engine }
  run.leg: { type: int, default: 0, owner: engine }
clock:
  day: run.day
  raise: { slot: morning, dayEnd: dusk }
```

`road.morning`은 `morning`에(`once: day`, `Day {{run.day}}, leg {{run.leg}}.`), `road.camp`는 `dusk`에
(`Camp on day {{run.day}}.`) 응답하고, 퀘스트 `trek`(`start="true"`)에는 목표가 하나 있습니다:
`done="run.leg >= 3" by="clock.index >= 2"` — 3일째 전에 세 번째 구간을 걷는 것.

**같은 순간의 엔진 쓰기.** `advance:` 스텝은 `engine:`을 함께 가질 수 있습니다 — 시계가 움직일 때 엔진이
쓰는 것, 여기서는 일행이 밤새 끝낸 구간입니다. 쓰기는 시계가 도착하는 곳에 적용됩니다: advance가 가는 길에
발생시키는 모든 `dayEnd` / `dayStart` 뒤 — 그래서 그날 저녁의 `dayEnd`는 여전히 자신이 닫는 날을 읽습니다 —
그리고 마지막 정착과 발생 전입니다. 마지막 이동과 쓰기 뒤에 정착이 한 번 따르고, `done`이 `by`보다 먼저
판정되므로, 쓰기가 완료시키는 목표는 이동의 기한이 지나는 바로 그 정착에서 완료됩니다:

```yaml
steps:
  - advance: slot
  - advance: day
    engine: { state: { run.leg: 3 } }
expect:
  quests: { trek: complete }
```

```
── start ──────────────
  quest trek -> active
── step 1 · advance slot: day 1 → day 2 ──────────────
── step 1 · day 1 · dusk ──────────────
  ✓ road.camp [scene, priority 0]
  → road.camp
@narrator: Camp on day 1.
── step 1 · day 2 ──────────────
  set run.day = 2
── step 1 · morning ──────────────
  ✓ road.morning [scene, priority 0]
  → road.morning
@narrator: Day 2, leg 0.
── step 2 · advance day: day 2 → day 3 ──────────────
── step 2 · day 2 · dusk ──────────────
  ✓ road.camp [scene, priority 0]
  → road.camp
@narrator: Camp on day 2.
── step 2 · day 3 ──────────────
  set run.day = 3
  set run.leg = 3
  trek.leg3 done
  quest trek -> complete
── step 2 · morning ──────────────
  ✓ road.morning [scene, priority 0]
  → road.morning
@narrator: Day 3, leg 3.
── end: complete (2 steps) ──────────────
── expect: every expectation held ──────────────
```

시계가 `dayStart`를 선언하지 않았으므로 자정을 넘는 이동은 `morning` 발생 전에 자신의 위치 헤더
— `── step 2 · day 3`, 이동의 쓰기, 엔진의 쓰기, 정착과 함께 — 아래에 출력되고, 아침 줄은 이미 구간 3을
읽습니다. 같은 쓰기를 advance 뒤의 별도 `engine:` 스텝으로 쓰면 정착 하나만큼 늦습니다: 3일째로의 이동이
먼저 퀘스트를 실패시킵니다(`trek.leg3 failed (by)`). `--json`에서 이 쓰기는 advance 자신의 `writes`에서
마지막 이동의 쓰기 뒤에 오고, 그 정착이 `quests`입니다. 쓰기는 `engine:` 스텝의
규칙을 따르지만, `advance:` 옆에서 시계 자신의 `day`나 `slot` 경로를 쓰는 것은 사용법 오류(종료 코드 2)입니다:
`` step 1: `engine:` writes `run.day`, which the `advance:` beside it moves — write the clock in a step of its own, or let `advance:` move it ``.

시계는 앞으로만 움직입니다. `clock.index`를 뒤로 움직이는 `engine:` 스텝은 그 스텝에서 워크를 멈춥니다 —
스텝이 실행될 때 발견되는 사용법 오류, 종료 코드 2 — 그리고 두 위치를 모두 댑니다:

```
── step 2 · engine ──────────────
  set run.slot = "morning"
── halted: step 2: `engine:` moves the clock backward, from day 1 (Mon) afternoon to day 1 (Mon) morning (clock.index 1 → 0) — the clock only moves forward; `advance:` moves it, a `newRun` starts it over ──────────────
```

`newRun`은 run 등급의 나머지와 함께 시계를 초기화합니다. 시계를 선언하지 않은 프로젝트의 `advance:`,
`advance: 0`, 또는 `slot`, `day`, 1 이상의 정수, `{ to: … }` 형태가 아닌 값은 아무것도 재생하기 전에 거부됩니다.

**이름 붙은 순간으로 돌리기**(dsl 0.26.0 §7). `advance: { to: <slot> }`와
`advance: { to: { weekday: <label or number>, slot: <slot> } }`(둘 중 하나, 또는 둘 다)는 시계를 현재 위치
다음의, 그 슬롯과/또는 요일을 가진 위치로 옮깁니다 — 요일은 `week.labels`의 레이블이나 `clock.weekday`
숫자이며, 요일만 주면 그날의 첫 슬롯에 멈춥니다. 앞으로만 움직이고, 제자리에 머무는 일은 없습니다: 밤에 서
있으면 `{ to: night }`는 다음 날 밤으로 갑니다. 모든 advance가 시계를 움직이는 것과 같습니다. 이것은 advance
하나이며 `advance: <n>`처럼 발생시킵니다: `slot` 계기는 시계가 멈춘 곳에서 한 번 발생하고 — 지나친
슬롯에서는 발생하지 않습니다 — 건너는 자정마다 `dayEnd`와 `dayStart`가 발생합니다. 그래서 아침에서
`{ to: night }`는 `slotStart`를 하나 발생시키고, `advance: slot` 스텝 둘은 둘을 발생시킵니다. 사이의 슬롯에
응답하는 콘텐츠가 있으면 `advance: slot` 스텝을 따로 쓰세요. 스텝의 `expect: { clock: { weekday, slot, day } }`
(셋 중 아무거나)는 스텝 뒤에 시계가 어디에 있는지 판정합니다. 위의 하루 시계에서:

```yaml
steps:
  - occasion: visit
  - advance: { to: night }
    expect: { clock: { weekday: Mon, slot: night } }
  - advance: { to: night }
    expect: { clock: { day: 2, slot: night } }
  - advance: { to: { weekday: Fri, slot: morning } }
    expect: { clock: { weekday: Fri, slot: morning, day: 5 } }
```

```
── start ──────────────
  quest fest -> active
── step 1 · visit ──────────────
  ✓ cafe.wren [scene, priority 0]
  → cafe.wren
@narrator: Back again?
── step 2 · advance to night: day 1 (Mon) morning → day 1 (Mon) night ──────────────
  set run.slot = "night"
── step 2 · slotStart (select: sequence) ──────────────
  ✓ routine.night [scene, priority 5]
  ✗ routine.morning [scene, priority 5] — when: false
  → routine.night
@narrator: The lamps go out on day 1.
  note: passed day 1 (Mon) afternoon without raising `slotStart` (2 beats answer it; an `advance:` raises it only where the clock stops)
── step 3 · advance to night: day 1 (Mon) night → day 2 (Tue) night ──────────────
  set run.day = 2
── step 3 · slotStart (select: sequence) ──────────────
  ✓ routine.night [scene, priority 5]
  ✗ routine.morning [scene, priority 5] — when: false
  → routine.night
@narrator: The lamps go out on day 2.
  note: passed day 2 (Tue) morning, afternoon without raising `slotStart` (2 beats answer it; an `advance:` raises it only where the clock stops)
── step 4 · advance to Fri morning: day 2 (Tue) night → day 5 (Fri) morning ──────────────
  set run.day = 3
  set run.slot = "morning"
  fest.go failed (by)
  quest fest -> failed (by)
  set run.day = 5
── step 4 · slotStart (select: sequence) ──────────────
  ✓ routine.morning [scene, priority 5]
  ✗ routine.night [scene, priority 5] — when: false
  → routine.morning
@narrator: Fri morning. The kettle sings.
  note: passed day 3 (Wed) to day 4 (Thu), every slot without raising `slotStart` (2 beats answer it; an `advance:` raises it only where the clock stops)
── end: complete (4 steps) ──────────────
── expect: every expectation held ──────────────
```

스텝 2는 오후를 건너뜁니다: 오후의 `slotStart`는 발생하지 않습니다. 스텝 3은 이미 밤이므로 다음 밤으로
갑니다. 스텝 4는 자정 셋을 건너 금요일 아침으로 갑니다. 이 시계는 `dayEnd` / `dayStart`를 정하지 않았으므로
마지막 `slotStart`만 발생합니다. 그래도 퀘스트는 이동이 지나는 모든 위치에서 정산되므로, `fest`의 기한은
시계가 처음 3일째에 닿는 수요일 아침에 실패합니다. 트랜스크립트는 그 위치까지의 이동, 그 정산, 남은 이동
순서로 출력합니다. 도중에 열리고 닫히는 시즌이나 그곳에서 참이 되는 `rearm=`도 같은 방식으로 그 위치에서
작동합니다. `--json`에서
advance의 `by`는 `"to night"`이나 `"to Fri morning"`입니다. 시계가 선언하지 않은 슬롯이나 요일은 아무것도
재생하기 전의 사용법 오류입니다
(`` step 2: `advance:` to slot `dusk` — the clock's slots are: morning, afternoon, night ``,
`` step 2: `advance:` to weekday `Fry` — a weekday is a number 0..6 or one of: Mon, Tue, Wed, Thu, Fri, Sat, Sun ``).

**시계가 끝날 때**(dsl 0.27.0 §4). `last: { day: 1, slot: h05 }`(또는 `days: N`)를 선언한 시계는 거기서
멈춥니다. 마지막 위치에 정확히 도착하는 `advance:`는 평범한 전진입니다. 그 너머로 가려는 전진은 마지막
위치까지만 가서 마지막 날의 `dayEnd`를 한 번 발생시키고 `slot` 계기는 발생시키지 않으며, 머리글에
`· the clock ends (its last position)`이 붙습니다(`--json`: `advance.ended: true`). 그 스텝의 `engine:`
쓰기는 그 `dayEnd` 뒤에 적용됩니다. 그 뒤의 `advance:` — 또는 `engine:` 스텝이 날을 옮겨 끝을 지난 곳에서
시작하는 전진 — 은 `newRun`이 시계를 다시 시작하기 전까지 `E-CLOCK-END`로 플레이를 멈춥니다(종료 코드 1,
`lute test`에서도 같음):

```
── step 2 · advance slot: day 1 h05 → day 1 h05 · the clock ends (its last position) ──────────────
── step 2 · day 1 h05 · dawn ──────────────
  ✓ ward.dawn [scene, priority 0]
  → ward.dawn
@narrator: Dawn breaks on night 1.
── step 3 · advance slot: day 1 h05 → day 1 h05 ──────────────
── halted: s.play.yaml:4:5: step 3: `advance:` past the clock's last position (day 1 h05) — the clock ended; a `newRun` starts it over (E-CLOCK-END) ──────────────
```

**일과 나눠 쓰기.** `include: <file>`(동작이 `include`인 스텝. 0.27.0부터 `repeat`, `choose`, `bridges`도 함께 쓸 수 있음)은 다른 파일의 스텝을 그 자리에 끼워
넣습니다: 파일은 스텝 목록이거나, 키가 `steps:` 하나뿐인 매핑입니다. 경로는 포함하는 파일을 기준으로
해석되고, include는 중첩될 수 있습니다. 하루치 일과를 파일 하나에 두고 여러 경로가 나눠 씁니다:

```yaml
# plays/routes/day.yaml — one day of routine, shared by every route
- occasion: visit
- advance: slot
- advance: slot
```

```yaml
# plays/tuesday.play.yaml
steps:
  - include: routes/day.yaml
  - label: the next morning
    advance: slot
  - include: routes/day.yaml
```

스텝 번호는 끼워 넣은 뒤에 매겨지므로 이 스크립트는 일곱 스텝으로 재생됩니다 — 레이블은 스텝 4에 붙고
(`── step 4 (the next morning) · advance slot: day 1 (Mon) night → day 2 (Tue) morning`), 기대값 불일치는
끼워 넣은 스텝의 번호를 댑니다. 읽을 수 없는 파일, 모양이 틀린 파일, `label`, `repeat`, `choose`, `bridges` 말고 다른
키와 함께 쓴 `include:`, 그리고 이미 포함되는 중인 파일은 사용법 오류입니다:
`` plays/routes/loop.yaml: `include: ../loop.play.yaml` is a cycle — plays/routes/../loop.play.yaml is already being included ``.

`include:` 항목이 끝날 때 — `repeat:`의 모든 반복을 통틀어 — 그 항목 자신의 `choose:` 키를 어느 스텝도
제시하지 않았거나 `bridges:` 태그의 답을 어느 호출도 가져가지 않았다면, 그 항목의 마지막 스텝 아래에 그
`include:` 줄과 쓰이지 않은 키를 대는 노트가 붙습니다(잘못된 `include:`에 적은 결정). 결정과 답은 그 항목과
함께 버려집니다. 스텝 자신의 `choose:` 키를 그 스텝의 어느 제시도 쓰지 않았을 때도 같은 노트가 붙습니다:

```
  note: `include: routes/day.yaml` at plays/inc.play.yaml:3:5 never used its own `choose: maraAsk` (no step of the include presented `maraAsk`) — a decision or answer for a step outside the include belongs on that step, or on the script
  …
  note: step 3 never used its own `choose: maraAsk` (no presentation of the step presented `maraAsk`) — a decision for a later step belongs on that step, or on the script
```

스크립트가 정하지 않은 선택에서 멈춘 플레이는, 그 결정을 쓰지 않은 채 끝난 `include:`를 댑니다.

`include:`의 `label:`은 끼워 넣는 스텝 중 자기 레이블이 없는 스텝 모두에(중첩된 include의 스텝까지) 레이블을
붙입니다. `label: first day`인 `- include: routes/day.yaml`은 `── step 1 (first day) · townVisit`로 찍히고,
`label: own label`을 쓴 스텝은 그 레이블을 유지합니다.

**인터페이스로서의 steps 파일.** 여러 작가가 한 플레이스루를 나눠 쓸 때 — 각 지역이 `include:`되는 steps
파일을 소유하고, 리드의 스크립트가 그것들을 지도 순서대로 포함할 때 — steps 파일은 자신의 계약을 밝히므로,
앞쪽의 변경은 세 파일 뒤의 빠진 비트가 아니라 그것이 깨뜨린 파일에서 실패합니다. 첫 스텝의
`expect: { clock: … }`는 도착할 때 기대하는 시각을 밝히고(dsl 0.26.0 §7), 마지막 스텝의 `expect:`는
인계입니다: 다음 파일이 기대는 팩트, 상태, 퀘스트 상태, 시계. 플러그인 호출에는 스텝 수준 `bridges:`를,
결정에는 스텝 수준 `choose:`를 주어 그 응답이 다른 파일의 스텝에 소비되지 않게 하세요.
[여러 작가 가이드](/guides/multi-author/)를 보세요. 하루만 앞으로 돌린 경로 뒤에 포함된, 수요일 아침을
기대하는 시장:

```yaml
# plays/steps/market.steps.yaml — arrives Wednesday morning, leaves Wednesday night
steps:
  - occasion: visit
    label: the market opens
    expect: { clock: { weekday: Wed, slot: morning } }
  - advance: { to: night }
    label: market hand-off
    expect: { clock: { weekday: Wed, slot: night } }
```

```
── expect: 2 missed ──────────────
  ✗ step 2 (the market opens) at visit: expect clock weekday: expected Wed, actual Tue (1)
  ✗ step 3 (market hand-off) at slotStart: expect clock weekday: expected Wed, actual Tue (1)
```

### 레이블과 반복

`label: <text>`는 스텝에 이름을 붙입니다: 스텝 헤더에 출력되고(`── step 4 (the engine closes the day) · engine`),
`--json`에 실리며, 그 스텝의 모든 기대값 불일치가 이 이름을 댑니다. `repeat: <n>`(1 이상의 정수)은 스텝을
`n`번 실행합니다 — `── step 7 [1/2] · townVisit`, `── step 7 [2/2] · townVisit` — 그리고 각 반복은 자신의
스텝 레코드이고, 퀘스트 라이프사이클을 따로 정착시키며, `── end: complete (<n> steps)`에 세어집니다.
`repeat`는 엔진의 일상에 어울립니다: 세 번의 런 종료(`engine: { state: { user.runs: { add: 1 } } }`,
`repeat: 3`), 또는 플레이어가 매일 하는 방문.

### 세이브에서 시작하기

최상위 키 네 개가 스텝 1 전에 플레이스루의 이력을 시드하므로, 스크립트는 그 앞을 모두 다시 재생하는
대신 실제 플레이어의 세이브가 있는 지점에서 시작할 수 있습니다:

| 키 | 의미 |
|---|---|
| `visited: [scene ids]` | 이 세이브에서 제시된 씬 — `visited('<id>')`와 `after: visited(…)`가 읽습니다. 방문한 씬은 제시된 것이므로 그 `once: user`(와 `share` 키)가 소진됩니다. 방문이 이전 런의 것일 수 있어 `once: run`은 소진되지 않습니다 — 그러려면 `presented: { run: […] }`에 넣습니다. |
| `presented: { run: [beat ids], user: [beat ids] }` | 이미 제시된 씬 비트: `user` — 이전 런에서, 그래서 `once: user` 비트가 소진됨. `run` — 현재 런에서, 그래서 `once: run`과 `once: user`가 모두 소진됨. 나열된 모든 씬은 방문한 것으로도 셉니다. |
| `quests: { <id>: unset \| active \| complete \| failed }` | 퀘스트 라이프사이클 상태. 시작 정착은 퀘스트를 처음부터 다시 시작하지 않고 이 상태를 이어받습니다. 모든 목표는 미완료로 시작합니다 — 목표 진행은 `state:`로 시드하세요(아래). |
| `entriesRead: { run: [entry ids], user: [entry ids] }` | `run` — 현재 런에서 읽음: `entry.<id>.read`와 `entry.<id>.everRead`. `user` — 이전 런에서 읽음: `entry.<id>.everRead`만. |

프로젝트가 선언하지 않은 id는 did-you-mean이 붙은 사용법 오류이며
(`` `visited:` names `town.welcom`, which is no scene in this project — did you mean `town.welcome`? ``),
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
  ✗ memo [entry, priority 0, read] — once: run — already read this run
  → old
  entry old (first read)
@maud: The same notice as ever.
  notices.looked done
  quest notices -> complete
── end: complete (1 step) ──────────────
```

`veteran`은 완료 상태로 남고 — 시작 시 다시 활성화되지 않습니다 — 세이브의 읽기 기록이 두 엔트리를
소진시키면서 `old`를 엽니다. `memo`에는 `read`가 붙습니다(dsl 0.23.0): 이번 런에 `entry.<id>.read`가
세워진 엔트리 비트는 자격 여부와 상관없이 후보 줄에 그렇게 표시되므로, `select: all` 메뉴는 플레이어가
이미 본 엔트리를 보여 줄 수 있습니다. `notice`는 이전 런에서만 읽혔으므로 표시되지 않습니다.

`quests:`는 상태만 담습니다. 일부 목표를 이미 완료한 세이브는 최상위 `state:` 아래에 목표마다
`quest.<id>.objectives.<oid>.done: true`를 퀘스트 상태와 나란히 시드합니다:

```yaml
quests: { houndHunt: active }
state:
  run.floor: 3
  quest.houndHunt.objectives.collar.done: true
```

시작 정착은 `collar`를 완료된 것으로 세고 다시 완료시키지 않습니다 — `houndHunt.collar done` 줄도 없고
목표 본문도 다시 재생되지 않습니다 — 그리고 완료된 목표의 `by` 기한은 더 이상 적용되지 않으므로 4층을
지나도 아무것도 실패하지 않습니다. `report`는 여전히 열려 있습니다. 값은 bool이어야 하고, 다른 `state:`
시드처럼 퀘스트와 목표가 선언되어 있어야 합니다. 이를 위한 `quests:`의 긴 형태는 없습니다.

### 런 경계

`newRun: true`는 새 런을 시작합니다. 먼저 모든 `run.*` 값을 **`prev.run.*`**(dsl 0.23.0)로 스냅숏하고
— 런이 끝났을 때 각 경로가 가졌던 값 — 첫 줄에서 그렇다고 알린 뒤(`…; prev.run.* holds the ended run (1 value)`)
가져간 값을 하나씩 출력하고(`prev.run.floor = 6`, dsl 0.24.0. `--json`: 스텝의 `prevRun`, 경로 → 값)
다음을 초기화합니다:

- `run.*` 상태를 선언된 기본값으로, 그리고 모든 run 등급 `entry.<id>.read` 플래그를. 그래서 새 런의 첫
  읽기에서 엔트리 효과가 다시 적용되고 `once="run"` 엔트리가 다시 자격을 얻습니다.
- run 등급 팩트를 프로젝트의 시드 팩트로.
- 모든 `<quest tier="run">` 퀘스트를 `unset`으로, 목표도 되돌려서. `start`가 있는 퀘스트는 뒤따르는
  정착에서 다시 활성화되고, 수락으로 시작하는 퀘스트는 새 수락을 기다립니다. `unset`을 벗어났던
  퀘스트는 런이 남긴 상태와 함께 나열되고(`quest climb -> unset (tier: run; was complete)`), 여전히
  `unset`인 퀘스트는 조용히 초기화됩니다. 초기화 시점에 `active`이면서 완료되거나 실패한 목표가 하나도 없는
  **수락 방식** 퀘스트에는 `note:`가 붙습니다(dsl 0.24.0. `--json`: `resetUnjudged`) — 끝난 런이 수락하고는
  끝내 풀지 않은 일로, 런 사이의 허브에서 받은 퀘스트가 흔히 이렇게 보이므로 노트는
  `::accept{… at="nextRun"}`을 가리킵니다([퀘스트 구조](#퀘스트-구조) 참고). `climb` 같은 `start=` 퀘스트에는
  노트가 붙지 않습니다: 스스로 다시 시작하며, 어차피 `at="nextRun"`이 받을 수 없는 퀘스트입니다.
- `once: run` 소진 기록.

`user.*` / `app.*` 상태, user 등급 퀘스트(기본 `tier`), `entry.<id>.everRead`, user·app 등급 팩트,
`visited` 이력, `once: user` 소진 기록은 유지됩니다. 서로 다른 `lute play` 호출 사이의 `once: user`는
모델링되지 않습니다 — 여러 런을 한 스크립트에 넣고 `newRun` 스텝으로 나누거나, 세이브에서 시작하세요.

긴 형태 `newRun: { state: {…}, facts: […] }`는 그다음 쓰기를 새 런의 시드로 적용합니다 — `engine:`
스텝과 같은 `state:`(리터럴 또는 `{ add: n }`)와 `facts:` 규칙이며, `retract:`는 없습니다.
`::accept{… at="nextRun"}`으로 예약된 수락은 이 쓰기 바로 전에 적용됩니다. 그다음 퀘스트 라이프사이클이
정착합니다.

```yaml
steps:
  - label: a run ends
    engine:
      state: { run.floor: 6, user.runs: { add: 1 } }
      facts: [slew(warden)]
  - newRun: { state: { run.floor: 1 } }
  - occasion: townVisit
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
  run.* state, run-tier facts and once: run reset; prev.run.* holds the ended run (1 value)
  prev.run.floor = 6
  quest climb -> unset (tier: run; was complete)
  set run.floor = 1
  quest climb -> active
── step 3 · townVisit ──────────────
  ✓ town.idle [scene, priority 0]
  ✗ town.victory [scene, priority 10] — when: false
  → town.idle
@maud: Quiet night.
── step 4 (a run ends) [1/2] · engine ──────────────
  set user.runs = 2
── step 4 (a run ends) [2/2] · engine ──────────────
  set user.runs = 3
  veteran.three done
  quest veteran -> complete
── end: complete (5 steps) ──────────────
```

`slew`는 run 등급 관계이므로 처치 기록은 새 런까지 살아남지 못하고 `town.victory`는 다시 닫힙니다.
`climb`(`tier="run"`)은 처음부터 다시 시작하지만 `veteran`은 계속 셉니다.

[`prev.run.<path>`](/state/state-model/#the-previous-run)는 읽기 전용이고 런이 한 번 끝나기 전까지
`unset`이므로, 콘텐츠는 가드해야 합니다(`has(prev.run.floor)`). 탑의 `start.recap`이 그렇게 하며,
[계기 조합하기](#계기-조합하기)가 `newRun` 뒤의 모습을 보여 줍니다. 세이브 중간에서 시작하는 스크립트는
다른 경로처럼 시드합니다 — `state: { prev.run.floor: 5 }` — 그러면 첫 `runStart`가 회상을
`Floor 5 last time.`로 재생합니다.

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
  ✗ notice [entry, priority 0, read] — once: user — already read
  → memo
  entry memo (first read)
@maud: "Floor three is flooded again."
── step 3 · board (select: all, pick: old) ──────────────
  ✓ old [entry, priority 0]
  ✗ notice [entry, priority 0, read] — once: user — already read
  ✗ memo [entry, priority 0, read] — once: run — already read this run
  → old
  entry old (first read)
@maud: The same notice as ever.
── step 4 · new run ──────────────
  run.* state, run-tier facts and once: run reset; prev.run.* holds the ended run (1 value)
  prev.run.floor = 0
  quest climb -> unset (tier: run; was active)
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

스텝 4에서 `climb`은 활성 상태이고 목표는 완료되지도 실패하지도 않았습니다. `start=` 퀘스트이므로 초기화는
그것을 나열하고 다시 활성화할 뿐, 노트를 붙이지 않습니다. `prev.run.floor = 0`은 건드리지 않은 층수입니다 — 스냅숏은
쓰였든 아니든 모든 `run.*` 값을 가져갑니다.

### 시즌과 `rearm` 퀘스트

런에 묶이지 않는 초기화가 두 가지 있습니다(dsl 0.27.0 §5). 스키마의 [시즌](/state/schemas/#seasons)은
`live` 조건이 참이 될 때 열리고, 플레이는 그 순간을 원인이 된 스텝 아래에 출력합니다:
`season harvest opens — season.harvest.* reset to defaults; last window: prev.season.harvest.tokens = 2`
(다시 거짓이 되면 `season harvest closes`). 시즌이 열리면 `season.harvest.*`를 `prev.season.harvest.*`로 옮기고
기본값으로 되돌리며, 그 시즌의 `once: season:harvest` 비트와 `tier="season:harvest"` 퀘스트를 다시 처음
상태로 만듭니다(`quest missions -> unset (season:harvest opened; was complete)`). [`rearm=`](/language/quests-and-scenes/#quests-that-come-back-season-tiers-and-rearm)이 있는
퀘스트는 첫 정산 뒤로 조건이 참이 될 때마다 `unset`으로 돌아가며 `quest festival -> unset (rearmed; was complete)`로
출력되고, 성립하는 `start`가 있으면 같은 정산에서 다시 활성화됩니다.
여러 날을 건너는 `advance:` 하나는 지나는 모든 위치에서 둘 다 살핍니다: 도중에 열리고 닫히는 시즌은 그 자리에서
퀘스트를 시작하고 실패시키며, rearm은 조건이 참이 되는 날에 작동하고, 각각 그 위치로의 시계 이동 뒤에 출력됩니다.
[`spentBy`](/language/beats/#until-it-is-solved-spentby) 조건으로 소진된 비트는 조건이 성립하는 동안 ``spentBy: `run.solved` holds``,
한 번 성립한 뒤에는 ``spentBy: `run.solved` held — spent this run``이라는 이유와 함께(조건이 다시 거짓이 되어도 `once` 기간
동안 소진된 채로 남습니다), 소진된 `once: week` 비트는 `once: week — already presented this week`와 함께 나열됩니다.

### 계기 조합하기

`select: first` 계기는 비트 하나로 응답합니다. 두 가지 추가 기능(dsl 0.23.0)은 그 순서를 포기하지 않고도
계기가 더 많은 것을 말하게 합니다.

**`select: sequence`**는 자격 있는 비트를 모두 선택 순서대로 제시합니다. 탑은 런이 시작될 때
`runStart`를 발생시킵니다: `start.gear`(priority 10)는 일과이고, `start.recap`은 지난 런의 층수를
떠올리므로 런이 한 번 끝난 뒤에만 자격이 있습니다:

```yaml
steps:
  - occasion: runStart
  - label: the run ends on floor three
    engine: { state: { run.floor: 3, user.runs: { add: 1 } } }
  - newRun: true
  - occasion: runStart
    expect: { presented: [start.gear, start.recap] }
```

```
── start ──────────────
  quest climb -> active
  quest veteran -> active
  quest notices -> active
── step 1 · runStart (select: sequence) ──────────────
  ✓ start.gear [scene, priority 10]
  ✗ start.recap [scene, priority 0] — when: false
  → start.gear
@maud: Rope, lamp, bread. Up you go.
── step 2 (the run ends on floor three) · engine ──────────────
  set run.floor = 3
  set user.runs = 1
── step 3 · new run ──────────────
  run.* state, run-tier facts and once: run reset; prev.run.* holds the ended run (1 value)
  prev.run.floor = 3
  quest climb -> unset (tier: run; was active)
  quest climb -> active
── step 4 · runStart (select: sequence) ──────────────
  ✓ start.gear [scene, priority 10]
  ✓ start.recap [scene, priority 0]
  → start.gear
  → start.recap
@maud: Rope, lamp, bread. Up you go.
@maud: Floor 3 last time. Beat it.
── end: complete (4 steps) ──────────────
── expect: every expectation held ──────────────
```

- **스텝 1** — 아직 끝난 런이 없으므로 `prev.run.floor`는 unset이고 일과만 재생됩니다.
- **스텝 3** — `newRun`은 `run.floor`를 초기화하기 전에 그 값(3)을 `prev.run.floor`로 스냅숏하고 출력합니다.
  `climb`은 [런 경계](#런-경계)에서처럼 다시 시작합니다.
- **스텝 4** — 두 비트 모두 자격이 있습니다: 각자 `→` 줄을 받고, 차례로 재생되며, 각자 자신의 `once`를
  소진합니다. 스텝의 `presented:` 기대값은 목록 전체를 순서대로 단언합니다.

목록의 비트 하나가 재생된 뒤로는 뒤의 비트마다 제 차례 직전에 다시 판정됩니다: `run.day`를 2로 옮기는 저녁
비트 뒤에서는 같은 발생의 `when: 'run.day >= 2'` 편지가 재생되고, 앞선 비트가 `when`을 거짓으로 만든 비트는
건너뜁니다. 그렇게 판정된 후보 줄 끝에는 `(judged at its turn, after an earlier beat of this raise)`가 붙고,
발생의 `<on event>` 핸들러는 모든 비트 뒤에 실행됩니다. 퀘스트 라이프사이클은 제시 **하나하나** 뒤에
정착하므로, 목표 — 또는 기한 — 는 한 스텝의 두 비트 사이에서 판정됩니다. 고를 것이 없으므로
`sequence` 계기에 `pick:`을 쓰면 사용법 오류입니다.

**`also: true`**는 비트를 `select: first` 계기의 곁들이는 대사로 만듭니다. priority가 얼마든 절대 이기지
않으며, 자격이 있으면 승자 **뒤에** 제시됩니다 — 주 비트가 하나도 자격이 없으면 혼자 제시됩니다.
오스카의 번들 비트 `rumor`가 그렇습니다:

```yaml
steps:
  - engine: { state: { run.floor: 2 } }
  - occasion: talk
    target: npc.oskar
  - label: the hound falls
    engine: { facts: [slew(hound)] }
  - occasion: talk
    target: npc.maud
  - occasion: talk
    target: npc.oskar
expect:
  quests: { houndHunt: complete }
  state: { user.embers: 50 }
```

```
── start ──────────────
  quest climb -> active
  quest veteran -> active
  quest notices -> active
── step 1 · engine ──────────────
  set run.floor = 2
── step 2 · talk → npc.oskar ──────────────
  ✓ oskar.hunt [beat, priority 10]
  ✓ oskar.rumor [beat, priority 0, also]
  → oskar.hunt
  + oskar.rumor (also)
@oskar: The hound took my dog's collar. Bring it back before you pass floor four.
  quest houndHunt accepted
@oskar: They say the warden sleeps on floor six.
  quest houndHunt -> active
── step 3 (the hound falls) · engine ──────────────
  assert slew(hound)
  houndHunt.collar done
── step 4 · talk → npc.maud ──────────────
  ✓ maud.talk [scene, priority 0]
  → maud.talk
@maud: Up again?
── step 5 · talk → npc.oskar ──────────────
  ✓ oskar.rumor [beat, priority 0, also]
  ✗ oskar.hunt [beat, priority 10] — once: run — already presented this run
  → (no eligible main beat)
  + oskar.rumor (also)
@oskar: They say the warden sleeps on floor six.
  houndHunt.report done
  quest houndHunt -> complete
  grant houndHunt EMBERS 50 (credits user.embers = 50)
── end: complete (5 steps) ──────────────
── expect: every expectation held ──────────────
```

- **스텝 2** — `oskar.hunt`가 이깁니다. 자격 있는 `also` 비트는 `+ oskar.rumor (also)`로 표시되고 그
  뒤에 재생됩니다. 둘 다 번들 비트입니다: `→`와 `+` 줄이 이름을 대고, `beat` 레코드는 `--json`과
  `--ir`에만 나타납니다. hunt는 `houndHunt`를 수락하고, 퀘스트는 스텝 뒤의 정착에서 활성화됩니다.
- **스텝 5** — hunt는 소진되었으므로(`once`의 기본값은 `run`) 주 비트는 하나도 자격이 없고 —
  `→ (no eligible main beat)` — 곁들이는 대사는 그래도 재생됩니다. 트랜스크립트의 나머지는 퀘스트입니다:
  [기한과 대상 지정 목표](#기한과-대상-지정-목표)를 보세요.

`also` 비트도 다른 비트처럼 제시될 때 `once`를 소진합니다. `oskar.rumor`는 `once="false"`이므로
반복됩니다. `--json`에서 스텝의 `presented`는 승자 — 승자가 없으면 첫 `also` 비트 — 이고, `then`은 그
뒤에 제시된 비트를 순서대로 나열하며, `also` 비트에는 `"also": true`가 붙습니다. 후보 레코드에도
`"also": true`가 붙습니다:

```json
{
  "step": 2,
  "occasion": "talk",
  "target": "npc.oskar",
  "select": "first",
  "candidates": [
    { "id": "oskar.hunt", "kind": "beat", "document": "lore/oskar.lute", "priority": 10, "eligible": true },
    { "id": "oskar.rumor", "kind": "beat", "document": "lore/oskar.lute", "priority": 0, "eligible": true, "also": true }
  ],
  "winner": "oskar.hunt",
  "presented": { "id": "oskar.hunt", "kind": "beat", "document": "lore/oskar.lute", "commands": […], "stateDelta": {} },
  "then": [
    { "id": "oskar.rumor", "kind": "beat", "document": "lore/oskar.lute", "also": true, "commands": […], "stateDelta": {} }
  ]
}
```

`select: sequence` 스텝도 같은 두 키를 씁니다: `presented`는 목록의 첫 비트이고 `then`은 나머지입니다.

### 기한과 대상 지정 목표

두 목표 속성(dsl 0.23.0)은 목표가 **언제** 판정되는지를 바꿉니다 — 언어 쪽 설명은
[퀘스트와 씬](/language/quests-and-scenes/#deadlines)에 있습니다:

- `by="<condition>"` — 기한. 목표가 완료되지 않은 동안 `by`가 처음 성립하면 목표는 **실패**하고 다시는
  판정되지 않습니다. 실패한 필수 목표는 퀘스트를 실패시킵니다: `failed` 보상, `questFailed` 핸들러, 부모
  퀘스트로의 연쇄. `done`이 `by`보다 먼저 판정되므로 기한이 지나는 순간 완료된 목표는 실패하지
  않습니다. `by`는 모든 정착 — 제시, `engine:` 쓰기, `advance:`, `newRun` 뒤 — 에서, `on=`이 있든 없든 모든
  목표에 대해 판정됩니다(dsl 0.24.0 §2.1): 기한은 순간이므로, 목표의 계기를 한 번도 발생시키지 않는 플레이어도
  기한을 놓칩니다.
- `on=` 옆의 `until="<condition>"` — 장소에 묶인 기한(0.23.1에서 `on=` 목표의 `by`가 따르던 규칙): 목표가
  판정될 때만 — 그 계기가 (`target`이 있으면 그 대상을 위해) 발생했을 때, `done` 바로 뒤에 — 판정되므로,
  발생과 발생 사이에는 기다립니다. `failed (until)`로 출력됩니다.
- `on=` 옆의 `target="<target>"` — 목표는 **그 대상을 위해** 발생한 계기 스텝에서만 판정됩니다. 다른
  대상의 계기 스텝이나 대상이 없는 스텝은 그 목표를 건드리지 않습니다.

`by`는 모든 정착에서, `on=` 목표의 `done`은 그 계기가 발생할 때만 판정되므로, `done`이 함의하는
`by`(`done="run.v == 'fell'" by="run.v != 'undecided'"`)는 참이 되는 정착에서 — `done`이 판정되기도 전에 —
목표를 실패시킵니다. `check`는 그런 `by=`에 `W-DEADLINE-BEFORE-DONE`을 경고하고 같은 조건의 `until=`을
제안합니다. [퀘스트와 씬](/language/quests-and-scenes/)을 보세요.

위의 [`also` 예제](#계기-조합하기)에서 목걸이 목표는 엔진이 처치를 단언한 스텝 3 뒤의 정착에서
완료됩니다. 스텝 4는 `npc.maud`를 위해 `talk`를 발생시키므로 보고 목표(`on="talk" target="npc.oskar"`)는
판정되지 않고, 스텝 5는 `npc.oskar`를 위해 발생시키므로 퀘스트가 완료됩니다. 처치 없이 오르면 기한이
지나갑니다:

```yaml
steps:
  - occasion: talk
    target: npc.oskar
  - label: the climber passes floor four
    engine: { state: { run.floor: 4 } }
    expect: { quests: { houndHunt: failed } }
```

```
── start ──────────────
  quest climb -> active
  quest veteran -> active
  quest notices -> active
── step 1 · talk → npc.oskar ──────────────
  ✓ oskar.hunt [beat, priority 10]
  ✗ oskar.rumor [beat, priority 0, also] — when: false
  → oskar.hunt
@oskar: The hound took my dog's collar. Bring it back before you pass floor four.
  quest houndHunt accepted
  quest houndHunt -> active
── step 2 (the climber passes floor four) · engine ──────────────
  set run.floor = 4
  houndHunt.collar failed (by)
  quest houndHunt -> failed (by)
@oskar: Floor four already? Then it's gone to ground.
── end: complete (2 steps) ──────────────
── expect: every expectation held ──────────────
```

`houndHunt.collar failed (by)`가 기한입니다(`--json`: `"failed": true, "failedBy": "by"`인 `objective`
레코드). 필수 목표가 퀘스트를 실패시키고 — `-> failed (by)`가 그 `failedBy`를 댑니다 — 퀘스트의
`questFailed` 핸들러가 재생됩니다. 스텝 자신의 `expect:`는 그 정착 직후의 상태를 확인합니다. 실패는 읽을 수
있으며(`quest.houndHunt.objectives.collar.failed`, `quest.houndHunt.failedBy`), `tier="run"` 퀘스트라면
`newRun`이 지웁니다. 퀘스트가 실패하는 다른 세 가지 이유 — 자신의 `fail`, 실패한 부모, 형제 대안 — 도 같은
방식으로 이름이 붙습니다. [퀘스트 구조](#퀘스트-구조)를 보세요.

**상태에 적립되는 보상.** 탑의 `EMBERS` 보상 종류는
[`credits: user.embers`](/plugins/manifests/#rewards-that-credit-state)를 선언하므로, 위 스텝 5가 퀘스트
보상을 지급할 때 스칼라 수량이 그 경로에 더해집니다 — `grant houndHunt EMBERS 50 (credits user.embers = 50)`,
그리고 플레이 끝의 `state: { user.embers: 50 }`이 성립합니다. 정수는 소수점 없이 출력됩니다. `--json`에서
`grant` 레코드는 `"credited": { "path": "user.embers", "value": 50 }`을 가집니다. 범위 수량은 엔진의 굴림이라 여기서는
적립되지 않으며, 퀘스트 자신의 `<on>`이나 목표 본문에서 같은 경로를 `::set`하면 두 번 지급됩니다
(`W-REWARD-DOUBLE-CREDIT`).

### 퀘스트 구조

퀘스트와 계기의 속성 네 가지(dsl 0.24.0 §2)가 라이프사이클이 움직이는 방식을 바꾸며, 트랜스크립트는 그
하나하나를 보여 줍니다 — 언어 쪽 설명은 [퀘스트와 씬](/language/quests-and-scenes/)에 있습니다:

- 하위 퀘스트의 `<quest activate="accept">` — 부모와 함께 활성화되지 않습니다: `::accept`를 기다리고, 부모가
  `active`인 동안에만 활성화됩니다. 부모가 활성이 아닐 때의 수락은 아무 효과 없이 소비되며, 트랜스크립트는
  제시 아래에 그렇다고 알립니다 — 수락 방식 부모 `market`보다 먼저 수락된 하위 퀘스트 `haggle`이라면:
  `` note: accept of quest haggle spent — its parent quest market is not active yet; an `activate="accept"` child activates only while its parent is active ``
  (`--json`: 스텝의 `quests` 안의 `acceptSpent` 레코드,
  `{ "kind": "acceptSpent", "quest": "haggle", "parent": "market", "parentStatus": "unset" }`).
- 부모의 `<quest complete="any">` — 필수 목표 중 **아무거나** 하나가 완료되면 완료되고, 아직 `active`인 다른
  하위 퀘스트는 `superseded`로 실패합니다(아무도 수락하지 않은 하위 퀘스트는 `unset`으로 남습니다).
- 계기의 `judge: before` — 계기가 자신의 `on=` 목표를 판정하고 퀘스트를 정착시키는 일을 비트를 정하기
  **전에** 합니다. 그 전이는 스텝 헤더 바로 아래, 후보보다 먼저 출력됩니다(`--json`: 스텝의 `judgedBefore`.
  스텝의 `quests`는 그 뒤의 전이이며 핸들러 본문을 담습니다). 옮겨지는 것은 판정뿐입니다: 발생이 응답하는
  핸들러 — 같은 이름의 `<on event>` 핸들러, 그리고 정착한 퀘스트의 `questComplete` / `questFailed`
  핸들러 — 는 비트 뒤에 실행되므로, 그 내레이션은 씬 뒤에 나옵니다.
- `<on event="E" target="…">` — 핸들러는 계기 `E`가 그 대상을 위해 발생했을 때만 실행됩니다: 일반 `event:`
  스텝, 다른 대상, 라이프사이클 전이에서는 실행되지 않습니다.

모든 실패는 이유를 댑니다: 기한이면 `quest X -> failed (by)`나 `(until)`, 자신의 `fail=`이 성립하면
`(fail)`, 필수 하위 퀘스트가 실패하면 `(subquest)`, 부모가 실패하면 `(cascade)`, 그리고 `(superseded)`. `--json`의 `quest` 레코드에는 같은 `failedBy`가
담기고, 콘텐츠는 이를 `quest.<id>.failedBy`로 읽습니다(퀘스트가 실패하기 전까지 `unset`).

강 건너기가 이것들을 한데 모읍니다. 플러그인은 `chapterEnd: { select: first, judge: before }`를 선언하고,
`road`는 마을에서 각각 받는 두 하위 퀘스트 중 하나로 완료되고, `toll`에는 `questFailed` 핸들러가 있으며,
`purse`는 엔진이 `run.robbed`를 쓰면 실패합니다:

```lute
<quest id="road" title="Cross the river" start="true" complete="any">
  <objective id="words" title="Talk your way over" quest="parley"/>
  <objective id="silver" title="Pay the toll" quest="toll"/>
</quest>

<quest id="parley" title="Parley" activate="accept">
  <objective id="terms" title="Agree terms by nightfall" on="chapterEnd" done="run.talked"/>
</quest>

<quest id="toll" title="Toll" activate="accept">
  <objective id="pay" title="Pay the ferryman" done="run.paid"/>
  <on event="questFailed">
    @maud: The ferryman pockets his rope and goes home.
  </on>
</quest>

<quest id="purse" title="Keep your purse" start="true" fail="run.robbed">
  <objective id="count" title="Count the coin" quest="coin"/>
</quest>

<quest id="coin" title="Count the coin">
  <objective id="counted" title="Counted" done="run.paid"/>
</quest>
```

마을 씬의 branch `offer`는 `toll`(`silver`)이나 `parley`(`words` — `run.talked`도 세움)를 수락하고,
`chapterEnd`에 응답하는 씬은 `toll`이 왜 끝났는지 읽습니다:

```lute
<match subject="quest.toll.failedBy">
  <when is="superseded">
    @maud: You talked your way over. Keep your silver.
  </when>
  <otherwise>
    @maud: Night falls on the river.
  </otherwise>
</match>
```

```yaml
choose: { offer: [silver, words] }
steps:
  - occasion: townVisit
  - occasion: townVisit
  - label: robbers on the road
    engine: { state: { run.robbed: true } }
  - occasion: chapterEnd
expect:
  quests: { road: complete, toll: failed, purse: failed, coin: failed }
```

```
── start ──────────────
  quest road -> active
  quest purse -> active
  quest coin -> active
── step 1 · townVisit ──────────────
  ✓ town.offer [scene, priority 0]
  → town.offer
@maud: Two ways across.
▷ choice offer: words [silver]        ← chosen: silver
@maud: Pay, then.
  quest toll accepted
  quest toll -> active
── step 2 · townVisit ──────────────
  ✓ town.offer [scene, priority 0]
  → town.offer
@maud: Two ways across.
▷ choice offer: [words] silver        ← chosen: words
@maud: Talk, then.
  set run.talked = true
  quest parley accepted
  quest parley -> active
── step 3 (robbers on the road) · engine ──────────────
  set run.robbed = true
  quest purse -> failed (fail)
  quest coin -> failed (cascade)
── step 4 · chapterEnd ──────────────
  parley.terms done
  quest parley -> complete
  road.words done
  quest road -> complete
  quest toll -> failed (superseded)
  ✓ chapter.nightfall [scene, priority 0]
  → chapter.nightfall
  match -> arm 1
@maud: You talked your way over. Keep your silver.
@maud: The ferryman pockets his rope and goes home.
── end: complete (4 steps) ──────────────
── expect: every expectation held ──────────────
```

- **시작** — `coin`에는 `activate="accept"`가 없으므로 `purse`와 함께 활성화됩니다. `parley`와 `toll`은
  기다립니다.
- **스텝 1–2** — `road`가 활성이므로, 수락할 때마다 그 하위 퀘스트가 씬 뒤의 정착에서 활성화됩니다.
- **스텝 3** — 엔진의 쓰기로 `purse`의 `fail`이 성립합니다: `(fail)`. 아직 활성인 하위 퀘스트 `coin`도 함께
  실패합니다: `(cascade)`.
- **스텝 4** — `chapterEnd`는 `judge: before`이므로 `parley.terms`(`on="chapterEnd"`)가 먼저 판정됩니다:
  `road`가 그것으로 완료되고, 수락되어 아직 활성인 `toll`은 `superseded`가 됩니다 — 모두 후보 목록보다
  먼저이므로, 밤 씬의 `<match>`는 이미 `superseded`를 읽습니다. `toll`의 `questFailed` 핸들러는 여전히 씬
  뒤에 재생됩니다(`--json`: `judgedBefore`가 아니라 스텝의 `quests` 아래). 기본(`after`) 계기였다면 퀘스트
  줄들이 씬 뒤에 나왔을 것이고, `<match>`는 `unset`을 읽었을 것입니다.

**다음 런을 위해 퀘스트 받기.** `::accept{quest="…" at="nextRun"}`은 수락을 예약합니다: 수락은 다음
`newRun` 초기화 직후에 적용되므로, 런 사이의 허브에서 받은 run 등급 퀘스트가 초기화에서 살아남습니다.
탑에서 모드의 씬에 `::accept{quest="relic" at="nextRun"}`을 주고, `relic`은 목표가 `done="run.floor >= 6"`인
수락 방식 `tier="run"` 퀘스트라고 합시다:

```yaml
steps:
  - occasion: talk
    target: npc.maud
  - newRun: true
  - engine: { state: { run.floor: 6 } }
expect:
  quests: { relic: complete }
```

```
── start ──────────────
  quest climb -> active
  quest veteran -> active
  quest notices -> active
── step 1 · talk → npc.maud ──────────────
  ✓ maud.talk [scene, priority 0]
  → maud.talk
@maud: Up again? Next time, look for the warden's key.
  quest relic accepted (queued: applies after the next newRun)
── step 2 · new run ──────────────
  run.* state, run-tier facts and once: run reset; prev.run.* holds the ended run (1 value)
  prev.run.floor = 0
  quest climb -> unset (tier: run; was active)
  quest relic accepted (queued at="nextRun")
  quest relic -> active
  quest climb -> active
── step 3 · engine ──────────────
  set run.floor = 6
  relic.key done
  quest relic -> complete
  climb.high done
  quest climb -> complete
── end: complete (3 steps) ──────────────
── expect: every expectation held ──────────────
```

수락은 실행된 자리에서 예약된 채로 출력되고, `newRun`이 초기화 뒤에 그것을 적용합니다 —
`quest relic accepted (queued at="nextRun")`, `--json`에서는 스텝의 `accepted` — 그리고 `relic`은 새 런의
첫 정착에서 활성화됩니다. `--json`에서 씬의 `accept` 레코드에는 `"at": "nextRun"`이 붙습니다. 다른 `at`은
`E-ACCEPT-TARGET`입니다.

`at="nextRun"`이 없으면 수락은 끝나 가는 런에서 `relic`을 활성화하고, 초기화가 그것을 가져갑니다 — 수락
방식 퀘스트가 판정 없이 활성인 채로 초기화되었으므로 [런 경계](#런-경계)의 `note:`가 붙습니다:

```
── step 2 · new run ──────────────
  run.* state, run-tier facts and once: run reset; prev.run.* holds the ended run (1 value)
  prev.run.floor = 0
  quest climb -> unset (tier: run; was active)
  quest relic -> unset (tier: run; was active)
  note: quest relic was accepted this run and is still active with no objective done or failed — the reset discards it; a run-tier quest taken between runs is `::accept{quest="relic" at="nextRun"}`
  quest climb -> active
```

그러면 `relic`은 새 수락을 기다리고, 같은 스크립트는 `unset`으로 끝납니다.

### 브리지 호출에 답하기

호스트 서비스를 부르는 플러그인 지시문 — 기술 판정, 미니게임 — 은 `bridgeResult` 효과를 통해 서비스의 응답을
`scene.*` 결과 슬롯에 씁니다([매니페스트](/plugins/manifests/) 참고). 참조 플레이어는 서비스를 부르지
않으므로 스크립트가 직접 호출에 답합니다(dsl 0.24.0 §5): `bridges: { <tag>: [ {<field>: value}, … ] }`이며,
`<tag>`는 지시문의 이름이고 목록의 항목 하나가 그 태그의 호출 하나에 호출 순서대로 답합니다.

예제는 [예제](#예제)의 마을에 기술 판정을 더합니다: 플러그인은 `::check{skill dc resultKey}`를 선언하고, 그
효과는 `dice` 서비스의 `passed`와 `margin`으로 `scene.check.<key>.passed`와 `.margin`을 씁니다. `townVisit`에
priority 50으로 응답하는 씬 `gate.guards`는 판정을 두 번 하며, 각 판정 뒤에 결과에 대한 `<match>`가 옵니다.
가드 달린 줄 하나가 첫 판정의 margin도 읽습니다:

```lute
::check{skill="persuasion" dc="12" resultKey="guards"}
<match subject="scene.check.guards.passed">
  <when is="true">
    @narrator: The guards wave you through.
    @narrator{when="scene.check.guards.margin > 5"}: They barely look up.
  </when>
  <when is="false">
    @narrator: The guards bar the way.
  </when>
</match>
::check{skill="stealth" dc="10" resultKey="sneak"}
<match subject="scene.check.sneak.passed">
  <when is="true">
    @narrator: Nobody sees you slip past the stalls.
  </when>
  <when is="false">
    @narrator: A stallholder shouts after you.
  </when>
</match>
```

최상위 응답은 플레이 전체에 걸쳐 호출 순서대로 소비됩니다:

```yaml
bridges:
  check:
    - { passed: true, margin: 3 }
    - { passed: false, margin: -2 }
steps:
  - occasion: townVisit
```

```
── step 1 · townVisit ──────────────
  ✓ gate.guards [scene, priority 50]
  ✓ town.welcome [scene, priority 10]
  ✗ town.morning [scene, priority 0] — when: false
  → gate.guards
::check{skill="persuasion" dc=12 resultKey="guards"}        (bridge answered: passed=true, margin=3)
  set scene.check.guards.passed = true  (effect of ::check)
  set scene.check.guards.margin = 3  (effect of ::check)
  match -> arm 1
@narrator: The guards wave you through.
  skip @narrator "They barely look up." — when: false
::check{skill="stealth" dc=10 resultKey="sneak"}        (bridge answered: passed=false, margin=-2)
  set scene.check.sneak.passed = false  (effect of ::check)
  set scene.check.sneak.margin = -2  (effect of ::check)
  match -> arm 2
@narrator: A stallholder shouts after you.
── end: complete (1 step) ──────────────
```

- **콘텐츠가 읽는 필드, 로드 시점 검사.** 각 응답은 그 태그의 어느 호출을 통해서든 콘텐츠가 읽는 `bridgeResult`
  필드를 주며, 값은 각 결과 슬롯의 선언된 타입에 맞는 리터럴입니다. dsl 0.25.0 §7부터 어떤 콘텐츠도 읽지 않는
  필드는 빼도 됩니다: 여기서는 가드 달린 줄이 `margin`을 읽으므로 모든 `check` 응답이 그것을 주지만, 그 줄이
  없다면 `- { passed: true }`로 충분하고 아래의 힌트도 `passed`만 요구합니다. 읽지 않는 필드를 주어도 됩니다.
  알 수 없는 태그, 어떤 효과도 읽지 않는 필드, 콘텐츠가 읽는데 빠진 필드, 맞지 않는 값은 아무것도 재생하기
  전의 사용법 오류(종료 코드 2)입니다:
  `` top level: `bridges.check` answer 1 lacks `margin`, which content reads — an answer gives every bridge result `::check` content reads: `{ passed: <bool>, margin: <number> }` ``.
  철자가 틀린 태그에는 did-you-mean이 붙습니다.
- **스텝의 응답이 먼저.** 스텝 자신의 `bridges:`는 최상위 대기열보다 먼저 그 스텝의 호출이 소비합니다.
  스텝이 소비하지 않고 남긴 응답은 스텝을 실패시킵니다(종료 코드 1) — 무언가를 정하려고 쓴 응답이 아무것도
  정하지 않은 것입니다. 이 멈춤은 스크립트에서 그 스텝의 위치를 댑니다: ``── halted: <play>:<line>:<col>: step 1: its `bridges:` answers were not all consumed — no plugin call of the step took `check` {passed: false, margin: 9}``.
- **`scene.*`으로.** 응답은 씬의 결과 슬롯에 들어갑니다 — 스크립트가 `scene.*`을 쓰는 유일한 길입니다.
  결과 슬롯의 `state:` 시드는 아무것도 재생하기 전에 거부됩니다(종료 코드 2:
  `` `state.scene.check.guards.passed` is `scene.*`, which resets at every scene boundary and cannot be written ``).
  그런 시드를 쓰던 0.23.1 스크립트는 대신 `bridges:`로 호출에 답합니다.
- **슬롯의 타입을 따름**(dsl 0.26.0 §3.1). 응답의 타입은 그 효과가 쓰는 결과 슬롯이 정합니다 — 디렉티브의
  `state:` 모양이 선언한 슬롯이며, 호출이 컴포넌트 본문에서 와도 마찬가지입니다: 컴포넌트의 플러그인
  디렉티브는 `::use`마다 호스트에 결과 슬롯을 선언합니다. 그런 슬롯이 없으면 브리지 capability의 `result:`
  모양이 정합니다. 어느 쪽도 타입을 정하지 않는 응답은 호출에서 거부되며, 문자열로 저장되는 일은 없습니다
  (0.26.0 전에는 컴포넌트의 `::battle`이 `won: true`를 `"true"`로 저장해 `<match>`가 다른 갈래를 탔습니다).
  거부는 워크를 멈춥니다(종료 코드 2):

```
── halted: scene `ping.s` (scenes/ping.lute): the `bridges.ping` answer to plugin call `ping`: `ok` lands on `scene.ping.k.ok`, which no state slot of this artifact declares, and no bridge capability declares a `result:` type for it — an untyped answer is refused ──────────────
```

- **응답이 없으면 추측도 없음.** 남은 응답이 없는 호출은 워크를 **그 호출에서** 미완료(종료 코드 3)로
  멈추며, 그 뒤의 것 — `<match>`와 기본 갈래 포함 — 은 걷지 않습니다. 0.24 전에는 기본 갈래를 출력한 뒤에야
  멈췄습니다:

```
  → gate.guards
::check{skill="persuasion" dc=12 resultKey="guards"}        (bridge unanswered: passed, margin)
── halted: scene `gate.guards` (scenes/gate/guards.lute): plugin call `check` reads a bridge result and has no answer — give one with `bridges: { check: [ { passed: <bool>, margin: <number> } ] }` (top level or on the step) ──────────────
```

`--json`에서 호출의 `plugin` 레코드에는 `"answered": [{ "field": "passed", "value": true }, …]`가, 워크가
멈춘 곳에서는 `"unanswered": ["passed", "margin"]`이 담깁니다. [`lute trace`](/tooling/tracing/), `lute test`,
`lute run --mock`도 같은 `bridges:` 키를 받습니다.

### 기대값

스텝은 `expect:`를 가질 수 있고, 그 스텝이 한 일에 대해 판정됩니다. 네 키는 계기의 선택을 판정하므로
계기를 발생시키는 스텝에만 쓸 수 있습니다: `occasion` 스텝, 그리고 [`advance`](#시계-앞으로-돌리기) 스텝입니다.
`advance` 스텝에서 `winner`, `offered`, `notOffered`는 시계가 멈춘 자리의 발생을 판정하고, `presented`는 그
스텝이 제시한 비트를 모두 순서대로 나열합니다:

| 키 | 성립 조건 |
|---|---|
| `winner: <beat id>` | 그 비트가 제시됨. `winner: none` — 아무것도 제시되지 않음(자격 있는 비트가 없거나 `pick: none`) |
| `offered: [beat ids]` | 나열된 모든 비트가 그 스텝에서 자격이 있었음 — 부분집합, 순서 무관 |
| `notOffered: [beat ids]` | 나열된 비트 중 어느 것도 자격이 없었음 |
| `presented: [beat ids]` | 정확히 이 비트들이 이 순서로 제시됨(dsl 0.23.0): [`select: sequence`](#계기-조합하기) 스텝의 목록 전체, 또는 승자와 그 뒤의 `also` 비트. `[]` — 아무것도 제시되지 않음 |
| `presented: { <occasion>: [beat ids] }` | `advance` 스텝에서: 시계가 발생시키는 계기 가운데 이름을 댄 계기마다, 이 스텝의 그 발생들이 정확히 이 비트들을 이 순서로 제시함. 이름을 대지 않은 계기는 판정하지 않음([시계 앞으로 돌리기](#시계-앞으로-돌리기)) |

다섯 키가 더 있어 **스텝이 정착한 직후**의 월드를 판정합니다 — 계기 스텝이라면 제시, 계기의 목표 판정,
그 뒤의 정착이 모두 끝난 다음 — 그리고 어떤 종류의 스텝에든 쓸 수 있습니다(`end` 옆은 안 됨). 각 키는
`clock`을 빼면 아래 최상위에서와 같은 뜻을 그 시점에 대해 가집니다:

| 키 | 성립 조건 |
|---|---|
| `quests: { <id>: <status> }` | 스텝 뒤에 퀘스트가 그 상태임 |
| `state: { <path>: <value> }` | 스텝 뒤 경로의 유효 값이 그 값과 같음. 타입까지 비교 |
| `facts: [atoms]` / `notFacts: [atoms]` | 각 원자가 스텝 뒤에, **파생 이후** 성립함 / 성립하지 않음 |
| `clock: { weekday, slot, day, ended }` | 스텝 뒤에 시계가 그 위치에 있음(dsl 0.26.0 §7): `weekday`는 `week.labels` 레이블이나 `clock.weekday` 숫자, `slot`은 선언된 슬롯, `day`는 날 번호, `ended`는 끝나는 시계가 끝났는지(`clock.ended`, 끝나지 않는 시계에서는 사용 오류) — 아무거나 |

그래서 `engine:` 스텝은 자신의 쓰기가 한 일을 단언할 수 있고 — [기한 예제](#기한과-대상-지정-목표)의
`expect: { quests: { houndHunt: failed } }` — 계기 스텝은 끝까지 기다리지 않고 퀘스트 진행을 확인할 수
있습니다. 그 밖의 스텝의 `winner`, `offered`, `notOffered`, `presented`는 사용법 오류(종료
코드 2)입니다: `` step 2: `expect.winner` applies only to an `occasion` or `advance` step, not `engine` (a `engine` step may expect quests, state, facts, notFacts) ``.

`clock:` 불일치는 키를 대고, 요일 레이블 옆에 그 숫자를 출력합니다(`expect clock weekday: expected Wed, actual Tue (1)`).
시계를 선언하지 않은 프로젝트에서는 모든 `clock:` 키가 불일치입니다(`actual no clock position …`).
`include:`되는 steps 파일이 이 키를 쓰는 방법은 [시계 앞으로 돌리기](#시계-앞으로-돌리기)를 보세요.

스텝 키가 하나 더 있으며 어떤 스텝에든 쓸 수 있습니다: `options: { <branch or hub>: [ids] }`(dsl 0.24.0)는
그 스텝 동안 그 branch나 hub에서 정확히 그 선택지들이 제시되었을 때 성립합니다 — 집합이며, 어느 제시에서든
자격이 있었던 선택지를 모두 모으고, hub는 방문마다의 선택지를, `advance`는 스텝의 모든 발생을 합칩니다. 스텝이 제시하지 않은 branch나 hub는
불일치입니다(`` expect options offer: expected [words], actual no branch or hub `offer` was presented in this step ``).

최상위 `expect:`는 플레이의 끝을 판정합니다:

| 키 | 성립 조건 |
|---|---|
| `end: complete \| terminal \| incomplete \| error` | 워크가 그렇게 끝남: `complete` — 모든 스텝이 재생됨; `terminal` — 끝난 자리에서 프로젝트의 [`terminal:`](#게임이-끝났을-때)이 성립; `incomplete` — 정해지지 않은 선택이나 `when`에서 멈춤; `error` — 오류로 멈춤. 옛 이름 `exit:`는 그렇다고 알려 줍니다 |
| `quests: { <id>: <status> }` | 퀘스트가 그 상태로 끝남(아무것도 활성화하지 않은 퀘스트는 `unset`) |
| `state: { <path>: <value> }` | 경로의 최종 **유효** 값 — 마지막 쓰기, 없으면 시드, 없으면 선언된 기본값 — 이 그 값과 같음. 타입까지 비교(`1`은 `"1"`이 아님) |
| `facts: [atoms]` / `notFacts: [atoms]` | 각 원자가 끝에서, **파생 이후** 성립함 / 성립하지 않음 |
| `transcriptContains: [text]` / `transcriptLacks: [text]` | 각 텍스트가 재생된 콘텐츠 줄의 부분 문자열임 / 아님. 콘텐츠 줄은 한 가지 형태 `@speaker: text`로만 비교합니다(dsl 0.24.0). 속성 블록이 없는 바늘은 줄의 전달 속성과 상관없이 맞으므로 `"@mara: Any luck with the lamp?"`는 `@mara{emotion="shy"}: Any luck with the lamp?`로 출력된 줄과 일치하고, 블록이 있는 바늘 줄은 그 속성을 가진 줄과만 맞습니다(아래 참고). 스텝 헤더, 후보, 연출, 노트, `skip` 줄은 결코 일치하지 않습니다 — 재생되지 않은 가드된 줄은 어떤 `transcriptContains`도 만족시키지 않습니다. 보고 줄은 제 키로 단언합니다: `quest wire -> failed`는 `quests: { wire: failed }`로, 놓친 기한은 `state: { quest.wire.objectives.sent.failed: true }`로, `set` 줄은 `state:`로. 보고 줄 모양의 바늘은 결코 맞을 수 없으므로 사용법 오류입니다(`` `expect.transcriptContains` needle "quest lampOut -> active" is the shape of a line the engine's report prints, not of a content line — needles judge only what is said, so this one can never match; assert the quest with `quests: { <id>: <state> }` ``). `lute test`도 같은 형태로 비교하며, `--ir`이 무엇을 출력하든 같습니다 |

`repeat:` 스텝의 기대값은 반복마다 판정되며, 워크가 도달하지 못한 스텝의 기대값은 그 자체로 불일치입니다.
각 `expect:`는 아무것도 재생하기 전에 검증됩니다 — 알 수 없는 키는 합법 키 목록과 did-you-mean을 붙인
사용법 오류(종료 코드 2)이며, 그 키가 다른 수준에 속하면 그렇다고 알려 줍니다
(`` unknown top-level `expect:` key `winner` (`winner` belongs in a step `expect:`) ``).

트랜스크립트 뒤에, 기대값이 있는 스크립트는 `── expect: every expectation held` 또는
`── expect: <n> missed`를 출력하고, 불일치마다 스텝, 레이블, 계기, 반복 번호, 실제 값을 댄 줄을 하나씩
출력합니다. `state` 불일치는 문자열을 양쪽 모두 따옴표로 감싸므로 `"3"`과 `3`이 구별됩니다:

```
── expect: 3 missed ──────────────
  ✗ step 3 (ask about the oil) at talk npc.tomas: expect winner: expected tomasOil, actual tomasBusy
  ✗ end of play: expect quests lampOut: expected active, actual unset
  ✗ end of play: expect state user.bond.mara: expected 1, actual 0
```

`presented:` 불일치는 두 목록을 모두 출력하므로 순서 실수가 한눈에 보입니다:

```
── expect: 1 missed ──────────────
  ✗ step 1 at runStart: expect presented: expected [start.recap, start.gear], actual [start.gear, start.recap]
```

dsl 0.27.0부터 바늘의 속성 블록은 버리지 않고 판정합니다(0.26.0은 버렸습니다): `@speaker{…}: text`로 쓴 바늘
줄은 블록이 적은 속성을 모두 같은 값으로 가진 제시된 줄(속성이 더 있어도 됨)에서 시작해야 합니다.
그래서 트랜스크립트에서 복사한 줄 — `"@mara{emotion=\"content\"}: You're new."` — 은 복사해 온 그 줄과 맞지만,
같은 말을 다른 감정으로 또는 감정 없이 한 줄과는 맞지 않습니다. `transcriptLacks`에서는 그만큼 조건이
좁아집니다: `'@ren{emotion="sad"}: X'`는 X를 *슬프게* 말한 적이 없으면 성립하며, 평범하게 말했어도
마찬가지입니다. "X를 결코 말하지 않음"이라는 뜻이면 `'@ren: X'`로 쓰세요. 블록은 무엇이든 재생하기 전에,
`transcriptContains`와 `transcriptLacks` 모두에서 검사됩니다: 트랜스크립트 줄이 보여 주지 않는 키(`emotoin=`,
`when=`, `code=`), 도메인 밖의 값(`emotion="sadd"`), 정수가 아닌 `variant`, 값을 단 전달 플래그는 did-you-mean과
함께 사용법 오류(종료 코드 2)입니다 — 그래서 철자가 틀린 블록이 아무 줄과도 맞지 않아 `transcriptLacks`를
성립시키는 일은 더 이상 없습니다. `transcriptContains` 불일치는
바늘에 가장 가까운 제시된 줄을 말해진 그대로 댑니다:

```
── expect: 1 missed ──────────────
  ✗ end of play: expect transcriptContains: expected "@mara: Tomas keeps the oil. Ask her." present, actual absent (nearest line: "@mara{emotion=\"delighted\"}: Would you? Tomas keeps the oil. Ask him.")
```

가장 가까운 줄은 어떤 제시된 줄에도 들어 있지 않은 바늘의 첫 줄을 두고 고릅니다(round-5 T3-16): 먼저 바늘의
화자의 줄, 그다음 바늘의 다른 줄이 말해진 스텝의 줄, 그다음 바늘이 그 안에 나타나려면 고쳐야 할 글자가 가장
적은 줄 — 긴 줄의 나머지는 공짜이므로, 짧은 줄은 다를 글자가 적다는 이유만으로 긴 바늘에 "가깝지" 않습니다.

불일치가 있으면 `lute play`는 종료 코드 1로 끝납니다. 단, 워크 자체가 이미 오류(종료 코드 1)나 잘못된
산출물로 인한 러너 실패(종료 코드 2)로 끝났다면 그 코드를 따릅니다. 모든 기대값이 성립했어도 미완료(3)로
멈춘 워크는 여전히 3으로 끝납니다.

`lute test`는 디렉터리 아래에서 `expect:`를 — 스텝에든 최상위에든 — 가진 모든 `*.play.yaml`을
`*.test.yaml` 시나리오 테스트와 나란히, `--project`에 대해 또는 없으면 플레이 위쪽의 가장 가까운
`lute.project.yaml`에 대해 실행하고, 각각 `PASS` / `FAIL` 줄을 출력합니다(`--json`: `"kind": "play"`와
`misses`를 가진 항목). `expect:`가 없는 플레이는 테스트가 아니므로 건너뜁니다. 멈춘 플레이는 최상위
`expect:`가 종료를 선언하지 않는 한(`expect: { end: incomplete }`) 실패합니다. `--coverage`에서는 플레이가
— `occasion:` 스텝으로든 `advance:`가 발생시킨 계기로든 — 제시한 모든 문서와, 라이프사이클을 움직인 모든
퀘스트 문서가 커버된 것으로 셉니다. 플레이는 제시된 문서에만 반영되며, branch/hub와 갈래 행은 추적한
경로에서만 나옵니다. dsl 0.26.0부터 `lute test`는 플레이 프로젝트를 모든 플레이에 대해 한 번만 컴파일하고 —
컴파일 진단도 플레이마다가 아니라 한 번 출력됩니다 — 플레이를 모든 논리 코어에서 병렬로 실행하며
(`RAYON_NUM_THREADS`를 따름) 파일 순서대로 보고합니다.
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
── step 1 · townVisit ──────────────
  ✓ town.idle [scene, priority 0]
  ? town.victory [scene, priority 10] — when: unknown (`!holds('threat', ['warden'])` evaluates unknown: fact `threat(warden)` is undetermined)
── halted: step 1: the `when` of scene `town.victory` (scenes/town-victory.lute) decides the townVisit outcome but `!holds('threat', ['warden'])` evaluates unknown: fact `threat(warden)` is undetermined ──────────────
```

`--explain <atom>`(반복 가능)은 플레이가 끝난 뒤 그라운드 원자가 끝에서 왜 성립하는지, 또는 왜 성립하지
않는지 출력합니다. 성립하면: 사용된 규칙과 각 전제의 근거 — `seed fact`, 플레이 중 `asserted`(누가 단언했는지 아래 참고), 또는 다시
파생된 것이면 그 아래 들여쓰기로 — 가 나오고, 부정 전제는 `(absent)`로 표시됩니다. 성립하지 않으면: 그것을
결론지을 수 있는 모든 규칙이 전제 표시와 함께 나옵니다 — 없는 기반 팩트는 `✗ <atom>  (absent)`, 없는 파생
팩트는 `✗ <atom>  (not derived)`(그 자체도 설명됨), 존재하는 부정 전제는 `✗ not <atom>  (but it holds: …)`,
비교나 가드는 `✗ <test>  (false)` / `? <test>  (undecided)`, 첫 실패 뒤의 전제는
`· <premise>  (not reached)`. `plays/night.play.yaml`이 `townVisit` 스텝 하나일 때:

```console
$ lute play tower --script tower/plays/night.play.yaml --explain "threat(warden)" --explain "threat(hound)"
```

```
── start ──────────────
  quest climb -> active
  quest veteran -> active
  quest notices -> active
── step 1 · townVisit ──────────────
  ✓ town.idle [scene, priority 0]
  ✗ town.victory [scene, priority 10] — when: false
  → town.idle
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

단언된 팩트는 누가 언제 단언했는지 댑니다(dsl 0.24.0). `plays/kill.play.yaml` — `townVisit` 하나, 그다음
`engine: { state: { run.floor: 6 }, facts: [slew(warden)] }`를 가진 스텝 `label: the warden falls on floor six` —
뒤에는 같은 원자가 이렇게 읽힙니다:

```
explain threat(warden): does not hold
  threat(F) :- boss(F), not slew(F)
  ├─ boss(warden)  (seed fact)
  └─ ✗ not slew(warden)  (but it holds: asserted by engine step 2)
```

콘텐츠의 단언도 같은 방식으로 읽힙니다 — [예제](#예제)의 첫날 뒤에는
`knows(lamp)  (asserted by entry `tomasOil`, step 3)` — 그리고 `--json`은 이를 증명의 `assertedBy`에 담습니다.
원자가 없어서 성립하는 부정 전제는, 그 원자를 결론지을 수 있는 규칙이 있으면 한 단계 펼쳐집니다: 그런 규칙이
전제와 함께 나열되므로 발화를 막는 전제가 표시됩니다(JSON: 전제의 `attempts`). 탑에 규칙
`safe(F) :- boss(F), not threat(F)`를 더하면, 같은 플레이가 `safe(warden)`을 이렇게 설명합니다:

```
explain safe(warden): holds
  safe(warden)  ⇐ safe(F) :- boss(F), not threat(F)
  ├─ boss(warden)  (seed fact)
  └─ not threat(warden)  (absent — no rule concludes it:)
     threat(F) :- boss(F), not slew(F)
     ├─ boss(warden)  (seed fact)
     └─ ✗ not slew(warden)  (but it holds: asserted by engine step 2)
```

설명은 `── expect:` 블록 앞에 출력되며, `--json`은 이를 `explain`에 담습니다. 그라운드가 아닌
원자(`knows(X)`, `slew(_)`)는 사용법 오류(종료 코드 2)입니다. `--explain`은 `--no-derive`에서도 최종 팩트와
상태에 대해 규칙을 평가합니다.

### 배타 관계

[`excludes:`](/state/facts-and-datalog/#exclusive-relations-excludes)로 선언한 관계(dsl 0.25.0 §1)는 같은 인자에서
결코 함께 성립하지 않습니다. `check-project`가 증명할 수 있는 것은 증명하고, 둘 다 *가능하기만* 한 곳 — 한 씬의
갈래가 `panicked(maren)`을 assert하고 뒤의 씬이 `calm(maren)`을 assert하는 경우 — 에서는 플레이가 모든 쓰기
뒤에 파생된 것까지 포함한 실시간 팩트를 검사합니다. 둘 다 성립하면 그 쓰기 바로 아래에 위반을 출력하고 거기서
멈춥니다(종료 코드 1). 씬의 뒤쪽 쓰기가 되돌리더라도 마찬가지입니다:

```
── step 2 · morning ──────────────
  ✓ dawn [scene, priority 0]
  → dawn
▷ choice look: saw [nothing]        ← chosen: nothing
@narrator: Nobody answers.
  skip @narrator "Elias walks on." — when: false
  assert calm(maren)
  ✗ exclusive: calm(maren) and panicked(maren) both hold
── halted: scene `dawn` (scenes/dawn.lute): exclusive relations hold together — calm(maren) and panicked(maren) both hold ──────────────
```

짝은 알파벳 순서로 나옵니다. 선택이 둘을 함께 성립시키지 않는 스크립트는 조용히 계속됩니다. 규칙이 도출하는
팩트도 같은 검사를 받으므로, 다른 관계를 배제하는 파생 관계는 그 규칙을 발화시킨 쓰기에서 잡힙니다. 두 배타
팩트를 함께 성립시키는 `engine:` 스텝은 그 스텝에서 멈추고(``── halted: <play>:<line>:<col>: step <n>: exclusive relations hold together — …``),
스크립트 자신의 `facts:`(프로젝트의 시드와 규칙이 거기서 도출한 것 포함)가 이미 배타를 깨면 스텝 1 전에 멈춥니다:
``── halted: the script's seeded world holds exclusive relations together before step 1 — …; fix the script's `facts:` (or the `excludes:` declaration)``.
[`lute trace`](/tooling/tracing/#exclusive-relations)와 `lute test`는 같은 쓰기에서 워크를 거부합니다.

### 사용법 오류

다음 경우 스크립트는 아무것도 재생하기 전에 거부됩니다 — **사용법 오류, 종료 코드 2**, 스텝 번호를 댐:

- 읽을 수 없거나 잘못된 YAML, 알 수 없는 최상위 키, `steps`가 없거나 비어 있음.
- 스텝이 동작을 하나도 적지 않거나 둘 이상 적음(`engine:`을 함께 가진 `advance`는 하나로 셈), 알 수 없는 키, `occasion`이 아닌 스텝의 `target`,
  `occasion`도 `advance`도 아닌 스텝의 `pick` / `choose`, 그런 스텝의 계기 전용 `expect:` 키(`winner`,
  `offered`, `notOffered`, `presented`), 1 이상의 정수가 아닌 `repeat`.
- `advance`가 `slot`, `day`, 1 이상의 정수, `{ to: <slot> }`, `{ to: { weekday, slot } }`가 아님. `to`가 시계가
  선언하지 않은 슬롯이나, `week.labels` 레이블도 0..6 숫자도 아닌 요일을 가리킴. 프로젝트가 시계를 선언하지
  않음. 시계의 `raise:`가 `slot` 계기를 정하지 않았는데 `pick`을 가지거나, 시계가 아무것도 발생시키지
  않는데 `choose` / 선택 `expect:`를 가짐. 또는 그 `engine:`이 시계 자신의 `day`나
  `slot` 경로를 씀. `include:`가 읽을 수 없거나 모양이 틀린 파일을 가리키거나,
  `repeat`, `choose`, `bridges` 말고 다른 키와 함께 쓰였거나, 순환을 만듦.
- 최상위나 스텝의 `bridges:` 응답이, 브리지 결과를 읽는 효과를 가진 플러그인 호출을 프로젝트가 하지 않는
  태그를 가리키거나, 그런 효과가 읽지 않는 필드를 주거나, 읽는 필드가 빠졌거나, 결과 슬롯에 맞지 않는 값을
  줌. 또는 `end` 스텝이 `bridges`를 가짐.
- `end` 스텝이 `end: true`가 아니거나, `repeat`나 `expect`를 가짐.
- 계기 스텝이 해석된 플러그인 중 아무도 선언하지 않은 계기를 가리킴(어떤 플러그인이 계기를 선언한 경우),
  또는 모양만 검사하는 프로젝트에서 어떤 비트도 응답하지 않고 어떤 `<objective on>`도 판정하지 않는 계기를
  가리킴. 대상이 있는 계기를 `target` 없이, 또는 대상 없는 계기를 `target`과 함께 발생시킴. 계기 도메인
  밖의 대상. `select: first`나 `select: sequence` 계기의 `pick`, 그 계기에 응답하지 않는 비트를 고르는
  `pick`.
- `event:`가 선언된 월드 이벤트를 가리키지 않거나, 퀘스트 라이프사이클 이벤트를 가리킴.
- `engine:`이나 `newRun` 쓰기가 선언되지 않았거나 `scene.*` / `quest.*`인 경로, 선언된 타입에 맞지 않는
  값, `int`가 아닌 경로의 `{ add: … }`, 또는 그라운드가 아니거나 선언되지 않았거나 파생된 관계를
  가리키거나 인자 수가 틀리거나 닫힌 도메인의 멤버가 아닌 팩트를 가짐. `engine: { accept }`가 수락 방식이
  아닌 퀘스트를 가리킴. 또는 아무것도 쓰지 않음.
- `state:` / `facts:` 시드가 같은 검사에 실패하거나, 세이브 시드가 알 수 없는 id나 퀘스트 상태를 가리킴.
- `expect:`에 알 수 없는 키나 잘못된 값이 있음.

메시지는 키와 맞는 값을 알려 줍니다:
`` step 1: `engine.state.quest.climb.state`: quest state is written by the quest lifecycle, not the engine — seed a save's quest status with top-level `quests:` ``.

인자가 둘인 원자를 따옴표 없이 쓰는 것이 흔한 실수입니다: YAML은 `facts: [heard(tavi, regent)]`를 항목 둘로
읽고, 메시지가 그렇다고 알려 줍니다
(`` `facts:` entry `heard(tavi` is not a ground fact `rel(arg, …)` — quote the atom: YAML splits an unquoted `[a(b, c)]` at the comma (write `["a(b, c)"]`) ``).
시드에서도, `engine:`이나 `newRun` 쓰기에서도 같습니다. 시계를 뒤로 움직이는 `engine:` 스텝은 실행될 때에야
발견됩니다: 워크는 거기서 멈추며, 역시 종료 코드 2입니다([시계 앞으로 돌리기](#시계-앞으로-돌리기) 참고).

### 각 스텝이 하는 일

계기 스텝:

1. **후보** — 프로젝트의 비트 목록에서 `on`이 스텝의 계기와 같고, `target`이 없거나 스텝의 `target`과
   같거나, (dsl 0.26.0 §5) 스텝의 대상 멤버가 속한 종류 `K`의 `kind:<K>`인 모든 비트. 종류 비트는 같은
   priority의 다른 후보들 뒤에 놓이고, 제시되는 동안 그 멤버를 `occasion.target`으로 읽습니다.
2. **판정** — 후보는 `once`가 소진되지 않았고(씬이나 번들 비트: `run` — 마지막 `newRun` 이후 제시되지 않음,
   `user` — 이 플레이나 세이브에서 한 번도 제시되지 않음, `day` / `slot` — 시계의 날 / 슬롯이 마지막으로
   바뀐 뒤 제시되지 않음, `false` — 소진되지 않음. 엔트리: `run` — `entry.<id>.read`가 세워지지 않음,
   `user` — `entry.<id>.everRead`가 세워지지 않음, `day` / `slot` — 씬과 같음, `once` 없음 —
   소진되지 않음), `after:`가 성립하고(씬 비트; 제시된 씬의 **실시간** `visited` 집합과 실제 퀘스트
   상태의 `completed` / `active` 집합에 대해 평가), `when`이 성립할 때(참조 러너의 CEL 평가기가 실시간
   상태와 팩트에 대해, Datalog 규칙을 적용해 평가) 자격이 있습니다. `when`이 unknown으로 평가되면 —
   `validAt(…)`, `now()`, 또는 `--no-derive`에서의 파생 원자 — 그 비트를 이름 붙여
   **미완료(종료 코드 3)**로 정지합니다. 단, `select: first` 계기에서 확실히 자격 있는 비트가 그보다
   앞서면 승자를 바꿀 수 없으므로 정지하지 않습니다. 판정은 여기서 한 번 정해집니다: (5)의 제시가 무엇을
   하든 이 스텝에서 어떤 비트가 재생되는지는 바뀌지 않습니다.
3. **순서** — 자격 있는 비트를 priority 내림차순, 그다음 프로젝트 순서로.
4. **선택** — 계기의 `select`는 해석된 플러그인의 `occasions` export에서 옵니다(선언되지 않은 계기는
   `first`). `first`: `also`가 아닌 첫 번째 자격 있는 비트가 이기고, 자격 있는 `also` 비트가 모두 그 뒤를
   따릅니다(다른 비트가 없으면 혼자 재생). 자격 있는 비트가 없으면 계기는 스토리 없이 지나갑니다. `all`: 스텝의 `pick`이 제시되며, 그 순간 자격이 없는 pick은 **오류(종료 코드 1)**입니다.
   `pick: none`은 아무것도 제시하지 않으며, 자격 있는 비트가 없을 때 `pick` 없는 스텝도 마찬가지입니다 —
   자격 있는 목록이 있는데 `pick`이 없으면 워크는 제시된 비트를 대며 **오류(종료 코드 1)**로 멈춥니다.
   `sequence`: 자격 있는 비트 전부를 순서대로.
5. **제시** — 씬 비트는 참조 러너(`lute run`의 평가기)로 실행됩니다: `scene.*`는 씬 자신의 기본값으로
   초기화되고, `run.*` / `user.*` / `app.*` / `quest.*` 상태와 팩트는 이어지며, 스크립트의 `choose:` —
   그 위에 스텝 자신의 `choose:` — 가 branch와 hub를 결정합니다. 스크립트에 없는 결정은
   **미완료(종료 코드 3)**로 정지합니다. 번들 비트도 로어 산출물의 `beat` 레코드부터 같은 방식으로
   실행됩니다. 엔트리 비트는 로어 엔트리 규칙으로 제시됩니다: 효과는 첫 읽기에만
   적용되고, 그 뒤 `entry.<id>.read`와 `entry.<id>.everRead`가 true가 됩니다. `::end`는 그것이 실행된
   제시만 끝냅니다: 퀘스트 진행(6), 스텝의 다른 제시(`also` 비트, `sequence`의 나머지), 계기의 목표
   판정(7)은 그대로 실행되고, 플레이스루는 다음 스텝으로 이어집니다 — 플레이스루를 끝내는 것은
   [`end: true` 스텝](#플레이스루-끝내기)뿐입니다. 씬의 `::accept{quest="<id>"}`는 `quest <id> accepted`를 출력하고, 퀘스트는
   제시 직후의 진행에서 활성화됩니다. 씬이나 번들 비트는 제시가 끝나면 — `after:`와 모든 조건의
   `visited('<id>')`에 대해 — 방문한 것으로 셉니다. 효과가 브리지 결과를 읽는 플러그인 호출은 그 태그의 다음
   [`bridges:`](#브리지-호출에-답하기) 응답 — 스텝 자신의 것 먼저, 그다음 최상위 대기열 — 을 가져가며, 남은
   응답이 없으면 워크는 그 호출에서 **미완료(종료 코드 3)**로 멈춥니다. 한 스텝이 여러 비트를
   제시하면(`sequence`, `also`) (5)와 (6)이 비트마다 차례로 실행됩니다.
6. **퀘스트** — 매 제시 후, 모든 퀘스트 라이프사이클이 `lute run`이 퀘스트 산출물을 진행시키는 것과
   정확히 같게 진행됩니다: 활성화(`start`, 없으면 제시된 씬의 `::accept` — `activate="accept"` 하위 퀘스트는
   부모가 활성인 동안에만, `start` 없는 퀘스트는 스스로 활성화되지 않음), 목표 완료(단조적이며 목표 본문은
   한 번만 재생), 그다음 모든 열린 목표의 `by` 기한(처음 참이 되면 실패, `failed (by)`), 완료 전의 `fail`,
   `<on>` 핸들러, `<reward>` 지급 — 보상
   종류의 `credits:` 경로에 스칼라 수량을 더함. 실패한 퀘스트는 활성인 하위 퀘스트를 실패시키고(`cascade`),
   완료된 `complete="any"` 퀘스트는 활성인 다른 하위 퀘스트를 실패시킵니다(`superseded`). 핸들러나 목표
   본문의 `::end`는 스텝이 아니라 그 퀘스트 문서의 진행을 끝냅니다. 그래서 이후의 `quest.*`에 대한 `when`이나
   `after: completed(…)` / `active(…)`는 실제 진행을 봅니다.
7. **발생** — 이어서 스텝의 계기가 퀘스트에 발생합니다. `judge: before`로 선언된 계기(dsl 0.24.0 §2)에서는
   판정과 그 정착이 (1)보다 먼저 일어나므로 스텝의 비트가 그 전이를 봅니다 — 발생이 응답하는 핸들러
   본문(아래의 같은 이름 `<on event>` 핸들러, 그리고 정착한 퀘스트의 `questComplete` / `questFailed`
   핸들러)은 여전히 제시 뒤에 실행됩니다. 같은 이름의 월드 이벤트가 선언되어 있으면
   모든 **활성** 퀘스트의 `<on event="<occasion>">` 핸들러가 먼저 실행됩니다 — `target=`이 있는 핸들러는
   스텝의 `target`이 같을 때만(dsl 0.24.0 §2) — 그리고 이미 정착한 퀘스트는 그렇다고 알립니다:
   `<on event=storm> of quest climb skipped — quest complete`. 그다음 계기가 모든 활성 퀘스트의
   `<objective on="<occasion>">` 목표를 판정하고(dsl 0.21.0 §7a.2) — `target=`도 가진 목표는 스텝의
   `target`이 같을 때만(dsl 0.23.0) — 각 목표의 `done`, 그다음 `until`을 판정한 뒤 라이프사이클이 다시
   정착하므로, 퀘스트가 정확히 그 스텝에서 완료(또는 실패)할 수 있습니다. `on` 목표는 그 밖의 시점에는
   판정되지 않습니다. 목표만 판정하는 계기도 shape-only 프로젝트에서 합법적인 스텝이며, 제시할 비트가
   없으면 `(no candidates)`를 출력하고 지나간 뒤 판정합니다.

라이프사이클은 스텝 1 전에 한 번(세이브의 퀘스트 상태와 목표 시드가 이미 반영된 채로), 모든 `engine:`
스텝 뒤, 모든 `newRun` 뒤(초기화, 예약된 `at="nextRun"` 수락, 시드, 그다음 정착), 모든 `advance:` 뒤(그
발생 전)에도 정착합니다. `event:` 스텝은 이벤트를 모든 퀘스트 문서에 한 번씩 발생시킨 뒤 정착합니다. `end`
스텝은 플레이스루를 끝내는 것 말고는 아무것도 하지 않습니다.

### 종료 코드

| 코드 | 의미 |
|---|---|
| `0` | 완료 — 모든 스텝이 재생되었거나, `end: true` 스텝이 플레이스루를 끝냈거나, 플레이가 스키마의 [끝난 상태](#게임이-끝났을-때)로 끝났고, 모든 기대값이 성립함. |
| `1` | 오류 — 프로젝트 컴파일 실패(dsl 0.26.0부터 문서들이 한 상태 경로를 두 가지로 선언한 경우, `E-STATE-DECL-CONFLICT`도 포함), 어휘 충돌, 자격이 없는 `pick`, 자격 있는 비트가 있는데 `pick`이 없는 `select: all` 스텝, 메뉴가 제시하지 않는 `choose:` 결정(자격 없는 선택지나 이미 고른 `once` hub 선택지), 스텝 자신의 `bridges:` 응답이 다 소비되지 않음, 함께 성립한 두 [배타 관계](#배타-관계)(dsl 0.25.0), `raisedWhen` 관문이 거짓인 계기를 발생시키는 스텝이나 게임이 끝난 뒤의 `occasion:` / `advance:` 스텝(`E-OCCASION-GATE`, dsl 0.27.0 §4), 끝난 시계를 넘는 `advance:`(`E-CLOCK-END`), 또는 기대값 불일치. |
| `2` | 사용법 또는 I/O — 잘못된 스크립트([사용법 오류](#사용법-오류) 참고), 알 수 없는 계기나 월드 이벤트, 누락되었거나 도메인 밖인 `target`, 잘못된 시드나 `engine:` 쓰기, 수락 방식이 아닌 퀘스트의 `engine: { accept }`, 선언된 슬롯이나 요일을 가리키지 않는 `advance: { to }`, 시계를 뒤로 움직이는 `engine:` 스텝, 어떤 호출에도 맞지 않거나 어떤 결과 슬롯이나 capability `result:`도 타입을 정하지 않는 `bridges:` 응답(dsl 0.26.0), 그라운드가 아닌 `--explain` 원자, 읽을 수 없는 프로젝트, 잘못된 산출물. |
| `3` | 미완료 — 스크립트에 없는 choice나 hub, 바닥난 branch `choose:` 목록, unknown으로 평가되는 `when`이나 퀘스트 목표, 해석되지 않은 `now()` / `validAt()`, 또는 브리지 결과에 `bridges:` 응답이 없는 플러그인 호출. |

## 트랜스크립트

사람이 읽는 트랜스크립트는 모든 스텝을 이름 붙이고, 후보와 그 판정을 나열하고, 제시된 비트가 재생되기
전에 승자를 표시합니다. [플레이 스크립트](#플레이-스크립트)의 둘러보기 스크립트는 이렇게 출력합니다:

```
── start ──────────────
  quest climb -> active
  quest veteran -> active
  quest notices -> active
── step 1 · townVisit ──────────────
  ✓ town.idle [scene, priority 0]
  ✗ town.victory [scene, priority 10] — when: false
  → town.idle
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
  run.* state, run-tier facts and once: run reset; prev.run.* holds the ended run (1 value)
  prev.run.floor = 6
  quest climb -> unset (tier: run; was complete)
  set run.floor = 1
  quest climb -> active
── step 7 [1/2] · townVisit ──────────────
  ✓ town.idle [scene, priority 0]
  ✗ town.victory [scene, priority 10] — when: false
  → town.idle
@maud: Quiet night.
── step 7 [2/2] · townVisit ──────────────
  ✓ town.idle [scene, priority 0]
  ✗ town.victory [scene, priority 10] — when: false
  → town.idle
@maud: Quiet night.
── end: complete (8 steps) ──────────────
── expect: every expectation held ──────────────
```

- 모든 헤더는 텍스트 뒤에 고정된 `──────────────` 선이 붙습니다. `── start`는 스텝 1 전에 일어난 퀘스트
  전이를 담습니다. 각 스텝은 `── step <n>`으로 시작하고, 그 뒤에 `(label)`, 반복이면 `[k/n]`, 그리고 하는
  일이 옵니다: `· <occasion>` — 대상이 있는 스텝에는 `→ <target>`, `select: all` 계기에는
  `(select: all, pick: <id>)`(`pick` 없는 스텝이 자격 있는 비트를 찾지 못하면 `pick: none (nothing offered)`),
  `select: sequence` 계기에는 `(select: sequence)` — 또는 `· engine`, `· new run`, `· event <name>`,
  `· advance <slot | day | n>: <from> → <to>`(dsl 0.26.0부터 `{ to: … }` 형태는 `· advance to night: …`나
  `· advance to Fri morning: …`. 시계의 발생이 같은 스텝 번호 아래에 이어짐: 자정의 멈춤마다
  `── step <n> · <position> · dayEnd` / `· dayStart`, 거기서 아무것도 발생시키지 않는 이동은 맨
  `── step <n> · <position>`, 그다음 `slot` 계기 자신의 헤더),
  `· end (the playthrough ends)`. `end: true` 스텝 때문에 재생되지 않은
  스텝은 `── step <n> (label) · skipped (the playthrough ended)`로 출력됩니다.
- 후보는 자격 있는 것(`✓`)을 선택 순서대로 먼저, 그다음 나머지를 선택 순서대로 나열하며, 각각 종류 —
  `scene`, `entry`, 번들 비트는 `beat` — 와 priority를 표시하고, `also` 비트에는 `also`, 이번 런에 이미
  읽은 엔트리에는 `read`를 덧붙입니다(dsl 0.23.0). 자격 없는 후보(`✗`)에는 이유가 붙습니다:
  `once: run — already presented this run`, `once: user — already presented`,
  `once: day — already presented today`, `once: slot — already presented this slot`,
  `once: run — already read this run`, `once: user — already read`,
  `after: prerequisite not satisfied`, 또는 `when: false`. `when`이 unknown으로 평가된 후보는
  `when: unknown (<detail>)`과 함께 `?`로 표시됩니다. 어떤 비트도 응답하지 않는 계기의 스텝은
  `(no candidates)`를 나열합니다. `judge: before` 계기에서는 그 계기가 판정한 퀘스트 전이가 헤더 바로 아래에
  먼저 옵니다. 로스터 규모에서는(round-5 T3-16) 한 발생에서 `when: false`인 후보가 다섯 개 이상이면 한 줄로
  접힙니다 — ``✗ 8 beats — when: false: a, b, c, … (`lute play --json` lists every candidate)`` — 그리고
  `--quiet`는 `✗` 후보를 모두 뺍니다. `?` 후보는 플레이가 멈추는 이유이므로 그대로 출력됩니다.
- `→ <id>`가 승자를 가리킵니다 — `select: sequence` 스텝에서는 비트마다 `→` 줄 하나 — 그리고
  `+ <id> (also)`가 그 뒤를 잇는 `also` 비트를 가리킵니다. 승자가 없으면
  `→ (no eligible beat — the occasion passes)`, `also` 비트만 재생되면 `→ (no eligible main beat)`, 또는
  `→ (pick: none — the list closes; nothing presented)`로 표시됩니다. 번들 비트의 `beat` 레코드는
  출력되지 않습니다 — `→`나 `+` 줄이 이미 그 비트를 가리킵니다. 목표의 기한 실패는
  `<quest>.<objective> failed (by)`(또는 `failed (until)`), 퀘스트 실패는 이유를 붙인
  `quest <id> -> failed (by)` / `(until)` / `(fail)` / `(subquest)` / `(cascade)` / `(superseded)`(dsl 0.24.0 §2), 상태에
  적립되는 보상 지급은 `grant <quest> <KIND> <amount> (credits <path> = <value>)`로 표시되며, 정수 값은
  소수점 없이 나옵니다(`= 50`).
- 그 뒤에 제시된 비트 자신의 트랜스크립트가 소스처럼 읽히게 이어집니다: 콘텐츠 줄은 `@speaker: text`로,
  전달 방식을 유지하며(`@wren{mono}: …`, `@maud{as="Barkeep"}: …`), 보간은 엔진처럼 렌더링됩니다 —
  `{{user.deaths:ordinal}}`은 `2nd`, `labels:`가 있는 enum에 타입이 매인 경로는 그 멤버의 레이블(dsl 0.24.0 §1, §4),
  `{{occasion.target}}`은 발생한 멤버의 cast `name:`(dsl 0.26.0 §5).
  `when=`이 거짓인 줄은 `skip @maud "You again." — when: false`로, 적용되지 않은 가드된 쓰기는
  `skip set run.aff += 1 — when: false`로 표시됩니다. dsl 0.26.0부터 `when=`이 거짓인 다른 디렉티브도 모두
  같은 식입니다: `skip ::give{item="potion"} — when: false`, `skip assert metWren(wren) — when: false`, 그리고
  확장을 하나도 재생하지 않는 `::use`는 `skip ::use{component="…" …} — when: false`. 연출 디렉티브는 작가가
  쓴 그대로 나옵니다 —
  `::bg{location="parlor"}`, `::actor{character="maud" anchor="left"}`, `::vfx{type=…}`, 그리고
  플러그인 자신의 디렉티브는 그 이름 그대로 — 컴파일러가 주입한 연출(프리로드, 포즈 리셋, `::bg` 자동
  숨김)은 빠집니다. 참조 플레이어가 실행할 수 없는 플러그인 디렉티브에는 `(plugin call, not invoked)`가
  붙고, `bridges:` 응답이 브리지 결과를 정한 호출에는 `(bridge answered: passed=true, margin=3)`이, 워크를
  멈춘 호출에는 `(bridge unanswered: passed, margin)`이 붙습니다. `::end`는 쓴 그대로 뒤에
  `(this presentation ends; the play goes on)`이 붙습니다. 게임을 끝낸 스텝에서는
  `(this presentation ends)`만 붙고, 그 스텝의 note가 뒤따릅니다. 결정은
  `▷ choice <id>: … ← chosen: <id>`(또는 `▷ hub <id>: …`)이며, 메뉴에서 고른 선택지는 `[table]`,
  가드가 거짓인 선택지는 `piano✗`, 이미 고른 `once` 선택지는 `table(spent)`로 표시됩니다. 상태 쓰기는
  `set <path> = <value>`, 씬의 `::accept`는 `quest <id> accepted`(`at="nextRun"`이면
  `quest <id> accepted (queued: applies after the next newRun)`, JSON: `presented.commands`의
  `{"kind": "accept", "quest": "<id>"}` 레코드), 엔트리는 `entry <id> (first read)` — 또는
  `entry <id> (re-read: effects skipped)`와 함께 건너뛴 각 효과에 `(skipped: re-read)` 표시. 퀘스트
  전이가 마지막에 옵니다 — 제시가 일으킨 것, 그다음 스텝의 계기가 판정한 것: `<quest>.<objective> done`,
  `quest <id> -> <state>`, 보상 지급. JSON에서는 둘 다 스텝의 `quests`에 들어갑니다.
- 플러그인 디렉티브가 선언한 `effects:`가 만든 쓰기(dsl 0.27.0 §4)는 여느 쓰기처럼 호출 아래에 출력되고, 그것을
  만든 호출을 댑니다: `set scene.check.sneak.passed = false  (effect of ::check)`,
  `assert holding(brassKey)  (effect of ::give)`, `retract holding(lamp)  (effect of ::give)` — 철회가 먼저,
  그다음 호출의 단언. `--json`에서는 그 레코드에 `"effectOf": "<tag>"`가 붙습니다.
- `--ir`은 대신 낮춰진 레코드를 출력합니다: 각 연출 레코드를 IR 형태의 `::<kind>{…}`로 — 속성은 IR 필드
  이름순이며 `position`, `kind`, `family`는 빠집니다(`::actor{character="mara" anchor="left" action="sway"}`라면
  `::actor{action="sway" anchor="left" character="mara"}`) —, 컴파일러가 주입한 레코드도 포함해
  `(injected: <by>)`를 붙여서(`::bg`가 바뀔 때 무대를 비우는 퇴장이라면
  `::actor{character="mara" exit=true}        (injected: stage-bookkeeping)`), 번들 비트의 `beat` 레코드는
  `beat` 줄로. `--ir`은 출력만 바꿉니다: `transcriptContains` / `transcriptLacks`는 언제나 콘텐츠 줄로
  판정하며, `--json`은 어느 쪽이든 모든 레코드를 담습니다.
- `engine:` 스텝은 쓰기를 나열합니다: `set <path> = <value>`, `assert <atom>`, `retract <atom>` — 팩트가
  아니었으면 `retract <atom> (did not hold)` — 그리고 dsl 0.26.0부터 `accept:`마다 `quest <id> accepted (engine)`,
  이미 활성·완료·실패한 퀘스트라면 아무것도 바꾸지 않는 `note: quest <id> is already active — engine accept ignored`.
- `newRun` 스텝은 `run.* state, run-tier facts and once: run reset`을 출력하고, 끝난 런이 스냅숏할
  `run.*` 값을 남겼으면 `; prev.run.* holds the ended run (<n> values)`를 덧붙이며, 이어서 스냅숏한 값마다
  `prev.run.<path> = <value>`(dsl 0.24.0), `unset`을 벗어났던 run 등급 퀘스트마다
  `quest <id> -> unset (tier: run; was <status>)` — 완료되거나 실패한 목표 없이 활성이었던 수락 방식 퀘스트
  뒤에는 `note:` — 그다음 예약된 수락마다 `quest <id> accepted (queued at="nextRun")`, 그다음 시드의 쓰기를
  출력합니다.
- `event:` 스텝은 실행된 핸들러를 출력하고, 이미 정착한 퀘스트에는
  `<on event=<name>> of quest <id> skipped — quest complete`를 출력합니다. 라이프사이클 전이는 모든 종류의
  스텝 뒤에 따라옵니다.
- `--json`에서 `presented.commands`의 모든 레코드는 문서 안의 자기 `position`을 담습니다. 줄 레코드는
  `role`(`dialogue`, `narration`, `mono`, `os`, `vo` 중 하나), `lineId`, `voiceKey` — 내레이션을 포함해 모든 줄에
  `voiceKey`가 있습니다 — 와 해당하는 경우 `as`, `emotion`을 담고, choice와 hub 레코드는 제시되지 않은
  선택지를 `ineligible`에, 이미 고른 `once` 선택지를 `spent`에 나열합니다.
- 워크는 `── end: complete (<n> steps)` — 모든 반복을 셈 — 로, `end` 스텝 뒤에는
  ``── end: `end: true` at step <n> (<k> later steps skipped)``로, 마지막 스텝 뒤에 게임이 끝나 있으면
  ``── end: terminal — `terminal: <condition>` holds``로(dsl 0.27.0 §4), 중간에 멈추면 `── halted: <message>`로
  끝납니다. 그 뒤에 `--explain` 트리, 그다음 `── expect:` 블록이 옵니다.

플러그인이 없는 모양만 검사하는 프로젝트에서, 부업을 제안하는 마을 씬과 `calm` 목표가 `runEnd`에서
판정되는 퀘스트:

```lute unverified="one file of a multi-file project: the scene answering townVisit and a world schema declaring run.pressure sit beside it"
<quest id="holdLine" title="Hold the line" start="true">
  <objective id="sawShed" title="See the shed" done="visited('haven.shed')"/>
  <objective id="calm" title="Keep it calm" on="runEnd" done="run.pressure < 2"/>
</quest>

<quest id="sideJob" title="Side job">
  <objective id="mind" title="Mind the shed" done="run.pressure < 5"/>
</quest>
```

`steps: [{occasion: townVisit}, {occasion: runEnd}]`와 `choose: { offer: take }` — 본문이
`::accept{quest="sideJob"}`인 선택지 — 로, 플레이스루는 제시 중에 부업을 수락하고, 바로 뒤에 활성화하며,
`runEnd`가 발생해야만 `holdLine`을 완료합니다:

```
── start ──────────────
  quest holdLine -> active
── step 1 · townVisit ──────────────
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
  skipped?: { step: number; label?: string }[]; // the steps an `end: true` step left unplayed
  endReason?: string;                      // "complete (8 steps)", or "`end: true` at step 2 (1 later step skipped)"
  end: "complete" | "terminal" | "incomplete" | "error"; // how the walk ended — the values `expect.end` names; `terminal`: the schema's `terminal:` held
  error?: { message: string };
  expect?: { misses: ExpectMiss[] };       // when the script carries an `expect:`
  explain?: Explanation[];                 // one per `--explain` atom
};

type Step = (OccasionStep | EngineStep | NewRunStep | EventStep | AdvanceStep | EndStep) & {
  step: number;                            // the script step (shared by its repetitions)
  label?: string;
  iteration?: number;                      // 1-based repetition, when `repeat` > 1
  repeat?: number;
  quests: QuestGroup[];                    // transitions this step caused
};

type OccasionStep = {
  occasion: string;
  target?: string;
  select: "first" | "all" | "sequence";
  pick?: string;                           // a beat id, or "none"
  candidates: {
    id: string;
    kind: "scene" | "entry" | "beat";      // "beat": a bundle beat
    document: string;
    priority: number;
    eligible: boolean | null;              // null: the `when` evaluated to unknown
    reason?: string;                       // e.g. "when: false", "when: unknown (<detail>)"
    also?: true;                           // an `also` beat
    read?: true;                           // an entry already read in this run
    for?: string;                          // a `for="kind:<kind>"` beat's member (dsl 0.27.0 §3): one candidate per member
  }[];
  judgedBefore?: QuestGroup[];             // a `judge: before` occasion's quest transitions, made before the candidates were decided
                                           // (the handler bodies it answers follow the beats, in `quests`)
  winner: string | null;
  presented?: Presentation;                // the winner, else the first `also` beat; a sequence's first beat
  then?: Presentation[];                   // the beats presented after it, in order
};

type Presentation = {
  id: string;
  kind: "scene" | "entry" | "beat";
  document: string;
  also?: true;
  commands: RunnerRecord[];                // the records `lute run --json` emits
  stateDelta: Record<string, unknown>;     // path -> value
};

type EngineStep = { engine: WriteRecord[] };
type NewRunStep = {
  newRun: true;
  seed: WriteRecord[];
  resetQuests?: Record<string, string>;    // run-tier quest -> the status it had before the reset
  resetUnjudged?: string[];                // accept-driven run-tier quests reset while active with no objective done or failed (dsl 0.24.0)
  prevRun?: Record<string, unknown>;       // the `prev.run.*` snapshot: path -> value (dsl 0.24.0)
  accepted?: string[];                     // the `::accept{… at="nextRun"}` quests applied at this run start
};
type EventStep = { event: string };
type EndStep = { end: true };
// dsl 0.24.0 §1: the occasion the clock raised where it stopped, when `raise:` names a `slot`
// occasion, is carried as the OccasionStep fields beside `advance`.
type AdvanceStep = {
  advance: {
    by: string;                            // "slot", "day", the slot count, or "to night" / "to Fri morning" (dsl 0.26.0)
    from: string;                          // "day 1 (Mon) afternoon"
    to: string;
    days?: DayStop[];                      // each midnight stop, in order, when `raise:` names `dayEnd` / `dayStart`
    ended?: true;                          // 0.27.0: the advance reached a finite clock's end (its last `dayEnd` is in `days`; no slot raise)
    passed?: {                             // the positions it passed without raising its `raise.slot` occasion (neither start nor stop)
      occasion: string;
      beats: number;                       // how many beats answer that occasion
      at: { day: number; slot?: string }[];
    };
    writes: WriteRecord[];                 // the last move, to where the clock stops, then an `engine:` beside the advance
    quests: QuestGroup[];                  // the settle right after that move
  };
} & Partial<OccasionStep>;
type DayStop = {
  at: string;                              // "day 1 (Mon) night"
  writes: WriteRecord[];                   // the move to this stop
  settled: QuestGroup[];                   // the settle after that move
} & OccasionStep;                          // the `dayEnd` / `dayStart` occasion raised here

type WriteRecord =
  | { kind: "set"; path: string; value: unknown }
  | { kind: "assert"; fact: string }
  | { kind: "retract"; pattern: string; held: boolean }
  | { kind: "accept"; quest: string; by: "engine" };   // an `engine: { accept }` (dsl 0.26.0)

type QuestGroup = {
  document: string;                        // the quest document
  commands: RunnerRecord[];                // its `objective` (`failed: true` and `failedBy: "by" | "until"` on a missed
                                           // deadline) / `quest` (`failedBy` when it failed) /
                                           // `grant` (`credited: { path, value }`) / `acceptSpent` (`quest`, `parent`,
                                           // `parentStatus`: an accept spent while the parent was not active) / handler records
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
  | { fact: string; support: "seed fact" }
  | { fact: string; support: "asserted"; assertedBy?: string } // "engine step 2", "entry `tomasOil`, step 3"
  | { fact: string; support: "derived"; rule: string; premises: Premise[] };
type Attempt = { rule: string; premises: Premise[] };
type Premise =
  | { status: "holds"; proof: Proof }
  | { status: "missing"; atom: string; attempts: Attempt[] }
  | { status: "absent"; negated: string; attempts?: Attempt[] } // the rules that could conclude it, when any
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
│   ├── town/welcome.lute
│   ├── town/morning.lute
│   ├── town/day-end.lute
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
  townVisit: { select: first, description: The player arrives in town }
  talk:      { select: first, target: { prefix: npc, entity: npc }, description: The player talks to someone (npc.<name>) }
  dayEnd:    { select: first, description: "The engine closed the day; run.day is already advanced" }
```

공유 상태, `world.schema.yaml`. 하루는 엔진의 것입니다: 콘텐츠는 `run.day`를 읽지만, 쓰는 것은
엔진뿐이고 — 그래서 `engine:` 스텝뿐입니다:

```yaml
state:
  run.day:        { type: int, default: 1, owner: engine }
  user.bond.mara: { type: int, default: 0 }

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
| `town.welcome` | `scenes/town/welcome.lute` | `townVisit` | priority 10 | `user` |
| `town.morning` | `scenes/town/morning.lute` | `townVisit` | `when: '!@firstDay'` — `Day {{run.day}}.`를 출력 | `false` |
| `town.dayEnd` | `scenes/town/day-end.lute` | `dayEnd` | — | `false` |
| `mara.first` | `scenes/talk/mara-first.lute` | `talk` → `npc.mara` | priority 10 | `user` |
| `mara.idle` | `scenes/talk/mara-idle.lute` | `talk` → `npc.mara` | — | `false` |
| `tomasOil`(엔트리) | `lore/tomas.lute` | `talk` → `npc.tomas` | priority 10, `when="quest.lampOut.state == 'active'"`, `knows(lamp)`를 단언 | — |
| `tomasBusy`(엔트리) | `lore/tomas.lute` | `talk` → `npc.tomas` | — | — |

`mara.first`가 하루를 가르는 질문을 던집니다:

```lute
<branch id="maraAsk" prompt="What do you say?">
  <choice id="lamp" text="Offer to find out why">
    @mara{emotion="delighted"}: Would you? Tomas keeps the oil. Ask him.
    ::set{ user.bond.mara += 1 }
    ::accept{quest="lampOut"}
  </choice>
  <choice id="leave" text="Say nothing">
    @mara: Suit yourself.
  </choice>
</branch>
```

그 선택지가 수락하는 퀘스트 `quests/lamp.lute`는 엔진이 하루를 닫아야만 끝나며, 이번 런의 것(`tier="run"`)이라
새 런이 시작되면 처음부터 다시 시작합니다:

```lute
<quest id="lampOut" title="The lamp by the door" tier="run">
  <objective id="ask" title="Ask Tomas about the oil" done="holds('knows', ['lamp'])"/>
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
  - occasion: townVisit
    expect: { winner: town.welcome }
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
  - occasion: townVisit
    expect: { winner: town.morning, notOffered: [town.welcome] }
expect:
  end: complete
  quests: { lampOut: complete }
  state: { run.day: 2, user.bond.mara: 1 }
  facts: [knows(lamp)]
```

프로젝트 디렉터리에서:

```console
$ lute play . --script plays/first-day.play.yaml
```

```
── step 1 · townVisit ──────────────
  ✓ town.welcome [scene, priority 10]
  ✗ town.morning [scene, priority 0] — when: false
  → town.welcome
::bg{location="town" time="day"}
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
  ✓ town.dayEnd [scene, priority 0]
  → town.dayEnd
@narrator: One by one, the lamps go out.
  lampOut.wait done
  quest lampOut -> complete
@narrator: By morning the lamp by the door is burning again.
── step 6 · townVisit ──────────────
  ✓ town.morning [scene, priority 0]
  ✗ town.welcome [scene, priority 10] — once: user — already presented
  → town.morning
@narrator: Day 2. The square is already awake.
── end: complete (6 steps) ──────────────
── expect: every expectation held ──────────────
```

스텝별로 읽으면:

- **시작** — `lampOut`에는 `start`가 없으므로 첫 스텝 전에 아무것도 활성화되지 않고, `── start` 블록도
  없습니다.
- **스텝 1** — 1일째이므로 `town.morning`의 `@firstDay` 가드가 그것을 막고, 환영 인사가 재생됩니다.
- **스텝 2** — `talk`가 `npc` 종류의 멤버인 `npc.mara`를 위해 발생합니다. 마라의 씬 둘이 모두 후보이고
  첫 만남이 대체 씬보다 앞섭니다. `lamp`를 고르면 유대가 오르고 퀘스트가 수락되며, 퀘스트는 제시 직후
  활성화됩니다.
- **스텝 3** — 퀘스트가 활성이므로 토마스의 기름 엔트리가 자격을 얻고 그의 짧은 대사보다 앞섭니다. 첫
  읽기가 `knows(lamp)`를 단언하여 첫 번째 목표를 완료합니다.
- **스텝 4** — 엔진이 하루를 닫습니다. `engine:` 스텝은 콘텐츠가 쓸 수 없는(`owner: engine`) `run.day`를
  쓰며, 아무것도 제시되지 않고 계기도 발생하지 않습니다.
- **스텝 5** — `dayEnd`가 밤 씬을 제시한 뒤 `lampOut.wait`(`on="dayEnd"`)을 판정하고, 퀘스트가
  완료됩니다. 그 `questComplete` 핸들러가 재생됩니다.
- **스텝 6** — `town.welcome`은 영구히 소진되었고(`once: user`) `run.day`가 2이므로 아침 씬이 재생됩니다.

### 한 스텝만 다르게 결정하기

스텝 자신의 `choose:`는 한 번의 제시만 바꾸고 나머지는 건드리지 않습니다. 여기서 플레이어는 마라에게
아무 말도 하지 않지만, 기대값은 여전히 첫날을 기술하므로 불일치가 납니다:

```yaml
choose:
  maraAsk: lamp
steps:
  - occasion: townVisit
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
── step 1 · townVisit ──────────────
  ✓ town.welcome [scene, priority 10]
  ✗ town.morning [scene, priority 0] — when: false
  → town.welcome
::bg{location="town" time="day"}
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
presented: { user: [town.welcome, mara.first] }
quests: { lampOut: complete }
steps:
  - label: the engine starts run two on day 3
    newRun: { state: { run.day: 3 } }
  - occasion: townVisit
    expect: { winner: town.morning, notOffered: [town.welcome] }
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
    expect: { winner: town.dayEnd }
expect:
  end: complete
  state: { run.day: 5 }
  transcriptContains: ["Any luck with the lamp?", "Day 3."]
  transcriptLacks: ["You're new."]
```

```
── step 1 (the engine starts run two on day 3) · new run ──────────────
  run.* state, run-tier facts and once: run reset; prev.run.* holds the ended run (1 value)
  prev.run.day = 1
  quest lampOut -> unset (tier: run; was complete)
  set run.day = 3
── step 2 · townVisit ──────────────
  ✓ town.morning [scene, priority 0]
  ✗ town.welcome [scene, priority 10] — once: user — already presented
  → town.morning
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
  ✓ town.dayEnd [scene, priority 0]
  → town.dayEnd
@narrator: One by one, the lamps go out.
── end: complete (7 steps) ──────────────
── expect: every expectation held ──────────────
```

- **세이브** — `presented.user`가 `once: user` 씬 둘을 소진시키고, `quests:`가 `lampOut`을 완료 상태로
  이어받으며, `user.bond.mara` 시드 덕분에 마라의 `@trusted` 대사가 재생됩니다.
- **스텝 1** — 긴 형태의 `newRun`이 새 런을 시드합니다: 끝난 런의 `run.day`가 스냅숏되고(`prev.run.day = 1`),
  run 등급 퀘스트 `lampOut`이 `unset`으로 돌아가며(그래서 기름 엔트리의 `when`이 거짓), `run.day`가 기본값으로
  초기화된 뒤 시드가 3으로 설정합니다.
- **스텝 5** — `engine:` 스텝 하나를 반복: 각 반복은 자신의 레코드이고, 런은 `run.day`가 5인 채로
  끝납니다.

### 테스트 스위트에서

`expect:`를 가진 모든 플레이는 `lute test`에서 시나리오 테스트와 나란히 실행됩니다. 위의 두 플레이를
`plays/say-nothing.play.yaml`과 `plays/returning.play.yaml`로 저장하면:

```console
$ lute test . --project .
```

```
PASS  ./tests/lamp-quest.test.yaml  (./quests/lamp.lute)
PASS  ./tests/mara-first.test.yaml  (./scenes/talk/mara-first.lute)
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
PASS  ./tests/lamp-quest.test.yaml  (./quests/lamp.lute)
PASS  ./tests/mara-first.test.yaml  (./scenes/talk/mara-first.lute)
PASS  ./plays/first-day.play.yaml  (play of .)

3 passed, 0 failed

coverage over 2 traced path(s) and 1 play(s) (plays count toward what they presented and the choices they picked, not match arms):
  branch/hub maraAsk (scenes/talk/mara-first.lute:maraAsk): 1/2 chosen [lamp]; never chosen [leave]
  2 untested unit(s) under . — no *.test.yaml presents them and no play presents them:
    lore/tomas.lute: tomasBusy
    scenes/talk/mara-idle.lute
  every other beat under . is presented by a play
```

플레이가 환영 인사, 아침, 밤 씬, 마라와의 첫 만남, 토마스의 기름 엔트리를 제시했으므로 남은 것은
`mara-idle.lute`와 엔트리 `tomasBusy`뿐입니다 — 둘 다 `plays/returning.play.yaml`이 커버합니다.
