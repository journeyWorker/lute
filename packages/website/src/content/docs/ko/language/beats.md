---
title: 비트
description: "엔진 계기에 응답하는 장면, 로어 엔트리, 번들 <beat> 블록 — 장면 프런트매터 키 on, target, when, priority, once, also, spentBy, for, 엔트리 속성 on=, priority=, once=, spentBy=, for=, 로어 문서의 비트 번들, 계기 대상 도메인과 kind 대상, 멤버별 1회 비트(for=), 계기 페이로드, select: first / all / sequence, 적격 비트의 정렬 및 표시 방식, 체커가 비트에 대해 증명하는 내용(E-BEAT-ATTR, E-OCCASION-UNKNOWN, E-BEAT-UNREACHABLE, W-BEAT-SHADOWED, W-BEAT-PRIORITY-TIE, W-BEAT-ONCE-RUN-USER, W-ENTRY-WRITE-REREAD)."
---

많은 게임은 시계에 따라 진행되지 않습니다. 허브 방문, 방에 들어가기, NPC와 대화하기, 새로운 날의 시작,
런의 시작처럼 무언가가 일어날 때 이야기가 움직입니다. 그 순간 게임은 조건을 만족하는 이야기 한 조각을
고르거나, 아무것도 고르지 않습니다.

