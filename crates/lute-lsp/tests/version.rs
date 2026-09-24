//! `lute-lsp --version` prints the toolchain version and exits instead of
//! serving (dsl 0.22.0 §13) — `lute doctor` compares it with its own to catch
//! an editor running a stale language server from `PATH`.

use std::process::{Command, Stdio};

#[test]
fn version_flag_prints_the_toolchain_version_and_exits() {
    let out = Command::new(env!("CARGO_BIN_EXE_lute-lsp"))
        .arg("--version")
        // A server that ignored the flag would block reading LSP frames from
        // stdin; a closed stdin makes that a prompt exit rather than a hang,
        // and the missing version line still fails the test.
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        format!("lute-lsp {}\n", env!("CARGO_PKG_VERSION"))
    );
}
