---
title: 프로필 및 활성화
description: 소스 프로필로 capability 플러그인을 선택하고, 타입이 지정된 옵션을 계층화하며, 활성화와 신뢰된 프로젝트/호스트 권한 상한을 구분합니다.
---

**프로필**은 루트 수준 capability 선택기입니다. 즉, 씬에서 어떤 플러그인과 그에 따른 어떤 어휘가 활성 상태인지 결정합니다. 프로젝트는 프로필 그래프와 `defaultProfile`을 `lute.project.yaml`에서 선언하고, 씬은 프런트매터의 `profile:`로 하나를 선택하며 그 위에 씬 로컬 `plugins:`를 추가로 계층화할 수 있습니다.

## 프로필 그래프

```yaml
profiles:
  global:                       # reserved name; inherited by every profile
    plugins: { lute.core: true }
    lint: { unknownDirective: error }
  story:
    plugins: { arcia.vn: true }
  date:
    extends: story
    plugins: { arcia.date: { phoneSurface: enabled } }
  date-minigame:
    extends: date
    plugins:
      arcia.minigame: { resultScope: scene, allowedKinds: [rhythm, timing] }

defaultProfile: story
```

- **`global`은 예약되어 있으며** 다른 어떤 프로필보다 먼저 적용됩니다. 모든 프로필이 이를 상속하므로 `lute.core`의 디렉티브, 속성, 어휘 *슬롯*은 여기에서 비롯됩니다. `0.9.0`부터는 슬롯만 포함합니다. `lute.core`는 빈 `enums:`를 제공하므로 [멤버는 프로젝트가](/ko/language/vocabulary/) 선언해야 합니다.
- **`extends`**는 단일 부모 프로필의 이름을 지정하며, 체인은 반드시 순환이 없어야 합니다. 부모가 자식보다 먼저 적용됩니다.
- **`plugins`**는 플러그인 id를 타입이 지정된 옵션 객체 또는 `true`(기본값으로 정규화됨)에 매핑하는 맵입니다. 유효한 키가 있으면 해당 플러그인이 **활성화**됩니다. `plugins.use` 목록은 없으며 씬 로컬 *비활성화*도 없습니다. `false` 값은 정적 오류(`E-PROFILE-PLUGIN-VALUE`)이지 끄기 스위치가 아닙니다. 플러그인을 제외하려면 해당 플러그인을 활성화하는 프로필을 상속하지 마세요.

## 씬에서 프로필 선택

씬은 프런트매터에서 프로필을 지정합니다(없으면 ⇒ `defaultProfile`). 또한 씬 로컬 `plugins:`를 추가할 수 있으며, 이는 추가만 가능합니다. 여기서는 `allowedKinds`를 좁혀 `rhythm`만 유효한 미니게임 종류가 되도록 합니다.

```lute check="docs/examples/arcia-project/date-minigame.lute"
---
kind: scene
character: marina
season: 1
episode: 5
pov: fixer
profile: date-minigame
plugins:
  arcia.minigame:
    resultScope: scene
    allowedKinds: [rhythm]
---
```

