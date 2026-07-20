---
title: 첫 장면 작성하기
description: 빈 파일에서 작지만 실제로 동작하는 Lute 장면 하나를 단계별로 만들면서, 매 단계마다 lute 도구를 실행해 그 결과를 정확히 확인합니다.
---

이 문서는 Lute를 한 번도 다뤄본 적 없는 시나리오 작가를 위한 "여기서 시작" 안내입니다 —
컴파일러 배경지식은 필요 없습니다. 빈 파일에서 **작지만 실제로 동작하는 장면 하나**를 단계별로
만들며, 매 단계마다 실제 `lute` 도구를 실행해 도구가 정확히 뭐라고 말하는지 확인합니다. 언어
버전 **0.7.0**을 대상으로 합니다.

일반 텍스트 편집기, 터미널, 그리고 `lute` 명령
([먼저 설치하세요](/ko/getting-started/installation/))이 필요합니다. 여기서 작성하는 모든
것은 **코어 Lute만** 사용합니다 — 플러그인도, 프로젝트 설정도 없습니다. 오직 언어 그 자체입니다.

## Part 1 — 최소한의 뼈대

빈 파일 `my-scene.lute`를 만들고 체커를 실행하세요 — 체커는 `.lute` 파일이 유효한지 알려줍니다:

```
$ lute check my-scene.lute
my-scene.lute:1:1: error [E-KIND-MISSING] required frontmatter key `kind` is missing; every root document must declare `kind: scene` or `kind: quest`
my-scene.lute:1:1: error [E-META-MISSING] required meta key `character` is missing
my-scene.lute:1:1: error [E-META-MISSING] required meta key `season` is missing
my-scene.lute:1:1: error [E-META-MISSING] required meta key `episode` is missing
```

이것이 `lute check`의 핵심 아이디어입니다: 파일을 읽고 무엇이 왜 잘못되었는지 한 줄씩 정확히
알려줍니다 — 결코 조용히 실패하지 않습니다. 모든 `.lute` 파일은 "이 문서는 무엇이고, 누구의
장면인가?"에 답하는 YAML **프런트매터 블록**(두 `---` 줄 사이)으로 시작합니다. 하나 추가하세요:

```yaml
---
kind: scene
title: A Quiet Table
character: mira
season: 1
episode: 1
pov: fixer
---
```

- `kind: scene` — 이 파일은 장면입니다(다른 종류인 `quest`는 퀘스트 로직 파일용입니다).
- `character` — 누구의 에피소드인지(시점 캐릭터의 스토리라인).
- `season` / `episode` — 이 장면이 속한 에피소드.
- `pov` — 플레이어 캐릭터의 id(플레이어가 조종하는 주인공).

저장하고 다시 검사하세요:

```
$ lute check my-scene.lute
ok: my-scene.lute (0 warning(s))
```

깔끔합니다 — 하지만 파일에는 아직 내용이 없습니다. 프런트매터 바로 아래에 내레이션 한 줄을
추가해 보세요:

```
@narrator: The diner is empty at this hour, and Mira likes it that way.
```

다시 검사하세요:

```
$ lute check my-scene.lute
my-scene.lute:10:1: error [E-CONTENT-OUTSIDE-SHOT] content lives inside a shot; add a `## <title>` heading above it
```

기억해야 할 규칙: **모든 내용은 헤딩 아래에 있습니다.** Lute 문서는 "샷"의 나열입니다 — 장면의
비트(beat) — 그리고 대사, 내레이션, 연출의 모든 줄은 그중 하나 안에 들어갑니다. 그 줄 앞에
헤딩을 추가하세요:

```lute
## The Counter

@narrator: The diner is empty at this hour, and Mira likes it that way.
```

(헤딩은 `## ` 뒤의 자유 텍스트입니다 — `## The Counter`, `## Scene 1. The diner`, `## Prologue` 모두
유효합니다. `The Counter`, `The Regular`, … 는 여전히 좋은 관례이지만 숫자는 문법이 아닙니다. 샷은 문서
순서대로 번호가 매겨집니다.)

```
$ lute check my-scene.lute
ok: my-scene.lute (0 warning(s))
```

이것이 뼈대의 전부입니다: 프런트매터, 헤딩 하나, 그 아래 한 줄.

## Part 2 — 말하기, 내레이션하기, 느끼기

내용 줄은 언제나 같은 형태입니다: `@who{attributes}: what they say`. 내레이션은 예약된 화자
`@narrator`를 사용합니다. Mira가 말하는 줄을 추가하세요:

```lute
@mira{emotion="content" variant="0"}: {{userName}}, you made it.
```

