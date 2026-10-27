//! Pass-1 direct lowering (§5): each primitive node → its typed record,
//! schema-driven and pure. `addr`/`lineId`/`voiceKey` stay empty here — the
//! addressing pass (Task 11) owns identity; the stage walker (Tasks 8–9)
//! owns order, stamps, and injection.

use std::collections::BTreeMap;

use lute_manifest::schema::{
    DirectiveDecl, Lowering, OpBy, WriteDecl, WriteValue, DIRECTIVE_TEXT_CODE_ATTR,
};
use lute_manifest::snapshot::{CapabilitySnapshot, Domain};
use lute_manifest::stamps::StampSurface;
use lute_manifest::types::{Literal, PathSegment, Type};
use lute_syntax::ast::{Assert, Attr, AttrValue, Directive, InlineNode, Line, Retract, Set};

use crate::ir::{
    placeholder_from_interp, AcceptCmd, ActorCmd, AssertCmd, BgCmd, CameraCmd, CelPairExt, CgCmd,
    Command, Effect, EffectSource, EndCmd, FactRecord, JumpCmd, LineCmd, LocalizedText, MusicCmd,
    PluginCmd, RaiseRecord, RetractCmd, Role, Segment, SequenceCmd, SetCmd, SfxCmd, Stamp,
    TextIdentity, Timing, VfxCmd, VideoCmd,
};
use crate::normalize::{COMPONENT_BEGIN, COMPONENT_END};
use lute_ir::CelPair;

/// Bare-ident delivery flag (dsl 0.2.2 §D7: `mono`/`os`/`vo`, `AttrValue::
/// BoolTrue` by grammar convention — the checker (`content_line.rs`) gates
/// at-most-one before this ever runs, so priority among the three is moot
/// on a checked document; still deterministic when called directly (unit
/// tests, or an author-mode document with the conflict warning suppressed).
fn has_delivery_flag(attrs: &[Attr], key: &str) -> bool {
    attrs
        .iter()
        .any(|a| a.key == key && matches!(a.value, AttrValue::BoolTrue))
}

pub fn lower_line(line: &Line, snapshot: &CapabilitySnapshot) -> Command {
    let get = |k: &str| attr_string(&line.attrs, k);
    let role = if line.speaker == "narrator" {
        Role::Narration
    } else if has_delivery_flag(&line.attrs, "mono") {
        Role::Mono
    } else if has_delivery_flag(&line.attrs, "vo") {
        Role::Vo
    } else if has_delivery_flag(&line.attrs, "os") {
        Role::Os
    } else {
        Role::Dialogue
    };
    Command::Line(LineCmd {
        position: String::new(),
        role,
        speaker: line.speaker.clone(),
        text: line.plain_text(),
        emotion: get("emotion"),
        variant: get("variant").and_then(|v| v.parse::<i64>().ok()),
        action: get("action"),
        dialog_motion: get("dialogMotion"),
        as_label: get("as"),
        line_id: String::new(),
        voice_key: String::new(),
        placeholders: line.interps.iter().map(placeholder_from_interp).collect(),
        segments: inline_segments(&line.inline),
        texts: Default::default(),
        locale_segments: Default::default(),
        modifiers: line.modifier_multiset(),
        // An untagged component line carries its source-order back-fill
        // (`normalize::backfill_component_codes`); an authored code wins.
        code: get("code").or_else(|| get(crate::normalize::COMPONENT_CODE_ATTR)),
        stamp: Stamp {
            // 0.49.0 §4.2: an authored `wait` only; absent stays omitted (a
            // line always waits for its own advance).
            timing: Timing {
                wait: attr_bool(&line.attrs, "wait"),
                ..Timing::default()
            },
            // plugin §14.1: a content line carries the stamps admissible on
            // it like any other record. The built-in content-line keys win
            // (they ARE the record's own fields above) — the same precedence
            // the checker applies in `content_line::check_content_line_attrs`.
            stamps: plugin_stamps(&line.attrs, snapshot, Some(&StampSurface::Line), |k| {
                lute_manifest::core::LINE_ATTRS.contains(&k)
            }),
            ..Stamp::default()
        },
    })
}

/// A line's presentation runs (dsl 0.37.0 §3.6): empty unless `nodes` carry
/// an inline modifier. Text (and `{{…}}` markers, verbatim) coalesces while
/// the active styles (outermost first) and innermost `speed` rate stay the
/// same; `pause{s}` is a leaf of its own; empty text never appears. Each
/// run's text is in the IR encoding of `{{` (lute 0.47.1 §3,
/// [`lute_syntax::line_text::IrText`]), on its own.
pub(crate) fn inline_segments(nodes: &[InlineNode]) -> Vec<Segment> {
    use lute_syntax::line_text::IrText;
    enum Run {
        Text {
            text: IrText,
            styles: Vec<String>,
            rate: Option<f64>,
        },
        Pause(f64),
    }
    fn walk(nodes: &[InlineNode], styles: &mut Vec<String>, rate: Option<f64>, out: &mut Vec<Run>) {
        for node in nodes {
            match node {
                InlineNode::Text { text, .. } if text.is_empty() => {}
                InlineNode::Text { text, .. } => run(out, styles, rate).push_literal(text),
                InlineNode::Interpolation(i) => {
                    run(out, styles, rate).push_marker(&format!("{{{{{}}}}}", i.source));
                }
                InlineNode::Modifier(m) => {
                    let attr = |k: &str| {
                        m.attrs
                            .iter()
                            .find(|a| a.key == k)
                            .and_then(|a| a.value.parse::<f64>().ok())
                    };
                    match m.name.as_str() {
                        "pause" => out.push(Run::Pause(attr("s").unwrap_or(0.0))),
                        "speed" => walk(&m.children, styles, attr("rate").or(rate), out),
                        style => {
                            styles.push(style.to_string());
                            walk(&m.children, styles, rate, out);
                            styles.pop();
                        }
                    }
                }
            }
        }
    }
    /// The text run to append to: the last one when its styles and rate
    /// match, else a new one.
    fn run<'a>(out: &'a mut Vec<Run>, styles: &[String], rate: Option<f64>) -> &'a mut IrText {
        let same = matches!(
            out.last(),
            Some(Run::Text { styles: s, rate: r, .. }) if s.as_slice() == styles && *r == rate
        );
        if !same {
            out.push(Run::Text {
                text: IrText::new(),
                styles: styles.to_vec(),
                rate,
            });
        }
        match out.last_mut() {
            Some(Run::Text { text, .. }) => text,
            _ => unreachable!("a text run was just ensured"),
        }
    }
    if !nodes.iter().any(|n| matches!(n, InlineNode::Modifier(_))) {
        return Vec::new();
    }
    let mut runs = Vec::new();
    walk(nodes, &mut Vec::new(), None, &mut runs);
    runs.into_iter()
        .map(|run| match run {
            Run::Text { text, styles, rate } => Segment::Text {
                text: text.finish(),
                styles,
                rate,
            },
            Run::Pause(pause) => Segment::Pause { pause },
        })
        .collect()
}

pub fn lower_set(set: &Set) -> Command {
    Command::Set(SetCmd {
        position: String::new(),
        path: set.path.clone(),
        op: set.op.clone(),
        value: CelPair::from_slot(&set.expr),
        stamp: Stamp::default(),
    })
}

