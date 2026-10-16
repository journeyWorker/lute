---
title: AI 하네스 가이드
description: Lute를 AI 작성 파이프라인에 연결하는 방법 — 신뢰된 permission-profile 상한, 필터링된 lute context, lute check --json 피드백 루프, 종료 코드, 진단, 증명과 검토의 경계입니다.
---
## 기계 판독 CLI 계약 (0.43)

JSON을 지원하는 명령은 이제 배열이 아니라 `command`, `schemaVersion`, `ok`와 명령별 payload를 담은 하나의 객체를 출력합니다. 실패는 `ok: false`와 `error`(`code`, `message`, 선택적 `path`·`argument`·`hint`)로 표현됩니다. stdout을 파싱하기 전에 종료 코드를 확인하고, 프로젝트 해석·사용법 오류는 stderr도 보존하세요. 현재 명령 ID와 schema 버전은 `lute schema --json`으로 조회합니다. 재현 가능한 예에서는 `LUTE_TELEMETRY=0`을 사용하세요.

Lute는 모델이나 다른 작성 도구로 구동할 수 있지만, 도구가 권위자가 되지는 않습니다. 하네스는 문장이 아니라 종료 코드와 JSON을 읽으며, 호스트가 프로젝트와 [capability-permission 상한](/ko/tooling/capability-permissions/)을 독립적으로 선택합니다. 작동하는 루프는 **신뢰된 context 입력 → 생성 → 같은 상한으로 검사 → 승격 → 태그**입니다. Lute 자체는 AI나 원격 서비스를 호출하지 않습니다.

## 프롬프트 context: `lute context --json`

추측이 아니라 프로젝트의 *작성 표면*에서 모델을 시드하세요:

```sh
lute context scene.lute --json --project . \
  --permission-profile generated
```

프로젝트가 해석한 디렉티브, attrs, enum, 아티팩트 종류, provider, 상태 스키마, 관계형 어휘, 가져온 컴포넌트, 인라인 `textModifiers`, 유효한 권한 계층, `capabilitySnapshot`을 출력합니다. JSON에서 `permissions`는 `{ "layers": [...] }`이고, `bridges`에는 허용된 bridge 객체가 들어가며, `rewardKinds`는 허용된 이름 키 객체이고, `questsAllowed`는 boolean입니다. `directives`에서도 디렉티브 또는 bridge 정책으로 차단된 항목은 제외됩니다. 읽기 전용 외부 상태는 계속 표시됩니다. 이는 검증이 아니라 capability **질의**이므로 문서 자체에 진단이 있어도 출력합니다(종료 `0`). 그리고 핵심 속성으로, **빈 파일에서도 작동합니다**. 표면은 문서 본문이 아니라 해석된 프로젝트와 플러그인에서 오기 때문입니다. `capabilitySnapshot`을 프롬프트 캐시 키로 사용하세요. 제한적인 유효 권한은 이를 바꾸지만, 정책이 없는 프로젝트는 이전 해시를 바이트 단위로 그대로 유지합니다.

0.22.0부터 표면에는 모델이 그 밖에는 형제 파일을 읽거나 추측해야 하는 내용도 담깁니다:

