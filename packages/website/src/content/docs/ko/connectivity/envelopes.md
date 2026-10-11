---
title: Guaranteed / Possible 엔벌로프
description: 노드별 사용 가능 상태 분석 — run/user 상태에 대한 Guaranteed 및 Possible 표, 그 옆의 보장된 팩트 표, active(Q)가 completed(Q)보다 엄밀히 약한 이유, Possible-Guaranteed 경고, 그리고 아무 after도 없는 퀘스트에서 기본값만 답으로 제공하는 경우를 포함한 퀘스트 지정 방식입니다.
---

**엔벌로프(envelope)**는 사전 대응적인 질문에 답합니다. *제어가 노드 X에 도달할 때 어떤 상태가 실제로 설정되어 있습니까?* 이는 읽어도 되는 항목(schema import가 관할함)과는 다릅니다. 엔벌로프는 노드마다 **`run.*` / `user.*`만** 대상으로 두 집합을 추적합니다 — 쓰기가 단조적("한 번 설정되면 계속 유지되므로 선행 노드에 대한 합집합/교집합이 타당한")인 계층입니다. 퀘스트 임시 필드(`quest.<id>.*`)는 제외됩니다. X에서 도달 가능했는지는 경로 구조의 `completed(Q)` / `active(Q)`로 직접 답합니다.

- **Guaranteed(X)** — X로 가는 *모든* 선언된 경로에 설정됩니다.
- **Possible(X)** — X로 가는 *일부* 선언된 경로에 설정됩니다.

둘 다 `defassign` 자체의 격자를 그래프로 끌어올린 것입니다. 각 노드의 보장된 쓰기 집합은 확정 할당 워크로 계산한 다음, 수식 AST에 대한 구조적 재귀로 전파합니다.

```
visited(Y):   G = Guaranteed(Y) ∪ G(Y)      P = Possible(Y) ∪ P(Y)
completed(Q): G = P = writesOnComplete(Q)
active(Q):    G = ∅                         P = writesOnComplete(Q)
X && Y:       G = G(X) ∪ G(Y)               P = P(X) ∪ P(Y)
X || Y:       G = G(X) ∩ G(Y)               P = P(X) ∪ P(Y)
entry node:   G = P = D
```

진입점의 기본 경우인 `D`는 `run.*`/`user.*` 경로 중 스키마 `default`를 가진 경로의 집합이며, import 계층에서 그대로 재사용합니다. *모든* 노드에서 `D ⊆ Guaranteed(n)`이 성립합니다. 이는 "기본값이 있는 경로는 항상 할당되어 있다"는 확정 할당 불변식을 전체 그래프로 끌어올린 것과 같습니다. 이 구조적 재귀는 경로 집합을 지수적으로 생성하지 않고 경로별 ∩/∪를 계산한 결과와 동일함이 증명되어 있습니다.

## `active(Q)`는 `completed(Q)`보다 엄밀히 약합니다

두 퀘스트 원자가 갈라지는 곳은 여기 하나입니다. `completed(Q)`는 `quest.Q.state == complete`이라고 가정할 수 있게 하므로, 퀘스트의 필수 목표와 `<on quest complete>` 핸들러가 쓰는 모든 것 — `writesOnComplete(Q)` — 이 표의 **양쪽**에 들어갑니다. `active(Q)`는 오직 "`Q`가 `active`에 도달했다"고만 보장합니다. 완료 쓰기는 아직 실행되지 않았을 수 있으므로 **Possible**에만 기여하고 Guaranteed에는 전혀 기여하지 않습니다.

`findkai` 퀘스트는 필수 목표가 `run.kaiFound`를 쓰고, `after: 'active("findkai")'`로 게이트되어 이를 읽는 장면과 `after: 'completed("findkai")'`로 게이트된 두 번째 장면이 있다고 하겠습니다. 안전한 것은 두 번째 장면뿐입니다.

```console
$ lute scenario . envelope narrator.s01ep02
project root: .
envelope for scene(narrator.s01ep02) (pre-entry — state available when control REACHES this node, before its own writes):
  Guaranteed (safe to read under your declared routes):
    - run.greeted   written by: (nothing on a declared route reaching here — schema default only)
  Possible (set on SOME but not every declared route reaching this node; the Guaranteed paths above are not repeated):
    - run.kaiFound   written by: …
  Guaranteed facts (hold on every declared route reaching this node):
    (none)
  Possible \ Guaranteed -- warning-grade reads (set on SOME but not every declared route; suppressed by default in `check-project`, surfaced here):
    - ./scenes/whileActive.lute:13:36: state path `run.kaiFound` is set under your declared routes on SOME routes reaching this node, but not every one — not yet guaranteed; order this scene `after:` a scene every route passes that sets it, or guard the read with `has(run.kaiFound)`
```

그 노드의 `after`를 `completed("findkai")`로 바꾸면 `run.kaiFound`가 Guaranteed로 이동하고 `Possible \ Guaranteed`는 비게 됩니다. (`run.greeted`에는 스키마 `default`가 있으므로 `D`에 속하며 두 경우 모두 보장됩니다.)

