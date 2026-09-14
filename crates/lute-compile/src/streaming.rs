//! Checked incremental compilation for append-only scene continuations.
//!
//! Framing is incremental, but lowering deliberately is not: every completed
//! unit is appended to the original scene source and passed through the ordinary
//! checker/compiler pipeline. This keeps normalization, components, stage
//! injection, identity, and diagnostics identical to batch compilation.

use std::ops::Range;

use lute_check::{check, CheckInput};
use lute_core_span::{Diagnostic, Layer, Severity, Span, TextIndex};
use lute_manifest::project::IdentityTemplates;
use lute_syntax::incremental::{ContinuationUnit, IncrementalContinuationParser};

pub use lute_syntax::incremental::NeedMoreInput;

use crate::{compile_with_check, Artifact, Command, DocKind};

pub const E_STREAM_TEMPLATE: &str = "E-STREAM-TEMPLATE";
pub const E_STREAM_BODY: &str = "E-STREAM-BODY";
pub const E_STREAM_PREFIX_CHANGED: &str = "E-STREAM-PREFIX-CHANGED";
pub const E_STREAM_CLOSED: &str = "E-STREAM-CLOSED";

/// One accepted, immutable artifact snapshot.
#[derive(Clone, Debug)]
pub struct CompilationUpdate {
    pub sequence: u64,
    /// Command count in the preceding accepted snapshot.
    pub append_from: usize,
    pub artifact: Artifact,
}

/// Result of one [`ContinuationCompiler::push`] or
/// [`ContinuationCompiler::finish`] call.
#[derive(Clone, Debug)]
pub struct ContinuationCompilation {
    pub updates: Vec<CompilationUpdate>,
    pub need_more: Option<NeedMoreInput>,
    pub diagnostics: Vec<Diagnostic>,
    /// True once EOF succeeds or any terminal error has occurred.
    pub finished: bool,
}

/// Checked append-only compiler for the last shot of a scene template.
pub struct ContinuationCompiler {
    input: CheckInput,
    identity: IdentityTemplates,
    parser: Option<IncrementalContinuationParser>,
    artifact: Artifact,
    sequence: u64,
    terminal: bool,
}

impl ContinuationCompiler {
    /// Check and compile a complete scene prefix.
    pub fn new(input: CheckInput, identity: IdentityTemplates) -> Result<Self, Vec<Diagnostic>> {
        let (document, _) = lute_syntax::parse(&input.text);
        let checked = check(&input);
        let mut initial_diagnostics = checked.diagnostics.clone();
        let artifact = compile_with_check(&input, checked, &identity)?;
        if !matches!(artifact.kind, DocKind::Scene) || document.shots.is_empty() {
            initial_diagnostics.push(service_diagnostic(
                E_STREAM_TEMPLATE,
                "continuation template must be a scene with at least one shot",
                &input.text,
                0..input.text.len(),
            ));
            return Err(initial_diagnostics);
        }

        Ok(Self {
            input,
            identity,
            parser: Some(IncrementalContinuationParser::new()),
            artifact,
            sequence: 0,
            terminal: false,
        })
    }

    /// Latest accepted ordinary artifact.
    pub fn artifact(&self) -> &Artifact {
        &self.artifact
    }

    /// Append a UTF-8 text chunk and compile every newly completed unit.
    pub fn push(&mut self, chunk: &str) -> ContinuationCompilation {
        if self.terminal {
            return self.closed_result();
        }
        let batch = self
            .parser
            .as_mut()
            .expect("non-terminal compiler owns its parser")
            .push(chunk);
        self.accept_units(batch.units, batch.need_more)
    }

    /// Treat EOF as the delimiter for a final leaf and close the compiler.
    pub fn finish(&mut self) -> ContinuationCompilation {
        if self.terminal {
            return self.closed_result();
        }
        let finalization = self
            .parser
            .take()
            .expect("non-terminal compiler owns its parser")
            .finish();
        let mut result = self.accept_units(finalization.units, None);
        if self.terminal {
            return result;
        }

        if let Some(incomplete) = finalization.incomplete {
            let start = self.input.text.len();
            self.input.text.push_str(&incomplete.source);
            let end = self.input.text.len();
            result.diagnostics = match self.compile_current() {
                Ok((_, mut diagnostics)) => {
                    diagnostics.push(service_diagnostic(
                        E_STREAM_BODY,
                        "incomplete continuation at end of input",
                        &self.input.text,
                        start..end,
                    ));
                    diagnostics
                }
                Err(diagnostics) => diagnostics,
            };
            self.input.text.truncate(start);
            self.terminal = true;
            result.finished = true;
            result.need_more = None;
            return result;
        }

        self.terminal = true;
        result.finished = true;
        result.need_more = None;
        result
    }

