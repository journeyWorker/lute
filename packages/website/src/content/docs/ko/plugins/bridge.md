---
title: 타입이 지정된 브리지 디렉티브
description: 플러그인이 타입이 지정된 런타임 브리지를 선언하는 방법, 이를 호출하는 디렉티브, 디렉티브가 쓰는 선언된 결과 상태, 동기화 의미론, 결과 슬롯에 대한 match입니다.
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

모든 쓰기 경로는 슬롯의 shape로 선언되어야 **합니다(MUST)**. 호스트는 `sync` 같은 자체 옵션을 둘 수 있으며, 플러그인은 호출의 블로킹 여부를 정하는 예약된 DSL 타이밍 키 `wait`를 다시 선언하지 않습니다([동기화 의미론](#동기화-의미론) 참조). `at`, `duration`, `delay`, `wait`, `timeline`, `timing`, `provenance`, `source`는 코어 스탬프가 소유하는 여덟 이름입니다. 플러그인이 이 중 하나를 속성으로 선언하면 조립 시 [`E-PLUGIN-RESERVED-STAMP-ATTR`](/ko/plugins/manifests/)로 거부됩니다.

호출이 블로킹되는 것(호스트의 답을 기다리는 것)은 `effects.writes` 항목이 bridge 결과를 읽기 때문입니다. `layer: bridge`와 `bridgeCall`을 선언했지만 `fromBridgeResult` 쓰기가 없는 디렉티브는 아무것도 기다리지 않는 레코드로 컴파일되므로, 0.41.1부터 선언 위치에서 [`E-BRIDGE-NO-RESULT-WRITE`](/ko/reference/diagnostics/#e-bridge-no-result-write)가 되고 쓰기를 추가하거나 `layer: bridge`를 지울 때까지 프로젝트가 컴파일되지 않습니다.

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

## 동기화 의미론

결과를 쓰는 브리지 호출은 블로킹되며, 그 결과 쓰기는 **호출 이후의 확정 할당**입니다(0.49부터). *블로킹 브리지 호출*은 선언에 `fromBridgeResult` 값을 가진 `effects.writes` 항목이 하나 이상 있고 유효한 코어 `wait`가 `false`가 아닌 플러그인 디렉티브 호출입니다. 체커는 보호되지 않은 블로킹 호출의 해석된 각 결과 경로를 호출 시점부터 할당된 것으로 취급하므로, 위의 `<match>`는 `E-MAYBE-UNSET` 없이 `scene.minigame.service01.rank`를 읽으며 그 arm 하나를 지우면 `E-NONEXHAUSTIVE`가 보고됩니다. `when=` 가드 아래에서는 쓰기가 가드된 분기에만 속합니다.

`wait`는 언어의 블로킹 키입니다. **`wait="false"`**이면 호출은 fire-and-continue이고, 쓰기는 할당된 것으로 가정되지 *않으며*, 이어지는 결과 읽기는 `E-MAYBE-UNSET`을 유지합니다.

위의 `sync` 속성 같은 호스트 옵션은 플러그인 자체의 것입니다. **Lute는 `sync`를 읽지 않습니다.** 호스트 옵션이 "실행하고 계속 진행"을 뜻하는 디렉티브(`sync="false"`)는 반드시 `wait="false"`로도 작성해야 합니다. 그렇지 않으면 호스트는 결과를 기다리지 않는데 체커는 결과가 할당된 것으로 취급합니다.

```lute
::minigame{kind="rhythm" id="marina_service_01" resultKey="service01" sync="false" wait="false"}
```

스토리 로직이 결과에 따라 분기할 때는 블로킹 브리지를 사용하고, 같은 씬에서 결과를 읽는 것이 아무것도 없는 브리지에만 `wait="false"`를 남겨 두세요.

## 호스트가 받는 것

플러그인 레코드, 브리지 요청, `awaitBridge`는 호출의 `attrs`(`fields`)를 담고, 디렉티브에 횡단 [`stampAttrs`](/ko/plugins/manifests/#교차-속성stampattrs) 값이 붙어 있으면 별도의 `stamps` 객체도 담습니다. 비어 있으면 생략되며 `attrs`에 병합되지 않습니다.