`active`로 게이트된 장면에서 퀘스트의 완료 쓰기를 읽는 것은 따라서 `Possible \ Guaranteed` 경고 등급에 해당합니다 — 이는 올바른 동작이며, 바로 그 원자가 숨기지 않고 드러내도록 만든 실수입니다.

## 표 읽기

`lute scenario <dir> envelope <nodeId>`는 두 표를 모두 출력합니다. 노드 X에서 상태 경로 `P`를 읽을 때의 진단은 다음과 같습니다.

- `P ∈ Guaranteed(X)` → 선언된 경로에서 안전하므로 진단이 없습니다.
- `P ∉ Possible(X)` → X 전에 `P`를 설정하는 선언된 경로가 전혀 없습니다 — **오류 등급**인 `E-STATE-MAYBE-UNAVAILABLE`이며, 기본적으로 `check-project`가 제공합니다.
- `P ∈ Possible(X) \ Guaranteed(X)` → 일부 경로에는 설정되지만 전부는 아닙니다 — **경고 등급**이며, 기본적으로 이 명령의 출력에서 억제됩니다. 이는 `Possible \ Guaranteed` 읽기이며, `check-project`가 계산하고 기본적으로 버립니다.

모든 메시지에는 "under your declared routes"라는 문구가 그대로 포함됩니다(A-hybrid 방식).

## 보장된 팩트

스칼라 표 옆에서 엔벌로프는 **팩트 엔벌로프**(dsl 0.20.0)도 보유합니다. 이는 노드에 도달하는 *모든* 선언된 경로에서 성립하는 관계형 팩트입니다. `check-project`가 관계형 가드를 판정할 때 사용하는 must-set과 동일하며([`check-project`가 관계형 가드를 분석하는 방식](/ko/state/facts-and-datalog/#check-project가-관계형-가드를-분석하는-방법) 참고), `after:`를 따라 `Guaranteed`와 같은 구조적 재귀로 전파됩니다 — `visited(A)`는 `A`가 끝날 때 성립하는 것을 기여하고, `&&`는 합집합을, `||`는 교집합을 취하며, `completed(Q)`/`active(Q)`는 아무것도 기여하지 않습니다. 단조 팩트만 장면 경계를 넘어갑니다. 즉, 철회될 수 없고, 다른 assert가 다른 값으로 같은 `key:`를 공유할 수 없으며, 해당 관계가 `reserved:`도 엔진 개방형도 아니어야 합니다. `tier: scene`/`tier: quest` 팩트는 경계를 넘지 않습니다. 각 팩트에는 이를 설정한 것이 — assert 위치, `facts:` 시드 또는 규칙 — 표시됩니다.

Haven 예시의 사무장 장면(ep09)은 다음과 같습니다.

```console
$ lute scenario docs/examples/haven envelope haven.s01ep09
…
  Guaranteed facts (hold on every declared route reaching this node):
    - awake(ottavio) (docs/examples/haven/scenes/stowaway.lute:20)
    - awake(vesna) (`facts:` seed)
    - found(ottavio) (docs/examples/haven/scenes/stowaway.lute:19)
    - knows(ottavio, manifest) (docs/examples/haven/scenes/stowaway.lute:23)
…
```

에피소드 6에서는 Ottavio의 세 팩트를 모두 분기 밖에서 assert하고, ep09로 가는 모든 경로가 이를 통과합니다. 따라서 ep09의 `holds('found', ['ottavio'])` 가드는 결코 닫힐 수 없습니다 — 해당 장면의 `listTheMass` 선택지에 가드가 없는 이유입니다. `knows(vesna, manifest)`는 다른 문서 세 곳에서 assert하더라도 존재하지 않습니다. 그중 어느 것도 ep09로 가는 모든 경로에 놓이지 않기 때문입니다(수경 재배실은 두 대안 중 하나이고, 기록 보관소는 선택 사항이며, 사무장의 장부에 있는 assert는 자체 로어 엔트리 안에서만 성립하고 엔진이 원하는 시점에 제시합니다). `check-project`가 가드에서 `W-FACT-GUARANTEED`를 보고하면 이 표에서 그 팩트가 나온 것입니다. `--format json`에서는 같은 목록이 `envelope.guaranteedFacts`이며, `{ "fact", "establishedBy" }` 객체의 배열입니다.

## 퀘스트 지정

`lute scenario <dir> envelope quest:<id>`는 **모든** 퀘스트에 대해 실제 표를 출력합니다.

- `after` 속성이 **있는** 퀘스트 → 장면과 정확히 같은 방식으로 계산한 전체 `Guaranteed`/`Possible` 표입니다.
- `after`가 **없는** 퀘스트 → 기본값만 담은 `D` 표(결코 비어 있지 않고 오류도 아님)와, `after`를 선언하면 기본값만 있는 상태를 넘어 더 풍부한 정보를 얻는다는 한 줄 메모입니다.

퀘스트는 반응형입니다 — `<quest start>`가 가능한 가장 이른 순간에 실행될 수 있으므로, 작성자가 `after`를 통해 그래프 위치를 선택하지 않는 한 퀘스트 진입 시 스키마 기본값을 넘어서는 어떤 것도 건전하게 보장할 수 없습니다. 반응형 진단 측면("이 특정 가드의 읽기가 안전한가?")은 이미 퀘스트 자체 가드에 대해 `defassign`이 처리합니다.
