---
title: 플레이어 바인딩 레퍼런스
description: Lute 0.41 player.bindings.yaml의 모든 섹션, 효과 카탈로그, 매개변수와 프로젝트 전체 check 진단입니다.
---

`player.bindings.yaml`은 선택 사항이며 `lute.project.yaml` 옆에 둡니다. 프로젝트 전체 `lute check`(`lute check --project <DIR>`, 문서 피연산자 없음)가 컴파일 후 읽습니다. 프로젝트 fingerprint나 런타임 실행에는 영향을 주지 않습니다. 상대 경로는 `assetsBase` 아래에서 해석하며 기본값은 이 파일 옆의 `assets/`입니다.

```yaml
bindingsVersion: "0.52"
canvas: { width: 1920, height: 1080 }
assetsBase: assets/
assets: { theme: audio/theme.ogg }
backgrounds:
  foyer: { image: bg/foyer.png, times: { night: bg/foyer-night.png } }
characters:
  guide:
    sprite: { default: char/guide.png, emotions: { happy: char/guide-happy.png } }
    costumes: { coat: { default: char/guide-coat.png } }
    nameColor: "#e0c080"
anchors: { default: { x: 0.5 } }
actions: { enter: { effect: fadeIn, duration: 0.4 } }
transitions: { fade: { effect: fade, duration: 0.6 } }
cameraMove: { shake: { effect: shake, duration: 0.5, strength: 12 } }
framing: { closeUp: { zoom: 1.35 } }
cgLayouts: { full: { fit: cover }, inset: { fit: contain, scale: 0.6 } }
textStyles: { emphasis: { fontWeight: bold }, whisper: { alpha: 0.6 } }
vfx:
  flash: { effect: flash, color: "#fff", duration: 0.3 }
  lanterns: { effect: custom }
  tideRise: { effect: custom, name: tide-rise, duration: 2 }
music: { moods: { calm: audio/calm.ogg } }
volumes: { silent: 0, quiet: 0.25, normal: 0.5, loud: 0.75, full: 1 }
sfx: { door: audio/door.ogg }
sequences: { doorOpens: { video: video/door-opens.mp4 } }
voice: { pattern: "voice/{voiceKey}.ogg" }
ui:
  font: Noto Sans KR
  title: { text: Brinewake, background: bg/title.png, music: theme }
  ended:
    by: run.ending
    screens:
      restore: { title: "The Bell Rings Again", text: "Brinewake keeps its light.", background: dawnHarbor }
      retire: { title: "Quiet Water", text: "You hand the bell rope to Ivo." }
  theme: { text: "#ffffff", accent: "#e0c080" }
flow:
  newGame: { occasion: start }
  idle: { menu: [{ label: Foyer, occasion: visit, target: place.foyer }] }
```

## 최상위 섹션

