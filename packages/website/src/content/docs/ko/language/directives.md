---
title: 핵심 디렉티브
description: lute.core의 열세 가지 디렉티브 — 열 가지 연출 말단(::bg, ::music, ::sfx, ::actor, ::clear, ::camera, ::cg, ::vfx, ::video, ::sequence), 보행 종료자 ::end, 앞으로 이동하는 쌍 ::jump / ::label — 의 속성, 프로젝트가 선언하는 연출 어휘, 속성 인용, 타이밍 키, 대기 블로킹 모델, when= 가드를 설명합니다.
---

**연출 디렉티브(staging directive)**는 씬을 연출하는 한 줄짜리 말단입니다. 배경, 음악, 사운드, 캐릭터, 카메라, 이벤트 일러스트(CG), 비디오, 시네마틱 시퀀스, 효과를 다룹니다. 형태는 다음과 같습니다:

```
::name{attributes}
```

디렉티브는 절대 중첩되지 않습니다 — 자식을 가진 것은 논리 블록(`<branch>`, `<timeline>` 등)입니다. 디렉티브 이름과 속성의 의미는 **어휘**이며, 문법을 바꾸지 않고 플러그인으로 확장할 수 있습니다. `lute context <file>`를 실행하면 프로젝트가 허용하는 디렉티브와 속성을 나열합니다.

## 코어 어휘

`lute.core`는 다음 정식 속성을 가진 열세 가지 디렉티브를 선언합니다 — 열 가지 연출 말단, 보행 종료자 `::end`, 앞으로 이동하는 쌍 `::jump` / `::label`입니다:

| 디렉티브 | 속성 |
|---|---|
| `::bg` | `location`, `time`, `assetId` — 씬 변경: 무대에 남아 있는 캐릭터를 먼저 숨깁니다(아래 참고). |
| `::music` | `playback`(`musicPlayback` 도메인), `mood`(`mood` 도메인), `volume`(`volume` 도메인), `assetId` |
| `::sfx` | `sound`(들리는 것), `assetId`(구체적인 파일) |
| `::actor` | `character`(필수), `anchor`(`anchor` 도메인), `action`(`action` 도메인), `emotion`(`emotion` 도메인), `costume`(`costume` 도메인) — 등장, 퇴장, 포즈, 표정, 의상 |
| `::clear` | 없음 — 무대의 모든 캐릭터가 퇴장하며 배경과 음악은 유지됩니다(아래 참고). |
| `::camera` | `focus`(캐스트 멤버), `framing`(`framing` 도메인), `move`(`cameraMove` 도메인), `transition`(`transition` 도메인) — 최소 하나 필요 |
| `::cg` | `assetId`(필수), `display`(`show`\|`hide`, 기본값 `show`), `layout`(`cgLayout` 도메인) — 이벤트 일러스트. player-web는 배우 위, 대사 창 아래에 그립니다([스테이지 레이어](/ko/player/getting-started/#6-스테이지-레이어)) |
| `::vfx` | `type`(`vfxType` 도메인), `label`, `transition` |
| `::video` | `assetId`(필수), `display`(`show`\|`hide`, 기본값 `show`) |
| `::sequence` | `name`(필수, `sequence` 도메인) — 엔진이 소유한 시네마틱의 참조이며 `wait` 기본값은 `true`입니다. |
| `::end` | `reason`(선택, 자유 형식) — 보행을 종료하며, 연출이 아니라 제어 흐름입니다(아래 참고). |
| `::jump` | `to`(필수) — `::label`로 앞으로 이동하며, 제어 흐름입니다(아래 참고). |
| `::label` | `name`(필수) — `::jump`의 위치 이름을 지정하며, 레코드를 내보내지 않습니다(아래 참고). |

```lute
::bg{location="family_restaurant" time="afternoon" assetId="BG.space.family_restaurant.interior.afternoon"}
::music{playback="start" mood="peaceful" assetId="sound-bgm-common-vn-mood-peaceful-0.mp3" volume="quiet"}
::actor{character="marina" anchor="center" action="fadeInUp"}
::camera{focus="marina" framing="close" duration="0.5"}
```

*([`docs/examples/marina-s01ep02.lute`](https://github.com/journeyWorker/lute/blob/main/docs/examples/marina-s01ep02.lute)에서 가져왔습니다.)*

캐릭터 연출은 `::actor`가 담당합니다. 음악을 페이드아웃하려면 `::music{playback="fadeOut"}`이고, 캐릭터 퇴장은 `action` 도메인의 `exits:`에 `fadeOutDown`이 나열되어 있을 때 `::actor{character="marina" action="fadeOutDown"}`입니다. 모든 속성 값은 큰따옴표로 감싼 문자열입니다(안쪽의 `"`는 `\"` 또는 [`&quot;`](#인용된-값의-캐릭터-참조)로 씁니다. 작은따옴표는 `E-ATTR-QUOTE`입니다). 또는 [def](/ko/language/params/)의 상수로 접히는 맨 `@ref`를 쓸 수 있습니다(`::camera{focus=@hero}`; 상태를 읽는 def는 `E-ATTR-DEF-DYNAMIC`). 인라인 코드 표현식은 없으므로 연출은 튜링 완전하지 않습니다.

`bg` 스테이징 레코드는 `::bg`로 작성했든 플러그인 디렉티브의
`lower: { record: bg, fields: ... }`로 생성됐든 **씬 변경**입니다. 무대에 남아 있는 모든
캐릭터가 그 직전에 삽입된 `actor` 레코드로 숨겨집니다(`provenance.by: "stage-bookkeeping"`).
새 장소에서도 계속 말할 캐릭터는 `actor` 레코드로 다시 등장시켜야 합니다. 그 전에 쓴 줄에는
`W-STAGE-ABSENT`가 표시되므로([아래](#무대-상태) 참고), 명시적으로 다시 등장시키세요:

```lute
::bg{location="station" time="night"}
::actor{character="marina" action="fadeInUp"}
@marina: The last train is gone.
::bg{location="street" time="night"}
::actor{character="marina" action="fadeInUp"}
@marina: We walk, then.
```

## 연출 어휘는 프로젝트가 소유합니다

코어는 **슬롯**을 선언하고, 프로젝트는 **멤버**를 선언합니다. 위 표의 도메인 타입 속성은 모두 `enums:`에서 선언한 닫힌 도메인을 읽습니다 — 문서 프런트매터에 선언하거나, `uses:`를 통해 도달하는 스키마에 선언합니다([콘텐츠 어휘](/ko/language/vocabulary/) 참고). 코어는 어느 도메인에도 멤버를 제공하지 않으므로 한 프로젝트의 카메라는 `framing="closeUp"`, 다른 프로젝트의 카메라는 `framing="twoShot"`이라 할 수 있고, 검사기는 각각 자신의 목록을 기준으로 검사합니다:

| 도메인 | 읽는 곳 |
|---|---|
| `anchor` | `::actor{anchor}` — `anchor`를 생략하면 `default`를 사용합니다. |
| `action` | `::actor{action}`과 줄의 `action` — `exits:`의 멤버는 캐릭터를 무대에서 내립니다. |
| `emotion` | `::actor{emotion}`과 줄의 `emotion` |
| `costume` | `::actor{costume}` |
| `framing` | `::camera{framing}` |
| `cameraMove` | `::camera{move}` |
| `transition` | `::camera{transition}` |
| `cgLayout` | `::cg{layout}` |
| `musicPlayback` | `::music{playback}` |
| `sequence` | `::sequence{name}` |
| `mood`, `volume` | `::music{mood}`, `::music{volume}` |
| `vfxType` | `::vfx{type}` |
| `textStyle` | 줄 텍스트의 [인라인 스타일 스팬](/ko/language/dialogue-and-cast/#인라인-텍스트-수식자) |

도메인 밖의 값은 `E-BAD-ENUM`이고 문서가 선언하지 않은 도메인은 `E-DOMAIN-UNKNOWN`입니다. 프로젝트는 이런 방식으로 새 디렉티브를 선언하는 것이 아니라 코어 슬롯을 채웁니다:

```lute check
---
kind: scene
id: demo.rooftop
enums:
  anchor:
    members: [left, center, right]
    default: center
  action:
    members: [fadeInUp, fadeOutDown]
    exits: [fadeOutDown]
  emotion: [neutral, angry, shy]
  costume: [school, festival]
  framing: [wide, close, closeUp]
  cameraMove: [pushIn, shake]
  transition: [cut, dissolve]
  cgLayout: [full, inset]
  sequence: [fireworks]
  musicPlayback: [start, fadeOut]
  mood: [romantic]
---

## The Rooftop {#rooftop}

::bg{location="rooftop" time="night"}
::music{playback="start" mood="romantic"}
::actor{character="marina" anchor="left" action="fadeInUp" costume="festival"}
::camera{focus="marina" framing="close" transition="dissolve" duration="0.5"}
@marina: You came.
::actor{character="marina" emotion="shy"}
::camera{move="shake" duration="0.3"}
::sequence{name="fireworks"}
::cg{assetId="CUT.rooftop.fireworks" layout="full"}
@narrator: The sky opens in colour.
::cg{assetId="CUT.rooftop.fireworks" display="hide"}
::video{assetId="VID.rooftop.ending"}
::music{playback="fadeOut" duration="2"}
::actor{character="marina" action="fadeOutDown"}
```

각 디렉티브는 디렉티브와 속성의 이름을 그대로 담은 `kind`와 필드를 가진 연출 레코드 하나로 컴파일됩니다(`lute compile`, 연출 레코드만 추림):

```json
{"kind": "bg", "family": "staging", "position": "001-0100", "location": "rooftop", "time": "night", "timing": {"wait": true}}
{"kind": "music", "family": "staging", "position": "001-0200", "playback": "start", "mood": "romantic"}
{"kind": "actor", "family": "staging", "position": "001-0300", "character": "marina", "anchor": "left", "action": "fadeInUp", "costume": "festival"}
{"kind": "camera", "family": "staging", "position": "001-0400", "focus": "marina", "framing": "close", "transition": "dissolve", "timing": {"wait": false, "duration": 0.5}}
{"kind": "actor", "family": "staging", "position": "001-0600", "character": "marina", "emotion": "shy"}
{"kind": "camera", "family": "staging", "position": "001-0700", "move": "shake", "timing": {"wait": false, "duration": 0.3}}
{"kind": "sequence", "family": "staging", "position": "001-0800", "name": "fireworks", "timing": {"wait": true}}
{"kind": "cg", "family": "staging", "position": "001-0900", "assetId": "CUT.rooftop.fireworks", "display": "show", "layout": "full", "timing": {"wait": false}}
{"kind": "cg", "family": "staging", "position": "001-1100", "assetId": "CUT.rooftop.fireworks", "display": "hide", "timing": {"wait": false}}
{"kind": "video", "family": "staging", "position": "001-1200", "assetId": "VID.rooftop.ending", "display": "show", "timing": {"wait": true}}
{"kind": "music", "family": "staging", "position": "001-1300", "playback": "fadeOut", "timing": {"duration": 2.0}}
{"kind": "actor", "family": "staging", "position": "001-1400", "character": "marina", "action": "fadeOutDown", "exit": true}
```

이 레코드에는 다음과 같은 규칙이 적용됩니다:

- **줄 없이 쓰는 `::actor`.** 감정이나 의상만 설정하는 `::actor`는 캐릭터의 표정이나 의상을 현재 상태에서 바꿉니다. 작성된 `emotion`은 줄의 `emotion`과 마찬가지로 무대 상태를 갱신합니다. `exit: true`는 `exits:`에 나열된 `action`일 때만 나타납니다.
- **카메라 멤버는 불투명합니다.** `close`나 `shake`가 어떻게 보이는지는 엔진이 정합니다. Lute는 멤버십과 `focus` 캐스트 참조만 검사하며 숫자 변환을 합성하지 않습니다. `focus`, `framing`, `move`, `transition` 중 어느 것도 없는 `::camera`는 `E-CAMERA-EMPTY`입니다. 카메라는 저장된 상태의 일부가 아닙니다.
- **`display`는 항상 출력됩니다.** `::cg`와 `::video`는 생략된 `display`를 `show`로 해석하므로 레코드에는 항상 어느 방향인지 적힙니다.
- **시퀀스는 참조입니다.** `::sequence{name}`은 엔진이 재생하는 시네마틱을 가리킵니다. Lute는 그 내용을 설명하지 않으며 `wait="false"`로 쓰지 않는 한 블로킹합니다(`wait: true`). [`lute play`](/ko/tooling/play/)는 이를 시뮬레이션하지 않고 기록합니다.

### 인용된 값의 캐릭터 참조

디렉티브, 콘텐츠 줄 또는 `<choice>` 같은 태그의 큰따옴표 속성 값은 XML 문자 참조 `&quot;` `&apos;` `&amp;` `&lt;` `&gt;`와 숫자 형식 `&#NN;` / `&#xHH;`를 디코딩합니다(0.24.0). 0.24.0 이전에는 엔티티 텍스트가 그대로 제공되었습니다.

```lute check
---
kind: scene
id: demo.sign
---

## The Sign

::sfx{sound="a &quot;ding&quot; &amp; a hiss"}
<branch id="sign">
  <choice id="read" text="Read &quot;No Entry&quot; aloud">
    @narrator: You read it aloud.
  </choice>
  <choice id="fish" text="Order fish & chips, \"to go\"">
    @narrator: You order.
  </choice>
</branch>
```

아티팩트에는 `a "ding" & a hiss`와 `Read "No Entry" aloud`가 담깁니다. 디코딩은 한 번만 수행되므로 `&amp;quot;`는 `"`가 아니라 `&quot;`가 됩니다. 그 밖의 `&`는 그대로 남습니다. `fish & chips`의 맨 `&`, `&&`, 또는 `&nbsp;`처럼 목록 밖의 참조가 그 예입니다. `\"` 이스케이프는 여전히 작동합니다. 줄의 콜론 뒤 콘텐츠 텍스트는 속성 값이 아니므로 절대 디코딩되지 않습니다.
## 무대 상태

검사기는 문서 전체에 **무대 상태**를 이어 붙입니다 — 누가 무대에 있고, 어디에 있으며, 어떤 포즈인지 —
`lute compile`도 같은 상태에서 연출 레코드를 삽입합니다. 한 번도 보여 준 적 없는 화자의 첫 줄에는
`actor` 레코드가 필요하지 않으며 경고도 없습니다.
경고가 발생하는 것은 상태가 **떠났다고** 말하는 사람을 연출할 때입니다. `exit: true`인
`actor` 레코드의 선언된 actor 퇴장, 플러그인 디렉티브가 `bg`로 낮아져 발생한 자동 숨김,
또는 [`::clear`](#clear--무대-비우기) 뒤에, 그 캐릭터를 다시 actor 레코드로 보여 주기 전에 쓴
줄 — 또는 다른 선언된 퇴장 — 은 `W-STAGE-ABSENT`입니다.

0.22.0부터 무대 상태는 경로를 따라갑니다:

- 모든 `<branch>` 또는 `<hub>` 선택지와 모든 `<match>` 갈래에서 **분기**합니다. 각 갈래는 분기 지점의 무대 상태에서 시작하므로 한 갈래의 퇴장이 형제 갈래의 줄에 경고를 내지 않습니다.
- 갈래가 합류하는 곳에서는 모든 갈래에서 유지되는 것만 남깁니다. 모든 갈래가 캐릭터를 그곳에 남겨 둔 경우에만 합류 뒤에도 캐릭터가 무대에 있으며, 어느 한 갈래에서라도 캐릭터를 내렸다면 다시 보여 주지 않고 연출할 때 경고합니다.
- `::bg` 자동 숨김은 숨겨진 캐릭터를 **퇴장한 상태**로 기록합니다. 따라서 뒤의 줄에서 그중 하나가 말하면 다시 `::actor`로 데려올 때까지 경고하며, 씬 변경을 해도 앞서 선언된 퇴장을 더는 잊지 않습니다. (0.22.0 이전에는 둘 다 검사에서 통과했습니다.)

```lute check
---
kind: scene
id: demo.platform
enums:
  action:
    members: [fadeInUp, fadeOutDown]
    exits: [fadeOutDown]
  anchor:
    members: [left, center, right]
    default: center
---

## The Platform

::bg{location="station" time="night"}
::actor{character="marina" action="fadeInUp"}
@marina: The last train is gone.
<branch id="wait">
  <choice id="leave" text="Let her go">
    ::actor{character="marina" action="fadeOutDown"}
    @narrator: She walks off without a word.
  </choice>
  <choice id="stay" text="Ask her to stay">
    @marina: Fine. One more minute.
  </choice>
</branch>
@marina: So, what now?
```

`@marina: Fine. One more minute.`는 조용히 통과합니다 — `stay` 경로에서는 그녀가 떠난 적이 없기 때문입니다. 분기 뒤의 줄은 그렇지 않습니다. `leave` 경로가 마리나가 떠난 상태로 그 줄에 도달하기 때문입니다:

<!-- lute-diagnostics -->
```
platform.lute:27:1: warning [W-STAGE-ABSENT] `marina` left the stage on an earlier declared exit (line 20) on a path that reaches here and has not been shown again, so a spoken line here stages someone who is not present. Show them again with an `::actor` before this point, or remove the earlier exit
```

그 줄 전에 `::actor{character="marina" action="fadeInUp"}`를 두거나 `leave` 선택지 끝에 두면 모든 경로에서 마리나가 무대에 올라 경고가 사라집니다. `lute compile`은 같은 합류 지점에서 아티팩트를 연출합니다. 합류 뒤에는 그녀가 무대에 없으므로 그곳의 `::actor`는 새 등장이고 앵커도 다시 받습니다.

## `::clear` — 무대 비우기

`::clear`(dsl 0.24.0 §4)는 모든 캐릭터를 한 번에 무대에서 내립니다. 속성은 없습니다 — 타이밍 키도, 0.49.0부터는 플러그인 스탬프도 받지 않습니다(`E-UNKNOWN-ATTR`, [스탬프를 싣지 않는 디렉티브](#스탬프를-싣지-않는-디렉티브) 참고). 배경과 음악은 유지되며, 이것이 `::bg` 씬 변경과 구별되는 점입니다:

```lute check
---
kind: scene
id: demo.lastTrain
enums:
  action:
    members: [fadeInUp, fadeOutDown]
    exits: [fadeOutDown]
  anchor:
    members: [left, center, right]
    default: center
  musicPlayback: [start, stop]
  mood: [peaceful]
---

## The Last Train

::bg{location="station" time="night"}
::music{playback="start" mood="peaceful"}
::actor{character="marina" action="fadeInUp"}
::actor{character="oskar" action="fadeInUp"}
@marina: The last train is gone.
@oskar: So it is.
::clear
@narrator: The platform empties. The music plays on.
@marina: Wait for me.
```

`::bg`와 마찬가지로, `::clear`에 도달하는 경로 중 일부에만 무대에 있는 캐릭터도 포함하여 무대 상태가 보유한 모든 캐릭터가 퇴장합니다. 디렉티브는 캐릭터마다 `actor` 퇴장 레코드 하나로 컴파일되고, 자기 자신에 대한 레코드는 없으며 새로운 IR 종류도 없습니다. 그러므로 엔진에 새로 필요한 것은 없습니다:

```json
{ "kind": "actor", "family": "staging", "position": "001-0900", "character": "marina", "exit": true, "provenance": { "by": "stage-clear", "explanation": "`::clear` takes `marina` off stage" } }
{ "kind": "actor", "family": "staging", "position": "001-1000", "character": "oskar", "exit": true, "provenance": { "by": "stage-clear", "explanation": "`::clear` takes `oskar` off stage" } }
```

지워진 캐릭터는 `::actor`로 다시 보여 줄 때까지 사라집니다. 따라서 위 마지막 줄은 경고되고, 경고는 `::clear`를 지목합니다:

<!-- lute-diagnostics -->
```
platform.lute:25:1: warning [W-STAGE-ABSENT] `marina` was taken off stage by an earlier `::clear` (line 23) on a path that reaches here and has not been shown again, so a spoken line here stages someone who is not present. Show them again with an `::actor` after the `::clear`
```

[`lute play`](/ko/tooling/play/)는 실행된 곳에 `::clear`를 출력하고, `lute trace`는 이를 퇴장으로 기록합니다.

## 타이밍과 `wait` 모델

`duration`, `delay`, `wait`는 `::clear`, `::jump`, `::label`을 제외한 모든 디렉티브에 쓸 수 있는 예약된 **연출** 타이밍 키입니다:

- **`duration`** — 변환 길이(초)(예: `duration="0.6"`).
- **`delay`** — 디렉티브 자체 슬롯 시작에서부터의 오프셋(초).
- **`wait`** — 블로킹 제어(`true` / `false`).

`wait="true"`는 효과가 완료될 때까지 스크립트를 붙잡습니다. `wait`가 없거나 `false`이면 블로킹하지 않으므로 다음 줄이 동시에 진행됩니다. 기본값은 전역이 아니라 **디렉티브별**입니다. 예를 들어 `::bg`, `::video`, `::sequence`의 기본값은 `wait="true"`이고 대부분의 효과는 블로킹하지 않는 것이 기본입니다. 따라서 동시성은 연속된 `wait` 아닌 디렉티브로 표현하며 `<parallel>` 래퍼는 없습니다.

```lute
::camera{move="shake" duration="0.2"}                    /* no wait -> next line runs concurrently */
::camera{focus="elena" framing="closeUp" duration="0.5" wait="true"}  /* holds -> the following line waits for the push */
```

아티팩트에서는 해석된 값이 레코드의 `timing` 객체에 놓입니다 — `"timing": {"wait": true, "duration": 0.5}` — 설정된 값이 없으면 `timing`을 생략합니다.

`at` 키는 연출 타이밍 속성이 아닙니다. [`<timeline>`](/ko/language/timeline-and-property-tracks/) 안 클립에서만 유효한 타임라인 위치 키입니다.

콘텐츠 줄은 `wait`(bool)만 받고 다른 타이밍 키는 받지 않습니다. 줄의 `delay`와 `duration`은 계속 `E-UNKNOWN-ATTR`입니다. `wait`는 줄 레코드의 `timing.wait`로 컴파일되며, `wait`가 없으면 줄에 `timing`이 붙지 않습니다. 줄은 항상 자기 진행(advance)을 기다립니다. `wait`는 그 줄 뒤에서 다음 줄 전까지 나오는 연출 큐가 그 줄을 기다리는지를 정합니다. `true`(또는 생략)이면 기다리고, `false`이면 줄이 표시되는 동안 시작됩니다. 다음 줄은 여전히 앞의 줄을 기다립니다.

```lute
@marina{wait="false"}: The fireworks start as I speak.
::cg{assetId="CUT.rooftop.fireworks" display="show"}   /* 줄이 표시되는 동안 시작 */
@oskar: Look at that.                                  /* marina의 줄은 여전히 기다림 */
```

### 스탬프를 싣지 않는 디렉티브

`::jump`, `::label`, `::clear`는 스탬프가 붙는 레코드를 내보내지 않습니다. `::jump`는 스탬프 없는 `jump` 레코드로, `::label`은 레코드 없이 컴파일되고, `::clear`는 자기 레코드가 없습니다(무대에 있는 캐릭터마다 actor 퇴장 하나를 주입할 뿐이며, 빈 무대에서는 아무것도 주입하지 않습니다). 이 디렉티브들에 플러그인 스탬프 속성이나 코어 타이밍 키를 쓰면 조용히 버려지는 대신 해당 속성 위치에서 `E-UNKNOWN-ATTR`입니다:

- `::jump`: `` `::jump` does not carry `cue`: it compiles to a `jump` record that has no stamp — put the attribute on the line or directive before it ``
- `::label`: `` `::label` does not carry `cue`: it compiles to no record — put the attribute on the line or directive after it ``
- `::clear`: "속성을 받지 않으며, 무대의 모든 캐릭터는 각자의 퇴장으로 나갑니다"라는 기존 메시지가 이제 스탬프 속성에도 적용됩니다.

`::clear`는 자신이 주입하는 퇴장에 스탬프를 넘겨주지 않습니다. 퇴장이 있는지는 무대 상태에 달려 있어서, 빈 무대에서는 값이 버려지기 때문입니다. 스탬프는 직접 쓰는 `::actor{exit}`에 두세요.

## `::end` — 보행 종료

`::end`는 자기 레코드에서 보행을 멈춥니다. 명령 배열 끝까지 도달하는 것과 정확히 같지만, 이유를 담을 수 있습니다:

```lute
@narrator: The platform emptied out. Nothing left to catch.
::end{reason="missedTheLastTrain"}
```

`reason`은 선택적 자유 형식입니다 — `"completed"`, `"error"`, 엔딩 id 등을 쓸 수 있습니다. Lute는 여기에 의미를 부여하지 않으며 아티팩트에 전달하여 호스트가 표시하도록 합니다. 위 표에서 연출 말단이 아닌 항목은 `::end`, `::jump`, `::label`이므로 [`<track>`](/ko/language/timeline-and-property-tracks/) 클립 안에는 이들을 넣을 수 없습니다(`E-TIMELINE-CONTENT`). 해당 클립은 연출 말단과 `::set`만 받습니다.

[`lute play`](/ko/tooling/play/)에서 `::end`는 자신이 실행 중인 프레젠테이션(또는 퀘스트 핸들러)만 끝냅니다. 스텝은 계속 정착되고 플레이스루는 다음 스텝으로 진행됩니다. 플레이 스크립트는 스텝의 `end: true`로 일찍 멈출 수 있습니다. `::end`는 게임을 끝내지 않습니다. 게임이 끝날 수 있는 경우에는 스키마의 [`terminal:`](/ko/state/schemas/#게임-종료-terminal)로 이를 표현하며, 그 뒤에는 엔진이 계기를 발생시키지 않습니다. [게임이 끝났습니다](/ko/tooling/play/#게임이-끝났을-때)는 “끝”의 다섯 가지 의미를 나란히 설명합니다.

같은 직선형 본문에서 `::end` **뒤에** 있는 것은 실행될 수 없으며, 검사기는 본문마다 첫 데드 노드에 기준점을 두고 한 번 보고합니다: `W-CODE-AFTER-END` — *`::end` 뒤의 도달 불가 콘텐츠(여기서 보행이 종료됨)*. 이는 경고이며 `lute check --deny W-CODE-AFTER-END`로 승격할 수 있습니다.

범위는 *바로 바깥에 있는* 시퀀스 하나뿐입니다 — 섹션 본문 하나, `<choice>` 본문 하나, `<when>` 갈래 하나, 목표 본문 하나, `<on>` 본문 하나입니다. 한 선택지의 `::end`는 형제 선택지나 바깥 `<branch>` 뒤 콘텐츠에 아무 영향도 주지 않습니다. 따라서 갈래별 엔딩은 다음처럼 쓰며, 그 아래 공유 꼬리는 계속 살아 있습니다:

```lute
<branch id="ledge">
  <choice id="jump" text="Jump for it">
    @mira: Nothing to it.
    ::end{reason="fell"}
  </choice>
  <choice id="wait" text="Wait for the ladder">
    @mira: I can be patient.
  </choice>
</branch>

@narrator: The siren faded somewhere east.
```

### 종료가 플러그인 디렉티브가 아닌 코어인 이유

종료는 제어 흐름이며, 제어 흐름은 플러그인 어휘가 가질 수 없는 한 가지입니다. 플러그인 디렉티브는 `kind: "plugin"` 레코드로 낮아지고 검사기에는 불투명하므로, 도달 가능성 분석은 그런 레코드가 종료된다는 것을 알 수 없습니다. 따라서 뒤의 콘텐츠가 데드라고 보고되지 않으며 `W-CODE-AFTER-END`도 존재할 수 없습니다. `::end`를 새로운 IR 명령 종류로 제공하는 것은 정직한 버전 신호이기도 합니다. 이를 도입한 IR 마이너를 구현하지 않는 엔진(`0.8.0`)은 자신이 인식하지 못하는 레코드를 그냥 지나가는 대신 아티팩트를 거부해야 합니다.
## `::jump`와 `::label` — 앞으로 이동

`::label{name="…"}`은 위치에 이름을 붙이고, `::jump{to="…"}`은 보행을 그 위치로 이어 갑니다. 점프는 문서 순서상 **앞으로만** 이동하므로 보행은 DAG로 유지됩니다. 선택지를 다시 제공하려면 [`<hub>`](/ko/language/choices-and-hubs/)를 사용하세요. 라벨은 문서마다 하나의 네임스페이스를 공유하며, 섹션의 `{#id}`는 식별 메타데이터이지 라벨이 아니므로 `::jump`의 대상이 될 수 없습니다. `::jump`는 `when=`을 받으며, 가드된 점프는 조건이 참일 때만 실행됩니다:

```lute check
---
kind: scene
id: demo.jump
state:
  run.tip: { type: int, default: 0 }
---

## The Counter

@narrator: The diner is quiet.
::jump{to="outro" when="run.tip > 0"}
@narrator: You leave without a word.
::label{name="outro"}
@narrator: The bell over the door rings.
```

`::label`은 레코드를 내보내지 않습니다. 라벨은 컴파일 시 해석되며, 점프의 `target`은 라벨 뒤 첫 레코드의 `position`입니다. 위의 가드된 점프는 점프를 둘러싼 한 갈래 `match`로 컴파일됩니다:

```json
{"kind": "jump", "family": "control", "position": "001-0300", "target": "001-0700"}
```

문서에 `::label`이 없는 이름을 `to`로 지정하면 `E-JUMP-UNDEFINED`이고, 점프 위치와 같거나 그 위에 있는 라벨은 `E-JUMP-BACKWARD`입니다. 가드되지 않은 `::jump` 뒤의 같은 본문 콘텐츠는 실행되지 않으며 `W-CODE-AFTER-JUMP`입니다. 줄로 이동하려면 그 줄 바로 앞에 `::label`을 두세요.

`::jump`와 `::label`은 스탬프를 싣지 않습니다. 플러그인 스탬프나 `wait` 같은 타이밍 키는 `E-UNKNOWN-ATTR`입니다([스탬프를 싣지 않는 디렉티브](#스탬프를-싣지-않는-디렉티브) 참고). 속성은 점프 앞의 줄이나 디렉티브에, 또는 라벨 뒤의 줄이나 디렉티브에 두세요.

## 예약된 디렉티브

세 가지 `::` 디렉티브는 연출 어휘가 아니라 내장 기능입니다. `::set`은 선언된 상태를 쓰고([상태 모델](/ko/state/state-model/) 참고), `::use`는 재사용 가능한 콘텐츠 컴포넌트를 확장하며([컴포넌트와 extends](/ko/language/components-and-extends/) 참고), `::accept`는 퀘스트를 수락합니다(아래 참고). 콘텐츠는 팩트를 변경하는 `::assert` / `::retract`도 사용하며([팩트와 Datalog](/ko/state/facts-and-datalog/) 참고), 씬과 퀘스트 모두에서 쓸 수 있습니다. `docs/examples/haven/scenes/cryobank.lute`는 `kind: scene`이고 `<choice>` 본문 안에 `::assert` 디렉티브 네 개를 담고 있으며, `lute check`를 실행하면 `ok … (0 warning(s))`이 보고됩니다.

### `::accept` — 퀘스트 수락

`::accept{quest="<id>"}`(dsl 0.21.0)는 이 지점에서 플레이어가 **수락 주도(accept-driven)** 퀘스트 — `start` 술어가 없는 퀘스트 — 를 수락한다고 선언합니다. 이는 엔진의 “퀘스트 수락” 동작과 `lute trace --accepted-quest`의 씬 측 형태이며, 보통 플레이어가 동의하는 선택지에 둡니다:

```lute
<branch id="request">
  <choice id="accept" text="I'll keep it calm">
    ::accept{quest="calmTheShed"}
    @vesna: Thank you.
  </choice>
  <choice id="decline" text="Not now">
    @vesna: Another time, then.
  </choice>
</branch>
```

`::set` 및 `::assert`와 마찬가지로 플러그인 어휘가 아니라 내장 기능입니다. 자체 IR 레코드인 `{"kind": "accept", "family": "declaration", "position": …, "quest": "calmTheShed"}`로 낮아지며, 엔진은 퀘스트가 아직 `unset`이면 활성화하고 그렇지 않으면 레코드를 무시합니다. 대상은 두 번 검사됩니다. `quest`가 없거나 형식이 잘못되면 `lute check`에서 `E-ACCEPT-TARGET`이고, 프로젝트 전체 `lute check`는 id가 프로젝트에 없는 퀘스트를 가리키거나 `start` 술어가 있는 퀘스트를 가리킬 때 `E-ACCEPT-TARGET`을 보고합니다(그런 퀘스트는 스스로 활성화하므로 수락해도 의미가 없습니다). 0.24.0부터는 부모와 함께 활성화되는 자식 퀘스트도 마찬가지이며 메시지에 부모가 표시됩니다. [퀘스트와 씬](/ko/language/quests-and-scenes/#퀘스트가-씬-및-계기를-만나는-방식)을 참고하세요.

같은 디렉티브에 0.24.0의 추가 기능 두 가지가 있습니다. `activate="accept"`로 선언한 자식 퀘스트는 `::accept`를 기다리며 부모가 활성 상태인 동안에만 활성화됩니다. 부모가 활성 상태가 아닐 때 도착한 수락은 효과 없이 소진되며, 도구 모음은 이를 알립니다(`lute play`는 `note: accept of quest c spent — its parent quest p is not active yet …`를 출력합니다). `::accept{quest="…" at="nextRun"}`은 다음 `newRun` 초기화 직후까지 수락을 대기열에 넣으므로, 런 사이 허브에서 받은 런 등급 퀘스트가 그 초기화 뒤에도 유지됩니다. `nextRun`은 `at`에 쓸 수 있는 유일한 값이며 그 밖의 값은 `E-ACCEPT-TARGET`입니다. 둘 다 [퀘스트와 씬](/ko/language/quests-and-scenes/)에 설명되어 있습니다.

## 디렉티브 가드하기: `when=`

`::set{… when="<condition>"}`은 조건이 거짓일 때 건너뛰는 동작을 원래부터 지원했습니다. dsl 0.26.0 §4부터 `::use`, `::accept`, `::assert`, `::retract`, 플러그인 패스스루 및 브리지 디렉티브도 같은 `when=`을 같은 의미로 받습니다:

```lute check
---
kind: scene
id: dock.recruiter
state:
  run.pitchHeard: { type: bool, default: false }
entities:
  item: { members: [nugget] }
relations:
  hasItem: { args: [item], tier: run }
---

## The Dock

@recruiter: Think it over. The pay is good.
::assert{hasItem(nugget) when="!run.pitchHeard"}
::set{run.pitchHeard = true}
::accept{quest="recruiterJob" when="holds('hasItem', ['nugget'])"}
@narrator{when="holds('hasItem', ['nugget'])"}: The nugget is heavy in your pocket.
```

가드된 디렉티브는 한 갈래 `match`로 컴파일됩니다. 조건이 거짓이면 건너뛰고 보행은 계속됩니다. 가드된 [`::use`](/ko/language/components-and-extends/#use-가드하기-when)는 전체 확장을 실행하거나 전혀 실행하지 않으며, 인자 읽기는 그 가드 아래에서 판정됩니다. 플러그인 디렉티브도 같은 방식으로 가드됩니다(`::give{item="nugget" when="!run.pitchHeard"}`). 반복 가능한 비트가 한 번만 받을 선물을 제공하는 방법입니다.

검사기에게 가드된 디렉티브는 결코 확정 효과가 아닙니다. 가드된 `::assert`는 모든 경로에서 성립하는 팩트가 아니고, 가드된 `::set`은 확정 쓰기가 아니며, 가드된 `::accept`는 확정 수락이 아닙니다. 그래서 위 마지막 줄에는 `W-FACT-GUARANTEED`가 표시되지 않습니다. `lute trace`, `lute test`, `lute play`는 같은 쓰기, 팩트, 수락을 적용하며, `lute play`는 건너뛴 `::set`과 마찬가지로 각 건너뛰기를 출력합니다:

```
  skip ::give{item="potion"} — when: false
  skip ::use{component="trainerBattle" who="lassMina" …} — when: false
```

내장 레코드로 낮아지는 디렉티브는 자신이 놓인 곳에서 실행되며 `when=`을 거부합니다(`E-UNKNOWN-ATTR`). 여기에는 위 [표](#코어-어휘)의 모든 연출 디렉티브, `::end`, `::label`, 선언적 [`lower:` 레코드](/ko/plugins/manifests/#선언적-낮춤)가 있는 플러그인 디렉티브가 포함됩니다. 이런 디렉티브는 대신 `<match>` 안에 넣으세요. [`<track>`](/ko/language/timeline-and-property-tracks/) 클립 안에서도 거부됩니다(`E-TIMELINE-CONTENT`). 조건부 클립은 논리이기 때문입니다.

<!-- lute-diagnostics unverified="verbatim lute check output; the message's source names E-UNKNOWN-ATTR through a constant rather than a string literal, so the scraper cannot pair quote and code" -->
```
./scenes/dock.lute:14:28: error [E-UNKNOWN-ATTR] `::bg` cannot take `when=`: it lowers to a builtin record and runs where it stands — put it in a `<match>`; `when=` guards `::use`, `::accept`, `::assert`, `::retract`, `::set` and plugin passthrough directives
```

플러그인이 `when`이라는 이름의 속성을 선언하면 가드와 충돌하며, 그 속성을 사용하면 이름을 바꾸라는 `E-UNKNOWN-ATTR`이 발생합니다.