/// A [`FactTerm`] as its ground string (dsl 0.3.0 §5): `Ident` verbatim,
/// `Bool` as `"true"`/`"false"`, `Wildcard` as `"_"` (retract-pattern-only —
/// never emitted from an `::assert`, checker-enforced `E-RETRACT-WILDCARD-
/// ASSERT`). dsl 0.28.0 §3: `occasion.target` as written — the engine binds
/// the member the beat runs for when it applies the write.
fn fact_term_string(t: &lute_manifest::fact::FactTerm) -> String {
    use lute_manifest::fact::FactTerm;
    match t {
        FactTerm::Ident(s) => s.clone(),
        FactTerm::Bool(b) => b.to_string(),
        FactTerm::Wildcard => "_".to_string(),
        FactTerm::Param(p) => format!("@{p}"),
        FactTerm::Target => lute_manifest::semantics::beats::OCCASION_TARGET.to_string(),
    }
}

/// Resolved fact patterns as the IR's `{ relation, args }` records (a
/// plugin call's or a reward's declared `retracts` / `asserts`). A pattern
/// whose `@attr` stayed unbound is not written.
pub(crate) fn fact_records(patterns: &[lute_manifest::fact::FactPattern]) -> Vec<FactRecord> {
    patterns
        .iter()
        .filter(|p| {
            !p.args
                .iter()
                .any(|a| matches!(a.term, lute_manifest::fact::FactTerm::Param(_)))
        })
        .map(|p| FactRecord {
            relation: p.relation.clone(),
            args: p.args.iter().map(|a| fact_term_string(&a.term)).collect(),
        })
        .collect()
}

/// A fact pattern as authored text: `rel(a, b)` (`lute trace`'s form).
pub(crate) fn fact_text(pattern: &lute_manifest::fact::FactPattern) -> String {
    let args: Vec<String> = pattern
        .args
        .iter()
        .map(|a| fact_term_string(&a.term))
        .collect();
    format!("{}({})", pattern.relation, args.join(", "))
}

/// Lower an `::assert{ GroundFact }` (dsl 0.3.0 §5) to its delta command
/// record. Emitted as DATA only (D1) — no evaluation, no fact store; the
/// engine applies the write. The D13 malformed-parse sentinel
/// (`pattern.relation.is_empty()`) never reaches here: compile is check-
/// gated (`lib.rs`'s D6 gate) and a sentinel pattern is always paired with
/// an `E-DATALOG-PARSE`/`E-DATALOG-FUNCTION` Error diagnostic.
pub fn lower_assert(a: &Assert) -> Command {
    Command::Assert(AssertCmd {
        position: String::new(),
        relation: a.pattern.relation.clone(),
        args: a
            .pattern
            .args
            .iter()
            .map(|arg| fact_term_string(&arg.term))
            .collect(),
        stamp: Stamp::default(),
    })
}

/// Lower a `::retract{ RetractPattern }` (dsl 0.3.0 §5) to its delta command
/// record — mirrors [`lower_assert`]; `_` wildcard args pass through as
/// `"_"` verbatim (§5 RetractPattern).
pub fn lower_retract(r: &Retract) -> Command {
    Command::Retract(RetractCmd {
        position: String::new(),
        relation: r.pattern.relation.clone(),
        args: r
            .pattern
            .args
            .iter()
            .map(|arg| fact_term_string(&arg.term))
            .collect(),
        stamp: Stamp::default(),
    })
}

