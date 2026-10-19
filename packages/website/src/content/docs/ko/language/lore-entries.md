---
title: 로어 엔트리
description: "로어 문서 종류 — 엔진이 플레이하는 대신 찾아보는 <entry> 선언(아이템 설명, 발견한 쪽지, 비문, 코덱스 페이지, 말버릇)과 상태에 따라 달라지는 텍스트 및 팩트를 통해 밝혀지는 지식 — 그리고 그 옆에 짧은 씬 비트를 묶는 <beat> 블록입니다."
---

씬과 퀘스트는 스토리를 **시간 축**에 놓습니다: 무엇이 어떤 순서로 일어나는가. 하지만 게임 스토리의 상당 부분은 다른 곳에 있습니다 — **물체와 장소**에 있습니다. 연구실의 찢긴 페이지, 문에 새겨진 비문, 화재 뒤 설명이 달라지는 열쇠, 곁을 지나갈 때 NPC가 중얼거리는 한마디입니다. 플레이어는 이것들을 어떤 순서로든 발견하고, 다시 읽고, 스토리를 짜 맞춥니다.

**로어 문서**(`kind: lore`, dsl 0.19.0)는 그런 텍스트를 선언합니다. Lute는 엔트리가 무엇을 읽고, 언제 자격을 얻으며, 읽기가 무엇을 바꾸는지 말합니다. 어디에 존재하는지는 엔진이 결정합니다: 어느 방에 어떤 아이템이 생성되는지, 어떤 패널에 코덱스가 표시되는지, NPC가 언제 말을 거는지입니다.

## 로어 문서

```lute check
---
kind: lore
title: Ship's records
entities:
  crew: { members: [vesna, toma] }
  topic: { members: [project_lumen, true_heading] }
relations:
  knows: { args: [crew, topic], tier: run }
state:
  run.labBurned: { type: bool, default: false }
---

<entry id="scientistLog1" target="item.torn_note_1" category="note" series="scientistLog" order="1" title="Research log, day 3">
  @scientist: Day three. Subject E does not respond to light.
  ::assert{knows(vesna, project_lumen)}
</entry>

<entry id="scientistLog2" target="item.torn_note_2" category="note" series="scientistLog" order="2" when="entry.scientistLog1.read">
  @scientist: Day nine. We stopped writing her name in the logs.
</entry>

<entry id="rustyKey" target="item.rusty_key" category="item">
  <match subject="run.labBurned">
    <when is="true">
      @narrator: A scorched key. Someone's name has melted into the grip.
    </when>
    <otherwise>
      @narrator: A rusty key, stamped "Research wing B2".
    </otherwise>
  </match>
</entry>
```

