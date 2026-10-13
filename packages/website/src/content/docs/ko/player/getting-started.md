---
title: 플레이어 시작하기
description: Lute 프로젝트를 컴파일하고 player.bindings.yaml을 검사한 뒤 0.41 웹 플레이어를 마운트합니다.
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
bindingsVersion: "0.41"
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

정확한 0.42 패키지를 설치합니다.

```console
bun add @lute-lang/player-web@0.42.0 @lute-lang/player@0.42.0 @lute-lang/runtime@0.42.0 effect@4.0.2 @effect/platform-browser@4.0.2 pixi.js@8.22.0 @pixi/sound@6.0.1
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

`bundleUrl`은 `project.index.json` 디렉터리를, `wasmUrl`은 브라우저 wasm을 가리킵니다. `assetsUrl`은 `assetsBase`를 덮어씁니다. `mountPlayer`는 `act`, `view`, `unmount`를 반환하므로 종료할 때 `unmount()`를 호출해야 합니다.

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

이 스크립트는 `examples/player-demo`를 컴파일하고 `web/main.ts`를 번들링한 뒤 런타임 wasm을 복사하고 작은 서버를 시작합니다. 다른 `lute` 실행 파일은 `LUTE_BIN`, 포트는 `PORT`로 지정할 수 있습니다.
