---
title: 치트시트
description: "Lute 0.23.0로 글을 쓰는 동안 열어 두는 한 페이지: 모든 구문을 검사된 최소 스니펫으로 보여 줍니다(프로젝트 구성, 프론트매터, 대사, 선택지, match, 상태, CEL, 비트, 퀘스트, 로어, 컴포넌트, 타임라인). CLI 요약, 작가가 가장 자주 만나는 진단 코드, 주의할 점도 담았습니다."
---

모든 구문을 한 페이지에 복사해 쓸 수 있는 스니펫으로 모았습니다. 아래의 `lute` 블록은 모두 CI에서 실제
툴체인으로 컴파일 검사를 거치며, 각 절 끝의 링크는 전체 설명 페이지로 이어집니다.

## 프로젝트 구성

```
my-game/
├── lute.project.yaml            profiles, plugins, identity, defaults
├── world.schema.yaml            run/user/app state, enums, defs, facts, rules
├── plugins/game.occasions/      optional: plugin.yaml + occasions/*.yaml + events/*.yaml
├── scenes/*.lute                kind: scene
├── quests/*.lute                kind: quest
├── lore/*.lute                  kind: lore
├── components/*.component.lute  component: <name>
├── tests/*.test.yaml            lute test
└── plays/*.play.yaml            lute play; lute test runs those with an expect:
```

`lute init --template beats <dir>`는 이 구성을 그대로 만들어 줍니다: 계기 플러그인, `id:`가 있는 비트,
퀘스트, 엔트리 비트, `engine:` 스텝과 `expect:`가 담긴 플레이 스크립트, 시나리오 테스트까지 들어 있으며,
만든 그대로 `check-project`, `test`, `play`를 통과합니다.

`lute.project.yaml`:

```yaml
pluginsDir: plugins/
defaultProfile: game
profiles:
  game:
    plugins: { game.occasions: true }   # true = active with defaults
identity:                               # both values below are the defaults
  lineId: "{prefix}.{speaker}_{code}"
  voiceKey: "{prefix}.{speaker}-{code}" # the 0.21 default was {speaker}-{code}: pin it to keep old keys
defaults:                               # frontmatter every document inherits
  luteVersion: "0.23.0"
  uses: [world.schema.yaml]             # resolved against THIS file's directory
```

`defaults:`에는 `kind`, `character`, `season`, `episode`, `pov`, `luteVersion`, `contentLang`,
`uses`, `extends`, `components`, `extra`만 쓸 수 있습니다(그 밖의 키는 `E-DEFAULTS-KEY`). 문서가 어떤 키를
직접 쓰면 그 키의 기본값은 병합 없이 통째로 대체됩니다(`uses: []`는 "가져오기 없음"). 문서의 kind에서
허용되지 않는 기본값은 그 문서에는 적용되지 않습니다.

0.22.0부터 기본 `voiceKey`에 `{prefix}`가 들어가므로 보이스 키는 프로젝트 전체에서 유일합니다. 0.21 키로
음성을 녹음해 둔 프로젝트는 `identity: { voiceKey: "{speaker}-{code}" }`로 고정하면 되고, 그러면 텍스트가
다른 줄들이 한 키에 모이는 곳마다 `E-DUP-VOICEKEY`가 납니다.

`world.schema.yaml`은 `---` 구분선이 없는 일반 YAML입니다. 씬은 `uses:`로 이 파일을 가져옵니다:

```yaml
state:                                   # scalar only: number | bool | string | enum
  run.pressure: { type: number, default: 0 }
  run.day:      { type: number, default: 1, owner: engine }   # content reads it; ::set is E-ENGINE-OWNED-WRITE
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
  closeUp: "1.3"                              # a constant: the only kind of def an attribute takes
  atLeast: { type: bool, params: { n: number }, cel: "user.runs >= n" }   # params need type:
cast:                                    # optional (0.23.0): once declared, any other speaker is E-CAST-UNKNOWN
  vesna: { name: Vesna }
  toma:  { name: Toma }
  mira:  { name: Mira }
```

def 타입 추론: 비교, `&&` `||` `!`, `holds`, `has`, `isSet`은 `bool`이고, `count`, 산술, 숫자 리터럴은
`number`이며, 경로를 그대로 읽으면 그 경로의 타입입니다. 검사기가 타입을 알 수 없는 본문(예: `"@other"`)은
긴 형태 `{ type: …, cel: … }`로 써야 하며, 그렇지 않으면 `E-DEF-DECL`입니다.

어휘 슬롯은 `emotion`, `action`, `anchor`, `mood`, `volume`, `musicAction`, `vfxType` 일곱 가지입니다.
아무도 선언하지 않은 슬롯을 쓰면 `E-DOMAIN-UNKNOWN`입니다. `action`에는 `exits:`를, `anchor`에는
`default:`를 반드시 적어야 합니다.

`cast:`(0.23.0)는 화자를 선언합니다. 스키마 문서에 쓰거나, 플러그인의 `cast` 내보내기(`cast/*.yaml`, 같은
`cast:` 맵)로 선언합니다. 캐스트가 하나라도 선언되면 그 밖의 화자(`@narrator` 제외)는 씬, 퀘스트, 엔트리,
번들 비트 어디서나 비슷한 이름을 제안하는 `E-CAST-UNKNOWN`입니다. 캐스트를 선언하지 않으면 어떤 화자 id든
받아들여집니다. 씬 프론트매터에는 `cast:`를 쓸 수 없습니다(`E-META-UNKNOWN-KEY`). `lute context`가 캐스트를
보여 줍니다.

→ [상태 스키마](/state/schemas/) · [가져오기](/language/imports/) · [콘텐츠 어휘](/language/vocabulary/) · [팩트와 Datalog](/state/facts-and-datalog/)

## kind별 프론트매터 키

| kind | 필수 | 그 kind에서만 쓰는 키 |
|---|---|---|
| `kind: scene` | `id:`, 또는 레거시 `character` + `season` + `episode` | `id`, `character`, `season`, `episode`, `episodeId`, `pov`, `after`, 비트 키 `on` / `target` / `when` / `priority` / `once` / `also` |
| `kind: quest` | 본문에 `<quest>` 하나 이상 | `id` (선택, 묶음 이름) |
| `kind: lore` | 본문에 `<entry>`나 `<beat>` 하나 이상 | `id` (`<beat>`가 있으면 필수), `series` |
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
| `@speaker{attrs}: text` | `@narrator`는 내레이션이고, 그 밖의 화자는 모두 대사입니다(`pov:` 화자도 마찬가지이며, `pov`는 설명용일 뿐입니다). `: ` 뒤의 텍스트는 줄 끝까지 그대로입니다. `cast:`가 선언되어 있으면 화자는 그 안에 있어야 합니다(`E-CAST-UNKNOWN`). |
| 줄 속성 | `code`, `emotion`, `variant`, `action`, `dialogMotion`, `as`(이름표 덮어쓰기), `when`(가드), `id`(점프 라벨) |
| 전달 플래그 | `{mono}` 속마음, `{os}` 화면 밖, `{vo}` 보이스오버. 한 줄에 하나까지이며 `@narrator`에는 쓸 수 없습니다. |
| `{{…}}` | `{{userName}}`, 선언된 상태 경로, 또는 `{{@def}}`(산출물에 def 본문이 실리고 `lute run` / `lute play`가 그 값을 계산합니다). 값이 없을 수 있는 경로를 읽으면 `E-MAYBE-UNSET`입니다. 줄의 텍스트 전체가 `@name`이면 그 글자가 그대로 출하됩니다(`W-TEXT-LOOKS-LIKE-REF`). `{{@name}}`으로 쓰세요. |
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

