//! Stage-state injection reducer + provenance (architecture.md §"Compiler —
//! stateful resolution (auto-injection)").
//!
//! Lowering a `.lute` node stream is **not** a pure 1:1 map: the compiler carries
//! scene state while it walks and *injects* the implicit commands the author
//! never wrote (auto-anchor a fresh entrance, `posReset` a dirty pose, pre-load a
//! sprite's first emotion, auto-hide lingering sprites on a scene change). The
//! arch doc frames this as a **deterministic compile-time GC** for stage
//! entities: the named rules are the collector, [`Provenance`] is the visible
//! free-list, and conflicts (author-written vs would-be-injected) surface as
//! warnings instead of silent double-injection.
//!
//! This module implements that as the arch doc prescribes:
//! 1. an explicit typed [`StageState`] threaded through — *one value passed
//!    through, not scattered loop-local sets*;
//! 2. lowering as a **pure reducer** — [`lower_node`] takes `state` by value and
//!    a read-only `node` + `lookahead` slice and returns `(state', emit)`. No
//!    globals, no I/O, deterministic — testable by feeding a node + state and
//!    asserting the emitted commands + the next state;
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
//! 4. [`Provenance`] `{ injected, by, reason }` on every injected command.
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
//! ## Conflict channel
//! The fixed reducer signature returns only `(StageState, Vec<InjectedCommand>)`,
//! so the `W-INJECT-CONFLICT` [`Diagnostic`] rides on the threaded state's
//! [`StageState::diags`] accumulator — the pure-reducer analogue of a third
//! return value. The four semantic fields the contract lists (`on_stage`,
//! `dirty`, `bg`, `music`) are present verbatim; `diags` is the additive
//! diagnostic channel the T4.9 `Resolved` view reads alongside the injections.

use std::collections::{BTreeMap, BTreeSet};

use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_syntax::ast::{Attr, AttrValue, Directive, Line, Node};

