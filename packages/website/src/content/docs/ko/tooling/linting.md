---
title: 린팅
description: "lute lint — lute.lint.yaml에서 설정하는 자문형 프로젝트별 editorial lint(L-* 코드), 일곱 core 규칙, 프로젝트 로컬 custom 규칙, 플러그인 lint export, 규칙이 읽는 metric table, display-name advisory W-DISPLAY-NAME-DUP(dsl 0.26.0), 종료 코드 계약입니다."
---
## 기계 판독 출력 (0.43)

`lute lint --json`은 `command`, `schemaVersion`, `ok`와 진단 payload를 담은 CLI envelope를 사용합니다. 오류는 `ok: false`와 `error` 객체로 반환되고, 진단 결과는 성공 payload에 남습니다. `--deny` 또는 `--deny-warnings`로 경고를 승격한 경우에도 종료 코드가 권위 있는 결과입니다.

`lute lint [PATH]`는 설정 가능한 editorial 및 composition 문제를 검토합니다. 가장 가까운 `lute.project.yaml`별로 문서를 모으고, 활성화된 각 규칙에 대해 같은 metric table을 계산한 뒤, 일반 human 또는 `--json` 진단 형식으로 결과를 보고합니다.

Linting은 `lute check`를 대체하지 않고 보완합니다:

- **`lute check`**는 언어와 프로젝트 의미를 검증합니다. 기존 `W-TIMELINE-*` threshold를 포함한 계약은 변하지 않습니다.
- **`lute lint`**는 자문형 콘텐츠 규칙을 평가합니다. level과 threshold는 프로젝트 정책이므로 lint 규칙은 capability snapshot 또는 `capabilitySnapshot`에 참여하지 않으며 아티팩트 identity를 절대 바꾸지 않습니다.

설정 없이 `lute lint`를 실행하면 내장 규칙 기본값을 사용합니다. 선택적 `--config PATH`는 다른 config 파일을 선택하며, 그렇지 않으면 명령은 `<project root>/lute.lint.yaml`을 사용합니다. `--json`은 기존 JSON 진단 형식을 사용합니다.

## `lute.lint.yaml`

이 파일을 `lute.project.yaml` 옆에 두세요:

```yaml
lsp: true
ignore: ["drafts/**"]

rules:
  dialogue-length:      { level: warn, options: { maxWords: 40 } }
  dialogue-ratio:       { level: error, options: { min: 0.35 } }
  emotion-distribution: { level: warn, options: { pairWith: variant } }
  asset-exists:         { level: error, options: { providers: { bg: bg-catalog } } }
  my-plugin/variant-coverage: off
custom:
  - id: too-many-choices
    target: scene
    when: "scene.choices > options.max"
    level: warn
    message: "scene has {scene.choices} choices (budget {options.max})"
    options: { max: 6 }
```

### 최상위 키

| 키 | 타입 | 기본값 | 의미 |
| --- | --- | --- | --- |
| `lsp` | boolean | `false` | 이 config 파일이 존재하고 `true`로 설정된 경우에만 LSP의 열린 문서에 lint 결과를 발행합니다. 규칙이 설정되어 있어도 그 밖에는 LSP에서 lint가 없습니다. LSP는 `line`, `section`, `scene`, `speaker`, `group` 대상을 평가하며, `project` 규칙은 v1에서 CLI 전용입니다. |
| `ignore` | 프로젝트 루트 기준 glob 목록 | `[]` | lint에서 제외할 문서입니다. |
| `rules` | map | `{}` | core 및 활성 플러그인 규칙에 대한 규칙별 level과 option override입니다. |
| `custom` | 규칙 목록 | `[]` | 프로젝트 로컬 선언적 CEL 규칙입니다. |

`rules` map 항목은 level 축약형(예: `my-plugin/variant-coverage: off`) 또는 `{ level, options }`일 수 있습니다. level은 `off`, `hint`, `info`, `warn`, `error`이며 진단 심각도 Hint, Info, Warning, Error에 대응합니다. `off`는 규칙을 비활성화합니다. level을 생략하면 해당 규칙의 기본값을 유지합니다. option은 규칙이 선언한 기본값에 deep-merge되고, scalar 값은 override합니다.

