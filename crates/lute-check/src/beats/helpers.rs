use super::*;

/// The span of a top-level frontmatter key's inline VALUE — its text on the
/// key's own line, minus surrounding quotes and a trailing comment — so a CEL
/// slot's raw offsets map onto the source. Falls back to the key span for a
/// block value (nothing inline).
pub(crate) fn top_value_span(meta: &Meta, key: &str) -> Span {
    let Some((at, after)) = top_key_line(meta, key) else {
        return crate::meta::meta_key_span(meta, key);
    };
    let text = after.trim_end_matches(['\n', '\r']);
    let lead = text.len() - text.trim_start().len();
    let value = text.trim_start();
    let (start, len) = match value.chars().next() {
        Some(q @ ('\'' | '"')) => match value[1..].rfind(q) {
            Some(close) => (lead + 1, close),
            None => (lead, value.trim_end().len()),
        },
        _ => {
            let plain = value.find(" #").map_or(value, |c| &value[..c]).trim_end();
            (lead, plain.len())
        }
    };
    if len == 0 {
        return top_key_span(meta, key);
    }
    // `at` + the key + the colon (and any space before it) + the offset.
    let colon = meta.raw_yaml[at..].find(':').unwrap_or(key.len());
    let begin = interior_base(meta) + at + colon + 1 + start;
    bare_span(begin, begin + len)
}

/// The span of a NESTED frontmatter key's inline scalar value (`path` from
/// the top level, e.g. `["terminal", "when"]`, in a block or one-line flow
/// mapping) — minus surrounding quotes, a trailing comment and, in a flow
/// mapping, the `,`/`}` after it. Falls back to [`crate::meta::meta_path_span`].
pub(crate) fn nested_value_span(meta: &Meta, path: &[&str]) -> Span {
    let authored = crate::chapters::authored_yaml(&meta.raw_yaml);
    let fallback = || crate::meta::meta_path_span(meta, path);
    let Some(key) = lute_manifest::yaml_text::key_span(authored, path) else {
        return fallback();
    };
    let rest = &authored[key.end..];
    let Some(after) = rest.trim_start_matches([' ', '\t']).strip_prefix(':') else {
        return fallback();
    };
    let colon_end = key.end + (rest.len() - after.len());
    let lead = after.len() - after.trim_start_matches([' ', '\t']).len();
    let value = &after[lead..];
    let (start, len) = match value.chars().next() {
        Some(q @ ('\'' | '"')) => {
            let body = &value[1..];
            let close = body
                .char_indices()
                .find(|&(i, c)| c == q && (q == '\'' || !body[..i].ends_with('\\')));
            match close {
                Some((i, _)) => (lead + 1, i),
                None => return fallback(),
            }
        }
        _ => {
            let end = value.find(['\n', ',', '}', '#']).unwrap_or(value.len());
            (lead, value[..end].trim_end().len())
        }
    };
    if len == 0 {
        return fallback();
    }
    let begin = interior_base(meta) + colon_end + start;
    bare_span(begin, begin + len)
}

/// A top-level frontmatter key's inline value text (unquoted) and its
/// [`top_value_span`]; `None` for an absent key or a block value.
pub(crate) fn top_value_text<'m>(meta: &'m Meta, key: &str) -> Option<(&'m str, Span)> {
    let (at, _) = top_key_line(meta, key)?;
    let span = top_value_span(meta, key);
    let base = interior_base(meta);
    if span.byte_start == base + at {
        return None;
    }
    let text = meta
        .raw_yaml
        .get(span.byte_start.checked_sub(base)?..span.byte_end.checked_sub(base)?)?;
    Some((text, span))
}

pub(super) fn beat_diag(
    code: &str,
    severity: Severity,
    message: String,
    span: Span,
    layer: Layer,
) -> Diagnostic {
    let evidence = match crate::evidence::classification(code) {
        Some(crate::evidence::DiagnosticClass::Analysis { evidence }) => Some(evidence),
        _ => None,
    };
    Diagnostic {
        code: code.to_string(),
        severity,
        message,
        span,
        layer,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
        evidence,
    }
}

/// What a target looks like, for messages: the shape `is_entry_target`
/// accepts, in words.
pub(crate) const TARGET_SHAPE: &str = "a dotted id (`npc.maud`, `item.rusty-key`): names joined \
     by `.`, each letters, digits, `_` or `-`, not starting with `-`";

/// Why `t` is no target of `what` (`` `<objective>` ``, `` `<beat>` `` …).
/// `kind_ok`: `what` also takes `kind:<entity kind>` (beats and entries);
/// one that takes a single member (an objective, an `<on>` handler) says so
/// when handed a kind.
pub(crate) fn malformed_target(what: &str, t: &str, kind_ok: bool) -> String {
    if !kind_ok && t.starts_with("kind:") {
        return format!(
            "{what} `target=\"{t}\"` names a kind, but {what} takes one target — a single member \
             such as `npc.maud`; kind targets are for beats and entries"
        );
    }
    let kind = if kind_ok {
        "; or `kind:<entity kind>` for every member of a kind"
    } else {
        ""
    };
    format!("{what} `target=\"{t}\"` must be {TARGET_SHAPE}{kind}")
}

