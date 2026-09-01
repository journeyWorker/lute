//! Stage-state injection reducer + provenance (architecture.md §"Compiler —
//! stateful resolution (auto-injection)").
//!
//! Lowering a `.lute` node stream is **not** a pure 1:1 map: the compiler carries
//! scene state while it walks and *injects* the implicit commands the author
//! never wrote (auto-anchor a fresh entrance, `posReset` a dirty pose, pre-load a
//! sprite's first emotion, auto-hide lingering sprites on a scene change). The
//! arch doc frames this as a **deterministic compile-time GC** for stage
//! entities: the named rules are the collector and [`Provenance`] is the visible
//! free-list. An explicit value the author wrote always wins and is never
//! double-injected (0.10.0 §12.3, D-U).
//!
//! This module implements that as the arch doc prescribes:
//! 1. an explicit typed [`StageState`] threaded through — *one value passed
//!    through, not scattered loop-local sets*;
//! 2. lowering as a **pure reducer** — [`lower_node`] takes `state` by value and
//!    a read-only `node` + `lookahead` slice + the resolved `domains` vocabulary
//!    and returns `(state', emit)`. No globals, no I/O, deterministic — testable
//!    by feeding a node + state and asserting the emitted commands + the next
//!    state;
//! 3. the injection ruleset as **named, ordered, pure** functions, each
//!    unit-testable:
//!    - [`auto_anchor_on_show`] — a show/stage with no explicit anchor → inject
//!      an anchor (`by = "auto-anchor-on-show"`);
//!    - [`auto_pose_reset`] — a dirty (pose-changed) character speaking a plain,
//!      non-stateful line → inject a `posReset` (`by = "auto-pose-reset"`);
//!    - [`entry_emotion_lookahead`] — on entrance, look ahead for the character's
//!      first emotion and pre-load that sprite (`by = "entry-emotion-lookahead"`);
//!    - [`stage_bookkeeping`] — thread `on_stage`/`dirty`/`bg`/`music`, and
//!      auto-hide sprites left on stage across a scene change (`::bg`), the one
//!      implicit command this rule emits (`by = "stage-bookkeeping"`);
//! 4. [`Provenance`] `{ injected, by, explanation }` on every injected command.
//!
//! ## Implicit vocabulary reads are CHECKED reads
//! A rule that consults a domain's declared semantics when the corresponding
//! attribute is ABSENT owns the diagnostic for that domain being undeclared:
//! `check_directive` validates AUTHORED attrs only, so on the absent-attribute
//! path no other pass ever names the domain. `auto_anchor_on_show`'s read of
//! `anchor`'s `default:` is the one such read in this ruleset, and it reports
//! `E-DOMAIN-UNKNOWN` (see [`missing_anchor_domain_diag`]). Every other rule
//! either reads no vocabulary at all (`auto_pose_reset`, `entry_emotion_lookahead`,
//! `stage_bookkeeping`'s scene-change arm) or reads it only from an attribute the
//! author WROTE (`is_declared_exit` on `::auto`'s `action`), which the attribute
//! check already covers. A new rule of the first shape must diagnose, not
//! silently skip: skipping turns an undeclared slot into a behavior change.
//!
//! ## Data-vs-code boundary
//! The arch doc's ideal is *manifest-driven, code-executed*: the manifest's
//! per-directive `reads`/`writes`/`semantics` flags declare *which* directives
//! touch stage state, and the resolver algorithm stays code. `lute.core`'s
//! `::auto` already carries `["reads.onStage", "usesAnchor", "mayExitCharacter",
//! "writes.characterState"]` (see `assets/lute.core/directives/staging.yaml`).
//! At Task 4.8 the reducer hardcodes the *known* `lute.core` staging vocabulary
//! (`::auto` = entrance/exit/pose, `::bg` = scene change, `::line` emotion/pose
//! attrs) rather than reading those flags, because a stable, documented baseline
//! is more valuable here than a premature flag-driven dispatch. Swapping the
//! `is_*`/tag checks below for `semantics`-flag lookups is a mechanical follow-up
//! once the resolver consumes a `CapabilitySnapshot`.
//!
//! ## Diagnostic channel
//! The fixed reducer signature returns only `(StageState, Vec<InjectedCommand>)`,
//! so a [`Diagnostic`] rides on the threaded state's [`StageState::diags`]
//! accumulator — the pure-reducer analogue of a third return value. The four
//! semantic fields the contract lists (`on_stage`, `dirty`, `bg`, `music`) are
//! present verbatim; `diags` is the additive diagnostic channel the T4.9
//! `Resolved` view reads alongside the injections. It carries
//! `E-DOMAIN-UNKNOWN` from [`missing_anchor_domain_diag`] and, as of 0.10.0
//! §11.2, [`W_EXIT_INERT`] from [`exit_inert_diag`] and [`W_STAGE_ABSENT`] from
//! [`stage_absent_diag`].

use std::collections::{BTreeMap, BTreeSet};

use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_manifest::snapshot::Domain;
use lute_syntax::ast::{Attr, AttrValue, Directive, Line, Node};

/// Per-character stage entity: where the sprite stands and its current
/// pose/emotion. `Default` = an as-yet-unpositioned sprite.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SpriteState {
    /// Resolved anchor (`left|center|right`), explicit or injected.
    pub anchor: Option<String>,
    /// Current pose/action id (`pose-*`, `sway`, …); `Some` ⇒ potentially dirty.
    pub pose: Option<String>,
    /// Current emotion sprite (`delighted`, `neutral`, …).
    pub emotion: Option<String>,
}

/// Explicit, typed scene state threaded through the reducer — the arch doc's
/// "one value passed through, not scattered loop-local sets". Feeds the T4.9
/// `Resolved` view (`resolved.commands` + `resolved.injections`).
#[derive(Clone, Debug, Default)]
pub struct StageState {
    /// Characters currently on stage → their sprite state.
    pub on_stage: BTreeMap<String, SpriteState>,
    /// Characters whose pose changed and hasn't been reset yet.
    pub dirty: BTreeSet<String>,
    /// Characters removed by an **explicit declared exit** and not re-shown
    /// since (dsl 0.10.0 §11.2, **D-X**).
    ///
    /// `on_stage` cannot answer this on its own: a character who has never been
    /// shown and one who has left are both simply absent from it, and only the
    /// second is a staging impossibility. `W-STAGE-ABSENT` fires only for a
    /// member of this set, which is what keeps a character's FIRST line — an
    /// implicit entrance, and the overwhelmingly common shape — silent.
    ///
    /// Cleared per character on a re-show ([`stage_bookkeeping_show`]) and
    /// wholesale on a scene change ([`stage_bookkeeping_bg`], which clears the
    /// stage itself).
    pub exited: BTreeSet<String>,
    /// Current background (`::bg` location / assetId).
    pub bg: Option<String>,
    /// Current music (`::music` mood / action).
    pub music: Option<String>,
    /// Conflict diagnostics accumulated while folding (see module docs on the
    /// conflict channel). Not scene state proper — the reducer's diagnostic
    /// out-channel, since the fixed `lower_node` return can't carry a third slot.
    pub diags: Vec<Diagnostic>,
}

