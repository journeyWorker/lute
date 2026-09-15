---
title: 기능 권한
sidebar:
  label: 기능 권한
description: 프로젝트 프로파일이나 호스트가 신뢰하는 권한 프로파일로 디렉티브, 상태·팩트 쓰기, 브리지, 보상, 퀘스트 저작 범위를 제한하되 컴파일 시점 허용 검사를 런타임 샌드박스로 오해하지 않습니다.
---

기능 권한(capability permissions)은 신뢰하는 프로젝트나 호스트가 `.lute` 소스에
허용할 저작 기능의 상한을 정합니다. 검사기는 금지된 효과를 거부하고 나서야 컴파일러가
산출물을 만듭니다. 이 정책은 특정 제품이나 Lute IR을 실행하는 엔진과 무관한 범용
기능입니다.

이 문서는 **Unreleased 도구 기능**을 설명하며 이미 배포된 버전을 주장하지 않습니다.
[규범 플러그인 시스템 델타](https://github.com/journeyWorker/lute/blob/main/docs/proposals/plugin-system/0.0.6.md)와
[런타임/보안 가이드](https://github.com/journeyWorker/lute/blob/main/docs/runtime/capability-permissions.md)가
원문 계약입니다.

## authored와 restricted 프로파일 설정

```yaml
# lute.project.yaml
defaultProfile: authored

# 루트 permissions가 없으므로 프로젝트 전체 상한은 없습니다.
profiles:
  authored:
    plugins: {}
    # permissions가 없음: 제한 없음
  restricted:
    plugins: {}
    permissions:
      directives: [camera, bg, music, sfx, end, set, use]
      stateWrites: [scene.*]
      factWrites: []
      bridges: []
      rewards: false
      quests: false
```

전체 프로젝트 파일 형식은
[`schemas/lute.project.json`](https://github.com/journeyWorker/lute/blob/main/schemas/lute.project.json)에
게시되어 있습니다.

필드는 다음과 같습니다.

| 필드 | 값 | 일치 규칙 |
| --- | --- | --- |
| `directives` | `::`를 뺀 디렉티브 이름 또는 `*` | 정확히 일치 |
| `stateWrites` | 점 경로, `*`, 또는 끝의 `.*` | 정확한 경로 또는 하위 경로만 |
| `factWrites` | relation 이름 또는 `*` | 정확히 일치 |
| `bridges` | `service/operation` 또는 `*` | 정확히 일치 |
| `rewards` | boolean | `false`는 거부, `true`는 추가 제한 없음 |
| `quests` | boolean | `false`는 거부, `true`는 추가 제한 없음 |

**필드 없음과 빈 목록은 다릅니다.** 필드가 없으면 추가 제한이 없습니다.
`factWrites: []`는 모든 팩트 쓰기를 명시적으로 금지합니다. 같은 원리로
`rewards: true`는 제한하지 않고 `rewards: false`는 보상을 금지합니다. 명시적인
`null`, 알 수 없는 필드, 잘못된 패턴, 타입이 틀린 값은 설정 오류이며 결코 제한 없음으로
폴백하지 않습니다.

`scene.dialogue.*`는 `scene.dialogue.current`를 허용하지만 루트
`scene.dialogue`나 비슷한 이름의 `scene.dialogueOther.current`는 허용하지 않습니다.
정규식, 중간 와일드카드, 공백, 일부 세그먼트 와일드카드는 없습니다. 브리지는 항상
모호하지 않은 `service/operation` 표기를 사용합니다.

## 상한은 좁힐 수만 있습니다

프로젝트 루트, 예약된 `global` 프로파일, 모든 부모 프로파일, 선택한 프로파일의 정책은
AND로 결합됩니다. 한 레이어의 목록 안에서는 하나만 일치해도 되지만, 해당 필드를
제한하는 모든 레이어가 작업을 허용해야 합니다. 자식은 부모를 넓힐 수 없고 인라인
플러그인 활성화도 어떤 정책도 넓힐 수 없습니다. 권한은 `app.*` 읽기 전용 규칙 같은
기존 언어 규칙도 덮어쓰지 않습니다.

권한은 플러그인을 활성화하거나 어휘를 추가하지 않습니다. 일반 소스 프로파일과
플러그인 그래프를 먼저 해석하고, 유효 권한 레이어가 그 스냅샷을 좁힙니다.

## 신뢰하는 호스트 상한 고정

소스의 `profile:`은 기능 선택이지 권한 부여가 아닙니다. 저작 또는 생성된 Lute를 받는
호스트는 상한을 독립적으로 선택해야 합니다.

```console
$ lute check scene.lute --project . --permission-profile restricted
$ lute compile scene.lute --project . --permission-profile restricted -o scene.json
$ lute compile --all --project . --permission-profile restricted -o build
$ lute context scene.lute --json --project . --permission-profile restricted
```

`--permission-profile NAME`은 해당 프로파일의 프로젝트/global/부모/자기 권한을
**추가 상한**으로 적용합니다. 그 프로파일의 플러그인을 활성화하지 않고 소스 프로파일도
바꾸지 않습니다. 따라서 `profile: authored`인 문서도 호스트가 고정한 `restricted`
정책을 넓힐 수 없습니다. 프로젝트나 프로파일이 없으면 명시적인 해석 오류입니다.

`compile --all`은 같은 호스트 상한으로 모든 문서를 먼저 검사합니다. 하나라도 거부되면
금지된 산출물도 부분 출력 집합도 만들지 않습니다.

스트리밍에서는 고정된 같은 상한이 신뢰하는 템플릿과 모든 본문 단위를 검사합니다.

```console
$ printf '::set{scene.score = 1}\n' \
    | lute compile-stream scenes/live.lute --project . \
        --permission-profile restricted
```

금지된 단위는 마지막 NDJSON `error` 레코드를 만들고 금지된 `update`나 `finish`를
만들지 않습니다. 스트림 중간 권한 전환은 없습니다.

## 무엇을 거부하는가

공유 검사 패스는 눈에 보이는 `::name` leaf보다 넓은 범위를 다룹니다.

- `set`, `assert`, `retract`, 컴포넌트 `use`를 포함한 디렉티브
- 명시적 상태/팩트 쓰기, choice `into`, 암시적 선택 기록, hub 방문 기록
- 초기화도 효과이므로 상태 기본값과 seed fact
- 플러그인이 선언한 상태 쓰기와 브리지 `service/operation` 호출
- 퀘스트, 퀘스트 생명주기/목표 초기화, 선언형 보상
- 중첩 branch, match, hub, objective, `on`, timeline 본문
- 호출자 정책으로 실행되는 호출된 컴포넌트 본문

허용 여부는 실행될 경로가 아니라 저작된 소스를 대상으로 하므로 dead guard 뒤의 금지된
보상이나 효과도 숨길 수 없습니다. 기본값이 **없는** 상태 선언은 호스트가 제공하는 읽기
전용 컨텍스트로 계속 노출할 수 있습니다. 제한 중인 플러그인 쓰기 경로를 속성에서 해석할
수 없으면 fail-closed로 거부합니다.

금지된 소스에는 저작 위치에서 억제할 수 없는 오류가 납니다.

| 코드 | 범주 |
| --- | --- |
| `E-PERMISSION-DIRECTIVE` | 디렉티브 |
| `E-PERMISSION-STATE` | 상태 쓰기/기본값 |
| `E-PERMISSION-FACT` | 팩트 쓰기/seed fact |
| `E-PERMISSION-BRIDGE` | 브리지 |
| `E-PERMISSION-REWARD` | 보상 선언 |
| `E-PERMISSION-QUEST` | 퀘스트 선언 |

`compile`은 lowering 전에 같은 권한 게이트를 다시 실행합니다. 다른 정책에서 만든 성공한
검사 결과로 금지된 IR을 산출물에 밀어 넣을 수 없습니다.

## context와 에디터 동작

`lute context --json`은 유효 레이어를 `permissions: { layers: [...] }`로
직렬화합니다. `bridges`에는 허용된 브리지 기능 객체만, `rewardKinds`에는 허용된
이름별 보상 종류만 들어가며(보상을 금지하면 `{}`), `questsAllowed`는 퀘스트 저작
허용 여부를 나타냅니다. 기존 `directives` 배열도 디렉티브 이름이 금지되었거나 그
브리지의 `service/operation`이 금지된 항목을 제외합니다.

외부에서 제공하는 읽기 전용 상태는 계속 보여 주며, 속성에 따라 결정되는 플러그인 쓰기
경로를 모두 열거할 수 있다고 가장하지 않습니다. 텍스트 출력도 이 정책을 컴파일 시점
저작 제한이라고 명시하며 런타임 샌드박스라고 부르지 않습니다.

LSP는 같은 해석기와 검사기를 사용하여 같은 진단을 게시하고 금지된 디렉티브와 브리지를
자동 완성에서 제외합니다. 에디터 필터링은 저작 보조이며 실제 게이트는 `check`와
`compile`입니다.

제한적인 권한은 `capabilityVersion`에 포함됩니다. 완전히 제한 없는 정책은 정규화해서
없애므로 정책 없는 기존 기능 해시는 바이트 단위로 유지됩니다. 이 해시는 캐시가 저작
표면을 구분하는 메타데이터일 뿐, 산출물이 허가되었다는 서명이나 증거가 아닙니다.

## 완전한 범용 예제

저장소의
[`docs/examples/capability-permissions/`](https://github.com/journeyWorker/lute/tree/main/docs/examples/capability-permissions)에는
다음이 있습니다.

- 권한 필드가 없는 `authored` 프로파일
- 명시적 빈 목록과 `false` 게이트를 가진 `restricted` 프로파일
- 일반 프로파일에서는 선언형 보상이 컴파일되지만 호스트 고정 정책에서는 거부되는 퀘스트
- 기본값 없는 상태는 허용되다가 생성된 `::set`이 금지된 쓰기를 시도하면 거부되는
  스트리밍 장면

예제는 원격 호출이나 AI 호출을 하지 않습니다. 보상은 산출물의 선언형 데이터일 뿐이며
지급 로직을 구현하지 않습니다.

## 보안 비목표

기능 권한은 컴파일 시점 허용 검사입니다. 다음 기능이 아닙니다.

- AI 호출 또는 AI 전용 문법
- 특정 제품 전용 플러그인
- 런타임, 프로세스, 네트워크, 파일시스템, 브리지 구현, 비밀의 샌드박스
- 보상의 추첨, 지급, 정산, 저장 구현
- 신뢰할 수 없는 플러그인, 프로젝트, 산출물 또는 외부에서 받은
  `capabilityVersion` 해시를 안전하게 만드는 장치

호스트는 여전히 런타임 주체와 리소스를 인가하고, 의도한 구현에만 브리지를 연결하며,
영속성/멱등성을 관리하고, 신뢰하는 산출물만 로드해야 합니다.