- `@mira`는 화자입니다. `emotion="content"`와 `variant="0"`은 어떤 초상화/포즈를 보여줄지
  고릅니다 — 이것들은 당신이 지어내는 것이 아니라 카탈로그 어휘입니다. `lute context`(Part 4)가
  프로젝트에서 사용할 수 있는 유효한 값들을 나열합니다.
- `{{userName}}`은 **보간(interpolation)**입니다 — 이중 중괄호로 감싼 텍스트는 런타임에
  채워집니다. `{{userName}}`은 항상 사용 가능한 것입니다: 플레이어 자신의 이름입니다.

이제 Mira의 속마음 줄을 추가하세요 — 소리 내어 말하지 않는 그녀의 사적인 생각입니다:

```lute
@mira{mono}: I should not be this pleased about a coffee order.
```

`{mono}`는 **전달 플래그(delivery flag)**입니다: 중괄호 안의 맨 단어(`=value` 없이)로, 줄이
전달되는 방식을 바꿉니다. `{mono}`는 속마음(interior monologue)을 뜻합니다 — 말이 아니라 생각으로
렌더링되며 어떤 캐릭터에게도 적용됩니다. 다른 전달 플래그가 두 개 더 있습니다: `{os}`는 줄을
**화면 밖(off-screen)**으로 표시하고(화자의 소리는 들리지만 무대에는 없음), `{vo}`는
**보이스오버(voiceover)**로 표시합니다(장면 위에 겹쳐지는 내레이션 방식의 전달). 셋은 모두
상호 배타적입니다 — 한 줄에 최대 하나 — 그리고 `@narrator`에는 어느 것도 허용되지 않습니다.

지금까지의 파일:

```lute
---
kind: scene
title: A Quiet Table
character: mira
season: 1
episode: 1
pov: fixer
---

## The Counter

@narrator: The diner is empty at this hour, and Mira likes it that way.

@mira{emotion="content" variant="0"}: {{userName}}, you made it.

@mira{mono}: I should not be this pleased about a coffee order.
```

## Part 3 — 플레이어에게 선택지 주기

`<branch>`는 플레이어에게 메뉴를 제시합니다. 그 안의 각 `<choice>`는 하나의 선택지로, 고유한
`id`, `label`(버튼 텍스트), 그리고 플레이어가 그것을 골랐을 때 재생되는 줄들을 가집니다.

때로는 특정 조건에서만 선택지가 나타나야 합니다 — 예를 들어 플레이어가 Mira를 전에 만난 적이
있을 때만. 그것이 **가드(guard)**입니다: `when="<condition>"`. 가드는 선언된 **상태(state)** —
엔진이 추적하는 작은 명명된 값 — 를 읽으므로, 먼저 프런트매터 안 `state:` 블록에 하나 선언하세요:

```yaml
state:
  scene.knowsMira: { type: bool, default: false }
```

이제 분기:

```lute
<branch id="orderChoice">
  <choice id="black" label="Order it black">
    @mira{emotion="content" variant="0"}: Good. No nonsense in a cup.
  </choice>
  <choice id="familiar" label="Say hi like an old friend" when="scene.knowsMira">
    @mira{emotion="surprised" variant="0"}: You remembered. That's new.
  </choice>
</branch>
```

첫 번째 선택지 `black`에는 `when`이 없습니다 — 항상 제공됩니다. 두 번째 `familiar`는
`scene.knowsMira`가 참일 때만 나타납니다. 분기에는 언제나 가드 없는 선택지가 최소 하나 필요합니다 —
그렇지 않으면 플레이어에게 빈 메뉴가 보일 수 있고, 이는 체커가 대신 잡아줍니다.

## Part 4 — 루프: check → read → fix → compile → trace

이것이 Lute를 작성하는 일상적인 리듬입니다. `lute check`는 당신의 맞춤법 검사기입니다 — 끊임없이
실행하게 될 것이고, 종종 `lute fix`가 자동으로 고쳐줄 만큼 작은 문제를 잡아냅니다.

습관적으로 옛 스타일의 시길(sigil)을 입력했다고 해봅시다 — mono 줄에서 `@` 대신 콜론을:

```
:mira{mono}: I should not be this pleased about a coffee order.
```

```
$ lute check my-scene.lute
my-scene.lute:19:1: error [E-LEGACY-CONTENT-SIGIL] content line sigil `:` was replaced by `@` — write `@speaker{…}: text`; `lute fix` applies this migration automatically
```

**진단 읽기:** `file:line:col: error [CODE] message`. 정확한 줄, 정확한 문제, 그리고 대신 무엇을
써야 하는지를 정확히 알려줍니다. 이런 기계적인 부류의 수정에는 다음을 실행하세요:

```
$ lute fix my-scene.lute
lute: migrated 1 edit(s)
```

`lute fix`는 파일을 제자리에서 다시 씁니다(바꿔야 할 부분만) 그리고 다시 검사하면 깔끔하게
돌아옵니다.

파일이 깔끔하게 검사를 통과하면, `lute compile`은 그것을 게임 엔진이 재생하는 플랫 JSON 명령
목록으로 바꿉니다 — 줄, 선택, 점프마다 하나의 항목이 순서대로 들어갑니다:

```
$ lute compile my-scene.lute
{
  "kind": "scene",
  "lute": "0.7.0",
  "meta": { "character": "mira", "season": 1, "episode": 1, "episodeId": "s01ep01", "title": "A Quiet Table" },
  "state": [ … ],
  "commands": [
    { "kind": "line", "role": "narration", "speaker": "narrator",
      "text": "The diner is empty at this hour, and Mira likes it that way." },
    { "kind": "line", "role": "dialogue", "speaker": "mira",
      "text": "{{userName}}, you made it.", "emotion": "content", "variant": 0 },
    …
  ]
}
```

(공간을 위해 재포맷하여 표시했습니다.) 이 파일은 절대 손으로 편집하지 않습니다 — 엔진이 소비하는
컴파일된 산출물입니다. 오류 없이 컴파일되었다는 것은 그 장면이 **정적으로 유효함**을 증명합니다:
모든 구성이 올바르게 형성되었고, 모든 상태 경로가 선언되었으며, 모든 `<match>`가 망라적입니다.
이것이 장면이 처음부터 끝까지 플레이 가능함을 증명하는 것은 아닙니다 — 그것은 통합 시점에
검증되는 런타임 속성입니다.

마지막으로, `lute trace`는 게임을 열지 않고 플레이스루를 미리 봅니다 — 각 분기에서 어떤 선택을
할지 `--choose <branchId>=<choiceId>`로 알려주면, 장면을 따라가며 화면에 무엇이 표시될지
출력합니다:

```
$ lute trace my-scene.lute --choose orderChoice=black
trace: my-scene.lute
  ## The Counter
    @narrator  The diner is empty at this hour, and Mira likes it that way.
    @mira  {{userName}}, you made it.
    @mira  I should not be this pleased about a coffee order.
  <branch orderChoice>   eligible: black   -> black
    @mira  Good. No nonsense in a cup.
trace complete: 1 decision; choices 1/2 (orderChoice)
```

그 기록은 당신이 제공한 모의 시나리오를 정확히 미리 보여줍니다 — 분기가 제대로 읽히는지
점검하기 위한 저작 보조 수단이며, 런타임 동작의 증명은 결코 아닙니다.

## Part 5 — `after:`로 장면 순서 잡기

실제 에피소드는 *시퀀스*입니다 — 한 장면은 플레이어가 다른 장면을 본 뒤에 오도록 의도됩니다.
그 의도된 순서는 하나의 프런트매터 키로 선언합니다: **`after:`**.

`after:`는 Lute의 체커와 `lute scenario` 분석이 이 장면에 도달한다고 가정하는 경로를 선언합니다.
이것은 플레이어를 어디로도 이동시키지 않으며 점프도 아닙니다 — *권고적(advisory)*입니다: 도구는
이를 사용해 당신의 에피소드들이 하나의 일관되고 분석 가능한 그래프로 맞물리는지 검증합니다.

`after:`는 의도적으로 아주 작습니다. 정확히 두 개의 구성 요소만 주어집니다:

- `visited("<sceneKey>")` — 플레이어가 그 장면을 본 순간 참이 됩니다.
- `completed("<questId>")` — 그 퀘스트가 완료된 순간 참이 됩니다.

`&&`(둘 다)와 `||`(둘 중 하나)로 조합하세요:

```yaml
after: 'visited("mira.s01ep01")'
after: 'visited("mira.s01ep01") && completed("theCoffeeDebt")'
after: 'visited("mira.s01ep01") || visited("mira.s01ep02")'
```

이것이 어휘의 전부입니다. `!`도, 산술도, 상태 읽기도 없습니다 — 이것들은 의도적으로 제외되었습니다.
런타임 상태에 조건부인 것은 무엇이든 당신의 `when=` 가드에 남습니다.

장면의 **표준 키(canonical key)**는 `{character}.{episodeId}`이며, `episodeId`는 기본적으로
`s{season}ep{episode}`(0으로 채움)입니다. 우리 튜토리얼 장면(`character: mira`, `season: 1`,
`episode: 1`)의 키는 **`mira.s01ep01`**입니다.

