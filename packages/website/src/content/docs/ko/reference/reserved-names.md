---
title: 예약된 이름
description: "Lute가 스스로 쓰는 이름, 각 이름을 거부하는 자리, 대신 쓸 이름."
---

어떤 낱말은 Lute에서 이미 뜻이 있다. `run` 같은 state 루트, 값이 없음을 뜻하는
`unset`, CEL 리터럴과 키워드, play 스크립트의 스텝이 그렇다. 선언한 이름이 그
낱말로 읽힐 자리라면 그 이름을 쓸 수 없다. entity 멤버 `clock`은
`holds('found', ['clock'])`에서 `clock` state 루트로 읽히고, 선택지 `true`는
`<when is="true">`에서 불리언으로 읽힌다.

그래서 검사기는 예약된 이름을 쓰는 곳이 아니라 선언한 곳에서 거부한다. 오류는
`E-RESERVED-NAME`이다(plugin이 내보내는 이름은 `E-PLUGIN-RESERVED-NAME`, 템플릿
param은 `E-TEMPLATE`). 메시지는 그 낱말이 이미 무슨 뜻인지 말하고 대신 쓸 이름을
제시한다. 멤버라면 `theClock`, 선택지라면 `yes` 같은 이름이다.
`lute --explain E-RESERVED-NAME`은 같은 표를 터미널에 찍는다.

0.32부터 조건은 제한된 표준 CEL 프로필을 사용하며 숫자 타입은 `int`와
`double`입니다(`number`는 제거됨). 사실 함수는
`holds('관계', ['인자', '_'])` 같은 list 형식으로 씁니다.

## 표

각 줄은 이름, 그 이름을 받지 않는 선언, 그 이름이 이미 뜻하는 것, 알리는 코드다.

| 이름 | 거부하는 선언 | 이미 뜻하는 것 | 코드 |
| --- | --- | --- | --- |
| `unset` | entity 멤버; enum 멤버; scene·beat·entry·choice id | 값이 없음을 뜻하는 말. `is="unset"`과 `== 'unset'`은 아무 값도 없는 경로를 검사하므로 `unset`이라는 값은 결코 맞지 않는다 | E-RESERVED-NAME |
| `true` `false` `null` | entity 멤버; enum 멤버; scene·beat·entry·choice id; relation; 시즌; state 경로 조각(quest·objective·entry·branch·hub id 포함) | CEL 리터럴. 조건과 `is=`에서 이름이 아니라 값으로 읽힌다 | E-RESERVED-NAME |
| `_` | entity member; enum member | 사실 패턴의 와일드카드(`holds('knows', ['_'])`)이자 `per:` 기본값의 나머지 키 | E-RESERVED-NAME |
| `none` | scene·beat·entry·choice id | play와 test에서 고르지 않음, 이긴 것 없음을 뜻하는 말(`pick: none`, `winner: none`) | E-RESERVED-NAME |
| `scene` `run` `user` `app` `quest` `entry` `prev` `clock` `occasion` `season` | entity 멤버; def | state 루트. 조건에서 루트 이름만 쓰면 state 경로가 시작되므로 `holds('found', ['clock'])`와 `@clock`은 state를 읽는다 | E-RESERVED-NAME |
| `scene` `run` `user` `app` `quest` `entry` `prev` `clock` `occasion` `season` `day` `week` `slot` | 시즌 | state 루트이거나 `once`/tier 기간. `once="season:run"`이 뜻이 다른 `once="run"` 옆에 놓인다 | E-RESERVED-NAME |
| `as` `break` `const` `continue` `else` `for` `function` `if` `import` `in` `let` `loop` `namespace` `package` `return` `var` `void` `while` | entity 멤버; relation; 시즌; state 경로 조각(quest·objective·entry·branch·hub id 포함) | CEL 키워드. 조건에서 이름으로 쓸 수 없다(`quest.in.state`는 파싱되지 않는다) | E-RESERVED-NAME |
| `all` `count` `countDistinct` `exists` `exists_one` `filter` `has` `holds` `map` `now` `validAt` `visited` | relation | Lute-CEL 호출이나 CEL 매크로. `holds('관계', ['인자', …])`가 호출로 파싱된다 | E-RESERVED-NAME |
| `completed` `active` | relation | `after:` 호출(`completed("<quest>")`, `active("<quest>")`). `after="completed(dorm)"`이 퀘스트 호출로 읽힌다 | E-RESERVED-NAME |
| `cel` `not` | relation | 규칙 단어. `rules:`에서 `not(…)`은 부정이고 `cel("…")`은 조건이다 | E-RESERVED-NAME |
| `narrator` | cast id | 내장 내레이션 화자. `@narrator:` 줄은 내레이션이므로 이 cast 항목은 보이지 않고, 그 `present:`가 모든 내레이션 줄에 걸린다 | E-RESERVED-NAME, E-PLUGIN-RESERVED-NAME |
| `questActive` `questComplete` `questFailed` | plugin occasion; plugin event | 엔진 수명 주기 이벤트(`<on event="questComplete">`) | E-PLUGIN-RESERVED-NAME |
| `occasion` `newRun` `engine` `event` `advance` `end` | plugin occasion | play 스크립트 스텝 키. `- newRun: true`는 새 run을 시작하고 `- end: true`는 play를 끝낼 뿐, 그 이름의 occasion을 올리지 않는다 | E-PLUGIN-RESERVED-NAME |
| `set` `assert` `retract` `accept` `use` `body` `cut` | plugin directive | core 문장(`::set{…}`, `::use{component=…}`). 콘텐츠는 늘 core 쪽으로 읽는다 | E-PLUGIN-RESERVED-NAME |
| `scene` `on` `quest` `objective` `match` `branch` `hub` `choice` `when` `otherwise` `entry` `beat` `timeline` `track` `reward` `return` | plugin directive | core 블록 태그(`<match>`, `<quest>`). 콘텐츠는 늘 core 쪽으로 읽는다 | E-PLUGIN-RESERVED-NAME |
| `id` `use` `on` `target` `for` `title` `priority` `once` `share` `after` `when` `spentBy` `also` | beat 템플릿 param | beat 머리 키. `<beat use=… when=…>`는 param이 아니라 beat 자신의 `when`을 정한다 | E-TEMPLATE |
| `component` `when` | component param | `::use` 자신의 키(`::use{component=… when=…}`). 그 param은 넘길 수 없다 | E-TEMPLATE |

## 참고

- 멤버 목록에는 이름을 쓴다. `members:`, `add:`, enum의 YAML 숫자(`[1, 2, 3]`),
  불리언(`true`), `null`도 거부한다. 따옴표로 감싸지 말고 이름을 바꾼다(`floor1`).
- `none`은 play나 test가 `pick:`, `winner:`로 부를 수 있는 id(scene, beat, entry,
  choice)에서만 거부한다. enum이나 entity 멤버는 `none`이어도 된다
  (`weapon: [none, sword]`).
- 시즌 이름으로는 `once`/tier 기간 이름(`day`, `week`, `slot`)도 쓸 수 없다.
  `once="season:week"`이 `once="week"` 옆에 놓이기 때문이다.
- kind beat는 kind를 접두어와 함께 쓴다(`target="kind:guest"`). kind 자체는 접두어
  없이 선언한다(`entities: { guest: … }`).
