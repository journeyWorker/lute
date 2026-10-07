---
title: 진단 코드 레퍼런스
description: "Lute가 보고하는 모든 진단 코드: 무엇이 그 코드를 일으키는지, 그리고 그 뒤의 스펙 절."
---

<!-- crates/lute-cli/src/codes.rs의 레지스트리와 같은 순서로 코드마다 한 절을 둡니다(`cargo test -p lute-cli --bins codes`가 검사합니다). -->

Lute가 출력하는 모든 진단에는 코드가 붙습니다. `E-` 코드는 오류입니다: 문서가 검사를 통과하지 못하고 `lute check`는 1로 끝납니다. `W-` 코드는 경고입니다: `--deny <CODE>`나 `--deny-warnings`가 오류로 올리지 않는 한 문서는 통과합니다. `E-` 코드가 경고로 출력되는 일은 없습니다: `check-project --wip`가 봐주는 죽은 가드는 `W-WIP`로 보고되고, 메시지가 플래그 없이는 같은 가드에 붙을 오류 코드를 밝힙니다. `lute --explain <CODE>`는 아래 항목을 터미널에 출력하고, 에디터는 각 코드를 이 페이지의 해당 절로 연결합니다.

메시지는 무엇이 잘못되었는지를 평이한 말로 알려 줍니다. 코드 뒤의 스펙 절은 각 항목 아래에 해당 제안서 링크와 함께 나열되며, `--json` 출력에서는 각 진단의 `spec` 필드에 담깁니다.

위치 `file:line:column`의 줄과 열은 1부터 세며, 열은 바이트가 아니라 문자 단위입니다: 오류 앞의 한글 한 글자나 이모지 하나는 한 열입니다. `--json`의 `span.column`과 `lute scenario … reach`의 `causes[].column`도 같은 수입니다. 언어 서버는 LSP 규약대로 UTF-16 위치를 보고합니다.

## 오류

### E-ACCEPT-TARGET

`::accept` 지시어가 퀘스트를 지정하지 않았거나, `quest` 값이 인용된 퀘스트 id가 아니거나, `at` 값이 `"nextRun"`이 아니거나, 이미 부모와 함께 활성화되는 `accept="external"` 퀘스트를 대상으로 지정해 수락이 아무 효과가 없습니다.

명세: [dsl 0.21.0 §7a.3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.21.0.md), [dsl 0.24.0 §2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.24.0.md), [dsl 0.25.0 §5](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.25.0.md)

### E-ADVANCE-CASCADE

반복 가능한 비트가 자신을 표시한 클록 상승 이벤트에 응답하면서 다음 위치로 시간을 이동해, 반복되는 중첩 `advances:` 연쇄를 만듭니다.

명세: [dsl 0.31.0 §1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.31.0.md)

### E-AGE-GATE

연령 게이트가 적용된 `<match subject="app.rating">`가 `teen` 분기도 `<otherwise>`도 포함하지 않아, 릴리스 빌드에서 어느 경우에도 매치되지 않을 수 있습니다.

명세: [dsl §11.2](/spec/)

### E-APP-READONLY

`::set` 지시어가 `app.*` 네임스페이스에 씁니다. 이 네임스페이스는 엔진/설정 계층만 소유하므로 콘텐츠가 쓸 수 없습니다.

명세: [dsl §9.5](/spec/)

### E-ARM-DEAD

게이트가 걸린 콘텐츠 라인, `<match>` 분기, `<choice when>`, 또는 `::jump{when}`의 `when` 가드가 항상 거짓임이 증명되어 절대 표시되거나 선택될 수 없습니다.

명세: [dsl 0.4.0 §5.2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.4.0.md), [dsl 0.4.0 §7.2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.4.0.md), [dsl 0.20.0 §5](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.20.0.md)

### E-AS-REMOVED

선택지가 제거된 `as` 속성을 사용합니다. 선택지가 기록하는 run 팩트는 `into`로 지정합니다.

명세: [dsl 0.10.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.10.0.md)

### E-ASSET-DECOMPOSE

에셋 id 문자열의 세그먼트 개수가 선언된 종류에 맞지 않거나, 고정(`const`) 세그먼트 값이 해당 종류가 요구하는 값과 일치하지 않습니다.

명세: [dsl §7.2](/spec/)

### E-ASSET-SEGMENT

에셋 id의 한 세그먼트가 선언된 열거형의 구성원이 아니거나, 숫자가 필요한 자리에 숫자가 아니거나, 공급자 카탈로그에 알려진 id가 아닙니다.

명세: [dsl §7.2](/spec/)

### E-ASSET-UNKNOWN-ID

순수 조회형 에셋 종류의 id 문자열이 선언된 공급자 카탈로그에 알려진 id가 아닙니다.

명세: [dsl §7.2](/spec/)

### E-AT-CONTEXT

지시어(일반 `::directive` 또는 `::use`)가 `<track>` 클립 밖에서 타임라인 위치 속성 `at`을 사용했습니다. `at`은 그 위치에서 유효하지 않습니다.

명세: [dsl §7.5](/spec/)

### E-ATTR-DEF-DYNAMIC

지시어 속성 값으로 쓰인 `@def`나 컴포넌트 본문 속성에 대응되는 `::use` 인자가 상태에 의존해 컴파일 타임 상수로 축약되지 않습니다.

명세: [dsl §5.1](/spec/)

### E-ATTR-QUOTE

속성 값이 필수인 곧은 겹따옴표 `"` 대신 홑따옴표나 굽은(워드프로세서) 따옴표로 구분되어 있습니다.

명세: [dsl §4.4](/spec/), [dsl §4.5](/spec/)

### E-ATTR-TYPE

속성 값이 선언된 타입과 맞지 않습니다. 예를 들어 숫자가 아닌 `duration`/`delay`, 불리언이 아닌 플래그, 인용된 문자열이 필요한 자리의 맨 식별자, 또는 공급자·도메인·엔티티 종류의 구성원을 가리키지 않는 값입니다.

### E-AUTHOR-CASE

작성 속성, 태그 속성 또는 프런트매터 키가 lowerCamelCase가 아닙니다(`_`가 들어 있습니다). 메시지가 lowerCamelCase 표기를 알려 줍니다.

명세: [dsl 0.37.0 §2.1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.37.0.md)

### E-BAD-ENUM

값이 속해야 하는 닫힌 열거형, 도메인, 엔티티 종류의 구성원이 아닙니다. 예를 들어 콘텐츠 라인의 `emotion=`이 화자에게 선언된 `emotions:` 밖의 값이거나, 엔티티 id가 해당 종류 밖의 값입니다.

명세: [dsl 0.24.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.24.0.md)

### E-BEAT-ATTR

비트의 `on`, `target`, `priority`, `once` 속성이 잘못되었습니다. 이름이 아닌 `on`, `target: true`로 선언되지 않은 occasion에 붙은 `target`, 정수가 아닌 `priority`, `run`/`user`/`false` 밖의 `once`, `on` 없는 비트 키, 아직 존재하지 않는 씬 자신의 `scene.*` 상태를 읽는 `when`, 또는 `once: false`나 `share` 키와 함께 쓴 `spentBy`가 해당합니다.

명세: [dsl 0.21.0 §3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.21.0.md), [dsl 0.21.0 §5](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.21.0.md), [dsl 0.28.0 §6](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.28.0.md)

### E-BEAT-ID-DUP

한 lore 문서의 두 선언이 같은 id를 씁니다. 번들 `<beat>` id가 반복되었거나, `<entry>`의 id가 `<beat>`의 id와 같습니다. 이때 비트의 정식 id `<문서 id>.<id>`가 entry의 별칭이기도 해서, `visited()`나 플레이의 `expect.winner`가 둘을 모두 가리키게 됩니다.

명세: [dsl 0.28.0 §7](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.28.0.md)

### E-BEAT-UNREACHABLE

씬 비트의 `when` 가드가 항상 거짓임이 증명되어 그 비트가 절대 선택될 수 없습니다.

명세: [dsl 0.21.0 §5](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.21.0.md)

### E-BRANCH-ALL-GUARDED

비어 있지 않은 `<branch>`의 모든 `<choice>`에 `when` 가드가 걸려 있어, 모든 가드가 동시에 거짓이 되어 빈 메뉴가 나타날 수 있습니다. 가드 없는 선택지가 최소 하나 필요합니다.

명세: [dsl §11.1](/spec/)

### E-BRANCH-EMPTY

`<branch>` 또는 `<hub>`에 `<choice>`가 하나도 없어, 라우팅할 수 없는 선택지로 축약됩니다.

명세: [dsl §7.3](/spec/)

### E-BRANCH-PROMPT

`<branch prompt>` 또는 `<hub prompt>` 값이 없거나 빈 문자열입니다. 프롬프트는 UI가 그대로 보여주는, 비어 있지 않은 문장이어야 합니다.

명세: [dsl 0.11.1 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.11.1.md), [dsl 0.23.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.23.0.md)

### E-BRANCH-TIMEOUT

`<branch timeout>` 값이 양의 정수 초로 해석되지 않습니다.

명세: [dsl 0.11.1 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.11.1.md)

### E-CAMERA-EMPTY

`::camera` 지시어가 `focus`, `framing`, `move`, `transition` 중 어느 것도 지정하지 않아 할 일이 없습니다.

명세: [dsl 0.37.0 §3.3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.37.0.md)

### E-CAMERA-REMOVED

`::camera` 지시어가 제거된 수치·구식 속성(`zoom`, `moveX`, `moveY`, `shake`, `reset`, `easing`)을 씁니다. 카메라 값은 이제 프로젝트가 선언하는 `framing`, `cameraMove`, `transition` 구성원이므로 손으로 옮겨야 합니다.

명세: [dsl 0.37.0 §3.3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.37.0.md), [dsl 0.37.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.37.0.md)

### E-CAPABILITY-MISMATCH

같은 프로젝트의 두 문서가 서로 다른 기능(capability) 스냅샷으로 해석되어, 프로젝트가 색인할 단일 `capabilityVersion`을 가지지 못합니다.

명세: [dsl §13](/spec/)

### E-CAST-UNKNOWN

콘텐츠 라인의 화자나 `::actor{character}`/`::camera{focus}` 리터럴이 프로젝트에 선언된 캐스트 밖의 id를 가리킵니다.

명세: [dsl 0.23.0 §7](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.23.0.md), [dsl 0.24.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.24.0.md)

### E-CEL-PARSE

조건 슬롯, def 본문, 또는 `present:`/비트 `when` 안의 CEL 식이 올바른 CEL 문법으로 파싱되지 않습니다.

명세: [dsl 0.4.0 §8.1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.4.0.md)

### E-CEL-PROFILE

CEL 식이 제한된 Lute-CEL 프로파일 밖의 구문을 사용합니다. 허용되지 않는 함수 호출, 컴프리헨션 매크로, 맵/구조체 리터럴, 상태 경로나 def가 아닌 맨 식별자, 예약된 내부 토큰, 또는 조건 슬롯 밖에서 쓰인 `visited(…)`가 해당합니다.

명세: [dsl §8.4](/spec/), [dsl 0.21.0 §7a.1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.21.0.md)

### E-CEL-TYPE

조건의 타입이 쓴 뜻을 낼 수 없습니다. bool·숫자·문자열 사이의 비교(`visited('x') > 2`, `run.oil == true`, `run.day == 'monday'`), 숫자가 아닌 값의 순서 비교(`run.hour >= 'h03'`), `&&` / `||` / `!` / `?:`의 bool이 아닌 피연산자나 조건, 계산할 수 없는 산술, 정수 나머지 연산자 `%`의 정수가 아닌 피연산자가 해당합니다.

명세: [dsl 0.24.0 §1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.24.0.md), [dsl 0.28.0 §1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.28.0.md)

### E-CG-LAYOUT

`::cg` 지시어가 제거된 `full` 속성을 씁니다. 이를 대신할 수 있는 것은 `layout=`으로 쓰는 프로젝트 선언 `cgLayout` 구성원뿐이므로 손으로 옮겨야 합니다.

명세: [dsl 0.37.0 §3.3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.37.0.md), [dsl 0.37.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.37.0.md)

### E-CHAPTERS

프로젝트의 `chapters:`가 잘못되었습니다. `{ on, scenes }` 체인의 목록이 아니거나, 둘 중 어느 것도 아닌 키가 있거나(체인은 occasion을 `occasion:`이 아니라 `on:`으로 씁니다), 장면 id가 아닌 항목이 있거나, 장면이 두 번 나열되었거나, 한 occasion에 체인이 둘 있거나, 매니페스트가 아직 폐지된 `sequence:` 키를 씁니다. 또는 체인이 어떤 플러그인도 선언하지 않은 occasion을 가리키거나(shape-only에서는 다른 비트가 응답하는 occasion의 오타), 어떤 장면도 선언하지 않은 id를 나열하거나(번들 비트·lore 항목·문서면 그렇다고 말합니다), 나열된 장면의 `on:`이 다른 occasion에 응답하거나, target과 함께 올리는 occasion에서 `target:`이 없는 장면을 나열합니다(모든 target에서 재생됩니다). 잘못된 체인은 적용되지 않고, 나머지 체인은 적용됩니다. 매니페스트 줄에서 보고되며(빠진 `target:`은 장면의 `id:`에서), 문서들은 그대로 검사됩니다.

명세: [dsl 0.28.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.28.0.md)

### E-CHECK-ENGINE-SEMANTICS

선택한 엔진이 지원하지 않는 의미 기능을 구성 요소가 요구합니다.

명세: [dsl 0.33.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.33.0.md)

### E-CHOICE-DUP

`<branch>` 또는 `<hub>`가 같은 `id`를 가진 `<choice>`를 두 번 선언했습니다. 선택지 id는 같은 branch/hub 안에서 유일해야 합니다.

명세: [dsl §11.1](/spec/)

### E-CHOICELOG-READ

가드나 조건이 예약된 선택 기록(choice-log) 경로를 읽습니다. 이 경로는 가드나 조건에서 읽을 수 없습니다.

명세: [dsl §9.6](/spec/)

### E-CLIP-OVERLAP

같은 `<track>` 안의 두 클립이 겹치는 `[at, at+duration)` 구간을 가집니다.

명세: [dsl §11.4](/spec/)

### E-CLIP-TIMING

하나의 `<track>` 클립이 절대 위치인 `at`과 상대적 지연인 `delay`를 동시에 가지고 있습니다. 이 둘은 한 클립에서 함께 쓸 수 없습니다.

명세: [dsl §7.5](/spec/), [dsl §11.4](/spec/)

### E-CLOCK-DECL

`clock:` 선언이 잘못되었거나(필드 누락, `slots`에 없는 `last.slot`, `days: 0`, `last`와 `days` 동시 지정) 프로젝트가 `clock:`을 두 번 이상 선언했습니다.

명세: [dsl 0.24.0 §1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.24.0.md)

### E-CLOCK-END

유한한 클록이 이미 끝난 상태에서 `lute play`/`lute test`의 `advance:` 단계가 실행되었거나, 마지막 위치를 지나서 시작되었습니다. 클록은 이미 마지막 `dayEnd`를 발생시키고 멈췄으므로 `newRun`을 플레이하거나 스크립트를 종료해야 합니다.

