---
title: 플레이어 컨트롤
description: 기본 키보드, 마우스, touch와 gamepad 매핑, auto, skip, backlog와 저장 컨트롤입니다.
---

`@lute-lang/player-web`은 device event를 코어의 device-independent `PlayerAction`으로 변환합니다. 게임에 다른 매핑이 필요하면 custom `InputMap`을 입력 레이어에 제공합니다. 메뉴가 열려 있을 때 UI widget이 키보드 event를 소비합니다.

## 기본 키보드 매핑

| 키 | 동작 |
| --- | --- |
| `Enter`, `Space` | Advance; 입력 중이면 line을 reveal하고 표시 후에는 release합니다 |
| `A` | auto mode 전환 |
| `S` | skip mode 전환 |
| `Control` (누르는 동안) | skip 유지; 놓으면 normal로 돌아갑니다 |
| `Escape` | 메뉴 열기 |
| `PageUp`, 마우스 휠 위 | backlog 열기 |
| `F5` / `F9` | quick save / quick load |
| `H` | UI 표시/숨김 |
| `Backspace` | input point 하나 롤백 |
| `1`–`9` | 제시된 경우 option id `1`–`9` 선택 |

## 마우스와 touch

- 기본 클릭 또는 tap은 advance합니다. 입력 중인 line은 먼저 reveal하고 표시된 line은 release합니다.
- 아래 방향 swipe와 휠 위는 backlog를 엽니다.
- 기본 클릭을 500ms 길게 누르면 메뉴를 엽니다.
- 가운데 클릭은 UI를 숨기고 오른쪽 클릭은 메뉴를 엽니다.
- touch도 같은 pointer event를 사용합니다.

## Gamepad

표준 매핑 gamepad 중 첫 번째 연결 장치를 polling하고 button 누름 edge에서 동작을 발생시킵니다.

| 버튼 | 동작 |
| --- | --- |
| A (0) | Advance |
| B (1) | 메뉴 열기 |
| Y (3) | Backlog 열기 |
| LB (4) | 롤백 |
| RB (5, 누르는 동안) | Skip 유지 |
| Start (9) | 메뉴 열기 |

연결이 끊기면 edge 검출이 초기화됩니다.

## 모드

auto mode는 voice가 끝난 뒤(있다면) `settings.autoDelay`초를 기다리고 표시된 line을 release합니다. 입력이나 overlay는 대기 중인 auto를 취소합니다. skip mode는 `read`(기본값: 읽지 않은 line에서 중지) 또는 `all`입니다. 두 모드 모두 choice, bridge, idle에서 멈추며 skip은 시각 효과를 끝 상태로 즉시 이동시킵니다.

machine의 line 진행은 두 단계입니다. 입력 중 advance는 `line.reveal`을, 완전히 표시된 뒤 advance는 `line.release`를 발생시킵니다. 확인 입력을 직접 받는 presenter는 `confirmsLines: true`를 선언하거나(타이핑 중 advance를 보관했다가 line이 표시되면 해제로 적용), 호스트가 `linePacing: "presenter"`를 선택할 수 있습니다. 이때는 presenter의 `showLine` 완료가 line을 해제하고 advance와 auto 타이머는 line에 관여하지 않습니다.

## Backlog와 읽기 기록

backlog를 열면 overlay와 auto/countdown 동작이 일시 중지됩니다. backlog entry에는 `lineId`, voice key, speaker name, text, `hasVoice`가 있습니다. profile은 읽은 line id set을 저장하며 host는 `replayVoice`로 음성을 다시 재생할 수 있습니다.

## 저장과 quick save

`F5`는 `quickSave`, `F9`는 `quickLoad`를 보냅니다. 메뉴는 번호 저장/불러오기 슬롯, overwrite 및 delete 확인, auto 슬롯을 제공합니다. `mountPlayer`는 `storageKey`를 사용해 브라우저 local storage에 저장합니다. 자세한 저장 형식은 [커스텀 호스트](/ko/player/custom-hosts/#저장과-프로필-데이터)를 참고하세요.

## Custom UI용 action 이름

코어는 `advance`, `toggleAuto`, `toggleSkip`, `skipHold`, `rollback`, `openBacklog`, `openMenu`, `openSave`, `openLoad`, `openSettings`, `quickSave`, `quickLoad`, `hideUI`, `choose`, `close`, `newGame`, `continue`, `toTitle`, `saveSlot`, `loadSlot`, `deleteSlot`, `confirm`, `pickIdle`, `replayVoice`, `setSettings`, `send`(idle이나 엔딩에서의 런타임 입력, 거부되면 기록만 하고 아무것도 바꾸지 않음)를 받습니다.