/// Provenance stamp on every injected command (arch doc §5): *which* named rule
/// inserted it and *why*. Surfaced in the resolved/injection view so injection
/// is visible, not silent magic.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct Provenance {
    /// Always `true`. The field is retained for IR compatibility, but with
    /// `W-INJECT-CONFLICT` removed in 0.10.0 (§12.3, D-AA) nothing constructs a
    /// `false`, and a consumer MUST NOT read a `true` as distinguishing
    /// anything. Removing it is an IR break, deferred to a future cycle.
    pub injected: bool,
    /// The named rule responsible (e.g. `"auto-anchor-on-show"`).
    pub by: String,
    /// Human-readable justification, surfaced in the LSP injection view.
    /// Named `explanation`, not `reason`: `end.reason` is an opaque author
    /// token a host dispatches on, and these two share no contract (#36, D-AE).
    pub explanation: String,
}

/// The concrete implicit command a rule injects.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub enum InjectKind {
    /// Position a freshly shown character at `anchor`.
    Anchor { character: String, anchor: String },
    /// Reset a dirty character's pose to neutral before a plain line.
    PosReset { character: String },
    /// Pre-load a character's entrance sprite at the emotion seen ahead.
    SpriteLoad { character: String, emotion: String },
    /// Auto-hide a character left on stage across a scene change.
    Hide { character: String },
}

/// One implicit command the resolver inserted, with its provenance.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct InjectedCommand {
    pub kind: InjectKind,
    pub provenance: Provenance,
}

/// Pure reducer core (arch doc §2): fold one `node` through the `state`, using
/// `lookahead` (the following sibling nodes) for entry-emotion resolution, and
/// return the next state + the commands injected for this node.
///
/// Deterministic and side-effect-free: same `(state, node, lookahead, domains)`
/// ⇒ same `(state', emit)`. The named rules run in the arch doc's order per node
/// kind. Nested nodes (`<branch>`/`<match>`/`<timeline>` bodies) are walked by
/// the caller (T4.9); this reducer resolves one flat node at a time.
///
/// `domains` is the resolved vocabulary (dsl 0.9.0 D-D): the member-level facts
/// this reducer needs — the `anchor` domain's `default:` and the `action`
/// domain's `exits:` — are DECLARED there, never compiled in.
pub fn lower_node(
    mut state: StageState,
    node: &Node,
    lookahead: &[Node],
    domains: &BTreeMap<String, Domain>,
) -> (StageState, Vec<InjectedCommand>) {
    let mut emit = Vec::new();
    match node {
        Node::Directive(d) if d.tag == "auto" => {
            lower_auto(&mut state, d, lookahead, &mut emit, domains)
        }
        Node::Directive(d) if d.tag == "bg" => stage_bookkeeping_bg(&mut state, d, &mut emit),
        Node::Directive(d) if d.tag == "music" => {
            // Bookkeeping only: no implicit command.
            state.music = attr_str(&d.attrs, "mood").or_else(|| attr_str(&d.attrs, "action"));
        }
        Node::Line(l) => lower_line(&mut state, l, lookahead, &mut emit, domains),
        // Other leaf directives (sfx/vfx/cut/video/camera) and Set/Branch/Match/
        // Timeline don't participate in stage-entity lifetime here.
        _ => {}
    }
    (state, emit)
}

/// Lower an `::auto` directive — the character entrance / exit / pose node.
/// Runs the show rules in arch-doc order: `auto-anchor-on-show`, then
/// `entry-emotion-lookahead`, then `stage-bookkeeping`.
fn lower_auto(
    state: &mut StageState,
    d: &Directive,
    lookahead: &[Node],
    emit: &mut Vec<InjectedCommand>,
    domains: &BTreeMap<String, Domain>,
) {
    let Some(character) = attr_str(&d.attrs, "character") else {
        return;
    };
    let action = attr_str(&d.attrs, "action");

    // Exit: the `::auto` IS the hide command — bookkeeping just frees the slot.
    if action
        .as_deref()
        .is_some_and(|a| is_declared_exit(a, domains))
    {
        // §11.2 position 2 (**D-X**): a declared exit for a character the
        // threaded state already records as gone. Only after an EXPLICIT
        // earlier exit — `exited`, never `!on_stage` — so a first-ever `::auto`
        // exit for a character nothing staged is not the finding and is silent.
        if state.exited.contains(&character) {
            state.diags.push(stage_absent_diag(
                &character,
                "another declared exit",
                d.span,
            ));
        }
        state.on_stage.remove(&character);
        state.dirty.remove(&character);
        state.exited.insert(character);
        return;
    }

    // Already on stage ⇒ reposition / pose change, not a fresh show.
    if state.on_stage.contains_key(&character) {
        let mut mark_dirty = false;
        if let Some(sp) = state.on_stage.get_mut(&character) {
            if let Some(a) = attr_str(&d.attrs, "anchor") {
                sp.anchor = Some(a);
            }
            if let Some(p) = &action {
                sp.pose = Some(p.clone());
                mark_dirty = true;
            }
        }
        if mark_dirty {
            state.dirty.insert(character);
        }
        return;
    }

    // Entrance / show.
    auto_anchor_on_show(state, d, &character, emit, domains);
    let emotion = entry_emotion_lookahead(&character, lookahead, emit);
    stage_bookkeeping_show(state, d, &character, emotion, domains);
}

/// Rule `auto-anchor-on-show`: a character shown with **no** explicit anchor
/// gets an injected anchor command, at the `anchor` domain's DECLARED
/// `default:`. An anchor the author DID write is honoured verbatim and nothing
/// is injected.
///
/// 0.10.0 §12.3 (**D-U**) removed `W-INJECT-CONFLICT` from the explicit arm.
/// Injection happens only in the no-attribute arm below, so "the author wrote
/// X and the rule would have injected Y ≠ X" cannot arise; the code's only
/// emission condition was the author writing a value EQUAL to the default —
/// agreement, never a conflict. `--deny-warnings` in CI could not express
/// "centre, on purpose" (`0.6.1 §6` refuses an `--allow`, and that refusal is
/// untouched), so the code was removed rather than narrowed.
///
/// **The information is dropped, not migrated** (**D-AA**). This was the only
/// record anywhere in the toolchain that an author wrote what a rule would have
/// injected. There is no `injected: false` provenance entry and none is built:
/// `lute-compile`'s `inject_cmd` turns every `InjectedCommand` into a `sprite`
/// record, so one would plant a spurious anchor command in the artifact beside
/// the author's own.
///
/// The no-attribute arm makes the `anchor` domain an **implicit dependency of
/// the `::auto` itself**, and therefore a CHECKED one: see
/// [`missing_anchor_domain_diag`].
fn auto_anchor_on_show(
    state: &mut StageState,
    d: &Directive,
    character: &str,
    emit: &mut Vec<InjectedCommand>,
    domains: &BTreeMap<String, Domain>,
) {
    // An AUTHORED `anchor` names the domain, so `check_domain_member` already
    // validated both its membership and the domain's existence. Nothing to do.
    if d.attrs.iter().any(|a| a.key == "anchor") {
        return;
    }
    match default_anchor(domains) {
        Some(default) => emit.push(InjectedCommand {
            kind: InjectKind::Anchor {
                character: character.to_string(),
                anchor: default.to_string(),
            },
            provenance: Provenance {
                injected: true,
                by: "auto-anchor-on-show".to_string(),
                explanation: format!(
                    "`{character}` shown without an explicit anchor; defaulting to `{default}`"
                ),
            },
        }),
        // No `anchor` domain at all: the implicit read has nothing to read,
        // and nobody else will say so (see `missing_anchor_domain_diag`).
        None if !domains.contains_key("anchor") => state
            .diags
            .push(missing_anchor_domain_diag(character, d.span)),
        // DECLARED but with no `default:` — already `E-ENUM-MISSING-SEMANTICS`
        // at the declaration (dsl 0.9.0 D-D makes `default:` mandatory for
        // the `anchor` slot), so this arm stays silent instead of piling a
        // second diagnostic onto one mistake.
        None => {}
    }
}

