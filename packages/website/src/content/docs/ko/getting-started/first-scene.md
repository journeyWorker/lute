---
title: 첫 장면 작성하기
description: 빈 파일에서 작지만 실제로 동작하는 Lute 장면 하나를 단계별로 만들면서, 매 단계마다 lute 도구를 실행해 그 결과를 정확히 확인하고, 테스트로 고정합니다.
---

이 문서는 Lute를 한 번도 다뤄본 적 없는 시나리오 작가를 위한 "여기서 시작" 안내입니다 —
컴파일러 배경지식은 필요 없습니다. 빈 파일에서 **작지만 실제로 동작하는 장면 하나**를 단계별로
만들며, 매 단계마다 실제 `lute` 도구를 실행해 도구가 정확히 뭐라고 말하는지 확인합니다. 언어
버전 **0.36.1**를 대상으로 합니다.

일반 텍스트 편집기, 터미널, 그리고 `lute` 명령
([먼저 설치하세요](/ko/getting-started/installation/))이 필요합니다. 여기서 작성하는 모든
것은 **코어 Lute만** 사용합니다 — 플러그인도, 프로젝트 설정도 없습니다. 오직 언어 그 자체입니다.

**편집기와 터미널이 다르게 말하면** — `lute check`가 `ok`라고 하는 파일에 편집기가 빨간 밑줄을
긋거나, 그 반대라면 — 터미널을 믿고 `lute doctor .`를 실행하세요. 흔한 원인을 짚어 줍니다:
`lute`보다 오래된 편집기 언어 서버입니다(업그레이드한 뒤에는 편집기를 다시 시작하세요).

## Part 1 — 최소한의 뼈대

빈 파일 `my-scene.lute`를 만들고 체커를 실행하세요 — 체커는 `.lute` 파일이 유효한지 알려줍니다:

<!-- lute-diagnostics -->
```
$ lute check my-scene.lute
my-scene.lute:1:1: error [E-KIND-MISSING] required frontmatter key `kind` is missing; every root document must declare `kind: scene`, `kind: quest`, or `kind: lore`
my-scene.lute:1:1: error [E-META-MISSING] a scene needs an `id:`, its key in the project — write `id: opening` in the frontmatter
failed: my-scene.lute (2 error(s), 0 warning(s))
```

이것이 `lute check`의 핵심 아이디어입니다: 파일을 읽고 무엇이 왜 잘못되었는지 한 줄씩 정확히
알려줍니다 — 결코 조용히 실패하지 않습니다. 모든 `.lute` 파일은 "이 문서는 무엇이고, 이름은
무엇인가?"에 답하는 YAML **프런트매터 블록**(두 `---` 줄 사이)으로 시작합니다. 하나 추가하세요:

```yaml
---
kind: scene
id: mira.s01ep01
title: A Quiet Table
pov: fixer
---
```

- `kind: scene` — 이 파일은 장면, 즉 플레이어가 보는 대사입니다. 다른 두 종류는 `quest`(이야기가
  추적하는 목표)와 `lore`(아이템 설명처럼 게임이 찾아 읽는 텍스트)입니다.
- `id` — 프로젝트 안에서 유일한 장면의 이름입니다. 다른 장면과 테스트는 이 이름으로 이 장면을
  가리킵니다. 이름(글자, 숫자, `_`, `-`로 이루어지고 `-`로 시작하지 않는 이름)을 `.`으로 이은 것입니다
  (`door-notes`와 `doorNotes` 모두 됩니다). `mira.s01ep01`은 "Mira, 시즌 1,
  에피소드 1"로 읽히지만 어떤 이름이든 됩니다(`prologue`, `diner.opening`).
- `title` — 도구와 검색을 위한 사람이 읽는 제목입니다.
- `pov` — 플레이어 캐릭터의 id(플레이어가 조종하는 주인공).

위의 `E-META-MISSING` 오류가 요구하는 것이 바로 `id:` 줄입니다. 메시지의 `opening`은 예시 이름일
뿐이고, 이 장면의 id는 `mira.s01ep01`입니다.

저장하고 다시 검사하세요:

```
$ lute check my-scene.lute
ok: my-scene.lute (0 warning(s))
```

### 작가를 위한 따옴표와 YAML

프런트매터는 **YAML**이고, 뒤에 나올 테스트 파일과 플레이 파일도 YAML입니다. 이 안내서가 쓰는
것은 다섯 가지 규칙이면 모두 됩니다:

- **`key: value`**, 콜론 뒤에 공백 하나. 들여쓰기는 **공백으로만, 탭은 절대 쓰지 않습니다**.
  하위 항목은 부모 아래로 들여씁니다.
- **목록**은 한 줄에 `[a, b, c]`, 또는 줄마다 `- item`. **맵**은 한 줄에 `{ key: value }`, 또는
  들여쓴 `key: value` 줄들.