/// Lower one directive. `None` for `::use` and the component sentinels (the
/// walker consumes those). A plugin directive lowers to its declared
/// `lower: { record, fields }` staging command when it has one
/// ([`lower_record`]), else falls through to the `Some(Command::Plugin(..))`
/// passthrough.
///
/// `domains` is the resolved vocabulary: whether an `::actor`'s `action` ENDS the
/// character's presence is the `action` domain's declared `exits:` (dsl 0.9.0
/// D-D), not a prefix convention this crate re-guesses.
pub fn lower_directive(
    dir: &Directive,
    snapshot: &CapabilitySnapshot,
    domains: &BTreeMap<String, Domain>,
) -> Option<Command> {
    let get = |k: &str| attr_string(&dir.attrs, k);
    let decl = snapshot.directive(&dir.tag);
    // The directive as a stamp surface (0.50.0 §3.5); built only when a
    // plugin declares stamps.
    let surface =
        (!snapshot.stamp_attrs.is_empty()).then(|| StampSurface::Directive(dir.tag.clone()));
    let stamp = Stamp {
        timing: Timing {
            wait: effective_wait(dir, snapshot),
            // dsl 0.10.0 §10.3 (**D-T**): a time value's seconds are derived
            // from its milliseconds, never from a bare `f64::from_str`, so the
            // artifact agrees with the checker on `1.5s`/`250ms`.
            duration: time_attr_seconds(&dir.attrs, "duration"),
            delay: time_attr_seconds(&dir.attrs, "delay"),
            ..Timing::default()
        },
        // plugin §14.1: the stamps admissible on this directive ride its
        // stamp, core and plugin directives alike. A key the directive OWNS
        // stays out of them — the same precedence the checker applies in
        // `directives::check_directive` — so this only lifts the stamps.
        stamps: plugin_stamps(&dir.attrs, snapshot, surface.as_ref(), |k| {
            owns_attr(decl, k)
        }),
        authored: Some(authored_directive(dir, decl)),
        ..Stamp::default()
    };
    // dsl 0.37.0 D2: `display` is emitted resolved — author, then the
    // directive's declared default, then `show`.
    let display = || {
        get("display")
            .or_else(|| attr_default_string(decl, "display"))
            .unwrap_or_else(|| "show".to_string())
    };
    Some(match dir.tag.as_str() {
        "bg" => Command::Bg(BgCmd {
            position: String::new(),
            location: get("location"),
            time: get("time"),
            asset_id: get("assetId"),
            stamp,
        }),
        "music" => {
            let playback = get("playback");
            Command::Music(MusicCmd {
                position: String::new(),
                stop: playback.as_deref().and_then(|value| {
                    domains
                        .get("musicPlayback")
                        .filter(|domain| domain.stops.iter().any(|stop| stop == value))
                        .map(|_| true)
                }),
                playback,
                mood: get("mood"),
                volume: get("volume"),
                asset_id: get("assetId"),
                stamp,
            })
        }
        "sfx" => Command::Sfx(SfxCmd {
            position: String::new(),
            sound: get("sound"),
            asset_id: get("assetId"),
            stamp,
        }),
        "vfx" => Command::Vfx(VfxCmd {
            position: String::new(),
            r#type: get("type").unwrap_or_default(),
            label: get("label"),
            transition: get("transition"),
            stamp,
        }),
        lute_manifest::core::ACTOR_DIRECTIVE => {
            let action = get("action");
            // ONE reader of `exits:` for both crates (dsl 0.9.0 D-E): this used
            // to be a private prefix heuristic kept in sync by hand.
            let exit = match action.as_deref() {
                Some(a) if lute_check::is_declared_exit(a, domains) => Some(true),
                _ => None,
            };
            Command::Actor(ActorCmd {
                position: String::new(),
                character: get("character").unwrap_or_default(),
                anchor: get("anchor"),
                action,
                exit,
                emotion: get("emotion"),
                costume: get("costume"),
                pos_reset: None,
                preload: None,
                stamp,
            })
        }
        "camera" => Command::Camera(CameraCmd {
            position: String::new(),
            focus: get("focus"),
            framing: get("framing"),
            camera_move: get("move"),
            transition: get("transition"),
            stamp,
        }),
        "cg" => Command::Cg(CgCmd {
            position: String::new(),
            asset_id: get("assetId").unwrap_or_default(),
            display: display(),
            layout: get("layout"),
            stamp,
        }),
        "video" => Command::Video(VideoCmd {
            position: String::new(),
            asset_id: get("assetId").unwrap_or_default(),
            display: display(),
            stamp,
        }),
        lute_manifest::core::SEQUENCE_DIRECTIVE => Command::Sequence(SequenceCmd {
            position: String::new(),
            name: get("name").unwrap_or_default(),
            stamp,
        }),
        // dsl 0.8.0: the walk terminator. `::end` declares no `wait` attr, so
        // `effective_wait` yields `None` and the stamp stays omitted — the same
        // byte-stable treatment `music`/`sfx`/`vfx` get (§4.4).
        lute_manifest::core::END_DIRECTIVE => Command::End(EndCmd {
            position: String::new(),
            reason: get("reason"),
            stamp,
        }),
        // dsl 0.21.0 §7a.3: a CORE directive the checker resolves without a
        // snapshot lookup — dispatched on the tag before the plugin
        // fallthrough. The checker has proven `quest` a plain identifier
        // (`E-ACCEPT-TARGET`), so the quoted value lands verbatim.
        lute_syntax::ast::ACCEPT_DIRECTIVE => Command::Accept(AcceptCmd {
            position: String::new(),
            quest: dir
                .accept_quest()
                .map(|(q, _)| q.to_string())
                .unwrap_or_default(),
            // dsl 0.24.0 §2: the checker admits only `at="nextRun"`.
            applies: dir
                .accept_at()
                .filter(|(at, _)| *at == "nextRun")
                .map(|_| crate::ir::AcceptAt::NextRun),
            stamp,
        }),
        // dsl 0.12.0: `::label{name}` is a pure position anchor — emits NO
        // record. `id` is consumed by `stage::walk_seq`/`walk_quest`'s own
        // `mark` interception (`Emitter::bind_named`) BEFORE this function
        // is ever reached for a `mark` node — this arm exists only so the
        // generic `emit_primitive` dispatch (which calls `lower_directive`
        // for EVERY `Node::Directive`, mark included) stays total.
        lute_manifest::core::LABEL_DIRECTIVE => return None,
        // dsl 0.24.0 §4: `::clear` emits no record of its own. Its exits
        // depend on who is on stage, which only the walk's threaded
        // `StageState` knows: the reducer (`lute_check::inject`'s
        // `stage-clear` rule) injects one `sprite` exit per character, and
        // `stage::emit_primitive` emits those in its place.
        lute_manifest::core::CLEAR_DIRECTIVE => return None,
        // dsl 0.12.0: `::jump{to [when]}` — an unconditional jump (forward,
        // or back to an earlier label with `max`, dsl 0.51.0).
        // A GUARDED `::jump` is desugared by
        // `normalize::synth_when_next_match` into a canonical one-arm
        // `<match>` BEFORE this ever runs (mirrors the gated-line desugar),
        // so this arm only ever sees the UNCONDITIONAL form — reuses the
        // EXACT converge machinery branch/hub/match already lower through
        // (`JumpCmd`, `cfg::Label`/`Emitter`): `target` here is a NAMED
        // placeholder (`"#<id>"`), resolved to a real `addr` by
        // `address::assign_addresses`'s document-wide named-label pass,
        // exactly like a numeric `"@<n>"` resolves the anonymous ones.
        // dsl 0.51.0 §3.6: a bounded backward jump carries its `max` (the
        // checker has proven it a literal in 1..=99, `E-JUMP-MAX`). dsl
        // 0.52.0 §5.2: `leaves` comes from the checker's label pass, which
        // the walk holds (`stage::mark_hub_leave`).
        lute_manifest::core::JUMP_DIRECTIVE => Command::Jump(JumpCmd {
            position: String::new(),
            target: format!("#{}", get("to").unwrap_or_default()),
            max: dir
                .attrs
                .iter()
                .find(|a| a.key == lute_manifest::core::JUMP_MAX_ATTR)
                .and_then(|a| lute_check::next_labels::JumpMax::parse(&a.value))
                .map(lute_check::next_labels::JumpMax::get),
            leaves: None,
        }),
        // `COMPONENT_BEGIN`/`END`: normalization sentinels → no record. `use`:
        // DEFENSIVE/unreachable — normalize.rs fail-louds a timeline-clip `::use`
        // (E-COMPILE-COMPONENT) so `compile()` aborts at the §5 diag gate before any
        // artifact is kept; a Node-position `::use` is already expanded away (D8).
        "use" | COMPONENT_BEGIN | COMPONENT_END => return None,
        _ => {
            // Declarative lowering (`docs/plugin-system.md`): a directive whose
            // manifest decl carries `lower: { record, fields }` becomes that
            // CORE staging command, not the `kind: "plugin"` passthrough.
            // `lower: { kind: builtin, … }`, an absent `lower:` — and an
            // unknown/undeclared tag — fall through untouched.
            if let Some(cmd) = decl.and_then(|d| match &d.lower {
                Lowering::Record { record, fields } => {
                    // §4.4 blocking is a property of the RECORD KIND the engine
                    // dispatches on, not of the authored tag. `effective_wait`
                    // keys its builtin fallback on `dir.tag` (`bg`/`video`/…),
                    // which never matches a plugin tag — so without this a
                    // `lower: { record: background }` directive would emit a
                    // `background` record with no `wait` while core `::bg`
                    // emits `wait: true`, and an engine would block for one and
                    // not the other. Author/manifest resolution still wins.
                    let mut stamp = stamp.clone();
                    if stamp.timing.wait.is_none() {
                        stamp.timing.wait = record_wait_default(record);
                    }
                    lower_record(record, fields, dir, &stamp, domains)
                }
                Lowering::Builtin { .. } | Lowering::Passthrough => None,
            }) {
                return Some(cmd);
            }
            // Plugin passthrough (plan spec-gap note 1): fields typed via the
            // directive's manifest AttrDecls when the decl is known.
            let mut fields = BTreeMap::new();
            for a in &dir.attrs {
                if is_stamp_timing_attr(&a.key) {
                    continue; // already resolved into the stamp
                }
                // A stamp this directive does not own already rode into
                // `stamp.stamps` above; it must not be duplicated as one of
                // the record's own `fields`.
                if !owns_attr(decl, &a.key)
                    && surface
                        .as_ref()
                        .is_some_and(|s| snapshot.stamp_attrs.get(s, &a.key).is_some())
                {
                    continue;
                }
                // 0.51.0 §5.3 (Review051b F5): the implicit identity `code`
                // of a directive with `text` attributes is owned but is no
                // field — it reaches the artifact only in `localized` — and
                // neither is a component call's back-filled one.
                if (a.key == DIRECTIVE_TEXT_CODE_ATTR
                    || a.key == crate::normalize::COMPONENT_CODE_ATTR)
                    && decl.is_some_and(DirectiveDecl::has_text_attr)
                {
                    continue;
                }
                fields.insert(a.key.clone(), attr_json(a, decl));
            }
            // 0.41.0 §5.5: an omitted attr with a declared `default` ships
            // that default, typed as the authored value would be — the
            // runtime has no directive declarations to fill it from.
            for ad in decl.map(|d| d.attrs.as_slice()).unwrap_or_default() {
                if let Some(default) = ad
                    .default
                    .as_ref()
                    .filter(|_| !is_stamp_timing_attr(&ad.name))
                {
                    fields
                        .entry(ad.name.clone())
                        .or_insert_with(|| default_json(default, &ad.ty));
                }
            }
            // IR A12: resolve the manifest directive's declared `effects.writes`
            // into artifact-local bindings (fromAttr templates substituted).
            let effects = decl
                .and_then(|d| d.effects.as_ref().map(|eff| (d, eff)))
                .map(|(d, eff)| {
                    eff.writes
                        .iter()
                        .filter_map(|w| resolve_effect(w, dir, d, snapshot))
                        .collect()
                })
                .unwrap_or_default();
            // dsl 0.27.0 §4: the declared `effects.retracts` / `asserts`,
            // each `@attr` bound to the call's value or its declared default
            // (the checker's own resolution). A fact whose attr stays unbound
            // is not written.
            let facts = decl
                .map(|d| lute_check::directive_facts::call_facts(d, dir))
                .unwrap_or_default();
            // 0.51.0 §4.6: the declared `effects.raises`, each target
            // resolved as the checker's raise table resolves it (authored,
            // then the attribute's `default:`); an unbound one is not written.
            let raises = decl
                .map(|d| lute_check::raises::call_raises(d, &snapshot.occasions, &dir.attrs))
                .unwrap_or_default()
                .into_iter()
                .map(RaiseRecord::from)
                .collect();
            // 0.51.0 §5.4: each authored `text` attribute's localization;
            // addressing fills its `lineId` from `text_identity`.
            let text_decl = decl.filter(|d| d.has_text_attr());
            let localized = text_decl
                .map(|_| {
                    lute_check::directives::text_values(dir, snapshot)
                        .map(|(a, text)| {
                            let placeholders = lute_syntax::scan_label_interps(text, a.value_span)
                                .iter()
                                .map(placeholder_from_interp)
                                .collect();
                            (
                                a.key.clone(),
                                LocalizedText {
                                    line_id: String::new(),
                                    placeholders,
                                    texts: BTreeMap::new(),
                                },
                            )
                        })
                        .collect()
                })
                .unwrap_or_default();
            // An untagged component call carries its source-order back-fill
            // (`normalize::backfill_component_directive_codes`); an authored
            // code wins.
            let code_attr = |key: &str| {
                dir.attrs
                    .iter()
                    .find(|a| a.key == key)
                    .and_then(|a| match &a.value {
                        AttrValue::Str(s) => Some(s.trim().to_string()),
                        _ => None,
                    })
            };
            let text_identity = text_decl.map(|_| TextIdentity {
                code: if dir.attrs.iter().any(|a| a.key == DIRECTIVE_TEXT_CODE_ATTR) {
                    code_attr(DIRECTIVE_TEXT_CODE_ATTR)
                } else {
                    code_attr(crate::normalize::COMPONENT_CODE_ATTR)
                },
                at: dir.span.byte_start,
            });
            Command::Plugin(PluginCmd {
                position: String::new(),
                tag: dir.tag.clone(),
                plugin: snapshot
                    .directive_owner(&dir.tag)
                    .filter(|owner| *owner != "lute.core")
                    .map(str::to_owned),
                fields,
                effects,
                retracts: fact_records(&facts.retracts),
                asserts: fact_records(&facts.asserts),
                localized,
                stamp,
                raises,
                text_identity,
            })
        }
    })
}

