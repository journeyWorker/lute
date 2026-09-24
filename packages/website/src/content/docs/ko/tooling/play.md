---
title: 스토리 플레이
description: "계기(occasion)와 비트(beat, dsl 0.21.0) — 어떤 스토리 조각이 어떤 엔진 순간에 응답하는지 프로젝트가 선언하는 방법 — 그리고 스크립트로 적은 계기 발생 순서를 프로젝트 전체에 걸쳐 걸으며 모든 후보, 그 판정, 승자를 출력하는 참조 플레이어 `lute play`."
---

내러티브 게임은 저마다의 순간에 다음 스토리 조각을 고릅니다: 허브 방문, 방 입장, NPC와의 대화, 새로운
하루, 새 런의 시작. Lute는 그런 순간을 **계기(occasion)**, 그 순간에 응답하는 스토리 조각을
**비트(beat)** 라고 부릅니다(dsl 0.21.0). 엔진이 계기를 발생시키고, Lute는 어떤 비트가 자격이 있고 어느
비트가 이기는지를 정의합니다. `lute play`는 이 계약의 참조 플레이어입니다: 발생시킬 계기를 적은
스크립트를 주면 프로젝트 전체를 걸으며 모든 후보 비트와 그 판정, 승자를 출력하고, 승자를 `lute run`과
같은 참조 러너로 재생합니다.

규범 텍스트는 [0.21.0 제안서](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.21.0.md)이고,
엔진 측 계약(IR 필드와 엔진이 구현하는 선택 알고리즘)은
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
  talk:      { select: first, target: true }
  roomEnter: { select: first, target: true }
  inbox:     { select: all, description: Letters waiting at the fountain }
```

- `select: first`(기본값) — 엔진은 이긴 비트 하나를 제시합니다.
- `select: all` — 엔진은 자격 있는 비트를 모두 선택 순서대로 제시하고 플레이어가 하나를 고릅니다(메시지
  수신함, 지역 지도, "순서 무관" 퀘스트 제공자).
- `target: true` — 계기가 무언가를 **위해** 발생하며, 비트는 하나의 대상으로 자신을 제한할 수 있습니다.
  기본값 `false`.
- `description` — 도구용 선택적 설명.

해석된 플러그인 중 계기를 선언한 것이 없으면 계기 이름은 **모양만(shape-only)** 검사됩니다: 어떤
식별자든 받아들이므로, 엔진 플러그인이 생기기 전에도 비트를 쓸 수 있습니다. 어느 플러그인이든 계기를
선언하는 순간, 선언되지 않은 계기를 가리키는 비트는 `E-OCCASION-UNKNOWN`이 되고, `target: true` 없이
선언된 계기에 `target`을 붙이면 `E-BEAT-ATTR`입니다. 이 export는 가드된 섹션으로 캐퍼빌리티 스냅샷에
접히므로, 플러그인이 계기를 선언하지 않는 프로젝트는 `capabilityVersion`이 그대로입니다.

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
| `target` | 선택. 계기가 이 대상을 위해 발생했을 때만 씬이 후보가 됩니다(점으로 구분된 id, `<entry target>`과 같은 모양). |
| `when` | 선택. `run` / `user` / `app` 상태, `quest.*`, `entry.<id>.read`, 팩트 질의에 대한 CEL 조건. 씬 자신의 `scene.*` 상태는 아직 존재하지 않으므로 거부됩니다. |
| `priority` | 선택 정수, 기본값 `0`. 높을수록 이깁니다. |
| `once` | `run`(기본값 — 런마다 한 번), `user`(평생 한 번), 또는 `false`(반복 가능). |

`after:`는 의미가 그대로입니다 — [연결성](/connectivity/scene-graph/) 분석이 다루는 `visited` /
`completed` / `active`에 대한 구조적 전제조건 — 그리고 비트는 `after:`와 `when`이 모두 성립할 때만
자격이 있습니다. `on` 없이 `when`, `target`, `priority`, `once`를 쓰면 `E-BEAT-ATTR`입니다: 어떤 계기에도
응답하지 않는 씬은 이전처럼 명시적인 흐름으로 도달합니다.

### 엔트리 비트

[로어 엔트리](/language/lore-entries/)는 기존의 `target`, `when` 옆에 `on=`과 `priority=`를 적어
계기에 응답합니다:

```lute
<entry id="achillesBark" on="talk" target="npc.achilles" category="bark" priority="10" when="user.runs >= 3">
  @achilles: Back again, lad.
