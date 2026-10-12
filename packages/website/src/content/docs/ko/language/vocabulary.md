---
title: 콘텐츠 어휘
description: "컴파일러가 콘텐츠 어휘 슬롯을 선언하고 멤버는 제공하지 않는 방식 — 프로젝트가 감정, 행동, 앵커, 의상, 카메라 프레이밍과 이동, 전환, CG 레이아웃, 시퀀스, 음악 재생, 분위기, VFX 유형과 텍스트 스타일을 선언하는 방법, 세 선언 경로와 우선순위, exits:/stops:/default: 멤버 의미론입니다."
---

Lute는 범용 작성 도구체인이고, 여러분의 게임은 그것을 소비하는 쪽입니다. 따라서 구체적인 감정 목록은 컴파일러에 들어 있지 않습니다 — **컴파일러는 어휘 *슬롯*을 선언하고, 프로젝트가 *멤버*를 선언합니다.** 언어 `0.9.0`부터 내장 `lute.core` 플러그인은 비어 있는 `enums:`만 제공하며, 재정의하거나 반박하거나 우회할 기준 목록은 없습니다.

그 결과 거래의 다른 절반도 따라옵니다: **아무것도 선언하지 않은 슬롯을 사용하면 오류입니다.**

<!-- lute-diagnostics -->
```
error [E-DOMAIN-UNKNOWN] `emotion` is not a declared domain — declare its members in an `enums:`
block in this document's own frontmatter, in a project schema reached through `uses:`, or in a
plugin's `enums` export before using `emotion`
```

`0.9.0` 이전에는 원래 슬롯 여섯 개가 어떤 경로로도 확장할 수 없는 닫힌 멤버를 가졌고, 아무것도 선언하지 않았을 때 `action`은 체커가 완전히 건너뛰었으므로 `action="step-foward"` 같은 오타가 조용히 배포되었습니다. 이제 두 특수 사례는 모두 사라졌습니다.

## 슬롯

*슬롯*은 언어가 작성 위치에 연결하는 이름 붙은 도메인입니다. 이름은 언어의 것이고, 멤버는 여러분의 것입니다.