/// dsl 0.25.0 §2: a `share` key that is no name (`what` names the
/// construct: `` `<entry>` `` / `` `<beat>` ``).
pub(crate) fn share_malformed(what: &str, key: &str) -> String {
    name_message(&format!("{what} `share` key"), key)
}

/// The occasion an `on` names must be a name (plugins declare no other).
pub(crate) fn occasion_malformed(what: &str, on: &str) -> String {
    name_message(&format!("{what} `on` occasion"), on)
}

pub(super) fn name_message(what: &str, name: &str) -> String {
    lute_manifest::ident::name_fault(what, name)
        .unwrap_or_else(|| format!("{what} `{name}` is not a name"))
}

/// The spending `once` periods — every value [`BeatOnce::parse`] accepts —
/// in the order messages list them.
pub const SPENDING_ONCE: [&str; 6] = ["run", "user", "day", "slot", "week", "season:<name>"];

/// dsl 0.25.0 §2: `share` names a spend, and only a written, spending
/// `once` is one.
pub(crate) fn share_without_once(key: &str) -> String {
    let periods: Vec<String> = SPENDING_ONCE.iter().map(|p| format!("`{p}`")).collect();
    let (last, init) = periods.split_last().expect("SPENDING_ONCE is not empty");
    format!(
        "`share` `{key}` without `once`: a `share` key spends its beats together when one is \
         presented, for their common `once` period, so each beat of the key writes the same \
         `once` ({} or {last}) — add it, or remove `share`",
        init.join(", ")
    )
}

/// A `share` key beside `spentBy`: `share` spends its beats together when
/// one is presented, and a `spentBy` beat is never spent by presenting it.
pub(crate) fn share_with_spent_by(key: &str) -> String {
    format!(
        "`share` `{key}` beside `spentBy`: a `share` key spends its beats together when one is \
         presented, but a `spentBy` beat is spent by its condition, not by being presented — \
         remove `share`, or give each beat of the key the same `spentBy`"
    )
}

/// `once: false` beside `spentBy`: a `spentBy` beat stays spent for its
/// `once` period once the condition has held, and `false` is no period.
pub(crate) fn spent_by_once_false(raw: &str) -> String {
    format!(
        "`once: false` beside `spentBy`: a `spentBy` beat stays spent once its condition has held, \
         for its `once` period (`run` unless written: `user`, `day`, `slot`, `week` or \
         `season:<name>`); to keep the beat eligible only while the condition is false, write \
         `when: \"!({raw})\"` instead"
    )
}

/// dsl 0.26.0 §8 (T3-3, `lute beats`): per beat of `beats` — one root's
/// [`project_beats`] in selection order (priority descending, then project
/// order) — the index of the earlier beat that always wins where it is
/// eligible: a non-`also` beat on the same `select: first` occasion, a
/// candidate whenever it is (untargeted, or its target), never spent, with
/// no `after:`, whose `when` the later beat's `when` implies (the same
/// condition, or a stronger one on the paths it reads). A fallback so
/// covered never plays while the beat above it stands; unlike
/// [`W_BEAT_SHADOWED`] that is often the design (a lead's fallback for areas
/// that do not answer), so it is a verdict, not a warning. A beat whose
/// coverer has no `when` at all is shadowed instead, and not listed here.
pub fn coverers(beats: &[ProjectBeat<'_>]) -> Vec<Option<usize>> {
    let norm = |w: &str| w.split_whitespace().collect::<Vec<_>>().join(" ");
    let dnfs: Vec<Option<crate::reachability::Dnf>> = beats
        .iter()
        .map(|pb| {
            pb.when.as_deref().map(|w| {
                let defs = DefTable {
                    bodies: &pb.folded.def_bodies,
                    params: &pb.folded.env.def_params,
                };
                crate::reachability::when_dnf(
                    w,
                    &defs,
                    &pb.folded.env.state,
                    Some(&pb.folded.env.rel_vocab),
                )
            })
        })
        .collect();
    beats
        .iter()
        .enumerate()
        .map(|(j, b)| {
            let select = b
                .folded
                .occasions
                .get(b.on)
                .map_or(OccasionSelect::First, |o| o.select);
            if select != OccasionSelect::First || b.also {
                return None;
            }
            let bw = dnfs[j].as_ref()?;
            (0..j).find(|&i| {
                let a = &beats[i];
                let Some(aw) = dnfs[i].as_ref() else {
                    return false;
                };
                !a.also
                    && a.on == b.on
                    && a.cells().covers(b.cells())
                    && a.once == BeatOnce::None
                    && a.after.is_none()
                    && (a.when.as_deref().map(norm) == b.when.as_deref().map(norm)
                        || crate::reachability::implies(bw, aw))
            })
        })
        .collect()
}