/// Rule `entry-emotion-lookahead`: on a character's entrance, scan the lookahead
/// slice for that character's first spoken line carrying an `emotion` attr and
/// pre-load the sprite at that emotion, so the entrance renders correctly.
/// Returns the emotion so bookkeeping can seed the sprite state.
fn entry_emotion_lookahead(
    character: &str,
    lookahead: &[Node],
    emit: &mut Vec<InjectedCommand>,
) -> Option<String> {
    let emotion = first_emotion_for(character, lookahead)?;
    emit.push(InjectedCommand {
        kind: InjectKind::SpriteLoad {
            character: character.to_string(),
            emotion: emotion.clone(),
        },
        provenance: Provenance {
            injected: true,
            by: "entry-emotion-lookahead".to_string(),
            explanation: format!(
                "pre-loading `{character}`'s first emotion `{emotion}` seen ahead of the entrance"
            ),
        },
    });
    Some(emotion)
}

/// Rule `auto-pose-reset`: a character marked `dirty` (pose changed) who speaks a
/// plain, non-stateful `:line` (no pose/emotion/variant override) gets an
/// injected `posReset` first, restoring the neutral pose; the dirty flag clears.
/// A stateful line instead applies its own sprite state and (re)marks dirty.
///
/// dsl 0.10.0 §11.2 (**D-X**): also the site of [`W_EXIT_INERT`]. The resolved
/// `action` domain is demonstrably in hand here — `content_line.rs` already
/// enumerates its members on this exact attribute for `E-BAD-ENUM` — and the
/// reducer already consults `exits:` one construct away, in [`lower_auto`]. The
/// missing piece was only ever a lookup.
fn lower_line(
    state: &mut StageState,
    line: &Line,
    lookahead: &[Node],
    emit: &mut Vec<InjectedCommand>,
    domains: &BTreeMap<String, Domain>,
) {
    let speaker = &line.speaker;
    let stateful = line_is_stateful(line);

    // §11.2: a content-line `action=` naming a declared exit member. The
    // attribute IS honoured as an action — the sprite's pose changes below —
    // and it does NOT remove the character from the stage; the artifact gets no
    // `exit` record. That gap is the whole finding. Silent when the author
    // already wrote the two-event form, which is remedy 1 (see
    // [`exit_is_written_next`]).
    if let Some(a) = line.attrs.iter().find(|a| a.key == "action") {
        if let Some(action) = attr_value_str(&a.value) {
            if is_declared_exit(&action, domains)
                && !exit_is_written_next(speaker, lookahead, domains)
            {
                state
                    .diags
                    .push(exit_inert_diag(speaker, &action, a.value_span));
            }
        }
    }

    // §11.2 position 1 (**D-X**): a spoken line whose speaker was removed by a
    // declared exit earlier in the walk, with no intervening show.
    if state.exited.contains(speaker) {
        state
            .diags
            .push(stage_absent_diag(speaker, "a spoken line", line.span));
    }

    if !stateful && state.dirty.contains(speaker) && state.on_stage.contains_key(speaker) {
        emit.push(InjectedCommand {
            kind: InjectKind::PosReset {
                character: speaker.clone(),
            },
            provenance: Provenance {
                injected: true,
                by: "auto-pose-reset".to_string(),
                explanation: format!(
                    "`{speaker}` had a dirty pose before a plain line; resetting to neutral"
                ),
            },
        });
        state.dirty.remove(speaker);
        if let Some(sp) = state.on_stage.get_mut(speaker) {
            sp.pose = None;
        }
    }

    if stateful {
        stage_bookkeeping_line(state, line);
    }
}

/// Rule `stage-bookkeeping` (scene-change arm): a `::bg` is a scene change, so
/// auto-hide every sprite left on stage — the one implicit command this rule
/// emits — then clear the stage and record the new background.
fn stage_bookkeeping_bg(state: &mut StageState, d: &Directive, emit: &mut Vec<InjectedCommand>) {
    for character in state.on_stage.keys().cloned().collect::<Vec<_>>() {
        emit.push(InjectedCommand {
            kind: InjectKind::Hide {
                character: character.clone(),
            },
            provenance: Provenance {
                injected: true,
                by: "stage-bookkeeping".to_string(),
                explanation: format!(
                    "auto-hiding `{character}` left on stage across a scene change"
                ),
            },
        });
    }
    state.on_stage.clear();
    state.dirty.clear();
    // A `::bg` is a scene change: every sprite is auto-hidden above, so no
    // earlier exit constrains what follows (§11.2).
    state.exited.clear();
    state.bg = attr_str(&d.attrs, "location").or_else(|| attr_str(&d.attrs, "assetId"));
}

/// Rule `stage-bookkeeping` (show arm): record the entering character on stage
/// with its resolved anchor (explicit, else the `anchor` domain's declared
/// `default:`, else none) and looked-ahead emotion. Pure state update — the
/// anchor/emotion *commands* were already emitted by their rules.
fn stage_bookkeeping_show(
    state: &mut StageState,
    d: &Directive,
    character: &str,
    emotion: Option<String>,
    domains: &BTreeMap<String, Domain>,
) {
    let anchor =
        attr_str(&d.attrs, "anchor").or_else(|| default_anchor(domains).map(str::to_string));
    // A re-show ends the absence §11.2 warns about.
    state.exited.remove(character);
    state.on_stage.insert(
        character.to_string(),
        SpriteState {
            anchor,
            pose: None,
            emotion,
        },
    );
}