</entry>
```

엔트리에는 `once`가 없습니다: 다시 제시되는 것이 엔트리의 본성입니다. 한 번만 들려야 하는 엔트리는
자신의 `entry.<id>.read`로 가드합니다.

## 선택

엔진이 계기 `O`를, 선택적으로 대상 `T`를 위해 발생시키면:

1. **후보**는 `on: O`이고 `target`이 없거나 `T`와 같은 비트입니다. 대상 없이 발생한 계기에는 대상이
   없는 후보만 있습니다.
2. 후보는 `after:`가 성립하고(씬 비트), `when`이 성립하고, `once`가 소진되지 않았을 때 **자격이
   있습니다** — `run`: 이번 런에 아직 제시되지 않음, `user`: 한 번도 제시되지 않음, `false`: 소진되지
   않음. 엔트리 비트는 소진되지 않습니다.
3. 자격 있는 비트는 **priority 내림차순, 그다음 프로젝트 순서**로 정렬됩니다: 문서 경로, 그다음 문서 안의
   선언 순서 — `project.index.json`의 `beats` 순서입니다. 같은 계기의 씬 비트와 엔트리 비트는 한 목록에서
   경쟁합니다.
4. `select: first`는 첫 번째 자격 있는 비트를 제시하고, `select: all`은 정렬된 목록을 제시한 뒤
   플레이어가 고른 것을 제시합니다.
5. **자격 있는 비트가 없으면** — 계기는 스토리 없이 지나가고, 그 순간에 대한 엔진의 기본 동작이
   적용됩니다.

선택은 결정적입니다: 같은 상태, 팩트, 제시 이력이면 어느 엔진에서든 같은 비트를 고릅니다. 가중 무작위나
쿨다운은 그 위에 얹는 엔진 정책이며, 참조 도구는 정확히 이 순서를 구현합니다.

## `lute play`

```console
$ lute play <PROJECT_DIR> --script <FILE> [--json]
```

- `<PROJECT_DIR>` — 프로젝트 루트(`lute.project.yaml`과 그 플러그인). 프로젝트는 `compile --all`과 같은
  게이트와 선언 유니온(씬, 퀘스트, 로어 문서)으로 메모리에서 통째로 컴파일됩니다.
- `--script <FILE>` — 필수: 플레이 스크립트, `*.play.yaml` 파일.
- `--json` — 같은 트랜스크립트를 stdout에 JSON 객체 하나로 출력합니다.

명령줄에서 따로 시드할 것은 없습니다: 상태, 팩트, 결정이 모두 스크립트에 있으므로 하나의 플레이스루는
리뷰 가능한 파일 하나입니다.

### 플레이 스크립트

플레이 스크립트의 최상위 키는 정확히 네 개입니다 — `state`, `facts`, `choose`, `steps`:

```yaml
state: { user.runs: 9 }          # path -> scalar literal, over the declared defaults
facts: ["knows(achilles)"]       # ground facts, added to the project's seed facts
steps:                           # required, non-empty
  - occasion: hubVisit           # raise an occasion
  - occasion: talk
    target: npc.achilles         # only for an occasion declared `target: true`
  - occasion: inbox
    pick: megNote                # required for a `select: all` occasion, refused for `select: first`
  - newRun: true                 # start a new run
choose:                          # branch/hub id -> choice id (a list for a hub's visit sequence)
  gift: accept                   # applied to every presented scene
