---
title: 예제 게임
description: 저장소에 들어 있는 완성된 게임 열네 편 — 미스터리, 로그라이크, 비주얼 노벨, 라이브 옵스, 가챠, Ink 이식작 — 각각이 하나의 프로젝트이고, CI가 경고 0개로 검사하고 테스트와 플레이를 돌립니다.
---

저장소의 [`docs/examples/games/`](https://github.com/journeyWorker/lute/tree/main/docs/examples/games)에는
완성된 게임 열네 편이 있습니다. 각 게임은 이전 Lute 릴리스를 대상으로, 저마다 다른 종류의 작가를 맡은
에이전트가 도그푸드 프로젝트로 쓴 것이고, 지금의 언어로 옮겨 왔습니다. 게임 하나가 프로젝트 하나입니다 —
매니페스트, 스키마, 씬, 로어, 퀘스트, 플러그인, `tests/`, `plays/` — 그리고 README가 그 게임이 쓰는
기능과 기능마다 살펴볼 파일, 실행 명령을 적어 둡니다.

CI는 모든 게임을 `lute check-project --deny-warnings`로 검사하고 테스트와 플레이를 실행하므로, 언어가
바뀌어도 게임이 깨끗하게 유지됩니다. 경고가 물을 법한 설계를 게임이 일부러 택했을 때 — 영구 엔딩, 걸려서
풀리지 않는 `spentBy`, 두 캐스트 항목이 함께 쓰는 역할 이름 — 억제가 아니라 Lute가 그 의도를 위해 마련한
표기로 적습니다.

어디서 시작할까: [`tea-hollin`](https://github.com/journeyWorker/lute/tree/main/docs/examples/games/tea-hollin)이
가장 작고, [시작하기](/ko/getting-started/first-scene/) 페이지가 가르치는 방식 그대로 쓰였습니다. Ink나
Yarn에서 오셨다면 [`ledger`](https://github.com/journeyWorker/lute/tree/main/docs/examples/games/ledger)를
[Ink나 Yarn에서 오셨다면](/ko/guides/coming-from-ink-yarn/)과 나란히 읽으세요.
[`monster-league`](https://github.com/journeyWorker/lute/tree/main/docs/examples/games/monster-league)는
큰 규모의 프로젝트를 보여 줍니다.

| 게임 | 장르 | 보여 주는 것 |
|---|---|---|
| [The Ashen Stair](https://github.com/journeyWorker/lute/tree/main/docs/examples/games/ashen-stair) | 로그라이크 허브 (Hades 계열) | 런 경계와 `prev.run.*`, 엔진 소유 상태와 예약 관계, 보상 종류, 시즌, `for=`로 NPC마다 하는 배웅 |
| [The Drowned Crown](https://github.com/journeyWorker/lute/tree/main/docs/examples/games/drowned-crown) | 로그라이크 (Hades 계열) | run·user 계층, 영구 엔딩(`terminal: { when, persists: true }`), 허브 방문의 비트 사다리, 계기 페이로드 |
| [Ember Road](https://github.com/journeyWorker/lute/tree/main/docs/examples/games/ember-road) | 파티 RPG 챕터 | 동료 호감도와 세력 평판, 길 위의 밤을 세는 일 단위 시계, `::check` 주사위 브리지, 전투 페이로드 |
| [Harbor Days](https://github.com/journeyWorker/lute/tree/main/docs/examples/games/harbor) | 라이브 옵스 생활 시뮬레이션 | 주가 있는 유한 시계, 축제 시즌, 소진 주기, `once:` 옆의 `spentBy`, 디렉티브 효과, 주민별 생일 |
| [Hollow Ward](https://github.com/journeyWorker/lute/tree/main/docs/examples/games/hollow-ward) | 서바이벌 호러 / 탈출 | `raisedWhen:`으로 쓴 규칙 기반 문 그래프, 블로킹 브리지, 엔트리, 종료 운명, 큰 테스트 스위트 |
| [Lamplight](https://github.com/journeyWorker/lute/tree/main/docs/examples/games/lamplight) | 추리 미스터리 / 타임 루프 | 사실과 계층화된 Datalog 규칙, 루프를 넘어 남는 user 계층 증거, 수사 수첩 `series:`, 하위 퀘스트로 이루어진 퀘스트 |
| [Lantern Academy](https://github.com/journeyWorker/lute/tree/main/docs/examples/games/lantern-academy) | 오토메 / 연애 시뮬레이션 (NG+) | 규칙으로 쓴 루트 잠금, `raiseAtStart: true`, 학기를 넘어 남는 user 계층 `cleared` 사실, 데이트·시험 브리지 |
| [The Lighthouse Keeper's Ledger](https://github.com/journeyWorker/lute/tree/main/docs/examples/games/ledger) | 인터랙티브 픽션 (Ink 이식) | Ink를 Lute로: `<return>`이 있는 허브, 방문 카운터, `-> END`로서의 `terminal:`; Ink 원본을 옆에 둠 |
| [Skerry Rock](https://github.com/journeyWorker/lute/tree/main/docs/examples/games/lighthouse-keeper) | 연역 미스터리 (Obra Dinn 계열) | `excludes:`가 있는 계층화 규칙, 단언이 아니라 유도되는 결론, `raiseAtStart`가 있는 시계, 심리 챕터 체인 |
| [Monster League](https://github.com/journeyWorker/lute/tree/main/docs/examples/games/monster-league) | 몬스터 수집 RPG | 규모: 작가 다섯 명, 지역별 스키마, 계기 18개, 사실로 쓴 종 151개, 약 160번 쓰인 템플릿, `sharedName: true` |
| [Seven Days in Marrow Bay](https://github.com/journeyWorker/lute/tree/main/docs/examples/games/seven-days) | 일 단위 시계 비주얼 노벨 | 시계 위치로 쓴 기한, 유도 관계로 쓴 일과표, `newRun`과 `prev.run.*`, 종료 엔딩 셋 |
| [Starfall](https://github.com/journeyWorker/lute/tree/main/docs/examples/games/starfall-gacha) | 라이브 서비스 가챠 (스토리 레이어) | 복각되는 시즌 셋, def로 쓴 콘텐츠 캘린더, 희귀도 하위 종류, 소환 페이로드, `share=` |
| [Summer Station](https://github.com/journeyWorker/lute/tree/main/docs/examples/games/summer-station) | 시계 위의 비주얼 노벨 / 연애 시뮬레이션 | 규칙으로 쓴 일정, `select: sequence` 일과와 이벤트, 폭풍 시즌, `per:` 상태가 있는 하위 종류 |
| [Tea at Hollin Street](https://github.com/journeyWorker/lute/tree/main/docs/examples/games/tea-hollin) | 코지 미스터리 | 가장 작은 게임: 챕터 체인 하나, 분기와 허브, 하루짜리 시계, 기한이 있는 퀘스트 |

저장소를 체크아웃한 뒤 하나를 실행해 보세요:

```sh
lute check-project --deny-warnings docs/examples/games/tea-hollin
lute test docs/examples/games/tea-hollin
lute play docs/examples/games/tea-hollin --script docs/examples/games/tea-hollin/plays/magpie.play.yaml
```
