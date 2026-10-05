use std::collections::BTreeMap;
use lute_cel::CelArena;
use lute_core_span::Span;
use lute_syntax::ast::Document;
use crate::cel_paths::collect_path_uses;

/// dsl 0.24.0 (T1-6): a scene beat's frontmatter `when:` (a scene with
/// `on:`) as a slot over its inline value's source span — the one condition
/// slot [`lute_syntax::walk::for_each_cel_slot`] cannot see (the frontmatter
/// is YAML, lifted by the checker, not the parser).
fn scene_when_slot(doc: &Document, meta: &crate::meta::TypedMeta) -> Option<lute_syntax::ast::CelSlot> {
    let map = meta.yaml()?.as_mapping()?;
    let key = |k: &str| map.get(serde_yaml::Value::String(k.to_string()));
    key("on")?;
    let raw = key("when")?.as_str().filter(|w| !w.trim().is_empty())?;
    Some(lute_syntax::ast::CelSlot::raw(
        lute_syntax::ast::CelKind::Condition,
        raw.to_string(),
        crate::beats::top_value_span(&doc.meta, "when"),
    ))
}

/// Every path `doc` references that `keep` admits, paired with where it was
/// first found: the scene beat's frontmatter `when:` first (source order),
/// narrowed to the referenced id (`id_at` gives its `(offset, len)` within
/// the path) when the path appears verbatim in the value, then the enclosing
/// slot of each body read in canonical [`lute_syntax::walk::for_each_cel_slot`]
/// order. Each slot is re-parsed fresh; one that fails to parse contributes
/// nothing (the normal CEL-parse pass reports it).
pub(crate) fn referenced_paths(
    doc: &Document,
    meta: &crate::meta::TypedMeta,
    keep: impl Fn(&str) -> bool,
    id_at: impl Fn(&str) -> Option<(usize, usize)>,
) -> BTreeMap<String, Span> {
    let mut out = BTreeMap::new();
    let mut visit = |slot: &lute_syntax::ast::CelSlot, frontmatter: bool| {
        let raw = slot.raw.trim();
        if raw.is_empty() {
            return;
        }
        let mut arena = CelArena::default();
        let Ok(handle) = lute_cel::parse_slot(&mut arena, raw, 0) else {
            return;
        };
        let Some(rec) = arena.get(handle) else {
            return;
        };
        for use_ in collect_path_uses(&rec.expr) {
            if !keep(&use_.path) {
                continue;
            }
            let span = match (frontmatter, slot.raw.find(&use_.path), id_at(&use_.path)) {
                (true, Some(at), Some((off, len))) => {
                    let start = slot.span.byte_start + at + off;
                    Span {
                        byte_start: start,
                        byte_end: start + len,
                        line: 0,
                        column: 0,
                        utf16_range: (0, 0),
                    }
                }
                _ => slot.span,
            };
            out.entry(use_.path).or_insert(span);
        }
    };
    if let Some(slot) = scene_when_slot(doc, meta) {
        visit(&slot, true);
    }
    lute_syntax::walk::for_each_cel_slot(doc, &mut |slot| visit(slot, false));
    out
}

/// ` — did you mean `x`?` over `known`, or nothing when none is close. An
/// id with a `.` is refused where it is declared (`E-PATH-IDENT`), so it is
/// never the spelling to read: when it is the close one, the hint says to
/// rename it.
pub(crate) fn did_you_mean<'a>(id: &str, known: impl Iterator<Item = &'a str>) -> String {
    match lute_manifest::suggest::nearest(id, known, 2) {
        Some(near) if near.contains('.') => format!(
            " — `{near}` is close, but a `.` makes it no id: rename it (for example `{}`)",
            near.split('.')
                .enumerate()
                .map(|(i, s)| match (i, s.chars().next()) {
                    (0, _) | (_, None) => s.to_string(),
                    (_, Some(f)) => f.to_uppercase().chain(s.chars().skip(1)).collect(),
                })
                .collect::<String>()
        ),
        Some(near) => format!(" — did you mean `{near}`?"),
        None => String::new(),
    }
}
