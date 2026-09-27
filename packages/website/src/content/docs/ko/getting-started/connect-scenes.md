---
title: 장면을 이야기로 잇기
description: 게임 엔진 없이 장면들을 하나의 이야기로 이어 플레이하고 테스트합니다 — 모든 장면이 응답하는 지어낸 계기 하나, 순서는 after, 진행 순서는 priority, 갈라지는 엔딩은 when, 그리고 lute play용 플레이 스크립트.
---

[첫 장면 작성하기](/ko/getting-started/first-scene/)는 두 장면과 그 사이의 `after:` 한 줄로
끝났습니다. `after:`는 어느 장면이 어느 장면 *뒤에* 오는지 말합니다. 플레이어를 옮기지는
않습니다: 각 장면을 **시작**하는 무언가가 여전히 필요합니다. 출시된 게임에서는 엔진이 그 일을
합니다. 엔진이 생기기 전에는 **`lute play`**가 합니다: 도구가 이야기 전체를 첫 장면부터 엔딩까지
플레이하며 모든 줄을 출력하고, 각 장면이 왜 재생되었는지 또는 왜 재생되지 않았는지 알려줍니다.

이 페이지는 장면 다섯 개짜리 추리물을 만들고 플레이합니다. 전부 코어 Lute입니다: 플러그인도,
엔진도, 프로그래밍도 없습니다.

## 한 문단으로 보는 아이디어

`lute play`는 **계기(occasion)**를 발생시켜 이야기를 앞으로 움직입니다: "다음 장(chapter)" 같은
이름 붙은 순간입니다. 각 장면은 프런트매터 키 하나 `on:`으로 어떤 계기에 응답하는지 말합니다.
계기가 발생하면 그 계기에 응답하는 모든 장면이 후보가 됩니다. 후보는 `after:`와 `when:`이 성립하고
이번 런에서 아직 재생되지 않았을 때 **자격이 있습니다(eligible)**. 자격 있는 장면 중 `priority:`가
가장 높은 장면이 재생됩니다. 계기를 다시 발생시키면 다음 장면이 재생됩니다.

계기는 어디에도 선언하지 않습니다. 계기를 선언하는 플러그인이 없는 동안에는 어떤 이름이든
받아들여지므로, 지어내면 됩니다. 이 페이지에서는 `chapter`라고 부릅니다.

## 프로젝트

```
a-story/
  lute.project.yaml
  world.schema.yaml
  scenes/
    prologue.lute
    counter.lute
    accusation.lute
    ending/
      caught.lute
      wrong.lute
  plays/
    caught.play.yaml
  tests/
    accusation.test.yaml
```

