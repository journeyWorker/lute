use lute_core_span::{Diagnostic, Layer, Severity, Span};

/// `W-COMPONENT-UNVERIFIED`: a standalone component check with no caller in
/// scope (dsl 0.10.0 §9 rule 4, **D-W**).
pub const W_COMPONENT_UNVERIFIED: &str = "W-COMPONENT-UNVERIFIED";

/// Which of §9 rule 4's two "no caller in scope" disjuncts produced the
/// warning. They are not the same situation and they do not have the same
/// remedy: one says the tool could not look for a caller, the other says it
/// looked and found none.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComponentScope {
    /// No project resolved at all — `lute check <component>` with no
    /// `--project`. Nothing was searched; the author's next step is to name a
    /// project.
    NoProject,
    /// A project resolved and no document under it `::use`s this component.
    /// The search happened and came back empty; the author's next step is to
    /// find out why the component is unused.
    NoImporter,
}

/// Build the `W-COMPONENT-UNVERIFIED` warning.
///
/// **D-W**: `#23`'s own fix list says the standalone leg must "either forward
/// the caller-side verdict or refuse to claim `ok`". With a caller in scope it
/// forwards; with none it refuses, and says exactly what the verdict does and
/// does not cover. A bare `ok` here is the contradiction being closed: the
/// component's own `uses:` is the one vocabulary that NEVER applies at runtime
/// (`0.9.0 §6.1`), because it is discarded at `::use`.
///
/// Both disjuncts state the same COVERAGE — §9 requires the verdict to say what
/// it does cover — and differ in the one sentence that says which situation
/// produced it and what to do about it. An author who forgot `--project` and an
/// author whose component is genuinely unused are two different people.
pub fn component_unverified_diag(component: &str, at: Span, scope: ComponentScope) -> Diagnostic {
    // The two disjuncts differ in what they can HONESTLY claim to have done.
    // `NoProject` means nothing was searched; `NoImporter` means the search ran
    // and came back empty. Collapsing them into one string would tell the
    // author who forgot `--project` that their component is unused.
    let situation = match scope {
        ComponentScope::NoProject => "no project is resolved — `--project <dir>` was not given \
                                      and no manifest is discovered from the file's path — so no \
                                      caller was looked for"
            .to_string(),
        ComponentScope::NoImporter => format!(
            "the resolved project was searched and no document in it `::use`s component \
             `{component}`"
        ),
    };
    let next = match scope {
        ComponentScope::NoProject => {
            "Re-run with `--project <dir>`, or run `lute check-project <dir>` — the deciding leg."
        }
        ComponentScope::NoImporter => {
            "The component is unused under this root, or its call sites live outside it. \
             `check-project` is the deciding leg."
        }
    };
    Diagnostic {
        code: W_COMPONENT_UNVERIFIED.to_string(),
        severity: Severity::Warning,
        message: format!(
            "{situation}. This verdict therefore covers only component `{component}`'s own \
             frontmatter and body against its OWN `uses:` — the one vocabulary that is discarded \
             at `::use` and never applies at runtime (dsl 0.9.0 §6.1). {next} (dsl 0.10.0 §9, D-W)"
        ),
        evidence: None,
        span: at,
        layer: Layer::Content,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    }
}