```

`state:`, `facts:`, `choose:`는 [`lute trace --mock`](/tooling/tracing/) 파일과 정확히 같은 문법을
씁니다. 각 스텝은 `{occasion, target?, pick?}` 또는 `{newRun: true}` 중 하나입니다.

다음 경우 스크립트는 아무것도 재생하기 전에 거부됩니다 — **사용법 오류, 종료 코드 2**: 읽을 수 없거나
잘못된 YAML, 알 수 없는 최상위 키, `steps`가 없거나 비어 있음, 두 모양 중 어느 것과도 정확히 일치하지 않는
스텝, 해석된 플러그인 중 아무도 선언하지 않은 계기를 가리키는 스텝(어떤 플러그인이 계기를 선언한 경우),
또는 모양만 검사하는 프로젝트에서 어떤 비트도 응답하지 않는 계기, `target: true`로 선언되지 않은 계기에
`target`을 붙인 스텝, `select: first` 계기의 `pick`, `pick`이 없는 `select: all` 스텝, 그 계기에 응답하는
비트를 가리키지 않는 `pick`.

### 각 스텝이 하는 일

1. **후보** — 프로젝트의 비트 목록에서 `on`이 스텝의 계기와 같고 `target`이 없거나 스텝의 `target`과
   같은 모든 비트.
2. **판정** — 후보는 `once`가 소진되지 않았고(씬 비트만: `run` — 마지막 `newRun` 이후 제시되지 않음,
   `user` — 이 플레이에서 한 번도 제시되지 않음, `false` — 소진되지 않음), `after:`가 성립하고(씬 비트;
   제시된 씬의 **실시간** `visited` 집합과 실제 퀘스트 상태의 `completed` / `active` 집합에 대해 평가),
   `when`이 성립할 때(참조 러너의 CEL 평가기가 실시간 상태와 팩트에 대해, Datalog 규칙을 적용해 평가)
   자격이 있습니다. `when`이 unknown으로 평가되면 — `validAt(…)`, `now()` — 그 비트를 이름 붙여
   **미완료(종료 코드 3)**로 정지합니다. 단, `select: first` 계기에서 확실히 자격 있는 비트가 그보다
   앞서면 승자를 바꿀 수 없으므로 정지하지 않습니다.
3. **순서** — 자격 있는 비트를 priority 내림차순, 그다음 프로젝트 순서로.
4. **선택** — 계기의 `select`는 해석된 플러그인의 `occasions` export에서 옵니다(선언되지 않은 계기는
   `first`). `first`: 첫 번째 자격 있는 비트가 이기며, 자격 있는 비트가 없으면 계기는 스토리 없이
   지나갑니다. `all`: 스텝의 `pick`이 제시되며, 그 순간 자격이 없는 pick은 **오류(종료 코드 1)**입니다.
5. **제시** — 씬 비트는 참조 러너(`lute run`의 평가기)로 실행됩니다: `scene.*`는 씬 자신의 기본값으로
   초기화되고, `run.*` / `user.*` / `app.*` / `quest.*` 상태와 팩트는 이어지며, `choose:`가 branch와
   hub를 결정합니다. 스크립트에 없는 결정은 **미완료(종료 코드 3)**로 정지합니다. 엔트리 비트는 로어
   엔트리 규칙으로 제시됩니다: 효과는 첫 읽기에만 적용되고, 그 뒤 `entry.<id>.read`가 true가 됩니다.
   씬의 `::end`는 플레이스루 전체를 완료로 끝냅니다.
6. **퀘스트** — 매 제시 후, 그리고 스텝 1 전에 한 번, 모든 퀘스트 라이프사이클이 `lute run`이 퀘스트
   산출물을 진행시키는 것과 정확히 같게 진행됩니다: 활성화(`start`, 없으면 즉시), 목표 완료(단조적이며
   목표 본문은 한 번만 재생), 완료 전의 `fail`, `<on>` 핸들러, `<reward>` 지급. 그래서 이후의 `quest.*`에
   대한 `when`이나 `after: completed(…)` / `active(…)`는 실제 진행을 봅니다.

`newRun: true` 스텝은 `run.*` 상태를 선언된 기본값으로, run 등급 팩트를 프로젝트의 시드 팩트로 되돌리고,
run 등급인 `entry.<id>.read` 플래그(그래서 새 런의 첫 읽기에서 엔트리 효과가 다시 적용됩니다)와
`once: run` 소진 기록을 초기화합니다. `user.*` / `app.*` / `quest.*` 상태, user·app 등급 팩트, `visited`
이력, `once: user` 소진 기록은 유지됩니다. 서로 다른 `lute play` 호출 사이의 `once: user`는 모델링되지
않습니다 — 여러 런을 한 스크립트에 넣고 `newRun` 스텝으로 나누세요.

### 종료 코드

| 코드 | 의미 |
|---|---|
| `0` | 완료 — 모든 스텝이 재생되었거나, 씬의 `::end`가 플레이스루를 끝냄. |
| `1` | 오류 — 프로젝트 컴파일 실패, 어휘 충돌, 또는 자격이 없는 `pick`. |
| `2` | 사용법 또는 I/O — 잘못된 스크립트, 알 수 없는 계기, 읽을 수 없는 프로젝트, 잘못된 산출물. |
| `3` | 미완료 — 스크립트에 없는 choice나 hub, unknown으로 평가되는 `when`이나 퀘스트 목표, 또는 해석되지 않은 `now()` / `validAt()` / 플러그인 `bridgeResult`. |

## 트랜스크립트

사람이 읽는 트랜스크립트는 모든 스텝을 이름 붙이고, 후보와 그 판정을 나열하고, 제시된 비트가 재생되기
전에 승자를 표시합니다:

```
── start ──────────────
  quest oldSoldier -> active