명세: [dsl 0.27.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.27.0.md)

### E-COMMENT-UNTERMINATED

`/* … */` 블록 주석이 닫는 `*/` 없이 파일 끝까지 이어졌습니다.

명세: [dsl §4.2](/spec/)

### E-COMPILE-COMPONENT

`::use` 호출이 확인되지 않은 채로 컴파일 단계에 도달했습니다. `<timeline>` 클립 안에서 쓰였거나(허용되지 않음), 해석 가능한 컴포넌트를 지정하지 않았거나, 인자가 컴포넌트의 파라미터와 맞지 않는 경우로, 검사 게이트가 이미 잡았어야 하는 상황입니다.

### E-COMPILE-EXPAND

CEL 슬롯이나 비트 `when`이 컴파일 시점에 확장되지 못했습니다. 확장 순환, 알 수 없는 def, 또는 인자 개수 불일치가 원인입니다.

### E-COMPILE-INTERNAL

컴파일러가 복구할 수 없는 상태에 이르렀습니다. Lute의 버그이므로 이 오류를 일으키는 문서와 함께 보고해 주십시오.

### E-COMPONENT-ARG

`::use` 호출의 인자가 지정한 컴포넌트의 선언된 파라미터와 맞지 않습니다. 알 수 없는 인자, 필수 파라미터 누락, 타입이 맞지 않는 값, 호환되지 않는 기본값, 또는 `speaker` 파라미터에 리터럴 캐스트 id가 아닌 인자가 그 예입니다.

명세: [dsl §13](/spec/), [dsl 0.24.0 §3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.24.0.md), [dsl 0.26.0 §3.3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.26.0.md)

### E-COMPONENT-BODY

컴포넌트 본문에 표현(presentational)이 아닌 구문이 있습니다. 상태를 쓰거나 영향을 주는 `::set`, `<branch>`, `<hub>`, `<timeline>`, `<objective>`, `<on>`, `::assert`, `::retract`, 또는 `effects: true`를 선언하지 않고 효과가 있는 내부 컴포넌트를 `::use`한 경우입니다.

명세: [dsl 0.4.0 §6.2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.4.0.md), [dsl 0.24.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.24.0.md)

### E-COMPONENT-CYCLE

`::use` 호출들의 연쇄가 컴포넌트들 사이의 순환으로 확장됩니다.

명세: [dsl §13](/spec/)

### E-COMPONENT-DUP

서로 다른 두 컴포넌트 파일이 같은 `component:` 이름을 선언했습니다.

### E-COMPONENT-INSTANCE-DUPLICATE

같은 컴포넌트와 인스턴스 키가 하나의 직접 확장 소유자 안에서 두 번 나타납니다.

명세: [dsl 0.36.0 §2.3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.36.0.md)

### E-COMPONENT-INSTANCE-INVALID

`instance`가 비어 있거나 64자를 초과하거나 ASCII가 아닌 토큰을 사용하거나, 하나의 `::use`에서 identity 속성으로 두 번 나타납니다.

명세: [dsl 0.36.0 §2.3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.36.0.md)

### E-COMPONENT-PARSE

컴포넌트 파일을 읽거나 해석하거나 깨끗하게 파싱할 수 없습니다. 해석할 수 없는 `components:` 임포트 경로, 없는 `component:` 이름, 또는 잘못된 `params:` 항목이 원인입니다.

### E-COMPONENT-STATE

컴포넌트 본문이 파라미터를 통하지 않고 주변(ambient) 상태를 직접 읽거나 씁니다. 상태 경로를 참조하는 CEL, 팩트 조회, 또는 상태/브리지 결과 쓰기를 선언하는 지시어가 해당합니다.

명세: [dsl 0.4.0 §6.1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.4.0.md), [dsl 0.4.0 §6.2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.4.0.md), [dsl 0.24.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.24.0.md)

### E-COMPONENT-UNDECLARED

`::use`의 `component` 속성이 해석된 `components:` 테이블에 없는 컴포넌트를 지정합니다.

명세: [dsl §13](/spec/)

### E-CONN-CYCLE

프로젝트의 `after` 선행조건 그래프에 순환이 있어, 모든 `after` 조건을 동시에 만족하는 평가 순서가 존재하지 않습니다.

명세: [dsl §4.1](/spec/)

### E-CONN-EPISODE-ID-DUP

두 문서(또는 문서와 파생 키)가 프로젝트의 공유 id 네임스페이스에서 같은 문서 id를 갖습니다.

명세: [dsl 0.15.0 §2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.15.0.md), [dsl 0.19.0 §2.1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.19.0.md)

### E-CONN-FORMULA-TOO-COMPLEX

`after` 선행조건 수식의 원자(atom) 개수가 방어적 상한을 초과했습니다. 이는 사람이 직접 작성했다기보다 병적이거나 기계적으로 생성된 수식의 특징입니다.

명세: [dsl §4.1](/spec/)

### E-CONN-PROFILE

`after` 선행조건 수식이 제한된 선행조건 프로파일 밖의 구문을 사용합니다. `&&`/`||`와 괄호로 결합된 `visited(…)`/`completed(…)`/`active(…)` 원자 외의 것이 해당합니다.

명세: [dsl §4.1](/spec/)

### E-CONN-UNKNOWN-NODE

`after` 선행조건 수식의 `visited(K)`/`completed(Q)`/`active(Q)` 원자가 프로젝트 어디에도 존재하지 않는 노드를 가리킵니다.

명세: [dsl §2.3](/spec/), [dsl §4.1](/spec/)

### E-CONN-UNREACHABLE

씬, 퀘스트, 또는 비트가 도달할 수 없음이 증명됩니다. 어떤 평가 순서로도 해당 `after` 선행조건을 만족시킬 수 없습니다.

명세: [dsl §4.1](/spec/), [dsl §4.2](/spec/)

### E-CONSTRAINT-DECL

프로젝트 제약 선언이 잘못되었습니다. 알 수 없는 키/종류, 누락된 필수 필드, 잘못된 노드 참조 또는 중복 ID가 포함되어 있습니다.

명세: [dsl 0.34.0 §5.1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.34.0.md)

### E-CONSTRAINT-VIOLATED

프로젝트 제약이 위반되었습니다. 진단에는 판정의 증거 수준과 해당하는 경우 제한 범위 및 원인 스팬이 포함됩니다.

명세: [dsl 0.34.0 §5.2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.34.0.md)

### E-CONTENT-LINE-BRACKET

콘텐츠 라인의 속성이 필수인 `{…}` 대신 `[…]`로 작성되었습니다. (`::directive{…}`와 같은 구분자를 써야 합니다.)

명세: [dsl §2.1](/spec/)

### E-CONTENT-OUTSIDE-SECTION

`@speaker…`, `::directive`, `<tag>` 같은 콘텐츠 형태의 줄이 문서의 첫 `## ` 섹션 제목보다 앞에 나왔습니다. 콘텐츠는 섹션 본문 안에만 있을 수 있습니다.

명세: [dsl 0.5.0 §2.1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.5.0.md), [dsl 0.6.0 §3.3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.6.0.md)

### E-CONTEXT-POSITION

context 위치 질의의 좌표가 잘못되었거나 소스 파일 범위를 벗어났습니다.

명세: [dsl 0.36.0 §3.3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.36.0.md), [dsl 0.36.0 §8](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.36.0.md)

### E-CONTEXT-TARGET

context 대상 키가 프로젝트 그래프에서 없거나 모호합니다.

명세: [dsl 0.36.0 §3.1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.36.0.md), [dsl 0.36.0 §8](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.36.0.md)

### E-DATALOG-FUNCTION

`::assert`/`::retract` 페이로드나 `facts:` 항목, `rules:` 항의 항이 `f(g(x))`와 같은 복합/함수 항을 사용했지만, 팩트와 규칙 항은 그라운드 식별자/불리언만 허용합니다.

명세: [dsl 0.3.0 §7.1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.3.0.md)

### E-DATALOG-GUARD-FACT

규칙 본문의 CEL 가드가 `holds`, `count`, `validAt`, `now`를 통해 팩트 저장소나 서사 시간을 읽었지만, 가드는 이에 의존할 수 없습니다.

명세: [dsl 0.3.0 §7.2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.3.0.md), [dsl 0.3.0 §7.3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.3.0.md)

### E-DATALOG-PARSE

`facts:`/`rules:` 항목이 인용된 문자열이 아니거나, 인용된 `facts:`/`rules:` 문자열이 올바른 그라운드 팩트나 규칙으로 파싱되지 않습니다.

명세: [dsl 0.3.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.3.0.md), [dsl 0.3.0 §5](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.3.0.md)

### E-DATALOG-UNSAFE

규칙의 부정 원자, `!=` 비교, 또는 CEL 가드가 어떤 양의 본문 원자로도 먼저 바인딩되지 않은 변수를 읽어, 규칙을 안전하게 평가할 수 없습니다.

명세: [dsl 0.27.0 §3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.27.0.md), [dsl 0.24.0 §3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.24.0.md)

### E-DATALOG-UNSTRATIFIED

규칙의 부정 간선이 술어 의존 그래프에서 순환(`p :- not p` 포함)을 닫아, 규칙 집합을 계층화할 수 없습니다.

명세: [dsl §7.2](/spec/)

### E-DEF-DECL

`defs:` 항목이 잘못되었습니다 — CEL 문자열이나 `{ type?, params?, cel }` 매핑이 아니거나, 알 수 없는 키가 있거나, `cel:`이 없거나, `type:`이 본문에서 추론할 수도 본문과 일치하지도 않습니다.

명세: [dsl 0.21.0 §7b](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.21.0.md)

### E-DEFAULTS-KEY

매니페스트의 `defaults:` 블록이 닫힌 기본값 가능 프런트매터 집합 밖의 키를 지정했거나, 기본값 가능한 키에 잘못된 형태의 값을 지정했습니다.

명세: [dsl 0.10.0 §6.1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.10.0.md)

### E-DELIVERY-CONFLICT

콘텐츠 줄이 상호 배타적인 전달 플래그 `mono`, `os`, `vo` 중 둘 이상을 지정했습니다.

명세: [dsl 0.2.2 D7](/spec/)

### E-DELIVERY-FLAG-VALUE

전달 플래그(`mono`/`os`/`vo`)에 값이 지정되었지만, 전달 플래그는 값 없이 단독으로 씁니다.

명세: [dsl 0.2.2 D7](/spec/)

### E-DELIVERY-NARRATOR

`narrator` 콘텐츠 줄에 전달 속성이 붙었지만, 내레이션은 전달을 갖지 않습니다.

명세: [dsl 0.1.0 §12.1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.1.0.md)

### E-DEPENDS-CYCLE

둘 이상 플러그인의 `depends` 선언이 순환을 이루어, 활성화 순서를 결정할 수 없습니다.

### E-DEPENDS-UNRESOLVED

플러그인의 `depends`가 프로젝트에 설치되지 않은 다른 플러그인 id를 지정했습니다.

### E-DEPENDS-VERSION

플러그인의 `depends`가 설치된 플러그인을 지정했지만, 그 버전이 선언된 버전 범위를 만족하지 않습니다.

### E-DERIVE-TIER

관계가 `derive: true`로 선언되었지만 동시에 쓰기 `tier:`도 선언했으나, 파생 관계는 자체 쓰기 tier를 갖지 않습니다.

명세: [dsl 0.3.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.3.0.md), [dsl 0.3.0 §7.1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.3.0.md)

### E-DERIVE-UNDECLARED

`rules:` 항목의 머리가 `derive: true`로 선언되지 않은 관계(기본 관계, 예약 관계, 또는 엔티티 종류 이름)를 지정했지만, 파생 관계만 규칙의 머리가 될 수 있습니다.

명세: [dsl 0.3.0 §7.1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.3.0.md), [dsl 0.3.0 §3.1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.3.0.md)

### E-DERIVED-WRITE

콘텐츠가 `derive: true`로 선언된 관계를 assert하거나 retract했지만, 파생 관계는 `rules:`가 계산하므로 직접 쓸 수 없습니다.

명세: [dsl 0.3.0 §5](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.3.0.md)

### E-DIFF-INPUT

diff 한쪽을 읽거나 안전하게 임시 materialize하거나 완전한 프로젝트 모델로 만들 수 없습니다.

명세: [dsl 0.36.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.36.0.md), [dsl 0.36.0 §8](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.36.0.md)

### E-DIFF-MODEL

한쪽의 완전한 프로젝트 모델을 만들 수 없어 의미론적 diff를 수행할 수 없습니다.

명세: [dsl 0.36.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.36.0.md), [dsl 0.36.0 §8](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.36.0.md)

### E-DOLLAR-OUTSIDE-MATCH

매치 대상을 가리키는 `$`가 `<match>` 블록 밖에서 사용되었지만, 그 위치에서는 유효하지 않습니다.

명세: [dsl §8.2](/spec/)

### E-DOMAIN-DUP

프로젝트 자체의 `enums:`/`entities:` 선언(인라인 또는 `uses:`/`extends:`로 도달한 것)이 플러그인이나 코어가 이미 선언한 도메인 이름을 지정했지만, 도메인 이름은 정확히 하나의 출처만 가져야 합니다.

명세: [dsl 0.3.0 §3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.3.0.md), [dsl 0.9.0 D-D](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.9.0.md)

### E-DOMAIN-NAME-CLASH

한 이름이 `enums:` 도메인과 엔티티 종류로 함께 선언되었습니다. 한 문서 안이든, 문서가 합치는 여러 스키마 사이든 같습니다. enum과 엔티티 종류는 도메인 이름공간 하나를 함께 쓰므로, 그 이름이 값의 타입이 되는 곳마다 종류의 멤버가 enum의 멤버를 조용히 대신하게 됩니다.

### E-DOMAIN-UNKNOWN

콘텐츠 줄의 `emotion`/`action` 슬롯, 엔티티 속성, 또는 암묵적 `anchor` 읽기가 어떤 `enums:`/`entities:` 선언도 정의하지 않은 도메인을 지정했습니다.

명세: [dsl 0.9.0 D-C](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.9.0.md), [dsl 0.9.0 D-D](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.9.0.md)

### E-DUP-BRANCH

`<branch id>` 또는 `<hub id>`가 같은 에피소드 안에서 다른 브랜치나 허브가 이미 사용한 id를 반복했습니다. 브랜치와 허브 id는 하나의 고유성 영역을 공유합니다.

명세: [dsl §11.1](/spec/), [dsl §7.3.2](/spec/)

### E-DUP-LINE-CODE

같은 화자의 콘텐츠 줄 두 개가 동일한 `:line` `code=`를 공유했지만, (화자, 코드) 쌍은 고유해야 합니다 — 이는 voiceKey/번역 식별 조인 키입니다.

명세: [dsl §12](/spec/)

### E-DUP-TRACK

`<timeline>` 안의 두 `<track>`이 동일한 트랙 키를 공유했습니다.

### E-DUP-VOICEKEY

