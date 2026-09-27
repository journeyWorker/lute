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
/// `{ live: "<condition>" }`, or to the condition string itself (the short
/// form `harvest: "@harvestLive"`). Shape problems are [`E_SEASON_DECL`],
/// worded for the author (never serde's or Rust's own text), at the key
/// `span_at(path)` locates below `seasons:` — `[]` for `seasons:` itself,
/// `[name]` for an entry, `[name, "live"]` for its condition.
pub fn parse_seasons(
    value: &serde_yaml::Value,
    span_at: &dyn Fn(&[&str]) -> Span,
) -> (Seasons, Vec<Diagnostic>) {
    let mut out = Seasons::new();
    let mut diags = Vec::new();
    let Some(map) = value.as_mapping() else {
        diags.push(diag(
            format!("`seasons:` maps each season name to its condition, e.g. {SEASON_EXAMPLE}"),
            span_at(&[]),
        ));
        return (out, diags);
    };
    for (k, v) in map {
        let Some(name) = k.as_str().filter(|n| is_season_name(n)) else {
            let at = k.as_str().map_or_else(|| span_at(&[]), |k| span_at(&[k]));
            diags.push(diag(bad_season_name(k), at));
            continue;
        };
        match season_live(name, v) {
            Ok(live) => {
                out.insert(name.to_string(), SeasonDecl { live });
                continue;
            }
            Err(msg) => diags.push(diag(msg, span_at(&[name, "live"]))),
        }
        // The name is declared; only its body is at fault, and reported
        // above. An empty `live` keeps every use of the season from
        // repeating that one fault as "no schema declares it".
        out.insert(
            name.to_string(),
            SeasonDecl {
                live: String::new(),
            },
        );
    }
    (out, diags)
}

const SEASON_EXAMPLE: &str =
    "`seasons: { harvest: { live: \"@harvestLive\" } }` (short form `harvest: \"@harvestLive\"`)";

/// Why season key `k` is no season name, with the identifier it likely meant.
fn bad_season_name(k: &serde_yaml::Value) -> String {
    let shown = match k.as_str() {
        Some(s) => s.to_string(),
        None => serde_yaml::to_string(k)
            .unwrap_or_default()
            .trim()
            .to_string(),
    };
    // `lantern-fest` / `lantern fest` → `lanternFest`.
    let mut fixed = String::new();
    for (i, part) in shown
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .filter(|p| !p.is_empty())
        .enumerate()
    {
        let mut chars = part.chars();
        if let (true, Some(c)) = (i > 0, chars.next()) {
            fixed.push(c.to_ascii_uppercase());
            fixed.extend(chars);
        } else {
            fixed.push_str(part);
        }
    }
    let hint = if is_season_name(&fixed) && fixed != shown {
        format!(" — write `{fixed}`")
    } else {
        String::new()
    };
    format!(
        "season name `{shown}` is not a name: it is written in `season.<name>.*` paths and \
         `once: season:<name>`, so it is letters, digits and `_`, starting with a letter{hint}"
    )
}

/// The `live` condition of season `name`'s body `v`: a condition string (the
/// short form) or `{ live: "<condition>" }`.
fn season_live(name: &str, v: &serde_yaml::Value) -> Result<String, String> {
    let empty = || {
        format!(
            "season `{name}` has no condition — write `{name}: {{ live: \"<condition>\" }}` \
             (or `{name}: \"<condition>\"`), the condition that holds while its window is open"
        )
    };
    let live = match v {
        serde_yaml::Value::String(s) => s,
        serde_yaml::Value::Null => return Err(empty()),
        serde_yaml::Value::Mapping(m) => {
            if let Some(key) = m.keys().find(|k| k.as_str() != Some("live")) {
                let key = serde_yaml::to_string(key).unwrap_or_default();
                let key = key.trim();
                return Err(format!(
                    "season `{name}` has no key `{key}`{} — a season declares only \
                     `live: \"<condition>\"`",
                    lute_manifest::suggest::did_you_mean(key, ["live"])
                ));
            }
            match m.get("live") {
                Some(serde_yaml::Value::String(s)) => s,
                None | Some(serde_yaml::Value::Null) => return Err(empty()),
                Some(other) => {
                    let shown = serde_yaml::to_string(other).unwrap_or_default();
                    let shown = shown.trim();
                    return Err(format!(
                        "season `{name}`'s `live: {shown}` is not a condition string — quote \
                         it: `live: \"{shown}\"`"
                    ));
                }
            }
        }
        _ => {
            return Err(format!(
                "season `{name}` must map to its condition, e.g. {SEASON_EXAMPLE}"
            ))
        }
    };
    if live.trim().is_empty() {
        return Err(empty());
    }
    Ok(live.clone())
}

