//! `textDocument/hover` (Task 6.3): explain the construct under the cursor.
//!
//! A pure function over a parsed [`Document`] + [`CapabilitySnapshot`] + byte
//! offset. Resolves the cursor ([`super::resolve`]) and renders Markdown for:
//! - a directive `::name` -> its [`DirectiveDecl`] (layer, attrs w/ type+required,
//!   semantics);
//! - an attribute key -> its [`AttrDecl`] (type, required, default);
//! - an `@ref` -> the def's CEL text + type (author `defs:` first, then snapshot);
//! - a state path -> its `state:` decl type + default;
//! - an enum-typed attr value -> the enum domain;
//! - an `assetKind`-typed `assetId` value -> the segment under the cursor (its
//!   name, declared type, and authored value);
//! - a `::jump{to=…}` / `::label{name=…}` value -> the label, where it is
//!   declared and how many jumps target it (dsl 0.37.0 §3.5);
//! - a content-line attr key -> its meaning (delivery flags select the line
//!   role), and the `@speaker` -> the line's role (dsl 0.37.0 §3.4);
//! - an inline modifier name in line text -> `pause`/`speed`/`textStyle`
//!   (dsl 0.37.0 §3.6).
//!
//! A plain-string `assetId` (an attr NOT typed `assetKind`) or any other value
//! with no capability match yields `None`. `Hover.range` is `None`: highlighting the hovered
//! span is optional and would require the document text this pure fn does not hold.

use lute_check::{parse_meta, SchemaImports};
use lute_manifest::asset;
use lute_manifest::schema::AssetKindDecl;
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_core_span::Span;
use lute_manifest::core::{CLEAR_DIRECTIVE, JUMP_DIRECTIVE, LABEL_DIRECTIVE, LABEL_NAME_ATTR};
use lute_syntax::ast::{AttrValue, Document, InlineModifier, InlineNode, InterpKind, Line};
use tower_lsp_server::ls_types::{Hover, HoverContents, MarkupContent, MarkupKind};

use lute_resolve::cursor::{
    attr_at, attr_enum_values, asset_kind_for, asset_segment_index, choice_id, def_info,
    interp_path, interp_ref_name, is_state_path, literal_label, path_at, ref_at, subject_domain,
    subject_is_number, type_label, Cursor, QuestConstruct,
};

/// Hover documentation for the construct at byte offset `off`, or `None` when the
/// cursor rests on something with no capability-backed explanation.
pub fn hover_at(
    doc: &Document,
    snapshot: &CapabilitySnapshot,
    imports: &SchemaImports,
    off: usize,
) -> Option<Hover> {
    let (mut meta, _) = parse_meta(&doc.meta, snapshot);
    lute_resolve::cursor::merge_imports(&mut meta, imports);
    let cursor = match lute_resolve::cursor::resolve(doc, off) {
        Some(cursor) => cursor,
        None => return hover_with_identity(doc, off, modifier_hover(doc, off)),
    };
    let md = match cursor {
        Cursor::DirectiveName(tag) => directive_hover(snapshot, tag),
        Cursor::AttrValue {
            directive: Some(dir),
            key,
        } => {
            // A label name documents the label; an `assetId` value documents
            // the segment under the cursor; else an enum attr documents its
            // domain; else the attr's own declaration.
            if (dir == JUMP_DIRECTIVE && key == "to")
                || (dir == LABEL_DIRECTIVE && key == LABEL_NAME_ATTR)
            {
                super::label_name_at(doc, off).map(|name| label_hover(doc, name))
            } else if let Some(kind) = asset_kind_for(snapshot, dir, key) {
                asset_segment_hover(kind, doc, off)
            } else if let Some(vals) = attr_enum_values(snapshot, imports, &meta, dir, key) {
                Some(format!("**enum** `{key}`\n\ndomain: {}", vals.join(", ")))
            } else {
                attr_hover(snapshot, dir, key)
            }
        }
        Cursor::AttrKey {
            directive: Some(dir),
            key,
        } => attr_hover(snapshot, dir, key),
        Cursor::Cel { slot, .. } => {
            let local = off.saturating_sub(slot.span.byte_start);
            if let Some(host) = host_hover(&slot.raw, local) {
                Some(host)
            } else if let Some(r) = ref_at(slot, off) {
                if r.is_dollar {
                    Some("`$` — the `<match>` subject".to_string())
                } else {
                    ref_hover(&meta, snapshot, &r.name)
                }
            } else if let Some((tok, _)) = path_at(slot, off) {
                if is_state_path(&tok) {
                    state_hover(&meta, &tok).or_else(|| choice_hover(&tok))
                } else {
                    None
                }
            } else {
                None
            }
        }
        Cursor::Interp(i) => match i.kind {
            // A state path renders its `state:` decl (or the implicit choice path),
            // exactly as a CEL-slot path read does.
            InterpKind::Path => {
                let path = interp_path(&i.raw);
                if is_state_path(&path) {
                    state_hover(&meta, &path).or_else(|| choice_hover(&path))
                } else {
                    None
                }
            }
            // An `@ref` renders the def hover (`@fond` → the `fond` def).
            InterpKind::Ref => {
                interp_ref_name(&i.raw).and_then(|name| ref_hover(&meta, snapshot, &name))
            }
            // A reserved token (`userName`) always renders and has no decl.
            InterpKind::Reserved => Some(format!(
                "**reserved token** `{}` — always renders; no declaration",
                i.raw
            )),
        },
        Cursor::IsPattern { subject_path } => subject_domain(doc, &meta, subject_path)
            .map(|domain| format!("**pattern** — domain: {}", domain.join(", ")))
            .or_else(|| {
                subject_is_number(&meta, subject_path).then(|| {
                    "**pattern** — domain: int or double; literals are points (`3`, `-1.5`) or \
                     inclusive ranges (`1..3`, `2..`, `..0`)"
                        .to_string()
                })
            }),
        // A content-line key; a bare delivery flag (`{os}`) resolves as a value
        // because its key and value spans coincide.
        Cursor::AttrKey {
            directive: None,
            key,
        } => content_line_attr_hover(doc, off, key),
        Cursor::AttrValue {
            directive: None,
            key,
        } if attr_at(doc, off).is_some_and(|a| matches!(a.value, AttrValue::BoolTrue)) => {
            content_line_attr_hover(doc, off, key)
        }
        Cursor::Speaker => super::line_at(doc, off).map(speaker_hover),
        Cursor::DirectiveAttrArea { .. }
        | Cursor::AttrValue {
            directive: None, ..
        } => None,
        Cursor::SetPath { path } => state_hover(&meta, path),
        Cursor::ConstructAttrArea { construct } => Some(construct_hover(construct)),
        Cursor::OnEventValue(event) => event_hover(snapshot, event),
    };
    hover_with_identity(doc, off, md)
}

