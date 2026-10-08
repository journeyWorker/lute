use super::*;

/// Lift and validate the `facts:` and `rules:` datalog blocks.
///
/// Invalid entries are omitted from the typed declarations while retaining a
/// diagnostic at the entry's textual span. A malformed rule also records its
/// head so downstream checks can distinguish failed derivations.
pub(super) fn lift_datalog(
    meta: &Meta,
    map: &serde_yaml::Mapping,
    typed: &mut TypedMeta,
    diags: &mut Vec<Diagnostic>,
    span: Span,
) {
    // dsl 0.3.0 §4/§7.1, T5: entries are quoted strings. An unquoted
    // `head :- body` is a YAML mapping rather than a scalar.
    match map.get(yaml_key("facts")) {
        None | Some(serde_yaml::Value::Null) => {}
        Some(serde_yaml::Value::Sequence(seq)) => {
            for entry in seq {
                let Some(raw) = entry.as_str() else {
                    diags.push(err_at(
                        "E-DATALOG-PARSE",
                        "`facts:` entries must be quoted strings (dsl 0.3.0 §4 Quoting)"
                            .to_string(),
                        span,
                    ));
                    continue;
                };
                match lute_manifest::fact::parse_fact(raw) {
                    Ok(fact) => typed.rel_facts.push(FactDecl {
                        fact,
                        raw: raw.to_string(),
                        span: meta_key_span(meta, raw),
                    }),
                    Err(lute_manifest::fact::DatalogError::Malformed { msg, .. }) => {
                        diags.push(err_at(
                            "E-DATALOG-PARSE",
                            format!("malformed fact `{raw}`: {msg}"),
                            meta_key_span(meta, raw),
                        ));
                    }
                    Err(lute_manifest::fact::DatalogError::FunctionTerm { name, .. }) => {
                        diags.push(err_at(
                            "E-DATALOG-FUNCTION",
                            format!(
                                "fact `{raw}` uses a function/compound term `{name}(...)`; \
                                 facts admit only ground identifiers/booleans (dsl §7.1)"
                            ),
                            meta_key_span(meta, raw),
                        ));
                    }
                }
            }
        }
        Some(_) => diags.push(err_at(
            "E-DATALOG-PARSE",
            "`facts:` must be a list of quoted fact strings (dsl 0.3.0 §4)".to_string(),
            span,
        )),
    }
    match map.get(yaml_key("rules")) {
        None | Some(serde_yaml::Value::Null) => {}
        Some(serde_yaml::Value::Sequence(seq)) => {
            for entry in seq {
                let Some(raw) = entry.as_str() else {
                    diags.push(err_at(
                        "E-DATALOG-PARSE",
                        "`rules:` entries must be quoted strings (dsl 0.3.0 §4 Quoting)"
                            .to_string(),
                        span,
                    ));
                    continue;
                };
                let parsed = lute_syntax::datalog::parse_rule(raw);
                if parsed.is_err() {
                    let head: String = raw
                        .trim_start()
                        .chars()
                        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                        .collect();
                    if head.starts_with(|c: char| c.is_ascii_alphabetic()) {
                        typed.rel_rule_failed_heads.insert(head);
                    }
                }
                match parsed {
                    Ok(rule) => typed.rel_rules.push(RuleDecl {
                        rule,
                        raw: raw.to_string(),
                        span: scalar_span(meta, raw),
                    }),
                    Err(lute_manifest::fact::DatalogError::Malformed { msg, .. }) => {
                        diags.push(err_at(
                            "E-DATALOG-PARSE",
                            format!("malformed rule `{raw}`: {msg}"),
                            scalar_span(meta, raw),
                        ));
                    }
                    Err(lute_manifest::fact::DatalogError::FunctionTerm { name, .. }) => {
                        diags.push(err_at(
                            "E-DATALOG-FUNCTION",
                            format!(
                                "rule `{raw}` uses a function/compound term `{name}(...)`; \
                                 rule terms admit only Var/Const/bool (dsl §7.1)"
                            ),
                            scalar_span(meta, raw),
                        ));
                    }
                }
            }
        }
        Some(_) => diags.push(err_at(
            "E-DATALOG-PARSE",
            "`rules:` must be a list of quoted rule strings (dsl 0.3.0 §4)".to_string(),
            span,
        )),
    }
}
