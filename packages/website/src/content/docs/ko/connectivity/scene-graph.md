---
title: 씬 그래프와 `after:`
description: 각 문서의 after 선언에서 Lute가 프로젝트 전체 전제조건 그래프를 조립하는 방법, 제한된 visited/completed/active 수식 문법, 원자 종류별 엣지와 번들 비트 노드를 보여 주는 lute 시나리오 그래프 뷰를 설명합니다.
---

씬과 퀘스트는 이 노드를 사용할 수 있기 전에 무엇이 일어나야 하는지를 **전제조건**으로 선언하며, `check-project`는 이를 프로젝트 전체 그래프로 조립합니다. 이로써 언어의 한 가지 비대칭이 해소됩니다. 퀘스트에는 이미 선언적 활성화 술어(`<quest start>`)가 있었지만, 씬에는 전제조건을 적을 표면이 전혀 없었습니다. `after:`가 퀘스트에 이미 있던 기능을 에피소드에도 제공합니다.

## `after` 선언하기

씬은 프런트매터 키 `after:`로 전제조건을 선언하고, 퀘스트는 `<quest>` 요소에서 `start` / `fail`과 나란히 놓이는 `after` 속성으로 선언합니다. 씬 문서는 정확히 하나의 노드인 반면 퀘스트 문서에는 하나 이상의 `<quest>` 선언이 들어가므로 배치가 다릅니다. 하지만 둘 다 **같은 수식 문법**을 공유합니다.

```yaml
# scene frontmatter
after: 'visited("elena.ep02") && (completed("sideQuestA") || completed("sideQuestB"))'
```

```lute
<quest id="riverDebt" follows="visited('elena.ep02')" start="…" fail="…">
```

값은 세 개의 불투명한 술어에 대한 논리합과 논리곱만 정확히 허용하는 최대한 제한된 프로필의 CEL입니다 — **부정, 산술, 상태 읽기는 허용하지 않습니다**:

```
Formula ::= "visited(" StringLit ")" | "completed(" StringLit ")" | "active(" StringLit ")"
          | "(" Formula ")" | Formula "&&" Formula | Formula "||" Formula
```