Core 규칙 id는 bare kebab-case입니다. 플러그인 규칙 id는 `<plugin-id>/<rule-id>`입니다. custom 규칙 id는 bare여야 하며 core 규칙 id와 충돌해서는 **안 됩니다**. 잘못된 YAML은 종료 코드 2 오류입니다. 알 수 없는 규칙 또는 option 키, 잘못된 option 타입, 충돌하는 custom id는 `E-LINT-CONFIG` 결과입니다. 잘못된 항목은 건너뛰고 나머지 규칙을 실행하며 명령은 1로 종료합니다.

### Custom 규칙 형태

모든 custom 항목은 plugin 규칙과 같은 형태입니다:

| 키 | 필수 | 의미 |
| --- | --- | --- |
| `id` | 예 | 프로젝트 로컬 bare 규칙 id입니다. |
| `target` | 예 | `line`, `section`, `scene`, `speaker`, `group`, `project` 중 하나입니다. |
| `when` | 예 | 대상 행과 `options`에 대한 CEL assertion이며 `true`일 때 실행됩니다. |
| `level` | 아니요 | `off`, `hint`, `info`, `warn`, `error`입니다. |
| `message` | 예 | 결과 텍스트입니다. `{path.to.field}`는 metric 또는 option 경로를 보간합니다. 숫자는 끝의 `.0`을 제거하고 최대 두 자리로 반올림하여 출력하므로 ratio `3.3333…`은 `3.33`으로 표시됩니다. `{expr:%}`는 share를 반올림한 백분율로 출력합니다. |
| `options` | 아니요 | `when`과 message에 노출되는 기본값입니다. |

Lint CEL fragment는 ground-only입니다. `@ref`나 `$` DSL token, 상태 경로, macro(`filter`, `map`, `exists`)를 쓸 수 없습니다. 공유 ground-operation semantics, metric-path 선택 및 map indexing, 문자열·목록·map에 대한 `size()`를 지원합니다. 해석할 수 없는 field, 타입 불일치 또는 boolean이 아닌 `when` 결과는 `E-LINT-EXPR`이며 규칙 선언 위치에서 한 번 보고되고 해당 규칙은 건너뜁니다. 절대로 조용한 통과가 아닙니다.

## Metric table

규칙은 실행마다 한 번 계산되는 팩트를 검사합니다. 단어 수는 줄 텍스트를 whitespace로 나누며(각 interpolation은 token 하나로 셈), 문자 수는 Unicode scalar 수입니다.

| 대상 | 행 | 필드 |
| --- | --- | --- |
| `line` | 각 콘텐츠 `Line` | `words`, `chars`, `speaker`(내레이션은 `""`), `attrs`(문자열 map; `BoolTrue`는 `"true"`) |
| `section` | 각 `##` 섹션 | `index`(1부터), `title`, `dialogueLines`, `words`, `firstStagingTag`(또는 `""`), `kind` |
| `scene` | 각 문서 | `kind`, `dialogueLines`, `words`, `bodyNodes`(중첩 포함), `directives`, `sets`, `choices`, `sections`, `maxLineWords`, `avgLineWords`, `dialogueRatio` |
| `speaker` | 대사가 있는 각 문서/화자 | `lines`, `words`, `axis`, `attrShare` |
| `group` | 설정된 `groupBy`에 대한 각 문서/attribute/value | `attr`, `key`, `count`, `speakers` |
| `project` | 프로젝트 루트 | `scenes`, `sceneWords`, `spreadRatio` |

`scene.kind`는 문서를 분류합니다(dsl 0.22.0). `section.kind`는 그 문서를 둘러싼 값도 반복하므로 section 규칙이 다른 binding 없이 자신을 제한할 수 있습니다: 선형 씬은 `scene`, `on:`이 있는 씬은 [비트](/ko/tooling/play/)에 응답하는 `beat`, `component:`를 선언한 문서는 `component`이며, 그 밖에는 작성된 `kind:` — `quest`, `lore`, 또는 어떤 다른 값 — 입니다. 선형 episode용 규칙은 `scene.kind == "scene"`으로 가드해야 하며, 아래의 세 core 규칙은 이미 그렇게 합니다. manifest의 `chapters:`가 나열하는 씬은 `on:`이 해당 사슬에서 주는 beat이기도 합니다. 여기에는 `select: sequence` 계기의 동일한 발생에서 바로 앞 씬이 계속되는 것도 포함되며, 그 씬이 설정한 stage에서 진행됩니다.

