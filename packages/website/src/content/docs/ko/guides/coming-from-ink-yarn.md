---
title: Ink나 Yarn에서 오셨다면
description: "Ink와 Yarn Spinner에서 Lute로 옮기는 대응표: knot과 노드, divert와 jump, 선택지와 반복, 변수, 조건부 텍스트, 시퀀스, 엔딩, 주석과 태그, 그리고 .lute 파일에 Ink나 Yarn 문법이 섞여 들었을 때 검사기가 하는 말."
---

Ink와 Yarn Spinner는 흐름(flow)입니다: 텍스트가 위에서 아래로 흐르고, divert나 jump가 위쪽을 포함해
어디로든 보냅니다. Lute는 그 일을 둘로 나눕니다:

- **장면 안에서** 텍스트는 앞으로만 흐릅니다. 메뉴는 `<branch>`(한 번 묻습니다)와 `<hub>`(플레이어가 떠날
  때까지 다시 묻습니다)이고, 상태에 따라 줄을 고르는 것은 `<match>`와 `when=`입니다.
- **장면 사이에서는** 아무것도 divert하지 않습니다. 엔진이 **계기(occasion)**("다음 장", "플레이어가 틸리에게
  말을 건다")를 발생시키고, 각 장면은 자기가 어느 계기에 응답하는지 말합니다. 순서는 `after:`,
  `priority:`, 매니페스트의 `chapters:`가 정합니다. [장면을 이야기로
  잇기](/ko/getting-started/connect-scenes/)가 차근차근 보여 줍니다.

어떤 경로도 뒤로 뛰거나 저절로 반복하지 않으므로, 검사기는 장면을 지나는 모든 경로를 따라갈 수 있고,
그래서 어느 경로도 막다른 길이 아님을 증명할 수 있습니다.

## 대응표

| Ink | Yarn Spinner | Lute |
|---|---|---|
| `=== knot ===` | 노드(`title:` … `===`) | 장면: 자기 `.lute` 파일 하나, 프런트매터에 `kind: scene`과 `id:` |
| `= stitch` | — | 장면 안의 `## 제목` |
| 이야기의 다음 부분으로 가는 `-> knot` | `<<jump Node>>` | 계기(`on:`)에 응답하는 다른 장면. 순서는 `after:`나 `chapters:` |
| 플레이어가 돌아오는 메뉴로 가는 `-> knot` | 허브 노드로 돌아가는 `<<jump>>` | `<hub>`: 선택지 하나가 끝날 때마다 다시 묻고, `exit` 선택지에서 끝납니다 |
| 아래쪽의 이름 붙은 gather로 가는 `-> label` | — | 아래쪽 `::mark{id="label"}`로 가는 `::next{to="label"}`. 앞으로만 |
| 한 번만 고르는 `* [choice]` | `-> option <<once>>` | 허브의 `<choice … once>`. `<branch>`는 어차피 한 번만 묻습니다 |
| 계속 남는 `+ [choice]` | `-> option` | `<hub>` 안의 평범한 `<choice>` |
| `- gather` | 선택지 다음 줄들 | `</branch>` 다음 줄들. 어느 선택지를 골랐든 실행됩니다 |
| `VAR oil = 1` | `<<declare $oil = 1>>` | 스키마나 프런트매터의 `state:` 아래 `run.oil: { type: int, default: 1 }` |
| `CONST MAX = 5` | — | `defs:` 아래의 def. `@MAX`로 읽습니다 |
| `LIST mood = calm, stormy` | — | enum 타입 경로: `run.mood: { type: { enum: [calm, stormy] }, default: calm }` |
| `~ oil = oil + 2` | `<<set $oil to $oil + 2>>` | `::set{run.oil += 2}` |
| 텍스트 안의 `{oil}` | 텍스트 안의 `{$oil}` | `{{run.oil}}` |
| `{oil > 0: text}` | `<<if $oil > 0>>` | 가드를 단 줄: `@narrator{when="run.oil > 0"}: text` |
| `{x: a \| b}` | `<<if>> … <<else>>` | 가드를 단 줄 두 개, 또는 `<otherwise>` 갈래가 있는 `<match>` |
| `{a \| b \| c}`, `{&a \| b}`, `{!a \| b}` | `<<once>> … <<else>>` | `scene.*` 카운터와 그것을 읽는 `<match>`([아래](#바뀌는-텍스트)) |
| `{~a \| b}` | 줄 그룹(`=>`), `dice()`, `random()` | Lute에는 무작위 선택이 없습니다([아래](#바뀌는-텍스트)) |
| 방문 횟수 `{knot}`, `{knot > 2}` | `visited_count("Node")` | 직접 `::set{… += 1}` 하는 `int` 경로. `visited('<id>')`는 참/거짓뿐입니다 |
| `-> DONE` | `<<stop>>` | `::end`, 또는 그냥 장면의 끝 |
| `-> END` | 이야기의 끝 | `::set`이 참으로 만드는 스키마의 `terminal:` 조건([아래](#이야기-끝내기)) |
| 터널 `-> knot ->` | — | 컴포넌트: `::use{component="…"}` |
| `INCLUDE` | 여러 `.yarn` 파일 | 프로젝트: `lute.project.yaml` 아래의 모든 `.lute` 파일. 공유 상태는 `uses:`로 가져오는 스키마에 |
| — | 노드 그룹(`when:` 헤더) | 한 계기에 응답하는 여러 장면, 각자 자기 `when:`. 자격 있는 장면 중 `priority:`가 가장 높은 것이 재생됩니다([Beats](/language/beats/)) |
| — | `when: once`, `when: always` | `once:` 키: `once: run`(기본값), `once: user`, `once: false` |
| `# tag` | `#tag` | 자유 형식 줄 태그는 없습니다([아래](#주석과-태그)) |
| — | 현지화 줄 id `#line:…` | 콘텐츠 줄의 `code=`: `@narrator{code="0010"}: …`. `lute tag`가 채워 넣고, 컴파일된 `lineId`는 이것으로 만들어집니다([Dialogue & cast](/language/dialogue-and-cast/)) |

## knot 하나를 옮기기

어느 Ink 이야기의 등대 램프실입니다: 플레이어가 계속 돌아오는 메뉴, 장부를 읽을 때마다 바뀌는 텍스트,
한 번만 고르는 선택지, 인라인 조건이 있습니다.

```
VAR oil = 1

=== lamp_room ===
{lamp_room == 1: The lamp room smells of cold brass.|The lamp room again. The dark is closer.}
+ [Read the ledger] -> ledger
* [Go down to the stores] -> stores
+ [Wait for dark] -> dusk

=== ledger ===
{ledger:
- 1: The last entry is three weeks old. "Oil low. Ship due."
- 2: You read it again. The handwriting shakes toward the end.
- else: The words have stopped changing.
}
-> lamp_room

=== stores ===
You find two more cans of oil.
~ oil = oil + 2
-> lamp_room

=== dusk ===
{oil >= 3: The lamp catches.|The wick sputters.}
-> END
```

Lute에서는 knot 네 개가 장면 하나입니다. `lamp.lute`로 저장하세요:

```lute check
---
kind: scene
id: lamp
title: The Lamp Room
state:
  run.oil:           { type: int, default: 1 }
  scene.ledgerReads: { type: int, default: 0 }
---

## The Lamp Room

@narrator: The lamp room smells of cold brass.
<hub id="lamp">
  <return>
    @narrator: The lamp room again. The dark is closer.
  </return>
  <choice id="ledger" label="Read the ledger">
    ::set{scene.ledgerReads += 1}
    <match on="scene.ledgerReads">
      <when is="1">
        @narrator: The last entry is three weeks old. "Oil low. Ship due."
      </when>
      <when is="2">
        @narrator: You read it again. The handwriting shakes toward the end.
      </when>
      <otherwise>
        @narrator: The words have stopped changing.
      </otherwise>
    </match>
  </choice>
  <choice id="stores" label="Go down to the stores" once>
    @narrator: You find two more cans of oil.
    ::set{run.oil += 2}
  </choice>
  <choice id="dusk" label="Wait for dark" exit>
    @narrator{when="run.oil >= 3"}: The lamp catches.
    @narrator{when="run.oil < 3"}: The wick sputters.
  </choice>
</hub>
```

- **플레이어가 돌아오는 knot이 `<hub>`입니다.** 딸린 knot 하나하나가 허브의 선택지이고, 각 knot 끝의
  `-> lamp_room`은 적지 않아도 됩니다: 선택지의 줄이 실행되고 나면 허브가 다시 묻습니다. `exit`
  선택지만 허브를 떠납니다.
- **`+`는 평범한 선택지, `*`는 `once`입니다.** `once` 선택지는 한 번 고르면 메뉴에서 빠집니다.
- **처음 들어올 때의 텍스트**는 `<hub>` 앞의 줄이며 한 번만 재생됩니다. 돌아올 때마다의 텍스트는
  `<return>` 블록입니다: `exit`이 아닌 선택지가 끝날 때마다, 메뉴가 다시 나오기 전에 실행됩니다.
  마지막 `once` 선택지가 메뉴를 비울 때도, 허브가 닫히기 직전에 실행됩니다.
- **방문 횟수** `{ledger: - 1 … - 2 … - else …}`는 선택지가 직접 세는 숫자와 그것을 읽는 `<match>`입니다.
  허브도 고른 것을 `scene.visited.lamp.ledger`로 기록하지만, 그 값은 참/거짓뿐이고 선택지 자신의 줄
  안에서는 이미 참입니다: 고른 것은 그 줄들이 실행되기 전에 기록됩니다.
- **인라인 조건** `{oil >= 3: …|…}`는 각자 `when=`을 단 두 줄입니다.
- `run.oil`은 이 파일이 혼자서 검사되도록 이 장면에서 선언했습니다. 프로젝트에서는 다른 장면도 읽으므로
  모든 장면이 가져오는 스키마에 둡니다.

`lute trace`는 장면을 지나는 경로 하나를 재생합니다. `--choose`는 허브에서 고를 것을 순서대로 적습니다:

```
$ lute trace lamp.lute --choose lamp=ledger,ledger,stores,ledger,dusk
trace: lamp.lute  (seeds: 0 paths, 0 facts; 5 selections)
  ## The Lamp Room
    @narrator  The lamp room smells of cold brass.
  <hub lamp>   eligible: ledger, stores, dusk   -> ledger
    ::set  scene.ledgerReads = 1
  <match scene.ledgerReads>   -> arm 1 (is="1")
    @narrator  The last entry is three weeks old. "Oil low. Ship due."
    -- return (hub lamp) --
    @narrator  The lamp room again. The dark is closer.
  <hub lamp>   eligible: ledger, stores, dusk   -> ledger
    ::set  scene.ledgerReads = 2
  <match scene.ledgerReads>   -> arm 2 (is="2")
    @narrator  You read it again. The handwriting shakes toward the end.
    -- return (hub lamp) --
    @narrator  The lamp room again. The dark is closer.
  <hub lamp>   eligible: ledger, stores, dusk   -> stores
    @narrator  You find two more cans of oil.
    ::set  run.oil = 3
    -- return (hub lamp) --
    @narrator  The lamp room again. The dark is closer.
  <hub lamp>   eligible: ledger, dusk   -> ledger
    ::set  scene.ledgerReads = 3
  <match scene.ledgerReads>   -> otherwise
    @narrator  The words have stopped changing.
    -- return (hub lamp) --
    @narrator  The lamp room again. The dark is closer.
  <hub lamp>   eligible: ledger, dusk   -> dusk
  guard `run.oil >= 3`: taken
    @narrator  The lamp catches.
  guard `run.oil < 3`: skipped
trace complete: 10 decisions; choices 3/3 (lamp), arms 3/3 (scene.ledgerReads @19:5), guard `run.oil >= 3` @36:5: taken, guard `run.oil < 3` @37:5: skipped
```

`stores`는 한 번 고른 뒤 메뉴에서 빠집니다. 나머지 허브 규칙은 [Choices & hubs](/language/choices-and-hubs/)에
있습니다. 검사기가 모든 허브에 요구하는 규칙도 거기 있습니다: 허브는 끝날 수 있어야 합니다(가드 없는
`exit` 선택지, 또는 `once` 선택지만).

## 바뀌는 텍스트

Lute에는 인라인 대안이 없습니다. Ink의 `{a|b|c}`와 Yarn의 `<<once>> … <<else>>`는 `<match>`가 고르는 온전한
줄이 되고, `<match>`가 읽는 숫자는 직접 관리합니다.

- **시퀀스**(`{a|b|c}`, 또는 `{stopping: …}`): 숫자를 올리고 `1`, `2`, 그리고 마지막 것은 `<otherwise>`로
  맞춥니다. 위의 장부가 그렇게 합니다.
- **한 번만**(`{!a|b}`): 같은 방법에 `<otherwise>`를 비워 둡니다.
- **순환**(`{&a|b|c}`): 센 숫자를 줄 수로 나눈 나머지를 def로 만들어 맞춥니다:

```lute check
---
kind: scene
id: harbor
title: The Harbor
state:
  scene.looks: { type: int, default: 0 }
defs:
  weather: { type: int, cel: "scene.looks % 3" }
---

## The Harbor

<hub id="harbor">
  <choice id="look" label="Look at the sea">
    ::set{scene.looks += 1}
    <match on="@weather">
      <when is="1">
        @narrator: Fog on the water.
      </when>
      <when is="2">
        @narrator: Mist over the rocks.
      </when>
      <otherwise>
        @narrator: Rain, sideways.
      </otherwise>
    </match>
  </choice>
  <choice id="leave" label="Walk inland" exit>
    @narrator: You turn your back on the sea.
  </choice>
</hub>
```

- **섞기**(`{~a|b}`, Yarn의 `dice()`): Lute는 무작위로 고르지 않습니다. 그래야 한 경로의 trace와 play가
  언제나 같은 것을 출력합니다. 주사위는 엔진이 굴리게 하세요: 엔진이 쓰는 경로(스키마에서
  `owner: engine`)를 선언하고 그 경로에 `<match>`를 겁니다.

`scene.*` 카운터는 장면이 제시될 때마다 처음부터 다시 셉니다. 런 동안 유지하려면 `run.*`에, 영영
유지하려면 `user.*`에 세세요.

## 장면 사이를 옮겨 가기

뒤쪽 knot으로 가는 divert는 두 번째 장면이 되고, 엔진이 그 장면이 응답하는 계기를 발생시킵니다.
매니페스트의 `chapters:`는 장면 목록에 순서를 줍니다: 계기를 발생시킬 때마다 다음 장면이 제시됩니다.
Ink의 `{cond: -> a | -> b}`는 같은 계기에 응답하는 장면 두 개이고, 각자 `when:`을 답니다. 두 조건이 동시에
성립할 수 없으면 검사기는 둘을 같은 priority로 받아들입니다. [장면을 이야기로
잇기](/ko/getting-started/connect-scenes/)가 바로 이것을 만듭니다.

장면 안에서 `::next{to="…"}`는 아래쪽의 `::mark{id="…"}`(또는 줄의 `id=`)로 앞으로 뜁니다. 뒤로는 결코
뛰지 않습니다: 플레이어가 돌아오는 메뉴는 `<hub>`이고, 다시 재생되는 장면은 자기 계기에 다시 응답합니다.
제목은 점프 대상이 아니며, 선택지 id(Ink의 이름 붙은 선택지 `* (inside)`)도 아닙니다. 선택지 id는 고른
것의 이름이지 텍스트 안의 자리가 아닙니다.

`visited('<scene id>')`는 장면이 한 번이라도 제시되었는지 묻습니다. 횟수가 아니라 참/거짓이고, 세이브
전체에 걸칩니다: 새 런이 시작되어도 참으로 남습니다.

## 이야기 끝내기

Ink에는 끝이 두 가지 있고, Lute도 그렇습니다.

- **`-> DONE`**은 이 흐름을 끝냅니다. Lute에서는 `::end`, 또는 장면의 끝입니다: 제시가 끝나고, 다음 계기가
  다른 장면을 제시할 수 있습니다.
- **`-> END`**는 이야기를 끝냅니다. Lute에서는 스키마의 조건 `terminal:`입니다. 이 조건이 성립하면 새 런이
  시작될 때까지 엔진은 더는 계기를 발생시키지 않습니다(타이틀 화면처럼 `outsideRun: true`로 선언한 계기는
  예외입니다). 장면은 평범한 `::set`으로 조건을 성립시킵니다.

```yaml
# world.schema.yaml
state:
  run.fate: { type: { enum: [open, drowned] }, default: open }
terminal: "run.fate == 'drowned'"
```

```lute
<choice id="leap" label="Climb the rail toward the light" once>
  @narrator: The rail is wet. The light is very far away.
  ::set{run.fate = "drowned"}
  ::end
</choice>
```

`lute play`가 둘 다 보여 줍니다: `::end`는 장면을 닫고, 그 뒤로 게임은 끝난 상태입니다.

```
@narrator: The rail is wet. The light is very far away.
  set run.fate = "drowned"
::end        (this presentation ends)
  note: the game is over — `terminal: run.fate == 'drowned'` holds, so the engine raises no occasion from here (`occasion:` / `advance:` steps are refused; `newRun: true` starts a new run)
── end: terminal — `terminal: run.fate == 'drowned'` holds ──────────────
```

플레이 스크립트는 `expect: { end: terminal }`로 이것을 확인합니다([스토리 플레이](/ko/tooling/play/)).

## 주석과 태그

`//` 주석은 자기 줄에만(또는 디렉티브 뒤에) 쓰고, `/* … */`는 여러 줄에 걸칩니다. 줄의 텍스트 뒤에 오는
`//`와 `#`은 텍스트의 일부입니다: 플레이어에게 보이고, 검사기가 경고합니다(`W-TEXT-COMMENT-LIKE`).

Lute에는 자유 형식 태그가 없습니다. Ink나 Yarn 태그가 엔진에 알리던 것은 대신 검사되는 자리가 있습니다:

- 줄을 어떻게 전달할지: 줄 속성, `@keeper{emotion="tired"}: …`
  ([Dialogue & cast](/language/dialogue-and-cast/));
- 효과음, 음악, 카메라: 자기 줄에 쓰는 디렉티브, `::sfx{…}`, `::music{…}`
  ([Core directives](/language/directives/));
- 문서 자체의 태그(Yarn의 `tags:` 헤더): 프런트매터의 `extra: { tags: [...] }`.

## 검사기가 하는 말

`.lute` 파일 안의 Ink나 Yarn 문법은 Lute에서 쓸 형태를 알려 주는 오류나 경고가 됩니다. Ink 줄로 된 파일:

<!-- lute-diagnostics unverified="byte-exact lute check output; each E-UNCLASSIFIED hint is composed from per-shape parts, several of which contain a literal … that the matcher reads as an elision" -->
```
$ lute check ink.lute
ink.lute:10:1: error [E-UNCLASSIFIED] unrecognized line: `=== lamp_room ===` is an Ink knot; a Lute scene is its own `.lute` file (`kind: scene` and `id: lamp_room` in its frontmatter), and a section inside a scene is a `## lamp_room` heading
ink.lute:11:1: error [E-UNCLASSIFIED] unrecognized line: `= stitch` is an Ink stitch; a section inside a scene is a `## stitch` heading
ink.lute:12:1: error [E-UNCLASSIFIED] unrecognized line: `VAR` declares an Ink global; Lute declares state under `state:` in the frontmatter (`run.oil: { type: int, default: 1 }`) and writes it with `::set{…}`
ink.lute:13:1: error [E-UNCLASSIFIED] unrecognized line: `~ run.oil = run.oil + 2` is Ink logic; Lute writes state with `::set{run.oil = run.oil + 2}`, and the path is declared under `state:` in the frontmatter
ink.lute:14:1: error [E-UNCLASSIFIED] unrecognized line: a content line needs a speaker: narration is `@narrator: …`, dialogue `@<speaker>: …`
ink.lute:15:1: error [E-UNCLASSIFIED] unrecognized line: `* [Read the ledger]` is an Ink choice; Lute choices are `<choice id="…" label="…">` blocks inside a `<branch>` (asked once) or a `<hub>` (asked again until an `exit` choice); Ink's once-only `*` in a loop is a `<hub>` choice with the `once` flag
ink.lute:16:1: error [E-UNCLASSIFIED] unrecognized line: `+ [Wait for dark]` is an Ink choice; Lute choices are `<choice id="…" label="…">` blocks inside a `<branch>` (asked once) or a `<hub>` (asked again until an `exit` choice); Ink's sticky `+` is a plain `<hub>` choice
ink.lute:17:1: error [E-UNCLASSIFIED] unrecognized line: `- gather` is an Ink gather; Lute has no gathers: after a `<branch>` or `<hub>` closes, the lines below it run whichever choice was taken, so write the gathered text there as an ordinary line (`@narrator: …`)
ink.lute:18:1: error [E-UNCLASSIFIED] unrecognized line: `-> ledger` is an Ink divert; Lute has no diverts: `::next{to="ledger"}` jumps forward to a `::mark{id="ledger"}` later in this document, a `<hub>` repeats its choices until an `exit` choice, and another scene is reached through the occasion it answers (`on:` in its frontmatter)
ink.lute:19:1: error [E-UNCLASSIFIED] unrecognized line: `-> END` is an Ink divert; it ends the whole story, which in Lute is the schema's `terminal:` condition: a scene makes it hold with an ordinary `::set{…}` (`::end` is Ink's `-> DONE`: it ends only this scene)
failed: ink.lute (10 error(s), 0 warning(s))
```

Yarn도 마찬가지입니다:

<!-- lute-diagnostics unverified="byte-exact lute check output; each E-UNCLASSIFIED hint is composed from per-shape parts, several of which contain a literal … that the matcher reads as an elision" -->
```
$ lute check yarn.lute
yarn.lute:10:1: error [E-UNCLASSIFIED] unrecognized line: `<<declare $oil = 1>>` is a Yarn declaration; Lute declares state under `state:` in the frontmatter (`run.oil: { type: int, default: 1 }`) and writes it with `::set{…}`
yarn.lute:11:1: error [E-UNCLASSIFIED] unrecognized line: `<<set $oil to 3>>` is a Yarn command; Lute writes state with `::set{run.oil = 3}`, and the path is declared under `state:` in the frontmatter
yarn.lute:12:1: error [E-UNCLASSIFIED] unrecognized line: `<<if $oil > 2>>` is a Yarn conditional; Lute chooses between lines with `<match on="…">` and its `<when is="…">`/`<when test="…">` arms, or guards one line: `@narrator{when="run.oil > 2"}: …`
yarn.lute:13:1: error [E-UNCLASSIFIED] unrecognized line: `<<jump Lamp_Room>>` is a Yarn jump; Lute has no diverts: `::next{to="Lamp_Room"}` jumps forward to a `::mark{id="Lamp_Room"}` later in this document, a `<hub>` repeats its choices until an `exit` choice, and another scene is reached through the occasion it answers (`on:` in its frontmatter)
yarn.lute:14:1: error [E-UNCLASSIFIED] unrecognized line: `-> Read the ledger` is a Yarn option; Lute choices are `<choice id="…" label="…">` blocks inside a `<branch>` (asked once) or a `<hub>` (asked again until an `exit` choice)
failed: yarn.lute (5 error(s), 0 warning(s))
```

다른 언어의 마크업으로 쓴 텍스트는 문자 그대로 쓴 것일 수도 있으므로 오류가 아니라 경고입니다. 한 겹 중괄호,
줄 끝의 주석이나 태그, 대괄호를 쓴 선택지 label:

<!-- lute-diagnostics unverified="byte-exact lute check output; W-TEXT-SINGLE-BRACE prints a literal … (a shortened quote, Ink's {~…|…} notation) that the matcher reads as an elision" -->
```
$ lute check text.lute
text.lute:10:21: warning [W-TEXT-SINGLE-BRACE] `{run.oil}` is literal line text: single braces are not read, so the player sees them — interpolation is `{{run.oil}}`
text.lute:11:12: warning [W-TEXT-SINGLE-BRACE] `{run.oil > 0: A can of oil sits by the…}` is literal line text: single braces are not read, so the player sees them — Lute has no inline conditional text; guard the whole line instead (`@narrator{when="run.oil > 0"}: …`), or choose between lines with `<match>`
text.lute:12:12: warning [W-TEXT-SINGLE-BRACE] `{~Fog|Mist|Rain}` is literal line text: single braces are not read, so the player sees them — Lute has no inline alternatives (Ink's `{~…|…}` shuffle, `{&…|…}` cycle, `{!…|…}` once-only, `{…|…}` sequence); choose between whole lines with `<match>` or guarded lines (`when="…"`)
text.lute:13:17: warning [W-TEXT-SINGLE-BRACE] `{$oil}` is literal line text: single braces are not read, so the player sees them — Yarn's `{$oil}` is an interpolation of a declared state path in Lute, `{{run.oil}}`
text.lute:14:48: warning [W-TEXT-COMMENT-LIKE] `# mood:cold` is part of the line text, so the player sees it — Lute has no line tags; keep a note as a `// …` comment on a line of its own
text.lute:15:30: warning [W-TEXT-COMMENT-LIKE] `// TODO darker` is part of the line text, so the player sees it — a comment is `// …` on a line of its own (or after a directive), never after text
text.lute:17:3: warning [W-TEXT-BRACKET-LABEL] choice label `[Go inside]` shows its brackets on the button — a label is shown exactly as written (Lute has no Ink-style bracket suppression); write `label="Go inside"`
ok: text.lute (7 warning(s))
```

디렉티브처럼 쓴 divert, 뒤로 가는 점프, 점프 대상으로 쓴 제목, 조건 안의 Ink·Yarn 습관:

<!-- lute-diagnostics unverified="byte-exact lute check output; the did-you-mean and E-CEL-TYPE reasons are composed outside the file that declares their code, and E-NEXT-UNDEFINED matches two literals" -->
```
$ lute check flow.lute
flow.lute:12:1: error [E-UNKNOWN-DIRECTIVE] unknown directive `::goto` — did you mean `::next{to="…"}`? It jumps forward to a `::mark{id="…"}`
flow.lute:13:12: error [E-NEXT-BACKWARD] `::next` targets mark `top`, which is not forward of this `::next` in document order — `::next` only jumps forward; to offer choices again, use a `<hub>` (it asks until an `exit` choice is taken)
flow.lute:18:12: error [E-NEXT-UNDEFINED] `::next` targets `Gallery`, which no `::mark` or line `id=` in this document declares — `Gallery` is the `## Gallery` heading, not a mark; a `::next` target is a `::mark{id="…"}` (or a line's `id=`) later in this document, so put `::mark{id="Gallery"}` under that heading
failed: flow.lute (3 error(s), 0 warning(s))
$ lute check cond.lute
cond.lute:5:7: error [E-CEL-PROFILE] `once` is not a condition: how often a beat plays is its own key, `once` — `once: run` (once per run, the default), `once: user` (once ever) or `once: false` (every time); on a `<beat>` or `<entry>` it is `once="run"` — drop this `when`
cond.lute:12:17: error [E-CEL-TYPE] `visited('gallery') > 2`: `>` compares numbers, and `visited('gallery')` is a bool — `visited('gallery')` is a bool, whether the scene was ever presented, not how often; count visits in an `int` path you `::set` (for example `::set{run.visits += 1}`)
cond.lute:13:17: error [E-CEL-PROFILE] `$oil`: a state path takes no `$` (`$` alone is the `<match>` subject) — write the path with its tier — did you mean `run.oil`?
cond.lute:14:17: error [E-CEL-TYPE] `run.oil == true` compares an int with a bool, so it is never true
failed: cond.lute (4 error(s), 0 warning(s))
```

Ink의 첫 방문 검사를 허브의 기록으로 옮긴 경우도 잡힙니다: 선택지 자신의 줄 안에서는 기록이 이미 참이므로,
거짓을 요구하는 갈래는 결코 실행되지 않습니다.

<!-- lute-diagnostics -->
```
first.lute:11:7: error [E-ARM-DEAD] arm can never fire: its pattern `false` never matches here — `scene.visited.lamp.ledger` is `true` in its option's own arm — the visit record is set when the choice is picked, before its arm runs
```

모든 코드는 [진단 코드](/ko/reference/diagnostics/)에 있고, `lute --explain <CODE>`가 그 항목을 출력합니다.
