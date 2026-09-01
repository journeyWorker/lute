//! Rule registry (spec §6 & §7).
//!
//! Three intake paths, one dispatch:
//! - **Core data rules** — [`CORE_DATA_YAML`], one embedded YAML file per
//!   rule, parsed once via [`load_data_rule`]. Evaluated by the same
//!   [`crate::eval`] a plugin/custom rule uses (dogfood, spec §6).
//! - **Core Rust rules** — logic beyond one CEL assertion:
//!   `emotion-distribution`, `variant-composition`, `asset-exists`
//!   (spec §7). Each still exposes id/target/default-level/default-options
//!   so config handling is uniform.
//! - **Plugin & custom** rules — arrive as `Vec<(namespaced_id, decl)>` from
//!   the caller; the plugin loader (`lute_manifest::lint`) has already
//!   pre-namespaced plugin ids as `<plugin-id>/<rule-id>` per spec §3.
//!
//! Level precedence (spec §6): config override > decl level > rule default.

use std::collections::{BTreeMap, BTreeSet};

use lute_core_span::{Diagnostic, Layer, RelatedDiagnostic, Severity, Span};

use crate::config::{deep_merge, LintConfig, RuleOverride};
use crate::eval::{eval, render_message, Env, Value};
use crate::metrics::{
    AxisStats, DirectiveRow, DocTables, GroupRow, LineRow, ProjectRow, SceneRow, ShotRow,
    SpeakerRow, TopStats,
};
use crate::model::{
    diagnostic_code, level_severity, LintLevel, LintRuleDecl, LintTarget, E_LINT_CONFIG,
    E_LINT_EXPR,
};

/// Embedded core data-rule YAML (spec §7). Kept as one const per rule so a
/// grep for a rule id reaches the definition, not just the loader.
pub const CORE_DATA_YAML: &[&str] = &[
    include_str!("../rules/dialogue-length.yaml"),
    include_str!("../rules/dialogue-ratio.yaml"),
    include_str!("../rules/scene-length-spread.yaml"),
    include_str!("../rules/shot-starts-with-background.yaml"),
];

/// One rule ready for evaluation: id, target, resolved level, merged
/// options, and the impl (data-CEL or Rust closure).
#[derive(Clone, Debug)]
pub struct ResolvedRule {
    pub id: String,
    pub target: LintTarget,
    pub level: LintLevel,
    pub options: serde_yaml::Mapping,
    pub kind: RuleKind,
    /// Where the level came from — feeds `E-LINT-EXPR` provenance so the
    /// author sees which file put the rule at that severity.
    pub source: RuleSource,
}

#[derive(Clone, Debug)]
pub enum RuleKind {
    /// Data rule (embedded core, plugin YAML, or `custom:`) — CEL `when`
    /// and message template.
    Data { when: String, message: String },
    /// A built-in Rust rule.
    Rust(RustRuleId),
}

/// Discriminant for the three Rust rules — smaller than storing function
/// pointers, matches deterministically, and keeps the trait shape flat.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum RustRuleId {
    EmotionDistribution,
    VariantComposition,
    AssetExists,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuleSource {
    Core,
    Plugin,
    Custom,
}

/// One rule finding — the engine converts each into a [`Diagnostic`],
/// anchoring the span per spec §8.
#[derive(Clone, Debug)]
pub struct Finding {
    pub code: String,
    pub severity: Severity,
    pub message: String,
    pub span: Span,
    pub related: Vec<RelatedDiagnostic>,
    /// Overrides the default `Content` layer. Rules keep this at `None`.
    pub layer: Option<Layer>,
}

// ---------------------------------------------------------------------------
// Registry & config application
// ---------------------------------------------------------------------------

/// One catalog entry — enough metadata to resolve id/target/level and to
/// carry a Rust rule's default options through YAML unchanged.
struct CoreEntry {
    id: &'static str,
    target: LintTarget,
    default_level: LintLevel,
    kind: CoreKind,
}