평문 텍스트(인라인 수식 마크업을 제거한 텍스트)가 다른 두 줄이 동일한 `voiceKey`로 컴파일되어, 하나의 녹음이 둘 다를 대신 말하게 됩니다 — 보통 `voiceKey` 템플릿에 `{prefix}`가 없어 문서 간에 충돌하기 때문입니다.

명세: [dsl 0.37.0 §3.6](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.37.0.md)

### E-ENGINE-IR-VERSION

엔진 매트릭스와 실행 아티팩트가 허용된 IR 버전 계열 밖에 있습니다.

명세: [dsl 0.33.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.33.0.md), [dsl 0.33.0 §5](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.33.0.md)

### E-ENGINE-MATRIX

엔진 기능 매트릭스를 읽을 수 없거나 형식이 잘못되었습니다.

명세: [dsl 0.33.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.33.0.md)

### E-ENGINE-OWNED-WRITE

`::set`이 `owner: engine`으로 선언된 상태 경로에 썼지만, 그 경로는 엔진이 쓰고 콘텐츠는 읽을 수만 있습니다.

명세: [dsl 0.22.0 §1.2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.22.0.md)

### E-ENGINE-SEMANTICS

선택한 엔진이 아티팩트에 필요한 의미 기능을 지원하지 않습니다.

명세: [dsl 0.33.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.33.0.md)

### E-ENTITY-KIND-CLASH

어떤 id가 서로 다른 두 엔티티 종류의 `members:`에 함께 나열되었지만, 한 종류가 다른 종류의 `subsetOf:`로 선언되지 않는 한 id는 정확히 하나의 종류에만 속해야 합니다.

명세: [dsl 0.3.0 §3.1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.3.0.md), [dsl 0.24.0 §3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.24.0.md)

### E-ENTITY-KIND-SHAPE

엔티티 종류 선언이 잘못되었습니다 — `members:`/`open:` 중 어느 것도 또는 둘 다 선언했거나, 알 수 없는 키가 있거나, 멤버를 두 번 이상 나열했거나, `labels:`/`add:` 항목이 그 종류에 없는 멤버를 지정하거나 `open:`/알 수 없는 종류를 대상으로 합니다.

명세: [dsl 0.3.0 §3.1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.3.0.md), [dsl 0.26.0 §2.3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.26.0.md), [dsl 0.27.0 §2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.27.0.md)

### E-ENTRY-ATTR

`<entry>` 속성의 형태가 잘못되었습니다 — 값이 인용된 문자열이 아니거나, `id`가 없거나 잘못되었거나, 문서 수준 `series:` 아래에서 `series=`/`order=`를 작성했습니다.

명세: [dsl 0.19.0 §3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.19.0.md), [dsl 0.19.0 §2.1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.19.0.md)

### E-ENTRY-ID-DUP

두 `<entry>` 요소가 같은 `id`를 공유했지만, 항목 id는 문서 전체(그리고 `check-project`에서는 프로젝트 전체)에서 고유해야 합니다.

명세: [dsl 0.19.0 §3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.19.0.md)

### E-ENTRY-SERIES-ORDER

두 `<entry>` 요소가 같은 `(series, order)` 위치로 귀결되었지만, 한 시리즈의 각 위치는 정확히 하나의 항목만 지정해야 합니다.

명세: [dsl 0.19.0 §2.1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.19.0.md), [dsl 0.19.0 §3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.19.0.md)

### E-ENTRY-UNREACHABLE

로어 항목의 `when` 자격 가드가 결코 참이 될 수 없음이 증명되어, 그 항목은 결코 제시될 수 없습니다.

명세: [dsl 0.20.0 §5](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.20.0.md)

### E-ENUM-DEFAULT-NOT-MEMBER

도메인의 `default:` 값이 그 도메인 자신이 선언한 멤버 중 하나가 아닙니다.

명세: [dsl 0.9.0 D-D](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.9.0.md)

### E-ENUM-EXITS-NOT-MEMBER

도메인의 `exits:` 목록이 그 도메인 자신이 선언한 멤버가 아닌 값을 지정했습니다.

명세: [dsl 0.9.0 D-D](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.9.0.md)

### E-ENUM-LABEL-NOT-MEMBER

도메인의 `labels:` 키가 그 도메인 자신이 선언한 멤버가 아닌 값을 지정했습니다.

명세: [dsl 0.24.0 §1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.24.0.md)

### E-ENUM-MISSING-SEMANTICS

도메인이 `default:` 또는 `exits:` 의미를 요구하는 슬롯(예: `anchor`)을 차지했지만, 그 도메인(또는 이를 대신하는 `entities:` 종류)이 둘 다 선언하지 않았습니다.

명세: [dsl 0.9.0 D-D](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.9.0.md)

### E-ENUM-UNEXPECTED-SEMANTICS

도메인이 `default:` 또는 `exits:`를 선언했지만, 그 도메인이 채우는 슬롯에는 그런 의미가 없습니다.

명세: [dsl 0.9.0 D-D](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.9.0.md)

### E-EXTENDS-RELATION-SIG

`extends` 기반 엔티티 종류, 관계, 또는 열거형이 자식 스키마에서 members/open 형태가 바뀌거나 기반 멤버가 빠지거나 기반 선언과 다르게 재선언되었지만, 재선언은 합법적인 상위집합 세분화여야 합니다.

명세: [dsl 0.3.0 §4.1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.3.0.md)

### E-EXTENDS-STATE-TYPE

가져온 스키마의 상태 경로가 더 깊은 `extends` 기반이나 씬의 인라인 `state:`에서 다른 `type`으로 재선언되었지만, 영속 상태는 안정적인 타입을 유지해야 합니다.

명세: [dsl §9.2](/spec/)

### E-FACT-DOMAIN

규칙 가드, 색인 조회, 또는 콘텐츠 조회가 관계 인자를 닫힌 종류 위를 순회하지 않는 변수와 비교/바인딩하거나, 그 인자의 선언된 도메인과 멤버를 공유하지 않는 종류와 비교/바인딩했습니다.

명세: [dsl 0.27.0 §3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.27.0.md), [dsl 0.3.0 §3.1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.3.0.md)

### E-FACT-EXCLUSIVE

`::assert{A(x)}`가 상호 배타적인 관계의 팩트가 이미 성립하는 인자를 대상으로 했지만, 두 관계는 그 인자들에 대해 배타적으로 선언되어 있습니다.

명세: [dsl 0.25.0 §1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.25.0.md)

### E-FACT-QUERY

팩트 조회 호출(`holds`, `count`, `countDistinct`, `validAt`)이 0.32에서 요구하는 목록 형식 `name('relation', ['arg', …])`을 사용하지 않았습니다.

명세: [dsl 0.32.0 §2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.32.0.md)

### E-FACT-TIER-WRITE

콘텐츠가 `app` 등급 기본 관계를 assert하거나 retract했지만, 이는 `app.*` 스칼라 상태와 마찬가지로 엔진 소유의 읽기 전용입니다.

명세: [dsl 0.3.0 §5](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.3.0.md), [dsl 0.3.0 §9.5](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.3.0.md)

### E-FLAG-VALUE

flag 속성(`<choice once>`/`exit`, `<objective optional>`, `<beat also>`)에 `true`/`false`가 아닌 값이 지정되었습니다. flag는 값 없이 단독으로 쓰며, 선택지에 쓴 beat/entry의 `once` 주기(`run`, `user` 등)는 거부됩니다.

명세: [dsl 0.28.0 §1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.28.0.md)

### E-FMT

포매터가 요청한 파일을 파싱하거나 보존할 수 없습니다.

명세: [dsl 0.36.0 §1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.36.0.md), [dsl 0.36.0 §8](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.36.0.md)

### E-FMT-CHECK

포매터 검사에서 요청한 파일의 비표준 바이트를 발견했습니다.

명세: [dsl 0.36.0 §1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.36.0.md), [dsl 0.36.0 §8](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.36.0.md)

### E-FRONTMATTER-SCHEMA

활성 플러그인이 선언한 문서 프런트매터 키의 값이 그 플러그인이 선언한 타입 형태와 일치하지 않습니다.

### E-GRAMMAR-NOT-ADMITTED

문서가 해당 맥락에서 그 문서 종류의 문법이 금지하는 구성을 사용했습니다 — 예를 들어 씬 안의 `<quest>`, 퀘스트 본문 안의 `<hub>`/`<timeline>`, 로어 `<entry>` 안의 연출/선택지 등입니다.

명세: [dsl 0.2.0 §3.3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.2.0.md), [dsl 0.2.0 §6.7](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.2.0.md), [dsl 0.19.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.19.0.md)

### E-HUB-NO-EXIT

`<hub>`가 결코 빠져나갈 수 없습니다 — 가드 없는(`when` 없는) `exit` 선택지도 없고, 자격 있는 집합이 확실히 비도록 모든 선택지가 `once`인 것도 아닙니다.

명세: [dsl §7.3.2](/spec/), [dsl §11.1.3](/spec/)

### E-IDENTITY-TEMPLATE

프로젝트의 `identity.lineId`/`identity.voiceKey` 템플릿이 알 수 없는 `{token}`을 지정했거나, 빈 문자열로 귀결됩니다.

명세: [dsl 0.8.0 §9](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.8.0.md)

### E-INERT-TITLE

본문의 `# ` 제목은 문서 제목이 아닙니다. 값을 프런트매터 `title:`에 적으세요. 문서 제목은 그것 하나뿐입니다.

명세: [dsl 0.37.0 §3.1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.37.0.md)

### E-INTERP-DEF

`{{@def}}` 보간이 하나의 독립된 표현식으로 인라인될 수 없습니다 — 본문에 확장 순환이 있거나, `$`를 읽거나, 파싱에 실패합니다.

### E-INTERP-UNTERMINATED

`{{…}}` 보간이 줄 끝 전에 닫히지 않았습니다.

명세: [dsl §7.6](/spec/)

### E-INTO-TARGET

`<choice>`의 `into=` 속성이 없거나, `run.<path>` 문자열 리터럴이 아니거나, 단독 `run`이나 `run.*`가 아닌 경로를 지정했습니다.

명세: [dsl 0.6.0 §2.2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.6.0.md)

### E-INTO-UNDECLARED

`<choice>`의 `into="run.<path>"`가 run 스키마에 선언되지 않은 경로를 지정했으며, 선언되지 않은 경로는 필드를 암묵적으로 만들 수 없습니다.

명세: [dsl 0.6.0 §2.2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.6.0.md)

### E-INTO-VALUE

`<choice>`의 `into=` 런-레코드 축약 표기에서 `value` 속성이 없거나, bool 경로에 `true`/`false` 리터럴이 아니거나, 대상 경로의 선언된 타입과 호환되지 않습니다.

명세: [dsl 0.6.0 §2.2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.6.0.md)

### E-IR-REMOVED-FIELD

`lute run`이 받은 실행 IR에 0.37에서 제거되거나 이름이 바뀐 필드(`addr`, `capabilityVersion`, `recordKey`, `shots`, 평탄화된 `wait`/`duration`/`delay`/`at`/`timeline` 스탬프, `provenance.injected` 등)가 남아 있거나, `position`이 없는 명령이 있습니다. 현재 툴체인으로 소스를 다시 컴파일하세요.

명세: [dsl 0.37.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.37.0.md), [dsl 0.37.0 §5.3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.37.0.md)

### E-JUMP-BACKWARD

`::jump{to}`가 문서 순서상 자신보다 앞서거나 같은 위치의 레이블을 가리킵니다 — 점프는 항상 앞으로만 가능합니다.

명세: [dsl 0.12.0](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.12.0.md)

### E-JUMP-UNDEFINED

`::jump{to}`가 문서 내 어떤 `::label`도 선언하지 않은 레이블을 가리킵니다.

명세: [dsl 0.12.0](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.12.0.md)

### E-KIND-MISSING

루트 문서가 `kind:` 프런트매터 키를 선언하지 않았고 매니페스트의 `defaults:`에도 지정된 것이 없어 문서의 kind를 확정할 수 없습니다.

명세: [dsl 0.2.0 §3.1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.2.0.md), [dsl 0.19.0 §2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.19.0.md), [dsl 0.10.0 §6.3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.10.0.md)

### E-KIND-NAME-CLASH

`entities:` 블록에서 같은 엔티티 kind 이름을 두 번 선언했거나, 두 스키마가 한 kind를 다르게 선언했거나, 한 이름이 엔티티 kind이자 관계로 동시에 선언되었습니다.

명세: [dsl 0.3.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.3.0.md)

### E-L10N-MODIFIERS

대사 번역문의 인라인 수식(이름, 스팬/리프 형태, 속성)이 원문 줄과 다르거나 마크업을 파싱할 수 없어, 해당 번역문을 병합하지 않습니다.

명세: [dsl 0.37.0 §6](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.37.0.md)

### E-LABEL-DUP

레이블 이름(`::label{name}`)이 문서 내 어딘가에서 두 번 이상 선언되었습니다 — 모든 레이블은 문서 전체에서 하나의 네임스페이스를 공유합니다.

명세: [dsl 0.12.0](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.12.0.md)

### E-LEGACY-CONTENT-SIGIL

콘텐츠 줄이 `@`로 대체된 예전 `:` 화자 기호를 사용합니다 — `@speaker{…}: text` 형식으로 쓰십시오.

명세: [dsl §7.1](/spec/)

### E-LINT-CONFIG

`lute.lint.yaml`가 잘못되었습니다 — 타입이 틀렸거나 알 수 없는 최상위 키, 잘못된 규칙 오버라이드, 또는 코어 규칙 id와 충돌하는 커스텀 규칙 id입니다.

### E-LINT-EXPR

린트 규칙의 `when` CEL 표현식을 해석할 수 없거나 타입이 잘못되어, 해당 규칙이 이 문서에서 건너뛰어집니다.

### E-LINT-RULE

플러그인 또는 커스텀 린트 규칙 선언이 잘못되었습니다(예: 코어 규칙의 id를 재사용).

### E-LOCALE-BUNDLE

`lute compile --locales`가 로케일 임포트 파일을 병합하지 못했습니다 — 구문 오류, `locale`이 비어 있는 행, 또는 같은 로케일에 대한 중복된 `lineId`입니다.

### E-LOGIC-CONTENT

로직 블록에 허용되지 않는 자식이 있거나(`<branch>`와 `<hub>`는 `<choice>`만, `<match>`는 `<when>`과 `<otherwise>`만 받습니다), `<choice>`·`<when>`·`<otherwise>`·`<track>`·`<reward>`가 속해야 할 블록 밖에 있거나, `<reward>`가 자체 종료 형식이 아닙니다.

명세: [dsl §7.3](/spec/), [dsl §7.3.2](/spec/), [dsl 0.16.0 §2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.16.0.md)

### E-LOWER-RECORD-FIELD

디렉티브의 선언적 `lower: { record, fields }` 매핑이 해당 레코드에 없는 대상 필드를 지정했거나, 쓸 수 없는 필드에 매핑했습니다.

### E-LOWER-RECORD-UNKNOWN

디렉티브의 `lower: { record: … }`가 선언적 로워링이 지원하는 스테이징 레코드 종류의 닫힌 집합 밖의 값을 지정했습니다.

### E-MANIFEST