- **`defs`** — 문서가 `@name`으로 읽을 수 있는 모든 이름 있는 조건. 각각 `{ name, type, params, body }`입니다(`defs: { firstDay: "run.day == 1" }`라는 축약형은 추론된 `type: "bool"`과 빈 `params`로 도착합니다).
- **`builtinDirectives`** — 언어 자체의 `::set`, `::assert`, `::retract`, `::accept`, `::use`. 각각 `{ name, syntax, meaning }`이며 `directives`에는 프로젝트와 플러그인의 것만 나열됩니다.
- **`ids`** — `{ scenes, quests, entries }`: 전체 `--project`에서 모든 씬 id는 `visited('<id>')`로, 모든 퀘스트 id는 `quest.<id>.state`와 `::accept`로, 모든 로어 엔트리 id는 `entry.<id>.read` / `entry.<id>.everRead`로 가드가 읽습니다( `--project`가 없으면 문서 자신의 id만).
- `relations`에서 각 관계의 `tier`와 `reserved` 여부, `stateSchema`에서 엔진이 쓰는 경로의 `owner: "engine"`(콘텐츠가 이를 `::set`으로 쓰면 `E-ENGINE-OWNED-WRITE`)가 표시됩니다.
- `occasions`에서 각 계기의 `description`과 `target` — `false`, `true`(모양만 검사), 또는 비트 대상이 `<prefix>.<member>`여야 하는 `{ prefix, entity }` 도메인 — 이 표시됩니다.
- 인간용 개요의 **컴포넌트 시그니처** — 매개변수와 타입을 포함해 가져온 각 컴포넌트가 표시되고, JSON의 `components`에도 이미 들어 있습니다. 따라서 모델은 이름만이 아니라 시그니처에 맞춰 `::use{component="…" …}`를 씁니다.
- **`textModifiers`** — 줄의 텍스트가 사용할 수 있는 인라인 modifier 표면: core의 `:pause{s=…}` leaf와 `:speed[…]{rate=…}` span, 각각의 `syntax`와 `meaning`, 그리고 modifier가 적용된 줄이 lower되는 방식입니다. 그 밖의 `:name[…]`은 프로젝트의 `textStyle` 도메인(`projectEnums`/`enums`)의 멤버여야 하며, 절대로 만들어 내면 안 됩니다.

인간용 개요에도 같은 추가 정보가 출력됩니다 — `defs (N):`, `builtinDirectives (N):`, `scenes`/`quests`/`entries` id 목록, `run.day: int (owner: engine)`, `knows/1(item) [run]`, `talk (select: first, target: npc.<npc>)`입니다.

## 피드백 루프: `lute check --json`

각 생성 후 직렬화된 진단을 다시 입력하세요:

```sh
lute check scene.lute --json --project . \
  --permission-profile generated
```

context, check, compile, compile-stream에 **같은 신뢰된** `--permission-profile`을 사용하세요. 이름은 호스트/프로젝트 설정에서 해석되므로 생성된 소스가 이를 선택하게 하지 마세요. 소스가 작성한 `profile: authored`는 추가 호스트 상한을 넓힐 수 없습니다. 거부된 소스는 작성된 위치에서 억제할 수 없는 `E-PERMISSION-*` 오류입니다.

경고가 루프를 차단하게 하려면 rustc/clippy 스타일 플래그(0.6.1 §5)로 승격하고, `check-project`에서도 사용하세요:

```sh
lute check scene.lute --json --deny W-LUTE-VERSION-STALE --deny-warnings
```

`--deny <CODE>`(반복 가능)는 정확히 그 코드를 판정과 종료 코드에서 오류로 취급하고, `--deny-warnings`는 모든 경고를 승격합니다. 승격된 진단은 심각도 `error`를 보고하며 JSON에 `"denied": true`를 담아 기본 오류와 구별합니다. 알 수 없는 코드는 사용법 오류(종료 `2`)입니다. 오류는 절대로 강등되지 않습니다.

추가만 가능한 생성된 continuation에서는 모델별 JSON 문법을 발명하거나 파서 framing을 컴파일로 취급하지 마세요. 호스트가 소유하고 검사한 씬 템플릿을 [`lute compile-stream`](/ko/tooling/continuation-compiler/)에 주고, 일반적인 최종 섹션 Lute 본문 텍스트를 stdin으로 스트리밍하세요:

```sh
lute compile-stream scene.lute --project . \
  --permission-profile generated
```

수락된 완전한 단위마다 완전하게 검사된 아티팩트 스냅숏을 flush합니다. 프로젝트, capability, 권한 입력은 스트림 동안 고정되며, 같은 상한으로 템플릿과 누적된 모든 본문 후보를 검사합니다. 금지된 단위는 거부된 IR이 출력되기 전에 terminal `error` 레코드를 생성합니다. 그래도 호스트가 신뢰, 런타임 권한 부여, 효과, 영속성, 출력 idempotency를 소유합니다. 권한은 런타임 샌드박스가 아닙니다.

