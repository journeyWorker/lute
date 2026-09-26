//! dsl 0.27.0 §5: seasons — a schema's `seasons:` parsed, folded across the
//! imports and checked against every use: `season.<name>.*` state paths,
//! `once: season:<name>`, `<quest tier="season:<name>">`. Every misuse is
//! [`E_SEASON_DECL`]. The runtime model lives in
//! `lute_trace::exec::cadence`; the shared path arithmetic in
//! [`lute_manifest::season`].

use std::collections::BTreeMap;

use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_manifest::season::{is_season_name, season_of_path, season_ref, SeasonDecl};

use crate::meta::{Namespace, StateDecl, StateSchema};

/// A season declared or used wrongly (dsl 0.27.0 §5).
pub const E_SEASON_DECL: &str = "E-SEASON-DECL";

/// The declared seasons of a project, by name.
pub type Seasons = BTreeMap<String, SeasonDecl>;

fn diag(message: String, span: Span) -> Diagnostic {
    Diagnostic {
        code: E_SEASON_DECL.to_string(),
        severity: Severity::Error,
        message,
        span,
        layer: Layer::Content,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    }
}

/// Lift a schema document's `seasons:` value — a map of season name to
/// `{ live: "<condition>" }`. Shape problems are [`E_SEASON_DECL`] at `span`.
pub fn parse_seasons(value: &serde_yaml::Value, span: Span) -> (Seasons, Vec<Diagnostic>) {
    let mut out = Seasons::new();
    let mut diags = Vec::new();
    let shape = "`seasons:` maps each season name to `{ live: \"<condition>\" }`, e.g. \
                 `seasons: { harvest: { live: \"@harvestLive\" } }` (dsl 0.27.0 §5)";
    let Some(map) = value.as_mapping() else {
        diags.push(diag(shape.to_string(), span));
        return (out, diags);
    };
    for (k, v) in map {
        let Some(name) = k.as_str().filter(|n| is_season_name(n)) else {
            diags.push(diag(
                format!("{shape}: `{k:?}` is no season name (`[A-Za-z][A-Za-z0-9_]*`)"),
                span,
            ));
            continue;
        };
        match serde_yaml::from_value::<SeasonDecl>(v.clone()) {
            Ok(decl) if !decl.live.trim().is_empty() => {
                out.insert(name.to_string(), decl);
            }
            Ok(_) => diags.push(diag(
                format!("season `{name}`: `live:` is empty — name the condition that holds while its window is open (dsl 0.27.0 §5)"),
                span,
            )),
            Err(e) => diags.push(diag(format!("season `{name}`: {shape}: {e}"), span)),
        }
    }
    (out, diags)
}

/// Fold the seasons a document sees (its imports', then its own) into one
/// map; a name declared twice with different `live` is [`E_SEASON_DECL`].
pub fn fold_seasons<'a>(
    sources: impl Iterator<Item = (&'a str, &'a Seasons)>,
    span: Span,
) -> (Seasons, Vec<Diagnostic>) {
    let mut out = Seasons::new();
    let mut from: BTreeMap<String, &str> = BTreeMap::new();
    let mut diags = Vec::new();
    for (origin, seasons) in sources {
        for (name, decl) in seasons {
            match out.get(name) {
                Some(prev) if prev != decl => diags.push(diag(
                    format!(
                        "season `{name}` is declared twice with different `live:` conditions \
                         (`{}` in {} and `{}` in {origin}) — one season has one window \
                         (dsl 0.27.0 §5)",
                        prev.live, from[name], decl.live
                    ),
                    span,
                )),
                Some(_) => {}
                None => {
                    out.insert(name.clone(), decl.clone());
                    from.insert(name.clone(), origin);
                }
            }
        }
    }
    (out, diags)
}

/// The problem with naming season `name` in `what`, when it is undeclared.
pub fn undeclared(name: &str, what: &str, seasons: &Seasons) -> Option<String> {
    if seasons.contains_key(name) {
        return None;
    }
    let hint = lute_manifest::suggest::nearest(name, seasons.keys().map(String::as_str), 2)
        .map(|n| format!(" — did you mean `{n}`?"))
        .unwrap_or_default();
    let declared = if seasons.is_empty() {
        "the project declares no `seasons:`".to_string()
    } else {
        format!(
            "declared: {}",
            seasons.keys().cloned().collect::<Vec<_>>().join(", ")
        )
    };
    Some(format!(
        "{what} names season `{name}`, which no schema declares ({declared}){hint} (dsl 0.27.0 §5)"
    ))
}