`lute.project.yaml`를 매니페스트로 읽을 수 없습니다. 파싱되지 않거나, 매핑이 아니거나, `defaultProfile:`이 없거나, 값의 모양이 틀렸습니다.

명세: [dsl 0.28.0 §1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.28.0.md)

### E-MANIFEST-KEY

`lute.project.yaml`에 정의되지 않은 키가 있습니다(최상위, 프로필, `identity:`). 또는 다른 층이 소유한 키입니다(`terminal:` 같은 스키마 키, `defaults:` 아래에 써야 할 문서 키).

명세: [dsl 0.28.0 §1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.28.0.md)

### E-MATCH-DUP-OTHERWISE

`<match>`에 `<otherwise>` 분기가 두 개 이상 있지만, 최대 하나만 허용됩니다.

명세: [dsl §11.2](/spec/)

### E-MATCH-NO-SUBJECT

`subject=`가 없는 `<match>`에 `<when is=…>` 분기가 있어서, 그 리터럴을 비교할 주어가 없습니다. `<match>`에 `subject=`를 쓰거나, 분기를 `test=`로 쓰세요.

### E-MATCH-RELATION-SUBJECT

`<match subject>` 주어가 직접, 또는 확장되는 `@def`를 통해 `holds`/`count`/`validAt` 같은 사실 질의가 되었는데, match 주어로는 허용되지 않습니다.

명세: [dsl 0.27.0 §2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.27.0.md), [dsl 0.3.0 §8](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.3.0.md)

### E-MAYBE-UNSET

상태 경로가 아직 설정되지 않았을 수 있는 지점에서 읽혔습니다 — 기본값도, 지배하는 `::set`도, 이를 증명하는 가드도 없습니다.

명세: [dsl §9.4](/spec/)

### E-META-ID

문서의 `id:` 프런트매터 값이 점으로 이은 id가 아닙니다. id는 이름(글자, 숫자, `_`, `-`로 이루어지고 `-`로 시작하지 않는 이름)을 `.`으로 이은 것입니다.

명세: [dsl 0.30.0 §1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.30.0.md)

### E-META-MISSING

장면 정체성을 선언해야 하는 문서에 필수 프런트매터 키(예: `character`/`season`/`episode` 또는 `id:`)가 없습니다.

명세: [dsl 0.15.0 §2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.15.0.md), [dsl 0.15.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.15.0.md)

### E-META-PARSE

문서의 프런트매터가 올바른 YAML이 아니거나, YAML로 파싱되더라도 매핑 형태가 아닙니다.

### E-META-UNKNOWN-KEY

문서가 코어 키도 아니고 활성 플러그인이 소유하지도 않은 최상위 메타 키를 선언했거나, 다른 문서 kind 전용으로 예약된 키를 선언했습니다.

### E-META-VALUE

프런트매터 값의 형태가 잘못되었습니다 — 잘못된 `extra:` 매핑이나 키, 이름이 아닌 `series:`, 잘못된 `cast:`/`enums:`/`terminal:` 항목(`terminal:` 긴 형식에서 `when`/`persists` 외의 키, 또는 `true`/`false`가 아닌 `persists`), run 상태만 읽는 `terminal:`에 쓴 `persists: true`(그 결말은 run을 넘어 남을 수 없음), 또는 잘못된 `effects:` 플래그입니다.

명세: [dsl 0.15.0 §3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.15.0.md), [dsl 0.19.0 §2.1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.19.0.md), [dsl 0.23.0 §7](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.23.0.md), [dsl 0.29.0 §5](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.29.0.md)

### E-MISSING-ATTR

디렉티브에 선언된 스키마가 요구하는 속성이 빠져 있습니다.

### E-MOCK-SUBJECT

`--mock`/`mocks/*.yaml` 항목이 `file:`을 선언하지 않았거나, 존재하지 않거나 `.lute` 문서가 아닌 `file:` 경로를 지정했거나, 명령줄에 지정된 문서와 일치하지 않습니다.

명세: [dsl 0.10.0 §8](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.10.0.md)

### E-MONO-NO-POV

`{mono}` 줄이 있는 문서에서 시점이 정해지지 않았고(프런트매터 `pov:`도, 프로젝트 `defaults.pov`도 없습니다) 화자도 `monoSpeakers:`에 없습니다. 컴포넌트의 줄은 `::use` 지점마다 호출한 문서의 맥락으로 판정합니다.

명세: [dsl 0.37.0 §3.4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.37.0.md)

### E-MONO-POV

`{mono}` 줄의 화자가 문서의 실제 시점(`pov:`, 없으면 프로젝트의 `defaults.pov`)도 아니고 `monoSpeakers:` 목록에도 없습니다. 컴포넌트의 줄은 `::use` 지점마다 호출한 문서의 맥락으로 판정합니다.

명세: [dsl 0.37.0 §3.4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.37.0.md)

### E-NONEXHAUSTIVE

`<match>`에 `<otherwise>`가 없고 주어의 도메인이 `<when>` 분기들로 완전히 덮이지 않았습니다 — 메시지에 빠진 값이나 처음 덮이지 않은 구간이 표시됩니다.

명세: [dsl 0.18.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.18.0.md), [dsl §11.2](/spec/)

### E-OBJECTIVE-CONTRADICTION

하나의 `<quest>`에 속한 두 개의 필수 `<objective>`가 같은 상태 경로에 대해 서로 동시에 참일 수 없는 `done` 조건을 선언했습니다.

명세: [dsl 0.10.0 §5.2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.10.0.md)

### E-OBJECTIVE-ID-DUP

같은 `<quest>` 안에서 `<objective id>`가 두 번 이상 선언되었습니다.

### E-OBJECTIVE-ID-MISSING

`<quest>` 안의 `<objective>`에 `id`가 없습니다.

### E-OBJECTIVE-MISSING-DONE

`<objective>`의 `done` 완료 조건이 비어 있고 완료를 위임할 `quest=` 참조도 없습니다.

### E-OBJECTIVE-QUEST-DONE

`<objective>`가 `quest=` 하위 퀘스트 참조와 비어 있지 않은 `done=` 조건을 동시에 가지고 있는데, 이 둘은 함께 쓸 수 없습니다.

### E-OBJECTIVE-UNSATISFIABLE

필수 `<objective>`의 `done` 조건이 결코 참이 될 수 없거나, `<objective quest="…">`가 참조하는 하위 퀘스트 자체가 도달 불가능합니다.

명세: [dsl 0.4.0 §5.3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.4.0.md)

### E-OCCASION-GATE

`lute play` 단계가 `raisedWhen` 게이트가 거짓인 상태에서, 또는 스키마의 `terminal:` 조건이 이미 참인 상태에서 occasion을 발생시키려 했습니다 — 엔진은 실제로는 이를 발생시키지 않습니다.

명세: [dsl 0.27.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.27.0.md)

### E-OCCASION-UNKNOWN

비트 또는 엔트리의 `on=`이 어떤 리졸브된 플러그인도 선언하지 않은 occasion을 가리킵니다.

명세: [dsl 0.21.0 §2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.21.0.md)

### E-ON-NO-EVENT

`<on>`에 `event` 속성이 없습니다 — 모든 `<on>`은 하나의 명확한 이벤트에 연결되어야 합니다.

명세: [dsl 0.2.0 §4.1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.2.0.md)

### E-PATCH-CHECK

스테이징된 패치가 새로운 검사 또는 프로젝트 오류를 도입했습니다.

명세: [dsl 0.36.0 §5.2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.36.0.md), [dsl 0.36.0 §8](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.36.0.md)

### E-PATCH-EDIT

패치 편집이 잘못되었거나 대상 밖에 있거나 다른 편집과 겹치거나 escape-hatch 스팬 검사를 통과하지 못했습니다.

명세: [dsl 0.36.0 §5.1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.36.0.md), [dsl 0.36.0 §5.2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.36.0.md), [dsl 0.36.0 §8](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.36.0.md)

### E-PATCH-PRESERVE

패치가 요청된 보존 조건을 위반했습니다. 모호한 보상 또는 컴포넌트 일치도 포함됩니다.

명세: [dsl 0.36.0 §5.3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.36.0.md), [dsl 0.36.0 §8](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.36.0.md)

### E-PATCH-STALE

패치의 기준 프로젝트 또는 지정 파일 리비전이 현재 리비전과 다릅니다.

명세: [dsl 0.36.0 §5.2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.36.0.md), [dsl 0.36.0 §8](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.36.0.md)

### E-PATCH-TARGET

패치 대상이 없거나 모호하거나 편집 가능한 소스 앵커가 없습니다.

명세: [dsl 0.36.0 §5.1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.36.0.md), [dsl 0.36.0 §8](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.36.0.md)

### E-PATH-IDENT

이름에 이름 규칙(글자, 숫자, `_`, `-`로 이루어지고 `-`로 시작하지 않음) 밖의 문자가 있습니다: 상태 경로 세그먼트, quest·objective·entry·branch·hub·choice·mark id, relation·enum·엔티티 종류 이름, enum·엔티티 멤버. def와 def 파라미터는 `@name`이나 def 본문에서 그대로 읽히므로 식별자(글자나 `_`로 시작하고 글자, 숫자, `_`로 이어지는 이름)여야 합니다. 또는 조건식에서 식별자가 아닌 이름을 `.` 뒤에 썼습니다(`quest.zero-coke-001.state`). CEL은 이를 뺄셈으로 읽으므로, 메시지가 대괄호 표기(`quest["zero-coke-001"].state`)를 알려 줍니다.

명세: [dsl 0.30.0 §1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.30.0.md), [dsl 0.30.0 §2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.30.0.md), [dsl §8.4](/spec/)

### E-PERMISSION-BRIDGE

디렉티브가 유효한 `bridges` 권한 상한이 금지하는 브리지 서비스/오퍼레이션을 호출합니다.

### E-PERMISSION-DIRECTIVE

`::set`/`::assert`/`::retract` 또는 플러그인 디렉티브가 유효한 `directives` 권한 상한에 의해 금지됩니다.

### E-PERMISSION-FACT

사실 쓰기(`::assert`/`::retract`, 플러그인 효과, 또는 시드 팩트)가 유효한 `factWrites` 권한 상한이 금지하는 관계를 대상으로 합니다.

### E-PERMISSION-PROFILE

`--permission-profile <name>`가 `--project <DIR>`로 로드된 `lute.project.yaml` 없이 지정되어 프로파일을 확인할 수 없습니다.

### E-PERMISSION-QUEST

유효한 `quests` 권한 상한이 퀘스트 선언을 금지하는 곳에서 `<quest>`가 선언되었습니다.

### E-PERMISSION-REWARD

유효한 `rewards` 권한 상한이 보상 선언을 금지하는 곳에서 `<reward>`가 선언되었습니다.

### E-PERMISSION-STATE

`::set`, choice의 `into=`, 상태/플러그인 기본값 초기화, 또는 확정할 수 없는 플러그인 쓰기 경로가 유효한 `stateWrites` 권한 상한이 금지하는 경로를 대상으로 합니다.

### E-PERSIST-REMOVED

디렉티브가 제거된 `persist` 속성을 사용합니다 — `into=`만으로 run 팩트가 기록됩니다.

명세: [dsl 0.6.0 §2.2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.6.0.md)

### E-PLUGIN-ASSET-SEGMENT-TYPE

플러그인의 `assetKinds` export가 세그먼트 위치에서 허용되는 타입(`enum`, `int`, `double`, `string`, `providerRef`) 밖의 세그먼트 타입을 선언했습니다.

### E-PLUGIN-DUP-ACROSS

두 개의 활성 플러그인이 같은 디렉티브, 이벤트, 보상 종류, occasion, cast id, 또는 브리지 오퍼레이션을 선언했습니다. 먼저 선언한 플러그인의 것을 씁니다.

### E-PLUGIN-DUP-ID

플러그인 패키지가 하나의 익스포트 종류 안에서 같은 id를 두 번 이상 선언했습니다. 먼저 선언한 것을 쓰고, 프로젝트의 나머지 검사는 계속합니다.

### E-PLUGIN-INVALID-DIRECTIVE

플러그인 디렉티브 선언이 닫힌 어휘 밖의 `semantics:` 플래그를 사용했거나, 같은 속성 이름을 두 번 이상 선언했습니다.

### E-PLUGIN-IO

플러그인 익스포트 파일이나 디렉터리를 입출력 또는 인코딩 오류로 읽을 수 없습니다.

### E-PLUGIN-KEY

플러그인의 `plugin.yaml`이나 export 파일에 그 파일이 받지 않는 키가 있습니다(의도한 키를 함께 알려 줍니다. 예: `dependencies` → `depends`). 또는 `plugin.yaml`의 `kind:`가 `capability`가 아닙니다.

명세: [dsl 0.28.0 §1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.28.0.md)

### E-PLUGIN-MANIFEST

플러그인 패키지의 `plugin.yaml` 매니페스트가 없거나, 올바른 YAML이 아니거나, `id`/`version`/`kind`/`exports` 중 하나를 적지 않았습니다.

### E-PLUGIN-MISSING-ACTIVE

프로필이 활성화한 플러그인 `id`가 plugins 디렉터리에 설치되어 있지 않거나, 해당 패키지 로드에 실패했습니다.

명세: [dsl §11](/spec/)

### E-PLUGIN-MISSING-EXPORT

플러그인 매니페스트의 `exports:` 항목이 디스크에 존재하지 않는 경로를 가리킵니다.

명세: [dsl §4](/spec/), [dsl §11](/spec/)

### E-PLUGIN-OPTION-TYPE

플러그인 활성화 시 지정한 옵션 값이 매니페스트가 해당 옵션에 선언한 타입과 일치하지 않습니다.

명세: [plugin Appendix C1](/spec/)

### E-PLUGIN-OPTION-UNKNOWN

플러그인 활성화가 해당 플러그인 매니페스트에 없는 옵션 이름을 설정했습니다.

명세: [plugin Appendix C1](/spec/)

### E-PLUGIN-PARSE

플러그인 export 파일이 올바른 YAML이 아니거나 키의 값 모양이 틀렸습니다(줄과 그 키가 받는 모양을 알려 줍니다). 또는 디렉티브의 `effects.writes`/`effects.asserts`/`effects.retracts` 항목이 해당 디렉티브가 선언하지 않은 속성을 가리킵니다(또는 assert가 `_`를 사용합니다).

명세: [dsl 0.27.0 §2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.27.0.md), [dsl 0.27.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.27.0.md), [dsl 0.28.0 §7](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.28.0.md)

### E-PLUGIN-RESERVED-NAME

플러그인이 코어 언어가 가진 이름을 선언했습니다. 코어 문장(`set`, `assert`, `retract`, `accept`, `use`, `body`, `cut`), 코어 블록 태그(`scene`, `on`, `quest`, `objective`, `match`, `branch`, `hub`, `choice`, `when`, `otherwise`, `entry`, `beat`, `timeline`, `track`, `reward`, `return`), `lute.core` 디렉티브(`end`, `mark`, `bg` …)와 같은 이름의 디렉티브, 엔진 lifecycle 이벤트(`questComplete` …)나 play 스텝 키와 같은 이름의 이벤트·occasion, cast id `narrator`가 여기에 해당합니다. 선언한 파일과 줄을 가리킵니다.

