---
title: 스토리 개요
description: "계기로 선택되는 스토리를 읽기 전용으로 보는 세 가지 방법(dsl 0.23.0): `lute beats`는 각 계기의 비트 사다리를 검사기의 판정과 함께 출력하고, `lute calendar`는 상태 값 격자에서 플레이의 자격 판정을 평가하며, `lute scenario knowledge`는 모든 팩트 가드 조건을 규칙을 거쳐 그 팩트를 만드는 주체까지 추적합니다. 0.24.0부터: 시계와 visited 축, 계기별 축, `--facts` 표, calendar의 never-presented 목록, 작성된 그대로의 def와 `--expand`. 0.26.0부터: `lute beats`의 covered fallback과 종류 사다리, `lute scenario --facts`의 팩트 producer 간선, `knowledge`의 개수 세기 규칙 전제입니다."
---
## 기계 판독 출력 (0.43)

`beats`, `calendar`, `scenario`의 `--json` 결과는 `command`, `schemaVersion`, `ok`와 payload를 담은 envelope 객체입니다. 오류는 `ok: false`와 구조화된 `error`로 표현되며, 성공한 빈 결과로 위장하지 않습니다. 항상 종료 코드를 먼저 확인하세요.

스토리를 위에서 아래로 읽는 대신 [계기](/ko/tooling/play/)로 선택하면 "둘째 날 저녁 여관에서 무엇이 재생되는가?"에 답하는 단일 파일은 없습니다. 답은 모든 비트의 `on`, `target`, `priority`, `once`, `after:`, `when`, 프로젝트의 규칙, 세이브가 이미 보유한 값 전체에 흩어져 있습니다. dsl 0.23.0부터 세 명령이 이를 한 화면에 모읍니다:

- [`lute beats`](#lute-beats) — **사다리**: 각 계기와 대상의 모든 비트를 선택 순서대로, `check-project`가 내린 판정과 함께 보여 줍니다. 정적이며 프로젝트가 깨끗하게 검사될 필요는 없습니다.
- [`lute calendar`](#lute-calendar) — **격자**: 세이브에서 또는 플레이한 경로의 중간부터, 지정한 상태 값·퀘스트 상태·팩트의 모든 조합에 대해 `lute play`가 제시할 비트를 보여 줍니다. 플레이 자신의 자격 판정을 실행하므로 checker가 보수적인 곳에서도 정확합니다.
- [`lute scenario knowledge`](#lute-scenario-knowledge) — **knowledge map**: 팩트를 읽는 모든 조건에 대해 어떤 규칙이 그 조건을 결론 내리고, 필요한 팩트를 누가 만드는지, 아직 아무것도 만들지 않는 것까지 추적합니다.

세 명령 모두 읽기 전용입니다. 규범 텍스트는 [0.23.0 제안서](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.23.0.md)의 §1입니다.

dsl 0.26.0부터 이 보기들은 여러 작성자가 동시에 작성하는 프로젝트를 따릅니다. `lute beats`는 다른 비트가 항상 덮는 fallback을 표시하고 [종류 비트](/ko/tooling/play/#종류-대상)를 그 비트가 응답하는 모든 멤버에 나열합니다. [`lute scenario --facts`](#팩트-간선lute-scenario---facts)는 어떤 씬의 팩트가 어떤 gate를 여는지 — `after:` 그래프가 볼 수 없는 진행 — 를 그리며, [`lute refs`](/ko/tooling/cli/#refs)는 누가 무엇을 제공하는지, 즉 디렉티브 attribute 또는 보상 target의 모든 값과 이를 사용하는 문서를 나열합니다.

## 예제 프로젝트

이 페이지의 예제는 하루 시계가 있는 작은 항구 마을에서 실행됩니다. 플러그인은 세 계기를 선언합니다. 아침 종은 자격 있는 모든 비트를 차례로 재생하고(`select: sequence`, [계기 조합하기](/ko/tooling/play/#계기-조합하기) 참고), 장소 방문은 비트 하나를 고르며, 게시판은 목록을 제공합니다:

```yaml
occasions:
  dayStart:   { select: sequence, description: The bell rings; every eligible beat plays }
  placeVisit: { select: first, target: { prefix: place, entity: place } }
  board:      { select: all, description: Notices by the harbour master's door }
```

엔진이 시계를 소유합니다. 두 규칙은 하루 중 시각에 따라 사람을 배치하고, 세 번째 규칙은 소문이 달리 말하지 않는 한 Ada를 만난 뒤에는 Ada가 당신을 신뢰한다고 말합니다:

```yaml
state:
  run.day:     { type: int, default: 1, owner: engine }
  run.slot:    { type: { enum: [morning, evening] }, default: morning, owner: engine }
  user.visits: { type: int, default: 0, owner: engine }
entities:
  person: { members: [ada, bo] }
  place:  { members: [inn, dock] }
relations:
  works:   { args: [person, place], tier: run }
  met:     { args: [person], tier: run }
  rumor:   { args: [person], tier: run }
  arrived: { args: [place], tier: run, reserved: true }
  present: { args: [person, place], derive: true }
  trusted: { args: [person], derive: true }
facts:
  - "works(bo, dock)"
rules:
  - "present(ada, inn) :- cel(\"run.slot == 'evening'\")"
  - "present(P, dock) :- works(P, dock), cel(\"run.slot == 'morning' && run.day != 3\")"
  - "trusted(P) :- met(P), not rumor(P)"
```

비트:

| 비트 | 문서 | 응답 | 조건 | `once` |
|---|---|---|---|---|
| `day.bell` | `scenes/day/bell.lute` | `dayStart` | priority 10, `title: The bell` | `false` |
| `day.market` | `scenes/day/market.lute` | `dayStart` | `when: 'run.day == 3'`, `title: Market day` | `run` |
| `day.farewell` | `scenes/day/farewell.lute` | `dayStart` | `after: 'completed("ferry")'` | `run` |
| `inn.ada` | `scenes/inn/ada.lute` | `placeVisit` → `place.inn` | priority 20, `when: 'holds('present', ['ada', 'inn'])'`; `met(ada)`를 assert | `run` |
| `inn.again` | `scenes/inn/again.lute` | `placeVisit` → `place.inn` | priority 30, `after: 'visited("inn.ada")'`, `when: 'holds('trusted', ['ada'])'` | `run` |
| `inn.regular` | `scenes/inn/regular.lute` | `placeVisit` → `place.inn` | priority 10, `when: 'user.visits >= 3'` | `run` |
| `dock.storm` | `scenes/dock/storm.lute` | `placeVisit` → `place.dock` | priority 50, `when: "run.slot == 'evening' && run.day == 2"` | `run` |
| `innQuiet`, `dockBo`, `dockGulls`, `dockEmpty`(엔트리) | `lore/places.lute` | `placeVisit` | 아래 참고 | — |
| `noteFerry`, `noteFair`, `noteDocked`(엔트리) | `lore/places.lute` | `board` | 아래 참고 | — |

`lore/places.lute`의 엔트리:

```lute
<entry id="innQuiet" on="placeVisit" target="place.inn" category="bark" when="!holds('present', ['ada', 'inn'])">
  @narrator: The inn is quiet at this hour.
</entry>

<entry id="dockBo" on="placeVisit" target="place.dock" category="bark" when="holds('present', ['bo', 'dock'])">
  @bo: Mind the nets.
</entry>

<entry id="dockGulls" on="placeVisit" target="place.dock" category="bark">
  @narrator: Gulls fight over a fish head.
</entry>

<entry id="dockEmpty" on="placeVisit" target="place.dock" category="bark" priority="-1">
  @narrator: Nobody on the pier.
</entry>

<entry id="noteFerry" on="board" category="note" title="Ferry times" when="run.day <= 2">
  @narrator: "Ferry leaves on the third day."
</entry>

<entry id="noteFair" on="board" category="note" title="The fair" priority="5">
  @narrator: "Fair on the green, all week."
</entry>

<entry id="noteDocked" on="board" category="note" title="The ferry is in" priority="10" when="holds('arrived', ['dock'])">
  @narrator: "Ferry at the pier. Boarding now."
</entry>
```

그리고 `quests/ferry.lute`의 퀘스트 하나는 여관에서 Ada를 만난 뒤 시작됩니다:

```lute
<quest id="ferry" title="Catch the ferry" start="visited('inn.ada')">
  <objective id="word" title="Win Ada's trust" done="holds('trusted', ['ada'])"/>
  <objective id="board" title="Be at the dock on day three" done="run.day >= 3"/>
</quest>
```

## `lute beats`

```console
$ lute beats <dir> [--occasion <O>]… [--target <T>]… [--expand] [--json]
```

프로젝트의 모든 비트를 계기별 **사다리** 하나로, 대상 지정 계기라면 대상별 하나로 나누어 선택 순서 — priority 내림차순, 그 다음 `project.index.json`의 `beats` 순서인 프로젝트 순서 — 대로 출력합니다. 각 행에는 비트의 위치를 결정하는 것과 `check-project`가 비트에 대해 내린 판정이 함께 표시됩니다:

```console
$ lute beats .
project root: .

  board — select: all
    #  priority  beat                          kind   once  verdict  after  when
    1  10        noteDocked "The ferry is in"  entry  no    -        -      holds('arrived', ['dock'])
    2  5         noteFair "The fair"           entry  no    -        -      -
    3  0         noteFerry "Ferry times"       entry  no    -        -      run.day <= 2

  dayStart — select: sequence
    #  priority  beat                     kind   once  verdict  after               when
    1  10        day.bell "The bell"      scene  no    -        -                   -
    2  0         day.farewell             scene  run   -        completed("ferry")  -
    3  0         day.market "Market day"  scene  run   -        -                   run.day == 3

  placeVisit @ place.dock — select: first
    #  priority  beat        kind   once  verdict   after  when
    1  50        dock.storm  scene  run   -         -      run.slot == 'evening' && run.day == 2
    2  0         dockBo      entry  no    -         -      holds('present', ['bo', 'dock'])
    3  0         dockGulls   entry  no    tied      -      -
    4  -1        dockEmpty   entry  no    shadowed  -      -

  placeVisit @ place.inn — select: first
    #  priority  beat                      kind   once  verdict        after               when
    1  30        inn.again                 scene  run   -              visited("inn.ada")  holds('trusted', ['ada'])
    2  20        inn.ada "Ada at the inn"  scene  run   -              -                   holds('present', ['ada', 'inn'])
    3  10        inn.regular               scene  run   once-run-user  -                   user.visits >= 3
    4  0         innQuiet                  entry  no    -              -                   !holds('present', ['ada', 'inn'])
```

- **beat** — id이며 `title`이 있으면 뒤에 따옴표로 `title`을 붙입니다(scene의 `title:`, entry 또는 bundle beat의 `title=`).
- **kind** — `scene`, `entry`, 또는 `bundle`([bundle beat](/ko/tooling/play/#번들-비트)는 canonical `<document id>.<beat id>` 아래에 표시)입니다.
- **once** — `run`, `user`, `day`, `slot`(선언된 [시계](/ko/language/clock/)가 있을 때 scene, entry 또는 bundle beat의 `once="day"` / `once="slot"`), 또는 `no`(scene의 `once: false`, `once`가 없는 entry)입니다. [`also` beat](/ko/tooling/play/#계기-조합하기)면 뒤에 `, also`, [공유 소진](/ko/language/beats/#한-이벤트-여러-장소-share)이 있는 beat면 `, share <key>`가 붙습니다(dsl 0.25.0): `day, share solWarm`.
- **after** / **when** — 작성자가 쓴 조건이며 `@def` 참조도 포함합니다(dsl 0.24.0; `--expand`는 각 `when`을 확장해 출력). 없으면 `-`입니다.
- **verdict** — `check-project`가 이 beat에 대해 보고하는 프로젝트 전체 진단을 이름으로 표시합니다: `unreachable`(`E-BEAT-UNREACHABLE` / `E-ENTRY-UNREACHABLE`), `shadowed`(`W-BEAT-SHADOWED`), `tied`(`W-BEAT-PRIORITY-TIE`), `once-run-user`(`W-BEAT-ONCE-RUN-USER`). 이는 두 번째 분석이 아니라 같은 실행에서 얻은 checker 자신의 진단입니다. 각 의미는 [비트](/ko/language/beats/)를 참고하세요. 위에서는 `dockGulls`가 `dockBo`와 동률이고(같은 priority이며 조건이 서로 배타적이라고 증명되지 않음), 항상 자격 있는 `dockGulls`가 앞서므로 `dockEmpty`는 이길 수 없으며, `inn.regular`는 런당 한 번 소진되지만 `when`은 user 상태만 읽습니다.

대상 지정 계기는 비트가 이름 붙인 각 대상마다 하나의 사다리를 얻습니다. `target`이 없는 beat는 모든 발생에 응답하므로 모든 대상 사다리에 나타납니다. 어떤 beat도 대상 이름을 붙이지 않으면 계기는 `(any target)` 제목의 단일 사다리를 가지며 JSON에서는 `"anyTarget": true`입니다. Shadowing과 tie는 `select: first` 개념이므로 `select: all` 또는 `sequence` 사다리에는 나타나지 않습니다.

**Covered fallback**(dsl 0.26.0 §8). 더 앞에 있고 아직 소진되지 않았으며 `when`이 함의하는 beat가 항상 이기는 fallback은 절대로 재생되지 않지만 shadowed는 아닙니다. 그 위의 beat가 항상 자격 있는 것은 아니기 때문입니다. 이는 의도적으로 존재합니다(lead의 gate 줄로서, 한 영역이 gate에 응답할 때까지 유지). 따라서 verdict 열은 경고 대신 이를 덮는 대상을 표시합니다: `covered by <id>`(`--json`에서는 `coveredBy`). 영역의 `solGate`(priority 0)와 lead의 `gateFallback`(priority -10)은 `!holds('metWren', ['wren'])`이라는 같은 `when`을 공유하고, `once`가 없는 entry는 절대로 소진되지 않습니다. [종류 비트](/ko/tooling/play/#종류-대상)(dsl 0.26.0 §5)는 어떤 beat가 이름 붙인 모든 멤버의 사다리 — 그곳에서는 Gus 자신의 beat가 이를 앞섬 — 와, 어떤 beat도 자체적으로 이름 붙이지 않은 멤버의 `kind:<kind>` 사다리에 표시됩니다:

```console
$ lute beats . --occasion talk
project root: .

  talk @ npc.gus — select: first
    #  priority  beat                                       kind    once  verdict  after  when
    1  0         trainers.gusRematch "Gus wants a rematch"  bundle  no    -        -      -
    2  0         trainers.challenge "A trainer squares up"  bundle  no    -        -      -

  talk @ npc.sol — select: first
    #  priority  beat          kind   once  verdict             after  when
    1  0         solGate       entry  no    -                   -      !holds('metWren', ['wren'])
    2  -10       gateFallback  entry  no    covered by solGate  -      !holds('metWren', ['wren'])

  talk @ npc.wren — select: first
    #  priority  beat                          kind   once  verdict  after  when
    1  0         wren.talk "Wren at the pier"  scene  no    -        -      -

  talk @ kind:trainer — select: first
    #  priority  beat                                       kind    once  verdict  after  when
    1  0         trainers.challenge "A trainer squares up"  bundle  no    -        -      -
```

`--target`는 계기 도메인의 모든 멤버를 받습니다. `lute beats . --occasion talk --target npc.r16Gus`는 `talk @ npc.r16Gus` 사다리 하나를 출력하며 종류 beat만 나타납니다. Monster League에서는 열두 `covered by` 행이 lead의 gate fallback이고, 각각 gate에 응답하는 영역 beat로 덮입니다.

**프로젝트가 깨끗하게 검사될 필요는 없습니다.** 무언가가 잘못되었을 때 사다리가 가장 유용합니다. 폭풍의 조건을 모순으로 쓰면 `check-project`는 `E-BEAT-UNREACHABLE`로 실패하지만, 사다리는 dock의 맨 위에 해당 beat를 계속 나열하고 다음과 같이 표시합니다:

```console
$ lute beats . --target place.dock
project root: .

  placeVisit @ place.dock — select: first
    #  priority  beat        kind   once  verdict      after  when
    1  50        dock.storm  scene  run   unreachable  -      run.slot == 'morning' && run.slot == 'evening'
    2  0         dockBo      entry  no    -            -      holds('present', ['bo', 'dock'])
    3  0         dockGulls   entry  no    tied         -      -
    4  -1        dockEmpty   entry  no    shadowed     -      -
```

`--occasion <O>`(반복 가능)는 해당 계기의 사다리만 남기고, `--target <T>`(반복 가능)는 해당 대상에 발생하는 사다리만 남기므로 대상 없는 계기는 빠집니다. 프로젝트가 모르는 계기는 known 목록을 보여 주는 사용법 오류이며, beat가 이름 붙인 대상이 없으면 `(no beats)`를 출력합니다:

```console
$ lute beats . --occasion daystart
lute beats: `--occasion daystart` is not an occasion of this project (known: board, dayStart, placeVisit)
```

`--json`은 `{ "roots": [ { "root", "ladders": [ { "occasion", "select", "target"? | "anyTarget"?, "beats": [ … ] } ] } ] }`를 출력합니다. 각 beat는 `{ id, kind, document, priority, once, share?, target?, also?, after?, when?, whenAuthored?, title?, coveredBy?, neverFor?, verdicts }`이며, `once`는 `"run"`, `"user"`, `"day"`, `"slot"`, 또는 `"none"`, `share`는 beat의 공유 소진 키(dsl 0.25.0), `when`은 확장형, `whenAuthored`는 작성자 텍스트(dsl 0.24.0), `coveredBy`는 항상 beat를 이기는 beat의 id(dsl 0.26.0), `neverFor`는 종류 또는 `for=` beat의 `when`이 성립하지 않는 사다리 대상(dsl 0.28.0), `verdicts`는 전체 진단이며 각각 `{ code, severity, message }`입니다. 종류 사다리의 `target`은 `"kind:<kind>"`입니다:

```json
{
  "document": "lore/places.lute",
  "id": "dockEmpty",
  "kind": "entry",
  "once": "none",
  "priority": -1,
  "target": "place.dock",
  "verdicts": [
    {
      "code": "W-BEAT-SHADOWED",
      "message": "entry `dockEmpty` can never win occasion `placeVisit` for `place.dock`: entry `dockGulls` (priority 0) is ordered before it, is always eligible (no `after:`, and its `when` is absent or always true), and is never spent (an entry without `once`), so it wins every time",
      "severity": "warning"
    }
  ]
}
```

**작성된 그대로의 Def**(dsl 0.24.0). `when` 열은 작성자가 쓴 def 참조를 보여 주며, `--expand`는 대신 확장형을 출력합니다. [`beats` scaffold](/ko/tooling/play/#예제)에서 `town.morning`은 `!@firstDay`로 gate됩니다:

```console
$ lute beats . --occasion townVisit
project root: .

  townVisit — select: first
    #  priority  beat                            kind   once  verdict  after  when
    1  10        town.welcome "First arrival"    scene  user  -        -      -
    2  0         town.morning "Another morning"  scene  no    -        -      !@firstDay
$ lute beats . --occasion townVisit --expand
project root: .

  townVisit — select: first
    #  priority  beat                            kind   once  verdict  after  when
    1  10        town.welcome "First arrival"    scene  user  -        -      -
    2  0         town.morning "Another morning"  scene  no    -        -      !(run.day == 1)
```

성공 시 종료 **0**, I/O 실패 또는 알 수 없는 `--occasion`이면 **2**입니다.
## `lute calendar`

```console
$ lute calendar <dir> [--axis <path>=<lo>..<hi> | <path>=<a>,<b>,… | clock[=<d1>..<d2>]]…
                [--occasion <O>[@<axis>[=<value>],…]]… [--target <T>]… [--facts <relation>]…
                [--script <route.play.yaml> [--until <step>]] [--where <cel>]
                [--json | --csv]
```

상태 값 격자의 **모든 셀**과 모든 계기 열에 대해, [`lute play`](/ko/tooling/play/)가 그곳에서 자격 있다고 판정할 비트를 평가합니다 — 프로젝트의 규칙을 적용한 자체 후보, `once`, `after:`, `when` — 그리고 승자를 보고합니다. 모든 셀은 같은 월드에서 시작합니다. 선언된 기본값 또는 스텝을 재생한 플레이 스크립트의 세이브에서 시작한 뒤 축 값을 쓰고 퀘스트를 정착시킵니다. 격자 안에서 제시되는 것은 없습니다. 각 셀은 “지금 엔진이 이 계기를 발생시키면 무엇을 플레이할까?”라는 하나의 질문이며, 어떤 셀도 다른 셀이 한 일을 보지 않습니다.

- `--axis`(선택, 반복 가능)는 선언된 상태 경로와 그 값을 이름 붙입니다: 포함하는 정수 범위 `run.day=1..7`, 또는 목록 `run.slot=morning,evening`입니다. 각 값은 경로의 선언된 타입에 맞는지 검사됩니다. 격자는 축들의 곱이며, **첫 번째 축이 가장 느리게 변합니다**. `--axis`가 없으면 단일 셀입니다. 상태 쓰기로는 닿을 수 없는 더 많은 축 종류(퀘스트 상태, 팩트, 방문 집합, 선언된 시계)는 [퀘스트, 팩트, visited 및 시계 축](#퀘스트와-팩트-축)을 참고하세요.
- `--occasion <O>`(반복 가능) — 평가할 계기입니다. 기본값은 비트가 응답하는 모든 계기입니다. `<O>@<axis>,…`는 해당 계기에서 일부 축만 변화시킵니다 — [계기별 축과 각자의 위치](#계기별-축과-각자의-위치)를 참고하세요.
- `--facts <relation>`(반복 가능, dsl 0.24.0) — 정착이 끝난 뒤 모든 셀에서 해당 관계의 팩트를 출력합니다.
- `--target <T>`(반복 가능) — 대상에게 발생하는 계기의 대상입니다. 기본값은 비트가 이름 붙인 모든 대상이며, 선언된 [대상 도메인](/ko/tooling/play/#계기)의 나머지는 포함하지 않습니다. 비트가 응답하지 않아 모든 셀이 구멍으로 읽히기 때문입니다. [종류 비트](/ko/tooling/play/#종류-대상)(dsl 0.26.0)는 종류의 모든 멤버를 이름 붙이므로 각 멤버에 열이 생기며, 해당 멤버에게 발생할 때 그 priority의 자체 비트 뒤에서 제공됩니다. 어떤 비트도 대상을 이름 붙이지 않으면 계기에는 `(any)`라는 단일 열이 생깁니다(아래 목록에서 `<occasion>@(any)`). 이 열에는 대상 없는 비트만 응답합니다. 각 (계기, 대상) 쌍은 하나의 열입니다.
- `--script <file>` — 모든 셀이 시작할 플레이 스크립트입니다. **세이브**(`state:`, `facts:`, `visited:`, `presented:`, `quests:`, `entriesRead:` — [세이브에서 시작하기](/ko/tooling/play/#세이브에서-시작하기))에서 시작한 뒤 `steps:`를 `lute play`와 정확히 같은 방식으로 재생합니다. 세이브에는 스텝이 없어도 됩니다. 이 옵션이 없으면 모든 셀이 선언된 기본값과 seed 팩트에서 시작합니다.
- `--until <step>` — `--script`와 함께 사용하며, 이 스텝 전까지만 재생합니다. 1부터 세는 번호 또는 `label:`로 지정하며 해당 스텝 자체는 플레이하지 않습니다. [경로를 따라](#경로를-따라)를 참고하세요.
- `--where <cel>` — 이 조건이 성립하는 셀만 남깁니다. [어떤 런도 도달하지 않는 셀 버리기](#어떤-런도-도달하지-않는-셀-버리기)를 참고하세요.
- `--json` / `--csv` — 격자 대신 기계 판독 가능한 출력을 냅니다.

마을의 첫 3일을 보겠습니다.

```console
$ lute calendar . --axis run.day=1..3 --axis run.slot=morning,evening
calendar: . — 6 cell(s) × 4 column(s), from declared defaults

run.day  run.slot  board (all)          dayStart (sequence)   placeVisit     placeVisit
                                                              place.dock     place.inn
1        morning   noteFair, noteFerry  day.bell              dockBo +2      innQuiet
1        evening   noteFair, noteFerry  day.bell              dockGulls +1   inn.ada
2        morning   noteFair, noteFerry  day.bell              dockBo +2      innQuiet
2        evening   noteFair, noteFerry  day.bell              dock.storm +2  inn.ada
3        morning   noteFair             day.bell, day.market  dockGulls +1   innQuiet
3        evening   noteFair             day.bell, day.market  dockGulls +1   inn.ada

shadowed (eligible, not presented):
  run.day=1 run.slot=morning  placeVisit@place.dock: dockBo over dockGulls, dockEmpty
  run.day=1 run.slot=evening  placeVisit@place.dock: dockGulls over dockEmpty
  run.day=2 run.slot=morning  placeVisit@place.dock: dockBo over dockGulls, dockEmpty
  run.day=2 run.slot=evening  placeVisit@place.dock: dock.storm over dockGulls, dockEmpty
  run.day=3 run.slot=morning  placeVisit@place.dock: dockGulls over dockEmpty
  run.day=3 run.slot=evening  placeVisit@place.dock: dockGulls over dockEmpty

never eligible in any cell: 4
  noteDocked [entry, lore/places.lute] board — when: false
  day.farewell [scene, scenes/day/farewell.lute] dayStart — after: prerequisite not satisfied
  inn.again [scene, scenes/inn/again.lute] placeVisit@place.inn — after: prerequisite not satisfied
  inn.regular [scene, scenes/inn/regular.lute] placeVisit@place.inn — when: false

eligible but never presented in any cell: 1
  dockEmpty [entry, lore/places.lute] placeVisit@place.dock — lost to dock.storm; dockBo; dockGulls
```

셀을 읽는 방법:

- `select: first` 열은 **승자**를 보여 주고, 자격은 있었지만 순위에서 앞섰던 비트가 `+N`으로 이어집니다. 격자 아래 `shadowed` 블록에는 이들을 승자부터 나열합니다. `-`는 빈 셀입니다. 자격 있는 것이 없으므로 계기는 스토리 없이 지나갑니다. calendar가 찾는 구멍이 바로 이것입니다. `N`은 더 자격 있었지만 순위에서 밀린 비트 수입니다.
- `select: all` 열은 **제공 목록**을, `select: sequence` 열은 재생할 **시퀀스**를 각각 선택 순서대로 보여 줍니다.
- `?`는 **미결정** 셀입니다. 참조 런타임이 판정할 수 없는 `when`(가령 `validAt(…)`)이 결과를 결정하므로 `lute play`라면 그곳에서 멈춥니다. 이유는 `undecided` 아래에 나열됩니다.

여기서 부두는 절대 비지 않습니다. Bo가 부두에 없는 모든 셀을 `dockGulls`가 덮고, 두 번째 저녁에는 폭풍이 대신합니다. 규칙은 Ada를 이동시키므로 저녁에만 여관에 있습니다. calendar는 **어느 셀에서도 자격이 없는 비트**도 그 이유와 함께 나열합니다. `day.farewell`과 `inn.again`은 새 출발에 없는 이력(`after:`)을 기다리고, `inn.regular`는 세 번의 방문이 필요하며, `noteDocked`는 엔진만 단언하는 팩트가 필요합니다. dsl 0.24.0부터는 **자격은 있었지만 어느 셀에서도 제시되지 않은 비트**도 무엇이 이겼는지와 함께 나열합니다. `dockEmpty`는 언제나 순위에서 밀립니다(`--json`: `neverPresented`, 각 항목의 `beatenBy`; `--csv`: 두 번째 테이블).

### 세이브에서

축으로 바꿀 수 없는 것이 이력이며, 그래서 `--script`가 이를 제공합니다. 플레이어가 이전 저녁에 Ada를 만났고 방문이 모두 세 번인 세이브에는 스텝이 필요하지 않습니다.

```yaml
# A save: Ada met on an earlier evening, three visits in all. No steps.
visited: [inn.ada]
facts: ["met(ada)"]
state: { user.visits: 3 }
```

```console
$ lute calendar . --axis run.day=1..3 --axis run.slot=morning,evening \
    --script plays/regular.play.yaml --occasion dayStart --occasion placeVisit --target place.inn
calendar: . — 6 cell(s) × 2 column(s), from the save in plays/regular.play.yaml

run.day  run.slot  dayStart (sequence)                 placeVisit
                                                       place.inn
1        morning   day.bell                            inn.again +2
1        evening   day.bell                            inn.again +2
2        morning   day.bell                            inn.again +2
2        evening   day.bell                            inn.again +2
3        morning   day.bell, day.farewell, day.market  inn.again +2
3        evening   day.bell, day.farewell, day.market  inn.again +2

shadowed (eligible, not presented):
  run.day=1 run.slot=morning  placeVisit@place.inn: inn.again over inn.regular, innQuiet
  run.day=1 run.slot=evening  placeVisit@place.inn: inn.again over inn.ada, inn.regular
  run.day=2 run.slot=morning  placeVisit@place.inn: inn.again over inn.regular, innQuiet
  run.day=2 run.slot=evening  placeVisit@place.inn: inn.again over inn.ada, inn.regular
  run.day=3 run.slot=morning  placeVisit@place.inn: inn.again over inn.regular, innQuiet
  run.day=3 run.slot=evening  placeVisit@place.inn: inn.again over inn.ada, inn.regular

never eligible in any cell: none

eligible but never presented in any cell: 3
  innQuiet [entry, lore/places.lute] placeVisit@place.inn — lost to inn.again
  inn.ada [scene, scenes/inn/ada.lute] placeVisit@place.inn — lost to inn.again
  inn.regular [scene, scenes/inn/regular.lute] placeVisit@place.inn — lost to inn.again
```

방문으로 `inn.again`의 `after:`가 열리고 `met(ada)`에서 `trusted(ada)`가 파생되므로 Ada의 두 번째 씬이 모든 셀에서 여관을 차지합니다. 퀘스트는 셀의 값을 쓴 뒤 **모든 셀에서 정착**합니다. 방문이 `ferry`를 시작하고 3일째부터 두 목표가 모두 성립하므로 그곳에서 퀘스트가 완료되고, `after: completed("ferry")`인 `day.farewell`이 아침 시퀀스에 들어옵니다. 퀘스트 정착이 멈추는 셀(참조 런타임이 판정할 수 없는 목표)은 격자 뒤 `notes:`에 메모로 나옵니다. 정착 때문에 셀이 쓴 축 값에서 벗어나는 셀도 마찬가지입니다. 이는 핸들러의 해당 경로 쓰기나 수명 주기가 이동시키는 시드 퀘스트 상태일 수 있습니다([아래](#퀘스트와-팩트-축) 참고).

### 경로를 따라

세이브는 손으로 적은 이력입니다. 경로는 플레이한 이력입니다. 스크립트에 `steps:`가 있으면 모든 셀은 그 스텝이 남긴 월드에서 시작하며, `lute play`와 정확히 같은 방식으로 재생됩니다 — 제시, `once` 소진, 팩트 단언, 퀘스트 진행이 포함됩니다. `--until <step>`은 이 재생을 해당 스텝 앞에서 멈춥니다. 번호 또는 `label:`로 이름 붙이며, 격자는 그 스텝이 찾게 될 것을 보여 줍니다. 아래 경로는 저녁에 여관에 들렀다가 돌아옵니다.

```yaml
# plays/evening.play.yaml
state: { run.slot: evening }
steps:
  - occasion: placeVisit
    target: place.inn
  - label: back at the inn
    occasion: placeVisit
    target: place.inn
```

돌아오는 길 앞에서 멈추고 Ada에 대한 소문이 결과를 바꿀지 묻습니다.

```console
$ lute calendar . --script plays/evening.play.yaml --until "back at the inn" \
    --axis 'holds('rumor', ['ada'])=false,true' --occasion placeVisit --target place.inn
calendar: . — 2 cell(s) × 1 column(s), from the save in plays/evening.play.yaml, then its step 1 replayed (stopping before step 2, back at the inn)

holds('rumor', ['ada'])  placeVisit
                   place.inn
false              inn.again
true               -

never eligible in any cell: 3
  innQuiet [entry, lore/places.lute] placeVisit@place.inn — when: false
  inn.ada [scene, scenes/inn/ada.lute] placeVisit@place.inn — once: run — already presented this run
  inn.regular [scene, scenes/inn/regular.lute] placeVisit@place.inn — when: false

eligible but never presented in any cell: none
```

재생된 스텝은 `inn.ada`를 제시하고 `met(ada)`를 단언하며 `ferry`를 시작하고, 런 동안 소진시킵니다. 소문이 없으면 Ada가 당신을 신뢰하여 돌아오는 길에 `inn.again`이 이기지만, 소문이 있으면 여관의 어떤 것도 자격이 없습니다. 소문이 여는 구멍입니다. 헤더는 셀이 시작한 위치를 이름 붙이고 `--json`은 같은 텍스트를 `from`으로 전달합니다.

`--until`에는 `--script`가 필요합니다. 스크립트에 없는 스텝 번호나 선언되지 않은 레이블은 사용법 오류이며, 레이블에는 did-you-mean이 붙습니다. 재생이 멈추는 경우(가령 스크립트에 없는 선택지)도 마찬가지입니다. 플레이하지 못하는 경로가 어디까지 도달하는지 calendar가 말할 수 없기 때문입니다.

### 퀘스트와 팩트 축

선언된 상태 경로의 축은 `engine:` 스텝처럼 그 경로에 씁니다. 어떤 것은 쓰기 이상이 필요하므로 각자 별도의 축이 있습니다.

- `quest.<id>.state=unset,active,complete,failed`는 세이브의 `quests:`처럼 퀘스트의 **상태**를 시드합니다. 일반 쓰기는 셀의 정착 수명 주기에 의해 덮어써지지만 이 축은 그렇지 않습니다. `unset`과 `active`는 경로가 퀘스트 상태를 이름 붙이므로 재생된 경로가 만든 목표 진행도 지웁니다. `quest.<id>.objectives.<oid>.done=false,true`는 세이브의 목표 진행도처럼 작성된 그대로 취급합니다. 그 밖의 `quest.*` 경로(가령 `activatedAt`)는 수명 주기 자체의 장부이므로 사용법 오류이고, 어떤 퀘스트도 선언하지 않은 id도 마찬가지입니다.
- `holds(<fact>)=true,false`는 규칙이 파생하기 전에 기반 팩트를 단언(`true`)하거나 철회(`false`)하므로 파생 팩트도 그에 따릅니다. 팩트는 선언된 관계의 ground 팩트여야 하고 도메인의 멤버를 사용해야 합니다. 파생 관계는 규칙이 결정하므로 거부됩니다. 셸에서는 축을 인용합니다: `--axis 'holds('rumor', ['ada'])=false,true'`.
- `visited('<id>')=true,false`(dsl 0.24.0)는 씬 또는 번들 비트를 셀의 방문 집합에 넣거나 빼므로 `after: visited(…)` 뒤의 비트를 경로 없이 양쪽으로 읽을 수 있습니다.
- `clock=<d1>..<d2>`(dsl 0.24.0 §1)는 선언된 시계를 걷습니다. 시계 순서대로 각 날짜의 모든 슬롯을 지납니다 — `slots: [morning, afternoon, night]`에서 `clock=1..2`는 `1 Mon morning`부터 `2 Tue night`까지 6개 셀입니다(시계의 `week:`에 레이블이 있으면 요일 레이블도 표시). 각 셀은 시계의 `day`와 `slot` 경로를 쓰므로 해당 위치를 `clock.index`와 `clock.weekday`가 읽습니다. 시계 축은 자체 `day` 또는 `slot` 경로 축과 함께 쓸 수 없으며, 시계가 없는 프로젝트는 이를 거부합니다. `--axis clock`만 쓰면 day 1에서 시작하는 1주입니다(`week:`가 없으면 day 1 하나).
- `<family>.*=<values>`와 `<family>[<axis>]=<values>`(0.27.0부터)는 전체 [`per:` family](/ko/state/state-model/#엔터티마다-하나의-경로-per)를 변화시킵니다 — [하나의 `per:` family를 축으로](#하나의-per-family를-축으로)를 참고하세요.

그 밖의 것은 사용법 오류이며 축 종류와 오타 난 상태 경로에 대한 did-you-mean을 나열합니다.

```console
$ lute calendar . --axis run.dya=1..3
lute calendar: `--axis run.dya`: `run.dya` is not a declared state path in this project — did you mean `run.day`?; an axis is one of: a declared state path (`run.day=1..7`), every member of a `per:` family (`run.aff.*=6,7`) or the member another axis names (`run.aff[run.route]=6,7`), `quest.<id>.state=<status>,…`, `quest.<id>.objectives.<oid>.done=true,false`, `holds(<fact>)=true,false`, `visited('<scene or bundle-beat id>')=true,false`, `clock[=<d1>..<d2>]` (every slot of those days, in order)
$ lute calendar . --axis 'holds('trusted', ['ada'])=true,false'
lute calendar: `--axis holds('trusted', ['ada'])`: `trusted(ada)` is derived by rules and cannot be asserted
```

방문과 만남을 축으로 사용하면 두 가지가 모두 성립하는 곳에서만 `inn.again`이 이깁니다.

```console
$ lute calendar . --axis "visited('inn.ada')=false,true" --axis 'holds('met', ['ada'])=false,true' \
    --occasion placeVisit --target place.inn
calendar: . — 4 cell(s) × 1 column(s), from declared defaults

visited('inn.ada')  holds('met', ['ada'])  placeVisit
                                     place.inn
false               false            innQuiet
false               true             innQuiet
true                false            innQuiet
true                true             inn.again +1

shadowed (eligible, not presented):
  visited('inn.ada')=true holds('met', ['ada'])=true  placeVisit@place.inn: inn.again over innQuiet

never eligible in any cell: 2
  inn.ada [scene, scenes/inn/ada.lute] placeVisit@place.inn — when: false
  inn.regular [scene, scenes/inn/regular.lute] placeVisit@place.inn — when: false

eligible but never presented in any cell: none
```

상태는 seed이고 정착은 여전히 실행됩니다. 위 세이브에서 Ada를 만났고 `inn.ada`를 방문한 상태로, `unset`으로 시드한 퀘스트는 그대로 있지 않습니다.

```console
$ lute calendar . --script plays/regular.play.yaml --axis quest.ferry.state=unset,active --occasion dayStart
calendar: . — 2 cell(s) × 1 column(s), from the save in plays/regular.play.yaml

quest.ferry.state  dayStart (sequence)
unset              day.bell
active             day.bell

notes:
  quest.ferry.state=unset: quest.ferry.state settled to active

never eligible in any cell: 2
  day.farewell [scene, scenes/day/farewell.lute] dayStart — after: prerequisite not satisfied
  day.market [scene, scenes/day/market.lute] dayStart — when: false

eligible but never presented in any cell: none
```

`ferry`는 세이브가 가진 `visited('inn.ada')`에서 시작하므로 정착이 이를 활성화하고 메모가 그렇게 말합니다.

### 하나의 `per:` family를 축으로

[`per:` family](/ko/state/state-model/#엔터티마다-하나의-경로-per) — `run.aff: { type: int, default: 0, per: suitor }` — 는 멤버마다 하나의 경로(`run.aff.ren`, `run.aff.kai`, …)이고, 멤버마다 축을 하나씩 두면 격자가 그 수만큼 곱해집니다. 0.27.0부터는 하나의 축으로 family 전체를 덮을 수 있습니다.

- `--axis 'run.aff.*=6,7'`은 셀의 값을 **모든** 멤버에게 줍니다.
- `--axis 'run.aff[run.route]=6,7'`은 셀에서 `--axis run.route`의 값이 이름 붙인 멤버에게만 줍니다. 다른 멤버는 seed 또는 기본값을 유지합니다. 인덱싱 축은 같은 calendar에서 family 종류의 멤버를 하나 이상 이름 붙이는 `--axis`여야 합니다. family 밖의 경로(`hotaru`)를 이름 붙이는 값에서는 연결된 축이 아무것도 설정하지 않으므로 그 셀은 모든 값의 대표가 되고, 해당 `run.aff[run.route]` 열은 `(none)`을 읽으며 메모가 표시됩니다.

경로와 연결하면 결말 매트릭스는 경로 × 그 경로의 애정도가 되며 다른 구혼자는 모두 기본값에 남습니다.

```console
$ lute calendar . --script plays/confessed.play.yaml --axis run.route=ren,kai \
    --axis 'run.aff[run.route]=6,7' --occasion termEnd
calendar: . — 4 cell(s) × 1 column(s), from the save in plays/confessed.play.yaml

run.route  run.aff[run.route]  termEnd
ren        6                   end.ren.ember +1
ren        7                   end.ren.lantern +2
kai        6                   end.kai.ember +1
kai        7                   end.kai.lantern +2
```

축은 행 레이블, `--json`의 `at` 키, `--csv` 헤더에 작성된 그대로 이름 붙습니다. family 자체는 값이 아니므로 일반 `--axis run.aff=…`는 세 형태를 추측하지 않고 이름 붙이라고 하며, family 축이 이미 설정하는 멤버를 축으로 추가하는 것도 거부합니다. 예를 들어 `--axis 'run.aff.*=6' --axis run.aff.ren=7`은 거부됩니다.

```console
$ lute calendar . --axis run.route=ren,kai --axis run.aff=6,7 --occasion termEnd
lute calendar: `--axis run.aff`: `run.aff` is a `per: suitor` family — name a member (`run.aff.ren`), all of them (`run.aff.*`), or tie it to an axis (`run.aff[run.route]`)
```

### 계기별 축과 각자의 위치

어떤 계기는 하루에 한 번 발생하지만 day × slot 격자는 모든 행에서 모든 계기를 평가합니다. `--occasion <O>@<axis>,…`(dsl 0.24.0)는 `O`에서 이름 붙인 축만 변화시킵니다. 다른 모든 축이 첫 값에 있고 나머지는 비워진 곳에서 평가됩니다 — `dayStart@run.day`는 하루에 한 번 벨을 읽습니다. `@run.day,run.slot=evening`은 `run.slot`을 첫 값 대신 `evening`으로 고정합니다. 반복 가능한 `--facts <relation>`은 정착된 각 셀의 관계 팩트를 표로 추가합니다 — 첫 번째 인자마다 행을 만들고 셀마다 열을 만들어 사람을 이동시키는 규칙을 보이게 합니다.

```console
$ lute calendar . --axis run.day=1..3 --axis run.slot=morning,evening \
    --occasion dayStart@run.day --occasion placeVisit --target place.dock --facts present
```

그러면 격자는 다음과 같습니다(팩트 표와 목록은 그 뒤에 옵니다).

```console
calendar: . — 6 cell(s) × 2 column(s), from declared defaults
  dayStart: varies over run.day only, at run.slot=morning; blank elsewhere

run.day  run.slot  dayStart (sequence)   placeVisit
                                         place.dock
1        morning   day.bell              dockBo +2
1        evening                         dockGulls +1
2        morning   day.bell              dockBo +2
2        evening                         dock.storm +2
3        morning   day.bell, day.market  dockGulls +1
3        evening                         dockGulls +1
```

같은 6개 셀에서 모든 계기를 평가한 `--facts present`는 다음과 같습니다.

```console
facts present(person, place):
person  1/morning  1/evening  2/morning  2/evening  3/morning  3/evening
ada     -          inn        -          inn        -          inn
bo      dock       -          dock       -          -          -
```

`--json`에서 계기별 열에는 `varies`와 `heldAt`이 들어가며 열 바깥의 셀에는 그 열 결과가 없습니다. 각 셀에는 `facts` 맵(`{ "present": ["present(bo, dock)"] }`)이 있습니다. `--csv`는 `facts:present` 열을 추가합니다.

선언된 [시계](/ko/language/clock/)가 있으면 격자는 `--axis clock=<d1>..<d2>`이고 계기별 열은 시계 축을 이름 붙입니다. `dayStart@clock.day` 또는 시계가 가리키는 자체 경로인 `dayStart@run.day`에 `,clock.slot=<slot>`(또는 `,run.slot=<slot>`)을 붙여 그날의 첫 슬롯이 아닌 슬롯을 고정할 수 있습니다. 이미 가진 경로 위에 항구의 시계를 줍니다.

```yaml
clock:
  day: run.day
  slot: run.slot
  slots: [morning, evening]
```

```console
$ lute calendar . --axis clock=1..3 --occasion dayStart@clock.day,clock.slot=evening \
    --occasion placeVisit --target place.dock
calendar: . — 6 cell(s) × 2 column(s), from declared defaults
  dayStart: varies over clock.day only, at clock.slot=evening; blank elsewhere

clock      dayStart (sequence)   placeVisit
                                 place.dock
1 morning                        dockBo +2
1 evening  day.bell              dockGulls +1
2 morning                        dockBo +2
2 evening  day.bell              dock.storm +2
3 morning                        dockGulls +1
3 evening  day.bell, day.market  dockGulls +1
```
### 어떤 런도 도달하지 않는 셀 버리기

축은 서로 독립적이므로 어떤 런도 도달할 수 없는 조합이 곱에 들어갑니다. `--where <cel>`은 조건이 성립하는 셀만 남깁니다. 셀의 값을 쓰고 퀘스트를 정착시킨 뒤 평가하며, 헤더에는 버린 나머지도 셉니다. `ferry`는 3일째부터만 완료되므로 그 전의 완료 상태는 계획할 수 있는 상태가 아닙니다.

```console
$ lute calendar . --axis run.day=1..3 --axis quest.ferry.state=unset,active,complete \
    --where "quest.ferry.state != 'complete' || run.day >= 3" --occasion dayStart
calendar: . — 7 cell(s) (2 dropped by --where) × 1 column(s), from declared defaults

run.day  quest.ferry.state  dayStart (sequence)
1        unset              day.bell
1        active             day.bell
2        unset              day.bell
2        active             day.bell
3        unset              day.bell, day.market
3        active             day.bell, day.market
3        complete           day.bell, day.farewell, day.market

never eligible in any cell: none

eligible but never presented in any cell: none
```

`--where`는 일반 CEL입니다. 그곳에서는 `@def` 호출이 확장되지 않습니다. 파싱되지 않는 조건은 어떤 것도 평가하기 전에 사용법 오류가 됩니다. 어떤 셀에서 unknown으로 평가되는 조건(셀에 미결정으로 남은 경로를 읽는 경우)도 셀을 추측으로 버리거나 유지할 수 없으므로 그 셀을 이름 붙이는 사용법 오류입니다. `--json`은 버린 셀을 `pruned`로 셉니다.

### 미결정 셀

참조 런타임이 결과를 결정하는 `when`을 판정할 수 없으면 셀은 `?`입니다. 여관에 priority 40인 씬을 추가하고, `when`을 시계가 없는 이력 질의 `validAt('met', ['ada'], quest.ferry.activatedAt)`으로 두면 여관 열에 답할 수 없습니다. 이 이력 질의는 `lute play`에 시계가 없기 때문입니다.

```console
$ lute calendar . --axis run.slot=morning,evening --occasion placeVisit --target place.inn
calendar: . — 2 cell(s) × 1 column(s), from declared defaults

run.slot  placeVisit
          place.inn
morning   ? +1
evening   ? +1
```

`undecided` 블록에는 셀마다 이유가 나옵니다.

```
undecided (an unknown `when` decides the cell; play halts there):
  run.slot=morning  placeVisit@place.inn: inn.oldFriend — `validAt('met', ['ada'], quest.ferry.activatedAt)` evaluates unknown: now()/validAt(...) has no reference-runtime resolution
  run.slot=evening  placeVisit@place.inn: inn.oldFriend — `validAt('met', ['ada'], quest.ferry.activatedAt)` evaluates unknown: now()/validAt(...) has no reference-runtime resolution
```

승자 아래에 있는 unknown `when`은 셀을 미결정으로 만들지 않습니다. 승자는 그 조건이 어떻게 평가되든 제시됩니다.

### 기계 판독 가능한 출력

`--json`은 셀이 시작한 위치(`from`, 헤더의 텍스트), `--where`가 버린 셀 수(`pruned`), 축, 열, 셀마다 하나의 레코드, 자격이 없는 목록과 제시되지 않은 목록(`neverPresented`, 각각 `beatenBy` 포함)을 출력합니다. 대상 지정 계기의 열에는 `target`이 들어가며 `(any)` 열에는 `"anyTarget": true`가 들어갑니다. 셀의 `results`에는 열마다 하나씩 `winner`(아무것도 자격 없거나 `select: all`/`sequence` 열이면 `null`), `presented`(승자 또는 제공/시퀀스 전체), `shadowed`가 들어갑니다. `when`이 unknown이면 `unknown`(`[{ id, reason }]`)도 들어가며 그 unknown이 셀을 결정하면 `"undecided": true`가 붙습니다. 퀘스트 정착이 멈추거나 축 값을 움직인 셀에는 `notes` 목록이 들어갑니다.

```json
{
  "axes": [ { "path": "run.day", "values": [3] }, { "path": "run.slot", "values": ["morning"] } ],
  "cells": [
    {
      "at": { "run.day": 3, "run.slot": "morning" },
      "results": [
        { "occasion": "board", "select": "all", "winner": null, "presented": ["noteFair"], "shadowed": [] },
        { "occasion": "dayStart", "select": "sequence", "winner": null, "presented": ["day.bell", "day.market"], "shadowed": [] },
        { "occasion": "placeVisit", "target": "place.dock", "select": "first", "winner": "dockGulls", "presented": ["dockGulls"], "shadowed": ["dockEmpty"] },
        { "occasion": "placeVisit", "target": "place.inn", "select": "first", "winner": "innQuiet", "presented": ["innQuiet"], "shadowed": [] }
      ]
    }
  ],
  "columns": [ { "occasion": "board", "select": "all" }, … ],
  "from": "declared defaults",
  "neverEligible": [
    { "id": "dockBo", "kind": "entry", "document": "lore/places.lute", "on": "placeVisit", "target": "place.dock", "reasons": ["when: false"] },
    …
  ],
  "neverPresented": [
    { "id": "dockEmpty", "kind": "entry", "document": "lore/places.lute", "on": "placeVisit", "target": "place.dock", "beatenBy": ["dockGulls"] }
  ],
  "pruned": 0
}
```

`--csv`는 셀과 열마다 한 행을 씁니다. 축 값 뒤에 `occasion,target,select,winner,presented,shadowed,unknown,notes`가 오며, 목록은 `;`로 합칩니다. `(any)` 열의 target은 `(any)`로 표시합니다. 스프레드시트용 첫 테이블 뒤 빈 줄 다음에는 자격은 있었지만 제시되지 않은 비트와 무엇이 이겼는지를 담은 두 번째 테이블이 옵니다.

```console
$ lute calendar . --axis run.day=2..3 --axis run.slot=evening --occasion placeVisit --csv
run.day,run.slot,occasion,target,select,winner,presented,shadowed,unknown,notes
2,evening,placeVisit,place.dock,first,dock.storm,dock.storm,dockGulls;dockEmpty,,
2,evening,placeVisit,place.inn,first,inn.ada,inn.ada,,,
3,evening,placeVisit,place.dock,first,dockGulls,dockGulls,dockEmpty,,
3,evening,placeVisit,place.inn,first,inn.ada,inn.ada,,,

neverPresented,kind,document,occasion,target,beatenBy
dockEmpty,entry,lore/places.lute,placeVisit,place.dock,dock.storm;dockGulls
```

### 제한과 종료 코드

calendar는 `lute play`와 같은 방식으로 프로젝트 전체를 컴파일하므로 `lute beats`와 달리 컴파일되는 프로젝트가 필요합니다. [위의 비트](#lute-beats)에 나온 모순된 폭풍과 함께 실행하면 다음과 같습니다.

<!-- lute-diagnostics -->
```console
$ lute calendar . --axis run.day=1..3
./scenes/dock/storm.lute:7:8: error [E-BEAT-UNREACHABLE] beat `dock.storm` is never eligible: its `when` `run.slot == 'morning' && run.slot == 'evening'` is provably false
```

표준 오류에는 `lute calendar: 1 of 9 document(s) failed to compile; refusing to evaluate the calendar`가 추가되고 종료 코드는 **1**입니다.

모든 축은 셀을 평가하기 전에 검증됩니다. 선언되지 않은 경로, 경로 타입 밖의 값, 비어 있거나 정수가 아닌 범위, 두 번 주어진 축, status나 objective `done`이 아닌 퀘스트 경로, 선언되지 않은 퀘스트, 파생 또는 선언되지 않은 관계에 대한 `holds(…)`(또는 `true`/`false` 이외의 값), 나열된 계기 중 어느 것도 받지 않는 `--target`, **10,000셀**을 넘는 격자는 사용법 오류입니다. 스텝을 이름 붙이지 않는 `--until`, 멈추는 재생, 파싱되지 않거나 어떤 셀에서 unknown인 `--where`도 마찬가지입니다.

```console
$ lute calendar . --axis run.slot=noon
lute calendar: `--axis run.slot`: `run.slot` does not take `noon`: the enum's members are morning, evening
$ lute calendar . --axis quest.ferry.activatedAt=1
lute calendar: `--axis quest.ferry.activatedAt`: `quest.ferry.activatedAt` is the quest lifecycle's own bookkeeping and cannot be set per cell — an axis over a quest is `quest.<id>.state` (its status) or `quest.<id>.objectives.<oid>.done`
$ lute calendar . --script plays/evening.play.yaml --until "back at the ink"
lute calendar: plays/evening.play.yaml: `--until back at the ink` names no step number or `label:` of the script — did you mean `back at the inn`?
$ lute calendar . --axis run.day=1..200 --axis user.visits=0..60
lute calendar: the axes' product exceeds 10000 cells
```

성공 시 종료 **0**, 프로젝트가 컴파일되지 않으면 **1**, I/O 또는 사용법 오류면 **2**입니다.

## `lute scenario knowledge`

```console
$ lute scenario <dir> [--format text|json] knowledge [--for <node>]
```

모든 가드 슬롯에서 팩트를 질의하는 조건(`holds(…)`, `count(…)`, `countDistinct(…)`, `validAt(…)`)을 나열합니다(dsl 0.24.0). 대상에는 beat, entry 및 objective 가드, 줄의 `when=`, `<choice when>`, `<when>` arm 테스트, `::jump`/`::set` `when`, `<on when>`, reward `when`, quest `start`/`fail`, objective `until`이 모두 포함됩니다. 조건은 문서별로 묶고 소스 줄을 함께 표시하며, 각 질의 원자는 무엇이 생성할 수 있는지까지 추적합니다. 파생 원자는 규칙을 따라가되, 규칙 head의 변수를 해당 원자의 상수에 바인딩하므로 필요한 전제를 ground atom으로 표시합니다. 기반 원자는 producer에서 끝납니다.

- **asserted by** — `::assert`로 이를 생성할 수 있는 씬(씬 key로 표시), 퀘스트, 엔트리와 번들 비트(정규 id `<document id>.<beat id>`로 표시) 및 그 문서입니다.
- **seed facts** — 이에 맞는 스키마의 `facts:`입니다.
- **reserved** — `reserved: true` 관계로 엔진이 단언합니다.
- **NO PRODUCER** — 아무것도 이를 단언하지 않고 seed fact도 아니며 reserved도 아니고 규칙도 없습니다. 조건은 아직 누구도 작성하지 않은 콘텐츠를 기다립니다.

```console
$ lute scenario . knowledge
project root: .
  knowledge (every fact-guarded condition, by document -> relations read -> producers):

  lore/places.lute
    entry `innQuiet`
      when: !holds('present', ['ada', 'inn'])
      not present(ada, inn) — holds unless defeated
        defeated when present(ada, inn) is derived ⇐ cel("run.slot == 'evening'")
        rule: present(ada, inn) :- cel("run.slot == 'evening'")
          cel("run.slot == 'evening'") — state condition on run.slot, decided at run time

    entry `dockBo`
      when: holds('present', ['bo', 'dock'])
      present(bo, dock) — derived by 1 rule
        rule: present(P, dock) :- works(P, dock), cel("run.slot == 'morning' && run.day != 3")
          works(bo, dock) — seed facts works(bo, dock)
          cel("run.slot == 'morning' && run.day != 3") — state condition on run.day, run.slot, decided at run time

    entry `noteDocked`
      when: holds('arrived', ['dock'])
      arrived(dock) — reserved — the engine asserts it

  quests/ferry.lute
    objective `ferry.word`
      done: holds('trusted', ['ada'])
      trusted(ada) — derived by 1 rule
        rule: trusted(P) :- met(P), not rumor(P)
          met(ada) — asserted by scene `inn.ada` (scenes/inn/ada.lute)
          not rumor(ada) — always holds (nothing produces rumor(ada): no assert, seed fact, engine write or rule) — cannot be defeated

  scenes/inn/ada.lute
    scene `inn.ada`
      when: holds('present', ['ada', 'inn'])
      present(ada, inn) — derived by 1 rule — traced above under entry `innQuiet` in lore/places.lute

  scenes/inn/again.lute
    scene `inn.again`
      when: holds('trusted', ['ada'])
      trusted(ada) — derived by 1 rule — traced above under objective `ferry.word` in quests/ferry.lute
```

질의한 원자를 결론낼 수 있는 규칙만 따라갑니다. `dockBo`는 `present(bo, dock)`을 묻기 때문에 Ada를 여관에 두는 규칙은 그 아래에 나오지 않습니다. 파생 원자의 규칙은 보고서마다 한 번 출력하며, 이후 언급에서는 어디에서 추적했는지 말합니다. 부정은 “패배하기 전까지 holds”로 읽고, 이를 패배시킬 수 있는 팩트마다 한 줄씩 표시합니다 — `defeated when <fact> is asserted by …`, `… is derived ⇐ <premises>`, `defeated when the engine asserts <fact> (reserved)`, 또는 `defeated from the start: <fact> is a seed fact`입니다. `not rumor(ada)`에는 producer가 없으므로 “항상 성립 … — 패배시킬 수 없음”이라고 합니다. Ada를 불신하게 만들 소문은 선언되었지만 아무도 쓰지 않는 관계이기 때문입니다. (0.23에서는 이를 `NO PRODUCER`로 출력했는데 경고처럼 읽혔습니다. producer 없는 양의 읽기는 여전히 `NO PRODUCER`라고 합니다.) [`check-project --wip`](/ko/tooling/cli/#check-project)에서는 그런 관계 때문에만 dead인 가드가 오류가 아니라 경고입니다.

규칙의 모든 전제는 그 아래에서 추적됩니다. `cel(…)` 전제는 규칙이 미리 판정할 수 없는 상태 조건이므로 읽는 경로를 표시합니다 — `cel("run.slot == 'evening'") — state condition on run.slot, decided at run time` — 그리고 entity-kind 원자는 producer 없는 관계가 아니라 멤버십 검사입니다. trust 규칙을 `trusted(P) :- person(P), met(P), not rumor(P)`로 쓰면 다음과 같습니다.

```console
$ lute scenario . knowledge --for ferry.word
project root: .
  knowledge (every fact-guarded condition, by document -> relations read -> producers):

  quests/ferry.lute
    objective `ferry.word`
      done: holds('trusted', ['ada'])
      trusted(ada) — derived by 1 rule
        rule: trusted(P) :- person(P), met(P), not rumor(P)
          person(ada) — entity kind `person`; ada is a member
          met(ada) — asserted by scene `inn.ada` (scenes/inn/ada.lute)
          not rumor(ada) — always holds (nothing produces rumor(ada): no assert, seed fact, engine write or rule) — cannot be defeated
```

dsl 0.26.0 §6부터 규칙 본문은 개수를 셀 수 있습니다 — `canPass(pier) :- count(hasItem(_)) >= 2` — 이 개수도 다른 전제와 같이 추적하며, 세는 팩트와 그 producer를 아래에 표시합니다.

```console
$ lute scenario . knowledge --for pier.boat
project root: .
  knowledge (every fact-guarded condition, by document -> relations read -> producers):

  scenes/boat.lute
    scene `pier.boat`
      when: holds('canPass', ['pier'])
      canPass(pier) — derived by 1 rule
        rule: canPass(pier) :- count(hasItem(_)) >= 2
          count('hasItem', ['_']) >= 2 — counts:
            hasItem(_) — asserted by scene `wren.talk` (scenes/wren.lute)
```

프로젝트가 **거짓으로 만들 수 있는** 부정 전제는 그렇게 할 수 있는 방법을 따라갑니다. 규칙 본문은 어떤 런에서든 성립할 수 있는 모든 팩트에 대입됩니다(`check-project`가 여기서는 모든 assert 위치를 활성으로 보고 may 집합을 정함). 양의 전제가 정한 상수는 부정으로 가져갑니다(`not liar(hollis)`이지 `not liar(_)`가 아님). 번들 비트가 소문을 쓰면 다음과 같습니다.

```console
          not rumor(ada) — holds unless defeated
            defeated when rumor(ada) is asserted by beat `town.gossip.whisper` (lore/gossip.lute)
```

파생된 패배 원인은 이를 만들기 위해 조인하는 팩트를 표시합니다. 여기서는 `!holds('present', ['bo', 'dock'])`로 가드된 엔트리입니다.

```console
      not present(bo, dock) — holds unless defeated
        defeated when present(bo, dock) is derived ⇐ works(bo, dock) [seed], cel("run.slot == 'morning' && run.day != 3")
```

팩트에 여러 파생 경로가 있으면 모든 경로를 나열합니다(dsl 0.25.0 §9). 첫 경로만이 아니라 부정이 실패하는 모든 방법을 ` / `로 구분해 보여 줍니다. `present(P, L) :- hired(P, L)` 규칙과 `hired(bo, dock)`을 단언하는 `dock.hire` 씬을 추가하면 다음과 같습니다.

```console
      not present(bo, dock) — holds unless defeated
        defeated when present(bo, dock) is derived ⇐ works(bo, dock) [seed], cel("run.slot == 'morning' && run.day != 3") / ⇐ hired(bo, dock) [scene `dock.hire` (scenes/dock/hire.lute)]
```

전제 하나당 패배 팩트는 최대 세 개까지 출력하고, 그 뒤에는 `… and N more defeating facts`가 붙습니다. 열린 인자 도메인의 어떤 tuple이든 성립할 수 있는 부정 관계는 패배 줄 뒤에 `` — any `rumor` tuple may hold ``라고 합니다.

`--for <node>`는 씬(그 안의 모든 가드 — `inn.again`), 번들 비트, 엔트리 id(`dockBo`), `<quest>.<objective>` 형태의 퀘스트 목표(`ferry.word`), 퀘스트의 모든 목표를 위한 `quest:<id>`, 또는 한 choice의 가드를 위한 `<scene>#<branch>.<choice>`를 선택합니다. 줄과 choice가 팩트로 가드된 `inn.ask` 씬이 있다면 다음과 같습니다.

```console
$ lute scenario . knowledge --for inn.ask
project root: .
  knowledge (every fact-guarded condition, by document -> relations read -> producers):

  scenes/inn/ask.lute
    line `@ada` (line 9)
      when: holds('present', ['ada', 'inn'])
      present(ada, inn) — derived by 1 rule
        rule: present(ada, inn) :- cel("run.slot == 'evening'")
          cel("run.slot == 'evening'") — state condition on run.slot, decided at run time

    choice `ask.ferry` (line 12)
      when: holds('trusted', ['ada'])
      trusted(ada) — derived by 1 rule
        rule: trusted(P) :- met(P), not rumor(P)
          met(ada) — asserted by scene `inn.ada` (scenes/inn/ada.lute)
          not rumor(ada) — always holds (nothing produces rumor(ada): no assert, seed fact, engine write or rule) — cannot be defeated
```

`--for 'inn.ask#ask.ferry'`는 choice만 남깁니다. 팩트로 가드된 조건이 없는 node는 사용법 오류(종료 2)이며 가까운 이름이면 did-you-mean을 붙입니다.

```console
$ lute scenario . knowledge --for ferry.wrod
lute scenario knowledge: `--for ferry.wrod` names no fact-guarded condition (a scene, bundle beat, entry, `quest:<id>`, `<quest>.<objective>` or `<scene>#<branch>.<choice>`) — did you mean `ferry.word`?
```

`--format json`(모든 `scenario` 옵션처럼 하위 명령 앞에 둠)은 `{ "roots": [ { "root", "elements", "relations" } ] }`를 출력합니다. 각 element는 `{ node, document, line?, for, conditions, reads }`입니다. `for`에는 이를 선택하는 `--for` 핸들(`["inn.ask", "inn.ask#ask.ferry"]`)이 들어가고, `conditions`는 슬롯(`when`, `done`, …)별 확장 텍스트를 맵으로 가지며, `reads`는 질의하는 원자와 부정된 읽기 `!present(ada, inn)`를 나열합니다. `relations`는 관련된 각 관계를 `{ declared, derived, reserved, seedFacts, assertedBy, rules }`로 맵핑하고, 각 rule은 `{ rule, premises }`입니다. 전제는 `{ relation, negated? }`, kind 원자 `{ entityKind }`, `cel(…)` 전제 `{ cel }`(조건 텍스트) 중 하나입니다. `--format dot`은 knowledge 맵이 그래프가 아니므로 거부됩니다.

`lute scenario envelope`는 Facts 행도 같은 방식으로 표현합니다(`works/2 (producible) — seed facts works(bo, dock)`, `arrived/1 (producible) — reserved — the engine asserts it`, `trusted/1 (producible) — derived by 1 rule`). [`lute lore`](/ko/tooling/cli/#lore)는 같은 추적을 사용해 Derived 섹션으로 끝납니다. 규칙이 도달할 수 있는 각 결론, 그 규칙 인스턴스, 그 뒤의 근거 및 해당 결론이 게이트하는 조건을 보여 줍니다.

성공 시 종료 **0**, I/O 오류 또는 일치하지 않는 `--for`면 **2**입니다.
### 시나리오 그래프에서 생략하는 것

[시나리오 그래프](/ko/connectivity/scene-graph/)는 연결성이 분석하는 전제조건만 그립니다. 퀘스트의 간선은 `follows=`, 하위 퀘스트 트리, `start`의 앵커링 conjunct, 그리고 퀘스트를 취하는 `::accept`에서 나옵니다(dsl 0.24.0 §2, 0.25.0 §4). [퀘스트와 씬](/ko/language/quests-and-scenes/#quest)을 참고하세요. 기본 `lute scenario` 보기에는 그 결과 그리지 않은 것이 무엇인지도 표시됩니다. 등대를 예로 들어 봅시다. `start="entry.keeperLog.everRead && visited('arrival')"`이고 하위 퀘스트 `oil`, `wick` 두 개가 있는 `relight` 퀘스트, 게시판에서 엔진이 수락하는 `salvage` 퀘스트(`accept="external"`), 테스트만 수락하는 `lostDog` 퀘스트, `after="visited('arrival')"`인 `greeting`과 `when="visited('keeper.greeting')"`로 가드된 `warning` 두 keeper 비트, 그리고 `after: completed("salvage")`인 `farewell` 씬이 있습니다.

```console
$ lute scenario .
project root: .
  topological layers:
    layer 0: scene(arrival), scene(farewell), beat(keeper.warning), entry(keeperLog)
    layer 1: quest(relight), beat(keeper.greeting)
    layer 2: quest(oil), quest(wick)
  edges (prerequisite -> dependent) [atom kind(s)]:
    scene(arrival) -> quest(relight) [start]
    scene(arrival) -> beat(keeper.greeting) [visited]
    quest(relight) -> quest(oil) [subquest]
    quest(relight) -> quest(wick) [subquest]
    entry(keeperLog) -> quest(relight) [start]
  unanchored (no `after` / `follows` — available from the start of play; no prerequisites in this graph):
    quest(lostDog)
    quest(salvage)
    beat(keeper.warning) — its `when` reads visited('keeper.greeting'), which gates it but draws no edge; write `after="visited('keeper.greeting')"` to anchor it
  note: 6 `visited()`/`completed()`/`active()` reference(s) not drawn — a quest's edges come from its `follows`, its subquest tree, its `start` conjuncts and its `::accept`s:
    scene(farewell) -> completed("salvage") — quest(salvage) is on no edge (no `follows`, tree, `start` anchor or `::accept`)
    quest(relight) reads visited('keeper.warning') in its objective climb done — a condition read, not an anchor
    quest(oil) reads visited('arrival') in its objective fetch done — a condition read, not an anchor
    quest(wick) reads visited('arrival') in its objective trim done — a condition read, not an anchor
    quest(salvage) reads visited('arrival') in its objective dive done — a condition read, not an anchor
    quest(lostDog) reads visited('arrival') in its objective find done — a condition read, not an anchor
```

`relight`는 `start` conjunct들로 앵커되므로 `follows=`가 필요하지 않으며, `oil`과 `wick`은 `[subquest]` 간선으로 그 아래에 매달립니다. `keeper.greeting`은 자신의 `after=`로 순서가 정해집니다. `keeper.warning`은 같은 종류의 팩트를 `when`에서 읽지만 아무것도 순서화하지 않으므로, 작성할 `after=`와 함께 표시됩니다. `salvage`와 `lostDog`는 어떤 간선에도 없으므로 `farewell`의 `after:`에 있는 `completed("salvage")`도 그려지지 않으며, 목표의 `visited()` 읽기는 조건일 뿐 앵커가 아닙니다.

`--format json`은 루트의 `unanchored`에 앵커되지 않은 노드를, `unanchoredHints`(노드 → 힌트)에 비트 힌트를, 그리지 않은 참조를 `omitted`에 담습니다. 각 `omitted` 항목은 `completed`/`active` 참조의 경우 `{ from, kind, quest }`, 퀘스트의 `visited()` 읽기의 경우 `{ from, kind: "visited", scene, slot }`입니다. `lute scenario . reach quest:relight`는 퀘스트 앵커(`anchors`, JSON에서는 각 항목이 `{ kind, from }`)를 나열합니다.

```console
$ lute scenario . reach quest:relight
project root: .
reach quest(relight):
  verdict: Reachable — a satisfiable route exists under your declared routes.
  follows: (none declared) — anchored; each anchor holds before it activates, through any one of its sources:
    [start] entry(keeperLog)
    [start] scene(arrival)
  referenced node(s) (see the anchors above — this is NOT a flat requirement list):
    - scene(arrival): Reachable — a satisfiable route exists under your declared routes.
    - entry(keeperLog): Reachable — a satisfiable route exists under your declared routes.
```

로어 엔트리는 퀘스트의 `start` 앵커 소스인 `entry(<id>)`로서만 그래프에 들어옵니다. 번들 비트는 항상 노드입니다. `after=`가 없는 비트는 엔트리 노드이고, `after=`가 있는 비트는 자신이 그리는 간선의 종속 노드입니다. `lute scenario . reach keeper.greeting`(또는 `reach beat:keeper.greeting`)은 이를 선언한 파일과 `on`, `target`, `when`을 출력합니다. [씬 그래프](/ko/connectivity/scene-graph/#번들-비트)를 참고하세요.

### 팩트 간선(`lute scenario --facts`)

```console
$ lute scenario <dir> [--format text|json|dot] --facts
```

수집 게임의 진행은 방문이 아니라 팩트로 게이트됩니다. 배지가 있어야 체육관이 열리고 렌즈가 있어야 탑이 열립니다. 그런 게이트는 `after:` 간선을 그리지 않으므로 기본 그래프에서는 모든 체육관이 레이어 0에 놓입니다. `--facts`(dsl 0.26.0 §8)는 씬, 비트 또는 퀘스트의 게이트(`when:` 또는 `start=`)가 `holds(F)`를 읽고 producer가 `F`를 단언하는 곳에 **팩트 간선** `producer -> reader [fact]`를 추가합니다. `F`가 파생되면, 파생 규칙이 필요로 하는 팩트를 단언하는 곳에서 출발하여 규칙의 결론을 표시하며 `[hasItem(goodRod), via canPass(pier)]`처럼 출력합니다. 이후 순환을 닫지 않는 곳에서는 팩트 간선 위에 레이어도 그리므로, 팩트가 여는 것은 그것을 제공하는 것보다 아래에 놓입니다. Wren의 씬은 막대기를 제공합니다(`::assert{hasItem(goodRod)}`). 호수는 `holds('hasItem', ['goodRod'])`를 읽고, 보트는 `canPass(pier) :- hasItem(goodRod)` 규칙으로 파생되는 `holds('canPass', ['pier'])`를 읽습니다.

```console
$ lute scenario .
project root: .
  topological layers:
    layer 0: scene(pier.boat), scene(pier.lake), scene(wren.talk)
  edges (prerequisite -> dependent) [atom kind(s)]:
    (none)
$ lute scenario . --facts
project root: .
  topological layers (after: and fact edges):
    layer 0: scene(wren.talk)
    layer 1: scene(pier.boat), scene(pier.lake)
  edges (prerequisite -> dependent) [atom kind(s)]:
    (none)
  fact edges (producer -> reader) [asserted fact]:
    scene(wren.talk) -> scene(pier.boat) [hasItem(goodRod), via canPass(pier)]
    scene(wren.talk) -> scene(pier.lake) [hasItem(goodRod)]
```

전체 게임에서 이 간선은 한 지역의 게이트가 다른 지역의 씬에 매달린 모습을 보여 줍니다 — Monster League에서는 `scene(mid.gameCorner) -> scene(east.ashTowerLens) [hasItem(spectralLens)]`입니다. `--format json`은 루트에 `factEdges`를 추가하며 각 항목은 `{ from, to, fact, via?, layered }`입니다(`via`는 게이트가 파생 팩트를 읽을 때, `layered`는 간선이 레이어링에 참여했는지를 나타냄). `--format dot`은 각각을 점선과 보라색으로, 팩트를 레이블로 붙여 그립니다. `--facts`는 이 그래프 보기에서만 쓰는 옵션입니다. `reach`, `envelope`, `check-project`의 연결성 패스는 여전히 선언된 `after:` 구조를 읽습니다.

## `project.index.json`의 비트 행

개요 페이지는 엔진과 같은 비트 테이블을 읽습니다. 0.23.0부터 [`project.index.json`](/ko/tooling/cli/#--all--프로젝트-전체-컴파일-및-인덱스)의 비트 행에는 비트의 `when`이 포함됩니다. 모든 `@def`가 확장된 형태이므로 엔진이나 도구가 스키마 없이도 표시할 수 있습니다. 또한 `select: all` 메뉴가 보여 주는 레이블인 `title`도 포함됩니다. 비트에 둘 중 하나가 없으면 각각 생략되므로 둘 다 사용하지 않는 프로젝트는 바이트 단위로 동일하게 컴파일됩니다. [번들 비트](/ko/tooling/play/#번들-비트)의 행 종류는 `bundle`입니다. dsl 0.26.0부터 [종류 비트](/ko/tooling/play/#종류-대상)의 행은 `target: "kind:<kind>"`와 `targetKind: { kind, prefix, members }`를 가지므로 엔진이 모든 `<prefix>.<member>` 발생에 이를 제공할 수 있습니다 — `{"kind": "trainer", "prefix": "npc", "members": ["gus", "r16Gus"]}`.

```json
{"id": "noteFerry", "kind": "entry", "document": "lore/places.lute", "on": "board", "priority": 0, "when": "run.day <= 2", "title": "Ferry times"}
{"id": "day.market", "kind": "scene", "document": "scenes/day/market.lute", "on": "dayStart", "priority": 0, "once": "run", "when": "run.day == 3", "title": "Market day"}
{"id": "dock.storm", "kind": "scene", "document": "scenes/dock/storm.lute", "on": "placeVisit", "target": "place.dock", "priority": 50, "once": "run", "when": "run.slot == 'evening' && run.day == 2"}
```

## 0.34.0의 evidence

`calendar`의 셀 결과와 집계 행에는 `evidence`가 들어가며, 유한하게 한정된 집계에는 유한한 `scope`(축, 곱, seed, 그리고 모든 `--where` 필터)도 들어갑니다. 따라서 calendar 결과는 평가한 격자에 한정된 결과이지, 모든 런타임 상태에 대한 주장이 아닙니다. `scenario`, `beats`, `play`, `test`도 JSON에서 같은 evidence 어휘를 노출합니다.