/// Default anchor the [`auto_anchor_on_show`] rule injects when a character is
/// shown without one (dsl Appendix A: `anchor = left|center|right`).
pub const DEFAULT_ANCHOR: &str = "center";

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
/// is visible, not silent magic. `injected == false` marks a command the author
/// wrote that a rule *would* have injected (a conflict).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct Provenance {
    /// `true` when the compiler inserted this command.
    pub injected: bool,
    /// The named rule responsible (e.g. `"auto-anchor-on-show"`).
    pub by: String,
    /// Human-readable justification, surfaced in the LSP injection view.
    pub reason: String,
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
/// Deterministic and side-effect-free: same `(state, node, lookahead)` ⇒ same
/// `(state', emit)`. The named rules run in the arch doc's order per node kind.
/// Nested nodes (`<branch>`/`<match>`/`<timeline>` bodies) are walked by the
/// caller (T4.9); this reducer resolves one flat node at a time.
pub fn lower_node(
    mut state: StageState,
    node: &Node,
    lookahead: &[Node],
) -> (StageState, Vec<InjectedCommand>) {
    let mut emit = Vec::new();
    match node {
        Node::Directive(d) if d.tag == "auto" => lower_auto(&mut state, d, lookahead, &mut emit),
        Node::Directive(d) if d.tag == "bg" => stage_bookkeeping_bg(&mut state, d, &mut emit),
        Node::Directive(d) if d.tag == "music" => {
            // Bookkeeping only: no implicit command.
            state.music = attr_str(&d.attrs, "mood").or_else(|| attr_str(&d.attrs, "action"));
        }
        Node::Line(l) => lower_line(&mut state, l, &mut emit),
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
) {
    let Some(character) = attr_str(&d.attrs, "character") else {
        return;
    };
    let action = attr_str(&d.attrs, "action");

    // Exit: the `::auto` IS the hide command — bookkeeping just frees the slot.
    if action.as_deref().map(is_exit_action).unwrap_or(false) {
        state.on_stage.remove(&character);
        state.dirty.remove(&character);
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
    auto_anchor_on_show(state, d, &character, emit);
    let emotion = entry_emotion_lookahead(&character, lookahead, emit);
    stage_bookkeeping_show(state, d, &character, emotion);
}

/// Rule `auto-anchor-on-show`: a character shown with **no** explicit anchor
/// gets an injected anchor command (defaulting to [`DEFAULT_ANCHOR`]). If the
/// author wrote an anchor equal to what this rule would inject, that is the
/// author-written-vs-would-inject case → `W-INJECT-CONFLICT` (warn, no double
/// injection). A *different* explicit anchor is a deliberate override, honored
/// silently.
fn auto_anchor_on_show(
    state: &mut StageState,
    d: &Directive,
    character: &str,
    emit: &mut Vec<InjectedCommand>,
) {
    match d.attrs.iter().find(|a| a.key == "anchor") {
        None => emit.push(InjectedCommand {
            kind: InjectKind::Anchor {
                character: character.to_string(),
                anchor: DEFAULT_ANCHOR.to_string(),
            },
            provenance: Provenance {
                injected: true,
                by: "auto-anchor-on-show".to_string(),
                reason: format!(
                    "`{character}` shown without an explicit anchor; defaulting to `{DEFAULT_ANCHOR}`"
                ),
            },
        }),
        Some(anchor_attr) => {
            if attr_value_str(&anchor_attr.value).as_deref() == Some(DEFAULT_ANCHOR) {
                state.diags.push(conflict_diag(
                    format!(
                        "`{character}` is shown with an explicit `anchor=\"{DEFAULT_ANCHOR}\"` that \
                         `auto-anchor-on-show` would otherwise inject"
                    ),
                    anchor_attr.value_span,
                ));
            }
        }
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
            reason: format!(
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
fn lower_line(state: &mut StageState, line: &Line, emit: &mut Vec<InjectedCommand>) {
    let speaker = &line.speaker;
    let stateful = line_is_stateful(line);

    if !stateful && state.dirty.contains(speaker) && state.on_stage.contains_key(speaker) {
        emit.push(InjectedCommand {
            kind: InjectKind::PosReset {
                character: speaker.clone(),
            },
            provenance: Provenance {
                injected: true,
                by: "auto-pose-reset".to_string(),
                reason: format!(
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
                reason: format!("auto-hiding `{character}` left on stage across a scene change"),
            },
        });
    }
    state.on_stage.clear();
    state.dirty.clear();
    state.bg = attr_str(&d.attrs, "location").or_else(|| attr_str(&d.attrs, "assetId"));
}

/// Rule `stage-bookkeeping` (show arm): record the entering character on stage
/// with its resolved anchor (explicit or [`DEFAULT_ANCHOR`]) and looked-ahead
/// emotion. Pure state update — the anchor/emotion *commands* were already
/// emitted by their rules.
fn stage_bookkeeping_show(
    state: &mut StageState,
    d: &Directive,
    character: &str,
    emotion: Option<String>,
) {
    let anchor = attr_str(&d.attrs, "anchor").unwrap_or_else(|| DEFAULT_ANCHOR.to_string());
    state.on_stage.insert(
        character.to_string(),
        SpriteState {
            anchor: Some(anchor),
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
        if let Some(p) = attr_str(&line.attrs, "action").or_else(|| attr_str(&line.attrs, "pose")) {
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
            "emotion" | "variant" | "action" | "pose" | "dialogMotion"
        )
    })
}

/// The `::auto` action ids that exit a character (dsl Appendix A: `fade-out-*`).
fn is_exit_action(action: &str) -> bool {
    action.starts_with("fade-out") || action.starts_with("exit") || action == "hide"
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

/// Build the `W-INJECT-CONFLICT` staging-layer warning.
fn conflict_diag(message: String, span: Span) -> Diagnostic {
    Diagnostic {
        code: "W-INJECT-CONFLICT".to_string(),
        severity: Severity::Warning,
        message,
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
        let st = StageState::default();
        let (st2, injected) = lower_node(st, &show_bianca_no_anchor(), &[]);
        assert!(injected
            .iter()
            .any(|c| c.provenance.by == "auto-anchor-on-show"));
        assert!(injected.iter().any(|c| c.provenance.injected
            && matches!(&c.kind, InjectKind::Anchor { anchor, .. } if anchor == DEFAULT_ANCHOR)));
        assert!(st2.on_stage.contains_key("bianca"));
    }

    // --- rule 2: auto-pose-reset (brief) ---
    #[test]
    fn dirty_pose_before_nonstateful_line_injects_posreset() {
        let mut st = StageState::default();
        st.dirty.insert("bianca".into());
        st.on_stage.insert("bianca".into(), SpriteState::default());
        let (st2, injected) = lower_node(st, &line_bianca(), &[]);
        assert!(injected
            .iter()
            .any(|c| c.provenance.by == "auto-pose-reset"));
        assert!(!st2.dirty.contains("bianca"), "dirty flag should clear");
    }

    #[test]
    fn stateful_line_does_not_pose_reset_and_marks_dirty() {
        let mut st = StageState::default();
        st.on_stage.insert("bianca".into(), SpriteState::default());
        let (st2, injected) =
            lower_node(st, &line("bianca", vec![attr("emotion", "delighted")]), &[]);
        assert!(!injected
            .iter()
            .any(|c| c.provenance.by == "auto-pose-reset"));
        assert!(st2.dirty.contains("bianca"), "a stateful line marks dirty");
        assert_eq!(st2.on_stage["bianca"].emotion.as_deref(), Some("delighted"));
    }

    // --- rule 3: entry-emotion-lookahead ---
    #[test]
    fn entry_emotion_lookahead_preloads_first_emotion() {
        let st = StageState::default();
        let look = [line("bianca", vec![attr("emotion", "delighted")])];
        let (st2, injected) = lower_node(st, &show_bianca_no_anchor(), &look);
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
            span: span(),
        });
        let (st2, injected) = lower_node(st, &bg, &[]);
        assert!(injected
            .iter()
            .any(|c| c.provenance.by == "stage-bookkeeping"
                && matches!(c.kind, InjectKind::Hide { .. })));
        assert!(st2.on_stage.is_empty(), "scene change clears the stage");
        assert!(st2.dirty.is_empty());
        assert_eq!(st2.bg.as_deref(), Some("cafe"));
    }

    // --- W-INJECT-CONFLICT: author wrote the anchor the rule would inject ---
    #[test]
    fn explicit_default_anchor_warns_inject_conflict() {
        let st = StageState::default();
        let show = auto(vec![
            attr("character", "bianca"),
            attr("anchor", DEFAULT_ANCHOR),
        ]);
        let (st2, injected) = lower_node(st, &show, &[]);
        // Author wrote what the rule would inject → warn, don't double-inject.
        assert!(!injected
            .iter()
            .any(|c| c.provenance.by == "auto-anchor-on-show"));
        assert!(st2.diags.iter().any(|d| d.code == "W-INJECT-CONFLICT"
            && d.severity == Severity::Warning
            && d.layer == Layer::Staging));
        // The character is still staged, at the author's anchor.
        assert_eq!(
            st2.on_stage["bianca"].anchor.as_deref(),
            Some(DEFAULT_ANCHOR)
        );
    }

    #[test]
    fn explicit_override_anchor_is_silent() {
        // A *different* explicit anchor is a deliberate override: no injection,
        // no conflict.
        let st = StageState::default();
        let show = auto(vec![attr("character", "bianca"), attr("anchor", "left")]);
        let (st2, injected) = lower_node(st, &show, &[]);
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
        let (st2, injected) = lower_node(st, &exit, &[]);
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
        let (a_st, a_em) = lower_node(build(), &show_bianca_no_anchor(), &look);
        let (b_st, b_em) = lower_node(build(), &show_bianca_no_anchor(), &look);
        assert_eq!(a_em, b_em);
        assert_eq!(a_st.on_stage, b_st.on_stage);
        assert_eq!(a_st.dirty, b_st.dirty);
    }
}