/// The directive as written: `::tag{key="value" key=@ref flag}` in source
/// attribute order (a component's arguments already substituted); the
/// compiler's own `__`-prefixed bookkeeping attrs are not source. With the
/// directive's declaration, a value its attribute types as a number or a
/// bool reads bare (`n=2`), and so does an `occasion.target` argument — it
/// names the member, not the string.
pub(crate) fn authored_directive(dir: &Directive, decl: Option<&DirectiveDecl>) -> String {
    let attrs: Vec<String> = dir
        .attrs
        .iter()
        .filter(|a| !a.key.starts_with("__"))
        .map(|a| match &a.value {
            AttrValue::Str(s)
                if s == lute_manifest::semantics::beats::OCCASION_TARGET
                    || (decl.is_some() && !attr_json(a, decl).is_string()) =>
            {
                format!("{}={s}", a.key)
            }
            AttrValue::Str(s) => format!("{}=\"{}\"", a.key, s.replace('"', "\\\"")),
            AttrValue::Ref(slot) => format!("{}={}", a.key, slot.raw),
            AttrValue::BoolTrue => a.key.clone(),
        })
        .collect();
    if attrs.is_empty() {
        format!("::{}", dir.tag)
    } else {
        format!("::{}{{{}}}", dir.tag, attrs.join(" "))
    }
}

/// Build the CORE staging command a plugin directive declares via
/// `lower: { record, fields }` (`docs/plugin-system.md`), using the shared
/// checker field resolver so the artifact and stage reducer see identical
/// values. `None` when `record` is not one of the staging kinds.
fn lower_record(
    record: &str,
    fields: &yaml_serde::Value,
    dir: &Directive,
    stamp: &Stamp,
    domains: &BTreeMap<String, Domain>,
) -> Option<Command> {
    lute_manifest::validate::lower_record_fields(record)?;
    let resolved = lute_check::inject::resolve_record_fields(record, fields, dir);
    let s = |target: &str| {
        resolved
            .get(target)
            .and_then(lute_check::inject::ResolvedRecordField::as_str)
            .map(str::to_owned)
    };
    let b = |target: &str| {
        resolved
            .get(target)
            .and_then(lute_check::inject::ResolvedRecordField::as_bool)
    };

    let stamp = stamp.clone();
    let display = || s("display").unwrap_or_else(|| "show".to_string());
    // Field names below are the SERIALIZED (camelCase) IR names, matching
    // `lute_manifest::validate::lower_record_fields` entry-for-entry.
    Some(match record {
        "bg" => Command::Bg(BgCmd {
            position: String::new(),
            location: s("location"),
            time: s("time"),
            asset_id: s("assetId"),
            stamp,
        }),
        "music" => {
            let playback = s("playback");
            Command::Music(MusicCmd {
                position: String::new(),
                stop: playback.as_deref().and_then(|value| {
                    domains
                        .get("musicPlayback")
                        .filter(|domain| domain.stops.iter().any(|stop| stop == value))
                        .map(|_| true)
                }),
                playback,
                mood: s("mood"),
                volume: s("volume"),
                asset_id: s("assetId"),
                stamp,
            })
        }
        "sfx" => Command::Sfx(SfxCmd {
            position: String::new(),
            sound: s("sound"),
            asset_id: s("assetId"),
            stamp,
        }),
        "vfx" => Command::Vfx(VfxCmd {
            position: String::new(),
            r#type: s("type").unwrap_or_default(),
            label: s("label"),
            transition: s("transition"),
            stamp,
        }),
        "actor" => Command::Actor(ActorCmd {
            position: String::new(),
            character: s("character").unwrap_or_default(),
            anchor: s("anchor"),
            action: s("action"),
            exit: b("exit"),
            emotion: s("emotion"),
            costume: s("costume"),
            pos_reset: b("posReset"),
            preload: b("preload"),
            stamp,
        }),
        "camera" => Command::Camera(CameraCmd {
            position: String::new(),
            focus: s("focus"),
            framing: s("framing"),
            camera_move: s("move"),
            transition: s("transition"),
            stamp,
        }),
        "cg" => Command::Cg(CgCmd {
            position: String::new(),
            asset_id: s("assetId").unwrap_or_default(),
            display: display(),
            layout: s("layout"),
            stamp,
        }),
        "video" => Command::Video(VideoCmd {
            position: String::new(),
            asset_id: s("assetId").unwrap_or_default(),
            display: display(),
            stamp,
        }),
        "sequence" => Command::Sequence(SequenceCmd {
            position: String::new(),
            name: s("name").unwrap_or_default(),
            stamp,
        }),
        // Unreachable: the table gate above admits exactly these nine.
        _ => return None,
    })
}