예약된 이름 전체와 각 이름을 거부하는 자리, 대신 쓸 이름: [예약된 이름](/ko/reference/reserved-names/).

명세: [dsl §10](/spec/), [dsl 0.28.0 §1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.28.0.md)

### E-PLUGIN-RESERVED-STAMP-ATTR

코어가 아닌 플러그인의 `stampAttrs` export 또는 디렉티브의 `attrs`가 코어 stamp가 이미 소유한 속성 이름(`at`/`duration`/`delay`/`wait`/`timeline`/`provenance`/`source`)을 선언했습니다.

명세: [plugin §14](/spec/)

### E-PLUGIN-UNKNOWN-ASSETKIND

디렉티브가 활성 플러그인 어디에도 선언되지 않은 asset kind에 속성을 바인딩했습니다.

명세: [plugin §7](/spec/)

### E-PLUGIN-UNKNOWN-EXPORT

플러그인 매니페스트의 `exports:` 키가 export 종류가 아닙니다(did-you-mean을 붙입니다). 예전 철자 `rewardkinds`/`assetkinds`/`stampattrs`에는 새 이름 `rewardKinds`/`assetKinds`/`stampAttrs`를 알려 줍니다.

명세: [plugin §4](/spec/), [dsl 0.28.0 §6](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.28.0.md)

### E-PLUGIN-UNKNOWN-REWARD-TARGET

`rewardKinds:` 항목이 활성 플러그인 어디에도 선언되지 않은 provider를 `target: { provider: <name> }`로 지정했습니다.

명세: [dsl 0.16.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.16.0.md)

### E-PLURAL-FORM

`{{n:plural(…)}}` 힌트의 형태가 `|`로 구분된 따옴표 없는 단수형과 복수형이 아닙니다. 따옴표로 감싼 형태, `,` 구분자, 빠지거나 빈 형태가 여기에 해당합니다.

명세: [dsl 0.27.0 §7](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.27.0.md), [dsl 0.28.0 §5](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.28.0.md)

### E-PROFILE-EXTENDS-CYCLE

프로필의 `extends` 체인이 자기 자신으로 순환합니다.

명세: [dsl §11](/spec/)

### E-PROFILE-UNKNOWN

프로젝트가 `lute.project.yaml`에 선언되지 않은 프로필 이름을 선택했습니다.

명세: [dsl §11](/spec/)

### E-PROJECT-CONFIG

편집기가 문서의 `lute.project.yaml`을 로드하지 못했습니다(프로젝트 매니페스트가 잘못되었거나 읽을 수 없습니다).

### E-QUEST-ID-DUP

`<quest id=…>`가 이 문서 안, import 그래프, 또는 프로젝트 전체에서 이미 사용된 id를 반복합니다.

명세: [dsl 0.2.0 §6.3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.2.0.md)

### E-QUEST-ID-MISSING

`<quest>`에 `id` 속성이 없습니다.

명세: [dsl 0.2.0 §6.3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.2.0.md)

### E-QUEST-MULTI-PARENT

서브퀘스트가 서로 다른 두 부모 퀘스트의 `<objective quest=…>`에서 자식으로 참조되었지만, 퀘스트는 부모를 하나만 가질 수 있습니다.

### E-QUEST-REF-UNKNOWN

`<objective quest=…>`가 프로젝트의 어떤 퀘스트도 정의하지 않은 자식 퀘스트 id를 가리킵니다.

### E-QUEST-RESERVED-DECL

`state:` 선언 경로가 암묵적으로 선언된 예약 퀘스트 필드(`quest.<id>.*` / `objectives.<oid>.done`)와 충돌합니다.

명세: [dsl 0.2.0 §5.2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.2.0.md)

### E-QUEST-RESERVED-WRITE

`::set`가 엔진이 채우는 예약 경로(`quest.<id>.state`, `objectives.<oid>.done`, `entry.*` 경로, `prev.run.*`, `prev.season.*`, `clock.*`)에 값을 씁니다.

명세: [dsl 0.2.0 §5.4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.2.0.md), [dsl 0.19.0 §5](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.19.0.md)

### E-QUEST-TIER-MIX

서브퀘스트의 실효 `tier`가 부모 퀘스트의 tier와 다릅니다.

명세: [dsl 0.23.0 §6](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.23.0.md)

### E-QUEST-TREE-CYCLE

`<objective quest=…>`가 만드는 부모-자식 관계가 순환을 이루며, 퀘스트가 자기 자신을 자식으로 지정하는 경우도 포함됩니다.

### E-QUEST-UNREACHABLE

`<quest>`의 `start` 조건이 항상 거짓으로 결정되거나 `fail` 조건이 항상 참으로 결정되어, 해당 퀘스트가 결코 완료될 수 없음이 증명됩니다.

명세: [dsl 0.4.0 §5.3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.4.0.md)

### E-REF-ARG-TYPE

def를 호출하는 `@name(args)`에 def가 선언한 매개변수 타입과 맞지 않는 정적 타입의 인자를 전달했습니다.

명세: [dsl §8.1](/spec/)

### E-REF-ARITY

`@name(args)` 호출이 def가 선언한 매개변수 개수와 다른 개수의 인자를 전달했습니다.

명세: [dsl §8.1](/spec/)

### E-REF-TYPE

`@ref`가 채우는 CEL 슬롯·컴포넌트 인자·`{{…}}` 보간 위치와 맞지 않는 타입을 산출합니다 — 렌더링 불가능한 산출 타입이거나, `int`/`double`이 아닌 값에 숫자 형식 힌트를 붙인 경우를 포함합니다.

명세: [dsl §8](/spec/), [dsl §7.6](/spec/), [dsl 0.24.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.24.0.md)

### E-RELATION-ARITY

팩트 아톰(시드 `facts:` 항목, 규칙 본문·헤드 아톰, `::assert`/`::retract`, 또는 CEL 팩트 쿼리)이 관계가 선언한 것과 다른 개수의 인자를 전달합니다.

명세: [dsl 0.3.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.3.0.md), [dsl 0.3.0 §5](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.3.0.md)

### E-RELATION-DECL

관계가 `reserved: true`가 아닌데 `changedOn:`을 선언했거나, `changedOn:`이 선언되지 않은 occasion을 가리키거나, `excludes:` 항목이 인자 종류가 맞지 않는 관계를 가리키거나 대칭성 계약을 위반합니다.

명세: [dsl 0.25.0 §1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.25.0.md), [dsl 0.25.0 §6](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.25.0.md)

### E-RELATION-DOMAIN

관계가 스키마가 인식하지 못하는 필드, 알 수 없는 `tier`, 범위를 벗어나거나 중복된 `key:` 인덱스, 또는 선언되지 않은 엔티티 종류·enum·도메인을 가리키는 인자 도메인을 선언했습니다.

명세: [dsl 0.3.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.3.0.md)

### E-RELATION-DUP

`relations:` 블록에서 같은 관계 이름이 두 번 이상 선언되었습니다.

명세: [dsl 0.3.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.3.0.md)

### E-RELATION-EMPTY

관계가 `args:`를 선언하지 않았습니다.

명세: [dsl 0.3.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.3.0.md)

### E-RELATION-RESERVED-WRITE

관계가 `derive: true`와 `reserved: true`를 동시에 선언하여 서로 충돌하는 두 개의 쓰기 소유자를 갖게 되었습니다.

명세: [dsl 0.3.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.3.0.md), [dsl 0.3.0 §5](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.3.0.md)

### E-RELATION-UNKNOWN

팩트 아톰(시드, 규칙, `::assert`/`::retract`, 또는 CEL 팩트 쿼리)이 어떤 스키마도 선언하지 않은 관계를 가리킵니다.

명세: [dsl 0.3.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.3.0.md)

### E-REMOVED-ATTR

지시어가 손실 없이 바꿔 쓸 수 없는 제거된 속성(`::music{track}`, `::sfx{name}`)을 씁니다. 메시지가 대체할 방법을 알려 주며, 옮기는 작업은 손으로 해야 합니다.

명세: [dsl 0.37.0 §3.3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.37.0.md), [dsl 0.37.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.37.0.md)

### E-REMOVED-PROJECT-KEY

`lute.project.yaml`에 제거된 `sequence:` 키가 남아 있습니다. 이 키는 읽지 않으며 대체 키도 없습니다. 체인은 모두 `chapters:`에 선언합니다.

명세: [dsl 0.37.0 §2.2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.37.0.md)

### E-REMOVED-TAG

제거된 `<scene>` 태그를 썼습니다. 문서의 섹션은 `## ` 제목이고, 장면은 문서 그 자체입니다.

명세: [dsl 0.37.0 §2.2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.37.0.md)

### E-RENAME-LEDGER

이름 변경 항목이 잘못되었거나, 알 수 없는 종류/키를 사용하거나, 출발지 또는 목적지를 중복하거나, 정식 키 매핑이 아닙니다.

명세: [dsl 0.36.0 §3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.36.0.md)

### E-RENAME-LEDGER-CYCLE

이름 변경 항목이 체인, 자기 자신을 가리키는 고리, 또는 순환을 이룹니다.

명세: [dsl 0.36.0 §3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.36.0.md)

### E-RENAME-LEDGER-STALE

프로젝트 해석 후에도 원본 키가 남아 있거나 목적지 키가 없습니다.

명세: [dsl 0.36.0 §3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.36.0.md)

### E-RENAMED-ATTR

지시어나 콘텐츠 줄이 속성의 옛 이름(`::music{action}`, `::video{action}`, `::cg{action}`, `::label{id}`, 콘텐츠 줄의 `id=`)을 씁니다. 메시지가 새 이름을 알려 주고, `lute fix`가 바꿔 씁니다.

명세: [dsl 0.37.0 §2.2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.37.0.md), [dsl 0.37.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.37.0.md)

### E-RENAMED-DIRECTIVE

지시어가 옛 태그(`::auto`, `::cut`, `::next`, `::mark`)를 씁니다. 메시지가 새 태그(`::actor`, `::cg`, `::jump`, `::label`)를 알려 주고, `lute fix`가 바꿔 씁니다.

명세: [dsl 0.37.0 §2.2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.37.0.md), [dsl 0.37.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.37.0.md)

### E-RENAMED-TAG-ATTR

`<choice>`나 `<match>`가 속성의 옛 이름(`label=`, `on=`)을 씁니다. 메시지가 `text=` 또는 `subject=`를 알려 주고, `lute fix`가 바꿔 씁니다.

명세: [dsl 0.37.0 §3.5](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.37.0.md), [dsl 0.37.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.37.0.md)

### E-RESERVED-NAME

선언한 이름이 Lute가 스스로 쓰는 이름입니다. entity 멤버·def·시즌 이름이 state 루트이거나, 멤버 이름이 `unset`/`true`/`false`/`null`/`_`이거나, id가 `none`이나 CEL 리터럴이거나, state 경로(또는 경로가 되는 id)에 CEL 키워드가 있거나, 관계 이름이 CEL 호출이나 규칙 단어이거나, `cast:`에 `narrator`가 있거나, 멤버 목록에 숫자가 있습니다. 쓰는 곳에서 그 낱말로 읽히므로 선언에서 거부합니다. 메시지가 대신 쓸 이름을 제시하고, `lute --explain E-RESERVED-NAME`이 예약된 이름을 모두 보여 줍니다.

예약된 이름 전체와 각 이름을 거부하는 자리, 대신 쓸 이름: [예약된 이름](/ko/reference/reserved-names/).

명세: [dsl 0.28.0 §1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.28.0.md)

### E-RETRACT-WILDCARD-ASSERT

와일드카드(`_`)를 허용하는 `::retract` 패턴이 아닌 곳에서 관계 인자에 `_`를 사용했습니다.

명세: [dsl 0.3.0 §5](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.3.0.md)

### E-REWARD-ATTR

`<reward>` 요소의 형식이 잘못되었습니다: 비어 있거나 없는 `kind`, 부호 있는 정수나 올바른 `N..M` 범위가 아닌 `amount=`, 또는 objective 수준 reward에 쓰인 `outcome=`이나 `"failed"`가 아닌 `outcome=` 값입니다.

명세: [dsl 0.16.0 §2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.16.0.md), [dsl 0.16.0 §6](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.16.0.md)

### E-REWARD-DUP

한 퀘스트의 `<reward>` 두 개(퀘스트 자신의 것과 목표의 것을 모두 포함)가 같은 `id=`를 씁니다. 보상 id는 퀘스트 안에서 고유해야 합니다.

명세: [dsl 0.37.0 §3.5](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.37.0.md)

### E-REWARD-KIND

`<reward kind=…>` 값이 해석된 capability 스냅샷의 `rewardKinds` 어휘에 선언되지 않은 reward kind를 가리킵니다.

명세: [dsl 0.16.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.16.0.md), [dsl 0.16.0 §6](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.16.0.md)

### E-REWARD-TARGET

`<reward>`의 `target=`이 해당 reward kind의 target 계약을 위반합니다: 필수인데 없거나, 선언된 엔티티 종류의 멤버도 provider 카탈로그 id도 아닙니다.

명세: [dsl 0.26.0 §2.5](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.26.0.md)

### E-RULE-AGGREGATE-CYCLE

규칙의 `count(...)`/`countDistinct(...)` 집계가 그 규칙의 헤드 자신에 의존하는 관계를 읽지만, 집계는 자기 헤드의 순환 밖에 있는 관계만 읽을 수 있습니다.

명세: [dsl §9](/spec/)

### E-RULE-EXCLUSIVE

규칙이 특정 양의 본문 관계가 같은 인자에서 성립할 때만 헤드 관계를 도출하지만, 두 관계는 상호 배타적으로 선언되어 있어 모든 도출이 그 배타성을 위반합니다.

명세: [dsl 0.25.0 §1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.25.0.md)

### E-RULE-GUARD-DEF

규칙의 `cel("...")` 가드가 `@def`/`@def(args)` 참조를 전개할 수 없습니다 — def가 선언되지 않았거나, 인자 개수가 맞지 않거나, 가드에서 사용할 수 없는 형태입니다.

### E-RUN-OWNED-WRITE

`lute run`이 엔진 소유 상태 경로에 쓰거나 예약된 관계를 assert/retract하는 명령을 포함한 실행 IR을 받았습니다.

명세: [dsl 0.32.0 §5](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.32.0.md)

### E-SEASON-DECL

명세: [dsl 0.27.0 §5](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.27.0.md)

### E-SECTION-DUP

한 문서의 두 섹션이 같은 `{#id}`를 씁니다. 섹션 id는 문서 안에서 고유해야 합니다.

명세: [dsl 0.37.0 §3.1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.37.0.md)

### E-SECTION-SUFFIX

섹션 제목의 끝에 있는 `{#id}` 식별자 접미사가 형식에 맞지 않습니다.

명세: [dsl §3.1](/spec/)

### E-SEMANTICS-UNKNOWN

실행 아티팩트 또는 엔진 매트릭스가 알 수 없는 의미 ID를 지정합니다.

명세: [dsl 0.33.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.33.0.md)

### E-SET-OP-TYPE

