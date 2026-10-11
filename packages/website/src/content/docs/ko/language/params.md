---
title: 정의와 params
description: defs에 선언하고 @name으로 참조하며 @name(args)로 매개변수화하는 이름 있는 타입 지정 CEL 매크로 — 언어의 재사용 가능한 값 레이어입니다.
---

Lute의 모든 조건과 `::set` 오른쪽 값은 [CEL](https://cel.dev) 표현식입니다. 필요한 곳마다 조건을 반복하는 대신 **def**로 한 번 이름 붙이고 `@name`으로 참조합니다. def는 언어의 재사용 가능한 *값* 레이어이며, 스키마 재사용([가져오기](/ko/language/imports/)) 및 콘텐츠 재사용([컴포넌트와 extends](/ko/language/components-and-extends/))과는 다릅니다.

## def 선언

`defs`는 씬의 프런트매터 또는 가져온 스키마에 이름 있는 타입 지정 CEL 값을 선언합니다. 각 항목에는 `type`, 선택적인 `params` 블록, `cel` 본문이 있습니다.

```yaml
defs:
  warm:    { type: bool,   cel: "scene.affect.elena >= 2" }
  tight:   { type: string, cel: "'closeUp'" }
  fond:    { type: bool,   cel: "scene.affect.marina >= 1" }
```

def는 `@name`으로 참조합니다. `@`는 **컴파일 타임 매크로**이므로 참조가 평가 전에 인라인 CEL로 확장됩니다. def는 런타임 함수 호출이 아니라 이름 있는 CEL 조각입니다. bool def는 guard로 읽히고, 상수로 접히는 def는 스테이징 값으로 읽힙니다.

```lute
<when test="@fond">
  @fixer{mono}: I asked nicely.
</when>
::camera{framing=@tight}
```

`@ref`는 필요한 타입이 def가 선언한 `type`과 일치하는 위치에 있어야 하며, 이름은 `defs`에 선언되어 있어야 합니다. def 이름과 param 이름은 식별자입니다(문자 또는 `_`로 시작하고 그 뒤에 문자, 숫자, `_`가 옴). 조건에서는 JavaScript 변수처럼 그대로 읽으므로 `lamp-lit`은 `lampLit`을 가리키는 `E-PATH-IDENT`입니다.

디렉티브 속성의 ref는 **따옴표 없이** 씁니다 — `framing=@tight`. `framing="@tight"`은 리터럴 문자열 `@tight`이므로 `framing` 도메인이 `E-BAD-ENUM`으로 거부합니다. 속성 값은 표현식이 아니라 상수이므로 컴파일러는 def가 접히는 리터럴(`"framing": "closeUp"`)을 기록하고, 작성한 값과 동일하게 검사합니다(`E-BAD-ENUM`, `E-ATTR-TYPE`). 상태를 읽는 def는 접히지 않으며 해당 위치에서 사용하면 `E-ATTR-DEF-DYNAMIC`입니다. 대신 각 분기에 리터럴을 하나씩 두고 분기하세요.

```lute
<match subject="scene.affect.elena">
  <when is="5..">
    ::camera{framing="closeUp"}
  </when>
  <otherwise>
    ::camera{framing="wide"}
  </otherwise>
</match>
```

이 조언은 디렉티브 속성에 해당합니다. 컴포넌트 enum param은 상태를 읽는 def라도 직접 받습니다. `::use{component="greet" tier=@mood}`는 def를 컴포넌트에 전달하고, 컴포넌트의 `<match subject="@tier">`가 런타임에 이를 분기하므로 주변을 분기할 필요가 없습니다([재사용 가능한 콘텐츠 컴포넌트](/ko/language/components-and-extends/#재사용-가능한-콘텐츠-컴포넌트) 참고). param을 자체 디렉티브 속성에 넣는 컴포넌트만 `E-ATTR-DEF-DYNAMIC`을 다시 만나며, 이 경우 `::use`에서 보고됩니다.

콘텐츠 텍스트에서 `{{@name}}`은 def 값을 렌더링합니다. 아티팩트는 자리표시자와 함께 def 본문을 담고 엔진은 이를 guard처럼 평가합니다. 하나의 표현식에 인라인할 수 없는 본문(확장 순환 또는 `$`를 읽는 본문)은 `E-INTERP-DEF`입니다. def 본문은 모든 guard와 같은 CEL 프로필 검사를 받으므로 하나에 `%` 또는 `size()`를 쓰면 def 자체 키에서 `E-CEL-PROFILE`이 발생합니다.

## 매개변수화한 def

def는 타입이 지정된 **`params`**를 선언해 `@name(args)`로 호출하는 매개변수화 매크로가 될 수 있습니다. 확장 시 인수가 param에 바인딩됩니다.

```yaml
defs:
  atLeast: { type: bool, params: { n: int }, cel: "user.level >= n" }
  chose:   { type: bool, params: { q: choiceRef, opt: choiceId }, cel: "scene.choices[q] == opt" }
```

*([`docs/examples/showcase/schema/base.schema.yaml`](https://github.com/journeyWorker/lute/blob/main/docs/examples/showcase/schema/base.schema.yaml)에서 가져왔습니다.)*

```lute
<match subject="scene.affect.marina">
  <when test="@atLeast(3)">
    @fixer{mono}: A veteran's welcome.
  </when>
  <otherwise>
    @fixer{mono}: Early days yet.
  </otherwise>
</match>
```

*([`docs/examples/showcase/episode01.lute`](https://github.com/journeyWorker/lute/blob/main/docs/examples/showcase/episode01.lute)의 호출 위치이며, 최소 매개변수화 def는 [`docs/examples/param-def.lute`](https://github.com/journeyWorker/lute/blob/main/docs/examples/param-def.lute)에 있습니다.)*

인수는 하나의 단위로 본문에 삽입됩니다. 원자 인수 — 경로, 정수 또는 소수 리터럴, 일반 문자열 리터럴, 또는 이미 괄호로 묶인 그룹 — 는 그대로 들어가므로 `@atLeast(3)`은 `(user.level >= 3)`으로 확장됩니다. 그 밖의 것은 괄호를 유지하므로 `@atLeast(user.level + 1)`은 `(user.level >= (user.level + 1))`으로 확장되고, 인수는 본문의 연산자와 다시 묶일 수 없습니다. (0.24.0 전에는 모든 인수에 괄호가 붙었으며, 컴파일된 표현식은 같고 IR의 `raw` 텍스트만 더 짧아졌습니다.)

### trace와 beat 테이블의 def

`lute trace`와 `lute beats`는 작성한 그대로 def 참조를 출력합니다. `<match subject="@today">` 헤더는 `<match @today>`로 읽히고, 분기 또는 choice guard는 `(@atLeast(3))`로, 줄 guard는 `` guard `@atLeast(3)` ``로 출력되며, coverage 요약에는 def 이름이 들어가고 비트의 `when` 열은 `@runsAtLeast(2) && @firstDay`로 읽힙니다. 대신 인라인된 CEL을 보려면 `--expand`를 전달합니다. 다음 guard 줄이 있는 씬을 예로 듭니다: `@narrator{when="@atLeast(3)"}: …` 및 `@narrator{when="@atLeast(user.level + 1)"}: …`.

```console
$ lute trace board.lute
trace: board.lute  (seeds: 0 paths, 0 facts; 0 selections)
  ## Board
  guard `@atLeast(3)`: skipped
  guard `@atLeast(user.level + 1)`: skipped
trace complete: 2 decisions; guard `@atLeast(3)` @13:1: skipped, guard `@atLeast(user.level + 1)` @14:1: skipped
$ lute trace board.lute --expand
trace: board.lute  (seeds: 0 paths, 0 facts; 0 selections)
  ## Board
  guard `(user.level >= 3)`: skipped
  guard `(user.level >= (user.level + 1))`: skipped
trace complete: 2 decisions; guard `(user.level >= 3)` @13:1: skipped, guard `(user.level >= (user.level + 1))` @14:1: skipped
```

`--json`은 확장을 `id`/`guard`/`label`/`when` 필드에 유지하고, 그 옆에 작성한 텍스트를 추가합니다(trace에서는 `authoredGuard` 등, `lute beats`에서는 `whenAuthored`). [트레이싱](/ko/tooling/tracing/)을 참고하세요.

## def가 위치하는 곳

`run`/`user`/`app` 상태를 읽는 def는 공유 스키마 문서에 두어 필요한 모든 씬이 가져올 수 있게 합니다([`extends:`](/ko/language/components-and-extends/)로 정제할 수도 있음). `scene.*`만 읽는 def는 씬에 인라인으로 둘 수 있습니다. 가져오기 사이의 중복 def 이름은 정적 오류이며 자동으로 가려지지 않습니다.

## rule guard의 def

rule의 scalar guard에는 어떤 조건처럼 def를 사용할 수 있습니다. `lit(lamp) :- cel("@firstDay")` 또는 인수를 넣어 `cel("@dayAtLeast(3)")`처럼 씁니다. guard는 검사 및 평가 전에 프로젝트의 defs에 대해 확장되며, 확장된 본문도 여전히 rule-guard 방화벽을 통과해야 합니다(scalar 상태만 허용하고 팩트 질의는 허용하지 않음). 존재하지 않는 def, 잘못된 인수 개수, 또는 rule guard의 `$`는 `E-RULE-GUARD-DEF`입니다.

```lute expect="E-RULE-GUARD-DEF"
---
kind: scene
id: shed.lamp
title: The lamp
state:
  run.day: { type: int, default: 1 }
defs:
  firstDay: { type: bool, cel: "run.day == 1" }
entities:
  item: { members: [lamp, rope] }
relations:
  lit: { args: [item], derive: true }
rules:
  - "lit(lamp) :- cel(\"@firstDy\")"
---

## Lamp

@narrator{when="holds('lit', ['lamp'])"}: The lamp is lit.
```

0.24.0 전에는 이 rule이 `check`를 통과한 뒤 guard가 확장되지 않은 채 평가되어 조용히 아무것도 derive하지 않았습니다. rule과 그 guard는 [팩트와 Datalog](/ko/state/facts-and-datalog/)을 참고하세요.
## 검사기가 확인하는 것

정적 검사기는 다섯 가지 `@ref` 검사를 소유하며, 각각 보수적입니다(명백히 잘못된 경우만 표시).

- 이름이 `defs`에 선언되어 있어야 합니다 — 그렇지 않으면 `E-UNDECLARED-REF`입니다.
- 호출 arity가 선언된 `params`와 일치해야 합니다 — 그렇지 않으면 `E-REF-ARITY`입니다.
- 정적으로 해석 가능한 각 인수가 해당 param의 타입과 일치해야 합니다 — 그렇지 않으면 `E-REF-ARG-TYPE`입니다.
- 전체 슬롯의 `@ref`가 해당 위치에 필요한 타입을 만들어야 합니다 — 그렇지 않으면 `E-REF-TYPE`입니다.
- def 본문이 읽는 모든 상태 경로를 **def가 사용되는 위치에서** 안전하게 읽을 수 있어야 합니다 — 그렇지 않으면 경로와 def를 명시한 `E-MAYBE-UNSET`입니다.

마지막 검사는 모든 사용 위치에서 실행됩니다: `{{@lastF}}`, `when="@lastF > 30"`, `::use` 인수 `fathoms=@lastF`, `::set` 오른쪽 값, `<match subject="@lastF">` subject가 모두 def 본문이 읽는 것을 읽습니다. 따라서 기본값이 없는 경로 위의 def는 사용 위치에서 guard를 달거나 스스로 guard해야 합니다.

```lute expect="E-MAYBE-UNSET"
---
kind: scene
id: dock.recap
title: Last time
state:
  run.depth: { type: int, default: 0 }
defs:
  lastF: { type: int, cel: "prev.run.depth * 10" }
  deepLast: { type: bool, cel: "has(prev.run.depth) && prev.run.depth > 3" }
---

## Recap

@narrator{when="has(prev.run.depth)"}: Last run you reached {{@lastF}} feet.
@narrator{when="@deepLast"}: Deeper than most.
@narrator: That was {{@lastF}} feet.
```

첫 두 줄은 문제가 없습니다. 첫 줄의 guard가 `prev.run.depth`를 증명하고, `@deepLast`는 자체 `has` guard를 포함합니다. 세 번째 줄은 아무것도 설정되었음을 증명하지 않은 채 `@lastF`를 통해 `prev.run.depth`를 읽습니다.

<!-- lute-diagnostics -->
```
recap.lute:16:21: error [E-MAYBE-UNSET] state path `prev.run.depth` may be read before it is set, read through `@lastF` (no default, no dominating `::set`, no guard)
```

이전 초안의 `min`/`max`/`values` def 필드는 제거되었습니다. def는 `type`, 선택적 `params`, `cel`로 완전히 기술됩니다.
