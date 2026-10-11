---
title: 타임라인과 속성 트랙
description: <timeline> 다중 트랙 안무 블록 — 로컬 시계, 대상/채널/속성 트랙, 오프셋에서의 절대 시점, 트랙당 단일 작성자 규칙입니다.
---

`<timeline>`은 자체 로컬 시계를 가진, 범위가 정해진 비상호작용 **안무 단위**입니다. 연속된 `wait`가 아닌 디렉티브가 느슨한 동시성을 제공한다면, 타임라인은 *시간적 범위*를 제공합니다: 여러 **트랙**이 각각 시간 위치가 지정된 스테이징 클립을 담고, 플레이헤드가 진행되는 동안 모든 트랙이 동시에 재생되며, 전체 블록이 완료될 때까지 뒤의 콘텐츠를 막습니다.

```lute
<timeline duration="1.4">
  <track subject="camera">
    ::camera{focus="marina" framing="tight" duration="0.4"}
    ::camera{move="shake" duration="0.3" at="0.5"}
  </track>
  <track channel="fg">
    ::cg{assetId="CUT.scenarios.marina.s01ep02.01" at="0.5"}
    ::cg{assetId="CUT.scenarios.marina.s01ep02.01" display="hide" at="1.1"}
  </track>
  <track channel="vfx">
    ::vfx{type="whiteOut" transition="flash" at="0.5"}
  </track>
  <track channel="sfx">
    ::sfx{sound="finger_beam_flash" assetId="PLACEHOLDER_finger_beam_flash" at="0.5"}
  </track>
</timeline>
```

*([`docs/examples/marina-s01ep02.lute`](https://github.com/journeyWorker/lute/blob/main/docs/examples/marina-s01ep02.lute)에서 가져옴 — 하나의 네 트랙 비트로 만든 "손가락 빔 공연"입니다.)*

## 트랙과 키

`<track>`은 다음 세 방법 중 하나로 주어진 키로 식별합니다:

- **`subject="camera"`** — 대상 트랙(카메라나 캐릭터처럼 스테이징된 전체 엔티티)입니다.
- **`channel="fg"`** — 채널 트랙(전경 CG, vfx, sfx, 음악 같은 스테이징 채널)입니다.
- **`subject="marina" property="pos"`** — **속성 트랙**: 하나의 대상을 속성 범위가 있는 동시 트랙으로 나눕니다.

각 트랙의 키는 **서로 달라야** 합니다 — 키가 하나인 `subject="camera"` 트랙 두 개는 조용히 충돌하므로 거부되며, 사용할 수 있는 키가 없는 트랙은 `E-TRACK-KEY`입니다.

### 속성 트랙

속성 트랙을 사용하면 한 대상이 여러 속성에서 동시에 애니메이션되도록 할 수 있습니다. 여기서 `marina`는 독립적인 카메라 트랙과 나란히 위치와 불투명도에서 동시에 구동됩니다:

```lute
<timeline duration="1.2">
  <track subject="marina" property="pos">
    ::actor{character="marina" anchor="left" action="slideInLeft"}
  </track>
  <track subject="marina" property="opacity">
    ::actor{character="marina" action="fadeInUp"}
  </track>
  <track subject="camera">
    ::camera{focus="marina" framing="close" duration="0.6"}
    ::camera{move="shake" duration="0.3" at="0.7"}
  </track>
</timeline>
```

*([`docs/examples/property-tracks.lute`](https://github.com/journeyWorker/lute/blob/main/docs/examples/property-tracks.lute)에서 가져옴.)*

키 `marina.pos`, `marina.opacity`, `camera`는 모두 서로 다르므로 체커가 받아들입니다.

## 시간 모델

`<timeline>` 안에서 로컬 시계는 `0`에서 시작합니다. 각 트랙에 대해 다음이 적용됩니다:

- 클립의 **`at`**은 그 시계의 **절대** 시점입니다 — 절대로 상대적인 이동량이 아닙니다.
- `at`이 **생략되면** 해당 트랙의 이전 클립 뒤에 클립이 시작됩니다(첫 클립은 `0.0`에서 시작합니다).
- 클립은 같은 평탄화된 명령 레코드로 낮춰지고, 해석된 시점 순으로 정렬되며, `duration`(또는 해석된 최대 종료 시점)에 최종 배리어가 놓입니다.

트랙이 전혀 없는 `<timeline duration="1.2">`도 합법이며, 언어에 있는 유일한 비트 프리미티브입니다. 최종 배리어만으로 낮춰지고 다른 것은 없으며, 해당 시간만큼 뒤의 콘텐츠를 붙잡습니다. 일시 정지가 목적일 때 사용하세요.

```lute
<timeline duration="1.2">
</timeline>
@narrator: after the beat.
```

```json
{ "kind": "barrier", "family": "control", "position": "001-0100", "timeline": 0, "at": 1.2 }
```

## 제약

트랙은 **스테이징 전용이며 비상호작용**입니다: `::` 스테이징 리프와 상태 표시를 위한 `::set`만 담습니다. 대사, 산문, `<choice>`, `<branch>`, `<match>`는 안에 나타날 수 없습니다 — 그런 요소는 비트가 시계가 아니라 독자에 의해 진행되게 만듭니다. 타임라인은 중첩할 수 없고 퀘스트 문서에는 사용할 수 없습니다. 체커는 타임라인이 지나치게 커지면(대략 트랙 8개 초과, 트랙당 클립 12개 초과 또는 전체 클립 40개 초과) 경고합니다.
