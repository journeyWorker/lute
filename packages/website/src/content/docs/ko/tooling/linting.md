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
| `lsp` | boolean | `false` | 이 config 파일이 존재하고 `true`로 설정된 경우에만 LSP의 열린 문서에 lint 결과를 발행합니다. 규칙이 설정되어 있어도 그 밖에는 LSP에서 lint가 없습니다. LSP는 `line`, `section`, `scene`, `speaker`, `group`, (0.50부터) `node` 대상을 평가하며, `project` 규칙은 v1에서 CLI 전용입니다. |
| `ignore` | 프로젝트 루트 기준 glob 목록 | `[]` | lint에서 제외할 문서입니다. |
| `rules` | map | `{}` | core 및 활성 플러그인 규칙에 대한 규칙별 level과 option override입니다. |
| `custom` | 규칙 목록 | `[]` | 프로젝트 로컬 선언적 CEL 규칙입니다. |

`rules` map 항목은 level 축약형(예: `my-plugin/variant-coverage: off`) 또는 `{ level, options }`일 수 있습니다. level은 `off`, `hint`, `info`, `warn`, `error`이며 진단 심각도 Hint, Info, Warning, Error에 대응합니다. `off`는 규칙을 비활성화합니다. level을 생략하면 해당 규칙의 기본값을 유지합니다. option은 규칙이 선언한 기본값에 deep-merge되고, scalar 값은 override합니다.

Core 규칙 id는 bare kebab-case입니다. 플러그인 규칙 id는 `<plugin-id>/<rule-id>`입니다. custom 규칙 id는 bare여야 하며 core 규칙 id와 충돌해서는 **안 됩니다**. 잘못된 YAML은 종료 코드 2 오류입니다. 알 수 없는 규칙 또는 option 키, 잘못된 option 타입, 충돌하는 custom id는 `E-LINT-CONFIG` 결과입니다. `rules:`에 **checker** 코드를 지정하는 것도 `E-LINT-CONFIG`이며(0.50부터) 올바른 파일을 알려주는 힌트가 붙습니다: `` `W-STAGE-ABSENT` is a checker code, not a lint rule — set its level in `lute.project.yaml` `diagnostics:` ``. `E-*` id는 힌트가 `an error cannot be re-levelled`로 끝납니다. Lint 규칙 level(`L-*`)은 여기서만 설정합니다. 잘못된 항목은 건너뛰고 나머지 규칙을 실행하며 명령은 1로 종료합니다.

### Custom 규칙 형태

모든 custom 항목은 plugin 규칙과 같은 형태입니다:

| 키 | 필수 | 의미 |
| --- | --- | --- |
| `id` | 예 | 프로젝트 로컬 bare 규칙 id입니다. |
| `target` | 예 | `line`, `section`, `scene`, `speaker`, `group`, `project`, `node`(0.50부터), `pair`(0.51부터) 중 하나입니다. |
| `when` | 예(`pair` 제외) | 대상 행과 `options`에 대한 CEL assertion이며 `true`일 때 실행됩니다. `pair` 규칙은 `when` 대신 `open`과 `close`를 받고 `when`은 거부합니다([`pair` lint](#pair-lint-051부터) 참고). |
| `level` | 아니요 | `off`, `hint`, `info`, `warn`, `error`입니다. |
| `message` | 예 | 결과 텍스트입니다. `{path.to.field}`는 metric 또는 option 경로를 보간합니다. 숫자는 끝의 `.0`을 제거하고 최대 두 자리로 반올림하여 출력하므로 ratio `3.3333…`은 `3.33`으로 표시됩니다. `{expr:%}`는 share를 반올림한 백분율로 출력합니다. |
| `options` | 아니요 | `when`과 message에 노출되는 기본값입니다. |

Lint CEL fragment는 ground-only입니다. `@ref`나 `$` DSL token, 상태 경로, macro(`filter`, `map`, `exists`)를 쓸 수 없습니다. 공유 ground-operation semantics, metric-path 선택 및 map indexing, 문자열·목록·map에 대한 `size()`를 지원합니다. 해석할 수 없는 field, 타입 불일치 또는 boolean이 아닌 `when` 결과는 `E-LINT-EXPR`이며 규칙 선언 위치에서 한 번 보고되고 해당 규칙은 건너뜁니다. 절대로 조용한 통과가 아닙니다.

0.50부터 fragment에는 표준 CEL 문자열 멤버 `s.startsWith(t)`, `s.endsWith(t)`, `s.contains(t)`도 있습니다. 멤버 호출 형태만 가능하고 두 피연산자 모두 문자열이어야 하며, 문자열이 아닌 피연산자는 평가 오류(`E-LINT-EXPR`)입니다. 정규식과 macro는 없습니다.

## `node` lint (0.50부터)

`target: node`(플러그인 `lints:` export와 `lute.lint.yaml`의 `custom:` 규칙에서 유효)는 문서의 **노드**마다 규칙을 한 번씩 평가합니다. 행은 checker가 순회하는 모든 body를 문서 순서대로 따라갑니다:

- 섹션 body;
- `<quest>` 선언과 그 body(objective, `<on>` 핸들러);
- quest 또는 objective의 자식 `<reward>`(소유자의 자식 중 소스 위치대로);
- 최상위 `<entry>`·`<beat>` 선언과 그 body;
- `<branch>`의 choice와 choice body;
- `<hub>`의 choice, choice body, `<return>` body;
- `<match>` arm(`<when>`, `<otherwise>`)과 그 body;
- `<timeline>`(한 행이며 트랙 clip은 행이 아닙니다).

| `node.kind` | 대상 |
| --- | --- |
| `line` | 콘텐츠 줄 |
| `directive` | 모든 `::tag{…}`: 플러그인·core 디렉티브, `::set`, `::assert`, `::retract`, `::accept`, `::use`, `::label`, `::jump`, `::clear`, `::end` |
| 요소 이름 | `branch`, `hub`, `choice`, `return`, `match`, `when`, `otherwise`, `timeline`, `quest`, `objective`, `on`, `reward`, `entry`, `beat` |

`node` 규칙은 세 binding을 읽습니다: `node`, `scene`(기존 문서 행), `options`.

| 필드 | 의미 |
| --- | --- |
| `node.kind` | 위 표와 같음 |
| `node.tag` | 디렉티브 tag 또는 요소 이름. 줄은 `""` |
| `node.speaker` | 줄의 화자. 그 외는 `""` |
| `node.attrs` | **작성된 그대로**의 모든 attribute를 담은 문자열 → 문자열 map(아래 참고) |
| `node.index`, `node.first`, `node.last` | body 안에서의 0부터 시작하는 위치. 해당 body의 첫/마지막 여부 |
| `node.depth` | 섹션(또는 문서)의 최상위 노드는 0, 감싸는 요소마다 +1 |
| `node.prev`, `node.next` | 같은 body의 이웃 노드 `{ kind, tag, attrs }`. body 가장자리에서는 모든 필드가 비어 있음 |
| `node.parent` | 감싸는 요소의 `{ kind, tag, attrs }`. 섹션 최상위에서 `kind`는 `"section"`, quest/lore 문서 최상위에서는 `"document"` |
| `node.preceding` | tag → `{ attrs }` map: 각 tag에 대해, 이 노드와 각 조상의 앞쪽 형제 중 가장 가까운 같은 tag의 노드와 조상 자신. 앞쪽 형제의 body 안으로는 들어가지 **않습니다**(다른 arm은 이 경로 위에 있지 않음). 섹션과 선언마다 초기화됩니다. 줄은 키가 되지 않습니다. |
| `node.section` | 감싸는 섹션의 `{ index, title, id }`. 섹션 밖은 `{ 0, "", "" }` |

**위치와 메시지**는 `line` 행과 같습니다. 진단은 노드(줄, 디렉티브 또는 요소)를 가리키며 `{node.attrs.value}`를 보간할 수 있습니다.

**작성된 attribute.** `node.attrs`는 작가가 쓴 키만 가지며, 값은 따옴표를 벗긴 작성 값 텍스트입니다. 파서가 필드로 끌어올리는 attribute(`<branch id>`, `<reward kind target amount>`, `::jump{to}`)와 플러그인 stamp도 포함합니다. 값 없는 flag는 `"true"`이고 `@ref` 값은 소스 텍스트(`@name`)입니다. IR에 기본값이 있어도 작성하지 않은 attribute는 없습니다. 따라서 `amount`가 없는 `<reward>`에는 `amount` 키가 없습니다. `::set`, `::assert`, `::retract`는 `{}`입니다. 텍스트는 파싱된 필드에서 재구성하지 않고 파서 자신의 attribute scanner로 소스에서 다시 읽습니다. `<reward kind="COINS" amount="+005"/>`는 `"5"`가 아니라 `node.attrs["amount"] == "+005"`입니다. CRLF 줄바꿈과 멀티바이트 텍스트에서도 같은 값이 나옵니다.

**모든 인덱스를 가드하세요.** 없는 키는 CEL 의미를 유지합니다. 없는 키에 대한 `node.attrs["x"]`는 평가 오류(`E-LINT-EXPR`)입니다. `&&`는 단락 평가하므로 규칙은 사용하는 **모든** 인덱스를 가드합니다: `"x" in node.attrs && node.attrs["x"] …`, `"t" in node.preceding && "k" in node.preceding["t"].attrs && …`, options map을 인덱싱하기 전의 `key in options.cases && …`.

`node.preceding`은 행마다 복사하지 않고 하나의 진행 중인 map을 빌려 읽는 view로 제공되므로, 이를 읽는 `node` 규칙도 문서 크기에 거의 선형으로 유지됩니다. LSP는 다른 문서 내 대상처럼 `node` 규칙을 평가합니다. `node` 규칙은 IR, 이벤트, snapshot, `capabilitySnapshot`을 바꾸지 않고 semantic id도 없습니다.

### 레시피

모든 인덱스를 가드합니다. **알 수 없는 discriminator 정책:** 의존 값 규칙은 자신이 case를 가지지 않는 discriminator를 제외하고, 별도의 `*-known` 규칙이 discriminator 자체를 보고합니다. 따라서 각 위반은 그것을 지목하는 규칙이 한 번만 보고합니다.

```yaml
lints:
  - id: signal-needs-menu
    target: node
    when: 'node.tag == "signal" && "value" in node.attrs && node.attrs["value"] == "puzzle" && node.next.kind != "branch"'
    level: warn
    message: "a puzzle ::signal must be followed by a <branch>"
  - id: section-opens-with-bg
    target: node
    when: 'node.first && node.parent.kind == "section" && node.tag != "bg"'
    level: warn
    message: "section {node.section.title} opens with {node.tag}"
  - id: beat-label-prefix
    target: node
    when: 'node.tag == "label" && scene.kind == "beat" && "name" in node.attrs && !node.attrs["name"].startsWith("b_")'
    level: error
    message: "a beat's labels start with b_"
  # #177, same element: `mode` selects the legal `speed`s.
  - id: move-mode-known
    target: node
    when: 'node.tag == "move" && "mode" in node.attrs && !(node.attrs["mode"] in options.speeds)'
    level: error
    message: "unknown move mode {node.attrs.mode}"
    options: { speeds: { walk: [slow, normal], run: [normal, fast] } }
  - id: speed-fits-mode
    target: node
    when: >-
      node.tag == "move" && "speed" in node.attrs && "mode" in node.attrs &&
      node.attrs["mode"] in options.speeds &&
      !(node.attrs["speed"] in options.speeds[node.attrs["mode"]])
    level: error
    message: "speed {node.attrs.speed} is not legal in mode {node.attrs.mode}"
    options: { speeds: { walk: [slow, normal], run: [normal, fast] } }
  # #177, a marker directive before the branch decides the legal option values.
  - id: interaction-kind-known
    target: node
    when: 'node.tag == "interaction" && (!("kind" in node.attrs) || !(node.attrs["kind"] in options.cases))'
    level: error
    message: "::interaction needs a known kind"
    options: { cases: { greet: [wave, bow], gift: [accept, decline] } }
  - id: option-fits-interaction
    target: node
    when: >-
      node.tag == "option" && "value" in node.attrs &&
      "interaction" in node.preceding && "kind" in node.preceding["interaction"].attrs &&
      node.preceding["interaction"].attrs["kind"] in options.cases &&
      !(node.attrs["value"] in options.cases[node.preceding["interaction"].attrs["kind"]])
    level: error
    message: "option {node.attrs.value} does not belong to this interaction"
    options: { cases: { greet: [wave, bow], gift: [accept, decline] } }
  # #177 with #164 stamps: the parent <branch> carries the discriminator.
  - id: menu-type-known
    target: node
    when: 'node.kind == "branch" && "menuType" in node.attrs && !(node.attrs["menuType"] in options.keys)'
    level: error
    message: "unknown menu type {node.attrs.menuType}"
    options: { keys: { plain: [L1, R1], timed: [T1] } }
  - id: choice-fits-menu
    target: node
    when: >-
      node.kind == "choice" && "optionKey" in node.attrs &&
      "menuType" in node.parent.attrs && node.parent.attrs["menuType"] in options.keys &&
      !(node.attrs["optionKey"] in options.keys[node.parent.attrs["menuType"]])
    level: error
    message: "option key {node.attrs.optionKey} is not a {node.parent.attrs.menuType} key"
    options: { keys: { plain: [L1, R1], timed: [T1] } }
```

의존 attribute, discriminator 또는 marker가 없는 노드(`interaction` 없는 `preceding`, `kind` 없는 marker, `menuType` 없는 parent)에서는 결과도 `E-LINT-EXPR`도 없습니다. 멤버십 검사 없이 `node.attrs["mode"]`를 인덱싱하는 가드 없는 변형은 `mode`가 없는 노드에서 `E-LINT-EXPR`를 보고하며, 이것이 가드가 있는 이유입니다.

## `pair` lint (0.51부터)

`target: pair`(플러그인 `lints:` export와 `lute.lint.yaml`의 `custom:` 규칙에서 유효)는 두 종류의 노드가 짝을 이루어 균형을 맞추는지 검사합니다. `begin`과 그에 맞는 `end`, 즉 open과 close입니다. 플러그인이 디렉티브 두 개로 표시하는 마커를 위한 것이며, `node` 규칙이나 메트릭 표로는 이런 짝을 맞출 수 없습니다.

```yaml
lints:
  - id: group-balanced
    target: pair
    open:   'node.tag == "group" && "mode" in node.attrs && node.attrs["mode"] == "begin"'
    close:  'node.tag == "group" && "mode" in node.attrs && node.attrs["mode"] == "end"'
    inside: 'node.kind == "line" || node.tag in ["actor", "sfx"]'   # optional
    scope: body                                                    # body (default) | section | document
    level: error
    message: "::group begin/end"
```

| 키 | 필수 | 의미 |
| --- | --- | --- |
| `open` | 예 | [`node` 행](#node-lint-050부터)에 대한 lint-CEL 술어입니다. 짝을 여는 노드에서 성립합니다. |
| `close` | 예 | 같은 형태이며, 짝을 닫는 노드에서 성립합니다. |
| `inside` | 아니요 | open과 그 close 사이에 엄격히 놓인 모든 노드가 만족해야 하는 술어입니다. |
| `scope` | 아니요 | `body`(기본값), `section`, `document`(0.53부터)입니다. 아래를 참고하세요. |

`open`, `close`, `inside`는 `node` 규칙과 같은 `node` 행(`kind`, `tag`, `speaker`, 작성된 그대로의 `attrs`, `index`, `first`, `last`, `depth`, `prev`, `next`, `parent`, `preceding`, `section`)과 `scene`, `options` binding, 문자열 멤버를 읽습니다. 없는 키는 CEL 의미를 유지하므로(`E-LINT-EXPR`) [레시피](#레시피)처럼 모든 인덱스를 가드하세요. 오류는 규칙과 술어를 알려 줍니다(`` rule `g2`: `open`: missing key `kind` ``). 로더가 형태를 강제합니다. `pair`에서는 `open`과 `close`가 필수이고 `when`은 거부되며(`` rule `g3`: `when` is refused on `target: pair`, which states `open` and `close` instead ``), 다른 모든 대상에서는 `open`, `close`, `inside`, `scope`가 거부됩니다. `null`로 쓴 키는 없는 것으로 읽지 않고 거부합니다(`` `open` is null; give it a value or leave it out ``). 위반은 플러그인 `lints:`에서는 `E-PLUGIN-PARSE`, `lute.lint.yaml`의 `custom:`에서는 `E-LINT-CONFIG`입니다.

### 평가

규칙은 scope마다 **열린 슬롯 하나**를 유지하며, scope의 노드를 문서 순서로 한 번 훑습니다.

- **`scope: body`**(기본값): 각 body의 직계 자식을 순서대로 봅니다. 형제만 해당합니다. 모든 body(섹션, `<choice>`, `<match>` arm)가 각각 하나의 scope입니다.
- **`scope: section`**: 한 섹션 또는 최상위 선언의 모든 노드를 모든 깊이에서 문서 순서로 봅니다. `node.preceding`이 초기화되는 단위입니다. 경로가 아니라 문서 순서이므로, 한 `<choice>`의 `open`은 형제 choice의 `close`와 짝을 이룹니다.
- **`scope: document`**(0.53부터): 문서 전체에 슬롯 하나. 모든 섹션과 최상위 선언의 모든 행을 모든 깊이에서 문서 순서로 봅니다. `section` 순회에서 `##` 제목이나 선언마다 일어나는 초기화만 뺀 것이므로, 한 섹션에서 연 짝을 이후 섹션이나 이후 최상위 선언에서 닫을 수 있습니다.

각 노드에 대해:

- 짝이 열려 있는 동안 `close`가 성립하는 노드는 그 짝을 **닫습니다**. `open`과 `close`를 모두 만족하는 노드는 짝이 열려 있으면 close이고, 아니면 open입니다.
- 짝이 열려 있는 동안 `open`이 성립하는 노드는 **중첩 open**이며, 바깥 짝은 열린 채로 남습니다.
- 열린 짝이 없는데 `close`가 성립하는 노드는 **열리지 않은 close**입니다.
- `inside`가 있으면, open과 그 close 사이에 엄격히 놓인 노드 중 `inside`가 거짓인 노드는 모두 **inside 위반**입니다(`body` scope에서는 형제만 해당하며 그 자손은 해당하지 않습니다).
- scope가 끝났을 때 아직 열려 있는 짝은 **닫히지 않은 open**입니다.

결과는 규칙의 코드와 수준을 가지며 문제가 된 노드를 가리키고, 렌더링된 `message` 뒤에 고정된 문구 하나가 붙습니다.

| 위반 | 덧붙는 문구 |
| --- | --- |
| 닫히지 않은 open | `` — opened here, with no close in this body `` (`scope: section`에서는 `in this section`, `scope: document`에서는 `in this document`) |
| 열리지 않은 close | `` — a close with no open pair in this body `` (`in this section`, `in this document`) |
| 중첩 open | `` — opened again while the pair from line N is still open `` |
| inside 위반 | `` — not allowed inside the pair opened at line N `` |

중첩 open과 inside 결과는 open 노드를 `related`로도 가리킵니다.

위 규칙에 다음 body를 적용해 보겠습니다. `::group`이 닫히지 않습니다.

```lute
::group{mode="begin"}
@narrator: The crowd gathers.
@narrator: Nobody speaks.
```

`group-balanced`는 `begin`에서 `::group begin/end — opened here, with no close in this body`를 보고합니다(코드는 `acme.flow` 플러그인이 내보낼 때 `L-ACME-FLOW-GROUP-BALANCED`, `lute.lint.yaml`의 `custom:` 규칙일 때 `L-GROUP-BALANCED`입니다). begin 없는 `::group{mode="end"}`는 `::group begin/end — a close with no open pair in this body`, `end` 전에 나온 두 번째 `begin`은 `::group begin/end — opened again while the pair from line N is still open`으로 읽힙니다.

### `inside` 규칙

`inside`는 마커 사이에 놓일 수 있는 것을 제한합니다. 위의 `inside` 술어(줄, `::actor`, `::sfx`)에서는 마커 사이의 다른 디렉티브가 모두 위반입니다.

```lute
::group{mode="begin"}
@narrator: The crowd gathers.
::bg{assetId="street.night"}
::group{mode="end"}
```

`::bg`는 `::group begin/end — not allowed inside the pair opened at line 1`로 보고되며, `begin`이 `related`입니다.

### `section` scope 짝

`scope: body`에서는 `<branch>` 앞의 `begin`과 choice 안의 `end`가 서로 다른 body에 있습니다. `begin`은 닫히지 않은 open, `end`는 열리지 않은 close가 됩니다. `scope: section`은 문서 순서로 branch를 가로질러 둘을 짝짓습니다.

```yaml
lints:
  - id: fx-balanced
    target: pair
    open:   'node.tag == "fx" && "mode" in node.attrs && node.attrs["mode"] == "begin"'
    close:  'node.tag == "fx" && "mode" in node.attrs && node.attrs["mode"] == "end"'
    scope: section
    level: warn
    message: "::fx begin/end"
```

```lute
::fx{mode="begin"}
<branch id="ask">
  <choice id="stay" text="Stay">
    @narrator: You stay.
    ::fx{mode="end"}
  </choice>
  <choice id="go" text="Go">
    @narrator: You go.
  </choice>
</branch>
```

이 짝은 문서 순서로 균형이 맞으므로 결과가 없습니다. 섹션의 슬롯은 다음 `##` 제목이나 선언에서 초기화되므로, 거기까지 열려 있는 `begin`은 닫히지 않은 open(`in this section`)입니다.

### `document` scope 짝

원본의 샷마다 섹션 하나를 두는 포팅에서는 한 섹션에서 연 효과를 이후 섹션에서 닫는 일이 흔합니다. `scope: section`에서는 앞 섹션의 `begin`이 닫히지 않은 open, 뒤 섹션의 `end`가 열리지 않은 close가 됩니다. `scope: document`는 둘을 짝짓습니다. 프로젝트의 `lute.lint.yaml`에 쓰는 규칙은 다음과 같습니다.

```yaml
custom:
  - id: music-balanced
    target: pair
    open:   'node.tag == "music" && node.attrs["playback"] == "start"'
    close:  'node.tag == "music" && node.attrs["playback"] == "stop"'
    scope: document
    level: error
    message: "::music start/stop"
```

두 섹션에 짝이 걸쳐 있는 씬의 본문입니다.

```lute
## Arrival

::bg{assetId="BG.road"}
::music{assetId="BGM.walk" playback="start"}
@narrator: The walk takes a while.

## Gate

::bg{assetId="BG.gate"}
@narrator: Here we are.
::music{assetId="BGM.walk" playback="stop"}
```

```console
$ lute lint scenes/walk.lute
ok: scenes/walk.lute (0 error(s), 0 warning(s))
```

`scope: section`이면 같은 씬이 두 건의 결과를 냅니다. `scope: document`에서 `stop` 줄을 지우면 문서 끝에서 open이 보고됩니다.

<!-- lute-diagnostics unverified="verbatim lute lint output; L-MUSIC-BALANCED is the example's own custom rule message from lute.lint.yaml" -->
```console
$ lute lint scenes/walk.lute        # scope: section
scenes/walk.lute:12:1: error [L-MUSIC-BALANCED] ::music start/stop — opened here, with no close in this section
scenes/walk.lute:19:1: error [L-MUSIC-BALANCED] ::music start/stop — a close with no open pair in this section
failed: scenes/walk.lute (2 error(s), 0 warning(s))
$ lute lint scenes/walk.lute        # scope: document, no `stop`
scenes/walk.lute:12:1: error [L-MUSIC-BALANCED] ::music start/stop — opened here, with no close in this document
failed: scenes/walk.lute (1 error(s), 0 warning(s))
```

(줄 번호는 frontmatter를 포함한 전체 파일 기준입니다.) `node` 규칙에는 scope가 없고, `scope`는 `pair` 이외의 target에서는 계속 거부됩니다.

### 실행 위치와 비용

`pair` 규칙은 `node` 규칙과 같은 노드 순회에서 `lute lint`와 언어 서버 모두 실행되며, 각 규칙은 노드마다 그 자리에서 평가되고 노드별로 저장되는 것은 없습니다. 순회는 `node` 또는 `pair` 규칙이 활성일 때 실행됩니다. 규칙의 술어에서 처음 나온 `E-LINT-EXPR`는 `node` 규칙과 마찬가지로 그 문서에 대해 해당 규칙을 중단시킵니다. 상태 기계 자체의 오버헤드는 선형입니다. `pair` 규칙마다 노드당 술어 호출은 최대 세 번이고 슬롯 작업은 상수입니다. 술어가 하는 일은 선형이라고 주장하지 않습니다. 예를 들어 긴 문자열의 `size()`는 문자 단위로 세므로, N개 노드마다 N자를 읽는 술어의 비용은 N²입니다. 작업 카운터가 이 둘을 구분해 보고합니다. `pair_evals`(술어 호출 횟수), `pair_slots`(슬롯 열기·닫기와 프레임 push), 문자열 연산이 훑는 문자 수를 세는 `eval_chars`입니다. `pair` 규칙은 IR, 이벤트, snapshot, `capabilitySnapshot`을 바꾸지 않으며 semantic id도 없습니다.

**lint 비용(0.53).** `lute lint`는 프로젝트의 공유 데이터를 실행당 한 번만 빌드하고, 피연산자와 매니페스트, 플러그인, provider, 그리고 이들이 닿는 import만 읽습니다. 프로젝트의 다른 문서는 읽지 않습니다. 꺼진 규칙은 문서당 비용이 없고, 활성 규칙이 없으면 엔진이 문서별 작업 전에 반환합니다. `LUTE_STATS=input-cache`가 빌드 횟수를 보여 줍니다([비용 카운터](/ko/tooling/cli/#비용-카운터-lute_stats) 참고). 무관한 형제 문서를 읽을 수 없어도 더 이상 lint가 실패하지 않으며, 읽을 수 없는 피연산자는 자체 I/O 오류로 보고됩니다([`lute lint`](/ko/tooling/cli/#lint) 참고).

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

`asset-exists`의 `options.providers` 키는 셀렉터입니다(0.49.0):

| 키 | 검사 대상 |
| --- | --- |
| `<tag>` | `::<tag>`의 `assetId`(0.49 이전의 의미) |
| `<tag>.<attr>` | 해당 directive의 그 attribute |
| `@<attr>` | 모든 콘텐츠 줄의 그 attribute |

```yaml
asset-exists:
  level: error
  options:
    providers: { bg: bg-catalog, sfx.voiceAssetId: voice-catalog, "@assetId": portrait-catalog }
```

그 밖의 키 형태는 `E-LINT-CONFIG`입니다. 리터럴 문자열만 검사하며 `sentinels`가 적용됩니다. `<tag>` 결과는 이전과 같고, 나머지는 출처를 밝힙니다. 줄 attribute는 `` (`@mira` attribute `assetId`) ``, directive attribute는 `` (`::sfx` attribute `voiceAssetId`) ``입니다.

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

이 코드는 `lute.lint.yaml` 규칙이 아니므로 그곳에서 level을 설정할 수 없습니다. `rules:`에서 이 이름을 지정하면 올바른 위치를 알려주는 힌트와 함께 `E-LINT-CONFIG`입니다: `` `W-DISPLAY-NAME-DUP` is a checker code, not a lint rule — set its level in `lute.project.yaml` `diagnostics:` ``(0.50부터. `E-*` id는 힌트가 `… an error cannot be re-levelled`로 끝납니다). 프로젝트의 `diagnostics:` 키([CLI 페이지](/ko/tooling/cli/))는 checker `W-*` 코드를 `off`, `info`, `warn`, `error`로 설정합니다. `--deny W-DISPLAY-NAME-DUP` 또는 `--deny-warnings`는 이를 오류로 승격하고 실행을 실패시킵니다:

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

플러그인 규칙은 고정된 코어 계산 메트릭 표에 대해 단언할 때만 데이터로 취급됩니다. 규칙에 다른 메트릭, 순회, 순서 규칙 또는 평가 기본 요소가 필요하면 YAML 확장이 아니라 코어 변경입니다. 구조 술어(노드가 무엇인지, 주변에 무엇이 있는지, 작가가 어떤 attribute를 썼는지)는 [`node` 대상](#node-lint-050부터)이 맡고, 균형 검사는 [`pair` 대상](#pair-lint-051부터)(0.51)이 맡습니다. 플러그인 lint 선언은 자문용이며 capability snapshot과 `capabilitySnapshot` 모두에서 제외됩니다.

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