/// A content-line attribute key's meaning (dsl 0.37.0 §3.4); `None` off a
/// content line or for an unknown key.
fn content_line_attr_hover(doc: &Document, off: usize, key: &str) -> Option<String> {
    super::line_at(doc, off)?;
    super::completion::content_line_attr_detail(key)
        .map(|detail| format!("**`{key}`** (content line): {detail}"))
}

/// Append the identity metadata of the construct at `off` (when any) and wrap
/// the Markdown as a [`Hover`].
fn hover_with_identity(doc: &Document, off: usize, mut md: Option<String>) -> Option<Hover> {
    if let Some(identity) = super::identity_at(doc, off) {
        let source = format!("{:?}", identity.identity.source).to_ascii_lowercase();
        let suffix = format!(
            "**identity** `{}` — source `{source}`, stable `{}`{}",
            identity.identity.computed,
            identity.identity.stable,
            identity.component_scope.as_deref().map_or(String::new(), |scope| format!(", componentScope `{scope}`")),
        );
        if let Some(existing) = md.as_mut() {
            existing.push_str(&format!("\n\n{suffix}"));
        } else {
            md = Some(suffix);
        }
    }
    md.map(markdown)
}

fn markdown(value: String) -> Hover {
    Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value,
        }),
        range: None,
    }
}

/// The role a content line's speaker and delivery flag select (dsl 0.37.0
/// §3.4): `narration` for the narrator, `mono`/`os`/`vo` for those flags,
/// `dialogue` otherwise.
fn speaker_hover(line: &Line) -> String {
    let flag = line.attrs.iter().find_map(|a| match (a.key.as_str(), &a.value) {
        (flag @ ("mono" | "os" | "vo"), AttrValue::BoolTrue) => Some(flag),
        _ => None,
    });
    let role = match flag {
        _ if line.speaker == "narrator" => "narration",
        Some(flag) => flag,
        None => "dialogue",
    };
    format!(
        "**@{}** — role `{role}`\n\nEvery line carries a derived `voiceKey` join key; \
         `mono` is valid only for the effective POV speaker or a `monoSpeakers` member.",
        line.speaker
    )
}

/// A `::label` name: where it is declared and how many `::jump`s target it.
fn label_hover(doc: &Document, name: &str) -> String {
    let declared = super::label_decls(doc)
        .into_iter()
        .find(|(label, _)| *label == name)
        .map(|(_, span)| span.line);
    let jumps = super::jump_targets(doc)
        .into_iter()
        .filter(|(target, _)| *target == name)
        .count();
    let site = match declared {
        Some(line) => format!("declared by `::label` on line {line}"),
        None => "no `::label` in this document declares it".to_string(),
    };
    format!("**label** `{name}` — {site}; targeted by {jumps} `::jump`")
}

/// The inline modifier whose `:name` sits under `off` in a content line's
/// text (dsl 0.37.0 §3.6): the core `pause` leaf / `speed` span, or a
/// project `textStyle` member.
fn modifier_hover(doc: &Document, off: usize) -> Option<String> {
    fn find(nodes: &[InlineNode], off: usize) -> Option<&InlineModifier> {
        nodes.iter().find_map(|node| match node {
            InlineNode::Modifier(m) => {
                let name = Span {
                    byte_end: m.span.byte_start + ":".len() + m.name.len(),
                    ..m.span
                };
                if lute_resolve::cursor::span_contains(name, off) {
                    Some(m)
                } else {
                    find(&m.children, off)
                }
            }
            _ => None,
        })
    }
    let m = find(&super::line_at(doc, off)?.inline, off)?;
    Some(match m.name.as_str() {
        "pause" => "**:pause** — core leaf modifier: `:pause{s=seconds}` pauses the text \
                    for `s` seconds (required, non-negative)"
            .to_string(),
        "speed" => "**:speed** — core span modifier: `:speed[text]{rate=rate}` plays the \
                    span at `rate` (required, positive); the innermost rate wins"
            .to_string(),
        name => format!(
            "**:{name}** — text style: a member of the project `textStyle` domain; \
             `:{name}[text]`, no attributes"
        ),
    })
}

fn host_hover(raw: &str, local: usize) -> Option<String> {
    const SIGS: &[(&str, &str)] = &[
        ("holds", "holds(string, list(dyn)) -> bool"),
        ("count", "count(string, list(dyn)) -> int"),
        ("countDistinct", "countDistinct(string, list(dyn), int) -> int"),
        ("validAt", "validAt(string, list(dyn), int) -> bool"),
        ("now", "now() -> int"),
        ("visited", "visited(string) -> bool"),
        ("has", "has(select) -> bool"),
        ("int", "int(int|double) -> int"),
        ("double", "double(int|double) -> double"),
    ];
    let end = local.min(raw.len());
    for (name, sig) in SIGS {
        if raw[..end].ends_with(name)
            && raw[end..].chars().next().is_none_or(|c| !c.is_ascii_alphanumeric())
        {
            return Some(format!("**{name}** — `{sig}`"));
        }
    }
    None
}