## capability-resolution 오류가 종료 코드를 결정합니다

일부 오류는 문서 위치의 span이 아니라 **프로젝트**를 설명합니다. 예를 들면 존재하지 않거나 선언된 타입에 맞지 않는 플러그인 옵션, 알 수 없는 토큰을 이름 붙인 `identity:` 템플릿, 설치되지 않은 플러그인을 활성화한 profile, 잘못된 permission 객체, 또는 빠진 `--permission-profile`입니다. 이런 오류는 `lute:` 채널 — stderr, `lute: <CODE>: <message>` — 에 출력되며, 붙일 문서 위치가 없으므로 `--json` 진단 목록에는 들어가지 않습니다. 권한 설정 오류는 제한 없는 동작으로 fallback하지 않습니다.

0.8.0부터 이런 오류는 종료 코드를 설정합니다. 이전에는 출력만 하고 빌드가 통과했으므로, 오타 난 플러그인 옵션이 조용히 배포되었습니다:

<!-- lute-diagnostics -->
```console
$ lute check scene.lute --project . --json
lute: E-PLUGIN-OPTION-TYPE: option `showcase.pack.resultScope` expects enum(scene|run), got "galaxy"
$ echo $?
1
```

일반 capability-resolution 동작은 `check`, `check-project`, `compile`(`--all` 포함), `context`, `trace`에서 검증됩니다. 신뢰된 `--permission-profile` 옵션은 의도적으로 더 좁으며 `check`, `compile`(`--all` 포함), `compile-stream`, `context`에 있습니다. 일반 프로젝트 조정의 유일한 의도적 예외는 `compile --project` 뒤의 강제 단일 루트 스캔입니다. 중첩된 하위 프로젝트에 속한 형제 문서가 강제 루트 아래에서 올바르게 해석되지 않는 것은 정당하며, 대상 문서의 잘못이 아닙니다.

**하네스에서 중요한 결과입니다.** `--json`에서 stdout이 비어 있는데 종료 `1`인 상태가 이제 발생할 수 있으며, 이는 프로젝트 수준 오류를 뜻합니다. stdout을 파싱해 "진단 없음"을 "정상"으로 처리하는 루프는 망가진 프로젝트를 성공으로 보고합니다. 먼저 종료 코드에 따라 분기하고, 그 다음 JSON을 파싱하며, 둘이 다르면 stderr을 노출하세요.

이 채널의 플러그인 해석 코드는 `E-PLUGIN-OPTION-UNKNOWN`, `E-PLUGIN-OPTION-TYPE`, `E-PLUGIN-MISSING-ACTIVE`, `E-IDENTITY-TEMPLATE`, 플러그인 로드/조립 계열(`E-PLUGIN-MANIFEST`, `E-PLUGIN-PARSE`, `E-PLUGIN-DUP-ID`, …)입니다. 권한 loader/profile 조회 오류도 같은 프로젝트 해석 채널을 사용하며 권한 context를 명시합니다.

## 종료 코드 계약

| 명령 | 0 | 1 | 2 | 3 |
|---|---|---|---|---|
| `check` / `check-project` | 정상 | 오류 있음 | I/O | — |
| `compile`(`--all` 포함) | 성공 | 게이트 실패 | I/O / 직렬화 | — |
| `compile-stream` | 성공적인 EOF 확정 | 문법 / 의미 / 서비스 거부 | 사용법 / I/O / 잘못된 UTF-8 | — |
| `trace` | 완료 | 거부됨 | I/O | 미완료 |
| `test` | 모든 테스트 통과 | 테스트 실패 | I/O | — |
| `loc import` | 번들 작성 | `E-LOCALE-BUNDLE` | I/O | — |

`trace`에서는 두 실패 모드를 구분하세요: **1 = 거부됨**(check 오류 또는 잘못된 mock — 문서를 고치고 다시 시도) 대 **3 = 미완료**(`unknown` 가드가 walk를 중지 — mock seed를 더 제공하고 다시 시도)입니다. 둘은 서로 다른 재시도 전략을 요구합니다.