/// Resolved effective blocking (§4.3 / IR A8): author `wait` attr → manifest
/// `AttrDecl.default` → builtin fallback. The wait-family (compile-IR §4.4) is
/// `bg`/`video`/`sequence` (default `true`) and `cg`/`camera` (default
/// `false`); `camera`/`video`/`sequence` are normally resolved by their
/// manifest decls above and are listed here for completeness. `plugin`
/// directives flow through steps 1–2 (author → manifest, else none).
/// `music`/`sfx`/`vfx`/`actor` define no `wait` (§4.4) → `None` → omitted.
pub fn effective_wait(dir: &Directive, snapshot: &CapabilitySnapshot) -> Option<bool> {
    if let Some(b) = attr_bool(&dir.attrs, "wait") {
        return Some(b);
    }
    if let Some(decl) = snapshot.directive(&dir.tag) {
        if let Some(a) = decl.attrs.iter().find(|a| a.name == "wait") {
            if let Some(Literal::Bool(b)) = &a.default {
                return Some(*b);
            }
        }
    }
    record_wait_default(&dir.tag)
}

/// The builtin `wait` default of a CORE record KIND (compile-IR §4.4). Every
/// kind is named after its core directive (dsl 0.37.0), so this one table
/// serves both the authored tag ([`effective_wait`]) and a plugin directive
/// that reaches the kind through `lower: { record, fields }`:
/// `bg`/`video`/`sequence` block, `cg`/`camera` do not, and every other
/// staging kind defines no `wait` at all (the field stays omitted).
fn record_wait_default(record: &str) -> Option<bool> {
    match record {
        "bg" | "video" | "sequence" => Some(true),
        "cg" | "camera" => Some(false),
        _ => None,
    }
}

/// A directive attr's declared string default (`display: show`), if any.
fn attr_default_string(decl: Option<&DirectiveDecl>, key: &str) -> Option<String> {
    match &decl?.attrs.iter().find(|a| a.name == key)?.default {
        Some(Literal::Str(s)) => Some(s.clone()),
        _ => None,
    }
}

/// The value of attribute `key` as the compiler stages it: a string literal
/// verbatim, an `@ref` value's raw text, a bare flag as `true`. `lute loc
/// export` reads a prompt through this, so its row text is the record's.
pub fn attr_string(attrs: &[Attr], key: &str) -> Option<String> {
    attrs.iter().find(|a| a.key == key).map(|a| match &a.value {
        AttrValue::Str(s) => s.clone(),
        AttrValue::Ref(slot) => slot.raw.clone(),
        AttrValue::BoolTrue => "true".to_string(),
    })
}

/// A cross-cutting time attr as SECONDS, derived from its milliseconds
/// (dsl 0.10.0 §10.3, **D-T**). `None` when absent, unparseable, or finer than
/// a millisecond — the last of which is already `E-TIME-RESOLUTION` at the
/// checker, and `compile` does not gate on it.
fn time_attr_seconds(attrs: &[Attr], key: &str) -> Option<f64> {
    match lute_check::parse_time_ms(attr_string(attrs, key)?.as_str()) {
        lute_check::TimeParse::Ms(ms) => Some(lute_check::ms_to_seconds(ms)),
        lute_check::TimeParse::TooFine | lute_check::TimeParse::NotANumber => None,
    }
}

/// A boolean attr (a hub choice's `once`/`exit`, a directive's bool field),
/// read through the language's one flag reader, [`AttrValue::flag`]
/// (dsl 0.28.0 §1), so the compiler and the checker agree on every value.
pub(crate) fn attr_bool(attrs: &[Attr], key: &str) -> Option<bool> {
    attrs.iter().find(|a| a.key == key)?.value.flag()
}

/// Whether the directive `decl` owns `key`: an attribute it declares, or the
/// implicit identity `code` of a directive with a `text` attribute (0.51.0
/// §5.3, Review051b F5). The discriminator between "the record's own" and
/// "this may be a plugin-declared cross-cutting stamp attr" (plugin §14.1) —
/// the same precedence the checker uses when admitting an attr key.
fn owns_attr(decl: Option<&DirectiveDecl>, key: &str) -> bool {
    decl.is_some_and(|d| d.owns_attr(key))
}

/// 0.50.0 §3.5: the plugin stamps a `<branch>`, `<hub>` or `<choice>`
/// (`surface`) authors, for its record or its option. No key of the
/// element's own can resolve to a stamp: assembly refuses a scoped stamp
/// that shadows one (§3.2 rule 3), and a cross-cutting stamp never applies
/// to an element (§3.1).
pub(crate) fn element_stamps(
    attrs: &[Attr],
    snapshot: &CapabilitySnapshot,
    surface: &StampSurface,
) -> BTreeMap<String, serde_json::Value> {
    plugin_stamps(attrs, snapshot, Some(surface), |_| false)
}

/// Lift the plugin stamps (plugin §14.1 `stampAttrs`) admissible on
/// `surface` — the record's surface, `None` when no plugin declares stamps —
/// out of `attrs` into a record's `Stamp.stamps`, each value typed by the
/// declaration that applies on `surface` (0.50.0 §3.5) through the SAME
/// [`attr_json_typed`] path a directive field uses, so a stamp typed
/// differently on two surfaces serializes with each one's type. `own`
/// reports the keys the record itself consumes: those WIN and stay put,
/// mirroring the checker's admission order (the record's own decls, then
/// `stampAttrs`, then `E-UNKNOWN-ATTR`).
///
/// Byte-stability: an UNAUTHORED stamp attr is never injected, not even when
/// its decl carries a `default` — absent means absent, so a document that
/// authors none serializes exactly as it did before the vocabulary existed.
/// The map serializes nested under `stamps` (0.49.0 §3.1), so a stamp named
/// like a key of its record cannot collide with it. The core stamp names
/// (`at`, `duration`, `delay`, `wait`, `timeline`, `timing`, `provenance`,
/// `source`) stay reserved at assembly (`E-PLUGIN-RESERVED-STAMP-ATTR`):
/// they would read as the core stamp to an author.
fn plugin_stamps(
    attrs: &[Attr],
    snapshot: &CapabilitySnapshot,
    surface: Option<&StampSurface>,
    own: impl Fn(&str) -> bool,
) -> BTreeMap<String, serde_json::Value> {
    let mut stamps = BTreeMap::new();
    // Overwhelmingly the common case: no plugin declares a stamp vocabulary,
    // so skip the per-attr map lookups entirely.
    let Some(surface) = surface.filter(|_| !snapshot.stamp_attrs.is_empty()) else {
        return stamps;
    };
    for a in attrs {
        if own(&a.key) {
            continue;
        }
        if let Some(sdecl) = snapshot.stamp_attrs.get(surface, &a.key) {
            stamps.insert(a.key.clone(), attr_json_typed(a, Some(&sdecl.ty)));
        }
    }
    stamps
}

fn attr_json(attr: &Attr, decl: Option<&DirectiveDecl>) -> serde_json::Value {
    let ty = decl
        .and_then(|d| d.attrs.iter().find(|a| a.name == attr.key))
        .map(|a| &a.ty);
    attr_json_typed(attr, ty)
}

/// An attr value as JSON in the shape its declared type asks for: `int` or
/// `double` parses numerically, `bool` maps the two literals, everything else
/// stays a string.
pub(crate) fn attr_json_typed(attr: &Attr, ty: Option<&Type>) -> serde_json::Value {
    match &attr.value {
        AttrValue::BoolTrue => serde_json::Value::Bool(true),
        AttrValue::Ref(slot) => serde_json::Value::String(slot.raw.clone()),
        AttrValue::Str(s) => match ty {
            Some(Type::Int) => s.parse::<i64>().ok().map_or_else(
                || serde_json::Value::String(s.clone()),
                serde_json::Value::from,
            ),
            Some(Type::Double) => s.parse::<f64>().ok().map_or_else(
                || serde_json::Value::String(s.clone()),
                serde_json::Value::from,
            ),
            Some(Type::Bool) => match s.as_str() {
                "true" => serde_json::Value::Bool(true),
                "false" => serde_json::Value::Bool(false),
                _ => serde_json::Value::String(s.clone()),
            },
            _ => serde_json::Value::String(s.clone()),
        },
    }
}

