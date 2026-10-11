---
title: 타입이 지정된 브리지 디렉티브
description: 플러그인이 타입이 지정된 런타임 브리지를 선언하는 방법, 이를 호출하는 디렉티브, 디렉티브가 쓰는 선언된 결과 상태, 동기화 시점, 결과 슬롯에 대한 match입니다.
---

**브리지(bridge)**는 엔진이 소유하는 타입 지정 런타임 호출입니다 — 미니게임, 서비스, 외부 비트가 그 예입니다. DSL은 데이터만 방출하고, 엔진이 브리지를 실행하며, 스토리 제어 흐름은 디렉티브가 쓰는 **선언된 상태만** 관찰하고 원시 브리지 출력은 관찰하지 않습니다. 이것이 언어를 전체적으로 유지합니다. 브리지 호출은 임의의 도구 호출이 아니라 데이터입니다.
선언적 `lower: { record, fields }`나 이름 붙은 코어 내장 낮춤이 없는 패스스루 플러그인 디렉티브는 일반 `kind: "plugin"` 레코드(`family: "plugin"`)를 방출합니다. 이 레코드는 capability 조립에서 해석된 선택적 `plugin` 소유자 id(씬에 직접 작성하지 않음)를 `tag`, 타입이 지정된 `fields`, 선언된 `effects`와 함께 가질 수 있습니다:

```json
{
  "kind": "plugin",
  "family": "plugin",
  "position": "001-0400",
  "tag": "host-panel",
  "plugin": "game.presentation",
  "fields": {
    "assetId": "CG.chapter_one.first_choice",
    "resultKey": "first-choice",
    "text": "First choice"
  }
}
```

호스트는 **`(plugin, tag)`** 쌍으로 이 확장 레코드를 디스패치합니다. 코어 소유이거나 소유자를 해석하지 못한 경우 `plugin`은 생략됩니다. `lower: { record: ... }`를 선언한 프레젠테이션 디렉티브는 여전히 안정적인 코어 스테이징 레코드 종류로 낮춰집니다. 소유자 필드는 패스스루 플러그인 레코드에만 해당합니다.


## 브리지 capability 선언

`bridge/*.yaml` 파일은 서비스와 타입이 지정된 결과 필드를 이름 붙입니다. 이 결과 필드는 디렉티브가 씬 상태에 되돌려 쓸 수 있는 **유일한** 값입니다.

```yaml
bridgeCapabilities:
  - service: minigame
    operation: play
    replay: recorded          # recorded | deterministic | none
    result:
      - { name: score,   type: int }
      - { name: rank,    type: { enum: [fail, bronze, silver, gold] } }
      - { name: cleared, type: bool }
```

## 이를 호출하는 디렉티브

디렉티브는 `bridge: { service, operation }`으로 capability에 연결하고, `slotId` 속성으로 타입이 지정된 결과 슬롯을 열며, `writes`로 쓸 상태를 선언합니다 — 각 쓰기는 `fromBridgeResult`(또는 `{ op: increment }`)에 연결됩니다:

```yaml
directives:
  - name: minigame
    layer: bridge
    attrs:
      - { name: kind,      required: true, type: { enumFromOption: allowedKinds } }
      - { name: id,        required: true, type: { providerRef: minigameId } }
      - { name: resultKey, required: true, type: { slotId: { namespace: scene.minigame } } }
      - { name: sync,      type: bool, default: true }
    semantics: [ "writes.sceneState", "bridgeCall" ]
    state:
      declares:
        - { scope: scene, path: [minigame, { fromAttr: { name: resultKey, slotType: localId } }], shape: minigameResult }
    effects:
      writes:
        - { scope: scene, path: [minigame, { fromAttr: { name: resultKey } }, rank],     value: { fromBridgeResult: rank } }
        - { scope: scene, path: [minigame, { fromAttr: { name: resultKey } }, cleared],  value: { fromBridgeResult: cleared } }
        - { scope: scene, path: [minigame, { fromAttr: { name: resultKey } }, attempts], value: { op: increment, by: 1 } }
    bridge: { service: minigame, operation: play }
```

모든 쓰기 경로는 슬롯의 shape로 선언되어야 **합니다(MUST)**. 블로킹 브리지는 플러그인이 소유하는 `sync` 속성을 사용합니다 — 예약된 DSL 타이밍 키 `wait`가 아닙니다. `at`, `duration`, `delay`, `wait`, `timeline`, `timing`, `provenance`, `source`는 코어 스탬프가 소유하는 여덟 이름입니다. 플러그인이 이 중 하나를 속성으로 선언하면 조립 시 [`E-PLUGIN-RESERVED-STAMP-ATTR`](/ko/plugins/manifests/)로 거부됩니다.

## 예제: `::minigame`과 결과 슬롯에 대한 match

씬은 아무것도 손으로 쓰지 않습니다. 브리지를 실행한 뒤 선언된 슬롯을 읽습니다:

```lute
::minigame{kind="rhythm" id="marina_service_01" resultKey="service01" sync="true"}

<match subject="scene.minigame.service01.rank">
  <when is="gold">
    @marina{code="0030" emotion="delighted"}: Wonderful! A perfect service!
    ::set{scene.affect.marina += 2}
  </when>
  <when is="silver|bronze">
    @marina{code="0040" emotion="content"}: Not bad at all, Mr. Fixer.
    ::set{scene.affect.marina += 1}
  </when>
  <otherwise>
    @marina{code="0050" emotion="shy"}: Shall we try once more? The rhythm takes practice.
  </otherwise>
</match>
```

`rank`는 유한 enum `[fail, bronze, silver, gold]`이므로 `<match>`는 먼저 일치하는 것이 이기며 `<otherwise>`가 `fail`을 포괄합니다.

## 동기화 의미론과 오래된 기본값 함정

`sync="true"`(블로킹 브리지의 기본값)에서는 쓰기가 바로 뒤의 읽기를 **지배**하므로 `<match>`가 생성된 결과를 봅니다. `sync="false"`에서는 그렇지 않습니다. 결과 슬롯은 보통 shape의 기본값으로 정해지기 때문에(예: `rank: fail`), 뒤따르는 읽기는 `E-MAYBE-UNSET`이 **아닙니다** — 결과가 아니라 **기본값**을 조용히 읽습니다.

**현재 이를 진단하는 것은 아무것도 없습니다.** *결과가 생성되기 전 읽기* 검사는 명세되어 있지만 구현되지 않았습니다. 체커가 현재 묻지 않는 데이터 흐름 질문 — "이 읽기가 그것을 생성하는 브리지 호출에 의해 지배되는가?" — 이 필요하기 때문입니다. 이는 매니페스트 검증이 아니라 체커 코어 분석이며, 반쪽짜리로 제공하는 대신 연기되었습니다. 따라서 논블로킹 브리지의 결과를 너무 일찍 읽으면 검사는 깨끗하게 통과하고 컴파일도 깨끗하게 통과하지만 실행은 잘못됩니다.

이 기능이 들어오기 전까지 지킬 규칙은 여러분의 몫입니다: **스토리 로직이 결과에 따라 분기할 때는 블로킹 브리지를 사용하고**, 같은 씬에서 결과를 읽는 것이 아무것도 없는 브리지에만 `sync="false"`를 남겨 두세요.