enum CoreKind {
    Data(&'static str), // embedded YAML text
    Rust(RustRuleId, &'static str /* default options YAML */),
}

/// The full set of core rules. Order is the deterministic default; config
/// overrides are applied on top.
fn core_entries() -> Vec<CoreEntry> {
    vec![
        // Data rules — pulled from embedded YAML so declaration + defaults
        // stay together (spec §7 table).
        CoreEntry {
            id: "dialogue-length",
            target: LintTarget::Line,
            default_level: LintLevel::Warn,
            kind: CoreKind::Data(CORE_DATA_YAML[0]),
        },
        CoreEntry {
            id: "dialogue-ratio",
            target: LintTarget::Scene,
            default_level: LintLevel::Warn,
            kind: CoreKind::Data(CORE_DATA_YAML[1]),
        },
        CoreEntry {
            id: "scene-length-spread",
            target: LintTarget::Project,
            default_level: LintLevel::Warn,
            kind: CoreKind::Data(CORE_DATA_YAML[2]),
        },
        CoreEntry {
            id: "shot-starts-with-background",
            target: LintTarget::Shot,
            default_level: LintLevel::Warn,
            kind: CoreKind::Data(CORE_DATA_YAML[3]),
        },
        // Rust rules (spec §7 defaults verbatim).
        CoreEntry {
            id: "emotion-distribution",
            target: LintTarget::Speaker,
            default_level: LintLevel::Warn,
            kind: CoreKind::Rust(
                RustRuleId::EmotionDistribution,
                "domain: emotion\npairWith: null\nminLines: 10\nrunMax: 3\nstreakAvgMin: 1.5\nmaxShare: 0.4\n",
            ),
        },
        CoreEntry {
            id: "variant-composition",
            target: LintTarget::Speaker,
            default_level: LintLevel::Warn,
            kind: CoreKind::Rust(
                RustRuleId::VariantComposition,
                "attr: variant\ngroupBy: null\nminPerGroup: 2\nminShare: 0.0\nminLines: 10\n",
            ),
        },
        CoreEntry {
            id: "asset-exists",
            target: LintTarget::Line, // per spec §7: line/directive; engine
            // anchors to the directive's assetId span. Encoded as `Line`
            // here so the caller may still list config option `providers:`
            // without a bespoke target.
            default_level: LintLevel::Error,
            kind: CoreKind::Rust(
                RustRuleId::AssetExists,
                "providers: {}\nsentinels: [clear, empty, false, none, null, stop]\n",
            ),
        },
    ]
}

/// Names every group-by attr referenced by an active variant-composition
/// (or plugin) rule, so [`crate::metrics::compute_doc_tables`] materializes
/// only the groups a rule will read.
pub fn active_group_bys(rules: &[ResolvedRule]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for r in rules {
        if let RuleKind::Rust(RustRuleId::VariantComposition) = &r.kind {
            if let Some(gb) = r
                .options
                .get(serde_yaml::Value::from("groupBy"))
                .and_then(|v| v.as_str())
            {
                out.insert(gb.to_string());
            }
        }
    }
    out
}

/// Resolve every rule (spec §6 precedence).
///
/// - `plugin_rules` is `[(namespaced_id, decl)]` — the loader has already
///   applied `<plugin-id>/<rule-id>` per spec §3, so an id collision with
///   a core rule is a plugin-side configuration bug, reported to the
///   caller BEFORE this function (spec §8 `E-LINT-RULE`).
/// - Custom ids MUST NOT collide with a core id (spec §3); such a
///   collision is reported here as `E-LINT-CONFIG` and the entry is
///   skipped.
/// - Every config `rules:` id MUST refer to a known rule; unknown ids are
///   `E-LINT-CONFIG` (rule skipped, run continues, exits 1).
pub fn resolve_rules(
    config: &LintConfig,
    plugin_rules: &[(String, LintRuleDecl)],
    config_span: Span,
) -> (Vec<ResolvedRule>, Vec<Diagnostic>) {
    let entries = core_entries();
    let core_ids: BTreeSet<&str> = entries.iter().map(|e| e.id).collect();
    let mut out: Vec<ResolvedRule> = Vec::new();
    let mut config_diags: Vec<Diagnostic> = Vec::new();
    let mut seen_ids: BTreeSet<String> = BTreeSet::new();

    // Config validation up-front: any override id not matching a
    // core/plugin/custom id is an error the caller sees before evaluation.
    let mut all_ids: BTreeSet<String> = core_ids.iter().map(|s| (*s).to_string()).collect();
    for (id, _) in plugin_rules {
        all_ids.insert(id.clone());
    }
    for c in &config.custom {
        if core_ids.contains(c.id.as_str()) {
            config_diags.push(config_diag(
                format!("custom rule id `{}` collides with a core rule id", c.id),
                config_span,
            ));
            continue;
        }
        all_ids.insert(c.id.clone());
    }
    for id in config.rules.keys() {
        if !all_ids.contains(id) {
            config_diags.push(config_diag(
                format!("unknown rule id `{id}` in config `rules:`"),
                config_span,
            ));
        }
    }

    // Core.
    for entry in &entries {
        if let Some(r) = resolve_core(entry, &config.rules, config_span, &mut config_diags) {
            seen_ids.insert(r.id.clone());
            out.push(r);
        }
    }

    // Plugin.
    for (id, decl) in plugin_rules {
        if seen_ids.contains(id) {
            config_diags.push(config_diag(
                format!("duplicate rule id `{id}` (plugin conflicts with core/plugin)"),
                config_span,
            ));
            continue;
        }
        if let Some(r) = resolve_decl(
            id.clone(),
            decl,
            RuleSource::Plugin,
            &config.rules,
            config_span,
            &mut config_diags,
        ) {
            seen_ids.insert(r.id.clone());
            out.push(r);
        }
    }

    // Custom.
    for decl in &config.custom {
        if core_ids.contains(decl.id.as_str()) || seen_ids.contains(&decl.id) {
            // Collision with core is already reported above; a collision
            // with a plugin id is reported here.
            if !core_ids.contains(decl.id.as_str()) {
                config_diags.push(config_diag(
                    format!("custom rule id `{}` collides with a plugin rule", decl.id),
                    config_span,
                ));
            }
            continue;
        }
        if let Some(r) = resolve_decl(
            decl.id.clone(),
            decl,
            RuleSource::Custom,
            &config.rules,
            config_span,
            &mut config_diags,
        ) {
            seen_ids.insert(r.id.clone());
            out.push(r);
        }
    }

    (out, config_diags)
}

fn resolve_core(
    entry: &CoreEntry,
    overrides: &BTreeMap<String, RuleOverride>,
    config_span: Span,
    diags: &mut Vec<Diagnostic>,
) -> Option<ResolvedRule> {
    let (default_options, when, message) = match entry.kind {
        CoreKind::Data(yaml) => {
            let decl: LintRuleDecl = match serde_yaml::from_str(yaml) {
                Ok(d) => d,
                Err(e) => {
                    diags.push(config_diag(
                        format!(
                            "core rule `{}` embedded YAML failed to parse: {e}",
                            entry.id
                        ),
                        config_span,
                    ));
                    return None;
                }
            };
            let opts = to_mapping(&decl.options);
            (opts, decl.when, decl.message)
        }
        CoreKind::Rust(_, defaults_yaml) => {
            let opts: serde_yaml::Mapping = serde_yaml::from_str(defaults_yaml).unwrap_or_default();
            (opts, String::new(), String::new())
        }
    };
    let ovr = overrides.get(entry.id);
    let level = match ovr.and_then(|o| o.level) {
        Some(l) => l,
        None => entry.default_level,
    };
    let options = match ovr {
        Some(o) => {
            if let Err(msg) = validate_options(entry.id, &default_options, &o.options) {
                diags.push(config_diag(msg, config_span));
                return None;
            }
            deep_merge(&default_options, &o.options)
        }
        None => default_options,
    };
    let kind = match entry.kind {
        CoreKind::Data(_) => RuleKind::Data { when, message },
        CoreKind::Rust(rid, _) => RuleKind::Rust(rid),
    };
    Some(ResolvedRule {
        id: entry.id.to_string(),
        target: entry.target,
        level,
        options,
        kind,
        source: RuleSource::Core,
    })
}

fn resolve_decl(
    id: String,
    decl: &LintRuleDecl,
    source: RuleSource,
    overrides: &BTreeMap<String, RuleOverride>,
    config_span: Span,
    diags: &mut Vec<Diagnostic>,
) -> Option<ResolvedRule> {
    let default_options = to_mapping(&decl.options);
    let ovr = overrides.get(&id);
    let level = match ovr.and_then(|o| o.level) {
        Some(l) => l,
        None => decl.level.unwrap_or(LintLevel::Warn),
    };
    let options = match ovr {
        Some(o) => {
            if let Err(msg) = validate_options(&id, &default_options, &o.options) {
                diags.push(config_diag(msg, config_span));
                return None;
            }
            deep_merge(&default_options, &o.options)
        }
        None => default_options,
    };
    Some(ResolvedRule {
        id,
        target: decl.target,
        level,
        options,
        kind: RuleKind::Data {
            when: decl.when.clone(),
            message: decl.message.clone(),
        },
        source,
    })
}

fn to_mapping(m: &serde_yaml::Mapping) -> serde_yaml::Mapping {
    m.clone()
}

/// Every override key MUST match a declared default key (spec §3 "unknown
/// option key"). A missing default map means "no options at all", so any
/// key is unknown.
fn validate_options(
    id: &str,
    defaults: &serde_yaml::Mapping,
    overrides: &serde_yaml::Mapping,
) -> Result<(), String> {
    for (k, ov) in overrides {
        let Some(dv) = defaults.get(k) else {
            let key = k
                .as_str()
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("{k:?}"));
            return Err(format!("rule `{id}`: unknown option key `{key}`"));
        };
        if !type_compatible(dv, ov) {
            let key = k
                .as_str()
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("{k:?}"));
            return Err(format!("rule `{id}`: option `{key}` has wrong type"));
        }
    }
    Ok(())
}