/// Fold the seasons a document sees (its imports', then its own) into one
/// map; a name declared twice with different `live` is [`E_SEASON_DECL`],
/// reported at the second declaration's `seasons:` line (`origin`, when it
/// is an imported schema's; `span` for the document's own).
pub fn fold_seasons<'a>(
    sources: impl Iterator<Item = (&'a str, &'a Seasons, Option<crate::rel_schema::DeclOrigin>)>,
    span: Span,
) -> (Seasons, Vec<Diagnostic>) {
    let mut out = Seasons::new();
    let mut from: BTreeMap<String, &str> = BTreeMap::new();
    let mut diags = Vec::new();
    for (file, seasons, origin) in sources {
        for (name, decl) in seasons {
            match out.get(name) {
                Some(prev) if prev != decl => diags.push(crate::rel_schema::at_origin(
                    diag(
                        format!(
                            "season `{name}` is declared twice with different `live:` \
                             conditions (`{}` in {} and `{}` in {file}) — one season has one \
                             window (dsl 0.27.0 §5)",
                            prev.live, from[name], decl.live
                        ),
                        span,
                    ),
                    origin.as_ref(),
                )),
                Some(_) => {}
                None => {
                    out.insert(name.clone(), decl.clone());
                    from.insert(name.clone(), file);
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
///
/// A path an imported schema declares is reported at its line there
/// (`origins`, [`crate::rel_schema::DeclOrigins::state`]), once for the
/// project; the document's own at `span`.
pub fn check_state(
    schema: &StateSchema,
    seasons: &Seasons,
    span: Span,
    origins: &BTreeMap<String, crate::rel_schema::DeclOrigin>,
) -> (Vec<(String, StateDecl)>, Vec<Diagnostic>) {
    let mut mirrors = Vec::new();
    let mut diags = Vec::new();
    for (path, decl) in &schema.decls {
        let Some(name) = season_of_path(path) else {
            continue;
        };
        match undeclared(name, &format!("state path `{path}`"), seasons) {
            Some(msg) => diags.push(crate::rel_schema::at_origin(
                diag(msg, span),
                origins.get(path),
            )),
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
            out.push(diag(msg, crate::beats::top_value_span(&doc.meta, "once")));
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

/// dsl 0.28.0 §5 (T2-7): a relation's `tier: season:<name>` naming an
/// undeclared season — at the relation's schema line for an imported one,
/// else at `span` (this document's `relations:` key).
pub fn check_relation_tiers(
    vocab: &crate::rel_schema::RelVocab,
    seasons: &Seasons,
    span: Span,
) -> Vec<Diagnostic> {
    vocab
        .relations
        .iter()
        .filter_map(|(rel, decl)| {
            let name = season_ref(decl.tier.as_deref()?)?;
            let what = format!("relation `{rel}`'s `tier: season:{name}`");
            let msg = undeclared(name, &what, seasons)?;
            Some(crate::rel_schema::at_origin(
                diag(msg, span),
                vocab.origins.relations.get(rel),
            ))
        })
        .collect()
}

/// The declared season an illegal `once` / `tier` value most likely means:
/// the value is the season's name, its state-path spelling `season.<name>`,
/// or a near miss of a declared name. A `season:<x>` value is
/// [`undeclared`]'s.
pub fn meant_season<'a>(raw: &str, seasons: &'a Seasons) -> Option<&'a str> {
    if season_ref(raw).is_some() {
        return None;
    }
    let bare = raw.strip_prefix("season.").unwrap_or(raw);
    if let Some((name, _)) = seasons.get_key_value(bare) {
        return Some(name);
    }
    lute_manifest::suggest::nearest(bare, seasons.keys().map(String::as_str), 2)
}

/// Every illegal `once` / `tier` value of `doc` that names a declared
/// season ([`meant_season`]) gets ` — did you mean `season:<name>`?` on the
/// error that refused it (a scene's `once:`, an entry's or bundle beat's
/// `once=`, a quest's `tier=`, a relation's `tier:`). A scene's legacy
/// `season:` key holding a declared season's name is an [`E_SEASON_DECL`]
/// of its own, in place of the legacy-key warning.
pub(crate) fn hint_declared(
    doc: &lute_syntax::ast::Document,
    is_scene: bool,
    seasons: &Seasons,
    vocab: &crate::rel_schema::RelVocab,
    diags: &mut Vec<Diagnostic>,
) {
    if seasons.is_empty() {
        return;
    }
    let hint = |name: &str| format!(" — did you mean `season:{name}`?");
    let mut amend = |code: &str, span: Span, raw: &str| {
        let Some(name) = meant_season(raw, seasons) else {
            return;
        };
        for d in diags.iter_mut().filter(|d| {
            d.code == code
                && d.span.byte_start == span.byte_start
                && d.span.byte_end == span.byte_end
                && !d.message.contains("did you mean")
        }) {
            d.message.push_str(&hint(name));
        }
    };
    if let Some((raw, span)) = crate::beats::top_value_text(&doc.meta, "once") {
        amend(crate::beats::E_BEAT_ATTR, span, raw);
    }
    let onces = doc
        .entries
        .iter()
        .filter_map(|e| e.once.as_ref())
        .chain(doc.beats.iter().filter_map(|b| b.once.as_ref()));
    for (raw, span) in onces {
        amend(crate::beats::E_BEAT_ATTR, *span, raw);
    }
    for (raw, span) in doc.quests.iter().filter_map(|q| q.tier.as_ref()) {
        amend("E-ATTR-TYPE", *span, raw);
    }
    for (rel, decl) in &vocab.relations {
        let Some(tier) = decl.tier.as_deref() else {
            continue;
        };
        let Some(name) = meant_season(tier, seasons) else {
            continue;
        };
        let refused = format!("relation `{rel}` has unknown `tier: {tier}`");
        for d in diags.iter_mut().filter(|d| {
            d.code == "E-RELATION-DOMAIN"
                && d.message.starts_with(&refused)
                && !d.message.contains("did you mean")
        }) {
            d.message.push_str(&hint(name));
        }
    }
    if !is_scene {
        return;
    }
    let Some((raw, span)) = crate::beats::top_value_text(&doc.meta, "season") else {
        return;
    };
    let Some(name) = meant_season(raw, seasons) else {
        return;
    };
    diags.retain(|d| !(d.code == "W-META-LEGACY" && d.message.starts_with("`season`")));
    diags.push(diag(legacy_season(raw, name, &seasons[name].live), span));
}

/// A legacy `season:` (the scene's episode number) that names season
/// `name`: what ties a scene to a season instead.
fn legacy_season(raw: &str, name: &str, live: &str) -> String {
    format!(
        "`season: {raw}` is the legacy episode number, not season `{name}`; to tie this scene \
         to season `{name}` write `once: season:{name}` (spent once per window) and/or \
         `when: \"{live}\"` (the season's `live:`), and remove `season:`"
    )
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
    // A season whose body failed to parse (reported where it is declared).
    if raw.trim().is_empty() {
        return Vec::new();
    }
    // dsl 0.28.0 §1 (T1-5): judged between steps, so no `occasion.*`; and the
    // literal checks every condition slot gets.
    if let Some(mut d) = crate::gates::outside_occasion(raw, at) {
        d.message = format!("season `{name}` `live: {raw}`: {}", d.message);
        return vec![d];
    }
    let mut diags = crate::gates::check_condition(raw, at, ctx);
    if diags.iter().all(|d| d.code != "E-CEL-PARSE") {
        let slot = lute_syntax::ast::CelSlot::raw(
            lute_syntax::ast::CelKind::Condition,
            raw.to_string(),
            at,
        );
        diags.extend(crate::defassign::check_quest_guard_defassign(&slot, scope));
        diags.extend(crate::gates::condition_literals(
            raw,
            &scope.defs,
            scope.schema,
            None,
            at,
        ));
    }
    for d in &mut diags {
        d.message = format!("season `{name}` `live: {raw}`: {}", d.message);
        d.span = at;
        d.fixits.clear();
    }
    diags
}

/// A beat spent per season window (`once: season:<name>`) or a
/// `tier="season:<name>"` quest that can start while the season is not
/// live: `once` and `tier` only set how long the beat stays spent / when the
/// quest resets, never whether the season has opened.
pub const W_SEASON_UNGATED: &str = "W-SEASON-UNGATED";

/// Which slot a [`W_SEASON_UNGATED`] fix edits.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Gate {
    /// A scene's frontmatter `when:`.
    Frontmatter,
    /// An `<entry>` / bundle `<beat>` `when=`.
    Attr,
    /// A `<quest>` `start=`.
    Start,
}

/// [`W_SEASON_UNGATED`] for every beat of `doc` (its scene beat, entry
/// beats and bundle beats) with `once: season:<name>`, and every quest with
/// `tier="season:<name>"` and a `start`, whose condition — the beat's
/// `when` under its occasion's `raisedWhen` gate, the quest's `start` —
/// does not imply the season's `live`: `condition && !live` must decide
/// false. An undeclared season is `E-SEASON-DECL`'s.
pub(crate) fn check_ungated(
    doc: &lute_syntax::ast::Document,
    folded: &crate::check::FoldedEnv,
    defs: &crate::cel_expand::DefTable<'_>,
    ctx: &crate::decide::DecideCtx<'_>,
) -> Vec<Diagnostic> {
    use crate::decide::{decide_slot, Decided};
    let gate = |on: &str| {
        crate::gates::gate_of(&folded.occasions, on)
            .filter(|g| !crate::occasion_bind::mentions_target(g))
    };
    // (what, season, its condition's parts, the slot a fix edits, span)
    let mut units: Vec<(String, &str, Vec<&str>, Gate, Span)> = Vec::new();
    if let Some(b) = &folded.typed.beat {
        if let Some(name) = b.once.season() {
            units.push((
                format!("beat `{}`", crate::beats::scene_beat_name(folded)),
                name,
                b.when
                    .iter()
                    .map(|w| w.raw.as_str())
                    .chain(gate(&b.on))
                    .collect(),
                Gate::Frontmatter,
                crate::beats::top_value_span(&doc.meta, "once"),
            ));
        }
    }
    for e in &doc.entries {
        let (Some((on, _)), Some((once, span))) = (&e.on, &e.once) else {
            continue;
        };
        if let Some(name) = season_ref(once) {
            units.push((
                format!("entry `{}`", e.id),
                name,
                e.when
                    .iter()
                    .map(|w| w.raw.as_str())
                    .chain(gate(on))
                    .collect(),
                Gate::Attr,
                *span,
            ));
        }
    }
    let doc_id = folded.typed.id.as_deref().unwrap_or("this document");
    for b in &doc.beats {
        let (Some((on, _)), Some((once, span))) = (&b.on, &b.once) else {
            continue;
        };
        if let Some(name) = season_ref(once) {
            units.push((
                format!("beat `{}`", crate::bundles::bundle_beat_key(doc_id, &b.id)),
                name,
                b.when
                    .iter()
                    .map(|w| w.raw.as_str())
                    .chain(gate(on))
                    .collect(),
                Gate::Attr,
                *span,
            ));
        }
    }
    for q in &doc.quests {
        let (Some((tier, span)), Some(start)) = (&q.tier, &q.start) else {
            continue;
        };
        if let Some(name) = season_ref(tier) {
            units.push((
                format!("quest `{}`", q.id),
                name,
                vec![start.raw.as_str()],
                Gate::Start,
                *span,
            ));
        }
    }
    let mut out = Vec::new();
    for (what, name, parts, slot, span) in units {
        let Some(live) = folded
            .env
            .seasons
            .get(name)
            .map(|s| s.live.trim())
            .filter(|l| !l.is_empty())
        else {
            continue;
        };
        let parts: Vec<&str> = parts
            .into_iter()
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .collect();
        let mut judged: Vec<String> = parts.iter().map(|p| format!("({p})")).collect();
        judged.push(format!("!({live})"));
        if matches!(
            decide_slot(&judged.join(" && "), defs, ctx),
            Some(Decided::Bool(false))
        ) {
            continue;
        }
        out.push(ungated(
            what,
            name,
            live,
            slot,
            parts.is_empty(),
            folded,
            span,
        ));
    }
    out
}

/// One [`W_SEASON_UNGATED`]: the unit, the season and its `live`, and the fix
/// — the `live` condition (or a def whose body it is) added to the slot.
fn ungated(
    what: String,
    name: &str,
    live: &str,
    slot: Gate,
    unconditioned: bool,
    folded: &crate::check::FoldedEnv,
    span: Span,
) -> Diagnostic {
    let def = (!live.starts_with('@'))
        .then(|| {
            folded
                .def_bodies
                .iter()
                .find(|(_, body)| body.trim() == live)
                .map(|(def, _)| format!("@{def}"))
        })
        .flatten();
    let cond = def.as_deref().unwrap_or(live);
    let (own, consequence) = match slot {
        Gate::Start => (
            "`start`",
            format!(
                "so it can start while season `{name}` has never opened \
                 (`tier=\"season:{name}\"` only resets it when a window opens)"
            ),
        ),
        Gate::Frontmatter => (
            "`when`",
            format!(
                "so it plays even while season `{name}` has never opened (`once: \
                 season:{name}` only sets how long it stays spent)"
            ),
        ),
        Gate::Attr => (
            "`when`",
            format!(
                "so it plays even while season `{name}` has never opened (`once=\"\
                 season:{name}\"` only sets how long it stays spent)"
            ),
        ),
    };
    let mut fix = match (slot, unconditioned) {
        (Gate::Frontmatter, true) => format!("add `when: \"{cond}\"`"),
        (Gate::Attr, true) => format!("add `when=\"{cond}\"`"),
        _ => format!("add `&& {cond}` to its {own}"),
    };
    if def.is_none() && !live.starts_with('@') {
        fix.push_str(", or a def that reads it");
    }
    Diagnostic {
        code: W_SEASON_UNGATED.to_string(),
        severity: Severity::Warning,
        message: format!(
            "{what} is not gated on season `{name}`: its {own} does not imply the season's \
             `live: {live}`, {consequence} — {fix}"
        ),
        span,
        layer: Layer::Logic,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    }
}
