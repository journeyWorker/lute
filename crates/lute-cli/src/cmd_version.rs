//! `lute version`.

use std::process::ExitCode;

/// Print the three independent version axes (docs/versioning.md): the
/// TOOLCHAIN version (this CLI + the workspace crates, `CARGO_PKG_VERSION`),
/// the LANGUAGE version ([`lute_check::LUTE_LANG_VERSION`] — the grammar and
/// semantics the checker enforces), and the IR schema version
/// ([`lute_compile::LUTE_IR_VERSION`] — stamped as `irVersion` in every
/// compiled artifact). The three bump independently (a toolchain release need
/// not move the language, and vice versa). `--json` prints one stable-keyed
/// object (keys emitted in `toolchain`/`language`/`ir` order, values
/// JSON-escaped); human mode prints one labeled line each. Always exit `0`.
pub(crate) fn run_version(json: bool) -> ExitCode {
    let toolchain = env!("CARGO_PKG_VERSION");
    let language = lute_check::LUTE_LANG_VERSION;
    let ir = lute_compile::LUTE_IR_VERSION;
    if json {
        // Build the object by hand so the key order is fixed and the values
        // are correctly JSON-escaped (serde_json::to_string on a &str is
        // infallible — a Rust string is always valid UTF-8).
        println!(
            "{{\"toolchain\":{},\"language\":{},\"ir\":{}}}",
            serde_json::to_string(toolchain).expect("string serializes"),
            serde_json::to_string(language).expect("string serializes"),
            serde_json::to_string(ir).expect("string serializes"),
        );
    } else {
        println!("lute toolchain {toolchain}");
        println!("language      {language}");
        println!("IR schema     {ir}");
    }
    ExitCode::SUCCESS
}
