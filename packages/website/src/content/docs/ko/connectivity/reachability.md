---
title: 도달 가능성
description: 선언된 after 그래프에서 노드에 만족 가능한 경로가 있는지 Lute가 증명하는 방법, Reachable / Unreachable / Unknown 판정, 노드별 순환 저하, 그리고 lute scenario reach입니다.
---

[씬 그래프](/ko/connectivity/scene-graph/)가 조립되면 프로젝트 전체 `lute check`는 각 노드에 **프로젝트의 진입 집합에서 출발하는 만족 가능한 경로**가 하나 이상 있는지 묻습니다 — 경로를 열거하지 않고, 수식 AST를 메모이제이션하는 구조적 재귀로 계산합니다:

```
reachable(visited(Y)) = ¬unreachable(Y)
reachable(completed(Q)) = ¬E-QUEST-UNREACHABLE(Q)
reachable(active(Q))    = ¬E-QUEST-UNREACHABLE(Q)
reachable(X && Y) = reachable(X) ∧ reachable(Y)
reachable(X || Y) = reachable(X) ∨ reachable(Y)
```

`active(Q)`와 `completed(Q)`는 **여기서는 동일합니다**. 둘 다 "이 노드보다 먼저 해당 노드에 도달할 수 있어야 한다"고 주장하고, 둘 다 동일한 `quest(Q)` 노드로 해석되며, 둘 다 precedence DAG에 동일한 엣지를 추가합니다 — 따라서 `E-CONN-CYCLE`, `E-CONN-UNREACHABLE`, `E-CONN-UNKNOWN-NODE`는 `active`가 존재하기 전과 정확히 동일하게 동작하고, `active` 원자는 다른 원자와 마찬가지로 `E-CONN-FORMULA-TOO-COMPLEX`에 포함됩니다. 둘의 차이는 [envelope](/ko/connectivity/envelopes/)에서만 드러납니다. envelope는 경로가 존재하는지가 아니라 경로가 *무엇을 기록했는지*를 묻기 때문입니다. 도달 가능성은 더 약한 질문을 하므로 어느 쪽이든 같은 답을 얻습니다.

`after`가 없거나 비어 있는 노드는 모두 그래프 진입점입니다(`reachable = true`는 자명함) — 별도로 "시작점을 선언하는" 관례는 필요하지 않습니다. 문법에서 부정을 제외하므로 모든 수식은 단조적이며, 각 노드의 판정은 위상 순서를 따라 한 번 계산하고 메모이제이션합니다. 따라서 선형이고 폭발하지 않습니다. 만족 가능한 경로가 없는 노드는 `E-CONN-UNREACHABLE`입니다 — 이는 *작성된* 그래프 자체의 일관성에 관한 순수한 사실이므로, 유보가 필요 없는 유일한 연결성 오류입니다.

## 판정

`lute scenario --project <dir> reach --node <nodeId>`는 세 가지 판정 중 하나와 노드에 선언된 `after` 선행 조건 구조를 보고합니다:

- **Reachable** — 선언된 `after` 그래프에서 만족 가능한 경로가 존재합니다.
- **Unreachable** — 선언된 경로가 노드에 도달하지 않습니다(`E-CONN-UNREACHABLE`).
- **Unknown** — 노드가 순환 위에 있거나 순환의 하류에 있으므로 선행 조건 순서를 해석할 수 없습니다(아래 참고).

`<nodeId>`는 씬의 정식 키(예: `marina.s01ep02`) 또는 퀘스트의 `quest:<id>`입니다.

```console
$ lute scenario --project . reach --node narrator.s01ep02
project root: .
reach scene(narrator.s01ep02):
  verdict: Reachable — a satisfiable route exists under your declared routes.
  after: active("findkai")
  referenced node(s) (see `after` above for the && / || structure — this is NOT a flat requirement list):
    - quest(findkai): Reachable — a satisfiable route exists under your declared routes.
```

참조 노드 목록은 의도적으로 체크리스트가 아닙니다. 각 원자 자체의 판정을 보고하며, `after:` 위 줄만 `&&` / `||` 구조가 존재하는 곳입니다. 수식이 논리합인 노드는 그중 하나만 필요로 합니다.

## 결말