/// A declared attr `default` as JSON. Assembly already proved it a value of
/// the attr's type (`lute_manifest::validate::attr_default_accepts`), whose
/// one widening is a YAML integer defaulting a `double`: that reads `1.0`,
/// as an authored `1` would.
pub(crate) fn default_json(default: &Literal, ty: &Type) -> serde_json::Value {
    match (default, ty) {
        (Literal::Int(n), Type::Double) => serde_json::Value::from(*n as f64),
        _ => crate::literal_json(default),
    }
}

/// The cross-cutting timing attrs every directive resolves into its stamp
/// rather than its `fields`.
fn is_stamp_timing_attr(key: &str) -> bool {
    matches!(key, "wait" | "duration" | "delay")
}

/// The state path one manifest `WriteDecl` writes at this call: `scope` + each
/// segment joined by `.`, with `fromAttr` segments replaced by the record's
/// attr value (e.g. `resultKey="debut"` → `debut`).
pub fn effect_path(w: &WriteDecl, dir: &Directive) -> String {
    let mut segments = vec![w.scope.clone()];
    for seg in &w.path {
        match seg {
            PathSegment::Literal(s) => segments.push(s.clone()),
            PathSegment::FromAttr { from_attr } => {
                segments.push(attr_string(&dir.attrs, &from_attr.name).unwrap_or_default());
            }
        }
    }
    lute_check::target_writes::join_path(&segments)
}

/// Resolve one manifest `WriteDecl` into an artifact-local [`Effect`] (IR A12)
/// at [`effect_path`]. Numeric effect values use the declared destination field
/// type, rather than the YAML number's incidental integral/fractional spelling.
#[expect(
    clippy::cast_possible_truncation,
    reason = "a whole number from a plugin manifest: past the i64 range `as` saturates, as before"
)]
pub fn resolve_effect(
    w: &WriteDecl,
    dir: &Directive,
    decl: &DirectiveDecl,
    snapshot: &CapabilitySnapshot,
) -> Option<Effect> {
    let attr_value = |name: &str| -> Option<serde_json::Value> {
        let ty = decl.attrs.iter().find(|a| a.name == name);
        match dir.attrs.iter().find(|a| a.key == name) {
            Some(a) => Some(attr_json_typed(a, ty.map(|t| &t.ty))),
            None => ty.and_then(|t| t.default.as_ref()).map(crate::literal_json),
        }
    };
    let target_ty = effect_target_type(w, dir, decl, snapshot);
    let numeric = |n: f64| match target_ty {
        Some(Type::Int) if n.is_finite() && n.fract() == 0.0 => {
            Some(crate::literal_json(&Literal::Int(n as i64)))
        }
        Some(Type::Double) if n.is_finite() => Some(crate::literal_json(&Literal::Double(n))),
        _ if n.is_finite() && n.fract() == 0.0 => {
            Some(crate::literal_json(&Literal::Int(n as i64)))
        }
        _ if n.is_finite() => Some(crate::literal_json(&Literal::Double(n))),
        _ => None,
    };
    let from = match &w.value {
        WriteValue::FromBridgeResult { from_bridge_result } => EffectSource::BridgeResult {
            bridge_result: from_bridge_result.clone(),
        },
        WriteValue::Op { op, by } => EffectSource::Op {
            op: op.clone(),
            by: match by {
                OpBy::Num(n) => numeric(*n)?,
                OpBy::FromAttr { from_attr } => attr_value(from_attr)?,
            },
        },
        WriteValue::FromAttr { from_attr } => EffectSource::Literal(attr_value(from_attr)?),
        WriteValue::Literal(lit) => EffectSource::Literal(match (target_ty, lit) {
            (Some(Type::Int), Literal::Double(n)) if n.is_finite() && n.fract() == 0.0 => {
                crate::literal_json(&Literal::Int(*n as i64))
            }
            (Some(Type::Double), Literal::Int(n)) => {
                crate::literal_json(&Literal::Double(*n as f64))
            }
            _ => crate::literal_json(lit),
        }),
    };
    Some(Effect {
        path: effect_path(w, dir),
        from,
    })
}

/// Find the scalar type of an effect's destination field in its declared
/// result shape. The checker has already expanded the same declaration into
/// concrete state paths; this keeps compile-time JSON typing on that contract.
fn effect_target_type<'a>(
    w: &WriteDecl,
    dir: &Directive,
    decl: &'a DirectiveDecl,
    snapshot: &'a CapabilitySnapshot,
) -> Option<&'a Type> {
    let path = effect_path(w, dir);
    let mut best: Option<(String, &str)> = None;
    for slot in &decl.state.as_ref()?.declares {
        let mut base = slot.scope.clone();
        for seg in &slot.path {
            match seg {
                PathSegment::Literal(s) => {
                    base.push('.');
                    base.push_str(s);
                }
                PathSegment::FromAttr { from_attr } => {
                    base.push('.');
                    base.push_str(
                        attr_string(&dir.attrs, &from_attr.name)
                            .unwrap_or_default()
                            .as_str(),
                    );
                }
            }
        }
        if path.starts_with(&base)
            && path.as_bytes().get(base.len()) == Some(&b'.')
            && best.as_ref().is_none_or(|(old, _)| base.len() > old.len())
        {
            best = Some((base, slot.shape.as_str()));
        }
    }
    let (base, shape) = best?;
    let mut rest = path[base.len() + 1..].split('.');
    let mut shape = snapshot.state_shapes.get(shape)?;
    loop {
        let name = rest.next()?;
        let field = shape.fields.iter().find(|f| f.name == name)?;
        if let Some(next) = &field.shape {
            shape = snapshot.state_shapes.get(next)?;
        } else {
            return Some(&field.ty);
        }
    }
}

#[cfg(test)]
mod tests {
    use lute_core_span::Severity;
    use lute_syntax::ast::Node;

    use super::*;

    fn nodes(body: &str) -> Vec<Node> {
        let src =
            format!("---\nkind: scene\ncharacter: marina\nseason: 1\nepisode: 2\n---\n\n## Shot 1.\n\n{body}\n");
        let (doc, diags) = lute_syntax::parse(&src);
        assert!(
            diags.iter().all(|d| d.severity != Severity::Error),
            "{diags:#?}"
        );
        doc.sections[0].body.clone()
    }

    fn snap() -> CapabilitySnapshot {
        lute_manifest::core::load_core_snapshot()
    }

    /// The shared test vocabulary (`lute-test-vocab`), so these lowering tests
    /// declare their `action`/`anchor` members exactly once, in the same place
    /// `lute-check`'s suite does — a second hand-written copy here would be the
    /// duplication dsl 0.9.0 D-E deleted.
    fn doms() -> BTreeMap<String, Domain> {
        lute_test_vocab::test_domains()
    }

    fn lower_first(body: &str) -> serde_json::Value {
        let ns = nodes(body);
        let cmd = match &ns[0] {
            Node::Line(l) => lower_line(l, &snap()),
            Node::Directive(d) => lower_directive(d, &snap(), &doms()).expect("lowers"),
            Node::Set(s) => lower_set(s),
            other => panic!("unexpected node {other:?}"),
        };
        serde_json::to_value(&cmd).unwrap()
    }

