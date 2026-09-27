//! `lute context`: the project-resolved authoring surface, as JSON or a
//! human outline.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::Path;
use std::process::ExitCode;

use lute_check::{fold_env, CheckInput, Namespace, RelVocab};
use lute_manifest::relations::KindShape;
use lute_manifest::types::{Literal, Type};

use crate::context;
use crate::input::{build_input, BuiltInput};
use crate::output::write_stdout;

/// Emit the project-resolved AUTHORING SURFACE for `file`: everything an AI
/// needs to WRITE valid Lute against THIS file's project — the resolved
/// directives/attrs/enums/asset-kinds/providers, the FOLDED state schema (author
/// `state:` ∪ `uses:` imports ∪ implicit `<branch>`/`<hub>` choice+visited slots
/// ∪ plugin-declared slots), the imported components, and the `capabilityVersion`
/// they were resolved under.
///
/// Reuses the SAME resolution `check`/`compile` use — `build_input` (project +
/// provider + import resolution) and `fold_env` (the folded schema) — so the
/// surface never diverges from what the checker validates against. It is a
/// capability QUERY, not validation: it emits the surface regardless of any
/// document diagnostics (`fold_env` is pure/total). Exit `0` on success, `2` on
/// an I/O failure (unreadable file), matching `run_check`.
pub(crate) fn run_context(
    file: &Path,
    json: bool,
    providers: Option<&Path>,
    project: Option<&Path>,
    permission_profile: Option<&str>,
) -> ExitCode {
    // FS-F2: the same project `lute check` resolves the file against.
    let discovered = crate::project::discover_project(file, project);
    let project = project.or(discovered.as_deref());
    let Some(built) = build_input(file, providers, project, permission_profile) else {
        return ExitCode::from(2);
    };
    built.report_project_diags();
    let BuiltInput {
        input,
        resolve_error,
        ..
    } = built;
    // plugin 0.0.2 §2: an `E-` capability-resolution diagnostic (bad plugin
    // option, missing active plugin, bad identity template) is a build-failing
    // error; it printed above, and it MUST gate here or it would pass silently.
    if resolve_error {
        return ExitCode::from(1);
    }
    // Parse + fold exactly as `compile` does (minus codegen): the folded env's
    // `.state` is the document's valid readable/writable state surface. No CEL
    // fill is needed — the schema fold reads structural ids/attrs, not CEL slots.
    let (doc, _) = lute_syntax::parse(&input.text);
    let (folded, _, _) = fold_env(&doc, &input);
    // The ACTUAL implicit choice slots (`scene.choices.<branchId|hubId>`): reuse
    // compile's own discriminator so the surface's enum domains match the compiled
    // state table byte-for-byte (choice ids ∪ `unset`) — no divergence. The set is
    // expansion-invariant, so the raw parsed `doc` yields the same paths.
    let branch_paths = lute_compile::collect_branch_paths(&doc);
    // The reserved quest paths this document REFERENCES (any CEL slot) —
    // `lute-trace`'s own walk, the one `trace --state` admits by — joined in
    // `extend_surface` with every reserved path of every declared quest.
    let reserved_quest_paths = lute_trace::collect_referenced_reserved_quest_paths(&doc);
    let mut surface = authoring_surface(
        &input,
        &folded.env.state,
        &folded.env.rel_vocab,
        &branch_paths,
    );
    // dsl 0.22.0 §13: defs, built-in directives, project ids; dsl 0.27.0:
    // beat / quest keys, clock, terminal, seasons, the manifest's chapters.
    context::extend_surface(
        &mut surface,
        &folded,
        &input,
        &reserved_quest_paths,
        file,
        project,
    );
    if json {
        match serde_json::to_string_pretty(&surface) {
            Ok(s) => {
                if write_stdout(&format!("{s}\n")).is_err() {
                    return ExitCode::from(2);
                }
            }
            Err(e) => {
                eprintln!("lute: failed to serialize context: {e}");
                return ExitCode::from(2);
            }
        }
    } else if write_stdout(&context_outline(&surface)).is_err() {
        return ExitCode::from(2);
    }
    ExitCode::SUCCESS
}

