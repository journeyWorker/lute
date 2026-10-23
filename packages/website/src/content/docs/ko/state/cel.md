---
title: CEL 표현식
description: 제한된 Lute-CEL 프로필 — 모든 조건과 대입의 기반이 되는 폐쇄 환경, CEL이 나타나는 위치, 허용되는 팩트 질의 표면, 인라인 CEL 문자열로 컴파일되는 방식을 설명합니다.
---

Lute의 모든 조건과 모든 `::set` 우변은 [CEL](https://cel.dev)입니다. 즉, 종료가 보장되고 부작용이 없으며, **Lute-CEL 프로필**이라는 극도로 제한된 부분집합 아래에서 해석됩니다. CEL은 언어를 *튜링 완전하지 않고 전체적으로 만들기* 위한 장치입니다. 어디에도 호스트 언어(JS, Lua, …) 평가가 없습니다.

## CEL이 나타나는 위치

CEL 텍스트는 모든 가드와 값 슬롯에 들어갑니다:

- `<match subject="S">` subject, `<when test="…">` 가드, `<choice when="…">` 및 `when=` 콘텐츠 줄 게이트
- `::set{path = celExpr}` 우변, 그리고 0.24.0부터 `::set` 자체의 `when="…"` 가드
- 퀘스트 `<quest start="…" fail="…">` 및 `<objective done="…" visibleWhen="…" by="…" until="…">` 술어, `<on when="…">` 핸들러
- 비트 `when:` / `when=` 자격 조건
- 캐스트 엔트리의 `present:` 조건(0.24.0, [캐스트](/ko/language/dialogue-and-cast/#캐스트) 참고)

```lute
<match subject="scene.affect.elena">
  <when is="3..">
    @elena: …
  </when>
  <when test="@chose('couch', 'ignore')">
    @elena: …
  </when>
  <otherwise>
    @elena: …
  </otherwise>
</match>
```

`<match>` 안에서 토큰 `$`은 subject 표현식 `S`로 해석되며, 그 밖에는 나타나면 안 됩니다. `@ref` / `@fn(args)`는 평가 전에 인라인 CEL로 확장되는 **컴파일 타임 매크로**이며, 매개변수는 호출 인자에서 바인딩됩니다. 괄호가 적용되어 AST에 안전합니다.

## 폐쇄 환경

Lute-CEL 환경이 정확히 제공하는 것은 CEL 연산자, 리터럴, 리스트 리터럴, 삼항 연산자 `?:`, `in` 멤버십 연산자, `has()` 매크로, 그리고 단일 확장인 `has(<path>)`(상태 경로가 대입되어 있으면 true)입니다. **0.32에서는 `has(select)`가 식별자 경로를 검사합니다. 식별자가 아닌 키의 존재 여부에는 `"key" in map`을 사용합니다.** 다른 함수는 존재하지 않습니다. 따라서 컴프리헨션(`map`/`filter`/`exists`/`all`), `size`, `matches`는 정적 오류(`E-CEL-PROFILE`)입니다. 상태 경로 세그먼트는 이름입니다(문자, 숫자, `_` 또는 `-`이며 `-`로 시작하지 않음). `defs` 이름과 매개변수 이름은 식별자이므로 그대로 읽습니다. 식별자가 아닌 세그먼트는 인용하여 씁니다. `quest["zero-coke-001"].state`처럼 작성하며, `.` 뒤에 쓰면 뺄셈으로 읽힙니다(`E-PATH-IDENT`).

연산자에는 0.24.0부터 정수 `%`가 포함됩니다(그 전에는 `E-CEL-PROFILE`이었습니다). `run.day % 7 == 0`과 `wd: "run.day % 7"`이라는 def는 추론에 의해 `int`로 타입 지정되며 올바릅니다. 두 피연산자는 모두 정수여야 합니다. `int`가 아닌 피연산자(`'a' % 2`, bool 경로 또는 비교)나 소수 리터럴(`run.day % 2.5`)은 `E-CEL-TYPE`입니다. `%`는 절단된 정수 나머지이므로 `-7 % 3 == -1`입니다. 런타임에 소수 값이나 0인 제수가 들어오면 오류가 아니라 결과가 unknown이 됩니다.

```lute check
---
kind: scene
id: tower.stairs
state:
  run.day: { type: int, default: 1 }
  user.deaths: { type: int, default: 0 }
defs:
  weekday: "run.day % 7"
---

## The Stairs

@narrator{when="@weekday == 0"}: A full week on the stairs.
@narrator: This is your {{user.deaths:ordinal}} death.
```

조건은 선언된 상태 경로와, 선언 없이 엔진이 쓰는 예약 경로를 읽습니다. 예약 경로에는 퀘스트 슬롯 `quest.<id>.state`, `quest.<id>.activatedAt`, `quest.<id>.objectives.<o>.done`, 그리고 0.24.0부터 `quest.<id>.failedBy` 및 `quest.<id>.objectives.<o>.failed`가 있습니다. 또한 로어 엔트리 플래그 `entry.<id>.read`(런 계층)와 `entry.<id>.everRead`(사용자 계층, 0.22.0부터), 0.23.0부터 `prev.run.<path>`는 이전 런이 끝났을 때 각 선언된 `run.<path>`가 가졌던 값입니다(항상 미설정일 수 있음; [이전 런](/ko/state/state-model/#이전-런) 참고). 그리고 스키마가 [시계](/ko/language/clock/)를 선언한 경우 `clock.index`, `clock.weekday`, `clock.weekdayLabel`(0.24.0)이 있습니다. 이 중 어느 것도 `::set` 대상이 될 수 없습니다 — [엔진이 쓰는 경로](/ko/state/state-model/#엔진이-쓰는-경로)를 참고하세요.

관계형 계층이 사용 중이면 조건은 제한된 술어 표면을 통해 팩트 데이터베이스도 읽을 수 있습니다: `holds('rel', ['args|_'])`(현재 유효한 멤버십, 어떤 슬롯이든 `_`일 수 있음), `count('rel', ['args|_']) OP n`(서로 다른 유효 튜플 수), `countDistinct('rel', ['args|_'], int) OP n`(0.24.0부터: 리스트 요소가 `'_'`인 0부터 시작하는 위치의 서로 다른 값. 따라서 `countDistinct('sawAt', ['_', '_', '_'], 0)`은 서로 다른 목격자를 세고, `count('sawAt', ['_', '_', '_'])`은 목격 횟수를 셉니다), 그리고 `validAt('rel', ['args|_'], T)`(기본 관계에 대한 과거 시점 형식)입니다. `T`는 `narrativeTime` 표현식이며, 실제로는 작성자가 이름을 지정할 수 있는 유일한 표현식인 `quest.<id>.activatedAt`입니다([퀘스트와 씬](/ko/language/quests-and-scenes/) 참고). 집계 아래의 조인은 [파생 관계](/ko/state/facts-and-datalog/)로 표현하며, 여러 관계를 쓰는 `count`는 절대 사용하지 않습니다. `<match on>` subject는 스칼라/enum 경로로 유지해야 합니다. 팩트 질의는 가드 안에서만 사용하며, 어떤 팩트 질의도 규칙의 `cel()` 가드 안에 나타날 수 없습니다.

0.21.0부터 모든 조건 슬롯은 `visited('<scene id>')`도 호출할 수 있습니다. 이 함수는 해당 씬이 이 세이브에서 제시된 뒤 한 번이라도 제시되었으면 true이며, 씬의 `after:`가 읽는 것과 같은 방문 집합을 사용합니다. 인자는 프로젝트의 씬을 하나 지정하는 문자열 리터럴 하나이거나, 0.23.0부터 정식 `<document id>.<beat id>`로 지정한 [번들 비트](/ko/language/beats/#비트-번들)일 수 있습니다(그렇지 않으면 `E-CONN-UNKNOWN-NODE`가 프로젝트 전체 `lute check`에서 발생합니다). [퀘스트와 씬](/ko/language/quests-and-scenes/#퀘스트가-씬-및-계기를-만나는-방식)을 참고하세요.

## `per:` 멤버의 존재 확인

`per:` 패밀리는 그 종류의 멤버로 인덱싱되며, 이름이 CEL 식별자가 아닌 멤버(`look-around`)는 따옴표로 씁니다. `user.stepRank["look-around"]`처럼요. 그런 멤버가 설정되었는지 물으려면 멤버십 형식을 쓰세요(0.49.0).

```lute
@guide{when="'look-around' in user.stepRank"}: You have already looked around.
@guide{when="!('look-around' in user.stepRank)"}: Take a look around first.
```

`'<member>' in <family>`(문자열 리터럴과 `per:` 패밀리)는 `<family>.<member>`의 존재 가드이며, 점으로 이은 멤버에 `has()`를 쓴 것과 정확히 같습니다. 참이면 가드된 본문의 나머지에서 그 멤버가 대입된 것으로 증명되고, 부정은 그 반대를 증명합니다. 패밀리의 멤버가 아닌 리터럴은 `<family>["<literal>"]`을 읽을 때와 같은 진단을 받습니다. `lute trace`와 런타임은 이를 `has()`로 평가합니다.

`has(user.stepRank["look-around"])`는 계속 `E-CEL-PARSE`이며, 이제 ``— `has()` takes a field; for a member that is not an identifier write `'<m>' in <family>` `` 힌트가 붙습니다.

존재 테스트를 언급하는 모든 메시지는 같은 방식으로 표기합니다. 루트 뒤의 모든 세그먼트가 식별자이면 `has(<path>)` / `!has(<path>)`, 마지막 세그먼트가 식별자가 아닌 `per:` 멤버이면 `'<m>' in <family>` / `!('<m>' in <family>)`입니다. 그 밖에 식별자가 아닌 세그먼트가 있는 경로에는 가드 표기가 없으므로, 메시지는 `after:`와 `default:` 해결책만 알려 줍니다.

## 보간은 CEL이 아닙니다

콘텐츠 텍스트의 `{{…}}` 보간은 CEL 표현식이 아닙니다. 보간은 하나의 값을 지정합니다. 상태 경로(종류의 멤버를 위해 실행되는 비트에서는 멤버로 인덱싱된 `per:` 패밀리인 `{{user.bond[occasion.target]}}`도 해당), 그 안에서의 `{{occasion.target}}` 자체, def `@ref` / `@ref(args)`, 또는 `userName`입니다. 계산된 값에는 def가 필요합니다. 0.24.0부터 보간은 콜론 뒤에 하나의 **형식 힌트**를 포함할 수 있습니다. `{{user.deaths:ordinal}}`은 `1st`, `2nd`, `3rd`, `11th`, `21st`, `111th`로 렌더링됩니다([보간](/ko/language/dialogue-and-cast/#보간) 참고). 힌트는 `ordinal`, `ordinalWord`, `cardinalWord`, `plural(one|other)`이며 이는 `int` 또는 `double`에 사용하고, `capitalize`, `start`, `indefinite`이며 텍스트에 사용합니다. 다른 이름은 모두 `E-CEL-PROFILE`입니다. 문자열, enum 또는 bool 경로/def, 혹은 `{{userName}}`에 숫자 힌트를 사용하면 `E-REF-TYPE`이고, 숫자 경로나 bool에 텍스트 힌트를 사용해도 마찬가지입니다.

## 컴파일 대상

`@ref` 매크로는 **컴파일 타임**에 확장됩니다. 그 결과인 인라인 CEL 문자열은 평면 명령 레코드 아티팩트에 담기며, 엔진이 **런타임**에 평가합니다. 모든 것이 평면 레코드와 CEL 문자열로 디슈거됩니다 — 이것이 컴파일러↔엔진 계약입니다. 미설정 센티넬은 CEL `null`입니다(`!has(path)` 또는 `<when is="unset">` arm으로 검사). 문자열 `'unset'`은 절대 아닙니다. 따라서 해당 문자열과 비교하면 `E-UNSET-LITERAL`입니다. 유일한 예외는 `quest.<id>.state`입니다. 이 값은 항상 대입되어 있고 `unset`이 첫 생명 주기 멤버이므로, `quest.q.state == 'unset'`은 “아직 시작하지 않음”을 검사하는 올바른 방법입니다. `has(quest.q.state)`는 항상 true이므로 `W-QUEST-STATE-HAS`입니다. dsl 0.26.0부터 값이 이름의 폐쇄 집합인 경로 — enum 경로, `occasion.target`, 퀘스트의 `state` 또는 `failedBy`, `scene.choices.<id>`, enum 매개변수, `$`(`<match>` 안에서 사용됨) — 와 비교하는 문자열은 `==`, `!=`(어느 쪽이든), 또는 `in [...]` 요소로 사용하든 그 집합의 멤버여야 합니다. `quest.q.state == 'actve'`는 `E-WHEN-LITERAL-DOMAIN`입니다(`` did you mean `'active'`? ``). 이는 비교가 일으킬 데드 가드 오류 대신 외부 `<when is>` 리터럴이 받는 코드입니다.

## 0.32 표준 CEL 계약

폐쇄 프로필은 `int`와 `double`을 사용하며(`number`는 절대 사용하지 않음), 숫자 타입이 섞인 연산을 거부하고, 리스트 형식의 호스트 호출을
사용합니다:
`holds(string, list(dyn))`, `count(string, list(dyn))`,
`countDistinct(string, list(dyn), int)`, `validAt(string, list(dyn), int)`,
`now()`, `visited(string)`. 존재 여부에는 `has(select)`와 `"key" in map`을 사용합니다. 모든 실행 IR에는
`celEnv`가 포함됩니다. 조건 오류는 “충족되지 않음”을 뜻하지만, `::set` 오류는 쓰기 없이 중지합니다. 활성화에는 기본값과 함께 예약된 엔진 경로가 항상 포함됩니다. `quest.<id>.state`와 `.failedBy`는 `"unset"`이고, 각 퀘스트 목표의 `.done`과 `.failed`는 `false`이며, 각 로어 엔트리의 `.read`와 `.everRead`도 `false`입니다. `lute trace`의
3값 결과는 미리 보기일 뿐입니다.