    #[test]
    fn lowers_assert_and_retract() {
        let ns = nodes("::assert{ inParty(ana) }\n::retract{ atLoc(ana, _) }");
        let Node::Assert(a) = &ns[0] else { panic!() };
        let v = serde_json::to_value(lower_assert(a)).unwrap();
        assert_eq!(v["kind"], "assert");
        assert_eq!(v["relation"], "inParty");
        assert_eq!(v["args"], serde_json::json!(["ana"]));
        let Node::Retract(r) = &ns[1] else { panic!() };
        let v = serde_json::to_value(lower_retract(r)).unwrap();
        assert_eq!(v["kind"], "retract");
        assert_eq!(v["args"], serde_json::json!(["ana", "_"]));
    }

    #[test]
    fn line_roles_derive_from_speaker_and_delivery() {
        let v = lower_first("@narrator: Venny's.");
        assert_eq!(v["kind"], "line");
        assert_eq!(v["role"], "narration");
        let v = lower_first("@fixer{mono}: Hm.");
        assert_eq!(v["role"], "mono");
        let v = lower_first("@fixer{vo}: Later.");
        assert_eq!(v["role"], "vo");
        let v = lower_first("@fixer{os}: Behind the door.");
        assert_eq!(v["role"], "os");
        let v = lower_first(
            "@marina{code=\"0010\" emotion=\"surprised\" variant=\"0\" as=\"Hostess\"}: Oh!",
        );
        assert_eq!(v["role"], "dialogue");
        assert_eq!(v["speaker"], "marina");
        assert_eq!(v["text"], "Oh!");
        assert_eq!(v["emotion"], "surprised");
        assert_eq!(v["variant"], 0);
        assert_eq!(v["as"], "Hostess");
        // `code` is consumed into identity later — never a JSON field.
        assert!(v.get("code").is_none());
    }

    #[test]
    fn bg_defaults_wait_true_camera_defaults_wait_false() {
        let v =
            lower_first("::bg{location=\"family_restaurant\" time=\"afternoon\" assetId=\"BG.x\"}");
        assert_eq!(v["kind"], "bg");
        assert_eq!(v["family"], "staging");
        assert_eq!(v["location"], "family_restaurant");
        assert_eq!(v["time"], "afternoon");
        assert_eq!(v["assetId"], "BG.x");
        assert_eq!(v["timing"]["wait"], true);
        let v = lower_first("::camera{focus=\"marina\" framing=\"close\" duration=\"0.5\"}");
        assert_eq!(v["kind"], "camera");
        assert_eq!(v["focus"], "marina");
        assert_eq!(v["framing"], "close");
        assert_eq!(v["timing"]["duration"], 0.5);
        assert_eq!(v["timing"]["wait"], false); // manifest default
        let v = lower_first("::camera{move=\"shake\" wait=\"true\"}");
        assert_eq!(v["move"], "shake");
        assert_eq!(v["timing"]["wait"], true); // author override beats the default
    }

    #[test]
    fn wait_family_materialized_cg_gains_false_others_carry_none() {
        // IR A8 / compile-IR §4.4: the wait-family (bg/video/sequence/camera/
        // cg/plugin) MUST carry a resolved `timing.wait`; music/sfx/vfx/actor
        // carry NO `timing` at all.
        let v = lower_first("::cg{assetId=\"CUT.x\"}");
        assert_eq!(v["kind"], "cg");
        assert_eq!(v["timing"]["wait"], false);
        // bg/video default true; camera default false (manifest) — unchanged.
        assert_eq!(lower_first("::bg{location=\"r\"}")["timing"]["wait"], true);
        assert_eq!(
            lower_first("::video{assetId=\"MOVIE.x\" display=\"show\"}")["timing"]["wait"],
            true
        );
        assert_eq!(
            lower_first("::camera{move=\"shake\"}")["timing"]["wait"],
            false
        );
        // Non-wait families (§4.4) carry NO `timing` key.
        assert!(lower_first("::music{playback=\"start\"}")
            .get("timing")
            .is_none());
        assert!(lower_first("::sfx{sound=\"ding\"}").get("timing").is_none());
        assert!(lower_first("::vfx{type=\"whiteOut\"}")
            .get("timing")
            .is_none());
        assert!(
            lower_first("::actor{character=\"marina\" anchor=\"center\"}")
                .get("timing")
                .is_none()
        );
    }

    #[test]
    fn remaining_core_directives_lower_to_their_kinds() {
        let v = lower_first(
            "::music{playback=\"start\" mood=\"peaceful\" volume=\"quiet\" assetId=\"m.mp3\"}",
        );
        assert_eq!(v["kind"], "music");
        assert_eq!(v["playback"], "start");
        assert_eq!(v["mood"], "peaceful");
        assert_eq!(v["volume"], "quiet");
        let v = lower_first("::sfx{sound=\"hum\" assetId=\"s.mp3\"}");
        assert_eq!(v["kind"], "sfx");
        assert_eq!(v["sound"], "hum");
        let v = lower_first("::vfx{type=\"whiteOut\" transition=\"flash\"}");
        assert_eq!(v["kind"], "vfx");
        assert_eq!(v["type"], "whiteOut");
        let v = lower_first("::cg{assetId=\"CUT.x\"}");
        assert_eq!(v["kind"], "cg");
        assert_eq!(v["assetId"], "CUT.x");
        assert_eq!(v["display"], "show");
        let v = lower_first("::video{assetId=\"MOVIE.x\" display=\"hide\"}");
        assert_eq!(v["kind"], "video");
        assert_eq!(v["display"], "hide");
        assert_eq!(v["timing"]["wait"], true);
        let v = lower_first("::actor{character=\"marina\" anchor=\"center\" action=\"fadeInUp\"}");
        assert_eq!(v["kind"], "actor");
        assert_eq!(v["character"], "marina");
        assert_eq!(v["anchor"], "center");
        assert!(v.get("exit").is_none());
        let v = lower_first("::actor{character=\"marina\" action=\"fadeOutDown\"}");
        assert_eq!(v["exit"], true);
    }

    #[test]
    fn set_ops_lower_verbatim() {
        for op in ["=", "+=", "-=", "*="] {
            let v = lower_first(&format!("::set{{scene.affect.marina {op} 1}}"));
            assert_eq!(v["kind"], "set");
            assert_eq!(v["path"], "scene.affect.marina");
            assert_eq!(v["op"], *op);
            assert_eq!(v["value"]["cel"], "1");
            assert_eq!(v["value"]["expr"], serde_json::json!({"int": 1}));
        }
    }

    #[test]
    fn plugin_directive_passes_through_with_typed_fields() {
        // `::minigame` is NOT in the core snapshot => generic passthrough
        // (plan spec-gap note 1); untyped attrs stay strings.
        let v = lower_first("::minigame{kind=\"rhythm\" id=\"x\" resultKey=\"service01\"}");
        assert_eq!(v["kind"], "plugin");
        assert_eq!(v["tag"], "minigame");
        assert_eq!(v["fields"]["kind"], "rhythm");
        assert_eq!(v["fields"]["resultKey"], "service01");
    }

    #[test]
    fn use_and_sentinels_lower_to_nothing() {
        let ns = nodes("::use{component=\"greet\" who=\"marina\"}");
        let Node::Directive(d) = &ns[0] else { panic!() };
        assert!(lower_directive(d, &snap(), &doms()).is_none());
        let begin = lute_syntax::ast::Directive {
            tag: crate::normalize::COMPONENT_BEGIN.to_string(),
            attrs: Vec::new(),
            when: None,
            span: d.span,
        };
        assert!(lower_directive(&begin, &snap(), &doms()).is_none());
    }