    fn accept_units(
        &mut self,
        units: Vec<ContinuationUnit>,
        need_more: Option<NeedMoreInput>,
    ) -> ContinuationCompilation {
        let mut updates = Vec::with_capacity(units.len());
        let mut diagnostics = Vec::new();

        for unit in units {
            let start = self.input.text.len();
            self.input.text.push_str(&unit.source);
            let end = self.input.text.len();

            if forbidden_body_scaffolding(&unit.source) {
                diagnostics.push(service_diagnostic(
                    E_STREAM_BODY,
                    "continuation body cannot contain frontmatter or headings",
                    &self.input.text,
                    start..end,
                ));
                self.input.text.truncate(start);
                self.terminal = true;
                return ContinuationCompilation {
                    updates,
                    need_more: None,
                    diagnostics,
                    finished: true,
                };
            }

            let (candidate, candidate_diagnostics) = match self.compile_current() {
                Ok(compiled) => compiled,
                Err(errors) => {
                    self.input.text.truncate(start);
                    self.terminal = true;
                    return ContinuationCompilation {
                        updates,
                        need_more: None,
                        diagnostics: errors,
                        finished: true,
                    };
                }
            };

            if !preserves_emitted_prefix(&self.artifact, &candidate) {
                diagnostics = candidate_diagnostics;
                diagnostics.push(service_diagnostic(
                    E_STREAM_PREFIX_CHANGED,
                    "appended source would change previously emitted commands or state",
                    &self.input.text,
                    start..end,
                ));
                self.input.text.truncate(start);
                self.terminal = true;
                return ContinuationCompilation {
                    updates,
                    need_more: None,
                    diagnostics,
                    finished: true,
                };
            }

            let append_from = self.artifact.commands.len();
            self.sequence += 1;
            self.artifact = candidate;
            diagnostics = candidate_diagnostics;
            updates.push(CompilationUpdate {
                sequence: self.sequence,
                append_from,
                artifact: self.artifact.clone(),
            });
        }

        ContinuationCompilation {
            updates,
            need_more,
            diagnostics,
            finished: false,
        }
    }

    /// Compile current cumulative source through the ordinary checker/compiler.
    /// On success the returned diagnostics are the checker's complete warning
    /// stream; on failure existing checker/compile diagnostics are preserved.
    fn compile_current(&self) -> Result<(Artifact, Vec<Diagnostic>), Vec<Diagnostic>> {
        let checked = check(&self.input);
        let checker_diagnostics = checked.diagnostics.clone();
        match compile_with_check(&self.input, checked, &self.identity) {
            Ok(artifact) => Ok((artifact, checker_diagnostics)),
            Err(mut compile_diagnostics) => {
                if checker_diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.severity == Severity::Error)
                {
                    Err(compile_diagnostics)
                } else {
                    let mut diagnostics = checker_diagnostics;
                    diagnostics.append(&mut compile_diagnostics);
                    Err(diagnostics)
                }
            }
        }
    }

    fn closed_result(&self) -> ContinuationCompilation {
        let at = self.input.text.len();
        ContinuationCompilation {
            updates: Vec::new(),
            need_more: None,
            diagnostics: vec![service_diagnostic(
                E_STREAM_CLOSED,
                "continuation compiler is closed",
                &self.input.text,
                at..at,
            )],
            finished: true,
        }
    }
}

fn forbidden_body_scaffolding(source: &str) -> bool {
    let Ok(visible) = lute_syntax::lex::strip_comments_checked(source) else {
        return false;
    };
    visible.lines().any(|line| {
        let trimmed = line.trim();
        let quest_root = trimmed
            .strip_prefix("<quest")
            .and_then(|rest| rest.as_bytes().first())
            .is_some_and(|next| next.is_ascii_whitespace() || *next == b'>');
        trimmed == "---"
            || trimmed.starts_with("# ")
            || trimmed.starts_with("## ")
            || quest_root
    })
}

/// Compare immutable commands after normalizing only typed address slots and
/// control targets. Arbitrary strings in command payloads are never touched.
fn preserves_emitted_prefix(old: &Artifact, new: &Artifact) -> bool {
    if new.commands.len() < old.commands.len() {
        return false;
    }
    if !old.commands.iter().zip(&new.commands).all(|(old, new)| {
        match (canonical_command(old), canonical_command(new)) {
            (Some(old), Some(new)) => old == new,
            _ => false,
        }
    }) {
        return false;
    }

    old.state.iter().all(|old_entry| {
        new.state
            .iter()
            .find(|new_entry| new_entry.path == old_entry.path)
            .is_some_and(|new_entry| {
                match (
                    serde_json::to_value(old_entry),
                    serde_json::to_value(new_entry),
                ) {
                    (Ok(old), Ok(new)) => old == new,
                    _ => false,
                }
            })
    })
}

fn canonical_command(command: &Command) -> Option<serde_json::Value> {
    let mut command = command.clone();
    canonicalize_address(command.addr_mut());
    command.for_each_target(&mut canonicalize_address);
    serde_json::to_value(command).ok()
}

fn canonicalize_address(address: &mut String) {
    let Some((shot, index)) = address.split_once('-') else {
        return;
    };
    if shot.is_empty()
        || index.is_empty()
        || !shot.bytes().all(|byte| byte.is_ascii_digit())
        || !index.bytes().all(|byte| byte.is_ascii_digit())
    {
        return;
    }
    let shot = shot.trim_start_matches('0');
    let index = index.trim_start_matches('0');
    let shot = if shot.is_empty() { "0" } else { shot };
    let index = if index.is_empty() { "0" } else { index };
    *address = format!("{shot}-{index}");
}

fn service_diagnostic(code: &str, message: &str, source: &str, range: Range<usize>) -> Diagnostic {
    let start = range.start.min(source.len());
    let end = range.end.min(source.len()).max(start);
    Diagnostic {
        code: code.to_string(),
        severity: Severity::Error,
        message: message.to_string(),
        span: Span::from_bytes(&TextIndex::new(source), start, end),
        layer: Layer::Content,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    }
}