`::set`의 복합 연산자(`+=`/`-=`/`*=`)가 선언된 타입이 `int`나 `double`이 아닌 경로를 대상으로 합니다.

명세: [dsl §7.3.4](/spec/)

### E-SET-SHAPE

`::set`의 형식이 잘못되었습니다: 경로 뒤에 유효한 대입 연산자(`=`/`+=`/`-=`/`*=`)가 없거나, `=` 대신 `==`를 사용했거나, state family 경로를 구체적인 키가 아닌 파라미터로 인덱싱했습니다.

명세: [dsl 0.27.0 §2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.27.0.md)

### E-SET-TYPE

`::set`의 우변 표현식에서 결정 가능한 타입이 해당 경로에 선언된 타입과 일치하지 않습니다.

명세: [dsl 0.10.0 §3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.10.0.md)

### E-STATE-COLLECTION

`state:` 선언이 경로에 컬렉션 타입(`list`/`record`/`map`)을 지정했지만, 작성자 state는 스칼라여야 합니다.

명세: [dsl 0.3.0 §3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.3.0.md)

### E-STATE-DECL

`state:` 선언이 잘못되었습니다: 문자열이 아닌 키, 알 수 없거나 불완전한 `type:`(`type:` 아래에 중첩되지 않은 `enum` 포함), 잘못된 `default:`/`per:` 형태, 또는 `state:` 자체가 맵이 아닙니다.

명세: [dsl 0.8.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.8.0.md)

### E-STATE-DECL-CONFLICT

같은 경로에 대한 두 `state:` 선언이 `type`, `default`, `per`, `owner` 중 하나에서 서로 다르며, 어느 쪽도 `extends:`로 다른 쪽을 재정의하지 않습니다.

명세: [dsl §2](/spec/)

### E-STATE-MAYBE-UNAVAILABLE

state 경로를 읽는 지점에 도달하는 어떤 선언된 `after:` 경로도 그 값을 설정함을 보장하지 않거나(오류 등급), 일부 경로에서만 설정됩니다(경고 등급).

명세: [dsl §4.3](/spec/)

### E-STATE-NAMESPACE

`state:` 경로가 인식되는 네임스페이스 루트인 `scene.`, `run.`, `user.`, `app.`, `season.` 중 하나로 시작하지 않습니다.

### E-STATE-REDECLARE

씬의 인라인 `state:`가 임포트된(`uses:`) 스키마가 이미 선언한 상태 경로를 선언하거나 재정의했으며, 씬은 이를 재선언해서는 안 됩니다.

명세: [dsl §9.2](/spec/)

### E-STATE-SHAPE-CYCLE

`state:` 셰이프가 자기 자신을, 직접적으로든 다른 셰이프를 거쳐서든 참조하여 순환을 이룹니다.

### E-STREAM-BODY

스트리밍 이어쓰기가 입력 끝에서 구문 도중에 끝났거나, 본문에 이어쓰기 본문이 담을 수 없는 프런트매터나 헤딩이 포함되어 있습니다.

### E-STREAM-CLOSED

이어쓰기 컴파일러가 이미 닫힌 뒤에 스트리밍 이어쓰기가 제출되었습니다.

### E-STREAM-PREFIX-CHANGED

스트리밍 이어쓰기에서 덧붙인 소스가 입력의 앞부분에 대해 이미 방출된 명령이나 상태를 바꾸게 됩니다.

### E-STREAM-TEMPLATE

스트리밍 이어쓰기의 템플릿이 섹션을 하나 이상 가진 씬이 아닙니다.

### E-STRING-ESCAPE

따옴표로 묶인 속성 값이 정의된 네 가지 이스케이프(`\"`, `\\`, `\n`, `\t`) 외의 백슬래시 이스케이프를 사용합니다.

명세: [dsl §4.4](/spec/)

### E-SUBQUEST-REARM

`<objective quest=…>`가 하위 퀘스트로 지정한 퀘스트가 `rearm=`을 선언합니다. 하위 퀘스트는 부모와 함께 활성화되므로, 부모가 끝난 뒤에는 재무장된 자식이 `unset`으로 남습니다.

명세: [dsl 0.27.0 §5](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.27.0.md), [dsl 0.28.0 §5](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.28.0.md)

### E-TAG-INLINE-BODY

블록의 본문(그리고 흔히 닫는 태그까지)이 여는 태그와 같은 줄에 쓰였습니다. 여는 태그, 본문의 각 줄, `</tag>` 닫는 태그는 각각 자기 줄에 있어야 합니다.

명세: [dsl §2.3](/spec/)

### E-TAG-NOT-ONE-LINE

`<tag …>` 여는 태그의 속성들이 문법이 요구하는 한 줄에 머무르지 않고 다음 물리적 줄로 넘어갑니다.

명세: [dsl §2.3](/spec/)

### E-TASK-TRAP

편집 작업 모음의 트랩이 패치를 예상대로 거부하거나 표시하지 못했습니다.

명세: [dsl 0.36.0 §6](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.36.0.md), [dsl 0.36.0 §8](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.36.0.md)

### E-TEMPLATE

비트 템플릿이 잘못 사용되었습니다: `<beat use=>`가 컴포넌트를 지정하지 않거나 `beat:` 헤더가 없는 컴포넌트를 지정했거나, 템플릿의 `beat:` 헤더가 형식에 맞지 않거나 헤더 매개변수에 허용되지 않는 값을 주었거나, 컴포넌트가 사용 자리의 키와 이름이 같아 어떤 사용도 넘길 수 없는 매개변수(`component`나 `when`, 비트 템플릿이면 `title`, `once`, `id` 같은 `<beat>` 머리 키)를 선언했거나, `::body`가 템플릿의 최상위 밖에 나타났습니다.

명세: [dsl 0.27.0 §6](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.27.0.md), [dsl 0.28.0 §1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.28.0.md)

### E-TEMPORAL-ARG

내러티브 시간 값(`now()` 등)이 다른 내러티브 시간 값과의 순서 비교나 `validAt`의 세 번째 인자가 아닌 다른 자리(단독 값, 산술, 인덱싱, 필드 접근, 리스트 리터럴, 또는 `!=`)에 사용되었습니다.

명세: [dsl 0.3.0 §6](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.3.0.md)

### E-TEST-FILE

`*.test.yaml`의 `file:`이 존재하지 않는 문서를 가리킵니다.

### E-TEST-KEY

`*.test.yaml`에 인식되지 않는 최상위 또는 `expect:` 레벨 키가 있거나, 키가 문자열이 아닙니다.

### E-TEST-LORE

테스트의 `file:`이 로어 문서를 가리키는데, 로어 문서는 플레이되지 않고 조회되므로 테스트는 대신 표시할 대상(`entry:`/`entries:`, `beat:`)을 지정하거나 `expect:`로 판정해야 합니다.

명세: [dsl 0.22.0 §5](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.22.0.md)

### E-TEST-NEEDLE

`*.test.yaml`의 `transcriptContains`/`transcriptLacks` 니들이 프로젝트 캐스트 밖의 화자, 어떤 트랜스크립트 줄에도 나타나지 않는 속성, 또는 도메인 밖의 값을 지정해, 표시된 줄과 결코 일치할 수 없습니다.

### E-TEST-NO-EXPECT

`*.test.yaml`이 인식되는 `expect:` 키를 하나도 선언하지 않아, 이 테스트는 아무것도 단언하지 않으므로 통과할 수 없습니다.

### E-TEXT-ESCAPE

인라인 텍스트 수식에서 정의되지 않은 백슬래시 이스케이프를 사용했습니다.

명세: [dsl §3.6](/spec/)

### E-TEXT-MODIFIER

인라인 텍스트 수식의 중첩이 잘못되었거나 스팬 또는 속성 본문이 닫히지 않았습니다.

명세: [dsl §3.6](/spec/)

### E-TIME-RESOLUTION

작성된 시간 값(클립 `at`, `duration`, `delay`, 또는 `<timeline duration>`)이 밀리초보다 더 정밀한 소수 자릿수를 가집니다.

명세: [dsl 0.10.0 §10.1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.10.0.md)

### E-TIMELINE-CONTENT

`<timeline>` 또는 `<track>` 본문에 클립·스테이징 요소가 아닌 콘텐츠가 포함되어 있습니다.

명세: [dsl §7.4](/spec/)

### E-TIMELINE-DURATION

`<timeline duration>`이 클립들의 최대 해석된 끝 시점보다 낮게 명시적으로 설정되어, 타임라인 자체의 내용을 잘라내게 됩니다.

명세: [dsl §11.4](/spec/)

### E-TRACE-ACCEPT

`--accept`/`accept:` 항목이 알 수 없는 퀘스트 id를 지정했거나, `start` 술어를 가진(선언적으로 활성화되어 accept가 필요 없는) 퀘스트를 지정했거나, 부모의 `<objective quest=…>`가 참조하는(부모를 통해 활성화되는 no-start 자식) 퀘스트를 지정했습니다.

명세: [dsl 0.4.0 §4.3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.4.0.md), [dsl 0.4.0 §4.4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.4.0.md)

### E-TRACE-BEAT

`lute trace --beat <id>`가 `kind: lore`가 아닌 문서를 대상으로 하거나, 문서가 선언하지 않은 `<beat>` id를 지정했습니다.

명세: [dsl 0.23.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.23.0.md)

### E-TRACE-CHOICE

`--choose` 항목이 알 수 없는 분기/허브 id를 지정했거나, 그 분기/허브의 알 수 없는 선택지 id를 지정했거나, 도달 시점에 선택지의 가드가 거짓으로 판정되었습니다.

명세: [dsl 0.4.0 §4.3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.4.0.md), [dsl 0.4.0 §4.4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.4.0.md)

### E-TRACE-ENTRY

`lute trace --entry <id>`가 `kind: lore`가 아닌 문서를 대상으로 하거나, 문서가 선언하지 않은 `<entry>` id를 지정했습니다.

명세: [dsl 0.19.0 §8](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.19.0.md)

### E-TRACE-EVENT

`--event`/`events:` 항목이 엔진이 자체적으로 파생시키는 내장 생애주기 이벤트(`questActive`, `questComplete`, `questFailed`)를 지정했으며, 이는 사용자가 직접 발생시킬 수 없습니다.

명세: [dsl 0.4.0 §4.3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.4.0.md), [dsl 0.4.0 §4.4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.4.0.md)

### E-TRACE-MOCK-FACT

`--fact`/`facts:` 항목이나 테스트의 `expect.facts`/`expect.notFacts` 원자가 정형화된 사실 패턴으로 파싱되지 않거나, 알 수 없는 관계·잘못된 자릿수·다른 문서의 인자를 지정했습니다.

명세: [dsl 0.4.0 §4.3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.4.0.md)

### E-TRACE-MOCK-PARSE

`--mock`/`mocks/*.yaml` 파일이 잘못되었습니다 — 유효하지 않은 YAML이거나, 맵이 아니거나, 인식되지 않는 최상위 키가 있거나, `state:`/`facts:`/`choose:`/`events:`/`quests:` 섹션의 형식이 맞지 않습니다.

명세: [dsl 0.4.0 §4.3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.4.0.md), [dsl 0.10.0 §8](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.10.0.md)

### E-TRACE-MOCK-TYPE

목 또는 테스트 시드의 리터럴(`--state <path>=<literal>`, `state:`, `quests:`)이나 테스트의 `expect.state` 값이 해당 경로의 예약된 도메인이나 선언된 타입과 맞지 않거나 — `{ domain: K }`, `{ entity: K }` 또는 enum 경로라면 그 멤버가 아니거나 — 응답한 브리지 결과에 콘텐츠가 읽는 필드가 빠져 있습니다.

명세: [dsl 0.4.0 §4.3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.4.0.md), [dsl 0.24.0 §5](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.24.0.md), [dsl 0.27.0 §2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.27.0.md)

### E-TRACE-MOCK-UNDECLARED

`--state <path>=…` 시드가 클록이 파생시키는(시드 불가능한) 경로를 지정했거나, 해석된 스키마에 선언되지 않은 경로를 지정했거나, 문서 내 어디에서도 읽지 않는 경로를 지정했거나, 어떤 디렉티브도 읽거나 쓰지 않는 호출/필드를 지정한 브리지 응답입니다.

명세: [dsl 0.4.0 §4.3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.4.0.md), [dsl 0.24.0 §1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.24.0.md), [dsl 0.24.0 §5](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.24.0.md)

### E-TRACK-KEY

`<track>`이 `subject`, `channel`, `subject`+`property` 쌍 중 어느 것도 선언하지 않아 식별 키가 없습니다.

명세: [dsl §7.4](/spec/)

### E-UNCLASSIFIED

본문 줄이 Lute 구문이 아닙니다 — 콘텐츠 줄, 디렉티브, `::set`, 알려진 블록 중 어느 것도 아니거나 — 블록이 올 수 없는 자리(예: 섹션 안의 `<quest>`)에 있습니다.

명세: [dsl 0.5.0 §2.1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.5.0.md)

### E-UNCLOSED-TAG

블록이 닫히지 않았습니다 — 자기 `</tag>`보다 먼저 파일 끝, `## ` 제목, 또는 바깥 블록의 닫는 태그에 이르렀습니다 — 또는 `</tag>`가 열려 있는 어떤 블록도 닫지 않습니다.

명세: [dsl §5](/spec/), [dsl §7.3](/spec/)

### E-UNDECLARED

CEL 슬롯, `::set` 대상, 또는 규칙 가드가 어떤 스키마에도 선언되지 않은 상태 경로를 읽거나 씁니다.

명세: [dsl §7.3.4](/spec/), [dsl §9.4](/spec/)

### E-UNDECLARED-REF

`@name` 보간 또는 가드 참조가 어떤 스키마에도 선언되지 않은 `def`를 지정합니다.

명세: [dsl §8.1](/spec/)

### E-UNKNOWN-ATTR

콘텐츠 줄이나 디렉티브가 콘텐츠 줄 문법 또는 해당 디렉티브 선언이 인식하지 못하는 속성 키를 가지고 있습니다.

명세: [dsl 0.1.0 §7.1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.1.0.md)

### E-UNKNOWN-DIRECTIVE

`::directive`가 코어나 활성화된 플러그인 어디에도 선언되지 않은 태그를 지정합니다.

### E-UNKNOWN-EVENT

`<on event="…">`가 내장 생애주기 이벤트도, 기능으로 선언된 월드 이벤트도 아닌 이벤트를 지정합니다.

명세: [dsl 0.2.0 §4.5](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.2.0.md)

### E-UNKNOWN-ID

`providerRef` id를 참조하는 속성이 고정된 제공자 카탈로그에 없는 id를 지정합니다.

### E-UNKNOWN-KIND

문서의 `kind:` 프런트매터 키 값이 `scene`, `quest`, `lore` 중 어느 것도 아닙니다.

명세: [dsl 0.2.0 §3.1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.2.0.md), [dsl 0.19.0 §2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.19.0.md)

### E-UNSET-LITERAL

CEL 가드 슬롯이 unset일 수 있는 유한 도메인 주체를 실제로는 다른 문자열 리터럴인 `'unset'`과 비교했으며, 이는 DSL의 실제 unset 센티널을 가장 흔히 잘못 표기한 형태입니다.

