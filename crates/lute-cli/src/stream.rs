//! `lute compile-stream`: stream checked continuation artifacts as flushed NDJSON.

use std::io::{self, Read, Write};
use std::path::Path;
use std::process::ExitCode;

use lute_compile::streaming::{ContinuationCompilation, ContinuationCompiler};
use lute_compile::Artifact;
use lute_core_span::Diagnostic;
use serde::Serialize;

/// One wire event. Borrowing artifacts and diagnostics lets serialization write
/// directly to stdout without building an intermediate JSON value or cloning a
/// full artifact snapshot.
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum Event<'a> {
    Start {
        sequence: u64,
        #[serde(rename = "appendFrom")]
        append_from: usize,
        artifact: &'a Artifact,
    },
    Update {
        sequence: u64,
        #[serde(rename = "appendFrom")]
        append_from: usize,
        artifact: &'a Artifact,
    },
    Finish {
        sequence: u64,
    },
    Error {
        diagnostics: &'a [Diagnostic],
    },
}

/// Compile an immutable scene prefix, then incrementally decode and compile the
/// body arriving on stdin. Exit `0` only after successful EOF finalization, `1`
/// on compiler rejection, and `2` on file/stdin/stdout/UTF-8 failure.
pub fn run(file: &Path, providers: Option<&Path>, project: Option<&Path>) -> ExitCode {
    let Some(built) = crate::build_input(file, providers, project) else {
        return ExitCode::from(2);
    };
    built.report_project_diags();
    let crate::BuiltInput {
        input,
        resolve_error,
        identity,
        ..
    } = built;
    if resolve_error {
        return ExitCode::FAILURE;
    }

    let compiler = match ContinuationCompiler::new(input, identity) {
        Ok(compiler) => compiler,
        Err(diagnostics) => {
            let stdout = io::stdout();
            let mut output = stdout.lock();
            return match write_event(&mut output, &Event::Error { diagnostics: &diagnostics }) {
                Ok(()) => ExitCode::FAILURE,
                Err(error) => output_error(error),
            };
        }
    };

    let stdin = io::stdin();
    let stdout = io::stdout();
    run_stream(stdin.lock(), stdout.lock(), compiler)
}

/// Read bounded byte chunks rather than reading stdin to EOF. A carry buffer
/// holds only a split UTF-8 scalar (at most three trailing bytes in valid
/// input), so every valid prefix reaches the compiler as soon as `read` returns.
fn run_stream<R: Read, W: Write>(
    mut input: R,
    mut output: W,
    mut compiler: ContinuationCompiler,
) -> ExitCode {
    if let Err(error) = write_event(
        &mut output,
        &Event::Start {
            sequence: 0,
            append_from: 0,
            artifact: compiler.artifact(),
        },
    ) {
        return output_error(error);
    }

    let mut sequence = 0;
    let mut bytes = [0_u8; 8192];
    let mut pending = Vec::with_capacity(8195);
    loop {
        let read = match input.read(&mut bytes) {
            Ok(0) => break,
            Ok(read) => read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => {
                eprintln!("lute compile-stream: cannot read stdin: {error}");
                return ExitCode::from(2);
            }
        };
        pending.extend_from_slice(&bytes[..read]);

        match std::str::from_utf8(&pending) {
            Ok(text) => {
                if !text.is_empty() {
                    match emit_compilation(&mut output, &mut sequence, compiler.push(text)) {
                        Ok(false) => {}
                        Ok(true) => return ExitCode::FAILURE,
                        Err(error) => return output_error(error),
                    }
                }
                pending.clear();
            }
            Err(error) => {
                let valid_up_to = error.valid_up_to();
                if valid_up_to > 0 {
                    let text = std::str::from_utf8(&pending[..valid_up_to])
                        .expect("valid_up_to always delimits valid UTF-8");
                    match emit_compilation(&mut output, &mut sequence, compiler.push(text)) {
                        Ok(false) => {}
                        Ok(true) => return ExitCode::FAILURE,
                        Err(error) => return output_error(error),
                    }
                    pending.drain(..valid_up_to);
                }
                if error.error_len().is_some() {
                    eprintln!("lute compile-stream: stdin is not valid UTF-8");
                    return ExitCode::from(2);
                }
                // The remaining bytes are an incomplete scalar split across
                // reads. Do not pass them to the `&str` core API yet.
            }
        }
    }

    if !pending.is_empty() {
        eprintln!("lute compile-stream: stdin ends with incomplete UTF-8");
        return ExitCode::from(2);
    }

    let compilation = compiler.finish();
    match emit_compilation(&mut output, &mut sequence, compilation) {
        Ok(true) => ExitCode::FAILURE,
        Err(error) => output_error(error),
        Ok(false) => match write_event(&mut output, &Event::Finish { sequence }) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => output_error(error),
        },
    }
}

/// Flush accepted updates before reporting a later rejection from the same
/// compiler call. Returns `true` exactly when an error event was emitted.
fn emit_compilation<W: Write>(
    output: &mut W,
    sequence: &mut u64,
    compilation: ContinuationCompilation,
) -> io::Result<bool> {
    for update in compilation.updates {
        *sequence = update.sequence;
        write_event(
            output,
            &Event::Update {
                sequence: update.sequence,
                append_from: update.append_from,
                artifact: &update.artifact,
            },
        )?;
    }
    if compilation.diagnostics.is_empty() {
        return Ok(false);
    }
    write_event(
        output,
        &Event::Error {
            diagnostics: &compilation.diagnostics,
        },
    )?;
    Ok(true)
}

/// Serialize exactly one compact JSON value, terminate it with a newline, and
/// flush it before returning. This is the command's latency and framing
/// boundary, not merely an eventual stdout write.
fn write_event<W: Write>(output: &mut W, event: &Event<'_>) -> io::Result<()> {
    serde_json::to_writer(&mut *output, event).map_err(io::Error::other)?;
    output.write_all(b"\n")?;
    output.flush()
}

fn output_error(error: io::Error) -> ExitCode {
    eprintln!("lute compile-stream: cannot write stdout: {error}");
    ExitCode::from(2)
}