fn type_compatible(a: &serde_yaml::Value, b: &serde_yaml::Value) -> bool {
    use serde_yaml::Value::*;
    match (a, b) {
        (Bool(_), Bool(_)) => true,
        (Number(_), Number(_)) => true,
        (String(_), String(_)) => true,
        (Sequence(_), Sequence(_)) => true,
        (Mapping(_), Mapping(_)) => true,
        (Null, _) | (_, Null) => true, // permissive: nullable defaults accept overrides
        _ => false,
    }
}

fn config_diag(message: String, span: Span) -> Diagnostic {
    Diagnostic {
        code: E_LINT_CONFIG.to_string(),
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

// ---------------------------------------------------------------------------
// Evaluation
// ---------------------------------------------------------------------------

/// Everything a single-document rule pass needs. Kept as one struct so a
/// plugin/custom `when` and every core rule read the SAME bindings.
pub struct DocContext<'a> {
    pub tables: &'a DocTables,
    pub directives: &'a [DirectiveRow],
    pub providers: &'a lute_manifest::provider::ProviderSet,
}

/// Evaluate one rule against one document + the shared project row,
/// producing findings (spec §6 / §8).
///
/// `E-LINT-EXPR` is returned as ONE `Finding` per rule when evaluation
/// fails (spec §5); the engine then skips further rows for that rule.
pub fn evaluate_rule(
    rule: &ResolvedRule,
    ctx: &DocContext<'_>,
    project: &ProjectRow,
    rule_decl_span: Span,
) -> Vec<Finding> {
    let severity = match level_severity(rule.level) {
        Some(s) => s,
        None => return Vec::new(), // level: off
    };
    match &rule.kind {
        RuleKind::Data { when, message } => {
            evaluate_data(rule, when, message, severity, ctx, project, rule_decl_span)
        }
        RuleKind::Rust(RustRuleId::EmotionDistribution) => {
            eval_emotion_distribution(rule, severity, ctx)
        }
        RuleKind::Rust(RustRuleId::VariantComposition) => {
            eval_variant_composition(rule, severity, ctx)
        }
        RuleKind::Rust(RustRuleId::AssetExists) => eval_asset_exists(rule, severity, ctx),
    }
}

fn evaluate_data(
    rule: &ResolvedRule,
    when: &str,
    message: &str,
    severity: Severity,
    ctx: &DocContext<'_>,
    project: &ProjectRow,
    rule_decl_span: Span,
) -> Vec<Finding> {
    // Parse `when` once per rule per document. Parse failures produce
    // E-LINT-EXPR, rule skipped for this doc (spec §5).
    let mut arena = lute_cel::CelArena::default();
    let handle = match lute_cel::parse_slot(&mut arena, when, 0) {
        Ok(h) => h,
        Err(e) => {
            return vec![expr_error(
                &rule.id,
                format!("`when` failed to parse: {}", e.message),
                rule_decl_span,
            )];
        }
    };
    let ided = arena.get(handle).unwrap().clone();

    // Iterate the rows that match the target.
    let mut out = Vec::new();
    let options = mapping_to_value(&rule.options);
    let code = diagnostic_code(&rule.id);

    let evaluate_env =
        |env: Env, span: Span, related: Vec<RelatedDiagnostic>| match eval(&ided.expr, &env) {
            Ok(Value::Bool(true)) => Some(Finding {
                code: code.clone(),
                severity,
                message: render_message(message, &env),
                span,
                related,
                layer: None,
            }),
            Ok(Value::Bool(false)) => None,
            Ok(other) => Some(expr_error(
                &rule.id,
                format!("`when` returned non-bool: {other:?}"),
                rule_decl_span,
            )),
            Err(e) => Some(expr_error(&rule.id, e.message, rule_decl_span)),
        };

    match rule.target {
        LintTarget::Line => {
            for l in &ctx.tables.lines {
                let env = Env::new()
                    .with("line", line_value(l))
                    .with("options", options.clone());
                let mut first_err = false;
                if let Some(f) = evaluate_env(env, l.span, Vec::new()) {
                    if f.code == E_LINT_EXPR {
                        // A single expression error per rule per document —
                        // otherwise the same misspelled path floods the log.
                        first_err = true;
                        out.push(f);
                    } else {
                        out.push(f);
                    }
                }
                if first_err {
                    break;
                }
            }
        }
        LintTarget::Shot => {
            for s in &ctx.tables.shots {
                let env = Env::new()
                    .with("shot", shot_value(s))
                    .with("options", options.clone());
                if let Some(f) = evaluate_env(env, s.span, Vec::new()) {
                    out.push(f);
                    if out.last().map(|f| f.code == E_LINT_EXPR).unwrap_or(false) {
                        break;
                    }
                }
            }
        }
        LintTarget::Scene => {
            let Some(scene) = &ctx.tables.scene else {
                return out;
            };
            let env = Env::new()
                .with("scene", scene_value(scene))
                .with("options", options.clone());
            if let Some(f) = evaluate_env(env, scene.span, Vec::new()) {
                out.push(f);
            }
        }
        LintTarget::Speaker => {
            for sp in ctx.tables.speakers.values() {
                let env = Env::new()
                    .with("speaker", speaker_value(sp))
                    .with("options", options.clone());
                if let Some(f) = evaluate_env(env, sp.first_line_span, Vec::new()) {
                    out.push(f);
                    if out.last().map(|f| f.code == E_LINT_EXPR).unwrap_or(false) {
                        break;
                    }
                }
            }
        }
        LintTarget::Group => {
            for (attr, rows) in &ctx.tables.groups {
                for g in rows {
                    let env = Env::new()
                        .with("group", group_value(g))
                        .with("options", options.clone());
                    if let Some(f) = evaluate_env(env, g.first_line_span, Vec::new()) {
                        out.push(f);
                        if out.last().map(|f| f.code == E_LINT_EXPR).unwrap_or(false) {
                            return out;
                        }
                    }
                }
                let _ = attr;
            }
        }
        LintTarget::Project => {
            let env = Env::new()
                .with("project", project_value(project))
                .with("options", options.clone());
            let anchor = ctx
                .tables
                .scene
                .as_ref()
                .map(|s| s.span)
                .unwrap_or(rule_decl_span);
            if let Some(f) = evaluate_env(env, anchor, Vec::new()) {
                out.push(f);
            }
        }
    }

    out
}

fn expr_error(rule_id: &str, msg: String, span: Span) -> Finding {
    Finding {
        code: E_LINT_EXPR.to_string(),
        severity: Severity::Error,
        message: format!("rule `{rule_id}`: {msg}"),
        span,
        related: Vec::new(),
        layer: None,
    }
}

// ---------------------------------------------------------------------------
// Rust rules
// ---------------------------------------------------------------------------

fn opt_num(m: &serde_yaml::Mapping, key: &str) -> Option<f64> {
    m.get(serde_yaml::Value::from(key)).and_then(|v| v.as_f64())
}

fn opt_str<'a>(m: &'a serde_yaml::Mapping, key: &str) -> Option<&'a str> {
    m.get(serde_yaml::Value::from(key)).and_then(|v| v.as_str())
}