/// Rule `stage-bookkeeping` (line arm): a stateful line updates the speaker's
/// sprite (emotion/pose) and marks them dirty, so a later plain line triggers
/// `auto-pose-reset`.
fn stage_bookkeeping_line(state: &mut StageState, line: &Line) {
    if let Some(sp) = state.on_stage.get_mut(&line.speaker) {
        if let Some(e) = attr_str(&line.attrs, "emotion") {
            sp.emotion = Some(e);
        }
        if let Some(p) = attr_str(&line.attrs, "action") {
            sp.pose = Some(p);
        }
    }
    state.dirty.insert(line.speaker.clone());
}

/// A line is *stateful* when it carries any sprite-affecting attribute; such a
/// line changes the sprite (so it won't trigger a reset) and marks the speaker
/// dirty.
fn line_is_stateful(line: &Line) -> bool {
    line.attrs.iter().any(|a| {
        matches!(
            a.key.as_str(),
            "emotion" | "variant" | "action" | "dialogMotion"
        )
    })
}

/// The `anchor` domain's declared default, `None` when the project declares no
/// `anchor` vocabulary at all or declares it without a `default:` (dsl 0.9.0
/// D-D). Two very different shapes, so callers must distinguish them: the
/// second is already `E-ENUM-MISSING-SEMANTICS` at the declaration, the first is
/// diagnosed only where the read happens (see [`missing_anchor_domain_diag`]).
fn default_anchor(domains: &BTreeMap<String, Domain>) -> Option<&str> {
    domains.get("anchor")?.default.as_deref()
}

/// Whether `action` is a declared exit member of the resolved `action` domain
/// (dsl 0.9.0 D-D) — replaces the `fade-out*`/`exit*`/`hide` prefix heuristic
/// that this crate and `lute-compile` each carried a hand-synced copy of.
/// Named for what it now asks (a lookup in declared data), not for the rule it
/// replaced.
///
/// `pub` so `lute-compile`'s lowerer asks the SAME function instead of
/// re-spelling the lookup: the point of moving this fact into the manifest is
/// that exactly one place reads it.
pub fn is_declared_exit(action: &str, domains: &BTreeMap<String, Domain>) -> bool {
    domains
        .get("action")
        .is_some_and(|d| d.exits.iter().any(|e| e == action))
}

/// First `emotion` attr on a spoken line by `character` in the lookahead slice.
fn first_emotion_for(character: &str, lookahead: &[Node]) -> Option<String> {
    lookahead.iter().find_map(|n| match n {
        Node::Line(l) if l.speaker == character => attr_str(&l.attrs, "emotion"),
        _ => None,
    })
}

/// Literal string value of an attribute by key (`@ref` → its raw CEL text).
fn attr_str(attrs: &[Attr], key: &str) -> Option<String> {
    attrs
        .iter()
        .find(|a| a.key == key)
        .and_then(|a| attr_value_str(&a.value))
}

/// Literal string of an [`AttrValue`]; a bare-`true` ident has no string form.
fn attr_value_str(value: &AttrValue) -> Option<String> {
    match value {
        AttrValue::Str(s) => Some(s.clone()),
        AttrValue::Ref(slot) => Some(slot.raw.clone()),
        AttrValue::BoolTrue => None,
    }
}

/// Whether the **two-event form** is written for `speaker` at this point:
/// scanning forward, the first node that concerns that character's stage
/// presence is an `::auto` whose `action` is a declared exit member.
///
/// This lookahead is not an optimisation. Spec §11.2 remedy 1 (**D-AD**) says
/// in as many words that writing the departure where it happens *discharges*
/// [`W_EXIT_INERT`] — so without this the message would name a remedy that does
/// not work, which is the exact defect §12.3 removes a code for. Anything else
/// the character does first (speaking again, being re-posed, or a `::bg` scene
/// change that auto-hides the whole stage) means the later exit is not this
/// line's departure, and the value on this line really is a pose.
fn exit_is_written_next(
    speaker: &str,
    lookahead: &[Node],
    domains: &BTreeMap<String, Domain>,
) -> bool {
    for node in lookahead {
        match node {
            Node::Directive(d) if d.tag == "bg" => return false,
            Node::Directive(d) if d.tag == "auto" => {
                if attr_str(&d.attrs, "character").as_deref() == Some(speaker) {
                    return attr_str(&d.attrs, "action")
                        .is_some_and(|a| is_declared_exit(&a, domains));
                }
            }
            Node::Line(l) if l.speaker == speaker => return false,
            _ => {}
        }
    }
    false
}

/// `W-EXIT-INERT`: a content-line `action=` whose value is a member of the
/// resolved `action` domain's `exits:` (dsl 0.10.0 §11.2, **D-X**).
pub const W_EXIT_INERT: &str = "W-EXIT-INERT";

/// Build the `W-EXIT-INERT` staging-layer warning.
///
/// **D-AD**: the message names BOTH remedies, in as many words. §12.3 removes
/// `W-INJECT-CONFLICT` in this same release for having no expressible remedy —
/// there is no `--allow` and no in-source acknowledgement (`0.6.1 §6`,
/// untouched) — so a warning added here must not repeat that defect. The
/// second remedy is also the argument for keeping the warning at all: if a
/// project declares `go-under` an exit, then in that project `go-under` MEANS
/// exit, and a pose sharing the name is ambiguous by construction. The
/// ambiguity lives in the vocabulary, which is where it is fixed.
fn exit_inert_diag(speaker: &str, action: &str, span: Span) -> Diagnostic {
    Diagnostic {
        code: W_EXIT_INERT.to_string(),
        severity: Severity::Warning,
        message: format!(
            "`{action}` is a declared exit of the `action` domain, but on a content line it is \
             honoured as an action and does NOT remove `{speaker}` from the stage — the artifact \
             gets no `exit` record. Either write the two-event form (keep this line, then \
             `::auto{{character=\"{speaker}\" action=\"{action}\"}}`), or, if `{action}` is a \
             pose rather than a departure, remove it from the `action` domain's `exits:` \
             (dsl 0.10.0 §11.2)"
        ),
        span,
        layer: Layer::Staging,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    }
}

/// `W-STAGE-ABSENT`: a staging event for a character the threaded stage state
/// records as off stage after an explicit declared exit (dsl 0.10.0 §11.2,
/// **D-X**).
pub const W_STAGE_ABSENT: &str = "W-STAGE-ABSENT";

/// Build the `W-STAGE-ABSENT` staging-layer warning. `what` names the event —
/// `"a spoken line"` or `"another declared exit"`.
///
/// **D-X** keeps this separate from [`W_EXIT_INERT`]: they are different
/// claims. One says an attribute does not do what it looks like; this one says
/// the staging is impossible. `--deny <CODE>` must be able to separate them.
fn stage_absent_diag(character: &str, what: &str, span: Span) -> Diagnostic {
    Diagnostic {
        code: W_STAGE_ABSENT.to_string(),
        severity: Severity::Warning,
        message: format!(
            "`{character}` left the stage on an earlier declared exit and has not been shown \
             again, so {what} here stages someone who is not present. Show them again with an \
             `::auto` before this point, or remove the earlier exit (dsl 0.10.0 §11.2)"
        ),
        span,
        layer: Layer::Staging,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    }
}