`visited(K)`의 문자열은 프로젝트가 사용하는 정식 `{character}.{episodeId}` 에피소드 키입니다. 이는 컴파일러가 `lineId`에 대해 계산하는 것과 같은 결합입니다. `completed(Q)`와 `active(Q)`는 모두 `<quest id>`를 가리킵니다. `completed`와 `active`는 이 한 슬롯을 기준으로 하며(일반 CEL 가드에서는 대신 `quest.<id>.state`를 읽습니다), `visited(...)`는 0.21.0부터 같은 visited 집합을 대상으로 하는 일반 조건 함수이기도 합니다([퀘스트와 씬](/ko/language/quests-and-scenes/#퀘스트가-씬-및-계기를-만나는-방식) 참고). 위 문법을 벗어나는 것은 무엇이든 `E-CONN-PROFILE`입니다.

`active(Q)`(0.8.0)는 세 번째 전제조건 원자입니다. 퀘스트 생명주기는 `unset → active → complete | failed`이므로 `visited`/`completed`만 담은 프로필로는 관찰 가능한 세 상태 중 두 상태만 표현할 수 있었습니다. “조사가 진행 중인 *동안* 이 씬이 잠금 해제된다”를 표현할 방법이 없었던 것입니다. 이는 생명주기에 대해 **엄밀히 더 약한** 주장입니다 — `completed(Q)`는 “`Q`가 끝났고 완료 쓰기가 실행되었다”를 허용하지만, `active(Q)`는 “`Q`가 `active`에 도달했다”만 허용합니다. 따라서 [도달 가능성](/ko/connectivity/reachability/)은 둘을 똑같이 다루지만 [봉투](/ko/connectivity/envelopes/)는 다르게 다룹니다.

## 노드 조립과 엣지

`check-project` 시점에 도구는 모든 문서의 프런트매터를 순회하고, 각 에피소드의 정식 키를 프로젝트 전체 **집합**으로 계산한 다음, 각 `visited(K)` / `completed(K)` / `active(K)`를 정확한 문자열 동일성으로 해석합니다 — 키를 분해해서 해석하지 **않습니다**. 각 수식에서 **위상 우선순위 DAG**를 도출합니다. `&&`/`||`에서의 위치나 해당 노드를 이름 붙인 원자 종류와 관계없이, `n`의 수식 어디에서든 참조된 모든 노드 `p`에 대해 `p → n` 엣지를 만듭니다. 이 과대근사 엣지 집합은 순환 검출에 사용되며 도달 가능성과 봉투 패스의 순회 순서가 됩니다.

체커의 주장은 전적으로 **그래프 구조에 관한 것**입니다 — 엣지 집합이 비순환인지, 참조된 id가 존재하는지, 만족 가능한 경로가 있는지, 그래프가 무엇을 보장하는지 판단합니다. 플레이 세션에서 수식이 참인지 여부는 엔진이 `<quest start>`를 평가하는 것과 똑같이 평가하는 런타임 문제입니다. Lute는 이를 실행하지 않습니다.

### 경로 그래프가 아닌 가용성 격자

엣지는 전제조건이 충족되면 노드가 *사용 가능해진다*는 뜻입니다. 플레이어가 그 경로로 갔다는 뜻이 아니며, 언어 어디에도 어느 경로를 택했는지는 기록되지 않습니다. 대안으로 작성된 두 씬은 같은 전제조건에서 나오는 두 엣지일 뿐입니다 — 둘 다 같은 위상 계층에 도착하고, 둘 다 `Reachable`입니다.

```console
$ lute scenario .
project root: .
  topological layers:
    layer 0: scene(x.s01ep01)
    layer 1: scene(x.s01ep02), scene(x.s01ep03)
  edges (prerequisite -> dependent) [atom kind(s)]:
    scene(x.s01ep01) -> scene(x.s01ep02) [visited]
    scene(x.s01ep01) -> scene(x.s01ep03) [visited]
```

가용성도 설계상 단조롭습니다. 위 문법에는 부정이 없으므로 어떤 수식도 “~하지 않는 한”이라고 말할 수 없으며, 작성자가 잠금 해제된 노드를 다시 잠그는 방법도 없습니다. 두 대안은 모두 잠금 해제되고, 둘 다 계속 잠금 해제 상태로 남습니다. 이야기에서 **선택**이 중요해야 한다면 그 선택은 `<branch>`가 쓰고 가드가 읽는 상태에 속합니다 — `after:`에 속하지 않습니다.

## 그래프 보기

`lute scenario <dir>`는 확인된 프로젝트 루트마다 조립된 노드/엣지 그래프를 결정적인 위상 파동으로 출력합니다 — 프런트매터와 동기화해야 하는 중앙 집중식 엣지 매니페스트 파일 없이 전체 그래프를 보는 방법입니다.

```console
$ lute scenario .
project root: .
  topological layers:
    layer 0: scene(narrator.s01ep01)
    layer 1: quest(findkai)
    layer 2: scene(narrator.s01ep02)
    layer 3: scene(narrator.s01ep03)
  edges (prerequisite -> dependent) [atom kind(s)]:
    scene(narrator.s01ep01) -> quest(findkai) [visited]
    scene(narrator.s01ep02) -> scene(narrator.s01ep03) [visited]
    quest(findkai) -> scene(narrator.s01ep02) [active]
    quest(findkai) -> scene(narrator.s01ep03) [completed]
```

퀘스트는 `follows`를 선언하면(`follows=""`도 가능하며 그래프 메타데이터일 뿐, 퀘스트를 잠그지는 않음) 그래프에 들어옵니다. 이를 선언하지 않으면 앵커를 통해 들어옵니다. 앵커는 퀘스트를 받는 `::accept`(`[accept]`, dsl 0.24.0 §2), 부모 퀘스트(`[subquest]`, dsl 0.25.0 §4), 그리고 `start`의 `visited(…)` / `entry.X.everRead` / `quest.Y.state == …` 결합식(`[start]`, dsl 0.25.0 §4)입니다. (entry는 노드 `entry(X)`가 됩니다.) [퀘스트와 씬](/ko/language/quests-and-scenes/#quest)을 참고하세요. 이들 중 어느 것도 없는 퀘스트는 앵커되지 않은 것으로 나열됩니다. 그래도 `lute scenario <dir> envelope quest:<id>`로 지정할 수 있지만 엣지는 만들지 않습니다.

### 엣지 종류

`completed(Q)`와 `active(Q)`는 모두 같은 `quest(Q)` 노드에 도착하고 수식이 평탄화되면 서로 구별되지 않습니다. 따라서 그래프는 각 엣지와 함께 해당 엣지를 정당화하는 원자 종류를 기록합니다. 세 가지 출력 형식 모두 이를 보고합니다.

- **`text`** — 위와 같이 각 엣지 뒤에 대괄호 목록을 붙입니다. 같은 퀘스트를 수식에서 두 번 이름 붙이면 한 엣지에 두 종류가 모두 붙을 수 있습니다: `after: 'active("findkai") || completed("findkai")'`는 `[completed, active]`를 출력합니다.
- **`json`** — 각 엣지는 `{"from":…,"to":…,"kinds":[…]}`입니다. 같은 이유로 `kinds`는 배열입니다.
- **`dot`** — `active` 원자만으로 정당화된 엣지는 `[style=dashed]`로 그립니다. `visited` 또는 `completed` 원자를 포함하는 엣지는 강한 주장이므로 독자가 볼 수 있도록 실선으로 유지합니다.

종류는 표현과 분석을 위한 것이며 구조적인 것이 아닙니다. DAG 모양은 어느 쪽이든 동일합니다. `active` 엣지도 `completed` 엣지와 정확히 똑같이 순서를 제한하기 때문입니다. 세 가지 원자 종류 외에도 `after` 수식이 작성하지 않은 앵커일 수 있습니다: 퀘스트를 `::accept`s하는 단위에서 오는 `[accept]`, 부모 퀘스트에서 자식 퀘스트로 오는 `[subquest]`, `start`가 읽는 소스에서 오는 `[start]`입니다(dsl 0.24.0, 0.25.0).

### 번들 비트

로어 문서의 [번들 비트](/ko/tooling/play/#번들-비트)(dsl 0.23.0)도 노드이며, 각각 `beat(<document id>.<beat id>)`로 그립니다. `after=`가 없는 `<beat>`는 레이어 0의 엔트리 노드이고 자체 엣지는 없습니다. 언제 재생되는지는 계기, 대상, `when`이 결정합니다. 프로젝트에 포터의 대사를 추가해 보겠습니다. `id: harbor`인 로어 문서에 다음을 작성합니다.

```lute
<beat id="porter" on="talk" target="npc.porter" when="visited('narrator.s01ep02')">
  @porter: Back again? The boats won't wait.
</beat>
```

```console
$ lute scenario .
project root: .
  topological layers:
    layer 0: scene(narrator.s01ep01), beat(harbor.porter)
    layer 1: quest(findkai)
    layer 2: scene(narrator.s01ep02)
    layer 3: scene(narrator.s01ep03)
  edges (prerequisite -> dependent) [atom kind(s)]:
    scene(narrator.s01ep01) -> quest(findkai) [visited]
    scene(narrator.s01ep02) -> scene(narrator.s01ep03) [visited]
    quest(findkai) -> scene(narrator.s01ep02) [active]
    quest(findkai) -> scene(narrator.s01ep03) [completed]
  unanchored (no `after` / `follows` — available from the start of play; no prerequisites in this graph):
    beat(harbor.porter) — its `when` reads visited('narrator.s01ep02'), which gates it but draws no edge; write `after="visited('narrator.s01ep02')"` to anchor it
```

`when` 안의 `visited()`는 엣지를 만들지 않습니다. `when`은 전제조건이 아니라 런타임 조건이기 때문입니다. 따라서 dsl 0.25.0부터 기본 보기에는 이 비트가 앵커되지 않은 것으로 표시되며, 이 비트를 순서에 넣을 `after=`도 함께 알려 줍니다(`--format json`에서는 `unanchoredHints`). `--format json`에서 노드의 `kind`는 `beat`입니다(`prereq`는 `null`). `--format dot`에서는 `shape=note`로 그립니다. `lute scenario <dir> reach`와 `envelope`는 정식 id를 그대로 또는 `beat:<id>`로 받으며, `reach`는 비트가 선언된 위치와 비트를 선택하는 요소를 보여 줍니다.

```console
$ lute scenario . reach harbor.porter
project root: .
reach beat(harbor.porter):
  verdict: Reachable — a satisfiable route exists under your declared routes.
  after: (none declared) — an entry node: it plays when its occasion is raised and its `when` holds.
  declared in: ./lore/harbor.lute
  on: talk
  target: npc.porter
  when: visited('narrator.s01ep02')
```

dsl 0.25.0 §3부터 번들 비트 자체도 씬의 `after:`와 같은 문법과 의미로 `after="…"`를 선언할 수 있습니다. `<beat id="porter" on="talk" target="npc.porter" after="visited('narrator.s01ep02')">`로 작성하면 포터는 레이어 0에서 빠지고, 그래프에는 `scene(narrator.s01ep02) -> beat(harbor.porter) [visited]`가 추가됩니다. 이제 이 비트는 전제조건이 충족된 뒤에만 자격을 얻습니다. 이후 `reach harbor.porter`는 `after:` 줄에 `(none declared)` 대신 수식을 출력합니다(`after: visited("narrator.s01ep02")`). 번들 비트는 `after:`의 적법한 선행 노드이기도 합니다(dsl 0.24.0 §2). 씬이나 퀘스트의 `visited('harbor.porter')`가 이 비트를 가리키며, 연결성은 이를 씬과 같은 방식으로 통과합니다.
