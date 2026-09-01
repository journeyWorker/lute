//! Pass-1 direct lowering (§5): each primitive node → its typed record,
//! schema-driven and pure. `addr`/`lineId`/`voiceKey` stay empty here — the
//! addressing pass (Task 11) owns identity; the stage walker (Tasks 8–9)
//! owns order, stamps, and injection.

use std::collections::BTreeMap;

use lute_manifest::schema::{DirectiveDecl, Lowering, WriteDecl, WriteValue};
use lute_manifest::snapshot::{CapabilitySnapshot, Domain};
use lute_manifest::types::{Literal, PathSegment, Type};
use lute_syntax::ast::{Assert, Attr, AttrValue, Directive, Line, Retract, Set};

use crate::ir::*;
use crate::normalize::{COMPONENT_BEGIN, COMPONENT_END};

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
        Role::Monologue
    } else if has_delivery_flag(&line.attrs, "vo") {
        Role::Voiceover
    } else if has_delivery_flag(&line.attrs, "os") {
        Role::Offscreen
    } else {
        Role::Dialogue
    };
    Command::Line(LineCmd {
        addr: String::new(),
        role,
        speaker: line.speaker.clone(),
        text: line.text.clone(),
        emotion: get("emotion"),
        variant: get("variant").and_then(|v| v.parse::<i64>().ok()),
        action: get("action"),
        dialog_motion: get("dialogMotion"),
        as_label: get("as"),
        line_id: String::new(),
        voice_key: None,
        placeholders: line.interps.iter().map(placeholder_from_interp).collect(),
        texts: Default::default(),
        code: get("code"),
        stamp: Stamp {
            // plugin §14.1: a content line carries cross-cutting stamp attrs
            // like any other record. The built-in content-line keys win (they
            // ARE the record's own fields above) — the same precedence the
            // checker applies in `content_line::check_content_line_attrs`.
            extra: stamp_extra(&line.attrs, snapshot, |k| {
                lute_check::content_line::KNOWN_ATTRS.contains(&k)
            }),
            ..Stamp::default()
        },
    })
}

pub fn lower_set(set: &Set) -> Command {
    Command::Set(SetCmd {
        addr: String::new(),
        path: set.path.clone(),
        op: set.op.clone(),
        value: set.expr.raw.clone(),
        expr: crate::expr::lower_expr(&set.expr.raw),
        stamp: Stamp::default(),
    })
}

/// A [`FactTerm`] as its ground string (dsl 0.3.0 §5): `Ident` verbatim,
/// `Bool` as `"true"`/`"false"`, `Wildcard` as `"_"` (retract-pattern-only —
/// never emitted from an `::assert`, checker-enforced `E-RETRACT-WILDCARD-
/// ASSERT`).
fn fact_term_string(t: &lute_syntax::datalog::FactTerm) -> String {
    use lute_syntax::datalog::FactTerm;
    match t {
        FactTerm::Ident(s) => s.clone(),
        FactTerm::Bool(b) => b.to_string(),
        FactTerm::Wildcard => "_".to_string(),
    }
}

