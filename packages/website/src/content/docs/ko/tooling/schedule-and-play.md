---
title: 스케줄과 플레이
description: "`schedule.yaml` — 프로젝트의 장면을 파일 위치가 아니라 읽기 순서로 배치하는 틱 클록, `user`/`world` 레인, 가드된 배치 — 그리고 `--coverage` 리뷰-갭 리포팅을 갖춘 프로젝트 전체 연쇄 플레이스루 명령 `lute play`."
---

`schedule.yaml`은 파일 위치 대신 틱 클록 위에 프로젝트의 장면을 배치합니다.
`lute play <PROJECT_DIR>`는 그 스케줄을 하나의 연쇄된, 리뷰어가 읽을 수 있는
트랜스크립트로 걸어갑니다 — 한 루트를 따라가는 플레이어가 실제로 보는 순서
그대로입니다. 둘 다 **툴체인 전용**입니다: `schedule.yaml`은 `kind:`도
`luteVersion:`도, 캐퍼빌리티 폴드도 갖지 않으며 언어도 IR도 이를 추가하기 위해
움직이지 않았습니다 — `lute check`/`lute compile` 그 무엇도 이 파일을 들어본 적이
없습니다. `lute play`는 `schedule.yaml`이 없는 프로젝트를 그대로 거부합니다(종료
코드 **2**): 형제 루트 파일들은 의도적으로 가드되지 않으므로(파일 분리 자체가
루트입니다) `after:` 그래프 순회로는 그중 하나의 루트를 선택할 수 없습니다 —
모든 형제를 다 플레이하게 됩니다. 전체 키·진단 레퍼런스(이 페이지는 이를
압축한 것입니다): [`docs/schedule-and-play.md`](https://github.com/journeyWorker/lute/blob/main/docs/schedule-and-play.md).

## `schedule.yaml`

```yaml
clock:
  buckets: [dawn, morning, late_morning, afternoon, late_afternoon, evening, night, midnight]
  ticksPerBucket: 12
  days: 7

lanes:
  user:  { exclusive: true, idleThreshold: 0 }   # 단일 스레드, 겹침을 가드
  world: { exclusive: false }                    # 설계상 겹침 허용

assume:
  - "run.inflow != 'none'"                       # 루트-스페이스 스윕을 좁힐 뿐, 갭을 절대 숨기지 않음

placements:
  - event: kuhen
    lane: user
    at: d2.morning+0
    size: 4
    variants:
      - when: "run.inflow == 'iroha'"
        doc: scenes/kuhen/iroha.lute
      - when: "run.inflow == 'reiha'"
        doc: scenes/kuhen/reiha.lute
        at: d2.afternoon+0       # 같은 이벤트, 이 루트에서는 다른 위치
        size: 6
```

`clock:`은 `buckets`(이름 붙은, 순서 있는, 중복 없는 목록) × `ticksPerBucket`
× `days`(1부터 시작 — `d0`는 거부됩니다)이며, 스토리 클록은 이 셋의 곱입니다
(`u32` 오버플로 검사됨). `lanes:`는 임의의 이름의 레인 집합을 선언합니다;
`exclusive: true`는 동시-충족 가능한 배치들이 구간을 겹치지 못하도록 가드하고
(`E-SCHED-USER-OVERLAP`), `exclusive: false`는 설계상 겹침을 허용합니다.
`assume:`은 스케줄이 절대 일어나지 않는다고 증명할 수 있는 루트-스페이스
조합(예: "inflow는 절대 `none`이 아니다"라는 업스트림 계약)을 정적
갭/모호성/겹침 스윕에서 제외하는 가드-표면 CEL 문자열 목록입니다 — 스윕을
좁히기만 할 뿐, 실제 갭을 절대 숨기지 않습니다.

`placements:`의 각 항목은 하나의 **레인** 위에서 `[at, at+size)` 구간을
차지하는 하나의 **이벤트**이며, 두 형태 중 하나입니다: 배치에 직접 붙는
가드 없는 단일 `doc:`, 또는 라우트당 최대 하나만 충족 가능한
`variants:`(`when`/`doc`/`at?`/`size?`/`presentation?`) 목록. 두 형태 모두
주지 않거나, 둘 다 주거나, 빈 `variants:`를 주면 `E-SCHED-VARIANT-FORM`입니다.
`optional: true`는 어떤 루트에서는 충족 가능한 variant가 하나도 없어도 되도록
합법화합니다(`E-SCHED-VARIANT-GAP` 없음) — 일부 레인만 아직 작성된 콘텐츠용입니다.
`presentation`(기본값 `100`, 낮을수록 먼저 재생)은 *언제 씬이 제시되는지*를
*클록 위에서 언제 일어나는지*와 분리합니다 — variant는 자신이 속한 배치의
`at`/`size`/`presentation`을 오버라이드할 수 있으며, 이것이 "같은 이벤트,
이 루트에서는 다른 위치"를 표현하는 방법입니다 — 결코 번호가 다른 파일이
아닙니다.

### `at:` 문법

`[dN.]<bucket>+<tick>`(`dN.`은 생략 가능, 기본값은 1일차; `0 ≤ tick <
ticksPerBucket`) 또는 있는 그대로의 음이 아닌 절대 틱 정수. 잘못된 형태,
`d0`, 알 수 없는 bucket, 범위를 벗어난 tick은 `E-SCHED-AT-PARSE`입니다;
형태는 올바르지만 클록을 넘어 해석되는(혹은 해석 중 오버플로하는) 좌표는
*다른* 코드인 `E-SCHED-CLOCK-OVERFLOW`입니다 — 오버플로가 오타로 오인되는
일이 없도록 합니다.

`at:`을 **생략**하면 같은 레인의 직전 배치(선언 순서)의 해석된 `at + size`를
물려받습니다 — 단, 그 직전 배치 자신의 `at`/`size`가 (variant 오버라이드로 인해)
루트에 따라 달라지는 경우는 예외이며, 이때는 정적 커서를 계산할 수 없어
`E-SCHED-CURSOR-DYNAMIC`이 됩니다. 실행 순서는 별도의, 더 나중 단계의
정렬입니다: `(presentation, resolved at, declaration index)`.

## `lute play`

```console
$ lute play <PROJECT_DIR>
    --state run.inflow=iroha ...              # 스칼라 시드(루트 선택)
    --fact "..." ...                          # 팩트 시드
    --script routes/iroha.play.yaml           # 루트 스크립트(자체 폐쇄 문법)
    --choose kuhen/coffeeOrder=recommend ...  # 임시 오버라이드, 이벤트 한정
    --auto first                              # 스크립트에 없는 결정의 무인 정책
    --lanes user|all                          # 기본값 user(엄격한 플레이어 시점)
    --steps N                                 # N개 배치가 제시되면 정지
    --coverage <FILE>...                      # 리뷰-갭 코퍼스 리플레이(반복 가능)
    --json
```

프로젝트 전체를 한 번 메모리에 컴파일한 뒤(장면 *과* 퀘스트 종류 모두,
`compile --all`이 쓰는 것과 같은 선언 유니온), 스케줄의 user 레인 배치를
**presentation** 순서로 — `(presentation, resolved at, declaration index)`,
결코 파일 순서나 스토리 틱이 아님 — 걸으며, 각 이벤트의 가드된 variant를
실시간 상태에 대해 재평가하고 `run.*`/`user.*`/`app.*`/`quest.*` 상태와
팩트를 `lute run`의 참조 평가기를 통해 씬 경계 너머로 전달합니다
(`scene.*`는 매 경계마다 항상 초기화됩니다). `presentation: 0`으로 선언된
콜드오픈 플래시백은 스토리 틱상 시간순으로 가장 나중이더라도 정당하게
가장 먼저 재생될 수 있습니다.

루트 선택은 `--state`/`--fact` 시드, `--script <route>.play.yaml`(이 명령
**자체**의 폐쇄 문법 — `kuhen/coffeeOrder: [recommend]`처럼 이벤트로
한정된 id를 갖는 `state:`/`facts:`/`choose:`이며, 그런 형태를 모르는
`lute trace --mock` 파서가 아닙니다), 그리고/또는 임시
`--choose <event>/<id>=<choiceId>[,<choiceId>…]`입니다; 한정하지 않은
(bare) id는 전체 스케줄에서 유일할 때만 허용됩니다. `--auto first`는
스크립트에 없는 모든 결정을 처음 등장뿐 아니라 hub가 다시 제시될 때마다
매번 해소합니다. CLI 플래그는 같은 키가 충돌할 때 루트 스크립트의
`state:`/`choose:`를 이깁니다; `facts:`는 합집합입니다.

각 배치 앞에서: 그 variant들의 가드가 현재 상태에 대해 재평가됩니다 —
정확히 하나만 충족 가능하면 재생, `optional`이 아닌 이벤트에서 0개면
`E-SCHED-VARIANT-GAP`, 2개 이상이면 `E-SCHED-VARIANT-AMBIG`(둘 다 종료
코드 **1**로 정지). 씬의 `after:` 전제조건은 **presentation** 순서로
누적된 visited/completed 집합에 대해 검사됩니다 — 위반 시
`E-SCHED-AFTER-ORDER`, 종료 코드 **1**. 퀘스트 상태는 프로젝트 전역으로
데이터 유니온되지만(`quest.<id>.state`를 참조하는 가드는 타입체크를
통과합니다) `<quest>`/`<on>` 라이프사이클 자체는 씬 경계를 넘어 구동되지
않습니다 — 의도적인 스코핑 결정입니다. 그래서 `completed(...)`/
`active(...)`는 항상 빈 집합에 대해 평가되며, 퀘스트 완료에 인과적으로
게이트된 배치는 언제나 `E-SCHED-AFTER-ORDER`(종료 코드 **1**)로 정지합니다
— 결코 조용히 통과하지 않습니다.

### world 레인과 되감기

user 배치가 완료될 때마다, 방금 커버된 세그먼트 안에 시작 틱이 있고 아직
발화하지 않은 모든 world 배치가 `(at, declaration index)` 순서로 원자적으로
드레인됩니다 — `--lanes user`에서도 마찬가지입니다(world 씬은 여전히
**실행**됩니다 — 상태가 렌더링 여부에 의존해서는 안 되기 때문입니다 — 이
플래그는 트랜스크립트 표시만 제어합니다). presentation이 뒤로 점프하면 새
세그먼트가 시작되며 이는 순전히 시네마틱합니다: 상태는 롤백되지 않고,
아무것도 다시 재생되지 않습니다. 나중에 제시되는 세그먼트보다 "미래에"
재생되는 세그먼트 안에서 드레인되는 world 배치는
`W-SCHED-WORLD-IN-FLASHBACK`입니다 — 정지가 아니라 설계상의 냄새(smell)입니다.

### 트랜스크립트

```
── d6.midnight+0 (tick 564) · user · confinement/iroha ──────────────
::background{location="local_mart_indoor" time="midnight" wait=true}
@iroha{emotion="anxious" voiceKey="iroha-0010"}: 안 열려요!!! 어떡해요!!!!
▷ choice calmDown: [breathe] sortOut        ← chosen: breathe
...
⏪ d6.midnight+6 → d1.late_afternoon+0 (rewind, tick 570 → 48)
── d1.late_afternoon+0 (tick 48) · user · arrival/trunk ──────────────
...
⏩ tick 52 → 60 (fast-forward, empty user lane)
── d1.evening+0 (tick 60) · user · office-scene/trunk ──────────────
...
── end: clock exhausted (tick 672) ──────────────────────────
```

`──` 헤더는 `<tickLabel> (tick <N>) · <lane> · <event>/<variant>`를 나타냅니다;
`⏩`/`⏪`는 비어 있는 user 레인을 건너뛰는 빨리감기와 시네마틱 되감기를
표시합니다; `▷ choice <id>: opt1 [chosen] opt2 ← chosen: opt2`는 결정마다
제시된 모든 선택지를 다시 보여주며, `once` 옵션이 소진될 때마다 hub 재제시
한 줄씩 나타납니다; 스크립트에 없는 결정은 `← INCOMPLETE (no decision)`을
출력하고 정지 메시지가 이벤트, 문서, 종류, id, 그리고 아직 남은 모든
선택지를 이름 붙여 말합니다. `--json`은 `{exit, endReason, scenes: [...]}`를
내보냅니다 — 각 씬은 `event`/`variant`/`doc`/`lane`/`tick`/`tickLabel`/
`endTick`/`stateDelta`/`fastForwardFrom`/`rewindFrom`/`timeMismatch`/
`worldInFlashback`와 `commands`(`lute run --json`이 내보내는 것과 같은
레코드, `addr`로 주소 지정됨)를 갖습니다. 결정적입니다: 같은 시드와
스크립트는 바이트 단위로 동일한 출력을 냅니다.

### 커버리지(`--coverage`)

같은 체인 실행기를 통해 이름 붙은 모든 루트 스크립트를 리플레이하며,
스크립트별 트랜스크립트 출력은 생략하고, 코퍼스 전체가 한 번도 실행하지
않은 모든 배치·variant·hub/choice 선택지를 보고합니다. `--coverage <FILE>`은
반복 가능하며 플래그당 정확히 **파일 하나**만 받습니다 — 플래그 내부에서
글롭이 확장되지 않습니다; 셸 글롭을 직접 넘기면(`--coverage
routes/*.play.yaml`) clap 사용법 오류가 발생합니다. 셸이 이미 그것을 이
명령이 받아들이지 않는 여분의 위치 인자들로 확장해버리기 때문입니다:

```console
$ lute play . --coverage routes/*.play.yaml
error: unexpected argument 'routes/ann.play.yaml' found
```

대신 글롭을 직접 파일마다 하나의 `--coverage`로 셸에서 확장하세요.
`--script`/`--choose`/`--steps`와는 배타적입니다 — 단일 플레이스루 고유의
노브들은 코퍼스 리플레이와 조합되지 않습니다.

## 종료 코드

| 코드 | 일반 `lute play` | `--coverage` |
|---|---|---|
| `0` | 완료 — `::end` 또는 클록 소진. | 완전한 커버리지. |
| `1` | `E-SCHED-*` 코드(`VARIANT-GAP`/`VARIANT-AMBIG`/`AFTER-ORDER`)로 이름 붙은 정지, 또는 컴파일 전에 잡힌 정적 스케줄/루트-스페이스 오류. | 커버리지 갭이 남아 있음. |
| `2` | `schedule.yaml` 없음, 잘못된 루트 스크립트, 프로젝트 어휘 충돌, `--coverage`를 `--script`/`--choose`/`--steps`와 함께 사용, 또는 사용법 오류. | 동일. |
| `3` | **미완료** — `--auto` 정책 없이 스크립트에 없는 결정, 또는 해석 불가능한 참조-런타임 표면(아래). | 코퍼스 스크립트 중 하나 이상이 스스로 미완료로 정지. |

**지원되지 않는 표면**은 각각 조용히 결정되는 대신 정직하게 드러납니다:
`now()`/`validAt(...)`과 해석되지 않은 플러그인 `bridgeResult` 효과는
**미완료(종료 코드 3)**로 정지하며 이벤트와 문서를 이름 붙입니다; 퀘스트
체인 인과 게이트(`completed`/`active`에 대한 `after:`)는 그와 *다른*
등급입니다 — 항상 `E-SCHED-AFTER-ORDER`, **종료 코드 1** — 빈
completed/active 집합이 (비관적이더라도) 정의된 답이기 때문입니다.
벽시계 `<timeline>` 페이싱은 절대 시뮬레이션되지 않으며 그 자체로는
정지를 유발하지 않습니다.

## 진단

정적 오류 열다섯, 런타임 오류 하나, 경고 다섯 — 전체 `E-SCHED-*`/
`W-SCHED-*` 세트입니다.

| 코드 | 의미 |
|---|---|
| `E-SCHED-CLOCK-STRUCTURE` | `ticksPerBucket`/`days`가 `0`이거나 `buckets`가 비어 있음. |
| `E-SCHED-BUCKET-DUP` | 같은 bucket 이름이 두 번. |
| `E-SCHED-LANE-UNKNOWN` | 배치의 `lane:`이 선언된 레인을 가리키지 않음. |
| `E-SCHED-EVENT-DUP` | 같은 `(event, lane)`이 두 번 배치됨. |
| `E-SCHED-VARIANT-FORM` | `doc:`도 `variants:`도 없거나, 둘 다 있거나, `variants:`가 비어 있음. |
| `E-SCHED-SIZE-INVALID` | 해석된 `size`가 `0`. |
| `E-SCHED-AT-PARSE` | 잘못된 `at:` 형태. |
| `E-SCHED-CURSOR-DYNAMIC` | 루트-의존적인 같은 레인 직전 배치 뒤에 `at:` 생략. |
| `E-SCHED-CLOCK-OVERFLOW` | 해석된 구간이 클록을 넘거나, 해석 중 오버플로. |
| `E-SCHED-DOC-MISSING` | 프로젝트-상대 `doc:`이 존재하지 않는 파일을 가리킴. |
| `E-SCHED-DOC-PATH` | `doc:`이 절대 경로이거나 프로젝트 루트를 벗어남. |
| `E-SCHED-VARIANT-GAP` | `optional`이 아닌 배치가 어떤 루트에서 충족 가능한 variant를 하나도 갖지 않음. |
| `E-SCHED-VARIANT-AMBIG` | 어떤 루트에서 두 variant가 동시에 충족 가능함. |
| `E-SCHED-USER-OVERLAP` | exclusive 레인에서 동시-충족 가능한 구간이 겹침. |
| `E-SCHED-GUARD-PARSE` | `when:`/`assume:` CEL 가드 파싱 실패. |
| `E-SCHED-AFTER-ORDER` | (런타임) `after:`가 presentation 순서로 충족되지 않음. |
| `W-SCHED-DOC-UNPLACED` | 씬 문서가 존재하지만 어떤 배치도 참조하지 않음. |
| `W-SCHED-IDLE` | exclusive 레인의 간격이 페이싱 임계값을 초과. |
| `W-SCHED-ROUTESPACE-CAP` | 루트-스페이스 열거가 4096개 조합을 초과 — 스윕 생략. |
| `W-SCHED-TIME-MISMATCH` | 씬의 첫 `::bg time=`이 배치의 bucket과 다름. |
| `W-SCHED-WORLD-IN-FLASHBACK` | world 배치가 되감긴 세그먼트 안에서 드레인됨. |

필드별 전체 키 레퍼런스, 실제 예제, 네이밍 컨벤션은
[`docs/schedule-and-play.md`](https://github.com/journeyWorker/lute/blob/main/docs/schedule-and-play.md)에,
설계 근거는 [스케줄 + 플레이 설계 스펙](https://github.com/journeyWorker/lute/blob/main/docs/superpowers/specs/2026-08-14-lute-schedule-and-play-design.md)에 있습니다.
</content>
