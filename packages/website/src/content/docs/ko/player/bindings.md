---
title: 플레이어 바인딩 레퍼런스
description: Lute 0.41 player.bindings.yaml의 모든 섹션, 효과 카탈로그, 매개변수와 check-project 진단입니다.
---

`player.bindings.yaml`은 선택 사항이며 `lute.project.yaml` 옆에 둡니다. `lute check-project`가 컴파일 후 읽습니다. 프로젝트 fingerprint나 런타임 실행에는 영향을 주지 않습니다. 상대 경로는 `assetsBase` 아래에서 해석하며 기본값은 이 파일 옆의 `assets/`입니다.

```yaml
bindingsVersion: "0.44"
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
  title: { background: bg/title.png, music: theme }
  theme: { text: "#ffffff", accent: "#e0c080" }
flow:
  newGame: { occasion: start }
  idle: { menu: [{ label: Foyer, occasion: visit, target: place.foyer }] }
```

## 최상위 섹션

- `bindingsVersion`는 정확히 `"0.44"`입니다. `canvas`에는 양수 `width`와 `height`가 필요하고 `assetsBase`는 선택 사항입니다.
- `assets`는 컴파일된 `assetId`를 파일에 연결합니다.
- `backgrounds`는 `bg.location`을 `image`에 연결하고 `times`는 `bg.time`별 대체 파일을 연결합니다.
- `characters`는 cast id를 키로 합니다. 각 값에는 `sprite.default`, 선택적 `sprite.emotions`, 같은 sprite 형태의 선택적 `costumes`, 선택적 `nameColor`가 있습니다.
- `anchors`, `actions`, `transitions`, `framing`, `cameraMove`, `cgLayouts`, `textStyles`, `sequences`는 선언된 staging domain을 키로 합니다. anchor는 `x`와 선택적 `y`, `scale`을 사용하고, framing은 `zoom`과 선택적 `x`, `y`를 사용하며, CG layout은 `fit: cover|contain`과 선택적 `scale`, `x`, `y`를 사용합니다. text style은 `fontWeight`, `fontStyle`, `color`, `alpha`, `scale`, `letterSpacing`을 받습니다.
- `music.moods`, `sfx`, `vfx`는 컴파일 문서에서 사용하는 값을 키로 합니다. `sequences` 값에는 `video`가 있고 `voice.pattern`은 `{voiceKey}` 경로 템플릿입니다.
- `volumes`(0.42)는 프로젝트 `volume` 도메인의 각 멤버를 gain, 즉 `0`에서 `1` 사이의 유한한 수에 대응시킵니다. 음악 큐의 `volume`은 절대 레벨이므로 플레이어는 그 멤버의 gain으로 재생하고 숫자를 해석하지 않습니다. `volume`이 없는 큐는 gain `1`로, gain이 없는 멤버는 전체 볼륨이 아니라 `0`으로 재생됩니다. 알 수 없는 키는 `E-BINDING-KEY`, 키가 없는 멤버는 `W-BINDING-MISSING`, `[0, 1]` 범위의 유한한 수가 아닌 gain은 그 값 위치의 `E-BINDING-SHAPE`입니다.
- `ui`에는 `font`, title의 `background`와 `music`, 임의의 theme 색상 이름이 들어갑니다.
- `flow.newGame`은 하나의 계기를 발생시키며 `flow.idle`은 `{label, occasion, target?}` 메뉴 목록 또는 `raise: {occasion, target?}`입니다.

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

domain 키 바인딩은 병합된 프로젝트 domain의 구성원이어야 합니다. `characters` 키는 선언된 cast id여야 합니다. flow 계기와 계기 target도 선언되어야 합니다. 위반은 가까운 이름 제안이 있다면 함께 `E-BINDING-KEY`로 보고합니다.

선언된 domain 구성원, stage 위 cast 구성원 또는 컴파일된 사용처에 바인딩이 없으면 `W-BINDING-MISSING`입니다. asset/background/mood/sfx/vfx 사용 섹션의 바인딩을 컴파일 문서가 사용하지 않으면 `W-BINDING-UNUSED`입니다. 진단은 `player.bindings.yaml`에 연결되어 `lute check-project --json`에 표시됩니다.

정식 저장소 레퍼런스는 [`docs/player/bindings.md`](https://github.com/journeyWorker/lute/blob/main/docs/player/bindings.md)입니다.