- **기호로 시작하는 값**(`!`, `[`, `{`, `'`, `"`, `*`, `&`)이나 `: ` 또는 ` #`가 들어간 값은
  **따옴표로 감쌉니다**. 평범한 단어와 숫자는 따옴표가 필요 없습니다.
- **따옴표는 번갈아 겹칩니다.** 큰따옴표 안에는 작은따옴표를, 작은따옴표 안에는 큰따옴표를 씁니다:

  ```yaml
  after: 'visited("mira.s01ep01")'    # Lute의 "…"를 감싼 YAML의 작은따옴표
  when: "run.accused == 'ruben'"      # Lute의 '…'를 감싼 YAML의 큰따옴표
  when: "!visited('accusation')"      # !로 시작하므로 반드시 따옴표
  ```

  `when: "run.accused == "ruben""`이라고 쓰면 문자열이 두 번째 `"`에서 끝납니다: `lute check`는 그
  줄에 `E-META-PARSE`를 보고하며 안쪽에 작은따옴표를 쓰라고 제안합니다. 안쪽 한 쌍을 바꾸세요.
- **곧은 따옴표만.** 워드 프로세서와 메모 앱은 `"`를 둥근 `“ ”`로 바꿉니다. Lute는 프런트매터에서도,
  `label="…"` 같은 태그 속성에서도 곧은 `"`와 `'`만 읽습니다. 태그 속성의 둥근 따옴표는
  `E-ATTR-QUOTE`이며, 다시 입력하라고 알려 줍니다.

`.lute` 본문 안의 태그 속성은 언제나 큰따옴표로 감싸므로, 그 안의 조건은 작은따옴표를 씁니다:
`when="run.accused == 'ruben'"`.

### 내용은 헤딩 아래에

파일에는 아직 내용이 없습니다. 프런트매터 바로 아래에 내레이션 한 줄을 추가해 보세요:

```lute
@narrator: The diner is empty at this hour, and Mira likes it that way.
```

다시 검사하세요:

<!-- lute-diagnostics -->
```
$ lute check my-scene.lute
my-scene.lute:8:1: error [E-CONTENT-OUTSIDE-SHOT] content lives inside a shot; add a `## <title>` heading above it
failed: my-scene.lute (1 error(s), 0 warning(s))
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
  고릅니다.
- `{{userName}}`은 **보간(interpolation)**입니다 — 이중 중괄호로 감싼 텍스트는 런타임에
  채워집니다. `{{userName}}`은 항상 사용 가능한 것입니다: 플레이어 자신의 이름입니다.

저장하고 검사해 보세요. 이번에는 **통과하지 않습니다**:

<!-- lute-diagnostics -->
```
$ lute check my-scene.lute
my-scene.lute:12:16: error [E-DOMAIN-UNKNOWN] `emotion` is not a declared domain — declare its members in an `enums:` block in this document's own frontmatter, in a project schema reached through `uses:`, or in a plugin's `enums` export before using `emotion`
failed: my-scene.lute (1 error(s), 0 warning(s))
```

오타가 아닙니다 — **슬롯은 Lute가 정하고, 멤버는 당신이 정한다**는 규칙 때문입니다. `emotion`은
언어가 아는 일곱 개의 어휘 슬롯(`emotion`, `action`, `anchor`, `mood`, `volume`, `musicAction`,
`vfxType`) 중 하나이지만, 당신의 캐릭터가 어떤 감정을 갖는지에 대해 컴파일러는 아무 의견도 갖지
않습니다. 그것은 당신 이야기의 몫이므로, 선언하기 전까지는 어떤 값도 유효하지 않습니다. 멤버는
문서에 관한 다른 모든 것을 선언하는 곳 — 프런트매터 — 에 선언합니다:

```yaml
enums:
  emotion: [neutral, surprised, delighted, shy, content, angry, sad]
```

장면이 실제로 쓰는 슬롯만 선언하세요. 이 장면이 쓰는 것은 `emotion` 하나뿐입니다. (선언할 때
필수 의미를 함께 요구하는 슬롯이 둘 있습니다: `action`은 캐릭터를 무대에서 내보내는 멤버를
나열하는 `exits:`가, `anchor`는 `default:`가 필요합니다. `lute init`은 일곱 슬롯 전부를 시작용
멤버와 함께 공유 `vocabulary.schema.yaml`에 만들어 주고, 장면들은 `uses:`로 그것을 끌어옵니다 —
여러 파일이 하나의 어휘를 공유하게 되면 그 형태가 맞습니다. 튜토리얼처럼 파일이 하나라면
프런트매터가 더 간단합니다.)

다시 검사하세요:

```
$ lute check my-scene.lute
ok: my-scene.lute (0 warning(s))
```

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

