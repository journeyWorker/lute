---
title: 설치
description: bunx, 전역 bun 설치, 또는 Rust 소스로 Lute CLI를 설치하고, 툴체인을 확인한 뒤 첫 장면으로 넘어가세요.
---

Lute는 단일 명령줄 도구 `lute` 하나로 제공됩니다. 이 도구는 `.lute` 시나리오 파일을 읽어
검사(check), 컴파일(compile), 트레이스(trace)하고 그 내용을 살펴봅니다. 현재 언어 버전은
**0.28.0**입니다.

## `bunx`로 빠르게 시작하기

아무것도 영구적으로 설치하지 않고 Lute를 실행하는 가장 빠른 방법은 `bunx`입니다. 이 명령은
게시된 npm 패키지를 가져와 번들된 네이티브 바이너리를 실행합니다:

```sh
bunx @lute-lang/lute check scene.lute
```

npm 패키지 이름은 `@lute-lang/lute`이고, 이 패키지가 설치하는 명령은 `lute`입니다. `bunx @lute-lang/lute <args>`와
전역 설치된 `lute <args>`는 동일한 프로그램입니다.

## 전역 설치

일상적인 사용을 위해 `lute`를 `PATH`에 유지하려면, 패키지를 bun으로 전역 설치하세요:

```sh
bun add -g @lute-lang/lute
lute check scene.lute
```

`@lute-lang/lute`는 얇은 런처입니다: 플랫폼을 감지하여, 플랫폼별 선택적 의존성
(`@lute-lang/lute-core-darwin-arm64` 또는 `@lute-lang/lute-core-linux-x64`)으로 배포되는 사전 빌드된 네이티브
바이너리로 디스패치합니다. 올바른 바이너리는 설치 시점에 자동으로 선택됩니다.

## 플랫폼 지원

| 플랫폼 | npm 코어 패키지 | 상태 |
|---|---|---|
| macOS (Apple silicon) | `@lute-lang/lute-core-darwin-arm64` | 지원됨 |
| Linux (x86-64) | `@lute-lang/lute-core-linux-x64` | 지원됨 |

지원되지 않는 플랫폼에서는 런처가 지원 매트릭스를 알려주는 실행 가능한 오류와 함께 종료됩니다.
Windows와 musl 기반 Linux는 아직 패키징되지 않았습니다 — 대신 소스에서 빌드하세요.

## 소스에서 빌드하기

Lute의 컴파일러, 체커, CLI는 Rust로 작성되었습니다. Rust 툴체인이 있다면, 저장소 체크아웃에서
CLI를 직접 설치할 수 있습니다:

```sh
cargo install --path crates/lute-cli
```

이 명령은 `lute` 바이너리(크레이트가 `[[bin]] name = "lute"`로 선언함)를 빌드하여 Cargo bin
디렉터리에 배치합니다. 개발 중 임시로 로컬 빌드를 하려면 `cargo build -p lute-cli`가
`./target/debug/lute`를 생성합니다.

## 확인

어떤 경로로 설치했든, 도구가 `PATH`에 있는지 확인하세요:

```
$ lute version
lute toolchain 0.28.0
language      0.28.0
IR schema     0.28.0
```

세 줄은 **toolchain**(이 CLI), 체커가 강제하는 **language**, 그리고 `lute compile`이 모든 산출물에
`irVersion`으로 새겨 넣는 **IR schema**입니다. 릴리스는 세 줄을 모두 그 릴리스 번호로 맞추므로
세 줄에 같은 버전이 보여야 합니다. 축이 왜 셋인지, 엔진이 `irVersion`으로 어떻게 게이팅하는지는
[버전 정책](https://github.com/journeyWorker/lute/blob/main/docs/versioning.md)에 있습니다.

스크립트와 CI에서는 `--json`이 같은 세 축을 하나의 객체로 출력합니다:

```
$ lute version --json
{"toolchain":"0.28.0","language":"0.28.0","ir":"0.28.0"}
```

(`lute --version`도 동작하며 `lute 0.28.0`만 출력합니다 — toolchain 축 하나뿐입니다.)

에디터에서 언어 서버를 쓴다면 같은 빌드인지 확인하세요: `lute-lsp --version`은
`lute-lsp <version>`을 출력하고, `lute doctor`는 `PATH`에서 가장 먼저 찾은 `lute-lsp`를 이 CLI 옆의
것과 비교해 더 오래된 서버(이 플래그 이전 빌드는 버전을 아예 보고하지 못합니다)를 재설치 방법과 함께
알려 줍니다. 그 `lute-lsp`가 npm 런처라면 `lute doctor`는 런처가 띄우는 바이너리를 비교하고, 어느
바이너리인지 알 수 없으면 그 줄 끝에 `(compared by reported version only)`를 붙입니다.

**편집기와 터미널이 파일에 대해 다르게 말하면** — `lute check`가 `ok`라고 하는 파일에 빨간 밑줄이
있거나 그 반대라면 — 터미널을 믿고 프로젝트에서 `lute doctor .`를 실행하세요: 흔한 원인은
`lute`보다 오래된 편집기 언어 서버이고, 업그레이드 뒤 편집기를 다시 시작하면 해결됩니다.

## 다음

[첫 장면 작성하기](/ko/getting-started/first-scene/)로 이동하여, 빈 파일에서 실제 `.lute`
파일을 만들며 매 단계마다 `lute`를 실행해 보세요.

프로젝트 전체에서 시작할 수도 있습니다: `lute init <dir>`는 최소 프로젝트를 만들고,
`lute init --template beats <dir>`는 [비트와 계기](/ko/tooling/play/)로 움직이는 게임을 만듭니다 —
계기 플러그인, 비트, 퀘스트, 로어 엔트리, 플레이 스크립트, 시나리오 테스트까지, 만든 그대로
`lute check-project`, `lute test`, `lute play`가 통과합니다.
