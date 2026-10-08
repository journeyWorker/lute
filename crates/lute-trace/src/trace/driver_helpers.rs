use super::*;
pub(super) fn bare_decision(
    construct: &str,
    id: &str,
    span: Span,
    outcome: String,
    guard: Option<String>,
    forced: bool,
) -> Decision {
    Decision {
        construct: construct.to_string(),
        id: id.to_string(),
        span,
        outcome,
        guard: guard.filter(|g| !g.is_empty()),
        forced,
        auto: false,
        eligible: Vec::new(),
        authored_id: None,
        authored_guard: None,
        component: None,
    }
}

/// A plugin call as the walk made it — `::tag{field="value" n=3}` from its
/// IR record's resolved `fields` — rather than the bare `<tag>` a staging
/// directive reads as (AS-21: a plugin tag is no markup).
pub(super) fn plugin_call(tag: &str, cmd: Option<&Json>) -> String {
    let fields = cmd
        .and_then(|c| c.get("fields"))
        .and_then(Json::as_object)
        .map(|f| {
            f.iter()
                .map(|(k, v)| match v {
                    Json::String(s) if s == lute_manifest::semantics::beats::OCCASION_TARGET => {
                        format!("{k}={s}")
                    }
                    Json::String(s) => format!("{k}={s:?}"),
                    Json::Number(_) => format!("{k}={}", json_text(Some(v))),
                    v => format!("{k}={v}"),
                })
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default();
    format!("::{tag}{{{fields}}}")
}

/// A record value as display text (`unknown` for an undecided one).
pub(super) fn json_text(v: Option<&Json>) -> String {
    match v {
        Some(Json::String(s)) => s.clone(),
        Some(Json::Bool(b)) => b.to_string(),
        Some(Json::Number(n)) => n
            .as_f64()
            .map(report::format_num)
            .unwrap_or_else(|| n.to_string()),
        _ => "unknown".to_string(),
    }
}
