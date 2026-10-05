use super::*;

pub(super) fn parse_state(
    meta: &Meta,
    map: &serde_yaml::Mapping,
    typed: &mut TypedMeta,
    mut diags: &mut Vec<Diagnostic>,
) {
    let err = |code: &str, message: String| Diagnostic {
        code: code.to_string(),
        severity: Severity::Error,
        message,
        evidence: None,
        span: meta.span,
        layer: Layer::Content,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    };
    let err_at = |code: &str, message: String, span: Span| Diagnostic {
        code: code.to_string(),
        severity: Severity::Error,
        message,
        evidence: None,
        span,
        layer: Layer::Content,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    };

    // Parse the inline `state:` schema (dsl §9.3).
    if let Some(state_val) = map.get(yaml_key("state")) {
        match state_val {
            serde_yaml::Value::Null => {}
            serde_yaml::Value::Mapping(state_map) => {
                for (path_key, decl_val) in state_map.iter() {
                    let Some(path) = path_key.as_str() else {
                        diags.push(err(
                            "E-STATE-DECL",
                            "state path keys must be strings".to_string(),
                        ));
                        continue;
                    };
                    // dsl 0.28.0 §1 (T1-12): a path outside the author tiers,
                    // or inside one the engine owns (`scene.choices.*`,
                    // `scene.visited.*`, a quest or season path missing its
                    // id/name segment), is not declarable — anchored at its key.
                    let (Some(namespace), None) = (namespace_of(path), engine_namespace(path))
                    else {
                        let why = engine_namespace(path).unwrap_or_else(|| {
                            format!(
                                "state path `{path}` must begin with an author tier: `run.`, \
                                 `user.`, `scene.`, `app.`, `season.<name>.` or `quest.<id>.`"
                            )
                        });
                        diags.push(err_at("E-STATE-NAMESPACE", why, meta_key_span(meta, path)));
                        continue;
                    };
                    // dsl 0.2.0 §5.2/§9.3: `quest.<id>.state` / `quest.<id>.objectives.<oid>.done`
                    // are RESERVED — implicitly declared and MUST NOT be author-declared,
                    // regardless of whether THIS document owns a matching `<quest id>` (a
                    // shape check, not a doc-scope one; mirrors the name check /
                    // `narrativeTime` below). Skip the decl install entirely so a later read
                    // of `path` resolves via the reserved-path fallback, never a phantom
                    // author-typed decl. The `check.rs:410-427` collision guard (an imported/
                    // sibling-document quest's schema, folded later) still catches the
                    // fold-order case this shape check cannot see.
                    if is_reserved_quest_path(path) {
                        diags.push(err_at(
                            "E-QUEST-RESERVED-DECL",
                            format!(
                                "state path `{path}` collides with an implicitly-declared \
                                 reserved quest field (dsl 0.2.0 §5.2); it must not be \
                                 author-declared in `state:`"
                            ),
                            meta_key_span(meta, path),
                        ));
                        continue;
                    }
                    // dsl 0.30.0 §1: each state-path segment is a name; a
                    // character outside the rule is E-PATH-IDENT. Still record
                    // the decl below so downstream reads don't cascade to
                    // E-UNDECLARED.
                    if let Some(message) =
                        lute_manifest::ident::dotted_name_fault("state path", path)
                    {
                        diags.push(err_at(E_PATH_IDENT, message, meta_key_span(meta, path)));
                    }
                    // dsl 0.28.0 §1 (T1-2): a state row's keys are closed. An
                    // unknown one (`defualt:`, `ownr:`, `reserved:`, `tier:`)
                    // was dropped silently — the default, the owner or the
                    // tier the author wrote never applied. The row is still
                    // installed from the keys it does spell right, and the
                    // path is `faulty`: its reads are judged no further.
                    for (key, message) in unknown_state_row_keys(path, decl_val) {
                        diags.push(err_at(
                            "E-STATE-DECL",
                            message,
                            meta_path_span(meta, &["state", path, &key]),
                        ));
                        typed.state.faulty.insert(path.to_string());
                    }
                    match serde_yaml::from_value::<StateDeclRaw>(decl_val.clone()) {
                        Ok(raw) if matches!(raw.ty, Type::NarrativeTime) => {
                            // D11: `narrativeTime` is engine-surfaced only (a
                            // plugin capability's `state_shapes` anchor path,
                            // dsl 0.3.0 §6) — never author-declarable. Skip
                            // the decl entirely so a later read of `path`
                            // falls back to plain `E-UNDECLARED`, never a
                            // phantom narrative-time-typed path.
                            diags.push(err_at(
                                crate::temporal::E_TEMPORAL_ARG,
                                format!(
                                    "state path `{path}` cannot declare `type: narrativeTime`; \
                                     narrative-time paths are engine-surfaced (plugin capability \
                                     state shapes) only — author state is number|bool|string|enum \
                                     (dsl 0.3.0 §6, D11)"
                                ),
                                meta_key_span(meta, path),
                            ));
                        }
                        Ok(raw)
                            if matches!(
                                raw.ty,
                                Type::List(_) | Type::Record(_) | Type::Map { .. }
                            ) =>
                        {
                            // dsl 0.8.0 §4: author `state:` is SCALAR
                            // (`number|bool|string|enum`). `StateDeclRaw`
                            // deserializes the full `Type` union because the
                            // SAME shape carries plugin `state_shapes`
                            // expansions (where a `record`/`map` slot is
                            // legitimate) — so the scalar-only rule the
                            // normative text has always stated is enforced
                            // HERE, at the author-decl site, not in the
                            // deserializer. Mirrors the `narrativeTime` arm
                            // above: emit and SKIP the install, so a later
                            // read falls back to plain `E-UNDECLARED` rather
                            // than resolving through a phantom
                            // collection-typed slot (and, via
                            // `set_op::descend`, inventing field types for
                            // every path under it).
                            diags.push(err_at(
                                E_STATE_COLLECTION,
                                format!(
                                    "state path `{path}` cannot declare a collection type \
                                     (`list`/`record`/`map`); author state is \
                                     (int|double|bool|string|enum) — model collections as \
                                     `relations:` (dsl 0.3.0 §3) or a plugin `state_shapes` slot"
                                ),
                                meta_key_span(meta, path),
                            ));
                        }
                        Ok(raw) => {
                            if let Type::Enum(members) = &raw.ty {
                                for member in members {
                                    if let Some(message) =
                                        lute_manifest::ident::name_fault("enum member", member)
                                    {
                                        diags.push(err_at(
                                            E_PATH_IDENT,
                                            message,
                                            meta_key_span(meta, path),
                                        ));
                                    }
                                }
                            }
                            let decl = StateDecl {
                                ty: raw.ty,
                                default: None,
                                namespace,
                                owner: raw.owner,
                            };
                            let key_span = meta_key_span(meta, path);
                            match raw.per {
                                None => {
                                    let default = match raw.default {
                                        Some(Literal::Map(_)) => {
                                            diags.push(err_at(
                                                "E-STATE-DECL",
                                                format!(
                                                    "invalid state declaration for `{path}`: a map-valued \
                                                     `default:` gives each member of a `per:` family its own \
                                                     default, and `{path}` has no `per:` — give one scalar \
                                                     value (dsl 0.24.0 §3)"
                                                ),
                                                key_span,
                                            ));
                                            None
                                        },
                                        other => {
                                            let authored = other.is_some();
                                            let value =
                                                scalar_default(path, &decl.ty, other, &mut diags, key_span);
                                            if authored && value.is_none() {
                                                typed.state.faulty.insert(path.to_string());
                                            }
                                            value
                                        },
                                    };
                                    // dsl 0.28.0 §1 (T1-2): an inline enum's
                                    // `default:` must be one of its members —
                                    // `c` in `{ enum: [a, b] }` played as a
                                    // value no `<match>` arm can take.
                                    let default = match (&decl.ty, default) {
                                        (Type::Enum(ms), Some(Literal::Str(d)))
                                            if !ms.contains(&d) =>
                                        {
                                            let hint = lute_manifest::suggest::did_you_mean(
                                                &d,
                                                ms.iter().map(String::as_str),
                                            );
                                            diags.push(err_at(
                                                "E-STATE-DECL",
                                                format!(
                                                    "`{path}`'s `default: {d}` is not one of its \
                                                     members [{}]{hint}",
                                                    ms.join(", ")
                                                ),
                                                meta_path_span(meta, &["state", path, "default"]),
                                            ));
                                            typed.state.faulty.insert(path.to_string());
                                            None
                                        }
                                        (_, default) => default,
                                    };
                                    typed
                                        .state
                                        .decls
                                        .insert(path.to_string(), StateDecl { default, ..decl });
                                }
                                // dsl 0.24.0 §3: one decl per member of a
                                // closed kind. A kind this document does not
                                // declare is resolved against the schemas it
                                // imports ([`expand_per_pending`], dsl 0.28.0).
                                Some(kind) if !typed.rel_kinds.kinds.contains_key(&kind) => {
                                    typed.per_pending.push(PendingPer {
                                        path: path.to_string(),
                                        kind,
                                        decl,
                                        default: raw.default,
                                        span: key_span,
                                    });
                                }
                                Some(kind) => match per_members(&typed.rel_kinds, &kind) {
                                    Ok(members) => {
                                        let defaults = per_member_defaults(
                                            path,
                                            &kind,
                                            &members,
                                            &decl.ty,
                                            raw.default,
                                            &mut diags,
                                            key_span,
                                        );
                                        for (m, default) in members.iter().zip(defaults) {
                                            typed.state.decls.insert(
                                                format!("{path}.{m}"),
                                                StateDecl {
                                                    default,
                                                    ..decl.clone()
                                                },
                                            );
                                        }
                                        typed.state_index.insert(path.to_string(), kind);
                                    }
                                    Err(why) => diags.push(err_at(
                                        "E-STATE-DECL",
                                        per_fault(path, &kind, why),
                                        key_span,
                                    )),
                                },
                            }
                        }
                        // dsl 0.5.0 §2.2 / #21 T10.2: this arm forwarded
                        // `serde_yaml`'s own error — "invalid type: unit
                        // variant, expected newtype variant", or "expected
                        // struct StateDeclRaw". Neither "unit variant" nor
                        // "newtype variant" nor `StateDeclRaw` occurs
                        // anywhere in the docs, the language or YAML, and
                        // none of them names the wrong key, the missing key,
                        // the legal shape, or the one nesting level at issue.
                        // The neighbour `E_STATE_COLLECTION` above already
                        // writes its message for the author; so does this
                        // one, through the same `err_at`/`meta_key_span`
                        // pair, so it anchors at the offending key rather
                        // than at the whole meta block.
                        Err(_) => diags.push(err_at(
                            "E-STATE-DECL",
                            state_decl_message(path, decl_val),
                            meta_key_span(meta, path),
                        )),
                    }
                }
            }
            _ => diags.push(err(
                "E-STATE-DECL",
                "`state` must be a mapping of path to declaration".to_string(),
            )),
        }
    }
}
