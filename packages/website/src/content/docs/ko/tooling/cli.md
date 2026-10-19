---
title: CLI 레퍼런스
description: 모든 lute 하위 명령 — init, new, check, compile, expr, run, play, trace, test, lint, scenario, beats, calendar, refs, loc, context, tag, fix, lore, doctor, catalog refresh, version — 의 개요, 주요 플래그, 종료 코드 계약입니다.
---

`lute`는 `.lute` 문서를 헤드리스로 검사하고 컴파일하는 도구입니다. 핵심 `check()`가 계약이며, CLI는 인자 파싱, 파일 I/O, 출력 형식만 추가하고 검증 로직은 소유하지 않습니다. 프로젝트를 쓰는 모든 명령은 프로젝트를 `--project <DIR>`로만 받으며, 생략하면 가장 가까운 `lute.project.yaml`을 찾아 씁니다([프로젝트와 탐색](#프로젝트와-탐색)). 반복해서 쓰이는 해석 플래그는 두 가지입니다. `--providers <DIR>`는 id를 해석할 provider 스냅숏 디렉터리를 고정하고, `--project <DIR>`는 문서의 활성화된 capability 스냅숏을 해석할 `lute.project.yaml` + `plugins/`를 불러옵니다. 출력은 기본이 텍스트이고 JSON은 명시적인 `--json`일 때만 나옵니다([기계 판독 출력](#기계-판독-출력)). 0.27.0 이전에는 `check`와 `test`만 매니페스트를 찾았기 때문에, `check`가 통과시킨 문서를 `trace`가 `E-UNDECLARED`로 거부할 수 있었습니다. 아래의 권한 인식 작성 명령에서 `--permission-profile <NAME>`은 프로젝트 해석을 요구하며, 해당 신뢰 프로필의 [권한](/ko/tooling/capability-permissions/)을 그 플러그인을 활성화하거나 원본 프로필을 바꾸지 않고 추가 상한으로 적용합니다.

## 공통 규칙

### 프로젝트와 탐색

- 프로젝트를 쓰는 모든 명령은 프로젝트를 `--project <DIR>`로만 받습니다. 프로젝트를 피연산자로 받거나 다른 이름으로 받는 명령은 없습니다.
- **탐색.** `--project`가 없으면 프로젝트는 `lute.project.yaml`이 있는 가장 가까운 상위 디렉터리입니다. `check`, `compile`, `context`, `trace`는 각 파일 피연산자의 디렉터리에서(파일마다) 시작하고, 그 밖에는 현재 디렉터리에서 시작합니다. 발견된 디렉터리는 `--project`로 직접 준 것과 똑같이 쓰입니다(특히 `compile <FILE>`과 `trace <FILE>`은 조정된 프로젝트 판정으로 게이트하고, `trace`는 프로젝트의 단일 루트 producer 집합을 씁니다). 파일 명령이 출력하던 stderr 메모 `lute: note: using project <DIR> (nearest lute.project.yaml); pass --project to choose another`는 프로젝트마다 한 번 그대로 나옵니다.
- **프로젝트 디렉터리는 순회 루트입니다.** 0.47.2가 디렉터리 피연산자를 받던 모든 명령에서 `--project <DIR>`(명시 또는 발견)은 그 피연산자와 똑같은 뜻입니다. 순회 루트이며, 각 문서(또는 스크립트)는 `<DIR>` 이하에서 가장 가까운 프로젝트에 대해 해석됩니다. 중첩된 프로젝트는 자기 문서를 유지하고, `lute.project.yaml`이 없는 디렉터리는 문서가 각자의 프로젝트에 대해 해석되는 작업 공간이며 그렇지 않으면 core만 해석합니다. 탐색 자체는 매니페스트가 있는 디렉터리만 찾습니다.
- **찾지 못한 경우.** `--project`가 없고 상위에 매니페스트도 없을 때 프로젝트가 필요한 명령(전체 프로젝트 `check`, 문서 없는 `compile`, `impact`, `constraints`, `patch`, `play`, `test`, `loc export`, `loc report`, `scenario`와 그 하위 명령, `beats`, `calendar`, `lore`, `refs`, `expr`)은 `E-CLI-USAGE`(`argument: "--project"`)로 실패하며 메시지는 `lute <command>: no lute.project.yaml at or above <dir> — pass the project directory with --project <DIR>`입니다. `check <FILE>…`, `compile <FILE>`, `context`, `trace`, `lint`, `fmt`, `fix`, `tag`, `new`, `doctor`는 프로젝트 없이도 0.47.2처럼 동작합니다.
- `init [<DIR>]`은 예외입니다. 피연산자는 만들 디렉터리입니다(기본값 `.`).

### 공통 옵션 그룹

한 개념의 옵션은 한 번만 선언되어 각 명령에 그대로 붙으므로 도움말 문구와 값 이름이 하나입니다.

| 그룹 | 옵션 | 명령 |
|---|---|---|
| 프로젝트 | `--project <DIR>` | 프로젝트를 쓰는 모든 명령 |
| Providers | `--providers <DIR>` | `check`, `compile`, `context`, `trace`, `expr`, `lint`, `impact`, `constraints`, `patch`, `play`, `test`, `scenario`(전역), `beats`, `calendar`, 그리고 `catalog refresh`(다시 스탬프할 디렉터리) |
| 권한 프로필 | `--permission-profile <NAME>` | `check`, `compile`, `context`, `trace`, `expr`, `patch`, `play`, `test`, `calendar` |
| 진단 게이트 | `--deny <CODE>`(반복 가능), `--deny-warnings` | `check`, `compile`(`--stream`과는 함께 쓸 수 없음), `lint`, `trace`, `expr`, `play`, `test`, `calendar` |
| JSON | `--json` | 결과를 보고하는 모든 명령. 단 `schema`(항상 JSON)와 산출물을 쓰는 `loc export`는 제외 |

명령은 결과를 바꿀 수 있는 `--providers`/`--permission-profile`만 받으며, 이 옵션들은 명령이 만드는 프로젝트 모델에 그대로 전달됩니다. `--deny`/`--deny-warnings`는 명령이 종료 코드를 결정하는 판정의 진단을 승격합니다. `trace`는 check 게이트(거부, 종료 **1**), `expr`은 lowering 진단, `play`와 `calendar`는 프로젝트 compile 게이트, `test`는 테스트가 참조하는 모든 check 판정(추적한 문서마다의 check와 플레이마다의 compile 게이트)에 적용하므로, 승격된 경고는 그 테스트를 실패시킵니다.

### 출력

텍스트가 모든 명령의 기본이며 JSON은 `--json`일 때만 나옵니다. 출력은 스트림이 터미널인지, 환경 변수가 무엇인지에 따라 달라지지 않습니다. 자세한 내용은 [기계 판독 출력](#기계-판독-출력)을 참고하세요.


## 기계 판독 출력

모든 명령에는 **id**가 있습니다. 입력한 그대로의 하위 명령 경로입니다(`check`, `scenario reach`, `loc report`). 플래그는 id를 바꾸지 않습니다. 같은 id가 보고서, 오류 envelope, `lute schema`, telemetry에 쓰입니다.

텍스트가 기본이며 JSON은 명시적인 `--json`(또는 항상 JSON인 `schema`)일 때만 나옵니다. 터미널 여부나 환경 변수로 모드가 바뀌지 않고 `--format json`은 없습니다. `--json`이면 명령은 stdout에 JSON 문서 하나(보고서, 또는 사용법/I/O/입력 실패 시 오류 envelope)만 출력하며, 0.47.2가 stdout에 출력하던 안내 메시지는 JSON 모드에서 stderr로 갑니다.

**보고서.** JSON 모드로 출력되는 보고서는 JSON 객체 하나이며, 보고서 자체 필드 옆에 머리 필드 세 개가 있습니다. `command`(id), `schemaVersion`(payload 형태 버전), `ok`(프로세스가 **0**으로 끝날 때만 `true`)입니다. 키는 정렬된 순서로 출력되므로 이름으로 읽으세요.

| 명령 | JSON 플래그 | `schemaVersion` |
|---|---|---|
| `check` | `--json` | `0.48.0.check` |
| `impact` | `--json` | `0.43.0.impact` |
| `constraints` | `--json` | `0.43.0.constraints` |
| `lint` | `--json` | `0.43.0.lint` |
| `compile` | `--json` | `0.43.0.compile` |
| `compile` | `--stream` | (NDJSON 스트림, 보고서 아님) |
| `expr` | `--json` | `0.43.0.expr` |
| `context` | `--json` | `0.43.0.surface` |
| `context` | `--json --node` | `0.36.0.context` |
| `context` | `--json --at` | `0.36.0.position` |
| `diff` | `--json` | `0.36.0.diff` |
| `patch` | `--json` | `0.43.0.patch` |
| `trace` | `--json` | `0.43.0.trace` |
| `play` | `--json` | `0.43.0.play` |
| `test` | `--json` | `0.43.0.test` |
| `scenario` | `--json` | `0.43.0.scenario` |
| `scenario reach` | `--json` | `0.43.0.scenario-reach` |
| `scenario envelope` | `--json` | `0.43.0.scenario-envelope` |
| `scenario knowledge` | `--json` | `0.43.0.scenario-knowledge` |
| `beats` | `--json` | `0.43.0.beats` |
| `calendar` | `--json` | `0.43.0.calendar` |
| `refs` | `--json` | `0.43.0.refs` |
| `lore` | `--json` | `0.43.0.lore` |
| `loc import` | `--json` | `0.48.0.loc-import` |
| `loc report` | `--json` | `0.47.0.loc-report` |
| `doctor` | `--json` | `0.43.0.doctor` |
| `tag` | `--json` | `0.48.0.tag` |
| `fix` | `--json` | `0.48.0.fix` |
| `fmt` | `--json` | `0.48.0.fmt` |
| `catalog refresh` | `--json` | `0.48.0.catalog-refresh` |
| `init` | `--json` | `0.48.0.init` |
| `new scene`, `new quest`, `new lore`, `new schema` | `--json` | `0.48.0.new` |
| `schema` | 항상 | `0.47.0.schema` |

배열이던 보고서 세 개는 이제 객체입니다. `constraints`는 행을 `constraints` 필드에, `trace`와 `compile`의 거부는 진단을 `diagnostics` 필드에 담습니다. `patch`는 패치가 받아들여졌는지를 `accepted`로 알립니다(`ok`는 종료 코드 규칙을 따릅니다. [`lute patch`](#lute-patch-0366) 참고).

**보고서가 아닌 것.** 산출물과 엔진 계약은 자기 형태를 유지합니다. `compile`이 쓰는 실행 IR(그리고 프로젝트 `compile`의 출력 디렉터리), `compile --stream`의 NDJSON, `run --json`, `play --events`, `loc export` 산출물(JSON이 기본이며 플래그가 없고 CSV는 `--format csv`), `version --json`입니다.

**오류.** `--` 앞 인자에 `--json`이 있거나 명령이 `schema`이면 JSON 모드입니다(`--format json`은 더 이상 없습니다). JSON 모드에서 명령줄이 잘못되었거나, 경로를 읽거나 쓸 수 없거나, 입력 파일이 맞지 않아 실패하면 stdout에 오류 envelope를 출력하고 여전히 **2**로 끝납니다. 사람이 읽는 메시지는 stderr에 그대로 남습니다.

```json
{
  "command": "check",
  "error": {
    "code": "E-CLI-IO",
    "message": "lute: cannot read missing.lute: no such file or directory",
    "path": "missing.lute"
  },
  "ok": false,
  "schemaVersion": "0.43.0.error"
}
```

`error.code`는 `E-CLI-USAGE`(잘못된 명령줄. `error.argument`가 해당 인자를, `error.hint`가 파서의 제안을 담습니다), `E-CLI-IO`(경로를 읽거나 쓸 수 없음. `error.path`), `E-CLI-INPUT`(읽은 파일이 깨졌거나 종류가 다르거나 지원하지 않는 버전. 한 파일의 문제면 `error.path`) 중 하나입니다. `lute --explain E-CLI-USAGE`로 각각 설명을 볼 수 있습니다. `compile --stream`과 `play --events`는 스트림 프로토콜을 유지하며 envelope를 출력하지 않고, `patch`는 자체 거부 문서를 유지합니다.

## schema

```console
$ lute schema [<COMMAND>]…
```

명령줄 자체를 `schema` 보고서로 설명합니다(항상 JSON, 파일을 읽지 않음). 경로가 없으면 최상위 명령(`id`, `summary`, `hasSubcommands`)을 나열하고, 경로가 있으면(`lute schema scenario reach`) 그 명령을 설명합니다. `arguments`와 `options`(`name`, `required`, `repeatable`, `takesValue`, 해당할 때 `short`, `valueName`, `possibleValues`, `default`, `help`), `subcommands`, `outputs` — 각 기계 판독용 출력을 고르는 `flag`, 종류 `kind`(`report`, `artifact`, `stream`), 보고서의 `schemaVersion` — 그리고 `inputs`(0.47)입니다. `inputs`는 명령이 읽는 구조화된 입력마다 한 항목이며, `patch`를 뺀 모든 명령에서 빈 목록입니다. `patch`의 `PATCH` 요청은 `{ argument, kind: "request", format: "json", fields, ops, preserve, nodeKinds }`로 설명됩니다([patch 요청](/ko/tooling/ai-harness/#patch-요청-047) 참고). 파서 자신의 정의에서 나오므로 `--help`나 patch 요청 파서와 어긋날 수 없습니다. 알 수 없는 경로는 `E-CLI-USAGE`이며, 종료 코드는 **0** 또는 **2**입니다.

## Telemetry

`lute`를 실행할 때마다 Lute 상태 디렉터리의 `telemetry.jsonl`에 한 줄을 추가합니다. `XDG_STATE_HOME`이 절대 경로면 `$XDG_STATE_HOME/lute`, 아니면 `$HOME/.local/state/lute`, Windows에서는 `%LOCALAPPDATA%\lute`입니다.

```json
{"v":1,"ts":1760400000000,"lute":"0.47.2","command":"check","json":true,"exit":0,"durationMs":12}
```

이것이 기록의 전부입니다. 명령 id(또는 `null`), JSON 모드 여부, 종료 코드, 걸린 시간, 툴체인 버전입니다. 인자나 플래그 값, 경로, 파일 내용, 출력, 환경은 기록하지 않고 어디로도 보내지 않습니다. 파일은 내 컴퓨터에만 남습니다. 1 MiB가 되면 `telemetry.jsonl.1`로 돌리며, 기록은 best-effort라 명령의 출력이나 종료 코드를 바꾸지 않습니다. `LUTE_TELEMETRY=0`(또는 `false`, `off`, `no`)이나 `DO_NOT_TRACK=1`로 끕니다. `lute doctor`가 상태를 보여 줍니다(`• Telemetry: off (LUTE_TELEMETRY)`, 켜져 있으면 파일 경로). 저장소의 테스트, 스크립트, CI는 꺼진 상태로 실행합니다.

## check

```console
$ lute check [<FILE>]… [--project <DIR>] [--providers <DIR>] [--permission-profile <NAME>]
              [--engine <FILE>] [--wip] [--deny <CODE>]… [--deny-warnings] [--json]
```

`check`는 두 범위를 하나의 명령으로 다룹니다. 피연산자로 문서를 주면 그 문서만, 피연산자가 없으면 프로젝트 전체를 검사합니다.

**문서 모드.** `<FILE>`이 하나 이상이면 각 문서를 단일 문서 분석으로 정적으로 검증합니다(`.yaml`/`.yml` 스키마 피연산자도 그대로). 각 문서는 자신의 프로젝트에서 해석됩니다. `--project`가 없으면 그 파일의 디렉터리에서 위로 올라가며 가장 가까운 `lute.project.yaml`이 프로젝트(`defaults:`, 프로필, 플러그인)를 제공하고(파일마다), stderr에 `lute: note: using project <dir> (nearest lute.project.yaml); pass --project to choose another`를 알립니다. 위쪽에 매니페스트가 없으면 core만 검사합니다. **관계형 가드 판정**(관계에 대한 데드 가드, `W-WIP`)은 프로젝트 전체가 필요하므로 [전체 프로젝트 모드](#프로젝트-전체-모드)에서만 나옵니다. 디렉터리 피연산자는 `E-CLI-USAGE`이며 `--project`를 안내합니다(`lute check: <DIR> is a directory — check the whole project with lute check --project <DIR>`). 읽을 수 없는 피연산자는 보고서 없이 종료 **2**와 `E-CLI-IO` envelope로 끝나며, 그 앞의 피연산자는 이미 텍스트 줄을 출력했을 수 있습니다. 문서 피연산자와 함께 쓴 `--wip`는 `E-CLI-USAGE`(`argument: "--wip"`)입니다. `--engine <FILE>`은 불변 `lute.engine.yaml` 매트릭스를 불러오고 exact-minor IR 게이트 뒤에 작성 시점의 의미 협상을 수행합니다. 지원되지 않는 id가 필요한 각 lower된 구성은 소스 범위, 의미 id, 엔진 이름과 함께 `E-CHECK-ENGINE-SEMANTICS`로 보고됩니다. 이는 플레이백이 아닙니다. 종료 코드는 두 모드 모두 정상일 때 **0**, `Error` 심각도 진단이 하나라도 있을 때(또는 `--deny` 승격) **1**, 사용법/I/O 또는 매트릭스/버전 실패일 때 **2**입니다. `--permission-profile <NAME>`은 신뢰된 추가 상한을 적용합니다. 거부된 작성 효과는 억제할 수 없는 `E-PERMISSION-*` 오류이며, 프로젝트/프로필이 없거나 정책이 잘못되면 제한 없는 대체 동작이 아니라 명시적인 해석 오류가 됩니다. `--json`은 [`0.48.0.check` 보고서](#check-보고서)를 출력하고, 그렇지 않으면 진단마다 사람이 읽는 줄 하나를 출력합니다. `--deny <CODE>`(반복 가능, rustc/clippy `-D` 선례, 0.6.1 §5)는 정확히 그 코드를 가진 모든 진단을 판정과 종료 코드에 대해 오류로 승격하고, `--deny-warnings`는 모든 경고를 승격합니다 — 파이프라인은 중복된 관계형 가드를 거부하려고 `W-FACT-GUARANTEED`(전체 프로젝트 모드에서)를, 오래된 `luteVersion` 스탬프를 거부하려고 `W-LUTE-VERSION-STALE`을 deny합니다. 승격된 진단은 `--json`에서 `"denied": true` 표시와 함께 심각도 `error`로 보고됩니다. `--deny`의 알 수 없는 코드는 — `W-UNPROVEN-RELATIONAL`(0.20.0)처럼 제거된 코드를 포함해 — 사용법 오류(종료 **2**)이며, 오류는 절대 강등되지 않습니다.

<a id="check-보고서"></a>
**보고서 `0.48.0.check`**(두 모드 공통). 스키마는 `schemas/lute-check-report-0.48.schema.json`입니다.

```json
{
  "command": "check",
  "schemaVersion": "0.48.0.check",
  "ok": true,
  "files": [
    {"path": "scenes/a.lute", "ok": true, "diagnostics": [], "resolved": {"commands_preview": [], "injections": [], "timeline_tables": []}}
  ],
  "project_diagnostics": []
}
```

`files[]`에는 검사한 문서마다 항목이 하나씩, 검사한 순서대로 들어갑니다. 각 항목의 `path`는 준 그대로(또는 순회한 경로), `ok`는 `--deny` 이후의 값, `diagnostics`와 `resolved`는 `CheckResult`의 것이며 스키마 피연산자에서 `resolved`는 `null`입니다. `project_diagnostics`(각 항목은 진단에 `path`를 더한 것)는 전체 프로젝트 모드를 실행했을 때만 있습니다. `ok`는 0.43 §3.2를 따릅니다. 종료 **1**로 끝나는 실행도 JSON 모드에서 이 보고서를 출력합니다. 전체 프로젝트 모드에서 매니페스트가 잘못되면 `files: []`와 `path`가 매니페스트인 `project_diagnostics`를 담고, 문서의 플러그인/프로필 해석 오류는 그 문서 항목에 `ok: false`와 해석 진단을 담습니다. 한 문서만 검사했다면 `files[0]`을 읽으세요.

### 프로젝트 전체 모드

```console
$ lute check [--project <DIR>] [--providers <DIR>] [--permission-profile <NAME>]
              [--engine <FILE>] [--wip] [--deny <CODE>]… [--deny-warnings] [--json]
```

문서 피연산자 없이 실행하면(`--project <DIR>` 또는 발견된 프로젝트) 프로젝트 디렉터리 아래의 모든 `*.lute` 파일을 결정적인 정렬 순서로 재귀적으로 `check`합니다(0.29.0: 보고서는 원인을 먼저 출력합니다 — `lute.project.yaml` 행, 플러그인, 스키마, 경로 순서의 각 문서, 프로젝트 전체 행 순서). 각 파일을 자신의 가장 가까운 상위 `lute.project.yaml` 루트에 대해 검사하고, **프로젝트 전체 `<quest id>` 고유성**, 연결성 패스(`E-CONN-*`, `W-QUEST-REF-UNKNOWN`, `E-STATE-MAYBE-UNAVAILABLE`), [관계형 가드 분석](/ko/state/facts-and-datalog/#프로젝트-전체-lute-check가-관계형-가드를-분석하는-방법)(dsl 0.20.0)을 수행합니다. 가드의 모든 `holds(…)`/`count(…)`는 프로젝트 전체에서 불가능, 보장, 가능 중 하나로 판정되며, 절대 성립할 수 없는 가드는 슬롯의 데드 코드 오류(`E-ARM-DEAD`, `E-ENTRY-UNREACHABLE`, `E-OBJECTIVE-UNSATISFIABLE`, `E-QUEST-UNREACHABLE`)로, 중복 가드는 `W-FACT-GUARANTEED`로 보고됩니다. 프로젝트 전체 비트/퀘스트 권고도 실행합니다(dsl 0.22.0): `W-BEAT-PRIORITY-TIE`(같은 `select: first` 계기에서 같은 대상 또는 대상 없음이고 priority가 같으며 `when`들이 서로 배타적이라고 증명되지 않아 파일 순서가 승자를 정함), `W-BEAT-ONCE-RUN-USER`(`when`이 user 등급 상태만 읽으므로 매 런마다 반복되는 `once: run` 비트), `W-QUEST-HANDLER-DEAD`(`fail`도, 필수 하위 퀘스트 목표도, 부모 퀘스트도 없는 퀘스트의 `<on event="questFailed">`라서 절대 실패할 수 없음)입니다. 이어서 `identity:`에 따라 정상 검사된 모든 문서를 컴파일하므로 컴파일 단계 오류가 `compile` 때만이 아니라 여기서도 실패합니다: `E-DUP-VOICEKEY`(한 `voiceKey`에 서로 다른 텍스트의 줄 — 0.22.0부터 기본 `{prefix}.{speaker}-{code}`가 씬들을 분리하므로, 이전의 접두사 없는 `identity.voiceKey: "{speaker}-{code}"`를 고정하는 프로젝트가 겪는 위험) 및 `E-CAPABILITY-MISMATCH`(서로 다른 두 capability 스냅숏을 해석하는 문서로, 프로젝트 `compile`과 `play`가 거부함)입니다. `--engine <FILE>`은 프로젝트의 정렬된 `requiredSemantics` 합집합도 협상하고 기여한 각 문서/범위를 `E-CHECK-ENGINE-SEMANTICS`로 보고하며, playback을 열지 않습니다. 종료 코드는 정상일 때 **0**, 파일에 오류나 프로젝트 전체 충돌이 하나라도 있을 때 **1**, I/O 또는 매트릭스/버전 실패일 때 **2**입니다. 동일한 `--deny <CODE>`/`--deny-warnings` 승격도 프로젝트 전체에 적용됩니다. 중첩된 프로젝트는 각자의 매니페스트별로 묶입니다. 프로젝트를 찾지 못하면 `E-CLI-USAGE`입니다.

 dsl 0.24.0부터 전체 프로젝트 `check`는 `W-QUEST-NEVER-ACCEPTED`(accept 중심 퀘스트 — `start`가 없고 root 또는 `activate="accept"` 자식이지만 프로젝트의 어떤 `::accept`도 이름 붙이지 않음), `W-RELATION-UNREAD`(assert/seed/derive되었지만 어떤 조건, rule body, def도 읽지 않는 relation), `W-DEF-UNUSED`(아무것도 참조하지 않는 `@def`. `host: true`로 선언한 스키마·플러그인 def는 호스트가 읽으므로 제외)를 선언 위치에서 프로젝트마다 한 번씩 경고합니다. `mocks/*.yaml` 패스는 mock의 `bridges:` 응답을 mock이 지정한 문서의 플러그인 호출과 대조합니다(`E-TRACE-MOCK-UNDECLARED` / `E-TRACE-MOCK-TYPE`).
 
0.40부터 프로젝트에 `player.bindings.yaml`이 있으면 이 명령은 컴파일 후 선택적인
플레이어 바인딩도 검사합니다. 바인딩 스키마, `assetsBase` 아래의 asset 파일, domain과
cast 키, flow 계기와 target, 효과 이름과 매개변수를 확인합니다.
`E-BINDING-SHAPE`, `E-BINDING-FILE`, `E-BINDING-KEY`, `E-BINDING-EFFECT`가
바인딩 오류이며, 없는 항목과 사용하지 않은 항목은 각각 `W-BINDING-MISSING`,
`W-BINDING-UNUSED`입니다. JSON 출력에는 `player.bindings.yaml`에 연결된 진단이
포함됩니다.

이후 dsl 0.25.0부터 `accepts:` mock/test는 더 이상 `W-QUEST-NEVER-ACCEPTED`를 없애지 않으며(메시지에 이름이 표시됨), `<quest accept="external">`은 이를 없앱니다. 부모와 함께 활성화되는 자식의 `accept="external"`은 `E-ACCEPT-TARGET`입니다. [배타적 relation](/ko/state/facts-and-datalog/#배타적-관계-excludes)은 둘 다 필요한 가드를 dead로, 부정을 보장으로(`W-FACT-GUARANTEED`) 만들고, 다른 쪽이 모든 경로에서 성립할 때 한쪽을 assert하면 `E-FACT-EXCLUSIVE`, 배타성을 깨뜨릴 수밖에 없는 rule은 `E-RULE-EXCLUSIVE`입니다. 하나의 [`share`](/ko/language/beats/#한-이벤트-여러-장소-share) 키에 서로 다른 `once`를 쓰는 beat는 `E-BEAT-ATTR`이고, bundle beat의 `after=`는 알려진 노드를 지정해야 합니다(`E-CONN-UNKNOWN-NODE`). reserved relation의 [`changedOn:`](/ko/language/dialogue-and-cast/#엔진-이벤트-이후의-presence-changedon)은 cast presence를 scenario graph보다 먼저 정렬하므로 해당 계기 뒤의 줄에는 `assume: true`가 더 이상 적용되지 않습니다(`W-CAST-ABSENT`). 매니페스트 `defaults:`에서 모든 문서가 상속하는 오래된 `luteVersion`은 `lute.project.yaml` 해당 줄에서 프로젝트마다 한 번 `W-LUTE-VERSION-STALE`로 보고되고 `— every document inherits it (N documents)`로 끝납니다. 문서 모드 `lute check <file>`은 여전히 문서에서 보고합니다.

이후 dsl 0.26.0부터 여러 작성자가 동시에 프로젝트를 작성할 수 있도록 `E-STATE-DECL-CONFLICT`는 한 경로의 모든 frontmatter `state:` 선언(인라인 또는 스키마 import)을 비교하고 `type`, `default`, `per`, `owner`가 다르면 두 파일과 줄을 표시합니다(`scene.*` 경로는 scene-local이라 예외이며 `extends:`로 다른 선언을 정제하는 선언은 충돌하지 않음). `E-REWARD-TARGET`은 reward kind의 `target:` 계약을 검사합니다 — `{ entity: <kind> }` 또는 `{ provider: <name> }` 계약은 `target=` 값을 did-you-mean과 함께 검증하고, `required: true`는 target 없는 reward를 거부합니다. `E-RULE-AGGREGATE-CYCLE`은 자신의 head에 의존하는 relation을 `count(…)`/`countDistinct(…)`로 읽는 rule을 거부합니다. 두 권고도 추가됩니다: 대화 상자에서 두 speaker가 같은 이름으로 표시되는 `W-DISPLAY-NAME-DUP`([`lint`](#lint)와 [여러 작성자 가이드](/ko/guides/multi-author/) 참고), 그리고 `once` 없는 entry beat가 `::set` 또는 `::retract`를 포함해 매번 제시되지만 쓰기는 run의 첫 read에만 적용되는 `W-ENTRY-WRITE-REREAD`(`::assert`는 예외이며 어느 경우든 fact가 나머지 run 동안 유지됨)입니다. 이어지는 예시는 두 문서의 `run.mood` 충돌, 오타 난 reward target과 누락된 target, 반복 가능하지만 보상을 지급하는 entry를 보여 줍니다:

<!-- lute-diagnostics -->
```console
./scenes/town/welcome.lute:10:3: error [E-STATE-DECL-CONFLICT] state path `run.mood` is declared as int, default 0 at `./scenes/town/morning.lute:10` but as enum(calm|tense), default "calm" at `./scenes/town/welcome.lute:10`; every declaration of one path must agree on type, default, per and owner — they share one runtime value (declare it once in a schema both documents import)
./quests/fishing.lute:8:31: error [E-REWARD-TARGET] `goodRodd` is not a member of entity kind `bagItem`, which `<reward kind="ITEM">` targets — did you mean `goodRod`?
./quests/fishing.lute:9:3: error [E-REWARD-TARGET] `<reward kind="ITEM">` needs a `target=`: the reward kind declares `target: { required: true }`
```

<!-- lute-diagnostics unverified="verbatim lute check --project output, but composed at runtime: crates/lute-check/src/lore.rs Reread::message joins its remedies with `; `, so no single format! literal spans the text" -->
```console
./lore/contest.lute:17:3: warning [W-ENTRY-WRITE-REREAD] `<entry id="deskNote">` has no `once`, so every raise of `talk` can present it again in a run, but its `::set` applies on the first read in a run only — later reads skip it; to apply it on every presentation, put the body in a `<beat on="talk" target="npc.judge" once="false">` — a beat's writes apply each time it is presented; for one application per run, write `once="run"` on the entry; to keep the write to the first read on purpose, guard it with `when="!entry.deskNote.read"`
```

같은 릴리스에서 이 패스들의 fact 분석 비용은 beats × facts가 아니라 프로젝트가 읽는 양에 비례합니다. seed의 derived closure를 프로젝트 루트마다 한 번 준비하고 슬롯마다 확장하므로 Monster League(818 beats, 211 static facts)에서 전체 프로젝트 `check`는 동일한 출력으로 6.1초에서 약 1.2초로 줄었습니다.

`--wip` (dsl 0.23.0)는 아직 작성 중인 프로젝트를 위한 플래그이며 전체 프로젝트 모드에서만 유효합니다. `E-ENTRY-UNREACHABLE`, `E-BEAT-UNREACHABLE`, `E-OBJECTIVE-UNSATISFIABLE`, `E-ARM-DEAD`는 가드가 아직 아무것도 생성하지 않는 관계를 읽는 경우에만 죽어 있어서 — seed fact도, 어디에도 `::assert`도, rule도, `reserved`도 없을 때 — `W-WIP` 경고로 보고됩니다. 메시지에는 오류 코드가 표시됩니다. dsl 0.26.0 §2.6부터 유일한 producer가 바인딩되지 않은 `@param`을 가진 component `::assert`인 relation — 아직 어떤 `::use`도 호출하지 않은 badge component의 `hasBadge(@badge)` — 도 특정 인자에 대해서는 미생성으로 간주되며, 필수 `<objective quest=…>`의 자식이 오직 그 이유로 dead인 경우도 경고가 됩니다. 따라서 lead는 이를 채우는 영역이 아직 존재하지 않아도 골격을 검사할 수 있습니다. producer는 있지만 결코 일치할 수 없는 relation은 계속 오류이고, 하류의 `E-CONN-UNREACHABLE`도 마찬가지입니다(작성되지 않은 콘텐츠 때문에 dead인 quest도 연결성에 영향을 줌). 아직 어떤 문서도 assert하지 않은 `rumor` relation으로 가드된 entry의 예시는 다음과 같습니다:

<!-- lute-diagnostics unverified="verbatim lute check --project --wip output, but the message is composed at runtime: the E-ENTRY-UNREACHABLE literal plus the `--wip` suffix lute-check's wip_warning appends, so no single format! literal spans it" -->
```console
$ lute check --project . --wip
./lore/tomas.lute:22:74: warning [W-WIP] entry `noteWanted` is never eligible: its `when` guard `holds('rumor', ['mara'])` is provably false — no seed, assert, rule, or engine relation produces `rumor(mara)` under your declared routes — `E-ENTRY-UNREACHABLE` without `--wip`: it is dead only for want of producers not written yet (relations with no seed, assert, rule, or reserved declaration, or written only by a component `::assert` with an unbound `@param`)
```

`--wip`가 없으면 같은 줄이 `error [E-ENTRY-UNREACHABLE]`이 되고 명령은 **1**로 종료되며, 있으면 프로젝트를 통과합니다. [`lute scenario knowledge`](/ko/tooling/overviews/#lute-scenario-knowledge)는 이런 모든 relation을 `NO PRODUCER`로 나열합니다. 0.23.0부터 이 세 코드와 `E-ARM-DEAD`, `W-BEAT-PRIORITY-TIE`의 판정기는 `&&`/`||`를 경로별로도 추론하므로 `run.slot == 'morning' && run.slot == 'evening'` 같은 모순도 0.22에서 놓쳤던 위치에 보고됩니다 — [when 판정 방식](/ko/language/beats/#when의-판정-방식)을 참고하세요.

0.34.0부터 프로젝트 전체 명령은 가장 가까운 매니페스트 루트마다 하나의 `ProjectModel`을 불러옵니다. 따라서 전체 프로젝트 `check`, `scenario`, `beats`, `impact`, `calendar`, `play`, `test`는 하나의 조정된 스냅숏을 공유합니다. 분석에서 파생된 결과에는 `evidence` 수준(`proven`, `witnessed`, `bounded`, `heuristic`, `unknown`)이 붙습니다. 사람용 출력은 `[bounded: <scope>]`, `[heuristic]`, `[unknown]`을 추가하며 `proven`이나 `witnessed`에는 접미사를 붙이지 않습니다. bounded 결과는 “이 범위 안에서”라는 뜻이지 “경로가 없다”는 뜻이 아닙니다.

`lute.project.yaml`에 선언된 제약도 전체 프로젝트 `check`가 검사합니다. 위반만 진단으로 표시하며, 성립과 미확정 결과는 아래 보고서에 나타납니다.

요약 줄은 심각도별로 셉니다(0.41.0). 문서 모드 `lute check`는 파일마다 `ok: <path> (W warning(s))` 또는 `failed: <path> (E error(s), W warning(s))`로, 전체 프로젝트 모드는 `ok: <dir> (F file(s), W project-wide warning(s))` 또는 `failed: <dir> (F file(s), E project-wide error(s), W project-wide warning(s))`로 끝납니다. `info` 진단은 경고로 세지 않습니다. 실행에 info가 있으면 개수를 뒤에 덧붙이고(`, I info` / `, I project-wide info`), 없으면 위 줄을 그대로 출력합니다. `hint` 진단은 세지 않으며, `--deny`로 승격된 진단은 오류로만 셉니다.

파싱되지 않는 선언은 더 이상 연쇄 오류를 만들지 않습니다(0.41.0). 0.31에서 제거된 `type: number`는 `number` 위치에서 `E-TYPE-NUMBER`로 보고되며, `int`와 `double` 중 무엇을 고를지 알려 줍니다(그 선언이 받는 값이 모두 정수이면 `int`를 제안합니다). 선언은 이름으로 환경에 남습니다. 그런 선언이 든 export가 있는 플러그인도 나머지 항목은 그대로 불러오고, 그 선언에 기대는 곳은 선언을 고칠 때까지 검사하지 않습니다. `E-CLOCK-DECL`이나 사용처마다의 `E-UNKNOWN-DIRECTIVE` 대신, 잘못된 선언 하나당 첫 사용처에 `W-DEPENDS-ON-INVALID` 안내(info) 하나가 붙고 나머지는 개수로 셉니다.

## impact

```console
$ lute impact --node <NODE> [--project <DIR>] [--providers <DIR>] [--json]
```

어떤 프로젝트 항목이 대상을 의존하는지 묻습니다. 대상(`--node <NODE>`)은 `state:<path>`, `fact:<rel>(<args>)` (`_`는 와일드카드), `relation:<rel>`, `scene:<id>`, `quest:<id>`, `objective:<quest>.<id>`, `entry:<id>`, `beat:<id>`, `occasion:<name>`, `def:<name>`, `component:<name>`입니다. 결과는 유한 그래프에서 전이적인 역방향 의존성 폐쇄입니다. 자격, 도달 가능성, 제시된 콘텐츠, 보상 또는 효과가 대상의 역방향 폐쇄에 있는 무언가를 읽을 때 항목이 영향을 받습니다. 이는 의존성의 존재이지, 값을 바꾸면 반드시 항목이 뒤집힌다는 주장이 아닙니다. 방문 집합으로 순환을 종료하며 가장 강한 체인이 승리합니다(`proven` > `witnessed` > `bounded` > `heuristic` > `unknown`).

예를 들어 `lute impact --project docs/examples/games/drowned-crown --node fact:felled(regent)`는 영향을 받는 줄, 퀘스트/목표, 보상, 공개, 비트/씬, 하위 팩트/상태를 보고합니다. 각 항목에는 대상에서 항목으로 이어지는 이유 체인이 포함되며, 각 링크에는 edge, reason, file, line, source span이 있습니다.

`--json`의 형태는 다음과 같습니다(빈 그룹도 계속 포함됩니다).

```json
{
  "target": {"kind": "fact", "relation": "felled", "args": ["regent"]},
  "root": "docs/examples/games/drowned-crown",
  "items": {
    "lines": [{"key": "line:...", "lineId": "...", "speaker": "brann",
      "file": "lore/brann.lute", "line": 27, "evidence": "proven",
      "reasons": [{"edge": "gates", "reason": "holds('felled',['regent'])",
        "file": "lore/brann.lute", "line": 26,
        "sourceSpan": {"byte_start": 1106, "byte_end": 1172, "line": 26,
          "column": 140, "utf16_range": [1106, 1172]}}]}],
    "quests": [], "objectives": [], "rewards": [], "disclosures": [],
    "beats": [], "downstream": []
  }
}
```

## constraints

```console
$ lute constraints [--project <DIR>] [--providers <DIR>] [--script <FILE>]… [--all-scripts] [--json]
```

`lute.project.yaml`의 `constraints:` 아래에 프로젝트 불변식을 선언합니다. 지원되는 종류와 필드는 다음과 같습니다.

```yaml
constraints:
  - id: regent-reachable
    kind: reachable
    node: scene:hub.regentFell
    severity: error
  - id: library-completes
    kind: completable
    quest: libraryKey
  - id: dead-brann-never-speaks
    kind: speaksOnlyWhen
    speaker: brann
    when: "!holds('felled', ['brann'])"
  - id: no-slot-only-progress
    kind: noSingleSlotProgress
    quest: '*'
```

`reachable`은 `node`를, `completable`은 `quest`를, `speaksOnlyWhen`은 `speaker`와 CEL `when`을, `noSingleSlotProgress`는 `quest:<id>` 또는 `'*'`를 받습니다. 모든 선언에는 고유하고 비어 있지 않은 `id`, 닫힌 집합의 `kind`, 종류별 필드, 선택적 `severity`(기본값 `error`)가 필요합니다.

각 결과는 `holds`, `violated`, `unknown` 중 하나이며 `evidence`를 포함합니다. bounded 결과에는 `scope: "declared clock windows; no path search"`가 포함됩니다. 보고서는 성립과 미확정 결과를 포함해 모든 제약을 항상 나열합니다. `--all-scripts`는 프로젝트의 모든 `*.play.yaml`과 `*.test.yaml` 스크립트를 실행하고 `completable`을 위해 관찰된 퀘스트 완료 전환을 기록합니다. `--script <FILE>`(반복 가능)은 이름 붙인 스크립트만 실행하며, 디렉터리는 그 아래 모든 스크립트를 선택합니다. 둘은 함께 쓸 수 없습니다(`E-CLI-USAGE`). 전체 프로젝트 `check`는 위반(`E-CONSTRAINT-VIOLATED`)과 잘못된 선언(`E-CONSTRAINT-DECL`)만 보고하며 `W-CONSTRAINT-UNKNOWN`은 없습니다.

## Evidence levels

`proven`은 건전한 정적 증명이고, `witnessed`는 제공된 실행 또는 트레이스에서 나오며, `bounded`는 선언된 유한 범위 안에서만 건전하고, `heuristic`은 보수적인 안내이며, `unknown`은 분석이 판정하지 못했다는 뜻입니다. 진단에는 선택적 JSON `evidence` 및 `scope` 필드가 포함될 수 있습니다(bounded 진단에는 비어 있지 않은 scope가 반드시 있어야 합니다). `trace`, `play`, `test`, `calendar`, `scenario`, `beats`는 각 verdict/result에 `evidence`를 추가하며, calendar 집계 결과에는 `scope`도 포함됩니다. 파싱, 타입, 선언 오류에는 evidence가 없습니다.

impact 및 constraints 섹션은 역사적인 0.34.0 계약에서 도입되었습니다. 저장소의 현재 릴리스는 0.48.0이며 AI 편집 루프 명령은 아래에 설명되어 있습니다.
## compile

```console
$ lute compile <FILE> [--project <DIR>] [--providers <DIR>] [--permission-profile <NAME>]
                      [-o, --output <PATH>] [--locale-bundle <FILE>]
                      [--deny <CODE>]… [--deny-warnings] [--json]
$ lute compile --project <DIR> -o <DIR> [--providers <DIR>] [--permission-profile <NAME>]
                      [--locale-bundle <FILE>] [--deny <CODE>]… [--deny-warnings] [--json]
$ lute compile --stream <FILE> [--project <DIR>] [--providers <DIR>] [--permission-profile <NAME>]
```

문서를 JSON 명령 레코드 아티팩트로 컴파일합니다(정상적인 검사 통과와 유효 권한 정책에 대한 컴파일러 측 재검사를 거쳐야 합니다). 성공 시 종료 코드는 **0**, 게이트 실패 시 **1**, I/O 또는 직렬화 실패 시 **2**입니다. 아티팩트는 항상 JSON이며, `-o, --output`을 사용하면 stdout 대신 파일에 기록합니다. 프로젝트는 `--project`이거나, 없으면 파일 위쪽에서 가장 가까운 `lute.project.yaml`입니다(0.27.0, `check`와 같은 stderr 메모가 표시됨). 발견된 프로젝트도 명시한 것과 똑같이 쓰이며, 프로젝트가 있으면 게이트는 대상의 조정된 전체 프로젝트 `check` 판정이고 없으면 해당 파일에 대한 `lute check` 판정입니다. `--permission-profile` 상한은 낮추기 직전에 다시 검사하므로, 다른 정책으로 생성된 검사 결과로 금지된 IR을 승인할 수 없습니다.

### `--project` — 프로젝트 전체 컴파일 및 인덱스

문서 피연산자 없는 `compile`은 프로젝트(`--project <DIR>` 또는 발견된 프로젝트) 아래의 **모든** `*.lute` 문서를 `-o <DIR>`로 컴파일하고, 프로젝트 자체의 레이아웃을 그대로 반영합니다(`quests/a.lute` → `<outdir>/quests/a.lute.json`). 또한 `<outdir>/project.index.json`을 기록합니다. 이 모드에서 `-o`는 파일이 아닌 출력 **디렉터리**이며, 없으면 생성합니다. `*.component.lute` 조각은 건너뜁니다. 컴포넌트는 가져오는 문서에 인라인되며 자체 아티팩트는 없습니다. `--permission-profile`이 있으면 해당 상한은 소스가 선택한 각 프로필에 독립적으로 적용되며, 이름을 지정한 프로필의 플러그인을 활성화하지는 않습니다.

문서 없는 `compile`에는 프로젝트와 `-o <DIR>`이 필요하며 `<FILE>`은 받지 않습니다. 프로젝트를 찾지 못하면 `E-CLI-USAGE`(`argument: "--project"`), `-o`가 없으면 `E-CLI-USAGE`(`argument: "--output"`)이며, 문서를 읽지 않고 명령이 **2**로 종료됩니다.

```console
$ lute compile -o out
lute compile: no lute.project.yaml at or above . — pass the project directory with --project <DIR>
```

인덱스에는 문서 테이블과 각 아티팩트의 `entities`, `enums`, `relations`, `seedFacts`, `rules`, `prereqEdges`의 **합집합**이 들어갑니다. 이 합집합은 엔진이 무엇이든 평가하기 전에 [어차피 계산해야 하는 것](/ko/tooling/runtime-contract/)입니다.

```json
{
  "irVersion": "0.10.0",
  "capabilitySnapshot": "babc470773a644da19930785b89f402d4b8116530bd6b16533110de4b2a7a80a",
  "requiredSemantics": [
    "lute.core/1",
    "lute.quest.lifecycle/1"
  ],
  "documents": [
    {
      "path": "quests/findKai.lute",
      "artifact": "quests/findKai.lute.json",
      "kind": "quest",
      "key": "findKai"
    },
    {
      "path": "scenes/opening.lute",
      "artifact": "scenes/opening.lute.json",
      "kind": "scene",
      "key": "opening"
    }
  ],
  "entities": [],
  "enums": [],
  "relations": [],
  "seedFacts": [],
  "rules": [],
  "prereqEdges": []
}
```

`path`는 프로젝트 루트 기준의 소스 경로이고, `artifact`는 출력 디렉터리 기준의 컴파일된 출력 경로입니다. `key`는 문서의 정규 노드 id입니다. 씬에서는 `id:`이고, 퀘스트 문서에서는 선언된 **첫** `<quest id>`입니다(퀘스트 팩의 나머지 id는 자체 아티팩트의 `quest` 레코드에서 계속 복구할 수 있습니다). 모든 경로는 정방향 슬래시를 사용하는 상대 경로이며 절대 경로가 아니므로, 인덱스를 컴퓨터 사이에 복사하거나 게임 아카이브에 묶어도 사용할 수 있습니다.

`documents`는 `path` 순으로 정렬되고 모든 어휘 배열은 중복 제거된 완전 순서이므로, 인덱스는 실행마다 바이트 단위로 안정적입니다. 빈 어휘 배열을 생략하는 아티팩트와 달리 인덱스는 항상 여섯 배열을 모두 내보냅니다. 엔진은 이들을 무조건 합치며, 키가 없으면 “관계가 없음”과 “인덱스가 너무 오래되어 관계를 담지 않음”을 구분해야 하기 때문입니다.

### 프로젝트 컴파일은 전부 처리하거나 전혀 처리하지 않습니다

모든 문서를 메모리에서 컴파일하고 인덱스를 만든 뒤에야 파일 시스템에 접근합니다. 다음 세 가지 상황에서는 기록이 중단되며, 모두 아무것도 출력하지 않고 **1**로 종료됩니다.

- **일반 게이트 또는 권한 게이트 실패.** 한 문서의 진단이 출력된 다음 `N of M document(s) failed; no output written`이 출력됩니다. 호스트 권한 거부가 발생해도 거부된 아티팩트나 일부만 기록된 인덱스가 남지 않습니다.
- **`--deny`로 승격된 경고** (`--deny W-L10N-MISSING`, `--deny-warnings`): `--deny promoted N diagnostic(s); no output written`이 출력됩니다.
- **어휘 충돌.** 두 문서가 동일한 엔티티 종류 / enum / 관계 / prerequisite 노드를 서로 **다른** 시그니처로 선언하거나, 서로 다른 capability snapshot으로 해석하는 경우입니다. 조용히 하나를 선택하지 않습니다. 전체 프로젝트 `check`는 snapshot 사례를 `E-CAPABILITY-MISMATCH`로 보고합니다. 시그니처 충돌은 전체 프로젝트 `check`가 볼 수 없는 유일한 종류입니다. 각 문서를 자체적으로 해석된 어휘에 대해 검증하고 독립 문서 사이에서 합치지 않기 때문입니다.

```console
$ lute check --project .
ok: . (2 file(s), 0 project-wide warning(s))
$ lute compile --project . -o out
lute compile: relation `knows` is declared with conflicting signatures by two documents (`scenes/a.lute` and `scenes/b.lute`)
lute compile: 1 vocabulary conflict(s); no output written
```

`E-` 심각도의 capability 해석 진단(잘못된 플러그인 옵션, identity 템플릿, 권한 형태 또는 호스트 권한 프로필 조회 오류)도 **1**로 종료됩니다. [AI 하네스 가이드](/ko/tooling/ai-harness/#capability-resolution-오류가-종료-코드를-결정합니다)를 참고하세요.

프로젝트 컴파일은 기록하지만 정리(prune)하지는 않습니다. 소스 문서가 삭제된 아티팩트도 출력 디렉터리에 남습니다. 직접 관리하는 디렉터리에 빌드한 뒤 비우세요.

### `--locale-bundle` — 번역 번들 병합

`--locale-bundle <bundle.json>`은 로케일 번들을([`loc import`](#loc-import) 참고) 아티팩트에 병합합니다. 모든 줄 레코드와 모든 choice/hub 옵션에 `lineId`를 키로 하는 `texts` 맵을, `prompt=`가 있는 branch/hub에는 `promptLineId`를 키로 하는 `promptTexts` 맵을 추가하고, 인라인 수정자를 사용하는 줄에는 `localeSegments`를 추가합니다. 소스 언어의 `text`는 덮어쓰지 않으며 맵이 비어 있으면 생략합니다. 따라서 `--locale-bundle` 없이 컴파일한 문서는 이전과 바이트 단위로 동일합니다. 이 문서와 일치하는 항목이 없는 번들 항목은 무시됩니다. 번들은 프로젝트 전체에 걸쳐도 됩니다. 프로젝트 컴파일과 함께 사용하면 하나의 번들을 모든 아티팩트에 병합합니다. 인라인 수정자(이름, 형식, 속성의 멀티셋)가 소스 줄과 다른 번역은 병합하지 않습니다(`E-L10N-MODIFIERS`). 이 경우 아티팩트가 출력되지 않습니다.

번들이 선언한 로케일 중 번역 가능한 레코드에 없는 로케일은 `(lineId, locale)` 쌍마다 하나씩 `W-L10N-MISSING`으로 stderr에 기록됩니다. 이는 경고이므로 소스 언어 문자열을 담은 아티팩트는 여전히 출력됩니다. `--deny W-L10N-MISSING`(또는 `--deny-warnings`)을 사용하면 이를 승격하여, 아무것도 출시하기 전에 완전한 번역을 요구하도록 CI를 구성할 수 있습니다.

<!-- lute-diagnostics -->
```console
$ lute compile scenes/opening.lute --project . --locale-bundle bundle.json --deny W-L10N-MISSING -o out.json
scenes/opening.lute:1:1: error [W-L10N-MISSING] [denied] no `ja-JP` text for `narrator.s01ep01.narrator_0020`
--deny promoted 1 diagnostic(s); no artifact emitted
```

병합된 번역에 저장된 `sourceHash`([`loc import`](#loc-import) 참고)가 레코드의 현재 원문 해시와 다르면 `(lineId, locale)` 쌍마다 하나씩 `W-L10N-STALE`입니다. 번역자가 작업한 export 이후 대사, 선택지 라벨 또는 프롬프트가 바뀌었다는 뜻입니다. 메시지는 lineId와 로케일을 밝히고 `lute loc export`로 다시 내보내 번역을 갱신하라고 안내합니다. 번역은 그대로 병합됩니다. 다른 경고와 마찬가지로 단일 문서 컴파일과 프로젝트 컴파일 모두에서 `--deny W-L10N-STALE` 또는 `--deny-warnings`로 승격됩니다. `schemaVersion: 1` 번들에는 해시가 없으므로 이 검사를 하지 않습니다.

### `--stream` — 스트리밍 컴파일

`lute compile --stream <FILE>`은 완전한 씬 템플릿을 한 번 해석하고 검사합니다. `--project`를 사용하거나, 그렇지 않으면 파일 위쪽에서 가장 가까운 `lute.project.yaml`(0.27.0)에 대해 `check`와 같이 해석합니다. 그런 다음 stdin에서 추가만 가능한 일반 Lute 섹션 본문 텍스트를 읽습니다. 완성되어 승인된 각 줄/디렉티브/블록은 기존의 누적 전체 문서 파이프라인으로 컴파일되고 NDJSON으로 stdout에 플러시됩니다. 초기 전체 아티팩트를 담은 `start`, 단위마다 전체 아티팩트를 담은 `update`, 성공적인 EOF 뒤의 `finish` 순서입니다. 거부되면 진단이 담긴 `error` 레코드가 기록되고 `finish`는 나오지 않습니다.

`--stream`은 `-o`, `--locale-bundle`, `--json`, `--deny`, `--deny-warnings`와 함께 쓰거나 문서 없이 쓸 수 없으며, 충돌하는 옵션을 알려 주는 `E-CLI-USAGE`입니다. `lute schema compile`은 출력을 `artifact`(플래그 없음), `report`(`--json`, `0.43.0.compile`), `stream`(`--stream`)으로 나열합니다.

`--permission-profile <NAME>`은 프로젝트 snapshot과 함께 신뢰된 추가 상한 하나를 고정합니다. 고정 템플릿과 추가되는 모든 본문 단위에 적용되며, 스트림 중간에 전환할 수 없습니다. 금지된 단위는 거부된 IR을 담은 update가 나오기 전에 해당 `E-PERMISSION-*` 진단과 함께 종료됩니다.

성공적인 EOF 최종화 뒤에만 종료 **0**이며, 문법/의미/권한/컴파일/스트리밍 거부 시 **1**, 사용법, I/O, 끊어진 stdout 또는 잘못된 UTF-8 시 **2**입니다. 작성된 `::end`는 일반 런타임 명령이며 EOF를 대신하거나 stdin을 닫지 않습니다. 이 명령은 템플릿을 수정하거나 원격/런타임 효과를 수행하지 않습니다. 레코드 스키마, 누적 비용, 허용되는 본문 표면, 권한 고정, 소비자 커서/상태 규칙은 [스트리밍 연속 컴파일러 가이드](/ko/tooling/continuation-compiler/)를 참고하세요.

## expr

```console
$ lute expr <EXPR> [--project <DIR>] [--providers <DIR>] [--permission-profile <NAME>]
             [--deny <CODE>]… [--deny-warnings] [--json]
```

호스트 조건을 프로젝트에 대해 IR `exprNode`로 내려(lower), 게임이 런타임의 `evaluate` 질의로 실행 상태 위에서 평가할 수 있게 합니다. 런타임에는 CEL 파서가 없으므로 호스트는 빌드 시점에 조건을 내립니다. `--project`가 없으면 현재 디렉터리에서 위로 올라가며 가장 가까운 `lute.project.yaml`이 프로젝트입니다. 찾지 못하면 `E-CLI-USAGE`(종료 **2**)입니다. `--deny`/`--deny-warnings`는 lowering 진단을 승격합니다.

조건은 프로젝트 루트에 있는 씬의 `when:`으로 검사됩니다. 이 씬은 프로젝트 문서들이 가져오는 모든 스키마 파일을 가져오고, 프로젝트의 플러그인 집합 아래에서, 매개변수가 있는 def까지 포함한 [프로젝트 인덱스 `defs`](/ko/tooling/runtime-contract/#041의-projectindexjson)를 def 표로 씁니다. 즉 번들이 담는 어휘입니다. 기대 타입은 `bool`입니다. 진단은 검사기 자신의 코드(`E-CEL-PARSE`, `E-CEL-PROFILE`, `E-CEL-TYPE`, `E-UNDECLARED`, `E-UNDECLARED-REF`, `E-REF-ARITY`, `E-FACT-DOMAIN`, …)이며 `<expr>:<줄>:<열>`로 표시되고, 위치는 식 텍스트 안의 위치입니다. 검사는 통과하지만 내려지지 않는 조건은 `E-CEL-PROFILE`입니다.

해석되지 않는 프로젝트 — 매니페스트나 플러그인 오류, 또는 어떤 문서가 가져오는데 해석되지 않는 스키마 import(`E-USES-NOT-FOUND`) — 는 위치 없는 오류(`<expr>: error [CODE] …`)이며, 식은 검사하지 않습니다.

성공하면 stdout에 한 줄, 즉 JSON으로 된 `exprNode`가 나오고 경고는 stderr로 갑니다. 종료 **0**입니다. 오류가 있으면 진단은 stderr로 가고 stdout은 비며 종료 **1**입니다. `seven-days` 예제 게임에서:

```console
$ lute expr "run.day >= 3"
{"op":">=","l":{"path":"run.day"},"r":{"int":3}}
```

<!-- lute-diagnostics -->
```console
$ lute expr "@at(3)"
<expr>:1:1: error [E-REF-ARITY] `@at` expects 2 argument(s) but got 1
```

`--json`은 `command: "expr"`, `schemaVersion: "0.43.0.expr"`, `ok`를
포함하는 envelope 객체 하나를 출력하며, 컴파일된 조건의 필드(`cel`,
선택적 `authored`, `expr`, `diagnostics`)는 최상위에 그대로 둡니다. 오류도
`ok: false`와 `diagnostics`를 유지합니다.

```console
$ lute expr "@at(3, 'night')" --json
{"command":"expr","schemaVersion":"0.43.0.expr","ok":true,"cel":"(run.day == 3 && run.slot == 'night')","authored":"@at(3, 'night')","expr":{"op":"&&","l":{"op":"==","l":{"path":"run.day"},"r":{"int":3}},"r":{"op":"==","l":{"path":"run.slot"},"r":{"string":"night"}}},"diagnostics":[]}
```

## trace

```console
$ lute trace <FILE> [--project <DIR>] [--providers <DIR>] [--permission-profile <NAME>]
              [--deny <CODE>]… [--deny-warnings] [--state <PATH>=<LITERAL>]… [--fact <ATOM>]…
              [--choice <BRANCH>=<CHOICE>[,<CHOICE>…]]… [--event <N>]… [--accepted-quest <QUEST>]…
              [--occasion <OCCASION>[@<TARGET>]]… [--mock <FILE>] [--entry <ID> | --beat <ID>]
              [--no-derive] [--expand] [--json]
```

작성자가 제공한 mock을 사용해 문서를 미리 봅니다([tracing guide](/ko/tooling/tracing/) 참고). `--project`가 없으면 파일 위쪽에서 가장 가까운 `lute.project.yaml`이 `check`와 마찬가지로 프로젝트를 제공합니다(0.27.0; 이전에는 trace가 core-only로 해석되어 프로젝트의 `defaults:`가 제공하는 항목을 거부했습니다). 발견된 프로젝트도 `--project`로 준 것과 똑같이 쓰이며(단일 루트 producer 집합 포함) `--permission-profile`도 그대로 전달됩니다. `--deny`/`--deny-warnings`는 check 게이트의 진단을 승격합니다(거부, 종료 **1**). 종료 **0**은 완료, **1**은 거부(check 오류 또는 잘못된 mock — `E-TRACE-*` 코드는 check 진단처럼 렌더링됨 — 또는 dsl 0.25.0부터 두 [배타적 관계](/ko/tooling/tracing/#배타-관계)가 동시에 성립하게 만드는 write/seed와 `E-FACT-EXCLUSIVE`), **2**는 I/O, **3**은 미완료(`unknown` 가드가 진행을 중단함)입니다. `--occasion <O>`(반복 가능, dsl 0.21.0)는 퀘스트 walk가 정착한 뒤 계기를 발생시킵니다. 모든 활성 퀘스트의 같은 이름 `<on event="O">` 핸들러가 먼저 실행되고, 이어서 모든 활성 퀘스트의 `<objective on="O">` 목표를 판정합니다. mock 파일의 `occasions:` 목록도 동일하게 작동하며, `visited:` 목록은 제시된 씬으로 `visited('<id>')` 읽기를 시드합니다(목록에 없는 씬은 방문되지 않음). 퀘스트 walk는 `lute play`와 동일하게 정착됩니다. `credits:`를 선언한 보상 종류의 grant는 그 스칼라 값을 해당 경로에 더하고(grant 줄 끝에 `(credits <path> = <value>)`, JSON에서는 `credited`), 목표가 완료 상태가 될 때 목표 본문을 한 번 실행합니다. [보상과 목표 본문](/ko/tooling/tracing/#보상과-목표-본문)을 참고하세요.

mock은 세이브에서 시작할 수도 있습니다(dsl 0.22.0). `quests: { <id>: unset | active | complete | failed }`는 `quest.<id>.state`를 시드하고, `entriesRead: { run: [ids], user: [ids] }`는 엔트리 읽기 플래그를 시드합니다. `run:`은 `entry.<id>.read`와 `entry.<id>.everRead`를 모두, `user:`는 `entry.<id>.everRead`만 시드합니다. 각각 표기한 예약 경로의 mock 규칙을 따릅니다. 문서가 해당 경로를 읽어야 하며, 엔트리의 경우에는 문서가 그 엔트리를 선언해야 합니다.

**파생은 기본적으로 켜져 있습니다** (dsl 0.22.0). trace는 프로젝트의 seed `facts:`를 불러오고 mock 및 단언된 팩트에 Datalog 규칙(계층화된 부정)을 적용합니다. 따라서 결론을 mock하지 않아도 규칙으로 파생된 팩트가 가드, `done` 또는 `start`를 충족하며, 부정 전제가 성립해 거짓이 된 파생 팩트도 trace할 수 있습니다. 파생 원자를 mock하는 것도 그대로 작동하며 다른 seed와 같습니다. `--no-derive` 또는 mock의 `derive: false`(플래그가 우선)는 0.21 모델로 되돌립니다. seed를 불러오지 않고, mock하지 않은 파생 원자는 unknown이 되며, 읽은 각 파생 관계를 메모가 이름으로 표시합니다. trace, `test`, `run`, `play`는 하나의 Datalog 평가기를 공유하므로 프로젝트 규칙의 결론에 대해 서로 다른 결과를 낼 수 없습니다.

`--entry <ID>`는 시퀀스를 걷는 대신 [lore 문서](/ko/language/lore-entries/)의 `<entry>` **하나**를 제시합니다(dsl 0.19.0). 해당 줄, 선택된 `<match>` arm, 첫 읽기에서 적용되는 `::set` / `::assert` / `::retract`를 출력하며, mock이 `entry.<id>.read: true`를 시드한 경우에는 건너뜁니다. lore 문서에는 걸을 시퀀스가 없으므로 `--entry` 없이 trace하면 사용법 오류입니다(종료 **2**, 선언된 entry id를 나열함). 씬 또는 퀘스트에 `--entry`를 사용하거나 문서가 선언하지 않은 id를 지정하면 `E-TRACE-ENTRY`입니다(종료 **1**). 알 수 없는 id의 경우 메시지에 문서의 entry와 beat가 나열되고, id가 beat의 것이라면 `--beat`로 제시하라고 안내합니다.

`--beat <ID>`(dsl 0.23.0)는 lore 문서의 [번들 비트](/ko/tooling/play/#번들-비트) **하나**를 로컬 id 또는 정규 `<document id>.<beat id>`로 제시합니다. 본문은 씬처럼 걸으며(`--choice`가 분기를 결정함), 모든 효과를 적용하고 `when`은 강제하지 않고 기록합니다. lore 문서에는 `--entry`와 `--beat`를 함께 사용할 수 없습니다. 문서에 beat를 지정하지 않거나 `<beat>`가 없는 문서에 사용하면 `E-TRACE-BEAT`입니다(종료 **1**). `--occasion <name>@<target>`으로 **대상을 위해** 계기를 발생시킬 수 있고(mock에서는 `occasions: [talk@npc.oskar]`), 그러면 `<objective on="<name>" target="<target>">` 목표도 판정합니다. 대상 없이 발생시키면 해당 목표는 그대로 둡니다. [번들 비트](/ko/tooling/tracing/#번들-비트)와 [대상, 기한 및 이전 런](/ko/tooling/tracing/#대상-기한-및-이전-런)을 참고하세요.

추가로 dsl 0.24.0부터 `<match subject="@def">` subject, 가드, coverage summary는 def 참조를 작성된 형태로 출력합니다(`<match @weekday>`, `-> old (@atLeast(3))`). `--expand`를 사용하면 대신 확장된 형태를 출력하며(`--json`은 확장된 형태를 유지하고 `authoredId` / `authoredGuard` / `authoredLabel`을 추가함), mock의 `bridges:`는 bridge 결과를 읽는 플러그인 호출에 응답합니다. 응답되지 않은 호출은 결과 슬롯을 unknown으로 남기므로, 이를 대상으로 한 가드가 `bridges:` 힌트와 함께 walk를 중단합니다. `--accepted-quest`는 `activate="accept"` 자식 퀘스트를 수락하고, `complete="any"` 부모의 선택되지 않은 대안은 `superseded from quest.<parent>`를 읽습니다. `E@target` 발생은 해당 대상의 `<on event="E" target="…">` 핸들러를 실행하며, 프로젝트 trace에서는 읽은 퀘스트가 존재하는지 정착합니다. [Tracing](/ko/tooling/tracing/)을 참고하세요.

추가로 dsl 0.26.0부터 선택된 `::jump`를 해당 `::label`까지 따라갑니다(`<jump -> hall>`, JSON `{"kind": "jump", "to": "hall"}`). 따라서 `trace complete`는 문서 끝까지 walk가 도달했다는 뜻입니다. 퀘스트 `<on>` 핸들러의 `::accept`는 play와 같이 같은 문서의 퀘스트를 활성화합니다. 프로젝트가 있으면 `--accepted-quest` / `accepts:`가 프로젝트의 모든 퀘스트를 해석하며, 퀘스트 문서는 자체 `quest.<id>.*`를 시드할 수 있습니다. `--entry`는 `<document id>.<entry id>`도 받으며, [종류 비트](/ko/tooling/play/#종류-대상)는 `--state occasion.target=<member>`에서 멤버를 읽습니다. [선택된 `::jump` 따라가기](/ko/tooling/tracing/#선택된-jump-따라가기)를 참고하세요.

## scenario

```console
$ lute scenario [--project <DIR>] [--providers <DIR>] [--format text|dot] [--json] [--facts]
$ lute scenario reach [--node <NODE> | --endings[=<OCCASION>]]
$ lute scenario envelope --node <NODE>
$ lute scenario knowledge [--node <NODE>]
```

연결성 계층에 대해 읽기 전용 보고를 수행합니다. 하위 명령이 없으면 조립된 노드/엣지 그래프를 출력합니다. `reach --node <NODE>`는 노드의 [도달 가능성 판정](/ko/connectivity/reachability/)을 보고합니다. `envelope --node <NODE>`(또는 `envelope --node quest:<id>`)는 노드 진입 시점의 [Guaranteed/Possible 표](/ko/connectivity/envelopes/)와 [보장된 팩트](/ko/connectivity/envelopes/#보장된-팩트)를 출력합니다(dsl 0.20.0; `--json`에서는 `envelope.guaranteedFacts`, 각 항목은 `{fact, establishedBy}`). `<NODE>`는 씬의 정규 key, 퀘스트 id 또는 [번들 비트](/ko/tooling/play/#번들-비트)의 정규 `<document id>.<beat id>`입니다. 단순 id는 세 종류 모두에서 찾습니다. 둘 이상의 종류를 가리키면 명령을 거부하고 명시적인 `scene:`, `quest:` 또는 `beat:` 접두사를 요구하며, 이는 항상 해당 종류를 선택합니다. 성공 시 종료 **0**, I/O 또는 해석 불가/모호한 노드 id 시 **2**입니다.

`--format`은 단순 그래프 보기의 출력 형태를 `text` 또는 `dot`으로 고르고, JSON은 `--json`이 선택합니다. `--json`과 `--format`을 함께 쓰면 `E-CLI-USAGE`입니다. 보고서와 버전(`0.43.0.scenario*`)은 그대로입니다.

- `text` (기본값) — 위상 레이어를 출력한 다음, 각 엣지를 정당화하는 [원자 종류](/ko/connectivity/scene-graph/#엣지-종류)를 대괄호에 넣어 엣지마다 한 줄씩 출력합니다(또는 앵커 종류 `[accept]`, `[subquest]`, `[start]`). 그 뒤 존재하는 경우 ``unanchored (no `after` / `follows` — available from the start of play; no prerequisites in this graph):`` 아래에 비앵커 노드를 한 줄씩 출력합니다. 엣지가 없는 각 퀘스트는 `quest(<id>)`로, 그리고 (dsl 0.25.0 §3) `after`가 없고 `when`에 `visited()` conjunct가 있는 각 씬 또는 번들 비트는 작성할 `after`와 함께 출력합니다(``beat(keeper.warning) — its `when` reads visited('keeper.greeting'), which gates it but draws no edge; write `after="visited('keeper.greeting')"` to anchor it``).
- `--json` — `{"roots":[{"root":…,"layers":[[…]],"nodes":[…],"edges":[…]}]}`. 각 노드는 `{id, kind, prereq, reach}`입니다(`kind`는 `scene`, `quest`, `beat` 또는 퀘스트의 `[start]` 앵커 소스인 `entry`이며, dsl 0.25.0 §4; `prereq`는 원시 선언 식이고 entry 노드에서는 `null`). 각 엣지는 `{from, to, kinds}`이며, 하나의 식이 같은 노드를 둘 이상의 원자로 참조할 수 있으므로 `kinds`는 배열입니다. 비앵커 노드가 있는 root에는 `"unanchored": ["quest(<id>)", …]`도 포함되며, 씬 및 beat 힌트에는 `"unanchoredHints": { "<node>": "<hint>" }`도 포함됩니다(비어 있으면 각각 생략).
- `dot` — root마다 Graphviz `digraph` 하나를 출력합니다. 씬은 상자, 퀘스트는 타원, 번들 비트는 `shape=note`, 엔트리는 `shape=tab`이며, `active`만 사용하는 엣지는 `[style=dashed]`로 그립니다. 비앵커 퀘스트는 `quest(<id>) (unanchored)`로 표시한 파란색 점선 타원입니다.

**팩트 엣지** (dsl 0.26.0 §8). 팩트(배지, 핵심 아이템 등)가 진행을 게이트하면 `after:` 엣지가 그려지지 않으므로 모든 체육관이 레이어 0에 놓입니다. `--facts`는 씬, beat 또는 퀘스트의 게이트(`when:` / `start=`)가 `holds(F)`를 읽고 producer가 `F`를 단언하는 곳, 또는 규칙을 통해 `F`를 도출하는 규칙이 필요로 하는 팩트가 있는 곳에 `producer -> reader [fact]` 엣지를 추가합니다. 후자의 경우 `[hasItem(goodRod), via canPass(pier)]`처럼 표시합니다. 그 다음 순환을 만들지 않는 범위에서 레이어도 이 엣지를 기준으로 계산합니다. 텍스트 출력에서는 `fact edges (producer -> reader) [asserted fact]:` 아래에 나열하고, 레이어 앞에는 `topological layers (after: and fact edges):`를 붙입니다. `--json`은 각 항목이 `{from, to, fact, via?, layered}`인 `factEdges`를 추가하고, `--format dot`은 팩트를 레이블로 붙인 점선으로 그립니다. [스토리 개요](/ko/tooling/overviews/#팩트-간선lute-scenario---facts)를 참고하세요.

**결말** (0.27.0). 정적 판정이 결말을 반박하면 명령은 **1**로 종료하고(그 외에는 `unknown`을 포함해 0), CI가 이를 게이트로 사용할 수 있습니다. never-holds 행은 자신이 따르는 `when`에서 유도된 오류를 인용합니다(`caused by E-WHEN-LITERAL-DOMAIN at 7:99: …`, 인용된 리터럴 위치; JSON에서는 각 항목이 `{code, line, column, message}`인 `causes`, 열은 문자 단위로 계산). `--format`과 `--json`은 `reach` 앞이나 뒤에 올 수 있습니다. `reach --endings=<occasion>`은 해당 계기에 응답하는 모든 beat를 나열합니다. 인자 없는 `reach --endings`는 자체 본문 또는 `::use`하는 컴포넌트에서 `::end`를 실행할 수 있는 모든 beat(씬 beat, 번들 beat, entry)를 나열합니다. 결말마다 한 행을 출력하며, id와 파일, `reach --node <NODE>`가 제공하는 `after:` 판정, 전체 프로젝트 `check`가 도달하는 `when` 판정(`E-BEAT-UNREACHABLE` / `E-ENTRY-UNREACHABLE`: never holds, `W-BEAT-SHADOWED`: never wins), 그리고 `when`을 반박하는 것이 없는 경우 해당 `when`이 읽는 상태 경로와 팩트 원자 및 그 producer를 표시합니다. producer는 경로를 기록하는 문서(자체 `::set` 또는 `<choice into>`, `::use`된 컴포넌트, 플러그인 디렉티브의 선언된 효과, 보상의 `credits:`), `owner: engine` 경로의 엔진, 또는 쓰기가 도달하지 않는 콘텐츠 경로의 `nothing writes it`입니다. 팩트 producer는 `knowledge`와 같은 방식으로 읽습니다. 요약 행에는 `N ending(s): A reachable, B unreachable, C unknown`을 출력합니다. “Reachable”은 정적으로 반박하는 것이 없다는 뜻이며, 기록된 경로가 `when`에 필요한 값에 도달할 수 있는지는 분석하지 않습니다. 따라서 결말을 제시하는 플레이가 증명이고, `lute test --coverage`는 어떤 플레이도 제시하지 않는 beat를 나열합니다. 노드 id는 받지 않으며(`reach --node <NODE> --endings`는 사용법 오류, 종료 2), 어떤 beat도 응답하지 않는 계기도 마찬가지입니다. 명령은 종료 0으로 끝납니다. `--json`은 `{roots: [{root, occasion, endings: [{id, kind, file, verdict, after, when, needs}], summary}]}`를 제공합니다.

**번들 비트는 노드입니다** (dsl 0.23.0 §4). lore 문서의 각 `<beat>`는 `beat(<document id>.<beat id>)`로 그려집니다. `after=`를 선언하지 않았으면 엣지가 없는 레이어 0의 entry 노드이며, 대신 계기, 대상, `when`으로 선택됩니다. 선언한 경우에는 (dsl 0.25.0 §3) `after=`가 그리는 엣지의 종속 노드가 됩니다. `reach --node <beat id>`는 `Reachable`이라고 답한 다음 `after:` 행을 출력합니다(`(none declared) — an entry node: …` 또는 beat의 `after=` 식). 이어서 선언 파일(`declared in:`), 작성된 그대로의 `on:`, `target:`, `when:`을 출력합니다. `envelope --node <beat id>`는 entry-floor 표를 출력하며 seed 팩트가 보장된 팩트가 됩니다. dsl 0.24.0 §2부터 `after:`는 번들 beat(`visited('<doc>.<beat>')`)를 이름으로 지정할 수 있고 연결성은 이를 씬처럼 통과합니다. [씬 그래프](/ko/connectivity/scene-graph/#번들-비트)를 참고하세요.

비앵커 퀘스트(dsl 0.21.0 §7a.5)는 어떤 레이어나 엣지에도 놓이지 않지만 보고서에서 빠지지 않습니다. `reach --node quest:<id>`는 ``Unanchored — a quest with no declared `follows` edge: available from the start of play; …`` 판정을 제공합니다(JSON에서는 `"reach": "unanchored"`). `after:` 행은 `(none declared) — unanchored: this quest is in no prerequisite graph layer and on no edge; it is available from the start of play.`입니다. `follows=` 없이 앵커된 퀘스트는 이를 활성화하는 `::accept`, 부모(서브퀘스트), 또는 `start` conjunct(dsl 0.25.0 §4)가 제공하는 앵커를 대신 나열합니다. `[start] entry(keeperLog)`처럼 한 줄씩 표시하며, JSON에서는 각 항목이 `{ kind, from }`인 `anchors`입니다.

0.23.0부터 그래프 보기 마지막에는 그리지 않은 `completed()`/`active()`/`visited()` 참조를 나열하는 `note:`도 나옵니다. 퀘스트의 엣지는 `follows`, 서브퀘스트 트리, `start` conjunct, `::accept`에서 나오므로 엣지 없는 퀘스트를 이름으로 지정하는 `completed()`와 앵커 역할을 하는 `start` conjunct 밖에서 읽는 퀘스트의 `visited()`가 나열됩니다(`--json`에서는 `omitted`). [시나리오 그래프가 생략하는 것](/ko/tooling/overviews/#시나리오-그래프에서-생략하는-것)을 참고하세요. `knowledge [--node <NODE>]`는 모든 팩트 게이트 조건을 규칙을 따라 추적하여 팩트 producer인 단언 문서, seed 팩트, `reserved` 또는 `NO PRODUCER`를 찾습니다. dsl 0.24.0부터 beat, entry 및 objective 가드, 줄 `when=`, `<choice when>`, `<when>` arm 테스트, `::jump`/`::set` `when`, `<on when>`, 보상 `when`, 퀘스트 `start`/`fail` 및 목표 `until` 등 모든 가드 슬롯을 다룹니다. 문서별로 각 가드의 소스 줄과 함께 묶습니다. 부정은 “holds unless defeated”로 읽으며, 각 무효화 원인마다 `defeated when …` 한 줄을 출력합니다. 파생 무효화 원인은 모든 도출 경로와 함께 `⇐ … / ⇐ …`로 표시합니다(dsl 0.25.0 §9). 또는 “always holds (…) — cannot be defeated”를 출력하며, 파생 원자의 규칙은 한 번만 출력하고 이후 언급에서는 `traced above under …`라고 합니다. `knowledge`는 텍스트(기본) 또는 `--json`을 받습니다(JSON 요소에는 `for`와 `line`이 포함됨). `--node`는 씬(내부의 모든 가드), 번들 beat, entry id, `<quest>.<objective>`, `quest:<id>`, 또는 `<scene>#<branch>.<choice>`(단일 choice)를 받으며, 일치하지 않으면 did-you-mean과 함께 종료 **2**입니다. 규칙의 전제도 추적합니다. 엔티티 종류 원자는 멤버십으로 읽습니다(`person(ada) — entity kind `person`; ada is a member`, 선언되지 않은 관계가 아님). `cel(…)` 전제는 읽는 상태를 이름으로 표시합니다(`cel("run.slot == 'evening'") — state condition on run.slot, decided at run time`). `--json`에서 규칙 전제는 `{ relation, negated? }`, `{ entityKind }` 또는 `{ cel }`입니다. `envelope`의 Facts 행도 동일한 producer 표현(`seed facts …`, `reserved — the engine asserts it`, `derived by 1 rule`)을 사용합니다. 두 기능 모두 실제 출력과 함께 [스토리 개요](/ko/tooling/overviews/#lute-scenario-knowledge)에 설명되어 있습니다.

추가로 dsl 0.26.0 §6부터 규칙 본문은 개수를 셀 수 있습니다(`canPass(earthGymDoor) :- count(hasBadge(_)) >= 7`). `knowledge`는 이 전제를 `count(hasBadge(_)) >= 7 — counts:`로 추적하고, 그 아래에 집계된 팩트의 producer를 표시합니다.

## beats

```console
$ lute beats [--project <DIR>] [--providers <DIR>] [--occasion <OCCASION>]… [--target <TARGET>]… [--expand] [--json]
```

프로젝트의 모든 beat를 계기별 사다리 하나로, 대상 지정 계기라면 대상별 사다리로 나누어 선택 순서대로 출력합니다(dsl 0.23.0). 출력에는 priority, beat와 title, 종류(`scene`, `entry`, `bundle`), `once`(`run`, `user`, `day`, `slot`, 또는 `no`; 번들 beat의 `once="day"` / `once="slot"`도 포함), `also`, 그리고 dsl 0.25.0부터 `share` 키가 포함됩니다(`day, share solWarm`; `--json`에서는 `share`). 또한 전체 프로젝트 `check` 판정(`unreachable`, `shadowed`, `tied`, `once-run-user`), `after:`(번들 beat의 `after=`도 포함), 작성자가 쓴 그대로의 `when`을 출력하며 dsl 0.24.0부터 `@def` 참조도 포함합니다. `--expand`는 각 `when`을 def 확장 형태로 출력하고, `--json`은 두 형태를 `when`(확장)과 `whenAuthored`로 모두 담습니다. `--occasion`과 `--target`(둘 다 반복 가능)은 사다리를 거르고, `--json`은 각 판정의 전체 진단을 담습니다. 읽기 전용이며, 프로젝트가 깨끗하게 검사를 통과할 필요는 없습니다. 성공하면 종료 **0**, I/O 오류나 알 수 없는 `--occasion`이면 **2**입니다. [스토리 개요](/ko/tooling/overviews/#lute-beats)를 참고하세요.

추가로 dsl 0.26.0부터, 더 앞서 있고 아직 소진되지 않은 beat가 그 `when`이 암시하는 경우 항상 우선하는 fallback은 판정 열에 `covered by <id>`를 표시합니다(`--json`에서는 `coveredBy`). 이는 진단이 아닌 정보이므로, lead가 여전히 재생되는 fallback과 더 이상 재생될 수 없는 fallback을 구분할 수 있습니다. [종류 비트](/ko/tooling/play/#종류-대상)는 어떤 beat가 이름으로 지정한 모든 멤버의 사다리와, 어떤 beat도 자체적으로 이름으로 지정하지 않은 멤버의 `kind:<kind>` 사다리에 나열됩니다. `--target`은 대상 지정 계기 도메인의 모든 멤버를 받습니다.

추가로 dsl 0.28.0부터 [종류 비트](/ko/tooling/play/#종류-대상) 또는 `for=` beat의 `when`이 일부 멤버에게 작성된 형태로 절대 성립하지 않거나(`occasion.target != 'village'`), 해당 멤버에 필요한 팩트를 생성하는 것이 아무것도 없으면 판정 열에 `never for <member>`를 표시합니다. 종류 beat는 해당 멤버의 사다리에, `for=` beat는 계기의 사다리에 표시합니다(`--json`에서는 `neverFor`). beat는 다른 멤버에게는 여전히 재생되므로 전체 프로젝트 `check`는 아무것도 보고하지 않습니다.

## calendar

```console
$ lute calendar [--project <DIR>] [--providers <DIR>] [--permission-profile <NAME>]
                [--deny <CODE>]… [--deny-warnings]
                [--axis <AXIS>=<VALUES>]…
                [--occasion <OCCASION>]… [--occasion-axes <OCCASION>=<AXES>]… [--relation <RELATION>]…
                [--target <TARGET>]… [--script <FILE> [--until <STEP>]] [--where <EXPR>]
                [--format text|csv] [--json]
```

`lute play` 자체의 자격 판정을 상태 값 격자의 모든 셀에서 평가합니다(dsl 0.23.0). 각 셀과 각 계기/대상 열에 대해 격자는 승자와 `+N`개의 가려진 자격 beat, 제공 목록 또는 sequence 목록, 자격 없음일 때 `-`, 판정할 수 없는 `when`이 셀을 결정할 때 `?`를 표시합니다. 어떤 셀에서도 자격이 없는 beat는 이유와 함께 마지막에 나열되며, dsl 0.24.0부터 일부 셀에서는 자격이 있지만 어느 셀에서도 제시되지 않은 beat도 그 위에 무엇이 제시되었는지와 함께 나열됩니다(`--json`에서는 각각 `beatenBy`를 담은 `neverPresented`, `--format csv`에서는 두 번째 표). 각 셀은 같은 월드에서 시작하고 축 값을 기록한 다음 퀘스트를 정착한 뒤 계기를 평가합니다.

- `--axis <path>=<values>` (선택, 반복 가능) — 선언된 경로의 타입에 맞춰 검사하는 포괄적 정수 범위(`run.day=1..7`) 또는 목록(`run.slot=morning,evening`)입니다. 첫 번째 축이 가장 느리게 변하며 `--axis`가 없으면 격자는 한 셀입니다. `quest.<id>.state=unset,active,…`는 세이브의 `quests:`와 같이 퀘스트 상태를 시드합니다(`unset`과 `active`는 replay된 경로가 만든 목표 진행도 지움). `quest.<id>.objectives.<oid>.done`은 작성된 형태로 허용됩니다. 그 밖의 `quest.*` 경로는 사용법 오류입니다. `holds(<fact>)=true,false`는 선언된 관계의 base 팩트를 단언하거나 철회합니다. 파생 관계는 거부됩니다(`` `trusted(ada)` is derived by rules and cannot be asserted ``). `visited('<id>')=true,false`(dsl 0.24.0)는 씬 또는 번들 beat를 방문 집합에 넣거나 뺍니다. 따라서 `after: visited(…)` beat는 양쪽을 읽습니다. `clock[=<d1>..<d2>]`(dsl 0.24.0 §1)는 해당 날짜의 모든 슬롯을 시계 순서로 확장합니다(단순 `clock`: 한 주). 그 밖의 입력은 축 종류를 나열하는 사용법 오류이며, 잘못 입력한 상태 경로에는 did-you-mean을 제공합니다.
- `--axis <family>.*=<values>` / `--axis '<family>[<axis>]=<values>'` (0.27.0부터) — [`per:` family](/ko/state/state-model/#엔터티마다-하나의-경로-per) 전체에 하나의 축을 적용합니다. `run.aff.*=6,7`은 모든 멤버(`run.aff.ren`, `run.aff.kai`, …)에 셀 값을 주고, `run.aff[run.route]=6,7`은 해당 셀에서 `--axis run.route` 값이 이름으로 지정한 멤버에만 적용하며 다른 멤버는 seed 또는 기본값을 유지합니다. 하나의 축 쌍으로 route × own affection을 표현할 수 있습니다. 인덱싱 축은 같은 calendar에서 선언된 `--axis`여야 하며 family 종류의 멤버를 하나 이상 이름으로 지정해야 합니다. 아무 멤버도 지정하지 않는 값에서는 연결된 축이 아무것도 설정하지 않으므로 한 셀이 그 모든 값을 나타냅니다. 단순 family(`--axis run.aff=6,7`)는 세 형식을 이름으로 안내하는 사용법 오류입니다(`` `run.aff` is a `per: suitor` family — name a member (`run.aff.ren`), all of them (`run.aff.*`), or tie it to an axis (`run.aff[run.route]`) ``). family 축이 이미 설정한 멤버에 대한 두 번째 축도 마찬가지로 오류입니다. 행 레이블, `--json`의 `at` 키 및 CSV 헤더에는 축을 작성한 그대로 표시합니다.
- `--occasion <OCCASION>` (반복 가능) — 평가할 계기이며, 기본값은 beat가 응답하는 모든 계기입니다.
- `--occasion-axes <OCCASION>=<AXES>` (반복 가능, dsl 0.24.0) — `<OCCASION>`을 평가하면서(이 옵션이 그 계기를 선택하므로 `--occasion`에 따로 쓰지 않습니다) `<AXES>`(`<axis>[=<value>],…`)에 이름이 지정된 축만 변화시킵니다. 다른 모든 축이 첫 값인 곳에서 평가하고(`--occasion-axes dayEnd=run.day,run.slot=night`는 대신 `run.slot`을 `night`에 고정), 다른 곳은 비워 둡니다. 따라서 하루 한 번 계기는 하루에 한 번 읽습니다(`--json`: 열의 `varies` 및 `heldAt`). `--axis clock`에서는 시계의 축을 이름으로 지정합니다. `--occasion-axes dayEnd=clock.day,clock.slot=night` 또는 시계 자체 경로인 `--occasion-axes dayEnd=run.day,run.slot=night`로 `dayEnd`를 하루 한 슬롯에서 평가합니다(지정하지 않으면 그날의 첫 슬롯, 시계가 발생시키는 `raise.dayEnd` 계기에서는 그날의 마지막 슬롯). 같은 계기를 `--occasion`과 `--occasion-axes`에 모두 쓰거나 `--occasion-axes`에 두 번 쓰면 `E-CLI-USAGE`입니다.
- `--axis clock`에서 시계의 `raise:`가 이름으로 지정한 계기는 `lute play`의 `advance:`가 발생시키는 위치에서만 판정합니다. `slot` 계기는 런이 시작한 위치를 제외한 모든 위치, `dayStart`는 런이 시작한 날을 제외한 각 날짜의 첫 슬롯, `dayEnd`는 각 날짜의 마지막 슬롯(끝나는 시계에서는 마지막 위치)에서 판정합니다. 시계에 `raiseAtStart: true`가 있으면 런이 시작한 위치에서도 `slot`과 `dayStart`를 발생시킵니다. 나머지 셀은 모두 `not raised`이며 승자, 자격 목록, 가려진 beat를 표시하지 않습니다(`--json`: 결과에 `"notRaised": true`, root에 `{ occasion, reason }` 목록인 `notRaised`, CSV: `notes`에 `not raised`). 헤더 줄에는 시계가 계기를 발생시키는 위치가 표시됩니다. 이런 셀에서만 자격 있는 beat는 `` eligible at day 1 (Mon) morning, where the clock does not raise `dayStart` `` 이유로 never eligible입니다. 런은 세이브가 시계를 둔 위치에서, `--script` 단계 전에 시작합니다. 슬롯을 이름으로 지정한 `dayEnd` 열(`dayEnd@clock.day,clock.slot=afternoon`)은 해당 위치에서 판정하며, `advance: day`는 시계가 어디에 서 있든 그날을 닫습니다.
- `--target <T>` (반복 가능) — 대상 지정 계기를 발생시킬 대상입니다. 기본적으로 대상 지정 계기는 도메인의 모든 멤버가 아니라 beat가 이름으로 지정한 각 대상별 열 하나를 얻습니다. 어떤 beat도 대상을 이름으로 지정하지 않으면 대상 없는 beat만 응답하는 단일 `(any)` 열을 얻습니다.
- `--relation <RELATION>` (반복 가능, dsl 0.24.0) — 정착된 각 셀에서 관계의 팩트를 출력합니다. 첫 번째 인자마다 한 행, 셀마다 한 열인 표입니다(`--relation at`: 누가 언제 어디에 있는지). `--json`에서는 셀별 `facts` 맵, `--format csv`에서는 `facts:<rel>` 열을 제공합니다.
- `--script <FILE>` — play 스크립트입니다. 모든 셀은 세이브에서 시작하며 `steps:`를 `lute play`와 정확히 같은 방식으로 replay합니다.
- `--until <STEP>` — `--script`와 함께 사용하며, 이 단계 전까지만 replay합니다. 1부터 세는 단계 번호 또는 `label:` 단계이며(알 수 없는 레이블에는 did-you-mean이 표시됨), 해당 단계 자체는 재생하지 않습니다.
- `--where <EXPR>` — 축을 기록하고 퀘스트를 정착한 뒤 이 조건이 성립하는 셀만 남깁니다. 어떤 run도 도달하지 않는 독립 축 조합을 제거할 때 사용합니다. 이는 일반 CEL이며 `@def` 호출은 여기서 확장되지 않습니다. unknown으로 평가되는 셀은 사용법 오류입니다.
- `--format text|csv` / `--json` — 출력 형식입니다(`text`가 기본이며 `--json`과 `--format`을 함께 쓰면 `E-CLI-USAGE`). `--json`에는 셀이 시작한 위치를 나타내는 `from`, `--where`로 제거된 셀인 `pruned`, 셀별 `notes`가 포함됩니다.

셀 수가 10,000개를 초과하면 거부되며, 중단되는 replay도 사용법 오류입니다. 성공 시 종료 **0**, 프로젝트 컴파일 실패 시 **1**, I/O 또는 사용법 오류 시 **2**입니다. [스토리 개요](/ko/tooling/overviews/#lute-calendar)를 참고하세요.

## context

```console
$ lute context <FILE> [--project <DIR>] [--providers <DIR>] [--permission-profile <NAME>]
                      [--node <NODE> | --at <FILE>:<LINE>:<COLUMN>] [--max-items <N>]
                      [--script <FILE>]… [--all-scripts] [--json]
```

이 파일의 프로젝트에 대해 AI 또는 사람이 유효한 Lute를 작성하는 데 필요한 프로젝트 해석 **저작 표면**을 출력합니다. 디렉티브, attrs, enum, asset 종류, provider, 상태 스키마, 관계형 어휘, 전달 플래그, 인라인 텍스트 수정자, 참조된 예약 퀘스트 경로, 유효 권한 레이어 및 `capabilitySnapshot`이 포함됩니다. 이는 검증이 아닌 capability 질의이므로 문서 진단이 있어도 출력합니다. `--project`가 없으면 파일 위쪽에서 가장 가까운 `lute.project.yaml`이 프로젝트입니다(0.27.0, `check`와 동일). `--permission-profile`을 사용하면 JSON `permissions`는 `{ "layers": [...] }`이고, `bridges`에는 허용된 bridge capability 객체만 들어가며, `rewardKinds`는 허용된 이름 키 object입니다(보상이 거부되면 비어 있음). `questsAllowed`는 boolean입니다. `directives`에는 directive가 거부된 항목과 `service/operation`이 거부된 bridge directive가 모두 제외됩니다. 외부 읽기 전용 상태는 계속 표시됩니다. 텍스트 출력은 컴파일 시점 저작 제한을 설명하며 런타임 sandbox를 제공한다고 주장하지 않습니다. 성공 시 종료 **0**, I/O 시 **2**이며, 프로젝트/프로필 해석 오류는 제한 없는 상태로 처리하지 않고 그대로 드러냅니다.

0.22.0부터 이 표면에는 `defs`(각 이름 있는 조건의 `name`, `type`, `params`, `body`), `builtinDirectives` 아래의 언어 내장 디렉티브(`::set`, `::assert`, `::retract`, `::accept`, `::use`, `::body`, `::jump`, `::label`, `::end`, `::clear`, 각각 `syntax`와 `meaning` 포함), 그리고 `ids`가 추가로 들어갑니다. `ids`는 `--project`의 모든 씬, 퀘스트 및 lore entry id입니다(`{ scenes, quests, entries }`; `--project`가 없으면 문서 자체의 id). 관계는 `tier`와 `reserved` 여부를 보고하고, `owner: engine`으로 선언된 상태 경로는 이를 표시합니다. 계기에는 `description`과 `target`(`false`, `true`, 또는 `{ prefix, entity }` 도메인)이 포함됩니다. 사람용 출력은 `talk (select: first, target: npc.<npc>)`처럼 표시합니다. 또한 사람이 읽는 개요에는 가져온 각 컴포넌트의 매개변수와 타입이 출력됩니다. [AI 하네스 가이드](/ko/tooling/ai-harness/#프롬프트-context-lute-context---json)를 참고하세요.

`directives`에는 각 core 및 plugin directive와 그 `attrs`(`name`, `type`, `required`), `semantics`가 나열됩니다. 도메인 값을 갖는 속성의 `type`은 `domain:<name>`입니다(사람용 개요: `camera: focus: string, framing: domain:framing, move: domain:cameraMove, transition: domain:transition, …`, `music: playback: domain:musicPlayback, …`). `deliveryFlags`에는 `{mono}`, `{os}`, `{vo}`와 각각의 의미가 나열됩니다. `mono`는 유효 `pov` 또는 `monoSpeakers`의 speaker에만 적용됩니다. `textModifiers`는 콘텐츠 줄 text의 인라인 수정자 표면입니다. core의 `pause`와 `speed`, 프로젝트 `textStyle` 멤버(또는 도메인이 선언되지 않았다는 메모), 수정된 줄이 어떻게 lowering되는지가 포함됩니다. core-only `--json` 발췌는 다음과 같습니다.

```json
{
  "capabilitySnapshot": "babc470773a644da19930785b89f402d4b8116530bd6b16533110de4b2a7a80a",
  "textModifiers": {
    "core": [
      {
        "form": "leaf",
        "meaning": "a pause of `s` seconds (required, non-negative) at that point of the line",
        "name": "pause",
        "syntax": ":pause{s=0.5}"
      },
      {
        "form": "span",
        "meaning": "the wrapped text is delivered at `rate` (required, positive); a nested speed uses the innermost rate",
        "name": "speed",
        "syntax": ":speed[text]{rate=1.25}"
      }
    ],
    "lowering": "the IR `text` is the plain derivation (markup removed, escapes decoded, `{{…}}` kept, one `placeholders` entry each); a modified line adds `segments`: text runs {text, styles?, rate?} and {pause} leaves; in `text`, segment runs and `texts` a literal `{{` is written `\\{{` and a literal backslash directly before `{{` is doubled"
  }
}
```

## tag

```console
$ lute tag [<PATH>]… [--renumber [--commit]] [--check] [--json]
```

기본 모드(`--renumber` 없음)는 태그가 없는 모든 `:line`에 안정적인 `code`를, 누락된 컴포넌트 인스턴스 키와 함께 뒤에서 채워 파일을 제자리에서 다시 씁니다. 이미 완전히 태그된 문서는 바이트 단위로 그대로 둡니다. `--renumber`는 위험한 모드로, 모든 줄의 `code`를 깨끗한 문서 순서로 다시 번호 매깁니다(`0010`/`0020`/…를 speaker와 scope마다 적용). 이는 초안 작성 도구이며, frontmatter에 `codesLocked:`가 선언된 문서(게시된 코드는 `lineId`/`voiceKey` identity)나 구조 오류가 있는 문서에서는 코드 재번호가 거부됩니다(종료 **1**. `codesLocked:` 문서에서도 누락된 컴포넌트 인스턴스 키는 계속 추가됩니다). `--commit`이 있어야만 씁니다. `--commit` 없는 `--renumber`는 **아무것도 쓰지 않고** `--commit`이 할 일을 미리 보여 줍니다. 바뀌는 코드마다 한 줄 `<path>:<line>: <old or (none)> -> <new>`, 인스턴스 키가 추가될 파일마다 한 줄, 거부된 파일마다 거부 메시지를 출력한 다음 `lute: would renumber <N> line(s) in <M> of <T> file(s); pass --commit to apply`를 출력하며, 종료 코드는 **0**이거나 파일이 거부되면(`--commit`도 그렇듯) **1**입니다. `--check`는 아무것도 쓰지 않고 선택한 모드가 어떤 파일이든 바꾸면 **1**, 아니면 **0**으로 종료합니다(거부도 **1**). `--commit`을 `--renumber` 없이, 또는 `--check`와 함께 쓰면 `E-CLI-USAGE`입니다. 성공 시 종료 **0**, 문서가 거부되면 **1**, I/O 시 **2**입니다.

`<PATH>`는 파일 또는 디렉터리일 수 있으며 기본값은 `.`입니다(dsl 0.22.0). 그 아래 모든 `.lute` 파일을 전체 프로젝트 `check`가 순회하는 것과 같은 정렬 순서로 재귀적으로 다시 씁니다. 변경된 파일은 파일명을 담은 줄을 출력하고, 할 일이 없는 파일은 조용히 둡니다. 마지막에는 요약을 출력합니다. 거부되거나 읽을 수 없는 파일은 보고한 뒤 순회를 계속하며, 종료 코드는 파일 전체 결과 중 가장 심각한 결과입니다. `--json`은 보고서 `0.48.0.tag`를 출력합니다. `{mode: "tag"|"renumber", applied, files: [{path, changed, lines, instances, refused?, renumbered?: [{line, from, to}]}]}`이며 `applied`는 파일을 실제로 썼을 때만 `true`, `lines`는 추가되거나 다시 매겨진 코드 수, `instances`는 추가된 인스턴스 키 수, `refused`는 거부 메시지, `renumbered`는 renumber 모드에서만 나오고 코드가 없던 줄의 `from`은 `null`입니다.

```console
$ lute tag scenes
lute: scenes/town/day-end.lute: tagged 1 line(s)
lute: scenes/town/morning.lute: tagged 1 line(s)
lute: scenes/town/welcome.lute: tagged 1 line(s)
lute: scenes/talk/mara-first.lute: tagged 3 line(s)
lute: scenes/talk/mara-idle.lute: tagged 2 line(s)
lute: tagged 8 line(s) in 5 of 5 file(s)
```

`--renumber` 미리 보기의 출력 형식은 다음과 같습니다.

```console
$ lute tag --renumber scenes
<path>:<line>: <old or (none)> -> <new>
…
lute: would renumber <N> line(s) in <M> of <T> file(s); pass --commit to apply
```

## fix

```console
$ lute fix [<PATH>]… [--check] [--json]
```

기계적이며 의미를 보존하는 migration을 제자리에서 적용합니다. 바이트 단위로 정확하고 주석을 보존하며, 무언가 변경된 경우에만 다시 쓰고 다시 실행하면 아무것도 적용하지 않습니다. 각 `<PATH>`(기본값 `.`)는 `.lute` 파일, 프로젝트/스키마 YAML 파일 또는 디렉터리입니다. 디렉터리라면 그 아래 모든 `.lute` 및 YAML 파일을 재귀적으로 정렬된 순서로 처리합니다. 여러 경로는 지정된 순서대로 migration합니다. 변경된 파일마다 자체 줄을 출력한 다음 요약(`lute: applied N fix(es) in M of K file(s)`)을 출력합니다. 변경할 것이 없는 단일 파일은 `lute: nothing to fix`을 출력합니다. 성공 시 종료 **0**, I/O 시 **2**이며, 파일 전체 결과 중 가장 심각한 결과입니다. `--check`는 아무것도 쓰지 않으며 적용할 수정이 하나라도 있으면 **1**로 종료합니다. `--json`은 보고서 `0.48.0.fix`(`{applied, files: [{path, changed, fixes}]}`, `fixes`는 텍스트가 출력하던 개수)를 출력합니다.

0.37 표면 변경은 다음과 같습니다. 각각 `lute check`가 새 표기와 `lute fix`가 다시 작성한다는 메모를 보고합니다.

| 이전 | 이후 |
| ------ | ----- |
| `::auto{…}` | `::actor{…}` |
| `::cut{…}` | `::cg{…}` |
| `::next{to="x"}` | `::jump{to="x"}` |
| `::mark{id="x"}` | `::label{name="x"}` |
| `::music{action=…}` | `::music{playback=…}` |
| `::cg{action=…}` / `::video{action=…}` | `::cg{display=…}` / `::video{display=…}` |
| `<match on="…">` | `<match subject="…">` |
| `<choice label="…">` | `<choice text="…">` |
| `@speaker{id="x"}: …` | 앞 줄에 `::label{name="x"}`을 쓰고, `id=`가 없는 줄로 변경 |

이름이 바뀐 도메인은 프로젝트·스키마·플러그인 YAML에서도 바꿉니다(0.41.0). 최상위 `enums:`·`entities:` 블록 바로 아래의 키와 모든 `domain:` 타입 참조의 값이 대상이며 — `musicAction` → `musicPlayback`(0.37.0에서 이름 변경) — 바꾼 곳마다 수정 하나로 셉니다. 같은 매핑에 새 이름이 이미 있으면 옛 키를 그대로 두고 stderr에 알립니다: `` lute fix: <file>:<line>: kept `musicAction:` — `musicPlayback:` is already declared ``. 그 전까지 새 이름에 대한 `E-DOMAIN-UNKNOWN`은 `` — this project declares `musicAction`, renamed `musicPlayback` in 0.37.0; run `lute fix` ``로 끝납니다.

YAML 처리는 텍스트를 바이트 단위로 읽으므로, 스칼라·따옴표 문자열·주석 안의 한글, em dash 같은 UTF-8 문자는 바이트 그대로 유지됩니다(0.41.1. 0.41.0은 여기서 panic했습니다).

툴체인보다 오래된 기존 언어 스탬프는 툴체인 버전으로 올립니다(0.41.1). `lute.project.yaml`의 `defaults: luteVersion`과 `.lute` 문서 자체의 frontmatter `luteVersion`이 대상이며, 각각 수정 하나로 세고 값 주변의 따옴표와 주석은 유지합니다. 따라서 해당 `W-LUTE-VERSION-STALE`이 사라집니다. 없는 스탬프는 추가하지 않으며, 더 새롭거나 `MAJOR.MINOR.PATCH` 형식이 아닌 스탬프는 그대로 둡니다.

`player.bindings.yaml`의 `bindingsVersion`이 툴체인 라인(툴체인 버전의 `MAJOR.MINOR`)보다 오래된 `MAJOR.MINOR`이면 그 라인으로 올립니다(0.47). 수정 하나로 세고 값 주변의 따옴표와 주석은 유지하므로 해당 `W-BINDINGS-VERSION-STALE`이 사라집니다. 더 새롭거나 `MAJOR.MINOR` 형식이 아닌 값(앞에 0이 붙은 값 포함)과 하나의 flow 매핑으로 쓴 파일은 그대로 둡니다. [`bindingsVersion` 규칙](/ko/player/bindings/#bindingsversion)을 참고하세요.

옛 문서화된 `volume` 단계를 그대로 쓰는 프로젝트에서는 그 단계의 이름을 바꿉니다(0.42). 프로젝트의 접힌(folded) `volume` 도메인이 정확히 `[silent, down, normal, up, full]`일 때 — `volume` 도메인을 보는 모든 문서에서 그 멤버가 그 순서로 — 선언, 모든 `volume=` 디렉티브 값, `player.bindings.yaml`의 `volumes:` 키에서 `down`은 `quiet`, `up`은 `loud`가 되며 토큰마다 수정 하나로 셉니다. 프로젝트가 직접 고른 레벨(부분집합, 다른 이름, 다른 순서)이거나 문서들이 서로 다르면 건드리지 않습니다. 프로젝트 밖의 `.lute` 파일은 자신의 프런트매터로 판단합니다.

작성자 key의 snake case는 재작성 방식이 명확한 경우에만 lowerCamelCase가 됩니다. 인라인 텍스트 수정자는 절대 새로 만들지 않으며, 기존 수정자는 바이트 단위로 그대로 보존합니다. 이전 migration도 계속 적용됩니다. `:line[speaker]{…}: text` → `@speaker{…}: text`, 선행 `:` sigil → `@`, choice `as="…"` → `into="…"`, 리터럴 비교 `<when test="$ == 'gold'">` → `<when is="gold">`(`W-WHEN-TEST-LITERAL`), 그리고 0.31 CEL 팩트 질의 및 presence 호출 → 표준 CEL 형식입니다.

0.36 씬:

```lute expect="E-MATCH-NO-SUBJECT,E-RENAMED-ATTR,E-RENAMED-DIRECTIVE,E-RENAMED-TAG-ATTR"
---
kind: scene
id: pier
title: The pier
state:
  run.day: { type: int, default: 1 }
---
## Arrival
::cut{action="show" assetId="cg.pier"}
<branch id="ask">
  <choice id="stay" label="Stay">
    ::next{to="farewell"}
  </choice>
  <choice id="go" label="Leave">
    @mara: Fine.
  </choice>
</branch>
<match on="run.day">
  <when is="1">
    @narrator: The first morning.
  </when>
  <otherwise>
    @narrator: Another morning.
  </otherwise>
</match>
@mara{id="farewell"}: Safe travels.
```

```console
$ lute fix scenes/pier.lute lore
lute: scenes/pier.lute: applied 8 fix(es)
lute: lore/dock.lute: applied 8 fix(es)
lute: applied 16 fix(es) in 2 of 2 file(s)
$ lute fix scenes/pier.lute lore
lute: applied 0 fix(es) in 0 of 2 file(s)
```

이제 씬은 깨끗하게 검사됩니다.

```lute check
---
kind: scene
id: pier
title: The pier
state:
  run.day: { type: int, default: 1 }
---
## Arrival
::cg{display="show" assetId="cg.pier"}
<branch id="ask">
  <choice id="stay" text="Stay">
    ::jump{to="farewell"}
  </choice>
  <choice id="go" text="Leave">
    @mara: Fine.
  </choice>
</branch>
<match subject="run.day">
  <when is="1">
    @narrator: The first morning.
  </when>
  <otherwise>
    @narrator: Another morning.
  </otherwise>
</match>
::label{name="farewell"}
@mara: Safe travels.
```

`lute fix`는 의도를 추측하지 않습니다. 다음 항목은 `lute check`가 보고하며 수동으로 migration해야 합니다.

- camera의 `zoom`, `moveX`, `moveY`, `shake`, `reset`, `easing`(`E-CAMERA-REMOVED`) — `framing`, `cameraMove` 또는 `transition` 멤버를 선언하고 `framing=`, `move=` 또는 `transition=`을 작성합니다.
- cg의 `full`(`E-CG-LAYOUT`) — `cgLayout` 멤버를 선언하고 `layout=`을 작성합니다.
- music의 `track` 및 sfx의 `name`(`E-REMOVED-ATTR`) — `assetId`를 사용합니다(음향 효과에는 `sound`도 사용).
- 선언된 `musicAction` 도메인 — 이를 선언하는 모든 문서, 스키마 및 플러그인에서 `musicPlayback`으로 이름을 바꿉니다(그렇지 않으면 `E-DOMAIN-UNKNOWN`).
- 유효 `pov`가 아니고 `monoSpeakers`에도 나열되지 않은 speaker의 `{mono}` 줄(`E-MONO-POV`, `E-MONO-NO-POV`) — 문서 또는 프로젝트 `defaults:`에 `pov:` 또는 `monoSpeakers:`를 설정합니다.
- 프로젝트의 `sequence:`(`E-REMOVED-PROJECT-KEY`) 및 `<scene>` 태그(`E-REMOVED-TAG`).
- 본문의 `# Title` heading(`E-INERT-TITLE`) — frontmatter의 `title:`로 옮깁니다.
## catalog refresh

```console
$ lute catalog refresh [--project <DIR>] [--providers <DIR>] [--json]
```

`--providers <DIR>`는 다시 스탬프할 스냅숏 디렉터리이며 기본값은 프로젝트의 `catalogDir`(기본 `catalog/`)입니다. 프로젝트(`--project` 또는 발견)가 스탬프를 정하므로, 프로젝트 안에서는 스탬프가 프로젝트의 다중 플러그인 버전입니다(프로젝트 없이 실행하면 core 기준선). 이 디렉터리의 고정된 모든 provider 스냅숏을 현재 `capabilitySnapshot`에 맞춰 다시 스탬프하고 `stale` 플래그를 지웁니다([providers & catalog](/ko/tooling/providers-and-catalog/) 참고). 프로젝트도 `--providers`도 없으면 `--providers`를 알려 주는 `E-CLI-USAGE`입니다. `--json`은 보고서 `0.48.0.catalog-refresh`(`{providers, capabilitySnapshot, refreshed: [path], skipped: [{path, reason}]}`)를 출력합니다. 성공 시 종료 **0**, I/O 오류 시 **2**입니다.

## init

```console
$ lute init [<DIR>] [--template <NAME>] [--json]
```

`lute check --project`를 실행할 수 있는 새 Lute 프로젝트 디렉터리를 스캐폴딩합니다. `<DIR>`의 기본값은 `.`입니다. 쓸 파일 중 하나라도 이미 있으면 아무것도 쓰지 않고 그 파일을 알려 주며 종료 **2**로 거부합니다. 안내하는 "Next steps"와 생성된 README는 0.48 명령줄을 씁니다. `--json`은 보고서 `0.48.0.init`(`{dir, template, files: [path]}`)을 출력합니다. `--template`은 시작 콘텐츠를 선택합니다.

- `minimal` (기본값) — core만 포함한 `lute.project.yaml`, 상태 스키마, vocabulary 스키마, `id:`가 있는 시작 씬, trace mock, `tests/`의 통과하는 시나리오 테스트입니다.
- `investigation` — 완성된 추리극입니다.
  dsl 0.24.0부터 `beats` 뼈대 위에 만든 작은 추리극입니다. `arrive`를 발생시키는 `case.occasions` 플러그인, 대상 지정 `examine` (`item.<evidence>`) 및 `interview` (`npc.<suspect>`), `accuse`가 들어 있습니다. 증거 로어는 기록된 내용을 `::assert`하고, 계층화된 부정을 사용하는 파생 규칙은 알리바이가 있고 증거가 반박하지 않으면 용의자를 `cleared`로 만들며 `culprit`는 연루되었지만 cleared되지 않은 존재로 만듭니다. 수락 방식 퀘스트, `holds('culprit', ['…'])`로 선택지를 가드하는 고발, `expect:`가 있는 `plays/the-case.play.yaml`, 시나리오 테스트도 포함됩니다. 스캐폴딩된 상태에서 `check`, `test`, `play`가 통과하며 README에는 `beats` 명령도 나옵니다.
- `beats` (dsl 0.22.0) — [비트와 계기](/ko/tooling/play/)로 구동되는 게임입니다. 프로젝트 로컬 occasions 플러그인(하나의 계기와 `{ prefix, entity }` 대상 도메인), `defaults:`에서 `luteVersion`과 `uses`를 제공하는 매니페스트, `owner: engine` 시계와 축약 `defs`가 있는 `world.schema.yaml`, `id:`가 있는 씬 비트, 퀘스트, 로어 엔트리 비트, `engine:` 스텝과 `expect:`가 있는 플레이 스크립트, 시나리오 테스트가 포함됩니다. 스캐폴딩된 상태에서 `check`, `test`, `play`가 모두 통과합니다.

어떤 템플릿도 `identity:`를 고정하지 않습니다. 기본 `voiceKey`가 이미 씬 접두사를 포함하기 때문입니다. 명령은 만든 모든 파일과 다음에 실행할 명령을 출력합니다. 성공, I/O 오류, 알 수 없는 템플릿, 덮어쓰기 거부 시 종료 코드는 각각 **0**, **2**입니다.

## lore

```console
$ lute lore [--project <DIR>] [--json]
```

프로젝트의 **세계 내러티브 맵**(dsl 0.19.0)을 출력합니다. 프로젝트 디렉터리 아래의 모든 [로어 엔트리](/ko/language/lore-entries/)를 `target`과 `series`(`order` 순서)별로 그룹화한 뒤, 프로젝트 어디에서든 단언된 모든 관계에 대해 각 ground fact와 이를 드러내는 주체 — 로어 엔트리, 번들 비트, 씬/퀘스트, 또는 이 중 둘 이상인 `both` — 및 그렇게 하는 엔트리, 비트, 문서를 보여 줍니다. 비트는 엔트리 옆의 대상 아래에 `beat`로 표시하고 그 계기를 뒤따르게 합니다. [번들 비트](/ko/tooling/play/#번들-비트)는 정식 `<document id>.<beat id>` 아래에, 씬 비트는 씬 키 아래에 표시됩니다.

```text
  item.rusty_key
    beat  lore.dock.talk  "The key"  lore/dock.lute  (on talk)
    rustyKey  [item]  lore/ship.lute
```

번들 비트가 단언한 사실은 엔트리가 아니라 비트(`beats:` 줄)에 귀속되며, 씬 비트의 단언은 해당 씬에 남습니다. `--json`은 동일한 보고서를 `targets`, `series`, `relations` 배열을 가진 객체로 출력합니다. 비트 행에는 `"kind": "beat"`와 `on`이 들어가고, 각 사실에는 `entries`, `beats`, `documents`가 들어가며 `revealedBy`는 `entries`, `beats`, `scenes`, `both` 중 하나입니다. 읽기 전용이며 문서가 깨끗하게 검사될 필요는 없습니다. 성공 시 **0**, I/O 오류 시 **2**입니다.

 dsl 0.24.0부터 각 엔트리와 비트 행 뒤에 `when`이 나오며, 보고서 끝에는 **Derived** 섹션이 붙습니다. 프로젝트가 단언한 것에서 규칙이 도달할 수 있는 모든 파생 관계와 결론, 그 뒤의 규칙 인스턴스(`⇐` 뒤에 전제와 각 전제의 출처), 근거가 되는 증거, 이를 게이트하는 조건을 보여 줍니다(`--json`에서는 `derived`, 각 사실에 `fact`, `from`, `evidence`, `gates`).

```text
Derived (what the rules can conclude from what the project asserts)
  trusted
    trusted(ada)
      ⇐ met(ada) [scene `inn.ada` (scenes/inn/ada.lute)], not rumor(ada)
      evidence: scene `inn.ada` (scenes/inn/ada.lute)
      gates: objective `ferry.word` (quests/ferry.lute), scene `inn.again` (scenes/inn/again.lute)
```

## refs

```console
$ lute refs [--project <DIR>] [--attr <DIRECTIVE>.<ATTR>]… [--reward <KIND>]… [--json]
```

어떤 엔진 콘텐츠 id를 누가 사용하는지(dsl 0.26.0 §2.5) 보여 줍니다. 디렉티브 속성(`--attr give.item`)의 모든 값 또는 보상 종류(`--reward ITEM`)의 모든 `target=`을, 사용하는 문서와 줄과 함께 출력하므로 여러 작가의 작업을 병합하기 전에 누가 무엇을 주는지 확인할 수 있습니다. 같은 막대가 세 영역에서 나눠 지급되거나 TM이 두 가지 방식으로 표기되는 경우처럼, 두 플래그는 반복·조합할 수 있으며, 둘 다 지정하지 않으면 사용법 오류(종료 **2**)입니다. 컴포넌트를 통해 속성에 도달하는 값 — 본문 `::give{item=@item}` 위의 `::use{component="gift" item="goodRod"}` 및 중첩 `::use` 포함 — 은 값을 바인딩한 `::use` 줄에 ``(via component `gift`)``로 표시됩니다. `target=`이 없는 보상은 `(no target)`으로 표시되고, 아무것도 사용하지 않는 속성이나 종류는 `no uses`로 표시됩니다.

```console
$ lute refs --project . --attr give.item --reward ITEM
::give.item: 2 value(s)
  `goodRod` — 1 use(s) in 1 document(s)
    scenes/wren.lute:15 (via component `gift`)
  `potion` — 1 use(s) in 1 document(s)
    scenes/wren.lute:13
reward ITEM: 2 value(s)
  (no target) — 1 use(s) in 1 document(s)
    quests/fishing.lute:9
  `goodRod` — 1 use(s) in 1 document(s)
    quests/fishing.lute:8
```

`--json`은 `{ "queries": [ { "kind": "attr" | "reward", "name", "values": [ { "value", "uses": [ { "document", "line", "via"? } ] } ] } ] }`를 출력하며, `(no target)`의 `value`는 `null`, `via`는 컴포넌트 이름입니다. 이 보고서는 나열만 하고 검증하지 않습니다. 엔티티 종류로 타입 지정된 속성(`item: { type: { entity: bagItem } }`)과 보상 종류의 `target:` 계약은 `check`가 판단합니다. 읽기 전용이며 문서가 깨끗하게 검사될 필요는 없습니다. 성공 시 **0**, I/O 또는 사용법 실패 시 **2**입니다. [여러 작가를 위한 가이드](/ko/guides/multi-author/)를 참고하세요.

## new

```console
$ lute new <scene|quest|lore|schema> <NAME> [--project <DIR>] [--json]
$ lute new scene <NAME> [--project <DIR>] [--occasion <OCCASION>] [--target <TARGET>] [--json]
$ lute new quest <NAME> [--project <DIR>] [--start] [--json]
```

기존 프로젝트에 새 문서 하나를 스캐폴딩합니다. 첫 번째 인자는 문서 종류(`scene`, `quest`, `lore`, `schema`)이고 `<name>`은 문서의 `id:`가 되며 파일 이름도 그 id를 따릅니다. 입력한 대소문자는 id에 유지됩니다(`isles.harborNight`는 그대로 `isles.harborNight`). 구문을 이루는 단어와 `-` 이름의 부분은 camel case로 합쳐지고(`The Epilogue` → `theEpilogue`, `the-hunt` → `theHunt`), 점이 있는 이름의 점은 유지됩니다. 이름의 `/`는 하위 폴더에 파일을 중첩하며, 씬에서는 폴더를 id에 점으로 반영합니다. `lute new scene talk/tomas-evening`은 `scenes/talk/tomasEvening.lute`에 `id: talk.tomasEvening`을 씁니다. `lute new quest lamp.oil`은 `quests/lamp.oil.lute`에 `id: lamp.oil`을 쓰고, `lute new lore old-map`은 `lore/oldMap.lute`에 `id: oldMap`과 `item.oldMap`에 연결된 `<entry>` 하나를 씁니다 — `quest.`나 `lore.` 접두사는 추가하지 않습니다. 모든 문서는 매니페스트의 `defaults:`가 이미 제공하는 항목(예: `luteVersion`, `uses`)을 생략합니다. `lute new quest`는 **수락 방식** 스텁을 스캐폴딩하며 `start`가 없고 이를 시작하는 `::accept{quest="…"}`를 설명하는 주석이 있습니다. `--start`는 자동 시작하는 `start="true"` 형식을 씁니다. `--project`는 프로젝트를 지정하며(없으면 현재 디렉터리에서 위로 올라가며 가장 가까운 `lute.project.yaml`을 찾습니다), 문서는 프로젝트 루트 아래에 생성됩니다. 프로젝트 하위 폴더에서 실행해도 발견된 프로젝트 전체에 대해 동작합니다. `--json`은 보고서 `0.48.0.new`(`{kind, id, path}`)를 출력합니다. 종류는 하위 명령이므로 `lute new <kind> <NAME>` 형태는 그대로입니다. 프로젝트 밖에서는 `lute new`가 stderr에 이를 알리고 독립적으로 완전한 문서를 씁니다.

명시한 `--project`가 프로젝트 안이지만 루트가 아니면 거부됩니다(종료 **2**, 아무것도 쓰지 않음). 메시지는 `--project`를 알려 주고 대신 사용할 표기를 제안합니다.

```console
$ lute new scene tavi-shell --project scenes/talk
lute new: `--project scenes/talk` resolves to `/private/tmp/tp/town/scenes/talk`, which is inside the project at `/private/tmp/tp/town` (its lute.project.yaml) but not its root; `--project` names the project, not the destination folder — did you mean `lute new scene talk/tavi-shell`? (nothing was written)
```

씬, 퀘스트, 로어는 하나의 문서 id 집합을 공유하므로 다른 문서가 이미 선언한 id가 되는 이름은 거부되며 아무것도 쓰지 않습니다.

```console
$ lute new quest town/welcome
lute new: `scenes/town/welcome.lute` already declares `id: town.welcome` — scenes, quests and lore share one set of document ids; pick another name; nothing was written
```

`--occasion <occasion>`은 해당 계기에 응답하는 씬 [비트](/ko/language/beats/)를 만들고, `--target <target>`은 대상 지정 계기가 무엇을 위해 발생하는지, 즉 대상 도메인의 `<prefix>.<member>`를 지정합니다. 파일을 쓰기 전에 둘 다 프로젝트에 맞춰 검사합니다. 프로젝트 플러그인이 선언하지 않은 계기이거나 계기 도메인 밖의 대상이면 did-you-mean과 함께 종료 **2**가 되고 파일을 만들지 않습니다.

```console
$ lute new scene talk/tomas-evening --occasion talk --target npc.tomass
lute new: target `npc.tomass` is outside occasion `talk`'s domain `npc.<npc>` (`npc.mara`, `npc.tomas`) — did you mean `npc.tomas`?; nothing was written
```

대상 지정 계기에는 `--target`이 필요합니다. `target:` 없는 씬은 모든 대상을 위해 재생되기 때문입니다.

```console
$ lute new scene talk/tomas-evening --occasion talk
lute new: occasion `talk` is raised for a target, and a scene without `target:` would play for every one — pass `--target npc.<member>`, the target this scene answers; nothing was written
```

`--occasion O`는 항상 `on: O`를 씁니다(0.29.0). `O`가 매니페스트의 [`chapters:`](/ko/getting-started/connect-scenes/#장chapter들-chapters) 연쇄가 응답하는 계기라면, 그 씬을 대신 챕터로 만드는 방법도 출력합니다. 해당 연쇄의 `scenes:`에 id를 나열하고 `on:`을 제거하면 됩니다.

`--occasion` 없는 `--target`은 사용법 오류이며, 씬이 아닌 항목에 `--occasion`을 쓰는 것도 마찬가지입니다. 프로젝트 밖에서는 선언된 계기가 없으므로 `--occasion`을 거부합니다. 성공 시 **0**, I/O, 잘못된 종류, 거부된 `--occasion`/`--target`, 다른 문서가 선언한 id, 프로젝트 루트가 아닌 폴더를 가리키는 `--project`이면 **2**입니다.

## doctor

```console
$ lute doctor [--project <DIR>] [--strict] [--json]
```

로컬 도구체인과 프로젝트 설정을 진단합니다. 버전 축, 프로젝트 매니페스트, 콘텐츠 문서, 플레이 스크립트(`*.play.yaml`)와 시나리오 테스트(`*.test.yaml`), provider 스냅숏, 활성 플러그인, 선언된 각 계기와 이에 응답하는 비트 수, 선언된 vocabulary 슬롯, 편집기 통합을 확인합니다. `--project`는 검사할 프로젝트 디렉터리이며(없으면 현재 디렉터리에서 위로 올라가며 찾고, 찾지 못하면 이전처럼 매니페스트 누락을 보고합니다) 문서 스캔은 이 프로젝트 디렉터리를 순회합니다. provider 스냅숏은 매니페스트의 `catalogDir:`(기본값 `catalog/`)에서 찾습니다. 이는 `check`가 읽는 디렉터리입니다. 없으면 `no pinned provider snapshots`라고 표시합니다. 편집기 검사는 `PATH`에서 처음 찾은 `lute-lsp`에 `lute-lsp --version`을 실행하고, 다른 버전을 보고하거나 0.22.0보다 오래되어 버전을 보고하지 않는 경우 이를 표시하며 재설치 방법도 안내합니다. 실행 중인 `lute` 옆의 `lute-lsp`와 `PATH`의 것을 비교해(`lute-lsp beside lute`; `--json`에서는 `siblingLanguageServer`) 먼저 발견된 것이 다른 빌드이면 버전이 같아도 검사에 실패시키고 두 바이너리를 표시합니다. 해결하려면 `lute` 디렉터리를 `PATH` 앞에 두거나 편집기의 언어 서버를 옆의 `lute-lsp`로 지정합니다.

```console
$ lute doctor --project .
lute doctor — .
  • toolchain version: 0.22.0
  • language version: 0.22.0
  • IR schema version: 0.22.0
  ✓ lute.project.yaml: found at ./lute.project.yaml
  ✓ content documents: 7 `.lute` file(s) under .
  • play scripts: 1 `*.play.yaml`
  • scenario tests: 2 `*.test.yaml`
  • provider snapshots: no pinned provider snapshots
  • active plugins: game.occasions 0.1.0
  • occasions (beats answering): 3 declared, 7 beat(s) — dayEnd (1), townVisit (2), talk (4)
  • vocabulary slots declared: emotion, action (exits: fadeOut/hide), anchor (default: center), mood, volume, musicPlayback, vfxType
  • VS Code extension: not detectable from the CLI
  ✗ lute-lsp on PATH: /usr/local/bin/lute-lsp reports no version (older than 0.22.0) — differs from lute 0.22.0
      → reinstall the language server from this toolchain (`cargo install --path crates/lute-lsp`) and restart the editor
```

편집기는 자신이 시작한 `lute-lsp`를 계속 실행하므로 도구체인을 재설치해도 열린 편집기에는 적용되지 않습니다. 따라서 macOS와 Linux에서 doctor는 실행 중인 모든 `lute-lsp` 서버(`--json`의 `runningLanguageServers`)도 나열하고, 서버 시작 후 바이너리가 교체되었거나 바이너리가 다른 버전을 보고하는 서버를 표시하며 편집기 또는 언어 서버를 재시작하라고 안내합니다.

```console
  ✗ running lute-lsp: pid 12431 (/usr/local/bin/lute-lsp) started before its binary was replaced, so it runs an older build
      → restart the editor (or its language server) so it launches this toolchain's lute-lsp
```

`--json`은 동일한 검사를 `{ "dir", "checks": { <key>: { label, ok, detail, hint } } }` 객체 하나로 출력합니다(정보 줄에서 `ok`는 `null`). 기본적으로 doctor는 보고서일 뿐 게이트가 아니므로 검사 결과와 관계없이 종료 **0**입니다. `--strict`(dsl 0.26.0)는 검사 하나라도 실패(`✗`)하면 **1**을 반환합니다. 실행 중인 오래된 `lute-lsp`, `lute` 옆의 다른 빌드, 오래된 스냅숏 등이 이에 해당하므로 하네스나 CI 단계에서 깨진 설정으로 시작하는 것을 막을 수 있습니다. `--project` 디렉터리를 읽을 수 없으면 **2**입니다. dsl 0.26.0부터 교체된 `lute-lsp`는 편집기에서도 이를 자체적으로 알립니다 — [Editors](/ko/tooling/editors/#바이너리보다-오래된-서버)를 참고하세요.

## run

```console
$ lute run <FILE> [--engine <FILE>] [--mock <FILE>] [--occasion <OCCASION>[@<TARGET>]]…
              [--entry <ID> | --beat <ID>] [--conditions-output <FILE>] [--json]
```

**컴파일된 아티팩트**(`lute compile` 출력)를 mock 플레이스루에 대해 헤드리스로 실행합니다. [런타임 계약](/ko/tooling/runtime-contract/)의 참조 소비자로서 명령 디스패치, CEL 가드, facts와 Datalog 고정점, 허브, 퀘스트 수명주기를 실행합니다. `--engine <FILE>`은 엔진의 변경 불가능한 `lute.engine.yaml`을 로드하고 아티팩트의 정확한 major.minor IR 라인을 **먼저** 검사한 다음, 컴파일러가 도출한 모든 `requiredSemantics` id를 플레이백 전에 매트릭스의 `supportedIds`와 비교합니다. id가 없으면 종료 **2**와 `E-ENGINE-SEMANTICS`로 실행을 거부하며 명령, 조건, 에셋, 브리지는 실행되지 않습니다. 잘못된 매트릭스는 `E-ENGINE-MATRIX`, 지원하지 않는 IR 라인은 `E-ENGINE-IR-VERSION`, 알 수 없는 id는 `E-SEMANTICS-UNKNOWN`을 사용합니다. `--engine`을 생략하면 내장 `reference` 매트릭스를 사용합니다. `--mock`은 YAML 플레이스루이며(`lute trace --mock`과 동일한 표면), `--json`은 기계 판독 가능한 트랜스크립트를 출력하며, `--conditions-output <FILE>`은 CEL 조건을 평가할 때마다 JSONL 레코드 하나를 `<FILE>`에 추가합니다. 완전 실행 시 **0**, 거부 시 **1**, I/O 또는 협상 실패 시 **2**, 미완료 시 **3**입니다.

 dsl 0.24.0부터 mock의 `bridges: { <tag>: [ {<field>: value}, … ] }`는 브리지 결과를 읽는 플러그인 호출에 응답합니다. 태그별 호출 순서대로 한 번에 하나씩 답하며, 응답 값은 결과 슬롯에 기록되고 `plugin` 레코드에는 `"answered": [{"field", "value"}, …]`가 들어갑니다(사람용 표기: `plugin check (bridge answered: passed=true, margin=3)`). 답이 없는 호출은 `unresolvedEffects`를 기록에 남긴 채 계속 진행합니다. 실패한 `quest` 레코드에는 `failedBy`가 들어가고, 퀘스트 실패는 이유를 출력합니다(`quest X -> failed (by)`, `(until)`, `(fail)`, `(cascade)`, `(superseded)`).

퀘스트 아티팩트에서 `--occasion <O>`(반복 가능, dsl 0.21.0)는 walk가 정착한 뒤, mock 자체의 `occasions:` 다음 CLI 순서로 계기를 발생시키고, 활성 퀘스트의 `<objective on="O">` 목표를 판정합니다. 각 발생은 `{"kind": "occasion", "occasion": "O"}` 레코드입니다(사람용: `  occasion O`). `start`가 없는 퀘스트는 엔진에서처럼 수락 방식으로 동작하여 mock의 `accepts:`가 이름을 지정하거나 `accept` 레코드가 실행될 때까지 `unset` 상태입니다. 씬의 `::accept{quest="<id>"}`는 `{"kind": "accept", "position": "<position>", "quest": "<id>"}` 레코드입니다(사람용: `<position>  quest <id> accepted`). walk가 이미 퀘스트가 `unset`보다 이후 상태임을 알고 있으면 레코드에 `"ignored": "already <state>"`가 들어가고 줄은 ` (already <state> — ignored)`로 끝납니다. mock의 `visited:` 목록은 `visited('<id>')`가 읽는 씬을 presented 상태로 시드합니다.

`--entry <ID>`는 **로어 아티팩트** 하나의 `entry` 레코드를 제시합니다(dsl 0.19.0; [엔진 계약](https://github.com/journeyWorker/lute/blob/main/docs/runtime/lore-entries.md)). 트랜스크립트는 첫 읽기인지와 `when`이 성립하는지를 보고하고, 본문 세그먼트를 실행하며, 첫 읽기 효과를 적용합니다(`entry.<id>.read`가 `true`로 시드된 경우에는 건너뛴 것으로 기록). 그런 다음 `entry.<id>.read`를 설정합니다. 로어 아티팩트에 필요하며 다른 종류에서는 거부됩니다(둘 다 종료 **2**).

`--beat <ID>`(dsl 0.23.0)는 대신 로어 아티팩트의 **번들 비트** 하나를 제시합니다. 정식 `<document id>.<beat id>`(모호하지 않으면 비트 id만 가능)로 이름 붙은 `beat` 레코드와 본문 세그먼트를 씬처럼 실행하며 모든 효과를 적용합니다. 로어 아티팩트에는 `--entry`와 `--beat` 중 정확히 하나가 필요합니다. 아티팩트가 선언하지 않은 id나 다른 아티팩트 종류에서의 `--beat`는 종료 **2**입니다(`` lute run: `--beat oskar.hnut` names no beat in this artifact (declared: oskar.hunt, oskar.rumor) ``). `--occasion <name>@<target>`은 대상에 대한 계기를 발생시키고 `<objective on="<name>" target="<target>">` 목표를 판정합니다(사람용: `  occasion talk → npc.oskar`). 다른 대상에 대한 발생이나 대상 없는 발생은 이를 판정하지 않습니다.

## play

```console
$ lute play --script <FILE> [--project <DIR>] [--providers <DIR>] [--permission-profile <NAME>]
            [--engine <FILE>] [--events | --json] [--no-derive] [--derivation <ATOM>]… [--ir] [--quiet]
            [--conditions-output <FILE>] [--deny <CODE>]… [--deny-warnings]
```
프로젝트 **전체**를 발생한 **계기(occasion)**의 시퀀스로 스토리를 플레이합니다(dsl 0.21.0). [비트와 계기](/ko/tooling/play/)의 참조 런타임 소비자입니다. 프로젝트(`--project` 또는 발견된 프로젝트)는 메모리에서 한 번 컴파일되며, 프로젝트 `compile`이 사용하는 것과 같은 게이트와 선언 유합(씬, 퀘스트, 로어 문서)을 사용합니다. `--engine <FILE>`은 `lute.engine.yaml`을 로드하고 정확한 major.minor IR 게이트를 먼저 수행한 다음 프로젝트 인덱스의 `requiredSemantics` 유합을 `supportedIds`와 비교합니다. id가 없으면 어떤 스텝, 명령, 조건, 에셋, 브리지도 실행하기 전에 종료 **2**와 `E-ENGINE-SEMANTICS`로 거부합니다. 잘못된 매트릭스와 지원하지 않는 IR 라인은 `E-ENGINE-MATRIX` / `E-ENGINE-IR-VERSION`을 사용합니다. 생략하면 내장 `reference` 매트릭스를 사용합니다. 필수 `--script`는 닫힌 키 집합을 가진 `*.play.yaml` 파일입니다. 각 `steps:`는 계기 `occasion:` 발생(그리고 `target:`, `pick:`, 해당 제시에서 스크립트의 `choose:` 키를 키별로 대체하는 스텝 로컬 `choose:`), `newRun:` 시작, `engine:`을 통한 엔진 소유 내용 쓰기(`state:` 리터럴 또는 `{ add: <n> }`, `facts:`, `retract:`), 월드 `event:` 발생, 선언된 시계를 `advance: slot | day | <n>`으로 이동(dsl 0.24.0: 시계의 경로를 쓰고, 퀘스트를 정산하며, 시계의 `raise:`에 따라 지나는 자정마다 `dayEnd` / `dayStart`를, 멈춘 곳에서 `slot` 계기를 한 번 발생시킵니다. 같은 순간의 `engine:` 쓰기를 함께 실을 수 있으며, 이 쓰기는 시계가 도착한 곳에 적용됩니다 — 도중의 `dayEnd` / `dayStart` 이후, 마지막 정산과 발생 이전), 또는 `end: true`로 플레이스루를 끝냅니다. 그리고 어떤 스텝이든 `label:`, `repeat: <n>`, 자체 `bridges:` 응답, `expect: { winner, offered, notOffered, presented, options, quests, state, facts, notFacts }`를 가질 수 있으며, `- include: <file>`은 다른 파일의 스텝을 그 자리에 이어 붙입니다. `steps:` 외에도 스크립트는 `state:`, `facts:`, `choose:`, `bridges:`에 대해 `lute trace --mock` 문법을, 시작점이 되는 세이브(`visited:`, `presented: { run, user }`, `quests:`, `entriesRead: { run, user }`), `derive:`, 그리고 플레이의 끝을 판정하는 최상위 `expect: { exit, quests, state, facts, notFacts, transcriptContains, transcriptLacks }`(dsl 0.22.0)를 받습니다. 각 계기 스텝은 계기의 후보 비트와 그 판정을 나열하고, 승자(또는 `select: all` 계기에서는 스텝의 `pick`; `pick: none`이거나 자격 있는 비트가 없을 때 `pick`이 없으면 아무것도 제시하지 않음)를 `lute run`의 참조 평가기로 제시하며, 모든 퀘스트 생애주기를 진행시키므로 이후의 `when` 조건과 `after: completed(…)`가 실제 진행을 봅니다. `--json`은 같은 트랜스크립트를 하나의 객체로 내보냅니다.

dsl 0.26.0부터 `advance: { to: <slot> }` / `advance: { to: { weekday, slot } }`는 다음 해당 위치로 시계를 이동합니다(앞으로만, 0단계는 불가). 스텝 `expect:`는 `clock: { weekday, slot, day }`를 받고(0.28부터 `ended`도 가능), `engine: { accept: [quest ids] }`는 플레이 도중 수락 방식 퀘스트를 수락합니다(`quest <id> accepted (engine)`). 엔트리는 `pick:`, `entriesRead:`, 선택 기대값에서 `<document id>.<entry id>`로 이름 붙일 수 있습니다. `kind:<kind>`를 대상으로 하는 비트는 해당 종류의 모든 멤버에 응답합니다. 실행되지 않은 가드 디렉티브는 `skip ::give{item="potion"} — when: false`를 출력합니다. 브리지 응답은 결과 슬롯 또는 capability의 `result:`로 타입이 정해지며, 타입 없는 응답은 거부됩니다(종료 **2**). 문서가 하나의 상태 경로를 두 타입으로 선언하는 프로젝트도 거부됩니다(종료 **1**). 프로젝트는 플레이마다 한 번만 로드됩니다 — Monster League에서 플레이 로드 시간이 약 12초에서 약 1.5초로 줄었습니다. [스토리 플레이](/ko/tooling/play/)를 참고하세요.
`--events`는 재개 가능한 런타임 이벤트 스트림을 compact JSON Lines로 출력합니다. 첫 줄에는 seed와 그 출력이 들어가고, 이후 플레이어가 취한 각 입력은 출력과 짝을 이룹니다(`choose`와 `bridgeResult` 응답 포함). 모든 출력에는 `eventVersion: "0.47.2"`가 들어갑니다. `--json`, `--ir`, `--derivation`, `--conditions-output`과 함께 사용할 수 없습니다.

`--no-derive`(또는 스크립트의 `derive: false`; 플래그가 우선)는 프로젝트의 Datalog 규칙 적용을 중지하므로 mock하지 않은 파생 atom은 unknown이 되고 walk가 미완료로 멈춥니다. `--derivation <ATOM>`(반복 가능)은 플레이 후 ground atom의 도출 트리를 출력합니다. 사용된 규칙과 각 전제의 자체 근거(시드 fact, 단언, 또는 차례로 파생)가 나오고, 부정 전제는 `(absent)`로 표시됩니다. 성립하지 않으면 이를 결론낼 수 있는 모든 규칙과 실패한 전제를 출력합니다. `--json`에서는 같은 트리가 `explain` 아래에 들어갑니다. `--deny <CODE>`/`--deny-warnings`는 프로젝트 compile 게이트의 진단을 승격합니다. `--conditions-output <FILE>`은 CEL 조건을 평가할 때마다 JSONL 레코드 하나를 `<FILE>`에 추가합니다.

종료 **0**은 모든 스텝을 플레이하고 모든 기대값을 충족했거나 `end: true` 스텝이 플레이스루를 끝낸 완전 상태입니다. **1**은 오류입니다(프로젝트 컴파일 실패, vocabulary 충돌, 자격 없는 `pick`, 자격 있는 비트가 있는데 `pick` 없는 `select: all` 스텝, 스텝 자체의 `bridges:` 응답이 소비되지 않음, 두 [배타적 관계](/ko/tooling/play/#배타-관계)가 동시에 성립함 — 쓰기 시 `✗ exclusive: …`, dsl 0.25.0 — 또는 `expect:` 불일치. 각 불일치는 스텝, `label:`, 실제 값을 명시). **2**는 사용법/I/O 실패입니다(잘못된 스크립트, 알 수 없는 계기 또는 `expect:` 키, 계기 대상 도메인 밖의 스텝 대상, 프로젝트가 선언하지 않은 save id, 호출에 맞지 않거나 필드가 없는 `bridges:` 응답, `include:` 순환, 선언된 타입에 맞지 않거나 시계를 뒤로 이동시키는 `engine:` 쓰기, 그리고 모든 `quest.*` 쓰기 — 퀘스트 상태는 수명주기가 소유하며 save는 최상위 `quests:`로 시드 — 또는 읽을 수 없는 프로젝트). **3**은 미완료입니다(스크립트에 없는 choice나 hub, 참조 런타임이 결정할 수 없는 `when` 또는 퀘스트 목표, `now()`/`validAt()`, `bridges:` 응답이 없는 플러그인 호출로 호출에서 중단). 제공되지 않은 `choose:` 결정 — 자격 없는 choice 또는 이미 선택한 `once` 허브 옵션(`E-TRACE-CHOICE`) — 은 자격 없는 `pick`과 마찬가지로 종료 **1**입니다. `lute test`는 `expect:`가 있는 모든 플레이 스크립트를 실행합니다. 스크립트 형식, 선택 순서, 트랜스크립트 형태는 [스토리 플레이](/ko/tooling/play/)를 참고하세요.
## test

```console
$ lute test [<PATH>]… [--project <DIR>] [--providers <DIR>] [--permission-profile <NAME>]
            [--coverage] [--no-derive] [--deny <CODE>]… [--deny-warnings] [--json]
```

프로젝트의 시나리오 테스트를 실행합니다. `<PATH>` 피연산자는 테스트/플레이 스크립트를 고릅니다. 스크립트 파일(`*.test.yaml` 또는 expect가 있는 `*.play.yaml`)이거나, `*.test.yaml`과 기대값이 있는 `*.play.yaml`을 순회하는 디렉터리입니다. 피연산자가 없으면 프로젝트 디렉터리(`--project` 또는 발견된 프로젝트)를 순회합니다. 찾은 `*.test.yaml`은 선언된 모의 객체를 사용해 씬이나 퀘스트를 추적하거나, 로어 문서의 엔트리 또는 번들 비트 중 하나를 제시하고 선언된 기대값을 단언합니다. `expect:`(스텝 또는 최상위)가 있는 모든 `*.play.yaml`은 [`lute play`](#play)와 정확히 같은 방식으로 플레이한 뒤 자체 기대값으로 판정합니다(dsl 0.22.0). 그 밖의 파일이면 종료 코드가 **2**입니다. `--json`은 기계 판독 가능한 보고서를 출력하고, `--providers`는 스냅숏 디렉터리를 고정하며, 각 스크립트는 프로젝트 디렉터리 이하에서 가장 가까운 프로젝트에 대해 해석됩니다(발견된 프로젝트도 명시한 것과 똑같이 쓰이며, `--project`가 모든 스크립트를 단일 루트로 강제하지 않습니다). 해석은 문서 위쪽에서 가장 가까운 `lute.project.yaml`에 맞추며, 프로젝트마다 표준 오류에 한 번 이를 알립니다 — `lute: note: scenario tests use project . (nearest lute.project.yaml); pass --project to choose another`. 문서 위에 매니페스트가 없으면 추적은 코어만 사용합니다. `--permission-profile`은 프로젝트 해석에 전달됩니다. `--deny`/`--deny-warnings`는 테스트가 참조하는 모든 check 판정(추적한 문서마다의 check와 플레이마다의 compile 게이트)을 승격하므로 승격된 경고는 그 테스트를 실패시킵니다. `--no-derive`는 모든 테스트와 플레이에서 추론을 끄며 자체 `derive:` 키보다 우선합니다([trace](#trace) 참고). 모든 테스트와 플레이가 통과하면 종료 코드 **0**, 실패하면 **1**, I/O 오류 또는 잘못된 테스트 파일이면 **2**입니다.

각 테스트와 플레이는 파일 및 실행 내용을 명시한 `PASS`/`FAIL` 한 줄을 출력하고, 실패 뒤에는 누락 항목을 출력한 다음 요약을 출력합니다.

```console
$ lute test --project .
PASS  ./tests/lamp-quest.test.yaml  (./tests/../quests/lamp.lute)
PASS  ./tests/mara-first.test.yaml  (./tests/../scenes/talk/mara-first.lute)
FAIL  ./plays/first-day.play.yaml  (play of .)
      step 6 at townVisit: expect winner: expected town.welcome, actual town.morning

2 passed, 1 failed
```

`file:`이 존재하지 않는 문서를 가리키는 테스트는 `E-TEST-FILE`로 자체 실패하며, 나머지 테스트 모음은 계속 실행됩니다:

<!-- lute-diagnostics unverified="verbatim lute test output; crates/lute-cli/src/testcmd.rs writes the `error [E-TEST-FILE] ` prefix inside the same format! literal as the message, so the message alone matches no literal" -->
```console
FAIL  ./tests/old-shed.test.yaml  (./scenes/old-shed.lute)
        error [E-TEST-FILE] `file: ../scenes/old-shed.lute` names no document (./scenes/old-shed.lute does not exist)
```

모든 스텝과 최상위 기대값이 성립하고 플레이가 완료되면 플레이는 통과합니다. **중단된 플레이는 실패합니다**. 즉, 스크립트에 없는 선택이 발생했거나 참조 런타임이 `when`을 결정할 수 없는 경우입니다. 단, 최상위 `expect:`가 종료를 선언한 경우(`expect: { end: incomplete }`)는 예외이며, 실패에는 중단 이유가 표시됩니다. `--json`에서 각 `tests` 항목은 `"kind": "test"` 또는 `"kind": "play"`를 가지며, 플레이의 누락 항목은 `misses`에 `{ step, label, repetition, occasion, key, expected, actual }` 형태로 들어갑니다(`step`은 최상위 기대값에서 `null`).

실패한 테스트는 추적이 중단된 이유, 즉 미해결 가드와 이를 결정할 `state:`/`facts:` 항목을 보여 줍니다(`--json`에서는 `unresolved`, 각 항목의 `atoms`와 `supply`).

`lute test`는 각 프로젝트를 **한 번만** 로드하고 검사하며(dsl 0.26.0 §1), 그 프로젝트 아래의 모든 테스트와 플레이가 공유합니다. producer 집합과 퀘스트 id를 한 번 수집하고, 플레이 프로젝트는 모든 플레이에 대해 한 번 컴파일합니다(컴파일 진단도 플레이마다가 아니라 한 번만 출력). 그런 다음 테스트와 플레이를 모든 논리 코어에서 병렬 실행합니다(`RAYON_NUM_THREADS`로 스레드 수 제한). 결과는 일반적인 파일 순서로 보고되며 각 표준 오류도 같은 순서로 재생됩니다.

`--coverage`는 분기/arm 커버리지도 보고하고 **테스트되지 않은** 단위를 나열합니다. 프로젝트 디렉터리(`--project` 또는 발견된 프로젝트) 아래에서 측정하므로 `lute test tests --coverage`도 전체 프로젝트를 측정합니다. 단위는 파일이 아니라 비트입니다(0.27.0). 모든 번들 비트(`<document id>.<beat id>`)와 로어 엔트리는 각각 단위이고, 씬 또는 퀘스트 문서는 하나의 단위입니다. 컴포넌트는 테스트할 수 없으므로 나열되지 않습니다. 테스트가 단위를 제시하거나(씬 테스트는 씬을 추적하고 `beat:`, `entry:`, `entries:`는 해당 단위를 제시) 플레이가 제시하면 단위가 커버됩니다. 플레이는 `occasion:` 스텝을 통해, 또는 `advance:` 스텝 뒤 시계가 발생시키는 계기를 통해 제시합니다. branch/hub 행은 추적 경로가 선택한 옵션뿐 아니라 플레이가 선택한 옵션도 같은 `{file}:{id}` 행에서 셉니다. match arm과 `when=` guard 결과도 expect가 있는 플레이에서 source site별로 합쳐지므로 플레이만 실행한 arm도 실행됨으로 보고합니다. 마지막에는 플레이가 제시하지 않은 비트를 보여 주는 종료 증명 관점이 나옵니다. 씬 테스트는 모의 상태에서 비트를 추적할 뿐 실행 중 도달함을 증명하지 않지만, 플레이는 증명합니다:

```console
coverage over 2 traced path(s) and 1 play(s) (plays count toward what they presented, the choices they picked and the arms they executed):
  branch/hub maraAsk (scenes/talk/mara-first.lute:maraAsk): 2/2 chosen [lamp, leave]
  2 untested unit(s) under . — no *.test.yaml presents them and no play presents them:
    lore/endings/ren.lute: ember
    scenes/talk/mara-idle.lute
  1 more beat(s) no play presents (a test traces them; only a play proves they are reached in play):
    scenes/talk/mara-first.lute
```

경로는 `lute test`가 실행되는 디렉터리를 기준으로 합니다. 컴포넌트 `::use`가 확장한 `<match>`는 컴포넌트의 `file:line:column`에 해당 use를 명시하는 별도 행입니다(`components/card.lute:7:1 (card#1 in scenes/s.lute)`). `when=` 가드 행은 `match`가 아니라 `guard` 행이며 `taken` 및/또는 `skipped`로 표시됩니다.

`--json`은 동일한 내용을 `coverage`(`tracedPaths`, `plays`, `choices`, `arms`, `untested`, `notPresentedByPlay`) 아래에 담습니다. 각 `arms` 행은 `kind`가 `match` 또는 `guard`이고, 각 단위는 `{file, id, doc, local, kind}`입니다(`kind`는 `scene`, `quest`, `document`, `beat`, `entry` 중 하나). `id`는 프로젝트 전체 id입니다. 엔트리 id는 프로젝트 전역이고, 번들 비트 id는 `<document id>.<beat id>`이며, 씬 id는 그 키입니다. `doc`은 문서 id이고 `local`은 문서가 작성한 id입니다(전체 문서이면 `null`). `notPresentedByPlay`는 테스트가 추적했지만 플레이가 제시하지 않은 비트를 나열하며, 테스트되지 않은 비트는 `untested` 아래에 한 번만 나열됩니다.

A `*.test.yaml` 파일은 다음을 선언합니다:

```yaml
file: ../scenes/confrontation.lute   # the .lute under test, relative to THIS file (tests/ → ../scenes/…)
# optional mock surfaces — identical to `lute trace --mock`:
state:   { run.trueKiller: blake }
facts:   ["implicates(ledger, blake)"]
choose:  { accuse: accuseBlake }
events:  [npcSpoke]
accepts: [identifyKiller]
expect:
  transcriptContains: ["Case closed."]   # substrings that must appear in the transcript
  transcriptLacks: ["You let her go."]   # substrings that must NOT appear
  options: { accuse: [accuseBlake, accuseMoss] }  # the exact options offered at a branch/hub
  state: { run.accused: blake }          # path: literal assertions after the walk
  facts: ["points(blake)"]               # atoms that hold when the walk ends, after derivation
  notFacts: ["points(cass)"]             # atoms that do not hold then
  end: complete                         # complete | terminal | incomplete | error
```

`file:`은 필수이며 모든 모의 표면과 `expect:` 키는 선택 사항입니다. 모의 표면에는 `visited:`와 `occasions:`(dsl 0.21.0), 세이브에 시드하는 `quests:`와 `entriesRead:`(dsl 0.22.0), 그리고 [trace mock](#trace)과 동일한 `derive:`도 포함됩니다. `expect.transcriptContains`는 트랜스크립트에 반드시 나타나야 하는 부분 문자열을, `expect.transcriptLacks`는 나타나서는 안 되는 부분 문자열을 나열합니다. `expect.options`는 `<branch>`/`<hub>` id를 해당 추적이 제시한 정확한 옵션 집합에 매핑하며, 순서와 모든 제시를 무시합니다. 불일치 시 양쪽 집합을 명시하고, 제시되지 않은 branch는 그렇게 실패합니다. `expect.state`는 상태 경로를 추적이 끝난 뒤 그 경로가 가져야 할 리터럴에 매핑합니다 — 추적이 끝날 때의 값과 비교하며, trace가 읽는 방식대로 읽습니다: 추적의 마지막 쓰기([지급된 보상](/ko/tooling/tracing/#보상과-목표-본문) 포함), 없으면 테스트의 `state:` 시드, 그것도 없으면 선언된 `default:`입니다 — 그리고 `expect.end`는 추적이 어떻게 끝났는지(`complete`, 프로젝트의 `terminal:`이 끝에서 성립할 때의 `terminal`, `incomplete`, `error`)를 단언합니다. `expect.facts`와 `expect.notFacts`는 추적이 끝날 때 성립해야 하거나 성립하지 않아야 하는 ground atom을 나열합니다 — 추론 이후에 판정하므로 규칙의 결론을 직접 단언합니다. 불일치는 양쪽을 모두 밝히며(`facts points(blake): expected holds, got does not hold`), 도출 과정에서 미결정 상태를 읽은 atom은 `unknown`이 되어 어느 목록도 만족하지 않습니다.

0.24.0부터 테스트는 모의 객체의 `bridges:` 키도 받습니다([Bridge answers](/ko/tooling/tracing/#브리지-응답) 참고). 또한 세 가지 기대값이 확장되었습니다. `expect.accepts: [quest ids]`는 씬의 `::accept`가 수락한 퀘스트를 집합으로 단언하고(`accepts: expected [toll], got [parley]`), `<hub>`의 `expect.options`는 해당 방문 중 하나라도 선택 가능한 모든 선택지를 합집합으로 봅니다(기존에도 `[]`였습니다). `transcriptContains`/`transcriptLacks`는 실제 재생된 콘텐츠 줄만 `@speaker: text` 형식으로 비교합니다. 이는 플레이 스크립트가 비교하는 형식이므로 `"@narrator: Always shown."`을 양쪽에서 사용할 수 있고, 재생되지 않은 가드 줄은 더 이상 `transcriptContains`를 만족하지 않습니다.

속성 블록이 없는 needle은 전달 속성과 관계없이 줄과 일치합니다. dsl 0.27.0부터 needle 자체의 속성 블록도 판정합니다(0.26.0에서는 삭제됨). `@mara{emotion="content"}: You're new.` 형식의 needle 줄은 `emotion="content"`를 포함한 속성의 줄과만 일치합니다. 줄에 더 많은 속성이 있어도 됩니다. 따라서 플레이나 trace에서 복사한 줄은 같은 방식으로 말한 줄과 일치하고, 같은 단어를 다른 방식으로 말한 줄과는 일치하지 않습니다. 블록이 있는 `transcriptLacks` needle은 다른 속성으로 말한 경우 성립하므로, “절대 말하지 않음”을 뜻하려면 bare 형식(`"@mara: You're new."`)으로 씁니다. 블록의 키와 값은 먼저 검사됩니다. 트랜스크립트 줄에 없는 키(did-you-mean 포함), 도메인 밖의 값, 정수가 아닌 `variant`, 전달 플래그의 값은 테스트를 잘못된 것으로 만들며(`E-TEST-NEEDLE`, `E-TEST-KEY`처럼 `FAIL`로 보고됨) `transcriptContains`와 `transcriptLacks` 모두에 적용됩니다. `transcriptContains` 누락은 needle에 가장 가까운 제시 줄을 실제 발화 형태로 인용합니다: `` transcriptContains "@mara: Tomas keeps the oil. Ask her.": absent (nearest line: "@mara{emotion=\"delighted\"}: Would you? Tomas keeps the oil. Ask him.") (expected present) ``. 0.27.0부터 가장 가까운 줄은 먼저 needle과 같은 화자의 줄이고, 그다음 needle이 그 안에 나타나기 위해 필요한 편집이 가장 적은 줄이므로, 짧은 줄이 더 이상 긴 needle에 "가깝다"고 판정되지 않습니다(round-5 T3-16).

**불완전한 추적은 실패합니다.** 알 수 없는 가드 때문에 추적이 중단되면 그 뒤의 기대값은 실행되지 않았으므로, `expect: { end: incomplete }`를 선언하지 않는 한 다른 단언이 무엇이든 테스트는 실패합니다. 추론은 켜져 있으므로([trace](#trace)) 규칙으로 도출된 사실은 테스트의 `facts:`와 프로젝트 시드 사실에서 따라오며 결론을 모의할 필요가 없습니다. **0.21에서 마이그레이션:** 모의하지 않은 도출 atom이 unknown이어서 `incomplete`이 되거나 시드 관계가 비어 있다고 전제한 테스트는 이제 도출 또는 시드된 답을 봅니다. 이전 판정을 유지하려면 `derive: false`를 고정합니다.

**`end: complete`은 문서의 끝을 뜻합니다**(dsl 0.26.0 §7). 플레이와 마찬가지로 추적은 실행된 `::jump`를 `::label`까지 따라가므로, jump에서 멈춘 것이 아니라 실제 끝에 도달했을 때만 `end: complete`이 성립합니다. 트랜스크립트와 상태 기대값도 플레이와 같은 것을 봅니다([실행된 `::jump` 따라가기](/ko/tooling/tracing/#선택된-jump-따라가기) 참고). 퀘스트 `<on>` 핸들러의 `::accept`도 같은 문서의 퀘스트에 적용하므로 `expect: { quests: { second: active } }`가 플레이에서처럼 성립합니다. `accepts:`는 프로젝트 전체에서 퀘스트를 해석하며, 다른 문서가 선언한 퀘스트도 포함합니다. 퀘스트 문서는 자신의 `quest.<id>.*`를 시드할 수 있으므로 `quests/lamp.lute` 테스트의 `quests: { lampOut: active }`가 해당 퀘스트를 시작합니다. 0.26.0 이전에는 이 각각이 `lute play`와 다르게 동작했습니다.

`file:`은 무엇을 제시할지 테스트가 지정하면 로어 문서를 가리킬 수 있습니다. `entry: <id>` 또는 `entries: [ids]`로 여러 엔트리를 순서대로 제시하며, 그 사이에 읽기 플래그를 설정합니다. 같은 id를 반복하면 첫 읽기 효과를 건너뛰는 재읽기입니다. 또는 `beat: <id>`로 하나의 [번들 비트](/ko/tooling/tracing/#번들-비트)를 bare 또는 표준 `<document id>.<beat id>`로 지정할 수 있으며 `lute trace --beat`와 같이 걷습니다. 테스트는 `beat:`와 `entry:`/`entries:`를 동시에 가질 수 없고(종료 **2**), 아무것도 지정하지 않은 로어 테스트는 선언된 entry id와 beat id를 나열하는 `E-TEST-LORE`가 됩니다. 두 엔트리를 순서대로 읽는 예시는 다음과 같습니다:

```yaml
file: ../lore/tomas.lute
entries: [tomasOil, tomasBusy]
quests: { lampOut: active }            # seeds quest.lampOut.state, which tomasOil's `when` reads
expect:
  transcriptContains: ["Top shelf.", "Busy."]
```

엔트리는 `entry:`, `entries:` 및 `expect.eligible` 키에서 `<document id>.<entry id>`로 이름을 지정할 수 있습니다(`entries: [lore.tomas.tomasBusy]`, dsl 0.26.0 §8). [전체 종류](/ko/tooling/play/#종류-대상)를 대상으로 하는 번들 비트는 `state: { occasion.target: r16Gus }` 시드에서 해당 멤버를 읽으며, 종류로 타입이 지정됩니다. 이는 플레이 스텝의 `target`이 바인딩하는 방식과 같습니다.

Trace는 `when`의 성립 여부와 관계없이 엔트리나 비트를 제시합니다. `when`은 엔진의 게이트로 표시되지만 강제되지는 않습니다. `lute test`는 이를 강제합니다(dsl 0.26.0 §7). 테스트가 엔트리, 번들 비트 또는 씬 비트를 제시했는데 모의값으로 자격이 없으면 — `when`이 거짓이거나 `after:`가 성립하지 않거나 `once: user`가 소진되었거나 엔트리의 `once="run"`/`once="user"`가 소진된 경우 — `expect.eligible`을 단언하지 않는 한 **실패합니다**. 엔진은 이를 제시하지 않으므로 해당 추적은 플레이에 관해 아무것도 증명하지 못하기 때문입니다. 엔트리의 `once`는 읽기 플래그로 소진되며, 모의 `entriesRead:`가 이를 시드하고(`run:`은 `entry.<id>.read`와 `everRead`를 설정하고 `user:`는 `everRead`만 설정) 같은 테스트의 앞선 제시도 설정합니다. 따라서 실패는 어느 쪽인지 밝힙니다: ``it is `once="run"` and already spent — the mocked `entriesRead: { run: [<id>] }` read it (`entry.<id>.read`)``. `quests:` 시드가 없으면 `quest.lampOut.state`는 `unset`으로 읽히고 Tomas의 기름 엔트리는 자격이 없습니다:

```yaml
# tests/tomas-oil.test.yaml
file: ../lore/tomas.lute
entry: tomasOil
expect:
  transcriptContains: ["Top shelf."]
```

```console
FAIL  ./tests/tomas-oil.test.yaml  (./tests/../lore/tomas.lute)
      eligible tomasOil: not eligible under these mocks (its `when` is false) — the engine would never present it, so the walk proves nothing about play; fix the mocks, or assert `expect: { eligible: { tomasOil: false } }` (the body is then not walked)
```

실패는 거짓인 전제와, 존재한다면 이를 성립시키는 모의를 명시합니다. 예를 들어 ``its `when` (user.bond.mara >= 1) is false``, ``its `after: visited('town.welcome')` is false — mock `visited: [town.welcome]` ``, 또는 ``it is `once: user` and the mocked `visited:` already lists it``와 같습니다. 0.26.0 이전에는 이런 테스트가 안내와 함께 통과했으므로, 다른 작성자의 씬을 계약 테스트하는 동안 그 씬이 더 이상 제시될 수 없게 되어도 녹색으로 남았습니다.

`expect.eligible: { id: true }`가 실패하면 같은 전제를 명시합니다(round-5 T3-12). 번들 비트의 `after=`에는 ``eligible ember: expected true, got false — its `after="visited('ren.confession')"` is false — mock `visited: [ren.confession]` ``, 엔트리 또는 비트에는 ``… — its `when` (run.fish == 1) is false``가 표시됩니다. Trace와 test는 `lute play`와 `lute calendar`가 선택하는 것과 같은 하나의 규칙, 즉 `once`, `after`, `spentBy`, `when` 순서로 모든 씬, 엔트리, 번들 비트를 판정합니다.

`expect.eligible`은 테스트가 제시하는 모든 엔트리와 비트에 대해 `true` 또는 `false` 판정을 단언하거나 id에서 판정으로 매핑합니다(`eligible: { tomasOil: false }`; 비트는 bare id로도 지정 가능). `false` 판정은 닫혀야 하는 전제를 지정할 수 있습니다(`eligible: { mara.first: { false: once } }`). 가능한 전제는 `once`, `after`, `spentBy`, `when`, `gate`(계기의 `raisedWhen:`), `terminal` 중 하나입니다. 그러면 비트가 다른 이유로 닫혔을 때 테스트가 실패합니다(`eligible mara.first: expected false (when), got false (once)`). 지정된 경우 자격 없는 본문은 실행되지 않습니다. `eligible: false` 테스트에는 엔진이 실행하지 않는 본문의 브리지 응답이 필요하지 않으며, 해당 본문에 대한 트랜스크립트 기대값은 부재로 실패합니다. dsl 0.26.0부터 씬 비트(`on:`)에도 적용되며 씬의 `when`, `after:`, `once`로 판정합니다. 맵 키는 테스트가 제시하지 않은 파일의 엔트리 또는 번들 비트도 지정할 수 있습니다(dsl 0.24.0). 모의값 아래에서 단독으로 판정되므로, 로어 테스트가 아무것도 제시하지 않고 맵 형식 `eligible:`만 가질 수도 있습니다. 모의값이 결정하지 못한 `when`은 어느 판정과도 일치하지 않습니다. 누락은 `eligible tomasOil: expected true, got false`로 표시됩니다.

`expect.quests`(dsl 0.21.0 §7a.4)는 퀘스트 문서의 수명 주기 결과, 즉 각 퀘스트가 추적을 끝낸 상태를 직접 단언합니다. 상태는 `unset`, `active`, `complete`, `failed` 중 하나입니다:

```yaml
file: ../quests/hold.lute
visited: [haven.shed]
occasions: [runEnd]
expect:
  quests: { holdLine: complete, sideJob: unset }
```

불일치는 `quests holdLine: expected "complete", got "active"`로 실패합니다. 네 상태 밖의 값이나 추적 문서가 선언하지 않은 퀘스트 id도 이름을 명시한 메시지와 함께 테스트를 실패시킵니다. 최상위 `quests:` 키는 추적이 **시작하는** 시드이고, `expect.quests`는 추적이 **끝내야 하는** 결과입니다.

## lint

```console
$ lute lint [<PATH>] [--project <DIR>] [--providers <DIR>] [--deny <CODE>]… [--deny-warnings]
            [--config <FILE>] [--json]
```

권고 콘텐츠 lint를 파일 또는 디렉터리 트리(기본값: 현재 디렉터리)에 실행합니다. 검사 항목은 줄 길이, 대사 비율, 감정 연속, 누락된 에셋, 프로젝트 로컬 규칙입니다. 선형 VN 규범(`L-SECTION-STARTS-WITH-BACKGROUND`, `L-DIALOGUE-RATIO`, `L-SCENE-LENGTH-SPREAD`)은 선형 씬만 판정하며 비트, 컴포넌트, 퀘스트, 로어는 판정하지 않습니다. 문서는 가장 가까운 `lute.project.yaml`별로 그룹화되고, 각 프로젝트의 `lute.lint.yaml`(또는 `--config <FILE>`)이 규칙 수준, 임계값, 무시 glob, `custom:` 규칙을 정합니다. 결과는 `lute check`와 별개의 `L-*` 코드이며 lint는 capability snapshot에 들어가거나 아티팩트를 변경하지 않습니다. `--deny`/`--deny-warnings`는 `check`와 같은 방식으로 결과를 승격합니다. 하위 오류 결과만 있거나 깨끗하면 종료 **0**, 오류 심각도 결과(`E-LINT-CONFIG`/`E-LINT-EXPR` 포함)가 있으면 **1**, I/O, 잘못된 YAML 또는 사용법 오류면 **2**입니다. 규칙, 메트릭, 설정 형식은 [Linting](/ko/tooling/linting/)을 참고하세요. `--project <P>`를 주면 `<PATH>`가 없을 때 `<P>`를 검사하고, 각 문서는 `<P>` 이하에서 가장 가까운 프로젝트에 대해 해석되며(`<P>`는 다른 디렉터리 명령과 같이 순회 바닥), 각 루트의 `lute.lint.yaml`이 적용됩니다(`--config`가 있으면 그것).

0.26.0부터 lint는 `W-DISPLAY-NAME-DUP`도 보고하며 권고 전체 프로젝트 `check`도 보고합니다. 대화 상자가 같은 이름으로 표시할 두 화자, 즉 동일한 `name:`을 가진 두 cast 항목, `::use{… name="…"}` 표시 문자열과 같은 cast 이름, 또는 서로 다른 화자의 두 문자열(`who=`)이 대상입니다. `sharedName: true`로 표시된 cast 항목은 여러 화자가 공유하는 의도된 역할 이름이므로 집계하지 않습니다. 이 코드는 `lute.lint.yaml` 규칙이 아니며 `--deny W-DISPLAY-NAME-DUP`로 오류로 만들 수 있습니다. [Linting](/ko/tooling/linting/#표시-이름)을 참고하세요.

## loc export

```console
$ lute loc export [--project <DIR>] [--format csv] [-o, --output <PATH>]
```

프로젝트 전체에서 번역 가능한 모든 콘텐츠 줄(안정적인 `code`, 화자, 텍스트, 선택지 텍스트, branch/hub 프롬프트)을 추출해 로컬라이제이션 export로 만듭니다. export된 `text`는 소스 마크업을 유지합니다. 인라인 modifier(`:pause{s=0.5}`, `:speed[…]{rate=…}`, 텍스트 스타일 span)와 `{{…}}` interpolation을 포함하며, 번역은 같은 modifier 집합을 유지해야 합니다. JSON이 기본이며 `--format csv`가 CSV를 고릅니다(`--format json`은 `E-CLI-USAGE`이니 생략하세요). `-o, --output`은 stdout 대신 파일에 씁니다. `loc export`는 산출물을 쓰는 명령이므로 `--json`이 없습니다. 성공 시 종료 **0**, I/O 오류 시 **2**입니다.

각 행에는 컴파일러가 해당 레코드에 기록할 `lineId`도 들어가며, `loc import`와 `compile --locale-bundle`이 조인 키로 사용합니다. 작성된 `code`가 없는 줄은 JSON에서 `null`, CSV에서 빈 값입니다. 이 id는 확장 후 명령 스트림에서 컴파일러가 보완하며 소스만 걷는 것으로는 재현할 수 없습니다. 먼저 `lute tag`를 실행하면 stderr의 `N lines untagged — run lute tag` 안내가 사라집니다.

행 종류는 `line`, `choice`(선택지 라벨, `key`는 `{branchOrHubId}.{choiceId}`), `prompt`(branch/hub의 `prompt=`, `key`는 `{branchOrHubId}.prompt`, lineId는 `{prefix}.{branchOrHubId}.prompt`)이며 문서 순서를 따릅니다. 프롬프트 행은 그 branch/hub의 선택지보다 앞에 옵니다. 모든 행에는 `sourceHash`가 있습니다. 컴파일러가 내보내는 레코드의 **평문** 원문 — 인라인 수정자와 이스케이프를 제거한 대사 텍스트, 선택지 텍스트, 프롬프트 — 의 `"sha256:<hex>"`입니다. 번역자는 이 값을 바꾸지 않습니다. `loc import`가 이를 저장해 두므로 `compile --locale-bundle`은 그 뒤 원문이 바뀌었는지 알 수 있습니다(`W-L10N-STALE`). CSV에서는 마지막 열입니다.

```text
kind,file,line,lineId,code,speaker,key,text,sourceFile,sourceLine,sourceHash
```

## loc import

```console
$ lute loc import <FILE>… [-o, --output <PATH>] [--json]
```

번역된 `loc export` 파일을 하나의 **로케일 번들**로 정규화합니다. 이는 반대 방향의 작업이며 `lute compile --locale-bundle`이 소비합니다. `--json`은 보고서 `0.48.0.loc-import`(`{output, locales, lines, skipped, diagnostics: [{path, code, message}]}`)를 출력하며 `--output`이 필요합니다. 성공 시 **0**, `E-LOCALE-BUNDLE`이면 **1**, I/O 오류면 **2**입니다.

입력은 `export`가 출력하는 형식 그대로이며 두 형식을 모두 지원합니다(`.csv` → CSV, 그 밖의 파일 → JSON). `export`는 소스 언어를 추출하므로 로케일을 담지 않습니다. 일반적인 작업 흐름은 **로케일마다 파일 하나**를 두는 것입니다. export를 `ja-JP.json`으로 복사하고 `text` 값을 번역하면 파일 **stem**이 로케일 태그가 됩니다. 행 자체에 비어 있지 않은 `locale` 필드(JSON) 또는 `locale` 열(CSV)이 있으면 이를 우선하므로, 모든 로케일을 합친 단일 파일도 사용할 수 있습니다.

```json
{
  "schemaVersion": 2,
  "locales": ["en-US", "ja-JP"],
  "entries": {
    "marina.s01ep02.marina_0010": { "en-US": "Hello there.", "ja-JP": "こんにちは。" }
  },
  "sourceHashes": {
    "marina.s01ep02.marina_0010": {
      "en-US": "sha256:23ea498e82f4…",
      "ja-JP": "sha256:23ea498e82f4…"
    }
  }
}
```

(위 해시는 줄여 표기했습니다.) `sourceHashes`는 각 행의 `sourceHash`를 lineId **와** 로케일별로 담습니다(두 로케일이 서로 다른 export에서 번역될 수 있기 때문입니다). 항상 기록되며, 해시를 가진 행이 없으면 `{}`입니다. 모든 맵이 정렬되므로 번들은 바이트 안정적입니다. 같은 입력을 두 번 import하면 동일한 바이트가 생성됩니다. `compile --locale-bundle`은 `schemaVersion: 1` 번들도 읽습니다(`sourceHashes`가 없고 stale 검사도 하지 않습니다). 파싱할 수 없는 입력, 한 로케일 안에서 중복된 `lineId`, 빈 로케일 태그는 `E-LOCALE-BUNDLE`이며 문제가 있는 파일과 행을 함께 보고합니다. `lineId`가 **없는** 행은 거부하지 않고 건너뜁니다. 태그되지 않은 줄은 아직 안정적인 identity가 없기 때문이며, stderr의 단일 요약에 그 수가 집계됩니다.

## loc report

```console
$ lute loc report [--project <DIR>] [--json]
```

문서별 및 화자별 단어 수와 줄 수를 보고합니다. 같은 콘텐츠 줄을 대상으로 하는 제작 계획용 뷰입니다. 선택지 라벨은 개수(`choices`)만 세고 단어는 세지 않습니다. branch/hub 프롬프트는 개수(`prompts`, 0.47부터 — 보고서 버전 `0.47.0.loc-report`)를 세고, 그 단어는 문서 및 프로젝트 단어 합계에 포함됩니다(화자는 없습니다). `--json`은 사람이 읽는 표 대신 JSON으로 보고서를 출력합니다. 성공 시 **0**, I/O 오류 시 **2**입니다.

## --explain

```console
$ lute --explain <CODE>
```

진단 코드 하나를 설명합니다. `file:line:col: error [CODE] message`의 대괄호 안 `CODE`를 대소문자 구분 없이 사용하며(`e-set-shape`도 동작), 심각도, 발생 조건, 근거가 되는 사양 섹션, 모든 코드를 나열하는 [diagnostics reference](/ko/reference/diagnostics/)의 해당 항목 링크를 설명합니다. 알 수 없는 코드는 사용법 오류(종료 **2**)이고, 그 밖에는 종료 **0**입니다. 이는 서브커맨드가 아닌 최상위 플래그이며, 플레이 뒤 사실의 도출을 출력하는 [`lute play --derivation <ATOM>`](#play)과는 관련이 없습니다.

```
$ lute --explain E-LEGACY-CONTENT-SIGIL
E-LEGACY-CONTENT-SIGIL (error)

A content line uses the old `:` speaker sigil, which `@` replaced — write `@speaker{…}: text` instead.

Spec: dsl §7.1
More: https://lute-lang.vercel.app/reference/diagnostics/#e-legacy-content-sigil
```

## version

```console
$ lute version [--json]
```

서로 독립적인 세 가지 버전 축([versioning](https://github.com/journeyWorker/lute/blob/main/docs/versioning.md))을 출력합니다. **툴체인** 버전(이 CLI와 workspace crate), **언어** 버전(checker가 적용하는 문법/의미론), **IR** 스키마 버전(컴파일된 모든 아티팩트에 `irVersion`으로 기록됨)입니다. clap의 내장 `--version`과는 다릅니다. 내장 플래그는 툴체인 버전만 출력합니다. 언어 서버도 같은 플래그에 응답하며 `lute-lsp --version`은 `lute-lsp <version>`을 출력하고 [`doctor`](#doctor)가 이를 이 CLI와 비교합니다. `--json`은 `{"toolchain":…,"language":…,"ir":…}` 객체 하나를 출력하고, 사람용 모드는 레이블이 붙은 줄을 각각 출력합니다. 항상 종료 **0**입니다.

## 0.48에서 제거된 철자

0.48.0은 CLI 문법을 한 번에 바꿨습니다. 제거된 철자는 모두 `E-CLI-USAGE`(종료 **2**)로 응답하며, 메시지는 대체 철자를, 나머지 명령줄이 허락하는 경우에는 다시 쓴 전체 명령을 알려 줍니다(``lute check-project docs/x`` → ``lute check-project is now `lute check --project docs/x` ``). JSON 모드에서는 오류 envelope의 `argument`가 제거된 철자, `hint`가 대체 철자입니다. 이 응답은 한 마이너 릴리스 동안 유지됩니다. 제거된 명령은 명령이 아니므로 그 오류 envelope의 `command`는 `null`이고 telemetry도 그 명령의 id를 기록하지 않습니다.

| 제거된 철자 | 대체 |
|---|---|
| 명령 `check-project [<DIR>]` | `check --project <DIR>` |
| 명령 `compile-stream <FILE>` | `compile --stream <FILE>` |
| `compile --all` | `compile --project <DIR> -o <DIR>`(문서 없음) |
| `--out`(`compile`, `loc export`, `loc import`) | `-o, --output` |
| `compile --locales` | `--locale-bundle` |
| 프로젝트 디렉터리 피연산자(`impact`, `constraints`, `patch <DIR> <PATCH>`, `doctor`, `play`, `test`, `loc export`, `loc report`, `beats`, `calendar`, `scenario`, `lore`, `refs`)와 `check <DIR>` | `--project <DIR>` |
| `catalog refresh <DIR>` | `--providers <DIR>` |
| `new <KIND> <NAME> --dir <DIR>` | `--project <DIR>`(`new <kind> <NAME>` 자체는 그대로이며 종류는 하위 명령) |
| `impact <DIR> <TARGET>` | `--project <DIR> --node <TARGET>` |
| `context --target`, `--run` | `--node`, `--script` |
| `constraints --run` | `--all-scripts` |
| `scenario reach <NODE>`, `scenario envelope <NODE>`, `scenario knowledge --for` | `--node <NODE>` |
| `scenario --format json` | `--json` |
| `play --explain`, `--dump-conditions` | `--derivation`, `--conditions-output` |
| `run --dump-conditions` | `--conditions-output` |
| `trace --choose`, `--accept` | `--choice`, `--accepted-quest` |
| `calendar --facts`, `--csv`, `--occasion O@<AXES>` | `--relation`, `--format csv`, `--occasion-axes O=<AXES>` |
| `loc export --format json` | (생략: JSON이 기본) |
| `tag --force` | `--renumber --commit`(`--renumber`만 쓰면 미리 보기) |

`trace`, `beats`, `calendar`, `new`의 숨은 `--on` 거부와 `trace`의 숨은 `--target` 거부는 없어졌습니다. 이제 일반 알 수 없는 인자 오류입니다.

## `lute fmt` (0.36.6)

## `lute diff` (0.36.6)

## `lute patch` (0.36.6)

## `lute context` edit surface (0.36.6)

```console
$ lute fmt [<PATH>]… [--check] [--json]
$ lute diff <BEFORE> <AFTER> [--json]
$ lute patch <FILE> [--project <DIR>] [--providers <DIR>] [--permission-profile <NAME>] [--dry-run] [--json]
$ lute context <FILE> [--project <DIR>] --node <NODE> [--max-items <N>] [--script <FILE>]… [--all-scripts] [--json]
$ lute context <FILE> [--project <DIR>] --at <FILE>:<LINE>:<COLUMN> [--max-items <N>] [--script <FILE>]… [--all-scripts] [--json]
```
### `tag` 및 `diff`의 identity 마이그레이션 (0.36.6)

`lute tag [<PATH>]…`는 이제 누락된 줄 `code` 값과 누락된 컴포넌트 호출 키를 보완합니다. 작성된 모든 값은 보존하고, 각 owner와 컴포넌트 안에서 소스 순서대로 `instance="use-001"`, `use-002`, …를 기록하며 멱등적으로 동작합니다. `tag --renumber --commit`은 여전히 줄 코드만 다시 태그하며 명시적인 `instance`는 절대 다시 쓰지 않습니다.
`identity.requireStable: true`이면 태깅이 끝날 때까지 프로젝트가 태그되지 않은 줄과 `::use` 호출을 진단합니다. JSON diff 출력에서 작성된 `identity.renames` 원장은 `{kind:"renamed", node:<to>, from:<old>, to:<new>, before, after, locations:[old,new], declarationLocation:{file,span}}` 형태의 행을 만듭니다. 대응하는 원장 항목 없이 제거되거나 추가된 save 형태 identity에는 `unmappedIdentity: true`가 붙으며, Lute는 위치나 텍스트에서 rename을 추론하지 않습니다.

```lute check
---
kind: scene
id: cli-identity
---
## Opening
@narrator{code="intro"}: Ready.
```
`fmt`는 `<PATH>`(기본값 `.`) 아래의 `.lute`, project/schema YAML, 설정된 plugin YAML을 결정적 순서로 포맷합니다. YAML 값은 불투명하므로 임의의 YAML을 다시 포맷하지 않습니다. `--check`가 없으면 정규 텍스트를 쓰고, `--check`이면 아무것도 쓰지 않습니다. 선택한 파일이 다르면 1, 정규 형식이면 0, I/O·UTF-8·파싱 실패면 2로 종료합니다. `--json`은 보고서 `0.48.0.fmt`(`{applied, files: [{path, changed}]}`, 선택한 모든 파일)를 출력합니다.

`diff`는 디렉터리 또는 `git:<revision>` 양쪽을 비교하고 `schemaVersion: "0.36.0.diff"`, `before`, `after`, 정렬된 의미론적 `changes`를 출력합니다. 포맷만 달라진 경우 빈 배열입니다. 다른 쪽이 현재 git 작업 트리 안의 디렉터리이면 `git:` 쪽은 그 하위 트리만 archive하고(`git archive <rev> -- <그 경로>`) 그곳을 루트로 삼습니다. 그래서 저장소 루트에서 `lute diff git:HEAD story`를 실행하면 `story/` 프로젝트끼리 비교하고, 저장소의 다른 곳에 있는 심볼릭 링크는 상관없습니다(비교하는 트리 안의 심볼릭 링크는 여전히 거부됩니다. 디렉터리 이름은 글자 그대로의 경로이며, 그 revision에 없는 디렉터리는 디렉터리와 revision을 함께 알려 주는 `E-CLI-IO`입니다). 그 밖의 경우(다른 쪽이 저장소 밖에 있거나 양쪽이 모두 `git:`)에는 현재 디렉터리의 트리를 archive합니다(0.47). 비교가 생성되면 종료 0, 입력·materialization·파싱·I/O 오류면 종료 2입니다.

`patch`의 피연산자는 patch 요청 파일이며 프로젝트는 `--project`이거나 현재 디렉터리에서의 탐색입니다. 프로젝트 피연산자를 하나 더 주면 `--project`를 알려 주는 `E-CLI-USAGE`입니다. `--providers`와 `--permission-profile`은 `apply_patch`로 전달됩니다. `patch`는 [patch 요청](/ko/tooling/ai-harness/#patch-요청-047)에 문서화된 엄격한 요청 형식(필드, 연산, preserve 키, 노드 키 종류, 예시)을 받으며, `lute schema patch`가 이를 `inputs`로 설명합니다. 연산은 `replaceNode`, `insertBefore`, `insertAfter`, `replaceAttr`, `removeNode`, `createFile`, `moveFile`, revision 보호형 `replaceText`입니다. 보존 주장에는 `ids`, 위치 기반 `lineIds`, `voiceKeys`, `choiceEffects`, `rewards`, `conditions`, `reachability`, `hostContracts`, `constraints`가 포함됩니다. patch 보고서는 `schemaVersion: "0.43.0.patch"`와 `before`, `after`, `diff`, `writes`를 유지합니다. `accepted`는 patch 검증 통과 여부를 나타내고, envelope의 `ok`는 종료 코드 규칙을 따릅니다. 따라서 의미가 바뀐 승인 patch는 `accepted: true`, `ok: false`, 종료 **1**(검토용 diff 포함)이고, 의미를 보존한 patch는 `accepted: true`, `ok: true`, 종료 **0**입니다. `--dry-run`도 쓰기 없이 같은 판정을 사용합니다. 거부는 **2**로 종료하며 기존 `E-PATCH-STALE`, `E-PATCH-TARGET`, `E-PATCH-EDIT`, `E-PATCH-FORMAT`, `E-PATCH-CHECK`, `E-PATCH-PRESERVE` 코드를 유지합니다. 읽을 수 없는 patch 파일은 JSON error envelope을 사용합니다. 한 파일의 여러 편집은 원래 텍스트를 기준으로 해석되고, 편집이 닿은 줄만 포맷합니다. `lute fmt --check`를 통과하던 `.lute` 파일은 계속 통과하고, 그 밖의 파일에서는 닿은 줄이 정규 형식이 됩니다. 이를 깨뜨릴 patch는 파일 이름을 담은 `E-PATCH-FORMAT`으로 거부되며 아무것도 쓰지 않습니다(0.47).

`context`는 `schemaVersion`, `projectRevision`, `files`, `target`, `declared`, `references`, `affected`, `tests`, `plays`, `vocabulary`, `notIncluded`를 반환합니다. 경계는 조용히 버리지 않고 보고합니다. `--script`(반복 가능)에는 스크립트 파일(또는 그 아래의 모든 스크립트를 고르는 디렉터리)이 필요하며 실행된 증거를 `witnessed`로 표시하고, `--all-scripts`는 프로젝트의 모든 play/test 스크립트를 실행합니다. 둘은 함께 쓸 수 없습니다(`E-CLI-USAGE`). `--node <NODE>`는 편집 표면(보고서 `0.36.0.context`, 출력은 `--json --node`)을 고릅니다.