최상위에는 `<entry>` 선언과 dsl 0.23.0부터의 `<beat>` 블록([한 파일에 엔트리와 비트](#한-파일에-엔트리와-비트) 참고)만 있으며 그 밖에는 아무것도 없습니다 — `## ` 섹션도, `<quest>`도 없습니다. 문서의 제목은 프런트매터의 `title:`입니다.

## `<entry>` 속성

| Attr | 의미 |
|---|---|
| `id` | 필수이며 프로젝트 전체에서 고유합니다. |
| `target` | 엔트리가 속한 엔진 소유 대상 — `item.rusty_key`, `place.lab-b2`, `npc.vesna`입니다. 문자, 숫자, `_`, `-`로 이루어지고 `-`로 시작하지 않는 이름을 `.`으로 연결합니다. 계기의 대상 도메인이 종류의 멤버를 나열하면 그 멤버 중 하나여야 하며, 엔트리 비트에서는 `kind:<kind>`도 가능합니다. 이는 종류의 모든 멤버를 뜻합니다(dsl 0.26.0 §5). |
| `category` | 텍스트의 종류 — `note`, `item`, `place`, `codex`, `bark`, `key-item`, … (엔진 어휘이며 엔진이 쓰는 철자를 그대로 사용)입니다. |
| `title` | 퀘스트 제목처럼 현지화되는 표시 제목입니다. |
| `series` / `order` | 여러 부분으로 된 텍스트입니다. `order`는 `series` 안의 위치입니다(파일 수준 `series:`가 둘 다 제공할 수 있음 — 아래 참고). |
| `when` | 자격 조건입니다. 이 조건이 성립하는 동안에만 엔트리가 제시될 수 있습니다. |
| `on` / `priority` / `once` | 엔트리를 [비트](/ko/language/beats/)로 만들어 엔진 계기에 응답하게 합니다. `once="run"` 또는 `once="user"`는 읽은 뒤 다시 응답하지 않게 하며, [시계](/ko/language/clock/)가 있는 프로젝트에서는 `once="day"`, `once="week"`(시계의 `week:`와 함께), `once="slot"`이 시계의 날, 주 또는 슬롯이 바뀔 때까지 멈춥니다. `once="season:<name>"`은 선언된 [시즌](/ko/state/schemas/#시즌)이 다시 열릴 때까지 멈춥니다. |
| `spentBy` | 엔트리 비트에서 이 조건이 성립할 때까지 엔트리가 계속 응답하고, 그 뒤에는 다시 거짓이 되어도 `once` 기간 동안 소진된 상태로 유지됩니다(`run`이 기본값). [해결될 때까지](/ko/language/beats/#해결될-때까지-spentby)를 참고하세요. |
| `for` | 대상 없는 `select: sequence` 계기의 엔트리 비트에서 `for="kind:<kind>"`(dsl 0.27.0 §3)는 `when`이 성립하는 멤버마다 한 번씩 응답하며, 그 멤버를 `occasion.target`으로 읽습니다. [멤버마다 한 번](/ko/language/beats/#멤버별-1회-for)을 참고하세요. |

`target`과 `category`는 형태만 검사하므로 엔진의 아이템 카탈로그가 아직 없어도 로어를 작성할 수 있습니다. 여러 엔트리가 하나의 `target`을 공유할 수 있습니다 — 예를 들어 NPC의 말버릇입니다. 유일한 예외는 계기가 [대상 도메인](/ko/language/beats/#대상-도메인)을 선언한 엔트리 비트입니다. 이때 `target`은 해당 도메인의 `<prefix>.<member>`여야 합니다. 아무 대상 없이 선언된 계기에서 엔트리 비트의 `target`은 메타데이터입니다(dsl 0.24.0 §6). 여전히 엔트리가 무엇에 관한 것인지 말하지만, 계기가 발생할 때마다 응답합니다. 0.24.0 전에는 이것이 `E-BEAT-ATTR`이었고, 씬이나 번들 비트에서는 여전히 그렇습니다.

엔트리 비트는 대상 도메인이 있는 계기에서 전체 종류를 대상으로 할 수도 있습니다(`target="kind:bugRare"`). 씬 또는 번들 비트와 마찬가지로 발생한 멤버를 `when`과 본문에서 `occasion.target`으로 읽을 수 있습니다([종류 대상](/ko/language/beats/#kind-대상) 참고).

## 문서가 번들입니다

작성자는 읽는 순서대로 시리즈를 씁니다: 1페이지부터 7페이지까지 위에서 아래로, 한 파일에 담습니다. 로어 문서는 이를 직접 선언할 수 있습니다(dsl 0.19.0 §2.1).

- **`id:`**는 파일을 번들로 이름 붙입니다 — `haven.purserLedger`, `haven.captainsLog`. 씬의 `id:`와 같은 형태 규칙(`E-META-ID`)을 따르고, 아티팩트의 `meta.id`와 `project.index.json`의 문서 키가 되며, 씬 및 퀘스트 문서 id와 프로젝트 전체 네임스페이스를 공유합니다(`E-CONN-EPISODE-ID-DUP`). 퀘스트 문서도 하나를 선언할 수 있으며, 없으면 문서가 처음 선언한 퀘스트 또는 엔트리 id를 키로 사용합니다. `<beat>`를 담은 로어 문서는 모든 비트 id에 접두사를 붙여야 하므로 반드시 선언해야 합니다.
- **`series:`**는 파일의 모든 엔트리를 하나의 시리즈로 만들고, **파일 내 위치**(1부터 시작) 순서로 정렬합니다. 페이지 순서를 바꾸는 것은 엔트리를 옮기는 것이며, 페이지를 삽입하면 뒤의 엔트리 번호가 다시 매겨집니다.

```lute check="docs/examples/haven/lore/purser-ledger.lute"
---
kind: lore
luteVersion: "0.30.0"
id: haven.purserLedger
title: Purser's ledger
series: purserLedger
uses: ../world.schema.yaml
---

<entry id="purserLedger1" target="item.purser_ledger" category="note" title="Page 1">
  @ottavio{code="0010"}: Manifest amended at the third bell. Two pods logged aboard that were never loaded.
  ::assert{knows(vesna, manifest)}
</entry>

<entry id="purserLedger2" target="item.purser_ledger" category="note" title="Page 2" when="entry.purserLedger1.read">
  @ottavio{code="0010"}: If anyone reads this far: the heading on the bridge was never the true one.
  ::assert{knows(vesna, true_heading)}
</entry>
```

*([`docs/examples/haven/lore/purser-ledger.lute`](https://github.com/journeyWorker/lute/blob/main/docs/examples/haven/lore/purser-ledger.lute)에서 가져왔습니다.)*

`purserLedger1`은 `purserLedger` 시리즈의 `order` 1로, `purserLedger2`는 `order` 2로 컴파일됩니다. 컴파일된 `entry` 레코드와 `project.index.json`에는 해석된 `series` / `order`가 담기므로 어떤 형식으로 작성했든 엔진은 모든 엔트리를 같은 방식으로 읽습니다.

엔트리별 `series=` / `order=` 속성(이 페이지의 첫 번째 예제)은 여러 파일에 걸친 시리즈에 사용하는 형식으로 남아 있습니다. `series:`를 선언한 문서 안에서 자체 `series=` 또는 `order=`를 가진 엔트리는 `E-ENTRY-ATTR`입니다 — 한 파일에서 두 형식을 섞을 수 없습니다. 같은 `(series, order)`로 해석되는 두 엔트리는 `E-ENTRY-SERIES-ORDER`입니다. `lute check`에서는 문서별로, 프로젝트 전체 `lute check`에서는 프로젝트 전체에서 검사합니다.

## 엔트리 본문에 포함할 수 있는 것

콘텐츠 줄, `<match>`, `::set`, `::assert`, `::retract`입니다. `<branch>`나 `<hub>`는 안 됩니다(읽기에는 플레이어 선택이 없음). `<timeline>`이나 다른 `::` 디렉티브도 안 됩니다(엔트리 표시 방식은 엔진이 소유). `<on>`이나 `<objective>`도 안 됩니다. 그 밖의 것은 모두 `E-GRAMMAR-NOT-ADMITTED`입니다. 선택지가 있는 스토리는 씬 또는 같은 파일의 [`<beat>`](#한-파일에-엔트리와-비트)에 속합니다.

한 종류의 플러그인 디렉티브는 예외입니다(dsl 0.27.0 §4). [선언된 효과](/ko/plugins/manifests/#디렉티브-효과), 즉 디렉티브가 `writes`하는 상태와 `asserts` 또는 `retracts`하는 팩트만 동작이며 브리지 호출, 결과 슬롯, 스테이징 레이어 또는 lowering이 없는 디렉티브입니다. 일기 페이지를 읽으면 플레이어의 sanity가 감소할 수 있습니다.

```lute
<entry id="diary" target="item.diary" category="note">
  ::fright{amount=3}
  @narrator: The diary's last page is wet. Sanity {{run.sanity}}.
</entry>
```

이러한 호출은 엔트리 고유의 `::set`처럼 적용됩니다. 즉 첫 번째 읽기에만 적용됩니다([두 번 읽기](#두-번-읽기) 참고). 엔트리 안의 다른 디렉티브는 여전히 `E-GRAMMAR-NOT-ADMITTED`입니다.

줄은 일반 콘텐츠 줄입니다. 화자는 `@narrator` 또는 `@scientist` 같은 세계 안의 작가일 수 있으며, 퀘스트와 마찬가지로 각 엔트리는 자체 lineId / voiceKey 범위를 얻습니다.

## 지식 밝히기

엔트리를 읽으면 스키마가 이미 선언한 관계에 대해 일반 `::assert`로 지식이 드러납니다 — 검사기는 일반적인 arity, 도메인, tier 규칙을 적용합니다. 씬과 퀘스트는 `holds(…)`로 반응합니다.

```lute
@eris{when="holds('knows', ['vesna', 'project_lumen'])"}: ...So you read the log.
```
## 두 번 읽기

런에서 엔트리가 제시되는 **첫 번째** 순간에는 엔트리의 `::set` / `::assert` / `::retract`가 적용되고, 엔트리가 호출하는 효과 전용 디렉티브의 효과도 적용됩니다. 그 뒤 엔진은 **`entry.<id>.read`**를 `true`로 설정하고, 이후 읽기에서는 텍스트만 표시합니다 — 그때는 `<match>`가 다른 arm을 고를 수 있지만 다른 것은 바뀌지 않습니다. 다시 읽을 때 `lute trace`와 `lute play`는 적용하지 않은 효과를 `skipped`로 나열합니다.

따라서 같은 런에서 다시 읽은 엔트리는 첫 번째에만 상태를 바꿉니다. 이 규칙은 엔트리에 도달한 방식이 아니라 읽기에 따라 적용되므로, 한 런에서 두 번 이상 읽을 수 있는 모든 엔트리를 포괄합니다.

| 엔트리 | 런에서 다시 읽는 경우 | 쓰기가 적용되는 때 |
|---|---|---|
| 조회 엔트리(`on=` 없음): 코덱스 페이지, 아이템 설명 | 엔진이 열 때마다 | 런에서 첫 번째 읽기 |
| `once` 없는 엔트리 비트(또는 `once="false"`) | 계기가 발생할 때마다 | 런에서 첫 번째 읽기 |
| `once="day"`, `"slot"`, `"week"`, 또는 `"season:<name>"` | 새 기간마다 | 기간마다가 아니라 런에서 첫 번째 읽기 |
| `spentBy="…"` | 조건이 성립할 때까지 | 런에서 첫 번째 읽기 |
| `once="run"` / `"user"` 없는 `for="kind:<kind>"` | 발생 또는 기간마다 멤버별로 | 런에서 각 멤버의 첫 번째 읽기 |

`once="run"` 또는 `once="user"`인 엔트리는 런마다 최대 한 번( `for=`를 사용하면 멤버마다) 읽히므로 쓰기가 항상 적용됩니다. 그 밖의 엔트리에서는 첫 번째 `::set`, `::retract`, 또는 쓰기를 수행하는 효과 전용 디렉티브에 `lute check`가 `W-ENTRY-WRITE-REREAD`를 경고하고, 해당 엔트리 형태에서 작동하는 방법을 명시합니다. `once`가 없고 `for="kind:npc"`인 `morning`의 `greet` 엔트리는 다음과 같습니다.

<!-- lute-diagnostics unverified="verbatim lute check output; the message joins its remedies at runtime (crates/lute-check/src/lore.rs Reread::message), so no single format! literal spans it" -->
```
./lore/barks.lute:9:3: warning [W-ENTRY-WRITE-REREAD] `<entry id="greet">` is read per member, and with no `once` every raise of `morning` can present a member again in a run, but its `::set` applies on each member's first read in a run only — later reads skip it; to apply it on every presentation, put the body in a `<beat on="morning" for="kind:npc" once="false">` — a beat's writes apply each time it is presented; for one application per member per run, write `once="run"` on the entry; to keep the write to the first read on purpose, guard it with `when="!entry.greet.read"`
```

`::assert`는 예외입니다. 어느 경우든 팩트가 런의 나머지 동안 성립합니다.

- **쓸 때마다 쓰기가 일어나야 합니다.** 본문을 같은 `on`, `target`, `for`, `once`를 가진 [`<beat>`](#한-파일에-엔트리와-비트)에 넣습니다. 비트의 쓰기는 제시할 때마다 적용됩니다. 기본값을 주의하세요. `once` 없는 `<entry>`는 기본적으로 반복 가능하지만 `once` 없는 `<beat>`는 런 동안 소진되므로, 반복되는 비트에는 `once="false"`를 씁니다(`<beat on="talk" target="npc.tomas" once="false">`, `<beat on="morning" for="kind:npc" once="false">`).
- **런마다 한 번이 의도였습니다.** 엔트리에 `once="run"`을 씁니다.
- **쓰기가 의도적으로 첫 읽기에만 속합니다.** `when="!entry.<id>.read"`로 guard합니다. `::set`에 직접 달거나 `<match subject="entry.<id>.read">`의 `is="false"` arm에 달 수 있습니다. 이 guard는 독자에게 의도를 알리고 경고를 없앱니다.

`entry.<id>.read`는 어떤 문서에서도 읽을 수 있는 예약된 `bool`입니다. 시리즈의 다음 페이지를 gate하거나(`when="entry.scientistLog1.read"`), 씬을 그에 따라 분기하거나, 퀘스트 목표를 완료할 수 있습니다. 콘텐츠는 이를 쓰지 않습니다. 어떤 문서도 id를 선언하지 않으면 프로젝트 전체 `lute check`가 `W-ENTRY-REF-UNKNOWN`을 경고합니다.

이 플래그는 **run-tier**입니다. 새 런이 이를 초기화하고 다음 첫 읽기에서 효과가 다시 적용됩니다. 따라서 `when="!entry.<id>.read"`는 엔트리를 영원히 한 번이 아니라 런마다 한 번 플레이하게 합니다.

사용자 tier 쌍인 **`entry.<id>.everRead`**(dsl 0.22.0)는 "플레이어가 이 엔트리를 *한 번이라도* 읽었는가?"에 답합니다. 엔진은 `entry.<id>.read`를 설정할 때마다 이 값도 설정하므로, 처음 읽은 순간 true가 되고 새 런에서도 초기화되지 않습니다. `entry.<id>.read`와 같은 방식으로 예약되어 읽을 수 있지만 콘텐츠가 쓸 수 없으며, 그 쌍과 마찬가지로 `W-ENTRY-REF-UNKNOWN`으로 해석됩니다. 두 플래그를 함께 사용하면 나중 런을 첫 런과 구분할 수 있습니다.

```lute
<entry id="tomaDejaVu" target="npc.toma" category="bark" when="entry.scientistLog1.everRead && !entry.scientistLog1.read">
  @toma: You have that look again. As if you already know what the log says.
</entry>
```

`tomaDejaVu`는 플레이어가 로그를 처음 읽은 런의 다음 런에서만 자격이 있고, 로그를 다시 찾을 때까지만 자격이 있습니다. 그 자체로 `when="!entry.<id>.everRead"`를 쓰면 영원히 한 번 플레이됩니다.

| 플래그 | Tier | 설정 시점 | 초기화 |
|---|---|---|---|
| `entry.<id>.read` | run | 런에서 엔트리가 처음 제시된 뒤 | 새 런마다 |
| `entry.<id>.everRead` | user | 엔트리가 처음 제시된 뒤 | 없음 |

엔트리 비트는 이 플래그로 직접 자신을 소진시킬 수도 있습니다. `once="run"`은 `read`가 설정되어 있는 동안 계기에 응답하지 않게 하고, `once="user"`는 `everRead`가 설정되면 그렇게 합니다. `once="day"`와 `once="slot"`(dsl 0.24.0 §1, 선언된 [시계](/ko/language/clock/)가 있을 때만)은 대신 시간에 따라 소진합니다. 플래그 값과 상관없이 시계의 날 또는 슬롯이 바뀔 때까지 다시 응답하지 않습니다. [비트](/ko/language/beats/)를 참고하세요.

### 읽기가 증명하는 것

엔트리가 읽혔다고 가정하는 guard는 그 엔트리가 무엇을 assert했는지도 알 수 있습니다(dsl 0.24.0 §6). `entry.X.read`(또는 `entry.X.read == true`) 아래에서 프로젝트 전체 `lute check`는 모든 경로에서 `X`의 본문이 assert하고 아무것도 retract하지 않는 팩트를, [팩트 분석](/ko/state/facts-and-datalog/)의 알고 있는 Must 집합에 추가합니다. 그러면 그 guard 안에 있는 그 팩트의 `holds(…)`는 중복으로 간주되어 `W-FACT-GUARANTEED`가 되고, 그 부정은 절대 성립할 수 없어 줄에서 `E-ARM-DEAD`가 됩니다. 이 페이지의 첫 문서를 `lore/ship-records.lute`로 저장하고 `scientistLog2`에 두 번째 줄을 추가해 보세요.

```lute
<entry id="scientistLog2" target="item.torn_note_2" category="note" series="scientistLog" order="2" when="entry.scientistLog1.read">
  @scientist: Day nine. We stopped writing her name in the logs.
  @scientist{when="holds('knows', ['vesna', 'project_lumen'])"}: You know what Lumen was. Now you know who.
</entry>
```

<!-- lute-diagnostics -->
```
./lore/ship-records.lute:20:20: warning [W-FACT-GUARANTEED] guard `holds('knows', ['vesna', 'project_lumen'])` is redundant: `knows(vesna, project_lumen)` is asserted on every route to here (./lore/ship-records.lute:15)
```

엔트리의 `when`이 이미 이 런에서 로그를 읽었다는 것을 보장하므로 줄의 guard는 항상 true입니다. `entry.X.everRead`는 더 적은 것을 약속합니다. 새 런에도 유지되는 `tier: user`와 `tier: app` 관계가 assert한 팩트만 추가합니다. 위의 `knows`는 `tier: run`이므로 `tomaDejaVu`처럼 `everRead`만 있는 상태에서는 `holds(…)`가 여전히 미결정입니다.

## 한 파일에 엔트리와 비트

인터뷰, 짧은 NPC 순간, 로그를 집어 들 때 누군가가 하는 말은 엔트리가 아니라 씬 비트입니다. 플레이어가 읽는 대신 플레이하기 때문입니다. 또한 몇 줄뿐입니다. 순간마다 씬 파일을 하나씩 만드는 대신 로어 문서는 엔트리 옆에 `<beat>` 블록으로 이를 담을 수 있습니다(dsl 0.23.0).

```lute check
---
kind: lore
id: shipRecords
title: Ship's records
---

<entry id="bridgeLog" target="item.bridge_log" category="note" title="Bridge log">
  @narrator: The heading was changed eleven years ago.
</entry>

<beat id="tomaAtTheLog" on="examine" target="item.bridge_log" title="Toma at the log" when="entry.bridgeLog.read">
  @toma: You found it too.
  <branch id="heading">
    <choice id="tell" text="Tell her what it says">
      @toma: Then we are not going home.
    </choice>
    <choice id="hide" text="Say it is nothing">
      @toma: Your hands say otherwise.
    </choice>
  </branch>
</beat>
```

두 종류의 블록은 파일을 공유하지만 각각의 규칙은 유지합니다.

| | `<entry>` | `<beat>` |
|---|---|---|
| Id | 자체 `id`, 프로젝트 전체에서 고유 | `<document id>.<beat id>`: `shipRecords.tomaAtTheLog` |
| 본문 | 콘텐츠 줄, `<match>`, `::set` / `::assert` / `::retract`, 효과 전용 플러그인 디렉티브 | 씬 본문: 줄, branch, hub, `<match>`, 디렉티브 |
| 도달 | 엔진이 조회하거나 [엔트리 비트](/ko/language/beats/#엔트리-비트)로 도달 | `on` 계기에 응답하는 비트로만 도달하며 `when`과 (dsl 0.25.0) `after=`가 성립해야 함 |
| 효과 | 런에서 첫 읽기에 적용(`for=`를 쓰면 멤버별; [두 번 읽기](#두-번-읽기) 참고) | 씬처럼 제시할 때마다 적용 |
| 소진 | `once`가 없으면 기본적으로 절대 소진되지 않고 반복 가능. `once=`가 있으면 `entry.<id>.read` / `everRead`로 소진(시계의 `"day"` / `"week"` / `"slot"`, 시즌의 `"season:<name>"`), 또는 `spentBy=`로 소진 | 제시할 때 또는 `spentBy=`로 소진; `once` 기본값은 `run` |
| 공유 소진(dsl 0.25.0) | `once=` 옆에 `share=`: 해당 키의 모든 비트를 읽으면 소진 | `once` 옆에 `share=`: 해당 키의 모든 비트를 제시하면 소진 |
| 조건에서 읽기 | `entry.<id>.read`, `entry.<id>.everRead` | `visited('<document id>.<beat id>')` |

엔트리와 비트는 어떤 순서로든 올 수 있습니다. 컴파일된 로어 아티팩트에서 각각은 소스 순서대로 자신의 `position` 단위를 이룹니다. 첫 세그먼트는 `001`, `002`, …이며 `entry` 또는 `beat` 레코드를 담고, 이어지는 본문은 다음 `entry` 또는 `beat` 레코드까지 계속됩니다. 비트 본문의 lineId는 정식 id 아래에 만들어집니다(`shipRecords.tomaAtTheLog.toma_0010`). [비트](/ko/language/beats/#비트-번들) 페이지에서 `<beat>` 속성, 정식 id, 번들 비트를 검사하고 선택하는 방법을 설명합니다.

## 도구

- `lute trace <doc> --entry <id> --mock m.yaml`은 mock 상태에 대해 하나의 엔트리를 미리 봅니다. 다시 읽기를 미리 보려면 `entry.<id>.read: true` 또는 `entriesRead: { run: [<id>] }`를 seed합니다. `run:` id는 `entry.<id>.read`와 `entry.<id>.everRead`를 모두 설정합니다(이번 런에서 읽은 것은 ever에도 읽힌 것입니다). `entriesRead: { user: [<id>] }`는 `entry.<id>.everRead`만 설정합니다. 둘 다 문서가 선언한 어떤 엔트리에도 사용할 수 있습니다. `once="run"` 또는 `once="user"`인 엔트리는 해당 플래그로 소진되므로 `trace`와 `test`에서 자격이 없으며 `lute play`에서도 동일합니다. `lute run <artifact> --entry <id>`는 컴파일된 로어 아티팩트에 대해 같은 일을 합니다.
- `lute trace <doc> --beat <id> --mock m.yaml`은 로컬 또는 정식 id로 하나의 `<beat>`를 미리 봅니다. mock의 `choose:`가 branch를 고릅니다. `lute run <artifact> --beat <id>`는 컴파일된 로어 아티팩트에 대해 같은 일을 합니다. 비트의 `when`과 `after=`는 표시하지만 강제하지는 않습니다. 어떤 비트도 지정하지 않는 id는 `trace`에서 `E-TRACE-BEAT`, `run`에서 사용법 오류가 되며, 둘 다 문서의 비트를 나열합니다. [트레이싱](/ko/tooling/tracing/#번들-비트)을 참고하세요.
- `lute test`는 로어 문서도 테스트합니다(dsl 0.22.0). 테스트는 `entry: <id>` 또는 `entries: [ids]`로 제시할 엔트리를 지정합니다. 지정된 순서로 제시하며 그 사이에 읽기 플래그를 설정하므로 같은 id를 반복하면 다시 읽기가 됩니다. 이 페이지의 첫 문서를 `lore/ship-records.lute`로 저장했다면:

  ```yaml
  file: ../lore/ship-records.lute
  entries: [scientistLog1, scientistLog2]
  expect:
    transcriptContains: ["Day nine. We stopped writing her name in the logs."]
  ```

  아무 엔트리도 지정하지 않는 로어 테스트는 `E-TEST-LORE`가 되며, 메시지에 문서의 엔트리 id를 나열합니다. 로어도 테스트할 수 있으므로 `lute test --coverage`는 테스트되지 않은 다른 문서와 함께 테스트되지 않은 로어 문서도 나열합니다.

  dsl 0.26.0 §8부터 테스트(`entry:`, `entries:`, `expect.eligible` 키), 플레이 스크립트(`pick:`, `entriesRead:`, 스텝의 `winner` / `offered` / `notOffered` / `presented`), `lute trace --entry`는 모두 `<document id>.<entry id>`를 엔트리 id의 별칭으로도 받습니다. `entry: shipRecords.bridgeLog`는 이 페이지에서 `id:`가 `shipRecords`인 문서의 `bridgeLog`를 지정합니다. [종류](/ko/language/beats/#kind-대상)를 대상으로 하는 엔트리 또는 비트 테스트는 `state: { occasion.target: <member> }`로 발생한 멤버를 지정합니다.
- `lute lore --project <dir>`는 세계 서사 맵을 출력합니다. target별 및 series별 엔트리를, 각 엔트리와 비트의 `when`, 엔트리가 밝히는 팩트와 씬 및 퀘스트가 밝히는 팩트를 함께 보여 줍니다. 비트 본문도 읽습니다. 프로젝트 규칙이 관계를 derive하면 **Derived** 섹션에 규칙이 도달할 수 있는 모든 결론, 그 뒤의 규칙 인스턴스, 근거가 되는 증거(각 전제를 assert한 엔트리 또는 씬), 그리고 그 결론이 gate하는 엔트리와 비트를 나열합니다(`--json`에서는 `derived`). [CLI 레퍼런스](/ko/tooling/cli/)를 참고하세요.
- `lute new lore <name>`은 로어 문서 뼈대를 만듭니다.

엔진 계약 — 자격, 제시, 첫 읽기 효과, 두 읽기 플래그 — 은 [`docs/runtime/lore-entries.md`](https://github.com/journeyWorker/lute/blob/main/docs/runtime/lore-entries.md)에 있고, 규범 사양은 다음과 같습니다:
[`0.19.0.md`](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.19.0.md),
`everRead`와 로어 테스트를 위한
[`0.22.0.md`](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.22.0.md),
로어 문서의 비트를 위한
[`0.23.0.md`](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.23.0.md),
엔트리 target을 메타데이터로 취급하고 읽기가 증명하는 내용을 위한
[`0.24.0.md`](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.24.0.md)
§6, 종류 target, `W-ENTRY-WRITE-REREAD`, `<document id>.<entry id>`를 위한 초안
[`0.26.0.md`](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.26.0.md)입니다.
