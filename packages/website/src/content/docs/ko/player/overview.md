---
title: 플레이어 개요
description: Lute 0.41의 렌더러 독립 플레이어 코어와 PixiJS 웹 플레이어, 패키지와 아키텍처를 설명합니다.
---

Lute 0.41은 두 플레이어 패키지를 제공합니다. [`@lute-lang/player`](https://www.npmjs.com/package/@lute-lang/player)는 렌더러 독립 코어로, 진행, 큐 스케줄링, 상호작용 상태, 런타임 세션, 저장, 읽기 기록, 백로그, 자동/스킵, 롤백, 호스트 브리지를 담당합니다. [`@lute-lang/player-web`](https://www.npmjs.com/package/@lute-lang/player-web)는 PixiJS 8, Web Audio, 화면과 입력을 사용하는 브라우저 구현을 제공합니다.

두 패키지도 스토리를 컴파일하지 않습니다. `lute compile --all`로 프로젝트를 컴파일하고 결과 번들과 런타임 wasm, 검사된 `player.bindings.yaml`을 플레이어에 제공합니다.

## 아키텍처

```text
PlayerAction ──► playerMachine ──MachineEvent──► director ──► Presenter
(input)          (pure state)                    (time/effects) ├─ AudioPort
                                                                  └─ AssetLoader
                                  │
                    ┌─────────────┼─────────────┐
                    ▼             ▼             ▼
                 session       cues +       stores
                 runtime       scheduler     saves/profile
                                  │
                        runtime bundle + stage

player-web: mount ─► PixiApp ─► World/PixiPresenter
                         ├─ PixiAudio
                         ├─ PixiAssets
                         ├─ UI screens
                         └─ keyboard/pointer/touch/gamepad input
```

런타임은 스토리 상태, stage, await와 결과를 소유합니다. 플레이어는 입력을 보내고 반환된 레코드를 렌더링하며 런타임 결과를 직접 선택하지 않습니다. machine은 순수하고 명령을 내보냅니다. director는 타이머와 Effect fiber를 관리하고 포트를 호출하며 저장 또는 롤백 뒤 런타임 stage를 복원합니다. 저장에는 런타임 스냅숏과 플레이어 백로그/로그 데이터가 들어가며 actor 스냅숏은 들어가지 않습니다.

## 패키지

| 패키지 | 책임 | 공개 진입점 |
| --- | --- | --- |
| `@lute-lang/player@0.41.0` | 렌더러 독립 코어, 포트, 바인딩 디코더, 큐, 스케줄러, 세션, 저장소 | `LutePlayer.make(options)` |
| `@lute-lang/player-web@0.41.0` | PixiJS/Web Audio presenter, asset, 화면, 입력, 브라우저 마운트 | `mountPlayer(element, options)` 또는 `LuteWeb.mount` |
| `@lute-lang/runtime@0.41.0` | 번들 로딩, wasm 런타임 세션, 이벤트 계약, 스냅숏과 질의 | `@lute-lang/runtime/browser` |

코어 API는 호스트에 종속되지 않습니다. 다른 렌더러를 사용하려면 `Presenter`, `AudioPort`, `AssetLoader`를 구현하고, 브리지 호출, plugin 레코드, idle 정책에는 `PlayerHost`를 사용합니다. 플레이어 핸들은 idle이나 엔딩에서 런타임 입력을 보내고(`send`), 호스트 체크포인트에서 이어 가며(`restore`), 호스트 질의를 위해 런타임 `session`과 그 `program`을 노출합니다. 플레이어는 seed, 스냅숏, 호스트가 만든 런타임 세션 중 하나에서 시작할 수 있습니다. 자세한 내용은 [커스텀 호스트](/ko/player/custom-hosts/)를 참고하세요. 전체 staging 어휘는 [바인딩](/ko/player/bindings/)에서, 브라우저 경로는 [시작하기](/ko/player/getting-started/)에서 설명합니다.
