---
title: 치트시트
description: "Lute 0.25.1(시계, 퀘스트 구조, 파티, 브리지 응답, 배타 관계, 공유 소진, 퀘스트 그래프 간선 포함)으로 글을 쓰는 동안 열어 두는 한 페이지: 모든 구문을 검사된 최소 스니펫으로 보여 줍니다(프로젝트 구성, 프론트매터, 대사, 선택지, match, 상태, CEL, 비트, 시계, 퀘스트, 로어, 컴포넌트, 타임라인). CLI 요약, 작가가 가장 자주 만나는 진단 코드, 주의할 점도 담았습니다."
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
  luteVersion: "0.23.1"
  uses: [world.schema.yaml]             # resolved against THIS file's directory
```

`defaults:`에는 `kind`, `character`, `season`, `episode`, `pov`, `luteVersion`, `contentLang`,
`uses`, `extends`, `components`, `extra`만 쓸 수 있습니다(그 밖의 키는 `E-DEFAULTS-KEY`). 문서가 어떤 키를
직접 쓰면 그 키의 기본값은 병합 없이 통째로 대체됩니다(`uses: []`는 "가져오기 없음"). 문서의 kind에서
허용되지 않는 기본값은 그 문서에는 적용되지 않습니다.

문서의 `components:`는 **그 문서의** 디렉터리를 기준으로, `defaults: components:`는 `defaults: uses:`처럼
**매니페스트의** 디렉터리를 기준으로 해석합니다. 그래서 같은 파일을 `scenes/`의 씬은
`components: [../components/greet.component.lute]`로, 매니페스트는
`defaults: { components: [components/greet.component.lute] }`로 씁니다.

0.22.0부터 기본 `voiceKey`에 `{prefix}`가 들어가므로 보이스 키는 프로젝트 전체에서 유일합니다. 0.21 키로
음성을 녹음해 둔 프로젝트는 `identity: { voiceKey: "{speaker}-{code}" }`로 고정하면 되고, 그러면 텍스트가
다른 줄들이 한 키에 모이는 곳마다 `E-DUP-VOICEKEY`가 납니다.

`world.schema.yaml`은 `---` 구분선이 없는 일반 YAML입니다. 씬은 `uses:`로 이 파일을 가져옵니다:

```yaml
state:                                   # scalar only: number | bool | string | enum
  run.pressure: { type: number, default: 0 }
  run.day:      { type: number, default: 1, owner: engine }   # content reads it; ::set is E-ENGINE-OWNED-WRITE
  run.slot:     { type: { enum: [morning, afternoon, night] }, default: morning, owner: engine }
  run.mood:     { type: { enum: [calm, tense] }, default: calm }
  run.rival:    { type: { enum: [kai, lee] } }       # no default: maybe-unset until set
  run.trust:    { type: number, default: { _: 0, vesna: 2 }, per: crew }   # 0.24.0: run.trust.vesna (2), run.trust.toma (0)
  run.today:    { type: { domain: weekday }, default: mon }     # {{run.today}} renders "Monday"
  user.runs:    { type: number, default: 0 }
  app.rating:   { type: { enum: [teen, adult] }, default: teen }
enums:                                   # content vocabulary: you declare every member
  emotion: [neutral, happy, worried]
  action:  { members: [fade-in-up, fade-out-down], exits: [fade-out-down] }
  anchor:  { members: [left, center, right], default: center }
  weekday: { members: [mon, tue], labels: { mon: Monday, tue: Tuesday } }   # labels: 0.24.0
entities:
  crew:  { members: [vesna, toma] }
  watch: { subsetOf: crew, members: [vesna] }   # 0.24.0 sub-kind: legal wherever a kind is
  topic: { members: [manifest, heading] }
relations:
  awake:    { args: [crew], tier: run }
  knows:    { args: [crew, topic], tier: run }
  can_halt: { args: [crew], derive: true }
  asleep:   { args: [crew], tier: run, excludes: [awake] }   # 0.25.0: never both on the same args (symmetric)
  hurt:     { args: [crew], reserved: true, changedOn: [dusk] }   # 0.25.0: the engine writes it only on `dusk`
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
clock:                                   # optional (0.24.0), one per project: see Clock below
  day: run.day                           # owner: engine
  slot: run.slot                         # optional, with slots: (owner: engine); omit both for a clock of whole days
  slots: [morning, afternoon, night]     # exactly the slot enum's members, in order
  raise: { slot: slotStart, dayEnd: dusk }   # optional: one occasion (= slot:), or any of slot / dayStart / dayEnd
  week: { length: 7, first: 0, labels: [Mon, Tue, Wed, Thu, Fri, Sat, Sun] }   # optional
cast:                                    # optional (0.23.0): once declared, any other speaker is E-CAST-UNKNOWN
  vesna: { name: Vesna, present: "holds(awake(vesna))", emotions: [neutral, worried] }   # 0.24.0 keys
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
번들 비트 어디서나 비슷한 이름을 제안하는 `E-CAST-UNKNOWN`이며, 0.24.0부터는 캐스트 밖의
`::auto{character}`와 `::camera{focus}`도 마찬가지입니다. 캐스트를 선언하지 않으면 어떤 화자 id든
받아들여집니다. 씬 프론트매터에는 `cast:`를 쓸 수 없습니다(`E-META-UNKNOWN-KEY`). `lute context`가 캐스트를
보여 줍니다. 캐스트 항목에는 `present:`(조건)와 `emotions:`도 둘 수 있습니다(0.24.0). 그 화자의 줄을 감싼
가드가 `present`를 함의하지 않으면 `W-CAST-ABSENT`이고, 화자의 `emotions:` 밖의 `emotion=`은
`E-BAD-ENUM`입니다. `{vo}` 줄은 제외됩니다(화자가 씬의 시간 밖에 있을 수 있음). `{os}` 줄은 검사합니다.
`{os}`는 씬 안에 있지만 화면 밖이라는 뜻이기 때문입니다. 항목에 `assume: true`를 두면 `present:` 안의 엔진
`reserved:` 관계에 대한 부정 `holds`를 참으로 읽으므로, `present: "holds(inParty(isolde)) && !holds(fell(isolde))"`에는
`inParty` 가드만 있으면 됩니다. 0.25.0부터 그 관계가 `changedOn: [battleEnd]`를 선언하면, `battleEnd`에 제시된
단위와 시나리오 그래프에서 그 뒤에 오는 모든 단위(`after:` / `after=` / `[start]` 간선)에서는 `assume`이 더 이상
그 관계를 덮지 않습니다. 그런 줄은 가드를 달 때까지 다시 경고합니다.