<hub id="chat" prompt="Anything else?">
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
| `<hub id>` | `exit`를 고를 때까지 자격 있는 선택지를 다시 보여 줍니다. `once`는 한 번 고른 선택지를 없앱니다. 가드 없는 `exit`가 있거나 모든 선택지가 `once`여야 합니다(`E-HUB-NO-EXIT`). 고를 때마다 `scene.visited.<hub>.<choice>`가 설정됩니다. 선택 속성 `prompt=`(0.23.0)는 선택지와 함께 보여 줄 질문이며, 비어 있으면 `E-BRANCH-PROMPT`입니다. |
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
@vesna{when="isSet(prev.run.trust) && prev.run.trust >= 3"}: You trusted me last time.
@vesna{when="run.seen"}: You found the archive.
::assert{knows(vesna, manifest)}
@vesna{when="holds(knows(vesna, manifest))"}: I read the manifest.
@vesna{when="count(knows(_, manifest)) >= 2"}: So we both know.
::retract{knows(vesna, _)}
```

`_`는 질의와 `::retract`에서 와일드카드입니다. `app.*`는 읽기 전용이며(`E-APP-READONLY`), `owner: engine`으로
선언한 경로도 마찬가지입니다(`E-ENGINE-OWNED-WRITE`). 그 경로는 엔진이 쓰고, `lute play`에서는 `engine:`
스텝으로 씁니다. `derive: true`나 `reserved:` 관계는 콘텐츠에서 assert할 수 없습니다. 관계의 `key: [0]`은
첫 번째 인자를 함수적으로 만들어, 새로 assert하면 이전 팩트를 대체합니다.

`prev.run.<path>`(0.23.0)는 선언된 모든 `run.*` 경로에 대해, 이전 런이 끝났을 때 `run.<path>`가 가졌던
값을 같은 타입으로 읽습니다. 읽기 전용이고(`E-QUEST-RESERVED-WRITE`) 첫 런이 끝나기 전에는 값이 없으므로,
읽을 때마다 `isSet(prev.run.x)`나 `unset` 갈래가 필요합니다(`E-MAYBE-UNSET`). `prev.*` 경로를 직접
선언하면 `E-STATE-NAMESPACE`입니다. `lute play`는 `newRun`에서 스냅숏을 뜨고, 플레이 스크립트나 목의
`state:`로 시드할 수 있습니다.

→ [상태 모델](/state/state-model/) · [팩트와 Datalog](/state/facts-and-datalog/)

## CEL 요약

| 네임스페이스 | 초기화 시점 | 콘텐츠가 쓸 수 있는가 |
|---|---|---|
| `scene.*` | 씬이 끝날 때 | 예 |
| `run.*` | 새 런에서 | 예 |
| `user.*` | 프로필 초기화 시 | 예 |
| `app.*` | 앱 삭제 시 | 아니요 |
| `quest.<id>.state`(항상 값이 있음: 퀘스트가 활성화되기 전에는 `unset`, 그 뒤로 `active` `complete` `failed`), `quest.<id>.activatedAt`, `quest.<id>.objectives.<o>.done` | 엔진. `tier="run"` 퀘스트는 새 런에서 `unset`으로 돌아감 | 아니요 |
| `entry.<id>.read` | 엔진, run 등급 | 아니요 |
| `entry.<id>.everRead` | 엔진, user 등급: 처음 읽을 때 설정되고 새 런에서도 초기화되지 않음 | 아니요 |
| `owner: engine`으로 선언한 경로 | 그 네임스페이스를 따름 | 아니요(`E-ENGINE-OWNED-WRITE`) |
| `prev.run.<path>`(0.23.0) | 엔진: 런이 끝날 때의 `run.<path>` 스냅숏. 첫 런이 끝나기 전에는 값이 없음 | 아니요(`E-QUEST-RESERVED-WRITE`) |
| `scene.choices.<branch>`, `scene.visited.<hub>.<choice>` | 엔진 | 아니요 |
| `visited('<scene id>')`, `visited('<doc>.<beat>')` | 초기화되지 않음: 세이브 전체 | 아니요 |

| 연산자 | 함수와 참조 |
|---|---|
| `== != < <= > >=` · `&& \|\| !` · `+ - * /` · `c ? a : b` · `x in ['a', 'b']` · 문자열·숫자 리터럴 | `has(p)` / `isSet(p)`(값이 있는가) · `holds(rel(a, _))` · `count(rel(_)) >= n` · `validAt(rel(a), quest.q.activatedAt)` · `visited('scene.id')` · `@def` / `@def(args)` · `$`(`<match>` 안에서만) |

쓸 수 없는 것: `%`, `size`, `matches`, `map`/`filter`/`exists`/`all`(`E-CEL-PROFILE`). 가드에서도 def
본문에서도 마찬가지입니다. 값이 없음은 문자열 `'unset'`이 아닙니다(`E-UNSET-LITERAL`). `!isSet(p)`나
`is="unset"`으로 확인하세요. 예외는 `quest.<id>.state`로, 여기서는 `unset`이 실제 멤버입니다.
`quest.q.state == 'unset'`으로 쓰세요. `isSet(quest.q.state)`는 항상 참입니다(`W-QUEST-STATE-ISSET`).
경로 세그먼트, def 이름, 파라미터 이름에는 `-`를 쓸 수 없습니다.

CEL이 들어가는 곳: `<match on>`, `<when test>`, 줄이나 선택지의 `when=`, `::set`의 우변, `::next when`,
비트 `when:`, 엔트리 `when=`, 퀘스트 `start` / `fail`, 목표 `done` / `by` / `when`, `<on when>`,
`<reward when>`. 디렉티브 속성에서 def 참조는 따옴표 없이 씁니다: `zoom="@closeUp"`이 아니라
`::camera{zoom=@closeUp}`입니다. 속성에는 def가 접히는 상수가 들어가므로, 위의 `zoom`처럼 상태를 읽는
def는 `E-ATTR-DEF-DYNAMIC`입니다. `<match>`로 나누고 갈래마다 리터럴을 쓰세요.

→ [CEL 표현식](/state/cel/) · [정의와 파라미터](/language/params/)

## 비트: 계기에 응답하는 씬과 엔트리

비트는 계기(occasion)가 발생했을 때 엔진이 고를 수 있는 씬, 로어 엔트리, 번들 비트입니다.

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

<entry id="vesnaFirst" on="talk" target="npc.vesna" category="bark" priority="20" once="user">
  @vesna: So you are the new one.
</entry>
```

로어 문서에는 엔트리 옆에 `<beat>` 블록, 곧 번들 비트도 둘 수 있습니다(0.23.0):