```lute check
---
kind: scene
id: mira.s01ep01
title: A Quiet Table
pov: fixer
enums:
  emotion: [neutral, surprised, delighted, shy, content, angry, sad]
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

<!-- lute-diagnostics -->
```
$ lute check my-scene.lute
my-scene.lute:18:1: error [E-LEGACY-CONTENT-SIGIL] content line sigil `:` was replaced by `@` in 0.2.2 — write `@speaker{…}: text`; `lute fix` applies this migration automatically
failed: my-scene.lute (1 error(s), 0 warning(s))
```

**진단 읽기:** `file:line:col: error [CODE] message`. 정확한 줄, 정확한 문제, 그리고 대신 무엇을
써야 하는지를 정확히 알려줍니다. 메시지만으로 부족하면 `lute --explain <CODE>`가 그 코드의 뜻을
보여 주고, 모든 코드를 모아 둔 [진단 레퍼런스](/reference/diagnostics/)의 항목을 링크합니다:

```
$ lute --explain E-LEGACY-CONTENT-SIGIL
E-LEGACY-CONTENT-SIGIL (error)

A content line uses the old `:` speaker sigil, which `@` replaced — write `@speaker{…}: text` instead.

Spec: dsl §7.1
More: https://lute-lang.vercel.app/reference/diagnostics/#e-legacy-content-sigil
```

이런 기계적인 부류의 수정에는 다음을 실행하세요:

```
$ lute fix my-scene.lute
lute: applied 1 fix(es)
```

`lute fix`는 파일을 제자리에서 다시 씁니다(바꿔야 할 부분만) 그리고 다시 검사하면 깔끔하게
돌아옵니다.

파일이 깔끔하게 검사를 통과하면, `lute compile`은 그것을 게임 엔진이 재생하는 플랫 JSON 명령
목록으로 바꿉니다 — 줄, 선택, 점프마다 하나의 항목이 순서대로 들어갑니다:

```
$ lute compile my-scene.lute
{
  "kind": "scene",
  "lute": "0.36.1",
  "irVersion": "0.36.1",
  "capabilityVersion": "f78bb8efcaab8c3ea4ccf1bbee976a80596a04b1aca59fbe74123abfa1f55225",
  "meta": {
    "id": "mira.s01ep01",
    "title": "A Quiet Table"
  },
  "state": [ … ],
  "enums": [ … ],
  "commands": [
    {
      "kind": "line",
      "addr": "001-0100",
      "role": "narration",
      "speaker": "narrator",
      "text": "The diner is empty at this hour, and Mira likes it that way.",
      "lineId": "mira.s01ep01.narrator_0010"
    },
    {
      "kind": "line",
      "addr": "001-0200",
      "role": "dialogue",
      "speaker": "mira",
      "text": "{{userName}}, you made it.",
      "emotion": "content",
      "variant": 0,
      "lineId": "mira.s01ep01.mira_0010",
      "voiceKey": "mira.s01ep01.mira-0010",
      "placeholders": [ … ]
    },
    …
  ],
  "shots": [
    {
      "shot": 1,
      "heading": "The Counter"
    }
  ]
}
```

(`…`는 지면을 위해 잘라낸 자리이고, 나머지는 출력 그대로입니다.) 당신이 선언한 `enums:`는
산출물의 **`enums`** 블록으로 그대로 실려 가므로, 엔진은 체커가 쓴 것과 똑같은 어휘로 값을
해석합니다. 당신의 `id:`는 모든 `lineId`와 `voiceKey`의 접두사이므로, 이 장면의 모든 줄은 다른
장면의 줄이 가질 수 없는 이름을 갖습니다. 그 밖에 한눈에 알아둘 필드가 둘 있습니다. **`addr`**는
레코드의 주소로 `{shot}-{index}` 형태이며, 하나의 산출물 안의 모든
`addr`는 같은 너비로 채워집니다 — 그래서 `addr` 문자열을 정렬하기만 하면 실행 순서가 나옵니다.
파싱할 필요가 없습니다. **`shots`**는 당신이 쓴 `## ` 헤딩을 산출물까지 실어 나르므로, 하위
도구가 어떤 레코드가 *어느 비트에* 속하는지 여전히 말할 수 있습니다.

이 파일은 절대 손으로 편집하지 않습니다 — 엔진이 소비하는 컴파일된 산출물입니다. 오류 없이
컴파일되었다는 것은 그 장면이 **정적으로 유효함**을 증명합니다: 모든 구성이 올바르게
형성되었고, 모든 상태 경로가 선언되었으며, 모든 `<match>`가 망라적입니다. 이것이 장면이 의도한
대로 재생됨을 증명하지는 않습니다 — 그것은 `lute trace`와, Part 6의 `lute test`가 하는 일입니다.

`lute trace`는 게임을 열지 않고 플레이스루를 미리 봅니다 — 각 분기에서 어떤 선택을
할지 `--choose <branchId>=<choiceId>`로 알려주면, 장면을 따라가며 화면에 무엇이 표시될지
출력합니다:

```
$ lute trace my-scene.lute --choose orderChoice=black
trace: my-scene.lute  (seeds: 0 paths, 0 facts; 1 selection)
  ## The Counter
    @narrator  The diner is empty at this hour, and Mira likes it that way.
    @mira{emotion="content" variant="0"}  {{userName}}, you made it.
    @mira{mono}  I should not be this pleased about a coffee order.
  <branch orderChoice>   eligible: black   -> black
    @mira{emotion="content" variant="0"}  Good. No nonsense in a cup.
trace complete: 1 decision; choices 1/2 (orderChoice)
```

그 기록은 당신이 준 선택을 그대로 미리 보여줍니다 — 분기를 플레이어처럼 읽어 보는 빠른
방법입니다. 줄은 속성을 중괄호에 그대로 달고 나오므로, `@mira{mono}`를 보면 어느 줄이 생각인지 바로
알 수 있습니다.

## Part 5 — `after:`로 장면 순서 잡기

실제 에피소드는 *시퀀스*입니다 — 한 장면은 플레이어가 다른 장면을 본 뒤에 오도록 의도됩니다.
그 의도된 순서는 하나의 프런트매터 키로 선언합니다: **`after:`**.

`after:`는 Lute의 체커와 `lute scenario` 분석이 이 장면에 도달한다고 가정하는 경로를 선언합니다.
이것은 플레이어를 어디로도 이동시키지 않으며 점프도 아닙니다: "이 장면은 저 장면 뒤에 온다"고
말할 뿐이고, 도구는 이를 사용해 당신의 에피소드들이 하나의 일관되고 분석 가능한 그래프로
맞물리는지 검증합니다. 각 장면을 실제로 *시작*하는 것은 게임 엔진입니다 — 엔진이 아직 없다면
`lute play`이고, 다음 페이지 [장면을 이야기로 잇기](/ko/getting-started/connect-scenes/)에서
그것을 준비합니다.

`after:`는 의도적으로 아주 작습니다. 정확히 세 개의 구성 요소만 주어집니다:

- `visited("<id>")` — 플레이어가 그 `id:`의 장면을 본 순간 참이 됩니다.
- `completed("<questId>")` — 그 퀘스트가 완료된 순간 참이 됩니다.
- `active("<questId>")` — 그 퀘스트가 진행 중인 동안 참입니다: 시작되었고, 아직 끝나지 않은 상태.

`completed`와 `active`는 서로 반대말이 아닙니다. `completed`는 과거에 대한 영구적인 사실이고,
`active`는 열렸다가 닫히는 구간입니다. 퀘스트가 *진행되는 동안에만* 말이 되는 장면이라면
`active`를 쓰세요.

`&&`(둘 다)와 `||`(둘 중 하나)로 조합하세요:

```yaml
after: 'visited("mira.s01ep01")'
after: 'visited("mira.s01ep01") && completed("theCoffeeDebt")'
after: 'visited("mira.s01ep01") && active("theCoffeeDebt")'
after: 'visited("mira.s01ep01") || visited("mira.s01ep03")'
```

