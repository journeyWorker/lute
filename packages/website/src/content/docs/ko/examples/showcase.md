---
title: 전체 사양 쇼케이스
description: 프런트매터와 프로필, 네 가지 상태 단계, 스키마 구성, <branch>/<match>/<hub>, 타임라인, 콘텐츠 컴포넌트, 플러그인 브리지를 처음부터 끝까지 구동하고 0.9.0에서 깔끔하게 검사되는 완전한 기능의 에피소드와 hub 및 when-is 보조 파일을 포함한 독립 실행형 쇼케이스 프로젝트를 둘러봅니다.
---

[`docs/examples/showcase/`](https://github.com/journeyWorker/lute/tree/main/docs/examples/showcase) 프로젝트는 구현된 언어 기능 전반을 처음부터 끝까지 구동하는 하나의 독립 실행형 시나리오입니다. 프런트매터와 프로필, 네 가지 상태 단계, 스키마 구성, `<branch>`/`<match>`/`<hub>`, 타임라인, 콘텐츠 컴포넌트, 플러그인 브리지를 모두 다루며 깔끔하게 검사됩니다.

```sh
lute check docs/examples/showcase/episode01.lute --project docs/examples/showcase   # exit 0, 0 warnings
```

여기에는 export 종류 여섯 가지(`directives/`, `state/` (형태 + 템플릿), `providers/`, `bridge/`, `assetkinds/`, `defs/`)를 다루는 완전한 플러그인(`showcase.pack`)과 base/child 스키마 쌍, 재사용 가능한 콘텐츠 컴포넌트, 고정된 `castId` 카탈로그가 들어 있습니다. 세 개의 씬 파일이 이를 구동합니다.

## `episode01.lute` — 전체 기능 지도

이 에피소드는 모든 요소를 하나로 연결합니다. 씬 로컬 플러그인 옵션을 지정하는 루트 `profile` 선택, `extends:` 구성과 함께 사용하는 `uses:` 스키마 import, 네 가지 상태 단계 모두, `when` 가드 및 `into=` 실행 기록 단축 표기를 사용하는 `<branch>`/`<choice>`, `<match>`/`<when>`/`<otherwise>`, 네 트랙 `<timeline>`, 그리고 `providerRef` id와 `assetKind` id를 속성에 결합한 플러그인 브리지 디렉티브 `::serve`를 사용합니다.

```lute
::serve{kind="rhythm" performer="marina_star" poster="PT.marina_star.0" resultKey="debut" sync="true"}

<match subject="scene.serve.debut.rank">
  <when is="gold">
    @marina{code="0020" emotion="delighted" variant="1"}: A perfect service!
    ::set{scene.affect.marina += 1}
  </when>
  <when is="silver|bronze">
    @marina{code="0030" emotion="content" variant="0"}: Not bad at all, Mr. Fixer.
    ::set{run.affection += 1}
  </when>
  <otherwise>
    @marina{code="0040" emotion="shy" variant="0"}: Shall we try once more?
  </otherwise>
</match>
```

## `hub-demo.lute` — 재방문 허브, `<when is>`, 보간

검사를 통과할 뿐 아니라 *컴파일되는* 비에피소드 보조 파일입니다. 재방문 `<hub>`(여기에는 `once` 선택지, `when`으로 가드된 선택지, `exit` 선택지가 있어 막다른 끝이 생기지 않아야 한다는 조건을 충족합니다), 허브의 암시적 기록 enum에 대한 `<when is="…">` 리터럴 패턴 분기, `{{…}}` 콘텐츠 보간을 보여 줍니다.

```lute
<hub id="chatWithMarina">
  <choice id="askCoffee" text="Ask about the coffee" once>
    @marina{code="0020" emotion="content" variant="0"}: House blend. Bold, like the clientele.
  </choice>
  <choice id="compliment" text="Say she was kind earlier" when="@helped">
    @fixer{code="0030"}: You were gentle about it before. It stuck with me.
    ::set{scene.affect.marina += 1}
  </choice>
  <choice id="leave" text="Head out" exit>
    @fixer{code="0040"}: I'd better get moving.
  </choice>
</hub>
```

## `when-is-demo.lute` — 일반 enum에 대한 `<when is>`

hub-demo의 짝인 보조 파일입니다. 동일한 `<when is="…">` 리터럴 패턴 분기를 사용하지만, 이번에는 일반적인 씬 로컬 유한 enum을 대상으로 하며 **대안 분기**도 포함합니다. 기본값이 있는 enum은 확실히 할당되므로, `is`를 모두 다루면 `<otherwise>` 없이도 완전합니다.

```lute
<match subject="scene.mood">
  <when is="calm">
    @fixer{mono}: Steady breathing. Nothing to prove tonight.
  </when>
  <when is="tense">
    @fixer{mono}: Shoulders drawn tight — I should tread carefully.
  </when>
  <when is="joyful|playful">
    @fixer{mono}: Light in the eyes, whichever way it tilts.
  </when>
</match>
```

## 여기서 달라진 0.8.0

세 씬 모두 `luteVersion: "0.8.0"`으로 다시 기록되었습니다. 각 씬에서 한 줄만 달라졌고 플러그인은 건드리지 않았지만 여전히 깔끔하게 검사됩니다. artifact 자체의 `lute` / `irVersion` 스탬프 외에 컴파일된 출력에서 두 가지가 바뀌었으며, 둘 다 여러분이 작성하는 것은 아닙니다.

**`sections`.** 작성한 `## ` 제목은 예전에는 파싱된 후 버려졌습니다. IR 전달자가 없는 유일한 작성 구조였던 셈입니다. 이제는 컴파일 과정에서 보존되므로 `episode01.lute`가 명령 배열 옆에 여섯 개의 섹션 제목을 내보냅니다.

```json
"sections": [
  { "section": 1, "heading": "Venny's Again" },
  { "section": 2, "heading": "The Rehearsed Entrance" },
  { "section": 3, "heading": "The Service Bridge" },
  { "section": 4, "heading": "The Approach" },
  { "section": 5, "heading": "What Was Recorded" },
  { "section": 6, "heading": "Content Gates" }
]
```

**`capabilitySnapshot`.** `::end`가 추가되면서 `lute.core` 스냅샷이 디렉티브 여덟 개에서 아홉 개로 늘어났고, 해시가 스냅샷의 모든 디렉티브를 접어 넣으므로 여기의 모든 artifact가 새 스탬프를 가집니다. `showcase.pack`에는 아무 변화가 없습니다. 숫자가 바뀐 것은 core 어휘가 달라졌기 때문입니다.

`position`은 이동하지 않았습니다. 프로젝트의 모든 섹션은 100개보다 훨씬 적은 position을 내보내며, 이것이 바로 [uniform-width 규칙](/ko/tooling/runtime-contract/)이 보존하도록 설계된 바이트 안정성입니다.

이 쇼케이스에는 나머지 0.8.0 기능이 포함되지 않습니다. `::end`도, `after: active(…)`도, quest 문서도, `showcase.pack`의 `stampattrs/` export도 없습니다. 이런 기능은 [Core 디렉티브](/ko/language/directives/), [퀘스트 및 씬](/ko/language/quests-and-scenes/), [매니페스트](/ko/plugins/manifests/)에 있습니다.

## 여기서 달라진 0.9.0

언어 0.9.0에서는 콘텐츠 어휘의 **멤버**를 컴파일러 밖으로 옮겼습니다. `lute.core`는 어휘 슬롯(`emotion`, `action`, `anchor`, `costume`, `mood`, `volume`, `musicPlayback`, `vfxType`, `framing`, `cameraMove`, `transition`, `cgLayout`, `sequence`, `textStyle`)을 선언하지만 멤버는 제공하지 않으므로, 프로젝트가 직접 선언하지 않으면 `E-DOMAIN-UNKNOWN`이 발생합니다.

쇼케이스는 스키마가 아니라 **`showcase.pack`**을 통해 어휘를 선언합니다. `plugin.yaml`에 나열된 새로운 `enums/` export입니다. 이것이 핵심입니다. 플러그인 `enums` export는 엔진이나 장르 팩이 활성화한 모든 프로젝트에 어휘를 제공하는 방법이며, capability snapshot(`lute context --json` → `enums`, 따라서 `capabilitySnapshot`)에도 나타납니다. 다른 경로도 두 가지 있는데, 둘 다 **프로젝트** 데이터입니다. 하나는 `uses:`/`extends:`를 통해 도달하는 프로젝트 스키마의 `enums:` 블록입니다(`docs/examples/base.schema.yaml`에서 확인할 수 있습니다). 다른 하나는 문서 **자체 프런트매터**의 `enums:` 블록입니다. 프로젝트가 없는 단일 파일에 열려 있는 유일한 경로이므로 [playground](/playground/)가 이를 사용합니다. 두 경로 모두 별도의 `projectEnums` 키 아래에 나타나고 대신 컴파일된 artifact의 `enums` 배열로 전달됩니다. 한 루트에서 플러그인과 두 프로젝트 경로 중 하나를 통해 같은 슬롯을 선언하면 `E-DOMAIN-DUP`이며 플러그인이 우선하므로, 각 루트는 슬롯마다 하나의 경로를 선택합니다. inline과 imported는 중복이 아닙니다. inline이 우선하며 imported 멤버의 상위 집합을 다시 선언해야 합니다.

여기서 볼 수 있는 결과는 다음과 같습니다. `capabilitySnapshot`이 다시 이동했습니다(core의 어휘가 비워졌고 *그리고* `showcase.pack`에 export가 추가되었습니다). artifact의 `enums` 배열은 계속 **없습니다**. 플러그인이 제공하는 어휘는 문서별 데이터가 아니라 capability surface이기 때문입니다. `0.9.0`에서는 세 씬이 `luteVersion: "0.9.0"`으로 다시 기록되었고 `irVersion`도 `0.9.0`으로 읽혔는데, 이는 릴리스가 모든 버전 축을 다시 맞추기 때문일 뿐입니다. artifact 필드는 추가되거나 이름이 바뀌거나 이동하지 않았으므로 IR `0.9.0`은 `0.8.0`과 형태가 동일했고, `0.8`을 기준으로 게이트하는 엔진은 게이트 범위만 넓히면 됐습니다. 씬은 이제 `luteVersion: "0.10.0"`을 가지며, IR `0.10.0`에서는 형태가 실제로 바뀌었습니다. `provenance.reason`이 `provenance.explanation`으로 바뀌었으므로, `0.9`를 기준으로 게이트하는 엔진은 게이트 범위를 넓히는 동시에 해당 필드 하나의 이름도 바꿔야 합니다.

import된 `stinger` 컴포넌트도 이제 독립 실행형으로 검사할 때와 정확히 동일하게 검사됩니다. 예전에는 전체 문서 패스 다섯 개가 문서 루트에서만 실행되고 imported 컴포넌트 본문은 건너뛰었지만, 이제는 다섯 개 모두 해당 본문에도 실행됩니다. 따라서 `stinger.component.lute`의 `::music`/`::vfx` 값은 직접 사용할 때뿐 아니라 `::use`를 통해서도 검증됩니다.

플러그인 매니페스트, 스키마, 컴포넌트, 카탈로그, 전체 기능→줄 매핑을 포함한 완전한 프로젝트는 저장소의 [showcase 디렉터리와 README](https://github.com/journeyWorker/lute/tree/main/docs/examples/showcase)를 참조하세요.
