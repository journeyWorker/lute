---
title: 치트시트
description: "Lute 0.21.0으로 글을 쓰는 동안 열어 두는 한 페이지: 모든 구문을 검사된 최소 스니펫으로 보여 줍니다(프로젝트 구성, 프론트매터, 대사, 선택지, match, 상태, CEL, 비트, 퀘스트, 로어, 컴포넌트, 타임라인). CLI 요약, 작가가 가장 자주 만나는 진단 코드, 주의할 점도 담았습니다."
---

모든 구문을 한 페이지에 복사해 쓸 수 있는 스니펫으로 모았습니다. 아래의 `lute` 블록은 모두 CI에서 실제
툴체인으로 컴파일 검사를 거치며, 각 절 끝의 링크는 전체 설명 페이지로 이어집니다.

## 프로젝트 구성

```
my-game/
├── lute.project.yaml            profiles, plugins, identity, defaults
├── world.schema.yaml            run/user/app state, enums, defs, facts, rules
├── plugins/game.occasions/      optional: plugin.yaml + occasions/*.yaml
├── scenes/*.lute                kind: scene
├── quests/*.lute                kind: quest
├── lore/*.lute                  kind: lore
├── components/*.component.lute  component: <name>
├── tests/*.test.yaml            lute test
└── plays/*.play.yaml            lute play
```

`lute.project.yaml`:

```yaml
pluginsDir: plugins/
defaultProfile: game
profiles:
  game:
    plugins: { game.occasions: true }   # true = active with defaults
identity:                               # lineId below is the default
  lineId: "{prefix}.{speaker}_{code}"
  voiceKey: "{prefix}.{speaker}-{code}" # default {speaker}-{code} collides across scenes (E-DUP-VOICEKEY)
defaults:                               # frontmatter every document inherits
  luteVersion: "0.21.0"
  uses: [world.schema.yaml]             # resolved against THIS file's directory
```

`defaults:`에는 `kind`, `character`, `season`, `episode`, `pov`, `luteVersion`, `contentLang`,
`uses`, `extends`, `components`, `extra`만 쓸 수 있습니다(그 밖의 키는 `E-DEFAULTS-KEY`). 문서가 어떤 키를
직접 쓰면 그 키의 기본값은 병합 없이 통째로 대체됩니다(`uses: []`는 "가져오기 없음"). 문서의 kind에서
허용되지 않는 기본값은 그 문서에는 적용되지 않습니다.

`world.schema.yaml`은 `---` 구분선이 없는 일반 YAML입니다. 씬은 `uses:`로 이 파일을 가져옵니다:

```yaml
state:                                   # scalar only: number | bool | string | enum
  run.pressure: { type: number, default: 0 }
  run.mood:     { type: { enum: [calm, tense] }, default: calm }
  run.rival:    { type: { enum: [kai, lee] } }       # no default: maybe-unset until set
  user.runs:    { type: number, default: 0 }
  app.rating:   { type: { enum: [teen, adult] }, default: teen }
enums:                                   # content vocabulary: you declare every member
  emotion: [neutral, happy, worried]
  action:  { members: [fade-in-up, fade-out-down], exits: [fade-out-down] }
  anchor:  { members: [left, center, right], default: center }
entities:
  crew:  { members: [vesna, toma] }
  topic: { members: [manifest, heading] }
relations:
  awake:    { args: [crew], tier: run }
  knows:    { args: [crew, topic], tier: run }
  can_halt: { args: [crew], derive: true }
facts:
  - "awake(vesna)"
rules:
  - "can_halt(C) :- awake(C), knows(C, manifest)"
defs:
  calm: "run.pressure < 2"                    # shorthand: the body alone, type inferred (bool)
  veteran: "user.runs >= 10"
  vesnaKnows: "holds(knows(vesna, manifest))"
  zoom: "run.pressure > 2 ? 1.3 : 1.1"        # inferred number
  atLeast: { type: bool, params: { n: number }, cel: "user.runs >= n" }   # params need type:
```

def 타입 추론: 비교, `&&` `||` `!`, `holds`, `has`, `isSet`은 `bool`이고, `count`, 산술, 숫자 리터럴은
`number`이며, 경로를 그대로 읽으면 그 경로의 타입입니다. 검사기가 타입을 알 수 없는 본문(예: `"@other"`)은
긴 형태 `{ type: …, cel: … }`로 써야 하며, 그렇지 않으면 `E-DEF-DECL`입니다.

어휘 슬롯은 `emotion`, `action`, `anchor`, `mood`, `volume`, `musicAction`, `vfxType` 일곱 가지입니다.
아무도 선언하지 않은 슬롯을 쓰면 `E-DOMAIN-UNKNOWN`입니다. `action`에는 `exits:`를, `anchor`에는
`default:`를 반드시 적어야 합니다.

→ [상태 스키마](/state/schemas/) · [가져오기](/language/imports/) · [콘텐츠 어휘](/language/vocabulary/) · [팩트와 Datalog](/state/facts-and-datalog/)

## kind별 프론트매터 키

