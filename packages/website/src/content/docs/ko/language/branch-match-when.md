---
title: 분기, match 및 when
description: 로직 계층 — 플레이어가 선택하는 <branch> 메뉴와 상태가 선택하는 <match>/<when>/<otherwise> 분기, 리터럴 패턴, CEL 가드, 처음 일치하는 항목이 우선되는 방식, 완전성입니다.
---

Lute의 로직 계층은 어떤 콘텐츠를 재생할지 선택하는 중첩 블록의 작은 집합입니다. 흐름을 분기하는 방식은
두 가지입니다. **`<branch>`** 는 *플레이어* 입력을 받고, **`<match>`** 는 입력 없이 *상태*를 기준으로
디스패치합니다. 둘 다 컴파일 시 유한 명령 레코드로 축약되므로 언어는 전체성을 유지합니다.

## `<branch>` — 플레이어 선택

`<branch id>`는 메뉴를 표시하고, 각 `<choice>`는 하나의 선택지입니다. 선택한 선택지 id는
예약된 경로 `scene.choices.<branchId>`에 기록되며, 이후의 `<match>`가 이를 읽을 수 있습니다.

```lute
<branch id="number">
  <choice id="blunt" text="Just ask, flatly">
    @fixer{code="0050"}: Marina. Your number.
  </choice>
  <choice id="soft" text="Ask gently">
    @fixer{code="0052"}: Marina — would you mind terribly if I had your number?
    ::set{scene.affect.marina += 1}
  </choice>
</branch>
```