`lute scenario --project <dir> reach --endings[=<occasion>]`(0.27.0)는 모든 결말을 한 번에 검사합니다. `=<occasion>`을 지정하면 해당 계기에 응답하는 모든 beat를 검사하고, 인자를 생략하면 콘텐츠에서 `::end`를 실행할 수 있는 모든 beat를 검사합니다. 각 행은 옆에 `after:` 판정을 두고, `when` 판정, 즉 프로젝트 전체 `lute check`가 도달하는 판정(`E-BEAT-UNREACHABLE` / `E-ENTRY-UNREACHABLE`(결코 성립하지 않음) 또는 `W-BEAT-SHADOWED`(결코 승리하지 않음))을 표시합니다. 그리고 `when`을 반박하는 것이 없으면 읽는 상태 경로와 팩트, 그리고 이를 생성하는 주체를 표시합니다:

```console
$ lute scenario --project . reach --endings=termEnd
project root: .
endings (the beats answering `termEnd`):
  end.other.keeper (beat, lore/endings/other.lute): reachable
    after: Reachable — a satisfiable route exists under your declared routes.
    when: `run.route == 'hotaru'` — whole-project `check` does not refute it; it needs:
      run.route — written by scene `common.festival` (choice into)
  …
  end.ren.lantern (beat, lore/endings/ren.lute): reachable
    after: Reachable — a satisfiable route exists under your declared routes.
    when: `run.route == 'ren' && @devoted(run.aff.ren)` — whole-project `check` does not refute it; it needs:
      run.aff.ren — written by beat `hangouts.council` via component `hangout`, scene `common.arrival`, scene `common.clash`, scene `common.injury`, scene `ren.confession`, scene `ren.date`
      run.route — written by scene `common.festival` (choice into)
  …
10 ending(s): 10 reachable, 0 unreachable, 0 unknown
reachable here means nothing static refutes it, not that a run reaches it: a play that presents the ending is the proof — `lute test --coverage` lists the beats no play presents.
```

여기서 "Reachable"은 정적으로 결말을 반박하는 것이 아무것도 없다는 뜻입니다. 기록된 쓰기가 `run.aff.ren`을 `when`에 필요한 값으로 만들 수 있는지는 분석하지 않습니다. 결말을 제시하는 플레이가 증명이며, `lute test --coverage`는 어떤 플레이도 제시하지 않는 beat를 나열합니다. 경로에서 아무것도 쓰지 않는 경우에는 `nothing writes it`이라고 표시하며, 이는 올바른 경우입니다.

## 순환에 따른 저하는 노드별입니다

`E-CONN-CYCLE`은 잘못된 순서를 표시하지만 전체 프로젝트 루트를 무효화하지는 않습니다. 도달 가능성은 그래프의 자연스러운 위상 순서로 계산됩니다. 모든 선행 조건 엣지가 해석되면 노드가 그 순서에 들어가며, 이 과정은 순환 구성원과 그 구조적 하류 노드에서만 재귀적으로 실패합니다. 따라서 순환과 위상적으로 독립된 노드는 완전하고 건전한 판정을 계속 받고, 순환 위에 있거나 그 하류에 있는 노드만 `Unknown`으로 저하됩니다.

허용된 한 가지 보수적 공백이 있습니다. 엣지 모델이 `||` 위치를 과대 근사하므로, 순환 노드를 거치는 논리합 항을 통해서만 도달 가능한 노드는 독립적인 다른 논리합 항이 도달 가능성을 증명할 수 있더라도 보수적으로 저하된 것으로 보고됩니다. 이는 건전합니다 — **거짓 `Unknown`은 언제나 안전하지만, 거짓 `Reachable`은 절대 안전하지 않습니다** — 이를 복구하려면 SCC 응축을 인식하는 분석이 필요합니다(향후 작업).

고정된 A-hybrid enforcement 방침에서는 그래프가 엔진이 준수할 수도 있는 권고 데이터이므로, 모든 도달 가능성 메시지는 무조건적인 런타임 주장으로 쓰지 않고 "사용자가 선언한 `after` 경로에 따른"이라고 표현합니다.

## 0.34.0의 증거

시나리오 reach 결과에는 JSON에서 `evidence`가 포함됩니다. 구조적 도달 가능성
증명은 `proven`이고, 순환으로 저하되었거나 결정되지 않은 결과는 `unknown`입니다. 사람용
문구는 선언된 `after` 경로 범위로 한정되며, 유한 분석이 런타임 경로가 전혀 없음을
증명한다고 주장하지 않습니다.