/// Render a directive's declaration: name, layer, each attribute (type +
/// `required`), and its `semantics` vocabulary. The core `::accept` (dsl
/// 0.21.0 §7a.3) is part of the language, not of any snapshot.
fn directive_hover(snapshot: &CapabilitySnapshot, tag: &str) -> Option<String> {
    if tag == lute_syntax::ast::ACCEPT_DIRECTIVE {
        return Some(
            "**::accept** — core\n\nThe player accepts an accept-driven quest (one without \
             `start`; a subquest child only with `activate=\"accept\"`) at this point; the engine \
             activates it if it is `unset` and ignores it otherwise.\n\n**attributes:**\n\
             - `quest`: quest id (required)\n- `at`: `\"nextRun\"` — queue the acceptance until \
             after the next `newRun` reset (optional)"
                .to_string(),
        );
    }
    let decl = snapshot.directive(tag)?;
    let mut s = format!("**::{}**", decl.name);
    if let Some(layer) = &decl.layer {
        s.push_str(&format!(" — layer `{layer}`"));
    }
    if let Some(lang) = lute_check::directives::LANGUAGE_DIRECTIVES
        .iter()
        .find(|d| d.name == tag)
    {
        s.push_str(&format!("\n\n{}", lang.meaning));
    }
    if !decl.attrs.is_empty() {
        s.push_str("\n\n**attributes:**");
        for a in &decl.attrs {
            let req = if a.required { " (required)" } else { "" };
            s.push_str(&format!("\n- `{}`: {}{req}", a.name, type_label(&a.ty)));
            if let Some(def) = &a.default {
                s.push_str(&format!(" = {}", literal_label(def)));
            }
        }
    }
    if !decl.semantics.is_empty() {
        s.push_str(&format!("\n\n**semantics:** {}", decl.semantics.join(", ")));
    }
    Some(s)
}

/// Render one attribute's declaration (type, required, default). An
/// undeclared `duration`/`delay`/`wait` is the universal timing key every
/// directive but `::clear` takes (dsl 0.37.0 §3.3).
fn attr_hover(snapshot: &CapabilitySnapshot, directive: &str, key: &str) -> Option<String> {
    let decl = snapshot.directive(directive)?;
    let Some(attr) = decl.attrs.iter().find(|a| a.name == key) else {
        let (_, ty) = lute_check::directives::UNIVERSAL_TIMING_ATTRS
            .iter()
            .find(|(name, _)| *name == key && directive != CLEAR_DIRECTIVE)?;
        return Some(format!(
            "**`{key}`** (optional): {} — universal timing (seconds for \
             `duration`/`delay`)\n\non `::{directive}`",
            type_label(ty)
        ));
    };
    let req = if attr.required {
        "required"
    } else {
        "optional"
    };
    let mut s = format!("**`{key}`** ({req}): {}", type_label(&attr.ty));
    if let Some(def) = &attr.default {
        s.push_str(&format!("\n\ndefault: {}", literal_label(def)));
    }
    s.push_str(&format!("\n\non `::{directive}`"));
    Some(s)
}

/// Render the `assetId` segment under the cursor: its declared name plus type (a
/// const segment names its literal; a providerRef names its provider via
/// [`type_label`]). When the whole id decomposes against the kind, the segment's
/// current authored value is appended — the breakdown derived from the same
/// [`AssetKindDecl`] the checker uses, never a re-hardcoded vocabulary.
fn asset_segment_hover(kind: &AssetKindDecl, doc: &Document, off: usize) -> Option<String> {
    let attr = attr_at(doc, off)?;
    let AttrValue::Str(value) = &attr.value else {
        return None;
    };
    let idx = asset_segment_index(kind, value, attr.value_span.byte_start, off);
    let seg = kind.segments.get(idx)?;
    let mut s = format!("**{}**", seg.name);
    if let Some(c) = &seg.r#const {
        s.push_str(&format!(" — const `{c}`"));
    } else if let Some(ty) = &seg.ty {
        s.push_str(&format!(" — {}", type_label(ty)));
    }
    if let Ok(segs) = asset::decompose(kind, value) {
        if let Some(cur) = segs.get(idx) {
            s.push_str(&format!("\n\nvalue: `{}`", cur.value));
        }
    }
    Some(s)
}

/// Render a `state:` declaration (type + default).
fn state_hover(meta: &lute_check::TypedMeta, path: &str) -> Option<String> {
    let decl = meta.state.decls.get(path)?;
    let mut s = format!(
        "**state** `{}`: {}",
        lute_manifest::text::bracket_spelling_of(path),
        type_label(&decl.ty)
    );
    if let Some(def) = &decl.default {
        s.push_str(&format!("\n\ndefault: {}", literal_label(def)));
    }
    Some(s)
}

/// Render a `scene.choices.<id>` path when no state decl backs it (the implicit
/// branch-folded path): name the branch it resolves to.
fn choice_hover(path: &str) -> Option<String> {
    let id = choice_id(path)?;
    Some(format!(
        "**choice path** `{path}` — the chosen id of `<branch id=\"{id}\">`"
    ))
}

/// Render an `@ref` def: its CEL text, type, and any parameters.
fn ref_hover(
    meta: &lute_check::TypedMeta,
    snapshot: &CapabilitySnapshot,
    name: &str,
) -> Option<String> {
    let info = def_info(name, &meta.defs, snapshot)?;
    let mut s = format!("**@{name}**");
    if let Some(ty) = &info.ty {
        s.push_str(&format!(": {ty}"));
    }
    if !info.params.is_empty() {
        let ps = info
            .params
            .iter()
            .map(|(k, t)| format!("{k}: {t}"))
            .collect::<Vec<_>>();
        s.push_str(&format!("\n\nparams: {}", ps.join(", ")));
    }
    if !info.cel.is_empty() {
        s.push_str(&format!("\n\n```cel\n{}\n```", info.cel));
    }
    Some(s)
}

/// Render an `<on event="…">` value: whether it's a built-in lifecycle event
/// (dsl 0.2.0 §4.5) or a capability-declared world event (Plan B). `None` for
/// an unknown name — `check()` owns flagging `E-UNKNOWN-EVENT`, not this
/// best-effort fn.
fn event_hover(snapshot: &CapabilitySnapshot, event: &str) -> Option<String> {
    if lute_manifest::snapshot::BUILTIN_LIFECYCLE_EVENTS.contains(&event) {
        return Some(format!("**{event}** — built-in lifecycle event"));
    }
    snapshot
        .event(event)
        .map(|d| format!("**{}** — capability-declared world event", d.name))
}