Lute는 이런 순간을 **계기(occasion)**, 그에 응답하는 이야기 조각을 **비트(beat)** 라고 부릅니다(dsl
0.21.0). 엔진은 계기를 발생시킵니다. Lute는 각 계기에 응답하는 비트, 각 비트의 적격 조건,
그리고 적격 비트 중 승자를 선언합니다. 장면, [로어 엔트리](/ko/language/lore-entries/), 또는 로어 문서의
`<beat>` 블록([번들 비트](#비트-번들))은 자신이 응답할 계기를 지정하면 비트가 됩니다. 문서의
나머지 내용은 그대로입니다.

## 장면 비트

```lute check
---
kind: scene
id: achilles.gift
on: talk
target: npc.achilles
when: 'user.runs >= 10 && !run.giftRefused'
priority: 50
once: user
state:
  user.runs: { type: int, default: 0 }
  run.giftRefused: { type: bool, default: false }
---

## Opening {#opening}

@achilles: You have been at this a while, lad. Take this.
```

엔진이 `npc.achilles`에 대해 `talk`를 발생시키면 이 장면은 후보가 됩니다. 플레이어가 런을 열 번
시작했고 이번 런에 선물을 거절하지 않았을 때 적격입니다. 처음 재생되는 순간 `once: user`가 영구적으로
소진시키므로 이전에 표시된 적은 없습니다.

## 장면 비트 키

| 키 | 의미 |
|---|---|
| `on` | 이 장면이 응답하는 계기. 장면을 비트로 만드는 키입니다 |
| `target` | 선택 사항. 해당 대상에 대해 계기가 발생할 때만 장면이 후보가 됩니다. `<entry target>` 형태의 점 표기 id(`npc.achilles`, `place.lab_b2`)입니다. 계기가 [대상 도메인](#대상-도메인)을 선언하면 해당 도메인의 `<prefix>.<member>`여야 하며, `members:`를 나열한 경우 그중 하나여야 합니다. `kind:<kind>`(dsl 0.26.0 §5)는 kind의 모든 멤버에 응답합니다. [Kind 대상](#kind-대상)을 참조하세요 |
| `when` | 선택 사항. `run` / `user` / `app` 상태, `quest.*`, `entry.<id>.read` / `entry.<id>.everRead`, 팩트 쿼리(`holds(…)`, `count(…)`)에 대한 CEL 조건입니다 |
| `priority` | 선택 사항인 정수, 기본값 `0`. 클수록 우선합니다 |
| `once` | `run`(기본값: 런마다 최대 한 번), `user`(전체 기간 최대 한 번), `false`(반복 가능)입니다. [시계](/ko/language/clock/)가 있는 프로젝트에서는 `day`(시계의 하루마다 최대 한 번), `slot`(시계 슬롯마다 최대 한 번), 그리고 시계가 `week:`를 선언할 때 `week`(시계 주마다 최대 한 번, dsl 0.27.0 §5)도 사용할 수 있습니다. 선언된 [시즌](/ko/state/schemas/#시즌)이 있는 프로젝트에서는 `season:<name>`(해당 시즌의 각 기간마다 최대 한 번)도 사용합니다 |
| `also` | 선택 사항인 `true` / `false`, 기본값 `false`(dsl 0.23.0)입니다. `select: first` 계기에서 `also` 비트는 곁가지 발언으로, 경쟁하지 않고 승자 뒤에 표시됩니다. [다음 `also`로 곁가지 발언](#also로-곁가지-발언)을 참조하세요 |
| `share` | 선택 사항인 프로젝트 전체 키이며, 소진하는 `once` 옆에 씁니다(dsl 0.25.0 §2). 같은 키를 가진 모든 비트가 함께 소진됩니다. [한 이벤트, 여러 장소](#한-이벤트-여러-장소-share)를 참조하세요 |
| `spentBy` | 표시 대신 비트를 소진시키는 선택적 CEL 조건입니다. 한 번 조건이 성립하면 `once` 기간(`run`, 별도로 쓰지 않은 경우) 동안 비트는 소진 상태로 유지됩니다. [해결될 때까지: `spentBy`](#해결될-때까지-spentby)를 참조하세요 |
| `for` | 선택 사항인 `"kind:<kind>"`입니다. 대상이 지정되지 않은 `select: sequence` 계기에서 `target` 대신 사용합니다(dsl 0.27.0 §3). `when`이 성립하는 각 멤버마다 한 번씩 장면을 표시하며, 멤버는 `occasion.target`으로 읽습니다. [멤버별 1회](#멤버별-1회-for)를 참조하세요 |

비트 키는 장면 전용이며 프로젝트 `defaults:`에서 가져오지 않습니다. 비트는 하나의 장면에 속합니다.

`when`은 일반적인 CEL 슬롯이며, 퀘스트의 `start`와 같은 방식으로 검사됩니다. 프로필의 CEL 표면,
명확한 할당, 미설정 센티널 규칙이 모두 적용됩니다. 한 가지 추가 규칙은 `when`이 장면 자체의
`scene.*` 상태를 읽을 수 없다는 것입니다. 이 상태는 장면이 시작되기 전에는 존재하지 않습니다.

`after:`는 의미가 그대로입니다. [연결성](/ko/connectivity/scene-graph/)이 분석하는 `visited` / `completed` /
`active`에 대한 구조적 선행 조건입니다. 비트는 **`after:`와 `when`이 모두** 성립할 때만 적격이므로,
경로 순서는 `after:`에, 상태 조건은 `when`에 넣으세요.

`on:`이 없는 장면은 어떤 계기에도 응답하지 않으므로 계기가 표시하지 않습니다. 계기가
생기기 전 모든 장면을 시작하던 방식처럼 엔진이 장면 id로 직접 시작합니다. `lute trace`와 `lute run`은
그런 장면을 단독으로 순회하고, `lute play`는 절대 표시하지 않습니다. `lute play`로 재생할 수 있는
이야기에 장면을 연결하려면 [장면을 이야기로 연결하기](/ko/getting-started/connect-scenes/)를 참조하세요.
`on` 없이 `when`, `target`, `priority`, `once`, `also`, `share`, `spentBy`, `for`를 사용하면 오류입니다.

```lute expect="E-BEAT-ATTR"
---
kind: scene
id: courtyard.rumor
priority: 5
---

## Opening {#opening}

@narrator: A rumor drifts across the courtyard.
```

## 엔트리 비트

로어 엔트리는 기존 `target` 및 `when` 옆에 `on=`, `priority=`, `once=`를 지정하여 계기에 응답합니다.

```lute check
---
kind: lore
title: Achilles barks
state:
  user.runs: { type: int, default: 0 }
---

<entry id="achillesBark1" on="talk" target="npc.achilles" category="bark">
  @achilles: Keep your guard up.
</entry>

<entry id="achillesBark3" on="talk" target="npc.achilles" category="bark" priority="10" when="user.runs >= 3">
  @achilles: Back again, lad.
</entry>

<entry id="achillesMorning" on="talk" target="npc.achilles" category="bark" priority="20" once="run">
  @achilles: Up early again? Mind the stairs.
</entry>

<entry id="achillesFirstMeeting" on="talk" target="npc.achilles" category="bark" priority="30" once="user">
  @achilles: So you are the one who keeps coming back.
</entry>
```

`once`가 없으면 엔트리 비트는 반복 가능합니다. 엔트리를 다시 읽는 것은 정상입니다. NPC가 같은 말을
반복하거나, 코덱스 페이지가 계속 열려 있는 경우가 그렇습니다. `once`(dsl 0.22.0)는 엔트리를 소진 가능하게
만듭니다. `run`과 `user`는 표시 기록이 아니라 자체 읽기 플래그로 엔트리를 소진합니다.

| `once` | 다음 상태에서는 적격 아님 | 다시 적격이 되는 때 |
|---|---|---|
| 없음 | 한 번도 소진되지 않음 | — |
| `run` | `entry.<id>.read`가 설정됨: 이번 런에 읽힘 | 다음 런. `read`가 재설정됨 |
| `user` | `entry.<id>.everRead`가 설정됨: 어떤 런에서든 읽힘 | 없음 |
| `day` | 이번 시계 날짜에 앞서 표시됨 | 다음 날 |
| `week` | 이번 시계 주에 앞서 표시됨 | 다음 시계 주가 시작될 때 |
| `slot` | 이번 시계 슬롯에 앞서 표시됨 | 다음 슬롯 |
| `season:<name>` | 시즌이 마지막으로 열린 뒤 표시됨 | 시즌이 다시 열릴 때 |

엔진은 엔트리가 어떤 방식으로 표시되었든 런에서 처음 읽힌 뒤 두 플래그를 모두 설정합니다. 따라서 엔진이
`target`으로 조회한 엔트리도 `run` 또는 `user` `once`를 소진합니다. 플래그 자체는 [두 번 읽기](/ko/language/lore-entries/#두-번-읽기)를 참조하세요. `day`와 `slot`(dsl 0.24.0 §1)에는 선언된 [시계](/ko/language/clock/)가 필요합니다. 이 둘은 읽기가 아니라 시간을 셉니다. 엔진은 장면 비트와 마찬가지로 엔트리가 마지막으로 표시된 시계 위치를 기억하므로 `once="slot"`인 말은 `entry.<id>.read`가 여전히 설정되어 있어도 다음 슬롯에 다시 응답합니다.

엔트리의 `::set`과 `::retract`는 **런에서 처음 읽을 때**만 적용됩니다(`for=`를 사용하면 각 멤버의 첫 읽기). 같은 런에서 나중에 표시하면 텍스트만 보이고 아무것도 변경하지 않습니다. 따라서 쓰기를 수행하는 반복 가능한 엔트리 비트는 1에서 멈추는 카운터이며, 런보다 짧은 `once` 또는 `spentBy`를 사용하는 엔트리도 마찬가지입니다. `lute check`는 첫 번째 쓰기에서 **`W-ENTRY-WRITE-REREAD`** 경고를 내고 엔트리 형태에 맞는 해결책을 표시합니다. `::assert`는 어느 쪽이든 나머지 런 동안 팩트가 유지되므로 예외입니다. `achillesBark1`에 `::set{run.nagged += 1}`을 추가해 보세요.

<!-- lute-diagnostics unverified="verbatim lute check output; the message joins its remedies at runtime (crates/lute-check/src/lore.rs Reread::message), so no single format! literal spans it" -->
```
./lore/barks.lute:9:3: warning [W-ENTRY-WRITE-REREAD] `<entry id="achillesBark1">` has no `once`, so every raise of `talk` can present it again in a run, but its `::set` applies on the first read in a run only — later reads skip it; to apply it on every presentation, put the body in a `<beat on="talk" target="npc.achilles" once="false">` — a beat's writes apply each time it is presented; for one application per run, write `once="run"` on the entry; to keep the write to the first read on purpose, guard it with `when="!entry.achillesBark1.read"`
```

기본값은 다릅니다. `once`가 없는 `<entry>`는 반복 가능하지만, `once`가 없는 `<beat>`는 런 동안
소진됩니다. 따라서 쓰기를 반복하는 비트는 `once="false"`라고 써야 합니다. 규칙이 적용되는 모든 형태는
[두 번 읽기](/ko/language/lore-entries/#두-번-읽기)를 참조하세요.

`once`에는 `run`, `user`, `day`, `week`, `slot`, `season:<name>`을 사용합니다. `once="false"`는 속성을
생략한 것과 같으며 반복 가능한 엔트리입니다. 그 밖의 값은 `E-BEAT-ATTR`이고, 시계 없는 프로젝트에서의
`day`, `week`, `slot`도 그렇습니다(`week`에는 시계의 `week:`도 필요). `on=` 없이 `once=` 또는 `priority=`를
사용해도 같은 오류입니다. 반복 정책은 비트에 속하기 때문입니다. 어떤 스키마도 선언하지 않은 시즌은
`E-SEASON-DECL`입니다. 0.22.0 이전에는 런마다 한 번은 `when="!entry.<id>.read"`, 영구적으로 한 번은 엔트리가
설정하는 `user.*` 플래그로 조건을 적었습니다. 이 방식도 계속 작동하지만, 이제 `once`가 의도를 직접
표현합니다. 무언가가 완료될 때까지 응답해야 하는 엔트리는 [`spentBy=`](#해결될-때까지-spentby)를
사용하고(`once=`로 소진 기간을 지정), `select: sequence` 계기의 엔트리는 `for=`로 [멤버별 1회](#멤버별-1회-for)
읽을 수 있습니다. 엔트리는 다른 비트에 따라가지 않으므로 `<entry>`의 [`also`](#also로-곁가지-발언)도
`E-BEAT-ATTR`입니다. 엔트리 비트는 `once=` 옆에 [`share=`](#한-이벤트-여러-장소-share)를 사용할 수
있습니다(dsl 0.25.0 §2). 읽으면 해당 키의 모든 비트가 소진됩니다.

대상을 지정하지 않고 선언한 계기에서는 엔트리의 `target=`이 메타데이터입니다(dsl 0.24.0 §6). 엔진이
조회하는 엔트리에서와 마찬가지로 엔트리가 무엇에 관한 것인지 나타내며, 계기가 발생할 때마다 응답합니다.

```lute check
---
kind: lore
title: Shop window
---

<entry id="compassNote" on="shopVisit" target="item.compass" category="item">
  @narrator: A brass compass sits in the window, its needle trembling.
</entry>
```

`lute play`, `lute calendar`, `lute beats` 및 비트 경고는 이러한 엔트리를 대상 없는 것으로 취급합니다.
0.24.0 이전에는 `E-BEAT-ATTR`였습니다. 대상 없는 계기에 대한 장면 또는 번들 비트의 `target`은
여전히 오류입니다. 이 경우 대상은 어떤 발생에 응답할지를 제한하기 때문입니다.
## 비트 번들

인터뷰, 바에서 나누는 한 차례의 대화, 하루 동안 이어지는 짧은 NPC 순간들처럼 많은 비트는 몇 줄과
선택지 하나로 이루어집니다. 비트마다 파일을 만들면 프로젝트 곳곳으로 흩어집니다. 로어 문서는 이들을
함께 보관할 수 있습니다(dsl 0.23.0). `<beat>` 블록은 로어 문서 안에서 엔트리와 나란히 작성하는 장면
비트이며, 그 본문은 장면 본문입니다. 줄, 분기, 허브, `<match>`, 디렉티브를 포함할 수 있습니다.

```lute check
---
kind: lore
id: interviews
title: Station interviews
state:
  run.porterTrust: { type: int, default: 0 }
---

<entry id="porterNote" target="item.porter_note" category="note">
  @narrator: A note in the porter's hand: "Lamps out at ten."
</entry>

<beat id="porter" on="talk" target="npc.porter" title="The porter" priority="5">
  @porter: You again.
  <branch id="night">
    <choice id="ask" text="Ask about the night">
      @porter: I saw nothing. Nothing I'll say twice.
      ::set{run.porterTrust += 1}
    </choice>
    <choice id="leave" text="Leave him be">
      @porter: Good.
    </choice>
  </branch>
</beat>

<beat id="porterAgain" on="talk" target="npc.porter" title="The porter, again" after="visited('interviews.porter')" when="run.porterTrust >= 1">
  @porter: Fine. The lamps went out before ten.
</beat>
```

`<beat>`은 장면 비트 키를 속성으로 받습니다.

| 속성 | 의미 |
|---|---|
| `id` | 필수. 문서 안에서 고유한 이름이며 문자, 숫자, `_`, `-`를 사용할 수 있고 `-`로 시작할 수 없습니다 |
| `on` | 필수. 비트가 응답하는 계기 |
| `target`, `when`, `priority`, `once`, `also`, `share`, `spentBy`, `for` | [장면 비트](#장면-비트-키)와 같습니다. `once="false"`는 비트를 반복 가능하게 하며, `also`는 값 없이 작성할 수 있습니다 |
| `after` | 선택 사항(dsl 0.25.0 §3). 장면의 `after:` 수식이며 같은 의미를 가집니다. 즉 적격성의 결합 조건이자 [시나리오 그래프](/ko/connectivity/scene-graph/)의 간선입니다 |
| `title` | 선택 사항. `select: all` 메뉴에서 엔진이 비트에 표시하는 레이블이며 엔트리 제목처럼 현지화됩니다 |

번들 비트는 다른 파일에 존재하는 장면 비트입니다.

- 정식 **id**는 `<document id>.<beat id>`입니다. 위의 경우 `interviews.porter`이므로 문서가 `id:`를
  선언해야 합니다. 이 id는 `project.index.json`에서 해당 비트의 행이자 표시 기록의 키입니다. 프로젝트
  전체 문서 id 네임스페이스를 공유하므로, `id:`가 `interviews.porter`인 장면이 있으면
  `E-CONN-EPISODE-ID-DUP`입니다.
- `once`의 기본값은 `run`이며 장면과 마찬가지로 **표시**로 소진됩니다. 엔트리가 아니므로 읽기 플래그로는
  소진되지 않습니다. [시계](/ko/language/clock/)가 있으면 장면과 마찬가지로 `once="day"`, `once="week"`,
  `once="slot"`을 사용할 수 있고, 선언된 [시즌](/ko/state/schemas/#시즌)이 있으면
  `once="season:<name>"`도 사용할 수 있습니다.
- 표시하면 정식 id가 방문됨으로 표시되므로 어떤 조건에서든 `visited('interviews.porter')`로 읽을 수
  있습니다. dsl 0.24.0 §2부터는 유효한 `after:` 선행 노드이기도 합니다. 장면이나 퀘스트의
  `after: "visited('interviews.porter')"`로 지정할 수 있으며, 연결성은 장면처럼 이를 거쳐 경로를
  구성합니다. (0.24.0 이전에는 `E-CONN-UNKNOWN-NODE`였습니다.)
- dsl 0.25.0 §3부터는 `porterAgain`처럼 자체 `after="…"`를 선언할 수 있습니다. 수식은 장면 `after:`의
  문법과 의미를 가집니다. 즉 성립할 때만 비트가 적격입니다(`lute play`와 `lute calendar`는
  `— after: prerequisite not satisfied`로 표시하고, `lute trace --beat`는 이를 적용하지 않은 채
  비트 헤드에 ``not eligible: `after` prerequisite not satisfied``를 기록합니다). `lute scenario`는
  해당 간선을 그립니다. 잘못된 수식은 `E-CONN-PROFILE`, 어떤 문서도 선언하지 않은 노드는
  `check-project`에서 `E-CONN-UNKNOWN-NODE`입니다. 비트의 `when`에 있는 `visited()` 결합 조건은 여전히
  비트를 제한하지만 간선을 그리지 않으므로, `lute scenario`는 이러한 비트를 고립된 것으로 표시하고
  작성해야 할 `after=`를 이름으로 알려줍니다. 장면과 마찬가지로 경로 순서는 `after=`에, 상태는 `when`에
  넣으세요.
- 장면 비트와 같은 방식으로 검사합니다. `E-OCCASION-UNKNOWN`, `E-BEAT-UNREACHABLE`, 아래의
  `check-project` 경고 [다음 절](#검사기가-증명하는-내용)는 정식 id로 이름을 표시합니다.
- 줄은 정식 id 아래에서 주소가 지정됩니다. 포터의 첫 줄은 `interviews.porter.porter_0010`이고,
  제목의 `titleLineId`는 `interviews.porter.title`입니다.

형식 오류는 `E-BEAT-ATTR`입니다. `id`가 없거나 잘못되었거나 중복된 경우, 비트가 있는데 문서에
`id:`가 없는 경우, `on`이 없는 경우, `priority`, `once`, `also`, `share`, `target`이 잘못된 경우가 해당합니다.
그 밖의 속성은 `E-UNKNOWN-ATTR`이며, 장면 문서에는 `<beat>`를 작성할 수 없습니다. 엔트리와 비트는 어떤
순서로든 섞을 수 있습니다. 파일의 엔트리 측면은 [한 파일에 로어 엔트리와 비트](/ko/language/lore-entries/#한-파일에-엔트리와-비트)에서 설명합니다.

### 비트 템플릿: `use=`

영웅 명단의 각 영웅에게 유대 이야기 세 개가 있고, 수십 번 반복되는 헤더 형태가 있다고 합시다.
**비트 템플릿**은 이를 한 번만 작성합니다(dsl 0.27.0 §6). 이는 [컴포넌트](/ko/language/components-and-extends/)이며,
프런트매터에 `beat:` 헤더도 선언합니다.

```yaml
component: bondStory
params: { who: { type: string }, need: { type: int, default: 0 }, prev: { type: string, default: "" } }
beat:
  on: bond
  once: user
  when: "user.bond >= @need"
  after: "@prev"        # left out when empty
```

번들 비트는 `use=`로 템플릿 이름을 지정하고 속성으로 매개변수를 전달합니다.

```
<beat use="bondStory" id="first" who="aria">
  @aria: Thank you for staying.
</beat>

<beat use="bondStory" id="second" who="aria" need="2" prev="bonds.first">
  @aria: You came back.
</beat>
```

- `<beat>`가 직접 작성하지 않은 모든 헤더 키는 템플릿에서 가져오며, 각 `@param`은 인수의 텍스트(또는
  매개변수의 `default:`)로 치환됩니다. 결과가 빈 키는 생략되고, `after:`가 단순 id로 나오면
  `visited("<id>")`가 됩니다. 헤더 키는 `<beat>` 속성인 `on`, `target`, `for`, `title`, `priority`,
  `once`, `share`, `after`, `when`, `spentBy`, `also`입니다(`also: true`, 따옴표 없음). `title:`의
  `{{@who}}`는 인수, 즉 `speaker`의 배역 이름을 렌더링합니다.
- `when:`과 `spentBy:`에서는 `per:` 패밀리의 멤버를 컴포넌트 본문에서와 같은 방식으로 읽습니다.
  `who="aria"`일 때 `user.bond[@who]`는 `user.bond.aria`가 됩니다. 헤더에서는 점 표기
  `user.bond.@who`도 작동하며, 대괄호 사용을 권하는 `W-TEMPLATE-DOT-PARAM` 힌트를 냅니다.
- `<beat use="bondStory" … once="run">`처럼 `<beat>`에 작성한 속성이 우선합니다. 템플릿의 해당
  키 전체를 대체하므로, use에서 `when="@isNight"`를 쓰면 템플릿 자체의 `when`이 사라지고,
  `W-TEMPLATE-OVERRIDE`가 이를 알려 주며, 삭제된 조건과 그것만 읽던 인수를 이름으로 표시합니다.
  `lute beats`는 use의 자체 가드에 ``(replaces template `bondStory`'s `when: …`)``라는 메모를
  표시합니다(`--json`에서는 `replacesTemplateWhen`). 둘 다 유지하려면 use의 `when=`에 두 조건을 모두
  작성하세요. use가 조건을 추가하도록 하려면 헤더에 이를 위한 매개변수를 만들고 결합하세요:
  `only: { type: string, default: "" }` 및 `when: "!holds('defeated', [@who]) && (@only)"`를
  사용합니다. use가 `only="@isNight"`를 작성하고 비워 두거나 `true`로 두면
  `!holds('defeated', ['<who>'])`만 도출되며 `&& (true)`는 남지 않습니다.
- `@param`이 없는 헤더 값은 모든 use에서 동일합니다. 따라서 알 수 없는 계기나 잘못된 `once:` 같은
  오류는 컴포넌트의 헤더 키에서 한 번만 보고되며, use에는 해당 키가 도출되지 않습니다.
- 템플릿 자체의 본문이 먼저 실행되고, 그다음 비트의 본문이 실행됩니다. 템플릿 본문 최상위에
  `::body` 줄을 두면 그 위치에 비트 본문이 대신 들어가므로 템플릿으로 틀을 만들 수 있습니다. 템플릿
  본문에는 `## ` 헤딩이 필요 없습니다. 번들 비트와 마찬가지로 프런트매터 바로 뒤에 작성합니다.
- 나머지 속성은 `::use` 인수처럼 검사되는 템플릿 매개변수입니다(`E-COMPONENT-ARG`). 누락되었거나
  거부된 인수를 읽는 헤더 키는 도출되지 않으므로, 잘못된 인수 하나가 보고 하나를 만듭니다. 헤더 키와
  같은 이름(`title`, `once`, `id` 등)의 매개변수는 속성이 비트 자체의 키를 설정하므로 전달할 수 없으며,
  선언 위치에서 `E-TEMPLATE`입니다. 이름을 `title`에서 `heading`처럼 바꾸세요.
- 본문이 없는 템플릿과 자체 닫는 use는 한 줄 비트를 만듭니다:
  `<beat use="trainer" id="r3Joey" who="joey"/>`.

템플릿 use는 모든 도구에서 일반 비트로 취급됩니다. `lute beats`는 도출된 헤더를 나열하고, play,
trace 및 컴파일된 아티팩트는 이를 펼쳐 쓴 것처럼 표시합니다. 오용은 `E-TEMPLATE`입니다. 가져온
컴포넌트가 아닌 이름을 지정한 `use=`(did-you-mean 포함), `beat:`가 없는 컴포넌트, 잘못된 헤더,
템플릿 최상위가 아닌 곳의 `::body`가 해당합니다.

## 어떤 비트가 승리하는가

엔진이 계기 `O`를, 선택적으로 대상 `T`에 대해 발생시키면:

1. **후보**는 `on: O`이며 `target`이 없거나 `T`와 같은 비트, 그리고 (dsl 0.26.0) 해당 kind에
   `T`의 멤버가 속하는 [kind 비트](#kind-대상)입니다. 같은 계기의 장면, 엔트리, 번들 비트는
   하나의 목록에서 경쟁합니다.
2. 후보의 `after:`(번들 비트의 `after=`)와 `when`이 성립하고 `once`가 소진되지 않았을 때 **적격**입니다.
   장면 또는 번들 비트는 표시 기록으로, 엔트리는 읽기 플래그로 판정합니다(`once="day"` / `"week"` /
   `"slot"` / `"season:<name>"`이면 마지막 표시 시점으로 판정). [`share`](#한-이벤트-여러-장소-share)
   키가 있는 비트는 키의 어떤 비트가 소진되면 소진되고, [`spentBy`](#해결될-때까지-spentby)가
   있는 비트는 그 조건이 성립하면 소진됩니다.
3. 적격 비트는 **우선순위 내림차순, 그다음 프로젝트 순서**로 정렬합니다. 프로젝트 순서는 문서 경로,
   그다음 문서 내 선언 순서입니다. `project.index.json`은 `beats` 아래에 모든 비트를 이 순서로 나열합니다.
   우선순위가 같으면 kind 비트는 다른 후보 뒤에 오므로, `T` 자체를 지정한 비트가 이를 앞섭니다.
4. 표시할 내용을 계기의 `select`가 결정합니다. `select: first`에서는 `also`가 아닌 첫 적격 비트
   ( **승자** )를 표시한 뒤 모든 적격 [`also`](#also로-곁가지-발언) 비트를 표시합니다.
   `select: sequence`에서는 모든 적격 비트를 순서대로 표시합니다. `select: all`에서는 정렬된 전체 목록을
   제시하고 플레이어가 하나를 고르거나 목록을 닫습니다. 닫으면 아무것도 표시하지 않고 아무것도
   소진하지 않습니다. 엔진은 각 비트의 `title`(장면의 `title:`, 엔트리 또는 번들 비트의 `title=`)로
   메뉴에 레이블을 붙이며, `project.index.json`은 모든 비트 행에 이를 담습니다(dsl 0.23.0).
5. 적격 비트가 없으면 계기는 이야기 없이 지나가고 엔진은 기본 동작을 수행합니다.

위의 네 가지 말을 가져와 다섯 번째 런의 플레이어를 생각해 봅시다. 아킬레스를 처음 만났을 때 엔진은
`achillesFirstMeeting`(우선순위 30)을 표시하고 `once="user"`로 영구 소진합니다. 그 런에서 다음 대화는
`achillesMorning`(우선순위 20)을 표시합니다. 그 뒤에는 런의 나머지 동안 `achillesBark3`(우선순위 10)이
표시되고, `user.runs`가 3 미만일 때만 `achillesBark1`이 응답합니다. 이후의 모든 런은
`achillesMorning`으로 시작합니다. 새 런이 `entry.achillesMorning.read`를 재설정하기 때문입니다.
## 한 이벤트, 여러 장소: `share`

같은 이벤트가 서로 다른 장소에서 일어날 수 있습니다. Sol은 하루에 한 번 당신과 따뜻한 시간을 보냅니다.
아침에는 라디오에서, 밤에는 지붕에서, 어느 쪽이 먼저든 하나만 발생합니다. `once="day"`인 두 비트는
각자만 소진하므로 플레이어는 둘 다 보게 됩니다. 하나의 이벤트를 나타내는 비트는 하나의 소진을 공유할
수 있습니다(dsl 0.25.0 §2).

```lute
<beat id="solRadio" on="talk" target="npc.sol" once="day" share="solWarm" when="run.slot == 'morning'">
  @sol: The radio says clear skies. Stay a while.
</beat>

<beat id="solRoof" on="talk" target="npc.sol" once="day" share="solWarm" when="run.slot == 'night'">
  @sol: Up here you can see the whole bay.
</beat>

<beat id="solNod" on="talk" target="npc.sol" once="false" priority="-1">
  @sol: Hey.
</beat>
```

프로젝트 어디에 있든 같은 `share` 키를 가진 비트는 함께 소진됩니다(엔트리는 읽히면)
키의 모든 비트가 `once` 기간 동안 소진되며, 각 비트가 표시된 것과 정확히 같습니다. 장면 비트(프런트매터의
`share:`), 엔트리 비트(`share=`), 번들 비트(`share=`)는 하나의 키를 공유할 수 있습니다. 아침에는 라디오
비트가 재생되고 지붕 비트는 그날 남은 시간 동안 소진됩니다. [`lute play`](/ko/tooling/play/)는 이유를 다음과
같이 표시합니다.

```
── step 3 · talk → npc.sol ──────────────
  ✓ talks.solNod [beat, priority -1]
  ✗ talks.solRadio [beat, priority 0] — once: day — already presented today
  ✗ talks.solRoof [beat, priority 0] — once: day — `share: solWarm` already spent today by talks.solRadio
  → talks.solNod
```

키는 문자, 숫자, `_`, `-`로 이루어진 이름이며 `-`로 시작할 수 없습니다. 키 옆에 소진하는 `once`를
작성해야 합니다. `once`가 없거나 `once: false`인 `share`는 `E-BEAT-ATTR`입니다. 반복 가능한 비트에는
소진할 것이 없기 때문입니다. 하나의 키에 속한 모든 비트는 동일한 `once`를 선언해야 합니다. 키는 한 기간에
소진되기 때문입니다. 로어 문서 `harbor`에 `once="user" share="solWarm"`인 `solDock` 비트를 추가하면
`check-project`가 라디오와 지붕 비트의 불일치를 보고합니다.

<!-- lute-diagnostics -->
```
./lore/talks.lute:7:66: error [E-BEAT-ATTR] beat `talks.solRadio` shares `solWarm` with beat `harbor.solDock`, but declares `once: day` where beat `harbor.solDock` declares `once: user`; the beats of one `share` key are spent together for one period, so every one of them declares the same `once`
```

이 키는 날짜를 상태에 기록하고 모든 비트에 그 값을 가드로 넣던 기존 우회 방법
(`::set{run.solWarmDay = clock.index}`)을 대체합니다. [`lute beats`](/ko/tooling/overviews/#lute-beats)는 `once`
열에 키를 표시하고(`day, share solWarm`), [`lute calendar`](/ko/tooling/overviews/#lute-calendar)는 키를 함께
소진합니다. `W-BEAT-PRIORITY-TIE`와 캐스트 존재 여부는 키 전체의 소진 플래그를 읽습니다. 키는 비트 기록과
`project.index.json` 행에서 `share`로 IR에 들어가며, 작성되지 않은 경우에는 없습니다.

## 해결될 때까지: `spentBy`

플레이어가 실패하고 다시 시도할 수 있는 퍼즐은 표시되었다고 소진되지 않습니다. 해결되었을 때 소진됩니다.
0.27.0 이전에는 반복 가능한 비트와 결과에 대한 가드가 필요했습니다.

```lute
<beat id="valves" on="townVisit" once="false" when="!holds('solved', ['valves'])">
  @narrator: Three valves, one pressure gauge. The needle waits.
</beat>
```

`spentBy`는 이를 직접 표현합니다. `when`과 같은 방식으로 검사되는 CEL 조건이며, 표시 대신 비트를
소진합니다. 즉 조건이 성립할 때까지 표시할 때마다 비트는 계속 적격이고, 그때부터 소진됩니다.

```lute
<beat id="valves" on="townVisit" spentBy="holds('solved', ['valves'])">
  @narrator: Three valves, one pressure gauge. The needle waits.
</beat>
```

장면은 프런트매터에 `spentBy: "holds('solved', ['valves'])"`로 작성하고, 엔트리 및 번들 비트는
속성(`spentBy="…"`)으로 작성합니다. 조건은 `when`에서 읽을 수 있는 모든 것을 읽을 수 있습니다. 이는
래치입니다. 플레이 과정에서 settle이 끝난 세계(모든 표시, 엔진 쓰기, 시계 이동, 새 런 이후)에서 한 번
성립하거나 비트를 판정할 때 성립하면 비트는 소진되고, 이후 조건이 다시 거짓이 되어도 되돌아오지 않습니다.
래치는 settle이 끝난 뒤에만 한 번 확인하므로 `start="true"` 퀘스트는 처음 읽을 때 이미 `active`이며,
잠깐 전의 `unset`으로 읽히지 않습니다. [kind 비트](#kind-대상) 또는
[`for=` 비트](#멤버별-1회-for)에서는 조건이 멤버별로, `occasion.target`을 바인딩한 채 판정되며
해당 멤버를 소진합니다.

`spentBy` 옆의 `once`는 비트가 소진 상태로 유지되는 기간을 설정합니다. 별도로 쓰지 않으면 `run`(다음
런에 다시 소진 가능), `user`(영구), `day` / `slot` / `week`(시계의 기간이 넘어갈 때까지), 또는
`season:<name>`(시즌이 다시 열릴 때까지)입니다. 매주 초기화되는 퍼즐은 `once="week"`라고 씁니다.
`once="false"`는 아무것도 소진 상태로 두지 않으므로 `spentBy` 옆에 쓰면 `E-BEAT-ATTR`입니다. 무언가가
거짓일 때만 응답해야 하는 비트는 `when="!(…)"`라고 작성하세요. [`share`](#한-이벤트-여러-장소-share)
키는 표시로 비트를 소진하므로 `spentBy`와 함께 쓸 수 없습니다. 시작 시 이미 성립하는 `spentBy`(기본값의
모든 상태 경로, 시드 팩트만 있는 경우, `start`가 성립할 때까지 `unset`인 각 퀘스트)는
`W-BEAT-SPENT-AT-START`입니다. 비트가 재생되기 전에 소진되기 때문이며, 메시지는 `when: "!(…)"` 재작성도
이름으로 안내합니다.

래치는 각 발생 때마다 새로 판정하는 조건과 다릅니다. 조건이 성립한 뒤 다시 거짓이 될 수 있을 때 둘의
차이가 나타납니다. `check-project`는 `once`가 작성되지 않은 `spentBy` 비트의 조건이 `::retract` /
`::assert` 또는 디렉티브의 선언된 효과로 되돌릴 수 있는 팩트, 시즌 상태, 팩트나 퀘스트(시즌이 다시 열릴 때
각각 초기화됨), 또는 `rearm`으로 `unset`으로 돌아가는 퀘스트를 읽으면 **`W-SPENT-BY-REVERSIBLE`** 경고를
냅니다. 메시지는 다음 재작성 방법을 안내합니다. 조건이 다시 거짓일 때마다 되돌아와야 하는 비트에는
`once: false`와 `when: "!(…)"`, 한 시즌 동안의 조건에는 `once: season:<name>`, 래치를 의도적으로
유지하려면 `once: run`을 사용합니다. `once`를 작성하면 기간이 명시되므로 경고가 사라집니다.

`lute play`, `lute calendar`, `lute test`의 `eligible:`은 소진된 비트를 조건이 성립하는 동안
``spentBy: `run.solved` holds``라는 이유로, 한 번 성립한 뒤에는
``spentBy: `run.solved` held — spent this run``이라는 이유로 부적격이라고 보고합니다(kind 비트의 이유는
멤버를 포함합니다: ``… held for cod — spent this run``). 조건은 장면 `meta.beat`, `beat` 기록 또는 `entry`
기록에서 `spentBy: {cel, expr, authored?}` 형태로 IR에 들어가고, 비트의 `project.index.json` 행에는 원본
텍스트로 들어갑니다. 해당 행의 비트 `once`는 소진 기간이며(장면 또는 번들 비트에서 작성하지 않으면 `run`,
엔트리에서는 없음)입니다.

## 계기 구성하기

`select: first`는 계기마다 하나의 비트를 표시하며, 이는 대개 올바른 선택입니다. 하나의 주요 비트만
있으면 “Maud와 대화할 때 무슨 일이 일어나는가”를 설명하기 쉽습니다. dsl 0.23.0의 두 가지 추가 기능은
비트 하나만으로는 부족한 순간을 다룹니다.

### 루틴 다음에 오늘의 이벤트

`select: sequence`로 선언한 계기는 선택 순서대로 **적격한 모든 비트**를 하나씩 표시합니다.

```yaml
occasions:
  evening: { select: sequence }
```

`evening`에서 `priority: 5`, `once: false`이고 `::set{ run.day = run.day + 1 }`로 끝나는 저녁 식사
장면과, `evening`에서 `when: 'run.day >= 2'`인 편지 장면을 생각해 봅시다. 첫 저녁에는 저녁 식사 후 편지가 표시됩니다.
한 번 발생한 비트가 재생되면 이후 비트는 자기 차례 직전에 다시 판정되므로, 편지는 저녁 식사로 2가 된
날을 봅니다. 두 번째 저녁에는 편지의 기본 `once: run`이 소진되어 저녁 식사만 표시됩니다. 앞선 비트가
`when`을 거짓으로 만든 비트도 같은 방식으로 건너뜁니다. 시퀀스에는 선택할 것이 없으므로
[`lute play`](/ko/tooling/play/#계기-조합하기)에서 시퀀스 계기에 `pick:`을 쓰는 단계는 사용 오류입니다.

### `also`로 곁가지 발언

`select: first` 계기에서 `also: true`인 장면 또는 번들 비트는 승자를 두고 경쟁하지 않습니다. 적격이면
승자 **뒤에**, 승자와 함께 표시됩니다. 주요 비트가 전혀 적격하지 않아도 표시됩니다.

```lute check
---
kind: scene
id: town.cat
on: townVisit
also: true
when: 'run.day >= 2'
once: false
state:
  run.day: { type: int, default: 1 }
---

## Opening {#opening}

@narrator: The stray cat is back on the fountain's rim, watching you.
```

승자는 `also` 비트의 우선순위와 관계없이 `also`가 아닌 첫 적격 비트입니다. 적격한 `also` 비트가 여러
개라면 선택 순서로 승자 뒤에 따르고, 각각 자신의 `once`를 소진합니다. 위 고양이는 `once: false`이므로
2일째부터 모든 허브 방문에서 어떤 비트가 이기든 그 뒤에 따라옵니다.

`also`는 하나의 비트가 승자가 되는 곳에서만 의미가 있습니다. `select: all` 또는 `select: sequence`
계기에서는 모든 적격 비트가 이미 제시되거나 표시되므로 그곳의 `also: true`는 `E-BEAT-ATTR`입니다.

<!-- lute-diagnostics -->
```
./scenes/supper-aside.lute:6:7: error [E-BEAT-ATTR] `also: true` applies only to a `select: first` occasion; `evening` is `select: sequence`, which already presents or offers every eligible beat — remove `also:`
```

`true`나 `false`가 아닌 `also` 값도 마찬가지입니다. `W-BEAT-SHADOWED`와 `W-BEAT-PRIORITY-TIE`는
`also` 비트를 무시합니다. `also` 비트는 다른 비트를 가리지도, 다른 비트에 가려지지도, 동률이 되지도 않습니다.

`also` 비트는 계기가 발생할 때 한 번 판정되고, 위에서 설명한 것처럼 시퀀스 비트는 자기 차례에 다시
판정됩니다. 각 표시 비트는 자신의 `once`를 소진하고, 각 비트 뒤에 퀘스트가 settle되므로 하나의 계기에서
두 비트 사이에 퀘스트 [기한](/ko/language/quests-and-scenes/#기한)을 판정합니다. `questComplete`를
포함하여 이 발생이 응답하는 `<on event>` 핸들러는 모든 비트가 끝난 뒤 실행됩니다.

프로젝트의 `terminal:`은 계기가 발생할 때 판정되므로 이를 성립시키는 발생도 끝까지 진행됩니다.
게임을 끝내는 쓰기를 수행한 비트가 완료되고, 그 시퀀스의 나머지 및 핸들러가 이어집니다. 쓰기 뒤 또는
퀘스트 `terminal:`이 읽는 `questComplete` 핸들러에 에필로그를 넣을 수 있습니다. “종료”가 의미하는 다른
사항은 [게임 종료](/ko/tooling/play/#게임이-끝났을-때)를 참조하세요.
## 계기

계기는 엔진의 어휘입니다. 플러그인은 `occasions` export로 계기를 선언합니다.

```yaml
occasions:
  townVisit: { select: first }
  talk:      { select: first, target: { prefix: npc, entity: person } }
  examine:   { select: first, target: true }
  inbox:     { select: all }
  evening:   { select: sequence }
```

해결된 플러그인이 계기를 선언할 때까지 계기 이름은 **형태만** 확인됩니다. 어떤 이름이든
허용되므로 엔진 플러그인이 존재하기 전에 비트를 작성할 수 있습니다. 하나라도 플러그인이 계기를
선언하면 선언되지 않은 계기를 지정하는 비트는 `E-OCCASION-UNKNOWN`이며, 대상 없는 계기
(`target: true`도 도메인도 없이 선언된 계기)의 장면 또는 번들 비트에 있는 `target`은
`E-BEAT-ATTR`입니다. 해당 위치에서 엔트리의 `target`은 [메타데이터](#엔트리-비트)입니다(dsl 0.24.0 §6).
선언 형식, `select`, `target`은 [이야기 재생](/ko/tooling/play/#계기)에서 설명합니다.

계기는 퀘스트 목표도 판정할 수 있습니다. `<objective on="runEnd" …>`는 퀘스트가 활성 상태일 때
`runEnd`가 발생한 경우에만 `done`을 평가하며, 계기는 같은 어휘에 대해 검사됩니다. dsl 0.23.0부터
목표도 대상을 지정할 수 있습니다. `<objective on="talk"
target="npc.maud" …>`는 `npc.maud`에 대해
`talk`가 발생할 때만 판정되고, 대상은 비트의 대상과 정확히 같은 방식으로 검사됩니다(`E-BEAT-ATTR`):
점 표기 id이며 `on`과 함께만 사용할 수 있고, 대상을 받는 계기에서만, 계기의 도메인 안에서만 사용할 수 있습니다. `target`이 없으면 대상이 무엇이든 계기가 발생할 때마다 목표를 판정합니다.
[퀘스트와 장면](/ko/language/quests-and-scenes/#퀘스트가-씬-및-계기를-만나는-방식)을 참조하세요.

계기 발생은 비트 뒤에 퀘스트에 응답합니다. 계기의 비트가 표시된 뒤(또는 적격 비트가 없거나,
플레이어가 `select: all` 목록을 선택 없이 닫은 뒤) 발생은 다음 두 가지를 이 순서로 더 수행합니다.

1. 플러그인이 `events:` 아래에 같은 이름의 **월드 이벤트**도 선언했다면 계기 발생이 이를 실행합니다.
   모든 활성 퀘스트의 해당 이름 `<on event>` 핸들러가 한 번씩 실행됩니다. 이벤트에는 대상이 없습니다.
2. 계기는 모든 활성 퀘스트의 `on=` 목표를 판정하므로 목표가 핸들러가 방금 기록한 내용을 읽을 수
   있습니다. 그런 다음 퀘스트가 settle됩니다.

목표 또는 같은 이름의 핸들러만 참조하는 계기도 여전히 발생합니다. 아무것도 표시하지 않고 이 두 단계만
수행합니다. [`lute play`](/ko/tooling/play/), `lute run`, [`lute trace`](/ko/tooling/tracing/)는 같은 순서로 발생에
응답합니다. 따라서 플러그인이 `dayEnd`를 계기와 월드 이벤트 양쪽으로 선언하면, 활성 퀘스트의
`<on event="dayEnd">` 핸들러는 별도 이벤트를 실행하지 않아도 엔진이 `dayEnd`를 발생시킬 때마다 실행됩니다.

### 대상 도메인

`target: true`는 0.21.0의 의미를 그대로 유지합니다. 계기가 무언가를 *대상으로* 발생하며, 비트의
대상은 어떤 점 표기 id든 될 수 있습니다. 따라서 `npc.achilels` 같은 오타는 엔진이 절대 발생시키지 않는
대상을 지정하고, 비트는 조용히 재생되지 않습니다.

**대상 도메인** `{ prefix, entity }`(dsl 0.22.0)은 어떤 것들이 대상인지도 지정합니다. 계기의 모든
대상은 `<prefix>.<member>`이며, 여기서 `<member>`는 `entity`라는 entity kind의 멤버입니다. 이는 팩트가
사용하는 `entities:` 어휘와 같고, 문서 스키마(`uses:` 또는 프로젝트 `defaults:`)에서 읽습니다. 위의
`talk` 선언과 다음 설정을 함께 두면

```yaml
entities:
  person: { members: [maud, oskar] }
```

`target: npc.oskar`는 `talk`에 응답하지만 `target: npc.osker`는 did-you-mean과 함께 `E-BEAT-ATTR`입니다.

<!-- lute-diagnostics -->
```
./scenes/oskar.lute:5:9: error [E-BEAT-ATTR] target `npc.osker` is outside occasion `talk`'s domain `npc.<person>` (`npc.maud`, `npc.oskar`) — did you mean `npc.oskar`?
```

접두사가 잘못된 `place.oskar`도 마찬가지입니다. 엔진이 멤버를 채우는 `open:` kind에서는 어떤
`<prefix>.<id>`든 통과합니다. 문서 스키마가 선언하지 않은 kind를 도메인이 지정하면 해당 계기의 모든
대상에서 `E-BEAT-ATTR`입니다. 장면, 엔트리, 번들 비트의 대상은 동일하게 검사하며 목표의 대상도 검사합니다.
[`lute play`](/ko/tooling/play/)는 도메인 밖의 단계 대상을 같은 did-you-mean과 함께 거부합니다.

도메인은 엔진이 일부 멤버에 대해서만 계기를 발생시킬 때 **`members:`**로 kind를 더 좁힐 수도 있습니다.

```yaml
occasions:
  bossDefeated: { select: first, target: { prefix: boss, entity: foe, members: [gatekeeper, warden] } }
```

`entities:` 아래에 `foe: { members: [gatekeeper, warden, cinderhound] }`가 있으면 계기의 대상은
`boss.gatekeeper`와 `boss.warden`뿐입니다. `cinderhound`가 `foe`이더라도
`target: boss.cinderhound`는 `E-BEAT-ATTR`이며, did-you-mean은 나열된 멤버를 대상으로 동작합니다.

<!-- lute-diagnostics unverified="verbatim lute check output; the message ends in an optional did-you-mean slot that is empty here, and the matcher requires every interpolation to be non-empty" -->
```text
./scenes/hound.lute:5:9: error [E-BEAT-ATTR] target `boss.cinderhound` is outside occasion `bossDefeated`'s member list (`boss.gatekeeper`, `boss.warden`), a subset of entity kind `foe`
```

나열한 모든 멤버는 해당 kind에 속해야 합니다. kind가 선언하지 않은 멤버가 하나라도 있으면 철자가 올바른
대상까지 계기의 모든 대상이 `E-BEAT-ATTR`이 되며, 메시지는 잘못된 멤버를 알려 줍니다.
``occasion `bossFled` lists `wardne` in its target `members:`, but `wardne` is not a member of
entity kind `foe` — did you mean `warden`?``. `open:` kind의 멤버는 체커가 알 수 없으므로 목록을 작성된
그대로 사용합니다. 빈 목록이나 중복으로 나열한 멤버는 플러그인 로드에 실패합니다([매니페스트](/ko/plugins/manifests/) 참조).
이 부분집합은 도메인의 모든 소비자를 똑같이 제한합니다. 비트, 엔트리, 목표 대상, `lute play` 단계의
`target:`, `lute new scene --target`, `lute calendar --target`이 모두 해당합니다.

### Kind 대상

비트는 `target="kind:<kind>"`로 kind의 **모든 멤버**에 응답할 수 있습니다(dsl 0.26.0 §5). 벌레 잡기
대회가 여러 종을 몇 단계로 나누어 점수를 매긴다고 합시다. 종마다 `caught` 비트를 하나씩 만드는 대신,
프로젝트는 `species`의 하위 kind로 단계를 선언하고, 계기의 `{ prefix: mon, entity: species }`
도메인에 있는 entity kind를 사용합니다.

```yaml
entities:
  species:   { members: [inchlet, cocoonix, stingle, hornbeetle, bladebug] }
  bugRare:   { subsetOf: species, members: [hornbeetle, bladebug] }
  bugCommon: { subsetOf: species, members: [inchlet, cocoonix, stingle] }
```

그리고 로어 문서 `contest`에 단계별 비트 하나씩을 작성합니다.

```lute
<beat id="catchRare" on="caught" target="kind:bugRare" once="false" when="run.contest == 'entered'">
  ::set{run.contestScore = 3}
  @narrator: A {{occasion.target}}! The nets all around you go still.
</beat>

<beat id="catchCommon" on="caught" target="kind:bugCommon" once="false" when="run.contest == 'entered'">
  ::set{run.contestScore = 1 when="run.contestScore < 1"}
  @narrator: A {{occasion.target}}. It counts.
</beat>

<beat id="firstInchlet" on="caught" target="mon.inchlet" once="user">
  @narrator: Your very first inchlet. The judges smile.
</beat>
```

- **Kind.** `kind:<kind>`는 계기 도메인 안의 닫힌 kind(`members:`가 있는 kind)를 지정합니다.
  도메인 자체의 kind 또는 [하위 kind](/ko/state/facts-and-datalog/#하위-종류-subsetof) 중 하나여야 합니다.
  계기는 `{ prefix, entity }` 도메인이 있는 플러그인에 의해 선언되어야 합니다. 그 밖의 경우에는
  잘못된 kind에 대한 did-you-mean과 함께 `E-BEAT-ATTR`입니다. 계기가 해당 멤버 중 하나인
  `<prefix>.<member>`에 대해 발생하면 비트가 후보가 됩니다. 장면, 엔트리, 번들 비트 모두 kind 대상을
  동일하게 사용하며, 이는 kind를 읽은 것으로 계산되므로 `W-DOMAIN-UNREAD`가 발생하지 않습니다.
- **`occasion.target`.** 비트의 `when`, 가드, 텍스트에서 `occasion.target`은 계기가 발생한 대상의
  멤버입니다(`mon.inchlet`이면 `inchlet`). kind로 타입이 지정됩니다. 비교하는 값
  (`occasion.target == 'inchlet'`)으로 사용하거나 패턴 매칭할 수 있습니다. `<match subject="occasion.target">`는
  kind의 멤버에 대해 arm을 검사하며 엔진이 항상 값을 할당하므로 `unset` arm이 필요 없습니다. dsl
  0.27.0부터는 팩트 쿼리와 `per:` 패밀리에서 멤버 이름을 대신하는 데도 사용할 수 있습니다.
  [멤버에 관해 묻기](#멤버에-관해-묻기)를 참조하세요. 값은 해당 표시 동안만 유지됩니다.
  kind를 대상으로 하지도 않고 [멤버별 1회](#멤버별-1회-for)로 표시되지도 않는 비트나 엔트리에서
  `occasion.target`을 읽으면 `E-UNDECLARED`입니다.
- **표시.** `{{occasion.target}}`은 `{"kind": "occasionTarget", "entityKind": "bugCommon"}`
  플레이스홀더로 컴파일되므로 엔진이 멤버의 표시 이름을 보여줄 수 있습니다. [`lute play`](/ko/tooling/play/)
  는 멤버의 kind 레이블을, 멤버가 캐스트 id라면 캐스트의 `name:`을, 그렇지 않으면 id를 렌더링합니다
  (`A inchlet. It counts.`).
- **순위.** 우선순위가 같으면 멤버를 지정한 비트가 kind 비트보다 우선하며 둘 사이에는
  `W-BEAT-PRIORITY-TIE`가 발생하지 않습니다. `W-BEAT-SHADOWED`는 kind 비트를 멤버별로 판정합니다.

`lute play`는 첫 번째 발생에서 멤버 비트가 이기는 모습과 그 뒤 kind 비트가 응답하는 모습을 보여 줍니다.

```
── step 1 · caught → mon.inchlet ──────────────
  ✓ contest.firstInchlet [beat, priority 0]
  ✓ contest.catchCommon [beat, priority 0]
  → contest.firstInchlet
@narrator: Your very first inchlet. The judges smile.
── step 2 · caught → mon.inchlet ──────────────
  ✓ contest.catchCommon [beat, priority 0]
  ✗ contest.firstInchlet [beat, priority 0] — once: user — already presented
  → contest.catchCommon
  set run.contestScore = 1
@narrator: A inchlet. It counts.
```

각 형식 오류는 무엇이 잘못되었는지 이름으로 표시합니다.

<!-- lute-diagnostics -->
```
./lore/contest.lute:24:42: error [E-BEAT-ATTR] `target="kind:bugRar"`: `bugRar` is not a declared entity kind — did you mean `kind:bugRare`?
./lore/contest.lute:28:40: error [E-BEAT-ATTR] `target="kind:bugRare"`: `hornbeetle`, `bladebug` are outside occasion `talk`'s domain `npc.<person>`; a kind target names `person` or one of its sub-kinds
./lore/contest.lute:33:23: error [E-UNDECLARED] `occasion.target` is readable only in a beat or entry that targets a kind (`target="kind:<kind>"`) or runs once for each member of one (`for="kind:<kind>"`), where it is the member the beat answers
```

`lute play`와 `lute calendar`는 각 멤버의 발생마다 kind
비트를 제공합니다. `lute beats`는 자체 비트를 지정하는 각 멤버의 사다리와 나머지 멤버의
`kind:<kind>` 사다리에 이를 나열합니다(`caught @ kind:bugCommon`). `--target`은 어떤 멤버든 받습니다.
kind 비트의 [`lute test`](/ko/tooling/cli/)에서는 `state: { occasion.target: inchlet }`로 멤버를 지정합니다.
컴파일된 비트는 장면 `meta.beat`, `entry` 또는 `beat` 기록, `project.index.json` 행에
`targetKind: { kind, prefix, members }`를 담습니다.

#### 멤버에 관해 묻기

`occasion.target`은 멤버가 필요한 곳에서 멤버 이름을 대신합니다(dsl 0.27.0 §3). 팩트 쿼리의 인수
`holds('logged', [occasion.target])`와 `per:` 패밀리의 인덱스 `run.caught[occasion.target]`에서 사용할 수
있습니다. 체커는 kind의 모든 멤버에 대해 조건을 인스턴스화하고, 엔진은 발생시킨 멤버로 치환합니다.

```lute
<beat id="rareAgain" on="caught" target="kind:bugRare" once="false" priority="5" when="holds('logged', [occasion.target]) && run.caught[occasion.target] >= 2">
  @narrator: Another {{occasion.target}} for the book.
</beat>
```

멤버별로 검사하므로 오류는 해당 멤버를 이름으로 표시합니다. 관계의 인수 kind 밖에 있는 멤버는
`E-FACT-DOMAIN`이고, 멤버가 속하지 않은 kind에 `per:`로 선언된 패밀리는 해당 멤버 경로에서
`E-UNDECLARED`입니다.

[`lute test`](/ko/tooling/cli/)는 멤버를 지정하고 그 멤버에 대해 묻는 내용을 모의합니다.

```yaml
file: ../lore/contest.lute
beat: rareAgain
state: { occasion.target: hornbeetle, run.caught.hornbeetle: 2 }
facts: [logged(hornbeetle)]
expect:
  transcriptContains: ["Another hornbeetle for the book."]
```

멤버가 없으면 [`lute trace`](/ko/tooling/tracing/)가 중지되고 어떤 모의 값이 이를 결정하는지 알려 줍니다.
`supply --state occasion.target=<hornbeetle|bladebug> as a mock`을 사용하세요.

각 멤버에 대해 항상 자체 조건이 필요하면 멤버별 비트(`target="mon.hornbeetle"`)가 더 명확하며, 같은
우선순위에서는 kind 비트보다 우선합니다.

#### 멤버에게 쓰기

같은 비트가 자신이 묻는 내용을 쓸 수도 있습니다. 쓰기에서 멤버가 필요한 곳의 `occasion.target`은
비트가 실행되는 멤버를 나타냅니다.

- `::set`의 `per:` 패밀리 인덱스: `::set{run.caught[occasion.target] += 1}`;
- `::assert` 또는 `::retract`의 팩트 인수: `::assert{logged(occasion.target)}`;
- entity kind 또는 도메인으로 타입이 지정된 디렉티브 속성: `::haul{bug=occasion.target}`(디렉티브의 선언된
  효과도 멤버를 이름으로 표시합니다);
- 매개변수가 `speaker`이거나 kind로 타입이 지정된 컴포넌트 인수:
  `::use{component="reaction" who=occasion.target}`. 멤버가 컴포넌트의 `@@who:` 줄을 말하며,
  kind 타입 매개변수는 레이블을 렌더링합니다.

```lute
<beat id="rareAgain" on="caught" target="kind:bugRare" once="false" when="!holds('logged', [occasion.target])">
  @narrator: A {{occasion.target}} for the book.
  ::assert{logged(occasion.target)}
  ::set{run.caught[occasion.target] += 1}
</beat>
```

각 쓰기는 조건과 마찬가지로 멤버별 한 번 검사됩니다. 패밀리가 `per:`로 선언되지 않은 멤버, 또는 관계나
매개변수의 kind 밖에 있는 멤버는 해당 멤버를 이름으로 보고합니다. `lute play`는 비트가 실행된 멤버만
씁니다(`set run.caught.hornbeetle = 1`). kind를 대상으로 하지도 않고 kind의 각 멤버에 대해 실행되지도
않는 비트나 엔트리에서 `occasion.target`을 통한 쓰기는 `E-UNDECLARED`입니다. 쓸 멤버가 없기 때문입니다.

컴파일된 아티팩트에서 쓰기는 `occasion.target`을 유지합니다(`set` 기록의 `path`
`run.caught[occasion.target]`, `assert` 인수, 플러그인 필드). 엔진은 읽기와 정확히 같은 방식으로 바인딩한
멤버를 치환합니다. 대신 컴포넌트 인수는 멤버별로 확장되며, `occasion.target`에 대해 멤버마다 하나의
`match` arm이 생깁니다.

### 멤버별 1회: `for=`

특정 대상 없이 발생하는 계기도 있습니다. 아침이나 일일 재설정이 그 예입니다. 이런 계기의 비트도
kind의 각 멤버에 대해 차례로 실행될 수 있습니다. `for="kind:<kind>"`(dsl 0.27.0 §3)을 사용하면 대상 없이
발생한 `select: sequence` 계기의 비트가 kind의 [멤버 순서](/ko/state/facts-and-datalog/#하위-종류-subsetof)(자체
멤버, 그다음 선언 순서에 따른 하위 kind의 멤버)로 `when`이 성립하는 **각 멤버에 한 번씩** 표시되며,
`occasion.target`에는 해당 멤버가 바인딩됩니다.

```yaml
occasions:
  morning: { select: sequence }
```

```lute
<beat id="birthday" on="morning" for="kind:person" once="false" when="holds('birthdayToday', [occasion.target])">
  @narrator: It is {{occasion.target}}'s birthday.
</beat>
```

`birthdayToday(maud)`와 `birthdayToday(oskar)`가 성립하는 아침에는 이 비트가 `maud`, 그다음 `oskar`에
대해 두 번 표시되며, 계기의 다른 적격 비트 사이에 일반적인 순서로 배치됩니다. [Kind 대상](#kind-대상)에서
`occasion.target`에 대해 설명한 모든 내용이 여기에도 적용됩니다. 팩트 쿼리와 `per:` 패밀리가 이를 받고,
`{{occasion.target}}`은 멤버의 레이블을 렌더링합니다.

- 적격성은 멤버별로 결정됩니다. `once`도 **멤버별로** 소진됩니다. 오늘 아침 `maud`에 대해 표시된
  `once: run` 비트는 내일 `oskar`에 대해 다시 표시될 수 있지만, 이번 런에는 `maud`에 대해 다시 표시되지
  않습니다. `user`, `day`, `slot`, `week`, `season:<name>`도 동일합니다.
- [`spentBy`](#해결될-때까지-spentby) 조건도 `occasion.target`을 바인딩한 채 멤버별로 판정되므로
  한 번에 한 멤버를 소진합니다. 친해질 때까지 각 사람에게 인사하는 비트는 다음과 같습니다.

  ```lute
  <beat id="hello" on="morning" for="kind:person" spentBy="holds('befriended', [occasion.target])">
    @narrator: {{occasion.target}} nods at you, warily.
  </beat>
  ```

  `befriended(maud)`가 성립하면 해당 런 동안 비트는 `maud`에 대해 소진되고 다른 모든 사람에게는 계속
  인사합니다.
- 장면은 프런트매터에 같은 키를 `for: "kind:person"`으로 작성합니다.
- `for`에는 `select: sequence`로 선언되고 대상 없이 발생하는 계기 및 닫힌 kind(`members:`가 있는
  kind)가 필요합니다. `target` 옆에 작성하지 않습니다. 멤버에 대해 발생하는 계기에는 대신
  [`target="kind:<kind>"`](#kind-대상)를 사용합니다. 이 조건을 어기면 각각 did-you-mean과 함께
  `E-BEAT-ATTR`입니다.

[`lute play`](/ko/tooling/play/)는 멤버마다 후보를 하나씩 나열하며(`✓ lore.birthday for maud`), `--json` 후보는
멤버를 `for`로 담습니다. 비트의 `lute test`는 `state: { occasion.target: maud }`로 한 멤버를
지정합니다. 컴파일된 비트는 장면 `meta.beat`, `entry` 또는 `beat` 기록, `project.index.json` 행에
`forKind: { kind, members }`를 담습니다.
### 계기 페이로드

계기는 자신에 응답하는 비트에 타입이 지정된 값을 전달할 수 있습니다(dsl 0.27.0 §3). 가챠 뽑기는
뽑힌 영웅에 대해 발생하며, 그와 함께 온 복제본 수를 알려 줍니다.

```yaml
occasions:
  summon: { select: first, target: { prefix: hero, entity: hero }, payload: { copies: int } }
```

`summon`의 비트는 각 필드를 선언된 타입으로 `occasion.payload.<field>`로 읽으며, 이는 `when`,
가드, `::set` 값, 텍스트에서 모두 사용할 수 있습니다.

```lute
<beat id="pull" on="summon" target="kind:hero" once="false" when="occasion.payload.copies >= 1">
  @narrator: {{occasion.payload.copies}} copies of {{occasion.target}}.
  ::set{run.shards += occasion.payload.copies}
</beat>
```

엔진은 한 번의 발생에 대해 페이로드를 바인딩하며, 해당 발생의 비트가
제시되면 사라집니다. 그 필드를 선언하지 않은 계기의 비트에서 필드를 읽으면 `E-UNDECLARED`이며,
비트 밖에서 읽는 경우도 마찬가지입니다. [`lute play`](/ko/tooling/play/) 단계는 계기 옆에
페이로드(`payload: { copies: 2 }`)를 지정하며, 계기가 선언하지 않은 필드는 사용 오류입니다.
선언된 필드를 빠뜨린 단계도 실행 전에 사용 오류가 됩니다:
`` step 1: occasion `summon` declares payload field `copies`, which is missing from this raise — add `payload: { copies: <int> }` to the step ``
`lute trace`와 `lute test`는 이를 다른 상태처럼 모의합니다(`state: { occasion.payload.copies: 2 }`).
모의 상태에서 빠진 필드를 읽는 줄에 도달하면 플레이스홀더를 출력하는 대신 미완료 상태로
중지합니다. 페이로드가 있는 계기는 시계의 `raise:`가 될 수 없습니다(`E-CLOCK-DECL`). 시계에는
그 계기에 줄 값이 없기 때문입니다.

## 검사기가 증명하는 내용

| 코드 | 시점 |
|---|---|
| `E-BEAT-ATTR` | 잘못된 비트 키 또는 속성입니다. `on`이 이름이 아니거나, `target`이 점으로 구분된 id가 아니거나 계기의 [target domain](#대상-도메인) 밖에 있거나, [`kind:` target](#kind-대상)이 계기 도메인 안에서 닫힌 kind를 지정하지 않거나, [`for`](#멤버별-1회-for)가 닫힌 kind의 `kind:<kind>`가 아니거나 `target`과 함께 있거나 대상에 대해 발생했거나 `select: sequence`가 아닌 계기에 있거나, `priority`가 정수가 아니거나, scene 또는 bundle 비트의 `once`가 `run` / `user` / `false` 이외이거나 엔트리의 `once`가 `run` / `user` / `false` 이외입니다(각각 프로젝트에 [clock](/ko/language/clock/)이 있을 때만 `day` / `slot`도 가능하고, `week`는 해당 `week:`가 있을 때만 가능함; dsl 0.27.0 §5). `once="false"`가 있거나 `spentBy`와 나란히 `share` 키를 쓰거나, `also`가 bool이 아니거나 엔트리에 있거나 `select: all` / `sequence` 계기에 있거나, `share`가 식별자가 아니거나 옆에 소비하는 `once`가 없거나, 하나의 `share` 키를 가진 비트의 `once`가 서로 다르거나(`check-project`), `on`이 없는 비트 키이거나, 대상이 없는 계기에서 scene 또는 bundle 비트의 `target`을 사용하거나, [bundle beat](#비트-번들)의 형태가 잘못된 경우입니다(`id`, 중복 id, 또는 비트는 있지만 `id:`가 없는 lore 문서). |
| `E-OCCASION-UNKNOWN` | `on`이 확인된 플러그인에서 선언한 계기가 아닌 계기를 지정합니다(어떤 플러그인이 계기를 선언한 뒤에만 해당). |
| `E-BEAT-UNREACHABLE` | scene 또는 bundle 비트의 `when`이 결코 성립하지 않음이 증명됩니다. [when의 판정 방식](#when의-판정-방식)을 참조하십시오. `lute check`는 한 파일에서 결정되는 내용을 판정하고, `lute check-project`는 [fact envelope](/ko/state/facts-and-datalog/)를 통해 사실 쿼리도 판정합니다. 엔트리 비트의 성립하지 않는 `when`은 `E-ENTRY-UNREACHABLE`로 유지됩니다. |
| `W-BEAT-SHADOWED` | `check-project` 전용입니다. 동일한 계기와 대상에서 더 앞선 순서의 비트가 항상 실행 가능하고(`after:`가 없으며 `when`이 없거나 항상 참임) 소비되지 않기 때문에(`once`가 없는 엔트리, 또는 `once: false`인 scene 또는 bundle 비트), `select: first` 비트가 절대 승리할 수 없습니다. 대상 도메인이 닫힌 계기에서는 모든 `<prefix>.<member>`에 대해 해당 member의 비트가 이길 때 대상이 지정되지 않은 비트도 보고하며, 메시지는 대상별로 가리는 비트를 이름으로 표시합니다. [kind beat](#kind-대상)도 같은 방식으로 member별 판정을 받습니다. `also` 비트는 가리지도 않고 가려지지도 않습니다. |
| `W-BEAT-PRIORITY-TIE` | `check-project` 전용입니다. 하나의 `select: first` 계기에서 대상이 없거나 같은 대상에 해당하는 두 비트의 `priority`가 같고, 두 `when`이 서로 배타적이라고 증명되지 않습니다. 둘 다 실행 가능하면 프로젝트 순서가 승자를 정하므로 파일 이름을 바꾸거나 파일을 옮기면 결과가 바뀝니다. 하나의 `priority`를 다르게 지정하거나 조건을 배타적으로 만드십시오. 함께 성립할 수 없는 조건의 예로 `run.slot == 'morning'` 대 `run.slot == 'night'`, `run.day > 5` 대 `run.day < 3`, `holds(P)` 대 `!holds(P)`가 있습니다. 비트의 `once`도 조건의 일부로 셉니다. 엔트리의 `once="user"`는 `!entry.<id>.everRead`, `once="run"`은 `!entry.<id>.read`, scene 또는 bundle 비트의 `once: user`는 `!visited('<id>')`로 셉니다. 따라서 `once="user"`인 bark와 그 bark의 `everRead`를 읽는 `when`을 가진 bark는 충돌하지 않습니다. 모든 규칙이 ground이고 `cel()` 전용 스케줄인 파생 atom의 `holds(…)`는 해당 `cel()` 가드로 세므로, 서로 다른 스케줄 슬롯의 비트도 충돌하지 않습니다. dsl 0.26.0 §8에 따라 부정은 먼저 정규화하며(드모르간 법칙, `!(run.money < 100)`은 `run.money >= 100`이므로 `run.money < 100`과 배타적임), 각 조건의 대안을 비교합니다. 두 비트의 `when`에서 사실 분석의 must 집합을 읽고, 각 비트가 아직 실행되지 않은 `once: run` / `user` 프레젠테이션으로만 주장하는 사실은 해당 비트가 실행 가능할 때 없는 것으로 셉니다. 또한 메시지는 두 조건이 배타적이지 않은 이유를 설명합니다. 예를 들어 `once: run`보다 오래 지속되는 플래그, 다른 곳에서 주장되거나 `tier: user`인 사실, 또는 각 `when`이 읽는 경로입니다(``beat `tie.first` reads `run.metMira` and beat `tie.again` reads `run.money`: no path both constrain``). 멤버 비트와 [kind beat](#kind-대상)는 절대 충돌하지 않습니다. 멤버 비트가 먼저 순위를 얻기 때문입니다. 가려진 비트는 대신 `W-BEAT-SHADOWED`를 보고하며, `also` 비트는 절대 충돌하지 않습니다. |
| `W-BEAT-ONCE-RUN-USER` | `check-project` 전용입니다. scene 또는 bundle 비트가 기본값인 `run`을 `once`에 그대로 두었고 `when`이 사용자 계층 상태만 읽습니다. 즉 `user.*`, `entry.<id>.everRead`, `tier: user` 관계의 `holds` / `count`, 또는 사용자 계층 퀘스트의 `quest.<id>.*`이며, run 계층 경로, run 계층 사실, `visited()`는 없습니다. 조건이 성립하면 모든 실행에서 성립하므로 비트가 매 실행마다 다시 재생됩니다. 한 번만 들리는 비트에는 `once: user`를 사용하거나 run 계층 상태로 조건을 제한하십시오. 매 실행마다 재생하는 것이 의도라면 `once: run`을 작성하십시오. 엔트리의 `once="run"`처럼 작성자가 명시한 `once: run`은 의도를 나타내고 경고를 없앱니다. `prev.run.*`은 사용자 상태가 아니라 실행 이력이므로 이를 읽는 `when`에는 경고가 발생하지 않습니다. |
| `W-ENTRY-WRITE-REREAD` | 한 실행에서 다시 읽을 수 있는 엔트리(lookup 엔트리, `once`가 없는 엔트리 비트, 실행보다 짧은 `once`, `spentBy` entry, `once="run"` / `"user"`가 없는 `for=` entry)의 본문에 쓰기를 수행하는 `::set`, `::retract` 또는 effect-only 지시어가 있습니다. 이 쓰기는 한 실행에서 처음 읽을 때만 적용됩니다. `::assert`는 예외이며, `!entry.<id>.read`로 보호된 쓰기도 예외입니다. [두 번 읽기](/ko/language/lore-entries/#두-번-읽기)를 참조하십시오. |

### `when`의 판정 방식

검사기는 게임을 실행하지 않고 조건에 대해 알 수 있는 내용을 증명하며, dsl 0.23.0에서 이 기능이
더 정교해졌습니다. 하나의 `&&` 안에서 각 피연산자가 각 **경로**에 대해 말하는 내용을 수집합니다. 즉
같음, 다름, 숫자 범위입니다. 경로는 상태 경로, component 매개변수, `<match>`의 `$`,
또는 `holds(P)`, `visited(id)`, `count(P)` 같은 관계 호출이며, 텍스트로 비교됩니다. 한 경로의
제약을 모두 만족할 수 없으면 전체 결합식은 거짓입니다. `@def`는 먼저 확장됩니다. 다음 비트들은
일반 `lute check`에서 모두 `E-BEAT-UNREACHABLE`이 됩니다.

| `when` | 성립하지 않는 이유 |
|---|---|
| `run.slot == 'morning' && run.slot == 'night'` | 하나의 경로에 두 값이 있음 |
| `run.day > 5 && run.day < 3` | 빈 범위 |
| `run.flag && !run.flag` | 조건과 그 부정 |
| `holds('met', ['maud']) && !holds('met', ['maud'])` | 같은 쿼리를 양쪽 방식으로 사용함 |
| `run.slot == 'afternon'` | 경로의 열거형 밖에 있는 값 |

 def 뒤에 숨은 조건도 같은 방식으로 찾습니다.

```lute expect="E-BEAT-UNREACHABLE"
---
kind: scene
id: square.festival
on: townVisit
when: "@festivalNight && run.day == 2"
state:
  run.day: { type: int, default: 1 }
defs:
  festivalNight: { type: bool, cel: "run.day == 5" }
---

## Opening {#opening}

@narrator: Lanterns everywhere.
```

`||`는 반대로 작동합니다. 그 사례들이 경로가 취할 수 있는 모든 값을 포괄하면 항상 참입니다.
`run.day < 3 || run.day >= 3`은 모든 날짜에 성립하므로, 해당 `when`을 가진 반복 가능한 비트는
계기에서 뒤따르는 모든 비트를 가립니다(`W-BEAT-SHADOWED`).
`unset`도 값으로 셉니다. 설정되지 않은 경로에서 순서 비교나 단순 불리언을 읽으면 `false`가 아니라
오류이므로, `default`가 없는 경로의 모순은 같은 결합식이 경로가 설정되었음을 증명한 뒤에만 판정됩니다.
`has(run.mood) && run.mood > 5 && run.mood < 3`은 거짓입니다. (`has` 가드가 없으면 해당 경로를
읽는 `when`은 어차피 `E-MAYBE-UNSET`입니다.) 추론은 `&&`와 `||` 안에서만 이루어집니다. Datalog
규칙의 `cel(…)` 가드 내부는 살피지 않으므로, 파생 사실의 스칼라 전제와 모순되는 `when`은 발견되지
않습니다.

같은 판정기가 모든 조건 슬롯에 적용됩니다. 성립하지 않는 `<when test>` 또는 gated line은
`E-ARM-DEAD`, 성립하지 않는 엔트리 `when`은 `E-ENTRY-UNREACHABLE`, 성립하지 않는 objective `done`은
`E-OBJECTIVE-UNSATISFIABLE`이며, `W-BEAT-PRIORITY-TIE`는 두 `when`이 함께 성립할 수 있는지 이
판정기에게 묻습니다. 0.22에서 깨끗하게 검사되던 프로젝트가 예전에는 놓쳤던 모순을 보고할 수 있습니다.
비트가 전에도 재생되지 않은 것은 같지만, 이제 검사기가 그 사실을 말해 줍니다.

### 작업 중

사실로 제한된 비트는 그 사실을 주장하는 scene이 아직 작성되지 않았기 때문에만 성립하지 않는 경우가
많습니다. `lute check-project --wip`(dsl 0.23.0)는 어떤 관계에도 생산자가 전혀 없기 때문에 조건이
성립하지 않을 때 `E-BEAT-UNREACHABLE`, `E-ENTRY-UNREACHABLE`, `E-OBJECTIVE-UNSATISFIABLE`을
**경고**로 보고합니다. 즉 seed도, 어디에도 있는 `::assert`도, 규칙도, `reserved:`도 없는 경우입니다.
경고의 메모에는 이유가 적힙니다. 생산자는 있지만 일치할 수 없는 관계는 계속 오류입니다. 그 뒤에
따라오는 것도 마찬가지입니다. 콘텐츠가 아직 작성되지 않았기 때문에만 성립하지 않는 퀘스트도 연결성에는
포함되므로, 그 퀘스트가 일으키는 `E-CONN-UNREACHABLE`은 오류로 남습니다. [`lute check-project`](/ko/tooling/cli/#check-project)를 참조하십시오.

## 도구

- `lute play <dir> --script <play.yaml>`은 프로젝트 전체에서 계기의 스크립트 시퀀스를 발생시킵니다. 모든 후보를 판정과 함께 출력하고, 승자와 모든 `also` 비트를 제시합니다(또는 `select: sequence` 계기의 실행 가능한 모든 비트를 제시함). 그리고 각 후보 뒤에 퀘스트 수명 주기를 진행합니다. `select: all` 계기의 단계는 `pick:`으로 플레이어의 선택을 지정하거나 `pick: none`으로 목록을 닫으며, 단계의 `expect: { presented: [ids] }`는 제시된 비트가 순서대로인지 검증합니다. Bundle 비트는 scene 비트처럼 재생됩니다. [스토리 재생](/ko/tooling/play/)을 참조하십시오.
  dsl 0.26.0 §8부터 `lute play`와 `lute test`는 엔트리 id를 받는 모든 곳에서 `<document id>.<entry id>`(`talks.miraBark`) 형식의 엔트리도 받습니다. 즉 `pick:`, `entriesRead:`, 단계의 `winner` / `offered` / `notOffered` / `presented`, 테스트의 `entry:` / `entries:`, `expect.eligible` 키 및 `lute trace --entry`에서 사용할 수 있습니다.
- `lute beats <dir>`는 각 계기와 각 대상의 비트 사다리를 선택 순서대로 출력합니다. `priority`, `once`(및 `share` 키), `also`, `after:`, `when`, title, 위의 `check-project` 판정을 포함하며, 아직 깨끗하게 검사되지 않는 프로젝트에서도 출력합니다. dsl 0.26.0 §8부터는 더 이른 비트가 소비되지 않은 채 항상 이기고 fallback의 `when`이 더 이른 비트의 `when`을 함의하는 경우 fallback을 `covered by <id>`(`--json`: `coveredBy`)로 표시합니다. 이는 경고가 아니라 정보입니다.

  ```
    townVisit — select: first
      #  priority  beat               kind    once  verdict               after  when
      1  5         gate.open          bundle  no    -                     -      run.badges >= 1
      2  0         gate.shutFallback  bundle  no    covered by gate.open  -      run.badges >= 3
  ```

  `lute calendar <dir> --axis …`는 모든 날짜와 시간 슬롯처럼 상태 값의 격자에서 실행 가능성을 평가하고, 각 셀에 제시되는 내용을 보여 줍니다. 즉 승자 또는 sequence의 전체 목록입니다. `lute scenario <dir> knowledge`는 각 사실로 제한된 비트가 쿼리하는 사실과 그 사실을 생산하는 대상을 추적합니다. [개요](/ko/tooling/overviews/)를 참조하십시오.
- `lute trace <lore.lute> --beat <id>`는 하나의 bundle 비트를 미리 보고, `lute run <artifact> --beat <id>`는 컴파일된 lore artifact에서 하나를 실행합니다. bundle 비트를 지정하지 않는 id는 `trace`에서 `E-TRACE-BEAT`가 되고 `run`에서는 사용 오류입니다. [추적](/ko/tooling/tracing/#번들-비트)을 참조하십시오.
- `lute compile --all`은 `project.index.json`에 `beats`를 씁니다. scene, 엔트리 또는 bundle 비트(kind `bundle`)마다 한 행이며, 확장된 `when`과 `title`을 포함합니다(dsl 0.23.0). 컴파일된 각 scene 비트에는 `meta.beat`가 포함되고, 각 엔트리 비트에는 `entry` 레코드에 `on` / `priority` / `once`가 포함되며, 각 bundle 비트는 lore artifact에서 본문에 앞서 `beat` 레코드로 기록됩니다.
- `lute new scene <name> --occasion <occasion> [--target <target>]`은 먼저 프로젝트에서 계기와 대상을 확인한 뒤 scene 비트를 작성합니다. 대상이 지정된 계기에는 `--target`이 필요합니다.
- `lute context`는 프로젝트가 선언한 계기를 대상 도메인과 함께 다른 어휘 옆에 나열합니다.

엔진 계약(후보, 실행 가능성, 소비, 제시)은
[`docs/runtime/beats-and-occasions.md`](https://github.com/journeyWorker/lute/blob/main/docs/runtime/beats-and-occasions.md)에 있습니다.
규범 사양은 다음과 같습니다.
[`0.21.0.md`](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.21.0.md),
[`0.22.0.md`](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.22.0.md)는
엔트리 `once`와 대상 도메인을 다룹니다.
[`0.23.0.md`](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.23.0.md)는
`select: sequence`, `also`, beat bundle, 더 정교한 판정기, `--wip`를 다룹니다.
[`0.24.0.md`](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.24.0.md)는
`once: day` / `once: slot`, 메타데이터로서의 엔트리 대상, `after:` 선행 조건으로서의 bundle 비트를 다룹니다.
[`0.25.0.md`](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.25.0.md)는
`share`와 bundle 비트의 `after=`를 다룹니다.
[`0.26.0.md`](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.26.0.md)는
kind 대상, `W-ENTRY-WRITE-REREAD`, 더 정교한 `W-BEAT-PRIORITY-TIE`, `covered by`를 다루며,
초안인
[`0.27.0.md`](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.27.0.md)은
`once: week`, `once: season:<name>`, `spentBy`를 다룹니다.
## 0.34.0의 근거

`lute beats`는 JSON의 각 판정에 `evidence`를 추가합니다. 입증된 검사기 결정은 계속
`proven`으로 표시하고, 보수적인 사다리 안내는 `heuristic`으로 표시하며, 시계 창 결과는
범위가 `declared clock windows; no path search`인 `bounded`로 표시합니다. 제한된 범위의
`W-OBJECTIVE-STRANDED` 및 `W-SLOT-CONTENTION` 메시지는 보편적인 불가능성이 아니라
해당 범위만 설명합니다.

0.41.0부터 `W-OBJECTIVE-STRANDED`는 메시지에 이 근거 수준을 직접 밝히며 항상 경고입니다.
목표를 다시 시도하는 재무장 퀘스트(`tier="run"`, `tier="season:<name>"`, `rearm=`)와
다른 문서의 비트가 완료할 수 있는 목표는 보고하지 않습니다.

## 비트 도구의 JSON

`lute beats --json`은 아티팩트 스트림이 아닌 보고서입니다. 최상위
`command`, `schemaVersion: "0.43.0.beats"`, `ok`를 읽은 뒤 비트 판정과
`evidence`(`proven`, `heuristic`, `bounded`)를 처리하세요. 사람이 읽는 형식으로 적격성을
추론하지 마세요. 명령 id와 보고서 버전은
`LUTE_TELEMETRY=0 target/debug/lute schema beats`로 확인할 수 있습니다.