```lute check
---
kind: lore
id: cafe.talks
state:
  run.tips: { type: number, default: 0 }
---

<beat id="miraOrder" on="talk" target="npc.mira" title="Order" priority="10" when="run.tips >= 3">
  @mira: The usual?
  <hub id="order" prompt="What will it be?">
    <choice id="usual" label="The usual" exit>
      @mira: Coming up.
    </choice>
  </hub>
</beat>

<beat id="miraHum" on="talk" target="npc.mira" once="false" also>
  @narrator: Mira hums while she works.
</beat>
```

번들 비트는 `id`, `on`, `target`, `title`, `when`, `priority`, `once`, `also`를 받고, 본문은 씬 본문(대사,
branch, 허브, match, 디렉티브)입니다. 문서에는 `id:`가 있어야 하고, 비트 `id`는 `-`가 없는 식별자이며, 비트의
정식 id는 `<문서 id>.<비트 id>`(`cafe.talks.miraOrder`)입니다. `lute play`, `presented:`,
`visited('cafe.talks.miraOrder')`, `lute trace --beat`가 이 id를 씁니다. 씬 비트처럼 동작합니다: `once`의
기본값은 `run`이고, 제시되면 소진되며, `after:`는 없습니다. `title`은 `select: all` 메뉴에서 비트의
이름표가 됩니다. 정식 id가 씬 id와 같으면 `E-CONN-EPISODE-ID-DUP`입니다.

| 키 | 의미 |
|---|---|
| `on` | 응답하는 계기. 이 키가 씬을 비트로 만듭니다. `on` 없이 다른 비트 키를 쓰면 `E-BEAT-ATTR`입니다. |
| `target` | 선택, 점으로 구분한 id(`npc.vesna`). 계기가 그 대상에 대해 발생했을 때만 후보가 됩니다. 계기가 대상 도메인을 선언했다면 대상은 그 도메인의 `<prefix>.<member>`여야 합니다. |
| `when` | `run` / `user` / `app`, `quest.*`, `entry.*.read` / `entry.*.everRead`, 팩트, `visited()`에 대한 CEL. `scene.*`는 읽을 수 없습니다. 문자열을 비교할 때는 YAML 값을 큰따옴표로 감싸 CEL이 작은따옴표를 쓸 수 있게 하세요: `when: "run.slot == 'night' && user.runs >= 3"`. 두 층 모두 작은따옴표를 쓰면 `E-META-PARSE`입니다. |
| `priority` | 정수, 기본값 `0`. 높은 쪽이 이깁니다. |
| `once` | 씬: `run`(기본값), `user`(평생 한 번), `false`(반복 가능). 엔트리: `once="run"`(새 런이 `entry.<id>.read`를 초기화할 때까지) 또는 `once="user"`(`entry.<id>.everRead`가 설정되면 소진). 엔트리에 `once`가 없으면 반복됩니다. |
| `also` | 0.23.0. `select: first` 계기의 씬(`also: true`)과 번들 비트(`also`): 승자 뒤에, 또는 주 비트가 하나도 자격이 없을 때는 혼자 제시되며, 승자를 대신하지 않습니다. 엔트리에 쓰거나 `select: all` / `sequence` 계기에 쓰면 `E-BEAT-ATTR`입니다. `W-BEAT-SHADOWED`와 `W-BEAT-PRIORITY-TIE`는 `also` 비트를 무시합니다. |

선택: 후보는 `on`이 일치하고 `target`이 없거나 발생한 대상과 같은 비트입니다. 후보는 `after:`와 `when`이
성립하고 `once`가 소진되지 않았을 때 자격이 있습니다. 자격 있는 비트는 priority, 문서 경로, 선언 순서로
정렬됩니다. `select: first`는 `also`가 아닌 맨 위의 비트를 제시한 뒤 자격 있는 `also` 비트를 제시하고,
`select: sequence`(0.23.0)는 자격 있는 비트를 그 순서대로 모두 제시하며(루틴 다음에 그날의 이벤트),
`select: all`은 플레이어가 고르게 합니다. 자격은 계기가 발생할 때 한 번 정해지고, 퀘스트는 제시할 때마다
정산됩니다. 대상이 같거나 둘 다 없고 priority도 같은 `select: first` 비트 둘의 `when`이 서로 배타적임을
증명할 수 없으면 승자는 파일 순서로 정해집니다(`check-project`의 `W-BEAT-PRIORITY-TIE`). 계기와 월드
이벤트, 그리고 선택적으로 보상 종류와 캐스트는 플러그인이 선언합니다:

```yaml
# plugins/game.occasions/plugin.yaml
id: game.occasions
version: 0.1.0
kind: capability
depends: [ { id: lute.core, range: "^0.0.1" } ]
exports:
  occasions: occasions/
  events: events/
  rewardkinds: rewardkinds/             # rewardKinds: see Quests
  cast: cast/                           # cast: { <id>: { name } }, like a schema's cast:
```

```yaml
# plugins/game.occasions/occasions/game.yaml
occasions:
  hubVisit: { select: first }
  talk:     { select: first, target: { prefix: npc, entity: crew } }   # or `target: true`: any dotted id
  runEnd:   { select: first }
  evening:  { select: sequence }        # every eligible beat, in selection order (0.23.0)
  inbox:    { select: all, description: Letters waiting at the fountain }
```

```yaml
# plugins/game.occasions/events/game.yaml: for <on event> and a play's `event:` step
events:
  - name: combatEnd
```

계기를 선언하는 플러그인이 없으면 어떤 식별자든 받아들여집니다. 플러그인이 계기를 선언한 뒤에는 모르는
계기가 `E-OCCASION-UNKNOWN`이 되고, `target`을 선언하지 않은 계기에 `target`을 쓰면 `E-BEAT-ATTR`입니다.
대상 도메인 `{ prefix, entity }`는 그 `entities:` 종류의 멤버마다 `<prefix>.<member>`를 허용하므로(`open:`
종류라면 어떤 멤버든), `target: npc.vesan`은 비슷한 이름을 제안하는 `E-BEAT-ATTR`입니다.

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
  run.day:   { type: number, default: 1 }
  user.xp:   { type: number, default: 0 }
---

<quest id="regular" title="Become a regular" start="true" fail="run.fired" after="visited('cafe.counter')">
  <reward kind="BADGE" amount="1"/>
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

<quest id="lostCup" title="Find the lost cup" tier="run">
  <objective id="find" title="Find it by day 3" done="run.found" by="run.day > 3"/>
  <objective id="tell" title="Tell Mira" on="talk" target="npc.mira" done="run.found"/>