/// Assemble the deterministic JSON authoring surface: every map is a BTreeMap
/// (key-sorted by construction) and every array is emitted in a stable order
/// (directives by name, state paths by path, components by name; attrs/params in
/// declaration order). `enums`/`assetKinds`/`providers` come straight off the
/// string (see `attr_type_str`/`state_type_str`). `branch_paths` marks the ACTUAL
/// implicit choice slots so their enum domains gain `unset` (matching compile).
/// `rel_vocab` is the ALREADY-MERGED relational vocabulary `fold_env` computes
/// (dsl 0.3.0 §3/§4, spec §5) — entity kinds, relations (+arity/domains/
/// `derive`), seed facts, rules, and project-level `enums:` — surfaced here
/// verbatim, no new resolution. Reserved quest paths and `occasion.*` are
/// not state the document declares: `stateSchema` leaves them out
/// (`reservedQuestPaths`, `enginePaths` and each occasion's `payload` list
/// them), and marks every other engine-owned path `owner: engine`.
fn authoring_surface(
    input: &CheckInput,
    state: &lute_check::StateSchema,
    rel_vocab: &RelVocab,
    branch_paths: &BTreeSet<String>,
) -> serde_json::Value {
    use serde_json::{Map, Value};
    let snap = &input.snapshot;

    // Directives: BTreeMap key == directive name ⇒ iteration is name-sorted.
    // Attrs keep declaration order (their authoring/positional order).
    let directives: Vec<Value> = snap
        .directives
        .values()
        .filter(|d| snap.permissions.allows_directive(&d.name))
        .filter(|d| {
            d.bridge.as_ref().is_none_or(|bridge| {
                snap.permissions
                    .allows_bridge(&bridge.service, &bridge.operation)
            })
        })
        .map(|d| {
            let attrs: Vec<Value> = d
                .attrs
                .iter()
                .map(|a| {
                    let (ty, domain) = attr_type_str(&a.ty);
                    let mut o = Map::new();
                    o.insert("name".into(), a.name.clone().into());
                    o.insert("type".into(), ty.into());
                    o.insert("required".into(), a.required.into());
                    if let Some(dom) = domain {
                        o.insert("domain".into(), dom.into());
                    }
                    if let Some(def) = &a.default {
                        o.insert("default".into(), literal_json(def));
                    }
                    Value::Object(o)
                })
                .collect();
            let mut o = Map::new();
            o.insert("name".into(), d.name.clone().into());
            if let Some(layer) = &d.layer {
                o.insert("layer".into(), layer.clone().into());
            }
            o.insert("attrs".into(), attrs.into());
            o.insert("semantics".into(), d.semantics.clone().into());
            // plugin §7.4, dsl 0.27.0 §4: what a call writes / asserts / retracts.
            if let Some(effects) = d.effects.as_ref().filter(|e| !e.is_empty()) {
                o.insert(
                    "effects".into(),
                    serde_json::to_value(effects).unwrap_or(Value::Null),
                );
            }
            Value::Object(o)
        })
        .collect();

    // Bridge/reward vocabularies are authoring surfaces, not merely metadata.
    // Keep only entries admitted by the same effective snapshot policy the
    // checker and compiler enforce.
    let bridges: Vec<Value> = snap
        .bridge_capabilities
        .values()
        .filter(|bridge| {
            snap.permissions
                .allows_bridge(&bridge.service, &bridge.operation)
        })
        .map(|bridge| serde_json::to_value(bridge).unwrap_or(Value::Null))
        .collect();
    let reward_kinds = if snap.permissions.allows_rewards() {
        serde_json::to_value(&snap.reward_kinds).unwrap_or_else(|_| serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    // Folded state schema: BTreeMap key == path ⇒ iteration is path-sorted.
    let state_schema: Vec<Value> = state
        .decls
        .iter()
        .filter(|(path, _)| {
            lute_check::cel_paths::reserved_path(path).is_none() && !path.starts_with("occasion.")
        })
        .map(|(path, decl)| {
            // A path folded from a real `<branch>`/`<hub>` is an implicit choice
            // slot: its authorable enum domain is choice ids ∪ `unset` (compile's
            // state-table domain), NOT the folded members alone. Author enums at
            // any other path are not in `branch_paths` and keep their members.
            let (ty, domain) = state_type_str(branch_paths.contains(path), &decl.ty);
            let mut o = Map::new();
            o.insert("path".into(), path.clone().into());
            o.insert("type".into(), ty.into());
            o.insert("namespace".into(), namespace_str(decl.namespace).into());
            // dsl 0.22.0 §1.2: engine-owned paths are read-only to content —
            // declared `owner: engine`, or in a namespace the engine owns
            // (`scene.choices.*`, `clock.*`, `prev.*` …).
            if let Some(owner) = &decl.owner {
                o.insert(
                    "owner".into(),
                    serde_json::to_value(owner).unwrap_or(Value::Null),
                );
            } else if lute_check::cel_paths::is_engine_owned_path(path) {
                o.insert("owner".into(), "engine".into());
            }
            if let Some(def) = &decl.default {
                o.insert("default".into(), literal_json(def));
            }
            if let Some(dom) = domain {
                o.insert("domain".into(), dom.into());
            }
            Value::Object(o)
        })
        .collect();

    // Imported components (dsl §13): BTreeMap key == name ⇒ name-sorted; params
    // keep source (named-arg binding) order. A declared `default:` (dsl 0.26.0
    // §3) rides as `default`, a `@def` default by its name as written.
    let components: Vec<Value> = input
        .components
        .table
        .iter()
        .map(|(name, def)| {
            let params: Vec<Value> = def
                .params
                .iter()
                .map(|(pname, pty)| {
                    // dsl 0.24.0 §4: a `speaker` param is typed `string` in
                    // `params` and named in `speakers`.
                    let (ty, domain) = if def.speakers.contains(pname) {
                        ("speaker".to_string(), None)
                    } else {
                        attr_type_str(pty)
                    };
                    let mut o = Map::new();
                    o.insert("name".into(), pname.clone().into());
                    o.insert("type".into(), ty.into());
                    if let Some(dom) = domain {
                        o.insert("domain".into(), dom.into());
                    }
                    match def.defaults.get(pname) {
                        Some(lute_syntax::ast::AttrValue::Str(s)) => {
                            o.insert("default".into(), s.clone().into());
                        }
                        Some(lute_syntax::ast::AttrValue::Ref(slot)) => {
                            o.insert("default".into(), slot.raw.clone().into());
                        }
                        Some(lute_syntax::ast::AttrValue::BoolTrue) => {
                            o.insert("default".into(), "true".into());
                        }
                        None => {}
                    }
                    Value::Object(o)
                })
                .collect();
            let mut o = Map::new();
            o.insert("name".into(), name.clone().into());
            o.insert("params".into(), params.into());
            if def.effects {
                o.insert("effects".into(), true.into());
            }
            // dsl 0.27.0 §6: a beat template's header (`<beat use="name">`).
            if let Some(t) = &def.beat {
                o.insert("template".into(), true.into());
                let header: Map<String, Value> = t
                    .keys
                    .iter()
                    .map(|k| (k.key.clone(), k.raw.clone().into()))
                    .collect();
                o.insert("beat".into(), header.into());
            }
            Value::Object(o)
        })
        .collect();

    // Entity kinds (dsl 0.3.0 §3.1): BTreeMap key == name ⇒ name-sorted. A
    // closed kind (`members: [...]`) carries its member list; an `open: true`
    // kind carries no member list (any id is legal); `Invalid` (neither/both)
    // is preserved as data (rel_schema.rs's discipline) rather than hidden.
    let entities: Vec<Value> = rel_vocab
        .kinds
        .iter()
        .map(|(name, decl)| {
            let mut o = Map::new();
            o.insert("name".into(), name.clone().into());
            match &decl.shape {
                KindShape::Members(members) => {
                    o.insert("shape".into(), "members".into());
                    o.insert("members".into(), members.clone().into());
                }
                KindShape::Open => {
                    o.insert("shape".into(), "open".into());
                }
                KindShape::Invalid => {
                    o.insert("shape".into(), "invalid".into());
                }
            }
            // dsl 0.27.0 §7: the display text `{{…}}` renders per member.
            if !decl.labels.is_empty() {
                let text: serde_json::Map<String, Value> = decl
                    .labels
                    .iter()
                    .map(|(m, l)| (m.clone(), l.text.clone().into()))
                    .collect();
                o.insert("labels".into(), Value::Object(text));
            }
            Value::Object(o)
        })
        .collect();

    // Relations (dsl 0.3.0 §4): BTreeMap key == name ⇒ name-sorted. `args` is
    // the ordered argument-domain (entity kind or enum) list; `arity` is its
    // length, surfaced explicitly so an AI need not count. `derive: true`
    // marks a Datalog-derived relation (no direct write tier, `tier_of`).
    let relations: Vec<Value> = rel_vocab
        .relations
        .iter()
        .map(|(name, decl)| {
            let mut o = Map::new();
            o.insert("name".into(), name.clone().into());
            o.insert("arity".into(), decl.args.len().into());
            o.insert("args".into(), decl.args.clone().into());
            o.insert("derive".into(), decl.derive.into());
            // The write tier (`run` when unset) and whether only the engine
            // asserts it (`reserved: true`) — a derived relation has neither.
            if !decl.derive {
                o.insert(
                    "tier".into(),
                    decl.tier.clone().unwrap_or_else(|| "run".into()).into(),
                );
            }
            o.insert("reserved".into(), decl.reserved.into());
            Value::Object(o)
        })
        .collect();

    // Seed facts (dsl 0.3.0 §4, D12): raw source text, in declaration order
    // (a `Vec`, not name-keyed — authoring order is meaningful, unlike the
    // name-sorted maps above).
    let facts: Vec<Value> = rel_vocab
        .facts
        .iter()
        .map(|f| Value::String(f.raw.clone()))
        .collect();

    // Rules (dsl 0.3.0 §7.1): raw source text, declaration order.
    let rules: Vec<Value> = rel_vocab
        .rules
        .iter()
        .map(|r| Value::String(r.raw.clone()))
        .collect();

    // dsl 0.5.1 §3: the fixed, always-present set of content-line delivery
    // flags — `{mono}`/`{os}`/`{vo}` — with their normative meanings, in
    // spec declaration order.
    let delivery_flags: Vec<Value> = [
        (
            "mono",
            "interior monologue / thought (not spoken aloud in-scene)",
        ),
        (
            "os",
            "off-screen: the speaker is heard but not currently staged/visible",
        ),
        (
            "vo",
            "voiceover: narration-style delivery layered over the scene",
        ),
    ]
    .into_iter()
    .map(|(flag, meaning)| {
        let mut o = Map::new();
        o.insert("flag".into(), flag.into());
        o.insert("meaning".into(), meaning.into());
        Value::Object(o)
    })
    .collect();

    let mut root = Map::new();
    root.insert("capabilityVersion".into(), snap.version.clone().into());
    root.insert(
        "permissions".into(),
        serde_json::to_value(&snap.permissions)
            .unwrap_or_else(|_| serde_json::json!({ "layers": [] })),
    );
    root.insert("directives".into(), directives.into());
    root.insert("bridges".into(), bridges.into());
    root.insert("rewardKinds".into(), reward_kinds);
    // dsl 0.23.0 §7: the declared cast (plugins ∪ imported schemas) the
    // speakers are checked against; omitted while speakers are shape-only.
    let cast = lute_check::declared_cast(&input.snapshot, &input.imports, &[]);
    if !cast.is_empty() {
        root.insert(
            "cast".into(),
            serde_json::to_value(&cast).unwrap_or_else(|_| serde_json::json!({})),
        );
    }
    // dsl 0.21.0 §2: the occasion vocabulary beats answer with `on:`
    // (empty = shape-only). Key-sorted by the snapshot's BTreeMap.
    root.insert(
        "occasions".into(),
        serde_json::to_value(&snap.occasions).unwrap_or_else(|_| serde_json::json!({})),
    );
    root.insert(
        "questsAllowed".into(),
        snap.permissions.allows_quests().into(),
    );
    // enums/assetKinds/providers are BTreeMaps on the snapshot: their serde-JSON
    // objects are key-sorted by construction. `to_value` is infallible for these
    // concrete shapes; a defensive empty-object fallback keeps the surface total.
    root.insert(
        "enums".into(),
        serde_json::to_value(&snap.enums).unwrap_or_else(|_| serde_json::json!({})),
    );
    root.insert(
        "assetKinds".into(),
        serde_json::to_value(&snap.asset_kinds).unwrap_or_else(|_| serde_json::json!({})),
    );
    root.insert(
        "providers".into(),
        serde_json::to_value(&snap.providers).unwrap_or_else(|_| serde_json::json!({})),
    );
    root.insert("stateSchema".into(), state_schema.into());
    root.insert("components".into(), components.into());
    // Relational vocabulary (dsl 0.3.0 §3/§4, spec §5) — `entities`/`relations`/
    // `facts`/`rules` are new keys; `projectEnums` is the project-level
    // `enums:` (`rel_vocab.enums`), kept under its OWN key so it never
    // clobbers the plugin/core `enums` key above (a distinct vocabulary).
    root.insert("entities".into(), entities.into());
    root.insert("relations".into(), relations.into());
    root.insert("facts".into(), facts.into());
    root.insert("rules".into(), rules.into());
    root.insert(
        "projectEnums".into(),
        serde_json::to_value(&rel_vocab.enums).unwrap_or_else(|_| serde_json::json!({})),
    );
    // dsl 0.5.1 §3: the fixed delivery-flag vocabulary.
    root.insert("deliveryFlags".into(), delivery_flags.into());
    Value::Object(root)
}

/// Render a state-path `Type` for parity with `lute_compile`'s `type_label`
/// (dsl §4.1): scalars + `enum`(+members); id-flavored types collapse to their
/// value-level label (`string`/`enum`) exactly as the compiled artifact's state
/// table does. `is_implicit` (path ∈ `collect_branch_paths`) marks a REAL
/// `<branch>`/`<hub>` choice slot: its enum domain is choice ids ∪ `unset` — the
/// author must write `<when is="unset">` for the pre-choice state — appended LAST,
/// byte-identical to `type_label(true, …)`. A plain author enum (`is_implicit ==
/// false`) keeps its folded members as the authorable domain, no `unset`.
fn state_type_str(is_implicit: bool, ty: &Type) -> (String, Option<Vec<String>>) {
    match ty {
        Type::Bool => ("bool".to_string(), None),
        Type::Number => ("number".to_string(), None),
        Type::Str => ("string".to_string(), None),
        Type::Enum(members) => {
            let mut domain = members.clone();
            if is_implicit {
                domain.push("unset".to_string());
            }
            ("enum".to_string(), Some(domain))
        }
        Type::List(_) => ("list".to_string(), None),
        Type::Record(_) => ("record".to_string(), None),
        Type::Map { .. } => ("map".to_string(), None),
        Type::EnumFromOption(_) => ("enum".to_string(), None),
        Type::ProviderRef(_)
        | Type::Domain(_)
        | Type::Entity(_)
        | Type::SlotId { .. }
        | Type::AssetKind(_) => ("string".to_string(), None),
        Type::NarrativeTime => ("narrativeTime".to_string(), None),
    }
}

/// Render an attr/param `Type` for the AUTHORING surface. The base labels match
/// `type_label` (`bool`/`number`/`string`/`enum`), but reference-bearing types
/// keep their target so an AI knows WHAT an id resolves against —
/// `providerRef:<catalog>`, `assetKind:<kind>`, `slotId:<namespace>`,
/// `enumFromOption:<option>` — and compound types name their element(s)
/// (`list<T>`, `map<K,V>`, `record`). An `enum` also carries its member domain.
pub(crate) fn attr_type_str(ty: &Type) -> (String, Option<Vec<String>>) {
    match ty {
        Type::Bool => ("bool".to_string(), None),
        Type::Number => ("number".to_string(), None),
        Type::Str => ("string".to_string(), None),
        Type::Enum(members) => ("enum".to_string(), Some(members.clone())),
        Type::List(inner) => (format!("list<{}>", attr_type_str(inner).0), None),
        Type::Record(_) => ("record".to_string(), None),
        Type::Map { key, value } => (
            format!("map<{},{}>", attr_type_str(key).0, attr_type_str(value).0),
            None,
        ),
        Type::EnumFromOption(opt) => (format!("enumFromOption:{opt}"), None),
        Type::ProviderRef(name) => (format!("providerRef:{name}"), None),
        Type::Domain(name) => (format!("domain:{name}"), None),
        Type::Entity(kind) => (format!("entity:{kind}"), None),
        Type::SlotId { namespace } => (format!("slotId:{namespace}"), None),
        Type::AssetKind(name) => (format!("assetKind:{name}"), None),
        Type::NarrativeTime => ("narrativeTime".to_string(), None),
    }
}

/// The state lifetime tier (dsl §9.1) as a lowercase string — tells an AI which
/// namespace a state path belongs to (`scene`/`run`/`user`/`app`).
fn namespace_str(ns: Namespace) -> &'static str {
    match ns {
        Namespace::Scene => "scene",
        Namespace::Run => "run",
        Namespace::User => "user",
        Namespace::App => "app",
        Namespace::Quest => "quest",
        Namespace::Season => "season",
    }
}

/// Manifest `Literal` → JSON, mirroring `lute_compile`'s `literal_json`: an
/// integral float collapses to a JSON integer (`0`, not `0.0`) for a stable
/// authoring surface consistent with the compiled envelope.
fn literal_json(l: &Literal) -> serde_json::Value {
    match l {
        Literal::Bool(b) => serde_json::Value::Bool(*b),
        Literal::Num(n) if n.fract() == 0.0 && n.is_finite() && n.abs() < 9.0e15 => {
            serde_json::Value::from(*n as i64)
        }
        Literal::Num(n) => serde_json::Value::from(*n),
        Literal::Str(s) => serde_json::Value::String(s.clone()),
        Literal::List(xs) => serde_json::Value::Array(xs.iter().map(literal_json).collect()),
        Literal::Map(m) => serde_json::Value::Object(
            m.iter()
                .map(|(k, v)| (k.clone(), literal_json(v)))
                .collect(),
        ),
    }
}

/// A compact human outline of the authoring surface (non-`--json` mode): the
/// capabilityVersion, directive names + attr keys + semantics flags, enum
/// names WITH their members, state paths (with enum domains), the referenced
/// reserved quest paths (dsl 0.5.1 §2), the relational vocabulary (entity
/// kinds, relations w/ arity+domains+`derive`, seed facts, rules,
/// project-level enums), the fixed delivery-flag vocabulary (dsl 0.5.1 §3),
/// and component names. `--json` is the machine surface; this is a short
/// at-a-glance view.
fn context_outline(surface: &serde_json::Value) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "capabilityVersion: {}",
        surface["capabilityVersion"].as_str().unwrap_or("")
    );
    let permissions = serde_json::to_string(&surface["permissions"])
        .unwrap_or_else(|_| "{\"layers\":[]}".to_string());
    let _ = writeln!(
        out,
        "permissions: {permissions} (authoring/compile-time restrictions; not runtime sandbox enforcement)"
    );
    if let Some(dirs) = surface["directives"].as_array() {
        let _ = writeln!(out, "directives ({}):", dirs.len());
        for d in dirs {
            let name = d["name"].as_str().unwrap_or("");
            let layer = d["layer"]
                .as_str()
                .map(|l| format!(" [{l}]"))
                .unwrap_or_default();
            // The attribute types the checker holds each value to.
            let attrs: Vec<String> = d["attrs"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .map(|x| {
                            let dom = x["domain"]
                                .as_array()
                                .map(|d| {
                                    let m: Vec<&str> =
                                        d.iter().filter_map(|v| v.as_str()).collect();
                                    format!("[{}]", m.join(", "))
                                })
                                .unwrap_or_default();
                            let req = if x["required"].as_bool().unwrap_or(false) {
                                " (required)"
                            } else {
                                ""
                            };
                            format!(
                                "{}: {}{dom}{req}",
                                x["name"].as_str().unwrap_or(""),
                                x["type"].as_str().unwrap_or("")
                            )
                        })
                        .collect()
                })
                .unwrap_or_default();
            // #32 / T2.5: `--json` has always carried these and the human
            // outline dropped them. `mayExitCharacter` is the machine-readable
            // statement that `::auto` is the construct that ends a presence,
            // and it is on no page of the shipped website.
            let semantics: Vec<&str> = d["semantics"]
                .as_array()
                .map(|a| a.iter().filter_map(|x| x.as_str()).collect())
                .unwrap_or_default();
            let sem = if semantics.is_empty() {
                String::new()
            } else {
                format!("   [{}]", semantics.join(" "))
            };
            let _ = writeln!(out, "  {name}{layer}: {}{sem}", attrs.join(", "));
            let effects = &d["effects"];
            let mut parts = Vec::new();
            for w in effects["writes"].as_array().into_iter().flatten() {
                let mut path = vec![w["scope"].as_str().unwrap_or("").to_string()];
                for seg in w["path"].as_array().into_iter().flatten() {
                    path.push(match seg.as_str() {
                        Some(s) => s.to_string(),
                        None => format!("<{seg}>"),
                    });
                }
                parts.push(format!("writes {} = {}", path.join("."), w["value"]));
            }
            for (key, verb) in [("asserts", "asserts"), ("retracts", "retracts")] {
                for f in effects[key].as_array().into_iter().flatten() {
                    parts.push(format!("{verb} {}", f.as_str().unwrap_or("")));
                }
            }
            if !parts.is_empty() {
                let _ = writeln!(out, "    effects: {}", parts.join("; "));
            }
        }
    }
    if let Some(bridges) = surface["bridges"].as_array() {
        let _ = writeln!(out, "bridges ({}):", bridges.len());
        for bridge in bridges {
            let service = bridge["service"].as_str().unwrap_or("");
            let operation = bridge["operation"].as_str().unwrap_or("");
            let _ = writeln!(out, "  {service}/{operation}");
        }
    }
    if let Some(cast) = surface.get("cast").and_then(serde_json::Value::as_object) {
        let _ = writeln!(out, "cast ({}):", cast.len());
        for (id, m) in cast {
            match m["name"].as_str() {
                Some(name) => {
                    let _ = writeln!(out, "  {id} — {name}");
                }
                None => {
                    let _ = writeln!(out, "  {id}");
                }
            }
        }
    }
    if let Some(reward_kinds) = surface["rewardKinds"].as_object() {
        let _ = writeln!(out, "rewardKinds ({}):", reward_kinds.len());
        for name in reward_kinds.keys() {
            let _ = writeln!(out, "  {name}");
        }
    }
    if let Some(occasions) = surface["occasions"].as_object() {
        let _ = writeln!(out, "occasions ({}):", occasions.len());
        for (name, o) in occasions {
            let select = o["select"].as_str().unwrap_or("first");
            // dsl 0.22.0 §8: a domain target names its vocabulary.
            let target = match &o["target"] {
                serde_json::Value::Bool(true) => ", target".to_string(),
                serde_json::Value::Object(d) => format!(
                    ", target: {}.<{}>",
                    d.get("prefix").and_then(|v| v.as_str()).unwrap_or(""),
                    d.get("entity").and_then(|v| v.as_str()).unwrap_or("")
                ),
                _ => String::new(),
            };
            // dsl 0.27.0 §3/§4: the typed payload and the gate.
            let mut extra = String::new();
            if let Some(payload) = o["payload"].as_object() {
                // Payload, not state: each field is read as
                // `occasion.payload.<field>` in a beat answering it.
                let fields: Vec<String> = payload
                    .iter()
                    .map(|(f, ty)| match ty.as_str() {
                        Some(t) => format!("occasion.payload.{f}: {t}"),
                        None => format!("occasion.payload.{f}: {ty}"),
                    })
                    .collect();
                let _ = write!(extra, ", payload: {{ {} }}", fields.join(", "));
            }
            if let Some(gate) = o["raisedWhen"].as_str() {
                let _ = write!(extra, ", raisedWhen: {gate}");
            }
            // dsl 0.24.0 §2: `on=` objectives judged before the beats.
            if let Some(judge) = o["judge"].as_str() {
                let _ = write!(extra, ", judge: {judge}");
            }
            if o["outsideRun"].as_bool() == Some(true) {
                extra.push_str(", outsideRun");
            }
            let description = o["description"]
                .as_str()
                .map(|d| format!(" — {d}"))
                .unwrap_or_default();
            let _ = writeln!(
                out,
                "  {name} (select: {select}{target}{extra}){description}"
            );
        }
    }
    let _ = writeln!(
        out,
        "questsAllowed: {}",
        surface["questsAllowed"].as_bool().unwrap_or(true)
    );
    if let Some(enums) = surface["enums"].as_object() {
        // Members, not just names (spec §5) — an author choosing an
        // `emotion="…"` value sees the legal set without `--json`.
        let _ = writeln!(out, "enums ({}):", enums.len());
        for (name, members) in enums {
            let member_strs: Vec<&str> = members
                .as_array()
                .map(|a| a.iter().filter_map(|x| x.as_str()).collect())
                .unwrap_or_default();
            let _ = writeln!(out, "  {name}: {}", member_strs.join(", "));
        }
    }
    if let Some(state) = surface["stateSchema"].as_array() {
        let _ = writeln!(out, "stateSchema ({}):", state.len());
        for s in state {
            let path = s["path"].as_str().unwrap_or("");
            let ty = s["type"].as_str().unwrap_or("");
            let dom = s["domain"]
                .as_array()
                .map(|d| {
                    let members: Vec<&str> = d.iter().filter_map(|x| x.as_str()).collect();
                    format!(" [{}]", members.join(", "))
                })
                .unwrap_or_default();
            let owner = s["owner"]
                .as_str()
                .map(|o| format!(" (owner: {o})"))
                .unwrap_or_default();
            let _ = writeln!(out, "  {path}: {ty}{dom}{owner}");
        }
    }
    // Every reserved path of every declared (or read) quest, typed by the
    // checker's reserved-path table — never part of `stateSchema`.
    if let Some(reserved) = surface["reservedQuestPaths"].as_array() {
        if !reserved.is_empty() {
            let _ = writeln!(
                out,
                "reservedQuestPaths ({}; engine-owned, read-only):",
                reserved.len()
            );
            for s in reserved {
                let path = s["path"].as_str().unwrap_or("");
                let ty = s["type"].as_str().unwrap_or("");
                let dom = s["domain"]
                    .as_array()
                    .map(|d| {
                        let members: Vec<&str> = d.iter().filter_map(|x| x.as_str()).collect();
                        format!(" [{}]", members.join(", "))
                    })
                    .unwrap_or_default();
                let _ = writeln!(out, "  {path}: {ty}{dom}");
            }
        }
    }
    // dsl 0.5.1 §3: the fixed `{mono}`/`{os}`/`{vo}` delivery-flag
    // vocabulary — always present (a document either uses a flag or
    // doesn't; the set itself is fixed and never varies per document).
    if let Some(flags) = surface["deliveryFlags"].as_array() {
        let _ = writeln!(out, "deliveryFlags ({}):", flags.len());
        for f in flags {
            let flag = f["flag"].as_str().unwrap_or("");
            let meaning = f["meaning"].as_str().unwrap_or("");
            let _ = writeln!(out, "  {{{flag}}}: {meaning}");
        }
    }
    // Relational vocabulary (dsl 0.3.0 §3/§4, spec §5): entity kinds,
    // relations (name/arity/domains/derive), seed facts, rules, and the
    // project-level `enums:` — kept separate from the plugin/core `enums`
    // block above.
    if let Some(entities) = surface["entities"].as_array() {
        if !entities.is_empty() {
            let _ = writeln!(out, "entities ({}):", entities.len());
            for e in entities {
                let name = e["name"].as_str().unwrap_or("");
                let shape = e["shape"].as_str().unwrap_or("");
                if shape == "members" {
                    // dsl 0.27.0 §7: a labelled member shows its display text.
                    let members: Vec<String> = e["members"]
                        .as_array()
                        .map(|a| {
                            a.iter()
                                .filter_map(|x| x.as_str())
                                .map(|m| match e["labels"][m].as_str() {
                                    Some(label) => format!("{m} ({label:?})"),
                                    None => m.to_string(),
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    let _ = writeln!(out, "  {name}: {}", members.join(", "));
                } else {
                    let _ = writeln!(out, "  {name}: {shape}");
                }
            }
        }
    }
    if let Some(relations) = surface["relations"].as_array() {
        if !relations.is_empty() {
            let _ = writeln!(out, "relations ({}):", relations.len());
            for r in relations {
                let name = r["name"].as_str().unwrap_or("");
                let arity = r["arity"].as_u64().unwrap_or(0);
                let args: Vec<&str> = r["args"]
                    .as_array()
                    .map(|a| a.iter().filter_map(|x| x.as_str()).collect())
                    .unwrap_or_default();
                // `[derive]`, or the write tier plus `reserved` (engine-asserted).
                let tag = if r["derive"].as_bool().unwrap_or(false) {
                    " [derive]".to_string()
                } else {
                    let tier = r["tier"].as_str().unwrap_or("run");
                    if r["reserved"].as_bool().unwrap_or(false) {
                        format!(" [{tier}, reserved]")
                    } else {
                        format!(" [{tier}]")
                    }
                };
                let _ = writeln!(out, "  {name}/{arity}({}){tag}", args.join(", "));
            }
        }
    }
    if let Some(facts) = surface["facts"].as_array() {
        if !facts.is_empty() {
            let _ = writeln!(out, "facts ({}):", facts.len());
            for f in facts {
                let _ = writeln!(out, "  {}", f.as_str().unwrap_or(""));
            }
        }
    }
    if let Some(rules) = surface["rules"].as_array() {
        if !rules.is_empty() {
            let _ = writeln!(out, "rules ({}):", rules.len());
            for r in rules {
                let _ = writeln!(out, "  {}", r.as_str().unwrap_or(""));
            }
        }
    }
    if let Some(penums) = surface["projectEnums"].as_object() {
        if !penums.is_empty() {
            let _ = writeln!(out, "projectEnums ({}):", penums.len());
            for (name, members) in penums {
                let member_strs: Vec<&str> = members
                    .as_array()
                    .map(|a| a.iter().filter_map(|x| x.as_str()).collect())
                    .unwrap_or_default();
                let _ = writeln!(out, "  {name}: {}", member_strs.join(", "));
            }
        }
    }
    if let Some(comps) = surface["components"].as_array() {
        if !comps.is_empty() {
            // Signatures, not just names: `::use` binds these by name.
            let _ = writeln!(out, "components ({}):", comps.len());
            for c in comps {
                let params: Vec<String> = c["params"]
                    .as_array()
                    .map(|ps| {
                        ps.iter()
                            .map(|p| {
                                let dom = p["domain"]
                                    .as_array()
                                    .map(|d| {
                                        let m: Vec<&str> =
                                            d.iter().filter_map(|x| x.as_str()).collect();
                                        format!("[{}]", m.join(", "))
                                    })
                                    .unwrap_or_default();
                                // `= <default>`: a string default quoted,
                                // anything else (enum member, number, `@def`)
                                // as written.
                                let default = match p["default"].as_str() {
                                    Some(d) if p["type"] == "string" && !d.starts_with('@') => {
                                        format!(" = {d:?}")
                                    }
                                    Some(d) => format!(" = {d}"),
                                    None => String::new(),
                                };
                                format!(
                                    "{}: {}{dom}{default}",
                                    p["name"].as_str().unwrap_or(""),
                                    p["type"].as_str().unwrap_or("")
                                )
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let mut tags = Vec::new();
                if c["template"].as_bool() == Some(true) {
                    tags.push(format!(
                        "template: <beat use=\"{}\">",
                        c["name"].as_str().unwrap_or("")
                    ));
                }
                if c["effects"].as_bool() == Some(true) {
                    tags.push("effects: its body may write state".to_string());
                }
                let tags = if tags.is_empty() {
                    String::new()
                } else {
                    format!("   [{}]", tags.join("; "))
                };
                let _ = writeln!(
                    out,
                    "  {}({}){tags}",
                    c["name"].as_str().unwrap_or(""),
                    params.join(", ")
                );
                // dsl 0.27.0 §6: a beat template's header, for `<beat use>`.
                if let Some(header) = c["beat"].as_object() {
                    let keys: Vec<String> = header
                        .iter()
                        .map(|(k, v)| format!("{k}: {}", v.as_str().unwrap_or("")))
                        .collect();
                    let _ = writeln!(out, "    beat: {}", keys.join(", "));
                }
            }
        }
    }
    context::outline_extras(&mut out, surface);
    out
}