/// Every `season.<name>.*` state path naming an undeclared season, and the
/// read-only `prev.season.<name>.*` mirrors of the declared ones (same
/// type, no default: unset until the season has opened a second time).
pub fn check_state(
    schema: &StateSchema,
    seasons: &Seasons,
    span: Span,
) -> (Vec<(String, StateDecl)>, Vec<Diagnostic>) {
    let mut mirrors = Vec::new();
    let mut diags = Vec::new();
    for (path, decl) in &schema.decls {
        let Some(name) = season_of_path(path) else {
            continue;
        };
        match undeclared(name, &format!("state path `{path}`"), seasons) {
            Some(msg) => diags.push(diag(msg, span)),
            None => mirrors.push((
                format!("prev.{path}"),
                StateDecl {
                    ty: decl.ty.clone(),
                    default: None,
                    namespace: Namespace::Season,
                    owner: Some(lute_manifest::types::Owner::Engine),
                },
            )),
        }
    }
    (mirrors, diags)
}

/// `once: season:<name>` (a scene's, an entry's, a bundle beat's) and
/// `<quest tier="season:<name>">` naming an undeclared season.
pub fn check_uses(
    doc: &lute_syntax::ast::Document,
    beat: Option<&crate::beats::BeatMeta>,
    seasons: &Seasons,
) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    if let Some(name) = beat.and_then(|b| b.once.season()) {
        if let Some(msg) = undeclared(name, "`once:`", seasons) {
            out.push(diag(msg, crate::meta::meta_key_span(&doc.meta, "once")));
        }
    }
    let onces = doc
        .entries
        .iter()
        .filter_map(|e| e.once.as_ref())
        .chain(doc.beats.iter().filter_map(|b| b.once.as_ref()));
    for (raw, span) in onces {
        if let Some(name) = season_ref(raw) {
            if let Some(msg) = undeclared(name, &format!("`once=\"{raw}\"`"), seasons) {
                out.push(diag(msg, *span));
            }
        }
    }
    for q in &doc.quests {
        if let Some((raw, span)) = &q.tier {
            if let Some(name) = season_ref(raw) {
                if let Some(msg) = undeclared(name, &format!("`tier=\"{raw}\"`"), seasons) {
                    out.push(diag(msg, *span));
                }
            }
        }
    }
    out
}

/// Every season's `live:` checked like any condition slot in this
/// document's environment: a `Bool`, and definitely assigned with nothing
/// dominating it (the engine judges it between steps, like a `<quest
/// start>`). A schema document's own at its `seasons:` key; an imported
/// schema's at that schema's `seasons:` line (`check-project` folds the
/// importers' identical reports into one).
pub(crate) fn check_live_texts(
    doc: &lute_syntax::ast::Document,
    own: &Seasons,
    imported: &[(std::path::PathBuf, Seasons, Span)],
    ctx: &crate::ctx::Ctx<'_>,
    scope: &crate::defassign::Scope<'_>,
) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    if !own.is_empty() {
        let at = crate::meta::meta_key_span(&doc.meta, "seasons");
        for (name, decl) in own {
            out.extend(check_live(name, &decl.live, at, ctx, scope));
        }
    }
    for (file, seasons, span) in imported {
        let origin = crate::rel_schema::DeclOrigin {
            file: file.clone(),
            span: *span,
        };
        for (name, decl) in seasons {
            out.extend(
                check_live(name, &decl.live, doc.meta.span, ctx, scope)
                    .into_iter()
                    .map(|d| crate::rel_schema::at_origin(d, Some(&origin))),
            );
        }
    }
    out
}

/// Season `name`'s `live` condition `raw`, every diagnostic at `at` and
/// named as the season's.
fn check_live(
    name: &str,
    raw: &str,
    at: Span,
    ctx: &crate::ctx::Ctx<'_>,
    scope: &crate::defassign::Scope<'_>,
) -> Vec<Diagnostic> {
    let mut diags = crate::gates::check_condition(raw, at, ctx);
    if diags.iter().all(|d| d.code != "E-CEL-PARSE") {
        let slot = lute_syntax::ast::CelSlot::raw(
            lute_syntax::ast::CelKind::Condition,
            raw.to_string(),
            at,
        );
        diags.extend(crate::defassign::check_quest_guard_defassign(&slot, scope));
    }
    for d in &mut diags {
        d.message = format!("season `{name}` `live: {raw}`: {}", d.message);
        d.span = at;
        d.fixits.clear();
    }
    diags
}