| kind | 필수 | 그 kind에서만 쓰는 키 |
|---|---|---|
| `kind: scene` | `id:`, 또는 레거시 `character` + `season` + `episode` | `id`, `character`, `season`, `episode`, `episodeId`, `pov`, `after`, 비트 키 `on` / `target` / `when` / `priority` / `once` |
| `kind: quest` | 본문에 `<quest>` 하나 이상 | `id` (선택, 묶음 이름) |
| `kind: lore` | 본문에 `<entry>` 하나 이상 | `id`, `series` |
| 컴포넌트 (`kind:` 없음) | `component: <name>` | `component`, `params` |

모든 루트 kind는 `title`, `luteVersion`, `contentLang`, `profile`, `plugins`, `uses`, `extends`,
`components`, `state`, `defs`, `enums`, `entities`, `relations`, `facts`, `rules`, `codesLocked`,
`mode`, `extra`(자유로운 설명 데이터)도 받습니다. 이 목록 밖의 키는 `E-META-UNKNOWN-KEY`입니다. 퀘스트의
선행 조건은 `<quest>`의 `after=` 속성이며, 프론트매터 키가 아닙니다.

→ [프론트매터와 프로필](/language/frontmatter-and-profiles/)

## 대사, 캐스트, 연출

```lute check
---
kind: scene
id: diner.night
pov: fixer
enums:
  emotion: [neutral, happy]
  action: { members: [fade-in-up, fade-out-down], exits: [fade-out-down] }
  anchor: { members: [left, center, right], default: center }
  mood: [peaceful]
  volume: [down, normal]
  musicAction: [start, fade-out]
  vfxType: [whiteOut]
state:
  run.affection: { type: number, default: 0 }
---

## Counter

::bg{location="diner" time="night"}
::music{action="start" mood="peaceful" volume="down"}
::auto{character="mira" anchor="center" action="fade-in-up"}
::camera{focus="mira" zoom="1.2" duration="0.5" wait="true"}
@narrator: The diner hums.
@mira{code="0010" emotion="happy"}: You're back, {{userName}}! Warmth: {{run.affection}}.
@fixer: I am.
@fixer{mono}: She remembered.
@mira{os}: Hold on!
@mira{as="???"}: ...who's there?
::sfx{sound="door bell"}
::vfx{type="whiteOut"}
::auto{character="mira" action="fade-out-down"}
::music{action="fade-out"}
::end{reason="closing"}
```

| 요소 | 규칙 |
|---|---|
| `@speaker{attrs}: text` | `@narrator`는 내레이션입니다. `pov:`와 같은 화자는 플레이어입니다. `: ` 뒤의 텍스트는 줄 끝까지 그대로입니다. |
| 줄 속성 | `code`, `emotion`, `variant`, `action`, `dialogMotion`, `as`(이름표 덮어쓰기), `when`(가드), `id`(점프 라벨) |
| 전달 플래그 | `{mono}` 속마음, `{os}` 화면 밖, `{vo}` 보이스오버. 한 줄에 하나까지이며 `@narrator`에는 쓸 수 없습니다. |
| `{{…}}` | `{{userName}}` 또는 선언된 상태 경로. 값이 없을 수 있는 경로를 읽으면 `E-MAYBE-UNSET`입니다. |
| 샷 | 모든 콘텐츠는 `## 제목` 아래에 둡니다. `# 제목`만으로는 샷이 열리지 않습니다. |
| 디렉티브 | `::bg` `::music` `::sfx` `::auto`(등장, 포즈, 퇴장) `::camera` `::cut` `::vfx` `::video` `::end`. 타이밍 키: `duration`, `delay`, `wait="true"`(대기). |
| 주석 | `/* … */` |

→ [대사와 캐스트](/language/dialogue-and-cast/) · [코어 디렉티브](/language/directives/)

## 선택지, 허브, 점프, 엔딩

```lute check
---
kind: scene
id: cafe.counter
state:
  scene.warmth: { type: number, default: 0 }
  run.metMira:  { type: bool, default: false }
  run.tip:      { type: number, default: 0 }
defs:
  warm: "scene.warmth >= 2"
---

## Counter

<branch id="greet" prompt="Mira looks up.">
  <choice id="wave" label="Wave" into="run.metMira">
    @mira: Hi!
    ::set{scene.warmth += 2}
  </choice>
  <choice id="tip" label="Leave a tip" into="run.tip" value="5">
    @mira: Thanks!
  </choice>
  <choice id="flirt" label="Flirt" when="@warm">
    @mira: Oh, stop.
  </choice>
</branch>

<hub id="chat">
  <choice id="coffee" label="Ask about the coffee" once>
    @mira: House blend.
  </choice>
  <choice id="cup" label="Ask about the missing cup">
    ::accept{quest="lostCup"}
    @mira: Find it and your next one is free.
  </choice>
  <choice id="leave" label="Leave" exit>
    @mira: Bye.
  </choice>
</hub>

::next{to="outro" when="run.tip > 0"}
@mira: No tip, huh.
::mark{id="outro"}
@narrator: The door closes behind you.
::end{reason="leftCafe"}
```

