---
title: 편집기와 LSP
description: .lute를 위한 편집기 지원 — lute-lsp 설치, 클라이언트 연결, 프로젝트/플러그인/capability-permission 진단과 completion을 CLI 검사기와 공유하는 방법, 그리고 자신의 바이너리가 교체되었음을 감지하는 서버입니다.
---

모든 편집기 클라이언트는 `.lute`를 `lute` 언어와 연결하고 같은 `lute-lsp` stdio 언어 서버를 구동하므로 어디서나 동일한 언어 지능을 얻습니다. 프로젝트 루트는 `lute.project.yaml`, 그 다음 `.git` 마커로 찾습니다.

## `lute-lsp` 설치

모든 클라이언트는 `PATH`에서 `lute-lsp` 바이너리를 실행합니다. 한 번 설치하면 됩니다:

```console
$ cargo install --path crates/lute-lsp     # -> ~/.cargo/bin/lute-lsp
$ cargo install --path crates/lute-cli     # optional: the `lute` CLI checker
```

개발 checkout에서는 `cargo build -p lute-lsp`를 실행하고 `target/debug`를 `PATH`에 추가하세요. 해석되는지 확인하세요:

```console
$ command -v lute-lsp
```

## 클라이언트

| 편집기 | 설정 | 정적 하이라이팅 |
|---|---|---|
| **VS Code** | `editors/vscode/` — `npm install` 후 <kbd>F5</kbd>, 또는 `vsce package` + `code --install-extension`. | TextMate 문법 + LSP semantic token |
| **Neovim** | `editors/nvim/` — `plugin/lute.lua`를 `runtimepath`에 넣거나 nvim-lspconfig snippet 사용. | tree-sitter 문법(`tree-sitter-lute/`) + LSP semantic token |
| **Oh My Pi** | `.omp/lsp.json` — 바이너리가 `PATH`에 있고 root marker가 있으면 `.lute`에서 `lute-lsp`를 자동 감지합니다. 추가 설정이 없습니다. | LSP semantic token |

## 언어 기능

서버가 실행되면 다음을 사용할 수 있습니다:

- **진단** — 전체 checker(프로젝트 / 플러그인 / `uses` / `extends` / 컴포넌트 / [capability-permission](/ko/tooling/capability-permissions/) 인식)를 입력하면서 받습니다. 금지된 작성 효과는 CLI와 같은 `E-PERMISSION-*` 코드와 span을 사용합니다.
- **Hover** — 디렉티브, ref, 상태 경로, attribute의 타입과 문서.
- **Completion** — 디렉티브, attribute, `@ref`, 상태 경로, choice id. 문서의 유효한 project/profile 권한으로 금지된 디렉티브 후보는 제외되며, 디렉티브 이름이나 `service/operation` 중 하나라도 거부된 bridge 디렉티브도 제외됩니다.
- **정의로 이동 / 참조** — def, 컴포넌트, 스키마 선언.
- **접기 및 문서 기호** — 섹션, timeline, branch, match.
- **Semantic token** — 계층을 인식한 하이라이팅(content / staging / logic).

LSP는 CLI와 같은 manifest API를 통해 문서의 프로젝트, `global`/상위/선택된 profile 권한, capability를 해석합니다. 별도의 권한 부여 알고리즘을 구현하지 않습니다. 편집기의 필터링은 안내일 뿐 보안 경계가 아닙니다. 소스를 독립적으로 수락하는 호스트는 CLI `--permission-profile`로 신뢰된 상한을 고정한 다음 그 상한으로 검사하거나 컴파일합니다.

## 바이너리보다 오래된 서버

편집기는 `lute-lsp`를 한 번 시작하고 계속 실행하므로 툴체인을 재설치하거나 개발 checkout을 다시 빌드하면 아직 실행 중인 서버 아래의 바이너리가 교체됩니다. dsl 0.26.0 이전에는 서버가 계속 이전 빌드의 진단을 발행해 저장할 때마다 수백 개의 false 진단이 발생하곤 했습니다. dsl 0.26.0부터 서버는 매 분석 시 자신이 시작한 바이너리와 현재 바이너리 파일의 길이, 수정 시간, inode를 비교합니다. 파일이 교체되거나 제거되면 어떤 결과 대신 문서 상단에 진단 하나를 발행합니다. 심각도는 error, 코드는 `lute-lsp-stale`, source는 `lute-lsp`이며 자신의 버전을 이름 붙입니다:

> stale server: lute-lsp `<version>` was started from `<path>`, which has been replaced since, so its diagnostics would come from an older build — restart the language server

Checker가 뒷받침하는 요청 — hover, completion, 정의로 이동, 참조, code action — 은 서버가 재시작될 때까지 응답하지 않으며, 이전 빌드의 결과로 응답하지도 않습니다. 언어 서버(또는 편집기)를 재시작하면 새 빌드가 인계합니다.

[`lute doctor`](/ko/tooling/cli/#doctor)는 같은 상태를 명령줄에서 찾습니다. 바이너리가 교체되기 전에 시작된 실행 중인 `lute-lsp`, `lute` 옆의 다른 빌드, 다른 버전을 보고하는 `PATH`의 `lute-lsp`를 찾습니다. `lute doctor --strict`는 검사 하나라도 실패하면 1로 종료하므로 하네스는 낡은 설정에서 시작하기를 거부할 수 있습니다:

```console
$ lute doctor --project . --strict
…
  ✗ running lute-lsp: pid 12430 (/Users/you/.bun/bin/lute-lsp) started before its binary was replaced, so it runs an older build
      → restart the editor (or its language server) so it launches this toolchain's lute-lsp
$ echo $?
1
```

## 하이라이팅 모델

하이라이팅은 결합되는 두 계층입니다. 편집기별 **정적 문법**이 즉각적인 기본 하이라이팅을 제공합니다 — VS Code에서는 TextMate 문법, Neovim에서는 tree-sitter 문법입니다. 이후 `lute-lsp`의 **LSP semantic token**이 정적 문법으로는 알 수 없는 프로젝트 / 플러그인 / 스키마 지식으로 이를 정교화합니다. tree-sitter 문법은 `@speaker{attrs}: text` 콘텐츠 줄, `//` 주석, `{{…}}` 보간, `<hub>` 블록, `<when is>` 패턴, `<quest>`/`<objective>`/`<on>` 중첩, `::assert`/`::retract` 관계형 leaf를 파싱합니다. `lute-lsp`의 semantic token은 프로젝트를 인식한 정교화에서 권위 있는 역할을 유지합니다.
