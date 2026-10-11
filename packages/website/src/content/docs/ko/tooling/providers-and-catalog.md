---
title: 프로바이더와 카탈로그
description: 스냅숏 우선 프로바이더 해석 — 체커가 라이브 카탈로그에 의존하지 않는 이유, --providers 플래그, 카탈로그 새로 고침에 따른 재스탬프, capabilitySnapshot입니다.
---

플러그인은 **프로바이더(provider)** 를 추가합니다. 프로바이더는 디렉티브의 attrs가 해석되는 구체적인 id를 제공하는 id 레지스트리입니다(배경 위치, 음악 트랙, 캐릭터 id, 에셋 id). 컴파일러와 체커의 정확성은 **라이브 또는 원격 카탈로그에 절대로 의존해서는 안 되므로**, 프로바이더는 디스크에 있는 고정된 **스냅숏 아티팩트**를 기준으로 해석합니다. 파서는 프로바이더를 호출하지 않으며, 체커만 호출합니다.

## 스냅숏 우선 해석

해석이 고정된 스냅숏을 기준으로 이루어지므로, 필요한 데이터가 없을 때 컴파일러는 네트워크에서 대기하지 않고 깔끔하게 실패하며, LSP는 오프라인에서 오래된 스냅숏을 유지하고 *catalog-stale* 진단을 냅니다 — 절대로 잘못된 *unknown-id* 오류를 내지 않습니다. 따라서 빌드는 재현 가능합니다. 동일하게 고정된 입력은 언제나 동일한 id를 해석합니다.

## `--providers`

`check`, `check-project`, `compile`, `context`, `trace`, `scenario`는 모두 문서를 해석할 id가 들어 있는 고정 스냅숏 파일 디렉터리인 `--providers <DIR>`를 받습니다.

```console
$ lute check scene.lute --providers ./catalog
```

우선순위는 다음과 같습니다. 명시한 `--providers <DIR>`가 우선합니다. 그렇지 않으면 `--project` 아래에서 프로젝트 자체의 고정 카탈로그가 LSP가 사용하는 것과 동일한 공유 헬퍼를 통해 자동으로 발견되므로 두 표면은 같은 프로젝트에서 같은 id를 해석합니다. 둘 다 없으면 프로바이더 집합은 비어 있습니다.

## `catalog refresh`와 `capabilitySnapshot`

활성화 과정은 프로젝트에 설치된 플러그인과 선택한 프로필을 하나의 불변 capability 스냅숏으로 해석하고, `capabilitySnapshot` 해시를 찍습니다. 프로바이더 스냅숏은 이 해시에 고정됩니다. 해석된 `capabilitySnapshot`이 바뀌면 고정된 스냅숏에 다시 스탬프를 찍어야 합니다.

```console
$ lute catalog refresh ./catalog --project ./my-game
```

`catalog refresh <dir>`는 `<dir>`의 모든 고정 스냅숏에 현재 `capabilitySnapshot`을 다시 스탬프하고 `stale` 플래그를 지우며, 로더가 읽는 디스크상의 평면 형식으로 각 파일을 다시 씁니다. `--project` 아래에서는 해석된 다중 플러그인 `capabilitySnapshot`을 스탬프하고, 이를 생략하면 core 기준선을 사용합니다. 정확성은 라이브 카탈로그에 의존하지 않습니다 — refresh는 이미 고정된 아티팩트를 정규화하고 다시 스탬프할 뿐이므로, `refresh`한 뒤 `load`해도 정확히 왕복됩니다. 명시적인 `catalog refresh`는 빌드보다 먼저 실행합니다. 종료 코드는 성공 시 **0**, I/O 오류 시 **2**입니다.
