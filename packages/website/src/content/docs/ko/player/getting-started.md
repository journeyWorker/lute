---
title: 플레이어 시작하기
description: Lute 프로젝트를 컴파일하고 player.bindings.yaml을 검사한 뒤 0.47 웹 플레이어를 마운트합니다.
---

이 가이드는 저장소의 `examples/player-demo`를 사용합니다. 자신의 프로젝트에도 같은 단계를 적용할 수 있습니다.

## 1. 프로젝트 컴파일

완전한 번들을 만듭니다. 브라우저 런타임은 `project.index.json`과 그 안에 지정된 모든 아티팩트를 읽으므로 `--all`이 필요합니다.

```console
lute compile --all examples/player-demo -o dist/bundle
```

번들 디렉터리에는 `project.index.json`과 그 안에서 참조하는 모든 아티팩트 JSON 파일이 있어야 합니다. 0.41 런타임 바인딩 `lute_runtime_wasm_bg.wasm`도 제공합니다.

## 2. 바인딩 작성과 검사

`lute.project.yaml` 옆에 `player.bindings.yaml`을 만듭니다. 이 파일은 컴파일된 staging id를 파일에 연결하고 canvas와 효과를 지정합니다. 상대 경로는 `assetsBase` 아래에서 해석하며 기본값은 `assets/`입니다.

```yaml
bindingsVersion: "0.47"
canvas: { width: 1280, height: 720 }
volumes: { silent: 0, quiet: 0.25, normal: 0.5, loud: 0.75, full: 1 }
backgrounds:
  foyer: { image: bg/foyer.png }
characters:
  guide: { sprite: { default: char/guide.png } }
assetsBase: assets/
```

전체 형식은 [바인딩 레퍼런스](/ko/player/bindings/)에 있습니다. 컴파일 뒤 다음 명령을 실행합니다.

```console
lute check-project examples/player-demo
```

`check-project`는 스키마, domain 및 cast 키, 참조 파일, 효과와 flow 계기를 검사합니다. 잘못된 shape는 `E-BINDING-SHAPE`, 알 수 없는 효과나 매개변수는 `E-BINDING-EFFECT`, 없는 asset은 `E-BINDING-FILE`, 프로젝트 어휘 밖의 키는 `E-BINDING-KEY`입니다. `--json`으로 모든 진단을 확인할 수 있습니다.

## 3. 페이지에 마운트

정확한 0.47 패키지를 설치합니다.

```console
bun add @lute-lang/player-web@0.47.2 @lute-lang/player@0.47.2 @lute-lang/runtime@0.47.2 effect@4.0.2 @effect/platform-browser@4.0.2 pixi.js@8.22.0 @pixi/sound@6.0.1
```

`@lute-lang/player`는 `@lute-lang/runtime`과 `effect`를 peer dependency로 선언하므로 페이지가 한 번만 설치하고 모든 패키지가 같은 `effect`를 공유합니다.

```html
<div id="player" tabindex="0"></div>
<script type="module" src="/main.js"></script>
```

```ts
import { mountPlayer } from "@lute-lang/player-web"

const element = document.querySelector<HTMLElement>("#player")!
const player = await mountPlayer(element, {
  bindings: { url: "/player.bindings.yaml" },
  bundleUrl: "/bundle/",
  wasmUrl: "/wasm/lute_runtime_wasm_bg.wasm",
  assetsUrl: "/assets/",
  storageKey: "my-game"
})
await player.act({ type: "newGame" })
```

`bundleUrl`은 `project.index.json` 디렉터리를, `wasmUrl`은 브라우저 wasm을 가리킵니다. `assetsUrl`은 `assetsBase`를 덮어씁니다. `mountPlayer`는 `act`, `view`, `send`, `snapshot`, `restore`, `session`, `program`, `bundle`, `unmount`를 반환하며, 각 Promise 메서드는 해당 Effect를 그대로 실행하고 같은 실패를 reject합니다. `session`, `program`, `bundle`은 핸들의 값이며, `session.current`에는 `program`의 읽기 전용 질의가 받는 상태가 들어 있습니다.

저장한 게임을 새 플레이어에서 재개하려면 마운트할 때 runtime snapshot을 전달하고 전체 save를 restore합니다.

```ts
const save = await player.snapshot()
await player.unmount()
const fresh = await mountPlayer(element, {
  bindings: { url: "/player.bindings.yaml" },
  bundleUrl: "/bundle/",
  wasmUrl: "/wasm/lute_runtime_wasm_bg.wasm",
  snapshot: save.snapshot
})
await fresh.restore(save)
```

## 4. 모든 파일 제공

페이지, JavaScript 번들, 바인딩, 번들 JSON, wasm과 asset은 애플리케이션에서 제공해야 합니다(CORS 정책이 허용하는 URL도 가능합니다).