fn eval_emotion_distribution(
    rule: &ResolvedRule,
    severity: Severity,
    ctx: &DocContext<'_>,
) -> Vec<Finding> {
    let opts = &rule.options;
    let domain = opt_str(opts, "domain").unwrap_or("emotion").to_string();
    let pair_with = opt_str(opts, "pairWith").map(str::to_string);
    let min_lines = opt_num(opts, "minLines").unwrap_or(10.0) as u32;
    let run_max = opt_num(opts, "runMax").unwrap_or(3.0) as u32;
    let streak_avg_min = opt_num(opts, "streakAvgMin").unwrap_or(1.5);
    let max_share = opt_num(opts, "maxShare").unwrap_or(0.4);
    let code = diagnostic_code(&rule.id);
    let mut out = Vec::new();
    for sp in ctx.tables.speakers.values() {
        if sp.lines < min_lines {
            continue;
        }
        let mut reasons: Vec<String> = Vec::new();
        // Single-domain axis.
        if let Some(ax) = sp.axis.get(&domain) {
            report_axis(
                &mut reasons,
                &domain,
                ax,
                run_max,
                streak_avg_min,
                max_share,
            );
        }
        // Pair axis (e.g. emotion+variant).
        if let Some(pair) = &pair_with {
            let key = format!("{domain}+{pair}");
            if let Some(ax) = sp.axis.get(&key) {
                report_axis(&mut reasons, &key, ax, run_max, streak_avg_min, max_share);
            }
        }
        if !reasons.is_empty() {
            out.push(Finding {
                code: code.clone(),
                severity,
                message: format!("speaker `{}`: {}", sp.speaker, reasons.join("; ")),
                span: sp.first_line_span,
                related: Vec::new(),
                layer: None,
            });
        }
    }
    out
}