`compile --all`은 all-or-nothing입니다. 종료 `1`이면 아무것도 기록되지 않으므로, 부분적으로 갱신된 출력 디렉터리를 하네스가 판단해야 하는 상태는 결코 생기지 않습니다.

### 권한 거부

이 오류는 기본 제공되고 억제할 수 없는 실패입니다. 더 넓은 source profile로 재시도하지 마세요. 작성된 작업을 바꾸거나 신뢰된 호스트가 정책을 바꾸게 하세요:

| 코드 | 거부된 소스 |
|---|---|
| `E-PERMISSION-DIRECTIVE` | 디렉티브 |
| `E-PERMISSION-STATE` | 상태 쓰기 또는 초기화된 기본값 |
| `E-PERMISSION-FACT` | 팩트 쓰기 또는 seed 팩트 |
| `E-PERMISSION-BRIDGE` | bridge 서비스/작업 |
| `E-PERMISSION-REWARD` | 선언적 보상 |
| `E-PERMISSION-QUEST` | 퀘스트 선언 |

중첩되고 도달 불가능한 소스와 호출된 컴포넌트 본문에도 적용됩니다. Context filtering은 나쁜 생성을 줄이지만, 권한을 결정하는 것은 checker와 compiler gate입니다.

## 생성기가 마주칠 진단

이는 모델 기반 루프가 실제로 만나는 진단을 대응 방법별로 묶은 것입니다.

**`E-DOMAIN-UNKNOWN` — 가장 먼저 걸리는 진단입니다.** `0.9.0`부터 compiler는 콘텐츠 어휘의 일곱 *slot*을 선언하지만 **멤버는 제공하지 않으므로**, 프로젝트가 `emotion`을 선언하기 전에는 `@mira{emotion="delighted"}`가 실패합니다. 모델은 프로젝트의 palette를 추측할 방법이 없고, 추측은 바로 잘못된 방법입니다. 멤버는 capability snapshot에 있으므로 **읽고, 절대로 발명하지 마세요.** `lute context --json`은 경로별로 나눕니다. `enums:` 블록(인라인 또는 `uses:`/`extends:`를 통해)이 선언한 멤버는 `projectEnums`, 활성 플러그인이 export하는 멤버는 `enums`입니다:

```console
$ lute context docs/examples/property-tracks.lute --project docs/examples --json
{ …, "enums": {}, "projectEnums": { "action": ["fadeInUp", …, "hide"], "anchor": ["left", "center", "right"],
  "emotion": ["neutral", "surprised", "delighted", "shy", "content", "angry", "sad"], … } }
```

두 키 모두 `capabilitySnapshot`에 접혀 들어가므로 vocabulary가 바뀌면 prompt-cache 키도 이미 바뀝니다. 문서에 필요한 slot의 union이 비어 있으면 수정은 다른 attribute 값이 아니라 *선언*입니다. 세 경로는 [콘텐츠 어휘](/ko/language/vocabulary/)를 참고하세요. 모델이 선언 자체를 작성할 때를 포착하는 후속 코드는 `E-ENUM-MISSING-SEMANTICS`(`action`이 `exits:` 없이 선언되거나 `anchor`가 `default:` 없이 선언됨 — compiler는 더 이상 멤버 이름 접두사로 추론하지 않고 해당 멤버를 기준으로 분기)와 `E-DOMAIN-DUP`(같은 root에서 plugin과 project 경로가 하나의 slot을 함께 선언함 — slot마다 한 경로를 선택)입니다.

**`E-MONO-POV` / `E-MONO-NO-POV` — `mono`는 POV의 것입니다.** `{mono}` 줄(속마음)은 유효한 POV — frontmatter `pov:`, 없으면 프로젝트 `defaults.pov` — 의 화자이거나 유효한 `monoSpeakers`(frontmatter 또는 프로젝트 defaults)에 적힌 화자일 때만 유효합니다. 다른 화자는 `E-MONO-POV`이고 POV 자체가 해석되지 않으면 `E-MONO-NO-POV`입니다. 다른 캐릭터의 생각을 원하는 모델은 `mono`를 쓰지 말고 줄을 `dialogue`, `os`, 또는 `vo`로 작성하거나 작가가 화자를 `monoSpeakers`에 추가하게 해야 합니다. 생성기가 `pov:`나 `monoSpeakers`를 스스로 고르게 하지 마세요. 이는 corpus 소유자의 결정이며 `lute fix`도 추측하지 않습니다. 컴포넌트 줄은 각 `::use` 지점에서 호출자의 POV와 허용 목록에 따라 검사됩니다.

