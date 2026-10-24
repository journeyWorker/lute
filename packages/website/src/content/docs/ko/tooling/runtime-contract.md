---
title: 런타임 계약
description: 컴파일된 Lute 아티팩트를 실행하기 위해 게임 엔진이 구현해야 하는 것 — 엔벨로프, 정확한 마이너 IR 및 의미 협상, 엔진 캐퍼빌리티 매트릭스, 플레이백 전 거부 규칙, 씬 및 퀘스트 명령 종류를 순회하는 디스패처 루프입니다.
---

Lute는 완전하고 부작용이 없는 컴파일러입니다. `lute compile <file>`은
`.lute` 문서를 검사하고 JSON 아티팩트로 낮춘 다음 멈춥니다. **CEL을 실행하지 않고,
Datalog 고정점을 계산하지 않으며, 팩트 저장소를 유지하지 않고, 브리지를 호출하지도 않습니다**. 모든
동작은 아티팩트의 반대편에 있는 **엔진**에 있습니다. 이 페이지는 축약한 런타임 계약이며, 전체적인
소스 기반 규격은
[`docs/runtime/`](https://github.com/journeyWorker/lute/tree/main/docs/runtime)에 있습니다.
`lute.engine.yaml` 매트릭스 형식은
[`0.33.0.md §4`](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.33.0.md#4-engine-capability-matrix)에
[`schemas/lute-ir-0.49.schema.json`](https://github.com/journeyWorker/lute/blob/main/schemas/lute-ir-0.49.schema.json)
(JSON Schema draft 2020-12)입니다.

## 런타임 이벤트 계약과 `lute-runtime`

재개 가능한 런타임 API와 JSON 와이어 형식은
[런타임 이벤트 계약](https://github.com/journeyWorker/lute/blob/main/docs/runtime/event-contract.md)을
참고하세요. `lute-runtime` 크레이트는 순수한 통합 표면을 노출합니다. `Runtime::load`로
`compile --project` 번들을 로드하고, `begin`으로 세션을 만들고, `step`으로 진행하며,
`snapshot`/`restore`와 읽기 전용 질의를 사용해 호스트가 검사할 수 있습니다. 렌더링, 페이싱,
브리지 호출, 지급 확정은 호스트가 소유하고, 런타임은 그 효과를 데이터로 반환합니다.

## TypeScript 런타임

TypeScript 호스트는 wasm 런타임 위의 Effect 4 API인
[`@lute-lang/runtime`](https://github.com/journeyWorker/lute/tree/main/docs/runtime/typescript-runtime.md)을 사용할 수 있습니다. Bun 및 브라우저 레이어는 `lute compile --project` 번들을 로드하고, 범위가 지정된 세션과 이벤트 스트림을 노출하며, 타입이 지정된 거부, 스냅숏, 리플레이를 제공합니다. 설치, 완전한 Bun 세션, 브라우저 제공, 프로그램 질의, 오류, 생성된 계약 스키마, `Replay`에 대해서는 [전체 TypeScript 런타임 가이드](https://github.com/journeyWorker/lute/blob/main/docs/runtime/typescript-runtime.md)를 참고하세요.

0.41에서 `Program`에는 `gate(state, occasion, target?)`(`Effect<Gate, InvalidInput>`)와
`def(state, name, args?)`가 추가됩니다. `args`는 선언된 매개변수마다 JSON 스칼라 하나입니다. 닫힌 관문의
발생은 `Errors.OccasionGateClosed`(`E-OCCASION-GATE`)로 실패하며 상태를 바꾸지 않습니다.
`Session.Session`(서비스 타입 `Session.SessionShape`)은 자신이 로드한 `Program`을 `program`으로 노출하므로,
호스트는 번들을 두 번 로드하지 않고 `current`의 상태를 질의합니다. `RuntimeState`는 같은 번들
fingerprint의 모든 `Program`에서 유효하며, 다른 fingerprint의 프로그램에 넘기면 결함(defect)입니다.
`Bundle.index`의 타입은 `Ir.projectIndex`입니다.

Bun 호스트는 선택적 플랫폼 peer도 같은 Effect 릴리스에 정확히 고정해
런타임과 함께 설치해야 합니다:

```sh
bun add @lute-lang/runtime effect@4.0.2 @effect/platform-bun@4.0.2 @effect/platform-node-shared@4.0.2
```

로컬 tarball을 사용한다면 `scripts/pack-local.sh`가 출력한
`file:<runtime-tarball>`로 `@lute-lang/runtime`을 바꾸고, 출력된
`dependencies`와 `overrides` 항목을 붙여 넣으세요. 그 뒤에도
`effect@4.0.2`, `@effect/platform-bun@4.0.2`,
`@effect/platform-node-shared@4.0.2`를 함께 지정합니다.

릴리스되지 않은 체크아웃을 호스트에서 시험하려면 Lute 저장소에서 `bash scripts/pack-local.sh
[--web] [OUT_DIR]`를 실행하세요. 런타임과 player(`--web`이면 `player-web`도)를 `bun pm pack`으로 묶고,
호스트의 `package.json`에 붙여 넣을 `dependencies`와 `overrides` 항목(`file:<tarball>`)을 출력합니다.
호스트는 자신의 `effect` `4.0.2`를 유지합니다. `file:../lute/packages/*` 디렉터리 링크는 쓰지 마세요.
Lute 저장소 자신의 `effect`로 해석되어 호스트에 `effect` 인스턴스가 두 개 생깁니다.
[로컬 도그푸딩](https://github.com/journeyWorker/lute/blob/main/docs/runtime/typescript-runtime.md#local-dogfooding)을
참고하세요.

### 시계 값과 HUD 라벨

`Program.clock(state)`은 숫자 값입니다. day와 0부터 시작하는 slot
인덱스를 반환합니다. 슬롯 이름과 요일 라벨은
`program.view(state).clock`에서 가져오며, `ClockView`는
`{ day, slot, weekday, weekdayLabel, ended, last }` 필드를 가집니다.
Effect 스타일 HUD는 다음처럼 작성할 수 있습니다:

```ts
import { Effect, Option } from "effect"

const hud = Effect.gen(function* () {
  const position = yield* program.clock(state)
  const world = yield* program.view(state)
  if (Option.isNone(position) || world.clock == null) return "No clock"
  const clock = world.clock
  return `${clock.weekdayLabel ?? `Day ${clock.day}`} — ${clock.slot ?? `slot ${position.value.slot}`}`
})
```

정렬에는 숫자 위치를 사용하고, 표시 문자열에는 `ClockView.slot`과
`ClockView.weekdayLabel`을 사용하세요.
 
## 0.49 런타임 추가 사항

**플러그인 스탬프는 중첩된 `stamps` 객체 하나입니다.** 작성자가 디렉티브나 콘텐츠 줄에 쓴 횡단 플러그인
[`stampAttrs`](/ko/plugins/manifests/#교차-속성stampattrs) 값은 더 이상 레코드에 평탄화되지 않습니다.
스탬프가 붙는 모든 명령(`jump`와 `barrier`를 제외한 전부)은 `timing`, `provenance`, `source` 뒤에
`"stamps": { "<attr>": <value>, … }`를 가집니다. 작성된 스탬프가 없으면 객체는 생략되므로 스탬프가 없는
레코드는 0.48.1과 바이트 단위로 같습니다. 따라서 레코드 키와 같은 이름의 스탬프(`tag`, `speaker`, `text`,
`label` 등)도 더 이상 충돌하지 않습니다. 코어 스탬프 멤버 `timing`, `provenance`, `source`는 그대로
평탄화되며, 예약된 여덟 이름도 바뀌지 않습니다.

같은 객체가 모든 플러그인 경로에서 호스트에 전달되며, `attrs`와 분리되고 절대 `attrs`에 병합되지 않습니다.
정상 및 중단된 `plugin` 이벤트 레코드, `awaitBridge`, 브리지 요청, 이를 담는 중단된 스냅숏이 각각
선택적 `stamps`를 가지며 비어 있으면 생략됩니다. 스테이징 레코드는 코어 필드 `attrs`를 유지하며 스탬프는
섞이지 않습니다.

**콘텐츠 줄의 `timing.wait`.** 줄은 `wait`(불리언)를 받습니다. 이는 줄 레코드의 `timing.wait`로
컴파일되며 작성하지 않으면 생략됩니다. 줄은 항상 자신의 진행을 기다립니다. `wait`는 그 줄 뒤부터 다음
줄 앞까지의 스테이징 큐가 그 줄을 기다리는지를 말합니다. `true`이거나 없으면 기다립니다(기존과 같음).
`false`이면 줄이 표시되는 동안 시작합니다. 다음 줄은 여전히 이전 줄을 기다립니다. 레퍼런스 플레이어는
이를 따릅니다. `timing.wait === false`인 줄 큐는 포크되며, 스케줄러는 다음 `line` 큐 앞, `presentation` 큐 앞(대화창을 비우므로 줄과 겹치면 안 됨), 배치가
정착하기 전에 이를 조인합니다. 줄의 `delay`와 `duration`은 여전히 `E-UNKNOWN-ATTR`입니다.

### 타입이 지정된 브리지를 통한 자유 텍스트 입력

Lute에는 자유 텍스트 구성물이 없습니다. "플레이어에게 무언가를 입력하게 한다"의 호스트 쪽 절반은
결과가 `string`이고 상태 슬롯에 쓰이는 브리지입니다:

```yaml
# bridge/prompt.yaml
bridgeCapabilities:
  - service: prompt
    operation: ask
    replay: recorded
    result:
      - { name: answer, type: string }
```

```yaml
# directives/ask.yaml
directives:
  - name: ask
    layer: bridge
    attrs:
      - { name: slot, required: true, type: { slotId: { namespace: scene.ask } } }
    semantics: [ "writes.sceneState", "bridgeCall" ]
    state:
      declares:
        - { scope: scene, path: [ask, { fromAttr: { name: slot, slotType: localId } }], shape: textAnswer }
    effects:
      writes:
        - { scope: scene, path: [ask, { fromAttr: { name: slot } }, answer], value: { fromBridgeResult: answer } }
    bridge: { service: prompt, operation: ask }
```

런타임은 `awaitBridge`로 중단됩니다. 호스트는 호출의 `attrs`(여기서는 `slot`)로 자체 텍스트 프롬프트를
열고 `bridgeResult { request, fields: { "answer": "<플레이어가 입력한 값>" } }`로 답합니다. 런타임은
`string`을 검증하고 선언된 쓰기를 적용하므로, 호출 뒤의 줄은 일반 상태를 통해 답을 읽습니다. Lute는
입력 채널을 보지 못하며, 답은 다른 브리지 결과와 같은 데이터입니다. `.test.yaml`을 포함한 전체 프로젝트는
[`docs/examples/free-text-input/`](https://github.com/journeyWorker/lute/tree/main/docs/examples/free-text-input)입니다.

## 0.44 런타임 추가 사항

**엔딩에 속한 쓰기.** 런타임이 `ended`로 응답한 뒤에는 `newRun`이나 `outsideRun` 계기만 받습니다. 그래서 끝난 런에 속한 호스트 쓰기(엔딩 메타데이터, 최종 점수)는 종료를 일으키는 입력이 쓰기를 실을 수 있다면 그 입력의 `writes`에 실어야 합니다. `raiseOccasion`, `advanceClock`, `bridgeResult`, `hostWrite`가 그런 입력이며, 이들의 쓰기는 종료 전환보다 먼저 적용됩니다(`{"type": "raiseOccasion", "occasion": "dawn", "writes": {"state": [{"path": "run.endedBy", "value": "dawnSeven"}]}}`). `choose`, `timeout`, `worldEvent`는 쓰기를 싣지 않으므로, 이들이 일으키는 엔딩은 그 상태를 스토리(끝나는 갈래의 `::set`)나 그 전에 보낸 `hostWrite`에 기록합니다. `newRun`의 쓰기는 새로 시작하는 런의 초기값이며, 그 런에서 끝난 런의 `run.*` 값은 `prev.run.*`로 읽힙니다. 엔딩 뒤의 `hostWrite`는 여전히 상태를 바꾸지 않고 `E-RUNTIME-BUSY`로 거절되며, 이제 메시지가 그 쓰기가 어디에 실려야 하는지 알려 줍니다.

## 0.45 런타임 추가 사항

읽기 전용 질의(`candidates`, `evaluate`, `def`, `gate`, `view`)는 모든 await
(`idle`, `awaitChoice`, `awaitBridge`, `ended`)에서 상태를 바꾸지 않고 답합니다. TypeScript
`Session`에서 입력이 실행 중일 때 질의는 마지막으로 커밋된 상태를 읽습니다. choice나
bridge가 대기 중일 때는 해당 응답만 받습니다. 호스트는 `candidates(occasion, target?)`를
질의해 eligible인 `entry`를 고르고, artifact에서 본문을 찾아 모든 레코드가 placeholder 없는
`line`일 때에만 프레젠테이션 밖에서 background bark를 표시할 수 있습니다. 이 static bark는
읽음 표시, `once` 소비, writes를 기록하지 않습니다. 기록이 필요하거나 동적인 bark는 이야기가
`idle`로 돌아온 뒤 raise해야 합니다. 동시 프레젠테이션은 계획하지 않습니다. 자세한 내용은
[0.45 background-bark 호스트 패턴](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.45.0.md#32-the-pattern)과
[전체 이벤트 계약](https://github.com/journeyWorker/lute/blob/main/docs/runtime/event-contract.md)을 참고하세요.

## 0.42 런타임 추가 사항

모든 스테이징 레코드 — `bg`, `music`, `sfx`, `vfx`, `actor`, `camera`, `cg`, `video`, `sequence` — 는 `plugin` 레코드의 `attrs`처럼 명령의 해석된 필드를 `attrs`로 담습니다. 작성한 값과 lowering이 채운 기본값이 들어가고(`cg`의 `display: "show"`, actor의 선언된 기본 `anchor`), 없는 선택 필드는 생략되며 `attrs`는 항상 객체입니다. 레코드 엔벨로프(`kind`, `position`), `timing`, provenance, 중첩된 플러그인 `stamps`는 원래 자리에 있으며(스탬프는 `attrs`에 섞이지 않습니다), 호스트는 아티팩트에서 명령을 찾지 않고 스테이징 레코드를 읽습니다. `volume`은 여전히 프로젝트 `volume` 도메인의 멤버이며, 플레이어가 [`volumes`](/ko/player/bindings/)로 gain으로 바꿉니다.

| kind | `attrs` |
| --- | --- |
| `bg` | `location?`, `time?`, `assetId?` |
| `music` | `playback?`, `stop?`, `mood?`, `volume?`, `assetId?` |
| `sfx` | `sound?`, `assetId?` |
| `vfx` | `type`, `label?`, `transition?` |
| `actor` | `character`, `anchor?`, `action?`, `exit?`, `emotion?`, `costume?`, `posReset?`, `preload?` |
| `camera` | `focus?`, `framing?`, `move?`, `transition?` |
| `cg` | `assetId`, `display`, `layout?` |
| `video` | `assetId`, `display` |
| `sequence` | `name` |

## 0.41 런타임 추가 사항

- **관문 거절은 거부입니다.** `raisedWhen`이 거짓인 `raiseOccasion`, 또는 `terminal`이 성립한 뒤
  `outsideRun`이 아닌 occasion의 발생은 상태를 바꾸지 않고(입력 자신의 쓰기와 payload 포함)
  `E-OCCASION-GATE`로 거부되며, 메시지는 `lute play`의 거절 문구입니다. 판정할 수 없는 관문은 여전히
  `incomplete`로 정지합니다. 읽기 전용 질의 `gate(occasion, target?)`가 미리 `{ open, unresolved? }`를 답합니다.
- **`def(name, args)`**는 매개변수가 있는 인덱스 def를 평가합니다: 선언된 매개변수마다 JSON 스칼라 하나가
  `param` 노드에 바인딩되며, 개수·타입 불일치는 `E-RUNTIME-INPUT`입니다. `evaluate`는 `param` 노드를 받지 않습니다.
- **`awaitBridge`**는 `attrs`(호출의 속성, `occasion.target` 바인딩)와 `results`(콘텐츠가 읽는 결과 필드와
  선언 타입)를 가지며, 답은 그 타입으로 검증됩니다. `bridgeResult`는 `hostWrite`와 같은 `writes`를 가질 수 있고,
  호출의 선언된 효과 뒤에 적용되어 같은 프레젠테이션의 이후 줄이 읽습니다. 모든 `plugin` 레코드는 `attrs`를
  가지며, 차단하지 않는 plugin 레코드에는 쓰기가 없습니다.
- **`advanceClock { writes }`**는 시계가 움직이기 전에 쓰기를 적용하고 정착합니다(그때 `terminal`이 성립하면
  시계는 머물고 await는 `ended`). 이동의 발생들이 그 쓰기를 읽고, `clock` 이벤트는 마지막에 옵니다.

## 0.40 런타임 추가 사항

0.40 계약은 persistent presentation `stage`(background, music, actor, CG,
video)를 `world.stage` 스냅숏에 담고 읽기 전용 `stage()` 질의를 제공합니다.
중단된 choice나 bridge도 await 시점의 stage를 `continuation.stage`에 담습니다.
복원할 때 레코드를 다시 재생하지 말고 이 값으로 presenter를 재구성합니다.

시간 제한 branch는 `awaitChoice.menu.timeout`과 `menu.default`를 채웁니다.
호스트가 벽시계를 측정해 `{\"type\":\"timeout\",\"request\":N}`을 보냅니다.
런타임은 일치하는 시간 제한 choice에서만 이를 받아 선언된 default option을
적용합니다. 다른 await에서 보내면 `E-RUNTIME-TIMEOUT`으로 거부되며, 잘못된
request는 계속 `E-RUNTIME-REQUEST`입니다. 시간 제한 branch는 `timeout`과
`default`를 함께 선언해야 합니다.

:::caution[권한은 아티팩트 경계에서 멈춥니다]
[기능 권한](/ko/tooling/capability-permissions/)은 컴파일 전에 금지된 저작 효과를 거부합니다. 이것은 런타임 샌드박스가 아니며 엔진의 브리지, 영속성, 네트워크, 파일 시스템 또는 보상 효과를 허가하지도 않습니다. 엔진 호스트는 여전히 신뢰할 수 있는 아티팩트만 로드하고 모든 실제 효과를 허가해야 합니다. [런타임 보안 가이드](https://github.com/journeyWorker/lute/blob/main/docs/runtime/capability-permissions.md)를 참고하세요.
:::

## Lute가 하는 일과 엔진이 하는 일

| Lute(컴파일 시점) | 엔진(런타임) |
| ------------------- | ---------------- |
| 문서를 정적으로 검사하고, 오류가 있으면 출력을 거부합니다. | 아티팩트를 신뢰합니다 — 깨끗하게 컴파일되었습니다. |
| 상태 스키마를 초기화/타입 테이블로 접습니다. | 그 테이블에서 상태를 초기화하고 등급별 수명을 소유합니다. |
| **닫힌 §8.4 프로필 안에 텍스트가 있는** 모든 CEL 가드를 이식 가능한 `expr` AST로 낮추고, 나머지는 CEL 텍스트로 `cel`에 남깁니다. | 실제 상태에 대해 가드를 **평가합니다**. |
| 팩트, `assert`/`retract` 델타, Datalog 규칙을 **데이터**로 내보내고, 규칙이 층화되고 안전한지 증명합니다. | 팩트 저장소에서 **최소 모델**(최소 고정점)을 **계산합니다**. |
| 퀘스트, 목표, `<on>` 핸들러를 **선언**으로 내보냅니다. | `start`/`fail`/목표 완료로부터 퀘스트 생명주기를 **도출합니다**. |
| 플러그인 브리지 호출과 그 상태 쓰기 바인딩을 해석합니다. | **호출하고** 효과를 적용합니다. |
| 타임라인 클립을 스케줄링하고 쓰기 경합이 없는지 증명합니다. | 스케줄을 재생하거나(또는 트랙을 동시에 실행하고) 배리어를 준수합니다. |

`holds()`와 `count()`는 저자가 작성할 수 있는 §8.4 CEL 프로필 안에 있지만
`expr` AST에는 의도적으로 **없습니다**. 따라서 팩트를 질의하는 가드는 `expr` 형제 없이
`cel` 텍스트만으로 엔진에 도달합니다(`option.when`, `arm.test`,
`set.value`). 그러므로 엔진은 단순한 AST 순회기가 아니라 CEL 평가기를 반드시 갖춰야
합니다. `lute run`의 모듈 문서도 같은 내용을 말합니다. 즉, 구조화된 `expr` AST가 의도적으로
생략한 `holds`/`count` 팩트 질의 함수를 포함해 모든 슬롯을 `cel` 텍스트에서 해석합니다.
`visited('<scene id>')`(dsl 0.21.0)도 마찬가지입니다. 모든 조건 슬롯에서 사용할 수 있으며, 해당
씬이 이 세이브에서 제시되면 참이 됩니다 — 엔진이 이미 `after:`를 위해 유지하는 visited 집합입니다 —
그리고 `cel` 텍스트만으로 전달됩니다.

## identity 마이그레이션 (0.36.6)

컴포넌트 스코프는 저작된 `instance` 키를 사용합니다:
`::use{component="hearthFire" instance="opening"}`은 순번이 아닌
`hearthFire#opening`을 생성합니다. 조인 또는 세이브를 게시하는 프로젝트는
`identity.requireStable: true`를 선택합니다. 이렇게 하면 컴포넌트 인스턴스 누락 및 줄 코드에
대한 진단이 활성화됩니다. `lute tag`는 기존 저작 키를 보존하면서 두 항목을 모두 보충하고,
`use-001`, `use-002`, …를 할당합니다.

실행 IR과 프로젝트 인덱스의 선택적 `identityRenames` 배열은 엔진이 사용하는 유일한 이름 변경
마이그레이션 입력입니다. 각 항목은 정규 `NodeKey` 문자열을 사용하는 `{from, to}`입니다.
`lute diff`는 원장 일치 시 `renamed`를 보고하고, 저작된 매핑이 없으면
`unmappedIdentity`를 보고합니다. 텍스트나 위치를 근거로 추측하지는 않습니다.

```lute check
---
kind: scene
id: runtime-identity
---
## Opening
@narrator{code="intro"}: Ready.
```

핵심은 다음과 같습니다. Lute는 *형태와 구조*를 증명하고, 엔진은 *평가와 효과*를 제공합니다.
Lute의 정적 분석도 한계를 정직하게 드러냅니다 — 도달 가능성은 선언된 `after:` 경로 아래에서
보수적으로 계산되고, 관계형 팩트 게이트는 사람 검토 경계 뒤에서 **Unknown** 판정을 내리며,
`lute trace`는 결정적인 목업 기반 경로 하나를 순회할 뿐 모든 경로의 증명이 아닙니다.

## 엔벨로프

모든 아티팩트는 고정된 엔벨로프로 시작합니다(`crates/lute-compile/src/ir.rs`의
`ExecutionIr` 구조체).

| 필드 | 의미 |
| ----- | ------- |
| `kind` | `"scene"` \| `"quest"` — 먼저 읽으며 `meta`의 형태를 선택합니다. |
| `lute` | 언어 버전 고정값(런타임에 정보를 제공하기 위한 값). |
| `irVersion` | **가장 먼저 게이트할** IR 스키마 버전입니다. |
| `capabilitySnapshot` | 플러그인 캐퍼빌리티 스냅숏 해시입니다. 불일치하면 거부합니다. |
| `requiredSemantics` | 컴파일러가 도출한, 아티팩트가 사용하는 정렬된 의미 ID입니다. 저자는 이 목록을 편집할 수 없습니다. |
| `meta` | 씬 메타데이터 또는 퀘스트 메타데이터입니다. |
| `state` | 접힌 초기화/타입 테이블입니다. |
| `entities` / `enums` / `relations` / `seedFacts` / `rules` | 선언된 어휘입니다(비어 있으면 생략). |
| `commands` | 평탄하고 순서가 있는 명령 스트림입니다. 모든 레코드에 `kind`, `family`, `position`이 있습니다. |
| `prereqEdges` | 참고용 원시 그래프 간선입니다(비어 있으면 생략). |
| `sections` | 문서의 `##` 섹션이며, 각각 `{ section, heading, id? }`입니다(heading 또는 id가 하나도 없으면 생략). |
| `seasons` / `gates` / `terminal` / `terminalPersists` | 선택적 시계 및 계기 제어입니다. |

`requiredSemantics`는 하나의 아티팩트에서 낮춰진 기능들의 정렬된 합집합입니다. 프로젝트 인덱스에도 문서 아티팩트의 정렬된 합집합으로 같은 필드가 들어 있으므로, 프로젝트 전체 `check`, `play`, 엔진은 모든 문서를 열기 전에 협상할 수 있습니다. 완전한 레지스트리와 트리거 매핑은 [0.33.0 제안서 §2–§3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.33.0.md#2-semantic-id-registry)에 있으며, 이 페이지에서는 의도적으로 그 레지스트리를 반복하지 않습니다.

### 명령 레코드

모든 명령 레코드에는 필수 필드 세 개와 선택적 형제 필드 네 개가 있습니다.

| 필드 | 의미 |
| ----- | ------- |
| `kind` | 레코드 종류입니다. 알 수 없는 `kind`는 치명적 오류입니다. |
| `family` | `kind`의 규범적 그룹화입니다(아래 참조). `kind`보다 먼저 디스패치에서 읽습니다. |
| `position` | 빌드에 국한된 레코드 실행 위치입니다([위치](#위치) 참고). |
| `timing` | 선택적 `{ wait?, duration?, delay?, at?, timeline? }`입니다. 모든 멤버가 없으면 생략됩니다. `line`은 `wait`를 가질 수 있습니다([0.49](#049-런타임-추가-사항) 참고). |
| `provenance` | 컴파일러가 합성한 레코드에 붙는 선택적 `{ by, explanation }`입니다. |
| `source` | 선택적 소스 위치입니다. |
| `stamps` | 스탬프가 붙는 모든 명령에 있는 플러그인 `stampAttrs` 값의 선택적 객체(`{ "<attr>": <value> }`)입니다. 비어 있으면 생략되며, `jump`와 `barrier`에는 없습니다. |

| `family` | 종류 |
| -------- | ----- |
| `content` | `line` |
| `staging` | `bg`, `music`, `sfx`, `vfx`, `actor`, `camera`, `cg`, `video`, `sequence` |
| `state` | `set`, `assert`, `retract` |
| `control` | `choice`, `match`, `hub`, `jump`, `end`, `barrier` |
| `declaration` | `quest`, `on`, `entry`, `accept`, `beat` |
| `plugin` | `plugin` |

`timing.wait`는 불리언이고, `duration`, `delay`, `at`은 유한한 음이 아닌 초 단위 값입니다. `timeline`은 레코드가 방출된 `<timeline>`의 0부터 시작하는 순번이며, 초가 아닌 순번입니다. `barrier`에는 `timing`이 없습니다. 해당 레코드의 `timeline` 순번과 `at` 초는 직접 필드입니다. 타이밍은 `timing`에서만 읽으세요.

스테이징 값은 프로젝트가 선언한 도메인의 불투명한 멤버입니다:
`actor.anchor`/`action`/`emotion`/`costume`, `camera.framing`/`move`/
`transition`(`focus`도 포함하며, 이는 cast 참조입니다), `cg.layout`, `music.playback`,
`sequence.name`입니다. Lute는 숫자 카메라 변환을 합성하지 않습니다. `cg`와 `video`에는 항상
해석된 `display`가 들어갑니다(기본값은 `show`). `sequence` 레코드는 외부 시네마틱을
이름으로 가리키며, `timing.wait: true`이면 엔진은 진행하기 전에 해당 시네마틱의 제시를
완료합니다. 액터 퇴장은 파생된 `actor.exit: true`입니다(`action` 도메인의 `exits:`에서
유래). false일 때는 생략됩니다.

`line`에는 모든 역할에 대해 `role`(`dialogue`, `narration`, `mono`, `os`, `vo`),
`speaker`, `text`, `lineId`, `voiceKey`가 들어갑니다. `voiceKey`는 녹음이 존재한다는 약속이
아니라 결합 키입니다. `text`는 저작된 텍스트에서 파생된 평문입니다. 인라인 수정자는 제거하고,
이스케이프는 디코딩하며, `{{…}}` 마커는 유지하고 마커마다 순서대로 `placeholders` 항목이 하나씩 있습니다.
다만(lute 0.47.1) 리터럴 `{{`(저작 시 `\{{`)는 `\{{`로 쓰고, `{{` 바로 앞의 리터럴 백슬래시는 모두
두 번 씁니다. segment 실행, `texts`, `localeSegments`도 같은 인코딩을 씁니다. 왼쪽에서 오른쪽으로
읽으며, `{{` 바로 앞에 백슬래시가 *n*개 이어지면 ⌊*n*/2⌋개의 백슬래시를 뜻하고, *n*이 홀수이면 그
`{{`는 리터럴 텍스트(한 단위로 소비)이며 짝수이면 다음 마커를 엽니다. `run.a = 3`일 때
`Lit \{{run.a}} and {{ run.a }}.`는 `Lit {{run.a}} and 3.`로 렌더링됩니다. placeholder를 직접
치환하는 엔진은 이 읽기 규칙을 적용해야 합니다. `\{{`가 있는 줄은 loc `sourceHash`가 새로 바뀌므로
다시 내보내세요(0.47.0에서 내보낸 것은 `W-L10N-STALE`입니다). 인라인 수정자를 사용하는 줄에는 제시
실행 단위인 `segments`도 들어갑니다: `{ text, styles?, rate? }` 텍스트 실행과
`{ pause }` 리프(초)입니다. 수정자가 없는 줄에는 `segments`가 없습니다. 로캘 형식은
[지역화된 텍스트](#현지화-텍스트)를 참고하세요.

```json
{
  "kind": "music",
  "family": "staging",
  "position": "001-0400",
  "playback": "play",
  "assetId": "theme",
  "timing": { "duration": 1.5 }
}
```

## 활성화와 평가

엔진은 라이브 상태로 만든 중첩 맵을 대상으로 각 CEL 슬롯을 평가합니다.

- 선언된 슬롯으로 가는 경로의 선언된 모든 루트와 중간 맵이 존재합니다.
- 예약되지 않은 슬롯은 유효한 값(쓰기, 시드 또는 기본값)이 있을 때만 존재하며, 값은 선언된 CEL 타입을 유지합니다.
- 예약된 엔진 경로는 항상 예약된 기본값으로 존재합니다. IR이 선언하는 모든 퀘스트에 대해 `quest.<id>.state`와 `quest.<id>.failedBy`는 `"unset"`이고, 각 목표의 `quest.<id>.objectives.<oid>.done`과 `.failed`는 `false`이며, 모든 로어 엔트리의 `entry.<id>.read`와 `.everRead`도 `false`입니다.

존재 여부에는 `has(path)`를 사용하세요. `is`가 있는 `<match>` 갈래는 유효한 값이 없는
단순 경로에 대해 그 값이 정확히 `"unset"`일 때만 충족됩니다. 다른 `is` 값은 확실히
거짓입니다. 다른 갈래는 정상적으로 평가하며, 평가 오류가 나면 해당 갈래는 충족되지 않습니다.

문서마다 아티팩트 하나가 생성됩니다. 프로젝트의 엔진은 명령 스트림을 연결하는 것과 정확히
같은 방식으로 로드하는 모든 아티팩트에서 `relations` / `rules` / `seedFacts` /
`entities` / `enums` / `prereqEdges`를 **합집합**합니다.

스키마가 그대로인 동안 언어 `0.9.0`에서 **내용**이 이동한 필드는 `enums`가 유일합니다. 이 필드는 항상 저자가 `enums:` 블록에서 선언하는 도메인을 담아 왔습니다. 콘텐츠 어휘 멤버가 프로젝트가 선언할 대상이 된 이후에는 `emotion`, `action`, `anchor`, `mood`, `volume`, `musicPlayback`, `vfxType` 도메인도 같은 배열을 통해 전달됩니다. 프로젝트가 이를 인라인으로 선언하거나 `uses:`/`extends:`를 통해 선언하면 이를 내보냅니다. 반면 멤버가 플러그인의 `enums` export에서 오면 프로젝트는 `enums`를 전혀 내보내지 않습니다. 플러그인 어휘는 문서별 데이터가 아니라 캐퍼빌리티 표면(`capabilitySnapshot`에 접힘)이기 때문입니다. 어느 경우든 엔진이 이미 합집합하는 데이터이므로 엔진에 새로운 요구 사항은 없습니다 — `enums` 이동은 필드를 추가하거나 이름을 바꾸거나 옮기지 않았습니다.
이 이동은 IR 형태를 변경하지 않았습니다. (관련 없는 `meta.plugin` 키는 같은 릴리스에서 추가되었습니다. 변경 이력의 [IR 0.10.2에서 변경된 내용](#ir-0102에서-변경된-내용)을 참고하세요.) 컴파일러 의미를 담은 멤버(`action`의 `exits:`, `anchor`의 `default:`)는 컴파일 시점에 해석되어 사라지고 직렬화되지 않습니다. 엔진은 런타임에 멤버 의미가 필요하지 않습니다.

## 버전 협상과 엔진 통합

모든 호스트는 플레이백 세션을 열기 전에 다음 순서로 협상합니다.

1. **정확한 MAJOR.MINOR IR 게이트.** 1.0 이전에는 모든 마이너 버전이 실행 IR 형태를 깨뜨릴 수 있으므로, 엔진은 정확한 `irVersion` major.minor 계열을 고정합니다. `0.49` 엔진은 `0.49.*` 아티팩트만 받아들이고 `0.48.*` 및 `0.50.*`는 거부합니다. 패치 처리는 엔진 정책입니다. 1.0부터는 릴리스된 메이저 버전의 정책을 적용합니다.
2. **의미 캐퍼빌리티 게이트.** 변경할 수 없는 `lute.engine.yaml` 매트릭스를 로드하고 모든 아티팩트의 `requiredSemantics` ID를 `supportedIds`와 비교합니다. `engine`, `irVersion`, 고유한 `supportedIds`는 필수이며, `version`과 `description`은 설명용입니다. 전체 레지스트리는 [0.33.0 제안서 §2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.33.0.md#2-semantic-id-registry)에 있습니다.

매트릭스 예시는 다음과 같습니다.

```yaml
engine: chat-text-engine
irVersion: "0.49.0"
supportedIds:
  - lute.core/1
  - lute.quest.lifecycle/1
  - lute.quest.rewards/1
  - lute.knowledge.facts/1
  - lute.knowledge.rules/1
  - lute.knowledge.temporal/1
version: "1.4.0"
description: "Text-only chat client: no staging, timeline, or clock"
```

`lute run --engine <file>`과 `lute play --engine <file>`은 두 게이트를 모두 수행합니다.
ID가 누락되면 플레이백 전에 종료 코드 **2**와 함께
`E-ENGINE-SEMANTICS`로 거부합니다. 명령, 조건, 에셋 또는 브리지는 실행되지 않습니다.
형식이 잘못된 매트릭스에는 `E-ENGINE-MATRIX`를 사용합니다. 매트릭스의 정확한
major.minor 계열을 벗어난 아티팩트(예를 들어 위 `0.49.0` 매트릭스에 대한
`0.41.*` 아티팩트)에는 `E-ENGINE-IR-VERSION`을 사용합니다. 알 수 없는 아티팩트 또는
레지스트리 ID에는 `E-SEMANTICS-UNKNOWN`을 사용합니다. `lute run`은 제거된 0.36 필드를
여전히 담은 아티팩트를 `E-IR-REMOVED-FIELD`로 거부합니다.

`lute check --engine <file>`과 `lute check --project <DIR> --engine <file>`은 플레이백 없이
저작 시점의 협상을 수행합니다. 누락된 캐퍼빌리티는 낮춰진 구성의 소스 범위, 의미 ID,
엔진 이름과 함께 `E-CHECK-ENGINE-SEMANTICS`로 보고됩니다. 프로젝트 형식은 프로젝트 인덱스
합집합을 검사하고 기여한 각 문서/범위를 나열합니다. 플러그인 호환성은 여전히 정확한
`capabilitySnapshot` 검사로 유지되며 이 매트릭스에 포함되지 않습니다.

참조 실행기의 내장 매트릭스 이름은 `reference`이며, 현재의 모든 의미 ID를 지원합니다.

### Match 갈래 축약형 의미론 (dsl 0.32.0 §7)

각 match 갈래에는 `test`와 `target`이 있습니다. `<when is="…">`로 저작된 갈래에는
축약형 텍스트를 담은 의미 IR 필드 `is`도 있습니다. `test=` 갈래는 `is`를 생략합니다.
슬롯의 `authored` 필드는 진단 전용이며 갈래가 축약형에서 왔는지 추론하는 데 사용해서는
안 됩니다.

match 주제가 유효한 값이 없는 단순 상태 경로일 때 `"unset"`이 아닌 `is`를 가진 갈래는
확실히 충족되지 않고, `is: "unset"`은 충족됩니다. 다른 갈래는 정상적으로 평가하며,
평가 오류가 나면 충족되지 않습니다.

### 0.41의 `project.index.json`

인덱스 형태는 Rust 인덱스 타입에서 생성됩니다. IR 스키마의 `projectIndex`,
`indexDocument`, `indexEntry`, `indexBeat`, `indexOccasion`,
`indexStateDomain`, `indexDef`, `fieldType` 정의는 `lute-contract-schema`가
만들며, 아티팩트 형태(`enumEntry`, `celSlot`, `exprNode`, `clockDecl`, …)는
다시 정의하지 않고 참조합니다. 바뀐 키는 세 개입니다(spec 0.41.0 §4).

- **`bridgeResults`**: 각 브리지 결과 필드가 `"number"`/`"string"` 대신
  `occasions[*].payload`와 같은 형태의 선언 타입을 가집니다 — `"bool"`, `"int"`,
  `"double"`, `"string"`, `{"enum": [...]}`, `{"domain": "<name>"}`, ….
- **`enums[].labels: { <member>: "<text>" }`**: 병합된 `enums:` 선언의 긴 형식
  `labels:`이며, 인덱스와 아티팩트 행에 똑같이 실립니다. 선언이 없으면 생략됩니다.
- **`defs`**: 플러그인 또는 `uses:` 스키마 def마다 이름을 키로 한 행
  `{ cel, expr, authored?, params?, host? }`입니다. `params`는 인자 순서대로
  `[{ name, type }]`을 나열하고, 각 파라미터는 `expr`에서 `{"param": "<name>"}`로
  나타나며, `host: true`는 호스트가 읽는 def를 표시합니다. 스칼라가 아닌 파라미터가
  있거나, 본문이 낮춰지지 않거나, 두 문서에서 본문이나 파라미터 목록이 다른 def는
  빠집니다. `param` 노드는 다른 곳에는 나타나지 않습니다.

0.47부터 인덱스에는 선택적인 **`contentLang`**도 들어갑니다. 매니페스트의
`defaults.contentLang`, 즉 프로젝트 텍스트의 원본 언어이며 매니페스트에 없으면 생략됩니다.
플레이어는 이를 로드한 번들의 원본 로케일로 쓰며, 문서의 `meta.contentLang`보다 먼저 봅니다.

### IR 0.37.0에서 변경된 내용

**깨끗한 표면 전환: 이름이 바뀐 종류와 필드, 새 종류 하나, 제거된 필드.** 0.36 계열의
엔진은 0.37 아티팩트를 거부해야 하며, 이전 이름은 별칭으로도 허용되지 않습니다. 엔진을
0.37로 옮기려면(dsl 0.37.0 §2.2, §5) 다음을 적용하세요.

- **엔벨로프:** `capabilityVersion`은 `capabilitySnapshot`입니다(프로젝트 인덱스와 `lute context --json`도 같은 키를 사용합니다). `shots`는 `sections`가 되며, 각 항목은 `{ section, heading, id? }`입니다. 여기서 `id`는 저작된 `## Heading {#id}` 접미사입니다.
- **모든 레코드:** 새 필수 `family`를 가집니다([명령 레코드](#명령-레코드) 참고). 공통 `addr`은 같은 형식의 `position`이 되고, 평탄화된 타이밍 필드는 하나의 `timing` 객체로 이동하며, `provenance.injected`는 제거됩니다.
- **종류:** `background`는 `bg`, `sprite`는 `actor`, `cut`은 `cg`가 되며, `sequence`가 새로 추가됩니다. `music.action`은 `music.playback`이 되고, `cg.action`과 `video.action`은 `display`가 됩니다. `cg.full`은 도메인 값인 `layout`이 되고, `vfx.vfxType`은 `type`이 됩니다. `music.track`과 `sfx.name`은 제거됩니다. 카메라의 숫자 필드 `zoom`/`moveX`/`moveY`/`shake`/`reset`/`easing`은 도메인 값인 `framing`, `move`, `transition`으로 대체됩니다.
- **줄:** `monologue`, `offscreen`, `voiceover` 역할은 각각 `mono`, `os`, `vo`가 됩니다. 모든 줄에는 `voiceKey`가 들어가며, 인라인 수정자가 있는 줄에는 `segments`가 추가되고 번역에는 `localeSegments`가 추가됩니다.
- **제어:** choice 및 hub 옵션의 `label`은 `text`가 되고(로캘 맵은 `texts`), `recordKey`는 `selectionKey`가 됩니다.

스키마 파일은 `lute-ir-0.37.schema.json`입니다.
## IR 변경 이력

*이전 계열에서 엔진을 마이그레이션하는 사람을 위해 보존한 이력입니다.* 아래 각 항목은 해당 릴리스 자체의 게이트 규칙을 설명합니다(일부는 정확한 major.minor 게이트보다 앞섭니다). 현재 엔진은 위의 [버전 협상](#버전-협상과-엔진-통합)을 따릅니다.

### IR 0.30.0에서 변경된 내용

**필드가 추가·이름 변경·재타입되지 않았으며, 이제 이름에 `-`를 포함하거나 숫자로 시작할 수 있습니다.** 0.29 아티팩트를 로드하는 엔진은 다음과 같이 경로와 조건을 읽는다면 0.30 아티팩트도 로드할 수 있습니다(dsl 0.30.0 §3).

- **경로 필드는 표준 점 표기 문자열입니다.** `set` 명령의 `path`, `state` 항목의 `path`, `expr` 노드의 `path` / `isSet` / `has`, `{{…}}` 플레이스홀더의 `path`는 이름을 `.`으로 연결합니다: `quest.zero-coke-001.state`, `run.visits.lab-b2`, `run.visits.001`. 세그먼트에는 `-`가 포함되거나 숫자로 시작할 수 있지만 `.`은 포함하지 않으므로 `split('.')`으로 세그먼트를 복원할 수 있습니다. 해당 문자열 또는 그 세그먼트에 대해 상태를 조회하고, 이를 CEL로 다시 파싱하지 마세요.
- **조건은 작성된 형태로 전달됩니다.** 조건의 `raw`는 작성자가 쓴 CEL이며, 식별자가 아닌 이름은 JavaScript처럼 따옴표 인덱스로 접근합니다: `quest["zero-coke-001"].state`, `run.visits['lab-b2'] >= 2`. 이는 중첩 맵에 대한 일반 CEL입니다 — `m["k"]`와 `m.k`는 같은 항목을 읽으므로, 상태 맵에 대해 `raw`를 평가하는 엔진에는 새로 필요한 것이 없습니다. 이러한 조건의 이식 가능한 `expr`에는 표준 경로가 담깁니다(`{"path": "run.visits.lab-b2"}`).
- **팩트 인자는 이름입니다.** `raw`의 `holds('at', ["lab-b2"])`는 `assert` 명령이 `{"relation": "at", "args": ["lab-b2"]}`로 쓰는 팩트를 묻습니다. 따옴표는 조건의 표기일 뿐 이름의 일부가 아닙니다. 시드 팩트와 규칙 헤드·본문의 구조화된 항도 같은 방식으로 따옴표 없는 이름을 담습니다.

스키마 파일은 릴리스 계열마다 이름이 변경됩니다: `lute-ir-0.29.schema.json`은 이제 `lute-ir-0.30.schema.json`이며, 이름 패턴(season, occasion, `share` 키, `series`, 문서 id)은 `-`가 포함되거나 숫자로 시작하는 이름을 허용합니다.

### IR 0.29.0에서 변경된 내용

**선택적 필드 하나이며, 이름 변경은 없습니다.** 새 명령 `kind`는 없고 필드가 이름 변경·재타입·삭제되지 않았으므로, MAJOR 전용 게이트 아래에서 0.28 아티팩트를 로드하는 엔진은 0.29 아티팩트도 변경 없이 로드합니다(dsl 0.29.0 §5).

- **아티팩트와 `project.index.json`의 `terminalPersists: true`**: 모든 `terminal:` 선언이 `persists: true`를 포함한 긴 형식일 때 `terminal` 뒤에 직렬화됩니다. 종료 상태는 의도적으로 런을 넘어 유지됩니다(로그라이크의 영구 엔딩): `terminal`이 성립하면 이 런이나 이후 런에서 계기를 발생시키지 않습니다 — 새 런이 게임을 다시 열지 않습니다(지속될 수 있는 조건은 새 런에서도 유지되는 상태를 읽으므로 여전히 성립할 수 있습니다). 거짓이면 생략됩니다.

스키마 파일은 릴리스 계열마다 이름이 변경됩니다: `lute-ir-0.28.schema.json`은 이제 `lute-ir-0.29.schema.json`입니다.

### IR 0.28.0에서 변경된 내용

**퀘스트 계층 필드 세 개의 이름이 변경되었고 나머지는 모두 추가되었습니다.** 새 명령 `kind`는 없고 필드가 재타입·삭제되지 않았으므로, MAJOR 전용 게이트 아래에서 0.27 아티팩트를 로드하는 엔진은 0.28 아티팩트도 로드합니다. — 단, 이름이 변경된 필드를 읽는 엔진은 새 이름으로 읽어야 합니다(dsl 0.28.0 §6).

- **`ObjectiveEntry.visibleWhen`** (이전 `when`): 가시성만 제어합니다 — 성립하는 동안 목표를 표시하며 `done`을 제한하지 않습니다.
- **`RewardEntry.outcome`** (이전 `on`): 퀘스트 수준 보상에서 새로운 `failed` 전환이 발생할 때만 지급되며, 값은 오직 `"failed"`입니다.
- **퀘스트의 `prereqEdges` 행은 `follows`**(이전 `after`)를 담습니다: 퀘스트 그래프 메타데이터이며 퀘스트를 제한하지 않습니다(`start`가 제한합니다). 씬과 번들 비트 행은 `after`를 유지합니다.

새 동작과 선택적 필드는 다음과 같으며, 작성되지 않았으면 각각 생략됩니다.

- 아티팩트와 `project.index.json`의 **`outsideRun: [occasion]`**(이름순): `terminal`이 성립한 뒤에도 이 계기들을 발생시킵니다(타이틀 화면, 런 사이의 갤러리). 게임이 끝나면 나머지 모든 계기는 닫힌 상태로 유지됩니다.
- **`HubCmd.return`**: 허브의 `<return>` 세그먼트 위치입니다. 종료가 아닌 옵션의 세그먼트가 끝날 때마다 허브를 다시 판정하고 제시하기 전에 실행합니다 — 첫 메뉴 전이나 `exit` 옵션 뒤에는 실행하지 않습니다. 옵션 대상과 마찬가지로 그 앞의 세그먼트 범위를 정합니다.
- **`clock.raiseAtStart: true`**: 런의 첫 위치에서 시계 슬롯 계기(문자열 형식이면 그 문자열, 맵이면 `slot`)와 맵의 `dayStart`를 직접 발생시킵니다. 이 위치에서는 advance가 중지되지 않습니다. `false`이면 생략됩니다.
- 예약된 시계 읽기 값은 `clock.day`와 `clock.slot`(시계의 day 및 slot 경로 별칭)이며, 유한 시계에서는 **`clock.ended`**도 제공합니다: 시계를 끝내는 advance 전에는 `false`, 해당 advance의 정착부터 새 런이 런 계층 시계를 다시 시작할 때까지 `true`입니다.
- **`labelForms: { <member>: { start?, indefinite? } }`**는 `entities[]`와 `state[]` 항목에 제공되며, 플레이스홀더 형식 **`cardinalWord`**(`one` … `twenty`, 그 이상은 숫자), **`capitalize`**, **`start`**, **`indefinite`**도 `reserved`와 `occasionTarget` 플레이스홀더에 제공됩니다. `capitalize`는 렌더링된 텍스트의 첫 글자를 대문자로 만듭니다. `start`는 멤버의 `start` 형식을 렌더링하고, 없으면 대문자로 시작합니다. `indefinite`는 멤버의 `indefinite` 형식을 렌더링하고, 없으면 첫 글자에 따라 `a` / `an`, 공백, 텍스트를 사용합니다. `plural` 형식에서는 `#word` / `#Word`가 (대문자로 시작하는) 수사로서의 숫자입니다.
- **관계 `tier: "season:<name>"`**: 해당 관계의 팩트는 그 시즌이 열릴 때마다 시드 팩트로 되돌아갑니다.
- **`authored`**: `@def` 확장이 seam 조건(`gates[].raisedWhen`, `terminal`, `seasons[].live`)을 변경했을 때 `cel` 옆에 표시됩니다. 메시지용일 뿐이며 `cel`을 평가합니다.
- **`spentBy` 래치**: 이제 `spentBy` 비트의 `once`는 `"none"`이 아니라 해당 비트의 기간(작성되지 않으면 `run`)입니다 — 이러한 비트는 제시 시 소진하지 않습니다(`spentBy` 아래 참조).
- **`forKind` 비트는 멤버마다 `once`를 소진합니다**: 각 멤버의 제시가 별도로 계산되며, `for` 항목의 첫 읽기 쓰기는 런에서 각 멤버의 첫 읽기에 적용됩니다.
- **`occasion.target`을 통한 쓰기**: 아래의 *`occasion.target`을 통한 쓰기*를 참조하세요.
- **seam이 결정되는 시점**: 계기가 발생할 때 `terminal`과 계기의 `raisedWhen`을 판정합니다. `terminal`을 성립시키는 `judge: before` 판정은 해당 계기 자체의 비트를 닫지 않습니다. `select: sequence` 발생은 앞선 비트가 재생된 뒤, 각 비트의 차례 직전에 다시 판정합니다.

### IR 0.27.0에서 변경된 내용

**필드가 추가되고 기존 필드에 새 값이 추가되었습니다.** 필드의 이름 변경·재타입·삭제는 없고 새 명령 `kind`도 없으므로, MAJOR 전용 게이트 아래에서 0.26 아티팩트를 로드하는 엔진은 0.27 아티팩트도 로드합니다. 주기는 다음과 같습니다(dsl 0.27.0 §5).

- **`once: "week"`**: `BeatIr`(씬의 `meta.beat`), `BeatCmd`, `EntryCmd`, `project.index.json` 비트 행에 사용되며, 제시 시점부터 다음 시계 주가 시작될 때까지 소진됩니다. 주 *n*은 `(day - 1) div week.length`가 *n*인 날을 포함하므로 `clock.weekday`가 `week.first`로 돌아올 때 주가 시작됩니다. 시계가 `week:`를 선언한 경우에만 출력됩니다.
- **`once: "season:<name>"`**: 같은 레코드에 사용되며, 제시 시점부터 `<name>` 시즌이 다시 열릴 때까지 소진됩니다.
- **`spentBy: {cel, expr, authored?}`**: `BeatIr`, `BeatCmd`, `EntryCmd`에 제공됩니다(index 비트 행에는 `raw`만 제공). 비트가 제시되어서가 아니라 이 조건으로 소진됩니다. 모든 퀘스트 정착 시(종류 또는 `for` 비트의 멤버별, `occasion.target` 바인딩 시)와 비트 판정 시 관찰합니다. 한 번 성립하면 `once` 기간 동안 비트가 소진됩니다 — `once`가 `run`이거나 없으면 `run`(새 런이 초기화), `user`이면 절대 소진되지 않고, `day` / `slot` / `week`이면 처음 성립한 기간에 시계가 머무는 동안, `season:<name>`이면 해당 시즌이 다시 열릴 때까지 유지됩니다 — 조건이 거짓으로 바뀌어도 마찬가지입니다. `spentBy` 비트를 제시해도 아무것도 소진되지 않습니다. 작성되지 않았으면 생략됩니다.
- **`QuestCmd.rearm: {cel, expr, authored?}`**: 플레이스루의 모든 퀘스트 정착 시 관찰하며, 첫 관찰은 기준선입니다. false→true가 될 때마다 퀘스트를 `unset`으로 되돌립니다(완료되지 않은 목표, 삭제된 `failedBy`, 잊힌 기한). 성립하는 `start`는 같은 정착에서 퀘스트를 활성화합니다. 작성되지 않았으면 생략됩니다.
- **`QuestCmd.tier: "season:<name>"`**: 시즌이 다시 열릴 때 퀘스트를 `unset`으로 되돌리며, 새 런에서는 `"run"` 퀘스트가 됩니다.
- 아티팩트(`clock`, `gates`, `terminal` 뒤)와 `project.index.json`의 **`seasons: [{ name, live: {cel, expr, authored?} }]`**: `live`는 `@def` 확장 이후의 값입니다. 시즌의 `live`가 false→true가 되면 모든 `season.<name>.*` 값을 `prev.season.<name>.*`에 복사하고, `season.<name>.*`을 선언된 기본값으로 초기화하며, `once: "season:<name>"` 비트의 제시 기록을 지우고, 해당 시즌 계층 퀘스트를 `unset`으로 되돌립니다. 시즌은 서로 독립적이며 겹칠 수 있습니다. `prev.season.*`은 `prev.run.*`처럼 읽기 전용 미러입니다.

텍스트(dsl 0.27.0 §7):

- **`entities[].labels: { <member>: "<text>" }`**: 종류 멤버의 표시 텍스트이며, 공유 멤버에는 하위 종류와 조상의 레이블을 사용합니다. `entityKind` K의 `occasionTarget` 플레이스홀더는 K의 `labels[member]`로 렌더링하고, 멤버가 cast id이면 cast의 `name:`을, 그 외에는 id를 사용합니다. `{ domain: K }` 또는 `{ entity: K }` 상태 경로는 해당 `state[].labels`와 같은 맵을 전달하며, `path` 플레이스홀더는 이미 이를 렌더링합니다. 없으면 생략됩니다.
- **`forms: [one, other]`를 사용하는 플레이스홀더 `format: "plural"`**: 수가 1이면 `one`, 그 외에는 `other`를 렌더링하며, 각 형식의 `#`는 숫자로 치환됩니다(`{{n:plural(# lamp|# lamps)}}` → `3 lamps`). 현지화 엔진은 두 영어 형식에서 자체 복수 범주를 선택할 수 있습니다.

유한 시계(dsl 0.27.0 §4):

- **`clock.last: { day, slot? }`** 또는 **`clock.days: N`**은 선언된 그대로입니다(`days: N`은 `last: { day: N }`이며 `slot`이 없으면 해당 날의 마지막 슬롯). 시계는 그 위치에서 멈춥니다. 목적지가 이를 지나가는 advance는 마지막 위치로만 이동하고(그 과정에서 통과하는 매 자정마다 `dayEnd` / `dayStart`를 발생시킴), 마지막 날의 `dayEnd`를 한 번 발생시키며 `slot` 계기는 발생시키지 않고 시계를 종료합니다 — 새 런이 day 및 slot 경로를 초기화할 때까지 더 advance하지 않습니다. 절대 종료되지 않는 시계에서는 둘 다 생략됩니다.

엔진 seam(dsl 0.27.0 §4):

- **`gates: [{ occasion, raisedWhen: { cel, expr, authored? } }]`**(최상위, 계기순, `@def` 확장): 게이트가 성립하는 동안에만 `occasion`을 발생시키며, `occasion.target`을 발생 대상 멤버로 읽습니다(`room.office` → `office`). 시계가 만드는 발생(`raise.slot`, `dayStart`, `dayEnd`)의 게이트가 거짓이면 발생하지 않습니다. 시계는 계속 이동합니다. 검사기는 게이트 아래에서 해당 계기의 모든 비트를 판정했으므로, 절대 제시하지 않을 비트도 작성자에게 보고합니다. 어떤 계기도 게이트를 선언하지 않으면 생략됩니다.
- **`terminal: { cel, expr, authored? }`**(최상위): 성립하는 순간 게임이 종료되며 새 런이 시작될 때까지 어떤 계기도 발생시키지 않고 시계도 advance하지 않습니다. 여러 스키마의 선언은 `||`로 결합됩니다. 선언이 없으면 생략됩니다.
- **디렉티브 팩트 효과**: `kind: "plugin"` 레코드에는 **`retracts`**와 **`asserts`**가 각각 `[{ relation, args }]` 형식으로 포함될 수 있으며, 호출의 속성은 이미 치환되어 있습니다(`_`는 무엇이든 일치). 레코드 자체의 쓰기 뒤에 이를 적용하고, retract를 먼저 실행하며, 일반 assert/retract 경로를 사용합니다(`reserved` 관계도 포함하며, 쓰기는 엔진 자체의 쓰기입니다). 비어 있으면 생략됩니다. 로어 항목의 효과 전용 디렉티브는 항목의 `set` 레코드처럼 항목을 처음 읽을 때만 적용됩니다.

계기에 의해 바인딩되는 멤버(dsl 0.27.0 §3):

- **패턴 인자 및 패밀리 인덱스로서의 `occasion.target`**: 종류 비트(및 `forKind` 비트)의 CEL에서 팩트 질의 패턴은 `occasion.target`을 인자로 받을 수 있고(`holds('owned', [occasion.target])`), `per:` 패밀리는 이를 인덱스로 사용할 수 있습니다(`user.bond[occasion.target]`). 바인딩된 멤버로 치환합니다. 패턴 인자는 해당 멤버 id가 되고, `F[occasion.target]`은 `F.<member>`를 읽습니다. 검사기는 모든 멤버 인스턴스가 올바른 타입임을 입증했습니다.
- **`occasion.target`을 통한 쓰기**(dsl 0.28.0 §3): 같은 비트에서 `set` 레코드의 `path`는 `[occasion.target]`으로 끝날 수 있고(`run.count[occasion.target]`: `run.count.<member>`에 쓰기), `assert` / `retract` 인자와 `plugin` 레코드의 필드 값, 효과 경로 인덱스(`effects[].path`), 팩트 인자(`asserts` / `retracts`)도 `occasion.target`이 될 수 있습니다. 읽기와 동일하게 쓰기를 적용하기 전에 바인딩된 멤버로 치환합니다. 컴포넌트 인자는 이런 방식으로 전달되지 않습니다. `occasion.target`의 멤버마다 하나의 `match` arm으로 컴파일됩니다.
- **`forKind: { kind, members }`**: `BeatIr`(씬의 `meta.beat`), `BeatCmd`, `EntryCmd`, `project.index.json` 비트 행에 제공됩니다. 대상 없는 `select: sequence` 계기에 목록 순서대로 멤버마다 한 번 응답합니다. 발생 시 각 멤버의 자격을 `occasion.target`에 해당 멤버를 바인딩한 채 판정하고, 자격 있는 각 멤버마다 한 번씩 제시하며 실행 중에도 `occasion.target`을 바인딩합니다. `once`는 멤버가 아니라 비트를 소진합니다. 작성되지 않았으면 생략됩니다.
- **계기 페이로드**: `payload: { <field>: <type> }`로 선언한 계기는 각 발생에 타입이 지정된 값을 전달합니다. 비트를 판정하기 전에 `occasion.payload.<field>`를 발생 값에 바인딩하고, 비트가 제시된 뒤 바인딩을 해제합니다. 페이로드는 한 번의 발생 동안만 유지됩니다. 아티팩트의 CEL과 `path` 플레이스홀더는 상태처럼 이를 읽습니다(`occasion.payload.copies`).

### IR 0.22.0에서 변경된 내용

**추가 필드 두 개, 새 예약 플래그 하나, 확장된 계기 선언, 새 identity 값이 추가되었습니다.** 필드의 이름 변경·재타입·삭제는 없고 새 명령 `kind`도 없으므로, MAJOR 전용 게이트 아래에서 0.21 아티팩트를 로드하는 엔진은 0.22 아티팩트도 로드합니다. 새 필드를 무시하는 엔진은 0.21 의미론으로 실행합니다. 즉 모든 퀘스트가 런을 넘어 유지되고 모든 항목 비트가 반복됩니다. 작성된 대로 실행하려면 다음을 구현하세요.

- **`QuestCmd.tier`**는 `<quest tier="run">`에서 `"run"`이고 기본 `user` 계층에서는 생략됩니다. 하나의 퀘스트 문서가 여러 퀘스트를 선언할 수 있으므로 `meta`가 아니라 각 `quest` 레코드에 포함됩니다. 런이 시작되면 모든 런 계층 퀘스트를 완료되지 않은 목표와 함께 `unset`으로 되돌린 뒤, 어떤 평가 시점에서든 생명주기를 정착합니다. 사용자 계층 퀘스트는 상태를 유지합니다. [quest-lifecycle.md](https://github.com/journeyWorker/lute/blob/main/docs/runtime/quest-lifecycle.md)를 참조하세요.
- **`EntryCmd.once`**는 이를 작성한 항목 비트에서 `"run"` 또는 `"user"`이며, 그 외에는 생략됩니다(0.21처럼 반복 가능). `ProjectIndex.beats` 항목 행에도 같은 값이 포함되며, 씬 행에는 항상 `once`가 있었습니다. `"run"` 항목 비트는 `entry.<id>.read`가 true인 동안 자격이 없고, `"user"` 항목 비트는 `entry.<id>.everRead`가 true인 뒤 자격이 없습니다.
- **`entry.<id>.everRead`**는 항목마다 새로 추가되는 예약 사용자 계층 `bool`이며 기본값은 `false`입니다. 첫 읽기 후 `entry.<id>.read`를 설정할 때마다 함께 설정하고, 런 시작 시 재설정하지 않습니다. 아티팩트의 `state` 표에는 나열되지 않지만(`entry.<id>.read`는 `entry:<id>` provenance와 함께 나열됨), 콘텐츠는 raw CEL과 `expr` AST(`{ "path": "entry.<id>.everRead" }`)에서 어떤 경로처럼 읽습니다. 로드하는 각 항목에 대해 하나를 유지하세요.
- **계기 대상 도메인**: capability snapshot에서 계기의 `target`은 `false`, `true`, 또는 `{ "prefix", "entity" }`입니다. 도메인의 경우 검사기는 해당 계기의 모든 비트 대상이 프로젝트 엔티티 종류 `entity`의 `<prefix>.<member>`임을 보장하므로, 그 형태의 대상으로 계기를 발생시키세요. 계기 선언은 아티팩트에 포함되지 않습니다. 도메인은 `capabilitySnapshot`을 변경하며 `true`와 `false`는 0.21 스탬프를 유지합니다.
- **`owner: engine`**: `state:` 선언은 해당 경로의 작성자로 엔진을 지정할 수 있습니다. 검사기는 그 경로 또는 그 아래 필드에 대한 콘텐츠 `::set`을 거부하므로(`E-ENGINE-OWNED-WRITE`), 정상 아티팩트의 `set` 레코드는 이를 대상으로 하지 않습니다. 엔진이 씁니다. 해당 경로의 아티팩트 `state` 항목은 변하지 않습니다(path, type, default). `lute context --json`은 소유자를 `"owner": "engine"`으로 보고합니다.
- **Identity 값이 이동합니다.** 기본 `voiceKey` 템플릿은 `{prefix}.{speaker}-{code}`입니다(이전에는 `{speaker}-{code}`). 컴포넌트에서 확장된 줄은 `{prefix}.{component}#{n}` 아래에서 생성되며, `n`은 호스트가 해당 컴포넌트에 `::use`한 횟수를 1부터 셉니다. 따라서 기본 템플릿에서 `lineId`는 `{prefix}.{component}#{n}.{speaker}_{code}`, `voiceKey`는 `{prefix}.{component}#{n}.{speaker}-{code}`이고 중첩된 `::use`는 세그먼트를 하나 더 추가합니다. 필드 형태는 변하지 않았지만 값이 달라지므로 0.21 id를 키로 한 음성 및 현지화 표를 다시 만드세요. 이전 키로 녹음한 오디오가 있는 프로젝트는 `identity: { voiceKey: "{speaker}-{code}" }`를 고정합니다([Frontmatter & profiles](/ko/language/frontmatter-and-profiles/) 참조).

### IR 0.10.2에서 변경된 내용

**추가 필드 하나이며 필드의 이름 변경·재타입·삭제는 없습니다.** `meta`(`sceneMeta`와 `questMeta` 모두)에 `plugin`이 추가됩니다. 이는 플러그인이 소유하고 검사기가 검증한 최상위 frontmatter 키이며 `{ "<key>": <json value> }`로 접습니다. 문서에서 플러그인 소유 키를 작성하지 않았으면 완전히 생략됩니다.

```json
{ "meta": { "character": "marina", "season": 1, "episode": 2,
    "episodeId": "s01ep02", "plugin": { "cast": { "ana": { "costume": "school" } } } } }
```

키는 활성 플러그인이 선언했고 작성된 값이 해당 플러그인의 선언된 스키마를 독립적으로 통과할 때만 존재합니다 — 이는 검사기가 이미 `E-FRONTMATTER-SCHEMA` 적용 여부를 판단하는 데 사용하는 것과 같은 조건이므로 검사기가 거부할 값은 `meta.plugin`에 도달할 수 없습니다. 규범적 소속 규칙은 [plugin-system 0.0.4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/plugin-system/0.0.4.md)를 참조하세요.

**알 수 없는 필드 무시 규칙에 따라 게이트는 넓어지지 않습니다.** `0.10.2`는 `0.10.0`/`0.10.1`과 major.minor `0.10`을 공유하고 `plugin`은 진정으로 새로 추가된 객체 키이므로, 이미 `0.10.x` 아티팩트를 받아들이고 읽지 않는 키를 무시하는 엔진은 변경 없이 계속 작동합니다. 단순히 파싱하는 것이 아니라 공개 JSON Schema를 엄격히 검증한다면, `lute-ir-0.10.schema.json`의 `sceneMeta`/`questMeta`는 이제 `plugin`을 타입이 지정된 속성으로 허용합니다(그 외에는 여전히 `additionalProperties: false`). 게이트된 major.minor가 이동하지 않았으므로 파일 이름은 유지됩니다.

### IR 0.10.1에서 변경된 내용

*아직 `0.10.1` 계열을 사용하는 사람을 위해 보존한 이력입니다.* **형태에는 아무것도 없습니다.** IR `0.10.1`은 IR `0.10.0`과 형태가 동일했습니다 — 필드의 추가·이름 변경·이동·재타입이 없고 새 명령 `kind`도 없습니다. 릴리스가 보이는 모든 축을 다시 정렬했기 때문에 숫자만 이동했으며, `0.10.1`을 받아들이도록 게이트를 넓히는 데는 `0.10.0`이 이미 요구한 것 외에는 비용이 없었습니다. 둘 다 major.minor `0.10`을 공유하기 때문입니다.

### IR 0.10.0에서 변경된 내용

*아직 `0.10.0` 계열을 사용하는 사람을 위해 보존한 이력입니다.* **필드 이름 하나가 변경되었으며, `0.8.0` 이후 첫 형태 변경입니다.** 주입 provenance 스탬프의 `reason`이 **`explanation`**이 됩니다.

```json
{ "by": "auto-pose-reset",
  "explanation": "pre-loading `vesna`'s first emotion `level` seen ahead of the entrance" }
```

이전 이름은 동의어가 아니라 충돌이었습니다. `end.reason`은 **작성자가 쓰고 그 값으로 분기하는 불투명한 작성자 토큰**입니다. 반면 이 필드는 **컴파일러가 작성한 사람이 읽을 수 있는 영어**로, 합성한 레코드가 존재하는 이유를 설명하며 이 값을 기준으로 디스패치하지 않습니다. 이름만 같고 나머지는 전혀 공통점이 없는 두 키가 바로 이름 변경으로 제거할 함정이며, `explanation`은 `by` 옆에서 올바르게 읽힙니다.

형태에서 달라지는 다른 것은 없습니다. 필드 추가·재타입, 새 명령 `kind`, 제약 변경은 없습니다.

**위 게이트가 규범적이며, 이번 버전 상승에는 편집이 하나가 아니라 두 개 필요합니다.** IR `0.9`를 구현하는 엔진은 `0.10`이 더 새로운 major.minor이므로 `0.10.0`으로 표시된 아티팩트를 **거부해야 합니다**. 변경 사항은 다음과 같습니다.

> **게이트를 넓혀 `0.10`을 허용하고**, 주입 provenance 스탬프를 읽는다면 **`reason`을 `explanation`으로 이름 변경하세요.** 그 외에는 없습니다 — 디스패치할 새 `kind`도, 동작 차이도 없습니다.

JSON Schema에 대해 검증한다면 `lute-ir-0.10.schema.json`을 가리키세요. `0.9` 파일은 함께 보존됩니다.

아티팩트 **내용**도 비용 없이 변경됩니다. clip의 `at`, `duration`, `delay`, barrier의 `at`은 여전히 초 단위 JSON 숫자이지만 이제 정수 밀리초에서 계산되므로, 커서에서 파생된 `1.2`가 `1.2000000000000002`로 직렬화되지 않습니다. capability snapshot도 변경됩니다 — `W-INJECT-CONFLICT`가 코드 집합에서 빠지고 11개 코드가 추가되었습니다.

### IR 0.9.0에서 변경된 내용

*아직 `0.9` 계열을 사용하는 사람을 위해 보존한 이력입니다.* **형태에는 아무것도 없습니다.** IR `0.9.0`은 IR `0.8.0`과 형태가 동일했습니다 — 필드의 추가·이름 변경·이동·재타입이 없고 새 명령 `kind`도 없습니다. Lute의 [버전 정책](https://github.com/journeyWorker/lute/blob/main/docs/versioning.md)이 모든 릴리스에서 보이는 모든 축의 번호를 다시 정렬하기 때문에 숫자만 이동했으며, 게이트를 넓혀 `0.9`를 허용하는 것이 전체 마이그레이션이었습니다. 실제로 이동한 것은 아티팩트 **내용**입니다: `enums`가 프로젝트 콘텐츠 어휘 도메인을 담기 시작했고([엔벨로프](#엔벨로프) 참조), core의 어휘가 비면서 capability snapshot이 변경되었습니다. 둘 다 이미 존재하던 필드의 새 값입니다.

### IR 0.8.0에서 변경된 내용

*아직 `0.8` 계열을 사용하는 사람을 위해 보존한 이력입니다.* 스키마 파일은 minor 상승과 함께 `schemas/lute-ir-0.7.schema.json` → `schemas/lute-ir-0.8.schema.json`으로 이름이 변경되었습니다. 소비자에게 중요한 변경은 세 가지입니다.

- **`end`는 새 명령 `kind`입니다.** 알 수 없는 kind 규칙에 따라 IR 0.7만 구현한 엔진은 이를 포함한 아티팩트를 **거부해야 합니다** — 레코드를 통과시킬 수 없습니다. `end`는 순회를 종료하는데 통과시키면 작성자가 도달 불가로 표시한 콘텐츠를 실행하게 되기 때문입니다. 이것이 종료가 플러그인 디렉티브가 아니라 core kind인 이유입니다. 플러그인 디렉티브는 `kind: "plugin"`으로 낮춰지므로 이전 엔진이 아무 문제 없이 건너뛸 수 있습니다.
- **섹션 제목 표와 로케일 맵은 추가 전용 선택 필드**이므로, 알 수 없는 필드 무시 규칙에 따라 `end` 레코드가 없는 0.8 아티팩트는 0.7 엔진도 여전히 로드합니다.
- **위치 폭 불변식이 새로 생겼으며**, 이미 소비 중인 아티팩트의 바이트를 바꿀 수 있는 유일한 변경입니다. 아래를 참조하세요.
## 위치

모든 명령 레코드는 `position`을 가지며, 그 값은 `"{section}-{(index + 1) * 100}"`(예: `"001-0300"`)입니다. 앞부분은 1부터 세는 `##` 섹션 번호이고, 뒷부분은 해당 섹션 안에서 레코드의 순서입니다. 이 값은 **컴파일할 때마다 다시 생성**되며, 빌드에 한정된 실행 위치이지 식별자, 현지화 키 또는 세이브 키가 아닙니다. 안정적인 콘텐츠 결합에는 `lineId` / `voiceKey`를 사용하고, 섹션의 안정적인 식별자는 `sections`에 있는 선택적 `id`입니다.

두 부분 모두 문서에서 계산한 너비로 0을 채웁니다. 섹션은 최소 `3`, 인덱스는 최소 `4`이며, 문서에 더 큰 값이 필요하면 너비가 늘어납니다. 이 너비는 **아티팩트 전체에서 균일**합니다. 따라서 다음 보장을 신뢰할 수 있습니다.

> 하나의 아티팩트 안에서 모든 `position`의 길이는 같습니다.
> 따라서 `position`의 사전식 순서는 실행 순서와 같습니다.

위치를 전달하는 필드는 정확히 다음과 같습니다. 모든 명령의 `position`; `choice.converge`와 `choice.options[].target`; `hub.converge`, `hub.return`, `hub.options[].target`; `match.converge`, `match.otherwise`, `match.arms[].target`; `jump.target`; `quest.objectives[].body`(빈 본문이면 `null`); 그리고 `on.body`, `entry.body`, `beat.body`입니다.

## 스트리밍 스냅숏 교체

[`lute compile --stream`](/ko/tooling/continuation-compiler/)은 승인된 연속 입력 단위마다 완전한 일반 아티팩트 스냅숏을 내보냅니다. 두 번째 IR이나 패치 언어를 정의하지 않습니다. `appendFrom`은 이전 스냅숏의 명령 수이며, 새로 추가된 배열 영역만 식별합니다. 런타임 PC가 아니며 그 영역의 모든 레코드를 실행하라고 요청하는 값도 아닙니다.

스냅숏이 도착하면 불변 프로그램 이미지를 교체하고 `position -> command index` 맵을 다시 만듭니다. **숫자** 명령 커서, 현재 상태 값, 팩트, 선택된 제어 흐름 스택, 호스트가 소유한 효과/멱등성 기록은 유지합니다. 새로 선언된 상태 슬롯만 초기화합니다. 아티팩트 객체가 교체되었다는 이유만으로 기존 기본값이나 시드 팩트를 다시 적용해서는 안 됩니다.

명령을 추가하면 아티팩트 전체에서 균일한 위치 padding이 넓어질 수 있으므로 위치 조회를 다시 만드는 것이 필수입니다. 컴파일러는 타입이 있는 위치와 제어 대상을 숫자 `(section, index)` 의미로 비교하므로 이 서식만의 변경은 허용합니다. 이미 내보낸 명령이나 상태 엔트리에 의미상 변경이 있으면 `E-STREAM-PREFIX-CHANGED`로 거부합니다.

일반 디스패처는 여전히 branch, match, hub, jump 경로를 선택합니다. 현재 스트림 프런티어에 도달하면 장면이 완료되었다고 표시하지 말고 다른 스냅숏을 기다립니다. 완료는 컴파일러 EOF `finish`의 성공 또는 저자가 작성한 일반 `end` 명령에 뒤따릅니다. EOF와 `end`는 서로 다릅니다. `end`는 stdin이 닫히기 전에 런타임 실행을 멈출 수 있지만, EOF는 `end`를 합성하지 않습니다.

## 디스패처

`commands` 배열은 이미 실행 순서입니다. 제어 흐름 필드인 `jump.target`, choice/hub 옵션의 `target`과 `converge`, match arm의 `target`/`otherwise`/`converge`는 모두 [위치](#위치)입니다. `position → index` 맵을 통해 프로그램 카운터로 순회하면서 `family`와 `kind`에 따라 디스패치합니다.

```ts
// Every CEL slot carries its verbatim source under its own key — `option.when`,
// `arm.test`, `set.value` — and the lowered `expr` AST ONLY when that CEL is
// inside the closed §8.4 profile. A relational fact query or a `visited()` read
// carries raw text alone.
const evalSlot = (raw, expr, state, facts) =>
  expr !== undefined ? evalExpr(expr, state) : evalCel(raw, state, facts);

const index = new Map(artifact.commands.map((c, i) => [c.position, i]));
let pc = 0;
while (pc < artifact.commands.length) {
  const cmd = artifact.commands[pc];
  let next: string | null = null; // null ⇒ fall through to pc + 1

  switch (cmd.kind) {
    // family "content"
    case "line":       present(cmd, state); break; // placeholders; segments when present
    // family "staging": opaque domain members; honor cmd.timing
    case "bg":    case "music":  case "sfx": case "vfx":   case "actor":
    case "camera": case "cg":    case "video": case "sequence":
      stage(cmd, cmd.timing); break;

    // family "state"
    case "set":     writeState(state, cmd.path, cmd.op, evalSlot(cmd.value, cmd.expr, state, facts)); break;
    case "assert":  facts.assert(cmd.relation, cmd.args); break;
    case "retract": facts.retract(cmd.relation, cmd.args); break;

    // family "control"
    case "choice":
    case "hub": {
      const opt = pickOption(cmd, state);   // per option: evalSlot(o.when, o.expr, …)
      next = opt ? opt.target : cmd.converge; break;
    }
    case "match": {
      const arm = cmd.arms.find(a => truthy(evalSlot(a.test, a.expr, state, facts)));
      next = arm ? arm.target : (cmd.otherwise ?? cmd.converge); break;
    }
    case "jump":    next = cmd.target; break;
    case "end":     finish(cmd.reason); return;  // terminates the walk
    case "barrier": joinTimeline(cmd.timeline, cmd.at); break;

    // family "declaration" and family "plugin"
    case "quest":   registerQuest(cmd); break;
    case "on":      registerHandler(cmd); break;
    case "accept":  acceptQuest(cmd.quest); break; // activates it iff still `unset`
    case "plugin":  callBridgeAndApplyEffects(cmd, state); break;

    default: throw new UnknownCommandKind(cmd.kind); // version gate: hard error
  }

  pc = next === null ? pc + 1 : index.get(next)!;
}
```

전체 장면 및 퀘스트 명령 집합은 스물세 가지 종류입니다: `line`, `bg`, `music`, `sfx`, `vfx`, `actor`, `camera`, `cg`, `video`, `sequence`, `set`, `assert`, `retract`, `choice`, `match`, `hub`, `jump`, `end`, `barrier`, `quest`, `on`, `accept`, `plugin`. 로어 아티팩트에는 `entry`와 `beat` 선언 헤드도 추가되며, 엔진은 이를 재생하지 않고 조회합니다([로어 엔트리](/ko/language/lore-entries/)).

`accept` (dsl 0.21.0)는 씬 쪽의 `::accept{quest}`입니다. 엔진은 해당 accept 방식 퀘스트의 상태가 여전히 `unset`이면 활성화하고, 그렇지 않으면 레코드를 무시합니다([퀘스트와 씬](/ko/language/quests-and-scenes/#퀘스트가-씬-및-계기를-만나는-방식)).

`end`에는 선택적인 자유 형식 `reason`이 있습니다. 저자가 지정하는 문자열(`"completed"`, 결말 id)이며, Lute는 여기에 의미를 부여하지 않고 호스트가 표시할 수 있습니다. 이를 만나 종료하는 동작은 `commands`의 끝까지 실행한 것과 같지만, reason을 사용할 수 있습니다.

## 현지화 텍스트

`line` 레코드의 `text`, choice/hub 옵션의 `text`, branch/hub의 `prompt`는 항상 **원본 언어**(`contentLang`)의 일반 텍스트입니다. 아티팩트를 [`lute compile --locale-bundle`](/ko/tooling/cli/#--locale-bundle--번역-번들-병합)로 빌드하면 레코드에 locale 태그 → 번역된 일반 텍스트인 `texts` 맵도 들어가며, 키는 레코드의 `lineId`입니다. `prompt`가 있는 `choice`/`hub` 레코드에는 `promptLineId`(`{prefix}.{branchOrHubId}.prompt`, `prompt`가 있으면 항상 존재, dsl 0.47.0)와, 번들이 번역했다면 `promptTexts`(locale 태그 → 번역된 프롬프트)도 들어갑니다. 인라인 수정자가 있는 line에는 원본용 `segments`와 각 번역 locale용 `localeSegments`가 일반 `texts` 항목 옆에 들어갑니다. 모든 번역 locale에는 항상 `texts` 항목이 있으므로 backlog, TTS, 검색에서 세그먼트를 평탄화할 필요가 없습니다.

```json
{
  "kind": "line",
  "family": "content",
  "position": "001-0200",
  "role": "narration",
  "speaker": "narrator",
  "text": "Wait for it here it comes.",
  "lineId": "opening.narrator_0020",
  "voiceKey": "opening.narrator-0020",
  "segments": [
    { "text": "Wait for it" },
    { "pause": 0.5 },
    { "text": " " },
    { "text": "here it comes", "rate": 1.25 },
    { "text": "." }
  ],
  "texts": { "ja-JP": "待って 来るよ。" },
  "localeSegments": {
    "ja-JP": [
      { "text": "待って" },
      { "pause": 0.5 },
      { "text": " " },
      { "text": "来るよ", "rate": 1.25 },
      { "text": "。" }
    ]
  }
}
```

locale 맵은 비어 있으면 생략됩니다. 따라서 `--locale-bundle` 없이 컴파일한 아티팩트는 이전과 바이트 단위로 동일하고, 이를 무시하는 소비자는 계속 원본 언어를 렌더링합니다. `texts`(수정된 line이면 `localeSegments`도, 프롬프트는 `promptTexts`)에서 locale을 조회해 제시하고 `text`(`prompt`)로 대체하십시오. 컴파일러는 빌드 시 정확히 그런 누락에 대해 경고(`W-L10N-MISSING`)하므로 완전한 번들에는 대체할 부분이 남지 않습니다.

런타임의 `line` 레코드(`--locale-bundle`로 컴파일한 번들에 대한 `lute-runtime` / `@lute-lang/runtime` 이벤트)는 이 맵을 렌더링해 줍니다(dsl 0.47.0). `{{…}}` 마커를 값으로 바꾼 `text`와 `segments`에 더해, line 명령에 `texts`와 `localeSegments`가 있으면 이를 함께 싣는데, 각 번역의 마커는 원본에서 참조 대상이 같은 마커(`{{ }}` 안의 텍스트를 앞뒤 공백을 빼고 비교하므로 `{{ user.voyages }}`와 `{{user.voyages}}`는 같음)에 대해 렌더링한 값으로 바뀝니다. 번역자는 마커 순서를 바꿔도 되고, 원본에 없는 마커는 쓴 그대로 남습니다. 레코드는 디코딩된 텍스트를 담으며(lute 0.47.1: 인코딩 백슬래시 없음), 번역에도 `\{{`를 쓸 수 있고 원본과 마찬가지로 리터럴 `{{`로 남습니다. 런타임 line은 아티팩트의 템플릿이 아니라 레코드로 보여 주십시오. 컨포먼스 레코드인 `lute run --json`은 다른 delivery 필드처럼 이를 생략합니다. 선택지 라벨과 프롬프트는 렌더링하지 않습니다.

## 런타임 문서

각 표면에는 [`docs/runtime/`](https://github.com/journeyWorker/lute/tree/main/docs/runtime) 아래에 자체 계약 문서가 있습니다.

- **[incremental-continuations.md](https://github.com/journeyWorker/lute/blob/main/docs/runtime/incremental-continuations.md)** — 검사된 연속 컴파일, 전체 스냅숏 교체, 커서/상태 유지, 스트림 최종화 및 접두 안정성을 설명합니다.
- **[execution-model.md](https://github.com/journeyWorker/lute/blob/main/docs/runtime/execution-model.md)** — 아티팩트 형태, 버전 게이트, 위치 및 디스패처 루프를 설명합니다.
- **[state-lifecycle.md](https://github.com/journeyWorker/lute/blob/main/docs/runtime/state-lifecycle.md)** — `scene`/`run`/`user`/`app`/`quest.<id>` 계층, 초기화 및 리셋 경계를 설명합니다.
- **[cel-and-facts.md](https://github.com/journeyWorker/lute/blob/main/docs/runtime/cel-and-facts.md)** — `expr` AST 평가, 팩트 저장소의 assert/retract 델타 및 엔진이 계산하는 계층화된 최소 고정점을 설명합니다.
- **[quest-lifecycle.md](https://github.com/journeyWorker/lute/blob/main/docs/runtime/quest-lifecycle.md)** — `start`/`fail` 우선순위, 필수 및 선택 목표, 단조 증가하는 완료, run 계층 퀘스트 및 수명 주기 이벤트를 설명합니다.
- **[beats-and-occasions.md](https://github.com/journeyWorker/lute/blob/main/docs/runtime/beats-and-occasions.md)** — 계기와 대상 도메인, 비트 후보, 자격 및 `once` 소진, 선택 순서를 설명합니다.
- **[lore-entries.md](https://github.com/journeyWorker/lute/blob/main/docs/runtime/lore-entries.md)** — 엔트리 자격, 제시, 첫 읽기 효과 및 `read` / `everRead` 플래그를 설명합니다.
- **[timeline-semantics.md](https://github.com/journeyWorker/lute/blob/main/docs/runtime/timeline-semantics.md)** — 로컬 시계, 트랙별 커서, 장벽 및 검사기가 보장하는 대상별 단일 writer 불변식을 설명합니다.
- **[bridge-protocol.md](https://github.com/journeyWorker/lute/blob/main/docs/runtime/bridge-protocol.md)** — 타입이 지정된 브리지 호출, 반환 형태, `wait` 및 확정된 상태 효과를 설명합니다.