*([`docs/examples/marina-s01ep02.lute`](https://github.com/journeyWorker/lute/blob/main/docs/examples/marina-s01ep02.lute)에서 가져왔습니다.)*

브랜치 id는 에피소드 안에서 고유해야 하며, 메뉴가 검증상 비어 있지 않도록 브랜치에는 최소 하나의
**가드가 없는** 선택지가 있어야 합니다(`E-BRANCH-ALL-GUARDED`). 선택지 메커니즘 — `when`
가드, `into=` 실행 레코드 설탕, 재방문 `<hub>` — 은
[선택지와 허브](/ko/language/choices-and-hubs/)에서 다룹니다.

## `<match>` — 상태 디스패치

`<match subject="S">`는 주체 표현식 `S`를 평가하고, 처음 일치하는 `<when>` 분기를 실행하며,
아무것도 일치하지 않으면 `<otherwise>`로 넘어갑니다. 플레이어 입력은 관여하지 않습니다. 이는
장면이 앞서 이루어진 선택, 팩트 또는 플러그인 결과에 반응하는 방식입니다.

```lute
<match subject="scene.choices.number">
  <when test="@fond">
    @fixer{mono}: I asked nicely, which I am electing not to examine.
  </when>
  <when is="blunt">
    @fixer{mono}: Straight to the point.
  </when>
  <otherwise>
    @fixer{mono}: Whatever it was, it is done.
  </otherwise>
</match>
```

분기는 **위에서 아래로 평가되며, 처음 일치하는 항목이 승리합니다.**

### `<when>` 패턴 및 가드

`<when>` 분기는 리터럴 패턴(`is`), CEL 가드(`test`), 또는 둘 다를 기준으로 일치합니다.

- **`is`** 는 리터럴 패턴입니다. 리터럴 하나이거나 리터럴의 `|` 대체 목록입니다. 허용되는 리터럴은
  enum 멤버 id, `true`/`false`, 10진수, 양 끝을 포함하는 숫자 범위(`2..`, `..0`,
  `1..3`), 키워드 `unset`입니다. 주체와의 동등성으로 일치하며(범위는 범위 멤버십), `<when is="joyful|playful">`, `<when is="unset">`, `<when is="1 | 2 | 3">`,
  `<when is="..0 | 10..">`처럼 씁니다.
- **`test`** 는 CEL 가드이며, `$` 주체가 스코프에 들어 있습니다(`$`는 `subject`의 값입니다). `$`는
  `<match>` 안에서만 나타날 수 있습니다. 패턴으로 표현할 수 없는 조건(`$ > 2`, `@fond`)에 사용합니다.
  `test="$ == 'gold'"`처럼 단순 리터럴 비교를 쓰면 `W-WHEN-TEST-LITERAL`이 발생하고, `lute fix`가
  이를 `is="gold"`로 다시 씁니다.
- **`is` + `test`** 를 함께 쓰면 패턴 AND 가드가 됩니다.
- `is`와 `test`가 모두 없는 `<when>`은 `E-WHEN-PATTERN`입니다.

```lute
<match subject="scene.mood">
  <when is="calm">
    @fixer{mono}: Steady breathing. Nothing to prove tonight.
  </when>
  <when is="tense">
    @fixer{mono}: Shoulders drawn tight — I should tread carefully.
  </when>
  <when is="joyful|playful">
    @fixer{mono}: Light in the eyes.
  </when>
</match>
```

*([`docs/examples/showcase/when-is-demo.lute`](https://github.com/journeyWorker/lute/blob/main/docs/examples/showcase/when-is-demo.lute)에서 가져왔습니다.)*

### 숫자 범위

`int` 또는 `double` 주체에서는 `is`가 범위를 받습니다. 양쪽 경계는 모두 포함되며 어느 쪽이든 열어 둘 수
있습니다. 내림차순 임계값 캐스케이드는 위에서 아래로, 처음 일치하는 항목이 승리하도록 읽습니다.

```lute
<match subject="scene.affect.marina">
  <when is="3..">
    ::use{component="reaction" tier="fond"}
  </when>
  <when is="1..">
    ::use{component="reaction" tier="warm"}
  </when>
  <otherwise>
    ::use{component="reaction" tier="cold"}
  </otherwise>
</match>
```

*([`docs/examples/affinity-reaction.lute`](https://github.com/journeyWorker/lute/blob/main/docs/examples/affinity-reaction.lute)에서 가져왔습니다.)*

형식이 잘못되었거나 비어 있는 범위(`a..b`, `3..1`)는 `E-WHEN-RANGE`입니다. 숫자가 아닌 주체에 범위를
사용하면 `E-WHEN-LITERAL-DOMAIN`입니다. 배타적 경계에는 패턴 형식이 없으므로 `test="$ > 2"`를
작성합니다.

### 완전성

`<match>`는 완전해야 합니다. 완전성은 `is` 리터럴의 합집합(`unset` 분기도 포함)으로 계산합니다.
**유한 도메인** — enum, bool 또는 브랜치의 choice id — 에서는 전체 `is` 커버리지가 `<otherwise>` 없이도
완전합니다. 위의 네 멤버 enum은 `<otherwise>`가 필요 없으며, `is="true"`/`is="false"`로 bool을
커버해도 마찬가지입니다. 커버리지가 부족하면 `E-NONEXHAUSTIVE`가 빠진 멤버를 명시합니다(
`` `tue`, `wed` are not covered ``). **`int` 또는 `double`** 주체는 범위와 점이 전체 실수선을
커버할 때 완전합니다. `is="..0"` + `is="0.."`은 완전하지만, `is="..0"` + `is="1.."`은
그 사이에 `0.5`가 있으므로 완전하지 않으며 오류가 간격을 명시합니다.

그 밖의 경우 `<otherwise>`는 **필수**입니다. 주체가 아마 unset인 경우(기본값이 없는 `run.*`/`app.*`
경로이므로 `unset` 사례를 커버해야 하는 경우), 도메인이 문자열이거나 그 밖에 불투명한 경우, 또는
체커가 도메인을 커버한다고 증명할 수 없는 `test` 가드를 분기가 사용하는 경우가 이에 해당합니다.
릴리스 빌드에서 `app.rating`을 읽는 `<match>`는 `teen`을 커버하거나 `<otherwise>`를 포함해야 하는
강제 콘텐츠 게이트입니다.

### def에 대한 매칭

`<match subject="@def">`([정의와 매개변수](/ko/language/params/) 참조)는 다른 주체와 마찬가지로
도메인을 가집니다. 본문이 하나의 상태 경로(`today: "run.wd"`)인 def는 해당 경로와 정확히 같은
방식으로 일치합니다. 그 밖의 def는 선언되었거나 추론된 타입을 사용하므로, `{ enum: [early, late] }`
타입의 def는 해당 멤버들을 범위로 가집니다.

```lute check
---
kind: scene
id: town.square
title: The square
state:
  run.wd: { type: { enum: [mon, tue, wed] }, default: mon }
  run.day: { type: int, default: 1 }
defs:
  today: "run.wd"
  shift: { type: { enum: [early, late] }, cel: "run.day <= 3 ? 'early' : 'late'" }
---

## Square

<match subject="@today">
  <when is="mon">
    @narrator: Market day.
  </when>
  <when is="tue|wed">
    @narrator: The square is quiet.
  </when>
</match>

<match subject="@shift">
  <when is="early">
    @narrator: The stalls are still going up.
  </when>
  <when is="late">
    @narrator: The stalls are coming down.
  </when>
</match>
```

두 match 모두 `<otherwise>` 없이 완전합니다. 0.24.0 이전에는 def 주체에 도메인이 없었으므로 둘 다
`E-NONEXHAUSTIVE`였습니다. 오타인 `is="lat"`처럼 도메인 밖의 분기는 did-you-mean과 함께
`E-WHEN-LITERAL-DOMAIN`입니다. 0.26.0부터는 경로를 대상으로 하는 match에서 enum 타입 주체와
비교하는 문자열 가드(예: `test="$ == 'lat'"`)도 `E-WHEN-LITERAL-DOMAIN`입니다.

팩트를 조회하는 def — `count(…)`, `holds(…)`를 직접 또는 다른 def를 통해 조회하는 경우 — 는 전혀
주체가 아닙니다. `badgeCount: "count('hasBadge', ['_'])"`에 대한 `<match subject="@badgeCount">`는
`E-MATCH-RELATION-SUBJECT`이지만 count는 `int`입니다. 대신 아래처럼 `subject`가 없는 `<match>`의
분기에서 쿼리를 테스트합니다.

### 팩트에 대한 매칭: `subject`가 없는 `<match>`

`subject`가 없는 `<match>`에는 주체가 없습니다. 각 분기의 `test`는 일반 조건이며 `$`도 없습니다.
팩트 쿼리에 사용하는 형식이며, 팩트 쿼리는 가드로만 사용할 수 있습니다.

```lute
<match>
  <when test="@badgeCount >= 8">
    @profAlder: Eight badges. The League is waiting for you.
  </when>
  <when test="holds('hasBadge', ['stone'])">
    @profAlder: The Stone Badge — a start.
  </when>
  <otherwise>
    @profAlder: Come back with a badge.
  </otherwise>
</match>
```

평소와 같이 분기는 위에서 아래로 시도합니다. 체커는 `test` 가드가 모든 사례를 커버한다고 증명할 수
없으므로 `<otherwise>`가 필요합니다.

이러한 `<match>`의 모든 분기는 `test`입니다. `is`는 주체와 비교하는데 주체가 없으므로,
`subject`가 없는 `<match>` 안에서 `<when is="ren">` 또는 `<when is="run.lamps >= 3">`를 사용하면
`E-MATCH-NO-SUBJECT`입니다. `<match>`에 `subject="…"`를 추가하거나 조건을 `test="run.lamps >= 3"`로
작성합니다. 정확히 하나의 선언된 경로에 리터럴이 속하는 경우 메시지에 해당 경로가 명시됩니다.

### 분기가 주체를 좁힙니다

아마 unset인 경로 — 기본값이 없는 `run.*`/`app.*` 경로 — 를 읽으면 무언가가 설정되었음을 증명할
때까지 `E-MAYBE-UNSET`입니다. `<match>` 주체가 일반 상태 경로이면 분기가 그 안에서 주체를 읽을 때
그 증명을 제공합니다.

- 대안이 값을 명시하고 그중 `unset`인 것이 없는 `<when is="…">` 내부에서는 주체가 그 값 중 하나와
  같으므로 설정된 상태입니다.
- 한 분기가 모든 unset 값을 처리하면 — `test`가 없는 `is="unset"` — 이후의 어떤 분기와
  `<otherwise>`도 주체를 unset으로 볼 수 없으므로 주체를 설정된 것으로 읽습니다.

```lute
<match subject="run.rival">
  <when is="unset">
    @narrator: Nobody has taken your measure yet.
  </when>
  <when test="$ >= 3">
    @narrator: Your rival is {{run.rival}} bouts ahead.
  </when>
  <otherwise>
    @narrator: Your rival sits at {{run.rival}}.
  </otherwise>
</match>
```

두 보간 모두 `E-MAYBE-UNSET`이 아닙니다. 둘 다 `unset` 분기 뒤에 나오기 때문입니다. 이 증명은
match 내부에 머물며 아래 방향으로만 실행됩니다. 위의 어떤 분기도 모든 unset 값을 처리하지 않으면
`<otherwise>`(또는 `test`만 있는 분기)는 여전히 주체를 unset으로 볼 수 있습니다. `is="unset"` 분기가
없거나 그 분기에 `test`가 붙은 경우가 이에 해당하므로, 그곳에서 주체를 읽어도 오류가 보고됩니다.
`</match>` 뒤에서 읽는 것은 match 전과 마찬가지로 증명되지 않은 상태입니다.
## 가드는 적용 범위를 좁힙니다

가드는 검사한 상태에 대해 무언가를 증명하며, 체커는 가드가 적용되는 곳에서 그 증명을 사용합니다.

**비트의 가드는 본문 전체에서 적용됩니다.** 장면 비트의 `when:`, 번들 `<beat when>`, 엔트리의
`when=`은 본문이 실행되는 동안 항상 참입니다. 가드 안의 `has(…)`는 본문에서 해당 경로를 읽는 것과
`<match>` 주체를 읽는 것이 모두 증명되도록 하므로 `E-MAYBE-UNSET`이나 `E-UNSET-UNCOVERED`가
발생하지 않습니다. 가드는 `<match>`가 커버해야 하는 도메인도 좁힙니다. 아래에서는 `died`가 배제되므로
두 분기만으로 `<otherwise>` 없이 완전합니다.

```lute check
---
kind: scene
id: dock.return
title: Back at the dock
on: dockReturn
when: "has(run.outcome) && run.outcome != 'died'"
state:
  run.outcome: { type: { enum: [surfaced, diving, died] } }
---

## Dock

@narrator: The run ended {{run.outcome}}.

<match subject="run.outcome">
  <when is="surfaced">
    @narrator: You surfaced with air to spare.
  </when>
  <when is="diving">
    @narrator: They hauled you up mid-dive.
  </when>
</match>
```

가드가 배제하는 모든 값을 갖는 분기는 절대 실행될 수 없으므로 `E-ARM-DEAD`입니다. `<when is="died">`를
추가하면 다음 진단이 보고됩니다.

<!-- lute-diagnostics -->
```
dock.lute:22:3: error [E-ARM-DEAD] arm can never fire: its pattern `died` is ruled out by the body's `when` guard `has(run.outcome) && run.outcome != 'died'`, which holds whenever this body runs
```

두 분기 뒤에 `<otherwise>`를 추가해도 같은 이유로 `W-OTHERWISE-DEAD`입니다. 경고에는 "본문의 `when`
가드가 남긴 도메인"이 명시됩니다. 매치 전에 본문이 주체를 쓰는 경우(`::set{run.outcome = "died"}`)에는
전체 도메인이 유지되므로, 그때는 다시 `died` 분기나 `<otherwise>`가 필요합니다.

**존재 가드는 단락 평가로 보호되는 것을 증명합니다.** 하나의 표현식 안에서 `has(p)`가 참이 된 뒤에만
실행되는 부분은 `p`를 읽을 수 있습니다.

```lute check
---
kind: scene
id: dock.board
title: The board
state:
  run.a: { type: int, default: 0 }
  run.o: { type: { enum: [won, lost] } }
---

## Board

@narrator{when="run.a == 1 || (has(run.o) && run.o == 'won')"}: A clean week.
@narrator{when="has(run.o) ? run.o == 'won' : false"}: You won.
@narrator{when="!has(run.o) || run.o == 'won'"}: Nothing lost yet.
```

세 줄 모두 문제없이 검사됩니다. 증명은 표현식 안에 머뭅니다. `has(run.o) || run.a == 1`은 해당 줄이
`run.a == 1`만 참인 경우에도 실행되므로, 그 줄에서 `run.o`가 설정되었음을 증명하지 않습니다.

**`prev.run.*`은 하나의 스냅숏입니다.** `newRun`은 종료된 실행 전체를 한 번에 복사하므로, 어떤
`prev.run.<p>`가 존재한다고 알려지면 `run.<q>`가 `default`를 선언한 모든 `prev.run.<q>`도 존재합니다.

```lute check
---
kind: scene
id: dock.recap
title: Last time
state:
  run.depth: { type: int, default: 0 }
  run.gold: { type: int, default: 0 }
---

## Recap

@narrator{when="has(prev.run.depth)"}: Last run you reached {{prev.run.depth}} fathoms with {{prev.run.gold}} gold.
```

기본값이 없는 `run.<q>`는 실행이 종료될 때 unset이었을 수 있으므로 해당 `prev.run.<q>`에는 여전히
자체 가드가 필요합니다.

## `when=` 콘텐츠 라인 설탕

하나의 콘텐츠 라인에는 `when="G"` 가드를 직접 넣을 수 있으며, `G`가 참일 때만 라인이 출력됩니다.

```lute
@elena{when="run.metHelpfully"}: You helped me back then. I've been meaning to thank you.
```

*([`docs/examples/gated-line.lute`](https://github.com/journeyWorker/lute/blob/main/docs/examples/gated-line.lute)에서 가져왔습니다.)*

이는 가드가 `<match>` 주체로 허용되는 곳이라면 어디서든 한 분기 매치를 쓰는 정확한 설탕이며, 동일한
레코드로 변환되어 라인의 `code`/`lineId`/`voiceKey`는 변경되지 않습니다. 풀어 쓰면 아래의 명시적인
쌍이 됩니다. 인라인 `<when>…</when>` 형식은 없으므로 모든 태그는 각자 한 줄을 차지합니다.

```lute
<match subject="run.metHelpfully">
  <when test="$">
    @elena: You helped me back then. I've been meaning to thank you.
  </when>
  <otherwise>
  </otherwise>
</match>
```

*(그 파일은 두 형식을 각각 한 섹션에 유지하므로 서로 바꿔 쓸 수 있다는 점이 눈에 보입니다.)*

**팩트 쿼리**는 예외입니다. `@elena{when="holds('awake', ['toma'])"}: …`는 콘텐츠 라인에서 정상적으로
검사되지만, 주체로 동일한 가드를 사용하면 — `<match subject="holds('awake', ['toma'])">` —
`E-MATCH-RELATION-SUBJECT`입니다. 팩트 쿼리는 가드로만 사용할 수 있으며, 메시지는 쿼리를 분기에서
테스트하는 `subject`가 없는 `<match>`로 수정하라고 안내합니다([팩트에 대한 매칭](#팩트에-대한-매칭-subject가-없는-match) 참조). 위의 한 분기 쌍은 `<match subject>`가 받을 수 있는 스칼라 및 enum 주체에 사용할 수 있으며, 따라서 위 파일도 이를 사용합니다.

### 가드된 쓰기

같은 설탕은 단일 쓰기에도 적용됩니다(dsl 0.24.0 §1). `::set{run.aff += 1 when="run.warmed < run.day"}`는
가드가 참일 때만 쓰며, 가드된 라인과 동일한 한 분기 매치로 컴파일되므로 조건부 쓰기 하나를 감싸는
`<match>`가 필요하지 않습니다. 가드는 라인의 `when=`과 동일하게 검사됩니다(bool이며 `$`는 없고,
참임을 증명할 수 없으면 `E-ARM-DEAD`입니다). 또한 확정 할당이 아니므로 기본값이 없는 경로를 나중에
읽어도 `E-MAYBE-UNSET`이 유지됩니다. 이 기능이 만들어진 하루 한 번 패턴은 [시계](/ko/language/clock/)를
참조합니다.