    #[test]
    fn camera_duration_is_a_json_number_and_removed_numerics_are_absent() {
        // IR A10: a typed numeric attr is a JSON number, not a string. dsl
        // 0.37.0 §3.3 removed the numeric camera fields, so a domain-valued
        // camera fills none of them.
        let v = lower_first("::camera{framing=\"close\" move=\"shake\" duration=\"0.4\"}");
        assert_eq!(v["kind"], "camera");
        assert!(
            v["timing"]["duration"].is_number(),
            "duration must be a JSON number, got {}",
            v["timing"]["duration"]
        );
        assert_eq!(v["timing"]["duration"], 0.4);
        for removed in ["zoom", "moveX", "moveY", "shake", "reset", "easing"] {
            assert!(v.get(removed).is_none(), "{removed}: {v}");
        }
    }

    #[test]
    fn actor_record_omits_unauthored_costume() {
        // dsl 0.37.0 D4: `costume` is emitted when authored, absent otherwise.
        let v = lower_first("::actor{character=\"marina\" anchor=\"center\"}");
        assert_eq!(v["kind"], "actor");
        assert!(
            v.get("costume").is_none(),
            "costume must be absent, got {:?}",
            v.get("costume")
        );
    }

    /// A snapshot whose `::<tag>` plugin directive declares `attrs` and the
    /// given `lower:`.
    fn snap_with(tag: &str, attrs: &[(&str, Type)], lower: Lowering) -> CapabilitySnapshot {
        let mut snap = snap();
        snap.directives.insert(
            tag.to_string(),
            DirectiveDecl {
                name: tag.to_string(),
                layer: Some("staging".into()),
                attrs: attrs
                    .iter()
                    .map(|(n, ty)| lute_manifest::schema::AttrDecl {
                        name: (*n).to_string(),
                        required: false,
                        ty: ty.clone(),
                        default: None,
                    })
                    .collect(),
                semantics: vec![],
                state: None,
                effects: None,
                bridge: None,
                lower,
            },
        );
        snap
    }

    fn record_lowering(record: &str, fields_yaml: &str) -> Lowering {
        Lowering::Record {
            record: record.into(),
            fields: yaml_serde::from_str(fields_yaml).expect("fixture yaml"),
        }
    }

    fn lower_with(body: &str, snapshot: &CapabilitySnapshot) -> serde_json::Value {
        let ns = nodes(body);
        let Node::Directive(d) = &ns[0] else {
            panic!("expected a directive, got {:?}", ns[0])
        };
        serde_json::to_value(lower_directive(d, snapshot, &doms()).expect("lowers")).unwrap()
    }

    #[test]
    fn declarative_record_lowering_emits_the_core_staging_record() {
        // The declarative-lowering promise: `::backdrop{img=…}` declaring
        // `lower: { record: bg, fields: { assetId: { fromAttr: img } } }`
        // becomes a real `bg` record — NOT the `kind: "plugin"` passthrough.
        let snapshot = snap_with(
            "backdrop",
            &[("img", Type::Str)],
            record_lowering("bg", "{ assetId: { fromAttr: img } }"),
        );
        let v = lower_with("::backdrop{img=\"BG.x\"}", &snapshot);
        assert_eq!(v["kind"], "bg");
        assert_eq!(
            v["timing"]["wait"], true,
            "the bg kind's wait default applies"
        );
        assert_eq!(v["assetId"], "BG.x");
        assert!(v.get("tag").is_none(), "must not be a passthrough: {v}");
        // Unbound target fields stay absent (skip-if-none), so the record is
        // byte-identical to the same `::bg` an author could have written.
        assert!(v.get("location").is_none(), "{v}");
        assert!(v.get("time").is_none(), "{v}");
    }

    #[test]
    fn builtin_lowering_still_takes_the_plugin_passthrough() {
        let snapshot = snap_with(
            "minigame",
            &[("img", Type::Str)],
            Lowering::Builtin {
                kind: "builtin".into(),
                name: "minigame".into(),
            },
        );
        let v = lower_with("::minigame{img=\"BG.x\"}", &snapshot);
        assert_eq!(v["kind"], "plugin");
        assert_eq!(v["tag"], "minigame");
        assert_eq!(v["fields"]["img"], "BG.x");
    }

    #[test]
    fn record_lowering_mixes_literals_and_missing_sources() {
        // A baked literal fills its field; a `fromAttr` whose source attr the
        // author omitted leaves the target at its default (absent).
        let snapshot = snap_with(
            "scenery",
            &[("img", Type::Str), ("where", Type::Str)],
            record_lowering(
                "bg",
                "{ assetId: { fromAttr: img }, location: { fromAttr: where }, time: dusk }",
            ),
        );
        let v = lower_with("::scenery{img=\"BG.y\"}", &snapshot);
        assert_eq!(v["kind"], "bg");
        assert_eq!(v["assetId"], "BG.y");
        assert_eq!(v["time"], "dusk");
        assert!(
            v.get("location").is_none(),
            "unsourced field stays absent: {v}"
        );
    }

    #[test]
    fn record_lowering_typed_fields_serialize_as_json_scalars() {
        let snapshot = snap_with(
            "pose",
            &[("who", Type::Str), ("gone", Type::Bool)],
            record_lowering(
                "actor",
                "{ character: { fromAttr: who }, exit: { fromAttr: gone } }",
            ),
        );
        let v = lower_with("::pose{who=\"marina\" gone=\"true\"}", &snapshot);
        assert_eq!(v["kind"], "actor");
        assert_eq!(v["character"], "marina");
        assert_eq!(v["exit"], true);
    }

    #[test]
    fn unknown_record_degrades_to_passthrough_never_a_panic() {
        // A declaration assembly would have rejected (`E-LOWER-RECORD-UNKNOWN`)
        // must still lower TOTALLY if it somehow leaks through.
        let snapshot = snap_with(
            "narrate",
            &[("img", Type::Str)],
            record_lowering("line", "{ speaker: { fromAttr: img } }"),
        );
        let v = lower_with("::narrate{img=\"x\"}", &snapshot);
        assert_eq!(v["kind"], "plugin");
        assert_eq!(v["tag"], "narrate");
    }

    #[test]
    fn record_field_table_matches_lowering() {
        // The manifest-side field table (`lower_record_fields`, the validator's
        // authority) and this module's per-record construction MUST agree name
        // for name — bind every table field through a `fromAttr` and assert the
        // serialized record carries all of them.
        use lute_manifest::validate::{lower_record_fields, lower_record_kinds, LowerFieldKind};
        for record in lower_record_kinds() {
            let table = lower_record_fields(record).expect("table entry");
            let attrs: Vec<(&str, Type)> = table
                .iter()
                .map(|(n, k)| {
                    (
                        *n,
                        match k {
                            LowerFieldKind::Str => Type::Str,
                            LowerFieldKind::Bool => Type::Bool,
                        },
                    )
                })
                .collect();
            let fields_yaml = format!(
                "{{ {} }}",
                table
                    .iter()
                    .map(|(n, _)| format!("{n}: {{ fromAttr: {n} }}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            let body = format!(
                "::probe{{{}}}",
                table
                    .iter()
                    .map(|(n, k)| {
                        let v = match k {
                            LowerFieldKind::Str => "v",
                            LowerFieldKind::Bool => "true",
                        };
                        format!("{n}=\"{v}\"")
                    })
                    .collect::<Vec<_>>()
                    .join(" ")
            );
            let snapshot = snap_with("probe", &attrs, record_lowering(record, &fields_yaml));
            let v = lower_with(&body, &snapshot);
            assert_eq!(v["kind"], record, "record kind for `{record}`");
            for (name, _) in table {
                assert!(
                    v.get(name).is_some(),
                    "record `{record}` must bind `{name}`; got {v}"
                );
            }
        }
    }
}