에피소드를 넘나들며 팩트를 이어가려면, 지속되는 **`run.`** 계층을 사용하세요. 만남을 기억하도록
다이너를 가르쳐 봅시다 — `run.metMira`를 선언하고 설정하세요:

```yaml
state:
  run.metMira: { type: bool }
```

```lute
::set{run.metMira = true}
```

`after:`와 장면 간 읽기는 여러 파일에 걸쳐야만 의미가 있으므로, 두 장면을 한 폴더에 넣고, 그
폴더를 프로젝트 루트로 표시하는 한 줄짜리 `lute.project.yaml`을 두세요:

```yaml
# episodes/lute.project.yaml
defaultProfile: core
profiles:
  core:
    plugins: {}
```

```lute
---
kind: scene
title: The Usual Booth
character: mira
season: 1
episode: 2
pov: fixer
after: 'visited("mira.s01ep01")'
state:
  run.metMira: { type: bool }
---

## The Counter

@mira{emotion="content" variant="0" when="run.metMira"}: Back again. You know where you sit.

@narrator: The coffee is already poured.
```

단일 파일 `lute check`는 파일 간 관계를 판단할 수 없습니다 — `.lute` 파일 하나만으로는 다른
에피소드가 무엇이 존재하는지 알 길이 없습니다. **프로젝트** 체커는 할 수 있습니다:

```
$ lute check-project episodes
ok: episodes/booth.lute (0 warning(s))
ok: episodes/diner.lute (0 warning(s))
ok: episodes (2 file(s), 0 project-wide warning(s))
```

이제 그래프를 살펴봅시다. `lute scenario`는 `after:`가 함의하는 모든 것에 대한 읽기 전용 설계
표면입니다. 인자 없이 실행하면, 전체 그래프를 재생 순서대로 출력합니다:

```
$ lute scenario episodes
project root: episodes
  topological layers:
    layer 0: scene(mira.s01ep01)
    layer 1: scene(mira.s01ep02)
  edges (prerequisite -> dependent):
    scene(mira.s01ep01) -> scene(mira.s01ep02)
```

`reach <key>`는 "플레이어가 여기까지 도달할 수 있는가, 그리고 어떤 경로로?"에 답하고,
`envelope <key>`는 `when=` 가드를 작성하기 전에 가장 알고 싶은 질문 — *여기서 읽어도 안전한
상태는 무엇인가?* — 에 답합니다:

```
$ lute scenario episodes envelope mira.s01ep02
envelope for scene(mira.s01ep02) (pre-entry):
  Guaranteed (safe to read under your declared routes):
    - run.metMira
```

`run.metMira`가 **Guaranteed**인 것은 경로 때문입니다: 부스로 들어가는 모든 선언된 경로는
다이너를 거치고, 다이너는 언제나 그것을 `::set`합니다. 이것이 진정한 장면 간 보장입니다 — 부스의
`when="run.metMira"` 읽기가 안전함이 증명됩니다.

## Part 6 — 다음 갈 곳

**무엇을 쓸 수 있는지 확실하지 않으신가요?** `lute context <file>`는 프로젝트가 허용하는 어휘를
정확히 출력합니다 — 연출 디렉티브, 그 속성, 열거 값(예: `emotion`), 선언된 상태, 전달 플래그
어휘 — 당신이 지정한 특정 파일에 맞게 해석하여:

```
$ lute context my-scene.lute
directives (8):
  auto: character, anchor, action
  bg: location, time, assetId
  camera: focus, zoom, move-x, move-y, shake, reset, duration, easing, delay, wait
  cut: assetId, action, full
  music: action, mood, volume, assetId, track
  sfx: sound, assetId, name
  vfx: type, label, transition
  video: assetId, action, wait
enums (6):
  emotion: neutral, surprised, delighted, shy, content, angry, sad
  mood: peaceful, tense, romantic, sad, upbeat
  …
deliveryFlags (3):
  {mono}: interior monologue / thought (not spoken aloud in-scene)
  {os}: off-screen: the speaker is heard but not currently staged/visible
  {vo}: voiceover: narration-style delivery layered over the scene
```

디렉티브 이름, 속성, 유효한 `emotion` 값을 추측하는 대신 다시 확인하고 싶을 때 언제든 실행하세요.
여기서부터는 각 구성을 깊이 다루는 **Language** 섹션을 따라가거나, 실제 프로젝트를 기능별로
둘러보는 [전체 스펙 쇼케이스](/examples/showcase/)를 읽어보세요.