```lute expect="E-MONO-POV"
---
kind: scene
character: mira
season: 1
episode: 1
pov: mira
---

## The Counter

@mira{mono}: He is lying.
@jun{mono}: She knows.
```

<!-- lute-diagnostics -->
```console
$ lute check scene.lute
scene.lute:12:6: error [E-MONO-POV] `@jun{mono}` is an interior monologue, but `jun` is not this document's point of view (`mira`) and not in `monoSpeakers:` (which lists none) — only the POV character and the speakers `monoSpeakers:` lists may speak `mono`; add `jun` to `monoSpeakers:`, or write the line as dialogue, `{os}` or `{vo}`
failed: scene.lute (1 error(s), 0 warning(s))
```

**`E-STATE-COLLECTION`.** 작성자가 선언하는 `state:`는 scalar입니다: `bool | int | double | string | enum`. `list`, `record`, `map` 선언은 거부됩니다. 모델은 `type: { list: string }`을 계속 사용하며, 이것이 0.7.0에서 정상인 문서가 0.8.0에서 새로 실패하는 유일한 사례였습니다. 해결책은 타입을 억지로 바꾸는 것이 아닙니다. [`relations:`](/ko/state/facts-and-datalog/)로 collection을 모델링해야 하며 관계형 계층이 존재하는 이유가 바로 그것입니다. 진단을 그대로 피드백하세요. 진단이 해결책을 명시합니다.

```lute expect="E-STATE-COLLECTION"
---
kind: scene
character: mira
season: 1
episode: 1
state:
  run.inventory: { type: { list: string }, default: [] }
---

## The Counter

@mira: What have you got in there?
```

<!-- lute-diagnostics -->
```console
$ lute check scene.lute
scene.lute:7:3: error [E-STATE-COLLECTION] state path `run.inventory` cannot declare a collection type (`list`/`record`/`map`); author state is (int|double|bool|string|enum) — model collections as `relations:` or a plugin `state_shapes` slot
failed: scene.lute (1 error(s), 0 warning(s))
```

**`W-CODE-AFTER-END`** — 같은 직선 본문(섹션 본문, `<choice>` 본문, `<when>` arm, 목표 본문, `<on>` 본문)의 `::end` 뒤 콘텐츠는 도달할 수 없습니다. 각 본문마다 첫 번째 그런 노드에서 한 번 보고됩니다. terminator를 출력한 뒤 계속 작성하는 생성기가 정확히 이를 만듭니다. 죽은 콘텐츠가 루프를 차단해야 한다면 `--deny W-CODE-AFTER-END`로 승격하세요.

**`E-IDENTITY-TEMPLATE`** — `lute.project.yaml`의 `identity:` 템플릿이 `{prefix}`, `{speaker}`, `{code}` 이외의 token을 사용합니다. 프로젝트 수준 오류이므로 JSON이 아니라 위에서 설명한 `lute:` 채널로 옵니다.

**`W-L10N-MISSING` / `E-LOCALE-BUNDLE`** — localization round trip입니다. `compile --locales` bundle이 채우지 못한 `(lineId, locale)` 쌍마다 경고 하나가 나오며, `loc import`에서 malformed bundle(파싱 불가 입력, 한 locale 안에서 `lineId`가 두 번 등장, 빈 locale tag)이 오류입니다. 번역을 소유한 하네스는 `--deny W-L10N-MISSING`을 실행해 불완전한 bundle이 아티팩트에 도달하지 못하게 해야 합니다.

**플러그인 계열** — 모델이 문서가 아니라 *플러그인*을 작성할 때 발생합니다:

| 코드 | 원인 | 채널 |
|---|---|---|
| `E-PLUGIN-OPTION-UNKNOWN` | 활성화된 option 이름이 plugin manifest에 선언되지 않음 | `lute:` |
| `E-PLUGIN-OPTION-TYPE` | 병합된 option 값이 선언된 타입에 맞지 않음 | `lute:` |
| `E-PLUGIN-RESERVED-STAMP-ATTR` | `at`/`duration`/`delay`/`wait`/`timeline`/`timing`/`provenance`/`source`라는 attr — `stampAttrs`와 per-directive 표면 모두에서 예약된 stamp 키 | `lute:` |
| `E-LOWER-RECORD-UNKNOWN` | core staging kind이 아닌 것을 `lower: { record }`가 이름 붙임 | `lute:` |
| `E-LOWER-RECORD-FIELD` | record kind에 없는 field, 디렉티브가 선언하지 않은 `fromAttr`, 또는 field를 채울 수 없는 literal을 `lower: { fields }`가 이름 붙임 | `lute:` |
| `E-FRONTMATTER-SCHEMA` | plugin 소유 frontmatter 키의 값이 선언된 schema를 위반 | JSON, span 포함 |

`E-FRONTMATTER-SCHEMA`만 문서 위치가 있으므로 JSON 진단입니다. 나머지는 프로젝트 수준이어서 빈 JSON 본문과 함께 stderr로 도착합니다. 이 페이지의 모든 코드는 유효한 `--deny` 인자입니다. 알 수 없는 코드는 사용법 오류(종료 `2`)이므로 이 플래그가 아무것도 보호하지 않은 채 조용히 통과할 수 없습니다.
## 수정 사항

진단에는 `kind`가 있는 fixit이 포함됩니다. `kind: "migrate"`는 기계가 적용할 수 있으므로 질문하지 않고 적용하세요(이것이 `lute fix`가 하는 일입니다). `kind: "refactor"`는 작가의 선택이므로 LSP code action으로 제시하고 절대로 자동 적용하지 마세요.

## 검증의 경계

`check`는 sound하지만 관계형 계층에 대해서는 의도적으로 불완전합니다. 따라서 어느 영역이 증명으로 덮이고 어느 영역이 검토로 덮이는지 알아야 합니다:

- **Scalar gate는 증명으로 덮입니다** — 도달 가능성과 Guaranteed/Possible envelope(§5)가 정적으로 결정합니다.
- **관계형 팩트 gate는 극단값에서, `check-project`에서만 증명으로 덮입니다** — 가드 안 모든 `holds(…)`/`count(…)`를 프로젝트 전체에서 불가능, 보장, 가능으로 판정합니다(dsl 0.20.0). 절대로 성립하지 않는 gate는 해당 슬롯의 dead-code 오류(`E-ARM-DEAD`, `E-ENTRY-UNREACHABLE`, `E-OBJECTIVE-UNSATISFIABLE`, `E-QUEST-UNREACHABLE`)입니다. 항상 성립하는 가드는 `W-FACT-GUARANTEED`입니다. *가능*한 gate — 어떤 경로에는 팩트가 있고 다른 경로에는 없는 경우 — 는 조용히 지나가며 검토 대상으로 남습니다. 단일 파일 `check`는 모든 관계형 질의를 미결정 상태로 두므로 생성된 팩트 gate를 신뢰하기 전에 `check-project`를 실행하세요. (모든 관계형 gate를 표시하던 `W-UNPROVEN-RELATIONAL`은 0.20.0에서 제거되었습니다. `--deny`에서 이름을 지정하면 사용법 오류입니다.)
- `W-TRACE-MOCK-UNPRODUCIBLE`은 `lute trace` mock이 작성된 producer가 절대로 assert할 수 없는 관계에 팩트를 seed할 때 경고합니다. 이 walk는 도달 가능한 플레이에 대해 아무것도 증명하지 않습니다.
- `W-LUTE-VERSION-STALE`은 모델이 오래된 예시에서 복사한 낡은 `luteVersion` stamp를 감지합니다.

## `lute tag`로 마무리