*([`docs/examples/arcia-project/date-minigame.lute`](https://github.com/journeyWorker/lute/blob/main/docs/examples/arcia-project/date-minigame.lute)의 프런트매터입니다. 해당 [`lute.project.yaml`](https://github.com/journeyWorker/lute/blob/main/docs/examples/arcia-project/lute.project.yaml)이 위의 프로필 그래프를 선언합니다.)*

## 해석 및 병합

활성화는 다음의 정확한 순서로 결정론적으로 해석됩니다. `lute.core` → `global` → 선택한 프로필의 `extends` 체인(부모 먼저) → 선택한 프로필 → 씬 로컬 `plugins:` → 의존성 폐쇄. 같은 플러그인의 옵션이 여러 계층에서 설정되면 뒤의 계층이 우선합니다. **스칼라** 값은 재정의하고, **맵** 값은 깊게 병합하며, **목록** 값은 기본적으로 교체합니다. 결과적으로 활성 플러그인마다 정확히 하나의 옵션 객체와 정확히 하나의 [capability 스냅숏](/ko/plugins/manifests/)이 생깁니다.

설치되었지만 **비활성** 상태인 플러그인의 디렉티브, 속성 또는 id를 참조하면 수정 제안("프로필 변경" / "플러그인 활성화")이 포함된 진단이 발생하며, 구문이 조용히 허용되지는 않습니다.

## 활성화는 권한이 아닙니다

프로필은 또한
[`permissions:` 상한](/ko/tooling/capability-permissions/)을 해석된
표면에 적용할 수 있습니다. 이 둘은 서로 다른 축입니다.

- `plugins:`는 어떤 capability가 존재하는지와 타입이 지정된 옵션이 어떻게 병합되는지를 선택합니다.
- `permissions:`는 이미 존재하는 디렉티브, 쓰기, 브리지, 보상 또는
  퀘스트의 사용만 거부할 수 있습니다.

프로젝트 루트, `global`, 부모 및 선택한 프로필의 권한 계층은
결합적으로 적용됩니다. 누락된 필드는 제한이 없으며, 명시적인 빈 목록은
해당 범주 전체를 거부합니다. 씬 로컬 `plugins:`는 계속 추가만 가능하고 권한 거부를 확대할 수
없습니다. 권한 정책은 절대 씬 프런트매터나
플러그인 export에서 허용되지 않습니다.

소스가 작성한 `profile:`은 인증이 아니므로, 신뢰할 수 없는 작성 입력을 허용하는 호스트는
`--permission-profile NAME`을 check, compile,
compile-stream 또는 context에 추가합니다. 이 신뢰된 고정은 지정한
프로필의 권한 상한만 기여하며, 플러그인을 활성화하거나 소스에 작성된
프로필을 변경하지 않습니다.

## 옵션은 병합만이 아니라 검사됩니다

활성화는 제공된 모든 옵션을 소유 플러그인의 `options:` 선언에 대조해 검증합니다. 검사는 **병합 후** 값에서 실행되므로 최종적으로 계층화된 값만 판정됩니다. 자식 프로필이 재정의한 부모 프로필의 잘못된 값은 보고되지 않으며, 최종 값이 잘못되면 몇 개 계층에서 설정했는지와 관계없이 한 번만 보고됩니다.

플러그인이 선언하지 않은 옵션 이름은 **`E-PLUGIN-OPTION-UNKNOWN`**이며, 메시지에 선언된 전체 집합이 표시되므로 대개 오타만 고치면 됩니다.

<!-- lute-diagnostics -->
```
$ lute check scene.lute --project .
lute: E-PLUGIN-OPTION-UNKNOWN: plugin `arcia.bonus` has no option `resultScop` (declared: resultScope, rounds)
```

선언된 타입에 맞지 않는 값은 **`E-PLUGIN-OPTION-TYPE`**입니다. 프로필의 모든 위반은 첫 번째 위반에서 중단하지 않고 한 번에 수집됩니다.

<!-- lute-diagnostics -->
```
$ lute check scene.lute --project .
lute: E-PLUGIN-OPTION-TYPE: option `arcia.bonus.resultScope` expects enum(scene|run), got "galaxy"
lute: E-PLUGIN-OPTION-TYPE: option `arcia.bonus.rounds` expects int, got "three"
```

두 진단 모두 특정 파일의 한 스팬이 아니라 프로젝트를 설명하며, 둘 다 오류입니다. 파일 위치 없이 출력되고 CLI 종료 코드도 설정합니다. 이 진단은 로드 오류 및 해석되지 않은 `depends`와 동일한 해석 채널을 거치므로 CLI, 검사기 및 LSP가 동일하게 관찰합니다.

어떤 계층도 설정하지 않은 옵션은 병합에서 빠지고 선언된 기본값을 사용합니다. 이 기본값은 다시 검사하지 않습니다. 선언이 타입과 값의 출처이기 때문입니다.