```text
/player.bindings.yaml
/bundle/project.index.json
/bundle/<every artifact path in project.index.json>
/wasm/lute_runtime_wasm_bg.wasm
/assets/<every bound file>
```

`file://`로 HTML을 직접 열지 마세요. 브라우저가 wasm과 번들을 안정적으로 가져오지 못합니다.

## 5. 데모 실행

저장소 루트에서 CLI를 빌드하고 웹 데모를 실행합니다.

```console
cargo build --release -p lute-cli
bun run --cwd packages/player-web demo
```

이 스크립트는 `examples/player-demo`를 한국어 로케일 번들과 함께 컴파일하고(`lute compile --all examples/player-demo --locales examples/player-demo/locales/bundle.json`) `web/main.ts`를 번들링한 뒤 런타임 wasm을 복사하고 작은 서버를 시작합니다. 다른 `lute` 실행 파일은 `LUTE_BIN`, 포트는 `PORT`로 지정할 수 있습니다.

Settings 화면의 Locale에서 원본 로케일(`en-US`)과 `ko`를 고를 수 있습니다. 바꾸면 진행 중이든 새 게임이든 다음에 표시되는 대사, 선택지, 프롬프트부터 적용됩니다. 번들은 `examples/player-demo`에서 `lute loc export .`로 내보내 `locales/ko.json`으로 번역한 뒤 `lute loc import locales/ko.json -o locales/bundle.json`으로 만든 것입니다. 끝난 회차는 플레이어의 계보(lineage)로 남습니다. 새로고침한 뒤 새 게임을 시작하면 2회차가 1회차의 `user.*` 값과 `prev.run`을 이어받습니다(데모의 첫 장이 둘 다 보여 줍니다).

## 6. 스테이지 레이어

player-web는 스테이지를 고정된 레이어로 그립니다. 아래에서 위 순서입니다.

1. `background` — `::bg` 배경.
2. `camera` — 배우(`::actor`). 프레이밍과 카메라 이동은 이 레이어를 변형합니다.
3. `cg` — 이벤트 일러스트(`::cg`).
4. `video` — `::video` 재생.
5. `dialogue` — 대사 창.
6. `choice` — 선택지와 허브 메뉴.
7. `overlay` — 화면과 메뉴(타이틀, 저장/불러오기, 백로그, 설정, 엔딩).
8. `transition` — 장면 전환.

따라서 `::cg`는 스테이지의 배우를 덮고, 대사 창과 UI 아래에 남습니다. `cgLayouts`는 레이어 안에서 CG의 배치(`fit`, `scale`, `x`, `y`)만 정하며 순서는 바꾸지 않습니다.

### 카메라 전환

`camera` 레이어는 평소에 완전히 보이는 상태(알파 1, 마스크 없음)입니다. `move`가 있는 `::camera` 큐는 진행 중인 전환을 멈추고 프레이밍을 적용한 뒤 이동을 재생합니다. `move`는 `transition`보다 우선합니다. `transition`이 있으면 효과에 따라 새 프레이밍이 나타나는 방식이 정해집니다.

- `fade`, `dissolve`, `fadeOut`, `wipeLeft`/`wipeRight`/`wipeUp`/`wipeDown`, `slideLeft`, `slideRight`, `slideOut`은 지속 시간의 앞 절반 동안 배우를 감추고, 프레이밍과 초점을 적용한 다음, 뒤 절반 동안 배우를 다시 보여 줍니다(알파가 1로 돌아오거나, 와이프로 드러나거나, 슬라이드로 제자리에 돌아옵니다). 와이프는 배우가 걸친 범위만이 아니라 스테이지 전체를 쓸고 지나갑니다.
- 그 밖의 효과는 프레이밍을 적용한 뒤 재생합니다.

어느 쪽이든 큐가 끝나면 배우는 새 프레이밍으로 보이는 상태(알파 1, 마스크 없음)입니다. `cut`, `none`, 지속 시간 0, 즉시 큐(건너뛰기, 불러오기)는 프레이밍을 바로 적용합니다. 이미 시작된 전환은 이야기가 먼저 선택지에 이르더라도 끝까지 재생되며, 이후의 카메라 큐가 전환을 멈춥니다. 전환 도중에 불러오거나 되감으면 전환이 멈추고 복원된 스테이지에서 배우가 보입니다.

## 7. 타이틀 화면

타이틀 화면의 제목은 바인딩에 `ui.title.text`가 있으면 그 값입니다. 없으면 제목을 그리지 않습니다(player-web은 임시 이름을 표시하지 않습니다). `ui.title.background`와 `ui.title.music`은 선택 사항이며, 없으면 배경을 그리지 않고 음악도 재생하지 않습니다.

```yaml
bindingsVersion: "0.47"
ui:
  title: { text: "The Signal at Low Tide", background: ui/title.png, music: ui/title.wav }
```