fn report_axis(
    reasons: &mut Vec<String>,
    axis_name: &str,
    ax: &AxisStats,
    run_max: u32,
    streak_avg_min: f64,
    max_share: f64,
) {
    if ax.run > run_max {
        reasons.push(format!("{}-run={} (cap {})", axis_name, ax.run, run_max));
    }
    if ax.streakAvg < streak_avg_min {
        reasons.push(format!(
            "{}-streakAvg={:.2} (floor {})",
            axis_name, ax.streakAvg, streak_avg_min
        ));
    }
    if ax.top.share > max_share {
        reasons.push(format!(
            "{}-share={}% (cap {}%)",
            axis_name,
            (ax.top.share * 100.0).round() as i64,
            (max_share * 100.0).round() as i64,
        ));
    }
    let _ = ax.top.value; // reserved for a future dominance-detail message
}

fn eval_variant_composition(
    rule: &ResolvedRule,
    severity: Severity,
    ctx: &DocContext<'_>,
) -> Vec<Finding> {
    let opts = &rule.options;
    let attr = opt_str(opts, "attr").unwrap_or("variant").to_string();
    let group_by = opt_str(opts, "groupBy").map(str::to_string);
    let min_per_group = opt_num(opts, "minPerGroup").unwrap_or(2.0) as u32;
    let min_share = opt_num(opts, "minShare").unwrap_or(0.0);
    let min_lines = opt_num(opts, "minLines").unwrap_or(10.0) as u32;
    let code = diagnostic_code(&rule.id);

    // Inert if no line in the document carries the tracked attr (spec §7).
    let attr_seen = ctx.tables.lines.iter().any(|l| l.attrs.contains_key(&attr));
    if !attr_seen {
        return Vec::new();
    }

    let mut out = Vec::new();

    // Group-based check (spec §7 "With `groupBy`: groups with `count <
    // minPerGroup` fire.").
    if let Some(gb) = &group_by {
        if let Some(rows) = ctx.tables.groups.get(gb) {
            for g in rows {
                if g.count < min_per_group {
                    out.push(Finding {
                        code: code.clone(),
                        severity,
                        message: format!(
                            "group `{}`={}: only {} line(s) (need {})",
                            g.attr, g.key, g.count, min_per_group
                        ),
                        span: g.first_line_span,
                        related: Vec::new(),
                        layer: None,
                    });
                }
            }
        }
    }

    // Speaker share check (spec §7 "With `minShare > 0`: speakers whose
    // `attrShare[attr] < minShare` fire.").
    if min_share > 0.0 {
        for sp in ctx.tables.speakers.values() {
            if sp.lines < min_lines {
                continue;
            }
            let share = sp.attrShare.get(&attr).copied().unwrap_or(0.0);
            if share < min_share {
                out.push(Finding {
                    code: code.clone(),
                    severity,
                    message: format!(
                        "speaker `{}`: {} share {}% below floor {}%",
                        sp.speaker,
                        attr,
                        (share * 100.0).round() as i64,
                        (min_share * 100.0).round() as i64,
                    ),
                    span: sp.first_line_span,
                    related: Vec::new(),
                    layer: None,
                });
            }
        }
    }

    out
}