폴더를 만들고 아래의 모든 명령을 그 안에서 실행하세요. 파일은 통째로 보여 주므로 이 페이지만
보고 프로젝트를 입력할 수 있습니다. 같은 파일이 참고용으로 저장소의
[`docs/examples/connect-scenes/`](https://github.com/journeyWorker/lute/tree/main/docs/examples/connect-scenes)에
있습니다.

`lute.project.yaml`은 폴더를 프로젝트로 표시합니다. `defaults:` 블록은 모든 문서에 같은 `uses:`
줄을 주므로 어느 장면도 그 줄을 반복할 필요가 없습니다([Project defaults](/language/imports/#project-defaults)).
`chapters:` 블록은 장들을 순서대로 잇습니다:

```yaml
defaultProfile: core
profiles:
  core:
    plugins: {}
defaults:
  uses: [world.schema.yaml]
chapters:
  - on: chapter
    scenes: [prologue, counter, accusation]
```

`world.schema.yaml`은 이야기가 기억하는 상태 하나를 선언합니다: 플레이어가 누구를 지목했는지.

```yaml
state:
  run.accused: { type: { enum: [nobody, ruben, tilly] }, default: nobody }
```

## 장(chapter)들: `chapters:`

`chapters:`는 계기마다 하나씩 있는 사슬(chain)의 목록이므로, 한 프로젝트가 여러 계기를 이을 수 있습니다.
사슬은 장면들이 응답할 계기를 `on:`으로 이름 붙이고, 장면을 `id:`로 재생 순서대로 적습니다. 장면
자신은 자기가 누구인지만 말합니다:

```lute check="docs/examples/connect-scenes/scenes/prologue.lute"
---
kind: scene
id: prologue
title: Closing Time
---

## Closing Time

@narrator: Five minutes to close, and the bakery smells of burnt sugar.
@wren: Mr. Pryce? We're closing.
```

```lute check="docs/examples/connect-scenes/scenes/counter.lute"
---
kind: scene
id: counter
title: The Counter
---

## The Counter

@narrator: Mr. Pryce is face down on the counter. The till is open.
@wren: Somebody here knows what happened.
```

목록에 오른 장면마다 사슬이 직접 쓸 프런트매터 키 세 개를 대신 써 줍니다. `counter`는
프런트매터에 다음이 적힌 것과 똑같이 동작합니다:

```yaml
on: chapter
after: 'visited("prologue")'
priority: 20
```

- `on: chapter`는 `chapter`가 발생할 때마다 이 장면을 후보로 만듭니다.
- `after: 'visited("prologue")'`는 목록에서 바로 앞 장면이 재생될 때까지 이 장면을 기다리게 합니다.
  `visited("…")`는 장면을 `id:`로 가리킵니다. 첫 장면에는 `after:`가 없으므로 처음부터 자격이 있습니다.
  `visited()`는 런마다가 아니라 세이브 전체에 걸칩니다: 새 런이 시작되어도 장면은 방문한 채로 남으므로,
  두 번째 런부터는 사슬의 모든 `after:`가 이미 성립하고 `priority:`만이 장의 순서를 지킵니다.
- 장면은 런마다 최대 한 번 재생되므로, 프롤로그가 재생된 뒤에는 빠지고 카운터가 남습니다. 사슬은
  `once:`를 쓰지 않습니다. 런마다 한 장씩, 여러 런에 걸쳐 이어지는 이야기라면 목록의 장면마다
  `once: user`를 주어, 앞선 런에서 재생된 장이 계속 소진된 채로 남게 하세요.
- `priority:`는 진행 순서를 대놓고 적습니다: 높은 쪽이 먼저 재생됩니다. 사슬은 첫 장면부터 10씩
  내려가므로(장면 셋이면 30, 20, 10) 모든 장이 사슬 뒤에 오는 장면들보다 앞섭니다.

장면이 직접 쓴 키는 사슬보다 우선합니다: 한 장면에 자기 `priority:`나 다른 `after:`를 주면 그
키만 바뀝니다. 어느 장면도 선언하지 않은 id를 목록에 적거나, 목록의 장면이 자기 `on:`으로 다른 계기에
응답하거나, 한 장면을 두 번 적거나, 한 계기에 사슬을 둘 두거나, 어떤 플러그인도 선언하지 않은 `on:`을
적으면 `lute check-project`가 `E-CHAPTERS`를 보고합니다(비슷한 이름 제안 포함). 아래 엔딩처럼 목록에
없는 장면은 여전히 키를 직접 씁니다.

자기 `when:`이 이야기가 끝내 설정하지 않을 수도 있는 상태를 읽어 재생되지 않을 수도 있는 장은 사슬을
멈춥니다: 목록의 다음 장면이 `after:`로 그 장을 기다리기 때문이며, `lute check-project`가
경고합니다(`W-CHAPTER-STALL`). 시계만 읽는 `when:`은 그 계기의 이후 raise가 여전히 맞추는 동안 사슬을 늦출 뿐이므로 경고하지 않습니다. 창이 닫히는 `when:`은 경고합니다: 실행이 시작된 날의 `dayStart` 장(그날은 시계가 `dayStart`를 raise하지 않습니다. 시계가 `raiseAtStart: true`를 선언했다면 예외입니다)이나, 사슬이 너무 늦게 닿을 수 있는, 끝나는 시계의 마지막 날의 슬롯이 그렇습니다. 선택적인
장 뒤의 장면에 그 앞 장을 가리키는 자기 `after:`를 주거나, 첫 장이라면 사슬에서 빼고 자기 `on:`을
직접 쓰세요. `lute play`는 파생된 `after:`를 이유에 밝힙니다: `after: visited("pryceWakes") is not
satisfied (written by `chapters:` in lute.project.yaml)`.

멈추는 것은 사슬뿐입니다. `chapters:` 밖의 장면이 재생되지 않을 수도 있는 장면을 자기 `after:`로
가리켜도 `W-CHAPTER-STALL`은 나오지 않습니다: 직접 쓴 `after:`는 그 자체가 "그 장면을 기다린다"는
진술이기 때문입니다. 그러니 사슬이 기다리지 말아야 할 관문 있는 장면은 사슬에서 빼고 자기 `on:`과
`after:`를 주세요.

한 번의 발생에서 자격 있는 비트를 모두 보여 주는 `select: sequence` 계기에서는 사슬이 `on:`과
`priority:`만 씁니다: 목록의 장면들이 그 한 번의 발생 안에서 목록 순서대로 이어서 재생됩니다. 그 밖의
계기에서는 발생 한 번에 목록의 장면 하나가 재생됩니다. 대상*에게* 발생하는 계기라면 목록의 모든 장면이
자기 `target:`을 선언해야 합니다. 선언하지 않은 장면은 모든 대상에게 재생되므로 `E-CHAPTERS`입니다.

세 번째 장면은 플레이어에게 선택을 주고 그것을 기억합니다:

```lute check="docs/examples/connect-scenes/scenes/accusation.lute"
---
kind: scene
id: accusation
title: The Accusation
---

## The Accusation

@wren: One of you did this.

<branch id="accuse">
  <choice id="ruben" label="Ruben, the baker">
    ::set{ run.accused = "ruben" }
  </choice>
  <choice id="tilly" label="Tilly, the waitress">
    ::set{ run.accused = "tilly" }
  </choice>
</branch>
```

## 갈라지는 엔딩: `when:`

두 엔딩이 지목 장면 뒤에 같은 `chapter`에 응답합니다. `when:`이 지목 장면이 쓴 상태를 읽어 어느
쪽이 자격을 갖는지 정합니다:

```lute check="docs/examples/connect-scenes/scenes/ending/caught.lute"
---
kind: scene
id: ending.caught
title: Caught
on: chapter
after: 'visited("accusation")'
when: "run.accused == 'ruben'"
---

## Caught

@ruben: The sugar tin. Of course you noticed the sugar tin.
```

```lute check="docs/examples/connect-scenes/scenes/ending/wrong.lute"
---
kind: scene
id: ending.wrong
title: The Wrong Name
on: chapter
after: 'visited("accusation")'
when: "run.accused != 'ruben'"
---

## The Wrong Name

@tilly: Me? I was in the back the whole time.
@narrator: Behind her, the baker quietly unties his apron.
```

두 엔딩은 우선순위 `0`을 함께 쓰지만 경고가 나지 않습니다: 두 `when:`은 결코 동시에 참일 수 없고,
체커가 그것을 알아봅니다. 경로는 `after:`에, 조건은 `when:`에 두세요. 따옴표는 겹칩니다: 바깥
큰따옴표는 YAML의 것이고 안쪽 작은따옴표는 조건의 것입니다. 처음 보는 형태라면
[작가를 위한 따옴표와 YAML](/ko/getting-started/first-scene/#작가를-위한-따옴표와-yaml)을 읽으세요.

## 검사하고 순서 보기

```
$ lute check-project .
ok: ./scenes/accusation.lute (0 warning(s))
ok: ./scenes/counter.lute (0 warning(s))
ok: ./scenes/ending/caught.lute (0 warning(s))
ok: ./scenes/ending/wrong.lute (0 warning(s))
ok: ./scenes/prologue.lute (0 warning(s))
ok: . (5 file(s), 0 project-wide warning(s))
```

`lute beats`는 각 계기에 응답하는 모든 장면을 서로 겨루는 순서대로 출력합니다:

```
$ lute beats .
project root: .

  chapter — select: first
    #  priority  beat                           kind   once  verdict  after                  when
    1  30        prologue "Closing Time"        scene  run   -        -                      -
    2  20        counter "The Counter"          scene  run   -        visited("prologue")    -
    3  10        accusation "The Accusation"    scene  run   -        visited("counter")     -
    4  0         ending.caught "Caught"         scene  run   -        visited("accusation")  run.accused == 'ruben'
    5  0         ending.wrong "The Wrong Name"  scene  run   -        visited("accusation")  run.accused != 'ruben'

```

## 플레이하기

**플레이 스크립트**는 발생시킬 계기를 순서대로, 그리고 각 메뉴에서 고를 선택을 적습니다.
`plays/caught.play.yaml`로 저장하세요:

```yaml
choose: { accuse: ruben }
steps:
  - occasion: chapter
    expect: { winner: prologue }
  - occasion: chapter
    expect: { winner: counter }
  - occasion: chapter
    expect: { winner: accusation }
  - occasion: chapter
    expect: { winner: ending.caught }
expect:
  state: { run.accused: ruben }
```

- `choose:`는 분기 id를 고를 선택지 id에 대응시킵니다: `<branch id="accuse">`에서 `ruben`을 고릅니다.
- 각 스텝은 `chapter`를 한 번 발생시킵니다. 그 스텝의 `expect: { winner: … }`는 어느 장면이 재생되어야
  하는지 말합니다.
- 마지막 `expect:`는 이야기 전체가 끝난 뒤의 상태를 확인합니다.

```
$ lute play . --script plays/caught.play.yaml
── step 1 · chapter ──────────────
  ✓ prologue [scene, priority 30]
  ✗ counter [scene, priority 20] — after: visited("prologue") is not satisfied (written by `chapters:` in lute.project.yaml)
  ✗ accusation [scene, priority 10] — after: visited("counter") is not satisfied (written by `chapters:` in lute.project.yaml)
  ✗ ending.caught [scene, priority 0] — after: visited("accusation") is not satisfied
  ✗ ending.wrong [scene, priority 0] — after: visited("accusation") is not satisfied
  → prologue
@narrator: Five minutes to close, and the bakery smells of burnt sugar.
@wren: Mr. Pryce? We're closing.
── step 2 · chapter ──────────────
  ✓ counter [scene, priority 20]
  ✗ prologue [scene, priority 30] — once: run — already presented this run
  ✗ accusation [scene, priority 10] — after: visited("counter") is not satisfied (written by `chapters:` in lute.project.yaml)
  ✗ ending.caught [scene, priority 0] — after: visited("accusation") is not satisfied
  ✗ ending.wrong [scene, priority 0] — after: visited("accusation") is not satisfied
  → counter
@narrator: Mr. Pryce is face down on the counter. The till is open.
@wren: Somebody here knows what happened.
── step 3 · chapter ──────────────
  ✓ accusation [scene, priority 10]
  ✗ prologue [scene, priority 30] — once: run — already presented this run
  ✗ counter [scene, priority 20] — once: run — already presented this run
  ✗ ending.caught [scene, priority 0] — after: visited("accusation") is not satisfied
  ✗ ending.wrong [scene, priority 0] — after: visited("accusation") is not satisfied
  → accusation
@wren: One of you did this.
▷ choice accuse: [ruben] tilly        ← chosen: ruben
  set run.accused = "ruben"
── step 4 · chapter ──────────────
  ✓ ending.caught [scene, priority 0]
  ✗ prologue [scene, priority 30] — once: run — already presented this run
  ✗ counter [scene, priority 20] — once: run — already presented this run
  ✗ accusation [scene, priority 10] — once: run — already presented this run
  ✗ ending.wrong [scene, priority 0] — when: false
  → ending.caught
@ruben: The sugar tin. Of course you noticed the sugar tin.
── end: complete (4 steps) ──────────────
── expect: every expectation held ──────────────
```

각 스텝은 후보를 나열합니다: `✓`는 자격 있음, `✗`는 자격 없음과 그 이유입니다. `→`는 재생된 장면입니다.
장면이 기대한 자리에서 재생되지 않으면 이것을 읽으세요: 이유가 그 줄에 있습니다. `choose:`를
`{ accuse: tilly }`로 바꾸면 스텝 4에서 대신 `ending.wrong`이 재생됩니다.

## 테스트하기

`lute test`는 `expect:`가 있는 모든 플레이 스크립트를 실행하므로, 위의 플레이는 이미 테스트입니다.
**시나리오 테스트**는 장면 하나를 따로 확인합니다. `tests/accusation.test.yaml`로 저장하세요:

```yaml
file: ../scenes/accusation.lute
visited: [counter]
choose: { accuse: tilly }
expect:
  transcriptContains: ["@wren: One of you did this."]
  state: { run.accused: tilly }
```

`file:`은 테스트 파일 기준 상대 경로이므로 `tests/`에 있는 테스트는 장면을 `../scenes/…`로 가리킵니다.
`visited:`는 카운터 장면이 이미 재생된 것으로 칩니다. 이것을 빼면, 지목 장면이
`after: 'visited("counter")'`를 기다리기 때문에 테스트가 그 줄이 필요하다고 알려줍니다:

```
$ lute test . --project .
FAIL  ./tests/accusation.test.yaml  (./scenes/accusation.lute)
      eligible accusation: not eligible under these mocks (its `after: visited("counter")` (written by `chapters:` in lute.project.yaml) is false — add `visited: [counter]` to the mocks) — the engine would never present it, so the walk proves nothing about play; fix the mocks, or assert `expect: { eligible: { accusation: false } }` (the body is then not walked)
PASS  ./plays/caught.play.yaml  (play of .)

1 passed, 1 failed
```

`visited: [counter]`를 넣으면:

```
$ lute test . --project .
PASS  ./tests/accusation.test.yaml  (./scenes/accusation.lute)
PASS  ./plays/caught.play.yaml  (play of .)

2 passed, 0 failed
```

테스트 파일 형식은 [CLI 참조](/tooling/cli/#test)에, 플레이 스크립트의 모든 키는
[스토리 플레이](/ko/tooling/play/)에 있습니다.

## 장면 추가하기

`lute new scene <name> --occasion chapter`는 이름을 입력한 그대로 `id:`로 삼고, 파일도 그 id를 따라
이름 붙인 새 장면을 씁니다(`lute new scene pryceWakes --occasion chapter`는 `id: pryceWakes`인
`scenes/pryceWakes.lute`를 씁니다). `chapter`가 `chapters:`에 있는 사슬의 계기이므로
`on:`·`after:`·`priority:`는 쓰지 않고, id를 그 사슬에 더하라고 알려 줍니다: 목록의 알맞은 자리에
넣으면 사슬이 세 키를 모두 줍니다. 그다음 플레이 스크립트에 스텝을 하나 더하세요. (어떤 사슬도
응답하지 않는 계기라면 `on:`과, 이미 그 계기에 있는 모든 비트보다 낮은 `priority:`를 씁니다.)

다음 장이 아니라 플레이어가 고르는 대화는 누군가를 *대상으로* 발생하는 두 번째 계기입니다. 장면은
`target:`으로 그 사람을 가리킵니다:

```yaml
on: talk
target: npc.tilly
after: 'visited("counter")'
when: "!visited('accusation')"
```

플레이 스크립트의 스텝도 그 사람을 가리킵니다: `- occasion: talk`와 `target: npc.tilly`.
`when:`은 지목 장면이 재생되고 나면 대화를 닫습니다.

## 엔진이 생기면

이 페이지의 어느 것도 임시 대역이 아닙니다. 게임이 만들어지면 엔진이 플레이 스크립트가 하던
자리에서 `chapter`를 발생시키고, 이기는 장면을 `lute play`가 보여 준 그대로 제시합니다. 엔진의
플러그인이 계기를 선언하면 체커는 철자가 틀린 `on:`도 잡아냅니다.
[계기](/ko/tooling/play/#계기)와 [비트](/language/beats/)를 보세요.