/// Lower an `::assert{ GroundFact }` (dsl 0.3.0 §5) to its delta command
/// record. Emitted as DATA only (D1) — no evaluation, no fact store; the
/// engine applies the write. The D13 malformed-parse sentinel
/// (`pattern.relation.is_empty()`) never reaches here: compile is check-
/// gated (`lib.rs`'s D6 gate) and a sentinel pattern is always paired with
/// an `E-DATALOG-PARSE`/`E-DATALOG-FUNCTION` Error diagnostic.
pub fn lower_assert(a: &Assert) -> Command {
    Command::Assert(AssertCmd {
        addr: String::new(),
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
        addr: String::new(),
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
/// ([`lower_record`]), else falls through to the `Some(Command::Other(..))`
/// passthrough.
///
/// `domains` is the resolved vocabulary: whether an `::auto`'s `action` ENDS the
/// character's presence is the `action` domain's declared `exits:` (dsl 0.9.0
/// D-D), not a prefix convention this crate re-guesses.
pub fn lower_directive(
    dir: &Directive,
    snapshot: &CapabilitySnapshot,
    domains: &BTreeMap<String, Domain>,
) -> Option<Command> {
    let get = |k: &str| attr_string(&dir.attrs, k);
    let get_f64 = |k: &str| attr_f64(&dir.attrs, k);
    let get_bool = |k: &str| attr_bool(&dir.attrs, k);
    let decl = snapshot.directive(&dir.tag);
    let stamp = Stamp {
        wait: effective_wait(dir, snapshot),
        // dsl 0.10.0 §10.3 (**D-T**): a time value's seconds are derived from
        // its milliseconds, never from a bare `f64::from_str`. That also makes
        // the artifact agree with the checker on `1.5s`/`250ms`, which
        // `attr_f64` silently dropped while the timeline resolver accepted them.
        duration: time_attr_seconds(&dir.attrs, "duration"),
        delay: time_attr_seconds(&dir.attrs, "delay"),
        // plugin §14.1: cross-cutting `stampAttrs` ride the stamp on EVERY
        // directive, core and plugin alike. A key the directive DECLARES
        // itself stays the record's own field — the same precedence the
        // checker applies in `directives::check_directive` — so this only
        // lifts the genuinely cross-cutting ones.
        extra: stamp_extra(&dir.attrs, snapshot, |k| declares_attr(decl, k)),
        ..Stamp::default()
    };
    Some(match dir.tag.as_str() {
        "bg" => Command::Background(BackgroundCmd {
            addr: String::new(),
            location: get("location"),
            time: get("time"),
            asset_id: get("assetId"),
            stamp,
        }),
        "music" => Command::Music(MusicCmd {
            addr: String::new(),
            action: get("action").unwrap_or_default(),
            mood: get("mood"),
            volume: get("volume"),
            asset_id: get("assetId"),
            track: get("track"),
            stamp,
        }),
        "sfx" => Command::Sfx(SfxCmd {
            addr: String::new(),
            sound: get("sound"),
            asset_id: get("assetId"),
            name: get("name"),
            stamp,
        }),
        "vfx" => Command::Vfx(VfxCmd {
            addr: String::new(),
            vfx_type: get("type").unwrap_or_default(),
            label: get("label"),
            transition: get("transition"),
            stamp,
        }),
        "auto" => {
            let action = get("action");
            // ONE reader of `exits:` for both crates (dsl 0.9.0 D-E): this used
            // to be a private prefix heuristic kept in sync by hand.
            let exit = match action.as_deref() {
                Some(a) if lute_check::is_declared_exit(a, domains) => Some(true),
                _ => None,
            };
            Command::Sprite(SpriteCmd {
                addr: String::new(),
                character: get("character").unwrap_or_default(),
                anchor: get("anchor"),
                action,
                exit,
                pos_reset: None,
                preload: None,
                emotion: None,
                costume: None,
                stamp,
            })
        }
        "camera" => Command::Camera(CameraCmd {
            addr: String::new(),
            focus: get("focus"),
            zoom: get_f64("zoom"),
            move_x: get_f64("move-x"),
            move_y: get_f64("move-y"),
            shake: get_f64("shake"),
            reset: get_bool("reset"),
            easing: get("easing"),
            stamp,
        }),
        "cut" => Command::Cut(CutCmd {
            addr: String::new(),
            asset_id: get("assetId").unwrap_or_default(),
            action: get("action"),
            full: get_bool("full"),
            stamp,
        }),
        "video" => Command::Video(VideoCmd {
            addr: String::new(),
            asset_id: get("assetId").unwrap_or_default(),
            action: get("action"),
            stamp,
        }),
        // dsl 0.8.0: the walk terminator. `::end` declares no `wait` attr, so
        // `effective_wait` yields `None` and the stamp stays omitted — the same
        // byte-stable treatment `music`/`sfx`/`vfx` get (§4.4).
        lute_manifest::core::END_DIRECTIVE => Command::End(EndCmd {
            addr: String::new(),
            reason: get("reason"),
            stamp,
        }),
        // dsl 0.12.0: `::mark{id}` is a pure position anchor — emits NO
        // record. `id` is consumed by `stage::walk_seq`/`walk_quest`'s own
        // `mark` interception (`Emitter::bind_named`) BEFORE this function
        // is ever reached for a `mark` node — this arm exists only so the
        // generic `emit_primitive` dispatch (which calls `lower_directive`
        // for EVERY `Node::Directive`, mark included) stays total.
        lute_manifest::core::MARK_DIRECTIVE => return None,
        // dsl 0.12.0: `::next{to [when]}` — an unconditional forward jump.
        // A GUARDED `::next` is desugared by
        // `normalize::synth_when_next_match` into a canonical one-arm
        // `<match>` BEFORE this ever runs (mirrors the gated-line desugar),
        // so this arm only ever sees the UNCONDITIONAL form — reuses the
        // EXACT converge machinery branch/hub/match already lower through
        // (`JumpCmd`, `cfg::Label`/`Emitter`): `target` here is a NAMED
        // placeholder (`"#<id>"`), resolved to a real `addr` by
        // `address::assign_addresses`'s document-wide named-label pass,
        // exactly like a numeric `"@<n>"` resolves the anonymous ones.
        lute_manifest::core::NEXT_DIRECTIVE => Command::Jump(JumpCmd {
            addr: String::new(),
            target: format!("#{}", get("to").unwrap_or_default()),
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
            // `lower: { kind: builtin, … }` — and an unknown/undeclared tag —
            // fall through untouched.
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
                    if stamp.wait.is_none() {
                        stamp.wait = record_wait_default(record);
                    }
                    lower_record(record, fields, dir, &stamp)
                }
                Lowering::Builtin { .. } => None,
            }) {
                return Some(cmd);
            }
            // Plugin passthrough (plan spec-gap note 1): fields typed via the
            // directive's manifest AttrDecls when the decl is known.
            let mut fields = BTreeMap::new();
            for a in &dir.attrs {
                if a.key == "wait" || a.key == "duration" || a.key == "delay" {
                    continue; // already resolved into the stamp
                }
                // A cross-cutting stamp attr this directive does not declare
                // itself already rode into `stamp.extra` above; it must not be
                // duplicated as one of the record's own `fields`.
                if !declares_attr(decl, &a.key) && snapshot.stamp_attrs.contains_key(&a.key) {
                    continue;
                }
                fields.insert(a.key.clone(), attr_json(a, decl));
            }
            // IR A12: resolve the manifest directive's declared `effects.writes`
            // into artifact-local bindings (fromAttr templates substituted).
            let effects = decl
                .and_then(|d| d.effects.as_ref())
                .map(|eff| eff.writes.iter().map(|w| resolve_effect(w, dir)).collect())
                .unwrap_or_default();
            Command::Other(OtherCmd {
                addr: String::new(),
                tag: dir.tag.clone(),
                fields,
                effects,
                stamp,
            })
        }
    })
}

/// Where one declarative-lowering target field gets its value.
enum FieldSrc<'a> {
    /// `{ fromAttr: <attrName> }` — read the authored attr off the directive.
    Attr(&'a str),
    /// A YAML literal baked into the manifest.
    Lit(&'a serde_yaml::Value),
}

/// Build the CORE staging command a plugin directive declares via
/// `lower: { record, fields }` (`docs/plugin-system.md`), substituting each
/// `fromAttr` binding from the authored attrs and carrying the caller's
/// `stamp` unchanged. `None` when `record` is not one of the staging kinds —
/// the caller then falls back to the `kind: "plugin"` passthrough.
///
/// TOTAL by construction: the declaration was already validated at ASSEMBLY
/// (`lute_manifest::validate::validate_directive` → `E-LOWER-RECORD-UNKNOWN` /
/// `E-LOWER-RECORD-FIELD`) against the SAME field table this reads, so a
/// conforming project never lands here with a bad mapping. A non-conforming
/// one still cannot produce garbage: an unknown record degrades to
/// passthrough, an unknown target field is ignored, and an unresolvable
/// binding (missing attr, wrong literal kind) leaves the target field at its
/// `None`/default — never a panic.
fn lower_record(
    record: &str,
    fields: &serde_yaml::Value,
    dir: &Directive,
    stamp: &Stamp,
) -> Option<Command> {
    // Gate on the shared table so an unknown record kind is a passthrough, not
    // a silently-empty staging record.
    lute_manifest::validate::lower_record_fields(record)?;

    let mut srcs: BTreeMap<&str, FieldSrc> = BTreeMap::new();
    if let serde_yaml::Value::Mapping(m) = fields {
        for (k, v) in m {
            let Some(target) = k.as_str() else { continue };
            let src = match v.get("fromAttr").and_then(|a| a.as_str()) {
                Some(attr) => FieldSrc::Attr(attr),
                None => FieldSrc::Lit(v),
            };
            srcs.insert(target, src);
        }
    }

    // One resolver per target kind; a field with no binding, or a binding whose
    // source is absent/uncoercible, yields `None` (the field's default).
    let s = |target: &str| -> Option<String> {
        match srcs.get(target)? {
            FieldSrc::Attr(a) => attr_string(&dir.attrs, a),
            FieldSrc::Lit(v) => match v {
                serde_yaml::Value::String(s) => Some(s.clone()),
                serde_yaml::Value::Bool(b) => Some(b.to_string()),
                serde_yaml::Value::Number(n) => Some(n.to_string()),
                _ => None,
            },
        }
    };
    let n = |target: &str| -> Option<f64> {
        match srcs.get(target)? {
            FieldSrc::Attr(a) => attr_f64(&dir.attrs, a),
            FieldSrc::Lit(v) => v.as_f64(),
        }
    };
    let b = |target: &str| -> Option<bool> {
        match srcs.get(target)? {
            FieldSrc::Attr(a) => attr_bool(&dir.attrs, a),
            FieldSrc::Lit(v) => v.as_bool(),
        }
    };

    let stamp = stamp.clone();
    // Field names below are the SERIALIZED (camelCase) IR names, matching
    // `lute_manifest::validate::lower_record_fields` entry-for-entry — the
    // `record_field_table_matches_lowering` test holds the two in lockstep.
    Some(match record {
        "background" => Command::Background(BackgroundCmd {
            addr: String::new(),
            location: s("location"),
            time: s("time"),
            asset_id: s("assetId"),
            stamp,
        }),
        "music" => Command::Music(MusicCmd {
            addr: String::new(),
            action: s("action").unwrap_or_default(),
            mood: s("mood"),
            volume: s("volume"),
            asset_id: s("assetId"),
            track: s("track"),
            stamp,
        }),
        "sfx" => Command::Sfx(SfxCmd {
            addr: String::new(),
            sound: s("sound"),
            asset_id: s("assetId"),
            name: s("name"),
            stamp,
        }),
        "vfx" => Command::Vfx(VfxCmd {
            addr: String::new(),
            vfx_type: s("vfxType").unwrap_or_default(),
            label: s("label"),
            transition: s("transition"),
            stamp,
        }),
        "sprite" => Command::Sprite(SpriteCmd {
            addr: String::new(),
            character: s("character").unwrap_or_default(),
            anchor: s("anchor"),
            action: s("action"),
            exit: b("exit"),
            pos_reset: b("posReset"),
            preload: b("preload"),
            emotion: s("emotion"),
            costume: s("costume"),
            stamp,
        }),
        "camera" => Command::Camera(CameraCmd {
            addr: String::new(),
            focus: s("focus"),
            zoom: n("zoom"),
            move_x: n("moveX"),
            move_y: n("moveY"),
            shake: n("shake"),
            reset: b("reset"),
            easing: s("easing"),
            stamp,
        }),
        "cut" => Command::Cut(CutCmd {
            addr: String::new(),
            asset_id: s("assetId").unwrap_or_default(),
            action: s("action"),
            full: b("full"),
            stamp,
        }),
        "video" => Command::Video(VideoCmd {
            addr: String::new(),
            asset_id: s("assetId").unwrap_or_default(),
            action: s("action"),
            stamp,
        }),
        // Unreachable: the table gate above admits exactly these eight.
        _ => return None,
    })
}

/// Resolved effective blocking (§4.3 / IR A8): author `wait` attr → manifest
/// `AttrDecl.default` → builtin fallback. The wait-family (compile-IR §4.4) is
/// `bg`/`video` (default `true`) and `cut`/`camera` (default `false`, v1
/// non-blocking); `camera` is normally resolved by its manifest decl above and
/// is listed here for completeness. `plugin` directives flow through steps 1–2
/// (author → manifest, else none). `music`/`sfx`/`vfx`/`sprite` define no
/// `wait` (§4.4) → `None` → the field is omitted, keeping them byte-stable.
///
/// Step 1 (author override) is only *reachable* through `compile()`'s D6 gate
/// for directives whose manifest declares a `wait` attr — `video`/`camera`
/// (dsl §999). `bg`/`cut` declare no `wait`, so an authored `wait` on them is
/// rejected `E-UNKNOWN-ATTR` and never reaches here; they always carry the
/// fixed resolved default (`bg`→`true`, `cut`→`false`).
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
    match dir.tag.as_str() {
        "bg" | "video" => Some(true),
        "cut" | "camera" => Some(false),
        _ => None,
    }
}

/// The builtin `wait` default of a CORE record KIND (compile-IR §4.4), keyed
/// on the emitted `kind` rather than the authored tag. [`effective_wait`]'s
/// own fallback keys on `dir.tag`, which is correct for core directives (whose
/// tag and record kind coincide) but never matches a plugin directive that
/// reaches the same record kind through `lower: { record, fields }`. Kept
/// beside `effective_wait` so the two tables are read together and cannot
/// drift: `background`/`video` block, `cut`/`camera` do not, and every other
/// staging kind defines no `wait` at all (the field stays omitted).
fn record_wait_default(record: &str) -> Option<bool> {
    match record {
        "background" | "video" => Some(true),
        "cut" | "camera" => Some(false),
        _ => None,
    }
}

pub(crate) fn attr_string(attrs: &[Attr], key: &str) -> Option<String> {
    attrs.iter().find(|a| a.key == key).map(|a| match &a.value {
        AttrValue::Str(s) => s.clone(),
        AttrValue::Ref(slot) => slot.raw.clone(),
        AttrValue::BoolTrue => "true".to_string(),
    })
}

fn attr_f64(attrs: &[Attr], key: &str) -> Option<f64> {
    attr_string(attrs, key).and_then(|s| s.parse::<f64>().ok())
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

pub(crate) fn attr_bool(attrs: &[Attr], key: &str) -> Option<bool> {
    attrs
        .iter()
        .find(|a| a.key == key)
        .and_then(|a| match &a.value {
            AttrValue::BoolTrue => Some(true),
            AttrValue::Str(s) => match s.as_str() {
                "true" => Some(true),
                "false" => Some(false),
                _ => None,
            },
            AttrValue::Ref(_) => None,
        })
}

/// Whether `decl` declares `key` as one of the directive's OWN attributes.
/// The discriminator between "this is a field of the record" and "this may be
/// a plugin-declared cross-cutting stamp attr" (plugin §14.1) — and the same
/// precedence the checker uses when admitting an attr key.
fn declares_attr(decl: Option<&DirectiveDecl>, key: &str) -> bool {
    decl.is_some_and(|d| d.attrs.iter().any(|a| a.name == key))
}

/// Lift the plugin-declared CROSS-CUTTING attrs (plugin §14.1 `stampAttrs`)
/// out of `attrs` into a record's `Stamp.extra`, each value typed by its
/// declaring `AttrDecl` through the SAME [`attr_json_typed`] path a directive
/// field uses. `own` reports the keys the record itself consumes: those WIN
/// and stay put, mirroring the checker's admission order (the record's own
/// decls, then `stampAttrs`, then `E-UNKNOWN-ATTR`).
///
/// Byte-stability: an UNAUTHORED stamp attr is never injected, not even when
/// its decl carries a `default` — absent means absent, so a document that
/// authors none serializes exactly as it did before the vocabulary existed.
/// A reserved stamp key can never appear here (assembly rejects one,
/// `E-PLUGIN-RESERVED-STAMP-ATTR`), so `extra` can never shadow the core stamp.
fn stamp_extra(
    attrs: &[Attr],
    snapshot: &CapabilitySnapshot,
    own: impl Fn(&str) -> bool,
) -> BTreeMap<String, serde_json::Value> {
    let mut extra = BTreeMap::new();
    // Overwhelmingly the common case: no plugin declares a cross-cutting
    // vocabulary, so skip the per-attr map lookups entirely.
    if snapshot.stamp_attrs.is_empty() {
        return extra;
    }
    for a in attrs {
        if own(&a.key) {
            continue;
        }
        if let Some(sdecl) = snapshot.stamp_attrs.get(&a.key) {
            extra.insert(a.key.clone(), attr_json_typed(a, Some(&sdecl.ty)));
        }
    }
    extra
}

fn attr_json(attr: &Attr, decl: Option<&DirectiveDecl>) -> serde_json::Value {
    let ty = decl
        .and_then(|d| d.attrs.iter().find(|a| a.name == attr.key))
        .map(|a| &a.ty);
    attr_json_typed(attr, ty)
}

/// An attr value as JSON in the shape its declared type asks for: `number`
/// parses, `bool` maps the two literals, everything else stays a string. An
/// undeclared type (`None`) or an unparseable value falls back to the raw
/// string rather than dropping the value.
fn attr_json_typed(attr: &Attr, ty: Option<&Type>) -> serde_json::Value {
    match &attr.value {
        AttrValue::BoolTrue => serde_json::Value::Bool(true),
        AttrValue::Ref(slot) => serde_json::Value::String(slot.raw.clone()),
        AttrValue::Str(s) => match ty {
            Some(Type::Number) => s
                .parse::<f64>()
                .ok()
                .map(serde_json::Value::from)
                .unwrap_or_else(|| serde_json::Value::String(s.clone())),
            Some(Type::Bool) => match s.as_str() {
                "true" => serde_json::Value::Bool(true),
                "false" => serde_json::Value::Bool(false),
                _ => serde_json::Value::String(s.clone()),
            },
            _ => serde_json::Value::String(s.clone()),
        },
    }
}

/// Resolve one manifest `WriteDecl` into an artifact-local [`Effect`] (IR A12).
/// The path is `scope` + each segment joined by `.`, with `fromAttr` segments
/// replaced by the record's attr value (e.g. `resultKey="debut"` → `debut`).
/// The source is the bridge-result key, the `op`/`by` increment (integral `by`),
/// or a literal — all integral-collapsed via `literal_json` (no duplication).
fn resolve_effect(w: &WriteDecl, dir: &Directive) -> Effect {
    let mut segments = vec![w.scope.clone()];
    for seg in &w.path {
        match seg {
            PathSegment::Literal(s) => segments.push(s.clone()),
            PathSegment::FromAttr { from_attr } => {
                segments.push(attr_string(&dir.attrs, &from_attr.name).unwrap_or_default())
            }
        }
    }
    let from = match &w.value {
        WriteValue::FromBridgeResult { from_bridge_result } => EffectSource::BridgeResult {
            bridge_result: from_bridge_result.clone(),
        },
        WriteValue::Op { op, by } => EffectSource::Op {
            op: op.clone(),
            by: crate::literal_json(&Literal::Num(*by)),
        },
        WriteValue::Literal(lit) => EffectSource::Literal(crate::literal_json(lit)),
    };
    Effect {
        path: segments.join("."),
        from,
    }
}

#[cfg(test)]
mod tests {
    use lute_core_span::Severity;
    use lute_manifest::snapshot::CapabilitySnapshot;
    use lute_syntax::ast::Node;

    use super::*;

    fn nodes(body: &str) -> Vec<Node> {
        let src =
            format!("---\nkind: scene\ncharacter: bianca\nseason: 1\nepisode: 2\n---\n\n## Shot 1.\n\n{body}\n");
        let (doc, diags) = lute_syntax::parse(&src);
        assert!(
            diags.iter().all(|d| d.severity != Severity::Error),
            "{diags:#?}"
        );
        doc.shots[0].body.clone()
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
        assert_eq!(v["role"], "monologue");
        let v = lower_first("@fixer{vo}: Later.");
        assert_eq!(v["role"], "voiceover");
        let v = lower_first("@fixer{os}: Behind the door.");
        assert_eq!(v["role"], "offscreen");
        let v = lower_first(
            "@bianca{code=\"0010\" emotion=\"surprised\" variant=\"0\" as=\"Hostess\"}: Oh!",
        );
        assert_eq!(v["role"], "dialogue");
        assert_eq!(v["speaker"], "bianca");
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
        assert_eq!(v["kind"], "background");
        assert_eq!(v["location"], "family_restaurant");
        assert_eq!(v["time"], "afternoon");
        assert_eq!(v["assetId"], "BG.x");
        assert_eq!(v["wait"], true);
        let v = lower_first(
            "::camera{focus=\"bianca\" zoom=\"1.1\" move-x=\"0.2\" duration=\"0.5\" easing=\"ease-out\"}",
        );
        assert_eq!(v["kind"], "camera");
        assert_eq!(v["zoom"], 1.1);
        assert_eq!(v["moveX"], 0.2);
        assert_eq!(v["duration"], 0.5);
        assert_eq!(v["easing"], "ease-out");
        assert_eq!(v["wait"], false); // manifest default (arch §1 open question)
        let v = lower_first("::camera{shake=\"0.6\" wait=\"true\"}");
        assert_eq!(v["wait"], true); // author override beats the default
    }

    #[test]
    fn wait_family_materialized_cut_gains_false_others_carry_none() {
        // IR A8 / compile-IR §4.4: the wait-family (bg/video/camera/cut/plugin)
        // MUST carry a resolved `wait`; music/sfx/vfx/sprite carry NO `wait`.
        // THE FIX: `::cut` resolves to a concrete `false` (v1 non-blocking).
        let v = lower_first("::cut{assetId=\"CUT.x\"}");
        assert_eq!(v["kind"], "cut");
        assert_eq!(v["wait"], false);
        // bg/video default true; camera default false (manifest) — unchanged.
        assert_eq!(lower_first("::bg{location=\"r\"}")["wait"], true);
        assert_eq!(
            lower_first("::video{assetId=\"MOVIE.x\" action=\"show\"}")["wait"],
            true
        );
        assert_eq!(lower_first("::camera{shake=\"0.6\"}")["wait"], false);
        // Non-wait families (§4.4) carry NO `wait` key.
        assert!(lower_first("::music{action=\"start\"}")
            .get("wait")
            .is_none());
        assert!(lower_first("::sfx{sound=\"ding\"}").get("wait").is_none());
        assert!(lower_first("::vfx{type=\"whiteOut\"}")
            .get("wait")
            .is_none());
        assert!(
            lower_first("::auto{character=\"bianca\" anchor=\"center\"}")
                .get("wait")
                .is_none()
        );
    }

    #[test]
    fn remaining_core_directives_lower_to_their_kinds() {
        let v = lower_first(
            "::music{action=\"start\" mood=\"peaceful\" volume=\"down\" assetId=\"m.mp3\"}",
        );
        assert_eq!(v["kind"], "music");
        assert_eq!(v["action"], "start");
        assert_eq!(v["mood"], "peaceful");
        assert_eq!(v["volume"], "down");
        let v = lower_first("::sfx{sound=\"hum\" assetId=\"s.mp3\"}");
        assert_eq!(v["kind"], "sfx");
        assert_eq!(v["sound"], "hum");
        let v = lower_first("::vfx{type=\"whiteOut\" transition=\"flash\"}");
        assert_eq!(v["kind"], "vfx");
        assert_eq!(v["vfxType"], "whiteOut");
        let v = lower_first("::cut{assetId=\"CUT.x\" full}");
        assert_eq!(v["kind"], "cut");
        assert_eq!(v["assetId"], "CUT.x");
        assert_eq!(v["full"], true);
        let v = lower_first("::video{assetId=\"MOVIE.x\" action=\"show\"}");
        assert_eq!(v["kind"], "video");
        assert_eq!(v["wait"], true);
        let v = lower_first("::auto{character=\"bianca\" anchor=\"center\" action=\"fade-in-up\"}");
        assert_eq!(v["kind"], "sprite");
        assert_eq!(v["character"], "bianca");
        assert_eq!(v["anchor"], "center");
        assert!(v.get("exit").is_none());
        let v = lower_first("::auto{character=\"bianca\" action=\"fade-out-down\"}");
        assert_eq!(v["exit"], true);
    }

    #[test]
    fn set_ops_lower_verbatim() {
        for op in ["=", "+=", "-=", "*="] {
            let v = lower_first(&format!("::set{{scene.affect.bianca {op} 1}}"));
            assert_eq!(v["kind"], "set");
            assert_eq!(v["path"], "scene.affect.bianca");
            assert_eq!(v["op"], *op);
            assert_eq!(v["value"], "1");
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
        let ns = nodes("::use{component=\"greet\" who=\"bianca\"}");
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
    fn camera_shake_and_zoom_serialize_as_json_numbers() {
        // IR A10: typed numeric camera attrs are JSON numbers, not strings.
        // `shake` must match `zoom`/`moveX`/`moveY` (the audit found it emitted
        // as the string "0.4" beside `zoom: 1.2`).
        let v = lower_first("::camera{shake=\"0.4\" zoom=\"1.2\"}");
        assert_eq!(v["kind"], "camera");
        assert!(
            v["shake"].is_number(),
            "shake must be a JSON number, got {}",
            v["shake"]
        );
        assert_eq!(v["shake"], 0.4);
        assert!(
            v["zoom"].is_number(),
            "zoom must be a JSON number, got {}",
            v["zoom"]
        );
        assert_eq!(v["zoom"], 1.2);
    }

    #[test]
    fn camera_bool_attr_serializes_as_json_bool() {
        // IR A10: a typed bool attr is a JSON bool, not a string (confirms the
        // existing `get_bool` coercion for core records).
        let v = lower_first("::camera{shake=\"0.4\" reset=\"true\"}");
        assert!(
            v["reset"].is_boolean(),
            "reset must be a JSON bool, got {}",
            v["reset"]
        );
        assert_eq!(v["reset"], true);
    }

    #[test]
    fn sprite_record_omits_costume_until_cast_ships() {
        // IR A1 (schema-only): `costume` is always None until the character-cast
        // plugin ships, so it never serializes (skip-if-none).
        let v = lower_first("::auto{character=\"bianca\" anchor=\"center\"}");
        assert_eq!(v["kind"], "sprite");
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
            fields: serde_yaml::from_str(fields_yaml).expect("fixture yaml"),
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
        // `lower: { record: background, fields: { assetId: { fromAttr: img } } }`
        // becomes a real `background` record — NOT the `kind: "plugin"`
        // passthrough it used to fall into.
        let snapshot = snap_with(
            "backdrop",
            &[("img", Type::Str)],
            record_lowering("background", "{ assetId: { fromAttr: img } }"),
        );
        let v = lower_with("::backdrop{img=\"BG.x\"}", &snapshot);
        assert_eq!(v["kind"], "background");
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
                "background",
                "{ assetId: { fromAttr: img }, location: { fromAttr: where }, time: dusk }",
            ),
        );
        let v = lower_with("::scenery{img=\"BG.y\"}", &snapshot);
        assert_eq!(v["kind"], "background");
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
            "lens",
            &[("z", Type::Number), ("snap", Type::Bool)],
            record_lowering(
                "camera",
                "{ zoom: { fromAttr: z }, reset: { fromAttr: snap } }",
            ),
        );
        let v = lower_with("::lens{z=\"1.25\" snap=\"true\"}", &snapshot);
        assert_eq!(v["kind"], "camera");
        assert_eq!(v["zoom"], 1.25);
        assert_eq!(v["reset"], true);
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
                            LowerFieldKind::Num => Type::Number,
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
                            LowerFieldKind::Num => "1.5",
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