/// Build the `E-DOMAIN-UNKNOWN` error for `auto-anchor-on-show`'s IMPLICIT read
/// of the `anchor` domain (dsl 0.9.0 D-C/D-D), anchored at the `::auto` that
/// carries the dependency — the only span an author can act on, since no
/// attribute exists to point at.
///
/// Why the reducer reports this at all: `auto.anchor` is OPTIONAL, and
/// `check_directive`/`check_domain_member` only validate AUTHORED attrs. So on
/// the no-attribute path nothing else in the pipeline ever names `anchor`, and
/// an undeclared `anchor` slot used to sail through `check` while silently
/// dropping the default-anchor command 0.8.0 injected unconditionally — an
/// undeclared slot turning into a behavior change rather than an error, which is
/// the exact failure dsl 0.9.0 exists to prevent.
///
/// The code is reused, not minted: this IS "a domain slot is used but no source
/// declares the domain", the whole meaning of `E-DOMAIN-UNKNOWN` — merely used
/// implicitly rather than spelled out. A separate code would split one fact
/// across two names and need registering in the CLI's `DENIABLE_CODES`, where
/// `E-DOMAIN-UNKNOWN` already sits.
fn missing_anchor_domain_diag(character: &str, span: Span) -> Diagnostic {
    Diagnostic {
        code: "E-DOMAIN-UNKNOWN".to_string(),
        severity: Severity::Error,
        message: format!(
            "`{character}` is shown without an explicit `anchor`, so this `::auto` uses the \
             `anchor` domain's declared `default:` — but no source declares an `anchor` domain. \
             Declare it in an `enums:` block in this document's own frontmatter, in a project \
             schema reached through `uses:`, or in a plugin's `enums` export, or write an \
             explicit `anchor` here (dsl 0.9.0 D-D)"
        ),
        span,
        layer: Layer::Staging,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lute_core_span::{Layer, Severity, Span};
    use lute_syntax::ast::{Attr, AttrValue, Directive, Line, Node};

    fn span() -> Span {
        Span {
            byte_start: 0,
            byte_end: 0,
            line: 1,
            column: 1,
            utf16_range: (0, 0),
        }
    }

    fn attr(key: &str, val: &str) -> Attr {
        Attr {
            key: key.to_string(),
            value: AttrValue::Str(val.to_string()),
            value_span: span(),
            span: span(),
        }
    }

    fn auto(attrs: Vec<Attr>) -> Node {
        Node::Directive(Directive {
            tag: "auto".to_string(),
            attrs,
            when: None,
            span: span(),
        })
    }

    fn line(speaker: &str, attrs: Vec<Attr>) -> Node {
        Node::Line(Line {
            speaker: speaker.to_string(),
            attrs,
            when: None,
            text: "…".to_string(),
            text_span: span(),
            interps: Vec::new(),
            span: span(),
        })
    }

    // --- brief helpers ---
    fn show_bianca_no_anchor() -> Node {
        auto(vec![attr("character", "bianca")])
    }
    fn line_bianca() -> Node {
        line("bianca", vec![])
    }

    // --- rule 1: auto-anchor-on-show (brief) ---
    #[test]
    fn show_without_anchor_injects_anchor_with_provenance() {
        let doms = anchor_domain("center");
        let st = StageState::default();
        let (st2, injected) = lower_node(st, &show_bianca_no_anchor(), &[], &doms);
        assert!(injected
            .iter()
            .any(|c| c.provenance.by == "auto-anchor-on-show"));
        assert!(injected.iter().any(|c| c.provenance.injected
            && matches!(&c.kind, InjectKind::Anchor { anchor, .. } if anchor == "center")));
        assert!(st2.on_stage.contains_key("bianca"));
    }

    // --- rule 2: auto-pose-reset (brief) ---
    #[test]
    fn dirty_pose_before_nonstateful_line_injects_posreset() {
        let mut st = StageState::default();
        st.dirty.insert("bianca".into());
        st.on_stage.insert("bianca".into(), SpriteState::default());
        let (st2, injected) = lower_node(st, &line_bianca(), &[], &anchor_domain("center"));
        assert!(injected
            .iter()
            .any(|c| c.provenance.by == "auto-pose-reset"));
        assert!(!st2.dirty.contains("bianca"), "dirty flag should clear");
    }

    #[test]
    fn stateful_line_does_not_pose_reset_and_marks_dirty() {
        let mut st = StageState::default();
        st.on_stage.insert("bianca".into(), SpriteState::default());
        let (st2, injected) = lower_node(
            st,
            &line("bianca", vec![attr("emotion", "delighted")]),
            &[],
            &anchor_domain("center"),
        );
        assert!(!injected
            .iter()
            .any(|c| c.provenance.by == "auto-pose-reset"));
        assert!(st2.dirty.contains("bianca"), "a stateful line marks dirty");
        assert_eq!(st2.on_stage["bianca"].emotion.as_deref(), Some("delighted"));
    }

    /// `pose` is not a content-line attribute (`content_line.rs`'s
    /// KNOWN_ATTRS), so `@x{pose="…"}` is E-UNKNOWN-ATTR and the reducer could
    /// never observe one. The reads were unreachable; this pins that the
    /// stateful set is exactly the four real sprite-affecting slots.
    #[test]
    fn stateful_set_has_no_unreachable_attrs() {
        // `line()` yields a `Node`; `line_is_stateful` takes the inner `Line`.
        let spoken = |attrs: Vec<Attr>| match line("bianca", attrs) {
            Node::Line(l) => l,
            other => panic!("expected a Line, got {other:?}"),
        };
        for key in ["emotion", "variant", "action", "dialogMotion"] {
            assert!(
                line_is_stateful(&spoken(vec![attr(key, "x")])),
                "`{key}` must mark a line stateful"
            );
        }
        assert!(!line_is_stateful(&spoken(vec![attr("pose", "x")])));
    }

    // --- rule 3: entry-emotion-lookahead ---
    #[test]
    fn entry_emotion_lookahead_preloads_first_emotion() {
        let st = StageState::default();
        let look = [line("bianca", vec![attr("emotion", "delighted")])];
        let (st2, injected) = lower_node(
            st,
            &show_bianca_no_anchor(),
            &look,
            &anchor_domain("center"),
        );
        let load = injected
            .iter()
            .find(|c| c.provenance.by == "entry-emotion-lookahead")
            .expect("expected an emotion pre-load");
        assert!(
            matches!(&load.kind, InjectKind::SpriteLoad { emotion, .. } if emotion == "delighted")
        );
        assert!(load.provenance.injected);
        assert_eq!(st2.on_stage["bianca"].emotion.as_deref(), Some("delighted"));
    }

    // --- rule 4: stage-bookkeeping ---
    #[test]
    fn stage_bookkeeping_autohides_on_scene_change() {
        let mut st = StageState::default();
        st.on_stage.insert("bianca".into(), SpriteState::default());
        st.dirty.insert("bianca".into());
        let bg = Node::Directive(Directive {
            tag: "bg".to_string(),
            attrs: vec![attr("location", "cafe")],
            when: None,
            span: span(),
        });
        let (st2, injected) = lower_node(st, &bg, &[], &anchor_domain("center"));
        assert!(injected
            .iter()
            .any(|c| c.provenance.by == "stage-bookkeeping"
                && matches!(c.kind, InjectKind::Hide { .. })));
        assert!(st2.on_stage.is_empty(), "scene change clears the stage");
        assert!(st2.dirty.is_empty());
        assert_eq!(st2.bg.as_deref(), Some("cafe"));
    }

    // --- 0.10.0 §12.3 (D-U): an explicit anchor equal to the declared default
    // is SILENT. `auto-anchor-on-show` injects only in the no-attribute arm, so
    // "author wrote X, rule would inject Y ≠ X" cannot arise; the only shape the
    // removed `W-INJECT-CONFLICT` fired on was agreement. ---
    #[test]
    fn explicit_default_anchor_is_silent() {
        let doms = anchor_domain("center");
        let st = StageState::default();
        let show = auto(vec![attr("character", "bianca"), attr("anchor", "center")]);
        let (st2, emitted) = lower_node(st, &show, &[], &doms);
        assert!(
            !emitted
                .iter()
                .any(|c| c.provenance.by == "auto-anchor-on-show"),
            "an explicit anchor injects nothing; got {emitted:?}"
        );
        assert!(
            st2.diags.is_empty(),
            "an explicit anchor equal to the declared default is silent as of 0.10.0 \
             §12.3 (D-U); got {:?}",
            st2.diags.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
        assert_eq!(
            st2.on_stage.get("bianca").and_then(|s| s.anchor.as_deref()),
            Some("center"),
            "the character is still staged, at the author's anchor"
        );
    }

    #[test]
    fn explicit_override_anchor_is_silent() {
        // A *different* explicit anchor is a deliberate override: no injection,
        // no conflict.
        let st = StageState::default();
        let show = auto(vec![attr("character", "bianca"), attr("anchor", "left")]);
        let (st2, injected) = lower_node(st, &show, &[], &anchor_domain("center"));
        assert!(!injected
            .iter()
            .any(|c| c.provenance.by == "auto-anchor-on-show"));
        assert!(st2.diags.is_empty());
        assert_eq!(st2.on_stage["bianca"].anchor.as_deref(), Some("left"));
    }

    #[test]
    fn exit_action_frees_the_stage_slot() {
        let mut st = StageState::default();
        st.on_stage.insert("bianca".into(), SpriteState::default());
        st.dirty.insert("bianca".into());
        let exit = auto(vec![
            attr("character", "bianca"),
            attr("action", "fade-out-down"),
        ]);
        // The shipped test vocabulary declares `fade-out-down` an exit, which is
        // what this test always meant by it.
        let (st2, injected) = lower_node(st, &exit, &[], &lute_test_vocab::test_domains());
        assert!(injected.is_empty(), "the ::auto is itself the hide");
        assert!(!st2.on_stage.contains_key("bianca"));
        assert!(!st2.dirty.contains("bianca"));
    }

    #[test]
    fn reducer_is_pure_same_inputs_same_outputs() {
        let build = || {
            let mut st = StageState::default();
            st.on_stage.insert("bianca".into(), SpriteState::default());
            st.dirty.insert("bianca".into());
            st
        };
        let look = [line("bianca", vec![attr("emotion", "sad")])];
        let doms = anchor_domain("center");
        let (a_st, a_em) = lower_node(build(), &show_bianca_no_anchor(), &look, &doms);
        let (b_st, b_em) = lower_node(build(), &show_bianca_no_anchor(), &look, &doms);
        assert_eq!(a_em, b_em);
        assert_eq!(a_st.on_stage, b_st.on_stage);
        assert_eq!(a_st.dirty, b_st.dirty);
    }

    /// dsl 0.9.0 D-E: `exits:` must reproduce the deleted prefix heuristic's
    /// verdict on every member the shipped fixtures use, so the replacement is
    /// proven equivalent rather than assumed. Kept after deletion as the
    /// regression pin: the literal list below IS the old heuristic.
    #[test]
    fn declared_exits_match_the_former_heuristic() {
        fn former_heuristic(action: &str) -> bool {
            action.starts_with("fade-out") || action.starts_with("exit") || action == "hide"
        }
        let members = [
            "fade-in-up",
            "fade-in-slow",
            "slide-in-left",
            "walk-in",
            "idle",
            "wave",
            "sway",
            "lean",
            "pose-turn",
            "pose-lean",
            "fade-out",
            "fade-out-down",
            "fade-out-slow",
            "hide",
        ];
        let declared_exits = ["fade-out", "fade-out-down", "fade-out-slow", "hide"];
        for m in members {
            assert_eq!(
                declared_exits.contains(&m),
                former_heuristic(m),
                "`{m}`: declared exits disagree with the former heuristic"
            );
        }
        // The `exit*` arm of the heuristic matched nothing repo-wide, so no
        // `exit*` member exists to reproduce.
        assert!(!members.iter().any(|m| m.starts_with("exit")));
        // The literals above are the HISTORICAL rule, so they stay literal; tie
        // them to the vocabulary the fixtures actually declare, or the pin would
        // only compare a copy against itself.
        let action = &lute_test_vocab::test_domains()["action"];
        assert_eq!(action.members, members.map(str::to_string).to_vec());
        assert_eq!(action.exits, declared_exits.map(str::to_string).to_vec());
    }

    fn anchor_domain(default: &str) -> BTreeMap<String, Domain> {
        let mut d = BTreeMap::new();
        d.insert(
            "anchor".to_string(),
            Domain {
                members: vec!["left".into(), "middle".into(), "right".into()],
                open: false,
                default: Some(default.to_string()),
                exits: Vec::new(),
            },
        );
        d.insert(
            "action".to_string(),
            Domain {
                members: vec!["vanish".into(), "arrive".into()],
                open: false,
                default: None,
                exits: vec!["vanish".into()],
            },
        );
        d
    }

    /// The injected anchor is the DECLARED default, not a compiled-in `center`.
    #[test]
    fn injected_anchor_comes_from_the_domain() {
        let doms = anchor_domain("middle");
        let (st, injected) =
            lower_node(StageState::default(), &show_bianca_no_anchor(), &[], &doms);
        assert!(injected.iter().any(|c| c.provenance.injected
            && matches!(&c.kind, InjectKind::Anchor { anchor, .. } if anchor == "middle")));
        assert_eq!(st.on_stage["bianca"].anchor.as_deref(), Some("middle"));
    }

    /// Exit detection follows `exits:`, so a vocabulary that does not use the
    /// `fade-out*` convention still works.
    #[test]
    fn exit_follows_declared_exits() {
        let doms = anchor_domain("middle");
        let mut st = StageState::default();
        st.on_stage.insert("bianca".into(), SpriteState::default());
        let exit = auto(vec![attr("character", "bianca"), attr("action", "vanish")]);
        let (st2, _) = lower_node(st, &exit, &[], &doms);
        assert!(!st2.on_stage.contains_key("bianca"), "`vanish` must exit");
    }

    /// A DECLARED `anchor` that supplies no `default:` injects nothing: that
    /// shape is already `E-ENUM-MISSING-SEMANTICS` at the declaration (dsl
    /// 0.9.0 D-D makes `default:` mandatory for the `anchor` slot), so the
    /// reducer stays silent rather than reporting it a second time.
    #[test]
    fn declared_anchor_without_a_default_injects_nothing() {
        let mut doms = BTreeMap::new();
        doms.insert(
            "anchor".to_string(),
            Domain {
                members: vec!["left".into()],
                open: false,
                default: None,
                exits: Vec::new(),
            },
        );
        let (st, injected) =
            lower_node(StageState::default(), &show_bianca_no_anchor(), &[], &doms);
        assert!(!injected
            .iter()
            .any(|c| c.provenance.by == "auto-anchor-on-show"));
        assert!(st.diags.is_empty(), "got {:?}", st.diags);
        assert_eq!(st.on_stage["bianca"].anchor, None);
    }

    // --- the IMPLICIT `anchor` domain read (dsl 0.9.0 D-D) -------------------

    /// `action` declared in the 0.9.0 long form with `anchor` NOT declared —
    /// the project shape that silently dropped 0.8.0's default anchor.
    fn action_only_domain() -> BTreeMap<String, Domain> {
        let mut d = anchor_domain("center");
        d.remove("anchor");
        d
    }

    /// An `::auto` with no `anchor` attr READS the `anchor` domain's `default:`,
    /// so the domain is a dependency of the DIRECTIVE. Nothing authored the
    /// domain name, and directive validation walks AUTHORED attrs only — so
    /// unless the reducer reports it, 0.8.0's default-anchor command simply
    /// vanishes with zero diagnostics.
    #[test]
    fn implicit_anchor_read_without_a_declared_domain_errors() {
        let (st, injected) = lower_node(
            StageState::default(),
            &show_bianca_no_anchor(),
            &[],
            &action_only_domain(),
        );
        assert!(!injected
            .iter()
            .any(|c| c.provenance.by == "auto-anchor-on-show"));
        let unknown: Vec<_> = st
            .diags
            .iter()
            .filter(|d| d.code == "E-DOMAIN-UNKNOWN")
            .collect();
        assert_eq!(unknown.len(), 1, "one E-DOMAIN-UNKNOWN, got {:?}", st.diags);
        assert_eq!(unknown[0].severity, Severity::Error);
        assert_eq!(unknown[0].layer, Layer::Staging);
        assert_eq!(st.on_stage["bianca"].anchor, None);
    }

    /// The regression guard for the fix: the WORKING path still works. Same
    /// document, `anchor` declared ⇒ the declared default is injected and
    /// nothing is reported.
    #[test]
    fn implicit_anchor_read_with_a_declared_domain_stays_clean() {
        let (st, injected) = lower_node(
            StageState::default(),
            &show_bianca_no_anchor(),
            &[],
            &anchor_domain("center"),
        );
        assert!(injected.iter().any(|c| c.provenance.injected
            && matches!(&c.kind, InjectKind::Anchor { anchor, .. } if anchor == "center")));
        assert!(st.diags.is_empty(), "got {:?}", st.diags);
        assert_eq!(st.on_stage["bianca"].anchor.as_deref(), Some("center"));
    }

    /// An EXPLICIT `anchor` with no declared `anchor` domain is the ATTRIBUTE
    /// path's error (`check_domain_member`), never the reducer's. Pinned so the
    /// two paths keep reporting once each and never twice for one `::auto`.
    #[test]
    fn explicit_anchor_without_a_declared_domain_is_the_attributes_error() {
        let show = auto(vec![attr("character", "bianca"), attr("anchor", "center")]);
        let (st, injected) = lower_node(StageState::default(), &show, &[], &action_only_domain());
        assert!(!injected
            .iter()
            .any(|c| c.provenance.by == "auto-anchor-on-show"));
        assert!(
            st.diags.is_empty(),
            "the attribute check owns this one: {:?}",
            st.diags
        );
        assert_eq!(st.on_stage["bianca"].anchor.as_deref(), Some("center"));
    }

    // --- 0.10.0 §11.2 (D-X): a declared-exit member in content-line `action=` ---

    fn action_domain_with_exits(exits: &[&str]) -> BTreeMap<String, Domain> {
        let mut m = BTreeMap::new();
        m.insert(
            "action".to_string(),
            Domain {
                members: vec!["brace".into(), "drift".into(), "go-under".into()],
                open: false,
                default: None,
                exits: exits.iter().map(|s| (*s).to_string()).collect(),
            },
        );
        m
    }

    fn line_with_action(speaker: &str, action: &str) -> Node {
        line(speaker, vec![attr("action", action)])
    }

    fn staged(character: &str) -> StageState {
        let mut st = StageState::default();
        st.on_stage
            .insert(character.to_string(), SpriteState::default());
        st
    }

    /// The attribute is honoured as an action and does NOT remove the character
    /// from the stage. The two-event form is what does.
    #[test]
    fn content_line_exit_action_warns_inert() {
        let doms = action_domain_with_exits(&["go-under"]);
        let line = line_with_action("vesna", "go-under");
        let (st2, _emit) = lower_node(staged("vesna"), &line, &[], &doms);
        let d = st2
            .diags
            .iter()
            .find(|d| d.code == "W-EXIT-INERT")
            .unwrap_or_else(|| panic!("expected W-EXIT-INERT; got {:?}", st2.diags));
        assert_eq!(d.severity, Severity::Warning);
        assert_eq!(d.layer, Layer::Staging);
        assert!(
            st2.on_stage.contains_key("vesna"),
            "the attribute is honoured as an ACTION; it does not remove the character"
        );
    }

    /// **D-AD**: the message names BOTH remedies. A warning whose remedy exists
    /// but is undocumented is `W-UNPROVEN-RELATIONAL` again by another route,
    /// and §12.3 removes a code in this same release for exactly that defect.
    #[test]
    fn exit_inert_message_names_both_remedies() {
        let doms = action_domain_with_exits(&["go-under"]);
        let line = line_with_action("vesna", "go-under");
        let (st2, _emit) = lower_node(staged("vesna"), &line, &[], &doms);
        let m = &st2
            .diags
            .iter()
            .find(|d| d.code == "W-EXIT-INERT")
            .unwrap()
            .message;
        assert!(
            m.contains("::auto{"),
            "remedy 1, the two-event form, must be written out; got {m}"
        );
        assert!(
            m.contains("exits:"),
            "remedy 2, stop declaring that member an exit, must be named; got {m}"
        );
    }

    /// An ordinary (non-exit) action member on a content line is silent — that
    /// is the overwhelmingly common case and it is not the finding.
    #[test]
    fn content_line_non_exit_action_is_silent() {
        let doms = action_domain_with_exits(&["go-under"]);
        let line = line_with_action("vesna", "brace");
        let (st2, _emit) = lower_node(staged("vesna"), &line, &[], &doms);
        assert!(
            !st2.diags.iter().any(|d| d.code == "W-EXIT-INERT"),
            "`brace` is not in `exits:`; got {:?}",
            st2.diags
        );
    }

    /// A project that declares no `exits:` at all cannot produce this warning.
    #[test]
    fn no_declared_exits_means_no_warning() {
        let doms = action_domain_with_exits(&[]);
        let line = line_with_action("vesna", "go-under");
        let (st2, _emit) = lower_node(staged("vesna"), &line, &[], &doms);
        assert!(st2.diags.is_empty(), "got {:?}", st2.diags);
    }

    /// **D-AD remedy 1**, which spec §11.2 says *discharges* the warning: keep
    /// the line and follow it with the `::auto` that actually leaves. The
    /// message names this remedy, so the remedy must work.
    #[test]
    fn exit_inert_discharged_by_the_two_event_form() {
        let doms = action_domain_with_exits(&["go-under"]);
        let look = [auto(vec![
            attr("character", "vesna"),
            attr("action", "go-under"),
        ])];
        let (st2, _emit) = lower_node(
            staged("vesna"),
            &line_with_action("vesna", "go-under"),
            &look,
            &doms,
        );
        assert!(
            st2.diags.is_empty(),
            "the two-event form discharges it (§11.2 remedy 1); got {:?}",
            st2.diags
        );
    }

    /// The pose case is still the finding: the character keeps speaking, so the
    /// exit written later is not this line's departure.
    #[test]
    fn exit_inert_still_fires_when_the_speaker_carries_on() {
        let doms = action_domain_with_exits(&["go-under"]);
        let look = [
            line("vesna", vec![]),
            auto(vec![attr("character", "vesna"), attr("action", "go-under")]),
        ];
        let (st2, _emit) = lower_node(
            staged("vesna"),
            &line_with_action("vesna", "go-under"),
            &look,
            &doms,
        );
        assert!(
            st2.diags.iter().any(|d| d.code == "W-EXIT-INERT"),
            "`go-under` here is a pose, not a departure; got {:?}",
            st2.diags
        );
    }

    // --- 0.10.0 §11.2 (D-X): a staging event for a character the threaded
    // stage state records as OFF stage, after an explicit declared exit. ---

    fn auto_with_action(character: &str, action: &str) -> Node {
        auto(vec![attr("character", character), attr("action", action)])
    }

    fn plain_line(speaker: &str) -> Node {
        line(speaker, vec![])
    }

    /// Position 1: a spoken content line whose speaker was removed by a declared
    /// exit earlier in the walk, with no intervening show.
    #[test]
    fn line_after_a_declared_exit_warns_absent() {
        let doms = action_domain_with_exits(&["go-under"]);
        let (st, _) = lower_node(
            staged("vesna"),
            &auto_with_action("vesna", "go-under"),
            &[],
            &doms,
        );
        assert!(!st.on_stage.contains_key("vesna"), "the exit removed them");
        let (st2, _) = lower_node(st, &plain_line("vesna"), &[], &doms);
        assert!(
            st2.diags.iter().any(|d| d.code == "W-STAGE-ABSENT"),
            "a line after a declared exit is impossible staging; got {:?}",
            st2.diags
        );
    }

    /// Position 2: an `::auto` whose `action` is a declared exit member, for a
    /// character already off stage — the double exit T2.4 measured.
    #[test]
    fn second_declared_exit_warns_absent() {
        let doms = action_domain_with_exits(&["go-under"]);
        let (st, _) = lower_node(
            staged("vesna"),
            &auto_with_action("vesna", "go-under"),
            &[],
            &doms,
        );
        let (st2, _) = lower_node(st, &auto_with_action("vesna", "go-under"), &[], &doms);
        assert!(
            st2.diags.iter().any(|d| d.code == "W-STAGE-ABSENT"),
            "two exits with nothing between them; got {:?}",
            st2.diags
        );
    }

    /// **D-X's restriction, and the reason `exited` exists.** A character who
    /// has simply not been shown yet is put on stage by their first line, as
    /// today. That is not the finding and MUST NOT warn — `on_stage` alone
    /// cannot tell the two absences apart.
    #[test]
    fn a_never_shown_character_speaking_is_silent() {
        let doms = action_domain_with_exits(&["go-under"]);
        let (st2, _) = lower_node(StageState::default(), &plain_line("vesna"), &[], &doms);
        assert!(
            st2.diags.is_empty(),
            "a first line is an implicit entrance, not impossible staging; got {:?}",
            st2.diags
        );
    }

    /// The same restriction on the `::auto` half: a first-ever declared exit for
    /// a character nothing ever staged is absent-but-never-departed, so it is
    /// silent too. Only a SECOND exit is impossible.
    #[test]
    fn a_first_declared_exit_for_a_never_shown_character_is_silent() {
        let doms = action_domain_with_exits(&["go-under"]);
        let (st2, _) = lower_node(
            StageState::default(),
            &auto_with_action("vesna", "go-under"),
            &[],
            &doms,
        );
        assert!(
            !st2.diags.iter().any(|d| d.code == "W-STAGE-ABSENT"),
            "never shown is not the same as departed; got {:?}",
            st2.diags
        );
    }

    /// A re-show clears it: exit, show again, speak — silent.
    #[test]
    fn a_re_show_clears_the_exited_mark() {
        let doms = action_domain_with_exits(&["go-under"]);
        let (st, _) = lower_node(
            staged("vesna"),
            &auto_with_action("vesna", "go-under"),
            &[],
            &doms,
        );
        let (st, _) = lower_node(st, &auto_with_action("vesna", "brace"), &[], &doms);
        let (st2, _) = lower_node(st, &plain_line("vesna"), &[], &doms);
        assert!(
            !st2.diags.iter().any(|d| d.code == "W-STAGE-ABSENT"),
            "the character is back on stage; got {:?}",
            st2.diags
        );
    }

    /// A `::bg` is a scene change: the stage is cleared and every exit mark goes
    /// with it, so a line after the scene change is a fresh implicit entrance.
    #[test]
    fn a_scene_change_clears_the_exited_mark() {
        let doms = action_domain_with_exits(&["go-under"]);
        let (st, _) = lower_node(
            staged("vesna"),
            &auto_with_action("vesna", "go-under"),
            &[],
            &doms,
        );
        let bg = Node::Directive(Directive {
            tag: "bg".to_string(),
            attrs: vec![attr("location", "hold")],
            when: None,
            span: span(),
        });
        let (st, _) = lower_node(st, &bg, &[], &doms);
        let (st2, _) = lower_node(st, &plain_line("vesna"), &[], &doms);
        assert!(
            !st2.diags.iter().any(|d| d.code == "W-STAGE-ABSENT"),
            "a scene change resets the stage; got {:?}",
            st2.diags
        );
    }
}