`scene.directives`와 `section.firstStagingTag`는 staging 디렉티브만 셉니다. `::accept`, `::use`, `::end`, `::label`, `::jump`는 아무것도 제시하지 않으므로 건너뜁니다. 따라서 `::accept{quest="…"}`로 시작하고 `::bg`가 뒤따르는 섹션은 `firstStagingTag == "bg"`입니다.

`scene.dialogueRatio`는 `dialogueLines / bodyNodes`이며 body node가 없으면 `0.0`입니다. `project.scenes`는 선형 씬만 셉니다(`scene.kind == "scene"`). `project.sceneWords`는 단어 수에 대한 `{ min, max, mean, stddev }`이고, `spreadRatio`는 `max / min`이며 `min == 0`이거나 씬이 두 개 미만이면 `0.0`입니다. beat, component, quest, lore 문서는 spread에 들어가지 않습니다.

`speaker.axis`는 미리 설정되지 않고 화자의 줄 attribute에서 관찰됩니다. 관찰된 모든 domain slot과 관찰된 모든 slot-plus-stamp-attribute 조합(예: `emotion+variant`)에 항목이 있으며, 빠진 값은 `""` bucket이 됩니다. axis 항목은 `run`, `runValue`, `streaks`, `streakAvg`, `distinct`, `top { value, count, share }`를 노출합니다. `run`은 같은 값이 이어지는 가장 긴 streak이고 `streakAvg`는 줄 수를 streak 수로 나눈 값이며 `top.share`는 최상위 bucket의 수를 줄 수로 나눈 값입니다. `speaker.attrShare`는 각 attribute 이름을 해당 화자의 줄에서 그 attribute가 있는 비율에 매핑합니다.

## Core 규칙

일곱 core 규칙은 모두 기본적으로 활성화됩니다. `asset-exists`는 provider mapping이 없으면 동작하지 않고, `variant-composition`은 설정한 attribute를 가진 줄이 생길 때까지 동작하지 않으므로, 아무것도 없는 프로젝트에서 두 규칙이 잘못된 잡음을 내지 않습니다. 어떤 규칙이든 `level: error`로 설정하면 해당 프로젝트에서 release를 차단합니다.

| 규칙 | 대상 | 기본값 | trigger 및 기본값 |
| --- | --- | --- | --- |
| `dialogue-length` | line | warn | `line.words > maxWords`; `maxWords: 40`. 개별 줄을 읽기 쉽고 연기 가능한 길이로 유지합니다. |
| `dialogue-ratio` | scene | warn | `scene.kind == "scene"`, `bodyNodes >= minNodes`, `dialogueRatio < min`; `minNodes: 10`, `min: 0.35`. 작성된 body에 비해 대사가 너무 적은 선형 씬을 표시합니다. |
| `scene-length-spread` | project | warn | 선형 씬만 대상으로 `scenes >= 2` 및 `spreadRatio > maxRatio`; `maxRatio: 3.0`. 씬 길이 구성이 유난히 고르지 않은지 찾습니다. |
| `section-starts-with-background` | section | warn | `section.kind == "scene"` 및 `firstStagingTag != "bg"`; staging 디렉티브가 없는 섹션도 포함합니다. 선형 씬의 각 섹션이 먼저 배경을 설정하도록 권장합니다. |
| `emotion-distribution` | speaker | warn | 화자가 최소 `minLines: 10` 이상일 때 선택한 axis를 검사합니다: `domain: emotion`, 선택적 `pairWith`, `runMax: 3`, `streakAvgMin: 1.5`, `maxShare: 0.4`. 동일한 감정 streak 최대 3개, 평균 streak 길이 최소 1.5, dominance 최대 40%라는 upstream lineage의 hard cap을 적용하며, `pairWith`가 있으면 짝지은 axis도 검사합니다. 실패하는 화자마다 결과 하나를 내고 모든 이유를 합칩니다. streak는 한 번 앉아서 플레이어가 듣는 범위에서 셉니다. 문서의 섹션과 quest body를 합치고, 각 lore bundle `<beat>`는 따로 셉니다. lore `<entry>` 줄은 entry를 연달아 듣지 않으므로 측정하지 않습니다. |
| `variant-composition` | speaker 및 group | warn | `attr: variant`, 선택적 `groupBy`, `minPerGroup: 2`, `minShare: 0.0`, `minLines: 10`. `groupBy`를 쓰면 `minPerGroup` 미만인 group이 실행됩니다. `minShare > 0`이면 자체 `minLines`를 만족하면서 `attrShare[attr]`가 threshold 미만인 화자가 실행됩니다. |
| `asset-exists` | line/directive | error | `providers: {}`(동작하지 않음), `sentinels: [clear, empty, false, none, null, stop]`. 매핑된 directive tag마다 `assetId`를 고정된 provider snapshot과 대조합니다. 없는 asset은 결과를 내며 오래된 catalog data는 결과를 warn으로 낮춥니다. Sentinel 값은 대소문자를 구분하지 않고 제외됩니다. |