명세: [dsl 0.5.2 §2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.5.2.md)

### E-UNSET-UNCOVERED

`<match>` 주체가 unset일 수 있는데(스키마 `default`가 없는 `run.`/`user.`/`app.` 경로. 분기의 `scene.choices.*` 기록을 포함한 `scene.*` 경로는 경로마다 `E-MAYBE-UNSET`으로 판정합니다) `unset`을 매칭하는 분기나 `<otherwise>`로 커버되지 않았습니다.

명세: [dsl §11.2](/spec/)

### E-USES-CYCLE

문서의 `uses:`/`extends:` 임포트가 순환 구조를 이룹니다.

### E-USES-DUP-DEF

같은 임포트 깊이의 두 피어 임포트가 같은 `def` 이름을 서로 다르게 선언했습니다.

### E-USES-DUP-RELATION

같은 임포트 깊이의 두 피어 임포트가 같은 관계 또는 열거형 이름을 서로 다르게 선언했습니다.

### E-USES-DUP-STATE

같은 임포트 깊이의 두 피어 임포트가 같은 상태 경로를 서로 다르게 선언했습니다.

### E-USES-NOT-FOUND

`uses:`/`extends:` 임포트가 해석하거나 읽을 수 없는 경로를 지정합니다.

### E-USES-PARSE

`uses:`/`extends:` 임포트가 가리키는 대상 문서 자체에 파싱 또는 프런트매터 오류가 있습니다.

### E-VALIDAT-DERIVED

`validAt`이 파생 관계에 대해 사용되었으며, 그 규칙 클로저는 CEL 가드를 포함하고 있어 단일한 잘 정의된 타임스탬프를 유지하지 않습니다.

명세: [dsl §8](/spec/)

### E-WHEN-LITERAL-DOMAIN

`<when is="…">` 리터럴이 주체의 결정된 유한 도메인 밖에 있습니다 — 다른 열거형 멤버(오타), 도메인과 맞지 않는 숫자/불리언 리터럴, 또는 결코 unset이 될 수 없는 주체에 대한 `unset`입니다.

명세: [dsl 0.4.0 §5.2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.4.0.md), [dsl §6.3](/spec/)

### E-WHEN-PATTERN

`<when>` 항목에 `is` 리터럴 패턴도 `test` 가드도 없지만, 둘 중 하나는 반드시 있어야 합니다.

명세: [dsl §7.3.1](/spec/)

### E-WHEN-RANGE

`<when is="…">`의 항목이 `..`를 포함하지만 형식이 잘못되었거나(`..`, `a..b`, `1...2` 등) 비어 있는 범위 리터럴입니다(예: `3..1`).

명세: [dsl 0.18.0 §2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.18.0.md)

### E-WHEN-UNSET-SUBJECT

`<when is="unset">` 분기는 `<match>` 주어가 단순한 상태 경로여야 하는데, 이 주어는 표현식입니다.

명세: [dsl §7.3.1](/spec/)

### E-WRITE-CONFLICT

`<timeline>`의 서로 다른 `<track>`에 있는 두 `<clip>`이 겹치는 시간에 겹치는 상태 대상을 씁니다.

명세: [dsl §11.4](/spec/)

## 경고

### W-ASSET-PLACEHOLDER

에셋 id가 출시 전에 확정해야 할 자리표시자(placeholder)처럼 보입니다.

### W-BEAT-ONCE-RUN-USER

장면 비트의 `once`가 기본값 `run`이지만 `when`이 사용자 계층 상태만 읽어서 `once`를 명시적으로 쓰지 않으면 매 실행마다 다시 나타납니다.

명세: [dsl 0.22.0 §13](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.22.0.md), [dsl 0.23.1](/spec/)

### W-BEAT-PRIORITY-TIE

`select: first` 방식의 한 계기(occasion)에서 두 개 이상의 비트가 같은 `priority`를 가지고 동시에 자격을 얻을 수 있어, 어느 것이 선택되는지가 파일 순서에 따라 정해집니다.

명세: [dsl 0.27.0 §11](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.27.0.md), [dsl 0.22.0 §13](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.22.0.md)

### W-BEAT-SHADOWED

`select: first` 방식의 계기(occasion)에서, 더 앞 순서이며 항상 자격이 있고 소진되지 않는 같은(또는 지정 없는) 대상의 비트가 항상 먼저 선택되므로 이 비트는 결코 선택될 수 없습니다.

명세: [dsl 0.21.0 §5](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.21.0.md)

### W-BEAT-SPENT-AT-START

비트의 `spentBy`가 플레이 시작 시점(모든 상태 경로가 기본값이고 시드 팩트만 있으며, 각 퀘스트는 `start`가 성립할 때까지 `unset`인 때 — `start="true"` 퀘스트는 이미 `active`)에 이미 성립합니다. 흔히 `spentBy`를 "성립하는 동안 반복"으로 읽었거나, 예전 `when`의 뒤집힌 `!holds(…)`를 그대로 옮긴 경우입니다. `spentBy` 비트는 조건이 한 번 성립하면 소진된 채로 남으므로, 이 비트는 한 번도 제시되기 전에 소진됩니다. 메시지는 고쳐 쓸 형태를 알려 줍니다: 조건이 성립하지 않는 동안 재생할 비트라면 `when: "!(…)"`(반복하려면 `once: false`와 함께)로 씁니다.

명세: [dsl 0.27.0 §5](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.27.0.md), [dsl 0.28.0 §6](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.28.0.md)

### W-BEAT-UNRAISED

비트가 시계의 `raise:` 맵이 raise하는 계기에 응답하는데, 그 `when`은 시계가 그 계기를 raise하지 않는 곳에서만 참이 될 수 있습니다. 하루의 마지막 슬롯이 아닌 슬롯을 위한 `dayEnd` 비트나, 실행이 시작된 날의 뒤쪽 슬롯을 위한 `dayStart` 비트가 그렇습니다. 또는 마지막 `dayEnd`에서만 참이 될 수 있는데, 시계를 끝내는 advance는 `clock.ended`가 참이 된 뒤에 그것을 raise하므로, `clock.ended`와 함께 참이 되는 `terminal:`이 이미 게임을 끝낸 뒤입니다. 그래서 그 비트는 재생되지 않습니다. 그것이 참이 되는 곳에서 raise되는 계기에 응답하게 하거나 `when`을 바꾸세요. 마지막 `dayEnd`라면 시계가 끝나기 전에 raise하거나, 그 비트가 재생된 뒤에만 참이 되도록 `terminal:`을 쓰세요. 실행이 시작되는 위치에서 참이 되는 비트도 경고됩니다. 거기서는 어떤 `advance:`도 멈추지 않으므로 시계가 slot 계기도 `dayStart`도 raise하지 않지만, 엔진이 실행을 시작할 때 그 계기를 raise한다면 비트는 재생됩니다. 그렇다면 시계에 `raiseAtStart: true`를 선언하세요. 경고가 사라집니다. 그렇지 않다면 그것이 참이 되는 곳에서 raise되는 계기에 응답하게 하세요.

명세: [dsl 0.28.0](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.28.0.md)

### W-BRANCH-ID-SHARED

한 프로젝트의 두 문서가 각각 같은 id의 `<branch>`나 `<hub>`를 선언합니다. id는 한 문서 안에서만 유일하면 되지만, 플레이나 테스트의 `choose:`는 메뉴를 id만으로 가리키므로 키 하나가 두 메뉴에 모두 답합니다(결정 목록을 쓰면 두 메뉴가 이어서 소비합니다).

명세: [dsl 0.28.0 §7](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.28.0.md)

### W-CAST-ABSENT

대사 줄의 화자에게 `present:` 조건을 선언한 캐스트 항목이 있지만, 그 줄을 감싸는 조건들이 해당 조건을 보장하지 않습니다.

명세: [dsl 0.24.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.24.0.md)

### W-CATALOG-STALE

`providerRef` id가 고정된 프로바이더 카탈로그에서 찾을 수 없으며, 이는 id 자체의 오류라기보다 스냅샷이 오래되었거나 오프라인이기 때문일 수 있습니다.

명세: [dsl §7.2](/spec/)

### W-CHAPTER-ORDER

`select: sequence` 계기에서는 프로젝트 `chapters:`의 체인이 한 번의 raise 안의 순서입니다. 그런데 나열된 장면이 자체 `priority:`를 써서, 체인이 정한 순서를 벗어난 자리에서 재생됩니다. 장면의 `priority:`를 지우거나, 체인의 `scenes:`에서 장면을 옮기세요.

명세: [dsl 0.28.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.28.0.md)

### W-CHAPTER-STALL

프로젝트 `chapters:`의 체인에 나열된 장면의 자체 `when:`이 영영 거짓으로 남을 수 있습니다. 이야기가 끝내 쓰지 않을 수도 있는 state를 읽거나, 닫히는 시계 창을 읽는 경우입니다. 체인의 계기가 raise되는 어느 순간에도 맞지 않는 `when`(실행이 시작된 날이 필요한 `dayStart` 체인의 장면. 단, 엔진이 거기서 raise하므로 시계가 `raiseAtStart: true`를 선언했다면 제외)이나, 앞 장면이 늦게 재생된 뒤로는 이후 어떤 raise도 맞추지 못하는 `when`(끝나는 시계의 마지막 날의 슬롯)이 여기에 해당합니다. 그리고 다음 장면의 `after:`(체인이 쓴 것이든 장면이 직접 쓴 것이든)가 그 장면을 기다리므로, 장이 거기서 멈출 수 있습니다. 이후의 raise가 여전히 맞추는 시계 조건은 체인을 늦출 뿐이고, 다른 `owner: engine` state에 대한 조건은 엔진이 참으로 만들 몫이라 둘 다 보고하지 않습니다. 멈추는 것은 체인뿐입니다. `chapters:` 밖의 장면에 직접 쓴 `after:`는 그 자체가 "그 장면을 기다린다"는 진술이므로 경고하지 않습니다. 그 장면을 건너뛰어도 된다면 다음 장면이 그 앞 장면을 따르게 하세요(`after: visited("<앞 장면>")`; 건너뛸 수 있는 장면은 우선순위가 높아서 조건이 맞으면 여전히 먼저 재생됩니다). 반드시 재생되어야 한다면 이야기가 그 조건을 참으로 만들도록 하거나, 그 `when`이 이후의 raise에서도 참이 되게 하세요.

명세: [dsl 0.28.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.28.0.md), [dsl 0.29.0 §3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.29.0.md)

### W-CODE-AFTER-END

같은 직선 흐름의 본문 안에서 `::end` 지시어 뒤에 콘텐츠가 이어지지만, 그 지점에서 이미 진행이 종료되어 이후 내용은 실행될 수 없습니다.

명세: [dsl 0.8.0](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.8.0.md)

### W-CODE-AFTER-JUMP

같은 직선 흐름의 본문 안에서 조건 없는 `::jump` 지시어 뒤에 콘텐츠가 이어지지만, 그 점프가 본문을 벗어나므로 이후 내용은 실행될 수 없습니다.

명세: [dsl 0.12.0](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.12.0.md)

### W-COMPONENT-INSTANCE-UNTAGGED

`::use`에 명시적인 `instance`가 없어 과도기 위치 기반 폴백을 사용합니다.

명세: [dsl 0.36.0 §2.2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.36.0.md)

### W-COMPONENT-UNVERIFIED

단독 컴포넌트 검사에 범위 안의 호출자가 없습니다. 프로젝트가 해석되지 않았거나, 해석된 프로젝트 안에 이 컴포넌트를 `::use`하는 문서가 없어서, 검사 결과가 컴포넌트 자신의 프런트매터와 본문만을 다룹니다.

명세: [dsl 0.10.0 §9](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.10.0.md)

### W-DEADLINE-BEFORE-DONE

`on=` 목표(objective)의 `by=` 마감(`until=` 없이)이 `done` 조건이 판정되기 전에 반드시 참이 되어, 완료되기 전에 마감으로 인해 목표가 실패 처리됩니다.

명세: [dsl 0.24.0 §2.1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.24.0.md)

### W-DEADLINE-BEFORE-WINDOW

목표의 `done`이 `by=` 마감이 이미 성립한 시계 위치에서만 참이 될 수 있어(대개 `visited`로 읽는 비트의 `when`이 마감 뒤에 열림), 완료되기 전에 마감으로 인해 목표가 실패 처리됩니다.

명세: [dsl 0.28.0](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.28.0.md)

### W-DEADLINE-NEVER

목표의 `by=` 마감이 결코 성립할 수 없어(대개 끝이 있는 시계의 끝을 지난 시점) 목표를 실패시키지 못합니다.

명세: [dsl 0.24.0 §2.1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.24.0.md)

### W-DEF-UNUSED

선언된 `@def`가 프로젝트의 콘텐츠, 다른 def, 규칙 가드 어디에서도 `@이름` 형태로 참조되지 않습니다.

명세: [dsl 0.24.0](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.24.0.md)

### W-DERIVE-NO-RULES

`derive: true`로 선언된 관계에 이를 생성하는 규칙이 없어서, 문법적으로는 유효하지만 항상 비어 있습니다 — 대개는 규칙 head 이름의 오타입니다.

명세: [dsl 0.3.0 §7.1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.3.0.md)

### W-DISPLAY-NAME-DUP

서로 다른 두 화자가 같은 대사창 표시 이름(캐스트 항목의 `name:` 또는 컴포넌트 `::use{name=}`)을 사용하여 플레이어가 둘을 구분할 수 없습니다.

명세: [dsl 0.26.0 §2.8](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.26.0.md)

### W-DOMAIN-UNREAD

선언된 도메인을 어떤 활성 구성 요소도 읽지 않습니다 — 지시어 속성, 대사 슬롯, 상태 경로, `relations:` 인자, `per:`/`subsetOf:` 패밀리, 규칙·조건 질의 중 어느 것도 이를 참조하지 않아 아무것도 강제하지 않습니다.

명세: [dsl 0.10.0 §11.1](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.10.0.md)

### W-ENTRY-REF-UNKNOWN

`entry.<id>.read` 참조가 프로젝트의 어떤 로어 문서도 선언하지 않은 엔트리 id를 가리킵니다.

명세: [dsl 0.19.0 §5](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.19.0.md)

### W-ENTRY-WRITE-REREAD

한 실행에서 다시 읽힐 수 있는 엔트리(조회 엔트리, `once`가 없는 엔트리 비트, 실행보다 짧은 `once`, `spentBy` 엔트리, `once: run|user`가 없는 `for=` 엔트리)가 상태를 씁니다. 엔트리의 쓰기는 한 실행의 첫 읽기에만 적용됩니다. 메시지가 모양에 맞는 해결책(같은 속성의 `<beat>`, `once="run"`, 또는 경고도 없애는 `when="!entry.<id>.read"` 가드)을 알려 줍니다.

명세: [dsl 0.26.0 §8](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.26.0.md), [dsl 0.19.0 §6](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.19.0.md)

### W-EXIT-INERT

대사 줄의 `action`이 `action` 도메인에서 선언된 퇴장(exit) 값을 가리키지만, 대사 줄에서는 무대 연출 효과가 없어 캐릭터가 무대에 그대로 남습니다.