역시 0.24.0: `subsetOf:`는 멤버가 모두 부모 종류에 속해야 하는 하위 종류를 선언합니다(`E-ENTITY-KIND-SHAPE`).
`watch` 종류의 인자는 `crew`이기도 합니다. `per: <kind>`는 닫힌 종류의 멤버마다 경로를 하나씩
선언합니다(`run.trust.vesna`). 열린 종류나 모르는 종류는 `E-STATE-DECL`입니다. 맵 형태 `default:`는 멤버마다
값을 주고 나머지는 `_`로 채웁니다. 멤버가 아닌 키, 값도 `_`도 없는 멤버, `per:` 없는 맵 기본값은
`E-STATE-DECL`입니다. 긴 형태 enum의 `labels:`는
`{ domain: … }` 타입 경로를 `{{path}}`로 렌더링할 때 나오는 표시 이름이며, 멤버가 아닌 키의 라벨은
`E-ENUM-LABEL-NOT-MEMBER`입니다. `clock:` 블록은 [시계](#시계)에서 설명합니다.

→ [상태 스키마](/state/schemas/) · [가져오기](/language/imports/) · [콘텐츠 어휘](/language/vocabulary/) · [팩트와 Datalog](/state/facts-and-datalog/) · [시계](/language/clock/)

## kind별 프론트매터 키

| kind | 필수 | 그 kind에서만 쓰는 키 |
|---|---|---|
| `kind: scene` | `id:`, 또는 레거시 `character` + `season` + `episode` | `id`, `character`, `season`, `episode`, `episodeId`, `pov`, `after`, 비트 키 `on` / `target` / `when` / `priority` / `once` / `also` / `share`(0.25.0) |
| `kind: quest` | 본문에 `<quest>` 하나 이상 | `id` (선택, 묶음 이름) |
| `kind: lore` | 본문에 `<entry>`나 `<beat>` 하나 이상 | `id` (`<beat>`가 있으면 필수), `series` |
| 컴포넌트 (`kind:` 없음) | `component: <name>` | `component`, `params`, `effects`(0.24.0) |

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
@fixer{mono when="run.affection > 2"}: She remembered my order, too.
// a `//` line comment: the whole line is ignored
@mira{os}: Hold on!
@mira{as="???"}: ...who's there?
::sfx{sound="door bell"}
::vfx{type="whiteOut"}
::clear
::music{action="fade-out"}
::end{reason="closing"}
```

| 요소 | 규칙 |
|---|---|
| `@speaker{attrs}: text` | `@narrator`는 내레이션이고, 그 밖의 화자는 모두 대사입니다(`pov:` 화자도 마찬가지이며, `pov`는 설명용일 뿐입니다). `: ` 뒤의 텍스트는 줄 끝까지 그대로입니다. `cast:`가 선언되어 있으면 화자는 그 안에 있어야 합니다(`E-CAST-UNKNOWN`). 줄의 가드가 캐스트의 `present:`를 함의하지 않으면 `W-CAST-ABSENT`이고, 화자의 `emotions:` 밖의 `emotion=`은 `E-BAD-ENUM`입니다(0.24.0). |
| 줄 속성 | `code`, `emotion`, `variant`, `action`, `dialogMotion`, `as`(이름표 덮어쓰기), `when`(가드), `id`(점프 라벨). 따옴표로 감싼 값은 `&quot;` `&apos;` `&amp;` `&lt;` `&gt;` `&#NN;` `&#xHH;`를 해석합니다(0.24.0). 그 밖의 `&`는 그대로이며 `\"`도 여전히 됩니다. |
| 전달 플래그 | `{mono}` 속마음, `{os}` 화면 밖, `{vo}` 보이스오버. 한 줄에 하나까지이며 `@narrator`에는 쓸 수 없습니다. 플래그는 속성과 함께 쓸 수 있습니다: `{mono when="…"}`. |
| `{{…}}` | `{{userName}}`, 선언된 상태 경로, 또는 `{{@def}}`(산출물에 def 본문이 실리고 `lute run` / `lute play`가 그 값을 계산합니다). 값이 없을 수 있는 경로를 읽으면 `E-MAYBE-UNSET`입니다. 줄의 텍스트 전체가 `@name`이면 그 글자가 그대로 출하됩니다(`W-TEXT-LOOKS-LIKE-REF`). `{{@name}}`으로 쓰세요. 0.24.0: `{{run.visits:ordinal}}`은 `1st`, `2nd`, …로 렌더링되며(숫자 전용), `labels:`가 있는 enum 타입 경로는 라벨로 렌더링됩니다. 0.25.0: `{{run.day:ordinalWord}}`는 `first` … `twentieth`로 렌더링됩니다(엔진이 현지화하며, `lute play`는 스물을 넘으면 `21st` 같은 숫자로 돌아갑니다). 다른 힌트는 없습니다. |
| 샷 | 모든 콘텐츠는 `## 제목` 아래에 둡니다. `# 제목`만으로는 샷이 열리지 않습니다. |
| 디렉티브 | `::bg` `::music` `::sfx` `::auto`(등장, 포즈, 퇴장) `::camera` `::cut` `::vfx` `::video` `::end`, 그리고 `::clear`(0.24.0: 무대의 모두가 퇴장, 배경과 음악은 유지). 타이밍 키: `duration`, `delay`, `wait="true"`(대기). 캐스트가 선언되어 있으면 `::auto{character}`와 `::camera{focus}`도 그 안에 있어야 합니다(`E-CAST-UNKNOWN`). |
| 주석 | `// …`는 줄 끝까지이며, 그 줄에 홀로 있거나 디렉티브 뒤에 올 수 있습니다(`::set{run.n += 1} // why`). 그리고 `/* … */`. `<tag>` 뒤에 쓰면 `E-TAG-INLINE-BODY`이고, 대사 텍스트 안의 `//`는 글자 그대로입니다. |

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
| `::end{reason}` | 씬을 끝냅니다. `lute play`에서는 자신이 실행된 제시(또는 퀘스트 핸들러)만 끝내고, 플레이는 계속됩니다. 같은 본문에서 그 뒤의 콘텐츠는 `W-CODE-AFTER-END`입니다. |
| `::accept{quest}` | 수락형 퀘스트(`start`가 없는 퀘스트, 또는 `activate="accept"`인 자식)를 받아들입니다. `at="nextRun"`(0.24.0)은 다음 `newRun` 초기화 직후로 수락을 미루며, 그 밖의 `at`은 `E-ACCEPT-TARGET`입니다. [퀘스트](#퀘스트)를 보세요. |

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
  틈이 남습니다. 값이 없을 수 있는 주제에는 `is="unset"`이나 `<otherwise>`가 필요합니다. 갈래는 주제를
  좁혀 줍니다: `<when is="x">` 안에서는 주제에 값이 있고, `test` 없는 `is="unset"` 갈래 뒤의 갈래와
  `<otherwise>`는 값이 있는 것으로 읽습니다(`E-MAYBE-UNSET` 없음).
- 비트의 `when:`은 본문 전체에서 주제를 좁힙니다(0.24.0): `when: "run.verdict != 'undecided'"` 아래의
  `<match on="run.verdict">`에는 `undecided` 갈래가 필요 없고, 써 두면 `E-ARM-DEAD`입니다. 본문이 주제에
  쓰기를 하면 전체 도메인이 유지됩니다.
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

파티(0.24.0): 하위 종류, 엔티티별 상태, 더 풍부해진 규칙.

```lute check
---
kind: scene
id: camp.fire
state:
  run.day:      { type: number, default: 1 }
  run.approval: { type: number, default: 0, per: companion }   # run.approval.isolde, run.approval.corvin
entities:
  person:    { members: [isolde, corvin, hollis] }
  companion: { subsetOf: person, members: [isolde, corvin] }   # every member is a person
  place:     { members: [camp, ridge] }
relations:
  inParty: { args: [companion], tier: run }
  sawAt:   { args: [person, place], tier: run }
  loyal:   { args: [companion], derive: true }
  witness: { args: [person], derive: true }
defs:
  evenDay: "run.day % 2 == 0"
rules:
  - "loyal(P) :- inParty(P), cel(\"run.approval[P] >= 3\")"   # [P]: rule guards only
  - "witness(W) :- sawAt(W, _), cel(\"@evenDay\")"             # `_` in a body; a @def in a guard
---

## Camp

::assert{inParty(isolde)}
::set{run.approval.isolde += 2}
::set{run.approval.corvin += 1 when="run.day > 3"}
@isolde{when="holds(loyal(isolde))"}: I'm with you.
@hollis{when="countDistinct(sawAt(W, _), W) >= 2"}: Two of us saw it.
@hollis{when="holds(witness(hollis))"}: I was there.
```

- `per: companion`은 멤버마다 `run.approval.<member>`를 선언합니다. 콘텐츠는 멤버 이름으로 쓰고, 규칙의
  `cel()` 가드만 양의 원자가 묶은 변수로 이 묶음을 색인합니다(`run.approval[P]`). 그런 규칙은 멤버마다
  인스턴스 하나로 컴파일됩니다.
- 규칙 본문의 `_`는 새 익명 변수입니다(`not` 아래에서는 "그런 튜플이 아예 없음"). 규칙 머리
  (`E-DATALOG-PARSE`)와 비교식에서는 여전히 오류입니다.
- `countDistinct(sawAt(W, _), W)`는 한 위치의 서로 다른 값 개수를 세고, `count(…)`는 튜플 개수를 셉니다.
  `count`처럼 규칙 가드에는 쓸 수 없습니다.
- 규칙 가드는 `@def`를 부를 수 있습니다. 없는 def, 잘못된 인자 개수, `$`는 `E-RULE-GUARD-DEF`입니다.
  규칙 본문의 종류 원자(`companion(P)`)는 멤버십 검사입니다.
- `::set{… when="…"}`은 조건이 성립할 때만 씁니다(`lute play`는 `skip set … — when: false`로 출력).
  CEL 호출처럼 이름 붙인 관계(`has`, `holds`, `count`, `isSet`, `now`, …)는 `E-RELATION-RESERVED-NAME`입니다.

배타 관계(0.25.0): `excludes:`는 같은 인자에서 결코 함께 성립하지 않는 관계를 적습니다.

```lute check
---
kind: scene
id: tower.gallery
entities:
  person: { members: [elias, maren] }
relations:
  seen:      { args: [person], tier: run }
  seenAfter: { args: [person], derive: true, excludes: [fell] }   # symmetric: fell excludes seenAfter
  fell:      { args: [person], tier: run }
rules:
  - "seenAfter(P) :- seen(P)"
---

## Gallery

::assert{seen(elias)}
@maren{when="holds(seenAfter(elias))"}: He was on the stairs after the storm.
```

- 짝은 선언된 관계여야 하고, 인자 종류가 같아야 하며, 자기 자신일 수 없습니다(`E-RELATION-DECL`). IR의
  `RelationEntry.excludes`에는 대칭 폐포가 실립니다.
- `check-project`: `holds(seenAfter(x)) && holds(fell(x))`는 죽은 가드이고(`E-ARM-DEAD` / `E-BEAT-UNREACHABLE`),
  `holds(seenAfter(x))` 아래의 `!holds(fell(x))`는 `W-FACT-GUARANTEED`이며, 여기서 `::assert{fell(elias)}`는
  `E-FACT-EXCLUSIVE`(다른 쪽이 모든 경로에서 성립), 짝을 깨뜨릴 수밖에 없는 `fell(P) :- seenAfter(P)` 같은 규칙은
  `E-RULE-EXCLUSIVE`입니다.
- 둘 다 가능하기만 한 곳에서는 `lute play`가 쓰기 지점에서 `✗ exclusive: fell(elias) and seenAfter(elias) both
  hold`로 멈추고(종료 코드 1), `lute trace` / `lute test`는 그 지점에서 거부합니다(`E-FACT-EXCLUSIVE`).
- `reserved:` 관계는 엔진이 그 관계를 바꾸는 계기를 `changedOn: [<occasion>…]`으로 선언할 수 있습니다. 이것이
  캐스트의 `assume: true`를 좁힙니다([위의 스키마](#프로젝트-구성) 참고). `reserved`가 아닌 관계의 `changedOn`이나
  선언되지 않은 계기를 가리키는 `changedOn`은 `E-RELATION-DECL`입니다.

→ [상태 모델](/state/state-model/) · [팩트와 Datalog](/state/facts-and-datalog/)

## CEL 요약

| 네임스페이스 | 초기화 시점 | 콘텐츠가 쓸 수 있는가 |
|---|---|---|
| `scene.*` | 씬이 끝날 때 | 예 |
| `run.*` | 새 런에서 | 예 |
| `user.*` | 프로필 초기화 시 | 예 |
| `app.*` | 앱 삭제 시 | 아니요 |
| `quest.<id>.state`(항상 값이 있음: 퀘스트가 활성화되기 전에는 `unset`, 그 뒤로 `active` `complete` `failed`), `quest.<id>.activatedAt`, `quest.<id>.objectives.<o>.done` | 엔진. `tier="run"` 퀘스트는 새 런에서 `unset`으로 돌아감 | 아니요 |
| `quest.<id>.failedBy`(`unset` `fail` `by` `until` `cascade` `superseded`), `quest.<id>.objectives.<o>.failed`(0.24.0) | 엔진. run 등급 퀘스트와 함께 지워짐 | 아니요(`E-QUEST-RESERVED-WRITE`) |
| `clock.index`, `clock.weekday`, `clock.weekdayLabel`(0.24.0, 시계를 선언했을 때. `weekday*`는 `week:`가 있을 때) | 엔진: 시계의 날과 슬롯에서 계산 | 아니요(`E-QUEST-RESERVED-WRITE`) |
| `entry.<id>.read` | 엔진, run 등급 | 아니요 |
| `entry.<id>.everRead` | 엔진, user 등급: 처음 읽을 때 설정되고 새 런에서도 초기화되지 않음 | 아니요 |
| `owner: engine`으로 선언한 경로 | 그 네임스페이스를 따름 | 아니요(`E-ENGINE-OWNED-WRITE`) |
| `prev.run.<path>`(0.23.0) | 엔진: 런이 끝날 때의 `run.<path>` 스냅숏. 첫 런이 끝나기 전에는 값이 없음 | 아니요(`E-QUEST-RESERVED-WRITE`) |
| `scene.choices.<branch>`, `scene.visited.<hub>.<choice>` | 엔진 | 아니요 |
| `visited('<scene id>')`, `visited('<doc>.<beat>')` | 초기화되지 않음: 세이브 전체 | 아니요 |

| 연산자 | 함수와 참조 |
|---|---|
| `== != < <= > >=` · `&& \|\| !` · `+ - * /` · `%`(0.24.0: 정수 전용) · `c ? a : b` · `x in ['a', 'b']` · 문자열·숫자 리터럴 | `has(p)` / `isSet(p)`(값이 있는가) · `holds(rel(a, _))` · `count(rel(_)) >= n` · `countDistinct(rel(W, _), W)`(0.24.0) · `validAt(rel(a), quest.q.activatedAt)` · `visited('scene.id')` · `@def` / `@def(args)` · `$`(`<match>` 안에서만) |

쓸 수 없는 것: `size`, `matches`, `map`/`filter`/`exists`/`all`(`E-CEL-PROFILE`). 가드에서도 def
본문에서도 마찬가지입니다. `%`는 두 정수를 받으며, 숫자가 아닌 피연산자나 소수 리터럴은 `E-CEL-TYPE`입니다.
값이 없음은 문자열 `'unset'`이 아닙니다(`E-UNSET-LITERAL`). `!isSet(p)`나
`is="unset"`으로 확인하세요. 예외는 `quest.<id>.state`로, 여기서는 `unset`이 실제 멤버입니다.
`quest.q.state == 'unset'`으로 쓰세요. `isSet(quest.q.state)`는 항상 참입니다(`W-QUEST-STATE-ISSET`).
경로 세그먼트, def 이름, 파라미터 이름에는 `-`를 쓸 수 없습니다.

CEL이 들어가는 곳: `<match on>`, `<when test>`, 줄이나 선택지의 `when=`, `::set`의 우변과 `when=`,
`::next when`, 비트 `when:`, 엔트리 `when=`, 퀘스트 `start` / `fail`, 목표 `done` / `by` / `until` /
`when`, `<on when>`, `<reward when>`, 캐스트 항목의 `present:`. 디렉티브 속성에서 def 참조는 따옴표 없이
씁니다: `zoom="@closeUp"`이 아니라 `::camera{zoom=@closeUp}`입니다. 속성에는 def가 접히는 상수가 들어가므로, 위의 `zoom`처럼 상태를 읽는
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

<beat id="thanksCounter" on="talk" target="npc.mira" after="visited('cafe.talks.miraOrder')" once="user" share="miraThanks">
  @mira: Thanks for the tips, by the way.
</beat>

<beat id="thanksDoor" on="leave" once="user" share="miraThanks">
  @mira: Thanks for the tips! See you.
</beat>
```

번들 비트는 `id`, `on`, `target`, `title`, `when`, `priority`, `once`, `also`, 그리고 0.25.0부터 `after`와
`share`를 받고, 본문은 씬 본문(대사, branch, 허브, match, 디렉티브)입니다. 문서에는 `id:`가 있어야 하고, 비트
`id`는 `-`가 없는 식별자이며, 비트의 정식 id는 `<문서 id>.<비트 id>`(`cafe.talks.miraOrder`)입니다.
`lute play`, `presented:`, `visited('cafe.talks.miraOrder')`, `lute trace --beat`가 이 id를 씁니다. 씬
비트처럼 동작합니다: `once`의 기본값은 `run`이고, 제시되면 소진됩니다. 0.24.0부터는 씬이나 퀘스트가 번들 비트를
선행 조건으로 쓸 수 있고(`after: visited('cafe.talks.miraOrder')`), 0.25.0부터는 비트 자신의 `after="…"`가 씬의
`after:`와 같습니다: 자격 조건이자 시나리오 간선입니다. 비트 `when`의 `visited()`는 막기만 하고 간선을 그리지
않으므로, `lute scenario`는 그런 비트를 써야 할 `after=`와 함께 unanchored로 나열합니다. `title`은
`select: all` 메뉴에서 비트의 이름표가 됩니다. 정식 id가 씬 id와 같으면 `E-CONN-EPISODE-ID-DUP`입니다.

| 키 | 의미 |
|---|---|
| `on` | 응답하는 계기. 이 키가 씬을 비트로 만듭니다. `on` 없이 다른 비트 키를 쓰면 `E-BEAT-ATTR`입니다. |
| `target` | 선택, 점으로 구분한 id(`npc.vesna`). 계기가 그 대상에 대해 발생했을 때만 후보가 됩니다. 계기가 대상 도메인을 선언했다면 대상은 그 도메인의 `<prefix>.<member>`여야 합니다. `target:`이 없는 계기에서 엔트리의 `target=`은 메타데이터입니다(0.24.0): 엔트리는 모든 발생에 응답합니다. 씬이나 번들 비트의 `target`은 여전히 `E-BEAT-ATTR`입니다. |
| `when` | `run` / `user` / `app`, `quest.*`, `entry.*.read` / `entry.*.everRead`, 팩트, `visited()`에 대한 CEL. `scene.*`는 읽을 수 없습니다. 문자열을 비교할 때는 YAML 값을 큰따옴표로 감싸 CEL이 작은따옴표를 쓸 수 있게 하세요: `when: "run.slot == 'night' && user.runs >= 3"`. 두 층 모두 작은따옴표를 쓰면 `E-META-PARSE`입니다. |
| `priority` | 정수, 기본값 `0`. 높은 쪽이 이깁니다. |
| `once` | 씬: `run`(기본값), `user`(평생 한 번), `false`(반복 가능). 엔트리: `once="run"`(새 런이 `entry.<id>.read`를 초기화할 때까지) 또는 `once="user"`(`entry.<id>.everRead`가 설정되면 소진). 엔트리에 `once`가 없으면 반복됩니다. 시계를 선언했다면(0.24.0) `once: day` / `once: slot`(엔트리는 `once="day"` / `"slot"`)은 날이나 슬롯이 바뀔 때까지 소진 상태로 둡니다. 시계 없이 쓰면 `E-BEAT-ATTR`입니다. |
| `also` | 0.23.0. `select: first` 계기의 씬(`also: true`)과 번들 비트(`also`): 승자 뒤에, 또는 주 비트가 하나도 자격이 없을 때는 혼자 제시되며, 승자를 대신하지 않습니다. 엔트리에 쓰거나 `select: all` / `sequence` 계기에 쓰면 `E-BEAT-ATTR`입니다. `W-BEAT-SHADOWED`와 `W-BEAT-PRIORITY-TIE`는 `also` 비트를 무시합니다. |
| `share` | 0.25.0. 씬(`share:`), 엔트리와 번들 비트(`share=`): 여러 곳에서 이야기되는 한 사건을 위한 프로젝트 전체의 키입니다. 키의 어느 비트든 제시되면(엔트리는 읽히면) 그 키의 모든 비트가 `once` 기간 동안 소진됩니다(`lute play`: `` once: user — `share: miraThanks` already spent … by cafe.talks.thanksCounter ``). `false`가 아닌 `once`를 함께 써야 하고, 한 키의 모든 비트는 같은 `once`를 선언해야 합니다. 그렇지 않으면 `E-BEAT-ATTR`입니다. `lute beats`는 `user, share miraThanks`로 보여 줍니다. |

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
  cast: cast/                           # cast: { <id>: { name, present?, emotions? } }, like a schema's cast:
```

```yaml
# plugins/game.occasions/occasions/game.yaml
occasions:
  hubVisit: { select: first }
  talk:     { select: first, target: { prefix: npc, entity: crew } }   # or `target: true`: any dotted id
  greet:    { select: first, target: { prefix: npc, entity: crew, members: [mira, vesna] } }   # only these members (0.23.1)
  runEnd:   { select: first, judge: before }   # 0.24.0: judge on="runEnd" objectives before the beats
  evening:  { select: sequence }        # every eligible beat, in selection order (0.23.0)
  inbox:    { select: all, description: Letters waiting at the fountain }
```

```yaml
# plugins/game.occasions/events/game.yaml: for <on event> and a play's `event:` step
events:
  - name: combatEnd
  - name: talk                          # same name as an occasion: <on event="talk" target="npc.mira">
```

계기를 선언하는 플러그인이 없으면 어떤 식별자든 받아들여집니다. 플러그인이 계기를 선언한 뒤에는 모르는
계기가 `E-OCCASION-UNKNOWN`이 되고, `target`을 선언하지 않은 계기에 `target`을 쓰면 `E-BEAT-ATTR`입니다.
대상 도메인 `{ prefix, entity }`는 그 `entities:` 종류의 멤버마다 `<prefix>.<member>`를 허용하므로(`open:`
종류라면 어떤 멤버든), `target: npc.vesan`은 비슷한 이름을 제안하는 `E-BEAT-ATTR`입니다.

플러그인 매니페스트(0.24.0):

- 모든 내보내기 파일은 모르는 키를 거부합니다. `{ selct: all }`은 조용히 `select: first`가 되지 않고,
  비슷한 이름을 제안하는 `E-PLUGIN-PARSE`입니다. 쉼표가 든 flow-map 값은 따옴표로 감싸세요.
- 디렉티브의 `lower:`는 선택입니다. 없으면 일반 `kind: "plugin"` 패스스루로 컴파일됩니다.
  `{ kind: builtin, name }`은 코어 훅(`autoStage`, `cameraTransform`, `clearStage`, `end`, `mark`, `next`)을
  가리켜야 하며, 그렇지 않으면 `E-PLUGIN-PARSE`입니다.
- `judge: before`는 비트를 고르기 전에 그 계기의 `on=` 목표를 판정하고 퀘스트를 정산하므로, 그 계기의
  에필로그가 퀘스트가 어떻게 끝났는지 읽을 수 있습니다. 옮겨지는 것은 판정뿐이며, 그 발생이 응답하는 `<on>`
  핸들러 본문은 여전히 비트 뒤에 실행됩니다. 기본값은 `after`입니다.
- `cast` 내보내기 항목도 스키마처럼 `present:`, `emotions:`, `assume:`을 받습니다.

→ [비트](/language/beats/) · [스토리 플레이](/tooling/play/) · [플러그인 매니페스트](/plugins/manifests/)

## 시계

스키마의 `clock:`(0.24.0, 위의 `world.schema.yaml` 참고)은 엔진 소유의 날과 슬롯에 순서를 줍니다. 아래
비트는 시계를 읽으므로 그 스키마가 있는 프로젝트 안에서만 검사를 통과합니다:

```lute unverified="reads clock.*, which exists only with a schema clock: (world.schema.yaml above); checked clean in a scratch project with that schema"
---
kind: scene
id: cafe.open
on: slotStart
once: day
when: "clock.weekday < 5"
---

## Open

@mira: {{clock.weekdayLabel}} again. That's slot {{clock.index}} of the story.
```

- `day`(number)는 `owner: engine` 경로여야 합니다. `slot`(enum, 역시 `owner: engine`)과 `slots`(그 멤버를
  순서대로)는 선택이지만 둘을 함께 써야 하며, 둘 다 없으면 날 단위로만 세는 시계입니다. 잘못된 시계나 두 번째
  시계, 콘텐츠 소유 경로, `slot` 없는 `slots`, 모르는 `raise` 계기는 `E-CLOCK-DECL`입니다.
- 조건과 보간 어디서나 읽을 수 있는 읽기 전용 경로: `clock.index`(`(day-1) * len(slots) + slotIndex`,
  단조 증가, 날 시계에서는 `day-1`), `clock.weekday`(정수 `0..length-1`이므로 `is="0..5"` + `is="6"`은
  빠짐없고 `is="7"`은 `E-WHEN-LITERAL-DOMAIN`), `clock.weekdayLabel`(`week.labels`의 enum. 둘 다 `week:`
  필요). 여기에 `::set`하면 `E-QUEST-RESERVED-WRITE`입니다.
- 씬과 번들 비트는 `once: day` / `once: slot`, 엔트리는 `once="day"` / `"slot"`.
- `lute play`는 `advance: slot | day | <n>` 스텝으로 시계를 옮기고(뒤로는 못 가며, `advance: day`는 다음
  날 첫 슬롯에 도착) 퀘스트를 정산합니다. `raise:`는 계기 하나이거나 맵입니다: `dayEnd`는 자정을 넘을 때마다
  (`advance: <n>`은 먼저 그날의 마지막 슬롯까지 감), `dayStart`는 다음 날 첫 슬롯에서, `slot`(단일 형태와
  같음)은 시계가 멈춘 곳에서 한 번 발생하며, 각각 자기 머리글로 출력됩니다
  (`── step 1 · day 2 (Tue) morning · dawn`). `lute calendar --axis clock=1..3`은 그 날들의 모든 슬롯을
  훑고, `--occasion dusk@clock.day`는 `dusk`를 하루에 한 번 평가합니다.

함께 나온 기능으로, 시계가 없어도 쓸 수 있습니다: enum 라벨, 정수 `%`, 가드 달린 `::set`, `:ordinal` 힌트.

```lute check
---
kind: scene
id: diner.payday
state:
  run.day:    { type: number, default: 1 }
  run.visits: { type: number, default: 1 }
  run.tab:    { type: number, default: 0 }
  run.today:  { type: { domain: weekday }, default: fri }
enums:
  weekday: { members: [mon, fri], labels: { mon: Monday, fri: Friday } }
---

## Payday

@mira: Happy {{run.today}}! Your {{run.visits:ordinal}} visit.
::set{run.tab = 0 when="run.day % 7 == 0"}
@mira{when="run.day % 7 == 0"}: Tab's cleared.
```

`lute trace --state run.visits=3`은 `Happy Friday! Your 3rd visit.`을 출력합니다.

→ [시계](/language/clock/) · [스토리 플레이](/tooling/play/)

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
| `start=` | 성립하면 퀘스트를 활성화합니다(`unset` → `active`). `start`가 없으면 수락형입니다: 씬이 `::accept{quest="…"}`를 실행하거나 목(mock)이 수락할 때까지 `unset`으로 남습니다. 플레이 스크립트는 `quests:`로 세이브의 상태를 시드합니다. 어떤 `::accept`도 이름을 부르지 않는 수락형 퀘스트는 `W-QUEST-NEVER-ACCEPTED`입니다(0.24.0, `check-project`). 0.25.0부터 `accepts:` 목은 더 이상 치지 않습니다. |
| `accept="external"` | 0.25.0. 엔진이 문서 밖(퀘스트 게시판, 메뉴)에서 퀘스트를 수락합니다. `W-QUEST-NEVER-ACCEPTED`를 잠재웁니다. `start`와 함께 쓰면 `E-ATTR-TYPE`, 부모와 함께 활성화되는 자식에 쓰면 `E-ACCEPT-TARGET`입니다(`activate="accept"`를 더하세요). IR: `QuestCmd.accept`. |
| `fail=` | `active` → `failed`. 완료 조건과 동시에 성립하면 실패가 이깁니다. |
| `after=` | 씬 그래프를 위한 구조적 선행 조건: `&&` / `\|\|`로 묶은 `visited` / `completed` / `active`. 활성화를 막지 않으며, 활성화는 `start`가 정합니다. 씬은 프론트매터에 `after:`로 씁니다. 이것이 없으면 퀘스트는 자신의 `::accept`, 부모(`[subquest]`, 0.25.0), 그리고 `visited(…)` / `entry.X.everRead` / `quest.Y.state == …`를 읽는 `start` 연언항(`[start]`, 0.25.0)에 고정됩니다. |
| `tier="run"` | 새 런에서 퀘스트가 `unset`으로 돌아가고 목표도 모두 미완료가 됩니다. 기본값 `tier="user"`는 런이 바뀌어도 상태를 유지합니다. 하위 퀘스트의 등급은 부모와 같아야 합니다(`E-QUEST-TIER-MIX`). |
| `<objective done>` | `done`은 필수입니다(`E-OBJECTIVE-MISSING-DONE`). `optional`이 아닌 목표가 모두 완료되면 퀘스트가 완료됩니다. 완료는 되돌려지지 않으며 본문은 한 번만 재생됩니다. |
| `on="runEnd"` | 퀘스트가 활성인 동안 그 계기가 발생했을 때만 `done`을 판정합니다. 계기를 발생시키면 먼저 같은 이름으로 선언된 월드 이벤트의 핸들러(`<on event="runEnd">`)가 실행되고, 그다음 목표를 판정합니다. |
| `by=` | 기한이며, 0.24.0부터는 시점입니다: `on=`이 있든 없든 매 정산마다 판정합니다. 목표가 완료되지 않은 동안 처음으로 성립하면 목표는 영구히 실패하고, 필수 목표가 실패하면 퀘스트도 실패합니다(`failed` 보상, `questFailed`). 같은 정산에서는 `done`이 이기고, `on=` 목표의 계기를 발생시키는 스텝에서는 `done`을 먼저 판정합니다. `done`이 `by`를 함의하는 `on=` 목표는 그 스텝에서 둘이 함께 성립하지 않는 한 실패하며, `W-DEADLINE-BEFORE-DONE`이 `until=`을 제안합니다. |
| `until=`(0.24.0) | 장소에 묶인 기한(0.23.1에서 `on=` 목표의 `by`)입니다. 목표의 계기(와 `target`)가 발생할 때만, `done` 다음에 판정합니다. `on=` 없이 쓰면 `E-BEAT-ATTR`입니다. |
| `on="talk" target="npc.mira"`(0.23.0) | 계기가 그 대상에 대해 발생했을 때만 판정하며, 비트의 대상처럼 검사합니다(`E-BEAT-ATTR`). 도구에서는 `talk@npc.mira`로 발생시키고, `lute play`에서는 `target:`이 있는 스텝이 판정합니다. |
| 목표의 `when=` | 표시 여부만 정합니다. 완료에는 영향을 주지 않습니다. |
| `quest="child"` | 하위 퀘스트: 자식이 완료되면 완료되고, 필수 자식이 실패하면 부모도 실패합니다. `done=`과 함께 쓸 수 없습니다(`E-OBJECTIVE-QUEST-DONE`). `start`가 없는 자식은 부모와 함께 활성화되지만, `activate="accept"`(0.24.0)를 선언하면 부모가 활성인 동안 `::accept`를 기다립니다. 부모와 함께 활성화되는 자식을 `::accept`하면 `E-ACCEPT-TARGET`입니다. |
| `complete="any"`(0.24.0) | 필수 목표 중 하나만 완료되어도 퀘스트가 완료되고, 아직 활성인 나머지 자식은 `failedBy` `superseded`로 실패합니다. 대안 하나가 실패해도 퀘스트는 열려 있습니다. 기본값은 `complete="all"`입니다. |
| `quest.<id>.failedBy`(0.24.0) | 실패한 이유: 실패 전에는 `unset`, 그 뒤로 `fail`, `by`, `until`, `cascade`, `superseded`. `quest.<id>.objectives.<o>.failed`는 `by`/`until`이 그 목표를 실패시키면 `true`입니다. `lute play`는 `quest X -> failed (by)`로 출력합니다. |
| `<reward kind amount target when on/>` | 엔진이 지급하는 데이터입니다. 콘텐츠는 보상을 읽을 수 없으므로 같은 재화를 `<on>` 핸들러에서 `::set`으로 또 올리지 마세요. 두 번 지급됩니다. `amount`는 정수나 범위 `N..M`입니다. `on="failed"`는 실패 시에 지급합니다. `lute run`, `lute play`, `lute trace`, `lute test`는 `grant`를 출력하고, 보상 종류가 `credits:`(아래)를 선언했다면 스칼라 금액을 그 경로에 더합니다. 그 퀘스트의 `<on>`이나 목표 본문에서 같은 경로를 `::set`하면 `W-REWARD-DOUBLE-CREDIT`입니다. |
| `<on event>` | `questActive`, `questComplete`, `questFailed`, 또는 플러그인의 월드 이벤트. `when=`으로 가드할 수 있습니다. `target=`(0.24.0)을 쓰면 같은 이름의 계기가 그 대상에 대해 발생할 때만 실행됩니다(일반 `event:` 스텝에서는 실행되지 않음). 결코 실패할 수 없는 퀘스트(`fail`도, `by=` 기한이 있는 필수 목표도, 실패할 수 있는 필수 하위 퀘스트 목표도, 부모 퀘스트도 없음)의 `questFailed` 핸들러는 `W-QUEST-HANDLER-DEAD`입니다. |

퀘스트 문서에는 `#`/`##` 제목, `<hub>`, `<timeline>`이 없습니다. 다른 문서는
`<match on="quest.regular.state">`나 `when="quest.regular.state == 'complete'"`로 퀘스트를 읽습니다.
상태는 항상 값이 있으므로 `quest.regular.state == 'unset'`은 "아직 받지 않음"을 뜻합니다.

퀘스트 구조(0.24.0): 대안, 대화에서 받는 하위 퀘스트, 두 종류의 기한.

```lute check
---
kind: quest
id: keep.quests
state:
  run.day:   { type: number, default: 1 }
  run.gold:  { type: number, default: 0 }
  run.freed: { type: bool, default: false }
  run.lamp:  { type: bool, default: false }
---

<quest id="rescue" title="Free the prisoner" start="true" complete="any">
  <objective id="force" title="Storm the keep" quest="storm"/>
  <objective id="guile" title="Bribe the guard" quest="bribe"/>
  <on event="questComplete">
    @narrator{when="quest.storm.failedBy == 'superseded'"}: No blood was spilled.
  </on>
</quest>

<quest id="storm" title="Storm the keep" activate="accept">
  <objective id="breach" title="Breach the gate" done="run.freed" by="run.day > 3"/>
</quest>

<quest id="bribe" title="Bribe the guard" activate="accept">
  <objective id="pay" title="Pay him at his post" on="talk" target="npc.guard" done="run.gold >= 10" until="run.day > 2"/>
</quest>

<quest id="lamp" title="Light the lamp">
  <objective id="light" title="Light it" done="run.lamp" by="run.day > 2"/>
  <on event="questFailed">
    @narrator{when="quest.lamp.objectives.light.failed"}: The night came first.
  </on>
</quest>
```

씬의 `::accept{quest="storm"}`은 자식을 받아들입니다. `::accept{quest="lamp" at="nextRun"}`은 다음
`newRun` 초기화 직후로 수락을 미루므로, 런 사이 허브에서 받은 run 등급 퀘스트가 초기화를 견딥니다.
`after=`가 없는 수락형 퀘스트는 그것을 `::accept`하는 모든 씬, 번들 비트, 퀘스트 본문에 고정됩니다
(`lute scenario`가 `accept` 간선을 그립니다).

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
  `priority`, `once`(`run` 또는 `user`, 시계가 있으면 `day` / `slot`도, 비트에만). 여러 파일에 걸친 시리즈에는 `series`와 `order`를 씁니다.
  문서 수준의 `series:`는 파일 안의 위치로 엔트리 순서를 정하며, 그런 문서에서 엔트리별 `series=`/`order=`는
  `E-ENTRY-ATTR`입니다.
- 엔트리 본문에는 콘텐츠 줄, `<match>`, `::set`, `::assert`, `::retract`만 둘 수 있습니다. `<branch>`,
  디렉티브, 제목을 비롯한 그 밖의 것은 `E-GRAMMAR-NOT-ADMITTED`입니다.
- 효과는 한 런에서 처음 읽을 때만 적용됩니다. 그 뒤로 `entry.<id>.read`는 `true`이며 어느 문서에서나 읽을
  수 있습니다. run 등급이라 새 런에서 초기화됩니다. `entry.<id>.everRead`는 그 user 등급 짝으로, 처음 읽을
  때 설정되고 새 런에서도 초기화되지 않습니다. `entry.X.read`를 가정하는 가드 아래에서 `check-project`는
  X가 모든 경로에서 assert하는 팩트를 압니다(0.24.0). 그래서 거기서 중복된 `holds(…)`는
  `W-FACT-GUARANTEED`이고, `everRead`는 `tier: user` / `app` 팩트만 셉니다.
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

`effects: true`(0.24.0)를 선언한 컴포넌트는 `::set`, `::assert`, `::retract`를 쓸 수 있고, 팩트 원자에
파라미터를 쓸 수 있습니다(`::assert{gifted(@who, @item)}`. `::use`마다 인자로 묶이며, 상수가 아닌 인자는
`E-COMPONENT-ARG`, 컴포넌트 밖 팩트의 `@param`은 `E-FACT-DOMAIN`). 쓰기는
`::use`마다 호스트의 스키마로 검사되고 `::use`가 놓인 자리에 컴파일됩니다. `speaker` 파라미터는 캐스트
id를 받으며(선언된 캐스트 밖이면 `E-CAST-UNKNOWN`), `{{@who}}`는 캐스트의 `name`을 렌더링하고
`<match on="@who">`와 속성은 id를 봅니다.

```lute check
---
component: praise
effects: true
params:
  who: speaker
  delta: number
entities:                                  # declared here only so this file checks alone:
  companion: { members: [isolde, corvin] } # at `::use` the host's schema decides
state:
  run.approval: { type: number, default: 0, per: companion }
---

## Praise

@narrator: {{@who}} approves.
<match on="@who">
  <when is="isolde">
    ::set{run.approval.isolde += @delta}
  </when>
  <otherwise>
    ::set{run.approval.corvin += @delta}
  </otherwise>
</match>
```

본문의 가드와 match 대상은 여전히 상태를 읽을 수 없고(`E-COMPONENT-STATE`), `effects: true` 없이 쓰기를
하면 `E-COMPONENT-BODY`입니다.

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
| `lute trace <file> [--mock m.yaml] [--state P=V] [--fact "r(a)"] [--choose id=c[,c]] [--event e] [--accept q] [--occasion o[@target]] [--entry id \| --beat id] [--no-derive] [--expand]` | 프로젝트의 시드 팩트와 규칙을 적용한 채 소스를 목에 맞춰 미리 봅니다. 종료 코드 `3`은 판정할 수 없는 가드를 만났다는 뜻입니다. `--occasion talk@npc.mira`는 대상에 대해 계기를 발생시킵니다(0.23.0). `--beat`는 번들 비트 하나를 로컬 id나 정식 id로 제시합니다(없는 id면 `E-TRACE-BEAT`). `@def`는 쓴 그대로 출력되고(`<match @weekday>`), `--expand`(0.24.0)는 펼친 식을 출력합니다. `--accept`는 `activate="accept"` 자식도 받습니다. |
| `lute run <artifact> [--mock m.yaml] [--occasion o[@target]] [--entry id \| --beat id]` | 컴파일된 산출물을 엔진처럼 실행합니다. 로어 산출물에는 `--entry`와 `--beat`(번들 비트의 정식 id, 모호하지 않으면 로컬 id) 중 정확히 하나가 필요합니다. 목의 `bridges:`가 플러그인 호출에 응답합니다(0.24.0). |
| `lute play <dir> --script p.play.yaml [--json] [--ir] [--explain <atom>] [--no-derive]` | 프로젝트 전체에 계기를 발생시키며 퀘스트를 진행합니다. `expect:`가 어긋나면 종료 코드 `1`입니다. 연출은 작성한 그대로 출력되고, `--ir`은 대신 로워링된 레코드를 주입된 것까지 표시해 출력합니다. `--explain`(반복 가능)은 플레이가 끝난 뒤 ground atom의 도출 트리를, 성립하지 않으면 그것을 결론 낼 수 있는 규칙마다 실패한 전제를 출력합니다. 0.24.0부터 assert된 잎은 출처를 밝힙니다(``asserted by scene `cafe.open`, step 1``). |
| `lute test [<dir> \| <file>] [--project <dir>] [--coverage] [--no-derive]` | 모든 `*.test.yaml`과, `expect:`가 있는 모든 `*.play.yaml`을 실행합니다. 파일 하나를 주면 그 테스트나 플레이만 실행합니다. `--project`가 없으면 가장 가까운 `lute.project.yaml`을 기준으로 해석합니다(stderr에 알림). 미완료로 끝난 워크는 실패하고, `file:`이 없는 테스트도 스위트를 멈추지 않고 실패 하나(`E-TEST-FILE`)로 남습니다. `--coverage`는 `--project`나 가장 가까운 `lute.project.yaml`의 프로젝트에서 어떤 테스트도 트레이스하지 않고 어떤 플레이도 제시하지 않은 문서를 나열합니다. 0.24.0부터 `advance:` 스텝의 발생이 제시한 문서도 셈에 들고, 머리글은 `coverage over N traced path(s) and M play(s) (plays count toward documents presented only, not branches or arms):`입니다. |
| `lute scenario <dir> [reach <node> \| envelope <node> \| knowledge [--for <node>]] [--format text\|json\|dot]` | `after:` 그래프, 도달 가능성, 보장되는 상태와 팩트. 노드는 씬 id, `quest:<id>`, 또는 번들 비트의 정식 id입니다(그대로 또는 `beat:<doc>.<beat>`, 간선 없는 진입 노드로 그려짐). `knowledge`(0.23.0)는 팩트 가드가 있는 비트, 엔트리, 목표마다 질의하는 관계를 찾고, 각 관계를 규칙을 거슬러 그것을 만드는 쪽까지 추적합니다: assert하는 문서, 시드 팩트, 엔진(`reserved`), 또는 만드는 쪽 없음. 부정 전제를 깨뜨릴 수 있는 팩트도 알려 줍니다. `--for`에는 엔트리 id나 `<quest>.<objective>`도 줄 수 있습니다. 0.24.0부터 모든 가드 자리를 다루고, 종류 원자를 멤버십으로 읽으며(``suitor(sol) — entity kind `suitor`; sol is a member``), 규칙의 `cel()` 전제가 읽는 것을 밝힙니다. |
| `lute beats <dir> [--occasion o] [--target t] [--json] [--expand]` | 0.23.0. 계기별(대상별) 비트 사다리를 선택 순서대로 보여 줍니다: priority, `once`(번들 비트의 `day` / `slot` 포함), `also`, `after:`, `when`(`@def`는 쓴 그대로, `--expand`면 펼침), 제목, 그리고 `check-project`의 판정(도달 불가, 가려짐, 동점, once-run-user). 프로젝트가 깨끗하게 검사되지 않아도 됩니다. |
| `lute calendar <dir> [--axis run.day=1..7] [--axis quest.q.state=unset,active] [--axis 'holds(awake(toma))=true,false'] [--axis "visited('cafe.counter')=true,false"] [--axis clock=1..3] [--occasion o[@target \| @clock.day[,clock.slot=night]]] [--target t] [--facts <relation>] [--script p.play.yaml [--until <step \| label>]] [--where <cel>] [--json \| --csv]` | 0.23.0. 축들의 곱의 모든 칸(첫 축이 가장 느리게 바뀜)에서 계기별로 play와 같은 자격 판정을 보여 줍니다: 승자나 제시 목록, 가려진 자격 있는 비트 `+N`, 판정할 수 없는 칸 `?`, 마지막으로 어느 칸에서도 자격이 없는 비트와 (0.24.0) 어딘가에서 자격은 있었지만 한 번도 제시되지 않은 비트. 스크립트의 세이브에서 그 스텝을 재생한 상태(`--until`까지)나 선언된 기본값에서 시작합니다. `--where`는 조건이 성립하지 않는 칸을 뺍니다. 대상 있는 계기는 비트가 이름을 붙인 대상마다 열 하나를 받습니다. 0.24.0: `clock[=d1..d2]`는 날 × 슬롯을 시계 순서로 펼치고, `visited()` 축은 id를 세이브에 넣거나 뺍니다. `--occasion dusk@clock.day`(또는 `@run.day,run.slot=night`, 바뀌는 어느 경로든)는 그 계기를 그 축의 값마다 한 번 평가하고 나머지 칸은 비웁니다. `--facts at`은 칸마다 누가 어디 있는지 보여 줍니다. |
| `lute lore <dir>` | 대상별·시리즈별 엔트리와 비트, 그리고 그것이 드러내는 팩트. |
| `lute context <file> [--project <dir>]` | 여기서 쓸 수 있는 모든 것: 디렉티브(내장 포함), 어휘, 상태(`owner: engine` 표시), def, 등급과 `reserved` 여부를 담은 관계, 대상 도메인을 담은 계기, 캐스트, 컴포넌트 시그니처, 모든 씬·퀘스트·엔트리 id. |
| `lute lint [<path>] [--config lute.lint.yaml]` | 프로젝트별로 설정하는 권고성 편집 린트(`L-*`). 선형 VN 지표는 비트, 컴포넌트, 퀘스트, 로어를 건너뜁니다. |
| `lute doctor [<dir>]` | 툴체인과 프로젝트 설정: 버전, 활성 플러그인, 계기별로 응답하는 비트 수, 플레이 스크립트와 테스트, `PATH`의 `lute-lsp`가 이 버전인지, (0.24.0) 실행 중인 `lute` 옆의 빌드와 같은지(`lute-lsp beside lute`), 실행 중인 `lute-lsp`가 낡았는지(에디터 재시작). |
| `lute new scene\|quest\|lore\|schema <name> [--dir <dir>]` · `lute init <dir> [--template minimal\|investigation\|beats]` | 문서나 프로젝트의 뼈대를 만듭니다. 새 문서에는 `id:`가 들어가고 `defaults:`가 채워 주는 것은 빠집니다. `lute new scene <name> --on <occasion> [--target <prefix>.<member>]`은 계기와 대상을 프로젝트에 맞춰 검사한 뒤 비트를 씁니다. |

trace 목(`--mock`). 테스트의 목 키도 같은 형식입니다:

```yaml
# mocks/counter.yaml: lute trace scenes/counter.lute --project . --mock mocks/counter.yaml
file: ../scenes/counter.lute               # required under mocks/, which check-project validates
state:  { run.tip: 5 }                     # path: literal
facts:  ["knows(vesna, manifest)"]         # base facts, on top of the schema's facts: seeds
choose: { greet: tip, chat: [coffee, leave] }   # branch: choice; hub: its visit order
events: [combatEnd]                        # world events, for <on event>
# bridges: { check: [ { passed: true, margin: 3 } ] }   # 0.24.0: plugin-call answers by tag, in call order
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
  facts: ["regular(mira)"]                        # hold at the end, after derivation
  notFacts: ["banned(mira)"]
  transcriptContains: ["@narrator: You are a regular now."]   # lines that played, as `@speaker: text`
  transcriptLacks: ["You are no longer welcome."]
  exit: complete                                  # complete | incomplete; without it, incomplete fails
```

```yaml
# tests/counter.test.yaml
file: ../scenes/counter.lute
choose: { greet: wave, chat: [coffee, cup, leave] }
expect:
  offered: { greet: [wave, tip], chat: [coffee, cup, leave] }   # exact sets: flirt's `when` failed
  accepts: [lostCup]                    # 0.24.0: the quests this walk's ::accept took, as a set
```

```yaml
# tests/barks.test.yaml: a lore test names the entries it presents
file: ../lore/barks.lute
entries: [vesnaFirst, vesnaBack]        # in order, read flags set between; or `entry: <id>`
state: { user.runs: 3 }
expect:
  transcriptContains: ["So you are the new one.", "Back again?"]
  eligible: { vesnaFirst: true, vesnaBark: true }   # `when` verdicts; an unpresented key is judged alone
```

```yaml
# tests/order.test.yaml: a bundle beat, by its bare or canonical id
file: ../lore/talks.lute
beat: miraOrder                         # or cafe.talks.miraOrder
state: { run.tips: 3 }
expect:
  eligible: true
```

`offered:`는 branch나 허브가 제시한 선택지 집합 전체를 여러 번의 제시에 걸쳐 정확히 비교합니다(허브는
0.24.0부터 방문마다 자격 있던 선택지를 모두 합칩니다). `entry:`, `entries:`, `beat:`가 모두 없는 로어
테스트는 `E-TEST-LORE`입니다. 엔트리 하나는 `lute trace <file> --entry <id>`로 미리 볼 수 있습니다. 자격이
없는 엔트리나 비트를 제시하는 테스트는 `eligible:`로 단언하지 않으면 알림과 함께 통과합니다. 테스트도 목의
`bridges:`를 받습니다.

`plays/first.play.yaml`. 최상위 키는 `state`, `facts`, `choose`, `derive`, `bridges`(0.24.0), 세이브 시드인
`visited`, `presented`, `quests`, `entriesRead`, 그리고 `expect`와 `steps`입니다. 각 스텝은 `occasion`,
`engine`, `event`, `newRun`, `advance`, `include`, `end` 중 하나입니다:

```yaml
visited: [cafe.counter]                 # save seeds, applied before step 1
presented: { user: [vesna.gift] }       # spent `once: user` / `once: run` beats
quests: { lostCup: active }             # unset | active | complete | failed; objectives start undone
entriesRead: { user: [vesnaFirst] }     # run: entry.<id>.read · user: entry.<id>.everRead
state: { user.runs: 10, run.tips: 3, quest.lostCup.objectives.find.done: true }   # objective progress
facts: ["knows(vesna, manifest)"]
choose: { greet: wave, chat: [coffee, leave] }   # hub: visit order; a branch list: one per presentation
bridges: { check: [ { passed: true, margin: 3 } ] }   # 0.24.0: answers to plugin calls, consumed in order
steps:
  - occasion: hubVisit
    label: arrival                      # printed in the step header
    choose: { greet: tip }              # this step only, replacing that key
    expect: { options: { greet: [wave, tip] } }   # 0.24.0: the options this step offered, as a set
  - occasion: talk
    target: npc.vesna                   # <prefix>.<member> of the occasion's target domain
    expect: { winner: vesnaBack, offered: [vesnaBack, vesnaBark], notOffered: [vesna.gift] }
  - occasion: talk
    target: npc.mira
    expect: { presented: [cafe.talks.miraOrder, cafe.talks.miraHum] }   # the winner, then its `also` beats
  - occasion: inbox
    pick: megNote                       # required on a non-empty `select: all`, refused on `select: first` and `sequence`
  - occasion: inbox
    pick: none                          # pass: nothing presented or spent
  - occasion: evening                   # select: sequence: every eligible beat, in selection order
  - occasion: runEnd                    # a same-named world event's <on event> first, then <objective on="runEnd">
    expect: { quests: { lostCup: active }, state: { run.tips: 3 } }   # judged right after this step settles
  - event: combatEnd                    # a world event: active quests' <on event> run
  - engine:                             # writes what the engine owns; presents nothing
      state: { run.day: { add: 1 } }    # a literal, or { add: n }; quest.* is refused
      facts: ["awake(toma)"]            # any declared base relation, reserved ones included
      retract: ["awake(vesna)"]
  - advance: day                        # 0.24.0, with a clock: slot | day | <n>; settles, raises the clock's raise: occasions
    engine: { facts: ["awake(toma)"] }  # optional: writes of the same moment, applied where the clock arrives
  - include: common.steps.yaml          # 0.24.0: splice another script's steps here (path relative to this file)
  - newRun: { facts: ["knows(vesna, manifest)"] }   # or `true`; resets run.*, run facts, once: run, tier="run" quests
  - occasion: hubVisit
    repeat: 2
  - end: true                           # ends the playthrough (exit 0); later steps print as skipped
  - occasion: hubVisit                  # skipped
expect:                                 # judged at the end; a miss exits 1
  exit: complete
  quests: { regular: complete, lostCup: unset }
  state: { user.xp: 50, run.day: 1 }
  facts: ["can_halt(vesna)"]            # after derivation
  notFacts: ["awake(toma)"]
  transcriptContains: ["@narrator: You are a regular now."]
  transcriptLacks: ["You are no longer welcome."]
```

- `engine:` 스텝은 선언된 상태, 팩트, retract를 쓰며, 무엇이든 재생하기 전에 타입을 검사합니다. 쓴 뒤에는
  퀘스트 수명 주기를 정산하므로 쓰기 한 번으로 그 자리에서 퀘스트가 완료될 수 있습니다. `quest.*`는
  거부합니다: 퀘스트 상태는 수명 주기의 몫이며 `quests:`로 시드합니다. `newRun`은 같은 `state:`와
  `facts:`를 새 런의 시드로 받습니다.
- `target`, `pick`, `choose`는 `occasion` 스텝에만 씁니다. `label`은 어느 스텝에나, `repeat`은 `end`를 뺀
  어느 스텝에나 쓸 수 있습니다. 스텝의 `expect:`는 `occasion` 스텝에서 `winner`(계기가 지나가면 `none`),
  `offered`(자격 있는 비트의 부분집합), `notOffered`, `presented`(0.23.0: 제시된 id 전체를 순서대로)를,
  `end`를 뺀 모든 스텝에서 `quests`, `state`, `facts`, `notFacts`(0.23.1)를 받습니다. 어긋나면 그 스텝과
  양쪽 값을 알려 줍니다. `target:`이 있는 스텝은 그 대상의 `<objective on target>`도 판정합니다.
- `newRun` 스텝은 자신이 스냅숏한 `prev.run.*` 값 하나하나(`--json`: `prevRun`)와, 실제로 초기화한 run 등급
  퀘스트를 이전 상태와 함께 출력합니다. `::accept{at="nextRun"}`으로 미뤄 둔 퀘스트는 그 직후에 활성화됩니다.
  받아들일 수 없는 수락(부모가 활성이 아닌 `activate="accept"` 자식)은
  `note: accept of quest c spent — its parent quest p is not active yet …`로 출력됩니다.
- `advance:` 스텝은 시계를 앞으로만 옮깁니다. 그 `engine:` 쓰기는 시계가 도착하는 곳에 — 가는 길에 발생한
  `dayEnd` / `dayStart` 뒤, 마지막 정산과 발생 전에 — 적용됩니다. 거기서 시계 자신의 날이나 슬롯 경로에
  쓰거나, `clock.index`를 뒤로 옮기는 `engine:` 스텝은 사용 오류(종료 코드 2)입니다. `include:` 순환도 사용
  오류입니다.
- `bridges:`(0.24.0): 응답마다 그 호출이 읽는 결과 필드를 정확히 줍니다. 최상위 목록은 플레이 전체에서
  호출 순서대로 소비되고, 스텝 자신의 `bridges:`는 그 스텝의 호출이 먼저 소비하며 남은 응답은 그 스텝을
  실패시킵니다. 모르는 태그, 남거나 빠진 필드, 맞지 않는 값은 로드 시점의 사용 오류입니다. 응답 없는 호출은
  그 호출에서 플레이를 멈추고(종료 코드 3), `lute trace`와 `lute test`는 그 결과를 알 수 없음으로 읽습니다.
  목이나 테스트에서 빠진 필드는 그 파일 안을 가리키는 `E-TRACE-MOCK-TYPE`입니다. `scene.*` 시드는 더 이상
  호출에 응답하지 않으며, `lute play`는 그런 시드를 거부합니다(종료 코드 2).
- 스텝의 `expect.options: { <branch 또는 hub>: [ids] }`(0.24.0)는 그 스텝이 제시한 선택지를 단언합니다.
  자격 있는 비트가 없는 `select: all` 계기에는 `pick:`이 필요 없습니다.
- `transcriptContains` / `transcriptLacks`는 실제로 재생된 줄만, 한 줄에 `@speaker: text` 하나로 비교합니다
  (텍스트 조각만 써도 맞습니다). 건너뛴 가드 줄, 스텝 머리글, 연출은 `lute play`와 `lute test` 모두에서
  결코 맞지 않습니다.
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
| `E-CAST-UNKNOWN` | 캐스트가 선언되어 있는데(스키마의 `cast:`나 플러그인의 `cast` 내보내기) 이 화자는 그 안에 없거나, (0.24.0) `::auto{character}`, `::camera{focus}`, `speaker` 컴포넌트 인자가 캐스트 밖을 가리킵니다. 메시지가 가장 가까운 id를 제안합니다. |
| `W-CAST-ABSENT` | 0.24.0. 화자의 캐스트 항목이 `present:`를 선언했는데 줄을 감싼 가드가 그것을 함의하지 않습니다(`{vo}` 줄은 제외, `{os}` 줄은 검사). 줄에 가드를 달거나(`@corvin{when="holds(inParty(corvin))"}`) 그런 가드 아래로 옮기세요. 가드를 거짓으로 만들 수 있는 쓰기만 그 가드를 무효로 합니다. 단일 파일 `check`는 모든 경로에서 assert된 팩트를 볼 수 없지만 `check-project`는 봅니다. 0.25.0: 관계의 `changedOn:` 계기 뒤의 줄에서는 `assume: true`가 그 관계를 덮지 않습니다. |
| `E-BAD-ENUM` | enum 밖의 값입니다. 0.24.0부터는 화자의 캐스트 `emotions:` 밖의 `emotion=`도 해당합니다(`happy`는 `isolde`의 감정이 아님). |
| `E-ENGINE-OWNED-WRITE` | `::set`이 `owner: engine`으로 선언된 경로에 씁니다. 콘텐츠는 읽기만 합니다. `lute play`에서는 `engine:` 스텝으로, trace와 test에서는 목의 `state:`로 쓰세요. |
| `W-QUEST-STATE-ISSET` | `isSet(quest.<id>.state)`는 항상 참입니다. `'unset'`과 비교하세요. |
| `W-TEXT-LOOKS-LIKE-REF` | 줄의 텍스트 전체가 def나 파라미터 이름인 `@name`이라 글자 그대로 출하됩니다. `{{@name}}`으로 쓰세요. |
| `E-UNSET-UNCOVERED` / `E-NONEXHAUSTIVE` | `<match>`가 `unset`, enum 멤버, 숫자 틈을 놓쳤습니다(메시지가 빠진 값을 알려 줌). 갈래나 `<otherwise>`를 추가하세요. 0.24.0부터는 비트의 `when:`이 먼저 도메인을 좁히므로, 그것이 배제한 갈래는 쓰지 않아도 됩니다. |
| `E-TAG-INLINE-BODY` / `E-TAG-NOT-ONE-LINE` | 태그와 본문이 한 줄에 있거나, 태그가 여러 줄로 나뉘었습니다. |
| `E-BRANCH-ALL-GUARDED` / `E-HUB-NO-EXIT` | 메뉴가 빌 수 있거나, 허브가 끝나지 않을 수 있습니다. |
| `E-SET-TYPE` / `E-REF-TYPE` / `E-ATTR-TYPE` | 값의 타입이 자리에 맞지 않습니다. 디렉티브 안의 따옴표 친 `"@def"`나, 컴포넌트가 텍스트에 끼워 넣는 `string` 파라미터에 넘긴 `@def`가 흔한 원인입니다. |
| `E-ATTR-DEF-DYNAMIC` | 디렉티브 속성에 상태를 읽는 `@def`를 넣었습니다. 속성 값은 상수여야 하니 `<match>`로 나누세요. |
| `E-INTERP-DEF` | `{{@def}}`의 본문을 식 하나로 펼칠 수 없습니다(펼침 순환, 또는 `$`를 읽는 본문). |
| `E-ATTR-QUOTE` | 속성 값을 작은따옴표로 감쌌습니다. `"…"`를 쓰고, 값 안의 `"`는 `\"`로 쓰세요. |
| `E-DEF-DECL` | def 형식이 잘못되었습니다: 타입을 추론할 수 없거나, `type:` 없이 `params:`를 썼거나, 모르는 키가 있습니다. |
| `E-OBJECTIVE-MISSING-DONE` / `E-OBJECTIVE-QUEST-DONE` | 목표에 `done`이 없거나, `quest=`와 `done=`을 함께 썼습니다. |
| `E-QUEST-TIER-MIX` | 하위 퀘스트의 `tier`가 부모와 다릅니다. 둘의 등급을 맞추세요: 섞인 트리는 새 런이 한쪽만 초기화하면 영영 잠깁니다. |
| `E-GRAMMAR-NOT-ADMITTED` | 이 kind에서 허용되지 않는 구문입니다. 예: 엔트리 안의 `<branch>`, 퀘스트 안의 제목. |
| `E-BEAT-ATTR` | 비트 키 형식이 잘못되었거나 `on`이 없거나, `when`이 `scene.*`를 읽거나, 대상 없는 계기에 씬이나 번들 비트의 `target`을 썼거나 대상이 대상 도메인 밖에 있거나(비슷한 이름 제안과 함께), 엔트리의 `once`가 `run`, `user`, `day`, `slot`이 아니거나, 시계가 없는데 `once: day` / `slot`을 썼거나, `also`가 bool이 아니거나 엔트리 또는 `select: all` / `sequence` 계기에 있습니다. `id`가 없거나 `id`에 `-`가 있거나 문서에 `id:`가 없는 `<beat>`, 형식이 잘못되었거나 `on`이 없는 목표의 `target=`, `on=` 없는 `until=`도 `E-BEAT-ATTR`입니다. |
| `E-OCCASION-UNKNOWN` | 플러그인이 계기를 선언했는데 이 계기는 그중에 없습니다. |
| `W-BEAT-PRIORITY-TIE` | 한 `select: first` 계기에서 대상이 같거나 둘 다 없고 priority도 같은 두 비트의 `when`이 서로 배타적임을 증명할 수 없습니다. 승자는 파일 순서로 정해집니다. |
| `W-BEAT-ONCE-RUN-USER` | 기본값 `once: run`을 그대로 둔 비트의 `when`이 user 등급 상태만 읽어서 런마다 다시 재생됩니다. 의도한 것이면 `once: run`을 직접 쓰고, 아니면 `once: user`를 쓰세요. |
| `W-QUEST-HANDLER-DEAD` | 결코 실패할 수 없는 퀘스트(`fail`도, `by=` 기한이 있는 필수 목표도, 실패할 수 있는 필수 하위 퀘스트도, 부모 퀘스트도 없음)에 `<on event="questFailed">`가 있습니다. |
| `W-STAGE-ABSENT` | 어떤 경로에서 퇴장했거나 `::bg` 장면 전환으로 자동으로 숨겨진 캐릭터를 줄이 무대에 세웁니다. 선택지와 `<match>` 갈래는 따로 따라가므로 한 갈래의 퇴장이 형제 갈래에서 경고를 내지 않습니다. 갈래가 다시 합쳐진 뒤에는 모든 갈래가 무대에 남겨 둔 캐릭터만 무대에 있습니다. |
| `E-BEAT-UNREACHABLE` / `E-ARM-DEAD` | 조건이 결코 성립할 수 없습니다. `check-project`는 팩트 질의도 판정합니다. 0.23.0부터는 한 `&&` 안의 모순(`run.n > 5 && run.n < 3`)도 잡습니다. `check-project --wip`에서는 아직 아무것도 만들어 내지 않는 관계 때문에만 죽은 가드가 경고입니다. |
| `E-ACCEPT-TARGET` | `::accept`가 없는 퀘스트나 `start`가 있는 퀘스트를 가리키거나, (0.24.0) 부모와 함께 활성화되는 자식을 가리키거나(그 자식에 `activate="accept"`를 선언하세요), `at`이 `"nextRun"`이 아닙니다. |
| `W-QUEST-NEVER-ACCEPTED` | 0.24.0, `check-project`. 어떤 `::accept`도 이름을 부르지 않는 수락형 퀘스트(`start` 없음, 또는 `activate="accept"` 자식)입니다. 결코 활성화되지 않습니다. 0.25.0부터 목의 `accepts:`는 치지 않으며(메시지가 그 목을 밝힙니다), 엔진이 수락하는 퀘스트에는 `accept="external"`을 선언하세요. |
| `E-FACT-EXCLUSIVE` / `E-RULE-EXCLUSIVE` | 0.25.0. `::assert`가 두 `excludes:` 관계를 모든 경로에서(`lute trace` / `lute test`에서는 걸은 경로에서) 함께 성립하게 하거나, 규칙이 다른 쪽이 성립하는 곳에서만 한쪽을 도출합니다. |
| `W-LUTE-VERSION-STALE` | 문서의 `luteVersion`이 툴체인과 다릅니다. 0.25.0: 매니페스트의 `defaults: luteVersion`에서 물려받은 경우 `check-project`는 문서마다가 아니라 `lute.project.yaml`의 그 줄에서 한 번 보고합니다(`… — every document inherits it (N documents)`). |
| `W-DEADLINE-BEFORE-DONE` | 0.24.0. `on=` 목표의 `by=`가 `done`이 성립할 때마다 성립하므로, 매 정산마다 판정하는 `by`가 계기가 `done`을 판정하기 전에 목표를 실패시킵니다. 그 조건을 `until=`로 쓰세요. |
| `E-STATE-DECL` | 상태 선언이 잘못되었습니다. 0.24.0부터는 열린 종류나 모르는 종류에 대한 `per:`, 그리고 멤버가 아닌 키를 쓰거나 값도 `_`도 없는 멤버를 남기거나(`` `default:` gives no value for `guild` ``) `per:` 없는 경로에 쓴 맵 `default:`도 해당합니다. |
| `W-DOMAIN-UNREAD` | 선언했지만 아무것도 읽지 않는 enum이나 종류입니다. 0.24.0부터는 `per:` 색인, `subsetOf:` 부모, 규칙 본문이나 `holds(…)`의 종류 원자도 읽기로 치며, 경고는 선언 자신의 줄에 붙습니다. |
| `E-CONN-UNKNOWN-NODE` | `visited('…')`나 `after`가 프로젝트에 없는 씬(0.24.0부터는 번들 비트도 가능)을 가리킵니다. |
| `E-CLOCK-DECL` | 0.24.0. 스키마의 `clock:`이 잘못되었습니다: `day` / `slot`이 선언되지 않았거나 타입이 틀렸거나 `owner: engine`이 아니거나(`` `day: run.day` must be declared `owner: engine` ``), `slots`가 슬롯 enum의 멤버가 아니거나 `slot` 없이 쓰였거나, 모르는 `raise` 계기이거나, 두 번째 시계입니다. |
| `E-ENUM-LABEL-NOT-MEMBER` | 0.24.0. enum의 `labels:`가 그 멤버가 아닌 것을 가리킵니다. |
| `E-CEL-TYPE` | 0.24.0. `%`의 피연산자가 정수가 아닙니다: `` `%` takes two integers: `2.5` is not an integer ``. |
| `E-RULE-GUARD-DEF` | 0.24.0. 규칙의 `cel()` 가드가 없는 `@def`를 부르거나 인자 개수가 틀렸거나 `$`를 씁니다. |
| `E-RELATION-RESERVED-NAME` | 0.24.0. 관계 이름이 CEL 호출, 매크로, 키워드(`has`, `holds`, `count`, `isSet`, `now`, …)와 같아서 `holds(has(…))`를 쓸 수 없습니다. 이름을 바꾸세요. |
| `W-RELATION-UNREAD` / `W-DEF-UNUSED` | 0.24.0, `check-project`, 선언 위치에서. 관계가 assert, 시드, 도출되지만 어떤 조건, 규칙 본문, def도 읽지 않거나, 어떤 콘텐츠, def, 규칙 가드도 `@def`를 참조하지 않습니다. 플레이 스크립트와 테스트는 읽기로 치지 않습니다. |
| `E-PLUGIN-PARSE` | 플러그인 내보내기를 파싱할 수 없습니다. 0.24.0부터는 모르는 키(``unknown field `selct` … did you mean `select`?``)와 코어 훅이 아닌 `lower: { kind: builtin }`도 포함하며, 이어지는 `E-PLUGIN-MISSING-ACTIVE`는 플러그인을 불러오지 못했다고 알려 줍니다. |
| `E-CONN-EPISODE-ID-DUP` / `E-QUEST-ID-DUP` | 두 문서가 같은 씬 id나 퀘스트 id를 쓰거나, 번들 비트의 정식 `<doc>.<beat>` id가 씬 id와 같습니다. |
| `E-DUP-VOICEKEY` | 텍스트가 다른 줄들이 같은 `voiceKey`로 컴파일됩니다. 대개 `{speaker}-{code}` 템플릿으로 고정했을 때입니다. 기본값 `{prefix}.{speaker}-{code}`를 쓰거나 줄마다 다른 `code=`를 주세요. |
| `E-CAPABILITY-MISMATCH` | 프로젝트의 문서들이 서로 다른 기능 스냅샷으로 해석되어(다른 프로필이나 씬별 `plugins:`) 하나로 컴파일할 수 없습니다. |
| `E-TEST-LORE` | `*.test.yaml`이 `entry:`, `entries:`, `beat:` 없이 로어 문서를 가리킵니다. 제시할 것을 적으세요. |
| `E-TEST-FILE` | `*.test.yaml`의 `file:`이 가리키는 문서가 없습니다. 그 테스트만 실패하고 나머지 스위트는 계속 실행됩니다. |
| `E-TRACE-BEAT` | `lute trace --beat <id>`가 문서의 어떤 번들 비트도 가리키지 않습니다(또는 로어 문서가 아닙니다). `lute run --beat`는 같은 경우를 종료 코드 `2`로 거부합니다. |
| `W-REWARD-DOUBLE-CREDIT` | 퀘스트의 `<on>`이나 목표 본문이 보상 종류가 이미 `credits:`로 지정한 경로를 `::set`해서 두 번 지급됩니다. `::set`이나 보상 중 하나를 지우세요. |
| `W-FACT-GUARANTEED` | 팩트 가드가 모든 경로에서 항상 참이라 불필요합니다. |
| `W-BEAT-SHADOWED` | 항상 자격이 있고 소진되지 않는 앞선 비트가 매번 이깁니다. |
| `W-ENTRY-REF-UNKNOWN` | `entry.<id>.read`나 `entry.<id>.everRead`가 아무도 선언하지 않은 엔트리를 가리킵니다. |
| `E-LEGACY-CONTENT-SIGIL` · `W-WHEN-TEST-LITERAL` | 옛 문법입니다. `lute fix`가 고쳐 줍니다. |
| `E-PERSIST-REMOVED` | 선택지에서 `persist=`를 직접 지우세요. `into=`만으로 run 팩트가 기록됩니다. |

## 주의할 점

**`start`가 없는 퀘스트는 스스로 활성화되지 않습니다.** 수락형이라서, 씬이 `::accept{quest="id"}`를
실행하거나, 엔진이 `accept="external"` 퀘스트를 수락하거나, 목이나 테스트가 `accepts:`에 적을 때까지 `unset`으로 남습니다(플레이 스크립트는 `quests:`로
상태를 시드할 수 있습니다). `start`가 있는 퀘스트에 `::accept`를 쓰면 `E-ACCEPT-TARGET`입니다. 예외는
`quest=`로 지정된 자식으로, `activate="accept"`를 선언하지 않는 한 부모와 함께 활성화됩니다. 부모와 함께
활성화되는 자식을 `::accept`해도 `E-ACCEPT-TARGET`입니다.

**`once`는 씬과 엔트리에서 뜻이 다릅니다.** 씬 비트의 기본값은 `once: run`이라서, `once: false`로 쓰지
않으면 런마다 최대 한 번 재생됩니다. `once`가 없는 엔트리는 반복됩니다. `once="run"`은 새 런이
`entry.<id>.read`를 초기화할 때까지 엔트리를 소진시키고, `once="user"`는 영구히 소진시킵니다
(`entry.<id>.everRead`). 기본값 `once: run`을 그대로 둔 비트의 `when`이 user 등급 상태만 읽으면 런마다
다시 재생됩니다(`W-BEAT-ONCE-RUN-USER`). 대개 `once: user`를 뜻한 것이며, `once: run`을 직접 쓰면 다시
재생되는 것이 의도임을 밝힙니다.

**`after:`는 구조이고, 상태는 `when:`이 담당합니다.** `after:`는 `&&`와 `||`로 묶은 `visited`,
`completed`, `active`만 읽으며 씬 그래프에 쓰입니다. 비트는 둘 다 성립할 때만 자격이 있습니다.
`<quest>`의 `after=`는 그래프 메타데이터일 뿐 활성화를 늦추지 않습니다. 퀘스트를 씬에 묶으려면 조건을
`start`에 넣으세요: `start="visited('cafe.counter')"`.

**`::end`는 `lute play`에서 자신의 제시만 끝냅니다.** 플레이 전체가 끝나지는 않습니다. 그 스텝은 정산되고
플레이는 다음 스텝으로 이어집니다. 플레이를 일찍 멈추려면 `- end: true` 스텝을 추가하세요. 뒤의 스텝은
건너뜀으로 출력되고 플레이는 `0`으로 종료합니다.

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
스크립트 최상위 `quests:`로 시드하세요. 시계가 있으면(0.24.0) 시간은 `advance:` 스텝으로 옮기세요.
`clock.index`를 뒤로 옮기는 `engine:` 스텝은 사용 오류입니다.

**`::bg` 장면 전환은 모두를 무대에서 내립니다.** 자동으로 숨겨진 캐릭터는 퇴장한 것으로 기록되므로,
`::auto`로 다시 등장시키기 전의 대사는 `W-STAGE-ABSENT`입니다. `::clear`(0.24.0)도 같은 방식으로 모두를
퇴장시키되 배경은 그대로 두며, 이때 경고는 그 `::clear`를 가리킵니다. `::bg`의 경우:

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

**`by=`는 시점이고 `until=`은 장소입니다(0.24.0).** `by=` 기한은 매 정산마다 판정하므로, 목표의 계기로
돌아가지 않는 플레이어도 기한을 놓칩니다. 0.23.1은 `on=` 목표의 `by`를 그 계기가 발생할 때만 판정했습니다.
장소에 묶인 기한이 필요하면 `until=`을 쓰세요(`on=`이 필요합니다). 옛 규칙에 맞춰 쓴, `done`이 `by`를
함의하는 `on=` 목표(`done="run.v == 'fell'" by="run.v != 'undecided'"`)는 이제 계기를 발생시키는 스텝에서
둘이 함께 성립하지 않는 한 실패합니다. `W-DEADLINE-BEFORE-DONE`이 이를 알리고 같은 조건의 `until=`을
제안합니다.

**`::set{… when="…"}`은 확정 대입이 아닙니다.** 쓰기가 일어나지 않을 수 있으므로, 기본값이 없는 경로를
나중에 읽으면 여전히 값이 없을 수 있습니다:

```lute expect="E-MAYBE-UNSET"
---
kind: scene
id: tab.open
state:
  run.day: { type: number, default: 1 }
  run.tab: { type: number }
---

## Tab

::set{run.tab = 0 when="run.day > 1"}
@mira: Your tab is {{run.tab}}.
```

**`%`에는 정수가 필요합니다.** 두 피연산자 모두 정수여야 하며, 소수 리터럴이나 숫자가 아닌 피연산자는
`E-CEL-TYPE`입니다. 실행 중에는 소수 값이나 0으로 나누기가 알 수 없음으로 읽힙니다.

```lute expect="E-CEL-TYPE"
---
kind: scene
id: week.end
state:
  run.hours: { type: number, default: 0 }
---

## Payday

@mira{when="run.hours % 7.5 == 0"}: Payday.
```

**`_`는 규칙 본문에만 씁니다.** 본문에서는 새 변수입니다. 머리의 `_`는 `E-DATALOG-PARSE`인데, 머리의 모든
인자는 묶인 변수나 상수여야 하기 때문입니다:

```lute expect="E-DATALOG-PARSE"
---
kind: scene
id: ledger.rule
entities:
  person: { members: [ada, hollis] }
relations:
  owes:   { args: [person, person], tier: run }
  debtor: { args: [person, person], derive: true }
rules:
  - "debtor(D, _) :- owes(D, _)"
---

## Ledger

@narrator{when="holds(debtor(ada, hollis))"}: Ada owes.
```

**`transcriptContains`는 재생된 줄만 봅니다.** 0.24.0부터 `lute play`와 `lute test` 모두 제시된 콘텐츠
줄을 한 줄에 `@speaker: text` 하나로 비교합니다. 건너뛴 가드 줄(`skip @x "…" — when: false`), 스텝 머리글,
후보 목록, 연출은 결코 맞지 않으므로, 건너뛴 줄 덕분에 통과하던 `transcriptContains`는 이제 제대로
실패합니다.

**대상 없는 계기에서 엔트리의 `target=`은 메타데이터입니다.** 엔트리는 어떤 대상을 적었든 그 계기의 모든
발생에 응답합니다. 대상에 묶고 싶다면 계기에 `target:` 도메인을 주세요. 씬이나 번들 비트의 `target`은
거기서 여전히 `E-BEAT-ATTR`입니다.

**응답 없는 브리지는 멈춥니다. 기본값으로 채우지 않습니다.** 뒤의 가드가 결과를 읽는 플러그인 호출에는
`bridges:` 응답이 필요합니다. 응답이 없으면 `lute play`는 기본 갈래로 가기 전에 그 호출에서 멈추고(종료
코드 3), `lute trace` / `lute test`는 결과를 알 수 없음으로 읽어, 그대로 쓸 수 있는 답인
`bridges: { check: [ { passed: <bool>, margin: <number> } ] }` 힌트와 함께 미완료로 멈춥니다. 예전에는 상태
모양의 기본값을 읽었습니다. 결과 슬롯을 시드하던 0.23.1 테스트나 목(`state: { scene.check.guards.passed: true }`)은
더 이상 가드를 정하지 못하니, 그 시드를 `bridges: { check: [ { passed: true, margin: 3 } ] }`로 바꾸세요.

**플러그인 내보내기의 오타는 이제 오류입니다.** `{ selct: all }`은 예전에 `select: first`로 불러와졌지만,
0.24.0부터 모든 내보내기 파일은 모르는 키를 거부하며(비슷한 이름을 제안하는 `E-PLUGIN-PARSE`), 코어 훅이
아닌 `lower: { kind: builtin }`도 마찬가지입니다. 일반 패스스루가 필요하면 `lower:`를 빼세요.

**단일 파일 검사의 `W-CAST-ABSENT`는 오경보일 수 있습니다.** `lute check`는 줄을 감싼 가드만 봅니다.
모든 경로에서 앞서 실행된 `::assert{inParty(isolde)}`나 아무도 retract하지 않는 시드가 있으면
`check-project`는 모든 경로에서 성립하는 팩트를 세므로 `@isolde: …`가 깨끗합니다. 단일 파일 검사는 그럴 수
없으며, 메시지가 그 사실을 알려 줍니다.