0.22.0부터 세 선형-VN 규칙 — `dialogue-ratio`, `scene-length-spread`, `section-starts-with-background` — 은 선형 씬만 판정합니다. beat는 엔진이 이미 stage를 만든 순간에 제시되고, component·quest·lore entry는 다른 사람의 staging에 삽입되므로, [beats와 occasions](/ko/tooling/play/)를 기반으로 만든 프로젝트에서 모든 bark마다 `L-SECTION-STARTS-WITH-BACKGROUND`가 쏟아지지 않습니다. beat에 자체 규범을 적용하려면 `scene.kind == "beat"`(또는 `section.kind == "beat"`)로 가드하는 `custom:` 규칙을 작성하세요.
## 표시 이름

dsl 0.26.0부터 `lute lint`는 `W-DISPLAY-NAME-DUP`도 보고합니다(dsl 0.26.0 §2.8). 이는 대화 상자가 같은 이름으로 표시하는 서로 다른 두 화자입니다. id는 프로젝트 전체에서 유일하지만 플레이어는 이름을 읽으며, 각자 "Hiker Gus"를 캐스트한 두 작가는 한 사람을 두 번 쓴 셈입니다. 정확히 같은 `name:`을 가진 두 cast 항목, component의 `::use{… name="…"}` display string과 같은 cast 이름, 서로 다른 화자(`who=`)에 대한 그런 문자열 두 개에서 실행됩니다. 이는 프로젝트 전체 `check`도 보고하는 자문이며 `L-*`가 아닌 `W-` 코드를 유지합니다. 비교 범위는 해당 경로가 속한 프로젝트 전체이므로 한 파일을 lint해도 모든 cast 항목을 비교합니다. 결과는 이름 중 하나를 말하는 첫 줄 또는 어느 것도 말하지 않으면 문서의 첫 줄에 연결됩니다:

<!-- lute-diagnostics -->
```console
$ lute lint .
lore/trainers.lute:12:3: warning [W-DISPLAY-NAME-DUP] display name `Hiker Gus` is shown for 2 different speakers: `gus` (lore/trainers.lute:12), `r16Gus` (the cast, never spoken) — the dialogue box cannot tell them apart; rename one
ok: . (0 error(s), 1 warning(s))
```

여러 화자가 의도적으로 공유하는 역할 이름 — "Eclipse Grunt", "Gym Guide" — 은 충돌이 아닙니다. 각 cast 항목에 `sharedName: true`를 표시하면 계산되지 않습니다:

```yaml
cast:
  grunt1: { name: Eclipse Grunt, sharedName: true }
  grunt2: { name: Eclipse Grunt, sharedName: true }
```

이 코드는 `lute.lint.yaml` 규칙이 아니므로 그곳에서 level을 설정할 수 없습니다(`rules:`에서 이 이름을 지정하면 알 수 없는 rule id인 `E-LINT-CONFIG`입니다). `--deny W-DISPLAY-NAME-DUP` 또는 `--deny-warnings`는 이를 오류로 승격하고 실행을 실패시킵니다:

<!-- lute-diagnostics -->
```console
$ lute lint . --deny W-DISPLAY-NAME-DUP
lore/trainers.lute:12:3: error [W-DISPLAY-NAME-DUP] [denied] display name `Hiker Gus` is shown for 2 different speakers: `gus` (lore/trainers.lute:12), `r16Gus` (the cast, never spoken) — the dialogue box cannot tell them apart; rename one
failed: . (1 error(s), 0 warning(s))
```

`lute check --project . --deny-warnings`로 merge를 gate하는 프로젝트는 먼저 역할 이름에 `sharedName: true`를 표시합니다. [여러 작가 가이드](/ko/guides/multi-author/)를 참고하세요.

## 플러그인 규칙 작성

플러그인은 `plugin.yaml`의 `exports:` 아래에 `lints: lints/`를 나열하고 `lints/*.yaml` 파일을 배포합니다. 각 파일은 `custom`과 같은 규칙 형태의 최상위 `lints:` 목록입니다:

```yaml
lints:
  - id: variant-coverage
    target: speaker
    when: 'speaker.lines >= 10 && speaker.attrShare["variant"] < options.minShare'
    level: warn
    message: "speaker has {speaker.lines} dialogue lines"
    options: { minShare: 0.5 }
```

로더는 `id`를 원문 그대로 유지합니다. 플러그인 id가 `my-plugin`이면 활성 규칙 id는 `my-plugin/variant-coverage`이며, 사용자가 `rules:` 아래에 넣는 이름이자 진단 코드의 기반입니다. 잘못된 플러그인 항목은 로드 시 `E-LINT-RULE`이며 건너뜁니다. 플러그인 규칙은 프로젝트의 **기본 프로필** 활성화에서 옵니다. 프로젝트 루트마다 규칙 집합 하나를 모든 문서에 적용합니다(문서별 프로필 활성화는 향후 개선 사항입니다).

플러그인 규칙은 고정된 코어 계산 메트릭 표에 대해 단언할 때만 데이터로 취급됩니다. 규칙에 다른 메트릭, 순회, 순서 규칙 또는 평가 기본 요소가 필요하면 YAML 확장이 아니라 코어 변경입니다. 플러그인 lint 선언은 자문용이며 capability snapshot과 `capabilitySnapshot` 모두에서 제외됩니다.

## 코드, 거부 및 종료 상태

Lint 결과 코드는 rule id를 대문자로 바꾸고 `/`와 영숫자가 아닌 모든 문자를 `-`로 바꾼 `L-` + 결과입니다. `dialogue-length`는 `L-DIALOGUE-LENGTH`, `my-plugin/variant-coverage`는 `L-MY-PLUGIN-VARIANT-COVERAGE`가 됩니다.

내장 규칙 코드는 다음과 같습니다:

| 규칙 | 코드 |
| --- | --- |
| `dialogue-length` | `L-DIALOGUE-LENGTH` |
| `dialogue-ratio` | `L-DIALOGUE-RATIO` |
| `scene-length-spread` | `L-SCENE-LENGTH-SPREAD` |
| `section-starts-with-background` | `L-SECTION-STARTS-WITH-BACKGROUND` |
| `emotion-distribution` | `L-EMOTION-DISTRIBUTION` |
| `variant-composition` | `L-VARIANT-COMPOSITION` |
| `asset-exists` | `L-ASSET-EXISTS` |

`--deny CODE`를 반복해 다른 의미는 바꾸지 않고 특정 lint 코드를 오류로 승격할 수 있습니다. `--deny-warnings`는 경고도 승격합니다. 승격은 종료 상태에 영향을 줍니다.

| 종료 | 의미 |
| --- | --- |
| `0` | 정상 또는 오류 심각도 미만인 결과만 있음. |
| `1` | 오류 심각도 lint 결과가 하나 이상 있음(기본 또는 거부 승격), 또는 의미론적 config 오류. |
| `2` | I/O 오류, 잘못된 YAML 또는 CLI 사용법 오류. |