── step 1 · hubVisit ──────────────
  ✓ hub.firstEver [scene, priority 20]
  ✓ hub.welcome [scene, priority 10]
  → hub.firstEver
@hypnos: Oh! You're back already? I mean — welcome home, I guess.
```

- 모든 헤더는 텍스트 뒤에 고정된 `──────────────` 선이 붙습니다. `── start`는 스텝 1 전에 일어난 퀘스트
  전이를 담고, 각 스텝은 `── step <n> · <occasion>`으로 시작하며, 대상이 있는 스텝에는 `→ <target>`,
  pick에는 `(select: all, pick: <id>)`가 붙습니다.
- 후보는 자격 있는 것(`✓`)을 선택 순서대로 먼저, 그다음 자격 없는 것(`✗`)을 선택 순서대로 나열하며, 각각
  종류와 priority를 표시합니다. 자격 없는 후보에는 이유가 붙습니다:
  `once: run — already presented this run`, `once: user — already presented`,
  `after: prerequisite not satisfied`, 또는 `when: false`.
- `→ <id>`가 승자를 가리킵니다. 승자가 없으면 `→ (no eligible beat — the occasion passes)`로 표시됩니다.
- 그 뒤에 제시된 비트 자신의 트랜스크립트가 이어집니다: 콘텐츠 줄은 `@speaker: text`, 결정은
  `▷ choice <id>: … ← chosen: <id>`, 상태 쓰기는 `set <path> = <value>`, 엔트리는
  `entry <id> (first read)` — 또는 `entry <id> (re-read: effects skipped)`와 함께 건너뛴 각 효과에
  `(skipped: re-read)` 표시. 제시로 인한 퀘스트 전이가 마지막에 옵니다: `<quest>.<objective> done`,
  `quest <id> -> <state>`, 보상 지급.
- `newRun` 스텝은 `── step <n> · new run`과 `run.* state, run-tier facts and once: run reset`을
  출력합니다.
- 워크는 `── end: complete (<n> steps)`로, 중간에 멈추면 `── halted: <message>`로 끝납니다.

`--json`은 같은 워크를 객체 하나로 출력합니다:

```ts
type PlayTranscript = {
  exit: "complete" | "incomplete" | "error";
  start: { quests: QuestGroup[] };         // transitions made before step 1
  steps: (OccasionStep | NewRunStep)[];
  endReason?: string;
  error?: { message: string };
};

type OccasionStep = {
  step: number;
  occasion: string;
  target?: string;
  select: "first" | "all";
  pick?: string;
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
  quests: QuestGroup[];                    // transitions after this presentation
};

type NewRunStep = { step: number; newRun: true };