파이프라인 끝에서 `lute tag scene.lute`를 실행해 태그가 없는 모든 줄에 안정적인 `code`를 채우세요. 모델이 직접 `code=` 값을 쓰게 하지 마세요. 줄의 identity는 모델이 아니라 도구의 일입니다.

## Inspect → Plan → Patch → Check → Replay → Review (0.36.6)

```console
$ lute tag /tmp/drowned-crown
$ lute context /tmp/drowned-crown --target quest:libraryKey --max-items 20 --json > context.json
$ lute patch /tmp/drowned-crown planned-patch.json --dry-run --json
$ lute check-project /tmp/drowned-crown
$ lute diff /tmp/drowned-crown-before /tmp/drowned-crown --json
```

줄에 ID가 없으면 편집 전에 `lute tag`를 실행해야 합니다. 생성된 ID는 위치 기반이며 발명하거나 다시 번호를 매기면 안 됩니다. Context의 `projectRevision`과 파일 revision을 patch base에 복사하세요. `diff.changes`를 검토한 뒤 `--dry-run` 없이 같은 patch를 적용하고, 프로젝트 자체의 test, play 또는 trace 스크립트로 replay하세요. `context --at <file>:<line>:<column>`은 `--max-items N`과 `--run <FILE>`을 받을 수 있습니다(후자는 script file이 필요함). Lute는 결정적인 CLI 경계이지 chat 애플리케이션이나 MCP server가 아닙니다.

## patch 요청 (0.47)

`lute patch <DIR> <PATCH> [--dry-run] [--json]`은 JSON 객체 하나인 `PATCH`를 읽습니다. `lute schema patch`가 이를 `inputs` 아래에 설명합니다(`argument: "PATCH"`, `kind: "request"`, `format: "json"`, 그리고 `fields`, `ops`, `preserve`, `nodeKinds`). 이 설명은 요청 파서와 같은 표에서 읽으므로, 하네스는 이를 보고 요청을 만들 수 있습니다. 알 수 없는 필드는 거부됩니다(`E-PATCH-EDIT`).

| 필드 | 필수 | 타입 | 의미 |
|---|---|---|---|
| `base` | 예 | 객체 | `project`(필수): 프로젝트 revision `sha256:…`이며 `lute context --json`의 `projectRevision`에서 복사합니다. `files`(선택): 프로젝트 기준 상대 경로에서 그 파일의 revision `sha256:…`으로 가는 맵입니다(context의 `files`). 다르면 patch가 거부됩니다(`E-PATCH-STALE`). |
| `targets` | 아니오 | 노드 키 | 편집이 건드릴 수 있는 노드입니다. 편집이 가리키는 모든 노드나 span은 target 안에 있어야 합니다. |
| `edits` | 아니오 | 편집 | 모두 함께 적용되거나 하나도 적용되지 않습니다. |
| `preserve` | 아니오 | preserve 항목 | patch가 지켜야 할 의미 주장입니다(깨지면 `E-PATCH-PRESERVE`). |

각 편집은 `op`와 그 연산의 필드를 담은 객체이며, 필드는 모두 필수입니다.

| `op` | 필드 | 효과 |
|---|---|---|
| `replaceNode` | `node`, `text` | 노드의 소스 span을 `text`로 바꿉니다. |
| `insertBefore` | `node`, `text` | 노드 span의 시작에 `text`를 넣습니다. |
| `insertAfter` | `node`, `text` | 노드 span의 끝에 `text`를 넣습니다. |
| `replaceAttr` | `node`, `attr`, `value` | 노드 안 속성 하나의 값을 바꿉니다(따옴표는 유지). |
| `removeNode` | `node` | 노드의 소스 span을 지웁니다. |
| `replaceText` | `file`, `rev`, `span`, `text` | revision이 `rev`인 `file`의 `span`을 정확히 바꿉니다. `span`은 `lute context --json`의 `affected[].reasons[].sourceSpan`과 같은 모양의 소스 span 객체(`byte_start`, `byte_end`, `line`, `column`, `utf16_range`)이며 target 안에 있어야 합니다. |
| `createFile` | `path`, `text` | 새 파일을 만듭니다(파일 전체를 포맷). |
| `moveFile` | `from`, `to` | 파일을 옮깁니다. 다른 파일이 옛 경로를 아직 참조하면 거부됩니다. |