- `bindingsVersion`는 툴체인 라인인 `"MAJOR.MINOR"`입니다([`bindingsVersion`](#bindingsversion) 참고). `canvas`에는 양수 `width`와 `height`가 필요하고 `assetsBase`는 선택 사항입니다.
- `assets`는 컴파일된 `assetId`를 파일에 연결합니다.
- `backgrounds`는 `bg.location`을 `image`에 연결하고 `times`는 `bg.time`별 대체 파일을 연결합니다.
- `characters`는 cast id를 키로 합니다. 각 값에는 `sprite.default`, 선택적 `sprite.emotions`, 같은 sprite 형태의 선택적 `costumes`, 선택적 `nameColor`가 있습니다.
- `anchors`, `actions`, `transitions`, `framing`, `cameraMove`, `cgLayouts`, `textStyles`, `sequences`는 선언된 staging domain을 키로 합니다. anchor는 `x`와 선택적 `y`, `scale`을 사용하고, framing은 `zoom`과 선택적 `x`, `y`를 사용하며, CG layout은 `fit: cover|contain`과 선택적 `scale`, `x`, `y`를 사용합니다. text style은 `fontWeight`, `fontStyle`, `color`, `alpha`, `scale`, `letterSpacing`을 받습니다.
- `music.moods`, `sfx`, `vfx`는 컴파일 문서에서 사용하는 값을 키로 합니다. `sequences` 값에는 `video`가 있고 `voice.pattern`은 `{voiceKey}` 경로 템플릿입니다.
- `volumes`(0.42)는 프로젝트 `volume` 도메인의 각 멤버를 gain, 즉 `0`에서 `1` 사이의 유한한 수에 대응시킵니다. 음악 큐의 `volume`은 절대 레벨이므로 플레이어는 그 멤버의 gain으로 재생하고 숫자를 해석하지 않습니다. `volume`이 없는 큐는 gain `1`로, gain이 없는 멤버는 전체 볼륨이 아니라 `0`으로 재생됩니다. 알 수 없는 키는 `E-BINDING-KEY`, 키가 없는 멤버는 `W-BINDING-MISSING`, `[0, 1]` 범위의 유한한 수가 아닌 gain은 그 값 위치의 `E-BINDING-SHAPE`입니다.
- `ui`에는 `font`, 선택적인 `title`, 선택적인 `ended`, 임의의 theme 색상 이름이 들어갑니다. 모든 UI asset 참조는 먼저 `assets:` 키를 확인합니다. 키이면 해당 asset 사용으로 계산되고, 아니면 `assetsBase` 아래 파일로 처리되어 반드시 존재해야 합니다.
- `ui.title`(0.47)에는 선택적인 `text`(타이틀 화면 제목으로 그리는 게임 이름)와 선택적인 `background`, `music`이 있으며, `background`와 `music`은 다른 UI asset 참조와 같은 방식으로 해석합니다. `ui.title`을 쓰면 셋 중 하나 이상을 지정해야 하고 `text`는 빈 문자열이 아니어야 합니다. `ui.title: {}`이나 빈 `text`는 `E-BINDING-SHAPE`입니다. `text`가 없으면 타이틀 화면에 제목을 그리지 않고, `background`나 `music`이 없으면 타이틀 배경을 그리지 않거나 타이틀 음악을 재생하지 않습니다.
- `ui.ended`에는 닫힌 enum/entity domain을 가진 선언된 상태 경로인 `by`와 `screens`가 필요합니다. 각 screen에는 `title`과 `text`가 필요하고 `background`는 선택 사항입니다. 알 수 없는 screen 키는 `E-BINDING-KEY`이며, 화면이 없는 domain 구성원은 조용히 기본 화면으로 돌아갑니다.
- 종료 화면을 표시할 때 player-web은 런타임 view에서 `by` 위치의 항목, 즉 `program.view(state).state[by]`를 읽습니다. 이 항목은 단순 문자열이 아니라 런타임 값 `{ kind, value }`입니다. player-web은 그 항목이 `{ kind: "str" }`이고 `screens`가 그 구성원을 매핑할 때만 `value`를 사용하며, 이때 그 화면의 title, text, 선택적 background를 씁니다. 항목이 없거나, 다른 kind이거나, 매핑되지 않은 구성원이거나, 질의가 실패했거나, `ui.ended`가 없으면 기본 종료 화면을 사용합니다.

## `bindingsVersion`

`bindingsVersion`은 `luteVersion` 규칙을 따릅니다. 값은 `"MAJOR.MINOR"`이며, **툴체인 라인**은 툴체인 버전의 `MAJOR.MINOR`입니다(Lute 0.52.x에서는 `"0.52"`).

- 값이 툴체인 라인과 다르면 프로젝트 전체 `lute check`가 그 값 위치에 `W-BINDINGS-VERSION-STALE`을 보고합니다. 더 오래된 라인이면 툴체인 라인으로 올리라고 안내하며 `lute fix`가 이를 수행합니다. 더 새로운 라인이면 스탬프를 낮추지 말고 툴체인을 업그레이드하라고 안내합니다. `MAJOR.MINOR` 형식이 아닌 값(예: `"0.47.0"`)은 다시 스탬프하라고 안내합니다.
- 선언한 라인보다 새로운 섹션은 그 섹션 키 위치의 `E-BINDINGS-VERSION`이며, 필요한 라인을 알려 줍니다. 예: `` `ui.ended` needs bindingsVersion "0.46" or newer; this file declares "0.45" ``. 값이 `MAJOR.MINOR` 형식이 아니면 섹션을 검사하지 않습니다.
- `lute fix`는 `player.bindings.yaml`의 오래된 `MAJOR.MINOR` 값을 툴체인 라인으로 올리고 파일의 나머지(따옴표와 주석 포함)는 바이트 그대로 둡니다. 더 새롭거나 형식이 잘못된 값(`"0.046"`처럼 앞에 0이 붙은 값 포함)과 하나의 flow 매핑(`{ bindingsVersion: …, … }`)으로 쓴 파일은 그대로 두며, 경고는 `lute fix`가 값을 올릴 수 있을 때만 `lute fix`를 언급합니다.

| 섹션 | 필요한 `bindingsVersion` |
| --- | --- |
| `effect: custom`인 `vfx.<name>` | `"0.41"` |
| `volumes` | `"0.42"` |
| `ui.ended` | `"0.46"` |
| `ui.title.text` | `"0.47"` |
| 그 밖의 모든 섹션 | `"0.40"` |

## 효과

효과 객체에는 `effect` 키와 다음 표의 매개변수만 사용할 수 있습니다.

| 효과 | 매개변수 |
| --- | --- |
| `fade`, `dissolve`, `wipeLeft`, `wipeRight`, `wipeUp`, `wipeDown`, `slideLeft`, `slideRight` | `duration?` |
| `cut`, `none` | 없음 |
| `fadeIn`, `fadeOut`, `slideIn`, `slideOut`, `hop` | `duration?` |
| `shake` | `duration?`, `strength?` |
| `zoom` | `duration?`, `scale?` |
| `pan` | `duration?`, `x?`, `y?` |
| `flash` | `duration?`, `color?` |
| `blur` | `duration?`, `strength?` |
| `tint` | `duration?`, `color?` |
| `custom` (`vfx` 전용) | `name?`, `duration?` |

`vfx` 항목은 `{ effect: custom, name?, duration? }`일 수 있습니다. 카탈로그 밖의 presenter 정의 효과입니다. `name`의 기본값은 바인딩 키이며, player는 이 항목을 effect가 `custom`이고 해당 `name`을 가진 `Vfx` cue로 그대로 전달합니다. `actions`, `transitions`, `cameraMove`는 닫힌 카탈로그를 유지하므로 그곳의 `effect: custom`은 `E-BINDING-EFFECT`입니다.

알 수 없는 YAML 키는 거부됩니다. 알 수 없는 효과나 매개변수는 `E-BINDING-EFFECT`입니다. 잘못된 YAML 또는 shape는 YAML 위치와 함께 `E-BINDING-SHAPE`입니다. `assetsBase` 아래에서 참조 파일을 찾지 못하면 `E-BINDING-FILE`입니다.

## 프로젝트 검사와 진단

domain 키 바인딩은 병합된 프로젝트 domain의 구성원이어야 합니다. `characters` 키는 선언된 cast id여야 합니다. flow 계기도 선언되어야 합니다. 위반은 가까운 이름 제안이 있다면 함께 `E-BINDING-KEY`로 보고합니다.

flow target(`flow.newGame.target`, `flow.idle.raise.target`, `flow.idle.menu[].target`)은 `raiseOccasion`이 받는 계기 target 전체이며, 계기 target domain의 접두사 형태 `<prefix>.<member>`로 씁니다. 접두사 없는 `square`가 아니라 `place.square`처럼 씁니다(lute 0.47.1). 프로젝트 전체 `lute check`는 beat target과 `lute play` 단계 target과 같은 규칙으로 이를 검사합니다. `open:` 엔티티 종류는 어떤 `<prefix>.<id>`든 받고, 닫힌 종류나 `members:` 부분집합은 접두사 아래의 그 구성원만 받습니다. 그 밖의 값은 `E-BINDING-KEY`이며, 메시지는 ``flow target `…`:``로 시작하고 가까운 이름이 있으면 ``did you mean `place.square`?``를 제안합니다. target 없이 선언한 계기(`target:`이 없거나 `target: false`)에 target을 쓰거나, 프로젝트가 선언하지 않은 엔티티 종류의 계기에 target을 써도 `E-BINDING-KEY`입니다.

### flow와 선언된 계기

`flow.newGame`과 `flow.idle`(그 `raise`와 `menu` 항목)은 호스트 설정입니다. 참조 플레이어가 직접 이를 발생시키며, `flow.newGame`은 런타임이 시작된 뒤에, `flow.idle`은 런타임이 idle일 때 발생시킵니다. 0.51부터 플러그인 계기는 `raisedBy: declared`로 선언할 수 있으며([매니페스트](/ko/plugins/manifests/#선언된-occasion-raise) 참고), 이때 호스트는 디렉티브나 보상 종류의 선언된 `raises`를 통해서만 그것을 발생시킵니다. 그런 계기를 발생시키는 `flow.newGame`이나 `flow.idle`은 그 선언을 깨뜨립니다. 체커가 이미 그 계기를 선언된 소스만이 발생시키는 것으로 다뤘기 때문입니다. 이 바인딩은 호스트가 스스로 발생시키기로 한 계기에만 쓰세요.

참조 플레이어는 선언된 raise를 직접 수행합니다. 각 연출이 끝난 뒤 idle 흐름이 실행되기 **전에** 비우는 대기 큐를 사용하므로 `flow.idle` raise가 선언된 raise를 앞지르는 일은 없습니다([커스텀 호스트](/ko/player/custom-hosts/#선언된-raise) 참고). 큐를 담는 플레이어 저장은 형식 `"0.51"`이며, `"0.50"` 저장은 거부됩니다.

선언된 domain 구성원, stage 위 cast 구성원 또는 컴파일된 사용처에 바인딩이 없으면 `W-BINDING-MISSING`입니다. asset/background/mood/sfx/vfx 사용 섹션의 바인딩을 컴파일 문서가 사용하지 않으면 `W-BINDING-UNUSED`입니다. 진단은 `player.bindings.yaml`에 연결되어 `lute check --project <DIR> --json`에 표시됩니다.

정식 저장소 레퍼런스는 [`docs/player/bindings.md`](https://github.com/journeyWorker/lute/blob/main/docs/player/bindings.md)입니다.