type QuestGroup = {
  document: string;                        // the quest document
  commands: RunnerRecord[];                // its `objective` / `quest` / `grant` records
};
```

## 예제

*Hades* 모양의 작은 허브 게임입니다: 플레이어가 런 사이에 돌아오는 라운지, 열 번째 탈출 시도에 선물을
주는 노병, 그리고 편지 수신함. 프로젝트:

```
house/
├── lute.project.yaml
├── house.schema.yaml
├── plugins/house.occasions/
│   ├── plugin.yaml
│   └── occasions/house.yaml
├── scenes/
│   ├── hub-first-ever.lute
│   ├── hub-welcome.lute
│   └── achilles-gift.lute
├── quests/old-soldier.lute
├── lore/letters.lute
└── plays/tenth-run.play.yaml
```

프로젝트는 엔진의 계기만 export하는 플러그인 하나를 활성화합니다 — `lute.project.yaml`,
`plugins/house.occasions/plugin.yaml`, `plugins/house.occasions/occasions/house.yaml`:

```yaml
pluginsDir: plugins/
defaultProfile: house
profiles:
  house:
    plugins: { house.occasions: true }
```

```yaml
id: house.occasions
version: 0.1.0
kind: capability
depends: [ { id: lute.core, range: "^0.0.1" } ]
exports:
  occasions: occasions/
```

```yaml
occasions:
  hubVisit: { select: first }
  talk:     { select: first, target: true }
  inbox:    { select: all }
```

공유 상태, `house.schema.yaml`:

```yaml
state:
  user.runs:         { type: number, default: 0 }
  user.giftAccepted: { type: bool, default: false }
```

라운지 씬 두 개가 `hubVisit`에 응답합니다. 첫 귀환 인사는 일상 인사보다 우선순위가 높고 평생 한 번만
들리며, 일상 인사는 기본값 `once: run`을 유지합니다:

```lute unverified="one file of the multi-file worked project on this page; it needs the occasion-declaring plugin and house.schema.yaml shown beside it"
---
kind: scene
id: hub.firstEver
uses: ../house.schema.yaml
on: hubVisit
priority: 20
once: user
---

## The lounge

@hypnos: Oh! You're back already? I mean — welcome home, I guess.
```

```lute unverified="one file of the multi-file worked project on this page; it needs the occasion-declaring plugin and house.schema.yaml shown beside it"
---
kind: scene
id: hub.welcome
uses: ../house.schema.yaml
on: hubVisit
priority: 10
---

## The lounge

@hypnos: Welcome back. Try not to die so much.
```

노병의 선물은 한 대상을 위한 `talk`에, 열 번째 런부터만 응답합니다:

```lute unverified="one file of the multi-file worked project on this page; it needs the occasion-declaring plugin and house.schema.yaml shown beside it"
---
kind: scene
id: achilles.gift
uses: ../house.schema.yaml
on: talk
target: npc.achilles
when: 'user.runs >= 10'
priority: 50
once: user
---

## The courtyard

@achilles: Ten times through that door, lad. That has earned you something.

<branch id="gift">
  <choice id="accept" label="Accept the gift">
    ::set{ user.giftAccepted = true }
    @achilles: Wear it well.
  </choice>
  <choice id="decline" label="Decline">
    @achilles: Another time, then.
  </choice>
</branch>
```

퀘스트 하나가 선물을 추적하고, 편지 두 통이 `inbox`에 응답합니다 — 하나는 선물 이후에만:

```lute unverified="one file of the multi-file worked project on this page; it needs house.schema.yaml shown beside it"
---
kind: quest
uses: ../house.schema.yaml
title: The old soldier
---

<quest id="oldSoldier" title="The old soldier">
  <objective id="takeGift" title="Accept the old soldier's gift" done="user.giftAccepted"/>
</quest>
```

```lute unverified="one file of the multi-file worked project on this page; it needs the occasion-declaring plugin and house.schema.yaml shown beside it"
---
kind: lore
id: house.letters
uses: ../house.schema.yaml
---

<entry id="megNote" on="inbox" category="note" priority="5" when="user.giftAccepted">
  @meg: Heard the old soldier gave you something. Don't get sentimental.
</entry>

<entry id="dusaNote" on="inbox" category="note">
  @dusa: The lounge is spotless! Well, almost.