/// Render a keyword doc for the `<quest>`/`<on>`/`<objective>` construct (dsl
/// 0.2.0 §6.3/§4/§6.4). These have no capability-schema decl (unlike
/// `::directive`), so the text is a small hardcoded blurb — always `Some`,
/// mirroring `directive_hover`'s shape but with a fixed attr-key set instead
/// of a `DirectiveDecl` lookup.
fn construct_hover(construct: QuestConstruct) -> String {
    match construct {
        QuestConstruct::Quest => {
            "**\\<quest>** — a top-level quest declaration.\n\n\
             **attributes:**\n- `id` (required): string\n- `title`: string\n\
             - `start`: cel<bool>\n- `fail`: cel<bool>\n\
             - `follows`: prereq — the quest's place in the scene graph \
             (`completed(q)`/`active(q)`/`visited(k)`); never gates activation\n\
             - `tier`: `\"user\"` (default, status persists across runs), `\"run\"` \
             (status and objectives reset when a run starts), or \
             `\"season:<name>\"` (reset when that declared season opens again)\n\
             - `activate`: `\"accept\"` — a subquest child waits for `::accept` instead of \
             activating with its parent\n\
             - `complete`: `\"all\"` (default) or `\"any\"` — `any` completes on the first \
             required child done and fails the still-open rest as `superseded`\n\
             - `rearm`: cel<bool> — each time it turns from false to true the quest \
             returns to `unset` and can be taken up again"
                .to_string()
        }
        QuestConstruct::On => {
            "**\\<on>** — an ECA trigger fired by a lifecycle or \
             capability-declared world event.\n\n\
             **attributes:**\n- `event` (required): string\n- `when`: cel<bool>\n\
             - `target`: dotted id — fires only when the same-named occasion is raised \
             for this target"
                .to_string()
        }
        QuestConstruct::Objective => {
            "**\\<objective>** — a quest objective; \
             self-closing or with a body.\n\n\
             **attributes:**\n- `id` (required): string\n- `done`: cel<bool> (required unless `quest`)\n\
             - `quest`: string — a child quest whose completion completes this objective\n\
             - `visibleWhen`: cel<bool> — hides the objective while false; never gates `done`\n\
             - `title`: string\n- `optional`: bool\n\
             - `on`: ident — the occasion that judges this objective\n\
             - `target`: dotted id — narrows `on` to the occasion raised for this target\n\
             - `by`: cel<bool> — deadline, judged at every settle; `done` wins a tie\n\
             - `until`: cel<bool> — place-bound deadline, judged only when `on` \
             (and `target`) is raised, after `done` (requires `on`)"
                .to_string()
        }
        QuestConstruct::Entry => {
            "**\\<entry>** — a lore entry: text the engine looks up rather than \
             plays. Reading it the first time applies its \
             `::set`/`::assert`/`::retract` and sets `entry.<id>.read` (run tier) and \
             `entry.<id>.everRead` (user tier).\n\n\
             **attributes:**\n\
             - `id` (required): ident — unique across the project\n\
             - `target`: dotted id — the engine-owned thing it is attached to (`item.rusty_key`)\n\
             - `category`: ident — what kind of text it is (`note`, `item`, `codex`, …)\n\
             - `title`: string — display title, localized\n\
             - `series`: ident — groups multi-part text\n\
             - `order`: non-negative integer — position within `series` (requires `series`)\n\
             - `when`: cel<bool> — eligibility; presented only while it holds\n\
             - `on`: ident — the occasion this entry answers as a beat\n\
             - `priority`: integer — beat priority, higher wins (requires `on`)\n\
             - `once`: `\"run\"`, `\"user\"`, `\"day\"`, `\"week\"`, `\"slot\"`, or \
             `\"season:<name>\"` — not eligible once read this run / ever / this clock day / \
             this clock week / this clock slot / until the season opens again (requires `on`; \
             absent = repeatable; `day`/`slot` need a declared `clock:`, `week` its `week:`)\n\
             - `spentBy`: cel<bool> — spends the beat instead of a presentation: once the \
             condition has held it stays spent for its `once` period (`run` unless written)\n\
             - `for`: `\"kind:<kind>\"` — on an untargeted `select: sequence` occasion, \
             read once per member whose `when` holds, binding `occasion.target`"
                .to_string()
        }
        QuestConstruct::Beat => {
            "**\\<beat>** — a beat bundle: a scene-like beat declared inside a lore \
             document. Its body is a scene section body; presenting it \
             records `<document id>.<id>` in `visited`, and `scene.*` is fresh per \
             presentation.\n\n\
             **attributes:**\n\
             - `id` (required): ident — canonical id `<document id>.<id>`\n\
             - `on` (required): ident — the occasion this beat answers\n\
             - `target`: dotted id — the thing the occasion is judged against\n\
             - `title`: string — menu label, localized\n\
             - `when`: cel<bool> — eligibility; may not read `scene.*`\n\
             - `priority`: integer — beat priority, higher wins (default 0)\n\
             - `once`: `\"run\"` (default), `\"user\"`, `\"false\"`, `\"day\"`, `\"week\"`, \
             `\"slot\"`, or `\"season:<name>\"` — spent once presented this run / ever / never / \
             this clock day / this clock week / this clock slot / until the season opens again \
             (`day`/`slot` need a declared `clock:`, `week` its `week:`)\n\
             - `spentBy`: cel<bool> — spends the beat instead of a presentation: once the \
             condition has held it stays spent for its `once` period (`run` unless written)\n\
             - `also`: bool flag — on a `select: first` occasion, presented after \
             the winner in addition to it\n\
             - `use`: component — a beat template: the component's \
             `beat:` header supplies every attribute this `<beat>` does not write, its \
             body runs first, and every other attribute is one of its params\n\
             - `for`: `\"kind:<kind>\"` — on an untargeted `select: sequence` occasion, \
             presented once per member whose `when` holds, in member order, binding \
             `occasion.target`"
                .to_string()
        }
        QuestConstruct::Hub => {
            "**\\<hub>** — a revisit conversation: its options are re-presented \
             until an `exit` option is taken or none stays eligible.\n\n\
             **attributes:**\n\
             - `id` (required): ident — records `scene.choices.<id>` and \
             `scene.visited.<id>.<choice>`\n\
             - `prompt`: non-empty string — the prompt line shown with the \
             hub's options"
                .to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lute_manifest::core::load_core_snapshot;
    use lute_syntax::parse;

    /// Text of a `Markup` hover (the only variant `hover_at` emits).
    fn contents_text(h: &Hover) -> &str {
        match &h.contents {
            HoverContents::Markup(m) => &m.value,
            _ => panic!("expected markup hover"),
        }
    }

    fn parsed(text: &str) -> Document {
        parse(text).0
    }

    /// Byte offset just inside `needle` (on its first char) within `text`.
    fn pos_on(text: &str, needle: &str) -> usize {
        text.find(needle).expect("needle present") + 1
    }

    #[test]
    fn hover_on_line_exposes_identity_metadata() {
        let text = "## Shot 1.\n@narrator{code=\"intro\"}: Hello\n";
        let doc = parsed(text);
        let off = pos_on(text, "intro");
        let hover = hover_at(&doc, &load_core_snapshot(), &SchemaImports::default(), off).unwrap();
        let rendered = contents_text(&hover);
        assert!(rendered.contains("identity"), "{rendered}");
        assert!(rendered.contains("authored"), "{rendered}");
        assert!(rendered.contains("stable `true`"), "{rendered}");
    }

    const WITH_DEF_FOND: &str = "---\nkind: scene\ncharacter: marina\nseason: 1\nepisode: 2\nstate:\n  scene.affect.marina: { type: int, default: 0 }\ndefs:\n  fond: { type: bool, cel: \"scene.affect.marina >= 1\" }\n---\n## Shot 1.\n<match subject=\"scene.affect.marina\">\n  <when test=\"@fond\">\n    @fixer: gently.\n  </when>\n  <otherwise>\n    @fixer: bluntly.\n  </otherwise>\n</match>\n";

    #[test]
    fn hover_on_ref_shows_def_cel() {
        let doc = parsed(WITH_DEF_FOND);
        let off = pos_on(WITH_DEF_FOND, "@fond");
        let h = hover_at(&doc, &load_core_snapshot(), &SchemaImports::default(), off).unwrap();
        assert!(contents_text(&h).contains("scene.affect.marina >= 1"));
    }

    #[test]
    fn hover_on_ref_shows_def_type() {
        let doc = parsed(WITH_DEF_FOND);
        let off = pos_on(WITH_DEF_FOND, "@fond");
        let h = hover_at(&doc, &load_core_snapshot(), &SchemaImports::default(), off).unwrap();
        assert!(contents_text(&h).contains("bool"));
    }

    #[test]
    fn hover_on_directive_name_shows_attrs() {
        let text = "## Shot 1.\n::camera{focus=\"marina\"}\n";
        let doc = parsed(text);
        let off = pos_on(text, "camera");
        let h = hover_at(&doc, &load_core_snapshot(), &SchemaImports::default(), off).unwrap();
        let s = contents_text(&h);
        assert!(s.contains("::camera"), "names the directive: {s}");
        assert!(s.contains("focus"), "lists an attribute: {s}");
    }

    #[test]
    fn hover_on_state_path_shows_type_and_default() {
        let text = "---\nkind: scene\ncharacter: marina\nseason: 1\nepisode: 2\nstate:\n  scene.affect.marina: { type: int, default: 3 }\n---\n## Shot 1.\n::set{scene.affect.marina += 1}\n";
        let doc = parsed(text);
        // Cursor on the `::set` target path (first occurrence in the body).
        let body_start = text.find("::set{").unwrap();
        let off = text[body_start..].find("scene.affect.marina").unwrap() + body_start + 2;
        let h = hover_at(&doc, &load_core_snapshot(), &SchemaImports::default(), off).unwrap();
        let s = contents_text(&h);
        assert!(s.contains("int"), "shows the type: {s}");
        assert!(s.contains("3"), "shows the default: {s}");
    }

    /// dsl 0.24.0 §1: a state path inside a `::set{… when="…"}` guard
    /// hovers like one in the expression.
    #[test]
    fn hover_on_state_path_in_set_guard() {
        let text = "---\nkind: scene\ncharacter: marina\nseason: 1\nepisode: 2\nstate:\n  scene.a: { type: int, default: 0 }\n  scene.gate: { type: bool, default: false }\n---\n## Shot 1.\n::set{scene.a += 1 when=\"scene.gate\"}\n";
        let doc = parsed(text);
        let off = pos_on(text, "scene.gate\"}");
        let h = hover_at(&doc, &load_core_snapshot(), &SchemaImports::default(), off).unwrap();
        let s = contents_text(&h);
        assert!(s.contains("bool"), "shows the guard path's type: {s}");
    }

    #[test]
    fn hover_on_enum_attr_value_shows_domain() {
        let text = "## Shot 1.\n::actor{character=\"b\" anchor=\"center\"}\n";
        let doc = parsed(text);
        let off = pos_on(text, "center");
        let h = hover_at(
            &doc,
            &lute_test_vocab::vocab_snapshot(),
            &SchemaImports::default(),
            off,
        )
        .unwrap();
        let s = contents_text(&h);
        assert!(
            s.contains("left") && s.contains("right"),
            "shows enum domain: {s}"
        );
    }

    /// The declaration-site twin of the test above: with the CORE snapshot (dsl
    /// 0.9.0 D-A — it ships NO members), `anchor` can only resolve through the
    /// document's OWN inline `enums:` projection. Hover goes through the same
    /// `merge_domains` seam `check()` does, so an inline declaration documents
    /// itself exactly as a plugin/imported one does.
    #[test]
    fn hover_on_inline_declared_domain_shows_its_members() {
        let text = "---\nkind: scene\ncharacter: marina\nseason: 1\nepisode: 2\n\
                    enums:\n  anchor:\n    members: [portside, midships, starboard]\n    \
                    default: midships\n---\n## Shot 1.\n\
                    ::actor{character=\"b\" anchor=\"midships\"}\n";
        let doc = parsed(text);
        let off = pos_on(text, "\"midships\"") + 1;
        let h = hover_at(&doc, &load_core_snapshot(), &SchemaImports::default(), off)
            .expect("an inline `enums:` declaration must document its own attr value");
        let s = contents_text(&h);
        assert!(
            s.contains("portside") && s.contains("starboard"),
            "shows the INLINE domain: {s}"
        );
    }

    #[test]
    fn hover_on_unknown_ref_is_none() {
        let text = "## Shot 1.\n::set{scene.x = @nope}\n";
        let doc = parsed(text);
        let off = pos_on(text, "@nope");
        assert!(hover_at(&doc, &load_core_snapshot(), &SchemaImports::default(), off).is_none());
    }

    /// A snapshot with a `CH` assetKind (plugin §6.9 shape) and a `::portrait`
    /// directive whose `assetId` attr is typed `assetKind("CH")`.
    fn asset_snapshot() -> CapabilitySnapshot {
        use lute_manifest::schema::{
            AssetKindDecl, AssetResolve, AssetSegment, AttrDecl, DirectiveDecl, Lowering,
        };
        use lute_manifest::types::Type;
        let ch = AssetKindDecl {
            kind: "CH".to_string(),
            sep: ".".to_string(),
            resolve: AssetResolve::Compose,
            segments: vec![
                AssetSegment {
                    name: "prefix".to_string(),
                    r#const: Some("CH".to_string()),
                    ty: None,
                },
                AssetSegment {
                    name: "characterId".to_string(),
                    r#const: None,
                    ty: Some(Type::ProviderRef("character".to_string())),
                },
                AssetSegment {
                    name: "costume".to_string(),
                    r#const: None,
                    ty: Some(Type::Str),
                },
                AssetSegment {
                    name: "emotion".to_string(),
                    r#const: None,
                    ty: Some(Type::Enum(vec![
                        "delighted".to_string(),
                        "content".to_string(),
                        "neutral".to_string(),
                    ])),
                },
                AssetSegment {
                    name: "variant".to_string(),
                    r#const: None,
                    ty: Some(Type::Int),
                },
            ],
            provider: None,
            match_: Vec::new(),
            aliases: std::collections::BTreeMap::new(),
            fallback: Vec::new(),
            persistence: None,
        };
        let portrait = DirectiveDecl {
            name: "portrait".to_string(),
            layer: None,
            attrs: vec![AttrDecl {
                name: "assetId".to_string(),
                required: true,
                ty: Type::AssetKind("CH".to_string()),
                default: None,
            }],
            semantics: Vec::new(),
            state: None,
            effects: None,
            bridge: None,
            lower: Lowering::Builtin {
                kind: "builtin".to_string(),
                name: "portrait".to_string(),
            },
        };
        let mut snap = CapabilitySnapshot::default();
        snap.asset_kinds.insert("CH".to_string(), ch);
        snap.directives.insert("portrait".to_string(), portrait);
        snap
    }

    #[test]
    #[allow(non_snake_case)] // brief-specified name mirrors the `characterId` segment
    fn hover_characterId_segment() {
        // Cursor on `marina` → segment idx 1 (characterId, providerRef character).
        let text = "## Shot 1.\n::portrait{assetId=\"CH.marina.waitress.delighted.3\"}\n";
        let doc = parsed(text);
        let off = pos_on(text, "marina");
        let h = hover_at(&doc, &asset_snapshot(), &SchemaImports::default(), off).unwrap();
        let s = contents_text(&h);
        assert!(s.contains("characterId"), "names the segment: {s}");
        assert!(
            s.contains("providerRef(character)"),
            "names the provider type: {s}"
        );
    }

    /// A directly-constructed `SchemaImports` (no disk): an imported `run.gold`
    /// state path and an imported `helped` def — exactly the shape `check()` sees
    /// after resolving a scene's `uses:` schema (dsl §9.2).
    fn schema_imports() -> lute_check::SchemaImports {
        use lute_check::{Namespace, SchemaImports, StateDecl};
        use lute_manifest::types::Type;
        let mut imports = SchemaImports::default();
        imports.state.decls.insert(
            "run.gold".to_string(),
            StateDecl {
                ty: Type::Int,
                default: None,
                namespace: Namespace::Run,
                owner: None,
            },
        );
        imports.defs.insert(
            "helped".to_string(),
            serde_yaml::from_str("{ type: bool, cel: \"true\" }").unwrap(),
        );
        imports
    }

    #[test]
    fn hover_on_imported_state_path_shows_type() {
        // `run.gold` is NOT declared inline — it is only imported via `uses:`.
        let text = "---\nkind: scene\ncharacter: marina\nseason: 1\nepisode: 2\n---\n## Shot 1.\n::set{run.gold += 1}\n";
        let doc = parsed(text);
        let set_at = text.find("::set{").unwrap();
        let off = text[set_at..].find("run.gold").unwrap() + set_at + 2;
        let h = hover_at(&doc, &load_core_snapshot(), &schema_imports(), off).unwrap();
        let s = contents_text(&h);
        assert!(s.contains("run.gold"), "names the imported path: {s}");
        assert!(s.contains("int"), "shows the imported type: {s}");
    }

    #[test]
    fn hover_on_imported_ref_shows_def() {
        // `@helped` is NOT declared inline — it is only imported via `uses:`.
        let text = "---\nkind: scene\ncharacter: marina\nseason: 1\nepisode: 2\n---\n## Shot 1.\n<match subject=\"scene.affect.marina\">\n  <when test=\"@helped\">\n    @fixer: yes.\n  </when>\n  <otherwise>\n    @fixer: no.\n  </otherwise>\n</match>\n";
        let doc = parsed(text);
        let off = pos_on(text, "@helped");
        let h = hover_at(&doc, &load_core_snapshot(), &schema_imports(), off).unwrap();
        let s = contents_text(&h);
        assert!(s.contains("@helped"), "names the imported def: {s}");
        assert!(s.contains("bool"), "shows the imported def type: {s}");
    }

    #[test]
    fn imported_state_wins_over_inline_on_collision() {
        // Inline `state:` declares `run.gold` as string; the imported schema
        // (via `uses:`) declares the SAME path as number. `check()` makes
        // imported state authoritative on collision (E-STATE-REDECLARE), so the
        // feature merge must mirror that: hover reflects the IMPORTED type
        // (number), not the inline string.
        let text = "---\nkind: scene\ncharacter: marina\nseason: 1\nepisode: 2\nstate:\n  run.gold: { type: string }\n---\n## Shot 1.\n::set{run.gold += 1}\n";
        let doc = parsed(text);
        let set_at = text.find("::set{").unwrap();
        let off = text[set_at..].find("run.gold").unwrap() + set_at + 2;
        let h = hover_at(&doc, &load_core_snapshot(), &schema_imports(), off).unwrap();
        let s = contents_text(&h);
        assert!(s.contains("run.gold"), "names the path: {s}");
        assert!(
            s.contains("int"),
            "imported type (int) wins over inline (string): {s}"
        );
        assert!(
            !s.contains("string"),
            "inline type must not survive collision: {s}"
        );
    }

    /// A content line with all three interp kinds (dsl §7.6): a reserved token,
    /// a state path, and an `@ref` — over a doc that declares `run.coins` + `@fond`.
    const WITH_INTERPS: &str = "---\ncharacter: marina\nseason: 1\nepisode: 2\nstate:\n  run.coins: { type: int, default: 0 }\ndefs:\n  fond: { type: bool, cel: \"run.coins >= 1\" }\n---\n## Shot 1.\n@marina: Hi {{userName}}, {{run.coins}} — {{@fond}}.\n";

    /// Byte offset just inside the referent of the `{{whole}}` interp in `text`.
    fn interp_off(text: &str, whole: &str) -> usize {
        text.find(whole).expect("interp present") + 2
    }

    /// D1: hover inside `{{run.coins}}` renders the SAME state decl (type +
    /// default) as hovering `run.coins` in a `::set`/CEL slot.
    #[test]
    fn hover_on_interp_path_shows_state_decl() {
        let doc = parsed(WITH_INTERPS);
        let off = interp_off(WITH_INTERPS, "{{run.coins}}");
        let h = hover_at(&doc, &load_core_snapshot(), &SchemaImports::default(), off).unwrap();
        let s = contents_text(&h);
        assert!(s.contains("run.coins"), "names the path: {s}");
        assert!(s.contains("int"), "shows the state type: {s}");
    }

    /// D1: hover inside `{{@fond}}` renders the def hover (type + CEL).
    #[test]
    fn hover_on_interp_ref_shows_def() {
        let doc = parsed(WITH_INTERPS);
        let off = interp_off(WITH_INTERPS, "{{@fond}}");
        let h = hover_at(&doc, &load_core_snapshot(), &SchemaImports::default(), off).unwrap();
        let s = contents_text(&h);
        assert!(s.contains("fond"), "names the def: {s}");
        assert!(s.contains("bool"), "shows the def type: {s}");
    }

    /// D1: hover inside `{{userName}}` (Reserved) surfaces a one-line note, not a
    /// decl lookup.
    #[test]
    fn hover_on_interp_reserved_shows_note() {
        let doc = parsed(WITH_INTERPS);
        let off = interp_off(WITH_INTERPS, "{{userName}}");
        let h = hover_at(&doc, &load_core_snapshot(), &SchemaImports::default(), off).unwrap();
        let s = contents_text(&h);
        assert!(s.contains("reserved"), "notes the reserved token: {s}");
    }

    /// D3: hover on a `<when is="gold">` value whose `<match>` subject is a
    /// declared enum shows the subject's finite domain incl. `unset` (dsl §7.3.1)
    /// — the `is=` literal pattern menu, sourced from the same fold the checker
    /// uses. Pre-D3 the `is` value was discarded (no hover).
    #[test]
    fn hover_on_when_is_shows_enum_domain() {
        let text = "---\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  scene.serve.debut.rank: { type: { enum: [gold, silver, bronze] } }\n---\n## Shot 1.\n<match subject=\"scene.serve.debut.rank\">\n<when is=\"gold\">\n@fixer: nice.\n</when>\n<otherwise>\n@fixer: ok.\n</otherwise>\n</match>\n";
        let doc = parsed(text);
        let off = text.find("is=\"gold\"").unwrap() + "is=\"".len() + 1; // inside "gold"
        let h = hover_at(&doc, &load_core_snapshot(), &SchemaImports::default(), off).unwrap();
        let s = contents_text(&h);
        assert!(
            s.contains("gold")
                && s.contains("silver")
                && s.contains("bronze")
                && s.contains("unset"),
            "hover shows the enum domain ∪ unset: {s}"
        );
    }

    /// dsl 0.18.0 §2: hover on a `<when is="…">` range literal whose subject is
    /// a declared `number` says the domain is numeric and that ranges are
    /// inclusive — there is no finite member menu to list.
    #[test]
    fn hover_on_when_is_range_shows_number_domain() {
        let text = "---\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  scene.hp: { type: int }\n---\n## Shot 1.\n<match subject=\"scene.hp\">\n<when is=\"..0 | 5..\">\n@fixer: edge.\n</when>\n<otherwise>\n@fixer: ok.\n</otherwise>\n</match>\n";
        let doc = parsed(text);
        let off = text.find("is=\"..0").unwrap() + "is=\"".len() + 1; // inside "..0"
        let h = hover_at(&doc, &load_core_snapshot(), &SchemaImports::default(), off).unwrap();
        let s = contents_text(&h);
        assert!(
            s.contains("int or double") && s.contains("inclusive"),
            "hover names the numeric domain and inclusive ranges: {s}"
        );
    }

    // ---- dsl 0.2.0 §4/§6.3/§6.4: quest / on / objective hover ----

    const QUEST_DOC: &str =
        "---\nkind: quest\nstate:\n  run.d: { type: bool, default: false }\n---\n\
        <quest id=\"q\">\n\
        <objective id=\"o\" done=\"run.d\">\n</objective>\n\
        <on event=\"questComplete\">\n</on>\n\
        </quest>\n";

    #[test]
    fn hover_on_on_construct_renders_a_doc() {
        let doc = parsed(QUEST_DOC);
        let off = QUEST_DOC.find("<on ").unwrap() + 1;
        let h = hover_at(&doc, &load_core_snapshot(), &SchemaImports::default(), off).unwrap();
        assert!(contents_text(&h).contains("on"), "{}", contents_text(&h));
    }

    #[test]
    fn hover_on_objective_construct_renders_a_doc() {
        let doc = parsed(QUEST_DOC);
        let off = QUEST_DOC.find("<objective ").unwrap() + 1;
        let h = hover_at(&doc, &load_core_snapshot(), &SchemaImports::default(), off).unwrap();
        assert!(
            contents_text(&h).contains("objective"),
            "{}",
            contents_text(&h)
        );
    }

    #[test]
    fn hover_on_quest_construct_renders_a_doc() {
        let doc = parsed(QUEST_DOC);
        let off = QUEST_DOC.find("<quest ").unwrap() + 1;
        let h = hover_at(&doc, &load_core_snapshot(), &SchemaImports::default(), off).unwrap();
        assert!(contents_text(&h).contains("quest"), "{}", contents_text(&h));
    }

    #[test]
    fn hover_on_builtin_event_value_names_it_lifecycle() {
        let doc = parsed(QUEST_DOC);
        let off = pos_on(QUEST_DOC, "questComplete");
        let h = hover_at(&doc, &load_core_snapshot(), &SchemaImports::default(), off).unwrap();
        let s = contents_text(&h);
        assert!(s.contains("questComplete"), "{s}");
        assert!(s.contains("lifecycle"), "{s}");
    }

    #[test]
    fn hover_on_objective_done_still_resolves_the_state_decl() {
        // The `done=` CEL slot flows through the EXISTING `Cursor::Cel` path
        // (unchanged) — a regression check that quest resolution didn't
        // break the ordinary state-path hover for content INSIDE a quest.
        let doc = parsed(QUEST_DOC);
        let off = QUEST_DOC.find("done=\"run.d\"").unwrap() + "done=\"".len() + 1;
        let h = hover_at(&doc, &load_core_snapshot(), &SchemaImports::default(), off).unwrap();
        let s = contents_text(&h);
        assert!(s.contains("run.d"), "{s}");
        assert!(s.contains("bool"), "{s}");
    }

    // ---- dsl 0.19.0 §3: entry hover ----

    #[test]
    fn hover_on_entry_construct_explains_its_attrs() {
        let text =
            "---\nkind: lore\n---\n<entry id=\"e\" target=\"item.key\">\n@narrator: hi\n</entry>\n";
        let doc = parsed(text);
        let off = text.find("<entry ").unwrap() + 1;
        let h = hover_at(&doc, &load_core_snapshot(), &SchemaImports::default(), off).unwrap();
        let s = contents_text(&h);
        for k in [
            "entry", "target", "category", "series", "order", "when", "on", "priority", "once",
        ] {
            assert!(s.contains(k), "missing {k}: {s}");
        }
    }

    /// Hover text is author-facing: it cites no spec sections.
    #[test]
    fn hover_text_cites_no_spec_sections() {
        let mut texts: Vec<String> = [
            QuestConstruct::Quest,
            QuestConstruct::On,
            QuestConstruct::Objective,
            QuestConstruct::Entry,
            QuestConstruct::Beat,
            QuestConstruct::Hub,
        ]
        .into_iter()
        .map(construct_hover)
        .collect();
        let snap = load_core_snapshot();
        texts.extend(directive_hover(&snap, lute_syntax::ast::ACCEPT_DIRECTIVE));
        texts.extend(event_hover(&snap, "questComplete"));
        for s in &texts {
            assert!(
                !s.contains('§') && !s.contains("dsl 0.") && !s.contains("Appendix"),
                "{s}"
            );
        }
    }

    fn hover_text(text: &str, off: usize) -> Option<String> {
        hover_at(&parsed(text), &load_core_snapshot(), &SchemaImports::default(), off)
            .map(|h| contents_text(&h).to_string())
    }

    /// dsl 0.37.0 §3.5: a `::jump{to}` value names its label, where the label
    /// is declared and how many jumps target it.
    #[test]
    fn hover_on_jump_target_describes_the_label() {
        let text = "## Shot 1.\n::jump{to=\"detail\"}\n@fixer: skipped.\n::label{name=\"detail\"}\n@fixer: here.\n";
        let md = hover_text(text, text.find("detail").unwrap() + 1).unwrap();
        assert!(md.contains("**label** `detail`"), "{md}");
        assert!(md.contains("line 4"), "{md}");
        assert!(md.contains("targeted by 1 `::jump`"), "{md}");
    }

    /// The new core directives render their snapshot attrs (`::cg` `assetId`,
    /// `display`, `layout`; `::sequence` `name`) and the language meaning of
    /// a control-flow directive.
    #[test]
    fn hover_on_new_directives_lists_snapshot_attrs() {
        let text = "## Shot 1.\n::cg{assetId=\"cg.one\"}\n::sequence{name=\"intro\"}\n::jump{to=\"x\"}\n::label{name=\"x\"}\n";
        let cg = hover_text(text, text.find("::cg").unwrap() + 2).unwrap();
        for attr in ["`assetId`", "`display`", "`layout`"] {
            assert!(cg.contains(attr), "{cg}");
        }
        let seq = hover_text(text, text.find("::sequence").unwrap() + 2).unwrap();
        assert!(seq.contains("`name`: domain `sequence`") || seq.contains("`name`"), "{seq}");
        let jump = hover_text(text, text.find("::jump").unwrap() + 2).unwrap();
        assert!(jump.contains("jump forward"), "{jump}");
    }

    /// Camera domain attrs and the music `playback` key hover their declared
    /// domain; an undeclared universal timing key hovers as such.
    #[test]
    fn hover_on_camera_and_music_attr_keys() {
        let text = "## Shot 1.\n::camera{framing=\"close\" duration=\"1\"}\n::music{playback=\"start\"}\n::cg{assetId=\"a\" delay=\"1\"}\n";
        let framing = hover_text(text, text.find("framing").unwrap() + 1).unwrap();
        assert!(framing.contains("**`framing`**") && framing.contains("framing"), "{framing}");
        let playback = hover_text(text, text.find("playback").unwrap() + 1).unwrap();
        assert!(playback.contains("musicPlayback"), "{playback}");
        let delay = hover_text(text, text.find("delay").unwrap() + 1).unwrap();
        assert!(delay.contains("universal timing"), "{delay}");
    }

    /// dsl 0.37.0 §3.4: a delivery flag hovers its role; the speaker hovers
    /// the line's role.
    #[test]
    fn hover_on_line_role_flag_and_speaker() {
        let text = "## Shot 1.\n@mira{os code=\"c1\"}: Hello.\n@narrator: Prose.\n";
        let flag = hover_text(text, text.find("{os").unwrap() + 2).unwrap();
        assert!(flag.contains("role `os`"), "{flag}");
        let code = hover_text(text, text.find("code").unwrap() + 1).unwrap();
        assert!(code.contains("voiceKey"), "{code}");
        let speaker = hover_text(text, text.find("mira").unwrap() + 1).unwrap();
        assert!(speaker.contains("role `os`"), "{speaker}");
        let narrator = hover_text(text, text.find("narrator").unwrap() + 1).unwrap();
        assert!(narrator.contains("role `narration`"), "{narrator}");
    }

    /// dsl 0.37.0 §3.6: an inline modifier name hovers its meaning.
    #[test]
    fn hover_on_inline_modifier_name() {
        let text = "## Shot 1.\n@mira: Wait :pause{s=0.5} :emphasis[now].\n";
        let pause = hover_text(text, text.find(":pause").unwrap() + 2).unwrap();
        assert!(pause.contains("**:pause**"), "{pause}");
        let style = hover_text(text, text.find(":emphasis").unwrap() + 3).unwrap();
        assert!(style.contains("textStyle"), "{style}");
    }
}
