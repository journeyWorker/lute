---
title: 설치
description: bunx, 전역 bun 설치, 또는 Rust 소스로 Lute CLI를 설치하고, 툴체인을 확인한 뒤 첫 장면으로 넘어가세요.
---

Lute는 단일 명령줄 도구 `lute` 하나로 제공됩니다. 이 도구는 `.lute` 시나리오 파일을 읽어
검사(check), 컴파일(compile), 트레이스(trace)하고 그 내용을 살펴봅니다. 현재 언어 버전은
**0.5.2**입니다.

## `bunx`로 빠르게 시작하기

아무것도 영구적으로 설치하지 않고 Lute를 실행하는 가장 빠른 방법은 `bunx`입니다. 이 명령은
게시된 npm 패키지를 가져와 번들된 네이티브 바이너리를 실행합니다:

```sh
bunx lutecli check scene.lute
```

npm 패키지 이름은 `lutecli`이고, 이 패키지가 설치하는 명령은 `lute`입니다. `bunx lutecli <args>`와
전역 설치된 `lute <args>`는 동일한 프로그램입니다.

## 전역 설치

일상적인 사용을 위해 `lute`를 `PATH`에 유지하려면, 패키지를 bun으로 전역 설치하세요:

```sh
bun add -g lutecli
lute check scene.lute
```

`lutecli`는 얇은 런처입니다: 플랫폼을 감지하여, 플랫폼별 선택적 의존성
(`lutecli-core-darwin-arm64` 또는 `lutecli-core-linux-x64`)으로 배포되는 사전 빌드된 네이티브
바이너리로 디스패치합니다. 올바른 바이너리는 설치 시점에 자동으로 선택됩니다.

## 플랫폼 지원

| 플랫폼 | npm 코어 패키지 | 상태 |
|---|---|---|
| macOS (Apple silicon) | `lutecli-core-darwin-arm64` | 지원됨 |
| Linux (x86-64) | `lutecli-core-linux-x64` | 지원됨 |

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

```sh
lute --version
```

## 다음

[첫 장면 작성하기](/ko/getting-started/first-scene/)로 이동하여, 빈 파일에서 실제 `.lute`
파일을 만들며 매 단계마다 `lute`를 실행해 보세요.