경로는 프로젝트 기준 상대 경로이며 프로젝트를 벗어날 수 없습니다. 각 `preserve` 항목은 키가 정확히 하나인 객체입니다.

| 키 | 값 | 주장 |
|---|---|---|
| `ids` | 노드 키 | 노드가 identity를 유지합니다. |
| `lineIds` | 문자열(읽지 않음) | 어떤 line id도 바뀌지 않습니다. |
| `voiceKeys` | 문자열(읽지 않음) | 어떤 voice key도 바뀌지 않습니다. |
| `choiceEffects` | 노드 키 | 선택지의 효과가 그대로입니다. |
| `rewards` | 노드 키 | 보상이 그대로입니다. |
| `hostContracts` | `true` | 호스트 계약 표면이 그대로입니다. |
| `conditions` | 노드 키 | 노드의 조건이 그대로입니다. |
| `reachability` | 노드 키 | 노드의 도달 가능성이 그대로입니다. |
| `constraints` | `true` | 프로젝트 제약 판정이 그대로입니다. |

노드 키는 `kind:key`입니다. 키는 `lute context <file> --target <kind:key> --json`에서 가져오세요. `affected[]`의 모든 항목이 정규 `key`(와 `reasons[].sourceSpan`)를 담고, target 자신은 `target.kind`:`target.key`입니다(`target.key`만으로는 kind 없는 키입니다). 종류는 `project`, `document`, `scene`, `beat`, `shot`, `line`, `choice`, `quest`, `objective`, `reward`, `entry`, `occasion`, `relation`, `state`, `def`, `component`, `expanded`, `fact`, `clock`, `engine`입니다. patch는 편집 가능한 소스 span이 있는 노드만 target으로 삼을 수 있습니다. `project`, `occasion`, `relation`, `state`, `expanded`, `fact`, `engine`은 target으로 거부됩니다(`E-PATCH-TARGET`. 스키마 YAML은 target으로 삼을 수 있는 노드 안에서 `replaceText`로 편집하세요). `lute schema patch`의 `nodeKinds[].targetable`도 같은 내용을 알려 줍니다.

```json
{
  "base": { "project": "sha256:dc7f8a0c…" },
  "targets": ["line:fair.narrator_0020", "line:fair.pell_0010"],
  "edits": [
    { "op": "replaceNode", "node": "line:fair.narrator_0020",
      "text": "@narrator{code=\"0020\"}: Someone runs past, shouting." },
    { "op": "replaceNode", "node": "line:fair.pell_0010",
      "text": "@pell{code=\"0010\"}: Paper, a frame, and a pencil." }
  ],
  "preserve": [{ "lineIds": [] }, { "reachability": ["line:fair.pell_0010"] }]
}
```

한 파일의 모든 편집은 원래 텍스트를 기준으로 해석되므로, 한 파일의 여러 편집은 서로의 길이 변화에 영향을 받지 않습니다. patch는 자신이 건드린 줄만 포맷합니다. `accepted`를 보고하기 전에 영역 포맷을 거친 모든 `.lute` 파일은 다음을 만족해야 합니다. `lute fmt --check`를 통과하던 파일은 계속 통과하고, 그 밖의 파일에서는 건드린 줄을 다시 포맷해도 아무것도 바뀌지 않습니다. 그렇지 않으면 patch는 파일 이름을 담은 `E-PATCH-FORMAT`으로 거부되고 아무것도 쓰지 않습니다. 거부(`E-PATCH-STALE`, `E-PATCH-TARGET`, `E-PATCH-EDIT`, `E-PATCH-FORMAT`, `E-PATCH-CHECK`, `E-PATCH-PRESERVE`)는 `accepted: false`와 함께 종료 2이고, 승인된 patch는 의미가 바뀌면 종료 1, 바뀌지 않으면 종료 0입니다.
