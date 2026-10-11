---
title: 플러그인 개념
description: Lute 플러그인이 고정된 문법이나 컴파일러 동작을 건드리지 않고 어휘와 capability 표면을 확장하는 방법, 그리고 선언하는 모든 항목이 신뢰되는 대신 검증되는 방법입니다.
---

Lute에는 **고정된 문법**과 **전체적이며 튜링 완전하지 않은(non-Turing) 코어**가 있습니다. 플러그인은 어느 쪽도 바꾸지 않습니다. 플러그인은 문법이 이미 참조하는 *어휘와 capability 표면*만 추가합니다 — 디렉티브 이름, 속성, enum 도메인, 상태 shape, provider, 브리지 시그니처, 정의, 프런트매터 키, 아티팩트 종류, 진단이 그것입니다. 플러그인이 선언하는 모든 것은 **데이터**이며, 매니페스트 스키마에 따라 검증되고, 선택한 프로필과 함께 하나의 결정적인 **capability 스냅숏**으로 해석되어 체커, LSP, 컴파일러가 모두 소비합니다.

## 한 가지 규칙

플러그인은 어휘를 추가하고, 코어는 문법과 동작을 소유합니다. 제어 흐름, 낮춤 알고리즘, 완전성 검사, 자동 주입은 모두 컴파일러에 있으며 — 매니페스트에는 절대 있지 않습니다.

## 플러그인이 추가할 수 있는 것

- **디렉티브 어휘** — 타입이 지정된 `attrs`, `semantics` 플래그, 선언된 상태 효과를 가진 새로운 `::name` 스테이징 또는 브리지 디렉티브([매니페스트](/ko/plugins/manifests/) 참고)입니다.
- **상태 shape** — 타입이 지정된 재사용 레코드와 타입이 지정된 슬롯을 여는 구조화된 경로 템플릿입니다.
- **Provider** — 라이브 네트워크 호출이 아니라 고정된 카탈로그에 대해 해석되는, 스냅숏 우선 id 레지스트리(캐릭터 id, 아티팩트 id, 미니게임 id)입니다.
- **브리지 시그니처** — 타입이 지정된 런타임 호출입니다. 디렉티브가 스토리 상태에 되돌려 쓸 수 있는 값은 선언된 결과 필드뿐입니다([브리지](/ko/plugins/bridge/) 참고).
- **Enum, def, 프런트매터 키, 아티팩트 종류, 월드 이벤트, 횡단 속성** — 닫힌 값 집합, 공유 타입 지정 CEL `@refs`, 플러그인이 소유한 `---` 메타 키, 구조화된 아티팩트 id 템플릿, `<on event>` 트리거, 모든 디렉티브와 콘텐츠 줄에 허용되는 [`stampAttrs`](/ko/plugins/manifests/)입니다.
- **진단** — 속성별 메시지, 지원 중단(deprecation), 별칭, fix-it입니다.

## 플러그인이 추가해서는 안 되는 것

문법 생성 규칙, 새로운 괄호 형태, 새로운 블록 종류, 새로운 제어 흐름 또는 어떤 동작도 추가해서는 **안 됩니다(MUST NOT)**. capability가 플러그인 데이터로 등록될 수 있는 조건은 모두 다음과 같습니다: 고정된 문법, 속성과 카탈로그 조회에 국소화된 검증, 유한한 attrs→records 매핑인 낮춤, 새로운 제어 흐름 없음, 선언된 리소스 충돌을 넘는 형제 간 또는 전역 추론 없음, AST 형태 변경 없음, 기존 타임라인/`wait` 모델을 넘어서는 순서 민감한 해석 없음. 하나라도 어기면 capability는 **코드**이며 컴파일러 코어 변경에 해당하므로 플러그인에 속하지 않습니다.

이름 붙여야 할 함정은 이렇습니다. 작성자가 가장 원하는 것은 어휘처럼 *느껴지지만* 실제로는 컴파일러 동작인 것들입니다 — "그녀가 자연스럽게 떠나게 하기", "분위기가 바뀌지 않는 한 같은 포즈 유지하기", "줄이 나타나는 정확한 순간에 이 SFX 재생하기". 이런 기능은 가능하다면 디렉티브마다 golden 테스트로 보호되는 코어의 **이름 붙은 내장 낮춤 훅**으로 제공됩니다. **디렉티브가 깔끔한 golden 테스트를 얻을 수 없다면, 그것은 데이터만이 아니라 코드입니다.**

[캐릭터/캐스트](https://github.com/journeyWorker/lute/blob/main/docs/proposals/character-cast/0.0.1.md) capability가 대표적인 예입니다. `cast:` 프런트매터 키, `character` provider, `scene.cast.<id>.*` 상태, `CH` 아티팩트 종류, `::seal` / `::reveal` / `::wear` 디렉티브를 추가하지만 — 모두 새로운 의미론 없이 `::set`으로 순수하게 디슈거링합니다.

## 선언은 신뢰되는 것이 아니라 검사됩니다

매니페스트에 표면을 선언했다고 해서 도구체인이 이를 믿게 되는 것은 아닙니다. 작성자가 플러그인 소유 표면에 도달하기 전에 모두 검증됩니다: 활성화 시 [옵션 값](/ko/plugins/profiles/)(`E-PLUGIN-OPTION-UNKNOWN`, `E-PLUGIN-OPTION-TYPE`), [코어 스탬프에 대한 속성 이름](/ko/plugins/manifests/)(`E-PLUGIN-RESERVED-STAMP-ATTR`), [대상 레코드의 실제 필드 테이블에 대한 선언적 낮춤](/ko/plugins/manifests/)(`E-LOWER-RECORD-UNKNOWN`, `E-LOWER-RECORD-FIELD`)입니다. 이 검사는 모두 로드, 조립 또는 활성화 단계에서 실패하므로, 잘못된 매니페스트가 작성자를 오도하기 전에 막습니다.

플러그인 소유 **프런트매터 키**도 마찬가지입니다. `frontmatter/*.yaml` export는 `{ key, schema }` 쌍을 선언하고, 체커는 `E-META-UNKNOWN-KEY`를 넘겨 이 키를 허용하는 동시에 선언된 `schema`에 따라 값을 검증합니다. 위반은 키 자체의 범위에서 **`E-FRONTMATTER-SCHEMA`**로 보고됩니다:

<!-- lute-diagnostics -->
```
$ lute check scene.lute --project .
scene.lute:10:1: error [E-FRONTMATTER-SCHEMA] frontmatter key `bonusTrack` expects bool (declared by an active plugin), got "yesplease"
failed: scene.lute (1 error(s), 0 warning(s))
```

매니페스트 리터럴 표현 자체가 없는 값 — `null`, `!tag`, 문자열이 아닌 맵 키 — 도 똑같은 오류이며, 패닉도 아니고 조용한 통과도 아닙니다. 코어 프런트매터 키(보편 집합, 씬 전용 집합, 컴포넌트 전용 집합)는 기존의 전용 처리를 유지하며 이 검사를 통하지 않습니다.
