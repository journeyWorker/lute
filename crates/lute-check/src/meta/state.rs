use super::*;
/// dsl 0.28.0 §1 (T1-12): why `path` cannot be declared in `state:` — it is
/// a name the engine owns — or `None` when the namespace is the author's.
/// A non-tier root with no engine meaning (`foo.bar`) is `None` here too;
/// [`namespace_of`] refuses it.
pub(crate) fn engine_namespace(path: &str) -> Option<String> {
    let segs: Vec<&str> = path.split('.').collect();
    let why = match segs.as_slice() {
        ["scene", "choices", ..] => {
            "`scene.choices.<id>` records which choice a branch or hub took; the engine declares \
             it from the `<branch>`/`<hub>` and writes it when a choice is taken"
        }
        ["scene", "visited", ..] => {
            "`scene.visited.<hub>.<choice>` records whether that hub choice was taken in this \
             presentation; the engine declares it from the `<hub>` and writes it when the choice \
             is picked"
        }
        ["occasion", ..] => {
            "`occasion.*` describes the raise a beat answers — its `target` and `payload` come \
             from the raise, and payload fields are declared on the occasion"
        }
        ["clock", ..] => {
            "`clock.*` is derived from the declared `clock:`; declare the clock's own day and \
             slot paths under `run.` or `user.` instead"
        }
        ["entry", ..] => "`entry.<id>.read` is recorded by the engine when an entry is read",
        ["prev", ..] => {
            "`prev.*` is the engine's read-only mirror of the previous run's or season's values"
        }
        ["quest", _] | ["quest"] => {
            "a quest path is `quest.<questId>.<field>`, a field of one quest; for state shared \
             across quests use `run.` or `user.`"
        }
        ["season", _] | ["season"] => {
            "a season path is `season.<name>.<field>`, with `<name>` declared in `seasons:`"
        }
        _ => return None,
    };
    Some(format!("state path `{path}` cannot be declared: {why}"))
}

pub(crate) fn namespace_of(path: &str) -> Option<Namespace> {
    match map_prefix(path) {
        "scene" => Some(Namespace::Scene),
        "run" => Some(Namespace::Run),
        "user" => Some(Namespace::User),
        "app" => Some(Namespace::App),
        "quest" => Some(Namespace::Quest),
        "season" => Some(Namespace::Season),
        _ => None,
    }
}