| 구문 | 규칙 |
|---|---|
| `<branch id>` | 메뉴입니다. 고른 값은 `scene.choices.<id>`에 기록되고, 씬이 끝나면 지워집니다. 가드 없는 선택지가 하나 이상 있어야 합니다(`E-BRANCH-ALL-GUARDED`). `prompt=`와 `timeout="N"`은 선택입니다. |
| `<choice id label>` | `when=` 가드. `into="run.x"`는 `true`를 쓰고, number나 enum 경로에는 `value=`를 함께 써서 이후 씬이 읽을 수 있게 합니다. |
| `<hub id>` | `exit`를 고를 때까지 자격 있는 선택지를 다시 보여 줍니다. `once`는 한 번 고른 선택지를 없앱니다. 가드 없는 `exit`가 있거나 모든 선택지가 `once`여야 합니다(`E-HUB-NO-EXIT`). 고를 때마다 `scene.visited.<hub>.<choice>`가 설정됩니다. |
| `::next{to when}` | `::mark{id}`나 줄의 `id=`로 가는 앞쪽 전용 점프입니다. 뒤로 가는 점프는 `E-NEXT-BACKWARD`입니다. `when`이 없으면 그 뒤의 콘텐츠는 죽은 코드입니다(`W-CODE-AFTER-NEXT`). |
| `::end{reason}` | 진행을 멈춥니다. 같은 본문에서 그 뒤의 콘텐츠는 `W-CODE-AFTER-END`입니다. |
| `::accept{quest}` | 수락형 퀘스트(`start`가 없는 퀘스트)를 받아들입니다. [퀘스트](#퀘스트)를 보세요. |

→ [선택지와 허브](/language/choices-and-hubs/) · [branch, match, when](/language/branch-match-when/)

## match와 when

```lute check
---
kind: scene
id: cafe.moods
state:
  run.tips:  { type: number, default: 0 }
  run.mood:  { type: { enum: [calm, tense, joyful] }, default: calm }
  run.rival: { type: { enum: [kai, lee] } }
---

## Moods

<match on="run.mood">
  <when is="calm">
    @mira: Quiet day.
  </when>
  <when is="tense|joyful">
    @mira: Busy day.
  </when>
</match>

<match on="run.tips">
  <when is="10..">
    @mira: My best customer.
  </when>
  <when is="1..9">
    @mira: Thanks, as always.
  </when>
  <otherwise>
    @mira: Hmm.
  </otherwise>
</match>

<match on="run.rival">
  <when is="kai">
    @mira: Kai was here earlier.
  </when>
  <when test="$ == 'lee' && run.tips > 3">
    @mira: Lee asked about you.
  </when>
  <when is="unset">
    @mira: Nobody asked about you.
  </when>
  <otherwise>
    @mira: Lee was here.
  </otherwise>
</match>

@mira{when="run.mood != 'calm'"}: Take a breath.
```

- `is=`에는 enum 멤버, `true`/`false`, 숫자, 양끝을 포함하는 범위(`3..`, `..0`, `1..9`), `unset`,
  `|` 선택이 들어갑니다. `test=`는 주제 값이 `$`에 묶인 CEL 가드입니다. 둘 다 쓰면 패턴 AND 가드입니다.
- 갈래는 위에서 아래로 평가되며 처음 맞는 것이 이깁니다. match는 모든 경우를 다뤄야 합니다: 모든 멤버를
  다룬 enum이나 bool에는 `<otherwise>`가 필요 없습니다. 숫자는 실수이므로 `1..9`와 `10..` 사이에는 9.5라는
  틈이 남습니다. 값이 없을 수 있는 주제에는 `is="unset"`이나 `<otherwise>`가 필요합니다.
- `test="$ == 'x'"`는 `W-WHEN-TEST-LITERAL`이며, `lute fix`가 `is="x"`로 바꿔 줍니다.
- `@who{when="G"}: …`는 갈래가 하나인 match의 축약입니다. 팩트 질의(`holds(…)`)에는 이 줄 형태만 쓸 수
  있습니다. `<match on="holds(…)">`는 `E-MATCH-RELATION-SUBJECT`이기 때문입니다.
- 모든 태그는 한 물리적 줄에 혼자 놓입니다. 한 줄짜리 `<when …>text</when>`는 `E-TAG-INLINE-BODY`이고,
  여러 줄로 나눈 태그는 `E-TAG-NOT-ONE-LINE`입니다.

→ [branch, match, when](/language/branch-match-when/)

## 상태 쓰기와 팩트

```lute check
---
kind: scene
id: ship.archive
state:
  run.trust: { type: number, default: 0 }
  run.seen:  { type: bool }
entities:
  crew:  { members: [vesna, toma] }
  topic: { members: [manifest, heading] }
relations:
  knows: { args: [crew, topic], tier: run }
---

## Archive

::set{run.seen = true}     /* the first write of a no-default path must be `=` */
::set{run.trust += 1}      /* also -= and *= */
@vesna{when="run.seen"}: You found the archive.
::assert{knows(vesna, manifest)}
@vesna{when="holds(knows(vesna, manifest))"}: I read the manifest.
@vesna{when="count(knows(_, manifest)) >= 2"}: So we both know.
::retract{knows(vesna, _)}
```

`_`는 질의와 `::retract`에서 와일드카드입니다. `app.*`는 읽기 전용입니다(`E-APP-READONLY`).
`derive: true`나 `reserved:` 관계는 콘텐츠에서 assert할 수 없습니다. 관계의 `key: [0]`은 첫 번째 인자를
함수적으로 만들어, 새로 assert하면 이전 팩트를 대체합니다.

→ [상태 모델](/state/state-model/) · [팩트와 Datalog](/state/facts-and-datalog/)

## CEL 요약

| 네임스페이스 | 초기화 시점 | 콘텐츠가 쓸 수 있는가 |
|---|---|---|
| `scene.*` | 씬이 끝날 때 | 예 |
| `run.*` | 새 런에서 | 예 |
| `user.*` | 프로필 초기화 시 | 예 |
| `app.*` | 앱 삭제 시 | 아니요 |
| `quest.<id>.state` (`unset` `active` `complete` `failed`), `quest.<id>.activatedAt`, `quest.<id>.objectives.<o>.done` | 엔진 | 아니요 |
| `entry.<id>.read` | 엔진, run 등급 | 아니요 |
| `scene.choices.<branch>`, `scene.visited.<hub>.<choice>` | 엔진 | 아니요 |
| `visited('<scene id>')` | 초기화되지 않음: 세이브 전체 | 아니요 |

| 연산자 | 함수와 참조 |
|---|---|
| `== != < <= > >=` · `&& \|\| !` · `+ - * /` · `c ? a : b` · `x in ['a', 'b']` · 문자열·숫자 리터럴 | `has(p)` / `isSet(p)`(값이 있는가) · `holds(rel(a, _))` · `count(rel(_)) >= n` · `validAt(rel(a), quest.q.activatedAt)` · `visited('scene.id')` · `@def` / `@def(args)` · `$`(`<match>` 안에서만) |

쓸 수 없는 것: `%`, `size`, `matches`, `map`/`filter`/`exists`/`all`(`E-CEL-PROFILE`). 값이 없음은
문자열 `'unset'`이 아닙니다(`E-UNSET-LITERAL`). `!isSet(p)`나 `is="unset"`으로 확인하세요. 경로 세그먼트,
def 이름, 파라미터 이름에는 `-`를 쓸 수 없습니다.

CEL이 들어가는 곳: `<match on>`, `<when test>`, 줄이나 선택지의 `when=`, `::set`의 우변, `::next when`,
비트 `when:`, 엔트리 `when=`, 퀘스트 `start` / `fail`, 목표 `done` / `when`, `<on when>`,
`<reward when>`. 디렉티브 속성에서 def 참조는 따옴표 없이 씁니다: `zoom="@zoom"`이 아니라
`::camera{zoom=@zoom}`입니다.

→ [CEL 표현식](/state/cel/) · [정의와 파라미터](/language/params/)

## 비트: 계기에 응답하는 씬과 엔트리

비트는 계기(occasion)가 발생했을 때 엔진이 고를 수 있는 씬이나 로어 엔트리입니다.

```lute check
---
kind: scene
id: vesna.gift
on: talk
target: npc.vesna
when: 'user.runs >= 10 && !run.giftRefused'
priority: 50
once: user
after: 'visited("cafe.counter")'
state:
  user.runs:       { type: number, default: 0 }
  run.giftRefused: { type: bool, default: false }
---

## Gift

@vesna: You have been at this a while. Take this.
```

```lute check
---
kind: lore
id: vesna.barks
state:
  user.runs: { type: number, default: 0 }
---

<entry id="vesnaBark" on="talk" target="npc.vesna" category="bark">
  @vesna: Keep your head down.
</entry>

<entry id="vesnaBack" on="talk" target="npc.vesna" category="bark" priority="10" when="user.runs >= 3">
  @vesna: Back again?
</entry>

<entry id="vesnaFirst" on="talk" target="npc.vesna" category="bark" priority="20" when="!entry.vesnaFirst.read">
  @vesna: So you are the new one.
</entry>
```

| 키 | 의미 |
|---|---|
| `on` | 응답하는 계기. 이 키가 씬을 비트로 만듭니다. `on` 없이 다른 비트 키를 쓰면 `E-BEAT-ATTR`입니다. |
| `target` | 선택, 점으로 구분한 id(`npc.vesna`). 계기가 그 대상에 대해 발생했을 때만 후보가 됩니다. |
| `when` | `run` / `user` / `app`, `quest.*`, `entry.*.read`, 팩트, `visited()`에 대한 CEL. `scene.*`는 읽을 수 없습니다. |
| `priority` | 정수, 기본값 `0`. 높은 쪽이 이깁니다. |
| `once` | `run`(기본값), `user`(평생 한 번), `false`(반복 가능). 씬 전용이며 엔트리는 반복됩니다. |

선택: 후보는 `on`이 일치하고 `target`이 없거나 발생한 대상과 같은 비트입니다. 후보는 `after:`와 `when`이
성립하고 `once`가 소진되지 않았을 때 자격이 있습니다. 자격 있는 비트는 priority, 문서 경로, 선언 순서로
정렬됩니다. `select: first`는 맨 위의 비트를 제시하고, `select: all`은 플레이어가 고르게 합니다. 계기는
플러그인이 선언합니다:

```yaml
# plugins/game.occasions/plugin.yaml
id: game.occasions
version: 0.1.0
kind: capability
depends: [ { id: lute.core, range: "^0.0.1" } ]
exports:
  occasions: occasions/
```

```yaml
# plugins/game.occasions/occasions/game.yaml
occasions:
  hubVisit: { select: first }
  talk:     { select: first, target: true }
  runEnd:   { select: first }
  inbox:    { select: all, description: Letters waiting at the fountain }
```

계기를 선언하는 플러그인이 없으면 어떤 식별자든 받아들여집니다. 플러그인이 계기를 선언한 뒤에는 모르는
계기가 `E-OCCASION-UNKNOWN`이 되고, `target: true`가 없는 계기에 `target`을 쓰면 `E-BEAT-ATTR`입니다.

→ [비트](/language/beats/) · [스토리 플레이](/tooling/play/)

## 퀘스트

```lute check
---
kind: quest
id: cafe.quests
state:
  run.tips:  { type: number, default: 0 }
  run.fired: { type: bool, default: false }
  run.found: { type: bool, default: false }
  user.xp:   { type: number, default: 0 }
---

<quest id="regular" title="Become a regular" start="true" fail="run.fired" after="visited('cafe.counter')">
  <reward kind="XP" amount="100"/>
  <objective id="tip" title="Tip three times" done="run.tips >= 3">
    @narrator: Mira starts your order when you walk in.
  </objective>
  <objective id="calm" title="End the night calm" on="runEnd" done="run.tips > 0"/>
  <objective id="chat" title="Chat with Mira" done="visited('cafe.counter')" optional/>
  <objective id="help" title="Help out back" quest="sideJob"/>
  <on event="questComplete">
    ::set{user.xp += 50}
    @narrator: You are a regular now.
  </on>
  <on event="questFailed">
    @narrator: You are no longer welcome.
  </on>
</quest>

<quest id="sideJob" title="Help out back">
  <reward kind="GOLD" amount="10..20"/>
  <objective id="dishes" title="Do the dishes" done="run.tips >= 1"/>
</quest>

<quest id="lostCup" title="Find the lost cup">
  <objective id="find" title="Find it" done="run.found"/>
</quest>
```

| 요소 | 규칙 |
|---|---|
| `start=` | 성립하면 퀘스트를 활성화합니다(`unset` → `active`). `start`가 없으면 수락형입니다: 씬이 `::accept{quest="…"}`를 실행하거나 목(mock)이 수락할 때까지 `unset`으로 남습니다. |
| `fail=` | `active` → `failed`. 완료 조건과 동시에 성립하면 실패가 이깁니다. |
| `after=` | 씬 그래프를 위한 구조적 선행 조건: `&&` / `\|\|`로 묶은 `visited` / `completed` / `active`. 활성화를 막지 않으며, 활성화는 `start`가 정합니다. 씬은 프론트매터에 `after:`로 씁니다. |
| `<objective done>` | `done`은 필수입니다(`E-OBJECTIVE-MISSING-DONE`). `optional`이 아닌 목표가 모두 완료되면 퀘스트가 완료됩니다. 완료는 되돌려지지 않으며 본문은 한 번만 재생됩니다. |
| `on="runEnd"` | 퀘스트가 활성인 동안 그 계기가 발생했을 때만 `done`을 판정합니다. |
| 목표의 `when=` | 표시 여부만 정합니다. 완료에는 영향을 주지 않습니다. |
| `quest="child"` | 하위 퀘스트: 자식이 완료되면 완료되고, 필수 자식이 실패하면 부모도 실패합니다. `done=`과 함께 쓸 수 없습니다(`E-OBJECTIVE-QUEST-DONE`). `start`가 없는 자식은 부모와 함께 활성화됩니다. |
| `<reward kind amount target when on/>` | 엔진에 넘기는 데이터입니다. `amount`는 정수나 범위 `N..M`입니다. `on="failed"`는 실패 시에 지급합니다. |
| `<on event>` | `questActive`, `questComplete`, `questFailed`, 또는 플러그인의 월드 이벤트. `when=`으로 가드할 수 있습니다. |

퀘스트 문서에는 `#`/`##` 제목, `<hub>`, `<timeline>`이 없습니다. 다른 문서는
`<match on="quest.regular.state">`나 `when="quest.regular.state == 'complete'"`로 퀘스트를 읽습니다.

→ [퀘스트와 씬](/language/quests-and-scenes/)

## 로어 엔트리

```lute check
---
kind: lore
id: ship.records
series: captainsLog
entities:
  crew:  { members: [vesna] }
  topic: { members: [heading] }
relations:
  knows: { args: [crew, topic], tier: run }
state:
  run.fire: { type: bool, default: false }
---

<entry id="log1" target="item.captains_log" category="note" title="Day 1">
  @captain: We changed heading at midnight.
  ::assert{knows(vesna, heading)}
</entry>

<entry id="log2" target="item.captains_log" category="note" title="Day 2" when="entry.log1.read">
  <match on="run.fire">
    <when is="true">
      @narrator: The page is scorched.
    </when>
    <otherwise>
      @captain: Nobody noticed.
    </otherwise>
  </match>
</entry>
```

- `<entry>` 속성: `id`(필수, 프로젝트 안에서 유일), `target`, `category`, `title`, `when`, `on`,
  `priority`. 여러 파일에 걸친 시리즈에는 `series`와 `order`를 씁니다. 문서 수준의 `series:`는 파일 안의
  위치로 엔트리 순서를 정하며, 그런 문서에서 엔트리별 `series=`/`order=`는 `E-ENTRY-ATTR`입니다.
- 엔트리 본문에는 콘텐츠 줄, `<match>`, `::set`, `::assert`, `::retract`만 둘 수 있습니다. `<branch>`,
  디렉티브, 제목을 비롯한 그 밖의 것은 `E-GRAMMAR-NOT-ADMITTED`입니다.
- 효과는 첫 읽기에만 적용됩니다. 그 뒤로 `entry.<id>.read`는 `true`이며 어느 문서에서나 읽을 수 있습니다.

→ [로어 엔트리](/language/lore-entries/)

## 컴포넌트, extends, 파라미터

```lute check
---
component: greet
params:
  who: string
  tier: { enum: [cold, warm] }
---

## Greet

<match on="@tier">
  <when is="warm">
    @narrator: A warm welcome.
  </when>
  <when is="cold">
    @narrator: A curt nod.
  </when>
</match>
```

컴포넌트 파일 이름은 `name.component.lute`입니다. 본문에는 대사, 연출, `@param` 참조, 파라미터에 대한
`<match>`를 둘 수 있으며 상태를 읽거나 쓰지 않습니다. 가져오는 씬은 `components:`에 파일을 적고
`::use`로 펼칩니다:

```lute check="docs/examples/components/scene.lute"
---
kind: scene
character: demo
season: 1
episode: 2
uses: ../base.schema.yaml
components: [greet.component.lute]
---

## Greeting by Component

::use{component="greet" who="marina"}
@narrator: And the scene carries on.
```

스키마는 `extends: base.schema.yaml`로 다른 스키마를 다듬을 수 있습니다. 베이스가 아래층이므로 이름을 다시
선언하면 덮어씁니다. 상태 경로의 `type`을 바꾸면 `E-EXTENDS-STATE-TYPE`입니다. `uses:`는 동등한
스키마들을 합치며, 같은 이름이 두 번 선언되면 오류입니다.

→ [컴포넌트와 extends](/language/components-and-extends/) · [정의와 파라미터](/language/params/)

## 타임라인

```lute check
---
kind: scene
id: storm.beat
---

## Storm

<timeline duration="1.2">
  <track subject="camera">
    ::camera{focus="mira" zoom="1.2" duration="0.6"}
    ::camera{shake="0.4" duration="0.3" at="0.7"}
  </track>
  <track channel="sfx">
    ::sfx{sound="thunder" at="0.5"}
  </track>
</timeline>

<timeline duration="1.0">
</timeline>
@narrator: After the pause.
```

트랙에는 연출 디렉티브와 `::set`만 들어갑니다. 트랙 키(`subject=`, `channel=`, 또는 `subject=` +
`property=`)는 유일해야 합니다. `at=`은 타임라인 자체 시계의 절대 시각입니다. 빈 타임라인은 시간을 정한
멈춤입니다.

→ [타임라인과 속성 트랙](/language/timeline-and-property-tracks/)

## CLI

| 명령 | 용도 |
|---|---|
| `lute check <file> [--project <dir>]` | 문서 하나를 검사합니다. `defaults:`, 플러그인, 계기에 기대는 문서라면 반드시 `--project`를 붙이세요. |
| `lute check-project <dir>` | 모든 문서와 함께 연결성, 퀘스트 id, `::accept` 대상, 계기, 팩트 가드를 검사합니다. |
| `lute fix <file>` | 기계적 이전을 제자리에서 적용합니다: 옛 `:line` 표기, `as=` → `into=`, `test="$ == …"` → `is=`. |
| `lute tag <file>` | 모든 줄에 안정적인 `code`를 채웁니다. |
| `lute compile <file> -o out.json` · `--all --project <dir> -o <outdir>` | 산출물을 만듭니다. `--all`은 `beats`를 포함한 `project.index.json`도 씁니다. |
| `lute trace <file> [--mock m.yaml] [--state P=V] [--fact "r(a)"] [--choose id=c[,c]] [--event e] [--accept q] [--occasion o] [--entry id]` | 소스를 목에 맞춰 미리 봅니다. 종료 코드 `3`은 판정할 수 없는 가드를 만났다는 뜻입니다. |
| `lute run <artifact> [--mock m.yaml] [--occasion o] [--entry id]` | 컴파일된 산출물을 엔진처럼 실행합니다. |
| `lute play <dir> --script p.play.yaml [--json]` | 프로젝트 전체에 계기를 발생시키며 퀘스트를 진행합니다. |
| `lute test [<dir>] --project <dir> [--coverage]` | 모든 `*.test.yaml`을 실행합니다. |
| `lute scenario <dir> [reach <node> \| envelope <node>] [--format text\|json\|dot]` | `after:` 그래프, 도달 가능성, 보장되는 상태와 팩트. 노드는 씬 id나 `quest:<id>`입니다. |
| `lute lore <dir>` | 대상별·시리즈별 엔트리와 그 엔트리가 드러내는 팩트. |
| `lute context <file> [--project <dir>]` | 여기서 쓸 수 있는 모든 것: 디렉티브, 어휘, 상태, 계기. |
| `lute new scene\|quest\|lore\|schema <name> [--dir <dir>]` · `lute init <dir>` | 문서나 프로젝트의 뼈대를 만듭니다. |

trace 목(`--mock`). 테스트의 목 키도 같은 형식입니다:

```yaml
# mocks/counter.yaml: lute trace scenes/counter.lute --project . --mock mocks/counter.yaml
file: ../scenes/counter.lute               # required under mocks/, which check-project validates
state:  { run.tip: 5 }                     # path: literal
facts:  ["awake(vesna)"]                   # trace does NOT load the schema's facts: seeds
choose: { greet: tip, chat: [coffee, leave] }   # branch: choice; hub: its visit order
events: [combatEnd]                        # world events, for <on event>
# quest walks also take:
#   accepts:   [lostCup]                   # accept-driven quests to take up
#   visited:   [cafe.counter]              # scenes already played; unlisted = not visited
#   occasions: [runEnd]                    # raised after the walk settles
```

`tests/regular.test.yaml` (`file:`은 테스트 파일 기준 상대 경로):

```yaml
file: ../quests/cafe.lute
state: { run.tips: 3 }
visited: [cafe.counter]
occasions: [runEnd]
expect:
  quests: { regular: complete, lostCup: unset }   # unset | active | complete | failed
  state: { user.xp: 50 }
  transcriptContains: ["You are a regular now."]
  exit: complete                                  # complete | incomplete
```

`plays/first.play.yaml`. 최상위 키는 이 네 개만 허용됩니다:

```yaml
state: { user.runs: 10 }
facts: ["knows(vesna, manifest)"]
steps:
  - occasion: hubVisit
  - occasion: talk
    target: npc.vesna          # only on a `target: true` occasion
  - occasion: inbox
    pick: megNote              # required on `select: all`, refused on `select: first`
  - newRun: true               # resets run.*, run facts, once: run
  - occasion: runEnd           # judges <objective on="runEnd">
choose: { greet: wave, chat: [coffee, leave] }
```

→ [CLI 레퍼런스](/tooling/cli/) · [트레이싱](/tooling/tracing/) · [스토리 플레이](/tooling/play/)

## 자주 만나는 진단

| 코드 | 보통의 의미 |
|---|---|
| `E-KIND-MISSING` | 프론트매터에 `kind:`가 없고 `defaults:`도 채워 주지 않습니다. |
| `E-META-MISSING` | 씬에 `id:`도 `character` + `season` + `episode`도 없습니다. |
| `E-META-PARSE` | 프론트매터가 올바른 YAML이 아닙니다. 대개 `: `가 들어간 값을 따옴표 없이 썼을 때입니다. |
| `E-META-UNKNOWN-KEY` | 이 kind에서 쓸 수 없는 키입니다. 예: 퀘스트의 `after:`(`<quest after=…>`를 쓰세요). |
| `E-CONTENT-OUTSIDE-SHOT` | 첫 `## 제목` 앞에 콘텐츠가 있습니다. |
| `E-DOMAIN-UNKNOWN` | `emotion=`, `action=`, `anchor`, `mood` 등을 썼지만 그 슬롯에 선언된 멤버가 없습니다. |
| `E-UNDECLARED` / `E-UNDECLARED-REF` | 상태 경로나 `@def`가 선언되지 않았거나, 그 스키마를 가져오지 않았습니다. |
| `E-MAYBE-UNSET` | 기본값도, 앞선 `::set`도, `isSet` 가드도 없는 경로를 읽었습니다. |
| `E-UNSET-UNCOVERED` / `E-NONEXHAUSTIVE` | `<match>`가 `unset`, enum 멤버, 숫자 틈을 놓쳤습니다. 갈래나 `<otherwise>`를 추가하세요. |
| `E-TAG-INLINE-BODY` / `E-TAG-NOT-ONE-LINE` | 태그와 본문이 한 줄에 있거나, 태그가 여러 줄로 나뉘었습니다. |
| `E-BRANCH-ALL-GUARDED` / `E-HUB-NO-EXIT` | 메뉴가 빌 수 있거나, 허브가 끝나지 않을 수 있습니다. |
| `E-SET-TYPE` / `E-REF-TYPE` / `E-ATTR-TYPE` | 값의 타입이 자리에 맞지 않습니다. 디렉티브 안의 따옴표 친 `"@def"`가 흔한 원인입니다. |
| `E-DEF-DECL` | def 형식이 잘못되었습니다: 타입을 추론할 수 없거나, `type:` 없이 `params:`를 썼거나, 모르는 키가 있습니다. |
| `E-OBJECTIVE-MISSING-DONE` / `E-OBJECTIVE-QUEST-DONE` | 목표에 `done`이 없거나, `quest=`와 `done=`을 함께 썼습니다. |
| `E-GRAMMAR-NOT-ADMITTED` | 이 kind에서 허용되지 않는 구문입니다. 예: 엔트리 안의 `<branch>`, 퀘스트 안의 제목. |
| `E-BEAT-ATTR` | 비트 키 형식이 잘못되었거나 `on`이 없거나, `when`이 `scene.*`를 읽거나, 대상 없는 계기에 `target`을 썼습니다. |
| `E-OCCASION-UNKNOWN` | 플러그인이 계기를 선언했는데 이 계기는 그중에 없습니다. |
| `E-BEAT-UNREACHABLE` / `E-ARM-DEAD` | 조건이 결코 성립할 수 없습니다. `check-project`는 팩트 질의도 판정합니다. |
| `E-ACCEPT-TARGET` | `::accept`가 없는 퀘스트나 `start`가 있는 퀘스트를 가리킵니다. |
| `E-CONN-UNKNOWN-NODE` | `visited('…')`나 `after`가 프로젝트에 없는 씬을 가리킵니다. |
| `E-CONN-EPISODE-ID-DUP` / `E-QUEST-ID-DUP` | 두 문서가 같은 씬 id나 퀘스트 id를 씁니다. |
| `W-FACT-GUARANTEED` | 팩트 가드가 모든 경로에서 항상 참이라 불필요합니다. |
| `W-BEAT-SHADOWED` | 항상 자격이 있고 소진되지 않는 앞선 비트가 매번 이깁니다. |
| `W-ENTRY-REF-UNKNOWN` | `entry.<id>.read`가 아무도 선언하지 않은 엔트리를 가리킵니다. |
| `E-LEGACY-CONTENT-SIGIL` · `W-WHEN-TEST-LITERAL` | 옛 문법입니다. `lute fix`가 고쳐 줍니다. |
| `E-PERSIST-REMOVED` | 선택지에서 `persist=`를 직접 지우세요. `into=`만으로 run 팩트가 기록됩니다. |

## 주의할 점

**`start`가 없는 퀘스트는 스스로 활성화되지 않습니다.** 수락형이라서, 씬이 `::accept{quest="id"}`를
실행하거나 목이나 테스트가 `accepts:`에 적을 때까지 `unset`으로 남습니다. `start`가 있는 퀘스트에
`::accept`를 쓰면 `E-ACCEPT-TARGET`입니다. 예외는 `quest=`로 지정된 자식으로, 부모와 함께 활성화됩니다.

**`once`의 기본값은 `run`입니다.** `once: false`로 쓰지 않으면 씬 비트는 런마다 최대 한 번 재생됩니다.
엔트리에는 `once`가 없으니 `when="!entry.<id>.read"`로 가드하세요.

**`after:`는 구조이고, 상태는 `when:`이 담당합니다.** `after:`는 `&&`와 `||`로 묶은 `visited`,
`completed`, `active`만 읽으며 씬 그래프에 쓰입니다. 비트는 둘 다 성립할 때만 자격이 있습니다.
`<quest>`의 `after=`는 그래프 메타데이터일 뿐 활성화를 늦추지 않습니다. 퀘스트를 씬에 묶으려면 조건을
`start`에 넣으세요: `start="visited('cafe.counter')"`.

**`::end`는 현재 씬이 아니라 `lute play` 진행 전체를 끝냅니다.** 게임에 제어를 돌려줘야 하는 허브나 비트
씬은 마지막 줄에서 그냥 끝나면 됩니다.

**`visited()`는 세이브 전체에 걸칩니다.** 새 런에서도 지워지지 않습니다. 단일 파일 `check`는 이를 판정하지
않고, `check-project`는 id를 검증하며, `trace`와 `test`에는 `visited:` 목록이 필요합니다.

**def의 타입은 본문에서 나옵니다.** `calm: "run.pressure < 2"`는 bool입니다. 다른 def만 담은 본문
(`"@calm"`)은 추론할 수 없고, `params:`가 있는 def에는 `type:`이 필요합니다.

**`: `가 들어가거나 `!`, `@`, `{`, `[`, `*`, `&`로 시작하는 YAML 값은 따옴표로 감싸세요.**
`title: Chapter 1: Start`는 파싱 오류이고, 따옴표 없는 `when: !run.x`는 부정이 아니라 YAML 태그입니다:

```lute expect="E-META-PARSE"
---
kind: scene
id: chapter.one
title: Chapter 1: Start
---

## Start

@narrator: Once upon a time.
```

**`scene.choices.<branch>`는 branch 바로 뒤에서도 값이 없을 수 있습니다.** match에 `is="unset"` 갈래나
`<otherwise>`를 두세요:

```lute expect="E-MAYBE-UNSET,E-UNSET-UNCOVERED"
---
kind: scene
id: choice.readback
---

## Ask

<branch id="ask">
  <choice id="yes" label="Yes">
    @mira: Great.
  </choice>
  <choice id="no" label="No">
    @mira: Oh.
  </choice>
</branch>

<match on="scene.choices.ask">
  <when is="yes">
    @mira: You said yes.
  </when>
  <when is="no">
    @mira: You said no.
  </when>
</match>
```

**단일 파일 명령은 `lute.project.yaml`을 찾지 않습니다.** `check`, `trace`, `compile`, `context`, `test`는
`--project <dir>`를 줄 때만 프로젝트를 해석합니다. 없으면 `defaults:`로 끌어올린 것과 플러그인이 선언한
것이 모두 빠집니다. `check-project`, `play`, `scenario`는 넘겨받은 디렉터리에서 프로젝트를 읽습니다.

**`trace`는 명시한 세계에서 시작합니다.** 스키마의 `facts:` 시드는 불러오지 않으니 `--fact`나 `facts:`로
넘기세요. `lute run`과 `lute play`는 시드와 Datalog 규칙을 적용합니다.

**디렉티브 속성에는 따옴표 없는 참조를 씁니다.** `::camera{zoom=@zoom}`처럼 쓰세요. 따옴표 친
`zoom="@zoom"`은 문자열 `@zoom` 그대로입니다.

**`<match>`에서 숫자는 실수입니다.** `is="1..9"` 다음에 `is="10.."`를 써도 `9.5`는 다뤄지지 않습니다.
`<otherwise>`를 두거나, 끝이 맞닿는 열린 범위를 쓰세요.

**`lute trace`는 실행된 `::next`에서 멈춥니다.** 트레이스는 점프를 보고한 뒤 끝나므로 대상 `::mark` 뒤의
콘텐츠는 보이지 않습니다. 컴파일된 산출물에 `lute run`을 쓰면 점프를 따라갑니다.