(바깥 작은따옴표는 YAML의 것이고 안쪽 큰따옴표는 장면 id의 것입니다.
[작가를 위한 따옴표와 YAML](#작가를-위한-따옴표와-yaml)을 보세요.) 이것이 어휘의 전부입니다.
`!`도, 산술도, 상태 읽기도 없습니다 — 이것들은 의도적으로 제외되었습니다. 런타임 상태에
조건부인 것은 무엇이든 당신의 `when=` 가드에 남습니다.

`visited("mira.s01ep01")`는 Part 1에서 붙인 `id:`로 다이너를 가리킵니다. 모든 장면이 `id:`를 갖는
이유가 바로 이것입니다.

(**예전 장면.** `id:`가 생기기 전에는 장면을 세 키 `character:`, `season:`, `episode:`로 불렀고,
이름은 그 키들로 만들어졌습니다: `character: mira`, `season: 1`, `episode: 1`은 `mira.s01ep01`을
뜻했습니다. 그렇게 쓴 장면도 여전히 검사를 통과합니다. 새 장면에는 `id:`를 쓰세요. `id:`와 그
세 키 중 하나를 함께 쓴 장면은 키마다 `W-META-LEGACY` 경고 한 건을 받고, 서술 정보는 `extra:`
아래에 둡니다.)

에피소드를 넘나들며 팩트를 이어가려면, 지속되는 **`run.`** 계층을 사용하세요. 만남을 기억하도록
다이너를 가르쳐 봅시다 — `run.metMira`를 선언하고 장면 끝에서 설정하세요:

```yaml
state:
  run.metMira: { type: bool }
```

```lute
::set{run.metMira = true}
```

`after:`와 장면 간 읽기는 여러 파일에 걸쳐야만 의미가 있으므로, 두 장면을 한 폴더에 넣고, 그
폴더를 프로젝트 루트로 표시하는 `lute.project.yaml`을 두세요:

```
episodes/
  lute.project.yaml
  diner.lute        ← Part 1–4의 장면에 run.metMira와 ::set을 더한 것
  booth.lute        ← 아래의 새 후속 장면
```

음성 키에는 이미 장면이 들어가 있습니다 — 기본 `voiceKey`가 `{prefix}.{speaker}-{code}`이므로,
다이너와 부스의 첫 Mira 대사는 각각 `mira.s01ep01.mira-0010`과 `mira.s01ep02.mira-0010`, 녹음 두
개가 되고 따로 설정할 것은 없습니다:

```yaml
# episodes/lute.project.yaml
defaultProfile: core
profiles:
  core:
    plugins: {}
```

(0.22.0 이전에는 기본값이 접두사 없는 `{speaker}-{code}`여서 두 대사가 모두 `mira-0010`이
되었습니다 — 녹음 하나를 두 대사가 나눠 쓰게 되고, `check-project`는 이를 `E-DUP-VOICEKEY`로
거부합니다. 옛 키로 이미 음성을 녹음한 프로젝트는 `lute.project.yaml`에
`identity: { voiceKey: "{speaker}-{code}" }`를 고정해 그 키를 유지합니다.)

```lute check-project="docs/examples/episodes/booth.lute"
---
kind: scene
id: mira.s01ep02
title: The Usual Booth
pov: fixer
after: 'visited("mira.s01ep01")'
enums:
  emotion: [neutral, surprised, delighted, shy, content, angry, sad]
state:
  run.metMira: { type: bool }
---

## The Counter

@mira{emotion="content" variant="0" when="run.metMira"}: Back again. You know where you sit.

@narrator: The coffee is already poured.
```

어휘는 문서 단위로 선언되므로 부스 장면은 다이너의 `enums:` 블록을 그대로 반복합니다. 파일이
둘이 되는 지점이 복사를 그만둘 때입니다: 블록을 옆에 둔 `vocabulary.schema.yaml`로 옮기고, 각
장면에서는 `uses: [./vocabulary.schema.yaml]`로 바꾸세요. `lute init`이 만들어 주는 배치가 바로
이것입니다.

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
  edges (prerequisite -> dependent) [atom kind(s)]:
    scene(mira.s01ep01) -> scene(mira.s01ep02) [visited]
```

`reach <id>`는 "플레이어가 여기까지 도달할 수 있는가, 그리고 어떤 경로로?"에 답하고,
`envelope <id>`는 `when=` 가드를 작성하기 전에 가장 알고 싶은 질문 — *여기서 읽어도 안전한
상태는 무엇인가?* — 에 답합니다:

```
$ lute scenario episodes envelope mira.s01ep02
project root: episodes
envelope for scene(mira.s01ep02) (pre-entry — state available when control REACHES this node, before its own writes):
  Guaranteed (safe to read under your declared routes):
    - run.metMira   written by: scene(mira.s01ep01)
  Possible (set on SOME but not every declared route reaching this node; the Guaranteed paths above are not repeated):
    (none)
  Guaranteed facts (hold on every declared route reaching this node):
    (none)
  Possible \ Guaranteed -- warning-grade reads (set on SOME but not every declared route; suppressed by default in `check-project`, surfaced here):
    (none)
```

`run.metMira`가 **Guaranteed**인 것은 경로 때문입니다: 부스로 들어가는 모든 선언된 경로는
다이너를 거치고, 다이너는 언제나 그것을 `::set`합니다. 이것이 진정한 장면 간 보장입니다 — 부스의
`when="run.metMira"` 읽기가 안전함이 증명됩니다.

## Part 6 — 테스트로 이야기 고정하기

trace는 한 경로를 한 번 보여줍니다. **테스트**는 참이어야 할 것을 적어 두고 `lute test`를 실행할
때마다 확인하므로, 나중의 수정이 장면을 망가뜨리면 조용히 지나가지 않고 크게 실패합니다.

테스트는 프로젝트의 `tests/` 폴더에 `*.test.yaml` 파일 하나씩으로 둡니다.
`episodes/tests/diner.test.yaml`을 만드세요:

```yaml
file: ../diner.lute
choose: { orderChoice: black }
expect:
  transcriptContains: ["@mira: Good. No nonsense in a cup."]
  transcriptLacks: ["@mira: You remembered. That's new."]
  state: { run.metMira: true }
```

- `file:`은 테스트할 장면이며 **테스트 파일 기준 상대 경로**입니다: 테스트가 `tests/`에 있으므로
  다이너는 `../diner.lute`입니다. (`scenes/` 폴더가 있는 프로젝트라면 `../scenes/<name>.lute`.)
- `choose:`는 각 분기에서 고를 선택지로, `lute trace`의 `--choose`와 같습니다.
- `expect:`는 성립해야 할 것들입니다: `transcriptContains`는 반드시 나와야 할 줄(`@speaker: text`
  형태), `transcriptLacks`는 나오면 안 되는 줄, `state`는 장면이 끝난 뒤의 값입니다.

```
$ lute test episodes --project episodes
PASS  episodes/tests/diner.test.yaml  (episodes/tests/../diner.lute)

1 passed, 0 failed
```

실패한 테스트는 이유를 말합니다. `scene.knowsMira`로 가드된 `familiar`로 선택을 바꿔 보세요:

<!-- lute-diagnostics unverified="lute test respells the walk.rs literal `--choose {id}={choice}` as the test key `choose: {id}={choice}` and composes the reason, so no single format! literal matches; the block is byte-exact binary output" -->
```
$ lute test episodes --project episodes
FAIL  episodes/tests/diner.test.yaml  (episodes/tests/../diner.lute)
      trace refused:
        episodes/tests/../diner.lute:25:3: error [E-TRACE-CHOICE] `choose: orderChoice=familiar` is ineligible at its presentation point: its guard `scene.knowsMira` decided false: `scene.knowsMira` is false (mock `state: { scene.knowsMira: <value> }`)

0 passed, 1 failed
```

아무것도 `scene.knowsMira`를 설정하지 않았으므로 그 선택지의 가드는 거짓입니다. 테스트는 원하는
상태에서 시작할 수 있습니다: `choose:` 위에 `state: { scene.knowsMira: true }`를 더하면
`familiar` 경로도 테스트할 수 있습니다. 그다음 `lute test episodes --project episodes --coverage`는
아직 어느 테스트도 고르지 않은 선택지와, 어느 테스트도 가리키지 않는 장면을 나열합니다 — 다음
테스트를 위한 할 일 목록입니다.

테스트 파일이 받는 모든 키는 [CLI 참조](/tooling/cli/#test)에 있습니다.

## Part 7 — 다음 갈 곳

**이야기 전체를 플레이하기.** 테스트는 장면을 하나씩 확인합니다. 첫 장면부터 엔딩까지 모든 장면을
순서대로 플레이하려면 [장면을 이야기로 잇기](/ko/getting-started/connect-scenes/)로 가세요. 장면마다
한 줄과 플레이 스크립트 하나를 더하면, `lute play`가 플레이어처럼 이야기를 따라갑니다.

**무엇을 쓸 수 있는지 확실하지 않으신가요?** `lute context <file>`는 프로젝트가 허용하는 어휘를
정확히 출력합니다 — 연출 디렉티브, 그 속성, 현재 유효한 어휘 멤버(예: 당신의 `emotion` 목록),
선언된 상태, 전달 플래그 어휘, 언어 자체의 내장 디렉티브, `visited(…)`로 가리킬 수 있는 장면
id — 당신이 지정한 특정 파일에 맞게 해석하여:

```
$ lute context episodes/diner.lute
lute: note: using project episodes (nearest lute.project.yaml); pass --project to choose another
capabilityVersion: f78bb8efcaab8c3ea4ccf1bbee976a80596a04b1aca59fbe74123abfa1f55225
permissions: unrestricted (authoring/compile-time restrictions; not runtime sandbox enforcement)
directives (12):
  auto: character: string, anchor: domain:anchor, action: domain:action   [reads.onStage usesAnchor mayExitCharacter writes.characterState]
  bg: location: string, time: string, assetId: string   [mutatesScene]
  camera: focus: string, zoom: double, moveX: double, moveY: double, shake: double, reset: bool, duration: double, easing: string, delay: double, wait: bool
  clear:    [reads.onStage mayExitCharacter]
  cut: assetId: string, action: enum[show, hide], full: bool
  end: reason: string   [terminatesWalk]
  mark: id: string (required)
  music: action: domain:musicAction, mood: domain:mood, volume: domain:volume, assetId: string, track: string   [mutatesScene]
  next: to: string (required), when: string
  sfx: sound: string, assetId: string, name: string
  vfx: type: domain:vfxType, label: string, transition: string
  video: assetId: string, action: enum[show, hide], wait: bool
bridges (0):
rewardKinds (0):
occasions (0):
questsAllowed: true
enums (0):
stateSchema (4):
  prev.run.metMira: bool (owner: engine)
  run.metMira: bool
  scene.choices.orderChoice: enum [black, familiar, unset] (owner: engine)
  scene.knowsMira: bool
deliveryFlags (3):
  {mono}: interior monologue / thought (not spoken aloud in-scene)
  {os}: off-screen: the speaker is heard but not currently staged/visible
  {vo}: voiceover: narration-style delivery layered over the scene
projectEnums (1):
  emotion: neutral, surprised, delighted, shy, content, angry, sad
builtinDirectives (10):
  ::set{ <path> = <expr> [when="<condition>"] }  (also += / -=) — write a declared state path; engine-owned paths are the engine's (E-ENGINE-OWNED-WRITE)
  ::assert{ <relation>(<arg>, …) [when="<condition>"] } — assert a ground fact of a declared, non-derived, non-reserved relation
  ::retract{ <relation>(<arg | _>, …) [when="<condition>"] } — retract the matching facts of a declared, non-derived, non-reserved relation
  ::accept{quest="<questId>" [at="nextRun"] [when="<condition>"]} — accept a quest that has no `start` condition; `at="nextRun"` queues it until after the next new run
  ::use{component="<name>" <param>=<value> … [when="<condition>"]} — expand an imported component with named arguments; a param with a default may be omitted
  ::body — in a component with a `beat:` header, at the top level of its body: where a `<beat use=…>`'s own body goes
  ::next{to="<string>" [when="<condition>"]} — jump forward to the `::mark` named by `to` (only while `when` holds)
  ::mark{id="<string>" [when="<condition>"]} — name the position a `::next{to=…}` jumps to
  ::end{[reason="<string>"] [when="<condition>"]} — end this presentation here
  ::clear{[when="<condition>"]} — take every character on stage off it; takes no attributes
directiveAttrs (5; beyond each directive's own):
  when: condition — every directive
  duration: double — every directive but ::clear
  delay: double — every directive but ::clear
  wait: bool — every directive but ::clear
  at: time — a directive inside a <track> clip only
beatKeys (11; scene frontmatter `key: value`; <entry> / <beat> attributes `key="value"`):
  on: <occasion> — the occasion the beat answers
  target: <prefix>.<member> | kind:<kind> — the one target it answers, or every member of a kind (read as occasion.target)
  for: kind:<kind> — on an untargeted `select: sequence` occasion: presented once per member whose `when` holds, binding occasion.target
  when: <condition> — eligible only while it holds
  priority: <integer> — the higher eligible beat wins
  once: run | user | false | day | slot | week | season:<name> — presented at most once per run, ever, without limit, per clock day / slot / week, or per window of a season
  spentBy: <condition> — spent once the condition has held; `once` sets how long it stays spent (`run` unless written)
  also: true — scene and bundle beats, on a `select: first` occasion: presented after the winner too
  share: <key> — beats with one `share` key spend one `once` together
  after: <prerequisite> — scene and bundle beats: eligible once it holds, e.g. visited("<id>")
  use: <component> — bundle `<beat>`: its header from the component's `beat:` template, the component's params as attributes
questKeys (10; <quest> attributes):
  id="<questId>" — read as quest.<id>.state
  title="<text>" — the quest's name
  start="<condition>" — activates the quest when it holds; without it the quest is accept-driven
  fail="<condition>" — fails the active quest when it holds
  follows="<prerequisite>" — the quest's place in the scene graph; never gates activation (to wait, write start="visited('…')")
  tier="user | run | season:<name>" — when it returns to unset: never, at each new run, or each time the season opens
  activate="accept" — a child that waits for an ::accept instead of activating with its parent
  complete="all | any" — completes when every / any one required objective is done
  accept="external" — the engine accepts the quest outside any document
  rearm="<condition>" — returns the quest to unset (objectives cleared) each time the condition goes false→true
objectiveKeys (10; <objective> attributes):
  id="<objectiveId>" — read as quest.<quest>.objectives.<id>.done / .failed
  done="<condition>" — the objective is done once it holds
  quest="<questId>" — a subquest objective: done when that quest completes
  visibleWhen="<condition>" — hides the objective while false; never gates `done`
  title="<text>" — the objective's name
  optional="true" — not required for the quest to complete
  on="<occasion>" — judged when that occasion is raised
  by="<condition>" — a deadline: the first time it holds while not done, the objective fails
  target="<prefix>.<member>" — with `on`: judged only for a raise for that target
  until="<condition>" — with `on`: a deadline judged only when the objective's occasion is raised, after `done`
rewardKeys (5; <reward> attributes):
  kind="<rewardKind>" — what the engine pays
  target="<id>" — what the reward is for, per its kind
  amount="<integer> | <N>..<M>" — how much
  when="<condition>" — granted only while it holds
  outcome="failed" — grant when the quest fails; without it the reward grants on complete
enginePaths (13; the engine writes these — read them, never declare or ::set them):
  quest.<quest>.state: enum [active, complete, failed, unset] [quest] — the quest's lifecycle; `unset` until it activates (always assigned)
  quest.<quest>.failedBy: enum [unset, fail, by, until, subquest, cascade, superseded] [quest] — why the quest failed: its `fail`, a required objective's `by` / `until`, a required `subquest` that failed, a `cascade` from its parent, or `superseded` by a sibling; `unset` while it has not failed
  quest.<quest>.activatedAt: narrativeTime [quest] — the moment the quest activated
  quest.<quest>.objectives.<objective>.done: bool [quest] — the objective is done
  quest.<quest>.objectives.<objective>.failed: bool [quest] — the objective failed (its `by` / `until` deadline passed first)
  entry.<entry>.read: bool [run] — the entry was read this run
  entry.<entry>.everRead: bool [user] — the entry was ever read (a new run does not reset it)
  scene.choices.<branch> [scene] — the choice a `<branch>` / `<hub>` took: one of its choice ids, `unset` before
  scene.visited.<hub>.<choice> [scene] — that `<hub>` choice was ever taken (bool)
  occasion.target [occasion] — in a beat of a targeted occasion: the member the answered raise is for
  occasion.payload.<field> [occasion] — in a beat of an occasion with a `payload:`: that field of the answered raise
  clock.<field> [run] — derived from the declared `clock:` (day, slot, weekday, index, ended …)
  prev.<path> [run] — the previous run's (or season window's) value of `<path>`
scenes (2; read as visited("<id>")):
  mira.s01ep01, mira.s01ep02
```

`enums (0)`은 버그가 아닙니다: 그 줄은 활성화된 *플러그인*이 제공하는 멤버를 세는데, 이 파일은
플러그인을 하나도 쓰지 않습니다. 당신이 직접 선언한 것은 **`projectEnums`** 아래에 나옵니다 —
`emotion="content"`를 실제로 해석해 주는 어휘입니다.

`stateSchema`도 상태에 대해 같은 이야기입니다: 당신이 직접 선언한 것(Part 3의 `scene.knowsMira`,
Part 5의 `run.metMira`), 뒤따르는 구성이 플레이어가 어느 선택지를 골랐는지 읽을 수 있도록
`<branch>`가 대신 선언해 주는 `scene.choices.orderChoice`, 그리고 이전 런이 끝났을 때
`run.metMira`가 가졌던 값인 읽기 전용 `prev.run.metMira`입니다.

`builtinDirectives`는 언어가 직접 제공하는 디렉티브 목록입니다 — 이미 써 본 `::set`도 여기
있습니다. `beatKeys`와 `questKeys`는 비트 머리(장면의 frontmatter, `<entry>`나 `<beat>`)와 `<quest>`
요소가 가질 수 있는 키를 모두, 각각이 하는 일 한 줄과 함께 보여 줍니다 — 다른 사람의 장면에서 `on:`이나
`once:`를 만났을 때 찾아볼 곳입니다. `scenes`는 `visited("…")`가 가리킬 수 있는 id 목록입니다. `episodes/`에 `lute.project.yaml`이 있으므로 `context`는 `lute check`처럼 그 프로젝트로
파일을 해석하고 `lute: note:` 줄로 알려 줍니다. 그래서 이 목록에는 프로젝트의 모든 장면이 퀘스트와 로어
엔트리 id와 함께 나옵니다.

디렉티브 이름, 속성, 유효한 `emotion` 값을 추측하는 대신 다시 확인하고 싶을 때 언제든 실행하세요.
여기서부터는 각 구성을 깊이 다루는 **Language** 섹션을 따라가거나, 쓰는 동안
[치트시트](/ko/reference/cheatsheet/)를 열어 두거나, 실제 프로젝트를 기능별로 둘러보는
[전체 스펙 쇼케이스](/examples/showcase/)를 읽어보세요. 게임 한 편 전체가 궁금하다면 — 코지
미스터리, Ink 이식작, 로그라이크, 비주얼 노벨 — 테스트와 플레이까지 갖춰 검사를 통과하는
[예제 게임](/ko/examples/games/)을 보세요.

**편집기와 터미널이 다르게 말하나요?** 프로젝트에서 `lute doctor .`를 실행하세요. 툴체인과
프로젝트 설정을 검사하고, `lute`보다 오래된 편집기 언어 서버를 짚어 줍니다.

정해진 에피소드 순서가 아니라, 엔진이 알리는 순간 — 플레이어가 허브에 도착하고, 누군가에게 말을
걸고, 하루를 마치는 순간 — 에 따라 이야기가 흘러가는 게임을 만드시나요? 출발점이 될 동작하는
예제를 만들어 보세요:

```
$ lute init --template beats my-game
```

계기(occasion) 플러그인, 그 순간에 응답하는 비트, 퀘스트, 로어 엔트리, 플레이 스크립트, 시나리오
테스트를 만들어 주며 — 만든 그대로 `lute check-project`, `lute test`, `lute play`가 모두
통과합니다. [비트](/language/beats/)와 [스토리 플레이](/ko/tooling/play/)에서 모델을 설명합니다.