</entry>
```

스크립트는 열 번째 런과 열한 번째 런의 시작을 재생합니다:

```yaml
# plays/tenth-run.play.yaml
state: { user.runs: 10 }
steps:
  - occasion: hubVisit
  - occasion: inbox
    pick: dusaNote
  - occasion: talk
    target: npc.achilles
  - occasion: hubVisit
  - occasion: hubVisit
  - newRun: true
  - occasion: hubVisit
  - occasion: inbox
    pick: megNote
choose:
  gift: accept
```

```console
$ lute play house --script house/plays/tenth-run.play.yaml
```

```
── start ──────────────
  quest oldSoldier -> active
── step 1 · hubVisit ──────────────
  ✓ hub.firstEver [scene, priority 20]
  ✓ hub.welcome [scene, priority 10]
  → hub.firstEver
@hypnos: Oh! You're back already? I mean — welcome home, I guess.
── step 2 · inbox (select: all, pick: dusaNote) ──────────────
  ✓ dusaNote [entry, priority 0]
  ✗ megNote [entry, priority 5] — when: false
  → dusaNote
  entry dusaNote (first read)
@dusa: The lounge is spotless! Well, almost.
── step 3 · talk → npc.achilles ──────────────
  ✓ achilles.gift [scene, priority 50]
  → achilles.gift
@achilles: Ten times through that door, lad. That has earned you something.
▷ choice gift: [accept] decline        ← chosen: accept
  set user.giftAccepted = true
@achilles: Wear it well.
  oldSoldier.takeGift done
  quest oldSoldier -> complete
── step 4 · hubVisit ──────────────
  ✓ hub.welcome [scene, priority 10]
  ✗ hub.firstEver [scene, priority 20] — once: user — already presented
  → hub.welcome
@hypnos: Welcome back. Try not to die so much.
── step 5 · hubVisit ──────────────
  ✗ hub.firstEver [scene, priority 20] — once: user — already presented
  ✗ hub.welcome [scene, priority 10] — once: run — already presented this run
  → (no eligible beat — the occasion passes)
── step 6 · new run ──────────────
  run.* state, run-tier facts and once: run reset
── step 7 · hubVisit ──────────────
  ✓ hub.welcome [scene, priority 10]
  ✗ hub.firstEver [scene, priority 20] — once: user — already presented
  → hub.welcome
@hypnos: Welcome back. Try not to die so much.
── step 8 · inbox (select: all, pick: megNote) ──────────────
  ✓ megNote [entry, priority 5]
  ✓ dusaNote [entry, priority 0]
  → megNote
  entry megNote (first read)
@meg: Heard the old soldier gave you something. Don't get sentimental.
── end: complete (8 steps) ──────────────
```

스텝별로 읽으면:

- **시작** — 퀘스트에 `start`가 없으므로 첫 스텝 전에 활성화됩니다.
- **스텝 1** — 두 라운지 씬 모두 자격이 있고, priority 20이 10을 이깁니다.
- **스텝 2** — `inbox`는 `select: all`이므로 스크립트가 고릅니다. `megNote`의 `when`은 아직 false라서
  목록에는 나오지만 제시되지 않습니다. 여기서 그것을 고르면 오류(종료 코드 1)입니다.
- **스텝 3** — `user.runs`가 10이므로 선물이 자격을 얻습니다. 선물을 받으면 `user.giftAccepted`가
  설정되고, 제시 직후 퀘스트가 진행됩니다.
- **스텝 4–5** — 첫 귀환 인사는 영구히 소진되었고(`once: user`), 일상 인사는 이번 런에 한 번 재생되며,
  다음 방문은 스토리 없이 지나갑니다.
- **스텝 6–7** — 새 런이 `once: run` 소진 기록을 초기화하므로 일상 인사가 다시 자격을 얻습니다.
  `user.*` 상태와 `once: user` 소진 기록은 유지됩니다.
- **스텝 8** — 선물 덕분에 `megNote`가 자격을 얻었고, 퀘스트 완료는 이후의
  `after: completed("oldSoldier")`가 보게 될 바로 그 사실입니다.