명세: [dsl 0.10.0 §11.2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.10.0.md)

### W-FACT-GUARANTEED

가드의 관계형 질의(`holds`/`count`)가 그 지점에 도달하는 모든 경로에서 항상 참이 되어, 해당 조건이 불필요합니다.

명세: [dsl 0.20.0 §5](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.20.0.md)

### W-INTO-SET-DUP

`<choice>` 항목이 경로를 `::set`으로 쓰면서 동시에 같은 경로를 `into=`로도 기록하여 이중으로 기록됩니다.

명세: [dsl 0.6.0 §2.2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.6.0.md)

### W-L10N-MISSING

컴파일된 대사 레코드에 로케일라이제이션 번들이 선언한 로케일의 텍스트가 빠져 있습니다.

명세: [dsl 0.8.0 §7](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.8.0.md)

### W-LINE-CODE-UNTAGGED

콘텐츠 라인에 명시적인 화자별 `code`가 없어 컴파일러 할당을 사용하고 있습니다.

명세: [dsl 0.36.0 §2.2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.36.0.md)

### W-LUTE-VERSION-STALE

문서 프런트매터의 `luteVersion` 값이 있지만 현재 툴체인의 DSL 버전과 달라서, 예전 예제에서 그대로 복사된 것으로 보입니다.

명세: [dsl 0.6.1 §3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.6.1.md)

### W-META-LEGACY

프런트매터 문서가 `id:`와 함께 예전 장면 식별 키(`character`, `season`, `episode` 등)를 작성했지만, 이제 장면 식별은 `id:`가 담당하므로 예전 키는 제거해야 합니다.

명세: [dsl 0.15.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.15.0.md)

### W-OBJECTIVE-HIDDEN

필수(`!optional`) 목표(objective)의 `visibleWhen` 표시 조건이 결코 참이 될 수 없어, 완료 판정에는 여전히 관여하면서도 결코 표시되거나 추적될 수 없습니다.

명세: [dsl 0.4.0 §5.3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.4.0.md)

### W-OBJECTIVE-STRANDED

필수 목표가 시계로 제한된 비트로만 완료될 수 있고 그 비트들의 시간 창이 모두 닫힐 수 있지만, 목표에 `until=`이나 `by=` 기한이 없습니다. 기한과 `failedBy` 처리기를 추가하거나 기한 실패를 처리하는 콘텐츠를 작성하세요. run 계층 퀘스트에서는 다음 실행에서 다시 시도하므로 정보성 진단입니다.

명세: [dsl 0.31.0 §3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.31.0.md)

### W-OTHERWISE-DEAD

`<match>`의 `<otherwise>` 항목이, 앞선 무조건 `is` 항목들이 이미 주어의 전체 범위를 다 덮고 있어서 결코 도달할 수 없습니다.

명세: [dsl 0.4.0 §5.2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.4.0.md)

### W-OVERLAP-ARMS

두 `<when>` 항목이 같은 값에 대해 모두 일치함이 증명되어, 먼저 일치하는 항목이 우선하는 규칙상 나중 항목은 결코 도달할 수 없습니다.

명세: [dsl §11.2](/spec/), [dsl 0.18.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.18.0.md)

### W-PROJECT-INERT

강제 지정된 `--project` 루트에서는 이 매니페스트가 적용되지 않으며 원래대로라면 다르게 해석되었을 것이므로, 그 설정이 어떤 문서에도 적용되지 않습니다.

### W-QUEST-HANDLER-DEAD

퀘스트가 결코 실패할 수 없어서 `<on event="questFailed">` 핸들러가 실행되지 않습니다 — `fail` 조건도, `by=` 마감이 있는 필수 목표도, 실패 가능한 필수 하위 퀘스트도, 실패를 전파할 상위 퀘스트도 없습니다.

명세: [dsl 0.22.0 §7](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.22.0.md)

### W-QUEST-NEVER-ACCEPTED

수락 기반(accept-driven) 퀘스트를 어떤 `::accept{quest=…}`도 지정하지 않고 `accept="external"`도 아니어서, 프로젝트 안에서 이 퀘스트를 수락하는 곳이 없습니다.

명세: [dsl 0.24.0 §2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.24.0.md), [dsl 0.25.0 §5](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.25.0.md)

### W-QUEST-REARM-CONSTANT

퀘스트의 `rearm=` 조건이 상수입니다(`"true"`, `"false"`, 또는 상수로 접히는 def나 비교). 거짓에서 참으로 바뀌는 일이 없으므로 퀘스트가 재무장되지 않습니다.

명세: [dsl 0.27.0 §5](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.27.0.md), [dsl 0.28.0 §5](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.28.0.md)

### W-QUEST-REF-UNKNOWN

예약된 `quest.<id>.state` / `quest.<id>.objectives.<oid>.done` 등의 참조가 프로젝트의 어떤 퀘스트 문서도 정의하지 않은 퀘스트 id나 목표 id를 가리킵니다.

명세: [dsl 0.5.1 §1.4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.5.1.md)

### W-QUEST-STATE-HAS

퀘스트의 상태는 항상 값을 가지고 있어(활성화 전에는 `unset`, 이후에는 실제 상태) `has(quest.<id>.state)` 조건은 항상 참이므로 아무것도 검증하지 않습니다.

### W-QUEST-TIER-IMPLICIT

`tier=`를 지정하지 않아(기본값인 사용자 계층으로, 실행 간에도 유지되는) 퀘스트인데, `start`/`fail`/목표 조건이 모두 실행 계층 상태만 읽고 있어 매 실행마다 초기화될 의도였던 것으로 보입니다.

### W-RELATION-TIER-IMPLICIT

저장되는(`derive: true`가 아닌) 관계에 `tier:`가 없어 실행 계층이 됩니다. 그 사실은 새 실행마다 `facts:` 시드(있다면)에서 다시 시작하고, `reserved: true` 관계라면 엔진이 기록한 사실은 잊힙니다. 그대로 두려면 `tier: run`을 쓰고, 실행보다 오래 남길 사실이면 `user`, `app`, `season:<name>`을 씁니다.

명세: [dsl 0.28.0 §5](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.28.0.md)

### W-RELATION-UNREAD

선언된 비예약 관계가 쓰이기는 하지만(단언, 시드, 파생) 어떤 조건, 규칙 본문, def에서도 읽히지 않아 기록된 사실이 아무 영향도 주지 않습니다.

명세: [dsl 0.24.0](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.24.0.md)

### W-REWARD-DOUBLE-CREDIT

퀘스트 핸들러의 `::set`이 `<reward kind="…" credits=…>`가 지급 시 이미 적립하는 것과 같은 경로에 쓰고 있어 보상이 두 번 지급됩니다.

명세: [dsl 0.23.0 §8](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.23.0.md)

### W-SEASON-UNGATED

`once: season:<name>`인 비트(또는 `start`가 있는 `tier="season:<name>"` 퀘스트)의 `when`(또는 `start`)이 그 시즌의 `live` 조건을 함의하지 않습니다. `once`는 비트가 얼마나 오래 소진된 상태로 남는지만 정하므로, 시즌이 한 번도 열리지 않았어도 비트가 재생됩니다. 시즌의 `live` 조건(또는 그것을 읽는 def)을 `when`에 더하세요.

명세: [dsl 0.28.0 §7](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.28.0.md)

### W-SLOT-CONTENTION

한 퀘스트 안의 두 필수 run 계층 목표가 같은 하나의 시계 위치에서만 완료될 수 있고, 두 목표를 완료하는 비트가 모두 시간을 전진시킵니다. 하나를 표시하면 다른 목표가 완료될 유일한 슬롯도 소비됩니다.

명세: [dsl 0.31.0 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.31.0.md)

### W-SPENT-BY-REVERSIBLE

비트의 `spentBy`가 한 번 성립한 뒤 다시 거짓이 될 수 있습니다. `::retract` / `::assert`나 지시어가 선언한 효과가 되돌릴 수 있는 팩트, 시즌의 상태·팩트·퀘스트(시즌이 열릴 때마다 초기화됨), 또는 `rearm`이 `unset`으로 되돌리는 퀘스트를 읽는 경우입니다. 그런데 `once`를 쓰지 않았으므로 비트는 그 런이 끝날 때까지 소진된 채로 남아, 계기가 올 때마다 조건을 새로 판정하는 것과 다르게 동작합니다. 메시지는 고쳐 쓸 형태를 알려 줍니다: 매번 새로 판정하려면 `once: false`와 `when: "!(…)"`, 시즌이라면 `once: season:<name>`, 일부러 소진 상태를 유지하려면 `once: run`을 씁니다(`once`를 쓰면 이 경고는 나오지 않습니다).

명세: [dsl 0.28.0 §6](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.28.0.md)

### W-STAGE-ABSENT

대사 줄이나 `::actor`가, 이미 무대를 떠나(퇴장 지시어, `::bg` 장면 전환, 또는 `::clear`) 다시 등장하지 않은 캐릭터를 대상으로 하여 연출이 불가능합니다.

명세: [dsl 0.22.0 §12](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.22.0.md)

### W-TEMPLATE-DOT-PARAM

비트 템플릿의 `when:`이나 `spentBy:` 머리가 멤버를 param으로 쓴 경로 조각(`user.bond.@who`)으로 읽습니다. 머리에서는 동작하지만 컴포넌트 본문은 이 철자를 거부합니다. 둘 다 받아들이는 `user.bond[@who]`로 적으세요.

명세: [dsl 0.28.0 §3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.28.0.md)

### W-TEMPLATE-OVERRIDE

`<beat use=…>`가 자기 `when=`을 적어 템플릿의 `when:`을 통째로 대신하므로, 템플릿의 조건이 더 이상 비트를 막지 않습니다(그 조건만 읽던 인자는 쓰이지 않습니다). 두 조건을 모두 use의 `when=`에 적거나, 템플릿에 덧붙일 param을 두고(`when: "<조건> && (@only)"`) `when=` 대신 그 param을 넘기세요.

명세: [dsl 0.28.0 §3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.28.0.md)

### W-TERMINAL-PERSISTENT

스키마의 `terminal:`이 새 run이 유지하는 상태(`visited(…)`, `user.*`, `app.*`, `entry.<id>.everRead`, user tier 퀘스트나 relation)를 읽습니다. 한 번 성립하면 어떤 새 run도 이어서 플레이할 수 없습니다. 결말이 의도적으로 run을 넘어 남는다면(로그라이크의 영구 결말) `terminal: { when: "<condition>", persists: true }`로 그렇다고 적으면 경고가 사라집니다.

명세: [dsl 0.28.0 §2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.28.0.md), [dsl 0.29.0 §5](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.29.0.md)

### W-TEXT-BRACKET-LABEL

`<choice>` label 전체가 Ink의 대괄호 숨김처럼 `[…]`로 감싸여 있습니다. Lute는 label을 적힌 그대로 보여 주므로 버튼에 대괄호가 나타납니다. 대괄호 없이 적으세요. 앞에 태그가 오고 텍스트가 이어지는 label(`[Persuasion] Step closer`)은 경고하지 않습니다.

명세: [dsl 0.28.0 §2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.28.0.md)

### W-TEXT-COMMENT-LIKE

대사 텍스트나 선택지 label에 ` // …` 주석이 있거나 Ink `#tag`로 끝납니다. `: ` 뒤의 텍스트는 리터럴이라 플레이어에게 그대로 보입니다. 주석은 한 줄을 따로 쓰는 `// …`이고, Lute에는 줄 태그가 없습니다.

명세: [dsl 0.28.0 §2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.28.0.md)

### W-TEXT-GLUE

대사 텍스트나 선택지 label에 Ink 글루 `<>`가 있습니다. Lute는 줄을 잇지 않고 `: ` 뒤의 텍스트는 리터럴이라 플레이어에게 `<>`가 보입니다. 문장 전체를 한 줄에 적으세요.

명세: [dsl 0.28.0 §2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.28.0.md)

### W-TEXT-LOOKS-LIKE-REF

대사 줄 전체 텍스트가 선언된 def나 컴포넌트 매개변수의 `@이름`과 정확히 같아서, 값으로 해석되지 않고 리터럴 문자열 `"@이름"`으로 출력됩니다.

명세: [dsl §7.6](/spec/)

### W-TEXT-SINGLE-BRACE

대사 텍스트나 선택지 label에 다른 언어의 마크업으로 읽히는 한 겹 중괄호가 있습니다. 상태 경로나 def(`{run.oil}`), Yarn `{$var}`나 `{0}` 자리표시자, Ink 조건부 텍스트(`{cond: text}`)나 대안(`{~a|b}`)입니다. 한 겹 중괄호는 언제나 리터럴이고 백슬래시로 이스케이프되지 않으므로(`\{run.oil\}`은 백슬래시까지 그대로 출하됨), 보여 줄 중괄호는 그대로 적습니다. 보간은 `{{run.oil}}`이고, 조건부 텍스트는 guard를 단 줄이나 `<match>`로 씁니다.

명세: [dsl 0.28.0 §2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.28.0.md)

### W-TIMELINE-CLIPS

`<timeline>`의 한 트랙에 클립이 12개를 초과하여 있어, 검사기가 분리를 권장합니다.

명세: [dsl §11.4](/spec/)

### W-TIMELINE-TOTAL

`<timeline>`의 전체 트랙을 합친 클립 수가 40개를 초과하여, 검사기가 분리를 권장합니다.

명세: [dsl §11.4](/spec/)

### W-TIMELINE-TRACKS

`<timeline>`에 트랙이 8개를 초과하여 있어, 검사기가 분리를 권장합니다.

명세: [dsl §11.4](/spec/)

### W-TRACE-MOCK-UNPRODUCIBLE

주어진 `--fact`/모의 YAML 사실의 관계가 어떤 작성된 생성자로도 만들어질 수 없다고 판정되어, 이를 시드로 사용한 진행은 실제 도달 가능한 플레이에 대해 아무것도 증명하지 못합니다.

명세: [dsl 0.6.1 §4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.6.1.md)

### W-WHEN-TEST-LITERAL

`<when test="…">` 항목이 CEL 리터럴 비교로 작성되어 있는데, 같은 의미를 `is=` 패턴 형태로 더 명확히 표현할 수 있고 검사기도 이를 직접 분석할 수 있습니다.

명세: [dsl 0.18.0 §3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.18.0.md), [dsl §7.3.1](/spec/)

### W-WIP

`check-project --wip`에서, 가드나 목표가 필요로 하는 관계를 만들어 내는 콘텐츠가 아직 쓰이지 않았다는 이유만으로 죽어 있습니다(시드, `::assert`, 규칙, 예약 선언이 없거나, 바인딩되지 않은 `@param`을 가진 컴포넌트 `::assert`만 있음). 메시지는 `--wip` 없이 붙을 오류 코드(`E-ARM-DEAD`, `E-BEAT-UNREACHABLE`, `E-ENTRY-UNREACHABLE`, `E-OBJECTIVE-UNSATISFIABLE`)를 밝힙니다.

명세: [dsl 0.23.0 §10](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.23.0.md), [dsl 0.26.0 §2.6](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.26.0.md)
