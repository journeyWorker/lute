---
title: 스트리밍 연속 컴파일러
description: 검사된 장면 템플릿에 일반 Lute 본문을 이어 붙이고, lute compile-stream으로 즉시 플러시되는 전체 산출물 스냅샷을 받아 런타임 커서와 상태를 안전하게 유지합니다.
---

검사형 연속 컴파일러(checked continuation compiler)는 호스트가 관리하는 장면
템플릿의 마지막 샷에 일반 Lute **샷 본문 소스**를 이어 붙이고, 완성된 단위마다
검사된 일반 IR 산출물을 반환합니다. stdin의 EOF를 기다리지 않으므로 대화형 저작이나
생성형 파이프라인에서 첫 유효 결과를 일찍 받을 수 있습니다.

별도의 스트리밍 문법이나 JSON AST 패치 프로토콜이 아닙니다. 대사는 언제나 실제
Lute 인라인 내용 줄 문법을 사용합니다.

```lute
@guide: 복도의 불빛이 하나씩 켜진다.
```

[향후 규범 제안](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.16.1.md)과
[구현 설계](https://github.com/journeyWorker/lute/blob/main/docs/superpowers/specs/2026-09-14-streaming-continuation-compiler-design.md)는
Unreleased 도구 기능을 설명합니다. 이미 배포된 버전을 주장하거나 버전을 올리는 문서가
아닙니다.

## 끝까지 실행할 수 있는 예제

먼저 신뢰할 수 있는 완전한 장면 템플릿을 만듭니다. `kind: scene`이어야 하고 샷이
하나 이상 있어야 합니다. 연속 입력은 마지막 샷의 본문에만 추가됩니다.

```console
$ cat > /tmp/live-scene.lute <<'LUTE'
---
kind: scene
id: live-demo
---

## Live
@narrator: 연결이 열렸다.
LUTE
```

유효한 대사 두 줄 사이에 1초 동안 stdin을 열어 둡니다.

```console
$ { printf '@guide: 첫 번째 검사 완료 줄.\n'; sleep 1; printf '@guide: 두 번째 검사 완료 줄.\n'; } \
    | lute compile-stream /tmp/live-scene.lute
{"kind":"start","sequence":0,"appendFrom":0,"artifact":{...}}
{"kind":"update","sequence":1,"appendFrom":1,"artifact":{...}}
{"kind":"update","sequence":2,"appendFrom":2,"artifact":{...}}
{"kind":"finish","sequence":2}
```

이 문서의 `{...}`는 줄임 표기입니다. 실제 `start`와 `update`에는 완전한 일반 Lute
산출물이 들어가고, 각 줄은 유효한 NDJSON입니다. 첫 번째 `update`는 1초 대기 중에
즉시 플러시되며 EOF까지 기다리지 않습니다.

프로젝트 장면에는 일반 컴파일과 같은 해석 인자를 사용합니다.

```console
$ printf '@guide: 프로젝트 설정으로 검사한 줄.\n' \
    | lute compile-stream scenes/live.lute --project . --providers snapshots
```

CLI는 프로젝트, 프로바이더, 컴포넌트, 기본값, identity 템플릿을 한 번만 해석하며
템플릿 파일은 수정하지 않습니다. 종료 코드는 다음과 같습니다.

| 코드 | 의미 |
| --- | --- |
| `0` | stdin EOF 뒤 최종화에 성공했습니다. |
| `1` | 문법, 의미, 컴파일 또는 스트리밍 서비스가 입력을 거부했습니다. |
| `2` | 호출법, 파일 I/O, 끊어진 stdout 또는 잘못된 UTF-8 입력입니다. |

호출/I/O 오류는 stderr에 씁니다. stdout이 끊어지면 전달할 수 없는 입력을 계속
소비하지 않고 즉시 중단합니다.

## NDJSON 계약

- `start`는 검사된 초기 템플릿의 전체 산출물입니다.
- `update`는 소스 순서대로 승인된 최상위 본문 단위 하나의 전체 스냅샷입니다.
- `finish`는 EOF 최종화에 성공했을 때만 나옵니다.
- 거부 시 마지막 레코드는
  `{"kind":"error","diagnostics":[...]}`이며 `finish`가 뒤따르지 않습니다.

`appendFrom`은 **직전 스냅샷의 `commands.length`**입니다. 새 배열 영역의 시작을
설명할 뿐, 런타임 PC가 아니며 그 뒤의 모든 명령을 실행하라는 뜻도 아닙니다. 일반
choice/match/hub/jump 디스패처가 실제 실행 경로를 고릅니다.

## Rust API

공개 API는 `lute_compile::streaming`에 있습니다.

```rust
pub struct ContinuationCompiler { /* private */ }

impl ContinuationCompiler {
    pub fn new(
        input: CheckInput,
        identity: IdentityTemplates,
    ) -> Result<Self, Vec<Diagnostic>>;

    pub fn artifact(&self) -> &Artifact;
    pub fn push(&mut self, chunk: &str) -> ContinuationCompilation;
    pub fn finish(&mut self) -> ContinuationCompilation;
}

pub struct CompilationUpdate {
    pub sequence: u64,
    pub append_from: usize,
    pub artifact: Artifact,
}

pub struct ContinuationCompilation {
    pub updates: Vec<CompilationUpdate>,
    pub need_more: Option<NeedMoreInput>,
    pub diagnostics: Vec<Diagnostic>,
    pub finished: bool,
}
```

`CheckInput`에는 완전한 접두 소스와 해석된 capability, provider, import,
component, defaults, URI, 분석 모드가 들어갑니다. `IdentityTemplates`도 생성 시점에
고정됩니다. `new`는 접두 소스를 즉시 검사하고 컴파일합니다. 장면이 아니거나 샷이
없거나 기존 오류가 있으면 본문을 받기 전에 진단을 반환합니다. `artifact()`는 가장
최근에 승인된 전체 일반 산출물입니다.

한 번의 `push`가 여러 단위를 완성할 수 있습니다. 같은 호출에서 뒤쪽 단위가
실패해도 앞에서 승인된 `updates`는 유효합니다. 실패한 단위와 그 뒤의 단위만 IR을
만들지 않으므로 앞선 업데이트를 버리면 안 됩니다.

## 허용되는 소스 경계

연속 입력에는 기존의 합법적인 샷 본문 문법을 그대로 쓸 수 있습니다.

- `@speaker{attributes}: text` 형식의 대사와 내레이션
- 일반 디렉티브
- 완전히 닫힌 `<branch>`, `<match>`, `<hub>`, `<timeline>` 블록
- 컴포넌트 사용과 해석된 플러그인 디렉티브

frontmatter를 바꾸거나 샷 헤딩/레이블을 추가하거나 퀘스트 루트를 만들 수는 없습니다.
아직 나오지 않은 forward target이 있어야만 기존 컴파일이 성공하는 단위는 지금
실패합니다. 컴파일러는 미래 소스를 추측하거나 닫는 태그를 만들어 내지 않습니다.

템플릿과 프로젝트 입력은 호스트가 소유하고 신뢰해야 합니다. Lute가 연속 본문을
검사한다고 해서 생성된 텍스트 자체가 신뢰 가능해지는 것은 아닙니다. 이 기능은 LLM
클라이언트, 게임 엔진, 권한 시스템 또는 안전한 AI capability sandbox가 아닙니다.
원격 호출, 브리지 효과, 게시, 플레이어 상태 저장도 수행하지 않습니다.

## 청크, 완성 단위, EOF

네트워크 청크 경계는 문법이 아닙니다. `push(&str)`는 실제 줄, 따옴표로 감싼 속성,
블록 주석 또는 중첩 엘리먼트 중간에서 끝나도 됩니다. 잎 단위는 실제 개행 뒤에,
바깥 블록은 대응하는 닫는 태그가 있는 실제 줄 뒤에만 방출됩니다.

`need_more`의 `Line`, `QuotedAttribute`, `BlockComment`,
`NestedBlock { open_tags }`는 오류가 아니라 보관 중인 접미사 상태입니다. Rust API는
`&str`을 받으므로 raw byte 스트림을 다루는 호출자는 분할된 UTF-8을 먼저 버퍼링하고
디코딩해야 합니다. CLI는 이를 처리하고 잘못된 UTF-8을 종료 코드 `2`로 거부합니다.

`finish()`는 EOF를 마지막 완전한 잎 단위의 개행으로 인정합니다. 하지만 `</tag>`,
`*/`, forward target은 만들지 않습니다. 열린 블록은 오류이며 성공이나 실패 뒤에는
컴파일러가 닫힙니다. 이후 호출은 `E-STREAM-CLOSED`입니다.

EOF와 `::end`는 다릅니다.

- EOF는 컴파일러 입력을 닫고 성공 시 `finish` 레코드를 허용합니다.
- `::end{reason="complete"}`는 런타임 실행을 일찍 끝낼 수 있는 일반 Lute 명령입니다.
  stdin을 닫지 않으며 뒤에 들어온 잘못된 소스를 무시하게 만들지도 않습니다.

## 누적 컴파일 비용

완성된 단위마다 지금까지 누적된 템플릿과 본문을 **기존** parser, checker,
normalization, component expansion, stage injection, lowering, address assignment,
artifact assembly에 다시 통과시킵니다. `lute compile`과 어긋날 별도 lowering 알고리즘은
없습니다.

따라서 첫 검사 결과가 빨리 나오지만 총비용이 줄어드는 것은 아닙니다. 첫 단위는 첫
접두 전체, 두 번째 단위는 더 길어진 접두 전체를 다시 컴파일합니다. 닫히지 않은 긴
블록은 닫힐 때까지 아무것도 방출하지 않습니다. 큰 세션에서는 측정이 필요하며 단위당
고정 비용의 점근적 증분 컴파일이라고 설명하면 안 됩니다.

## 런타임 스냅샷 교체 규칙

각 업데이트는 완전하고 불변인 프로그램 스냅샷입니다. 소비자는 다음을 수행해야 합니다.

1. 기존 산출물을 `update.artifact`로 교체합니다.
2. `addr -> command index` 조회를 다시 만듭니다.
3. **숫자 명령 커서**, 현재 상태, facts, 선택된 제어 흐름 스택, 호스트의 효과/
   idempotency 기록은 유지합니다.
4. 새로 선언된 상태 슬롯만 초기화합니다.
5. 유지한 커서에서 일반 디스패처를 계속 실행합니다.

기존 상태의 default나 seed fact를 다시 적용하면 안 됩니다. 단지 새 레코드라는 이유로
`appendFrom`부터 실행하면 안 됩니다.

Lute 주소는 한 산출물 안에서 균일하게 padding됩니다. 명령이 늘면 `001-0900`이
`001-00900`처럼 전체적으로 넓어질 수 있으므로 주소 조회를 다시 만들어야 합니다.
컴파일러는 typed address/control target의 숫자 `(shot, index)` 의미만 비교해 이
서식 변화를 허용합니다. 임의의 payload 문자열은 정규화하지 않습니다.

추가 소스가 과거 line identity, stage-injected command, payload, control-flow 의미 또는
기존 상태 엔트리를 바꾼다면 `E-STREAM-PREFIX-CHANGED`로 거부합니다. 이미 전달된 IR은
수정하지 않습니다.

현재 스냅샷의 끝에 도달하면 다음 업데이트를 기다립니다. 아직 열린 스트림의 배열 끝은
장면 완료가 아닙니다. 성공한 `finish` 또는 일반 authored `end` 명령만 완료를
결정합니다. 호스트 side effect는 스냅샷 교체 뒤에도 idempotent해야 합니다.

## 진단

일반 문법/의미/해석/컴파일 진단은 그대로 유지되며 템플릿, 승인된 단위, 실패 단위를
합친 누적 소스 위치를 가리킵니다. 서비스 진단은 네 가지입니다.

| 코드 | 의미 |
| --- | --- |
| `E-STREAM-TEMPLATE` | 초기 입력으로 검사된 장면 접두와 산출물을 만들 수 없습니다. |
| `E-STREAM-BODY` | 추가 소스가 마지막 샷 본문 영역에 허용되지 않습니다. |
| `E-STREAM-PREFIX-CHANGED` | 후보가 이미 승인된 명령 또는 상태를 소급 변경합니다. |
| `E-STREAM-CLOSED` | 성공/실패로 종료된 인스턴스를 다시 사용했습니다. |

## parser 전용 API

산출물 없이 lossless 문법 framing만 필요하면
`lute_syntax::incremental::IncrementalContinuationParser`를 사용합니다.
`ContinuationUnit`은 정확한 소스, 스트림 기준 반열린 byte range, `Vec<Node>`, 해당
소스 기준 문법 진단을 보존합니다. `ContinuationFinalization`은 닫히지 않은 접미사를
`IncompleteContinuation`으로 분리합니다.

이 API는 프로젝트/provider 해석, 의미 검사, 컴포넌트 병합, lowering, addressing, IR
조립을 하지 않습니다. 소스 도구에는 유용하지만 런타임에 전달할 산출물이 필요하다면
`ContinuationCompiler`를 사용해야 합니다.

## 원문 계약

- [런타임 가이드](https://github.com/journeyWorker/lute/blob/main/docs/runtime/incremental-continuations.md)
- [향후 규범 제안](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.16.1.md)
- [`lute-compile` streaming 소스](https://github.com/journeyWorker/lute/blob/main/crates/lute-compile/src/streaming.rs)
- [`lute-syntax` continuation parser](https://github.com/journeyWorker/lute/blob/main/crates/lute-syntax/src/incremental.rs)