| 슬롯 | 연결 위치 |
|---|---|
| `emotion` | 콘텐츠 줄 `emotion=`, `::actor{emotion}` |
| `action` | 콘텐츠 줄 `action=`, `::actor{action}` |
| `anchor` | `::actor{anchor}` |
| `costume` | `::actor{costume}` |
| `framing` | `::camera{framing}` |
| `cameraMove` | `::camera{move}` |
| `transition` | `::camera{transition}` |
| `cgLayout` | `::cg{layout}` |
| `sequence` | `::sequence{name}` |
| `mood` | `::music{mood}` |
| `volume` | `::music{volume}` |
| `musicPlayback` | `::music{playback}`; 긴 형식은 현재 음악을 끝내는 멤버를 선택적 `stops:`로 지정할 수 있습니다 |
| `vfxType` | `::vfx{type}` |
| `textStyle` | 줄 텍스트의 [인라인 스타일 스팬](/ko/language/dialogue-and-cast/#인라인-텍스트-수식자) `:name[…]` |

슬롯 이름과 속성 이름이 다른 경우(`::camera{move}`는 `cameraMove`를 채우고, `::music{playback}`은 `musicPlayback`을 채웁니다) 진단은 슬롯 이름을 사용합니다. 카메라, CG, 의상, 시퀀스 슬롯은 코어가 하드코딩하지 않은 스테이징을 프로젝트가 표현하는 방법입니다. 카메라가 흔들리는 프로젝트는 `cameraMove: [shake]`를 선언하고 `::camera{move="shake"}`를 씁니다. [코어 디렉티브](/ko/language/directives/#연출-어휘는-프로젝트가-소유합니다)를 참고하세요.

**어휘가 아닌 것.** `::cg{display}`와 `::video{display}`는 디렉티브에 인라인으로 선언된 `{ enum: [show, hide] }`를 그대로 사용합니다 — 이는 엔진이 디스패치하는 두 멤버의 짝이지 공유 어휘가 아닙니다. 전달 플래그 `{mono}` / `{os}` / `{vo}`, 예약된 `narrator` 화자, 코어 텍스트 수식자 `:pause` / `:speed`, `::end` 태그는 문법입니다.

## 어휘 선언

선언은 `enums:` 블록입니다. 단순 시퀀스는 `{ members: [...] }`의 축약형이므로 `0.9.0` 이전의 모든 블록도 바이트 단위로 그대로 파싱됩니다.

```yaml
enums:
  emotion: [neutral, surprised, delighted, shy, content, angry, sad]
  anchor:
    members: [left, center, right]
    default: center
  action:
    members: [fadeInUp, sway, lean, idle, fadeOut, hide]
    exits: [fadeOut, hide]
  mood: [peaceful, tense, romantic, sad, upbeat]
  volume: [silent, down, normal, up, full]
  musicPlayback: [start, change, stop, resume, fadeOut]
  vfxType: [whiteOut, blackOut, rain, snow, leaves, petals, raindrop]
  framing: [wide, medium, close]
  cameraMove: [pushIn, pullOut, shake]
  transition: [cut, dissolve, fade]
  textStyle: [emphasis, whisper]
```

이 블록으로 가는 경로는 **세 가지**이며, 셋 모두 같은 파서와 같은 검증기를 통과합니다.

1. **인라인** — 사용하는 문서 자체의 프런트매터에 둡니다. `enums:`는 보편적인 프런트매터 키이므로 어떤 문서(씬, 퀘스트, 스키마)든 사용하는 곳에서 바로 어휘를 선언할 수 있습니다. 이것은 단일 파일 작성자와 플레이그라운드에 열려 있는 유일한 경로입니다. 플레이그라운드는 하나의 메모리 문서만 검사하므로 import를 해석할 수 없습니다.
2. **가져오기** — `uses:` 또는 `extends:`를 통해 도달한 프로젝트 스키마에 둡니다([상태 스키마](/ko/state/schemas/) 및 [가져오기](/ko/language/imports/) 참고). 여러 문서로 된 프로젝트가 선호해야 하는 경로입니다: 루트에서 한 번 선언하고 모든 문서가 같은 멤버에 도달하게 합니다. [`docs/examples/base.schema.yaml`](https://github.com/journeyWorker/lute/blob/main/docs/examples/base.schema.yaml)이 그 예입니다.
3. **플러그인** — capability 플러그인의 `enums` export에 둡니다([매니페스트](/ko/plugins/manifests/) 참고). 엔진이나 장르 팩이 활성화하는 모든 프로젝트에 어휘를 제공할 때 쓰는 경로입니다. [`showcase.pack`](https://github.com/journeyWorker/lute/blob/main/docs/examples/showcase/plugins/showcase.pack/enums/vocabulary.yaml)이 그 예입니다. 앞의 두 경로와 달리 이 경로는 capability 스냅숏에 나타납니다 — `lute context --json`의 `enums` 아래에 들어가 `capabilitySnapshot`을 바꿉니다. 두 프로젝트 경로는 별도의 `projectEnums` 키 아래에 들어가며 이를 바꾸지 않습니다.

선언은 프로젝트 루트(`lute.project.yaml`이 있는 디렉터리)별입니다. 형제 루트의 선언은 여러분의 루트 안으로 들어오지 않습니다.

:::note
플러그인의 멤버는 실행이 프로젝트를 볼 수 있을 때만 단일 파일 명령에 도달합니다. `lute check scenes/opening.lute`는 파일 위의 가장 가까운 `lute.project.yaml`을 스스로 찾아냅니다(stderr: `note: using project …`). `lute check-project .`은 루트 자체를 해석합니다. `lute compile`, `trace`, `context`에는 `--project .`이 필요합니다. 이것 없이 실행하면 core 전용으로 동작하므로 플러그인 경로에 의존하는 문서는 `E-DOMAIN-UNKNOWN`을 보고합니다.
:::

## 두 경로가 하나의 슬롯을 선언할 때의 우선순위

두 충돌은 서로 다르게 동작하며, 이 차이는 두 번 읽어 볼 가치가 있습니다.

**프로젝트 경로와 플러그인의 충돌은 `E-DOMAIN-DUP`이며, 플러그인이 승리합니다.** 두 프로젝트 경로 모두에 적용됩니다 — 인라인 `enums:` 블록과 가져온 스키마의 선언은 똑같이 취급됩니다. 프로젝트 항목은 삭제되고 보고되며, 이후 값은 플러그인의 멤버에 대해 판정됩니다:

<!-- lute-diagnostics -->
```
error [E-DOMAIN-DUP] domain `emotion` is declared by this project — in a document's own `enums:`
frontmatter or in a project schema reached through `uses:`/`extends:` — but already exists in the
plugin/core vocabulary; a domain name must be declared by exactly one source, so drop the project
declaration or the plugin's `enums` export (the plugin's wins)
```

<!-- lute-diagnostics unverified="verbatim; crates/lute-check/src/directives.rs appends a did-you-mean `{hint}` right after `)`, empty here since no member is near `furious`, and an empty placeholder matches no literal" -->
```
error [E-BAD-ENUM] `furious` is not a valid value for `emotion` of `::narrator`
(expected one of: neutral, delighted, pensive)
```

슬롯마다 경로 하나를 고르세요. 코어는 멤버를 제공하지 않으므로, 이 오류는 활성화한 플러그인과 충돌할 때만 발생합니다.

**가져온 항목에 대한 인라인 선언은 충돌이 아닙니다 — 인라인 선언이 이기며, 상위 집합이어야 합니다.** 스키마가 이미 선언한 슬롯을 문서가 다시 선언해 멤버를 *추가*할 수 있고, 그러면 인라인 목록이 적용됩니다. 기반 선언의 멤버를 빼면 `E-EXTENDS-RELATION-SIG`입니다:

<!-- lute-diagnostics -->
```
error [E-EXTENDS-RELATION-SIG] enum `emotion` is missing base member(s) ["delighted"]; an inline
re-declaration must re-declare a superset of the imported base's members
```

이는 의도적으로 `E-DOMAIN-DUP`가 아닙니다. 그 코드는 플러그인이 관련된 충돌을 위해 예약되어 있습니다.

## 멤버 의미론: `exits:`와 `default:`

두 슬롯에는 컴파일러가 분기하는 멤버가 있으며, 컴파일러는 이를 추측하지 않습니다.

- **`action` 선언에는 `exits:`가 반드시 있어야 합니다.** 스테이지에 캐릭터의 존재를 끝내는 멤버입니다. 여기에 적힌 멤버는 `exit: true`를 담은 `actor` 레코드로 낮춰지고, 적히지 않은 멤버는 그렇지 않습니다.
- **`anchor` 선언에는 `default:`가 반드시 있어야 합니다.** 명시적 앵커 없이 캐릭터를 보일 때 사용하는 멤버입니다. `provenance.by: "auto-anchor-on-show"`를 가진 `actor` 레코드로 주입됩니다.

둘 다 예전에는 하드코딩되어 있었습니다. 종료는 두 곳에서 손으로 동기화한 복사본에서 이름(`fadeOut*` / `exit*` / `hide`)으로 감지했고, 기본 앵커는 체커의 `DEFAULT_ANCHOR = "center"` 상수였습니다. 이제 생략은 대체 동작이 아니라 오류입니다. 이름 접두사에 조용히 결합하는 것이 바로 `0.9.0`이 제거하는 숨은 결합이기 때문입니다. 언어는 이 두 슬롯에 멤버 의미론이 필요하다는 사실은 소유하지만, 어떤 멤버가 이를 충족하는지는 알지 못합니다.

다른 모든 슬롯에서 `exits:`와 `default:`는 의미가 없으며 거부됩니다. 따라서 오타가 무시된 키 안에 숨을 수 없습니다. `default:`와 모든 `exits:` 항목 자체도 선언된 멤버여야 합니다.

| 코드 | 발생 조건 |
|---|---|
| `E-ENUM-MISSING-SEMANTICS` | `exits:` 없이 `action`을 선언하거나, `default:` 없이 `anchor`를 선언할 때 |
| `E-ENUM-UNEXPECTED-SEMANTICS` | 이런 의미론이 없는 슬롯에 `exits:` 또는 `default:`를 둘 때 |
| `E-ENUM-DEFAULT-NOT-MEMBER` | `default:`가 `members:` 중 하나가 아닐 때 |
| `E-ENUM-EXITS-NOT-MEMBER` | `exits:` 항목이 `members:` 중 하나가 아닐 때 |

두 키 모두 아티팩트에 직렬화되지 않습니다. 컴파일러가 이미 멤버 의미론을 `actor.exit`와 방출되는 앵커로 해석했으므로, 엔진에는 런타임 멤버 의미론이 필요 없습니다.
## 도구

`lute init`은 `emotion`, `anchor`, `action`, `mood`, `volume`, `musicPlayback`, `vfxType`을 포함한 시작용 어휘를 생성된 씬이 가져오는 `vocabulary.schema.yaml`에 뼈대로 만듭니다 — 규칙이 아니라 여러분이 소유하고 편집하는, 의견이 담긴 템플릿입니다. 프로젝트가 카메라, CG, 의상, 시퀀스, 텍스트 스타일 중 하나를 처음 스테이징할 때 해당 슬롯을 추가하세요.

`lute doctor <dir>`는 루트가 선언한 슬롯을 멤버 의미론과 함께 인라인으로 보고합니다:

```
$ lute init demo && lute doctor demo
lute doctor — demo
  • toolchain version: 0.10.0
  • language version: 0.10.0
  • IR schema version: 0.10.0
  ✓ lute.project.yaml: found at demo/lute.project.yaml
  ✓ content documents: 1 `.lute` file(s) under demo
  • provider snapshots: no providers/ directory (core-only project)
  • vocabulary slots declared: emotion, action (exits: fadeOut/hide), anchor (default: center), mood, volume, musicPlayback, vfxType
  • VS Code extension: not detectable from the CLI
```

아무것도 선언하지 않는 루트는 `vocabulary slots declared: none`을 보고합니다.

`uses:`/`extends:`를 통해 인라인으로 선언하거나 도달하는 프로젝트에서는 컴파일된 아티팩트의 `enums` 배열이 채워집니다 — 아티팩트가 자신이 컴파일된 어휘를 자체적으로 설명하게 됩니다. 플러그인이 제공하는 어휘는 그 배열에 나타나지 않습니다. 그것은 capability 표면이며, 대신 `capabilitySnapshot`과 `lute context --json`의 `enums`에 나타납니다. 어느 쪽이든 아티팩트-*콘텐츠* 변경일 뿐입니다: 어휘 작업은 IR 필드를 추가하거나 이름을 바꾸거나 옮기지 않습니다. (`irVersion`은 `"0.10.0"`으로 읽히며, 그 번호가 담는 형태 변화는 어휘와 무관한 단일 provenance 필드 이름 변경입니다.)

## 알려진 제한: 컴포넌트 본문은 가져오는 문서에 대해 해석됩니다

분명히 적어 둡니다. 누군가 이 문제를 겪게 될 것이기 때문입니다. **컴포넌트 자체의 `uses:`와 자체 인라인 `enums:`는 파싱 때 모두 버려집니다.** 해석된 컴포넌트에는 `params`, `body`, `src`만 남으므로, 본문이 가져오는 문서에 접혀 들어간 뒤 검사 대상 어휘는 **가져오는 문서의** 어휘입니다.

따라서 컴포넌트가 자기만 선언한 슬롯의 이름을 쓰면 단독으로는 통과하지만 `::use`를 통해서는 실패합니다:

<!-- lute-diagnostics unverified="deliberately abridged — both quotes elide the component's path (`…/c.component.lute`) and the tail of the shared E-DOMAIN-UNKNOWN sentence (`— …`) because the block exists to show the `component ... (...):` prefix, not the message it prefixes; the unabridged sentence is pinned at vocabulary.md:14" -->
```
$ lute check c.component.lute      # its own uses: declares emotion, its own inline enums: declares vfxType
ok: c.component.lute (0 warning(s))

$ lute check s.lute                # components: [c.component.lute], scene declares no vocabulary
s.lute:1:1: error [E-DOMAIN-UNKNOWN] component `reaction` (…/c.component.lute): `emotion` is not a declared domain — …
s.lute:1:1: error [E-DOMAIN-UNKNOWN] component `reaction` (…/c.component.lute): `vfxType` is not a declared domain — …
failed: s.lute (2 error(s), 0 warning(s))
```

진단은 `::use` 위치, 즉 가져오는 문서의 프런트매터에 걸리며, 컴포넌트와 그 파일 이름을 지정하는 접두사가 붙습니다.

**어휘 import는 소비자가 소유합니다.** 실질적인 규칙은 이렇습니다: 선언을 프로젝트 루트에 두고 씬과 컴포넌트가 모두 같은 선언에 도달하게 하세요. `docs/examples`가 이 방식을 씁니다. *컴포넌트 스키마* 표면 — 컴포넌트가 자체 import를 선언하고 `::use`가 그것을 확장으로 가져오는 방식 — 은 별도로 제출된 이름 붙은 향후 방향이며, 이번 릴리스에는 포함되지 않습니다.

이는 검사 차이가 아니라 *범위 지정*의 한계입니다. 모든 콘텐츠 줄 규칙은 이제 두 경로 모두에서 실행됩니다. `0.9.0`은 예전에는 루트에서만 실행되던 `check()`의 다섯 단계를 닫았으므로, 이제 컴포넌트 본문이 `::use`를 통해 조용히 검사를 통과하지 않고 씬 수준에서 같은 줄을 보고합니다. [컴포넌트와 extends](/ko/language/components-and-extends/)를 참고하세요.
