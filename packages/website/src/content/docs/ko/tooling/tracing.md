---
title: 트레이싱 가이드
description: "출시 전에 씬 미리 보기 — 상태, 팩트, 선택지, 이벤트, 수락, 방문 집합, 세이브의 퀘스트 상태와 엔트리 읽기, 발생한 계기(대상 지정 포함), 이전 런, 플래그 또는 mock YAML 파일을 통한 플러그인 브리지 응답, 크레딧이 있는 보상과 목표 본문, 퀘스트 구조, 번들 비트 하나 또는 종류 비트의 멤버 제시, 프로젝트 규칙이 그 위에서 파생하는 방식, 결정 트랜스크립트 읽기(작성된 그대로의 def 참조 또는 `--expand` 확장, 선택된 `::jump`를 라벨까지 따라가기), E-TRACE-* 거부를 다룹니다."
---
## 기계 판독 출력 (0.43)

`lute trace --json`은 `{ "command", "schemaVersion", "ok", ... }` 하나의 envelope를 반환합니다. trace가 거부되면 `ok: false`와 구조화된 `error`가 나오며, stdout을 파싱하기 전에 종료 코드를 처리해야 합니다. `--events`는 NDJSON입니다.

`lute trace`는 **작성자가 제공한 mock**에 대해 문서 하나를 결정적으로 한 번 걸으며 모든 결정과 그 이유를 보고합니다. 이것은 작성 중 미리 보는 기능이지 보장이 아닙니다. `check`/`compile`에 절대로 입력하지 않으며 정적 도달 가능성 증명도 아닙니다. 사용자가 제공한 mock 시나리오만 탐색하므로, 증명이 아니라 커버리지 보조 수단입니다. 0.22.0부터는 기본적으로 프로젝트의 seed 팩트와 Datalog 규칙을 이 mock 위에 적용하며, `lute run`과 `lute play`가 하는 일과 정확히 같습니다 — [파생](#파생)을 참고하세요. dsl 0.26.0부터는 선택된 `::jump`를 그 `::label`까지 따라가고, 퀘스트 `<on>` 핸들러의 `::accept`도 `play`처럼 적용합니다. `lute test`는 mock 때문에 자격이 없어진 비트를 제시하는 테스트를 실패시킵니다 — [선택된 `::jump` 따라가기](#선택된-jump-따라가기)와 [`lute test`](/ko/tooling/cli/#test)를 참고하세요.

## 월드 시드하기

Trace는 제공한 월드를 걷습니다. 시드하지 않은 상태 경로는 선언된 기본값을 읽고(기본값이 없으면 unknown), 팩트는 mock, 스키마의 seed `facts:`, 그리고 프로젝트 규칙이 그로부터 파생한 모든 것입니다. 반복 가능한 플래그 다섯 종류가 이 walk에 값을 공급합니다.

```console
$ lute trace scene.lute \
    --state run.metMira=true \
    --fact "inParty(shadowheart)" \
    --choice sofaHelp=help \
    --event npcSpoke \
    --accepted-quest sideQuest
```

- `--state <path>=<literal>` — 선언된 경로에 대한 스칼라 시드입니다.
- `--fact "<rel>(<arg>…)"` — 선언된 어휘 위에 놓인 ground 팩트로, 현재 유효합니다(명시한 답이므로 `derive:`/`reserved:` 관계를 가리킬 수도 있음).
- `--choice <id>=<choiceId>[,<choiceId>…]` — `<branch>`/`<hub>` id에서 메뉴를 선택합니다. 하나의 플래그에서 쉼표로 나열하면 허브 전체의 순서 있는 방문 시퀀스를 강제할 수 있습니다. 목록이 끝나기 전에 허브가 종료되지 않으면 `lute play`와 같이 walk가 미완료로 멈추고(종료 3), 목록을 이름으로 표시합니다.
- `--event <name>` — capability/world 이벤트를 CLI 순서대로 발생시킵니다.
- `--accepted-quest <questId>` — `start`가 없는(수락으로 구동되는) 퀘스트를 수락하는 일을 시뮬레이션합니다. 프로젝트가 있으면 dsl 0.26.0부터 추적 문서가 선언한 퀘스트뿐 아니라 프로젝트의 모든 퀘스트를 지정할 수 있습니다. 어떤 퀘스트도 선언하지 않은 id는 `E-TRACE-ACCEPT`입니다.

퀘스트와 씬 조건에는 두 가지 입력원이 더 있습니다(dsl 0.21.0).

- `--occasion <OCCASION>` — walk가 정착한 뒤 CLI 순서대로 계기를 발생시킵니다(반복 가능하며 mock의 `occasions:` 뒤에 옵니다). 각 발생은 먼저 활성 퀘스트의 `<on event="<name>">` 핸들러를 모두 실행하고 — 계기는 `lute play`와 `lute run`처럼 같은 이름의 월드 이벤트도 발생시킵니다 — 그다음 모든 활성 퀘스트의 `<objective on="<name>">` 목표를 판정합니다. 따라서 목표는 핸들러가 쓴 내용을 읽을 수 있으며, `on` 목표는 그 밖의 경우에는 절대 판정하지 않습니다. `--occasion <OCCASION>@<target>`(dsl 0.23.0)은 대상에게 계기를 발생시킵니다 — [대상, 기한 및 이전 런](#대상-기한-및-이전-런)을 참고하세요.
- `visited: [<scene id>…]` (mock 파일만 해당) — 이미 제시된 씬입니다. 어떤 조건에서든 `visited('<scene id>')`가 읽으며, 목록에 없는 씬은 방문되지 않은 것입니다. 집합이 닫혀 있으므로 `visited(…)` 읽기는 항상 `true` 또는 `false`이고 미해결이 아닙니다.
- `bridges: { <tag>: [ {<field>: value}, … ] }` (mock 또는 test만 해당, dsl 0.24.0 §5) — 브리지 결과를 읽는 플러그인 호출의 응답으로, 해당 태그 호출마다 호출 순서대로 하나씩 둡니다. [브리지 응답](#브리지-응답)을 참고하세요.

같은 입력원은 `--mock <file.yaml>` 문서에도 있으며, CLI 플래그와 mock을 함께 쓸 수 있고 충돌하면 플래그가 우선합니다.

```yaml
state:  { run.metMira: true }
facts:  ["inParty(shadowheart)"]
choose: { sofaHelp: help }
events: [npcSpoke]
accepts: [sideQuest]
visited: [haven.shed]
occasions: [runEnd]
```

두 가지 메모는 계기 불일치를 지적하지만 walk를 거부하지는 않습니다. 하나는 walk가 발생시키지 않은 계기에서 판정되는 목표입니다(``objective `q.o` is judged at occasion `runEnd`, which this walk never raised (supply `--occasion runEnd` or `occasions: [runEnd]`)``). 다른 하나는 발생했지만 문서에서 응답하는 것이 없는 계기입니다(``occasion `dayEnd` is judged by no `<objective on>` and fires no `<on event>` handler in this document``).

### 세이브의 이력

mock 키가 두 가지 더 있습니다(dsl 0.22.0, mock 파일만 해당). 플레이어의 세이브가 이미 보유한 것을 시드하며, [플레이 스크립트가 시작하는 방식](/ko/tooling/play/#세이브에서-시작하기)과 같은 키입니다.

- `quests: { <questId>: unset | active | complete | failed }` — 퀘스트 수명 주기 상태이며, `quest.<id>.state` 상태 시드와 정확히 같습니다.
- `entriesRead: { run: [<entry id>…], user: [<entry id>…] }` — `run`은 `entry.<id>.read: true`와 `entry.<id>.everRead: true`를 시드합니다(이번 런에 읽었으므로 ever에도 읽힘). `user`는 `entry.<id>.everRead: true`만 시드합니다(이전 런에 읽었으므로 새 런에서 `read`는 지워지고 `everRead`는 유지됨). `once="run"`/`once="user"`로 소진된 엔트리는 자격이 없습니다. trace는 ``not eligible (`once="run"` is already spent)``라고 표시하며, `expect.eligible`을 단언하지 않으면 이를 제시하는 테스트는 실패합니다.

둘 다 예약 상태 경로를 표기하는 방식이므로 해당 경로의 mock 규칙을 따릅니다. 추적 문서는 그 경로를 읽거나 선언해야 합니다. 문서가 읽지도 선언하지도 않은 퀘스트를 시드하거나, 읽지도 선언하지도 않은 엔트리를 시드하면 `E-TRACE-MOCK-UNDECLARED`이고, 네 상태 밖의 상태는 `E-TRACE-MOCK-PARSE`입니다. dsl 0.26.0부터 퀘스트 문서는 자신이 선언한 모든 퀘스트의 `quest.<id>.*` 경로를 읽는 것으로 간주하므로, 자신의 테스트에서 `quests: { lampOut: active }`를 시드할 수 있고 walk가 그 퀘스트에서 시작합니다. [`beats` 스캐폴드](/ko/tooling/play/#예제)에서 Tomas의 oil 엔트리는 lamp 퀘스트가 active인지를 가드합니다.

```yaml
# mocks/oil.yaml
quests: { lampOut: active }
```

```console
$ lute trace lore/tomas.lute --project . --entry tomasOil --mock mocks/oil.yaml
trace: lore/tomas.lute  (seeds: 1 paths, 0 facts; 0 selections)
  <entry tomasOil>   (first read)
    @tomas  Oil? Top shelf. Tell Mara it's the wick, not the oil.
    ::assert  knows(lamp)
trace complete: 0 decisions
```

mock가 없으면 엔트리는 ``(first read, not eligible: its `when` (quest.lampOut.state == 'active') is false (`quest.lampOut.state` is unset))``로 읽히고, 엔진이 발생시키지 않을 비트는 대신 그렇게 표시됩니다(occasion의 `raisedWhen`이 false이거나 프로젝트의 `terminal:`이 성립함). 또한 같은 `quests:`를 퀘스트 `lampOut`의 상태를 전혀 읽지 않는 씬에 쓰면 `E-TRACE-MOCK-UNDECLARED`입니다. `--project`(또는 파일 위 매니페스트)가 있으면 trace는 문서가 읽는 퀘스트가 존재하는지 정착합니다(dsl 0.24.0). 프로젝트가 선언한 퀘스트는 메모가 없고, 선언하지 않은 퀘스트는 did-you-mean과 함께 ``quest `lampOot` is declared by no quest document of the project — did you mean `lampOut`? (every read of `quest.lampOot.*` takes its reserved default)``라고 합니다. 프로젝트가 없는 trace만 존재 여부를 확인할 수 없다고 메모합니다.

## 대상, 기한 및 이전 런

세 가지 dsl 0.23.0 추가 기능은 같은 입력원을 통해 trace에 도달합니다.

**대상에게 발생하기.** `target=`이 있는 목표(`<objective on="talk" target="npc.oskar">`)는 `<occasion>@<target>`으로 적은 대상 지정 발생에서만 판정됩니다 — `--occasion talk@npc.oskar`, 또는 mock의 `occasions: [talk@npc.oskar]`입니다. 일반 `talk`이나 `talk@npc.maud`는 이를 그대로 두며, walk는 어느 발생을 기다리는지 말합니다. [스토리 플레이](/ko/tooling/play/#기한과-대상-지정-목표)의 탑 사냥 퀘스트에서 처치를 mock한 경우:

```console
$ lute trace quests/hound.lute --project . --accepted-quest houndHunt --fact "slew(hound)" --occasion talk@npc.maud
trace: quests/hound.lute  (seeds: 0 paths, 1 facts; 0 selections)
note: objective `houndHunt.report` is judged at occasion `talk` for `npc.oskar`, which this walk never raised (supply `--occasion talk@npc.oskar` or `occasions: [talk@npc.oskar]`)
  <quest houndHunt>   -> active (forced)
  <objective collar>   -> done (holds('slew', ['hound']))
trace complete: 2 decisions
$ lute trace quests/hound.lute --project . --accepted-quest houndHunt --fact "slew(hound)" --occasion talk@npc.oskar
trace: quests/hound.lute  (seeds: 0 paths, 1 facts; 0 selections)
  <quest houndHunt>   -> active (forced)
  <objective collar>   -> done (holds('slew', ['hound']))
  <objective report>   -> done (holds('slew', ['hound']))
  <quest houndHunt>   -> complete
    grant houndHunt  EMBERS 50 (credits user.embers = 50)
trace complete: 4 decisions
```

**기한.** 목표의 `by=`는 `done` 판정 뒤 모든 settle에서 판정됩니다. 목표가 아직 done이 아닐 때 처음 성립하는 순간 목표는 `failed`가 되고 기한을 표시하며, 다시는 판정하지 않습니다. 필수 목표라면 퀘스트도 실패합니다. 처치 없이 4층에 도달한 경우:

```console
$ lute trace quests/hound.lute --project . --accepted-quest houndHunt --state run.floor=4
trace: quests/hound.lute  (seeds: 1 paths, 0 facts; 0 selections)
  <quest houndHunt>   -> active (forced)
  <objective collar>   -> pending (holds('slew', ['hound']))
  <objective collar>   -> failed (run.floor >= 4)
  <quest houndHunt>   -> failed (run.floor >= 4)
  <on questFailed>   -> fires
    @oskar  Floor four already? Then it's gone to ground.
trace complete: 5 decisions
```

**이전 런.** [`prev.run.<path>`](/ko/state/state-model/#이전-런)은 다른 상태 경로와 같은 mock 가능 상태 경로입니다 — `--state prev.run.floor=3`, 또는 mock의 `state: { prev.run.floor: 3 }`입니다. mock하지 않으면 첫 런이 끝나기 전과 마찬가지로 `unset`이므로, 이를 가드하는 씬은 `unset` 경로를 택합니다.

```console
$ lute trace scenes/start-recap.lute --project . --state prev.run.floor=3
trace: scenes/start-recap.lute  (seeds: 1 paths, 0 facts; 0 selections)
  ## The foot of the stair
  <match has(prev.run.floor)>   -> arm 1 ((has(prev.run.floor)))
    @maud  Floor 3 last time. Beat it.
trace complete: 1 decision; arms 1/2 (has(prev.run.floor) @11:1)
```

## 보상과 목표 본문

퀘스트 walk는 [`lute play`](/ko/tooling/play/)와 같은 방식으로 정착하므로 trace, test, play는 완료 후 남는 결과에 합의합니다.

**크레딧이 있는 보상.** `<reward>`의 kind가 [`credits: <path>`](/ko/plugins/manifests/#상태에-적립되는-보상)를 선언하면 스칼라 금액을 부여할 때 `::set <path> += <n>`과 같은 규칙으로 그 경로에 더합니다. 위의 탑 `EMBERS` kind가 `user.embers`에 크레딧하므로, 부여 줄은 `grant houndHunt  EMBERS 50 (credits user.embers = 50)`처럼 크레딧을 끝에 표시하고 `--json`에서 `grant` 스텝은 `"credited": { "path": "user.embers", "value": "50" }`를 가집니다. walk는 크레딧된 값으로 끝나므로 테스트의 `expect.state`는 `user.embers: 50`을 단언할 수 있습니다. 범위 금액은 엔진이 굴리는 값이라 아무것도 크레딧하지 않으며, 더할 값이 없는 경로는 `unknown`으로 남습니다.

**목표 본문.** 본문이 있는 `<objective>`는 목표가 처음 done으로 바뀔 때 한 번 재생됩니다. 그 줄, `::set`, `::assert`, `::retract`는 목표의 `done` 판정과 자체 보상 뒤 트랜스크립트에 들어가며, `--choice`가 그 안의 `<branch>`를 결정합니다.

**퀘스트 구조** (dsl 0.24.0 §2). `--accepted-quest`(및 `accepts:`)는 `activate="accept"`인 자식 퀘스트를 취하며, 부모가 active인 동안에만 활성화합니다. `complete="any"`인 부모가 완료되면 다른 active 자식들은 실패하고, 결정에는 `superseded from quest.<parent>`라고 이유가 붙습니다(실패한 부모의 자식은 여전히 `cascade from quest.<parent>`를 읽습니다). `road`가 `parley` 또는 `toll` 중 하나로 완료되는 강 건너기 예시는 다음과 같습니다.

```console
$ lute trace quests/road.lute --project . --accepted-quest parley --accepted-quest toll --state run.talked=true
trace: quests/road.lute  (seeds: 1 paths, 0 facts; 0 selections)
  <quest road>   -> active (true)
  <quest parley>   -> active (forced)
  <quest toll>   -> active (forced)
  <objective words>   -> pending (quest.parley.state == 'complete')
  <objective silver>   -> pending (quest.toll.state == 'complete')
  <objective terms>   -> done (run.talked)
  <quest parley>   -> complete
  <objective pay>   -> pending (run.paid)
  <objective words>   -> done (quest.parley.state == 'complete')
  <objective silver>   -> pending (quest.toll.state == 'complete')
  <quest road>   -> complete
  <quest toll>   -> failed (superseded from quest.road)
trace complete: 12 decisions
```

대상에 대한 발생은 그 대상에 맞는 `<on event="E" target="…">` 핸들러도 실행합니다. `--occasion talk@npc.maud`(또는 `occasions: [talk@npc.maud]`)는 `<on event="talk" target="npc.maud">`를 발생시키지만, 일반 `talk`이나 다른 대상은 발생시키지 않습니다. 목표의 `until=`(dsl 0.24.0 §2.1)은 그러한 발생에서만 판정되고, `by=`는 모든 settle에서 판정됩니다. 씬의 `::accept{quest="…" at="nextRun"}`는 `quest relic accepted (queued: applies after the next run start)`를 렌더링하며 JSON 스텝은 `{"kind": "accept", "quest": "relic", "nextRun": true}`입니다 — 다음 `newRun` 뒤에 수락이 적용되며, 이는 [`lute play`](/ko/tooling/play/#퀘스트-구조)만 모델링합니다.

**핸들러의 accept** (dsl 0.26.0 §7). 퀘스트의 `<on>` 핸들러 안 `::accept{quest="…"}`는 `lute play`와 정확히 같이 같은 문서의 퀘스트를 취합니다. 핸들러의 `quest second accepted` 줄 뒤에는 `<quest second> -> active`가 나오고, 수락된 퀘스트의 목표도 같은 walk에서 판정되므로 테스트는 `expect: { quests: { second: active } }`를 단언할 수 있습니다. 0.26.0 전 trace는 accept를 출력하고 퀘스트를 `unset`으로 두었으므로 같은 문서의 test와 play가 서로 다른 결과를 냈습니다.

## 번들 비트

[번들 비트](/ko/tooling/play/#번들-비트)가 있는 lore 문서는 걸을 시퀀스가 없으므로 trace가 한 번에 하나씩 제시합니다: 로컬 id 또는 정규 `<document id>.<beat id>`인 `--beat <id>`를 사용합니다. 본문은 씬처럼 걸으며(`--choice`가 분기와 허브를 결정), 모든 효과를 적용하고 `when`은 강제하지 않고 표시합니다(JSON: 첫 스텝은 `{"kind": "beat", "id": …, "eligible": …}`). 오스카의 hunt는 다음과 같습니다.

```console
$ lute trace lore/oskar.lute --project . --beat hunt
trace: lore/oskar.lute  (seeds: 0 paths, 0 facts; 0 selections)
  <beat oskar.hunt>
    @oskar  The hound took my dog's collar. Bring it back before you pass floor four.
    quest houndHunt accepted
trace complete: 0 decisions
```

비트 자신의 `after="…"`(dsl 0.25.0 §3)도 mock의 `visited:`와 퀘스트 상태 위에서 같은 방식으로 표시되지만 강제되지는 않습니다. 성립하지 않으면 walk 시작에 ``<beat keeper.greeting>   (not eligible: `after` prerequisite not satisfied)``가 나옵니다.

전체 종류를 대상으로 하는 비트 또는 엔트리(`target="kind:trainer"`, dsl 0.26.0 §5; [종류 대상](/ko/tooling/play/#종류-대상) 참고)는 발생한 멤버를 `occasion.target`으로 읽습니다. 상태 경로와 같이 시드할 수 있습니다 — `--state occasion.target=r16Gus`, 또는 mock/test의 `state: { occasion.target: r16Gus }` — 종류가 정한 타입을 따르므로 비멤버는 `E-TRACE-MOCK-TYPE`입니다. 또는 `--occasion <OCCASION>@<target>`으로 비트의 계기를 해당 멤버에게 발생시키면 엔진처럼 바인딩됩니다(종류 밖 대상도 `E-TRACE-MOCK-TYPE`). 텍스트가 이를 보간하면 trace와 test는 `lute play`처럼 멤버를 출력합니다. 종류의 label이 우선하고, cast id인 멤버면 cast `name:`을 사용하며, 그 밖에는 id를 씁니다(`@narrator  Hiker Brom squares up.`). 둘 중 어느 것도 없으면 멤버를 필요로 하는 첫 줄 또는 쓰기에서 walk가 미완료로 멈추고 시드를 표시합니다. `--entry`도 엔트리의 `<document id>.<entry id>`를 받습니다(`--entry lore.tomas.tomasOil`).

lore 문서는 `--entry` 또는 `--beat`가 필요하며, 둘 다 없으면 사용법 오류(종료 **2**)에서 두 종류의 id를 모두 나열합니다. 문서에 없는 beat를 `--beat`로 지정하면 `E-TRACE-BEAT`(종료 **1**)이며 선언된 목록을 표시합니다.

<!-- lute-diagnostics -->
```console
$ lute trace lore/oskar.lute --project . --beat hnut
lore/oskar.lute: error [E-TRACE-BEAT] `--beat hnut` names an unknown beat id `hnut`; this document declares: oskar.hunt, oskar.rumor
```

`<beat>`가 없는 문서(가령 씬)에 `--beat`를 쓰는 경우도 `E-TRACE-BEAT`입니다. [`lute run --beat`](/ko/tooling/cli/#run)는 컴파일된 lore 아티팩트에서 같은 비트를 제시합니다.

시나리오 테스트는 `entry:` 대신 `beat: <id>`(일반 id 또는 정규 id)로 같은 비트를 제시합니다. Trace는 beat의 `when`을 표시하지만 강제하지 않습니다. 반면 dsl 0.26.0부터 `lute test`는 이를 강제하므로 mock이 자격 없게 만든 entry 또는 beat를 제시하는 테스트는 거짓인 전제를 표시하며 실패합니다. 단 `expect.eligible`을 단언한 경우는 예외입니다 — [`lute test`](/ko/tooling/cli/#test)를 참고하세요. beat의 id를 `--entry`에 주면 `E-TRACE-ENTRY`로 거부되고, 문서의 entry 뒤 메시지에 ``— `hunt` is a `<beat>`: present it with `--beat hunt` ``가 추가됩니다. 테스트에서는 대신 `beat:` 키를 표시합니다. 어느 쪽도 아닌 id라면 entry 뒤에 beat(`--beat`)도 나열합니다.

`eligible:` 기대값은 이 실패에 답합니다. 이를 단언하면 자격 없는 본문은 걷지 않으므로 `eligible: false` 테스트에는 재생되지 않는 본문의 브리지 응답이 필요하지 않습니다. 맵 키는 테스트가 제시하지 않은 파일의 entry 또는 번들 beat를 가리킬 수도 있습니다. 같은 mock 아래에서 단독으로 판정되므로, lore 테스트는 아무것도 제시하지 않고도 맵 형태 `eligible:`를 가질 수 있습니다. walk 구조를 읽는 테스트 기대값도 두 가지 더 있습니다. `expect.accepts: [quest ids]`는 씬의 `::accept`가 취한 퀘스트를 집합으로 단언하고(`accepts: expected [toll], got [parley]`), `<hub>`의 `expect.options`는 branch와 마찬가지로 그 모든 방문에서 자격 있는 선택지의 합집합입니다(원래도 `[]`였습니다). [`lute test`](/ko/tooling/cli/#test)를 참고하세요.
## 트랜스크립트 읽기

사람이 읽는 형식은 들여쓰기된 순서 있는 트랜스크립트입니다. 출력된 콘텐츠 줄(판정된 보간은 치환하고, 알 수 없는 경우 `{{…}}`를 그대로 유지함), 연출 디렉티브, 상태 쓰기, 그리고 **판정**마다 한 줄 — 구성 요소, 이긴 arm/choice, 읽은 값이 표시된 가드 — 를 담습니다. 마지막 요약에는 수행한 판정, arm/choice 커버리지, 그리고 아직 해결되지 않은 원자가 표시됩니다.

```console
$ lute trace docs/examples/choice-persist.lute --choice sofaHelp=help
trace: docs/examples/choice-persist.lute  (seeds: 0 paths, 0 facts; 1 selection)
  ## Recording the Choice
    @narrator  Elena had slipped on the wet step. You could help — or walk on.
  <branch sofaHelp>   eligible: help, warmly, tip   -> help
    @elena  Thank you. I won't forget this.
    ::set  run.metHelpfully = true  (into sugar)
  ## Reading It Back
  <match run.metHelpfully>   -> arm 1 (is="true")
    @elena  You helped me back then. I've been meaning to thank you again.
trace complete: 2 decisions; choices 1/3 (sofaHelp), arms 1/2 (run.metHelpfully @42:1)
```

`unknown` 가드는 그 구성 요소에서 걷기를 멈추고(종료 3), mock에 필요한 미해결 원자 — 어떤 경로나 팩트가 필요한지 — 를 보고합니다. Trace는 자격이 `unknown`인 지점을 절대 추측해서 지나가지 않습니다. `--choice`로 `unknown` 가드를 강제로 지나가는 것이 문서화된 탈출구입니다. 강제 선택도 집계됩니다. 요약에는 `1 unresolved (forced past an unknown guard — the walk continued, exit unchanged)`가 표시되고, 이를 결정할 원자도 이름이 나옵니다. `--json`에서는 이를 `forcedUnknown` 아래에 나열합니다. 종료 코드는 나머지 걷기에서 얻은 값 그대로 유지됩니다. 예약된 퀘스트 읽기(`quest.<id>.state`, `…objectives.<oid>.done`, 그리고 0.24.0부터 `quest.<id>.failedBy` 및 `…objectives.<oid>.failed`)는 mock(`quests:` 또는 `--state`)하지 않는 한 기본값(`unset` / `false`)으로 해석됩니다. 프로젝트가 없으면 각 읽기에 "existence unverified" 메모가 붙고, 프로젝트가 있으면 trace가 id를 프로젝트의 퀘스트와 대조합니다.

**def 참조를 작성된 그대로 읽기**(dsl 0.24.0). `<match subject="@def">` 헤더, arm 또는 choice 가드, 커버리지 요약은 작성자가 쓴 방식으로 def 참조를 출력합니다. `--expand`를 사용하면 걷기에서 평가한 확장 형태를 출력합니다:

```console
$ lute trace week.lute --state user.runs=3 --choice ask=old
trace: week.lute  (seeds: 1 paths, 0 facts; 1 selection)
  ## Morning
  <match @weekday>   -> arm 1 (is="mon")
    @narrator  Monday again.
  <branch ask>   eligible: old, new   -> old (@atLeast(3))
    @narrator  You remember.
trace complete: 2 decisions; choices 1/2 (ask), arms 1/2 (@weekday @13:1)
$ lute trace week.lute --state user.runs=3 --choice ask=old --expand
trace: week.lute  (seeds: 1 paths, 0 facts; 1 selection)
  ## Morning
  <match (run.day == 1 ? 'mon' : run.day == 2 ? 'tue' : 'other')>   -> arm 1 (is="mon")
    @narrator  Monday again.
  <branch ask>   eligible: old, new   -> old ((user.runs >= 3))
    @narrator  You remember.
trace complete: 2 decisions; choices 1/2 (ask), arms 1/2 ((run.day == 1 ? 'mon' : run.day == 2 ? 'tue' : 'other') @13:1)
```

`--json`은 항상 `id`, `guard`, 커버리지 항목의 `label`에 확장 형태를 담고, 확장으로 바뀐 부분에 한해서만 작성자가 쓴 텍스트를 `authoredId`, `authoredGuard`, `authoredLabel`로 추가합니다.

**비트 자체의 `when`**. [비트 씬](/ko/language/beats/)을 trace하면 프런트매터의 `when:` 성립 여부와 관계없이 본문을 걷습니다. Trace는 미리보기이기 때문입니다. mock이 해당 `when`을 거짓 또는 미결정으로 만들면, trace는 메모로 시작합니다 — ``beat `when` (run.day == 3) is false under these mocks — the `visit` selector would never present this scene; the walk below shows it as if it had been presented``. `lute test`는 그런 걷기를 통과시키지 않습니다. dsl 0.26.0부터 mock이 `when`, `after:`, 또는 소진된 `once: user` 때문에 비트를 부적격으로 만들었는데도 그 비트를 제시하는 테스트는 `expect.eligible`을 단언하지 않는 한 실패합니다([`lute test`](/ko/tooling/cli/#test) 참고).

씬의 [`::accept{quest="<id>"}`](/ko/language/directives/#accept--퀘스트-수락)는 자체 트랜스크립트 줄 `quest <id> accepted`(JSON step `{"kind": "accept", "quest": "<id>"}`)로 렌더링됩니다. Trace는 문서 하나를 걷기 때문에 accept를 기록만 하고 적용하지 않습니다. 퀘스트의 수명 주기가 실행되는 곳은 퀘스트 자체의 문서입니다(그곳에서 `--accepted-quest` / `accepts:`를 사용하거나, 프로젝트 전체에서는 `lute play`를 사용). 퀘스트 자체의 `<on>` 핸들러에 있는 `::accept`는 적용됩니다 — [핸들러의 accept](#보상과-목표-본문)를 참고하세요.

```console
$ lute trace scenes/shed.lute --project . --choice offer=take
trace: scenes/shed.lute  (seeds: 0 paths, 0 facts; 1 selection)
  ## Shed
    @vesna  Somebody has to mind the shed.
  <branch offer>   eligible: take, leave   -> take
    quest sideJob accepted
    @vesna  Good. It's yours.
trace complete: 1 decision; choices 1/2 (offer)
```

### 선택된 `::jump` 따라가기

걷기가 취한 `::jump{to="<label>"}`는 `lute run` 및 `lute play`와 정확히 같이 해당 `::label{name="<label>"}`로 점프합니다(dsl 0.26.0 §7). 트랜스크립트에는 `<jump -> <label>>`(JSON step `{"kind": "jump", "to": "<label>"}`)가 표시되고, 걷기는 라벨부터 계속되므로 `trace complete` — 그리고 테스트의 `end: complete` — 는 걷기가 문서 끝에 도달했다는 뜻입니다. 0.26.0 이전에는 걷기가 취한 점프에서 끝나도 complete를 보고했으므로, 테스트가 플레이에서는 절대 보이지 않는 트랜스크립트를 대상으로 통과할 수 있었습니다. 다음은 문 선택이 가드를 건너뛰어 점프하는 금고입니다:

```console
$ lute trace scenes/vault.lute --project . --choice enter=yes
trace: scenes/vault.lute  (seeds: 0 paths, 0 facts; 1 selection)
  ## The door
    @narrator  The vault door stands open.
  <branch enter>   eligible: yes, no   -> yes
    <jump -> hall>
  ## The hall
    <label>
    @narrator  The treasure hall.
    ::set  run.gold = 10
trace complete: 1 decision; choices 1/2 (enter)
```

같은 걷기에 대한 테스트는 플레이에서 보이는 것을 단언합니다 — `transcriptContains: ["@narrator: The treasure hall."]`, `transcriptLacks: ["@narrator: A guard waves you back."]`, `state: { run.gold: 10 }` — 그리고 통과합니다.

### 루프 단계 (0.51)

한정된 뒤로 가는 `::jump{to="…" max="N"}`([디렉티브](/ko/language/directives/#한정된-뒤로-점프--max) 참고)는 방문할 때마다 트랜스크립트에 나타납니다. 실행된 점프는 몇 번째인지와 한계를 출력하며 — `<jump -> retry (again 1/2)>` — `N`번을 다 쓰고 나면 점프는 **실행되지 않고** 걷기는 다음 노드로 이어집니다: `<jump -> retry not taken (2/2 used)>`. 드라이버의 `jump` 관찰도 같은 사실을 담습니다. 실행된 점프에는 번호와 `max`가, 이어지는 경우에는 `exhausted` 관찰이 붙습니다. 반복할 때마다 다시 도달한 branch는 새 메뉴이며, 그 `--choice` 목록은 제시마다 결정 하나씩 소비되므로 항목 세 개가 반복 세 번을 이끕니다. `<branch id="again">` 앞에 `::label{name="retry"}`가 있고 그 `push` 선택지에 `::jump{to="retry" max="2"}`가 있는 latch 장면에 `--choice again=push,push,push`로 답하면:

```console
$ lute trace scenes/latch.lute --project . --choice again=push,push,push
trace: scenes/latch.lute  (seeds: 0 paths, 0 facts; 3 selections)
  ## The latch
    <label>
    @mira  The latch sticks.
  <branch again>   eligible: push, leave   -> push
    <jump -> retry (again 1/2)>
    <label>
    @mira  The latch sticks.
  <branch again>   eligible: push, leave   -> push
    <jump -> retry (again 2/2)>
    <label>
    @mira  The latch sticks.
  <branch again>   eligible: push, leave   -> push
    <jump -> retry not taken (2/2 used)>
    @mira  Let's go.
trace complete: 3 decisions; choices 1/2 (again)
```

트랜스크립트의 나머지와 마찬가지로 이것은 mock이 이끈 걷기에 대한 설명이지 정적 주장이 아닙니다. 실제 플레이에서 루프가 도달 가능한지 아닌지를 말해 주지 않습니다.

## 브리지 응답

호스트 서비스를 호출하는 플러그인 디렉티브는 `bridgeResult` 효과를 통해 그 결과를 `scene.*` 슬롯에 씁니다. Trace는 서비스를 호출하지 않습니다. 0.24.0부터 응답되지 않은 호출은 해당 슬롯을 **unknown**으로 남깁니다 — 상태 형태의 기본값을 절대 사용하지 않습니다. 따라서 그 슬롯을 검사하는 가드가 있으면, 답변해야 할 내용을 이름으로 알려 주는 힌트와 함께 걷기가 미완료로 중단됩니다. [스토리 플레이](/ko/tooling/play/#브리지-호출에-답하기)의 마을 문에서 `::check`는 `scene.check.<key>.passed`와 `.margin`을 씁니다:

```console
$ lute trace scenes/gate/guards.lute --project .
trace: scenes/gate/guards.lute  (seeds: 0 paths, 0 facts; 0 selections)
  ## The gate
    <check>
      (bridge unanswered: no `bridges.check` answer — its results read unknown)
trace incomplete: 1 unresolved atom (exit 3)
  unresolved: match `is="true"` (scene.check.guards.passed match) — supply bridges: { check: [ { passed: <bool>, margin: <int> } ] } (plugin `check` call unanswered; `scene.check.guards.passed` reads its `passed` result) as a mock; arms 0/2 (scene.check.guards.passed @12:1)
```

힌트는 자리표시자를 채우면 로더가 받아들이는 답변입니다. 콘텐츠가 읽는 호출 결과의 모든 필드를 그 타입과 함께 나열합니다 — enum은 `<bool>`, `<int>`, `<double>`, `<string>`, `<one of: a|b>`, 그 밖에는 `<value>`입니다. mock(또는 `*.test.yaml`)의 `bridges:`는 호출 순서대로 답변하며, 태그가 호출될 때마다 하나씩 답하고 콘텐츠가 읽는 결과 필드를 각각 제공합니다:

```yaml
# mocks/gate.yaml
file: ../scenes/gate/guards.lute
bridges:
  check:
    - { passed: true, margin: 3 }
    - { passed: false, margin: -2 }
```

```console
$ lute trace scenes/gate/guards.lute --project . --mock mocks/gate.yaml
trace: scenes/gate/guards.lute  (seeds: 0 paths, 0 facts; 0 selections)
  ## The gate
    <check>
      (bridge answered: passed=true, margin=3)
  <match scene.check.guards.passed>   -> arm 1 (is="true")
    @narrator  The guards wave you through.
  <match scene.check.guards.margin > 5>   -> otherwise
    <check>
      (bridge answered: passed=false, margin=-2)
  <match scene.check.sneak.passed>   -> arm 2 (is="false")
    @narrator  A stallholder shouts after you.
trace complete: 3 decisions; arms 1/2 (scene.check.guards.passed @12:1), arms 1/2 (scene.check.guards.margin > 5 @15:5), arms 1/2 (scene.check.sneak.passed @22:1)
```

**아무것도 읽지 않는 필드는 생략할 수 있습니다**(dsl 0.25.0 §7). 프로젝트에서 어떤 가드, 줄 또는 쓰기도 결과 필드를 읽지 않는다면 그 필드는 아무것도 결정하지 않으므로 답변에서 생략할 수 있습니다. 결과 슬롯은 그대로 미해결 상태로 남습니다. 여기서는 한 줄이 `scene.check.guards.margin`을 읽으므로 `margin`이 필요합니다. 그 줄이 없다면 `- { passed: true }`가 완전한 답변이며, 응답되지 않은 호출의 힌트에는 콘텐츠가 읽는 것만 나옵니다: `supply bridges: { check: [ { passed: <bool> } ] }`. 답변은 읽히지 않는 필드를 호출의 효과가 쓰는 경우에도 제공할 수 있습니다.

문서의 플러그인 호출 중 브리지 결과를 읽는 것이 없는 태그, 또는 효과가 읽는 필드가 없는 답변은 `E-TRACE-MOCK-UNDECLARED`입니다. 콘텐츠가 읽는 필드가 빠진 답변이나 결과 슬롯에 맞지 않는 값은 `E-TRACE-MOCK-TYPE`입니다 — 누락된 필드의 메시지는 타입이 지정된 답변을 적고, 잘못된 값은 태그의 호출이 쓰는 모든 슬롯에 대해 검사합니다. 각각은 문서가 아니라 mock 파일의 문제가 있는 태그 키, 답변 또는 필드 키에 위치가 표시됩니다(`*.test.yaml`, `--mock` 파일 또는 `mocks/*.yaml`). JSON 진단에는 `"provenance": "mock"`이 들어갑니다:

<!-- lute-diagnostics -->
```console
$ lute trace scenes/gate/guards.lute --project . --mock lack.yaml
lack.yaml:4:7: error [E-TRACE-MOCK-TYPE] `bridges.check` answer 1 lacks `margin`, which content reads — an answer gives every bridge result `::check` content reads: `{ passed: <bool>, margin: <int> }`
trace refused: scenes/gate/guards.lute — invalid mock input
```

<!-- lute-diagnostics -->
```console
$ lute trace scenes/gate/guards.lute --project . --mock bad.yaml
bad.yaml:4:9: error [E-TRACE-MOCK-TYPE] `bridges.check` answer 1: `passed: yes` is not compatible with `scene.check.guards.passed`'s declared type
bad.yaml:4:9: error [E-TRACE-MOCK-TYPE] `bridges.check` answer 1: `passed: yes` is not compatible with `scene.check.sneak.passed`'s declared type
trace refused: scenes/gate/guards.lute — invalid mock input
```

`lack.yaml`에는 `- { passed: true }`가, `bad.yaml`에는 `- { passed: yes, margin: 3 }`가 각각 파일의 네 번째 줄에 있습니다. 0.24 이전에는 필드 누락이 `E-TRACE-MOCK-UNDECLARED`였고, 이런 오류는 모두 `<document>:0:0`에 표시되었습니다. 시나리오 테스트에서는 같은 오류가 테스트 자체의 줄에 표시됩니다 — `FAIL` 아래의 `./tests/t.test.yaml:6:9: error [E-TRACE-MOCK-TYPE] …` — 그리고 프로젝트 전체 `check`는 잘못된 `mocks/*.yaml`을 `./mocks/lack.yaml:4:7`에 보고합니다.

**0.23.1 mock 마이그레이션**. 결과 슬롯을 시드하여 호출에 답하던 mock 또는 테스트 — `state: { scene.check.guards.passed: true }` — 는 이제 가드를 결정하지 않습니다. 호출의 답변은 `bridges:`에서만 읽으므로, 걷기는 위의 힌트와 함께 미해결 상태로 멈춥니다. 시드를 `bridges: { check: [ { passed: true, margin: 3 } ] }`로 바꾸고, 호출 순서대로 호출마다 하나씩, 콘텐츠가 읽는 모든 필드를 넣으세요.

프로젝트 전체 `check`는 `mocks/*.yaml`에도 같은 검사를 수행합니다. 답변이 없는 시나리오 테스트는 미완료로 실패하며 힌트가 `in this test`로 끝납니다. `lute run --mock`은 같은 키에서 호출 답변을 가져오고, [`lute play`](/ko/tooling/play/#브리지-호출에-답하기)는 최상위와 각 스텝에서 답변을 받습니다.

## 파생

**팩트는 닫혀 있고, 규칙이 파생합니다**(dsl 0.22.0 §6). Trace는 스키마의 시드 `facts:`를 불러오고, 사용자의 `--fact` / `facts:` mock과 걷기에서 수행한 모든 `::assert` / `::retract`를 더한 다음, 프로젝트의 Datalog 규칙 — 계층화된 부정 포함 — 을 적용합니다. `lute run` 및 `lute play`가 사용하는 것과 같은 평가기이므로 trace, test, play는 프로젝트 규칙이 내리는 결론에 대해 서로 다른 결과를 낼 수 없습니다. 시드되지도, mock되지도, assert되지도 않은 기본 팩트는 거짓입니다. 파생 팩트는 규칙이 결론으로 낼 때 정확히 성립합니다. 따라서 파생 관계에 대한 가드, `done` 또는 `start`는 결론을 mock하지 않아도 결정되며, "부정 전제가 성립하여 파생 결과가 거짓이 됨"도 테스트할 수 있습니다:

```lute check
---
kind: scene
id: case.accuse
entities:
  person: { members: [ann, bob] }
relations:
  suspect: { args: [person], tier: run }
  alibi: { args: [person], tier: run }
  culprit: { args: [person], derive: true }
facts:
  - "suspect(ann)"
rules:
  - "culprit(P) :- suspect(P), not alibi(P)"
---

## The parlour

@inspector: Everyone stays in this room.

<branch id="ask">
  <choice id="maid" text="Ask the maid first">
    @maid: Miss Ann? She was at the harbour all evening.
    ::assert{ alibi(ann) }
  </choice>
  <choice id="skip" text="Ask no one">
    @inspector: No more questions.
  </choice>
</branch>

<branch id="verdict">
  <choice id="ann" text="Name Ann" when="holds('culprit', ['ann'])">
    @inspector: It was Ann. No one can place her anywhere else.
  </choice>
  <choice id="wait" text="Keep looking">
    @inspector: Then we start again.
  </choice>
</branch>
```

mock이 전혀 없어도 시드된 `suspect(ann)`과 없는 `alibi(ann)`에서 `culprit(ann)`이 파생됩니다:

```console
$ lute trace accuse.lute --choice ask=skip --choice verdict=ann
trace: accuse.lute  (seeds: 0 paths, 0 facts; 2 selections)
  ## The parlour
    @inspector  Everyone stays in this room.
  <branch ask>   eligible: maid, skip   -> skip
    @inspector  No more questions.
  <branch verdict>   eligible: ann, wait   -> ann (holds('culprit', ['ann']))
    @inspector  It was Ann. No one can place her anywhere else.
trace complete: 2 decisions; choices 1/2 (ask), choices 1/2 (verdict)
```

하녀의 `::assert{ alibi(ann) }`가 실행되면 부정 전제가 성립하여 `culprit(ann)`은 확실히 거짓이 됩니다 — `verdict`에는 `wait`만 제시되고, 그때 `--choice verdict=ann`으로 강제하려 하면 `E-TRACE-CHOICE`(종료 1)로 거부됩니다. `unknown`을 강제로 지나가는 것이 아닙니다:

```console
$ lute trace accuse.lute --choice ask=maid --choice verdict=wait
trace: accuse.lute  (seeds: 0 paths, 0 facts; 2 selections)
  ## The parlour
    @inspector  Everyone stays in this room.
  <branch ask>   eligible: maid, skip   -> maid
    @maid  Miss Ann? She was at the harbour all evening.
    ::assert  alibi(ann)
  <branch verdict>   eligible: wait   -> wait
    @inspector  Then we start again.
trace complete: 2 decisions; choices 1/2 (ask), choices 1/2 (verdict)
```

거부 메시지는 거짓인 전제(라운드-5 T3-12)의 이름을 밝힙니다. 즉 작성된 가드, 그 가드가 거짓인 모든 읽기, 그리고 결과를 바꿀 mock을 보여 줍니다 — ``its guard `holds('found', ['receipt'])` decided false: `found(receipt)` does not hold (mock `--fact "found(receipt)"`)`` 또는 ``… `run.x` is 2 (mock `--state run.x=<value>`)``. `visited(…)` 또는 퀘스트 상태 읽기는 `--mock` 파일 키(`visited: [a]`, `quests: { q: <state> }`)와 함께 이름이 표시되고, 단일 파일 trace는 이전 씬에서 무슨 일이 일어났는지 알 수 없다는 메모가 붙습니다. 소진된 허브 옵션은 ``it is `once` and already taken in this visit of hub `h` ``라고 표시됩니다. `*.test.yaml`에서는 힌트가 테스트 키로 표기됩니다(`facts: ["found(receipt)"]`, `state: { run.x: <value> }`). `lute play`는 같은 전제를 자체 `E-TRACE-CHOICE`에 덧붙입니다.

mock된 파생 원자도 받아들입니다. 다른 것과 마찬가지로 시드이므로 `--fact "culprit(ann)"`은 규칙이 결론 내리는 것을 성립시킵니다. [`lute test`](/ko/tooling/cli/#test)도 같은 방식으로 걷습니다. 걷기가 중단되었는데 `expect: { end: incomplete }`를 선언하지 않은 테스트는 실패합니다. 테스트는 `expect.facts`와 `expect.notFacts`로 규칙의 결론을 단언합니다 — 걷기가 끝났을 때 성립해야 하거나 성립하지 않아야 하는 원자입니다:

```yaml
file: accuse.lute
choose: { ask: maid, verdict: wait }
expect:
  facts: ["alibi(ann)"]
  notFacts: ["culprit(ann)"]
```

불일치는 양쪽을 모두 표시합니다 — `notFacts culprit(ann): expected does not hold, got holds` — 그리고 파생 과정에서 미결정 상태를 읽은 원자는 `unknown`이며 어느 목록도 충족하지 않습니다.

**미결정 상태 위의 규칙 가드**. 규칙은 상태를 검사할 수 있습니다(`cel("…")`). mock이 그 상태를 미결정으로 남겨 두면 규칙은 아무것도 결정하지 않습니다. 결론은 unknown이 되고, 보고서는 파생 원자가 아니라 이를 결정할 상태 경로를 이름으로 표시합니다:

```lute check
---
kind: scene
id: case.arrest
state:
  run.day: { type: int }
entities:
  person: { members: [ann] }
relations:
  suspect: { args: [person], tier: run }
  warrant: { args: [person], derive: true }
facts:
  - "suspect(ann)"
rules:
  - 'warrant(P) :- suspect(P), cel("run.day >= 3")'
---

## The station

<branch id="arrest">
  <choice id="now" text="Arrest Ann" when="holds('warrant', ['ann'])">
    @inspector: The warrant came through. Bring her in.
  </choice>
  <choice id="later" text="Wait for the warrant">
    @inspector: Not yet.
  </choice>
</branch>
```

```console
$ lute trace arrest.lute --choice arrest=now
trace: arrest.lute  (seeds: 0 paths, 0 facts; 1 selection)
  ## The station
  <branch arrest>   eligible: later   -> now (holds('warrant', ['ann'])) (forced)
    @inspector  The warrant came through. Bring her in.
trace complete: 1 decision; 1 unresolved (forced past an unknown guard — the walk continued, exit unchanged); choices 1/2 (arrest)
  unresolved (forced): branch `arrest -> now` choice guard `holds('warrant', ['ann'])` was unknown — supply --state run.day=<value> as a mock to decide it
```

`--state run.day=3`을 주면 규칙이 `warrant(ann)`을 결론 내리고 `now`는 곧바로 자격을 얻습니다.

**`derive: false`는 0.21을 복원합니다.** mock 키 `derive: false`(`*.test.yaml`에서도 허용됨) 또는 `lute trace`와 `lute test`의 `--no-derive` 플래그 — 플래그가 키보다 우선함 — 는 조회만 하는 모델을 복원합니다. 시드 팩트는 불러오지 않고 규칙은 적용하지 않으며, mock하지 않은 파생 관계의 팩트는 **unknown**이 됩니다. 따라서 그 팩트의 가드는 강제하지 않는 한 걷기를 중단합니다(종료 3). 스키마가 공급되지 않은 시드 팩트를 선언한 경우 이를 알리는 메모리가 표시되고, 걷기에서 읽은 각 파생 관계를 이름으로 밝히는 메모리도 표시됩니다. (`lute play`도 같은 키와 플래그를 받지만, 그 경우 시드 팩트는 여전히 로드되고 규칙만 멈춥니다 — [스토리 플레이](/ko/tooling/play/#파생과---derivation)를 참고하세요.)

```console
$ lute trace accuse.lute --choice ask=skip --choice verdict=ann --no-derive
trace: accuse.lute  (seeds: 0 paths, 0 facts; 2 selections)
note: the schema declares seed facts (e.g. `suspect`) but under `derive: false` trace does not auto-load them (the explicit-world model) — supply seeded relations explicitly via --fact
note: derived relation `culprit` read under `derive: false`: its rules were not applied, so an unmocked `culprit(…)` is unknown — supply it via --fact, or drop `derive: false`
  ## The parlour
    @inspector  Everyone stays in this room.
  <branch ask>   eligible: maid, skip   -> skip
    @inspector  No more questions.
  <branch verdict>   eligible: wait   -> ann (holds('culprit', ['ann'])) (forced)
    @inspector  It was Ann. No one can place her anywhere else.
trace complete: 2 decisions; 1 unresolved (forced past an unknown guard — the walk continued, exit unchanged); choices 1/2 (ask), choices 1/2 (verdict)
  unresolved (forced): branch `verdict -> ann` choice guard `holds('culprit', ['ann'])` was unknown — supply --fact "culprit(ann)" as a mock to decide it
```

**0.21에서 마이그레이션하기**. mock하지 않은 파생 원자가 unknown(종료 3)이거나 시드된 관계가 빈 값으로 읽히는 데 의존하던 테스트는 이제 파생 또는 시드된 답을 보고 판정이 바뀔 수 있습니다. 이전 동작을 유지하려면 그 테스트 또는 mock에 `derive: false`를 고정하세요. 더 나은 방법은 `expect.facts` / `expect.notFacts`로 규칙이 내리는 결론을 단언하는 것입니다. 한 번에 모든 것을 예전 방식으로 걷고 싶다면 `lute test --no-derive`를 실행하세요. 이 플래그는 각 테스트와 플레이 스크립트의 자체 `derive:`보다 우선합니다.
## 배타 관계

[`excludes:`](/ko/state/facts-and-datalog/#배타적-관계-excludes)로 선언한 관계(dsl 0.25.0 §1)는 같은 인자에서 함께 성립하지 않습니다. Trace는 매번 쓰기가 끝난 뒤 mock, 시드, assert, 파생된 팩트를 검사하며, 두 배타 팩트를 함께 성립시키는 쓰기가 나오면 그 자리에서 걷기를 멈춥니다. `panicked(maren)`을 시드하고 `calm(maren)`을 assert하는 새벽 씬을 걸어 보세요.

<!-- lute-diagnostics unverified="verbatim lute trace output; E-FACT-EXCLUSIVE is named through a constant in the trace crate, so the scraper cannot pair quote and code" -->
```console
$ lute trace scenes/dawn.lute --project . --fact "panicked(maren)" --choice look=nothing
trace: scenes/dawn.lute  (seeds: 0 paths, 1 facts; 1 selection)
  ## First light
  <branch look>   eligible: saw, nothing   -> nothing
    @narrator  Nobody answers.
  <match>   -> otherwise
    ::assert  calm(maren)
    ✗ exclusive: calm(maren) and panicked(maren) both hold
trace stopped at the `✗ exclusive` line above (exit 1); choices 1/2 (look), arms 1/2 (match @26:1)
scenes/dawn.lute:31:5: error [E-FACT-EXCLUSIVE] this write makes exclusive relations hold together: calm(maren) and panicked(maren) both hold
trace refused: scenes/dawn.lute — exclusive relations hold together
```

종료 코드는 **1**이며, 쓰기 뒤의 어떤 내용도 걷지 않습니다. 이런 쓰기에 도달하는 시나리오 테스트도 같은 방식으로 실패합니다. 시드된 팩트 자체 — mock의 `facts:` 또는 `--fact`, 프로젝트의 `facts:` 시드, 그리고 규칙이 그 팩트에서 파생한 내용 — 가 이미 배타성을 깨뜨리면, trace는 걷기 시작 전에 `E-FACT-EXCLUSIVE`와 함께 거부합니다. 프로젝트 전체 `check`는 `::assert`에 도달하는 모든 경로에서 다른 팩트가 성립하는 경우 같은 코드를 정적으로 보고합니다. Trace는 이전 씬의 한 분기에서만 assert되거나 mock으로 공급되는 것처럼 가능하기만 한 경우를 포착합니다.

## `E-TRACE-*` 거부

걷기 전에 trace는 `check`와 정확히 같은 방식으로 문서를 해석하고, 검사 오류나 잘못된 mock이 있는 문서는 **거부**합니다(종료 1) — 먼저 `check`를 실행하세요. mock 거부 코드는 다음과 같습니다. `E-TRACE-MOCK-PARSE`(잘못된 mock 파일 — 네 가지 상태 이외의 `quests:` 상태, `{ run: [...], user: [...] }`가 아닌 `entriesRead:`, `true`/`false`가 아닌 `derive:`, 태그 → 답변 맵이 아닌 `bridges:` 포함), `E-TRACE-MOCK-UNDECLARED`(선언되지 않은 `--state` 경로, 문서가 읽지도 선언하지도 않은 `quests:`/`entriesRead:` 경로, 또는 어떤 플러그인 호출도 읽지 않는 `bridges:` 태그나 필드), `E-TRACE-MOCK-TYPE`(시드 또는 브리지 답변의 리터럴 타입이 잘못되었거나, 호출이 읽는 필드가 브리지 답변에서 빠짐), `E-TRACE-MOCK-FACT`(알 수 없는 관계/arity/외부 인자), `E-TRACE-CHOICE`(알 수 없거나 자격 없는 강제 선택), `E-TRACE-ENTRY`(lore가 아닌 문서에서 `--entry`를 사용하거나 해당 entry 중 어느 것도 이름으로 지정하지 않음 — 메시지는 문서의 entry를 나열한 뒤 beat도 나열하며, beat의 id를 지정한 경우에는 `--beat`로 제시하라고 말함), `E-TRACE-BEAT`(문서에 없는 bundle beat를 `--beat`로 지정함, dsl 0.23.0), `E-TRACE-EVENT`(내장 수명 주기 이벤트 `questActive`/`questComplete`/`questFailed` — 엔진이 파생하며 손으로 발생시키지 않음), `E-TRACE-ACCEPT`(알 수 없는 퀘스트 id 또는 `start` predicate를 지녀 accept가 필요 없는 퀘스트)입니다. 일치하는 것이 없는 `--event`는 거부가 아니라 정보성 메모입니다.

0.6.1부터 trace는 거부가 아닌 경고도 냅니다 — 어떤 작성된 producer도 절대 assert할 수 없는 관계의 `--fact`/mock-YAML 팩트(`producible()`이 생산 불가로 판정함)에 대해 `W-TRACE-MOCK-UNPRODUCIBLE`을 냅니다. 공급한 답변은 도달 가능한 플레이에서 절대 발생할 수 없으므로, 그것을 시드한 "complete" 걷기는 아무것도 증명하지 못합니다. `reserved: true`이거나 `open: engine` 인자 관계는 정의상 생산 가능하므로 경고하지 않습니다.

Trace가 프로젝트를 볼 수 있으면 판정 범위는 프로젝트 전체입니다 — `--project <DIR>`을 주거나 trace하는 파일 위에 `lute.project.yaml`이 있는 경우입니다. `facts:` 시드 또는 프로젝트 전체 `check`가 도달 불가라고 증명하지 않은 문서의 `::assert`가 관계를 생산할 수 있으면 그 관계는 생산 가능하며, 파생 관계는 규칙을 통해 판정합니다. 프로젝트가 없으면 경고는 trace 대상 문서만 판정하고 이를 명시합니다(`judged against this document only; pass --project <DIR> to count the asserts of the project's other documents`). 따라서 이웃 씬에서 assert한 단서는 그 문서에서만 "not producible"입니다.

## 0.34.0의 증거

Trace 판정과 JSON verdict에는 `evidence`가 들어갑니다. 구체적인 mock 기반 판정은 `witnessed`이고, 미해결 가드 또는 미완료 걷기는 `unknown`입니다. 사람이 읽는 출력에서는 접미사 `[unknown]`으로 표시됩니다. Trace는 제공된 하나의 걷기일 뿐, 가능한 모든 경로의 증명이 아닙니다.