</quest>
```

| 요소 | 규칙 |
|---|---|
| `start=` | 성립하면 퀘스트를 활성화합니다(`unset` → `active`). `start`가 없으면 수락형입니다: 씬이 `::accept{quest="…"}`를 실행하거나 목(mock)이 수락할 때까지 `unset`으로 남습니다. 플레이 스크립트는 `quests:`로 세이브의 상태를 시드합니다. |
| `fail=` | `active` → `failed`. 완료 조건과 동시에 성립하면 실패가 이깁니다. |
| `after=` | 씬 그래프를 위한 구조적 선행 조건: `&&` / `\|\|`로 묶은 `visited` / `completed` / `active`. 활성화를 막지 않으며, 활성화는 `start`가 정합니다. 씬은 프론트매터에 `after:`로 씁니다. |
| `tier="run"` | 새 런에서 퀘스트가 `unset`으로 돌아가고 목표도 모두 미완료가 됩니다. 기본값 `tier="user"`는 런이 바뀌어도 상태를 유지합니다. |
| `<objective done>` | `done`은 필수입니다(`E-OBJECTIVE-MISSING-DONE`). `optional`이 아닌 목표가 모두 완료되면 퀘스트가 완료됩니다. 완료는 되돌려지지 않으며 본문은 한 번만 재생됩니다. |
| `on="runEnd"` | 퀘스트가 활성인 동안 그 계기가 발생했을 때만 `done`을 판정합니다. |
| `by=`(0.23.0) | 기한입니다. 목표가 완료되지 않은 동안 처음으로 성립하면 목표는 영구히 실패하고, 필수 목표가 실패하면 퀘스트도 실패합니다(`failed` 보상, `questFailed`). `done`을 먼저 판정하므로 완료된 목표가 기한 때문에 실패하지는 않습니다. |
| `on="talk" target="npc.mira"`(0.23.0) | 계기가 그 대상에 대해 발생했을 때만 판정하며, 비트의 대상처럼 검사합니다(`E-BEAT-ATTR`). 도구에서는 `talk@npc.mira`로 발생시키고, `lute play`에서는 `target:`이 있는 스텝이 판정합니다. |
| 목표의 `when=` | 표시 여부만 정합니다. 완료에는 영향을 주지 않습니다. |
| `quest="child"` | 하위 퀘스트: 자식이 완료되면 완료되고, 필수 자식이 실패하면 부모도 실패합니다. `done=`과 함께 쓸 수 없습니다(`E-OBJECTIVE-QUEST-DONE`). `start`가 없는 자식은 부모와 함께 활성화됩니다. |
| `<reward kind amount target when on/>` | 엔진이 지급하는 데이터입니다. 콘텐츠는 보상을 읽을 수 없으므로 같은 재화를 `<on>` 핸들러에서 `::set`으로 또 올리지 마세요. 두 번 지급됩니다. `amount`는 정수나 범위 `N..M`입니다. `on="failed"`는 실패 시에 지급합니다. `lute run` / `lute play`는 `grant`를 출력하고, 보상 종류가 `credits:`(아래)를 선언했다면 스칼라 금액을 그 경로에 더합니다. 그 퀘스트의 `<on>`이나 목표 본문에서 같은 경로를 `::set`하면 `W-REWARD-DOUBLE-CREDIT`입니다. |
| `<on event>` | `questActive`, `questComplete`, `questFailed`, 또는 플러그인의 월드 이벤트. `when=`으로 가드할 수 있습니다. 결코 실패할 수 없는 퀘스트(`fail`도, `by=` 기한이 있는 필수 목표도, 실패할 수 있는 필수 하위 퀘스트 목표도, 부모 퀘스트도 없음)의 `questFailed` 핸들러는 `W-QUEST-HANDLER-DEAD`입니다. |

퀘스트 문서에는 `#`/`##` 제목, `<hub>`, `<timeline>`이 없습니다. 다른 문서는
`<match on="quest.regular.state">`나 `when="quest.regular.state == 'complete'"`로 퀘스트를 읽습니다.
상태는 항상 값이 있으므로 `quest.regular.state == 'unset'`은 "아직 받지 않음"을 뜻합니다.

보상 종류는 지급액이 들어갈 상태 경로를 지정할 수 있습니다(0.23.0):

```yaml
# plugins/game.occasions/rewardkinds/game.yaml
rewardKinds:
  GOLD:  { credits: user.gold }         # a grant adds its amount to user.gold (a range is the engine's roll)
  BADGE: {}
```

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
  `priority`, `once`(`run` 또는 `user`, 비트에만). 여러 파일에 걸친 시리즈에는 `series`와 `order`를 씁니다.
  문서 수준의 `series:`는 파일 안의 위치로 엔트리 순서를 정하며, 그런 문서에서 엔트리별 `series=`/`order=`는
  `E-ENTRY-ATTR`입니다.
- 엔트리 본문에는 콘텐츠 줄, `<match>`, `::set`, `::assert`, `::retract`만 둘 수 있습니다. `<branch>`,
  디렉티브, 제목을 비롯한 그 밖의 것은 `E-GRAMMAR-NOT-ADMITTED`입니다.
- 효과는 한 런에서 처음 읽을 때만 적용됩니다. 그 뒤로 `entry.<id>.read`는 `true`이며 어느 문서에서나 읽을
  수 있습니다. run 등급이라 새 런에서 초기화됩니다. `entry.<id>.everRead`는 그 user 등급 짝으로, 처음 읽을
  때 설정되고 새 런에서도 초기화되지 않습니다.