pub(super) fn get_str(map: &serde_yaml::Mapping, key: &str) -> Option<String> {
    map.get(yaml_key(key))
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

pub(super) fn get_i64(map: &serde_yaml::Mapping, key: &str) -> Option<i64> {
    map.get(yaml_key(key)).and_then(|v| v.as_i64())
}

/// `uses`/`extends` (dsl §9.2) may each be a single ref or a list of refs;
/// normalize to a Vec.
pub(super) fn get_ref_list(map: &serde_yaml::Mapping, key: &str) -> Vec<String> {
    match map.get(yaml_key(key)) {
        Some(serde_yaml::Value::String(s)) => vec![s.clone()],
        Some(serde_yaml::Value::Sequence(items)) => items
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect(),
        _ => Vec::new(),
    }
}

pub(super) fn get_sub_map(map: &serde_yaml::Mapping, key: &str) -> BTreeMap<String, serde_yaml::Value> {
    match map.get(yaml_key(key)) {
        Some(serde_yaml::Value::Mapping(m)) => m
            .iter()
            .filter_map(|(k, v)| k.as_str().map(|k| (k.to_string(), v.clone())))
            .collect(),
        _ => BTreeMap::new(),
    }
}

/// A component file's `params:` (dsl §13) is a YAML MAPPING (`{ who: <type> }`)
/// read in SOURCE order (`serde_yaml::Mapping` is insertion-ordered), the same
/// spelling as a def's `params:` (dsl §8.1). Each value deserializes to a
/// manifest [`Type`] via the same serde path `Type` uses.
///
/// dsl 0.10.0 §12.4: the LONG form `{ type: X }` is accepted as a synonym for
/// `X`, for every spelling `X` the shared deserializer admits — that is how
/// `state:` and `defs:` entries are written, and `components-and-extends.md`
/// says a component param is typed exactly like a def param. dsl 0.26.0 §3.3:
/// the long form MAY also carry `default:` — a scalar literal, or a string
/// `"@def"` naming a def the host resolves — the argument an omitted param
/// takes. Any other key stays malformed.
///
/// dsl 0.24.0 §4: a component param MAY be typed `speaker` (a cast id). It is
/// entered in the params list as `string` and ALSO named in the returned
/// speaker list; `speaker` is a component-param spelling only, never a
/// manifest [`Type`].
///
/// Returns the valid `(name, type)` pairs, the `speaker` param names, the
/// declared defaults, and a `malformed` flag that is `true`
/// when `params:` is PRESENT but any part of it is invalid — not a mapping, a
/// non-string key, a value that fails `Type` deserialization, or a `default:`
/// that is no scalar. The caller
/// (component resolver) turns a set flag into `E-COMPONENT-PARSE` so a malformed
/// signature is never silently shrunk. Absent `params:` ⇒ `(empty, empty, empty, false)`.
/// Never panics.
pub(super) type ParsedParams = (
    Vec<DefParam>,
    Vec<String>,
    BTreeMap<String, lute_syntax::ast::AttrValue>,
    bool,
);
pub(super) fn get_params(map: &serde_yaml::Mapping, key: &str) -> ParsedParams {
    let mut defaults = BTreeMap::new();
    let Some(raw) = map.get(yaml_key(key)) else {
        return (Vec::new(), Vec::new(), defaults, false); // absent — fine (no params)
    };
    let Some(pm) = raw.as_mapping() else {
        return (Vec::new(), Vec::new(), defaults, true); // present but not a mapping — malformed
    };
    let mut params = Vec::new();
    let mut speakers = Vec::new();
    let mut malformed = false;
    for (k, tv) in pm.iter() {
        let Some(name) = k.as_str() else {
            malformed = true; // non-string key
            continue;
        };
        let (tv, default) = match unwrap_long_form(tv) {
            Some((ty, default)) => (ty, default),
            None => (tv, None),
        };
        if let Some(default) = default {
            match param_default(default) {
                Some(v) => {
                    defaults.insert(name.to_string(), v);
                }
                None => malformed = true,
            }
        }
        if tv.as_str() == Some("speaker") {
            speakers.push(name.to_string());
            params.push(DefParam {
                name: name.to_string(),
                ty: Type::Str,
            });
            continue;
        }
        match serde_yaml::from_value::<Type>(tv.clone()) {
            Ok(ty) => params.push(DefParam {
                name: name.to_string(),
                ty,
            }),
            Err(_) => malformed = true, // value is not a valid Type
        }
    }
    (params, speakers, defaults, malformed)
}

/// dsl 0.26.0 §3.3: a param `default:` as the `::use` argument it stands for:
/// `"@name"` is a def reference (resolved in the host, like `p=@name`), any
/// other scalar the literal `p="…"` would give. `None` for a non-scalar.
pub(super) fn param_default(v: &serde_yaml::Value) -> Option<lute_syntax::ast::AttrValue> {
    use lute_syntax::ast::{AttrValue, CelKind, CelSlot};
    let text = match v {
        serde_yaml::Value::String(s) => s.clone(),
        serde_yaml::Value::Bool(b) => b.to_string(),
        serde_yaml::Value::Number(n) => n.to_string(),
        _ => return None,
    };
    // The span is the component file's, meaningless in a host: every binding
    // site re-anchors the argument at its `::use`
    // (`component_effects::use_args_for`).
    let at = lute_core_span::Span {
        byte_start: 0,
        byte_end: 0,
        line: 1,
        column: 1,
        utf16_range: (0, 0),
    };
    Some(if text.starts_with('@') {
        AttrValue::Ref(CelSlot::raw(CelKind::AttrValue, text, at))
    } else {
        AttrValue::Str(text)
    })
}

/// dsl 0.10.0 §12.4: unwrap the long form `{ type: X }` to `X` — with dsl
/// 0.26.0 §3.3's optional `default:` beside it. `None` for anything else,
/// including a `type:` wrapper carrying another key — the caller then hands
/// the ORIGINAL value to the `Type` deserializer, which rejects it, so an
/// unwrap miss is never a silent acceptance.
///
/// There is no ambiguity to resolve: `Type` has no `type` variant
/// (`lute-manifest/src/types.rs`), so `{ type: … }` is not already a legal
/// spelling and this cannot shadow one.
pub(super) fn unwrap_long_form(
    tv: &serde_yaml::Value,
) -> Option<(&serde_yaml::Value, Option<&serde_yaml::Value>)> {
    let m = tv.as_mapping()?;
    let ty = m.get(yaml_key("type"))?;
    let default = m.get(yaml_key("default"));
    (m.len() == 1 + usize::from(default.is_some())).then_some((ty, default))
}
