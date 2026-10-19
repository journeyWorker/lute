---
title: 가져오기(uses:)
description: 씬이 uses:로 공유 상태 스키마를 가져오는 방법, 일반 YAML .schema.yaml 선언 파일, uses / extends / components가 공유하는 DAG 규율입니다.
---

`run`/`user`/`app` 상태 스키마는 **게임/시즌 전체에서 공유**됩니다. 하나의 영속 값이 씬마다 다른 타입을 가질 수 없으므로, 모든 곳에 다시 선언하는 대신 각 씬이 가져오는 단일 진실 공급원 스키마 문서에 둡니다. 이 가져오기는 프런트매터 키 **`uses:`**입니다.

```lute check="docs/examples/carry-ep.lute"
---
kind: scene
character: elena
season: 1
episode: 3
uses: state.schema.yaml
---

## Carrying the Choice Forward

@narrator: Previously, a choice was made.

<match subject="run.choseHelp">
  <when is="true">
    @elena: Thanks for helping me back then.
  </when>
  <otherwise>
    @elena: ...
  </otherwise>
</match>
```

*([`docs/examples/carry-ep.lute`](https://github.com/journeyWorker/lute/blob/main/docs/examples/carry-ep.lute)에서 가져왔습니다.)* 이 씬은 인라인이 아니라 가져온 `state.schema.yaml`에 선언된 `run.choseHelp`를 읽습니다.

## 선언 파일

`.schema.yaml` 파일은 **일반 YAML 선언 맵**입니다 — `---` 엔벨로프도, 본문도 없고 `state:` / `defs:`만 있습니다(관계형 레이어에는 `entities:` / `relations:` / `facts:` / `rules:`도 있습니다). 이 파일은 import 모드에서 가져와 검사되며 씬으로 실행되지 않으므로 `character`/`season`/`episode`를 포함하지 않습니다.

```yaml
state:
  run.choseHelp: { type: bool, default: false }
  user.level:    { type: int, default: 1 }
defs:
  helped: "run.choseHelp"
  atLeast: { type: bool, params: { n: int }, cel: "user.level >= n" }
```

def는 CEL 본문을 문자열로 쓴 것입니다 — `helped: "run.choseHelp"` — 이며 타입은 본문에서 추론됩니다. dsl 0.26.0 §2.7부터 `visited('…')` 또는 `validAt(…)` 호출인 본문은 `bool`을 추론하므로 `pierSeen: "visited('south.pier')"`에는 긴 형식이 필요 없습니다. `params:`가 있거나 검사기가 타입을 추론할 수 없는 def는 `{ type: bool, cel: "…" }` 긴 형식을 사용합니다. 다른 형태는 `E-DEF-DECL`입니다. 가져온 스키마의 def에서 프로젝트 전체 `lute check`는 가져오는 문서마다가 아니라 스키마 줄에서 한 번 `E-DEF-DECL`을 보고하며 메시지 끝에 `(imported by N documents)`를 붙입니다.

가져오기 경로는 **가져오는 씬 파일을 기준으로** 해석되므로, 씬과 스키마는 함께 이동해야 합니다(스키마 없이 씬을 `/tmp`로 복사하면 `E-USES-NOT-FOUND`가 발생합니다).

진정으로 씬 로컬인 `scene.*` 선언만 씬의 `state:` 블록에 인라인으로 둘 수 있습니다. 씬은 가져온 `run`/`user`/`app` 경로를 다시 선언하거나 재정의해서는 **안 됩니다**.

## 가져오기 DAG

`uses:`, [`extends:`](/ko/language/components-and-extends/), [`components:`](/ko/language/components-and-extends/)는 모두 하나의 가져오기 규율을 공유하며 **방향성 비순환 그래프**를 이룹니다.

- **순환은 정적 오류**이며 진단에 문제가 되는 경로가 출력됩니다(`E-USES-CYCLE`).
- 가져온 스키마는 어떤 씬보다 먼저 로드되고 검사됩니다.
- 가져오기 간 동일한 `defs` 이름은 오류입니다 — 자동으로 가려지지 않습니다.
- 가져오기 참조는 정규화되므로 같은 파일로 가는 두 경로는 하나의 스키마 정체성이 되고, 두 경로로 도달한 파일(다이아몬드)은 중복 선언되지 않고 **중복 제거**됩니다.

`uses:`는 **동료 스키마를 합집합으로 결합**합니다. 두 동료 스키마가 같은 이름을 선언하면 충돌입니다. 합집합이 아니라 정제하려면 — 기반 선언을 자신의 선언으로 덮어쓰려면 — `extends:`를 사용하세요. 더 낮은 우선순위의 기반 레이어와 재정의 규칙은 [컴포넌트와 extends](/ko/language/components-and-extends/)에서 설명합니다.

```lute check="docs/examples/extends-demo.lute"
---
kind: scene
character: elena
season: 1
episode: 4
uses: child.schema.yaml
---
```

*([`docs/examples/extends-demo.lute`](https://github.com/journeyWorker/lute/blob/main/docs/examples/extends-demo.lute)에서 가져왔습니다. 이 파일의 `child.schema.yaml` 자체가 기반을 `extends:`하며, DAG는 여러 파일 깊이로 실행됩니다.)*

가져온 파일(스키마 또는 컴포넌트)에 있는 누락 파일, 순환, 파싱 오류는 공유된 `E-USES-{NOT-FOUND,CYCLE,PARSE}` 진단으로 나타납니다.

## 프로젝트 기본값

모든 문서가 같은 스키마를 가져오는 프로젝트는 `lute.project.yaml`의 `defaults:` 아래에 이를 한 번만 적을 수 있습니다. 매니페스트 아래의 모든 문서가 이 키를 상속하며, 그곳의 `uses:` 경로는 매니페스트 디렉터리를 기준으로 해석됩니다.

```yaml
defaults:
  luteVersion: "0.30.0"
  uses:
    - schema/world.schema.yaml
    - schema/items.schema.yaml
    - schema/areas/*.schema.yaml
  questTier: run
```

dsl 0.26.0 §2.4부터:

- `defaults.uses` 항목은 **글롭**(`*`, `**`)일 수 있습니다. 경로 순서로 확장되므로 여러 작성자가 만든 프로젝트의 각 영역은 자신의 `schema/areas/<area>.schema.yaml`을 유지할 수 있고, 새 영역에는 매니페스트 수정이 필요 없습니다. 빈 디렉터리 또는 아직 존재하지 않는 디렉터리( git은 빈 디렉터리를 유지하지 않음) 위에서 글롭이 아무것도 일치하지 않으면 아무것도 가져오지 않으며 오류도 아닙니다. 리터럴 경로는 반드시 존재해야 합니다.
- **`questTier: run | user`**는 아무것도 쓰지 않는 모든 `<quest>`의 `tier=`를 설정합니다([`<quest>`](/ko/language/quests-and-scenes/#quest) 참고). 이것이 없으면 기본값은 계속 `user`입니다.

글롭이 가져오는 파일은 다른 `uses:` 가져오기와 마찬가지로 동료입니다. 둘 중 두 파일이 선언한 이름은 여전히 충돌합니다. 단, entity kind는 [`add:`로 확장](/ko/state/schemas/#여러-파일에-걸쳐-조합되는-엔터티-종류-add)할 수 있습니다. 스키마를 영역별로 나누는 방법은 [여러 작성자 프로젝트](/ko/guides/multi-author/)를 참고하세요.