- 로어 문서에는 씬 본문을 가진 `<beat>` 블록도 둘 수 있습니다(0.23.0). [비트](#비트-계기에-응답하는-씬과-엔트리)를
  보세요.

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

@narrator: {{@who}} walks in.

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
`<match>`를 둘 수 있으며 스스로 상태를 읽거나 쓰지 않습니다. `{{@param}}`은 number, bool, enum
파라미터를 렌더링하고, 0.23.0부터는 `string` 파라미터도 렌더링합니다. 리터럴 `::use` 인자가 펼칠 때
치환되므로 호출 지점마다 자기 문장을 자기 `lineId`로 출하합니다. 텍스트에 끼워 넣는 `string` 파라미터에
`@def`를 넘기면 `E-REF-TYPE`입니다. 가져오는 씬은 `components:`에 파일을 적고
`::use`로 펼칩니다. 인자로 호출하는 쪽의 `@def`를 넘길 수 있습니다(`tier=@mood`). enum 파라미터라면 def가
낼 수 있는 모든 값이 멤버여야 하고(`E-COMPONENT-ARG`), 컴포넌트가 디렉티브 속성에 넣는 인자가 상태에
따라 바뀌는 def이면 `E-ATTR-DEF-DYNAMIC`입니다:

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

컴포넌트가 펼친 줄의 주소는 `{prefix}.{component}#{n}.{speaker}_{code}`입니다. `n`은 호스트가 그
컴포넌트를 `::use`한 순번(1부터, 문서 순서)이므로, 두 번 쓴 컴포넌트가 같은 `lineId`를 공유하지 않고
컴포넌트 줄의 code도 주변 호스트 줄에 따라 달라지지 않습니다.

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
| `lute check <file> [--project <dir>]` | 문서 하나를 검사합니다. `--project`가 없으면 파일 위쪽에서 가장 가까운 `lute.project.yaml`을 적용하고, 그 사실을 stderr에 알립니다. |
| `lute check-project <dir> [--wip]` | 모든 문서와 함께 연결성, 퀘스트 id, `::accept` 대상, 계기, 팩트 가드를 검사합니다. 깨끗한 문서는 모두 컴파일까지 해 보므로, 컴파일 단계 오류(`E-DUP-VOICEKEY`, `E-CAPABILITY-MISMATCH`)도 여기서 실패합니다. `W-BEAT-PRIORITY-TIE`, `W-QUEST-HANDLER-DEAD` 같은 프로젝트 권고도 여기서 나옵니다. `--wip`(0.23.0)는 아직 아무것도 만들어 내지 않는 관계(시드, assert, 규칙, `reserved` 모두 없음) 때문에만 가드가 죽은 `E-BEAT-UNREACHABLE`, `E-ENTRY-UNREACHABLE`, `E-OBJECTIVE-UNSATISFIABLE`을 경고로 낮춥니다. 만드는 쪽이 있는데도 결코 맞지 않는 관계는 여전히 오류입니다. |
| `lute fix <file\|dir>` | 기계적 이전을 제자리에서 적용합니다: 옛 `:line` 표기, `as=` → `into=`, `test="$ == …"` → `is=`. 디렉터리를 주면 그 아래의 모든 `.lute` 파일을 재귀적으로, 정렬 순서대로 처리합니다. |
| `lute tag <file\|dir>` | 파일 하나, 또는 디렉터리 아래 모든 `.lute` 파일의 모든 줄에 안정적인 `code`를 채웁니다. |
| `lute compile <file> -o out.json` · `--all --project <dir> -o <outdir>` | 산출물을 만듭니다. `--all`은 `beats`를 포함한 `project.index.json`도 씁니다. |
| `lute trace <file> [--mock m.yaml] [--state P=V] [--fact "r(a)"] [--choose id=c[,c]] [--event e] [--accept q] [--occasion o[@target]] [--entry id \| --beat id] [--no-derive]` | 프로젝트의 시드 팩트와 규칙을 적용한 채 소스를 목에 맞춰 미리 봅니다. 종료 코드 `3`은 판정할 수 없는 가드를 만났다는 뜻입니다. `--occasion talk@npc.mira`는 대상에 대해 계기를 발생시킵니다(0.23.0). `--beat`는 번들 비트 하나를 로컬 id나 정식 id로 제시합니다(없는 id면 `E-TRACE-BEAT`). |
| `lute run <artifact> [--mock m.yaml] [--occasion o[@target]] [--entry id \| --beat id]` | 컴파일된 산출물을 엔진처럼 실행합니다. 로어 산출물에는 `--entry`와 `--beat`(번들 비트의 정식 id, 모호하지 않으면 로컬 id) 중 정확히 하나가 필요합니다. |
| `lute play <dir> --script p.play.yaml [--json] [--explain <atom>] [--no-derive]` | 프로젝트 전체에 계기를 발생시키며 퀘스트를 진행합니다. `expect:`가 어긋나면 종료 코드 `1`입니다. `--explain`(반복 가능)은 플레이가 끝난 뒤 ground atom의 도출 트리를, 성립하지 않으면 그것을 결론 낼 수 있는 규칙마다 실패한 전제를 출력합니다. |
| `lute test [<dir>] [--project <dir>] [--coverage] [--no-derive]` | 모든 `*.test.yaml`과, `expect:`가 있는 모든 `*.play.yaml`을 실행합니다. 미완료로 끝난 워크는 실패합니다. `--coverage`는 `--project`나 가장 가까운 `lute.project.yaml`의 프로젝트에서 어떤 테스트도 트레이스하지 않고 어떤 플레이도 제시하지 않은 문서를 나열합니다. |
| `lute scenario <dir> [reach <node> \| envelope <node> \| knowledge [--for <node>]] [--format text\|json\|dot]` | `after:` 그래프, 도달 가능성, 보장되는 상태와 팩트. 노드는 씬 id나 `quest:<id>`입니다. `knowledge`(0.23.0)는 팩트 가드가 있는 비트, 엔트리, 목표마다 질의하는 관계를 찾고, 각 관계를 규칙을 거슬러 그것을 만드는 쪽까지 추적합니다: assert하는 문서, 시드 팩트, 엔진(`reserved`), 또는 만드는 쪽 없음. `--for`에는 엔트리 id나 `<quest>.<objective>`도 줄 수 있습니다. |
| `lute beats <dir> [--occasion o] [--target t] [--json]` | 0.23.0. 계기별(대상별) 비트 사다리를 선택 순서대로 보여 줍니다: priority, `once`, `also`, `after:`, `when`, 제목, 그리고 `check-project`의 판정(도달 불가, 가려짐, 동점, once-run-user). 프로젝트가 깨끗하게 검사되지 않아도 됩니다. |
| `lute calendar <dir> --axis run.day=1..7 [--axis run.slot=day,night] [--occasion o] [--target t] [--script save.play.yaml] [--json \| --csv]` | 0.23.0. 축들의 곱의 모든 칸(첫 축이 가장 느리게 바뀜)에서 계기별로 play와 같은 자격 판정을 보여 줍니다: 승자나 제시 목록, 가려진 자격 있는 비트 `+N`, 판정할 수 없는 칸 `?`, 마지막으로 어느 칸에서도 자격이 없는 비트. 스크립트의 세이브(그 `steps:`는 재생하지 않음)나 선언된 기본값에서 시작합니다. |
| `lute lore <dir>` | 대상별·시리즈별 엔트리와 그 엔트리가 드러내는 팩트. |
| `lute context <file> [--project <dir>]` | 여기서 쓸 수 있는 모든 것: 디렉티브(내장 포함), 어휘, 상태(`owner: engine` 표시), def, 등급과 `reserved` 여부를 담은 관계, 대상 도메인을 담은 계기, 캐스트, 컴포넌트 시그니처, 모든 씬·퀘스트·엔트리 id. |
| `lute lint [<path>] [--config lute.lint.yaml]` | 프로젝트별로 설정하는 권고성 편집 린트(`L-*`). 선형 VN 지표는 비트, 컴포넌트, 퀘스트, 로어를 건너뜁니다. |
| `lute doctor [<dir>]` | 툴체인과 프로젝트 설정: 버전, 활성 플러그인, 계기별로 응답하는 비트 수, 플레이 스크립트와 테스트, `PATH`의 `lute-lsp`가 이 버전인지. |
| `lute new scene\|quest\|lore\|schema <name> [--dir <dir>]` · `lute init <dir> [--template minimal\|investigation\|beats]` | 문서나 프로젝트의 뼈대를 만듭니다. 새 문서에는 `id:`가 들어가고 `defaults:`가 채워 주는 것은 빠집니다. `lute new scene <name> --on <occasion> [--target <prefix>.<member>]`은 계기와 대상을 프로젝트에 맞춰 검사한 뒤 비트를 씁니다. |

trace 목(`--mock`). 테스트의 목 키도 같은 형식입니다:

```yaml
# mocks/counter.yaml: lute trace scenes/counter.lute --project . --mock mocks/counter.yaml
file: ../scenes/counter.lute               # required under mocks/, which check-project validates
state:  { run.tip: 5 }                     # path: literal
facts:  ["knows(vesna, manifest)"]         # base facts, on top of the schema's facts: seeds
choose: { greet: tip, chat: [coffee, leave] }   # branch: choice; hub: its visit order
events: [combatEnd]                        # world events, for <on event>
# derive: false                            # the 0.21 model: no seeds, no rules
# a save's history, for the paths the document reads:
#   visited:     [cafe.counter]            # scenes already played; unlisted = not visited
#   quests:      { regular: complete }     # quest.<id>.state
#   entriesRead: { run: [log1], user: [vesnaFirst] }   # entry.<id>.read / entry.<id>.everRead
# quest walks also take:
#   accepts:   [lostCup]                   # accept-driven quests to take up
#   occasions: [runEnd, talk@npc.mira]     # raised after the walk settles; <occasion>@<target> for a target
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
  transcriptLacks: ["You are no longer welcome."]
  exit: complete                                  # complete | incomplete; without it, incomplete fails
```

```yaml
# tests/counter.test.yaml
file: ../scenes/counter.lute
choose: { greet: wave, chat: [coffee, leave] }
expect:
  offered: { greet: [wave, tip] }       # the exact set offered: flirt's `when` failed
```

```yaml
# tests/barks.test.yaml: a lore test names the entries it presents
file: ../lore/barks.lute
entries: [vesnaFirst, vesnaBack]        # in order, read flags set between; or `entry: <id>`
state: { user.runs: 3 }
expect:
  transcriptContains: ["So you are the new one.", "Back again?"]
```

`offered:`는 branch나 허브가 제시한 선택지 집합 전체를 여러 번의 제시에 걸쳐 정확히 비교합니다. `entry:`나
`entries:`가 없는 로어 테스트는 `E-TEST-LORE`입니다. 엔트리 하나는 `lute trace <file> --entry <id>`로 미리 볼
수 있습니다.

`plays/first.play.yaml`. 최상위 키는 `state`, `facts`, `choose`, `derive`, 세이브 시드인 `visited`,
`presented`, `quests`, `entriesRead`, 그리고 `expect`와 `steps`입니다. 각 스텝은 `occasion`, `engine`,
`event`, `newRun` 중 하나입니다:

```yaml
visited: [cafe.counter]                 # save seeds, applied before step 1
presented: { user: [vesna.gift] }       # spent `once: user` / `once: run` beats
quests: { lostCup: active }             # unset | active | complete | failed
entriesRead: { user: [vesnaFirst] }     # run: entry.<id>.read · user: entry.<id>.everRead
state: { user.runs: 10, run.tips: 3 }
facts: ["knows(vesna, manifest)"]
choose: { greet: wave, chat: [coffee, leave] }   # hub: visit order; a branch list: one per presentation
steps:
  - occasion: hubVisit
    label: arrival                      # printed in the step header
    choose: { greet: tip }              # this step only, replacing that key
  - occasion: talk
    target: npc.vesna                   # <prefix>.<member> of the occasion's target domain
    expect: { winner: vesnaBack, offered: [vesnaBack, vesnaBark], notOffered: [vesna.gift] }
  - occasion: talk
    target: npc.mira
    expect: { presented: [cafe.talks.miraOrder, cafe.talks.miraHum] }   # the winner, then its `also` beats
  - occasion: inbox
    pick: megNote                       # required on `select: all`, refused on `select: first` and `sequence`
  - occasion: inbox
    pick: none                          # pass: nothing presented or spent
  - occasion: evening                   # select: sequence: every eligible beat, in selection order
  - occasion: runEnd                    # judges <objective on="runEnd">
  - event: combatEnd                    # a world event: active quests' <on event> run
  - engine:                             # writes what the engine owns; presents nothing
      state: { run.day: { add: 1 } }    # a literal, or { add: n }; quest.* is refused
      facts: ["awake(toma)"]            # any declared base relation, reserved ones included
      retract: ["awake(vesna)"]
  - newRun: { facts: ["knows(vesna, manifest)"] }   # or `true`; resets run.*, run facts, once: run, tier="run" quests
  - occasion: hubVisit
    repeat: 2
expect:                                 # judged at the end; a miss exits 1
  exit: complete
  quests: { regular: complete, lostCup: unset }
  state: { user.xp: 50, run.day: 1 }
  facts: ["can_halt(vesna)"]            # after derivation
  notFacts: ["awake(toma)"]
  transcriptContains: ["You are a regular now."]
  transcriptLacks: ["You are no longer welcome."]
```

- `engine:` 스텝은 선언된 상태, 팩트, retract를 쓰며, 무엇이든 재생하기 전에 타입을 검사합니다. 쓴 뒤에는
  퀘스트 수명 주기를 정산하므로 쓰기 한 번으로 그 자리에서 퀘스트가 완료될 수 있습니다. `quest.*`는
  거부합니다: 퀘스트 상태는 수명 주기의 몫이며 `quests:`로 시드합니다. `newRun`은 같은 `state:`와
  `facts:`를 새 런의 시드로 받습니다.
- `target`, `pick`, `choose`, `expect`는 `occasion` 스텝에만 쓰고, `label`과 `repeat`은 어느 스텝에나 쓸 수
  있습니다. 스텝의 `expect:`는 `winner`(계기가 지나가면 `none`), `offered`(자격 있는 비트의 부분집합),
  `notOffered`, `presented`(0.23.0: 제시된 id 전체를 순서대로)를 받습니다. 어긋나면 그 스텝과 실제 값을
  알려 줍니다. `target:`이 있는 스텝은 그 대상의 `<objective on target>`도 판정합니다.
- `lute play . --script plays/first.play.yaml --explain "can_halt(vesna)"`는 어떤 규칙이 그 atom을
  결론 냈고 각 전제가 어디서 왔는지 보여 줍니다.

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
| `E-MAYBE-UNSET` | 기본값도, 앞선 `::set`도, `isSet` 가드도 없는 경로를 읽었습니다. `prev.run.*`를 읽을 때는 항상 필요합니다. |
| `E-CAST-UNKNOWN` | 캐스트가 선언되어 있는데(스키마의 `cast:`나 플러그인의 `cast` 내보내기) 이 화자는 그 안에 없습니다. 메시지가 가장 가까운 id를 제안합니다. |
| `E-ENGINE-OWNED-WRITE` | `::set`이 `owner: engine`으로 선언된 경로에 씁니다. 콘텐츠는 읽기만 합니다. `lute play`에서는 `engine:` 스텝으로, trace와 test에서는 목의 `state:`로 쓰세요. |
| `W-QUEST-STATE-ISSET` | `isSet(quest.<id>.state)`는 항상 참입니다. `'unset'`과 비교하세요. |
| `W-TEXT-LOOKS-LIKE-REF` | 줄의 텍스트 전체가 def나 파라미터 이름인 `@name`이라 글자 그대로 출하됩니다. `{{@name}}`으로 쓰세요. |
| `E-UNSET-UNCOVERED` / `E-NONEXHAUSTIVE` | `<match>`가 `unset`, enum 멤버, 숫자 틈을 놓쳤습니다. 갈래나 `<otherwise>`를 추가하세요. |
| `E-TAG-INLINE-BODY` / `E-TAG-NOT-ONE-LINE` | 태그와 본문이 한 줄에 있거나, 태그가 여러 줄로 나뉘었습니다. |
| `E-BRANCH-ALL-GUARDED` / `E-HUB-NO-EXIT` | 메뉴가 빌 수 있거나, 허브가 끝나지 않을 수 있습니다. |
| `E-SET-TYPE` / `E-REF-TYPE` / `E-ATTR-TYPE` | 값의 타입이 자리에 맞지 않습니다. 디렉티브 안의 따옴표 친 `"@def"`나, 컴포넌트가 텍스트에 끼워 넣는 `string` 파라미터에 넘긴 `@def`가 흔한 원인입니다. |
| `E-ATTR-DEF-DYNAMIC` | 디렉티브 속성에 상태를 읽는 `@def`를 넣었습니다. 속성 값은 상수여야 하니 `<match>`로 나누세요. |
| `E-INTERP-DEF` | `{{@def}}`의 본문을 식 하나로 펼칠 수 없습니다(펼침 순환, 또는 `$`를 읽는 본문). |
| `E-ATTR-QUOTE` | 속성 값을 작은따옴표로 감쌌습니다. `"…"`를 쓰고, 값 안의 `"`는 `\"`로 쓰세요. |
| `E-DEF-DECL` | def 형식이 잘못되었습니다: 타입을 추론할 수 없거나, `type:` 없이 `params:`를 썼거나, 모르는 키가 있습니다. |
| `E-OBJECTIVE-MISSING-DONE` / `E-OBJECTIVE-QUEST-DONE` | 목표에 `done`이 없거나, `quest=`와 `done=`을 함께 썼습니다. |
| `E-GRAMMAR-NOT-ADMITTED` | 이 kind에서 허용되지 않는 구문입니다. 예: 엔트리 안의 `<branch>`, 퀘스트 안의 제목. |
| `E-BEAT-ATTR` | 비트 키 형식이 잘못되었거나 `on`이 없거나, `when`이 `scene.*`를 읽거나, 대상 없는 계기에 `target`을 썼거나 대상이 대상 도메인 밖에 있거나(비슷한 이름 제안과 함께), 엔트리의 `once`가 `run`이나 `user`가 아니거나, `also`가 bool이 아니거나 엔트리 또는 `select: all` / `sequence` 계기에 있습니다. `id`가 없거나 `id`에 `-`가 있거나 문서에 `id:`가 없는 `<beat>`, 형식이 잘못되었거나 `on`이 없는 목표의 `target=`도 `E-BEAT-ATTR`입니다. |
| `E-OCCASION-UNKNOWN` | 플러그인이 계기를 선언했는데 이 계기는 그중에 없습니다. |
| `W-BEAT-PRIORITY-TIE` | 한 `select: first` 계기에서 대상이 같거나 둘 다 없고 priority도 같은 두 비트의 `when`이 서로 배타적임을 증명할 수 없습니다. 승자는 파일 순서로 정해집니다. |
| `W-BEAT-ONCE-RUN-USER` | `once: run` 비트의 `when`이 user 등급 상태만 읽어서 런마다 다시 재생됩니다. |
| `W-QUEST-HANDLER-DEAD` | 결코 실패할 수 없는 퀘스트(`fail`도, `by=` 기한이 있는 필수 목표도, 실패할 수 있는 필수 하위 퀘스트도, 부모 퀘스트도 없음)에 `<on event="questFailed">`가 있습니다. |
| `W-STAGE-ABSENT` | 어떤 경로에서 퇴장했거나 `::bg` 장면 전환으로 자동으로 숨겨진 캐릭터를 줄이 무대에 세웁니다. 선택지와 `<match>` 갈래는 따로 따라가므로 한 갈래의 퇴장이 형제 갈래에서 경고를 내지 않습니다. 갈래가 다시 합쳐진 뒤에는 모든 갈래가 무대에 남겨 둔 캐릭터만 무대에 있습니다. |
| `E-BEAT-UNREACHABLE` / `E-ARM-DEAD` | 조건이 결코 성립할 수 없습니다. `check-project`는 팩트 질의도 판정합니다. 0.23.0부터는 한 `&&` 안의 모순(`run.n > 5 && run.n < 3`)도 잡습니다. `check-project --wip`에서는 아직 아무것도 만들어 내지 않는 관계 때문에만 죽은 가드가 경고입니다. |
| `E-ACCEPT-TARGET` | `::accept`가 없는 퀘스트나 `start`가 있는 퀘스트를 가리킵니다. |
| `E-CONN-UNKNOWN-NODE` | `visited('…')`나 `after`가 프로젝트에 없는 씬을 가리킵니다. |
| `E-CONN-EPISODE-ID-DUP` / `E-QUEST-ID-DUP` | 두 문서가 같은 씬 id나 퀘스트 id를 쓰거나, 번들 비트의 정식 `<doc>.<beat>` id가 씬 id와 같습니다. |
| `E-DUP-VOICEKEY` | 텍스트가 다른 줄들이 같은 `voiceKey`로 컴파일됩니다. 대개 `{speaker}-{code}` 템플릿으로 고정했을 때입니다. 기본값 `{prefix}.{speaker}-{code}`를 쓰거나 줄마다 다른 `code=`를 주세요. |
| `E-CAPABILITY-MISMATCH` | 프로젝트의 문서들이 서로 다른 기능 스냅샷으로 해석되어(다른 프로필이나 씬별 `plugins:`) 하나로 컴파일할 수 없습니다. |
| `E-TEST-LORE` | `*.test.yaml`이 `entry:`나 `entries:` 없이 로어 문서를 가리킵니다. 제시할 엔트리를 적으세요. |
| `E-TRACE-BEAT` | `lute trace --beat <id>`가 문서의 어떤 번들 비트도 가리키지 않습니다(또는 로어 문서가 아닙니다). `lute run --beat`는 같은 경우를 종료 코드 `2`로 거부합니다. |
| `W-REWARD-DOUBLE-CREDIT` | 퀘스트의 `<on>`이나 목표 본문이 보상 종류가 이미 `credits:`로 지정한 경로를 `::set`해서 두 번 지급됩니다. `::set`이나 보상 중 하나를 지우세요. |
| `W-FACT-GUARANTEED` | 팩트 가드가 모든 경로에서 항상 참이라 불필요합니다. |
| `W-BEAT-SHADOWED` | 항상 자격이 있고 소진되지 않는 앞선 비트가 매번 이깁니다. |
| `W-ENTRY-REF-UNKNOWN` | `entry.<id>.read`나 `entry.<id>.everRead`가 아무도 선언하지 않은 엔트리를 가리킵니다. |
| `E-LEGACY-CONTENT-SIGIL` · `W-WHEN-TEST-LITERAL` | 옛 문법입니다. `lute fix`가 고쳐 줍니다. |
| `E-PERSIST-REMOVED` | 선택지에서 `persist=`를 직접 지우세요. `into=`만으로 run 팩트가 기록됩니다. |

## 주의할 점

**`start`가 없는 퀘스트는 스스로 활성화되지 않습니다.** 수락형이라서, 씬이 `::accept{quest="id"}`를
실행하거나 목이나 테스트가 `accepts:`에 적을 때까지 `unset`으로 남습니다(플레이 스크립트는 `quests:`로
상태를 시드할 수 있습니다). `start`가 있는 퀘스트에 `::accept`를 쓰면 `E-ACCEPT-TARGET`입니다. 예외는
`quest=`로 지정된 자식으로, 부모와 함께 활성화됩니다.

**`once`는 씬과 엔트리에서 뜻이 다릅니다.** 씬 비트의 기본값은 `once: run`이라서, `once: false`로 쓰지
않으면 런마다 최대 한 번 재생됩니다. `once`가 없는 엔트리는 반복됩니다. `once="run"`은 새 런이
`entry.<id>.read`를 초기화할 때까지 엔트리를 소진시키고, `once="user"`는 영구히 소진시킵니다
(`entry.<id>.everRead`). `when`이 user 등급 상태만 읽는 `once: run` 비트는 런마다 다시 재생됩니다
(`W-BEAT-ONCE-RUN-USER`). 대개 `once: user`를 뜻한 것입니다.

**`after:`는 구조이고, 상태는 `when:`이 담당합니다.** `after:`는 `&&`와 `||`로 묶은 `visited`,
`completed`, `active`만 읽으며 씬 그래프에 쓰입니다. 비트는 둘 다 성립할 때만 자격이 있습니다.
`<quest>`의 `after=`는 그래프 메타데이터일 뿐 활성화를 늦추지 않습니다. 퀘스트를 씬에 묶으려면 조건을
`start`에 넣으세요: `start="visited('cafe.counter')"`.

**`::end`는 현재 씬이 아니라 `lute play` 진행 전체를 끝냅니다.** 그 스텝은 먼저 정산되고(퀘스트 진행과
계기의 `<objective on>` 판정) 나서 워크가 멈춥니다. 게임에 제어를 돌려줘야 하는 허브나 비트 씬은 마지막
줄에서 그냥 끝나면 됩니다.

**`visited()`는 세이브 전체에 걸칩니다.** 새 런에서도 지워지지 않습니다. 단일 파일 `check`는 이를 판정하지
않고, `check-project`는 id를 검증하며, `trace`와 `test`에는 `visited:` 목록이 필요하고, 세이브에서
시작하는 플레이 스크립트는 최상위 `visited:`에 적습니다.

**엔진 소유 상태는 콘텐츠가 아니라 하네스에서 옵니다.** `owner: engine` 경로에 `::set`을 쓰면 오류입니다.
`lute play`에서는 `engine:` 스텝으로, trace와 test에서는 목의 `state:`로 쓰세요:

```lute expect="E-ENGINE-OWNED-WRITE"
---
kind: scene
id: clock.cheat
state:
  run.day: { type: number, default: 1, owner: engine }
---

## Night

::set{run.day += 1}
@narrator: The day ends.
```

퀘스트 상태도 엔진이 쓰는 값이 아닙니다: `engine:` 스텝은 `quest.*`를 거부합니다. 세이브의 퀘스트 상태는
스크립트 최상위 `quests:`로 시드하세요.

**`::bg` 장면 전환은 모두를 무대에서 내립니다.** 자동으로 숨겨진 캐릭터는 퇴장한 것으로 기록되므로,
`::auto`로 다시 등장시키기 전의 대사는 `W-STAGE-ABSENT`입니다:

```lute check
---
kind: scene
id: dock.night
enums:
  action: { members: [fade-in-up, fade-out-down], exits: [fade-out-down] }
  anchor: { members: [left, center, right], default: center }
---

## Dock

::auto{character="mira" action="fade-in-up"}
@mira: Over here.
::bg{location="street" time="night"}
::auto{character="mira" action="fade-in-up"}
@mira: Keep walking.
```

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

**`lute check`는 프로젝트를 찾지만 다른 단일 파일 명령은 찾지 않습니다.** `lute check <file>`은 파일
위쪽에서 가장 가까운 `lute.project.yaml`을 적용합니다(stderr 알림이 그 경로를 알려 줍니다). `trace`,
`compile`, `context`, `test`는 `--project <dir>`를 줄 때만 프로젝트를 해석하며, 없으면 `defaults:`로
끌어올린 것과 플러그인이 선언한 것이 모두 빠집니다. `check-project`, `play`, `scenario`는 넘겨받은
디렉터리에서 프로젝트를 읽고, `lute test`는 `*.play.yaml`을 `--project`나 가장 가까운
`lute.project.yaml`의 프로젝트로 플레이합니다.

**`trace`, `test`, `play`는 도출합니다: 결론이 아니라 전제를 목으로 주세요.** 스키마의 `facts:` 시드를
불러오고, 목으로 준 팩트와 assert된 팩트 위에 Datalog 규칙을 적용합니다. `lute run`과 같습니다. 도출된
atom을 목으로 주어도 시드 하나로 받아들여집니다. 목으로 주지 않은 기본 팩트는 거짓입니다. trace가 판정할 수
없는 상태에 걸린 규칙 가드는 결론을 unknown으로 남기고 워크를 멈춥니다(종료 코드 3). `lute test`에서는
`expect: { exit: incomplete }`를 선언하지 않은 테스트가 이때 실패합니다. `derive: false`(목, 테스트, 플레이
스크립트 키)나 `--no-derive`는 0.21의 명시적 세계로 되돌립니다: 시드가 없고, 목으로 주지 않은 도출 atom은
unknown입니다.

**디렉티브 속성에는 상수 def를 따옴표 없이 참조합니다.** `::camera{zoom=@closeUp}`처럼 쓰세요. 따옴표 친
`zoom="@closeUp"`은 문자열 `@closeUp` 그대로입니다(숫자 속성이면 `E-ATTR-TYPE`). 상태를 읽는 def는
`E-ATTR-DEF-DYNAMIC`입니다.

**속성 값은 큰따옴표로 감쌉니다.** `label='"Hi."'`는 `E-ATTR-QUOTE`입니다. `label="\"Hi.\""`로 쓰면
레이블은 `"Hi."`가 됩니다.

**`<match>`에서 숫자는 실수입니다.** `is="1..9"` 다음에 `is="10.."`를 써도 `9.5`는 다뤄지지 않습니다.
`<otherwise>`를 두거나, 끝이 맞닿는 열린 범위를 쓰세요.

**0.23.0부터 검사기는 더 많은 조건이 거짓임을 증명합니다.** `&&` 전체에 걸쳐 경로별로 추론하므로
`run.n > 5 && run.n < 3`, `run.slot == 'a' && run.slot == 'b'`, `x && !x`는 결코 성립하지 않고, 경우들이
경로의 도메인을 모두 덮는 `||`는 항상 성립합니다. 0.22에서 깨끗하게 검사되던 프로젝트가 이제
`E-BEAT-UNREACHABLE`, `E-ENTRY-UNREACHABLE`, `E-ARM-DEAD`, `E-OBJECTIVE-UNSATISFIABLE`,
`W-BEAT-PRIORITY-TIE`를 보고할 수 있습니다. 실제 모순이니 조건을 고치세요. `unset`도 하나의 값으로 치므로,
기본값 없는 경로에서 `run.m > 5 && run.m < 3`은 판정되지 않고 `isSet(run.m) && run.m > 5 && run.m < 3`은
거짓입니다.

```lute expect="E-BEAT-UNREACHABLE"
---
kind: scene
id: late.shift
on: hubVisit
when: "run.day > 5 && run.day < 3"
state:
  run.day: { type: number, default: 1 }
---

## Late

@mira: You're early.
```

**`lute trace`는 실행된 `::next`에서 멈춥니다.** 트레이스는 점프를 보고한 뒤 끝나므로 대상 `::mark` 뒤의
콘텐츠는 보이지 않습니다. 컴파일된 산출물에 `lute run`을 쓰면 점프를 따라갑니다.