fn eval_asset_exists(
    rule: &ResolvedRule,
    severity: Severity,
    ctx: &DocContext<'_>,
) -> Vec<Finding> {
    let opts = &rule.options;
    // Directive-tag → provider-id map.
    let providers_map = match opts.get(serde_yaml::Value::from("providers")) {
        Some(serde_yaml::Value::Mapping(m)) => m.clone(),
        _ => return Vec::new(),
    };
    if providers_map.is_empty() {
        return Vec::new();
    }
    // Sentinel values that skip validation.
    let sentinels: BTreeSet<String> = match opts.get(serde_yaml::Value::from("sentinels")) {
        Some(serde_yaml::Value::Sequence(seq)) => seq
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_ascii_lowercase()))
            .collect(),
        _ => ["clear", "empty", "false", "none", "null", "stop"]
            .iter()
            .map(|s| s.to_string())
            .collect(),
    };
    let code = diagnostic_code(&rule.id);

    let mut out = Vec::new();
    for d in ctx.directives {
        let Some(provider_val) = providers_map.get(serde_yaml::Value::from(d.tag.as_str())) else {
            continue;
        };
        let Some(provider) = provider_val.as_str() else {
            continue;
        };
        let Some(asset_id) = &d.assetId else { continue };
        // `@ref` values are checked by the semantic layer, not lint.
        if !d.asset_is_static {
            continue;
        }
        if sentinels.contains(&asset_id.to_ascii_lowercase()) {
            continue;
        }
        match ctx.providers.contains(provider, asset_id) {
            lute_manifest::provider::IdStatus::Fresh => {}
            lute_manifest::provider::IdStatus::Absent => {
                out.push(Finding {
                    code: code.clone(),
                    severity,
                    message: format!(
                        "asset id `{}` not in provider `{}` (`::{}`)",
                        asset_id, provider, d.tag
                    ),
                    span: d.assetId_span,
                    related: Vec::new(),
                    layer: None,
                });
            }
            lute_manifest::provider::IdStatus::Stale => {
                out.push(Finding {
                    code: code.clone(),
                    // Downgrade to Warning per spec §7.
                    severity: Severity::Warning,
                    message: format!(
                        "asset id `{}` not in provider `{}` (`::{}`) — catalog is stale",
                        asset_id, provider, d.tag
                    ),
                    span: d.assetId_span,
                    related: Vec::new(),
                    layer: None,
                });
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Value conversion — metric rows into `eval::Value`
// ---------------------------------------------------------------------------

fn num(n: f64) -> Value {
    Value::Num(n)
}

fn line_value(l: &LineRow) -> Value {
    let mut m = BTreeMap::new();
    m.insert("words".into(), num(l.words as f64));
    m.insert("chars".into(), num(l.chars as f64));
    m.insert("speaker".into(), Value::Str(l.speaker.clone()));
    let mut attrs = BTreeMap::new();
    for (k, v) in &l.attrs {
        attrs.insert(k.clone(), Value::Str(v.clone()));
    }
    m.insert("attrs".into(), Value::Map(attrs));
    Value::Map(m)
}

fn shot_value(s: &ShotRow) -> Value {
    let mut m = BTreeMap::new();
    m.insert("index".into(), num(s.index as f64));
    m.insert("title".into(), Value::Str(s.title.clone()));
    m.insert("dialogueLines".into(), num(s.dialogueLines as f64));
    m.insert("words".into(), num(s.words as f64));
    m.insert(
        "firstStagingTag".into(),
        Value::Str(s.firstStagingTag.clone()),
    );
    Value::Map(m)
}

fn scene_value(s: &SceneRow) -> Value {
    let mut m = BTreeMap::new();
    m.insert("dialogueLines".into(), num(s.dialogueLines as f64));
    m.insert("words".into(), num(s.words as f64));
    m.insert("bodyNodes".into(), num(s.bodyNodes as f64));
    m.insert("directives".into(), num(s.directives as f64));
    m.insert("sets".into(), num(s.sets as f64));
    m.insert("choices".into(), num(s.choices as f64));
    m.insert("shots".into(), num(s.shots as f64));
    m.insert("maxLineWords".into(), num(s.maxLineWords as f64));
    m.insert("avgLineWords".into(), num(s.avgLineWords));
    m.insert("dialogueRatio".into(), num(s.dialogueRatio));
    Value::Map(m)
}

fn speaker_value(s: &SpeakerRow) -> Value {
    let mut m = BTreeMap::new();
    m.insert("speaker".into(), Value::Str(s.speaker.clone()));
    m.insert("lines".into(), num(s.lines as f64));
    m.insert("words".into(), num(s.words as f64));
    let mut axis = BTreeMap::new();
    for (k, v) in &s.axis {
        axis.insert(k.clone(), axis_value(v));
    }
    m.insert("axis".into(), Value::Map(axis));
    let mut share = BTreeMap::new();
    for (k, v) in &s.attrShare {
        share.insert(k.clone(), num(*v));
    }
    m.insert("attrShare".into(), Value::Map(share));
    Value::Map(m)
}

fn axis_value(ax: &AxisStats) -> Value {
    let mut m = BTreeMap::new();
    m.insert("run".into(), num(ax.run as f64));
    m.insert("runValue".into(), Value::Str(ax.runValue.clone()));
    m.insert("streaks".into(), num(ax.streaks as f64));
    m.insert("streakAvg".into(), num(ax.streakAvg));
    m.insert("distinct".into(), num(ax.distinct as f64));
    m.insert("top".into(), top_value(&ax.top));
    Value::Map(m)
}

fn top_value(t: &TopStats) -> Value {
    let mut m = BTreeMap::new();
    m.insert("value".into(), Value::Str(t.value.clone()));
    m.insert("count".into(), num(t.count as f64));
    m.insert("share".into(), num(t.share));
    Value::Map(m)
}

fn group_value(g: &GroupRow) -> Value {
    let mut m = BTreeMap::new();
    m.insert("attr".into(), Value::Str(g.attr.clone()));
    m.insert("key".into(), Value::Str(g.key.clone()));
    m.insert("count".into(), num(g.count as f64));
    m.insert("speakers".into(), num(g.speakers as f64));
    Value::Map(m)
}

fn project_value(p: &ProjectRow) -> Value {
    let mut m = BTreeMap::new();
    m.insert("scenes".into(), num(p.scenes as f64));
    let mut sw = BTreeMap::new();
    sw.insert("min".into(), num(p.sceneWords.min));
    sw.insert("max".into(), num(p.sceneWords.max));
    sw.insert("mean".into(), num(p.sceneWords.mean));
    sw.insert("stddev".into(), num(p.sceneWords.stddev));
    m.insert("sceneWords".into(), Value::Map(sw));
    m.insert("spreadRatio".into(), num(p.spreadRatio));
    Value::Map(m)
}

fn mapping_to_value(m: &serde_yaml::Mapping) -> Value {
    let mut out = BTreeMap::new();
    for (k, v) in m {
        let Some(k) = k.as_str() else {
            continue;
        };
        out.insert(k.to_string(), yaml_to_value(v));
    }
    Value::Map(out)
}

fn yaml_to_value(v: &serde_yaml::Value) -> Value {
    match v {
        serde_yaml::Value::Null => Value::Null,
        serde_yaml::Value::Bool(b) => Value::Bool(*b),
        serde_yaml::Value::Number(n) => n.as_f64().map(Value::Num).unwrap_or(Value::Null),
        serde_yaml::Value::String(s) => Value::Str(s.clone()),
        serde_yaml::Value::Sequence(seq) => Value::List(seq.iter().map(yaml_to_value).collect()),
        serde_yaml::Value::Mapping(m) => mapping_to_value(m),
        serde_yaml::Value::Tagged(t) => yaml_to_value(&t.value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_data_yaml_parses() {
        for yaml in CORE_DATA_YAML {
            let _decl: LintRuleDecl = serde_yaml::from_str(yaml).expect(yaml);
        }
    }
}
